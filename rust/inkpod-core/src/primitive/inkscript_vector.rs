//! Private pre-ratification InkScript adapter for vector primitives.

use super::CanonicalInvocation;
use super::inkscript_batch;
use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use crate::{
    PixelValue, PointF32, VectorCubicSegment, VectorEraseMode, VectorPathInput, VectorWidthMode,
};
use inkpod_format::{
    InkScriptCommandResultSchema, InkScriptCommandSchema, InkScriptEnumSchema,
    InkScriptFieldSchema, InkScriptRecordSchema, InkScriptResultAvailability, InkScriptTypedStep,
    InkScriptTypedValue, InkScriptTypedValueKind, MAX_VECTOR_BOUNDARIES, MAX_VECTOR_FILLS,
    MAX_VECTOR_SEGMENTS,
};
use std::collections::BTreeMap;

const Q16_SCALE: f64 = 65_536.0;
const MAX_VECTOR_COORDINATE_Q16: i64 = 2_000_000 * 65_536;
const MAX_VECTOR_WIDTH_Q16: i64 = 4_096 * 65_536;

pub(crate) const VECTOR_ENUMS: &[InkScriptEnumSchema] = &[
    InkScriptEnumSchema::new(
        "vector_erase_mode",
        &["partial", "to_intersection", "whole_path"],
    ),
    InkScriptEnumSchema::new(
        "vector_width_operation",
        &["add", "subtract", "scale", "constant"],
    ),
];

const VECTOR_CUBIC_SEGMENT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("p0", "point", 0),
    InkScriptFieldSchema::required("p1", "point", 1),
    InkScriptFieldSchema::required("p2", "point", 2),
    InkScriptFieldSchema::required("p3", "point", 3),
    InkScriptFieldSchema::required("width_start", "q16", 4),
    InkScriptFieldSchema::required("width_end", "q16", 5),
];
const VECTOR_PATH_INPUT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("segments", "list<vector_cubic_segment>", 0),
    InkScriptFieldSchema::required("color", "pixel_value", 1),
    InkScriptFieldSchema::required("closed", "bool", 2),
];
const VECTOR_WIDTH_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("operation", "vector_width_operation", 0),
    InkScriptFieldSchema::required("value", "q16", 1),
];

pub(crate) const VECTOR_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("vector_cubic_segment", VECTOR_CUBIC_SEGMENT_FIELDS),
    InkScriptRecordSchema::new("vector_path_input", VECTOR_PATH_INPUT_FIELDS),
    InkScriptRecordSchema::new("vector_width", VECTOR_WIDTH_FIELDS),
];

const VECTOR_ADD_PATH_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("input", "vector_path_input", 1),
];
const VECTOR_ADD_FILL_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("boundary_path_ids", "list<vector_path_ref>", 1),
    InkScriptFieldSchema::required("color", "pixel_value", 2),
];
const VECTOR_ERASE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("point", "point", 1),
    InkScriptFieldSchema::required("radius", "q16", 2),
    InkScriptFieldSchema::required("mode", "vector_erase_mode", 3),
];
const VECTOR_CONNECT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("maximum_gap", "q16", 1),
];
const VECTOR_CORRECT_WIDTH_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("path_ids", "list<vector_path_ref>", 0),
    InkScriptFieldSchema::required("width", "vector_width", 1),
];
const RASTERIZE_VECTOR_LAYER_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("layer_id", "layer_ref", 0),
    InkScriptFieldSchema::required("antialias", "bool", 1),
    InkScriptFieldSchema::required("name", "string", 2),
];
const VECTORIZE_RASTER_PLANE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("source_plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("target_vector_layer_id", "layer_ref", 1),
    InkScriptFieldSchema::required("alpha_threshold", "u32", 2),
];
const VECTORIZE_RASTER_PLANE_INTO_NEW_LAYER_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("source_plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("alpha_threshold", "u32", 1),
    InkScriptFieldSchema::required("name", "string", 2),
];

