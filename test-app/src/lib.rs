use slint_bridge::{Config, InputDevice};

slint::include_modules!();

pub mod showcase;

#[path = "../fixtures/textures.rs"]
mod textures;
pub use textures::configure_textures;

/// Device paths are chosen by the caller, independently of the board feature.
pub fn config_from_args() -> Config {
    let mut config = Config::default();
    for argument in std::env::args().skip(1) {
        if let Some(path) = argument.strip_prefix("--framebuffer=") {
            config.framebuffer = path.into();
        } else if let Some(path) = argument.strip_prefix("--touch=") {
            config.inputs.push(InputDevice::touchscreen(path));
        }
    }
    config
}
