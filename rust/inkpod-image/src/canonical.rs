//! Canonical fixed-point, rounding, geometry, alpha, and color operations.
//!
//! These routines are the single numeric authority for image-result semantics.
//! They use integer arithmetic after exact IEEE-754 decomposition and therefore
//! do not depend on the host rounding mode, `libm`, SIMD width, or architecture.

/// Fraction bits in canonical signed document coordinates.
pub const CANONICAL_DOCUMENT_FRACTION_BITS: u32 = 16;
/// Canonical units in one document pixel.
pub const CANONICAL_DOCUMENT_ONE: i64 = 1_i64 << CANONICAL_DOCUMENT_FRACTION_BITS;
/// One in the signed Q30 trigonometric representation.
pub const Q30_ONE: i64 = 1_i64 << 30;

const QUARTER_TURN: u32 = 1_u32 << 30;
const CORDIC_GAIN_INVERSE_Q30: i64 = 652_032_874;
const CORDIC_ATAN_TURNS: [i64; 31] = [
    536_870_912,
    316_933_406,
    167_458_907,
    85_004_756,
    42_667_331,
    21_354_465,
    10_679_838,
    5_340_245,
    2_670_163,
    1_335_087,
    667_544,
    333_772,
    166_886,
    83_443,
    41_722,
    20_861,
    10_430,
    5_215,
    2_608,
    1_304,
    652,
    326,
    163,
    81,
    41,
    20,
    10,
    5,
    3,
    1,
    1,
];
const Q48_ONE: u64 = 1_u64 << 48;
const EXP2_FRACTION_Q48: [u64; 32] = [
    398_065_729_532_861,
    334_732_044_999_537,
    306_950_638_654_744,
    293_936_938_588_305,
    287_638_476_118_103,
    284_540_038_248_454,
    283_003_357_999_923,
    282_238_132_792_268,
    281_856_296_460_737,
    281_665_572_056_717,
    281_570_258_256_901,
    281_522_613_452_764,
    281_498_794_074_042,
    281_486_885_140_443,
    281_480_930_862_574,
    281_477_953_770_871,
    281_476_465_236_828,
    281_475_720_972_758,
    281_475_348_841_461,
    281_475_162_775_997,
    281_475_069_743_311,
    281_475_023_226_980,
    281_474_999_968_817,
    281_474_988_339_736,
    281_474_982_525_196,
    281_474_979_617_926,
    281_474_978_164_291,
    281_474_977_437_474,
    281_474_977_074_065,
    281_474_976_892_360,
    281_474_976_801_508,
    281_474_976_756_082,
];

/// Divides with round-to-nearest, ties-to-even.
///
/// `denominator` must be positive. `None` reports a zero denominator or a
/// quotient that cannot be represented as `i128`.
#[must_use]
pub fn div_round_ties_even_i128(numerator: i128, denominator: i128) -> Option<i128> {
    if denominator <= 0 {
        return None;
    }
    let negative = numerator.is_negative();
    let magnitude = numerator.unsigned_abs();
    let denominator = denominator as u128;
    let quotient = magnitude / denominator;
    let remainder = magnitude % denominator;
    let doubled = remainder.checked_mul(2);
    let increment = match doubled {
        Some(value) => value > denominator || (value == denominator && quotient & 1 == 1),
        None => true,
    };
    let rounded = quotient.checked_add(u128::from(increment))?;
    if negative {
        if rounded == (1_u128 << 127) {
            Some(i128::MIN)
        } else {
            i128::try_from(rounded).ok().map(|value| -value)
        }
    } else {
        i128::try_from(rounded).ok()
    }
}

