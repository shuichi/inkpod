use super::common::*;
use super::*;
use crate::{RasterError, TileRaster};

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
    // Convert before subtraction so arbitrary public-API i64 coordinates cannot
    // overflow. Every i64 value is finite and exactly bounded in f64 here.
    let dx = gradient.end_x_milli as f64 - gradient.start_x_milli as f64;
    let dy = gradient.end_y_milli as f64 - gradient.start_y_milli as f64;
    let length_squared = dx.mul_add(dx, dy * dy);
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let px = f64::from(x).mul_add(1_000.0, 500.0) - gradient.start_x_milli as f64;
            let py = f64::from(y).mul_add(1_000.0, 500.0) - gradient.start_y_milli as f64;
            let t = match gradient.kind {
                GradientKind::Linear => (px.mul_add(dx, py * dy) / length_squared).clamp(0.0, 1.0),
                GradientKind::Radial => {
                    let radius = length_squared.sqrt();
                    (px.hypot(py) / radius).clamp(0.0, 1.0)
                }
            };
            let mut color = sample_stops(&gradient.stops, (t * 1_000.0).round() as u32);
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
