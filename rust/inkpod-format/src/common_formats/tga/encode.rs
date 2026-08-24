use super::model::*;
use crate::{CommonRaster, FormatError, MAX_COMMON_RASTER_BYTES};
use std::collections::BTreeMap;

const HEADER_BYTES: usize = 18;
const EXTENSION_BYTES: usize = 495;
const FOOTER_SIGNATURE: &[u8; 18] = b"TRUEVISION-XFILE.\0";

struct EncodedImage {
    bytes: Vec<u8>,
    scan_line_relative_offsets: Vec<usize>,
    color_map: Option<TgaColorMap>,
}

pub(super) fn encode_document(document: &TgaDocument) -> Result<Vec<u8>, FormatError> {
    validate_document(document)?;
    let options = &document.options;
    let image = encode_image(document.raster.as_ref(), options)?;
    let color_map = image.color_map.as_ref().or(options.color_map.as_ref());
    let image_id_len = u8::try_from(options.metadata.image_id.len())
        .map_err(|_| FormatError::Invalid("TGA Image ID exceeds 255 bytes"))?;
    let (image_type, depth, width, height) = header_image_fields(document)?;
    let width = u16::try_from(width)
        .map_err(|_| FormatError::Unsupported("TGA dimensions exceed 16-bit fields"))?;
    let height = u16::try_from(height)
        .map_err(|_| FormatError::Unsupported("TGA dimensions exceed 16-bit fields"))?;
    let mut output = vec![0_u8; HEADER_BYTES];
    output[0] = image_id_len;
    output[1] = u8::from(color_map.is_some());
    output[2] = image_type;
    if let Some(map) = color_map {
        output[3..5].copy_from_slice(&map.first_index.to_le_bytes());
        output[5..7].copy_from_slice(
            &u16::try_from(map.entries.len())
                .map_err(|_| FormatError::Invalid("TGA color map has too many entries"))?
                .to_le_bytes(),
        );
        output[7] = map.entry_depth;
    }
    output[8..10].copy_from_slice(&options.metadata.x_origin.to_le_bytes());
    output[10..12].copy_from_slice(&options.metadata.y_origin.to_le_bytes());
    output[12..14].copy_from_slice(&width.to_le_bytes());
    output[14..16].copy_from_slice(&height.to_le_bytes());
    output[16] = depth;
    output[17] = options.origin.descriptor_bits() | attribute_bits(options);
    output.extend_from_slice(&options.metadata.image_id);
    if let Some(map) = color_map {
        encode_color_map(&mut output, map, effective_alpha_type(options), options)?;
    }
    let image_data_start = output.len();
    output.extend_from_slice(&image.bytes);

    let needs_footer = options.metadata.write_footer
        || options.metadata.extension.is_some()
        || !options.metadata.developer_fields.is_empty();
    if !needs_footer {
        ensure_output_bound(&output)?;
        return Ok(output);
    }

    let mut extension_offset = 0_u32;
    let mut developer_offset = 0_u32;
    let mut color_correction_offset = 0_u32;
    let mut postage_offset = 0_u32;
    let mut scan_line_offset = 0_u32;
    if let Some(extension) = &options.metadata.extension {
        validate_extension(extension)?;
        if let Some(table) = &extension.color_correction_table {
            color_correction_offset = current_u32_offset(&output)?;
            encode_color_correction(&mut output, table)?;
        }
        if let Some(postage) = &extension.postage_stamp {
            postage_offset = current_u32_offset(&output)?;
            encode_postage_stamp(&mut output, postage, options, color_map)?;
        }
        if extension.scan_line_table {
            scan_line_offset = current_u32_offset(&output)?;
            if image.scan_line_relative_offsets.len() != usize::from(height) {
                return Err(FormatError::Invalid(
                    "TGA scan-line table does not match image height",
                ));
            }
            for relative in &image.scan_line_relative_offsets {
                let absolute = image_data_start
                    .checked_add(*relative)
                    .ok_or(FormatError::Invalid("TGA scan-line offset overflows"))?;
                output.extend_from_slice(
                    &u32::try_from(absolute)
                        .map_err(|_| FormatError::Invalid("TGA scan-line offset overflows"))?
                        .to_le_bytes(),
                );
            }
        }
    }
    if !options.metadata.developer_fields.is_empty() {
        let count = u16::try_from(options.metadata.developer_fields.len())
            .map_err(|_| FormatError::Invalid("TGA has too many developer fields"))?;
        let mut descriptors = Vec::with_capacity(options.metadata.developer_fields.len());
        for field in &options.metadata.developer_fields {
            let offset = current_u32_offset(&output)?;
            let size = u32::try_from(field.data.len())
                .map_err(|_| FormatError::Invalid("TGA developer field is too large"))?;
            output.extend_from_slice(&field.data);
            descriptors.push((field.tag, offset, size));
        }
        developer_offset = current_u32_offset(&output)?;
        output.extend_from_slice(&count.to_le_bytes());
        for (tag, offset, size) in descriptors {
            output.extend_from_slice(&tag.to_le_bytes());
            output.extend_from_slice(&offset.to_le_bytes());
            output.extend_from_slice(&size.to_le_bytes());
        }
    }
    if let Some(extension) = &options.metadata.extension {
        extension_offset = current_u32_offset(&output)?;
        encode_extension(
            &mut output,
            extension,
            color_correction_offset,
            postage_offset,
            scan_line_offset,
        )?;
    }
    output.extend_from_slice(&extension_offset.to_le_bytes());
    output.extend_from_slice(&developer_offset.to_le_bytes());
    output.extend_from_slice(FOOTER_SIGNATURE);
    ensure_output_bound(&output)?;
    Ok(output)
}

