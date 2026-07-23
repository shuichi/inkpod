use crate::{PixelFormat, PixelValue, RasterError, TileRaster};

pub const MAX_FILTER_RADIUS: u32 = 64;
pub const MAX_CURVE_POINTS: usize = 64;
pub const MAX_GRADIENT_STOPS: usize = 64;
/// Bounds allocations and synchronous work for one image-edit transaction.
/// 8192 x 8192 is the largest full-plane edit accepted by the current M6
/// implementation; radius-dependent effects have an additional work bound.
pub const MAX_IMAGE_EDIT_PIXELS: u64 = 67_108_864;
const MAX_IMAGE_EDIT_WORK: u128 = 1_100_000_000;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectSample {
    pub x_milli: i64,
    pub y_milli: i64,
    pub pressure_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AirbrushGesture {
    pub samples: Vec<EffectSample>,
    pub radius_milli: u32,
    pub hardness_milli: u32,
    pub spacing_milli: u32,
    pub opacity_milli: u32,
    pub fade_milli: u32,
    pub pressure_size: bool,
    pub pressure_opacity: bool,
    pub continuous_dabs: u32,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StampGesture {
    pub source_x_milli: i64,
    pub source_y_milli: i64,
    pub samples: Vec<EffectSample>,
    pub radius_milli: u32,
    pub hardness_milli: u32,
    pub spacing_milli: u32,
    pub opacity_milli: u32,
    pub shape: StampShape,
    pub pressure_size: bool,
    pub pressure_opacity: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StampShape {
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DustMode {
    RemoveForeground,
    FillTransparentHoles,
    ReplaceColorOutliers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DustRemoval {
    pub mode: DustMode,
    pub maximum_pixels: u32,
}

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

pub fn apply_dust_removal(
    source: &TileRaster,
    operation_mask: Option<&TileRaster>,
    options: DustRemoval,
    revision: u64,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, operation_mask)?;
    if options.maximum_pixels == 0 || options.maximum_pixels > 65_536 {
        return Err(RasterError::InvalidDimensions);
    }
    let pixel_count = usize::try_from(u64::from(source.width()) * u64::from(source.height()))
        .map_err(|_| RasterError::InvalidDimensions)?;
    let total = u64::try_from(pixel_count).map_err(|_| RasterError::InvalidDimensions)?;
    let mut visited = vec![false; pixel_count];
    let mut result = source.clone();
    let mut completed = 0_u64;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let index = raster_index(source.width(), x, y)?;
            if visited[index] || !selected(operation_mask, x, y)? {
                completed += 1;
                continue;
            }
            let seed = source
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            let eligible = match options.mode {
                DustMode::RemoveForeground => seed[3] != 0,
                DustMode::FillTransparentHoles => seed[3] == 0,
                DustMode::ReplaceColorOutliers => true,
            };
            if !eligible {
                visited[index] = true;
                completed += 1;
                continue;
            }
            let mut queue = std::collections::VecDeque::from([(x, y)]);
            let mut component = Vec::new();
            let mut oversized = false;
            let mut touches_boundary = false;
            while let Some((candidate_x, candidate_y)) = queue.pop_front() {
                let candidate_index = raster_index(source.width(), candidate_x, candidate_y)?;
                if visited[candidate_index] || !selected(operation_mask, candidate_x, candidate_y)?
                {
                    continue;
                }
                let value = source
                    .pixel(candidate_x, candidate_y)?
                    .rgba16()
                    .ok_or(RasterError::PixelFormatMismatch)?;
                let same_component = match options.mode {
                    DustMode::RemoveForeground => value[3] != 0,
                    DustMode::FillTransparentHoles => value[3] == 0,
                    DustMode::ReplaceColorOutliers => value == seed,
                };
                if !same_component {
                    continue;
                }
                visited[candidate_index] = true;
                if component.len() <= options.maximum_pixels as usize {
                    component.push((candidate_x, candidate_y));
                } else {
                    oversized = true;
                }
                touches_boundary |= candidate_x == 0
                    || candidate_y == 0
                    || candidate_x + 1 == source.width()
                    || candidate_y + 1 == source.height();
                for neighbor in
                    four_neighbors(candidate_x, candidate_y, source.width(), source.height())
                {
                    queue.push_back(neighbor);
                }
            }
            completed = completed.saturating_add(component.len() as u64);
            if !progress(completed.min(total), total) {
                return Err(RasterError::Cancelled);
            }
            if component.is_empty()
                || oversized
                || component.len() > options.maximum_pixels as usize
            {
                continue;
            }
            if options.mode == DustMode::FillTransparentHoles && touches_boundary {
                continue;
            }
            let replacement = match options.mode {
                DustMode::RemoveForeground => [0; 4],
                DustMode::FillTransparentHoles | DustMode::ReplaceColorOutliers => {
                    surrounding_average(source, &component, seed)?
                }
            };
            if options.mode == DustMode::ReplaceColorOutliers && replacement == seed {
                continue;
            }
            let replacement = from_rgba16(source.format(), replacement);
            for (component_x, component_y) in component {
                result.set_pixel(component_x, component_y, replacement, revision)?;
            }
        }
        if !progress(completed.min(total), total) {
            return Err(RasterError::Cancelled);
        }
    }
    if !progress(total, total) {
        return Err(RasterError::Cancelled);
    }
    Ok(result)
}

fn raster_index(width: u32, x: u32, y: u32) -> Result<usize, RasterError> {
    usize::try_from(u64::from(y) * u64::from(width) + u64::from(x))
        .map_err(|_| RasterError::InvalidDimensions)
}

fn four_neighbors(x: u32, y: u32, width: u32, height: u32) -> Vec<(u32, u32)> {
    let mut result = Vec::with_capacity(4);
    if x > 0 {
        result.push((x - 1, y));
    }
    if x + 1 < width {
        result.push((x + 1, y));
    }
    if y > 0 {
        result.push((x, y - 1));
    }
    if y + 1 < height {
        result.push((x, y + 1));
    }
    result
}

fn surrounding_average(
    source: &TileRaster,
    component: &[(u32, u32)],
    component_color: [u16; 4],
) -> Result<[u16; 4], RasterError> {
    let component_set = component
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut sums = [0_u128; 4];
    let mut count = 0_u128;
    for &(x, y) in component {
        for (neighbor_x, neighbor_y) in four_neighbors(x, y, source.width(), source.height()) {
            if component_set.contains(&(neighbor_x, neighbor_y)) {
                continue;
            }
            let value = source
                .pixel(neighbor_x, neighbor_y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            if value == component_color {
                continue;
            }
            for channel in 0..4 {
                sums[channel] += u128::from(value[channel]);
            }
            count += 1;
        }
    }
    if count == 0 {
        return Ok(component_color);
    }
    Ok(std::array::from_fn(|channel| {
        ((sums[channel] + count / 2) / count) as u16
    }))
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

pub fn apply_alpha_gradient(
    source: &TileRaster,
    selection: Option<&TileRaster>,
    gradient: &Gradient,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    let alpha_source = apply_gradient(source, selection, gradient, revision)?;
    let mut result = source.clone();
    for y in 0..source.height() {
        for x in 0..source.width() {
            if !selected(selection, x, y)? {
                continue;
            }
            let mut original = source
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            original[3] = alpha_source
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?[3];
            result.set_pixel(x, y, from_rgba16(source.format(), original), revision)?;
        }
    }
    Ok(result)
}

fn validate_color_raster(raster: &TileRaster) -> Result<(), RasterError> {
    let pixels = u64::from(raster.width())
        .checked_mul(u64::from(raster.height()))
        .ok_or(RasterError::InvalidDimensions)?;
    if pixels <= MAX_IMAGE_EDIT_PIXELS
        && matches!(
            raster.format(),
            PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
        )
    {
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

#[allow(clippy::too_many_arguments)]
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
        if !progress(progress_base + u64::from(y) + 1, progress_total.max(1)) {
            return Err(RasterError::Cancelled);
        }
    }
    Ok(result)
}

fn validate_radius_work(source: &TileRaster, radius: u32) -> Result<(), RasterError> {
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
            let expected = match format {
                PixelFormat::StraightRgba8 => PixelValue::Rgba([177, 138, 99, 128]),
                PixelFormat::StraightRgba16 => PixelValue::Rgba16([45_535, 35_535, 25_535, 32_768]),
                _ => unreachable!("test uses straight RGBA formats"),
            };
            assert_eq!(output.pixel(1, 0).unwrap(), expected);
        }
    }

    #[test]
    fn m6_invalid_extremes_and_oversized_work_are_rejected_without_panicking() {
        let source = rgba8(2, 2);
        for filter in [
            Filter::BrightnessContrast {
                brightness_milli: i32::MIN,
                contrast_milli: 0,
            },
            Filter::Hsv(HsvAdjustment {
                hue_degrees_milli: i32::MIN,
                saturation_milli: 0,
                value_milli: 0,
            }),
            Filter::ColorBalance(ColorBalance {
                red_milli: i32::MIN,
                green_milli: 0,
                blue_milli: 0,
            }),
        ] {
            assert!(apply_filter(&source, None, &filter, 2).is_err());
        }

        let extreme_gradient = Gradient {
            kind: GradientKind::Linear,
            mode: GradientMode::Overwrite,
            start_x_milli: i64::MIN,
            start_y_milli: i64::MIN,
            end_x_milli: i64::MAX,
            end_y_milli: i64::MAX,
            dither: false,
            stops: vec![
                GradientStop {
                    position_milli: 0,
                    color: [0; 4],
                },
                GradientStop {
                    position_milli: 500,
                    color: [32_768; 4],
                },
                GradientStop {
                    position_milli: 1_000,
                    color: [65_535; 4],
                },
            ],
        };
        assert!(apply_gradient(&source, None, &extreme_gradient, 2).is_ok());
        assert!(
            apply_airbrush(
                &source,
                None,
                AirbrushStroke {
                    center_x_milli: i64::MIN,
                    center_y_milli: i64::MAX,
                    radius_milli: 1_000,
                    hardness_milli: 0,
                    opacity_milli: 1_000,
                    color: [65_535; 4],
                },
                2,
            )
            .is_ok()
        );
        assert_eq!(
            apply_stamp(
                &source,
                None,
                Stamp {
                    source_x: i32::MIN,
                    source_y: i32::MIN,
                    destination_x: i32::MAX,
                    destination_y: i32::MAX,
                    width: u32::MAX,
                    height: u32::MAX,
                    opacity_milli: 1_000,
                },
                2,
            )
            .unwrap(),
            source
        );

        let oversized = TileRaster::new(8_193, 8_193, PixelFormat::StraightRgba8).unwrap();
        assert!(apply_filter(&oversized, None, &Filter::AutoContrast, 2).is_err());
        let expensive = TileRaster::new(1_024, 1_024, PixelFormat::StraightRgba8).unwrap();
        assert!(
            apply_filter(
                &expensive,
                None,
                &Filter::GaussianBlur {
                    radius: MAX_FILTER_RADIUS,
                    strength_milli: 1_000,
                },
                2,
            )
            .is_err()
        );
    }

    #[test]
    fn m6_full_effect_gestures_are_deterministic_and_pressure_aware() {
        let source = rgba8(8, 4);
        let gesture = AirbrushGesture {
            samples: vec![
                EffectSample {
                    x_milli: 1_500,
                    y_milli: 1_500,
                    pressure_milli: 250,
                },
                EffectSample {
                    x_milli: 6_500,
                    y_milli: 1_500,
                    pressure_milli: 1_000,
                },
            ],
            radius_milli: 1_500,
            hardness_milli: 500,
            spacing_milli: 500,
            opacity_milli: 1_000,
            fade_milli: 250,
            pressure_size: true,
            pressure_opacity: true,
            continuous_dabs: 2,
            color: [65_535, 0, 0, 65_535],
        };
        let first = apply_airbrush_gesture(&source, None, &gesture, 2).unwrap();
        let second = apply_airbrush_gesture(&source, None, &gesture, 2).unwrap();
        assert_eq!(first, second);
        assert_ne!(first, source);
        assert!(
            first.pixel(6, 1).unwrap().rgba16().unwrap()[3]
                > first.pixel(1, 1).unwrap().rgba16().unwrap()[3]
        );

        let mut stamp_source = rgba8(8, 4);
        stamp_source
            .set_pixel(1, 1, PixelValue::Rgba([0, 255, 0, 255]), 1)
            .unwrap();
        stamp_source
            .set_pixel(3, 1, PixelValue::Rgba([0, 255, 0, 255]), 1)
            .unwrap();
        let stamped = apply_stamp_gesture(
            &stamp_source,
            None,
            &StampGesture {
                source_x_milli: 1_500,
                source_y_milli: 1_500,
                samples: vec![
                    EffectSample {
                        x_milli: 4_500,
                        y_milli: 1_500,
                        pressure_milli: 1_000,
                    },
                    EffectSample {
                        x_milli: 6_500,
                        y_milli: 1_500,
                        pressure_milli: 500,
                    },
                ],
                radius_milli: 600,
                hardness_milli: 1_000,
                spacing_milli: 1_000,
                opacity_milli: 1_000,
                shape: StampShape::Round,
                pressure_size: true,
                pressure_opacity: true,
            },
            2,
        )
        .unwrap();
        assert_eq!(
            stamped.pixel(4, 1).unwrap(),
            PixelValue::Rgba([0, 255, 0, 255])
        );
        assert_ne!(
            stamped.pixel(6, 1).unwrap(),
            stamp_source.pixel(6, 1).unwrap()
        );
    }

    #[test]
    fn paint_003_dust_modes_preview_bounds_and_cancel_are_atomic() {
        let mut point = rgba8(5, 5);
        point
            .set_pixel(2, 2, PixelValue::Rgba([255, 0, 0, 255]), 1)
            .unwrap();
        let removed = apply_dust_removal(
            &point,
            None,
            DustRemoval {
                mode: DustMode::RemoveForeground,
                maximum_pixels: 1,
            },
            2,
            |_, _| true,
        )
        .unwrap();
        assert_eq!(removed.pixel(2, 2).unwrap(), PixelValue::Rgba([0; 4]));

        let mut hole = rgba8(5, 5);
        for y in 1..4 {
            for x in 1..4 {
                hole.set_pixel(x, y, PixelValue::Rgba([20, 40, 60, 255]), 1)
                    .unwrap();
            }
        }
        hole.set_pixel(2, 2, PixelValue::Rgba([0; 4]), 1).unwrap();
        let filled = apply_dust_removal(
            &hole,
            None,
            DustRemoval {
                mode: DustMode::FillTransparentHoles,
                maximum_pixels: 1,
            },
            2,
            |_, _| true,
        )
        .unwrap();
        assert_eq!(
            filled.pixel(2, 2).unwrap(),
            PixelValue::Rgba([20, 40, 60, 255])
        );

        let mut outlier = hole.clone();
        outlier
            .set_pixel(2, 2, PixelValue::Rgba([0, 0, 255, 255]), 1)
            .unwrap();
        let replaced = apply_dust_removal(
            &outlier,
            None,
            DustRemoval {
                mode: DustMode::ReplaceColorOutliers,
                maximum_pixels: 1,
            },
            2,
            |_, _| true,
        )
        .unwrap();
        assert_eq!(
            replaced.pixel(2, 2).unwrap(),
            PixelValue::Rgba([20, 40, 60, 255])
        );

        let mut polls = 0;
        assert_eq!(
            apply_dust_removal(
                &outlier,
                None,
                DustRemoval {
                    mode: DustMode::ReplaceColorOutliers,
                    maximum_pixels: 8
                },
                2,
                |_, _| {
                    polls += 1;
                    polls < 2
                },
            ),
            Err(RasterError::Cancelled)
        );
        assert_eq!(
            apply_filter_with_progress(&outlier, None, &Filter::AutoContrast, 2, |_, _| false,),
            Err(RasterError::Cancelled)
        );
    }

    #[test]
    fn adjust_001_alpha_gradient_never_changes_rgb() {
        let mut source = rgba8(3, 1);
        for x in 0..3 {
            source
                .set_pixel(x, 0, PixelValue::Rgba([10, 20, 30, 200]), 1)
                .unwrap();
        }
        let output = apply_alpha_gradient(
            &source,
            None,
            &Gradient {
                kind: GradientKind::Linear,
                mode: GradientMode::Overwrite,
                start_x_milli: 500,
                start_y_milli: 500,
                end_x_milli: 2_500,
                end_y_milli: 500,
                dither: false,
                stops: vec![
                    GradientStop {
                        position_milli: 0,
                        color: [0, 0, 0, 0],
                    },
                    GradientStop {
                        position_milli: 500,
                        color: [0, 0, 0, 32_768],
                    },
                    GradientStop {
                        position_milli: 1_000,
                        color: [0, 0, 0, 65_535],
                    },
                ],
            },
            2,
        )
        .unwrap();
        for x in 0..3 {
            assert_eq!(
                &output.pixel(x, 0).unwrap().rgba16().unwrap()[..3],
                &[2_570, 5_140, 7_710]
            );
        }
        assert_eq!(output.pixel(0, 0).unwrap().rgba16().unwrap()[3], 0);
        assert_eq!(output.pixel(2, 0).unwrap().rgba16().unwrap()[3], 65_535);
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
