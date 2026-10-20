use slint::platform::{
    Platform, WindowAdapter,
    software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
};
use slint::{ComponentHandle, PhysicalSize};
use slint_bridge_test_app::{Showcase, showcase::configure};
use std::rc::Rc;

struct RenderPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for RenderPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn scene(mode: RepaintBufferType, zoom: f32) -> (Rc<MinimalSoftwareWindow>, Showcase) {
    let window = MinimalSoftwareWindow::new(mode);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let app = Showcase::new().unwrap();
    configure(&app).stop();
    app.set_page(6);
    app.set_image_zoom(zoom);
    app.show().unwrap();
    window.set_size(PhysicalSize::new(1024, 600));
    (window, app)
}

#[test]
fn png_and_svg_resize_at_runtime_and_clear_previous_extent() {
    let (window, app) = scene(RepaintBufferType::ReusedBuffer, 100.0);
    let mut pixels = vec![PremultipliedRgbaColor::default(); 1024 * 600];
    for zoom in [
        50.0, 100.0, 125.0, 148.0, 148.02744, 149.0, 150.0, 200.0, 149.0, 50.0, 100.0,
    ] {
        app.set_image_zoom(zoom);
        for image in [app.get_scaling_png(), app.get_scaling_svg()] {
            assert_eq!((image.size().width, image.size().height), (256, 160));
        }
        slint::platform::update_timers_and_animations();
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 1024);
        });
        // Independent full repaint: never reset the tested renderer's retained cache.
        let reference = std::thread::spawn(move || {
            let (_window, app) = scene(RepaintBufferType::NewBuffer, zoom);
            app.window().take_snapshot().unwrap().as_slice().to_vec()
        })
        .join()
        .unwrap();
        for y in 207..460 {
            for x in 198..1000 {
                let at = y * 1024 + x;
                let actual = pixels[at];
                let expected = reference[at];
                assert_eq!(
                    [actual.red, actual.green, actual.blue, actual.alpha],
                    [expected.r, expected.g, expected.b, expected.a],
                    "retained image at {zoom}%, ({x}, {y})"
                );
            }
        }
        // The solid interior excludes the outer stroke and white cross (1 px each).
        // Its size must change with zoom; allow two pixels for filtering/rounding.
        if zoom <= 150.0 {
            for (format, range) in [("PNG", 210..580), ("SVG", 618..988)] {
                let red = (220..450)
                    .flat_map(|y| range.clone().map(move |x| (x, y)))
                    .filter(|&(x, y)| {
                        let p = pixels[y * 1024 + x];
                        [p.red, p.green, p.blue, p.alpha] == [217, 75, 64, 255]
                    })
                    .collect::<Vec<_>>();
                assert!(!red.is_empty(), "missing {format} red quadrant");
                let width = red.iter().map(|p| p.0).max().unwrap()
                    - red.iter().map(|p| p.0).min().unwrap()
                    + 1;
                let height = red.iter().map(|p| p.1).max().unwrap()
                    - red.iter().map(|p| p.1).min().unwrap()
                    + 1;
                assert!(
                    (width as f32 - 110.0 * zoom / 100.0).abs() <= 2.0,
                    "{format} quadrant width {width} at {zoom}%"
                );
                assert!(
                    (height as f32 - 62.0 * zoom / 100.0).abs() <= 2.0,
                    "{format} quadrant height {height} at {zoom}%"
                );
            }
        }
    }
    app.hide().unwrap();
}
