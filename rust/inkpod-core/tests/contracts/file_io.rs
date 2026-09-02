//! IO-003 public asynchronous filesystem publication contracts.
use inkpod_core::*;
use inkpod_format::{BATCH_GRAPH_VERSION, CommonRaster, encode_common_raster};
use inkpod_io::{IoConfig, IoManager, RecoveryArtifactProof};
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
        self.image_with_dpi(name, format, Some(144_000))
    }
    fn image_with_dpi(
        &self,
        name: &str,
        format: CommonRasterFormat,
        dpi_milli: Option<u32>,
    ) -> PathBuf {
        let path = self.0.join(name);
        let raster = CommonRaster::new(
            2,
            2,
            PixelFormat::StraightRgba8,
            dpi_milli,
            dpi_milli,
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

fn finish_manager_cleanup(manager: &IoManager) {
    let barrier = manager.submit(|_| Ok(())).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(result) = barrier.try_take() {
            result.unwrap();
            return;
        }
        assert!(
            Instant::now() < deadline,
            "asynchronous file cleanup did not finish"
        );
        std::thread::yield_now();
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

fn save_paths(
    core: &mut Core,
    manager: &IoManager,
    paths: Vec<PathBuf>,
    overwrite_confirmed: bool,
) {
    let mut request = FileIoRequest::new(FileIoKind::SavePair, paths);
    request.overwrite_confirmed = overwrite_confirmed;
    let mut job = FileIoJob::start(Some(core), manager.clone(), request).unwrap();
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
}

fn write_recovery(
    core: &Core,
    manager: &IoManager,
    path: &Path,
    source_path: &Path,
    generation: u64,
) -> RecoveryArtifactProof {
    let document = core.document_info().unwrap();
    let metadata =
        recovery_metadata_for_pair(manager, document.document_uuid, source_path, generation);
    let (native, _) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap();
    manager
        .write_recovery(path, &metadata, &inkpod_io::JobContext::new(), |writer| {
            inkpod_format::write_procedure_to_writer(writer, &native, || false)?;
            Ok(())
        })
        .unwrap()
}

fn recovery_metadata_for_pair(
    manager: &IoManager,
    document_uuid: u128,
    source_path: &Path,
    generation: u64,
) -> inkpod_io::RecoveryMetadata {
    let native_path = source_path.with_extension("inkpod");
    let context = inkpod_io::JobContext::new();
    let native = manager.metadata(&native_path, &context).unwrap();
    let raster = manager.metadata(source_path, &context).unwrap();
    inkpod_io::RecoveryMetadata {
        session_id: 1,
        generation,
        document_uuid,
        original_identity: inkpod_io::RecoveryIdentity {
            kind: inkpod_io::RecoveryIdentityKind::PhysicalFile,
            volume_serial: native.identity.volume,
            file_id: native.identity.file.to_le_bytes(),
            ..inkpod_io::RecoveryIdentity::default()
        },
        original_path: native_path.to_string_lossy().into_owned(),
        source_path: source_path.to_string_lossy().into_owned(),
        pair_proof: Some(inkpod_io::RecoveryPairProof::Committed { native, raster }),
        written_time_100ns: 123,
    }
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

fn assert_normal_pair_authority_retained(
    core: &mut Core,
    manager: &IoManager,
    path: &Path,
    old_save: &DocumentSaveToken,
) {
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
    assert_eq!(
        ready(&mut save).state,
        FileIoState::Ready,
        "{:?}",
        save.error()
    );
    assert_eq!(save.poll().result_count, 2);
    assert_eq!(save.item(0).unwrap().path, path);
    assert!(save.item(0).unwrap().identity_physical);
    assert!(save.item(1).unwrap().identity_physical);
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
fn io_003_save_pair_ready_advertises_both_future_final_identities() {
    let files = Files::new();
    let manager = serial_manager();
    let native = files.0.join("future.inkpod");
    let raster = files.0.join("future.png");
    let mut core = Core::new();
    core.new_cell(2, 2, 144_000, 144_000).unwrap();
    let mut job = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SavePair, vec![native.clone()]),
    )
    .unwrap();

    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    assert!(!job.requires_finalization());
    let document_uuid = core.document_info().unwrap().document_uuid;
    let advertised_native = job.item(0).unwrap().identity;
    let advertised_raster = job.item(1).unwrap().identity;
    let native_item_address = job.item(0).unwrap() as *const FileIoItem;
    let raster_item_address = job.item(1).unwrap() as *const FileIoItem;
    let native_name_address = job.item(0).unwrap().name.as_ptr();
    let raster_name_address = job.item(1).unwrap().name.as_ptr();
    assert!(job.item(0).unwrap().identity_physical);
    assert!(job.item(1).unwrap().identity_physical);
    assert_eq!(job.item(0).unwrap().path, native);
    assert_eq!(job.item(1).unwrap().path, raster);
    assert_eq!(job.item(0).unwrap().name, "future.inkpod");
    assert_eq!(job.item(1).unwrap().name, "future.png");
    assert_eq!(job.item(0).unwrap().format, None);
    assert_eq!(job.item(1).unwrap().format, Some(CommonRasterFormat::Png));
    for item in [job.item(0).unwrap(), job.item(1).unwrap()] {
        assert_eq!(item.source_generation, 1);
        assert_eq!(item.document_uuid, document_uuid);
    }
    assert!(!native.exists());
    assert!(!raster.exists());

    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    assert!(job.requires_finalization());
    let (native_identity, native_physical) = manager.resolve_identity(&native).unwrap();
    let (raster_identity, raster_physical) = manager.resolve_identity(&raster).unwrap();
    assert!(native_physical && raster_physical);
    assert_eq!(native_identity, advertised_native);
    assert_eq!(raster_identity, advertised_raster);
    assert_eq!(job.item(0).unwrap().identity, advertised_native);
    assert_eq!(job.item(1).unwrap().identity, advertised_raster);

    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert_eq!(job.item(0).unwrap().identity, advertised_native);
    assert_eq!(job.item(1).unwrap().identity, advertised_raster);
    assert_eq!(
        job.item(0).unwrap() as *const FileIoItem,
        native_item_address
    );
    assert_eq!(
        job.item(1).unwrap() as *const FileIoItem,
        raster_item_address
    );
    assert_eq!(job.item(0).unwrap().name.as_ptr(), native_name_address);
    assert_eq!(job.item(1).unwrap().name.as_ptr(), raster_name_address);
    assert!(!job.requires_finalization());
    manager.shutdown_and_wait();
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
    let recovery_proof = *job.recovery_artifact_proof().unwrap();
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
    assert_eq!(stored.session_id, metadata.session_id);
    assert_eq!(stored.generation, metadata.generation);
    assert_eq!(stored.document_uuid, metadata.document_uuid);
    assert_eq!(stored.original_path, normal.to_string_lossy());
    assert_eq!(
        stored.source_path,
        normal.with_extension("tif").to_string_lossy()
    );
    assert!(matches!(
        stored.pair_proof,
        Some(inkpod_io::RecoveryPairProof::Committed { .. })
    ));
    assert_eq!(std::fs::read(&normal).unwrap(), normal_bytes);
    assert!(!recovery.with_extension("tif").exists());
    let recovery_bytes = std::fs::read(&recovery).unwrap();
    let recovery_file_count = std::fs::read_dir(recovery.parent().unwrap())
        .unwrap()
        .count();
    // Recovery publication is append-only. Simulate an external same-path
    // replacement by removing the old generation before creating a new one;
    // the retained old proof must no longer authorize a restore.
    manager
        .discard_recovery(&recovery, &inkpod_io::JobContext::new())
        .unwrap();
    let replacement_proof = manager
        .write_recovery(
            &recovery,
            &stored,
            &inkpod_io::JobContext::new(),
            |writer| {
                std::io::Write::write_all(writer, &recovery_bytes)?;
                Ok(())
            },
        )
        .unwrap();
    assert_ne!(replacement_proof, recovery_proof);

    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let before_stale_proof = core.document_info().unwrap();
    let mut stale_proof = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((recovery.clone(), recovery_proof)),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut stale_proof).state, FileIoState::Failed);
    assert_eq!(stale_proof.error(), Some(&CoreError::FileConflict));
    assert_eq!(core.document_info().unwrap(), before_stale_proof);
    let mut restored = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((recovery.clone(), replacement_proof)),
        None,
    )
    .unwrap();
    assert_eq!(
        ready(&mut restored).state,
        FileIoState::Ready,
        "{:?}",
        restored.error()
    );
    assert_eq!(restored.poll().result_count, 2);
    assert_eq!(
        restored.item(0).unwrap().format,
        Some(CommonRasterFormat::Tiff)
    );
    assert_eq!(restored.item(1).unwrap().format, None);
    for item in [restored.item(0).unwrap(), restored.item(1).unwrap()] {
        assert!(item.identity_physical);
        assert_eq!(item.document_uuid, request.target_document_uuid);
    }
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
    assert_normal_pair_authority_retained(&mut core, &manager, &normal, &old_save);
    assert_eq!(
        std::fs::read(normal.with_extension("tif")).unwrap(),
        raster_bytes
    );
    assert_eq!(std::fs::read(normal).unwrap(), normal_bytes);
    finish_manager_cleanup(&manager);
    assert!(!core.revert().unwrap().recovered);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_sequence_target_recovery_rejects_a_coherent_external_pair_save() {
    let files = Files::new();
    let manager = serial_manager();
    let normal = files.0.join("coherent.inkpod");
    let recovery = files.0.join("recovery").join("coherent.inkpod");
    let mut core = sequence_core();
    save(&mut core, &manager, &normal);
    core.set_main_line_color(PixelValue::Rgba([10, 20, 30, 255]))
        .unwrap();
    let source_uuid = core.document_info().unwrap().document_uuid;
    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let metadata = inkpod_io::RecoveryMetadata {
        session_id: 11,
        generation: 1,
        document_uuid: source_uuid,
        written_time_100ns: 123,
        ..inkpod_io::RecoveryMetadata::default()
    };
    let mut switch = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(recovery.clone()),
        None,
        Some(metadata),
    )
    .unwrap();
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    assert!(matches!(
        switch.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    let recovery_proof = *switch.recovery_artifact_proof().unwrap();
    assert!(matches!(
        switch.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    let captured = manager
        .read_recovery_metadata(&recovery, &inkpod_io::JobContext::new())
        .unwrap();
    let old_pair = captured.pair_proof.unwrap();

    // A second owner opens the exact same coherent pair, creates a different
    // history/composite with the same UUID and Genesis, then normally saves it.
    let mut external = Core::new();
    let mut open = FileIoJob::start(
        Some(&external),
        manager.clone(),
        FileIoRequest::new(
            FileIoKind::OpenRasterPair,
            vec![normal.with_extension("tif")],
        ),
    )
    .unwrap();
    assert_eq!(ready(&mut open).state, FileIoState::Ready);
    assert!(matches!(
        open.apply(&mut external).unwrap(),
        FileIoApply::Complete { .. }
    ));
    let document = external.document_info().unwrap();
    external
        .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            stroke: Stroke {
                tool: PaintTool::Pencil,
                plane: ActivePlane::Color,
                color: [80, 90, 100, 255],
                diameter: 1.0,
                shape: BrushShape::Round,
                smoothing: 0,
                start_color: StartColorPredicate::Any,
                auto_erase: false,
                pressure_size: false,
                coordinate_space: CoordinateSpace::Document,
                samples: vec![StrokeSample {
                    x: 0.0,
                    y: 0.0,
                    pressure: 1.0,
                }],
            },
        })
        .unwrap();
    save(&mut external, &manager, &normal);
    let current_native = manager
        .metadata(&normal, &inkpod_io::JobContext::new())
        .unwrap();
    let current_raster = manager
        .metadata(&normal.with_extension("tif"), &inkpod_io::JobContext::new())
        .unwrap();
    assert!(match old_pair {
        inkpod_io::RecoveryPairProof::Committed { native, raster } => {
            native != current_native && raster != current_raster
        }
        inkpod_io::RecoveryPairProof::Planned { .. }
        | inkpod_io::RecoveryPairProof::RepairNeeded { .. } => false,
    });

    // Even a sidecar rewritten to claim the replacement pair's exact current
    // stamps cannot attach that authority to recovery history whose encoded
    // savepoint is based on the previous pair. Pair identity and UUID/Genesis
    // checks alone would accept this lost-update case.
    let forged_recovery = files.0.join("recovery").join("forged.inkpod");
    let recovery_bytes = std::fs::read(&recovery).unwrap();
    let mut forged_metadata = captured.clone();
    forged_metadata.pair_proof = Some(inkpod_io::RecoveryPairProof::Committed {
        native: current_native,
        raster: current_raster,
    });
    let forged_artifact_proof = manager
        .write_recovery(
            &forged_recovery,
            &forged_metadata,
            &inkpod_io::JobContext::new(),
            |writer| {
                std::io::Write::write_all(writer, &recovery_bytes)?;
                Ok(())
            },
        )
        .unwrap();

    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let before = core.document_info().unwrap();
    let before_editor = core.editor_state().unwrap();
    let before_history = core.history_entries().to_vec();
    let mut restore = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((recovery, recovery_proof)),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut restore).state, FileIoState::Failed);
    assert_eq!(restore.error(), Some(&CoreError::FileConflict));
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.editor_state().unwrap(), before_editor);
    assert_eq!(core.history_entries(), before_history);

    let mut forged_restore = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((forged_recovery, forged_artifact_proof)),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut forged_restore).state, FileIoState::Failed);
    assert_eq!(forged_restore.error(), Some(&CoreError::FileConflict));
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.editor_state().unwrap(), before_editor);
    assert_eq!(core.history_entries(), before_history);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_normal_pair_clean_history_freshness_preserves_redo_across_sequence_switch() {
    let files = Files::new();
    let manager = serial_manager();
    let normal = files.0.join("normal-clean-history.inkpod");
    let recovery = files.0.join("recovery").join("normal-clean-history.inkpod");
    let mut core = sequence_core();
    save(&mut core, &manager, &normal);
    let no_op = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(!no_op.requires_switch());
    assert!(!no_op.requires_source_recovery());
    let baseline_digest = core.document_state_digest().unwrap();

    core.set_main_line_color(PixelValue::Rgba([21, 22, 23, 255]))
        .unwrap();
    let edited_digest = core.document_state_digest().unwrap();
    core.undo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), baseline_digest);
    let info = core.document_info().unwrap();
    assert!(!info.dirty && !info.recovered && info.can_redo);

    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(request.requires_source_recovery());
    assert!(matches!(
        FileIoJob::start_sequence_switch(&core, manager.clone(), request, None, None, None,),
        Err(CoreError::InvalidArgument(
            "sequence source requires a recovery destination"
        ))
    ));
    let metadata = inkpod_io::RecoveryMetadata {
        session_id: 16,
        generation: 1,
        document_uuid: info.document_uuid,
        written_time_100ns: 125,
        ..inkpod_io::RecoveryMetadata::default()
    };
    let mut switch = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(recovery.clone()),
        None,
        Some(metadata),
    )
    .unwrap();
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    assert!(matches!(
        switch.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    let proof = *switch.recovery_artifact_proof().unwrap();
    switch.apply(&mut core).unwrap();

    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(!request.requires_source_recovery());
    let mut restore = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((recovery, proof)),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut restore).state, FileIoState::Ready);
    restore.apply(&mut core).unwrap();
    let info = core.document_info().unwrap();
    assert!(!info.dirty && !info.recovered && info.can_redo);
    assert_eq!(core.document_state_digest().unwrap(), baseline_digest);
    core.redo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), edited_digest);
    save(&mut core, &manager, &normal);
    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(!request.requires_source_recovery());
    manager.shutdown_and_wait();
}

