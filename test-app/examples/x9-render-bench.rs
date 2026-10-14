//! Deterministic CPU render benchmark; no framebuffer or display SDK access.
use slint::{
    ComponentHandle, LogicalPosition, PhysicalSize,
    platform::{
        Platform, PointerEventButton, WindowAdapter, WindowEvent,
        software_renderer::{
            MinimalSoftwareWindow, PremultipliedRgbaColor, RenderingRotation, RepaintBufferType,
        },
    },
};
use slint_bridge_test_app::{Showcase, showcase::configure};
use std::{io::Write, rc::Rc, time::Instant};

struct RenderPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for RenderPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let rotation = match args.get(1).map(String::as_str) {
        Some("0") => RenderingRotation::NoRotation,
        Some("90") => RenderingRotation::Rotate90,
        _ => return Err("rotation must be 0 or 90".into()),
    };
    let horizontal = match args.get(2).map(String::as_str) {
        Some("vertical") => false,
        Some("horizontal") => true,
        _ => return Err("axis must be vertical or horizontal".into()),
    };
    let frames: usize = args.get(3).ok_or("provide frame count")?.parse()?;
    if !(1..=300).contains(&frames) {
        return Err("frame count must be 1..300".into());
    }
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone())))?;
    let app = Showcase::new()?;
    configure(&app).stop();
    app.invoke_navigate(7);
    app.show()?;
    window.set_size(PhysicalSize::new(1280, 800));
    let stride = if rotation == RenderingRotation::Rotate90 {
        800
    } else {
        1280
    };
    let mut pixels = vec![PremultipliedRgbaColor::default(); 1280 * 800];
    window.draw_if_needed(|renderer| {
        renderer.set_rendering_rotation(rotation);
        renderer.render(&mut pixels, stride);
    });
    if horizontal {
        for event in [
            WindowEvent::PointerPressed {
                position: LogicalPosition::new(900., 138.),
                button: PointerEventButton::Left,
            },
            WindowEvent::PointerReleased {
                position: LogicalPosition::new(900., 138.),
                button: PointerEventButton::Left,
            },
        ] {
            app.window().dispatch_event(event);
        }
    }
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(500., 350.),
        delta_x: if horizontal { -3200. } else { 0. },
        delta_y: if horizontal { 0. } else { -2160. },
    });
    println!("frame,event_us,render_us,dirty_pixels");
    for frame in 0..frames + 20 {
        let start = Instant::now();
        app.window().dispatch_event(WindowEvent::PointerScrolled {
            position: LogicalPosition::new(500., 350.),
            delta_x: if horizontal { -8. } else { 0. },
            delta_y: if horizontal { 0. } else { -8. },
        });
        slint::platform::update_timers_and_animations();
        let event_us = start.elapsed().as_micros();
        window.request_redraw();
        let mut elapsed = 0;
        let mut dirty = 0u64;
        assert!(window.draw_if_needed(|renderer| {
            let start = Instant::now();
            let region = renderer.render(&mut pixels, stride);
            elapsed = start.elapsed().as_micros();
            dirty = region
                .iter()
                .map(|(_, size)| u64::from(size.width) * u64::from(size.height))
                .sum();
        }));
        if frame >= 20 {
            assert!(dirty > 0, "scroll produced no damage");
            println!("{},{event_us},{elapsed},{dirty}", frame - 20);
        }
    }
    let hash = pixels
        .iter()
        .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    eprintln!("rotation={rotation:?} horizontal={horizontal} frames={frames} hash={hash:016x}");
    if let Some(path) = args.get(4) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let bytes: Vec<_> = pixels
            .iter()
            .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
            .collect();
        file.write_all(&bytes)?;
    }
    app.hide()?;
    Ok(())
}
