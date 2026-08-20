use super::assets::{FrozenScriptAssets, ScriptAssetError};
use super::bind::{
    InkScriptBindingError, InkScriptBoundValue, InkScriptComparableValue, InkScriptEntityReference,
    InkScriptEntitySnapshot, InkScriptInitialDocumentSnapshot, InkScriptPreparedStatement,
    InkScriptSelectionSnapshot, prepare_inkscript_initial_state_with_parameters,
};
use super::compile::{ScriptCompileError, ScriptSchemas, StaticScriptProgram, catalog};
use super::report::{ScriptDryRunReport, ScriptResultValue, ScriptStatementOutcome};
use crate::primitive::{
    DocumentTreeAdapterError, DocumentTreeScriptStep, FillGradientAdapterError,
    FillGradientScriptStep, FrameAdapterError, FrameScriptStep, GestureAdjustmentAdapterError,
    GestureAdjustmentScriptAction, InkScriptEntityKind, InkScriptRuntimeReferences,
    InvocationResult, LegacyImageAdapterError, LegacyImageScriptStep, LegacySimpleAdapterError,
    LegacySimpleScriptStep, LightTableAdapterError, LightTableScriptAction,
    MetadataColorGuideAdapterError, MetadataColorGuideScriptStep, SelectionFloatingAdapterError,
    SelectionFloatingScriptAction, StrokeGeometryImportAction, StrokeGeometryImportAdapterError,
};
use crate::{
    Core, CoreError, DocumentStateDigest, LayerKind, MAX_PERSISTENT_NUMERIC_ID, PixelFormat,
    PlaneType, PrimitiveRequest,
};
use inkpod_format::{
    InkScriptInputDeclarationKind, InkScriptResultAvailability, InkScriptResultCardinality,
    InkScriptTypedProgramNode, decode_procedure_file,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Immutable identity and revision fingerprint for one captured in-memory input.
pub struct InMemoryInputFingerprint {
    document_uuid: u128,
    document_revision: u64,
    state_digest: DocumentStateDigest,
    next_stable_id: u64,
    next_procedure_id: u64,
    next_state_id: u64,
}

#[derive(Clone, Copy, Debug)]
/// Borrowed input captured for one staged InkScript run.
pub enum CapturedScriptInput<'a> {
    /// A `Core` and its immutable capture-time fingerprint.
    InMemory {
        /// Borrowed source document; the staged runner never mutates it.
        core: &'a Core,
        /// Capture-time identity, revision, digest, and ID high-watermarks.
        fingerprint: InMemoryInputFingerprint,
    },
    /// Exact-current native `.inkpod` bytes decoded into a new staged `Core`.
    NativeBytes(&'a [u8]),
}

#[derive(Debug)]
/// A successful staged run and its deterministic report.
pub struct ScriptDryRunResult {
    pub(crate) report: ScriptDryRunReport,
    pub(crate) staged: Core,
}

impl ScriptDryRunResult {
    /// Returns the immutable execution report.
    pub const fn report(&self) -> &ScriptDryRunReport {
        &self.report
    }

    /// Returns the staged document. The captured source document remains unchanged.
    pub const fn staged(&self) -> &Core {
        &self.staged
    }

    /// Returns the staged document mutably for explicit post-run inspection such as Undo/Redo.
    pub fn staged_mut(&mut self) -> &mut Core {
        &mut self.staged
    }

    /// Consumes the result and returns the staged document.
    pub fn into_staged(self) -> Core {
        self.staged
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Stable failures from initial binding or staged execution.
pub enum ScriptRunError {
    /// Initial selector or assertion binding failed.
    Binding(InkScriptBindingError),
    /// The compiled program no longer satisfies the exact catalog contract.
    Compile(ScriptCompileError),
    /// The caller cancelled before a staged result was published.
    Cancelled,
    /// The captured input changed after capture.
    StaleInput,
    /// A checked execution or allocation resource bound was exceeded.
    ResourceLimit,
    /// The captured input kind or native bytes are invalid for this program.
    InvalidInput,
    /// One typed invocation is invalid for the staged document.
    InvalidStep,
    /// A later reference requires a result that was not produced.
    MissingResult,
    /// The canonical Core executor or native decoder failed.
    Core(CoreError),
}

impl From<CoreError> for ScriptRunError {
    fn from(value: CoreError) -> Self {
        if value == CoreError::Cancelled {
            Self::Cancelled
        } else {
            Self::Core(value)
        }
    }
}

impl From<InkScriptBindingError> for ScriptRunError {
    fn from(value: InkScriptBindingError) -> Self {
        Self::Binding(value)
    }
}

/// Captures the identity, revision, state digest, and ID high-watermarks of one document.
///
/// The returned value is opaque and can be paired with [`capture_in_memory_input_at`] to prove
/// stale-input rejection. This query does not mutate the document.
pub fn capture_in_memory_fingerprint(core: &Core) -> Result<InMemoryInputFingerprint, CoreError> {
    let info = core.document_info()?;
    Ok(InMemoryInputFingerprint {
        document_uuid: info.document_uuid,
        document_revision: info.document_revision,
        state_digest: core.document_state_digest()?,
        next_stable_id: core.next_id.next_raw(),
        next_procedure_id: core.next_procedure.get(),
        next_state_id: core.next_state.get(),
    })
}

/// Captures one borrowed in-memory input without mutating or cloning its document state.
pub fn capture_in_memory_input(core: &Core) -> Result<CapturedScriptInput<'_>, CoreError> {
    Ok(CapturedScriptInput::InMemory {
        core,
        fingerprint: capture_in_memory_fingerprint(core)?,
    })
}

/// Pairs a document with an earlier fingerprint for explicit stale-input validation.
pub const fn capture_in_memory_input_at(
    core: &Core,
    fingerprint: InMemoryInputFingerprint,
) -> CapturedScriptInput<'_> {
    CapturedScriptInput::InMemory { core, fingerprint }
}

/// Captures borrowed exact-current native bytes for staged decoding and execution.
pub const fn native_script_input(bytes: &[u8]) -> CapturedScriptInput<'_> {
    CapturedScriptInput::NativeBytes(bytes)
}

