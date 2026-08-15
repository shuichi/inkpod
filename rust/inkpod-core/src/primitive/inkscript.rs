//! Private pre-ratification adapters for the LegacySimple legacy-simple InkScript catalog family.

use super::CanonicalInvocation;
use crate::{
    DocumentResize, LayerKind, MAX_FILL_PIXELS, MirrorAxis, PixelFormat, PlaneType, PrimitiveId,
    ResizeAnchor, RotateDirection,
};
use inkpod_format::{
    InkScriptCommandSchema, InkScriptEnumSchema, InkScriptFieldSchema, InkScriptRecordSchema,
    InkScriptSchemaView, InkScriptSource, InkScriptSourceId, InkScriptTypeDiagnosticCode,
    InkScriptTypedStep, InkScriptTypedValue, InkScriptTypedValueKind,
    build_inkscript_declaration_model, parse_inkscript,
};
use std::collections::BTreeMap;

const ADAPTER_SOURCE_UUID: &str = "00000000-0000-0000-0000-000000000007";
const MAX_NODE_NAME_BYTES: usize = 1_024;

pub(crate) const LEGACY_SIMPLE_ENUMS: &[InkScriptEnumSchema] = &[
    InkScriptEnumSchema::new("mirror_axis", &["horizontal", "vertical"]),
    InkScriptEnumSchema::new("rotate_direction", &["left_90", "right_90"]),
    InkScriptEnumSchema::new(
        "resize_anchor",
        &[
            "top_left",
            "top_right",
            "center",
            "bottom_left",
            "bottom_right",
        ],
    ),
];

const DOCUMENT_RESIZE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("width", "u32", 0),
    InkScriptFieldSchema::required("height", "u32", 1),
    InkScriptFieldSchema::required("dpi_x_milli", "u32", 2),
    InkScriptFieldSchema::required("dpi_y_milli", "u32", 3),
    InkScriptFieldSchema::required("anchor", "resize_anchor", 4),
    InkScriptFieldSchema::required("resample", "bool", 5),
];
pub(crate) const LEGACY_SIMPLE_RECORDS: &[InkScriptRecordSchema] = &[InkScriptRecordSchema::new(
    "document_resize",
    DOCUMENT_RESIZE_FIELDS,
)];

const SET_LAYER_PROPERTIES_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("layer_id", "layer_ref", 0),
    InkScriptFieldSchema::required("visible", "bool", 1),
    InkScriptFieldSchema::required("editable", "bool", 2),
    InkScriptFieldSchema::required("opacity_milli", "u32", 3),
    InkScriptFieldSchema::required("name", "string", 4),
];
const SET_PLANE_PROPERTIES_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("visible", "bool", 1),
    InkScriptFieldSchema::required("editable", "bool", 2),
    InkScriptFieldSchema::required("opacity_milli", "u32", 3),
    InkScriptFieldSchema::required("name", "string", 4),
];
const CONVERT_PLANE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("plane_id", "plane_ref", 0),
    InkScriptFieldSchema::required("destination_kind", "plane_kind", 1),
    InkScriptFieldSchema::required("destination_format", "pixel_format", 2),
];
const CONVERT_LAYER_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("layer_id", "layer_ref", 0),
    InkScriptFieldSchema::required("destination", "layer_kind", 1),
];
const MIRROR_DOCUMENT_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("axis", "mirror_axis", 0)];
const ROTATE_DOCUMENT_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "direction",
    "rotate_direction",
    0,
)];
const RESIZE_DOCUMENT_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "resize",
    "document_resize",
    0,
)];

