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
