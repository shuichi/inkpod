use super::*;
use std::io::Cursor;

pub(super) fn encode_png(info: CommonRasterInfo, pixels: &[u8]) -> Result<Vec<u8>, FormatError> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, info.width, info.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(match info.pixel_format {
            PixelFormat::StraightRgba8 => png::BitDepth::Eight,
            PixelFormat::StraightRgba16 => png::BitDepth::Sixteen,
            _ => return Err(FormatError::Invalid("PNG requires RGBA pixels")),
        });
        if let (Some(x), Some(y)) = (info.dpi_x_milli, info.dpi_y_milli) {
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: dpi_milli_to_pixels_per_meter(x),
                yppu: dpi_milli_to_pixels_per_meter(y),
                unit: png::Unit::Meter,
            }));
        }
        let mut writer = encoder
            .write_header()
            .map_err(|_| FormatError::Invalid("PNG header encoding failed"))?;
        if info.pixel_format == PixelFormat::StraightRgba16 {
            let mut big_endian = pixels.to_vec();
            for channel in big_endian.chunks_exact_mut(2) {
                channel.swap(0, 1);
            }
            writer
                .write_image_data(&big_endian)
                .map_err(|_| FormatError::Invalid("PNG pixel encoding failed"))?;
        } else {
            writer
                .write_image_data(pixels)
                .map_err(|_| FormatError::Invalid("PNG pixel encoding failed"))?;
        }
    }
    Ok(output)
}

pub(super) fn decode_png(bytes: &[u8]) -> Result<CommonRaster, FormatError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|_| FormatError::Invalid("PNG header is invalid"))?;
    validate_dimensions(reader.info().width, reader.info().height)?;
    if reader.output_buffer_size() > MAX_COMMON_RASTER_BYTES {
        return Err(FormatError::Invalid(
            "PNG decoded pixel buffer exceeds its bound",
        ));
    }
    let mut source = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut source)
        .map_err(|_| FormatError::Invalid("PNG pixel data is invalid"))?;
    source.truncate(frame.buffer_size());
    let format = match frame.bit_depth {
        png::BitDepth::Eight => PixelFormat::StraightRgba8,
        png::BitDepth::Sixteen => PixelFormat::StraightRgba16,
        _ => return Err(FormatError::Unsupported("PNG bit depth is unsupported")),
    };
    let channel_bytes = if format == PixelFormat::StraightRgba8 {
        1
    } else {
        2
    };
    let pixel_count = usize::try_from(frame.width)
        .ok()
        .and_then(|width| width.checked_mul(frame.height as usize))
        .ok_or(FormatError::Invalid("PNG dimensions overflow"))?;
    let mut pixels = Vec::with_capacity(expected_bytes(
        frame.width,
        frame.height,
        4 * channel_bytes,
    )?);
    match frame.color_type {
        png::ColorType::Rgba => pixels.extend_from_slice(&source),
        png::ColorType::Rgb => {
            let stride = 3 * channel_bytes;
            for pixel in source.chunks_exact(stride) {
                pixels.extend_from_slice(pixel);
                if channel_bytes == 1 {
                    pixels.push(u8::MAX);
                } else {
                    pixels.extend_from_slice(&u16::MAX.to_be_bytes());
                }
            }
        }
        png::ColorType::Grayscale => {
            for sample in source.chunks_exact(channel_bytes) {
                pixels.extend_from_slice(sample);
                pixels.extend_from_slice(sample);
                pixels.extend_from_slice(sample);
                if channel_bytes == 1 {
                    pixels.push(u8::MAX);
                } else {
                    pixels.extend_from_slice(&u16::MAX.to_be_bytes());
                }
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for sample in source.chunks_exact(2 * channel_bytes) {
                let gray = &sample[..channel_bytes];
                pixels.extend_from_slice(gray);
                pixels.extend_from_slice(gray);
                pixels.extend_from_slice(gray);
                pixels.extend_from_slice(&sample[channel_bytes..]);
            }
        }
        png::ColorType::Indexed => {
            return Err(FormatError::Unsupported(
                "indexed PNG requires palette expansion before import",
            ));
        }
    }
    if pixels.len() != pixel_count * 4 * channel_bytes {
        return Err(FormatError::Invalid("PNG decoded pixel length is invalid"));
    }
    if format == PixelFormat::StraightRgba16 {
        for channel in pixels.chunks_exact_mut(2) {
            channel.swap(0, 1);
        }
    }
    let pixel_dims = reader.info().pixel_dims;
    let (dpi_x_milli, dpi_y_milli) = pixel_dims
        .filter(|dimensions| dimensions.unit == png::Unit::Meter)
        .map_or((None, None), |dimensions| {
            (
                Some(pixels_per_meter_to_dpi_milli(dimensions.xppu)),
                Some(pixels_per_meter_to_dpi_milli(dimensions.yppu)),
            )
        });
    CommonRaster::new(
        frame.width,
        frame.height,
        format,
        dpi_x_milli,
        dpi_y_milli,
        pixels,
    )
}
