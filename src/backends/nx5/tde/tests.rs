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

#[test]
fn changing_foreground_keeps_recent_background_texture_cached() {
    use std::cell::RefCell;

    #[derive(Default)]
    struct Uploads {
        background: usize,
        foreground: usize,
        bytes: usize,
        slots: [Vec<u8>; 2],
    }
    struct CacheProbe(Rc<RefCell<Uploads>>);
    impl Accelerator for CacheProbe {
        fn draw(
            &mut self,
            _: &mut [PremultipliedRgbaColor],
            texture: &Texture<'_>,
        ) -> Result<Outcome, Failure> {
            let mut uploads = self.0.borrow_mut();
            let slot = texture.slot as usize;
            if texture.upload {
                if texture.width == 512 {
                    uploads.background += 1;
                } else {
                    uploads.foreground += 1;
                }
                uploads.bytes += texture.data.len();
                uploads.slots[slot] = texture.data.to_vec();
            } else {
                assert_eq!(
                    uploads.slots[slot], texture.data,
                    "cache hit reused stale pixels"
                );
            }
            Ok(if texture.width == 512 {
                Outcome::Copied
            } else {
                Outcome::Blended
            })
        }
    }

    let (window, scene) = setup();
    moving_scene(&scene);
    let uploads = Rc::new(RefCell::new(Uploads::default()));
    let mut buffer = Buffer::software(512, 384).unwrap();
    buffer.engine = Some(Box::new(CacheProbe(uploads.clone())));
    for frame in 0..8 {
        let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(256, 256);
        pixels.make_mut_slice().fill(Rgba8Pixel {
            r: frame * 20,
            g: 40,
            b: 80,
            a: 128,
        });
        scene.set_picture(slint::Image::from_rgba8(pixels));
        draw(&window, &mut buffer).unwrap();
    }
    let uploads = uploads.borrow();
    eprintln!(
        "texture uploads: background={} foreground={} bytes={}",
        uploads.background, uploads.foreground, uploads.bytes
    );
    assert_eq!(
        uploads.background, 1,
        "recently used background was evicted"
    );
    assert_eq!(uploads.foreground, 8, "each changed foreground must upload");
    assert_eq!(uploads.bytes, 512 * 384 * 4 + 8 * 256 * 256 * 4);
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
    hardware.fill_enabled = true;
    let mut software = Buffer::software(512, 384).unwrap();
    for alpha in [0, 1, 127, 255] {
        scene.set_canvas_color(slint::Color::from_argb_u8(alpha, 211, 97, 39));
        let before = hardware.stats.filled;
        let dispatch = draw_audited(&window, &mut hardware);
        assert!(dispatch.accepted[0] > 0);
        assert_eq!(dispatch.rows_after_accepted, [0; 3]);
        println!("dispatch fill_alpha={alpha} counts={dispatch:?}");
        draw(&window, &mut software).unwrap();
        assert_eq!(difference(hardware.pixels(), software.pixels()), (0, 0));
        assert!(hardware.stats.filled > before);
        println!("correctness name=fill alpha={alpha} exact=true");
    }
    let rect = Rect {
        sx: 0,
        sy: 0,
        dx: 3,
        dy: 5,
        width: 301,
        height: 201,
    };
    let color: PremultipliedRgbaColor = slint::Color::from_rgb_u8(213, 93, 37).into();
    assert!(hardware.fill(color, &[rect]));
    for y in 5..206 {
        software.pixels.cpu_mut()[y * 512 + 3..y * 512 + 304].fill(color);
    }
    assert_eq!(difference(hardware.pixels(), software.pixels()), (0, 0));
    println!("correctness name=fill-odd-rectangle exact=true guards=true");
    {
        let name = "fill-full";
        for (label, buffer) in [("hardware", &mut hardware), ("software", &mut software)] {
            let start = Instant::now();
            for _ in 0..100 {
                draw(&window, buffer).unwrap();
            }
            println!(
                "benchmark name={name} path={label} frames=100 mean_ms={:.3}",
                start.elapsed().as_secs_f64() * 10.
            );
        }
    }
    hardware.fill_enabled = false;
    scene.set_canvas_color(slint::Color::from_rgb_u8(25, 55, 77));
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
        ("global-opacity", true, true, 0., 0., 512., 384., 0.5, true),
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
        let dispatch = draw_audited(&window, &mut hardware);
        assert_eq!(dispatch.rows_after_accepted, [0; 3]);
        assert_eq!(dispatch.accepted[2] > 0, accelerated);
        assert_eq!(dispatch.rows_after_fallback[2] > 0, !accelerated);
        println!("dispatch name={name} counts={dispatch:?}");
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
    for (name, alpha, premul) in [
        ("opaque-opacity", false, false),
        ("straight-opacity", true, false),
        ("premultiplied-opacity", true, true),
    ] {
        scene.set_picture(image(512, 384, alpha, premul));
        for opacity in [1. / 255., 63. / 255., 0.5, 0.5, 254. / 255., 1.] {
            scene.set_image_opacity(opacity);
            let before = hardware.stats;
            draw(&window, &mut hardware).unwrap();
            draw(&window, &mut software).unwrap();
            let (maximum, different) = difference(hardware.pixels(), software.pixels());
            println!(
                "correctness name={name} opacity={opacity} max_channel_error={maximum} differing_channels={different}"
            );
            assert!(maximum <= 1, "{name} at {opacity}");
            // Slint culls images at opacity <= 0.01 before the callback.
            assert_eq!(
                hardware.stats.blended + hardware.stats.copied > before.blended + before.copied,
                opacity > 0.01
            );
        }
        scene.set_image_opacity(0.5);
        draw(&window, &mut hardware).unwrap();
        for (path, buffer) in [("hardware", &mut hardware), ("software", &mut software)] {
            let start = Instant::now();
            for _ in 0..60 {
                draw(&window, buffer).unwrap();
            }
            println!(
                "benchmark name={name} path={path} frames=60 mean_ms={:.3}",
                start.elapsed().as_secs_f64() * 1000. / 60.
            );
        }
    }
    scene.set_image_opacity(1.);
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

#[test]
fn image_opacity_invalidates_prepared_source_cache() {
    struct Probe;
    impl Accelerator for Probe {
        fn draw(
            &mut self,
            _: &mut [PremultipliedRgbaColor],
            _: &Texture<'_>,
        ) -> Result<Outcome, Failure> {
            Ok(Outcome::Blended)
        }
    }
    let (window, scene) = setup();
    scene.set_picture(image(512, 384, true, true));
    let mut buffer = Buffer::software(512, 384).unwrap();
    buffer.engine = Some(Box::new(Probe));
    for opacity in [1.0, 0.5, 0.5, 0.25, 0.25, 1.0] {
        scene.set_image_opacity(opacity);
        draw(&window, &mut buffer).unwrap();
    }
    assert_eq!(buffer.stats.blended, 6);
    assert_eq!(buffer.stats.cache_hits, 2);
}

#[test]
fn solid_fill_clipping_and_software_edges_match() {
    struct FillProbe;
    impl Accelerator for FillProbe {
        fn fill(
            &mut self,
            pixels: &mut [PremultipliedRgbaColor],
            color: PremultipliedRgbaColor,
            rects: &[Rect],
        ) -> Result<bool, Failure> {
            for r in rects {
                for y in r.dy..r.dy + r.height as i32 {
                    let offset = y as usize * 512 + r.dx as usize;
                    pixels[offset..offset + r.width as usize].fill(color);
                }
            }
            Ok(true)
        }
        fn draw(
            &mut self,
            _: &mut [PremultipliedRgbaColor],
            _: &Texture<'_>,
        ) -> Result<Outcome, Failure> {
            Ok(Outcome::Unsupported)
        }
    }
    let (window, scene) = setup();
    let mut hardware = Buffer::software(512, 384).unwrap();
    hardware.engine = Some(Box::new(FillProbe));
    hardware.fill_enabled = true;
    let mut software = Buffer::software(512, 384).unwrap();
    for alpha in [0, 1, 127, 255] {
        scene.set_canvas_color(slint::Color::from_argb_u8(alpha, 211, 97, 39));
        draw(&window, &mut hardware).unwrap();
        draw(&window, &mut software).unwrap();
        assert_eq!(difference(hardware.pixels(), software.pixels()), (0, 0));
    }
    assert_eq!(hardware.stats.filled, 4);
    let mut args = DrawRectangleArgs::default();
    args.x = -17.;
    args.y = 3.;
    args.width = 300.;
    args.height = 220.;
    args.alpha = 255;
    args.background = slint::Color::from_rgb_u8(217, 107, 29).into();
    // Use the renderer's full dirty region without constructing private Slint fields.
    let mut region = None;
    window.request_redraw();
    window.draw_if_needed(|r| region = Some(r.render_into_buffer(&mut software)));
    let region = region.unwrap();
    assert!(hardware.draw_rectangle(&args, &region));
    assert_eq!(hardware.pixels()[3 * 512].red, 217);
    assert_eq!(hardware.pixels()[3 * 512 + 282].red, 217);
    assert_ne!(hardware.pixels()[3 * 512 + 283].red, 217);
    args.x = 0.5;
    assert!(!hardware.draw_rectangle(&args, &region));
    args.x = 0.;
    args.top_left_radius = 1.;
    assert!(!hardware.draw_rectangle(&args, &region));
    args.top_left_radius = 0.;
    args.alpha = 127;
    assert!(!hardware.draw_rectangle(&args, &region));
    args.alpha = 255;
    args.border_width = 1.;
    assert!(!hardware.draw_rectangle(&args, &region));
}

#[test]
fn fill_failure_preserves_fallback_and_unknown_dma_frame_rejection() {
    struct FillFailure(bool);
    impl Accelerator for FillFailure {
        fn fill(
            &mut self,
            _: &mut [PremultipliedRgbaColor],
            _: PremultipliedRgbaColor,
            _: &[Rect],
        ) -> Result<bool, Failure> {
            Err(Failure {
                code: -78,
                unknown: self.0,
            })
        }
        fn draw(
            &mut self,
            _: &mut [PremultipliedRgbaColor],
            _: &Texture<'_>,
        ) -> Result<Outcome, Failure> {
            unreachable!("fill failure disables the accelerator before image drawing")
        }
    }
    let (window, scene) = setup();
    scene.set_picture(image(512, 384, true, true));
    let mut reference = Buffer::software(512, 384).unwrap();
    draw(&window, &mut reference).unwrap();
    for unknown in [false, true] {
        let mut buffer = Buffer::software(512, 384).unwrap();
        buffer.fill_enabled = true;
        buffer.engine = Some(Box::new(FillFailure(unknown)));
        let result = draw(&window, &mut buffer);
        assert_eq!(result.is_err(), unknown);
        assert_eq!(buffer.has_unknown_dma(), unknown);
        assert!(buffer.engine.is_none());
        assert_eq!(buffer.stats.errors, 1);
        if unknown {
            assert!(matches!(
                draw(&window, &mut buffer),
                Err(Error::AccelerationFailed(_))
            ));
        } else {
            assert_eq!(difference(buffer.pixels(), reference.pixels()), (0, 0));
        }
    }
}

#[test]
#[ignore = "requires exclusive NX5; measures fill area/shape crossover without framebuffer writes"]
fn nx5_fill_area_crossover() {
    use std::hint::black_box;
    let _display = crate::backends::nx5::open().unwrap();
    let mut hardware = Buffer::new(1024, 600).unwrap();
    let mut software = Buffer::software(1024, 600).unwrap();
    assert!(hardware.engine.is_some(), "TDE must be active");
    let summarize = |mut samples: Vec<f64>| {
        let average = samples.iter().sum::<f64>() / samples.len() as f64;
        samples.sort_by(f64::total_cmp);
        (
            average,
            samples[samples.len() / 2],
            samples[samples.len() * 95 / 100],
        )
    };
    for (name, width, height, split) in [
        ("tiny", 16, 16, false),
        ("small", 64, 64, false),
        ("square-16k", 128, 128, false),
        ("wide-32k", 1024, 32, false),
        ("tall-32k", 128, 256, false),
        ("square-64k", 256, 256, false),
        ("split-64k", 1024, 32, true),
        ("wide-128k", 1024, 128, false),
        ("half-196k", 512, 384, false),
        ("half-screen", 1024, 300, false),
        ("full-screen", 1024, 600, false),
    ] {
        let mut rects = vec![Rect {
            sx: 0,
            sy: 0,
            dx: 0,
            dy: 0,
            width,
            height,
        }];
        if split {
            rects.push(Rect {
                dy: 568,
                ..rects[0]
            });
        }
        for (color_name, color) in [
            ("clear", PremultipliedRgbaColor::default()),
            ("opaque", slint::Color::from_rgb_u8(25, 55, 77).into()),
        ] {
            let mut timings = [Vec::new(), Vec::new(), Vec::new()];
            for iteration in 0..70 {
                // Rotate hardware, heap CPU and MMZ CPU order; discard ten
                // warm-up rounds. TDE includes wait and cache ownership transfer.
                for path in [iteration % 3, (iteration + 1) % 3, (iteration + 2) % 3] {
                    let started = Instant::now();
                    if path == 0 {
                        assert!(
                            hardware
                                .engine
                                .as_mut()
                                .unwrap()
                                .fill(
                                    black_box(hardware.pixels.accelerator_buffer()),
                                    black_box(color),
                                    black_box(&rects)
                                )
                                .unwrap()
                        );
                    } else {
                        let pixels = black_box(if path == 1 {
                            software.pixels.cpu_mut()
                        } else {
                            hardware.pixels.cpu_mut()
                        });
                        for r in &rects {
                            for y in r.dy as usize..r.dy as usize + r.height as usize {
                                let at = y * 1024 + r.dx as usize;
                                pixels[at..at + r.width as usize].fill(black_box(color));
                            }
                        }
                    }
                    if iteration >= 10 {
                        timings[path].push(started.elapsed().as_secs_f64() * 1e6);
                    }
                }
            }
            // Check TDE alone after timing: a later CPU-MMZ fill must not hide
            // a missed hardware write. The sentinel also checks outside guards.
            let sentinel = PremultipliedRgbaColor::from_rgb(165, 165, 165);
            hardware.pixels.cpu_mut().fill(sentinel);
            software.pixels.cpu_mut().fill(sentinel);
            assert!(
                hardware
                    .engine
                    .as_mut()
                    .unwrap()
                    .fill(hardware.pixels.accelerator_buffer(), color, &rects)
                    .unwrap()
            );
            for r in &rects {
                for y in r.dy as usize..r.dy as usize + r.height as usize {
                    let at = y * 1024 + r.dx as usize;
                    software.pixels.cpu_mut()[at..at + r.width as usize].fill(color);
                }
            }
            assert_eq!(difference(hardware.pixels(), software.pixels()), (0, 0));
            let (ha, hm, hp) = summarize(std::mem::take(&mut timings[0]));
            let (ca, cm, cp) = summarize(std::mem::take(&mut timings[1]));
            let (ma, mm, mp) = summarize(std::mem::take(&mut timings[2]));
            let area = width * height * rects.len() as u32;
            println!(
                "fill_area name={name} color={color_name} width={width} height={height} rects={} pixels={area} samples=60 hw_mean_us={ha:.3} hw_p50_us={hm:.3} hw_p95_us={hp:.3} cpu_mean_us={ca:.3} cpu_p50_us={cm:.3} cpu_p95_us={cp:.3} mmz_cpu_mean_us={ma:.3} mmz_cpu_p50_us={mm:.3} mmz_cpu_p95_us={mp:.3} speedup={:.3} exact=true",
                rects.len(),
                ma / ha
            );
        }
    }
}
// Observe the real renderer callback boundary. The fixture contains only
// background, Image and Rectangle operations, so every raster row has an owner.
#[derive(Default, Debug)]
struct DispatchCounts {
    accepted: [usize; 3], // background, rectangle, texture
    fallback: [usize; 3],
    rows_after_accepted: [usize; 3],
    rows_after_fallback: [usize; 3],
}
struct DispatchAudit<'a> {
    buffer: &'a mut Buffer,
    current: Option<(usize, bool)>,
    counts: DispatchCounts,
}
impl DispatchAudit<'_> {
    fn record(&mut self, kind: usize, accepted: bool) -> bool {
        self.current = Some((kind, accepted));
        if accepted {
            self.counts.accepted[kind] += 1;
        } else {
            self.counts.fallback[kind] += 1;
        }
        accepted
    }
}
impl TargetPixelBuffer for DispatchAudit<'_> {
    type TargetPixel = Pixel;
    fn num_lines(&self) -> usize {
        self.buffer.num_lines()
    }
    fn line_slice(&mut self, line: usize) -> &mut [Pixel] {
        if let Some((kind, accepted)) = self.current {
            if accepted {
                self.counts.rows_after_accepted[kind] += 1;
            } else {
                self.counts.rows_after_fallback[kind] += 1;
            }
        }
        self.buffer.line_slice(line)
    }
    fn fill_background(&mut self, brush: &Brush, clip: &PhysicalRegion) -> bool {
        let accepted = self.buffer.fill_background(brush, clip);
        self.record(0, accepted)
    }
    fn draw_rectangle(&mut self, args: &DrawRectangleArgs, clip: &PhysicalRegion) -> bool {
        let accepted = self.buffer.draw_rectangle(args, clip);
        self.record(1, accepted)
    }
    fn draw_texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        let accepted = self.buffer.draw_texture(args, clip);
        self.record(2, accepted)
    }
}
fn draw_audited(window: &MinimalSoftwareWindow, buffer: &mut Buffer) -> DispatchCounts {
    let mut audit = DispatchAudit {
        buffer,
        current: None,
        counts: DispatchCounts::default(),
    };
    window.request_redraw();
    slint::platform::update_timers_and_animations();
    let mut rendered = false;
    window.draw_if_needed(|renderer| {
        rendered = true;
        renderer.render_into_buffer(&mut audit);
    });
    assert!(rendered && !audit.buffer.has_unknown_dma());
    audit.counts
}

