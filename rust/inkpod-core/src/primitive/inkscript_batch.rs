//! Private typed adapter for the legacy-image InkScript catalog family.

use super::CanonicalInvocation;
use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use crate::{
    BatchColorPair, BatchSeparation, BatchSeparationDestination, BoundaryAirbrush, Channel,
    ColorBalance, CurveInterpolation, CurvePoint, DustMode, DustRemoval, EditorTarget,
    FillOperation, FillRequest, Filter, HsvAdjustment, InclusionMode, Levels, PixelValue, PointF32,
    PrimitiveId, RectI32, SelectionSample, SelectionShape,
};
use inkpod_format::{
    InkScriptCommandSchema, InkScriptEnumSchema, InkScriptFieldSchema, InkScriptRecordSchema,
    InkScriptSchemaView, InkScriptSource, InkScriptSourceId, InkScriptTypeDiagnosticCode,
    InkScriptTypedStep, InkScriptTypedValue, InkScriptTypedValueKind,
    build_inkscript_declaration_model, parse_inkscript,
};
use inkpod_image::{CANONICAL_DOCUMENT_ONE, canonical_q16_from_f32, canonical_unit_u16_from_f32};
use std::collections::BTreeMap;

const ADAPTER_SOURCE_UUID: &str = "00000000-0000-0000-0000-000000000008";

pub(crate) const LEGACY_IMAGE_ENUMS: &[InkScriptEnumSchema] = &[
    InkScriptEnumSchema::new("fill_operation", &["seed", "closed_region", "extend"]),
    InkScriptEnumSchema::new(
        "fill_inclusion_mode",
        &["no_inclusion", "specified", "except_specified"],
    ),
    InkScriptEnumSchema::new("filter_channel", &["rgb", "red", "green", "blue"]),
    InkScriptEnumSchema::new("curve_interpolation", &["bezier", "b_spline"]),
    InkScriptEnumSchema::new(
        "filter_kind",
        &[
            "sharpen_weak",
            "sharpen_strong",
            "blur_weak",
            "blur_strong",
            "gaussian_blur",
            "unsharp_mask",
            "invert",
            "auto_contrast",
            "brightness_contrast",
            "tone_curve",
            "levels",
            "hsv",
            "color_balance",
        ],
    ),
    InkScriptEnumSchema::new(
        "dust_mode",
        &[
            "remove_foreground",
            "fill_transparent_holes",
            "replace_color_outliers",
        ],
    ),
    InkScriptEnumSchema::new(
        "separation_destination",
        &[
            "replace_source",
            "selection_mask",
            "main_line_plane",
            "color_plane",
            "native_file",
        ],
    ),
    InkScriptEnumSchema::new(
        "selection_shape_kind",
        &[
            "rectangle",
            "ellipse",
            "rectangle_gesture",
            "ellipse_gesture",
            "lasso",
            "polyline",
            "trace",
            "trace_brush",
            "wand",
        ],
    ),
];

const FILL_REQUEST_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("operation", "fill_operation", 0),
    InkScriptFieldSchema::required("seed_x", "u32", 1),
    InkScriptFieldSchema::required("seed_y", "u32", 2),
    InkScriptFieldSchema::required("color", "pixel_value", 3),
    InkScriptFieldSchema::required("selection", "nullable<pixel_rect>", 4),
    InkScriptFieldSchema::required("use_document_selection", "bool", 5),
    InkScriptFieldSchema::required("tolerance", "u32", 6),
    InkScriptFieldSchema::required("detached_regions", "bool", 7),
    InkScriptFieldSchema::required("overflow_abort", "bool", 8),
    InkScriptFieldSchema::required("gap_close", "u32", 9),
    InkScriptFieldSchema::required("transparent_only", "bool", 10),
    InkScriptFieldSchema::required("inclusion_mode", "fill_inclusion_mode", 11),
    InkScriptFieldSchema::required("inclusion_colors", "list<pixel_value>", 12),
    InkScriptFieldSchema::required("extension_distance", "u32", 13),
];
const CURVE_POINT_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("input", "u32", 0),
    InkScriptFieldSchema::required("output", "u32", 1),
];
const LEVELS_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("channel", "filter_channel", 0),
    InkScriptFieldSchema::required("input_shadow", "u32", 1),
    InkScriptFieldSchema::required("input_gamma_milli", "u32", 2),
    InkScriptFieldSchema::required("input_highlight", "u32", 3),
    InkScriptFieldSchema::required("output_shadow", "u32", 4),
    InkScriptFieldSchema::required("output_highlight", "u32", 5),
];
const HSV_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("hue_degrees_milli", "i32", 0),
    InkScriptFieldSchema::required("saturation_milli", "i32", 1),
    InkScriptFieldSchema::required("value_milli", "i32", 2),
];
const COLOR_BALANCE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("red_milli", "i32", 0),
    InkScriptFieldSchema::required("green_milli", "i32", 1),
    InkScriptFieldSchema::required("blue_milli", "i32", 2),
];
const FILTER_SPEC_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("kind", "filter_kind", 0),
    InkScriptFieldSchema::required("radius", "nullable<u32>", 1),
    InkScriptFieldSchema::required("strength_milli", "nullable<u32>", 2),
    InkScriptFieldSchema::required("amount_milli", "nullable<u32>", 3),
    InkScriptFieldSchema::required("threshold", "nullable<u32>", 4),
    InkScriptFieldSchema::required("channel", "nullable<filter_channel>", 5),
    InkScriptFieldSchema::required("brightness_milli", "nullable<i32>", 6),
    InkScriptFieldSchema::required("contrast_milli", "nullable<i32>", 7),
    InkScriptFieldSchema::required("interpolation", "nullable<curve_interpolation>", 8),
    InkScriptFieldSchema::required("points", "list<curve_point>", 9),
    InkScriptFieldSchema::required("levels", "nullable<filter_levels>", 10),
    InkScriptFieldSchema::required("hsv", "nullable<filter_hsv>", 11),
    InkScriptFieldSchema::required("color_balance", "nullable<filter_color_balance>", 12),
];
const BOUNDARY_AIRBRUSH_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("colors", "list<rgba16>", 0),
    InkScriptFieldSchema::required("width", "u32", 1),
    InkScriptFieldSchema::required("strength_milli", "u32", 2),
];
const DUST_REMOVAL_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("mode", "dust_mode", 0),
    InkScriptFieldSchema::required("maximum_pixels", "u32", 1),
];
const COLOR_PAIR_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("enabled", "bool", 0),
    InkScriptFieldSchema::required("old", "pixel_value", 1),
    InkScriptFieldSchema::required("new", "pixel_value", 2),
];
const SEPARATION_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("colors", "list<pixel_value>", 0),
    InkScriptFieldSchema::required("replacement", "pixel_value", 1),
    InkScriptFieldSchema::required("invert", "bool", 2),
    InkScriptFieldSchema::required("destination", "separation_destination", 3),
];
const SELECTION_SAMPLE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("x", "q16", 0),
    InkScriptFieldSchema::required("y", "q16", 1),
    InkScriptFieldSchema::required("pressure", "u32", 2),
];
const SELECTION_SHAPE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("kind", "selection_shape_kind", 0),
    InkScriptFieldSchema::required("rect", "nullable<pixel_rect>", 1),
    InkScriptFieldSchema::required("anchor", "nullable<point>", 2),
    InkScriptFieldSchema::required("current", "nullable<point>", 3),
    InkScriptFieldSchema::required("points", "list<point>", 4),
    InkScriptFieldSchema::required("samples", "list<selection_sample>", 5),
    InkScriptFieldSchema::required("diameter", "nullable<q16>", 6),
    InkScriptFieldSchema::required("x", "nullable<u32>", 7),
    InkScriptFieldSchema::required("y", "nullable<u32>", 8),
    InkScriptFieldSchema::required("tolerance", "nullable<u32>", 9),
    InkScriptFieldSchema::required("gap_close", "nullable<u32>", 10),
];

