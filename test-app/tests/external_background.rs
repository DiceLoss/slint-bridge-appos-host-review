#![cfg(feature = "nx5-mma-background")]

use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use std::sync::Arc;

#[test]
fn external_pixels_retain_owner_and_copy_before_mutation() {
    let owner = Arc::new(vec![Rgba8Pixel::new(7, 11, 17, 255); 8]);
    let weak = Arc::downgrade(&owner);
    let pointer = owner.as_ptr();
    // SAFETY: the Arc owns immutable, initialized pixels across all clones.
    let pixels = unsafe { SharedPixelBuffer::from_external(pointer, 4, 2, owner) };
    let image = Image::from_rgba8_premultiplied(pixels.clone());
    drop(pixels);
    let mut copy = image.to_rgba8_premultiplied().unwrap();
    assert_eq!(copy.as_slice().as_ptr(), pointer);
    copy.make_mut_slice()[0] = Rgba8Pixel::new(1, 2, 3, 255);
    assert_ne!(copy.as_slice().as_ptr(), pointer);
    assert_eq!(
        image.to_rgba8().unwrap().as_slice()[0],
        Rgba8Pixel::new(7, 11, 17, 255)
    );
    assert!(weak.upgrade().is_some());
    drop(image);
    assert!(weak.upgrade().is_none());
    assert_eq!(copy.as_bytes()[..4], [1, 2, 3, 255]);
}

#[test]
fn showcase_external_background_matches_owned_software_pixels() {
    use slint::platform::{
        Platform, WindowAdapter,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    };
    use slint_bridge_test_app::{Showcase, showcase::configure};
    use std::rc::Rc;
    struct RenderPlatform(Rc<MinimalSoftwareWindow>);
    impl Platform for RenderPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            Ok(self.0.clone())
        }
    }
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let app = Showcase::new().unwrap();
    assert_eq!(
        app.get_wallpaper().size().width,
        0,
        "background must not decode before application storage selection"
    );
    configure(&app).stop();
    app.show().unwrap();
    window.set_size(slint::PhysicalSize::new(1024, 600));
    let owned = app.get_wallpaper();
    let pixels = owned.to_rgba8_premultiplied().unwrap();
    let owner = Arc::new(pixels.as_slice().to_vec());
    // SAFETY: the image retains the immutable allocation through its owner.
    let external = Image::from_rgba8_premultiplied(unsafe {
        SharedPixelBuffer::from_external(owner.as_ptr(), pixels.width(), pixels.height(), owner)
    });
    for page in [0, 1, 2, 3, 4, 5, 6, 7] {
        app.invoke_navigate(page);
        app.set_wallpaper(owned.clone());
        let expected = app.window().take_snapshot().unwrap();
        app.set_wallpaper(external.clone());
        let actual = app.window().take_snapshot().unwrap();
        assert_eq!(
            actual.as_bytes(),
            expected.as_bytes(),
            "software page {page}"
        );
    }
}
