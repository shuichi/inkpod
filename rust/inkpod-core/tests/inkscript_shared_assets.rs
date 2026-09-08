use inkpod_core::inkscript::abi_bridge::*;
use inkpod_core::inkscript::{
    InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId, ScriptAssetError,
    ScriptIoAdapter, ScriptIoSequenceInput, ScriptStatementOutcome, StaticScriptProgram,
    compile_inkscript,
};
use inkpod_core::{
    AssetAlphaSemantics, AssetColorSpace, Core, DEFAULT_DPI_MILLI, GridConfig, PixelFormat,
    PrimitiveRequest, RasterAssetInput,
};
use inkpod_io::{IoConfig, IoManager};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);
const PIXELS: [u8; 4] = [1, 2, 3, 255];

struct Fixture {
    directory: PathBuf,
    program: StaticScriptProgram,
    source: Core,
    expected: Core,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "inkpod-shared-assets-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("paint.bin"), PIXELS).unwrap();
        let mut source = Core::new();
        let info = source
            .new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x5a551)
            .unwrap();
        let mut expected = Core::new();
        expected
            .new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x5a551)
            .unwrap();
        expected
            .execute_primitive(PrimitiveRequest::ImportRasterAsset {
                expected_revision: info.document_revision,
                target_plane_id: info.color_plane_id,
                raster: RasterAssetInput {
                    width: 1,
                    height: 1,
                    pixel_format: PixelFormat::StraightRgba8,
                    color_space: Some(AssetColorSpace::Srgb),
                    alpha_semantics: AssetAlphaSemantics::Straight,
                    canonical_stride: 4,
                    pixels: PIXELS.to_vec(),
                    expected_id: None,
                },
            })
            .unwrap();
        let asset = expected.asset_infos().last().unwrap().id;
        let digest: String = asset
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let text = format!(
            r#"inkscript 3;
requires {{ procedure_catalog = 8; replay_epoch = 29; }}
inputs {{ current_document; }}
bindings {{ let paint = select plane {{ source_document_uuid = uuid"00000000-0000-0000-0000-00000005a551"; persistent_id = {}; }}; }}
program {{
step "Import" {{ enabled = true; invoke import_raster_asset {{ plane_id = $paint; raster = asset(paint_asset); }}; }}
step "Import no-op" {{ enabled = true; invoke import_raster_asset {{ plane_id = $paint; raster = asset(paint_asset); }}; }}
}}
output {{ policy = new_tabs; }}
execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}
assets {{ asset paint_asset {{
kind = "canonical_raster"; asset_id = blake3"{digest}";
descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 4; element_count = 1; }};
data_file = "paint.bin";
}}; }}"#,
            info.color_plane_id
        );
        let program = compile_inkscript(
            &InkScriptSource::new(InkScriptSourceId::new(505), text.as_bytes()).unwrap(),
            InkScriptRunParameterDecision::Resolve(vec![]),
        )
        .unwrap();
        Self {
            directory,
            program,
            source,
            expected,
        }
    }

    fn adapter(&self, config: IoConfig) -> ScriptIoAdapter {
        let mut adapter = ScriptIoAdapter::new(
            IoManager::new(config).unwrap(),
            self.program
                .path_intents()
                .iter()
                .map(|intent| (intent.id(), self.directory.join(intent.text())))
                .collect(),
            1,
        )
        .unwrap();
        adapter
            .capture_session(17, 3, 5, "current.inkpod".into(), 1, None, &self.source)
            .unwrap();
        adapter
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn shared_asset_plan_freezes_bytes_and_preserves_issue_time_context() {
    let fixture = Fixture::new();
    let before = fixture.source.document_state_digest().unwrap();
    let mut adapter = fixture.adapter(IoConfig::default());
    let plan = adapter
        .plan(
            &fixture.program,
            Some(17),
            None,
            ScriptPlanLimits::exact_current(),
            &mut || false,
        )
        .unwrap();
    assert_eq!(
        plan.command_context().current_session_identity(),
        Some((17, 3, 5))
    );
    fs::remove_file(fixture.directory.join("paint.bin")).unwrap();
    let mut confirmation = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
    let mut task = start_inkscript_run(
        &fixture.program,
        plan,
        &mut confirmation,
        ScriptRunMode::DryRun,
        ScriptRunLimits::exact_current(),
    )
    .unwrap();
    while !matches!(
        task.advance(&mut adapter, &mut || false),
        ScriptRunAdvance::Complete
    ) {}
    assert_eq!(
        task.finish().unwrap().items[0].outcome,
        ScriptItemOutcome::DryRun
    );
    let mut results = task.take_dry_results().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].1.report().statements(),
        &[
            ScriptStatementOutcome::Committed,
            ScriptStatementOutcome::NoOp
        ]
    );
    assert_eq!(results[0].1.report().commit_count(), 1);
    assert_eq!(
        results[0].1.staged().document_state_digest().unwrap(),
        fixture.expected.document_state_digest().unwrap()
    );
    assert_eq!(fixture.source.document_state_digest().unwrap(), before);
    assert!(task.take_dry_results().unwrap().is_empty());
    let mut staged = results.pop().unwrap().1.into_staged();
    let after = staged.document_state_digest().unwrap();
    staged.undo().unwrap();
    assert_eq!(staged.document_state_digest().unwrap(), before);
    staged.redo().unwrap();
    assert_eq!(staged.document_state_digest().unwrap(), after);
    let output = fixture.directory.join("frozen.inkpod");
    staged.save(&output).unwrap();
    let mut reopened = Core::new();
    reopened.open(&output).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), after);
    assert_eq!(
        reopened
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        after
    );
}

