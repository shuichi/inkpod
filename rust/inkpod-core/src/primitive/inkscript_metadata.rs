//! Private pre-ratification InkScript adapter for color metadata and guides.

use super::inkscript_reference::{
    InkScriptEntityKind, InkScriptReferenceError, InkScriptRuntimeReferences,
};
use super::{CanonicalInvocation, InvocationResult};
use crate::{
    ColorChartEntry, Core, CoreError, GridConfig, GuideAxis, MAX_APPLICATION_COLORS,
    MAX_COLOR_CHART_NAME_BYTES, PixelValue, PrimitiveId, PrimitiveRequest,
};
use inkpod_format::{
    InkScriptCommandResultSchema, InkScriptCommandSchema, InkScriptConstructorArgumentSchema,
    InkScriptConstructorSchema, InkScriptFieldSchema, InkScriptRecordSchema,
    InkScriptResultAvailability, InkScriptSchemaView, InkScriptSource, InkScriptSourceId,
    InkScriptTypeDiagnosticCode, InkScriptTypedStep, InkScriptTypedValue, InkScriptTypedValueKind,
    build_inkscript_declaration_model, parse_inkscript,
};
use std::collections::BTreeMap;

const ADAPTER_SOURCE_UUID: &str = "00000000-0000-0000-0000-000000000016";

const CHART_NAME_TEXT_ARGUMENTS: &[InkScriptConstructorArgumentSchema] =
    &[InkScriptConstructorArgumentSchema::new(
        "value",
        "string",
        &[],
    )];
const CHART_NAME_SCALAR_ARGUMENTS: &[InkScriptConstructorArgumentSchema] =
    &[InkScriptConstructorArgumentSchema::new(
        "values",
        "list<u32>",
        &[],
    )];

pub(crate) const METADATA_COLOR_GUIDE_CONSTRUCTORS: &[InkScriptConstructorSchema] = &[
    InkScriptConstructorSchema::new(
        "chart_name_text",
        "color_chart_name",
        CHART_NAME_TEXT_ARGUMENTS,
    ),
    InkScriptConstructorSchema::new(
        "chart_name_scalars",
        "color_chart_name",
        CHART_NAME_SCALAR_ARGUMENTS,
    ),
];

const COLOR_CHART_ENTRY_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("color", "pixel_value", 0),
    InkScriptFieldSchema::required("name", "color_chart_name", 1),
];
const GRID_CONFIG_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("origin_x", "i32", 0),
    InkScriptFieldSchema::required("origin_y", "i32", 1),
    InkScriptFieldSchema::required("spacing_x", "u32", 2),
    InkScriptFieldSchema::required("spacing_y", "u32", 3),
    InkScriptFieldSchema::required("subdivisions", "u32", 4),
];

pub(crate) const METADATA_COLOR_GUIDE_RECORDS: &[InkScriptRecordSchema] = &[
    InkScriptRecordSchema::new("color_chart_name", &[]),
    InkScriptRecordSchema::new("color_chart_entry", COLOR_CHART_ENTRY_FIELDS),
    InkScriptRecordSchema::new("grid_config", GRID_CONFIG_FIELDS),
];

const COLOR_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("color", "pixel_value", 0)];
const PALETTE_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "colors",
    "list<pixel_value>",
    0,
)];
const COLOR_CHART_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("entries", "list<color_chart_entry>", 0),
    InkScriptFieldSchema::required("locked", "bool", 1),
];
const ADD_GUIDE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("axis", "guide_axis", 0),
    InkScriptFieldSchema::required("position", "i32", 1),
];
const GUIDE_ID_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("guide_id", "guide_ref", 0)];
const MOVE_GUIDE_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("guide_id", "guide_ref", 0),
    InkScriptFieldSchema::required("position", "i32", 1),
];
const SET_GRID_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("grid", "grid_config", 0)];
const GUIDE_RESULT: &[InkScriptCommandResultSchema] = &[InkScriptCommandResultSchema::scalar(
    "guide",
    "guide_ref",
    InkScriptResultAvailability::AlwaysOnSuccess,
    0,
)];

pub(crate) const METADATA_COLOR_GUIDE_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::new("set_main_line_color", COLOR_FIELDS),
    InkScriptCommandSchema::new("replace_palette", PALETTE_FIELDS),
    InkScriptCommandSchema::new("replace_color_chart", COLOR_CHART_FIELDS),
    InkScriptCommandSchema::with_results("add_guide", ADD_GUIDE_FIELDS, GUIDE_RESULT),
    InkScriptCommandSchema::new("move_guide", MOVE_GUIDE_FIELDS),
    InkScriptCommandSchema::new("delete_guide", GUIDE_ID_FIELDS),
    InkScriptCommandSchema::new("set_grid", SET_GRID_FIELDS),
    InkScriptCommandSchema::new("delete_all_guides", &[]),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MetadataColorGuideCatalogEntry {
    pub(crate) command: &'static str,
    pub(crate) primitive_id: PrimitiveId,
    pub(crate) primitive_schema_version: u16,
    pub(crate) semantics_revision: u16,
    pub(crate) equivalence_test: &'static str,
}