#[test]
fn io_003_clean_recovered_source_refreshes_binding_and_preserves_redo() {
    let files = Files::new();
    let manager = serial_manager();
    let normal = files.0.join("clean-retire.inkpod");
    let recovery = files.0.join("recovery").join("dirty-generation.inkpod");
    let clean_recovery = files.0.join("recovery").join("clean-generation.inkpod");
    let branch_recovery = files.0.join("recovery").join("branch-generation.inkpod");
    let mut core = sequence_core();
    save(&mut core, &manager, &normal);
    let baseline_digest = core.document_state_digest().unwrap();
    core.set_main_line_color(PixelValue::Rgba([31, 32, 33, 255]))
        .unwrap();
    let dirty_digest = core.document_state_digest().unwrap();
    assert_ne!(dirty_digest, baseline_digest);

    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let metadata = inkpod_io::RecoveryMetadata {
        session_id: 15,
        generation: 1,
        document_uuid: core.document_info().unwrap().document_uuid,
        written_time_100ns: 123,
        ..inkpod_io::RecoveryMetadata::default()
    };
    let mut switch = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(recovery.clone()),
        None,
        Some(metadata),
    )
    .unwrap();
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    assert!(matches!(
        switch.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    let artifact_proof = *switch.recovery_artifact_proof().unwrap();
    switch.apply(&mut core).unwrap();
    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut restore = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((recovery.clone(), artifact_proof)),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut restore).state, FileIoState::Ready);
    restore.apply(&mut core).unwrap();
    assert_eq!(core.document_state_digest().unwrap(), dirty_digest);
    assert!(core.document_info().unwrap().dirty);
    assert!(core.document_info().unwrap().recovered);

    core.undo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), baseline_digest);
    assert!(!core.document_info().unwrap().dirty);
    let retained_recovery = std::fs::read(&recovery).unwrap();

    // Reaching the clean savepoint through Undo does not retire RECOVERED:
    // its redo tail still lives only in the current Core/recovery generation.
    // Sequence navigation therefore requires a fresh append-only artifact.
    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(matches!(
        FileIoJob::start_sequence_switch(&core, manager.clone(), request, None, None, None,),
        Err(CoreError::InvalidArgument(
            "sequence source requires a recovery destination"
        ))
    ));

    let metadata = inkpod_io::RecoveryMetadata {
        session_id: 15,
        generation: 2,
        document_uuid: core.document_info().unwrap().document_uuid,
        written_time_100ns: 124,
        ..inkpod_io::RecoveryMetadata::default()
    };
    let mut clean_switch = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(clean_recovery.clone()),
        None,
        Some(metadata),
    )
    .unwrap();
    assert_eq!(ready(&mut clean_switch).state, FileIoState::Ready);
    assert!(matches!(
        clean_switch.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut clean_switch).state, FileIoState::Ready);
    let clean_artifact_proof = *clean_switch.recovery_artifact_proof().unwrap();
    assert!(matches!(
        clean_switch.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    // Core never mutates the retired generation: the frontend swaps its
    // generation-bound binding, then discards this exact artifact separately.
    assert_eq!(std::fs::read(&recovery).unwrap(), retained_recovery);

    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut clean_revisit = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((clean_recovery.clone(), clean_artifact_proof)),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut clean_revisit).state, FileIoState::Ready);
    clean_revisit.apply(&mut core).unwrap();
    assert_eq!(core.document_state_digest().unwrap(), baseline_digest);
    assert_ne!(core.document_state_digest().unwrap(), dirty_digest);
    assert!(!core.document_info().unwrap().dirty);
    assert!(!core.document_info().unwrap().recovered);
    assert!(core.document_info().unwrap().can_redo);

    // Replace the old redo tail with a new branch, then Undo to the same clean
    // visible savepoint. RECOVERED is now false, so only the runtime baseline
    // can prove that the serializable branch graph is newer than generation 2.
    core.set_main_line_color(PixelValue::Rgba([61, 62, 63, 255]))
        .unwrap();
    let branch_digest = core.document_state_digest().unwrap();
    assert_ne!(branch_digest, dirty_digest);
    core.undo().unwrap();
    let info = core.document_info().unwrap();
    assert!(!info.dirty && !info.recovered && info.can_redo);
    assert_eq!(core.document_state_digest().unwrap(), baseline_digest);
    let retained_clean_recovery = std::fs::read(&clean_recovery).unwrap();
    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(request.requires_source_recovery());
    let metadata = inkpod_io::RecoveryMetadata {
        session_id: 15,
        generation: 3,
        document_uuid: info.document_uuid,
        written_time_100ns: 126,
        ..inkpod_io::RecoveryMetadata::default()
    };
    let mut branch_switch = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(branch_recovery.clone()),
        None,
        Some(metadata),
    )
    .unwrap();
    assert_eq!(ready(&mut branch_switch).state, FileIoState::Ready);
    assert!(matches!(
        branch_switch.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut branch_switch).state, FileIoState::Ready);
    let branch_proof = *branch_switch.recovery_artifact_proof().unwrap();
    branch_switch.apply(&mut core).unwrap();
    assert_eq!(
        std::fs::read(&clean_recovery).unwrap(),
        retained_clean_recovery
    );

    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut branch_revisit = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((branch_recovery, branch_proof)),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut branch_revisit).state, FileIoState::Ready);
    branch_revisit.apply(&mut core).unwrap();
    let info = core.document_info().unwrap();
    assert!(!info.dirty && !info.recovered && info.can_redo);
    assert_eq!(core.document_state_digest().unwrap(), baseline_digest);
    core.redo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), branch_digest);
    assert_eq!(
        core.redo(),
        Err(CoreError::InvalidState("there is no command to redo"))
    );
    manager.shutdown_and_wait();
}

