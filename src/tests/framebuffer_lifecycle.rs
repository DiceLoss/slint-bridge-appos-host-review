use super::*;
use std::{
    os::{fd::FromRawFd, unix::fs::MetadataExt},
    sync::{Mutex, mpsc},
    thread,
    time::Duration,
};

#[derive(Debug)]
struct DropEvent {
    name: &'static str,
    mapped: bool,
    file_open: bool,
}

type Events = Arc<Mutex<Vec<DropEvent>>>;

fn mapping_exists(inode: u64) -> bool {
    std::fs::read_to_string("/proc/self/maps")
        .unwrap()
        .lines()
        .any(|line| line.split_whitespace().nth(4).and_then(|n| n.parse().ok()) == Some(inode))
}

fn record(events: &Events, name: &'static str, inode: u64, fd: i32) {
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // fstat accepts an already closed fd; compare the inode in case it was reused.
    let file_open =
        unsafe { libc::fstat(fd, &mut stat) } == 0 && u128::from(stat.st_ino) == u128::from(inode);
    events.lock().unwrap().push(DropEvent {
        name,
        mapped: mapping_exists(inode),
        file_open,
    });
}

struct Session {
    events: Events,
    inode: u64,
    fd: i32,
    dropped: mpsc::SyncSender<()>,
}

impl Drop for Session {
    fn drop(&mut self) {
        record(&self.events, "sdk", self.inode, self.fd);
        let _ = self.dropped.send(());
    }
}

#[test]
fn installed_platform_releases_resources_after_normal_exit() {
    installed_platform_exit(Exit::Normal);
}

#[test]
fn installed_platform_releases_resources_after_eio_exit() {
    installed_platform_exit(Exit::Eio);
}

#[test]
fn installed_platform_releases_resources_after_show_error() {
    installed_platform_exit(Exit::ShowError);
}

#[derive(Clone, Copy)]
enum Exit {
    Normal,
    Eio,
    ShowError,
}

