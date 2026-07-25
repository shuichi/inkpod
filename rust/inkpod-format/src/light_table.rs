//! Native light-table metadata encoding.

use super::{
    FormatError, MAX_MANIFEST_BYTES, MAX_NODE_NAME_BYTES, MAX_PLANES, PixelValue, Reader, RectI32,
    push_color_value, push_i32, push_u32, push_u64,
};
use std::collections::BTreeSet;

const MAX_LIGHT_TABLE_SETS: usize = 256;
const MAX_LIGHT_TABLE_ITEMS: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightTableDisplayMode {
    Color,
    Monotone,
    Halftone,
}

impl LightTableDisplayMode {
    const fn code(self) -> u32 {
        match self {
            Self::Color => 1,
            Self::Monotone => 2,
            Self::Halftone => 3,
        }
    }

    fn from_code(value: u32) -> Result<Self, FormatError> {
        match value {
            1 => Ok(Self::Color),
            2 => Ok(Self::Monotone),
            3 => Ok(Self::Halftone),
            _ => Err(FormatError::Unsupported("unknown light-table display mode")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileLightTableItem {
    pub id: u64,
    pub source_plane_id: u64,
    pub source_document_uuid: [u8; 16],
    pub source_revision: u64,
    pub source_reference_frame: RectI32,
    pub source_dpi_x_milli: u32,
    pub source_dpi_y_milli: u32,
    pub name: String,
    pub visible: bool,
    pub opacity_milli: u32,
    pub display_mode: LightTableDisplayMode,
    pub display_color: PixelValue,
    pub translate_x_milli: i32,
    pub translate_y_milli: i32,
    pub scale_x_milli: u32,
    pub scale_y_milli: u32,
    pub rotation_milli_degrees: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileLightTableSet {
    pub id: u64,
    pub name: String,
    pub global_opacity_milli: u32,
    pub items: Vec<FileLightTableItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileLightTableMetadata {
    pub active_set_id: u64,
    pub sets: Vec<FileLightTableSet>,
}

pub(super) fn encode_light_table_metadata(
    metadata: &FileLightTableMetadata,
) -> Result<Vec<u8>, FormatError> {
    validate_light_table_metadata(metadata, None)?;
    let mut output = Vec::new();
    output.extend_from_slice(b"M4WF");
    push_u32(&mut output, 1);
    push_u64(&mut output, metadata.active_set_id);
    push_u32(&mut output, metadata.sets.len() as u32);
    push_u32(&mut output, 0);
    for set in &metadata.sets {
        push_u64(&mut output, set.id);
        push_u32(&mut output, set.global_opacity_milli);
        push_u32(&mut output, set.name.len() as u32);
        push_u32(&mut output, set.items.len() as u32);
        push_u32(&mut output, 0);
        output.extend_from_slice(set.name.as_bytes());
        for item in &set.items {
            push_u64(&mut output, item.id);
            push_u64(&mut output, item.source_plane_id);
            output.extend_from_slice(&item.source_document_uuid);
            push_u64(&mut output, item.source_revision);
            for value in [
                item.source_reference_frame.x,
                item.source_reference_frame.y,
                item.source_reference_frame.width,
                item.source_reference_frame.height,
            ] {
                push_i32(&mut output, value);
            }
            push_u32(&mut output, item.source_dpi_x_milli);
            push_u32(&mut output, item.source_dpi_y_milli);
            push_u32(&mut output, u32::from(item.visible));
            push_u32(&mut output, item.opacity_milli);
            push_u32(&mut output, item.display_mode.code());
            push_color_value(&mut output, item.display_color)?;
            push_i32(&mut output, item.translate_x_milli);
            push_i32(&mut output, item.translate_y_milli);
            push_u32(&mut output, item.scale_x_milli);
            push_u32(&mut output, item.scale_y_milli);
            push_i32(&mut output, item.rotation_milli_degrees);
            push_u32(&mut output, item.name.len() as u32);
            output.extend_from_slice(item.name.as_bytes());
        }
    }
    if output.len() > MAX_MANIFEST_BYTES as usize {
        return Err(FormatError::Invalid(
            "light-table metadata exceeds its bound",
        ));
    }
    Ok(output)
}

pub(super) fn decode_light_table_metadata(
    bytes: &[u8],
) -> Result<FileLightTableMetadata, FormatError> {
    let mut reader = Reader::new(bytes);
    if reader.take(4)? != b"M4WF" || reader.u32()? != 1 {
        return Err(FormatError::Unsupported(
            "light-table metadata version is not supported",
        ));
    }
    let active_set_id = reader.u64()?;
    let set_count = reader.u32()? as usize;
    if reader.u32()? != 0 || set_count == 0 || set_count > MAX_LIGHT_TABLE_SETS {
        return Err(FormatError::Invalid(
            "light-table set count is outside bounds",
        ));
    }
    let mut sets = Vec::with_capacity(set_count);
    let mut total_items = 0_usize;
    for _ in 0..set_count {
        let id = reader.u64()?;
        let global_opacity_milli = reader.u32()?;
        let name_len = reader.u32()? as usize;
        let item_count = reader.u32()? as usize;
        if reader.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "light-table set reserved field is not zero",
            ));
        }
        total_items = total_items
            .checked_add(item_count)
            .ok_or(FormatError::Invalid("light-table item count overflows"))?;
        if item_count > MAX_PLANES || total_items > MAX_LIGHT_TABLE_ITEMS {
            return Err(FormatError::Invalid(
                "light-table item count is outside bounds",
            ));
        }
        let name = read_name(&mut reader, name_len)?;
        let mut items = Vec::with_capacity(item_count);
        for _ in 0..item_count {
            let item_id = reader.u64()?;
            let source_plane_id = reader.u64()?;
            let source_document_uuid: [u8; 16] = reader
                .take(16)?
                .try_into()
                .map_err(|_| FormatError::Invalid("light-table source UUID is truncated"))?;
            let source_revision = reader.u64()?;
            let source_reference_frame = RectI32 {
                x: reader.i32()?,
                y: reader.i32()?,
                width: reader.i32()?,
                height: reader.i32()?,
            };
            let source_dpi_x_milli = reader.u32()?;
            let source_dpi_y_milli = reader.u32()?;
            let flags = reader.u32()?;
            if flags & !1 != 0 {
                return Err(FormatError::Unsupported("unknown light-table item flags"));
            }
            let opacity_milli = reader.u32()?;
            let display_mode = LightTableDisplayMode::from_code(reader.u32()?)?;
            let display_color = reader.color_value()?;
            let translate_x_milli = reader.i32()?;
            let translate_y_milli = reader.i32()?;
            let scale_x_milli = reader.u32()?;
            let scale_y_milli = reader.u32()?;
            let rotation_milli_degrees = reader.i32()?;
            let item_name_len = reader.u32()? as usize;
            items.push(FileLightTableItem {
                id: item_id,
                source_plane_id,
                source_document_uuid,
                source_revision,
                source_reference_frame,
                source_dpi_x_milli,
                source_dpi_y_milli,
                name: read_name(&mut reader, item_name_len)?,
                visible: flags & 1 != 0,
                opacity_milli,
                display_mode,
                display_color,
                translate_x_milli,
                translate_y_milli,
                scale_x_milli,
                scale_y_milli,
                rotation_milli_degrees,
            });
        }
        sets.push(FileLightTableSet {
            id,
            name,
            global_opacity_milli,
            items,
        });
    }
    if reader.position != bytes.len() {
        return Err(FormatError::Invalid(
            "light-table metadata has trailing bytes",
        ));
    }
    let metadata = FileLightTableMetadata {
        active_set_id,
        sets,
    };
    validate_light_table_metadata(&metadata, None)?;
    Ok(metadata)
}

fn read_name(reader: &mut Reader<'_>, length: usize) -> Result<String, FormatError> {
    if length == 0 || length > MAX_NODE_NAME_BYTES {
        return Err(FormatError::Invalid(
            "light-table name length is outside bounds",
        ));
    }
    let text = std::str::from_utf8(reader.take(length)?)
        .map_err(|_| FormatError::Invalid("light-table name is not valid UTF-8"))?;
    if text.chars().any(char::is_control) {
        return Err(FormatError::Invalid(
            "light-table name contains control characters",
        ));
    }
    Ok(text.to_owned())
}

pub(super) fn validate_light_table_metadata(
    metadata: &FileLightTableMetadata,
    source_plane_ids: Option<&BTreeSet<u64>>,
) -> Result<(), FormatError> {
    if metadata.sets.is_empty() || metadata.sets.len() > MAX_LIGHT_TABLE_SETS {
        return Err(FormatError::Invalid(
            "light-table set count is outside bounds",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut referenced_planes = BTreeSet::new();
    let mut active_found = false;
    let mut item_count = 0_usize;
    for set in &metadata.sets {
        validate_name(&set.name)?;
        if set.id == 0 || !ids.insert(set.id) || set.global_opacity_milli > 1_000 {
            return Err(FormatError::Invalid(
                "light-table set properties are invalid",
            ));
        }
        active_found |= set.id == metadata.active_set_id;
        item_count = item_count
            .checked_add(set.items.len())
            .ok_or(FormatError::Invalid("light-table item count overflows"))?;
        if item_count > MAX_LIGHT_TABLE_ITEMS {
            return Err(FormatError::Invalid(
                "light-table item count exceeds its bound",
            ));
        }
        for item in &set.items {
            validate_name(&item.name)?;
            if item.id == 0
                || item.source_plane_id == 0
                || !ids.insert(item.id)
                || !ids.insert(item.source_plane_id)
                || !referenced_planes.insert(item.source_plane_id)
                || item.source_document_uuid.iter().all(|byte| *byte == 0)
                || item.source_revision == 0
                || item.source_dpi_x_milli == 0
                || item.source_dpi_y_milli == 0
                || item.opacity_milli > 1_000
                || item.scale_x_milli == 0
                || item.scale_y_milli == 0
                || item.scale_x_milli > 64_000
                || item.scale_y_milli > 64_000
                || item.rotation_milli_degrees.unsigned_abs() > 360_000
                || item.source_reference_frame.width <= 0
                || item.source_reference_frame.height <= 0
                || item.display_color.rgba16().is_none()
            {
                return Err(FormatError::Invalid(
                    "light-table item properties are invalid",
                ));
            }
        }
    }
    if !active_found {
        return Err(FormatError::Invalid("light-table active set ID is invalid"));
    }
    if source_plane_ids.is_some_and(|planes| planes != &referenced_planes) {
        return Err(FormatError::Invalid(
            "light-table item and light-table plane IDs differ",
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), FormatError> {
    if name.is_empty() || name.len() > MAX_NODE_NAME_BYTES || name.chars().any(char::is_control) {
        Err(FormatError::Invalid("light-table name is invalid"))
    } else {
        Ok(())
    }
}