#[test]
fn shared_asset_plan_rejects_bad_payload_missing_authority_limits_and_cancel() {
    let fixture = Fixture::new();
    let before = fixture.source.document_state_digest().unwrap();
    let mut adapter = fixture.adapter(IoConfig::default());
    assert_eq!(
        adapter
            .plan(
                &fixture.program,
                Some(17),
                None,
                ScriptPlanLimits::exact_current(),
                &mut || true
            )
            .unwrap_err(),
        ScriptPlanError::Cancelled
    );
    let mut absent =
        ScriptIoAdapter::new(IoManager::new(IoConfig::default()).unwrap(), vec![], 1).unwrap();
    assert_eq!(
        absent
            .plan(
                &fixture.program,
                None,
                None,
                ScriptPlanLimits::exact_current(),
                &mut || false
            )
            .unwrap_err(),
        ScriptPlanError::AuthorityMismatch
    );
    fs::write(fixture.directory.join("paint.bin"), [4, 3, 2, 1]).unwrap();
    assert_eq!(
        adapter
            .plan(
                &fixture.program,
                Some(17),
                None,
                ScriptPlanLimits::exact_current(),
                &mut || false
            )
            .unwrap_err(),
        ScriptPlanError::Asset(ScriptAssetError::DigestMismatch)
    );
    fs::write(fixture.directory.join("paint.bin"), PIXELS).unwrap();
    let mut limited = fixture.adapter(IoConfig {
        max_encoded_bytes: 3,
        ..IoConfig::default()
    });
    assert_eq!(
        limited
            .plan(
                &fixture.program,
                Some(17),
                None,
                ScriptPlanLimits::exact_current(),
                &mut || false
            )
            .unwrap_err(),
        ScriptPlanError::ResourceLimit
    );
    assert_eq!(fixture.source.document_state_digest().unwrap(), before);
}

#[test]
fn shared_asset_plan_rejects_revocation_during_ingestion() {
    let fixture = Fixture::new();
    let mut adapter = fixture.adapter(IoConfig::default());
    let invalidator = adapter.clone();
    let mut polls = 0;
    let result = adapter.plan(
        &fixture.program,
        Some(17),
        None,
        ScriptPlanLimits::exact_current(),
        &mut || {
            polls += 1;
            if polls == 2 {
                invalidator.invalidate_authority().unwrap();
            }
            false
        },
    );
    assert_eq!(result.unwrap_err(), ScriptPlanError::StaleAuthority);
}

