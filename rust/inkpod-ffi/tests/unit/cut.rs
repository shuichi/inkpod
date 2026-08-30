use super::*;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};

static CUT_TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn test_directory() -> PathBuf {
    let sequence = CUT_TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("inkpod-cut-ffi-{}-{sequence}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

fn span(value: &str) -> InkpodUtf8Span {
    InkpodUtf8Span {
        bytes: value.as_ptr(),
        byte_count: value.len() as u64,
    }
}

fn metadata(cut_name: &str) -> InkpodCutMetadataInput {
    InkpodCutMetadataInput {
        struct_size: size_of::<InkpodCutMetadataInput>() as u32,
        duration_frames: 24,
        work_title: span("Work"),
        episode: span("01"),
        scene: span("A"),
        cut_name: span(cut_name),
        instruction: span("Paint"),
    }
}

fn defaults() -> InkpodCutDefaultsInput {
    InkpodCutDefaultsInput {
        struct_size: size_of::<InkpodCutDefaultsInput>() as u32,
        sizing_mode: INKPOD_CELL_SIZING_IMAGE_PIXELS,
        feature_flags: INKPOD_FEATURE_NONE,
        width: 16,
        height: 12,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
        margin_milli: 50,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: INKPOD_FRAME_ANCHOR_CENTER,
        pixel_format: INKPOD_STORAGE_RGBA8,
        reserved: 0,
    }
}

fn cut_info() -> InkpodCutInfo {
    InkpodCutInfo {
        struct_size: size_of::<InkpodCutInfo>() as u32,
        ..InkpodCutInfo::default()
    }
}

fn dispatch() -> InkpodDispatchResult {
    InkpodDispatchResult {
        struct_size: size_of::<InkpodDispatchResult>() as u32,
        reserved: 0,
        revision: 0,
        accepted_command_count: 0,
    }
}

unsafe fn create_saved_cell(path: &Path, uuid: u128) -> InkpodDocumentInfo {
    let config = InkpodCoreConfig {
        struct_size: size_of::<InkpodCoreConfig>() as u32,
        abi_version: INKPOD_ABI_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
    };
    let mut core = ptr::null_mut();
    // SAFETY: Test records and owner storage satisfy the exported contracts.
    assert_eq!(
        unsafe { inkpod_core_create(&config, &mut core) },
        INKPOD_STATUS_OK
    );
    let options = InkpodCellCreateOptions {
        struct_size: size_of::<InkpodCellCreateOptions>() as u32,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
        document_uuid_high: (uuid >> 64) as u64,
        document_uuid_low: uuid as u64,
        width: 16,
        height: 12,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
    };
    let mut info = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..InkpodDocumentInfo::default()
    };
    // SAFETY: Live owner-thread Core and complete records are supplied.
    assert_eq!(
        unsafe { inkpod_core_new_cell(core, &options, &mut info) },
        INKPOD_STATUS_OK
    );
    let bytes = path_bytes(path);
    // SAFETY: The path span remains readable and output is complete.
    assert_eq!(
        unsafe { inkpod_core_save(core, bytes.as_ptr(), bytes.len() as u64, &mut info) },
        INKPOD_STATUS_OK
    );
    let mut thumbnail = InkpodDocumentThumbnailBuffer {
        struct_size: size_of::<InkpodDocumentThumbnailBuffer>() as u32,
        ..InkpodDocumentThumbnailBuffer::default()
    };
    assert_eq!(
        unsafe { inkpod_core_document_thumbnail_get(core, &mut thumbnail) },
        INKPOD_STATUS_OK
    );
    assert_eq!(thumbnail.stride_bytes, thumbnail.width * 4);
    assert_eq!(
        thumbnail.required_bytes,
        u64::from(thumbnail.stride_bytes) * u64::from(thumbnail.height)
    );
    assert_ne!(thumbnail.checksum, 0);
    let mut short = vec![0_u8; thumbnail.required_bytes as usize - 1];
    thumbnail.pixels_rgba8 = short.as_mut_ptr();
    thumbnail.pixel_capacity = short.len() as u64;
    assert_eq!(
        unsafe { inkpod_core_document_thumbnail_get(core, &mut thumbnail) },
        INKPOD_STATUS_BUFFER_TOO_SMALL
    );
    let mut pixels = vec![0_u8; thumbnail.required_bytes as usize];
    thumbnail.pixels_rgba8 = pixels.as_mut_ptr();
    thumbnail.pixel_capacity = pixels.len() as u64;
    assert_eq!(
        unsafe { inkpod_core_document_thumbnail_get(core, &mut thumbnail) },
        INKPOD_STATUS_OK
    );
    thumbnail.reserved = 1;
    assert_eq!(
        unsafe { inkpod_core_document_thumbnail_get(core, &mut thumbnail) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    // SAFETY: The owner storage contains the unique live Core handle.
    assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    info
}

#[test]
fn cut_abi_covers_contract_history_persistence_ownership_and_negative_cases() {
    let directory = test_directory();
    let cell_path = directory.join("cell-0001.inkpod");
    let second_cell_path = directory.join("cell-0002.inkpod");
    let third_cell_path = directory.join("cell-0003.inkpod");
    let descriptor_path = directory.join("cut.inkpod");
    let recovery_path = directory.join("cut-recovery.inkpod");
    let cell_uuid = 0x2222_3333_4444_5555_6666_7777_8888_9999_u128;
    // SAFETY: This test owns every handle and keeps all advertised spans live.
    unsafe {
        let cell = create_saved_cell(&cell_path, cell_uuid);
        let second_cell = create_saved_cell(&second_cell_path, cell_uuid + 1);
        let third_cell = create_saved_cell(&third_cell_path, cell_uuid + 2);
        assert_ne!(cell.cell_id, 0);
        let member_name = "cell-0001.inkpod";
        let member = InkpodCutMemberInput {
            struct_size: size_of::<InkpodCutMemberInput>() as u32,
            display_number: 1,
            cell_id: cell.cell_id,
            document_uuid_high: cell.document_uuid_high,
            document_uuid_low: cell.document_uuid_low,
            relative_path: span(member_name),
        };
        let initial_metadata = metadata("C001");
        let initial_defaults = defaults();
        let request = InkpodCutCreateRequest {
            struct_size: size_of::<InkpodCutCreateRequest>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            cut_uuid_high: 0xaaaa_bbbb_cccc_dddd,
            cut_uuid_low: 0x1111_2222_3333_4444,
            metadata: &initial_metadata,
            defaults: &initial_defaults,
            members: &member,
            member_count: 1,
            member_stride_bytes: size_of::<InkpodCutMemberInput>() as u64,
        };
        let mut cut = ptr::null_mut();
        assert_eq!(inkpod_cut_create(&request, &mut cut), INKPOD_STATUS_OK);

        let mut info = cut_info();
        assert_eq!(inkpod_cut_info(cut, &mut info), INKPOD_STATUS_OK);
        assert_eq!(info.member_count, 1);
        assert_ne!(info.flags & INKPOD_CUT_FLAG_DIRTY, 0);
        assert_eq!(info.cut_name_bytes, 4);

        let mut metadata_output = InkpodCutMetadataBuffer {
            struct_size: size_of::<InkpodCutMetadataBuffer>() as u32,
            ..InkpodCutMetadataBuffer::default()
        };
        assert_eq!(
            inkpod_cut_metadata_copy(cut, &mut metadata_output),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut work = vec![0_u8; metadata_output.work_title.byte_count as usize];
        let mut episode = vec![0_u8; metadata_output.episode.byte_count as usize];
        let mut scene = vec![0_u8; metadata_output.scene.byte_count as usize];
        let mut name = vec![0_u8; metadata_output.cut_name.byte_count as usize];
        let mut instruction = vec![0_u8; metadata_output.instruction.byte_count as usize];
        metadata_output.work_title = InkpodUtf8Buffer {
            bytes: work.as_mut_ptr(),
            capacity: work.len() as u64,
            byte_count: 0,
        };
        metadata_output.episode = InkpodUtf8Buffer {
            bytes: episode.as_mut_ptr(),
            capacity: episode.len() as u64,
            byte_count: 0,
        };
        metadata_output.scene = InkpodUtf8Buffer {
            bytes: scene.as_mut_ptr(),
            capacity: scene.len() as u64,
            byte_count: 0,
        };
        metadata_output.cut_name = InkpodUtf8Buffer {
            bytes: name.as_mut_ptr(),
            capacity: name.len() as u64,
            byte_count: 0,
        };
        metadata_output.instruction = InkpodUtf8Buffer {
            bytes: instruction.as_mut_ptr(),
            capacity: instruction.len() as u64,
            byte_count: 0,
        };
        assert_eq!(
            inkpod_cut_metadata_copy(cut, &mut metadata_output),
            INKPOD_STATUS_OK
        );
        assert_eq!(name, b"C001");

        let mut member_output = InkpodCutMemberInfo {
            struct_size: size_of::<InkpodCutMemberInfo>() as u32,
            ..InkpodCutMemberInfo::default()
        };
        assert_eq!(
            inkpod_cut_member_get(cut, 0, &mut member_output),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut member_path = vec![0_u8; member_output.relative_path.byte_count as usize];
        member_output.relative_path = InkpodUtf8Buffer {
            bytes: member_path.as_mut_ptr(),
            capacity: member_path.len() as u64,
            byte_count: 0,
        };
        assert_eq!(
            inkpod_cut_member_get(cut, 0, &mut member_output),
            INKPOD_STATUS_OK
        );
        assert_eq!(member_output.cell_id, cell.cell_id);
        assert_eq!(member_path, member_name.as_bytes());
        assert_eq!(
            inkpod_cut_member_get(cut, 1, &mut member_output),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let sequence_operations = [
            InkpodCutSequenceEditOperation {
                struct_size: size_of::<InkpodCutSequenceEditOperation>() as u32,
                kind: INKPOD_CUT_SEQUENCE_INSERT,
                cell_id: second_cell.cell_id,
                document_uuid_high: second_cell.document_uuid_high,
                document_uuid_low: second_cell.document_uuid_low,
                position: 1,
                display_number: 2,
                relative_path: span("cell-0002.inkpod"),
                ..InkpodCutSequenceEditOperation::default()
            },
            InkpodCutSequenceEditOperation {
                struct_size: size_of::<InkpodCutSequenceEditOperation>() as u32,
                kind: INKPOD_CUT_SEQUENCE_INSERT,
                cell_id: third_cell.cell_id,
                document_uuid_high: third_cell.document_uuid_high,
                document_uuid_low: third_cell.document_uuid_low,
                position: 2,
                display_number: 3,
                relative_path: span("cell-0003.inkpod"),
                ..InkpodCutSequenceEditOperation::default()
            },
            InkpodCutSequenceEditOperation {
                struct_size: size_of::<InkpodCutSequenceEditOperation>() as u32,
                kind: INKPOD_CUT_SEQUENCE_MOVE_BEFORE,
                cell_id: third_cell.cell_id,
                document_uuid_high: third_cell.document_uuid_high,
                document_uuid_low: third_cell.document_uuid_low,
                anchor_cell_id: cell.cell_id,
                anchor_document_uuid_high: cell.document_uuid_high,
                anchor_document_uuid_low: cell.document_uuid_low,
                ..InkpodCutSequenceEditOperation::default()
            },
            InkpodCutSequenceEditOperation {
                struct_size: size_of::<InkpodCutSequenceEditOperation>() as u32,
                kind: INKPOD_CUT_SEQUENCE_RENUMBER_RANGE,
                position: 0,
                count: 3,
                first_number: 10,
                step: 10,
                ..InkpodCutSequenceEditOperation::default()
            },
            InkpodCutSequenceEditOperation {
                struct_size: size_of::<InkpodCutSequenceEditOperation>() as u32,
                kind: INKPOD_CUT_SEQUENCE_REMOVE,
                cell_id: second_cell.cell_id,
                document_uuid_high: second_cell.document_uuid_high,
                document_uuid_low: second_cell.document_uuid_low,
                ..InkpodCutSequenceEditOperation::default()
            },
        ];
        let sequence_request = InkpodCutSequenceEditRequest {
            struct_size: size_of::<InkpodCutSequenceEditRequest>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            base_revision: info.revision,
            operations: sequence_operations.as_ptr(),
            operation_count: sequence_operations.len() as u64,
            operation_stride_bytes: size_of::<InkpodCutSequenceEditOperation>() as u64,
        };
        let mut sequence_result = InkpodCutSequenceEditResult {
            struct_size: size_of::<InkpodCutSequenceEditResult>() as u32,
            ..InkpodCutSequenceEditResult::default()
        };
        assert_eq!(
            inkpod_cut_sequence_edit(cut, &sequence_request, &mut sequence_result),
            INKPOD_STATUS_OK
        );
        assert_ne!(sequence_result.flags & INKPOD_CUT_SEQUENCE_EDIT_APPLIED, 0);
        assert_eq!(sequence_result.revision, info.revision + 1);
        assert_eq!(sequence_result.state_id, info.state_id + 1);
        assert_eq!(sequence_result.member_count, 2);
        assert_eq!(sequence_result.operation_count, 5);
        assert_eq!(
            sequence_result.failed_operation_index,
            INKPOD_CUT_SEQUENCE_REQUEST_ERROR_INDEX
        );
        assert!(second_cell_path.exists());

        let invalid_operations = [
            InkpodCutSequenceEditOperation {
                struct_size: size_of::<InkpodCutSequenceEditOperation>() as u32,
                kind: INKPOD_CUT_SEQUENCE_MOVE_AFTER,
                cell_id: third_cell.cell_id,
                document_uuid_high: third_cell.document_uuid_high,
                document_uuid_low: third_cell.document_uuid_low,
                anchor_cell_id: cell.cell_id,
                anchor_document_uuid_high: cell.document_uuid_high,
                anchor_document_uuid_low: cell.document_uuid_low,
                ..InkpodCutSequenceEditOperation::default()
            },
            InkpodCutSequenceEditOperation {
                struct_size: size_of::<InkpodCutSequenceEditOperation>() as u32,
                kind: INKPOD_CUT_SEQUENCE_RENUMBER_RANGE,
                position: 0,
                count: 2,
                first_number: 1,
                step: 0,
                ..InkpodCutSequenceEditOperation::default()
            },
        ];
        let invalid_request = InkpodCutSequenceEditRequest {
            base_revision: sequence_result.revision,
            operations: invalid_operations.as_ptr(),
            operation_count: invalid_operations.len() as u64,
            ..sequence_request
        };
        let stable_revision = sequence_result.revision;
        let stable_state_id = sequence_result.state_id;
        assert_eq!(
            inkpod_cut_sequence_edit(cut, &invalid_request, &mut sequence_result),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(sequence_result.failed_operation_index, 1);
        assert_eq!(sequence_result.revision, stable_revision);
        assert_eq!(sequence_result.state_id, stable_state_id);

        let short_stride_request = InkpodCutSequenceEditRequest {
            operation_stride_bytes: size_of::<InkpodCutSequenceEditOperation>() as u64 - 1,
            ..invalid_request
        };
        assert_eq!(
            inkpod_cut_sequence_edit(cut, &short_stride_request, &mut sequence_result),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            sequence_result.failed_operation_index,
            INKPOD_CUT_SEQUENCE_REQUEST_ERROR_INDEX
        );
        assert_eq!(sequence_result.revision, stable_revision);
        assert_eq!(sequence_result.state_id, stable_state_id);

        let short_operation = InkpodCutSequenceEditOperation {
            struct_size: size_of::<InkpodCutSequenceEditOperation>() as u32 - 1,
            ..invalid_operations[0]
        };
        let short_operation_request = InkpodCutSequenceEditRequest {
            operations: &short_operation,
            operation_count: 1,
            operation_stride_bytes: size_of::<InkpodCutSequenceEditOperation>() as u64,
            ..invalid_request
        };
        assert_eq!(
            inkpod_cut_sequence_edit(cut, &short_operation_request, &mut sequence_result),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(sequence_result.failed_operation_index, 0);
        assert_eq!(sequence_result.revision, stable_revision);
        assert_eq!(sequence_result.state_id, stable_state_id);

        let stale_request = InkpodCutSequenceEditRequest {
            base_revision: stable_revision - 1,
            operations: ptr::null(),
            operation_count: 0,
            operation_stride_bytes: 0,
            ..invalid_request
        };
        assert_eq!(
            inkpod_cut_sequence_edit(cut, &stale_request, &mut sequence_result),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            sequence_result.failed_operation_index,
            INKPOD_CUT_SEQUENCE_REQUEST_ERROR_INDEX
        );
        assert_eq!(sequence_result.revision, stable_revision);
        assert_eq!(sequence_result.state_id, stable_state_id);

        assert_eq!(
            inkpod_cut_sequence_cancel(cut, &mut sequence_result),
            INKPOD_STATUS_OK
        );
        assert_eq!(sequence_result.flags, 0);

        let changed_metadata = metadata("C002");
        let update = InkpodCutUpdateRequest {
            struct_size: size_of::<InkpodCutUpdateRequest>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            base_revision: sequence_result.revision,
            metadata: &changed_metadata,
            defaults: &initial_defaults,
        };
        let mut result = dispatch();
        assert_eq!(
            inkpod_cut_update(cut, &update, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(result.accepted_command_count, 1);
        let changed_revision = result.revision;
        assert_eq!(
            inkpod_cut_update(cut, &update, &mut result),
            INKPOD_STATUS_INVALID_STATE
        );
        let no_op_update = InkpodCutUpdateRequest {
            base_revision: changed_revision,
            ..update
        };
        assert_eq!(
            inkpod_cut_update(cut, &no_op_update, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(result.accepted_command_count, 0);
        assert_eq!(inkpod_cut_cancel_update(cut, &mut result), INKPOD_STATUS_OK);
        assert_eq!(result.accepted_command_count, 0);
        assert_eq!(inkpod_cut_undo(cut, &mut result), INKPOD_STATUS_OK);
        assert_eq!(result.accepted_command_count, 1);
        assert_eq!(inkpod_cut_redo(cut, &mut result), INKPOD_STATUS_OK);
        assert_eq!(result.accepted_command_count, 1);

        let descriptor = path_bytes(&descriptor_path);
        assert_eq!(
            inkpod_cut_save(cut, descriptor.as_ptr(), descriptor.len() as u64, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.flags & INKPOD_CUT_FLAG_DIRTY, 0);
        let recovery = path_bytes(&recovery_path);
        assert_eq!(
            inkpod_cut_autosave(cut, recovery.as_ptr(), recovery.len() as u64, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.flags & INKPOD_CUT_FLAG_DIRTY, 0);

        let raw = cut as usize;
        let wrong_thread = std::thread::spawn(move || {
            let mut other_info = cut_info();
            inkpod_cut_info(raw as *const InkpodCut, &mut other_info)
        })
        .join()
        .unwrap();
        assert_eq!(wrong_thread, INKPOD_STATUS_WRONG_THREAD);

        assert_eq!(inkpod_cut_destroy(&mut cut), INKPOD_STATUS_OK);
        assert_eq!(inkpod_cut_destroy(&mut cut), INKPOD_STATUS_OK);
        let mut reopened = ptr::null_mut();
        assert_eq!(
            inkpod_cut_open(descriptor.as_ptr(), descriptor.len() as u64, &mut reopened),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_cut_info(reopened, &mut info), INKPOD_STATUS_OK);
        assert_eq!(info.member_count, 2);
        assert_eq!(inkpod_cut_destroy(&mut reopened), INKPOD_STATUS_OK);
        let mut recovered = ptr::null_mut();
        assert_eq!(
            inkpod_cut_open_recovery(recovery.as_ptr(), recovery.len() as u64, &mut recovered),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_cut_info(recovered, &mut info), INKPOD_STATUS_OK);
        assert_ne!(info.flags & INKPOD_CUT_FLAG_DIRTY, 0);
        assert_ne!(info.flags & INKPOD_CUT_FLAG_RECOVERED, 0);
        assert_eq!(inkpod_cut_destroy(&mut recovered), INKPOD_STATUS_OK);

        let duplicate_members = [member, member];
        let duplicate_request = InkpodCutCreateRequest {
            members: duplicate_members.as_ptr(),
            member_count: 2,
            ..request
        };
        let mut rejected = ptr::dangling_mut::<InkpodCut>();
        assert_eq!(
            inkpod_cut_create(&duplicate_request, &mut rejected),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(rejected.is_null());
        let mut short_request = request;
        short_request.struct_size -= 1;
        assert_eq!(
            inkpod_cut_create(&short_request, &mut rejected),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert!(rejected.is_null());
        assert_eq!(
            inkpod_cut_create(ptr::null(), &mut rejected),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(rejected.is_null());
    }
    std::fs::remove_dir_all(directory).unwrap();
}
