//! Optional texture offload. CPU pixels are authoritative; MMZ is private scratch.
use crate::Error;
use i_slint_renderer_software::{
    DrawTextureArgs, PhysicalRegion, PremultipliedRgbaColor, RenderingRotation, SoftwareRenderer,
    TargetPixelBuffer, TextureCacheKey, TexturePixelFormat,
};

#[cfg(target_arch = "arm")]
pub(crate) unsafe fn rgba_to_bgra(source: *const u8, destination: *mut u8, count: usize) -> bool {
    unsafe extern "C" {
        fn bridge_neon_available() -> i32;
        fn bridge_rgba_to_bgra(destination: *mut u8, source: *const u8, count: usize);
    }
    // SAFETY: caller guarantees disjoint, valid count*4 byte ranges. The C
    // implementation accepts unaligned rows and handles the scalar remainder.
    unsafe {
        if bridge_neon_available() == 0 {
            return false;
        }
        bridge_rgba_to_bgra(destination, source, count);
    }
    true
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    sx: i32,
    sy: i32,
    dx: i32,
    dy: i32,
    width: u32,
    height: u32,
}

fn clipped(
    dst: (i64, i64, u32, u32),
    clip: (i32, i32, u32, u32),
    size: (u32, u32),
) -> Option<Rect> {
    let (x, y, w, h) = dst;
    let left = x.max(clip.0 as i64).max(0);
    let top = y.max(clip.1 as i64).max(0);
    let right = (x + w as i64)
        .min(clip.0 as i64 + clip.2 as i64)
        .min(size.0 as i64);
    let bottom = (y + h as i64)
        .min(clip.1 as i64 + clip.3 as i64)
        .min(size.1 as i64);
    (right > left && bottom > top).then(|| Rect {
        sx: (left - x) as i32,
        sy: (top - y) as i32,
        dx: left as i32,
        dy: top as i32,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

#[cfg_attr(not(target_arch = "arm"), allow(dead_code))] // FFI fields are consumed on ARM.
struct Texture<'a> {
    data: &'a [u8],
    width: u32,
    height: u32,
    stride: u32,
    premultiplied: bool,
    slot: u32,
    upload: bool,
    rects: &'a [Rect],
}
#[derive(Debug, Clone, Copy)]
struct Failure {
    code: i32,
    unknown: bool,
}
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "arm"), allow(dead_code))] // Native return values on ARM.
enum Outcome {
    Copied,
    Blended,
    Unsupported,
}
trait Accelerator {
    fn draw(
        &mut self,
        pixels: &mut [PremultipliedRgbaColor],
        texture: &Texture<'_>,
    ) -> Result<Outcome, Failure>;
}

#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct Stats {
    pub copied: u64,
    pub blended: u64,
    pub software: u64,
    pub cache_hits: u64,
    pub errors: u64,
}

pub(crate) struct Buffer {
    pixels: Vec<PremultipliedRgbaColor>,
    width: u32,
    height: u32,
    engine: Option<Box<dyn Accelerator>>,
    keys: [Option<TextureCacheKey>; 2],
    next_slot: usize,
    fatal: Option<Failure>,
    pub stats: Stats,
    frames: u64,
    trace: bool,
}

