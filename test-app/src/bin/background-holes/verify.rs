//! Exclusive NX5 test: compare both physical pages with independent software snapshots.
use slint::{ComponentHandle, Rgba8Pixel, SharedPixelBuffer};
use slint_bridge_test_app::BackgroundHolesDemo;
use std::{ffi::CString, os::unix::ffi::OsStrExt, time::Duration};

pub fn run(
    app: &BackgroundHolesDemo,
    framebuffer: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let framebuffer = CString::new(framebuffer.as_os_str().as_bytes())?;
    let plugin = unsafe { libloading::Library::new("libbackground-checker.so")? };
    let check = unsafe {
        plugin.get::<unsafe extern "C" fn(*const libc::c_char, *const u8, *const u8, u32, u32) -> i32>(b"bridge_background_check\0")?
    };
    let mut previous: Option<SharedPixelBuffer<Rgba8Pixel>> = None;
    let mut previous_page = None;
    // Same actions as the visible controls: move/resize, hide both,
    // restore, foreground updates, dialog, changes behind it, close and reset.
    let actions = [
        7, 0, 1, 3, 4, 8, 2, 5, 2, 5, 6, 0, 1, -1, 7, 3, 4, 6, 5, -1, 5, 8, 7,
    ];
    for (stage, action) in actions.into_iter().enumerate() {
        if action < 0 {
            app.set_modal(false);
            app.set_revision(app.get_revision() + 1);
        } else {
            app.invoke_action(action);
        }
        slint::Timer::single_shot(Duration::from_millis(350), || {
            slint::quit_event_loop().unwrap();
        });
        app.run()?; // The bridge drains the outstanding submission before returning.
        let current = app.window().take_snapshot()?;
        let previous_ptr = previous
            .as_ref()
            .map_or(std::ptr::null(), |p| p.as_bytes().as_ptr());
        if stage == 0 {
            let mut corrupt = current.clone();
            corrupt.make_mut_slice()[0].a = 0;
            let rejected = unsafe {
                check(
                    framebuffer.as_ptr(),
                    corrupt.as_bytes().as_ptr(),
                    std::ptr::null(),
                    current.width(),
                    current.height(),
                )
            };
            if rejected != -1 {
                return Err("physical checker accepted a deliberately incorrect pixel".into());
            }
            eprintln!("background-pages: negative_control=rejected");
        }
        let page = unsafe {
            check(
                framebuffer.as_ptr(),
                current.as_bytes().as_ptr(),
                previous_ptr,
                current.width(),
                current.height(),
            )
        };
        if !matches!(page, 0 | 1) {
            return Err(format!("physical page comparison failed at stage {stage}").into());
        }
        if previous_page == Some(page) {
            return Err(format!("page did not alternate at stage {stage}").into());
        }
        previous_page = Some(page);
        previous = Some(current);
        eprintln!("background-pages: stage={stage} action={action} page={page} passed=true");
    }
    eprintln!("background-pages: stages=23 result=0");
    Ok(())
}
