//! Single-job VE PNG decoding on the admitted D211 firmware.
//! Register and IRQ semantics are documented in docs/f1-image-decoder-research.md.
use super::{Png, completed};
use crate::Error;
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{FileTypeExt, MetadataExt},
    },
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

static QUARANTINED: AtomicBool = AtomicBool::new(false);
const BITSTREAM: usize = 0;
const OUTPUT: usize = 1;
const LZ77: usize = 2;

#[repr(C)]
#[derive(Default)]
struct Allocation {
    len: u64,
    fd: u32,
    fd_flags: u32,
    flags: u64,
}
#[repr(C)]
struct Import {
    fd: i32,
    address: u32,
}
#[repr(C)]
struct Wait {
    milliseconds: i32,
    status: u32,
}
const _: () = {
    assert!(size_of::<Allocation>() == 24);
    assert!(size_of::<Import>() == 8);
    assert!(size_of::<Wait>() == 8);
};

fn ioctl<T>(file: &File, request: libc::c_ulong, value: &mut T) -> io::Result<()> {
    // SAFETY: every caller supplies the matching native DMA/VE UAPI structure.
    if unsafe { libc::ioctl(file.as_raw_fd(), request, value) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

struct Mapping {
    pointer: NonNull<u8>,
    length: usize,
}
impl Mapping {
    fn new(file: &File, length: usize) -> io::Result<Self> {
        // SAFETY: length comes from the checked allocation or VE register ABI.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let Some(pointer) = NonNull::new(ptr.cast()) else {
            // SAFETY: release the mapping which cannot form a Rust reference.
            unsafe {
                libc::munmap(ptr, length);
            }
            return Err(io::Error::other("null DMA mapping"));
        };
        Ok(Self { pointer, length })
    }
    fn bytes(&self) -> &[u8] {
        // SAFETY: callers use DMA mappings only while CPU-owned and initialized.
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.length) }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: exclusive initialized mapping, before submission or after completion.
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.length) }
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: the whole owning Job is retained instead of dropped on uncertain DMA.
        unsafe {
            libc::munmap(self.pointer.as_ptr().cast(), self.length);
        }
    }
}

struct Dma {
    file: File,
    mapping: Mapping,
    address: u32,
}
impl Dma {
    fn cache(&self, mut flags: u64) -> io::Result<()> {
        ioctl(&self.file, 0x4008_6200, &mut flags)
    }
}

struct Job {
    ve: File,
    registers: Mapping,
    buffers: Vec<Dma>,
    _lock: File,
    client: bool,
}

