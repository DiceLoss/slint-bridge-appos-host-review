use crate::{Error, Rotation};
use slint::{
    LogicalPosition, PhysicalSize, SharedString,
    platform::{Key, PointerEventButton, WindowEvent},
};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::PathBuf,
};

/// An evdev key code and the Slint character or special key it emits.
#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub code: u16,
    pub text: String,
}

impl KeyBinding {
    pub fn new(code: u16, text: impl Into<SharedString>) -> Self {
        Self {
            code,
            text: text.into().to_string(),
        }
    }
}

/// Explicit or startup-discovered input selection. Touchscreens use ABS_X/ABS_Y and BTN_TOUCH,
/// or Type-A ABS_MT_POSITION_X/Y with tracking IDs (no slots);
/// key devices use mappings rather than assuming a keyboard layout or IME.
#[derive(Debug, Clone)]
pub enum InputDevice {
    /// Discover exactly one supported touchscreen when the platform starts.
    /// Does not monitor hotplug or select between multiple candidates.
    AutoTouchscreen,
    Touchscreen {
        path: PathBuf,
    },
    Keys {
        path: PathBuf,
        bindings: Vec<KeyBinding>,
    },
}

impl InputDevice {
    /// Select the unique supported touchscreen from `/dev/input/event*` at startup.
    /// Ambiguous or incomplete scans fail; no device is selected by event number.
    pub fn auto_touchscreen() -> Self {
        Self::AutoTouchscreen
    }

    pub fn touchscreen(path: impl Into<PathBuf>) -> Self {
        Self::Touchscreen { path: path.into() }
    }