pub(crate) const LEGACY_SIMPLE_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::new("set_layer_properties", SET_LAYER_PROPERTIES_FIELDS),
    InkScriptCommandSchema::new("set_plane_properties", SET_PLANE_PROPERTIES_FIELDS),
    InkScriptCommandSchema::new("convert_plane", CONVERT_PLANE_FIELDS),
    InkScriptCommandSchema::new("convert_layer", CONVERT_LAYER_FIELDS),
    InkScriptCommandSchema::new("mirror_document", MIRROR_DOCUMENT_FIELDS),
    InkScriptCommandSchema::new("rotate_document", ROTATE_DOCUMENT_FIELDS),
    InkScriptCommandSchema::new("resize_document", RESIZE_DOCUMENT_FIELDS),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LegacySimplePortability {
    Portable,
    RequiresBinding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LegacySimpleWorkFormula {
    Constant(u64),
    ResizePixels,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LegacySimpleCatalogEntry {
    command: &'static str,
    primitive_id: PrimitiveId,
    primitive_schema_version: u16,
    semantics_revision: u16,
    portability: LegacySimplePortability,
    work: LegacySimpleWorkFormula,
    legacy_projection: Option<&'static str>,
    allow_skip_dependents: bool,
    result_count: u16,
    asset_count: u16,
}

macro_rules! legacy_simple_entry {
    ($command:literal, $primitive:expr, $portability:ident, $work:expr, $projection:expr, $skip:literal) => {
        LegacySimpleCatalogEntry {
            command: $command,
            primitive_id: $primitive,
            primitive_schema_version: 2,
            semantics_revision: 2,
            portability: LegacySimplePortability::$portability,
            work: $work,
            legacy_projection: $projection,
            allow_skip_dependents: $skip,
            result_count: 0,
            asset_count: 0,
        }
    };
}

const LEGACY_SIMPLE_CATALOG: &[LegacySimpleCatalogEntry] = &[
    legacy_simple_entry!(
        "set_layer_properties",
        PrimitiveId::SET_LAYER_PROPERTIES,
        RequiresBinding,
        LegacySimpleWorkFormula::Constant(1),
        Some("layer_property"),
        true
    ),
    legacy_simple_entry!(
        "set_plane_properties",
        PrimitiveId::SET_PLANE_PROPERTIES,
        RequiresBinding,
        LegacySimpleWorkFormula::Constant(1),
        Some("plane_property"),
        true
    ),
    legacy_simple_entry!(
        "convert_plane",
        PrimitiveId::CONVERT_PLANE,
        RequiresBinding,
        LegacySimpleWorkFormula::Constant(MAX_FILL_PIXELS),
        Some("plane_conversion"),
        true
    ),
    legacy_simple_entry!(
        "convert_layer",
        PrimitiveId::CONVERT_LAYER,
        RequiresBinding,
        LegacySimpleWorkFormula::Constant(MAX_FILL_PIXELS),
        None,
        true
    ),
    legacy_simple_entry!(
        "mirror_document",
        PrimitiveId::MIRROR_DOCUMENT,
        Portable,
        LegacySimpleWorkFormula::Constant(MAX_FILL_PIXELS),
        Some("mirror"),
        false
    ),
    legacy_simple_entry!(
        "rotate_document",
        PrimitiveId::ROTATE_DOCUMENT,
        Portable,
        LegacySimpleWorkFormula::Constant(MAX_FILL_PIXELS),
        Some("rotate_90"),
        false
    ),
    legacy_simple_entry!(
        "resize_document",
        PrimitiveId::RESIZE_DOCUMENT,
        Portable,
        LegacySimpleWorkFormula::ResizePixels,
        Some("resize"),
        false
    ),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LegacySimpleEntityKind {
    Layer,
    Plane,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LegacySimpleResolvedBinding {
    entity: LegacySimpleEntityKind,
    persistent_id: u64,
}

type LegacySimpleLiftedArguments = (&'static str, Option<(LegacySimpleEntityKind, u64)>, String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LegacySimpleWorkEstimate {
    max_invocations: u64,
    max_output_ids: u64,
    max_asset_bytes: u64,
    max_work_units: u64,
    max_output_growth: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LegacySimpleAdapterError {
    InvalidSource,
    Type(InkScriptTypeDiagnosticCode),
    UnsupportedPrimitive,
    UnknownCommand,
    InvalidTypedStep,
    MissingBinding,
    TargetMismatch,
    InvalidValue,
    ResourceLimit,
}

#[derive(Clone, Debug)]
pub(crate) struct LegacySimpleScriptStep {
    typed: InkScriptTypedStep,
    arguments: InkScriptTypedValue,
    bindings: BTreeMap<String, LegacySimpleResolvedBinding>,
}

impl LegacySimpleScriptStep {
    pub(crate) fn from_canonical(
        invocation: &CanonicalInvocation,
    ) -> Result<Self, LegacySimpleAdapterError> {
        let (command, binding, arguments) = lift_arguments(invocation)?;
        let mut source = String::from(
            "inkscript_fragment 1;\nrequires { procedure_catalog = 1; replay_epoch = 23; }\n",
        );
        let mut bindings = BTreeMap::new();
        if let Some((entity, persistent_id)) = binding {
            if persistent_id == 0 {
                return Err(LegacySimpleAdapterError::InvalidValue);
            }
            let entity_name = match entity {
                LegacySimpleEntityKind::Layer => "layer",
                LegacySimpleEntityKind::Plane => "plane",
            };
            source.push_str(&format!(
                "bindings {{ let target = select {entity_name} {{ source_document_uuid = uuid\"{ADAPTER_SOURCE_UUID}\"; persistent_id = {persistent_id}; }}; }}\n"
            ));
            bindings.insert(
                "target".to_owned(),
                LegacySimpleResolvedBinding {
                    entity,
                    persistent_id,
                },
            );
        }
        source.push_str(&format!(
            "program {{ step \"Canonical LegacySimple adapter\" {{ enabled = true; invoke {command} {{ {arguments} }}; }} }}\n"
        ));
        Self::from_source(&source, bindings)
    }

    fn from_source(
        source: &str,
        bindings: BTreeMap<String, LegacySimpleResolvedBinding>,
    ) -> Result<Self, LegacySimpleAdapterError> {
        let source = InkScriptSource::new(InkScriptSourceId::new(7), source.as_bytes())
            .map_err(|_| LegacySimpleAdapterError::InvalidSource)?;
        let parsed = parse_inkscript(&source);
        if !parsed.is_valid() {
            return Err(LegacySimpleAdapterError::InvalidSource);
        }
        let schema = legacy_simple_schema()?;
        let model = build_inkscript_declaration_model(&parsed, &schema)
            .map_err(|error| LegacySimpleAdapterError::Type(error.code()))?;
        if model.steps().len() != 1 || !model.steps()[0].enabled() {
            return Err(LegacySimpleAdapterError::InvalidTypedStep);
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
        bindings: &BTreeMap<String, crate::script::bind::InkScriptBoundValue>,
    ) -> Result<Self, LegacySimpleAdapterError> {
        let bindings = bindings
            .iter()
            .filter_map(|(name, value)| match value {
                crate::script::bind::InkScriptBoundValue::One(reference) => Some((
                    name.clone(),
                    LegacySimpleResolvedBinding {
                        entity: match reference.entity.as_str() {
                            "layer" => LegacySimpleEntityKind::Layer,
                            "plane" => LegacySimpleEntityKind::Plane,
                            _ => return None,
                        },
                        persistent_id: reference.persistent_id,
                    },
                )),
                _ => None,
            })
            .collect();
        Ok(Self {
            typed: typed.clone(),
            arguments,
            bindings,
        })
    }

    pub(crate) fn to_canonical(&self) -> Result<CanonicalInvocation, LegacySimpleAdapterError> {
        let arguments = record(&self.arguments)?;
        match self.typed.command() {
            "set_layer_properties" => Ok(CanonicalInvocation::SetLayerProperties {
                layer_id: binding_id(
                    field(arguments, "layer_id")?,
                    &self.bindings,
                    LegacySimpleEntityKind::Layer,
                )?,
                visible: boolean(field(arguments, "visible")?)?,
                editable: boolean(field(arguments, "editable")?)?,
                opacity_milli: opacity(field(arguments, "opacity_milli")?)?,
                name: node_name(field(arguments, "name")?)?,
            }),
            "set_plane_properties" => Ok(CanonicalInvocation::SetPlaneProperties {
                plane_id: binding_id(
                    field(arguments, "plane_id")?,
                    &self.bindings,
                    LegacySimpleEntityKind::Plane,
                )?,
                visible: boolean(field(arguments, "visible")?)?,
                editable: boolean(field(arguments, "editable")?)?,
                opacity_milli: opacity(field(arguments, "opacity_milli")?)?,
                name: node_name(field(arguments, "name")?)?,
            }),
            "convert_plane" => Ok(CanonicalInvocation::ConvertPlane {
                plane_id: binding_id(
                    field(arguments, "plane_id")?,
                    &self.bindings,
                    LegacySimpleEntityKind::Plane,
                )?,
                destination_kind: plane_kind(field(arguments, "destination_kind")?)?,
                destination_format: pixel_format(field(arguments, "destination_format")?)?,
            }),
            "convert_layer" => Ok(CanonicalInvocation::ConvertLayer {
                layer_id: binding_id(
                    field(arguments, "layer_id")?,
                    &self.bindings,
                    LegacySimpleEntityKind::Layer,
                )?,
                destination: layer_kind(field(arguments, "destination")?)?,
            }),
            "mirror_document" => Ok(CanonicalInvocation::MirrorDocument {
                axis: mirror_axis(field(arguments, "axis")?)?,
            }),
            "rotate_document" => Ok(CanonicalInvocation::RotateDocument {
                direction: rotate_direction(field(arguments, "direction")?)?,
            }),
            "resize_document" => Ok(CanonicalInvocation::ResizeDocument {
                resize: document_resize(field(arguments, "resize")?)?,
            }),
            _ => Err(LegacySimpleAdapterError::UnknownCommand),
        }
    }

    fn metadata(&self) -> Result<&'static LegacySimpleCatalogEntry, LegacySimpleAdapterError> {
        LEGACY_SIMPLE_CATALOG
            .iter()
            .find(|entry| entry.command == self.typed.command())
            .ok_or(LegacySimpleAdapterError::UnknownCommand)
    }

    fn work(&self) -> Result<LegacySimpleWorkEstimate, LegacySimpleAdapterError> {
        let metadata = self.metadata()?;
        let (max_work_units, max_output_growth) = match metadata.work {
            LegacySimpleWorkFormula::Constant(value) => (value, 0),
            LegacySimpleWorkFormula::ResizePixels => {
                let arguments = record(&self.arguments)?;
                let resize = document_resize(field(arguments, "resize")?)?;
                let pixels = u64::from(resize.width)
                    .checked_mul(u64::from(resize.height))
                    .ok_or(LegacySimpleAdapterError::ResourceLimit)?;
                if pixels > MAX_FILL_PIXELS {
                    return Err(LegacySimpleAdapterError::ResourceLimit);
                }
                (pixels, pixels)
            }
        };
        Ok(LegacySimpleWorkEstimate {
            max_invocations: 1,
            max_output_ids: 0,
            max_asset_bytes: 0,
            max_work_units,
            max_output_growth,
        })
    }
}

fn legacy_simple_schema() -> Result<InkScriptSchemaView<'static>, LegacySimpleAdapterError> {
    InkScriptSchemaView::exact_current_with_catalog(
        LEGACY_SIMPLE_ENUMS,
        &[],
        LEGACY_SIMPLE_RECORDS,
        LEGACY_SIMPLE_COMMANDS,
    )
    .map_err(|_| LegacySimpleAdapterError::InvalidTypedStep)
}

fn lift_arguments(
    invocation: &CanonicalInvocation,
) -> Result<LegacySimpleLiftedArguments, LegacySimpleAdapterError> {
    Ok(match invocation {
        CanonicalInvocation::SetLayerProperties {
            layer_id,
            visible,
            editable,
            opacity_milli,
            name,
        } => {
            validate_properties(*opacity_milli, name)?;
            (
                "set_layer_properties",
                Some((LegacySimpleEntityKind::Layer, *layer_id)),
                format!(
                    "layer_id = $target; visible = {visible}; editable = {editable}; opacity_milli = {opacity_milli}; name = {};",
                    string_literal(name)
                ),
            )
        }
        CanonicalInvocation::SetPlaneProperties {
            plane_id,
            visible,
            editable,
            opacity_milli,
            name,
        } => {
            validate_properties(*opacity_milli, name)?;
            (
                "set_plane_properties",
                Some((LegacySimpleEntityKind::Plane, *plane_id)),
                format!(
                    "plane_id = $target; visible = {visible}; editable = {editable}; opacity_milli = {opacity_milli}; name = {};",
                    string_literal(name)
                ),
            )
        }
        CanonicalInvocation::ConvertPlane {
            plane_id,
            destination_kind,
            destination_format,
        } => (
            "convert_plane",
            Some((LegacySimpleEntityKind::Plane, *plane_id)),
            format!(
                "plane_id = $target; destination_kind = {}; destination_format = {};",
                plane_kind_name(*destination_kind),
                pixel_format_name(*destination_format)?
            ),
        ),
        CanonicalInvocation::ConvertLayer {
            layer_id,
            destination,
        } => (
            "convert_layer",
            Some((LegacySimpleEntityKind::Layer, *layer_id)),
            format!(
                "layer_id = $target; destination = {};",
                layer_kind_name(*destination)
            ),
        ),
        CanonicalInvocation::MirrorDocument { axis } => (
            "mirror_document",
            None,
            format!("axis = {};", mirror_axis_name(*axis)),
        ),
        CanonicalInvocation::RotateDocument { direction } => (
            "rotate_document",
            None,
            format!("direction = {};", rotate_direction_name(*direction)),
        ),
        CanonicalInvocation::ResizeDocument { resize } => {
            validate_resize(*resize)?;
            (
                "resize_document",
                None,
                format!(
                    "resize = {{ width = {}; height = {}; dpi_x_milli = {}; dpi_y_milli = {}; anchor = {}; resample = {}; }};",
                    resize.width,
                    resize.height,
                    resize.dpi_x_milli,
                    resize.dpi_y_milli,
                    resize_anchor_name(resize.anchor),
                    resize.resample
                ),
            )
        }
        _ => return Err(LegacySimpleAdapterError::UnsupportedPrimitive),
    })
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, LegacySimpleAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(LegacySimpleAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, LegacySimpleAdapterError> {
    fields
        .get(name)
        .ok_or(LegacySimpleAdapterError::InvalidTypedStep)
}

fn binding_id(
    value: &InkScriptTypedValue,
    bindings: &BTreeMap<String, LegacySimpleResolvedBinding>,
    expected: LegacySimpleEntityKind,
) -> Result<u64, LegacySimpleAdapterError> {
    let InkScriptTypedValueKind::Reference { root, segments } = value.kind() else {
        return Err(LegacySimpleAdapterError::InvalidTypedStep);
    };
    if !segments.is_empty() {
        return Err(LegacySimpleAdapterError::InvalidTypedStep);
    }
    let binding = bindings
        .get(root)
        .ok_or(LegacySimpleAdapterError::MissingBinding)?;
    if binding.entity != expected {
        return Err(LegacySimpleAdapterError::TargetMismatch);
    }
    if binding.persistent_id == 0 {
        return Err(LegacySimpleAdapterError::InvalidValue);
    }
    Ok(binding.persistent_id)
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, LegacySimpleAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(LegacySimpleAdapterError::InvalidTypedStep),
    }
}

fn u32_value(value: &InkScriptTypedValue) -> Result<u32, LegacySimpleAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(LegacySimpleAdapterError::InvalidTypedStep),
    }
}

fn enum_name<'a>(
    value: &'a InkScriptTypedValue,
    expected_type: &str,
) -> Result<&'a str, LegacySimpleAdapterError> {
    if value.type_name() != expected_type {
        return Err(LegacySimpleAdapterError::InvalidTypedStep);
    }
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(LegacySimpleAdapterError::InvalidTypedStep),
    }
}