pub(crate) const LEGACY_IMAGE_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("fill_request", FILL_REQUEST_FIELDS),
    InkScriptRecordSchema::new("curve_point", CURVE_POINT_FIELDS),
    InkScriptRecordSchema::new("filter_levels", LEVELS_FIELDS),
    InkScriptRecordSchema::new("filter_hsv", HSV_FIELDS),
    InkScriptRecordSchema::new("filter_color_balance", COLOR_BALANCE_FIELDS),
    InkScriptRecordSchema::new("filter_spec", FILTER_SPEC_FIELDS),
    InkScriptRecordSchema::new("boundary_airbrush", BOUNDARY_AIRBRUSH_FIELDS),
    InkScriptRecordSchema::new("dust_removal", DUST_REMOVAL_FIELDS),
    InkScriptRecordSchema::new("color_pair", COLOR_PAIR_FIELDS),
    InkScriptRecordSchema::new("separation", SEPARATION_FIELDS),
    InkScriptRecordSchema::new("selection_sample", SELECTION_SAMPLE_FIELDS),
    InkScriptRecordSchema::new("selection_shape", SELECTION_SHAPE_FIELDS),
];

const APPLY_FILL_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("layer_id", "layer_ref", 0),
    InkScriptFieldSchema::required("plane_id", "plane_ref", 1),
    InkScriptFieldSchema::required("request", "fill_request", 2),
    InkScriptFieldSchema::required("use_light_table_boundary", "bool", 3),
    InkScriptFieldSchema::required("use_light_table_color", "bool", 4),
];
const PLANE_BOUNDARY_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("effect", "boundary_airbrush", 1),
];
const PLANE_DUST_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("shape", "nullable<selection_shape>", 1),
    InkScriptFieldSchema::required("options", "dust_removal", 2),
];
const PLANE_FILTER_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("filter", "filter_spec", 1),
];
const PLANE_PAIRS_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("pairs", "list<color_pair>", 1),
];
const PLANE_SEPARATION_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("options", "separation", 1),
];
pub(crate) const LEGACY_IMAGE_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::new("apply_fill", APPLY_FILL_FIELDS),
    InkScriptCommandSchema::new("apply_boundary_airbrush", PLANE_BOUNDARY_FIELDS),
    InkScriptCommandSchema::new("apply_dust_removal", PLANE_DUST_FIELDS),
    InkScriptCommandSchema::new("apply_filter", PLANE_FILTER_FIELDS),
    InkScriptCommandSchema::new("replace_raster_colors", PLANE_PAIRS_FIELDS),
    InkScriptCommandSchema::new("separate_raster_colors", PLANE_SEPARATION_FIELDS),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LegacyImageCatalogEntry {
    pub(crate) command: &'static str,
    pub(crate) primitive_id: PrimitiveId,
    pub(crate) semantics_revision: u16,
    pub(crate) cancellation_boundary: &'static str,
    pub(crate) legacy_projection: &'static str,
}

