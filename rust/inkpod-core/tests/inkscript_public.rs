use inkpod_core::inkscript::{
    InkScriptExportError, InkScriptExportLimits, InkScriptExportPortability,
    InkScriptFragmentExport, InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId,
    ScriptCompileError, ScriptCompileLimits, ScriptRunError, ScriptStatementOutcome,
    capture_in_memory_fingerprint, capture_in_memory_input, capture_in_memory_input_at,
    compile_inkscript, compile_inkscript_with_limits, export_inkscript_fragment,
    export_inkscript_fragment_with_limits, run_inkscript_dry,
};
use inkpod_core::{
    AssetAlphaSemantics, AssetColorSpace, Core, DEFAULT_DPI_MILLI, GuideAxis, JournalEntry,
    JournalEventId, OutputColorGuardProfile, PixelFormat, PrimitiveRequest, RasterAssetInput,
    SelectionOperation,
};

fn source(text: &str) -> InkScriptSource {
    InkScriptSource::new(InkScriptSourceId::new(2300), text.as_bytes())
        .expect("public fixture must be valid UTF-8")
}

fn program_source() -> InkScriptSource {
    source(
        r#"inkscript 2;
requires { procedure_catalog = 5; replay_epoch = 27; }
inputs { current_document; }
program {
    step "Set grid" {
        enabled = true;
        invoke set_grid {
            grid = { origin_x = 1; origin_y = 2; spacing_x = 8; spacing_y = 9; subdivisions = 2; };
        };
    }
    step "Repeat grid" {
        enabled = true;
        invoke set_grid {
            grid = { origin_x = 1; origin_y = 2; spacing_x = 8; spacing_y = 9; subdivisions = 2; };
        };
    }
}
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "public"; start_number = 1; direction = ascending; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }
"#,
    )
}

fn core() -> Core {
    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .expect("fixture document must be created");
    core
}

fn defaults() -> InkScriptRunParameterDecision {
    InkScriptRunParameterDecision::Resolve(Vec::new())
}

fn assert_send_sync<T: Send + Sync>() {}
fn assert_send<T: Send>() {}

fn commit_events(core: &Core) -> Vec<JournalEventId> {
    core.journal_entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Commit(commit) => Some(commit.event_id()),
            JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => None,
        })
        .collect()
}

fn fragment_as_file(fragment: &str) -> InkScriptSource {
    let text = fragment
        .replacen("inkscript_fragment 2;", "inkscript 2;", 1)
        .replacen("program {", "inputs { current_document; }\nprogram {", 1);
    source(&format!(
        "{text}output {{ policy = duplicate; format = inkpod; folder = \"out\"; cell_folder = false; basename = \"export\"; start_number = 1; direction = ascending; }}\nexecution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}\n"
    ))
}

fn uuid_text(value: u128) -> String {
    let digits = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &digits[0..8],
        &digits[8..12],
        &digits[12..16],
        &digits[16..20],
        &digits[20..32]
    )
}

