use super::MAX_IMAGE_EDIT_PIXELS;
use crate::{PixelFormat, PixelValue, RasterError, TileRaster};

pub(super) fn validate_color_raster(raster: &TileRaster) -> Result<(), RasterError> {
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

pub(super) fn validate_selection(
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

pub(super) fn selected(
    selection: Option<&TileRaster>,
    x: u32,
    y: u32,
) -> Result<bool, RasterError> {
    match selection {
        None => Ok(true),
        Some(selection) => Ok(matches!(selection.pixel(x, y)?, PixelValue::Binary(255))),
    }
}

pub(super) fn source_over(
    background: PixelValue,
    foreground: [u16; 4],
) -> Result<PixelValue, RasterError> {
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

pub(super) fn from_rgba16(format: PixelFormat, value: [u16; 4]) -> PixelValue {
    match format {
        PixelFormat::StraightRgba8 => {
            PixelValue::Rgba(value.map(|channel| ((u32::from(channel) + 128) / 257) as u8))
        }
        PixelFormat::StraightRgba16 => PixelValue::Rgba16(value),
        _ => unreachable!("validated straight RGBA format"),
    }
}

pub(super) fn lerp_u16(left: u16, right: u16, amount_milli: u32) -> u16 {
    let amount = amount_milli.min(1_000);
    ((u64::from(left) * u64::from(1_000 - amount) + u64::from(right) * u64::from(amount) + 500)
        / 1_000) as u16
}

pub(super) fn normalized_u16(value: f64) -> u16 {
    (value.clamp(0.0, 1.0) * 65_535.0).round() as u16
}
