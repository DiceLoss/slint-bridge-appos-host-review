//! F1's reference-example PAN -> WAITFORVSYNC presentation sequence.
//! This follows the explicitly selected board convention, not a submission-ID
//! retirement fence. See docs/f1-kernel-analysis.md for the known limitations.
use crate::{
    Backend, Error,
    backends::{Admission, PresentationContract},
    framebuffer::{FixedInfo, Layout, VariableInfo},
};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd},
        unix::fs::{FileTypeExt, MetadataExt},
    },
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(feature = "f1-g2d")]
pub(crate) mod g2d;

fn rejected(reason: &'static str) -> Error {
    Error::DisplayNotAdmitted {
        backend: Backend::F1,
        reason,
    }
}

fn firmware(release: &str, version: &str, compatible: &[u8]) -> bool {
    release.trim() == "5.10.44"
        && version.trim() == "#1 Fri May 22 02:04:02 UTC 2026"
        && compatible
            .split(|b| *b == 0)
            .any(|name| name == b"artinchip,d211")
}

fn validate_layout(f: &FixedInfo, v: &VariableInfo) -> Result<Layout, Error> {
    let layout = Layout::new(f, v)?;
    if (
        v.xres,
        v.yres,
        v.xres_virtual,
        v.yres_virtual,
        f.line_length,
        f.smem_len,
    ) != (1024, 600, 1024, 1200, 4096, 4_915_200)
        || f.smem_start != 0x4244_0000
        || f.xpanstep != 0
        || f.ypanstep != 1
        || f.ywrapstep != 0
        || v.bits_per_pixel != 32
        || v.reserved[0] != 0
        || f.id[..7] != b"aicfb0\0".map(|byte| libc::c_char::from_ne_bytes([byte]))
        || [v.red, v.green, v.blue, v.alpha].map(|c| (c.offset, c.length))
            != [(16, 8), (8, 8), (0, 8), (24, 8)]
    {
        return Err(rejected("unverified F1 framebuffer layout"));
    }
    Ok(layout)
}

