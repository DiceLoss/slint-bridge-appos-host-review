use super::*;

const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><rect width="32" height="16" fill="#4080c0" fill-opacity="0.5"/></svg>"##;

slint::slint! {
    export { SvgPlatformTest } from "../../test-app/ui/tests/svg-platform.slint";
}

#[test]
fn installed_bridge_transparently_caches_standard_svg_images() {
    const NAME: &str = "tests::svg::installed_bridge_transparently_caches_standard_svg_images";
    if std::env::var("SLINT_BRIDGE_TEST_CHILD").as_deref() != Ok(NAME) {
        let run = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("SLINT_BRIDGE_TEST_CHILD", NAME)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        return;
    }
    let _guard = SLINT_TEST.lock().unwrap();
    let surface = surface(256, 192, 32);
    let platform = crate::runtime::BridgePlatform::with_inputs(
        Config::default(),
        surface.fb,
        Vec::new(),
        Vec::new(),
    );
    slint::platform::set_platform(Box::new(platform)).unwrap();
    // This is the ordinary application call, with no resource registration.
    let image = slint::Image::load_from_data(SVG, Some("svg")).unwrap();
    assert_eq!((image.size().width, image.size().height), (32, 16));
    let first = image.to_rgba8_premultiplied().unwrap();
    let second = image.to_rgba8_premultiplied().unwrap();
    assert_eq!(first.as_slice(), second.as_slice());
    assert_eq!(
        first.as_bytes().as_ptr(),
        second.as_bytes().as_ptr(),
        "the installed bridge must reuse the SVG raster"
    );
    let scene = SvgPlatformTest::new().unwrap();
    let embedded = scene.get_embedded_image();
    assert!(
        embedded.path().is_none(),
        "fixture must exercise embedded SVG data"
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-app/ui/assets/scaling-reference.svg");
    let file = slint::Image::load_from_path(&path).unwrap();
    for image in [
        embedded,
        file,
        slint::Image::load_from_data(SVG, None).unwrap(),
    ] {
        let first = image.to_rgba8_premultiplied().unwrap();
        let second = image.to_rgba8_premultiplied().unwrap();
        assert_eq!(first.as_bytes().as_ptr(), second.as_bytes().as_ptr());
    }
    const SVGZ: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0xb3, 0x29, 0x2e, 0x4b, 0x57,
        0x28, 0xcf, 0x4c, 0x29, 0xc9, 0xb0, 0x55, 0x32, 0x36, 0x32, 0x50, 0x52, 0xc8, 0x48, 0xcd,
        0x4c, 0xcf, 0x28, 0xb1, 0x55, 0x32, 0x32, 0x00, 0x72, 0x2a, 0x72, 0x73, 0xf2, 0x8a, 0x6d,
        0x95, 0x32, 0x4a, 0x4a, 0x0a, 0xac, 0xf4, 0xf5, 0xcb, 0xcb, 0xcb, 0xf5, 0xca, 0x8d, 0xf5,
        0xf2, 0x8b, 0xd2, 0xf5, 0x81, 0xb2, 0x06, 0xfa, 0x40, 0xad, 0x4a, 0x76, 0x36, 0x20, 0xca,
        0x0e, 0x00, 0x37, 0x91, 0x7a, 0xd6, 0x47, 0x00, 0x00, 0x00,
    ];

    let compressed = slint::Image::load_from_data(SVGZ, Some("svgz")).unwrap();
    assert_eq!(
        (compressed.size().width, compressed.size().height),
        (320, 200)
    );
    let first = compressed.to_rgba8_premultiplied().unwrap();
    let second = compressed.to_rgba8_premultiplied().unwrap();
    assert_eq!(first.as_bytes().as_ptr(), second.as_bytes().as_ptr());
    assert!(slint::Image::load_from_data(b"<svg broken", Some("svg")).is_err());
    assert!(slint::Image::load_from_svg_data(b"<svg broken").is_err());
}
