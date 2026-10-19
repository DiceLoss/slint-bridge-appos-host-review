//! The bounded PNG subset accepted by F1's VE, before any hardware access.
use slint::{Image, Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer};

#[cfg(target_arch = "riscv64")]
mod native;

const MAX_ENCODED: usize = 2 * 1024 * 1024;
const MAX_PIXELS: usize = 1024 * 600;

struct Png<'a> {
    data: &'a [u8],
    width: u32,
    height: u32,
    color: u8,
    stride: usize,
    idat_len: usize,
}

fn be(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes[..4].try_into().unwrap())
}

impl<'a> Png<'a> {
    fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() > MAX_ENCODED || data.get(..8)? != b"\x89PNG\r\n\x1a\n" {
            return None;
        }
        let mut result = Self {
            data,
            width: 0,
            height: 0,
            color: 0,
            stride: 0,
            idat_len: 0,
        };
        let mut at = 8;
        let mut idat_ended = false;
        let mut idat_seen = false;
        let mut zlib = [0u8; 2];
        let mut zlib_bytes = 0;
        while at < data.len() {
            let header = data.get(at..at.checked_add(8)?)?;
            let length = be(header) as usize;
            let end = at.checked_add(12)?.checked_add(length)?;
            let chunk = data.get(at..end)?;
            if crc32fast::hash(&chunk[4..8 + length]) != be(&chunk[8 + length..]) {
                return None;
            }
            let kind = &header[4..8];
            let payload = &chunk[8..8 + length];
            if !kind.iter().all(u8::is_ascii_alphabetic) || kind[2] & 32 != 0 {
                return None;
            }
            if result.width == 0 && kind != b"IHDR" {
                return None;
            }
            match kind {
                b"IHDR" => {
                    if at != 8
                        || length != 13
                        || payload[8] != 8
                        || !matches!(payload[9], 2 | 6)
                        || payload[10..] != [0, 0, 0]
                    {
                        return None;
                    }
                    result.width = be(payload);
                    result.height = be(&payload[4..]);
                    result.color = payload[9];
                    let pixels = (result.width as usize).checked_mul(result.height as usize)?;
                    if pixels == 0
                        || pixels > MAX_PIXELS
                        || result.width > 4096
                        || result.height > 4096
                    {
                        return None;
                    }
                    result.stride = (result.width as usize * 4 + 7) & !7;
                }
                b"IDAT" => {
                    if idat_ended {
                        return None;
                    }
                    idat_seen = true;
                    result.idat_len = result.idat_len.checked_add(length)?;
                    for byte in payload.iter().take(2 - zlib_bytes) {
                        zlib[zlib_bytes] = *byte;
                        zlib_bytes += 1;
                    }
                }
                b"IEND" => {
                    if length != 0
                        || end != data.len()
                        || result.idat_len <= 6
                        || zlib[0] & 15 != 8
                        || zlib[0] >> 4 > 7
                        || zlib[1] & 32 != 0
                        || !u16::from_be_bytes(zlib).is_multiple_of(31)
                    {
                        return None;
                    }
                    return Some(result);
                }
                b"PLTE" | b"tRNS" | b"acTL" | b"fcTL" | b"fdAT" => return None,
                _ => {
                    if kind[0] & 32 == 0 {
                        return None;
                    }
                    idat_ended |= idat_seen;
                }
            }
            at = end;
        }
        None
    }

    fn output_len(&self) -> usize {
        self.stride * self.height as usize
    }

    fn bitstream_len(&self) -> usize {
        (self.idat_len + 64 + 4095) & !4095
    }

    fn copy_idat(&self, destination: &mut [u8]) {
        assert!(destination.len() >= self.idat_len);
        let mut at = 8;
        let mut written = 0;
        while at < self.data.len() {
            let length = be(&self.data[at..]) as usize;
            if &self.data[at + 4..at + 8] == b"IDAT" {
                destination[written..written + length]
                    .copy_from_slice(&self.data[at + 8..at + 8 + length]);
                written += length;
            }
            at += length + 12;
        }
        assert_eq!(written, self.idat_len);
    }

    fn image(&self, output: &[u8]) -> Image {
        assert!(output.len() >= self.output_len());
        if self.color == 2 {
            let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(self.width, self.height);
            for (row, target) in output
                .chunks(self.stride)
                .zip(pixels.make_mut_slice().chunks_mut(self.width as usize))
            {
                for (source, target) in row.as_chunks::<4>().0.iter().zip(target) {
                    *target = Rgb8Pixel::new(source[0], source[1], source[2]);
                }
            }
            Image::from_rgb8(pixels)
        } else {
            let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(self.width, self.height);
            for (row, target) in output
                .chunks(self.stride)
                .zip(pixels.make_mut_bytes().chunks_mut(self.width as usize * 4))
            {
                target.copy_from_slice(&row[..target.len()]);
                for target in target.as_chunks_mut::<4>().0 {
                    if target[3] == 255 {
                        continue;
                    }
                    let alpha = u32::from(target[3]);
                    // Match Slint's RGBA decoder rounding before smooth scaling.
                    let channel = |value| (((u32::from(value) * alpha + 128) * 257) >> 16) as u8;
                    *target = [
                        channel(target[0]),
                        channel(target[1]),
                        channel(target[2]),
                        target[3],
                    ];
                }
            }
            Image::from_rgba8_premultiplied(pixels)
        }
    }
}

fn completed(status: u32, written: u32, png: &Png<'_>) -> bool {
    status == 1
        && written as usize == png.stride * (png.height as usize - 1) + png.width as usize * 4
}

#[cfg(target_arch = "riscv64")]
pub(crate) fn decode(data: &[u8]) -> Result<Option<Image>, crate::Error> {
    native::decode(data)
}

#[cfg(test)]
mod tests;
