//! Fixed-point conservative output-colour guard kernels.

use crate::{PixelValue, RasterError};

const CHANNEL_MAX: i64 = 65_535;
const LUMA_DENOMINATOR: i64 = 10_000;
const CB_DENOMINATOR: i64 = 18_556;
const CR_DENOMINATOR: i64 = 15_748;
const LUMA_MIN: u16 = 16 * 257;
const LUMA_MAX: u16 = 235 * 257;
const CHROMA_MIN: u16 = 16 * 257;
const CHROMA_MAX: u16 = 240 * 257;

/// Fixed 16-bit BT.709-derived code values used by inkpod's conservative guard.
///
/// These values are not a declaration of broadcast conformance. They are the
/// deterministic result of the native guard equations documented in `SPEC.md`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bt709Ycbcr16 {
    /// Luma-like code value using the BT.709 0.2126/0.7152/0.0722 coefficients.
    pub y_prime: u16,
    /// Blue-difference code value centered at half the 16-bit channel range.
    pub cb: u16,
    /// Red-difference code value centered at half the 16-bit channel range.
    pub cr: u16,
}

/// Classification of one straight-alpha pixel for the conservative guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputColorGuardCategory {
    /// Alpha is zero, so the stored RGB is intentionally not inspected.
    Transparent,
    /// Every derived component lies on or inside the inclusive native guard bounds.
    Safe,
    /// At least one derived component lies outside the native guard bounds.
    Outside,
}

/// Computes the guard's 16-bit BT.709-derived components from straight RGBA16.
///
/// Positive-alpha RGB is used without premultiplication. All operations are
/// checked fixed-width integer operations and positive division rounds half up.
/// Zero alpha returns `None` without inspecting RGB.
pub fn bt709_conservative_ycbcr16(rgba: [u16; 4]) -> Result<Option<Bt709Ycbcr16>, RasterError> {
    if rgba[3] == 0 {
        return Ok(None);
    }
    let red = i64::from(rgba[0]);
    let green = i64::from(rgba[1]);
    let blue = i64::from(rgba[2]);
    let y_numerator = 2_126_i64
        .checked_mul(red)
        .and_then(|value| value.checked_add(7_152_i64.checked_mul(green)?))
        .and_then(|value| value.checked_add(722_i64.checked_mul(blue)?))
        .ok_or(RasterError::InvalidDimensions)?;
    let y_prime = round_half_up(y_numerator, LUMA_DENOMINATOR)?;
    let cb_delta = LUMA_DENOMINATOR
        .checked_mul(blue)
        .and_then(|value| value.checked_sub(y_numerator))
        .ok_or(RasterError::InvalidDimensions)?;
    let cr_delta = LUMA_DENOMINATOR
        .checked_mul(red)
        .and_then(|value| value.checked_sub(y_numerator))
        .ok_or(RasterError::InvalidDimensions)?;
    let cb = centered_component(cb_delta, CB_DENOMINATOR)?;
    let cr = centered_component(cr_delta, CR_DENOMINATOR)?;
    Ok(Some(Bt709Ycbcr16 { y_prime, cb, cr }))
}

/// Classifies one straight RGBA8 or RGBA16 pixel for the native guard.
///
/// Eight-bit channels are promoted exactly by multiplication with 257. Binary,
/// grayscale, and other non-straight-RGBA values are rejected.
pub fn bt709_conservative_guard_category(
    pixel: PixelValue,
) -> Result<OutputColorGuardCategory, RasterError> {
    let rgba = match pixel {
        PixelValue::Rgba(value) => value.map(|channel| u16::from(channel) * 257),
        PixelValue::Rgba16(value) => value,
        PixelValue::Binary(_) | PixelValue::Grayscale8(_) | PixelValue::Grayscale16(_) => {
            return Err(RasterError::PixelFormatMismatch);
        }
    };
    let Some(value) = bt709_conservative_ycbcr16(rgba)? else {
        return Ok(OutputColorGuardCategory::Transparent);
    };
    let safe = (LUMA_MIN..=LUMA_MAX).contains(&value.y_prime)
        && (CHROMA_MIN..=CHROMA_MAX).contains(&value.cb)
        && (CHROMA_MIN..=CHROMA_MAX).contains(&value.cr);
    Ok(if safe {
        OutputColorGuardCategory::Safe
    } else {
        OutputColorGuardCategory::Outside
    })
}

fn centered_component(delta: i64, denominator: i64) -> Result<u16, RasterError> {
    let numerator = CHANNEL_MAX
        .checked_mul(denominator)
        .and_then(|value| value.checked_add(delta.checked_mul(2)?))
        .ok_or(RasterError::InvalidDimensions)?;
    round_half_up(
        numerator,
        denominator
            .checked_mul(2)
            .ok_or(RasterError::InvalidDimensions)?,
    )
}

fn round_half_up(numerator: i64, denominator: i64) -> Result<u16, RasterError> {
    if numerator < 0 || denominator <= 0 {
        return Err(RasterError::InvalidDimensions);
    }
    let value = numerator
        .checked_add(denominator / 2)
        .ok_or(RasterError::InvalidDimensions)?
        / denominator;
    u16::try_from(value).map_err(|_| RasterError::InvalidDimensions)
}
