use super::*;
use slint::{
    ComponentHandle, PhysicalSize, Rgba8Pixel, SharedPixelBuffer,
    platform::{
        Platform, WindowAdapter,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    },
};
use std::{rc::Rc, time::Instant};

pub(crate) fn buffer_with_drop_observer(
    width: u32,
    height: u32,
    on_drop: impl FnOnce() + 'static,
) -> Buffer {
    struct Probe<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Accelerator for Probe<F> {
        fn draw(
            &mut self,
            _: &mut [PremultipliedRgbaColor],
            _: &Texture<'_>,
        ) -> Result<Outcome, Failure> {
            Ok(Outcome::Unsupported)
        }
    }
    impl<F: FnOnce()> Drop for Probe<F> {
        fn drop(&mut self) {
            self.0.take().unwrap()();
        }
    }
    let mut buffer = Buffer::software(width, height).unwrap();
    buffer.engine = Some(Box::new(Probe(Some(on_drop))));
    buffer
}

slint::slint! {
    export component Scene inherits Window {
        preferred-width: 512px; preferred-height: 384px;
        in property <color> canvas-color: #19374d;
        background: canvas-color;
        in property <image> picture;
        in property <image> backdrop;
        in property <length> image-x;
        in property <length> image-y;
        in property <length> image-width: 512px;
        in property <length> image-height: 384px;
        in property <float> image-opacity: 1;
        in property <int> crop-x;
        in property <int> crop-y;
        in property <int> crop-width: picture.width;
        in property <int> crop-height: picture.height;
        if backdrop.width > 0: Image { source: backdrop; width: parent.width; height: parent.height; image-fit: fill; }
        Image { x: image-x; y: image-y; width: image-width; height: image-height;
            source: picture; opacity: image-opacity; image-fit: fill;
            source-clip-x: crop-x; source-clip-y: crop-y;
            source-clip-width: crop-width; source-clip-height: crop-height; }
        Rectangle { x: 14px; y: 18px; width: 31px; height: 19px; background: #edab62; }
    }
}
struct TestPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}
fn setup() -> (Rc<MinimalSoftwareWindow>, Scene) {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(TestPlatform(window.clone()))).unwrap();
    let scene = Scene::new().unwrap();
    scene.show().unwrap();
    window.set_size(PhysicalSize::new(512, 384));
    slint::platform::update_timers_and_animations();
    (window, scene)
}
fn image(width: u32, height: u32, alpha: bool, premul: bool) -> slint::Image {
    let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    for (i, p) in pixels.make_mut_slice().iter_mut().enumerate() {
        let a = if alpha {
            [0, 1, 63, 127, 128, 191, 254, 255][(i / 17) % 8]
        } else {
            255
        };
        let c = |value: usize| {
            let v = (value % 256) as u32;
            if premul {
                (v * a as u32 / 255) as u8
            } else {
                v as u8
            }
        };
        *p = Rgba8Pixel {
            r: c(i * 3),
            g: c(i / 3 + 71),
            b: c(i / 11 + 137),
            a,
        };
    }
    if premul {
        slint::Image::from_rgba8_premultiplied(pixels)
    } else {
        slint::Image::from_rgba8(pixels)
    }
}
fn draw(window: &MinimalSoftwareWindow, buffer: &mut Buffer) -> Result<(), Error> {
    window.request_redraw();
    slint::platform::update_timers_and_animations();
    let mut result = None;
    window.draw_if_needed(|renderer| {
        result = Some(buffer.render(renderer).map(|_| ()));
    });
    result.expect("render requested")
}
fn difference(a: &[PremultipliedRgbaColor], b: &[PremultipliedRgbaColor]) -> (u8, usize) {
    let mut maximum = 0;
    let mut count = 0;
    for (a, b) in a.iter().zip(b) {
        for (a, b) in [a.red, a.green, a.blue, a.alpha]
            .into_iter()
            .zip([b.red, b.green, b.blue, b.alpha])
        {
            let delta = a.abs_diff(b);
            maximum = maximum.max(delta);
            count += usize::from(delta != 0);
        }
    }
    (maximum, count)
}

fn moving_scene(scene: &Scene) {
    scene.set_canvas_color(slint::Color::from_rgb_u8(25, 55, 77));
    scene.set_backdrop(image(512, 384, false, false));
    scene.set_picture(image(256, 256, true, false));
    scene.set_crop_x(0);
    scene.set_crop_y(0);
    scene.set_crop_width(256);
    scene.set_crop_height(256);
    scene.set_image_width(256.);
    scene.set_image_height(256.);
    scene.set_image_y(64.);
    scene.set_image_opacity(1.);
}

