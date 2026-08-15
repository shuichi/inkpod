use super::assets::{ScriptAssetError, external_asset_path};
use super::catalog::{
    CatalogAssetMetadata, CatalogAssetSummary, CatalogCommandDomain, CatalogEditorMetadata,
    CatalogEntry, CatalogError, CatalogNumericExpression, CatalogPortabilityEvaluator,
    CatalogResultMetadata, CatalogWorkEstimate, CatalogWorkFormula, InkScriptCatalogView,
    InkScriptPortability, InkScriptPortabilityClass,
};
use crate::primitive::{
    inkscript, inkscript_batch, inkscript_document_tree, inkscript_metadata,
    inkscript_stroke_geometry,
};
use inkpod_format::{
    InkScriptDeclarationModel, InkScriptEnvelopeErrorCode, InkScriptInputDeclarationKind,
    InkScriptOrchestrationEnvelope, InkScriptOutput, InkScriptPathIntentAccess,
    InkScriptRunParameterDecision, InkScriptSchemaView, InkScriptSemanticErrorCode,
    InkScriptSource, InkScriptTypeDiagnostic, InkScriptTypeDiagnosticCode, InkScriptTypedValue,
    InkScriptTypedValueKind, build_inkscript_declaration_model,
    build_inkscript_orchestration_envelope, build_inkscript_semantic, emit_inkscript_canonical,
    parse_inkscript, resolve_inkscript_run_parameters,
};
use std::collections::BTreeMap;

const STATIC_COMPILE_DIGEST_CONTEXT: &str = "inkpod.inkscript.static-compile.v1";
const PATH_INTENT_DIGEST_CONTEXT: &str = "inkpod.inkscript.path-intent.v1";
const MAX_SCRIPT_INVOCATIONS: u64 = 1_048_576;
const MAX_SCRIPT_WORK_UNITS: u64 = 1_100_000_000_000;
const MAX_SCRIPT_OUTPUT_GROWTH: u64 = 67_108_864;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScriptCompileLimits {
    invocations: u64,
}

impl ScriptCompileLimits {
    pub(crate) const fn exact_current() -> Self {
        Self {
            invocations: MAX_SCRIPT_INVOCATIONS,
        }
    }

