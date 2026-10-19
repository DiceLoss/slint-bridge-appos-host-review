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
