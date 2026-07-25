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
    if bytes.len() < 18 || bytes[1] != 0 || bytes[2] != 2 {
        return Err(FormatError::Unsupported(
            "TGA must be an uncompressed true-color image",
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
    let source_len = expected_bytes(width, height, source_bpp)?;
    let start = 18_usize
        .checked_add(id_length)
        .ok_or(FormatError::Invalid("TGA ID length overflows"))?;
    let source = bytes
        .get(start..start + source_len)
        .ok_or(FormatError::Invalid("TGA pixel data is truncated"))?;
    let top_origin = bytes[17] & 0x20 != 0;
    let right_origin = bytes[17] & 0x10 != 0;
    let mut pixels = vec![0_u8; expected_bytes(width, height, 4)?];
    for source_y in 0..height as usize {
        let destination_y = if top_origin {
            source_y
        } else {
            height as usize - 1 - source_y
        };
        for source_x in 0..width as usize {
            let destination_x = if right_origin {
                width as usize - 1 - source_x
            } else {
                source_x
            };
            let source_offset = (source_y * width as usize + source_x) * source_bpp;
            let destination_offset = (destination_y * width as usize + destination_x) * 4;
            pixels[destination_offset..destination_offset + 4].copy_from_slice(&[
                source[source_offset + 2],
                source[source_offset + 1],
                source[source_offset],
                if source_bpp == 4 && alpha_bits == 8 {
                    source[source_offset + 3]
                } else {
                    u8::MAX
                },
            ]);
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