    pub(crate) const fn with_invocations(mut self, maximum: u64) -> Self {
        self.invocations = if maximum == 0 {
            1
        } else if maximum < MAX_SCRIPT_INVOCATIONS {
            maximum
        } else {
            MAX_SCRIPT_INVOCATIONS
        };
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScriptCompileError {
    Syntax,
    Semantic(InkScriptSemanticErrorCode),
    Envelope(InkScriptEnvelopeErrorCode),
    Type(InkScriptTypeDiagnostic),
    Freeze(InkScriptTypeDiagnosticCode),
    ParameterCancelled,
    InvalidPathIntent,
    Asset(ScriptAssetError),
    Catalog(CatalogError),
    ResourceLimit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScriptBudget {
    pub(crate) max_invocations: u64,
    pub(crate) max_output_ids: u64,
    pub(crate) max_asset_bytes: u64,
    pub(crate) max_work_units: u64,
    pub(crate) max_output_growth: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StaticScriptProgram {
    pub(crate) static_compile_digest: [u8; 32],
    pub(crate) path_intent_digest: [u8; 32],
    pub(crate) budget: ScriptBudget,
    pub(crate) parameters: BTreeMap<String, InkScriptTypedValue>,
    pub(crate) frozen_arguments: Vec<InkScriptTypedValue>,
    pub(crate) asset_summaries: BTreeMap<String, CatalogAssetSummary>,
    pub(crate) model: InkScriptDeclarationModel,
    pub(crate) envelope: InkScriptOrchestrationEnvelope,
    pub(crate) path_intents: Vec<ScriptStaticPathIntent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScriptPathIntentSubject {
    Input(usize),
    Asset(String),
    OutputRoot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScriptStaticPathIntent {
    id: u64,
    access: InkScriptPathIntentAccess,
    text: String,
    subject: ScriptPathIntentSubject,
}

impl ScriptStaticPathIntent {
    pub(crate) const fn id(&self) -> u64 {
        self.id
    }

    pub(crate) const fn access(&self) -> InkScriptPathIntentAccess {
        self.access
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) const fn subject(&self) -> &ScriptPathIntentSubject {
        &self.subject
    }
}

pub(crate) fn compile_inkscript(
    source: &InkScriptSource,
    parameters: InkScriptRunParameterDecision,
) -> Result<StaticScriptProgram, ScriptCompileError> {
    compile_inkscript_with_limits(source, parameters, ScriptCompileLimits::exact_current())
}

pub(crate) fn compile_inkscript_with_limits(
    source: &InkScriptSource,
    parameters: InkScriptRunParameterDecision,
    limits: ScriptCompileLimits,
) -> Result<StaticScriptProgram, ScriptCompileError> {
    let parsed = parse_inkscript(source);
    if !parsed.is_valid() {
        return Err(ScriptCompileError::Syntax);
    }
    let schemas = ScriptSchemas::new();
    let schema = schemas.view()?;
    let semantic = build_inkscript_semantic(&parsed, &schema)
        .map_err(|error| ScriptCompileError::Semantic(error.code()))?;
    let envelope = build_inkscript_orchestration_envelope(&semantic)
        .map_err(|error| ScriptCompileError::Envelope(error.code()))?;
    let model =
        build_inkscript_declaration_model(&parsed, &schema).map_err(ScriptCompileError::Type)?;
    let Some(run_parameters) = resolve_inkscript_run_parameters(&model, &schema, parameters)
        .map_err(ScriptCompileError::Type)?
    else {
        return Err(ScriptCompileError::ParameterCancelled);
    };
    let parameters = run_parameters
        .values()
        .iter()
        .map(|value| (value.name().to_owned(), value.value().clone()))
        .collect::<BTreeMap<_, _>>();
    let frozen_arguments = model
        .steps()
        .iter()
        .map(|step| run_parameters.freeze_value(step.arguments()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(ScriptCompileError::Freeze)?;
    let catalog = catalog(&schemas.commands)?;
    let asset_summaries = catalog_asset_summaries(&model)?;
    let mut budget = ScriptBudget::default();
    for (step, arguments) in model.steps().iter().zip(&frozen_arguments) {
        if step.enabled() {
            add_budget(
                &mut budget,
                catalog
                    .evaluate_work_with_assets(step.command(), arguments, &asset_summaries)
                    .map_err(ScriptCompileError::Catalog)?,
            )?;
        }
    }
    if budget.max_invocations > limits.invocations
        || budget.max_work_units > MAX_SCRIPT_WORK_UNITS
        || budget.max_output_growth > MAX_SCRIPT_OUTPUT_GROWTH
    {
        return Err(ScriptCompileError::ResourceLimit);
    }

    let canonical = emit_inkscript_canonical(&semantic, &schema)
        .map_err(|error| ScriptCompileError::Semantic(error.code()))?;
    let static_compile_digest = compile_digest(&canonical, &parameters, &frozen_arguments);
    let path_intents = build_path_intents(&envelope, &model)?;
    let path_intent_digest = path_digest(&path_intents);
    Ok(StaticScriptProgram {
        static_compile_digest,
        path_intent_digest,
        budget,
        parameters,
        frozen_arguments,
        asset_summaries,
        model,
        envelope,
        path_intents,
    })
}

fn catalog_asset_summaries(
    model: &InkScriptDeclarationModel,
) -> Result<BTreeMap<String, CatalogAssetSummary>, ScriptCompileError> {
    let mut summaries = BTreeMap::new();
    for asset in model.assets() {
        let InkScriptTypedValueKind::Record(body) = asset.body().kind() else {
            return Err(ScriptCompileError::Asset(
                ScriptAssetError::InvalidTypedModel,
            ));
        };
        let descriptor = body.get("descriptor").ok_or(ScriptCompileError::Asset(
            ScriptAssetError::InvalidTypedModel,
        ))?;
        let InkScriptTypedValueKind::Record(descriptor) = descriptor.kind() else {
            return Err(ScriptCompileError::Asset(
                ScriptAssetError::InvalidTypedModel,
            ));
        };
        let logical_element_count = match descriptor.get("element_count").map(|value| value.kind())
        {
            Some(InkScriptTypedValueKind::U64(value)) => *value,
            _ => {
                return Err(ScriptCompileError::Asset(
                    ScriptAssetError::InvalidTypedModel,
                ));
            }
        };
        let stride = match descriptor.get("stride").map(|value| value.kind()) {
            Some(InkScriptTypedValueKind::U32(value)) => u64::from(*value),
            _ => {
                return Err(ScriptCompileError::Asset(
                    ScriptAssetError::InvalidTypedModel,
                ));
            }
        };
        let height = match descriptor.get("height").map(|value| value.kind()) {
            Some(InkScriptTypedValueKind::U32(value)) => u64::from(*value),
            _ => {
                return Err(ScriptCompileError::Asset(
                    ScriptAssetError::InvalidTypedModel,
                ));
            }
        };
        let logical_payload_bytes = stride
            .checked_mul(height)
            .ok_or(ScriptCompileError::ResourceLimit)?;
        if summaries
            .insert(
                asset.name().to_owned(),
                CatalogAssetSummary {
                    logical_element_count,
                    logical_payload_bytes,
                },
            )
            .is_some()
        {
            return Err(ScriptCompileError::Asset(
                ScriptAssetError::InvalidTypedModel,
            ));
        }
    }
    Ok(summaries)
}

pub(super) struct ScriptSchemas {
    enums: Vec<inkpod_format::InkScriptEnumSchema>,
    constructors: Vec<inkpod_format::InkScriptConstructorSchema>,
    records: Vec<inkpod_format::InkScriptRecordSchema>,
    pub(super) commands: Vec<inkpod_format::InkScriptCommandSchema>,
}

impl ScriptSchemas {
    pub(super) fn new() -> Self {
        Self {
            enums: inkscript::LEGACY_SIMPLE_ENUMS
                .iter()
                .chain(inkscript_batch::LEGACY_IMAGE_ENUMS)
                .chain(inkscript_stroke_geometry::STROKE_GEOMETRY_ENUMS)
                .copied()
                .collect(),
            constructors: inkscript_document_tree::DOCUMENT_TREE_CONSTRUCTORS
                .iter()
                .chain(inkscript_metadata::METADATA_COLOR_GUIDE_CONSTRUCTORS)
                .copied()
                .collect(),
            records: inkscript::LEGACY_SIMPLE_RECORDS
                .iter()
                .chain(inkscript_batch::LEGACY_IMAGE_RECORDS)
                .chain(inkscript_document_tree::DOCUMENT_TREE_RECORDS)
                .chain(inkscript_metadata::METADATA_COLOR_GUIDE_RECORDS)
                .chain(inkscript_stroke_geometry::STROKE_GEOMETRY_RECORDS)
                .copied()
                .collect(),
            commands: inkscript::LEGACY_SIMPLE_COMMANDS
                .iter()
                .chain(inkscript_batch::LEGACY_IMAGE_COMMANDS)
                .chain(inkscript_document_tree::DOCUMENT_TREE_COMMANDS)
                .chain(inkscript_metadata::METADATA_COLOR_GUIDE_COMMANDS)
                .chain(inkscript_stroke_geometry::STROKE_GEOMETRY_COMMANDS)
                .copied()
                .collect(),
        }
    }

    pub(super) fn view(&self) -> Result<InkScriptSchemaView<'_>, ScriptCompileError> {
        InkScriptSchemaView::exact_current_with_catalog(
            &self.enums,
            &self.constructors,
            &self.records,
            &self.commands,
        )
        .map_err(|error| ScriptCompileError::Semantic(error.code()))
    }
}

pub(super) fn catalog(
    schemas: &[inkpod_format::InkScriptCommandSchema],
) -> Result<InkScriptCatalogView, ScriptCompileError> {
    let mut entries = Vec::with_capacity(schemas.len());
    for schema in schemas {
        let (class, preconditions, work, projection, skip, results, family) = match schema.name() {
            "set_layer_properties" => tuple(1, true, Some("layer_property")),
            "set_plane_properties" => tuple(1, true, Some("plane_property")),
            "convert_plane" => tuple(16_777_216, true, Some("plane_conversion")),
            "convert_layer" => tuple(16_777_216, true, None),
            "mirror_document" => portable(16_777_216, Some("mirror")),
            "rotate_document" => portable(16_777_216, Some("rotate_90")),
            "resize_document" => (
                InkScriptPortabilityClass::Portable,
                Vec::new(),
                CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::Literal(0),
                    max_asset_bytes: CatalogNumericExpression::Literal(0),
                    max_work_units: CatalogNumericExpression::CheckedMultiply(
                        Box::new(CatalogNumericExpression::Field(vec!["resize", "width"])),
                        Box::new(CatalogNumericExpression::Field(vec!["resize", "height"])),
                    ),
                    max_output_growth: CatalogNumericExpression::CheckedMultiply(
                        Box::new(CatalogNumericExpression::Field(vec!["resize", "width"])),
                        Box::new(CatalogNumericExpression::Field(vec!["resize", "height"])),
                    ),
                },
                Some("resize"),
                false,
                Vec::new(),
                "legacy_simple",
            ),
            "apply_fill" => restricted(
                16_777_216,
                &["semantic_target", "state_coupled_fill_boundary"],
                "continuous_fill_seed",
            ),
            "apply_boundary_airbrush" => {
                restricted(67_108_864, &["semantic_target"], "boundary_airbrush")
            }
            "apply_dust_removal" => restricted(67_108_864, &["semantic_target"], "dust_removal"),
            "apply_filter" => restricted(1_100_000_000, &["semantic_target"], "filter"),
            "replace_raster_colors" => {
                restricted(67_108_864, &["semantic_target"], "color_replace")
            }
            "separate_raster_colors" => restricted(
                67_108_864,
                &["semantic_target", "typed_destination"],
                "separation",
            ),
            "update_paper_frames" => document_tree_portable(1, vec![]),
            "create_layer" => document_tree_portable(1, entity_result("layer")),
            "duplicate_layer" => document_tree_bound(1, entity_result("layer")),
            "delete_layer" => document_tree_bound(67_108_864, vec![]),
            "merge_layer" => document_tree_bound_with(
                67_108_864,
                &["semantic_target", "adjacent_merge_target"],
                vec![],
            ),
            "reorder_layer" => document_tree_bound_with(
                1,
                &["semantic_target", "initial_document_tree_order"],
                vec![],
            ),
            "create_plane" => document_tree_bound(1, entity_result("plane")),
            "duplicate_plane" => document_tree_bound(67_108_864, entity_result("plane")),
            "delete_plane" => document_tree_bound(67_108_864, vec![]),
            "merge_plane" => document_tree_bound_with(
                67_108_864,
                &["semantic_target", "adjacent_merge_target"],
                vec![],
            ),
            "reorder_plane" => document_tree_bound_with(
                1,
                &["semantic_target", "initial_document_tree_order"],
                vec![],
            ),
            "delete_hidden_layers" => document_tree_portable(67_108_864, vec![]),
            "edit_targets" => (
                InkScriptPortabilityClass::RequiresBinding,
                vec!["semantic_target"],
                CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::ListLength {
                        path: vec!["targets"],
                        maximum: 4_096,
                    },
                    max_asset_bytes: CatalogNumericExpression::Literal(0),
                    max_work_units: CatalogNumericExpression::CheckedMultiply(
                        Box::new(CatalogNumericExpression::ListLength {
                            path: vec!["targets"],
                            maximum: 4_096,
                        }),
                        Box::new(CatalogNumericExpression::Literal(67_108_864)),
                    ),
                    max_output_growth: CatalogNumericExpression::Literal(67_108_864),
                },
                None,
                false,
                vec![
                    CatalogResultMetadata {
                        name: "layers",
                        namespace: Some("document_stable"),
                        owner_role: Some("layer"),
                        output_id_ordinal: None,
                    },
                    CatalogResultMetadata {
                        name: "planes",
                        namespace: Some("document_stable"),
                        owner_role: Some("plane"),
                        output_id_ordinal: None,
                    },
                ],
                "document_tree",
            ),
            "set_main_line_color" => metadata_portable(literal_work(1), Vec::new()),
            "replace_palette" => metadata_portable(list_work("colors", 4_096), Vec::new()),
            "replace_color_chart" => metadata_portable(list_work("entries", 4_096), Vec::new()),
            "add_guide" => {
                metadata_portable(literal_work_with_outputs(1, 1), entity_result("guide"))
            }
            "move_guide" | "delete_guide" => metadata_bound(literal_work(1), Vec::new()),
            "set_grid" => metadata_portable(literal_work(1), Vec::new()),
            "delete_all_guides" => metadata_portable(literal_work(4_096), Vec::new()),
            "apply_raster_stroke" => stroke_geometry_bound(
                CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::Literal(0),
                    max_asset_bytes: CatalogNumericExpression::CheckedAdd(
                        Box::new(CatalogNumericExpression::CheckedMultiply(
                            Box::new(CatalogNumericExpression::ListLength {
                                path: vec!["stroke", "samples"],
                                maximum: 1_048_576,
                            }),
                            Box::new(CatalogNumericExpression::Literal(24)),
                        )),
                        Box::new(CatalogNumericExpression::Literal(8)),
                    ),
                    max_work_units: CatalogNumericExpression::Literal(16_777_216),
                    max_output_growth: CatalogNumericExpression::Literal(16_777_216),
                },
                Vec::new(),
            ),
            "apply_geometry" => stroke_geometry_bound(
                CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::Literal(2),
                    max_asset_bytes: CatalogNumericExpression::Literal(0),
                    max_work_units: CatalogNumericExpression::Literal(16_777_216),
                    max_output_growth: CatalogNumericExpression::Literal(16_777_216),
                },
                vec![
                    CatalogResultMetadata {
                        name: "paths",
                        namespace: Some("document_stable"),
                        owner_role: Some("vector_path"),
                        output_id_ordinal: None,
                    },
                    CatalogResultMetadata {
                        name: "fills",
                        namespace: Some("document_stable"),
                        owner_role: Some("vector_fill"),
                        output_id_ordinal: None,
                    },
                ],
            ),
            "import_raster_asset" => stroke_geometry_bound(
                CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::Literal(0),
                    max_asset_bytes: CatalogNumericExpression::Field(vec![
                        "raster",
                        "logical_payload_bytes",
                    ]),
                    max_work_units: CatalogNumericExpression::Field(vec![
                        "raster",
                        "logical_element_count",
                    ]),
                    max_output_growth: CatalogNumericExpression::Field(vec![
                        "raster",
                        "logical_payload_bytes",
                    ]),
                },
                Vec::new(),
            ),
            _ => return Err(ScriptCompileError::Catalog(CatalogError::InvalidEntry)),
        };
        let assets = if schema.name() == "import_raster_asset" {
            vec![CatalogAssetMetadata {
                name: "source_raster",
                kind: "canonical_raster",
                inline: true,
                external: true,
            }]
        } else {
            Vec::new()
        };
        entries.push(CatalogEntry {
            schema: *schema,
            domain: CatalogCommandDomain::DocumentMutation,
            results,
            assets,
            portability: CatalogPortabilityEvaluator {
                rules: Vec::new(),
                default: InkScriptPortability {
                    class,
                    required_preconditions: preconditions,
                },
            },
            work,
            editor: CatalogEditorMetadata {
                family,
                legacy_projection: projection,
                allow_skip_dependents: skip,
            },
        });
    }
    InkScriptCatalogView::new(entries).map_err(ScriptCompileError::Catalog)
}

type EntryTuple = (
    InkScriptPortabilityClass,
    Vec<&'static str>,
    CatalogWorkFormula,
    Option<&'static str>,
    bool,
    Vec<CatalogResultMetadata>,
    &'static str,
);

fn literal_work(work: u64) -> CatalogWorkFormula {
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(0),
        max_asset_bytes: CatalogNumericExpression::Literal(0),
        max_work_units: CatalogNumericExpression::Literal(work),
        max_output_growth: CatalogNumericExpression::Literal(0),
    }
}

fn tuple(work: u64, skip: bool, projection: Option<&'static str>) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        vec!["semantic_target"],
        literal_work(work),
        projection,
        skip,
        Vec::new(),
        "legacy_simple",
    )
}

