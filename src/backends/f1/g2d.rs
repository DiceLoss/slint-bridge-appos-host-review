//! GE normal-mode fills and bounded image blends on an offscreen RGBA canvas.
//! No GE job targets either scanout page. Unscaled images use direct CPU rows
//! or bounded GE blends; other textures and rounded shapes keep Slint rendering.
use crate::Error;
use i_slint_renderer_software::{
    DrawRectangleArgs, DrawTextureArgs, PhysicalRegion, PremultipliedRgbaColor, RenderingRotation,
    SoftwareRenderer, TargetPixelBuffer, TextureCacheKey, TexturePixelFormat,
};
use slint::Brush;

// Two lazy 512 KiB source slots; no full-screen source duplication.
const SOURCE_PIXELS: usize = 128 * 1024;

fn source_pixel_stride(width: u32, height: u32) -> Option<usize> {
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return None;
    }
    // GE requires an eight-byte source stride, including odd RGBA widths.
    let stride = (width as usize + 1) & !1;
    (stride * height as usize <= SOURCE_PIXELS).then_some(stride)
}

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
    fn texture(
        &mut self,
        _args: &DrawTextureArgs,
        _rects: &[Rect],
        _slot: usize,
        _upload: bool,
    ) -> Result<bool, String> {
        Ok(false)
    }
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
    // Retain only the full-canvas immutable source identity, never copied pixels.
    opaque_background: Option<(TextureCacheKey, bool)>,
    background_seen: bool,
    texture_keys: [Option<(TextureCacheKey, u8)>; 2],
    texture_frames: [u64; 2],
    frame: u64,
    #[cfg(test)]
    texture_hits: usize,
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
            opaque_background: None,
            background_seen: false,
            texture_keys: [None, None],
            texture_frames: [0; 2],
            frame: 0,
            #[cfg(test)]
            texture_hits: 0,
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
        self.frame += 1;
        self.background_seen = false;
        let region = renderer.render_into_buffer(self);
        for slot in 0..2 {
            if self.texture_frames[slot] != self.frame {
                self.texture_keys[slot] = None;
            }
        }
        if !self.background_seen {
            self.opaque_background = None;
        }
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

    fn draw_texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        if self.failed.is_some() {
            return true;
        }
        let source = args.source();
        if !matches!(
            source.pixel_format,
            TexturePixelFormat::Rgba | TexturePixelFormat::RgbaPremultiplied
        ) || args.rotation != RenderingRotation::NoRotation
            || args.colorize.is_some()
            || args.tiling.is_some()
            || args.dst_width != source.width as usize
            || args.dst_height != source.height as usize
            || source.width == 0
            || source.height == 0
        {
            return crate::texture_renderer::draw(
                args,
                clip,
                self.canvas.pixels_mut(),
                self.width as usize,
            );
        }
        let row_bytes = source.width as usize * 4;
        if source.byte_stride < row_bytes
            || (source.height as usize - 1)
                .checked_mul(source.byte_stride)
                .and_then(|n| n.checked_add(row_bytes))
                .is_none_or(|n| n > source.data.len())
        {
            return false;
        }
        let (x, y) = (args.dst_x as i64, args.dst_y as i64);
        let Some(right) = x.checked_add(i64::from(source.width)) else {
            return false;
        };
        let Some(bottom) = y.checked_add(i64::from(source.height)) else {
            return false;
        };
        let rects = self.clipped((x, y, right, bottom), clip.iter());
        let mut opaque = false;
        if args.alpha == 255
            && x == 0
            && y == 0
            && source.width == self.width
            && source.height == self.height
            && let Some(key) = args.source_cache_key()
        {
            self.background_seen = true;
            if self
                .opaque_background
                .as_ref()
                .is_none_or(|entry| entry.0 != key)
            {
                let is_opaque = (0..source.height as usize).all(|y| {
                    source.data[y * source.byte_stride..y * source.byte_stride + row_bytes]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| p[3] == 255)
                });
                self.opaque_background = Some((key, is_opaque));
            }
            opaque = self.opaque_background.as_ref().unwrap().1;
        }
        let area = rects
            .iter()
            .map(|r| u64::from(r.width) * u64::from(r.height))
            .sum::<u64>();
        if !opaque
            && source_pixel_stride(source.width, source.height).is_some()
            && area >= 32768
            && !rects.is_empty()
            && area / rects.len() as u64 >= 16384
            && let Some(key) = args.source_cache_key()
        {
            let key = (key, args.alpha);
            let cached = self
                .texture_keys
                .iter()
                .position(|k| k.as_ref() == Some(&key));
            let slot = cached.unwrap_or_else(|| {
                if self.texture_frames[0] <= self.texture_frames[1] {
                    0
                } else {
                    1
                }
            });
            match self.canvas.texture(args, &rects, slot, cached.is_none()) {
                Ok(true) => {
                    self.texture_keys[slot] = Some(key);
                    self.texture_frames[slot] = self.frame;
                    #[cfg(test)]
                    {
                        self.texture_hits += 1;
                    }
                    return true;
                }
                Ok(false) => {
                    self.texture_keys[slot] = None;
                }
                Err(error) => {
                    self.failed = Some(error);
                    return true;
                }
            }
        }
        let pixels = self.canvas.pixels_mut();
        for rect in rects {
            for dy in rect.y..rect.y + rect.height {
                let at = (i64::from(dy) - y) as usize * source.byte_stride
                    + (i64::from(rect.x) - x) as usize * 4;
                let src = &source.data[at..at + rect.width as usize * 4];
                let start = dy as usize * self.width as usize + rect.x as usize;
                let dst = &mut pixels[start..start + rect.width as usize];
                if opaque {
                    // SAFETY: repr(C) RGBA consists of four initialized bytes.
                    // Source image and exclusive canvas do not overlap; both
                    // byte ranges are checked above. Byte copy permits any alignment.
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            src.as_ptr(),
                            dst.as_mut_ptr().cast::<u8>(),
                            src.len(),
                        );
                    }
                } else {
                    let mut samples = src.as_chunks::<4>().0.iter();
                    crate::texture_renderer::blend_sampled_slice(dst, || {
                        let src = samples.next().unwrap();
                        let scale = |v: u8| (u16::from(v) * u16::from(args.alpha) / 255) as u8;
                        if source.pixel_format == TexturePixelFormat::RgbaPremultiplied {
                            PremultipliedRgbaColor {
                                red: scale(src[0]),
                                green: scale(src[1]),
                                blue: scale(src[2]),
                                alpha: scale(src[3]),
                            }
                        } else {
                            PremultipliedRgbaColor::from(slint::Color::from_argb_u8(
                                scale(src[3]),
                                src[0],
                                src[1],
                                src[2],
                            ))
                        }
                    });
                }
            }
        }
        #[cfg(test)]
        {
            self.texture_hits += 1;
        }
        true
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
