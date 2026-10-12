//! NX5 SDK and framebuffer integration for blocking double-buffered operation.
//! Blocking PAN is the presentation path. Readback detects errors; it is not itself a
//! hardware retirement fence or proof of every physical LCD frame.
#[cfg(feature = "nx5-tde")]
pub(crate) mod tde;

use crate::{
    Backend, Error,
    backends::PresentationContract,
    framebuffer::{FixedInfo, VariableInfo},
};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::fd::{AsRawFd, BorrowedFd},
    path::Path,
    time::{Duration, Instant},
};

// The measured #233 kernel reports 10 ms hrtimer resolution. This scheduling
// parameter is not a framebuffer completion fence or a firmware version check.
pub(crate) const TIMER_QUANTUM: Duration = Duration::from_millis(10);

// Resolve the SDK through the system loader (including LD_LIBRARY_PATH).
const LIBRARY: &str = "libVideoDec.so";

fn rejected(reason: &'static str) -> Error {
    Error::DisplayNotAdmitted {
        backend: Backend::Nx5,
        reason,
    }
}

fn verify_target(path: &Path) -> Result<(), Error> {
    if !cfg!(all(target_arch = "arm", target_pointer_width = "32")) || path != Path::new("/dev/fb0")
    {
        return Err(rejected("NX5 requires ARM32 and /dev/fb0"));
    }
    Ok(())
}

pub(crate) fn validate_layout(f: &FixedInfo, v: &VariableInfo) -> Result<(), Error> {
    if v.xres != 1024
        || v.yres != 600
        || v.bits_per_pixel != 32
        || f.line_length != 4096
        || v.yres_virtual != 1200
        || [v.red.offset, v.green.offset, v.blue.offset, v.alpha.offset] != [16, 8, 0, 24]
        || [v.red.length, v.green.length, v.blue.length, v.alpha.length] != [8, 8, 8, 8]
    {
        return Err(Error::UnsupportedFramebuffer(
            "NX5 requires 1024x600 ARGB8888, stride 4096 and two pages",
        ));
    }
    Ok(())
}

struct Session {
    disable: unsafe extern "C" fn() -> libc::c_int,
    _library: libloading::Library,
    _lock: File,
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: exact SDK signature; the library and exclusive lock are alive.
        if unsafe { (self.disable)() } == 0 {
            eprintln!("slint-bridge: VO_Disable failed");
        }
    }
}