fn header_image_fields(document: &TgaDocument) -> Result<(u8, u8, u32, u32), FormatError> {
    let (base_type, depth) = match document.options.image_format {
        TgaImageFormat::None => (0, 0),
        TgaImageFormat::ColorMapped {
            index_depth,
            entry_depth,
            ..
        } => {
            if !matches!(index_depth, 8 | 16) || !matches!(entry_depth, 15 | 16 | 24 | 32) {
                return Err(FormatError::Unsupported(
                    "TGA color-mapped storage depths are unsupported",
                ));
            }
            (1, index_depth)
        }
        TgaImageFormat::TrueColor { depth } => {
            if !matches!(depth, 16 | 24 | 32) {
                return Err(FormatError::Unsupported(
                    "TGA true-color depth is unsupported",
                ));
            }
            (2, depth)
        }
        TgaImageFormat::Grayscale { depth } => {
            if depth != 8 {
                return Err(FormatError::Unsupported(
                    "TGA black-and-white depth is unsupported",
                ));
            }
            (3, depth)
        }
    };
    let image_type = if document.options.compression == TgaCompression::RunLengthEncoded {
        base_type + 8
    } else {
        base_type
    };
    let (width, height) = document
        .raster
        .as_ref()
        .map_or((0, 0), |raster| (raster.info.width, raster.info.height));
    Ok((image_type, depth, width, height))
}

fn attribute_bits(options: &TgaEncodeOptions) -> u8 {
    let meaningful = effective_alpha_type(options).retains_attribute();
    match options.image_format {
        TgaImageFormat::TrueColor { depth: 32 } if meaningful => 8,
        TgaImageFormat::TrueColor { depth: 16 } if meaningful => 1,
        _ => 0,
    }
}

fn effective_alpha_type(options: &TgaEncodeOptions) -> TgaAlphaType {
    options
        .metadata
        .extension
        .as_ref()
        .map_or(TgaAlphaType::Straight, |extension| extension.alpha_type)
}

