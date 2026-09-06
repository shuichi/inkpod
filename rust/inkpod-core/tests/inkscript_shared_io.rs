use inkpod_core::inkscript::abi_bridge::*;
use inkpod_core::inkscript::{
    InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId, ScriptIoAdapter,
    StaticScriptProgram, compile_inkscript,
};
use inkpod_core::{Core, DEFAULT_DPI_MILLI, GridConfig};
use inkpod_io::{IoConfig, IoManager};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "inkpod-script-shared-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn core(uuid: u128) -> Core {
    let mut core = Core::new();
    core.new_cell_with_uuid(3, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
        .unwrap();
    core
}

fn file_publication_available() -> bool {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    if manager.supports_guarded_publication() {
        return true;
    }
    let root = Directory::new();
    let destination = root.0.join("unsupported.inkpod");
    let context = inkpod_io::JobContext::new();
    let authority = manager
        .observe_path_authority(&destination, &context)
        .unwrap();
    assert!(matches!(
        manager.publish_guarded(&authority, None, b"never", &context, &mut || false),
        Err(inkpod_io::IoError::InvalidInput(_))
    ));
    assert!(!destination.exists());
    false
}
fn program(profile: &str, inputs: &str, output: &str) -> StaticScriptProgram {
    let text = format!(
        "inkscript 3; requires {{ procedure_catalog = 8; replay_epoch = 29; }} inputs {{profile = {profile}; {inputs}}} program {{}} output {{{output}}} execution {{failure = continue; wait_ms = 0; preview_before_save = false;}}"
    );
    compile_inkscript(
        &InkScriptSource::new(InkScriptSourceId::new(904), text.as_bytes()).unwrap(),
        InkScriptRunParameterDecision::Resolve(vec![]),
    )
    .unwrap()
}
fn adapter(program: &StaticScriptProgram, root: &Path, capacity: usize) -> ScriptIoAdapter {
    ScriptIoAdapter::new(
        IoManager::new(IoConfig::default()).unwrap(),
        program
            .path_intents()
            .iter()
            .map(|intent| (intent.id(), root.join(intent.text())))
            .collect(),
        capacity,
    )
    .unwrap()
}
fn task(
    program: &StaticScriptProgram,
    adapter: &mut ScriptIoAdapter,
    current: Option<u64>,
    mode: ScriptRunMode,
) -> ScriptRunTask {
    let authority = adapter.authority(program, current, None).unwrap();
    let plan = plan_inkscript(
        program,
        &authority,
        adapter,
        &mut [],
        ScriptPlanLimits::exact_current(),
        &mut || false,
    )
    .unwrap();
    let mut confirmation = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
    start_inkscript_run(
        program,
        plan,
        &mut confirmation,
        mode,
        ScriptRunLimits::exact_current(),
    )
    .unwrap()
}
fn finish(task: &mut ScriptRunTask, adapter: &mut ScriptIoAdapter) -> ScriptRunReport {
    loop {
        if matches!(
            task.advance(adapter, &mut || false),
            ScriptRunAdvance::Complete
        ) {
            return task.finish().unwrap();
        }
    }
}

#[test]
fn shared_native_folder_runs_and_dry_run_never_creates_outputs() {
    if !file_publication_available() {
        return;
    }
    let root = Directory::new();
    let mut input = core(901);
    input.save(&root.0.join("A001.inkpod")).unwrap();
    let original = fs::read(root.0.join("A001.inkpod")).unwrap();
    let program = program(
        "batch",
        "file \"A001.inkpod\";",
        "policy = folder; format = inkpod; folder = \"out\"; naming_template = \"{stem}_{index:2}\";",
    );
    let mut adapter = adapter(&program, &root.0, 0);
    let mut dry = task(&program, &mut adapter, None, ScriptRunMode::DryRun);
    assert_eq!(
        finish(&mut dry, &mut adapter).items[0].outcome,
        ScriptItemOutcome::DryRun
    );
    assert!(!root.0.join("out").exists());
    let mut run = task(&program, &mut adapter, None, ScriptRunMode::Install);
    let report = finish(&mut run, &mut adapter);
    assert_eq!(
        report.items[0].outcome,
        ScriptItemOutcome::Installed,
        "{report:?}"
    );
    let mut reopened = Core::new();
    reopened.open(&root.0.join("out/A001_01.inkpod")).unwrap();
    assert_eq!(
        reopened.document_state_digest().unwrap(),
        input.document_state_digest().unwrap()
    );
    assert_eq!(fs::read(root.0.join("A001.inkpod")).unwrap(), original);
}