fn opacity(value: &InkScriptTypedValue) -> Result<u32, LegacySimpleAdapterError> {
    let value = u32_value(value)?;
    (value <= 1_000)
        .then_some(value)
        .ok_or(LegacySimpleAdapterError::InvalidValue)
}

fn node_name(value: &InkScriptTypedValue) -> Result<String, LegacySimpleAdapterError> {
    let InkScriptTypedValueKind::String(value) = value.kind() else {
        return Err(LegacySimpleAdapterError::InvalidTypedStep);
    };
    validate_properties(0, value)?;
    Ok(value.clone())
}

fn mirror_axis(value: &InkScriptTypedValue) -> Result<MirrorAxis, LegacySimpleAdapterError> {
    match enum_name(value, "mirror_axis")? {
        "horizontal" => Ok(MirrorAxis::Horizontal),
        "vertical" => Ok(MirrorAxis::Vertical),
        _ => Err(LegacySimpleAdapterError::InvalidValue),
    }
}

fn rotate_direction(
    value: &InkScriptTypedValue,
) -> Result<RotateDirection, LegacySimpleAdapterError> {
    match enum_name(value, "rotate_direction")? {
        "left_90" => Ok(RotateDirection::Left90),
        "right_90" => Ok(RotateDirection::Right90),
        _ => Err(LegacySimpleAdapterError::InvalidValue),
    }
}

