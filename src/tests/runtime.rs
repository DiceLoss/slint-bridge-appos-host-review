use super::*;

#[cfg(feature = "appos-host")]
#[test]
fn appos_first_frame_gate_returns_only_after_completed_presentation() {
    const NAME: &str =
        "tests::runtime::appos_first_frame_gate_returns_only_after_completed_presentation";
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

    let (completed, completion) = mpsc::sync_channel(1);
    let target = surface_with_completion(8, 6, 32, 0, move |_| {
        completed.send(()).unwrap();
        Ok(())
    });
    let commits = target.commits.clone();
    let platform = crate::runtime::BridgePlatform::with_inputs(
        Config::default(),
        target.fb,
        Vec::new(),
        Vec::new(),
    );
    slint::platform::set_platform(Box::new(platform)).unwrap();
    let ui = LoopScene::new().unwrap();
    ui.show().unwrap();

    crate::runtime::run_until_first_presented_frame().unwrap();

    completion.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(commits.lock().unwrap().len(), 1);
    assert!(ui.window().is_visible());
    ui.hide().unwrap();
}

#[cfg(feature = "appos-host")]
#[test]
fn appos_host_defers_font_registration_until_the_window_has_a_slint_context() {
    const NAME: &str =
        "tests::runtime::appos_host_defers_font_registration_until_the_window_has_a_slint_context";
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

    let target = surface_with_completion(8, 6, 32, 0, |_| Ok(()));
    let font = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("third-party/fonts/DejaVuSans.ttf");
    let platform = crate::runtime::BridgePlatform::with_host_inputs(
        Config::default(),
        target.fb,
        Vec::new(),
        Vec::new(),
        vec![font],
        None,
    );
    slint::platform::set_platform(Box::new(platform)).unwrap();
    let ui = LoopScene::new().unwrap();
    ui.show().unwrap();

    crate::runtime::run_until_first_presented_frame().unwrap();

    assert!(ui.window().is_visible());
    ui.hide().unwrap();
}

#[cfg(feature = "appos-host")]
#[test]
fn appos_host_reuses_one_adapter_across_sequential_activity_roots() {
    const NAME: &str =
        "tests::runtime::appos_host_reuses_one_adapter_across_sequential_activity_roots";
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

    let target = surface_with_completion(8, 6, 32, 0, |_| Ok(()));
    let platform = crate::runtime::BridgePlatform::with_host_inputs(
        Config::default(),
        target.fb,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
    );
    slint::platform::set_platform(Box::new(platform)).unwrap();

    let outgoing = LoopScene::new().unwrap();
    let outgoing_weak = outgoing.as_weak();
    outgoing.show().unwrap();
    outgoing.hide().unwrap();
    // Keep the outgoing component alive while constructing the incoming one;
    // this is the Activity transition ordering used by AppOS.
    let incoming = LoopScene::new().unwrap();
    incoming.show().unwrap();

    crate::runtime::run_until_first_presented_frame().unwrap();

    assert!(incoming.window().is_visible());
    drop(outgoing);
    assert!(
        outgoing_weak.upgrade().is_none(),
        "the reused adapter retained the hidden outgoing Activity root"
    );
    incoming.hide().unwrap();
}

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
fn queued_callbacks_yield_to_touch_before_finishing_the_backlog() {
    const NAME: &str =
        "tests::runtime::queued_callbacks_yield_to_touch_before_finishing_the_backlog";
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
    let (frame_ready, rendered) = mpsc::sync_channel(1);
    let target = surface_with_completion(8, 6, 32, 0, move |_| {
        let _ = frame_ready.try_send(());
        Ok(())
    });
    let (reader, writer) = input_pipe();
    let platform = crate::runtime::BridgePlatform::with_inputs(
        Config::default(),
        target.fb,
        vec![Evdev {
            file: reader,
            decoder: super::input::touch_decoder(Rotation::None),
        }],
        Vec::new(),
    );
    slint::platform::set_platform(Box::new(platform)).unwrap();
    let ui = LoopScene::new().unwrap();
    ui.invoke_focus_keys();
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = Arc::new(Mutex::new(None));
    let started = Arc::new(Mutex::new(None::<std::time::Instant>));
    let count = completed.clone();
    let result = observed.clone();
    let start = started.clone();
    ui.on_tapped(move || {
        *result.lock().unwrap() = Some((
            count.load(Ordering::SeqCst),
            start.lock().unwrap().unwrap().elapsed(),
        ));
    });
    let mut input_writer = Some(writer.try_clone().unwrap());
    let task_count = completed.clone();
    let producer = thread::spawn(move || {
        // Input needs the real window's first layout, just like a displayed UI.
        rendered.recv_timeout(Duration::from_secs(3)).unwrap();
        let (release, gate) = mpsc::sync_channel(1);
        let (entered, staging) = mpsc::sync_channel(1);
        slint::invoke_from_event_loop(move || {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(3)).unwrap();
        })
        .unwrap();
        staging.recv_timeout(Duration::from_secs(3)).unwrap();
        for index in 0..64 {
            let mut writer = input_writer.take();
            let count = task_count.clone();
            let start = started.clone();
            slint::invoke_from_event_loop(move || {
                assert_eq!(
                    count.fetch_add(1, Ordering::SeqCst),
                    index,
                    "task FIFO changed"
                );
                if let Some(writer) = &mut writer {
                    *start.lock().unwrap() = Some(std::time::Instant::now());
                    send_input(
                        writer,
                        &[
                            (3, 0, 400),
                            (3, 1, 500),
                            (1, 330, 1),
                            (0, 0, 0),
                            (1, 330, 0),
                            (0, 0, 0),
                        ],
                    );
                }
                // Model small UI updates on a slow CPU, not one uninterruptible task.
                thread::sleep(Duration::from_millis(2));
                if index == 63 {
                    slint::Timer::single_shot(Duration::from_millis(10), || {
                        slint::quit_event_loop().unwrap()
                    });
                }
            })
            .unwrap();
        }
        release.send(()).unwrap();
    });
    ui.run().unwrap();
    producer.join().unwrap();
    let (callbacks, elapsed) = observed
        .lock()
        .unwrap()
        .expect("touch callback was not dispatched");
    eprintln!("touch handled after {callbacks} callbacks, elapsed={elapsed:?}");
    assert_eq!(
        ui.get_taps(),
        1,
        "pending touch must reach the Slint component"
    );
    assert!(
        callbacks <= 8,
        "touch waited for {callbacks} queued callbacks ({elapsed:?})"
    );
    assert_eq!(
        completed.load(Ordering::SeqCst),
        64,
        "yielding lost queued work"
    );
    drop(writer);
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
    );
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
    );
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
    );
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

