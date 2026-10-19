use super::*;
use std::cell::RefCell;

fn inspected() -> (FixedInfo, VariableInfo) {
    let (mut f, v) = crate::tests::info(1024, 600, 32);
    f.smem_start = 0x4244_0000;
    f.line_length = 4096;
    f.smem_len = 4_915_200;
    f.ypanstep = 1;
    for (dst, src) in f.id.iter_mut().zip(b"aicfb0\0") {
        *dst = libc::c_char::from_ne_bytes([*src]);
    }
    (f, v)
}

#[test]
fn only_inspected_firmware_and_two_page_layout_are_admitted() {
    assert!(firmware(
        "5.10.44\n",
        "#1 Fri May 22 02:04:02 UTC 2026\n",
        b"artinchip,d211\0"
    ));
    assert!(!firmware("5.10.44", "different", b"artinchip,d211\0"));
    assert!(!firmware(
        "5.10.44",
        "#1 Fri May 22 02:04:02 UTC 2026",
        b"artinchip,d213\0"
    ));
    for y in [0, 600] {
        let (f, mut v) = inspected();
        v.yoffset = y;
        assert_eq!(
            validate_layout(&f, &v).unwrap().initial_page,
            (y / 600) as usize
        );
    }
    let (mut f, mut v) = inspected();
    v.yres_virtual = 600;
    assert!(validate_layout(&f, &v).is_err());
    v.yres_virtual = 1200;
    v.reserved[0] = 3;
    assert!(validate_layout(&f, &v).is_err());
    v.reserved[0] = 0;
    f.line_length = 4104;
    assert!(validate_layout(&f, &v).is_err());
    f.line_length = 4096;
    f.smem_start += 4096;
    assert!(validate_layout(&f, &v).is_err());
    f.smem_start -= 4096;
    std::mem::swap(&mut v.red, &mut v.blue);
    assert!(validate_layout(&f, &v).is_err());
}

#[test]
fn pan_wait_and_readback_are_ordered_and_fail_closed() {
    for failure in [None, Some(0), Some(1), Some(2)] {
        let calls = RefCell::new(Vec::new());
        let operation = |index| {
            calls.borrow_mut().push(index);
            if failure == Some(index) {
                Err(io::Error::other("injected"))
            } else {
                Ok(())
            }
        };
        let result = reference_sequence(|| operation(0), || operation(1), || operation(2));
        assert_eq!(result.is_err(), failure.is_some());
        assert_eq!(
            *calls.borrow(),
            (0..=failure.unwrap_or(2)).collect::<Vec<_>>()
        );
    }
}
