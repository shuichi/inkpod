use super::*;
use inkpod_format::{CommonRaster, encode_common_raster};
use std::time::{Duration, Instant};

static PATH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[test]
fn io_003_public_protocol_constants_match_abi_v31() {
    assert_eq!(INKPOD_IO_OPEN_RASTER_PAIR, 22);
    assert_eq!(INKPOD_IO_REVERT_CURRENT, 1_u64 << 4);
    assert_eq!(INKPOD_IO_RECOVERY_ARTIFACT_READONLY, 1_u32 << 0);
    assert_eq!(INKPOD_IO_RECOVERY_PAIR_NONE, 0);
    assert_eq!(INKPOD_IO_RECOVERY_PAIR_COMMITTED, 1);
    assert_eq!(INKPOD_IO_RECOVERY_PAIR_PLANNED, 2);
    assert_eq!(INKPOD_IO_RECOVERY_PAIR_REPAIR_NEEDED, 3);
}

fn wait_ready(job: *const InkpodIoJob) -> InkpodIoJobInfo {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let mut info = InkpodIoJobInfo {
            struct_size: size_of::<InkpodIoJobInfo>() as u32,
            ..Default::default()
        };
        // SAFETY: The test retains the live handle and stack output for the call.
        assert_eq!(
            unsafe { inkpod_io_job_poll(job, &mut info) },
            INKPOD_STATUS_OK
        );
        if matches!(
            info.state,
            INKPOD_IO_READY | INKPOD_IO_FAILED | INKPOD_IO_CANCELLED | INKPOD_IO_COMPLETE
        ) {
            return info;
        }
        assert!(
            Instant::now() < deadline,
            "I/O job stalled, state {}",
            info.state
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn request(path: &InkpodIoPath, kind: u32) -> InkpodIoRequest {
    InkpodIoRequest {
        struct_size: size_of::<InkpodIoRequest>() as u32,
        kind,
        flags: 0,
        paths: path,
        path_count: 1,
        path_stride_bytes: size_of::<InkpodIoPath>() as u64,
        object_id: 0,
        document_uuid_high: 0,
        document_uuid_low: 0,
        raster_format: 0,
        reserved: 0,
    }
}

fn path_input(text: &str) -> InkpodIoPath {
    InkpodIoPath {
        struct_size: size_of::<InkpodIoPath>() as u32,
        reserved: 0,
        path: text.as_ptr(),
        path_bytes: text.len() as u64,
    }
}

fn temporary_directory(label: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "inkpod-ffi-io-{label}-{}-{}",
        std::process::id(),
        PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    directory
}

fn create_blank_core(uuid: u64) -> *mut InkpodCore {
    let config = InkpodCoreConfig {
        struct_size: size_of::<InkpodCoreConfig>() as u32,
        abi_version: INKPOD_ABI_VERSION,
        feature_flags: 0,
    };
    let options = InkpodCellCreateOptions {
        struct_size: size_of::<InkpodCellCreateOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
        document_uuid_high: 0,
        document_uuid_low: uuid,
        width: 2,
        height: 2,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
    };
    let mut core = ptr::null_mut();
    let mut document = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..Default::default()
    };
    // SAFETY: Complete records and a unique empty owner remain live during each call.
    unsafe {
        assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut document),
            INKPOD_STATUS_OK
        );
    }
    core
}