#[test]
fn public_compile_bind_and_staged_run_fail_closed() {
    assert_send_sync::<inkpod_core::inkscript::StaticScriptProgram>();
    assert_send_sync::<inkpod_core::inkscript::InMemoryInputFingerprint>();
    assert_send_sync::<InkScriptExportLimits>();
    assert_send_sync::<InkScriptFragmentExport>();
    assert_send::<inkpod_core::inkscript::ScriptDryRunResult>();

    let source_core = core();
    let before = source_core.document_state_digest().unwrap();
    let program = compile_inkscript(&program_source(), defaults()).unwrap();
    assert_eq!(program.budget().max_invocations(), 2);
    assert_ne!(program.static_compile_digest(), &[0; 32]);
    assert!(!program.path_intents().is_empty());

    let captured = capture_in_memory_input(&source_core).unwrap();
    let mut never_cancel = || false;
    let mut result = run_inkscript_dry(&program, captured, &mut never_cancel).unwrap();
    assert_eq!(result.report().commit_count(), 1);
    assert_eq!(
        result.report().statements(),
        &[
            ScriptStatementOutcome::Committed,
            ScriptStatementOutcome::NoOp
        ]
    );
    assert_eq!(source_core.document_state_digest().unwrap(), before);
    let after = result.staged().document_state_digest().unwrap();
    result.staged_mut().undo().unwrap();
    assert_eq!(result.staged().document_state_digest().unwrap(), before);
    result.staged_mut().redo().unwrap();
    assert_eq!(result.staged().document_state_digest().unwrap(), after);
    result.staged_mut().release_history_cache().unwrap();
    assert_eq!(
        result
            .staged()
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        after
    );

    assert_eq!(
        compile_inkscript(
            &source("inkscript 2; requires { procedure_catalog = 5; replay_epoch = 27; }"),
            defaults(),
        ),
        Err(ScriptCompileError::Syntax)
    );
    let old_catalog = source(
        &program_source()
            .text()
            .replace("procedure_catalog = 5", "procedure_catalog = 1"),
    );
    assert_eq!(
        compile_inkscript(&old_catalog, defaults()),
        Err(ScriptCompileError::Envelope(
            inkpod_format::InkScriptEnvelopeErrorCode::NonCurrentProcedureCatalog
        ))
    );
    assert_eq!(
        compile_inkscript_with_limits(
            &program_source(),
            defaults(),
            ScriptCompileLimits::exact_current().with_invocations(1),
        ),
        Err(ScriptCompileError::ResourceLimit)
    );

    let mut cancel = || true;
    assert_eq!(
        run_inkscript_dry(
            &program,
            capture_in_memory_input(&source_core).unwrap(),
            &mut cancel,
        )
        .unwrap_err(),
        ScriptRunError::Cancelled
    );

    let mut stale_core = core();
    let fingerprint = capture_in_memory_fingerprint(&stale_core).unwrap();
    stale_core.add_guide(GuideAxis::Vertical, 3).unwrap();
    let stale_digest = stale_core.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_dry(
            &program,
            capture_in_memory_input_at(&stale_core, fingerprint),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::StaleInput
    );
    assert_eq!(stale_core.document_state_digest().unwrap(), stale_digest);
}

#[test]
fn journal_export_round_trips_a_linear_selection_with_typed_results() {
    let mut base = core();
    base.set_grid(inkpod_core::GridConfig {
        origin_x: 1,
        origin_y: 1,
        spacing_x: 4,
        spacing_y: 4,
        subdivisions: 1,
    })
    .unwrap();
    base.release_history_cache().unwrap();
    base.verify_journal_replay().unwrap();

    let text = r#"inkscript 2;
requires { procedure_catalog = 5; replay_epoch = 27; }
inputs { current_document; }
program {
    step "Add guide" as created { enabled = true; invoke add_guide { axis = vertical; position = 2; }; }
    step "Move guide" { enabled = true; invoke move_guide { guide_id = $created.guide; position = 3; }; }
}
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "source"; start_number = 1; direction = ascending; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }
"#;
    let program = compile_inkscript(&source(text), defaults()).unwrap();
    let mut never_cancel = || false;
    let source_result = run_inkscript_dry(
        &program,
        capture_in_memory_input(&base).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    let selected = commit_events(source_result.staged())
        .into_iter()
        .skip(1)
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 2);

    let before_digest = source_result.staged().document_state_digest().unwrap();
    let before_info = source_result.staged().document_info().unwrap();
    let before_journal = source_result.staged().journal_entries().to_vec();
    let exported =
        export_inkscript_fragment(source_result.staged(), &selected, &mut never_cancel).unwrap();
    assert!(exported.text().contains("assert document"));
    assert!(exported.text().contains("$step_1.guide"));
    assert_eq!(exported.commit_count(), 2);
    let base_state = match &before_journal[0] {
        JournalEntry::Commit(commit) => commit.committed_state_id(),
        JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => unreachable!(),
    };
    assert_eq!(exported.base_state_id(), base_state);
    assert_eq!(
        source_result.staged().document_state_digest().unwrap(),
        before_digest
    );
    assert_eq!(source_result.staged().document_info().unwrap(), before_info);
    assert_eq!(source_result.staged().journal_entries(), before_journal);

    let replay_program = compile_inkscript(&fragment_as_file(exported.text()), defaults()).unwrap();
    let replay = run_inkscript_dry(
        &replay_program,
        capture_in_memory_input(&base).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        replay.staged().document_state_digest().unwrap(),
        source_result.staged().document_state_digest().unwrap()
    );
    assert_eq!(
        replay.report().next_stable_id(),
        source_result.report().next_stable_id()
    );
    let result_identity = |values: &[inkpod_core::inkscript::ScriptResultValue]| {
        values
            .iter()
            .map(|value| {
                (
                    value.field().to_owned(),
                    value.output_id_ordinal(),
                    value.persistent_id(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        result_identity(replay.report().results()),
        result_identity(source_result.report().results())
    );

    let expected = source_result
        .staged()
        .journal_entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Commit(commit) if selected.contains(&commit.event_id()) => {
                Some(commit.procedure().clone())
            }
            JournalEntry::Commit(_) | JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => {
                None
            }
        })
        .collect::<Vec<_>>();
    let actual = replay
        .staged()
        .journal_entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Commit(commit) if commit.event_id().get() > 1 => Some(commit.procedure()),
            JournalEntry::Commit(_) | JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_eq!(actual.primitive_id(), expected.primitive_id());
        assert_eq!(
            actual.primitive_schema_version(),
            expected.primitive_schema_version()
        );
        assert_eq!(actual.pre_state_digest(), expected.pre_state_digest());
        assert_eq!(actual.post_state_digest(), expected.post_state_digest());
        assert_eq!(actual.input_ids(), expected.input_ids());
        assert_eq!(actual.output_ids(), expected.output_ids());
        assert_eq!(actual.asset_ids(), expected.asset_ids());
        assert_eq!(actual.canonical_arguments(), expected.canonical_arguments());
        assert_eq!(actual.canonical_payload(), expected.canonical_payload());
    }
}

