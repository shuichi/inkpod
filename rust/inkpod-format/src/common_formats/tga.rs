use super::*;

pub(super) fn encode_tga(info: CommonRasterInfo, pixels: &[u8]) -> Result<Vec<u8>, FormatError> {
    let width = u16::try_from(info.width)
        .map_err(|_| FormatError::Unsupported("TGA dimensions exceed 16-bit fields"))?;
    let height = u16::try_from(info.height)
        .map_err(|_| FormatError::Unsupported("TGA dimensions exceed 16-bit fields"))?;
    let mut output = vec![0_u8; 18];
    output[2] = 2;
    output[12..14].copy_from_slice(&width.to_le_bytes());
    output[14..16].copy_from_slice(&height.to_le_bytes());
    output[16] = 32;
    output[17] = 0x28; // top-left origin, eight alpha bits
    output.reserve(pixels.len());
    for pixel in pixels.chunks_exact(4) {
        output.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(output)
}

pub(super) fn decode_tga(bytes: &[u8]) -> Result<CommonRaster, FormatError> {
    if bytes.len() < 18 || bytes[1] != 0 || !matches!(bytes[2], 2 | 10) {
        return Err(FormatError::Unsupported(
            "TGA must be an uncompressed or RLE true-color image",
        ));
    }
    let id_length = bytes[0] as usize;
    let width = u32::from(u16::from_le_bytes([bytes[12], bytes[13]]));
    let height = u32::from(u16::from_le_bytes([bytes[14], bytes[15]]));
    let depth = bytes[16];
    if !matches!(depth, 24 | 32) {
        return Err(FormatError::Unsupported("TGA pixel depth is unsupported"));
    }
    let alpha_bits = bytes[17] & 0x0f;
    if (depth == 24 && alpha_bits != 0) || (depth == 32 && !matches!(alpha_bits, 0 | 8)) {
        return Err(FormatError::Unsupported(
            "TGA alpha attribute depth is unsupported",
        ));
    }
    let source_bpp = usize::from(depth / 8);
    let start = 18_usize
        .checked_add(id_length)
        .ok_or(FormatError::Invalid("TGA ID length overflows"))?;
    let source = bytes
        .get(start..)
        .ok_or(FormatError::Invalid("TGA pixel data is truncated"))?;
    let top_origin = bytes[17] & 0x20 != 0;
    let right_origin = bytes[17] & 0x10 != 0;
    let width_usize = width as usize;
    let height_usize = height as usize;
    let pixel_count = expected_bytes(width, height, 1)?;
    let mut pixels = vec![0_u8; expected_bytes(width, height, 4)?];
    let mut write_pixel = |source_index: usize, source_pixel: &[u8]| {
        let source_y = source_index / width_usize;
        let source_x = source_index % width_usize;
        let destination_y = if top_origin {
            source_y
        } else {
            height_usize - 1 - source_y
        };
        let destination_x = if right_origin {
            width_usize - 1 - source_x
        } else {
            source_x
        };
        let destination_offset = (destination_y * width_usize + destination_x) * 4;
        pixels[destination_offset..destination_offset + 4].copy_from_slice(&[
            source_pixel[2],
            source_pixel[1],
            source_pixel[0],
            if source_bpp == 4 && alpha_bits == 8 {
                source_pixel[3]
            } else {
                u8::MAX
            },
        ]);
    };

    if bytes[2] == 2 {
        let source_len = expected_bytes(width, height, source_bpp)?;
        let source = source
            .get(..source_len)
            .ok_or(FormatError::Invalid("TGA pixel data is truncated"))?;
        for (source_index, source_pixel) in source.chunks_exact(source_bpp).enumerate() {
            write_pixel(source_index, source_pixel);
        }
    } else {
        let mut cursor = 0_usize;
        let mut source_index = 0_usize;
        while source_index < pixel_count {
            let packet_header = *source
                .get(cursor)
                .ok_or(FormatError::Invalid("TGA RLE pixel data is truncated"))?;
            cursor += 1;
            let packet_pixels = usize::from(packet_header & 0x7f) + 1;
            if packet_pixels > pixel_count - source_index {
                return Err(FormatError::Invalid("TGA RLE packet exceeds image bounds"));
            }
            if packet_header & 0x80 != 0 {
                let packet_end = cursor
                    .checked_add(source_bpp)
                    .ok_or(FormatError::Invalid("TGA RLE pixel data is truncated"))?;
                let source_pixel = source
                    .get(cursor..packet_end)
                    .ok_or(FormatError::Invalid("TGA RLE pixel data is truncated"))?;
                cursor = packet_end;
                for _ in 0..packet_pixels {
                    write_pixel(source_index, source_pixel);
                    source_index += 1;
                }
            } else {
                let packet_bytes = packet_pixels
                    .checked_mul(source_bpp)
                    .ok_or(FormatError::Invalid("TGA RLE pixel data length overflows"))?;
                let packet_end = cursor
                    .checked_add(packet_bytes)
                    .ok_or(FormatError::Invalid("TGA RLE pixel data length overflows"))?;
                let packet = source
                    .get(cursor..packet_end)
                    .ok_or(FormatError::Invalid("TGA RLE pixel data is truncated"))?;
                cursor = packet_end;
                for source_pixel in packet.chunks_exact(source_bpp) {
                    write_pixel(source_index, source_pixel);
                    source_index += 1;
                }
            }
        }
    }
    CommonRaster::new(
        width,
        height,
        PixelFormat::StraightRgba8,
        None,
        None,
        pixels,
    )
}