#[test]
fn io_003_standalone_recovery_sequence_round_trip_keeps_authority_none() {
    let files = Files::new();
    let manager = serial_manager();
    let standalone = files.0.join("standalone.inkpod");
    let sequence_recovery = files.0.join("attempts").join("cell0.inkpod");
    let original = sequence_core();
    original.autosave(&standalone).unwrap();

    let mut core = Core::new();
    core.new_cell(1, 1, 96_000, 96_000).unwrap();
    let mut open = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRecovery, vec![standalone.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut open).state, FileIoState::Ready);
    assert!(matches!(
        open.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert!(core.document_info().unwrap().recovered);
    assert!(core.document_info().unwrap().dirty);
    let restored_uuid = core.document_info().unwrap().document_uuid;
    let raster = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(144_000),
        Some(144_000),
        [1, 2, 3, 255].repeat(4),
    )
    .unwrap();
    let first =
        SequenceCellSource::from_common_raster("cell1.tif", restored_uuid, &raster).unwrap();
    let mut second = SequenceCellSource::from_common_raster("cell2.bmp", 0x1020, &raster).unwrap();
    second.raster_file_format = CommonRasterFormat::Bmp;
    core.set_sequence(vec![first, second]).unwrap();
    assert_eq!(
        core.revert(),
        Err(CoreError::InvalidState("document has no normal-save path"))
    );
    let digest = core.document_state_digest().unwrap();
    let history = core.history_entries().to_vec();
    let editor = core.editor_state().unwrap();
    let (_, old_save) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(false, || false)
        .unwrap();

    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let metadata = inkpod_io::RecoveryMetadata {
        session_id: 12,
        generation: 1,
        document_uuid: core.document_info().unwrap().document_uuid,
        written_time_100ns: 123,
        ..inkpod_io::RecoveryMetadata::default()
    };
    let mut switch = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(sequence_recovery.clone()),
        None,
        Some(metadata),
    )
    .unwrap();
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    assert!(matches!(
        switch.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    let proof = *switch.recovery_artifact_proof().unwrap();
    assert!(
        switch
            .published_recovery_metadata()
            .unwrap()
            .pair_proof
            .is_none()
    );
    assert!(matches!(
        switch.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert!(!core.document_info().unwrap().recovered);

    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut restore = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((sequence_recovery, proof)),
        None,
    )
    .unwrap();
    let progress = ready(&mut restore);
    assert_eq!(
        (progress.state, progress.result_count),
        (FileIoState::Ready, 0)
    );
    assert!(matches!(
        restore.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert_eq!(core.document_state_digest().unwrap(), digest);
    assert_eq!(core.history_entries(), history);
    assert_eq!(core.editor_state().unwrap(), editor);
    assert!(core.document_info().unwrap().recovered);
    assert!(core.document_info().unwrap().dirty);
    assert_normal_file_authority_revoked(&mut core, &manager, &standalone, &old_save);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_untitled_and_explicit_import_sequence_recovery_keep_authority_none() {
    for imported in [false, true] {
        let files = Files::new();
        let manager = serial_manager();
        let recovery_path = files
            .0
            .join(if imported { "imported" } else { "untitled" })
            .join("cell1.inkpod");
        let raster = CommonRaster::new(
            2,
            2,
            PixelFormat::StraightRgba8,
            Some(144_000),
            Some(144_000),
            [1, 2, 3, 255].repeat(4),
        )
        .unwrap();
        let source_uuid = if imported { 0x7711 } else { 0x7722 };
        let mut core = Core::new();
        if imported {
            core.import_decoded_common_raster(CommonRasterFormat::Png, &raster, source_uuid)
                .unwrap();
        } else {
            core.new_cell_with_uuid(2, 2, 144_000, 144_000, source_uuid)
                .unwrap();
        }
        let first =
            SequenceCellSource::from_common_raster("cell1.png", source_uuid, &raster).unwrap();
        let mut second =
            SequenceCellSource::from_common_raster("cell2.bmp", 0x7733, &raster).unwrap();
        second.raster_file_format = CommonRasterFormat::Bmp;
        core.set_sequence(vec![first, second]).unwrap();
        let expected_digest = core.document_state_digest().unwrap();
        let expected_history = core.history_entries().to_vec();
        let expected_editor = core.editor_state().unwrap();

        let request = core
            .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
            .unwrap();
        let metadata = inkpod_io::RecoveryMetadata {
            session_id: 14,
            generation: u64::from(imported) + 1,
            document_uuid: source_uuid,
            written_time_100ns: 123,
            ..inkpod_io::RecoveryMetadata::default()
        };
        let mut switch = FileIoJob::start_sequence_switch(
            &core,
            manager.clone(),
            request,
            Some(recovery_path.clone()),
            None,
            Some(metadata),
        )
        .unwrap();
        assert_eq!(ready(&mut switch).state, FileIoState::Ready);
        assert!(matches!(
            switch.apply(&mut core).unwrap(),
            FileIoApply::Pending
        ));
        assert_eq!(ready(&mut switch).state, FileIoState::Ready);
        let proof = *switch.recovery_artifact_proof().unwrap();
        assert!(
            switch
                .published_recovery_metadata()
                .unwrap()
                .pair_proof
                .is_none()
        );
        switch.apply(&mut core).unwrap();

        let request = core
            .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
            .unwrap();
        let mut restore = FileIoJob::start_sequence_switch(
            &core,
            manager.clone(),
            request,
            None,
            Some((recovery_path, proof)),
            None,
        )
        .unwrap();
        let progress = ready(&mut restore);
        assert_eq!(progress.state, FileIoState::Ready);
        assert_eq!(progress.result_count, 0);
        restore.apply(&mut core).unwrap();
        assert_eq!(core.document_state_digest().unwrap(), expected_digest);
        assert_eq!(core.history_entries(), expected_history);
        let restored_editor = core.editor_state().unwrap();
        assert_eq!(restored_editor.revision, expected_editor.revision);
        assert_eq!(restored_editor.digest, expected_editor.digest);
        assert_eq!(restored_editor.state, expected_editor.state);
        assert!(restored_editor.dirty);
        assert!(core.document_info().unwrap().dirty);
        assert!(core.document_info().unwrap().recovered);
        assert_eq!(
            core.revert(),
            Err(CoreError::InvalidState("document has no normal-save path"))
        );
        manager.shutdown_and_wait();
    }
}

#[test]
fn io_003_sequence_recovery_retains_native_with_missing_raster_repair_authority() {
    let files = Files::new();
    let manager = serial_manager();
    let raster_path = files.image("cell1.png", CommonRasterFormat::Png);
    let native_path = raster_path.with_extension("inkpod");
    let recovery_path = files.0.join("attempts").join("cell1.inkpod");

    let mut original = Core::new();
    open(&mut original, &manager, &raster_path);
    original
        .apply_stroke(&super::line_stroke(vec![StrokeSample {
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
        }]))
        .unwrap();
    save_paths(
        &mut original,
        &manager,
        vec![native_path.clone(), raster_path.clone()],
        true,
    );
    std::fs::remove_file(&raster_path).unwrap();

    let mut core = Core::new();
    let mut opened = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native_path.clone()]),
    )
    .unwrap();
    let opened_progress = ready(&mut opened);
    assert_eq!(opened_progress.state, FileIoState::Ready);
    assert_eq!(opened_progress.result_count, 2);
    assert!(!opened.item(1).unwrap().identity_physical);
    opened.apply(&mut core).unwrap();

    let source_raster = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(144_000),
        Some(144_000),
        [10, 20, 30, 255].repeat(4),
    )
    .unwrap();
    let source_uuid = core.document_info().unwrap().document_uuid;
    let first =
        SequenceCellSource::from_common_raster("cell1.png", source_uuid, &source_raster).unwrap();
    let mut second =
        SequenceCellSource::from_common_raster("cell2.bmp", 0x2233, &source_raster).unwrap();
    second.raster_file_format = CommonRasterFormat::Bmp;
    core.set_sequence(vec![first, second]).unwrap();
    assert!(!core.document_info().unwrap().dirty);
    let expected_digest = core.document_state_digest().unwrap();
    let expected_history = core.history_entries().to_vec();

    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(matches!(
        FileIoJob::start_sequence_switch(&core, manager.clone(), request, None, None, None,),
        Err(CoreError::InvalidArgument(
            "sequence source requires a recovery destination"
        ))
    ));
    let metadata = inkpod_io::RecoveryMetadata {
        session_id: 13,
        generation: 1,
        document_uuid: source_uuid,
        written_time_100ns: 123,
        ..inkpod_io::RecoveryMetadata::default()
    };
    let mut switch = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(recovery_path.clone()),
        None,
        Some(metadata),
    )
    .unwrap();
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    assert!(matches!(
        switch.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    let artifact_proof = *switch.recovery_artifact_proof().unwrap();
    let pair_proof = switch
        .published_recovery_metadata()
        .unwrap()
        .pair_proof
        .unwrap();
    assert!(matches!(
        pair_proof,
        inkpod_io::RecoveryPairProof::RepairNeeded { .. }
    ));
    switch.apply(&mut core).unwrap();

    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let before_conflict = core.document_info().unwrap();
    std::fs::write(
        &raster_path,
        encode_common_raster(CommonRasterFormat::Png, &source_raster, false).unwrap(),
    )
    .unwrap();
    let mut conflicted = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((recovery_path.clone(), artifact_proof)),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut conflicted).state, FileIoState::Failed);
    assert_eq!(conflicted.error(), Some(&CoreError::FileConflict));
    assert_eq!(core.document_info().unwrap(), before_conflict);
    std::fs::remove_file(&raster_path).unwrap();

    let mut restore = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((recovery_path, artifact_proof)),
        None,
    )
    .unwrap();
    let progress = ready(&mut restore);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", restore.error());
    assert_eq!(progress.result_count, 2);
    assert_eq!(restore.item(0).unwrap().path, raster_path);
    assert!(!restore.item(0).unwrap().identity_physical);
    assert_eq!(restore.item(1).unwrap().path, native_path);
    assert!(restore.item(1).unwrap().identity_physical);
    restore.apply(&mut core).unwrap();
    assert_eq!(core.document_state_digest().unwrap(), expected_digest);
    assert_eq!(core.history_entries(), expected_history);
    assert!(!core.document_info().unwrap().dirty);
    assert!(!core.document_info().unwrap().recovered);

    save_paths(
        &mut core,
        &manager,
        vec![native_path.clone(), raster_path.clone()],
        false,
    );
    assert!(native_path.is_file());
    assert!(raster_path.is_file());
    assert!(!core.document_info().unwrap().dirty);
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
            FileIoJob::start_sequence_switch(&core, manager.clone(), request, None, None, None,),
            Err(CoreError::InvalidArgument(
                "sequence source requires a recovery destination"
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
    let missing = files.0.join("missing.inkpod");
    let metadata_mismatch = files.0.join("metadata-mismatch.inkpod");
    let native_mismatch = files.0.join("native-mismatch.inkpod");
    let mut core = sequence_core();
    let target0_native = files.0.join("target0.inkpod");
    save(&mut core, &manager, &target0_native);
    let target0_raster = target0_native.with_extension("tif");
    let recovery_proof = write_recovery(&core, &manager, &recovery, &target0_raster, 1);
    let target_uuid = core.document_info().unwrap().document_uuid;
    let (mismatch_native, _) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap();
    let mismatch_metadata = recovery_metadata_for_pair(
        &manager,
        core.document_info().unwrap().document_uuid ^ 1,
        &target0_raster,
        4,
    );
    let metadata_mismatch_proof = manager
        .write_recovery(
            &metadata_mismatch,
            &mismatch_metadata,
            &inkpod_io::JobContext::new(),
            |writer| {
                inkpod_format::write_procedure_to_writer(writer, &mismatch_native, || false)?;
                Ok(())
            },
        )
        .unwrap();
    assert!(!core.sequence_activate(1).unwrap().dirty);
    let target1_native = files.0.join("target1.inkpod");
    save(&mut core, &manager, &target1_native);
    let target1_raster = target1_native.with_extension("bmp");
    let wrong_proof = write_recovery(&core, &manager, &wrong, &target1_raster, 2);
    let (wrong_native, _) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap();
    let native_mismatch_proof = manager
        .write_recovery(
            &native_mismatch,
            &recovery_metadata_for_pair(&manager, target_uuid, &target0_raster, 5),
            &inkpod_io::JobContext::new(),
            |writer| {
                inkpod_format::write_procedure_to_writer(writer, &wrong_native, || false)?;
                Ok(())
            },
        )
        .unwrap();
    let malformed_proof = write_recovery(&core, &manager, &malformed, &target1_raster, 3);
    std::fs::write(&malformed, b"not a native recovery file").unwrap();
    let bytes: Vec<_> = std::fs::read_dir(&files.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
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
        Some((recovery.clone(), recovery_proof)),
        None,
    )
    .unwrap();
    drop(dropped);
    let mut cancelled = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        None,
        Some((recovery.clone(), recovery_proof)),
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
        Some((recovery.clone(), recovery_proof)),
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

    for (invalid, proof) in [
        (&wrong, wrong_proof),
        (&malformed, malformed_proof),
        (&missing, recovery_proof),
        (&metadata_mismatch, metadata_mismatch_proof),
        (&native_mismatch, native_mismatch_proof),
    ] {
        let mut failed = FileIoJob::start_sequence_switch(
            &core,
            manager.clone(),
            request,
            None,
            Some((invalid.clone(), proof)),
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
        Some((recovery, recovery_proof)),
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
    let source_metadata = inkpod_io::RecoveryMetadata {
        session_id: 1,
        generation: 7,
        document_uuid: before.document_uuid,
        written_time_100ns: 123,
        ..inkpod_io::RecoveryMetadata::default()
    };
    assert!(matches!(
        FileIoJob::start_sequence_switch(
            &core,
            manager.clone(),
            request,
            Some(path.clone()),
            None,
            None,
        ),
        Err(CoreError::InvalidArgument(
            "sequence source recovery requires typed metadata"
        ))
    ));
    let gate = WorkerGate::new(&manager);
    let dropped = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(path.clone()),
        None,
        Some(source_metadata.clone()),
    )
    .unwrap();
    drop(dropped);
    let mut cancelled = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(path.clone()),
        None,
        Some(source_metadata.clone()),
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
        Some(source_metadata.clone()),
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

    // A frontend may fail while copying the proof/effective metadata after the
    // durable worker has already reached its final READY. Cancellation at that
    // point must suppress the target commit, while a mandatory final apply
    // releases the originating Core's installation fence.
    let published_path = files.0.join("published-before-query-failure.inkpod");
    let mut cancelled_ready = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(published_path.clone()),
        None,
        Some(source_metadata.clone()),
    )
    .unwrap();
    assert_eq!(ready(&mut cancelled_ready).state, FileIoState::Ready);
    assert!(matches!(
        cancelled_ready.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut cancelled_ready).state, FileIoState::Ready);
    assert!(cancelled_ready.recovery_artifact_proof().is_ok());
    cancelled_ready.cancel();
    assert_eq!(cancelled_ready.poll().state, FileIoState::Ready);
    assert_eq!(
        cancelled_ready.apply(&mut core).unwrap_err(),
        CoreError::Cancelled
    );
    assert!(!cancelled_ready.requires_finalization());
    assert_eq!(core.document_info().unwrap(), before);
    assert!(published_path.exists());
    core.validate_document_save(&old_save).unwrap();

    let mut stale = FileIoJob::start_sequence_switch(
        &core,
        manager.clone(),
        request,
        Some(path.clone()),
        None,
        Some(source_metadata),
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
    let mut no_op =
        FileIoJob::start_sequence_switch(&core, manager.clone(), same, None, None, None).unwrap();
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
fn io_003_raster_pair_open_reports_missing_native_and_remains_pathless() {
    let files = Files::new();
    let raster = files.image("source.png", CommonRasterFormat::Png);
    let native = raster.with_extension("inkpod");
    let manager = manager();
    let mut core = Core::new();
    let mut job = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster.clone()]),
    )
    .unwrap();
    let progress = ready(&mut job);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", job.error());
    assert_eq!(progress.result_count, 2);
    let raster_item = job.item(0).unwrap();
    assert_eq!(raster_item.path, std::fs::canonicalize(&raster).unwrap());
    assert_eq!(raster_item.format, Some(CommonRasterFormat::Png));
    assert!(raster_item.identity_physical);
    let native_item = job.item(1).unwrap();
    assert_eq!(
        native_item.path,
        std::fs::canonicalize(&files.0)
            .unwrap()
            .join("source.inkpod")
    );
    assert_eq!(native_item.format, None);
    assert!(!native_item.identity_physical);
    assert_eq!(native_item.document_uuid, raster_item.document_uuid);
    assert!(matches!(
        job.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
    assert_eq!(
        core.revert(),
        Err(CoreError::InvalidState("document has no normal-save path"))
    );

    save_paths(
        &mut core,
        &manager,
        vec![native.clone(), raster.clone()],
        false,
    );
    assert!(native.is_file());
    assert!(raster.is_file());
    // The explicitly supplied raster path is retained as subsequent authority.
    save_paths(
        &mut core,
        &manager,
        vec![native.clone(), raster.clone()],
        false,
    );
    std::fs::write(&raster, b"externally changed").unwrap();
    let mut changed = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SavePair, vec![native.clone(), raster]),
    )
    .unwrap();
    assert_eq!(ready(&mut changed).state, FileIoState::Failed);
    assert_eq!(changed.error(), Some(&CoreError::FileConflict));
    manager.shutdown_and_wait();
}