fn encode_image(
    raster: Option<&CommonRaster>,
    options: &TgaEncodeOptions,
) -> Result<EncodedImage, FormatError> {
    let Some(raster) = raster else {
        return Ok(EncodedImage {
            bytes: Vec::new(),
            scan_line_relative_offsets: Vec::new(),
            color_map: options.color_map.clone(),
        });
    };
    let mut color_map = options.color_map.clone();
    let samples = match options.image_format {
        TgaImageFormat::None => unreachable!("validated no-image document"),
        TgaImageFormat::TrueColor { depth } => encode_true_color_samples(raster, depth, options)?,
        TgaImageFormat::Grayscale { depth: 8 } => encode_grayscale_samples(raster, options)?,
        TgaImageFormat::Grayscale { .. } => {
            return Err(FormatError::Unsupported(
                "TGA black-and-white depth is unsupported",
            ));
        }
        TgaImageFormat::ColorMapped {
            index_depth,
            entry_depth,
            first_index,
        } => {
            let (encoded, actual_map) = encode_index_samples(
                raster,
                index_depth,
                entry_depth,
                first_index,
                color_map.as_ref(),
                options,
            )?;
            color_map = Some(actual_map);
            encoded
        }
    };
    let bytes_per_sample = match options.image_format {
        TgaImageFormat::TrueColor { depth } => usize::from(depth / 8),
        TgaImageFormat::ColorMapped { index_depth, .. } => usize::from(index_depth / 8),
        TgaImageFormat::Grayscale { .. } => 1,
        TgaImageFormat::None => 0,
    };
    let ordered = reorder_samples_for_origin(
        &samples,
        raster.info.width as usize,
        raster.info.height as usize,
        bytes_per_sample,
        options.origin,
    );
    let (bytes, scan_line_relative_offsets) = match options.compression {
        TgaCompression::Uncompressed => {
            let stride = raster.info.width as usize * bytes_per_sample;
            let offsets = (0..raster.info.height as usize)
                .map(|row| row * stride)
                .collect();
            (ordered, offsets)
        }
        TgaCompression::RunLengthEncoded => encode_rle_rows(
            &ordered,
            raster.info.width as usize,
            raster.info.height as usize,
            bytes_per_sample,
        )?,
    };
    Ok(EncodedImage {
        bytes,
        scan_line_relative_offsets,
        color_map,
    })
}

fn encode_true_color_samples(
    raster: &CommonRaster,
    depth: u8,
    options: &TgaEncodeOptions,
) -> Result<Vec<u8>, FormatError> {
    let alpha_type = effective_alpha_type(options);
    let bytes_per_sample = usize::from(depth / 8);
    let mut output = Vec::with_capacity(raster.pixels.len() / 4 * bytes_per_sample);
    for pixel in raster.pixels.chunks_exact(4) {
        let rgba = [pixel[0], pixel[1], pixel[2], pixel[3]];
        match depth {
            16 => {
                require_alpha(rgba[3], options, alpha_type.retains_attribute())?;
                require_one_bit_alpha(rgba[3], options, alpha_type.retains_attribute())?;
                require_5_bit_precision(&rgba, options, "TGA 16-bit output")?;
                let stored = stored_rgba(rgba, alpha_type);
                let value = pack_555(stored, alpha_type.retains_attribute());
                output.extend_from_slice(&value.to_le_bytes());
            }
            24 => {
                require_alpha(rgba[3], options, false)?;
                output.extend_from_slice(&[rgba[2], rgba[1], rgba[0]]);
            }
            32 => {
                require_alpha(rgba[3], options, alpha_type.retains_attribute())?;
                let stored = stored_rgba(rgba, alpha_type);
                output.extend_from_slice(&[stored[2], stored[1], stored[0], stored[3]]);
            }
            _ => {
                return Err(FormatError::Unsupported(
                    "TGA true-color depth is unsupported",
                ));
            }
        }
    }
    Ok(output)
}

fn encode_grayscale_samples(
    raster: &CommonRaster,
    options: &TgaEncodeOptions,
) -> Result<Vec<u8>, FormatError> {
    let mut output = Vec::with_capacity(raster.pixels.len() / 4);
    for pixel in raster.pixels.chunks_exact(4) {
        require_alpha(pixel[3], options, false)?;
        let gray = if pixel[0] == pixel[1] && pixel[1] == pixel[2] {
            pixel[0]
        } else if options.grayscale_conversion == TgaGrayscaleConversion::Bt709 {
            ((u32::from(pixel[0]) * 54
                + u32::from(pixel[1]) * 183
                + u32::from(pixel[2]) * 19
                + 128)
                >> 8) as u8
        } else {
            return Err(FormatError::Unsupported(
                "TGA grayscale output requires explicit color conversion",
            ));
        };
        output.push(gray);
    }
    Ok(output)
}

