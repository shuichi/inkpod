use super::*;
use crate::script::compile::compile_inkscript;
use crate::script::plan::{
    AuthorityGrant, AuthoritySnapshot, FolderScan, NativeInputFingerprint, OpenSessionRecord,
    OpenSessionSetSnapshot, ScriptCommandContext, ScriptDestinationRequest, ScriptPlanAdapter,
    ScriptPlanAdapterError, ScriptPlanLimits, ScriptSequenceExpectation, ScriptSequenceSnapshot,
    ScriptSessionExpectation, ScriptSessionSnapshot, ValidatedPathIdentity,
    issue_confirmation_token, plan_inkscript,
};
use crate::{
    AssetAlphaSemantics, AssetColorSpace, Core, DEFAULT_DPI_MILLI, NativeOpenStrategy, PixelFormat,
    RasterAssetInput,
};
use inkpod_format::{
    InkScriptPathIntentAccess, InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId,
    decode_procedure_file, encode_procedure_file,
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
struct InputFixture {
    fingerprint: NativeInputFingerprint,
    bytes: Vec<u8>,
    core: Core,
}

struct PlannedFixture {
    program: StaticScriptProgram,
    plan: ScriptExecutionPlan,
    confirmation: ScriptConfirmationToken,
    inputs: Vec<InputFixture>,
}

#[derive(Default)]
struct TestPlanAdapter {
    files: Vec<NativeInputFingerprint>,
    current: Option<ScriptSessionSnapshot>,
    destinations: VecDeque<ValidatedPathIdentity>,
}

impl ScriptPlanAdapter for TestPlanAdapter {
    fn authority_generation(&mut self) -> Result<u64, ScriptPlanAdapterError> {
        Ok(9)
    }

    fn open_session_set(&mut self) -> Result<OpenSessionSetSnapshot, ScriptPlanAdapterError> {
        OpenSessionSetSnapshot::new(4, Vec::new()).map_err(|_| ScriptPlanAdapterError::InvalidData)
    }

    fn resolve_file(
        &mut self,
        _intent_id: u64,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<NativeInputFingerprint, ScriptPlanAdapterError> {
        self.files
            .first()
            .cloned()
            .ok_or(ScriptPlanAdapterError::Unavailable)
    }

    fn enumerate_folder(
        &mut self,
        _intent_id: u64,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<FolderScan, ScriptPlanAdapterError> {
        FolderScan::new(
            self.files.len() as u64 + 3,
            128,
            self.files.len() as u64 + 4,
            1,
            self.files.clone(),
        )
        .map_err(|_| ScriptPlanAdapterError::InvalidData)
    }

    fn capture_current_document(
        &mut self,
        _expected: &ScriptSessionExpectation,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
        self.current
            .clone()
            .ok_or(ScriptPlanAdapterError::Unavailable)
    }

    fn capture_current_sequence(
        &mut self,
        _expected: &ScriptSequenceExpectation,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSequenceSnapshot, ScriptPlanAdapterError> {
        Err(ScriptPlanAdapterError::Unavailable)
    }

    fn capture_open_session(
        &mut self,
        _session: &OpenSessionRecord,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
        Err(ScriptPlanAdapterError::Unavailable)
    }

    fn resolve_destination(
        &mut self,
        _request: &ScriptDestinationRequest,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ValidatedPathIdentity, ScriptPlanAdapterError> {
        self.destinations
            .pop_front()
            .ok_or(ScriptPlanAdapterError::Unavailable)
    }
}

struct TestRunAdapter {
    authority_generation: u64,
    open_generation: u64,
    authority_calls: usize,
    stale_authority_after: Option<usize>,
    sessions_current: bool,
    files: BTreeMap<String, (NativeInputFingerprint, Vec<u8>)>,
    guarded_fingerprint: Option<NativeInputFingerprint>,
    install_capable: bool,
    overwrite_capable: bool,
    shared_directory: ValidatedPathIdentity,
    directory_prepared: bool,
    known_directory_counts: Vec<usize>,
    set_cancel_after_prepare: Option<Arc<AtomicBool>>,
    temporary_calls: usize,
    write_calls: usize,
    fail_write_call: Option<usize>,
    swap_temporary_after_close: bool,
    cleanup_calls: usize,
    guard_release_calls: usize,
    temporaries: BTreeMap<[u8; 32], Vec<u8>>,
    outputs: BTreeMap<String, Vec<u8>>,
    install_results: VecDeque<ScriptAtomicInstallResult>,
}

impl TestRunAdapter {
    fn from_fixture(fixture: &PlannedFixture) -> Self {
        let files = fixture
            .inputs
            .iter()
            .map(|input| {
                (
                    input.fingerprint.path().canonical_key().to_owned(),
                    (input.fingerprint.clone(), input.bytes.clone()),
                )
            })
            .collect();
        Self {
            authority_generation: 9,
            open_generation: 4,
            authority_calls: 0,
            stale_authority_after: None,
            sessions_current: true,
            files,
            guarded_fingerprint: None,
            install_capable: true,
            overwrite_capable: true,
            shared_directory: existing("root:/out/shared", 90, 60),
            directory_prepared: false,
            known_directory_counts: Vec::new(),
            set_cancel_after_prepare: None,
            temporary_calls: 0,
            write_calls: 0,
            fail_write_call: None,
            swap_temporary_after_close: false,
            cleanup_calls: 0,
            guard_release_calls: 0,
            temporaries: BTreeMap::new(),
            outputs: BTreeMap::new(),
            install_results: VecDeque::new(),
        }
    }
}

impl ScriptRunAdapter for TestRunAdapter {
    fn authority_generation(&mut self) -> Result<u64, ScriptRunAdapterError> {
        self.authority_calls += 1;
        if self
            .stale_authority_after
            .is_some_and(|limit| self.authority_calls > limit)
        {
            Ok(self.authority_generation + 1)
        } else {
            Ok(self.authority_generation)
        }
    }

    fn open_session_set_generation(&mut self) -> Result<u64, ScriptRunAdapterError> {
        Ok(self.open_generation)
    }

    fn session_is_current(
        &mut self,
        _session_id: u64,
        _session_generation: u64,
        _source_generation: u64,
    ) -> Result<bool, ScriptRunAdapterError> {
        Ok(self.sessions_current)
    }

    fn read_native(
        &mut self,
        expected: &NativeInputFingerprint,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptNativeRead, ScriptRunAdapterError> {
        if cancelled() {
            return Err(ScriptRunAdapterError::Cancelled);
        }
        let (observed, bytes) = self
            .files
            .get(expected.path().canonical_key())
            .ok_or(ScriptRunAdapterError::Unavailable)?;
        Ok(ScriptNativeRead::new(
            bytes.clone(),
            observed.clone(),
            observed.clone(),
        ))
    }

    fn fingerprint_native(
        &mut self,
        expected: &NativeInputFingerprint,
    ) -> Result<NativeInputFingerprint, ScriptRunAdapterError> {
        self.files
            .get(expected.path().canonical_key())
            .map(|value| value.0.clone())
            .ok_or(ScriptRunAdapterError::Unavailable)
    }

    fn atomic_capabilities(
        &mut self,
        _destination: &ValidatedPathIdentity,
    ) -> Result<ScriptAtomicCapabilities, ScriptRunAdapterError> {
        Ok(ScriptAtomicCapabilities {
            install: self.install_capable,
            overwrite: self.overwrite_capable,
        })
    }

    fn prepare_destination(
        &mut self,
        destination: &ValidatedPathIdentity,
        known_job_directories: &[ValidatedPathIdentity],
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptPreparedDestination, ScriptRunAdapterError> {
        self.known_directory_counts
            .push(known_job_directories.len());
        let uses_shared_directory = destination.canonical_key().starts_with("root:/out/shared/");
        let created = if !uses_shared_directory || self.directory_prepared {
            Vec::new()
        } else {
            self.directory_prepared = true;
            vec![self.shared_directory.clone()]
        };
        if let Some(flag) = &self.set_cancel_after_prepare {
            flag.store(true, Ordering::SeqCst);
        }
        let observed = if uses_shared_directory {
            ValidatedPathIdentity::expected_absent(
                destination.canonical_key().to_owned(),
                destination.volume_id(),
                self.shared_directory.object_id().unwrap(),
                destination.alias_key(),
                self.shared_directory.alias_key(),
            )
            .map_err(|_| ScriptRunAdapterError::InvalidData)?
        } else {
            destination.clone()
        };
        Ok(ScriptPreparedDestination::new(observed, created))
    }

    fn revalidate_destination(
        &mut self,
        destination: &ValidatedPathIdentity,
    ) -> Result<ValidatedPathIdentity, ScriptRunAdapterError> {
        Ok(destination.clone())
    }

    fn create_same_volume_temporary(
        &mut self,
        destination: &ValidatedPathIdentity,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        if cancelled() {
            return Err(ScriptRunAdapterError::Cancelled);
        }
        self.temporary_calls += 1;
        let id = [u8::try_from(self.temporary_calls).unwrap(); 32];
        let temporary = ScriptTemporaryIdentity::new(
            destination.volume_id(),
            destination.parent_object_id(),
            destination.parent_generation(),
            id,
            1,
        )?;
        self.temporaries.insert(id, Vec::new());
        Ok(temporary)
    }

    fn write_flush_close_temporary(
        &mut self,
        temporary: ScriptTemporaryIdentity,
        bytes: &[u8],
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        self.write_calls += 1;
        if cancelled() || self.fail_write_call == Some(self.write_calls) {
            self.temporaries.remove(&temporary.object_id);
            return Err(if cancelled() {
                ScriptRunAdapterError::Cancelled
            } else {
                ScriptRunAdapterError::Io
            });
        }
        *self
            .temporaries
            .get_mut(&temporary.object_id)
            .ok_or(ScriptRunAdapterError::InvalidData)? = bytes.to_vec();
        Ok(temporary)
    }

    fn revalidate_closed_temporary(
        &mut self,
        temporary: ScriptTemporaryIdentity,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        if self.swap_temporary_after_close {
            ScriptTemporaryIdentity::new(
                temporary.volume_id,
                temporary.parent_object_id,
                temporary.parent_generation,
                [211; 32],
                temporary.object_generation + 1,
            )
        } else {
            Ok(temporary)
        }
    }

    fn acquire_overwrite_guard(
        &mut self,
        _source: &NativeInputFingerprint,
    ) -> Result<ScriptOverwriteGuard, ScriptRunAdapterError> {
        ScriptOverwriteGuard::new([7; 32])
    }

    fn fingerprint_under_guard(
        &mut self,
        _guard: ScriptOverwriteGuard,
        source: &NativeInputFingerprint,
    ) -> Result<NativeInputFingerprint, ScriptRunAdapterError> {
        Ok(self
            .guarded_fingerprint
            .clone()
            .unwrap_or_else(|| source.clone()))
    }

    fn release_overwrite_guard(&mut self, _guard: ScriptOverwriteGuard) {
        self.guard_release_calls += 1;
    }

    fn atomic_install(
        &mut self,
        temporary: ScriptTemporaryIdentity,
        destination: &ValidatedPathIdentity,
        _overwrite_guard: Option<ScriptOverwriteGuard>,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptAtomicInstallResult, ScriptRunAdapterError> {
        let result = self
            .install_results
            .pop_front()
            .unwrap_or(ScriptAtomicInstallResult::Installed);
        if matches!(
            result,
            ScriptAtomicInstallResult::Installed
                | ScriptAtomicInstallResult::InstalledAfterCancellation
        ) {
            let bytes = self
                .temporaries
                .remove(&temporary.object_id)
                .ok_or(ScriptRunAdapterError::InvalidData)?;
            self.outputs
                .insert(destination.canonical_key().to_owned(), bytes);
        }
        Ok(result)
    }

    fn cleanup_closed_temporary(&mut self, temporary: ScriptTemporaryIdentity) {
        self.cleanup_calls += 1;
        self.temporaries.remove(&temporary.object_id);
    }
}

pub(super) fn sequential_multi_item_native_run_contracts() {
    let fixture = folder_fixture(3, "continue", 17);
    assert_eq!(
        fixture
            .plan
            .items()
            .iter()
            .map(|item| item.display_label())
            .collect::<Vec<_>>(),
        vec!["cell2.inkpod", "cell3.inkpod", "cell10.inkpod"]
    );
    let original_inputs = fixture
        .inputs
        .iter()
        .map(|input| {
            (
                input.fingerprint.path().canonical_key().to_owned(),
                input.bytes.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut adapter = TestRunAdapter::from_fixture(&fixture);
    let (report, waits) = drive(
        fixture,
        &mut adapter,
        ScriptRunLimits::exact_current(),
        || false,
    );

    assert_eq!(waits, vec![17, 17]);
    assert_eq!(
        report
            .items
            .iter()
            .map(|item| item.outcome.clone())
            .collect::<Vec<_>>(),
        vec![
            ScriptItemOutcome::Installed,
            ScriptItemOutcome::Installed,
            ScriptItemOutcome::Installed,
        ]
    );
    assert_eq!(adapter.outputs.len(), 3);
    assert_eq!(report.created_directories, vec!["root:/out/shared"]);
    assert_eq!(adapter.known_directory_counts, vec![0, 1, 1]);
    for item in &report.items {
        let output = &adapter.outputs[&item.destination_key];
        let input_key = format!("root:/in/{}", item.input_label);
        assert_saved_output(
            &original_inputs[&input_key],
            output,
            item.execution.as_ref().unwrap(),
        );
        assert_eq!(&adapter.files[&input_key].1, &original_inputs[&input_key]);
    }
}

pub(super) fn failure_cancel_and_install_race_contracts() {
    let stop_fixture = folder_fixture(3, "stop", 0);
    let mut stop_adapter = TestRunAdapter::from_fixture(&stop_fixture);
    stop_adapter.fail_write_call = Some(2);
    let (stop, _) = drive(
        stop_fixture,
        &mut stop_adapter,
        ScriptRunLimits::exact_current(),
        || false,
    );
    assert_eq!(
        outcomes(&stop),
        vec![
            ScriptItemOutcome::Installed,
            ScriptItemOutcome::Failed(ScriptItemFailure::Save),
            ScriptItemOutcome::NotStarted,
        ]
    );
    assert_eq!(stop_adapter.outputs.len(), 1);

    let continue_fixture = folder_fixture(3, "continue", 0);
    let mut continue_adapter = TestRunAdapter::from_fixture(&continue_fixture);
    continue_adapter.fail_write_call = Some(2);
    let (continued, _) = drive(
        continue_fixture,
        &mut continue_adapter,
        ScriptRunLimits::exact_current(),
        || false,
    );
    assert_eq!(
        outcomes(&continued),
        vec![
            ScriptItemOutcome::Installed,
            ScriptItemOutcome::Failed(ScriptItemFailure::Save),
            ScriptItemOutcome::Installed,
        ]
    );
    assert_eq!(continue_adapter.outputs.len(), 2);

    let limited_fixture = folder_fixture(1, "stop", 0);
    let mut limited_adapter = TestRunAdapter::from_fixture(&limited_fixture);
    let (limited, _) = drive(
        limited_fixture,
        &mut limited_adapter,
        ScriptRunLimits::exact_current().with_output_bytes(1),
        || false,
    );
    assert_eq!(
        outcomes(&limited),
        vec![ScriptItemOutcome::Failed(ScriptItemFailure::ResourceLimit)]
    );
    assert_eq!(limited_adapter.temporary_calls, 0);

    let before_fixture = folder_fixture(2, "continue", 0);
    let mut before_adapter = TestRunAdapter::from_fixture(&before_fixture);
    before_adapter
        .install_results
        .push_back(ScriptAtomicInstallResult::CancelledBeforeLinearization);
    let (before, _) = drive(
        before_fixture,
        &mut before_adapter,
        ScriptRunLimits::exact_current(),
        || false,
    );
    assert_eq!(
        outcomes(&before),
        vec![ScriptItemOutcome::Cancelled, ScriptItemOutcome::NotStarted]
    );
    assert!(before_adapter.outputs.is_empty());

    let after_fixture = folder_fixture(2, "continue", 0);
    let mut after_adapter = TestRunAdapter::from_fixture(&after_fixture);
    after_adapter
        .install_results
        .push_back(ScriptAtomicInstallResult::InstalledAfterCancellation);
    let (after, _) = drive(
        after_fixture,
        &mut after_adapter,
        ScriptRunLimits::exact_current(),
        || false,
    );
    assert_eq!(
        outcomes(&after),
        vec![ScriptItemOutcome::Installed, ScriptItemOutcome::NotStarted]
    );
    assert!(after.cancelled);
    assert_eq!(after_adapter.outputs.len(), 1);
}

pub(super) fn authority_overwrite_and_temporary_identity_contracts() {
    let unsupported_fixture = overwrite_fixture();
    let mut unsupported = TestRunAdapter::from_fixture(&unsupported_fixture);
    unsupported.overwrite_capable = false;
    let (report, _) = drive(
        unsupported_fixture,
        &mut unsupported,
        ScriptRunLimits::exact_current(),
        || false,
    );
    assert_eq!(
        outcomes(&report),
        vec![ScriptItemOutcome::Failed(
            ScriptItemFailure::UnsupportedAtomicOverwrite
        )]
    );
    assert_eq!(unsupported.temporary_calls, 0);

    let guarded_fixture = overwrite_fixture();
    let mut guarded = TestRunAdapter::from_fixture(&guarded_fixture);
    let input = &guarded_fixture.inputs[0];
    guarded.guarded_fingerprint = Some(
        NativeInputFingerprint::new(
            input.fingerprint.path().clone(),
            "cell1.inkpod".to_owned(),
            1,
            input.core.document_info().unwrap().document_uuid,
            input.bytes.len() as u64,
            digest(b"same identity changed content"),
            Some(digest(b"changed token")),
            true,
        )
        .unwrap(),
    );
    let (report, _) = drive(
        guarded_fixture,
        &mut guarded,
        ScriptRunLimits::exact_current(),
        || false,
    );
    assert_eq!(
        outcomes(&report),
        vec![ScriptItemOutcome::Failed(ScriptItemFailure::StaleInput)]
    );
    assert!(guarded.outputs.is_empty());
    assert_eq!(guarded.cleanup_calls, 1);
    assert_eq!(guarded.guard_release_calls, 1);

    let swapped_fixture = overwrite_fixture();
    let mut swapped = TestRunAdapter::from_fixture(&swapped_fixture);
    swapped.swap_temporary_after_close = true;
    let (report, _) = drive(
        swapped_fixture,
        &mut swapped,
        ScriptRunLimits::exact_current(),
        || false,
    );
    assert_eq!(
        outcomes(&report),
        vec![ScriptItemOutcome::Failed(
            ScriptItemFailure::StaleDestination
        )]
    );
    assert_eq!(swapped.cleanup_calls, 0);
    assert!(swapped.outputs.is_empty());

    let stale_fixture = folder_fixture(1, "stop", 0);
    let mut stale = TestRunAdapter::from_fixture(&stale_fixture);
    stale.stale_authority_after = Some(2);
    let (report, _) = drive(
        stale_fixture,
        &mut stale,
        ScriptRunLimits::exact_current(),
        || false,
    );
    assert_eq!(
        outcomes(&report),
        vec![ScriptItemOutcome::Failed(ScriptItemFailure::StaleAuthority)]
    );
    assert_eq!(stale.temporary_calls, 0);

    let cancelled_fixture = folder_fixture(1, "stop", 0);
    let mut cancelled_adapter = TestRunAdapter::from_fixture(&cancelled_fixture);
    let flag = Arc::new(AtomicBool::new(false));
    cancelled_adapter.set_cancel_after_prepare = Some(flag.clone());
    let (report, _) = drive(
        cancelled_fixture,
        &mut cancelled_adapter,
        ScriptRunLimits::exact_current(),
        || flag.load(Ordering::SeqCst),
    );
    assert_eq!(outcomes(&report), vec![ScriptItemOutcome::Cancelled]);
    assert_eq!(cancelled_adapter.temporary_calls, 0);
}

pub(super) fn dirty_pathless_dry_run_and_saved_snapshot_contracts() {
    let fixture = current_document_fixture();
    let source_before = fixture.inputs[0].core.clone();
    assert!(source_before.document_info().unwrap().dirty);
    assert!(source_before.current_path.is_none());

    let mut dry_confirmation =
        issue_confirmation_token(&fixture.plan, ScriptRunScope::All).unwrap();
    let mut dry_adapter = TestRunAdapter::from_fixture(&fixture);
    let mut dry_task = start_inkscript_run(
        &fixture.program,
        fixture.plan.clone(),
        &mut dry_confirmation,
        ScriptRunMode::DryRun,
        ScriptRunLimits::exact_current(),
    )
    .unwrap();
    let dry = drive_task(&mut dry_task, &mut dry_adapter, || false).0;
    assert_eq!(outcomes(&dry), vec![ScriptItemOutcome::DryRun]);
    assert_eq!(dry_adapter.temporary_calls, 0);
    assert!(!dry_adapter.directory_prepared);
    assert!(dry_adapter.outputs.is_empty());

    let mut install_adapter = TestRunAdapter::from_fixture(&fixture);
    let (installed, _) = drive(
        fixture,
        &mut install_adapter,
        ScriptRunLimits::exact_current(),
        || false,
    );
    assert_eq!(outcomes(&installed), vec![ScriptItemOutcome::Installed]);
    let output = &install_adapter.outputs[&installed.items[0].destination_key];
    let reopened = Core::from_procedure_file(decode_procedure_file(output).unwrap()).unwrap();
    assert_eq!(
        reopened.document_info().unwrap().document_uuid,
        source_before.document_info().unwrap().document_uuid
    );
    assert_eq!(
        reopened.history_entries().len(),
        source_before.history_entries().len() + 1
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
    assert!(source_before.document_info().unwrap().dirty);
    assert!(source_before.current_path.is_none());
    let mut undo = reopened.clone();
    undo.undo().unwrap();
    assert_eq!(
        undo.document_state_digest().unwrap(),
        source_before.document_state_digest().unwrap()
    );
    undo.redo().unwrap();
    assert_eq!(
        undo.document_state_digest().unwrap(),
        reopened.document_state_digest().unwrap()
    );
    assert_eq!(reopened.next_id, source_before.next_id);
    assert_eq!(
        reopened.next_procedure.get(),
        source_before.next_procedure.get() + 1
    );
    assert_eq!(
        reopened.next_state.get(),
        source_before.next_state.get() + 1
    );
    assert_eq!(reopened.next_branch, source_before.next_branch);
    assert_eq!(
        reopened.next_journal_event.get(),
        source_before.next_journal_event.get() + 1
    );
    assert!(source_before.persistence_info().unwrap().asset_count > 0);
    assert_eq!(
        reopened.persistence_info().unwrap().asset_count,
        source_before.persistence_info().unwrap().asset_count
    );
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
}

fn folder_fixture(count: usize, failure: &str, wait_ms: u32) -> PlannedFixture {
    let inputs = (0..count)
        .map(|index| {
            let number = [10_u32, 2, 3][index];
            input_fixture(number, u8::try_from(index + 1).unwrap())
        })
        .collect::<Vec<_>>();
    let program = compile_program(&format!(
        r#"inputs {{ folder "in"; }}
output {{ policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "scripted"; start_number = 1; direction = ascending; }}
execution {{ failure = {failure}; wait_ms = {wait_ms}; preview_before_save = false; }}"#
    ));
    plan_fixture(program, inputs, None, false)
}

fn overwrite_fixture() -> PlannedFixture {
    let inputs = vec![input_fixture(1, 1)];
    let program = compile_program(
        r#"inputs { file "cell1.inkpod"; }
output { policy = explicit_overwrite; format = inkpod; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }"#,
    );
    plan_fixture(program, inputs, None, true)
}

fn current_document_fixture() -> PlannedFixture {
    let mut core = Core::new();
    core.new_cell_from_raster_asset(
        RasterAssetInput {
            width: 4,
            height: 4,
            pixel_format: PixelFormat::StraightRgba8,
            color_space: Some(AssetColorSpace::Srgb),
            alpha_semantics: AssetAlphaSemantics::Straight,
            canonical_stride: 16,
            pixels: vec![37; 64],
            expected_id: None,
        },
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        0x9001,
    )
    .unwrap();
    let info = core.document_info().unwrap();
    core.set_layer_properties(info.layer_id, true, true, 950, "Abandoned branch")
        .unwrap();
    core.undo().unwrap();
    core.set_layer_properties(info.layer_id, true, true, 900, "Dirty source")
        .unwrap();
    let snapshot =
        ScriptSessionSnapshot::capture(71, 3, 5, "current.inkpod".to_owned(), 1, None, &core)
            .unwrap();
    let program = compile_program(
        r#"inputs { current_document; }
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "scripted"; start_number = 1; direction = ascending; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }"#,
    );
    let input = InputFixture {
        fingerprint: input_fixture(1, 1).fingerprint,
        bytes: Vec::new(),
        core,
    };
    plan_fixture(program, vec![input], Some(snapshot), false)
}

fn plan_fixture(
    program: StaticScriptProgram,
    inputs: Vec<InputFixture>,
    current: Option<ScriptSessionSnapshot>,
    overwrite: bool,
) -> PlannedFixture {
    let command_context = if let Some(snapshot) = current.as_ref() {
        ScriptCommandContext::new(
            Some(ScriptSessionExpectation::from_snapshot(snapshot).unwrap()),
            None,
        )
    } else {
        ScriptCommandContext::default()
    };
    let grants = program
        .path_intents
        .iter()
        .map(|intent| {
            let resolved = match intent.access() {
                InkScriptPathIntentAccess::Read | InkScriptPathIntentAccess::Replace => {
                    inputs[0].fingerprint.path().clone()
                }
                InkScriptPathIntentAccess::Enumerate => existing("root:/in", 40, 70),
                InkScriptPathIntentAccess::Create => existing("root:/out", 60, 70),
            };
            AuthorityGrant::new(
                intent.id(),
                intent.access(),
                [intent.id() as u8; 32],
                9,
                resolved,
            )
            .unwrap()
        })
        .collect();
    let authority = AuthoritySnapshot::new(
        program.static_compile_digest,
        program.path_intent_digest,
        9,
        grants,
        command_context,
        4,
        None,
    )
    .unwrap();
    let destinations = if overwrite {
        vec![inputs[0].fingerprint.path().clone()]
    } else {
        (0..inputs.len())
            .map(|index| {
                absent(
                    &format!("root:/out/shared/scripted_{:04}.inkpod", index + 1),
                    60,
                )
            })
            .collect()
    };
    let mut adapter = TestPlanAdapter {
        files: inputs
            .iter()
            .map(|input| input.fingerprint.clone())
            .collect(),
        current,
        destinations: destinations.into(),
    };
    let plan = plan_inkscript(
        &program,
        &authority,
        &mut adapter,
        &mut [],
        ScriptPlanLimits::exact_current(),
        &mut || false,
    )
    .unwrap();
    let confirmation = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
    PlannedFixture {
        program,
        plan,
        confirmation,
        inputs,
    }
}

fn compile_program(orchestration: &str) -> StaticScriptProgram {
    let text = format!(
        r#"inkscript 2;
requires {{ procedure_catalog = 4; replay_epoch = 25; }}
{orchestration}
bindings {{ let paint = select plane {{ plane_kind = color; cardinality = one; missing = error; }}; }}
program {{
    step "Rename" {{
        enabled = true;
        invoke set_plane_properties {{
            plane_id = $paint;
            visible = true;
            editable = true;
            opacity_milli = 1000;
            name = "Scripted";
        }};
    }}
}}
"#
    );
    let source = InkScriptSource::new(InkScriptSourceId::new(901), text.as_bytes()).unwrap();
    compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap()
}

fn input_fixture(number: u32, object: u8) -> InputFixture {
    let core = new_core_with_uuid(0x1000 + u128::from(object));
    let bytes = native_bytes(&core);
    let label = format!("cell{number}.inkpod");
    let path = existing(&format!("root:/in/{label}"), object, 40);
    let fingerprint = NativeInputFingerprint::new(
        path,
        label,
        number,
        core.document_info().unwrap().document_uuid,
        bytes.len() as u64,
        digest(&bytes),
        Some(digest(format!("change:{number}").as_bytes())),
        true,
    )
    .unwrap();
    InputFixture {
        fingerprint,
        bytes,
        core,
    }
}

fn new_core_with_uuid(uuid: u128) -> Core {
    let mut core = Core::new();
    core.new_cell_with_uuid(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
        .unwrap();
    core
}

fn native_bytes(core: &Core) -> Vec<u8> {
    let editor = core.editor_state().unwrap().digest;
    let file = core
        .build_procedure_file(Some(core.current_state), Some(editor))
        .unwrap();
    encode_procedure_file(&file).unwrap()
}

fn drive(
    mut fixture: PlannedFixture,
    adapter: &mut TestRunAdapter,
    limits: ScriptRunLimits,
    cancelled: impl FnMut() -> bool,
) -> (ScriptRunReport, Vec<u32>) {
    let mut task = start_inkscript_run(
        &fixture.program,
        fixture.plan,
        &mut fixture.confirmation,
        ScriptRunMode::Install,
        limits,
    )
    .unwrap();
    drive_task(&mut task, adapter, cancelled)
}

fn drive_task(
    task: &mut ScriptRunTask,
    adapter: &mut TestRunAdapter,
    mut cancelled: impl FnMut() -> bool,
) -> (ScriptRunReport, Vec<u32>) {
    let mut waits = Vec::new();
    loop {
        match task.advance(adapter, &mut cancelled) {
            ScriptRunAdvance::ItemCompleted { .. } => {}
            ScriptRunAdvance::WaitRequested { milliseconds } => waits.push(milliseconds),
            ScriptRunAdvance::Complete => break,
        }
    }
    (task.finish().unwrap(), waits)
}

fn assert_saved_output(source_bytes: &[u8], output_bytes: &[u8], report: &ScriptDryRunReport) {
    let mut direct =
        Core::from_procedure_file(decode_procedure_file(source_bytes).unwrap()).unwrap();
    let info = direct.document_info().unwrap();
    let source_digest = direct.document_state_digest().unwrap();
    let source_history = direct.history_entries();
    direct
        .set_plane_properties(info.color_plane_id, true, true, 1_000, "Scripted")
        .unwrap();
    let direct_editor = direct.editor_state().unwrap().digest;
    let direct_saved = Core::from_procedure_file(
        direct
            .build_procedure_file(Some(direct.current_state), Some(direct_editor))
            .unwrap(),
    )
    .unwrap();
    let reopened = Core::from_procedure_file(decode_procedure_file(output_bytes).unwrap()).unwrap();
    assert_eq!(
        reopened.document_state_digest().unwrap(),
        direct_saved.document_state_digest().unwrap()
    );
    assert_eq!(reopened.history_entries(), direct_saved.history_entries());
    assert_eq!(reopened.journal_state(), direct_saved.journal_state());
    assert_eq!(reopened.next_id, direct_saved.next_id);
    assert_eq!(reopened.next_procedure, direct_saved.next_procedure);
    assert_eq!(reopened.next_state, direct_saved.next_state);
    assert_eq!(reopened.next_journal_event, direct_saved.next_journal_event);
    assert_eq!(
        report.final_state_digest,
        direct.document_state_digest().unwrap()
    );
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
    let mut moved = reopened.clone();
    moved.undo().unwrap();
    assert_eq!(moved.document_state_digest().unwrap(), source_digest);
    assert_eq!(moved.history_entries().len(), source_history.len() + 1);
    moved.redo().unwrap();
    assert_eq!(
        moved.document_state_digest().unwrap(),
        reopened.document_state_digest().unwrap()
    );
}

fn outcomes(report: &ScriptRunReport) -> Vec<ScriptItemOutcome> {
    report
        .items
        .iter()
        .map(|item| item.outcome.clone())
        .collect()
}

fn existing(key: &str, object: u8, parent: u8) -> ValidatedPathIdentity {
    ValidatedPathIdentity::existing(
        key.to_owned(),
        [1; 16],
        [object; 32],
        digest(key.as_bytes()),
        [parent; 32],
        digest(format!("parent:{parent}").as_bytes()),
    )
    .unwrap()
}

fn absent(key: &str, parent: u8) -> ValidatedPathIdentity {
    ValidatedPathIdentity::expected_absent(
        key.to_owned(),
        [1; 16],
        [parent; 32],
        digest(key.as_bytes()),
        digest(format!("parent:{parent}").as_bytes()),
    )
    .unwrap()
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

#[test]
fn run_task_and_adapter_are_core_engine_send_values() {
    fn assert_send<T: Send>() {}
    assert_send::<ScriptRunTask>();
    assert_send::<Box<dyn ScriptRunAdapter>>();
}
