use super::model::*;
use crate::{CommonRaster, FormatError, MAX_COMMON_RASTER_BYTES};
use inkpod_image::PixelFormat;

const HEADER_BYTES: usize = 18;
const FOOTER_BYTES: usize = 26;
const EXTENSION_BYTES: usize = 495;
const FOOTER_SIGNATURE: &[u8; 18] = b"TRUEVISION-XFILE.\0";
const COLOR_CORRECTION_ENTRIES: usize = 256;
const COLOR_CORRECTION_BYTES: usize = COLOR_CORRECTION_ENTRIES * 8;

#[derive(Clone, Copy)]
struct Header {
    image_type: u8,
    width: u32,
    height: u32,
    depth: u8,
    descriptor: u8,
    color_map_first: u16,
    color_map_length: u16,
    color_map_depth: u8,
    color_map_type: u8,
    id_length: usize,
    x_origin: u16,
    y_origin: u16,
}

#[derive(Clone, Copy)]
struct Footer {
    extension_offset: usize,
    developer_offset: usize,
}

#[derive(Clone)]
struct ParsedExtension {
    value: TgaExtension,
    color_correction_offset: usize,
    postage_offset: usize,
    scan_line_offset: usize,
}

pub(super) fn decode_document(bytes: &[u8]) -> Result<TgaDocument, FormatError> {
    if bytes.len() > MAX_COMMON_RASTER_BYTES {
        return Err(FormatError::Invalid("TGA file exceeds byte limit"));
    }
    let header = parse_header(bytes)?;
    let footer = parse_footer(bytes)?;
    let mut parsed_extension = footer
        .filter(|footer| footer.extension_offset != 0)
        .map(|footer| parse_extension_fixed(bytes, footer.extension_offset))
        .transpose()?;
    let alpha_type = parsed_extension
        .as_ref()
        .map(|value| value.value.alpha_type);
    let origin = TgaOrigin::from_descriptor(header.descriptor);
    let mut cursor = HEADER_BYTES
        .checked_add(header.id_length)
        .ok_or(FormatError::Invalid("TGA Image ID length overflows"))?;
    let image_id = bytes
        .get(HEADER_BYTES..cursor)
        .ok_or(FormatError::Invalid("TGA Image ID is truncated"))?
        .to_vec();
    let color_map = if header.color_map_type == 1 {
        let (map, next) = decode_color_map(bytes, cursor, header, alpha_type)?;
        cursor = next;
        Some(map)
    } else {
        None
    };
    let image_data_start = cursor;
    let (raster, image_end) = decode_image(
        bytes,
        cursor,
        header,
        origin,
        color_map.as_ref(),
        alpha_type,
    )?;
    if let Some(extension) = &mut parsed_extension {
        decode_extension_blocks(
            bytes,
            extension,
            header,
            origin,
            color_map.as_ref(),
            image_data_start,
            image_end,
        )?;
    }
    let developer_fields = footer
        .filter(|footer| footer.developer_offset != 0)
        .map(|footer| decode_developer_fields(bytes, footer.developer_offset))
        .transpose()?
        .unwrap_or_default();
    let image_format = image_format_from_header(header)?;
    let compression = if matches!(header.image_type, 9..=11) {
        TgaCompression::RunLengthEncoded
    } else {
        TgaCompression::Uncompressed
    };
    Ok(TgaDocument {
        raster,
        options: TgaEncodeOptions {
            image_format,
            compression,
            origin,
            color_map,
            metadata: TgaMetadata {
                image_id,
                x_origin: header.x_origin,
                y_origin: header.y_origin,
                extension: parsed_extension.map(|value| value.value),
                developer_fields,
                write_footer: footer.is_some(),
            },
            alpha_loss: TgaAlphaLoss::Reject,
            grayscale_conversion: TgaGrayscaleConversion::RequireExact,
            allow_color_precision_loss: matches!(
                image_format,
                TgaImageFormat::TrueColor { depth: 16 }
                    | TgaImageFormat::ColorMapped {
                        entry_depth: 15 | 16,
                        ..
                    }
            ),
            allow_alpha_precision_loss: false,
        },
    })
}

