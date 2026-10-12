use super::*;
use crate::framebuffer::Framebuffer;

#[test]
fn sdk_session_releases_vo_after_failed_init_and_can_retry() {
    const NAME: &str =
        "backends::nx5::session_tests::sdk_session_releases_vo_after_failed_init_and_can_retry";
    let directory = match std::env::var_os("SLINT_BRIDGE_TEST_SDK_DIRECTORY") {
        Some(path) => std::path::PathBuf::from(path),
        None => {
            let directory =
                std::env::temp_dir().join(format!("slint-bridge-session {}", std::process::id()));
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("sdk.c"), include_str!("session-stub.c")).unwrap();
            assert!(
                std::process::Command::new("cc")
                    .args(["-shared", "-fPIC"])
                    .arg(directory.join("sdk.c"))
                    .arg("-o")
                    .arg(directory.join("libVideoDec.so"))
                    .status()
                    .unwrap()
                    .success()
            );
            for automatic in ["1", "0"] {
                let child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", NAME, "--nocapture"])
                    .env("SLINT_BRIDGE_TEST_SDK_DIRECTORY", &directory)
                    .env("SLINT_BRIDGE_TEST_VO_STATE", directory.join("vo"))
                    .env("SLINT_BRIDGE_TEST_AUTO_VO", automatic)
                    .output()
                    .unwrap();
                assert!(
                    child.status.success(),
                    "constructor auto-enable={automatic}: {}{}",
                    String::from_utf8_lossy(&child.stdout),
                    String::from_utf8_lossy(&child.stderr)
                );
            }
            fs::remove_dir_all(directory).unwrap();
            return;
        }
    };
    let state = directory.join("vo");
    fs::write(&state, "-----DEV CONFIG\n-----VIDEO LAYER STATUS\n").unwrap();
    let library = directory.join("libVideoDec.so");
    let acquire = || {
        Session::load(
            File::open("/dev/null").unwrap(),
            library.as_os_str(),
            || fs::read_to_string(&state),
        )
    };
    let session = acquire().unwrap();
    // Keep the library resident after the failed init, as another SDK user may.
    let observer = unsafe { libloading::Library::new(&library) }.unwrap();
    let references =
        unsafe { observer.get::<unsafe extern "C" fn() -> i32>(b"test_vo_references\0") }.unwrap();
    assert!(matches!(acquire(), Err(Error::DisplayNotAdmitted { .. })));
    let contract = PresentationContract {
        validate_layout,
        complete: Box::new(move |_, _, _| {
            let _alive = &session;
            Ok(())
        }),
    };
    let result = Framebuffer::open(&directory.join("missing-framebuffer"), contract);
    assert!(matches!(
        result,
        Err(Error::Io {
            operation: "open framebuffer",
            ..
        })
    ));
    assert_eq!(
        unsafe { references() },
        0,
        "failed init leaked a VO reference"
    );
    assert!(!fs::read_to_string(&state).unwrap().contains("0 Y"));

    let session = acquire().unwrap();
    let contract = PresentationContract {
        validate_layout,
        complete: Box::new(move |_, _, _| {
            let _alive = &session;
            Ok(())
        }),
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(directory.join("framebuffer"))
        .unwrap();
    let (fixed, variable) = crate::tests::info(8, 6, 32);
    file.set_len(u64::from(fixed.smem_len)).unwrap();
    let framebuffer =
        Framebuffer::from_file(file, Layout::new(&fixed, &variable).unwrap(), contract).unwrap();
    let result = crate::runtime::BridgePlatform::new(
        crate::Config {
            inputs: vec![crate::InputDevice::touchscreen(
                directory.join("missing-input"),
            )],
            ..Default::default()
        },
        framebuffer,
        Duration::ZERO,
    );
    assert!(matches!(
        result,
        Err(Error::Io {
            operation: "open evdev input",
            ..
        })
    ));
    assert_eq!(
        unsafe { references() },
        0,
        "input initialization failure leaked VO"
    );

    // No constructor runs this time: acquire/release the resident SDK once.
    let session = acquire().unwrap();
    assert_eq!(unsafe { references() }, 1);
    drop(session);
    assert_eq!(unsafe { references() }, 0);
    drop(observer);
    // Releasing the last handle must not orphan the SDK's cached descriptors.
    let descriptors = || fs::read_dir("/proc/self/fd").unwrap().count();
    let before = descriptors();
    for _ in 0..3 {
        drop(acquire().unwrap());
        assert_eq!(descriptors(), before, "SDK reload orphaned a cached fd");
    }
    assert!(!fs::read_to_string(&state).unwrap().contains("0 Y"));
    let mut reads = 0;
    let result = Session::load(
        File::open("/dev/null").unwrap(),
        library.as_os_str(),
        || {
            reads += 1;
            if reads == 1 {
                fs::read_to_string(&state)
            } else {
                Err(io::ErrorKind::PermissionDenied.into())
            }
        },
    );
    assert!(matches!(
        result,
        Err(Error::Io {
            operation: "read NX5 VO state",
            ..
        })
    ));
    assert!(!fs::read_to_string(&state).unwrap().contains("0 Y"));
}
