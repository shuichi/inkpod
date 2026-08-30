use super::common::*;
use super::*;
use crate::{PixelFormat, PixelValue, RasterError, TileRaster};

pub fn apply_filter(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    filter: &Filter,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    apply_filter_with_progress(source, selection, filter, revision, |_, _| true)
}

pub fn apply_filter_with_progress(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    filter: &Filter,
    revision: u64,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, selection)?;
    validate_filter(filter)?;
    if !progress(0, u64::from(source.height()).max(1)) {
        return Err(RasterError::Cancelled);
    }
    match filter {
        Filter::BlurWeak => blur_progress(
            source,
            selection,
            1,
            1_000,
            revision,
            &mut progress,
            0,
            u64::from(source.height()),
        ),
        Filter::BlurStrong => blur_progress(
            source,
            selection,
            2,
            1_000,
            revision,
            &mut progress,
            0,
            u64::from(source.height()),
        ),
        Filter::GaussianBlur {
            radius,
            strength_milli,
        } => blur_progress(
            source,
            selection,
            *radius,
            *strength_milli,
            revision,
            &mut progress,
            0,
            u64::from(source.height()),
        ),
        Filter::SharpenWeak => {
            unsharp_progress(source, selection, 1, 500, 0, revision, &mut progress)
        }
        Filter::SharpenStrong => {
            unsharp_progress(source, selection, 1, 1_000, 0, revision, &mut progress)
        }
        Filter::UnsharpMask {
            radius,
            amount_milli,
            threshold,
        } => unsharp_progress(
            source,
            selection,
            *radius,
            *amount_milli,
            *threshold,
            revision,
            &mut progress,
        ),
        Filter::AutoContrast => auto_contrast_progress(source, selection, revision, &mut progress),
        _ => map_selected_progress(
            source,
            selection,
            revision,
            |value| transform_pixel(value, filter),
            &mut progress,
            0,
            u64::from(source.height()),
        ),
    }
}

fn validate_filter(filter: &Filter) -> Result<(), RasterError> {
    match filter {
        Filter::GaussianBlur {
            radius,
            strength_milli,
        } if *radius == 0 || *radius > MAX_FILTER_RADIUS || *strength_milli > 1_000 => {
            Err(RasterError::InvalidDimensions)
        }
        Filter::UnsharpMask {
            radius,
            amount_milli,
            ..
        } if *radius == 0 || *radius > MAX_FILTER_RADIUS || *amount_milli > 5_000 => {
            Err(RasterError::InvalidDimensions)
        }
        Filter::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        } if !(-1_000..=1_000).contains(brightness_milli)
            || !(-1_000..=1_000).contains(contrast_milli) =>
        {
            Err(RasterError::InvalidDimensions)
        }
        Filter::ToneCurve { points, .. } => validate_curve(points),
        Filter::Levels(levels) => validate_levels(levels),
        Filter::Hsv(value)
            if !(-360_000..=360_000).contains(&value.hue_degrees_milli)
                || !(-1_000..=1_000).contains(&value.saturation_milli)
                || !(-1_000..=1_000).contains(&value.value_milli) =>
        {
            Err(RasterError::InvalidDimensions)
        }
        Filter::ColorBalance(value)
            if !(-1_000..=1_000).contains(&value.red_milli)
                || !(-1_000..=1_000).contains(&value.green_milli)
                || !(-1_000..=1_000).contains(&value.blue_milli) =>
        {
            Err(RasterError::InvalidDimensions)
        }
        _ => Ok(()),
    }
}

fn validate_curve(points: &[CurvePoint]) -> Result<(), RasterError> {
    if points.len() < 2
        || points.len() > MAX_CURVE_POINTS
        || points.first().is_none_or(|point| point.input != 0)
        || points.last().is_none_or(|point| point.input != u16::MAX)
        || points.windows(2).any(|pair| pair[0].input >= pair[1].input)
    {
        Err(RasterError::InvalidDimensions)
    } else {
        Ok(())
    }
}

fn validate_levels(levels: &Levels) -> Result<(), RasterError> {
    if levels.input_shadow >= levels.input_highlight
        || levels.output_shadow > levels.output_highlight
        || !(100..=10_000).contains(&levels.input_gamma_milli)
    {
        Err(RasterError::InvalidDimensions)
    } else {
        Ok(())
    }
}

