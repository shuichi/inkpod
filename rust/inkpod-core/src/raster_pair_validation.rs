use crate::{CommonRasterFormat, CoreError, DEFAULT_DPI_MILLI, PixelFormat};
use inkpod_format::{CommonRaster, decode_common_raster, encode_common_raster};

/// Compares canonical decoded pair content while treating missing DPI as the
/// Core's 96-DPI default. DPI is first reduced to what the selected raster
/// format can preserve; every other decoded field and every pixel stay exact.
pub(crate) fn canonical_raster_pair_eq(
    expected_format: CommonRasterFormat,
    expected: &CommonRaster,
    actual_format: CommonRasterFormat,
    actual: &CommonRaster,
) -> Result<bool, CoreError> {
    if expected_format != actual_format
        || expected.info.width != actual.info.width
        || expected.info.height != actual.info.height
        || expected.info.pixel_format != actual.info.pixel_format
        || expected.info.has_alpha != actual.info.has_alpha
        || expected.pixels != actual.pixels
    {
        return Ok(false);
    }
    if expected.info.dpi_x_milli == actual.info.dpi_x_milli
        && expected.info.dpi_y_milli == actual.info.dpi_y_milli
    {
        return Ok(true);
    }
    Ok(canonical_dpi(expected_format, expected)? == canonical_dpi(actual_format, actual)?)
}

pub(crate) fn canonical_raster_pair_eq_tiled(
    expected_format: CommonRasterFormat,
    expected: &CommonRaster,
    actual_format: CommonRasterFormat,
    actual_info: inkpod_format::CommonRasterInfo,
    actual: &crate::TileRaster,
) -> Result<bool, CoreError> {
    if expected_format != actual_format
        || expected.info.width != actual_info.width
        || expected.info.height != actual_info.height
        || expected.info.pixel_format != actual_info.pixel_format
        || expected.info.has_alpha != actual_info.has_alpha
        || actual.width() != actual_info.width
        || actual.height() != actual_info.height
        || actual.format() != actual_info.pixel_format
    {
        return Ok(false);
    }
    let bytes_per_pixel = actual_info.pixel_format.bytes_per_pixel();
    for (index, bytes) in expected.pixels.chunks_exact(bytes_per_pixel).enumerate() {
        let index = index as u64;
        let x = (index % u64::from(actual_info.width)) as u32;
        let y = (index / u64::from(actual_info.width)) as u32;
        let pixel = actual.pixel(x, y)?;
        let matches = match pixel {
            crate::PixelValue::Rgba(value) => bytes == value,
            crate::PixelValue::Rgba16(value) => bytes
                .chunks_exact(2)
                .zip(value)
                .all(|(bytes, value)| bytes == value.to_le_bytes()),
            _ => false,
        };
        if !matches {
            return Ok(false);
        }
    }
    if expected.info.dpi_x_milli == actual_info.dpi_x_milli
        && expected.info.dpi_y_milli == actual_info.dpi_y_milli
    {
        return Ok(true);
    }
    let actual_probe = CommonRaster::new(
        1,
        1,
        PixelFormat::StraightRgba8,
        actual_info.dpi_x_milli,
        actual_info.dpi_y_milli,
        vec![0, 0, 0, 0],
    )?;
    Ok(canonical_dpi(expected_format, expected)? == canonical_dpi(actual_format, &actual_probe)?)
}

fn canonical_dpi(
    format: CommonRasterFormat,
    raster: &CommonRaster,
) -> Result<(u32, u32), CoreError> {
    let probe = CommonRaster::new(
        1,
        1,
        PixelFormat::StraightRgba8,
        Some(raster.info.dpi_x_milli.unwrap_or(DEFAULT_DPI_MILLI)),
        Some(raster.info.dpi_y_milli.unwrap_or(DEFAULT_DPI_MILLI)),
        vec![0, 0, 0, 0],
    )?;
    let encoded = encode_common_raster(format, &probe, false)?;
    let decoded = decode_common_raster(format, &encoded)?;
    Ok((
        decoded.info.dpi_x_milli.unwrap_or(DEFAULT_DPI_MILLI),
        decoded.info.dpi_y_milli.unwrap_or(DEFAULT_DPI_MILLI),
    ))
}
