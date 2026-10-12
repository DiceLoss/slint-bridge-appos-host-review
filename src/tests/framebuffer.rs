use super::*;

#[test]
fn transparent_ui_preserves_alpha_and_clears_moving_content_on_both_pages() {
    let _serial = SLINT_TEST.lock().unwrap();
    slint::slint! {
        export component Overlay inherits Window {
            preferred-width: 8px; preferred-height: 6px;
            background: transparent;
            in property<length> position;
            in property<color> fill;
            Rectangle { x: root.position; y: 1px; width: 2px; height: 4px; background: root.fill; }
        }
    }
    let mut target = surface(8, 6, 32);
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let ui = Overlay::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(8, 6));
    for (position, alpha) in [
        (0, 128),
        (3, 63),
        (5, 0),
        (2, 1),
        (4, 254),
        (6, 255),
        (0, 0),
    ] {
        ui.set_position(position as f32);
        ui.set_fill(slint::Color::from_argb_u8(alpha, 255, 0, 0));
        assert!(draw(&window, &mut target.fb).unwrap());
        let frames = target.commits.lock().unwrap();
        let frame = frames.last().unwrap();
        for y in 0..6 {
            for x in 0..8 {
                let at = y * target.fb.layout.stride + x * 4;
                let expected =
                    if (1..5).contains(&y) && (position..position + 2).contains(&x) && alpha != 0 {
                        u32::from(alpha) << 24 | 0x00ff0000
                    } else {
                        0
                    };
                assert_eq!(
                    &frame[at..at + 4],
                    &expected.to_ne_bytes(),
                    "x={x} y={y} position={position} alpha={alpha}"
                );
            }
            assert!(
                frame[y * target.fb.layout.stride + 32..(y + 1) * target.fb.layout.stride]
                    .iter()
                    .all(|&byte| byte == 0xa5)
            );
        }
    }
    assert_eq!(target.commits.lock().unwrap().len(), 7);
}

#[test]
fn reject_unsafe_framebuffer_layouts() {
    let (mut f, mut v) = info(8, 6, 32);
    assert!(Layout::new(&f, &v).is_ok());
    v.yres_virtual = v.yres;
    assert!(Layout::new(&f, &v).is_err());
    v.yres_virtual = v.yres * 2;
    f.smem_len -= 1;
    assert!(Layout::new(&f, &v).is_err());
    f.smem_len += 1;
    v.red.offset = u32::MAX;
    assert!(Layout::new(&f, &v).is_err());
    v.red.offset = v.blue.offset;
    assert!(Layout::new(&f, &v).is_err());
    v.red.offset = 16;
    f.line_length = 1;
    assert!(Layout::new(&f, &v).is_err());
}

#[test]
fn linux_framebuffer_abi_matches_native_uapi() {
    assert_eq!(size_of::<VariableInfo>(), 160);
    assert_eq!(
        size_of::<FixedInfo>(),
        if size_of::<usize>() == 8 { 80 } else { 68 }
    );
}

