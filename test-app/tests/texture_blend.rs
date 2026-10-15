use slint::platform::{
    Platform, WindowAdapter,
    software_renderer::{
        MinimalSoftwareWindow, PremultipliedRgbaColor, RenderingRotation, RepaintBufferType,
    },
};
use slint::{Color, ComponentHandle, Image, PhysicalSize, Rgba8Pixel, SharedPixelBuffer};
use slint_bridge_test_app::TextureBlendTest;
use std::rc::Rc;

struct Host(Rc<MinimalSoftwareWindow>);
impl Platform for Host {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

#[test]
fn premultiplied_textures_preserve_blending_clipping_and_rotated_pixels() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(Host(window.clone()))).unwrap();
    let app = TextureBlendTest::new().unwrap();
    app.show().unwrap();
    window.set_size(PhysicalSize::new(128, 96));
    let mut pixels = vec![PremultipliedRgbaColor::default(); 128 * 96];
    for (rotation, stride) in [
        (RenderingRotation::NoRotation, 128),
        (RenderingRotation::Rotate90, 96),
        (RenderingRotation::Rotate180, 128),
        (RenderingRotation::Rotate270, 96),
    ] {
        for (case, (dx, dy, base_alpha)) in [
            (0, 0, 0),
            (1, 3, 63),
            (-7, 2, 128),
            (9, -5, 255),
            (31, 7, 254),
            (0, 0, 255),
        ]
        .into_iter()
        .enumerate()
        {
            let base = Color::from_argb_u8(base_alpha, 137, 83, 219);
            app.set_base(base);
            app.set_offset_x(dx);
            app.set_offset_y(dy);
            let mut image = SharedPixelBuffer::<Rgba8Pixel>::new(128, 96);
            for (i, p) in image.make_mut_slice().iter_mut().enumerate() {
                let a = if case == 5 {
                    255
                } else {
                    (i + case * 37) as u8
                };
                *p = Rgba8Pixel::new(a / 2, a / 3, a, a);
            }
            let source = image.clone();
            app.set_picture(Image::from_rgba8_premultiplied(image));
            window.request_redraw();
            assert!(window.draw_if_needed(|r| {
                r.set_rendering_rotation(rotation);
                r.render(&mut pixels, stride);
            }));
            let bg = [137u16, 83, 219].map(|v| (v * u16::from(base_alpha) / 255) as u8);
            for y in 0..96i32 {
                for x in 0..128i32 {
                    let mut expected = [bg[0], bg[1], bg[2], base_alpha];
                    if (0..128).contains(&(x - dx)) && (0..96).contains(&(y - dy)) {
                        let s = source.as_slice()[((y - dy) * 128 + x - dx) as usize];
                        for (c, v) in [s.r, s.g, s.b].into_iter().enumerate() {
                            expected[c] =
                                v + (u16::from(expected[c]) * (255 - u16::from(s.a)) / 255) as u8;
                        }
                        expected[3] = (u16::from(base_alpha) + u16::from(s.a)
                            - u16::from(base_alpha) * u16::from(s.a) / 255)
                            as u8;
                    }
                    let (px, py) = match rotation {
                        RenderingRotation::NoRotation => (x, y),
                        RenderingRotation::Rotate90 => (95 - y, x),
                        RenderingRotation::Rotate180 => (127 - x, 95 - y),
                        RenderingRotation::Rotate270 => (y, 127 - x),
                        _ => unreachable!(),
                    };
                    let p = pixels[py as usize * stride + px as usize];
                    assert_eq!(
                        [p.red, p.green, p.blue, p.alpha],
                        expected,
                        "{rotation:?}, case={case}, at {x},{y}"
                    );
                }
            }
        }
    }
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
    PremultipliedRgbaColor::blend_sampled_slice(&mut pixels, || sampled.next().unwrap());
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
                PremultipliedRgbaColor::blend_sampled_slice(
                    &mut dest[offset..offset + count],
                    || sampled.next().unwrap(),
                );
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
