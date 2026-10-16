use super::*;
use slint::{
    ComponentHandle, Image, PhysicalSize, Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer,
    platform::{
        Platform, WindowAdapter,
        software_renderer::{MinimalSoftwareWindow, RenderingRotation, RepaintBufferType},
    },
};
use std::rc::Rc;

slint::slint! {
    export component BackgroundScene inherits Window {
        background: #19374a;
        in property <image> picture;
        in property <float> opacity-value: 1;
        in property <bool> show-picture: true;
        in property <length> inset: 0px;
        in property <length> marker: 8px;
        Rectangle {
            x: root.inset; y: root.inset;
            width: parent.width - root.inset; height: parent.height - root.inset;
            clip: true;
            if root.show-picture: Image {
                x: -root.inset; y: -root.inset;
                width: root.width; height: root.height;
                source: root.picture; image-fit: cover; opacity: root.opacity-value;
            }
        }
        Rectangle { x: root.marker; y: 10px; width: 22px; height: 19px; background: #80a0d080; border-radius: 5px; }
    }
}

struct TestPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn image(source: &SharedPixelBuffer<Rgba8Pixel>, format: u8) -> Image {
    match format {
        0 => Image::from_rgba8(source.clone()),
        1 => {
            let mut pixels = source.clone();
            for p in pixels.make_mut_slice() {
                p.r = (u16::from(p.r) * u16::from(p.a) / 255) as u8;
                p.g = (u16::from(p.g) * u16::from(p.a) / 255) as u8;
                p.b = (u16::from(p.b) * u16::from(p.a) / 255) as u8;
            }
            Image::from_rgba8_premultiplied(pixels)
        }
        _ => {
            let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(source.width(), source.height());
            for (p, s) in pixels.make_mut_slice().iter_mut().zip(source.as_slice()) {
                *p = Rgb8Pixel {
                    r: s.r,
                    g: s.g,
                    b: s.b,
                };
            }
            Image::from_rgb8(pixels)
        }
    }
}

fn render_sequence(cached: bool, format: u8, send: std::sync::mpsc::SyncSender<Vec<u8>>) -> usize {
    let window = MinimalSoftwareWindow::new(if cached {
        RepaintBufferType::ReusedBuffer
    } else {
        RepaintBufferType::NewBuffer
    });
    slint::platform::set_platform(Box::new(TestPlatform(window.clone()))).unwrap();
    let app = BackgroundScene::new().unwrap();
    let mut source = SharedPixelBuffer::<Rgba8Pixel>::new(100, 60);
    for (i, p) in source.make_mut_slice().iter_mut().enumerate() {
        *p = Rgba8Pixel {
            r: (i * 13) as u8,
            g: (i / 3) as u8,
            b: (i * 7) as u8,
            a: 255,
        };
    }
    app.set_picture(image(&source, format));
    app.show().unwrap();
    let mut cache = BackgroundCache::default();
    for rotation in [
        RenderingRotation::NoRotation,
        RenderingRotation::Rotate90,
        RenderingRotation::Rotate180,
        RenderingRotation::Rotate270,
    ] {
        let mut pixels = vec![PremultipliedRgbaColor::default(); 128 * 80];
        window.set_size(PhysicalSize::new(128, 80));
        let stride = if matches!(
            rotation,
            RenderingRotation::Rotate90 | RenderingRotation::Rotate270
        ) {
            80
        } else {
            128
        };
        for step in 0..12 {
            app.set_marker((8 + step * 3) as f32);
            match step {
                0 => {
                    app.set_inset(0.);
                    app.set_opacity_value(1.);
                    app.set_show_picture(true);
                    app.set_picture(image(&source, format));
                }
                2 => app.set_inset(7.),
                3 => app.set_inset(0.),
                4 => {
                    // The previous cache key must retain the old allocation: COW
                    // changes source pixels without aliasing the cached identity.
                    source.make_mut_slice()[150] = Rgba8Pixel {
                        r: 255,
                        g: 0,
                        b: 17,
                        a: 255,
                    };
                    app.set_picture(image(&source, format));
                }
                5 => app.set_opacity_value(0.5),
                6 => app.set_opacity_value(1.),
                7 => app.set_show_picture(false),
                8 => app.set_show_picture(true),
                9 => {
                    let mut transparent = source.clone();
                    transparent.make_mut_slice()[150].a = 30;
                    app.set_picture(image(&transparent, format));
                }
                10 => {
                    app.set_picture(image(&source, format));
                    window.set_size(PhysicalSize::new(125, 79));
                }
                _ => (),
            }
            let (w, h) = if step >= 10 {
                if stride == 80 { (79, 125) } else { (125, 79) }
            } else {
                (stride, 128 * 80 / stride)
            };
            pixels.resize(w * h, PremultipliedRgbaColor::default());
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.set_rendering_rotation(rotation);
                if cached {
                    cache.render(renderer, &mut pixels, w);
                } else {
                    renderer.render(&mut pixels, w);
                }
            });
            send.send(
                pixels
                    .iter()
                    .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
                    .collect(),
            )
            .unwrap();
        }
    }
    cache.hits
}

#[test]
fn cached_background_matches_original_with_damage_rotation_clipping_and_source_changes() {
    for format in 0..3 {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let reference = std::thread::spawn(move || render_sequence(false, format, tx));
        let (tx, actual) = std::sync::mpsc::sync_channel(1);
        let candidate = std::thread::spawn(move || render_sequence(true, format, tx));
        for frame in 0..48 {
            let expected = rx.recv().unwrap();
            let got = actual.recv().unwrap();
            assert_eq!(got.len(), expected.len());
            assert!(
                got == expected,
                "background frame {frame}: first differing byte {:?}",
                got.iter().zip(&expected).position(|(a, b)| a != b)
            );
        }
        reference.join().unwrap();
        assert!(
            candidate.join().unwrap() >= 12,
            "background cache was never reused"
        );
    }
}
