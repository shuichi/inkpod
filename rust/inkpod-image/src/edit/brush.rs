use super::common::*;
use super::filter::validate_radius_work;
use super::*;
use crate::{RasterError, TileRaster};

pub fn apply_airbrush(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    stroke: AirbrushStroke,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, selection)?;
    if stroke.radius_milli == 0
        || stroke.radius_milli > MAX_FILTER_RADIUS * 1_000
        || stroke.hardness_milli > 1_000
        || stroke.opacity_milli > 1_000
    {
        return Err(RasterError::InvalidDimensions);
    }
    let mut result = source.clone();
    apply_airbrush_dab(&mut result, selection, stroke, revision)?;
    Ok(result)
}

pub fn apply_airbrush_gesture(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    gesture: &AirbrushGesture,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, selection)?;
    validate_effect_samples(&gesture.samples)?;
    if gesture.radius_milli == 0
        || gesture.radius_milli > MAX_FILTER_RADIUS * 1_000
        || gesture.hardness_milli > 1_000
        || gesture.spacing_milli == 0
        || gesture.spacing_milli > 4_000
        || gesture.opacity_milli > 1_000
        || gesture.fade_milli > 1_000
        || gesture.continuous_dabs > 1_024
    {
        return Err(RasterError::InvalidDimensions);
    }
    let dabs = interpolated_effect_samples(
        &gesture.samples,
        gesture.spacing_milli,
        gesture.continuous_dabs,
    )?;
    let mut result = source.clone();
    let denominator = dabs.len().saturating_sub(1).max(1) as u64;
    for (index, sample) in dabs.into_iter().enumerate() {
        let pressure = sample.pressure_milli.min(1_000);
        let fade = 1_000_u64
            .saturating_sub(u64::from(gesture.fade_milli) * index as u64 / denominator)
            as u32;
        let radius = if gesture.pressure_size {
            ((u64::from(gesture.radius_milli) * u64::from(pressure) + 500) / 1_000).max(1) as u32
        } else {
            gesture.radius_milli
        };
        let mut opacity =
            ((u64::from(gesture.opacity_milli) * u64::from(fade) + 500) / 1_000) as u32;
        if gesture.pressure_opacity {
            opacity = ((u64::from(opacity) * u64::from(pressure) + 500) / 1_000) as u32;
        }
        apply_airbrush_dab(
            &mut result,
            selection,
            AirbrushStroke {
                center_x_milli: sample.x_milli,
                center_y_milli: sample.y_milli,
                radius_milli: radius,
                hardness_milli: gesture.hardness_milli,
                opacity_milli: opacity,
                color: gesture.color,
            },
            revision,
        )?;
    }
    Ok(result)
}

fn apply_airbrush_dab(
    result: &mut TileRaster,
    selection: Option<&TileRaster>,
    stroke: AirbrushStroke,
    revision: u64,
) -> Result<(), RasterError> {
    let radius = f64::from(stroke.radius_milli);
    let hard_radius = radius * f64::from(stroke.hardness_milli) / 1_000.0;
    let (left, right) =
        clipped_effect_bounds(stroke.center_x_milli, stroke.radius_milli, result.width());
    let (top, bottom) =
        clipped_effect_bounds(stroke.center_y_milli, stroke.radius_milli, result.height());
    for y in top..bottom {
        for x in left..right {
            if !selected(selection, x, y)? {
                continue;
            }
            let dx = f64::from(x).mul_add(1_000.0, 500.0) - stroke.center_x_milli as f64;
            let dy = f64::from(y).mul_add(1_000.0, 500.0) - stroke.center_y_milli as f64;
            let distance = dx.hypot(dy);
            if distance > radius {
                continue;
            }
            let falloff = if distance <= hard_radius || hard_radius == radius {
                1.0
            } else {
                1.0 - (distance - hard_radius) / (radius - hard_radius)
            };
            let mut color = stroke.color;
            color[3] = ((f64::from(color[3]) * f64::from(stroke.opacity_milli) * falloff) / 1_000.0)
                .round()
                .clamp(0.0, 65_535.0) as u16;
            let after = source_over(result.pixel(x, y)?, color)?;
            result.set_pixel(x, y, after, revision)?;
        }
    }
    Ok(())
}

