use crate::{PixelFormat, PixelValue, RasterError, TileRaster};

pub const MAX_FILTER_RADIUS: u32 = 64;
pub const MAX_CURVE_POINTS: usize = 64;
pub const MAX_GRADIENT_STOPS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    Rgb,
    Red,
    Green,
    Blue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurveInterpolation {
    Bezier,
    BSpline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurvePoint {
    /// Input and output values use the full normalized 0..=65535 range.
    pub input: u16,
    pub output: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Levels {
    pub channel: Channel,
    pub input_shadow: u16,
    pub input_gamma_milli: u32,
    pub input_highlight: u16,
    pub output_shadow: u16,
    pub output_highlight: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HsvAdjustment {
    pub hue_degrees_milli: i32,
    pub saturation_milli: i32,
    pub value_milli: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorBalance {
    pub red_milli: i32,
    pub green_milli: i32,
    pub blue_milli: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Filter {
    SharpenWeak,
    SharpenStrong,
    BlurWeak,
    BlurStrong,
    GaussianBlur {
        radius: u32,
        strength_milli: u32,
    },
    UnsharpMask {
        radius: u32,
        amount_milli: u32,
        threshold: u16,
    },
    Invert {
        channel: Channel,
    },
    AutoContrast,
    BrightnessContrast {
        brightness_milli: i32,
        contrast_milli: i32,
    },
    ToneCurve {
        channel: Channel,
        interpolation: CurveInterpolation,
        points: Vec<CurvePoint>,
    },
    Levels(Levels),
    Hsv(HsvAdjustment),
    ColorBalance(ColorBalance),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Adjustment {
    BrightnessContrast {
        brightness_milli: i32,
        contrast_milli: i32,
    },
    ToneCurve {
        channel: Channel,
        interpolation: CurveInterpolation,
        points: Vec<CurvePoint>,
    },
    Levels(Levels),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GradientKind {
    Linear,
    Radial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GradientMode {
    Composite,
    Overwrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GradientStop {
    pub position_milli: u32,
    pub color: [u16; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub mode: GradientMode,
    pub start_x_milli: i64,
    pub start_y_milli: i64,
    pub end_x_milli: i64,
    pub end_y_milli: i64,
    pub dither: bool,
    pub stops: Vec<GradientStop>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AirbrushStroke {
    pub center_x_milli: i64,
    pub center_y_milli: i64,
    pub radius_milli: u32,
    pub hardness_milli: u32,
    pub opacity_milli: u32,
    pub color: [u16; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryAirbrush {
    pub colors: Vec<[u16; 4]>,
    pub width: u32,
    pub strength_milli: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stamp {
    pub source_x: i32,
    pub source_y: i32,
    pub destination_x: i32,
    pub destination_y: i32,
    pub width: u32,
    pub height: u32,
    pub opacity_milli: u32,
}

pub fn apply_filter(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    filter: &Filter,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, selection)?;
    validate_filter(filter)?;
    match filter {
        Filter::BlurWeak => blur(source, selection, 1, 1_000, revision),
        Filter::BlurStrong => blur(source, selection, 2, 1_000, revision),
        Filter::GaussianBlur {
            radius,
            strength_milli,
        } => blur(source, selection, *radius, *strength_milli, revision),
        Filter::SharpenWeak => unsharp(source, selection, 1, 500, 0, revision),
        Filter::SharpenStrong => unsharp(source, selection, 1, 1_000, 0, revision),
        Filter::UnsharpMask {
            radius,
            amount_milli,
            threshold,
        } => unsharp(
            source,
            selection,
            *radius,
            *amount_milli,
            *threshold,
            revision,
        ),
        Filter::AutoContrast => auto_contrast(source, selection, revision),
        _ => map_selected(source, selection, revision, |value| {
            transform_pixel(value, filter)
        }),
    }
}

pub fn apply_adjustment(
    value: PixelValue,
    adjustment: &Adjustment,
) -> Result<PixelValue, RasterError> {
    validate_adjustment(adjustment)?;
    let filter = match adjustment {
        Adjustment::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        } => Filter::BrightnessContrast {
            brightness_milli: *brightness_milli,
            contrast_milli: *contrast_milli,
        },
        Adjustment::ToneCurve {
            channel,
            interpolation,
            points,
        } => Filter::ToneCurve {
            channel: *channel,
            interpolation: *interpolation,
            points: points.clone(),
        },
        Adjustment::Levels(levels) => Filter::Levels(levels.clone()),
    };
    transform_pixel(value, &filter)
}

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
    let dx = gradient.end_x_milli - gradient.start_x_milli;
    let dy = gradient.end_y_milli - gradient.start_y_milli;
    let length_squared = (dx as f64).mul_add(dx as f64, (dy as f64) * (dy as f64));
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let px = i64::from(x) * 1_000 + 500 - gradient.start_x_milli;
            let py = i64::from(y) * 1_000 + 500 - gradient.start_y_milli;
            let t = match gradient.kind {
                GradientKind::Linear => ((px as f64).mul_add(dx as f64, (py as f64) * (dy as f64))
                    / length_squared)
                    .clamp(0.0, 1.0),
                GradientKind::Radial => {
                    let radius = length_squared.sqrt();
                    ((px as f64).hypot(py as f64) / radius).clamp(0.0, 1.0)
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
    let radius = f64::from(stroke.radius_milli);
    let hard_radius = radius * f64::from(stroke.hardness_milli) / 1_000.0;
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let dx = i64::from(x) * 1_000 + 500 - stroke.center_x_milli;
            let dy = i64::from(y) * 1_000 + 500 - stroke.center_y_milli;
            let distance = (dx as f64).hypot(dy as f64);
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
            let after = source_over(source.pixel(x, y)?, color)?;
            result.set_pixel(x, y, after, revision)?;
        }
    }
    Ok(result)
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
    let mut samples = Vec::new();
    for y in 0..stamp.height {
        for x in 0..stamp.width {
            let sx = i64::from(stamp.source_x) + i64::from(x);
            let sy = i64::from(stamp.source_y) + i64::from(y);
            if sx >= 0
                && sy >= 0
                && sx < i64::from(source.width())
                && sy < i64::from(source.height())
            {
                samples.push((x, y, source.pixel(sx as u32, sy as u32)?));
            }
        }
    }
    let mut result = source.clone();
    for (x, y, sample) in samples {
        let dx = i64::from(stamp.destination_x) + i64::from(x);
        let dy = i64::from(stamp.destination_y) + i64::from(y);
        if dx < 0 || dy < 0 || dx >= i64::from(source.width()) || dy >= i64::from(source.height()) {
            continue;
        }
        let (dx, dy) = (dx as u32, dy as u32);
        if !selected(selection, dx, dy)? {
            continue;
        }
        let mut rgba = sample.rgba16().ok_or(RasterError::PixelFormatMismatch)?;
        rgba[3] = ((u64::from(rgba[3]) * u64::from(stamp.opacity_milli) + 500) / 1_000) as u16;
        let after = source_over(source.pixel(dx, dy)?, rgba)?;
        result.set_pixel(dx, dy, after, revision)?;
    }
    Ok(result)
}

pub fn edit_alpha(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    alpha: &TileRaster,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, selection)?;
    if source.width() != alpha.width()
        || source.height() != alpha.height()
        || !matches!(
            alpha.format(),
            PixelFormat::Grayscale8 | PixelFormat::Grayscale16
        )
    {
        return Err(RasterError::PixelFormatMismatch);
    }
    let mut result = source.clone();
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let mut color = source
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            color[3] = match alpha.pixel(x, y)? {
                PixelValue::Grayscale8(value) => u16::from(value) * 257,
                PixelValue::Grayscale16(value) => value,
                _ => return Err(RasterError::PixelFormatMismatch),
            };
            result.set_pixel(x, y, from_rgba16(source.format(), color), revision)?;
        }
    }
    Ok(result)
}

fn validate_color_raster(raster: &TileRaster) -> Result<(), RasterError> {
    if matches!(
        raster.format(),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
    ) {
        Ok(())
    } else {
        Err(RasterError::PixelFormatMismatch)
    }
}

fn validate_selection(
    source: &TileRaster,
    selection: Option<&TileRaster>,
) -> Result<(), RasterError> {
    let Some(selection) = selection else {
        return Ok(());
    };
    if selection.width() != source.width()
        || selection.height() != source.height()
        || selection.format() != PixelFormat::BinaryMask8
    {
        Err(RasterError::PixelFormatMismatch)
    } else {
        Ok(())
    }
}

fn selected(selection: Option<&TileRaster>, x: u32, y: u32) -> Result<bool, RasterError> {
    match selection {
        None => Ok(true),
        Some(selection) => Ok(matches!(selection.pixel(x, y)?, PixelValue::Binary(255))),
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
        } if brightness_milli.abs() > 1_000 || contrast_milli.abs() > 1_000 => {
            Err(RasterError::InvalidDimensions)
        }
        Filter::ToneCurve { points, .. } => validate_curve(points),
        Filter::Levels(levels) => validate_levels(levels),
        Filter::Hsv(value)
            if value.hue_degrees_milli.abs() > 360_000
                || value.saturation_milli.abs() > 1_000
                || value.value_milli.abs() > 1_000 =>
        {
            Err(RasterError::InvalidDimensions)
        }
        Filter::ColorBalance(value)
            if value.red_milli.abs() > 1_000
                || value.green_milli.abs() > 1_000
                || value.blue_milli.abs() > 1_000 =>
        {
            Err(RasterError::InvalidDimensions)
        }
        _ => Ok(()),
    }
}

fn validate_adjustment(adjustment: &Adjustment) -> Result<(), RasterError> {
    match adjustment {
        Adjustment::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        } => validate_filter(&Filter::BrightnessContrast {
            brightness_milli: *brightness_milli,
            contrast_milli: *contrast_milli,
        }),
        Adjustment::ToneCurve { points, .. } => validate_curve(points),
        Adjustment::Levels(levels) => validate_levels(levels),
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

fn map_selected<F>(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    revision: u64,
    mut transform: F,
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

fn blur(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    radius: u32,
    strength_milli: u32,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    let radius = i32::try_from(radius).map_err(|_| RasterError::InvalidDimensions)?;
    let sigma = (f64::from(radius) / 2.0).max(0.5);
    let mut kernel = Vec::new();
    let mut kernel_sum = 0_u64;
    for offset in -radius..=radius {
        let weight = (-(f64::from(offset * offset)) / (2.0 * sigma * sigma)).exp();
        let fixed = (weight * 1_000_000.0).round().max(1.0) as u64;
        kernel.push(fixed);
        kernel_sum += fixed;
    }
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
    }
    Ok(result)
}

fn unsharp(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    radius: u32,
    amount_milli: u32,
    threshold: u16,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    let soft = blur(source, None, radius, 1_000, revision)?;
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
    }
    Ok(result)
}

fn auto_contrast(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    revision: u64,
) -> Result<TileRaster, RasterError> {
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
    }
    if !found {
        return Ok(source.clone());
    }
    map_selected(source, selection, revision, |value| {
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
    })
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
    let normalized = ((f64::from(value) - f64::from(levels.input_shadow))
        / f64::from(levels.input_highlight - levels.input_shadow))
    .clamp(0.0, 1.0);
    let gamma = f64::from(levels.input_gamma_milli) / 1_000.0;
    let corrected = normalized.powf(1.0 / gamma);
    let output = f64::from(levels.output_shadow)
        + corrected * f64::from(levels.output_highlight - levels.output_shadow);
    output.round().clamp(0.0, 65_535.0) as u16
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

fn source_over(background: PixelValue, foreground: [u16; 4]) -> Result<PixelValue, RasterError> {
    let format = match background {
        PixelValue::Rgba(_) => PixelFormat::StraightRgba8,
        PixelValue::Rgba16(_) => PixelFormat::StraightRgba16,
        _ => return Err(RasterError::PixelFormatMismatch),
    };
    let background = background
        .rgba16()
        .ok_or(RasterError::PixelFormatMismatch)?;
    let foreground_alpha = u64::from(foreground[3]);
    let inverse = u64::from(u16::MAX) - foreground_alpha;
    let background_alpha = u64::from(background[3]);
    let output_alpha = foreground_alpha + (background_alpha * inverse + 32_767) / 65_535;
    let mut output = [0_u16; 4];
    output[3] = output_alpha as u16;
    for channel in 0..3 {
        let numerator = u64::from(foreground[channel]) * foreground_alpha
            + (u64::from(background[channel]) * background_alpha * inverse + 32_767) / 65_535;
        output[channel] = (numerator + output_alpha / 2)
            .checked_div(output_alpha)
            .unwrap_or(0) as u16;
    }
    Ok(from_rgba16(format, output))
}

fn from_rgba16(format: PixelFormat, value: [u16; 4]) -> PixelValue {
    match format {
        PixelFormat::StraightRgba8 => {
            PixelValue::Rgba(value.map(|channel| ((u32::from(channel) + 128) / 257) as u8))
        }
        PixelFormat::StraightRgba16 => PixelValue::Rgba16(value),
        _ => unreachable!("validated straight RGBA format"),
    }
}

fn lerp_u16(left: u16, right: u16, amount_milli: u32) -> u16 {
    let amount = amount_milli.min(1_000);
    ((u64::from(left) * u64::from(1_000 - amount) + u64::from(right) * u64::from(amount) + 500)
        / 1_000) as u16
}

fn normalized_u16(value: f64) -> u16 {
    (value.clamp(0.0, 1.0) * 65_535.0).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba8(width: u32, height: u32) -> TileRaster {
        TileRaster::new(width, height, PixelFormat::StraightRgba8).unwrap()
    }

    #[test]
    fn m6_acceptance_eight_sixteen_bit_alpha_and_selection_edges_are_golden_fixed() {
        for format in [PixelFormat::StraightRgba8, PixelFormat::StraightRgba16] {
            let mut source = TileRaster::new(3, 1, format).unwrap();
            let left = from_rgba16(format, [10_000, 20_000, 30_000, 0]);
            let middle = from_rgba16(format, [20_000, 30_000, 40_000, 32_768]);
            let right = from_rgba16(format, [30_000, 40_000, 50_000, 65_535]);
            source.set_pixel(0, 0, left, 1).unwrap();
            source.set_pixel(1, 0, middle, 1).unwrap();
            source.set_pixel(2, 0, right, 1).unwrap();
            let mut selection = TileRaster::new(3, 1, PixelFormat::BinaryMask8).unwrap();
            selection
                .set_pixel(1, 0, PixelValue::Binary(255), 1)
                .unwrap();
            let output = apply_filter(
                &source,
                Some(&selection),
                &Filter::Invert {
                    channel: Channel::Rgb,
                },
                2,
            )
            .unwrap();
            assert_eq!(output.pixel(0, 0).unwrap(), left);
            assert_eq!(output.pixel(2, 0).unwrap(), right);
            let center = output.pixel(1, 0).unwrap().rgba16().unwrap();
            assert_eq!(center[3], middle.rgba16().unwrap()[3]);
        }
    }

    #[test]
    fn boundary_effect_never_changes_a_uniform_region() {
        let mut source = rgba8(7, 3);
        for y in 0..3 {
            for x in 0..7 {
                let color = if x < 3 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                source.set_pixel(x, y, PixelValue::Rgba(color), 1).unwrap();
            }
        }
        let output = apply_boundary_airbrush(
            &source,
            None,
            &BoundaryAirbrush {
                colors: vec![[65_535, 0, 0, 65_535], [0, 0, 65_535, 65_535]],
                width: 1,
                strength_milli: 1_000,
            },
            2,
        )
        .unwrap();
        assert_eq!(output.pixel(0, 1).unwrap(), source.pixel(0, 1).unwrap());
        assert_eq!(output.pixel(6, 1).unwrap(), source.pixel(6, 1).unwrap());
        assert_ne!(output.pixel(2, 1).unwrap(), source.pixel(2, 1).unwrap());
        assert_ne!(output.pixel(3, 1).unwrap(), source.pixel(3, 1).unwrap());
    }

    #[test]
    fn m6_filter_catalog_executes_with_bounded_parameters() {
        let mut source = TileRaster::new(3, 2, PixelFormat::StraightRgba16).unwrap();
        for y in 0..2 {
            for x in 0..3 {
                source
                    .set_pixel(
                        x,
                        y,
                        PixelValue::Rgba16([
                            (x * 10_000 + y * 2_000) as u16,
                            (x * 5_000 + 10_000) as u16,
                            (y * 20_000 + 5_000) as u16,
                            (20_000 + x * 10_000) as u16,
                        ]),
                        1,
                    )
                    .unwrap();
            }
        }
        let curve = vec![
            CurvePoint {
                input: 0,
                output: 0,
            },
            CurvePoint {
                input: 32_768,
                output: 40_000,
            },
            CurvePoint {
                input: 65_535,
                output: 65_535,
            },
        ];
        let filters = vec![
            Filter::SharpenWeak,
            Filter::SharpenStrong,
            Filter::BlurWeak,
            Filter::BlurStrong,
            Filter::GaussianBlur {
                radius: 1,
                strength_milli: 500,
            },
            Filter::UnsharpMask {
                radius: 1,
                amount_milli: 1_250,
                threshold: 64,
            },
            Filter::Invert {
                channel: Channel::Green,
            },
            Filter::AutoContrast,
            Filter::BrightnessContrast {
                brightness_milli: 100,
                contrast_milli: -200,
            },
            Filter::ToneCurve {
                channel: Channel::Rgb,
                interpolation: CurveInterpolation::Bezier,
                points: curve.clone(),
            },
            Filter::ToneCurve {
                channel: Channel::Blue,
                interpolation: CurveInterpolation::BSpline,
                points: curve,
            },
            Filter::Levels(Levels {
                channel: Channel::Red,
                input_shadow: 1_000,
                input_gamma_milli: 1_200,
                input_highlight: 64_000,
                output_shadow: 500,
                output_highlight: 65_000,
            }),
            Filter::Hsv(HsvAdjustment {
                hue_degrees_milli: 30_000,
                saturation_milli: 100,
                value_milli: -100,
            }),
            Filter::ColorBalance(ColorBalance {
                red_milli: 50,
                green_milli: -50,
                blue_milli: 100,
            }),
        ];
        for (revision, filter) in filters.iter().enumerate() {
            let output = apply_filter(&source, None, filter, revision as u64 + 2).unwrap();
            assert_eq!(output.format(), PixelFormat::StraightRgba16);
            assert_eq!((output.width(), output.height()), (3, 2));
        }
    }

    #[test]
    fn m6_gradient_airbrush_stamp_and_alpha_edit_are_typed_and_deterministic() {
        let source = rgba8(5, 5);
        let gradient = apply_gradient(
            &source,
            None,
            &Gradient {
                kind: GradientKind::Linear,
                mode: GradientMode::Overwrite,
                start_x_milli: 500,
                start_y_milli: 500,
                end_x_milli: 4_500,
                end_y_milli: 500,
                dither: false,
                stops: vec![
                    GradientStop {
                        position_milli: 0,
                        color: [65_535, 0, 0, 65_535],
                    },
                    GradientStop {
                        position_milli: 500,
                        color: [0, 65_535, 0, 32_768],
                    },
                    GradientStop {
                        position_milli: 1_000,
                        color: [0, 0, 65_535, 65_535],
                    },
                ],
            },
            2,
        )
        .unwrap();
        let sprayed = apply_airbrush(
            &gradient,
            None,
            AirbrushStroke {
                center_x_milli: 2_500,
                center_y_milli: 2_500,
                radius_milli: 2_000,
                hardness_milli: 500,
                opacity_milli: 500,
                color: [65_535; 4],
            },
            3,
        )
        .unwrap();
        let stamped = apply_stamp(
            &sprayed,
            None,
            Stamp {
                source_x: 0,
                source_y: 0,
                destination_x: 3,
                destination_y: 3,
                width: 2,
                height: 2,
                opacity_milli: 1_000,
            },
            4,
        )
        .unwrap();
        assert_ne!(stamped.checksum(), gradient.checksum());

        let mut alpha = TileRaster::new(5, 5, PixelFormat::Grayscale16).unwrap();
        alpha
            .set_pixel(2, 2, PixelValue::Grayscale16(12_345), 1)
            .unwrap();
        let before = stamped.pixel(2, 2).unwrap().rgba16().unwrap();
        let edited = edit_alpha(&stamped, None, &alpha, 5).unwrap();
        let after = edited.pixel(2, 2).unwrap().rgba16().unwrap();
        assert_eq!(&after[..3], &before[..3]);
        assert_eq!(after[3], ((12_345_u32 + 128) / 257 * 257) as u16);
    }
}
