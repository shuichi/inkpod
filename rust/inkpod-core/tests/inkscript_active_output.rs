use inkpod_core::inkscript::abi_bridge::*;
use inkpod_core::inkscript::{
    InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId, ScriptIoAdapter,
    ScriptRunError, ScriptStagedResult, compile_inkscript,
};
use inkpod_core::*;
use inkpod_io::{IoConfig, IoManager};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "inkpod-active-output-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Core {
    let mut core = Core::new();
    let info = core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x4143_5449_5645)
        .unwrap();
    core.apply_fill_for_editor_target(
        &FillRequest {
            operation: FillOperation::Seed,
            seed_x: 0,
            seed_y: 0,
            color: PixelValue::Rgba([255, 0, 0, 255]),
            selection: None,
            use_document_selection: false,
            tolerance: 0,
            detached_regions: false,
            overflow_abort: false,
            gap_close: 0,
            transparent_only: false,
            inclusion_mode: InclusionMode::None,
            inclusion_colors: vec![],
            extension_distance: 0,
        },
        EditorTarget {
            layer_id: info.layer_id,
            plane_id: info.color_plane_id,
        },
        false,
        false,
    )
    .unwrap();
    core
}

fn staged(core: &Core, backing: Option<&Path>, noop: bool) -> ScriptStagedResult {
    let color = if noop {
        "rgba8(0,255,0,255)"
    } else {
        "rgba8(255,0,0,255)"
    };
    let text = format!(
        r#"inkscript 3;
requires {{ procedure_catalog = 8; replay_epoch = 29; }}
inputs {{ profile = batch; current_document; }}
program {{ step "erase" {{ enabled = true; invoke apply_batch_operations {{ operations = [
    {{ kind = erase; enabled = true; target = {{ kind = role; plane_kind = color; missing = error; }}; colors = [{color}]; }}
]; }}; }} }}
output {{ policy = active_document; }}
execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"#
    );
    let program = compile_inkscript(
        &InkScriptSource::new(InkScriptSourceId::new(9941), text.as_bytes()).unwrap(),
        InkScriptRunParameterDecision::Resolve(vec![]),
    )
    .unwrap();
    let mut adapter =
        ScriptIoAdapter::new(IoManager::new(IoConfig::default()).unwrap(), vec![], 0).unwrap();
    adapter
        .capture_session(
            1,
            2,
            3,
            "active-document.inkpod".into(),
            1,
            backing.map(Path::to_path_buf),
            core,
        )
        .unwrap();
    let authority = adapter.authority(&program, Some(1), None).unwrap();
    let plan = plan_inkscript(
        &program,
        &authority,
        &mut adapter,
        &mut [],
        ScriptPlanLimits::exact_current(),
        &mut || false,
    )
    .unwrap();
    let mut token = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
    let mut task = start_inkscript_run(
        &program,
        plan,
        &mut token,
        ScriptRunMode::Install,
        ScriptRunLimits::exact_current(),
    )
    .unwrap();
    while !matches!(
        task.advance(&mut adapter, &mut || false),
        ScriptRunAdvance::Complete
    ) {}
    let report = task.finish().unwrap();
    assert_eq!(
        report.items[0].outcome,
        ScriptItemOutcome::Staged,
        "{report:?}"
    );
    task.take_staged_results(&mut adapter).unwrap().remove(0)
}

fn native(core: &Core) -> inkpod_format::NativeFile {
    core.capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap()
        .0
}

fn observation(
    core: &Core,
) -> (
    DocumentInfo,
    DocumentStateDigest,
    EditorStateInfo,
    JournalState,
    Vec<JournalEntry>,
) {
    (
        core.document_info().unwrap(),
        core.document_state_digest().unwrap(),
        core.editor_state().unwrap(),
        core.journal_state().unwrap(),
        core.journal_entries().to_vec(),
    )
}

#[test]
fn active_result_checks_cancellation_at_publication_and_drop_leaves_source_unchanged() {
    for noop in [false, true] {
        let mut core = fixture();
        let before = observation(&core);
        let result = staged(&core, None, noop);
        assert_eq!(
            result.apply_active(&mut core, 1, 2, 3, &mut || true),
            Err(ScriptRunError::Cancelled)
        );
        assert_eq!(observation(&core), before);
        let result = staged(&core, None, noop);
        drop(result);
        assert_eq!(observation(&core), before);
    }
    let mut core = fixture();
    let before = observation(&core);
    let result = staged(&core, None, false);
    let mut checks = 0;
    assert_eq!(
        result.apply_active(&mut core, 1, 2, 3, &mut || {
            checks += 1;
            checks == 2
        }),
        Err(ScriptRunError::Cancelled)
    );
    assert_eq!(checks, 2);
    assert_eq!(observation(&core), before);
}