#[test]
fn journal_export_accepts_inactive_commits_and_rejects_invalid_selection_atomically() {
    let mut core = core();
    core.add_guide(GuideAxis::Vertical, 1).unwrap();
    core.add_guide(GuideAxis::Horizontal, 2).unwrap();
    let inactive = commit_events(&core)[1];
    core.undo().unwrap();
    let movement = core
        .journal_entries()
        .iter()
        .find_map(|entry| match entry {
            JournalEntry::HistoryMove(value) => Some(value.event_id()),
            JournalEntry::Commit(_) | JournalEntry::BranchCut(_) => None,
        })
        .unwrap();
    core.add_guide(GuideAxis::Vertical, 3).unwrap();
    let active = *commit_events(&core).last().unwrap();
    let before = (
        core.document_state_digest().unwrap(),
        core.document_info().unwrap(),
        core.journal_entries().to_vec(),
        core.journal_state().unwrap(),
        core.editor_state().unwrap(),
        core.persistence_info().unwrap(),
        core.resource_usage(),
    );
    let mut allocator_control = core.clone();

    let mut never_cancel = || false;
    let inactive_export = export_inkscript_fragment(&core, &[inactive], &mut never_cancel).unwrap();
    assert_eq!(inactive_export.commit_count(), 1);
    let active_export = export_inkscript_fragment(&core, &[active], &mut never_cancel).unwrap();
    assert_eq!(active_export.commit_count(), 1);
    assert_eq!(
        export_inkscript_fragment(&core, &[inactive, active], &mut never_cancel).unwrap_err(),
        InkScriptExportError::NonLinearSelection
    );
    assert_eq!(
        export_inkscript_fragment(&core, &[movement], &mut never_cancel).unwrap_err(),
        InkScriptExportError::NotACommit(movement)
    );
    assert_eq!(
        export_inkscript_fragment(&core, &[], &mut never_cancel).unwrap_err(),
        InkScriptExportError::EmptySelection
    );
    assert_eq!(
        export_inkscript_fragment_with_limits(
            &core,
            &[inactive, active],
            InkScriptExportLimits::exact_current().with_commits(1),
            &mut never_cancel,
        )
        .unwrap_err(),
        InkScriptExportError::ResourceLimit
    );
    let mut cancel = || true;
    assert_eq!(
        export_inkscript_fragment(&core, &[inactive], &mut cancel).unwrap_err(),
        InkScriptExportError::Cancelled
    );
    assert_eq!(core.document_state_digest().unwrap(), before.0);
    assert_eq!(core.document_info().unwrap(), before.1);
    assert_eq!(core.journal_entries(), before.2);
    assert_eq!(core.journal_state().unwrap(), before.3);
    assert_eq!(core.editor_state().unwrap(), before.4);
    assert_eq!(core.persistence_info().unwrap(), before.5);
    assert_eq!(core.resource_usage(), before.6);
    let (_, expected_next_id) = allocator_control
        .add_guide(GuideAxis::Horizontal, 7)
        .unwrap();
    let (_, actual_next_id) = core.add_guide(GuideAxis::Horizontal, 7).unwrap();
    assert_eq!(actual_next_id, expected_next_id);
}

