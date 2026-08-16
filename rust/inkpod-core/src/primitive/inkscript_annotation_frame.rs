//! Private pre-ratification InkScript adapter for annotation, shooting-frame, and vanishing-point primitives.

use super::CanonicalInvocation;
use super::inkscript_batch;
use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use crate::{
    AnnotationEdit, AnnotationKind, AnnotationObjectInput, AnnotationOutput, AnnotationPoint,
    MAX_ANNOTATION_BATCH_EDITS, MAX_ANNOTATION_OBJECTS, MAX_ANNOTATION_POINTS,
    MAX_VANISHING_POINT_EDITS, MAX_VANISHING_POINTS, PixelValue, RectI32, ShootingFrameAnchor,
    ShootingFrameEdit, ShootingFrameInput, VanishingPointEdit, VanishingPointInput,
};
use inkpod_format::{
    InkScriptCommandResultSchema, InkScriptCommandSchema, InkScriptEnumSchema,
    InkScriptFieldSchema, InkScriptRecordSchema, InkScriptResultAvailability, InkScriptTypedStep,
    InkScriptTypedValue, InkScriptTypedValueKind,
};
use std::collections::BTreeMap;

pub(crate) const ANNOTATION_FRAME_ENUMS: &[InkScriptEnumSchema] = &[
    InkScriptEnumSchema::new("annotation_output", &["normal", "instruction"]),
    InkScriptEnumSchema::new(
        "shooting_frame_anchor",
        &[
            "top_left",
            "top_right",
            "center",
            "bottom_left",
            "bottom_right",
        ],
    ),
];

const ANNOTATION_POINT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("x_milli", "i32", 0),
    InkScriptFieldSchema::required("y_milli", "i32", 1),
];
const ANNOTATION_INPUT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("layer_id", "layer_ref", 0),
    InkScriptFieldSchema::required("kind", "annotation_kind", 1),
    InkScriptFieldSchema::required("output", "annotation_output", 2),
    InkScriptFieldSchema::required("bounds", "pixel_rect", 3),
    InkScriptFieldSchema::required("font_family_hint", "string", 4),
    InkScriptFieldSchema::required("font_size_milli", "u32", 5),
    InkScriptFieldSchema::required("style_flags", "u32", 6),
    InkScriptFieldSchema::required("color", "pixel_value", 7),
    InkScriptFieldSchema::required("text", "string", 8),
    InkScriptFieldSchema::required("points", "list<annotation_point_milli>", 9),
    InkScriptFieldSchema::required("stroke_width_milli", "u32", 10),
];
const ANNOTATION_EDIT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("operation", "u32", 0),
    InkScriptFieldSchema::required("object_id", "nullable<annotation_ref>", 1),
    InkScriptFieldSchema::required("input", "nullable<annotation_object_input>", 2),
    InkScriptFieldSchema::required("delta_x", "i32", 3),
    InkScriptFieldSchema::required("delta_y", "i32", 4),
];
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
const VANISHING_POINT_INPUT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("layer_id", "layer_ref", 0),
    InkScriptFieldSchema::required("x_milli", "i64", 1),
    InkScriptFieldSchema::required("y_milli", "i64", 2),
    InkScriptFieldSchema::required("interval_milli_degrees", "u32", 3),
    InkScriptFieldSchema::required("angle_milli_degrees", "u32", 4),
    InkScriptFieldSchema::required("color", "pixel_value", 5),
    InkScriptFieldSchema::required("opacity_milli", "u32", 6),
    InkScriptFieldSchema::required("visible", "bool", 7),
];
const VANISHING_POINT_EDIT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("operation", "u32", 0),
    InkScriptFieldSchema::required("point_id", "nullable<vanishing_point_ref>", 1),
    InkScriptFieldSchema::required("input", "nullable<vanishing_point_input>", 2),
];