pub(crate) const LEGACY_IMAGE_CATALOG: &[LegacyImageCatalogEntry] = &[
    LegacyImageCatalogEntry {
        command: "apply_fill",
        primitive_id: PrimitiveId::APPLY_FILL,
        semantics_revision: 2,
        cancellation_boundary: "bounded_work_chunk",
        legacy_projection: "continuous_fill_seed",
    },
    LegacyImageCatalogEntry {
        command: "apply_boundary_airbrush",
        primitive_id: PrimitiveId::APPLY_BOUNDARY_AIRBRUSH,
        semantics_revision: 2,
        cancellation_boundary: "before_primitive",
        legacy_projection: "boundary_airbrush",
    },
    LegacyImageCatalogEntry {
        command: "apply_dust_removal",
        primitive_id: PrimitiveId::APPLY_DUST_REMOVAL,
        semantics_revision: 2,
        cancellation_boundary: "bounded_work_chunk",
        legacy_projection: "dust_removal",
    },
    LegacyImageCatalogEntry {
        command: "apply_filter",
        primitive_id: PrimitiveId::APPLY_FILTER,
        semantics_revision: 2,
        cancellation_boundary: "bounded_work_chunk",
        legacy_projection: "filter",
    },
    LegacyImageCatalogEntry {
        command: "replace_raster_colors",
        primitive_id: PrimitiveId::REPLACE_RASTER_COLORS,
        semantics_revision: 2,
        cancellation_boundary: "bounded_work_chunk",
        legacy_projection: "color_replace",
    },
    LegacyImageCatalogEntry {
        command: "separate_raster_colors",
        primitive_id: PrimitiveId::SEPARATE_RASTER_COLORS,
        semantics_revision: 3,
        cancellation_boundary: "bounded_work_chunk",
        legacy_projection: "separation",
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LegacyImageAdapterError {
    InvalidSource,
    Type(InkScriptTypeDiagnosticCode, String),
    UnsupportedPrimitive,
    UnknownCommand,
    InvalidTypedStep,
    MissingBinding,
    TargetMismatch,
    InvalidValue,
    ResourceLimit,
}

#[derive(Clone, Debug)]
pub(crate) struct LegacyImageScriptStep {
    typed: InkScriptTypedStep,
    arguments: InkScriptTypedValue,
    bindings: InkScriptRuntimeReferences,
}

impl LegacyImageScriptStep {
    pub(crate) fn from_canonical(
        invocation: &CanonicalInvocation,
        enabled: bool,
        editor_group: &str,
    ) -> Result<Self, LegacyImageAdapterError> {
        if editor_group.is_empty() {
            return Err(LegacyImageAdapterError::InvalidValue);
        }
        let (command, layer_id, plane_id, arguments) = lift_arguments(invocation)?;
        let mut source = String::from(
            "inkscript_fragment 2;\nrequires { procedure_catalog = 3; replay_epoch = 24; }\n",
        );
        let mut bindings = InkScriptRuntimeReferences::default();
        source.push_str("bindings { ");
        if let Some(layer_id) = layer_id {
            if layer_id == 0 {
                return Err(LegacyImageAdapterError::InvalidValue);
            }
            source.push_str(&format!(
                "let target_layer = select layer {{ source_document_uuid = uuid\"{ADAPTER_SOURCE_UUID}\"; persistent_id = {layer_id}; }}; "
            ));
            bindings
                .insert("target_layer", InkScriptEntityKind::Layer, layer_id)
                .map_err(reference_error)?;
        }
        if plane_id == 0 {
            return Err(LegacyImageAdapterError::InvalidValue);
        }
        source.push_str(&format!(
            "let target_plane = select plane {{ source_document_uuid = uuid\"{ADAPTER_SOURCE_UUID}\"; persistent_id = {plane_id}; }}; }}\n"
        ));
        bindings
            .insert("target_plane", InkScriptEntityKind::Plane, plane_id)
            .map_err(reference_error)?;
        source.push_str(&format!(
            "program {{ step \"Canonical LegacyImage adapter\" {{ enabled = {}; editor_group = {}; invoke {command} {{ {arguments} }}; }} }}\n",
            if enabled { "true" } else { "false" },
            string_literal(editor_group),
        ));
        Self::from_source(&source, bindings)
    }

    fn from_source(
        source: &str,
        bindings: InkScriptRuntimeReferences,
    ) -> Result<Self, LegacyImageAdapterError> {
        let source = InkScriptSource::new(InkScriptSourceId::new(8), source.as_bytes())
            .map_err(|_| LegacyImageAdapterError::InvalidSource)?;
        let parsed = parse_inkscript(&source);
        if !parsed.is_valid() {
            return Err(LegacyImageAdapterError::InvalidSource);
        }
        let schema = InkScriptSchemaView::exact_current_with_catalog(
            LEGACY_IMAGE_ENUMS,
            &[],
            LEGACY_IMAGE_RECORDS,
            LEGACY_IMAGE_COMMANDS,
        )
        .map_err(|_| LegacyImageAdapterError::InvalidValue)?;
        let model = build_inkscript_declaration_model(&parsed, &schema).map_err(|error| {
            LegacyImageAdapterError::Type(error.code(), error.path().to_owned())
        })?;
        if model.steps().len() != 1 {
            return Err(LegacyImageAdapterError::InvalidTypedStep);
        }
        Ok(Self {
            arguments: model.steps()[0].arguments().clone(),
            typed: model.steps()[0].clone(),
            bindings,
        })
    }

    pub(crate) fn from_compiled(
        typed: &InkScriptTypedStep,
        arguments: InkScriptTypedValue,
        bindings: &InkScriptRuntimeReferences,
    ) -> Result<Self, LegacyImageAdapterError> {
        Ok(Self {
            typed: typed.clone(),
            arguments,
            bindings: bindings.clone(),
        })
    }

    pub(crate) const fn enabled(&self) -> bool {
        self.typed.enabled()
    }

    pub(crate) fn editor_group(&self) -> Option<&str> {
        self.typed.editor_group()
    }

    pub(crate) fn metadata(
        &self,
    ) -> Result<&'static LegacyImageCatalogEntry, LegacyImageAdapterError> {
        LEGACY_IMAGE_CATALOG
            .iter()
            .find(|entry| entry.command == self.typed.command())
            .ok_or(LegacyImageAdapterError::UnknownCommand)
    }

    pub(crate) fn to_canonical(&self) -> Result<CanonicalInvocation, LegacyImageAdapterError> {
        let arguments = record(&self.arguments)?;
        let plane_id = binding_id(
            field(arguments, "plane_id")?,
            &self.bindings,
            InkScriptEntityKind::Plane,
        )?;
        match self.typed.command() {
            "apply_fill" => Ok(CanonicalInvocation::ApplyFill {
                request: fill_request(field(arguments, "request")?)?,
                target: EditorTarget {
                    layer_id: binding_id(
                        field(arguments, "layer_id")?,
                        &self.bindings,
                        InkScriptEntityKind::Layer,
                    )?,
                    plane_id,
                },
                use_light_table_boundary: boolean(field(arguments, "use_light_table_boundary")?)?,
                use_light_table_color: boolean(field(arguments, "use_light_table_color")?)?,
            }),
            "apply_boundary_airbrush" => Ok(CanonicalInvocation::ApplyBoundaryAirbrush {
                plane_id,
                effect: boundary_airbrush(field(arguments, "effect")?)?,
            }),
            "apply_dust_removal" => Ok(CanonicalInvocation::ApplyDustRemoval {
                plane_id,
                shape: nullable(field(arguments, "shape")?, selection_shape)?,
                options: dust_removal(field(arguments, "options")?)?,
            }),
            "apply_filter" => Ok(CanonicalInvocation::ApplyFilter {
                plane_id,
                filter: filter(field(arguments, "filter")?)?,
            }),
            "replace_raster_colors" => Ok(CanonicalInvocation::ReplaceRasterColors {
                plane_id,
                pairs: list(field(arguments, "pairs")?)?
                    .iter()
                    .map(color_pair)
                    .collect::<Result<_, _>>()?,
            }),
            "separate_raster_colors" => Ok(CanonicalInvocation::SeparateRasterColors {
                plane_id,
                options: separation(field(arguments, "options")?)?,
            }),
            _ => Err(LegacyImageAdapterError::UnknownCommand),
        }
    }
}

pub(crate) type LiftedArguments = (&'static str, Option<u64>, u64, String);

pub(crate) fn lift_arguments(
    invocation: &CanonicalInvocation,
) -> Result<LiftedArguments, LegacyImageAdapterError> {
    Ok(match invocation {
        CanonicalInvocation::ApplyFill {
            request,
            target,
            use_light_table_boundary,
            use_light_table_color,
        } => (
            "apply_fill",
            Some(target.layer_id),
            target.plane_id,
            format!(
                "layer_id = $target_layer; plane_id = $target_plane; request = {}; use_light_table_boundary = {}; use_light_table_color = {};",
                fill_request_literal(request)?,
                boolean_literal(*use_light_table_boundary),
                boolean_literal(*use_light_table_color),
            ),
        ),
        CanonicalInvocation::ApplyBoundaryAirbrush { plane_id, effect } => (
            "apply_boundary_airbrush",
            None,
            *plane_id,
            format!(
                "plane_id = $target_plane; effect = {};",
                boundary_airbrush_literal(effect)
            ),
        ),
        CanonicalInvocation::ApplyDustRemoval {
            plane_id,
            shape,
            options,
        } => (
            "apply_dust_removal",
            None,
            *plane_id,
            format!(
                "plane_id = $target_plane; shape = {}; options = {};",
                shape
                    .as_ref()
                    .map(selection_shape_literal)
                    .transpose()?
                    .unwrap_or_else(|| "none".to_owned()),
                dust_removal_literal(*options),
            ),
        ),
        CanonicalInvocation::ApplyFilter { plane_id, filter } => (
            "apply_filter",
            None,
            *plane_id,
            format!(
                "plane_id = $target_plane; filter = {};",
                filter_literal(filter)
            ),
        ),
        CanonicalInvocation::ReplaceRasterColors { plane_id, pairs } => (
            "replace_raster_colors",
            None,
            *plane_id,
            format!(
                "plane_id = $target_plane; pairs = {};",
                list_literal(pairs.iter().map(color_pair_literal))
            ),
        ),
        CanonicalInvocation::SeparateRasterColors { plane_id, options } => (
            "separate_raster_colors",
            None,
            *plane_id,
            format!(
                "plane_id = $target_plane; options = {};",
                separation_literal(options)
            ),
        ),
        _ => return Err(LegacyImageAdapterError::UnsupportedPrimitive),
    })
}

fn fill_request_literal(request: &FillRequest) -> Result<String, LegacyImageAdapterError> {
    Ok(format!(
        "{{ operation = {}; seed_x = {}; seed_y = {}; color = {}; selection = {}; use_document_selection = {}; tolerance = {}; detached_regions = {}; overflow_abort = {}; gap_close = {}; transparent_only = {}; inclusion_mode = {}; inclusion_colors = {}; extension_distance = {}; }}",
        fill_operation_name(request.operation),
        request.seed_x,
        request.seed_y,
        pixel_literal(request.color),
        request
            .selection
            .map(rect_literal)
            .transpose()?
            .unwrap_or_else(|| "none".to_owned()),
        boolean_literal(request.use_document_selection),
        request.tolerance,
        boolean_literal(request.detached_regions),
        boolean_literal(request.overflow_abort),
        request.gap_close,
        boolean_literal(request.transparent_only),
        inclusion_mode_name(request.inclusion_mode),
        list_literal(request.inclusion_colors.iter().copied().map(pixel_literal)),
        request.extension_distance,
    ))
}

fn boundary_airbrush_literal(effect: &BoundaryAirbrush) -> String {
    format!(
        "{{ colors = {}; width = {}; strength_milli = {}; }}",
        list_literal(effect.colors.iter().copied().map(rgba16_literal)),
        effect.width,
        effect.strength_milli,
    )
}

fn dust_removal_literal(options: DustRemoval) -> String {
    format!(
        "{{ mode = {}; maximum_pixels = {}; }}",
        dust_mode_name(options.mode),
        options.maximum_pixels,
    )
}

fn color_pair_literal(pair: &BatchColorPair) -> String {
    format!(
        "{{ enabled = {}; old = {}; new = {}; }}",
        boolean_literal(pair.enabled),
        pixel_literal(pair.old),
        pixel_literal(pair.new),
    )
}

fn separation_literal(options: &BatchSeparation) -> String {
    format!(
        "{{ colors = {}; replacement = {}; invert = {}; destination = {}; }}",
        list_literal(options.colors.iter().copied().map(pixel_literal)),
        pixel_literal(options.replacement),
        boolean_literal(options.invert),
        separation_destination_name(options.destination),
    )
}

fn filter_literal(filter: &Filter) -> String {
    let mut radius = "none".to_owned();
    let mut strength_milli = "none".to_owned();
    let mut amount_milli = "none".to_owned();
    let mut threshold = "none".to_owned();
    let mut channel = "none".to_owned();
    let mut brightness_milli = "none".to_owned();
    let mut contrast_milli = "none".to_owned();
    let mut interpolation = "none".to_owned();
    let mut points = "[]".to_owned();
    let mut levels = "none".to_owned();
    let mut hsv = "none".to_owned();
    let mut color_balance = "none".to_owned();
    let kind = match filter {
        Filter::SharpenWeak => "sharpen_weak",
        Filter::SharpenStrong => "sharpen_strong",
        Filter::BlurWeak => "blur_weak",
        Filter::BlurStrong => "blur_strong",
        Filter::GaussianBlur {
            radius: value,
            strength_milli: strength,
        } => {
            radius = value.to_string();
            strength_milli = strength.to_string();
            "gaussian_blur"
        }
        Filter::UnsharpMask {
            radius: value,
            amount_milli: amount,
            threshold: value_threshold,
        } => {
            radius = value.to_string();
            amount_milli = amount.to_string();
            threshold = value_threshold.to_string();
            "unsharp_mask"
        }
        Filter::Invert { channel: value } => {
            channel = channel_name(*value).to_owned();
            "invert"
        }
        Filter::AutoContrast => "auto_contrast",
        Filter::BrightnessContrast {
            brightness_milli: brightness,
            contrast_milli: contrast,
        } => {
            brightness_milli = brightness.to_string();
            contrast_milli = contrast.to_string();
            "brightness_contrast"
        }
        Filter::ToneCurve {
            channel: value_channel,
            interpolation: value_interpolation,
            points: value_points,
        } => {
            channel = channel_name(*value_channel).to_owned();
            interpolation = interpolation_name(*value_interpolation).to_owned();
            points =
                list_literal(value_points.iter().map(|point| {
                    format!("{{ input = {}; output = {}; }}", point.input, point.output)
                }));
            "tone_curve"
        }
        Filter::Levels(value) => {
            levels = format!(
                "{{ channel = {}; input_shadow = {}; input_gamma_milli = {}; input_highlight = {}; output_shadow = {}; output_highlight = {}; }}",
                channel_name(value.channel),
                value.input_shadow,
                value.input_gamma_milli,
                value.input_highlight,
                value.output_shadow,
                value.output_highlight,
            );
            "levels"
        }
        Filter::Hsv(value) => {
            hsv = format!(
                "{{ hue_degrees_milli = {}; saturation_milli = {}; value_milli = {}; }}",
                value.hue_degrees_milli, value.saturation_milli, value.value_milli,
            );
            "hsv"
        }
        Filter::ColorBalance(value) => {
            color_balance = format!(
                "{{ red_milli = {}; green_milli = {}; blue_milli = {}; }}",
                value.red_milli, value.green_milli, value.blue_milli,
            );
            "color_balance"
        }
    };
    format!(
        "{{ kind = {kind}; radius = {radius}; strength_milli = {strength_milli}; amount_milli = {amount_milli}; threshold = {threshold}; channel = {channel}; brightness_milli = {brightness_milli}; contrast_milli = {contrast_milli}; interpolation = {interpolation}; points = {points}; levels = {levels}; hsv = {hsv}; color_balance = {color_balance}; }}"
    )
}

pub(crate) fn selection_shape_literal(
    shape: &SelectionShape,
) -> Result<String, LegacyImageAdapterError> {
    let mut rect = "none".to_owned();
    let mut anchor = "none".to_owned();
    let mut current = "none".to_owned();
    let mut points = "[]".to_owned();
    let mut samples = "[]".to_owned();
    let mut diameter = "none".to_owned();
    let mut x = "none".to_owned();
    let mut y = "none".to_owned();
    let mut tolerance = "none".to_owned();
    let mut gap_close = "none".to_owned();
    let kind = match shape {
        SelectionShape::Rectangle(value) => {
            rect = rect_literal(*value)?;
            "rectangle"
        }
        SelectionShape::Ellipse(value) => {
            rect = rect_literal(*value)?;
            "ellipse"
        }
        SelectionShape::RectangleGesture {
            anchor: value_anchor,
            current: value_current,
        } => {
            anchor = point_literal(*value_anchor)?;
            current = point_literal(*value_current)?;
            "rectangle_gesture"
        }
        SelectionShape::EllipseGesture {
            anchor: value_anchor,
            current: value_current,
        } => {
            anchor = point_literal(*value_anchor)?;
            current = point_literal(*value_current)?;
            "ellipse_gesture"
        }
        SelectionShape::Lasso(value) => {
            points = point_list_literal(value)?;
            "lasso"
        }
        SelectionShape::Polyline(value) => {
            points = point_list_literal(value)?;
            "polyline"
        }
        SelectionShape::Trace {
            points: value,
            diameter: value_diameter,
        } => {
            points = point_list_literal(value)?;
            diameter = q16_literal(*value_diameter)?;
            "trace"
        }
        SelectionShape::TraceBrush {
            samples: value,
            diameter: value_diameter,
        } => {
            samples = list_literal(
                value
                    .iter()
                    .map(selection_sample_literal)
                    .collect::<Result<Vec<_>, _>>()?,
            );
            diameter = q16_literal(*value_diameter)?;
            "trace_brush"
        }
        SelectionShape::Wand {
            x: value_x,
            y: value_y,
            tolerance: value_tolerance,
            gap_close: value_gap_close,
        } => {
            x = value_x.to_string();
            y = value_y.to_string();
            tolerance = value_tolerance.to_string();
            gap_close = value_gap_close.to_string();
            "wand"
        }
    };
    Ok(format!(
        "{{ kind = {kind}; rect = {rect}; anchor = {anchor}; current = {current}; points = {points}; samples = {samples}; diameter = {diameter}; x = {x}; y = {y}; tolerance = {tolerance}; gap_close = {gap_close}; }}"
    ))
}

fn point_list_literal(points: &[PointF32]) -> Result<String, LegacyImageAdapterError> {
    Ok(list_literal(
        points
            .iter()
            .copied()
            .map(point_literal)
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

fn point_literal(point: PointF32) -> Result<String, LegacyImageAdapterError> {
    Ok(format!(
        "point({}, {})",
        q16_literal(point.x)?,
        q16_literal(point.y)?
    ))
}

fn selection_sample_literal(sample: &SelectionSample) -> Result<String, LegacyImageAdapterError> {
    let pressure = canonical_unit_u16_from_f32(sample.pressure)
        .ok_or(LegacyImageAdapterError::InvalidValue)?;
    Ok(format!(
        "{{ x = {}; y = {}; pressure = {}; }}",
        q16_literal(sample.x)?,
        q16_literal(sample.y)?,
        pressure,
    ))
}

pub(crate) fn q16_literal(value: f32) -> Result<String, LegacyImageAdapterError> {
    canonical_q16_from_f32(value)
        .map(|raw| format!("q16({raw})"))
        .ok_or(LegacyImageAdapterError::InvalidValue)
}

fn rect_literal(rect: RectI32) -> Result<String, LegacyImageAdapterError> {
    let width = u32::try_from(rect.width).map_err(|_| LegacyImageAdapterError::InvalidValue)?;
    let height = u32::try_from(rect.height).map_err(|_| LegacyImageAdapterError::InvalidValue)?;
    Ok(format!("rect({}, {}, {width}, {height})", rect.x, rect.y))
}

pub(crate) fn pixel_literal(value: PixelValue) -> String {
    match value {
        PixelValue::Binary(value) => format!("mask8({value})"),
        PixelValue::Grayscale8(value) => format!("gray8({value})"),
        PixelValue::Grayscale16(value) => format!("gray16({value})"),
        PixelValue::Rgba(value) => format!(
            "rgba8({}, {}, {}, {})",
            value[0], value[1], value[2], value[3]
        ),
        PixelValue::Rgba16(value) => rgba16_literal(value),
    }
}

pub(crate) fn rgba16_literal(value: [u16; 4]) -> String {
    format!(
        "rgba16({}, {}, {}, {})",
        value[0], value[1], value[2], value[3]
    )
}

fn list_literal(values: impl IntoIterator<Item = String>) -> String {
    let values = values.into_iter().collect::<Vec<_>>();
    if values.is_empty() {
        "[]".to_owned()
    } else {
        format!("[{},]", values.join(", "))
    }
}

const fn boolean_literal(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn string_literal(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ => result.push(character),
        }
    }
    result.push('"');
    result
}

const fn fill_operation_name(value: FillOperation) -> &'static str {
    match value {
        FillOperation::Seed => "seed",
        FillOperation::ClosedRegion => "closed_region",
        FillOperation::Extend => "extend",
    }
}
const fn inclusion_mode_name(value: InclusionMode) -> &'static str {
    match value {
        InclusionMode::None => "no_inclusion",
        InclusionMode::Specified => "specified",
        InclusionMode::ExceptSpecified => "except_specified",
    }
}
const fn channel_name(value: Channel) -> &'static str {
    match value {
        Channel::Rgb => "rgb",
        Channel::Red => "red",
        Channel::Green => "green",
        Channel::Blue => "blue",
    }
}
const fn interpolation_name(value: CurveInterpolation) -> &'static str {
    match value {
        CurveInterpolation::Bezier => "bezier",
        CurveInterpolation::BSpline => "b_spline",
    }
}
const fn dust_mode_name(value: DustMode) -> &'static str {
    match value {
        DustMode::RemoveForeground => "remove_foreground",
        DustMode::FillTransparentHoles => "fill_transparent_holes",
        DustMode::ReplaceColorOutliers => "replace_color_outliers",
    }
}
const fn separation_destination_name(value: BatchSeparationDestination) -> &'static str {
    match value {
        BatchSeparationDestination::ReplaceSource => "replace_source",
        BatchSeparationDestination::SelectionMask => "selection_mask",
        BatchSeparationDestination::MainLinePlane => "main_line_plane",
        BatchSeparationDestination::ColorPlane => "color_plane",
        BatchSeparationDestination::NativeFile => "native_file",
    }
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, LegacyImageAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(LegacyImageAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, LegacyImageAdapterError> {
    fields
        .get(name)
        .ok_or(LegacyImageAdapterError::InvalidTypedStep)
}

fn list(value: &InkScriptTypedValue) -> Result<&[InkScriptTypedValue], LegacyImageAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::List(values) => Ok(values),
        _ => Err(LegacyImageAdapterError::InvalidTypedStep),
    }
}

