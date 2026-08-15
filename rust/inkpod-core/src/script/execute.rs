use super::bind::{
    InkScriptBindingError, InkScriptComparableValue, InkScriptEntityReference,
    InkScriptEntitySnapshot, InkScriptInitialDocumentSnapshot, InkScriptPreparedStatement,
    InkScriptSelectionSnapshot, prepare_inkscript_initial_state_with_parameters,
};
use super::compile::{ScriptCompileError, ScriptSchemas, StaticScriptProgram, catalog};
use super::report::{ScriptDryRunReport, ScriptResultValue, ScriptStatementOutcome};
use crate::primitive::{InvocationResult, LegacyImageScriptStep, LegacySimpleScriptStep};
use crate::{
    Core, CoreError, DocumentStateDigest, LayerKind, MAX_PERSISTENT_NUMERIC_ID, PixelFormat,
    PlaneType,
};
use inkpod_format::{
    InkScriptInputDeclarationKind, InkScriptResultAvailability, InkScriptTypedProgramNode,
    decode_procedure_file,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InMemoryInputFingerprint {
    document_uuid: u128,
    document_revision: u64,
    state_digest: DocumentStateDigest,
    next_stable_id: u64,
    next_procedure_id: u64,
    next_state_id: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CapturedScriptInput<'a> {
    InMemory {
        core: &'a Core,
        fingerprint: InMemoryInputFingerprint,
    },
    NativeBytes(&'a [u8]),
}

#[derive(Debug)]
pub(crate) struct ScriptDryRunResult {
    pub(crate) report: ScriptDryRunReport,
    pub(crate) staged: Core,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScriptRunError {
    Binding(InkScriptBindingError),
    Compile(ScriptCompileError),
    Cancelled,
    StaleInput,
    ResourceLimit,
    InvalidInput,
    InvalidStep,
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

pub(crate) fn capture_in_memory_fingerprint(
    core: &Core,
) -> Result<InMemoryInputFingerprint, CoreError> {
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

pub(crate) fn capture_in_memory_input(core: &Core) -> Result<CapturedScriptInput<'_>, CoreError> {
    Ok(CapturedScriptInput::InMemory {
        core,
        fingerprint: capture_in_memory_fingerprint(core)?,
    })
}

pub(crate) const fn capture_in_memory_input_at(
    core: &Core,
    fingerprint: InMemoryInputFingerprint,
) -> CapturedScriptInput<'_> {
    CapturedScriptInput::InMemory { core, fingerprint }
}

pub(crate) const fn native_script_input(bytes: &[u8]) -> CapturedScriptInput<'_> {
    CapturedScriptInput::NativeBytes(bytes)
}

pub(crate) fn run_inkscript_dry(
    program: &StaticScriptProgram,
    input: CapturedScriptInput<'_>,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<ScriptDryRunResult, ScriptRunError> {
    if cancelled() {
        return Err(ScriptRunError::Cancelled);
    }
    let mut working = match input {
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
    )?;

    let mut statements = Vec::with_capacity(prepared.statements.len());
    let mut results = Vec::new();
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
                let invocation = if is_simple(step.command()) {
                    LegacySimpleScriptStep::from_compiled(
                        step,
                        program.frozen_arguments[index].clone(),
                        &prepared.bindings,
                    )
                    .and_then(|step| step.to_canonical())
                    .map_err(|_| ScriptRunError::InvalidStep)?
                } else {
                    LegacyImageScriptStep::from_compiled(
                        step,
                        program.frozen_arguments[index].clone(),
                        &prepared.bindings,
                    )
                    .and_then(|step| step.to_canonical())
                    .map_err(|_| ScriptRunError::InvalidStep)?
                };
                let before_revision = working.document_info()?.document_revision;
                let result = working.execute_canonical_invocation(invocation)?;
                let changed = result.dispatch.revision() != before_revision;
                if changed {
                    commits = commits
                        .checked_add(1)
                        .ok_or(ScriptRunError::ResourceLimit)?;
                    statements.push(ScriptStatementOutcome::Committed);
                } else {
                    statements.push(ScriptStatementOutcome::NoOp);
                }
                materialize_results(step, &result, changed, &mut results)?;
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

fn initial_snapshot(core: &Core) -> Result<InkScriptInitialDocumentSnapshot, CoreError> {
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
        LayerKind::Text => "text",
        LayerKind::Annotation => "annotation",
        LayerKind::VectorColoring => "vector_coloring",
    }
}

fn plane_kind(value: PlaneType) -> &'static str {
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
    output: &mut Vec<ScriptResultValue>,
) -> Result<(), ScriptRunError> {
    let Some(alias) = step.result_alias() else {
        return Ok(());
    };
    for (index, result) in step.results().iter().enumerate() {
        if result.availability() == InkScriptResultAvailability::OnlyOnChange && !changed {
            continue;
        }
        let Some(id) = invocation.output_ids.get(index) else {
            return Err(ScriptRunError::InvalidStep);
        };
        output.push(ScriptResultValue {
            alias: alias.to_owned(),
            field: result.name().to_owned(),
            output_id_ordinal: u16::try_from(index).map_err(|_| ScriptRunError::ResourceLimit)?,
            persistent_id: *id,
        });
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
