//! Exclusive NX5 transparent UI and post-compositor regression; no video decoder.
use slint::ComponentHandle;
use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path, time::Duration};

include!(concat!(env!("OUT_DIR"), "/alpha.rs"));

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("usage: alpha-regression OUTPUT_DIRECTORY [device paths]")?;
    let output = Path::new(&output);
    if !output.is_dir() {
        return Err("output directory must exist".into());
    }
    let config = slint_bridge_test_app::config_from_args();
    let framebuffer = CString::new(config.framebuffer.as_os_str().as_bytes())?;
    slint_bridge::init(config)?;
    // The plugin links to the same already-owned SDK and never closes VO.
    let plugin = unsafe { libloading::Library::new("libalpha-observer.so")? };
    let check = unsafe {
        plugin
            .get::<unsafe extern "C" fn(*const libc::c_char, *const libc::c_char, i32, i32) -> i32>(
                b"bridge_alpha_check\0",
            )?
    };
    let app = AlphaWindow::new()?;
    let mut colors = Vec::new();
    for channel in 0..3 {
        for reference in [false, true] {
            for alpha in [0, 1, 63, 127, 128, 191, 254, 255] {
                let mut rgb = [0; 3];
                rgb[channel] = if reference { alpha } else { 255 };
                colors.push(slint::Color::from_argb_u8(
                    if reference { 255 } else { alpha },
                    rgb[0],
                    rgb[1],
                    rgb[2],
                ));
            }
        }
    }
    app.set_colors(std::rc::Rc::new(slint::VecModel::from(colors)).into());
    let mut previous_page = None;
    for (stage, (offset, visible)) in [(4, true), (12, true), (8, false), (8, true), (4, false)]
        .into_iter()
        .enumerate()
    {
        app.set_offset(offset);
        app.set_patches(visible);
        slint::Timer::single_shot(Duration::from_millis(700), || {
            slint::quit_event_loop().unwrap();
        });
        app.run()?; // Drains presentation before reading pages or starting WBC.
        let prefix = CString::new(output.join(format!("stage-{stage}")).as_os_str().as_bytes())?;
        let page = unsafe {
            check(
                framebuffer.as_ptr(),
                prefix.as_ptr(),
                offset,
                i32::from(visible),
            )
        };
        if !matches!(page, 0 | 1) {
            return Err(format!("alpha stage {stage} failed: {page}").into());
        }
        if let Some(previous) = previous_page {
            assert_ne!(page, previous, "stage did not flip to the other page");
        }
        previous_page = Some(page);
        eprintln!("alpha: stage={stage} offset={offset} visible={visible} page={page} passed=true");
    }
    eprintln!("alpha: stages=5 result=0");
    Ok(())
}
