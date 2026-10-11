//! On-device motion/touch demo. --pattern emits uniform alternating frames for WBC.
use slint::ComponentHandle;
use slint_bridge_test_app::{App, config_from_args, configure_textures};
use std::{cell::Cell, rc::Rc};

#[cfg(feature = "nx5")]
mod nx5_observer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = config_from_args();
    let frame_interval = config.frame_interval;
    slint_bridge::init(config)?;
    let app = App::new()?;
    app.set_pattern(std::env::args().any(|arg| arg == "--pattern"));
    app.set_split_pattern(std::env::args().any(|arg| arg == "--split"));
    if std::env::args().any(|arg| arg == "--textures") {
        configure_textures(&app);
    }
    let start = std::time::Instant::now();
    let seconds = std::env::args().find_map(|arg| {
        arg.strip_prefix("--seconds=")
            .map(|s| s.parse::<u64>().expect("seconds"))
    });
    #[cfg(feature = "nx5")]
    let observation =
        std::env::args().find_map(|arg| arg.strip_prefix("--observe=").map(str::to_owned));
    #[cfg(feature = "nx5")]
    let observer = Rc::new(std::cell::RefCell::new(None));
    #[cfg(feature = "nx5")]
    let timer_observer = observer.clone();
    let weak = app.as_weak();
    app.on_tapped(move || {
        if let Some(app) = weak.upgrade() {
            app.set_paused(!app.get_paused());
        }
    });
    let frame = Rc::new(Cell::new(0));
    let weak = app.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, frame_interval, move || {
        if seconds.is_some_and(|seconds| start.elapsed().as_secs() >= seconds) {
            let _ = slint::quit_event_loop();
            return;
        }
        #[cfg(feature = "nx5")]
        if start.elapsed() >= std::time::Duration::from_secs(2)
            && timer_observer.borrow().is_none()
            && observation.is_some()
        {
            *timer_observer.borrow_mut() = Some(nx5_observer::start(observation.clone().unwrap()));
        }
        if let Some(app) = weak.upgrade() {
            if app.get_paused() {
                return;
            }
            let next = (frame.get() + 1) % 30_000;
            frame.set(next);
            app.set_frame(next);
            app.set_bar_x(((next * 12) % 924) as f32);
            // Hold each diagnostic color for two UI ticks so rendering ahead
            // of a two-refresh display does not repeatedly sample one parity.
            app.set_white_frame((next / 2) % 2 == 0);
        }
    });
    // On error, dropping the timer and shared Observer also joins WBC cleanup.
    app.run()?;
    timer.stop();
    #[cfg(feature = "nx5")]
    if let Some(observer) = observer.borrow_mut().take() {
        observer.join().map_err(|_| "WBC observer panicked")?;
    }
    Ok(())
}