fn resize_anchor(value: &InkScriptTypedValue) -> Result<ResizeAnchor, LegacySimpleAdapterError> {
    match enum_name(value, "resize_anchor")? {
        "top_left" => Ok(ResizeAnchor::TopLeft),
        "top_right" => Ok(ResizeAnchor::TopRight),
        "center" => Ok(ResizeAnchor::Center),
        "bottom_left" => Ok(ResizeAnchor::BottomLeft),
        "bottom_right" => Ok(ResizeAnchor::BottomRight),
        _ => Err(LegacySimpleAdapterError::InvalidValue),
    }
}

fn plane_kind(value: &InkScriptTypedValue) -> Result<PlaneType, LegacySimpleAdapterError> {
    match enum_name(value, "plane_kind")? {
        "main_line" => Ok(PlaneType::MainLine),
        "color" => Ok(PlaneType::Color),
        "raster" => Ok(PlaneType::Raster),
        "selection" => Ok(PlaneType::Selection),
        "vector_main_line" => Ok(PlaneType::VectorMainLine),
        "color_trace" => Ok(PlaneType::ColorTrace),
        "vector_fill" => Ok(PlaneType::VectorFill),
        _ => Err(LegacySimpleAdapterError::InvalidValue),
    }
}

fn pixel_format(value: &InkScriptTypedValue) -> Result<PixelFormat, LegacySimpleAdapterError> {
    match enum_name(value, "pixel_format")? {
        "mask8" => Ok(PixelFormat::BinaryMask8),
        "gray8" => Ok(PixelFormat::Grayscale8),
        "gray16" => Ok(PixelFormat::Grayscale16),
        "rgba8" => Ok(PixelFormat::StraightRgba8),
        "rgba16" => Ok(PixelFormat::StraightRgba16),
        _ => Err(LegacySimpleAdapterError::InvalidValue),
    }
}

