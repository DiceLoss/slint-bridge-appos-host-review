#[cfg(feature = "nx5")]
pub(crate) mod nx5;

use crate::{
    Backend, Error,
    framebuffer::{FixedInfo, VariableInfo},
};
use std::{io, os::fd::BorrowedFd, path::Path, time::Duration};

type CompletePresentation = dyn FnMut(BorrowedFd<'_>, u32) -> io::Result<()> + Send;

/// Private seam for a board's blocking presentation implementation. Success
/// transfers the previous scanout page back to the renderer. NX5 uses the
/// explicitly accepted, firmware-specific blocking PAN implementation.
/// No public API can create this value or supply a callback.
pub(crate) struct PresentationContract {
    pub(crate) complete: Box<CompletePresentation>,
    pub(crate) validate_layout: fn(&FixedInfo, &VariableInfo) -> Result<(), Error>,
}

/// Verified board resources and OS timing information. Timer precision affects
/// event scheduling only; it cannot establish display-page ownership.
pub(crate) struct Admission {
    pub(crate) presentation: PresentationContract,
    pub(crate) timer_quantum: Duration,
}

pub(crate) fn require(backend: Backend, path: &Path) -> Result<Admission, Error> {
    #[cfg(feature = "nx5")]
    if backend == Backend::Nx5 {
        return Ok(Admission {
            presentation: crate::backends::nx5::open(path)?,
            timer_quantum: crate::backends::nx5::TIMER_QUANTUM,
        });
    }
    let _ = path;
    Err(Error::DisplayNotAdmitted {
        backend,
        reason: match backend {
            Backend::Nx5 => "this build does not include the NX5 display implementation",
            Backend::Fbdev => "no framebuffer driver has a reviewed tear-free completion contract",
        },
    })
}
