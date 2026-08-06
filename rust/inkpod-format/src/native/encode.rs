use super::model::*;
use super::validate::{validate_document, validate_document_metadata};
use crate::adjustment::encode_adjustment_metadata;
use crate::light_table::encode_light_table_metadata;
use crate::vector::encode_vector_metadata;
use inkpod_image::{PixelFormat, PixelValue};
pub fn encode_document_archive(document: &DocumentArchive) -> Result<Vec<u8>, FormatError> {
    validate_document(document)?;
    let document_metadata = document
        .document_metadata
        .as_ref()
        .map(encode_document_metadata)
        .transpose()?;
    let light_table_metadata = document
        .light_table_metadata
        .as_ref()
        .map(encode_light_table_metadata)
        .transpose()?;
    let vector_metadata = document
        .vector_metadata
        .as_ref()
        .map(encode_vector_metadata)
        .transpose()?;
    let adjustment_metadata = document
        .adjustment_metadata
        .as_ref()
        .map(encode_adjustment_metadata)
        .transpose()?;
    let blob_count = document.planes.iter().try_fold(0_usize, |count, plane| {
        count
            .checked_add(plane.tiles.len())
            .ok_or(FormatError::Invalid("blob count overflows"))
    })?;
    if blob_count > MAX_BLOBS {
        return Err(FormatError::Invalid("too many blobs"));
    }
    let color_metadata_len = COLOR_METADATA_FIXED_BYTES
        .checked_add(
            document
                .palette
                .len()
                .checked_mul(COLOR_VALUE_BYTES)
                .ok_or(FormatError::Invalid("palette manifest overflows"))?,
        )
        .ok_or(FormatError::Invalid("color metadata length overflows"))?;
    let manifest_len = FIXED_MANIFEST_BYTES
        .checked_add(color_metadata_len)
        .and_then(|value| {
            value.checked_add(
                document_metadata
                    .as_ref()
                    .map_or(0, |bytes| bytes.len().saturating_add(8)),
            )
        })
        .and_then(|value| {
            value.checked_add(
                light_table_metadata
                    .as_ref()
                    .map_or(0, |bytes| bytes.len().saturating_add(8)),
            )
        })
        .and_then(|value| {
            value.checked_add(
                vector_metadata
                    .as_ref()
                    .map_or(0, |bytes| bytes.len().saturating_add(8)),
            )
        })
        .and_then(|value| {
            value.checked_add(
                adjustment_metadata
                    .as_ref()
                    .map_or(0, |bytes| bytes.len().saturating_add(8)),
            )
        })
        .and_then(|value| {
            value.checked_add(document.planes.len().checked_mul(PLANE_DESCRIPTOR_BYTES)?)
        })
        .and_then(|value| value.checked_add(blob_count.checked_mul(BLOB_DESCRIPTOR_BYTES)?))
        .ok_or(FormatError::Invalid("manifest length overflows"))?;

    let mut descriptors = Vec::with_capacity(blob_count);
    let mut blobs = Vec::new();
    for (plane_index, plane) in document.planes.iter().enumerate() {
        for tile in &plane.tiles {
            let offset = u64::try_from(blobs.len())
                .map_err(|_| FormatError::Invalid("blob offset is not representable"))?;
            let length = u64::try_from(tile.bytes.len())
                .map_err(|_| FormatError::Invalid("blob length is not representable"))?;
            if offset
                .checked_add(length)
                .is_none_or(|end| end > MAX_FILE_BYTES)
            {
                return Err(FormatError::Invalid("blob area exceeds the bounded size"));
            }
            descriptors.push(BlobDescriptor {
                plane_index: plane_index as u32,
                tile_x: tile.coord.x,
                tile_y: tile.coord.y,
                width: tile.width,
                height: tile.height,
                pixel_format: plane.pixel_format,
                offset,
                length,
                checksum: checksum(&tile.bytes),
            });
            blobs.extend_from_slice(&tile.bytes);
        }
    }

    let total_len = HEADER_BYTES
        .checked_add(manifest_len)
        .and_then(|value| value.checked_add(blobs.len()))
        .ok_or(FormatError::Invalid("file length overflows"))?;
    if total_len as u64 > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("file exceeds the bounded size"));
    }

    let mut output = Vec::with_capacity(total_len);
    output.extend_from_slice(&MAGIC);
    push_u32(&mut output, DOCUMENT_ARCHIVE_VERSION);
    push_u32(
        &mut output,
        CONTAINER_FLAG_COLOR_METADATA
            | if document_metadata.is_some() {
                CONTAINER_FLAG_DOCUMENT_METADATA
            } else {
                0
            }
            | if light_table_metadata.is_some() {
                CONTAINER_FLAG_LIGHT_TABLE_METADATA
            } else {
                0
            }
            | if vector_metadata.is_some() {
                CONTAINER_FLAG_VECTOR_METADATA
            } else {
                0
            }
            | if adjustment_metadata.is_some() {
                CONTAINER_FLAG_ADJUSTMENT_METADATA
            } else {
                0
            },
    );
    push_u64(&mut output, manifest_len as u64);
    push_u64(&mut output, blob_count as u64);

    push_u64(&mut output, document.document_id);
    push_u64(&mut output, document.layer_id);
    push_u64(&mut output, document.main_plane_id);
    push_u64(&mut output, document.color_plane_id);
    output.extend_from_slice(&document.document_uuid);
    push_u32(&mut output, document.width);
    push_u32(&mut output, document.height);
    push_u32(&mut output, document.dpi_x_milli);
    push_u32(&mut output, document.dpi_y_milli);
    push_u32(&mut output, 1); // sRGB
    push_u32(&mut output, 0);
    for frame in [
        document.frames.hundred_frame,
        document.frames.reference_frame,
        document.frames.drawing_frame,
        document.frames.safe_frame,
    ] {
        push_i32(&mut output, frame.x);
        push_i32(&mut output, frame.y);
        push_i32(&mut output, frame.width);
        push_i32(&mut output, frame.height);
    }
    push_u32(&mut output, document.frames.margins.left);
    push_u32(&mut output, document.frames.margins.top);
    push_u32(&mut output, document.frames.margins.right);
    push_u32(&mut output, document.frames.margins.bottom);
    push_u32(&mut output, document.planes.len() as u32);
    push_u32(&mut output, blob_count as u32);

    push_color_value(&mut output, document.main_line_color)?;
    push_u32(&mut output, document.palette.len() as u32);
    push_u32(&mut output, 0);
    for color in &document.palette {
        push_color_value(&mut output, *color)?;
    }
    if let Some(metadata) = &document_metadata {
        push_u32(
            &mut output,
            metadata.len().try_into().map_err(|_| {
                FormatError::Invalid("document metadata length is not representable")
            })?,
        );
        push_u32(&mut output, 0);
        output.extend_from_slice(metadata);
    }
    if let Some(metadata) = &light_table_metadata {
        push_u32(
            &mut output,
            metadata.len().try_into().map_err(|_| {
                FormatError::Invalid("light-table metadata length is not representable")
            })?,
        );
        push_u32(&mut output, 0);
        output.extend_from_slice(metadata);
    }
    if let Some(metadata) = &vector_metadata {
        push_u32(
            &mut output,
            metadata
                .len()
                .try_into()
                .map_err(|_| FormatError::Invalid("vector metadata length is not representable"))?,
        );
        push_u32(&mut output, 0);
        output.extend_from_slice(metadata);
    }
    if let Some(metadata) = &adjustment_metadata {
        push_u32(
            &mut output,
            metadata.len().try_into().map_err(|_| {
                FormatError::Invalid("adjustment metadata length is not representable")
            })?,
        );
        push_u32(&mut output, 0);
        output.extend_from_slice(metadata);
    }

    let mut first_blob = 0_u32;
    for plane in &document.planes {
        push_u64(&mut output, plane.id);
        push_u32(&mut output, plane.kind.code());
        push_u32(&mut output, pixel_format_code(plane.pixel_format));
        push_u32(&mut output, first_blob);
        push_u32(&mut output, plane.tiles.len() as u32);
        push_u32(&mut output, plane.width);
        push_u32(&mut output, plane.height);
        first_blob = first_blob
            .checked_add(plane.tiles.len() as u32)
            .ok_or(FormatError::Invalid("blob index overflows"))?;
    }
    for descriptor in descriptors {
        push_u32(&mut output, descriptor.plane_index);
        push_u32(&mut output, descriptor.tile_x);
        push_u32(&mut output, descriptor.tile_y);
        push_u32(&mut output, descriptor.width);
        push_u32(&mut output, descriptor.height);
        push_u32(&mut output, pixel_format_code(descriptor.pixel_format));
        push_u64(&mut output, descriptor.offset);
        push_u64(&mut output, descriptor.length);
        push_u64(&mut output, descriptor.checksum);
    }
    debug_assert_eq!(output.len(), HEADER_BYTES + manifest_len);
    output.extend_from_slice(&blobs);
    Ok(output)
}

