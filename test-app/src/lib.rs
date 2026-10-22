use slint_bridge::{BackgroundStorage, Config, InputDevice, Rotation};

slint::include_modules!();

pub mod background;
pub mod background_demo;
pub mod showcase;

#[path = "../fixtures/textures.rs"]
mod textures;
pub use textures::configure_textures;

/// Device paths are chosen by the caller, independently of the board feature.
pub fn config_from_args() -> Config {
    config_from_arguments(std::env::args().skip(1))
}

fn config_from_arguments(arguments: impl IntoIterator<Item = String>) -> Config {
    let mut config = Config::default();
    for argument in arguments {
        if argument == "--mma-background" {
            config.background_storage = BackgroundStorage::Mma;
        } else if let Some(value) = argument.strip_prefix("--background=") {
            config.background_storage = match value {
                "heap" => BackgroundStorage::Heap,
                "mma" => BackgroundStorage::Mma,
                _ => panic!("--background must be heap or mma"),
            };
        } else if let Some(path) = argument.strip_prefix("--framebuffer=") {
            config.framebuffer = path.into();
        } else if let Some(path) = argument.strip_prefix("--touch=") {
            config.inputs.push(if path == "auto" {
                InputDevice::auto_touchscreen()
            } else {
                InputDevice::touchscreen(path)
            });
        } else if let Some(value) = argument.strip_prefix("--rotation=") {
            config.rotation = match value {
                "0" => Rotation::None,
                "90" => Rotation::Clockwise90,
                "180" => Rotation::Clockwise180,
                "270" => Rotation::Clockwise270,
                _ => panic!("--rotation must be 0, 90, 180, or 270"),
            };
        }
    }
    config
}

#[cfg(test)]
#[test]
fn rotation_argument_selects_panel_mapping() {
    for (value, expected) in [
        ("0", Rotation::None),
        ("90", Rotation::Clockwise90),
        ("180", Rotation::Clockwise180),
        ("270", Rotation::Clockwise270),
    ] {
        assert_eq!(
            config_from_arguments([format!("--rotation={value}")]).rotation,
            expected
        );
    }
}

#[cfg(test)]
#[test]
fn background_argument_selects_config_storage() {
    assert_eq!(
        config_from_arguments([]).background_storage,
        BackgroundStorage::Heap
    );
    for (argument, expected) in [
        ("--background=heap", BackgroundStorage::Heap),
        ("--background=mma", BackgroundStorage::Mma),
        ("--mma-background", BackgroundStorage::Mma),
    ] {
        assert_eq!(
            config_from_arguments([argument.into()]).background_storage,
            expected
        );
    }
    assert!(
        std::panic::catch_unwind(|| config_from_arguments(["--background=auto".into()])).is_err()
    );
}

#[cfg(test)]
#[test]
fn touch_argument_keeps_auto_and_explicit_paths_distinct() {
    assert!(config_from_arguments([]).inputs.is_empty());
    assert!(matches!(
        config_from_arguments(["--touch=auto".into()])
            .inputs
            .as_slice(),
        [InputDevice::AutoTouchscreen]
    ));
    assert!(
        matches!(config_from_arguments(["--touch=/dev/input/event2".into()]).inputs.as_slice(), [InputDevice::Touchscreen { path }] if path == std::path::Path::new("/dev/input/event2"))
    );
}
