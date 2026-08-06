use super::*;

fn object_id() -> InkpodObjectId {
    InkpodObjectId {
        struct_size: size_of::<InkpodObjectId>() as u32,
        ..InkpodObjectId::default()
    }
}

fn primitive_result() -> InkpodPrimitiveResultV3 {
    InkpodPrimitiveResultV3 {
        struct_size: size_of::<InkpodPrimitiveResultV3>() as u32,
        ..InkpodPrimitiveResultV3::default()
    }
}

fn rgba8(red: u16, green: u16, blue: u16, alpha: u16) -> InkpodColorValue {
    InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        depth: INKPOD_COLOR_DEPTH_8,
        red,
        green,
        blue,
        alpha,
    }
}

fn primitive_request(opcode: u32, schema_version: u32, revision: u64) -> InkpodPrimitiveRequestV3 {
    InkpodPrimitiveRequestV3 {
        struct_size: size_of::<InkpodPrimitiveRequestV3>() as u32,
        opcode,
        schema_version,
        base_revision: revision,
        payload_id: object_id(),
        ..InkpodPrimitiveRequestV3::default()
    }
}

fn create_v3_core(uuid_low: u64) -> (*mut InkpodCore, InkpodDocumentInfo) {
    let mut core = ptr::null_mut();
    let config = InkpodCoreConfig {
        struct_size: size_of::<InkpodCoreConfig>() as u32,
        abi_version: INKPOD_ABI_VERSION,
        feature_flags: 0,
    };
    let options = InkpodCellCreateOptions {
        struct_size: size_of::<InkpodCellCreateOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
        document_uuid_high: 0x5633_434f_4e54_5241,
        document_uuid_low: uuid_low,
        width: 4,
        height: 4,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
    };
    let mut info = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..InkpodDocumentInfo::default()
    };
    // SAFETY: Complete non-overlapping owner records remain live for both calls.
    unsafe {
        assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut info),
            INKPOD_STATUS_OK
        );
    }
    (core, info)
}

