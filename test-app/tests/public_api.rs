use slint_bridge::{Config, Error, InputDevice, init, selected_backend};
use std::time::Duration;

#[test]
fn admission_precedes_all_device_access() {
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
