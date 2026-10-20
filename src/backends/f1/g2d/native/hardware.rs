//! Explicitly selected board test. Never opens framebuffer or submits a PAN.
use super::*;
use std::time::Instant;

fn pattern(index: usize, round: u32) -> PremultipliedRgbaColor {
    let alpha = ((index * 7 + round as usize * 13) % 256) as u8;
    let limit = usize::from(alpha) + 1;
    PremultipliedRgbaColor {
        red: ((index * 3 + 17) % limit) as u8,
        green: ((index * 5 + round as usize) % limit) as u8,
        blue: ((index * 11 + 23) % limit) as u8,
        alpha,
    }
}

fn bytes(pixel: PremultipliedRgbaColor) -> [u8; 4] {
    [pixel.red, pixel.green, pixel.blue, pixel.alpha]
}

#[test]
#[ignore = "F1-only offscreen GE/cache test; SD deployment and ownership preflight required"]
fn offscreen_fill_cache() {
    let started = Instant::now();
    let mut total_pixels = 0usize;
    for (width, height) in [(64, 32), (256, 128), (1024, 600)] {
        eprintln!(
            "F1_GE_BEGIN size={width}x{height} bytes={}",
            width * height * 4
        );
        // Directly require native GE. A heap fallback must not pass this test.
        let mut native = Native::new(width, height).expect("native GE allocation/init");
        let phase = Instant::now();
        let mut checksum = 0xcbf2_9ce4_8422_2325u64;
        for round in 0..16u32 {
            // Dirty every CPU cache line, including pixels GE must not modify.
            // Each round follows readback of the previous hardware result.
            for (i, pixel) in native.pixels_mut().iter_mut().enumerate() {
                *pixel = pattern(i, round);
            }
            let rects = match round % 4 {
                0 => vec![Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                }],
                1 => vec![Rect {
                    x: 3,
                    y: 5,
                    width: width - 7,
                    height: height - 11,
                }],
                2 => vec![
                    Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    Rect {
                        x: width - 1,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    Rect {
                        x: 0,
                        y: height - 1,
                        width: 1,
                        height: 1,
                    },
                    Rect {
                        x: width - 1,
                        y: height - 1,
                        width: 1,
                        height: 1,
                    },
                ],
                _ => vec![
                    Rect {
                        x: 0,
                        y: height / 2,
                        width,
                        height: 1,
                    },
                    Rect {
                        x: width / 2,
                        y: 0,
                        width: 1,
                        height,
                    },
                ],
            };
            let color = PremultipliedRgbaColor {
                red: (round * 17) as u8,
                green: (255 - round * 11) as u8,
                blue: (round * 7 + 31) as u8,
                alpha: 255,
            };
            if let Err(error) = native.fill(color, &rects) {
                if native.buffer.quarantined {
                    // Exiting the test process would close even leaked FDs.
                    // Keep this owner alive for explicit board recovery; the
                    // host must stop testing, not kill this process on timeout.
                    eprintln!(
                        "F1_GE_QUARANTINED pid={} size={width}x{height} round={round} error={error}",
                        std::process::id()
                    );
                    loop {
                        std::thread::park();
                    }
                }
                panic!("GE failed before submission: {error}");
            }
            for (i, actual) in native.pixels().iter().enumerate() {
                let x = i as u32 % width;
                let y = i as u32 / width;
                let covered = rects
                    .iter()
                    .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height);
                let expected = if covered { color } else { pattern(i, round) };
                assert_eq!(
                    bytes(*actual),
                    bytes(expected),
                    "size={width}x{height} round={round} pixel={x},{y}"
                );
                for byte in bytes(*actual) {
                    checksum = (checksum ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
                }
            }
            total_pixels += (width * height) as usize;
        }
        eprintln!(
            "F1_GE_PASS size={width}x{height} rounds=16 checksum={checksum:016x} elapsed_ms={}",
            phase.elapsed().as_millis()
        );
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status
                .lines()
                .filter(|l| l.starts_with("VmRSS:") || l.starts_with("VmHWM:"))
            {
                eprintln!("{line}");
            }
        }
        // Successful completion permits ordinary DMA-BUF and GE cleanup before
        // the next size; at most one canvas is resident at a time.
        drop(native);
    }
    eprintln!(
        "F1_GE_DONE checked_pixels={total_pixels} elapsed_ms={}",
        started.elapsed().as_millis()
    );
}

slint::slint! {
    export component BlendScene inherits Window {
        width: 320px; height: 240px;
        background: base;
        in property <color> base: #234567;
        in property <image> picture;
        in property <float> opacity-value: 1;
        in property <length> picture-width: 300px;
        in property <length> picture-height: 160px;
        Rectangle { x: 9px; y: 7px; width: 281px; height: 171px; clip: true;
            Image { x: -5px; y: -3px; width: root.picture-width; height: root.picture-height;
                source: root.picture; opacity: root.opacity-value; image-fit: fill; }
        }
    }
}

