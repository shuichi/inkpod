use super::tiff::{Endian, read_u16, read_u32};
use super::*;

pub(super) fn encode_bmp(info: CommonRasterInfo, pixels: &[u8]) -> Result<Vec<u8>, FormatError> {
    const DIB_BYTES: u32 = 124;
    const PIXEL_OFFSET: u32 = 14 + DIB_BYTES;
    let pixel_length = u32::try_from(pixels.len())
        .map_err(|_| FormatError::Invalid("BMP pixel payload is too large"))?;
    let file_size = PIXEL_OFFSET
        .checked_add(pixel_length)
        .ok_or(FormatError::Invalid("BMP file length overflows"))?;
    let mut output = Vec::with_capacity(file_size as usize);
    output.extend_from_slice(b"BM");
    output.extend_from_slice(&file_size.to_le_bytes());
    output.extend_from_slice(&0_u32.to_le_bytes());
    output.extend_from_slice(&PIXEL_OFFSET.to_le_bytes());
    output.extend_from_slice(&DIB_BYTES.to_le_bytes());
    output.extend_from_slice(&(info.width as i32).to_le_bytes());
    output.extend_from_slice(&(-(info.height as i32)).to_le_bytes());
    output.extend_from_slice(&1_u16.to_le_bytes());
    output.extend_from_slice(&32_u16.to_le_bytes());
    output.extend_from_slice(&3_u32.to_le_bytes()); // BI_BITFIELDS
    output.extend_from_slice(&pixel_length.to_le_bytes());
    output.extend_from_slice(
        &(info.dpi_x_milli.map_or(0, dpi_milli_to_pixels_per_meter) as i32).to_le_bytes(),
    );
    output.extend_from_slice(
        &(info.dpi_y_milli.map_or(0, dpi_milli_to_pixels_per_meter) as i32).to_le_bytes(),
    );
    output.extend_from_slice(&0_u32.to_le_bytes());
    output.extend_from_slice(&0_u32.to_le_bytes());
    output.extend_from_slice(&0x00ff_0000_u32.to_le_bytes());
    output.extend_from_slice(&0x0000_ff00_u32.to_le_bytes());
    output.extend_from_slice(&0x0000_00ff_u32.to_le_bytes());
    output.extend_from_slice(&0xff00_0000_u32.to_le_bytes());
    output.extend_from_slice(b"sRGB");
    output.resize(PIXEL_OFFSET as usize, 0);
    for pixel in pixels.chunks_exact(4) {
        output.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(output)
}

pub(super) fn decode_bmp(bytes: &[u8]) -> Result<CommonRaster, FormatError> {
    if bytes.get(..2) != Some(b"BM") {
        return Err(FormatError::Invalid("BMP magic is invalid"));
    }
    let pixel_offset = read_u32(bytes, 10, Endian::Little)? as usize;
    let dib_bytes = read_u32(bytes, 14, Endian::Little)?;
    if dib_bytes < 40 {
        return Err(FormatError::Unsupported("BMP DIB header is unsupported"));
    }
    let width = read_u32(bytes, 18, Endian::Little)? as i32;
    let height = read_u32(bytes, 22, Endian::Little)? as i32;
    let planes = read_u16(bytes, 26, Endian::Little)?;
    let depth = read_u16(bytes, 28, Endian::Little)?;
    let compression = read_u32(bytes, 30, Endian::Little)?;
    if width <= 0
        || height == 0
        || planes != 1
        || !matches!((depth, compression), (24, 0) | (32, 0) | (32, 3))
    {
        return Err(FormatError::Unsupported(
            "BMP must be a 24-bit RGB or 32-bit RGB/bitfield image",
        ));
    }
    if compression == 3
        && (dib_bytes < 56
            || read_u32(bytes, 54, Endian::Little)? != 0x00ff_0000
            || read_u32(bytes, 58, Endian::Little)? != 0x0000_ff00
            || read_u32(bytes, 62, Endian::Little)? != 0x0000_00ff
            || read_u32(bytes, 66, Endian::Little)? != 0xff00_0000)
    {
        return Err(FormatError::Unsupported(
            "BMP bitfield masks are unsupported",
        ));
    }
    let width = width as u32;
    let absolute_height = height.unsigned_abs();
    validate_dimensions(width, absolute_height)?;
    let source_bpp = usize::from(depth / 8);
    let packed_row = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(source_bpp))
        .ok_or(FormatError::Invalid("BMP row length overflows"))?;
    let source_stride = packed_row
        .checked_add(3)
        .map(|row| row & !3)
        .ok_or(FormatError::Invalid("BMP row stride overflows"))?;
    let source_len = source_stride
        .checked_mul(absolute_height as usize)
        .filter(|bytes| *bytes <= MAX_COMMON_RASTER_BYTES)
        .ok_or(FormatError::Invalid("BMP pixel byte length overflows"))?;
    let source = bytes
        .get(pixel_offset..pixel_offset + source_len)
        .ok_or(FormatError::Invalid("BMP pixel data is truncated"))?;
    let mut pixels = vec![0_u8; expected_bytes(width, absolute_height, 4)?];
    for source_y in 0..absolute_height as usize {
        let destination_y = if height < 0 {
            source_y
        } else {
            absolute_height as usize - 1 - source_y
        };
        for x in 0..width as usize {
            let source_offset = source_y * source_stride + x * source_bpp;
            let destination_offset = (destination_y * width as usize + x) * 4;
            pixels[destination_offset..destination_offset + 4].copy_from_slice(&[
                source[source_offset + 2],
                source[source_offset + 1],
                source[source_offset],
                if source_bpp == 4 && compression == 3 {
                    source[source_offset + 3]
                } else {
                    u8::MAX
                },
            ]);
        }
    }
    let xppm = read_u32(bytes, 38, Endian::Little)? as i32;
    let yppm = read_u32(bytes, 42, Endian::Little)? as i32;
    let dpi_x = (xppm > 0).then(|| pixels_per_meter_to_dpi_milli(xppm as u32));
    let dpi_y = (yppm > 0).then(|| pixels_per_meter_to_dpi_milli(yppm as u32));
    CommonRaster::new(
        width,
        absolute_height,
        PixelFormat::StraightRgba8,
        dpi_x,
        dpi_y,
        pixels,
    )
}
