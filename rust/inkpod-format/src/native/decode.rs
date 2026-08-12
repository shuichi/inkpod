use super::model::*;
use super::validate::{validate_document, validate_document_metadata, validate_tile_shape};
use crate::adjustment::decode_adjustment_metadata;
use crate::light_table::decode_light_table_metadata;
use crate::vector::decode_vector_metadata;
use inkpod_image::{MAX_PALETTE_COLORS, PixelFormat, PixelValue, TileCoord};
use std::collections::BTreeSet;
pub fn decode_document_archive(bytes: &[u8]) -> Result<DocumentArchive, FormatError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("file exceeds the bounded size"));
    }
    let mut reader = Reader::new(bytes);
    if reader.take(8)? != MAGIC {
        return Err(FormatError::Invalid("magic does not match"));
    }
    if reader.u32()? != DOCUMENT_ARCHIVE_VERSION {
        return Err(FormatError::Unsupported("format version is not supported"));
    }
    let container_flags = reader.u32()?;
    if container_flags
        & !(CONTAINER_FLAG_COLOR_METADATA
            | CONTAINER_FLAG_DOCUMENT_METADATA
            | CONTAINER_FLAG_LIGHT_TABLE_METADATA
            | CONTAINER_FLAG_VECTOR_METADATA
            | CONTAINER_FLAG_ADJUSTMENT_METADATA)
        != 0
    {
        return Err(FormatError::Unsupported(
            "required container flags are unknown",
        ));
    }
    if container_flags & CONTAINER_FLAG_COLOR_METADATA == 0 {
        return Err(FormatError::Invalid("required color metadata is missing"));
    }
    let manifest_len = reader.u64()?;
    let header_blob_count = reader.u64()?;
    if manifest_len < FIXED_MANIFEST_BYTES as u64 || manifest_len > MAX_MANIFEST_BYTES {
        return Err(FormatError::Invalid("manifest length is outside bounds"));
    }
    let manifest_end = HEADER_BYTES
        .checked_add(
            usize::try_from(manifest_len)
                .map_err(|_| FormatError::Invalid("manifest length is not representable"))?,
        )
        .ok_or(FormatError::Invalid("manifest end overflows"))?;
    if manifest_end > bytes.len() {
        return Err(FormatError::Invalid("manifest is truncated"));
    }

    let document_id = reader.u64()?;
    let cell_id = reader.u64()?;
    let layer_id = reader.u64()?;
    let main_plane_id = reader.u64()?;
    let color_plane_id = reader.u64()?;
    let document_uuid: [u8; 16] = reader
        .take(16)?
        .try_into()
        .map_err(|_| FormatError::Invalid("document UUID is truncated"))?;
    let width = reader.u32()?;
    let height = reader.u32()?;
    let dpi_x_milli = reader.u32()?;
    let dpi_y_milli = reader.u32()?;
    if reader.u32()? != 1 {
        return Err(FormatError::Unsupported("required color space is unknown"));
    }
    if reader.u32()? != 0 {
        return Err(FormatError::Unsupported(
            "manifest reserved field is not zero",
        ));
    }
    let mut rects = [RectI32::default(); 6];
    for rect in &mut rects {
        *rect = RectI32 {
            x: reader.i32()?,
            y: reader.i32()?,
            width: reader.i32()?,
            height: reader.i32()?,
        };
    }
    let margins = Margins {
        left: reader.u32()?,
        top: reader.u32()?,
        right: reader.u32()?,
        bottom: reader.u32()?,
    };
    let plane_count = reader.u32()? as usize;
    let manifest_blob_count = reader.u32()? as usize;
    if plane_count == 0 || plane_count > MAX_PLANES {
        return Err(FormatError::Invalid("plane count is outside bounds"));
    }
    if manifest_blob_count > MAX_BLOBS || header_blob_count != manifest_blob_count as u64 {
        return Err(FormatError::Invalid("blob count is inconsistent"));
    }
    let main_line_color = reader.color_value()?;
    let palette_count = reader.u32()? as usize;
    if reader.u32()? != 0 {
        return Err(FormatError::Unsupported(
            "color metadata reserved field is not zero",
        ));
    }
    if palette_count > MAX_PALETTE_COLORS {
        return Err(FormatError::Invalid("palette count exceeds its bound"));
    }
    let mut palette = Vec::with_capacity(palette_count);
    for _ in 0..palette_count {
        palette.push(reader.color_value()?);
    }
    let color_metadata_len = COLOR_METADATA_FIXED_BYTES
        .checked_add(
            palette
                .len()
                .checked_mul(COLOR_VALUE_BYTES)
                .ok_or(FormatError::Invalid("palette manifest overflows"))?,
        )
        .ok_or(FormatError::Invalid("color metadata length overflows"))?;
    let (document_metadata, document_metadata_len) =
        if container_flags & CONTAINER_FLAG_DOCUMENT_METADATA != 0 {
            let byte_count = reader.u32()? as usize;
            if reader.u32()? != 0 {
                return Err(FormatError::Unsupported(
                    "document metadata reserved field is not zero",
                ));
            }
            if byte_count > MAX_MANIFEST_BYTES as usize {
                return Err(FormatError::Invalid("document metadata exceeds its bound"));
            }
            let metadata = decode_document_metadata(reader.take(byte_count)?)?;
            (Some(metadata), byte_count.saturating_add(8))
        } else {
            (None, 0)
        };
    let (light_table_metadata, light_table_metadata_len) =
        if container_flags & CONTAINER_FLAG_LIGHT_TABLE_METADATA != 0 {
            let byte_count = reader.u32()? as usize;
            if reader.u32()? != 0 {
                return Err(FormatError::Unsupported(
                    "light-table metadata reserved field is not zero",
                ));
            }
            if byte_count > MAX_MANIFEST_BYTES as usize {
                return Err(FormatError::Invalid(
                    "light-table metadata exceeds its bound",
                ));
            }
            let metadata = decode_light_table_metadata(reader.take(byte_count)?)?;
            (Some(metadata), byte_count.saturating_add(8))
        } else {
            (None, 0)
        };
    let (vector_metadata, vector_metadata_len) =
        if container_flags & CONTAINER_FLAG_VECTOR_METADATA != 0 {
            let byte_count = reader.u32()? as usize;
            if reader.u32()? != 0 {
                return Err(FormatError::Unsupported(
                    "vector metadata reserved field is not zero",
                ));
            }
            if byte_count > MAX_MANIFEST_BYTES as usize {
                return Err(FormatError::Invalid("vector metadata exceeds its bound"));
            }
            let metadata = decode_vector_metadata(reader.take(byte_count)?)?;
            (Some(metadata), byte_count.saturating_add(8))
        } else {
            (None, 0)
        };
    let (adjustment_metadata, adjustment_metadata_len) =
        if container_flags & CONTAINER_FLAG_ADJUSTMENT_METADATA != 0 {
            let byte_count = reader.u32()? as usize;
            if reader.u32()? != 0 {
                return Err(FormatError::Unsupported(
                    "adjustment metadata reserved field is not zero",
                ));
            }
            if byte_count > MAX_MANIFEST_BYTES as usize {
                return Err(FormatError::Invalid(
                    "adjustment metadata exceeds its bound",
                ));
            }
            let metadata = decode_adjustment_metadata(reader.take(byte_count)?)?;
            (Some(metadata), byte_count.saturating_add(8))
        } else {
            (None, 0)
        };
    let expected_manifest_len = FIXED_MANIFEST_BYTES
        .checked_add(color_metadata_len)
        .and_then(|value| value.checked_add(document_metadata_len))
        .and_then(|value| value.checked_add(light_table_metadata_len))
        .and_then(|value| value.checked_add(vector_metadata_len))
        .and_then(|value| value.checked_add(adjustment_metadata_len))
        .and_then(|value| value.checked_add(plane_count.checked_mul(PLANE_DESCRIPTOR_BYTES)?))
        .and_then(|value| {
            value.checked_add(manifest_blob_count.checked_mul(BLOB_DESCRIPTOR_BYTES)?)
        })
        .ok_or(FormatError::Invalid("manifest length overflows"))?;
    if expected_manifest_len != manifest_len as usize {
        return Err(FormatError::Invalid(
            "manifest length does not match its counts",
        ));
    }

    struct PlaneDescriptor {
        id: u64,
        kind: PlaneKind,
        pixel_format: PixelFormat,
        first_blob: usize,
        blob_count: usize,
        width: u32,
        height: u32,
    }
    let mut plane_descriptors = Vec::with_capacity(plane_count);
    let mut ids = BTreeSet::new();
    for id in [document_id, layer_id, main_plane_id, color_plane_id] {
        if id == 0 || !ids.insert(id) {
            return Err(FormatError::Invalid(
                "stable IDs must be nonzero and unique",
            ));
        }
    }
    let mut plane_ids = BTreeSet::new();
    for _ in 0..plane_count {
        let id = reader.u64()?;
        if id == 0 || !plane_ids.insert(id) {
            return Err(FormatError::Invalid("plane ID is invalid"));
        }
        plane_descriptors.push(PlaneDescriptor {
            id,
            kind: PlaneKind::from_code(reader.u32()?)?,
            pixel_format: pixel_format_from_code(reader.u32()?)?,
            first_blob: reader.u32()? as usize,
            blob_count: reader.u32()? as usize,
            width: reader.u32()?,
            height: reader.u32()?,
        });
    }
    let mut next_blob = 0_usize;
    for descriptor in &plane_descriptors {
        if descriptor.first_blob != next_blob {
            return Err(FormatError::Invalid(
                "plane blob ranges are not contiguous and ordered",
            ));
        }
        next_blob = next_blob
            .checked_add(descriptor.blob_count)
            .ok_or(FormatError::Invalid("plane blob range overflows"))?;
    }
    if next_blob != manifest_blob_count {
        return Err(FormatError::Invalid(
            "plane blob ranges do not cover the manifest",
        ));
    }
    let mut blob_descriptors = Vec::with_capacity(manifest_blob_count);
    for _ in 0..manifest_blob_count {
        blob_descriptors.push(BlobDescriptor {
            plane_index: reader.u32()?,
            tile_x: reader.u32()?,
            tile_y: reader.u32()?,
            width: reader.u32()?,
            height: reader.u32()?,
            pixel_format: pixel_format_from_code(reader.u32()?)?,
            offset: reader.u64()?,
            length: reader.u64()?,
            checksum: reader.u64()?,
        });
    }
    if reader.position != manifest_end {
        return Err(FormatError::Invalid(
            "manifest cursor did not end at its boundary",
        ));
    }

    let blob_area = &bytes[manifest_end..];
    let mut next_offset = 0_u64;
    for blob in &blob_descriptors {
        if blob.offset != next_offset {
            return Err(FormatError::Invalid(
                "blob ranges are not contiguous and ordered",
            ));
        }
        next_offset = next_offset
            .checked_add(blob.length)
            .ok_or(FormatError::Invalid("blob range overflows"))?;
    }
    if next_offset != blob_area.len() as u64 {
        return Err(FormatError::Invalid(
            "blob ranges do not cover the file blob area",
        ));
    }
    let mut planes = Vec::with_capacity(plane_count);
    for (plane_index, descriptor) in plane_descriptors.into_iter().enumerate() {
        if descriptor.kind != PlaneKind::LightTable
            && (descriptor.width != width || descriptor.height != height)
        {
            return Err(FormatError::Invalid(
                "plane dimensions do not match the document",
            ));
        }
        let end_blob = descriptor
            .first_blob
            .checked_add(descriptor.blob_count)
            .ok_or(FormatError::Invalid("plane blob range overflows"))?;
        if end_blob > blob_descriptors.len() {
            return Err(FormatError::Invalid(
                "plane blob range is outside the manifest",
            ));
        }
        let mut tile_coords = BTreeSet::new();
        let mut tiles = Vec::with_capacity(descriptor.blob_count);
        for blob in &blob_descriptors[descriptor.first_blob..end_blob] {
            if blob.plane_index as usize != plane_index
                || blob.pixel_format != descriptor.pixel_format
            {
                return Err(FormatError::Invalid(
                    "blob references the wrong plane or format",
                ));
            }
            let coord = TileCoord {
                x: blob.tile_x,
                y: blob.tile_y,
            };
            if !tile_coords.insert(coord) {
                return Err(FormatError::Invalid("duplicate tile coordinates"));
            }
            validate_tile_shape(
                descriptor.width,
                descriptor.height,
                descriptor.pixel_format,
                coord,
                blob.width,
                blob.height,
                blob.length,
            )?;
            let start = usize::try_from(blob.offset)
                .map_err(|_| FormatError::Invalid("blob offset is not representable"))?;
            let length = usize::try_from(blob.length)
                .map_err(|_| FormatError::Invalid("blob length is not representable"))?;
            let end = start
                .checked_add(length)
                .ok_or(FormatError::Invalid("blob range overflows"))?;
            let data = blob_area
                .get(start..end)
                .ok_or(FormatError::Invalid("blob range is outside the file"))?;
            if checksum(data) != blob.checksum {
                return Err(FormatError::ChecksumMismatch);
            }
            tiles.push(FileTile {
                coord,
                width: blob.width,
                height: blob.height,
                bytes: data.to_vec(),
            });
        }
        planes.push(FilePlane {
            id: descriptor.id,
            kind: descriptor.kind,
            pixel_format: descriptor.pixel_format,
            width: descriptor.width,
            height: descriptor.height,
            tiles,
        });
    }

    let document = DocumentArchive {
        document_uuid,
        document_id,
        cell_id,
        layer_id,
        main_plane_id,
        color_plane_id,
        width,
        height,
        dpi_x_milli,
        dpi_y_milli,
        frames: FrameMetadata {
            hundred_frame: rects[0],
            reference_frame: rects[1],
            drawing_frame: rects[2],
            safe_frame: rects[3],
            shooting_frame: rects[4],
            maximum_close_frame: rects[5],
            margins,
        },
        main_line_color,
        palette,
        planes,
        document_metadata,
        light_table_metadata,
        vector_metadata,
        adjustment_metadata,
    };
    validate_document(&document)?;
    Ok(document)
}