fn parse_header(bytes: &[u8]) -> Result<Header, FormatError> {
    let header = bytes
        .get(..HEADER_BYTES)
        .ok_or(FormatError::Invalid("TGA header is truncated"))?;
    let value = Header {
        id_length: header[0] as usize,
        color_map_type: header[1],
        image_type: header[2],
        color_map_first: read_u16(header, 3)?,
        color_map_length: read_u16(header, 5)?,
        color_map_depth: header[7],
        x_origin: read_u16(header, 8)?,
        y_origin: read_u16(header, 10)?,
        width: u32::from(read_u16(header, 12)?),
        height: u32::from(read_u16(header, 14)?),
        depth: header[16],
        descriptor: header[17],
    };
    if value.descriptor & 0xc0 != 0 {
        return Err(FormatError::Unsupported(
            "TGA descriptor reserved bits are nonzero",
        ));
    }
    if !matches!(value.color_map_type, 0 | 1) {
        return Err(FormatError::Unsupported(
            "TGA color-map type is unsupported",
        ));
    }
    if !matches!(value.image_type, 0 | 1 | 2 | 3 | 9 | 10 | 11) {
        return Err(FormatError::Unsupported("TGA image type is unsupported"));
    }
    if value.color_map_type == 0
        && (value.color_map_first != 0 || value.color_map_length != 0 || value.color_map_depth != 0)
    {
        return Err(FormatError::Invalid(
            "TGA absent color-map fields must be zero",
        ));
    }
    if value.color_map_type == 1
        && (value.color_map_length == 0 || !matches!(value.color_map_depth, 15 | 16 | 24 | 32))
    {
        return Err(FormatError::Unsupported(
            "TGA color-map specification is unsupported",
        ));
    }
    if matches!(value.image_type, 1 | 9) && value.color_map_type != 1 {
        return Err(FormatError::Invalid(
            "TGA color-mapped image has no color map",
        ));
    }
    if value.image_type != 0 {
        super::super::validate_dimensions(value.width, value.height)?;
    }
    let alpha_bits = value.descriptor & 0x0f;
    match value.image_type {
        0 if value.depth != 0 => {
            return Err(FormatError::Invalid(
                "TGA no-image type has a nonzero pixel depth",
            ));
        }
        1 | 9 if !matches!(value.depth, 8 | 16) || alpha_bits != 0 => {
            return Err(FormatError::Unsupported(
                "TGA color-mapped pixel layout is unsupported",
            ));
        }
        2 | 10 => match value.depth {
            16 if matches!(alpha_bits, 0 | 1) => {}
            24 if alpha_bits == 0 => {}
            32 if matches!(alpha_bits, 0 | 8) => {}
            _ => {
                return Err(FormatError::Unsupported(
                    "TGA true-color pixel layout is unsupported",
                ));
            }
        },
        3 | 11 if value.depth != 8 || alpha_bits != 0 => {
            return Err(FormatError::Unsupported(
                "TGA black-and-white pixel layout is unsupported",
            ));
        }
        _ => {}
    }
    Ok(value)
}

fn parse_footer(bytes: &[u8]) -> Result<Option<Footer>, FormatError> {
    if bytes.len() < FOOTER_BYTES {
        return Ok(None);
    }
    let start = bytes.len() - FOOTER_BYTES;
    if bytes.get(start + 8..start + FOOTER_BYTES) != Some(FOOTER_SIGNATURE.as_slice()) {
        return Ok(None);
    }
    Ok(Some(Footer {
        extension_offset: read_u32(bytes, start)? as usize,
        developer_offset: read_u32(bytes, start + 4)? as usize,
    }))
}

