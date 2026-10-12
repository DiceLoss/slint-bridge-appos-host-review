//! NX5 SDK and framebuffer integration for blocking double-buffered operation.
//! Blocking PAN is the presentation path. Readback detects errors; it is not itself a
//! hardware retirement fence or proof of every physical LCD frame.
#[cfg(feature = "nx5-tde")]
pub(crate) mod tde;

use crate::{
    Backend, Error,
    backends::PresentationContract,
    framebuffer::{FixedInfo, Layout, VariableInfo},
};
use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd},
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

pub(crate) fn validate_layout(f: &FixedInfo, v: &VariableInfo) -> Result<Layout, Error> {
    Layout::new(f, v)
}

fn vo_enabled(state: &str) -> Result<bool, Error> {
    let devices = state
        .split_once("-----DEV CONFIG")
        .and_then(|(_, rest)| rest.split_once("-----VIDEO LAYER STATUS"))
        .ok_or_else(|| rejected("unrecognized NX5 VO status"))?
        .0;
    Ok(devices.lines().any(|line| {
        let mut columns = line.split_whitespace();
        columns.next().is_some_and(|id| id.parse::<u32>().is_ok()) && columns.next() == Some("Y")
    }))
}

struct Session {
    disable: unsafe extern "C" fn() -> libc::c_int,
    library: libloading::Library,
    _lock: File,
}

impl Session {
    fn load(
        lock: File,
        library_name: &OsStr,
        mut read_state: impl FnMut() -> io::Result<String>,
    ) -> Result<Self, Error> {
        // This firmware's library constructor resets SYS and enables VO. Do not
        // load it while another application owns an already enabled display.
        if vo_enabled(&read_state().map_err(|e| Error::io("read NX5 VO ownership", e))?)? {
            return Err(rejected(
                "NX5 VO is already enabled; release its current owner before init",
            ));
        }
        // SAFETY: NX5 integration requires the SDK ABI from codec/vd/VoApi.h.
        // VO_Enable/Disable return 1 for success, 0 for failure. Library loading and
        // symbol lookup report errors; they cannot verify ABI compatibility.
        let session = unsafe {
            // SYS/VB/MMZ descriptors are cached in SDK globals without an unload
            // destructor. Keep that image resident so retries reuse the cache;
            // Session still releases VO and its exclusive lock normally.
            let library: libloading::Library = libloading::os::unix::Library::open(
                Some(library_name),
                libc::RTLD_LAZY | libc::RTLD_LOCAL | libc::RTLD_NODELETE,
            )
            .map_err(|e| Error::io("load NX5 SDK", io::Error::other(e)))?
            .into();
            let disable = *library
                .get::<unsafe extern "C" fn() -> libc::c_int>(b"VO_Disable\0")
                .map_err(|e| Error::io("find VO_Disable", io::Error::other(e)))?;
            // Own the constructor's VO reference before another fallible step.
            let session = Session {
                disable,
                library,
                _lock: lock,
            };
            let enable = *session
                .library
                .get::<unsafe extern "C" fn() -> libc::c_int>(b"VO_Enable\0")
                .map_err(|e| Error::io("find VO_Enable", io::Error::other(e)))?;
            // A first dlopen may already enable VO. A resident or non-auto-
            // enabling SDK needs exactly one explicit acquire instead.
            if !vo_enabled(&read_state().map_err(|e| Error::io("read NX5 VO state", e))?)?
                && enable() == 0
            {
                return Err(Error::io(
                    "enable NX5 display",
                    io::Error::other("VO_Enable returned 0"),
                ));
            }
            session
        };
        Ok(session)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: exact SDK signature; the library and exclusive lock are alive.
        if unsafe { (self.disable)() } == 0 {
            eprintln!("slint-bridge: VO_Disable failed");
        }
    }
}