fn decode_document_metadata(bytes: &[u8]) -> Result<FileDocumentMetadata, FormatError> {
    let mut reader = Reader::new(bytes);
    if reader.take(4)? != DOCUMENT_METADATA_MAGIC || reader.u32()? != 4 {
        return Err(FormatError::Unsupported(
            "document metadata version is not supported",
        ));
    }
    let active_layer_id = reader.u64()?;
    let active_plane_id = reader.u64()?;
    let selection_plane_id = reader.u64()?;
    let layer_count = reader.u32()? as usize;
    let guide_count = reader.u32()? as usize;
    let annotation_count = reader.u32()? as usize;
    let shooting_frame_present = match reader.u32()? {
        0 => false,
        1 => true,
        _ => {
            return Err(FormatError::Unsupported(
                "document shooting-frame presence field is invalid",
            ));
        }
    };
    if layer_count == 0
        || layer_count > MAX_LAYERS
        || guide_count > MAX_GUIDES
        || annotation_count > MAX_ANNOTATION_OBJECTS
    {
        return Err(FormatError::Invalid(
            "document layer or guide count is outside bounds",
        ));
    }
    let grid = FileGrid {
        origin_x: reader.i32()?,
        origin_y: reader.i32()?,
        spacing_x: reader.u32()?,
        spacing_y: reader.u32()?,
        subdivisions: reader.u32()?,
    };
    let color_chart_length = reader.u32()? as usize;
    let color_chart_locked = match reader.u32()? {
        0 => false,
        1 => true,
        _ => {
            return Err(FormatError::Unsupported(
                "document Color chart lock field is invalid",
            ));
        }
    };
    if color_chart_length > MAX_MANIFEST_BYTES as usize {
        return Err(FormatError::Unsupported(
            "document Color chart length is unsupported",
        ));
    }
    let mut layers = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        let id = reader.u64()?;
        let kind = LayerKind::from_code(reader.u32()?)?;
        let flags = reader.u32()?;
        if flags & !3 != 0 {
            return Err(FormatError::Unsupported("unknown document layer flags"));
        }
        let opacity_milli = reader.u32()?;
        let name_len = reader.u32()? as usize;
        let plane_count = reader.u32()? as usize;
        if reader.u32()? != 0 || plane_count > MAX_PLANES {
            return Err(FormatError::Invalid("document layer descriptor is invalid"));
        }
        let name = read_name(&mut reader, name_len)?;
        let mut planes = Vec::with_capacity(plane_count);
        for _ in 0..plane_count {
            let plane_id = reader.u64()?;
            let plane_flags = reader.u32()?;
            if plane_flags & !3 != 0 {
                return Err(FormatError::Unsupported("unknown document plane flags"));
            }
            let plane_opacity = reader.u32()?;
            let plane_name_len = reader.u32()? as usize;
            if reader.u32()? != 0 {
                return Err(FormatError::Unsupported(
                    "document plane reserved field is not zero",
                ));
            }
            planes.push(FilePlaneProperties {
                id: plane_id,
                name: read_name(&mut reader, plane_name_len)?,
                visible: flags_bit(plane_flags, 0),
                editable: flags_bit(plane_flags, 1),
                opacity_milli: plane_opacity,
            });
        }
        layers.push(FileLayer {
            id,
            kind,
            name,
            visible: flags_bit(flags, 0),
            editable: flags_bit(flags, 1),
            opacity_milli,
            planes,
        });
    }
    let mut guides = Vec::with_capacity(guide_count);
    for _ in 0..guide_count {
        guides.push(FileGuide {
            id: reader.u64()?,
            axis: match reader.u32()? {
                1 => GuideAxis::Horizontal,
                2 => GuideAxis::Vertical,
                _ => return Err(FormatError::Unsupported("unknown guide axis")),
            },
            position: reader.i32()?,
        });
    }
    let shooting_frame = if shooting_frame_present {
        let id = reader.u64()?;
        let center_x_milli = reader.i64()?;
        let center_y_milli = reader.i64()?;
        let width_milli = reader.u64()?;
        let height_milli = reader.u64()?;
        let rotation_turns = reader.u32()?;
        let anchor = match reader.u32()? {
            1 => FileShootingFrameAnchor::TopLeft,
            2 => FileShootingFrameAnchor::TopRight,
            3 => FileShootingFrameAnchor::Center,
            4 => FileShootingFrameAnchor::BottomLeft,
            5 => FileShootingFrameAnchor::BottomRight,
            _ => return Err(FormatError::Unsupported("unknown shooting-frame anchor")),
        };
        let flags = reader.u32()?;
        if flags & !3 != 0 || reader.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "shooting-frame flags or reserved field is invalid",
            ));
        }
        Some(FileShootingFrame {
            id,
            center_x_milli,
            center_y_milli,
            width_milli,
            height_milli,
            rotation_turns,
            anchor,
            visible: flags_bit(flags, 0),
            include_in_instruction_export: flags_bit(flags, 1),
        })
    } else {
        None
    };
    let mut annotations = Vec::with_capacity(annotation_count);
    for _ in 0..annotation_count {
        let id = reader.u64()?;
        let layer_id = reader.u64()?;
        let kind = match reader.u32()? {
            1 => FileAnnotationKind::Text,
            2 => FileAnnotationKind::Stroke,
            3 => FileAnnotationKind::Leader,
            4 => FileAnnotationKind::Value,
            _ => return Err(FormatError::Unsupported("unknown annotation object kind")),
        };
        let output = match reader.u32()? {
            1 => FileAnnotationOutput::Normal,
            2 => FileAnnotationOutput::Instruction,
            _ => return Err(FormatError::Unsupported("unknown annotation output kind")),
        };
        let bounds = RectI32 {
            x: reader.i32()?,
            y: reader.i32()?,
            width: reader.i32()?,
            height: reader.i32()?,
        };
        let font_length = reader.u32()? as usize;
        let text_length = reader.u32()? as usize;
        let point_count = reader.u32()? as usize;
        let font_size_milli = reader.u32()?;
        let style_flags = reader.u32()?;
        let stroke_width_milli = reader.u32()?;
        let color = reader.color_value()?;
        if font_length > MAX_NODE_NAME_BYTES
            || text_length > MAX_ANNOTATION_TEXT_BYTES
            || point_count > MAX_ANNOTATION_POINTS
        {
            return Err(FormatError::Invalid(
                "annotation variable-length field exceeds its bound",
            ));
        }
        let font_family_hint = read_utf8(&mut reader, font_length, "annotation font is invalid")?;
        let text = read_utf8(&mut reader, text_length, "annotation text is invalid")?;
        let mut points = Vec::with_capacity(point_count);
        for _ in 0..point_count {
            points.push(FileAnnotationPoint {
                x_milli: reader.i32()?,
                y_milli: reader.i32()?,
            });
        }
        annotations.push(FileAnnotationObject {
            id,
            layer_id,
            kind,
            output,
            bounds,
            font_family_hint,
            font_size_milli,
            style_flags,
            color,
            text,
            points,
            stroke_width_milli,
        });
    }
    let color_chart = crate::decode_color_chart(reader.take(color_chart_length)?)?;
    if reader.position != bytes.len() {
        return Err(FormatError::Invalid("document metadata has trailing bytes"));
    }
    let metadata = FileDocumentMetadata {
        active_layer_id,
        active_plane_id,
        selection_plane_id,
        layers,
        guides,
        grid,
        color_chart,
        color_chart_locked,
        annotations,
        shooting_frame,
    };
    validate_document_metadata(&metadata, None)?;
    Ok(metadata)
}