fn portable(work: u64, projection: Option<&'static str>) -> EntryTuple {
    (
        InkScriptPortabilityClass::Portable,
        Vec::new(),
        literal_work(work),
        projection,
        false,
        Vec::new(),
        "legacy_simple",
    )
}

fn restricted(work: u64, preconditions: &[&'static str], projection: &'static str) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        preconditions.to_vec(),
        literal_work(work),
        Some(projection),
        true,
        Vec::new(),
        "legacy_image",
    )
}

fn entity_result(name: &'static str) -> Vec<CatalogResultMetadata> {
    vec![CatalogResultMetadata {
        name,
        namespace: Some("document_stable"),
        owner_role: Some(name),
        output_id_ordinal: Some(0),
    }]
}

fn document_tree_portable(work: u64, results: Vec<CatalogResultMetadata>) -> EntryTuple {
    (
        InkScriptPortabilityClass::Portable,
        Vec::new(),
        literal_work_with_outputs(work, u64::from(!results.is_empty())),
        None,
        false,
        results,
        "document_tree",
    )
}

fn document_tree_bound(work: u64, results: Vec<CatalogResultMetadata>) -> EntryTuple {
    document_tree_bound_with(work, &["semantic_target"], results)
}

fn document_tree_bound_with(
    work: u64,
    preconditions: &[&'static str],
    results: Vec<CatalogResultMetadata>,
) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        preconditions.to_vec(),
        literal_work_with_outputs(work, u64::from(!results.is_empty())),
        None,
        false,
        results,
        "document_tree",
    )
}