#[test]
fn shared_explicit_overwrite_remains_supported_and_aliases_fail_before_work() {
    if !file_publication_available() {
        return;
    }
    let root = Directory::new();
    let path = root.0.join("A001.inkpod");
    core(902).save(&path).unwrap();
    let program = program(
        "canonical",
        "file \"A001.inkpod\";",
        "policy = explicit_overwrite; format = inkpod;",
    );
    let mut adapter = adapter(&program, &root.0, 0);
    let mut run = task(&program, &mut adapter, None, ScriptRunMode::Install);
    let report = finish(&mut run, &mut adapter);
    assert_eq!(
        report.items[0].outcome,
        ScriptItemOutcome::Installed,
        "{report:?}"
    );
    fs::hard_link(&path, root.0.join("alias.inkpod")).unwrap();
    let duplicate = program_source_alias();
    let mut duplicate_adapter = adapter_for_alias(&duplicate, &root.0);
    let authority = duplicate_adapter.authority(&duplicate, None, None).unwrap();
    assert!(
        plan_inkscript(
            &duplicate,
            &authority,
            &mut duplicate_adapter,
            &mut [],
            ScriptPlanLimits::exact_current(),
            &mut || false
        )
        .is_err()
    );
}
fn program_source_alias() -> StaticScriptProgram {
    program(
        "batch",
        "file \"A001.inkpod\"; file \"alias.inkpod\";",
        "policy = new_tabs;",
    )
}
fn adapter_for_alias(program: &StaticScriptProgram, root: &Path) -> ScriptIoAdapter {
    adapter(program, root, 2)
}

#[test]
fn captured_dirty_file_uses_canonical_snapshot_and_batch_reads_disk() {
    let root = Directory::new();
    let path = root.0.join("A001.inkpod");
    let mut live = core(903);
    live.save(&path).unwrap();
    let disk_digest = live.document_state_digest().unwrap();
    live.set_grid(GridConfig {
        origin_x: 1,
        origin_y: 2,
        spacing_x: 8,
        spacing_y: 9,
        subdivisions: 2,
    })
    .unwrap();
    let live_digest = live.document_state_digest().unwrap();
    assert_ne!(live_digest, disk_digest);
    for (profile, expected) in [("canonical", live_digest), ("batch", disk_digest)] {
        let program = program(profile, "file \"A001.inkpod\";", "policy = new_tabs;");
        let mut adapter = adapter(&program, &root.0, 1);
        adapter
            .capture_session(1, 1, 1, "A001.inkpod".into(), 1, Some(path.clone()), &live)
            .unwrap();
        let mut run = task(&program, &mut adapter, Some(1), ScriptRunMode::DryRun);
        let report = finish(&mut run, &mut adapter);
        assert_eq!(
            report.items[0].outcome,
            ScriptItemOutcome::DryRun,
            "{report:?}"
        );
        assert_eq!(
            report.items[0]
                .execution
                .as_ref()
                .unwrap()
                .final_state_digest(),
            expected
        );
    }
    assert_eq!(live.document_state_digest().unwrap(), live_digest);
}

