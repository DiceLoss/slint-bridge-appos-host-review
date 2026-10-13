use crate::{DeviceRow, EventRow, Showcase};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc, time::Instant};

const PAGES: [&str; 6] = [
    "Overview",
    "Controls",
    "Devices",
    "Motion",
    "Layers",
    "Interaction lab",
];

struct Session {
    events: Rc<VecModel<EventRow>>,
    sequence: i32,
    visited: u8,
    exercised: [bool; 6],
    tap_targets: u8,
    toast_until: Instant,
}

impl Session {
    fn log(&mut self, app: &Showcase, action: impl Into<SharedString>) {
        self.sequence += 1;
        self.events.insert(
            0,
            EventRow {
                sequence: self.sequence,
                action: action.into(),
            },
        );
        if slint::Model::row_count(&*self.events) > 24 {
            self.events.remove(24);
        }
        app.set_event_count(self.sequence);
    }

    fn exercise(&mut self, app: &Showcase, index: usize) {
        self.exercised[index] = true;
        app.set_exercised(ModelRc::new(VecModel::from(self.exercised.to_vec())));
        app.set_completed(self.exercised.iter().filter(|done| **done).count() as i32);
    }

    fn notify(&mut self, app: &Showcase, message: &str) {
        app.set_toast(message.into());
        self.toast_until = Instant::now() + std::time::Duration::from_secs(3);
    }
}

fn devices() -> Vec<DeviceRow> {
    (0..18)
        .map(|i| DeviceRow {
            id: i + 1,
            name: format!(
                "{} {:02}",
                ["Light panel", "Air sensor", "Room controller"][i as usize % 3],
                i + 1
            )
            .into(),
            location: ["Studio", "Meeting room", "Lobby", "Workshop"][i as usize % 4].into(),
            online: i % 4 != 3,
            level: 35 + i * 7 % 65,
        })
        .collect()
}

fn filter_devices(app: &Showcase) {
    let query = app.get_query().trim().to_lowercase();
    app.set_devices(ModelRc::new(VecModel::from(
        devices()
            .into_iter()
            .filter(|device| {
                (!app.get_online_only() || device.online)
                    && format!("{} {}", device.name, device.location)
                        .to_lowercase()
                        .contains(&query)
            })
            .collect::<Vec<_>>(),
    )));
}

fn textures(app: &Showcase) {
    let mut image = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(640, 240);
    for (i, p) in image.make_mut_slice().iter_mut().enumerate() {
        let x = i % 640;
        let y = i / 640;
        let ridge = 135.0 + 35.0 * (x as f32 / 75.0).sin();
        let (r, g, b) = if y as f32 > ridge {
            (15 + y / 15, 83 + x / 12, 99 + y / 8)
        } else {
            (26 + x / 5, 74 + y / 3, 137 + x / 10)
        };
        *p = slint::Rgba8Pixel {
            r: r as u8,
            g: g as u8,
            b: b as u8,
            a: 255,
        };
    }
    let mut overlay = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(192, 192);
    for (i, p) in overlay.make_mut_slice().iter_mut().enumerate() {
        let x = (i % 192) as f32 - 96.0;
        let y = (i / 192) as f32 - 96.0;
        let radius = (x * x + y * y).sqrt();
        *p = slint::Rgba8Pixel {
            r: 100,
            g: 255,
            b: 209,
            a: ((1.0 - radius / 96.0).clamp(0.0, 1.0) * 255.0) as u8,
        };
    }
    app.set_texture(slint::Image::from_rgba8(image));
    app.set_overlay(slint::Image::from_rgba8(overlay));
}