impl Buffer {
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        let mut buffer = Self::software(width, height)?;
        #[cfg(target_arch = "arm")]
        match native::Native::new(width, height) {
            Ok(engine) => {
                buffer.engine = Some(Box::new(engine));
                eprintln!(
                    "slint-bridge: TDE enabled; private MMZ target + two source slots; CPU fallback"
                );
            }
            Err(code) => eprintln!("slint-bridge: TDE unavailable ({code}); software rendering"),
        }
        // Host builds keep the identical callback/fallback path without the SDK.
        buffer.trace = std::env::var_os("SLINT_BRIDGE_TRACE").is_some();
        Ok(buffer)
    }
    pub fn software(width: u32, height: u32) -> Result<Self, Error> {
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(Error::UnsupportedFramebuffer("TDE canvas size overflow"))?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(|_| Error::UnsupportedFramebuffer("TDE canvas allocation failed"))?;
        pixels.resize(count, PremultipliedRgbaColor::default());
        Ok(Self {
            pixels,
            width,
            height,
            engine: None,
            keys: [None, None],
            next_slot: 0,
            fatal: None,
            stats: Stats::default(),
            frames: 0,
            trace: false,
        })
    }
    pub fn pixels(&self) -> &[PremultipliedRgbaColor] {
        &self.pixels
    }
    pub fn render(&mut self, renderer: &SoftwareRenderer) -> Result<PhysicalRegion, Error> {
        if let Some(error) = self.fatal {
            return Err(Error::AccelerationFailed(format!(
                "SDK code {}",
                error.code
            )));
        }
        let region = renderer.render_into_buffer(self);
        if let Some(error) = self.fatal {
            return Err(Error::AccelerationFailed(format!(
                "SDK code {}; MMZ quarantined",
                error.code
            )));
        }
        self.frames += 1;
        if self.trace && self.frames.is_multiple_of(150) {
            eprintln!(
                "slint-bridge: tde frames={} copy={} blend={} software={} cache_hits={} errors={}",
                self.frames,
                self.stats.copied,
                self.stats.blended,
                self.stats.software,
                self.stats.cache_hits,
                self.stats.errors
            );
        }
        Ok(region)
    }

    fn texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        if self.engine.is_none() {
            return false;
        }
        let source = args.source();
        if !matches!(
            source.pixel_format,
            TexturePixelFormat::Rgba | TexturePixelFormat::RgbaPremultiplied
        ) || args.alpha != 255
            || args.colorize.is_some()
            || args.tiling.is_some()
            || args.rotation != RenderingRotation::NoRotation
            || args.dst_width != source.width as usize
            || args.dst_height != source.height as usize
            || source.width == 0
            || source.height == 0
            || source.width > self.width
            || source.height > self.height
            || !source.width.is_multiple_of(4)
            || !source.height.is_multiple_of(2)
            || source.byte_stride > u32::MAX as usize
        {
            return false;
        }
        let row = source.width as usize * 4;
        if source.byte_stride < row
            || (source.height as usize - 1)
                .checked_mul(source.byte_stride)
                .and_then(|n| n.checked_add(row))
                .is_none_or(|n| n > source.data.len())
        {
            return false;
        }
        let rects: Vec<_> = clip
            .iter()
            .filter_map(|(p, s)| {
                clipped(
                    (
                        args.dst_x as i64,
                        args.dst_y as i64,
                        source.width,
                        source.height,
                    ),
                    (p.x, p.y, s.width, s.height),
                    (self.width, self.height),
                )
            })
            .collect();
        let area: u64 = rects
            .iter()
            .map(|r| u64::from(r.width) * u64::from(r.height))
            .sum();
        if area < 32768 || rects.len() > 16 {
            return false;
        }
        let key = args.source_cache_key();
        let cached = key.as_ref().and_then(|key| {
            self.keys
                .iter()
                .position(|entry| entry.as_ref() == Some(key))
        });
        let slot = cached.unwrap_or(self.next_slot);
        if cached.is_none() {
            self.keys[slot] = None;
            self.next_slot = 1 - slot;
        }
        let texture = Texture {
            data: source.data,
            width: source.width,
            height: source.height,
            stride: source.byte_stride as u32,
            premultiplied: matches!(source.pixel_format, TexturePixelFormat::RgbaPremultiplied),
            slot: slot as u32,
            upload: cached.is_none(),
            rects: &rects,
        };
        match self
            .engine
            .as_mut()
            .unwrap()
            .draw(&mut self.pixels, &texture)
        {
            Ok(Outcome::Unsupported) => false,
            Ok(outcome) => {
                self.keys[slot] = key;
                self.stats.cache_hits += u64::from(cached.is_some());
                match outcome {
                    Outcome::Copied => self.stats.copied += 1,
                    Outcome::Blended => self.stats.blended += 1,
                    _ => unreachable!(),
                }
                true
            }
            Err(error) => {
                self.stats.errors += 1;
                eprintln!(
                    "slint-bridge: TDE disabled after SDK error {} (unknown completion={})",
                    error.code, error.unknown
                );
                if error.unknown {
                    self.fatal = Some(error);
                }
                // Native Drop retains MMZ/library/fd if completion is unknown.
                self.engine = None;
                self.keys = [None, None];
                false
            }
        }
    }
}
impl TargetPixelBuffer for Buffer {
    type TargetPixel = PremultipliedRgbaColor;
    fn num_lines(&self) -> usize {
        self.height as usize
    }
    fn line_slice(&mut self, line: usize) -> &mut [Self::TargetPixel] {
        let offset = line * self.width as usize;
        &mut self.pixels[offset..offset + self.width as usize]
    }
    fn draw_texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        let done = self.texture(args, clip);
        if !done {
            self.stats.software += 1;
        }
        done
    }
}

#[cfg(target_arch = "arm")]
mod native {
    use super::*;
    use std::{ffi::c_void, ptr::NonNull};
    unsafe extern "C" {
        fn bridge_tde_create(width: u32, height: u32, error: *mut i32) -> *mut c_void;
        fn bridge_tde_destroy(context: *mut c_void);
        fn bridge_tde_draw(
            context: *mut c_void,
            pixels: *mut u8,
            data: *const u8,
            width: u32,
            height: u32,
            stride: u32,
            premultiplied: i32,
            slot: u32,
            upload: i32,
            rects: *const Rect,
            count: u32,
            error: *mut i32,
        ) -> i32;
    }
    pub(super) struct Native(NonNull<c_void>);
    impl Native {
        pub fn new(width: u32, height: u32) -> Result<Self, i32> {
            let mut error = 0;
            // SAFETY: bounded dimensions checked by C; NOLOAD cannot initialize VO.
            NonNull::new(unsafe { bridge_tde_create(width, height, &mut error) })
                .map(Self)
                .ok_or(error)
        }
    }
    impl Accelerator for Native {
        fn draw(
            &mut self,
            pixels: &mut [PremultipliedRgbaColor],
            t: &Texture<'_>,
        ) -> Result<Outcome, Failure> {
            let mut code = 0;
            // SAFETY: CPU buffers and rects are valid for this synchronous call.
            // Rust validated source length; C rechecks all geometry. DMA targets
            // private MMZ, never these CPU pointers. Commit is after completion.
            let result = unsafe {
                bridge_tde_draw(
                    self.0.as_ptr(),
                    pixels.as_mut_ptr().cast(),
                    t.data.as_ptr(),
                    t.width,
                    t.height,
                    t.stride,
                    t.premultiplied as i32,
                    t.slot,
                    t.upload as i32,
                    t.rects.as_ptr(),
                    t.rects.len() as u32,
                    &mut code,
                )
            };
            match result {
                0 => Ok(Outcome::Copied),
                1 => Ok(Outcome::Blended),
                2 => Ok(Outcome::Unsupported),
                _ => Err(Failure {
                    code,
                    unknown: result == -2,
                }),
            }
        }
    }
    impl Drop for Native {
        fn drop(&mut self) {
            // SAFETY: owns the context; C deliberately retains uncertain jobs.
            unsafe {
                bridge_tde_destroy(self.0.as_ptr());
            }
        }
    }
}

#[cfg(test)]
mod tests;