#[test]
fn new_tab_capacity_stale_authority_and_shared_session_invalidation_are_enforced() {
    let root = Directory::new();
    let live = core(904);
    let program = program("batch", "current_document;", "policy = new_tabs;");
    let mut adapter = adapter(&program, &root.0, 0);
    adapter
        .capture_session(1, 1, 1, "active-document.inkpod".into(), 1, None, &live)
        .unwrap();
    let authority = adapter.authority(&program, Some(1), None).unwrap();
    assert!(
        plan_inkscript(
            &program,
            &authority,
            &mut adapter,
            &mut [],
            ScriptPlanLimits::exact_current(),
            &mut || false
        )
        .is_err()
    );
    let mut adapter =
        ScriptIoAdapter::new(IoManager::new(IoConfig::default()).unwrap(), vec![], 1).unwrap();
    adapter
        .capture_session(1, 1, 1, "active-document.inkpod".into(), 1, None, &live)
        .unwrap();
    let mut run = task(&program, &mut adapter, Some(1), ScriptRunMode::Install);
    let owner = adapter.clone();
    owner.invalidate_session(1).unwrap();
    let report = finish(&mut run, &mut adapter);
    assert!(matches!(
        report.items[0].outcome,
        ScriptItemOutcome::Failed(ScriptItemFailure::StaleAuthority)
    ));
    assert!(run.take_staged_results(&mut adapter).is_err());
}

#[test]
fn shared_raster_file_inputs_and_folder_codecs_use_the_common_decoder() {
    if !file_publication_available() {
        return;
    }
    use inkpod_format::CommonRasterFormat;
    let root = Directory::new();
    let input = core(906);
    for (extension, format) in [
        ("png", CommonRasterFormat::Png),
        ("tiff", CommonRasterFormat::Tiff),
        ("tga", CommonRasterFormat::Tga),
        ("bmp", CommonRasterFormat::Bmp),
    ] {
        let name = format!("A001.{extension}");
        let bytes = input.export_common_raster(format, false).unwrap();
        fs::write(root.0.join(&name), &bytes).unwrap();
        let program = program(
            "batch",
            &format!("file \"{name}\";"),
            &format!(
                "policy = folder; format = {extension}; folder = \"{extension}-out\"; naming_template = \"{{stem}}\";"
            ),
        );
        let mut adapter = adapter(&program, &root.0, 0);
        let mut run = task(&program, &mut adapter, None, ScriptRunMode::Install);
        let report = finish(&mut run, &mut adapter);
        assert_eq!(
            report.items[0].outcome,
            ScriptItemOutcome::Installed,
            "{extension}: {report:?}"
        );
        let decoded = adapter
            .manager()
            .read_image(
                &root.0.join(format!("{extension}-out/{name}")),
                &inkpod_io::JobContext::new(),
            )
            .unwrap();
        assert_eq!(decoded.raster().info.width, 3);
        assert_eq!(decoded.raster().info.height, 2);
        assert_eq!(fs::read(root.0.join(name)).unwrap(), bytes);
    }
}

#[test]
fn shared_new_tab_has_fresh_identity_and_no_source_history() {
    let root = Directory::new();
    let live = core(907);
    let original = live.document_info().unwrap();
    let program = program("batch", "current_document;", "policy = new_tabs;");
    let mut adapter = adapter(&program, &root.0, 2);
    adapter
        .capture_session(1, 1, 1, "active-document.inkpod".into(), 1, None, &live)
        .unwrap();
    let mut run = task(&program, &mut adapter, Some(1), ScriptRunMode::Install);
    let report = finish(&mut run, &mut adapter);
    assert_eq!(report.items[0].outcome, ScriptItemOutcome::Staged);
    let staged = run
        .take_staged_results(&mut adapter)
        .unwrap()
        .pop()
        .unwrap()
        .into_new_tab()
        .unwrap();
    let info = staged.document_info().unwrap();
    assert_ne!(info.document_uuid, original.document_uuid);
    assert!(info.dirty);
    assert!(staged.journal_entries().is_empty());
    assert_eq!(live.document_info().unwrap(), original);
}

