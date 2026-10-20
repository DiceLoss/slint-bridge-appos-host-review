//! ArtInChip normal-mode UAPI, equivalent to the reference MPP GE operations.
//! No decoder/VO SDK is loaded. DMA storage is never a physical display page.
use super::{Canvas, DrawTextureArgs, PremultipliedRgbaColor, Rect, TexturePixelFormat};
use std::{
    fs::{File, OpenOptions},
    io,
    os::fd::{AsRawFd, FromRawFd},
    ptr::NonNull,
};

#[repr(C)]
#[derive(Default)]
struct Allocation {
    len: u64,
    fd: u32,
    fd_flags: u32,
    heap_flags: u64,
}

#[repr(C)]
#[derive(Default)]
struct MppBuffer {
    kind: i32,
    // MPP_DMA_BUF_FD selects this member of the fd/physical-address union.
    fd: [i32; 3],
    stride: [u32; 3],
    size: [i32; 2],
    crop_enabled: u32,
    crop: [i32; 4],
    format: i32,
    flags: u32,
}

#[repr(C)]
#[derive(Default)]
struct Fill {
    kind: i32,
    start_color: u32,
    end_color: u32,
    destination: MppBuffer,
    // ge_ctrl: alpha_en/rules, src/dst alpha mode/value, key/value, dither, flags.
    control: [u32; 10],
}

#[repr(C)]
#[derive(Default)]
struct Bitblt {
    source: MppBuffer,
    destination: MppBuffer,
    control: [u32; 10],
}

const _: () = {
    assert!(size_of::<Bitblt>() == 168);
    assert!(size_of::<Allocation>() == 24);
    assert!(size_of::<MppBuffer>() == 64);
    assert!(size_of::<Fill>() == 116);
};

fn ioctl<T>(file: &File, request: libc::c_ulong, data: &mut T) -> io::Result<()> {
    // SAFETY: callers pair each Linux/vendor request with its native UAPI buffer.
    if unsafe { libc::ioctl(file.as_raw_fd(), request, data) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

struct DmaBuffer {
    file: Option<File>,
    pointer: NonNull<PremultipliedRgbaColor>,
    count: usize,
    quarantined: bool,
}

impl DmaBuffer {
    fn new(count: usize) -> io::Result<Self> {
        let len = count
            .checked_mul(4)
            .ok_or_else(|| io::Error::other("DMA size overflow"))?;
        let heap = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/dma_heap/reserved")?;
        let mut allocation = Allocation {
            len: len as u64,
            fd_flags: (libc::O_RDWR | libc::O_CLOEXEC) as u32,
            ..Default::default()
        };
        ioctl(&heap, 0xc018_4800, &mut allocation)?; // DMA_HEAP_IOCTL_ALLOC
        // SAFETY: successful allocation returns a new owned DMA-BUF descriptor.
        let file = unsafe { File::from_raw_fd(allocation.fd as i32) };
        // SAFETY: mapping exactly the allocated length; only this owner uses it.
        let mapping = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if mapping == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let Some(pointer) = NonNull::new(mapping.cast()) else {
            // A zero address can be a valid mmap result but not a Rust slice.
            unsafe {
                libc::munmap(mapping, len);
            }
            return Err(io::Error::other("DMA mapping has a null address"));
        };
        let mut buffer = Self {
            file: Some(file),
            pointer,
            count,
            quarantined: false,
        };
        buffer.cache(1)?; // SDK CACHE_INVALID: DMA_BUF_SYNC_READ | START
        buffer.pixels_mut().fill(PremultipliedRgbaColor::default());
        Ok(buffer)
    }

    fn cache(&self, flags: u64) -> io::Result<()> {
        ioctl(self.file.as_ref().unwrap(), 0x4008_6200, &mut { flags })
    }

    fn pixels(&self) -> &[PremultipliedRgbaColor] {
        assert!(!self.quarantined);
        // SAFETY: initialized mapping of count repr(C) RGBA pixels, CPU-owned.
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.count) }
    }

    fn pixels_mut(&mut self) -> &mut [PremultipliedRgbaColor] {
        assert!(!self.quarantined);
        // SAFETY: exclusive owner, no DMA operation is in flight while borrowed.
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.count) }
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        if self.quarantined {
            // Neither unmap nor release storage whose device ownership is unknown.
            std::mem::forget(self.file.take());
        } else {
            // SAFETY: the mapping is owned here and all hardware access completed.
            unsafe {
                libc::munmap(self.pointer.as_ptr().cast(), self.count * 4);
            }
        }
    }
}