fn map_selected_progress<F>(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    revision: u64,
    mut transform: F,
    progress: &mut impl FnMut(u64, u64) -> bool,
    progress_base: u64,
    progress_total: u64,
) -> Result<TileRaster, RasterError>
where
    F: FnMut(PixelValue) -> Result<PixelValue, RasterError>,
{
    let mut result = source.clone();
    for y in 0..source.height() {
        for x in 0..source.width() {
            if selected(selection, x, y)? {
                result.set_pixel(x, y, transform(source.pixel(x, y)?)?, revision)?;
            }
        }
        if !progress(progress_base + u64::from(y) + 1, progress_total.max(1)) {
            return Err(RasterError::Cancelled);
        }
    }
    Ok(result)
}

fn transform_pixel(value: PixelValue, filter: &Filter) -> Result<PixelValue, RasterError> {
    let format = match value {
        PixelValue::Rgba(_) => PixelFormat::StraightRgba8,
        PixelValue::Rgba16(_) => PixelFormat::StraightRgba16,
        _ => return Err(RasterError::PixelFormatMismatch),
    };
    let mut color = value.rgba16().ok_or(RasterError::PixelFormatMismatch)?;
    match filter {
        Filter::Invert { channel } => {
            apply_channels(&mut color, *channel, |value| u16::MAX - value)
        }
        Filter::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        } => {
            let contrast = f64::from(*contrast_milli) / 1_000.0;
            let factor = if contrast >= 0.0 {
                1.0 / (1.0 - contrast.min(0.999))
            } else {
                1.0 + contrast
            };
            for value in &mut color[..3] {
                let normalized = f64::from(*value) / 65_535.0;
                let adjusted =
                    ((normalized - 0.5) * factor + 0.5) + f64::from(*brightness_milli) / 1_000.0;
                *value = normalized_u16(adjusted);
            }
        }
        Filter::ToneCurve {
            channel,
            interpolation,
            points,
        } => {
            apply_channels(&mut color, *channel, |value| {
                curve_value(points, value, *interpolation)
            });
        }
        Filter::Levels(levels) => {
            apply_channels(&mut color, levels.channel, |value| {
                level_value(levels, value)
            });
        }
        Filter::Hsv(adjustment) => apply_hsv(&mut color, *adjustment),
        Filter::ColorBalance(balance) => {
            for (value, amount) in color[..3].iter_mut().zip([
                balance.red_milli,
                balance.green_milli,
                balance.blue_milli,
            ]) {
                *value = (i64::from(*value) + i64::from(amount) * 65_535 / 1_000).clamp(0, 65_535)
                    as u16;
            }
        }
        Filter::AutoContrast
        | Filter::SharpenWeak
        | Filter::SharpenStrong
        | Filter::BlurWeak
        | Filter::BlurStrong
        | Filter::GaussianBlur { .. }
        | Filter::UnsharpMask { .. } => {}
    }
    Ok(from_rgba16(format, color))
}

