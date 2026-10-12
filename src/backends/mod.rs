#[cfg(feature = "nx5")]
pub(crate) mod nx5;

use crate::{
    Backend, Error,
    framebuffer::{FixedInfo, Layout, VariableInfo},
};
use std::{io, os::fd::BorrowedFd, time::Duration};

type CompletePresentation = dyn FnMut(BorrowedFd<'_>, u32, &Layout) -> io::Result<()> + Send;

/// Private seam for a board's blocking presentation implementation. Success
/// transfers the previous scanout page back to the renderer. NX5 uses the
/// board's blocking PAN completion contract.
/// No public API can create this value or supply a callback.
pub(crate) struct PresentationContract {
    pub(crate) complete: Box<CompletePresentation>,
    pub(crate) validate_layout: fn(&FixedInfo, &VariableInfo) -> Result<Layout, Error>,
}

/// Verified board resources and OS timing information. Timer precision affects
/// event scheduling only; it cannot establish display-page ownership.
pub(crate) struct Admission {
    pub(crate) presentation: PresentationContract,
    pub(crate) timer_quantum: Duration,
}

pub(crate) fn require(backend: Backend) -> Result<Admission, Error> {
    #[cfg(feature = "nx5")]
    if backend == Backend::Nx5 {
        return Ok(Admission {
            presentation: crate::backends::nx5::open()?,
            timer_quantum: crate::backends::nx5::TIMER_QUANTUM,
        });
    }
    Err(Error::DisplayNotAdmitted {
        backend,
        reason: match backend {
            Backend::Nx5 => "this build does not include the NX5 display implementation",
            Backend::Fbdev => "no framebuffer driver has a reviewed tear-free completion contract",
        },
    })
}