fn compare_incremental(window: &MinimalSoftwareWindow, scene: &Scene, hardware: &mut Buffer) {
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let reference = std::thread::spawn(move || {
        let (window, scene) = setup();
        moving_scene(&scene);
        let mut software = Buffer::software(512, 384).unwrap();
        for frame in 0..80 {
            scene.set_image_x(((frame % 24) * 12) as f32);
            draw(&window, &mut software).unwrap();
            if send.send(software.pixels().to_vec()).is_err() {
                break;
            }
        }
    });
    moving_scene(scene);
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer)
    });
    let mut max_error = 0;
    for frame in 0..80 {
        scene.set_image_x(((frame % 24) * 12) as f32);
        draw(window, hardware).unwrap();
        let software = receive.recv().unwrap();
        let (error, differing) = difference(hardware.pixels(), &software);
        max_error = max_error.max(error);
        if error > 1
            && let Ok(path) = std::env::var("SLINT_BRIDGE_TEST_OUTPUT")
        {
            for (name, pixels) in [
                ("incremental-hw.rgba", hardware.pixels()),
                ("incremental-sw.rgba", software.as_slice()),
            ] {
                let bytes: Vec<u8> = pixels
                    .iter()
                    .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
                    .collect();
                std::fs::write(std::path::Path::new(&path).join(name), bytes).unwrap();
            }
        }
        assert!(
            error <= 1,
            "incremental frame={frame} max_error={error} differing={differing}"
        );
    }
    reference.join().unwrap();
    println!("correctness name=incremental-motion frames=80 max_channel_error={max_error}");
    window.request_redraw();
    window
        .draw_if_needed(|renderer| renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer));
    scene.set_backdrop(slint::Image::default());
}

#[test]
fn incremental_motion_matches_full_software_frames() {
    let (window, scene) = setup();
    let mut buffer = Buffer::software(512, 384).unwrap();
    compare_incremental(&window, &scene, &mut buffer);
}

struct Reject {
    unknown: bool,
}
impl Accelerator for Reject {
    fn draw(
        &mut self,
        _: &mut [PremultipliedRgbaColor],
        texture: &Texture<'_>,
    ) -> Result<Outcome, Failure> {
        assert!(!texture.data.is_empty() && !texture.rects.is_empty());
        Err(Failure {
            code: -77,
            unknown: self.unknown,
        })
    }
}
pub(crate) fn buffer_after_unknown_dma() -> Buffer {
    let (window, scene) = setup();
    scene.set_picture(image(512, 384, false, false));
    let mut buffer = Buffer::software(512, 384).unwrap();
    buffer.engine = Some(Box::new(Reject { unknown: true }));
    assert!(matches!(
        draw(&window, &mut buffer),
        Err(Error::AccelerationFailed(_))
    ));
    buffer
}

#[test]
fn safe_failure_redraws_in_software_unknown_completion_rejects_frame() {
    let (window, scene) = setup();
    scene.set_picture(image(512, 384, true, false));
    let mut reference = Buffer::software(512, 384).unwrap();
    draw(&window, &mut reference).unwrap();
    let mut buffer = Buffer::software(512, 384).unwrap();
    buffer.engine = Some(Box::new(Reject { unknown: false }));
    draw(&window, &mut buffer).unwrap();
    assert_eq!(difference(buffer.pixels(), reference.pixels()), (0, 0));
    assert!(buffer.engine.is_none());
    assert_eq!(buffer.stats.errors, 1);
    assert!(buffer.stats.software > 0);
    draw(&window, &mut buffer).unwrap();
    assert_eq!(buffer.stats.errors, 1);
    buffer.engine = Some(Box::new(Reject { unknown: true }));
    assert!(matches!(
        draw(&window, &mut buffer),
        Err(Error::AccelerationFailed(_))
    ));
    assert!(matches!(
        draw(&window, &mut buffer),
        Err(Error::AccelerationFailed(_))
    ));
}

#[test]
fn clipping_preserves_source_origin_and_empty_intersections() {
    assert_eq!(
        clipped((-8, -2, 512, 384), (0, 0, 512, 384), (512, 384)),
        Some(Rect {
            sx: 8,
            sy: 2,
            dx: 0,
            dy: 0,
            width: 504,
            height: 382
        })
    );
    assert_eq!(
        clipped((4, 6, 512, 384), (10, 12, 128, 64), (512, 384)),
        Some(Rect {
            sx: 6,
            sy: 6,
            dx: 10,
            dy: 12,
            width: 128,
            height: 64
        })
    );
    assert_eq!(clipped((0, 0, 4, 4), (8, 8, 4, 4), (512, 384)), None);
}

