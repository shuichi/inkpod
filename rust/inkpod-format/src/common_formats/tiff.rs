use super::*;

pub(super) fn encode_tiff(info: CommonRasterInfo, pixels: &[u8]) -> Result<Vec<u8>, FormatError> {
    const ENTRY_COUNT: u16 = 14;
    let ifd_offset = 8_u32;
    let ifd_bytes = 2_u32 + u32::from(ENTRY_COUNT) * 12 + 4;
    let bits_offset = ifd_offset + ifd_bytes;
    let x_resolution_offset = bits_offset + 8;
    let y_resolution_offset = x_resolution_offset + 8;
    let pixel_offset = y_resolution_offset + 8;
    let pixel_length = u32::try_from(pixels.len())
        .map_err(|_| FormatError::Invalid("TIFF pixel payload is too large"))?;
    let mut output = Vec::with_capacity(pixel_offset as usize + pixels.len());
    output.extend_from_slice(b"II");
    output.extend_from_slice(&42_u16.to_le_bytes());
    output.extend_from_slice(&ifd_offset.to_le_bytes());
    output.extend_from_slice(&ENTRY_COUNT.to_le_bytes());
    let bits = if info.pixel_format == PixelFormat::StraightRgba8 {
        8
    } else {
        16
    };
    let dpi_x = info.dpi_x_milli.unwrap_or(96_000);
    let dpi_y = info.dpi_y_milli.unwrap_or(96_000);
    for (tag, kind, count, value) in [
        (256_u16, 4_u16, 1_u32, info.width),
        (257, 4, 1, info.height),
        (258, 3, 4, bits_offset),
        (259, 3, 1, 1),
        (262, 3, 1, 2),
        (273, 4, 1, pixel_offset),
        (277, 3, 1, 4),
        (278, 4, 1, info.height),
        (279, 4, 1, pixel_length),
        (282, 5, 1, x_resolution_offset),
        (283, 5, 1, y_resolution_offset),
        (284, 3, 1, 1),
        (296, 3, 1, 2),
        (338, 3, 1, 2), // one unassociated (straight) alpha sample
    ] {
        tiff_entry(&mut output, tag, kind, count, value);
    }
    output.extend_from_slice(&0_u32.to_le_bytes());
    for _ in 0..4 {
        output.extend_from_slice(&(bits as u16).to_le_bytes());
    }
    output.extend_from_slice(&dpi_x.to_le_bytes());
    output.extend_from_slice(&1_000_u32.to_le_bytes());
    output.extend_from_slice(&dpi_y.to_le_bytes());
    output.extend_from_slice(&1_000_u32.to_le_bytes());
    output.extend_from_slice(pixels);
    Ok(output)
}

fn tiff_entry(output: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: u32) {
    output.extend_from_slice(&tag.to_le_bytes());
    output.extend_from_slice(&kind.to_le_bytes());
    output.extend_from_slice(&count.to_le_bytes());
    output.extend_from_slice(&value.to_le_bytes());
}

#[derive(Clone, Copy)]
pub(super) enum Endian {
    Little,
    Big,
}

