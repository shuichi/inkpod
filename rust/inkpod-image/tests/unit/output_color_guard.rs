use super::*;

#[test]
fn bt709_guard_has_exact_inclusive_boundaries_and_transparent_skip() {
    for value in [16_u16, 235] {
        let promoted = value * 257;
        assert_eq!(
            bt709_conservative_guard_category(PixelValue::Rgba([
                value as u8,
                value as u8,
                value as u8,
                255,
            ]))
            .unwrap(),
            OutputColorGuardCategory::Safe
        );
        assert_eq!(
            bt709_conservative_guard_category(PixelValue::Rgba16([
                promoted, promoted, promoted, 65_535,
            ]))
            .unwrap(),
            OutputColorGuardCategory::Safe
        );
    }

    for value in [15_u16, 236] {
        let promoted = value * 257;
        assert_eq!(
            bt709_conservative_guard_category(PixelValue::Rgba([
                value as u8,
                value as u8,
                value as u8,
                255,
            ]))
            .unwrap(),
            OutputColorGuardCategory::Outside
        );
        assert_eq!(
            bt709_conservative_guard_category(PixelValue::Rgba16([
                promoted, promoted, promoted, 65_535,
            ]))
            .unwrap(),
            OutputColorGuardCategory::Outside
        );
    }

    assert_eq!(
        bt709_conservative_guard_category(PixelValue::Rgba([255, 0, 0, 0])).unwrap(),
        OutputColorGuardCategory::Transparent
    );
    assert_eq!(
        bt709_conservative_guard_category(PixelValue::Rgba16([65_535, 0, 0, 0])).unwrap(),
        OutputColorGuardCategory::Transparent
    );
}

#[test]
fn bt709_guard_uses_fixed_half_up_luma_and_chroma_without_premultiplication() {
    let red = bt709_conservative_ycbcr16([65_535, 0, 0, 32_768])
        .unwrap()
        .expect("positive alpha is inspected");
    assert_eq!(red.y_prime, 13_933);
    assert_eq!(red.cr, 65_535);
    assert_eq!(
        bt709_conservative_guard_category(PixelValue::Rgba16([65_535, 0, 0, 32_768,])).unwrap(),
        OutputColorGuardCategory::Outside
    );

    let promoted = [32_u16 * 257, 128_u16 * 257, 64_u16 * 257, 1];
    let eight = bt709_conservative_ycbcr16([32 * 257, 128 * 257, 64 * 257, 65_535])
        .unwrap()
        .unwrap();
    let sixteen = bt709_conservative_ycbcr16(promoted).unwrap().unwrap();
    assert_eq!(eight, sixteen);
}
