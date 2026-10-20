use super::*;
use std::cell::RefCell;

fn inspected() -> (FixedInfo, VariableInfo) {
    let (mut f, v) = crate::tests::info(1024, 600, 32);
    f.smem_start = 0x4310_0000;
    f.line_length = 4096;
    f.smem_len = 4_915_200;
    f.xpanstep = 1;
    f.ypanstep = 1;
    (f, v)
}

#[test]
fn only_inspected_firmware_and_two_page_layout_are_admitted() {
    let version = "#1 Fri Mar 25 14:53:50 CST 2022";
    assert!(firmware("3.4.39\n", version, "Hardware\t: sun8i\n"));
    assert!(!firmware("3.4.40", version, "Hardware: sun8i"));
    assert!(!firmware("3.4.39", "different", "Hardware: sun8i"));
    assert!(!firmware("3.4.39", version, "Hardware: other"));
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
    v.yoffset = 1;
    assert!(validate_layout(&f, &v).is_err());
    v.yoffset = 0;
    v.reserved[1] = 1;
    assert!(validate_layout(&f, &v).is_err());
    v.reserved[1] = 0;
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
fn pan_and_readback_stop_at_the_first_visible_error() {
    for failure in [None, Some(0), Some(1)] {
        let calls = RefCell::new(Vec::new());
        let operation = |index| {
            calls.borrow_mut().push(index);
            if failure == Some(index) {
                Err(io::Error::other("injected"))
            } else {
                Ok(())
            }
        };
        let result = reference_sequence(|| operation(0), || operation(1));
        assert_eq!(result.is_err(), failure.is_some());
        assert_eq!(
            *calls.borrow(),
            (0..=failure.unwrap_or(1)).collect::<Vec<_>>()
        );
    }
}