pub(super) struct Native {
    ge: Option<File>,
    buffer: DmaBuffer,
    sources: [Option<DmaBuffer>; 2],
    width: u32,
    height: u32,
}

impl Native {
    pub(super) fn new(width: u32, height: u32) -> io::Result<Self> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(io::Error::other("unsupported GE canvas size"));
        }
        let ge = OpenOptions::new().read(true).write(true).open("/dev/ge")?;
        let mut mode = -1i32;
        let mut version = 0u32;
        ioctl(&ge, 0x8004_4700, &mut version)?;
        ioctl(&ge, 0x8004_4701, &mut mode)?;
        if mode != 0 || version != 0x100 {
            return Err(io::Error::other(
                "unverified GE mode/version; normal mode 0x100 required",
            ));
        }
        let buffer = DmaBuffer::new(width as usize * height as usize)?;
        Ok(Self {
            ge: Some(ge),
            buffer,
            sources: [None, None],
            width,
            height,
        })
    }
}

impl Canvas for Native {
    fn pixels(&self) -> &[PremultipliedRgbaColor] {
        self.buffer.pixels()
    }
    fn pixels_mut(&mut self) -> &mut [PremultipliedRgbaColor] {
        self.buffer.pixels_mut()
    }

    fn texture(
        &mut self,
        args: &DrawTextureArgs,
        rects: &[Rect],
        slot: usize,
        upload: bool,
    ) -> Result<bool, String> {
        // The hardware path is proven only over opaque destination pixels.
        // It rounds RGB by at most one; transparent targets keep Slint's exact
        // premultiplied arithmetic and are not converted in place.
        for r in rects {
            for y in r.y..r.y + r.height {
                let start = (y * self.width + r.x) as usize;
                if self.buffer.pixels()[start..start + r.width as usize]
                    .iter()
                    .any(|p| p.alpha != 255)
                {
                    return Ok(false);
                }
            }
        }
        let data = args.source();
        let Some(pixel_stride) = super::source_pixel_stride(data.width, data.height) else {
            return Ok(false);
        };
        if self.sources[slot].is_none() {
            let Ok(source) = DmaBuffer::new(super::SOURCE_PIXELS) else {
                return Ok(false);
            };
            self.sources[slot] = Some(source);
        }
        let source = self.sources[slot].as_mut().unwrap();
        if upload {
            for y in 0..data.height as usize {
                let row = &data.data
                    [y * data.byte_stride..y * data.byte_stride + data.width as usize * 4];
                let target = &mut source.pixels_mut()[y * pixel_stride..(y + 1) * pixel_stride];
                let (target, padding) = target.split_at_mut(data.width as usize);
                padding.fill(PremultipliedRgbaColor::default());
                for (dst, src) in target.iter_mut().zip(row.as_chunks::<4>().0) {
                    let scale = |v: u8| (u16::from(v) * u16::from(args.alpha) / 255) as u8;
                    *dst = if data.pixel_format == TexturePixelFormat::RgbaPremultiplied {
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
                    };
                }
            }
            source
                .cache(7)
                .map_err(|e| format!("F1 GE source flush: {e}"))?;
        }
        self.buffer
            .cache(7)
            .map_err(|e| format!("F1 GE destination flush: {e}"))?;
        self.buffer.quarantined = true;
        source.quarantined = true;
        for r in rects {
            let mut blt = Bitblt {
                source: MppBuffer {
                    fd: [source.file.as_ref().unwrap().as_raw_fd(), -1, -1],
                    stride: [pixel_stride as u32 * 4, 0, 0],
                    size: [data.width as i32, data.height as i32],
                    crop_enabled: 1,
                    crop: [
                        (i64::from(r.x) - args.dst_x as i64) as i32,
                        (i64::from(r.y) - args.dst_y as i64) as i32,
                        r.width as i32,
                        r.height as i32,
                    ],
                    format: 1,
                    flags: 1 << 8,
                    ..Default::default()
                },
                // With opaque destination, straight and premultiplied bytes
                // coincide. Do not enable the driver's destination de/pre-multiply
                // path, which corrupts this firmware's blended RGB.
                destination: MppBuffer {
                    fd: [self.buffer.file.as_ref().unwrap().as_raw_fd(), -1, -1],
                    stride: [self.width * 4, 0, 0],
                    size: [self.width as i32, self.height as i32],
                    crop_enabled: 1,
                    crop: [r.x as i32, r.y as i32, r.width as i32, r.height as i32],
                    format: 1,
                    flags: 0,
                    ..Default::default()
                },
                // alpha_en=1 (header prose is reversed), GE_PD_NONE. The
                // driver's premultiplied-source branch selects Fs=1,Fd=1-Sa.
                control: [1, 0, 0, 255, 0, 255, 0, 0, 0, 0],
            };
            ioctl(self.ge.as_ref().unwrap(), 0x40a8_4703, &mut blt)
                .map_err(|e| format!("F1 GE bitblt completion unknown: {e}"))?;
        }
        self.buffer
            .cache(1)
            .map_err(|e| format!("F1 GE destination invalidate: {e}"))?;
        self.buffer.quarantined = false;
        source.quarantined = false;
        for r in rects {
            for y in r.y..r.y + r.height {
                let start = (y * self.width + r.x) as usize;
                for p in &mut self.buffer.pixels_mut()[start..start + r.width as usize] {
                    p.alpha = 255;
                }
            }
        }
        Ok(true)
    }

