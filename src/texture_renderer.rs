//! CPU texture spans shared by the board adapters, using Slint's drawing interface.
use i_slint_renderer_software::{
    DrawTextureArgs, PhysicalRegion, PremultipliedRgbaColor, RenderingRotation, TexturePixelFormat,
};

pub(crate) fn blend_sampled_slice(
    pixels: &mut [PremultipliedRgbaColor],
    mut sample: impl FnMut() -> PremultipliedRgbaColor,
) {
    // Gather strided samples before mixing so the arithmetic has contiguous inputs.
    for pixels in pixels.chunks_mut(32) {
        let mut colors = [PremultipliedRgbaColor::default(); 32];
        let colors = &mut colors[..pixels.len()];
        for color in colors.iter_mut() {
            *color = sample();
        }
        if colors.iter().fold(255, |alpha, color| alpha & color.alpha) == 255 {
            pixels.copy_from_slice(colors);
            continue;
        }
        for (pixel, color) in pixels.iter_mut().zip(colors) {
            let inverse = u16::from(255 - color.alpha);
            // Exact floor(v / 255) for byte products, with SIMD-friendly operations.
            let div255 = |v: u16| (v + 1 + (v >> 8)) >> 8;
            pixel.red = div255(u16::from(pixel.red) * inverse) as u8 + color.red;
            pixel.green = div255(u16::from(pixel.green) * inverse) as u8 + color.green;
            pixel.blue = div255(u16::from(pixel.blue) * inverse) as u8 + color.blue;
            pixel.alpha = (u16::from(pixel.alpha) + u16::from(color.alpha)
                - div255(u16::from(pixel.alpha) * u16::from(color.alpha)))
                as u8;
        }
    }
}

