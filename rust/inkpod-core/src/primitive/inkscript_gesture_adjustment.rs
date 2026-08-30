//! Private pre-ratification InkScript adapter for gesture, alpha, and scoped-color primitives.

use super::CanonicalInvocation;
use super::inkscript_batch;
use super::inkscript_fill_gradient;
use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use crate::{
    AirbrushGesture, AirbrushStroke, EffectSample, ScopedColorReplaceMode, SelectionShape, Stamp,
    StampGesture, StampShape,
};
use inkpod_format::{
    InkScriptCommandSchema, InkScriptEnumSchema, InkScriptFieldSchema, InkScriptRecordSchema,
    InkScriptTypedStep, InkScriptTypedValue, InkScriptTypedValueKind,
};
use std::collections::BTreeMap;

const MAX_EFFECT_SAMPLES: usize = 1_048_576;

pub(crate) const GESTURE_ADJUSTMENT_ENUMS: &[InkScriptEnumSchema] = &[
    InkScriptEnumSchema::new("stamp_shape", &["round", "square"]),
    InkScriptEnumSchema::new("scoped_color_mode", &["raster_color", "raster_main_line"]),
];

const EFFECT_SAMPLE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("position", "point", 0),
    InkScriptFieldSchema::required("pressure_milli", "u32", 1),
];
const AIRBRUSH_STROKE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("center", "point", 0),
    InkScriptFieldSchema::required("radius_milli", "u32", 1),
    InkScriptFieldSchema::required("hardness_milli", "u32", 2),
    InkScriptFieldSchema::required("opacity_milli", "u32", 3),
    InkScriptFieldSchema::required("color", "rgba16", 4),
];
const AIRBRUSH_GESTURE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("samples", "list<effect_sample>", 0),
    InkScriptFieldSchema::required("radius_milli", "u32", 1),
    InkScriptFieldSchema::required("hardness_milli", "u32", 2),
    InkScriptFieldSchema::required("spacing_milli", "u32", 3),
    InkScriptFieldSchema::required("opacity_milli", "u32", 4),
    InkScriptFieldSchema::required("fade_milli", "u32", 5),
    InkScriptFieldSchema::required("pressure_size", "bool", 6),
    InkScriptFieldSchema::required("pressure_opacity", "bool", 7),
    InkScriptFieldSchema::required("continuous_dabs", "u32", 8),
    InkScriptFieldSchema::required("color", "rgba16", 9),
];
const STAMP_SPEC_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("source_x", "i32", 0),
    InkScriptFieldSchema::required("source_y", "i32", 1),
    InkScriptFieldSchema::required("destination_x", "i32", 2),
    InkScriptFieldSchema::required("destination_y", "i32", 3),
    InkScriptFieldSchema::required("width", "u32", 4),
    InkScriptFieldSchema::required("height", "u32", 5),
    InkScriptFieldSchema::required("opacity_milli", "u32", 6),
];
const STAMP_GESTURE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("source", "point", 0),
    InkScriptFieldSchema::required("samples", "list<effect_sample>", 1),
    InkScriptFieldSchema::required("radius_milli", "u32", 2),
    InkScriptFieldSchema::required("hardness_milli", "u32", 3),
    InkScriptFieldSchema::required("spacing_milli", "u32", 4),
    InkScriptFieldSchema::required("opacity_milli", "u32", 5),
    InkScriptFieldSchema::required("shape", "stamp_shape", 6),
    InkScriptFieldSchema::required("pressure_size", "bool", 7),
    InkScriptFieldSchema::required("pressure_opacity", "bool", 8),
];
pub(crate) const GESTURE_ADJUSTMENT_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("effect_sample", EFFECT_SAMPLE_FIELDS),
    InkScriptRecordSchema::new("airbrush_stroke", AIRBRUSH_STROKE_FIELDS),
    InkScriptRecordSchema::new("airbrush_gesture", AIRBRUSH_GESTURE_FIELDS),
    InkScriptRecordSchema::new("stamp_spec", STAMP_SPEC_FIELDS),
    InkScriptRecordSchema::new("stamp_gesture", STAMP_GESTURE_FIELDS),
];