#[test]
fn accepted_texture_is_not_drawn_again_by_software() {
    struct Sentinel;
    impl Accelerator for Sentinel {
        fn draw(
            &mut self,
            pixels: &mut [PremultipliedRgbaColor],
            _: &Texture<'_>,
        ) -> Result<Outcome, Failure> {
            pixels.fill(PremultipliedRgbaColor::from_rgb(252, 13, 47));
            Ok(Outcome::Copied)
        }
    }
    let (window, scene) = setup();
    scene.set_picture(image(512, 384, false, true));
    let mut buffer = Buffer::software(512, 384).unwrap();
    buffer.engine = Some(Box::new(Sentinel));
    let counts = draw_audited(&window, &mut buffer);
    assert_eq!(counts.accepted[2], 1);
    assert_eq!(counts.rows_after_accepted, [0; 3]);
    assert!(counts.rows_after_fallback[0] > 0 && counts.rows_after_fallback[1] > 0);
    let pixel = buffer.pixels()[200 * 512 + 200];
    assert_eq!(
        [pixel.red, pixel.green, pixel.blue, pixel.alpha],
        [252, 13, 47, 255]
    );
    // If draw_texture's success were ignored, software would replace this
    // sentinel with the source image. A rejected operation must do exactly that.
    buffer.engine = None;
    let counts = draw_audited(&window, &mut buffer);
    assert_eq!(counts.accepted[2], 0);
    assert_eq!(counts.fallback[2], 1);
    assert!(counts.rows_after_fallback[2] > 0);
    let mut reference = Buffer::software(512, 384).unwrap();
    draw(&window, &mut reference).unwrap();
    assert_eq!(difference(buffer.pixels(), reference.pixels()), (0, 0));
}