#[test]
fn shared_sequence_inputs_register_file_paths_without_partial_capture() {
    let mut fixture = Fixture::new();
    let path = fixture.directory.join("input.inkpod");
    fixture.source.save(&path).unwrap();
    let text = "inkscript 3; requires { procedure_catalog = 8; replay_epoch = 29; } inputs { current_sequence; } program {} output { policy = new_tabs; } execution { failure = stop; wait_ms = 0; preview_before_save = false; }";
    let program = compile_inkscript(
        &InkScriptSource::new(InkScriptSourceId::new(506), text.as_bytes()).unwrap(),
        InkScriptRunParameterDecision::Resolve(vec![]),
    )
    .unwrap();
    let mut adapter = fixture.adapter(IoConfig::default());
    adapter
        .capture_sequence_inputs(
            42,
            9,
            &[ScriptIoSequenceInput::File {
                path: path.clone(),
                source_generation: 4,
            }],
            &mut || false,
        )
        .unwrap();
    let original_digest = adapter
        .plan(
            &program,
            None,
            None,
            ScriptPlanLimits::exact_current(),
            &mut || false,
        )
        .unwrap()
        .plan_digest();
    assert_eq!(
        adapter
            .capture_sequence_inputs(42, 10, &[ScriptIoSequenceInput::Session(17)], &mut || true),
        Err(ScriptPlanError::Cancelled)
    );
    assert_eq!(
        adapter.capture_sequence_inputs(
            42,
            10,
            &[
                ScriptIoSequenceInput::Session(17),
                ScriptIoSequenceInput::Session(17)
            ],
            &mut || false
        ),
        Err(ScriptPlanError::InvalidInput)
    );
    assert_eq!(
        adapter.capture_sequence_inputs(
            42,
            10,
            &[ScriptIoSequenceInput::File {
                path: "relative.inkpod".into(),
                source_generation: 1
            }],
            &mut || false
        ),
        Err(ScriptPlanError::AuthorityMismatch)
    );
    assert_eq!(
        adapter.capture_sequence_inputs(
            42,
            10,
            &vec![ScriptIoSequenceInput::Session(17); inkpod_format::MAX_INKSCRIPT_INPUTS + 1],
            &mut || false
        ),
        Err(ScriptPlanError::ResourceLimit)
    );
    let plan = adapter
        .plan(
            &program,
            None,
            None,
            ScriptPlanLimits::exact_current(),
            &mut || false,
        )
        .unwrap();
    assert_eq!(plan.input_count(), 1);
    assert_eq!(plan.plan_digest(), original_digest);
    assert_eq!(plan.command_context().current_session_identity(), None);
    let mut confirmation = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
    let mut task = start_inkscript_run(
        &program,
        plan,
        &mut confirmation,
        ScriptRunMode::DryRun,
        ScriptRunLimits::exact_current(),
    )
    .unwrap();
    while !matches!(
        task.advance(&mut adapter, &mut || false),
        ScriptRunAdvance::Complete
    ) {}
    assert_eq!(
        task.finish().unwrap().items[0].outcome,
        ScriptItemOutcome::DryRun
    );
}

