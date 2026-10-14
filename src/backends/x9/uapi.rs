//! Linux DRM UAPI. Pointers in mode ioctls are fixed u64, while VERSION uses
//! native pointers/size_t. Keep both ARM32 and host layouts covered by tests.
use std::{
    io,
    os::fd::{AsRawFd, BorrowedFd},
};

pub(super) const fn request<T>(number: u8) -> libc::c_ulong {
    (0xc000_6400u32 | (size_of::<T>() as u32) << 16 | number as u32) as libc::c_ulong
}
pub(super) fn ioctl<T>(
    fd: BorrowedFd<'_>,
    request: libc::c_ulong,
    value: &mut T,
) -> io::Result<()> {
    // SAFETY: callers pair a Linux UAPI request with its matching repr(C) value.
    if unsafe { libc::ioctl(fd.as_raw_fd(), request, value) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[repr(C)]
#[derive(Default)]
pub(super) struct Version {
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
    pub name_len: usize,
    pub name: *mut u8,
    pub date_len: usize,
    pub date: *mut u8,
    pub desc_len: usize,
    pub desc: *mut u8,
}
#[repr(C)]
#[derive(Default)]
pub(super) struct Resources {
    pub fb_id_ptr: u64,
    pub crtc_id_ptr: u64,
    pub connector_id_ptr: u64,
    pub encoder_id_ptr: u64,
    pub count_fbs: u32,
    pub count_crtcs: u32,
    pub count_connectors: u32,
    pub count_encoders: u32,
    pub min_width: u32,
    pub max_width: u32,
    pub min_height: u32,
    pub max_height: u32,
}
#[repr(C)]
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Mode {
    pub clock: u32,
    pub hdisplay: u16,
    pub hsync_start: u16,
    pub hsync_end: u16,
    pub htotal: u16,
    pub hskew: u16,
    pub vdisplay: u16,
    pub vsync_start: u16,
    pub vsync_end: u16,
    pub vtotal: u16,
    pub vscan: u16,
    pub vrefresh: u32,
    pub flags: u32,
    pub kind: u32,
    pub name: [u8; 32],
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub(super) struct Crtc {
    pub connectors: u64,
    pub count_connectors: u32,
    pub crtc_id: u32,
    pub fb_id: u32,
    pub x: u32,
    pub y: u32,
    pub gamma_size: u32,
    pub mode_valid: u32,
    pub mode: Mode,
}
#[repr(C)]
#[derive(Default)]
pub(super) struct Fb {
    pub fb_id: u32,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub depth: u32,
    pub handle: u32,
}
#[repr(C)]
pub(super) struct GemClose {
    pub handle: u32,
    pub pad: u32,
}
#[repr(C)]
pub(super) struct Flip {
    pub crtc_id: u32,
    pub fb_id: u32,
    pub flags: u32,
    pub reserved: u32,
    pub user_data: u64,
}
#[cfg(test)]
#[test]
fn uapi_sizes_match_linux() {
    assert_eq!(size_of::<Resources>(), 64);
    assert_eq!(size_of::<Mode>(), 68);
    assert_eq!(size_of::<Crtc>(), 104);
    assert_eq!(size_of::<Fb>(), 28);
    assert_eq!(size_of::<Flip>(), 24);
    assert_eq!(
        size_of::<Version>(),
        if size_of::<usize>() == 4 { 36 } else { 64 }
    );
}