// This exact test binary runs on NX5 after the previous UI has exited. It owns
// one reviewed VO session; native TDE can only reuse that already loaded SDK.
#[test]
#[ignore = "requires exclusive current NX5; run with --ignored --nocapture"]
fn nx5_hardware_compare() {
    let _display = crate::backends::nx5::open().unwrap();
    let (window, scene) = setup();
    let mut hardware = Buffer::new(512, 384).unwrap();
    assert!(hardware.engine.is_some(), "TDE must actually be active");
    let mut software = Buffer::software(512, 384).unwrap();
    for (name, alpha, premul, x, y, w, h, opacity, accelerated) in [
        ("opaque", false, false, 0., 0., 512., 384., 1., true),
        ("straight-alpha", true, false, 0., 0., 512., 384., 1., true),
        (
            "premultiplied-alpha",
            true,
            true,
            0.,
            0.,
            512.,
            384.,
            1.,
            true,
        ),
        ("negative-clip", true, true, -8., -2., 512., 384., 1., true),
        (
            "odd-clip-fallback",
            true,
            true,
            1.,
            1.,
            512.,
            384.,
            1.,
            false,
        ),
        (
            "opacity-fallback",
            true,
            true,
            0.,
            0.,
            512.,
            384.,
            0.5,
            false,
        ),
        (
            "scale-fallback",
            false,
            false,
            0.,
            0.,
            256.,
            192.,
            1.,
            false,
        ),
    ] {
        scene.set_picture(image(512, 384, alpha, premul));
        scene.set_image_x(x);
        scene.set_image_y(y);
        scene.set_image_width(w);
        scene.set_image_height(h);
        scene.set_image_opacity(opacity);
        let before = hardware.stats;
        draw(&window, &mut hardware).unwrap();
        draw(&window, &mut software).unwrap();
        let (maximum, differing) = difference(hardware.pixels(), software.pixels());
        let submitted =
            hardware.stats.copied + hardware.stats.blended - before.copied - before.blended;
        println!(
            "correctness name={name} max_channel_error={maximum} differing_channels={differing} submitted={submitted}"
        );
        assert!(maximum <= 1, "{name}: mismatch {maximum}");
        assert_eq!(submitted > 0, accelerated, "{name}: dispatch");
        for _ in 0..3 {
            draw(&window, &mut hardware).unwrap();
        }
        assert!(difference(hardware.pixels(), software.pixels()).0 <= 1);
        if accelerated {
            assert!(hardware.stats.cache_hits > before.cache_hits);
        }
    }
    scene.set_image_x(0.);
    scene.set_image_y(0.);
    scene.set_image_width(512.);
    scene.set_image_height(384.);
    scene.set_image_opacity(1.);
    for (name, alpha, premul) in [
        ("opaque", false, false),
        ("straight-alpha", true, false),
        ("premultiplied-alpha", true, true),
    ] {
        scene.set_picture(image(512, 384, alpha, premul));
        for _ in 0..5 {
            draw(&window, &mut hardware).unwrap();
            draw(&window, &mut software).unwrap();
        }
        let start = Instant::now();
        for _ in 0..60 {
            draw(&window, &mut hardware).unwrap();
        }
        let hw = start.elapsed().as_secs_f64() / 60.;
        let start = Instant::now();
        for _ in 0..60 {
            draw(&window, &mut software).unwrap();
        }
        let sw = start.elapsed().as_secs_f64() / 60.;
        println!(
            "benchmark name={name} frames=60 hardware_ms={:.3} software_ms={:.3} speedup={:.3}",
            hw * 1000.,
            sw * 1000.,
            sw / hw
        );
    }
    // A cropped source view has a larger byte stride than the uploaded width.
    scene.set_picture(image(528, 388, true, true));
    scene.set_crop_x(8);
    scene.set_crop_y(2);
    scene.set_crop_width(512);
    scene.set_crop_height(384);
    let before = hardware.stats.blended;
    draw(&window, &mut hardware).unwrap();
    draw(&window, &mut software).unwrap();
    let delta = difference(hardware.pixels(), software.pixels());
    assert!(delta.0 <= 1);
    assert!(hardware.stats.blended > before);
    println!(
        "correctness name=strided-source max_channel_error={} differing_channels={}",
        delta.0, delta.1
    );
    scene.set_crop_x(0);
    scene.set_crop_y(0);

    // Retained keys must keep the old image alive so copy-on-write invalidates a cache slot.
    let mut original = SharedPixelBuffer::<Rgba8Pixel>::new(512, 384);
    original.make_mut_slice().fill(Rgba8Pixel {
        r: 17,
        g: 83,
        b: 197,
        a: 255,
    });
    let mut changed = original.clone();
    scene.set_picture(slint::Image::from_rgba8(original));
    draw(&window, &mut hardware).unwrap();
    let hits = hardware.stats.cache_hits;
    changed.make_mut_slice()[0] = Rgba8Pixel {
        r: 212,
        g: 37,
        b: 54,
        a: 255,
    };
    scene.set_picture(slint::Image::from_rgba8(changed));
    draw(&window, &mut hardware).unwrap();
    draw(&window, &mut software).unwrap();
    assert_eq!(difference(hardware.pixels(), software.pixels()), (0, 0));
    assert_eq!(hardware.stats.cache_hits, hits);
    draw(&window, &mut hardware).unwrap();
    assert_eq!(hardware.stats.cache_hits, hits + 1);
    println!("correctness name=copy-on-write-cache max_channel_error=0");

    scene.set_picture(image(64, 64, true, false));
    scene.set_crop_width(64);
    scene.set_crop_height(64);
    scene.set_image_width(64.);
    scene.set_image_height(64.);
    let submitted = hardware.stats.copied + hardware.stats.blended;
    draw(&window, &mut hardware).unwrap();
    draw(&window, &mut software).unwrap();
    assert_eq!(difference(hardware.pixels(), software.pixels()), (0, 0));
    assert_eq!(hardware.stats.copied + hardware.stats.blended, submitted);
    println!("fallback name=small-image exact=true");
    scene.set_picture(image(512, 384, true, true));
    scene.set_crop_width(512);
    scene.set_crop_height(384);
    scene.set_image_width(512.);
    scene.set_image_height(384.);

    let submitted = hardware.stats.copied + hardware.stats.blended;
    window.request_redraw();
    window.draw_if_needed(|renderer| renderer.set_rendering_rotation(RenderingRotation::Rotate180));
    draw(&window, &mut hardware).unwrap();
    draw(&window, &mut software).unwrap();
    assert_eq!(difference(hardware.pixels(), software.pixels()), (0, 0));
    assert_eq!(hardware.stats.copied + hardware.stats.blended, submitted);
    println!("fallback name=rotation-180 exact=true");
    window.request_redraw();
    window
        .draw_if_needed(|renderer| renderer.set_rendering_rotation(RenderingRotation::NoRotation));
    scene.set_canvas_color(slint::Color::from_argb_u8(0, 0, 0, 0));
    draw(&window, &mut hardware).unwrap();
    draw(&window, &mut software).unwrap();
    assert_eq!(difference(hardware.pixels(), software.pixels()), (0, 0));
    assert_eq!(hardware.stats.copied + hardware.stats.blended, submitted);
    println!("fallback name=transparent-destination exact=true");
    scene.set_canvas_color(slint::Color::from_rgb_u8(25, 55, 77));

    // Exercise a recoverable driver error through the real Slint callback/fallback path.
    compare_incremental(&window, &scene, &mut hardware);
    scene.set_picture(image(512, 384, true, true));
    scene.set_image_x(0.);
    scene.set_image_y(0.);
    scene.set_crop_width(512);
    scene.set_crop_height(384);
    scene.set_image_width(512.);
    scene.set_image_height(384.);
    hardware.engine = Some(Box::new(Reject { unknown: false }));
    draw(&window, &mut hardware).unwrap();
    draw(&window, &mut software).unwrap();
    assert_eq!(difference(hardware.pixels(), software.pixels()), (0, 0));
    println!(
        "fallback injected_pre_submit_error=pass stats={:?}",
        hardware.stats
    );
}

