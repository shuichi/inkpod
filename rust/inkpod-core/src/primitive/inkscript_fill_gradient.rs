//! Private pre-ratification InkScript adapter for fill and gradient primitives.

use super::CanonicalInvocation;
use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use crate::{Gradient, GradientKind, GradientMode, GradientStop, MAX_GRADIENT_STOPS};
use inkpod_format::{
    InkScriptCommandSchema, InkScriptEnumSchema, InkScriptFieldSchema, InkScriptRecordSchema,
    InkScriptTypedStep, InkScriptTypedValue, InkScriptTypedValueKind,
};
use inkpod_image::{CANONICAL_DOCUMENT_ONE, div_round_ties_even_i128};
use std::collections::BTreeMap;

pub(crate) const FILL_GRADIENT_ENUMS: &[InkScriptEnumSchema] = &[
    InkScriptEnumSchema::new("gradient_kind", &["linear", "radial"]),
    InkScriptEnumSchema::new("gradient_mode", &["composite", "overwrite"]),
];

const GRADIENT_STOP_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("position_milli", "u32", 0),
    InkScriptFieldSchema::required("color", "rgba16", 1),
];
const GRADIENT_SPEC_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("kind", "gradient_kind", 0),
    InkScriptFieldSchema::required("mode", "gradient_mode", 1),
    InkScriptFieldSchema::required("start", "point", 2),
    InkScriptFieldSchema::required("end", "point", 3),
    InkScriptFieldSchema::required("dither", "bool", 4),
    InkScriptFieldSchema::required("stops", "list<gradient_stop>", 5),
];

pub(crate) const FILL_GRADIENT_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("gradient_stop", GRADIENT_STOP_FIELDS),
    InkScriptRecordSchema::new("gradient_spec", GRADIENT_SPEC_FIELDS),
];

const APPLY_GRADIENT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("gradient", "gradient_spec", 1),
];

