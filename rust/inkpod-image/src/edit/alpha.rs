use super::common::*;
use super::*;
use crate::{PixelFormat, PixelValue, RasterError, TileRaster};

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