fn encode_index_samples(
    raster: &CommonRaster,
    index_depth: u8,
    entry_depth: u8,
    first_index: u16,
    supplied_map: Option<&TgaColorMap>,
    options: &TgaEncodeOptions,
) -> Result<(Vec<u8>, TgaColorMap), FormatError> {
    if !matches!(index_depth, 8 | 16) {
        return Err(FormatError::Unsupported(
            "TGA color-map index depth is unsupported",
        ));
    }
    let map = if let Some(map) = supplied_map {
        if map.first_index != first_index || map.entry_depth != entry_depth {
            return Err(FormatError::Invalid(
                "TGA supplied color map does not match storage options",
            ));
        }
        validate_color_map(map)?;
        map.clone()
    } else {
        let mut seen = BTreeMap::<[u8; 4], usize>::new();
        let mut entries = Vec::new();
        for pixel in raster.pixels.chunks_exact(4) {
            let rgba = [pixel[0], pixel[1], pixel[2], pixel[3]];
            if let std::collections::btree_map::Entry::Vacant(entry) = seen.entry(rgba) {
                entry.insert(entries.len());
                entries.push(rgba);
            }
        }
        TgaColorMap {
            first_index,
            entry_depth,
            entries,
        }
    };
    validate_palette_representation(&map, options)?;
    let maximum = if index_depth == 8 {
        usize::from(u8::MAX)
    } else {
        usize::from(u16::MAX)
    };
    let last = usize::from(map.first_index)
        .checked_add(map.entries.len() - 1)
        .ok_or(FormatError::Invalid("TGA color-map index range overflows"))?;
    if last > maximum {
        return Err(FormatError::Unsupported(
            "TGA color map does not fit the selected index depth",
        ));
    }
    let indices = map
        .entries
        .iter()
        .enumerate()
        .map(|(offset, color)| (*color, offset))
        .collect::<BTreeMap<_, _>>();
    let mut output = Vec::with_capacity(raster.pixels.len() / 4 * usize::from(index_depth / 8));
    for pixel in raster.pixels.chunks_exact(4) {
        let rgba = [pixel[0], pixel[1], pixel[2], pixel[3]];
        let relative = indices.get(&rgba).ok_or(FormatError::Unsupported(
            "TGA supplied color map does not contain an input color",
        ))?;
        let index = usize::from(map.first_index)
            .checked_add(*relative)
            .ok_or(FormatError::Invalid("TGA color-map index overflows"))?;
        if index_depth == 8 {
            output.push(index as u8);
        } else {
            output.extend_from_slice(&(index as u16).to_le_bytes());
        }
    }
    Ok((output, map))
}

fn validate_palette_representation(
    map: &TgaColorMap,
    options: &TgaEncodeOptions,
) -> Result<(), FormatError> {
    let alpha_type = effective_alpha_type(options);
    for entry in &map.entries {
        match map.entry_depth {
            15 => {
                require_alpha(entry[3], options, false)?;
                require_5_bit_precision(entry, options, "TGA 15-bit palette")?;
            }
            16 => {
                require_alpha(entry[3], options, alpha_type.retains_attribute())?;
                require_one_bit_alpha(entry[3], options, alpha_type.retains_attribute())?;
                require_5_bit_precision(entry, options, "TGA 16-bit palette")?;
            }
            24 => require_alpha(entry[3], options, false)?,
            32 => require_alpha(entry[3], options, alpha_type.retains_attribute())?,
            _ => {
                return Err(FormatError::Unsupported(
                    "TGA color-map entry depth is unsupported",
                ));
            }
        }
    }
    Ok(())
}

fn require_5_bit_precision(
    rgba: &[u8; 4],
    options: &TgaEncodeOptions,
    message: &'static str,
) -> Result<(), FormatError> {
    if !options.allow_color_precision_loss
        && rgba[..3].iter().copied().any(|value| !is_exact_5(value))
    {
        Err(FormatError::Unsupported(message))
    } else {
        Ok(())
    }
}

fn require_alpha(
    alpha: u8,
    options: &TgaEncodeOptions,
    represented: bool,
) -> Result<(), FormatError> {
    if alpha != u8::MAX && !represented && options.alpha_loss == TgaAlphaLoss::Reject {
        Err(FormatError::Unsupported(
            "TGA output would discard alpha without explicit permission",
        ))
    } else {
        Ok(())
    }
}

fn require_one_bit_alpha(
    alpha: u8,
    options: &TgaEncodeOptions,
    represented: bool,
) -> Result<(), FormatError> {
    if represented && !matches!(alpha, 0 | u8::MAX) && !options.allow_alpha_precision_loss {
        Err(FormatError::Unsupported(
            "TGA one-bit alpha output would lose precision",
        ))
    } else {
        Ok(())
    }
}

fn stored_rgba(mut rgba: [u8; 4], alpha_type: TgaAlphaType) -> [u8; 4] {
    if alpha_type == TgaAlphaType::Premultiplied {
        let alpha = u32::from(rgba[3]);
        for channel in &mut rgba[..3] {
            *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
        }
    } else if !alpha_type.retains_attribute() {
        rgba[3] = u8::MAX;
    }
    rgba
}

