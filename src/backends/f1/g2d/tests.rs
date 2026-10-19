use super::*;
use std::{cell::Cell, rc::Rc};

slint::slint! {
    export component FillScene inherits Window {
        width: 320px; height: 240px;
        background: #123456;
        Rectangle { x: 12px; y: 8px; width: 300px; height: 160px; background: #781234; }
        Rectangle {
            x: 20px; y: 20px; width: 180px; height: 100px;
            background: #ff000080; border-radius: 6px;
        }
        Rectangle {
            x: 100px; y: 180px; width: 250px; height: 100px;
            background: @linear-gradient(90deg, blue, green);
        }
    }
}

struct RenderPlatform(Rc<slint::platform::software_renderer::MinimalSoftwareWindow>);
impl slint::platform::Platform for RenderPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

struct FillCanvas {
    pixels: Vec<PremultipliedRgbaColor>,
    width: u32,
    calls: Rc<Cell<usize>>,
}
impl Canvas for FillCanvas {
    fn pixels(&self) -> &[PremultipliedRgbaColor] {
        &self.pixels
    }
    fn pixels_mut(&mut self) -> &mut [PremultipliedRgbaColor] {
        &mut self.pixels
    }
    fn fill(&mut self, color: PremultipliedRgbaColor, rects: &[Rect]) -> Result<bool, String> {
        self.calls.set(self.calls.get() + 1);
        for r in rects {
            for y in r.y..r.y + r.height {
                let start = (y * self.width + r.x) as usize;
                self.pixels[start..start + r.width as usize].fill(color);
            }
        }
        Ok(true)
    }
}

#[test]
fn opaque_fill_callbacks_preserve_software_pixels_and_fallbacks() {
    use slint::ComponentHandle;
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let scene = FillScene::new().unwrap();
    scene.show().unwrap();
    window.set_size(slint::PhysicalSize::new(320, 240));
    use i_slint_renderer_software::RenderingRotation;
    for (rotation, width, height) in [
        (RenderingRotation::NoRotation, 320, 240),
        (RenderingRotation::Rotate90, 240, 320),
        (RenderingRotation::Rotate180, 320, 240),
        (RenderingRotation::Rotate270, 240, 320),
    ] {
        let calls = Rc::new(Cell::new(0));
        let mut buffer = Buffer {
            canvas: Box::new(FillCanvas {
                pixels: allocate(320 * 240).unwrap(),
                width,
                calls: calls.clone(),
            }),
            width,
            height,
            failed: None,
            discard: allocate(width as usize).unwrap(),
            opaque_background: None,
            background_seen: false,
            texture_keys: [None, None],
            texture_frames: [0; 2],
            frame: 0,
            texture_hits: 0,
        };
        let mut expected = allocate(320 * 240).unwrap();
        window.request_redraw();
        let mut drew = false;
        window.draw_if_needed(|renderer| {
            renderer.set_rendering_rotation(rotation);
            renderer.render(&mut expected, width as usize);
            buffer.render(renderer).unwrap();
            drew = true;
        });
        assert!(drew);
        assert!(
            calls.get() >= 2,
            "background and large opaque rectangle should use fill"
        );
        for (a, b) in expected.iter().zip(buffer.pixels()) {
            assert_eq!(
                [a.red, a.green, a.blue, a.alpha],
                [b.red, b.green, b.blue, b.alpha],
                "{rotation:?}"
            );
        }
    }
}

struct FailingCanvas {
    pixels: Vec<PremultipliedRgbaColor>,
    calls: Rc<Cell<usize>>,
    uncertain: bool,
    quarantined: Rc<Cell<bool>>,
}

impl Canvas for FailingCanvas {
    fn pixels(&self) -> &[PremultipliedRgbaColor] {
        assert!(!self.uncertain, "read during unknown DMA");
        &self.pixels
    }
    fn pixels_mut(&mut self) -> &mut [PremultipliedRgbaColor] {
        assert!(!self.uncertain, "write during unknown DMA");
        &mut self.pixels
    }
    fn fill(&mut self, _: PremultipliedRgbaColor, _: &[Rect]) -> Result<bool, String> {
        self.calls.set(self.calls.get() + 1);
        self.uncertain = true;
        Err("injected uncertain completion".into())
    }
}

impl Drop for FailingCanvas {
    fn drop(&mut self) {
        self.quarantined.set(self.uncertain);
    }
}

#[test]
fn failed_fill_is_not_retried_or_exposed_to_cpu_callbacks() {
    let calls = Rc::new(Cell::new(0));
    let quarantined = Rc::new(Cell::new(false));
    let mut buffer = Buffer {
        canvas: Box::new(FailingCanvas {
            pixels: allocate(256 * 128).unwrap(),
            calls: calls.clone(),
            uncertain: false,
            quarantined: quarantined.clone(),
        }),
        width: 256,
        height: 128,
        failed: None,
        discard: allocate(256).unwrap(),
        opaque_background: None,
        background_seen: false,
        texture_keys: [None, None],
        texture_frames: [0; 2],
        frame: 0,
        texture_hits: 0,
    };
    let opaque = PremultipliedRgbaColor {
        red: 255,
        green: 0,
        blue: 0,
        alpha: 255,
    };
    let rects = [Rect {
        x: 0,
        y: 0,
        width: 256,
        height: 128,
    }];
    // Translucent pixels cannot be handled by the opaque fill path.
    assert!(!buffer.fill(
        PremultipliedRgbaColor {
            alpha: 128,
            ..opaque
        },
        &rects
    ));
    assert_eq!(calls.get(), 0);
    assert!(buffer.fill(opaque, &rects));
    buffer.line_slice(0).fill(opaque); // must only touch the discard row
    assert!(buffer.fill(opaque, &rects));
    assert_eq!(calls.get(), 1);
    let renderer = SoftwareRenderer::new();
    assert!(matches!(
        buffer.render(&renderer),
        Err(Error::AccelerationFailed(_))
    ));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| buffer.pixels())).is_err());
    drop(buffer);
    assert!(quarantined.get());
}