fn binding_id(
    value: &InkScriptTypedValue,
    bindings: &InkScriptRuntimeReferences,
    expected: InkScriptEntityKind,
) -> Result<u64, LegacyImageAdapterError> {
    bindings.resolve(value, expected).map_err(reference_error)
}

fn reference_error(error: InkScriptReferenceError) -> LegacyImageAdapterError {
    match error {
        InkScriptReferenceError::InvalidReference => LegacyImageAdapterError::InvalidTypedStep,
        InkScriptReferenceError::MissingReference => LegacyImageAdapterError::MissingBinding,
        InkScriptReferenceError::KindMismatch => LegacyImageAdapterError::TargetMismatch,
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, LegacyImageAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(LegacyImageAdapterError::InvalidTypedStep),
    }
}

fn u32_value(value: &InkScriptTypedValue) -> Result<u32, LegacyImageAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(LegacyImageAdapterError::InvalidTypedStep),
    }
}

fn i32_value(value: &InkScriptTypedValue) -> Result<i32, LegacyImageAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::I32(value) => Ok(*value),
        _ => Err(LegacyImageAdapterError::InvalidTypedStep),
    }
}

fn q16_value(value: &InkScriptTypedValue) -> Result<f32, LegacyImageAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Q16(value) => Ok(*value as f32 / CANONICAL_DOCUMENT_ONE as f32),
        _ => Err(LegacyImageAdapterError::InvalidTypedStep),
    }
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, LegacyImageAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(LegacyImageAdapterError::InvalidTypedStep),
    }
}