    /// Standard navigation buttons. Use `Keys` directly for product-specific mappings.
    pub fn keys(path: impl Into<PathBuf>) -> Self {
        Self::Keys {
            path: path.into(),
            bindings: vec![
                KeyBinding::new(1, Key::Escape),
                KeyBinding::new(14, Key::Backspace),
                KeyBinding::new(15, Key::Tab),
                KeyBinding::new(28, Key::Return),
                KeyBinding::new(57, " "),
                KeyBinding::new(102, Key::Home),
                KeyBinding::new(103, Key::UpArrow),
                KeyBinding::new(104, Key::PageUp),
                KeyBinding::new(105, Key::LeftArrow),
                KeyBinding::new(106, Key::RightArrow),
                KeyBinding::new(107, Key::End),
                KeyBinding::new(108, Key::DownArrow),
                KeyBinding::new(109, Key::PageDown),
                KeyBinding::new(111, Key::Delete),
            ],
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Axis {
    pub min: i32,
    pub max: i32,
    pub value: i32,
}

impl Axis {
    fn coordinate(self, extent: u32) -> f32 {
        let offset = i64::from(self.value) - i64::from(self.min);
        let range = i64::from(self.max) - i64::from(self.min);
        (offset.clamp(0, range) as f64 / range as f64 * f64::from(extent - 1)) as f32
    }
}

struct Touch {
    x: Axis,
    y: Axis,
    down: bool,
    reported_down: bool,
    reported_position: LogicalPosition,
}

#[derive(Default)]
struct Contact {
    id: Option<i32>,
    x: Option<i32>,
    y: Option<i32>,
}

#[derive(Default)]
struct TypeA {
    contact: Contact,
    first: Option<(i32, i32, i32)>,
    primary_position: Option<(i32, i32)>,
    primary: Option<i32>,
    blocked: bool,
    button: Option<bool>,
}

impl TypeA {
    fn finish_contact(&mut self) {
        let contact = std::mem::take(&mut self.contact);
        if let (Some(id), Some(x), Some(y)) = (contact.id, contact.x, contact.y)
            && id >= 0
        {
            self.first.get_or_insert((id, x, y));
            if self.primary == Some(id) {
                self.primary_position = Some((x, y));
            }
        }
    }

    fn clear_frame(&mut self) {
        self.contact = Contact::default();
        self.first = None;
        self.primary_position = None;
        self.button = None;
    }

    fn finish_frame(&mut self, touch: &mut Touch) {
        self.finish_contact();
        if self.button == Some(false) {
            self.first = None;
            self.primary_position = None;
        }
        if self.primary.is_none()
            && !self.blocked
            && let Some((id, x, y)) = self.first
        {
            self.primary = Some(id);
            self.primary_position = Some((x, y));
        }
        if let Some((x, y)) = self.primary_position {
            touch.x.value = x;
            touch.y.value = y;
            touch.down = true;
        } else {
            touch.down = false;
            self.primary = None;
            // Wait for all contacts to lift before accepting a new primary.
            self.blocked = self.first.is_some();
        }
        self.clear_frame();
    }
}

pub(crate) struct Decoder {
    panel: PhysicalSize,
    rotation: Rotation,
    touch: Option<Touch>,
    type_a: Option<TypeA>,
    bindings: BTreeMap<u16, String>,
    pressed: BTreeMap<u16, String>,
    dropped: bool,
}

impl Decoder {
    pub(crate) fn new(
        panel: PhysicalSize,
        rotation: Rotation,
        axes: Option<(Axis, Axis)>,
        bindings: &[KeyBinding],
    ) -> Result<Self, Error> {
        if panel.width == 0
            || panel.height == 0
            || axes.is_some_and(|(x, y)| x.min >= x.max || y.min >= y.max)
        {
            return Err(Error::UnsupportedInput(
                "invalid touchscreen geometry or axis range",
            ));
        }
        let mut mapping = BTreeMap::new();
        for binding in bindings {
            if binding.code >= 768
                || binding.text.is_empty()
                || mapping.insert(binding.code, binding.text.clone()).is_some()
            {
                return Err(Error::InvalidConfig(
                    "key mappings require distinct Linux key codes below 768 and nonempty text",
                ));
            }
        }
        Ok(Self {
            panel,
            rotation,
            bindings: mapping,
            pressed: BTreeMap::new(),
            dropped: false,
            type_a: None,
            touch: axes.map(|(x, y)| Touch {
                x,
                y,
                down: false,
                reported_down: false,
                reported_position: LogicalPosition::default(),
            }),
        })
    }

    pub(crate) fn enable_type_a(&mut self) {
        self.type_a = Some(TypeA::default());
    }

    fn flush_touch(&mut self, out: &mut Vec<WindowEvent>) {
        let Some(t) = &mut self.touch else {
            return;
        };
        let position = self.rotation.input(
            self.panel,
            t.x.coordinate(self.panel.width),
            t.y.coordinate(self.panel.height),
        );
        if t.down != t.reported_down {
            out.push(if t.down {
                WindowEvent::PointerPressed {
                    position,
                    button: PointerEventButton::Left,
                }
            } else {
                WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                }
            });
            if !t.down {
                out.push(WindowEvent::PointerExited);
            }
        } else if t.down && position != t.reported_position {
            out.push(WindowEvent::PointerMoved { position });
        }
        t.reported_down = t.down;
        t.reported_position = position;
    }

    fn key(&mut self, code: u16, value: i32, out: &mut Vec<WindowEvent>) {
        let Some(text) = self.bindings.get(&code) else {
            return;
        };
        match value {
            1 if !self.pressed.contains_key(&code) => {
                self.pressed.insert(code, text.clone());
                out.push(WindowEvent::KeyPressed {
                    text: text.as_str().into(),
                });
            }
            2 if self.pressed.contains_key(&code) => out.push(WindowEvent::KeyPressRepeated {
                text: text.as_str().into(),
            }),
            0 => {
                if let Some(text) = self.pressed.remove(&code) {
                    out.push(WindowEvent::KeyReleased { text: text.into() });
                }
            }
            _ => {}
        }
    }

    /// True requests an EVIOCG* resynchronization after the SYN_DROPPED boundary.
    pub(crate) fn event(
        &mut self,
        kind: u16,
        code: u16,
        value: i32,
        out: &mut Vec<WindowEvent>,
    ) -> bool {
        if kind == 0 && code == 3 {
            self.dropped = true;
            if let Some(mt) = &mut self.type_a {
                mt.clear_frame();
            }
            return false;
        }
        if self.dropped {
            if kind == 0 && code == 0 {
                self.dropped = false;
                return true;
            }
            return false;
        }
        if let (Some(mt), Some(touch)) = (&mut self.type_a, &mut self.touch) {
            match (kind, code) {
                (3, 53) => mt.contact.x = Some(value),
                (3, 54) => mt.contact.y = Some(value),
                (3, 57) => mt.contact.id = Some(value),
                (0, 2) => mt.finish_contact(),
                (0, 0) => {
                    mt.finish_frame(touch);
                    self.flush_touch(out);
                }
                (1, 330) => mt.button = Some(value != 0),
                (1, code) => self.key(code, value, out),
                _ => {}
            }
            return false;
        }
        match (kind, code) {
            (0, 0) => self.flush_touch(out),
            (3, 0) => {
                if let Some(t) = &mut self.touch {
                    t.x.value = value;
                }
            }
            (3, 1) => {
                if let Some(t) = &mut self.touch {
                    t.y.value = value;
                }
            }
            (1, 330) if self.touch.is_some() => self.touch.as_mut().unwrap().down = value != 0,
            (1, code) => self.key(code, value, out),
            _ => {}
        }
        false
    }

    pub(crate) fn resync(
        &mut self,
        axes: Option<(Axis, Axis)>,
        keys: &[u8; 96],
        out: &mut Vec<WindowEvent>,
    ) {
        if let (Some(mt), Some(t)) = (&mut self.type_a, &mut self.touch) {
            mt.clear_frame();
            // Type A has no EVIOCGMTSLOTS snapshot. Preserve the primary until
            // the next complete frame, unless BTN_TOUCH proves all fingers up.
            if keys[330 / 8] & (1 << (330 % 8)) == 0 {
                mt.primary = None;
                mt.blocked = false;
                t.down = false;
                self.flush_touch(out);
            }
        } else if let (Some(t), Some((x, y))) = (&mut self.touch, axes) {
            t.x = x;
            t.y = y;
            t.down = keys[330 / 8] & (1 << (330 % 8)) != 0;
            self.flush_touch(out);
        }
        let codes: Vec<_> = self.bindings.keys().copied().collect();
        for code in codes {
            let down = keys[code as usize / 8] & (1 << (code % 8)) != 0;
            self.key(code, i32::from(down), out);
        }
    }
}

fn touch_protocol(bits: &[u8; 8]) -> Result<bool, Error> {
    let has = |code: usize| bits[code / 8] & (1 << (code % 8)) != 0;
    if has(0) && has(1) {
        return Ok(false);
    }
    if !has(47) && has(53) && has(54) && has(57) {
        return Ok(true);
    }
    Err(Error::UnsupportedInput(
        "touch requires legacy axes or Type-A tracking IDs",
    ))
}

pub(crate) struct Evdev {
    pub(crate) file: File,
    pub(crate) decoder: Decoder,
}

impl Evdev {
    pub(crate) fn open(
        device: &InputDevice,
        panel: PhysicalSize,
        rotation: Rotation,
    ) -> Result<Self, Error> {
        if matches!(device, InputDevice::AutoTouchscreen) {
            return discovery::open(panel, rotation);
        }
        let (path, touch, bindings) = match device {
            InputDevice::Touchscreen { path } => (path, true, &[][..]),
            InputDevice::Keys { path, bindings } => (path, false, bindings.as_slice()),
            InputDevice::AutoTouchscreen => unreachable!(),
        };
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
            .map_err(|e| Error::io("open evdev input", e))?;
        let type_a = if touch {
            let mut bits = [0u8; 8];
            // EVIOCGBIT(EV_ABS, 8). Unsupported EVIOCGABS can return a zero
            // range on older drivers, so inspect capabilities before choosing.
            if unsafe {
                libc::ioctl(
                    file.as_raw_fd(),
                    0x8008_4523 as libc::c_ulong,
                    bits.as_mut_ptr(),
                )
            } < 0
            {
                return Err(Error::io(
                    "query evdev absolute capabilities",
                    io::Error::last_os_error(),
                ));
            }
            touch_protocol(&bits)?
        } else {
            false
        };
        let axes = if touch {
            let (x, y) = if type_a { (53, 54) } else { (0, 1) };
            Some((Self::axis(&file, x)?, Self::axis(&file, y)?))
        } else {
            None
        };
        let mut decoder = Decoder::new(panel, rotation, axes, bindings)?;
        if type_a {
            decoder.enable_type_a();
        }
        Ok(Self { file, decoder })
    }

    fn axis(file: &File, code: u8) -> Result<Axis, Error> {
        // _IOR('E', 0x40 + axis, struct input_absinfo), native Linux UAPI.
        let request = 0x8000_0000u32
            | (size_of::<libc::input_absinfo>() as u32) << 16
            | u32::from(b'E') << 8
            | u32::from(0x40 + code);
        let mut value: libc::input_absinfo = unsafe { std::mem::zeroed() };
        // SAFETY: kernel writes the native libc input_absinfo buffer.
        if unsafe { libc::ioctl(file.as_raw_fd(), request as libc::c_ulong, &mut value) } < 0 {
            return Err(Error::io(
                "query evdev absolute axis",
                io::Error::last_os_error(),
            ));
        }
        if value.minimum >= value.maximum {
            return Err(Error::UnsupportedInput("absolute axis has no usable range"));
        }
        Ok(Axis {
            min: value.minimum,
            max: value.maximum,
            value: value.value,
        })
    }

    pub(crate) fn resync(&mut self, out: &mut Vec<WindowEvent>) -> Result<(), Error> {
        let mut keys = [0u8; 96];
        let request = 0x8000_0000u32 | 96 << 16 | u32::from(b'E') << 8 | 0x18;
        // SAFETY: EVIOCGKEY(96) writes at most the supplied bitmap size.
        if unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                request as libc::c_ulong,
                keys.as_mut_ptr(),
            )
        } < 0
        {
            return Err(Error::io(
                "resynchronize evdev keys",
                io::Error::last_os_error(),
            ));
        }
        let axes = if self.decoder.touch.is_some() && self.decoder.type_a.is_none() {
            Some((Self::axis(&self.file, 0)?, Self::axis(&self.file, 1)?))
        } else {
            None
        };
        self.decoder.resync(axes, &keys, out);
        Ok(())
    }

