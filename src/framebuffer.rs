use crate::{Error, backends::PresentationContract, presentation::Presentation, wake::WakeQueue};
use slint::{
    PhysicalSize,
    platform::software_renderer::{PremultipliedRgbaColor, SoftwareRenderer},
};
use std::{
    fs::{File, OpenOptions},
    io,
    os::fd::{AsFd, AsRawFd},
    path::Path,
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{Ordering, fence},
    },
};

// Rounded straight-alpha output without per-channel division. The native
// conversion borrows the same table; exhaustive tests compare with division.
pub(crate) static ALPHA_RECIPROCALS: [u32; 256] = {
    let mut values = [0; 256];
    let mut alpha = 1;
    while alpha < 256 {
        values[alpha] = (255u32 << 16).div_ceil(alpha as u32);
        alpha += 1;
    }
    values
};

fn straight_rgba(pixel: PremultipliedRgbaColor) -> [u8; 4] {
    match pixel.alpha {
        0 => [0; 4],
        255 => [pixel.red, pixel.green, pixel.blue, 255],
        alpha => {
            let factor = ALPHA_RECIPROCALS[usize::from(alpha)];
            let channel = |value| ((u32::from(value) * factor + 32768) >> 16).min(255) as u8;
            [
                channel(pixel.red),
                channel(pixel.green),
                channel(pixel.blue),
                alpha,
            ]
        }
    }
}

