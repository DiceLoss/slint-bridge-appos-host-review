//! Bounded rotation cache for repeated, unscaled premultiplied image backgrounds.
use i_slint_renderer_software::{
    DrawTextureArgs, PhysicalRegion, PremultipliedRgbaColor, RenderingRotation, TargetPixel,
    TextureCacheKey, TexturePixelFormat,
};

// Two 512 KiB slots: the alternating row/card backgrounds fit without retaining
// every item in a list. Source keys additionally retain their shared image storage.
const MAX_ENTRY_PIXELS: usize = 128 * 1024;

struct Entry {
    source: TextureCacheKey,
    rotation: RenderingRotation,
    width: usize,
    pixels: Vec<PremultipliedRgbaColor>,
    used: u64,
}

#[derive(Default)]
pub(crate) struct RotatedCache {
    entries: [Option<Entry>; 2],
    clock: u64,
    #[cfg(test)]
    hits: usize,
    #[cfg(test)]
    misses: usize,
}

impl RotatedCache {
    pub(crate) fn draw(
        &mut self,
        args: &DrawTextureArgs,
        clip: &PhysicalRegion,
        pixels: &mut [PremultipliedRgbaColor],
        stride: usize,
    ) -> bool {
        if !matches!(
            args.rotation,
            RenderingRotation::Rotate90 | RenderingRotation::Rotate270
        ) || args.alpha != 255
            || args.colorize.is_some()
            || args.tiling.is_some()
        {
            return false;
        }
        let source = args.source();
        if source.pixel_format != TexturePixelFormat::RgbaPremultiplied
            || source.width < 64
            || source.height < 64
            || args.dst_width != source.height as usize
            || args.dst_height != source.width as usize
        {
            return false;
        }
        let Some(count) = args
            .dst_width
            .checked_mul(args.dst_height)
            .filter(|&n| n <= MAX_ENTRY_PIXELS)
        else {
            return false;
        };
        let Some(key) = args.source_cache_key() else {
            return false;
        };
        let slot = self.entries.iter().position(|e| {
            e.as_ref()
                .is_some_and(|e| e.source == key && e.rotation == args.rotation)
        });
        self.clock += 1;
        let slot = if let Some(slot) = slot {
            slot
        } else {
            let slot = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.as_ref().map_or(0, |e| e.used))
                .unwrap()
                .0;
            // Remember the first use without generating pixels. Continuously
            // resized SVGs and other one-use sources should not build a cache.
            // Replacing the slot also frees its old pixels before any allocation.
            self.entries[slot] = Some(Entry {
                source: key,
                rotation: args.rotation,
                width: args.dst_width,
                pixels: Vec::new(),
                used: self.clock,
            });
            return false;
        };
        let entry = self.entries[slot].as_mut().unwrap();
        entry.used = self.clock;
        if entry.pixels.is_empty() {
            if entry.pixels.try_reserve_exact(count).is_err() {
                return false;
            }
            for y in 0..args.dst_height {
                for x in 0..args.dst_width {
                    let (sx, sy) = if args.rotation == RenderingRotation::Rotate90 {
                        (y, args.dst_width - 1 - x)
                    } else {
                        (args.dst_height - 1 - y, x)
                    };
                    let at = sy * source.byte_stride + sx * 4;
                    let p = &source.data[at..at + 4];
                    entry.pixels.push(PremultipliedRgbaColor {
                        red: p[0],
                        green: p[1],
                        blue: p[2],
                        alpha: p[3],
                    });
                }
            }
            #[cfg(test)]
            {
                self.misses += 1;
            }
        } else {
            #[cfg(test)]
            {
                self.hits += 1;
            }
        }
        for (origin, size) in clip.iter() {
            let sx = (origin.x as isize - args.dst_x) as usize;
            let sy = (origin.y as isize - args.dst_y) as usize;
            for row in 0..size.height as usize {
                let start = (sy + row) * entry.width + sx;
                let mut samples = entry.pixels[start..start + size.width as usize]
                    .iter()
                    .copied();
                let dest = (origin.y as usize + row) * stride + origin.x as usize;
                // Reuse Slint's exact alpha rounding and opaque-batch fast path.
                PremultipliedRgbaColor::blend_sampled_slice(
                    &mut pixels[dest..dest + size.width as usize],
                    || samples.next().unwrap(),
                );
            }
        }
        true
    }

    #[cfg(test)]
    fn cached_pixels(&self) -> usize {
        self.entries
            .iter()
            .flatten()
            .map(|e| e.pixels.capacity())
            .sum()
    }
}

#[cfg(test)]
#[path = "tests/software_texture.rs"]
mod tests;