fn parse_extension_fixed(bytes: &[u8], offset: usize) -> Result<ParsedExtension, FormatError> {
    let size = usize::from(read_u16(bytes, offset)?);
    if size < EXTENSION_BYTES {
        return Err(FormatError::Invalid(
            "TGA extension area is shorter than version 2.0",
        ));
    }
    let end = offset
        .checked_add(size)
        .ok_or(FormatError::Invalid("TGA extension area length overflows"))?;
    let extension = bytes
        .get(offset..end)
        .ok_or(FormatError::Invalid("TGA extension area is truncated"))?;
    let timestamp_values = read_u16_values(extension, 367, 6)?;
    let timestamp = if timestamp_values.iter().all(|value| *value == 0) {
        None
    } else {
        let value = TgaTimestamp {
            month: timestamp_values[0],
            day: timestamp_values[1],
            year: timestamp_values[2],
            hour: timestamp_values[3],
            minute: timestamp_values[4],
            second: timestamp_values[5],
        };
        value.validate()?;
        Some(value)
    };
    let duration_values = read_u16_values(extension, 420, 3)?;
    let job_duration = if duration_values.iter().all(|value| *value == 0) {
        None
    } else {
        let value = TgaDuration {
            hours: duration_values[0],
            minutes: duration_values[1],
            seconds: duration_values[2],
        };
        value.validate()?;
        Some(value)
    };
    let software_version_letter = match extension[469] {
        0 | b' ' => None,
        value if value.is_ascii_graphic() => Some(value),
        _ => {
            return Err(FormatError::Invalid(
                "TGA software version letter is not printable ASCII",
            ));
        }
    };
    let mut comments = Vec::with_capacity(4);
    for index in 0..4 {
        let start = 43 + index * 81;
        comments.push(decode_fixed_text(&extension[start..start + 81])?);
    }
    let author_comments: [String; 4] = comments
        .try_into()
        .map_err(|_| FormatError::Invalid("TGA author comment count is invalid"))?;
    let value = TgaExtension {
        author_name: decode_fixed_text(&extension[2..43])?,
        author_comments,
        timestamp,
        job_name: decode_fixed_text(&extension[379..420])?,
        job_duration,
        software_id: decode_fixed_text(&extension[426..467])?,
        software_version: read_u16(extension, 467)?,
        software_version_letter,
        key_color: [
            extension[471],
            extension[472],
            extension[473],
            extension[470],
        ],
        pixel_aspect_ratio: decode_ratio(extension, 474)?,
        gamma: decode_ratio(extension, 478)?,
        color_correction_table: None,
        postage_stamp: None,
        scan_line_table: read_u32(extension, 490)? != 0,
        alpha_type: TgaAlphaType::from_code(extension[494])?,
        extra: extension[EXTENSION_BYTES..].to_vec(),
    };
    validate_extension(&value)?;
    Ok(ParsedExtension {
        color_correction_offset: read_u32(extension, 482)? as usize,
        postage_offset: read_u32(extension, 486)? as usize,
        scan_line_offset: read_u32(extension, 490)? as usize,
        value,
    })
}

fn decode_fixed_text(bytes: &[u8]) -> Result<String, FormatError> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let text = &bytes[..end];
    if !text.is_ascii() {
        return Err(FormatError::Invalid("TGA extension text is not ASCII"));
    }
    String::from_utf8(text.to_vec())
        .map_err(|_| FormatError::Invalid("TGA extension text is invalid"))
}

fn decode_ratio(bytes: &[u8], offset: usize) -> Result<Option<TgaRatio>, FormatError> {
    let numerator = read_u16(bytes, offset)?;
    let denominator = read_u16(bytes, offset + 2)?;
    if denominator == 0 {
        Ok(None)
    } else {
        Ok(Some(TgaRatio {
            numerator,
            denominator,
        }))
    }
}