fn require_unowned(device: u64) -> io::Result<()> {
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let files = match fs::read_dir(entry.path().join("fd")) {
            Ok(files) => files,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for fd in files {
            match fs::metadata(fd?.path()) {
                Ok(info) if info.file_type().is_char_device() && info.rdev() == device => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "VE has another owner",
                    ));
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        match fs::read_to_string(entry.path().join("maps")) {
            Ok(maps) if maps.lines().any(|line| line.ends_with("/dev/aic_ve")) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "VE registers have another owner",
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

impl Job {
    fn new(png: &Png<'_>) -> io::Result<Self> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open("/run/slint-bridge-f1-ve.lock")?;
        // SAFETY: the file remains held for the entire VE operation.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if QUARANTINED.load(Ordering::Acquire) {
            return Err(io::Error::other(
                "PNG decoder was disabled before acquiring the lock",
            ));
        }
        let metadata = fs::metadata("/dev/aic_ve")?;
        if !metadata.file_type().is_char_device() {
            return Err(io::Error::other("VE is not a character device"));
        }
        require_unowned(metadata.rdev())?;
        let ve = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/aic_ve")?;
        let mut size = 0i32;
        ioctl(&ve, 0x8004_5603, &mut size)?;
        if size != 0x4000 {
            return Err(io::Error::other("unverified VE register layout"));
        }
        let registers = Mapping::new(&ve, size as usize)?;
        let mut job = Self {
            ve,
            registers,
            buffers: Vec::with_capacity(3),
            _lock: lock,
            client: false,
        };
        ioctl(&job.ve, 0x5600, &mut 0u32)?;
        job.client = true;
        let heap = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/dma_heap/reserved")?;
        for (index, size) in [png.bitstream_len(), png.output_len(), 32768]
            .into_iter()
            .enumerate()
        {
            let length = (size + 4095) & !4095;
            let mut allocation = Allocation {
                len: length as u64,
                fd_flags: (libc::O_RDWR | libc::O_CLOEXEC) as u32,
                ..Default::default()
            };
            ioctl(&heap, 0xc018_4800, &mut allocation)?;
            // SAFETY: successful DMA_HEAP_IOCTL_ALLOC returns a new owned descriptor.
            let file = unsafe { File::from_raw_fd(allocation.fd as i32) };
            let mut mapping = Mapping::new(&file, length)?;
            ioctl(&file, 0x4008_6200, &mut 1u64)?;
            if index == LZ77 {
                mapping.bytes_mut().fill(0);
            } else if index == OUTPUT {
                // VE's first-row filter workaround requires a zero previous row.
                // The completed job overwrites every visible output pixel.
                mapping.bytes_mut()[png.stride * (png.height as usize - 1)..png.output_len()]
                    .fill(0);
            }
            let mut import = Import {
                fd: file.as_raw_fd(),
                address: 0,
            };
            ioctl(&job.ve, 0xc008_5606, &mut import)?;
            job.buffers.push(Dma {
                file,
                mapping,
                address: import.address,
            });
            if import.address == 0
                || !import.address.is_multiple_of(4096)
                || import.address.checked_add(length as u32).is_none()
            {
                return Err(io::Error::other("invalid VE DMA address"));
            }
        }
        png.copy_idat(job.buffers[BITSTREAM].mapping.bytes_mut());
        job.buffers[BITSTREAM].mapping.bytes_mut()[png.idat_len..].fill(0);
        for buffer in &job.buffers {
            buffer.cache(7)?;
        }
        Ok(job)
    }

    fn read(&self, offset: usize) -> u32 {
        // SAFETY: checked register aperture and aligned constant offsets. Full
        // fences order normal memory and I/O in both directions on RISC-V.
        unsafe {
            core::arch::asm!("fence iorw,iorw", options(nostack));
            let value = self
                .registers
                .pointer
                .as_ptr()
                .add(offset)
                .cast::<u32>()
                .read_volatile();
            core::arch::asm!("fence iorw,iorw", options(nostack));
            value
        }
    }
    fn write(&self, offset: usize, value: u32) {
        // SAFETY: same register aperture and ordering contract as read().
        unsafe {
            core::arch::asm!("fence iorw,iorw", options(nostack));
            self.registers
                .pointer
                .as_ptr()
                .add(offset)
                .cast::<u32>()
                .write_volatile(value);
            core::arch::asm!("fence iorw,iorw", options(nostack));
        }
    }

    fn prepare(&self, png: &Png<'_>) -> io::Result<()> {
        self.write(0, 1);
        self.write(0xc, 0);
        for offset in [0x10, 0x14, 0x18] {
            self.write(offset, 0);
        }
        self.write(4, 0xffff_fccf);
        let deadline = Instant::now() + Duration::from_millis(20);
        while self.read(4) >> 16 != 0 {
            if Instant::now() >= deadline {
                return Err(io::Error::other(
                    "VE reset did not finish before submission",
                ));
            }
        }
        self.write(8, 1);
        self.write(0x18, 1);
        self.write(0xc50, 1);
        self.write(0xc00, 0);
        self.write(0xc04, 15);
        // WAIT consumes the driver's IRQ flag; clear stale completions before
        // START. A zero-time timeout is expected here, with no submitted job.
        let mut drained = false;
        for _ in 0..2 {
            match ioctl(
                &self.ve,
                0xc008_5602,
                &mut Wait {
                    milliseconds: 0,
                    status: 0,
                },
            ) {
                Err(e) if e.raw_os_error() == Some(libc::ETIMEDOUT) => {
                    drained = true;
                    break;
                }
                Err(e) => return Err(e),
                Ok(()) => {}
            }
        }
        if !drained {
            return Err(io::Error::other("VE has persistent pending interrupts"));
        }
        for (offset, value) in [
            (0xc10, 1 | u32::from(png.color) << 8),
            (0xc14, png.width | png.height << 16),
            (0xc18, png.stride as u32),
            (0xc1c, 1),
            (0xc30, self.buffers[OUTPUT].address),
            (0xc34, png.output_len() as u32),
            (0xc40, self.buffers[LZ77].address),
            (0xc20, self.buffers[BITSTREAM].address),
            (
                0xc24,
                self.buffers[BITSTREAM].address + self.buffers[BITSTREAM].mapping.length as u32 - 1,
            ),
            (0xc28, 16),
            (0xc2c, ((png.idat_len - 6) * 8) as u32),
            (0xc04, 15),
            (0xc00, 15),
            (0xc, 1),
        ] {
            self.write(offset, value);
        }
        Ok(())
    }

    fn run(&self, png: &Png<'_>) -> io::Result<()> {
        self.write(0xc08, 1);
        self.write(0xc48, 0x8000_0003);
        let mut wait = Wait {
            milliseconds: 1000,
            status: 0,
        };
        ioctl(&self.ve, 0xc008_5602, &mut wait)?;
        let written = self.read(0xc38);
        if !completed(wait.status, written, png) {
            return Err(io::Error::other(format!(
                "VE status={:#x}, output={written}",
                wait.status
            )));
        }
        self.write(0x18, 0);
        self.buffers[OUTPUT].cache(1)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        for buffer in self.buffers.drain(..) {
            let mut import = Import {
                fd: buffer.file.as_raw_fd(),
                address: buffer.address,
            };
            if let Err(error) = ioctl(&self.ve, 0x4008_5607, &mut import) {
                QUARANTINED.store(true, Ordering::Release);
                eprintln!("slint-bridge: F1 PNG import release failed: {error}");
            }
        }
        if self.client
            && let Err(error) = ioctl(&self.ve, 0x5601, &mut 0u32)
        {
            QUARANTINED.store(true, Ordering::Release);
            eprintln!("slint-bridge: F1 PNG client release failed: {error}");
        }
    }
}

pub(super) fn decode(data: &[u8]) -> Result<Option<slint::Image>, Error> {
    if QUARANTINED.load(Ordering::Acquire) {
        return Err(Error::AccelerationFailed(
            "F1 PNG decoder is quarantined".into(),
        ));
    }
    let Some(png) = Png::parse(data) else {
        return Ok(None);
    };
    if u64::from(png.width) * u64::from(png.height) < 32768 {
        return Ok(None);
    }
    let firmware = fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .zip(fs::read_to_string("/proc/sys/kernel/version").ok())
        .zip(fs::read("/sys/firmware/devicetree/base/compatible").ok());
    if !firmware.is_some_and(|((release, version), compatible)| {
        super::super::firmware(&release, &version, &compatible)
    }) {
        return Ok(None);
    }
    let available = fs::read_to_string("/proc/meminfo").ok().and_then(|text| {
        text.lines().find_map(|line| {
            line.strip_prefix("MemAvailable:")
                .and_then(|line| line.split_whitespace().next()?.parse::<usize>().ok())
        })
    });
    let required = png.bitstream_len()
        + ((png.output_len() + 4095) & !4095)
        + 32768
        + png.width as usize * png.height as usize * 4
        + 8 * 1024 * 1024;
    if available.is_none_or(|kib| kib.saturating_mul(1024) < required) {
        return Ok(None);
    }
    let job = match Job::new(&png) {
        Ok(job) => job,
        Err(error) => {
            if QUARANTINED.load(Ordering::Acquire) {
                return Err(Error::AccelerationFailed(format!(
                    "F1 PNG decoder disabled: {error}"
                )));
            }
            eprintln!("slint-bridge: F1 PNG unavailable ({error}); use software decoder");
            return Ok(None);
        }
    };
    if let Err(error) = job.prepare(&png) {
        eprintln!("slint-bridge: F1 PNG preparation failed ({error}); use software decoder");
        return Ok(None);
    }
    if let Err(error) = job.run(&png) {
        QUARANTINED.store(true, Ordering::Release);
        // Keep the VE client, lock, register mapping, imports and allocations.
        // Closing the driver is not a substitute for a DMA completion event.
        std::mem::forget(job);
        return Err(Error::AccelerationFailed(format!(
            "F1 PNG completion uncertain: {error}"
        )));
    }
    let image = png.image(job.buffers[OUTPUT].mapping.bytes());
    if std::env::var_os("SLINT_BRIDGE_TRACE").is_some() {
        let available = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let available = available
            .lines()
            .find(|line| line.starts_with("MemAvailable:"));
        eprintln!(
            "slint-bridge: f1 png {}x{} dma_bytes={} with_image_{:?}",
            png.width,
            png.height,
            job.buffers
                .iter()
                .map(|buffer| buffer.mapping.length)
                .sum::<usize>(),
            available,
        );
    }
    Ok(Some(image))
}
