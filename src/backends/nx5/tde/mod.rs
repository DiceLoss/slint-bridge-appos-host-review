//! NX5 cached MMZ canvas shared by synchronous CPU and TDE drawing.
use crate::Error;
use i_slint_renderer_software::{
    DrawRectangleArgs, DrawTextureArgs, PhysicalRegion, PremultipliedRgbaColor, RenderingRotation,
    RepaintBufferType, SoftwareRenderer, TargetPixel, TargetPixelBuffer, TextureCacheKey,
    TexturePixelFormat,
};
use slint::Brush;

// Keep Slint's exact pixel semantics while accelerating long constant-color spans.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub(crate) struct Pixel(PremultipliedRgbaColor);

impl TargetPixel for Pixel {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        self.0.blend(color);
    }

    fn blend_slice(slice: &mut [Self], color: PremultipliedRgbaColor) {
        if color.alpha == 255 {
            slice.fill(Self(color));
            return;
        }
        #[cfg(target_arch = "arm")]
        {
            unsafe extern "C" {
                fn bridge_blend_rgba(pixels: *mut u8, count: usize, color: *const u8);
            }
            // SAFETY: Pixel is a transparent wrapper around repr(C) RGBA bytes;
            // the C routine writes exactly slice.len() pixels, including its tail.
            unsafe {
                bridge_blend_rgba(
                    slice.as_mut_ptr().cast(),
                    slice.len(),
                    (&color as *const PremultipliedRgbaColor).cast(),
                );
            }
        }
        #[cfg(not(target_arch = "arm"))]
        for pixel in slice {
            pixel.0.blend(color);
        }
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self(PremultipliedRgbaColor::from_rgb(red, green, blue))
    }

    fn background() -> Self {
        Self(PremultipliedRgbaColor::background())
    }
}

#[cfg(target_arch = "arm")]
pub(crate) unsafe fn rgba_to_bgra(
    source: *const u8,
    destination: *mut u8,
    count: usize,
    alpha_max: u8,
) -> bool {
    unsafe extern "C" {
        fn bridge_neon_available() -> i32;
        fn bridge_rgba_to_bgra(
            destination: *mut u8,
            source: *const u8,
            count: usize,
            reciprocals: *const u32,
            alpha_max: u8,
        );
    }
    // SAFETY: caller guarantees disjoint, valid count*4 byte ranges. The C
    // implementation accepts unaligned rows and handles the scalar remainder.
    unsafe {
        if bridge_neon_available() == 0 {
            return false;
        }
        bridge_rgba_to_bgra(
            destination,
            source,
            count,
            crate::framebuffer::ALPHA_RECIPROCALS.as_ptr(),
            alpha_max,
        );
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
    alpha: u8,
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
    fn fill(
        &mut self,
        _pixels: &mut [PremultipliedRgbaColor],
        _color: PremultipliedRgbaColor,
        _rects: &[Rect],
    ) -> Result<bool, Failure> {
        Ok(false)
    }

    fn draw(
        &mut self,
        pixels: &mut [PremultipliedRgbaColor],
        texture: &Texture<'_>,
    ) -> Result<Outcome, Failure>;
}

#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct Stats {
    pub filled: u64,
    pub copied: u64,
    pub blended: u64,
    pub software: u64,
    pub cache_hits: u64,
    pub errors: u64,
}

struct CachedTexture {
    last_frame: u64,
    pixels: u64,
    key: (TextureCacheKey, u8),
    // Opaque copies are reusable at any position; blend uploads encode parity.
    phase: Option<(u8, u8)>,
}