fn decode_color_map(
    bytes: &[u8],
    offset: usize,
    header: Header,
    alpha_type: Option<TgaAlphaType>,
) -> Result<(TgaColorMap, usize), FormatError> {
    let entry_bytes = usize::from(header.color_map_depth.div_ceil(8));
    let length = usize::from(header.color_map_length);
    let byte_length = length
        .checked_mul(entry_bytes)
        .ok_or(FormatError::Invalid("TGA color-map length overflows"))?;
    let end = offset
        .checked_add(byte_length)
        .ok_or(FormatError::Invalid("TGA color-map length overflows"))?;
    let source = bytes
        .get(offset..end)
        .ok_or(FormatError::Invalid("TGA color-map data is truncated"))?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(length)
        .map_err(|_| FormatError::Invalid("TGA color-map allocation failed"))?;
    let old_format_alpha = if header.color_map_depth == 16 {
        1
    } else if header.color_map_depth == 32 {
        8
    } else {
        0
    };
    for encoded in source.chunks_exact(entry_bytes) {
        entries.push(decode_color(
            encoded,
            header.color_map_depth,
            old_format_alpha,
            alpha_type,
        )?);
    }
    let map = TgaColorMap {
        first_index: header.color_map_first,
        entry_depth: header.color_map_depth,
        entries,
    };
    validate_color_map(&map)?;
    Ok((map, end))
}

fn decode_image(
    bytes: &[u8],
    offset: usize,
    header: Header,
    origin: TgaOrigin,
    color_map: Option<&TgaColorMap>,
    alpha_type: Option<TgaAlphaType>,
) -> Result<(Option<CommonRaster>, usize), FormatError> {
    if header.image_type == 0 {
        return Ok((None, offset));
    }
    let source_bpp = usize::from(header.depth / 8);
    let pixel_count = super::super::expected_bytes(header.width, header.height, 1)?;
    let (samples, end) = if matches!(header.image_type, 9..=11) {
        decode_rle_samples(bytes, offset, pixel_count, source_bpp)?
    } else {
        let length = pixel_count
            .checked_mul(source_bpp)
            .ok_or(FormatError::Invalid("TGA image data length overflows"))?;
        let end = offset
            .checked_add(length)
            .ok_or(FormatError::Invalid("TGA image data length overflows"))?;
        let source = bytes
            .get(offset..end)
            .ok_or(FormatError::Invalid("TGA pixel data is truncated"))?;
        (source.to_vec(), end)
    };
    let mut pixels = vec![0_u8; super::super::expected_bytes(header.width, header.height, 4)?];
    let width = header.width as usize;
    let height = header.height as usize;
    for (source_index, sample) in samples.chunks_exact(source_bpp).enumerate() {
        let rgba = match header.image_type {
            1 | 9 => decode_index(sample, header.depth, color_map)?,
            2 | 10 => decode_color(sample, header.depth, header.descriptor & 0x0f, alpha_type)?,
            3 | 11 => [sample[0], sample[0], sample[0], u8::MAX],
            _ => unreachable!("validated standard TGA image type"),
        };
        let source_y = source_index / width;
        let source_x = source_index % width;
        let y = if origin.top() {
            source_y
        } else {
            height - 1 - source_y
        };
        let x = if origin.right() {
            width - 1 - source_x
        } else {
            source_x
        };
        let destination = (y * width + x) * 4;
        pixels[destination..destination + 4].copy_from_slice(&rgba);
    }
    Ok((
        Some(CommonRaster::new(
            header.width,
            header.height,
            PixelFormat::StraightRgba8,
            None,
            None,
            pixels,
        )?),
        end,
    ))
}