#[test]
#[ignore = "F1-only offscreen image/cache test; SD deployment and no display test load"]
fn offscreen_texture_cache() {
    use super::super::{Buffer, DrawTextureArgs};
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
    use std::{cell::Cell, rc::Rc};
    struct Platform(Rc<MinimalSoftwareWindow>);
    impl slint::platform::Platform for Platform {
        fn create_window_adapter(
            &self,
        ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
            Ok(self.0.clone())
        }
    }
    struct Observed {
        native: Native,
        calls: Rc<Cell<usize>>,
        uploads: Rc<Cell<usize>>,
        source_bytes: Rc<Cell<usize>>,
    }
    impl Canvas for Observed {
        fn pixels(&self) -> &[PremultipliedRgbaColor] {
            self.native.pixels()
        }
        fn pixels_mut(&mut self) -> &mut [PremultipliedRgbaColor] {
            self.native.pixels_mut()
        }
        fn texture(
            &mut self,
            args: &DrawTextureArgs,
            rects: &[Rect],
            slot: usize,
            upload: bool,
        ) -> Result<bool, String> {
            let result = self.native.texture(args, rects, slot, upload);

            if let Err(error) = &result
                && self.native.buffer.quarantined
            {
                eprintln!("F1_GE_QUARANTINED pid={} error={error}", std::process::id());
                loop {
                    std::thread::park();
                }
            }
            let bytes = self
                .native
                .sources
                .iter()
                .flatten()
                .map(|s| s.count * 4)
                .sum();
            self.source_bytes.set(self.source_bytes.get().max(bytes));
            assert!(bytes <= 1024 * 1024, "GE source allocation exceeded 1 MiB");

            if matches!(result, Ok(true)) {
                self.calls.set(self.calls.get() + 1);
                self.uploads.set(self.uploads.get() + usize::from(upload));
            }
            result
        }
    }
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Platform(window.clone()))).unwrap();
    let scene = BlendScene::new().unwrap();
    scene.show().unwrap();
    window.set_size(slint::PhysicalSize::new(320, 240));
    let calls = Rc::new(Cell::new(0));
    let uploads = Rc::new(Cell::new(0));
    let source_bytes = Rc::new(Cell::new(0));
    let mut buffer = Buffer::new(320, 240).unwrap();
    buffer.canvas = Box::new(Observed {
        native: Native::new(320, 240).unwrap(),
        calls: calls.clone(),
        uploads: uploads.clone(),
        source_bytes: source_bytes.clone(),
    });
    let mut max_delta = 0u8;
    let mut frames = 0;
    for (width, height, fits) in [
        (300, 160, true),
        (379, 237, true),
        (381, 239, true),
        (511, 256, true),
        (255, 513, false),
    ] {
        scene.set_picture_width(width as f32);
        scene.set_picture_height(height as f32);
        for premul in [true, false] {
            for opaque_target in [true, false] {
                scene.set_base(slint::Color::from_argb_u8(
                    if opaque_target { 255 } else { 128 },
                    35,
                    69,
                    103,
                ));
                let mut source = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
                for (i, p) in source.make_mut_slice().iter_mut().enumerate() {
                    let a = (i * 13 % 256) as u8;
                    let c = |v: u8| {
                        if premul {
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
                for opacity in [1., 0.5, 0.01, 0.99, 0.] {
                    scene.set_opacity_value(opacity);
                    scene.set_picture(if premul {
                        Image::from_rgba8_premultiplied(source.clone())
                    } else {
                        Image::from_rgba8(source.clone())
                    });
                    for pass in 0..3 {
                        if pass == 2 {
                            source.make_mut_slice()[1025] = Rgba8Pixel {
                                r: 0,
                                g: 0,
                                b: 0,
                                a: 0,
                            };
                            scene.set_picture(if premul {
                                Image::from_rgba8_premultiplied(source.clone())
                            } else {
                                Image::from_rgba8(source.clone())
                            });
                        }
                        let before = calls.get();
                        let mut expected = vec![PremultipliedRgbaColor::default(); 320 * 240];
                        window.request_redraw();
                        window.draw_if_needed(|renderer| {
                            renderer.render(&mut expected, 320);
                            buffer.render(renderer).unwrap();
                        });
                        if opaque_target && opacity >= 0.5 && fits {
                            assert!(
                                calls.get() > before,
                                "hardware path was not exercised: size={width}x{height} premul={premul} opaque={opaque_target} opacity={opacity} corner_alpha={}",
                                buffer.pixels()[0].alpha
                            );
                        }
                        if !opaque_target || !fits {
                            assert_eq!(
                                calls.get(),
                                before,
                                "transparent target or oversized padded source must use software"
                            );
                        }
                        for (a, b) in expected.iter().zip(buffer.pixels()) {
                            for (a, b) in bytes(*a).into_iter().zip(bytes(*b)) {
                                let d = a.abs_diff(b);
                                max_delta = max_delta.max(d);
                                assert!(
                                    d <= u8::from(opaque_target && fits),
                                    "delta={d} size={width}x{height} premul={premul} opaque={opaque_target} opacity={opacity}"
                                );
                            }
                        }
                        frames += 1;
                    }
                }
            }
        }
        eprintln!(
            "F1_GE_TEXTURE_SIZE_DONE width={width} height={height} source_bytes={}",
            source_bytes.get()
        );
    }
    assert!(
        calls.get() > uploads.get(),
        "stable images must reuse source DMA pixels"
    );
    eprintln!(
        "F1_GE_TEXTURE_DONE frames={frames} calls={} uploads={} max_delta={max_delta} max_source_bytes={}",
        calls.get(),
        uploads.get(),
        source_bytes.get()
    );
}