fn layer_kind(value: &InkScriptTypedValue) -> Result<LayerKind, LegacySimpleAdapterError> {
    match enum_name(value, "layer_kind")? {
        "binary_coloring" => Ok(LayerKind::BinaryColoring),
        "grayscale_coloring" => Ok(LayerKind::GrayscaleColoring),
        "raster" => Ok(LayerKind::Raster),
        "selection" => Ok(LayerKind::Selection),
        "frame" => Ok(LayerKind::Frame),
        "vanishing_point" => Ok(LayerKind::VanishingPoint),
        "adjustment" => Ok(LayerKind::Adjustment),
        "text" => Ok(LayerKind::Text),
        "annotation" => Ok(LayerKind::Annotation),
        "vector_coloring" => Ok(LayerKind::VectorColoring),
        _ => Err(LegacySimpleAdapterError::InvalidValue),
    }
}

fn document_resize(
    value: &InkScriptTypedValue,
) -> Result<DocumentResize, LegacySimpleAdapterError> {
    if value.type_name() != "document_resize" {
        return Err(LegacySimpleAdapterError::InvalidTypedStep);
    }
    let fields = record(value)?;
    let resize = DocumentResize {
        width: u32_value(field(fields, "width")?)?,
        height: u32_value(field(fields, "height")?)?,
        dpi_x_milli: u32_value(field(fields, "dpi_x_milli")?)?,
        dpi_y_milli: u32_value(field(fields, "dpi_y_milli")?)?,
        anchor: resize_anchor(field(fields, "anchor")?)?,
        resample: boolean(field(fields, "resample")?)?,
    };
    validate_resize(resize)?;
    Ok(resize)
}

