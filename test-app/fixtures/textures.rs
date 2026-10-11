use super::App;

pub fn configure_textures(app: &App) {
    let mut backdrop = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(952, 300);
    for (i, p) in backdrop.make_mut_slice().iter_mut().enumerate() {
        *p = slint::Rgba8Pixel {
            r: (18 + (i % 952) / 12) as u8,
            g: (35 + (i / 952) / 3) as u8,
            b: 86,
            a: 255,
        };
    }
    let mut overlay = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(256, 256);
    for (i, p) in overlay.make_mut_slice().iter_mut().enumerate() {
        let x = (i % 256) as i32 - 128;
        let y = (i / 256) as i32 - 128;
        let a = if x * x + y * y < 120 * 120 { 160 } else { 0 };
        *p = slint::Rgba8Pixel {
            r: 61,
            g: 221,
            b: 196,
            a,
        };
    }
    app.set_backdrop(slint::Image::from_rgba8(backdrop));
    app.set_overlay(slint::Image::from_rgba8(overlay));
    app.set_textures(true);
}
