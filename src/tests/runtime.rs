use super::*;

fn send_input(writer: &mut File, events: &[(u16, u16, i32)]) {
    let records: Vec<libc::input_event> = events
        .iter()
        .map(|&(kind, code, value)| {
            let mut event: libc::input_event = unsafe { std::mem::zeroed() };
            event.type_ = kind;
            event.code = code;
            event.value = value;
            event
        })
        .collect();
    let bytes = unsafe {
        std::slice::from_raw_parts(records.as_ptr().cast(), size_of_val(records.as_slice()))
    };
    writer.write_all(bytes).unwrap();
}

#[test]
fn event_loop_runs_callbacks_and_timers_during_flip_and_reports_idle_failure() {
    // Slint installs its process-global proxy once; isolate this second real
    // platform from the existing event-loop integration test.
    const NAME: &str =
        "tests::runtime::event_loop_runs_callbacks_and_timers_during_flip_and_reports_idle_failure";
    if std::env::var("SLINT_BRIDGE_TEST_CHILD").as_deref() != Ok(NAME) {
        let run = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("SLINT_BRIDGE_TEST_CHILD", NAME)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        return;
    }
    let _serial = SLINT_TEST.lock().unwrap();
    let (entered, started) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let target = surface_with_completion(8, 6, 32, 0, move |_| {
        entered.send(()).unwrap();
        gate.recv_timeout(Duration::from_secs(3)).unwrap();
        Err(std::io::Error::other("injected idle presentation failure"))
    });
    let platform = crate::runtime::BridgePlatform::with_inputs(
        Config {
            frame_interval: Duration::from_millis(2),
            ..Config::default()
        },
        target.fb,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let wake = platform.wake.clone();
    slint::platform::set_platform(Box::new(platform)).unwrap();
    let ui = LoopScene::new().unwrap();
    let weak = ui.as_weak();
    let fallback = Arc::new(AtomicBool::new(false));
    let watchdog = fallback.clone();
    let (finished, done) = mpsc::sync_channel(1);
    let control = thread::spawn(move || {
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        let (timed, timer_ran) = mpsc::sync_channel(1);
        Proxy(wake.clone())
            .invoke_from_event_loop(Box::new(move || {
                weak.upgrade()
                    .unwrap()
                    .set_paint(slint::Color::from_rgb_u8(0, 255, 0));
                slint::Timer::single_shot(Duration::from_millis(5), move || {
                    timed.send(()).unwrap();
                });
            }))
            .unwrap();
        // The blocked display call must not prevent this UI timer from firing.
        timer_ran.recv_timeout(Duration::from_secs(2)).unwrap();
        release.send(()).unwrap();
        // No more timers or input: the worker's completion signal must wake UI.
        if done.recv_timeout(Duration::from_secs(2)).is_err() {
            watchdog.store(true, Ordering::SeqCst);
            let _ = Proxy(wake).quit_event_loop();
        }
    });
    let error = ui.run().unwrap_err();
    let _ = finished.send(());
    control.join().unwrap();
    assert!(
        !fallback.load(Ordering::SeqCst),
        "completion did not wake the idle loop"
    );
    assert!(
        error
            .to_string()
            .contains("injected idle presentation failure")
    );
}

#[test]
fn event_loop_preserves_ui_and_pending_presentation_errors() {
    const NAME: &str = "tests::runtime::event_loop_preserves_ui_and_pending_presentation_errors";
    if std::env::var("SLINT_BRIDGE_TEST_CHILD").as_deref() != Ok(NAME) {
        for outcome in ["ok", "error"] {
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture"])
                .env("SLINT_BRIDGE_TEST_CHILD", NAME)
                .env("SLINT_BRIDGE_TEST_COMPLETION", outcome)
                .output()
                .unwrap();
            assert!(
                run.status.success(),
                "completion={outcome}: {}{}",
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            );
        }
        return;
    }
    let fail = std::env::var("SLINT_BRIDGE_TEST_COMPLETION").as_deref() == Ok("error");
    let (entered, started) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let target = surface_with_completion(8, 6, 32, 0, move |_| {
        entered.send(()).unwrap();
        gate.recv_timeout(Duration::from_secs(3)).unwrap();
        if fail {
            Err(std::io::Error::other("injected final presentation failure"))
        } else {
            Ok(())
        }
    });
    let commits = target.commits.clone();
    let platform = crate::runtime::BridgePlatform::with_inputs(
        Config::default(),
        target.fb,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let wake = platform.wake.clone();
    slint::platform::set_platform(Box::new(platform)).unwrap();
    let ui = LoopScene::new().unwrap();
    let weak = ui.as_weak();
    let control = thread::spawn(move || {
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        Proxy(wake)
            .invoke_from_event_loop(Box::new(move || {
                // The loop has already polled completion before this callback.
                // Its layout error must still drain the blocked submission.
                weak.upgrade().unwrap().set_required_width(200.0);
                release.send(()).unwrap();
            }))
            .unwrap();
    });
    let error = ui.run().unwrap_err().to_string();
    control.join().unwrap();
    assert!(error.contains("window layout constraints"), "{error}");
    if fail {
        assert!(
            error.contains("injected final presentation failure"),
            "{error}"
        );
        assert!(error.contains("page ownership is unknown"), "{error}");
        assert!(commits.lock().unwrap().is_empty());
        // Clearing the UI error cannot make the failed display writable again.
        ui.set_required_width(0.0);
        let again = ui.run().unwrap_err().to_string();
        assert_eq!(
            again.matches("injected final presentation failure").count(),
            1
        );
        assert!(commits.lock().unwrap().is_empty());
    } else {
        assert!(!error.contains("presentation failed"), "{error}");
        assert_eq!(commits.lock().unwrap().len(), 1);
    }
}

#[test]
fn real_event_loop_handles_evdev_timers_cross_thread_updates_and_quit() {
    let _serial = SLINT_TEST.lock().unwrap();
    let target = surface(8, 6, 32);
    let commits = target.commits.clone();
    let (reader, writer) = input_pipe();
    let decoder = Decoder::new(
        PhysicalSize::new(8, 6),
        Rotation::Clockwise90,
        Some((
            Axis {
                min: 0,
                max: 1000,
                value: 0,
            },
            Axis {
                min: 0,
                max: 1000,
                value: 0,
            },
        )),
        &[KeyBinding::new(28, "\n")],
    )
    .unwrap();
    let platform = crate::runtime::BridgePlatform::with_inputs(
        Config {
            rotation: Rotation::Clockwise90,
            frame_interval: Duration::from_millis(2),
            ..Config::default()
        },
        target.fb,
        vec![Evdev {
            file: reader,
            decoder,
        }],
        Vec::new(),
    )
    .unwrap();
    let queue = platform.wake.clone();
    slint::platform::set_platform(Box::new(platform)).unwrap();
    let ui = LoopScene::new().unwrap();
    ui.invoke_focus_keys();
    let weak = ui.as_weak();
    let worker_queue = queue.clone();
    let mut worker_writer = writer.try_clone().unwrap();
    let (finish_tx, finish_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20)); // Wake an otherwise idle event loop.
        send_input(
            &mut worker_writer,
            &[
                (1, 330, 1),
                (3, 0, 800),
                (3, 1, 200),
                (0, 0, 0),
                (1, 330, 0),
                (0, 0, 0),
                (1, 28, 1),
                (1, 28, 0),
                (0, 0, 0),
            ],
        );
        slint::invoke_from_event_loop(move || {
            let ui = weak.upgrade().unwrap();
            ui.set_paint(slint::Color::from_rgb_u8(0, 255, 0));
            slint::Timer::single_shot(Duration::from_millis(30), || {
                slint::quit_event_loop().unwrap()
            });
        })
        .unwrap();
        if finish_rx.recv_timeout(Duration::from_secs(3)).is_err() {
            let _ = Proxy(worker_queue).quit_event_loop();
        }
    });
    ui.run().unwrap();
    finish_tx.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(ui.get_taps(), 1);
    assert_eq!(ui.get_keys(), 1);
    assert!(commits.lock().unwrap().len() >= 2);
    assert!(
        commits.lock().unwrap().len() <= 3,
        "static UI should not render continuously while idle"
    );
    assert!(
        Proxy(queue)
            .invoke_from_event_loop(Box::new(|| {}))
            .is_err()
    );
    let before_animation = commits.lock().unwrap().len();
    ui.set_overlay_opacity(1.0);
    slint::Timer::single_shot(Duration::from_millis(160), || {
        slint::quit_event_loop().unwrap()
    });
    ui.run().unwrap();
    let animation_frames = commits.lock().unwrap().len() - before_animation;
    assert!(
        animation_frames > 2 && animation_frames <= 55,
        "animation must advance and stop redrawing: {animation_frames}"
    );
    ui.set_required_width(200.0);
    assert!(
        ui.show().is_err(),
        "fixed-size display must reject incompatible UI constraints instead of panicking during render"
    );
    drop(writer);
}
