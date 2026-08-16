use super::StaticScriptProgram;
use super::execute::{ScriptRunError, run_inkscript_on_staged_core};
use super::plan::{
    NativeInputFingerprint, PlannedInputSource, ScriptConfirmationToken,
    ScriptConsumedConfirmation, ScriptExecutionPlan, ScriptPlanError, ScriptPlannedInput,
    ScriptRunScope, ScriptSessionSnapshot, ValidatedPathIdentity,
};
use super::report::ScriptDryRunReport;
use crate::{Core, CoreError};
use inkpod_format::{
    InkScriptExecutionFailure, InkScriptOutput, decode_procedure_file, encode_procedure_file,
};

const MAX_RUN_OUTPUT_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptRunMode {
    DryRun,
    Install,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptRunLimits {
    output_bytes: u64,
}

impl ScriptRunLimits {
    pub const fn exact_current() -> Self {
        Self {
            output_bytes: MAX_RUN_OUTPUT_BYTES,
        }
    }

    pub const fn with_output_bytes(mut self, maximum: u64) -> Self {
        self.output_bytes = if maximum == 0 { 1 } else { maximum };
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptRunAdapterError {
    Cancelled,
    Unavailable,
    InvalidData,
    Io,
    UnsupportedAtomicInstall,
    UnsupportedAtomicOverwrite,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptNativeRead {
    bytes: Vec<u8>,
    before: NativeInputFingerprint,
    after: NativeInputFingerprint,
}

impl ScriptNativeRead {
    pub fn new(
        bytes: Vec<u8>,
        before: NativeInputFingerprint,
        after: NativeInputFingerprint,
    ) -> Self {
        Self {
            bytes,
            before,
            after,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptPreparedDestination {
    observed: ValidatedPathIdentity,
    created_directories: Vec<ValidatedPathIdentity>,
}

impl ScriptPreparedDestination {
    pub fn new(
        observed: ValidatedPathIdentity,
        created_directories: Vec<ValidatedPathIdentity>,
    ) -> Self {
        Self {
            observed,
            created_directories,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptAtomicCapabilities {
    pub install: bool,
    pub overwrite: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptTemporaryIdentity {
    volume_id: [u8; 16],
    parent_object_id: [u8; 32],
    parent_generation: u64,
    object_id: [u8; 32],
    object_generation: u64,
}

impl ScriptTemporaryIdentity {
    pub fn new(
        volume_id: [u8; 16],
        parent_object_id: [u8; 32],
        parent_generation: u64,
        object_id: [u8; 32],
        object_generation: u64,
    ) -> Result<Self, ScriptRunAdapterError> {
        if volume_id == [0; 16]
            || parent_object_id == [0; 32]
            || parent_generation == 0
            || object_id == [0; 32]
            || object_generation == 0
        {
            return Err(ScriptRunAdapterError::InvalidData);
        }
        Ok(Self {
            volume_id,
            parent_object_id,
            parent_generation,
            object_id,
            object_generation,
        })
    }

    pub const fn volume_id(self) -> [u8; 16] {
        self.volume_id
    }

    pub const fn parent_object_id(self) -> [u8; 32] {
        self.parent_object_id
    }

    pub const fn parent_generation(self) -> u64 {
        self.parent_generation
    }

    pub const fn object_id(self) -> [u8; 32] {
        self.object_id
    }

    pub const fn object_generation(self) -> u64 {
        self.object_generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptOverwriteGuard {
    id: [u8; 32],
}

impl ScriptOverwriteGuard {
    pub fn new(id: [u8; 32]) -> Result<Self, ScriptRunAdapterError> {
        if id == [0; 32] {
            return Err(ScriptRunAdapterError::InvalidData);
        }
        Ok(Self { id })
    }

    pub const fn id(self) -> [u8; 32] {
        self.id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptAtomicInstallResult {
    Installed,
    InstalledAfterCancellation,
    CancelledBeforeLinearization,
}

/// Runtime-only filesystem/session bridge. Implementations own all OS handles and guards.
/// A successful `atomic_install` is the per-item linearization point; it must never report
/// an error after the destination change has become visible.
#[doc(hidden)]
pub trait ScriptRunAdapter: Send {
    fn authority_generation(&mut self) -> Result<u64, ScriptRunAdapterError>;
    fn open_session_set_generation(&mut self) -> Result<u64, ScriptRunAdapterError>;
    fn session_is_current(
        &mut self,
        session_id: u64,
        session_generation: u64,
        source_generation: u64,
    ) -> Result<bool, ScriptRunAdapterError>;
    fn read_native(
        &mut self,
        expected: &NativeInputFingerprint,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptNativeRead, ScriptRunAdapterError>;
    fn fingerprint_native(
        &mut self,
        expected: &NativeInputFingerprint,
    ) -> Result<NativeInputFingerprint, ScriptRunAdapterError>;
    fn atomic_capabilities(
        &mut self,
        destination: &ValidatedPathIdentity,
    ) -> Result<ScriptAtomicCapabilities, ScriptRunAdapterError>;
    fn prepare_destination(
        &mut self,
        destination: &ValidatedPathIdentity,
        known_job_directories: &[ValidatedPathIdentity],
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptPreparedDestination, ScriptRunAdapterError>;
    fn revalidate_destination(
        &mut self,
        destination: &ValidatedPathIdentity,
    ) -> Result<ValidatedPathIdentity, ScriptRunAdapterError>;
    fn create_same_volume_temporary(
        &mut self,
        destination: &ValidatedPathIdentity,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError>;
    /// On error, the adapter must close and remove only the exact temporary it created.
    fn write_flush_close_temporary(
        &mut self,
        temporary: ScriptTemporaryIdentity,
        bytes: &[u8],
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError>;
    fn revalidate_closed_temporary(
        &mut self,
        temporary: ScriptTemporaryIdentity,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError>;
    fn acquire_overwrite_guard(
        &mut self,
        source: &NativeInputFingerprint,
    ) -> Result<ScriptOverwriteGuard, ScriptRunAdapterError>;
    fn fingerprint_under_guard(
        &mut self,
        guard: ScriptOverwriteGuard,
        source: &NativeInputFingerprint,
    ) -> Result<NativeInputFingerprint, ScriptRunAdapterError>;
    fn release_overwrite_guard(&mut self, guard: ScriptOverwriteGuard);
    fn atomic_install(
        &mut self,
        temporary: ScriptTemporaryIdentity,
        destination: &ValidatedPathIdentity,
        overwrite_guard: Option<ScriptOverwriteGuard>,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptAtomicInstallResult, ScriptRunAdapterError>;
    /// Cleanup must be identity-guarded and leave a different object untouched.
    fn cleanup_closed_temporary(&mut self, temporary: ScriptTemporaryIdentity);

    #[cfg(test)]
    fn observe_staged_execution(&mut self, _report: &ScriptDryRunReport) {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptItemFailure {
    StaleAuthority,
    StaleSession,
    StaleInput,
    StaleDestination,
    UnsupportedAtomicInstall,
    UnsupportedAtomicOverwrite,
    Decode,
    Execute,
    Encode,
    Save,
    ResourceLimit,
    Adapter,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptItemOutcome {
    Installed,
    DryRun,
    Failed(ScriptItemFailure),
    Cancelled,
    NotStarted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptRunItemReport {
    pub ordinal: usize,
    pub input_label: String,
    pub destination_key: String,
    pub outcome: ScriptItemOutcome,
    pub execution: Option<ScriptDryRunReport>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptRunReport {
    pub dry_run: bool,
    pub cancelled: bool,
    pub items: Vec<ScriptRunItemReport>,
    pub created_directories: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptRunAdvance {
    ItemCompleted {
        ordinal: usize,
        completed: usize,
        total: usize,
        outcome: ScriptItemOutcome,
    },
    WaitRequested {
        milliseconds: u32,
    },
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptRunStartError {
    Plan(ScriptPlanError),
    ProgramMismatch,
    NotComplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskPhase {
    Ready,
    Wait(u32),
    Complete,
}

#[derive(Debug)]
#[doc(hidden)]
pub struct ScriptRunTask {
    program: StaticScriptProgram,
    plan: ScriptExecutionPlan,
    confirmation: ScriptConsumedConfirmation,
    mode: ScriptRunMode,
    limits: ScriptRunLimits,
    selected: Vec<bool>,
    next_ordinal: usize,
    completed: usize,
    total: usize,
    phase: TaskPhase,
    known_directories: Vec<ValidatedPathIdentity>,
    report: ScriptRunReport,
}

/// Consumes an immutable plan and one-shot confirmation into a sequential run task.
#[doc(hidden)]
pub fn start_inkscript_run(
    program: &StaticScriptProgram,
    plan: ScriptExecutionPlan,
    confirmation: &mut ScriptConfirmationToken,
    mode: ScriptRunMode,
    limits: ScriptRunLimits,
) -> Result<ScriptRunTask, ScriptRunStartError> {
    if !plan.matches_program(program) {
        return Err(ScriptRunStartError::ProgramMismatch);
    }
    let consumed = confirmation
        .consume_for_run(&plan)
        .map_err(ScriptRunStartError::Plan)?;
    if !consumed.matches(&plan) {
        return Err(ScriptRunStartError::ProgramMismatch);
    }
    let selected = plan
        .items()
        .iter()
        .map(|item| scope_selects(consumed.scope(), item))
        .collect::<Vec<_>>();
    let total = selected.iter().filter(|value| **value).count();
    if total == 0 {
        return Err(ScriptRunStartError::Plan(ScriptPlanError::InvalidScope));
    }
    let items = plan
        .items()
        .iter()
        .zip(plan.destinations())
        .enumerate()
        .map(|(ordinal, (item, destination))| ScriptRunItemReport {
            ordinal,
            input_label: item.display_label().to_owned(),
            destination_key: destination.canonical_key().to_owned(),
            outcome: ScriptItemOutcome::NotStarted,
            execution: None,
        })
        .collect();
    Ok(ScriptRunTask {
        program: program.clone(),
        plan,
        confirmation: consumed,
        mode,
        limits,
        selected,
        next_ordinal: 0,
        completed: 0,
        total,
        phase: TaskPhase::Ready,
        known_directories: Vec::new(),
        report: ScriptRunReport {
            dry_run: mode == ScriptRunMode::DryRun,
            cancelled: false,
            items,
            created_directories: Vec::new(),
        },
    })
}

impl ScriptRunTask {
    pub const fn total_items(&self) -> usize {
        self.total
    }

    pub fn advance(
        &mut self,
        adapter: &mut dyn ScriptRunAdapter,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> ScriptRunAdvance {
        match self.phase {
            TaskPhase::Complete => return ScriptRunAdvance::Complete,
            TaskPhase::Wait(milliseconds) => {
                self.phase = TaskPhase::Ready;
                if cancelled() {
                    self.report.cancelled = true;
                    self.phase = TaskPhase::Complete;
                    return ScriptRunAdvance::Complete;
                }
                return ScriptRunAdvance::WaitRequested { milliseconds };
            }
            TaskPhase::Ready => {}
        }

        while self.next_ordinal < self.selected.len() && !self.selected[self.next_ordinal] {
            self.next_ordinal += 1;
        }
        if self.next_ordinal == self.selected.len() {
            self.phase = TaskPhase::Complete;
            return ScriptRunAdvance::Complete;
        }

        let ordinal = self.next_ordinal;
        self.next_ordinal += 1;
        let item = self.plan.items()[ordinal].clone();
        let destination = self.plan.destinations()[ordinal].clone();
        let result = run_one_item(
            &self.program,
            &self.plan,
            &self.confirmation,
            self.mode,
            self.limits,
            &item,
            &destination,
            &mut self.known_directories,
            adapter,
            cancelled,
        );

        for directory in result.created_directories {
            if !self
                .report
                .created_directories
                .iter()
                .any(|key| key == directory.canonical_key())
            {
                self.report
                    .created_directories
                    .push(directory.canonical_key().to_owned());
            }
        }
        self.report.items[ordinal].outcome = result.outcome.clone();
        self.report.items[ordinal].execution = result.execution;
        self.completed += 1;

        let stop_for_failure = matches!(result.outcome, ScriptItemOutcome::Failed(_))
            && self.program.envelope.execution().failure() == InkScriptExecutionFailure::Stop;
        let stop_for_cancel = matches!(result.outcome, ScriptItemOutcome::Cancelled)
            || result.cancel_after_linearization;
        if stop_for_cancel {
            self.report.cancelled = true;
        }
        if stop_for_failure || stop_for_cancel {
            self.phase = TaskPhase::Complete;
        } else if self.has_remaining_selected() && self.program.envelope.execution().wait_ms() != 0
        {
            self.phase = TaskPhase::Wait(self.program.envelope.execution().wait_ms());
        } else if !self.has_remaining_selected() {
            self.phase = TaskPhase::Complete;
        }

        ScriptRunAdvance::ItemCompleted {
            ordinal,
            completed: self.completed,
            total: self.total,
            outcome: result.outcome,
        }
    }

    pub fn finish(&self) -> Result<ScriptRunReport, ScriptRunStartError> {
        if self.phase != TaskPhase::Complete {
            return Err(ScriptRunStartError::NotComplete);
        }
        Ok(self.report.clone())
    }

    fn has_remaining_selected(&self) -> bool {
        self.selected[self.next_ordinal..]
            .iter()
            .any(|value| *value)
    }
}

struct ItemRunResult {
    outcome: ScriptItemOutcome,
    execution: Option<ScriptDryRunReport>,
    created_directories: Vec<ValidatedPathIdentity>,
    cancel_after_linearization: bool,
}

impl ItemRunResult {
    fn failed(failure: ScriptItemFailure) -> Self {
        Self {
            outcome: ScriptItemOutcome::Failed(failure),
            execution: None,
            created_directories: Vec::new(),
            cancel_after_linearization: false,
        }
    }

    fn cancelled() -> Self {
        Self {
            outcome: ScriptItemOutcome::Cancelled,
            execution: None,
            created_directories: Vec::new(),
            cancel_after_linearization: false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_one_item(
    program: &StaticScriptProgram,
    plan: &ScriptExecutionPlan,
    confirmation: &ScriptConsumedConfirmation,
    mode: ScriptRunMode,
    limits: ScriptRunLimits,
    item: &ScriptPlannedInput,
    destination: &ValidatedPathIdentity,
    known_directories: &mut Vec<ValidatedPathIdentity>,
    adapter: &mut dyn ScriptRunAdapter,
    cancelled: &mut dyn FnMut() -> bool,
) -> ItemRunResult {
    if cancelled() {
        return ItemRunResult::cancelled();
    }
    if let Err(failure) = validate_runtime_state(plan, confirmation, adapter) {
        return ItemRunResult::failed(failure);
    }
    let working = match stage_input(item, adapter, cancelled) {
        Ok(value) => value,
        Err(ItemStageError::Cancelled) => return ItemRunResult::cancelled(),
        Err(ItemStageError::Failed(failure)) => return ItemRunResult::failed(failure),
    };
    let executed =
        match run_inkscript_on_staged_core(program, working, Some(plan.frozen_assets()), cancelled)
        {
            Ok(value) => value,
            Err(ScriptRunError::Cancelled) => return ItemRunResult::cancelled(),
            Err(error) => return ItemRunResult::failed(execution_failure(error)),
        };
    #[cfg(test)]
    adapter.observe_staged_execution(&executed.report);
    if mode == ScriptRunMode::DryRun {
        return ItemRunResult {
            outcome: ScriptItemOutcome::DryRun,
            execution: Some(executed.report),
            created_directories: Vec::new(),
            cancel_after_linearization: false,
        };
    }
    if cancelled() {
        return ItemRunResult::cancelled();
    }

    let editor_digest = match executed.staged.editor_state() {
        Ok(value) => value.digest,
        Err(_) => return ItemRunResult::failed(ScriptItemFailure::Encode),
    };
    let native = match executed
        .staged
        .build_procedure_file(Some(executed.staged.current_state), Some(editor_digest))
    {
        Ok(value) => value,
        Err(_) => return ItemRunResult::failed(ScriptItemFailure::Encode),
    };
    let encoded = match encode_procedure_file(&native) {
        Ok(value) => value,
        Err(_) => return ItemRunResult::failed(ScriptItemFailure::Encode),
    };
    if encoded.len() as u64 > limits.output_bytes {
        return ItemRunResult::failed(ScriptItemFailure::ResourceLimit);
    }

    if let Err(failure) = validate_runtime_state(plan, confirmation, adapter) {
        return ItemRunResult::failed(failure);
    }
    if let Err(failure) = validate_source(item, adapter) {
        return ItemRunResult::failed(failure);
    }
    if cancelled() {
        return ItemRunResult::cancelled();
    }
    let overwrite = matches!(
        program.envelope.output(),
        InkScriptOutput::ExplicitOverwrite
    );
    let capabilities = match adapter.atomic_capabilities(destination) {
        Ok(value) => value,
        Err(error) => return ItemRunResult::failed(adapter_failure(error)),
    };
    if !capabilities.install {
        return ItemRunResult::failed(ScriptItemFailure::UnsupportedAtomicInstall);
    }
    if overwrite && !capabilities.overwrite {
        return ItemRunResult::failed(ScriptItemFailure::UnsupportedAtomicOverwrite);
    }
    let prepared = match adapter.prepare_destination(destination, known_directories, cancelled) {
        Ok(value) => value,
        Err(error) if is_cancel(error) => return ItemRunResult::cancelled(),
        Err(error) => return ItemRunResult::failed(adapter_failure(error)),
    };
    let created = match merge_directories(
        known_directories,
        prepared.created_directories,
        destination.volume_id(),
    ) {
        Ok(value) => value,
        Err(failure) => return ItemRunResult::failed(failure),
    };
    if cancelled() {
        return ItemRunResult {
            outcome: ScriptItemOutcome::Cancelled,
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    let install_destination = prepared.observed;
    if !destination_matches_plan(destination, &install_destination, known_directories) {
        return ItemRunResult {
            outcome: ScriptItemOutcome::Failed(ScriptItemFailure::StaleDestination),
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    if let Err(failure) = validate_runtime_state(plan, confirmation, adapter) {
        return ItemRunResult {
            outcome: ScriptItemOutcome::Failed(failure),
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    let temporary = match adapter.create_same_volume_temporary(&install_destination, cancelled) {
        Ok(value) => value,
        Err(error) if is_cancel(error) => {
            return ItemRunResult {
                outcome: ScriptItemOutcome::Cancelled,
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        }
        Err(error) => {
            return ItemRunResult {
                outcome: ScriptItemOutcome::Failed(adapter_failure(error)),
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        }
    };
    if !temporary_matches_destination(temporary, &install_destination) {
        adapter.cleanup_closed_temporary(temporary);
        return ItemRunResult {
            outcome: ScriptItemOutcome::Failed(ScriptItemFailure::StaleDestination),
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    let closed = match adapter.write_flush_close_temporary(temporary, &encoded, cancelled) {
        Ok(value) => value,
        Err(error) if is_cancel(error) => {
            return ItemRunResult {
                outcome: ScriptItemOutcome::Cancelled,
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        }
        Err(error) => {
            return ItemRunResult {
                outcome: ScriptItemOutcome::Failed(adapter_failure(error)),
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        }
    };
    if closed != temporary {
        return ItemRunResult {
            outcome: ScriptItemOutcome::Failed(ScriptItemFailure::StaleDestination),
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    let revalidated = match adapter.revalidate_closed_temporary(closed) {
        Ok(value) => value,
        Err(error) => {
            return ItemRunResult {
                outcome: ScriptItemOutcome::Failed(adapter_failure(error)),
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        }
    };
    if revalidated != closed {
        return ItemRunResult {
            outcome: ScriptItemOutcome::Failed(ScriptItemFailure::StaleDestination),
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    if cancelled() {
        adapter.cleanup_closed_temporary(closed);
        return ItemRunResult {
            outcome: ScriptItemOutcome::Cancelled,
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    if let Err(failure) = validate_runtime_state(plan, confirmation, adapter) {
        adapter.cleanup_closed_temporary(closed);
        return ItemRunResult {
            outcome: ScriptItemOutcome::Failed(failure),
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    if let Err(failure) = validate_source(item, adapter) {
        adapter.cleanup_closed_temporary(closed);
        return ItemRunResult {
            outcome: ScriptItemOutcome::Failed(failure),
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    let observed_destination = match adapter.revalidate_destination(&install_destination) {
        Ok(value) => value,
        Err(error) => {
            adapter.cleanup_closed_temporary(closed);
            return ItemRunResult {
                outcome: ScriptItemOutcome::Failed(adapter_failure(error)),
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        }
    };
    if !observed_destination.matches_exact(&install_destination) {
        adapter.cleanup_closed_temporary(closed);
        return ItemRunResult {
            outcome: ScriptItemOutcome::Failed(ScriptItemFailure::StaleDestination),
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }

    let mut guard = None;
    if overwrite {
        let PlannedInputSource::File(source) = item.source() else {
            adapter.cleanup_closed_temporary(closed);
            return ItemRunResult {
                outcome: ScriptItemOutcome::Failed(ScriptItemFailure::StaleInput),
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        };
        if !source.supports_atomic_overwrite() {
            adapter.cleanup_closed_temporary(closed);
            return ItemRunResult {
                outcome: ScriptItemOutcome::Failed(ScriptItemFailure::UnsupportedAtomicOverwrite),
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        }
        let acquired = match adapter.acquire_overwrite_guard(source) {
            Ok(value) => value,
            Err(error) => {
                adapter.cleanup_closed_temporary(closed);
                return ItemRunResult {
                    outcome: ScriptItemOutcome::Failed(adapter_failure(error)),
                    execution: None,
                    created_directories: created,
                    cancel_after_linearization: false,
                };
            }
        };
        let guarded = match adapter.fingerprint_under_guard(acquired, source) {
            Ok(value) => value,
            Err(error) => {
                adapter.release_overwrite_guard(acquired);
                adapter.cleanup_closed_temporary(closed);
                return ItemRunResult {
                    outcome: ScriptItemOutcome::Failed(adapter_failure(error)),
                    execution: None,
                    created_directories: created,
                    cancel_after_linearization: false,
                };
            }
        };
        if !source.matches_exact(&guarded) {
            adapter.release_overwrite_guard(acquired);
            adapter.cleanup_closed_temporary(closed);
            return ItemRunResult {
                outcome: ScriptItemOutcome::Failed(ScriptItemFailure::StaleInput),
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        }
        guard = Some(acquired);
    }
    if cancelled() {
        if let Some(value) = guard {
            adapter.release_overwrite_guard(value);
        }
        adapter.cleanup_closed_temporary(closed);
        return ItemRunResult {
            outcome: ScriptItemOutcome::Cancelled,
            execution: None,
            created_directories: created,
            cancel_after_linearization: false,
        };
    }
    let installed = match adapter.atomic_install(closed, &install_destination, guard, cancelled) {
        Ok(value) => value,
        Err(error) => {
            if let Some(value) = guard {
                adapter.release_overwrite_guard(value);
            }
            adapter.cleanup_closed_temporary(closed);
            return ItemRunResult {
                outcome: ScriptItemOutcome::Failed(adapter_failure(error)),
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            };
        }
    };
    match installed {
        ScriptAtomicInstallResult::CancelledBeforeLinearization => {
            if let Some(value) = guard {
                adapter.release_overwrite_guard(value);
            }
            adapter.cleanup_closed_temporary(closed);
            ItemRunResult {
                outcome: ScriptItemOutcome::Cancelled,
                execution: None,
                created_directories: created,
                cancel_after_linearization: false,
            }
        }
        ScriptAtomicInstallResult::Installed
        | ScriptAtomicInstallResult::InstalledAfterCancellation => ItemRunResult {
            outcome: ScriptItemOutcome::Installed,
            execution: Some(executed.report),
            created_directories: created,
            cancel_after_linearization: installed
                == ScriptAtomicInstallResult::InstalledAfterCancellation
                || cancelled(),
        },
    }
}

fn scope_selects(scope: &ScriptRunScope, item: &ScriptPlannedInput) -> bool {
    match scope {
        ScriptRunScope::All => true,
        ScriptRunScope::CurrentDocument(uuid) => item.document_uuid() == *uuid,
        ScriptRunScope::CurrentFile(alias) => {
            item.path().is_some_and(|path| path.alias_key() == *alias)
        }
    }
}

fn validate_runtime_state(
    plan: &ScriptExecutionPlan,
    confirmation: &ScriptConsumedConfirmation,
    adapter: &mut dyn ScriptRunAdapter,
) -> Result<(), ScriptItemFailure> {
    if !confirmation.matches(plan) {
        return Err(ScriptItemFailure::StaleAuthority);
    }
    if adapter.authority_generation().map_err(adapter_failure)? != plan.authority_generation()
        || adapter
            .open_session_set_generation()
            .map_err(adapter_failure)?
            != plan.open_session_set_generation()
    {
        return Err(ScriptItemFailure::StaleAuthority);
    }
    Ok(())
}

enum ItemStageError {
    Cancelled,
    Failed(ScriptItemFailure),
}

fn stage_input(
    item: &ScriptPlannedInput,
    adapter: &mut dyn ScriptRunAdapter,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Core, ItemStageError> {
    match item.source() {
        PlannedInputSource::Session(snapshot) => {
            validate_session(snapshot, adapter).map_err(ItemStageError::Failed)?;
            snapshot
                .clone_staged_core()
                .map_err(|_| ItemStageError::Failed(ScriptItemFailure::StaleInput))
        }
        PlannedInputSource::File(expected) => {
            let read = adapter.read_native(expected, cancelled).map_err(|error| {
                if is_cancel(error) {
                    ItemStageError::Cancelled
                } else {
                    ItemStageError::Failed(adapter_failure(error))
                }
            })?;
            if !expected.matches_exact(&read.before)
                || !expected.matches_exact(&read.after)
                || read.bytes.len() as u64 != expected.logical_length()
                || *blake3::hash(&read.bytes).as_bytes() != expected.content_digest()
            {
                return Err(ItemStageError::Failed(ScriptItemFailure::StaleInput));
            }
            let file = decode_procedure_file(&read.bytes)
                .map_err(|_| ItemStageError::Failed(ScriptItemFailure::Decode))?;
            let core = Core::from_procedure_file(file)
                .map_err(|_| ItemStageError::Failed(ScriptItemFailure::Decode))?;
            if core
                .document_info()
                .map_err(|_| ItemStageError::Failed(ScriptItemFailure::Decode))?
                .document_uuid
                != expected.document_uuid()
            {
                return Err(ItemStageError::Failed(ScriptItemFailure::StaleInput));
            }
            Ok(core)
        }
    }
}

fn validate_source(
    item: &ScriptPlannedInput,
    adapter: &mut dyn ScriptRunAdapter,
) -> Result<(), ScriptItemFailure> {
    match item.source() {
        PlannedInputSource::Session(snapshot) => validate_session(snapshot, adapter),
        PlannedInputSource::File(expected) => {
            let observed = adapter
                .fingerprint_native(expected)
                .map_err(adapter_failure)?;
            if expected.matches_exact(&observed) {
                Ok(())
            } else {
                Err(ScriptItemFailure::StaleInput)
            }
        }
    }
}

fn validate_session(
    snapshot: &ScriptSessionSnapshot,
    adapter: &mut dyn ScriptRunAdapter,
) -> Result<(), ScriptItemFailure> {
    if adapter
        .session_is_current(
            snapshot.session_id(),
            snapshot.session_generation(),
            snapshot.source_generation(),
        )
        .map_err(adapter_failure)?
    {
        Ok(())
    } else {
        Err(ScriptItemFailure::StaleSession)
    }
}

fn merge_directories(
    known: &mut Vec<ValidatedPathIdentity>,
    observed: Vec<ValidatedPathIdentity>,
    volume_id: [u8; 16],
) -> Result<Vec<ValidatedPathIdentity>, ScriptItemFailure> {
    let mut added = Vec::new();
    for directory in observed {
        if directory.is_expected_absent()
            || directory.object_id().is_none()
            || directory.volume_id() != volume_id
        {
            return Err(ScriptItemFailure::StaleDestination);
        }
        if let Some(existing) = known
            .iter()
            .find(|value| value.canonical_key() == directory.canonical_key())
        {
            if !existing.matches_exact(&directory) {
                return Err(ScriptItemFailure::StaleDestination);
            }
        } else {
            known.push(directory.clone());
            added.push(directory);
        }
    }
    known.sort_by(|left, right| left.canonical_key().cmp(right.canonical_key()));
    Ok(added)
}

fn temporary_matches_destination(
    temporary: ScriptTemporaryIdentity,
    destination: &ValidatedPathIdentity,
) -> bool {
    temporary.volume_id == destination.volume_id()
        && temporary.parent_object_id == destination.parent_object_id()
        && temporary.parent_generation == destination.parent_generation()
}

fn destination_matches_plan(
    planned: &ValidatedPathIdentity,
    prepared: &ValidatedPathIdentity,
    known_directories: &[ValidatedPathIdentity],
) -> bool {
    if prepared.matches_exact(planned) {
        return true;
    }
    if !planned.is_expected_absent()
        || !prepared.is_expected_absent()
        || planned.canonical_key() != prepared.canonical_key()
        || planned.volume_id() != prepared.volume_id()
        || planned.alias_key() != prepared.alias_key()
        || planned.object_id().is_some()
        || prepared.object_id().is_some()
    {
        return false;
    }
    let Some(parent_key) = planned
        .canonical_key()
        .rsplit_once('/')
        .map(|(parent, _)| parent)
    else {
        return false;
    };
    known_directories.iter().any(|directory| {
        directory.canonical_key() == parent_key
            && directory.volume_id() == prepared.volume_id()
            && directory.object_id() == Some(prepared.parent_object_id())
            && directory.object_generation() == Some(prepared.parent_generation())
    })
}

fn execution_failure(error: ScriptRunError) -> ScriptItemFailure {
    match error {
        ScriptRunError::Cancelled => ScriptItemFailure::Execute,
        ScriptRunError::ResourceLimit => ScriptItemFailure::ResourceLimit,
        ScriptRunError::Core(CoreError::Format(_)) => ScriptItemFailure::Decode,
        _ => ScriptItemFailure::Execute,
    }
}

fn is_cancel(error: ScriptRunAdapterError) -> bool {
    error == ScriptRunAdapterError::Cancelled
}

fn adapter_failure(error: ScriptRunAdapterError) -> ScriptItemFailure {
    match error {
        ScriptRunAdapterError::Cancelled => ScriptItemFailure::Adapter,
        ScriptRunAdapterError::UnsupportedAtomicInstall => {
            ScriptItemFailure::UnsupportedAtomicInstall
        }
        ScriptRunAdapterError::UnsupportedAtomicOverwrite => {
            ScriptItemFailure::UnsupportedAtomicOverwrite
        }
        ScriptRunAdapterError::Io => ScriptItemFailure::Save,
        ScriptRunAdapterError::Unavailable | ScriptRunAdapterError::InvalidData => {
            ScriptItemFailure::Adapter
        }
    }
}

#[cfg(test)]
pub(super) fn test_sequential_multi_item_native_run_contracts() {
    tests::sequential_multi_item_native_run_contracts();
}

#[cfg(test)]
pub(super) fn test_failure_cancel_and_install_race_contracts() {
    tests::failure_cancel_and_install_race_contracts();
}

#[cfg(test)]
pub(super) fn test_authority_overwrite_and_temporary_identity_contracts() {
    tests::authority_overwrite_and_temporary_identity_contracts();
}

#[cfg(test)]
pub(super) fn test_dirty_pathless_dry_run_and_saved_snapshot_contracts() {
    tests::dirty_pathless_dry_run_and_saved_snapshot_contracts();
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