#[test]
fn multi_item_folder_run_reuses_only_its_known_created_parent() {
    if !file_publication_available() {
        return;
    }
    let root = Directory::new();
    core(910).save(&root.0.join("A010.inkpod")).unwrap();
    core(911).save(&root.0.join("A002.inkpod")).unwrap();
    let program = program(
        "batch",
        "folder \".\";",
        "policy = folder; format = inkpod; folder = \"out\"; naming_template = \"{index:2}\";",
    );
    let mut adapter = adapter(&program, &root.0, 0);
    let mut run = task(&program, &mut adapter, None, ScriptRunMode::Install);
    let report = finish(&mut run, &mut adapter);
    assert_eq!(report.items.len(), 2);
    assert_eq!(report.items[0].input_label, "A002.inkpod");
    assert!(
        report
            .items
            .iter()
            .all(|item| item.outcome == ScriptItemOutcome::Installed),
        "{report:?}"
    );
    assert!(root.0.join("out/01.inkpod").exists());
    assert!(root.0.join("out/02.inkpod").exists());
}

#[test]
fn repeated_jobs_and_duplicate_active_inputs_allocate_distinct_new_document_identities() {
    let root = Directory::new();
    let mut live = core(920);
    live.set_grid(GridConfig {
        origin_x: 1,
        origin_y: 2,
        spacing_x: 8,
        spacing_y: 9,
        subdivisions: 2,
    })
    .unwrap();
    assert!(!live.journal_entries().is_empty());
    let program = program(
        "batch",
        "current_document; current_document;",
        "policy = new_tabs;",
    );
    let mut adapter = adapter(&program, &root.0, 2);
    adapter
        .capture_session(1, 1, 1, "active-document.inkpod".into(), 1, None, &live)
        .unwrap();
    let mut identities =
        std::collections::BTreeSet::from([live.document_info().unwrap().document_uuid]);
    for iteration in 0..2 {
        let mut run = task(&program, &mut adapter, Some(1), ScriptRunMode::Install);
        let report = finish(&mut run, &mut adapter);
        assert_eq!(report.items.len(), 2);
        assert!(
            report
                .items
                .iter()
                .all(|item| item.outcome == ScriptItemOutcome::Staged)
        );
        let staged = run.take_staged_results(&mut adapter).unwrap();
        assert_eq!(staged.len(), 2);
        for result in staged {
            let ordinal = result.ordinal();
            let mut new_tab = result.into_new_tab().unwrap();
            assert!(identities.insert(new_tab.document_info().unwrap().document_uuid));
            assert!(new_tab.journal_entries().is_empty());
            assert!(new_tab.document_info().unwrap().dirty);
            let digest = new_tab.document_state_digest().unwrap();
            new_tab.release_history_cache().unwrap();
            assert_eq!(
                new_tab
                    .verify_journal_replay()
                    .unwrap()
                    .document_state_digest(),
                digest
            );
            let path = root.0.join(format!("{iteration}-{ordinal}.inkpod"));
            new_tab.save(&path).unwrap();
            let mut reopened = Core::new();
            reopened.open(&path).unwrap();
            assert_eq!(reopened.document_state_digest().unwrap(), digest);
            reopened.release_history_cache().unwrap();
            assert_eq!(
                reopened
                    .verify_journal_replay()
                    .unwrap()
                    .document_state_digest(),
                digest
            );
        }
    }
    assert!(!live.journal_entries().is_empty());
}

#[test]
fn completed_new_tab_remains_takeable_when_a_later_item_is_cancelled() {
    let root = Directory::new();
    let live = core(921);
    let program = program(
        "batch",
        "current_document; current_document;",
        "policy = new_tabs;",
    );
    let mut adapter = adapter(&program, &root.0, 2);
    adapter
        .capture_session(1, 1, 1, "active-document.inkpod".into(), 1, None, &live)
        .unwrap();
    let mut run = task(&program, &mut adapter, Some(1), ScriptRunMode::Install);
    assert!(matches!(
        run.advance(&mut adapter, &mut || false),
        ScriptRunAdvance::ItemCompleted {
            ordinal: 0,
            outcome: ScriptItemOutcome::Staged,
            ..
        }
    ));
    run.advance(&mut adapter, &mut || true);
    let report = run.finish().unwrap();
    assert!(report.cancelled);
    assert_eq!(report.items[0].outcome, ScriptItemOutcome::Staged);
    let staged = run.take_staged_results(&mut adapter).unwrap();
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].ordinal(), 0);
}