#[test]
fn io_003_planned_pair_rejects_any_open_time_filesystem_change() {
    let manager = manager();
    for change in ["native-created", "raster-changed", "raster-removed"] {
        let files = Files::new();
        let raster = files.image("source.png", CommonRasterFormat::Png);
        let native = raster.with_extension("inkpod");
        let mut core = Core::new();
        let mut open = FileIoJob::start(
            Some(&core),
            manager.clone(),
            FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster.clone()]),
        )
        .unwrap();
        assert_eq!(ready(&mut open).state, FileIoState::Ready);
        open.apply(&mut core).unwrap();
        match change {
            "native-created" => std::fs::write(&native, b"unexpected native").unwrap(),
            "raster-changed" => std::fs::write(&raster, b"external change").unwrap(),
            "raster-removed" => std::fs::remove_file(&raster).unwrap(),
            _ => unreachable!(),
        }
        let mut save = FileIoJob::start(
            Some(&core),
            manager.clone(),
            FileIoRequest::new(FileIoKind::SavePair, vec![native, raster]),
        )
        .unwrap();
        assert_eq!(ready(&mut save).state, FileIoState::Failed, "{change}");
        assert_eq!(save.error(), Some(&CoreError::FileConflict), "{change}");
    }
    manager.shutdown_and_wait();
}

#[test]
fn io_003_raster_pair_open_adopts_only_an_exact_existing_sidecar() {
    let files = Files::new();
    let raster = files.image("source.png", CommonRasterFormat::Png);
    let native = raster.with_extension("inkpod");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );

    let mut reopened = Core::new();
    let mut job = FileIoJob::start(
        Some(&reopened),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster.clone()]),
    )
    .unwrap();
    let progress = ready(&mut job);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", job.error());
    assert_eq!(progress.result_count, 2);
    assert_eq!(
        job.item(0).unwrap().path,
        std::fs::canonicalize(&raster).unwrap()
    );
    assert_eq!(
        job.item(1).unwrap().path,
        std::fs::canonicalize(&native).unwrap()
    );
    assert!(job.item(1).unwrap().identity_physical);
    job.apply(&mut reopened).unwrap();
    save_paths(
        &mut reopened,
        &manager,
        vec![native.clone(), raster.clone()],
        false,
    );
    reopened.revert().unwrap();

    let replacement = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(144_000),
        Some(144_000),
        [200, 100, 50, 255].repeat(4),
    )
    .unwrap();
    std::fs::write(
        &raster,
        encode_common_raster(CommonRasterFormat::Png, &replacement, false).unwrap(),
    )
    .unwrap();
    let mismatch_core = Core::new();
    let mut request = FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster]);
    request.force_reload = true;
    let mut mismatch = FileIoJob::start(Some(&mismatch_core), manager.clone(), request).unwrap();
    assert_eq!(ready(&mut mismatch).state, FileIoState::Failed);
    assert_eq!(mismatch.error(), Some(&CoreError::FileConflict));
    assert_eq!(mismatch_core.document_info(), Err(CoreError::NoDocument));
    manager.shutdown_and_wait();
}

