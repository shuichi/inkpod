use super::common::*;
use super::*;
use crate::{RasterError, TileRaster, integer_sqrt};

pub fn apply_gradient(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    gradient: &Gradient,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, selection)?;
    validate_gradient(gradient)?;
    let mut result = source.clone();
    let dx = i128::from(gradient.end_x_milli) - i128::from(gradient.start_x_milli);
    let dy = i128::from(gradient.end_y_milli) - i128::from(gradient.start_y_milli);
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let px = i128::from(x) * 1_000 + 500 - i128::from(gradient.start_x_milli);
            let py = i128::from(y) * 1_000 + 500 - i128::from(gradient.start_y_milli);
            let shift = [dx, dy, px, py]
                .into_iter()
                .map(i128::unsigned_abs)
                .max()
                .unwrap_or(0)
                .ilog2()
                .saturating_sub(61);
            let [dx, dy, px, py] = [dx, dy, px, py].map(|value| value / (1_i128 << shift));
            let length_squared = squared_length(dx, dy).ok_or(RasterError::InvalidDimensions)?;
            let position_milli = match gradient.kind {
                GradientKind::Linear => {
                    let dot = px
                        .checked_mul(dx)
                        .and_then(|value| value.checked_add(py.checked_mul(dy)?))
                        .ok_or(RasterError::InvalidDimensions)?;
                    ratio_milli_signed(dot, length_squared)?
                }
                GradientKind::Radial => {
                    let distance_squared =
                        squared_length(px, py).ok_or(RasterError::InvalidDimensions)?;
                    ratio_milli_unsigned(
                        integer_sqrt(distance_squared),
                        integer_sqrt(length_squared),
                    )?
                }
            };
            let mut color = sample_stops(&gradient.stops, position_milli);
            if gradient.dither {
                let signed = if (x.wrapping_mul(17) ^ y.wrapping_mul(31)) & 1 == 0 {
                    -1_i32
                } else {
                    1_i32
                };
                for channel in &mut color[..3] {
                    *channel = (i32::from(*channel) + signed * 128).clamp(0, 65_535) as u16;
                }
            }
            let before = source.pixel(x, y)?;
            let after = match gradient.mode {
                GradientMode::Overwrite => from_rgba16(source.format(), color),
                GradientMode::Composite => source_over(before, color)?,
            };
            result.set_pixel(x, y, after, revision)?;
        }
    }
    Ok(result)
}

fn squared_length(x: i128, y: i128) -> Option<u128> {
    x.unsigned_abs()
        .checked_mul(x.unsigned_abs())?
        .checked_add(y.unsigned_abs().checked_mul(y.unsigned_abs())?)
}

fn ratio_milli_signed(numerator: i128, denominator: u128) -> Result<u32, RasterError> {
    if numerator <= 0 {
        return Ok(0);
    }
    let numerator = numerator as u128;
    if numerator >= denominator {
        return Ok(1_000);
    }
    Ok(mul_div_round_u128(numerator, 1_000, denominator))
}

fn ratio_milli_unsigned(numerator: u128, denominator: u128) -> Result<u32, RasterError> {
    if denominator == 0 {
        return Err(RasterError::InvalidDimensions);
    }
    if numerator >= denominator {
        return Ok(1_000);
    }
    ratio_milli_signed(
        i128::try_from(numerator).map_err(|_| RasterError::InvalidDimensions)?,
        denominator,
    )
}

fn mul_div_round_u128(numerator: u128, multiplier: u32, denominator: u128) -> u32 {
    let mut quotient = 0_u128;
    let mut remainder = 0_u128;
    let highest_bit = 31 - multiplier.leading_zeros();
    for bit in (0..=highest_bit).rev() {
        quotient *= 2;
        remainder *= 2;
        if remainder >= denominator {
            remainder -= denominator;
            quotient += 1;
        }
        if multiplier & (1_u32 << bit) != 0 {
            remainder += numerator;
            if remainder >= denominator {
                remainder -= denominator;
                quotient += 1;
            }
        }
    }
    let doubled = remainder * 2;
    if doubled > denominator || doubled == denominator && quotient & 1 == 1 {
        quotient += 1;
    }
    quotient as u32
}

fn validate_gradient(gradient: &Gradient) -> Result<(), RasterError> {
    if gradient.stops.len() < 3
        || gradient.stops.len() > MAX_GRADIENT_STOPS
        || gradient.start_x_milli == gradient.end_x_milli
            && gradient.start_y_milli == gradient.end_y_milli
        || gradient
            .stops
            .first()
            .is_none_or(|stop| stop.position_milli != 0)
        || gradient
            .stops
            .last()
            .is_none_or(|stop| stop.position_milli != 1_000)
        || gradient
            .stops
            .windows(2)
            .any(|pair| pair[0].position_milli >= pair[1].position_milli)
    {
        Err(RasterError::InvalidDimensions)
    } else {
        Ok(())
    }
}

fn sample_stops(stops: &[GradientStop], position_milli: u32) -> [u16; 4] {
    let pair = stops
        .windows(2)
        .find(|pair| position_milli <= pair[1].position_milli)
        .unwrap_or_else(|| &stops[stops.len() - 2..]);
    let span = pair[1].position_milli - pair[0].position_milli;
    let offset = position_milli
        .saturating_sub(pair[0].position_milli)
        .min(span);
    let mut output = [0_u16; 4];
    for (channel, output) in output.iter_mut().enumerate() {
        let left = u64::from(pair[0].color[channel]) * u64::from(span - offset);
        let right = u64::from(pair[1].color[channel]) * u64::from(offset);
        *output = ((left + right + u64::from(span / 2)) / u64::from(span)) as u16;
    }
    output
}