pub(crate) const ANNOTATION_FRAME_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("annotation_point_milli", ANNOTATION_POINT_FIELDS),
    InkScriptRecordSchema::new("annotation_object_input", ANNOTATION_INPUT_FIELDS),
    InkScriptRecordSchema::new("annotation_edit", ANNOTATION_EDIT_FIELDS),
    InkScriptRecordSchema::new("shooting_frame_input", SHOOTING_FRAME_INPUT_FIELDS),
    InkScriptRecordSchema::new("shooting_frame_edit", SHOOTING_FRAME_EDIT_FIELDS),
    InkScriptRecordSchema::new("vanishing_point_input", VANISHING_POINT_INPUT_FIELDS),
    InkScriptRecordSchema::new("vanishing_point_edit", VANISHING_POINT_EDIT_FIELDS),
];

const EDIT_ANNOTATION_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "edits",
    "list<annotation_edit>",
    0,
)];
const EDIT_SHOOTING_FRAME_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "edit",
    "shooting_frame_edit",
    0,
)];
const EDIT_VANISHING_POINT_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "edits",
    "list<vanishing_point_edit>",
    0,
)];

const ANNOTATION_RESULTS: &[InkScriptCommandResultSchema] =
    &[InkScriptCommandResultSchema::ordered_list(
        "annotations",
        "annotation_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    )];
const SHOOTING_FRAME_RESULTS: &[InkScriptCommandResultSchema] =
    &[InkScriptCommandResultSchema::ordered_list(
        "shooting_frames",
        "shooting_frame_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    )];
const VANISHING_POINT_RESULTS: &[InkScriptCommandResultSchema] =
    &[InkScriptCommandResultSchema::ordered_list(
        "vanishing_points",
        "vanishing_point_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    )];

pub(crate) const ANNOTATION_FRAME_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::with_results(
        "edit_annotations",
        EDIT_ANNOTATION_FIELDS,
        ANNOTATION_RESULTS,
    ),
    InkScriptCommandSchema::with_results(
        "edit_shooting_frame",
        EDIT_SHOOTING_FRAME_FIELDS,
        SHOOTING_FRAME_RESULTS,
    ),
    InkScriptCommandSchema::with_results(
        "edit_vanishing_points",
        EDIT_VANISHING_POINT_FIELDS,
        VANISHING_POINT_RESULTS,
    ),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnnotationFrameScriptStep {
    invocation: CanonicalInvocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AnnotationFrameAdapterError {
    InvalidTypedStep,
    InvalidValue,
    MissingReference,
    ResourceLimit,
    UnsupportedPrimitive,
}

impl AnnotationFrameScriptStep {
    pub(crate) fn from_compiled(
        step: &InkScriptTypedStep,
        arguments: &InkScriptTypedValue,
        bindings: &InkScriptRuntimeReferences,
    ) -> Result<Self, AnnotationFrameAdapterError> {
        let fields = record(arguments)?;
        let invocation = match step.command() {
            "edit_annotations" => CanonicalInvocation::EditAnnotations {
                edits: annotation_edits(field(fields, "edits")?, bindings)?,
            },
            "edit_shooting_frame" => CanonicalInvocation::EditShootingFrame {
                edit: shooting_frame_edit(field(fields, "edit")?, bindings)?,
            },
            "edit_vanishing_points" => CanonicalInvocation::EditVanishingPoints {
                edits: vanishing_point_edits(field(fields, "edits")?, bindings)?,
            },
            _ => return Err(AnnotationFrameAdapterError::UnsupportedPrimitive),
        };
        Ok(Self { invocation })
    }

    pub(crate) fn to_canonical(&self) -> CanonicalInvocation {
        self.invocation.clone()
    }

    pub(crate) fn output_entity_kinds(
        &self,
        output_count: usize,
    ) -> Result<Vec<InkScriptEntityKind>, AnnotationFrameAdapterError> {
        let (maximum, kind) = match self.invocation {
            CanonicalInvocation::EditAnnotations { .. } => {
                (MAX_ANNOTATION_OBJECTS, InkScriptEntityKind::Annotation)
            }
            CanonicalInvocation::EditShootingFrame { .. } => {
                (1, InkScriptEntityKind::ShootingFrame)
            }
            CanonicalInvocation::EditVanishingPoints { .. } => {
                (MAX_VANISHING_POINTS, InkScriptEntityKind::VanishingPoint)
            }
            _ => return Err(AnnotationFrameAdapterError::UnsupportedPrimitive),
        };
        if output_count > maximum {
            return Err(AnnotationFrameAdapterError::ResourceLimit);
        }
        Ok(vec![kind; output_count])
    }
}

fn annotation_edits(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<Vec<AnnotationEdit>, AnnotationFrameAdapterError> {
    let values = bounded_list(value, 1, MAX_ANNOTATION_BATCH_EDITS)?;
    let mut edits = Vec::new();
    edits
        .try_reserve_exact(values.len())
        .map_err(|_| AnnotationFrameAdapterError::ResourceLimit)?;
    for value in values {
        let fields = record(value)?;
        let operation = u32_value(field(fields, "operation")?)?;
        let object_id = nullable_reference(
            field(fields, "object_id")?,
            bindings,
            InkScriptEntityKind::Annotation,
        )?;
        let input = nullable(field(fields, "input")?, |value| {
            annotation_input(value, bindings)
        })?;
        let delta_x = i32_value(field(fields, "delta_x")?)?;
        let delta_y = i32_value(field(fields, "delta_y")?)?;
        edits.push(match (operation, object_id, input, delta_x, delta_y) {
            (1, None, Some(input), 0, 0) => AnnotationEdit::Create(input),
            (2, Some(object_id), Some(input), 0, 0) => AnnotationEdit::Update { object_id, input },
            (3, Some(object_id), None, delta_x, delta_y) => AnnotationEdit::Move {
                object_id,
                delta_x,
                delta_y,
            },
            (4, Some(object_id), None, 0, 0) => AnnotationEdit::Delete { object_id },
            _ => return Err(AnnotationFrameAdapterError::InvalidValue),
        });
    }
    Ok(edits)
}

fn annotation_input(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<AnnotationObjectInput, AnnotationFrameAdapterError> {
    let fields = record(value)?;
    let point_values = bounded_list(field(fields, "points")?, 0, MAX_ANNOTATION_POINTS)?;
    let mut points = Vec::new();
    points
        .try_reserve_exact(point_values.len())
        .map_err(|_| AnnotationFrameAdapterError::ResourceLimit)?;
    for point in point_values {
        let fields = record(point)?;
        points.push(AnnotationPoint {
            x_milli: i32_value(field(fields, "x_milli")?)?,
            y_milli: i32_value(field(fields, "y_milli")?)?,
        });
    }
    Ok(AnnotationObjectInput {
        layer_id: reference(
            field(fields, "layer_id")?,
            bindings,
            InkScriptEntityKind::Layer,
        )?,
        kind: match enum_value(field(fields, "kind")?)? {
            "text" => AnnotationKind::Text,
            "stroke" => AnnotationKind::Stroke,
            "leader" => AnnotationKind::Leader,
            "value" => AnnotationKind::Value,
            _ => return Err(AnnotationFrameAdapterError::InvalidValue),
        },
        output: match enum_value(field(fields, "output")?)? {
            "normal" => AnnotationOutput::Normal,
            "instruction" => AnnotationOutput::Instruction,
            _ => return Err(AnnotationFrameAdapterError::InvalidValue),
        },
        bounds: rect(field(fields, "bounds")?)?,
        font_family_hint: string(field(fields, "font_family_hint")?)?.to_owned(),
        font_size_milli: u32_value(field(fields, "font_size_milli")?)?,
        style_flags: u32_value(field(fields, "style_flags")?)?,
        color: rgba_pixel(field(fields, "color")?)?,
        text: string(field(fields, "text")?)?.to_owned(),
        points,
        stroke_width_milli: u32_value(field(fields, "stroke_width_milli")?)?,
    })
}

fn shooting_frame_edit(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<ShootingFrameEdit, AnnotationFrameAdapterError> {
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
        _ => Err(AnnotationFrameAdapterError::InvalidValue),
    }
}

fn shooting_frame_input(
    value: &InkScriptTypedValue,
) -> Result<ShootingFrameInput, AnnotationFrameAdapterError> {
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
            _ => return Err(AnnotationFrameAdapterError::InvalidValue),
        },
        visible: boolean(field(fields, "visible")?)?,
        include_in_instruction_export: boolean(field(fields, "include_in_instruction_export")?)?,
    })
}

fn vanishing_point_edits(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<Vec<VanishingPointEdit>, AnnotationFrameAdapterError> {
    let values = bounded_list(value, 1, MAX_VANISHING_POINT_EDITS)?;
    let mut edits = Vec::new();
    edits
        .try_reserve_exact(values.len())
        .map_err(|_| AnnotationFrameAdapterError::ResourceLimit)?;
    for value in values {
        let fields = record(value)?;
        let operation = u32_value(field(fields, "operation")?)?;
        let point_id = nullable_reference(
            field(fields, "point_id")?,
            bindings,
            InkScriptEntityKind::VanishingPoint,
        )?;
        let input = nullable(field(fields, "input")?, |value| {
            vanishing_point_input(value, bindings)
        })?;
        edits.push(match (operation, point_id, input) {
            (1, None, Some(input)) => VanishingPointEdit::Create(input),
            (2, Some(point_id), Some(input)) => VanishingPointEdit::Update { point_id, input },
            (3, Some(point_id), None) => VanishingPointEdit::Delete { point_id },
            (4, None, None) if values.len() == 1 => VanishingPointEdit::DeleteAll,
            _ => return Err(AnnotationFrameAdapterError::InvalidValue),
        });
    }
    Ok(edits)
}

fn vanishing_point_input(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<VanishingPointInput, AnnotationFrameAdapterError> {
    let fields = record(value)?;
    Ok(VanishingPointInput {
        layer_id: reference(
            field(fields, "layer_id")?,
            bindings,
            InkScriptEntityKind::Layer,
        )?,
        x_milli: i64_value(field(fields, "x_milli")?)?,
        y_milli: i64_value(field(fields, "y_milli")?)?,
        interval_milli_degrees: u32_value(field(fields, "interval_milli_degrees")?)?,
        angle_milli_degrees: u32_value(field(fields, "angle_milli_degrees")?)?,
        color: rgba_pixel(field(fields, "color")?)?,
        opacity_milli: u32_value(field(fields, "opacity_milli")?)?,
        visible: boolean(field(fields, "visible")?)?,
    })
}

fn rgba_pixel(value: &InkScriptTypedValue) -> Result<PixelValue, AnnotationFrameAdapterError> {
    let pixel = inkscript_batch::pixel(value).map_err(legacy_image_error)?;
    if pixel.rgba16().is_none() {
        return Err(AnnotationFrameAdapterError::InvalidValue);
    }
    Ok(pixel)
}

fn rect(value: &InkScriptTypedValue) -> Result<RectI32, AnnotationFrameAdapterError> {
    let values = constructor(value, "rect")?;
    if values.len() != 4 {
        return Err(AnnotationFrameAdapterError::InvalidValue);
    }
    Ok(RectI32 {
        x: i32_value(&values[0])?,
        y: i32_value(&values[1])?,
        width: i32::try_from(u32_value(&values[2])?)
            .map_err(|_| AnnotationFrameAdapterError::InvalidValue)?,
        height: i32::try_from(u32_value(&values[3])?)
            .map_err(|_| AnnotationFrameAdapterError::InvalidValue)?,
    })
}

fn nullable_reference(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
    kind: InkScriptEntityKind,
) -> Result<Option<u64>, AnnotationFrameAdapterError> {
    nullable(value, |value| reference(value, bindings, kind))
}

fn reference(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
    kind: InkScriptEntityKind,
) -> Result<u64, AnnotationFrameAdapterError> {
    bindings.resolve(value, kind).map_err(reference_error)
}

fn reference_error(error: InkScriptReferenceError) -> AnnotationFrameAdapterError {
    match error {
        InkScriptReferenceError::MissingReference => AnnotationFrameAdapterError::MissingReference,
        InkScriptReferenceError::InvalidReference | InkScriptReferenceError::KindMismatch => {
            AnnotationFrameAdapterError::InvalidValue
        }
    }
}

fn legacy_image_error(
    error: inkscript_batch::LegacyImageAdapterError,
) -> AnnotationFrameAdapterError {
    match error {
        inkscript_batch::LegacyImageAdapterError::MissingBinding => {
            AnnotationFrameAdapterError::MissingReference
        }
        inkscript_batch::LegacyImageAdapterError::ResourceLimit => {
            AnnotationFrameAdapterError::ResourceLimit
        }
        _ => AnnotationFrameAdapterError::InvalidValue,
    }
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, AnnotationFrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(AnnotationFrameAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, AnnotationFrameAdapterError> {
    fields
        .get(name)
        .ok_or(AnnotationFrameAdapterError::InvalidTypedStep)
}

fn bounded_list(
    value: &InkScriptTypedValue,
    minimum: usize,
    maximum: usize,
) -> Result<&[InkScriptTypedValue], AnnotationFrameAdapterError> {
    let InkScriptTypedValueKind::List(values) = value.kind() else {
        return Err(AnnotationFrameAdapterError::InvalidTypedStep);
    };
    if values.len() < minimum || values.len() > maximum {
        return Err(AnnotationFrameAdapterError::ResourceLimit);
    }
    Ok(values)
}

fn nullable<T>(
    value: &InkScriptTypedValue,
    parse: impl FnOnce(&InkScriptTypedValue) -> Result<T, AnnotationFrameAdapterError>,
) -> Result<Option<T>, AnnotationFrameAdapterError> {
    if matches!(value.kind(), InkScriptTypedValueKind::None) {
        Ok(None)
    } else {
        parse(value).map(Some)
    }
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected: &str,
) -> Result<&'a [InkScriptTypedValue], AnnotationFrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments } if name == expected => {
            Ok(arguments)
        }
        _ => Err(AnnotationFrameAdapterError::InvalidValue),
    }
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, AnnotationFrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(AnnotationFrameAdapterError::InvalidValue),
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, AnnotationFrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(AnnotationFrameAdapterError::InvalidValue),
    }
}

fn string(value: &InkScriptTypedValue) -> Result<&str, AnnotationFrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::String(value) => Ok(value),
        _ => Err(AnnotationFrameAdapterError::InvalidValue),
    }
}

fn u32_value(value: &InkScriptTypedValue) -> Result<u32, AnnotationFrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(AnnotationFrameAdapterError::InvalidValue),
    }
}

fn i32_value(value: &InkScriptTypedValue) -> Result<i32, AnnotationFrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::I32(value) => Ok(*value),
        _ => Err(AnnotationFrameAdapterError::InvalidValue),
    }
}

fn u64_value(value: &InkScriptTypedValue) -> Result<u64, AnnotationFrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U64(value) => Ok(*value),
        _ => Err(AnnotationFrameAdapterError::InvalidValue),
    }
}

fn i64_value(value: &InkScriptTypedValue) -> Result<i64, AnnotationFrameAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::I64(value) => Ok(*value),
        _ => Err(AnnotationFrameAdapterError::InvalidValue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn annotation_frame_adapter_is_core_owned_and_thread_suitable() {
        assert_send_sync::<AnnotationFrameScriptStep>();
        assert_send_sync::<AnnotationFrameAdapterError>();
    }
}
