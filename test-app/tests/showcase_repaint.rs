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

fn scene(mode: RepaintBufferType) -> (Rc<MinimalSoftwareWindow>, Showcase) {
    let window = MinimalSoftwareWindow::new(mode);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let app = Showcase::new().unwrap();
    configure(&app).stop();
    app.show().unwrap();
    window.set_size(PhysicalSize::new(1024, 600));
    (window, app)
}

#[test]
fn cached_title_repaints_after_animation_and_navigation() {
    let (window, app) = scene(RepaintBufferType::ReusedBuffer);
    let mut pixels = vec![PremultipliedRgbaColor::default(); 1024 * 600];
    // Churn the real overview text model without wall-clock sleeps. The title
    // remains unchanged and outside the animated regions while caches age.
    for tick in 0..180 {
        app.set_ticks(tick);
        slint::platform::update_timers_and_animations();
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 1024);
        });
    }
    for page in [1, 2, 4, 0, 2] {
        app.invoke_navigate(page);
        slint::platform::update_timers_and_animations();
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 1024);
        });
        // An independent renderer keeps the tested partial-render cache intact.
        let reference = std::thread::spawn(move || {
            let (_window, app) = scene(RepaintBufferType::NewBuffer);
            app.invoke_navigate(page);
            app.window().take_snapshot().unwrap().as_slice().to_vec()
        })
        .join()
        .unwrap();
        for (at, (actual, expected)) in pixels[..82 * 1024].iter().zip(reference).enumerate() {
            assert_eq!(
                [actual.red, actual.green, actual.blue, actual.alpha],
                [expected.r, expected.g, expected.b, expected.a],
                "header on page {page}, pixel {at}"
            );
        }
    }
}
