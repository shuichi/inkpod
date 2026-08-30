//! Private pre-ratification InkScript adapter for shooting-frame primitives.

use super::CanonicalInvocation;
use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use crate::{ShootingFrameAnchor, ShootingFrameEdit, ShootingFrameInput};
use inkpod_format::{
    InkScriptCommandResultSchema, InkScriptCommandSchema, InkScriptEnumSchema,
    InkScriptFieldSchema, InkScriptRecordSchema, InkScriptResultAvailability, InkScriptTypedStep,
    InkScriptTypedValue, InkScriptTypedValueKind,
};
use std::collections::BTreeMap;

pub(crate) const FRAME_ENUMS: &[InkScriptEnumSchema] = &[InkScriptEnumSchema::new(
    "shooting_frame_anchor",
    &[
        "top_left",
        "top_right",
        "center",
        "bottom_left",
        "bottom_right",
    ],
)];

const SHOOTING_FRAME_INPUT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("center_x_milli", "i64", 0),
    InkScriptFieldSchema::required("center_y_milli", "i64", 1),
    InkScriptFieldSchema::required("width_milli", "u64", 2),
    InkScriptFieldSchema::required("height_milli", "u64", 3),
    InkScriptFieldSchema::required("rotation_turns", "u32", 4),
    InkScriptFieldSchema::required("anchor", "shooting_frame_anchor", 5),
    InkScriptFieldSchema::required("visible", "bool", 6),
    InkScriptFieldSchema::required("include_in_instruction_export", "bool", 7),
];
const SHOOTING_FRAME_EDIT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("operation", "u32", 0),
    InkScriptFieldSchema::required("frame_id", "nullable<shooting_frame_ref>", 1),
    InkScriptFieldSchema::required("input", "nullable<shooting_frame_input>", 2),
];
pub(crate) const FRAME_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("shooting_frame_input", SHOOTING_FRAME_INPUT_FIELDS),
    InkScriptRecordSchema::new("shooting_frame_edit", SHOOTING_FRAME_EDIT_FIELDS),
];

