//! AppOS integration through resources admitted and transferred by its launcher.

use crate::{
    BackgroundStorage, Config, Error, Rotation,
    backends::PresentationContract,
    framebuffer::{Framebuffer, Layout},
    input::Evdev,
    runtime::BridgePlatform,
};
use slint::platform::software_renderer::PremultipliedRgbaColor;
use slint::platform::{WindowEvent, set_platform};
use std::{
    cell::Cell,
    fmt,
    os::fd::OwnedFd,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::KeyBinding;

thread_local! {
    static PANEL_SIZE: Cell<Option<slint::PhysicalSize>> = const { Cell::new(None) };
    static VIDEO_SURFACE: Cell<Option<HostVideoSurfaceRect>> = const { Cell::new(None) };
    static FRAME_POLICY: Cell<HostFramePolicy> = const { Cell::new(HostFramePolicy::DEFAULT) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostInputEvent {
    TouchPressed,
    TouchReleased,
    PointerMotion,
    Key,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostInputDisposition {
    Dispatch,
    Consume,
}

/// Output policy applied while copying the retained Slint frame to scanout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostFramePolicy {
    pub blanked: bool,
    pub luminance: u8,
}

impl HostFramePolicy {
    pub const DEFAULT: Self = Self {
        blanked: false,
        luminance: u8::MAX,
    };
}

impl Default for HostFramePolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Narrow callbacks from the device-owning GUI backend to AppOS policy.
/// Implementations must be bounded and must not re-enter Slint.
pub trait HostHooks: Send + Sync {
    fn input(&self, _event: HostInputEvent, _at: Instant) -> HostInputDisposition {
        HostInputDisposition::Dispatch
    }

    fn evaluate_idle(&self, _now: Instant) -> bool {
        false
    }

    fn until_idle_deadline(&self, _now: Instant) -> Option<Duration> {
        None
    }

    fn frame_policy(&self) -> HostFramePolicy {
        HostFramePolicy::DEFAULT
    }

    fn frame_presented(&self, _started_at: Instant, _presented_at: Instant) {}
}

pub(crate) fn set_frame_policy(policy: HostFramePolicy) -> bool {
    FRAME_POLICY.with(|active| active.replace(policy) != policy)
}

pub(crate) fn output_policy_is_identity() -> bool {
    FRAME_POLICY.with(|policy| policy.get() == HostFramePolicy::DEFAULT)
}

pub(crate) fn transform_output_pixel(mut pixel: PremultipliedRgbaColor) -> PremultipliedRgbaColor {
    FRAME_POLICY.with(|policy| {
        let policy = policy.get();
        if policy.blanked {
            return PremultipliedRgbaColor {
                red: 0,
                green: 0,
                blue: 0,
                alpha: u8::MAX,
            };
        }
        let scale =
            |channel: u8| ((u16::from(channel) * u16::from(policy.luminance) + 127) / 255) as u8;
        pixel.red = scale(pixel.red);
        pixel.green = scale(pixel.green);
        pixel.blue = scale(pixel.blue);
        pixel
    })
}

/// Physical AppOS video viewport whose pixels remain transparent for the
/// lower hardware video layer. AppOS retains session and generation ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostVideoSurfaceRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub(crate) fn apply_video_surface(
    pixels: &mut [PremultipliedRgbaColor],
    stride: usize,
) -> Option<(slint::PhysicalPosition, slint::PhysicalSize)> {
    let rect = VIDEO_SURFACE.with(Cell::get)?;
    for row in rect.y..rect.y + rect.height {
        let start = row as usize * stride + rect.x as usize;
        let end = start + rect.width as usize;
        pixels[start..end].fill(PremultipliedRgbaColor::default());
    }
    Some((
        slint::PhysicalPosition::new(rect.x as i32, rect.y as i32),
        slint::PhysicalSize::new(rect.width, rect.height),
    ))
}

/// Apply the framework-owned video viewport and schedule one complete repaint.
pub fn set_video_surface_rect(rect: Option<HostVideoSurfaceRect>) -> Result<(), Error> {
    let panel = PANEL_SIZE
        .with(Cell::get)
        .ok_or(Error::InvalidConfig("AppOS host is not initialized"))?;
    if let Some(rect) = rect {
        let x1 = rect.x.checked_add(rect.width);
        let y1 = rect.y.checked_add(rect.height);
        if rect.width == 0
            || rect.height == 0
            || x1.is_none_or(|x| x > panel.width)
            || y1.is_none_or(|y| y > panel.height)
        {
            return Err(Error::InvalidConfig(
                "video surface must be nonempty and inside the AppOS panel",
            ));
        }
    }
    let changed = VIDEO_SURFACE.with(|active| active.replace(rect) != rect);
    if changed {
        crate::runtime::request_full_redraw();
    }
    Ok(())
}

/// An input descriptor admitted and transferred by the AppOS launcher.
///
/// The bridge consumes descriptor ownership. It never scans `/dev/input` or
/// reopens a device path in this mode.
#[derive(Debug)]
pub enum HostInputDevice {
    Touchscreen {
        descriptor: OwnedFd,
    },
    Keys {
        descriptor: OwnedFd,
        bindings: Vec<KeyBinding>,
    },
}

impl HostInputDevice {
    pub fn touchscreen(descriptor: OwnedFd) -> Self {
        Self::Touchscreen { descriptor }
    }

    pub fn keys(descriptor: OwnedFd, bindings: Vec<KeyBinding>) -> Self {
        Self::Keys {
            descriptor,
            bindings,
        }
    }
}

/// AppOS-owned display and input resources for the single-page staged-copy
/// backend used by PLC NX5 and NX1 products.
///
/// The expected dimensions come from the AppOS board requirement manifest and
/// bind the transferred descriptor to the selected product profile.
pub struct HostConfig {
    pub framebuffer: OwnedFd,
    pub inputs: Vec<HostInputDevice>,
    pub expected_width: u32,
    pub expected_height: u32,
    pub rotation: Rotation,
    pub frame_interval: Duration,
    pub font_paths: Vec<PathBuf>,
    pub hooks: Option<Arc<dyn HostHooks>>,
}

impl fmt::Debug for HostConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostConfig")
            .field("framebuffer", &self.framebuffer)
            .field("inputs", &self.inputs)
            .field("expected_width", &self.expected_width)
            .field("expected_height", &self.expected_height)
            .field("rotation", &self.rotation)
            .field("frame_interval", &self.frame_interval)
            .field("font_paths", &self.font_paths)
            .field("hooks", &self.hooks.as_ref().map(|_| "installed"))
            .finish()
    }
}

