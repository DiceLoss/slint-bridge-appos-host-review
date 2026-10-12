//! NX5 startup/lifetime regression. Requires exclusive ownership of the display.
use slint::ComponentHandle;
use std::{
    fs::{self, File},
    os::{fd::AsRawFd, unix::fs::FileExt},
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
    ui.set_required_width(ui.window().size().width as f32 + 1.0);
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

// Native Linux fb UAPI, kept local to this diagnostic rather than exposing
// private library state through the public platform API.
#[repr(C)]
#[derive(Default)]
struct FixedInfo {
    id: [libc::c_char; 16],
    smem_start: libc::c_ulong,
    smem_len: u32,
    kind: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: libc::c_ulong,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}
#[repr(C)]
#[derive(Default)]
struct VariableInfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    channels: [[u32; 3]; 4],
    remaining: [u32; 20],
}

fn front_page(path: &Path) -> Result<(File, FixedInfo, VariableInfo), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut fixed = FixedInfo::default();
    let mut variable = VariableInfo::default();
    // SAFETY: native Linux framebuffer UAPI buffers, checked below and in tests.
    if unsafe { libc::ioctl(file.as_raw_fd(), 0x4602, &mut fixed) } < 0
        || unsafe { libc::ioctl(file.as_raw_fd(), 0x4600, &mut variable) } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    assert!(variable.xres > 0 && variable.yres > 0);
    assert_eq!(variable.xoffset, 0);
    assert!(variable.yoffset == 0 || variable.yoffset == variable.yres);
    // The color diagnostic checks BGRA bytes; the platform itself also supports RGB565.
    assert_eq!(variable.bits_per_pixel, 32);
    assert_eq!(
        variable.channels,
        [[16, 8, 0], [8, 8, 0], [0, 8, 0], [24, 8, 0]]
    );
    assert!(u64::from(fixed.line_length) >= u64::from(variable.xres) * 4);
    assert!(
        u64::from(fixed.line_length) * u64::from(variable.yres) * 2 <= u64::from(fixed.smem_len)
    );
    Ok((file, fixed, variable))
}

fn next_color(path: &Path) -> Result<(slint::Color, [u8; 4]), Box<dyn std::error::Error>> {
    let (file, fixed, variable) = front_page(path)?;
    let mut previous = [0; 4];
    file.read_exact_at(
        &mut previous,
        u64::from(variable.yoffset) * u64::from(fixed.line_length),
    )?;
    eprintln!("startup: previous_bgra={previous:?}");
    // Require a different color, so stale pixels cannot satisfy the assertion.
    Ok(if previous == [0, 255, 0, 128] {
        (slint::Color::from_rgb_u8(0, 0, 255), [255, 0, 0, 128])
    } else {
        (slint::Color::from_rgb_u8(0, 255, 0), [0, 255, 0, 128])
    })
}

fn check_front_pixel_data(
    path: &Path,
    expected: [u8; 4],
) -> Result<(), Box<dyn std::error::Error>> {
    // Presentation is drained. Readback checks output, not hardware retirement.
    let (file, fixed, variable) = front_page(path)?;
    let mut row = vec![0; variable.xres as usize * 4];
    let mut bad = 0;
    for y in 0..variable.yres {
        file.read_exact_at(
            &mut row,
            (u64::from(variable.yoffset) + u64::from(y)) * u64::from(fixed.line_length),
        )?;
        bad += row
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| **pixel != expected)
            .count();
    }
    let y = variable.yoffset;
    let count = u64::from(variable.xres) * u64::from(variable.yres);
    eprintln!("startup: yoffset={y} expected_bgra={expected:?} pixels={count} bad_pixels={bad}");
    assert_eq!(bad, 0, "front page does not contain the requested frame");
    assert_eq!(front_page(path)?.2.yoffset, y);
    Ok(())
}

#[test]
fn framebuffer_query_uses_native_linux_abi() {
    assert_eq!(size_of::<VariableInfo>(), 160);
    assert_eq!(
        size_of::<FixedInfo>(),
        if size_of::<usize>() == 8 { 80 } else { 68 }
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.len() >= 3,
        "usage: startup-regression show-error|retry|queued|timer|queued-retry OUTPUT_DIRECTORY [--framebuffer=PATH] [--touch=PATH]"
    );
    let mode = args[1].as_str();
    assert!(matches!(
        mode,
        "show-error" | "retry" | "queued" | "timer" | "queued-retry"
    ));
    let output = Path::new(&args[2]);
    assert!(output.is_dir());
    let config = slint_bridge_test_app::config_from_args();
    let framebuffer = config.framebuffer.clone();
    slint_bridge::init(config)?;
    let (_, fixed, variable) = front_page(&framebuffer)?;
    eprintln!(
        "startup: framebuffer width={} height={} stride={}",
        variable.xres, variable.yres, fixed.line_length
    );
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
            let (color, bgra) = next_color(&framebuffer)?;
            ui.set_paint(color);
            quit_later();
            ui.run()?;
            assert!(!ui.window().is_visible());
            assert!(invoked.load(Ordering::SeqCst), "startup task was discarded");
            check_front_pixel_data(&framebuffer, bgra)?;
        }
        eprintln!("startup: same_component_retry=pass queued_task=true");
    } else {
        let root = Arc::new(Mutex::new(None));
        let saved = root.clone();
        let retry = mode == "queued-retry";
        let (color, bgra) = next_color(&framebuffer)?;
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
        check_front_pixel_data(&framebuffer, bgra)?;
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
