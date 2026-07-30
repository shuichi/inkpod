use crate::document::bounded_document_pixels;
use crate::selection::paste_value;
use crate::*;

pub(crate) fn convert_main_line_raster(
    source: &TileRaster,
    grayscale: bool,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut destination = TileRaster::new(
        source.width(),
        source.height(),
        if grayscale {
            PixelFormat::Grayscale8
        } else {
            PixelFormat::BinaryMask8
        },
    )?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = match source.pixel(x, y)? {
                PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value,
                PixelValue::Grayscale16(value) => ((u32::from(value) + 128) / 257) as u8,
                _ => return Err(CoreError::InvalidState("main-line plane format is invalid")),
            };
            let value = if grayscale {
                PixelValue::Grayscale8(value)
            } else {
                PixelValue::Binary(if value >= 128 { 255 } else { 0 })
            };
            destination.set_pixel(x, y, value, revision)?;
        }
    }
    Ok(destination)
}

pub(crate) fn merge_raster(
    destination: &mut TileRaster,
    source: &TileRaster,
    revision: u64,
) -> Result<(), CoreError> {
    if destination.width() != source.width()
        || destination.height() != source.height()
        || destination.format() != source.format()
    {
        return Err(CoreError::InvalidArgument("merge raster formats differ"));
    }
    bounded_document_pixels(source.width(), source.height())?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let source_value = source.pixel(x, y)?;
            if source_value.is_zero() {
                continue;
            }
            let before = destination.pixel(x, y)?;
            let after = paste_value(
                before,
                source_value,
                match source.format() {
                    PixelFormat::BinaryMask8
                    | PixelFormat::Grayscale8
                    | PixelFormat::Grayscale16 => PlaneType::MainLine,
                    _ => PlaneType::Raster,
                },
            )?;
            destination.set_pixel(x, y, after, revision)?;
        }
    }
    Ok(())
}

pub(super) fn mirror_raster(
    source: &TileRaster,
    axis: MirrorAxis,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut destination = TileRaster::new(source.width(), source.height(), source.format())?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = source.pixel(x, y)?;
            if value.is_zero() {
                continue;
            }
            let (destination_x, destination_y) = match axis {
                MirrorAxis::Horizontal => (source.width() - 1 - x, y),
                MirrorAxis::Vertical => (x, source.height() - 1 - y),
            };
            destination.set_pixel(destination_x, destination_y, value, revision)?;
        }
    }
    Ok(destination)
}

pub(crate) fn convert_plane_raster(
    source: &TileRaster,
    destination_format: PixelFormat,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut destination = TileRaster::new(source.width(), source.height(), destination_format)?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = convert_plane_pixel(source.pixel(x, y)?, destination_format)?;
            if !value.is_zero() {
                destination.set_pixel(x, y, value, revision)?;
            }
        }
    }
    Ok(destination)
}

pub(crate) fn convert_plane_pixel(
    source: PixelValue,
    destination_format: PixelFormat,
) -> Result<PixelValue, CoreError> {
    let coverage16 = match source {
        PixelValue::Binary(value) | PixelValue::Grayscale8(value) => u16::from(value) * 257,
        PixelValue::Grayscale16(value) => value,
        PixelValue::Rgba(value) => u16::from(value[3]) * 257,
        PixelValue::Rgba16(value) => value[3],
    };
    let rgba16 = match source {
        PixelValue::Rgba(value) => [
            u16::from(value[0]) * 257,
            u16::from(value[1]) * 257,
            u16::from(value[2]) * 257,
            u16::from(value[3]) * 257,
        ],
        PixelValue::Rgba16(value) => value,
        _ => [0, 0, 0, coverage16],
    };
    match destination_format {
        PixelFormat::BinaryMask8 => Ok(PixelValue::Binary(if coverage16 == 0 { 0 } else { 255 })),
        PixelFormat::Grayscale8 => Ok(PixelValue::Grayscale8((coverage16 / 257) as u8)),
        PixelFormat::Grayscale16 => Ok(PixelValue::Grayscale16(coverage16)),
        PixelFormat::StraightRgba8 => Ok(PixelValue::Rgba([
            (rgba16[0] / 257) as u8,
            (rgba16[1] / 257) as u8,
            (rgba16[2] / 257) as u8,
            (rgba16[3] / 257) as u8,
        ])),
        PixelFormat::StraightRgba16 => Ok(PixelValue::Rgba16(rgba16)),
        PixelFormat::PremultipliedBgra8 => Err(CoreError::InvalidArgument(
            "premultiplied display format cannot be stored in a document plane",
        )),
    }
}

pub(crate) fn zero_pixel(format: PixelFormat) -> Result<PixelValue, CoreError> {
    match format {
        PixelFormat::BinaryMask8 => Ok(PixelValue::Binary(0)),
        PixelFormat::Grayscale8 => Ok(PixelValue::Grayscale8(0)),
        PixelFormat::Grayscale16 => Ok(PixelValue::Grayscale16(0)),
        PixelFormat::StraightRgba8 => Ok(PixelValue::Rgba([0; 4])),
        PixelFormat::StraightRgba16 => Ok(PixelValue::Rgba16([0; 4])),
        PixelFormat::PremultipliedBgra8 => Err(CoreError::InvalidArgument(
            "premultiplied display format cannot be stored in a document plane",
        )),
    }
}

pub(super) fn rotate_raster(
    source: &TileRaster,
    direction: RotateDirection,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.height(), source.width())?;
    let mut destination = TileRaster::new(source.height(), source.width(), source.format())?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = source.pixel(x, y)?;
            if value.is_zero() {
                continue;
            }
            let (destination_x, destination_y) = match direction {
                RotateDirection::Left90 => (y, source.width() - 1 - x),
                RotateDirection::Right90 => (source.height() - 1 - y, x),
            };
            destination.set_pixel(destination_x, destination_y, value, revision)?;
        }
    }
    Ok(destination)
}

pub(super) fn place_raster(
    source: &TileRaster,
    destination_size: DocumentSizeU32,
    offset: DocumentOffsetI32,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(destination_size.width, destination_size.height)?;
    let mut destination = TileRaster::new(
        destination_size.width,
        destination_size.height,
        source.format(),
    )?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let destination_x = i64::from(x) + i64::from(offset.x);
            let destination_y = i64::from(y) + i64::from(offset.y);
            if destination_x < 0
                || destination_y < 0
                || destination_x >= i64::from(destination_size.width)
                || destination_y >= i64::from(destination_size.height)
            {
                continue;
            }
            let value = source.pixel(x, y)?;
            if !value.is_zero() {
                destination.set_pixel(
                    destination_x as u32,
                    destination_y as u32,
                    value,
                    revision,
                )?;
            }
        }
    }
    Ok(destination)
}

pub(super) fn resample_raster_nearest(
    source: &TileRaster,
    width: u32,
    height: u32,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(width, height)?;
    let mut destination = TileRaster::new(width, height, source.format())?;
    for y in 0..height {
        let source_y = ((u64::from(y) * u64::from(source.height())) / u64::from(height))
            .min(u64::from(source.height() - 1)) as u32;
        for x in 0..width {
            let source_x = ((u64::from(x) * u64::from(source.width())) / u64::from(width))
                .min(u64::from(source.width() - 1)) as u32;
            let value = source.pixel(source_x, source_y)?;
            if !value.is_zero() {
                destination.set_pixel(x, y, value, revision)?;
            }
        }
    }
    Ok(destination)
}
