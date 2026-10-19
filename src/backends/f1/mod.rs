//! F1 integration boundary. See docs/f1-display.md for the inspected firmware.
//!
//! The reference SDK exposes PAN and a shared VBLANK wait, but neither binds a
//! successful result to retirement of the submitted page. Do not open graphics
//! resources or construct a PresentationContract until that gap is resolved.
use crate::{Backend, Error, backends::Admission};

#[cfg(feature = "f1-g2d")]
pub(crate) mod g2d;

pub(crate) fn open() -> Result<Admission, Error> {
    Err(Error::DisplayNotAdmitted {
        backend: Backend::F1,
        reason: if cfg!(all(target_arch = "riscv64", target_pointer_width = "64")) {
            "F1 display completion is not verified; PAN and one VSYNC do not retire the old page"
        } else {
            "F1 requires RISC-V 64-bit Linux and a verified display completion contract"
        },
    })
}
