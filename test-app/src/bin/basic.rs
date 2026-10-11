use slint::ComponentHandle;
use slint_bridge_test_app::{BasicWindow, config_from_args};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    slint_bridge::init(config_from_args())?;
    BasicWindow::new()?.run()?;
    Ok(())
}
