use slint::{
    ComponentHandle, PhysicalSize,
    platform::{
        Platform, WindowAdapter,
        software_renderer::{
            MinimalSoftwareWindow, PremultipliedRgbaColor, RenderingRotation, RepaintBufferType,
        },
    },
};
use slint_bridge_test_app::{Showcase, showcase::configure};
use std::rc::Rc;

struct RenderPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for RenderPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

#[test]
fn snapshots_use_logical_orientation_and_preserve_panel_rendering() {
    for rotation in [
        RenderingRotation::NoRotation,
        RenderingRotation::Rotate90,
        RenderingRotation::Rotate180,
        RenderingRotation::Rotate270,
    ] {
        std::thread::spawn(move || {
            let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
            slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
            let app = Showcase::new().unwrap();
            configure(&app).stop();
            app.set_page(1);
            app.show().unwrap();
            window.set_size(PhysicalSize::new(1280, 800));
            let reference = app.window().take_snapshot().unwrap();
            let stride = if matches!(
                rotation,
                RenderingRotation::Rotate90 | RenderingRotation::Rotate270
            ) {
                800
            } else {
                1280
            };
            let mut physical = vec![PremultipliedRgbaColor::default(); 1280 * 800];
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.set_rendering_rotation(rotation);
                renderer.render(&mut physical, stride);
            });
            let snapshot = app.window().take_snapshot().unwrap();
            assert_eq!((snapshot.width(), snapshot.height()), (1280, 800));
            assert_eq!(
                snapshot
                    .as_slice()
                    .iter()
                    .zip(reference.as_slice())
                    .position(|(a, b)| a != b),
                None,
                "{rotation:?}"
            );
            let before = physical.clone();
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                assert_eq!(renderer.rendering_rotation(), rotation);
                assert_eq!(
                    renderer.repaint_buffer_type(),
                    RepaintBufferType::ReusedBuffer
                );
                renderer.render(&mut physical, stride);
            });
            let rgba = |p: &PremultipliedRgbaColor| [p.red, p.green, p.blue, p.alpha];
            assert_eq!(
                physical
                    .iter()
                    .zip(&before)
                    .position(|(a, b)| rgba(a) != rgba(b)),
                None,
                "snapshot changed subsequent panel rendering"
            );
            app.hide().unwrap();
        })
        .join()
        .unwrap();
    }
}

#[test]
fn rotated_svg_and_png_keep_their_requested_raster_resolution() {
    for rotation in [
        RenderingRotation::NoRotation,
        RenderingRotation::Rotate90,
        RenderingRotation::Rotate180,
        RenderingRotation::Rotate270,
    ] {
        std::thread::spawn(move || {
            let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
            slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
            let app = Showcase::new().unwrap();
            configure(&app).stop();
            app.set_page(6);
            app.set_image_zoom(125.0);
            app.show().unwrap();
            window.set_size(PhysicalSize::new(1280, 800));
            let reference = app.window().take_snapshot().unwrap();
            let transposed = matches!(
                rotation,
                RenderingRotation::Rotate90 | RenderingRotation::Rotate270
            );
            let stride = if transposed { 800 } else { 1280 };
            let mut physical = vec![PremultipliedRgbaColor::default(); 1280 * 800];
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.set_rendering_rotation(rotation);
                renderer.render(&mut physical, stride);
            });
            // Both image elements occupy 320x200 at 125%, including transparent margins.
            for (format, x_start) in [("PNG", 299), ("SVG", 835)] {
                let mut mismatches = 0;
                for y in 346..546 {
                    for x in x_start..x_start + 320 {
                        let (px, py) = match rotation {
                            RenderingRotation::NoRotation => (x, y),
                            RenderingRotation::Rotate90 => (799 - y, x),
                            RenderingRotation::Rotate180 => (1279 - x, 799 - y),
                            RenderingRotation::Rotate270 => (y, 1279 - x),
                            _ => unreachable!(),
                        };
                        let a = physical[py * stride + px];
                        let b = reference.as_slice()[y * 1280 + x];
                        mismatches +=
                            usize::from([a.red, a.green, a.blue, a.alpha] != [b.r, b.g, b.b, b.a]);
                    }
                }
                assert_eq!(mismatches, 0, "{format} at {rotation:?}");
            }
            app.hide().unwrap();
        })
        .join()
        .unwrap();
    }
}
