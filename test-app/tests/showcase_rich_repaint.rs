use slint::platform::{
    Platform, PointerEventButton, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
};
use slint::{ComponentHandle, LogicalPosition, Model, PhysicalSize, VecModel};
use slint_bridge_test_app::{DeviceRow, Showcase, showcase::configure};
use std::rc::Rc;

struct RenderPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for RenderPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Scroll(f32, f32),
    Toggle,
    Select(i32),
    Change(usize),
    Reenter,
}

fn scene(mode: RepaintBufferType) -> (Rc<MinimalSoftwareWindow>, Showcase) {
    let window = MinimalSoftwareWindow::new(mode);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let app = Showcase::new().unwrap();
    configure(&app).stop();
    app.invoke_navigate(7);
    app.show().unwrap();
    window.set_size(PhysicalSize::new(1024, 600));
    (window, app)
}

fn apply(app: &Showcase, action: Action) {
    match action {
        Action::Scroll(delta_x, delta_y) => {
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(500., 350.),
                delta_x,
                delta_y,
            })
        }
        Action::Toggle => {
            for event in [
                WindowEvent::PointerPressed {
                    position: LogicalPosition::new(740., 138.),
                    button: PointerEventButton::Left,
                },
                WindowEvent::PointerReleased {
                    position: LogicalPosition::new(740., 138.),
                    button: PointerEventButton::Left,
                },
            ] {
                app.window().dispatch_event(event);
            }
        }
        Action::Select(id) => app.invoke_select_rich(id),
        Action::Change(index) => {
            let rows = app.get_rich_rows();
            let model = rows.as_any().downcast_ref::<VecModel<DeviceRow>>().unwrap();
            let mut row = model.row_data(index).unwrap();
            row.name = format!("Updated space {} with wrapping text", row.id).into();
            row.level = 99;
            row.online = !row.online;
            model.set_row_data(index, row);
        }
        Action::Reenter => {
            app.invoke_navigate(0);
            app.invoke_navigate(7);
        }
    }
    slint::platform::update_timers_and_animations();
}

#[test]
fn rich_scroll_repaints_clipped_items_and_offscreen_changes() {
    let actions = [
        Action::Scroll(0., 0.),
        Action::Scroll(0., -1.),
        Action::Scroll(0., -118.5),
        Action::Scroll(0., -120.),
        Action::Change(99),
        Action::Scroll(0., -4000.),
        Action::Scroll(0., -100000.),
        Action::Select(100),
        Action::Change(0),
        Action::Scroll(0., 100000.),
        Action::Select(1),
        Action::Toggle,
        Action::Scroll(-1., 0.),
        Action::Scroll(-254.5, 0.),
        Action::Scroll(-256., 0.),
        Action::Change(99),
        Action::Scroll(-12000., 0.),
        Action::Scroll(-100000., 0.),
        Action::Select(100),
        Action::Change(0),
        Action::Scroll(100000., 0.),
        Action::Select(1),
        Action::Toggle,
        Action::Scroll(0., -119.25),
        Action::Reenter,
    ];
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let reference = std::thread::spawn(move || {
        let (window, app) = scene(RepaintBufferType::NewBuffer);
        let mut pixels = vec![PremultipliedRgbaColor::default(); 1024 * 600];
        for action in actions {
            apply(&app, action);
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, 1024);
            });
            send.send(
                pixels
                    .iter()
                    .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        }
    });
    let (window, app) = scene(RepaintBufferType::ReusedBuffer);
    let mut pixels = vec![PremultipliedRgbaColor::default(); 1024 * 600];
    let mut previous = Vec::new();
    for (frame, action) in actions.into_iter().enumerate() {
        apply(&app, action);
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 1024);
        });
        let bytes: Vec<_> = pixels
            .iter()
            .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
            .collect();
        if matches!(action, Action::Scroll(x, y) if x != 0. || y != 0.) {
            assert!(bytes != previous, "scroll did not change frame {frame}");
        }
        if matches!(action, Action::Change(_)) {
            assert!(
                bytes == previous,
                "offscreen update changed visible pixels at frame {frame}"
            );
        }
        let expected = receive.recv().unwrap();
        if let Some(at) = bytes.iter().zip(&expected).position(|(a, b)| a != b) {
            panic!(
                "frame {frame} {action:?}, pixel {}, channel {}: {} != {}",
                at / 4,
                at % 4,
                bytes[at],
                expected[at]
            );
        }
        previous = bytes;
    }
    reference.join().unwrap();
}