fn decode_rle_samples(
    bytes: &[u8],
    offset: usize,
    pixel_count: usize,
    bytes_per_sample: usize,
) -> Result<(Vec<u8>, usize), FormatError> {
    let capacity = pixel_count
        .checked_mul(bytes_per_sample)
        .ok_or(FormatError::Invalid("TGA RLE image length overflows"))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| FormatError::Invalid("TGA RLE allocation failed"))?;
    let mut cursor = offset;
    let mut decoded = 0_usize;
    while decoded < pixel_count {
        let packet = *bytes
            .get(cursor)
            .ok_or(FormatError::Invalid("TGA RLE pixel data is truncated"))?;
        cursor += 1;
        let count = usize::from(packet & 0x7f) + 1;
        if count > pixel_count - decoded {
            return Err(FormatError::Invalid("TGA RLE packet exceeds image bounds"));
        }
        if packet & 0x80 != 0 {
            let end = cursor
                .checked_add(bytes_per_sample)
                .ok_or(FormatError::Invalid("TGA RLE pixel data is truncated"))?;
            let sample = bytes
                .get(cursor..end)
                .ok_or(FormatError::Invalid("TGA RLE pixel data is truncated"))?;
            for _ in 0..count {
                output.extend_from_slice(sample);
            }
            cursor = end;
        } else {
            let length = count
                .checked_mul(bytes_per_sample)
                .ok_or(FormatError::Invalid("TGA RLE pixel data length overflows"))?;
            let end = cursor
                .checked_add(length)
                .ok_or(FormatError::Invalid("TGA RLE pixel data length overflows"))?;
            output.extend_from_slice(
                bytes
                    .get(cursor..end)
                    .ok_or(FormatError::Invalid("TGA RLE pixel data is truncated"))?,
            );
            cursor = end;
        }
        decoded += count;
    }
    Ok((output, cursor))
}

fn decode_index(
    sample: &[u8],
    depth: u8,
    color_map: Option<&TgaColorMap>,
) -> Result<[u8; 4], FormatError> {
    let map = color_map.ok_or(FormatError::Invalid(
        "TGA color-mapped image has no color map",
    ))?;
    let index = if depth == 8 {
        u16::from(sample[0])
    } else {
        u16::from_le_bytes([sample[0], sample[1]])
    };
    let relative = index
        .checked_sub(map.first_index)
        .ok_or(FormatError::Invalid(
            "TGA color-map index is below its origin",
        ))?;
    map.entries
        .get(usize::from(relative))
        .copied()
        .ok_or(FormatError::Invalid(
            "TGA color-map index is outside its range",
        ))
}

fn decode_color(
    encoded: &[u8],
    depth: u8,
    alpha_bits: u8,
    alpha_type: Option<TgaAlphaType>,
) -> Result<[u8; 4], FormatError> {
    let meaningful = alpha_type.map_or(alpha_bits != 0, TgaAlphaType::retains_attribute);
    let mut rgba = match depth {
        15 | 16 => {
            let value = u16::from_le_bytes([encoded[0], encoded[1]]);
            [
                expand_5(((value >> 10) & 0x1f) as u8),
                expand_5(((value >> 5) & 0x1f) as u8),
                expand_5((value & 0x1f) as u8),
                if depth == 16 && meaningful {
                    if value & 0x8000 == 0 { 0 } else { u8::MAX }
                } else {
                    u8::MAX
                },
            ]
        }
        24 => [encoded[2], encoded[1], encoded[0], u8::MAX],
        32 => [
            encoded[2],
            encoded[1],
            encoded[0],
            if meaningful { encoded[3] } else { u8::MAX },
        ],
        _ => return Err(FormatError::Unsupported("TGA color depth is unsupported")),
    };
    if alpha_type == Some(TgaAlphaType::Premultiplied) {
        rgba = unpremultiply(rgba);
    }
    Ok(rgba)
}

const fn expand_5(value: u8) -> u8 {
    (value << 3) | (value >> 2)
}

