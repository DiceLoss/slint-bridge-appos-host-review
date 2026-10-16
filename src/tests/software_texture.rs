use super::*;
use i_slint_renderer_software::{RenderingRotation, RepaintBufferType, TargetPixelBuffer};
use slint::{
    ComponentHandle, Image, PhysicalSize, Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer,
    platform::{Platform, WindowAdapter, software_renderer::MinimalSoftwareWindow},
};
use std::rc::Rc;

slint::slint! {
    export component Cards inherits Window {
        background: #19374a80;
        in property <image> first;
        in property <image> second;
        in property <length> offset;
        in property <bool> visible-card: true;
        in property <bool> crop;
        in property <bool> large;
        in property <bool> clipping: true;
        in property <float> zoom: 1;
        in property <float> image-opacity: 1;
        in property <color> tint: transparent;
        Rectangle {
            x: 13px; y: 17px; width: 180px; height: 130px; clip: root.clipping;
            if root.visible-card: Image {
                x: root.offset; y: 16.25px;
                width: (root.large ? 600px : root.crop ? 88px : 96px) * root.zoom;
                height: (root.large ? 600px : 80px) * root.zoom;
                source: root.first; image-fit: fill;
                source-clip-x: root.crop ? 8 : 0;
                source-clip-width: root.large ? 600 : root.crop ? 88 : 96;
                source-clip-height: root.large ? 600 : 80;
                opacity: root.image-opacity; colorize: root.tint;
            }
            Image { x: root.offset + 40px; y: 65.5px; width: 112px; height: 72px; source: root.second; image-fit: fill; }
        }
        Rectangle { x: 70px; y: 90px; width: 22px; height: 19px; background: #80a0d080; border-radius: 5px; }
    }
}

struct TestPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

struct Buffer<'a> {
    pixels: &'a mut [PremultipliedRgbaColor],
    stride: usize,
    cache: &'a mut RotatedCache,
}
impl TargetPixelBuffer for Buffer<'_> {
    type TargetPixel = PremultipliedRgbaColor;
    fn num_lines(&self) -> usize {
        self.pixels.len() / self.stride
    }
    fn line_slice(&mut self, line: usize) -> &mut [Self::TargetPixel] {
        &mut self.pixels[line * self.stride..(line + 1) * self.stride]
    }
    fn draw_texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        self.cache.draw(args, clip, self.pixels, self.stride)
    }
}

fn source(width: u32, height: u32, seed: usize) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut data = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    for (i, p) in data.make_mut_slice().iter_mut().enumerate() {
        let a = [0, 1, 17, 127, 128, 254, 255][i % 7];
        let channel = |v: usize| (u16::from(v as u8) * u16::from(a) / 255) as u8;
        *p = Rgba8Pixel {
            r: channel(i * 13 + seed),
            g: channel(i / 3 + seed),
            b: channel(i * 7 + seed),
            a,
        };
    }
    data
}

fn image(source: &SharedPixelBuffer<Rgba8Pixel>, format: u8) -> Image {
    match format {
        0 => Image::from_rgba8_premultiplied(source.clone()),
        1 => Image::from_rgba8(source.clone()),
        _ => {
            let mut data = SharedPixelBuffer::<Rgb8Pixel>::new(source.width(), source.height());
            for (p, s) in data.make_mut_slice().iter_mut().zip(source.as_slice()) {
                *p = Rgb8Pixel {
                    r: s.r,
                    g: s.g,
                    b: s.b,
                };
            }
            Image::from_rgb8(data)
        }
    }
}

