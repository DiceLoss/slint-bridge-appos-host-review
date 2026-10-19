use super::*;

const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><rect x="1" y="1" width="30" height="14" fill="#4080c0" fill-opacity="0.5"/></svg>"##;

#[test]
fn rasters_keep_copy_on_write_and_one_size_per_source() {
    let images = SvgImages::default();
    let image = images.load(SVG, &SvgOptions::default()).unwrap();
    for size in [
        None,
        Some(PhysicalSize::new(41, 29)),
        Some(PhysicalSize::new(16, 8)),
        None,
    ] {
        let mut first = image.render(size).unwrap();
        let again = image.render(size).unwrap();
        assert_eq!(first.as_bytes().as_ptr(), again.as_bytes().as_ptr());
        first.make_mut_bytes().fill(255);
        assert_eq!(image.render(size).unwrap().as_bytes(), again.as_bytes());
        assert_ne!(first.as_bytes(), again.as_bytes());
        assert_eq!(images.cache.borrow().len(), 1);
    }
    assert!(matches!(
        image.render(Some(PhysicalSize::new(0, 16))),
        Err(SvgError::InvalidSize)
    ));
    assert!(images.load(b"not svg", &SvgOptions::default()).is_err());
}

#[test]
fn raster_cache_bounds_memory_and_does_not_keep_sources_alive() {
    let images = SvgImages::default();
    let sources: Vec<_> = (0..40)
        .map(|_| images.load(SVG, &SvgOptions::default()).unwrap())
        .collect();
    let first = sources[0].render(None).unwrap();
    for image in &sources[1..] {
        image.render(None).unwrap();
    }
    assert_eq!(images.cache.borrow().len(), MAX_ENTRIES);
    assert_ne!(
        first.as_bytes().as_ptr(),
        sources[0].render(None).unwrap().as_bytes().as_ptr()
    );
    for image in &sources[..3] {
        image.render(Some(PhysicalSize::new(700, 350))).unwrap();
    }
    assert!(images.cache.borrow().len() <= 2);
    assert!(
        images
            .cache
            .borrow()
            .iter()
            .map(|e| e.pixels.as_bytes().len())
            .sum::<usize>()
            <= MAX_BYTES
    );

    let oversized = Some(PhysicalSize::new(2048, 1024));
    let first = sources[0].render(oversized).unwrap();
    let again = sources[0].render(oversized).unwrap();
    assert_ne!(first.as_bytes().as_ptr(), again.as_bytes().as_ptr());
    assert!(
        images
            .cache
            .borrow()
            .iter()
            .all(|entry| entry.size != oversized)
    );

    drop(sources);
    assert!(
        images
            .cache
            .borrow()
            .iter()
            .all(|entry| entry.source.upgrade().is_none())
    );
    let fresh = images.load(SVG, &SvgOptions::default()).unwrap();
    fresh.render(None).unwrap();
    assert_eq!(images.cache.borrow().len(), 1);
}

slint::slint! {
    export { SvgPlatformTest } from "../../test-app/ui/tests/svg-platform.slint";
}

#[test]
fn platform_svg_matches_native_pixels_with_fonts_scaling_clipping_and_rotation() {
    use slint::platform::software_renderer::{
        MinimalSoftwareWindow, PremultipliedRgbaColor, RenderingRotation, RepaintBufferType,
    };
    use slint::{ComponentHandle, platform::WindowAdapter};

    struct Platform {
        window: Rc<MinimalSoftwareWindow>,
        images: Option<SvgImages>,
    }
    impl slint::platform::Platform for Platform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            Ok(self.window.clone())
        }
        fn load_svg_image(
            &self,
            data: &[u8],
            options: &SvgOptions<'_>,
        ) -> Option<Result<Box<dyn SvgImage>, SvgError>> {
            self.images
                .as_ref()
                .map(|images| images.load(data, options))
        }
    }
    fn frames(provider: bool, rotation: RenderingRotation) -> Vec<Vec<[u8; 4]>> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        slint::platform::set_platform(Box::new(Platform {
            window: window.clone(),
            images: provider.then(SvgImages::default),
        }))
        .unwrap();
        let scene = SvgPlatformTest::new().unwrap();
        window
            .renderer()
            .register_font_from_memory(include_bytes!("../../third-party/fonts/DejaVuSans.ttf"))
            .unwrap();
        let text = slint::Image::load_from_svg_data(br##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="80"><text x="5" y="40" font-family="DejaVu Sans" font-size="24" fill="white">SVG text</text></svg>"##).unwrap();
        assert!(
            text.to_rgba8_premultiplied()
                .unwrap()
                .as_slice()
                .iter()
                .any(|p| p.a != 0)
        );
        let picture = scene.get_embedded_image();
        let replaced = slint::Image::load_from_data(SVG, Some("svg")).unwrap();
        scene.show().unwrap();
        window.set_size(PhysicalSize::new(256, 192));
        let stride = if matches!(
            rotation,
            RenderingRotation::Rotate90 | RenderingRotation::Rotate270
        ) {
            192
        } else {
            256
        };
        let mut pixels = vec![PremultipliedRgbaColor::default(); 256 * 192];
        let mut frames = Vec::new();
        for (i, zoom) in [0.5, 1.0, 1.25, 2.0, 1.25, 0.5, 1.0, 1.0]
            .into_iter()
            .enumerate()
        {
            scene.set_picture(match i {
                4 | 5 => replaced.clone(),
                7 => text.clone(),
                _ => picture.clone(),
            });
            scene.set_zoom(zoom);
            scene.set_visible_image(i != 6);
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.set_rendering_rotation(rotation);
                renderer.render(&mut pixels, stride);
            });
            frames.push(
                pixels
                    .iter()
                    .map(|p| [p.red, p.green, p.blue, p.alpha])
                    .collect(),
            );
        }
        scene.hide().unwrap();
        frames
    }
    for rotation in [
        RenderingRotation::NoRotation,
        RenderingRotation::Rotate90,
        RenderingRotation::Rotate180,
        RenderingRotation::Rotate270,
    ] {
        let expected = std::thread::spawn(move || frames(false, rotation))
            .join()
            .unwrap();
        let actual = std::thread::spawn(move || frames(true, rotation))
            .join()
            .unwrap();
        for (frame, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            for (pixel, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_eq!(
                    *actual, expected,
                    "SVG pixels at {rotation:?}, frame {frame}, pixel {pixel}"
                );
            }
        }
        assert!(
            actual[0] != actual[1],
            "changing SVG size must change pixels"
        );
    }
}
