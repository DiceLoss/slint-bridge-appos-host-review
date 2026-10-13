use slint::platform::{
    Platform, WindowAdapter,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use slint::{ComponentHandle, Model, PhysicalSize};
use slint_bridge_test_app::{Showcase, showcase::configure};
use std::rc::Rc;

struct RenderPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for RenderPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

#[test]
fn showcase_state_forms_filtering_and_transparency() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let app = Showcase::new().unwrap();
    let _timer = configure(&app);
    app.show().unwrap();
    window.set_size(PhysicalSize::new(1024, 600));

    // Invalid input must not replace the last applied profile or complete the exercise.
    app.set_station_name("   ".into());
    app.invoke_apply_profile();
    assert_eq!(app.get_completed(), 0);
    assert_eq!(app.get_saved_summary(), "No profile applied yet.");
    app.set_station_name("Workshop".into());
    app.set_level(27.0);
    app.set_power(false);
    app.invoke_apply_profile();
    assert!(
        app.get_saved_summary()
            .contains("Workshop · Balanced · 27% · Off")
    );
    app.invoke_navigate(2);
    app.invoke_navigate(1);
    assert_eq!(app.get_level(), 27.0);
    app.invoke_open_dialog(1);
    app.invoke_open_dialog(0);
    assert_eq!(app.get_level(), 27.0, "cancel must preserve form values");
    app.invoke_confirm_reset();
    assert_eq!(app.get_level(), 68.0);
    assert_eq!(app.get_station_name(), "NX5 Studio");

    // Keyboard cancellation is isolated from the form and backspace handles Unicode.
    app.set_station_name("é".into());
    app.invoke_open_dialog(3);
    app.invoke_key("backspace".into());
    assert_eq!(app.get_keyboard_draft(), "");
    app.invoke_key("A".into());
    app.invoke_open_dialog(0);
    assert_eq!(app.get_station_name(), "é");
    app.invoke_open_dialog(3);
    app.invoke_key("B".into());
    app.invoke_key("done".into());
    assert_eq!(app.get_station_name(), "éB");

    app.set_query("workshop".into());
    app.invoke_filter();
    assert_eq!(app.get_devices().row_count(), 4);
    app.set_online_only(true);
    app.invoke_filter();
    assert_eq!(app.get_devices().row_count(), 0);
    app.set_query("LIGHT".into());
    app.invoke_filter();
    let device = app.get_devices().row_data(1).unwrap();
    app.invoke_select_device(device.id);
    assert_eq!(
        app.get_selected_name(),
        device.name,
        "selection uses the original id after filtering"
    );
    app.invoke_open_dialog(0);

    // The real embedded bitmap must decode, and changing it must affect the
    // scrolling/animation content, not just the gutters around opaque cards.
    let size = app.get_wallpaper().size();
    assert_eq!((size.width, size.height), (1024, 600));
    assert!(
        app.get_wallpaper().to_rgb8().is_none(),
        "TDE background must be RGBA"
    );
    assert!(
        app.get_wallpaper()
            .to_rgba8()
            .unwrap()
            .as_slice()
            .iter()
            .all(|p| p.a == 255)
    );
    let photograph = app.get_wallpaper();
    let mut swatch = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(1, 1);
    swatch.make_mut_slice()[0] = slint::Rgba8Pixel {
        r: 220,
        g: 30,
        b: 20,
        a: 255,
    };
    let red = slint::Image::from_rgba8(swatch.clone());
    swatch.make_mut_slice()[0] = slint::Rgba8Pixel {
        r: 20,
        g: 30,
        b: 220,
        a: 255,
    };
    let blue = slint::Image::from_rgba8(swatch);
    for (page, x, y) in [(2, 700, 285), (3, 450, 280), (4, 240, 310)] {
        app.invoke_navigate(page);
        app.set_wallpaper(red.clone());
        let red_frame = app.window().take_snapshot().unwrap();
        app.set_wallpaper(blue.clone());
        let blue_frame = app.window().take_snapshot().unwrap();
        let at = y * 1024 + x;
        assert_ne!(
            red_frame.as_slice()[at],
            blue_frame.as_slice()[at],
            "page {page} hides the bitmap"
        );
    }
    app.set_wallpaper(photograph);

    app.invoke_navigate(4);
    app.invoke_toggle_cutout();
    app.set_alpha(0.0);
    app.invoke_blend_changed();
    app.set_toast("".into());
    let snapshot = app.window().take_snapshot().unwrap();
    // Central unpainted media opening is genuinely transparent, not a black rectangle.
    assert_eq!(snapshot.as_slice()[300 * 1024 + 240].a, 0);
    assert_eq!(snapshot.as_slice()[30 * 1024 + 30].a, 255);
    app.invoke_toggle_cutout();
    let snapshot = app.window().take_snapshot().unwrap();
    assert_eq!(snapshot.as_slice()[300 * 1024 + 240].a, 255);

    app.invoke_replay();
    app.invoke_toggle_motion();
    assert!(!app.get_running());
    for target in 0..3 {
        app.invoke_tap(target);
    }
    app.invoke_dragged();
    for page in 0..6 {
        app.invoke_navigate(page);
    }
    assert_eq!(app.get_completed(), 6);
    for _ in 0..30 {
        app.invoke_tap(0);
    }
    assert_eq!(app.get_events().row_count(), 24);
    let sequence = app.get_event_count();
    app.invoke_clear_journal();
    assert_eq!(app.get_events().row_count(), 0);
    app.invoke_tap(0);
    assert_eq!(app.get_events().row_data(0).unwrap().sequence, sequence + 1);
    app.hide().unwrap();
}