fn validate_properties(opacity_milli: u32, name: &str) -> Result<(), LegacySimpleAdapterError> {
    if opacity_milli > 1_000
        || name.is_empty()
        || name.len() > MAX_NODE_NAME_BYTES
        || name.chars().any(char::is_control)
    {
        Err(LegacySimpleAdapterError::InvalidValue)
    } else {
        Ok(())
    }
}

fn validate_resize(resize: DocumentResize) -> Result<(), LegacySimpleAdapterError> {
    if resize.width == 0 || resize.height == 0 || resize.dpi_x_milli == 0 || resize.dpi_y_milli == 0
    {
        return Err(LegacySimpleAdapterError::InvalidValue);
    }
    let pixels = u64::from(resize.width)
        .checked_mul(u64::from(resize.height))
        .ok_or(LegacySimpleAdapterError::ResourceLimit)?;
    if pixels > MAX_FILL_PIXELS {
        return Err(LegacySimpleAdapterError::ResourceLimit);
    }
    Ok(())
}

fn string_literal(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            _ => result.push(character),
        }
    }
    result.push('"');
    result
}

const fn mirror_axis_name(value: MirrorAxis) -> &'static str {
    match value {
        MirrorAxis::Horizontal => "horizontal",
        MirrorAxis::Vertical => "vertical",
    }
}

const fn rotate_direction_name(value: RotateDirection) -> &'static str {
    match value {
        RotateDirection::Left90 => "left_90",
        RotateDirection::Right90 => "right_90",
    }
}

const fn resize_anchor_name(value: ResizeAnchor) -> &'static str {
    match value {
        ResizeAnchor::TopLeft => "top_left",
        ResizeAnchor::TopRight => "top_right",
        ResizeAnchor::Center => "center",
        ResizeAnchor::BottomLeft => "bottom_left",
        ResizeAnchor::BottomRight => "bottom_right",
    }
}

const fn plane_kind_name(value: PlaneType) -> &'static str {
    match value {
        PlaneType::MainLine => "main_line",
        PlaneType::Color => "color",
        PlaneType::Raster => "raster",
        PlaneType::Selection => "selection",
        PlaneType::VectorMainLine => "vector_main_line",
        PlaneType::ColorTrace => "color_trace",
        PlaneType::VectorFill => "vector_fill",
    }
}

fn pixel_format_name(value: PixelFormat) -> Result<&'static str, LegacySimpleAdapterError> {
    match value {
        PixelFormat::BinaryMask8 => Ok("mask8"),
        PixelFormat::Grayscale8 => Ok("gray8"),
        PixelFormat::Grayscale16 => Ok("gray16"),
        PixelFormat::StraightRgba8 => Ok("rgba8"),
        PixelFormat::StraightRgba16 => Ok("rgba16"),
        PixelFormat::PremultipliedBgra8 => Err(LegacySimpleAdapterError::InvalidValue),
    }
}

