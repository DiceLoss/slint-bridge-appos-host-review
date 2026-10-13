//! Physical writeback comparison, nested in framebuffer to exercise its real
//! mapping, layout conversion and blocking presentation lifecycle.
use super::*;
use std::{hint::black_box, os::unix::fs::FileExt, time::Instant};

#[test]
#[ignore = "requires exclusive NX5; checks sliced writes on real framebuffer pages"]
fn budgeted_shared_mmz_writeback_preserves_pixels() {
    let contract = crate::backends::nx5::open().unwrap();
    let path =
        std::env::var_os("SLINT_BRIDGE_TEST_FRAMEBUFFER").unwrap_or_else(|| "/dev/fb0".into());
    let mut framebuffer = Framebuffer::open(Path::new(&path), contract).unwrap();
    let layout = framebuffer.layout;
    let width = layout.size.width as usize;
    let height = layout.size.height as usize;
    let mut before = vec![0; layout.page_bytes];
    let mut actual = before.clone();
    for (frame, alpha) in [0, 127, 255, 127, 0, 255].into_iter().enumerate() {
        framebuffer.wait_for_present().unwrap();
        let front_offset = (framebuffer.front * layout.page_bytes) as u64;
        framebuffer
            ._file
            .read_exact_at(&mut before, front_offset)
            .unwrap();
        let color: PremultipliedRgbaColor =
            slint::Color::from_argb_u8(alpha, 217 - frame as u8 * 7, 93 + frame as u8 * 5, 41)
                .into();
        crate::backends::nx5::tde::tests::fill_shared_canvas(&mut framebuffer.retained, color);
        framebuffer.prepared_damage = Some(vec![Rect {
            x0: 0,
            y0: 0,
            x1: width,
            y1: height,
        }]);
        let mut slices = 1;
        assert!(!framebuffer.present(Some(Duration::ZERO)).unwrap());
        assert!(!framebuffer.can_render());
        framebuffer
            ._file
            .read_exact_at(&mut actual, front_offset)
            .unwrap();
        assert_eq!(actual, before, "partial write modified the front page");
        while !framebuffer.present(Some(Duration::ZERO)).unwrap() {
            slices += 1;
            assert!(slices <= height, "writeback made no progress");
        }
        slices += 1;
        framebuffer.wait_for_present().unwrap();
        framebuffer
            ._file
            .read_exact_at(&mut actual, (framebuffer.front * layout.page_bytes) as u64)
            .unwrap();
        let expected = layout.pixel(color).to_ne_bytes();
        for y in 0..height {
            for x in 0..width {
                let at = y * layout.stride + x * layout.bytes_per_pixel;
                assert_eq!(
                    &actual[at..at + layout.bytes_per_pixel],
                    &expected[..layout.bytes_per_pixel],
                    "frame={frame} at {x},{y}"
                );
            }
        }
        assert!(
            framebuffer
                .retained
                .pixels()
                .iter()
                .all(|p| [p.red, p.green, p.blue, p.alpha]
                    == [color.red, color.green, color.blue, color.alpha])
        );
        println!(
            "budgeted-writeback frame={frame} alpha={alpha} slices={slices} pixels={} front_unchanged=true source_unchanged=true actual_page_exact=true",
            width * height
        );
    }
}

#[test]
#[ignore = "requires exclusive NX5; writes and presents real framebuffer pages"]
fn shared_mmz_writeback_matches_staged_pixels() {
    let contract = crate::backends::nx5::open().unwrap();
    let path =
        std::env::var_os("SLINT_BRIDGE_TEST_FRAMEBUFFER").unwrap_or_else(|| "/dev/fb0".into());
    let mut framebuffer = Framebuffer::open(Path::new(&path), contract).unwrap();
    let layout = framebuffer.layout;
    let width = layout.size.width as usize;
    let height = layout.size.height as usize;
    let mut staging = vec![PremultipliedRgbaColor::default(); width * height];
    let mut actual = vec![0; layout.stride * height];
    for alpha in [0, 127, 255] {
        let color: PremultipliedRgbaColor = slint::Color::from_argb_u8(alpha, 217, 93, 41).into();
        let mut stage_us = [0.; 2];
        let mut write_us = [0.; 2];
        let mut total_us = [Vec::new(), Vec::new()];
        for iteration in 0..35 {
            for path in [iteration % 2, 1 - iteration % 2] {
                // Use the production retirement boundary before touching either
                // physical page. DMA below only prepares the private MMZ canvas.
                framebuffer.wait_for_present().unwrap();
                crate::backends::nx5::tde::tests::fill_shared_canvas(
                    &mut framebuffer.retained,
                    color,
                );
                let back = 1 - framebuffer.front;
                let started = Instant::now();
                let source = framebuffer.retained.pixels();
                let source = if path == 1 {
                    // Reproduce the previous MMZ-to-heap row readback before
                    // the same production format-conversion/write operation.
                    for (out, input) in staging.chunks_mut(width).zip(source.chunks(width)) {
                        out.copy_from_slice(black_box(input));
                    }
                    &staging[..]
                } else {
                    source
                };
                let staged = Instant::now();
                let mapped = framebuffer.mapping.bytes_mut();
                for y in 0..height {
                    let at = back * layout.page_bytes + y * layout.stride;
                    layout.write_row(
                        black_box(&source[y * width..(y + 1) * width]),
                        &mut mapped[at..at + width * layout.bytes_per_pixel],
                    );
                }
                fence(Ordering::SeqCst);
                let finished = Instant::now();
                if iteration >= 5 {
                    stage_us[path] += (staged - started).as_secs_f64() * 1e6;
                    write_us[path] += (finished - staged).as_secs_f64() * 1e6;
                    total_us[path].push((finished - started).as_secs_f64() * 1e6);
                }
                framebuffer
                    .presentation
                    .submit(back as u32 * layout.size.height)
                    .unwrap();
                framebuffer.pending_damage = Some(vec![Rect {
                    x0: 0,
                    y0: 0,
                    x1: width,
                    y1: height,
                }]);
                framebuffer.wait_for_present().unwrap();
                if iteration == 34 {
                    framebuffer
                        ._file
                        .read_exact_at(&mut actual, (back * layout.page_bytes) as u64)
                        .unwrap();
                    let expected = layout.pixel(color).to_ne_bytes();
                    for y in 0..height {
                        for x in 0..width {
                            let at = y * layout.stride + x * layout.bytes_per_pixel;
                            assert_eq!(
                                &actual[at..at + layout.bytes_per_pixel],
                                &expected[..layout.bytes_per_pixel],
                                "alpha={alpha} path={path} at {x},{y}"
                            );
                        }
                    }
                }
            }
        }
        for path in 0..2 {
            total_us[path].sort_by(f64::total_cmp);
            println!(
                "writeback path={} alpha={alpha} pixels={} samples=30 stage_mean_us={:.3} convert_write_mean_us={:.3} total_p50_us={:.3} total_p95_us={:.3} actual_page_exact=true",
                ["shared-mmz", "staged-heap"][path],
                width * height,
                stage_us[path] / 30.,
                write_us[path] / 30.,
                total_us[path][15],
                total_us[path][28]
            );
        }
    }
}