#[test]
fn io_003_pair_save_reopen_and_revert_preserve_genesis_branches_assets_and_editor_savepoints() {
    let files = Files::new();
    let manager = serial_manager();
    let source = files.0.join("history-source.png");
    let native = files.0.join("history-pair.inkpod");
    let raster = native.with_extension("png");
    let source_raster = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(144_000),
        Some(144_000),
        [10, 20, 30, 128].repeat(4),
    )
    .unwrap();
    std::fs::write(
        &source,
        encode_common_raster(CommonRasterFormat::Png, &source_raster, false).unwrap(),
    )
    .unwrap();

    let mut core = Core::new();
    open(&mut core, &manager, &source);
    let document = core.document_info().unwrap();
    let genesis = core.genesis_info().unwrap();
    let genesis_asset = core.asset_infos()[0].id;
    let import_color_asset = |core: &mut Core, pixel: [u8; 4]| {
        core.execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: core.document_info().unwrap().document_revision,
            target_plane_id: document.color_plane_id,
            raster: RasterAssetInput {
                width: 2,
                height: 2,
                pixel_format: PixelFormat::StraightRgba8,
                color_space: Some(AssetColorSpace::Srgb),
                alpha_semantics: AssetAlphaSemantics::Straight,
                canonical_stride: 8,
                pixels: pixel.repeat(4),
                expected_id: None,
            },
        })
        .unwrap()
        .procedure()
        .unwrap()
        .asset_ids()[0]
    };

    let inactive_asset = import_color_asset(&mut core, [1, 2, 3, 255]);
    core.undo().unwrap();
    let active_asset = import_color_asset(&mut core, [210, 30, 20, 255]);
    let saved_digest = core.document_state_digest().unwrap();
    let redo_asset = import_color_asset(&mut core, [20, 210, 30, 255]);
    let redo_digest = core.document_state_digest().unwrap();
    core.undo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), saved_digest);
    assert!(core.document_info().unwrap().can_redo);
    assert_eq!(core.collect_unreferenced_assets().unwrap(), 0);
    for id in [genesis_asset, inactive_asset, active_asset, redo_asset] {
        assert!(
            core.asset_info(id).is_some(),
            "retained asset {id:?} is missing"
        );
    }
    assert!(
        core.journal_entries()
            .iter()
            .any(|entry| matches!(entry, JournalEntry::BranchCut(_)))
    );
    core.update_editor_state(
        core.editor_state().unwrap().revision,
        EditorStateUpdate::SetActiveTool(EditorTool::Eraser),
    )
    .unwrap();
    assert!(core.document_info().unwrap().dirty);
    assert!(core.editor_state().unwrap().dirty);

    save_paths(
        &mut core,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );
    assert!(!core.document_info().unwrap().dirty);
    assert!(!core.editor_state().unwrap().dirty);
    assert!(core.document_info().unwrap().can_redo);
    let expected_genesis = core.genesis_info().unwrap();
    let expected_assets = core.asset_infos();
    let expected_asset_usage = core.asset_store_usage();
    let expected_history = core.history_entries().to_vec();
    let expected_journal = core.journal_entries().to_vec();
    let expected_journal_state = core.journal_state().unwrap();
    let expected_editor = core.editor_state().unwrap();
    let expected_editor_frame = core.editor_state_frame().unwrap();
    assert_eq!(expected_genesis, genesis);
    assert_eq!(
        expected_journal_state.current_state_id(),
        expected_journal_state.savepoint_state_id().unwrap()
    );
    let saved_raster = inkpod_format::decode_common_raster(
        CommonRasterFormat::Png,
        &std::fs::read(&raster).unwrap(),
    )
    .unwrap();
    let expected_raster = inkpod_format::decode_common_raster(
        CommonRasterFormat::Png,
        &core
            .export_common_raster(CommonRasterFormat::Png, false)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved_raster, expected_raster);
    assert_ne!(saved_raster.pixels, source_raster.pixels);

    let mut reopened = Core::new();
    reopened.bind_file_io(manager.clone()).unwrap();
    let mut open = FileIoJob::start(
        Some(&reopened),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]),
    )
    .unwrap();
    assert_eq!(
        ready(&mut open).state,
        FileIoState::Ready,
        "{:?}",
        open.error()
    );
    open.apply(&mut reopened).unwrap();
    let assert_saved_state = |core: &mut Core| {
        assert_eq!(core.collect_unreferenced_assets().unwrap(), 0);
        assert_eq!(core.document_state_digest().unwrap(), saved_digest);
        assert_eq!(core.genesis_info().unwrap(), expected_genesis);
        assert_eq!(core.asset_infos(), expected_assets);
        assert_eq!(core.asset_store_usage(), expected_asset_usage);
        assert_eq!(core.history_entries(), expected_history);
        assert_eq!(core.journal_entries(), expected_journal);
        assert_eq!(core.journal_state(), Some(expected_journal_state));
        assert_eq!(core.editor_state().unwrap(), expected_editor);
        assert_eq!(core.editor_state_frame().unwrap(), expected_editor_frame);
        assert!(!core.document_info().unwrap().dirty);
        assert!(!core.editor_state().unwrap().dirty);
        assert!(core.document_info().unwrap().can_redo);
        for id in [genesis_asset, inactive_asset, active_asset, redo_asset] {
            assert!(
                core.asset_info(id).is_some(),
                "retained asset {id:?} is missing"
            );
        }
    };
    assert_saved_state(&mut reopened);

    reopened.redo().unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), redo_digest);
    reopened
        .update_editor_state(
            reopened.editor_state().unwrap().revision,
            EditorStateUpdate::SetActiveTool(EditorTool::BoxZoom),
        )
        .unwrap();
    assert!(reopened.document_info().unwrap().dirty);
    assert!(reopened.editor_state().unwrap().dirty);
    reopened.revert().unwrap();
    assert_saved_state(&mut reopened);
    reopened.redo().unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), redo_digest);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_synchronous_revert_revalidates_and_retains_pair_save_authority() {
    for companion_present in [true, false] {
        let files = Files::new();
        let native = files.0.join(if companion_present {
            "sync-revert-committed.inkpod"
        } else {
            "sync-revert-repair.inkpod"
        });
        let raster = native.with_extension("tif");
        let manager = manager();
        let mut core = sequence_core();
        core.bind_file_io(manager.clone()).unwrap();
        save_paths(
            &mut core,
            &manager,
            vec![native.clone(), raster.clone()],
            true,
        );
        let saved_digest = core.document_state_digest().unwrap();
        let saved_history = core.history_entries();
        let saved_editor = core.editor_state().unwrap();
        let sequence_before = core.sequence_catalog_info();
        core.set_main_line_color(PixelValue::Rgba([20, 40, 60, 255]))
            .unwrap();
        assert!(core.document_info().unwrap().dirty);
        if !companion_present {
            std::fs::remove_file(&raster).unwrap();
        }

        let reverted = core.revert().unwrap();
        assert!(!reverted.dirty);
        assert_eq!(core.document_state_digest().unwrap(), saved_digest);
        assert_eq!(core.history_entries(), saved_history);
        assert_eq!(core.editor_state().unwrap(), saved_editor);
        let sequence_after = core.sequence_catalog_info();
        assert_eq!(sequence_after.revision, sequence_before.revision);
        assert_eq!(sequence_after.cell_count, sequence_before.cell_count);
        assert_eq!(sequence_after.active_index, sequence_before.active_index);
        assert_ne!(
            sequence_after.owner_generation,
            sequence_before.owner_generation
        );

        // No overwrite confirmation represents the ordinary Save route. A
        // missing companion must be regenerated from repair-needed Committed
        // authority instead of falling back to Save As.
        save_paths(
            &mut core,
            &manager,
            vec![native.clone(), raster.clone()],
            false,
        );
        assert!(native.is_file());
        assert!(raster.is_file());
        manager.shutdown_and_wait();
    }
}

#[test]
fn io_003_synchronous_revert_rejects_a_mismatched_companion_atomically() {
    let files = Files::new();
    let native = files.0.join("sync-revert-conflict.inkpod");
    let raster = native.with_extension("tif");
    let manager = manager();
    let mut core = sequence_core();
    core.bind_file_io(manager.clone()).unwrap();
    save_paths(&mut core, &manager, vec![native, raster.clone()], true);
    core.set_main_line_color(PixelValue::Rgba([20, 40, 60, 255]))
        .unwrap();
    let before_info = core.document_info().unwrap();
    let before_digest = core.document_state_digest().unwrap();
    let before_history = core.history_entries();
    let before_sequence = core.sequence_catalog_info();
    let replacement = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(144_000),
        Some(144_000),
        [200, 100, 50, 255].repeat(4),
    )
    .unwrap();
    std::fs::write(
        &raster,
        encode_common_raster(CommonRasterFormat::Tiff, &replacement, false).unwrap(),
    )
    .unwrap();

    assert_eq!(core.revert(), Err(CoreError::FileConflict));
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.document_state_digest().unwrap(), before_digest);
    assert_eq!(core.history_entries(), before_history);
    assert_eq!(core.sequence_catalog_info(), before_sequence);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_synchronous_revert_keeps_live_sequence_snapshot_charges_on_the_core() {
    let files = Files::new();
    let native = files.0.join("sync-revert-ledger.inkpod");
    let raster = native.with_extension("png");
    let manager = manager();
    let mut core = sequence_core();
    core.sequence_activate(1).unwrap();
    core.sequence_activate(0).unwrap();
    save_paths(&mut core, &manager, vec![native, raster], true);

    // Build before binding the manager so no speculative prefetch can add a
    // second reservation to this exact ledger assertion.
    let held_snapshot = core.build_snapshot();
    let source_before = held_snapshot.sequence_render_source().unwrap();
    let charged = core.resource_usage();
    assert!(charged.sequence_render_cache_bytes > 0);
    assert_eq!(charged.sequence_render_cache_source_count, 1);
    core.bind_file_io(manager.clone()).unwrap();

    core.revert().unwrap();
    let sequence_after = core.sequence_catalog_info();
    assert_ne!(
        sequence_after.owner_generation,
        source_before.owner_generation
    );
    let retained = core.resource_usage();
    assert_eq!(
        retained.sequence_render_cache_bytes,
        charged.sequence_render_cache_bytes
    );
    assert_eq!(
        retained.sequence_render_cache_source_count,
        charged.sequence_render_cache_source_count
    );
    assert_eq!(
        retained.sequence_render_cache_tile_count,
        charged.sequence_render_cache_tile_count
    );

    drop(held_snapshot);
    let released = core.resource_usage();
    assert_eq!(released.sequence_render_cache_bytes, 0);
    assert_eq!(released.sequence_render_cache_source_count, 0);
    assert_eq!(released.sequence_render_cache_tile_count, 0);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_forced_native_reload_retains_the_runtime_sequence_for_revert() {
    let files = Files::new();
    let native = files.0.join("revert-sequence.inkpod");
    let raster = native.with_extension("tif");
    let manager = manager();
    let mut core = sequence_core();
    save_paths(
        &mut core,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );
    let before = core.sequence_catalog_info();
    assert_eq!(before.cell_count, 2);
    assert_eq!(before.active_index, Some(0));
    std::fs::remove_file(&raster).unwrap();

    let mut request = FileIoRequest::new(FileIoKind::OpenNative, vec![native]);
    request.force_reload = true;
    request.revert_current = true;
    let mut job = FileIoJob::start(Some(&core), manager.clone(), request).unwrap();
    assert_eq!(
        ready(&mut job).state,
        FileIoState::Ready,
        "{:?}",
        job.error()
    );
    job.apply(&mut core).unwrap();

    let after = core.sequence_catalog_info();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.cell_count, before.cell_count);
    assert_eq!(after.active_index, before.active_index);
    assert_ne!(after.owner_generation, before.owner_generation);
    let switch = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(switch.requires_switch());
    assert!(switch.requires_source_recovery());
    manager.shutdown_and_wait();
}

