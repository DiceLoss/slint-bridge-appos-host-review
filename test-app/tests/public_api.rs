use slint_bridge::{Config, Error, init};
#[cfg(not(any(
    all(feature = "nx1", target_arch = "arm", target_pointer_width = "32"),
    all(feature = "nx5", target_arch = "arm", target_pointer_width = "32"),
    all(feature = "f1", target_arch = "riscv64", target_pointer_width = "64")
)))]
use slint_bridge::{InputDevice, selected_backend};
use std::time::Duration;

#[test]
fn unsupported_png_requests_allow_software_handling() {
    for data in [&[][..], &b"not a PNG"[..]] {
        assert!(slint_bridge::try_decode_png(data).unwrap().is_none());
    }
}

#[test]
#[cfg(not(all(feature = "f1", target_arch = "riscv64")))]
fn optional_hardware_decoder_preserves_software_loading() {
    let data = include_bytes!("../ui/assets/scaling-reference.png");
    let decoded = slint_bridge::try_decode_png(data).unwrap();
    assert!(decoded.is_none());
    let image = decoded.unwrap_or_else(|| slint::Image::load_from_data(data, Some("png")).unwrap());
    assert_eq!((image.size().width, image.size().height), (256, 160));
}

#[test]
#[cfg(feature = "f1")]
fn f1_selection_requires_board_admission() {
    assert_eq!(slint_bridge::selected_backend(), slint_bridge::Backend::F1);
    #[cfg(not(all(target_arch = "riscv64", target_pointer_width = "64")))]
    assert!(matches!(
        init(Config {
            framebuffer: "/does/not/exist".into(),
            ..Config::default()
        }),
        Err(Error::DisplayNotAdmitted {
            backend: slint_bridge::Backend::F1,
            ..
        })
    ));
}

#[test]
#[cfg(not(any(
    all(feature = "nx1", target_arch = "arm", target_pointer_width = "32"),
    all(feature = "nx5", target_arch = "arm", target_pointer_width = "32"),
    all(feature = "f1", target_arch = "riscv64", target_pointer_width = "64")
)))]
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
#[cfg(feature = "nx1")]
fn nx1_selection_requires_board_admission() {
    assert_eq!(slint_bridge::selected_backend(), slint_bridge::Backend::Nx1);
    #[cfg(not(all(target_arch = "arm", target_pointer_width = "32")))]
    assert!(matches!(
        init(Config {
            framebuffer: "/does/not/exist".into(),
            ..Config::default()
        }),
        Err(Error::DisplayNotAdmitted {
            backend: slint_bridge::Backend::Nx1,
            ..
        })
    ));
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