fn nullable<T>(
    value: &InkScriptTypedValue,
    parse: impl FnOnce(&InkScriptTypedValue) -> Result<T, LegacyImageAdapterError>,
) -> Result<Option<T>, LegacyImageAdapterError> {
    if matches!(value.kind(), InkScriptTypedValueKind::None) {
        Ok(None)
    } else {
        parse(value).map(Some)
    }
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected: &str,
) -> Result<&'a [InkScriptTypedValue], LegacyImageAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments } if name == expected => {
            Ok(arguments)
        }
        _ => Err(LegacyImageAdapterError::InvalidValue),
    }
}

pub(crate) fn pixel(value: &InkScriptTypedValue) -> Result<PixelValue, LegacyImageAdapterError> {
    match value.type_name() {
        "mask8" => Ok(PixelValue::Binary(narrow_u8(
            &constructor(value, "mask8")?[0],
        )?)),
        "gray8" => Ok(PixelValue::Grayscale8(narrow_u8(
            &constructor(value, "gray8")?[0],
        )?)),
        "gray16" => Ok(PixelValue::Grayscale16(narrow_u16(
            &constructor(value, "gray16")?[0],
        )?)),
        "rgba8" => {
            let values = constructor(value, "rgba8")?;
            Ok(PixelValue::Rgba([
                narrow_u8(&values[0])?,
                narrow_u8(&values[1])?,
                narrow_u8(&values[2])?,
                narrow_u8(&values[3])?,
            ]))
        }
        "rgba16" => Ok(PixelValue::Rgba16(rgba16(value)?)),
        _ => Err(LegacyImageAdapterError::InvalidValue),
    }
}

fn rgba16(value: &InkScriptTypedValue) -> Result<[u16; 4], LegacyImageAdapterError> {
    let values = constructor(value, "rgba16")?;
    Ok([
        narrow_u16(&values[0])?,
        narrow_u16(&values[1])?,
        narrow_u16(&values[2])?,
        narrow_u16(&values[3])?,
    ])
}

fn narrow_u8(value: &InkScriptTypedValue) -> Result<u8, LegacyImageAdapterError> {
    u8::try_from(u32_value(value)?).map_err(|_| LegacyImageAdapterError::InvalidValue)
}

fn narrow_u16(value: &InkScriptTypedValue) -> Result<u16, LegacyImageAdapterError> {
    u16::try_from(u32_value(value)?).map_err(|_| LegacyImageAdapterError::InvalidValue)
}

fn fill_request(value: &InkScriptTypedValue) -> Result<FillRequest, LegacyImageAdapterError> {
    if value.type_name() != "fill_request" {
        return Err(LegacyImageAdapterError::InvalidTypedStep);
    }
    let fields = record(value)?;
    Ok(FillRequest {
        operation: match enum_value(field(fields, "operation")?)? {
            "seed" => FillOperation::Seed,
            "closed_region" => FillOperation::ClosedRegion,
            "extend" => FillOperation::Extend,
            _ => return Err(LegacyImageAdapterError::InvalidValue),
        },
        seed_x: u32_value(field(fields, "seed_x")?)?,
        seed_y: u32_value(field(fields, "seed_y")?)?,
        color: pixel(field(fields, "color")?)?,
        selection: nullable(field(fields, "selection")?, rect)?,
        use_document_selection: boolean(field(fields, "use_document_selection")?)?,
        tolerance: narrow_u16(field(fields, "tolerance")?)?,
        detached_regions: boolean(field(fields, "detached_regions")?)?,
        overflow_abort: boolean(field(fields, "overflow_abort")?)?,
        gap_close: narrow_u8(field(fields, "gap_close")?)?,
        transparent_only: boolean(field(fields, "transparent_only")?)?,
        inclusion_mode: match enum_value(field(fields, "inclusion_mode")?)? {
            "no_inclusion" => InclusionMode::None,
            "specified" => InclusionMode::Specified,
            "except_specified" => InclusionMode::ExceptSpecified,
            _ => return Err(LegacyImageAdapterError::InvalidValue),
        },
        inclusion_colors: list(field(fields, "inclusion_colors")?)?
            .iter()
            .map(pixel)
            .collect::<Result<_, _>>()?,
        extension_distance: u32_value(field(fields, "extension_distance")?)?,
    })
}