#[test]
fn io_003_revert_flag_is_explicit_and_rejects_invalid_combinations() {
    let path_text = "missing.inkpod";
    let path = path_input(path_text);
    let mut manager = ptr::null_mut();
    let mut core = create_blank_core(89);
    let mut job = ptr::null_mut();
    // SAFETY: Complete records and path bytes remain live for each synchronous
    // submit, and both owners are released exactly once below.
    unsafe {
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_bind_io_manager(core, manager), INKPOD_STATUS_OK);

        let mut invalid = request(&path, INKPOD_IO_OPEN_NATIVE);
        invalid.flags = INKPOD_IO_REVERT_CURRENT;
        assert_eq!(
            inkpod_core_io_submit(core, manager, &invalid, &mut job),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(job.is_null());

        invalid.kind = INKPOD_IO_OPEN_RASTER;
        invalid.flags = INKPOD_IO_FORCE_RELOAD | INKPOD_IO_REVERT_CURRENT;
        assert_eq!(
            inkpod_core_io_submit(core, manager, &invalid, &mut job),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(job.is_null());

        invalid.kind = INKPOD_IO_OPEN_NATIVE;
        invalid.flags = 1_u64 << 63;
        assert_eq!(
            inkpod_core_io_submit(core, manager, &invalid, &mut job),
            INKPOD_STATUS_UNSUPPORTED
        );
        assert!(job.is_null());

        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
}

#[test]
fn io_003_missing_selected_native_is_reported_as_io_error() {
    let directory = temporary_directory("missing-native-open");
    let missing = directory.join("missing.inkpod");
    let missing_text = missing.to_str().unwrap();
    let input = path_input(missing_text);
    let mut manager = ptr::null_mut();
    let mut core = create_blank_core(891);
    let mut job = ptr::null_mut();
    // SAFETY: Complete records and path bytes remain live, and each returned
    // owner is released exactly once below.
    unsafe {
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_bind_io_manager(core, manager), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_io_submit(
                core,
                manager,
                &request(&input, INKPOD_IO_OPEN_NATIVE),
                &mut job,
            ),
            INKPOD_STATUS_OK
        );
        let progress = wait_ready(job);
        assert_eq!(progress.state, INKPOD_IO_FAILED);
        assert_eq!(progress.status, INKPOD_STATUS_IO_ERROR);
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn io_003_native_open_reports_existing_and_missing_companion_candidates() {
    let directory = temporary_directory("native-pair-items");
    let native_path = directory.join("source.inkpod");
    let raster_path = directory.join("source.png");
    let raster = CommonRaster::new(
        1,
        1,
        PixelFormat::StraightRgba8,
        Some(96_000),
        Some(96_000),
        vec![10, 20, 30, 255],
    )
    .unwrap();
    let mut fixture = inkpod_core::Core::new();
    fixture
        .import_decoded_common_raster(CommonRasterFormat::Png, &raster, 0x91)
        .unwrap();
    fixture.save(&native_path).unwrap();
    std::fs::write(
        &raster_path,
        encode_common_raster(CommonRasterFormat::Png, &raster, false).unwrap(),
    )
    .unwrap();

    let native_text = native_path.to_str().unwrap();
    let input = path_input(native_text);
    let mut manager = ptr::null_mut();
    let mut core = create_blank_core(90);
    let mut job = ptr::null_mut();
    let mut document = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..Default::default()
    };
    // SAFETY: Every input/output record and path span remains live, the Core
    // stays on this owner thread, and every returned handle is released once.
    unsafe {
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_bind_io_manager(core, manager), INKPOD_STATUS_OK);
        for expected_identity_kind in [1, 2] {
            assert_eq!(
                inkpod_core_io_submit(
                    core,
                    manager,
                    &request(&input, INKPOD_IO_OPEN_NATIVE),
                    &mut job,
                ),
                INKPOD_STATUS_OK
            );
            let progress = wait_ready(job);
            assert_eq!(
                (progress.state, progress.result_count),
                (INKPOD_IO_READY, 2)
            );
            let mut native = InkpodIoItemInfo {
                struct_size: size_of::<InkpodIoItemInfo>() as u32,
                ..Default::default()
            };
            let mut companion = native;
            assert_eq!(
                inkpod_io_job_get_item(job, 0, &mut native, ptr::null_mut(), 0, ptr::null_mut(), 0,),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_io_job_get_item(
                    job,
                    1,
                    &mut companion,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    0,
                ),
                INKPOD_STATUS_OK
            );
            assert_eq!((native.raster_format, native.identity.kind), (0, 1));
            assert_eq!(companion.raster_format, INKPOD_COMMON_RASTER_PNG);
            assert_eq!(companion.identity.kind, expected_identity_kind);
            let mut companion_path = vec![0; companion.path_bytes as usize];
            assert_eq!(
                inkpod_io_job_get_item(
                    job,
                    1,
                    &mut companion,
                    companion_path.as_mut_ptr(),
                    companion_path.len() as u64,
                    ptr::null_mut(),
                    0,
                ),
                INKPOD_STATUS_OK
            );
            let expected_path = if expected_identity_kind == 1 {
                std::fs::canonicalize(&raster_path).unwrap()
            } else {
                raster_path.clone()
            };
            assert_eq!(companion_path, expected_path.to_string_lossy().as_bytes());
            assert_eq!(
                inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
            if expected_identity_kind == 1 {
                std::fs::remove_file(&raster_path).unwrap();
            }
        }
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn io_003_raster_pair_open_reports_raster_then_missing_native_candidate() {
    let directory = temporary_directory("raster-pair");
    let raster_path = directory.join("source.png");
    let raster = CommonRaster::new(
        1,
        1,
        PixelFormat::StraightRgba8,
        Some(96_000),
        Some(96_000),
        vec![10, 20, 30, 255],
    )
    .unwrap();
    std::fs::write(
        &raster_path,
        encode_common_raster(CommonRasterFormat::Png, &raster, false).unwrap(),
    )
    .unwrap();
    let raster_text = raster_path.to_str().unwrap();
    let input = path_input(raster_text);
    let mut manager = ptr::null_mut();
    let mut core = create_blank_core(91);
    let mut job = ptr::null_mut();
    // SAFETY: All records and path bytes remain live for the calls, and each
    // returned owner is released exactly once below.
    unsafe {
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_bind_io_manager(core, manager), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_io_submit(
                core,
                manager,
                &request(&input, INKPOD_IO_OPEN_RASTER_PAIR),
                &mut job,
            ),
            INKPOD_STATUS_OK
        );
        let progress = wait_ready(job);
        assert_eq!(progress.kind, INKPOD_IO_OPEN_RASTER_PAIR);
        assert_eq!(
            (progress.state, progress.result_count),
            (INKPOD_IO_READY, 2)
        );

        let mut first = InkpodIoItemInfo {
            struct_size: size_of::<InkpodIoItemInfo>() as u32,
            ..Default::default()
        };
        let mut second = first;
        assert_eq!(
            inkpod_io_job_get_item(job, 0, &mut first, ptr::null_mut(), 0, ptr::null_mut(), 0),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_io_job_get_item(job, 1, &mut second, ptr::null_mut(), 0, ptr::null_mut(), 0),
            INKPOD_STATUS_OK
        );
        assert_eq!(first.raster_format, INKPOD_COMMON_RASTER_PNG);
        assert_eq!(first.identity.kind, 1);
        assert_eq!(second.raster_format, 0);
        assert_eq!(second.identity.kind, 2);
        assert_eq!(
            (first.document_uuid_high, first.document_uuid_low),
            (second.document_uuid_high, second.document_uuid_low)
        );

        let mut document = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..Default::default()
        };
        let mut object_id = 0;
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, &mut object_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn io_003_sequence_raster_pair_submit_requires_target_and_reports_raster_then_native() {
    let directory = temporary_directory("sequence-raster-pair");
    let first_path = directory.join("cell1.png");
    let target_path = directory.join("cell2.png");
    for (path, format, pixel) in [
        (&first_path, CommonRasterFormat::Png, 10),
        (&target_path, CommonRasterFormat::Png, 20),
    ] {
        let raster = CommonRaster::new(
            1,
            1,
            PixelFormat::StraightRgba8,
            Some(96_000),
            Some(96_000),
            vec![pixel, 0, 0, 255],
        )
        .unwrap();
        std::fs::write(path, encode_common_raster(format, &raster, false).unwrap()).unwrap();
    }
    let first_text = first_path.to_str().unwrap();
    let target_text = target_path.to_str().unwrap();
    let first = path_input(first_text);
    let target = path_input(target_text);
    let sequence_paths = [path_input(first_text), path_input(target_text)];
    let empty_target = path_input("");
    let mut manager = ptr::null_mut();
    let mut core = create_blank_core(92);
    let mut job = ptr::null_mut();
    let mut document = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..Default::default()
    };
    // SAFETY: All records and their path spans remain live for every call. The
    // opaque owners stay on this thread and are released exactly once below.
    unsafe {
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_bind_io_manager(core, manager), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_io_submit(
                core,
                manager,
                &request(&first, INKPOD_IO_OPEN_RASTER_PAIR),
                &mut job,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);

        let mut sequence = request(&sequence_paths[0], INKPOD_IO_SEQUENCE_FILES);
        sequence.paths = sequence_paths.as_ptr();
        sequence.path_count = sequence_paths.len() as u64;
        assert_eq!(
            inkpod_core_io_submit(core, manager, &sequence, &mut job),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);

        let mut switch = InkpodSequenceSwitchRequest {
            struct_size: size_of::<InkpodSequenceSwitchRequest>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_sequence_switch_request(
                core,
                1,
                INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
                &mut switch,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(switch.flags, INKPOD_SEQUENCE_SWITCH_REQUIRED);

        switch.feature_flags = INKPOD_SEQUENCE_SWITCH_TARGET_RASTER_PAIR | (1_u64 << 63);
        assert_eq!(
            inkpod_core_io_sequence_switch_submit(
                core,
                manager,
                &switch,
                ptr::null(),
                &target,
                ptr::null(),
                ptr::null(),
                &mut job,
            ),
            INKPOD_STATUS_UNSUPPORTED
        );
        assert!(job.is_null());

        switch.feature_flags = INKPOD_SEQUENCE_SWITCH_TARGET_RASTER_PAIR;
        for missing_target in [ptr::null(), &empty_target as *const InkpodIoPath] {
            assert_eq!(
                inkpod_core_io_sequence_switch_submit(
                    core,
                    manager,
                    &switch,
                    ptr::null(),
                    missing_target,
                    ptr::null(),
                    ptr::null(),
                    &mut job,
                ),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            assert!(job.is_null());
        }

        assert_eq!(
            inkpod_core_io_sequence_switch_submit(
                core,
                manager,
                &switch,
                ptr::null(),
                &target,
                ptr::null(),
                ptr::null(),
                &mut job,
            ),
            INKPOD_STATUS_OK
        );
        let progress = wait_ready(job);
        let mut error_size = 0;
        assert_eq!(
            inkpod_io_job_copy_error(job, ptr::null_mut(), 0, &mut error_size),
            INKPOD_STATUS_OK
        );
        let mut error = vec![0; error_size as usize];
        assert_eq!(
            inkpod_io_job_copy_error(job, error.as_mut_ptr(), error.len() as u64, &mut error_size,),
            INKPOD_STATUS_OK
        );
        assert_eq!(progress.kind, INKPOD_IO_SEQUENCE_SWITCH);
        assert_eq!(
            (progress.state, progress.result_count),
            (INKPOD_IO_READY, 2),
            "{}",
            std::str::from_utf8(&error).unwrap()
        );

        let mut raster_item = InkpodIoItemInfo {
            struct_size: size_of::<InkpodIoItemInfo>() as u32,
            ..Default::default()
        };
        let mut native_item = raster_item;
        assert_eq!(
            inkpod_io_job_get_item(
                job,
                0,
                &mut raster_item,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                0,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_io_job_get_item(
                job,
                1,
                &mut native_item,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                0,
            ),
            INKPOD_STATUS_OK
        );
        let mut raster_path = vec![0; raster_item.path_bytes as usize];
        let mut native_path = vec![0; native_item.path_bytes as usize];
        assert_eq!(
            inkpod_io_job_get_item(
                job,
                0,
                &mut raster_item,
                raster_path.as_mut_ptr(),
                raster_path.len() as u64,
                ptr::null_mut(),
                0,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_io_job_get_item(
                job,
                1,
                &mut native_item,
                native_path.as_mut_ptr(),
                native_path.len() as u64,
                ptr::null_mut(),
                0,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            std::path::Path::new(std::str::from_utf8(&raster_path).unwrap()),
            std::fs::canonicalize(&target_path).unwrap()
        );
        assert_eq!(
            std::path::Path::new(std::str::from_utf8(&native_path).unwrap()),
            std::fs::canonicalize(&directory)
                .unwrap()
                .join("cell2.inkpod")
        );
        assert_eq!(raster_item.raster_format, INKPOD_COMMON_RASTER_PNG);
        assert_eq!(raster_item.identity.kind, 1);
        assert_eq!(native_item.raster_format, 0);
        assert_eq!(native_item.identity.kind, 2);
        assert_eq!(
            (
                raster_item.document_uuid_high,
                raster_item.document_uuid_low
            ),
            (
                native_item.document_uuid_high,
                native_item.document_uuid_low
            )
        );

        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

fn recovery_input(original: &str, source: &str) -> InkpodIoRecoveryMetadata {
    InkpodIoRecoveryMetadata {
        struct_size: size_of::<InkpodIoRecoveryMetadata>() as u32,
        flags: 1,
        session_id: 7,
        generation: 9,
        document_uuid_high: 0,
        document_uuid_low: 42,
        written_time_100ns: 123,
        modified_time_100ns: 0,
        identity_kind: 3,
        reserved: 0,
        identity_volume: 0,
        identity_object_high: 11,
        identity_object_low: 13,
        original_path: path_input(original),
        source_path: path_input(source),
        identity_path: path_input(""),
        pair_proof: recovery_pair_none_input(),
    }
}

fn recovery_pair_empty_stamp(kind: u32, object_low: u64) -> InkpodIoRecoveryArtifactStamp {
    InkpodIoRecoveryArtifactStamp {
        struct_size: size_of::<InkpodIoRecoveryArtifactStamp>() as u32,
        identity: InkpodIoFileIdentity {
            struct_size: size_of::<InkpodIoFileIdentity>() as u32,
            kind,
            object_low,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn recovery_pair_none_input() -> InkpodIoRecoveryPairProof {
    InkpodIoRecoveryPairProof {
        struct_size: size_of::<InkpodIoRecoveryPairProof>() as u32,
        kind: INKPOD_IO_RECOVERY_PAIR_NONE,
        native: recovery_pair_empty_stamp(0, 0),
        raster: recovery_pair_empty_stamp(0, 0),
    }
}

fn recovery_artifact_stamp_input() -> InkpodIoRecoveryArtifactStamp {
    InkpodIoRecoveryArtifactStamp {
        struct_size: size_of::<InkpodIoRecoveryArtifactStamp>() as u32,
        identity: InkpodIoFileIdentity {
            struct_size: size_of::<InkpodIoFileIdentity>() as u32,
            kind: 1,
            object_low: 1,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn recovery_artifact_proof_input() -> InkpodIoRecoveryArtifactProof {
    InkpodIoRecoveryArtifactProof {
        struct_size: size_of::<InkpodIoRecoveryArtifactProof>() as u32,
        native: recovery_artifact_stamp_input(),
        metadata: recovery_artifact_stamp_input(),
        ..Default::default()
    }
}

fn publish_test_recovery(
    core: *mut InkpodCore,
    manager: *mut InkpodIoManager,
    path: &std::path::Path,
    metadata: &InkpodIoRecoveryMetadata,
) -> InkpodIoRecoveryArtifactProof {
    let text = path.to_str().unwrap();
    let mut job = ptr::null_mut();
    let mut document = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..Default::default()
    };
    let mut proof = recovery_artifact_proof_input();
    // SAFETY: The caller retains both live handles and every stack/span input
    // until each synchronous ABI call returns.
    unsafe {
        assert_eq!(
            inkpod_core_io_autosave_submit(
                core,
                manager,
                text.as_ptr(),
                text.len() as u64,
                metadata,
                &mut job,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_io_job_get_recovery_artifact_proof(job, &mut proof),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
    }
    proof
}

fn exact_discard_progress(
    manager: *mut InkpodIoManager,
    path: &std::path::Path,
    proof: &InkpodIoRecoveryArtifactProof,
) -> InkpodIoJobInfo {
    let text = path.to_str().unwrap();
    let mut job = ptr::null_mut();
    // SAFETY: The live manager and complete path/proof/output values outlive
    // submit; the accepted job owns copied inputs until release.
    unsafe {
        assert_eq!(
            inkpod_core_io_recovery_discard_exact_submit(
                ptr::null_mut(),
                manager,
                text.as_ptr(),
                text.len() as u64,
                proof,
                &mut job,
            ),
            INKPOD_STATUS_OK
        );
        let progress = wait_ready(job);
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        progress
    }
}

#[test]
fn io_003_recovery_artifact_proof_rejects_weakened_abi_stamps() {
    let valid = recovery_artifact_proof_input();
    // SAFETY: Every test record is a complete live size-prefixed value.
    unsafe {
        assert!(recovery::parse_artifact_proof(&valid).is_ok());

        let mut zero_identity = valid;
        zero_identity.native.identity.volume = 0;
        zero_identity.native.identity.object_high = 0;
        zero_identity.native.identity.object_low = 0;
        assert!(recovery::parse_artifact_proof(&zero_identity).is_err());

        let mut short_nested = valid;
        short_nested.metadata.struct_size = 0;
        assert!(recovery::parse_artifact_proof(&short_nested).is_err());

        let mut short_identity = valid;
        short_identity.native.identity.struct_size = 0;
        assert!(recovery::parse_artifact_proof(&short_identity).is_err());

        let mut unknown_flags = valid;
        unknown_flags.native.flags = 2;
        assert!(recovery::parse_artifact_proof(&unknown_flags).is_err());

        let mut reserved = valid;
        reserved.reserved = 1;
        assert!(recovery::parse_artifact_proof(&reserved).is_err());
    }
}

#[test]
fn io_003_recovery_codec_owns_text_and_preserves_independent_untitled_identity() {
    let input = recovery_input("日本語/原画.inkpod", "原画001.png");
    let mut encoded_size = 0;
    let mut text_size = 0;
    let mut output = recovery_input("", "");
    // SAFETY: Input strings, complete output records and nonoverlapping buffers
    // live for each call. Negative cases use only null/short in-bounds records.
    unsafe {
        assert_eq!(
            inkpod_recovery_metadata_encode(&input, ptr::null_mut(), 0, &mut encoded_size),
            INKPOD_STATUS_OK
        );
        let mut encoded = vec![0; encoded_size as usize];
        assert_eq!(
            inkpod_recovery_metadata_encode(&input, encoded.as_mut_ptr(), 1, &mut encoded_size),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(
            inkpod_recovery_metadata_encode(
                &input,
                encoded.as_mut_ptr(),
                encoded.len() as u64,
                &mut encoded_size
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_recovery_metadata_decode(
                encoded.as_ptr(),
                encoded_size,
                &mut output,
                ptr::null_mut(),
                0,
                &mut text_size
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            text_size,
            input.original_path.path_bytes + input.source_path.path_bytes
        );
        assert!(output.original_path.path.is_null());
        let mut text = vec![0; text_size as usize];
        assert_eq!(
            inkpod_recovery_metadata_decode(
                encoded.as_ptr(),
                encoded_size,
                &mut output,
                text.as_mut_ptr(),
                1,
                &mut text_size
            ),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(
            inkpod_recovery_metadata_decode(
                encoded.as_ptr(),
                encoded_size,
                &mut output,
                text.as_mut_ptr(),
                text.len() as u64,
                &mut text_size
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (output.document_uuid_high, output.document_uuid_low),
            (0, 42)
        );
        assert_eq!(
            (output.identity_object_high, output.identity_object_low),
            (11, 13)
        );
        assert_eq!(output.pair_proof.kind, INKPOD_IO_RECOVERY_PAIR_NONE);
        assert_eq!(
            slice::from_raw_parts(
                output.original_path.path,
                output.original_path.path_bytes as usize
            ),
            "日本語/原画.inkpod".as_bytes()
        );
        assert_eq!(
            slice::from_raw_parts(
                output.source_path.path,
                output.source_path.path_bytes as usize
            ),
            "原画001.png".as_bytes()
        );
        let mut invalid = input;
        invalid.identity_kind = u32::MAX;
        assert_eq!(
            inkpod_recovery_metadata_encode(&invalid, ptr::null_mut(), 0, &mut encoded_size),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut planned = input;
        planned.pair_proof.kind = INKPOD_IO_RECOVERY_PAIR_PLANNED;
        planned.pair_proof.native = recovery_pair_empty_stamp(2, 0x1234);
        planned.pair_proof.native.identity.volume = u64::MAX;
        planned.pair_proof.raster = recovery_artifact_stamp_input();
        planned.pair_proof.raster.length = 17;
        assert_eq!(
            inkpod_recovery_metadata_encode(&planned, ptr::null_mut(), 0, &mut encoded_size),
            INKPOD_STATUS_OK
        );
        let mut planned_bytes = vec![0; encoded_size as usize];
        assert_eq!(
            inkpod_recovery_metadata_encode(
                &planned,
                planned_bytes.as_mut_ptr(),
                planned_bytes.len() as u64,
                &mut encoded_size
            ),
            INKPOD_STATUS_OK
        );
        let mut planned_output = recovery_input("", "");
        assert_eq!(
            inkpod_recovery_metadata_decode(
                planned_bytes.as_ptr(),
                planned_bytes.len() as u64,
                &mut planned_output,
                ptr::null_mut(),
                0,
                &mut text_size
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            planned_output.pair_proof.kind,
            INKPOD_IO_RECOVERY_PAIR_PLANNED
        );
        assert_eq!(planned_output.pair_proof.native.identity.volume, u64::MAX);
        assert_eq!(planned_output.pair_proof.native.identity.object_low, 0x1234);
        planned.pair_proof.native.identity.volume = 0;
        assert_eq!(
            inkpod_recovery_metadata_encode(&planned, ptr::null_mut(), 0, &mut encoded_size),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        planned.pair_proof.native.identity.volume = u64::MAX;
        planned.pair_proof.native.identity.struct_size = 0;
        assert_eq!(
            inkpod_recovery_metadata_encode(&planned, ptr::null_mut(), 0, &mut encoded_size),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        let mut repair = input;
        repair.pair_proof.kind = INKPOD_IO_RECOVERY_PAIR_REPAIR_NEEDED;
        repair.pair_proof.native = recovery_artifact_stamp_input();
        repair.pair_proof.native.length = 29;
        repair.pair_proof.raster = recovery_pair_empty_stamp(2, 0x5678);
        repair.pair_proof.raster.identity.volume = u64::MAX;
        assert_eq!(
            inkpod_recovery_metadata_encode(&repair, ptr::null_mut(), 0, &mut encoded_size),
            INKPOD_STATUS_OK
        );
        let mut repair_bytes = vec![0; encoded_size as usize];
        assert_eq!(
            inkpod_recovery_metadata_encode(
                &repair,
                repair_bytes.as_mut_ptr(),
                repair_bytes.len() as u64,
                &mut encoded_size
            ),
            INKPOD_STATUS_OK
        );
        let mut repair_output = recovery_input("", "");
        assert_eq!(
            inkpod_recovery_metadata_decode(
                repair_bytes.as_ptr(),
                repair_bytes.len() as u64,
                &mut repair_output,
                ptr::null_mut(),
                0,
                &mut text_size
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            repair_output.pair_proof.kind,
            INKPOD_IO_RECOVERY_PAIR_REPAIR_NEEDED
        );
        assert_eq!(repair_output.pair_proof.native.identity.kind, 1);
        assert_eq!(repair_output.pair_proof.raster.identity.kind, 2);
        assert_eq!(repair_output.pair_proof.raster.identity.volume, u64::MAX);
        repair.pair_proof.raster.identity.volume = 0;
        assert_eq!(
            inkpod_recovery_metadata_encode(&repair, ptr::null_mut(), 0, &mut encoded_size),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        encoded[0] ^= 1;
        assert_ne!(
            inkpod_recovery_metadata_decode(
                encoded.as_ptr(),
                encoded.len() as u64,
                &mut output,
                ptr::null_mut(),
                0,
                &mut text_size
            ),
            INKPOD_STATUS_OK
        );
        output.struct_size = 4;
        assert_eq!(
            inkpod_recovery_metadata_decode(
                encoded.as_ptr(),
                encoded.len() as u64,
                &mut output,
                ptr::null_mut(),
                0,
                &mut text_size
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
    }
}

#[test]
fn io_003_autosave_catalog_probe_and_discard_share_rust_io_without_normal_savepoint() {
    let directory = temporary_directory("recovery");
    let recovery = directory.join("session.recovery.inkpod");
    let source = directory.join("source.png");
    let original = directory.join("original.inkpod");
    let recovery_text = recovery.to_str().unwrap();
    let mut input = recovery_input(original.to_str().unwrap(), source.to_str().unwrap());
    input.written_time_100ns = 0;
    let mut core = create_blank_core(42);
    let (mut manager, mut job) = (ptr::null_mut(), ptr::null_mut());
    let mut document = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..Default::default()
    };
    // SAFETY: Unique live handles, stack records and UTF-8 inputs outlive each call.
    unsafe {
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_bind_io_manager(core, manager), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        let before = (document.document_revision, document.flags);
        input.document_uuid_low += 1;
        assert_eq!(
            inkpod_core_io_autosave_submit(
                core,
                manager,
                recovery_text.as_ptr(),
                recovery_text.len() as u64,
                &input,
                &mut job
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(job.is_null());
        assert!(!recovery.exists());
        input.document_uuid_low -= 1;
        assert_eq!(
            inkpod_core_io_autosave_submit(
                core,
                manager,
                recovery_text.as_ptr(),
                recovery_text.len() as u64,
                &input,
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        let mut proof = recovery_artifact_proof_input();
        assert_eq!(
            inkpod_io_job_get_recovery_artifact_proof(job, &mut proof),
            INKPOD_STATUS_OK
        );
        assert_eq!(proof.native.identity.kind, 1);
        assert_eq!(proof.metadata.identity.kind, 1);
        assert_ne!(proof.native.identity.object_low, 0);
        assert_ne!(proof.metadata.identity.object_low, 0);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert_eq!((document.document_revision, document.flags), before);
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert!(recovery.exists());
        assert!(!recovery.with_extension("png").exists());
        assert!(!original.exists());

        assert_eq!(
            inkpod_core_io_autosave_submit(
                core,
                manager,
                recovery_text.as_ptr(),
                recovery_text.len() as u64,
                &input,
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_FAILED);
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert!(
            recovery.exists(),
            "append-only autosave kept its first generation"
        );

        let root = path_input(directory.to_str().unwrap());
        assert_eq!(
            inkpod_core_io_submit(
                ptr::null_mut(),
                manager,
                &request(&root, INKPOD_IO_RECOVERY_LIST),
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        let info = wait_ready(job);
        assert_eq!((info.state, info.result_count), (INKPOD_IO_COMPLETE, 1));
        let mut metadata = recovery_input("", "");
        let mut required = 0;
        assert_eq!(
            inkpod_io_job_get_recovery_metadata(
                job,
                0,
                &mut metadata,
                ptr::null_mut(),
                0,
                &mut required
            ),
            INKPOD_STATUS_OK
        );
        let mut text = vec![0; required as usize];
        assert_eq!(
            inkpod_io_job_get_recovery_metadata(
                job,
                0,
                &mut metadata,
                text.as_mut_ptr(),
                text.len() as u64,
                &mut required
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (metadata.flags, metadata.session_id, metadata.generation),
            (1, 7, 9)
        );
        assert_eq!(
            (metadata.identity_object_high, metadata.identity_object_low),
            (11, 13)
        );
        assert!(metadata.written_time_100ns > 0);
        assert_eq!(
            inkpod_io_job_get_recovery_metadata(
                job,
                1,
                &mut metadata,
                ptr::null_mut(),
                0,
                &mut required
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);

        let paths = [
            path_input(original.to_str().unwrap()),
            path_input(recovery_text),
        ];
        let mut probe = request(&paths[0], INKPOD_IO_RECOVERY_PROBE);
        probe.paths = paths.as_ptr();
        probe.path_count = paths.len() as u64;
        assert_eq!(
            inkpod_core_io_submit(ptr::null_mut(), manager, &probe, &mut job),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            wait_ready(job).result_count,
            1,
            "recovery is newer than absent original"
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);

        let missing_native = directory.join("missing-native.recovery.inkpod");
        let missing_native_proof = publish_test_recovery(core, manager, &missing_native, &input);
        let missing_metadata = directory.join("missing-metadata.recovery.inkpod");
        let missing_metadata_proof =
            publish_test_recovery(core, manager, &missing_metadata, &input);
        let mixed = directory.join("mixed.recovery.inkpod");
        let mixed_proof = publish_test_recovery(core, manager, &mixed, &input);
        let mixed_donor = directory.join("mixed-donor.recovery.inkpod");
        let _mixed_donor_proof = publish_test_recovery(core, manager, &mixed_donor, &input);

        let missing_native_sidecar = inkpod_io::recovery_metadata_path(&missing_native).unwrap();
        let missing_metadata_sidecar =
            inkpod_io::recovery_metadata_path(&missing_metadata).unwrap();
        let mixed_sidecar = inkpod_io::recovery_metadata_path(&mixed).unwrap();
        let mixed_donor_sidecar = inkpod_io::recovery_metadata_path(&mixed_donor).unwrap();
        std::fs::remove_file(&missing_native).unwrap();
        std::fs::remove_file(&missing_metadata_sidecar).unwrap();
        std::fs::remove_file(&mixed_sidecar).unwrap();
        std::fs::rename(&mixed_donor_sidecar, &mixed_sidecar).unwrap();
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        let mut wrong_proof = proof;
        wrong_proof.native.length += 1;
        assert_eq!(
            inkpod_core_io_recovery_discard_exact_submit(
                ptr::null_mut(),
                manager,
                recovery_text.as_ptr(),
                recovery_text.len() as u64,
                &wrong_proof,
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        let wrong_discard = wait_ready(job);
        assert_eq!(wrong_discard.state, INKPOD_IO_FAILED);
        assert_eq!(wrong_discard.status, INKPOD_STATUS_FILE_CONFLICT);
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert!(recovery.exists(), "wrong proof must not delete recovery");
        for (path, proof) in [
            (&missing_native, &missing_native_proof),
            (&missing_metadata, &missing_metadata_proof),
            (&mixed, &mixed_proof),
        ] {
            let conflict = exact_discard_progress(manager, path, proof);
            assert_eq!(conflict.state, INKPOD_IO_FAILED);
            assert_eq!(conflict.status, INKPOD_STATUS_FILE_CONFLICT);
        }
        assert!(missing_native_sidecar.exists());
        assert!(missing_metadata.exists());
        assert!(mixed.exists());
        assert!(mixed_sidecar.exists());
        assert_eq!(
            inkpod_core_io_recovery_discard_exact_submit(
                ptr::null_mut(),
                manager,
                recovery_text.as_ptr(),
                recovery_text.len() as u64,
                &proof,
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        let discard_state = wait_ready(job).state;
        assert!(matches!(
            discard_state,
            INKPOD_IO_COMPLETE | INKPOD_IO_FAILED
        ));
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert_eq!(
            recovery.exists(),
            discard_state == INKPOD_IO_FAILED,
            "only a completed exact discard may remove the proven artifact"
        );
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn io_003_sequence_switch_and_compacted_copy_require_owner_finalization() {
    let directory = temporary_directory("session");
    let first_path = directory.join("cell1.png");
    let second = directory.join("cell2.bmp");
    for (path, format, pixel) in [
        (&first_path, CommonRasterFormat::Png, 10),
        (&second, CommonRasterFormat::Bmp, 20),
    ] {
        let raster = CommonRaster::new(
            1,
            1,
            PixelFormat::StraightRgba8,
            None,
            None,
            vec![pixel, 0, 0, 255],
        )
        .unwrap();
        std::fs::write(path, encode_common_raster(format, &raster, false).unwrap()).unwrap();
    }
    let first_text = first_path.to_str().unwrap().to_owned();
    let first = path_input(&first_text);
    let source_recovery = directory.join("source.recovery.inkpod");
    let recovery = path_input(source_recovery.to_str().unwrap());
    let cancelled_recovery = directory.join("cancelled-after-ready.recovery.inkpod");
    let cancelled_recovery_input = path_input(cancelled_recovery.to_str().unwrap());
    let missing_target_path = directory.join("missing-target.inkpod");
    let corrupt_target_path = directory.join("corrupt-target.inkpod");
    std::fs::write(&corrupt_target_path, b"not an inkpod recovery").unwrap();
    let missing_target = path_input(missing_target_path.to_str().unwrap());
    let corrupt_target = path_input(corrupt_target_path.to_str().unwrap());
    let compacted = directory.join("compacted.inkpod");
    let compacted_text = compacted.to_str().unwrap();
    let mut core = create_blank_core(99);
    let (mut manager, mut job) = (ptr::null_mut(), ptr::null_mut());
    let mut document = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..Default::default()
    };
    // SAFETY: Handles are owned on this thread; inputs and outputs are complete,
    // aligned and remain live, including all buffers referenced by path records.
    unsafe {
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_bind_io_manager(core, manager), INKPOD_STATUS_OK);
        for kind in [INKPOD_IO_OPEN_RASTER_PAIR, INKPOD_IO_SEQUENCE_AUTO] {
            assert_eq!(
                inkpod_core_io_submit(core, manager, &request(&first, kind), &mut job),
                INKPOD_STATUS_OK
            );
            assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
            if kind == INKPOD_IO_SEQUENCE_AUTO {
                let mut active = InkpodIoSequenceResidentInfo {
                    struct_size: size_of::<InkpodIoSequenceResidentInfo>() as u32,
                    ..Default::default()
                };
                assert_eq!(
                    inkpod_io_job_get_sequence_resident(job, 0, &mut active, ptr::null_mut(), 0,),
                    INKPOD_STATUS_OK
                );
                assert_eq!(active.flags, 0);
                let mut resident = InkpodIoSequenceResidentInfo {
                    struct_size: size_of::<InkpodIoSequenceResidentInfo>() as u32,
                    ..Default::default()
                };
                assert_eq!(
                    inkpod_io_job_get_sequence_resident(job, 1, &mut resident, ptr::null_mut(), 0,),
                    INKPOD_STATUS_OK
                );
                assert_eq!(resident.flags, INKPOD_IO_SEQUENCE_RESIDENT_AVAILABLE);
                assert_eq!(resident.native_identity.kind, 2);
                assert!(resident.native_path_bytes > 0);
                let mut native_path = vec![0; resident.native_path_bytes as usize];
                assert_eq!(
                    inkpod_io_job_get_sequence_resident(
                        job,
                        1,
                        &mut resident,
                        native_path.as_mut_ptr(),
                        native_path.len() as u64,
                    ),
                    INKPOD_STATUS_OK
                );
                assert!(
                    std::str::from_utf8(&native_path)
                        .unwrap()
                        .ends_with("cell2.inkpod")
                );
            }
            assert_eq!(
                inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        }
        let original_uuid = (document.document_uuid_high, document.document_uuid_low);
        let mut original_editor = InkpodEditorStateInfo {
            struct_size: size_of::<InkpodEditorStateInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_get_editor_state(core, &mut original_editor),
            INKPOD_STATUS_OK
        );
        assert_eq!(document.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        assert_eq!(original_editor.flags & INKPOD_EDITOR_STATE_DIRTY, 0);
        let mut switch = InkpodSequenceSwitchRequest {
            struct_size: size_of::<InkpodSequenceSwitchRequest>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_sequence_switch_request(
                core,
                1,
                INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
                &mut switch
            ),
            INKPOD_STATUS_OK
        );
        let before_failed_target = (
            document.document_uuid_high,
            document.document_uuid_low,
            document.document_revision,
            document.flags,
        );
        let invalid_proof = recovery_artifact_proof_input();
        assert_eq!(
            inkpod_core_io_sequence_switch_submit(
                core,
                manager,
                &switch,
                ptr::null(),
                &missing_target,
                ptr::null(),
                ptr::null(),
                &mut job,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(job.is_null());
        for explicit_target in [&missing_target, &corrupt_target] {
            assert_eq!(
                inkpod_core_io_sequence_switch_submit(
                    core,
                    manager,
                    &switch,
                    ptr::null(),
                    explicit_target,
                    &invalid_proof,
                    ptr::null(),
                    &mut job,
                ),
                INKPOD_STATUS_OK
            );
            assert_eq!(wait_ready(job).state, INKPOD_IO_FAILED);
            assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                (
                    document.document_uuid_high,
                    document.document_uuid_low,
                    document.document_revision,
                    document.flags,
                ),
                before_failed_target
            );
        }
        assert_eq!(
            inkpod_core_io_sequence_switch_submit(
                core,
                manager,
                ptr::null(),
                &recovery,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                &mut job
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut source_metadata = recovery_input("", "");
        source_metadata.document_uuid_high = document.document_uuid_high;
        source_metadata.document_uuid_low = document.document_uuid_low;
        assert_eq!(
            inkpod_core_io_sequence_switch_submit(
                core,
                manager,
                &switch,
                &recovery,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                &mut job,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(job.is_null());
        // Simulate a frontend failure while copying fixed proof/binding data
        // from the final installing READY. Cancel must suppress the target
        // commit, and the mandatory final apply must release the Core fence.
        assert_eq!(
            inkpod_core_io_sequence_switch_submit(
                core,
                manager,
                &switch,
                &cancelled_recovery_input,
                ptr::null(),
                ptr::null(),
                &source_metadata,
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_PENDING
        );
        let final_ready = wait_ready(job);
        assert_eq!(final_ready.state, INKPOD_IO_READY);
        assert_ne!(final_ready.flags & INKPOD_IO_RESULT_INSTALLING, 0);
        let mut unavailable_metadata = recovery_input("", "");
        let mut unavailable_bytes = 0;
        assert_eq!(
            inkpod_io_job_get_recovery_metadata(
                job,
                1,
                &mut unavailable_metadata,
                ptr::null_mut(),
                0,
                &mut unavailable_bytes,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(inkpod_io_job_cancel(job), INKPOD_STATUS_OK);
        let cancelled_ready = wait_ready(job);
        assert_eq!(cancelled_ready.state, INKPOD_IO_READY);
        assert_ne!(cancelled_ready.flags & INKPOD_IO_RESULT_INSTALLING, 0);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_CANCELLED
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert_eq!(
            (document.document_uuid_high, document.document_uuid_low),
            original_uuid
        );
        assert!(cancelled_recovery.exists());
        assert_eq!(
            inkpod_core_io_sequence_switch_submit(
                core,
                manager,
                &switch,
                &recovery,
                ptr::null(),
                ptr::null(),
                &source_metadata,
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        let mut source_proof = recovery_artifact_proof_input();
        assert_eq!(
            inkpod_io_job_get_recovery_artifact_proof(job, &mut source_proof),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_PENDING
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_INVALID_STATE);
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_io_job_get_recovery_artifact_proof(job, &mut source_proof),
            INKPOD_STATUS_OK
        );
        let mut effective_metadata = recovery_input("", "");
        let mut metadata_bytes = 0;
        assert_eq!(
            inkpod_io_job_get_recovery_metadata(
                job,
                0,
                &mut effective_metadata,
                ptr::null_mut(),
                0,
                &mut metadata_bytes,
            ),
            INKPOD_STATUS_OK
        );
        let mut metadata_text = vec![0; metadata_bytes as usize];
        assert_eq!(
            inkpod_io_job_get_recovery_metadata(
                job,
                0,
                &mut effective_metadata,
                metadata_text.as_mut_ptr(),
                metadata_text.len() as u64,
                &mut metadata_bytes,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            effective_metadata.pair_proof.kind,
            INKPOD_IO_RECOVERY_PAIR_PLANNED
        );
        assert_eq!(effective_metadata.pair_proof.native.identity.kind, 2);
        assert_eq!(
            effective_metadata.pair_proof.native.identity.volume,
            u64::MAX
        );
        assert_eq!(effective_metadata.pair_proof.raster.identity.kind, 1);
        let effective_source = std::str::from_utf8(slice::from_raw_parts(
            effective_metadata.source_path.path,
            effective_metadata.source_path.path_bytes as usize,
        ))
        .unwrap();
        assert_eq!(
            std::path::Path::new(effective_source),
            std::fs::canonicalize(&first_path).unwrap()
        );
        let parsed_effective = recovery::parse_metadata(&effective_metadata).unwrap();
        assert!(matches!(
            parsed_effective.pair_proof,
            Some(inkpod_io::RecoveryPairProof::Planned {
                native_missing: inkpod_io::FileIdentity {
                    volume: u64::MAX,
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert_ne!(
            (document.document_uuid_high, document.document_uuid_low),
            original_uuid
        );
        assert!(source_recovery.exists());
        assert!(!source_recovery.with_extension("png").exists());
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);

        let mut plan = InkpodCompactionPlan {
            struct_size: size_of::<InkpodCompactionPlan>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_core_compaction_plan(core, &mut plan),
            INKPOD_STATUS_OK
        );
        let before = (document.document_revision, document.flags);
        plan.reserved = 1;
        assert_eq!(
            inkpod_core_io_compacted_copy_submit(
                core,
                manager,
                compacted_text.as_ptr(),
                compacted_text.len() as u64,
                &plan,
                &mut job
            ),
            INKPOD_STATUS_UNSUPPORTED
        );
        plan.reserved = 0;
        assert_eq!(
            inkpod_core_io_compacted_copy_submit(
                core,
                manager,
                compacted_text.as_ptr(),
                compacted_text.len() as u64,
                &plan,
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_PENDING
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert_eq!((document.document_revision, document.flags), before);
        assert!(compacted.exists());
        assert!(!compacted.with_extension("bmp").exists());
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        let selected = directory.join("export.png");
        let paths = [
            path_input(directory.to_str().unwrap()),
            path_input(selected.to_str().unwrap()),
        ];
        let mut export = request(&paths[0], INKPOD_IO_EXPORT_SEQUENCE);
        export.paths = paths.as_ptr();
        export.path_count = 2;
        export.raster_format = INKPOD_COMMON_RASTER_PNG;
        assert_eq!(
            inkpod_core_io_submit(core, manager, &export, &mut job),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert!(directory.join("export-cell1.png").exists());
        assert!(directory.join("export-cell2.png").exists());
        assert!(!selected.exists());
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        let recovery_bytes = std::fs::read(&source_recovery).unwrap();
        let file_count = std::fs::read_dir(&directory).unwrap().count();
        let empty_source = path_input("");
        for (iteration, omitted_source) in [ptr::null(), &empty_source as *const InkpodIoPath]
            .into_iter()
            .enumerate()
        {
            assert_eq!(document.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
            assert_eq!(
                inkpod_core_sequence_switch_request(
                    core,
                    0,
                    INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
                    &mut switch,
                ),
                INKPOD_STATUS_OK
            );
            assert_eq!(switch.flags, INKPOD_SEQUENCE_SWITCH_REQUIRED);
            assert_eq!(
                inkpod_core_io_sequence_switch_submit(
                    core,
                    manager,
                    &switch,
                    omitted_source,
                    &recovery,
                    &source_proof,
                    ptr::null(),
                    &mut job,
                ),
                INKPOD_STATUS_OK
            );
            let ready = wait_ready(job);
            assert_eq!(ready.state, INKPOD_IO_READY);
            assert_eq!(ready.flags & INKPOD_IO_RESULT_INSTALLING, 0);
            assert_eq!(ready.result_count, 2);
            let mut raster_item = InkpodIoItemInfo {
                struct_size: size_of::<InkpodIoItemInfo>() as u32,
                ..Default::default()
            };
            let mut native_item = raster_item;
            assert_eq!(
                inkpod_io_job_get_item(
                    job,
                    0,
                    &mut raster_item,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    0,
                ),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_io_job_get_item(
                    job,
                    1,
                    &mut native_item,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    0,
                ),
                INKPOD_STATUS_OK
            );
            assert_eq!(raster_item.raster_format, INKPOD_COMMON_RASTER_PNG);
            assert_eq!(raster_item.identity.kind, 1);
            assert_eq!(native_item.raster_format, 0);
            assert_eq!(native_item.identity.kind, 2);
            assert_eq!(
                (
                    raster_item.document_uuid_high,
                    raster_item.document_uuid_low
                ),
                original_uuid
            );
            assert_eq!(
                (
                    native_item.document_uuid_high,
                    native_item.document_uuid_low
                ),
                original_uuid
            );
            assert_eq!(
                inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
                INKPOD_STATUS_OK
            );
            assert_eq!(wait_ready(job).state, INKPOD_IO_COMPLETE);
            assert_eq!(
                (document.document_uuid_high, document.document_uuid_low),
                original_uuid
            );
            assert_eq!(
                document.flags & (INKPOD_DOCUMENT_FLAG_DIRTY | INKPOD_DOCUMENT_FLAG_RECOVERED),
                0
            );
            let mut restored_editor = InkpodEditorStateInfo {
                struct_size: size_of::<InkpodEditorStateInfo>() as u32,
                ..Default::default()
            };
            assert_eq!(
                inkpod_core_get_editor_state(core, &mut restored_editor),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                restored_editor.editor_revision,
                original_editor.editor_revision
            );
            assert_eq!(restored_editor.editor_digest, original_editor.editor_digest);
            assert_eq!(restored_editor.flags & INKPOD_EDITOR_STATE_DIRTY, 0);
            assert_eq!(std::fs::read(&source_recovery).unwrap(), recovery_bytes);
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), file_count);
            assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);

            if iteration == 0 {
                assert_eq!(
                    inkpod_core_sequence_switch_request(
                        core,
                        1,
                        INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
                        &mut switch,
                    ),
                    INKPOD_STATUS_OK
                );
                assert_eq!(switch.flags, INKPOD_SEQUENCE_SWITCH_REQUIRED);
                assert_eq!(
                    inkpod_core_io_sequence_switch_submit(
                        core,
                        manager,
                        &switch,
                        omitted_source,
                        ptr::null(),
                        ptr::null(),
                        ptr::null(),
                        &mut job,
                    ),
                    INKPOD_STATUS_OK
                );
                assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
                assert_eq!(
                    inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
                    INKPOD_STATUS_OK
                );
                assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
            }
        }
        let normal_native = directory.join("cell1.inkpod");
        let normal_text = normal_native.to_str().unwrap();
        let normal_input = path_input(normal_text);
        assert_eq!(
            inkpod_core_io_submit(
                core,
                manager,
                &request(&normal_input, INKPOD_IO_SAVE_PAIR),
                &mut job,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).result_count, 2);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_PENDING
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            document.flags & (INKPOD_DOCUMENT_FLAG_DIRTY | INKPOD_DOCUMENT_FLAG_RECOVERED),
            0
        );
        assert!(normal_native.exists());
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn io_003_ffi_manager_paths_poll_apply_and_ownership_are_connected() {
    let directory = std::env::temp_dir().join(format!(
        "inkpod-ffi-io-{}-{}",
        std::process::id(),
        PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("a1.png");
    let raster = CommonRaster::new(
        1,
        1,
        PixelFormat::StraightRgba8,
        Some(144_000),
        Some(144_000),
        vec![10, 20, 30, 255],
    )
    .unwrap();
    std::fs::write(
        &path,
        encode_common_raster(CommonRasterFormat::Png, &raster, false).unwrap(),
    )
    .unwrap();
    let text = path.to_str().unwrap();
    let path = InkpodIoPath {
        struct_size: size_of::<InkpodIoPath>() as u32,
        reserved: 0,
        path: text.as_ptr(),
        path_bytes: text.len() as u64,
    };
    let config = InkpodCoreConfig {
        struct_size: size_of::<InkpodCoreConfig>() as u32,
        abi_version: INKPOD_ABI_VERSION,
        feature_flags: 0,
    };
    let (mut manager, mut core, mut job, mut subpalette) = (
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    );
    // SAFETY: Every pointer/span below belongs to a live test allocation. Each
    // owner variable is released exactly once; repeating cleared owners is tested.
    unsafe {
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_bind_io_manager(core, manager), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_set_new_cell_raster_format(core, INKPOD_COMMON_RASTER_TIFF),
            INKPOD_STATUS_OK
        );
        let mut identity = InkpodIoFileIdentity {
            struct_size: size_of::<InkpodIoFileIdentity>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_io_resolve_identity(manager, path.path, path.path_bytes, &mut identity),
            INKPOD_STATUS_OK
        );
        assert_eq!(identity.kind, 1);
        assert_eq!(
            inkpod_core_io_submit(
                core,
                manager,
                &request(&path, INKPOD_IO_OPEN_RASTER),
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).loaded_count, 1);
        let mut item = InkpodIoItemInfo {
            struct_size: size_of::<InkpodIoItemInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_io_job_get_item(job, 0, &mut item, ptr::null_mut(), 0, ptr::null_mut(), 0),
            INKPOD_STATUS_OK
        );
        let mut path_out = vec![0; item.path_bytes as usize];
        let mut name_out = vec![0; item.name_bytes as usize];
        assert_eq!(
            inkpod_io_job_get_item(
                job,
                0,
                &mut item,
                path_out.as_mut_ptr(),
                path_out.len() as u64,
                name_out.as_mut_ptr(),
                name_out.len() as u64
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(name_out, b"a1.png");
        let mut info = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..Default::default()
        };
        let mut object = 0;
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut info, &mut object),
            INKPOD_STATUS_OK
        );
        let mut format = 0;
        assert_eq!(
            inkpod_core_get_raster_file_format(core, &mut format),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            format, INKPOD_COMMON_RASTER_PNG,
            "file format overrides future blank default"
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert_eq!(inkpod_subpalette_create(&mut subpalette), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_io_submit(
                ptr::null_mut(),
                manager,
                &request(&path, INKPOD_IO_REFERENCE_FILES),
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).loaded_count, 1);
        let mut reference = InkpodSubpaletteInfo {
            struct_size: size_of::<InkpodSubpaletteInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_subpalette_io_job_apply(subpalette, job, &mut reference),
            INKPOD_STATUS_OK
        );
        assert_eq!(reference.item_count, 1);
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        let mut cache = InkpodIoCacheInfo {
            struct_size: size_of::<InkpodIoCacheInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_io_manager_get_cache_info(manager, &mut cache),
            INKPOD_STATUS_OK
        );
        assert_eq!(cache.image_count, 1);
        assert_eq!(cache.decodes, 1);
        assert!(cache.cache_hits >= 1);
        assert_eq!(
            inkpod_core_io_submit(
                ptr::null_mut(),
                manager,
                &request(&path, INKPOD_IO_REFERENCE_FILES),
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_io_job_cancel(job), INKPOD_STATUS_OK);
        assert_eq!(wait_ready(job).state, INKPOD_IO_CANCELLED);
        let mut needed = 0;
        assert_eq!(
            inkpod_io_job_copy_error(job, ptr::null_mut(), 0, &mut needed),
            INKPOD_STATUS_OK
        );
        assert!(needed > 0);
        let mut error = vec![0; needed as usize];
        assert_eq!(
            inkpod_io_job_copy_error(job, error.as_mut_ptr(), needed, &mut needed),
            INKPOD_STATUS_OK
        );
        assert!(std::str::from_utf8(&error).unwrap().contains("cancel"));
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert_eq!(inkpod_subpalette_release(&mut subpalette), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn io_003_validated_target_cache_limit_is_bounded_and_observable() {
    // SAFETY: Manager/output pointers are live and all records advertise their
    // complete current layout for the duration of each call.
    unsafe {
        let mut manager = ptr::null_mut();
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        let mut cache = InkpodIoCacheInfo {
            struct_size: size_of::<InkpodIoCacheInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            inkpod_io_manager_get_cache_info(manager, &mut cache),
            INKPOD_STATUS_OK
        );
        assert_eq!(cache.validated_target_maximum_bytes, 1024 * 1024 * 1024);
        assert_eq!(cache.validated_target_count, 0);

        assert_eq!(
            inkpod_io_manager_set_validated_target_cache_bytes(manager, 0),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_io_manager_get_cache_info(manager, &mut cache),
            INKPOD_STATUS_OK
        );
        assert_eq!(cache.validated_target_maximum_bytes, 0);

        assert_eq!(
            inkpod_io_manager_set_validated_target_cache_bytes(manager, 1024 * 1024 * 1024),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_io_manager_set_validated_target_cache_bytes(manager, 1024 * 1024 * 1024 + 1),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_io_manager_get_cache_info(manager, &mut cache),
            INKPOD_STATUS_OK
        );
        assert_eq!(cache.validated_target_maximum_bytes, 1024 * 1024 * 1024);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
}

#[test]
fn io_003_ffi_rejects_null_short_unknown_enum_stride_and_occupied_owner() {
    // SAFETY: Invalid pointers are null or aligned in-bounds test records; no
    // dangling/unmapped pointers are passed, and validation precedes dereference.
    unsafe {
        let mut manager = ptr::null_mut();
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_INVALID_STATE
        );
        let bytes = b"x.png";
        let path = InkpodIoPath {
            struct_size: size_of::<InkpodIoPath>() as u32,
            reserved: 0,
            path: bytes.as_ptr(),
            path_bytes: bytes.len() as u64,
        };
        let mut input = request(&path, INKPOD_IO_REFERENCE_FILES);
        let mut job = ptr::null_mut();
        input.struct_size = 4;
        assert_eq!(
            inkpod_core_io_submit(ptr::null_mut(), manager, &input, &mut job),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        input.struct_size = size_of::<InkpodIoRequest>() as u32;
        input.kind = u32::MAX;
        assert_eq!(
            inkpod_core_io_submit(ptr::null_mut(), manager, &input, &mut job),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        input.kind = INKPOD_IO_REFERENCE_FILES;
        input.path_stride_bytes = 1;
        assert_eq!(
            inkpod_core_io_submit(ptr::null_mut(), manager, &input, &mut job),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        input.path_stride_bytes = size_of::<InkpodIoPath>() as u64;
        input.path_count = u64::MAX;
        assert_eq!(
            inkpod_core_io_submit(ptr::null_mut(), manager, &input, &mut job),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(job.is_null());
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
}

#[test]
fn io_003_authority_repair_progress_flag_is_independent_and_additive() {
    let progress = inkpod_core::FileIoProgress {
        job_id: 1,
        kind: inkpod_core::FileIoKind::SavePair,
        state: inkpod_core::FileIoState::Ready,
        discovered_count: 0,
        total_count: 0,
        read_count: 0,
        loaded_count: 0,
        failed_count: 0,
        cancelled_count: 0,
        completed_work: 0,
        total_work: 0,
        result_count: 2,
        truncated: false,
        installing: true,
        cut_descriptor: false,
        authority_repaired: true,
        authority_revoked: false,
    };
    assert_eq!(
        super::query::progress_flags(&progress),
        INKPOD_IO_RESULT_INSTALLING | INKPOD_IO_RESULT_AUTHORITY_REPAIRED
    );
    let revoked = inkpod_core::FileIoProgress {
        installing: false,
        authority_repaired: false,
        authority_revoked: true,
        state: inkpod_core::FileIoState::Failed,
        ..progress
    };
    assert_eq!(
        super::query::progress_flags(&revoked),
        INKPOD_IO_RESULT_AUTHORITY_REVOKED
    );
}
