//! NX5 init failures and UI-thread teardown while the process remains alive.
use slint::ComponentHandle;
use slint_bridge::{Error, InputDevice};
use std::{fs, path::Path, time::Duration};

fn check_idle(output: &Path, stage: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let vo = fs::read_to_string("/proc/umap/vo")?;
    let devices = vo
        .split_once("-----DEV CONFIG")
        .ok_or("missing VO device table")?
        .1
        .split_once("-----VIDEO LAYER STATUS")
        .ok_or("missing VO layer table")?
        .0;
    assert!(
        !devices.lines().any(|line| {
            let mut columns = line.split_whitespace();
            columns.next().is_some_and(|id| id.parse::<u32>().is_ok())
                && columns.next() == Some("Y")
        }),
        "VO is still enabled after {stage}"
    );
    let mmz = fs::read_to_string("/proc/media-mem")?;
    assert!(
        !mmz.contains("bridge_tde_"),
        "TDE allocation survived {stage}"
    );
    fs::write(output.join(format!("{stage}.vo")), vo)?;
    fs::write(output.join(format!("{stage}.mmz")), mmz)?;
    let mut devices = Vec::new();
    for entry in fs::read_dir("/proc/self/fd")? {
        if let Ok(target) = fs::read_link(entry?.path())
            && target.starts_with("/dev")
        {
            devices.push(target.display().to_string());
        }
    }
    devices.sort();
    fs::write(output.join(format!("{stage}.fds")), devices.join("\n"))?;
    eprintln!(
        "init-retry: stage={stage} vo_disabled=true tde_allocations=0 device_fds={}",
        devices.len()
    );
    Ok(devices)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("usage: init-retry OUTPUT_DIRECTORY [--framebuffer=PATH] [--touch=PATH]")?;
    let output = std::path::PathBuf::from(output);
    assert!(output.is_dir());
    let config = slint_bridge_test_app::config_from_args();
    let mut previous = None;
    for stage in [
        "missing-framebuffer-1",
        "missing-framebuffer-2",
        "missing-input-1",
        "missing-input-2",
    ] {
        let missing = output.join(stage);
        assert!(!missing.exists());
        let mut invalid = config.clone();
        let operation = if stage.starts_with("missing-input") {
            invalid.inputs = vec![InputDevice::touchscreen(missing)];
            "open evdev input"
        } else {
            invalid.framebuffer = missing;
            "open framebuffer"
        };
        let result = slint_bridge::init(invalid);
        assert!(
            matches!(result, Err(Error::Io { operation: actual, .. }) if actual == operation),
            "unexpected initialization result: {result:?}"
        );
        let devices = check_idle(&output, stage)?;
        if stage.ends_with("-2") {
            assert_eq!(
                Some(&devices),
                previous.as_ref(),
                "SDK descriptor cache grew"
            );
        }
        previous = Some(devices);
    }
    std::thread::spawn(move || -> Result<(), String> {
        slint_bridge::init(config).map_err(|e| e.to_string())?;
        let window = slint_bridge_test_app::BasicWindow::new().map_err(|e| e.to_string())?;
        slint::Timer::single_shot(Duration::from_millis(700), || {
            slint::quit_event_loop().unwrap();
        });
        window.run().map_err(|e| e.to_string())
    })
    .join()
    .map_err(|_| "UI thread panicked")??;
    let devices = check_idle(&output, "ui-thread-exited")?;
    assert_eq!(
        Some(devices),
        previous,
        "UI teardown leaked device descriptors"
    );
    eprintln!("init-retry: same_process_retry=pass ui_thread_teardown=pass");
    Ok(())
}
