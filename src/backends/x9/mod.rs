//! CPU rendering into the existing Lombo fbdev pages. DRM supplies only the
//! matching legacy flip completion event; no DRM buffer is allocated.
use crate::{
    Backend, Error,
    backends::PresentationContract,
    framebuffer::{FixedInfo, Layout, VariableInfo},
};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd},
        unix::fs::MetadataExt,
    },
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

mod uapi;
use uapi::*;

fn rejected(reason: &'static str) -> Error {
    Error::DisplayNotAdmitted {
        backend: Backend::Fbdev,
        reason,
    }
}

// This admission is intentionally specific to the inspected firmware. A driver
// name alone does not establish the vendor's completion/cache behavior.
fn firmware(release: &str, version: &str) -> bool {
    release.trim() == "3.10.100"
        && version.trim() == "#452 SMP PREEMPT Mon Dec 12 11:32:50 CST 2022"
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
    ) != (800, 1280, 800, 2560, 3200, 8_192_000)
        || f.smem_start != 0x5010_0000
        || v.bits_per_pixel != 32
        || [v.red, v.green, v.blue, v.alpha].map(|c| (c.offset, c.length))
            != [(16, 8), (8, 8), (0, 8), (24, 8)]
    {
        return Err(rejected("unverified X9 framebuffer layout"));
    }
    Ok(layout)
}

struct Console {
    file: File,
    previous: libc::c_int,
    restore: Arc<AtomicBool>,
}
impl Console {
    fn acquire() -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty0")?;
        let mut previous = 0;
        ioctl(file.as_fd(), 0x4b3b, &mut previous)?; // KDGETMODE
        if previous != 0 {
            return Err(io::Error::other("X9 console is already in graphics mode"));
        }
        // SAFETY: KDSETMODE takes an integer, not a pointer. Suppress fbcon writes.
        if unsafe { libc::ioctl(file.as_raw_fd(), 0x4b3a, 1) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            file,
            previous,
            restore: Arc::new(AtomicBool::new(true)),
        })
    }
}
impl Drop for Console {
    fn drop(&mut self) {
        if self.restore.load(Ordering::Relaxed) {
            // SAFETY: restore the mode read from this console while owning it.
            if unsafe { libc::ioctl(self.file.as_raw_fd(), 0x4b3a, self.previous) } < 0 {
                eprintln!(
                    "slint-bridge: X9 console restore failed: {}",
                    io::Error::last_os_error()
                );
            }
        } else {
            eprintln!("slint-bridge: X9 completion unknown; console remains in graphics mode");
        }
    }
}

#[derive(Clone, Copy)]
struct Display {
    crtc: Crtc,
}
impl Display {
    fn inspect(drm: BorrowedFd<'_>) -> io::Result<Self> {
        let mut resources = Resources::default();
        ioctl(drm, request::<Resources>(0xa0), &mut resources)?;
        if resources.count_crtcs != 1 {
            return Err(io::Error::other("X9 requires exactly one CRTC"));
        }
        let mut id = 0u32;
        let mut resources = Resources {
            crtc_id_ptr: (&mut id as *mut u32) as u64,
            count_crtcs: 1,
            ..Default::default()
        };
        ioctl(drm, request::<Resources>(0xa0), &mut resources)?;
        if resources.count_crtcs != 1 || id == 0 {
            return Err(io::Error::other("X9 CRTC changed"));
        }
        let mut crtc = Crtc {
            crtc_id: id,
            ..Default::default()
        };
        ioctl(drm, request::<Crtc>(0xa1), &mut crtc)?;
        if crtc.fb_id == 0 || crtc.mode_valid != 1 {
            return Err(io::Error::other("X9 has no active framebuffer"));
        }
        let mut fb = Fb {
            fb_id: crtc.fb_id,
            ..Default::default()
        };
        ioctl(drm, request::<Fb>(0xad), &mut fb)?;
        if fb.handle != 0 {
            ioctl(
                drm,
                0x4008_6409,
                &mut GemClose {
                    handle: fb.handle,
                    pad: 0,
                },
            )?;
        }
        if (fb.width, fb.height, fb.pitch, fb.bpp, fb.depth) != (800, 2560, 3200, 32, 32) {
            return Err(io::Error::other(
                "X9 DRM framebuffer differs from verified fbdev layout",
            ));
        }
        Ok(Self { crtc })
    }