pub fn configure(app: &Showcase) -> slint::Timer {
    let session = Rc::new(RefCell::new(Session {
        events: Rc::new(VecModel::default()),
        sequence: 0,
        visited: 1,
        exercised: [false; 6],
        tap_targets: 0,
        toast_until: Instant::now(),
    }));
    app.set_events(session.borrow().events.clone().into());
    app.set_exercised(ModelRc::new(VecModel::from(vec![false; 6])));
    filter_devices(app);
    textures(app);
    session.borrow_mut().log(app, "Session started");

    let (weak, state) = (app.as_weak(), session.clone());
    app.on_navigate(move |page| {
        let Some(app) = weak.upgrade() else { return };
        let Some(name) = PAGES.get(page as usize) else {
            return;
        };
        app.set_page(page);
        let mut state = state.borrow_mut();
        state.visited |= 1 << page;
        if state.visited == 63 {
            state.exercise(&app, 0);
        }
        state.log(&app, format!("Opened {name}"));
    });
    let (weak, state) = (app.as_weak(), session.clone());
    app.on_apply_profile(move || {
        let Some(app) = weak.upgrade() else { return };
        let mut state = state.borrow_mut();
        let name = app.get_station_name().trim().to_owned();
        if name.is_empty() || name.chars().count() > 24 {
            state.notify(&app, "Use a station name of 1–24 characters.");
            return;
        }
        app.set_station_name(name.clone().into());
        let mode = ["Balanced", "Quiet", "Performance"][app.get_mode().clamp(0, 2) as usize];
        app.set_saved_summary(
            format!(
                "{name} · {mode} · {:.0}% · {} · Alerts {}",
                app.get_level(),
                if app.get_power() { "On" } else { "Off" },
                if app.get_notifications() { "On" } else { "Off" }
            )
            .into(),
        );
        state.exercise(&app, 1);
        state.log(&app, "Applied demo profile");
        state.notify(&app, "Profile applied to this demo session.");
    });
    let weak = app.as_weak();
    app.on_open_dialog(move |dialog| {
        if let Some(app) = weak.upgrade() {
            if dialog == 3 {
                app.set_keyboard_draft(app.get_station_name());
            } else if dialog == 4 {
                app.set_keyboard_draft(app.get_query());
            }
            app.set_dialog(dialog);
        }
    });
    let (weak, state) = (app.as_weak(), session.clone());
    app.on_confirm_reset(move || {
        if let Some(app) = weak.upgrade() {
            app.set_station_name("NX5 Studio".into());
            app.set_level(68.0);
            app.set_power(true);
            app.set_notifications(true);
            app.set_mode(0);
            app.set_saved_summary("No profile applied yet.".into());
            app.set_dialog(0);
            state.borrow_mut().log(&app, "Reset demo profile");
        }
    });
    let weak = app.as_weak();
    app.on_key(move |key| {
        if let Some(app) = weak.upgrade() {
            let mut draft = app.get_keyboard_draft().to_string();
            match key.as_str() {
                "done" => {
                    if app.get_dialog() == 4 {
                        app.set_query(draft.into());
                        filter_devices(&app);
                    } else {
                        app.set_station_name(draft.into());
                    }
                    app.set_dialog(0);
                    return;
                }
                "backspace" => {
                    draft.pop();
                }
                _ if draft.chars().count() < 24 => draft.push_str(&key),
                _ => {}
            }
            app.set_keyboard_draft(draft.into());
        }
    });
    let weak = app.as_weak();
    app.on_filter(move || {
        if let Some(app) = weak.upgrade() {
            filter_devices(&app);
        }
    });
    let (weak, state) = (app.as_weak(), session.clone());
    app.on_select_device(move |id| {
        if let Some(app) = weak.upgrade()
            && let Some(device) = devices().into_iter().find(|d| d.id == id)
        {
            app.set_selected_name(device.name.clone());
            app.set_device_details(
                format!(
                    "{}\n{} · Level {}%\n\nSimulated device. No network connection.",
                    device.location,
                    if device.online { "Online" } else { "Offline" },
                    device.level
                )
                .into(),
            );
            app.set_dialog(2);
            let mut state = state.borrow_mut();
            state.exercise(&app, 2);
            state.log(&app, format!("Selected {}", device.name));
        }
    });
    let (weak, state) = (app.as_weak(), session.clone());
    app.on_toggle_motion(move || {
        if let Some(app) = weak.upgrade() {
            app.set_running(!app.get_running());
            state.borrow_mut().log(
                &app,
                if app.get_running() {
                    "Motion resumed"
                } else {
                    "Motion paused"
                },
            );
        }
    });
    let (weak, state) = (app.as_weak(), session.clone());
    app.on_replay(move || {
        if let Some(app) = weak.upgrade() {
            app.set_expanded(!app.get_expanded());
            let mut state = state.borrow_mut();
            state.exercise(&app, 3);
            state.log(&app, "Replayed native transition");
        }
    });
    let (weak, state) = (app.as_weak(), session.clone());
    app.on_toggle_cutout(move || {
        if let Some(app) = weak.upgrade() {
            app.set_cutout(!app.get_cutout());
            state.borrow_mut().log(
                &app,
                if app.get_cutout() {
                    "Opened transparent area"
                } else {
                    "Restored test image"
                },
            );
        }
    });
    let (weak, state) = (app.as_weak(), session.clone());
    app.on_blend_changed(move || {
        if let Some(app) = weak.upgrade() {
            let mut state = state.borrow_mut();
            // A slider emits many updates while dragged; journal the exercise once.
            if !state.exercised[4] {
                state.log(&app, "Adjusted image opacity");
                state.exercise(&app, 4);
            }
        }
    });
    let (weak, state) = (app.as_weak(), session.clone());
    app.on_tap(move |target| {
        if let Some(app) = weak.upgrade() {
            if !(0..3).contains(&target) {
                return;
            }
            app.set_taps(app.get_taps() + 1);
            let mut state = state.borrow_mut();
            state.tap_targets |= 1 << target;
            if state.tap_targets == 7 && app.get_drag_count() > 0 {
                state.exercise(&app, 5);
            }
            state.log(&app, format!("Tapped target {}", target + 1));
        }
    });
    let (weak, state) = (app.as_weak(), session.clone());
    app.on_dragged(move || {
        if let Some(app) = weak.upgrade() {
            app.set_drag_count(app.get_drag_count() + 1);
            let mut state = state.borrow_mut();
            if state.tap_targets == 7 {
                state.exercise(&app, 5);
            }
            state.log(&app, "Released drag pad");
        }
    });
    let state = session.clone();
    app.on_clear_journal(move || {
        state.borrow().events.set_vec(Vec::new());
    });

    let samples = Rc::new(VecModel::from(
        (0..32)
            .map(|i| 45.0 + 27.0 * (i as f32 * 0.3).sin())
            .collect::<Vec<_>>(),
    ));
    app.set_samples(samples.clone().into());
    let weak = app.as_weak();
    let last = Rc::new(RefCell::new(Instant::now()));
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(33),
        move || {
            let now = Instant::now();
            let elapsed = now.duration_since(*last.borrow()).as_secs_f32();
            *last.borrow_mut() = now;
            if let Some(app) = weak.upgrade() {
                if !app.get_toast().is_empty() && now >= session.borrow().toast_until {
                    app.set_toast("".into());
                }
                if !app.get_running() || !matches!(app.get_page(), 0 | 3) || app.get_dialog() != 0 {
                    return;
                }
                let ticks = (app.get_ticks() + 1) % 1_000_000;
                app.set_ticks(ticks);
                app.set_phase((app.get_phase() + elapsed * 65.0 * app.get_speed()) % 360.0);
                if app.get_page() == 0 && ticks % 6 == 0 {
                    samples.remove(0);
                    samples.push(48.0 + 30.0 * app.get_phase().to_radians().sin());
                }
            }
        },
    );
    timer
}