#[test]
fn owned_session_capture_uses_core_backing_for_dirty_input_and_overwrite_rejection() {
    let mut fixture = Fixture::new();
    let path = fixture.directory.join("owned.inkpod");
    fixture.source.save(&path).unwrap();
    let disk = fs::read(&path).unwrap();
    fixture
        .source
        .set_grid(GridConfig {
            origin_x: 1,
            origin_y: 2,
            spacing_x: 8,
            spacing_y: 9,
            subdivisions: 2,
        })
        .unwrap();
    let expected = fixture.source.document_state_digest().unwrap();
    for overwrite in [false, true] {
        let output = if overwrite {
            "policy = explicit_overwrite; format = inkpod;"
        } else {
            "policy = new_tabs;"
        };
        let text = format!(
            "inkscript 3; requires {{ procedure_catalog = 8; replay_epoch = 29; }} inputs {{ file \"owned.inkpod\"; }} program {{}} output {{{output}}} execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"
        );
        let program = compile_inkscript(
            &InkScriptSource::new(InkScriptSourceId::new(507), text.as_bytes()).unwrap(),
            InkScriptRunParameterDecision::Resolve(vec![]),
        )
        .unwrap();
        let mut adapter = ScriptIoAdapter::new(
            IoManager::new(IoConfig::default()).unwrap(),
            program
                .path_intents()
                .iter()
                .map(|intent| (intent.id(), path.clone()))
                .collect(),
            1,
        )
        .unwrap();
        adapter
            .capture_session_from_core(17, 3, 5, "open-session.inkpod".into(), 1, &fixture.source)
            .unwrap();
        let plan = adapter.plan(
            &program,
            None,
            None,
            ScriptPlanLimits::exact_current(),
            &mut || false,
        );
        if overwrite {
            assert_eq!(plan.unwrap_err(), ScriptPlanError::OpenSessionOverwrite);
            continue;
        }
        let plan = plan.unwrap();
        assert_eq!(plan.preview_items()[0].display_label(), "owned.inkpod");
        let mut confirmation = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
        let mut task = start_inkscript_run(
            &program,
            plan,
            &mut confirmation,
            ScriptRunMode::DryRun,
            ScriptRunLimits::exact_current(),
        )
        .unwrap();
        while !matches!(
            task.advance(&mut adapter, &mut || false),
            ScriptRunAdvance::Complete
        ) {}
        let report = task.finish().unwrap();
        assert_eq!(report.items[0].outcome, ScriptItemOutcome::DryRun);
        assert_eq!(
            report.items[0]
                .execution
                .as_ref()
                .unwrap()
                .final_state_digest(),
            expected
        );
    }
    assert_eq!(fs::read(&path).unwrap(), disk);
    assert_eq!(fixture.source.document_state_digest().unwrap(), expected);
}

#[test]
fn shared_file_plan_propagates_cancellation_from_manager_read() {
    let mut fixture = Fixture::new();
    let path = fixture.directory.join("input.inkpod");
    fixture.source.save(&path).unwrap();
    let bytes = fs::read(&path).unwrap();
    let text = "inkscript 3; requires { procedure_catalog = 8; replay_epoch = 29; } inputs { file \"input.inkpod\"; } program {} output { policy = new_tabs; } execution { failure = stop; wait_ms = 0; preview_before_save = false; }";
    let program = compile_inkscript(
        &InkScriptSource::new(InkScriptSourceId::new(508), text.as_bytes()).unwrap(),
        InkScriptRunParameterDecision::Resolve(vec![]),
    )
    .unwrap();
    let manager = IoManager::new(IoConfig::default()).unwrap();
    let mut adapter = ScriptIoAdapter::new(
        manager.clone(),
        program
            .path_intents()
            .iter()
            .map(|intent| (intent.id(), path.clone()))
            .collect(),
        1,
    )
    .unwrap();
    let result = adapter.plan(
        &program,
        None,
        None,
        ScriptPlanLimits::exact_current(),
        &mut || manager.cache_stats().encoded_bytes != 0,
    );
    assert_eq!(result.unwrap_err(), ScriptPlanError::Cancelled);
    let stats = manager.cache_stats();
    assert_eq!((stats.encoded_bytes, stats.physical_reads), (0, 0));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let fingerprint =
        ScriptPlanAdapter::resolve_file(&mut adapter, program.path_intents()[0].id(), &mut || {
            false
        })
        .unwrap();
    manager.clear_cache();
    let before_reads = manager.cache_stats().physical_reads;
    assert!(matches!(
        ScriptRunAdapter::read_native(&mut adapter, &fingerprint, &mut || manager
            .cache_stats()
            .encoded_bytes
            != 0),
        Err(ScriptRunAdapterError::Cancelled)
    ));
    let stats = manager.cache_stats();
    assert_eq!(
        (stats.encoded_bytes, stats.physical_reads),
        (0, before_reads)
    );
}

