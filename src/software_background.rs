//! One bounded, opaque background cache in physical canvas coordinates.
use i_slint_renderer_software::{
    DrawTextureArgs, PhysicalRegion, PremultipliedRgbaColor, RenderingRotation, SoftwareRenderer,
    TargetPixelBuffer, TextureCacheKey, TexturePixelFormat,
};

// At most 4 MiB of derived pixels, enough for X9's 800 x 1280 canvas.
const MAX_PIXELS: usize = 1024 * 1024;

#[derive(PartialEq)]
struct Key {
    source: TextureCacheKey,
    width: usize,
    height: usize,
    rotation: RenderingRotation,
}

#[derive(Default)]
pub(crate) struct BackgroundCache {
    key: Option<Key>,
    // An empty vector with a key also remembers a rejected (non-opaque) source.
    pixels: Vec<PremultipliedRgbaColor>,
    #[cfg(test)]
    hits: usize,
}

impl BackgroundCache {
    pub(crate) fn render(
        &mut self,
        renderer: &SoftwareRenderer,
        pixels: &mut [PremultipliedRgbaColor],
        width: usize,
    ) -> PhysicalRegion {
        renderer.render_into_buffer(&mut Buffer {
            pixels,
            width,
            cache: self,
            considered: false,
        })
    }
}

struct Buffer<'a> {
    pixels: &'a mut [PremultipliedRgbaColor],
    width: usize,
    cache: &'a mut BackgroundCache,
    considered: bool,
}

impl TargetPixelBuffer for Buffer<'_> {
    type TargetPixel = PremultipliedRgbaColor;

    fn line_slice(&mut self, line: usize) -> &mut [Self::TargetPixel] {
        &mut self.pixels[line * self.width..(line + 1) * self.width]
    }

    fn num_lines(&self) -> usize {
        self.pixels.len() / self.width
    }

    fn draw_texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        let height = self.num_lines();
        if self.considered
            || self.pixels.len() > MAX_PIXELS
            || args.dst_x != 0
            || args.dst_y != 0
            || args.dst_width != self.width
            || args.dst_height != height
            || args.alpha != 255
            || args.colorize.is_some()
            || args.tiling.is_some()
        {
            return false;
        }
        let Some(source_key) = args.source_cache_key() else {
            return false;
        };
        let source = args.source();
        let bpp = match source.pixel_format {
            TexturePixelFormat::Rgb => 3,
            TexturePixelFormat::Rgba | TexturePixelFormat::RgbaPremultiplied => 4,
            _ => return false,
        };
        let transpose = matches!(
            args.rotation,
            RenderingRotation::Rotate90 | RenderingRotation::Rotate270
        );
        let (width_unrotated, height_unrotated) = if transpose {
            (height, self.width)
        } else {
            (self.width, height)
        };
        let dx = ((source.width as usize * 256) / width_unrotated).max(1);
        let dy = ((source.height as usize * 256) / height_unrotated).max(1);
        // Slint uses Q8 steps and Q4 clip offsets. Only admit scales whose
        // offsets stay exact for every integer clip origin, including nested
        // clipping. Other scales retain Slint's original sampling path.
        if dx > u16::MAX as usize
            || dy > u16::MAX as usize
            || !dx.is_multiple_of(16)
            || !dy.is_multiple_of(16)
            || source.width > 4095
            || source.height > 4095
        {
            return false;
        }
        self.considered = true; // Never churn between multiple full-screen layers in one frame.
        let key = Key {
            source: source_key,
            width: self.width,
            height,
            rotation: args.rotation,
        };
        if self.cache.key.as_ref() != Some(&key) {
            self.cache.key = Some(key);
            self.cache.pixels.clear();
            if bpp == 4
                && (0..source.height as usize).any(|y| {
                    source.data
                        [y * source.byte_stride..y * source.byte_stride + source.width as usize * 4]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|p| p[3] != 255)
                })
            {
                return false;
            }
            if self
                .cache
                .pixels
                .try_reserve_exact(self.pixels.len())
                .is_err()
            {
                return false;
            }
            for y in 0..height {
                for x in 0..self.width {
                    let (sx, sy) = match args.rotation {
                        RenderingRotation::NoRotation => (x, y),
                        RenderingRotation::Rotate90 => (y, self.width - 1 - x),
                        RenderingRotation::Rotate180 => (self.width - 1 - x, height - 1 - y),
                        RenderingRotation::Rotate270 => (height - 1 - y, x),
                        _ => return false,
                    };
                    let at = (sy * dy / 256 % source.height as usize) * source.byte_stride
                        + (sx * dx / 256 % source.width as usize) * bpp;
                    let p = &source.data[at..at + bpp];
                    self.cache.pixels.push(PremultipliedRgbaColor {
                        red: p[0],
                        green: p[1],
                        blue: p[2],
                        alpha: 255,
                    });
                }
            }
        } else {
            #[cfg(test)]
            {
                self.cache.hits += usize::from(!self.cache.pixels.is_empty());
            }
        }
        if self.cache.pixels.is_empty() {
            return false;
        }
        for (origin, size) in clip.iter() {
            for y in origin.y as usize..origin.y as usize + size.height as usize {
                let start = y * self.width + origin.x as usize;
                let end = start + size.width as usize;
                self.pixels[start..end].copy_from_slice(&self.cache.pixels[start..end]);
            }
        }
        true
    }
}

#[cfg(test)]
#[path = "tests/software_background.rs"]
mod tests;
