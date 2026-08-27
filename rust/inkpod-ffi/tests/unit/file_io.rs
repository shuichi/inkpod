use super::*;
use inkpod_format::{CommonRaster, encode_common_raster};
use std::time::{Duration, Instant};

static PATH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_OK
        );
        assert_eq!((document.document_revision, document.flags), before);
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert!(recovery.exists());
        assert!(!recovery.with_extension("png").exists());
        assert!(!original.exists());

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
        assert_eq!(
            inkpod_core_io_submit(
                ptr::null_mut(),
                manager,
                &request(&paths[1], INKPOD_IO_RECOVERY_DISCARD),
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_COMPLETE);
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        assert!(!recovery.exists());
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn io_003_sequence_switch_and_compacted_copy_require_owner_finalization() {
    let directory = temporary_directory("session");
    let first = directory.join("cell1.png");
    let second = directory.join("cell2.bmp");
    for (path, format, pixel) in [
        (&first, CommonRasterFormat::Png, 10),
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
    let first = path_input(first.to_str().unwrap());
    let source_recovery = directory.join("source.recovery.inkpod");
    let recovery = path_input(source_recovery.to_str().unwrap());
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
        for kind in [INKPOD_IO_OPEN_RASTER, INKPOD_IO_SEQUENCE_AUTO] {
            assert_eq!(
                inkpod_core_io_submit(core, manager, &request(&first, kind), &mut job),
                INKPOD_STATUS_OK
            );
            assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
            assert_eq!(
                inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        }
        let original_uuid = (document.document_uuid_high, document.document_uuid_low);
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
        assert_eq!(
            inkpod_core_io_sequence_switch_submit(
                core,
                manager,
                ptr::null(),
                &recovery,
                ptr::null(),
                ptr::null(),
                &mut job
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_io_sequence_switch_submit(
                core,
                manager,
                &switch,
                &recovery,
                ptr::null(),
                ptr::null(),
                &mut job
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
        assert_eq!(
            inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
            INKPOD_STATUS_PENDING
        );
        assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_INVALID_STATE);
        assert_eq!(wait_ready(job).state, INKPOD_IO_READY);
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