#[test]
fn backing_validation_allows_document_edits_and_rejects_save_as_or_replacement() {
    let mut fixture = Fixture::new();
    let path = fixture.directory.join("original.inkpod");
    fixture.source.save(&path).unwrap();
    let mut adapter = fixture.adapter(IoConfig::default());
    adapter
        .capture_session_from_core(17, 3, 5, "fallback.inkpod".into(), 1, &fixture.source)
        .unwrap();
    assert!(
        adapter
            .validate_captured_session_backing(17, &fixture.source)
            .unwrap()
    );
    fixture
        .source
        .set_grid(GridConfig {
            origin_x: 1,
            origin_y: 2,
            spacing_x: 8,
            spacing_y: 9,
            subdivisions: 2,
        })
        .unwrap();
    assert!(
        adapter
            .validate_captured_session_backing(17, &fixture.source)
            .unwrap()
    );
    fixture.source.undo().unwrap();
    assert!(
        adapter
            .validate_captured_session_backing(17, &fixture.source)
            .unwrap()
    );
    fixture.source.redo().unwrap();
    assert!(
        adapter
            .validate_captured_session_backing(17, &fixture.source)
            .unwrap()
    );
    let other_owner = fixture.source.clone();
    assert!(
        !adapter
            .validate_captured_session_backing(17, &other_owner)
            .unwrap()
    );
    fixture
        .source
        .save(&fixture.directory.join("saved-as.inkpod"))
        .unwrap();
    assert!(
        !adapter
            .validate_captured_session_backing(17, &fixture.source)
            .unwrap()
    );
    adapter
        .capture_session_from_core(17, 3, 5, "fallback.inkpod".into(), 1, &fixture.source)
        .unwrap();
    fixture
        .source
        .new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x5a552)
        .unwrap();
    assert!(
        !adapter
            .validate_captured_session_backing(17, &fixture.source)
            .unwrap()
    );
}

fn finish_file_job(core: &mut Core, manager: &IoManager, request: inkpod_core::FileIoRequest) {
    use inkpod_core::{FileIoApply, FileIoJob, FileIoState};
    let mut job = FileIoJob::start(Some(core), manager.clone(), request).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let progress = job.poll();
        match progress.state {
            FileIoState::Ready => {
                if matches!(job.apply(core).unwrap(), FileIoApply::Complete { .. }) {
                    break;
                }
            }
            FileIoState::Failed | FileIoState::Cancelled => panic!("{:?}", job.error()),
            _ => {}
        }
        assert!(std::time::Instant::now() < deadline, "file job timed out");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn owned_pair_raster_input_uses_canonical_snapshot_and_batch_disk() {
    use inkpod_core::{CommonRasterFormat, FileIoKind, FileIoRequest};
    for committed in [true, false] {
        let mut fixture = Fixture::new();
        let manager = IoManager::new(IoConfig::default()).unwrap();
        let native = fixture.directory.join("A007.inkpod");
        let raster = fixture.directory.join("A007.png");
        if committed {
            finish_file_job(
                &mut fixture.source,
                &manager,
                FileIoRequest::new(FileIoKind::SavePair, vec![native.clone(), raster.clone()]),
            );
        } else {
            fs::write(
                &raster,
                fixture
                    .source
                    .export_common_raster(CommonRasterFormat::Png, false)
                    .unwrap(),
            )
            .unwrap();
            finish_file_job(
                &mut fixture.source,
                &manager,
                FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster.clone()]),
            );
        }
        let disk = fs::read(&raster).unwrap();
        fixture
            .source
            .set_grid(GridConfig {
                origin_x: 1,
                origin_y: 2,
                spacing_x: 8,
                spacing_y: 9,
                subdivisions: 2,
            })
            .unwrap();
        let expected = fixture.source.document_state_digest().unwrap();
        let expected_editor = fixture.source.editor_state().unwrap();
        let expected_uuid = fixture.source.document_info().unwrap().document_uuid;
        for profile in ["canonical", "batch"] {
            let text = format!(
                "inkscript 3; requires {{ procedure_catalog = 8; replay_epoch = 29; }} inputs {{ profile = {profile}; file \"A007.png\"; }} program {{}} output {{ policy = new_tabs; }} execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"
            );
            let program = compile_inkscript(
                &InkScriptSource::new(InkScriptSourceId::new(510), text.as_bytes()).unwrap(),
                InkScriptRunParameterDecision::Resolve(vec![]),
            )
            .unwrap();
            let mut adapter = ScriptIoAdapter::new(
                manager.clone(),
                program
                    .path_intents()
                    .iter()
                    .map(|intent| (intent.id(), raster.clone()))
                    .collect(),
                1,
            )
            .unwrap();
            adapter
                .capture_session_from_core(
                    17,
                    3,
                    5,
                    "open-session.inkpod".into(),
                    1,
                    &fixture.source,
                )
                .unwrap();
            let plan = adapter
                .plan(
                    &program,
                    None,
                    None,
                    ScriptPlanLimits::exact_current(),
                    &mut || false,
                )
                .unwrap();
            let mut confirmation = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
            let mut task = start_inkscript_run(
                &program,
                plan,
                &mut confirmation,
                ScriptRunMode::DryRun,
                ScriptRunLimits::exact_current(),
            )
            .unwrap();
            while !matches!(
                task.advance(&mut adapter, &mut || false),
                ScriptRunAdvance::Complete
            ) {}
            assert_eq!(
                task.finish().unwrap().items[0].outcome,
                ScriptItemOutcome::DryRun
            );
            let mut results = task.take_dry_results().unwrap();
            let staged = results.pop().unwrap().1.into_staged();
            if profile == "canonical" {
                assert_eq!(
                    staged.document_state_digest().unwrap(),
                    expected,
                    "committed={committed}"
                );
                assert_eq!(staged.editor_state().unwrap(), expected_editor);
                assert_eq!(staged.document_info().unwrap().document_uuid, expected_uuid);
                assert_eq!(
                    staged
                        .verify_journal_replay()
                        .unwrap()
                        .document_state_digest(),
                    expected
                );
            } else {
                assert_ne!(staged.document_state_digest().unwrap(), expected);
                assert_eq!(
                    staged
                        .export_common_raster(CommonRasterFormat::Png, false)
                        .unwrap(),
                    disk
                );
            }
        }
        assert_eq!(fs::read(&raster).unwrap(), disk);
        assert_eq!(native.exists(), committed);
    }
}