enum Pixels {
    Heap(Vec<PremultipliedRgbaColor>),
    #[cfg(target_arch = "arm")]
    Mmz(native::Canvas),
}
impl Pixels {
    // Buffer guards both accessors while a frame is invalid or DMA is unknown.
    fn cpu(&self) -> &[PremultipliedRgbaColor] {
        match self {
            Self::Heap(pixels) => pixels,
            #[cfg(target_arch = "arm")]
            Self::Mmz(canvas) => canvas.pixels(),
        }
    }
    fn cpu_mut(&mut self) -> &mut [PremultipliedRgbaColor] {
        match self {
            Self::Heap(pixels) => pixels,
            #[cfg(target_arch = "arm")]
            Self::Mmz(canvas) => canvas.pixels_mut(),
        }
    }
    fn accelerator_buffer(&mut self) -> &mut [PremultipliedRgbaColor] {
        match self {
            Self::Heap(pixels) => pixels,
            // Do not create a Rust slice aliasing an in-place DMA destination.
            // Native uses its owned mapping; heap-backed test doubles use pixels.
            #[cfg(target_arch = "arm")]
            Self::Mmz(_) => &mut [],
        }
    }
}

pub(crate) struct Buffer {
    pixels: Pixels,
    width: u32,
    height: u32,
    engine: Option<Box<dyn Accelerator>>,
    keys: [Option<CachedTexture>; 3],
    fatal: Option<Failure>,
    pub stats: Stats,
    frames: u64,
    trace: bool,
    fill_enabled: bool,
    replay_full: bool,
    discarded_row: Vec<PremultipliedRgbaColor>,
}