#[test]
fn failed_show_can_retry_without_losing_tasks_or_explicit_quit() {
    const NAME: &str =
        "tests::runtime::failed_show_can_retry_without_losing_tasks_or_explicit_quit";
    if std::env::var("SLINT_BRIDGE_TEST_CHILD").as_deref() != Ok(NAME) {
        for mode in ["retry", "explicit-quit"] {
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture"])
                .env("SLINT_BRIDGE_TEST_CHILD", NAME)
                .env("SLINT_BRIDGE_TEST_STARTUP", mode)
                .output()
                .unwrap();
            assert!(
                run.status.success(),
                "{mode}: {}{}",
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            );
        }
        return;
    }
    let explicit_quit =
        std::env::var("SLINT_BRIDGE_TEST_STARTUP").as_deref() == Ok("explicit-quit");
    let (presented, frame_ready) = mpsc::sync_channel(1);
    let target = surface_with_completion(8, 6, 32, 0, move |_| {
        let _ = presented.try_send(());
        Ok(())
    });
    let commits = target.commits.clone();
    let platform = crate::runtime::BridgePlatform::with_inputs(
        Config::default(),
        target.fb,
        Vec::new(),
        Vec::new(),
    );
    slint::platform::set_platform(Box::new(platform)).unwrap();
    let called = Arc::new(AtomicBool::new(false));
    let observed = called.clone();
    slint::invoke_from_event_loop(move || observed.store(true, Ordering::SeqCst)).unwrap();
    if explicit_quit {
        slint::quit_event_loop().unwrap();
    }
    let ui = LoopScene::new().unwrap();
    ui.set_required_width(200.0);
    for _ in 0..2 {
        let error = ui.run().unwrap_err().to_string();
        assert!(error.contains("window layout constraints"), "{error}");
        assert!(
            !ui.window().is_visible(),
            "rejected show left a visible component reference"
        );
    }
    ui.set_required_width(0.0);
    ui.set_paint(slint::Color::from_rgb_u8(0, 255, 0));
    if explicit_quit {
        ui.run().unwrap();
        assert!(
            !called.load(Ordering::SeqCst),
            "rollback lost the explicit quit"
        );
        assert!(commits.lock().unwrap().is_empty());
    } else {
        let control = thread::spawn(move || {
            let rendered = frame_ready.recv_timeout(Duration::from_secs(3)).is_ok();
            let _ = slint::quit_event_loop();
            rendered
        });
        ui.run().unwrap();
        assert!(
            control.join().unwrap(),
            "corrected component did not render"
        );
        assert!(
            called.load(Ordering::SeqCst),
            "rollback discarded queued work"
        );
        let frames = commits.lock().unwrap();
        assert_eq!(&frames.last().unwrap()[..4], &[0, 255, 0, 255]);
    }
    assert!(!ui.window().is_visible());
}

