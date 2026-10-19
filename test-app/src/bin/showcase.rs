use slint::ComponentHandle;
use slint_bridge_test_app::{Showcase, config_from_args, showcase::configure};

fn load_background_png(data: &[u8], preview: bool) -> Result<slint::Image, slint::LoadImageError> {
    if !preview {
        match slint_bridge::try_decode_png(data) {
            Ok(Some(image)) => return Ok(image),
            Ok(None) => {}
            Err(error) => {
                eprintln!(
                    "F1_PNG_QUARANTINED: {error}; keep this process alive for device recovery"
                );
                loop {
                    std::thread::park();
                }
            }
        }
    }
    slint::Image::load_from_data(data, Some("png"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The explicit host preview uses Slint's own headless backend, never a
    // substitute completion contract for the bridge's physical framebuffer.
    let config = config_from_args();
    let preview = std::env::args().any(|arg| arg == "--preview");
    // Decode before the display canvas exists to keep F1's DMA peak bounded.
    let early_background = if cfg!(all(feature = "f1", target_arch = "riscv64")) && !preview {
        Some(load_background_png(
            include_bytes!(concat!(env!("OUT_DIR"), "/coastal-background.png")),
            preview,
        )?)
    } else {
        None
    };
    if preview {
        if config.background_storage != slint_bridge::BackgroundStorage::Heap {
            return Err("--preview only supports --background=heap".into());
        }
        #[cfg(not(feature = "showcase-preview"))]
        return Err("--preview requires --features showcase-preview".into());
        #[cfg(feature = "showcase-preview")]
        slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
            i_slint_backend_testing::TestingBackendOptions {
                mock_time: false,
                threading: true,
                renderer_name: Some("software".into()),
            },
        )))?;
    } else {
        slint_bridge::init(config.clone())?;
    }
    let app = Showcase::new()?;
    let source = match early_background {
        Some(image) => image,
        None => load_background_png(
            include_bytes!(concat!(env!("OUT_DIR"), "/coastal-background.png")),
            preview,
        )?,
    };
    app.set_wallpaper(config.prepare_background(source)?);
    let _session = configure(&app);
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