mod actual_demo {
    slint::slint! { export { App } from "../../../../test-app/ui/display.slint"; }
    mod textures {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/test-app/fixtures/textures.rs"
        ));
    }
    pub use textures::configure_textures;
}

fn setup_demo() -> (Rc<MinimalSoftwareWindow>, actual_demo::App) {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(TestPlatform(window.clone()))).unwrap();
    let app = actual_demo::App::new().unwrap();
    app.show().unwrap();
    window.set_size(PhysicalSize::new(1024, 600));
    actual_demo::configure_textures(&app);
    slint::platform::update_timers_and_animations();
    (window, app)
}
fn demo_frame(app: &actual_demo::App, frame: i32) {
    app.set_frame(frame);
    app.set_bar_x((frame * 12) as f32 % app.get_bar_range().max(1.0));
}

// Uses the actual demo UI and the production double-page writer, comparing every
// presented pixel against independently rendered full software reference frames.
#[test]
#[ignore = "requires exclusive current NX5; run alone with --ignored --nocapture"]
fn nx5_demo_pages_match_full_frames() {
    use crate::framebuffer::{Bitfield, FixedInfo, Framebuffer, Layout, VariableInfo};
    use std::{
        fs::File,
        os::{fd::FromRawFd, unix::fs::FileExt},
    };
    let display = crate::backends::nx5::open().unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let reference = std::thread::spawn(move || {
        let (window, app) = setup_demo();
        let mut software = Buffer::software(1024, 600).unwrap();
        for frame in 0..100 {
            demo_frame(&app, frame);
            draw(&window, &mut software).unwrap();
            if send.send(software.pixels().to_vec()).is_err() {
                break;
            }
        }
    });
    let (window, app) = setup_demo();
    window.request_redraw();
    window.draw_if_needed(|r| r.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer));
    let fd = unsafe {
        libc::syscall(
            libc::SYS_memfd_create,
            c"tde-pages-test".as_ptr(),
            libc::MFD_CLOEXEC,
        )
    } as i32;
    assert!(fd >= 0);
    let file = unsafe { File::from_raw_fd(fd) };
    file.set_len(1024 * 600 * 4 * 2).unwrap();
    let reader = file.try_clone().unwrap();
    let channel = |offset| Bitfield {
        offset,
        length: 8,
        msb_right: 0,
    };
    let fixed = FixedInfo {
        visual: 2,
        line_length: 4096,
        smem_len: 1024 * 600 * 4 * 2,
        ..Default::default()
    };
    let variable = VariableInfo {
        xres: 1024,
        yres: 600,
        xres_virtual: 1024,
        yres_virtual: 1200,
        bits_per_pixel: 32,
        red: channel(16),
        green: channel(8),
        blue: channel(0),
        alpha: channel(24),
        ..Default::default()
    };
    let mut count = 0;
    let mut last = 0;
    let contract = crate::backends::PresentationContract {
        validate_layout: crate::framebuffer::Layout::new,
        complete: Box::new(move |_, offset, _| {
            assert_ne!(offset, last);
            last = offset;
            let expected = receive.recv().unwrap();
            let mut bytes = vec![0u8; 1024 * 600 * 4];
            reader
                .read_exact_at(&mut bytes, u64::from(offset) * 4096)
                .unwrap();
            let mut maximum = 0;
            let mut different = 0;
            for (pixel, e) in bytes.as_chunks::<4>().0.iter().zip(&expected) {
                for (a, b) in pixel.iter().copied().zip([e.blue, e.green, e.red, 255]) {
                    let error = a.abs_diff(b);
                    maximum = maximum.max(error);
                    different += usize::from(error > 1);
                }
            }
            println!(
                "demo_frame={count} yoffset={offset} max_error={maximum} bad_channels={different}"
            );
            if maximum > 1
                && let Ok(path) = std::env::var("SLINT_BRIDGE_TEST_OUTPUT")
            {
                std::fs::write(std::path::Path::new(&path).join("demo-page.bgra"), &bytes).unwrap();
                let pixels: Vec<u8> = expected
                    .iter()
                    .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
                    .collect();
                std::fs::write(
                    std::path::Path::new(&path).join("demo-reference.rgba"),
                    pixels,
                )
                .unwrap();
            }
            assert!(maximum <= 1, "demo page mismatch on frame {count}");
            count += 1;
            Ok(())
        }),
    };
    let mut framebuffer =
        Framebuffer::from_file(file, Layout::new(&fixed, &variable).unwrap(), contract).unwrap();
    for frame in 0..100 {
        demo_frame(&app, frame);
        window.request_redraw();
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            assert!(framebuffer.render(renderer).unwrap());
            assert!(framebuffer.present().unwrap());
        });
    }
    framebuffer.wait_for_present().unwrap();
    reference.join().unwrap();
    drop(framebuffer);
    drop(display);
}
