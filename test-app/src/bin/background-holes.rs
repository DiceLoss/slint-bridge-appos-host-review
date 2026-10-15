use slint::ComponentHandle;
use slint_bridge_test_app::{BackgroundHolesDemo, background_demo, config_from_args};

#[cfg(feature = "nx5")]
#[path = "background-holes/verify.rs"]
mod verify;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = config_from_args();
    #[cfg(feature = "nx5")]
    let framebuffer = config.framebuffer.clone();
    slint_bridge::init(config)?;
    let app = BackgroundHolesDemo::new()?;
    background_demo::configure(&app);
    if std::env::args().any(|arg| arg == "--verify") {
        #[cfg(feature = "nx5")]
        return verify::run(&app, &framebuffer);
        #[cfg(not(feature = "nx5"))]
        return Err("--verify requires NX5 and libbackground-checker.so".into());
    }
    let exit_timer = slint::Timer::default();
    if let Some(seconds) =
        std::env::args().find_map(|arg| arg.strip_prefix("--seconds=").map(str::parse::<u64>))
    {
        exit_timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_secs(seconds?),
            || {
                let _ = slint::quit_event_loop();
            },
        );
    }
    app.run()?;
    Ok(())
}