const fn flags_bit(flags: u32, bit: u32) -> bool {
    flags & (1 << bit) != 0
}

fn read_name(reader: &mut Reader<'_>, length: usize) -> Result<String, FormatError> {
    if length == 0 || length > MAX_NODE_NAME_BYTES {
        return Err(FormatError::Invalid("node name length is outside bounds"));
    }
    let text = std::str::from_utf8(reader.take(length)?)
        .map_err(|_| FormatError::Invalid("node name is not valid UTF-8"))?;
    if text.chars().any(char::is_control) {
        return Err(FormatError::Invalid(
            "node name contains control characters",
        ));
    }
    Ok(text.to_owned())
}

fn read_utf8(
    reader: &mut Reader<'_>,
    length: usize,
    message: &'static str,
) -> Result<String, FormatError> {
    std::str::from_utf8(reader.take(length)?)
        .map(str::to_owned)
        .map_err(|_| FormatError::Invalid(message))
}

fn pixel_format_from_code(value: u32) -> Result<PixelFormat, FormatError> {
    match value {
        1 => Ok(PixelFormat::BinaryMask8),
        2 => Ok(PixelFormat::StraightRgba8),
        3 => Ok(PixelFormat::PremultipliedBgra8),
        4 => Ok(PixelFormat::Grayscale8),
        5 => Ok(PixelFormat::Grayscale16),
        6 => Ok(PixelFormat::StraightRgba16),
        _ => Err(FormatError::Unsupported("unknown required pixel format")),
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    pub(crate) position: usize,
}

impl<'a> Reader<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    pub(crate) fn take(&mut self, length: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(FormatError::Invalid("input cursor overflows"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(FormatError::Invalid("input is truncated"))?;
        self.position = end;
        Ok(value)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, FormatError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| FormatError::Invalid("u32 is truncated"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    pub(crate) fn i32(&mut self) -> Result<i32, FormatError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| FormatError::Invalid("i32 is truncated"))?;
        Ok(i32::from_le_bytes(bytes))
    }

    pub(crate) fn i64(&mut self) -> Result<i64, FormatError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| FormatError::Invalid("i64 is truncated"))?;
        Ok(i64::from_le_bytes(bytes))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, FormatError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| FormatError::Invalid("u64 is truncated"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    pub(crate) fn color_value(&mut self) -> Result<PixelValue, FormatError> {
        let depth = self.u32()?;
        if self.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "color metadata record reserved field is not zero",
            ));
        }
        let mut channels = [0_u16; 4];
        for channel in &mut channels {
            let bytes: [u8; 2] = self
                .take(2)?
                .try_into()
                .map_err(|_| FormatError::Invalid("color metadata is truncated"))?;
            *channel = u16::from_le_bytes(bytes);
        }
        match depth {
            8 if channels
                .iter()
                .all(|channel| *channel <= u16::from(u8::MAX)) =>
            {
                Ok(PixelValue::Rgba(channels.map(|channel| channel as u8)))
            }
            16 => Ok(PixelValue::Rgba16(channels)),
            8 => Err(FormatError::Invalid(
                "8-bit color metadata contains a channel above 255",
            )),
            _ => Err(FormatError::Unsupported(
                "color metadata depth is not supported",
            )),
        }
    }
}