fn unpremultiply(mut rgba: [u8; 4]) -> [u8; 4] {
    let alpha = u32::from(rgba[3]);
    if alpha == 0 {
        rgba[..3].fill(0);
    } else {
        for channel in &mut rgba[..3] {
            let numerator = u32::from(*channel) * 255 + alpha / 2;
            *channel = numerator.checked_div(alpha).unwrap_or(0).min(255) as u8;
        }
    }
    rgba
}

fn decode_extension_blocks(
    bytes: &[u8],
    extension: &mut ParsedExtension,
    header: Header,
    origin: TgaOrigin,
    color_map: Option<&TgaColorMap>,
    image_start: usize,
    image_end: usize,
) -> Result<(), FormatError> {
    if extension.color_correction_offset != 0 {
        let end = extension
            .color_correction_offset
            .checked_add(COLOR_CORRECTION_BYTES)
            .ok_or(FormatError::Invalid(
                "TGA color-correction table length overflows",
            ))?;
        let source =
            bytes
                .get(extension.color_correction_offset..end)
                .ok_or(FormatError::Invalid(
                    "TGA color-correction table is truncated",
                ))?;
        let mut table = Vec::with_capacity(COLOR_CORRECTION_ENTRIES);
        for entry in source.chunks_exact(8) {
            table.push([
                u16::from_le_bytes([entry[2], entry[3]]),
                u16::from_le_bytes([entry[4], entry[5]]),
                u16::from_le_bytes([entry[6], entry[7]]),
                u16::from_le_bytes([entry[0], entry[1]]),
            ]);
        }
        extension.value.color_correction_table = Some(table);
    }
    if extension.postage_offset != 0 {
        let width = u32::from(
            *bytes
                .get(extension.postage_offset)
                .ok_or(FormatError::Invalid("TGA postage stamp is truncated"))?,
        );
        let height_offset = extension
            .postage_offset
            .checked_add(1)
            .ok_or(FormatError::Invalid("TGA postage stamp offset overflows"))?;
        let height = u32::from(
            *bytes
                .get(height_offset)
                .ok_or(FormatError::Invalid("TGA postage stamp is truncated"))?,
        );
        if width == 0 || height == 0 {
            return Err(FormatError::Invalid(
                "TGA postage stamp dimensions are zero",
            ));
        }
        let mut postage_header = header;
        postage_header.width = width;
        postage_header.height = height;
        postage_header.image_type = match header.image_type {
            9 => 1,
            10 => 2,
            11 => 3,
            value => value,
        };
        let data_offset = extension
            .postage_offset
            .checked_add(2)
            .ok_or(FormatError::Invalid("TGA postage stamp offset overflows"))?;
        let (postage, _) = decode_image(
            bytes,
            data_offset,
            postage_header,
            origin,
            color_map,
            Some(extension.value.alpha_type),
        )?;
        extension.value.postage_stamp = postage;
    }
    if extension.scan_line_offset != 0 {
        let count = header.height as usize;
        let length = count
            .checked_mul(4)
            .ok_or(FormatError::Invalid("TGA scan-line table length overflows"))?;
        let end = extension
            .scan_line_offset
            .checked_add(length)
            .ok_or(FormatError::Invalid("TGA scan-line table length overflows"))?;
        let table = bytes
            .get(extension.scan_line_offset..end)
            .ok_or(FormatError::Invalid("TGA scan-line table is truncated"))?;
        for entry in table.chunks_exact(4) {
            let offset = u32::from_le_bytes(entry.try_into().expect("four-byte chunk")) as usize;
            if offset < image_start || offset >= image_end {
                return Err(FormatError::Invalid(
                    "TGA scan-line offset is outside image data",
                ));
            }
        }
        extension.value.scan_line_table = true;
    }
    Ok(())
}