pub fn apply_boundary_airbrush(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    effect: &BoundaryAirbrush,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, selection)?;
    if effect.colors.len() < 2
        || effect.colors.len() > MAX_GRADIENT_STOPS
        || effect.width == 0
        || effect.width > MAX_FILTER_RADIUS
        || effect.strength_milli > 1_000
    {
        return Err(RasterError::InvalidDimensions);
    }
    validate_radius_work(source, effect.width)?;
    let mut result = source.clone();
    let radius = i32::try_from(effect.width).map_err(|_| RasterError::InvalidDimensions)?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let center = source
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            if !effect.colors.contains(&center) {
                continue;
            }
            let mut sums = [0_u64; 4];
            let mut count = 0_u64;
            let mut distinct_neighbor = false;
            for offset_y in -radius..=radius {
                for offset_x in -radius..=radius {
                    if offset_x * offset_x + offset_y * offset_y > radius * radius {
                        continue;
                    }
                    let Some(nx) = i64::from(x).checked_add(i64::from(offset_x)) else {
                        continue;
                    };
                    let Some(ny) = i64::from(y).checked_add(i64::from(offset_y)) else {
                        continue;
                    };
                    if nx < 0
                        || ny < 0
                        || nx >= i64::from(source.width())
                        || ny >= i64::from(source.height())
                    {
                        continue;
                    }
                    let value = source
                        .pixel(nx as u32, ny as u32)?
                        .rgba16()
                        .ok_or(RasterError::PixelFormatMismatch)?;
                    if effect.colors.contains(&value) {
                        distinct_neighbor |= value != center;
                        for channel in 0..4 {
                            sums[channel] += u64::from(value[channel]);
                        }
                        count += 1;
                    }
                }
            }
            if !distinct_neighbor || count == 0 {
                continue;
            }
            let mut average = [0_u16; 4];
            for channel in 0..4 {
                average[channel] = ((sums[channel] + count / 2) / count) as u16;
                average[channel] =
                    lerp_u16(center[channel], average[channel], effect.strength_milli);
            }
            result.set_pixel(x, y, from_rgba16(source.format(), average), revision)?;
        }
    }
    Ok(result)
}

pub fn apply_stamp(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    stamp: Stamp,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, selection)?;
    if stamp.width == 0 || stamp.height == 0 || stamp.opacity_milli > 1_000 {
        return Err(RasterError::InvalidDimensions);
    }
    let x_range = clipped_stamp_axis(
        stamp.width,
        stamp.source_x,
        stamp.destination_x,
        source.width(),
    );
    let y_range = clipped_stamp_axis(
        stamp.height,
        stamp.source_y,
        stamp.destination_y,
        source.height(),
    );
    let clipped_pixels = u64::from(x_range.end.saturating_sub(x_range.start))
        .checked_mul(u64::from(y_range.end.saturating_sub(y_range.start)))
        .ok_or(RasterError::InvalidDimensions)?;
    if clipped_pixels > MAX_IMAGE_EDIT_PIXELS {
        return Err(RasterError::InvalidDimensions);
    }
    let mut result = source.clone();
    for y in y_range {
        for x in x_range.clone() {
            let sx = i64::from(stamp.source_x) + i64::from(x);
            let sy = i64::from(stamp.source_y) + i64::from(y);
            let dx = i64::from(stamp.destination_x) + i64::from(x);
            let dy = i64::from(stamp.destination_y) + i64::from(y);
            debug_assert!(sx >= 0 && sy >= 0 && dx >= 0 && dy >= 0);
            let sample = source.pixel(sx as u32, sy as u32)?;
            let (dx, dy) = (dx as u32, dy as u32);
            if !selected(selection, dx, dy)? {
                continue;
            }
            let mut rgba = sample.rgba16().ok_or(RasterError::PixelFormatMismatch)?;
            rgba[3] = ((u64::from(rgba[3]) * u64::from(stamp.opacity_milli) + 500) / 1_000) as u16;
            let after = source_over(source.pixel(dx, dy)?, rgba)?;
            result.set_pixel(dx, dy, after, revision)?;
        }
    }
    Ok(result)
}