fn literal_work_with_outputs(work: u64, output_ids: u64) -> CatalogWorkFormula {
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(output_ids),
        max_asset_bytes: CatalogNumericExpression::Literal(0),
        max_work_units: CatalogNumericExpression::Literal(work),
        max_output_growth: CatalogNumericExpression::Literal(0),
    }
}

fn list_work(path: &'static str, maximum: u64) -> CatalogWorkFormula {
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(0),
        max_asset_bytes: CatalogNumericExpression::Literal(0),
        max_work_units: CatalogNumericExpression::ListLength {
            path: vec![path],
            maximum,
        },
        max_output_growth: CatalogNumericExpression::Literal(0),
    }
}

fn metadata_portable(work: CatalogWorkFormula, results: Vec<CatalogResultMetadata>) -> EntryTuple {
    (
        InkScriptPortabilityClass::Portable,
        Vec::new(),
        work,
        None,
        false,
        results,
        "metadata_color_guide",
    )
}

fn metadata_bound(work: CatalogWorkFormula, results: Vec<CatalogResultMetadata>) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        vec!["semantic_target"],
        work,
        None,
        false,
        results,
        "metadata_color_guide",
    )
}

fn stroke_geometry_bound(
    work: CatalogWorkFormula,
    results: Vec<CatalogResultMetadata>,
) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        vec!["semantic_target"],
        work,
        None,
        false,
        results,
        "stroke_geometry_import",
    )
}

