#[cfg(feature = "f1")]
pub(crate) mod f1;
#[cfg(feature = "nx1")]
mod nx1;
#[cfg(feature = "nx5")]
pub(crate) mod nx5;
#[cfg(feature = "fbdev")]
mod x9;

use crate::{
    Backend, Error,
    framebuffer::{FixedInfo, Layout, VariableInfo},
};
use std::{
    io,
    os::fd::{AsRawFd, BorrowedFd},
    time::Duration,
};

pub(crate) fn lock_exclusive(fd: BorrowedFd<'_>, operation: &'static str) -> Result<(), Error> {
    // SAFETY: the borrowed descriptor remains valid throughout flock.
    if unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Err(Error::DisplayNotAdmitted {
            backend: crate::selected_backend(),
            reason: "display is already locked by another owner",
        })
    } else {
        Err(Error::io(operation, error))
    }
}

type ValidateLayout =
    dyn Fn(BorrowedFd<'_>, &FixedInfo, &VariableInfo) -> Result<Layout, Error> + Send;

type CompletePresentation = dyn FnMut(BorrowedFd<'_>, u32, &Layout) -> io::Result<()> + Send;

/// Private seam for a board's blocking presentation implementation. The board's
/// success convention transfers the previous page back to the renderer. NX5 uses
/// blocking PAN; F1 and NX1 follow explicitly selected reference conventions,
/// whose hardware retirement limitations are recorded in their board reports.
/// No public API can create this value or supply a callback.
pub(crate) struct PresentationContract {
    pub(crate) complete: Box<CompletePresentation>,
    pub(crate) validate_layout: Box<ValidateLayout>,
}

/// Checked board resources and OS timing information. Timer precision affects
/// event scheduling only; it cannot establish display-page ownership.
pub(crate) struct Admission {
    pub(crate) presentation: PresentationContract,
    pub(crate) timer_quantum: Duration,
}

pub(crate) fn require(backend: Backend) -> Result<Admission, Error> {
    #[cfg(feature = "nx1")]
    if backend == Backend::Nx1 {
        return nx1::open();
    }
    #[cfg(feature = "f1")]
    if backend == Backend::F1 {
        return f1::open();
    }
    #[cfg(feature = "nx5")]
    if backend == Backend::Nx5 {
        return Ok(Admission {
            presentation: crate::backends::nx5::open()?,
            timer_quantum: crate::backends::nx5::TIMER_QUANTUM,
        });
    }
    #[cfg(feature = "fbdev")]
    if backend == Backend::Fbdev {
        return Ok(Admission {
            presentation: x9::open()?,
            timer_quantum: Duration::ZERO,
        });
    }
    Err(Error::DisplayNotAdmitted {
        backend,
        reason: match backend {
            Backend::Nx1 => "this build does not include the NX1 integration target",
            Backend::Nx5 => "this build does not include the NX5 display implementation",
            Backend::Fbdev => "no framebuffer driver has a reviewed tear-free completion contract",
            Backend::F1 => "this build does not include the F1 integration target",
        },
    })
}