#[test]
fn render_ahead_preserves_both_pages_until_completion_and_repairs_history() {
    let _serial = SLINT_TEST.lock().unwrap();
    let (entered, started) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let mut target = surface_with_completion(8, 6, 32, 0, move |offset| {
        entered.send(offset).unwrap();
        gate.recv_timeout(Duration::from_secs(3))
            .map_err(std::io::Error::other)
    });
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let ui = Pixels::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(8, 6));
    assert!(prepare(&window, &mut target.fb).unwrap());
    assert!(target.fb.present().unwrap());
    assert_eq!(started.recv_timeout(Duration::from_secs(3)).unwrap(), 6);

    // Real Slint rendering finishes while the first driver call is blocked.
    // Neither display page may change, even though the canvas has advanced.
    let mut frozen = vec![0; target.fb.layout.page_bytes * 2];
    target.file.read_exact_at(&mut frozen, 0).unwrap();
    ui.set_left_color(slint::Color::from_rgb_u8(0, 255, 0));
    assert!(prepare(&window, &mut target.fb).unwrap());
    assert!(!target.fb.can_render());
    assert!(!target.fb.ready_to_present());
    assert!(target.commits.lock().unwrap().is_empty());
    let mut after = frozen.clone();
    target.file.read_exact_at(&mut after, 0).unwrap();
    assert_eq!(frozen, after);
    release.send(()).unwrap();
    target.fb.wait_for_present().unwrap();
    assert!(target.fb.ready_to_present());
    assert!(target.fb.present().unwrap());
    assert_eq!(started.recv_timeout(Duration::from_secs(3)).unwrap(), 0);

    // Prepare the third frame before the second page is acknowledged. Its
    // dirty history must include the second frame when page 1 is reused.
    ui.set_right_color(slint::Color::from_rgb_u8(255, 255, 255));
    assert!(prepare(&window, &mut target.fb).unwrap());
    release.send(()).unwrap();
    target.fb.wait_for_present().unwrap();
    assert!(target.fb.present().unwrap());
    assert_eq!(started.recv_timeout(Duration::from_secs(3)).unwrap(), 6);
    release.send(()).unwrap();
    target.fb.wait_for_present().unwrap();
    let commits = target.commits.lock().unwrap();
    assert_eq!(commits.len(), 3);
    let image = commits.last().unwrap();
    for y in 0..6 {
        for x in 0..8 {
            let at = y * target.fb.layout.stride + x * 4;
            let expected = if x < 4 { 0xff00ff00u32 } else { 0xffffffffu32 };
            assert_eq!(&image[at..at + 4], &expected.to_ne_bytes());
        }
    }
}

#[test]
fn failed_pending_submission_wakes_poll_and_quarantines_the_prepared_frame() {
    let _serial = SLINT_TEST.lock().unwrap();
    let (entered, started) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let mut target = surface_with_completion(8, 6, 32, 0, move |_| {
        entered.send(()).unwrap();
        gate.recv_timeout(Duration::from_secs(3)).unwrap();
        Err(std::io::Error::other("injected completion failure"))
    });
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let ui = Pixels::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(8, 6));
    prepare(&window, &mut target.fb).unwrap();
    target.fb.present().unwrap();
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    ui.set_left_color(slint::Color::from_rgb_u8(0, 255, 0));
    assert!(prepare(&window, &mut target.fb).unwrap());
    let mut before = vec![0; target.fb.layout.page_bytes * 2];
    target.file.read_exact_at(&mut before, 0).unwrap();
    release.send(()).unwrap();
    let mut fd = libc::pollfd {
        fd: target.fb.wake().fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(unsafe { libc::poll(&mut fd, 1, 3000) }, 1);
    assert!(matches!(
        target.fb.poll_present(),
        Err(Error::PresentationFailed(_))
    ));
    assert!(matches!(
        target.fb.present(),
        Err(Error::PresentationFailed(_))
    ));
    assert!(matches!(
        prepare(&window, &mut target.fb),
        Err(Error::PresentationFailed(_))
    ));
    let mut after = before.clone();
    target.file.read_exact_at(&mut after, 0).unwrap();
    assert_eq!(before, after);
    assert!(target.commits.lock().unwrap().is_empty());
}