fn run(
    cached: bool,
    format: u8,
    partial: bool,
    tx: std::sync::mpsc::SyncSender<Vec<u8>>,
) -> (usize, usize) {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(TestPlatform(window.clone()))).unwrap();
    let app = Cards::new().unwrap();
    let mut first = source(96, 80, 0);
    app.set_second(image(&source(112, 72, 17), format));
    app.show().unwrap();
    let mut cache = RotatedCache::default();
    for rotation in [
        RenderingRotation::NoRotation,
        RenderingRotation::Rotate90,
        RenderingRotation::Rotate180,
        RenderingRotation::Rotate270,
    ] {
        window.set_size(PhysicalSize::new(256, 192));
        app.set_first(image(&first, format));
        app.set_crop(false);
        app.set_large(false);
        app.set_visible_card(true);
        app.set_clipping(true);
        app.set_zoom(1.);
        app.set_image_opacity(1.);
        app.set_tint(slint::Color::from_argb_u8(0, 0, 0, 0));
        let transpose = matches!(
            rotation,
            RenderingRotation::Rotate90 | RenderingRotation::Rotate270
        );
        let mut pixels = vec![PremultipliedRgbaColor::default(); 256 * 192];
        for frame in 0..20 {
            app.set_offset([-50.25, -0.5, 0., 0.5, 10.25, 30.5, 60.75, 145.75][frame % 8]);
            match frame {
                3 => app.set_image_opacity(0.5),
                4 => app.set_tint(slint::Color::from_argb_u8(120, 20, 80, 60)),
                5 => {
                    app.set_image_opacity(1.);
                    app.set_tint(slint::Color::from_argb_u8(0, 0, 0, 0));
                }
                6 => {
                    let red = ((u16::from(first.as_slice()[297].r) + 31) % 201) as u8;
                    first.make_mut_slice()[297] = Rgba8Pixel {
                        r: red,
                        g: 40,
                        b: 3,
                        a: 200,
                    };
                    app.set_first(image(&first, format));
                }
                7 => app.set_first(image(&source(96, 80, 57), format)),
                8 => app.set_first(image(&first, format)),
                9 => app.set_visible_card(false),
                10 => app.set_visible_card(true),
                11 => app.set_crop(true),
                12 => app.set_crop(false),
                13 => app.set_zoom(1.25),
                14 => app.set_zoom(1.),
                16 => app.set_clipping(false),
                17 => app.set_clipping(true),
                18 => {
                    app.set_large(true);
                    app.set_first(image(&source(600, 600, 71), format));
                }
                19 => {
                    app.set_large(false);
                    app.set_first(image(&first, format));
                    window.set_size(PhysicalSize::new(250, 190));
                }
                _ => (),
            }
            let (w, h) = if frame == 19 { (250, 190) } else { (256, 192) };
            let stride = if transpose { h } else { w };
            pixels.resize(w * h, PremultipliedRgbaColor::default());
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.set_rendering_rotation(rotation);
                renderer.set_repaint_buffer_type(if partial && frame > 0 {
                    RepaintBufferType::ReusedBuffer
                } else {
                    RepaintBufferType::NewBuffer
                });
                if cached {
                    renderer.render_into_buffer(&mut Buffer {
                        pixels: &mut pixels,
                        stride,
                        cache: &mut cache,
                    });
                } else {
                    renderer.render(&mut pixels, stride);
                }
            });
            assert!(
                cache.cached_pixels() <= 256 * 1024,
                "cache exceeded 1 MiB pixel budget"
            );
            tx.send(
                pixels
                    .iter()
                    .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
                    .collect(),
            )
            .unwrap();
        }
    }
    (cache.hits, cache.misses)
}

#[test]
fn rotated_cards_match_full_render_through_clipping_motion_replacement_and_fallbacks() {
    for partial in [false, true] {
        for format in 0..3 {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let reference = std::thread::spawn(move || run(false, format, partial, tx));
            let (tx, actual) = std::sync::mpsc::sync_channel(1);
            let candidate = std::thread::spawn(move || run(true, format, partial, tx));
            for frame in 0..80 {
                let expected = rx.recv().unwrap();
                let got = actual.recv().unwrap();
                assert_eq!(expected.len(), got.len());
                assert!(
                    expected == got,
                    "format {format}, frame {frame}, first mismatch {:?}",
                    expected.iter().zip(&got).position(|(a, b)| a != b)
                );
            }
            reference.join().unwrap();
            let (hits, misses) = candidate.join().unwrap();
            if format == 0 {
                assert!(hits >= 10, "rotated cache was never reused");
                assert!(misses >= 8, "source/rotation changes were not exercised");
            } else {
                assert_eq!(hits, 0);
            }
        }
    }
}

#[test]
fn one_use_sources_do_not_allocate_rotated_pixels() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(TestPlatform(window.clone()))).unwrap();
    let app = Cards::new().unwrap();
    app.show().unwrap();
    window.set_size(PhysicalSize::new(256, 192));
    let mut pixels = vec![PremultipliedRgbaColor::default(); 256 * 192];
    let mut cache = RotatedCache::default();
    for frame in 0..8 {
        app.set_first(image(&source(96, 80, frame), 0));
        app.set_second(image(&source(112, 72, frame + 20), 0));
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.set_rendering_rotation(RenderingRotation::Rotate90);
            renderer.render_into_buffer(&mut Buffer {
                pixels: &mut pixels,
                stride: 192,
                cache: &mut cache,
            });
        });
        assert_eq!(cache.cached_pixels(), 0);
    }
    assert_eq!(cache.misses, 0);
}