struct PartialWriteFailure(bool);
impl Accelerator for PartialWriteFailure {
    fn draw(
        &mut self,
        pixels: &mut [PremultipliedRgbaColor],
        _: &Texture<'_>,
    ) -> Result<Outcome, Failure> {
        // Simulate a job that changed the shared canvas before an error.
        pixels.fill(PremultipliedRgbaColor::from_rgb(165, 165, 165));
        Err(Failure {
            code: -79,
            unknown: self.0,
        })
    }
}

#[test]
fn partial_shared_canvas_failure_requires_full_software_repaint() {
    let (window, scene) = setup();
    scene.set_picture(image(512, 384, true, true));
    scene.set_image_opacity(0.5);
    let mut reference = Buffer::software(512, 384).unwrap();
    draw(&window, &mut reference).unwrap();
    let mut buffer = Buffer::software(512, 384).unwrap();
    scene.set_image_opacity(1.);
    window.request_redraw();
    window.draw_if_needed(|r| r.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer));
    draw(&window, &mut buffer).unwrap();
    scene.set_image_opacity(0.5);
    buffer.engine = Some(Box::new(PartialWriteFailure(false)));
    draw(&window, &mut buffer).unwrap();
    assert_eq!(difference(buffer.pixels(), reference.pixels()), (0, 0));
    window.request_redraw();
    window.draw_if_needed(|r| assert_eq!(r.repaint_buffer_type(), RepaintBufferType::ReusedBuffer));
    scene.set_image_opacity(0.75);
    draw(&window, &mut buffer).unwrap();
    window.request_redraw();
    window.draw_if_needed(|r| r.set_repaint_buffer_type(RepaintBufferType::NewBuffer));
    draw(&window, &mut reference).unwrap();
    assert_eq!(difference(buffer.pixels(), reference.pixels()), (0, 0));
}