pub(crate) const FILL_GRADIENT_COMMANDS: &[InkScriptCommandSchema] =
    &[InkScriptCommandSchema::new(
        "apply_gradient",
        APPLY_GRADIENT_FIELDS,
    )];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FillGradientScriptStep {
    invocation: CanonicalInvocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FillGradientAdapterError {
    InvalidTypedStep,
    InvalidValue,
    MissingReference,
    ResourceLimit,
    UnsupportedPrimitive,
}

impl FillGradientScriptStep {
    pub(crate) fn from_compiled(
        step: &InkScriptTypedStep,
        arguments: &InkScriptTypedValue,
        bindings: &InkScriptRuntimeReferences,
    ) -> Result<Self, FillGradientAdapterError> {
        if step.command() != "apply_gradient" {
            return Err(FillGradientAdapterError::UnsupportedPrimitive);
        }
        let fields = record(arguments)?;
        let plane_id = bindings
            .resolve(field(fields, "plane_id")?, InkScriptEntityKind::Plane)
            .map_err(reference_error)?;
        let gradient = gradient(field(fields, "gradient")?)?;
        Ok(Self {
            invocation: CanonicalInvocation::ApplyGradient { plane_id, gradient },
        })
    }

    pub(crate) fn to_canonical(&self) -> CanonicalInvocation {
        self.invocation.clone()
    }
}

pub(crate) fn gradient(value: &InkScriptTypedValue) -> Result<Gradient, FillGradientAdapterError> {
    let fields = record(value)?;
    let start = point_milli(field(fields, "start")?)?;
    let end = point_milli(field(fields, "end")?)?;
    let values = list(field(fields, "stops")?)?;
    if values.len() > MAX_GRADIENT_STOPS {
        return Err(FillGradientAdapterError::ResourceLimit);
    }
    let mut stops = Vec::new();
    stops
        .try_reserve_exact(values.len())
        .map_err(|_| FillGradientAdapterError::ResourceLimit)?;
    for value in values {
        let fields = record(value)?;
        stops.push(GradientStop {
            position_milli: unsigned(field(fields, "position_milli")?)?,
            color: rgba16(field(fields, "color")?)?,
        });
    }
    Ok(Gradient {
        kind: match enum_value(field(fields, "kind")?)? {
            "linear" => GradientKind::Linear,
            "radial" => GradientKind::Radial,
            _ => return Err(FillGradientAdapterError::InvalidValue),
        },
        mode: match enum_value(field(fields, "mode")?)? {
            "composite" => GradientMode::Composite,
            "overwrite" => GradientMode::Overwrite,
            _ => return Err(FillGradientAdapterError::InvalidValue),
        },
        start_x_milli: start.0,
        start_y_milli: start.1,
        end_x_milli: end.0,
        end_y_milli: end.1,
        dither: boolean(field(fields, "dither")?)?,
        stops,
    })
}

pub(crate) fn point_milli(
    value: &InkScriptTypedValue,
) -> Result<(i64, i64), FillGradientAdapterError> {
    let values = constructor(value, "point")?;
    if values.len() != 2 {
        return Err(FillGradientAdapterError::InvalidValue);
    }
    Ok((q16_milli(&values[0])?, q16_milli(&values[1])?))
}

fn q16_milli(value: &InkScriptTypedValue) -> Result<i64, FillGradientAdapterError> {
    let InkScriptTypedValueKind::Q16(value) = value.kind() else {
        return Err(FillGradientAdapterError::InvalidValue);
    };
    let scaled = i128::from(*value)
        .checked_mul(1_000)
        .ok_or(FillGradientAdapterError::ResourceLimit)?;
    div_round_ties_even_i128(scaled, i128::from(CANONICAL_DOCUMENT_ONE))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(FillGradientAdapterError::ResourceLimit)
}

pub(crate) fn rgba16(value: &InkScriptTypedValue) -> Result<[u16; 4], FillGradientAdapterError> {
    let values = constructor(value, "rgba16")?;
    if values.len() != 4 {
        return Err(FillGradientAdapterError::InvalidValue);
    }
    Ok([
        narrow_u16(&values[0])?,
        narrow_u16(&values[1])?,
        narrow_u16(&values[2])?,
        narrow_u16(&values[3])?,
    ])
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, FillGradientAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(FillGradientAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, FillGradientAdapterError> {
    fields
        .get(name)
        .ok_or(FillGradientAdapterError::InvalidTypedStep)
}

fn list(value: &InkScriptTypedValue) -> Result<&[InkScriptTypedValue], FillGradientAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::List(values) => Ok(values),
        _ => Err(FillGradientAdapterError::InvalidTypedStep),
    }
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected: &str,
) -> Result<&'a [InkScriptTypedValue], FillGradientAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments } if name == expected => {
            Ok(arguments)
        }
        _ => Err(FillGradientAdapterError::InvalidValue),
    }
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, FillGradientAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(FillGradientAdapterError::InvalidValue),
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, FillGradientAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(FillGradientAdapterError::InvalidValue),
    }
}

fn unsigned(value: &InkScriptTypedValue) -> Result<u32, FillGradientAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(FillGradientAdapterError::InvalidValue),
    }
}

fn narrow_u16(value: &InkScriptTypedValue) -> Result<u16, FillGradientAdapterError> {
    u16::try_from(unsigned(value)?).map_err(|_| FillGradientAdapterError::InvalidValue)
}

fn reference_error(error: InkScriptReferenceError) -> FillGradientAdapterError {
    match error {
        InkScriptReferenceError::MissingReference => FillGradientAdapterError::MissingReference,
        InkScriptReferenceError::InvalidReference | InkScriptReferenceError::KindMismatch => {
            FillGradientAdapterError::InvalidValue
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn fill_gradient_adapter_is_core_owned_and_thread_suitable() {
        assert_send_sync::<FillGradientScriptStep>();
        assert_send_sync::<FillGradientAdapterError>();
    }
}