#[test]
fn initial_front_page_one_preserves_pixels_and_damage_history() {
    let _serial = SLINT_TEST.lock().unwrap();
    let offsets = Arc::new(Mutex::new(Vec::new()));
    let submitted = offsets.clone();
    let mut target = surface_with_completion(8, 6, 32, 1, move |offset| {
        submitted.lock().unwrap().push(offset);
        Ok(())
    });
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let ui = Pixels::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(8, 6));
    draw(&window, &mut target.fb).unwrap();
    let mut original_front = vec![0; target.fb.layout.page_bytes];
    target
        .file
        .read_exact_at(&mut original_front, target.fb.layout.page_bytes as u64)
        .unwrap();
    assert!(original_front.iter().all(|&byte| byte == 0xa5));
    ui.set_left_color(slint::Color::from_rgb_u8(0, 255, 0));
    draw(&window, &mut target.fb).unwrap();
    ui.set_right_color(slint::Color::from_rgb_u8(255, 255, 255));
    draw(&window, &mut target.fb).unwrap();
    ui.set_left_color(slint::Color::from_rgb_u8(255, 0, 255));
    draw(&window, &mut target.fb).unwrap();
    assert_eq!(*offsets.lock().unwrap(), [0, 6, 0, 6]);
    let commits = target.commits.lock().unwrap();
    assert_eq!(commits.len(), 4);
    for (image, (left, right)) in commits.iter().zip([
        (0xffff0000u32, 0xff0000ffu32),
        (0xff00ff00, 0xff0000ff),
        (0xff00ff00, 0xffffffff),
        (0xffff00ff, 0xffffffff),
    ]) {
        for y in 0..6 {
            for x in 0..8 {
                let at = y * target.fb.layout.stride + x * 4;
                let expected = if x < 4 { left } else { right };
                assert_eq!(&image[at..at + 4], &expected.to_ne_bytes());
            }
            assert!(
                image[y * target.fb.layout.stride + 32..(y + 1) * target.fb.layout.stride]
                    .iter()
                    .all(|&byte| byte == 0xa5)
            );
        }
    }
}

slint::slint! {
    export component SparseUpdates inherits Window {
        preferred-width: 64px; preferred-height: 64px; background: black;
        in property <color> first: red;
        in property <color> second: blue;
        Rectangle { x: 0px; y: 0px; width: 4px; height: 4px; background: first; }
        Rectangle { x: 60px; y: 60px; width: 4px; height: 4px; background: second; }
    }
}

#[test]
fn sparse_damage_does_not_copy_the_gap_between_current_and_previous_updates() {
    let _serial = SLINT_TEST.lock().unwrap();
    let mut target = surface(64, 64, 32);
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let ui = SparseUpdates::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(64, 64));
    draw(&window, &mut target.fb).unwrap(); // Initialize page 1.
    ui.set_first(slint::Color::from_rgb_u8(0, 255, 0));
    draw(&window, &mut target.fb).unwrap(); // Initialize page 0, retaining the first dirty region.
    // A canary in the inactive page makes unnecessary writes observable. This
    // deliberately instruments an unchanged pixel outside both dirty regions.
    let at = target.fb.layout.page_bytes + 32 * target.fb.layout.stride + 32 * 4;
    let canary = 0xffa55aa5u32.to_ne_bytes();
    target.file.write_all_at(&canary, at as u64).unwrap();
    ui.set_second(slint::Color::from_rgb_u8(255, 255, 255));
    draw(&window, &mut target.fb).unwrap();
    let mut actual = [0; 4];
    target.file.read_exact_at(&mut actual, at as u64).unwrap();
    assert_eq!(
        actual, canary,
        "copied the unchanged gap between disjoint dirty regions"
    );
    let output = target.commits.lock().unwrap();
    let output = output.last().unwrap();
    assert_eq!(
        &output[..4],
        &0xff00ff00u32.to_ne_bytes(),
        "previous-page damage still needs repair"
    );
    let at = 60 * target.fb.layout.stride + 60 * 4;
    assert_eq!(&output[at..at + 4], &0xffffffffu32.to_ne_bytes());
}