#[test]
fn active_result_is_one_undo_with_original_path_both_savepoints_and_cache_free_replay() {
    let dir = Directory::new();
    let path = dir.0.join("source.inkpod");
    let mut core = fixture();
    core.save(&path).unwrap();
    let editor = core.editor_state().unwrap();
    core.update_editor_state(
        editor.revision,
        EditorStateUpdate::SetActiveTool(EditorTool::Brush),
    )
    .unwrap();
    let before = observation(&core);
    let result = staged(&core, Some(&path), false);
    assert_eq!(observation(&core), before);
    let view_id = core.create_view().unwrap();
    let view = core
        .apply_view_for(
            view_id,
            ViewCommand::PanBy {
                device_dx: 4.0,
                device_dy: 7.0,
            },
        )
        .unwrap();
    result
        .apply_active(&mut core, 1, 2, 3, &mut || false)
        .unwrap();
    assert_eq!(core.build_snapshot_for(view_id).unwrap().view(), view);
    assert!(core.create_view().unwrap() > view_id);
    assert_eq!(core.journal_entries().len(), before.4.len() + 1);
    assert_eq!(
        core.journal_state().unwrap().savepoint_state_id(),
        before.3.savepoint_state_id()
    );
    assert_eq!(core.editor_state().unwrap(), before.2);
    assert_eq!(
        core.document_info().unwrap().document_uuid,
        before.0.document_uuid
    );
    let changed = core.document_state_digest().unwrap();
    assert_ne!(changed, before.1);
    core.undo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), before.1);
    assert_eq!(core.editor_state().unwrap(), before.2);
    core.redo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), changed);
    let mut file = native(&core);
    file.sections.retain(|section| section.fourcc != *b"CKPT");
    let reopened = Core::from_native_file(file, false).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), changed);
    assert_eq!(
        reopened.journal_state().unwrap(),
        core.journal_state().unwrap()
    );
    assert_eq!(
        reopened.editor_state().unwrap(),
        core.editor_state().unwrap()
    );
    core.revert().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), before.1);
}

#[test]
fn noop_active_preserves_complete_persistence_token_and_every_public_state() {
    let dir = Directory::new();
    let path = dir.0.join("source.inkpod");
    let mut core = fixture();
    core.save(&path).unwrap();
    let result = staged(&core, Some(&path), true);
    let token = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap()
        .1;
    let before = observation(&core);
    let rendered_before = core
        .build_snapshot()
        .tiles()
        .iter()
        .map(RenderTile::tile_revision)
        .collect::<Vec<_>>();
    result
        .apply_active(&mut core, 1, 2, 3, &mut || false)
        .unwrap();
    assert_eq!(observation(&core), before);
    assert_eq!(
        core.build_snapshot()
            .tiles()
            .iter()
            .map(RenderTile::tile_revision)
            .collect::<Vec<_>>(),
        rendered_before
    );
    core.validate_document_save(&token).unwrap();
    core.revert().unwrap();
}

#[test]
fn editor_only_change_is_stale_even_when_callers_reuse_generations() {
    let mut core = fixture();
    let result = staged(&core, None, false);
    let editor = core.editor_state().unwrap();
    core.update_editor_state(
        editor.revision,
        EditorStateUpdate::SetActiveTool(EditorTool::Brush),
    )
    .unwrap();
    let changed = observation(&core);
    assert_eq!(
        result.apply_active(&mut core, 1, 2, 3, &mut || false),
        Err(ScriptRunError::StaleInput)
    );
    assert_eq!(observation(&core), changed);
}

#[test]
fn save_only_authority_change_is_stale_even_without_document_or_editor_revision_change() {
    let dir = Directory::new();
    let path = dir.0.join("source.inkpod");
    let mut core = fixture();
    core.save(&path).unwrap();
    let result = staged(&core, Some(&path), false);
    let before = observation(&core);
    core.save(&path).unwrap();
    assert_eq!(observation(&core), before);
    assert_eq!(
        result.apply_active(&mut core, 1, 2, 3, &mut || false),
        Err(ScriptRunError::StaleInput)
    );
    assert_eq!(observation(&core), before);
}

#[test]
fn editor_savepoint_only_change_is_stale_and_clone_owner_cannot_receive_result() {
    let mut core = fixture();
    let editor = core.editor_state().unwrap();
    core.update_editor_state(
        editor.revision,
        EditorStateUpdate::SetActiveTool(EditorTool::Brush),
    )
    .unwrap();
    let result = staged(&core, None, false);
    core.commit_editor_savepoint(core.editor_savepoint_token().unwrap())
        .unwrap();
    let before = observation(&core);
    assert_eq!(
        result.apply_active(&mut core, 1, 2, 3, &mut || false),
        Err(ScriptRunError::StaleInput)
    );
    assert_eq!(observation(&core), before);
    let result = staged(&core, None, false);
    let mut unrelated = core.clone();
    assert_eq!(
        result.apply_active(&mut unrelated, 1, 2, 3, &mut || false),
        Err(ScriptRunError::StaleInput)
    );
    assert_eq!(observation(&unrelated), before);
}
