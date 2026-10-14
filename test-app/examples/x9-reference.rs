//! Export static, rotated CPU reference frames for X9 framebuffer comparison.
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
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    rc::Rc,
};

struct RenderPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for RenderPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("provide a new output directory")?,
    );
    fs::create_dir(&output)?;
    for (name, page) in [("controls", 1), ("scaling", 6)] {
        let path = output.join(format!("{name}.rgba"));
        std::thread::spawn(move || {
            let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
            slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
            let app = Showcase::new().unwrap();
            configure(&app).stop();
            app.set_page(page);
            app.set_level(48.0);
            app.set_image_zoom(125.0);
            app.show().unwrap();
            window.set_size(PhysicalSize::new(1280, 800));
            let mut physical = vec![PremultipliedRgbaColor::default(); 800 * 1280];
            let rendered = window.draw_if_needed(|renderer| {
                renderer.set_rendering_rotation(RenderingRotation::Rotate90);
                renderer.render(&mut physical, 800);
            });
            assert!(rendered);
            assert!(physical.iter().all(|p| p.alpha == 255));
            let bytes: Vec<u8> = physical
                .iter()
                .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
                .collect();
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap()
                .write_all(&bytes)
                .unwrap();
            app.hide().unwrap();
        })
        .join()
        .unwrap();
    }
    Ok(())
}