#[test]
fn real_slint_renders_rotations_formats_and_preserves_page_history() {
    let _serial = SLINT_TEST.lock().unwrap();
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ] {
        for bits in [16, 32] {
            thread::spawn(move || {
                let mut target = surface(8, 6, bits);
                let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
                slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
                let ui = Pixels::new().unwrap();
                ui.show().unwrap();
                window.set_size(rotation.logical_size(PhysicalSize::new(8, 6)));
                window.draw_if_needed(|renderer| {
                    renderer.set_rendering_rotation(rotation.renderer())
                });
                draw(&window, &mut target.fb).unwrap();
                assert_eq!(
                    (ui.get_actual_width(), ui.get_actual_height()),
                    (
                        rotation.logical_size(PhysicalSize::new(8, 6)).width as f32,
                        rotation.logical_size(PhysicalSize::new(8, 6)).height as f32
                    )
                );
                // Cross-check our inverse input transform against actual Slint pixels.
                let image = target.commits.lock().unwrap().last().unwrap().clone();
                let logical_width = rotation.logical_size(PhysicalSize::new(8, 6)).width as f32;
                for y in 0..6 {
                    for x in 0..8 {
                        let logical = rotation.input(PhysicalSize::new(8, 6), x as f32, y as f32);
                        let red = logical.x < logical_width / 2.0;
                        let offset = y * target.fb.layout.stride + x * (bits as usize / 8);
                        if bits == 16 {
                            assert_eq!(
                                u16::from_ne_bytes(image[offset..offset + 2].try_into().unwrap()),
                                if red { 0xf800 } else { 0x001f },
                                "{rotation:?} at ({x}, {y})"
                            );
                        } else {
                            assert_eq!(
                                &image[offset..offset + 4],
                                &if red { 0xffff0000u32 } else { 0xff0000ffu32 }.to_ne_bytes(),
                                "{rotation:?} at ({x}, {y})"
                            );
                        }
                    }
                    assert!(
                        image[y * target.fb.layout.stride + 8 * (bits as usize / 8)
                            ..(y + 1) * target.fb.layout.stride]
                            .iter()
                            .all(|&b| b == 0xa5)
                    );
                }
                ui.set_left_color(slint::Color::from_rgb_u8(0, 255, 0));
                draw(&window, &mut target.fb).unwrap();
                ui.set_right_color(slint::Color::from_rgb_u8(255, 255, 255));
                draw(&window, &mut target.fb).unwrap();
                let image = target.commits.lock().unwrap().last().unwrap().clone();
                for y in 0..6 {
                    for x in 0..8 {
                        let logical = rotation.input(PhysicalSize::new(8, 6), x as f32, y as f32);
                        let green = logical.x < logical_width / 2.0;
                        let offset = y * target.fb.layout.stride + x * (bits as usize / 8);
                        if bits == 16 {
                            assert_eq!(
                                u16::from_ne_bytes(image[offset..offset + 2].try_into().unwrap()),
                                if green { 0x07e0 } else { 0xffff }
                            );
                        } else {
                            assert_eq!(
                                &image[offset..offset + 4],
                                &if green { 0xff00ff00u32 } else { 0xffffffffu32 }.to_ne_bytes()
                            );
                        }
                    }
                }
                let count = target.commits.lock().unwrap().len();
                draw(&window, &mut target.fb).unwrap();
                assert_eq!(
                    target.commits.lock().unwrap().len(),
                    count,
                    "unchanged scene must not submit an empty frame"
                );
                // Unknown completion must quarantine both pages, even on a redraw.
                target.fail.store(true, Ordering::SeqCst);
                ui.set_left_color(slint::Color::from_rgb_u8(128, 0, 0));
                assert!(matches!(
                    draw(&window, &mut target.fb),
                    Err(Error::PresentationFailed(_))
                ));
                let mut before = vec![0; target.fb.layout.page_bytes * 2];
                target.file.read_exact_at(&mut before, 0).unwrap();
                ui.set_right_color(slint::Color::from_rgb_u8(0, 128, 0));
                assert!(matches!(
                    draw(&window, &mut target.fb),
                    Err(Error::PresentationFailed(_))
                ));
                let mut after = before.clone();
                target.file.read_exact_at(&mut after, 0).unwrap();
                assert_eq!(before, after);
            })
            .join()
            .unwrap();
        }
    }
}

#[test]
fn software_path_renders_font_pixels() {
    let _serial = SLINT_TEST.lock().unwrap();
    let mut target = surface(64, 32, 32);
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let ui = Glyphs::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(64, 32));
    draw(&window, &mut target.fb).unwrap();
    let image = target.commits.lock().unwrap().last().unwrap().clone();
    let lit = (0..32)
        .flat_map(|y| (0..64).map(move |x| (x, y)))
        .filter(|&(x, y)| image[y * target.fb.layout.stride + x * 4] > 0)
        .count();
    assert!(
        lit > 10 && lit < 64 * 32,
        "expected actual glyph pixels against a black background, got {lit}"
    );
}

