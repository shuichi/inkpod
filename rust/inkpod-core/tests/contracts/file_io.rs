//! IO-003 public asynchronous filesystem publication contracts.
use inkpod_core::*;
use inkpod_format::{BATCH_GRAPH_VERSION, CommonRaster, encode_common_raster};
use inkpod_io::{IoConfig, IoManager};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static DIRECTORY: AtomicU64 = AtomicU64::new(1);

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "inkpod-io-contract-{}-{}",
            std::process::id(),
            DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn image(&self, name: &str, format: CommonRasterFormat) -> PathBuf {
        let path = self.0.join(name);
        let raster = CommonRaster::new(
            2,
            2,
            PixelFormat::StraightRgba8,
            Some(144_000),
            Some(144_000),
            [10, 20, 30, 255].repeat(4),
        )
        .unwrap();
        std::fs::write(&path, encode_common_raster(format, &raster, false).unwrap()).unwrap();
        path
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn manager() -> IoManager {
    IoManager::new(IoConfig {
        worker_count: 2,
        ..IoConfig::default()
    })
    .unwrap()
}

fn ready(job: &mut FileIoJob) -> FileIoProgress {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let progress = job.poll();
        if matches!(
            progress.state,
            FileIoState::Ready
                | FileIoState::Failed
                | FileIoState::Cancelled
                | FileIoState::Complete
        ) {
            return progress;
        }
        assert!(Instant::now() < deadline, "job stalled: {progress:?}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn open(core: &mut Core, manager: &IoManager, path: &Path) {
    let mut job = FileIoJob::start(
        Some(core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRaster, vec![path.to_path_buf()]),
    )
    .unwrap();
    assert_eq!(
        ready(&mut job).state,
        FileIoState::Ready,
        "{:?}",
        job.error()
    );
    assert!(matches!(
        job.apply(core).unwrap(),
        FileIoApply::Complete { .. }
    ));
}

fn save(core: &mut Core, manager: &IoManager, path: &Path) {
    let mut job = FileIoJob::start(
        Some(core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SavePair, vec![path.to_path_buf()]),
    )
    .unwrap();
    assert_eq!(
        ready(&mut job).state,
        FileIoState::Ready,
        "{:?}",
        job.error()
    );
    assert!(matches!(job.apply(core).unwrap(), FileIoApply::Pending));
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    assert!(matches!(
        job.apply(core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert_eq!(job.poll().result_count, 2);
}

fn assert_normal_file_authority_revoked(
    core: &mut Core,
    manager: &IoManager,
    path: &Path,
    old_save: &DocumentSaveToken,
) {
    let before = core.document_info().unwrap();
    assert_eq!(
        core.revert(),
        Err(CoreError::InvalidState("document has no normal-save path"))
    );
    assert_eq!(
        core.validate_document_save(old_save),
        Err(CoreError::InvalidState("document file request is stale"))
    );
    let mut save = FileIoJob::start(
        Some(core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SavePair, vec![path.to_path_buf()]),
    )
    .unwrap();
    assert_eq!(ready(&mut save).state, FileIoState::Failed);
    assert_eq!(save.error(), Some(&CoreError::FileConflict));
    assert_eq!(core.document_info().unwrap(), before);
}

struct WorkerGate {
    release: Option<std::sync::mpsc::Sender<()>>,
    job: inkpod_io::IoJob<()>,
}

impl WorkerGate {
    fn new(manager: &IoManager) -> Self {
        let (entered, observed) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let job = manager
            .submit(move |_| {
                entered.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(20)).unwrap();
                Ok(())
            })
            .unwrap();
        observed.recv_timeout(Duration::from_secs(20)).unwrap();
        Self {
            release: Some(release),
            job,
        }
    }

    fn release(mut self) {
        self.release.take().unwrap().send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(result) = self.job.try_take() {
                result.unwrap();
                break;
            }
            assert!(Instant::now() < deadline, "test worker did not resume");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for WorkerGate {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

fn serial_manager() -> IoManager {
    IoManager::new(IoConfig {
        worker_count: 1,
        ..IoConfig::default()
    })
    .unwrap()
}

fn batch_options() -> BatchRunOptions {
    BatchRunOptions {
        scope: BatchRunScope::All,
        dry_run: false,
        preview_confirmed: true,
    }
}

fn batch_folder(path: &Path) -> BatchInputSelector {
    BatchInputSelector {
        kind: BatchInputKind::Folder,
        path: path.to_string_lossy().into_owned(),
        first_cell: 0,
        last_cell: 0,
    }
}

fn batch_graph(inputs: Vec<BatchInputSelector>, output: BatchOutputSettings) -> BatchGraph {
    BatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "async IO contract".to_owned(),
        inputs,
        operations: vec![BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            target: BatchTargetSelector::color_plane(),
            additional_targets: Vec::new(),
            kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([0; 4]),
                new: PixelValue::Rgba([20, 40, 60, 255]),
            }]),
        }],
        output,
    }
}

fn sequence_core() -> Core {
    let mut core = Core::new();
    core.set_new_cell_raster_format(CommonRasterFormat::Tiff);
    let current = core.new_cell(2, 2, 144_000, 144_000).unwrap();
    let raster = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(144_000),
        Some(144_000),
        [1, 2, 3, 255].repeat(4),
    )
    .unwrap();
    let first = SequenceCellSource::from_common_raster("cell1.tif", current.document_uuid, &raster)
        .unwrap();
    let mut second = SequenceCellSource::from_common_raster("cell2.bmp", 0x1020, &raster).unwrap();
    second.raster_file_format = CommonRasterFormat::Bmp;
    core.set_sequence(vec![first, second]).unwrap();
    core
}

#[test]
fn io_003_compacted_output_fences_install_without_adopting_path_or_savepoint() {
    let files = Files::new();
    let manager = serial_manager();
    let normal = files.0.join("normal.inkpod");
    let output = files.0.join("compact.inkpod");
    let mut core = Core::new();
    core.new_cell(2, 2, 144_000, 144_000).unwrap();
    core.save(&normal).unwrap();
    let normal_bytes = std::fs::read(&normal).unwrap();
    core.set_main_line_color(PixelValue::Rgba([3, 4, 5, 255]))
        .unwrap();
    let before = core.document_info().unwrap();
    let history = core.history_entries().to_vec();
    let (_, token) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap();
    assert!(
        FileIoJob::start_compacted_copy(
            &core,
            manager.clone(),
            normal.clone(),
            core.compaction_plan().unwrap()
        )
        .is_err()
    );
    let mut job = FileIoJob::start_compacted_copy(
        &core,
        manager.clone(),
        output.clone(),
        core.compaction_plan().unwrap(),
    )
    .unwrap();
    assert_eq!(
        ready(&mut job).state,
        FileIoState::Ready,
        "{:?}",
        job.error()
    );
    assert!(!output.exists());
    assert_eq!(core.document_info().unwrap(), before);
    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert!(job.requires_finalization());
    assert!(
        core.set_main_line_color(PixelValue::Rgba([6, 7, 8, 255]))
            .is_err()
    );
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    let mut wrong_owner = Core::new();
    wrong_owner.new_cell(1, 1, 144_000, 144_000).unwrap();
    assert!(job.apply(&mut wrong_owner).is_err());
    assert!(job.requires_finalization());
    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert!(!job.requires_finalization());
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.history_entries(), history);
    core.validate_document_save(&token).unwrap();
    assert_eq!(std::fs::read(&normal).unwrap(), normal_bytes);
    assert!(output.is_file());
    assert!(!output.with_extension("png").exists());
    let compacted =
        Core::from_native_file(inkpod_format::read_procedure_file(&output).unwrap(), false)
            .unwrap();
    assert_eq!(compacted.persistence_info().unwrap().procedure_count, 0);
    assert_eq!(
        compacted.document_state_digest().unwrap(),
        core.document_state_digest().unwrap()
    );
    manager.shutdown_and_wait();
}