#[test]
fn unknown_shared_dma_does_not_allow_following_software_writes() {
    let (window, scene) = setup();
    scene.set_picture(image(512, 384, true, true));
    let mut buffer = Buffer::software(512, 384).unwrap();
    buffer.engine = Some(Box::new(PartialWriteFailure(true)));
    assert!(matches!(
        draw(&window, &mut buffer),
        Err(Error::AccelerationFailed(_))
    ));
    // This fake has no DMA. Inspecting its heap verifies that all remaining
    // renderer callbacks were diverted away from the quarantined canvas.
    assert!(
        buffer
            .pixels
            .cpu()
            .iter()
            .all(|p| [p.red, p.green, p.blue, p.alpha] == [165, 165, 165, 255])
    );
}

pub(crate) fn fill_shared_canvas(buffer: &mut Buffer, color: PremultipliedRgbaColor) {
    #[cfg(target_arch = "arm")]
    assert!(
        matches!(buffer.pixels, Pixels::Mmz(_)),
        "expected a mapped MMZ canvas"
    );
    let enabled = buffer.fill_enabled;
    buffer.fill_enabled = true;
    let rect = Rect {
        sx: 0,
        sy: 0,
        dx: 0,
        dy: 0,
        width: buffer.width,
        height: buffer.height,
    };
    let done = buffer.fill(color, &[rect]);
    buffer.fill_enabled = enabled;
    assert!(
        done,
        "hardware fill must complete before reading its shared output"
    );
}