pub(crate) const METADATA_COLOR_GUIDE_CATALOG: &[MetadataColorGuideCatalogEntry] = &[
    entry(
        "set_main_line_color",
        PrimitiveId::SET_MAIN_LINE_COLOR,
        1,
        3,
        "INKS-EQ-0021",
    ),
    entry(
        "replace_palette",
        PrimitiveId::REPLACE_PALETTE,
        1,
        3,
        "INKS-EQ-0022",
    ),
    entry(
        "replace_color_chart",
        PrimitiveId::REPLACE_COLOR_CHART,
        1,
        1,
        "INKS-EQ-0023",
    ),
    entry("add_guide", PrimitiveId::ADD_GUIDE, 2, 2, "INKS-EQ-0024"),
    entry("move_guide", PrimitiveId::MOVE_GUIDE, 2, 2, "INKS-EQ-0025"),
    entry(
        "delete_guide",
        PrimitiveId::DELETE_GUIDE,
        2,
        2,
        "INKS-EQ-0026",
    ),
    entry("set_grid", PrimitiveId::SET_GRID, 2, 2, "INKS-EQ-0027"),
    entry(
        "delete_all_guides",
        PrimitiveId::DELETE_ALL_GUIDES,
        2,
        2,
        "INKS-EQ-0028",
    ),
];