#[test]
fn journal_export_replays_from_genesis_and_emits_external_strict_bindings() {
    let mut source_core = Core::new();
    source_core
        .new_cell_with_uuid(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x24a0)
        .unwrap();
    let (_, guide_id) = source_core.add_guide(GuideAxis::Vertical, 2).unwrap();
    let first_commit = commit_events(&source_core)[0];
    let mut never_cancel = || false;
    let genesis_export =
        export_inkscript_fragment(&source_core, &[first_commit], &mut never_cancel).unwrap();
    let genesis_program =
        compile_inkscript(&fragment_as_file(genesis_export.text()), defaults()).unwrap();
    let mut genesis = Core::new();
    genesis
        .new_cell_with_uuid(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x24a0)
        .unwrap();
    let genesis_replay = run_inkscript_dry(
        &genesis_program,
        capture_in_memory_input(&genesis).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        genesis_replay.staged().document_state_digest().unwrap(),
        source_core.document_state_digest().unwrap()
    );

    let mut parent = Core::new();
    parent
        .new_cell_with_uuid(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x24a0)
        .unwrap();
    let (_, parent_guide) = parent.add_guide(GuideAxis::Vertical, 2).unwrap();
    assert_eq!(parent_guide, guide_id);
    source_core.move_guide(guide_id, 4).unwrap();
    let move_commit = commit_events(&source_core)[1];
    let external =
        export_inkscript_fragment(&source_core, &[move_commit], &mut never_cancel).unwrap();
    assert_eq!(
        external.portability(),
        InkScriptExportPortability::RequiresBinding
    );
    assert!(external.text().contains("bindings {"));
    assert!(
        external
            .text()
            .contains(&format!("persistent_id = {guide_id};")),
        "{}",
        external.text()
    );
    let external_program =
        compile_inkscript(&fragment_as_file(external.text()), defaults()).unwrap();
    let external_replay = run_inkscript_dry(
        &external_program,
        capture_in_memory_input(&parent).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        external_replay.staged().document_state_digest().unwrap(),
        source_core.document_state_digest().unwrap()
    );
    assert_eq!(
        export_inkscript_fragment_with_limits(
            &source_core,
            &[move_commit],
            InkScriptExportLimits::exact_current().with_source_bytes(1),
            &mut never_cancel,
        )
        .unwrap_err(),
        InkScriptExportError::ResourceLimit
    );
}

#[test]
fn journal_export_embeds_exact_retained_raster_assets_with_bounded_failure() {
    let mut source_core = Core::new();
    let document = source_core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x24a1)
        .unwrap();
    source_core
        .execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            raster: RasterAssetInput {
                width: 2,
                height: 1,
                pixel_format: PixelFormat::StraightRgba8,
                color_space: Some(AssetColorSpace::Srgb),
                alpha_semantics: AssetAlphaSemantics::Straight,
                canonical_stride: 8,
                pixels: vec![1, 2, 3, 255, 4, 5, 6, 128],
                expected_id: None,
            },
        })
        .unwrap();
    let event = commit_events(&source_core)[0];
    let mut never_cancel = || false;
    let exported = export_inkscript_fragment(&source_core, &[event], &mut never_cancel).unwrap();
    assert!(exported.text().contains("asset asset_1"));
    assert!(exported.text().contains("data = base64"));

    let program = compile_inkscript(&fragment_as_file(exported.text()), defaults()).unwrap();
    assert_eq!(program.budget().max_asset_bytes(), 8);
    assert_eq!(
        export_inkscript_fragment_with_limits(
            &source_core,
            &[event],
            InkScriptExportLimits::exact_current().with_asset_bytes(7),
            &mut never_cancel,
        )
        .unwrap_err(),
        InkScriptExportError::ResourceLimit
    );
}