fn rect(value: &InkScriptTypedValue) -> Result<RectI32, LegacyImageAdapterError> {
    let values = constructor(value, "rect")?;
    Ok(RectI32 {
        x: i32_value(&values[0])?,
        y: i32_value(&values[1])?,
        width: i32::try_from(u32_value(&values[2])?)
            .map_err(|_| LegacyImageAdapterError::InvalidValue)?,
        height: i32::try_from(u32_value(&values[3])?)
            .map_err(|_| LegacyImageAdapterError::InvalidValue)?,
    })
}

fn point(value: &InkScriptTypedValue) -> Result<PointF32, LegacyImageAdapterError> {
    let values = constructor(value, "point")?;
    Ok(PointF32 {
        x: q16_value(&values[0])?,
        y: q16_value(&values[1])?,
    })
}

fn boundary_airbrush(
    value: &InkScriptTypedValue,
) -> Result<BoundaryAirbrush, LegacyImageAdapterError> {
    let fields = record(value)?;
    Ok(BoundaryAirbrush {
        colors: list(field(fields, "colors")?)?
            .iter()
            .map(rgba16)
            .collect::<Result<_, _>>()?,
        width: u32_value(field(fields, "width")?)?,
        strength_milli: u32_value(field(fields, "strength_milli")?)?,
    })
}

fn dust_removal(value: &InkScriptTypedValue) -> Result<DustRemoval, LegacyImageAdapterError> {
    let fields = record(value)?;
    Ok(DustRemoval {
        mode: match enum_value(field(fields, "mode")?)? {
            "remove_foreground" => DustMode::RemoveForeground,
            "fill_transparent_holes" => DustMode::FillTransparentHoles,
            "replace_color_outliers" => DustMode::ReplaceColorOutliers,
            _ => return Err(LegacyImageAdapterError::InvalidValue),
        },
        maximum_pixels: u32_value(field(fields, "maximum_pixels")?)?,
    })
}

fn color_pair(value: &InkScriptTypedValue) -> Result<BatchColorPair, LegacyImageAdapterError> {
    let fields = record(value)?;
    Ok(BatchColorPair {
        enabled: boolean(field(fields, "enabled")?)?,
        old: pixel(field(fields, "old")?)?,
        new: pixel(field(fields, "new")?)?,
    })
}

fn separation(value: &InkScriptTypedValue) -> Result<BatchSeparation, LegacyImageAdapterError> {
    let fields = record(value)?;
    Ok(BatchSeparation {
        colors: list(field(fields, "colors")?)?
            .iter()
            .map(pixel)
            .collect::<Result<_, _>>()?,
        replacement: pixel(field(fields, "replacement")?)?,
        invert: boolean(field(fields, "invert")?)?,
        destination: match enum_value(field(fields, "destination")?)? {
            "replace_source" => BatchSeparationDestination::ReplaceSource,
            "selection_mask" => BatchSeparationDestination::SelectionMask,
            "main_line_plane" => BatchSeparationDestination::MainLinePlane,
            "color_plane" => BatchSeparationDestination::ColorPlane,
            "native_file" => BatchSeparationDestination::NativeFile,
            _ => return Err(LegacyImageAdapterError::InvalidValue),
        },
    })
}

fn filter(value: &InkScriptTypedValue) -> Result<Filter, LegacyImageAdapterError> {
    let fields = record(value)?;
    let radius = nullable(field(fields, "radius")?, u32_value)?;
    let strength = nullable(field(fields, "strength_milli")?, u32_value)?;
    let amount = nullable(field(fields, "amount_milli")?, u32_value)?;
    let threshold = nullable(field(fields, "threshold")?, narrow_u16)?;
    let channel = nullable(field(fields, "channel")?, filter_channel)?;
    let brightness = nullable(field(fields, "brightness_milli")?, i32_value)?;
    let contrast = nullable(field(fields, "contrast_milli")?, i32_value)?;
    let interpolation = nullable(field(fields, "interpolation")?, curve_interpolation)?;
    let points = list(field(fields, "points")?)?
        .iter()
        .map(curve_point)
        .collect::<Result<Vec<_>, _>>()?;
    let levels = nullable(field(fields, "levels")?, filter_levels)?;
    let hsv = nullable(field(fields, "hsv")?, filter_hsv)?;
    let color_balance = nullable(field(fields, "color_balance")?, filter_color_balance)?;

    let all_none = || {
        radius.is_none()
            && strength.is_none()
            && amount.is_none()
            && threshold.is_none()
            && channel.is_none()
            && brightness.is_none()
            && contrast.is_none()
            && interpolation.is_none()
            && points.is_empty()
            && levels.is_none()
            && hsv.is_none()
            && color_balance.is_none()
    };
    Ok(match enum_value(field(fields, "kind")?)? {
        "sharpen_weak" if all_none() => Filter::SharpenWeak,
        "sharpen_strong" if all_none() => Filter::SharpenStrong,
        "blur_weak" if all_none() => Filter::BlurWeak,
        "blur_strong" if all_none() => Filter::BlurStrong,
        "gaussian_blur"
            if radius.is_some()
                && strength.is_some()
                && amount.is_none()
                && threshold.is_none()
                && channel.is_none()
                && brightness.is_none()
                && contrast.is_none()
                && interpolation.is_none()
                && points.is_empty()
                && levels.is_none()
                && hsv.is_none()
                && color_balance.is_none() =>
        {
            Filter::GaussianBlur {
                radius: radius.unwrap(),
                strength_milli: strength.unwrap(),
            }
        }
        "unsharp_mask"
            if radius.is_some()
                && amount.is_some()
                && threshold.is_some()
                && strength.is_none()
                && channel.is_none()
                && brightness.is_none()
                && contrast.is_none()
                && interpolation.is_none()
                && points.is_empty()
                && levels.is_none()
                && hsv.is_none()
                && color_balance.is_none() =>
        {
            Filter::UnsharpMask {
                radius: radius.unwrap(),
                amount_milli: amount.unwrap(),
                threshold: threshold.unwrap(),
            }
        }
        "invert"
            if channel.is_some()
                && radius.is_none()
                && strength.is_none()
                && amount.is_none()
                && threshold.is_none()
                && brightness.is_none()
                && contrast.is_none()
                && interpolation.is_none()
                && points.is_empty()
                && levels.is_none()
                && hsv.is_none()
                && color_balance.is_none() =>
        {
            Filter::Invert {
                channel: channel.unwrap(),
            }
        }
        "auto_contrast" if all_none() => Filter::AutoContrast,
        "brightness_contrast"
            if brightness.is_some()
                && contrast.is_some()
                && radius.is_none()
                && strength.is_none()
                && amount.is_none()
                && threshold.is_none()
                && channel.is_none()
                && interpolation.is_none()
                && points.is_empty()
                && levels.is_none()
                && hsv.is_none()
                && color_balance.is_none() =>
        {
            Filter::BrightnessContrast {
                brightness_milli: brightness.unwrap(),
                contrast_milli: contrast.unwrap(),
            }
        }
        "tone_curve"
            if channel.is_some()
                && interpolation.is_some()
                && !points.is_empty()
                && radius.is_none()
                && strength.is_none()
                && amount.is_none()
                && threshold.is_none()
                && brightness.is_none()
                && contrast.is_none()
                && levels.is_none()
                && hsv.is_none()
                && color_balance.is_none() =>
        {
            Filter::ToneCurve {
                channel: channel.unwrap(),
                interpolation: interpolation.unwrap(),
                points,
            }
        }
        "levels"
            if levels.is_some()
                && radius.is_none()
                && strength.is_none()
                && amount.is_none()
                && threshold.is_none()
                && channel.is_none()
                && brightness.is_none()
                && contrast.is_none()
                && interpolation.is_none()
                && points.is_empty()
                && hsv.is_none()
                && color_balance.is_none() =>
        {
            Filter::Levels(levels.unwrap())
        }
        "hsv"
            if hsv.is_some()
                && radius.is_none()
                && strength.is_none()
                && amount.is_none()
                && threshold.is_none()
                && channel.is_none()
                && brightness.is_none()
                && contrast.is_none()
                && interpolation.is_none()
                && points.is_empty()
                && levels.is_none()
                && color_balance.is_none() =>
        {
            Filter::Hsv(hsv.unwrap())
        }
        "color_balance"
            if color_balance.is_some()
                && radius.is_none()
                && strength.is_none()
                && amount.is_none()
                && threshold.is_none()
                && channel.is_none()
                && brightness.is_none()
                && contrast.is_none()
                && interpolation.is_none()
                && points.is_empty()
                && levels.is_none()
                && hsv.is_none() =>
        {
            Filter::ColorBalance(color_balance.unwrap())
        }
        _ => return Err(LegacyImageAdapterError::InvalidValue),
    })
}