#[test]
fn framebuffer_uses_the_configured_device_path() {
    let path =
        std::env::temp_dir().join(format!("slint-bridge-framebuffer {}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let result = Framebuffer::open(
        &path,
        PresentationContract {
            validate_layout: Layout::new,
            complete: Box::new(|_, _, _| unreachable!()),
        },
    );
    drop(file);
    std::fs::remove_file(path).unwrap();
    assert!(matches!(
        result,
        Err(Error::Io {
            operation: "read framebuffer layout",
            ..
        })
    ));
}

#[test]
fn framebuffer_lock_conflict_is_display_not_admitted() {
    use std::os::fd::AsRawFd;
    let path = std::env::temp_dir().join(format!("slint-bridge-busy-fb-{}", std::process::id()));
    let owner = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(owner.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let result = Framebuffer::open(
        &path,
        PresentationContract {
            validate_layout: Layout::new,
            complete: Box::new(|_, _, _| unreachable!()),
        },
    );
    std::fs::remove_file(path).unwrap();
    assert!(
        matches!(result, Err(Error::DisplayNotAdmitted { backend, .. }) if backend == selected_backend())
    );
}

#[test]
fn discarded_preparation_carries_damage_to_both_pages() {
    let _serial = SLINT_TEST.lock().unwrap();
    slint::slint! {
        export component Tiles inherits Window {
            preferred-width: 64px; preferred-height: 32px;
            in-out property<color> a: red;
            in-out property<color> b: red;
            in-out property<color> c: red;
            background: black;
            Rectangle { x: 2px; y: 2px; width: 8px; height: 8px; background: root.a; }
            Rectangle { x: 24px; y: 2px; width: 8px; height: 8px; background: root.b; }
            Rectangle { x: 46px; y: 2px; width: 8px; height: 8px; background: root.c; }
        }
    }
    let mut target = surface(64, 32, 32);
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let ui = Tiles::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(64, 32));
    let green = slint::Color::from_rgb_u8(0, 255, 0);
    let blue = slint::Color::from_rgb_u8(0, 0, 255);
    draw(&window, &mut target.fb).unwrap();
    ui.set_c(green);
    draw(&window, &mut target.fb).unwrap(); // Both physical pages are initialized.
    ui.set_a(green);
    assert!(prepare(&window, &mut target.fb).unwrap());
    let mut before = vec![0; target.fb.layout.page_bytes * 2];
    target.file.read_exact_at(&mut before, 0).unwrap();
    assert!(target.fb.discard_prepared());
    let mut after = before.clone();
    target.file.read_exact_at(&mut after, 0).unwrap();
    assert_eq!(before, after, "discard must not touch either physical page");
    ui.set_b(blue);
    draw(&window, &mut target.fb).unwrap();
    ui.set_c(blue);
    draw(&window, &mut target.fb).unwrap(); // Repair the other page too.
    let check = |expected: [[u8; 4]; 3]| {
        let frames = target.commits.lock().unwrap();
        let frame = frames.last().unwrap();
        for (x, value) in [4, 26, 48].into_iter().zip(expected) {
            let at = 4 * target.fb.layout.stride + x * 4;
            assert_eq!(&frame[at..at + 4], value);
        }
    };
    check([[0, 255, 0, 255], [255, 0, 0, 255], [255, 0, 0, 255]]);
    ui.set_a(blue);
    prepare(&window, &mut target.fb).unwrap();
    target.fb.discard_prepared();
    // Slint has no new changes, but the discarded image was never submitted.
    assert!(draw(&window, &mut target.fb).unwrap());
    let frames = target.commits.lock().unwrap();
    let at = 4 * target.fb.layout.stride + 4 * 4;
    assert_eq!(&frames.last().unwrap()[at..at + 4], &[255, 0, 0, 255]);
}