    fn fill(&mut self, color: PremultipliedRgbaColor, rects: &[Rect]) -> Result<bool, String> {
        self.buffer
            .cache(7)
            .map_err(|e| format!("F1 GE cache flush: {e}"))?;
        // Keep the mapping and GE owner if ANY submitted job or post-job cache
        // operation fails. Do not retry, CPU-fallback or free uncertain DMA.
        self.buffer.quarantined = true;
        for rect in rects {
            let mut fill = Fill {
                start_color: u32::from(color.alpha) << 24
                    | u32::from(color.red) << 16
                    | u32::from(color.green) << 8
                    | u32::from(color.blue),
                destination: MppBuffer {
                    fd: [self.buffer.file.as_ref().unwrap().as_raw_fd(), -1, -1],
                    stride: [self.width * 4, 0, 0],
                    size: [self.width as i32, self.height as i32],
                    crop_enabled: 1,
                    crop: [
                        rect.x as i32,
                        rect.y as i32,
                        rect.width as i32,
                        rect.height as i32,
                    ],
                    format: 1,     // MPP_FMT_ABGR_8888: little-endian RGBA canvas.
                    flags: 1 << 8, // MPP_BUF_IS_PREMULTIPLY (fill is opaque).
                    ..Default::default()
                },
                ..Default::default()
            };
            // Normal mode waits inside FILLRECT; a separate GE_SYNC is invalid.
            ioctl(self.ge.as_ref().unwrap(), 0x4074_4702, &mut fill)
                .map_err(|e| format!("F1 GE fill completion unknown: {e}"))?;
        }
        self.buffer
            .cache(1)
            .map_err(|e| format!("F1 GE cache invalidate: {e}"))?;
        self.buffer.quarantined = false;
        Ok(true)
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        if self.buffer.quarantined {
            std::mem::forget(self.ge.take());
            eprintln!("slint-bridge: F1 GE completion/cache state unknown; resources quarantined");
        }
    }
}

#[cfg(test)]
mod hardware;