    fn check(
        self,
        drm: BorrowedFd<'_>,
        fb: BorrowedFd<'_>,
        expected: &Layout,
        y: u32,
    ) -> io::Result<VariableInfo> {
        let mut fixed = FixedInfo::default();
        let mut variable = VariableInfo::default();
        ioctl(fb, 0x4602, &mut fixed)?;
        ioctl(fb, 0x4600, &mut variable)?;
        let layout = validate_layout(&fixed, &variable).map_err(io::Error::other)?;
        let mut crtc = Crtc {
            crtc_id: self.crtc.crtc_id,
            ..Default::default()
        };
        ioctl(drm, request::<Crtc>(0xa1), &mut crtc)?;
        if !layout.same_mode(*expected)
            || variable.yoffset != y
            || crtc.y != y
            || crtc.x != 0
            || crtc.fb_id != self.crtc.fb_id
            || crtc.mode_valid != 1
            || crtc.mode != self.crtc.mode
            || crtc.mode.hdisplay != 800
            || crtc.mode.vdisplay != 1280
        {
            return Err(io::Error::other(format!(
                "X9 scanout changed: expected y={y} fb={}, actual fbdev y={} CRTC x={} y={} fb={} valid={} mode={}x{} same_mode={} same_layout={}",
                self.crtc.fb_id,
                variable.yoffset,
                crtc.x,
                crtc.y,
                crtc.fb_id,
                crtc.mode_valid,
                crtc.mode.hdisplay,
                crtc.mode.vdisplay,
                crtc.mode == self.crtc.mode,
                layout.same_mode(*expected)
            )));
        }
        Ok(variable)
    }

    fn fence(self, drm: BorrowedFd<'_>, cookie: u64) -> io::Result<()> {
        let mut flip = Flip {
            crtc_id: self.crtc.crtc_id,
            fb_id: self.crtc.fb_id,
            flags: 1,
            reserved: 0,
            user_data: cookie,
        };
        ioctl(drm, request::<Flip>(0xb0), &mut flip)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "X9 flip completion timeout",
                ));
            }
            let mut poll = libc::pollfd {
                fd: drm.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: a single valid pollfd; timeout is bounded to two seconds.
            let result = unsafe { libc::poll(&mut poll, 1, remaining.as_millis().max(1) as i32) };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if result == 0 {
                continue;
            }
            if poll.revents != libc::POLLIN {
                return Err(io::Error::other("X9 DRM poll failed"));
            }
            let mut events = [0u8; 4096];
            // SAFETY: writable byte array and a live nonblocking DRM descriptor.
            let count =
                unsafe { libc::read(drm.as_raw_fd(), events.as_mut_ptr().cast(), events.len()) };
            if count < 0 {
                let error = io::Error::last_os_error();
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) {
                    continue;
                }
                return Err(error);
            }
            return completion(&events[..count as usize], cookie);
        }
    }
}

// One outstanding request on our own file description. v3.10's final word is
// reserved, not crtc_id, and this vendor reports sequence=0 for every event.
fn completion(bytes: &[u8], cookie: u64) -> io::Result<()> {
    if bytes.len() != 32
        || u32::from_ne_bytes(bytes[0..4].try_into().unwrap()) != 2
        || u32::from_ne_bytes(bytes[4..8].try_into().unwrap()) != 32
        || u64::from_ne_bytes(bytes[8..16].try_into().unwrap()) != cookie
    {
        return Err(io::Error::other(
            "X9 missing, malformed, or unmatched flip completion",
        ));
    }
    Ok(())
}