const fn entry(
    command: &'static str,
    primitive_id: PrimitiveId,
    primitive_schema_version: u16,
    semantics_revision: u16,
    equivalence_test: &'static str,
) -> MetadataColorGuideCatalogEntry {
    MetadataColorGuideCatalogEntry {
        command,
        primitive_id,
        primitive_schema_version,
        semantics_revision,
        equivalence_test,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MetadataColorGuideAdapterError {
    InvalidSource,
    Type(InkScriptTypeDiagnosticCode),
    UnsupportedPrimitive,
    UnknownCommand,
    InvalidTypedStep,
    MissingReference,
    TargetMismatch,
    InvalidValue,
    ResourceLimit,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MetadataColorGuideInvocation {
    SetMainLineColor(PixelValue),
    ReplacePalette(Vec<PixelValue>),
    ReplaceColorChart {
        entries: Vec<ColorChartEntry>,
        locked: bool,
    },
    Document(CanonicalInvocation),
}

impl MetadataColorGuideInvocation {
    pub(crate) fn primitive_id(&self) -> Result<PrimitiveId, MetadataColorGuideAdapterError> {
        Ok(match self {
            Self::SetMainLineColor(_) => PrimitiveId::SET_MAIN_LINE_COLOR,
            Self::ReplacePalette(_) => PrimitiveId::REPLACE_PALETTE,
            Self::ReplaceColorChart { .. } => PrimitiveId::REPLACE_COLOR_CHART,
            Self::Document(invocation) if is_guide_invocation(invocation) => {
                invocation.primitive_id()
            }
            Self::Document(_) => return Err(MetadataColorGuideAdapterError::UnsupportedPrimitive),
        })
    }

    pub(crate) fn execute(self, core: &mut Core) -> Result<InvocationResult, CoreError> {
        let expected_revision = core.document_info()?.document_revision;
        match self {
            Self::SetMainLineColor(color) => core
                .execute_primitive(PrimitiveRequest::SetMainLineColor {
                    expected_revision,
                    color,
                })
                .map(|outcome| InvocationResult::dispatch(outcome.dispatch())),
            Self::ReplacePalette(colors) => core
                .execute_primitive(PrimitiveRequest::ReplacePalette {
                    expected_revision,
                    colors,
                })
                .map(|outcome| InvocationResult::dispatch(outcome.dispatch())),
            Self::ReplaceColorChart { entries, locked } => core
                .execute_primitive(PrimitiveRequest::ReplaceColorChart {
                    expected_revision,
                    entries,
                    locked,
                })
                .map(|outcome| InvocationResult::dispatch(outcome.dispatch())),
            Self::Document(invocation) => core.execute_canonical_invocation(invocation),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MetadataColorGuideScriptStep {
    typed: InkScriptTypedStep,
    arguments: InkScriptTypedValue,
    references: InkScriptRuntimeReferences,
}

impl MetadataColorGuideScriptStep {
    pub(crate) fn from_canonical(
        invocation: &MetadataColorGuideInvocation,
    ) -> Result<Self, MetadataColorGuideAdapterError> {
        let mut source = String::from(
            "inkscript_fragment 2;\nrequires { procedure_catalog = 4; replay_epoch = 25; }\n",
        );
        let mut references = InkScriptRuntimeReferences::default();
        let (command, arguments, has_result) =
            lift_arguments(invocation, &mut source, &mut references)?;
        let result = if has_result { " as adapter_result" } else { "" };
        source.push_str(&format!(
            "program {{ step \"Canonical metadata adapter\"{result} {{ enabled = true; invoke {command} {{ {arguments} }}; }} }}\n"
        ));
        Self::from_source(&source, references)
    }

    fn from_source(
        source: &str,
        references: InkScriptRuntimeReferences,
    ) -> Result<Self, MetadataColorGuideAdapterError> {
        let source = InkScriptSource::new(InkScriptSourceId::new(16), source.as_bytes())
            .map_err(|_| MetadataColorGuideAdapterError::InvalidSource)?;
        let parsed = parse_inkscript(&source);
        if !parsed.is_valid() {
            return Err(MetadataColorGuideAdapterError::InvalidSource);
        }
        let schema = InkScriptSchemaView::exact_current_with_catalog(
            &[],
            METADATA_COLOR_GUIDE_CONSTRUCTORS,
            METADATA_COLOR_GUIDE_RECORDS,
            METADATA_COLOR_GUIDE_COMMANDS,
        )
        .map_err(|_| MetadataColorGuideAdapterError::InvalidTypedStep)?;
        let model = build_inkscript_declaration_model(&parsed, &schema)
            .map_err(|error| MetadataColorGuideAdapterError::Type(error.code()))?;
        if model.steps().len() != 1 || !model.steps()[0].enabled() {
            return Err(MetadataColorGuideAdapterError::InvalidTypedStep);
        }
        Ok(Self {
            arguments: model.steps()[0].arguments().clone(),
            typed: model.steps()[0].clone(),
            references,
        })
    }

    pub(crate) fn from_compiled(
        typed: &InkScriptTypedStep,
        arguments: InkScriptTypedValue,
        references: &InkScriptRuntimeReferences,
    ) -> Self {
        Self {
            typed: typed.clone(),
            arguments,
            references: references.clone(),
        }
    }

    pub(crate) fn to_canonical(
        &self,
    ) -> Result<MetadataColorGuideInvocation, MetadataColorGuideAdapterError> {
        let arguments = record(&self.arguments)?;
        match self.typed.command() {
            "set_main_line_color" => {
                let color = rgba_pixel(field(arguments, "color")?)?;
                Ok(MetadataColorGuideInvocation::SetMainLineColor(color))
            }
            "replace_palette" => {
                let colors = rgba_list(field(arguments, "colors")?, MAX_APPLICATION_COLORS)?;
                Ok(MetadataColorGuideInvocation::ReplacePalette(colors))
            }
            "replace_color_chart" => {
                let values = list(field(arguments, "entries")?)?;
                if values.len() > MAX_APPLICATION_COLORS {
                    return Err(MetadataColorGuideAdapterError::ResourceLimit);
                }
                let entries = values
                    .iter()
                    .map(color_chart_entry)
                    .collect::<Result<Vec<_>, _>>()?;
                validate_chart_entries(&entries)?;
                Ok(MetadataColorGuideInvocation::ReplaceColorChart {
                    entries,
                    locked: boolean(field(arguments, "locked")?)?,
                })
            }
            "add_guide" => Ok(MetadataColorGuideInvocation::Document(
                CanonicalInvocation::AddGuide {
                    axis: guide_axis(field(arguments, "axis")?)?,
                    position: i32_value(field(arguments, "position")?)?,
                },
            )),
            "move_guide" => Ok(MetadataColorGuideInvocation::Document(
                CanonicalInvocation::MoveGuide {
                    guide_id: guide_id(field(arguments, "guide_id")?, &self.references)?,
                    position: i32_value(field(arguments, "position")?)?,
                },
            )),
            "delete_guide" => Ok(MetadataColorGuideInvocation::Document(
                CanonicalInvocation::DeleteGuide {
                    guide_id: guide_id(field(arguments, "guide_id")?, &self.references)?,
                },
            )),
            "set_grid" => Ok(MetadataColorGuideInvocation::Document(
                CanonicalInvocation::SetGrid {
                    grid: grid_config(field(arguments, "grid")?)?,
                },
            )),
            "delete_all_guides" => Ok(MetadataColorGuideInvocation::Document(
                CanonicalInvocation::DeleteAllGuides,
            )),
            _ => Err(MetadataColorGuideAdapterError::UnknownCommand),
        }
    }

    pub(crate) fn output_entity_kinds(
        invocation: &MetadataColorGuideInvocation,
    ) -> Vec<InkScriptEntityKind> {
        if matches!(
            invocation,
            MetadataColorGuideInvocation::Document(CanonicalInvocation::AddGuide { .. })
        ) {
            vec![InkScriptEntityKind::Guide]
        } else {
            Vec::new()
        }
    }
}

pub(crate) type LiftedArguments = (&'static str, String, bool);

pub(crate) fn lift_arguments(
    invocation: &MetadataColorGuideInvocation,
    source: &mut String,
    references: &mut InkScriptRuntimeReferences,
) -> Result<LiftedArguments, MetadataColorGuideAdapterError> {
    Ok(match invocation {
        MetadataColorGuideInvocation::SetMainLineColor(color) => {
            validate_rgba(*color)?;
            (
                "set_main_line_color",
                format!("color = {};", pixel_literal(*color)?),
                false,
            )
        }
        MetadataColorGuideInvocation::ReplacePalette(colors) => {
            validate_rgba_slice(colors, MAX_APPLICATION_COLORS)?;
            (
                "replace_palette",
                format!(
                    "colors = {};",
                    list_literal(
                        colors
                            .iter()
                            .copied()
                            .map(pixel_literal)
                            .collect::<Result<Vec<_>, _>>()?
                    )
                ),
                false,
            )
        }
        MetadataColorGuideInvocation::ReplaceColorChart { entries, locked } => {
            validate_chart_entries(entries)?;
            (
                "replace_color_chart",
                format!(
                    "entries = {}; locked = {};",
                    list_literal(entries.iter().map(color_chart_entry_literal).collect()),
                    boolean_literal(*locked)
                ),
                false,
            )
        }
        MetadataColorGuideInvocation::Document(CanonicalInvocation::AddGuide {
            axis,
            position,
        }) => (
            "add_guide",
            format!("axis = {}; position = {position};", guide_axis_name(*axis)),
            true,
        ),
        MetadataColorGuideInvocation::Document(CanonicalInvocation::MoveGuide {
            guide_id,
            position,
        }) => {
            bind_guide(source, references, *guide_id)?;
            (
                "move_guide",
                format!("guide_id = $target_guide; position = {position};"),
                false,
            )
        }
        MetadataColorGuideInvocation::Document(CanonicalInvocation::DeleteGuide { guide_id }) => {
            bind_guide(source, references, *guide_id)?;
            (
                "delete_guide",
                "guide_id = $target_guide;".to_owned(),
                false,
            )
        }
        MetadataColorGuideInvocation::Document(CanonicalInvocation::SetGrid { grid }) => (
            "set_grid",
            format!("grid = {};", grid_literal(*grid)),
            false,
        ),
        MetadataColorGuideInvocation::Document(CanonicalInvocation::DeleteAllGuides) => {
            ("delete_all_guides", String::new(), false)
        }
        MetadataColorGuideInvocation::Document(_) => {
            return Err(MetadataColorGuideAdapterError::UnsupportedPrimitive);
        }
    })
}

fn is_guide_invocation(invocation: &CanonicalInvocation) -> bool {
    matches!(
        invocation,
        CanonicalInvocation::AddGuide { .. }
            | CanonicalInvocation::MoveGuide { .. }
            | CanonicalInvocation::DeleteGuide { .. }
            | CanonicalInvocation::SetGrid { .. }
            | CanonicalInvocation::DeleteAllGuides
    )
}

fn bind_guide(
    source: &mut String,
    references: &mut InkScriptRuntimeReferences,
    id: u64,
) -> Result<(), MetadataColorGuideAdapterError> {
    if id == 0 {
        return Err(MetadataColorGuideAdapterError::InvalidValue);
    }
    source.push_str(&format!(
        "bindings {{ let target_guide = select guide {{ source_document_uuid = uuid\"{ADAPTER_SOURCE_UUID}\"; persistent_id = {id}; }}; }}\n"
    ));
    references
        .insert("target_guide", InkScriptEntityKind::Guide, id)
        .map_err(reference_error)
}

fn color_chart_entry_literal(entry: &ColorChartEntry) -> String {
    format!(
        "{{ color = {}; name = {}; }}",
        pixel_literal(entry.color).expect("validated chart color"),
        chart_name_literal(&entry.name)
    )
}

fn chart_name_literal(name: &str) -> String {
    if name.contains('\0') {
        let values = list_literal(
            name.chars()
                .map(|value| u32::from(value).to_string())
                .collect(),
        );
        format!("chart_name_scalars({values})")
    } else {
        format!("chart_name_text({})", string_literal(name))
    }
}

fn grid_literal(grid: GridConfig) -> String {
    format!(
        "{{ origin_x = {}; origin_y = {}; spacing_x = {}; spacing_y = {}; subdivisions = {}; }}",
        grid.origin_x, grid.origin_y, grid.spacing_x, grid.spacing_y, grid.subdivisions
    )
}

fn pixel_literal(value: PixelValue) -> Result<String, MetadataColorGuideAdapterError> {
    match value {
        PixelValue::Rgba(value) => Ok(format!(
            "rgba8({}, {}, {}, {})",
            value[0], value[1], value[2], value[3]
        )),
        PixelValue::Rgba16(value) => Ok(format!(
            "rgba16({}, {}, {}, {})",
            value[0], value[1], value[2], value[3]
        )),
        PixelValue::Binary(_) | PixelValue::Grayscale8(_) | PixelValue::Grayscale16(_) => {
            Err(MetadataColorGuideAdapterError::InvalidValue)
        }
    }
}

fn list_literal(values: Vec<String>) -> String {
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
            value if value <= '\u{1f}' => {
                result.push_str(&format!("\\u{{{:x}}}", u32::from(value)))
            }
            value => result.push(value),
        }
    }
    result.push('"');
    result
}

const fn guide_axis_name(axis: GuideAxis) -> &'static str {
    match axis {
        GuideAxis::Horizontal => "horizontal",
        GuideAxis::Vertical => "vertical",
    }
}

fn record(
    value: &InkScriptTypedValue,
) -> Result<&BTreeMap<String, InkScriptTypedValue>, MetadataColorGuideAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Record(fields) => Ok(fields),
        _ => Err(MetadataColorGuideAdapterError::InvalidTypedStep),
    }
}

fn field<'a>(
    fields: &'a BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'a InkScriptTypedValue, MetadataColorGuideAdapterError> {
    fields
        .get(name)
        .ok_or(MetadataColorGuideAdapterError::InvalidTypedStep)
}

fn list(
    value: &InkScriptTypedValue,
) -> Result<&[InkScriptTypedValue], MetadataColorGuideAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::List(values) => Ok(values),
        _ => Err(MetadataColorGuideAdapterError::InvalidTypedStep),
    }
}