/// Binds and executes a compiled program against a private staged `Core`.
///
/// The source `Core` or native byte slice is never mutated. Cancellation, stale input, invalid
/// binding, overflow, resource failure, and canonical execution failure return no partial staged
/// result. A successful result preserves ordinary per-invocation history and Undo/Redo behavior.
pub fn run_inkscript_dry(
    program: &StaticScriptProgram,
    input: CapturedScriptInput<'_>,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<ScriptDryRunResult, ScriptRunError> {
    if cancelled() {
        return Err(ScriptRunError::Cancelled);
    }
    let working = match input {
        CapturedScriptInput::InMemory { core, fingerprint } => {
            if program.envelope.inputs()[0].kind() != InkScriptInputDeclarationKind::CurrentDocument
            {
                return Err(ScriptRunError::InvalidInput);
            }
            if capture_in_memory_fingerprint(core)? != fingerprint {
                return Err(ScriptRunError::StaleInput);
            }
            core.clone()
        }
        CapturedScriptInput::NativeBytes(bytes) => {
            if program.envelope.inputs()[0].kind() != InkScriptInputDeclarationKind::File {
                return Err(ScriptRunError::InvalidInput);
            }
            let file = decode_procedure_file(bytes)
                .map_err(|error| ScriptRunError::Core(CoreError::Format(error.to_string())))?;
            Core::from_procedure_file(file)?
        }
    };
    run_inkscript_on_staged_core(program, working, None, cancelled)
}

