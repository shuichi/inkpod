use super::catalog::{
    CatalogAssetMetadata, CatalogCommandDomain, CatalogEditorMetadata, CatalogEntry, CatalogError,
    CatalogNumericExpression, CatalogPortabilityEvaluator, CatalogResultMetadata,
    CatalogWorkEstimate, CatalogWorkFormula, InkScriptCatalogView, InkScriptPortability,
    InkScriptPortabilityClass,
};
use crate::primitive::{inkscript, inkscript_batch};
use inkpod_format::{
    InkScriptDeclarationModel, InkScriptEnvelopeErrorCode, InkScriptInputDeclarationKind,
    InkScriptOrchestrationEnvelope, InkScriptPathIntentAccess, InkScriptRunParameterDecision,
    InkScriptSchemaView, InkScriptSemanticErrorCode, InkScriptSource, InkScriptTypeDiagnostic,
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
    UnsupportedInput,
    AssetsNotYetSupported,
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
    pub(crate) model: InkScriptDeclarationModel,
    pub(crate) envelope: InkScriptOrchestrationEnvelope,
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
    if envelope.inputs().len() != 1
        || !matches!(
            envelope.inputs()[0].kind(),
            InkScriptInputDeclarationKind::CurrentDocument | InkScriptInputDeclarationKind::File
        )
    {
        return Err(ScriptCompileError::UnsupportedInput);
    }
    let model =
        build_inkscript_declaration_model(&parsed, &schema).map_err(ScriptCompileError::Type)?;
    if !model.assets().is_empty() {
        return Err(ScriptCompileError::AssetsNotYetSupported);
    }
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
    let mut budget = ScriptBudget::default();
    for (step, arguments) in model.steps().iter().zip(&frozen_arguments) {
        if step.enabled() {
            add_budget(
                &mut budget,
                catalog
                    .evaluate_work(step.command(), arguments)
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
    let path_intent_digest = path_digest(&envelope);
    Ok(StaticScriptProgram {
        static_compile_digest,
        path_intent_digest,
        budget,
        parameters,
        frozen_arguments,
        model,
        envelope,
    })
}

pub(super) struct ScriptSchemas {
    enums: Vec<inkpod_format::InkScriptEnumSchema>,
    records: Vec<inkpod_format::InkScriptRecordSchema>,
    pub(super) commands: Vec<inkpod_format::InkScriptCommandSchema>,
}

impl ScriptSchemas {
    pub(super) fn new() -> Self {
        Self {
            enums: inkscript::LEGACY_SIMPLE_ENUMS
                .iter()
                .chain(inkscript_batch::LEGACY_IMAGE_ENUMS)
                .copied()
                .collect(),
            records: inkscript::LEGACY_SIMPLE_RECORDS
                .iter()
                .chain(inkscript_batch::LEGACY_IMAGE_RECORDS)
                .copied()
                .collect(),
            commands: inkscript::LEGACY_SIMPLE_COMMANDS
                .iter()
                .chain(inkscript_batch::LEGACY_IMAGE_COMMANDS)
                .copied()
                .collect(),
        }
    }

    pub(super) fn view(&self) -> Result<InkScriptSchemaView<'_>, ScriptCompileError> {
        InkScriptSchemaView::exact_current_with_catalog(
            &self.enums,
            &[],
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
        let (class, preconditions, work, projection, skip) = match schema.name() {
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
            _ => return Err(ScriptCompileError::Catalog(CatalogError::InvalidEntry)),
        };
        entries.push(CatalogEntry {
            schema: *schema,
            domain: CatalogCommandDomain::DocumentMutation,
            results: Vec::<CatalogResultMetadata>::new(),
            assets: Vec::<CatalogAssetMetadata>::new(),
            portability: CatalogPortabilityEvaluator {
                rules: Vec::new(),
                default: InkScriptPortability {
                    class,
                    required_preconditions: preconditions,
                },
            },
            work,
            editor: CatalogEditorMetadata {
                family: if schema.name().starts_with("apply_") || schema.name().contains("raster") {
                    "legacy_image"
                } else {
                    "legacy_simple"
                },
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
    )
}

fn portable(work: u64, projection: Option<&'static str>) -> EntryTuple {
    (
        InkScriptPortabilityClass::Portable,
        Vec::new(),
        literal_work(work),
        projection,
        false,
    )
}

fn restricted(work: u64, preconditions: &[&'static str], projection: &'static str) -> EntryTuple {
    (
        InkScriptPortabilityClass::RequiresBinding,
        preconditions.to_vec(),
        literal_work(work),
        Some(projection),
        true,
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

fn path_digest(envelope: &InkScriptOrchestrationEnvelope) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(PATH_INTENT_DIGEST_CONTEXT);
    for intent in envelope.path_intent_preview().intents() {
        hasher.update(&[match intent.access() {
            InkScriptPathIntentAccess::Read => 1,
            InkScriptPathIntentAccess::Enumerate => 2,
            InkScriptPathIntentAccess::Create => 3,
            InkScriptPathIntentAccess::Replace => 4,
        }]);
        hasher.update(
            &intent
                .input_index()
                .map_or(u64::MAX, |value| value as u64)
                .to_le_bytes(),
        );
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
