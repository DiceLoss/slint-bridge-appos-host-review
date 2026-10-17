use slint_bridge::{Config, Error, init};
#[cfg(not(all(feature = "nx5", target_arch = "arm", target_pointer_width = "32")))]
use slint_bridge::{InputDevice, selected_backend};
use std::time::Duration;

#[test]
#[cfg(not(all(feature = "nx5", target_arch = "arm", target_pointer_width = "32")))]
fn unsupported_backend_is_rejected_before_device_access() {
    let error = init(Config {
        framebuffer: "/does/not/exist".into(),
        inputs: vec![InputDevice::touchscreen("/also/missing")],
        ..Config::default()
    })
    .unwrap_err();
    assert!(
        matches!(error, Error::DisplayNotAdmitted { backend, .. } if backend == selected_backend())
    );
}

#[test]
fn invalid_frame_timing_is_reported_without_opening_devices() {
    assert!(matches!(
        init(Config {
            frame_interval: Duration::ZERO,
            ..Config::default()
        }),
        Err(Error::InvalidConfig(_))
    ));
    assert!(matches!(
        init(Config {
            frame_interval: Duration::MAX,
            ..Config::default()
        }),
        Err(Error::InvalidConfig(_))
    ));
}

#[test]
#[cfg(not(feature = "nx5-mma-background"))]
fn ordinary_pixel_buffer_keeps_original_layout() {
    #[repr(C)]
    struct OriginalPixelBuffer {
        width: u32,
        height: u32,
        data: i_slint_core::SharedVector<slint::Rgba8Pixel>,
    }
    assert_eq!(
        std::mem::size_of::<slint::SharedPixelBuffer<slint::Rgba8Pixel>>(),
        std::mem::size_of::<OriginalPixelBuffer>()
    );
    assert_eq!(
        std::mem::align_of::<slint::SharedPixelBuffer<slint::Rgba8Pixel>>(),
        std::mem::align_of::<OriginalPixelBuffer>()
    );
}

#[test]
fn ordinary_background_preserves_image_storage_and_alpha() {
    use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
    use slint_bridge::BackgroundStorage;
    let config = Config::default();
    assert_eq!(config.background_storage, BackgroundStorage::Heap);
    let pixels = SharedPixelBuffer::clone_from_slice(&[30u8, 40, 50, 0, 80, 90, 100, 128], 2, 1);
    let pointer = pixels.as_slice().as_ptr();
    let prepared = config
        .prepare_background(Image::from_rgba8(pixels))
        .unwrap();
    let output = prepared.to_rgba8().unwrap();
    assert_eq!(output.as_slice().as_ptr(), pointer);
    assert_eq!(output.as_slice()[1], Rgba8Pixel::new(80, 90, 100, 128));
}

#[test]
fn mma_background_requires_supported_initialization() {
    use slint_bridge::BackgroundStorage;
    let config = Config {
        background_storage: BackgroundStorage::Mma,
        framebuffer: "/does/not/exist".into(),
        ..Config::default()
    };
    assert!(config.prepare_background(slint::Image::default()).is_err());
    #[cfg(not(all(
        feature = "nx5-mma-background",
        target_arch = "arm",
        target_pointer_width = "32"
    )))]
    assert!(matches!(init(config), Err(Error::InvalidConfig(reason)) if reason.contains("MMA")));
}