pub(crate) fn open(path: &Path) -> Result<PresentationContract, Error> {
    verify_target(path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open("/run/slint-bridge-nx5.lock")
        .map_err(|e| Error::io("open NX5 owner lock", e))?;
    // SAFETY: lock is owned until the display session is dropped.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } < 0 {
        return Err(Error::io(
            "lock NX5 display session",
            io::Error::last_os_error(),
        ));
    }
    // This firmware's library constructor resets SYS and enables VO. Do not
    // load it while another application owns an already enabled display.
    let state =
        fs::read_to_string("/proc/umap/vo").map_err(|e| Error::io("read NX5 VO ownership", e))?;
    let devices = state
        .split_once("-----DEV CONFIG")
        .and_then(|(_, rest)| rest.split_once("-----VIDEO LAYER STATUS"))
        .ok_or_else(|| rejected("unrecognized NX5 VO status"))?
        .0;
    if devices.lines().any(|line| {
        let mut columns = line.split_whitespace();
        columns.next().is_some_and(|id| id.parse::<u32>().is_ok()) && columns.next() == Some("Y")
    }) {
        return Err(rejected(
            "NX5 VO is already enabled; release its current owner before init",
        ));
    }
    // SAFETY: NX5 integration requires the SDK ABI from codec/vd/VoApi.h.
    // VO_Enable/Disable return 1 for success, 0 for failure. Library loading and
    // symbol lookup report errors; they cannot verify ABI compatibility.
    let session = unsafe {
        let library = libloading::Library::new(LIBRARY)
            .map_err(|e| Error::io("load NX5 SDK", io::Error::other(e)))?;
        let enable = *library
            .get::<unsafe extern "C" fn() -> libc::c_int>(b"VO_Enable\0")
            .map_err(|e| Error::io("find VO_Enable", io::Error::other(e)))?;
        let disable = *library
            .get::<unsafe extern "C" fn() -> libc::c_int>(b"VO_Disable\0")
            .map_err(|e| Error::io("find VO_Disable", io::Error::other(e)))?;
        if enable() == 0 {
            return Err(Error::io(
                "enable NX5 display",
                io::Error::other("VO_Enable returned 0"),
            ));
        }
        Session {
            disable,
            _library: library,
            _lock: lock,
        }
    };
    let trace = std::env::var_os("SLINT_BRIDGE_TRACE").is_some();
    let mut count = 0u64;
    let mut batch = Instant::now();
    let mut blocking_us = 0u128;
    eprintln!("slint-bridge: NX5 VO enabled; blocking PAN, two pages");
    Ok(PresentationContract {
        validate_layout,
        complete: Box::new(move |fd, yoffset| {
            let _keep_alive = &session;
            let start = Instant::now();
            present(fd, yoffset)?;
            if trace {
                count += 1;
                blocking_us += start.elapsed().as_micros();
                if count.is_multiple_of(150) {
                    eprintln!(
                        "slint-bridge: presented={count} fps={:.2} flip_avg_ms={:.3} yoffset={yoffset}",
                        150.0 / batch.elapsed().as_secs_f64(),
                        blocking_us as f64 / 150_000.0
                    );
                    batch = Instant::now();
                    blocking_us = 0;
                }
            }
            Ok(())
        }),
    })
}

fn present(fd: BorrowedFd<'_>, yoffset: u32) -> io::Result<()> {
    let mut requested = VariableInfo::default();
    let mut actual = VariableInfo::default();
    // SAFETY: native Linux fb UAPI layout; no concurrent submitter in this session.
    unsafe {
        if libc::ioctl(fd.as_raw_fd(), 0x4600, &mut requested) < 0 {
            return Err(io::Error::last_os_error());
        }
        if requested.xres != 1024 || requested.yres != 600 || !matches!(yoffset, 0 | 600) {
            return Err(io::Error::other("NX5 display geometry changed"));
        }
        requested.yoffset = yoffset;
        // NX5 integration requires PAN to wait for completion, as measured on
        // #233 fyfb. Change only the page offset, preserving mode flags and
        // avoiding a per-frame modeset via FBIOPUT_VSCREENINFO.
        if libc::ioctl(fd.as_raw_fd(), 0x4606, &requested) < 0
            || libc::ioctl(fd.as_raw_fd(), 0x4600, &mut actual) < 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    if actual.yoffset != yoffset {
        return Err(io::Error::other("NX5 page readback mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsFd;

    #[test]
    #[ignore = "requires exclusive current NX5; alternates existing pages without writing pixels"]
    fn blocking_flip_cadence() {
        let mut contract = open(Path::new("/dev/fb0")).unwrap();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/fb0")
            .unwrap();
        let mut actual = VariableInfo::default();
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0x4600, &mut actual) },
            0
        );
        assert!(matches!(actual.yoffset, 0 | 600));
        let mut times = Vec::new();
        for frame in 0..128 {
            let start = Instant::now();
            actual.yoffset = 600 - actual.yoffset;
            (contract.complete)(file.as_fd(), actual.yoffset).unwrap();
            let elapsed = start.elapsed().as_micros();
            if frame >= 8 {
                times.push(elapsed);
            }
        }
        times.sort_unstable();
        let mean = times.iter().sum::<u128>() as f64 / times.len() as f64;
        println!(
            "flip_cadence samples={} mean_us={mean:.3} min_us={} p50_us={} p95_us={} max_us={} max_fps={:.3}",
            times.len(),
            times[0],
            times[times.len() / 2],
            times[times.len() * 95 / 100],
            times[times.len() - 1],
            1_000_000. / mean
        );
    }
}