pub(super) fn run_inkscript_on_staged_core(
    program: &StaticScriptProgram,
    mut working: Core,
    assets: Option<&FrozenScriptAssets>,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<ScriptDryRunResult, ScriptRunError> {
    if cancelled() {
        return Err(ScriptRunError::Cancelled);
    }
    preflight_resources(&working, program)?;

    let schemas = ScriptSchemas::new();
    let schema = schemas.view().map_err(ScriptRunError::Compile)?;
    let catalog = catalog(&schemas.commands).map_err(ScriptRunError::Compile)?;
    let snapshot = initial_snapshot(&working)?;
    let prepared = prepare_inkscript_initial_state_with_parameters(
        &program.model,
        &schema,
        &catalog,
        &snapshot,
        &program.parameters,
        &program.frozen_arguments,
        &program.asset_summaries,
    )?;

    let mut statements = Vec::with_capacity(prepared.statements.len());
    let mut results = Vec::new();
    let mut runtime_references = initial_runtime_references(&prepared.bindings)?;
    let mut commits = 0_u64;
    let mut step_index = 0usize;
    if program.model.program().len() != prepared.statements.len() {
        return Err(ScriptRunError::InvalidStep);
    }
    for (node, prepared_statement) in program.model.program().iter().zip(&prepared.statements) {
        if cancelled() {
            return Err(ScriptRunError::Cancelled);
        }
        match (*node, *prepared_statement) {
            (InkScriptTypedProgramNode::Assert(_), InkScriptPreparedStatement::AssertPassed) => {
                statements.push(ScriptStatementOutcome::AssertPassed);
            }
            (InkScriptTypedProgramNode::Assert(_), InkScriptPreparedStatement::Skipped) => {
                statements.push(ScriptStatementOutcome::Skipped);
            }
            (InkScriptTypedProgramNode::Step(_), InkScriptPreparedStatement::Disabled) => {
                statements.push(ScriptStatementOutcome::Disabled);
                step_index += 1;
            }
            (InkScriptTypedProgramNode::Step(_), InkScriptPreparedStatement::Skipped) => {
                statements.push(ScriptStatementOutcome::Skipped);
                step_index += 1;
            }
            (InkScriptTypedProgramNode::Step(index), InkScriptPreparedStatement::StepReady) => {
                if index != step_index {
                    return Err(ScriptRunError::InvalidStep);
                }
                let step = &program.model.steps()[index];
                let before_revision = working.document_info()?.document_revision;
                let (result, output_kinds) = if is_simple(step.command()) {
                    let invocation = LegacySimpleScriptStep::from_compiled(
                        step,
                        program.frozen_arguments[index].clone(),
                        &runtime_references,
                    )
                    .and_then(|step| step.to_canonical())
                    .map_err(simple_adapter_error)?;
                    (working.execute_canonical_invocation(invocation), Vec::new())
                } else if is_document_tree(step.command()) {
                    let invocation = DocumentTreeScriptStep::from_compiled(
                        step,
                        program.frozen_arguments[index].clone(),
                        &runtime_references,
                    )
                    .and_then(|step| step.to_canonical())
                    .map_err(document_tree_adapter_error)?;
                    let output_kinds = DocumentTreeScriptStep::output_entity_kinds(&invocation);
                    (
                        working.execute_canonical_invocation(invocation),
                        output_kinds,
                    )
                } else if is_metadata_color_guide(step.command()) {
                    let invocation = MetadataColorGuideScriptStep::from_compiled(
                        step,
                        program.frozen_arguments[index].clone(),
                        &runtime_references,
                    )
                    .to_canonical()
                    .map_err(metadata_color_guide_adapter_error)?;
                    let output_kinds =
                        MetadataColorGuideScriptStep::output_entity_kinds(&invocation);
                    (invocation.execute(&mut working), output_kinds)
                } else if is_stroke_geometry_import(step.command()) {
                    let action = StrokeGeometryImportAction::from_compiled(
                        step,
                        &program.frozen_arguments[index],
                        &runtime_references,
                    )
                    .map_err(stroke_geometry_import_adapter_error)?;
                    let result = match &action {
                        StrokeGeometryImportAction::RasterStroke(arguments) => working
                            .execute_canonical_stroke_arguments(arguments.clone())
                            .map(|outcome| InvocationResult::dispatch(outcome.dispatch())),
                        StrokeGeometryImportAction::Geometry(invocation) => {
                            working.execute_canonical_invocation(invocation.clone())
                        }
                        StrokeGeometryImportAction::ImportRaster {
                            plane_id,
                            asset_symbol,
                        } => {
                            let assets = assets.ok_or(ScriptRunError::InvalidStep)?;
                            let role = catalog
                                .entry(step.command())
                                .map_err(ScriptCompileError::Catalog)
                                .map_err(ScriptRunError::Compile)?
                                .assets
                                .first()
                                .ok_or(ScriptRunError::InvalidStep)?;
                            let _role_plan = assets
                                .bind_role(role, asset_symbol)
                                .map_err(script_asset_error)?;
                            let raster = assets
                                .raster_input(asset_symbol)
                                .map_err(script_asset_error)?;
                            let expected_revision = working.document_info()?.document_revision;
                            working
                                .execute_primitive(PrimitiveRequest::ImportRasterAsset {
                                    expected_revision,
                                    target_plane_id: *plane_id,
                                    raster,
                                })
                                .map(|outcome| InvocationResult::dispatch(outcome.dispatch()))
                        }
                    };
                    let result = result?;
                    let output_kinds = action
                        .output_entity_kinds(result.output_ids.len())
                        .map_err(stroke_geometry_import_adapter_error)?;
                    (Ok(result), output_kinds)
                } else if is_fill_gradient(step.command()) {
                    let invocation = FillGradientScriptStep::from_compiled(
                        step,
                        &program.frozen_arguments[index],
                        &runtime_references,
                    )
                    .map(|step| step.to_canonical())
                    .map_err(fill_gradient_adapter_error)?;
                    (working.execute_canonical_invocation(invocation), Vec::new())
                } else if is_gesture_adjustment(step.command()) {
                    let action = GestureAdjustmentScriptAction::from_compiled(
                        step,
                        &program.frozen_arguments[index],
                        &runtime_references,
                    )
                    .map_err(gesture_adjustment_adapter_error)?;
                    let result = match &action {
                        GestureAdjustmentScriptAction::Canonical(invocation) => {
                            working.execute_canonical_invocation(invocation.clone())
                        }
                        GestureAdjustmentScriptAction::EditAlpha {
                            plane_id,
                            asset_symbol,
                        } => {
                            let assets = assets.ok_or(ScriptRunError::InvalidStep)?;
                            let role = catalog
                                .entry(step.command())
                                .map_err(ScriptCompileError::Catalog)
                                .map_err(ScriptRunError::Compile)?
                                .assets
                                .first()
                                .ok_or(ScriptRunError::InvalidStep)?;
                            let _role_plan = assets
                                .bind_role(role, asset_symbol)
                                .map_err(script_asset_error)?;
                            let alpha = assets.raster(asset_symbol).map_err(script_asset_error)?;
                            working.execute_canonical_invocation(
                                crate::primitive::CanonicalInvocation::EditPlaneAlpha {
                                    plane_id: *plane_id,
                                    alpha: alpha.clone(),
                                },
                            )
                        }
                    };
                    let result = result?;
                    let output_kinds = action
                        .output_entity_kinds(result.output_ids.len())
                        .map_err(gesture_adjustment_adapter_error)?;
                    (Ok(result), output_kinds)
                } else if is_selection_floating(step.command()) {
                    let action = SelectionFloatingScriptAction::from_compiled(
                        step,
                        &program.frozen_arguments[index],
                        &runtime_references,
                    )
                    .map_err(selection_floating_adapter_error)?;
                    let symbols = action.asset_symbols();
                    let mut rasters = Vec::new();
                    if !symbols.is_empty() {
                        let assets = assets.ok_or(ScriptRunError::InvalidStep)?;
                        let role = catalog
                            .entry(step.command())
                            .map_err(ScriptCompileError::Catalog)
                            .map_err(ScriptRunError::Compile)?
                            .assets
                            .first()
                            .ok_or(ScriptRunError::InvalidStep)?;
                        rasters
                            .try_reserve_exact(symbols.len())
                            .map_err(|_| ScriptRunError::ResourceLimit)?;
                        for symbol in symbols {
                            let _role_plan =
                                assets.bind_role(role, symbol).map_err(script_asset_error)?;
                            rasters.push(assets.raster(symbol).map_err(script_asset_error)?);
                        }
                    }
                    let invocation = action
                        .to_canonical_with_rasters(&rasters)
                        .map_err(selection_floating_adapter_error)?;
                    let result = working.execute_canonical_invocation(invocation)?;
                    let output_kinds = action
                        .output_entity_kinds(result.output_ids.len())
                        .map_err(selection_floating_adapter_error)?;
                    (Ok(result), output_kinds)
                } else if is_frame(step.command()) {
                    let invocation = FrameScriptStep::from_compiled(
                        step,
                        &program.frozen_arguments[index],
                        &runtime_references,
                    )
                    .map_err(frame_adapter_error)?;
                    let result = working.execute_canonical_invocation(invocation.to_canonical())?;
                    let output_kinds = invocation
                        .output_entity_kinds(result.output_ids.len())
                        .map_err(frame_adapter_error)?;
                    (Ok(result), output_kinds)
                } else if is_light_table(step.command()) {
                    let action = LightTableScriptAction::from_compiled(
                        step,
                        &program.frozen_arguments[index],
                        &runtime_references,
                    )
                    .map_err(light_table_adapter_error)?;
                    let symbols = action.asset_symbols();
                    let mut records = Vec::new();
                    if !symbols.is_empty() {
                        let assets = assets.ok_or(ScriptRunError::InvalidStep)?;
                        let role = catalog
                            .entry(step.command())
                            .map_err(ScriptCompileError::Catalog)
                            .map_err(ScriptRunError::Compile)?
                            .assets
                            .first()
                            .ok_or(ScriptRunError::InvalidStep)?;
                        records
                            .try_reserve_exact(symbols.len())
                            .map_err(|_| ScriptRunError::ResourceLimit)?;
                        for symbol in symbols {
                            let _role_plan =
                                assets.bind_role(role, symbol).map_err(script_asset_error)?;
                            records.push(assets.raster_record(symbol).map_err(script_asset_error)?);
                        }
                    }
                    let invocation = action
                        .to_canonical_with_assets(&records)
                        .map_err(light_table_adapter_error)?;
                    let result = working.execute_canonical_invocation(invocation)?;
                    let output_kinds = action
                        .output_entity_kinds(result.output_ids.len())
                        .map_err(light_table_adapter_error)?;
                    (Ok(result), output_kinds)
                } else {
                    let invocation = LegacyImageScriptStep::from_compiled(
                        step,
                        program.frozen_arguments[index].clone(),
                        &runtime_references,
                    )
                    .and_then(|step| step.to_canonical())
                    .map_err(image_adapter_error)?;
                    (working.execute_canonical_invocation(invocation), Vec::new())
                };
                let result = result?;
                let changed = result.dispatch.revision() != before_revision;
                if changed {
                    commits = commits
                        .checked_add(1)
                        .ok_or(ScriptRunError::ResourceLimit)?;
                    statements.push(ScriptStatementOutcome::Committed);
                } else {
                    statements.push(ScriptStatementOutcome::NoOp);
                }
                materialize_results(
                    step,
                    &result,
                    changed,
                    &output_kinds,
                    &mut runtime_references,
                    &mut results,
                )?;
                step_index += 1;
            }
            _ => return Err(ScriptRunError::InvalidStep),
        }
    }
    if step_index != program.model.steps().len() {
        return Err(ScriptRunError::InvalidStep);
    }
    if cancelled() {
        return Err(ScriptRunError::Cancelled);
    }
    let info = working.document_info()?;
    let report = ScriptDryRunReport {
        statements,
        commit_count: commits,
        results,
        final_state_digest: working.document_state_digest()?,
        final_revision: info.document_revision,
        next_stable_id: working.next_id.next_raw(),
    };
    Ok(ScriptDryRunResult {
        report,
        staged: working,
    })
}

fn is_simple(command: &str) -> bool {
    matches!(
        command,
        "set_layer_properties"
            | "set_plane_properties"
            | "convert_plane"
            | "convert_layer"
            | "mirror_document"
            | "rotate_document"
            | "resize_document"
    )
}

fn is_document_tree(command: &str) -> bool {
    crate::primitive::inkscript_document_tree::DOCUMENT_TREE_COMMANDS
        .iter()
        .any(|schema| schema.name() == command)
}

fn is_metadata_color_guide(command: &str) -> bool {
    crate::primitive::inkscript_metadata::METADATA_COLOR_GUIDE_COMMANDS
        .iter()
        .any(|schema| schema.name() == command)
}

fn is_stroke_geometry_import(command: &str) -> bool {
    crate::primitive::inkscript_stroke_geometry::STROKE_GEOMETRY_COMMANDS
        .iter()
        .any(|schema| schema.name() == command)
}

fn is_fill_gradient(command: &str) -> bool {
    crate::primitive::inkscript_fill_gradient::FILL_GRADIENT_COMMANDS
        .iter()
        .any(|schema| schema.name() == command)
}

fn is_gesture_adjustment(command: &str) -> bool {
    crate::primitive::inkscript_gesture_adjustment::GESTURE_ADJUSTMENT_COMMANDS
        .iter()
        .any(|schema| schema.name() == command)
}

fn is_selection_floating(command: &str) -> bool {
    crate::primitive::inkscript_selection_floating::SELECTION_FLOATING_COMMANDS
        .iter()
        .any(|schema| schema.name() == command)
}

fn is_frame(command: &str) -> bool {
    crate::primitive::inkscript_frame::FRAME_COMMANDS
        .iter()
        .any(|schema| schema.name() == command)
}

fn is_light_table(command: &str) -> bool {
    crate::primitive::inkscript_light_table::LIGHT_TABLE_COMMANDS
        .iter()
        .any(|schema| schema.name() == command)
}

fn initial_runtime_references(
    bindings: &BTreeMap<String, InkScriptBoundValue>,
) -> Result<InkScriptRuntimeReferences, ScriptRunError> {
    let mut references = InkScriptRuntimeReferences::default();
    for (name, value) in bindings {
        match value {
            InkScriptBoundValue::One(reference) => {
                insert_runtime_reference(&mut references, name.clone(), reference)?
            }
            InkScriptBoundValue::All(values) => {
                for (index, reference) in values.iter().enumerate() {
                    insert_runtime_reference(
                        &mut references,
                        format!("{name}[{index}]"),
                        reference,
                    )?;
                }
            }
            InkScriptBoundValue::Skipped => {}
        }
    }
    Ok(references)
}

fn insert_runtime_reference(
    references: &mut InkScriptRuntimeReferences,
    key: String,
    reference: &InkScriptEntityReference,
) -> Result<(), ScriptRunError> {
    let kind =
        InkScriptEntityKind::from_name(&reference.entity).ok_or(ScriptRunError::InvalidInput)?;
    references
        .insert(key, kind, reference.persistent_id)
        .map_err(|_| ScriptRunError::InvalidInput)
}

fn simple_adapter_error(error: LegacySimpleAdapterError) -> ScriptRunError {
    match error {
        LegacySimpleAdapterError::MissingBinding => ScriptRunError::MissingResult,
        _ => ScriptRunError::InvalidStep,
    }
}

fn image_adapter_error(error: LegacyImageAdapterError) -> ScriptRunError {
    match error {
        LegacyImageAdapterError::MissingBinding => ScriptRunError::MissingResult,
        _ => ScriptRunError::InvalidStep,
    }
}

fn document_tree_adapter_error(error: DocumentTreeAdapterError) -> ScriptRunError {
    match error {
        DocumentTreeAdapterError::MissingReference => ScriptRunError::MissingResult,
        _ => ScriptRunError::InvalidStep,
    }
}

fn metadata_color_guide_adapter_error(error: MetadataColorGuideAdapterError) -> ScriptRunError {
    match error {
        MetadataColorGuideAdapterError::MissingReference => ScriptRunError::MissingResult,
        MetadataColorGuideAdapterError::ResourceLimit => ScriptRunError::ResourceLimit,
        _ => ScriptRunError::InvalidStep,
    }
}

fn stroke_geometry_import_adapter_error(error: StrokeGeometryImportAdapterError) -> ScriptRunError {
    match error {
        StrokeGeometryImportAdapterError::MissingReference => ScriptRunError::MissingResult,
        StrokeGeometryImportAdapterError::ResourceLimit => ScriptRunError::ResourceLimit,
        _ => ScriptRunError::InvalidStep,
    }
}

fn fill_gradient_adapter_error(error: FillGradientAdapterError) -> ScriptRunError {
    match error {
        FillGradientAdapterError::MissingReference => ScriptRunError::MissingResult,
        FillGradientAdapterError::ResourceLimit => ScriptRunError::ResourceLimit,
        _ => ScriptRunError::InvalidStep,
    }
}

fn gesture_adjustment_adapter_error(error: GestureAdjustmentAdapterError) -> ScriptRunError {
    match error {
        GestureAdjustmentAdapterError::MissingReference => ScriptRunError::MissingResult,
        GestureAdjustmentAdapterError::ResourceLimit => ScriptRunError::ResourceLimit,
        _ => ScriptRunError::InvalidStep,
    }
}

fn selection_floating_adapter_error(error: SelectionFloatingAdapterError) -> ScriptRunError {
    match error {
        SelectionFloatingAdapterError::MissingReference => ScriptRunError::MissingResult,
        SelectionFloatingAdapterError::ResourceLimit => ScriptRunError::ResourceLimit,
        _ => ScriptRunError::InvalidStep,
    }
}

fn frame_adapter_error(error: FrameAdapterError) -> ScriptRunError {
    match error {
        FrameAdapterError::MissingReference => ScriptRunError::MissingResult,
        FrameAdapterError::ResourceLimit => ScriptRunError::ResourceLimit,
        _ => ScriptRunError::InvalidStep,
    }
}

fn light_table_adapter_error(error: LightTableAdapterError) -> ScriptRunError {
    match error {
        LightTableAdapterError::MissingReference => ScriptRunError::MissingResult,
        LightTableAdapterError::ResourceLimit => ScriptRunError::ResourceLimit,
        LightTableAdapterError::InvalidTypedStep
        | LightTableAdapterError::InvalidValue
        | LightTableAdapterError::UnsupportedPrimitive => ScriptRunError::InvalidStep,
    }
}

fn script_asset_error(error: ScriptAssetError) -> ScriptRunError {
    match error {
        ScriptAssetError::ResourceLimit => ScriptRunError::ResourceLimit,
        _ => ScriptRunError::InvalidStep,
    }
}

fn preflight_resources(core: &Core, program: &StaticScriptProgram) -> Result<(), ScriptRunError> {
    let count = program.budget.max_invocations;
    let output_ids = program.budget.max_output_ids;
    let branch_cut = u64::from(core.history_cursor < core.history.len());
    let journal_events = count
        .checked_add(branch_cut)
        .ok_or(ScriptRunError::ResourceLimit)?;
    let info = core.document_info()?;
    let within = |next: u64, amount: u64| {
        next.checked_add(amount)
            .is_some_and(|following| following <= MAX_PERSISTENT_NUMERIC_ID)
    };
    if !within(core.next_procedure.get(), count)
        || !within(core.next_state.get(), count)
        || !within(core.next_journal_event.get(), journal_events)
        || !within(core.next_branch.get(), branch_cut)
        || !within(core.next_id.next_raw(), output_ids)
        || info.document_revision.checked_add(count).is_none()
        || core.history.len().checked_add(count as usize).is_none()
        || core
            .journal
            .len()
            .checked_add(journal_events as usize)
            .is_none()
        || core.journal.len() as u64 + journal_events > crate::journal::MAX_JOURNAL_EVENTS as u64
        || core.branch_tails.len() as u64 + branch_cut > crate::journal::MAX_JOURNAL_BRANCHES
    {
        return Err(ScriptRunError::ResourceLimit);
    }
    Ok(())
}

pub(super) fn initial_snapshot(core: &Core) -> Result<InkScriptInitialDocumentSnapshot, CoreError> {
    let info = core.document_info()?;
    let mut entities = Vec::new();
    for layer in core.layers()? {
        let layer_ref = InkScriptEntityReference {
            entity: "layer".to_owned(),
            persistent_id: layer.id,
        };
        let mut properties = BTreeMap::new();
        properties.insert(
            "layer_kind".to_owned(),
            InkScriptComparableValue::Enum(layer_kind(layer.kind).to_owned()),
        );
        properties.insert(
            "name".to_owned(),
            InkScriptComparableValue::String(layer.name),
        );
        properties.insert(
            "visible".to_owned(),
            InkScriptComparableValue::Boolean(layer.visible),
        );
        properties.insert(
            "editable".to_owned(),
            InkScriptComparableValue::Boolean(layer.editable),
        );
        properties.insert(
            "opacity_milli".to_owned(),
            InkScriptComparableValue::U64(u64::from(layer.opacity_milli)),
        );
        entities.push(InkScriptEntitySnapshot {
            reference: layer_ref.clone(),
            owner: None,
            properties,
        });
        for plane in layer.planes {
            let mut properties = BTreeMap::new();
            properties.insert(
                "plane_kind".to_owned(),
                InkScriptComparableValue::Enum(plane_kind(plane.kind).to_owned()),
            );
            properties.insert(
                "pixel_format".to_owned(),
                InkScriptComparableValue::Enum(pixel_format(plane.pixel_format).to_owned()),
            );
            properties.insert(
                "name".to_owned(),
                InkScriptComparableValue::String(plane.name),
            );
            properties.insert(
                "visible".to_owned(),
                InkScriptComparableValue::Boolean(plane.visible),
            );
            properties.insert(
                "editable".to_owned(),
                InkScriptComparableValue::Boolean(plane.editable),
            );
            properties.insert(
                "opacity_milli".to_owned(),
                InkScriptComparableValue::U64(u64::from(plane.opacity_milli)),
            );
            entities.push(InkScriptEntitySnapshot {
                reference: InkScriptEntityReference {
                    entity: "plane".to_owned(),
                    persistent_id: plane.id,
                },
                owner: Some(layer_ref.clone()),
                properties,
            });
        }
    }
    for guide in core.guides()? {
        let mut properties = BTreeMap::new();
        properties.insert(
            "axis".to_owned(),
            InkScriptComparableValue::Enum(
                match guide.axis {
                    crate::GuideAxis::Horizontal => "horizontal",
                    crate::GuideAxis::Vertical => "vertical",
                }
                .to_owned(),
            ),
        );
        properties.insert(
            "position".to_owned(),
            InkScriptComparableValue::I64(i64::from(guide.position)),
        );
        entities.push(InkScriptEntitySnapshot {
            reference: InkScriptEntityReference {
                entity: "guide".to_owned(),
                persistent_id: guide.id,
            },
            owner: None,
            properties,
        });
    }
    if let Some(frame) = core.shooting_frame()? {
        entities.push(InkScriptEntitySnapshot {
            reference: InkScriptEntityReference {
                entity: "shooting_frame".to_owned(),
                persistent_id: frame.id,
            },
            owner: None,
            properties: BTreeMap::new(),
        });
    }
    for point in core.vanishing_points()? {
        entities.push(InkScriptEntitySnapshot {
            reference: InkScriptEntityReference {
                entity: "vanishing_point".to_owned(),
                persistent_id: point.id,
            },
            owner: Some(InkScriptEntityReference {
                entity: "layer".to_owned(),
                persistent_id: point.layer_id,
            }),
            properties: BTreeMap::new(),
        });
    }
    for set in core.light_table_sets()? {
        let mut properties = BTreeMap::new();
        properties.insert(
            "name".to_owned(),
            InkScriptComparableValue::String(set.name),
        );
        entities.push(InkScriptEntitySnapshot {
            reference: InkScriptEntityReference {
                entity: "light_table_set".to_owned(),
                persistent_id: set.id,
            },
            owner: None,
            properties,
        });
    }
    for (set_id, item_id) in core.light_table_item_owners()? {
        entities.push(InkScriptEntitySnapshot {
            reference: InkScriptEntityReference {
                entity: "light_table_item".to_owned(),
                persistent_id: item_id,
            },
            owner: Some(InkScriptEntityReference {
                entity: "light_table_set".to_owned(),
                persistent_id: set_id,
            }),
            properties: BTreeMap::new(),
        });
    }
    let selection = core.selection_bounds()?;
    Ok(InkScriptInitialDocumentSnapshot {
        source_document_uuid: uuid_text(info.document_uuid),
        state_digest: hex(core.document_state_digest()?.as_bytes()),
        id_allocations: vec![
            ("document_stable".to_owned(), core.next_id.next_raw()),
            ("procedure".to_owned(), core.next_procedure.get()),
            ("state".to_owned(), core.next_state.get()),
            ("journal_event".to_owned(), core.next_journal_event.get()),
            ("branch".to_owned(), core.next_branch.get()),
        ],
        width: info.width,
        height: info.height,
        dpi_x: dpi_q16(info.dpi_x_milli),
        dpi_y: dpi_q16(info.dpi_y_milli),
        color_space: "srgb".to_owned(),
        entities,
        selection: InkScriptSelectionSnapshot {
            empty: selection.is_none(),
            bounds: selection
                .map(|bounds| {
                    Ok::<(i32, i32, u32, u32), CoreError>((
                        bounds.x,
                        bounds.y,
                        u32::try_from(bounds.width)
                            .map_err(|_| CoreError::InvalidState("selection width is negative"))?,
                        u32::try_from(bounds.height)
                            .map_err(|_| CoreError::InvalidState("selection height is negative"))?,
                    ))
                })
                .transpose()?,
        },
    })
}

fn dpi_q16(value: u32) -> i64 {
    (i64::from(value) * 65_536 + 500) / 1_000
}

fn uuid_text(value: u128) -> String {
    let value = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &value[0..8],
        &value[8..12],
        &value[12..16],
        &value[16..20],
        &value[20..32]
    )
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    text
}