#[allow(clippy::too_many_arguments)]
fn blur_progress(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    radius: u32,
    strength_milli: u32,
    revision: u64,
    progress: &mut impl FnMut(u64, u64) -> bool,
    progress_base: u64,
    progress_total: u64,
) -> Result<TileRaster, RasterError> {
    validate_radius_work(source, radius)?;
    let radius = i32::try_from(radius).map_err(|_| RasterError::InvalidDimensions)?;
    let kernel = canonical_blur_kernel(radius as u32);
    let kernel_sum = kernel.iter().sum::<u64>();
    let mut result = source.clone();
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let mut sums = [0_u128; 4];
            let mut weights = 0_u128;
            for offset_y in -radius..=radius {
                let sy = (i64::from(y) + i64::from(offset_y))
                    .clamp(0, i64::from(source.height()) - 1) as u32;
                let wy = kernel[(offset_y + radius) as usize];
                for offset_x in -radius..=radius {
                    let sx = (i64::from(x) + i64::from(offset_x))
                        .clamp(0, i64::from(source.width()) - 1)
                        as u32;
                    let wx = kernel[(offset_x + radius) as usize];
                    let weight = u128::from(wx) * u128::from(wy);
                    let rgba = source
                        .pixel(sx, sy)?
                        .rgba16()
                        .ok_or(RasterError::PixelFormatMismatch)?;
                    let alpha = u128::from(rgba[3]);
                    for channel in 0..3 {
                        sums[channel] += u128::from(rgba[channel]) * alpha * weight;
                    }
                    sums[3] += alpha * weight;
                    weights += weight;
                }
            }
            debug_assert!(weights >= u128::from(kernel_sum));
            let alpha = ((sums[3] + weights / 2) / weights) as u16;
            let mut blurred = [0_u16; 4];
            blurred[3] = alpha;
            for channel in 0..3 {
                blurred[channel] = (sums[channel] + sums[3] / 2)
                    .checked_div(sums[3])
                    .unwrap_or(0) as u16;
            }
            let original = source
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            for channel in 0..4 {
                blurred[channel] = lerp_u16(original[channel], blurred[channel], strength_milli);
            }
            result.set_pixel(x, y, from_rgba16(source.format(), blurred), revision)?;
        }
        if !progress(progress_base + u64::from(y) + 1, progress_total.max(1)) {
            return Err(RasterError::Cancelled);
        }
    }
    Ok(result)
}

fn canonical_blur_kernel(radius: u32) -> Vec<u64> {
    let mut row = vec![1_u64];
    for _ in 0..radius.saturating_mul(2) {
        let mut next = vec![1_u64; row.len() + 1];
        for index in 1..row.len() {
            next[index] = row[index - 1] + row[index];
        }
        if next.iter().copied().max().unwrap_or(1) > 1_000_000_000_000 {
            for value in &mut next {
                *value = (*value + 512) / 1_024;
                *value = (*value).max(1);
            }
        }
        row = next;
    }
    row
}

pub(super) fn validate_radius_work(source: &TileRaster, radius: u32) -> Result<(), RasterError> {
    let diameter = u128::from(radius)
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .ok_or(RasterError::InvalidDimensions)?;
    let work = u128::from(source.width())
        .checked_mul(u128::from(source.height()))
        .and_then(|pixels| pixels.checked_mul(diameter))
        .and_then(|value| value.checked_mul(diameter))
        .ok_or(RasterError::InvalidDimensions)?;
    if work > MAX_IMAGE_EDIT_WORK {
        Err(RasterError::InvalidDimensions)
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn unsharp_progress(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    radius: u32,
    amount_milli: u32,
    threshold: u16,
    revision: u64,
    progress: &mut impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    let height = u64::from(source.height());
    let total = height.saturating_mul(2).max(1);
    let soft = blur_progress(source, None, radius, 1_000, revision, progress, 0, total)?;
    let mut result = source.clone();
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let original = source
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            let blurred = soft
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            let mut output = original;
            for channel in 0..3 {
                if original[channel].abs_diff(blurred[channel]) < threshold {
                    continue;
                }
                output[channel] = (i64::from(original[channel])
                    + (i64::from(original[channel]) - i64::from(blurred[channel]))
                        * i64::from(amount_milli)
                        / 1_000)
                    .clamp(0, 65_535) as u16;
            }
            result.set_pixel(x, y, from_rgba16(source.format(), output), revision)?;
        }
        if !progress(height + u64::from(y) + 1, total) {
            return Err(RasterError::Cancelled);
        }
    }
    Ok(result)
}

fn auto_contrast_progress(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    revision: u64,
    progress: &mut impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    let height = u64::from(source.height());
    let total = height.saturating_mul(2).max(1);
    let mut minimum = [u16::MAX; 3];
    let mut maximum = [0_u16; 3];
    let mut found = false;
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let color = source
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            // Alpha intentionally does not participate in the color histogram.
            for channel in 0..3 {
                minimum[channel] = minimum[channel].min(color[channel]);
                maximum[channel] = maximum[channel].max(color[channel]);
            }
            found = true;
        }
        if !progress(u64::from(y) + 1, total) {
            return Err(RasterError::Cancelled);
        }
    }
    if !found {
        return Ok(source.clone());
    }
    map_selected_progress(
        source,
        selection,
        revision,
        |value| {
            let mut color = value.rgba16().ok_or(RasterError::PixelFormatMismatch)?;
            for channel in 0..3 {
                let range = u32::from(maximum[channel] - minimum[channel]);
                if range != 0 {
                    color[channel] = ((u64::from(color[channel] - minimum[channel]) * 65_535
                        + u64::from(range / 2))
                        / u64::from(range)) as u16;
                }
            }
            Ok(from_rgba16(source.format(), color))
        },
        progress,
        height,
        total,
    )
}

