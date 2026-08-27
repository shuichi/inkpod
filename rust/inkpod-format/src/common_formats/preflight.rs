use super::tiff::{Endian, read_u16, read_u32};
use super::{CommonRasterFormat, FormatError, expected_bytes, validate_dimensions};

/// Returns a conservative bound for the RGBA pixel allocation before decoding.
///
/// This header-only query does not allocate pixels and does not replace the full
/// decoder's structural/checksum validation. Malformed headers and dimensions are
/// rejected. The returned byte count includes straight RGBA expansion and native
/// 16-bit depth; it excludes codec metadata and temporary scanline buffers.
pub fn common_raster_decoded_byte_limit(
    format: CommonRasterFormat,
    bytes: &[u8],
) -> Result<u64, FormatError> {
    let (width, height, bytes_per_pixel) = match format {
        CommonRasterFormat::Png => {
            if bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n")
                || bytes.get(12..16) != Some(b"IHDR")
                || read_u32(bytes, 8, Endian::Big)? != 13
            {
                return Err(FormatError::Invalid("PNG header is invalid"));
            }
            let depth = *bytes
                .get(24)
                .ok_or(FormatError::Invalid("PNG header is truncated"))?;
            if !matches!(depth, 1 | 2 | 4 | 8 | 16) {
                return Err(FormatError::Unsupported("PNG bit depth is unsupported"));
            }
            (
                read_u32(bytes, 16, Endian::Big)?,
                read_u32(bytes, 20, Endian::Big)?,
                if depth == 16 { 8 } else { 4 },
            )
        }
        CommonRasterFormat::Bmp => {
            if bytes.get(..2) != Some(b"BM") || read_u32(bytes, 14, Endian::Little)? < 40 {
                return Err(FormatError::Invalid("BMP header is invalid"));
            }
            let width = read_u32(bytes, 18, Endian::Little)? as i32;
            let height = read_u32(bytes, 22, Endian::Little)? as i32;
            if width <= 0 || height == 0 {
                return Err(FormatError::Invalid("BMP dimensions are invalid"));
            }
            (width as u32, height.unsigned_abs(), 4)
        }
        CommonRasterFormat::Tga => {
            let image_type = bytes.get(2).copied().unwrap_or(0);
            if !matches!(image_type, 1..=3 | 9..=11) || bytes.len() < 18 {
                return Err(FormatError::Unsupported("TGA image type is unsupported"));
            }
            (
                u32::from(read_u16(bytes, 12, Endian::Little)?),
                u32::from(read_u16(bytes, 14, Endian::Little)?),
                4,
            )
        }
        CommonRasterFormat::Tiff => tiff_dimensions(bytes)?,
    };
    validate_dimensions(width, height)?;
    Ok(expected_bytes(width, height, bytes_per_pixel)? as u64)
}

/// Bounds materialized pixel allocations that coexist during decode, including
/// the PNG/TGA source buffer and TGA color-map/postage pixels. Codec metadata and
/// bounded compression/scanline workspaces are separate from the pixel budget.
/// After decode the caller may reduce its reservation to the resident byte limit.
pub fn common_raster_decode_allocation_limit(
    format: CommonRasterFormat,
    bytes: &[u8],
) -> Result<u64, FormatError> {
    let resident = common_raster_decoded_byte_limit(format, bytes)?;
    let transient = match format {
        CommonRasterFormat::Png => resident,
        CommonRasterFormat::Tga => {
            let mut additional = resident;
            if bytes.get(1) == Some(&1) {
                additional += u64::from(read_u16(bytes, 5, Endian::Little)?) * 4;
            }
            if bytes.len() >= 26 && bytes.ends_with(b"TRUEVISION-XFILE.\0") {
                let extension = read_u32(bytes, bytes.len() - 26, Endian::Little)? as usize;
                if extension != 0 {
                    let end = extension
                        .checked_add(495)
                        .ok_or(FormatError::Invalid("TGA extension offset overflows"))?;
                    let header = bytes
                        .get(extension..end)
                        .ok_or(FormatError::Invalid("TGA extension is truncated"))?;
                    if read_u32(header, 482, Endian::Little)? != 0 {
                        additional += 256 * 8;
                    }
                    let postage = read_u32(header, 486, Endian::Little)? as usize;
                    if postage != 0 {
                        let width = u64::from(
                            *bytes
                                .get(postage)
                                .ok_or(FormatError::Invalid("TGA postage is truncated"))?,
                        );
                        let height = u64::from(
                            *bytes
                                .get(postage + 1)
                                .ok_or(FormatError::Invalid("TGA postage is truncated"))?,
                        );
                        // RGBA plus at most four source channels per pixel.
                        additional += width * height * 8;
                    }
                }
            }
            additional
        }
        CommonRasterFormat::Bmp | CommonRasterFormat::Tiff => 0,
    };
    resident
        .checked_add(transient)
        .ok_or(FormatError::Invalid("decoded allocation bound overflows"))
}

fn tiff_dimensions(bytes: &[u8]) -> Result<(u32, u32, usize), FormatError> {
    let endian = match bytes.get(..2) {
        Some(b"II") => Endian::Little,
        Some(b"MM") => Endian::Big,
        _ => return Err(FormatError::Invalid("TIFF byte order is invalid")),
    };
    if read_u16(bytes, 2, endian)? != 42 {
        return Err(FormatError::Invalid("TIFF magic is invalid"));
    }
    let ifd = read_u32(bytes, 4, endian)? as usize;
    let count = usize::from(read_u16(bytes, ifd, endian)?);
    if count > 256 {
        return Err(FormatError::Invalid(
            "TIFF IFD entry count exceeds its bound",
        ));
    }
    let mut width = None;
    let mut height = None;
    let mut bits = None;
    for index in 0..count {
        let offset = ifd
            .checked_add(2 + index * 12)
            .ok_or(FormatError::Invalid("TIFF IFD offset overflows"))?;
        let tag = read_u16(bytes, offset, endian)?;
        let kind = read_u16(bytes, offset + 2, endian)?;
        let count = read_u32(bytes, offset + 4, endian)?;
        let value = read_u32(bytes, offset + 8, endian)?;
        if matches!(tag, 256 | 257) {
            if count != 1 || !matches!(kind, 3 | 4) {
                return Err(FormatError::Invalid("TIFF dimension tag is invalid"));
            }
            let scalar = if kind == 3 {
                u32::from(read_u16(bytes, offset + 8, endian)?)
            } else {
                value
            };
            if tag == 256 {
                width = Some(scalar);
            } else {
                height = Some(scalar);
            }
        }
        if tag == 258 {
            bits = Some((kind, count, value));
        }
    }
    let (kind, count, offset) =
        bits.ok_or(FormatError::Invalid("TIFF bits-per-sample is missing"))?;
    if kind != 3 || !matches!(count, 3 | 4) {
        return Err(FormatError::Unsupported(
            "TIFF channel depths are unsupported",
        ));
    }
    let mut depth = 0_u16;
    for channel in 0..count as usize {
        let value = read_u16(bytes, offset as usize + channel * 2, endian)?;
        if !matches!(value, 8 | 16) {
            return Err(FormatError::Unsupported("TIFF bit depth is unsupported"));
        }
        depth = depth.max(value);
    }
    Ok((
        width.ok_or(FormatError::Invalid("TIFF width is missing"))?,
        height.ok_or(FormatError::Invalid("TIFF height is missing"))?,
        if depth == 16 { 8 } else { 4 },
    ))
}
