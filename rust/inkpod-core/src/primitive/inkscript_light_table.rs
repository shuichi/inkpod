//! Private pre-ratification InkScript adapter for replayable Light Table primitives.

use super::CanonicalInvocation;
use super::inkscript_batch;
use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use crate::asset::AssetRecord;
use crate::{
    LightTableDisplayMode, LightTableItemInput, LightTableItemProperties, LightTableSource,
    PixelFormat, PixelValue, RectI32,
};
use inkpod_format::{
    InkScriptCommandResultSchema, InkScriptCommandSchema, InkScriptEnumSchema,
    InkScriptFieldSchema, InkScriptRecordSchema, InkScriptResultAvailability, InkScriptTypedStep,
    InkScriptTypedValue, InkScriptTypedValueKind,
};
use std::collections::BTreeMap;
use std::sync::Arc;

const MAX_LIGHT_TABLE_ITEMS: usize = 4_096;
const MAX_NODE_NAME_BYTES: usize = 1_024;

pub(crate) const LIGHT_TABLE_ENUMS: &[InkScriptEnumSchema] = &[InkScriptEnumSchema::new(
    "light_table_display_mode",
    &["color", "monotone", "halftone"],
)];

const LIGHT_TABLE_SOURCE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("document_uuid", "uuid", 0),
    InkScriptFieldSchema::required("source_revision", "u64", 1),
    InkScriptFieldSchema::required("reference_frame", "pixel_rect", 2),
    InkScriptFieldSchema::required("dpi_x_milli", "u32", 3),
    InkScriptFieldSchema::required("dpi_y_milli", "u32", 4),
    InkScriptFieldSchema::required("raster", "asset_ref", 5),
];
const LIGHT_TABLE_PROPERTIES_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("visible", "bool", 0),
    InkScriptFieldSchema::required("opacity_milli", "u32", 1),
    InkScriptFieldSchema::required("display_mode", "light_table_display_mode", 2),
    InkScriptFieldSchema::required("display_color", "pixel_value", 3),
    InkScriptFieldSchema::required("translate_x_milli", "i32", 4),
    InkScriptFieldSchema::required("translate_y_milli", "i32", 5),
    InkScriptFieldSchema::required("scale_x_milli", "u32", 6),
    InkScriptFieldSchema::required("scale_y_milli", "u32", 7),
    InkScriptFieldSchema::required("rotation_milli_degrees", "i32", 8),
];
const LIGHT_TABLE_ITEM_INPUT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("name", "string", 0),
    InkScriptFieldSchema::required("source", "light_table_source", 1),
    InkScriptFieldSchema::required("properties", "light_table_item_properties", 2),
];

pub(crate) const LIGHT_TABLE_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("light_table_source", LIGHT_TABLE_SOURCE_FIELDS),
    InkScriptRecordSchema::new("light_table_item_properties", LIGHT_TABLE_PROPERTIES_FIELDS),
    InkScriptRecordSchema::new("light_table_item_input", LIGHT_TABLE_ITEM_INPUT_FIELDS),
];

const GLOBAL_OPACITY_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("opacity_milli", "u32", 0)];
const CREATE_SET_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("name", "string", 0)];
const SET_ID_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "set_id",
    "light_table_set_ref",
    0,
)];
const RENAME_SET_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("set_id", "light_table_set_ref", 0),
    InkScriptFieldSchema::required("name", "string", 1),
];
const REORDER_SET_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("set_id", "light_table_set_ref", 0),
    InkScriptFieldSchema::required("destination_index", "u64", 1),
];
const ADD_ITEM_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "input",
    "light_table_item_input",
    0,
)];
const UPDATE_ITEM_PROPERTIES_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("item_id", "light_table_item_ref", 0),
    InkScriptFieldSchema::required("properties", "light_table_item_properties", 1),
];
const UPDATE_ITEM_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("item_id", "light_table_item_ref", 0),
    InkScriptFieldSchema::required("input", "light_table_item_input", 1),
];
const ITEM_ID_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "item_id",
    "light_table_item_ref",
    0,
)];
const REORDER_ITEM_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("item_id", "light_table_item_ref", 0),
    InkScriptFieldSchema::required("destination_index", "u64", 1),
];
const BULK_REGISTER_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("target_set_id", "light_table_set_ref", 0),
    InkScriptFieldSchema::required("inputs", "list<light_table_item_input>", 1),
];