fn apply_channels<F>(color: &mut [u16; 4], channel: Channel, mut operation: F)
where
    F: FnMut(u16) -> u16,
{
    match channel {
        Channel::Rgb => {
            for value in &mut color[..3] {
                *value = operation(*value);
            }
        }
        Channel::Red => color[0] = operation(color[0]),
        Channel::Green => color[1] = operation(color[1]),
        Channel::Blue => color[2] = operation(color[2]),
    }
}

fn curve_value(points: &[CurvePoint], value: u16, interpolation: CurveInterpolation) -> u16 {
    let index = points
        .windows(2)
        .position(|pair| value <= pair[1].input)
        .unwrap_or(points.len() - 2);
    let left = points[index];
    let right = points[index + 1];
    let span = f64::from(right.input - left.input);
    let t = f64::from(value - left.input) / span;
    let output = match interpolation {
        CurveInterpolation::Bezier => {
            // Each ordered pair is a monotone cubic Bezier segment with zero
            // tangent at its anchors; this passes exactly through every point.
            let smooth = t * t * (3.0 - 2.0 * t);
            f64::from(left.output) + (f64::from(right.output) - f64::from(left.output)) * smooth
        }
        CurveInterpolation::BSpline => {
            // Cardinal cubic interpolation uses adjacent anchors while clamping
            // the end controls. Results are rounded once to normalized u16.
            let y0 = f64::from(points[index.saturating_sub(1)].output);
            let y1 = f64::from(left.output);
            let y2 = f64::from(right.output);
            let y3 = f64::from(points[(index + 2).min(points.len() - 1)].output);
            0.5 * ((2.0 * y1)
                + (-y0 + y2) * t
                + (2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3) * t * t
                + (-y0 + 3.0 * y1 - 3.0 * y2 + y3) * t * t * t)
        }
    };
    output.round().clamp(0.0, 65_535.0) as u16
}

fn level_value(levels: &Levels, value: u16) -> u16 {
    let span = u32::from(levels.input_highlight - levels.input_shadow);
    let offset = u32::from(value.saturating_sub(levels.input_shadow)).min(span);
    let normalized =
        ((u64::from(offset) * u64::from(u16::MAX) + u64::from(span / 2)) / u64::from(span)) as u16;
    let corrected = crate::canonical_pow_unit_u16(normalized, 1_000, levels.input_gamma_milli)
        .expect("validated positive gamma");
    let output_span = u64::from(levels.output_highlight - levels.output_shadow);
    (u64::from(levels.output_shadow)
        + (u64::from(corrected) * output_span + u64::from(u16::MAX / 2)) / u64::from(u16::MAX))
        as u16
}

fn apply_hsv(color: &mut [u16; 4], adjustment: HsvAdjustment) {
    let r = f64::from(color[0]) / 65_535.0;
    let g = f64::from(color[1]) / 65_535.0;
    let b = f64::from(color[2]) / 65_535.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let mut hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    hue = (hue + f64::from(adjustment.hue_degrees_milli) / 1_000.0).rem_euclid(360.0);
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    let saturation =
        (saturation * (1.0 + f64::from(adjustment.saturation_milli) / 1_000.0)).clamp(0.0, 1.0);
    let value = (max * (1.0 + f64::from(adjustment.value_milli) / 1_000.0)).clamp(0.0, 1.0);
    let chroma = value * saturation;
    let x = chroma * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match (hue / 60.0).floor() as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    color[0] = normalized_u16(r1 + m);
    color[1] = normalized_u16(g1 + m);
    color[2] = normalized_u16(b1 + m);
}