pub(crate) fn filter_channel(
    value: &InkScriptTypedValue,
) -> Result<Channel, LegacyImageAdapterError> {
    match enum_value(value)? {
        "rgb" => Ok(Channel::Rgb),
        "red" => Ok(Channel::Red),
        "green" => Ok(Channel::Green),
        "blue" => Ok(Channel::Blue),
        _ => Err(LegacyImageAdapterError::InvalidValue),
    }
}

pub(crate) fn curve_interpolation(
    value: &InkScriptTypedValue,
) -> Result<CurveInterpolation, LegacyImageAdapterError> {
    match enum_value(value)? {
        "bezier" => Ok(CurveInterpolation::Bezier),
        "b_spline" => Ok(CurveInterpolation::BSpline),
        _ => Err(LegacyImageAdapterError::InvalidValue),
    }
}

pub(crate) fn curve_point(
    value: &InkScriptTypedValue,
) -> Result<CurvePoint, LegacyImageAdapterError> {
    let fields = record(value)?;
    Ok(CurvePoint {
        input: narrow_u16(field(fields, "input")?)?,
        output: narrow_u16(field(fields, "output")?)?,
    })
}

pub(crate) fn filter_levels(
    value: &InkScriptTypedValue,
) -> Result<Levels, LegacyImageAdapterError> {
    let fields = record(value)?;
    Ok(Levels {
        channel: filter_channel(field(fields, "channel")?)?,
        input_shadow: narrow_u16(field(fields, "input_shadow")?)?,
        input_gamma_milli: u32_value(field(fields, "input_gamma_milli")?)?,
        input_highlight: narrow_u16(field(fields, "input_highlight")?)?,
        output_shadow: narrow_u16(field(fields, "output_shadow")?)?,
        output_highlight: narrow_u16(field(fields, "output_highlight")?)?,
    })
}

fn filter_hsv(value: &InkScriptTypedValue) -> Result<HsvAdjustment, LegacyImageAdapterError> {
    let fields = record(value)?;
    Ok(HsvAdjustment {
        hue_degrees_milli: i32_value(field(fields, "hue_degrees_milli")?)?,
        saturation_milli: i32_value(field(fields, "saturation_milli")?)?,
        value_milli: i32_value(field(fields, "value_milli")?)?,
    })
}

fn filter_color_balance(
    value: &InkScriptTypedValue,
) -> Result<ColorBalance, LegacyImageAdapterError> {
    let fields = record(value)?;
    Ok(ColorBalance {
        red_milli: i32_value(field(fields, "red_milli")?)?,
        green_milli: i32_value(field(fields, "green_milli")?)?,
        blue_milli: i32_value(field(fields, "blue_milli")?)?,
    })
}

pub(crate) fn selection_shape(
    value: &InkScriptTypedValue,
) -> Result<SelectionShape, LegacyImageAdapterError> {
    let fields = record(value)?;
    let rect_value = nullable(field(fields, "rect")?, rect)?;
    let anchor = nullable(field(fields, "anchor")?, point)?;
    let current = nullable(field(fields, "current")?, point)?;
    let points = list(field(fields, "points")?)?
        .iter()
        .map(point)
        .collect::<Result<Vec<_>, _>>()?;
    let samples = list(field(fields, "samples")?)?
        .iter()
        .map(selection_sample)
        .collect::<Result<Vec<_>, _>>()?;
    let diameter = nullable(field(fields, "diameter")?, q16_value)?;
    let x = nullable(field(fields, "x")?, u32_value)?;
    let y = nullable(field(fields, "y")?, u32_value)?;
    let tolerance = nullable(field(fields, "tolerance")?, narrow_u16)?;
    let gap_close = nullable(field(fields, "gap_close")?, narrow_u8)?;

    Ok(match enum_value(field(fields, "kind")?)? {
        "rectangle"
            if selection_shape_exact(
                &[rect_value.is_some()],
                &[
                    anchor.is_none(),
                    current.is_none(),
                    points.is_empty(),
                    samples.is_empty(),
                    diameter.is_none(),
                    x.is_none(),
                    y.is_none(),
                    tolerance.is_none(),
                    gap_close.is_none(),
                ],
            ) =>
        {
            SelectionShape::Rectangle(rect_value.unwrap())
        }
        "ellipse"
            if selection_shape_exact(
                &[rect_value.is_some()],
                &[
                    anchor.is_none(),
                    current.is_none(),
                    points.is_empty(),
                    samples.is_empty(),
                    diameter.is_none(),
                    x.is_none(),
                    y.is_none(),
                    tolerance.is_none(),
                    gap_close.is_none(),
                ],
            ) =>
        {
            SelectionShape::Ellipse(rect_value.unwrap())
        }
        "rectangle_gesture"
            if selection_shape_exact(
                &[anchor.is_some(), current.is_some()],
                &[
                    rect_value.is_none(),
                    points.is_empty(),
                    samples.is_empty(),
                    diameter.is_none(),
                    x.is_none(),
                    y.is_none(),
                    tolerance.is_none(),
                    gap_close.is_none(),
                ],
            ) =>
        {
            SelectionShape::RectangleGesture {
                anchor: anchor.unwrap(),
                current: current.unwrap(),
            }
        }
        "ellipse_gesture"
            if selection_shape_exact(
                &[anchor.is_some(), current.is_some()],
                &[
                    rect_value.is_none(),
                    points.is_empty(),
                    samples.is_empty(),
                    diameter.is_none(),
                    x.is_none(),
                    y.is_none(),
                    tolerance.is_none(),
                    gap_close.is_none(),
                ],
            ) =>
        {
            SelectionShape::EllipseGesture {
                anchor: anchor.unwrap(),
                current: current.unwrap(),
            }
        }
        "lasso"
            if !points.is_empty()
                && selection_shape_exact(
                    &[],
                    &[
                        rect_value.is_none(),
                        anchor.is_none(),
                        current.is_none(),
                        samples.is_empty(),
                        diameter.is_none(),
                        x.is_none(),
                        y.is_none(),
                        tolerance.is_none(),
                        gap_close.is_none(),
                    ],
                ) =>
        {
            SelectionShape::Lasso(points)
        }
        "polyline"
            if !points.is_empty()
                && selection_shape_exact(
                    &[],
                    &[
                        rect_value.is_none(),
                        anchor.is_none(),
                        current.is_none(),
                        samples.is_empty(),
                        diameter.is_none(),
                        x.is_none(),
                        y.is_none(),
                        tolerance.is_none(),
                        gap_close.is_none(),
                    ],
                ) =>
        {
            SelectionShape::Polyline(points)
        }
        "trace"
            if !points.is_empty()
                && diameter.is_some()
                && selection_shape_exact(
                    &[],
                    &[
                        rect_value.is_none(),
                        anchor.is_none(),
                        current.is_none(),
                        samples.is_empty(),
                        x.is_none(),
                        y.is_none(),
                        tolerance.is_none(),
                        gap_close.is_none(),
                    ],
                ) =>
        {
            SelectionShape::Trace {
                points,
                diameter: diameter.unwrap(),
            }
        }
        "trace_brush"
            if !samples.is_empty()
                && diameter.is_some()
                && selection_shape_exact(
                    &[],
                    &[
                        rect_value.is_none(),
                        anchor.is_none(),
                        current.is_none(),
                        points.is_empty(),
                        x.is_none(),
                        y.is_none(),
                        tolerance.is_none(),
                        gap_close.is_none(),
                    ],
                ) =>
        {
            SelectionShape::TraceBrush {
                samples,
                diameter: diameter.unwrap(),
            }
        }
        "wand"
            if x.is_some()
                && y.is_some()
                && tolerance.is_some()
                && gap_close.is_some()
                && selection_shape_exact(
                    &[],
                    &[
                        rect_value.is_none(),
                        anchor.is_none(),
                        current.is_none(),
                        points.is_empty(),
                        samples.is_empty(),
                        diameter.is_none(),
                    ],
                ) =>
        {
            SelectionShape::Wand {
                x: x.unwrap(),
                y: y.unwrap(),
                tolerance: tolerance.unwrap(),
                gap_close: gap_close.unwrap(),
            }
        }
        _ => return Err(LegacyImageAdapterError::InvalidValue),
    })
}