pub(crate) fn draw(
    args: &DrawTextureArgs,
    clip: &PhysicalRegion,
    pixels: &mut [PremultipliedRgbaColor],
    stride: usize,
) -> bool {
    if stride == 0
        || !pixels.len().is_multiple_of(stride)
        || args.dst_width == 0
        || args.dst_height == 0
        || args.dst_width > i16::MAX as usize
        || args.dst_height > i16::MAX as usize
        || args.colorize.is_some()
        || args.tiling.is_some()
    {
        return false;
    }
    let transpose = match args.rotation {
        RenderingRotation::NoRotation | RenderingRotation::Rotate180 => false,
        RenderingRotation::Rotate90 | RenderingRotation::Rotate270 => true,
        _ => return false,
    };
    let source = args.source();
    let bpp = match source.pixel_format {
        TexturePixelFormat::Rgb => 3,
        TexturePixelFormat::Rgba | TexturePixelFormat::RgbaPremultiplied => 4,
        _ => return false,
    };
    if source.width == 0
        || source.height == 0
        || source.width > 4095
        || source.height > 4095
        || source.byte_stride < source.width as usize * bpp
        || !source.byte_stride.is_multiple_of(bpp)
        || source.byte_stride / bpp > u16::MAX as usize
        || (source.height as usize - 1)
            .checked_mul(source.byte_stride)
            .and_then(|n| n.checked_add(source.width as usize * bpp))
            != Some(source.data.len())
    {
        return false;
    }
    let (width, height) = if transpose {
        (args.dst_height, args.dst_width)
    } else {
        (args.dst_width, args.dst_height)
    };
    let dx = (source.width as usize * 256 / width).max(1);
    let dy = (source.height as usize * 256 / height).max(1);
    // Slint 1.18.1 quantizes clip offsets to Q4 after computing Q8 steps.
    // Drawing callbacks expose damage clips, not the original clip origin.
    // These steps preserve identical sampling for every integer clip origin.
    if dx > u16::MAX as usize
        || dy > u16::MAX as usize
        || !dx.is_multiple_of(16)
        || !dy.is_multiple_of(16)
    {
        return false;
    }
    let (x, y) = (args.dst_x as i64, args.dst_y as i64);
    let Some(right) = x.checked_add(args.dst_width as i64) else {
        return false;
    };
    let Some(bottom) = y.checked_add(args.dst_height as i64) else {
        return false;
    };
    let canvas_height = pixels.len() / stride;
    let tiled = transpose && args.dst_width >= 64 && args.dst_height >= 64;
    for (origin, size) in clip.iter() {
        let left = x.max(i64::from(origin.x)).max(0);
        let top = y.max(i64::from(origin.y)).max(0);
        let right = right
            .min(i64::from(origin.x) + i64::from(size.width))
            .min(stride as i64);
        let bottom = bottom
            .min(i64::from(origin.y) + i64::from(size.height))
            .min(canvas_height as i64);
        if left >= right || top >= bottom {
            continue;
        }
        let (left, top, right, bottom) =
            (left as usize, top as usize, right as usize, bottom as usize);
        let block_width = if tiled { 32 } else { right - left };
        let block_height = if tiled { 32 } else { bottom - top };
        // Rotated rows read source columns. Tiles keep adjacent columns in cache.
        for tile_y in (top..bottom).step_by(block_height) {
            for tile_x in (left..right).step_by(block_width) {
                let end = right.min(tile_x + block_width);
                for row in tile_y..bottom.min(tile_y + block_height) {
                    let mut column = tile_x;
                    blend_sampled_slice(
                        &mut pixels[row * stride + tile_x..row * stride + end],
                        || {
                            let (tx, ty) =
                                ((column as i64 - x) as usize, (row as i64 - y) as usize);
                            column += 1;
                            let (u, v) = match args.rotation {
                                RenderingRotation::NoRotation => (tx, ty),
                                RenderingRotation::Rotate90 => (ty, args.dst_width - 1 - tx),
                                RenderingRotation::Rotate180 => {
                                    (args.dst_width - 1 - tx, args.dst_height - 1 - ty)
                                }
                                RenderingRotation::Rotate270 => (args.dst_height - 1 - ty, tx),
                                _ => unreachable!(),
                            };
                            let at = (v * dy / 256) * source.byte_stride + (u * dx / 256) * bpp;
                            let p = &source.data[at..at + bpp];
                            let alpha = if bpp == 3 {
                                args.alpha
                            } else {
                                (u16::from(p[3]) * u16::from(args.alpha) / 255) as u8
                            };
                            let factor =
                                if source.pixel_format == TexturePixelFormat::RgbaPremultiplied {
                                    args.alpha
                                } else {
                                    alpha
                                };
                            let channel = |c: u8| {
                                if factor == 255 {
                                    c
                                } else {
                                    (u16::from(c) * u16::from(factor) / 255) as u8
                                }
                            };
                            PremultipliedRgbaColor {
                                red: channel(p[0]),
                                green: channel(p[1]),
                                blue: channel(p[2]),
                                alpha,
                            }
                        },
                    );
                }
            }
        }
    }
    true
}

#[cfg(not(any(feature = "fbdev", feature = "nx5-tde", feature = "f1-g2d")))]
pub(crate) fn render(
    renderer: &i_slint_renderer_software::SoftwareRenderer,
    pixels: &mut [PremultipliedRgbaColor],
    stride: usize,
) -> PhysicalRegion {
    renderer.render_into_buffer(&mut Buffer { pixels, stride })
}

#[cfg(not(any(feature = "fbdev", feature = "nx5-tde", feature = "f1-g2d")))]
struct Buffer<'a> {
    pixels: &'a mut [PremultipliedRgbaColor],
    stride: usize,
}

#[cfg(not(any(feature = "fbdev", feature = "nx5-tde", feature = "f1-g2d")))]
impl i_slint_renderer_software::TargetPixelBuffer for Buffer<'_> {
    type TargetPixel = PremultipliedRgbaColor;
    fn num_lines(&self) -> usize {
        self.pixels.len() / self.stride
    }
    fn line_slice(&mut self, line: usize) -> &mut [Self::TargetPixel] {
        &mut self.pixels[line * self.stride..(line + 1) * self.stride]
    }
    fn draw_texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        draw(args, clip, self.pixels, self.stride)
    }
}

#[cfg(test)]
#[path = "tests/texture_renderer.rs"]
mod tests;
