use super::*;
use std::{
    io::Read,
    sync::atomic::{AtomicUsize, Ordering},
};

fn bits(bytes: &mut [u8], codes: &[usize]) {
    for &code in codes {
        bytes[code / 8] |= 1 << (code % 8);
    }
}

fn legacy(direct: bool) -> Capabilities {
    let mut c = Capabilities {
        keys: [0; 96],
        ..Default::default()
    };
    bits(&mut c.events, &[1, 3]);
    bits(&mut c.absolute, &[0, 1]);
    bits(&mut c.keys, &[330]);
    if direct {
        bits(&mut c.properties, &[1]);
    }
    c
}

#[test]
fn accepts_direct_and_propertyless_single_touch() {
    assert_eq!(protocol(&legacy(true)), Ok(false));
    assert_eq!(protocol(&legacy(false)), Ok(false));
}

#[test]
fn distinguishes_type_a_slots_and_legacy_compatibility() {
    let mut c = legacy(true);
    c.absolute = [0; 8];
    bits(&mut c.absolute, &[53, 54, 57]);
    assert_eq!(protocol(&c), Ok(true));
    bits(&mut c.absolute, &[47]);
    assert!(protocol(&c).is_err());
    bits(&mut c.absolute, &[0, 1]);
    assert_eq!(protocol(&c), Ok(false));
    c.keys.fill(0);
    assert!(protocol(&c).is_err());
    c.absolute[47 / 8] &= !(1 << (47 % 8));
    assert_eq!(protocol(&c), Ok(true));
}

#[test]
fn rejects_non_touch_pointer_tablet_and_sensor_devices() {
    assert!(protocol(&Capabilities::default()).is_err());
    let mut joystick = legacy(false);
    joystick.keys.fill(0);
    assert!(protocol(&joystick).is_err());
    for property in [0, 2, 3, 4, 6] {
        let mut c = legacy(true);
        bits(&mut c.properties, &[property]);
        assert!(protocol(&c).is_err());
    }
    for key in [320, 321, 331, 332] {
        let mut c = legacy(true);
        bits(&mut c.keys, &[key]);
        assert!(protocol(&c).is_err());
    }
    let mut c = legacy(false);
    bits(&mut c.keys, &[325]);
    assert!(protocol(&c).is_err());
    bits(&mut c.properties, &[1]);
    assert_eq!(protocol(&c), Ok(false));
}

struct Directory(PathBuf);
impl Directory {
    fn new(names: &[&str]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "slint-touch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        for name in names {
            fs::write(path.join(name), "original").unwrap();
        }
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn candidate(path: &Path) -> io::Result<Probe> {
    let axis = Axis {
        min: 0,
        max: 1024,
        value: 0,
    };
    Ok(Probe::Candidate(Candidate {
        file: File::open(path)?,
        name: "fixture touch".into(),
        type_a: false,
        axes: (axis, axis),
    }))
}

#[test]
fn skips_non_event_entries_and_keeps_the_probed_descriptor() {
    let dir = Directory::new(&["event0", "event2", "eventX", "mouse0", "event"]);
    let mut seen = Vec::new();
    let (path, mut chosen) = select(&dir.0, |path| {
        seen.push(path.file_name().unwrap().to_owned());
        if path.ends_with("event0") {
            return Ok(Probe::Rejected("keyboard".into()));
        }
        candidate(path)
    })
    .unwrap();
    assert_eq!(seen, ["event0", "event2"]);
    assert_eq!(path, dir.0.join("event2"));
    fs::rename(&path, dir.0.join("previous-device")).unwrap();
    fs::write(&path, "replacement").unwrap();
    let mut text = String::new();
    chosen.file.read_to_string(&mut text).unwrap();
    assert_eq!(text, "original");
}

#[test]
fn rejects_multiple_candidates_instead_of_picking_lowest_event() {
    let dir = Directory::new(&["event0", "event2"]);
    let error = select(&dir.0, candidate).err().unwrap();
    assert!(error.to_string().contains("multiple touchscreens"));
    assert!(error.to_string().contains("event0"));
    assert!(error.to_string().contains("event2"));
}

#[test]
fn unreadable_or_disappeared_candidates_make_the_scan_incomplete() {
    for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::NotFound] {
        let dir = Directory::new(&["event0", "event2"]);
        let error = select(&dir.0, |path| {
            if path.ends_with("event2") {
                return Err(io::Error::from(kind));
            }
            candidate(path)
        })
        .err()
        .unwrap();
        assert!(
            matches!(error, Error::Io { source, .. } if source.kind() == kind && source.to_string().contains("incomplete"))
        );
    }
}

#[test]
fn reports_absent_and_rejected_candidates() {
    let empty = Directory::new(&[]);
    assert!(
        matches!(select(&empty.0, candidate), Err(Error::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound)
    );
    let dir = Directory::new(&["event0"]);
    let error = select(&dir.0, |_| {
        Ok(Probe::Rejected("absolute axis has no usable range".into()))
    })
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("absolute axis has no usable range")
    );
    assert!(matches!(
        inspect(&dir.0.join("event0")),
        Ok(Probe::Rejected(_))
    ));
}