#[test]
fn io_003_forced_native_reload_rejects_a_different_document_uuid() {
    let files = Files::new();
    let native = files.0.join("foreign-revert.inkpod");
    let raster = native.with_extension("bmp");
    let manager = manager();
    let mut saved = sequence_core();
    saved.sequence_activate(1).unwrap();
    save_paths(&mut saved, &manager, vec![native.clone(), raster], true);

    let mut current = sequence_core();
    assert_ne!(
        current.document_info().unwrap().document_uuid,
        saved.document_info().unwrap().document_uuid
    );
    let before_info = current.document_info().unwrap();
    let before_digest = current.document_state_digest().unwrap();
    let before_catalog = current.sequence_catalog_info();
    let mut request = FileIoRequest::new(FileIoKind::OpenNative, vec![native]);
    request.force_reload = true;
    request.revert_current = true;
    let mut job = FileIoJob::start(Some(&current), manager.clone(), request).unwrap();
    assert_eq!(
        ready(&mut job).state,
        FileIoState::Ready,
        "{:?}",
        job.error()
    );
    assert!(matches!(
        job.apply(&mut current),
        Err(CoreError::FileConflict)
    ));
    assert_eq!(current.document_info().unwrap(), before_info);
    assert_eq!(current.document_state_digest().unwrap(), before_digest);
    assert_eq!(current.sequence_catalog_info(), before_catalog);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_forced_native_reload_without_revert_remains_an_ordinary_open() {
    let files = Files::new();
    let native = files.0.join("forced-open.inkpod");
    let raster = native.with_extension("tif");
    let manager = manager();
    let mut saved = sequence_core();
    let expected_uuid = saved.document_info().unwrap().document_uuid;
    save_paths(&mut saved, &manager, vec![native.clone(), raster], true);

    let mut reopened = Core::new();
    let mut request = FileIoRequest::new(FileIoKind::OpenNative, vec![native]);
    request.force_reload = true;
    let mut job = FileIoJob::start(Some(&reopened), manager.clone(), request).unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    job.apply(&mut reopened).unwrap();
    assert_eq!(
        reopened.document_info().unwrap().document_uuid,
        expected_uuid
    );
    assert_eq!(reopened.sequence_catalog_info().cell_count, 0);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_revert_rejects_the_same_document_at_a_different_path() {
    let files = Files::new();
    let native = files.0.join("current.inkpod");
    let raster = native.with_extension("tif");
    let alternate = files.0.join("alternate.inkpod");
    let manager = manager();
    let mut core = sequence_core();
    save_paths(&mut core, &manager, vec![native.clone(), raster], true);
    std::fs::copy(&native, &alternate).unwrap();
    let before_info = core.document_info().unwrap();
    let before_digest = core.document_state_digest().unwrap();
    let before_catalog = core.sequence_catalog_info();

    let mut request = FileIoRequest::new(FileIoKind::OpenNative, vec![alternate]);
    request.force_reload = true;
    request.revert_current = true;
    let mut job = FileIoJob::start(Some(&core), manager.clone(), request).unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    assert!(matches!(job.apply(&mut core), Err(CoreError::FileConflict)));
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.document_state_digest().unwrap(), before_digest);
    assert_eq!(core.sequence_catalog_info(), before_catalog);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_revert_flag_requires_a_forced_native_open() {
    let files = Files::new();
    let manager = manager();
    let core = Core::new();
    let mut request = FileIoRequest::new(
        FileIoKind::OpenNative,
        vec![files.0.join("invalid-revert.inkpod")],
    );
    request.revert_current = true;
    assert!(matches!(
        FileIoJob::start(Some(&core), manager.clone(), request),
        Err(CoreError::InvalidArgument(
            "current-document revert requires a forced native open"
        ))
    ));
    manager.shutdown_and_wait();
}

#[test]
fn io_003_direct_native_open_accepts_an_exact_companion() {
    let files = Files::new();
    let raster = files.image("source.png", CommonRasterFormat::Png);
    let native = raster.with_extension("inkpod");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );
    let expected_digest = original.document_state_digest().unwrap();

    let mut reopened = Core::new();
    let mut job = FileIoJob::start(
        Some(&reopened),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]),
    )
    .unwrap();
    let progress = ready(&mut job);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", job.error());
    assert_eq!(progress.loaded_count, 1);
    assert_eq!(progress.result_count, 2);
    assert_eq!(job.item(0).unwrap().path, native);
    assert_eq!(job.item(0).unwrap().format, None);
    assert!(job.item(0).unwrap().identity_physical);
    assert_eq!(
        job.item(1).unwrap().path,
        std::fs::canonicalize(&raster).unwrap()
    );
    assert_eq!(job.item(1).unwrap().format, Some(CommonRasterFormat::Png));
    assert!(job.item(1).unwrap().identity_physical);
    job.apply(&mut reopened).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), expected_digest);
    assert!(!reopened.document_info().unwrap().dirty);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_direct_native_open_rejects_a_mismatched_companion() {
    let files = Files::new();
    let raster = files.image("source.png", CommonRasterFormat::Png);
    let native = raster.with_extension("inkpod");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );
    let replacement = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(72_000),
        Some(72_000),
        [200, 100, 50, 255].repeat(4),
    )
    .unwrap();
    std::fs::write(
        &raster,
        encode_common_raster(CommonRasterFormat::Png, &replacement, false).unwrap(),
    )
    .unwrap();

    let untouched = Core::new();
    let mut request = FileIoRequest::new(FileIoKind::OpenNative, vec![native]);
    request.force_reload = true;
    let mut job = FileIoJob::start(Some(&untouched), manager.clone(), request).unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Failed);
    assert_eq!(job.error(), Some(&CoreError::FileConflict));
    assert_eq!(untouched.document_info(), Err(CoreError::NoDocument));
    manager.shutdown_and_wait();
}

#[test]
fn io_003_dpi_less_png_pair_uses_the_canonical_96_dpi_default() {
    let files = Files::new();
    let raster = files.image_with_dpi("source.png", CommonRasterFormat::Png, None);
    let dpi_less_bytes = std::fs::read(&raster).unwrap();
    let native = raster.with_extension("inkpod");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );
    std::fs::write(&raster, &dpi_less_bytes).unwrap();

    let mut raster_reopened = Core::new();
    let mut raster_request = FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster.clone()]);
    raster_request.force_reload = true;
    let mut raster_job =
        FileIoJob::start(Some(&raster_reopened), manager.clone(), raster_request).unwrap();
    assert_eq!(
        ready(&mut raster_job).state,
        FileIoState::Ready,
        "{:?}",
        raster_job.error()
    );
    raster_job.apply(&mut raster_reopened).unwrap();

    let mut native_reopened = Core::new();
    let mut native_request = FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]);
    native_request.force_reload = true;
    let mut native_job =
        FileIoJob::start(Some(&native_reopened), manager.clone(), native_request).unwrap();
    assert_eq!(
        ready(&mut native_job).state,
        FileIoState::Ready,
        "{:?}",
        native_job.error()
    );
    native_job.apply(&mut native_reopened).unwrap();

    let explicit_72_dpi = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(72_000),
        Some(72_000),
        [10, 20, 30, 255].repeat(4),
    )
    .unwrap();
    std::fs::write(
        &raster,
        encode_common_raster(CommonRasterFormat::Png, &explicit_72_dpi, false).unwrap(),
    )
    .unwrap();
    let untouched = Core::new();
    let mut mismatch_request = FileIoRequest::new(FileIoKind::OpenNative, vec![native]);
    mismatch_request.force_reload = true;
    let mut mismatch =
        FileIoJob::start(Some(&untouched), manager.clone(), mismatch_request).unwrap();
    assert_eq!(ready(&mut mismatch).state, FileIoState::Failed);
    assert_eq!(mismatch.error(), Some(&CoreError::FileConflict));
    assert_eq!(untouched.document_info(), Err(CoreError::NoDocument));
    manager.shutdown_and_wait();
}

#[test]
fn io_003_direct_native_open_repairs_a_missing_companion_on_clean_save() {
    let files = Files::new();
    let raster = files.image("source.png", CommonRasterFormat::Png);
    let native = raster.with_extension("inkpod");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );
    std::fs::remove_file(&raster).unwrap();

    let mut reopened = Core::new();
    let mut job = FileIoJob::start(
        Some(&reopened),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]),
    )
    .unwrap();
    let progress = ready(&mut job);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", job.error());
    assert_eq!(progress.result_count, 2);
    assert_eq!(job.item(1).unwrap().path, raster);
    assert_eq!(job.item(1).unwrap().format, Some(CommonRasterFormat::Png));
    assert!(!job.item(1).unwrap().identity_physical);
    job.apply(&mut reopened).unwrap();
    let before = reopened.document_info().unwrap();
    assert!(!before.dirty);
    assert!(!raster.exists());

    save(&mut reopened, &manager, &native);
    assert!(raster.is_file());
    let after = reopened.document_info().unwrap();
    assert_eq!(after.document_revision, before.document_revision);
    assert!(!after.dirty);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_direct_native_open_preserves_an_exact_tiff_alias() {
    let files = Files::new();
    let raster = files.image("source.tiff", CommonRasterFormat::Tiff);
    let canonical_alias = raster.with_extension("tif");
    let native = raster.with_extension("inkpod");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );
    assert!(!canonical_alias.exists());

    let mut reopened = Core::new();
    let mut job = FileIoJob::start(
        Some(&reopened),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]),
    )
    .unwrap();
    let progress = ready(&mut job);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", job.error());
    assert_eq!(progress.result_count, 2);
    assert_eq!(
        job.item(1).unwrap().path,
        std::fs::canonicalize(&raster).unwrap()
    );
    assert_eq!(job.item(1).unwrap().format, Some(CommonRasterFormat::Tiff));
    assert!(job.item(1).unwrap().identity_physical);
    job.apply(&mut reopened).unwrap();
    save(&mut reopened, &manager, &native);
    assert!(raster.is_file());
    assert!(!canonical_alias.exists());
    manager.shutdown_and_wait();
}

#[test]
fn io_003_direct_native_open_reports_a_missing_selected_file_as_io_failure() {
    let files = Files::new();
    let missing = files.0.join("missing.inkpod");
    let manager = manager();
    let untouched = Core::new();
    let mut job = FileIoJob::start(
        Some(&untouched),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![missing]),
    )
    .unwrap();

    assert_eq!(ready(&mut job).state, FileIoState::Failed);
    assert!(matches!(job.error(), Some(CoreError::Format(_))));
    assert_eq!(untouched.document_info(), Err(CoreError::NoDocument));
    manager.shutdown_and_wait();
}