fn pack_555(rgba: [u8; 4], alpha: bool) -> u16 {
    u16::from(rgba[2] >> 3)
        | (u16::from(rgba[1] >> 3) << 5)
        | (u16::from(rgba[0] >> 3) << 10)
        | if alpha && rgba[3] >= 128 { 0x8000 } else { 0 }
}

fn is_exact_5(value: u8) -> bool {
    expand_5(value >> 3) == value
}

const fn expand_5(value: u8) -> u8 {
    (value << 3) | (value >> 2)
}

fn reorder_samples_for_origin(
    samples: &[u8],
    width: usize,
    height: usize,
    bytes_per_sample: usize,
    origin: TgaOrigin,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(samples.len());
    for source_y in 0..height {
        let y = if origin.top() {
            source_y
        } else {
            height - 1 - source_y
        };
        for source_x in 0..width {
            let x = if origin.right() {
                width - 1 - source_x
            } else {
                source_x
            };
            let offset = (y * width + x) * bytes_per_sample;
            output.extend_from_slice(&samples[offset..offset + bytes_per_sample]);
        }
    }
    output
}

fn encode_rle_rows(
    samples: &[u8],
    width: usize,
    height: usize,
    bytes_per_sample: usize,
) -> Result<(Vec<u8>, Vec<usize>), FormatError> {
    let stride = width
        .checked_mul(bytes_per_sample)
        .ok_or(FormatError::Invalid("TGA RLE row length overflows"))?;
    let mut output = Vec::new();
    let mut offsets = Vec::with_capacity(height);
    for row in samples.chunks_exact(stride) {
        offsets.push(output.len());
        let mut pixel = 0_usize;
        while pixel < width {
            let run = repeated_run(row, pixel, width, bytes_per_sample);
            if run >= 2 {
                let count = run.min(128);
                output.push(0x80 | (count as u8 - 1));
                let start = pixel * bytes_per_sample;
                output.extend_from_slice(&row[start..start + bytes_per_sample]);
                pixel += count;
                continue;
            }
            let raw_start = pixel;
            pixel += 1;
            while pixel < width && pixel - raw_start < 128 {
                if repeated_run(row, pixel, width, bytes_per_sample) >= 2 {
                    break;
                }
                pixel += 1;
            }
            let count = pixel - raw_start;
            output.push(count as u8 - 1);
            output.extend_from_slice(&row[raw_start * bytes_per_sample..pixel * bytes_per_sample]);
        }
    }
    Ok((output, offsets))
}

fn repeated_run(row: &[u8], start: usize, width: usize, bytes_per_sample: usize) -> usize {
    let first = &row[start * bytes_per_sample..(start + 1) * bytes_per_sample];
    let mut count = 1_usize;
    while start + count < width && count < 128 {
        let next = &row[(start + count) * bytes_per_sample..(start + count + 1) * bytes_per_sample];
        if next != first {
            break;
        }
        count += 1;
    }
    count
}

fn encode_color_map(
    output: &mut Vec<u8>,
    map: &TgaColorMap,
    alpha_type: TgaAlphaType,
    options: &TgaEncodeOptions,
) -> Result<(), FormatError> {
    validate_palette_representation(map, options)?;
    for entry in &map.entries {
        let stored = stored_rgba(*entry, alpha_type);
        match map.entry_depth {
            15 => output.extend_from_slice(&pack_555(stored, false).to_le_bytes()),
            16 => output
                .extend_from_slice(&pack_555(stored, alpha_type.retains_attribute()).to_le_bytes()),
            24 => output.extend_from_slice(&[stored[2], stored[1], stored[0]]),
            32 => output.extend_from_slice(&[stored[2], stored[1], stored[0], stored[3]]),
            _ => unreachable!("validated TGA color-map depth"),
        }
    }
    Ok(())
}

fn encode_postage_stamp(
    output: &mut Vec<u8>,
    postage: &CommonRaster,
    options: &TgaEncodeOptions,
    color_map: Option<&TgaColorMap>,
) -> Result<(), FormatError> {
    let mut postage_options = options.clone();
    postage_options.compression = TgaCompression::Uncompressed;
    postage_options.color_map = color_map.cloned();
    postage_options.metadata = TgaMetadata::default();
    let encoded = encode_image(Some(postage), &postage_options)?;
    output.push(postage.info.width as u8);
    output.push(postage.info.height as u8);
    output.extend_from_slice(&encoded.bytes);
    Ok(())
}