#[test]
fn io_003_compacted_output_cancel_drop_stale_and_install_failure_preserve_live_state() {
    let files = Files::new();
    let manager = serial_manager();
    let mut core = Core::new();
    core.new_cell(2, 2, 144_000, 144_000).unwrap();
    core.set_main_line_color(PixelValue::Rgba([9, 8, 7, 255]))
        .unwrap();
    let plan = core.compaction_plan().unwrap();
    let before = core.document_info().unwrap();

    let gate = WorkerGate::new(&manager);
    let dropped_path = files.0.join("dropped.inkpod");
    let dropped =
        FileIoJob::start_compacted_copy(&core, manager.clone(), dropped_path.clone(), plan)
            .unwrap();
    drop(dropped);
    let cancelled_path = files.0.join("cancelled.inkpod");
    let mut cancelled =
        FileIoJob::start_compacted_copy(&core, manager.clone(), cancelled_path.clone(), plan)
            .unwrap();
    cancelled.cancel();
    gate.release();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Cancelled);
    assert!(cancelled.apply(&mut core).is_err());
    assert!(!dropped_path.exists() && !cancelled_path.exists());
    assert_eq!(core.document_info().unwrap(), before);

    let mut cancelled =
        FileIoJob::start_compacted_copy(&core, manager.clone(), cancelled_path.clone(), plan)
            .unwrap();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Ready);
    let gate = WorkerGate::new(&manager);
    assert!(matches!(
        cancelled.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    cancelled.cancel();
    gate.release();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Ready);
    assert_eq!(
        cancelled.apply(&mut core).unwrap_err(),
        CoreError::Cancelled
    );
    assert!(!cancelled.requires_finalization());
    core.capture_document_save().unwrap();
    assert!(!cancelled_path.exists());
    assert_eq!(core.document_info().unwrap(), before);

    let existing_path = files.0.join("existing.inkpod");
    std::fs::write(&existing_path, b"do not overwrite").unwrap();
    let mut existing =
        FileIoJob::start_compacted_copy(&core, manager.clone(), existing_path.clone(), plan)
            .unwrap();
    assert_eq!(ready(&mut existing).state, FileIoState::Ready);
    assert!(matches!(
        existing.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut existing).state, FileIoState::Ready);
    assert!(existing.apply(&mut core).is_err());
    assert!(!existing.requires_finalization());
    assert_eq!(std::fs::read(&existing_path).unwrap(), b"do not overwrite");
    assert_eq!(core.document_info().unwrap(), before);

    let stale_path = files.0.join("stale.inkpod");
    let mut stale =
        FileIoJob::start_compacted_copy(&core, manager.clone(), stale_path.clone(), plan).unwrap();
    assert_eq!(ready(&mut stale).state, FileIoState::Ready);
    core.set_main_line_color(PixelValue::Rgba([6, 5, 4, 255]))
        .unwrap();
    let edited = core.document_info().unwrap();
    assert!(stale.apply(&mut core).is_err());
    assert!(!stale_path.exists());
    assert_eq!(core.document_info().unwrap(), edited);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_sequence_activation_and_step_revoke_old_native_save_authority() {
    for (step, same_uuid) in [(false, false), (true, false), (false, true), (true, true)] {
        let files = Files::new();
        let manager = serial_manager();
        let normal = files.0.join("source.inkpod");
        let mut core = Core::new();
        core.set_new_cell_raster_format(CommonRasterFormat::Tiff);
        let initial = core
            .new_cell_with_uuid(2, 2, 144_000, 144_000, 0x1010)
            .unwrap();
        let raster = CommonRaster::new(
            2,
            2,
            PixelFormat::StraightRgba8,
            Some(144_000),
            Some(144_000),
            [1, 2, 3, 255].repeat(4),
        )
        .unwrap();
        let target_uuid = if same_uuid {
            initial.document_uuid
        } else {
            0x2020
        };
        let mut sources = Vec::new();
        for (name, uuid, generation) in [
            ("cell1.tif", initial.document_uuid, 1),
            ("cell2.tif", target_uuid, 2),
        ] {
            let mut source = SequenceCellSource::from_common_raster_with_generation(
                name, uuid, generation, &raster,
            )
            .unwrap();
            source.raster_file_format = CommonRasterFormat::Tiff;
            sources.push(source);
        }
        core.set_sequence(sources).unwrap();
        save(&mut core, &manager, &normal);
        let native_before = std::fs::read(&normal).unwrap();
        let raster_before = std::fs::read(normal.with_extension("tif")).unwrap();
        let (_, old_save) = core
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap();
        let request = core
            .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
            .unwrap();
        assert!(request.requires_switch());
        assert_eq!(
            core.resolve_sequence_activation(1).unwrap().kind,
            SequenceActivationKind::Replace
        );
        let before = core.document_info().unwrap();
        let after = if step {
            let plan = core
                .resolve_sequence_step(SequenceDirection::Next, SequenceEndpointPolicy::Stop)
                .unwrap();
            assert!(plan.requires_switch());
            core.commit_sequence_step(plan).unwrap()
        } else {
            core.sequence_activate(1).unwrap()
        };
        assert_eq!(after.document_uuid, target_uuid);
        assert!(after.document_revision > before.document_revision);
        assert!(!after.dirty);
        assert!(!core.editor_state().unwrap().dirty);
        assert_normal_file_authority_revoked(&mut core, &manager, &normal, &old_save);
        assert_eq!(std::fs::read(&normal).unwrap(), native_before);
        assert_eq!(
            std::fs::read(normal.with_extension("tif")).unwrap(),
            raster_before
        );
        manager.shutdown_and_wait();
    }
}

#[test]
fn io_003_pathless_replacements_revoke_previous_native_pair_authority() {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Replacement {
        LightTable,
        EncodedRaster,
        DecodedRaster,
        PathlessAdopt,
    }

    for replacement in [
        Replacement::LightTable,
        Replacement::EncodedRaster,
        Replacement::DecodedRaster,
        Replacement::PathlessAdopt,
    ] {
        let files = Files::new();
        let manager = serial_manager();
        let normal = files.0.join("previous.inkpod");
        let mut core = Core::new();
        core.new_cell_with_uuid(2, 2, 144_000, 144_000, 0x8011)
            .unwrap();
        let raster = CommonRaster::new(
            2,
            2,
            PixelFormat::StraightRgba8,
            Some(144_000),
            Some(144_000),
            [60, 80, 100, 255].repeat(4),
        )
        .unwrap();
        let encoded = encode_common_raster(CommonRasterFormat::Png, &raster, false).unwrap();
        let item_id = if replacement == Replacement::LightTable {
            let source = LightTableSource::from_common_raster(
                0x8022,
                1,
                RectI32 {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
                &raster,
            )
            .unwrap();
            core.light_table_add_item(LightTableItemInput::new("replacement", source))
                .unwrap()
                .1
        } else {
            0
        };
        save(&mut core, &manager, &normal);
        let before = core.document_info().unwrap();
        let (_, before_failure) = core
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap();
        let rejected = match replacement {
            Replacement::LightTable => core.light_table_swap_with_active(u64::MAX),
            Replacement::EncodedRaster => {
                core.import_common_raster(CommonRasterFormat::Png, b"invalid image", 0x8022)
            }
            Replacement::DecodedRaster => {
                core.import_decoded_common_raster(CommonRasterFormat::Png, &raster, 0)
            }
            Replacement::PathlessAdopt => {
                let token = core.capture_document_open().unwrap();
                let staged = core.clone();
                core.adopt_opened_document(token, staged, Some(Path::new("")))
            }
        };
        assert!(rejected.is_err(), "{replacement:?}");
        assert_eq!(core.document_info().unwrap(), before, "{replacement:?}");
        core.validate_document_save(&before_failure).unwrap();
        if replacement == Replacement::LightTable {
            core.add_guide(GuideAxis::Vertical, 1).unwrap();
            let dirty = core.document_info().unwrap();
            assert_eq!(
                core.light_table_swap_with_active(item_id),
                Err(CoreError::UnsavedChanges)
            );
            assert_eq!(core.document_info().unwrap(), dirty);
        }
        // Failed replacement retains authority: this same-path save requires no confirmation.
        save(&mut core, &manager, &normal);
        let native_before = std::fs::read(&normal).unwrap();
        let raster_before = std::fs::read(normal.with_extension("png")).unwrap();
        let (_, old_save) = core
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap();
        let replaced = match replacement {
            Replacement::LightTable => core.light_table_swap_with_active(item_id),
            Replacement::EncodedRaster => {
                core.import_common_raster(CommonRasterFormat::Png, &encoded, 0x8022)
            }
            Replacement::DecodedRaster => {
                core.import_decoded_common_raster(CommonRasterFormat::Png, &raster, 0x8022)
            }
            Replacement::PathlessAdopt => {
                let token = core.capture_document_open().unwrap();
                let staged = core.clone();
                core.adopt_opened_document(token, staged, None)
            }
        }
        .unwrap();
        assert_eq!(
            replaced.document_uuid,
            if replacement == Replacement::PathlessAdopt {
                0x8011
            } else {
                0x8022
            }
        );
        assert_normal_file_authority_revoked(&mut core, &manager, &normal, &old_save);
        assert_eq!(std::fs::read(&normal).unwrap(), native_before);
        assert_eq!(
            std::fs::read(normal.with_extension("png")).unwrap(),
            raster_before
        );
        manager.shutdown_and_wait();
    }
}

#[test]
fn io_003_sequence_switch_installs_recovery_before_one_owner_commit_and_restores_exactly() {
    let files = Files::new();
    let manager = serial_manager();
    let normal = files.0.join("normal.inkpod");
    let recovery = files.0.join("recovery").join("source.inkpod");
    let target_recovery = files.0.join("recovery").join("target.inkpod");
    let mut core = sequence_core();
    save(&mut core, &manager, &normal);
    let normal_bytes = std::fs::read(&normal).unwrap();
    let raster_bytes = std::fs::read(normal.with_extension("tif")).unwrap();
    core.set_main_line_color(PixelValue::Rgba([10, 11, 12, 255]))
        .unwrap();
    core.update_editor_state(
        core.editor_state().unwrap().revision,
        EditorStateUpdate::SetToolDiameter {
            tool: EditorTool::Brush,
            diameter_q16: 13_i64 << 16,
        },
    )
    .unwrap();
    let before = core.document_info().unwrap();
    let digest = core.document_state_digest().unwrap();
    let history = core.history_entries().to_vec();
    let editor = core.editor_state().unwrap();
    assert!(editor.dirty);
    let metadata = inkpod_io::RecoveryMetadata {
        session_id: 1,
        generation: 2,
        document_uuid: before.document_uuid,
        original_path: normal.to_string_lossy().into_owned(),
        written_time_100ns: 123,
        ..inkpod_io::RecoveryMetadata::default()
    };
    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let (_, old_save) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(false, || false)
        .unwrap();
    let mut job = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(recovery.clone()),
        None,
        Some(metadata.clone()),
    )
    .unwrap();
    assert_eq!(
        ready(&mut job).state,
        FileIoState::Ready,
        "{:?}",
        job.error()
    );
    assert!(!recovery.exists());
    assert_eq!(core.document_info().unwrap(), before);
    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert!(core.new_cell(1, 1, 144_000, 144_000).is_err());
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    assert_eq!(core.document_info().unwrap(), before);
    job.apply(&mut core).unwrap();
    assert_eq!(
        core.document_info().unwrap().document_uuid,
        request.target_document_uuid
    );
    assert!(!core.document_info().unwrap().dirty);
    assert!(!core.editor_state().unwrap().dirty);
    assert_eq!(core.raster_file_format().unwrap(), CommonRasterFormat::Bmp);
    assert_normal_file_authority_revoked(&mut core, &manager, &normal, &old_save);
    let stored = manager
        .read_recovery_metadata(&recovery, &inkpod_io::JobContext::new())
        .unwrap();
    assert_eq!(stored, metadata);
    assert_eq!(std::fs::read(&normal).unwrap(), normal_bytes);
    assert!(!recovery.with_extension("tif").exists());
    let recovery_bytes = std::fs::read(&recovery).unwrap();
    let recovery_file_count = std::fs::read_dir(recovery.parent().unwrap())
        .unwrap()
        .count();

    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut restored = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some(recovery.clone()),
        None,
    )
    .unwrap();
    assert_eq!(
        ready(&mut restored).state,
        FileIoState::Ready,
        "{:?}",
        restored.error()
    );
    assert!(matches!(
        restored.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert_eq!(restored.poll().state, FileIoState::Complete);
    assert!(!restored.requires_finalization());
    assert!(!target_recovery.exists());
    assert_eq!(std::fs::read(&recovery).unwrap(), recovery_bytes);
    assert_eq!(
        std::fs::read_dir(recovery.parent().unwrap())
            .unwrap()
            .count(),
        recovery_file_count
    );
    assert_eq!(core.document_state_digest().unwrap(), digest);
    assert_eq!(core.history_entries(), history);
    assert_eq!(core.editor_state().unwrap(), editor);
    assert!(core.document_info().unwrap().recovered);
    assert!(core.document_info().unwrap().dirty);
    assert_eq!(core.raster_file_format().unwrap(), CommonRasterFormat::Tiff);
    assert_normal_file_authority_revoked(&mut core, &manager, &normal, &old_save);
    assert_eq!(
        std::fs::read(normal.with_extension("tif")).unwrap(),
        raster_bytes
    );
    assert_eq!(std::fs::read(normal).unwrap(), normal_bytes);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_sequence_switch_requires_recovery_for_document_or_editor_edits() {
    let files = Files::new();
    let manager = serial_manager();
    for editor_only in [false, true] {
        let mut core = sequence_core();
        assert!(!core.sequence_activate(1).unwrap().dirty);
        let clean = core.document_info().unwrap();
        if editor_only {
            core.update_editor_state(
                core.editor_state().unwrap().revision,
                EditorStateUpdate::SetToolDiameter {
                    tool: EditorTool::Brush,
                    diameter_q16: 19_i64 << 16,
                },
            )
            .unwrap();
            assert_eq!(
                core.document_info().unwrap().document_revision,
                clean.document_revision
            );
        } else {
            core.replace_palette(&[PixelValue::Rgba([10, 11, 12, 255])])
                .unwrap();
        }
        let before = core.document_info().unwrap();
        let editor = core.editor_state().unwrap();
        let history = core.history_entries().to_vec();
        let journal = core.journal_entries().to_vec();
        assert!(before.dirty);
        assert_eq!(editor.dirty, editor_only);
        let request = core
            .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
            .unwrap();
        assert!(request.requires_switch());
        assert!(matches!(
            FileIoJob::start_sequence_switch(
                &core,
                manager.clone(),
                request,
                None,
                Some(files.0.join("unread-target.inkpod")),
                None,
            ),
            Err(CoreError::InvalidArgument(
                "dirty sequence source requires a recovery destination"
            ))
        ));
        assert_eq!(core.document_info().unwrap(), before);
        assert_eq!(core.editor_state().unwrap(), editor);
        assert_eq!(core.history_entries(), history);
        assert_eq!(core.journal_entries(), journal);
        assert_eq!(std::fs::read_dir(&files.0).unwrap().count(), 0);
    }
    manager.shutdown_and_wait();
}

#[test]
fn io_003_clean_sequence_source_omission_keeps_cancel_failure_and_stale_atomic() {
    let files = Files::new();
    let manager = serial_manager();
    let recovery = files.0.join("source.inkpod");
    let wrong = files.0.join("wrong.inkpod");
    let malformed = files.0.join("malformed.inkpod");
    let mut core = sequence_core();
    core.autosave(&recovery).unwrap();
    assert!(!core.sequence_activate(1).unwrap().dirty);
    core.autosave(&wrong).unwrap();
    std::fs::write(&malformed, b"not a native recovery file").unwrap();
    let bytes: Vec<_> = [&recovery, &wrong, &malformed]
        .map(|path| (path.clone(), std::fs::read(path).unwrap()))
        .into_iter()
        .collect();
    let before = core.document_info().unwrap();
    let editor = core.editor_state().unwrap();
    let history = core.history_entries().to_vec();
    let journal = core.journal_entries().to_vec();
    let (_, save_token) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(false, || false)
        .unwrap();
    let assert_unchanged = |core: &Core| {
        assert_eq!(core.document_info().unwrap(), before);
        assert_eq!(core.editor_state().unwrap(), editor);
        assert_eq!(core.history_entries(), history);
        assert_eq!(core.journal_entries(), journal);
        core.validate_document_save(&save_token).unwrap();
    };
    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(request.requires_switch());
    let gate = WorkerGate::new(&manager);
    let dropped = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some(recovery.clone()),
        None,
    )
    .unwrap();
    drop(dropped);
    let mut cancelled = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some(recovery.clone()),
        None,
    )
    .unwrap();
    cancelled.cancel();
    gate.release();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Cancelled);
    assert!(cancelled.apply(&mut core).is_err());
    assert!(!cancelled.requires_finalization());
    assert_unchanged(&core);

    let mut cancelled = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some(recovery.clone()),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Ready);
    cancelled.cancel();
    assert_eq!(
        cancelled.apply(&mut core).unwrap_err(),
        CoreError::Cancelled
    );
    assert!(!cancelled.requires_finalization());
    assert_unchanged(&core);

    for invalid in [&wrong, &malformed] {
        let mut failed = FileIoJob::start_sequence_switch(
            &core,
            manager.clone(),
            request,
            None,
            Some(invalid.clone()),
            None,
        )
        .unwrap();
        assert_eq!(ready(&mut failed).state, FileIoState::Failed);
        assert!(failed.apply(&mut core).is_err());
        assert!(!failed.requires_finalization());
        assert_unchanged(&core);
    }

    let mut stale = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some(recovery),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut stale).state, FileIoState::Ready);
    core.update_editor_state(
        core.editor_state().unwrap().revision,
        EditorStateUpdate::SetToolDiameter {
            tool: EditorTool::Brush,
            diameter_q16: 23_i64 << 16,
        },
    )
    .unwrap();
    let edited = core.document_info().unwrap();
    let edited_editor = core.editor_state().unwrap();
    assert!(stale.apply(&mut core).is_err());
    assert!(!stale.requires_finalization());
    assert_eq!(core.document_info().unwrap(), edited);
    assert_eq!(core.editor_state().unwrap(), edited_editor);
    assert_eq!(core.history_entries(), history);
    assert_eq!(core.journal_entries(), journal);
    assert_eq!(std::fs::read_dir(&files.0).unwrap().count(), bytes.len());
    for (path, original) in bytes {
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
    manager.shutdown_and_wait();
}