fn encode_document_metadata(metadata: &FileDocumentMetadata) -> Result<Vec<u8>, FormatError> {
    validate_document_metadata(metadata, None)?;
    let mut output = Vec::new();
    output.extend_from_slice(&DOCUMENT_METADATA_MAGIC);
    push_u32(&mut output, 1);
    push_u64(&mut output, metadata.active_layer_id);
    push_u64(&mut output, metadata.active_plane_id);
    push_u64(&mut output, metadata.selection_plane_id);
    push_u32(&mut output, metadata.layers.len() as u32);
    push_u32(&mut output, metadata.guides.len() as u32);
    push_i32(&mut output, metadata.grid.origin_x);
    push_i32(&mut output, metadata.grid.origin_y);
    push_u32(&mut output, metadata.grid.spacing_x);
    push_u32(&mut output, metadata.grid.spacing_y);
    push_u32(&mut output, metadata.grid.subdivisions);
    push_u32(&mut output, 0);
    for layer in &metadata.layers {
        push_u64(&mut output, layer.id);
        push_u32(&mut output, layer.kind.code());
        push_u32(
            &mut output,
            u32::from(layer.visible) | (u32::from(layer.editable) << 1),
        );
        push_u32(&mut output, layer.opacity_milli);
        push_u32(&mut output, layer.name.len() as u32);
        push_u32(&mut output, layer.planes.len() as u32);
        push_u32(&mut output, 0);
        output.extend_from_slice(layer.name.as_bytes());
        for plane in &layer.planes {
            push_u64(&mut output, plane.id);
            push_u32(
                &mut output,
                u32::from(plane.visible) | (u32::from(plane.editable) << 1),
            );
            push_u32(&mut output, plane.opacity_milli);
            push_u32(&mut output, plane.name.len() as u32);
            push_u32(&mut output, 0);
            output.extend_from_slice(plane.name.as_bytes());
        }
    }
    for guide in &metadata.guides {
        push_u64(&mut output, guide.id);
        push_u32(
            &mut output,
            match guide.axis {
                GuideAxis::Horizontal => 1,
                GuideAxis::Vertical => 2,
            },
        );
        push_i32(&mut output, guide.position);
    }
    if output.len() > MAX_MANIFEST_BYTES as usize {
        return Err(FormatError::Invalid("document metadata exceeds its bound"));
    }
    Ok(output)
}

const fn pixel_format_code(format: PixelFormat) -> u32 {
    match format {
        PixelFormat::BinaryMask8 => 1,
        PixelFormat::StraightRgba8 => 2,
        PixelFormat::PremultipliedBgra8 => 3,
        PixelFormat::Grayscale8 => 4,
        PixelFormat::Grayscale16 => 5,
        PixelFormat::StraightRgba16 => 6,
    }
}

pub(crate) fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_color_value(output: &mut Vec<u8>, color: PixelValue) -> Result<(), FormatError> {
    let (depth, channels) = match color {
        PixelValue::Rgba(value) => (8_u32, value.map(u16::from)),
        PixelValue::Rgba16(value) => (16_u32, value),
        _ => return Err(FormatError::Invalid("color metadata value is not RGBA")),
    };
    push_u32(output, depth);
    push_u32(output, 0);
    for channel in channels {
        output.extend_from_slice(&channel.to_le_bytes());
    }
    Ok(())
}
