//! NX5 startup/lifetime regression. Requires exclusive ownership of the display.
use slint::ComponentHandle;
use std::{
    fs::{self, File},
    os::unix::fs::FileExt,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

include!(concat!(env!("OUT_DIR"), "/startup.rs"));

fn snapshot(output: &Path, stage: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mmz = fs::read_to_string("/proc/media-mem")?;
    assert_eq!(mmz.matches("name=\"bridge_tde_targe\"").count(), 1);
    assert_eq!(mmz.matches("name=\"bridge_tde_sourc\"").count(), 2);
    fs::write(output.join(format!("{stage}.mmz")), mmz)?;
    fs::write(
        output.join(format!("{stage}.vo")),
        fs::read("/proc/umap/vo")?,
    )?;
    eprintln!("startup: stage={stage} tde_blocks=3");
    Ok(())
}

fn reject_show(ui: &StartupWindow) -> slint::PlatformError {
    ui.set_required_width(2048.0);
    let error = ui.run().expect_err("oversized root must be rejected");
    assert!(error.to_string().contains("window layout constraints"));
    assert!(!ui.window().is_visible(), "rejected root remained visible");
    eprintln!("startup: rejected visible=false error={error}");
    error
}

fn quit_later() {
    slint::Timer::single_shot(Duration::from_millis(700), || {
        slint::quit_event_loop().unwrap();
    });
}

fn front_page() -> Result<(File, u64, String), Box<dyn std::error::Error>> {
    let pan = fs::read_to_string("/sys/class/graphics/fb0/pan")?;
    let (x, y) = pan.trim().split_once(',').expect("framebuffer pan");
    let y: u64 = y.parse()?;
    assert_eq!(x, "0");
    assert!(y == 0 || y == 600);
    assert_eq!(
        fs::read_to_string("/sys/class/graphics/fb0/stride")?.trim(),
        "4096"
    );
    Ok((File::open("/dev/fb0")?, y, pan))
}

fn next_color() -> Result<(slint::Color, [u8; 4]), Box<dyn std::error::Error>> {
    let (file, y, _) = front_page()?;
    let mut previous = [0; 4];
    file.read_exact_at(&mut previous, y * 4096)?;
    eprintln!("startup: previous_bgra={previous:?}");
    // The requested color must differ even if the preceding process left a
    // uniform frame. Stale framebuffer contents cannot satisfy the assertion.
    Ok(if previous == [0, 255, 0, 255] {
        (slint::Color::from_rgb_u8(0, 0, 255), [255, 0, 0, 255])
    } else {
        (slint::Color::from_rgb_u8(0, 255, 0), [0, 255, 0, 255])
    })
}

fn check_front_pixel_data(expected: [u8; 4]) -> Result<(), Box<dyn std::error::Error>> {
    // run_event_loop has drained presentation. Readback is an output check,
    // not a substitute for the board's page-retirement contract or LCD capture.
    let (file, y, pan) = front_page()?;
    let mut pixels = vec![0; 4096 * 600];
    file.read_exact_at(&mut pixels, y * 4096)?;
    let bad = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| **pixel != expected)
        .count();
    eprintln!("startup: yoffset={y} expected_bgra={expected:?} pixels=614400 bad_pixels={bad}");
    assert_eq!(bad, 0, "front page does not contain the requested frame");
    assert_eq!(fs::read_to_string("/sys/class/graphics/fb0/pan")?, pan);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: startup-regression show-error|retry|queued|timer|queued-retry OUTPUT_DIRECTORY"
    );
    let mode = args[1].as_str();
    assert!(matches!(
        mode,
        "show-error" | "retry" | "queued" | "timer" | "queued-retry"
    ));
    let output = Path::new(&args[2]);
    assert!(output.is_dir());
    slint_bridge::init(slint_bridge::Config::default())?;
    snapshot(output, "initialized")?;
    let timed_out = Arc::new(AtomicBool::new(false));
    let watchdog = timed_out.clone();
    slint::Timer::single_shot(Duration::from_secs(5), move || {
        watchdog.store(true, Ordering::SeqCst);
        let _ = slint::quit_event_loop();
    });
    if mode == "show-error" {
        let ui = StartupWindow::new()?;
        let error = reject_show(&ui);
        snapshot(output, "rejected")?;
        return Err(error.into());
    }
    if mode == "retry" {
        let ui = StartupWindow::new()?;
        let invoked = Arc::new(AtomicBool::new(false));
        let marker = invoked.clone();
        slint::invoke_from_event_loop(move || marker.store(true, Ordering::SeqCst))?;
        for _ in 0..2 {
            reject_show(&ui);
        }
        snapshot(output, "rejected")?;
        ui.set_required_width(0.0);
        for _ in 0..2 {
            let (color, bgra) = next_color()?;
            ui.set_paint(color);
            quit_later();
            ui.run()?;
            assert!(!ui.window().is_visible());
            assert!(invoked.load(Ordering::SeqCst), "startup task was discarded");
            check_front_pixel_data(bgra)?;
        }
        eprintln!("startup: same_component_retry=pass queued_task=true");
    } else {
        let root = Arc::new(Mutex::new(None));
        let saved = root.clone();
        let retry = mode == "queued-retry";
        let (color, bgra) = next_color()?;
        let create = move || {
            let ui = StartupWindow::new().unwrap();
            if retry {
                reject_show(&ui);
            }
            ui.set_required_width(0.0);
            ui.set_paint(color);
            ui.show().unwrap();
            *saved.lock().unwrap() = Some(ui.as_weak());
            eprintln!("startup: deferred_window_created");
            quit_later();
        };
        if mode == "timer" {
            slint::Timer::single_shot(Duration::from_millis(150), create);
        } else {
            // Exercise a startup task beyond the loop's per-iteration budget.
            for _ in 0..128 {
                slint::invoke_from_event_loop(|| {})?;
            }
            slint::invoke_from_event_loop(create)?;
        }
        eprintln!("startup: enter_loop_without_window mode={mode}");
        slint::run_event_loop()?;
        let ui = root
            .lock()
            .unwrap()
            .take()
            .expect("root created")
            .upgrade()
            .expect("visible root");
        check_front_pixel_data(bgra)?;
        ui.hide()?;
        assert!(!ui.window().is_visible());
        eprintln!("startup: deferred_start=pass mode={mode}");
    }
    assert!(
        !timed_out.load(Ordering::SeqCst),
        "startup watchdog expired"
    );
    snapshot(output, "finished")?;
    Ok(())
}
