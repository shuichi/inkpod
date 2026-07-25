use crate::{PixelValue, RasterError, TileRaster};
#[derive(Clone, Copy, Debug)]
pub struct PlaneSample<'a> {
    pub raster: &'a TileRaster,
    /// Binary/grayscale line planes return this exact base color to the
    /// eyedropper while display composition applies the stored coverage.
    pub base_color: Option<PixelValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EyedropperSource {
    TopmostNonTransparent,
    SelectedPlane,
    Composite,
    LightTableTopmost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorCheckMode {
    LegacyWhiteTransparency,
    NativeAlpha,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorCheckCategory {
    Colored,
    ExactWhite,
    Transparent,
}

pub fn eyedropper(
    source: EyedropperSource,
    x: u32,
    y: u32,
    selected: PlaneSample<'_>,
    top_to_bottom: &[PlaneSample<'_>],
    light_table_top_to_bottom: &[PlaneSample<'_>],
) -> Result<Option<PixelValue>, RasterError> {
    match source {
        EyedropperSource::SelectedPlane => sample_plane_exact(selected, x, y),
        EyedropperSource::TopmostNonTransparent => first_visible_sample(top_to_bottom, x, y),
        EyedropperSource::LightTableTopmost => {
            first_visible_sample(light_table_top_to_bottom, x, y)
        }
        EyedropperSource::Composite => {
            let mut composite = None;
            for plane in top_to_bottom.iter().rev() {
                if let Some(sample) = sample_plane_display(*plane, x, y)? {
                    composite = Some(match composite {
                        Some(background) => blend_over(background, sample),
                        None => sample,
                    });
                }
            }
            Ok(composite)
        }
    }
}

#[must_use]
pub const fn color_check_category(value: PixelValue, mode: ColorCheckMode) -> ColorCheckCategory {
    if matches!(mode, ColorCheckMode::NativeAlpha) && value.is_transparent() {
        ColorCheckCategory::Transparent
    } else if value.is_exact_white() {
        ColorCheckCategory::ExactWhite
    } else {
        ColorCheckCategory::Colored
    }
}

fn first_visible_sample(
    planes: &[PlaneSample<'_>],
    x: u32,
    y: u32,
) -> Result<Option<PixelValue>, RasterError> {
    for plane in planes {
        if let Some(value) = sample_plane_exact(*plane, x, y)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn sample_plane_exact(
    plane: PlaneSample<'_>,
    x: u32,
    y: u32,
) -> Result<Option<PixelValue>, RasterError> {
    let value = plane.raster.pixel(x, y)?;
    match value {
        PixelValue::Binary(0) | PixelValue::Grayscale8(0) | PixelValue::Grayscale16(0) => Ok(None),
        PixelValue::Binary(_) | PixelValue::Grayscale8(_) | PixelValue::Grayscale16(_) => plane
            .base_color
            .ok_or(RasterError::PixelFormatMismatch)
            .map(Some),
        PixelValue::Rgba(value) if value[3] == 0 => Ok(None),
        PixelValue::Rgba16(value) if value[3] == 0 => Ok(None),
        PixelValue::Rgba(_) | PixelValue::Rgba16(_) => Ok(Some(value)),
    }
}

fn sample_plane_display(
    plane: PlaneSample<'_>,
    x: u32,
    y: u32,
) -> Result<Option<PixelValue>, RasterError> {
    let value = plane.raster.pixel(x, y)?;
    let coverage = match value {
        PixelValue::Binary(value) | PixelValue::Grayscale8(value) => u16::from(value) * 257,
        PixelValue::Grayscale16(value) => value,
        PixelValue::Rgba(value) => return Ok((value[3] != 0).then_some(PixelValue::Rgba(value))),
        PixelValue::Rgba16(value) => {
            return Ok((value[3] != 0).then_some(PixelValue::Rgba16(value)));
        }
    };
    if coverage == 0 {
        return Ok(None);
    }
    let base = plane.base_color.ok_or(RasterError::PixelFormatMismatch)?;
    Ok(Some(match base {
        PixelValue::Rgba(mut value) => {
            value[3] = ((u32::from(value[3]) * u32::from(coverage) + 32_767) / 65_535) as u8;
            PixelValue::Rgba(value)
        }
        PixelValue::Rgba16(mut value) => {
            value[3] = ((u32::from(value[3]) * u32::from(coverage) + 32_767) / 65_535) as u16;
            PixelValue::Rgba16(value)
        }
        _ => return Err(RasterError::PixelFormatMismatch),
    }))
}

fn blend_over(background: PixelValue, foreground: PixelValue) -> PixelValue {
    let background16 = background.rgba16().expect("validated RGBA background");
    let foreground16 = foreground.rgba16().expect("validated RGBA foreground");
    let foreground_alpha = u64::from(foreground16[3]);
    let inverse = u64::from(u16::MAX) - foreground_alpha;
    let background_alpha = u64::from(background16[3]);
    let output_alpha = foreground_alpha + (background_alpha * inverse + 32_767) / 65_535;
    let mut output = [0_u16; 4];
    output[3] = output_alpha as u16;
    if output_alpha != 0 {
        for channel in 0..3 {
            let foreground_premultiplied = u64::from(foreground16[channel]) * foreground_alpha;
            let background_premultiplied = u64::from(background16[channel]) * background_alpha;
            let numerator =
                foreground_premultiplied + (background_premultiplied * inverse + 32_767) / 65_535;
            output[channel] = (numerator + output_alpha / 2)
                .checked_div(output_alpha)
                .unwrap_or(0) as u16;
        }
    }
    if matches!(background, PixelValue::Rgba(_)) && matches!(foreground, PixelValue::Rgba(_)) {
        PixelValue::Rgba(output.map(|channel| ((u32::from(channel) + 128) / 257) as u8))
    } else {
        PixelValue::Rgba16(output)
    }
}