fn selection_shape_exact(required: &[bool], unused: &[bool]) -> bool {
    required.iter().all(|value| *value) && unused.iter().all(|value| *value)
}

fn selection_sample(
    value: &InkScriptTypedValue,
) -> Result<SelectionSample, LegacyImageAdapterError> {
    let fields = record(value)?;
    let pressure = narrow_u16(field(fields, "pressure")?)?;
    Ok(SelectionSample {
        x: q16_value(field(fields, "x")?)?,
        y: q16_value(field(fields, "y")?)?,
        pressure: f32::from(pressure) / f32::from(u16::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invocations() -> Vec<CanonicalInvocation> {
        vec![
            CanonicalInvocation::ApplyFill {
                request: FillRequest {
                    operation: FillOperation::ClosedRegion,
                    seed_x: 3,
                    seed_y: 4,
                    color: PixelValue::Rgba16([257, 514, 771, u16::MAX]),
                    selection: Some(RectI32 {
                        x: 1,
                        y: 2,
                        width: 5,
                        height: 6,
                    }),
                    use_document_selection: true,
                    tolerance: 123,
                    detached_regions: true,
                    overflow_abort: true,
                    gap_close: 7,
                    transparent_only: true,
                    inclusion_mode: InclusionMode::Specified,
                    inclusion_colors: vec![PixelValue::Rgba16([1, 2, 3, 4])],
                    extension_distance: 9,
                },
                target: EditorTarget {
                    layer_id: 11,
                    plane_id: 12,
                },
                use_light_table_boundary: true,
                use_light_table_color: false,
            },
            CanonicalInvocation::ApplyBoundaryAirbrush {
                plane_id: 12,
                effect: BoundaryAirbrush {
                    colors: vec![[0, 0, 0, u16::MAX], [u16::MAX; 4]],
                    width: 3,
                    strength_milli: 750,
                },
            },
            CanonicalInvocation::ApplyDustRemoval {
                plane_id: 12,
                shape: Some(SelectionShape::TraceBrush {
                    samples: vec![SelectionSample {
                        x: 1.5,
                        y: 2.25,
                        pressure: 1.0,
                    }],
                    diameter: 3.5,
                }),
                options: DustRemoval {
                    mode: DustMode::ReplaceColorOutliers,
                    maximum_pixels: 16,
                },
            },
            CanonicalInvocation::ApplyFilter {
                plane_id: 12,
                filter: Filter::ToneCurve {
                    channel: Channel::Blue,
                    interpolation: CurveInterpolation::BSpline,
                    points: vec![
                        CurvePoint {
                            input: 0,
                            output: 1,
                        },
                        CurvePoint {
                            input: u16::MAX,
                            output: u16::MAX - 1,
                        },
                    ],
                },
            },
            CanonicalInvocation::ReplaceRasterColors {
                plane_id: 12,
                pairs: vec![BatchColorPair {
                    enabled: false,
                    old: PixelValue::Grayscale16(1),
                    new: PixelValue::Grayscale16(2),
                }],
            },
            CanonicalInvocation::SeparateRasterColors {
                plane_id: 12,
                options: BatchSeparation {
                    colors: vec![PixelValue::Rgba16([1, 2, 3, 4])],
                    replacement: PixelValue::Rgba16([5, 6, 7, 8]),
                    invert: true,
                    destination: BatchSeparationDestination::NativeFile,
                },
            },
        ]
    }

    #[test]
    fn exact_catalog_codec_covers_all_m08_primitives_and_full_typed_payloads() {
        assert_eq!(LEGACY_IMAGE_CATALOG.len(), 6);
        for (index, invocation) in invocations().iter().enumerate() {
            let step = LegacyImageScriptStep::from_canonical(
                invocation,
                index % 2 == 0,
                &format!("image_{index}"),
            )
            .unwrap();
            assert_eq!(step.to_canonical().unwrap(), *invocation, "fixture {index}");
            assert_eq!(step.enabled(), index % 2 == 0);
            assert_eq!(step.editor_group(), Some(format!("image_{index}").as_str()));
            let metadata = step.metadata().unwrap();
            assert_eq!(metadata.primitive_id, invocation.primitive_id());
            assert_eq!(metadata.semantics_revision, if index == 5 { 3 } else { 2 });
            assert!(!metadata.cancellation_boundary.is_empty());
            assert!(!metadata.legacy_projection.is_empty());
        }
    }

    #[test]
    fn unknown_field_enum_and_nonexact_filter_variant_are_rejected() {
        let prefix = format!(
            "inkscript_fragment 2; requires {{ procedure_catalog = 3; replay_epoch = 24; }} bindings {{ let target_plane = select plane {{ source_document_uuid = uuid\"{ADAPTER_SOURCE_UUID}\"; persistent_id = 12; }}; }} "
        );
        let fields = "radius = none; strength_milli = none; amount_milli = none; threshold = none; channel = none; brightness_milli = none; contrast_milli = none; interpolation = none; points = []; levels = none; hsv = none; color_balance = none;";
        let unknown_enum = format!(
            "{prefix} program {{ step \"Bad\" {{ enabled = true; editor_group = \"bad\"; invoke apply_filter {{ plane_id = $target_plane; filter = {{ kind = diagonal; {fields} }}; }}; }} }}"
        );
        assert!(matches!(
            LegacyImageScriptStep::from_source(&unknown_enum, plane_references(12),),
            Err(LegacyImageAdapterError::Type(
                InkScriptTypeDiagnosticCode::ValueOutOfRange,
                _
            ))
        ));

        let unknown_field = format!(
            "{prefix} program {{ step \"Bad\" {{ enabled = true; editor_group = \"bad\"; invoke apply_filter {{ plane_id = $target_plane; filter = {{ kind = sharpen_weak; {fields} extra = true; }}; }}; }} }}"
        );
        assert!(matches!(
            LegacyImageScriptStep::from_source(&unknown_field, plane_references(12),),
            Err(LegacyImageAdapterError::Type(
                InkScriptTypeDiagnosticCode::InvalidSemanticModel,
                _
            ))
        ));

        let nonexact = format!(
            "{prefix} program {{ step \"Bad\" {{ enabled = true; editor_group = \"bad\"; invoke apply_filter {{ plane_id = $target_plane; filter = {{ kind = sharpen_weak; radius = 2; strength_milli = none; amount_milli = none; threshold = none; channel = none; brightness_milli = none; contrast_milli = none; interpolation = none; points = []; levels = none; hsv = none; color_balance = none; }}; }}; }} }}"
        );
        let step = LegacyImageScriptStep::from_source(&nonexact, plane_references(12)).unwrap();
        assert_eq!(
            step.to_canonical(),
            Err(LegacyImageAdapterError::InvalidValue)
        );
    }

    #[test]
    fn private_image_catalog_values_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LegacyImageScriptStep>();
        assert_send_sync::<LegacyImageCatalogEntry>();
        assert_send_sync::<LegacyImageAdapterError>();
    }

    fn plane_references(id: u64) -> InkScriptRuntimeReferences {
        let mut references = InkScriptRuntimeReferences::default();
        references
            .insert("target_plane", InkScriptEntityKind::Plane, id)
            .unwrap();
        references
    }
}
