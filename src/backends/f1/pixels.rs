//! Packed scalar writes for F1's little-endian BGRA framebuffer.
use i_slint_renderer_software::PremultipliedRgbaColor;

#[inline]
fn swap_pairs(value: u64) -> u64 {
    (value & 0xff00_ff00_ff00_ff00)
        | ((value & 0x0000_00ff_0000_00ff) << 16)
        | ((value >> 16) & 0x0000_00ff_0000_00ff)
}

/// A false result requires the caller to overwrite the entire destination row.
pub(crate) fn copy_opaque(source: &[PremultipliedRgbaColor], destination: &mut [u8]) -> bool {
    assert_eq!(destination.len(), source.len() * 4);
    let src = source.as_ptr().cast::<u8>();
    let dst = destination.as_mut_ptr();
    if cfg!(target_endian = "big")
        || src.align_offset(4) != 0
        || dst.align_offset(4) != 0
        || src.align_offset(8) != dst.align_offset(8)
    {
        return false;
    }
    let mut at = 0;
    // SAFETY: RGBA has four initialized bytes per pixel. The slices are distinct,
    // exactly sized, and word-aligned. A leading pixel aligns both pointers to
    // eight bytes; paired accesses stay within complete groups of four pixels.
    unsafe {
        if !source.is_empty() && src.align_offset(8) != 0 {
            let value = src.cast::<u32>().read();
            if value >> 24 != 255 {
                return false;
            }
            dst.cast::<u32>().write(swap_pairs(u64::from(value)) as u32);
            at = 1;
        }
        while at + 4 <= source.len() {
            let source_words = src.add(at * 4).cast::<u64>();
            let first = source_words.read();
            let second = source_words.add(1).read();
            if first & second & 0xff00_0000_ff00_0000 != 0xff00_0000_ff00_0000 {
                return false;
            }
            let target_words = dst.add(at * 4).cast::<u64>();
            target_words.write(swap_pairs(first));
            target_words.add(1).write(swap_pairs(second));
            at += 4;
        }
        while at < source.len() {
            let value = src.add(at * 4).cast::<u32>().read();
            if value >> 24 != 255 {
                return false;
            }
            dst.add(at * 4)
                .cast::<u32>()
                .write(swap_pairs(u64::from(value)) as u32);
            at += 1;
        }
    }
    true
}
