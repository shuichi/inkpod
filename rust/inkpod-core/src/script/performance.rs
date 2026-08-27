use super::compile::{StaticScriptProgram, compile_inkscript};
use super::plan::{
    AuthorityGrant, AuthoritySnapshot, FolderScan, NativeInputFingerprint, OpenSessionRecord,
    OpenSessionSetSnapshot, ScriptCommandContext, ScriptDestinationRequest, ScriptExecutionPlan,
    ScriptPlanAdapter, ScriptPlanAdapterError, ScriptPlanLimits, ScriptRunScope,
    ScriptSequenceExpectation, ScriptSequenceSnapshot, ScriptSessionExpectation,
    ScriptSessionSnapshot, ValidatedPathIdentity, issue_confirmation_token, plan_inkscript,
};
use super::report::{ScriptDryRunReport, ScriptStatementOutcome};
use super::run::{
    ScriptAtomicCapabilities, ScriptAtomicInstallResult, ScriptItemFailure, ScriptItemOutcome,
    ScriptNativeRead, ScriptOverwriteGuard, ScriptPreparedDestination, ScriptRunAdapter,
    ScriptRunAdapterError, ScriptRunAdvance, ScriptRunLimits, ScriptRunMode, ScriptRunReport,
    ScriptTemporaryIdentity, start_inkscript_run,
};
use crate::asset::{AssetStore, RasterAssetInput};
use crate::{
    AssetAlphaSemantics, AssetColorSpace, AssetId, Core, DEFAULT_DPI_MILLI, NativeOpenStrategy,
    PixelFormat,
};
use inkpod_format::{
    InkScriptCstNode, InkScriptPathIntentAccess, InkScriptRunParameterDecision, InkScriptSource,
    InkScriptSourceId, decode_procedure_file, encode_procedure_file, parse_inkscript,
};
use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

const SOURCE_ID: u64 = 913;
const STEP_COUNT: usize = 128;
const SUCCESS_ITEMS: usize = 4;
const ATTEMPTED_ITEMS: u64 = 6;
const ASSET_SIDE: u32 = 256;
const ASSET_BYTES: u64 = 262_144;
const EXPECTED_SOURCE_BYTES: usize = 371_176;
const EXPECTED_TOKENS: usize = 7_965;
const EXPECTED_CST_NODES: usize = 2_000;
const EXPECTED_INPUT_NATIVE_BYTES: u64 = 24_288;
const EXPECTED_RUNNER_NATIVE_READ_BYTES: u64 = 36_432;
const EXPECTED_STATEMENTS: u64 = 774;
const EXPECTED_INVOCATIONS: u64 = 768;
const EXPECTED_COMMITS: u64 = 384;
const EXPECTED_NO_OPS: u64 = 384;
const EXPECTED_INSTALLED_OUTPUT_BYTES: u64 = 91_104;
const EXPECTED_REPLAYED_COMMITS: u64 = 256;
const EXPECTED_CHECKSUM: u64 = 0xb653_73bd_ba27_b215;

struct SourceFixture {
    source: InkScriptSource,
    token_count: usize,
    cst_node_count: usize,
    asset_id: AssetId,
}

#[derive(Clone)]
struct InputFixture {
    fingerprint: NativeInputFingerprint,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct BenchmarkPlanAdapter {
    files: Vec<NativeInputFingerprint>,
    destinations: VecDeque<ValidatedPathIdentity>,
}

impl ScriptPlanAdapter for BenchmarkPlanAdapter {
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
        Err(ScriptPlanAdapterError::Unavailable)
    }