fn add_budget(
    total: &mut ScriptBudget,
    item: CatalogWorkEstimate,
) -> Result<(), ScriptCompileError> {
    total.max_invocations = total
        .max_invocations
        .checked_add(item.max_invocations)
        .ok_or(ScriptCompileError::ResourceLimit)?;
    total.max_output_ids = total
        .max_output_ids
        .checked_add(item.max_output_ids)
        .ok_or(ScriptCompileError::ResourceLimit)?;
    total.max_asset_bytes = total
        .max_asset_bytes
        .checked_add(item.max_asset_bytes)
        .ok_or(ScriptCompileError::ResourceLimit)?;
    total.max_work_units = total
        .max_work_units
        .checked_add(item.max_work_units)
        .ok_or(ScriptCompileError::ResourceLimit)?;
    total.max_output_growth = total
        .max_output_growth
        .checked_add(item.max_output_growth)
        .ok_or(ScriptCompileError::ResourceLimit)?;
    Ok(())
}

fn compile_digest(
    canonical: &[u8],
    parameters: &BTreeMap<String, InkScriptTypedValue>,
    arguments: &[InkScriptTypedValue],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(STATIC_COMPILE_DIGEST_CONTEXT);
    hash_bytes(&mut hasher, canonical);
    for (name, value) in parameters {
        hash_bytes(&mut hasher, name.as_bytes());
        hash_typed_value(&mut hasher, value);
    }
    for value in arguments {
        hash_typed_value(&mut hasher, value);
    }
    *hasher.finalize().as_bytes()
}