#[test]
fn io_003_sequence_switch_drop_cancel_and_stale_do_not_write_recovery() {
    let files = Files::new();
    let manager = serial_manager();
    let mut core = sequence_core();
    let normal = files.0.join("normal.inkpod");
    save(&mut core, &manager, &normal);
    let native_before = std::fs::read(&normal).unwrap();
    let raster_before = std::fs::read(normal.with_extension("tif")).unwrap();
    let before = core.document_info().unwrap();
    let (_, old_save) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(false, || false)
        .unwrap();
    assert_eq!(core.sequence_activate(0).unwrap(), before);
    let stopped = core
        .resolve_sequence_step(SequenceDirection::Previous, SequenceEndpointPolicy::Stop)
        .unwrap();
    assert!(!stopped.requires_switch());
    assert_eq!(core.commit_sequence_step(stopped).unwrap(), before);
    assert!(matches!(
        core.sequence_activate(usize::MAX),
        Err(CoreError::InvalidArgument(_))
    ));
    core.validate_document_save(&old_save).unwrap();
    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let path = files.0.join("source.inkpod");
    let gate = WorkerGate::new(&manager);
    let dropped = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(path.clone()),
        None,
        None,
    )
    .unwrap();
    drop(dropped);
    let mut cancelled = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(path.clone()),
        None,
        None,
    )
    .unwrap();
    cancelled.cancel();
    gate.release();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Cancelled);
    assert!(!path.exists());
    assert_eq!(core.document_info().unwrap(), before);

    let mut cancelled = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(path.clone()),
        None,
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Ready);
    let gate = WorkerGate::new(&manager);
    cancelled.apply(&mut core).unwrap();
    assert!(cancelled.requires_finalization());
    cancelled.cancel();
    gate.release();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Ready);
    assert_eq!(
        cancelled.apply(&mut core).unwrap_err(),
        CoreError::Cancelled
    );
    assert!(!cancelled.requires_finalization());
    assert_eq!(core.document_info().unwrap(), before);
    assert!(!path.exists());
    core.validate_document_save(&old_save).unwrap();

    let mut stale = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(path.clone()),
        None,
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut stale).state, FileIoState::Ready);
    core.set_main_line_color(PixelValue::Rgba([1, 9, 3, 255]))
        .unwrap();
    let edited = core.document_info().unwrap();
    assert!(stale.apply(&mut core).is_err());
    assert_eq!(core.document_info().unwrap(), edited);
    assert!(!path.exists());
    let same = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let (_, no_op_save) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(false, || false)
        .unwrap();
    let irrelevant_recovery = files.0.join("ignored.inkpod");
    std::fs::write(&irrelevant_recovery, b"not a native file").unwrap();
    let mut no_op = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        same,
        None,
        Some(irrelevant_recovery),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut no_op).state, FileIoState::Ready);
    assert!(matches!(
        no_op.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert!(!no_op.requires_finalization());
    assert_eq!(core.document_info().unwrap(), edited);
    core.validate_document_save(&no_op_save).unwrap();
    assert_eq!(std::fs::read(&normal).unwrap(), native_before);
    assert_eq!(
        std::fs::read(normal.with_extension("tif")).unwrap(),
        raster_before
    );
    // Cancel, stale and no-op results retain the existing pair's save authority.
    save(&mut core, &manager, &normal);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_batch_active_result_is_one_undo_unit_and_keeps_owner_view_identity() {
    let files = Files::new();
    let manager = serial_manager();
    let mut core = Core::new();
    core.new_cell(2, 2, 144_000, 144_000).unwrap();
    core.save(&files.0.join("normal.inkpod")).unwrap();
    let before = core.document_info().unwrap();
    let original = core.document_state_digest().unwrap();
    let history_count = core.history_entries().len();
    let graph = batch_graph(
        vec![BatchInputSelector::active_document()],
        BatchOutputSettings {
            destination: BatchOutputDestination::ActiveDocument,
            ..BatchOutputSettings::default()
        },
    );
    let gate = WorkerGate::new(&manager);
    let mut job = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph.clone(),
        FileIoKind::BatchRun,
        batch_options(),
        0,
    )
    .unwrap();
    assert!(
        job.apply(&mut core).is_err(),
        "not-ready apply is only a query error"
    );
    assert_eq!(core.document_info().unwrap(), before);
    gate.release();
    assert_eq!(
        ready(&mut job).state,
        FileIoState::Ready,
        "{:?}",
        job.error()
    );
    assert_eq!(core.document_info().unwrap(), before);
    let view_id = core.create_view().unwrap();
    let view = core
        .apply_view_for(
            view_id,
            ViewCommand::PanBy {
                device_dx: 2.0,
                device_dy: 3.0,
            },
        )
        .unwrap();
    core.set_new_cell_raster_format(CommonRasterFormat::Bmp);
    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert_eq!(core.history_entries().len(), history_count + 1);
    assert!(core.document_info().unwrap().dirty);
    assert_ne!(core.document_state_digest().unwrap(), original);
    assert_eq!(core.build_snapshot_for(view_id).unwrap().view(), view);
    assert!(core.create_view().unwrap() > view_id);
    assert!(job.apply(&mut core).is_err());
    assert_eq!(job.poll().state, FileIoState::Complete);
    assert_eq!(job.take_batch_report().unwrap().failure_count(), 0);
    assert!(job.take_batch_report().is_err());
    core.undo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), original);
    assert!(!core.document_info().unwrap().dirty);

    let mut stale = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph,
        FileIoKind::BatchRun,
        batch_options(),
        0,
    )
    .unwrap();
    assert_eq!(ready(&mut stale).state, FileIoState::Ready);
    core.set_main_line_color(PixelValue::Rgba([8, 7, 6, 255]))
        .unwrap();
    let edited = core.document_info().unwrap();
    let history = core.history_entries().to_vec();
    assert!(stale.apply(&mut core).is_err());
    assert_eq!(core.document_info().unwrap(), edited);
    assert_eq!(core.history_entries(), history);
    core.new_cell(1, 1, 144_000, 144_000).unwrap();
    assert_eq!(core.raster_file_format().unwrap(), CommonRasterFormat::Bmp);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_batch_worker_preflight_cancel_and_drop_do_not_touch_output_or_owner() {
    let files = Files::new();
    let manager = serial_manager();
    let mut core = Core::new();
    core.new_cell(2, 2, 144_000, 144_000).unwrap();
    let before = core.document_info().unwrap();
    let output = files.0.join("never-created-output");
    let graph = batch_graph(
        vec![batch_folder(&files.0.join("missing-input"))],
        BatchOutputSettings {
            folder: output.to_string_lossy().into_owned(),
            ..BatchOutputSettings::default()
        },
    );
    let gate = WorkerGate::new(&manager);
    // Missing-folder failure must happen on the queued worker, not at submit.
    let mut invalid = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph.clone(),
        FileIoKind::BatchPlan,
        batch_options(),
        0,
    )
    .unwrap();
    assert_eq!(invalid.poll().total_count, 0);
    assert!(!output.exists());
    gate.release();
    assert_eq!(ready(&mut invalid).state, FileIoState::Failed);
    assert_eq!(core.document_info().unwrap(), before);
    assert!(!output.exists());

    let graph = batch_graph(
        vec![BatchInputSelector::active_document()],
        BatchOutputSettings {
            folder: output.to_string_lossy().into_owned(),
            ..BatchOutputSettings::default()
        },
    );
    let gate = WorkerGate::new(&manager);
    let dropped = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph.clone(),
        FileIoKind::BatchRun,
        batch_options(),
        0,
    )
    .unwrap();
    drop(dropped);
    let mut cancelled = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph,
        FileIoKind::BatchRun,
        batch_options(),
        0,
    )
    .unwrap();
    cancelled.cancel();
    gate.release();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Cancelled);
    assert!(cancelled.apply(&mut core).is_err());
    assert_eq!(core.document_info().unwrap(), before);
    assert!(!output.exists());
    manager.shutdown_and_wait();
}