    pub(crate) fn read_ready(&mut self, out: &mut Vec<WindowEvent>) -> Result<(), Error> {
        // Limit each input's work per iteration so timers and rendering stay live.
        for _ in 0..8 {
            let mut events: [libc::input_event; 32] = unsafe { std::mem::zeroed() };
            // SAFETY: events is a fully allocated array of native input_event structs.
            let n = unsafe {
                libc::read(
                    self.file.as_raw_fd(),
                    events.as_mut_ptr().cast(),
                    size_of_val(&events),
                )
            };
            if n < 0 {
                let error = io::Error::last_os_error();
                match error.kind() {
                    io::ErrorKind::WouldBlock => return Ok(()),
                    io::ErrorKind::Interrupted => continue,
                    _ => return Err(Error::io("read evdev input", error)),
                }
            }
            if n == 0 {
                return Err(Error::io(
                    "evdev disconnected",
                    io::ErrorKind::UnexpectedEof.into(),
                ));
            }
            if !(n as usize).is_multiple_of(size_of::<libc::input_event>()) {
                return Err(Error::UnsupportedInput("partial evdev event record"));
            }
            for event in &events[..n as usize / size_of::<libc::input_event>()] {
                if self
                    .decoder
                    .event(event.type_, event.code, event.value, out)
                {
                    self.resync(out)?;
                    // The queried state includes the remainder of this read batch.
                    break;
                }
            }
        }
        Ok(())
    }
}

mod discovery;
