//! Private pre-ratification InkScript adapter for selection and floating primitives.

use super::CanonicalInvocation;
use super::inkscript_batch;
use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use crate::history::PixelChange;
use crate::identity::{LayerId, PlaneId};
use crate::selection::{FloatingDestination, FloatingSelection};
use crate::{
    ClipboardPayload, ClipboardPixel, ClipboardPlane, EditorTarget, FloatingTransform,
    FloatingTransformAnchor, MAX_FILL_PIXELS, OutputColorGuardProfile, PixelFormat, PlaneType,
    RangeInterpretation, RectI32, SavedSelectionId, SavedSelectionOperation,
    SelectionConstructionOptions, SelectionOperation, TileRaster, TraceBrushOptions,
    TraceBrushShape,
};
use inkpod_format::{
    InkScriptCommandResultSchema, InkScriptCommandSchema, InkScriptEnumSchema,
    InkScriptFieldSchema, InkScriptRecordSchema, InkScriptResultAvailability, InkScriptTypedStep,
    InkScriptTypedValue, InkScriptTypedValueKind,
};
use std::collections::BTreeMap;

const MAX_FLOATING_PLANES: usize = 4_096;

pub(crate) const SELECTION_FLOATING_ENUMS: &[InkScriptEnumSchema] = &[
    InkScriptEnumSchema::new(
        "selection_operation",
        &["new", "add", "subtract", "intersect"],
    ),
    InkScriptEnumSchema::new(
        "range_interpretation",
        &[
            "normal",
            "tight",
            "enclosed_interior",
            "drawing",
            "boundary",
        ],
    ),
    InkScriptEnumSchema::new("trace_brush_shape", &["round", "square"]),
    InkScriptEnumSchema::new("saved_selection_operation", &["replace", "add", "subtract"]),
    InkScriptEnumSchema::new("output_color_guard_profile", &["bt709_conservative_ycbcr"]),
    InkScriptEnumSchema::new(
        "floating_destination_kind",
        &["existing_planes", "new_plane"],
    ),
    InkScriptEnumSchema::new(
        "floating_anchor",
        &[
            "top_left",
            "top_right",
            "center",
            "bottom_left",
            "bottom_right",
        ],
    ),
];

const PIXEL_CHANGE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("x", "u32", 0),
    InkScriptFieldSchema::required("y", "u32", 1),
    InkScriptFieldSchema::required("before", "pixel_value", 2),
    InkScriptFieldSchema::required("after", "pixel_value", 3),
];
const TRACE_OPTIONS_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("shape", "trace_brush_shape", 0),
    InkScriptFieldSchema::required("pressure_size", "bool", 1),
    InkScriptFieldSchema::required("screen_size", "bool", 2),
    InkScriptFieldSchema::required("view_zoom", "q16", 3),
];
const SELECTION_OPTIONS_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("aspect_ratio_q16", "u32", 0),
    InkScriptFieldSchema::required("from_center", "bool", 1),
    InkScriptFieldSchema::required("constrain_rotation_45", "bool", 2),
    InkScriptFieldSchema::required("rotation_turns", "u32", 3),
    InkScriptFieldSchema::required("trace", "selection_trace_options", 4),
];
const FLOATING_PLANE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("kind", "plane_kind", 0),
    InkScriptFieldSchema::required("pixel_format", "pixel_format", 1),
    InkScriptFieldSchema::required("origin_x", "i32", 2),
    InkScriptFieldSchema::required("origin_y", "i32", 3),
    InkScriptFieldSchema::required("raster", "asset_ref", 4),
];
const FLOATING_TRANSFORM_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("anchor", "floating_anchor", 0),
    InkScriptFieldSchema::required("target_x", "q16", 1),
    InkScriptFieldSchema::required("target_y", "q16", 2),
    InkScriptFieldSchema::required("scale_x", "q16", 3),
    InkScriptFieldSchema::required("scale_y", "q16", 4),
    InkScriptFieldSchema::required("rotation_turns", "u32", 5),
];
const FLOATING_DESTINATION_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("kind", "floating_destination_kind", 0),
    InkScriptFieldSchema::required("existing_plane_ids", "list<plane_ref>", 1),
    InkScriptFieldSchema::required("new_layer_id", "nullable<layer_ref>", 2),
    InkScriptFieldSchema::required("new_plane_kind", "nullable<plane_kind>", 3),
    InkScriptFieldSchema::required("new_pixel_format", "nullable<pixel_format>", 4),
    InkScriptFieldSchema::required("new_name", "nullable<string>", 5),
    InkScriptFieldSchema::required("new_opacity_milli", "nullable<u32>", 6),
];
const FLOATING_PAYLOAD_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("source_document_uuid", "uuid", 0),
    InkScriptFieldSchema::required("bounds", "pixel_rect", 1),
    InkScriptFieldSchema::required("planes", "list<floating_plane>", 2),
];