const APPLY_BLUR_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("radius", "u32", 1),
    InkScriptFieldSchema::required("strength_milli", "u32", 2),
];
const APPLY_AIRBRUSH_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("stroke", "airbrush_stroke", 1),
];
const APPLY_AIRBRUSH_GESTURE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("gesture", "airbrush_gesture", 1),
];
const APPLY_STAMP_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("stamp", "stamp_spec", 1),
];
const APPLY_STAMP_GESTURE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("gesture", "stamp_gesture", 1),
];
const APPLY_BLUR_TOOL_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("shape", "selection_shape", 1),
    InkScriptFieldSchema::required("radius", "u32", 2),
    InkScriptFieldSchema::required("strength_milli", "u32", 3),
];
const EDIT_PLANE_ALPHA_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("alpha", "asset_ref", 1),
];
const APPLY_ALPHA_GRADIENT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("gradient", "gradient_spec", 1),
];
const SCOPED_COLOR_REPLACE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("mode", "scoped_color_mode", 1),
    InkScriptFieldSchema::required("target", "pixel_value", 2),
    InkScriptFieldSchema::required("replacement", "pixel_value", 3),
    InkScriptFieldSchema::required("region", "nullable<selection_shape>", 4),
];
pub(crate) const GESTURE_ADJUSTMENT_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::new("apply_blur", APPLY_BLUR_FIELDS),
    InkScriptCommandSchema::new("apply_airbrush", APPLY_AIRBRUSH_FIELDS),
    InkScriptCommandSchema::new("apply_airbrush_gesture", APPLY_AIRBRUSH_GESTURE_FIELDS),
    InkScriptCommandSchema::new("apply_stamp", APPLY_STAMP_FIELDS),
    InkScriptCommandSchema::new("apply_stamp_gesture", APPLY_STAMP_GESTURE_FIELDS),
    InkScriptCommandSchema::new("apply_blur_tool", APPLY_BLUR_TOOL_FIELDS),
    InkScriptCommandSchema::new("edit_plane_alpha", EDIT_PLANE_ALPHA_FIELDS),
    InkScriptCommandSchema::new("apply_alpha_gradient", APPLY_ALPHA_GRADIENT_FIELDS),
    InkScriptCommandSchema::new("scoped_color_replace", SCOPED_COLOR_REPLACE_FIELDS),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GestureAdjustmentScriptAction {
    Canonical(CanonicalInvocation),
    EditAlpha { plane_id: u64, asset_symbol: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GestureAdjustmentAdapterError {
    InvalidTypedStep,
    InvalidValue,
    MissingReference,
    ResourceLimit,
    UnsupportedPrimitive,
}

impl GestureAdjustmentScriptAction {
    pub(crate) fn from_compiled(
        step: &InkScriptTypedStep,
        arguments: &InkScriptTypedValue,
        bindings: &InkScriptRuntimeReferences,
    ) -> Result<Self, GestureAdjustmentAdapterError> {
        let fields = record(arguments)?;
        let plane = |name| plane_reference(field(fields, name)?, bindings);
        let canonical = match step.command() {
            "apply_blur" => CanonicalInvocation::ApplyBlur {
                plane_id: plane("plane_id")?,
                radius: unsigned(field(fields, "radius")?)?,
                strength_milli: unsigned(field(fields, "strength_milli")?)?,
            },
            "apply_airbrush" => CanonicalInvocation::ApplyAirbrush {
                plane_id: plane("plane_id")?,
                stroke: airbrush_stroke(field(fields, "stroke")?)?,
            },
            "apply_airbrush_gesture" => CanonicalInvocation::ApplyAirbrushGesture {
                plane_id: plane("plane_id")?,
                gesture: airbrush_gesture(field(fields, "gesture")?)?,
            },
            "apply_stamp" => CanonicalInvocation::ApplyStamp {
                plane_id: plane("plane_id")?,
                stamp: stamp(field(fields, "stamp")?)?,
            },
            "apply_stamp_gesture" => CanonicalInvocation::ApplyStampGesture {
                plane_id: plane("plane_id")?,
                gesture: stamp_gesture(field(fields, "gesture")?)?,
            },
            "apply_blur_tool" => CanonicalInvocation::ApplyBlurTool {
                plane_id: plane("plane_id")?,
                shape: selection_shape(field(fields, "shape")?)?,
                radius: unsigned(field(fields, "radius")?)?,
                strength_milli: unsigned(field(fields, "strength_milli")?)?,
            },
            "edit_plane_alpha" => {
                return Ok(Self::EditAlpha {
                    plane_id: plane("plane_id")?,
                    asset_symbol: asset_reference(field(fields, "alpha")?)?.to_owned(),
                });
            }
            "apply_alpha_gradient" => CanonicalInvocation::ApplyAlphaGradient {
                plane_id: plane("plane_id")?,
                gradient: inkscript_fill_gradient::gradient(field(fields, "gradient")?)
                    .map_err(fill_gradient_error)?,
            },
            "scoped_color_replace" => CanonicalInvocation::ScopedColorReplace {
                plane_id: plane("plane_id")?,
                mode: scoped_color_mode(field(fields, "mode")?)?,
                target: inkscript_batch::pixel(field(fields, "target")?)
                    .map_err(legacy_image_error)?,
                replacement: inkscript_batch::pixel(field(fields, "replacement")?)
                    .map_err(legacy_image_error)?,
                region: nullable(field(fields, "region")?, selection_shape)?,
            },
            _ => return Err(GestureAdjustmentAdapterError::UnsupportedPrimitive),
        };
        Ok(Self::Canonical(canonical))
    }

    pub(crate) fn output_entity_kinds(
        &self,
        output_count: usize,
    ) -> Result<Vec<InkScriptEntityKind>, GestureAdjustmentAdapterError> {
        match self {
            Self::Canonical(_) | Self::EditAlpha { .. } if output_count == 0 => Ok(Vec::new()),
            Self::Canonical(_) | Self::EditAlpha { .. } => {
                Err(GestureAdjustmentAdapterError::InvalidValue)
            }
        }
    }
}

fn airbrush_stroke(
    value: &InkScriptTypedValue,
) -> Result<AirbrushStroke, GestureAdjustmentAdapterError> {
    let fields = record(value)?;
    let center = inkscript_fill_gradient::point_milli(field(fields, "center")?)
        .map_err(fill_gradient_error)?;
    Ok(AirbrushStroke {
        center_x_milli: center.0,
        center_y_milli: center.1,
        radius_milli: unsigned(field(fields, "radius_milli")?)?,
        hardness_milli: unsigned(field(fields, "hardness_milli")?)?,
        opacity_milli: unsigned(field(fields, "opacity_milli")?)?,
        color: inkscript_fill_gradient::rgba16(field(fields, "color")?)
            .map_err(fill_gradient_error)?,
    })
}

fn airbrush_gesture(
    value: &InkScriptTypedValue,
) -> Result<AirbrushGesture, GestureAdjustmentAdapterError> {
    let fields = record(value)?;
    Ok(AirbrushGesture {
        samples: effect_samples(field(fields, "samples")?)?,
        radius_milli: unsigned(field(fields, "radius_milli")?)?,
        hardness_milli: unsigned(field(fields, "hardness_milli")?)?,
        spacing_milli: unsigned(field(fields, "spacing_milli")?)?,
        opacity_milli: unsigned(field(fields, "opacity_milli")?)?,
        fade_milli: unsigned(field(fields, "fade_milli")?)?,
        pressure_size: boolean(field(fields, "pressure_size")?)?,
        pressure_opacity: boolean(field(fields, "pressure_opacity")?)?,
        continuous_dabs: unsigned(field(fields, "continuous_dabs")?)?,
        color: inkscript_fill_gradient::rgba16(field(fields, "color")?)
            .map_err(fill_gradient_error)?,
    })
}

fn stamp(value: &InkScriptTypedValue) -> Result<Stamp, GestureAdjustmentAdapterError> {
    let fields = record(value)?;
    Ok(Stamp {
        source_x: signed(field(fields, "source_x")?)?,
        source_y: signed(field(fields, "source_y")?)?,
        destination_x: signed(field(fields, "destination_x")?)?,
        destination_y: signed(field(fields, "destination_y")?)?,
        width: unsigned(field(fields, "width")?)?,
        height: unsigned(field(fields, "height")?)?,
        opacity_milli: unsigned(field(fields, "opacity_milli")?)?,
    })
}

fn stamp_gesture(
    value: &InkScriptTypedValue,
) -> Result<StampGesture, GestureAdjustmentAdapterError> {
    let fields = record(value)?;
    let source = inkscript_fill_gradient::point_milli(field(fields, "source")?)
        .map_err(fill_gradient_error)?;
    Ok(StampGesture {
        source_x_milli: source.0,
        source_y_milli: source.1,
        samples: effect_samples(field(fields, "samples")?)?,
        radius_milli: unsigned(field(fields, "radius_milli")?)?,
        hardness_milli: unsigned(field(fields, "hardness_milli")?)?,
        spacing_milli: unsigned(field(fields, "spacing_milli")?)?,
        opacity_milli: unsigned(field(fields, "opacity_milli")?)?,
        shape: match enum_value(field(fields, "shape")?)? {
            "round" => StampShape::Round,
            "square" => StampShape::Square,
            _ => return Err(GestureAdjustmentAdapterError::InvalidValue),
        },
        pressure_size: boolean(field(fields, "pressure_size")?)?,
        pressure_opacity: boolean(field(fields, "pressure_opacity")?)?,
    })
}

fn effect_samples(
    value: &InkScriptTypedValue,
) -> Result<Vec<EffectSample>, GestureAdjustmentAdapterError> {
    let values = list(value)?;
    if values.is_empty() || values.len() > MAX_EFFECT_SAMPLES {
        return Err(GestureAdjustmentAdapterError::ResourceLimit);
    }
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(values.len())
        .map_err(|_| GestureAdjustmentAdapterError::ResourceLimit)?;
    for value in values {
        let fields = record(value)?;
        let position = inkscript_fill_gradient::point_milli(field(fields, "position")?)
            .map_err(fill_gradient_error)?;
        samples.push(EffectSample {
            x_milli: position.0,
            y_milli: position.1,
            pressure_milli: unsigned(field(fields, "pressure_milli")?)?,
        });
    }
    Ok(samples)
}

fn selection_shape(
    value: &InkScriptTypedValue,
) -> Result<SelectionShape, GestureAdjustmentAdapterError> {
    inkscript_batch::selection_shape(value).map_err(legacy_image_error)
}

fn scoped_color_mode(
    value: &InkScriptTypedValue,
) -> Result<ScopedColorReplaceMode, GestureAdjustmentAdapterError> {
    match enum_value(value)? {
        "raster_color" => Ok(ScopedColorReplaceMode::RasterColor),
        "raster_main_line" => Ok(ScopedColorReplaceMode::RasterMainLine),
        _ => Err(GestureAdjustmentAdapterError::InvalidValue),
    }
}

fn plane_reference(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<u64, GestureAdjustmentAdapterError> {
    bindings
        .resolve(value, InkScriptEntityKind::Plane)
        .map_err(reference_error)
}

fn reference_error(error: InkScriptReferenceError) -> GestureAdjustmentAdapterError {
    match error {
        InkScriptReferenceError::MissingReference => {
            GestureAdjustmentAdapterError::MissingReference
        }
        InkScriptReferenceError::InvalidReference | InkScriptReferenceError::KindMismatch => {
            GestureAdjustmentAdapterError::InvalidValue
        }
    }
}

fn fill_gradient_error(
    error: inkscript_fill_gradient::FillGradientAdapterError,
) -> GestureAdjustmentAdapterError {
    match error {
        inkscript_fill_gradient::FillGradientAdapterError::MissingReference => {
            GestureAdjustmentAdapterError::MissingReference
        }
        inkscript_fill_gradient::FillGradientAdapterError::ResourceLimit => {
            GestureAdjustmentAdapterError::ResourceLimit
        }
        _ => GestureAdjustmentAdapterError::InvalidValue,
    }
}

fn legacy_image_error(
    error: inkscript_batch::LegacyImageAdapterError,
) -> GestureAdjustmentAdapterError {
    match error {
        inkscript_batch::LegacyImageAdapterError::MissingBinding => {
            GestureAdjustmentAdapterError::MissingReference
        }
        _ => GestureAdjustmentAdapterError::InvalidValue,
    }
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, GestureAdjustmentAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(GestureAdjustmentAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, GestureAdjustmentAdapterError> {
    fields
        .get(name)
        .ok_or(GestureAdjustmentAdapterError::InvalidTypedStep)
}

fn list(
    value: &InkScriptTypedValue,
) -> Result<&[InkScriptTypedValue], GestureAdjustmentAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::List(values) => Ok(values),
        _ => Err(GestureAdjustmentAdapterError::InvalidTypedStep),
    }
}

fn nullable<T>(
    value: &InkScriptTypedValue,
    convert: impl FnOnce(&InkScriptTypedValue) -> Result<T, GestureAdjustmentAdapterError>,
) -> Result<Option<T>, GestureAdjustmentAdapterError> {
    if matches!(value.kind(), InkScriptTypedValueKind::None) {
        Ok(None)
    } else {
        convert(value).map(Some)
    }
}

fn asset_reference(value: &InkScriptTypedValue) -> Result<&str, GestureAdjustmentAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::AssetReference(value) => Ok(value),
        _ => Err(GestureAdjustmentAdapterError::InvalidValue),
    }
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, GestureAdjustmentAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(GestureAdjustmentAdapterError::InvalidValue),
    }
}

fn string(value: &InkScriptTypedValue) -> Result<&str, GestureAdjustmentAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::String(value) => Ok(value),
        _ => Err(GestureAdjustmentAdapterError::InvalidValue),
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, GestureAdjustmentAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(GestureAdjustmentAdapterError::InvalidValue),
    }
}

fn signed(value: &InkScriptTypedValue) -> Result<i32, GestureAdjustmentAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::I32(value) => Ok(*value),
        _ => Err(GestureAdjustmentAdapterError::InvalidValue),
    }
}

fn unsigned(value: &InkScriptTypedValue) -> Result<u32, GestureAdjustmentAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(GestureAdjustmentAdapterError::InvalidValue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn gesture_adjustment_adapter_is_core_owned_and_thread_suitable() {
        assert_send_sync::<GestureAdjustmentScriptAction>();
        assert_send_sync::<GestureAdjustmentAdapterError>();
    }
}