fn build_path_intents(
    envelope: &InkScriptOrchestrationEnvelope,
    model: &InkScriptDeclarationModel,
) -> Result<Vec<ScriptStaticPathIntent>, ScriptCompileError> {
    let mut intents = Vec::new();
    for (index, input) in envelope.inputs().iter().enumerate() {
        let access = match input.kind() {
            InkScriptInputDeclarationKind::File => Some(InkScriptPathIntentAccess::Read),
            InkScriptInputDeclarationKind::Folder => Some(InkScriptPathIntentAccess::Enumerate),
            InkScriptInputDeclarationKind::CurrentDocument
            | InkScriptInputDeclarationKind::CurrentSequence => None,
        };
        if let (Some(access), Some(text)) = (access, input.path_text()) {
            push_path_intent(
                &mut intents,
                access,
                text,
                ScriptPathIntentSubject::Input(index),
            )?;
        }
    }
    for asset in model.assets() {
        if let Some(path) = external_asset_path(asset).map_err(ScriptCompileError::Asset)? {
            push_path_intent(
                &mut intents,
                InkScriptPathIntentAccess::Read,
                path,
                ScriptPathIntentSubject::Asset(asset.name().to_owned()),
            )?;
        }
    }
    match envelope.output() {
        InkScriptOutput::Duplicate(output) | InkScriptOutput::NewSave(output) => {
            push_path_intent(
                &mut intents,
                InkScriptPathIntentAccess::Create,
                output.folder(),
                ScriptPathIntentSubject::OutputRoot,
            )?;
        }
        InkScriptOutput::ExplicitOverwrite => {
            for (index, input) in envelope.inputs().iter().enumerate() {
                let Some(text) = input.path_text() else {
                    continue;
                };
                push_path_intent(
                    &mut intents,
                    InkScriptPathIntentAccess::Replace,
                    text,
                    ScriptPathIntentSubject::Input(index),
                )?;
            }
        }
    }
    Ok(intents)
}

