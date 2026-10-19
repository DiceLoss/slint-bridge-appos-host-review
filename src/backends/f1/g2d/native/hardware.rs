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