#[test]
fn moved_job_created_subtree_does_not_replace_original_output_ancestor_authority() {
    if !file_publication_available() {
        return;
    }
    let container = Directory::new();
    let approved = container.0.join("approved");
    fs::create_dir(&approved).unwrap();
    let live = core(929);
    let program = program(
        "batch",
        "current_document; current_document;",
        "policy = folder; format = inkpod; folder = \"out/nested\"; naming_template = \"{index:2}\";",
    );
    let mut adapter = adapter(&program, &approved, 0);
    adapter
        .capture_session(1, 1, 1, "active.inkpod".into(), 1, None, &live)
        .unwrap();
    let mut run = task(&program, &mut adapter, Some(1), ScriptRunMode::Install);
    assert!(matches!(
        run.advance(&mut adapter, &mut || false),
        ScriptRunAdvance::ItemCompleted {
            outcome: ScriptItemOutcome::Installed,
            ..
        }
    ));
    let installed = fs::read(approved.join("out/nested/01.inkpod")).unwrap();
    let moved = container.0.join("moved");
    fs::rename(&approved, &moved).unwrap();
    fs::create_dir(&approved).unwrap();
    fs::rename(moved.join("out"), approved.join("out")).unwrap();
    let report = finish(&mut run, &mut adapter);
    assert!(
        matches!(report.items[1].outcome, ScriptItemOutcome::Failed(_)),
        "{report:?}"
    );
    assert_eq!(
        fs::read(approved.join("out/nested/01.inkpod")).unwrap(),
        installed
    );
    assert!(!approved.join("out/nested/02.inkpod").exists());
}

#[test]
fn prior_atomic_file_install_survives_later_stale_input_or_cancellation() {
    if !file_publication_available() {
        return;
    }
    for cancel in [false, true] {
        let root = Directory::new();
        core(922).save(&root.0.join("A001.inkpod")).unwrap();
        core(923).save(&root.0.join("A002.inkpod")).unwrap();
        let program = program(
            "batch",
            "file \"A001.inkpod\";file \"A002.inkpod\";",
            "policy = folder; format = inkpod; folder = \"out\"; naming_template = \"{index:2}\";",
        );
        let mut adapter = adapter(&program, &root.0, 0);
        let mut run = task(&program, &mut adapter, None, ScriptRunMode::Install);
        assert!(matches!(
            run.advance(&mut adapter, &mut || false),
            ScriptRunAdvance::ItemCompleted {
                ordinal: 0,
                outcome: ScriptItemOutcome::Installed,
                ..
            }
        ));
        let installed = fs::read(root.0.join("out/01.inkpod")).unwrap();
        if cancel {
            run.advance(&mut adapter, &mut || true);
        } else {
            core(924).save(&root.0.join("A002.inkpod")).unwrap();
            finish(&mut run, &mut adapter);
        }
        let report = run.finish().unwrap();
        assert_eq!(report.items[0].outcome, ScriptItemOutcome::Installed);
        if cancel {
            assert!(report.cancelled);
        } else {
            assert!(matches!(
                report.items[1].outcome,
                ScriptItemOutcome::Failed(_)
            ));
        }
        assert_eq!(fs::read(root.0.join("out/01.inkpod")).unwrap(), installed);
        assert!(!root.0.join("out/02.inkpod").exists());
    }
}

#[test]
fn planned_dry_results_preserve_canonical_identity_history_and_cache_free_replay() {
    let root = Directory::new();
    let mut live = core(925);
    live.set_grid(GridConfig {
        origin_x: 3,
        origin_y: 2,
        spacing_x: 7,
        spacing_y: 9,
        subdivisions: 2,
    })
    .unwrap();
    let digest = live.document_state_digest().unwrap();
    let history = live.journal_entries().to_vec();
    let info = live.document_info().unwrap();
    let program = program("canonical", "current_document;", "policy = new_tabs;");
    let mut adapter = adapter(&program, &root.0, 1);
    adapter
        .capture_session(1, 1, 1, "current-cell.inkpod".into(), 1, None, &live)
        .unwrap();
    let mut run = task(&program, &mut adapter, Some(1), ScriptRunMode::DryRun);
    finish(&mut run, &mut adapter);
    let mut results = run.take_dry_results().unwrap();
    assert_eq!(results.len(), 1);
    let (ordinal, result) = results.pop().unwrap();
    assert_eq!(ordinal, 0);
    let mut staged = result.into_staged();
    assert_eq!(staged.document_info().unwrap(), info);
    assert_eq!(staged.journal_entries(), history);
    staged.release_history_cache().unwrap();
    assert_eq!(
        staged
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        digest
    );
    assert!(run.take_staged_results(&mut adapter).unwrap().is_empty());
    assert!(run.take_dry_results().unwrap().is_empty());
}

