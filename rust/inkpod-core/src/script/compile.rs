use super::assets::{ScriptAssetError, external_asset_path};
use super::catalog::{
    CatalogAssetMetadata, CatalogAssetSummary, CatalogBooleanExpression, CatalogCommandDomain,
    CatalogComparison, CatalogEditorMetadata, CatalogEntry, CatalogError, CatalogNumericExpression,
    CatalogPortabilityEvaluator, CatalogResultMetadata, CatalogWorkEstimate, CatalogWorkFormula,
    InkScriptCatalogView, InkScriptPortability, InkScriptPortabilityClass,
};
use crate::primitive::{
    inkscript, inkscript_batch, inkscript_document_tree, inkscript_fill_gradient, inkscript_frame,
    inkscript_gesture_adjustment, inkscript_light_table, inkscript_metadata,
    inkscript_selection_floating, inkscript_stroke_geometry,
};
use inkpod_format::{
    INKSCRIPT_PRODUCTION_CATALOG_COMMAND_COUNT, InkScriptDeclarationModel,
    InkScriptEnvelopeErrorCode, InkScriptInputDeclarationKind, InkScriptOrchestrationEnvelope,
    InkScriptOutput, InkScriptPathIntentAccess, InkScriptRunParameterDecision, InkScriptSchemaView,
    InkScriptSemanticErrorCode, InkScriptSource, InkScriptTypeDiagnostic,
    InkScriptTypeDiagnosticCode, InkScriptTypedValue, InkScriptTypedValueKind,
    build_inkscript_declaration_model, build_inkscript_orchestration_envelope,
    build_inkscript_semantic, emit_inkscript_canonical, parse_inkscript,
    resolve_inkscript_run_parameters,
};
use std::collections::BTreeMap;

const STATIC_COMPILE_DIGEST_CONTEXT: &str = "inkpod.inkscript.static-compile.v1";
const PATH_INTENT_DIGEST_CONTEXT: &str = "inkpod.inkscript.path-intent.v1";
const MAX_SCRIPT_INVOCATIONS: u64 = 1_048_576;
const MAX_SCRIPT_WORK_UNITS: u64 = 1_100_000_000_000;
const MAX_SCRIPT_OUTPUT_GROWTH: u64 = 67_108_864;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Caller-lowerable compile limits bounded by the exact-current InkScript contract.
pub struct ScriptCompileLimits {
    invocations: u64,
}

impl ScriptCompileLimits {
    /// Returns the exact-current maximum compile envelope.
    pub const fn exact_current() -> Self {
        Self {
            invocations: MAX_SCRIPT_INVOCATIONS,
        }
    }