pub(crate) fn open() -> Result<PresentationContract, Error> {
    if !cfg!(all(target_arch = "arm", target_pointer_width = "32")) {
        return Err(rejected("X9 requires ARM32"));
    }
    let io_error = |e| Error::io("admit X9 display", e);
    if !firmware(
        &fs::read_to_string("/proc/sys/kernel/osrelease").map_err(io_error)?,
        &fs::read_to_string("/proc/sys/kernel/version").map_err(io_error)?,
    ) {
        return Err(rejected("unverified X9 kernel firmware"));
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open("/run/slint-bridge-x9.lock")
        .map_err(io_error)?;
    crate::backends::lock_exclusive(lock.as_fd(), "lock X9 display")?;
    use std::os::unix::fs::OpenOptionsExt;
    let drm = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open("/dev/dri/card0")
        .map_err(io_error)?;
    let mut name = [0u8; 32];
    let mut version = Version {
        name_len: name.len(),
        name: name.as_mut_ptr(),
        ..Default::default()
    };
    ioctl(drm.as_fd(), request::<Version>(0), &mut version).map_err(io_error)?;
    if version.name_len != 5
        || &name[..5] != b"lombo"
        || (version.major, version.minor, version.patch) != (1, 0, 0)
    {
        return Err(rejected("unverified X9 DRM driver"));
    }
    // SAFETY: DRM_SET_MASTER has no payload; an existing owner causes failure.
    if unsafe { libc::ioctl(drm.as_raw_fd(), 0x641e) } < 0 {
        return Err(io_error(io::Error::last_os_error()));
    }
    let display = Display::inspect(drm.as_fd()).map_err(io_error)?;
    let console = Console::acquire().map_err(io_error)?;
    let initial_drm = drm
        .try_clone()
        .map_err(|e| Error::io("duplicate X9 DRM descriptor", e))?;
    // Use the page verified after fbdev opens and the initial fence completes,
    // rather than the offset captured during the earlier DRM inspection.
    let mut front = None;
    let initial_restore = console.restore.clone();
    let mut cookie = 0x5839_0000_0000u64;
    Ok(PresentationContract {
        validate_layout: Box::new(move |fd, f, v| {
            let layout = validate_layout(f, v)?;
            let validate = || -> io::Result<()> {
                // Match the actual opened framebuffer, not its caller-provided path.
                let metadata = File::from(fd.try_clone_to_owned()?).metadata()?;
                let device = metadata.rdev();
                let fb_device = fs::canonicalize(format!(
                    "/sys/dev/char/{}:{}/device",
                    libc::major(device),
                    libc::minor(device)
                ))?;
                if fb_device != fs::canonicalize("/sys/class/drm/card0/device")? {
                    return Err(io::Error::other("framebuffer and DRM device differ"));
                }
                display.check(initial_drm.as_fd(), fd, &layout, v.yoffset)?;
                initial_restore.store(false, Ordering::Relaxed);
                display.fence(initial_drm.as_fd(), cookie)?;
                display.check(initial_drm.as_fd(), fd, &layout, v.yoffset)?;
                initial_restore.store(true, Ordering::Relaxed);
                Ok(())
            };
            validate().map_err(|e| Error::io("validate X9 initial scanout", e))?;
            Ok(layout)
        }),
        complete: Box::new(move |fd, y, layout| {
            let _owner = (&lock, &console);
            if !console.restore.load(Ordering::Relaxed) {
                return Err(io::Error::other("X9 completion previously failed"));
            }
            if y != 0 && y != layout.size.height {
                return Err(io::Error::other("invalid X9 page offset"));
            }
            let mut variable = display.check(
                drm.as_fd(),
                fd,
                layout,
                front.unwrap_or(layout.initial_page as u32 * layout.size.height),
            )?;
            variable.yoffset = y;
            console.restore.store(false, Ordering::Relaxed);
            ioctl(fd, 0x4606, &mut variable)?;
            cookie += 1;
            display.fence(drm.as_fd(), cookie)?;
            display.check(drm.as_fd(), fd, layout, y)?;
            front = Some(y);
            console.restore.store(true, Ordering::Relaxed);
            Ok(())
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_matching_flip_retires_old_page() {
        let cookie = 0x5839_1234_5678u64;
        let mut event = [0u8; 32];
        event[..4].copy_from_slice(&2u32.to_ne_bytes());
        event[4..8].copy_from_slice(&32u32.to_ne_bytes());
        event[8..16].copy_from_slice(&cookie.to_ne_bytes());
        completion(&event, cookie).unwrap(); // zero sequence is valid on this firmware
        for n in 0..32 {
            assert!(completion(&event[..n], cookie).is_err());
        }
        assert!(completion(&[event, event].concat(), cookie).is_err());
        assert!(completion(&event, cookie + 1).is_err());
        event[0] = 1; // ordinary vblank is not a flip completion
        assert!(completion(&event, cookie).is_err());
        event[0] = 2;
        event[4] = 31;
        assert!(completion(&event, cookie).is_err());
    }
    #[test]
    fn rejects_unverified_firmware_and_layout() {
        assert!(firmware(
            "3.10.100\n",
            "#452 SMP PREEMPT Mon Dec 12 11:32:50 CST 2022\n"
        ));
        assert!(!firmware("3.10.100", "#453"));
        let (mut f, v) = crate::tests::info(800, 1280, 32);
        f.smem_start = 0x5010_0000;
        f.line_length = 3200;
        f.smem_len = 8_192_000;
        validate_layout(&f, &v).unwrap();
        f.smem_start += 4096;
        assert!(validate_layout(&f, &v).is_err());
    }
}
