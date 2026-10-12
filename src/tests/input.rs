use super::*;

fn touch_decoder(rotation: Rotation) -> Decoder {
    Decoder::new(
        PhysicalSize::new(8, 6),
        rotation,
        Some((
            Axis {
                min: -100,
                max: 900,
                value: -100,
            },
            Axis {
                min: 0,
                max: 1000,
                value: 0,
            },
        )),
        &[],
    )
    .unwrap()
}

#[test]
fn touch_waits_for_complete_packet_and_rotates_coordinates() {
    let mut decoder = touch_decoder(Rotation::Clockwise90);
    let mut events = Vec::new();
    decoder.event(1, 330, 1, &mut events); // Linux sends contact before coordinates.
    decoder.event(3, 0, 900, &mut events);
    decoder.event(3, 1, 1000, &mut events);
    assert!(events.is_empty());
    decoder.event(0, 0, 0, &mut events);
    assert!(
        matches!(events[0], WindowEvent::PointerPressed { position, .. } if position.x == 5.0 && position.y == 0.0)
    );
    events.clear();
    decoder.event(1, 330, 0, &mut events);
    decoder.event(0, 0, 0, &mut events);
    assert!(matches!(events[0], WindowEvent::PointerReleased { .. }));
    assert!(matches!(events[1], WindowEvent::PointerExited));
}

#[test]
fn dropped_input_resynchronizes_releases_without_replaying_stale_packets() {
    let binding = [KeyBinding::new(28, slint::platform::Key::Return)];
    let mut decoder = Decoder::new(
        PhysicalSize::new(8, 6),
        Rotation::None,
        Some((
            Axis {
                min: 0,
                max: 10,
                value: 3,
            },
            Axis {
                min: 0,
                max: 10,
                value: 4,
            },
        )),
        &binding,
    )
    .unwrap();
    let mut events = Vec::new();
    decoder.event(1, 330, 1, &mut events);
    decoder.event(1, 28, 1, &mut events);
    decoder.event(0, 0, 0, &mut events);
    events.clear();
    decoder.event(0, 3, 0, &mut events);
    decoder.event(1, 28, 2, &mut events);
    decoder.event(3, 0, 99, &mut events);
    assert!(events.is_empty());
    assert!(decoder.event(0, 0, 0, &mut events));
    decoder.resync(
        Some((
            Axis {
                min: 0,
                max: 10,
                value: 3,
            },
            Axis {
                min: 0,
                max: 10,
                value: 4,
            },
        )),
        &[0; 96],
        &mut events,
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WindowEvent::PointerReleased { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WindowEvent::KeyReleased { .. }))
    );
}

#[test]
fn mapped_buttons_keep_press_repeat_release_order() {
    let mut decoder = Decoder::new(
        PhysicalSize::new(8, 6),
        Rotation::None,
        None,
        &[KeyBinding::new(28, "\n")],
    )
    .unwrap();
    let mut events = Vec::new();
    for value in [1, 1, 2, 0, 0] {
        decoder.event(1, 28, value, &mut events);
    }
    assert_eq!(events.len(), 3);
    assert!(matches!(events[0], WindowEvent::KeyPressed { .. }));
    assert!(matches!(events[1], WindowEvent::KeyPressRepeated { .. }));
    assert!(matches!(events[2], WindowEvent::KeyReleased { .. }));
}

#[test]
fn evdev_disconnect_is_an_error_instead_of_an_idle_spin() {
    let (reader, writer) = input_pipe();
    drop(writer);
    let mut input = Evdev {
        file: reader,
        decoder: Decoder::new(PhysicalSize::new(8, 6), Rotation::None, None, &[]).unwrap(),
    };
    assert!(
        matches!(input.read_ready(&mut Vec::new()), Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn touch_uses_the_configured_device_path() {
    let path = std::env::temp_dir().join(format!("slint-bridge-touch {}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let result = Evdev::open(
        &InputDevice::touchscreen(&path),
        PhysicalSize::new(800, 480),
        Rotation::None,
    );
    drop(file);
    std::fs::remove_file(path).unwrap();
    // A normal file opens successfully, then rejects the evdev axis ioctl.
    // Opening a hard-coded event device would fail at the earlier open step.
    assert!(matches!(
        result,
        Err(Error::Io {
            operation: "query evdev absolute axis",
            ..
        })
    ));
}

#[test]
fn touch_scales_device_axes_to_the_actual_panel_size() {
    for panel in [
        PhysicalSize::new(800, 480),
        PhysicalSize::new(1280, 720),
        PhysicalSize::new(480, 800),
    ] {
        let mut decoder = Decoder::new(
            panel,
            Rotation::None,
            Some((
                Axis {
                    min: -100,
                    max: 1900,
                    value: 1900,
                },
                Axis {
                    min: 200,
                    max: 4200,
                    value: 4200,
                },
            )),
            &[],
        )
        .unwrap();
        let mut events = Vec::new();
        decoder.event(1, 330, 1, &mut events);
        decoder.event(0, 0, 0, &mut events);
        assert!(
            matches!(events[0], WindowEvent::PointerPressed { position, .. }
            if position.x == (panel.width - 1) as f32 && position.y == (panel.height - 1) as f32)
        );
    }
}