#[test]
fn journal_export_uses_schema_role_indices_for_deleted_intermediate_outputs() {
    let mut base = Core::new();
    base.new_cell_with_uuid(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x24a2)
        .unwrap();
    let info = base.document_info().unwrap();
    let script = format!(
        r#"inkscript 2;
requires {{ procedure_catalog = 5; replay_epoch = 27; }}
inputs {{ current_document; }}
bindings {{
    let target = select layer {{ source_document_uuid = uuid"{}"; persistent_id = {}; }};
}}
program {{
    step "Duplicate" as copies {{
        enabled = true;
        invoke edit_targets {{ targets = [layer_target($target)]; command = duplicate_targets(); }};
    }}
    step "Delete duplicate" {{
        enabled = true;
        invoke delete_layer {{ layer_id = $copies.layers[0]; }};
    }}
}}
output {{ policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "roles"; start_number = 1; direction = ascending; }}
execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}
"#,
        uuid_text(info.document_uuid),
        info.layer_id
    );
    let program = compile_inkscript(&source(&script), defaults()).unwrap();
    let mut never_cancel = || false;
    let original = run_inkscript_dry(
        &program,
        capture_in_memory_input(&base).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    let selected = commit_events(original.staged());
    assert_eq!(selected.len(), 2);
    let exported =
        export_inkscript_fragment(original.staged(), &selected, &mut never_cancel).unwrap();
    assert!(exported.text().contains("$step_1.layers[0]"));

    let replay_program = compile_inkscript(&fragment_as_file(exported.text()), defaults()).unwrap();
    let replay = run_inkscript_dry(
        &replay_program,
        capture_in_memory_input(&base).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        replay.staged().document_state_digest().unwrap(),
        original.staged().document_state_digest().unwrap()
    );
    assert_eq!(
        replay.report().next_stable_id(),
        original.report().next_stable_id()
    );
}

#[test]
fn journal_export_reports_strict_source_only_catalog_steps() {
    let raster = || RasterAssetInput {
        width: 1,
        height: 1,
        pixel_format: PixelFormat::StraightRgba8,
        color_space: Some(AssetColorSpace::Srgb),
        alpha_semantics: AssetAlphaSemantics::Straight,
        canonical_stride: 4,
        pixels: vec![0, 255, 0, 255],
        expected_id: None,
    };
    let mut source_core = Core::new();
    let document = source_core
        .new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x24a3)
        .unwrap();
    source_core
        .execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            raster: raster(),
        })
        .unwrap();
    let guard_revision = source_core.document_info().unwrap().document_revision;
    source_core
        .select_output_color_guard(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            guard_revision,
        )
        .unwrap();
    let event = commit_events(&source_core)[1];
    let mut never_cancel = || false;
    let exported = export_inkscript_fragment(&source_core, &[event], &mut never_cancel).unwrap();
    assert_eq!(
        exported.portability(),
        InkScriptExportPortability::StrictSourceOnly
    );
    assert!(
        exported
            .required_preconditions()
            .contains(&"exact_document_revision".to_owned())
    );

    let mut base = Core::new();
    let document = base
        .new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x24a3)
        .unwrap();
    base.execute_primitive(PrimitiveRequest::ImportRasterAsset {
        expected_revision: document.document_revision,
        target_plane_id: document.color_plane_id,
        raster: raster(),
    })
    .unwrap();
    let replay_program = compile_inkscript(&fragment_as_file(exported.text()), defaults()).unwrap();
    let replay = run_inkscript_dry(
        &replay_program,
        capture_in_memory_input(&base).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        replay.staged().document_state_digest().unwrap(),
        source_core.document_state_digest().unwrap()
    );
}