// Linux UAPI linux/fb.h. c_ulong is intentional: these layouts differ on ARM32
// and x86_64. No build-host-generated bindings are embedded in the ARM build.
#[repr(C)]
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bitfield {
    pub offset: u32,
    pub length: u32,
    pub msb_right: u32,
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct FixedInfo {
    pub id: [libc::c_char; 16],
    pub smem_start: libc::c_ulong,
    pub smem_len: u32,
    pub kind: u32,
    pub type_aux: u32,
    pub visual: u32,
    pub xpanstep: u16,
    pub ypanstep: u16,
    pub ywrapstep: u16,
    pub line_length: u32,
    pub mmio_start: libc::c_ulong,
    pub mmio_len: u32,
    pub accel: u32,
    pub capabilities: u16,
    pub reserved: [u16; 2],
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct VariableInfo {
    pub xres: u32,
    pub yres: u32,
    pub xres_virtual: u32,
    pub yres_virtual: u32,
    pub xoffset: u32,
    pub yoffset: u32,
    pub bits_per_pixel: u32,
    pub grayscale: u32,
    pub red: Bitfield,
    pub green: Bitfield,
    pub blue: Bitfield,
    pub alpha: Bitfield,
    pub nonstd: u32,
    pub activate: u32,
    pub height_mm: u32,
    pub width_mm: u32,
    pub accel_flags: u32,
    pub pixclock: u32,
    pub left_margin: u32,
    pub right_margin: u32,
    pub upper_margin: u32,
    pub lower_margin: u32,
    pub hsync_len: u32,
    pub vsync_len: u32,
    pub sync: u32,
    pub vmode: u32,
    pub rotate: u32,
    pub colorspace: u32,
    pub reserved: [u32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout {
    pub size: PhysicalSize,
    pub stride: usize,
    pub page_bytes: usize,
    pub initial_page: usize,
    // Board scanout alpha range; independent of the fbdev channel bit width.
    pub alpha_max: u8,
    bytes_per_pixel: usize,
    channels: [Bitfield; 4],
}

impl Layout {
    pub(crate) fn new(f: &FixedInfo, v: &VariableInfo) -> Result<Self, Error> {
        let unsupported = Error::UnsupportedFramebuffer;
        if f.kind != 0 || f.visual != 2 || v.grayscale != 0 || v.nonstd != 0 {
            return Err(unsupported("only packed true-color pixels are supported"));
        }
        if v.xres == 0
            || v.yres == 0
            || v.xres > i16::MAX as u32
            || v.yres > i16::MAX as u32
            || v.xoffset != 0
            || v.rotate != 0
            || v.vmode != 0
        {
            return Err(unsupported(
                "invalid geometry, offsets, interlacing, or hardware rotation",
            ));
        }
        let channels = [v.red, v.green, v.blue, v.alpha];
        let lengths = [v.red.length, v.green.length, v.blue.length, v.alpha.length];
        if !((v.bits_per_pixel == 16 && lengths == [5, 6, 5, 0])
            || (v.bits_per_pixel == 32 && lengths[..3] == [8, 8, 8] && matches!(lengths[3], 0 | 8)))
        {
            return Err(unsupported(
                "only RGB565 and 32-bit RGB888 with optional alpha are supported",
            ));
        }
        let mut occupied = 0u32;
        for channel in channels {
            if channel.msb_right != 0
                || (channel.length != 0
                    && (channel.offset >= v.bits_per_pixel
                        || channel.length > v.bits_per_pixel - channel.offset))
            {
                return Err(unsupported("invalid channel bitfield"));
            }
            if channel.length != 0 {
                let mask = ((1u32 << channel.length) - 1) << channel.offset;
                if occupied & mask != 0 {
                    return Err(unsupported("overlapping pixel channels"));
                }
                occupied |= mask;
            }
        }
        let bytes_per_pixel = (v.bits_per_pixel / 8) as usize;
        let stride = f.line_length as usize;
        let page_bytes = stride
            .checked_mul(v.yres as usize)
            .ok_or(unsupported("page size overflow"))?;
        let mapping_size = page_bytes
            .checked_mul(2)
            .ok_or(unsupported("mapping size overflow"))?;
        if mapping_size > isize::MAX as usize {
            return Err(unsupported("mapping exceeds addressable slice size"));
        }
        if stride < v.xres as usize * bytes_per_pixel
            || v.xres_virtual < v.xres
            || v.yres_virtual < v.yres * 2
            || (f.smem_len as usize) < mapping_size
            || !matches!(v.yoffset, 0) && v.yoffset != v.yres
        {
            return Err(unsupported(
                "two complete pages with a valid stride and front page are required",
            ));
        }
        Ok(Self {
            size: PhysicalSize::new(v.xres, v.yres),
            stride,
            page_bytes,
            initial_page: (v.yoffset / v.yres) as usize,
            alpha_max: 255,
            bytes_per_pixel,
            channels,
        })
    }

    #[cfg(feature = "nx5")]
    pub(crate) fn same_mode(self, other: Self) -> bool {
        self.size == other.size
            && self.stride == other.stride
            && self.bytes_per_pixel == other.bytes_per_pixel
            && self.channels == other.channels
    }

    fn pixel(self, pixel: PremultipliedRgbaColor) -> u32 {
        let channels = if self.channels[3].length == 8 {
            self.output_rgba(pixel)
        } else {
            // Alpha-less outputs retain the black-matted premultiplied RGB.
            [pixel.red, pixel.green, pixel.blue, pixel.alpha]
        };
        channels
            .into_iter()
            .zip(self.channels)
            .fold(0, |bits, (value, field)| {
                if field.length == 0 {
                    bits
                } else {
                    bits | ((u32::from(value) >> (8 - field.length)) << field.offset)
                }
            })
    }

    fn output_rgba(self, pixel: PremultipliedRgbaColor) -> [u8; 4] {
        let mut rgba = straight_rgba(pixel);
        rgba[3] = ((u16::from(rgba[3]) * u16::from(self.alpha_max) + 127) / 255) as u8;
        rgba
    }

    fn write_row(self, source: &[PremultipliedRgbaColor], destination: &mut [u8]) {
        assert_eq!(destination.len(), source.len() * self.bytes_per_pixel);
        #[cfg(all(feature = "nx5-tde", target_arch = "arm"))]
        if self.bytes_per_pixel == 4
            && self.channels.map(|c| (c.offset, c.length)) == [(16, 8), (8, 8), (0, 8), (24, 8)]
        {
            // SAFETY: PremultipliedRgbaColor is repr(C) RGBA bytes. These
            // distinct retained/mapped slices are exactly sized.
            if unsafe {
                crate::backends::nx5::tde::rgba_to_bgra(
                    source.as_ptr().cast(),
                    destination.as_mut_ptr(),
                    source.len(),
                    self.alpha_max,
                )
            } {
                return;
            }
        }
        if self.bytes_per_pixel == 4 && destination.as_ptr().align_offset(4) == 0 {
            // SAFETY: mmap and this row are u32 aligned; the byte range contains
            // exactly source.len() words. u32 accepts every bit pattern and the
            // unique byte borrow is not used while these words are borrowed.
            let words = unsafe {
                std::slice::from_raw_parts_mut(destination.as_mut_ptr().cast::<u32>(), source.len())
            };
            if self.channels.map(|c| (c.offset, c.length)) == [(16, 8), (8, 8), (0, 8), (24, 8)] {
                for (word, pixel) in words.iter_mut().zip(source) {
                    let [r, g, b, a] = self.output_rgba(*pixel);
                    *word = (u32::from(a) << 24)
                        | (u32::from(r) << 16)
                        | (u32::from(g) << 8)
                        | u32::from(b);
                }
            } else {
                for (word, pixel) in words.iter_mut().zip(source) {
                    *word = self.pixel(*pixel);
                }
            }
        } else {
            for (bytes, pixel) in destination
                .chunks_exact_mut(self.bytes_per_pixel)
                .zip(source)
            {
                let pixel = self.pixel(*pixel);
                if self.bytes_per_pixel == 2 {
                    bytes.copy_from_slice(&(pixel as u16).to_ne_bytes());
                } else {
                    bytes.copy_from_slice(&pixel.to_ne_bytes());
                }
            }
        }
    }
}

#[cfg(test)]
mod row_tests {
    use super::*;

    #[cfg(feature = "nx5")]
    #[test]
    fn nx5_scanout_encodes_alpha_in_the_measured_zero_to_128_range() {
        let (fixed, variable) = crate::tests::info(256, 1, 32);
        let layout = crate::backends::nx5::validate_layout(&fixed, &variable).unwrap();
        let source: Vec<_> = (0..=255u8)
            .map(|alpha| PremultipliedRgbaColor {
                red: alpha,
                green: 0,
                blue: 0,
                alpha,
            })
            .collect();
        let mut bytes = vec![0; 256 * 4];
        layout.write_row(&source, &mut bytes);
        for (alpha, pixel) in bytes.as_chunks::<4>().0.iter().enumerate() {
            let expected = if alpha == 0 {
                0
            } else {
                (alpha.div_ceil(2) as u32) << 24 | 0x00ff0000
            };
            assert_eq!(*pixel, expected.to_ne_bytes(), "UI alpha={alpha}");
        }
    }

    #[test]
    fn straight_alpha_matches_rounded_division_for_every_channel_and_alpha() {
        for alpha in 0..=255u8 {
            for value in 0..=255u8 {
                let pixel = PremultipliedRgbaColor {
                    red: value,
                    green: value,
                    blue: value,
                    alpha,
                };
                let expected = if alpha == 0 {
                    0
                } else {
                    ((u32::from(value) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255)
                        as u8
                };
                assert_eq!(
                    straight_rgba(pixel),
                    [expected, expected, expected, alpha],
                    "channel={value} alpha={alpha}"
                );
            }
        }
    }

    #[test]
    fn word_writes_match_pixel_format_and_preserve_surrounding_bytes() {
        let source: Vec<_> = (0..37u8)
            .map(|v| PremultipliedRgbaColor {
                red: v * 3,
                green: v * 5,
                blue: 255 - v * 2,
                alpha: [0, 1, 63, 127, 128, 191, 254, 255][usize::from(v) % 8],
            })
            .collect();
        let original = source.clone();
        for fields in [
            [(16, 8), (8, 8), (0, 8), (24, 8)],
            [(0, 8), (8, 8), (16, 8), (24, 8)],
            [(16, 8), (8, 8), (0, 8), (0, 0)],
            [(11, 5), (5, 6), (0, 5), (0, 0)],
        ] {
            let bytes_per_pixel = if fields[0].1 == 5 { 2 } else { 4 };
            let layout = Layout {
                size: PhysicalSize::new(37, 1),
                stride: 160,
                page_bytes: 160,
                initial_page: 0,
                alpha_max: 255,
                bytes_per_pixel,
                channels: fields.map(|(offset, length)| Bitfield {
                    offset,
                    length,
                    msb_right: 0,
                }),
            };
            for offset in 0..4 {
                let mut bytes = vec![0xa5; 37 * bytes_per_pixel + 8];
                let mut expected = bytes.clone();
                for (i, pixel) in source.iter().enumerate() {
                    let at = offset + i * bytes_per_pixel;
                    let channel = |value: u8| {
                        if fields[3].1 == 0 {
                            u32::from(value)
                        } else if pixel.alpha == 0 {
                            0
                        } else {
                            ((u32::from(value) * 255 + u32::from(pixel.alpha) / 2)
                                / u32::from(pixel.alpha))
                            .min(255)
                        }
                    };
                    let pixel = [
                        channel(pixel.red),
                        channel(pixel.green),
                        channel(pixel.blue),
                        u32::from(pixel.alpha),
                    ]
                    .into_iter()
                    .zip(fields)
                    .filter(|(_, (_, bits))| *bits != 0)
                    .fold(0u32, |word, (c, (shift, bits))| {
                        word | ((c >> (8 - bits)) << shift)
                    });
                    if bytes_per_pixel == 2 {
                        expected[at..at + 2].copy_from_slice(&(pixel as u16).to_ne_bytes());
                    } else {
                        expected[at..at + 4].copy_from_slice(&pixel.to_ne_bytes());
                    }
                }
                layout.write_row(&source, &mut bytes[offset..offset + 37 * bytes_per_pixel]);
                assert_eq!(bytes, expected, "fields={fields:?}, offset={offset}");
                for (actual, expected) in source.iter().zip(&original) {
                    assert_eq!(
                        [actual.red, actual.green, actual.blue, actual.alpha],
                        [expected.red, expected.green, expected.blue, expected.alpha],
                        "conversion modified the retained canvas"
                    );
                }
            }
        }
    }
}

struct Mapping {
    start: NonNull<u8>,
    length: usize,
}

impl Mapping {
    fn new(file: &File, length: usize) -> Result<Self, Error> {
        // SAFETY: fd lives in Framebuffer, length is checked against smem_len.
        let start = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if start == libc::MAP_FAILED {
            return Err(Error::io("map framebuffer", io::Error::last_os_error()));
        }
        Ok(Self {
            start: NonNull::new(start.cast()).expect("mmap returned null"),
            length,
        })
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: this object owns the mapping and only lends one mutable view.
        unsafe { std::slice::from_raw_parts_mut(self.start.as_ptr(), self.length) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: start and length are the original mmap allocation.
        unsafe {
            libc::munmap(self.start.as_ptr().cast(), self.length);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

fn row_spans(rectangles: &[Rect], y: usize, spans: &mut Vec<(usize, usize)>) {
    spans.clear();
    spans.extend(
        rectangles
            .iter()
            .filter(|r| r.y0 <= y && y < r.y1)
            .map(|r| (r.x0, r.x1)),
    );
    spans.sort_unstable();
    let mut used = 0;
    for i in 0..spans.len() {
        let (start, end) = spans[i];
        if used > 0 && start <= spans[used - 1].1 {
            spans[used - 1].1 = spans[used - 1].1.max(end);
        } else {
            spans[used] = (start, end);
            used += 1;
        }
    }
    spans.truncate(used);
}

pub(crate) struct Framebuffer {
    #[cfg(not(feature = "nx5-tde"))]
    retained: Vec<PremultipliedRgbaColor>,
    #[cfg(feature = "nx5-tde")]
    retained: crate::backends::nx5::tde::Buffer,
    // Retire TDE resources before closing the parent SDK/VO session.
    presentation: Presentation,
    mapping: Mapping,
    _file: File,
    pub(crate) layout: Layout,
    front: usize,
    initialized: [bool; 2],
    previous_damage: Vec<Rect>,
    pending_damage: Option<Vec<Rect>>,
    prepared_damage: Option<Vec<Rect>>,
    unsubmitted_damage: Vec<Rect>,
}

impl Framebuffer {
    pub(crate) fn open(path: &Path, contract: PresentationContract) -> Result<Self, Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| Error::io("open framebuffer", e))?;
        // Advisory exclusion for cooperating users; board admission additionally
        // requires exclusive ownership of the actual display hardware.
        crate::backends::lock_exclusive(file.as_fd(), "lock framebuffer")?;
        let mut fixed = FixedInfo::default();
        let mut variable = VariableInfo::default();
        // SAFETY: Linux UAPI buffers have the exact native ABI layouts above.
        if unsafe { libc::ioctl(file.as_raw_fd(), 0x4602, &mut fixed) } < 0
            || unsafe { libc::ioctl(file.as_raw_fd(), 0x4600, &mut variable) } < 0
        {
            return Err(Error::io(
                "read framebuffer layout",
                io::Error::last_os_error(),
            ));
        }
        let layout = (contract.validate_layout)(&fixed, &variable)?;
        Self::from_file(file, layout, contract)
    }

    pub(crate) fn from_file(
        file: File,
        layout: Layout,
        contract: PresentationContract,
    ) -> Result<Self, Error> {
        let mapping = Mapping::new(&file, layout.page_bytes * 2)?;
        let presentation = Presentation::new(
            file.try_clone()
                .map_err(|e| Error::io("duplicate framebuffer for presentation", e))?,
            layout,
            contract,
        )?;
        #[cfg(feature = "nx5-tde")]
        let retained =
            crate::backends::nx5::tde::Buffer::new(layout.size.width, layout.size.height)?;
        #[cfg(not(feature = "nx5-tde"))]
        let retained = {
            let count = layout.size.width as usize * layout.size.height as usize;
            let mut retained = Vec::new();
            retained
                .try_reserve_exact(count)
                .map_err(|_| Error::UnsupportedFramebuffer("software buffer allocation failed"))?;
            retained.resize(count, PremultipliedRgbaColor::default());
            retained
        };
        Ok(Self {
            _file: file,
            mapping,
            layout,
            presentation,
            retained,
            front: layout.initial_page,
            initialized: [false; 2],
            previous_damage: Vec::new(),
            pending_damage: None,
            prepared_damage: None,
            unsubmitted_damage: Vec::new(),
        })
    }

    pub(crate) fn render(&mut self, renderer: &SoftwareRenderer) -> Result<bool, Error> {
        self.poll_present()?;
        assert!(
            self.can_render(),
            "at most one rendered frame may wait for submission"
        );
        #[cfg(not(feature = "nx5-tde"))]
        let damage = renderer.render(&mut self.retained, self.layout.size.width as usize);
        #[cfg(feature = "nx5-tde")]
        let damage = self.retained.render(renderer)?;
        let mut current = std::mem::take(&mut self.unsubmitted_damage);
        current.extend(damage.iter().filter_map(|(origin, size)| {
            let rect = Rect {
                x0: (origin.x as i64).clamp(0, self.layout.size.width as i64) as usize,
                y0: (origin.y as i64).clamp(0, self.layout.size.height as i64) as usize,
                x1: (origin.x as i64 + size.width as i64).clamp(0, self.layout.size.width as i64)
                    as usize,
                y1: (origin.y as i64 + size.height as i64).clamp(0, self.layout.size.height as i64)
                    as usize,
            };
            (rect.x0 < rect.x1 && rect.y0 < rect.y1).then_some(rect)
        }));
        let changed = !current.is_empty();
        self.prepared_damage = Some(current);
        Ok(changed)
    }

    pub(crate) fn can_render(&self) -> bool {
        self.prepared_damage.is_none()
    }

    pub(crate) fn discard_prepared(&mut self) -> bool {
        if let Some(damage) = self.prepared_damage.take() {
            // The retained image and Slint's ReusedBuffer already include these
            // changes. Carry them into the next rendered/committed damage set.
            self.unsubmitted_damage.extend(damage);
            true
        } else {
            false
        }
    }

    pub(crate) fn ready_to_present(&self) -> bool {
        self.prepared_damage.is_some() && !self.presentation.pending()
    }

    pub(crate) fn wake(&self) -> Arc<WakeQueue> {
        self.presentation.wake.clone()
    }

    pub(crate) fn poll_present(&mut self) -> Result<(), Error> {
        self.complete_present(false)
    }

    pub(crate) fn wait_for_present(&mut self) -> Result<(), Error> {
        self.complete_present(true)
    }

    fn complete_present(&mut self, wait: bool) -> Result<(), Error> {
        if self.presentation.complete(wait)? {
            self.front = 1 - self.front;
            self.initialized[self.front] = true;
            self.previous_damage = self.pending_damage.take().expect("submitted damage");
        }
        Ok(())
    }

    pub(crate) fn present(&mut self) -> Result<bool, Error> {
        // Drawing touches only the retained canvas. Both physical pages stay
        // untouched until the preceding blocking submission has completed.
        self.wait_for_present()?;
        let Some(current) = self.prepared_damage.take() else {
            return Ok(false);
        };
        if current.is_empty() {
            return Ok(false);
        }
        let back = 1 - self.front;
        let areas = if !self.initialized[back] {
            vec![Rect {
                x0: 0,
                y0: 0,
                x1: self.layout.size.width as usize,
                y1: self.layout.size.height as usize,
            }]
        } else {
            current
                .iter()
                .chain(&self.previous_damage)
                .copied()
                .collect()
        };
        let layout = self.layout;
        #[cfg(not(feature = "nx5-tde"))]
        let pixels = &self.retained;
        #[cfg(feature = "nx5-tde")]
        let pixels = self.retained.pixels();
        let mapped = self.mapping.bytes_mut();
        let top = areas.iter().map(|r| r.y0).min().unwrap();
        let bottom = areas.iter().map(|r| r.y1).max().unwrap();
        let mut spans = Vec::with_capacity(areas.len());
        for y in top..bottom {
            // Repair the union of current/previous damage without expanding the
            // gaps between unrelated widgets or copying an overlap twice.
            row_spans(&areas, y, &mut spans);
            for &(start, end) in &spans {
                let offset =
                    back * layout.page_bytes + y * layout.stride + start * layout.bytes_per_pixel;
                let row = y * layout.size.width as usize;
                layout.write_row(
                    &pixels[row + start..row + end],
                    &mut mapped[offset..offset + (end - start) * layout.bytes_per_pixel],
                );
            }
        }
        // The device-specific contract must also perform any required DMA cache
        // maintenance. This fence alone does not make a cached mapping coherent.
        fence(Ordering::SeqCst);
        self.presentation.submit(back as u32 * layout.size.height)?;
        self.pending_damage = Some(current);
        Ok(true)
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        // Join before dropping the mapping or TDE resources. The presentation
        // owner then keeps the SDK alive until the retained renderer is dropped.
        self.presentation.shutdown();
        #[cfg(feature = "nx5-tde")]
        if self.retained.has_unknown_dma() {
            self.presentation.quarantine_session();
        }
    }
}

#[cfg(test)]
#[path = "tests/framebuffer_lifecycle.rs"]
mod lifecycle_tests;

#[cfg(all(test, feature = "nx5-tde"))]
#[path = "tests/nx5_writeback.rs"]
mod nx5_writeback;