#[test]
fn event_loop_accepts_deferred_window_creation() {
    const NAME: &str = "tests::runtime::event_loop_accepts_deferred_window_creation";
    if std::env::var("SLINT_BRIDGE_TEST_CHILD").as_deref() != Ok(NAME) {
        for mode in ["queued", "timer", "queued-retry"] {
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture"])
                .env("SLINT_BRIDGE_TEST_CHILD", NAME)
                .env("SLINT_BRIDGE_TEST_STARTUP", mode)
                .output()
                .unwrap();
            assert!(
                run.status.success(),
                "{mode}: {}{}",
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            );
        }
        return;
    }
    let (presented, frame_ready) = mpsc::sync_channel(1);
    let target = surface_with_completion(8, 6, 32, 0, move |_| {
        let _ = presented.try_send(());
        Ok(())
    });
    let commits = target.commits.clone();
    let platform = crate::runtime::BridgePlatform::with_inputs(
        Config::default(),
        target.fb,
        Vec::new(),
        Vec::new(),
    );
    slint::platform::set_platform(Box::new(platform)).unwrap();
    let root = Arc::new(Mutex::new(None));
    let created = root.clone();
    let mode = std::env::var("SLINT_BRIDGE_TEST_STARTUP").unwrap();
    let retry = mode == "queued-retry";
    let create = move || {
        let ui = LoopScene::new().unwrap();
        if retry {
            ui.set_required_width(200.0);
            assert!(ui.show().is_err());
            assert!(!ui.window().is_visible());
            ui.set_required_width(0.0);
        }
        ui.set_paint(slint::Color::from_rgb_u8(0, 255, 0));
        ui.show().unwrap();
        *created.lock().unwrap() = Some(ui.as_weak());
    };
    if mode != "timer" {
        // Put root creation beyond one iteration's task budget. A windowless
        // loop must keep consuming queued work instead of sleeping forever.
        for _ in 0..128 {
            slint::invoke_from_event_loop(|| {}).unwrap();
        }
        slint::invoke_from_event_loop(create).unwrap();
    } else {
        slint::Timer::single_shot(Duration::from_millis(10), create);
    }
    let control = thread::spawn(move || {
        let rendered = frame_ready.recv_timeout(Duration::from_secs(3)).is_ok();
        let _ = slint::quit_event_loop();
        rendered
    });
    let result = slint::run_event_loop();
    let rendered = control.join().unwrap();
    result.unwrap();
    assert!(rendered, "deferred component did not render");
    let ui = root.lock().unwrap().take().unwrap().upgrade().unwrap();
    ui.hide().unwrap();
    assert_eq!(
        &commits.lock().unwrap().last().unwrap()[..4],
        &[0, 255, 0, 255]
    );
}

#[test]
fn rerun_redraws_current_state_before_presenting() {
    const NAME: &str = "tests::runtime::rerun_redraws_current_state_before_presenting";
    if std::env::var("SLINT_BRIDGE_TEST_CHILD").as_deref() != Ok(NAME) {
        for outcome in ["quit", "layout-error"] {
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture"])
                .env("SLINT_BRIDGE_TEST_CHILD", NAME)
                .env("SLINT_BRIDGE_TEST_EXIT", outcome)
                .output()
                .unwrap();
            assert!(
                run.status.success(),
                "{outcome}: {}{}",
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            );
        }
        return;
    }
    let layout_error = std::env::var("SLINT_BRIDGE_TEST_EXIT").as_deref() == Ok("layout-error");
    let (entered, started) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let mut first = true;
    let target = surface_with_completion(8, 6, 32, 0, move |_| {
        if first {
            first = false;
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        Ok(())
    });
    let commits = target.commits.clone();
    let platform = crate::runtime::BridgePlatform::with_inputs(
        Config {
            frame_interval: Duration::from_millis(1),
            ..Config::default()
        },
        target.fb,
        Vec::new(),
        Vec::new(),
    );
    let wake = platform.wake.clone();
    slint::platform::set_platform(Box::new(platform)).unwrap();
    let ui = LoopScene::new().unwrap();
    let weak = ui.as_weak();
    let control = thread::spawn(move || {
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        Proxy(wake)
            .invoke_from_event_loop(Box::new(move || {
                weak.upgrade()
                    .unwrap()
                    .set_paint(slint::Color::from_rgb_u8(0, 255, 0));
                slint::Timer::single_shot(Duration::from_millis(30), move || {
                    if layout_error {
                        weak.upgrade().unwrap().set_required_width(200.0);
                    } else {
                        slint::quit_event_loop().unwrap();
                    }
                    release.send(()).unwrap();
                });
            }))
            .unwrap();
    });
    let result = ui.run();
    if layout_error {
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("window layout constraints")
        );
        ui.set_required_width(0.0);
    } else {
        result.unwrap();
    }
    control.join().unwrap();
    assert_eq!(commits.lock().unwrap().len(), 1);
    ui.set_paint(slint::Color::from_rgb_u8(0, 0, 255));
    slint::Timer::single_shot(Duration::from_millis(30), || {
        slint::quit_event_loop().unwrap()
    });
    ui.run().unwrap();
    let frames = commits.lock().unwrap();
    eprintln!(
        "presented BGRA colors: {:?}",
        frames.iter().map(|f| &f[..4]).collect::<Vec<_>>()
    );
    assert!(frames.len() >= 2);
    assert_eq!(
        &frames[1][..4],
        &[255, 0, 0, 255],
        "first frame after rerun must use current blue state, not the old green preparation"
    );
}