#[test]
fn clip_cannot_address_outside_the_canvas() {
    let buffer = Buffer::new(256, 128).unwrap();
    // Exercise each intersection boundary without depending on the native SDK.
    let clip = [(
        slint::PhysicalPosition::new(0, 0),
        slint::PhysicalSize::new(256, 128),
    )];
    let rects = buffer.clipped((-5, 20, 500, 200), clip.into_iter());
    assert_eq!(rects.len(), 1);
    assert_eq!(
        (rects[0].x, rects[0].y, rects[0].width, rects[0].height),
        (0, 20, 256, 108)
    );
    assert!(
        buffer
            .clipped((300, 0, 400, 10), clip.into_iter())
            .is_empty()
    );
}

slint::slint! {
    export component TextureScene inherits Window {
        width: 192px; height: 144px;
        background: #18375180;
        in property <image> picture;
        in property <float> image-opacity: 1;
        in property <bool> full: false;
        in property <float> zoom: 1;
        Rectangle {
            x: root.full ? 0px : 9px; y: root.full ? 0px : 7px; width: root.full ? 192px : 171px; height: root.full ? 144px : 129px; clip: true;
            Image { x: root.full ? 0px : -7px; y: root.full ? 0px : -4px; width: 192px * root.zoom; height: 144px * root.zoom;
                source: root.picture; opacity: root.image-opacity; image-fit: fill; }
        }
    }
}

#[test]
fn unscaled_images_match_software_with_clipping_alpha_and_changed_sources() {
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let scene = TextureScene::new().unwrap();
    scene.show().unwrap();
    window.set_size(slint::PhysicalSize::new(192, 144));
    let mut buffer = Buffer::new(192, 144).unwrap();
    for full in [false, true] {
        scene.set_full(full);
        for premultiplied in [true, false] {
            for opaque in [true, false] {
                let mut source = SharedPixelBuffer::<Rgba8Pixel>::new(192, 144);
                for (i, p) in source.make_mut_slice().iter_mut().enumerate() {
                    let a = if opaque { 255 } else { (i * 13 % 256) as u8 };
                    let c = |v: u8| {
                        if premultiplied {
                            (u16::from(v) * u16::from(a) / 255) as u8
                        } else {
                            v
                        }
                    };
                    *p = Rgba8Pixel {
                        r: c((i * 3) as u8),
                        g: c((i * 7) as u8),
                        b: c((i * 11) as u8),
                        a,
                    };
                }
                for opacity in [1., 0.5, 0., 0.01, 0.99] {
                    scene.set_image_opacity(opacity);
                    scene.set_picture(if premultiplied {
                        Image::from_rgba8_premultiplied(source.clone())
                    } else {
                        Image::from_rgba8(source.clone())
                    });
                    // Repeat to exercise a stable source, then change it (COW).
                    for changed in [false, true] {
                        if changed {
                            source.make_mut_slice()[1025] = Rgba8Pixel {
                                r: 0,
                                g: 0,
                                b: 0,
                                a: 0,
                            };
                            scene.set_picture(if premultiplied {
                                Image::from_rgba8_premultiplied(source.clone())
                            } else {
                                Image::from_rgba8(source.clone())
                            });
                        }
                        let mut expected = allocate(192 * 144).unwrap();
                        window.request_redraw();
                        window.draw_if_needed(|renderer| {
                            renderer.render(&mut expected, 192);
                            buffer.render(renderer).unwrap();
                        });
                        assert!(
                            buffer.texture_hits > 0,
                            "1:1 images should use the direct path"
                        );
                        for (a, b) in expected.iter().zip(buffer.pixels()) {
                            assert_eq!(
                                [a.red, a.green, a.blue, a.alpha],
                                [b.red, b.green, b.blue, b.alpha],
                                "premultiplied={premultiplied} opaque={opaque} opacity={opacity} changed={changed}"
                            );
                        }
                    }
                }
            }
        }
    }
    scene.set_picture(Image::default());
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        buffer.render(renderer).unwrap();
    });
    assert!(
        buffer.opaque_background.is_none(),
        "hidden source must not retain image storage"
    );
}