const PATH_RESULTS: &[InkScriptCommandResultSchema] =
    &[InkScriptCommandResultSchema::ordered_list(
        "paths",
        "vector_path_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    )];
const OPTIONAL_PATH_RESULTS: &[InkScriptCommandResultSchema] =
    &[InkScriptCommandResultSchema::ordered_list(
        "paths",
        "vector_path_ref",
        InkScriptResultAvailability::OnlyOnChange,
        0,
    )];
const FILL_RESULTS: &[InkScriptCommandResultSchema] =
    &[InkScriptCommandResultSchema::ordered_list(
        "fills",
        "vector_fill_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    )];
const LAYER_RESULTS: &[InkScriptCommandResultSchema] = &[InkScriptCommandResultSchema::scalar(
    "layer",
    "layer_ref",
    InkScriptResultAvailability::AlwaysOnSuccess,
    0,
)];
const NEW_VECTOR_LAYER_RESULTS: &[InkScriptCommandResultSchema] = &[
    InkScriptCommandResultSchema::scalar(
        "layer",
        "layer_ref",
        InkScriptResultAvailability::OnlyOnChange,
        0,
    ),
    InkScriptCommandResultSchema::ordered_list(
        "fills",
        "vector_fill_ref",
        InkScriptResultAvailability::OnlyOnChange,
        1,
    ),
];

pub(crate) const VECTOR_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::with_results("vector_add_path", VECTOR_ADD_PATH_FIELDS, PATH_RESULTS),
    InkScriptCommandSchema::with_results("vector_add_fill", VECTOR_ADD_FILL_FIELDS, FILL_RESULTS),
    InkScriptCommandSchema::new("vector_erase", VECTOR_ERASE_FIELDS),
    InkScriptCommandSchema::with_results(
        "vector_connect",
        VECTOR_CONNECT_FIELDS,
        OPTIONAL_PATH_RESULTS,
    ),
    InkScriptCommandSchema::new("vector_correct_width", VECTOR_CORRECT_WIDTH_FIELDS),
    InkScriptCommandSchema::with_results(
        "rasterize_vector_layer",
        RASTERIZE_VECTOR_LAYER_FIELDS,
        LAYER_RESULTS,
    ),
    InkScriptCommandSchema::with_results(
        "vectorize_raster_plane",
        VECTORIZE_RASTER_PLANE_FIELDS,
        FILL_RESULTS,
    ),
    InkScriptCommandSchema::with_results(
        "vectorize_raster_plane_into_new_layer",
        VECTORIZE_RASTER_PLANE_INTO_NEW_LAYER_FIELDS,
        NEW_VECTOR_LAYER_RESULTS,
    ),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VectorScriptStep {
    invocation: CanonicalInvocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VectorAdapterError {
    InvalidTypedStep,
    InvalidValue,
    MissingReference,
    ResourceLimit,
    UnsupportedPrimitive,
}

impl VectorScriptStep {
    pub(crate) fn from_compiled(
        step: &InkScriptTypedStep,
        arguments: &InkScriptTypedValue,
        bindings: &InkScriptRuntimeReferences,
    ) -> Result<Self, VectorAdapterError> {
        let fields = record(arguments)?;
        let invocation = match step.command() {
            "vector_add_path" => CanonicalInvocation::VectorAddPath {
                plane_id: reference(
                    field(fields, "plane_id")?,
                    bindings,
                    InkScriptEntityKind::Plane,
                )?,
                input: path_input(field(fields, "input")?)?,
            },
            "vector_add_fill" => CanonicalInvocation::VectorAddFill {
                plane_id: reference(
                    field(fields, "plane_id")?,
                    bindings,
                    InkScriptEntityKind::Plane,
                )?,
                boundary_path_ids: references(
                    field(fields, "boundary_path_ids")?,
                    bindings,
                    InkScriptEntityKind::VectorPath,
                    1,
                    MAX_VECTOR_BOUNDARIES,
                )?,
                color: rgba_pixel(field(fields, "color")?)?,
            },
            "vector_erase" => CanonicalInvocation::VectorErase {
                plane_id: reference(
                    field(fields, "plane_id")?,
                    bindings,
                    InkScriptEntityKind::Plane,
                )?,
                point: point(field(fields, "point")?)?,
                radius: positive_q16(field(fields, "radius")?)?,
                mode: erase_mode(field(fields, "mode")?)?,
            },
            "vector_connect" => CanonicalInvocation::VectorConnect {
                plane_id: reference(
                    field(fields, "plane_id")?,
                    bindings,
                    InkScriptEntityKind::Plane,
                )?,
                maximum_gap: positive_q16(field(fields, "maximum_gap")?)?,
            },
            "vector_correct_width" => CanonicalInvocation::VectorCorrectWidth {
                path_ids: references(
                    field(fields, "path_ids")?,
                    bindings,
                    InkScriptEntityKind::VectorPath,
                    1,
                    inkpod_format::MAX_VECTOR_PATHS,
                )?,
                mode: width_mode(field(fields, "width")?)?,
            },
            "rasterize_vector_layer" => CanonicalInvocation::RasterizeVectorLayer {
                layer_id: reference(
                    field(fields, "layer_id")?,
                    bindings,
                    InkScriptEntityKind::Layer,
                )?,
                antialias: boolean(field(fields, "antialias")?)?,
                name: string(field(fields, "name")?)?.to_owned(),
            },
            "vectorize_raster_plane" => CanonicalInvocation::VectorizeRasterPlane {
                source_plane_id: reference(
                    field(fields, "source_plane_id")?,
                    bindings,
                    InkScriptEntityKind::Plane,
                )?,
                target_vector_layer_id: reference(
                    field(fields, "target_vector_layer_id")?,
                    bindings,
                    InkScriptEntityKind::Layer,
                )?,
                alpha_threshold: narrow_u8(field(fields, "alpha_threshold")?)?,
            },
            "vectorize_raster_plane_into_new_layer" => {
                CanonicalInvocation::VectorizeRasterPlaneIntoNewLayer {
                    source_plane_id: reference(
                        field(fields, "source_plane_id")?,
                        bindings,
                        InkScriptEntityKind::Plane,
                    )?,
                    alpha_threshold: narrow_u8(field(fields, "alpha_threshold")?)?,
                    name: string(field(fields, "name")?)?.to_owned(),
                }
            }
            _ => return Err(VectorAdapterError::UnsupportedPrimitive),
        };
        Ok(Self { invocation })
    }

    pub(crate) fn to_canonical(&self) -> CanonicalInvocation {
        self.invocation.clone()
    }

    pub(crate) fn output_entity_kinds(
        &self,
        output_count: usize,
    ) -> Result<Vec<InkScriptEntityKind>, VectorAdapterError> {
        match &self.invocation {
            CanonicalInvocation::VectorAddPath { .. } if output_count == 1 => {
                Ok(vec![InkScriptEntityKind::VectorPath])
            }
            CanonicalInvocation::VectorConnect { .. } => match output_count {
                0 => Ok(Vec::new()),
                1 => Ok(vec![InkScriptEntityKind::VectorPath]),
                _ => Err(VectorAdapterError::InvalidValue),
            },
            CanonicalInvocation::VectorAddFill { .. } if output_count == 1 => {
                Ok(vec![InkScriptEntityKind::VectorFill])
            }
            CanonicalInvocation::RasterizeVectorLayer { .. } if output_count == 1 => {
                Ok(vec![InkScriptEntityKind::Layer])
            }
            CanonicalInvocation::VectorizeRasterPlane { .. }
                if output_count <= MAX_VECTOR_FILLS =>
            {
                Ok(vec![InkScriptEntityKind::VectorFill; output_count])
            }
            CanonicalInvocation::VectorizeRasterPlaneIntoNewLayer { .. } => {
                if output_count == 0 {
                    Ok(Vec::new())
                } else if output_count <= MAX_VECTOR_FILLS + 1 {
                    let mut kinds = Vec::new();
                    kinds
                        .try_reserve_exact(output_count)
                        .map_err(|_| VectorAdapterError::ResourceLimit)?;
                    kinds.push(InkScriptEntityKind::Layer);
                    kinds.resize(output_count, InkScriptEntityKind::VectorFill);
                    Ok(kinds)
                } else {
                    Err(VectorAdapterError::ResourceLimit)
                }
            }
            CanonicalInvocation::VectorErase { .. }
            | CanonicalInvocation::VectorCorrectWidth { .. }
                if output_count == 0 =>
            {
                Ok(Vec::new())
            }
            _ => Err(VectorAdapterError::InvalidValue),
        }
    }
}

fn path_input(value: &InkScriptTypedValue) -> Result<VectorPathInput, VectorAdapterError> {
    let fields = record(value)?;
    let values = list(field(fields, "segments")?)?;
    if values.is_empty() || values.len() > MAX_VECTOR_SEGMENTS {
        return Err(VectorAdapterError::ResourceLimit);
    }
    let mut segments = Vec::new();
    segments
        .try_reserve_exact(values.len())
        .map_err(|_| VectorAdapterError::ResourceLimit)?;
    for value in values {
        let fields = record(value)?;
        segments.push(VectorCubicSegment {
            p0: point(field(fields, "p0")?)?,
            p1: point(field(fields, "p1")?)?,
            p2: point(field(fields, "p2")?)?,
            p3: point(field(fields, "p3")?)?,
            width_start: positive_q16(field(fields, "width_start")?)?,
            width_end: positive_q16(field(fields, "width_end")?)?,
        });
    }
    Ok(VectorPathInput {
        segments,
        color: rgba_pixel(field(fields, "color")?)?,
        closed: boolean(field(fields, "closed")?)?,
    })
}

fn width_mode(value: &InkScriptTypedValue) -> Result<VectorWidthMode, VectorAdapterError> {
    let fields = record(value)?;
    let value = positive_q16(field(fields, "value")?)?;
    match enum_value(field(fields, "operation")?)? {
        "add" => Ok(VectorWidthMode::Add(value)),
        "subtract" => Ok(VectorWidthMode::Subtract(value)),
        "scale" => Ok(VectorWidthMode::Scale(value)),
        "constant" => Ok(VectorWidthMode::Constant(value)),
        _ => Err(VectorAdapterError::InvalidValue),
    }
}

fn erase_mode(value: &InkScriptTypedValue) -> Result<VectorEraseMode, VectorAdapterError> {
    match enum_value(value)? {
        "partial" => Ok(VectorEraseMode::Partial),
        "to_intersection" => Ok(VectorEraseMode::ToIntersection),
        "whole_path" => Ok(VectorEraseMode::WholePath),
        _ => Err(VectorAdapterError::InvalidValue),
    }
}

fn rgba_pixel(value: &InkScriptTypedValue) -> Result<PixelValue, VectorAdapterError> {
    let pixel = inkscript_batch::pixel(value).map_err(legacy_image_error)?;
    if pixel.rgba16().is_none() {
        return Err(VectorAdapterError::InvalidValue);
    }
    Ok(pixel)
}

fn point(value: &InkScriptTypedValue) -> Result<PointF32, VectorAdapterError> {
    let values = constructor(value, "point")?;
    if values.len() != 2 {
        return Err(VectorAdapterError::InvalidValue);
    }
    let x = q16(&values[0])?;
    let y = q16(&values[1])?;
    if x.unsigned_abs() > MAX_VECTOR_COORDINATE_Q16 as u64
        || y.unsigned_abs() > MAX_VECTOR_COORDINATE_Q16 as u64
    {
        return Err(VectorAdapterError::InvalidValue);
    }
    Ok(PointF32 {
        x: q16_f32(x),
        y: q16_f32(y),
    })
}

fn positive_q16(value: &InkScriptTypedValue) -> Result<f32, VectorAdapterError> {
    let value = q16(value)?;
    if !(1..=MAX_VECTOR_WIDTH_Q16).contains(&value) {
        return Err(VectorAdapterError::InvalidValue);
    }
    Ok(q16_f32(value))
}

fn q16_f32(value: i64) -> f32 {
    (value as f64 / Q16_SCALE) as f32
}

fn references(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
    kind: InkScriptEntityKind,
    minimum: usize,
    maximum: usize,
) -> Result<Vec<u64>, VectorAdapterError> {
    let values = list(value)?;
    if values.len() < minimum || values.len() > maximum {
        return Err(VectorAdapterError::ResourceLimit);
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(values.len())
        .map_err(|_| VectorAdapterError::ResourceLimit)?;
    for value in values {
        result.push(reference(value, bindings, kind)?);
    }
    Ok(result)
}

fn reference(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
    kind: InkScriptEntityKind,
) -> Result<u64, VectorAdapterError> {
    bindings.resolve(value, kind).map_err(reference_error)
}

fn reference_error(error: InkScriptReferenceError) -> VectorAdapterError {
    match error {
        InkScriptReferenceError::MissingReference => VectorAdapterError::MissingReference,
        InkScriptReferenceError::InvalidReference | InkScriptReferenceError::KindMismatch => {
            VectorAdapterError::InvalidValue
        }
    }
}

fn legacy_image_error(error: inkscript_batch::LegacyImageAdapterError) -> VectorAdapterError {
    match error {
        inkscript_batch::LegacyImageAdapterError::MissingBinding => {
            VectorAdapterError::MissingReference
        }
        inkscript_batch::LegacyImageAdapterError::ResourceLimit => {
            VectorAdapterError::ResourceLimit
        }
        _ => VectorAdapterError::InvalidValue,
    }
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, VectorAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(VectorAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, VectorAdapterError> {
    fields.get(name).ok_or(VectorAdapterError::InvalidTypedStep)
}

fn list(value: &InkScriptTypedValue) -> Result<&[InkScriptTypedValue], VectorAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::List(values) => Ok(values),
        _ => Err(VectorAdapterError::InvalidTypedStep),
    }
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected: &str,
) -> Result<&'a [InkScriptTypedValue], VectorAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments } if name == expected => {
            Ok(arguments)
        }
        _ => Err(VectorAdapterError::InvalidValue),
    }
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, VectorAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(VectorAdapterError::InvalidValue),
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, VectorAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(VectorAdapterError::InvalidValue),
    }
}

fn string(value: &InkScriptTypedValue) -> Result<&str, VectorAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::String(value) => Ok(value),
        _ => Err(VectorAdapterError::InvalidValue),
    }
}

fn q16(value: &InkScriptTypedValue) -> Result<i64, VectorAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Q16(value) => Ok(*value),
        _ => Err(VectorAdapterError::InvalidValue),
    }
}

fn narrow_u8(value: &InkScriptTypedValue) -> Result<u8, VectorAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => {
            u8::try_from(*value).map_err(|_| VectorAdapterError::InvalidValue)
        }
        _ => Err(VectorAdapterError::InvalidValue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn vector_adapter_is_core_owned_and_thread_suitable() {
        assert_send_sync::<VectorScriptStep>();
        assert_send_sync::<VectorAdapterError>();
    }
}