fn layer_kind(value: LayerKind) -> &'static str {
    match value {
        LayerKind::BinaryColoring => "binary_coloring",
        LayerKind::GrayscaleColoring => "grayscale_coloring",
        LayerKind::Raster => "raster",
        LayerKind::Selection => "selection",
        LayerKind::Frame => "frame",
        LayerKind::VanishingPoint => "vanishing_point",
        LayerKind::Adjustment => "adjustment",
    }
}

fn plane_kind(value: PlaneType) -> &'static str {
    match value {
        PlaneType::MainLine => "main_line",
        PlaneType::Color => "color",
        PlaneType::Raster => "raster",
        PlaneType::Selection => "selection",
    }
}

fn pixel_format(value: PixelFormat) -> &'static str {
    match value {
        PixelFormat::BinaryMask8 => "mask8",
        PixelFormat::Grayscale8 => "gray8",
        PixelFormat::Grayscale16 => "gray16",
        PixelFormat::StraightRgba8 => "rgba8",
        PixelFormat::StraightRgba16 => "rgba16",
        PixelFormat::PremultipliedBgra8 => "bgra8_premultiplied",
    }
}

fn materialize_results(
    step: &inkpod_format::InkScriptTypedStep,
    invocation: &InvocationResult,
    changed: bool,
    entity_kinds: &[InkScriptEntityKind],
    references: &mut InkScriptRuntimeReferences,
    output: &mut Vec<ScriptResultValue>,
) -> Result<(), ScriptRunError> {
    let Some(alias) = step.result_alias() else {
        return Ok(());
    };
    let mut consumed = vec![false; invocation.output_ids.len()];
    for result in step.results() {
        if result.availability() == InkScriptResultAvailability::OnlyOnChange && !changed {
            continue;
        }
        let matching = match result.cardinality() {
            InkScriptResultCardinality::Scalar => consumed
                .iter()
                .position(|value| !*value)
                .into_iter()
                .collect::<Vec<_>>(),
            InkScriptResultCardinality::OrderedList => {
                let expected = match result.name() {
                    "layers" => InkScriptEntityKind::Layer,
                    "planes" => InkScriptEntityKind::Plane,
                    "guides" => InkScriptEntityKind::Guide,
                    "shooting_frames" => InkScriptEntityKind::ShootingFrame,
                    "vanishing_points" => InkScriptEntityKind::VanishingPoint,
                    "set" => InkScriptEntityKind::LightTableSet,
                    "item" | "items" => InkScriptEntityKind::LightTableItem,
                    _ => return Err(ScriptRunError::InvalidStep),
                };
                entity_kinds
                    .iter()
                    .enumerate()
                    .filter_map(|(output_index, kind)| {
                        (!consumed[output_index] && *kind == expected).then_some(output_index)
                    })
                    .collect()
            }
        };
        for (element_index, output_index) in matching.into_iter().enumerate() {
            let Some(id) = invocation.output_ids.get(output_index) else {
                return Err(ScriptRunError::InvalidStep);
            };
            let Some(kind) = entity_kinds.get(output_index) else {
                return Err(ScriptRunError::InvalidStep);
            };
            let key = match result.cardinality() {
                InkScriptResultCardinality::Scalar => format!("{alias}.{}", result.name()),
                InkScriptResultCardinality::OrderedList => {
                    format!("{alias}.{}[{element_index}]", result.name())
                }
            };
            references
                .insert(key, *kind, *id)
                .map_err(|_| ScriptRunError::InvalidStep)?;
            output.push(ScriptResultValue {
                alias: alias.to_owned(),
                field: result.name().to_owned(),
                output_id_ordinal: u16::try_from(output_index)
                    .map_err(|_| ScriptRunError::ResourceLimit)?,
                persistent_id: *id,
            });
            consumed[output_index] = true;
        }
    }
    if invocation.output_ids.len() != entity_kinds.len() || consumed.iter().any(|value| !*value) {
        return Err(ScriptRunError::InvalidStep);
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn test_result_materialization_contract() {
    fn project(
        availability: &[InkScriptResultAvailability],
        changed: bool,
        result: &InvocationResult,
    ) -> Vec<(u16, u64)> {
        availability
            .iter()
            .enumerate()
            .filter(|(_, availability)| {
                **availability == InkScriptResultAvailability::AlwaysOnSuccess || changed
            })
            .map(|(index, _)| (index as u16, result.output_ids[index]))
            .collect()
    }
    let result = InvocationResult::outputs(
        crate::DispatchOutcome {
            revision: 2,
            accepted_commands: 1,
        },
        vec![41, 42],
    );
    let availability = [
        InkScriptResultAvailability::AlwaysOnSuccess,
        InkScriptResultAvailability::OnlyOnChange,
    ];
    assert_eq!(
        project(&availability, true, &result),
        vec![(0, 41), (1, 42)]
    );
    assert_eq!(project(&availability, false, &result), vec![(0, 41)]);
}