#[test]
fn owned_pair_missing_member_remains_reserved_for_normal_save() {
    use inkpod_core::{CommonRasterFormat, FileIoKind, FileIoRequest};
    for committed in [true, false] {
        let mut fixture = Fixture::new();
        let manager = IoManager::new(IoConfig::default()).unwrap();
        let native = fixture.directory.join("A007.inkpod");
        let raster = fixture.directory.join("A007.png");
        if committed {
            finish_file_job(
                &mut fixture.source,
                &manager,
                FileIoRequest::new(FileIoKind::SavePair, vec![native.clone(), raster.clone()]),
            );
            fs::remove_file(&raster).unwrap();
            finish_file_job(
                &mut fixture.source,
                &manager,
                FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]),
            );
        } else {
            fs::write(
                &raster,
                fixture
                    .source
                    .export_common_raster(CommonRasterFormat::Png, false)
                    .unwrap(),
            )
            .unwrap();
            finish_file_job(
                &mut fixture.source,
                &manager,
                FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster.clone()]),
            );
        }
        let format = if committed { "png" } else { "inkpod" };
        let text = format!(
            "inkscript 3; requires {{ procedure_catalog = 8; replay_epoch = 29; }} inputs {{ current_document; }} program {{}} output {{ policy = folder; format = {format}; folder = \"out\"; naming_template = \"A007\"; }} execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"
        );
        let program = compile_inkscript(
            &InkScriptSource::new(InkScriptSourceId::new(511), text.as_bytes()).unwrap(),
            InkScriptRunParameterDecision::Resolve(vec![]),
        )
        .unwrap();
        let mut adapter = ScriptIoAdapter::new(
            manager,
            program
                .path_intents()
                .iter()
                .map(|intent| (intent.id(), fixture.directory.clone()))
                .collect(),
            0,
        )
        .unwrap();
        adapter
            .capture_session_from_core(17, 3, 5, "open-session.inkpod".into(), 1, &fixture.source)
            .unwrap();
        assert_eq!(
            adapter
                .plan(
                    &program,
                    Some(17),
                    None,
                    ScriptPlanLimits::exact_current(),
                    &mut || false
                )
                .err(),
            Some(ScriptPlanError::OutputCollision),
            "committed={committed}"
        );
        assert_eq!(native.exists(), committed);
        assert_ne!(raster.exists(), committed);
    }
}