impl HostConfig {
    pub fn single_page(framebuffer: OwnedFd, expected_width: u32, expected_height: u32) -> Self {
        Self {
            framebuffer,
            inputs: Vec::new(),
            expected_width,
            expected_height,
            rotation: Rotation::None,
            frame_interval: Duration::from_micros(16_667),
            font_paths: Vec::new(),
            hooks: None,
        }
    }
}

/// Install the AppOS-owned Slint platform.
///
/// This entry point accepts only owned descriptors. Device discovery and path
/// reopening remain available to standalone bridge products through [`crate::init`]
/// but are not part of the AppOS trust boundary.
pub fn init_host(config: HostConfig) -> Result<(), Error> {
    if config.expected_width == 0 || config.expected_height == 0 {
        return Err(Error::InvalidConfig(
            "AppOS host dimensions must be nonzero",
        ));
    }
    if config.frame_interval.is_zero() {
        return Err(Error::InvalidConfig("frame_interval must be nonzero"));
    }
    if Instant::now().checked_add(config.frame_interval).is_none() {
        return Err(Error::InvalidConfig(
            "frame_interval exceeds the monotonic clock range",
        ));
    }

    let touches = config
        .inputs
        .iter()
        .filter(|input| matches!(input, HostInputDevice::Touchscreen { .. }))
        .count();
    let keys = config.inputs.len() - touches;
    if touches > 1 || keys > 1 {
        return Err(Error::InvalidConfig(
            "at most one touchscreen and one key device are supported",
        ));
    }

    let contract = PresentationContract {
        validate_layout: Box::new(|_, fixed, variable| Layout::single_page(fixed, variable)),
        complete: Box::new(|_, offset, _| {
            if offset == 0 {
                Ok(())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "single-page AppOS presentation requires offset zero",
                ))
            }
        }),
    };
    let framebuffer = Framebuffer::from_owned_fd(config.framebuffer, contract)?;
    let panel = framebuffer.layout.size;
    if panel.width != config.expected_width || panel.height != config.expected_height {
        return Err(Error::DisplayNotAdmitted {
            backend: crate::Backend::AppOsHost,
            reason: "framebuffer geometry differs from the AppOS board requirement",
        });
    }
    let mut inputs = Vec::with_capacity(config.inputs.len());
    let mut initial_events: Vec<WindowEvent> = Vec::new();
    for input in config.inputs {
        let mut input = match input {
            HostInputDevice::Touchscreen { descriptor } => {
                Evdev::from_owned_fd(descriptor, true, &[], panel, config.rotation)?
            }
            HostInputDevice::Keys {
                descriptor,
                bindings,
            } => Evdev::from_owned_fd(descriptor, false, &bindings, panel, config.rotation)?,
        };
        input.resync(&mut initial_events)?;
        inputs.push(input);
    }

    let runtime_config = Config {
        framebuffer: PathBuf::new(),
        inputs: Vec::new(),
        rotation: config.rotation,
        background_storage: BackgroundStorage::Heap,
        frame_interval: config.frame_interval,
    };
    let platform = BridgePlatform::with_host_inputs(
        runtime_config,
        framebuffer,
        inputs,
        initial_events,
        config.font_paths,
        config.hooks,
    );
    PANEL_SIZE.with(|size| size.set(Some(panel)));
    VIDEO_SURFACE.with(|surface| surface.set(None));
    FRAME_POLICY.with(|policy| policy.set(HostFramePolicy::DEFAULT));
    let installed = set_platform(Box::new(platform)).map_err(Error::Platform);
    if installed.is_err() {
        PANEL_SIZE.with(|size| size.set(None));
        VIDEO_SURFACE.with(|surface| surface.set(None));
    }
    installed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, os::fd::AsRawFd};

    #[test]
    fn single_page_layout_requires_only_one_complete_scanout_page() {
        let (mut fixed, mut variable) = crate::tests::info(1024, 600, 32);
        variable.yres_virtual = variable.yres;
        variable.yoffset = 0;
        fixed.smem_len = fixed.line_length * variable.yres;

        let layout = Layout::single_page(&fixed, &variable).unwrap();

        assert_eq!(layout.page_count, 1);
        assert_eq!(layout.initial_page, 0);
        assert_eq!(layout.page_bytes, fixed.smem_len as usize);
    }

    #[test]
    fn single_page_layout_rejects_a_nonzero_scanout_offset() {
        let (mut fixed, mut variable) = crate::tests::info(1024, 600, 32);
        variable.yres_virtual = variable.yres;
        variable.yoffset = variable.yres;
        fixed.smem_len = fixed.line_length * variable.yres;

        assert!(matches!(
            Layout::single_page(&fixed, &variable),
            Err(Error::UnsupportedFramebuffer(_))
        ));
    }

    #[test]
    fn rejected_framebuffer_consumes_the_transferred_descriptor() {
        let descriptor: OwnedFd = File::open("/dev/null").unwrap().into();
        let raw = descriptor.as_raw_fd();
        let error = init_host(HostConfig::single_page(descriptor, 1024, 600)).unwrap_err();

        assert!(matches!(
            error,
            Error::Io {
                operation: "read framebuffer layout",
                ..
            }
        ));
        assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
    }

    #[test]
    fn host_configuration_rejects_ambiguous_input_ownership() {
        let framebuffer: OwnedFd = File::open("/dev/null").unwrap().into();
        let first: OwnedFd = File::open("/dev/null").unwrap().into();
        let second: OwnedFd = File::open("/dev/null").unwrap().into();
        let mut config = HostConfig::single_page(framebuffer, 1024, 600);
        config.inputs = vec![
            HostInputDevice::touchscreen(first),
            HostInputDevice::touchscreen(second),
        ];

        assert!(matches!(
            init_host(config),
            Err(Error::InvalidConfig(
                "at most one touchscreen and one key device are supported"
            ))
        ));
    }

    #[test]
    fn video_surface_rejects_invalid_geometry_and_clears_only_its_rectangle() {
        PANEL_SIZE.with(|size| size.set(Some(slint::PhysicalSize::new(4, 3))));
        VIDEO_SURFACE.with(|surface| surface.set(None));
        assert!(
            set_video_surface_rect(Some(HostVideoSurfaceRect {
                x: 3,
                y: 2,
                width: 2,
                height: 1,
            }))
            .is_err()
        );

        let rect = HostVideoSurfaceRect {
            x: 1,
            y: 1,
            width: 2,
            height: 1,
        };
        set_video_surface_rect(Some(rect)).unwrap();
        let opaque = PremultipliedRgbaColor {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 255,
        };
        let mut pixels = vec![opaque; 12];
        assert_eq!(
            apply_video_surface(&mut pixels, 4),
            Some((
                slint::PhysicalPosition::new(1, 1),
                slint::PhysicalSize::new(2, 1),
            ))
        );
        assert_eq!(
            (
                pixels[4].red,
                pixels[4].green,
                pixels[4].blue,
                pixels[4].alpha
            ),
            (1, 2, 3, 255)
        );
        assert_eq!(
            (
                pixels[5].red,
                pixels[5].green,
                pixels[5].blue,
                pixels[5].alpha
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(
            (
                pixels[6].red,
                pixels[6].green,
                pixels[6].blue,
                pixels[6].alpha
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(
            (
                pixels[7].red,
                pixels[7].green,
                pixels[7].blue,
                pixels[7].alpha
            ),
            (1, 2, 3, 255)
        );
        VIDEO_SURFACE.with(|surface| surface.set(None));
        PANEL_SIZE.with(|size| size.set(None));
    }

    #[test]
    fn output_policy_dims_without_mutating_alpha_and_blank_is_opaque() {
        set_frame_policy(HostFramePolicy {
            blanked: false,
            luminance: 128,
        });
        let dimmed = transform_output_pixel(PremultipliedRgbaColor {
            red: 200,
            green: 100,
            blue: 50,
            alpha: 220,
        });
        assert_eq!(
            [dimmed.red, dimmed.green, dimmed.blue, dimmed.alpha],
            [100, 50, 25, 220]
        );
        set_frame_policy(HostFramePolicy {
            blanked: true,
            luminance: 0,
        });
        let blank = transform_output_pixel(PremultipliedRgbaColor {
            red: 200,
            green: 100,
            blue: 50,
            alpha: 0,
        });
        assert_eq!(
            [blank.red, blank.green, blank.blue, blank.alpha],
            [0, 0, 0, 255]
        );
        set_frame_policy(HostFramePolicy::DEFAULT);
    }
}
