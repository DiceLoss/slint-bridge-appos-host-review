//! Optional GE normal-mode fills on an offscreen RGBA canvas.
//! Display admission is independent and remains closed. No GE job targets either
//! scanout page. Transparent/rounded/textured drawing retains Slint semantics.
use crate::Error;
use i_slint_renderer_software::{
    DrawRectangleArgs, PhysicalRegion, PremultipliedRgbaColor, SoftwareRenderer, TargetPixelBuffer,
};
use slint::Brush;

#[cfg(target_arch = "riscv64")]
mod native;

#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(target_arch = "riscv64"), allow(dead_code))] // Coordinates are consumed by native GE.
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

// Native owns the DMA mapping. No Rust pixel slice exists during fill(). This
// seam also lets host regressions exercise an uncertain hardware result.
trait Canvas {
    fn pixels(&self) -> &[PremultipliedRgbaColor];
    fn pixels_mut(&mut self) -> &mut [PremultipliedRgbaColor];
    fn fill(&mut self, _color: PremultipliedRgbaColor, _rects: &[Rect]) -> Result<bool, String> {
        Ok(false)
    }
}

impl Canvas for Vec<PremultipliedRgbaColor> {
    fn pixels(&self) -> &[PremultipliedRgbaColor] {
        self
    }
    fn pixels_mut(&mut self) -> &mut [PremultipliedRgbaColor] {
        self
    }
}

pub(crate) struct Buffer {
    canvas: Box<dyn Canvas>,
    width: u32,
    height: u32,
    failed: Option<String>,
    discard: Vec<PremultipliedRgbaColor>,
}

fn allocate(count: usize) -> Result<Vec<PremultipliedRgbaColor>, Error> {
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count)
        .map_err(|_| Error::UnsupportedFramebuffer("F1 canvas allocation failed"))?;
    pixels.resize(count, PremultipliedRgbaColor::default());
    Ok(pixels)
}

impl Buffer {
    pub(crate) fn new(width: u32, height: u32) -> Result<Self, Error> {
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(Error::UnsupportedFramebuffer("F1 canvas size overflow"))?;
        let discard = allocate(width as usize)?;
        #[cfg(all(target_arch = "riscv64", not(test)))]
        let canvas: Box<dyn Canvas> = match native::Native::new(width, height) {
            Ok(canvas) => Box::new(canvas),
            Err(error) => {
                // No hardware job was submitted during construction.
                eprintln!("slint-bridge: F1 GE unavailable ({error}); software canvas");
                Box::new(allocate(count)?)
            }
        };
        // Ordinary unit tests use simulated surfaces, including on RISC-V.
        #[cfg(any(not(target_arch = "riscv64"), test))]
        let canvas: Box<dyn Canvas> = Box::new(allocate(count)?);
        Ok(Self {
            canvas,
            width,
            height,
            failed: None,
            discard,
        })
    }

    pub(crate) fn pixels(&self) -> &[PremultipliedRgbaColor] {
        assert!(self.failed.is_none(), "F1 canvas is not CPU-owned");
        self.canvas.pixels()
    }

    pub(crate) fn render(&mut self, renderer: &SoftwareRenderer) -> Result<PhysicalRegion, Error> {
        if let Some(error) = &self.failed {
            return Err(Error::AccelerationFailed(error.clone()));
        }
        let region = renderer.render_into_buffer(self);
        match &self.failed {
            Some(error) => Err(Error::AccelerationFailed(error.clone())),
            None => Ok(region),
        }
    }

    fn fill(&mut self, color: PremultipliedRgbaColor, rects: &[Rect]) -> bool {
        if self.failed.is_some() {
            // The frame is already invalid; do not call the engine again.
            return true;
        }
        if color.alpha != 255
            || rects.is_empty()
            || rects.len() > 16
            || rects
                .iter()
                .map(|r| u64::from(r.width) * u64::from(r.height))
                .sum::<u64>()
                < 32768
        {
            return false;
        }
        match self.canvas.fill(color, rects) {
            Ok(done) => done,
            Err(error) => {
                // Even a completed earlier rect may have changed the canvas.
                // Abort this frame; never reinterpret a failed job as fallback.
                self.failed = Some(error);
                true
            }
        }
    }

    fn clipped(
        &self,
        bounds: (i64, i64, i64, i64),
        clip: impl Iterator<Item = (slint::PhysicalPosition, slint::PhysicalSize)>,
    ) -> Vec<Rect> {
        clip.filter_map(|(p, s)| {
            let x = bounds.0.max(i64::from(p.x)).max(0);
            let y = bounds.1.max(i64::from(p.y)).max(0);
            let right = bounds
                .2
                .min(i64::from(p.x) + i64::from(s.width))
                .min(i64::from(self.width));
            let bottom = bounds
                .3
                .min(i64::from(p.y) + i64::from(s.height))
                .min(i64::from(self.height));
            (x < right && y < bottom).then_some(Rect {
                x: x as u32,
                y: y as u32,
                width: (right - x) as u32,
                height: (bottom - y) as u32,
            })
        })
        .collect()
    }
}

impl TargetPixelBuffer for Buffer {
    type TargetPixel = PremultipliedRgbaColor;

    fn num_lines(&self) -> usize {
        self.height as usize
    }

    fn line_slice(&mut self, line: usize) -> &mut [Self::TargetPixel] {
        if self.failed.is_some() {
            return &mut self.discard;
        }
        let start = line * self.width as usize;
        &mut self.canvas.pixels_mut()[start..start + self.width as usize]
    }

    fn fill_background(&mut self, brush: &Brush, clip: &PhysicalRegion) -> bool {
        let Brush::SolidColor(color) = brush else {
            return false;
        };
        let rects = self.clipped((0, 0, self.width.into(), self.height.into()), clip.iter());
        self.fill((*color).into(), &rects)
    }

    fn draw_rectangle(&mut self, args: &DrawRectangleArgs, clip: &PhysicalRegion) -> bool {
        let Brush::SolidColor(color) = args.background else {
            return false;
        };
        if args.alpha != 255
            || color.alpha() != 255
            || args.border_width != 0.
            || [
                args.top_left_radius,
                args.top_right_radius,
                args.bottom_left_radius,
                args.bottom_right_radius,
            ]
            .iter()
            .any(|r| *r != 0.)
            || [args.x, args.y, args.width, args.height]
                .iter()
                .any(|v| !v.is_finite() || v.fract() != 0. || v.abs() > i32::MAX as f32)
            || args.width <= 0.
            || args.height <= 0.
        {
            return false;
        }
        let x = args.x as i64;
        let y = args.y as i64;
        let rects = self.clipped(
            (x, y, x + args.width as i64, y + args.height as i64),
            clip.iter(),
        );
        self.fill(color.into(), &rects)
    }
}

#[cfg(test)]
mod tests;
