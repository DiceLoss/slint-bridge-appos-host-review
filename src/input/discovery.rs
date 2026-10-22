//! Startup-only evdev selection. Keep the probed descriptor through decoding.
use super::{Axis, Decoder, Error, Evdev, PhysicalSize, Rotation};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

fn has(bits: &[u8], code: usize) -> bool {
    bits[code / 8] & (1 << (code % 8)) != 0
}

struct Capabilities {
    events: [u8; 4],
    absolute: [u8; 8],
    keys: [u8; 96],
    properties: [u8; 4],
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            events: [0; 4],
            absolute: [0; 8],
            keys: [0; 96],
            properties: [0; 4],
        }
    }
}

fn protocol(c: &Capabilities) -> Result<bool, &'static str> {
    if !has(&c.events, 3) {
        return Err("no absolute coordinates");
    }
    // POINTER, BUTTONPAD, SEMI_MT, TOPBUTTONPAD, ACCELEROMETER; pen/tool buttons.
    if [0, 2, 3, 4, 6].into_iter().any(|p| has(&c.properties, p))
        || [320, 321, 322, 323, 324, 326, 327, 331, 332]
            .into_iter()
            .any(|key| has(&c.keys, key))
    {
        return Err("pointer, tablet, touchpad or sensor capabilities");
    }
    if !has(&c.properties, 1) && (has(&c.events, 2) || has(&c.keys, 325)) {
        return Err("legacy device cannot be distinguished from an indirect pointer");
    }
    if has(&c.events, 1) && has(&c.absolute, 0) && has(&c.absolute, 1) && has(&c.keys, 330) {
        return Ok(false);
    }
    if !has(&c.absolute, 47) && [53, 54, 57].into_iter().all(|a| has(&c.absolute, a)) {
        return Ok(true);
    }
    Err("requires ABS_X/Y + BTN_TOUCH or Type-A tracking IDs; pure Type-B is unsupported")
}

fn query<const N: usize>(file: &File, number: u8) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    let request = 0x8000_0000u32 | (N as u32) << 16 | u32::from(b'E') << 8 | u32::from(number);
    // SAFETY: N is the ioctl output size, and every number is an evdev byte-array query.
    if unsafe {
        libc::ioctl(
            file.as_raw_fd(),
            libc::c_ulong::from(request),
            bytes.as_mut_ptr(),
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(bytes)
}

struct Candidate {
    file: File,
    name: String,
    type_a: bool,
    axes: (Axis, Axis),
}

enum Probe {
    Candidate(Candidate),
    Rejected(String),
}

fn inspect(path: &Path) -> io::Result<Probe> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.file_type().is_char_device() {
        return Ok(Probe::Rejected("not a character device".into()));
    }
    let mut caps = Capabilities {
        events: query(&file, 0x20)?,
        ..Default::default()
    };
    if !has(&caps.events, 3) {
        return Ok(Probe::Rejected("no absolute coordinates".into()));
    }
    caps.absolute = query(&file, 0x23)?;
    caps.keys = query(&file, 0x21)?;
    caps.properties = match query(&file, 0x09) {
        Ok(bits) => bits,
        Err(e) if matches!(e.raw_os_error(), Some(libc::ENOTTY | libc::EINVAL)) => [0; 4],
        Err(e) => return Err(e),
    };
    let name = query::<128>(&file, 0x06)?;
    let name =
        String::from_utf8_lossy(name.split(|&b| b == 0).next().unwrap_or_default()).into_owned();
    let type_a = match protocol(&caps) {
        Ok(value) => value,
        Err(reason) => return Ok(Probe::Rejected(format!("{name:?}: {reason}"))),
    };
    let (x, y) = if type_a { (53, 54) } else { (0, 1) };
    let axes = match (Evdev::axis(&file, x), Evdev::axis(&file, y)) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(Error::Io { source, .. }), _) | (_, Err(Error::Io { source, .. })) => {
            return Err(source);
        }
        (Err(Error::UnsupportedInput(reason)), _) | (_, Err(Error::UnsupportedInput(reason))) => {
            return Ok(Probe::Rejected(format!("{name:?}: {reason}")));
        }
        (Err(e), _) | (_, Err(e)) => return Err(io::Error::other(e.to_string())),
    };
    Ok(Probe::Candidate(Candidate {
        file,
        name,
        type_a,
        axes,
    }))
}

fn select(
    directory: &Path,
    mut probe: impl FnMut(&Path) -> io::Result<Probe>,
) -> Result<(PathBuf, Candidate), Error> {
    let fail = |kind, message| Error::io("discover touchscreen", io::Error::new(kind, message));
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| Error::io("enumerate evdev inputs", e))? {
        let entry = entry.map_err(|e| Error::io("enumerate evdev inputs", e))?;
        if entry
            .file_name()
            .to_str()
            .and_then(|s| s.strip_prefix("event"))
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    let mut candidates = Vec::new();
    let mut reports = Vec::new();
    let mut failure = None;
    for path in paths {
        match probe(&path) {
            Ok(Probe::Candidate(candidate)) => {
                reports.push(format!(
                    "{} ({:?}): supported",
                    path.display(),
                    candidate.name
                ));
                candidates.push((path, candidate));
            }
            Ok(Probe::Rejected(reason)) => reports.push(format!("{}: {reason}", path.display())),
            Err(e) => {
                failure.get_or_insert(e.kind());
                reports.push(format!("{}: {e}", path.display()));
            }
        }
    }
    let details = reports.join("; ");
    if let Some(kind) = failure {
        return Err(fail(
            kind,
            format!("incomplete device scan; refusing to guess: {details}"),
        ));
    }
    match candidates.len() {
        1 => Ok(candidates.pop().unwrap()),
        0 => Err(fail(
            io::ErrorKind::NotFound,
            format!(
                "no supported touchscreen in {}: {details}",
                directory.display()
            ),
        )),
        _ => Err(fail(
            io::ErrorKind::InvalidInput,
            format!("multiple touchscreens; specify a path: {details}"),
        )),
    }
}

pub(super) fn open(panel: PhysicalSize, rotation: Rotation) -> Result<Evdev, Error> {
    let (path, candidate) = select(Path::new("/dev/input"), inspect)?;
    let mut decoder = Decoder::new(panel, rotation, Some(candidate.axes), &[])?;
    if candidate.type_a {
        decoder.enable_type_a();
    }
    eprintln!(
        "slint-bridge: auto touchscreen {} ({:?}), {}, axes {}..{} / {}..{}",
        path.display(),
        candidate.name,
        if candidate.type_a {
            "Type-A"
        } else {
            "single-touch"
        },
        candidate.axes.0.min,
        candidate.axes.0.max,
        candidate.axes.1.min,
        candidate.axes.1.max
    );
    Ok(Evdev {
        file: candidate.file,
        decoder,
    })
}

#[cfg(test)]
mod tests;