fn installed_platform_exit(exit: Exit) {
    let name = match exit {
        Exit::Normal => {
            "framebuffer::lifecycle_tests::installed_platform_releases_resources_after_normal_exit"
        }
        Exit::Eio => {
            "framebuffer::lifecycle_tests::installed_platform_releases_resources_after_eio_exit"
        }
        Exit::ShowError => {
            "framebuffer::lifecycle_tests::installed_platform_releases_resources_after_show_error"
        }
    };
    // Each installed Slint platform needs its own process-global proxy.
    if std::env::var("SLINT_BRIDGE_TEST_CHILD").as_deref() != Ok(name) {
        let run = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("SLINT_BRIDGE_TEST_CHILD", name)
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
    let events: Events = Arc::default();
    let observed = events.clone();
    let (inode, visible_after_exit) = thread::spawn(move || {
        slint::slint! {
            export component ExitScene inherits Window {
                preferred-width: 8px; preferred-height: 6px;
                in-out property<length> required-width: 0px;
                min-width: root.required-width;
                background: red;
            }
        }
        let fd = unsafe {
            libc::syscall(
                libc::SYS_memfd_create,
                c"platform-exit-test".as_ptr(),
                libc::MFD_CLOEXEC,
            )
        } as i32;
        assert!(fd >= 0);
        let file = unsafe { File::from_raw_fd(fd) };
        let (fixed, variable) = crate::tests::info(8, 6, 32);
        file.set_len(u64::from(fixed.smem_len)).unwrap();
        let inode = file.metadata().unwrap().ino();
        let (dropped, _done) = mpsc::sync_channel(1);
        let session = Session {
            events: events.clone(),
            inode,
            fd,
            dropped,
        };
        let (presented, frame_ready) = mpsc::sync_channel(1);
        let contract = PresentationContract {
            validate_layout: crate::framebuffer::Layout::new,
            complete: Box::new(move |_, _, _| {
                record(&session.events, "submission", inode, fd);
                let _ = presented.try_send(());
                Ok(())
            }),
        };
        #[allow(unused_mut)]
        let mut framebuffer =
            Framebuffer::from_file(file, Layout::new(&fixed, &variable).unwrap(), contract)
                .unwrap();
        #[cfg(feature = "nx5-tde")]
        {
            let resources = events.clone();
            framebuffer.retained =
                crate::backends::nx5::tde::tests::buffer_with_drop_observer(8, 6, move || {
                    record(&resources, "tde", inode, fd)
                });
        }
        let mut inputs = Vec::new();
        let mut slave = None;
        if matches!(exit, Exit::Eio) {
            // Closing a PTY slave makes the actual master read syscall return EIO.
            let master =
                unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC) };
            assert!(master >= 0);
            assert_eq!(unsafe { libc::grantpt(master) }, 0);
            assert_eq!(unsafe { libc::unlockpt(master) }, 0);
            let mut path = [0; 128];
            assert_eq!(
                unsafe { libc::ptsname_r(master, path.as_mut_ptr(), path.len()) },
                0
            );
            let slave_fd = unsafe {
                libc::open(
                    path.as_ptr(),
                    libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
                )
            };
            assert!(slave_fd >= 0);
            slave = Some(unsafe { File::from_raw_fd(slave_fd) });
            inputs.push(crate::input::Evdev {
                file: unsafe { File::from_raw_fd(master) },
                decoder: crate::input::Decoder::new(
                    slint::PhysicalSize::new(8, 6),
                    crate::Rotation::None,
                    None,
                    &[],
                )
                .unwrap(),
            });
        }
        let platform = crate::runtime::BridgePlatform::with_inputs(
            crate::Config::default(),
            framebuffer,
            inputs,
            Vec::new(),
        );
        slint::platform::set_platform(Box::new(platform)).unwrap();
        let app = ExitScene::new().unwrap();
        if matches!(exit, Exit::ShowError) {
            app.set_required_width(200.0);
            // A rejected first show never enters run_event_loop or submits a page.
            let error = app.run().unwrap_err().to_string();
            assert!(error.contains("window layout constraints"), "{error}");
        } else {
            let control = thread::spawn(move || {
                let rendered = frame_ready.recv_timeout(Duration::from_secs(3)).is_ok();
                slint::invoke_from_event_loop(move || {
                    if rendered && matches!(exit, Exit::Eio) {
                        drop(slave);
                    } else {
                        slint::quit_event_loop().unwrap();
                    }
                })
                .unwrap();
                assert!(rendered, "no frame submitted before exit trigger");
            });
            let result = app.run();
            control.join().unwrap();
            if matches!(exit, Exit::Eio) {
                let error = result.unwrap_err().to_string();
                assert!(
                    error.contains("read evdev input") && error.contains("os error 5"),
                    "{error}"
                );
            } else {
                result.unwrap();
            }
        }
        // Leave the installed platform in Slint, exactly as an application does.
        (inode, app.window().is_visible())
    })
    .join()
    .unwrap();
    let events = observed.lock().unwrap();
    assert_eq!(
        events.iter().any(|event| event.name == "submission"),
        !matches!(exit, Exit::ShowError),
        "unexpected submission state: {events:?}"
    );
    let releases: Vec<_> = events
        .iter()
        .filter(|event| event.name != "submission")
        .map(|event| event.name)
        .collect();
    #[cfg(feature = "nx5-tde")]
    assert_eq!(
        releases,
        ["tde", "sdk"],
        "installed platform retained resources: {events:?}"
    );
    #[cfg(not(feature = "nx5-tde"))]
    assert_eq!(
        releases,
        ["sdk"],
        "installed platform retained resources: {events:?}"
    );
    assert!(
        events.iter().all(|event| event.mapped && event.file_open),
        "{events:?}"
    );
    assert!(
        !mapping_exists(inode),
        "framebuffer mapping retained after UI thread exit"
    );
    assert!(
        !visible_after_exit,
        "failed or exited window remained visible"
    );
}