fn ioctl<T>(fd: BorrowedFd<'_>, request: libc::c_ulong, data: &mut T) -> io::Result<()> {
    // SAFETY: callers provide the matching Linux fbdev native UAPI buffer.
    if unsafe { libc::ioctl(fd.as_raw_fd(), request, data) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn snapshot(fd: BorrowedFd<'_>, expected: &Layout, y: u32) -> io::Result<VariableInfo> {
    let mut fixed = FixedInfo::default();
    let mut variable = VariableInfo::default();
    ioctl(fd, 0x4602, &mut fixed)?;
    ioctl(fd, 0x4600, &mut variable)?;
    let actual = validate_layout(&fixed, &variable).map_err(io::Error::other)?;
    if !actual.same_mode(*expected) || variable.yoffset != y {
        return Err(io::Error::other("F1 layout or visible page changed"));
    }
    Ok(variable)
}

// Keep sequencing testable without issuing host ioctls. Errors stop at their
// source; especially do not retry a PAN or reinterpret a failed wait as success.
fn reference_sequence(
    pan: impl FnOnce() -> io::Result<()>,
    wait: impl FnOnce() -> io::Result<()>,
    check: impl FnOnce() -> io::Result<()>,
) -> io::Result<(Duration, Duration)> {
    let start = Instant::now();
    pan()?;
    let pan_time = start.elapsed();
    let start = Instant::now();
    wait()?;
    let wait_time = start.elapsed();
    check()?;
    Ok((pan_time, wait_time))
}

fn present(
    fd: BorrowedFd<'_>,
    front: u32,
    next: u32,
    layout: &Layout,
) -> io::Result<(Duration, Duration)> {
    if next != 0 && next != layout.size.height {
        return Err(io::Error::other("invalid F1 page offset"));
    }
    let mut variable = snapshot(fd, layout, front)?;
    variable.yoffset = next;
    variable.reserved = [0; 4];
    reference_sequence(
        || ioctl(fd, 0x4606, &mut variable),
        || ioctl(fd, 0x4004_4620, &mut 0u32),
        || snapshot(fd, layout, next).map(|_| ()),
    )
}

fn ownership_error(error: io::Error) -> Error {
    if error.kind() == io::ErrorKind::AlreadyExists {
        eprintln!("slint-bridge: {error}");
        rejected("F1 display already has an owner")
    } else {
        Error::io("check F1 display ownership", error)
    }
}

fn require_unowned() -> io::Result<()> {
    let self_pid = std::process::id().to_string();
    let device = fs::metadata("/dev/fb0")?.rdev();
    for process in fs::read_dir("/proc")? {
        let process = process?;
        let name = process.file_name();
        let name = name.to_string_lossy();
        if name == self_pid || !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let descriptors = match fs::read_dir(process.path().join("fd")) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for descriptor in descriptors {
            match fs::metadata(descriptor?.path()) {
                Ok(metadata)
                    if metadata.file_type().is_char_device() && metadata.rdev() == device =>
                {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("F1 framebuffer owned by PID {name}"),
                    ));
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        match fs::read_to_string(process.path().join("maps")) {
            Ok(maps) if maps.lines().any(|line| line.ends_with("/dev/fb0")) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("F1 framebuffer mapped by PID {name}"),
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    for console in fs::read_dir("/sys/class/vtconsole")? {
        let console = console?.path();
        if fs::read_to_string(console.join("bind"))?.trim() == "1"
            && fs::read_to_string(console.join("name"))?.contains("frame buffer")
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "F1 framebuffer console is active",
            ));
        }
    }
    Ok(())
}

pub(crate) fn open() -> Result<Admission, Error> {
    if !cfg!(all(target_arch = "riscv64", target_pointer_width = "64")) {
        return Err(rejected("F1 requires RISC-V 64-bit Linux"));
    }
    let io_error = |e| Error::io("admit F1 display", e);
    if !firmware(
        &fs::read_to_string("/proc/sys/kernel/osrelease").map_err(io_error)?,
        &fs::read_to_string("/proc/sys/kernel/version").map_err(io_error)?,
        &fs::read("/sys/firmware/devicetree/base/compatible").map_err(io_error)?,
    ) {
        return Err(rejected("unverified F1 firmware/device"));
    }
    let lock = Arc::new(
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open("/run/slint-bridge-f1.lock")
            .map_err(io_error)?,
    );
    crate::backends::lock_exclusive(lock.as_fd(), "lock F1 display")?;
    require_unowned().map_err(ownership_error)?;
    let initial_lock = lock.clone();
    let trace = std::env::var_os("SLINT_BRIDGE_TRACE").is_some();
    let mut front = None;
    let mut count = 0u64;
    let mut batch_count = 0u64;
    let mut batch = Instant::now();
    let mut pan_us = 0u128;
    let mut wait_us = 0u128;
    eprintln!("slint-bridge: F1 reference PAN -> WAITFORVSYNC; two pages");
    Ok(Admission {
        timer_quantum: Duration::ZERO,
        presentation: PresentationContract {
            validate_layout: Box::new(move |fd, f, v| {
                let _owner = &initial_lock;
                let opened = File::from(fd.try_clone_to_owned().map_err(io_error)?)
                    .metadata()
                    .map_err(io_error)?;
                if !opened.file_type().is_char_device()
                    || opened.rdev() != fs::metadata("/dev/fb0").map_err(io_error)?.rdev()
                {
                    return Err(rejected("F1 requires the inspected framebuffer device"));
                }
                require_unowned().map_err(ownership_error)?;
                let layout = validate_layout(f, v)?;
                // Establish the reference sequence before either page is written.
                present(fd, v.yoffset, v.yoffset, &layout).map_err(io_error)?;
                Ok(layout)
            }),
            complete: Box::new(move |fd, y, layout| {
                let _owner = &lock;
                let previous = front.unwrap_or(layout.initial_page as u32 * layout.size.height);
                if y == previous {
                    return Err(io::Error::other("F1 must alternate pages"));
                }
                let (pan, wait) = present(fd, previous, y, layout)?;
                front = Some(y);
                if trace {
                    count += 1;
                    batch_count += 1;
                    pan_us += pan.as_micros();
                    wait_us += wait.as_micros();
                    let elapsed = batch.elapsed();
                    if elapsed >= Duration::from_secs(1) {
                        let mut now = libc::timespec {
                            tv_sec: 0,
                            tv_nsec: 0,
                        };
                        // SAFETY: valid timespec; timestamp is diagnostic only.
                        unsafe {
                            libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now);
                        }
                        eprintln!(
                            "slint-bridge: f1 mono_us={} presented={count} fps={:.2} pan_avg_ms={:.3} wait_avg_ms={:.3}",
                            now.tv_sec as u64 * 1_000_000 + now.tv_nsec as u64 / 1000,
                            batch_count as f64 / elapsed.as_secs_f64(),
                            pan_us as f64 / batch_count as f64 / 1000.,
                            wait_us as f64 / batch_count as f64 / 1000.
                        );
                        batch = Instant::now();
                        batch_count = 0;
                        pan_us = 0;
                        wait_us = 0;
                    }
                }
                Ok(())
            }),
        },
    })
}

#[cfg(test)]
mod tests;