/// Mathematical floor division for a positive denominator.
#[must_use]
pub const fn floor_div_i128(numerator: i128, denominator: i128) -> Option<i128> {
    if denominator <= 0 {
        return None;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    Some(if remainder < 0 {
        quotient - 1
    } else {
        quotient
    })
}

/// Mathematical ceiling division for a positive denominator.
#[must_use]
pub const fn ceil_div_i128(numerator: i128, denominator: i128) -> Option<i128> {
    if denominator <= 0 {
        return None;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    Some(if remainder > 0 {
        quotient + 1
    } else {
        quotient
    })
}

/// Returns `floor(sqrt(value))` using a bounded integer algorithm.
#[must_use]
pub const fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut bit = 1_u128 << 126;
    while bit > value {
        bit >>= 2;
    }
    let mut remainder = value;
    let mut root = 0_u128;
    while bit != 0 {
        if remainder >= root + bit {
            remainder -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}

/// Raises a normalized u16 value to one positive rational power.
///
/// The log2/exp2 implementation uses fixed 32- and 48-bit iterations and a
/// frozen constant table. It is intended for canonical image-result curves,
/// not general-purpose floating-point mathematics.
#[must_use]
pub fn canonical_pow_unit_u16(
    value: u16,
    exponent_numerator: u32,
    exponent_denominator: u32,
) -> Option<u16> {
    if exponent_denominator == 0 {
        return None;
    }
    if value == 0 {
        return Some(0);
    }
    if value == u16::MAX || exponent_numerator == 0 {
        return Some(u16::MAX);
    }
    let mut normalized = div_round_ties_even_i128(
        i128::from(value) * i128::from(Q48_ONE),
        i128::from(u16::MAX),
    )? as u64;
    let leading_shift = normalized.leading_zeros().saturating_sub(15);
    normalized <<= leading_shift;
    let mut log2_q32 = -i64::from(leading_shift) * (1_i64 << 32);
    for bit in (0..32).rev() {
        let squared = (u128::from(normalized) * u128::from(normalized)) >> 48;
        normalized = squared as u64;
        if normalized >= (Q48_ONE << 1) {
            normalized >>= 1;
            log2_q32 += 1_i64 << bit;
        }
    }
    let exponent_q32 = div_round_ties_even_i128(
        i128::from(log2_q32) * i128::from(exponent_numerator),
        i128::from(exponent_denominator),
    )?;
    let integer = exponent_q32.div_euclid(1_i128 << 32);
    if integer < -63 {
        return Some(0);
    }
    let fraction = exponent_q32.rem_euclid(1_i128 << 32) as u32;
    let mut result = Q48_ONE;
    for (index, factor) in EXP2_FRACTION_Q48.into_iter().enumerate() {
        if fraction & (1_u32 << (31 - index)) != 0 {
            result = div_round_ties_even_i128(
                i128::from(result) * i128::from(factor),
                i128::from(Q48_ONE),
            )? as u64;
        }
    }
    if integer < 0 {
        let divisor = 1_i128.checked_shl((-integer) as u32)?;
        result = div_round_ties_even_i128(i128::from(result), divisor)? as u64;
    } else {
        result = result.checked_shl(integer as u32)?;
    }
    div_round_ties_even_i128(
        i128::from(result) * i128::from(u16::MAX),
        i128::from(Q48_ONE),
    )?
    .clamp(0, i128::from(u16::MAX))
    .try_into()
    .ok()
}

/// Converts finite binary32 input to a signed scaled integer exactly, with
/// round-to-nearest, ties-to-even. Negative zero becomes zero.
#[must_use]
pub fn canonical_scaled_i64_from_f32(
    value: f32,
    scale_numerator: u64,
    scale_denominator: u64,
) -> Option<i64> {
    let bits = value.to_bits();
    let exponent_bits = (bits >> 23) & 0xff;
    if exponent_bits == 0xff || scale_denominator == 0 {
        return None;
    }
    let fraction = u128::from(bits & 0x7f_ffff);
    let (significand, exponent) = if exponent_bits == 0 {
        (fraction, -149)
    } else {
        ((1_u128 << 23) | fraction, exponent_bits as i32 - 150)
    };
    scaled_from_parts(
        bits >> 31 != 0,
        significand,
        exponent,
        scale_numerator,
        scale_denominator,
    )
}

/// Converts finite binary64 input to a signed scaled integer exactly, with
/// round-to-nearest, ties-to-even. Negative zero becomes zero.
#[must_use]
pub fn canonical_scaled_i64_from_f64(
    value: f64,
    scale_numerator: u64,
    scale_denominator: u64,
) -> Option<i64> {
    let bits = value.to_bits();
    let exponent_bits = (bits >> 52) & 0x7ff;
    if exponent_bits == 0x7ff || scale_denominator == 0 {
        return None;
    }
    let fraction = u128::from(bits & 0x000f_ffff_ffff_ffff);
    let (significand, exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        ((1_u128 << 52) | fraction, exponent_bits as i32 - 1075)
    };
    scaled_from_parts(
        bits >> 63 != 0,
        significand,
        exponent,
        scale_numerator,
        scale_denominator,
    )
}

fn scaled_from_parts(
    negative: bool,
    significand: u128,
    binary_exponent: i32,
    scale_numerator: u64,
    scale_denominator: u64,
) -> Option<i64> {
    if significand == 0 || scale_numerator == 0 {
        return Some(0);
    }
    let scaled = significand.checked_mul(u128::from(scale_numerator))?;
    let (numerator, denominator) = if binary_exponent >= 0 {
        (
            scaled.checked_shl(binary_exponent as u32)?,
            u128::from(scale_denominator),
        )
    } else {
        let shift = binary_exponent.unsigned_abs();
        if shift >= 128 {
            // The significand and u64 scale product is below 2^117, so it is
            // strictly below half of any denominator with at least 2^128.
            return Some(0);
        }
        (scaled, u128::from(scale_denominator).checked_shl(shift)?)
    };
    signed_rounded_ratio(negative, numerator, denominator)
}

fn signed_rounded_ratio(negative: bool, numerator: u128, denominator: u128) -> Option<i64> {
    if denominator == 0 {
        return None;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let doubled = remainder.checked_mul(2);
    let increment = match doubled {
        Some(value) => value > denominator || (value == denominator && quotient & 1 == 1),
        None => true,
    };
    let magnitude = quotient.checked_add(u128::from(increment))?;
    if negative {
        if magnitude == i64::MAX as u128 + 1 {
            Some(i64::MIN)
        } else {
            i64::try_from(magnitude).ok().map(|value| -value)
        }
    } else {
        i64::try_from(magnitude).ok()
    }
}

/// Converts a finite binary32 document coordinate to signed Q16.
#[must_use]
pub fn canonical_q16_from_f32(value: f32) -> Option<i64> {
    canonical_scaled_i64_from_f32(value, CANONICAL_DOCUMENT_ONE as u64, 1)
}

/// Converts a finite binary64 document coordinate to signed Q16.
#[must_use]
pub fn canonical_q16_from_f64(value: f64) -> Option<i64> {
    canonical_scaled_i64_from_f64(value, CANONICAL_DOCUMENT_ONE as u64, 1)
}

/// Converts normalized binary32 input to `0..=65_535` with ties-to-even.
#[must_use]
pub fn canonical_unit_u16_from_f32(value: f32) -> Option<u16> {
    if !(0.0..=1.0).contains(&value) {
        return None;
    }
    canonical_scaled_i64_from_f32(value, u16::MAX.into(), 1).and_then(|value| value.try_into().ok())
}

/// Converts degrees to modulo-`2^32` canonical turns without `libm`.
#[must_use]
pub fn canonical_turns_from_degrees_f64(value: f64) -> Option<u32> {
    let turns = canonical_scaled_i64_from_f64(value, 1_u64 << 32, 360)?;
    Some(turns.rem_euclid(1_i64 << 32) as u32)
}

/// Returns deterministic `(sin, cos)` in signed Q30 for canonical turns.
#[must_use]
pub fn sin_cos_turns_q30(turns: u32) -> (i64, i64) {
    match turns {
        0 => return (0, Q30_ONE),
        QUARTER_TURN => return (Q30_ONE, 0),
        0x8000_0000 => return (0, -Q30_ONE),
        0xc000_0000 => return (-Q30_ONE, 0),
        _ => {}
    }
    let quadrant = turns >> 30;
    let offset = turns & (QUARTER_TURN - 1);
    let first_quadrant = if quadrant & 1 == 0 {
        offset
    } else {
        QUARTER_TURN - offset
    };
    let mut x = CORDIC_GAIN_INVERSE_Q30;
    let mut y = 0_i64;
    let mut z = i64::from(first_quadrant);
    for (shift, angle) in CORDIC_ATAN_TURNS.into_iter().enumerate() {
        let old_x = x;
        if z >= 0 {
            x -= y >> shift;
            y += old_x >> shift;
            z -= angle;
        } else {
            x += y >> shift;
            y -= old_x >> shift;
            z += angle;
        }
    }
    match quadrant {
        0 => (y, x),
        1 => (y, -x),
        2 => (-y, -x),
        _ => (-y, x),
    }
}

/// Rotates one signed Q16 vector by canonical turns and rounds each component
/// to nearest, ties-to-even.
#[must_use]
pub fn rotate_q16(x: i64, y: i64, turns: u32) -> Option<(i64, i64)> {
    let (sine, cosine) = sin_cos_turns_q30(turns);
    let output_x = i128::from(x)
        .checked_mul(i128::from(cosine))?
        .checked_sub(i128::from(y).checked_mul(i128::from(sine))?)?;
    let output_y = i128::from(x)
        .checked_mul(i128::from(sine))?
        .checked_add(i128::from(y).checked_mul(i128::from(cosine))?)?;
    Some((
        div_round_ties_even_i128(output_x, i128::from(Q30_ONE))?
            .try_into()
            .ok()?,
        div_round_ties_even_i128(output_y, i128::from(Q30_ONE))?
            .try_into()
            .ok()?,
    ))
}

/// Applies straight-alpha source-over to exact RGBA16 values.
#[must_use]
pub fn source_over_rgba16(background: [u16; 4], foreground: [u16; 4]) -> [u16; 4] {
    let foreground_alpha = u64::from(foreground[3]);
    let background_alpha = u64::from(background[3]);
    let inverse = u64::from(u16::MAX) - foreground_alpha;
    let output_alpha = foreground_alpha
        + div_round_ties_even_i128(i128::from(background_alpha * inverse), i128::from(u16::MAX))
            .expect("bounded alpha quotient") as u64;
    if output_alpha == 0 {
        return [0; 4];
    }
    let mut output = [0_u16; 4];
    output[3] = output_alpha as u16;
    for channel in 0..3 {
        let foreground_premultiplied = u64::from(foreground[channel]) * foreground_alpha;
        let background_premultiplied = u64::from(background[channel]) * background_alpha;
        let retained_background = div_round_ties_even_i128(
            i128::from(background_premultiplied * inverse),
            i128::from(u16::MAX),
        )
        .expect("bounded color quotient") as u64;
        output[channel] = div_round_ties_even_i128(
            i128::from(foreground_premultiplied + retained_background),
            i128::from(output_alpha),
        )
        .expect("positive output alpha") as u16;
    }
    output
}

/// Applies straight-alpha source-over to exact RGBA8 values.
#[must_use]
pub fn source_over_rgba8(background: [u8; 4], foreground: [u8; 4]) -> [u8; 4] {
    let foreground_alpha = u32::from(foreground[3]);
    let background_alpha = u32::from(background[3]);
    if foreground_alpha == 0 {
        return if background_alpha == 0 {
            [0; 4]
        } else {
            background
        };
    }
    if foreground_alpha == u32::from(u8::MAX) || background_alpha == 0 {
        return foreground;
    }
    let inverse = u32::from(u8::MAX) - foreground_alpha;
    let output_alpha = foreground_alpha + (background_alpha * inverse + 127) / 255;
    let channel = |index: usize| -> u8 {
        let foreground_premultiplied = u32::from(foreground[index]) * foreground_alpha;
        let background_premultiplied = u32::from(background[index]) * background_alpha;
        ((foreground_premultiplied
            + (background_premultiplied * inverse + 127) / 255
            + output_alpha / 2)
            / output_alpha) as u8
    };
    [channel(0), channel(1), channel(2), output_alpha as u8]
}

/// Premultiplies one 8-bit channel using nearest integer rounding.
#[must_use]
pub const fn premultiply_u8(channel: u8, alpha: u8) -> u8 {
    ((channel as u32 * alpha as u32 + 127) / 255) as u8
}

/// Tests the canonical fill color distance: every normalized RGBA16 channel
/// must be within the inclusive per-channel tolerance.
#[must_use]
pub fn color_within_tolerance(left: [u16; 4], right: [u16; 4], tolerance: u16) -> bool {
    left.into_iter()
        .zip(right)
        .all(|(left, right)| left.abs_diff(right) <= tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ieee_canonicalization_uses_ties_to_even_and_rejects_nonfinite_values() {
        assert_eq!(canonical_q16_from_f32(0.5 / 65_536.0), Some(0));
        assert_eq!(canonical_q16_from_f32(1.5 / 65_536.0), Some(2));
        assert_eq!(canonical_q16_from_f32(-1.5 / 65_536.0), Some(-2));
        assert_eq!(canonical_q16_from_f32(-0.0), Some(0));
        assert_eq!(canonical_unit_u16_from_f32(0.5), Some(32_768));
        assert_eq!(canonical_unit_u16_from_f32(1.0), Some(65_535));
        assert_eq!(canonical_q16_from_f32(f32::INFINITY), None);
        assert_eq!(canonical_scaled_i64_from_f64(f64::NAN, 1, 1), None);
    }

    #[test]
    fn integer_sqrt_and_signed_division_cover_boundaries() {
        assert_eq!(integer_sqrt(0), 0);
        assert_eq!(integer_sqrt(15), 3);
        assert_eq!(integer_sqrt(16), 4);
        assert_eq!(integer_sqrt(u128::MAX), u64::MAX.into());
        assert_eq!(floor_div_i128(-1, 65_536), Some(-1));
        assert_eq!(ceil_div_i128(-1, 65_536), Some(0));
        assert_eq!(div_round_ties_even_i128(5, 2), Some(2));
        assert_eq!(div_round_ties_even_i128(7, 2), Some(4));
        assert_eq!(div_round_ties_even_i128(-7, 2), Some(-4));
    }

    #[test]
    fn cordic_axes_and_rotation_are_closed() {
        assert_eq!(sin_cos_turns_q30(0), (0, Q30_ONE));
        assert_eq!(sin_cos_turns_q30(1 << 30), (Q30_ONE, 0));
        assert_eq!(
            rotate_q16(CANONICAL_DOCUMENT_ONE, 0, 1 << 30),
            Some((0, CANONICAL_DOCUMENT_ONE))
        );
        assert_eq!(canonical_turns_from_degrees_f64(90.0), Some(1 << 30));
        assert_eq!(canonical_turns_from_degrees_f64(-90.0), Some(0xc000_0000));
    }

    #[test]
    fn alpha_and_color_distance_have_exact_integer_results() {
        assert_eq!(
            source_over_rgba16([0; 4], [1, 2, 3, u16::MAX]),
            [1, 2, 3, u16::MAX]
        );
        assert_eq!(
            source_over_rgba8([0; 4], [1, 2, 3, u8::MAX]),
            [1, 2, 3, u8::MAX]
        );
        assert_eq!(premultiply_u8(255, 128), 128);
        assert!(color_within_tolerance([1, 2, 3, 4], [2, 3, 4, 5], 1));
        assert!(!color_within_tolerance([1, 2, 3, 4], [3, 3, 4, 5], 1));
    }

    #[test]
    fn fixed_power_has_locked_endpoints_and_gamma_direction() {
        assert_eq!(canonical_pow_unit_u16(0, 5, 6), Some(0));
        assert_eq!(canonical_pow_unit_u16(u16::MAX, 5, 6), Some(u16::MAX));
        let midpoint = canonical_pow_unit_u16(32_768, 5, 6).unwrap();
        assert!(midpoint > 32_768);
    }
}