const EDIT_SHOOTING_FRAME_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "edit",
    "shooting_frame_edit",
    0,
)];
const SHOOTING_FRAME_RESULTS: &[InkScriptCommandResultSchema] =
    &[InkScriptCommandResultSchema::ordered_list(
        "shooting_frames",
        "shooting_frame_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    )];
pub(crate) const FRAME_COMMANDS: &[InkScriptCommandSchema] =
    &[InkScriptCommandSchema::with_results(
        "edit_shooting_frame",
        EDIT_SHOOTING_FRAME_FIELDS,
        SHOOTING_FRAME_RESULTS,
    )];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FrameScriptStep {
    invocation: CanonicalInvocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameAdapterError {
    InvalidTypedStep,
    InvalidValue,
    MissingReference,
    ResourceLimit,
    UnsupportedPrimitive,
}

impl FrameScriptStep {
    pub(crate) fn from_compiled(
        step: &InkScriptTypedStep,
        arguments: &InkScriptTypedValue,
        bindings: &InkScriptRuntimeReferences,
    ) -> Result<Self, FrameAdapterError> {
        let fields = record(arguments)?;
        let invocation = match step.command() {
            "edit_shooting_frame" => CanonicalInvocation::EditShootingFrame {
                edit: shooting_frame_edit(field(fields, "edit")?, bindings)?,
            },
            _ => return Err(FrameAdapterError::UnsupportedPrimitive),
        };
        Ok(Self { invocation })
    }

    pub(crate) fn to_canonical(&self) -> CanonicalInvocation {
        self.invocation.clone()
    }

    pub(crate) fn output_entity_kinds(
        &self,
        output_count: usize,
    ) -> Result<Vec<InkScriptEntityKind>, FrameAdapterError> {
        let (maximum, kind) = match self.invocation {
            CanonicalInvocation::EditShootingFrame { .. } => {
                (1, InkScriptEntityKind::ShootingFrame)
            }
            _ => return Err(FrameAdapterError::UnsupportedPrimitive),
        };
        if output_count > maximum {
            return Err(FrameAdapterError::ResourceLimit);
        }
        Ok(vec![kind; output_count])
    }
}

fn shooting_frame_edit(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<ShootingFrameEdit, FrameAdapterError> {
    let fields = record(value)?;
    let operation = u32_value(field(fields, "operation")?)?;
    let frame_id = nullable_reference(
        field(fields, "frame_id")?,
        bindings,
        InkScriptEntityKind::ShootingFrame,
    )?;
    let input = nullable(field(fields, "input")?, shooting_frame_input)?;
    match (operation, frame_id, input) {
        (1, None, Some(input)) => Ok(ShootingFrameEdit::Create(input)),
        (2, Some(frame_id), Some(input)) => Ok(ShootingFrameEdit::Update { frame_id, input }),
        (3, Some(frame_id), None) => Ok(ShootingFrameEdit::Delete { frame_id }),
        _ => Err(FrameAdapterError::InvalidValue),
    }
}

fn shooting_frame_input(
    value: &InkScriptTypedValue,
) -> Result<ShootingFrameInput, FrameAdapterError> {
    let fields = record(value)?;
    Ok(ShootingFrameInput {
        center_x_milli: i64_value(field(fields, "center_x_milli")?)?,
        center_y_milli: i64_value(field(fields, "center_y_milli")?)?,
        width_milli: u64_value(field(fields, "width_milli")?)?,
        height_milli: u64_value(field(fields, "height_milli")?)?,
        rotation_turns: u32_value(field(fields, "rotation_turns")?)?,
        anchor: match enum_value(field(fields, "anchor")?)? {
            "top_left" => ShootingFrameAnchor::TopLeft,
            "top_right" => ShootingFrameAnchor::TopRight,
            "center" => ShootingFrameAnchor::Center,
            "bottom_left" => ShootingFrameAnchor::BottomLeft,
            "bottom_right" => ShootingFrameAnchor::BottomRight,
            _ => return Err(FrameAdapterError::InvalidValue),
        },
        visible: boolean(field(fields, "visible")?)?,
        include_in_instruction_export: boolean(field(fields, "include_in_instruction_export")?)?,
    })
}

fn nullable_reference(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
    kind: InkScriptEntityKind,
) -> Result<Option<u64>, FrameAdapterError> {
    nullable(value, |value| reference(value, bindings, kind))
}

fn reference(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
    kind: InkScriptEntityKind,
) -> Result<u64, FrameAdapterError> {
    bindings.resolve(value, kind).map_err(reference_error)
}

fn reference_error(error: InkScriptReferenceError) -> FrameAdapterError {
    match error {
        InkScriptReferenceError::MissingReference => FrameAdapterError::MissingReference,
        InkScriptReferenceError::InvalidReference | InkScriptReferenceError::KindMismatch => {
            FrameAdapterError::InvalidValue
        }
    }
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, FrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(FrameAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, FrameAdapterError> {
    fields.get(name).ok_or(FrameAdapterError::InvalidTypedStep)
}

fn nullable<T>(
    value: &InkScriptTypedValue,
    parse: impl FnOnce(&InkScriptTypedValue) -> Result<T, FrameAdapterError>,
) -> Result<Option<T>, FrameAdapterError> {
    if matches!(value.kind(), InkScriptTypedValueKind::None) {
        Ok(None)
    } else {
        parse(value).map(Some)
    }
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected: &str,
) -> Result<&'a [InkScriptTypedValue], FrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments } if name == expected => {
            Ok(arguments)
        }
        _ => Err(FrameAdapterError::InvalidValue),
    }
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, FrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(FrameAdapterError::InvalidValue),
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, FrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(FrameAdapterError::InvalidValue),
    }
}

fn u32_value(value: &InkScriptTypedValue) -> Result<u32, FrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(FrameAdapterError::InvalidValue),
    }
}

fn u64_value(value: &InkScriptTypedValue) -> Result<u64, FrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U64(value) => Ok(*value),
        _ => Err(FrameAdapterError::InvalidValue),
    }
}

fn i64_value(value: &InkScriptTypedValue) -> Result<i64, FrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::I64(value) => Ok(*value),
        _ => Err(FrameAdapterError::InvalidValue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn frame_adapter_is_core_owned_and_thread_suitable() {
        assert_send_sync::<FrameScriptStep>();
        assert_send_sync::<FrameAdapterError>();
    }
}