pub(super) fn decode_tiff(bytes: &[u8]) -> Result<CommonRaster, FormatError> {
    let endian = match bytes.get(..2) {
        Some(b"II") => Endian::Little,
        Some(b"MM") => Endian::Big,
        _ => return Err(FormatError::Invalid("TIFF byte order is invalid")),
    };
    if read_u16(bytes, 2, endian)? != 42 {
        return Err(FormatError::Invalid("TIFF magic is invalid"));
    }
    let ifd = read_u32(bytes, 4, endian)? as usize;
    let count = read_u16(bytes, ifd, endian)? as usize;
    if count > 256 {
        return Err(FormatError::Invalid(
            "TIFF IFD entry count exceeds its bound",
        ));
    }
    let mut width = None;
    let mut height = None;
    let mut bits_offset = None;
    let mut compression = 1_u32;
    let mut photometric = None;
    let mut strip_offset = None;
    let mut samples = None;
    let mut rows_per_strip = None;
    let mut strip_bytes = None;
    let mut x_resolution = None;
    let mut y_resolution = None;
    let mut planar = 1_u32;
    let mut resolution_unit = 2_u32;
    let mut extra_samples = None;
    for index in 0..count {
        let offset = ifd
            .checked_add(2 + index * 12)
            .ok_or(FormatError::Invalid("TIFF IFD offset overflows"))?;
        let tag = read_u16(bytes, offset, endian)?;
        let kind = read_u16(bytes, offset + 2, endian)?;
        let item_count = read_u32(bytes, offset + 4, endian)?;
        let value = read_u32(bytes, offset + 8, endian)?;
        let scalar = || tiff_scalar(bytes, offset + 8, endian, kind, item_count, value);
        match tag {
            256 => width = Some(scalar()?),
            257 => height = Some(scalar()?),
            258 => bits_offset = Some((kind, item_count, value)),
            259 => compression = scalar()?,
            262 => photometric = Some(scalar()?),
            273 => strip_offset = Some(scalar()?),
            277 => samples = Some(scalar()?),
            278 => rows_per_strip = Some(scalar()?),
            279 => strip_bytes = Some(scalar()?),
            282 => x_resolution = Some(read_rational(bytes, value as usize, endian)?),
            283 => y_resolution = Some(read_rational(bytes, value as usize, endian)?),
            284 => planar = scalar()?,
            296 => resolution_unit = scalar()?,
            338 => extra_samples = Some(scalar()?),
            _ => {}
        }
    }
    let width = width.ok_or(FormatError::Invalid("TIFF width is missing"))?;
    let height = height.ok_or(FormatError::Invalid("TIFF height is missing"))?;
    let samples = samples.ok_or(FormatError::Invalid("TIFF samples are missing"))?;
    if compression != 1
        || photometric != Some(2)
        || planar != 1
        || rows_per_strip != Some(height)
        || !matches!(samples, 3 | 4)
        || (samples == 4 && extra_samples != Some(2))
        || (samples == 3 && extra_samples.is_some())
    {
        return Err(FormatError::Unsupported(
            "TIFF must be one uncompressed chunky RGB(A) strip with straight alpha",
        ));
    }
    let (bits_kind, bits_count, bits_value) =
        bits_offset.ok_or(FormatError::Invalid("TIFF bits-per-sample is missing"))?;
    if bits_kind != 3 || bits_count != samples {
        return Err(FormatError::Unsupported(
            "TIFF channel depths are unsupported",
        ));
    }
    let bits_position = bits_value as usize;
    let mut bit_depth = None;
    for channel in 0..samples as usize {
        let depth = read_u16(bytes, bits_position + channel * 2, endian)?;
        if bit_depth
            .replace(depth)
            .is_some_and(|previous| previous != depth)
        {
            return Err(FormatError::Unsupported("TIFF channel depths differ"));
        }
    }
    let bit_depth = bit_depth.unwrap_or(0);
    let format = match bit_depth {
        8 => PixelFormat::StraightRgba8,
        16 => PixelFormat::StraightRgba16,
        _ => return Err(FormatError::Unsupported("TIFF bit depth is unsupported")),
    };
    let channel_bytes = usize::from(bit_depth / 8);
    let source_len = expected_bytes(width, height, samples as usize * channel_bytes)?;
    if strip_bytes != Some(source_len as u32) {
        return Err(FormatError::Invalid(
            "TIFF strip byte count is inconsistent",
        ));
    }
    let start = strip_offset.ok_or(FormatError::Invalid("TIFF strip offset is missing"))? as usize;
    let source = bytes
        .get(start..start + source_len)
        .ok_or(FormatError::Invalid("TIFF pixel strip is truncated"))?;
    let mut pixels = Vec::with_capacity(expected_bytes(width, height, 4 * channel_bytes)?);
    for pixel in source.chunks_exact(samples as usize * channel_bytes) {
        for channel in 0..3 {
            let start = channel * channel_bytes;
            if channel_bytes == 1 || matches!(endian, Endian::Little) {
                pixels.extend_from_slice(&pixel[start..start + channel_bytes]);
            } else {
                pixels.extend(pixel[start..start + channel_bytes].iter().rev());
            }
        }
        if samples == 4 {
            let start = 3 * channel_bytes;
            if channel_bytes == 1 || matches!(endian, Endian::Little) {
                pixels.extend_from_slice(&pixel[start..start + channel_bytes]);
            } else {
                pixels.extend(pixel[start..start + channel_bytes].iter().rev());
            }
        } else if channel_bytes == 1 {
            pixels.push(u8::MAX);
        } else {
            pixels.extend_from_slice(&u16::MAX.to_le_bytes());
        }
    }
    let dpi = |resolution: Option<(u32, u32)>| -> Option<u32> {
        let (numerator, denominator) = resolution?;
        if denominator == 0 {
            return None;
        }
        let scale = match resolution_unit {
            2 => 1_000_u64,
            3 => 2_540_u64,
            _ => return None,
        };
        u32::try_from(
            (u64::from(numerator) * scale + u64::from(denominator) / 2) / u64::from(denominator),
        )
        .ok()
    };
    CommonRaster::new(
        width,
        height,
        format,
        dpi(x_resolution),
        dpi(y_resolution),
        pixels,
    )
}

fn tiff_scalar(
    bytes: &[u8],
    value_offset: usize,
    endian: Endian,
    kind: u16,
    count: u32,
    value: u32,
) -> Result<u32, FormatError> {
    if count != 1 {
        return Err(FormatError::Unsupported("TIFF array tag is unsupported"));
    }
    match kind {
        3 => Ok(u32::from(read_u16(bytes, value_offset, endian)?)),
        4 => Ok(value),
        _ => Err(FormatError::Unsupported("TIFF scalar type is unsupported")),
    }
}

fn read_rational(bytes: &[u8], offset: usize, endian: Endian) -> Result<(u32, u32), FormatError> {
    Ok((
        read_u32(bytes, offset, endian)?,
        read_u32(bytes, offset + 4, endian)?,
    ))
}

pub(super) fn read_u16(bytes: &[u8], offset: usize, endian: Endian) -> Result<u16, FormatError> {
    let value: [u8; 2] = bytes
        .get(offset..offset + 2)
        .ok_or(FormatError::Invalid("common raster field is truncated"))?
        .try_into()
        .map_err(|_| FormatError::Invalid("common raster field is truncated"))?;
    Ok(match endian {
        Endian::Little => u16::from_le_bytes(value),
        Endian::Big => u16::from_be_bytes(value),
    })
}

pub(super) fn read_u32(bytes: &[u8], offset: usize, endian: Endian) -> Result<u32, FormatError> {
    let value: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(FormatError::Invalid("common raster field is truncated"))?
        .try_into()
        .map_err(|_| FormatError::Invalid("common raster field is truncated"))?;
    Ok(match endian {
        Endian::Little => u32::from_le_bytes(value),
        Endian::Big => u32::from_be_bytes(value),
    })
}