pub(crate) const SELECTION_FLOATING_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("selection_pixel_change", PIXEL_CHANGE_FIELDS),
    InkScriptRecordSchema::new("selection_trace_options", TRACE_OPTIONS_FIELDS),
    InkScriptRecordSchema::new("selection_construction_options", SELECTION_OPTIONS_FIELDS),
    InkScriptRecordSchema::new("floating_plane", FLOATING_PLANE_FIELDS),
    InkScriptRecordSchema::new("floating_transform", FLOATING_TRANSFORM_FIELDS),
    InkScriptRecordSchema::new("floating_destination", FLOATING_DESTINATION_FIELDS),
    InkScriptRecordSchema::new("floating_payload", FLOATING_PAYLOAD_FIELDS),
];

const RESTORE_SELECTED_PIXELS_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("changes", "list<selection_pixel_change>", 1),
];
const APPLY_SELECTION_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("shape", "selection_shape", 0),
    InkScriptFieldSchema::required("operation", "selection_operation", 1),
    InkScriptFieldSchema::required("interpretation", "range_interpretation", 2),
    InkScriptFieldSchema::required("options", "selection_construction_options", 3),
    InkScriptFieldSchema::required("target_layer_id", "layer_ref", 4),
    InkScriptFieldSchema::required("target_plane_id", "plane_ref", 5),
];
const EMPTY_FIELDS: &[InkScriptFieldSchema] = &[];
const RESIZE_SELECTION_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("pixels", "i32", 0)];
const SELECT_COLOR_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("color", "pixel_value", 0),
    InkScriptFieldSchema::required("tolerance", "u32", 1),
    InkScriptFieldSchema::required("different", "bool", 2),
    InkScriptFieldSchema::required("operation", "selection_operation", 3),
    InkScriptFieldSchema::required("target_layer_id", "layer_ref", 4),
    InkScriptFieldSchema::required("target_plane_id", "plane_ref", 5),
];
const SELECT_OUTPUT_COLOR_GUARD_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("profile", "output_color_guard_profile", 0),
    InkScriptFieldSchema::required("operation", "selection_operation", 1),
    InkScriptFieldSchema::required("base_revision", "u64", 2),
];
const SAVE_SELECTION_MASK_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("name", "string", 0)];
const APPLY_SAVED_SELECTION_MASK_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("saved_selection_id", "saved_selection_mask_ref", 0),
    InkScriptFieldSchema::required("operation", "saved_selection_operation", 1),
];
const RENAME_SAVED_SELECTION_MASK_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("saved_selection_id", "saved_selection_mask_ref", 0),
    InkScriptFieldSchema::required("name", "string", 1),
];
const DELETE_SAVED_SELECTION_MASK_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required(
        "saved_selection_id",
        "saved_selection_mask_ref",
        0,
    )];