#[test]
fn framebuffer_drop_drains_submission_before_releasing_resources() {
    for outcome in ["ok", "error", "panic"] {
        let events: Events = Arc::default();
        let observed = events.clone();
        let (entered, started) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let (dropping, drop_started) = mpsc::sync_channel(1);
        let (returned, done) = mpsc::sync_channel(1);
        let (session_dropped, session_done) = mpsc::sync_channel(1);
        // Construct and destroy on the owning thread: TDE resources are not Send.
        let owner = thread::spawn(move || {
            let fd = unsafe {
                libc::syscall(
                    libc::SYS_memfd_create,
                    c"framebuffer-drop-test".as_ptr(),
                    libc::MFD_CLOEXEC,
                )
            } as i32;
            assert!(fd >= 0);
            let file = unsafe { File::from_raw_fd(fd) };
            let (fixed, variable) = crate::tests::info(8, 6, 32);
            file.set_len(u64::from(fixed.smem_len)).unwrap();
            let inode = file.metadata().unwrap().ino();
            let session = Session {
                events: events.clone(),
                inode,
                fd,
                dropped: session_dropped,
            };
            let contract = PresentationContract {
                validate_layout: crate::framebuffer::Layout::new,
                complete: Box::new(move |_, _, _| {
                    entered.send(()).unwrap();
                    gate.recv_timeout(Duration::from_secs(3)).unwrap();
                    record(&session.events, "submission", session.inode, session.fd);
                    match outcome {
                        "error" => Err(io::Error::other("injected completion failure")),
                        "panic" => panic!("injected completion panic"),
                        _ => Ok(()),
                    }
                }),
            };
            let mut framebuffer =
                Framebuffer::from_file(file, Layout::new(&fixed, &variable).unwrap(), contract)
                    .unwrap();
            #[cfg(feature = "nx5-tde")]
            {
                let resources = events.clone();
                framebuffer.retained =
                    crate::backends::nx5::tde::tests::buffer_with_drop_observer(8, 6, move || {
                        record(&resources, "tde", inode, fd)
                    });
            }
            // Submit the initialized zero canvas through the real write/submit path.
            framebuffer.prepared_damage = Some(vec![Rect {
                x0: 0,
                y0: 0,
                x1: 8,
                y1: 6,
            }]);
            framebuffer.present().unwrap();
            started.recv_timeout(Duration::from_secs(3)).unwrap();
            dropping.send(inode).unwrap();
            drop(framebuffer);
            record(&events, "framebuffer", inode, fd);
            returned.send(()).unwrap();
        });
        let inode = drop_started.recv_timeout(Duration::from_secs(3)).unwrap();
        let early = done.recv_timeout(Duration::from_millis(100));
        let mapped_while_pending = mapping_exists(inode);
        release.send(()).unwrap();
        if early.is_err() {
            done.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        session_done.recv_timeout(Duration::from_secs(3)).unwrap();
        owner.join().unwrap();
        let events = observed.lock().unwrap();
        assert!(
            early.is_err(),
            "drop returned with a pending submission: {events:?}"
        );
        assert!(
            mapped_while_pending,
            "mapping released during submission: {events:?}"
        );
        let names: Vec<_> = events.iter().map(|event| event.name).collect();
        #[cfg(feature = "nx5-tde")]
        assert_eq!(names, ["submission", "tde", "sdk", "framebuffer"]);
        #[cfg(not(feature = "nx5-tde"))]
        assert_eq!(names, ["submission", "sdk", "framebuffer"]);
        for event in events.iter().take(events.len() - 1) {
            assert!(event.mapped && event.file_open, "{outcome}: {events:?}");
        }
        let last = events.last().unwrap();
        assert!(
            !last.mapped && !last.file_open,
            "resources leaked: {events:?}"
        );
    }
}

#[cfg(feature = "nx5-tde")]
#[test]
fn unknown_dma_keeps_the_board_session_alive() {
    const NAME: &str = "framebuffer::lifecycle_tests::unknown_dma_keeps_the_board_session_alive";
    if std::env::var("SLINT_BRIDGE_TEST_CHILD").as_deref() != Ok(NAME) {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("SLINT_BRIDGE_TEST_CHILD", NAME)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        return;
    }
    let events: Events = Arc::default();
    let observed = events.clone();
    let fd = unsafe {
        libc::syscall(
            libc::SYS_memfd_create,
            c"quarantined-session".as_ptr(),
            libc::MFD_CLOEXEC,
        )
    } as i32;
    assert!(fd >= 0);
    let file = unsafe { File::from_raw_fd(fd) };
    let (fixed, variable) = crate::tests::info(512, 384, 32);
    file.set_len(u64::from(fixed.smem_len)).unwrap();
    let (dropped, _done) = mpsc::sync_channel(1);
    let session = Session {
        events,
        inode: file.metadata().unwrap().ino(),
        fd,
        dropped,
    };
    let contract = PresentationContract {
        validate_layout: Layout::new,
        complete: Box::new(move |_, _, _| {
            let _alive = &session;
            Ok(())
        }),
    };
    let mut framebuffer =
        Framebuffer::from_file(file, Layout::new(&fixed, &variable).unwrap(), contract).unwrap();
    framebuffer.retained = crate::backends::nx5::tde::tests::buffer_after_unknown_dma();
    drop(framebuffer);
    assert!(
        observed.lock().unwrap().is_empty(),
        "uncertain TDE work must not trigger SDK/SYS shutdown"
    );
}