fn constructor<'a>(
    value: &'a InkScriptTypedValue,
    expected: &str,
) -> Result<&'a [InkScriptTypedValue], MetadataColorGuideAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Constructor { name, arguments } if name == expected => {
            Ok(arguments)
        }
        _ => Err(MetadataColorGuideAdapterError::InvalidValue),
    }
}

fn rgba_pixel(value: &InkScriptTypedValue) -> Result<PixelValue, MetadataColorGuideAdapterError> {
    let color = match value.type_name() {
        "rgba8" => {
            let values = constructor(value, "rgba8")?;
            PixelValue::Rgba([
                narrow_u8(&values[0])?,
                narrow_u8(&values[1])?,
                narrow_u8(&values[2])?,
                narrow_u8(&values[3])?,
            ])
        }
        "rgba16" => {
            let values = constructor(value, "rgba16")?;
            PixelValue::Rgba16([
                narrow_u16(&values[0])?,
                narrow_u16(&values[1])?,
                narrow_u16(&values[2])?,
                narrow_u16(&values[3])?,
            ])
        }
        _ => return Err(MetadataColorGuideAdapterError::InvalidValue),
    };
    Ok(color)
}

fn rgba_list(
    value: &InkScriptTypedValue,
    maximum: usize,
) -> Result<Vec<PixelValue>, MetadataColorGuideAdapterError> {
    let values = list(value)?;
    if values.len() > maximum {
        return Err(MetadataColorGuideAdapterError::ResourceLimit);
    }
    values.iter().map(rgba_pixel).collect()
}

