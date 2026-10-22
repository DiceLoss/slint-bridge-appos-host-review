//! A single fullscreen Slint platform for embedded Linux.
//!
//! Select exactly one board feature (`fbdev`, `nx1`, `nx5` or `f1`).
//! `nx5-tde` implies `nx5`. NX1 uses the inspected firmware's reference PAN
//! convention, whose internal wait errors are not exposed to userspace.
//! NX5 supports the current firmware's blocking double-buffered presentation.
//! F1 uses its reference-example PAN followed by WAITFORVSYNC convention on the
//! inspected firmware. It is not a submission-ID completion fence.
//! Other framebuffer drivers need their own presentation implementation.

#[cfg(not(target_os = "linux"))]
compile_error!("slint-bridge currently supports Linux only");
#[cfg(not(any(feature = "fbdev", feature = "nx1", feature = "nx5", feature = "f1")))]
compile_error!(
    "select one board feature: fbdev, nx1, nx5 or f1 (nx5-tde implies nx5). \
     Enable it in the slint-bridge dependency's features, or select an application feature that forwards to it."
);
#[cfg(any(
    all(feature = "fbdev", feature = "nx5"),
    all(feature = "f1", feature = "nx5"),
    all(feature = "f1", feature = "fbdev"),
    all(
        feature = "nx1",
        any(feature = "fbdev", feature = "nx5", feature = "f1")
    )
))]
compile_error!("fbdev, nx1, nx5 and f1 are mutually exclusive board features");

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
mod texture_renderer;
mod wake;

pub use geometry::Rotation;
pub use input::{InputDevice, KeyBinding};

use std::{fmt, io, path::PathBuf, time::Duration};

/// Compile-time board selection. This is not a statement of hardware admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Fbdev,
    /// NX1 / sun8i, using the explicitly selected reference PAN convention.
    Nx1,
    Nx5,
    /// F1 / ArtInChip D211, using the reference PAN + WAITFORVSYNC sequence.
    F1,
}

/// The board selected by Cargo features.
pub const fn selected_backend() -> Backend {
    if cfg!(feature = "nx5") {
        Backend::Nx5
    } else if cfg!(feature = "f1") {
        Backend::F1
    } else if cfg!(feature = "nx1") {
        Backend::Nx1
    } else {
        Backend::Fbdev
    }
}

/// Try F1's bounded hardware PNG decoder, without installing a Slint platform.
///
/// `Ok(None)` means software decoding is required: the board, PNG encoding,
/// image size, available memory or decoder ownership is unsuitable.
/// Other boards always return `Ok(None)`. The returned image owns ordinary
/// CPU pixels; no decoder buffer remains allocated after success.
///
/// Uncertain completion retains the decoder and DMA resources and returns an
/// error. Completion or resource-release failures reject later hardware decodes.
/// Do not retry or fall back on an error. Keep the process alive until device
/// recovery establishes that DMA has stopped.
pub fn try_decode_png(data: &[u8]) -> Result<Option<slint::Image>, Error> {
    #[cfg(all(feature = "f1", target_arch = "riscv64"))]
    {
        backends::f1::png::decode(data)
    }
    #[cfg(not(all(feature = "f1", target_arch = "riscv64")))]
    {
        let _ = data;
        Ok(None)
    }
}

/// Storage used by [`Config::prepare_background`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundStorage {
    /// Keep the supplied Slint image unchanged. Supported on every backend.
    #[default]
    Heap,
    /// Use one existing NX5 MMA source slot for an opaque canvas-sized image.
    /// Requires ARM32 and the `nx5-mma-background` feature.
    Mma,
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
    /// Background storage selection. Defaults to [`BackgroundStorage::Heap`].
    /// Selecting MMA requires an initialized hardware TDE canvas; it never
    /// silently substitutes heap storage if initialization cannot provide it.
    pub background_storage: BackgroundStorage,
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
            background_storage: BackgroundStorage::Heap,
            frame_interval: Duration::from_micros(16_667),
        }
    }
}

impl Config {
    fn validate_background_storage(&self) -> Result<(), Error> {
        if self.background_storage == BackgroundStorage::Mma
            && !cfg!(all(
                feature = "nx5-mma-background",
                target_arch = "arm",
                target_pointer_width = "32"
            ))
        {
            return Err(Error::InvalidConfig(
                "MMA background requires nx5-mma-background on ARM32",
            ));
        }
        Ok(())
    }

    /// Prepare an application background according to `background_storage`.
    ///
    /// Heap mode returns the supplied image unchanged, without copying pixels.
    /// MMA mode requires successful [`init`] with the same storage selection,
    /// on this UI thread, and must be called once before the first frame.
    /// It accepts an opaque image matching the physical canvas dimensions.
    /// The result retains its MMA allocation and SDK session for software sampling.
    ///
    /// This consumes the source handle. Other clones or Slint's resource cache
    /// can still retain the original heap pixels; use an uncached decoded image
    /// when eliminating that copy is required.
    pub fn prepare_background(&self, image: slint::Image) -> Result<slint::Image, Error> {
        match self.background_storage {
            BackgroundStorage::Heap => Ok(image),
            BackgroundStorage::Mma => {
                self.validate_background_storage()?;
                #[cfg(feature = "nx5-mma-background")]
                {
                    backends::nx5::tde::background(image)
                }
                #[cfg(not(feature = "nx5-mma-background"))]
                {
                    unreachable!("MMA support was validated above")
                }
            }
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
            Self::AccelerationFailed(reason) => write!(f, "hardware acceleration failed: {reason}"),
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
/// SDK loading and device I/O failures return [`Error::Io`].
/// F1 requires the inspected RISC-V firmware, double-page layout and exclusive
/// ownership, and uses the reference PAN + WAITFORVSYNC sequence.
/// Frame timing and input counts are validated before board initialization. Errors during the later
/// component event loop are returned through Slint's `PlatformError`.
pub fn init(config: Config) -> Result<(), Error> {
    config.validate_background_storage()?;
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
        .filter(|input| {
            matches!(
                input,
                InputDevice::Touchscreen { .. } | InputDevice::AutoTouchscreen
            )
        })
        .count();
    let keys = config.inputs.len() - touches;
    if touches > 1 || keys > 1 {
        return Err(Error::InvalidConfig(
            "at most one touchscreen and one key device are supported",
        ));
    }
    let admitted = backends::require(selected_backend())?;
    let framebuffer = framebuffer::Framebuffer::open(&config.framebuffer, admitted.presentation)?;
    #[cfg(feature = "nx5-mma-background")]
    if config.background_storage == BackgroundStorage::Mma {
        framebuffer.enable_mma_background()?;
    }
    let platform = runtime::BridgePlatform::new(config, framebuffer, admitted.timer_quantum)?;
    slint::platform::set_platform(Box::new(platform)).map_err(Error::Platform)
}

#[cfg(test)]
mod tests;