    /// Lowers the maximum invocation count; zero is normalized to one and the contract maximum
    /// cannot be raised.
    #[must_use]
    pub const fn with_invocations(mut self, maximum: u64) -> Self {
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
/// Stable failures produced before an InkScript execution plan can be created.
pub enum ScriptCompileError {
    /// Bounded parsing did not produce a valid exact-current file.
    Syntax,
    /// Semantic construction rejected the source.
    Semantic(InkScriptSemanticErrorCode),
    /// The orchestration envelope is invalid.
    Envelope(InkScriptEnvelopeErrorCode),
    /// Typed declaration or reference analysis failed.
    Type(InkScriptTypeDiagnostic),
    /// Resolving run parameters did not yield a closed typed value.
    Freeze(InkScriptTypeDiagnosticCode),
    /// The caller cancelled per-run parameter resolution.
    ParameterCancelled,
    /// A source path intent is invalid or exceeds its bound.
    InvalidPathIntent,
    /// Asset declaration validation failed.
    Asset(ScriptAssetError),
    /// The exact-current catalog rejected a command or formula.
    Catalog(CatalogError),
    /// A checked aggregate compile bound was exceeded.
    ResourceLimit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// Checked upper bounds computed from enabled invocations and exact catalog formulas.
pub struct ScriptBudget {
    pub(crate) max_invocations: u64,
    pub(crate) max_output_ids: u64,
    pub(crate) max_asset_bytes: u64,
    pub(crate) max_work_units: u64,
    pub(crate) max_output_growth: u64,
}

impl ScriptBudget {
    /// Returns the maximum number of canonical invocations.
    pub const fn max_invocations(self) -> u64 {
        self.max_invocations
    }

    /// Returns the maximum number of persistent output identifiers.
    pub const fn max_output_ids(self) -> u64 {
        self.max_output_ids
    }

    /// Returns the maximum number of logical asset bytes.
    pub const fn max_asset_bytes(self) -> u64 {
        self.max_asset_bytes
    }

    /// Returns the maximum catalog work-unit count.
    pub const fn max_work_units(self) -> u64 {
        self.max_work_units
    }

    /// Returns the maximum native output growth in bytes.
    pub const fn max_output_growth(self) -> u64 {
        self.max_output_growth
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Immutable exact-current program produced by [`compile_inkscript`].
///
/// The program owns all resolved parameters and frozen invocation values. It is safe to move
/// between threads, but execution remains subject to the caller's single-writer `Core` route.
pub struct StaticScriptProgram {
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

impl StaticScriptProgram {
    /// Returns the digest of canonical source semantics, resolved parameters, and frozen values.
    pub const fn static_compile_digest(&self) -> &[u8; 32] {
        &self.static_compile_digest
    }

    /// Returns the digest of every authority-free path intent in source order.
    pub const fn path_intent_digest(&self) -> &[u8; 32] {
        &self.path_intent_digest
    }

    /// Returns the checked aggregate catalog budget.
    pub const fn budget(&self) -> ScriptBudget {
        self.budget
    }

    /// Returns immutable authority-free path intents in source order.
    pub fn path_intents(&self) -> &[ScriptStaticPathIntent] {
        &self.path_intents
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Source declaration that owns one authority-free path intent.
pub enum ScriptPathIntentSubject {
    /// An input declaration at the given zero-based source index.
    Input(usize),
    /// A named external asset declaration.
    Asset(String),
    /// The output-root declaration.
    OutputRoot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Immutable path text and requested access captured during static compilation.
pub struct ScriptStaticPathIntent {
    id: u64,
    access: InkScriptPathIntentAccess,
    text: String,
    subject: ScriptPathIntentSubject,
}

impl ScriptStaticPathIntent {
    /// Returns the nonzero program-local intent identifier.
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Returns the requested path access without granting OS authority.
    pub const fn access(&self) -> InkScriptPathIntentAccess {
        self.access
    }

    /// Returns the exact source-owned path text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the declaration that owns this intent.
    pub const fn subject(&self) -> &ScriptPathIntentSubject {
        &self.subject
    }
}

/// Compiles one exact-current source with an explicit per-run parameter decision.
///
/// Parsing, semantic analysis, parameter freezing, catalog lookup, and resource calculation are
/// all-or-nothing and do not read paths or mutate a document.
pub fn compile_inkscript(
    source: &InkScriptSource,
    parameters: InkScriptRunParameterDecision,
) -> Result<StaticScriptProgram, ScriptCompileError> {
    compile_inkscript_with_limits(source, parameters, ScriptCompileLimits::exact_current())
}

/// Compiles with caller-lowered resource limits.
///
/// This has the same atomic ownership contract as [`compile_inkscript`]. Limits cannot raise the
/// exact-current contract envelope.
pub fn compile_inkscript_with_limits(
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
        let schemas = Self {
            enums: inkscript::LEGACY_SIMPLE_ENUMS
                .iter()
                .chain(inkscript_batch::LEGACY_IMAGE_ENUMS)
                .chain(inkscript_fill_gradient::FILL_GRADIENT_ENUMS)
                .chain(inkscript_gesture_adjustment::GESTURE_ADJUSTMENT_ENUMS)
                .chain(inkscript_selection_floating::SELECTION_FLOATING_ENUMS)
                .chain(inkscript_stroke_geometry::STROKE_GEOMETRY_ENUMS)
                .chain(inkscript_frame::FRAME_ENUMS)
                .chain(inkscript_light_table::LIGHT_TABLE_ENUMS)
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
                .chain(inkscript_fill_gradient::FILL_GRADIENT_RECORDS)
                .chain(inkscript_gesture_adjustment::GESTURE_ADJUSTMENT_RECORDS)
                .chain(inkscript_selection_floating::SELECTION_FLOATING_RECORDS)
                .chain(inkscript_stroke_geometry::STROKE_GEOMETRY_RECORDS)
                .chain(inkscript_frame::FRAME_RECORDS)
                .chain(inkscript_light_table::LIGHT_TABLE_RECORDS)
                .copied()
                .collect(),
            commands: inkscript::LEGACY_SIMPLE_COMMANDS
                .iter()
                .chain(inkscript_batch::LEGACY_IMAGE_COMMANDS)
                .chain(inkscript_document_tree::DOCUMENT_TREE_COMMANDS)
                .chain(inkscript_metadata::METADATA_COLOR_GUIDE_COMMANDS)
                .chain(inkscript_fill_gradient::FILL_GRADIENT_COMMANDS)
                .chain(inkscript_gesture_adjustment::GESTURE_ADJUSTMENT_COMMANDS)
                .chain(inkscript_selection_floating::SELECTION_FLOATING_COMMANDS)
                .chain(inkscript_stroke_geometry::STROKE_GEOMETRY_COMMANDS)
                .chain(inkscript_frame::FRAME_COMMANDS)
                .chain(inkscript_light_table::LIGHT_TABLE_COMMANDS)
                .copied()
                .collect(),
        };
        assert_eq!(
            schemas.commands.len(),
            INKSCRIPT_PRODUCTION_CATALOG_COMMAND_COUNT,
            "Rust command declarations must stay bijective with the frozen production catalog"
        );
        schemas
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
            "apply_gradient" => (
                InkScriptPortabilityClass::RequiresBinding,
                vec![
                    "semantic_target",
                    "state_coupled_raster",
                    "state_coupled_selection",
                ],
                CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::Literal(0),
                    max_asset_bytes: CatalogNumericExpression::CheckedAdd(
                        Box::new(CatalogNumericExpression::Literal(44)),
                        Box::new(CatalogNumericExpression::CheckedMultiply(
                            Box::new(CatalogNumericExpression::ListLength {
                                path: vec!["gradient", "stops"],
                                maximum: 64,
                            }),
                            Box::new(CatalogNumericExpression::Literal(12)),
                        )),
                    ),
                    max_work_units: CatalogNumericExpression::Literal(67_108_864),
                    max_output_growth: CatalogNumericExpression::Literal(0),
                },
                None,
                false,
                Vec::new(),
                "fill_gradient",
            ),
            "apply_blur" => gesture_adjustment_bound(
                literal_work_with_growth(1_100_000_000, 0),
                &raster_effect_preconditions(),
                Vec::new(),
            ),
            "apply_airbrush" => gesture_adjustment_bound(
                fixed_payload_work(52, 67_108_864, 0),
                &raster_effect_preconditions(),
                Vec::new(),
            ),
            "apply_airbrush_gesture" => gesture_adjustment_bound(
                list_payload_work(vec!["gesture", "samples"], 52, 20, 1_048_576, 1_048_576, 0),
                &raster_effect_preconditions(),
                Vec::new(),
            ),
            "apply_stamp" => gesture_adjustment_bound(
                fixed_payload_work(32, 67_108_864, 0),
                &raster_effect_preconditions(),
                Vec::new(),
            ),
            "apply_stamp_gesture" => gesture_adjustment_bound(
                list_payload_work(vec!["gesture", "samples"], 48, 20, 1_048_576, 1_048_576, 0),
                &raster_effect_preconditions(),
                Vec::new(),
            ),
            "apply_blur_tool" => gesture_adjustment_bound(
                selection_payload_work(vec!["shape"], 1_100_000_000, 0),
                &raster_effect_preconditions(),
                Vec::new(),
            ),
            "edit_plane_alpha" => gesture_adjustment_bound(
                CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::Literal(0),
                    max_asset_bytes: CatalogNumericExpression::Field(vec![
                        "alpha",
                        "logical_payload_bytes",
                    ]),
                    max_work_units: CatalogNumericExpression::Field(vec![
                        "alpha",
                        "logical_element_count",
                    ]),
                    max_output_growth: CatalogNumericExpression::Literal(0),
                },
                &raster_effect_preconditions(),
                Vec::new(),
            ),
            "apply_alpha_gradient" => gesture_adjustment_bound(
                gradient_payload_work("gradient", 0),
                &raster_effect_preconditions(),
                Vec::new(),
            ),
            "create_adjustment_layer" => gesture_adjustment_portable(
                adjustment_payload_work("adjustment", 1, 1),
                entity_result("layer"),
            ),
            "update_adjustment_layer" => gesture_adjustment_bound(
                adjustment_payload_work("adjustment", 0, 0),
                &["semantic_target", "state_coupled_adjustment"],
                Vec::new(),
            ),
            "scoped_color_replace" => gesture_adjustment_bound(
                fixed_payload_work(37_748_800, 67_108_864, 0),
                &[
                    "semantic_target",
                    "state_coupled_raster",
                    "state_coupled_selection",
                ],
                Vec::new(),
            ),
            "restore_selected_pixels" => selection_bound(
                list_payload_work(vec!["changes"], 0, 24, 1_048_576, 1, 0),
                &[
                    "semantic_target",
                    "exact_pixel_precondition",
                    "state_coupled_selection",
                ],
                Vec::new(),
            ),
            "apply_selection" => selection_bound(
                selection_payload_work(vec!["shape"], 67_108_864, 0),
                &[
                    "semantic_target",
                    "state_coupled_raster",
                    "state_coupled_selection",
                ],
                Vec::new(),
            ),
            "invert_selection" | "clear_selection" | "resize_selection" => selection_bound(
                literal_work(67_108_864),
                &["state_coupled_selection"],
                Vec::new(),
            ),
            "select_color" => selection_bound(
                literal_work(67_108_864),
                &[
                    "semantic_target",
                    "state_coupled_raster",
                    "state_coupled_selection",
                ],
                Vec::new(),
            ),
            "select_output_color_guard" => selection_strict(
                literal_work(67_108_864),
                &[
                    "exact_document_revision",
                    "state_coupled_composite",
                    "state_coupled_selection",
                ],
                Vec::new(),
            ),
            "selection_to_layer" => selection_bound(
                CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::Literal(2),
                    max_asset_bytes: CatalogNumericExpression::Literal(0),
                    max_work_units: CatalogNumericExpression::Literal(67_108_864),
                    max_output_growth: CatalogNumericExpression::Literal(1),
                },
                &["state_coupled_selection"],
                entity_result("layer"),
            ),
            "selection_from_layer" => selection_bound(
                literal_work(67_108_864),
                &["semantic_target", "state_coupled_selection"],
                Vec::new(),
            ),
            "clear_selected_content" => selection_bound(
                literal_work(67_108_864),
                &[
                    "semantic_target",
                    "state_coupled_raster",
                    "state_coupled_selection",
                ],
                Vec::new(),
            ),
            "commit_floating" => selection_bound(
                CatalogWorkFormula {
                    max_invocations: CatalogNumericExpression::Literal(1),
                    max_output_ids: CatalogNumericExpression::Literal(1),
                    max_asset_bytes: CatalogNumericExpression::Literal(536_870_912),
                    max_work_units: CatalogNumericExpression::Literal(1_100_000_000),
                    max_output_growth: CatalogNumericExpression::Literal(67_108_864),
                },
                &[
                    "semantic_target",
                    "state_coupled_raster",
                    "floating_asset_payload",
                ],
                Vec::new(),
            ),
            "edit_shooting_frame" => frame_bound(
                shooting_frame_work(),
                &["semantic_target"],
                ordered_entity_result("shooting_frames", "shooting_frame", 0),
            ),
            "edit_vanishing_points" => frame_bound(
                vanishing_point_work(),
                &["semantic_target"],
                ordered_entity_result("vanishing_points", "vanishing_point", 0),
            ),
            "light_table_set_global_opacity" => {
                light_table_strict(literal_work(1), &["active_light_table_set"], Vec::new())
            }
            "light_table_create_set" => light_table_bound(
                literal_work_with_outputs_and_growth(256, 1, 1),
                &["initial_light_table_set_names"],
                entity_result_with_role("set", "light_table_set"),
            ),
            "light_table_duplicate_set" => light_table_bound(
                literal_work_with_outputs_and_growth(4_097, 1, 4_097),
                &[
                    "semantic_target",
                    "state_coupled_light_table_items",
                    "initial_light_table_set_names",
                ],
                entity_result_with_role("set", "light_table_set"),
            ),
            "light_table_delete_set" => light_table_bound(
                literal_work(4_097),
                &["semantic_target", "initial_light_table_set_order"],
                Vec::new(),
            ),
            "light_table_rename_set" => {
                light_table_bound(literal_work(1), &["semantic_target"], Vec::new())
            }
            "light_table_reorder_set" => light_table_bound(
                literal_work(256),
                &["semantic_target", "initial_light_table_set_order"],
                Vec::new(),
            ),
            "light_table_set_active" => {
                light_table_bound(literal_work(1), &["semantic_target"], Vec::new())
            }
            "light_table_add_item" => light_table_strict(
                light_table_item_asset_work(&["input"], 1),
                &["active_light_table_set", "source_raster_asset"],
                entity_result_with_role("item", "light_table_item"),
            ),
            "light_table_update_item_properties" => light_table_bound(
                literal_work(1),
                &["semantic_target", "active_light_table_set"],
                Vec::new(),
            ),
            "light_table_update_item" => light_table_bound(
                light_table_item_asset_work(&["input"], 0),
                &[
                    "semantic_target",
                    "active_light_table_set",
                    "source_raster_asset",
                ],
                Vec::new(),
            ),
            "light_table_remove_item" => light_table_bound(
                literal_work(4_096),
                &["semantic_target", "active_light_table_set"],
                Vec::new(),
            ),
            "light_table_reorder_item" => light_table_bound(
                literal_work(4_096),
                &[
                    "semantic_target",
                    "active_light_table_set",
                    "initial_light_table_item_order",
                ],
                Vec::new(),
            ),
            "light_table_bulk_register" => light_table_bound(
                light_table_bulk_asset_work(),
                &["semantic_target", "source_raster_assets"],
                ordered_entity_result("items", "light_table_item", 0),
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
                    max_output_ids: CatalogNumericExpression::Literal(0),
                    max_asset_bytes: CatalogNumericExpression::Literal(0),
                    max_work_units: CatalogNumericExpression::Literal(16_777_216),
                    max_output_growth: CatalogNumericExpression::Literal(16_777_216),
                },
                Vec::new(),
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
        let assets = match schema.name() {
            "import_raster_asset" => vec![CatalogAssetMetadata {
                name: "source_raster",
                kind: "canonical_raster",
                inline: true,
                external: true,
            }],
            "edit_plane_alpha" => vec![CatalogAssetMetadata {
                name: "alpha_raster",
                kind: "canonical_raster",
                inline: true,
                external: true,
            }],
            "commit_floating" => vec![CatalogAssetMetadata {
                name: "floating_rasters",
                kind: "canonical_raster",
                inline: true,
                external: true,
            }],
            "light_table_add_item" | "light_table_update_item" | "light_table_bulk_register" => {
                vec![CatalogAssetMetadata {
                    name: "source_rasters",
                    kind: "canonical_raster",
                    inline: true,
                    external: true,
                }]
            }
            _ => Vec::new(),
        };
        entries.push(CatalogEntry {
            schema: *schema,
            domain: CatalogCommandDomain::DocumentMutation,
            results,
            assets,
            portability: CatalogPortabilityEvaluator {
                rules: frame_portability_rules(schema.name()),
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

fn entity_result_with_role(
    name: &'static str,
    owner_role: &'static str,
) -> Vec<CatalogResultMetadata> {
    vec![CatalogResultMetadata {
        name,
        namespace: Some("document_stable"),
        owner_role: Some(owner_role),
        output_id_ordinal: Some(0),
    }]
}

fn ordered_entity_result(
    name: &'static str,
    owner_role: &'static str,
    output_id_ordinal: u16,
) -> Vec<CatalogResultMetadata> {
    vec![CatalogResultMetadata {
        name,
        namespace: Some("document_stable"),
        owner_role: Some(owner_role),
        output_id_ordinal: Some(output_id_ordinal),
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

fn raster_effect_preconditions() -> [&'static str; 3] {
    [
        "semantic_target",
        "state_coupled_raster",
        "state_coupled_selection",
    ]
}

fn gesture_adjustment_bound(
    work: CatalogWorkFormula,
    preconditions: &[&'static str],
    results: Vec<CatalogResultMetadata>,
) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        preconditions.to_vec(),
        work,
        None,
        false,
        results,
        "gesture_alpha_adjustment",
    )
}

fn gesture_adjustment_portable(
    work: CatalogWorkFormula,
    results: Vec<CatalogResultMetadata>,
) -> EntryTuple {
    (
        InkScriptPortabilityClass::Portable,
        Vec::new(),
        work,
        None,
        false,
        results,
        "gesture_alpha_adjustment",
    )
}

fn selection_bound(
    work: CatalogWorkFormula,
    preconditions: &[&'static str],
    results: Vec<CatalogResultMetadata>,
) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        preconditions.to_vec(),
        work,
        None,
        false,
        results,
        "selection_floating",
    )
}

fn selection_strict(
    work: CatalogWorkFormula,
    preconditions: &[&'static str],
    results: Vec<CatalogResultMetadata>,
) -> EntryTuple {
    (
        InkScriptPortabilityClass::StrictSourceOnly,
        preconditions.to_vec(),
        work,
        None,
        false,
        results,
        "selection_floating",
    )
}

fn frame_bound(
    work: CatalogWorkFormula,
    preconditions: &[&'static str],
    results: Vec<CatalogResultMetadata>,
) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        preconditions.to_vec(),
        work,
        None,
        false,
        results,
        "frame_vanishing",
    )
}

fn light_table_bound(
    work: CatalogWorkFormula,
    preconditions: &[&'static str],
    results: Vec<CatalogResultMetadata>,
) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        preconditions.to_vec(),
        work,
        None,
        false,
        results,
        "light_table",
    )
}

fn light_table_strict(
    work: CatalogWorkFormula,
    preconditions: &[&'static str],
    results: Vec<CatalogResultMetadata>,
) -> EntryTuple {
    (
        InkScriptPortabilityClass::StrictSourceOnly,
        preconditions.to_vec(),
        work,
        None,
        false,
        results,
        "light_table",
    )
}

fn light_table_item_asset_work(root: &[&'static str], output_ids: u64) -> CatalogWorkFormula {
    let mut payload_path = root.to_vec();
    payload_path.extend(["source", "raster", "logical_payload_bytes"]);
    let mut element_path = root.to_vec();
    element_path.extend(["source", "raster", "logical_element_count"]);
    let payload = CatalogNumericExpression::Field(payload_path);
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(output_ids),
        max_asset_bytes: payload.clone(),
        max_work_units: CatalogNumericExpression::Field(element_path),
        max_output_growth: payload,
    }
}

fn light_table_bulk_asset_work() -> CatalogWorkFormula {
    let payload = CatalogNumericExpression::BoundedSum {
        path: vec!["inputs"],
        maximum_items: 4_096,
        body: Box::new(CatalogNumericExpression::Field(vec![
            "source",
            "raster",
            "logical_payload_bytes",
        ])),
    };
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::ListLength {
            path: vec!["inputs"],
            maximum: 4_096,
        },
        max_asset_bytes: payload.clone(),
        max_work_units: CatalogNumericExpression::BoundedSum {
            path: vec!["inputs"],
            maximum_items: 4_096,
            body: Box::new(CatalogNumericExpression::Field(vec![
                "source",
                "raster",
                "logical_element_count",
            ])),
        },
        max_output_growth: payload,
    }
}

fn shooting_frame_work() -> CatalogWorkFormula {
    let output_ids = CatalogNumericExpression::Conditional {
        condition: Box::new(operation_condition(&["edit", "operation"], 3)),
        when_true: Box::new(CatalogNumericExpression::Literal(0)),
        when_false: Box::new(CatalogNumericExpression::Literal(1)),
    };
    let growth = CatalogNumericExpression::Conditional {
        condition: Box::new(operation_condition(&["edit", "operation"], 1)),
        when_true: Box::new(CatalogNumericExpression::Literal(1)),
        when_false: Box::new(CatalogNumericExpression::Literal(0)),
    };
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: output_ids,
        max_asset_bytes: CatalogNumericExpression::Literal(56),
        max_work_units: CatalogNumericExpression::Literal(1),
        max_output_growth: growth,
    }
}

fn vanishing_point_work() -> CatalogWorkFormula {
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: operation_count(&["edits"], 1_024, &[1, 2]),
        max_asset_bytes: CatalogNumericExpression::CheckedMultiply(
            Box::new(CatalogNumericExpression::ListLength {
                path: vec!["edits"],
                maximum: 1_024,
            }),
            Box::new(CatalogNumericExpression::Literal(48)),
        ),
        max_work_units: CatalogNumericExpression::ListLength {
            path: vec!["edits"],
            maximum: 1_024,
        },
        max_output_growth: operation_count(&["edits"], 1_024, &[1]),
    }
}

fn frame_portability_rules(command: &str) -> Vec<(CatalogBooleanExpression, InkScriptPortability)> {
    match command {
        "edit_shooting_frame" => vec![(
            operation_condition(&["edit", "operation"], 1),
            InkScriptPortability {
                class: InkScriptPortabilityClass::Portable,
                required_preconditions: vec!["shooting_frame_absent"],
            },
        )],
        "edit_vanishing_points" => vec![(
            greater_than_zero(operation_count(&["edits"], 1_024, &[4])),
            InkScriptPortability {
                class: InkScriptPortabilityClass::StrictSourceOnly,
                required_preconditions: vec![
                    "exact_document_revision",
                    "state_coupled_vanishing_points",
                ],
            },
        )],
        _ => Vec::new(),
    }
}

fn operation_count(
    list_path: &[&'static str],
    maximum_items: u64,
    operations: &[u64],
) -> CatalogNumericExpression {
    let mut operations = operations.iter().copied();
    let first = operations.next().expect("operation set must be non-empty");
    let mut condition = operation_condition(&["operation"], first);
    for operation in operations {
        condition = or_conditions(condition, operation_condition(&["operation"], operation));
    }
    CatalogNumericExpression::BoundedSum {
        path: list_path.to_vec(),
        maximum_items,
        body: Box::new(CatalogNumericExpression::Conditional {
            condition: Box::new(condition),
            when_true: Box::new(CatalogNumericExpression::Literal(1)),
            when_false: Box::new(CatalogNumericExpression::Literal(0)),
        }),
    }
}

fn operation_condition(path: &[&'static str], operation: u64) -> CatalogBooleanExpression {
    CatalogBooleanExpression::Compare {
        comparison: CatalogComparison::Equal,
        left: CatalogNumericExpression::Field(path.to_vec()),
        right: CatalogNumericExpression::Literal(operation),
    }
}

fn or_conditions(
    left: CatalogBooleanExpression,
    right: CatalogBooleanExpression,
) -> CatalogBooleanExpression {
    CatalogBooleanExpression::Or(Box::new(left), Box::new(right))
}

fn greater_than_zero(value: CatalogNumericExpression) -> CatalogBooleanExpression {
    CatalogBooleanExpression::Compare {
        comparison: CatalogComparison::Greater,
        left: value,
        right: CatalogNumericExpression::Literal(0),
    }
}

fn literal_work_with_growth(work: u64, growth: u64) -> CatalogWorkFormula {
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(0),
        max_asset_bytes: CatalogNumericExpression::Literal(0),
        max_work_units: CatalogNumericExpression::Literal(work),
        max_output_growth: CatalogNumericExpression::Literal(growth),
    }
}

fn literal_work_with_outputs_and_growth(
    work: u64,
    output_ids: u64,
    growth: u64,
) -> CatalogWorkFormula {
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(output_ids),
        max_asset_bytes: CatalogNumericExpression::Literal(0),
        max_work_units: CatalogNumericExpression::Literal(work),
        max_output_growth: CatalogNumericExpression::Literal(growth),
    }
}

fn fixed_payload_work(payload: u64, work: u64, growth: u64) -> CatalogWorkFormula {
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(0),
        max_asset_bytes: CatalogNumericExpression::Literal(payload),
        max_work_units: CatalogNumericExpression::Literal(work),
        max_output_growth: CatalogNumericExpression::Literal(growth),
    }
}

fn list_payload_work(
    path: Vec<&'static str>,
    base_bytes: u64,
    element_bytes: u64,
    maximum: u64,
    work_per_element: u64,
    growth: u64,
) -> CatalogWorkFormula {
    let length = CatalogNumericExpression::ListLength { path, maximum };
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(0),
        max_asset_bytes: CatalogNumericExpression::CheckedAdd(
            Box::new(CatalogNumericExpression::Literal(base_bytes)),
            Box::new(CatalogNumericExpression::CheckedMultiply(
                Box::new(length.clone()),
                Box::new(CatalogNumericExpression::Literal(element_bytes)),
            )),
        ),
        max_work_units: CatalogNumericExpression::CheckedMultiply(
            Box::new(length),
            Box::new(CatalogNumericExpression::Literal(work_per_element)),
        ),
        max_output_growth: CatalogNumericExpression::Literal(growth),
    }
}

fn selection_payload_work(path: Vec<&'static str>, work: u64, growth: u64) -> CatalogWorkFormula {
    let mut points_path = path.clone();
    points_path.push("points");
    let mut samples_path = path;
    samples_path.push("samples");
    let points = CatalogNumericExpression::ListLength {
        path: points_path,
        maximum: 1_048_576,
    };
    let samples = CatalogNumericExpression::ListLength {
        path: samples_path,
        maximum: 1_048_576,
    };
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(0),
        max_asset_bytes: CatalogNumericExpression::CheckedAdd(
            Box::new(CatalogNumericExpression::Literal(64)),
            Box::new(CatalogNumericExpression::CheckedAdd(
                Box::new(CatalogNumericExpression::CheckedMultiply(
                    Box::new(points),
                    Box::new(CatalogNumericExpression::Literal(16)),
                )),
                Box::new(CatalogNumericExpression::CheckedMultiply(
                    Box::new(samples),
                    Box::new(CatalogNumericExpression::Literal(20)),
                )),
            )),
        ),
        max_work_units: CatalogNumericExpression::Literal(work),
        max_output_growth: CatalogNumericExpression::Literal(growth),
    }
}

fn gradient_payload_work(path: &'static str, growth: u64) -> CatalogWorkFormula {
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(0),
        max_asset_bytes: CatalogNumericExpression::CheckedAdd(
            Box::new(CatalogNumericExpression::Literal(44)),
            Box::new(CatalogNumericExpression::CheckedMultiply(
                Box::new(CatalogNumericExpression::ListLength {
                    path: vec![path, "stops"],
                    maximum: 64,
                }),
                Box::new(CatalogNumericExpression::Literal(12)),
            )),
        ),
        max_work_units: CatalogNumericExpression::Literal(67_108_864),
        max_output_growth: CatalogNumericExpression::Literal(growth),
    }
}

fn adjustment_payload_work(path: &'static str, output_ids: u64, growth: u64) -> CatalogWorkFormula {
    CatalogWorkFormula {
        max_invocations: CatalogNumericExpression::Literal(1),
        max_output_ids: CatalogNumericExpression::Literal(output_ids),
        max_asset_bytes: CatalogNumericExpression::CheckedAdd(
            Box::new(CatalogNumericExpression::Literal(40)),
            Box::new(CatalogNumericExpression::CheckedMultiply(
                Box::new(CatalogNumericExpression::ListLength {
                    path: vec![path, "points"],
                    maximum: 64,
                }),
                Box::new(CatalogNumericExpression::Literal(8)),
            )),
        ),
        max_work_units: CatalogNumericExpression::Literal(1),
        max_output_growth: CatalogNumericExpression::Literal(growth),
    }
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