pub(crate) fn open() -> Result<PresentationContract, Error> {
    if !cfg!(all(target_arch = "arm", target_pointer_width = "32")) {
        return Err(rejected("NX5 requires ARM32"));
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open("/run/slint-bridge-nx5.lock")
        .map_err(|e| Error::io("open NX5 owner lock", e))?;
    crate::backends::lock_exclusive(lock.as_fd(), "lock NX5 display session")?;
    let session = Session::load(lock, OsStr::new(LIBRARY), || {
        fs::read_to_string("/proc/umap/vo")
    })?;
    let trace = std::env::var_os("SLINT_BRIDGE_TRACE").is_some();
    let mut count = 0u64;
    let mut batch = Instant::now();
    let mut blocking_us = 0u128;
    eprintln!("slint-bridge: NX5 VO enabled; blocking PAN, two pages");
    Ok(PresentationContract {
        validate_layout,
        complete: Box::new(move |fd, yoffset, layout| {
            let _keep_alive = &session;
            let start = Instant::now();
            present(fd, yoffset, layout)?;
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

fn validate_present(
    expected: &Layout,
    fixed: &FixedInfo,
    variable: &VariableInfo,
    yoffset: u32,
) -> io::Result<()> {
    let actual = Layout::new(fixed, variable).map_err(io::Error::other)?;
    if !expected.same_mode(actual) || (yoffset != 0 && yoffset != expected.size.height) {
        return Err(io::Error::other(
            "NX5 display layout changed or page offset is invalid",
        ));
    }
    Ok(())
}

fn present(fd: BorrowedFd<'_>, yoffset: u32, layout: &Layout) -> io::Result<()> {
    let mut fixed = FixedInfo::default();
    let mut requested = VariableInfo::default();
    let mut actual = VariableInfo::default();
    // SAFETY: native Linux fb UAPI layout; no concurrent submitter in this session.
    unsafe {
        if libc::ioctl(fd.as_raw_fd(), 0x4602, &mut fixed) < 0
            || libc::ioctl(fd.as_raw_fd(), 0x4600, &mut requested) < 0
        {
            return Err(io::Error::last_os_error());
        }
        validate_present(layout, &fixed, &requested, yoffset)?;
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
    validate_present(layout, &fixed, &actual, yoffset)?;
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
    fn accepts_device_reported_resolutions_and_padded_rows() {
        for (width, height) in [(800, 480), (1280, 720), (480, 800)] {
            let (fixed, variable) = crate::tests::info(width, height, 32);
            let layout = validate_layout(&fixed, &variable).unwrap();
            assert_eq!(layout.size, slint::PhysicalSize::new(width, height));
            assert_eq!(
                layout.page_bytes,
                fixed.line_length as usize * height as usize
            );
        }
    }

    #[test]
    fn presentation_uses_actual_page_height_and_rejects_changed_layouts() {
        let (mut fixed, mut variable) = crate::tests::info(800, 480, 32);
        let expected = validate_layout(&fixed, &variable).unwrap();
        for front in [0, 480] {
            variable.yoffset = front;
            for next in [0, 480] {
                validate_present(&expected, &fixed, &variable, next).unwrap();
            }
        }
        assert!(validate_present(&expected, &fixed, &variable, 600).is_err());
        variable.xres -= 1;
        assert!(validate_present(&expected, &fixed, &variable, 0).is_err());
        variable.xres += 1;
        fixed.line_length -= 4;
        assert!(validate_present(&expected, &fixed, &variable, 0).is_err());
        fixed.line_length += 4;
        std::mem::swap(&mut variable.red, &mut variable.blue);
        assert!(validate_present(&expected, &fixed, &variable, 0).is_err());
        std::mem::swap(&mut variable.red, &mut variable.blue);
        variable.yres_virtual = variable.yres;
        assert!(validate_layout(&fixed, &variable).is_err());
    }

    #[test]
    #[ignore = "requires exclusive current NX5; alternates existing pages without writing pixels"]
    fn blocking_flip_cadence() {
        let mut contract = open().unwrap();
        let path = std::env::var_os("SLINT_BRIDGE_TEST_FRAMEBUFFER")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| crate::Config::default().framebuffer);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        let mut fixed = FixedInfo::default();
        let mut actual = VariableInfo::default();
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0x4600, &mut actual) },
            0
        );
        assert_eq!(
            unsafe { libc::ioctl(file.as_raw_fd(), 0x4602, &mut fixed) },
            0
        );
        let layout = validate_layout(&fixed, &actual).unwrap();
        let mut times = Vec::new();
        for frame in 0..128 {
            let start = Instant::now();
            actual.yoffset = layout.size.height - actual.yoffset;
            (contract.complete)(file.as_fd(), actual.yoffset, &layout).unwrap();
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

#[cfg(test)]
mod session_tests;