fn push_path_intent(
    intents: &mut Vec<ScriptStaticPathIntent>,
    access: InkScriptPathIntentAccess,
    text: &str,
    subject: ScriptPathIntentSubject,
) -> Result<(), ScriptCompileError> {
    if text.is_empty() && !matches!(subject, ScriptPathIntentSubject::OutputRoot) {
        return Err(ScriptCompileError::InvalidPathIntent);
    }
    validate_path_intent_text(text)?;
    let id = u64::try_from(intents.len())
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(ScriptCompileError::ResourceLimit)?;
    intents.push(ScriptStaticPathIntent {
        id,
        access,
        text: text.to_owned(),
        subject,
    });
    Ok(())
}

fn validate_path_intent_text(text: &str) -> Result<(), ScriptCompileError> {
    if text.starts_with("//")
        || text.starts_with("\\\\")
        || text.contains("://")
        || text == "~"
        || text.starts_with("~/")
        || text.starts_with("~\\")
        || text.contains('*')
        || text.contains('?')
        || text.split(['/', '\\']).any(|component| component == "..")
    {
        return Err(ScriptCompileError::InvalidPathIntent);
    }
    Ok(())
}

fn path_digest(intents: &[ScriptStaticPathIntent]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(PATH_INTENT_DIGEST_CONTEXT);
    for intent in intents {
        hasher.update(&[match intent.access() {
            InkScriptPathIntentAccess::Read => 1,
            InkScriptPathIntentAccess::Enumerate => 2,
            InkScriptPathIntentAccess::Create => 3,
            InkScriptPathIntentAccess::Replace => 4,
        }]);
        hasher.update(&intent.id().to_le_bytes());
        match intent.subject() {
            ScriptPathIntentSubject::Input(index) => {
                hasher.update(&[0]);
                hasher.update(&(*index as u64).to_le_bytes());
            }
            ScriptPathIntentSubject::Asset(name) => {
                hasher.update(&[1]);
                hash_bytes(&mut hasher, name.as_bytes());
            }
            ScriptPathIntentSubject::OutputRoot => {
                hasher.update(&[2]);
            }
        };
        hash_bytes(&mut hasher, intent.text().as_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn hash_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn hash_typed_value(hasher: &mut blake3::Hasher, value: &InkScriptTypedValue) {
    hash_bytes(hasher, value.type_name().as_bytes());
    match value.kind() {
        InkScriptTypedValueKind::Boolean(value) => {
            hasher.update(&[0, u8::from(*value)]);
        }
        InkScriptTypedValueKind::U32(value) => {
            hasher.update(&[1]);
            hasher.update(&value.to_le_bytes());
        }
        InkScriptTypedValueKind::I32(value) => {
            hasher.update(&[2]);
            hasher.update(&value.to_le_bytes());
        }
        InkScriptTypedValueKind::U64(value) => {
            hasher.update(&[3]);
            hasher.update(&value.to_le_bytes());
        }
        InkScriptTypedValueKind::I64(value) | InkScriptTypedValueKind::Q16(value) => {
            hasher.update(&[4]);
            hasher.update(&value.to_le_bytes());
        }
        InkScriptTypedValueKind::String(value)
        | InkScriptTypedValueKind::Uuid(value)
        | InkScriptTypedValueKind::Digest(value)
        | InkScriptTypedValueKind::Enum(value)
        | InkScriptTypedValueKind::AssetReference(value) => {
            hasher.update(&[5]);
            hash_bytes(hasher, value.as_bytes());
        }
        InkScriptTypedValueKind::Base64(value) => {
            hasher.update(&[6]);
            hash_bytes(hasher, value);
        }
        InkScriptTypedValueKind::Constructor { name, arguments } => {
            hasher.update(&[7]);
            hash_bytes(hasher, name.as_bytes());
            for value in arguments {
                hash_typed_value(hasher, value);
            }
        }
        InkScriptTypedValueKind::None => {
            hasher.update(&[8]);
        }
        InkScriptTypedValueKind::List(values) => {
            hasher.update(&[9]);
            for value in values {
                hash_typed_value(hasher, value);
            }
        }
        InkScriptTypedValueKind::Record(fields) => {
            hasher.update(&[10]);
            for (name, value) in fields {
                hash_bytes(hasher, name.as_bytes());
                hash_typed_value(hasher, value);
            }
        }
        InkScriptTypedValueKind::Reference { root, segments } => {
            hasher.update(&[11]);
            hash_bytes(hasher, root.as_bytes());
            for segment in segments {
                match segment {
                    inkpod_format::InkScriptReferenceSegment::Field(_) => hasher.update(&[0]),
                    inkpod_format::InkScriptReferenceSegment::Index(_) => hasher.update(&[1]),
                };
                let value = match segment {
                    inkpod_format::InkScriptReferenceSegment::Field(value)
                    | inkpod_format::InkScriptReferenceSegment::Index(value) => value,
                };
                hash_bytes(hasher, value.as_bytes());
            }
        }
    };
}