#[cfg(target_arch = "arm")]
#[test]
#[ignore = "CPU-only ARM NEON boundary test; build with the NX5 preset"]
fn neon_writeback_boundaries() {
    let convert = |source: &[u8], count: usize, shift: usize, alpha_max: u8| {
        let original = source.to_vec();
        let mut actual = vec![165u8; source.len()];
        let mut expected = actual.clone();
        for i in 0..count {
            let at = shift + i * 4;
            let alpha = u32::from(source[at + 3]);
            for c in 0..3 {
                let value = u32::from(source[at + 2 - c]);
                expected[at + c] = if alpha == 0 {
                    0
                } else {
                    ((value * 255 + alpha / 2) / alpha).min(255) as u8
                };
            }
            expected[at + 3] = ((alpha * u32::from(alpha_max) + 127) / 255) as u8;
        }
        // SAFETY: separate buffers, each covering shift+count*4 bytes and guards.
        assert!(unsafe {
            rgba_to_bgra(
                source.as_ptr().add(shift),
                actual.as_mut_ptr().add(shift),
                count,
                alpha_max,
            )
        });
        assert_eq!(
            actual, expected,
            "count={count} shift={shift} alpha_max={alpha_max}"
        );
        assert_eq!(source, original);
    };
    for alpha_max in [128, 255] {
        for alpha in 0..=255 {
            let mut source = vec![0; 256 * 4 + 8];
            for channel in 0..256 {
                source[channel * 4..channel * 4 + 3].fill(channel as u8);
                source[channel * 4 + 3] = alpha;
            }
            convert(&source, 256, 0, alpha_max);
        }
        for count in 0..=65 {
            for shift in 0..4 {
                for mode in 0..5 {
                    let mut source = vec![39; 65 * 4 + 8];
                    for i in 0..count {
                        for c in 0..3 {
                            source[shift + i * 4 + c] = (i * 37 + c * 51) as u8;
                        }
                        source[shift + i * 4 + 3] = match mode {
                            0 => 0,
                            1 => 255,
                            3 => {
                                if i < 32 {
                                    255
                                } else if i < 48 {
                                    127
                                } else {
                                    0
                                }
                            }
                            4 => {
                                if i < 16 {
                                    127
                                } else if i < 48 {
                                    255
                                } else {
                                    0
                                }
                            }
                            _ => (i * 73) as u8,
                        };
                    }
                    convert(&source, count, shift, alpha_max);
                }
            }
        }
    }
    println!(
        "NEON output: all channel/alpha pairs, alpha_max=128/255, zero/opaque/mixed blocks, lengths=0..65, alignment=0..3 and guards exact"
    );
}