#[test]
fn io_003_direct_native_open_rejects_ambiguous_tiff_aliases() {
    let files = Files::new();
    let raster = files.image("source.tiff", CommonRasterFormat::Tiff);
    let canonical_alias = raster.with_extension("tif");
    let native = raster.with_extension("inkpod");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );
    std::fs::copy(&raster, canonical_alias).unwrap();

    let untouched = Core::new();
    let mut job = FileIoJob::start(
        Some(&untouched),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Failed);
    assert_eq!(job.error(), Some(&CoreError::FileConflict));
    let mut raster_job = FileIoJob::start(
        Some(&untouched),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster]),
    )
    .unwrap();
    assert_eq!(ready(&mut raster_job).state, FileIoState::Failed);
    assert_eq!(raster_job.error(), Some(&CoreError::FileConflict));
    assert_eq!(untouched.document_info(), Err(CoreError::NoDocument));
    manager.shutdown_and_wait();
}

#[test]
fn io_003_pair_open_retains_exact_case_companion_paths() {
    let files = Files::new();
    let raster = files.image("source.PNG", CommonRasterFormat::Png);
    let native = raster.with_extension("INKPOD");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );

    let mut direct = Core::new();
    let mut direct_job = FileIoJob::start(
        Some(&direct),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]),
    )
    .unwrap();
    assert_eq!(
        ready(&mut direct_job).state,
        FileIoState::Ready,
        "{:?}",
        direct_job.error()
    );
    assert_eq!(
        direct_job.item(1).unwrap().path,
        std::fs::canonicalize(&raster).unwrap()
    );
    direct_job.apply(&mut direct).unwrap();
    save(&mut direct, &manager, &native);
    let lowercase_alias = native.with_extension("png");
    let (selected_identity, selected_physical) = manager.resolve_identity(&raster).unwrap();
    let (alias_identity, alias_physical) = manager.resolve_identity(&lowercase_alias).unwrap();
    assert!(selected_physical);
    assert!(
        !alias_physical || alias_identity == selected_identity,
        "normal save created a distinct lowercase companion"
    );

    let mut pair = Core::new();
    let mut pair_job = FileIoJob::start(
        Some(&pair),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster.clone()]),
    )
    .unwrap();
    assert_eq!(
        ready(&mut pair_job).state,
        FileIoState::Ready,
        "{:?}",
        pair_job.error()
    );
    assert_eq!(
        pair_job.item(1).unwrap().path,
        std::fs::canonicalize(&native).unwrap()
    );
    pair_job.apply(&mut pair).unwrap();
    manager.shutdown_and_wait();
}

#[test]
fn io_003_pair_open_rejects_case_variant_companion_ambiguity() {
    let files = Files::new();
    let raster = files.image("source.PNG", CommonRasterFormat::Png);
    let native = raster.with_extension("INKPOD");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );

    let raster_alias = raster.with_extension("png");
    let raster_identity = manager.resolve_identity(&raster).unwrap().0;
    let (alias_identity, alias_physical) = manager.resolve_identity(&raster_alias).unwrap();
    if alias_physical && alias_identity == raster_identity {
        // A case-insensitive volume exposes another spelling of the selected
        // file, not a second companion candidate.
        manager.shutdown_and_wait();
        return;
    }
    if !alias_physical {
        std::fs::copy(&raster, &raster_alias).unwrap();
    }
    assert_ne!(
        manager.resolve_identity(&raster_alias).unwrap().0,
        raster_identity
    );
    let untouched = Core::new();
    let mut direct = FileIoJob::start(
        Some(&untouched),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut direct).state, FileIoState::Failed);
    assert_eq!(direct.error(), Some(&CoreError::FileConflict));
    let mut raster_pair = FileIoJob::start(
        Some(&untouched),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut raster_pair).state, FileIoState::Failed);
    assert_eq!(raster_pair.error(), Some(&CoreError::FileConflict));
    std::fs::remove_file(raster_alias).unwrap();

    let native_alias = native.with_extension("inkpod");
    std::fs::copy(&native, native_alias).unwrap();
    let mut direct_native_alias = FileIoJob::start(
        Some(&untouched),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut direct_native_alias).state, FileIoState::Failed);
    assert_eq!(direct_native_alias.error(), Some(&CoreError::FileConflict));
    let mut pair = FileIoJob::start(
        Some(&untouched),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster]),
    )
    .unwrap();
    assert_eq!(ready(&mut pair).state, FileIoState::Failed);
    assert_eq!(pair.error(), Some(&CoreError::FileConflict));
    assert_eq!(untouched.document_info(), Err(CoreError::NoDocument));
    manager.shutdown_and_wait();
}

#[test]
fn io_003_pair_save_rejects_unselected_same_format_aliases_even_when_confirmed() {
    let files = Files::new();
    let raster = files.image("source.tiff", CommonRasterFormat::Tiff);
    let alias = raster.with_extension("tif");
    let native = raster.with_extension("inkpod");
    let manager = manager();
    let mut original = Core::new();
    open(&mut original, &manager, &raster);
    save_paths(
        &mut original,
        &manager,
        vec![native.clone(), raster.clone()],
        true,
    );

    let mut reopened = Core::new();
    let mut open_native = FileIoJob::start(
        Some(&reopened),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut open_native).state, FileIoState::Ready);
    open_native.apply(&mut reopened).unwrap();
    std::fs::copy(&raster, &alias).unwrap();

    for overwrite_confirmed in [false, true] {
        let mut request = FileIoRequest::new(FileIoKind::SavePair, vec![native.clone()]);
        request.overwrite_confirmed = overwrite_confirmed;
        let mut save = FileIoJob::start(Some(&reopened), manager.clone(), request).unwrap();
        assert_eq!(ready(&mut save).state, FileIoState::Failed);
        assert_eq!(save.error(), Some(&CoreError::FileConflict));
    }

    let save_as_native = files.0.join("other.inkpod");
    let save_as_raster = files.0.join("other.tiff");
    std::fs::copy(&raster, save_as_raster.with_extension("tif")).unwrap();
    let mut save_as_request = FileIoRequest::new(
        FileIoKind::SavePair,
        vec![save_as_native.clone(), save_as_raster],
    );
    save_as_request.overwrite_confirmed = true;
    let mut save_as = FileIoJob::start(Some(&reopened), manager.clone(), save_as_request).unwrap();
    assert_eq!(ready(&mut save_as).state, FileIoState::Failed);
    assert_eq!(save_as.error(), Some(&CoreError::FileConflict));
    assert!(!save_as_native.exists());
    manager.shutdown_and_wait();
}

#[test]
fn io_003_raster_pair_open_reports_a_malformed_sidecar_as_conflict() {
    let files = Files::new();
    let raster = files.image("source.png", CommonRasterFormat::Png);
    std::fs::write(raster.with_extension("inkpod"), b"malformed native sidecar").unwrap();
    let manager = manager();
    let untouched = Core::new();
    let mut job = FileIoJob::start(
        Some(&untouched),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![raster]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Failed);
    assert_eq!(job.error(), Some(&CoreError::FileConflict));
    assert_eq!(untouched.document_info(), Err(CoreError::NoDocument));
    manager.shutdown_and_wait();
}

#[test]
fn io_003_direct_native_open_reports_pending_journal_failure_as_conflict() {
    use std::io::Write as _;

    let files = Files::new();
    let raster = files.image("source.png", CommonRasterFormat::Png);
    let native = raster.with_extension("inkpod");
    let initial_manager = manager();
    let mut original = Core::new();
    open(&mut original, &initial_manager, &raster);
    save_paths(
        &mut original,
        &initial_manager,
        vec![native.clone(), raster.clone()],
        true,
    );
    initial_manager.shutdown_and_wait();

    let native_bytes = std::fs::read(&native).unwrap();
    let raster_bytes = std::fs::read(&raster).unwrap();
    let staging_manager = manager();
    let prepared = staging_manager
        .prepare_pair(
            &native,
            &raster,
            &inkpod_io::JobContext::new(),
            |file| {
                file.write_all(&native_bytes)?;
                Ok(())
            },
            &raster_bytes,
            true,
        )
        .unwrap();
    staging_manager.shutdown_and_wait();
    drop(prepared);
    let journal = std::fs::read_dir(&files.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|value| value == "journal"))
        .unwrap();
    std::fs::write(journal, b"malformed pair journal").unwrap();

    let opening_manager = manager();
    let untouched = Core::new();
    let mut job = FileIoJob::start(
        Some(&untouched),
        opening_manager.clone(),
        FileIoRequest::new(FileIoKind::OpenNative, vec![native]),
    )
    .unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Failed);
    assert_eq!(job.error(), Some(&CoreError::FileConflict));
    assert_eq!(untouched.document_info(), Err(CoreError::NoDocument));
    opening_manager.shutdown_and_wait();
}

#[test]
fn io_003_sequence_pair_switch_replays_existing_sidecar_history() {
    let files = Files::new();
    let first = files.image("cell1.png", CommonRasterFormat::Png);
    let second = files.image("cell2.png", CommonRasterFormat::Png);
    let second_native = second.with_extension("inkpod");
    let manager = manager();

    let mut saved_target = Core::new();
    open(&mut saved_target, &manager, &second);
    saved_target
        .apply_stroke(&super::line_stroke(vec![StrokeSample {
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
        }]))
        .unwrap();
    let expected_digest = saved_target.document_state_digest().unwrap();
    let expected_history = saved_target.history_entries().to_vec();
    save_paths(
        &mut saved_target,
        &manager,
        vec![second_native.clone(), second.clone()],
        true,
    );

    let mut core = Core::new();
    let mut opened = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![first.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut opened).state, FileIoState::Ready);
    opened.apply(&mut core).unwrap();
    let mut sequence = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(
            FileIoKind::SequenceFiles,
            vec![first.clone(), second.clone()],
        ),
    )
    .unwrap();
    assert_eq!(ready(&mut sequence).state, FileIoState::Ready);
    sequence.apply(&mut core).unwrap();
    let before = core.document_info().unwrap();
    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut switch = FileIoJob::start_sequence_raster_pair_switch(
        &core,
        manager.clone(),
        request,
        None,
        second.clone(),
        None,
    )
    .unwrap();
    let progress = ready(&mut switch);
    assert_eq!(progress.state, FileIoState::Ready, "{:?}", switch.error());
    assert_eq!(progress.result_count, 2);
    assert!(switch.item(1).unwrap().identity_physical);
    switch.apply(&mut core).unwrap();

    let after = core.document_info().unwrap();
    assert_ne!(after.document_uuid, before.document_uuid);
    assert_eq!(
        after.document_uuid,
        saved_target.document_info().unwrap().document_uuid
    );
    assert_eq!(
        core.sequence_cell(1).unwrap().document_uuid,
        after.document_uuid
    );
    assert_eq!(core.document_state_digest().unwrap(), expected_digest);
    assert_eq!(core.history_entries(), expected_history);
    assert!(!after.dirty);
    let sequence_before_revert = core.sequence_catalog_info();
    assert!(core.revert().is_ok());
    assert_eq!(core.document_state_digest().unwrap(), expected_digest);
    let sequence_after_revert = core.sequence_catalog_info();
    assert_eq!(
        (
            sequence_after_revert.revision,
            sequence_after_revert.cell_count,
            sequence_after_revert.active_index,
        ),
        (
            sequence_before_revert.revision,
            sequence_before_revert.cell_count,
            sequence_before_revert.active_index,
        )
    );
    assert_ne!(
        sequence_after_revert.owner_generation,
        sequence_before_revert.owner_generation
    );
    manager.shutdown_and_wait();
}