#[test]
fn v3_value_control_plane_owns_payloads_and_rejects_stale_wrong_ids_atomically() {
    let (mut core, initial) = create_v3_core(1);
    let mut core_id = object_id();
    let mut source = [rgba8(10, 20, 30, 255)];
    let colors = InkpodColorArray {
        struct_size: size_of::<InkpodColorArray>() as u32,
        reserved: 0,
        feature_flags: 0,
        colors: source.as_ptr(),
        color_count: 1,
        color_stride_bytes: size_of::<InkpodColorValue>() as u64,
    };
    let mut colors_id = object_id();

    // SAFETY: All input spans and output records are complete, aligned, and live for each call.
    unsafe {
        assert_eq!(inkpod_core_get_id_v3(core, &mut core_id), INKPOD_STATUS_OK);
        assert_eq!(core_id.object_type, INKPOD_OBJECT_CORE);
        assert_ne!(core_id.generation, 0);
        assert_eq!(
            inkpod_core_register_color_array_v3(core, &colors, &mut colors_id),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(colors_id.object_type, INKPOD_OBJECT_COLOR_ARRAY);
    assert_eq!(colors_id.generation, core_id.generation);

    source[0] = rgba8(200, 210, 220, 255);
    assert_eq!(source[0].red, 200);
    let mut request = primitive_request(
        INKPOD_PRIMITIVE_REPLACE_PALETTE,
        1,
        initial.document_revision,
    );
    request.payload_id = colors_id;
    let mut result = primitive_result();
    // SAFETY: The request is pointer-free and references a live same-generation object.
    unsafe {
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &request, &mut result),
            INKPOD_STATUS_OK
        );
    }
    assert_ne!(result.flags & INKPOD_PRIMITIVE_RESULT_COMMITTED, 0);
    assert_eq!(result.opcode, INKPOD_PRIMITIVE_REPLACE_PALETTE);
    assert_eq!(result.schema_version, 1);
    assert_eq!(result.revision, initial.document_revision + 1);

    let mut copied = [rgba8(0, 0, 0, 0)];
    let mut buffer = InkpodColorBuffer {
        struct_size: size_of::<InkpodColorBuffer>() as u32,
        reserved: 0,
        feature_flags: 0,
        colors: copied.as_mut_ptr(),
        color_capacity: 1,
        color_stride_bytes: size_of::<InkpodColorValue>() as u64,
        color_count: 0,
    };
    // SAFETY: The one-record caller buffer is complete and writable.
    unsafe {
        assert_eq!(inkpod_core_palette_get(core, &mut buffer), INKPOD_STATUS_OK);
    }
    assert_eq!(
        (copied[0].red, copied[0].green, copied[0].blue),
        (10, 20, 30),
        "registration must sever the caller-buffer lifetime"
    );

    request.base_revision = result.revision;
    let before_noop = result.revision;
    // SAFETY: Same live payload and current revision; the semantic value is unchanged.
    unsafe {
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &request, &mut result),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(result.flags & INKPOD_PRIMITIVE_RESULT_COMMITTED, 0);
    assert_eq!(result.revision, before_noop);

    request.base_revision = before_noop - 1;
    // SAFETY: Complete request/result records remain live; stale work must be rejected.
    unsafe {
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &request, &mut result),
            INKPOD_STATUS_INVALID_STATE
        );
    }
    let mut after_failure = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..InkpodDocumentInfo::default()
    };
    // SAFETY: Complete writable query output remains live.
    unsafe {
        assert_eq!(
            inkpod_core_get_document_info(core, &mut after_failure),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(after_failure.document_revision, before_noop);

    let sample = InkpodStrokeSample {
        struct_size: size_of::<InkpodStrokeSample>() as u32,
        flags: 0,
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
        reserved: 0,
    };
    let span = InkpodStrokeSampleSpan {
        struct_size: size_of::<InkpodStrokeSampleSpan>() as u32,
        reserved: 0,
        feature_flags: 0,
        samples: &sample,
        sample_count: 1,
        sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
    };
    let mut samples_id = object_id();
    // SAFETY: Complete one-sample span and empty output ID are live for the call.
    unsafe {
        assert_eq!(
            inkpod_core_register_sample_stream_v3(core, &span, &mut samples_id),
            INKPOD_STATUS_OK
        );
    }
    request.base_revision = before_noop;
    request.payload_id = samples_id;
    // SAFETY: The wrong-type ID is live but must be rejected without consuming it.
    unsafe {
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &request, &mut result),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
    }

    let (mut other_core, other_info) = create_v3_core(2);
    request.base_revision = other_info.document_revision;
    request.payload_id = colors_id;
    // SAFETY: Both Cores are live; an ID from another generation must be rejected.
    unsafe {
        assert_eq!(
            inkpod_core_primitive_execute_v3(other_core, &request, &mut result),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            inkpod_core_object_release_v3(core, &colors_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_object_release_v3(core, &colors_id),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            inkpod_core_object_release_v3(core, &samples_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut other_core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn v3_registered_sample_and_raster_assets_feed_stable_primitive_records() {
    let (mut core, initial) = create_v3_core(3);
    let mut samples = [InkpodStrokeSample {
        struct_size: size_of::<InkpodStrokeSample>() as u32,
        flags: 0,
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
        reserved: 0,
    }];
    let span = InkpodStrokeSampleSpan {
        struct_size: size_of::<InkpodStrokeSampleSpan>() as u32,
        reserved: 0,
        feature_flags: 0,
        samples: samples.as_ptr(),
        sample_count: 1,
        sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
    };
    let mut sample_id = object_id();
    // SAFETY: The one-sample span is copied before return.
    unsafe {
        assert_eq!(
            inkpod_core_register_sample_stream_v3(core, &span, &mut sample_id),
            INKPOD_STATUS_OK
        );
    }
    samples[0].x = 3.0;
    samples[0].y = 3.0;
    assert_eq!((samples[0].x, samples[0].y), (3.0, 3.0));

    let mut stroke = primitive_request(
        INKPOD_PRIMITIVE_APPLY_RASTER_STROKE,
        2,
        initial.document_revision,
    );
    stroke.target_id = initial.main_plane_id;
    stroke.payload_id = sample_id;
    stroke.tool = INKPOD_TOOL_PENCIL;
    stroke.plane = INKPOD_PLANE_MAIN_LINE;
    stroke.coordinate_space = INKPOD_COORDINATE_SPACE_DOCUMENT;
    stroke.color = rgba8(0, 0, 0, 255);
    stroke.diameter = 1.0;
    let mut result = primitive_result();
    // SAFETY: Pointer-free request references the Rust-owned copied sample stream.
    unsafe {
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &stroke, &mut result),
            INKPOD_STATUS_OK
        );
    }
    assert_ne!(result.flags & INKPOD_PRIMITIVE_RESULT_COMMITTED, 0);

    let mut pixels = [0_u8; 4 * 4 * 4];
    pixels[..4].copy_from_slice(&[11, 22, 33, 255]);
    let raster = InkpodRasterAssetInputV3 {
        struct_size: size_of::<InkpodRasterAssetInputV3>() as u32,
        pixel_format: INKPOD_STORAGE_RGBA8,
        feature_flags: 0,
        width: 4,
        height: 4,
        reserved: 0,
        reserved_2: 0,
        row_stride_bytes: 16,
        pixels: pixels.as_ptr(),
        pixel_bytes: pixels.len() as u64,
    };
    let mut asset_id = object_id();
    // SAFETY: The exact raster span is copied before return.
    unsafe {
        assert_eq!(
            inkpod_core_register_raster_asset_v3(core, &raster, &mut asset_id),
            INKPOD_STATUS_OK
        );
    }
    pixels.fill(0xff);
    let mut import = primitive_request(INKPOD_PRIMITIVE_IMPORT_RASTER_ASSET, 1, result.revision);
    import.target_id = initial.color_plane_id;
    import.payload_id = asset_id;
    // SAFETY: Pointer-free request references the copied immutable raster object.
    unsafe {
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &import, &mut result),
            INKPOD_STATUS_OK
        );
    }
    assert_ne!(result.flags & INKPOD_PRIMITIVE_RESULT_COMMITTED, 0);
    let mut info = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..InkpodDocumentInfo::default()
    };
    // SAFETY: Complete query output and live object IDs are used on the owner thread.
    unsafe {
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_OK
        );
        assert_ne!(info.color_plane_checksum, 0);
        assert_eq!(
            inkpod_core_object_release_v3(core, &sample_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_object_release_v3(core, &asset_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn v3_snapshot_thumbnail_export_and_task_ids_use_bounded_copy_and_exact_release() {
    let (mut core, document) = create_v3_core(4);
    let sample = InkpodStrokeSample {
        struct_size: size_of::<InkpodStrokeSample>() as u32,
        flags: 0,
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
        reserved: 0,
    };
    let stroke = InkpodStrokeInput {
        struct_size: size_of::<InkpodStrokeInput>() as u32,
        tool: INKPOD_TOOL_PENCIL,
        plane: INKPOD_PLANE_MAIN_LINE,
        coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
        flags: 0,
        color_rgba: 0x0000_00ff,
        diameter: 1.0,
        samples: &sample,
        sample_count: 1,
        sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
    };
    let mut stroke_result = InkpodDispatchResult {
        struct_size: size_of::<InkpodDispatchResult>() as u32,
        reserved: 0,
        revision: 0,
        accepted_command_count: 0,
    };
    // SAFETY: Complete one-sample input and result are live on the Core owner thread.
    unsafe {
        assert_eq!(
            inkpod_core_apply_stroke(core, &stroke, &mut stroke_result),
            INKPOD_STATUS_OK
        );
    }
    let options = InkpodSnapshotOptions {
        struct_size: size_of::<InkpodSnapshotOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
    };
    let mut snapshot_id = object_id();
    // SAFETY: Complete options and empty output ID are live on the owner thread.
    unsafe {
        assert_eq!(
            inkpod_core_build_snapshot_id_v3(core, &options, &mut snapshot_id),
            INKPOD_STATUS_OK
        );
    }
    let mut snapshot_info = InkpodSnapshotInfoV3 {
        struct_size: size_of::<InkpodSnapshotInfoV3>() as u32,
        ..InkpodSnapshotInfoV3::default()
    };
    // SAFETY: Complete ID and writable metadata output are live.
    unsafe {
        assert_eq!(
            inkpod_core_snapshot_get_info_v3(core, &snapshot_id, &mut snapshot_info),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(
        (snapshot_info.document_width, snapshot_info.document_height),
        (4, 4)
    );
    assert!(snapshot_info.tile_count > 0);

    let mut tiles = vec![InkpodSnapshotTileInfoV3::default(); snapshot_info.tile_count as usize];
    let mut copied = 0;
    // SAFETY: The exact strided output batch is writable.
    unsafe {
        assert_eq!(
            inkpod_core_snapshot_tiles_copy_v3(
                core,
                &snapshot_id,
                0,
                tiles.as_mut_ptr(),
                tiles.len() as u64,
                size_of::<InkpodSnapshotTileInfoV3>() as u64,
                &mut copied,
            ),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(copied, snapshot_info.tile_count);
    assert!(tiles.iter().all(|tile| tile.pixel_bytes > 0));

    let mut pixel_query = InkpodBufferCopyV3 {
        struct_size: size_of::<InkpodBufferCopyV3>() as u32,
        ..InkpodBufferCopyV3::default()
    };
    // SAFETY: Zero-capacity query uses null storage.
    unsafe {
        assert_eq!(
            inkpod_core_snapshot_tile_pixels_copy_v3(core, &snapshot_id, 0, &mut pixel_query,),
            INKPOD_STATUS_OK
        );
    }
    let mut pixel_bytes = vec![0_u8; pixel_query.total_bytes as usize];
    pixel_query.bytes = pixel_bytes.as_mut_ptr();
    pixel_query.byte_capacity = pixel_bytes.len() as u64;
    // SAFETY: The advertised caller buffer is writable for the bounded copy.
    unsafe {
        assert_eq!(
            inkpod_core_snapshot_tile_pixels_copy_v3(core, &snapshot_id, 0, &mut pixel_query,),
            INKPOD_STATUS_OK
        );
    }
    assert_eq!(pixel_query.written_bytes, pixel_query.total_bytes);

    let mut zero = 99;
    // SAFETY: Empty record families use null/zero capacity/stride and a writable copied count.
    unsafe {
        assert_eq!(
            inkpod_core_snapshot_guides_copy_v3(
                core,
                &snapshot_id,
                0,
                ptr::null_mut(),
                0,
                0,
                &mut zero,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(zero, 0);
        assert_eq!(
            inkpod_core_snapshot_vector_segments_copy_v3(
                core,
                &snapshot_id,
                0,
                ptr::null_mut(),
                0,
                0,
                &mut zero,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_snapshot_vector_fills_copy_v3(
                core,
                &snapshot_id,
                0,
                ptr::null_mut(),
                0,
                0,
                &mut zero,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_snapshot_vector_boundary_ids_copy_v3(
                core,
                &snapshot_id,
                0,
                ptr::null_mut(),
                0,
                0,
                &mut zero,
            ),
            INKPOD_STATUS_OK
        );
    }

    let mut thumbnail_id = object_id();
    let mut export_id = object_id();
    let mut task_id = object_id();
    // SAFETY: Empty output IDs and fixed scalar arguments are valid on the Core owner thread.
    unsafe {
        assert_eq!(
            inkpod_core_layer_thumbnail_id_v3(core, document.layer_id, 16, 16, &mut thumbnail_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_export_common_raster_id_v3(
                core,
                INKPOD_COMMON_RASTER_PNG,
                0,
                &mut export_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_task_create_v3(core, &mut task_id),
            INKPOD_STATUS_OK
        );
    }
    for id in [thumbnail_id, export_id] {
        let mut object_info = InkpodObjectInfoV3 {
            struct_size: size_of::<InkpodObjectInfoV3>() as u32,
            ..InkpodObjectInfoV3::default()
        };
        let mut copy = InkpodBufferCopyV3 {
            struct_size: size_of::<InkpodBufferCopyV3>() as u32,
            ..InkpodBufferCopyV3::default()
        };
        // SAFETY: Metadata and zero-capacity byte-count queries are complete.
        unsafe {
            assert_eq!(
                inkpod_core_object_get_info_v3(core, &id, &mut object_info),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_object_bytes_copy_v3(core, &id, &mut copy),
                INKPOD_STATUS_OK
            );
        }
        assert_eq!(object_info.byte_count, copy.total_bytes);
        assert!(copy.total_bytes > 0);
    }
    let mut task_info = InkpodTaskInfo {
        struct_size: size_of::<InkpodTaskInfo>() as u32,
        state: 99,
        reserved: 0,
        completed_work: 0,
        total_work: 0,
    };
    // SAFETY: Live task ID and complete output are used on the owner thread.
    unsafe {
        assert_eq!(
            inkpod_core_task_query_v3(core, &task_id, &mut task_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(task_info.state, INKPOD_TASK_READY);
        assert_eq!(inkpod_core_task_cancel_v3(core, &task_id), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_task_query_v3(core, &task_id, &mut task_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(task_info.state, INKPOD_TASK_CANCELLED);
        for id in [snapshot_id, thumbnail_id, export_id, task_id] {
            assert_eq!(inkpod_core_object_release_v3(core, &id), INKPOD_STATUS_OK);
        }
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn v3_nested_structure_schema_and_output_validation_fail_closed() {
    let (mut core, document) = create_v3_core(5);
    let mut request = primitive_request(
        INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR,
        1,
        document.document_revision,
    );
    request.color = rgba8(1, 2, 3, 255);
    let mut result = primitive_result();
    // SAFETY: Each deliberately malformed record exposes at least its readable size prefix.
    unsafe {
        let original_size = request.struct_size;
        request.struct_size -= 1;
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &request, &mut result),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        request.struct_size = original_size;
        request.payload_id.struct_size -= 1;
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &request, &mut result),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        request.payload_id.struct_size = size_of::<InkpodObjectId>() as u32;
        request.opcode = u32::MAX;
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &request, &mut result),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        request.opcode = INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR;
        request.schema_version = 99;
        assert_eq!(
            inkpod_core_primitive_execute_v3(core, &request, &mut result),
            INKPOD_STATUS_UNSUPPORTED
        );
        let mut occupied = object_id();
        occupied.object_type = INKPOD_OBJECT_CORE;
        occupied.generation = 1;
        occupied.value = 1;
        assert_eq!(
            inkpod_core_get_id_v3(core, &mut occupied),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}
