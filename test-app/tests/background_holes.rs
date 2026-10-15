use slint::platform::{
    Platform, WindowAdapter,
    software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
};
use slint::{ComponentHandle, Image, Model, PhysicalSize, Rgba8Pixel, SharedPixelBuffer, VecModel};
use slint_bridge_test_app::{BackgroundGeometry, BackgroundHole, BackgroundHolesTest, background};
use std::rc::Rc;

struct Host(Rc<MinimalSoftwareWindow>);
impl Platform for Host {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn hole(x: i32, y: i32, width: i32, height: i32) -> BackgroundHole {
    BackgroundHole {
        x,
        y,
        width,
        height,
        visible: true,
    }
}

fn picture(variant: u8) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut image = SharedPixelBuffer::new(128, 96);
    for (i, p) in image.make_mut_slice().iter_mut().enumerate() {
        *p = Rgba8Pixel::new((i % 128 * 2) as u8, (i / 128 * 2) as u8, variant, 255);
    }
    image
}

// Independent point-in-union oracle: it never partitions the background into rectangles.
fn check(
    window: &MinimalSoftwareWindow,
    pixels: &mut [PremultipliedRgbaColor],
    holes: &VecModel<BackgroundHole>,
    variant: u8,
    overlay: bool,
    modal: bool,
    stage: usize,
) {
    assert!(
        window.draw_if_needed(|r| {
            r.render(pixels, 128);
        }),
        "stage {stage} must redraw"
    );
    for y in 0..96 {
        for x in 0..128 {
            let transparent = holes.iter().any(|h| {
                h.visible
                    && h.width > 0
                    && h.height > 0
                    && i64::from(x) >= i64::from(h.x)
                    && i64::from(x) < i64::from(h.x) + i64::from(h.width)
                    && i64::from(y) >= i64::from(h.y)
                    && i64::from(y) < i64::from(h.y) + i64::from(h.height)
            });
            let mut expected = if transparent {
                [0; 4]
            } else {
                [(x * 2) as u8, (y * 2) as u8, variant, 255]
            };
            if overlay && (12..100).contains(&x) && (14..22).contains(&y) {
                expected = [224, 189, 69, 255];
            }
            if modal {
                for (channel, source) in [16, 32, 48].into_iter().enumerate() {
                    expected[channel] = source + (u16::from(expected[channel]) * 127 / 255) as u8;
                }
                expected[3] = 128 + (u16::from(expected[3]) * 127 / 255) as u8;
                if (33..95).contains(&x) && (25..67).contains(&y) {
                    expected = [244, 247, 251, 255];
                }
            }
            let p = pixels[(y * 128 + x) as usize];
            let actual = [p.red, p.green, p.blue, p.alpha];
            assert_eq!(actual[3], expected[3], "stage {stage} alpha at {x},{y}");
            assert!(
                actual[..3]
                    .iter()
                    .zip(&expected[..3])
                    .all(|(a, b)| a.abs_diff(*b) <= 1),
                "stage {stage} color at {x},{y}: {actual:?} != {expected:?}"
            );
        }
    }
}

#[test]
fn image_holes_and_foreground_survive_model_edits_and_reused_buffer_redraws() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(Host(window.clone()))).unwrap();
    let app = BackgroundHolesTest::new().unwrap();
    background::install(app.global::<BackgroundGeometry>());
    app.set_picture(Image::from_rgba8(picture(173)));
    let holes = Rc::new(VecModel::from(vec![hole(17, 9, 45, 35)]));
    app.set_holes(holes.clone().into());
    app.show().unwrap();
    window.set_size(PhysicalSize::new(128, 96));
    let mut pixels = vec![PremultipliedRgbaColor::default(); 128 * 96];
    let mut stage = 0;
    let mut verify = |variant, overlay, modal| {
        check(&window, &mut pixels, &holes, variant, overlay, modal, stage);
        stage += 1;
    };
    verify(173, false, false);
    holes.set_row_data(0, hole(70, 9, 45, 35));
    verify(173, false, false);
    holes.push(hole(29, 31, 53, 41));
    verify(173, false, false);
    for h in [
        hole(3, 7, 97, 79),
        hole(-23, -11, 79, 67),
        hole(119, 83, 50, 40),
        hole(0, 0, 128, 96),
        hole(1, 1, 0, 20),
        hole(1, 1, 20, -1),
        hole(i32::MAX - 3, 4, 20, 10),
        hole(-5, -5, i32::MAX, i32::MAX),
    ] {
        holes.set_row_data(0, h);
        verify(173, false, false);
    }
    holes.set_row_data(0, hole(17, 9, 45, 35));
    app.set_overlay(true);
    verify(173, true, false);
    app.set_modal(true);
    verify(173, true, true);
    holes.set_row_data(1, hole(5, 7, 105, 83));
    verify(173, true, true);
    app.set_modal(false);
    verify(173, true, false);
    let mut hidden = holes.row_data(0).unwrap();
    hidden.visible = false;
    holes.set_row_data(0, hidden);
    verify(173, true, false);
    holes.remove(1);
    verify(173, true, false);
    app.set_picture(Image::from_rgba8(picture(67)));
    verify(67, true, false);
    holes.set_row_data(0, hole(17, 9, 45, 35));
    verify(67, true, false);
    app.set_overlay(false);
    verify(67, false, false);
    holes.remove(0);
    verify(67, false, false);
    // Replacing the model also invalidates the component's geometry binding.
    app.set_holes(Rc::new(VecModel::from(vec![hole(17, 9, 45, 35)])).into());
    holes.push(hole(17, 9, 45, 35));
    verify(67, false, false);
    assert_eq!(stage, 22);
}