#[test]
fn cancellation_after_parent_creation_reports_the_retained_directory() {
    if !file_publication_available() {
        return;
    }
    let root = Directory::new();
    core(926).save(&root.0.join("A001.inkpod")).unwrap();
    let program = program(
        "batch",
        "file \"A001.inkpod\";",
        "policy = folder; format = inkpod; folder = \"out/nested\"; naming_template = \"{stem}\";",
    );
    let mut adapter = adapter(&program, &root.0, 0);
    let mut run = task(&program, &mut adapter, None, ScriptRunMode::Install);
    loop {
        if matches!(
            run.advance(&mut adapter, &mut || root.0.join("out").exists()),
            ScriptRunAdvance::Complete
        ) {
            break;
        }
    }
    let report = run.finish().unwrap();
    assert!(report.cancelled);
    assert_eq!(report.created_directories.len(), 1);
    assert!(report.created_directories[0].ends_with("/out"));
    assert!(root.0.join("out").exists());
    assert!(!root.0.join("out/nested").exists());
}

#[test]
fn sequence_snapshot_runs_and_owner_recapture_invalidates_previous_plans() {
    let root = Directory::new();
    let live = core(929);
    let program = program("canonical", "current_sequence;", "policy = new_tabs;");
    let mut adapter = adapter(&program, &root.0, 1);
    adapter
        .capture_session(1, 1, 1, "A001.inkpod".into(), 1, None, &live)
        .unwrap();
    let snapshot =
        ScriptSessionSnapshot::capture(1, 1, 1, "A001.inkpod".into(), 1, None, &live).unwrap();
    adapter
        .capture_sequence(
            ScriptSequenceSnapshot::new(
                1,
                1,
                vec![ScriptSequenceMemberSnapshot::Session(snapshot.clone())],
            )
            .unwrap(),
        )
        .unwrap();
    let mut dry = task(&program, &mut adapter, Some(1), ScriptRunMode::DryRun);
    assert_eq!(
        finish(&mut dry, &mut adapter).items[0].outcome,
        ScriptItemOutcome::DryRun
    );
    let mut run = task(&program, &mut adapter, Some(1), ScriptRunMode::Install);
    let mut owner = adapter.clone();
    owner
        .capture_sequence(
            ScriptSequenceSnapshot::new(
                1,
                2,
                vec![ScriptSequenceMemberSnapshot::Session(snapshot)],
            )
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        finish(&mut run, &mut adapter).items[0].outcome,
        ScriptItemOutcome::Failed(ScriptItemFailure::StaleAuthority)
    ));
    let authority = adapter.authority(&program, Some(1), None).unwrap();
    assert!(
        plan_inkscript(
            &program,
            &authority,
            &mut adapter,
            &mut [],
            ScriptPlanLimits::exact_current(),
            &mut || false
        )
        .is_err()
    );
}

#[test]
fn shared_adapter_rejects_excessive_approved_path_count_before_observation() {
    let maximum = 2 * inkpod_format::MAX_INKSCRIPT_INPUTS
        + inkpod_format::MAX_INKSCRIPT_CONTAINER_ELEMENTS
        + 1;
    let paths = (1..=maximum + 1)
        .map(|id| (id as u64, PathBuf::from("/never-open")))
        .collect();
    assert!(matches!(
        ScriptIoAdapter::new(IoManager::new(IoConfig::default()).unwrap(), paths, 0),
        Err(ScriptPlanError::ResourceLimit)
    ));
}