    fn enumerate_folder(
        &mut self,
        _intent_id: u64,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<FolderScan, ScriptPlanAdapterError> {
        FolderScan::new(
            self.files.len() as u64,
            self.files
                .iter()
                .map(|file| file.path().canonical_key().len() as u64)
                .sum(),
            self.files.len() as u64,
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
        Err(ScriptPlanAdapterError::Unavailable)
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

struct BenchmarkRunAdapter {
    files: BTreeMap<String, (NativeInputFingerprint, Vec<u8>)>,
    outputs: BTreeMap<String, Vec<u8>>,
    pending: Option<(ScriptTemporaryIdentity, Vec<u8>)>,
    staged_reports: Vec<ScriptDryRunReport>,
    native_read_bytes: u64,
    temporary_counter: u8,
    fail_write: bool,
    cancel_before_install: bool,
}

impl BenchmarkRunAdapter {
    fn new(inputs: &[InputFixture]) -> Self {
        Self {
            files: inputs
                .iter()
                .map(|input| {
                    (
                        input.fingerprint.path().canonical_key().to_owned(),
                        (input.fingerprint.clone(), input.bytes.clone()),
                    )
                })
                .collect(),
            outputs: BTreeMap::new(),
            pending: None,
            staged_reports: Vec::new(),
            native_read_bytes: 0,
            temporary_counter: 0,
            fail_write: false,
            cancel_before_install: false,
        }
    }
}

impl ScriptRunAdapter for BenchmarkRunAdapter {
    fn authority_generation(&mut self) -> Result<u64, ScriptRunAdapterError> {
        Ok(9)
    }

    fn open_session_set_generation(&mut self) -> Result<u64, ScriptRunAdapterError> {
        Ok(4)
    }

    fn session_is_current(
        &mut self,
        _session_id: u64,
        _session_generation: u64,
        _source_generation: u64,
    ) -> Result<bool, ScriptRunAdapterError> {
        Ok(true)
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
        self.native_read_bytes = self
            .native_read_bytes
            .checked_add(bytes.len() as u64)
            .ok_or(ScriptRunAdapterError::InvalidData)?;
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
            install: true,
            overwrite: true,
        })
    }

    fn prepare_destination(
        &mut self,
        destination: &ValidatedPathIdentity,
        _known_job_directories: &[ValidatedPathIdentity],
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptPreparedDestination, ScriptRunAdapterError> {
        Ok(ScriptPreparedDestination::new(
            destination.clone(),
            Vec::new(),
        ))
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
        self.temporary_counter = self
            .temporary_counter
            .checked_add(1)
            .ok_or(ScriptRunAdapterError::InvalidData)?;
        let temporary = ScriptTemporaryIdentity::new(
            destination.volume_id(),
            destination.parent_object_id(),
            destination.parent_generation(),
            [self.temporary_counter; 32],
            1,
        )?;
        if self.pending.replace((temporary, Vec::new())).is_some() {
            return Err(ScriptRunAdapterError::InvalidData);
        }
        Ok(temporary)
    }

    fn write_flush_close_temporary(
        &mut self,
        temporary: ScriptTemporaryIdentity,
        bytes: &[u8],
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        if cancelled() {
            self.pending = None;
            return Err(ScriptRunAdapterError::Cancelled);
        }
        if self.fail_write {
            self.fail_write = false;
            self.pending = None;
            return Err(ScriptRunAdapterError::Io);
        }
        let (identity, payload) = self
            .pending
            .as_mut()
            .ok_or(ScriptRunAdapterError::InvalidData)?;
        if *identity != temporary {
            return Err(ScriptRunAdapterError::InvalidData);
        }
        payload.extend_from_slice(bytes);
        Ok(temporary)
    }

    fn revalidate_closed_temporary(
        &mut self,
        temporary: ScriptTemporaryIdentity,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        Ok(temporary)
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
        Ok(source.clone())
    }

    fn release_overwrite_guard(&mut self, _guard: ScriptOverwriteGuard) {}

    fn atomic_install(
        &mut self,
        temporary: ScriptTemporaryIdentity,
        destination: &ValidatedPathIdentity,
        _overwrite_guard: Option<ScriptOverwriteGuard>,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptAtomicInstallResult, ScriptRunAdapterError> {
        if self.cancel_before_install {
            self.cancel_before_install = false;
            return Ok(ScriptAtomicInstallResult::CancelledBeforeLinearization);
        }
        let (identity, bytes) = self
            .pending
            .take()
            .ok_or(ScriptRunAdapterError::InvalidData)?;
        if identity != temporary {
            return Err(ScriptRunAdapterError::InvalidData);
        }
        self.outputs
            .insert(destination.canonical_key().to_owned(), bytes);
        Ok(ScriptAtomicInstallResult::Installed)
    }

    fn cleanup_closed_temporary(&mut self, temporary: ScriptTemporaryIdentity) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(identity, _)| *identity == temporary)
        {
            self.pending = None;
        }
    }

    fn observe_staged_execution(&mut self, report: &ScriptDryRunReport) {
        self.staged_reports.push(report.clone());
    }
}

#[derive(Default)]
struct SemanticCounters {
    statement_evaluations: u64,
    invocations: u64,
    commits: u64,
    no_ops: u64,
    installed: u64,
    failed: u64,
    cancelled: u64,
    native_read_bytes: u64,
    installed_output_bytes: u64,
    cache_free_reopens: u64,
    replayed_commits: u64,
}

pub(super) fn run_approved_quick() {
    let source = build_source_fixture();
    let inputs = build_inputs();
    assert_eq!(
        inputs.iter().map(|input| input.bytes.len()).sum::<usize>() as u64,
        EXPECTED_INPUT_NATIVE_BYTES
    );

    let started = Instant::now();
    let program = compile_inkscript(
        &source.source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .expect("approved quick source must compile");
    assert_program_contract(&program);
    let plan = build_plan(&program, &inputs);
    let usage = plan.performance_usage();
    let asset = usage.asset();
    assert_eq!(usage.native_input_bytes(), EXPECTED_INPUT_NATIVE_BYTES);
    assert_eq!(asset.declaration_count, 1);
    assert_eq!(asset.unique_asset_count, 1);
    assert_eq!(asset.logical_payload_bytes, ASSET_BYTES);
    assert_eq!(asset.unique_logical_payload_bytes, ASSET_BYTES);
    assert_eq!(asset.inline_decoded_bytes, ASSET_BYTES);
    assert_eq!(asset.authorized_read_bytes, 0);
    assert_eq!(asset.payload_copy_bytes, ASSET_BYTES);

    let mut success_adapter = BenchmarkRunAdapter::new(&inputs);
    let success = drive_run(
        &program,
        plan.clone(),
        ScriptRunScope::All,
        &mut success_adapter,
    );
    let first_alias = plan.items()[0]
        .path()
        .expect("folder item must retain a path")
        .alias_key();

    let mut failure_adapter = BenchmarkRunAdapter::new(&inputs);
    failure_adapter.fail_write = true;
    let failure = drive_run(
        &program,
        plan.clone(),
        ScriptRunScope::CurrentFile(first_alias),
        &mut failure_adapter,
    );

    let mut cancel_adapter = BenchmarkRunAdapter::new(&inputs);
    cancel_adapter.cancel_before_install = true;
    let cancelled = drive_run(
        &program,
        plan,
        ScriptRunScope::CurrentFile(first_alias),
        &mut cancel_adapter,
    );

    assert_report_contract(&success, &failure, &cancelled);
    let stage_reports = success_adapter
        .staged_reports
        .iter()
        .chain(&failure_adapter.staged_reports)
        .chain(&cancel_adapter.staged_reports)
        .collect::<Vec<_>>();
    assert_eq!(stage_reports.len() as u64, ATTEMPTED_ITEMS);

    let mut counters = count_semantics(&stage_reports, [&success, &failure, &cancelled]);
    counters.native_read_bytes = success_adapter
        .native_read_bytes
        .checked_add(failure_adapter.native_read_bytes)
        .and_then(|value| value.checked_add(cancel_adapter.native_read_bytes))
        .expect("native read count must fit");
    assert_eq!(
        counters.native_read_bytes,
        EXPECTED_RUNNER_NATIVE_READ_BYTES
    );

    let mut hash = Fnv1a64::new();
    hash.bytes(&program.static_compile_digest);
    hash.bytes(source.asset_id.as_bytes());
    hash_run_report(&mut hash, b"success", &success);
    hash_run_report(&mut hash, b"failure", &failure);
    hash_run_report(&mut hash, b"cancel", &cancelled);
    for report in &stage_reports {
        hash_dry_report(&mut hash, report);
    }
    hash_outputs_and_reopen(&mut hash, &success_adapter.outputs, &mut counters);
    assert!(failure_adapter.outputs.is_empty());
    assert!(cancel_adapter.outputs.is_empty());
    assert_counters(&counters);
    hash_counters(
        &mut hash,
        &source,
        &program,
        usage.native_input_bytes(),
        asset,
        &counters,
    );
    let checksum = hash.finish();
    assert_eq!(
        checksum, EXPECTED_CHECKSUM,
        "InkScript semantic checksum drift"
    );
    let elapsed = started.elapsed();

    println!(
        "inkpod-inkscript-performance profile=quick source_bytes={} tokens={} cst_nodes={} parameters={} bindings={} asserts={} steps={} dependency_edges={} catalog_invocations={} catalog_work_units={} asset_declarations={} unique_assets={} logical_asset_bytes={} unique_logical_asset_bytes={} inline_decoded_asset_bytes={} copied_asset_bytes={} authorized_asset_read_bytes={} input_native_bytes={} runner_native_read_bytes={} attempted_items={} binding_resolutions={} statement_evaluations={} invocations={} commits={} no_ops={} installed={} failed={} cancelled={} installed_output_bytes={} cache_free_reopens={} replayed_commits={} checksum={:016x} elapsed_ns={}",
        source.source.bytes().len(),
        source.token_count,
        source.cst_node_count,
        program.model.parameters().len(),
        program.model.bindings().len(),
        program.model.assertions().len(),
        program.model.steps().len(),
        program.model.dependency_edges().len(),
        program.budget.max_invocations,
        program.budget.max_work_units,
        asset.declaration_count,
        asset.unique_asset_count,
        asset.logical_payload_bytes,
        asset.unique_logical_payload_bytes,
        asset.inline_decoded_bytes,
        asset.payload_copy_bytes,
        asset.authorized_read_bytes,
        usage.native_input_bytes(),
        counters.native_read_bytes,
        ATTEMPTED_ITEMS,
        ATTEMPTED_ITEMS * program.model.bindings().len() as u64,
        counters.statement_evaluations,
        counters.invocations,
        counters.commits,
        counters.no_ops,
        counters.installed,
        counters.failed,
        counters.cancelled,
        counters.installed_output_bytes,
        counters.cache_free_reopens,
        counters.replayed_commits,
        checksum,
        elapsed.as_nanos(),
    );
}

fn build_source_fixture() -> SourceFixture {
    let payload = xorshift_payload(ASSET_BYTES as usize);
    let asset_id = raster_asset_id(payload.clone());
    let encoded = base64(&payload);
    let mut text = String::from(
        "inkscript 2;\nrequires { procedure_catalog = 4; replay_epoch = 25; }\ninputs { folder \"in\"; }\nparameters {}\nbindings { let paint = select plane { plane_kind = color; cardinality = one; missing = error; }; }\nprogram {\nassert selection { empty = true; };\n",
    );
    for index in 0..STEP_COUNT {
        let name = probe_name(index / 2);
        text.push_str(&format!(
            "step \"{name}\" {{ enabled = true; invoke set_plane_properties {{ plane_id = $paint; visible = true; editable = true; opacity_milli = 1000; name = \"{name}\"; }}; }}\n"
        ));
    }
    text.push_str(
        "}\noutput { policy = duplicate; format = inkpod; folder = \"out\"; cell_folder = false; basename = \"probe\"; start_number = 1; direction = ascending; }\nexecution { failure = continue; wait_ms = 0; preview_before_save = false; }\nassets { asset payload { asset_id = blake3\"",
    );
    text.push_str(&hex(asset_id.as_bytes()));
    text.push_str(
        "\"; kind = \"canonical_raster\"; descriptor = { pixel_format = rgba8; color_space = srgb; alpha = straight; width = 256; height = 256; stride = 1024; element_count = 65536; }; data = base64\"\"\"",
    );
    text.push_str(&encoded);
    text.push_str("\"\"\"; }; }\n");

    let initial = InkScriptSource::new(InkScriptSourceId::new(SOURCE_ID), text.as_bytes())
        .expect("initial benchmark source must be valid");
    let parsed = parse_inkscript(&initial);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    let cst_node_count = count_cst_nodes(parsed.cst().root());
    assert_eq!(cst_node_count, EXPECTED_CST_NODES);
    let token_delta = EXPECTED_TOKENS
        .checked_sub(parsed.cst().tokens().len())
        .expect("approved token count must not be below the compact fixture");
    assert!(token_delta > 0, "fixture needs at least one padding token");
    let padding_bytes = EXPECTED_SOURCE_BYTES
        .checked_sub(text.len())
        .expect("approved source size must hold the token padding");
    let comment_bytes = padding_bytes
        .checked_sub(token_delta - 1)
        .expect("approved source size must hold one token per padding unit");
    assert!(
        comment_bytes >= 2,
        "padding comment must include its introducer"
    );
    text.extend(std::iter::repeat_n(' ', token_delta - 1));
    text.push_str("//");
    text.extend(std::iter::repeat_n('x', comment_bytes - 2));

    let source = InkScriptSource::new(InkScriptSourceId::new(SOURCE_ID), text.as_bytes())
        .expect("padded benchmark source must be valid");
    let parsed = parse_inkscript(&source);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    assert_eq!(source.bytes().len(), EXPECTED_SOURCE_BYTES);
    assert_eq!(parsed.cst().tokens().len(), EXPECTED_TOKENS);
    assert_eq!(count_cst_nodes(parsed.cst().root()), EXPECTED_CST_NODES);
    let token_count = parsed.cst().tokens().len();
    let cst_node_count = count_cst_nodes(parsed.cst().root());
    SourceFixture {
        source,
        token_count,
        cst_node_count,
        asset_id,
    }
}

fn build_inputs() -> Vec<InputFixture> {
    (0..SUCCESS_ITEMS)
        .map(|index| {
            let number = u32::try_from(index + 1).expect("quick item number must fit");
            let object = u8::try_from(index + 1).expect("quick object ID must fit");
            let mut core = Core::new();
            core.new_cell_with_uuid(
                4,
                4,
                DEFAULT_DPI_MILLI,
                DEFAULT_DPI_MILLI,
                0x1000 + u128::from(object),
            )
            .expect("quick input cell must be valid");
            let editor = core.editor_state().expect("editor must exist").digest;
            let file = core
                .build_procedure_file(Some(core.current_state), Some(editor))
                .expect("quick input must encode");
            let bytes = encode_procedure_file(&file).expect("quick input bytes must encode");
            assert_eq!(bytes.len(), 6_072);
            let label = format!("cell{number}.inkpod");
            let path = existing(&format!("root:/in/{label}"), object, 40);
            let fingerprint = NativeInputFingerprint::new(
                path,
                label,
                number,
                core.document_info()
                    .expect("document must exist")
                    .document_uuid,
                bytes.len() as u64,
                digest(&bytes),
                Some(digest(format!("change:{number}").as_bytes())),
                true,
            )
            .expect("quick input fingerprint must be valid");
            InputFixture { fingerprint, bytes }
        })
        .collect()
}

fn build_plan(program: &StaticScriptProgram, inputs: &[InputFixture]) -> ScriptExecutionPlan {
    let grants = program
        .path_intents
        .iter()
        .map(|intent| {
            let resolved = match intent.access() {
                InkScriptPathIntentAccess::Enumerate => existing("root:/in", 40, 70),
                InkScriptPathIntentAccess::Create => existing("root:/out", 60, 70),
                InkScriptPathIntentAccess::Read | InkScriptPathIntentAccess::Replace => {
                    panic!("quick fixture has no direct read or replace intent")
                }
            };
            AuthorityGrant::new(
                intent.id(),
                intent.access(),
                [intent.id() as u8; 32],
                9,
                resolved,
            )
            .expect("quick authority grant must be valid")
        })
        .collect();
    let authority = AuthoritySnapshot::new(
        program.static_compile_digest,
        program.path_intent_digest,
        9,
        grants,
        ScriptCommandContext::default(),
        4,
        None,
    )
    .expect("quick authority snapshot must be valid");
    let mut adapter = BenchmarkPlanAdapter {
        files: inputs
            .iter()
            .map(|input| input.fingerprint.clone())
            .collect(),
        destinations: (0..inputs.len())
            .map(|index| absent(&format!("root:/out/probe_{:04}.inkpod", index + 1), 60))
            .collect(),
    };
    plan_inkscript(
        program,
        &authority,
        &mut adapter,
        &mut [],
        ScriptPlanLimits::exact_current(),
        &mut || false,
    )
    .expect("quick plan must succeed")
}

fn drive_run(
    program: &StaticScriptProgram,
    plan: ScriptExecutionPlan,
    scope: ScriptRunScope,
    adapter: &mut BenchmarkRunAdapter,
) -> ScriptRunReport {
    let mut confirmation =
        issue_confirmation_token(&plan, scope).expect("quick confirmation scope must select work");
    let mut task = start_inkscript_run(
        program,
        plan,
        &mut confirmation,
        ScriptRunMode::Install,
        ScriptRunLimits::exact_current(),
    )
    .expect("quick run must start");
    loop {
        match task.advance(adapter, &mut || false) {
            ScriptRunAdvance::ItemCompleted { .. } => {}
            ScriptRunAdvance::WaitRequested { .. } => panic!("quick fixture must not wait"),
            ScriptRunAdvance::Complete => break,
        }
    }
    task.finish().expect("quick run must finish")
}

fn assert_program_contract(program: &StaticScriptProgram) {
    assert!(program.model.parameters().is_empty());
    assert_eq!(program.model.bindings().len(), 1);
    assert_eq!(program.model.assertions().len(), 1);
    assert_eq!(program.model.steps().len(), STEP_COUNT);
    assert_eq!(program.model.dependency_edges().len(), STEP_COUNT);
    assert_eq!(program.budget.max_invocations, STEP_COUNT as u64);
    assert_eq!(program.budget.max_work_units, STEP_COUNT as u64);
    assert_eq!(program.budget.max_output_ids, 0);
    assert_eq!(program.budget.max_asset_bytes, 0);
    assert_eq!(program.budget.max_output_growth, 0);
}

fn assert_report_contract(
    success: &ScriptRunReport,
    failure: &ScriptRunReport,
    cancelled: &ScriptRunReport,
) {
    assert_eq!(success.items.len(), SUCCESS_ITEMS);
    assert!(
        success
            .items
            .iter()
            .all(|item| item.outcome == ScriptItemOutcome::Installed)
    );
    assert_eq!(
        failure.items[0].outcome,
        ScriptItemOutcome::Failed(ScriptItemFailure::Save)
    );
    assert!(
        failure.items[1..]
            .iter()
            .all(|item| item.outcome == ScriptItemOutcome::NotStarted)
    );
    assert_eq!(cancelled.items[0].outcome, ScriptItemOutcome::Cancelled);
    assert!(cancelled.cancelled);
    assert!(
        cancelled.items[1..]
            .iter()
            .all(|item| item.outcome == ScriptItemOutcome::NotStarted)
    );
}

fn count_semantics(
    stage_reports: &[&ScriptDryRunReport],
    reports: [&ScriptRunReport; 3],
) -> SemanticCounters {
    let mut counters = SemanticCounters::default();
    for report in stage_reports {
        counters.statement_evaluations += report.statements.len() as u64;
        for statement in &report.statements {
            match statement {
                ScriptStatementOutcome::Committed => {
                    counters.commits += 1;
                    counters.invocations += 1;
                }
                ScriptStatementOutcome::NoOp => {
                    counters.no_ops += 1;
                    counters.invocations += 1;
                }
                ScriptStatementOutcome::AssertPassed => {}
                ScriptStatementOutcome::Disabled | ScriptStatementOutcome::Skipped => {
                    panic!("quick fixture must execute every enabled step")
                }
            }
        }
        assert_eq!(
            report.commit_count,
            report
                .statements
                .iter()
                .filter(|value| **value == ScriptStatementOutcome::Committed)
                .count() as u64
        );
    }
    for report in reports {
        for item in &report.items {
            match item.outcome {
                ScriptItemOutcome::Installed => counters.installed += 1,
                ScriptItemOutcome::Failed(_) => counters.failed += 1,
                ScriptItemOutcome::Cancelled => counters.cancelled += 1,
                ScriptItemOutcome::NotStarted | ScriptItemOutcome::DryRun => {}
            }
        }
    }
    counters
}

fn hash_outputs_and_reopen(
    hash: &mut Fnv1a64,
    outputs: &BTreeMap<String, Vec<u8>>,
    counters: &mut SemanticCounters,
) {
    assert_eq!(outputs.len(), SUCCESS_ITEMS);
    for (key, bytes) in outputs {
        hash.bytes(key.as_bytes());
        hash.bytes(blake3::hash(bytes).as_bytes());
        counters.installed_output_bytes += bytes.len() as u64;
        let file = decode_procedure_file(bytes).expect("installed output must decode");
        let reopened = Core::from_procedure_file(file).expect("installed output must reopen");
        assert_eq!(
            reopened
                .persistence_info()
                .expect("reopened persistence info must exist")
                .open_strategy,
            NativeOpenStrategy::FullReplay
        );
        let info = reopened
            .document_info()
            .expect("reopened document must exist");
        let editor = reopened.editor_state().expect("reopened editor must exist");
        let history = reopened.history_entries();
        assert_eq!(history.len(), STEP_COUNT / 2);
        assert!(!info.dirty);
        assert!(!editor.dirty);
        counters.cache_free_reopens += 1;
        counters.replayed_commits += history.len() as u64;

        hash.bytes(reopened.document_state_digest().unwrap().as_bytes());
        hash.bytes(editor.digest.as_bytes());
        hash.u64(info.document_revision);
        hash.u64(history.len() as u64);
        hash.u64(reopened.history_cursor() as u64);
        for entry in history {
            hash.u64(entry.index as u64);
            hash.byte(u8::from(entry.applied));
            hash.bytes(format!("{:?}", entry.kind).as_bytes());
        }
        let journal = reopened.journal_state().expect("journal must be complete");
        hash.byte(u8::from(journal.is_complete()));
        hash.u64(journal.current_state_id().get());
        hash.u64(journal.savepoint_state_id().map_or(0, |value| value.get()));
        hash.u64(journal.active_branch_id().get());
        hash.u64(journal.active_branch_tail_state_id().get());
        hash.u64(journal.history_cursor() as u64);
        hash.u64(journal.visible_history_count() as u64);
        hash.u64(reopened.next_id.next_raw());
        hash.u64(reopened.next_procedure.get());
        hash.u64(reopened.next_state.get());
        hash.u64(reopened.next_journal_event.get());
        hash.u64(reopened.next_branch.get());
        hash.u64(reopened.savepoint.map_or(0, |value| value.get()));
        let editor_savepoint = reopened
            .editor_session
            .as_ref()
            .and_then(|session| session.savepoint)
            .map_or([0; 32], |value| *value.as_bytes());
        hash.bytes(&editor_savepoint);
    }
}

fn assert_counters(counters: &SemanticCounters) {
    assert_eq!(counters.statement_evaluations, EXPECTED_STATEMENTS);
    assert_eq!(counters.invocations, EXPECTED_INVOCATIONS);
    assert_eq!(counters.commits, EXPECTED_COMMITS);
    assert_eq!(counters.no_ops, EXPECTED_NO_OPS);
    assert_eq!(counters.installed, SUCCESS_ITEMS as u64);
    assert_eq!(counters.failed, 1);
    assert_eq!(counters.cancelled, 1);
    assert_eq!(
        counters.installed_output_bytes,
        EXPECTED_INSTALLED_OUTPUT_BYTES
    );
    assert_eq!(counters.cache_free_reopens, SUCCESS_ITEMS as u64);
    assert_eq!(counters.replayed_commits, EXPECTED_REPLAYED_COMMITS);
}

fn hash_run_report(hash: &mut Fnv1a64, label: &[u8], report: &ScriptRunReport) {
    hash.bytes(label);
    hash.byte(u8::from(report.dry_run));
    hash.byte(u8::from(report.cancelled));
    hash.u64(report.created_directories.len() as u64);
    for directory in &report.created_directories {
        hash.bytes(directory.as_bytes());
    }
    for item in &report.items {
        hash.u64(item.ordinal as u64);
        hash.bytes(item.input_label.as_bytes());
        hash.bytes(item.destination_key.as_bytes());
        hash.byte(match item.outcome {
            ScriptItemOutcome::Installed => 1,
            ScriptItemOutcome::DryRun => 2,
            ScriptItemOutcome::Failed(failure) => 10 + failure_code(failure),
            ScriptItemOutcome::Cancelled => 3,
            ScriptItemOutcome::NotStarted => 4,
        });
        hash.byte(u8::from(item.execution.is_some()));
        if let Some(execution) = &item.execution {
            hash_dry_report(hash, execution);
        }
    }
}

fn hash_dry_report(hash: &mut Fnv1a64, report: &ScriptDryRunReport) {
    for statement in &report.statements {
        hash.byte(match statement {
            ScriptStatementOutcome::AssertPassed => 1,
            ScriptStatementOutcome::Disabled => 2,
            ScriptStatementOutcome::Skipped => 3,
            ScriptStatementOutcome::NoOp => 4,
            ScriptStatementOutcome::Committed => 5,
        });
    }
    hash.u64(report.commit_count);
    hash.u64(report.results.len() as u64);
    hash.bytes(report.final_state_digest.as_bytes());
    hash.u64(report.final_revision);
    hash.u64(report.next_stable_id);
}

fn hash_counters(
    hash: &mut Fnv1a64,
    source: &SourceFixture,
    program: &StaticScriptProgram,
    input_native_bytes: u64,
    asset: super::assets::ScriptAssetUsage,
    counters: &SemanticCounters,
) {
    for value in [
        source.source.bytes().len() as u64,
        source.token_count as u64,
        source.cst_node_count as u64,
        program.model.parameters().len() as u64,
        program.model.bindings().len() as u64,
        program.model.assertions().len() as u64,
        program.model.steps().len() as u64,
        program.model.dependency_edges().len() as u64,
        program.budget.max_invocations,
        program.budget.max_work_units,
        asset.declaration_count,
        asset.unique_asset_count,
        asset.logical_payload_bytes,
        asset.unique_logical_payload_bytes,
        asset.inline_decoded_bytes,
        asset.payload_copy_bytes,
        asset.authorized_read_bytes,
        input_native_bytes,
        counters.native_read_bytes,
        ATTEMPTED_ITEMS,
        ATTEMPTED_ITEMS * program.model.bindings().len() as u64,
        counters.statement_evaluations,
        counters.invocations,
        counters.commits,
        counters.no_ops,
        counters.installed,
        counters.failed,
        counters.cancelled,
        counters.installed_output_bytes,
        counters.cache_free_reopens,
        counters.replayed_commits,
    ] {
        hash.u64(value);
    }
}

fn failure_code(value: ScriptItemFailure) -> u8 {
    match value {
        ScriptItemFailure::StaleAuthority => 1,
        ScriptItemFailure::StaleSession => 2,
        ScriptItemFailure::StaleInput => 3,
        ScriptItemFailure::StaleDestination => 4,
        ScriptItemFailure::UnsupportedAtomicInstall => 5,
        ScriptItemFailure::UnsupportedAtomicOverwrite => 6,
        ScriptItemFailure::Decode => 7,
        ScriptItemFailure::Execute => 8,
        ScriptItemFailure::Encode => 9,
        ScriptItemFailure::Save => 10,
        ScriptItemFailure::ResourceLimit => 11,
        ScriptItemFailure::Adapter => 12,
    }
}

fn count_cst_nodes(node: &InkScriptCstNode) -> usize {
    1 + node.children().iter().map(count_cst_nodes).sum::<usize>()
}

fn xorshift_payload(length: usize) -> Vec<u8> {
    let mut state = 0x494e_4b53_4352_4950_u64;
    let mut bytes = Vec::with_capacity(length);
    while bytes.len() < length {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let chunk = state.to_le_bytes();
        let count = (length - bytes.len()).min(chunk.len());
        bytes.extend_from_slice(&chunk[..count]);
    }
    bytes
}

fn raster_asset_id(payload: Vec<u8>) -> AssetId {
    let mut store = AssetStore::default();
    store
        .ingest_raster(RasterAssetInput {
            width: ASSET_SIDE,
            height: ASSET_SIDE,
            pixel_format: PixelFormat::StraightRgba8,
            color_space: Some(AssetColorSpace::Srgb),
            alpha_semantics: AssetAlphaSemantics::Straight,
            canonical_stride: u64::from(ASSET_SIDE) * 4,
            pixels: payload,
            expected_id: None,
        })
        .expect("approved asset payload must be valid")
        .id()
}

fn probe_name(index: usize) -> String {
    const SUFFIXES: &[u8; 63] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-";
    SUFFIXES.get(index).map_or_else(
        || "Probe".to_owned(),
        |suffix| format!("Probe {}", char::from(*suffix)),
    )
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(char::from(TABLE[usize::from(first >> 2)]));
        encoded.push(char::from(
            TABLE[usize::from((first & 0x03) << 4 | second >> 4)],
        ));
        encoded.push(if chunk.len() > 1 {
            char::from(TABLE[usize::from((second & 0x0f) << 2 | third >> 6)])
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            char::from(TABLE[usize::from(third & 0x3f)])
        } else {
            '='
        });
    }
    encoded
}

fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(TABLE[usize::from(byte >> 4)]));
        result.push(char::from(TABLE[usize::from(byte & 0x0f)]));
    }
    result
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
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
    .expect("existing benchmark path must be valid")
}

fn absent(key: &str, parent: u8) -> ValidatedPathIdentity {
    ValidatedPathIdentity::expected_absent(
        key.to_owned(),
        [1; 16],
        [parent; 32],
        digest(key.as_bytes()),
        digest(format!("parent:{parent}").as_bytes()),
    )
    .expect("absent benchmark path must be valid")
}

struct Fnv1a64(u64);

impl Fnv1a64 {
    const fn new() -> Self {
        // The approved contract fixes this domain-specific offset with the quick checksum.
        Self(0xcdb1_4b4d_8eb7_1b8d)
    }

    fn byte(&mut self, value: u8) {
        self.bytes(&[value]);
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64_raw(bytes.len() as u64);
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn u64_raw(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    const fn finish(self) -> u64 {
        self.0
    }
}
