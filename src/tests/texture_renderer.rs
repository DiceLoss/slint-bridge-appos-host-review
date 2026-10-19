use super::*;
use i_slint_renderer_software::{
    RenderingRotation, RepaintBufferType, TargetPixel, TargetPixelBuffer,
};
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

struct Candidate<'a> {
    pixels: &'a mut [PremultipliedRgbaColor],
    stride: usize,
    hits: usize,
    fallbacks: usize,
}
impl TargetPixelBuffer for Candidate<'_> {
    type TargetPixel = PremultipliedRgbaColor;
    fn num_lines(&self) -> usize {
        self.pixels.len() / self.stride
    }
    fn line_slice(&mut self, line: usize) -> &mut [Self::TargetPixel] {
        &mut self.pixels[line * self.stride..(line + 1) * self.stride]
    }
    fn draw_texture(&mut self, args: &DrawTextureArgs, clip: &PhysicalRegion) -> bool {
        let done = super::draw(args, clip, self.pixels, self.stride);
        self.hits += usize::from(done);
        self.fallbacks += usize::from(!done);
        done
    }
}
#[test]
fn texture_drawing_matches_reference_with_rotation_and_clipping() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(TestPlatform(window.clone()))).unwrap();
    let app = Cards::new().unwrap();
    app.set_first(image(&source(96, 80, 0), 0));
    app.set_second(image(&source(112, 72, 17), 0));
    app.show().unwrap();
    window.set_size(PhysicalSize::new(240, 180));
    let mut reference = vec![PremultipliedRgbaColor::default(); 240 * 180];
    let mut candidate = reference.clone();
    window.request_redraw();
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.set_rendering_rotation(RenderingRotation::Rotate90);
        renderer.render(&mut reference, 180);
        let mut buffer = Candidate {
            pixels: &mut candidate,
            stride: 180,
            hits: 0,
            fallbacks: 0,
        };
        renderer.render_into_buffer(&mut buffer);
        assert!(buffer.hits > 0, "the adapter must handle eligible textures");
        for (a, b) in reference.iter().zip(&candidate) {
            assert_eq!(
                [a.red, a.green, a.blue, a.alpha],
                [b.red, b.green, b.blue, b.alpha]
            );
        }
    });
}

#[test]
fn premultiplied_blending_matches_division_for_all_byte_products() {
    use slint::platform::software_renderer::TargetPixel;
    let mut pixels = Vec::new();
    let mut colors = Vec::new();
    let mut reference = Vec::new();
    for alpha in 0..=255u16 {
        for value in 0..=255u16 {
            for translucent in [false, true] {
                let mut pixel = PremultipliedRgbaColor {
                    red: value as u8,
                    green: if translucent {
                        (value / 3) as u8
                    } else {
                        (255 - value) as u8
                    },
                    blue: if translucent { (value / 2) as u8 } else { 255 },
                    alpha: if translucent { value as u8 } else { 255 },
                };
                let source = PremultipliedRgbaColor {
                    red: (alpha / 2) as u8,
                    green: (alpha / 3) as u8,
                    blue: (alpha / 4) as u8,
                    alpha: alpha as u8,
                };
                let expected = [
                    u16::from(pixel.red) * (255 - alpha) / 255 + u16::from(source.red),
                    u16::from(pixel.green) * (255 - alpha) / 255 + u16::from(source.green),
                    u16::from(pixel.blue) * (255 - alpha) / 255 + u16::from(source.blue),
                    u16::from(pixel.alpha) + alpha - u16::from(pixel.alpha) * alpha / 255,
                ]
                .map(|v| v as u8);
                pixels.push(pixel);
                colors.push(source);
                reference.push(expected);
                pixel.blend(source);
                assert_eq!([pixel.red, pixel.green, pixel.blue, pixel.alpha], expected);
            }
        }
    }
    let mut sampled = colors.iter().copied();
    super::blend_sampled_slice(&mut pixels, || sampled.next().unwrap());
    assert!(sampled.next().is_none());
    for (pixel, expected) in pixels.iter().zip(reference) {
        assert_eq!([pixel.red, pixel.green, pixel.blue, pixel.alpha], expected);
    }
    for count in [0, 1, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65] {
        for offset in 0..4 {
            for opaque in [false, true] {
                let mut dest: Vec<_> = (0..count + 8)
                    .map(|i| pixels[(i * 7919) % pixels.len()])
                    .collect();
                let src: Vec<_> = (0..count)
                    .map(|i| {
                        let mut p = colors[(i * 1031) % colors.len()];
                        if opaque {
                            p.alpha = 255;
                        }
                        p
                    })
                    .collect();
                let mut expected = dest.clone();
                for (d, s) in expected[offset..offset + count].iter_mut().zip(&src) {
                    d.blend(*s);
                }
                let mut sampled = src.into_iter();
                super::blend_sampled_slice(&mut dest[offset..offset + count], || {
                    sampled.next().unwrap()
                });
                assert!(sampled.next().is_none());
                for (a, b) in dest.iter().zip(expected) {
                    assert_eq!(
                        [a.red, a.green, a.blue, a.alpha],
                        [b.red, b.green, b.blue, b.alpha]
                    );
                }
            }
        }
    }
}

