//! NX5-only composed-output sampling; normal UI checks use Slint MCP.

pub struct Observer(Option<std::thread::JoinHandle<i32>>);

impl Observer {
    pub fn join(mut self) -> std::thread::Result<i32> {
        self.0.take().unwrap().join()
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        // Error returns also wait for the plugin to release WBC before exit.
        if let Some(observer) = self.0.take()
            && observer.join().is_err()
        {
            eprintln!("display: WBC observer panicked during cleanup");
        }
    }
}

pub fn start(samples: String) -> Observer {
    Observer(Some(std::thread::spawn(move || {
        // Loading this SDK in a second process resets the global VO/SYS hardware.
        unsafe {
            let library = libloading::Library::new("/run/slint-bridge-display/libobserver.so")
                .expect("load observer");
            let function = library
                .get::<unsafe extern "C" fn(i32, *const *const libc::c_char) -> i32>(
                    b"bridge_wbc_observe\0",
                )
                .expect("observer function");
            let argument = std::ffi::CString::new(samples).unwrap();
            let args = [c"observer".as_ptr(), argument.as_ptr()];
            let result = function(2, args.as_ptr());
            eprintln!("display: observation result={result}");
            result
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::{
        ComponentHandle,
        platform::{
            Platform, WindowAdapter,
            software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
        },
    };
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

    struct FailingPlatform(Rc<MinimalSoftwareWindow>);
    impl Platform for FailingPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            Ok(self.0.clone())
        }
        fn run_event_loop(&self) -> Result<(), slint::PlatformError> {
            Err(slint::PlatformError::from(
                "injected UI failure".to_string(),
            ))
        }
    }

    #[test]
    fn error_return_waits_for_observer_cleanup() {
        let (entered, started) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let (finished, cleaned) = mpsc::sync_channel(1);
        let (returned, done) = mpsc::sync_channel(1);
        let cleanup = Arc::new(AtomicBool::new(false));
        let owner = thread::spawn(move || {
            let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
            window.set_size(slint::PhysicalSize::new(1024, 600));
            slint::platform::set_platform(Box::new(FailingPlatform(window))).unwrap();
            let worker_cleanup = cleanup.clone();
            let observer = Observer(Some(thread::spawn(move || {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(3)).unwrap();
                worker_cleanup.store(true, Ordering::SeqCst);
                finished.send(()).unwrap();
                0
            })));
            let result = (|| -> Result<(), slint::PlatformError> {
                let app = slint_bridge_test_app::App::new()?;
                let observer = Rc::new(RefCell::new(Some(observer)));
                // The application and its timer callback share this owner.
                let timer_observer = observer.clone();
                let timer = slint::Timer::default();
                timer.start(
                    slint::TimerMode::Repeated,
                    Duration::from_secs(1),
                    move || {
                        let _ = timer_observer.borrow().is_some();
                    },
                );
                app.run()?;
                Ok(())
            })();
            returned
                .send((
                    result.map_err(|e| e.to_string()),
                    cleanup.load(Ordering::SeqCst),
                ))
                .unwrap();
        });
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        let early = done.recv_timeout(Duration::from_millis(100));
        release.send(()).unwrap();
        let (result, cleanup_before_return) =
            early.unwrap_or_else(|_| done.recv_timeout(Duration::from_secs(3)).unwrap());
        cleaned.recv_timeout(Duration::from_secs(3)).unwrap();
        owner.join().unwrap();
        assert_eq!(result, Err("injected UI failure".to_string()));
        assert!(
            cleanup_before_return,
            "error return detached the WBC observer"
        );
    }

    #[test]
    fn observer_panic_during_cleanup_preserves_the_ui_error() {
        let result: Result<(), &'static str> = {
            let _observer = Observer(Some(thread::spawn(|| panic!("injected observer panic"))));
            Err("injected UI failure")
        };
        assert_eq!(result, Err("injected UI failure"));
    }

    #[test]
    fn explicit_join_preserves_the_observer_result_and_panic() {
        assert_eq!(Observer(Some(thread::spawn(|| 7))).join().unwrap(), 7);
        assert!(
            Observer(Some(thread::spawn(|| panic!("injected observer panic"))))
                .join()
                .is_err()
        );
    }
}