fn color_chart_entry(
    value: &InkScriptTypedValue,
) -> Result<ColorChartEntry, MetadataColorGuideAdapterError> {
    let fields = record(value)?;
    Ok(ColorChartEntry {
        color: rgba_pixel(field(fields, "color")?)?,
        name: chart_name(field(fields, "name")?)?,
    })
}

fn chart_name(value: &InkScriptTypedValue) -> Result<String, MetadataColorGuideAdapterError> {
    if value.type_name() != "color_chart_name" {
        return Err(MetadataColorGuideAdapterError::InvalidTypedStep);
    }
    let InkScriptTypedValueKind::Constructor { name, arguments } = value.kind() else {
        return Err(MetadataColorGuideAdapterError::InvalidTypedStep);
    };
    let result = match name.as_str() {
        "chart_name_text" => string_value(&arguments[0])?.to_owned(),
        "chart_name_scalars" => {
            let mut result = String::new();
            for value in list(&arguments[0])? {
                let scalar = char::from_u32(u32_value(value)?)
                    .ok_or(MetadataColorGuideAdapterError::InvalidValue)?;
                result.push(scalar);
            }
            result
        }
        _ => return Err(MetadataColorGuideAdapterError::InvalidValue),
    };
    validate_chart_name(&result)?;
    Ok(result)
}