#[test]
fn io_003_sequence_pair_switch_reopens_a_cell_after_its_normal_pair_save() {
    let files = Files::new();
    let first = files.image("cell1.png", CommonRasterFormat::Png);
    let first_native = first.with_extension("inkpod");
    let second = files.image("cell2.png", CommonRasterFormat::Png);
    let manager = manager();
    let mut core = Core::new();

    let mut opened = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![first.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut opened).state, FileIoState::Ready);
    opened.apply(&mut core).unwrap();
    let mut sequence = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(
            FileIoKind::SequenceFiles,
            vec![first.clone(), second.clone()],
        ),
    )
    .unwrap();
    assert_eq!(ready(&mut sequence).state, FileIoState::Ready);
    sequence.apply(&mut core).unwrap();

    core.apply_stroke(&super::line_stroke(vec![StrokeSample {
        x: 0.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let saved_digest = core.document_state_digest().unwrap();
    let saved_history = core.history_entries().to_vec();
    save_paths(
        &mut core,
        &manager,
        vec![first_native, first.clone()],
        false,
    );

    let to_second = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut second_switch = FileIoJob::start_sequence_raster_pair_switch(
        &core,
        manager.clone(),
        to_second,
        None,
        second,
        None,
    )
    .unwrap();
    assert_eq!(
        ready(&mut second_switch).state,
        FileIoState::Ready,
        "{:?}",
        second_switch.error()
    );
    second_switch.apply(&mut core).unwrap();
    let second_source = core.build_snapshot().sequence_render_source().unwrap();
    assert_eq!(
        (second_source.document_uuid, second_source.source_generation),
        (
            core.document_info().unwrap().document_uuid,
            core.sequence_cell(1).unwrap().source_generation,
        )
    );

    let to_first = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut first_switch = FileIoJob::start_sequence_raster_pair_switch(
        &core,
        manager.clone(),
        to_first,
        None,
        first,
        None,
    )
    .unwrap();
    assert_eq!(
        ready(&mut first_switch).state,
        FileIoState::Ready,
        "{:?}",
        first_switch.error()
    );
    first_switch.apply(&mut core).unwrap();
    let first_source = core.build_snapshot().sequence_render_source().unwrap();
    assert_eq!(
        (first_source.document_uuid, first_source.source_generation),
        (
            core.document_info().unwrap().document_uuid,
            core.sequence_cell(0).unwrap().source_generation,
        )
    );
    assert_eq!(core.document_state_digest().unwrap(), saved_digest);
    assert_eq!(core.history_entries(), saved_history);
    assert!(!core.document_info().unwrap().dirty);

    // Revert replaces document/history/editor state in the same live session.
    // Primary and duplicate Canvas views are runtime owners: neither their
    // current transforms nor their Core-local IDs may roll back to the view
    // topology captured by the earlier Save.
    let primary_before_revert = core
        .apply_view(ViewCommand::PanBy {
            device_dx: 3.0,
            device_dy: 2.0,
        })
        .unwrap();
    let duplicate_view = core.create_view().unwrap();
    let duplicate_before_revert = core
        .apply_view_for(duplicate_view, ViewCommand::SetAlphaView(true))
        .unwrap();
    let sequence_before_revert = core.sequence_catalog_info();
    assert!(core.revert().is_ok());
    assert_eq!(core.view_state(), primary_before_revert);
    assert_eq!(
        core.build_snapshot_for(duplicate_view).unwrap().view(),
        duplicate_before_revert
    );
    assert!(core.create_view().unwrap() > duplicate_view);
    let sequence_after_revert = core.sequence_catalog_info();
    assert_eq!(
        (
            sequence_after_revert.revision,
            sequence_after_revert.cell_count,
            sequence_after_revert.active_index,
        ),
        (
            sequence_before_revert.revision,
            sequence_before_revert.cell_count,
            sequence_before_revert.active_index,
        )
    );
    assert_ne!(
        sequence_after_revert.owner_generation,
        sequence_before_revert.owner_generation
    );
    manager.shutdown_and_wait();
}

#[test]
fn io_003_sequence_pair_switch_accepts_a_dpi_less_png_target() {
    let files = Files::new();
    let first = files.image("cell1.png", CommonRasterFormat::Png);
    let second = files.image_with_dpi("cell2.png", CommonRasterFormat::Png, None);
    let dpi_less_bytes = std::fs::read(&second).unwrap();
    let second_native = second.with_extension("inkpod");
    let manager = manager();

    let mut saved_target = Core::new();
    open(&mut saved_target, &manager, &second);
    let expected_digest = saved_target.document_state_digest().unwrap();
    save_paths(
        &mut saved_target,
        &manager,
        vec![second_native, second.clone()],
        true,
    );
    std::fs::write(&second, dpi_less_bytes).unwrap();

    let mut core = Core::new();
    let mut opened = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![first.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut opened).state, FileIoState::Ready);
    opened.apply(&mut core).unwrap();
    let mut sequence_request =
        FileIoRequest::new(FileIoKind::SequenceFiles, vec![first, second.clone()]);
    sequence_request.force_reload = true;
    let mut sequence = FileIoJob::start(Some(&core), manager.clone(), sequence_request).unwrap();
    assert_eq!(ready(&mut sequence).state, FileIoState::Ready);
    sequence.apply(&mut core).unwrap();

    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut switch = FileIoJob::start_sequence_raster_pair_switch(
        &core,
        manager.clone(),
        request,
        None,
        second,
        None,
    )
    .unwrap();
    assert_eq!(
        ready(&mut switch).state,
        FileIoState::Ready,
        "{:?}",
        switch.error()
    );
    switch.apply(&mut core).unwrap();
    assert_eq!(core.document_state_digest().unwrap(), expected_digest);
    assert!(!core.document_info().unwrap().dirty);
    manager.shutdown_and_wait();
}

#[test]
fn io_003_sequence_pair_switch_retains_missing_sidecar_first_save_proof() {
    let files = Files::new();
    let first = files.image("cell1.png", CommonRasterFormat::Png);
    let second = files.image("cell2.png", CommonRasterFormat::Png);
    let second_native = second.with_extension("inkpod");
    let manager = manager();
    let mut core = Core::new();
    let mut opened = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::OpenRasterPair, vec![first.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut opened).state, FileIoState::Ready);
    opened.apply(&mut core).unwrap();
    let mut sequence = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SequenceFiles, vec![first, second.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut sequence).state, FileIoState::Ready);
    sequence.apply(&mut core).unwrap();
    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let mut switch = FileIoJob::start_sequence_raster_pair_switch(
        &core,
        manager.clone(),
        request,
        None,
        second.clone(),
        None,
    )
    .unwrap();
    assert_eq!(ready(&mut switch).state, FileIoState::Ready);
    assert!(!switch.item(1).unwrap().identity_physical);
    switch.apply(&mut core).unwrap();
    assert_eq!(
        core.revert(),
        Err(CoreError::InvalidState("document has no normal-save path"))
    );
    save_paths(
        &mut core,
        &manager,
        vec![second_native.clone(), second.clone()],
        false,
    );
    assert!(second_native.is_file());
    assert!(second.is_file());
    assert!(core.revert().is_ok());
    manager.shutdown_and_wait();
}

#[test]
fn io_003_pair_install_is_fenced_and_post_open_companion_deletion_conflicts() {
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
    let mut conflicted = FileIoJob::start(
        Some(&core),
        manager.clone(),
        FileIoRequest::new(FileIoKind::SavePair, vec![destination.clone()]),
    )
    .unwrap();
    assert_eq!(ready(&mut conflicted).state, FileIoState::Failed);
    assert_eq!(conflicted.error(), Some(&CoreError::FileConflict));
    assert!(!destination.with_extension("tif").exists());
    assert_eq!(core.document_info().unwrap(), before);

    let mut confirmed = FileIoRequest::new(FileIoKind::SavePair, vec![destination.clone()]);
    confirmed.overwrite_confirmed = true;
    let mut confirmed = FileIoJob::start(Some(&core), manager.clone(), confirmed).unwrap();
    assert_eq!(ready(&mut confirmed).state, FileIoState::Ready);
    assert!(matches!(
        confirmed.apply(&mut core).unwrap(),
        FileIoApply::Pending
    ));
    assert_eq!(ready(&mut confirmed).state, FileIoState::Ready);
    assert!(matches!(
        confirmed.apply(&mut core).unwrap(),
        FileIoApply::Complete { .. }
    ));
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
    let old_native_identity = manager.resolve_identity(&destination).unwrap().0;
    let old_raster_identity = manager
        .resolve_identity(&destination.with_extension("png"))
        .unwrap()
        .0;
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
    let mut request = FileIoRequest::new(FileIoKind::SavePair, vec![destination.clone()]);
    request.overwrite_confirmed = true;
    let mut job = FileIoJob::start(Some(&core), manager.clone(), request).unwrap();
    assert_eq!(ready(&mut job).state, FileIoState::Ready);
    let future_native_identity = job.item(0).unwrap().identity;
    let future_raster_identity = job.item(1).unwrap().identity;
    assert_ne!(future_native_identity, old_native_identity);
    assert_ne!(future_raster_identity, old_raster_identity);
    job.apply(&mut core).unwrap();
    ready(&mut job);
    job.apply(&mut core).unwrap();
    assert_eq!(job.item(0).unwrap().identity, future_native_identity);
    assert_eq!(job.item(1).unwrap().identity, future_raster_identity);
    assert_eq!(
        manager.resolve_identity(&destination).unwrap().0,
        future_native_identity
    );
    assert_eq!(
        manager
            .resolve_identity(&destination.with_extension("png"))
            .unwrap()
            .0,
        future_raster_identity
    );
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
