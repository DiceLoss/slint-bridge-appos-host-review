//! A single fullscreen Slint platform for embedded Linux.
//!
//! Select exactly one board feature (`fbdev` or `nx5`). `nx5-tde` implies `nx5`.
//! NX5 supports the current firmware's blocking double-buffered presentation.
//! Other framebuffer drivers need their own presentation implementation.

#[cfg(not(target_os = "linux"))]
compile_error!("slint-bridge currently supports Linux only");
#[cfg(not(any(feature = "fbdev", feature = "nx5")))]
compile_error!(
    "select one board feature: fbdev or nx5 (nx5-tde implies nx5). \
     Enable it in the slint-bridge dependency's features, or select an application feature that forwards to it."
);
#[cfg(all(feature = "fbdev", feature = "nx5"))]
compile_error!("fbdev and nx5 are mutually exclusive board features");

mod backends;
mod event_wait;
mod framebuffer;
mod geometry;
mod input;
mod presentation;
mod runtime;
#[cfg(feature = "fbdev")]
mod software_background;
#[cfg(feature = "fbdev")]
mod software_texture;
mod wake;

pub use geometry::Rotation;
pub use input::{InputDevice, KeyBinding};

use std::{fmt, io, path::PathBuf, time::Duration};

/// Compile-time board selection. This is not a statement of hardware admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Fbdev,
    Nx5,
}

/// The board selected by Cargo features.
pub const fn selected_backend() -> Backend {
    if cfg!(feature = "nx5") {
        Backend::Nx5
    } else {
        Backend::Fbdev
    }
}

/// Configuration shared by all board implementations.
#[derive(Debug, Clone)]
pub struct Config {
    /// Framebuffer device path. Defaults to `/dev/fb0` and may be overridden.
    pub framebuffer: PathBuf,
    /// Empty by default; at most one touchscreen and one mapped-key device.
    pub inputs: Vec<InputDevice>,
    /// Clockwise output rotation, fixed for the lifetime of this platform.
    pub rotation: Rotation,
    /// Minimum time between frame starts; idle windows do not render periodically.
    /// One retained frame may render ahead of display completion.
    /// Defaults to 16_667 microseconds; must be nonzero and fit the monotonic clock.
    pub frame_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            framebuffer: "/dev/fb0".into(),
            inputs: Vec::new(),
            rotation: Rotation::None,
            frame_interval: Duration::from_micros(16_667),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    DisplayNotAdmitted {
        backend: Backend,
        reason: &'static str,
    },
    InvalidConfig(&'static str),
    UnsupportedFramebuffer(&'static str),
    UnsupportedInput(&'static str),
    PresentationFailed(String),
    AccelerationFailed(String),
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Platform(slint::platform::SetPlatformError),
}

impl Error {
    pub(crate) fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DisplayNotAdmitted { backend, reason } => {
                write!(f, "{backend:?} display is not admitted: {reason}")
            }
            Self::InvalidConfig(reason) => write!(f, "invalid configuration: {reason}"),
            Self::UnsupportedFramebuffer(reason) => write!(f, "unsupported framebuffer: {reason}"),
            Self::UnsupportedInput(reason) => write!(f, "unsupported input: {reason}"),
            Self::PresentationFailed(reason) => write!(
                f,
                "presentation failed; page ownership is unknown: {reason}"
            ),
            Self::AccelerationFailed(reason) => write!(
                f,
                "TDE completion is unknown; frame was not presented: {reason}"
            ),
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
            Self::Platform(source) => fmt::Display::fmt(source, f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Platform(source) => Some(source),
            _ => None,
        }
    }
}

/// Install the platform on the UI thread, before constructing a Slint component.
/// Then create the component and call its usual `run()` method.
///
/// NX5 requires ARM32, exclusive display ownership and a compatible
/// SDK/driver ABI. It does not check kernel versions, driver names or file hashes.
/// Target/ownership mismatches and generic fbdev return [`Error::DisplayNotAdmitted`];
/// SDK loading and device I/O failures return [`Error::Io`]. Frame timing and input
/// counts are validated before board initialization. Errors during the later
/// component event loop are returned through Slint's `PlatformError`.
pub fn init(config: Config) -> Result<(), Error> {
    if config.frame_interval.is_zero() {
        return Err(Error::InvalidConfig("frame_interval must be nonzero"));
    }
    if std::time::Instant::now()
        .checked_add(config.frame_interval)
        .is_none()
    {
        return Err(Error::InvalidConfig(
            "frame_interval exceeds the monotonic clock range",
        ));
    }
    let touches = config
        .inputs
        .iter()
        .filter(|input| matches!(input, InputDevice::Touchscreen { .. }))
        .count();
    let keys = config.inputs.len() - touches;
    if touches > 1 || keys > 1 {
        return Err(Error::InvalidConfig(
            "at most one touchscreen and one key device are supported",
        ));
    }
    let admitted = backends::require(selected_backend())?;
    let framebuffer = framebuffer::Framebuffer::open(&config.framebuffer, admitted.presentation)?;
    let platform = runtime::BridgePlatform::new(config, framebuffer, admitted.timer_quantum)?;
    slint::platform::set_platform(Box::new(platform)).map_err(Error::Platform)
}

#[cfg(test)]
mod tests;
