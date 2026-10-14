use slint_bridge::{Config, InputDevice, Rotation};

slint::include_modules!();

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
        if let Some(path) = argument.strip_prefix("--framebuffer=") {
            config.framebuffer = path.into();
        } else if let Some(path) = argument.strip_prefix("--touch=") {
            config.inputs.push(InputDevice::touchscreen(path));
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