fn guide_id(
    value: &InkScriptTypedValue,
    references: &InkScriptRuntimeReferences,
) -> Result<u64, MetadataColorGuideAdapterError> {
    references
        .resolve(value, InkScriptEntityKind::Guide)
        .map_err(reference_error)
}

fn reference_error(error: InkScriptReferenceError) -> MetadataColorGuideAdapterError {
    match error {
        InkScriptReferenceError::InvalidReference => {
            MetadataColorGuideAdapterError::InvalidTypedStep
        }
        InkScriptReferenceError::MissingReference => {
            MetadataColorGuideAdapterError::MissingReference
        }
        InkScriptReferenceError::KindMismatch => MetadataColorGuideAdapterError::TargetMismatch,
    }
}

fn grid_config(value: &InkScriptTypedValue) -> Result<GridConfig, MetadataColorGuideAdapterError> {
    let fields = record(value)?;
    Ok(GridConfig {
        origin_x: i32_value(field(fields, "origin_x")?)?,
        origin_y: i32_value(field(fields, "origin_y")?)?,
        spacing_x: u32_value(field(fields, "spacing_x")?)?,
        spacing_y: u32_value(field(fields, "spacing_y")?)?,
        subdivisions: u32_value(field(fields, "subdivisions")?)?,
    })
}

fn guide_axis(value: &InkScriptTypedValue) -> Result<GuideAxis, MetadataColorGuideAdapterError> {
    match enum_value(value)? {
        "horizontal" => Ok(GuideAxis::Horizontal),
        "vertical" => Ok(GuideAxis::Vertical),
        _ => Err(MetadataColorGuideAdapterError::InvalidValue),
    }
}

fn boolean(value: &InkScriptTypedValue) -> Result<bool, MetadataColorGuideAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => Ok(*value),
        _ => Err(MetadataColorGuideAdapterError::InvalidTypedStep),
    }
}

fn u32_value(value: &InkScriptTypedValue) -> Result<u32, MetadataColorGuideAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(MetadataColorGuideAdapterError::InvalidTypedStep),
    }
}

fn i32_value(value: &InkScriptTypedValue) -> Result<i32, MetadataColorGuideAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::I32(value) => Ok(*value),
        _ => Err(MetadataColorGuideAdapterError::InvalidTypedStep),
    }
}

fn narrow_u8(value: &InkScriptTypedValue) -> Result<u8, MetadataColorGuideAdapterError> {
    u8::try_from(u32_value(value)?).map_err(|_| MetadataColorGuideAdapterError::InvalidValue)
}

fn narrow_u16(value: &InkScriptTypedValue) -> Result<u16, MetadataColorGuideAdapterError> {
    u16::try_from(u32_value(value)?).map_err(|_| MetadataColorGuideAdapterError::InvalidValue)
}

fn enum_value(value: &InkScriptTypedValue) -> Result<&str, MetadataColorGuideAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(MetadataColorGuideAdapterError::InvalidTypedStep),
    }
}

fn string_value(value: &InkScriptTypedValue) -> Result<&str, MetadataColorGuideAdapterError> {
    match value.kind() {
        InkScriptTypedValueKind::String(value) => Ok(value),
        _ => Err(MetadataColorGuideAdapterError::InvalidTypedStep),
    }
}

fn validate_rgba(value: PixelValue) -> Result<(), MetadataColorGuideAdapterError> {
    if matches!(value, PixelValue::Rgba(_) | PixelValue::Rgba16(_)) {
        Ok(())
    } else {
        Err(MetadataColorGuideAdapterError::InvalidValue)
    }
}

fn validate_rgba_slice(
    values: &[PixelValue],
    maximum: usize,
) -> Result<(), MetadataColorGuideAdapterError> {
    if values.len() > maximum {
        return Err(MetadataColorGuideAdapterError::ResourceLimit);
    }
    values.iter().try_for_each(|value| validate_rgba(*value))
}

