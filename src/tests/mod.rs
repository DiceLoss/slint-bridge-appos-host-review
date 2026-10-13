use super::*;
use crate::{
    backends::PresentationContract,
    framebuffer::{Bitfield, FixedInfo, Framebuffer, Layout, VariableInfo},
    input::{Axis, Decoder, Evdev},
    wake::{Proxy, WakeQueue},
};
use slint::{
    ComponentHandle, PhysicalSize,
    platform::{
        EventLoopProxy, Platform, WindowAdapter, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    },
};
use std::{
    fs::File,
    io::Write,
    os::{fd::FromRawFd, unix::fs::FileExt},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

// Slint's public global event-loop proxy is process-wide. Keep its real event-loop
// test separate from direct-render tests that create thread-local platforms.
static SLINT_TEST: Mutex<()> = Mutex::new(());

pub(crate) fn info(width: u32, height: u32, bits: u32) -> (FixedInfo, VariableInfo) {
    let stride = width * (bits / 8) + 8; // Exercise padded rows, not just packed images.
    let field = |offset, length| Bitfield {
        offset,
        length,
        msb_right: 0,
    };
    let (red, green, blue, alpha) = if bits == 16 {
        (field(11, 5), field(5, 6), field(0, 5), field(0, 0))
    } else {
        (field(16, 8), field(8, 8), field(0, 8), field(24, 8))
    };
    (
        FixedInfo {
            visual: 2,
            line_length: stride,
            smem_len: stride * height * 2,
            ..Default::default()
        },
        VariableInfo {
            xres: width,
            yres: height,
            xres_virtual: width,
            yres_virtual: height * 2,
            bits_per_pixel: bits,
            red,
            green,
            blue,
            alpha,
            ..Default::default()
        },
    )
}

struct Surface {
    fb: Framebuffer,
    file: File,
    commits: Arc<Mutex<Vec<Vec<u8>>>>,
    fail: Arc<AtomicBool>,
}

fn surface(width: u32, height: u32, bits: u32) -> Surface {
    surface_with_completion(width, height, bits, 0, |_| Ok(()))
}

fn surface_with_completion(
    width: u32,
    height: u32,
    bits: u32,
    initial_front: usize,
    mut complete: impl FnMut(u32) -> std::io::Result<()> + Send + 'static,
) -> Surface {
    // The direct syscall keeps this helper compatible with the old ARM libc too.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_memfd_create,
            c"slint-bridge-test".as_ptr(),
            libc::MFD_CLOEXEC,
        )
    } as i32;
    assert!(fd >= 0);
    let mut file = unsafe { File::from_raw_fd(fd) };
    let (f, mut v) = info(width, height, bits);
    v.yoffset = initial_front as u32 * height;
    let layout = Layout::new(&f, &v).unwrap();
    file.set_len(f.smem_len as u64).unwrap();
    file.write_all(&vec![0xa5; f.smem_len as usize]).unwrap();
    let observer = file.try_clone().unwrap();
    let commits = Arc::new(Mutex::new(Vec::new()));
    let saved = commits.clone();
    let fail = Arc::new(AtomicBool::new(false));
    let failure = fail.clone();
    let mut front = layout.initial_page;
    let mut previous = vec![0xa5; layout.page_bytes];
    let contract = PresentationContract {
        validate_layout: crate::framebuffer::Layout::new,
        complete: Box::new(move |_, yoffset, reported_layout| {
            assert_eq!(reported_layout.size, layout.size);
            assert_eq!(reported_layout.stride, layout.stride);
            let next = (yoffset / height) as usize;
            assert_eq!(next, 1 - front, "must only prepare the inactive page");
            let mut bytes = vec![0; layout.page_bytes * 2];
            observer.read_exact_at(&mut bytes, 0).unwrap();
            assert_eq!(
                &bytes[front * layout.page_bytes..(front + 1) * layout.page_bytes],
                &previous,
                "preparing a frame modified the active scanout page"
            );
            complete(yoffset)?;
            let mut after = vec![0; bytes.len()];
            observer.read_exact_at(&mut after, 0).unwrap();
            assert_eq!(
                bytes, after,
                "display pages changed while submission was pending"
            );
            if failure.load(Ordering::SeqCst) {
                return Err(std::io::Error::other("injected unknown retirement"));
            }
            previous = bytes[next * layout.page_bytes..(next + 1) * layout.page_bytes].to_vec();
            saved.lock().unwrap().push(previous.clone());
            front = next;
            Ok(())
        }),
    };
    Surface {
        fb: Framebuffer::from_file(file.try_clone().unwrap(), layout, contract).unwrap(),
        file,
        commits,
        fail,
    }
}

slint::slint! {
    export component Pixels inherits Window {
        preferred-width: 8px; preferred-height: 6px;
        in-out property<color> left-color: red;
        in-out property<color> right-color: blue;
        out property<length> actual-width: root.width;
        out property<length> actual-height: root.height;
        background: root.right-color;
        Rectangle { x: 0px; y: 0px; width: parent.width / 2; height: parent.height; background: root.left-color; }
    }
    export component LoopScene inherits Window {
        preferred-width: 8px; preferred-height: 6px;
        in-out property<int> taps: 0;
        callback tapped();
        in-out property<int> keys: 0;
        in-out property<color> paint: red;
        in-out property<float> overlay-opacity: 0;
        in-out property<length> required-width: 0px;
        min-width: root.required-width;
        animate overlay-opacity { duration: 100ms; }
        background: root.paint;
        Rectangle { background: white; opacity: root.overlay-opacity; }
        TouchArea { clicked => { root.taps += 1; root.tapped(); } }
        key-focus := FocusScope {
            key-pressed(event) => { root.keys += 1; accept }
        }
        public function focus-keys() { key-focus.focus(); }
    }
    export component Glyphs inherits Window {
        preferred-width: 64px; preferred-height: 32px;
        background: black;
        Text { x: 0px; y: 0px; text: "Test"; color: white; font-size: 20px; }
    }
}

struct RenderPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for RenderPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn draw(window: &MinimalSoftwareWindow, fb: &mut Framebuffer) -> Result<bool, Error> {
    let changed = prepare(window, fb)?;
    fb.present(None)?;
    fb.wait_for_present()?;
    Ok(changed)
}

fn prepare(window: &MinimalSoftwareWindow, fb: &mut Framebuffer) -> Result<bool, Error> {
    window.request_redraw();
    let mut result = None;
    window.draw_if_needed(|renderer| result = Some(fb.render(renderer)));
    result.unwrap()
}

fn input_pipe() -> (File, File) {
    let mut fds = [-1; 2];
    assert_eq!(
        unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) },
        0
    );
    unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) }
}

mod admission;
mod framebuffer;
mod input;
mod runtime;
mod wake;