fn encode_color_correction(output: &mut Vec<u8>, table: &[[u16; 4]]) -> Result<(), FormatError> {
    if table.len() != 256 {
        return Err(FormatError::Invalid(
            "TGA color-correction table must contain 256 entries",
        ));
    }
    for [red, green, blue, alpha] in table {
        output.extend_from_slice(&alpha.to_le_bytes());
        output.extend_from_slice(&red.to_le_bytes());
        output.extend_from_slice(&green.to_le_bytes());
        output.extend_from_slice(&blue.to_le_bytes());
    }
    Ok(())
}

fn encode_extension(
    output: &mut Vec<u8>,
    extension: &TgaExtension,
    color_correction_offset: u32,
    postage_offset: u32,
    scan_line_offset: u32,
) -> Result<(), FormatError> {
    let size = u16::try_from(EXTENSION_BYTES + extension.extra.len())
        .map_err(|_| FormatError::Invalid("TGA extension area is too large"))?;
    let start = output.len();
    output.resize(start + EXTENSION_BYTES, 0);
    output[start..start + 2].copy_from_slice(&size.to_le_bytes());
    write_fixed_text(&mut output[start + 2..start + 43], &extension.author_name);
    for (index, comment) in extension.author_comments.iter().enumerate() {
        let offset = start + 43 + index * 81;
        write_fixed_text(&mut output[offset..offset + 81], comment);
    }
    if let Some(timestamp) = extension.timestamp {
        write_u16_values(
            &mut output[start + 367..start + 379],
            &[
                timestamp.month,
                timestamp.day,
                timestamp.year,
                timestamp.hour,
                timestamp.minute,
                timestamp.second,
            ],
        );
    }
    write_fixed_text(&mut output[start + 379..start + 420], &extension.job_name);
    if let Some(duration) = extension.job_duration {
        write_u16_values(
            &mut output[start + 420..start + 426],
            &[duration.hours, duration.minutes, duration.seconds],
        );
    }
    write_fixed_text(
        &mut output[start + 426..start + 467],
        &extension.software_id,
    );
    output[start + 467..start + 469].copy_from_slice(&extension.software_version.to_le_bytes());
    output[start + 469] = extension.software_version_letter.unwrap_or(0);
    output[start + 470..start + 474].copy_from_slice(&[
        extension.key_color[3],
        extension.key_color[0],
        extension.key_color[1],
        extension.key_color[2],
    ]);
    write_ratio(
        &mut output[start + 474..start + 478],
        extension.pixel_aspect_ratio,
    );
    write_ratio(&mut output[start + 478..start + 482], extension.gamma);
    output[start + 482..start + 486].copy_from_slice(&color_correction_offset.to_le_bytes());
    output[start + 486..start + 490].copy_from_slice(&postage_offset.to_le_bytes());
    output[start + 490..start + 494].copy_from_slice(&scan_line_offset.to_le_bytes());
    output[start + 494] = extension.alpha_type.code();
    output.extend_from_slice(&extension.extra);
    Ok(())
}

fn write_fixed_text(destination: &mut [u8], value: &str) {
    let count = value.len().min(destination.len().saturating_sub(1));
    destination[..count].copy_from_slice(&value.as_bytes()[..count]);
}

fn write_u16_values(destination: &mut [u8], values: &[u16]) {
    for (chunk, value) in destination.chunks_exact_mut(2).zip(values.iter().copied()) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
}

fn write_ratio(destination: &mut [u8], ratio: Option<TgaRatio>) {
    if let Some(ratio) = ratio {
        destination[..2].copy_from_slice(&ratio.numerator.to_le_bytes());
        destination[2..].copy_from_slice(&ratio.denominator.to_le_bytes());
    }
}

fn current_u32_offset(output: &[u8]) -> Result<u32, FormatError> {
    u32::try_from(output.len()).map_err(|_| FormatError::Invalid("TGA file offset overflows"))
}

fn ensure_output_bound(output: &[u8]) -> Result<(), FormatError> {
    if output.len() > MAX_COMMON_RASTER_BYTES {
        Err(FormatError::Invalid("TGA output exceeds byte limit"))
    } else {
        Ok(())
    }
}