fn validate_chart_entries(
    entries: &[ColorChartEntry],
) -> Result<(), MetadataColorGuideAdapterError> {
    if entries.len() > MAX_APPLICATION_COLORS {
        return Err(MetadataColorGuideAdapterError::ResourceLimit);
    }
    for entry in entries {
        validate_rgba(entry.color)?;
        validate_chart_name(&entry.name)?;
    }
    Ok(())
}

fn validate_chart_name(name: &str) -> Result<(), MetadataColorGuideAdapterError> {
    if name.is_empty() || name.len() > MAX_COLOR_CHART_NAME_BYTES {
        Err(MetadataColorGuideAdapterError::InvalidValue)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::canonical_document_state;
    use crate::{DEFAULT_DPI_MILLI, DocumentStateDigest, MAX_GUIDES};

    fn core() -> Core {
        let mut core = Core::new();
        core.new_cell(8, 6, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core
    }

    fn digest(core: &Core) -> DocumentStateDigest {
        canonical_document_state(core.document.as_ref().unwrap())
            .unwrap()
            .1
    }

    fn fixture(index: usize) -> (Core, MetadataColorGuideInvocation) {
        let mut core = core();
        let invocation = match index {
            0 => MetadataColorGuideInvocation::SetMainLineColor(PixelValue::Rgba16([
                1,
                2,
                3,
                u16::MAX,
            ])),
            1 => MetadataColorGuideInvocation::ReplacePalette(vec![
                PixelValue::Rgba([1, 2, 3, 4]),
                PixelValue::Rgba16([5, 6, 7, 8]),
            ]),
            2 => MetadataColorGuideInvocation::ReplaceColorChart {
                entries: vec![
                    ColorChartEntry {
                        color: PixelValue::Rgba([10, 20, 30, 40]),
                        name: "Eight".to_owned(),
                    },
                    ColorChartEntry {
                        color: PixelValue::Rgba16([11, 22, 33, 44]),
                        name: "Sixteen\0exact".to_owned(),
                    },
                ],
                locked: true,
            },
            3 => MetadataColorGuideInvocation::Document(CanonicalInvocation::AddGuide {
                axis: GuideAxis::Vertical,
                position: 4,
            }),
            4 => {
                let id = core.add_guide(GuideAxis::Horizontal, 1).unwrap().1;
                MetadataColorGuideInvocation::Document(CanonicalInvocation::MoveGuide {
                    guide_id: id,
                    position: 3,
                })
            }
            5 => {
                let id = core.add_guide(GuideAxis::Horizontal, 1).unwrap().1;
                MetadataColorGuideInvocation::Document(CanonicalInvocation::DeleteGuide {
                    guide_id: id,
                })
            }
            6 => MetadataColorGuideInvocation::Document(CanonicalInvocation::SetGrid {
                grid: GridConfig {
                    origin_x: -2,
                    origin_y: 3,
                    spacing_x: 7,
                    spacing_y: 9,
                    subdivisions: 3,
                },
            }),
            7 => {
                core.add_guide(GuideAxis::Vertical, 2).unwrap();
                MetadataColorGuideInvocation::Document(CanonicalInvocation::DeleteAllGuides)
            }
            _ => panic!("unknown fixture"),
        };
        (core, invocation)
    }

    #[test]
    fn exact_catalog_codec_and_executor_equivalence_cover_all_m16_primitives() {
        assert_eq!(METADATA_COLOR_GUIDE_CATALOG.len(), 8);
        for (index, metadata) in METADATA_COLOR_GUIDE_CATALOG.iter().enumerate() {
            let (base, invocation) = fixture(index);
            let step = MetadataColorGuideScriptStep::from_canonical(&invocation).unwrap();
            let lowered = step.to_canonical().unwrap();
            assert_eq!(lowered, invocation, "codec fixture {index}");
            assert_eq!(metadata.command, step.typed.command());
            assert_eq!(metadata.primitive_id, invocation.primitive_id().unwrap());
            assert!(!metadata.equivalence_test.is_empty());

            let mut direct = base.clone();
            let mut scripted = base;
            let direct_result = invocation.clone().execute(&mut direct).unwrap();
            let scripted_result = lowered.execute(&mut scripted).unwrap();
            assert_eq!(direct_result.output_ids, scripted_result.output_ids);
            assert_eq!(
                direct_result.dispatch.revision(),
                scripted_result.dispatch.revision()
            );
            assert_eq!(digest(&direct), digest(&scripted));
            assert_eq!(direct.current_state, scripted.current_state);
            assert_eq!(direct.document_revision, scripted.document_revision);
            assert_eq!(direct.history_entries(), scripted.history_entries());
            assert_eq!(direct.next_id, scripted.next_id);
            assert_eq!(direct.savepoint, scripted.savepoint);
        }
    }

    #[test]
    fn exact_depth_metadata_no_op_and_guide_order_are_stable() {
        let mut core = core();
        for invocation in [
            MetadataColorGuideInvocation::SetMainLineColor(PixelValue::Rgba16([
                257,
                514,
                771,
                u16::MAX,
            ])),
            MetadataColorGuideInvocation::ReplacePalette(vec![
                PixelValue::Rgba([1, 2, 3, 4]),
                PixelValue::Rgba16([257, 514, 771, 1028]),
            ]),
            MetadataColorGuideInvocation::ReplaceColorChart {
                entries: vec![ColorChartEntry {
                    color: PixelValue::Rgba16([9, 8, 7, 6]),
                    name: "Name\0with NUL".to_owned(),
                }],
                locked: false,
            },
        ] {
            let lowered = MetadataColorGuideScriptStep::from_canonical(&invocation)
                .unwrap()
                .to_canonical()
                .unwrap();
            lowered.clone().execute(&mut core).unwrap();
            let before = (
                digest(&core),
                core.document_revision,
                core.history_entries(),
                core.current_state,
                core.next_id,
            );
            lowered.execute(&mut core).unwrap();
            assert_eq!(
                (
                    digest(&core),
                    core.document_revision,
                    core.history_entries(),
                    core.current_state,
                    core.next_id,
                ),
                before
            );
        }

        let first = MetadataColorGuideInvocation::Document(CanonicalInvocation::AddGuide {
            axis: GuideAxis::Vertical,
            position: 2,
        })
        .execute(&mut core)
        .unwrap()
        .output_ids[0];
        let second = MetadataColorGuideInvocation::Document(CanonicalInvocation::AddGuide {
            axis: GuideAxis::Horizontal,
            position: 3,
        })
        .execute(&mut core)
        .unwrap()
        .output_ids[0];
        let third = MetadataColorGuideInvocation::Document(CanonicalInvocation::AddGuide {
            axis: GuideAxis::Horizontal,
            position: 1,
        })
        .execute(&mut core)
        .unwrap()
        .output_ids[0];
        assert_eq!(
            core.guides().unwrap(),
            [
                crate::Guide {
                    id: third,
                    axis: GuideAxis::Horizontal,
                    position: 1,
                },
                crate::Guide {
                    id: second,
                    axis: GuideAxis::Horizontal,
                    position: 3,
                },
                crate::Guide {
                    id: first,
                    axis: GuideAxis::Vertical,
                    position: 2,
                },
            ]
        );
    }

    #[test]
    fn invalid_resource_missing_and_overflow_inputs_are_atomic() {
        assert_eq!(
            MetadataColorGuideScriptStep::from_canonical(
                &MetadataColorGuideInvocation::SetMainLineColor(PixelValue::Grayscale8(1))
            )
            .unwrap_err(),
            MetadataColorGuideAdapterError::InvalidValue
        );
        assert_eq!(
            MetadataColorGuideScriptStep::from_canonical(
                &MetadataColorGuideInvocation::ReplacePalette(vec![
                    PixelValue::Rgba([0, 0, 0, 0]);
                    MAX_APPLICATION_COLORS + 1
                ])
            )
            .unwrap_err(),
            MetadataColorGuideAdapterError::ResourceLimit
        );
        assert_eq!(MAX_GUIDES, MAX_APPLICATION_COLORS);

        let mut core = core();
        let before = (
            digest(&core),
            core.document_revision,
            core.history_entries(),
            core.current_state,
            core.next_id,
        );
        for invocation in [
            MetadataColorGuideInvocation::Document(CanonicalInvocation::SetGrid {
                grid: GridConfig {
                    spacing_x: 0,
                    ..GridConfig::default()
                },
            }),
            MetadataColorGuideInvocation::Document(CanonicalInvocation::MoveGuide {
                guide_id: u64::MAX,
                position: 0,
            }),
        ] {
            let lowered = MetadataColorGuideScriptStep::from_canonical(&invocation)
                .unwrap()
                .to_canonical()
                .unwrap();
            assert!(lowered.execute(&mut core).is_err());
            assert_eq!(
                (
                    digest(&core),
                    core.document_revision,
                    core.history_entries(),
                    core.current_state,
                    core.next_id,
                ),
                before
            );
        }
    }

    #[test]
    fn private_values_are_send_sync_and_publish_no_second_executor() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MetadataColorGuideScriptStep>();
        assert_send_sync::<MetadataColorGuideInvocation>();
        assert_send_sync::<MetadataColorGuideCatalogEntry>();
        assert_eq!(
            METADATA_COLOR_GUIDE_COMMANDS.len(),
            METADATA_COLOR_GUIDE_CATALOG.len()
        );
    }
}