impl Buffer {
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        #[cfg(target_arch = "arm")]
        let result = match native::Native::new(width, height) {
            Ok((engine, canvas)) => {
                let pixels = Pixels::Mmz(canvas);
                eprintln!(
                    "slint-bridge: TDE enabled; shared cached MMZ canvas + two full source slots and one compact slot"
                );
                Self::with_pixels(width, height, pixels, Some(Box::new(engine)))
            }
            Err(code) => {
                eprintln!("slint-bridge: TDE unavailable ({code}); software rendering");
                Self::software(width, height)
            }
        };
        #[cfg(not(target_arch = "arm"))]
        let result = Self::software(width, height);
        let mut buffer = result?;
        // Host builds keep the identical callback/fallback path without the SDK.
        buffer.trace = std::env::var_os("SLINT_BRIDGE_TRACE").is_some();
        // Keep the explicit override while comparing fill on the shared canvas.
        buffer.fill_enabled = std::env::var("SLINT_BRIDGE_TDE_FILL").as_deref() == Ok("1");
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
        Self::with_pixels(width, height, Pixels::Heap(pixels), None)
    }
    fn with_pixels(
        width: u32,
        height: u32,
        pixels: Pixels,
        engine: Option<Box<dyn Accelerator>>,
    ) -> Result<Self, Error> {
        let mut discarded_row = Vec::new();
        discarded_row
            .try_reserve_exact(width as usize)
            .map_err(|_| Error::UnsupportedFramebuffer("TDE discard row allocation failed"))?;
        discarded_row.resize(width as usize, PremultipliedRgbaColor::default());
        Ok(Self {
            pixels,
            width,
            height,
            engine,
            keys: [None, None, None],
            fatal: None,
            stats: Stats::default(),
            frames: 0,
            trace: false,
            fill_enabled: false,
            replay_full: false,
            discarded_row,
        })
    }
    pub fn pixels(&self) -> &[PremultipliedRgbaColor] {
        assert!(
            self.fatal.is_none() && !self.replay_full,
            "canvas is not CPU-owned"
        );
        self.pixels.cpu()
    }
    pub(crate) fn has_unknown_dma(&self) -> bool {
        self.fatal.is_some()
    }
    pub fn render(&mut self, renderer: &SoftwareRenderer) -> Result<PhysicalRegion, Error> {
        if let Some(error) = self.fatal {
            return Err(Error::AccelerationFailed(format!(
                "SDK code {}",
                error.code
            )));
        }
        let mut region = renderer.render_into_buffer(self);
        if let Some(error) = self.fatal {
            return Err(Error::AccelerationFailed(format!(
                "SDK code {}; MMZ quarantined",
                error.code
            )));
        }
        if std::mem::take(&mut self.replay_full) {
            // A retired failed job may have changed any part of its destination.
            // Discard this frame and reconstruct the entire canvas in software.
            let mode = renderer.repaint_buffer_type();
            renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            region = renderer.render_into_buffer(self);
            renderer.set_repaint_buffer_type(mode);
        }
        self.frames += 1;
        if self.trace && self.frames.is_multiple_of(150) {
            eprintln!(
                "slint-bridge: tde frames={} copy={} blend={} software={} cache_hits={} errors={} fill={}",
                self.frames,
                self.stats.copied,
                self.stats.blended,
                self.stats.software,
                self.stats.cache_hits,
                self.stats.errors,
                self.stats.filled
            );
        }
        Ok(region)
    }

    fn disable(&mut self, error: Failure) {
        self.stats.errors += 1;
        eprintln!(
            "slint-bridge: TDE disabled after SDK error {} (unknown completion={})",
            error.code, error.unknown
        );
        if error.unknown {
            self.fatal = Some(error);
        } else {
            self.replay_full = true;
        }
        // The canvas retains MMZ ownership after acceleration is disabled.
        // Native context destruction also retains unknown DMA allocations.
        self.engine = None;
        self.keys = [None, None, None];
    }

    fn fill(&mut self, color: PremultipliedRgbaColor, rects: &[Rect]) -> bool {
        if !self.fill_enabled {
            return false;
        }
        let Some(engine) = self.engine.as_mut() else {
            return false;
        };
        let area: u64 = rects
            .iter()
            .map(|r| u64::from(r.width) * u64::from(r.height))
            .sum();
        if area < 32768 || rects.len() > 16 {
            return false;
        }
        match engine.fill(self.pixels.accelerator_buffer(), color, rects) {
            Ok(done) => {
                self.stats.filled += u64::from(done);
                done
            }
            Err(error) => {
                self.disable(error);
                false
            }
        }
    }

    fn fill_rects(&self, bounds: (i64, i64, u32, u32), clip: &PhysicalRegion) -> Vec<Rect> {
        clip.iter()
            .filter_map(|(p, s)| {
                clipped(
                    bounds,
                    (p.x, p.y, s.width, s.height),
                    (self.width, self.height),
                )
            })
            .collect()
    }

    fn texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        if self.engine.is_none() {
            return false;
        }
        let source = args.source();
        if !matches!(
            source.pixel_format,
            TexturePixelFormat::Rgba | TexturePixelFormat::RgbaPremultiplied
        ) || args.colorize.is_some()
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
        // Prepared source pixels include opacity; changing it must upload again.
        let key = args.source_cache_key().map(|key| (key, args.alpha));
        let phase = ((args.dst_x & 1) as u8, (args.dst_y & 1) as u8);
        let cached = key.as_ref().and_then(|key| {
            self.keys.iter().position(|entry| {
                entry.as_ref().is_some_and(|entry| {
                    entry.key == *key && entry.phase.is_none_or(|value| value == phase)
                })
            })
        });
        let canvas_pixels = u64::from(self.width) * u64::from(self.height);
        let padded_pixels = (u64::from(self.width) + 4) * (u64::from(self.height) + 2);
        let compact_pixels = canvas_pixels / 4 + padded_pixels - canvas_pixels;
        let source_pixels = (u64::from(source.width) + 4) * (u64::from(source.height) + 2);
        // Use empty slots first, then older-frame sources. Within one frame,
        // preserve a larger upload instead of evicting it for small foregrounds.
        let slot = cached.unwrap_or_else(|| {
            self.keys
                .iter()
                .enumerate()
                .filter(|(slot, _)| *slot < 2 || source_pixels <= compact_pixels)
                .min_by_key(|(_, entry)| {
                    entry.as_ref().map(|entry| (entry.last_frame, entry.pixels))
                })
                .unwrap()
                .0
        });
        if cached.is_none() {
            self.keys[slot] = None;
        }
        let texture = Texture {
            data: source.data,
            width: source.width,
            height: source.height,
            stride: source.byte_stride as u32,
            premultiplied: matches!(source.pixel_format, TexturePixelFormat::RgbaPremultiplied),
            alpha: args.alpha,
            slot: slot as u32,
            upload: cached.is_none(),
            rects: &rects,
        };
        match self
            .engine
            .as_mut()
            .unwrap()
            .draw(self.pixels.accelerator_buffer(), &texture)
        {
            Ok(Outcome::Unsupported) => false,
            Ok(outcome) => {
                self.keys[slot] = key.map(|key| CachedTexture {
                    last_frame: self.frames,
                    pixels: u64::from(source.width) * u64::from(source.height),
                    key,
                    phase: matches!(outcome, Outcome::Blended).then_some(phase),
                });
                self.stats.cache_hits += u64::from(cached.is_some());
                match outcome {
                    Outcome::Copied => self.stats.copied += 1,
                    Outcome::Blended => self.stats.blended += 1,
                    _ => unreachable!(),
                }
                true
            }
            Err(error) => {
                self.disable(error);
                false
            }
        }
    }
}
impl TargetPixelBuffer for Buffer {
    type TargetPixel = Pixel;
    fn num_lines(&self) -> usize {
        self.height as usize
    }
    fn line_slice(&mut self, line: usize) -> &mut [Self::TargetPixel] {
        let offset = line * self.width as usize;
        let row = if self.fatal.is_some() || self.replay_full {
            // Slint's callback is infallible. Let it finish on a disposable row
            // without exposing or writing the shared canvas after an error.
            &mut self.discarded_row[..]
        } else {
            &mut self.pixels.cpu_mut()[offset..offset + self.width as usize]
        };
        // SAFETY: repr(transparent) preserves layout and alignment; the returned
        // mutable slice has the same lifetime and exclusive access as this row.
        unsafe { std::slice::from_raw_parts_mut(row.as_mut_ptr().cast(), row.len()) }
    }
    fn fill_background(&mut self, brush: &Brush, clip: &PhysicalRegion) -> bool {
        if !self.fill_enabled || self.engine.is_none() {
            return false;
        }
        let Brush::SolidColor(color) = brush else {
            return false;
        };
        let rects = self.fill_rects((0, 0, self.width, self.height), clip);
        self.fill((*color).into(), &rects)
    }
    fn draw_rectangle(&mut self, args: &DrawRectangleArgs, clip: &PhysicalRegion) -> bool {
        if !self.fill_enabled || self.engine.is_none() {
            return false;
        }
        let Brush::SolidColor(color) = args.background else {
            return false;
        };
        // QuickFill replaces pixels. Anti-aliased edges, borders, gradients and
        // translucent source-over retain Slint's software implementation.
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
        let rects = self.fill_rects(
            (
                args.x as i64,
                args.y as i64,
                args.width as u32,
                args.height as u32,
            ),
            clip,
        );
        self.fill(color.into(), &rects)
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
    use std::{ffi::c_void, ptr::NonNull, rc::Rc};
    unsafe extern "C" {
        fn bridge_tde_create(width: u32, height: u32, error: *mut i32) -> *mut c_void;
        fn bridge_tde_destroy(context: *mut c_void);
        fn bridge_tde_pixels(context: *mut c_void) -> *mut u8;
        fn bridge_tde_fill(
            context: *mut c_void,
            color: u32,
            rects: *const Rect,
            count: u32,
            error: *mut i32,
        ) -> i32;
        fn bridge_tde_draw(
            context: *mut c_void,
            data: *const u8,
            width: u32,
            height: u32,
            stride: u32,
            premultiplied: i32,
            alpha: u8,
            slot: u32,
            upload: i32,
            rects: *const Rect,
            count: u32,
            error: *mut i32,
        ) -> i32;
    }
    struct Context {
        raw: NonNull<c_void>,
        pixels: NonNull<PremultipliedRgbaColor>,
        count: usize,
    }
    pub(super) struct Canvas(Rc<Context>);
    impl Canvas {
        pub fn pixels(&self) -> &[PremultipliedRgbaColor] {
            // SAFETY: creation zero-initializes count pixels. Buffer only lends
            // CPU slices outside synchronous DMA and after successful retirement.
            unsafe { std::slice::from_raw_parts(self.0.pixels.as_ptr(), self.0.count) }
        }
        pub fn pixels_mut(&mut self) -> &mut [PremultipliedRgbaColor] {
            // SAFETY: Canvas has a single owner; Native's Rc only keeps the SDK
            // context alive. Buffer never lends these pixels during a DMA call.
            unsafe { std::slice::from_raw_parts_mut(self.0.pixels.as_ptr(), self.0.count) }
        }
    }
    pub(super) struct Native(Rc<Context>);
    impl Native {
        pub fn new(width: u32, height: u32) -> Result<(Self, Canvas), i32> {
            let count = (width as usize)
                .checked_mul(height as usize)
                .filter(|count| {
                    *count <= isize::MAX as usize / std::mem::size_of::<PremultipliedRgbaColor>()
                })
                .ok_or(-1)?;
            let mut error = 0;
            // SAFETY: bounded dimensions checked by C; NOLOAD cannot initialize VO.
            let raw = NonNull::new(unsafe { bridge_tde_create(width, height, &mut error) })
                .ok_or(error)?;
            // SAFETY: a successfully created engine owns a non-null, initialized
            // MMZ mapping. C never relocates it, including after drawing errors.
            let pixels = unsafe { NonNull::new_unchecked(bridge_tde_pixels(raw.as_ptr()).cast()) };
            let context = Rc::new(Context { raw, pixels, count });
            Ok((Self(context.clone()), Canvas(context)))
        }
    }
    impl Accelerator for Native {
        fn fill(
            &mut self,
            _pixels: &mut [PremultipliedRgbaColor],
            color: PremultipliedRgbaColor,
            rects: &[Rect],
        ) -> Result<bool, Failure> {
            let mut code = 0;
            let packed = u32::from_le_bytes([color.red, color.green, color.blue, color.alpha]);
            // SAFETY: canvas and rectangles remain valid during this synchronous
            // call. C validates geometry and hands the shared canvas back only
            // after DMA retirement and cache synchronization.
            let result = unsafe {
                bridge_tde_fill(
                    self.0.raw.as_ptr(),
                    packed,
                    rects.as_ptr(),
                    rects.len() as u32,
                    &mut code,
                )
            };
            match result {
                0 => Ok(true),
                2 => Ok(false),
                _ => Err(Failure {
                    code,
                    unknown: result == -2,
                }),
            }
        }

        fn draw(
            &mut self,
            _pixels: &mut [PremultipliedRgbaColor],
            t: &Texture<'_>,
        ) -> Result<Outcome, Failure> {
            let mut code = 0;
            // SAFETY: CPU buffers and rects are valid for this synchronous call.
            // Rust validated source length; C rechecks all geometry. There is no
            // Rust slice aliasing the destination during this in-place DMA job.
            let result = unsafe {
                bridge_tde_draw(
                    self.0.raw.as_ptr(),
                    t.data.as_ptr(),
                    t.width,
                    t.height,
                    t.stride,
                    t.premultiplied as i32,
                    t.alpha,
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
    impl Drop for Context {
        fn drop(&mut self) {
            // SAFETY: owns the context; C deliberately retains uncertain jobs.
            unsafe {
                bridge_tde_destroy(self.raw.as_ptr());
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