const fn layer_kind_name(value: LayerKind) -> &'static str {
    match value {
        LayerKind::BinaryColoring => "binary_coloring",
        LayerKind::GrayscaleColoring => "grayscale_coloring",
        LayerKind::Raster => "raster",
        LayerKind::Selection => "selection",
        LayerKind::Frame => "frame",
        LayerKind::VanishingPoint => "vanishing_point",
        LayerKind::Adjustment => "adjustment",
        LayerKind::Text => "text",
        LayerKind::Annotation => "annotation",
        LayerKind::VectorColoring => "vector_coloring",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::canonical_document_state;
    use crate::{Core, CoreError, DEFAULT_DPI_MILLI};

    fn core() -> Core {
        let mut core = Core::new();
        core.new_cell(8, 6, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core
    }

    fn fixture_invocations(core: &Core) -> Vec<CanonicalInvocation> {
        let layers = core.layers().unwrap();
        let layer = &layers[0];
        let color_plane = layer
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Color)
            .unwrap();
        vec![
            CanonicalInvocation::SetLayerProperties {
                layer_id: layer.id,
                visible: false,
                editable: true,
                opacity_milli: 750,
                name: "Layer \"LegacySimple\"".to_owned(),
            },
            CanonicalInvocation::SetPlaneProperties {
                plane_id: color_plane.id,
                visible: false,
                editable: true,
                opacity_milli: 625,
                name: "Color \\ LegacySimple".to_owned(),
            },
            CanonicalInvocation::ConvertPlane {
                plane_id: color_plane.id,
                destination_kind: PlaneType::Color,
                destination_format: PixelFormat::StraightRgba16,
            },
            CanonicalInvocation::ConvertLayer {
                layer_id: layer.id,
                destination: LayerKind::GrayscaleColoring,
            },
            CanonicalInvocation::MirrorDocument {
                axis: MirrorAxis::Horizontal,
            },
            CanonicalInvocation::RotateDocument {
                direction: RotateDirection::Right90,
            },
            CanonicalInvocation::ResizeDocument {
                resize: DocumentResize {
                    width: 10,
                    height: 7,
                    dpi_x_milli: 120_000,
                    dpi_y_milli: 96_000,
                    anchor: ResizeAnchor::BottomRight,
                    resample: false,
                },
            },
        ]
    }

    fn digest(core: &Core) -> crate::DocumentStateDigest {
        canonical_document_state(core.document.as_ref().unwrap())
            .unwrap()
            .1
    }

    #[test]
    fn exact_catalog_metadata_and_codec_cover_all_legacy_simple_primitives() {
        let core = core();
        let invocations = fixture_invocations(&core);
        assert_eq!(LEGACY_SIMPLE_CATALOG.len(), 7);
        for (invocation, metadata) in invocations.iter().zip(LEGACY_SIMPLE_CATALOG) {
            let step = LegacySimpleScriptStep::from_canonical(invocation).unwrap();
            assert_eq!(step.to_canonical().unwrap(), *invocation);
            assert_eq!(step.metadata().unwrap(), metadata);
            assert_eq!(metadata.primitive_id, invocation.primitive_id());
            assert_eq!(metadata.primitive_schema_version, 2);
            assert_eq!(metadata.semantics_revision, 2);
            assert_eq!(metadata.result_count, 0);
            assert_eq!(metadata.asset_count, 0);
            let work = step.work().unwrap();
            assert_eq!(work.max_invocations, 1);
            assert_eq!(work.max_output_ids, 0);
            assert_eq!(work.max_asset_bytes, 0);
            if metadata.command == "resize_document" {
                assert_eq!(work.max_work_units, 70);
                assert_eq!(work.max_output_growth, 70);
            }
        }
        assert_eq!(
            LegacySimpleScriptStep::from_canonical(&CanonicalInvocation::ClearSelection)
                .unwrap_err(),
            LegacySimpleAdapterError::UnsupportedPrimitive
        );
    }

    #[test]
    fn script_lowering_and_direct_execution_have_identical_state() {
        for index in 0..7 {
            let mut direct = core();
            let mut scripted = core();
            let invocation = fixture_invocations(&direct).remove(index);
            let step = LegacySimpleScriptStep::from_canonical(&invocation).unwrap();
            let lowered = step.to_canonical().unwrap();
            direct.execute_canonical_invocation(invocation).unwrap();
            scripted.execute_canonical_invocation(lowered).unwrap();
            assert_eq!(digest(&direct), digest(&scripted), "fixture {index}");
            assert_eq!(direct.current_state, scripted.current_state);
            assert_eq!(direct.document_revision, scripted.document_revision);
            assert_eq!(direct.history_entries(), scripted.history_entries());
            assert_eq!(direct.next_id, scripted.next_id);
            assert_eq!(direct.savepoint, scripted.savepoint);
            assert_eq!(
                direct.document_info().unwrap(),
                scripted.document_info().unwrap()
            );
        }
    }

    #[test]
    fn no_op_invalid_resource_and_stale_failures_are_atomic() {
        let mut unchanged = core();
        let layer = unchanged.layers().unwrap()[0].clone();
        let no_op = CanonicalInvocation::SetLayerProperties {
            layer_id: layer.id,
            visible: layer.visible,
            editable: layer.editable,
            opacity_milli: layer.opacity_milli,
            name: layer.name,
        };
        let before_digest = digest(&unchanged);
        let before_revision = unchanged.document_revision;
        let before_history = unchanged.history_entries();
        let lowered = LegacySimpleScriptStep::from_canonical(&no_op)
            .unwrap()
            .to_canonical()
            .unwrap();
        unchanged.execute_canonical_invocation(lowered).unwrap();
        assert_eq!(digest(&unchanged), before_digest);
        assert_eq!(unchanged.document_revision, before_revision);
        assert_eq!(unchanged.history_entries(), before_history);

        let invalid = CanonicalInvocation::SetLayerProperties {
            layer_id: 1,
            visible: true,
            editable: true,
            opacity_milli: 1_001,
            name: "Invalid".to_owned(),
        };
        assert_eq!(
            LegacySimpleScriptStep::from_canonical(&invalid).unwrap_err(),
            LegacySimpleAdapterError::InvalidValue
        );
        let oversized = CanonicalInvocation::ResizeDocument {
            resize: DocumentResize {
                width: u32::MAX,
                height: u32::MAX,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
                anchor: ResizeAnchor::Center,
                resample: true,
            },
        };
        assert_eq!(
            LegacySimpleScriptStep::from_canonical(&oversized).unwrap_err(),
            LegacySimpleAdapterError::ResourceLimit
        );

        let valid = fixture_invocations(&unchanged).remove(0);
        let mut missing = LegacySimpleScriptStep::from_canonical(&valid).unwrap();
        missing.bindings.clear();
        assert_eq!(
            missing.to_canonical(),
            Err(LegacySimpleAdapterError::MissingBinding)
        );
        let mut wrong_kind = LegacySimpleScriptStep::from_canonical(&valid).unwrap();
        wrong_kind.bindings.get_mut("target").unwrap().entity = LegacySimpleEntityKind::Plane;
        assert_eq!(
            wrong_kind.to_canonical(),
            Err(LegacySimpleAdapterError::TargetMismatch)
        );

        let mut stale_core = core();
        let mut stale = LegacySimpleScriptStep::from_canonical(&valid).unwrap();
        stale.bindings.get_mut("target").unwrap().persistent_id = u64::MAX;
        let stale_invocation = stale.to_canonical().unwrap();
        let stale_digest = digest(&stale_core);
        let stale_revision = stale_core.document_revision;
        assert!(matches!(
            stale_core.execute_canonical_invocation(stale_invocation),
            Err(CoreError::InvalidArgument(_))
        ));
        assert_eq!(digest(&stale_core), stale_digest);
        assert_eq!(stale_core.document_revision, stale_revision);
    }

    #[test]
    fn unknown_field_type_enum_and_format_mismatch_are_rejected() {
        let prefix = "inkscript_fragment 1; requires { procedure_catalog = 1; replay_epoch = 23; }";
        let unknown_field = format!(
            "{prefix} program {{ step \"Bad\" {{ enabled = true; invoke mirror_document {{ axis = horizontal; extra = true; }}; }} }}"
        );
        assert_eq!(
            LegacySimpleScriptStep::from_source(&unknown_field, BTreeMap::new()).unwrap_err(),
            LegacySimpleAdapterError::Type(InkScriptTypeDiagnosticCode::InvalidSemanticModel)
        );
        let unknown_enum = format!(
            "{prefix} program {{ step \"Bad\" {{ enabled = true; invoke mirror_document {{ axis = diagonal; }}; }} }}"
        );
        assert_eq!(
            LegacySimpleScriptStep::from_source(&unknown_enum, BTreeMap::new()).unwrap_err(),
            LegacySimpleAdapterError::Type(InkScriptTypeDiagnosticCode::ValueOutOfRange)
        );
        const UNKNOWN_FIELDS: &[InkScriptFieldSchema] =
            &[InkScriptFieldSchema::required("value", "missing_type", 0)];
        const UNKNOWN_COMMANDS: &[InkScriptCommandSchema] =
            &[InkScriptCommandSchema::new("unknown_type", UNKNOWN_FIELDS)];
        assert!(
            InkScriptSchemaView::exact_current_with_catalog(
                LEGACY_SIMPLE_ENUMS,
                &[],
                LEGACY_SIMPLE_RECORDS,
                UNKNOWN_COMMANDS,
            )
            .is_err()
        );

        let base = core();
        let plane_id = base.layers().unwrap()[0]
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Color)
            .unwrap()
            .id;
        let mismatch = CanonicalInvocation::ConvertPlane {
            plane_id,
            destination_kind: PlaneType::MainLine,
            destination_format: PixelFormat::StraightRgba8,
        };
        let step = LegacySimpleScriptStep::from_canonical(&mismatch).unwrap();
        let mut target = core();
        let before = digest(&target);
        let revision = target.document_revision;
        assert!(matches!(
            target.execute_canonical_invocation(step.to_canonical().unwrap()),
            Err(CoreError::InvalidArgument(_))
        ));
        assert_eq!(digest(&target), before);
        assert_eq!(target.document_revision, revision);
    }

    #[test]
    fn private_adapter_values_are_send_sync_and_do_not_publish_a_second_executor() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LegacySimpleScriptStep>();
        assert_send_sync::<LegacySimpleCatalogEntry>();
        assert!(LEGACY_SIMPLE_CATALOG.iter().all(|entry| {
            entry.result_count == 0
                && entry.asset_count == 0
                && matches!(
                    entry.portability,
                    LegacySimplePortability::Portable | LegacySimplePortability::RequiresBinding
                )
                && entry
                    .legacy_projection
                    .is_none_or(|projection| !projection.is_empty())
        }));
        assert!(
            LEGACY_SIMPLE_CATALOG
                .iter()
                .filter(|entry| entry.portability == LegacySimplePortability::RequiresBinding)
                .all(|entry| entry.allow_skip_dependents)
        );
    }
}