#[test]
fn sampled_blending_matches_scalar_with_guards() {
    let color = |i: usize, alpha: u8| PremultipliedRgbaColor {
        red: (i * 17 % (usize::from(alpha) + 1)) as u8,
        green: (i * 71 % (usize::from(alpha) + 1)) as u8,
        blue: (i * 137 % (usize::from(alpha) + 1)) as u8,
        alpha,
    };
    for count in 0..=65 {
        for offset in 0..4 {
            for alpha in [Some(0), Some(1), Some(127), Some(254), Some(255), None] {
                let mut expected: Vec<_> =
                    (0..count + 8).map(|i| color(i, (i * 71) as u8)).collect();
                let mut pixels: Vec<_> = expected.clone();
                let sources: Vec<_> = (0..count)
                    .map(|i| color(i + 1, alpha.unwrap_or((i * 37) as u8)))
                    .collect();
                for (pixel, source) in expected[offset..offset + count].iter_mut().zip(&sources) {
                    pixel.blend(*source);
                }
                let mut sampled = 0;
                super::blend_sampled_slice(&mut pixels[offset..offset + count], || {
                    let source = sources[sampled];
                    sampled += 1;
                    source
                });
                assert_eq!(sampled, count);
                for (a, b) in pixels.iter().zip(&expected) {
                    assert_eq!(
                        [a.red, a.green, a.blue, a.alpha],
                        [b.red, b.green, b.blue, b.alpha]
                    );
                }
            }
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
    let (mut hits, mut fallbacks) = (0, 0);
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
        for frame in 0..24 {
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
                20 => app.set_zoom(0.5),
                21 => app.set_zoom(2.),
                22 => app.set_zoom(0.875),
                23 => app.set_zoom(1.),
                _ => (),
            }
            let (w, h) = if frame >= 19 { (250, 190) } else { (256, 192) };
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
                    let mut buffer = Candidate {
                        pixels: &mut pixels,
                        stride,
                        hits: 0,
                        fallbacks: 0,
                    };
                    renderer.render_into_buffer(&mut buffer);
                    hits += buffer.hits;
                    fallbacks += buffer.fallbacks;
                } else {
                    renderer.render(&mut pixels, stride);
                }
            });
            tx.send(
                pixels
                    .iter()
                    .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
                    .collect(),
            )
            .unwrap();
        }
    }
    (hits, fallbacks)
}

#[test]
fn texture_sequences_match_slint_for_all_rotations_formats_and_repaint_modes() {
    for partial in [false, true] {
        for format in 0..3 {
            let (tx, reference) = std::sync::mpsc::sync_channel(1);
            let baseline = std::thread::spawn(move || run(false, format, partial, tx));
            let (tx, candidate) = std::sync::mpsc::sync_channel(1);
            let worker = std::thread::spawn(move || run(true, format, partial, tx));
            for frame in 0..96 {
                let expected = reference.recv().unwrap();
                let actual = candidate.recv().unwrap();
                assert!(
                    expected == actual,
                    "partial={partial} format={format} frame={frame} first difference={:?}",
                    expected.iter().zip(&actual).position(|(a, b)| a != b)
                );
            }
            baseline.join().unwrap();
            let (hits, fallbacks) = worker.join().unwrap();
            assert!(hits > 20, "adapter drawing was not exercised");
            assert!(fallbacks > 0, "unsupported scales and tint must fall back");
        }
    }
}