slint::slint! {
    export component LargeTextureScene inherits Window {
        width: 320px; height: 240px; background: #123456;
        in property <image> picture;
        Image { x: 7px; y: 9px; width: 300px; height: 160px; source: root.picture; }
    }
}

#[test]
fn failed_texture_stops_cpu_access_and_repeated_submissions() {
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
    struct FailingTexture(FailingCanvas);
    impl Canvas for FailingTexture {
        fn pixels(&self) -> &[PremultipliedRgbaColor] {
            self.0.pixels()
        }
        fn pixels_mut(&mut self) -> &mut [PremultipliedRgbaColor] {
            self.0.pixels_mut()
        }
        fn texture(
            &mut self,
            _: &DrawTextureArgs,
            rects: &[Rect],
            _: usize,
            _: bool,
        ) -> Result<bool, String> {
            self.0.fill(PremultipliedRgbaColor::default(), rects)
        }
    }
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let scene = LargeTextureScene::new().unwrap();
    let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(300, 160);
    pixels.make_mut_slice().fill(Rgba8Pixel {
        r: 120,
        g: 70,
        b: 40,
        a: 128,
    });
    scene.set_picture(Image::from_rgba8(pixels));
    scene.show().unwrap();
    window.set_size(slint::PhysicalSize::new(320, 240));
    let calls = Rc::new(Cell::new(0));
    let quarantined = Rc::new(Cell::new(false));
    let mut buffer = Buffer::new(320, 240).unwrap();
    buffer.canvas = Box::new(FailingTexture(FailingCanvas {
        pixels: allocate(320 * 240).unwrap(),
        calls: calls.clone(),
        uncertain: false,
        quarantined: quarantined.clone(),
    }));
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        assert!(matches!(
            buffer.render(renderer),
            Err(Error::AccelerationFailed(_))
        ));
        assert!(matches!(
            buffer.render(renderer),
            Err(Error::AccelerationFailed(_))
        ));
    });
    assert_eq!(calls.get(), 1);
    buffer.line_slice(0).fill(PremultipliedRgbaColor::default());
    drop(buffer);
    assert!(quarantined.get());
}

#[test]
fn scaled_and_rotated_images_keep_the_software_path() {
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(RenderPlatform(window.clone()))).unwrap();
    let scene = TextureScene::new().unwrap();
    scene.show().unwrap();
    window.set_size(slint::PhysicalSize::new(192, 144));
    let mut src = SharedPixelBuffer::<Rgba8Pixel>::new(192, 144);
    for (i, p) in src.make_mut_slice().iter_mut().enumerate() {
        *p = Rgba8Pixel {
            r: (i * 3) as u8,
            g: (i * 7) as u8,
            b: (i * 11) as u8,
            a: (i * 13) as u8,
        };
    }
    scene.set_picture(Image::from_rgba8(src));
    for (rotation, width, height) in [
        (RenderingRotation::NoRotation, 192, 144),
        (RenderingRotation::Rotate90, 144, 192),
        (RenderingRotation::Rotate180, 192, 144),
        (RenderingRotation::Rotate270, 144, 192),
    ] {
        for zoom in [0.5, 1., 1.25] {
            if rotation == RenderingRotation::NoRotation && zoom == 1. {
                continue;
            }
            scene.set_zoom(zoom);
            let mut buffer = Buffer::new(width, height).unwrap();
            let mut expected = allocate(192 * 144).unwrap();
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.set_rendering_rotation(rotation);
                renderer.render(&mut expected, width as usize);
                buffer.render(renderer).unwrap();
            });
            assert_eq!(buffer.texture_hits, 0, "{rotation:?} zoom={zoom}");
            for (a, b) in expected.iter().zip(buffer.pixels()) {
                assert_eq!(
                    [a.red, a.green, a.blue, a.alpha],
                    [b.red, b.green, b.blue, b.alpha]
                );
            }
        }
    }
}