pub fn apply_stamp_gesture(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    gesture: &StampGesture,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, selection)?;
    validate_effect_samples(&gesture.samples)?;
    if gesture.radius_milli == 0
        || gesture.radius_milli > MAX_FILTER_RADIUS * 1_000
        || gesture.hardness_milli > 1_000
        || gesture.spacing_milli == 0
        || gesture.spacing_milli > 4_000
        || gesture.opacity_milli > 1_000
    {
        return Err(RasterError::InvalidDimensions);
    }
    let dabs = interpolated_effect_samples(&gesture.samples, gesture.spacing_milli, 0)?;
    let destination_anchor = *gesture
        .samples
        .first()
        .ok_or(RasterError::InvalidDimensions)?;
    let mut result = source.clone();
    for sample in dabs {
        let pressure = sample.pressure_milli.min(1_000);
        let radius = if gesture.pressure_size {
            ((u64::from(gesture.radius_milli) * u64::from(pressure) + 500) / 1_000).max(1) as u32
        } else {
            gesture.radius_milli
        };
        let opacity = if gesture.pressure_opacity {
            ((u64::from(gesture.opacity_milli) * u64::from(pressure) + 500) / 1_000) as u32
        } else {
            gesture.opacity_milli
        };
        apply_stamp_dab(
            source,
            &mut result,
            selection,
            gesture.source_x_milli,
            gesture.source_y_milli,
            destination_anchor.x_milli,
            destination_anchor.y_milli,
            sample.x_milli,
            sample.y_milli,
            radius,
            gesture.hardness_milli,
            opacity,
            gesture.shape,
            revision,
        )?;
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn apply_stamp_dab(
    source: &TileRaster,
    result: &mut TileRaster,
    selection: Option<&TileRaster>,
    source_anchor_x: i64,
    source_anchor_y: i64,
    destination_anchor_x: i64,
    destination_anchor_y: i64,
    center_x: i64,
    center_y: i64,
    radius_milli: u32,
    hardness_milli: u32,
    opacity_milli: u32,
    shape: StampShape,
    revision: u64,
) -> Result<(), RasterError> {
    let radius = f64::from(radius_milli);
    let hard_radius = radius * f64::from(hardness_milli) / 1_000.0;
    let (left, right) = clipped_effect_bounds(center_x, radius_milli, source.width());
    let (top, bottom) = clipped_effect_bounds(center_y, radius_milli, source.height());
    let offset_x = i128::from(source_anchor_x) - i128::from(destination_anchor_x);
    let offset_y = i128::from(source_anchor_y) - i128::from(destination_anchor_y);
    for y in top..bottom {
        for x in left..right {
            if !selected(selection, x, y)? {
                continue;
            }
            let pixel_x = i64::from(x) * 1_000 + 500;
            let pixel_y = i64::from(y) * 1_000 + 500;
            let dx = (pixel_x as f64 - center_x as f64).abs();
            let dy = (pixel_y as f64 - center_y as f64).abs();
            let distance = match shape {
                StampShape::Round => dx.hypot(dy),
                StampShape::Square => dx.max(dy),
            };
            if distance > radius {
                continue;
            }
            let source_x_milli = i128::from(pixel_x) + offset_x;
            let source_y_milli = i128::from(pixel_y) + offset_y;
            if source_x_milli < 0 || source_y_milli < 0 {
                continue;
            }
            let source_x =
                u32::try_from(source_x_milli / 1_000).map_err(|_| RasterError::PixelOutOfBounds)?;
            let source_y =
                u32::try_from(source_y_milli / 1_000).map_err(|_| RasterError::PixelOutOfBounds)?;
            if source_x >= source.width() || source_y >= source.height() {
                continue;
            }
            let falloff = if distance <= hard_radius || hard_radius == radius {
                1.0
            } else {
                1.0 - (distance - hard_radius) / (radius - hard_radius)
            };
            let mut color = source
                .pixel(source_x, source_y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            color[3] = ((f64::from(color[3]) * f64::from(opacity_milli) * falloff) / 1_000.0)
                .round()
                .clamp(0.0, 65_535.0) as u16;
            let after = source_over(result.pixel(x, y)?, color)?;
            result.set_pixel(x, y, after, revision)?;
        }
    }
    Ok(())
}

fn clipped_effect_bounds(center_milli: i64, radius_milli: u32, bound: u32) -> (u32, u32) {
    let center = center_milli as f64;
    let radius = f64::from(radius_milli);
    let bound = f64::from(bound);
    let first = ((center - radius) / 1_000.0 - 1.0)
        .floor()
        .clamp(0.0, bound) as u32;
    let last = ((center + radius) / 1_000.0 + 1.0).ceil().clamp(0.0, bound) as u32;
    (first, last)
}

fn validate_effect_samples(samples: &[EffectSample]) -> Result<(), RasterError> {
    if samples.is_empty()
        || samples.len() > 1_048_576
        || samples.iter().any(|sample| sample.pressure_milli > 1_000)
    {
        Err(RasterError::InvalidDimensions)
    } else {
        Ok(())
    }
}

fn interpolated_effect_samples(
    samples: &[EffectSample],
    spacing_milli: u32,
    continuous_dabs: u32,
) -> Result<Vec<EffectSample>, RasterError> {
    validate_effect_samples(samples)?;
    if spacing_milli == 0 {
        return Err(RasterError::InvalidDimensions);
    }
    let mut result = Vec::new();
    result.push(samples[0]);
    for pair in samples.windows(2) {
        let dx = pair[1].x_milli as f64 - pair[0].x_milli as f64;
        let dy = pair[1].y_milli as f64 - pair[0].y_milli as f64;
        let distance = dx.hypot(dy);
        let steps = (distance / f64::from(spacing_milli)).ceil().max(1.0) as u64;
        if result.len().saturating_add(steps as usize) > 1_048_576 {
            return Err(RasterError::InvalidDimensions);
        }
        for step in 1..=steps {
            let ratio = step as f64 / steps as f64;
            result.push(EffectSample {
                x_milli: (pair[0].x_milli as f64 + dx * ratio).round() as i64,
                y_milli: (pair[0].y_milli as f64 + dy * ratio).round() as i64,
                pressure_milli: (f64::from(pair[0].pressure_milli)
                    + (pair[1].pressure_milli as f64 - pair[0].pressure_milli as f64) * ratio)
                    .round()
                    .clamp(0.0, 1_000.0) as u32,
            });
        }
    }
    if continuous_dabs > 0 {
        result.extend(std::iter::repeat_n(
            *samples.last().unwrap(),
            continuous_dabs as usize,
        ));
    }
    Ok(result)
}

fn clipped_stamp_axis(
    length: u32,
    source_start: i32,
    destination_start: i32,
    bound: u32,
) -> std::ops::Range<u32> {
    let start = 0_i64
        .max(-i64::from(source_start))
        .max(-i64::from(destination_start));
    let end = i64::from(length)
        .min(i64::from(bound) - i64::from(source_start))
        .min(i64::from(bound) - i64::from(destination_start));
    if start >= end {
        0..0
    } else {
        start as u32..end as u32
    }
}