const SET_RESULT: &[InkScriptCommandResultSchema] = &[InkScriptCommandResultSchema::scalar(
    "set",
    "light_table_set_ref",
    InkScriptResultAvailability::AlwaysOnSuccess,
    0,
)];
const ITEM_RESULT: &[InkScriptCommandResultSchema] = &[InkScriptCommandResultSchema::scalar(
    "item",
    "light_table_item_ref",
    InkScriptResultAvailability::AlwaysOnSuccess,
    0,
)];
const ITEMS_RESULT: &[InkScriptCommandResultSchema] =
    &[InkScriptCommandResultSchema::ordered_list(
        "items",
        "light_table_item_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    )];

pub(crate) const LIGHT_TABLE_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::new("light_table_set_global_opacity", GLOBAL_OPACITY_FIELDS),
    InkScriptCommandSchema::with_results("light_table_create_set", CREATE_SET_FIELDS, SET_RESULT),
    InkScriptCommandSchema::with_results("light_table_duplicate_set", SET_ID_FIELDS, SET_RESULT),
    InkScriptCommandSchema::new("light_table_delete_set", SET_ID_FIELDS),
    InkScriptCommandSchema::new("light_table_rename_set", RENAME_SET_FIELDS),
    InkScriptCommandSchema::new("light_table_reorder_set", REORDER_SET_FIELDS),
    InkScriptCommandSchema::new("light_table_set_active", SET_ID_FIELDS),
    InkScriptCommandSchema::with_results("light_table_add_item", ADD_ITEM_FIELDS, ITEM_RESULT),
    InkScriptCommandSchema::new(
        "light_table_update_item_properties",
        UPDATE_ITEM_PROPERTIES_FIELDS,
    ),
    InkScriptCommandSchema::new("light_table_update_item", UPDATE_ITEM_FIELDS),
    InkScriptCommandSchema::new("light_table_remove_item", ITEM_ID_FIELDS),
    InkScriptCommandSchema::new("light_table_reorder_item", REORDER_ITEM_FIELDS),
    InkScriptCommandSchema::with_results(
        "light_table_bulk_register",
        BULK_REGISTER_FIELDS,
        ITEMS_RESULT,
    ),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LightTableSourceSpec {
    document_uuid: u128,
    source_revision: u64,
    reference_frame: RectI32,
    dpi_x_milli: u32,
    dpi_y_milli: u32,
    asset_symbol: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LightTableItemSpec {
    name: String,
    source: LightTableSourceSpec,
    properties: LightTableItemProperties,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LightTableScriptAction {
    Canonical(CanonicalInvocation),
    AddItem(LightTableItemSpec),
    UpdateItem {
        item_id: u64,
        input: LightTableItemSpec,
    },
    BulkRegister {
        target_set_id: u64,
        inputs: Vec<LightTableItemSpec>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LightTableAdapterError {
    InvalidTypedStep,
    InvalidValue,
    MissingReference,
    ResourceLimit,
    UnsupportedPrimitive,
}

impl LightTableScriptAction {
    pub(crate) fn from_compiled(
        step: &InkScriptTypedStep,
        arguments: &InkScriptTypedValue,
        bindings: &InkScriptRuntimeReferences,
    ) -> Result<Self, LightTableAdapterError> {
        let fields = record(arguments)?;
        Ok(match step.command() {
            "light_table_set_global_opacity" => {
                let opacity_milli = u32_value(field(fields, "opacity_milli")?)?;
                if opacity_milli > 1_000 {
                    return Err(LightTableAdapterError::InvalidValue);
                }
                Self::Canonical(CanonicalInvocation::LightTableSetGlobalOpacity { opacity_milli })
            }
            "light_table_create_set" => Self::Canonical(CanonicalInvocation::LightTableCreateSet {
                name: node_name(field(fields, "name")?)?,
            }),
            "light_table_duplicate_set" => {
                Self::Canonical(CanonicalInvocation::LightTableDuplicateSet {
                    set_id: reference(
                        field(fields, "set_id")?,
                        bindings,
                        InkScriptEntityKind::LightTableSet,
                    )?,
                })
            }
            "light_table_delete_set" => Self::Canonical(CanonicalInvocation::LightTableDeleteSet {
                set_id: reference(
                    field(fields, "set_id")?,
                    bindings,
                    InkScriptEntityKind::LightTableSet,
                )?,
            }),
            "light_table_rename_set" => Self::Canonical(CanonicalInvocation::LightTableRenameSet {
                set_id: reference(
                    field(fields, "set_id")?,
                    bindings,
                    InkScriptEntityKind::LightTableSet,
                )?,
                name: node_name(field(fields, "name")?)?,
            }),
            "light_table_reorder_set" => {
                Self::Canonical(CanonicalInvocation::LightTableReorderSet {
                    set_id: reference(
                        field(fields, "set_id")?,
                        bindings,
                        InkScriptEntityKind::LightTableSet,
                    )?,
                    destination_index: u64_value(field(fields, "destination_index")?)?,
                })
            }
            "light_table_set_active" => Self::Canonical(CanonicalInvocation::LightTableSetActive {
                set_id: reference(
                    field(fields, "set_id")?,
                    bindings,
                    InkScriptEntityKind::LightTableSet,
                )?,
            }),
            "light_table_add_item" => Self::AddItem(item_spec(field(fields, "input")?)?),
            "light_table_update_item_properties" => {
                Self::Canonical(CanonicalInvocation::LightTableUpdateItemProperties {
                    item_id: reference(
                        field(fields, "item_id")?,
                        bindings,
                        InkScriptEntityKind::LightTableItem,
                    )?,
                    properties: item_properties(field(fields, "properties")?)?,
                })
            }
            "light_table_update_item" => Self::UpdateItem {
                item_id: reference(
                    field(fields, "item_id")?,
                    bindings,
                    InkScriptEntityKind::LightTableItem,
                )?,
                input: item_spec(field(fields, "input")?)?,
            },
            "light_table_remove_item" => {
                Self::Canonical(CanonicalInvocation::LightTableRemoveItem {
                    item_id: reference(
                        field(fields, "item_id")?,
                        bindings,
                        InkScriptEntityKind::LightTableItem,
                    )?,
                })
            }
            "light_table_reorder_item" => {
                Self::Canonical(CanonicalInvocation::LightTableReorderItem {
                    item_id: reference(
                        field(fields, "item_id")?,
                        bindings,
                        InkScriptEntityKind::LightTableItem,
                    )?,
                    destination_index: u64_value(field(fields, "destination_index")?)?,
                })
            }
            "light_table_bulk_register" => Self::BulkRegister {
                target_set_id: reference(
                    field(fields, "target_set_id")?,
                    bindings,
                    InkScriptEntityKind::LightTableSet,
                )?,
                inputs: item_specs(field(fields, "inputs")?)?,
            },
            _ => return Err(LightTableAdapterError::UnsupportedPrimitive),
        })
    }

    pub(crate) fn asset_symbols(&self) -> Vec<&str> {
        match self {
            Self::Canonical(_) => Vec::new(),
            Self::AddItem(input) | Self::UpdateItem { input, .. } => {
                vec![input.source.asset_symbol.as_str()]
            }
            Self::BulkRegister { inputs, .. } => inputs
                .iter()
                .map(|input| input.source.asset_symbol.as_str())
                .collect(),
        }
    }

    pub(crate) fn to_canonical_with_assets(
        &self,
        assets: &[Arc<AssetRecord>],
    ) -> Result<CanonicalInvocation, LightTableAdapterError> {
        match self {
            Self::Canonical(invocation) if assets.is_empty() => Ok(invocation.clone()),
            Self::Canonical(_) => Err(LightTableAdapterError::InvalidValue),
            Self::AddItem(input) if assets.len() == 1 => {
                Ok(CanonicalInvocation::LightTableAddItem {
                    input: build_item(input, Arc::clone(&assets[0]))?,
                })
            }
            Self::UpdateItem { item_id, input } if assets.len() == 1 => {
                Ok(CanonicalInvocation::LightTableUpdateItem {
                    item_id: *item_id,
                    input: build_item(input, Arc::clone(&assets[0]))?,
                })
            }
            Self::BulkRegister {
                target_set_id,
                inputs,
            } if inputs.len() == assets.len() => {
                let mut built = Vec::new();
                built
                    .try_reserve_exact(inputs.len())
                    .map_err(|_| LightTableAdapterError::ResourceLimit)?;
                for (input, asset) in inputs.iter().zip(assets) {
                    built.push(build_item(input, Arc::clone(asset))?);
                }
                Ok(CanonicalInvocation::LightTableBulkRegister {
                    target_set_id: *target_set_id,
                    inputs: built,
                })
            }
            Self::AddItem(_) | Self::UpdateItem { .. } | Self::BulkRegister { .. } => {
                Err(LightTableAdapterError::InvalidValue)
            }
        }
    }

    pub(crate) fn output_entity_kinds(
        &self,
        output_count: usize,
    ) -> Result<Vec<InkScriptEntityKind>, LightTableAdapterError> {
        match self {
            Self::Canonical(
                CanonicalInvocation::LightTableCreateSet { .. }
                | CanonicalInvocation::LightTableDuplicateSet { .. },
            ) if output_count == 1 => Ok(vec![InkScriptEntityKind::LightTableSet]),
            Self::AddItem(_) if output_count == 1 => Ok(vec![InkScriptEntityKind::LightTableItem]),
            Self::BulkRegister { inputs, .. }
                if output_count == inputs.len() && output_count <= MAX_LIGHT_TABLE_ITEMS =>
            {
                Ok(vec![InkScriptEntityKind::LightTableItem; output_count])
            }
            Self::UpdateItem { .. } if output_count == 0 => Ok(Vec::new()),
            Self::Canonical(_) if output_count == 0 => Ok(Vec::new()),
            _ => Err(LightTableAdapterError::InvalidValue),
        }
    }
}

fn item_specs(
    value: &InkScriptTypedValue,
) -> Result<Vec<LightTableItemSpec>, LightTableAdapterError> {
    let values = list(value)?;
    if values.is_empty() || values.len() > MAX_LIGHT_TABLE_ITEMS {
        return Err(LightTableAdapterError::ResourceLimit);
    }
    let mut inputs = Vec::new();
    inputs
        .try_reserve_exact(values.len())
        .map_err(|_| LightTableAdapterError::ResourceLimit)?;
    for value in values {
        inputs.push(item_spec(value)?);
    }
    Ok(inputs)
}

fn item_spec(value: &InkScriptTypedValue) -> Result<LightTableItemSpec, LightTableAdapterError> {
    let fields = record(value)?;
    let source_fields = record(field(fields, "source")?)?;
    let reference_frame = rectangle(field(source_fields, "reference_frame")?)?;
    if reference_frame.width <= 0 || reference_frame.height <= 0 {
        return Err(LightTableAdapterError::InvalidValue);
    }
    let document_uuid = uuid(field(source_fields, "document_uuid")?)?;
    let source_revision = u64_value(field(source_fields, "source_revision")?)?;
    let dpi_x_milli = u32_value(field(source_fields, "dpi_x_milli")?)?;
    let dpi_y_milli = u32_value(field(source_fields, "dpi_y_milli")?)?;
    if document_uuid == 0 || source_revision == 0 || dpi_x_milli == 0 || dpi_y_milli == 0 {
        return Err(LightTableAdapterError::InvalidValue);
    }
    Ok(LightTableItemSpec {
        name: node_name(field(fields, "name")?)?,
        source: LightTableSourceSpec {
            document_uuid,
            source_revision,
            reference_frame,
            dpi_x_milli,
            dpi_y_milli,
            asset_symbol: asset_reference(field(source_fields, "raster")?)?.to_owned(),
        },
        properties: item_properties(field(fields, "properties")?)?,
    })
}

fn item_properties(
    value: &InkScriptTypedValue,
) -> Result<LightTableItemProperties, LightTableAdapterError> {
    let fields = record(value)?;
    let properties = LightTableItemProperties {
        visible: boolean(field(fields, "visible")?)?,
        opacity_milli: u32_value(field(fields, "opacity_milli")?)?,
        display_mode: match enum_value(field(fields, "display_mode")?)? {
            "color" => LightTableDisplayMode::Color,
            "monotone" => LightTableDisplayMode::Monotone,
            "halftone" => LightTableDisplayMode::Halftone,
            _ => return Err(LightTableAdapterError::InvalidValue),
        },
        display_color: rgba_pixel(field(fields, "display_color")?)?,
        translate_x_milli: i32_value(field(fields, "translate_x_milli")?)?,
        translate_y_milli: i32_value(field(fields, "translate_y_milli")?)?,
        scale_x_milli: u32_value(field(fields, "scale_x_milli")?)?,
        scale_y_milli: u32_value(field(fields, "scale_y_milli")?)?,
        rotation_milli_degrees: i32_value(field(fields, "rotation_milli_degrees")?)?,
    };
    if properties.opacity_milli > 1_000
        || !(1..=64_000).contains(&properties.scale_x_milli)
        || !(1..=64_000).contains(&properties.scale_y_milli)
        || properties.rotation_milli_degrees.unsigned_abs() > 360_000
    {
        return Err(LightTableAdapterError::InvalidValue);
    }
    Ok(properties)
}

fn build_item(
    input: &LightTableItemSpec,
    asset: Arc<AssetRecord>,
) -> Result<LightTableItemInput, LightTableAdapterError> {
    let source = LightTableSource::from_record(
        input.source.document_uuid,
        input.source.source_revision,
        input.source.reference_frame,
        input.source.dpi_x_milli,
        input.source.dpi_y_milli,
        asset,
    )
    .map_err(|_| LightTableAdapterError::InvalidValue)?;
    if !matches!(
        source.pixel_format(),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
    ) {
        return Err(LightTableAdapterError::InvalidValue);
    }
    Ok(LightTableItemInput {
        name: input.name.clone(),
        source,
        visible: input.properties.visible,
        opacity_milli: input.properties.opacity_milli,
        display_mode: input.properties.display_mode,
        display_color: input.properties.display_color,
        translate_x_milli: input.properties.translate_x_milli,
        translate_y_milli: input.properties.translate_y_milli,
        scale_x_milli: input.properties.scale_x_milli,
        scale_y_milli: input.properties.scale_y_milli,
        rotation_milli_degrees: input.properties.rotation_milli_degrees,
    })
}

fn reference(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
    kind: InkScriptEntityKind,
) -> Result<u64, LightTableAdapterError> {
    bindings.resolve(value, kind).map_err(reference_error)
}

fn reference_error(error: InkScriptReferenceError) -> LightTableAdapterError {
    match error {
        InkScriptReferenceError::MissingReference => LightTableAdapterError::MissingReference,
        InkScriptReferenceError::InvalidReference | InkScriptReferenceError::KindMismatch => {
            LightTableAdapterError::InvalidValue
        }
    }
}

fn rgba_pixel(value: &InkScriptTypedValue) -> Result<PixelValue, LightTableAdapterError> {
    let pixel = inkscript_batch::pixel(value).map_err(|error| match error {
        inkscript_batch::LegacyImageAdapterError::MissingBinding => {
            LightTableAdapterError::MissingReference
        }
        inkscript_batch::LegacyImageAdapterError::ResourceLimit => {
            LightTableAdapterError::ResourceLimit
        }
        _ => LightTableAdapterError::InvalidValue,
    })?;
    pixel
        .rgba16()
        .is_some()
        .then_some(pixel)
        .ok_or(LightTableAdapterError::InvalidValue)
}

fn node_name(value: &InkScriptTypedValue) -> Result<String, LightTableAdapterError> {
    let value = string(value)?;
    if value.is_empty() || value.len() > MAX_NODE_NAME_BYTES || value.chars().any(char::is_control)
    {
        return Err(LightTableAdapterError::InvalidValue);
    }
    Ok(value.to_owned())
}

fn rectangle(value: &InkScriptTypedValue) -> Result<RectI32, LightTableAdapterError> {
    let values = constructor(value, "rect")?;
    if values.len() != 4 {
        return Err(LightTableAdapterError::InvalidValue);
    }
    Ok(RectI32 {
        x: i32_value(&values[0])?,
        y: i32_value(&values[1])?,
        width: i32::try_from(u32_value(&values[2])?)
            .map_err(|_| LightTableAdapterError::InvalidValue)?,
        height: i32::try_from(u32_value(&values[3])?)
            .map_err(|_| LightTableAdapterError::InvalidValue)?,
    })
}

fn uuid(value: &InkScriptTypedValue) -> Result<u128, LightTableAdapterError> {
    let InkScriptTypedValueKind::Uuid(value) = value.kind() else {
        return Err(LightTableAdapterError::InvalidValue);
    };
    let compact = value.replace('-', "");
    if compact.len() != 32 {
        return Err(LightTableAdapterError::InvalidValue);
    }
    u128::from_str_radix(&compact, 16).map_err(|_| LightTableAdapterError::InvalidValue)
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(LightTableAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, LightTableAdapterError> {
    fields
        .get(name)
        .ok_or(LightTableAdapterError::InvalidTypedStep)
}

fn list(value: &InkScriptTypedValue) -> Result<&[InkScriptTypedValue], LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::List(values) => Ok(values),
        _ => Err(LightTableAdapterError::InvalidTypedStep),
    }
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected: &str,
) -> Result<&'a [InkScriptTypedValue], LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments } if name == expected => {
            Ok(arguments)
        }
        _ => Err(LightTableAdapterError::InvalidValue),
    }
}

fn asset_reference(value: &InkScriptTypedValue) -> Result<&str, LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::AssetReference(value) => Ok(value),
        _ => Err(LightTableAdapterError::InvalidValue),
    }
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(LightTableAdapterError::InvalidValue),
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(LightTableAdapterError::InvalidValue),
    }
}

fn string(value: &InkScriptTypedValue) -> Result<&str, LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::String(value) => Ok(value),
        _ => Err(LightTableAdapterError::InvalidValue),
    }
}

fn u32_value(value: &InkScriptTypedValue) -> Result<u32, LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(LightTableAdapterError::InvalidValue),
    }
}

fn i32_value(value: &InkScriptTypedValue) -> Result<i32, LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::I32(value) => Ok(*value),
        _ => Err(LightTableAdapterError::InvalidValue),
    }
}

fn u64_value(value: &InkScriptTypedValue) -> Result<u64, LightTableAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U64(value) => Ok(*value),
        _ => Err(LightTableAdapterError::InvalidValue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn light_table_adapter_is_core_owned_and_thread_suitable() {
        assert_send_sync::<LightTableScriptAction>();
        assert_send_sync::<LightTableAdapterError>();
    }
}