fn decode_developer_fields(
    bytes: &[u8],
    directory_offset: usize,
) -> Result<Vec<TgaDeveloperField>, FormatError> {
    let count = usize::from(read_u16(bytes, directory_offset)?);
    let directory_length = count
        .checked_mul(10)
        .and_then(|value| value.checked_add(2))
        .ok_or(FormatError::Invalid(
            "TGA developer directory length overflows",
        ))?;
    let directory_end =
        directory_offset
            .checked_add(directory_length)
            .ok_or(FormatError::Invalid(
                "TGA developer directory length overflows",
            ))?;
    bytes
        .get(directory_offset..directory_end)
        .ok_or(FormatError::Invalid("TGA developer directory is truncated"))?;
    let mut fields = Vec::new();
    fields
        .try_reserve_exact(count)
        .map_err(|_| FormatError::Invalid("TGA developer directory allocation failed"))?;
    let mut total = 0_usize;
    for index in 0..count {
        let entry = directory_offset + 2 + index * 10;
        let tag = read_u16(bytes, entry)?;
        let offset = read_u32(bytes, entry + 2)? as usize;
        let size = read_u32(bytes, entry + 6)? as usize;
        total = total
            .checked_add(size)
            .filter(|value| *value <= MAX_COMMON_RASTER_BYTES)
            .ok_or(FormatError::Invalid(
                "TGA developer field byte total exceeds its bound",
            ))?;
        let end = offset
            .checked_add(size)
            .ok_or(FormatError::Invalid("TGA developer field length overflows"))?;
        let data = bytes
            .get(offset..end)
            .ok_or(FormatError::Invalid("TGA developer field is truncated"))?
            .to_vec();
        fields.push(TgaDeveloperField { tag, data });
    }
    Ok(fields)
}

fn image_format_from_header(header: Header) -> Result<TgaImageFormat, FormatError> {
    match header.image_type {
        0 => Ok(TgaImageFormat::None),
        1 | 9 => Ok(TgaImageFormat::ColorMapped {
            index_depth: header.depth,
            entry_depth: header.color_map_depth,
            first_index: header.color_map_first,
        }),
        2 | 10 => Ok(TgaImageFormat::TrueColor {
            depth: header.depth,
        }),
        3 | 11 => Ok(TgaImageFormat::Grayscale {
            depth: header.depth,
        }),
        _ => Err(FormatError::Unsupported("TGA image type is unsupported")),
    }
}

pub(super) fn apply_color_correction(
    pixels: &mut [u8],
    table: &[[u16; 4]],
) -> Result<(), FormatError> {
    if table.len() != COLOR_CORRECTION_ENTRIES {
        return Err(FormatError::Invalid(
            "TGA color-correction table must contain 256 entries",
        ));
    }
    for pixel in pixels.chunks_exact_mut(4) {
        let source = [pixel[0], pixel[1], pixel[2], pixel[3]];
        pixel[0] = downconvert_u16(table[usize::from(source[0])][0]);
        pixel[1] = downconvert_u16(table[usize::from(source[1])][1]);
        pixel[2] = downconvert_u16(table[usize::from(source[2])][2]);
        pixel[3] = downconvert_u16(table[usize::from(source[3])][3]);
    }
    Ok(())
}

fn downconvert_u16(value: u16) -> u8 {
    ((u32::from(value) + 128) / 257) as u8
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, FormatError> {
    let end = offset
        .checked_add(2)
        .ok_or(FormatError::Invalid("TGA 16-bit field offset overflows"))?;
    bytes
        .get(offset..end)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(FormatError::Invalid("TGA 16-bit field is truncated"))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, FormatError> {
    let end = offset
        .checked_add(4)
        .ok_or(FormatError::Invalid("TGA 32-bit field offset overflows"))?;
    bytes
        .get(offset..end)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(FormatError::Invalid("TGA 32-bit field is truncated"))
}

fn read_u16_values(bytes: &[u8], offset: usize, count: usize) -> Result<Vec<u16>, FormatError> {
    (0..count)
        .map(|index| read_u16(bytes, offset + index * 2))
        .collect()
}