#[test]
fn io_003_batch_file_folder_newtabs_preview_and_native_only_output_share_image_counts() {
    let files = Files::new();
    let manager = manager();
    files.image("cell2.tif", CommonRasterFormat::Tiff);
    let native_path = files.0.join("cell1.inkpod");
    let mut fixture = Core::new();
    fixture.set_new_cell_raster_format(CommonRasterFormat::Tga);
    fixture.new_cell(2, 2, 144_000, 144_000).unwrap();
    fixture.save(&native_path).unwrap();
    let mut core = Core::new();
    core.new_cell(1, 1, 144_000, 144_000).unwrap();
    let before = core.document_info().unwrap();
    let graph = batch_graph(
        vec![batch_folder(&files.0)],
        BatchOutputSettings {
            destination: BatchOutputDestination::NewTabs,
            ..BatchOutputSettings::default()
        },
    );

    let mut plan = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph.clone(),
        FileIoKind::BatchPlan,
        batch_options(),
        2,
    )
    .unwrap();
    let progress = ready(&mut plan);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", plan.error());
    assert_eq!((progress.total_count, progress.loaded_count), (2, 2));
    plan.apply(&mut core).unwrap();
    let preview = plan.take_batch_preview().unwrap();
    assert_eq!(
        preview
            .items
            .iter()
            .map(|item| item.input_name.as_str())
            .collect::<Vec<_>>(),
        vec!["cell1.inkpod", "cell2.tif"]
    );
    assert!(preview.items.iter().all(|item| item.warnings.is_empty()));
    assert!(plan.take_batch_preview().is_err());

    let mut insufficient = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph.clone(),
        FileIoKind::BatchRun,
        batch_options(),
        1,
    )
    .unwrap();
    assert_eq!(ready(&mut insufficient).state, FileIoState::Failed);
    assert_eq!(core.document_info().unwrap(), before);
    let mut tabs = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph.clone(),
        FileIoKind::BatchRun,
        batch_options(),
        2,
    )
    .unwrap();
    assert_eq!(ready(&mut tabs).loaded_count, 2);
    tabs.apply(&mut core).unwrap();
    let report = tabs.take_batch_report().unwrap();
    assert_eq!(report.failure_count(), 0);
    assert_eq!(report.staged_results.len(), 2);
    for (result, format) in report
        .staged_results
        .into_iter()
        .zip([CommonRasterFormat::Tga, CommonRasterFormat::Tiff])
    {
        assert!(result.is_pathless());
        let staged = result.into_core();
        assert!(staged.document_info().unwrap().dirty);
        assert_eq!(staged.raster_file_format().unwrap(), format);
    }

    let mut contact = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph.clone(),
        FileIoKind::BatchPreview,
        batch_options(),
        1,
    )
    .unwrap();
    let progress = ready(&mut contact);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", contact.error());
    assert_eq!(
        (progress.total_count, progress.loaded_count),
        (2, 2),
        "temporary rereads are not extra input images"
    );
    contact.apply(&mut core).unwrap();
    let report = contact.take_batch_report().unwrap();
    assert_eq!(report.failure_count(), 0);
    assert_eq!(report.staged_results.len(), 1);
    assert!(
        !report
            .staged_results
            .into_iter()
            .next()
            .unwrap()
            .into_core()
            .document_info()
            .unwrap()
            .dirty
    );
    let output = files.0.join("outputs");
    let graph = BatchGraph {
        output: BatchOutputSettings {
            folder: output.to_string_lossy().into_owned(),
            format: BatchOutputFormat::Inkpod,
            ..BatchOutputSettings::default()
        },
        ..graph
    };
    let mut output_job = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph,
        FileIoKind::BatchRun,
        batch_options(),
        0,
    )
    .unwrap();
    let progress = ready(&mut output_job);
    assert_eq!(
        progress.state,
        FileIoState::Ready,
        "{:?}",
        output_job.error()
    );
    assert_eq!(progress.loaded_count, 2);
    output_job.apply(&mut core).unwrap();
    let report = output_job.take_batch_report().unwrap();
    assert_eq!(report.failure_count(), 0);
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 2);
    assert!(
        report
            .items
            .iter()
            .all(|item| item.output_path.as_ref().unwrap().extension().unwrap() == "inkpod")
    );
    assert_eq!(core.document_info().unwrap(), before);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_light_table_reload_retains_properties_is_undoable_and_rejects_failed_or_cancelled_pixels()
{
    let files = Files::new();
    let path = files.image("reference.png", CommonRasterFormat::Png);
    let manager = serial_manager();
    let mut core = Core::new();
    core.new_cell(2, 2, 144_000, 144_000).unwrap();
    let mut add = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::LightTableAdd, vec![path.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut add).loaded_count, 1);
    let FileIoApply::Complete { object_id, .. } = add.apply(&mut core).unwrap() else {
        panic!("Light Table add unexpectedly deferred")
    };
    assert_ne!(object_id, 0);
    core.light_table_update_item_properties(
        object_id,
        LightTableItemProperties {
            visible: true,
            opacity_milli: 321,
            display_mode: LightTableDisplayMode::Color,
            display_color: PixelValue::Rgba([10, 30, 50, 255]),
            translate_x_milli: 125,
            translate_y_milli: -250,
            scale_x_milli: 1100,
            scale_y_milli: 900,
            rotation_milli_degrees: 1200,
        },
    )
    .unwrap();
    let original = core.light_table_items().unwrap()[0].clone();
    let original_digest = core.document_state_digest().unwrap();
    let history_count = core.history_entries().len();
    let raster = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(144_000),
        Some(144_000),
        [200, 100, 50, 255].repeat(4),
    )
    .unwrap();
    std::fs::write(
        &path,
        encode_common_raster(CommonRasterFormat::Png, &raster, false).unwrap(),
    )
    .unwrap();
    let mut request = FileIoRequest::new(FileIoKind::LightTableReload, vec![path.clone()]);
    request.object_id = object_id;
    let mut reload = FileIoJob::start(Some(&core), manager.clone(), request.clone()).unwrap();
    assert_eq!(ready(&mut reload).loaded_count, 1);
    assert_eq!(core.light_table_items().unwrap()[0], original);
    reload.apply(&mut core).unwrap();
    let replaced = core.light_table_items().unwrap()[0].clone();
    assert_ne!(replaced.source_revision, original.source_revision);
    let mut expected = original.clone();
    expected.source_document_uuid = replaced.source_document_uuid;
    expected.source_revision = replaced.source_revision;
    assert_eq!(replaced, expected);
    assert_ne!(core.document_state_digest().unwrap(), original_digest);
    assert_eq!(core.history_entries().len(), history_count + 1);
    core.undo().unwrap();
    assert_eq!(core.light_table_items().unwrap()[0], original);
    assert_eq!(core.document_state_digest().unwrap(), original_digest);
    core.redo().unwrap();
    let before = core.document_info().unwrap();
    let journal = core.journal_entries().to_vec();
    std::fs::write(&path, b"broken replacement").unwrap();
    let mut failed = FileIoJob::start(Some(&core), manager.clone(), request.clone()).unwrap();
    assert_eq!(ready(&mut failed).state, FileIoState::Failed);
    assert!(failed.apply(&mut core).is_err());
    assert_eq!(core.light_table_items().unwrap()[0], replaced);
    assert_eq!(core.document_info().unwrap(), before);
    let gate = WorkerGate::new(&manager);
    let reads = manager.cache_stats().physical_reads;
    let mut cancelled = FileIoJob::start(Some(&core), manager.clone(), request).unwrap();
    cancelled.cancel();
    gate.release();
    assert_eq!(ready(&mut cancelled).state, FileIoState::Cancelled);
    assert_eq!(manager.cache_stats().physical_reads, reads);
    assert_eq!(core.light_table_items().unwrap()[0], replaced);
    assert_eq!(core.journal_entries(), journal);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_raster_sequence_navigation_keeps_unedited_cells_clean() {
    let files = Files::new();
    let formats = [
        CommonRasterFormat::Png,
        CommonRasterFormat::Tga,
        CommonRasterFormat::Tiff,
        CommonRasterFormat::Bmp,
    ];
    let paths: Vec<_> = ["a1.png", "a2.tga", "a3.tif", "a4.bmp"]
        .into_iter()
        .zip(formats)
        .map(|(name, format)| files.image(name, format))
        .collect();
    let original_bytes: Vec<_> = paths
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect();
    let manager = manager();
    let mut core = Core::new();
    open(&mut core, &manager, &paths[0]);
    let opened = core.document_info().unwrap();
    let opened_editor = core.editor_state().unwrap();
    assert!(!opened.dirty);
    assert!(!opened_editor.dirty);
    let mut job = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SequenceAuto, vec![paths[0].clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    job.apply(&mut core).unwrap();
    assert_eq!(core.document_info().unwrap(), opened);
    assert_eq!(core.editor_state().unwrap(), opened_editor);
    assert_eq!(core.sequence_cells().unwrap().len(), formats.len());

    for index in [0, 1, 2, 3, 2, 1, 0, 1] {
        let plan = core.resolve_sequence_activation(index).unwrap();
        let active = core.commit_sequence_activation(plan).unwrap();
        assert!(!active.dirty, "sequence index {index}");
        assert!(!core.editor_state().unwrap().dirty);
        assert_eq!(core.raster_file_format().unwrap(), formats[index]);
        assert!(!active.can_undo && !active.can_redo);
        assert!(core.history_entries().is_empty());
        assert!(core.journal_entries().is_empty());
        assert_eq!(
            core.revert(),
            Err(CoreError::InvalidState("document has no normal-save path"))
        );
        assert_eq!(core.document_info().unwrap(), active);
    }
    assert_eq!(std::fs::read_dir(&files.0).unwrap().count(), paths.len());
    for (path, bytes) in paths.iter().zip(&original_bytes) {
        assert_eq!(&std::fs::read(path).unwrap(), bytes);
    }

    core.apply_stroke(&super::line_stroke(vec![StrokeSample {
        x: 0.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let edited_digest = core.document_state_digest().unwrap();
    assert!(core.document_info().unwrap().dirty);
    assert!(!core.editor_state().unwrap().dirty);
    core.undo().unwrap();
    assert!(!core.document_info().unwrap().dirty);
    core.redo().unwrap();
    assert!(core.document_info().unwrap().dirty);
    assert_eq!(core.document_state_digest().unwrap(), edited_digest);

    let native_path = files.0.join("edited.inkpod");
    save(&mut core, &manager, &native_path);
    assert!(!core.document_info().unwrap().dirty);
    assert!(!core.editor_state().unwrap().dirty);
    assert!(native_path.with_extension("tga").is_file());
    let mut reopened = Core::new();
    assert!(!reopened.open(&native_path).unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
    assert_eq!(reopened.document_state_digest().unwrap(), edited_digest);
    assert_eq!(reopened.history_entries(), core.history_entries());
    for (path, bytes) in paths.iter().zip(&original_bytes) {
        assert_eq!(&std::fs::read(path).unwrap(), bytes);
    }
    manager.shutdown_and_wait();
}

#[test]
fn io_003_parallel_sequence_attaches_without_reopening_or_losing_later_edits() {
    let files = Files::new();
    let seed = files.image("a001_tail.png", CommonRasterFormat::Png);
    files.image("A2_tail.tif", CommonRasterFormat::Tiff);
    files.image("a03_TAIL.bmp", CommonRasterFormat::Bmp);
    let manager = manager();
    let mut core = Core::new();
    open(&mut core, &manager, &seed);
    let mut job = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SequenceAuto, vec![seed]),
    )
    .unwrap();
    core.replace_palette(&[PixelValue::Rgba([11, 22, 33, 255])])
        .unwrap();
    core.update_editor_state(
        core.editor_state().unwrap().revision,
        EditorStateUpdate::SetToolDiameter {
            tool: EditorTool::Brush,
            diameter_q16: 37_i64 << 16,
        },
    )
    .unwrap();
    let edited = core.document_info().unwrap();
    let edited_editor = core.editor_state().unwrap();
    assert!(edited_editor.dirty);
    let progress = ready(&mut job);
    assert_eq!(progress.loaded_count, 3);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", job.error());
    job.apply(&mut core).unwrap();
    assert_eq!(core.document_info().unwrap(), edited);
    assert_eq!(core.editor_state().unwrap(), edited_editor);
    assert_eq!(
        core.sequence_cell(0).unwrap().document_uuid,
        edited.document_uuid
    );
    assert!(core.sequence_cell(2).is_ok());
    assert!(
        manager.cache_stats().cache_hits >= 1,
        "seed should come from shared cache"
    );
    manager.shutdown_and_wait();
}

#[test]
fn io_003_failed_automatic_neighbor_keeps_primary_and_current_state() {
    let files = Files::new();
    let seed = files.image("a1.png", CommonRasterFormat::Png);
    std::fs::write(files.0.join("a2.png"), b"corrupt").unwrap();
    let manager = manager();
    let mut core = Core::new();
    open(&mut core, &manager, &seed);
    let before = core.document_info().unwrap();
    let mut job = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SequenceAuto, vec![seed]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Failed);
    assert!(job.apply(&mut core).is_err());
    assert_eq!(core.document_info().unwrap(), before);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_stale_and_cross_core_open_results_never_publish() {
    let files = Files::new();
    let seed = files.image("a1.tga", CommonRasterFormat::Tga);
    let manager = manager();
    let mut core = Core::new();
    let mut job = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRaster, vec![seed]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).loaded_count, 1);
    let mut other = Core::new();
    assert!(job.apply(&mut other).is_err());
    assert_eq!(other.document_info(), Err(CoreError::NoDocument));
    core.new_cell(1, 1, 144_000, 144_000).unwrap();
    assert!(job.apply(&mut core).is_err());
    assert_eq!(core.document_info().unwrap().width, 1);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_pair_install_is_fenced_and_clean_save_repairs_missing_companion() {
    let files = Files::new();
    let seed = files.image("source.tif", CommonRasterFormat::Tiff);
    let destination = files.0.join("saved.inkpod");
    let manager = manager();
    let mut core = Core::new();
    open(&mut core, &manager, &seed);
    let mut job = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SavePair, vec![destination.clone()]),
    )
    .unwrap();
    assert_eq!(
        ready(&mut job).state,
        FileIoState::Ready,
        "{:?}",
        job.error()
    );
    assert!(
        !destination.exists(),
        "preparation must not publish a destination"
    );
    let editor_before = core.editor_state().unwrap();
    let editor_frame = core.editor_state_frame().unwrap();
    let editor_savepoint = core.editor_savepoint_token().unwrap();
    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert!(core.new_cell(1, 1, 144_000, 144_000).is_err());
    assert!(matches!(
        core.update_editor_state(
            editor_before.revision,
            EditorStateUpdate::SetActiveTool(EditorTool::BoxZoom),
        ),
        Err(CoreError::InvalidState(_))
    ));
    assert!(matches!(
        core.restore_editor_state_frame(&editor_frame, EditorFrameDisposition::Saved),
        Err(CoreError::InvalidState(_))
    ));
    assert!(matches!(
        core.commit_editor_savepoint(editor_savepoint),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(core.editor_state().unwrap(), editor_before);
    assert_eq!(core.editor_state_frame().unwrap(), editor_frame);
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    job.apply(&mut core).unwrap();
    assert!(!job.requires_finalization());
    core.update_editor_state(
        editor_before.revision,
        EditorStateUpdate::SetActiveTool(editor_before.state.active_tool),
    )
    .unwrap();
    core.restore_editor_state_frame(&editor_frame, EditorFrameDisposition::Saved)
        .unwrap();
    core.commit_editor_savepoint(core.editor_savepoint_token().unwrap())
        .unwrap();
    assert!(!core.document_info().unwrap().dirty);
    assert!(destination.with_extension("tif").is_file());
    std::fs::remove_file(destination.with_extension("tif")).unwrap();
    let before = core.document_info().unwrap();
    save(&mut core, &manager, &destination);
    assert!(destination.with_extension("tif").is_file());
    assert_eq!(
        core.document_info().unwrap().document_revision,
        before.document_revision
    );
    let mut reopened = Core::new();
    let mut open = FileIoJob::start(
        Some(&reopened),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![destination]),
    )
    .unwrap();
    assert_eq!(
        ready(&mut open).state,
        FileIoState::Ready,
        "{:?}",
        open.error()
    );
    open.apply(&mut reopened).unwrap();
    assert_eq!(
        reopened.raster_file_format().unwrap(),
        CommonRasterFormat::Tiff
    );
    manager.shutdown_and_wait();
}

#[test]
fn io_003_external_pair_change_requires_explicit_renewed_confirmation() {
    let files = Files::new();
    let destination = files.0.join("saved.inkpod");
    let manager = manager();
    let mut core = Core::new();
    core.new_cell(1, 1, 144_000, 144_000).unwrap();
    save(&mut core, &manager, &destination);
    let before = core.document_info().unwrap();
    std::fs::write(destination.with_extension("png"), b"external change").unwrap();
    let mut job = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SavePair, vec![destination.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Failed);
    assert_eq!(job.error(), Some(&CoreError::FileConflict));
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(
        std::fs::read(destination.with_extension("png")).unwrap(),
        b"external change"
    );
    let mut request = FileIoRequest::new(FileIoKind::SavePair, vec![destination]);
    request.overwrite_confirmed = true;
    let mut job = FileIoJob::start(Some(&core), manager.clone(), request).unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    job.apply(&mut core).unwrap();
    ready(&mut job);
    job.apply(&mut core).unwrap();
    manager.shutdown_and_wait();
}

#[test]
fn io_003_reference_catalog_keeps_resident_cache_and_rejects_failed_replacement() {
    let files = Files::new();
    let first = files.image("one.png", CommonRasterFormat::Png);
    let second = files.image("two.bmp", CommonRasterFormat::Bmp);
    let manager = manager();
    let mut catalog = SubpaletteCatalog::new().unwrap();
    let mut job = FileIoJob::start(
        None,
        manager.clone(),
        FileIoRequest::new(FileIoKind::ReferenceFiles, vec![first.clone(), second]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).loaded_count, 2);
    let before = job.apply_reference(&mut catalog).unwrap();
    assert_eq!(before.item_count, 2);
    let reads = manager.cache_stats().physical_reads;
    std::fs::write(&first, b"corrupt").unwrap();
    let mut replacement = FileIoRequest::new(FileIoKind::ReferenceFiles, vec![first]);
    replacement.force_reload = true;
    let mut replacement = FileIoJob::start(None, manager.clone(), replacement).unwrap();
    assert_eq!(ready(&mut replacement).state, FileIoState::Failed);
    assert!(replacement.apply_reference(&mut catalog).is_err());
    assert_eq!(catalog.info(), before);
    let after_failure = manager.cache_stats().physical_reads;
    assert!(after_failure > reads);
    // Navigation works exclusively from the old resident candidate.
    let second_id = catalog.item(1).unwrap().id;
    let first_id = catalog.item(0).unwrap().id;
    catalog.select_cached_image(second_id).unwrap();
    catalog.select_cached_image(first_id).unwrap();
    assert_eq!(manager.cache_stats().physical_reads, after_failure);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_reference_snapshot_and_tile_clones_keep_derived_pixels_charged_until_release() {
    let files = Files::new();
    let path = files.image("small.png", CommonRasterFormat::Png);
    let manager = IoManager::new(IoConfig {
        worker_count: 1,
        // Source RGBA: 16 bytes. One visible tile: 16 bytes. Converting its
        // temporary Vec into immutable Arc storage needs 16 additional bytes.
        max_decoded_bytes: 48,
        ..IoConfig::default()
    })
    .unwrap();
    let mut job = FileIoJob::start(
        None,
        manager.clone(),
        FileIoRequest::new(FileIoKind::ReferenceFiles, vec![path]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    let mut catalog = SubpaletteCatalog::new().unwrap();
    job.apply_reference(&mut catalog).unwrap();
    drop(job);
    catalog
        .apply_view(ViewCommand::OneToOne {
            viewport_width: 2.0,
            viewport_height: 2.0,
        })
        .unwrap();
    // Catalog publication already verified and retained its first display tiles.
    assert_eq!(manager.cache_stats().decoded_bytes, 32);
    let snapshot = catalog.build_snapshot().unwrap();
    assert_eq!(snapshot.tiles().len(), 1);
    assert_eq!(manager.cache_stats().decoded_bytes, 32);
    assert_eq!(catalog.build_snapshot().unwrap(), snapshot);
    let snapshot_clone = snapshot.clone();
    let tile_clone = snapshot.tiles()[0].clone();
    assert_eq!(manager.cache_stats().decoded_bytes, 32);

    catalog
        .apply_view(ViewCommand::PanBy {
            device_dx: 10.0,
            device_dy: 0.0,
        })
        .unwrap();
    assert!(catalog.build_snapshot().unwrap().tiles().is_empty());
    manager.clear_cache();
    catalog
        .apply_view(ViewCommand::PanBy {
            device_dx: -10.0,
            device_dy: 0.0,
        })
        .unwrap();
    let before = catalog.info();
    assert!(matches!(
        catalog.build_snapshot(),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(catalog.info(), before);
    assert_eq!(manager.cache_stats().decoded_bytes, 32);
    drop(snapshot);
    assert_eq!(manager.cache_stats().decoded_bytes, 32);
    drop(snapshot_clone);
    assert!(matches!(
        catalog.build_snapshot(),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(tile_clone.pixels(), [30, 20, 10, 255].repeat(4));
    drop(tile_clone);
    assert_eq!(manager.cache_stats().decoded_bytes, 16);

    let rebuilt = catalog.build_snapshot().unwrap();
    let retained_tile = rebuilt.tiles()[0].clone();
    assert_eq!(manager.cache_stats().decoded_bytes, 32);
    drop(rebuilt);
    drop(catalog);
    manager.clear_cache();
    assert_eq!(manager.cache_stats().encoded_bytes, 0);
    assert_eq!(manager.cache_stats().decoded_bytes, 16);
    assert_eq!(manager.cache_stats().images, 1);
    assert_eq!(retained_tile.pixels(), [30, 20, 10, 255].repeat(4));
    drop(retained_tile);
    assert_eq!(manager.cache_stats().decoded_bytes, 0);
    assert_eq!(manager.cache_stats().images, 0);
    manager.shutdown_and_wait();
}

#[test]
fn sequence_render_snapshot_and_tile_clones_keep_managed_reservations_after_core_drop() {
    let files = Files::new();
    let path = files.image("cell1.png", CommonRasterFormat::Png);
    let manager = manager();
    let image = manager
        .read_image(&path, &inkpod_io::JobContext::new())
        .unwrap();
    let source = SequenceCellSource::from_loaded_image(&manager, &image, 0x7f01).unwrap();
    drop(image);
    let mut core = Core::new();
    core.new_cell_with_uuid(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x7f00)
        .unwrap();
    core.set_sequence(vec![source]).unwrap();
    core.sequence_activate(0).unwrap();
    let snapshot = core.build_snapshot();
    let snapshot_clone = snapshot.clone();
    let tile = snapshot.tiles()[0].clone();
    assert_eq!(manager.cache_stats().sequence_render_allocations, 1);
    assert_eq!(manager.cache_stats().sequence_render_bytes, 16);
    drop(core);
    manager.clear_cache();
    assert_eq!(manager.cache_stats().decoded_bytes, 16);
    assert_eq!(manager.cache_stats().sequence_render_bytes, 16);
    drop(snapshot);
    drop(snapshot_clone);
    assert_eq!(manager.cache_stats().sequence_render_allocations, 1);
    assert_eq!(tile.pixels(), [30, 20, 10, 255].repeat(4));
    drop(tile);
    assert_eq!(manager.cache_stats().sequence_render_bytes, 0);
    assert_eq!(manager.cache_stats().sequence_render_allocations, 0);
    assert_eq!(manager.cache_stats().decoded_bytes, 0);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_reference_replacement_reserves_initial_display_before_publishing_catalog() {
    let files = Files::new();
    let old_path = files.image("old.png", CommonRasterFormat::Png);
    let new_path = files.image("new.png", CommonRasterFormat::Png);
    let manager = IoManager::new(IoConfig {
        worker_count: 1,
        // Both sources and the old display fit. The new display's Vec/Arc
        // overlap needs 80 bytes and must not replace the working catalog.
        max_decoded_bytes: 64,
        ..IoConfig::default()
    })
    .unwrap();
    let mut catalog = SubpaletteCatalog::new().unwrap();
    let mut initial = FileIoJob::start(
        None,
        manager.clone(),
        FileIoRequest::new(FileIoKind::ReferenceFiles, vec![old_path.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut initial).state, FileIoState::Ready);
    initial.apply_reference(&mut catalog).unwrap();
    drop(initial);
    let snapshot = catalog.build_snapshot().unwrap();
    let before = catalog.info();
    let old_id = catalog.item(0).unwrap().id;
    assert_eq!(manager.cache_stats().decoded_bytes, 32);

    let mut replacement = FileIoJob::start(
        None,
        manager.clone(),
        FileIoRequest::new(FileIoKind::ReferenceFiles, vec![new_path]),
    )
    .unwrap();
    assert_eq!(ready(&mut replacement).state, FileIoState::Ready);
    assert_eq!(manager.cache_stats().decoded_bytes, 48);
    assert!(matches!(
        replacement.apply_reference(&mut catalog),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(replacement.poll().state, FileIoState::Failed);
    assert_eq!(catalog.info(), before);
    assert_eq!(catalog.item(0).unwrap().id, old_id);
    assert_eq!(catalog.build_snapshot().unwrap(), snapshot);
    assert_eq!(
        catalog.sample(0.5, 0.5).unwrap(),
        PixelValue::Rgba([10, 20, 30, 255])
    );
    drop(replacement);

    // Reusing the old source needs only new display pixels; the unpinned failed
    // source can be evicted while the old renderer snapshot remains leased.
    let mut shared_source = FileIoJob::start(
        None,
        manager.clone(),
        FileIoRequest::new(FileIoKind::ReferenceFiles, vec![old_path]),
    )
    .unwrap();
    assert_eq!(ready(&mut shared_source).state, FileIoState::Ready);
    shared_source.apply_reference(&mut catalog).unwrap();
    assert_eq!(catalog.item(0).unwrap().id.get(), old_id.get() + 1);
    assert_eq!(manager.cache_stats().decoded_bytes, 48);
    assert_eq!(snapshot.tiles()[0].pixels(), [30, 20, 10, 255].repeat(4));
    drop(shared_source);
    drop(catalog);
    drop(snapshot);
    manager.clear_cache();
    assert_eq!(manager.cache_stats().decoded_bytes, 0);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_public_core_clone_has_independent_file_authority() {
    let mut core = Core::new();
    core.new_cell(2, 2, 144_000, 144_000).unwrap();
    let (_, token) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap();
    let cloned_token = token.clone();
    let cloned = core.clone();
    assert_eq!(
        cloned.document_info().unwrap(),
        core.document_info().unwrap()
    );
    assert_eq!(cloned.history_entries(), core.history_entries());
    assert!(matches!(
        cloned.validate_document_save(&token),
        Err(CoreError::InvalidState(_))
    ));
    core.validate_document_save(&token).unwrap();
    core.validate_document_save(&cloned_token).unwrap();
}

#[test]
fn io_003_core_cloned_during_install_cannot_finalize_or_inherit_original_fence() {
    let files = Files::new();
    let manager = serial_manager();
    let mut core = Core::new();
    core.new_cell(2, 2, 144_000, 144_000).unwrap();
    let before = core.document_info().unwrap();
    let destination = files.0.join("original.inkpod");
    let mut job = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SavePair, vec![destination.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    let gate = WorkerGate::new(&manager);
    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    let mut cloned = core.clone();
    assert_eq!(cloned.document_info().unwrap(), before);
    gate.release();
    assert_eq!(ready(&mut job).state, FileIoState::Ready);

    // Identical document/editor state must not let another runtime consume the
    // final result and leave the original owner permanently fenced.
    assert!(matches!(
        job.apply(&mut cloned),
        Err(CoreError::InvalidState(_))
    ));
    assert!(job.requires_finalization());
    assert_eq!(job.poll().state, FileIoState::Ready);
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(cloned.document_info().unwrap(), before);
    assert!(core.new_cell(1, 1, 144_000, 144_000).is_err());
    cloned.new_cell(1, 1, 144_000, 144_000).unwrap();

    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert!(!job.requires_finalization());
    assert!(!core.document_info().unwrap().dirty);
    assert!(destination.with_extension("png").is_file());
    core.new_cell(3, 3, 144_000, 144_000).unwrap();
    manager.shutdown_and_wait();
}

#[test]
fn io_003_noop_edit_and_batch_staging_preserve_original_file_authority() {
    let manager = serial_manager();
    let mut core = Core::new();
    core.new_cell(2, 2, 144_000, 144_000).unwrap();
    let before = core.document_info().unwrap();
    let (_, token) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap();
    core.set_main_line_color(core.main_line_color().unwrap())
        .unwrap();
    assert_eq!(core.document_info().unwrap(), before);
    core.validate_document_save(&token).unwrap();

    let mut graph = batch_graph(
        vec![BatchInputSelector::active_document()],
        BatchOutputSettings {
            destination: BatchOutputDestination::ActiveDocument,
            ..BatchOutputSettings::default()
        },
    );
    graph.operations[0].kind = BatchOperationKind::ColorReplace(vec![BatchColorPair {
        enabled: true,
        old: PixelValue::Rgba([1, 2, 3, 255]),
        new: PixelValue::Rgba([20, 40, 60, 255]),
    }]);
    let mut job = FileIoJob::start_batch(
        &core,
        manager.clone(),
        graph,
        FileIoKind::BatchRun,
        batch_options(),
        0,
    )
    .unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert_eq!(job.take_batch_report().unwrap().failure_count(), 0);
    assert_eq!(core.document_info().unwrap(), before);
    core.validate_document_save(&token).unwrap();
    manager.shutdown_and_wait();
}