const CLEAR_SELECTED_CONTENT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("target_layer_id", "layer_ref", 0),
    InkScriptFieldSchema::required("target_plane_id", "plane_ref", 1),
];
const COMMIT_FLOATING_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("payload", "floating_payload", 0),
    InkScriptFieldSchema::required("destination", "floating_destination", 1),
    InkScriptFieldSchema::required("transform", "floating_transform", 2),
];
const SAVE_SELECTION_MASK_RESULTS: &[InkScriptCommandResultSchema] =
    &[InkScriptCommandResultSchema::scalar(
        "saved_selection_mask",
        "saved_selection_mask_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    )];

pub(crate) const SELECTION_FLOATING_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::new("restore_selected_pixels", RESTORE_SELECTED_PIXELS_FIELDS),
    InkScriptCommandSchema::new("apply_selection", APPLY_SELECTION_FIELDS),
    InkScriptCommandSchema::new("invert_selection", EMPTY_FIELDS),
    InkScriptCommandSchema::new("clear_selection", EMPTY_FIELDS),
    InkScriptCommandSchema::new("resize_selection", RESIZE_SELECTION_FIELDS),
    InkScriptCommandSchema::new("select_color", SELECT_COLOR_FIELDS),
    InkScriptCommandSchema::new(
        "select_output_color_guard",
        SELECT_OUTPUT_COLOR_GUARD_FIELDS,
    ),
    InkScriptCommandSchema::with_results(
        "save_selection_mask",
        SAVE_SELECTION_MASK_FIELDS,
        SAVE_SELECTION_MASK_RESULTS,
    ),
    InkScriptCommandSchema::new(
        "apply_saved_selection_mask",
        APPLY_SAVED_SELECTION_MASK_FIELDS,
    ),
    InkScriptCommandSchema::new(
        "rename_saved_selection_mask",
        RENAME_SAVED_SELECTION_MASK_FIELDS,
    ),
    InkScriptCommandSchema::new(
        "delete_saved_selection_mask",
        DELETE_SAVED_SELECTION_MASK_FIELDS,
    ),
    InkScriptCommandSchema::new("clear_selected_content", CLEAR_SELECTED_CONTENT_FIELDS),
    InkScriptCommandSchema::new("commit_floating", COMMIT_FLOATING_FIELDS),
];

#[derive(Clone, Debug, PartialEq)]
struct FloatingPlaneSpec {
    kind: PlaneType,
    pixel_format: PixelFormat,
    origin_x: i32,
    origin_y: i32,
    asset_symbol: String,
}

#[derive(Clone, Debug, PartialEq)]
enum FloatingDestinationSpec {
    ExistingPlanes(Vec<u64>),
    NewPlane {
        layer_id: u64,
        kind: PlaneType,
        format: PixelFormat,
        name: String,
        opacity_milli: u32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FloatingSpec {
    source_document_uuid: u128,
    bounds: RectI32,
    planes: Vec<FloatingPlaneSpec>,
    destination: FloatingDestinationSpec,
    transform: FloatingTransform,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SelectionFloatingScriptAction {
    Canonical(CanonicalInvocation),
    CommitFloating(FloatingSpec),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionFloatingAdapterError {
    InvalidTypedStep,
    InvalidValue,
    MissingReference,
    ResourceLimit,
    UnsupportedPrimitive,
}

impl SelectionFloatingScriptAction {
    pub(crate) fn from_compiled(
        step: &InkScriptTypedStep,
        arguments: &InkScriptTypedValue,
        bindings: &InkScriptRuntimeReferences,
    ) -> Result<Self, SelectionFloatingAdapterError> {
        let fields = record(arguments)?;
        let target = |layer: &str, plane: &str| {
            Ok(EditorTarget {
                layer_id: reference(field(fields, layer)?, bindings, InkScriptEntityKind::Layer)?,
                plane_id: reference(field(fields, plane)?, bindings, InkScriptEntityKind::Plane)?,
            })
        };
        let invocation = match step.command() {
            "restore_selected_pixels" => CanonicalInvocation::RestoreSelectedPixels {
                plane_id: reference(
                    field(fields, "plane_id")?,
                    bindings,
                    InkScriptEntityKind::Plane,
                )?,
                changes: list(field(fields, "changes")?)?
                    .iter()
                    .map(pixel_change)
                    .collect::<Result<Vec<_>, _>>()?,
            },
            "apply_selection" => CanonicalInvocation::ApplySelection {
                shape: inkscript_batch::selection_shape(field(fields, "shape")?)
                    .map_err(legacy_image_error)?,
                operation: selection_operation(field(fields, "operation")?)?,
                interpretation: range_interpretation(field(fields, "interpretation")?)?,
                options: selection_options(field(fields, "options")?)?,
                target: target("target_layer_id", "target_plane_id")?,
            },
            "invert_selection" => CanonicalInvocation::InvertSelection,
            "clear_selection" => CanonicalInvocation::ClearSelection,
            "resize_selection" => CanonicalInvocation::ResizeSelection {
                pixels: signed(field(fields, "pixels")?)?,
            },
            "select_color" => CanonicalInvocation::SelectColor {
                color: inkscript_batch::pixel(field(fields, "color")?)
                    .map_err(legacy_image_error)?,
                tolerance: u16::try_from(unsigned(field(fields, "tolerance")?)?)
                    .map_err(|_| SelectionFloatingAdapterError::InvalidValue)?,
                different: boolean(field(fields, "different")?)?,
                operation: selection_operation(field(fields, "operation")?)?,
                target: target("target_layer_id", "target_plane_id")?,
            },
            "select_output_color_guard" => CanonicalInvocation::SelectOutputColorGuard {
                profile: output_color_guard_profile(field(fields, "profile")?)?,
                operation: selection_operation(field(fields, "operation")?)?,
                base_revision: unsigned64(field(fields, "base_revision")?)?,
            },
            "save_selection_mask" => CanonicalInvocation::SaveSelectionMask {
                name: string(field(fields, "name")?)?.to_owned(),
            },
            "apply_saved_selection_mask" => CanonicalInvocation::ApplySavedSelectionMask {
                saved_selection_id: saved_selection_id(
                    field(fields, "saved_selection_id")?,
                    bindings,
                )?,
                operation: saved_selection_operation(field(fields, "operation")?)?,
            },
            "rename_saved_selection_mask" => CanonicalInvocation::RenameSavedSelectionMask {
                saved_selection_id: saved_selection_id(
                    field(fields, "saved_selection_id")?,
                    bindings,
                )?,
                name: string(field(fields, "name")?)?.to_owned(),
            },
            "delete_saved_selection_mask" => CanonicalInvocation::DeleteSavedSelectionMask {
                saved_selection_id: saved_selection_id(
                    field(fields, "saved_selection_id")?,
                    bindings,
                )?,
            },
            "clear_selected_content" => CanonicalInvocation::ClearSelectedContent {
                target: target("target_layer_id", "target_plane_id")?,
            },
            "commit_floating" => {
                return Ok(Self::CommitFloating(floating_spec(
                    field(fields, "payload")?,
                    field(fields, "destination")?,
                    field(fields, "transform")?,
                    bindings,
                )?));
            }
            _ => return Err(SelectionFloatingAdapterError::UnsupportedPrimitive),
        };
        Ok(Self::Canonical(invocation))
    }

    pub(crate) fn asset_symbols(&self) -> Vec<&str> {
        match self {
            Self::Canonical(_) => Vec::new(),
            Self::CommitFloating(spec) => spec
                .planes
                .iter()
                .map(|plane| plane.asset_symbol.as_str())
                .collect(),
        }
    }

    pub(crate) fn to_canonical_with_rasters(
        &self,
        rasters: &[&TileRaster],
    ) -> Result<CanonicalInvocation, SelectionFloatingAdapterError> {
        match self {
            Self::Canonical(invocation) if rasters.is_empty() => Ok(invocation.clone()),
            Self::Canonical(_) => Err(SelectionFloatingAdapterError::InvalidValue),
            Self::CommitFloating(spec) => build_floating(spec, rasters),
        }
    }

    pub(crate) fn output_entity_kinds(
        &self,
        output_count: usize,
    ) -> Result<Vec<InkScriptEntityKind>, SelectionFloatingAdapterError> {
        match self {
            Self::Canonical(CanonicalInvocation::SaveSelectionMask { .. }) if output_count == 1 => {
                Ok(vec![InkScriptEntityKind::SavedSelectionMask])
            }
            Self::Canonical(CanonicalInvocation::SaveSelectionMask { .. }) => {
                Err(SelectionFloatingAdapterError::InvalidValue)
            }
            Self::Canonical(_) | Self::CommitFloating(_) if output_count == 0 => Ok(Vec::new()),
            Self::Canonical(_) | Self::CommitFloating(_) => {
                Err(SelectionFloatingAdapterError::InvalidValue)
            }
        }
    }
}

fn floating_spec(
    payload: &InkScriptTypedValue,
    destination: &InkScriptTypedValue,
    transform: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<FloatingSpec, SelectionFloatingAdapterError> {
    let payload = record(payload)?;
    let source_document_uuid = uuid(field(payload, "source_document_uuid")?)?;
    let bounds = rectangle(field(payload, "bounds")?)?;
    if bounds.width <= 0 || bounds.height <= 0 {
        return Err(SelectionFloatingAdapterError::InvalidValue);
    }
    let plane_values = list(field(payload, "planes")?)?;
    if plane_values.is_empty() || plane_values.len() > MAX_FLOATING_PLANES {
        return Err(SelectionFloatingAdapterError::ResourceLimit);
    }
    let planes = plane_values
        .iter()
        .map(floating_plane)
        .collect::<Result<Vec<_>, _>>()?;
    let destination = floating_destination(destination, bindings)?;
    let transform = floating_transform(transform)?;
    Ok(FloatingSpec {
        source_document_uuid,
        bounds,
        planes,
        destination,
        transform,
    })
}

fn floating_plane(
    value: &InkScriptTypedValue,
) -> Result<FloatingPlaneSpec, SelectionFloatingAdapterError> {
    let fields = record(value)?;
    Ok(FloatingPlaneSpec {
        kind: plane_kind(field(fields, "kind")?)?,
        pixel_format: pixel_format(field(fields, "pixel_format")?)?,
        origin_x: signed(field(fields, "origin_x")?)?,
        origin_y: signed(field(fields, "origin_y")?)?,
        asset_symbol: asset_reference(field(fields, "raster")?)?.to_owned(),
    })
}

fn floating_destination(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<FloatingDestinationSpec, SelectionFloatingAdapterError> {
    let fields = record(value)?;
    let existing = list(field(fields, "existing_plane_ids")?)?
        .iter()
        .map(|value| reference(value, bindings, InkScriptEntityKind::Plane))
        .collect::<Result<Vec<_>, _>>()?;
    let layer = nullable(field(fields, "new_layer_id")?, |value| {
        reference(value, bindings, InkScriptEntityKind::Layer)
    })?;
    let plane = nullable(field(fields, "new_plane_kind")?, plane_kind)?;
    let format = nullable(field(fields, "new_pixel_format")?, pixel_format)?;
    let name = nullable(field(fields, "new_name")?, |value| {
        string(value).map(str::to_owned)
    })?;
    let opacity = nullable(field(fields, "new_opacity_milli")?, unsigned)?;
    match enum_value(field(fields, "kind")?)? {
        "existing_planes"
            if !existing.is_empty()
                && layer.is_none()
                && plane.is_none()
                && format.is_none()
                && name.is_none()
                && opacity.is_none() =>
        {
            Ok(FloatingDestinationSpec::ExistingPlanes(existing))
        }
        "new_plane"
            if existing.is_empty()
                && layer.is_some()
                && plane.is_some()
                && format.is_some()
                && name.is_some()
                && opacity.is_some_and(|value| value <= 1_000) =>
        {
            Ok(FloatingDestinationSpec::NewPlane {
                layer_id: layer.unwrap(),
                kind: plane.unwrap(),
                format: format.unwrap(),
                name: name.unwrap(),
                opacity_milli: opacity.unwrap(),
            })
        }
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn floating_transform(
    value: &InkScriptTypedValue,
) -> Result<FloatingTransform, SelectionFloatingAdapterError> {
    let fields = record(value)?;
    let scale_x = q16(field(fields, "scale_x")?)?;
    let scale_y = q16(field(fields, "scale_y")?)?;
    if scale_x <= 0 || scale_y <= 0 {
        return Err(SelectionFloatingAdapterError::InvalidValue);
    }
    let turns = unsigned(field(fields, "rotation_turns")?)?;
    Ok(FloatingTransform {
        anchor: match enum_value(field(fields, "anchor")?)? {
            "top_left" => FloatingTransformAnchor::TopLeft,
            "top_right" => FloatingTransformAnchor::TopRight,
            "center" => FloatingTransformAnchor::Center,
            "bottom_left" => FloatingTransformAnchor::BottomLeft,
            "bottom_right" => FloatingTransformAnchor::BottomRight,
            _ => return Err(SelectionFloatingAdapterError::InvalidValue),
        },
        target_x: q16(field(fields, "target_x")?)? as f64 / 65_536.0,
        target_y: q16(field(fields, "target_y")?)? as f64 / 65_536.0,
        scale_x: scale_x as f64 / 65_536.0,
        scale_y: scale_y as f64 / 65_536.0,
        rotation_degrees: f64::from(turns) * 360.0 / 4_294_967_296.0,
    })
}

fn build_floating(
    spec: &FloatingSpec,
    rasters: &[&TileRaster],
) -> Result<CanonicalInvocation, SelectionFloatingAdapterError> {
    if rasters.len() != spec.planes.len() {
        return Err(SelectionFloatingAdapterError::InvalidValue);
    }
    let width = u32::try_from(spec.bounds.width)
        .map_err(|_| SelectionFloatingAdapterError::InvalidValue)?;
    let height = u32::try_from(spec.bounds.height)
        .map_err(|_| SelectionFloatingAdapterError::InvalidValue)?;
    let work = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(SelectionFloatingAdapterError::ResourceLimit)?;
    if work > MAX_FILL_PIXELS {
        return Err(SelectionFloatingAdapterError::ResourceLimit);
    }
    let mut planes = Vec::new();
    planes
        .try_reserve_exact(spec.planes.len())
        .map_err(|_| SelectionFloatingAdapterError::ResourceLimit)?;
    for (plane, raster) in spec.planes.iter().zip(rasters) {
        if plane.origin_x != spec.bounds.x
            || plane.origin_y != spec.bounds.y
            || raster.width() != width
            || raster.height() != height
            || raster.format() != plane.pixel_format
        {
            return Err(SelectionFloatingAdapterError::InvalidValue);
        }
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let value = raster
                    .pixel(x, y)
                    .map_err(|_| SelectionFloatingAdapterError::InvalidValue)?;
                if value.is_zero() {
                    continue;
                }
                if pixels.len() == pixels.capacity() {
                    pixels
                        .try_reserve(1_024)
                        .map_err(|_| SelectionFloatingAdapterError::ResourceLimit)?;
                }
                pixels.push(ClipboardPixel {
                    x: plane
                        .origin_x
                        .checked_add(
                            i32::try_from(x)
                                .map_err(|_| SelectionFloatingAdapterError::InvalidValue)?,
                        )
                        .ok_or(SelectionFloatingAdapterError::InvalidValue)?,
                    y: plane
                        .origin_y
                        .checked_add(
                            i32::try_from(y)
                                .map_err(|_| SelectionFloatingAdapterError::InvalidValue)?,
                        )
                        .ok_or(SelectionFloatingAdapterError::InvalidValue)?,
                    value,
                });
            }
        }
        planes.push(ClipboardPlane {
            kind: plane.kind,
            pixel_format: plane.pixel_format,
            origin_x: plane.origin_x,
            origin_y: plane.origin_y,
            pixels,
        });
    }
    let destination = match &spec.destination {
        FloatingDestinationSpec::ExistingPlanes(ids) if ids.len() == planes.len() => {
            FloatingDestination::ExistingPlanes(
                ids.iter().copied().map(PlaneId::from_raw).collect(),
            )
        }
        FloatingDestinationSpec::NewPlane {
            layer_id,
            kind,
            format,
            name,
            opacity_milli,
        } if planes.len() == 1 && planes[0].kind == *kind && planes[0].pixel_format == *format => {
            FloatingDestination::NewPlane {
                layer_id: LayerId::from_raw(*layer_id),
                kind: *kind,
                format: *format,
                name: name.clone(),
                opacity_milli: *opacity_milli,
            }
        }
        _ => return Err(SelectionFloatingAdapterError::InvalidValue),
    };
    Ok(CanonicalInvocation::CommitFloating {
        floating: FloatingSelection {
            payload: ClipboardPayload {
                source_document_uuid: spec.source_document_uuid,
                bounds: spec.bounds,
                planes,
            },
            destination,
            transform: spec.transform,
            asset_ids: Vec::new(),
        },
    })
}

fn pixel_change(value: &InkScriptTypedValue) -> Result<PixelChange, SelectionFloatingAdapterError> {
    let fields = record(value)?;
    Ok(PixelChange {
        x: unsigned(field(fields, "x")?)?,
        y: unsigned(field(fields, "y")?)?,
        before: inkscript_batch::pixel(field(fields, "before")?).map_err(legacy_image_error)?,
        after: inkscript_batch::pixel(field(fields, "after")?).map_err(legacy_image_error)?,
    })
}

fn selection_options(
    value: &InkScriptTypedValue,
) -> Result<SelectionConstructionOptions, SelectionFloatingAdapterError> {
    let fields = record(value)?;
    let trace = record(field(fields, "trace")?)?;
    let view_zoom_q16 = q16(field(trace, "view_zoom")?)?;
    if view_zoom_q16 <= 0 {
        return Err(SelectionFloatingAdapterError::InvalidValue);
    }
    Ok(SelectionConstructionOptions {
        aspect_ratio_q16: unsigned(field(fields, "aspect_ratio_q16")?)?,
        from_center: boolean(field(fields, "from_center")?)?,
        constrain_rotation_45: boolean(field(fields, "constrain_rotation_45")?)?,
        rotation_turns: unsigned(field(fields, "rotation_turns")?)?,
        trace: TraceBrushOptions {
            shape: match enum_value(field(trace, "shape")?)? {
                "round" => TraceBrushShape::Round,
                "square" => TraceBrushShape::Square,
                _ => return Err(SelectionFloatingAdapterError::InvalidValue),
            },
            pressure_size: boolean(field(trace, "pressure_size")?)?,
            screen_size: boolean(field(trace, "screen_size")?)?,
            view_zoom_q16,
        },
    })
}

fn selection_operation(
    value: &InkScriptTypedValue,
) -> Result<SelectionOperation, SelectionFloatingAdapterError> {
    match enum_value(value)? {
        "new" => Ok(SelectionOperation::New),
        "add" => Ok(SelectionOperation::Add),
        "subtract" => Ok(SelectionOperation::Subtract),
        "intersect" => Ok(SelectionOperation::Intersect),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn range_interpretation(
    value: &InkScriptTypedValue,
) -> Result<RangeInterpretation, SelectionFloatingAdapterError> {
    match enum_value(value)? {
        "normal" => Ok(RangeInterpretation::Normal),
        "tight" => Ok(RangeInterpretation::Tight),
        "enclosed_interior" => Ok(RangeInterpretation::EnclosedInterior),
        "drawing" => Ok(RangeInterpretation::Drawing),
        "boundary" => Ok(RangeInterpretation::Boundary),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn saved_selection_operation(
    value: &InkScriptTypedValue,
) -> Result<SavedSelectionOperation, SelectionFloatingAdapterError> {
    match enum_value(value)? {
        "replace" => Ok(SavedSelectionOperation::Replace),
        "add" => Ok(SavedSelectionOperation::Add),
        "subtract" => Ok(SavedSelectionOperation::Subtract),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn output_color_guard_profile(
    value: &InkScriptTypedValue,
) -> Result<OutputColorGuardProfile, SelectionFloatingAdapterError> {
    match enum_value(value)? {
        "bt709_conservative_ycbcr" => Ok(OutputColorGuardProfile::Bt709ConservativeYCbCr),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn plane_kind(value: &InkScriptTypedValue) -> Result<PlaneType, SelectionFloatingAdapterError> {
    match enum_value(value)? {
        "main_line" => Ok(PlaneType::MainLine),
        "color" => Ok(PlaneType::Color),
        "raster" => Ok(PlaneType::Raster),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn pixel_format(value: &InkScriptTypedValue) -> Result<PixelFormat, SelectionFloatingAdapterError> {
    match enum_value(value)? {
        "mask8" => Ok(PixelFormat::BinaryMask8),
        "gray8" => Ok(PixelFormat::Grayscale8),
        "gray16" => Ok(PixelFormat::Grayscale16),
        "rgba8" => Ok(PixelFormat::StraightRgba8),
        "rgba16" => Ok(PixelFormat::StraightRgba16),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn rectangle(value: &InkScriptTypedValue) -> Result<RectI32, SelectionFloatingAdapterError> {
    let values = constructor(value, "rect")?;
    Ok(RectI32 {
        x: signed(&values[0])?,
        y: signed(&values[1])?,
        width: i32::try_from(unsigned(&values[2])?)
            .map_err(|_| SelectionFloatingAdapterError::InvalidValue)?,
        height: i32::try_from(unsigned(&values[3])?)
            .map_err(|_| SelectionFloatingAdapterError::InvalidValue)?,
    })
}

fn uuid(value: &InkScriptTypedValue) -> Result<u128, SelectionFloatingAdapterError> {
    let InkScriptTypedValueKind::Uuid(value) = value.kind() else {
        return Err(SelectionFloatingAdapterError::InvalidValue);
    };
    let compact = value.replace('-', "");
    if compact.len() != 32 {
        return Err(SelectionFloatingAdapterError::InvalidValue);
    }
    u128::from_str_radix(&compact, 16).map_err(|_| SelectionFloatingAdapterError::InvalidValue)
}

fn reference(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
    kind: InkScriptEntityKind,
) -> Result<u64, SelectionFloatingAdapterError> {
    bindings.resolve(value, kind).map_err(reference_error)
}

fn saved_selection_id(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
) -> Result<SavedSelectionId, SelectionFloatingAdapterError> {
    SavedSelectionId::from_raw(reference(
        value,
        bindings,
        InkScriptEntityKind::SavedSelectionMask,
    )?)
    .ok_or(SelectionFloatingAdapterError::InvalidValue)
}

fn reference_error(error: InkScriptReferenceError) -> SelectionFloatingAdapterError {
    match error {
        InkScriptReferenceError::MissingReference => {
            SelectionFloatingAdapterError::MissingReference
        }
        InkScriptReferenceError::InvalidReference | InkScriptReferenceError::KindMismatch => {
            SelectionFloatingAdapterError::InvalidValue
        }
    }
}

fn legacy_image_error(
    error: inkscript_batch::LegacyImageAdapterError,
) -> SelectionFloatingAdapterError {
    match error {
        inkscript_batch::LegacyImageAdapterError::MissingBinding => {
            SelectionFloatingAdapterError::MissingReference
        }
        inkscript_batch::LegacyImageAdapterError::ResourceLimit => {
            SelectionFloatingAdapterError::ResourceLimit
        }
        _ => SelectionFloatingAdapterError::InvalidValue,
    }
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(SelectionFloatingAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, SelectionFloatingAdapterError> {
    fields
        .get(name)
        .ok_or(SelectionFloatingAdapterError::InvalidTypedStep)
}

fn list(
    value: &InkScriptTypedValue,
) -> Result<&[InkScriptTypedValue], SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::List(values) => Ok(values),
        _ => Err(SelectionFloatingAdapterError::InvalidTypedStep),
    }
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected: &str,
) -> Result<&'a [InkScriptTypedValue], SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments }
            if name == expected && arguments.len() == 4 =>
        {
            Ok(arguments)
        }
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn nullable<T>(
    value: &InkScriptTypedValue,
    convert: impl FnOnce(&InkScriptTypedValue) -> Result<T, SelectionFloatingAdapterError>,
) -> Result<Option<T>, SelectionFloatingAdapterError> {
    if matches!(value.kind(), InkScriptTypedValueKind::None) {
        Ok(None)
    } else {
        convert(value).map(Some)
    }
}

fn asset_reference(value: &InkScriptTypedValue) -> Result<&str, SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::AssetReference(value) => Ok(value),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn string(value: &InkScriptTypedValue) -> Result<&str, SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::String(value) => Ok(value),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn signed(value: &InkScriptTypedValue) -> Result<i32, SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::I32(value) => Ok(*value),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn unsigned(value: &InkScriptTypedValue) -> Result<u32, SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn unsigned64(value: &InkScriptTypedValue) -> Result<u64, SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U64(value) => Ok(*value),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

fn q16(value: &InkScriptTypedValue) -> Result<i64, SelectionFloatingAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Q16(value) => Ok(*value),
        _ => Err(SelectionFloatingAdapterError::InvalidValue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn selection_floating_adapter_is_core_owned_and_thread_suitable() {
        assert_send_sync::<SelectionFloatingScriptAction>();
        assert_send_sync::<SelectionFloatingAdapterError>();
    }
}
