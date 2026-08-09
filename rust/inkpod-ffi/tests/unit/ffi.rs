use super::*;

static PERSISTENCE_PATH_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

fn config() -> InkpodCoreConfig {
    InkpodCoreConfig {
        struct_size: size_of::<InkpodCoreConfig>() as u32,
        abi_version: INKPOD_ABI_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
    }
}

#[test]
fn complete_cell_creation_plan_is_bounded_owned_and_atomic() {
    let mut plan = ptr::null_mut();
    let options = InkpodCellCreationOptions {
        struct_size: size_of::<InkpodCellCreationOptions>() as u32,
        sizing_mode: INKPOD_CELL_SIZING_IMAGE_PIXELS,
        feature_flags: INKPOD_FEATURE_NONE,
        width: 320,
        height: 180,
        dpi_x_milli: 144_000,
        dpi_y_milli: 144_000,
        margin_milli: 50,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: INKPOD_FRAME_ANCHOR_CENTER,
        initial_layer_kind: INKPOD_LAYER_GRAYSCALE_COLORING,
        pixel_format: INKPOD_STORAGE_RGBA16,
        count: 3,
        reserved: 0,
    };
    // SAFETY: All size-prefixed records and owner pointers remain live and aligned.
    unsafe {
        assert_eq!(
            inkpod_cell_creation_plan_create(&options, ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut short = options;
        short.struct_size -= 1;
        assert_eq!(
            inkpod_cell_creation_plan_create(&short, &mut plan),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert!(plan.is_null());
        let mut unknown = options;
        unknown.sizing_mode = u32::MAX;
        assert_eq!(
            inkpod_cell_creation_plan_create(&unknown, &mut plan),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut oversized = options;
        oversized.count = MAX_CELL_CREATION_COUNT + 1;
        assert_eq!(
            inkpod_cell_creation_plan_create(&oversized, &mut plan),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_cell_creation_plan_create(&options, &mut plan),
            INKPOD_STATUS_OK
        );
        assert!(!plan.is_null());
        let mut count = 0_u32;
        assert_eq!(
            inkpod_cell_creation_plan_count(plan, &mut count),
            INKPOD_STATUS_OK
        );
        assert_eq!(count, 3);
        assert_eq!(
            inkpod_cell_creation_plan_count(ptr::null(), &mut count),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_cell_creation_plan_count(plan, ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut items = [InkpodCellCreationPlanItem {
            struct_size: size_of::<InkpodCellCreationPlanItem>() as u32,
            ..InkpodCellCreationPlanItem::default()
        }; 3];
        let mut written = 0_u32;
        items[0].width = 41;
        items[1].width = 42;
        items[2].width = 43;
        items[1].struct_size -= 1;
        assert_eq!(
            inkpod_cell_creation_plan_copy(
                plan,
                items.as_mut_ptr(),
                items.len() as u32,
                size_of::<InkpodCellCreationPlanItem>() as u64,
                &mut written,
            ),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(written, 0);
        assert_eq!(
            [items[0].width, items[1].width, items[2].width],
            [41, 42, 43]
        );
        items[1].struct_size = size_of::<InkpodCellCreationPlanItem>() as u32;
        assert_eq!(
            inkpod_cell_creation_plan_copy(
                plan,
                items.as_mut_ptr(),
                items.len() as u32,
                size_of::<InkpodCellCreationPlanItem>() as u64 - 1,
                &mut written,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(written, 0);
        assert_eq!(
            inkpod_cell_creation_plan_copy(
                plan,
                ptr::null_mut(),
                items.len() as u32,
                size_of::<InkpodCellCreationPlanItem>() as u64,
                &mut written,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(written, 0);
        assert_eq!(
            inkpod_cell_creation_plan_copy(
                plan,
                items.as_mut_ptr(),
                items.len() as u32,
                size_of::<InkpodCellCreationPlanItem>() as u64,
                &mut written,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(written, 3);
        assert_eq!(items[0].width, 320);
        assert_eq!(items[0].height, 180);
        assert_eq!(items[0].pixel_format, INKPOD_STORAGE_RGBA16);
        assert_eq!(items[0].shooting_frame, items[0].hundred_frame);
        assert_eq!(items[0], items[1]);

        let mut core = ptr::null_mut();
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        let mut info = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_new_cell_from_plan(core, plan, 3, 1, 1, &mut info),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_NO_DOCUMENT
        );
        assert_eq!(
            inkpod_core_new_cell_from_plan(core, plan, 0, 1, 1, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.width, 320);
        assert_eq!(info.height, 180);
        assert_eq!(info.shooting_frame, items[0].shooting_frame);
        assert_eq!(info.maximum_close_frame, items[0].maximum_close_frame);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_cell_creation_plan_release(&mut plan),
            INKPOD_STATUS_OK
        );
        assert!(plan.is_null());
        assert_eq!(
            inkpod_cell_creation_plan_release(&mut plan),
            INKPOD_STATUS_OK
        );
    }
}

#[test]
fn persistence_checkpoint_and_compaction_abi_are_bounded_confirmed_and_atomic() {
    let mut core = ptr::null_mut();
    let mut document = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..InkpodDocumentInfo::default()
    };
    let create = InkpodCellCreateOptions {
        struct_size: size_of::<InkpodCellCreateOptions>() as u32,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
        document_uuid_high: 1,
        document_uuid_low: 9,
        width: 4,
        height: 4,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
    };
    let path = std::env::temp_dir().join(format!(
        "inkpod-ffi-compaction-{}-{}.inkpod",
        std::process::id(),
        PERSISTENCE_PATH_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let path_bytes = path.to_str().unwrap().as_bytes().to_vec();
    // SAFETY: All public records, handles, and exact UTF-8 spans remain live.
    unsafe {
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_new_cell(core, &create, &mut document),
            INKPOD_STATUS_OK
        );
        let mut short = InkpodPersistenceInfo {
            struct_size: size_of::<InkpodPersistenceInfo>() as u32 - 1,
            format_version: u32::MAX,
            ..InkpodPersistenceInfo::default()
        };
        assert_eq!(
            inkpod_core_get_persistence_info(core, &mut short),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(short.format_version, u32::MAX);
        let mut persistence = InkpodPersistenceInfo {
            struct_size: size_of::<InkpodPersistenceInfo>() as u32,
            ..InkpodPersistenceInfo::default()
        };
        assert_eq!(
            inkpod_core_get_persistence_info(core, &mut persistence),
            INKPOD_STATUS_OK
        );
        assert_eq!(persistence.format_version, 10);
        assert_eq!(persistence.open_strategy, INKPOD_NATIVE_OPEN_NOT_OPENED);
        assert_eq!(persistence.flags, 0);

        let mut stale = InkpodCompactionPlan {
            struct_size: size_of::<InkpodCompactionPlan>() as u32,
            ..InkpodCompactionPlan::default()
        };
        assert_eq!(
            inkpod_core_compaction_plan(core, &mut stale),
            INKPOD_STATUS_OK
        );
        let color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_8,
            red: 1,
            green: 2,
            blue: 3,
            alpha: 255,
        };
        let mut dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        assert_eq!(
            inkpod_core_set_main_line_color(core, &color, &mut dispatch),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_write_compacted_copy(
                core,
                path_bytes.as_ptr(),
                path_bytes.len() as u64,
                &stale,
            ),
            INKPOD_STATUS_INVALID_STATE
        );
        assert!(!path.exists());
        let mut current = InkpodCompactionPlan {
            struct_size: size_of::<InkpodCompactionPlan>() as u32,
            ..InkpodCompactionPlan::default()
        };
        assert_eq!(
            inkpod_core_compaction_plan(core, &mut current),
            INKPOD_STATUS_OK
        );
        assert_eq!(current.history_procedure_count, 1);
        assert_eq!(
            inkpod_core_write_compacted_copy(
                core,
                path_bytes.as_ptr(),
                path_bytes.len() as u64,
                &current,
            ),
            INKPOD_STATUS_OK
        );
        let before_revision = dispatch.revision;
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        assert_eq!(document.document_revision, before_revision);
        assert_ne!(document.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO, 0);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn resource_usage_query_validates_output_and_is_read_only() {
    let mut core = ptr::null_mut();
    let config = config();
    // SAFETY: Config and output records remain live, aligned, and non-overlapping.
    unsafe {
        assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_get_resource_usage(core, ptr::null_mut()),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let mut short = InkpodResourceUsage {
            struct_size: size_of::<InkpodResourceUsage>() as u32 - 1,
            feature_flags: u64::MAX,
            ..InkpodResourceUsage::default()
        };
        assert_eq!(
            inkpod_core_get_resource_usage(core, &mut short),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(short.feature_flags, u64::MAX);

        let mut usage = InkpodResourceUsage {
            struct_size: size_of::<InkpodResourceUsage>() as u32,
            feature_flags: u64::MAX,
            ..InkpodResourceUsage::default()
        };
        assert_eq!(
            inkpod_core_get_resource_usage(core, &mut usage),
            INKPOD_STATUS_OK
        );
        assert_eq!(usage.feature_flags, INKPOD_FEATURE_NONE);
        assert_eq!(usage.document_tile_bytes, 0);
        assert_eq!(usage.document_tile_count, 0);
        assert_eq!(usage.history_bytes, 0);
        assert_eq!(usage.history_entry_count, 0);
        assert_eq!(usage.thumbnail_cache_bytes, 0);

        let mut document = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_NO_DOCUMENT
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn light_table_sequence_motion_and_dirty_switch_abi_are_connected() {
    let mut core = ptr::null_mut();
    let config = config();
    // SAFETY: Test records remain live and non-overlapping for each call.
    unsafe {
        assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
        let options = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 1,
            document_uuid_low: 1,
            width: 8,
            height: 8,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut document = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut document),
            INKPOD_STATUS_OK
        );

        let mut light_pixels = [0_u8; 4 * 20];
        light_pixels[2 * 20 + 2 * 4..2 * 20 + 2 * 4 + 4].copy_from_slice(&[90, 80, 70, 255]);
        let raster = InkpodRasterSourceInput {
            struct_size: size_of::<InkpodRasterSourceInput>() as u32,
            pixel_format: INKPOD_STORAGE_RGBA8,
            flags: 0,
            document_uuid_high: 2,
            document_uuid_low: 2,
            source_revision: 3,
            width: 4,
            height: 4,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
            reference_frame: InkpodFrameRect {
                x: 2,
                y: 2,
                width: 4,
                height: 4,
            },
            pixels: light_pixels.as_ptr(),
            pixel_bytes: light_pixels.len() as u64,
            row_stride_bytes: 20,
        };
        let name = b"reference";
        let item = InkpodLightTableItemInput {
            struct_size: size_of::<InkpodLightTableItemInput>() as u32,
            flags: INKPOD_LIGHT_TABLE_ITEM_VISIBLE,
            opacity_milli: 500,
            display_mode: INKPOD_LIGHT_TABLE_COLOR,
            display_color: InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_8,
                red: 0,
                green: 128,
                blue: 255,
                alpha: 255,
            },
            translate_x_milli: 0,
            translate_y_milli: 0,
            scale_x_milli: 1_000,
            scale_y_milli: 1_000,
            rotation_milli_degrees: 0,
            reserved: 0,
            name_utf8: name.as_ptr(),
            name_bytes: name.len() as u64,
            source: raster,
        };
        let mut dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        let mut item_id = 0;
        assert_eq!(
            inkpod_core_light_table_add_item(core, &item, &mut dispatch, &mut item_id),
            INKPOD_STATUS_OK
        );
        assert_ne!(item_id, 0);
        light_pixels.fill(0);
        assert_eq!(
            inkpod_core_light_table_set_global_opacity(core, 500, &mut dispatch),
            INKPOD_STATUS_OK
        );
        let mut sampled = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        };
        assert_eq!(
            inkpod_core_light_table_sample(core, 4, 4, &mut sampled),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (sampled.red, sampled.green, sampled.blue, sampled.alpha),
            (90, 80, 70, 64)
        );
        let fill_color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_8,
            red: 200,
            green: 10,
            blue: 20,
            alpha: 255,
        };
        let fill = InkpodFillInput {
            struct_size: size_of::<InkpodFillInput>() as u32,
            operation: INKPOD_FILL_SEED,
            flags: INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY,
            seed_x: 0,
            seed_y: 0,
            color: fill_color,
            tolerance: 0,
            gap_close: 0,
            inclusion_mode: INKPOD_INCLUSION_NONE,
            selection: InkpodFrameRect::default(),
            inclusion_colors: ptr::null(),
            inclusion_color_count: 0,
            inclusion_color_stride_bytes: 0,
            extension_distance: 0,
            reserved: 0,
        };
        let mut fill_result = InkpodFillResult {
            struct_size: size_of::<InkpodFillResult>() as u32,
            ..InkpodFillResult::default()
        };
        assert_eq!(
            inkpod_core_apply_fill(core, &fill, &mut fill_result),
            INKPOD_STATUS_OK
        );
        assert_eq!(fill_result.changed_pixel_count, 63);
        assert_eq!(
            inkpod_core_light_table_sample(core, 4, 4, &mut sampled),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (sampled.red, sampled.green, sampled.blue, sampled.alpha),
            (90, 80, 70, 64)
        );
        let mut sixteen_pixels = [0_u8; 8];
        for (index, channel) in [1_u16, 257, 32_769, 65_535].into_iter().enumerate() {
            sixteen_pixels[index * 2..index * 2 + 2].copy_from_slice(&channel.to_le_bytes());
        }
        let sixteen_name = b"rgba16";
        let sixteen_item = InkpodLightTableItemInput {
            struct_size: size_of::<InkpodLightTableItemInput>() as u32,
            flags: INKPOD_LIGHT_TABLE_ITEM_VISIBLE,
            opacity_milli: 1_000,
            display_mode: INKPOD_LIGHT_TABLE_COLOR,
            display_color: InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_16,
                red: 0,
                green: 0,
                blue: 0,
                alpha: u16::MAX,
            },
            translate_x_milli: 0,
            translate_y_milli: 0,
            scale_x_milli: 1_000,
            scale_y_milli: 1_000,
            rotation_milli_degrees: 0,
            reserved: 0,
            name_utf8: sixteen_name.as_ptr(),
            name_bytes: sixteen_name.len() as u64,
            source: InkpodRasterSourceInput {
                struct_size: size_of::<InkpodRasterSourceInput>() as u32,
                pixel_format: INKPOD_STORAGE_RGBA16,
                flags: 0,
                document_uuid_high: 3,
                document_uuid_low: 3,
                source_revision: 1,
                width: 1,
                height: 1,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
                reference_frame: InkpodFrameRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                pixels: sixteen_pixels.as_ptr(),
                pixel_bytes: sixteen_pixels.len() as u64,
                row_stride_bytes: 8,
            },
        };
        let mut sixteen_item_id = 0;
        assert_eq!(
            inkpod_core_light_table_add_item(
                core,
                &sixteen_item,
                &mut dispatch,
                &mut sixteen_item_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_ne!(sixteen_item_id, 0);
        sixteen_pixels.fill(0);
        assert_eq!(
            inkpod_core_light_table_sample(core, 4, 4, &mut sampled),
            INKPOD_STATUS_OK
        );
        assert_eq!(sampled.depth, INKPOD_COLOR_DEPTH_16);
        assert_eq!(
            (sampled.red, sampled.green, sampled.blue, sampled.alpha),
            (1, 257, 32_769, 32_768)
        );
        assert_eq!(
            inkpod_core_light_table_swap(core, item_id, &mut document),
            INKPOD_STATUS_UNSAVED_CHANGES
        );

        let mut sequence_pixels_a = [1_u8, 2, 3, 255];
        let mut sequence_pixels_b = [4_u8, 5, 6, 255];
        let names = [b"cell10.png".as_slice(), b"cell2.png".as_slice()];
        let pixels = [sequence_pixels_a.as_slice(), sequence_pixels_b.as_slice()];
        let mut cells = Vec::new();
        for index in 0..2 {
            cells.push(InkpodSequenceCellInput {
                struct_size: size_of::<InkpodSequenceCellInput>() as u32,
                reserved: 0,
                name_utf8: names[index].as_ptr(),
                name_bytes: names[index].len() as u64,
                source: InkpodRasterSourceInput {
                    struct_size: size_of::<InkpodRasterSourceInput>() as u32,
                    pixel_format: INKPOD_STORAGE_RGBA8,
                    flags: 0,
                    document_uuid_high: 5,
                    document_uuid_low: index as u64 + 1,
                    source_revision: 1,
                    width: 1,
                    height: 1,
                    dpi_x_milli: 96_000,
                    dpi_y_milli: 96_000,
                    reference_frame: InkpodFrameRect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    pixels: pixels[index].as_ptr(),
                    pixel_bytes: 4,
                    row_stride_bytes: 4,
                },
            });
        }
        let sequence = InkpodSequenceInput {
            struct_size: size_of::<InkpodSequenceInput>() as u32,
            reserved: 0,
            feature_flags: 0,
            cells: cells.as_ptr(),
            cell_count: cells.len() as u64,
            cell_stride_bytes: size_of::<InkpodSequenceCellInput>() as u64,
        };
        assert_eq!(inkpod_core_sequence_set(core, &sequence), INKPOD_STATUS_OK);
        sequence_pixels_a.fill(0);
        sequence_pixels_b.fill(0);
        assert_eq!(
            inkpod_core_sequence_step(core, INKPOD_SEQUENCE_NEXT, 0, &mut document),
            INKPOD_STATUS_UNSAVED_CHANGES
        );
        let motion = InkpodMotionCheckInput {
            struct_size: size_of::<InkpodMotionCheckInput>() as u32,
            fps: 24,
            flags: INKPOD_MOTION_FLAG_LOOP,
        };
        let mut frame = InkpodMotionFrame {
            struct_size: size_of::<InkpodMotionFrame>() as u32,
            ..InkpodMotionFrame::default()
        };
        assert_eq!(
            inkpod_core_motion_check_start(core, &motion, &mut frame),
            INKPOD_STATUS_OK
        );
        assert_eq!(frame.cell_number, 2);
        assert_ne!(frame.thumbnail_checksum, 0);
        assert_eq!(
            inkpod_core_motion_check_step(core, INKPOD_SEQUENCE_NEXT, &mut frame),
            INKPOD_STATUS_OK
        );
        assert_eq!(frame.cell_number, 10);
        assert_eq!(inkpod_core_motion_check_stop(core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn ffi_rejects_nested_raster_bounds_and_extreme_rotation_without_mutation() {
    let mut core = ptr::null_mut();
    let config = config();
    // SAFETY: Test records remain live and non-overlapping for every call.
    unsafe {
        assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
        let options = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 7,
            document_uuid_low: 7,
            width: 1,
            height: 1,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut before = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut before),
            INKPOD_STATUS_OK
        );
        let pixel = [1_u8, 2, 3, 255];
        let name = b"invalid";
        let raster = InkpodRasterSourceInput {
            struct_size: size_of::<InkpodRasterSourceInput>() as u32,
            pixel_format: INKPOD_STORAGE_RGBA8,
            flags: 0,
            document_uuid_high: 8,
            document_uuid_low: 8,
            source_revision: 1,
            width: 1,
            height: 1,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
            reference_frame: InkpodFrameRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            pixels: pixel.as_ptr(),
            pixel_bytes: pixel.len() as u64,
            row_stride_bytes: 4,
        };
        let base_item = InkpodLightTableItemInput {
            struct_size: size_of::<InkpodLightTableItemInput>() as u32,
            flags: INKPOD_LIGHT_TABLE_ITEM_VISIBLE,
            opacity_milli: 1_000,
            display_mode: INKPOD_LIGHT_TABLE_COLOR,
            display_color: InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_8,
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            translate_x_milli: 0,
            translate_y_milli: 0,
            scale_x_milli: 1_000,
            scale_y_milli: 1_000,
            rotation_milli_degrees: 0,
            reserved: 0,
            name_utf8: name.as_ptr(),
            name_bytes: name.len() as u64,
            source: raster,
        };
        let mut dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        let mut item_id = 0;

        let mut short_nested = base_item;
        short_nested.source.struct_size = size_of::<InkpodRasterSourceInput>() as u32 - 1;
        assert_eq!(
            inkpod_core_light_table_add_item(core, &short_nested, &mut dispatch, &mut item_id,),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );

        let mut oversized = base_item;
        oversized.source.width = MAX_RASTER_DIMENSION + 1;
        assert_eq!(
            inkpod_core_light_table_add_item(core, &oversized, &mut dispatch, &mut item_id,),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let mut invalid_reference = base_item;
        invalid_reference.source.reference_frame.width = 0;
        assert_eq!(
            inkpod_core_light_table_add_item(core, &invalid_reference, &mut dispatch, &mut item_id,),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let mut extreme_rotation = base_item;
        extreme_rotation.rotation_milli_degrees = i32::MIN;
        assert_eq!(
            inkpod_core_light_table_add_item(core, &extreme_rotation, &mut dispatch, &mut item_id,),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let mut after = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_get_document_info(core, &mut after),
            INKPOD_STATUS_OK
        );
        assert_eq!(after.document_revision, before.document_revision);
        assert_eq!(after.flags, before.flags);
        assert_eq!(item_id, 0);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn abi_001_lifecycle_and_double_release_are_safe() {
    let mut core = ptr::null_mut();
    // SAFETY: All pointers reference initialized local storage.
    assert_eq!(
        unsafe { inkpod_core_create(&config(), &mut core) },
        INKPOD_STATUS_OK
    );
    assert!(!core.is_null());

    let options = InkpodSnapshotOptions {
        struct_size: size_of::<InkpodSnapshotOptions>() as u32,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
    };
    let mut snapshot = ptr::null_mut();
    // SAFETY: The core is live and outputs point to local storage.
    assert_eq!(
        unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
        INKPOD_STATUS_OK
    );

    let mut view = InkpodSnapshotView {
        struct_size: size_of::<InkpodSnapshotView>() as u32,
        abi_version: 0,
        feature_flags: u64::MAX,
        revision: u64::MAX,
        tiles: ptr::null(),
        tile_count: u64::MAX,
        tile_stride_bytes: 0,
    };
    // SAFETY: Snapshot and output view are live for this call.
    assert_eq!(
        unsafe { inkpod_snapshot_get_view(snapshot, &mut view) },
        INKPOD_STATUS_OK
    );
    assert_eq!(view.abi_version, INKPOD_ABI_VERSION);
    assert_eq!(view.revision, 0);
    assert!(view.tiles.is_null());
    assert_eq!(view.tile_count, 0);
    assert_eq!(
        view.tile_stride_bytes,
        size_of::<InkpodSnapshotTile>() as u64
    );

    // SAFETY: Owner variables contain live handles, then null after first calls.
    assert_eq!(
        unsafe { inkpod_snapshot_release(&mut snapshot) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_snapshot_release(&mut snapshot) },
        INKPOD_STATUS_OK
    );
    assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
}

#[test]
fn abi_001_rejects_null_and_short_structures() {
    #[repr(C, align(8))]
    struct StructSizePrefix {
        struct_size: u32,
    }

    let mut core = ptr::null_mut();
    // SAFETY: Null input is intentionally tested; output is writable.
    assert_eq!(
        unsafe { inkpod_core_create(ptr::null(), &mut core) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    assert!(core.is_null());

    let short = StructSizePrefix { struct_size: 4 };
    // SAFETY: The deliberately short allocation contains the required size
    // prefix and is sufficiently aligned; no complete config is advertised.
    assert_eq!(
        unsafe { inkpod_core_create((&raw const short).cast::<InkpodCoreConfig>(), &mut core) },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    assert!(core.is_null());

    // SAFETY: All pointers reference initialized local storage.
    assert_eq!(
        unsafe { inkpod_core_create(&config(), &mut core) },
        INKPOD_STATUS_OK
    );
    let mut short_output = StructSizePrefix { struct_size: 4 };
    let mut snapshot = ptr::null_mut();
    // SAFETY: The short options expose only their aligned size prefix.
    assert_eq!(
        unsafe {
            inkpod_core_build_snapshot(
                core,
                (&raw const short).cast::<InkpodSnapshotOptions>(),
                &mut snapshot,
            )
        },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    assert!(snapshot.is_null());

    let options = InkpodSnapshotOptions {
        struct_size: size_of::<InkpodSnapshotOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
    };
    // SAFETY: Inputs and output reference initialized local storage.
    assert_eq!(
        unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
        INKPOD_STATUS_OK
    );
    // SAFETY: The short view exposes only its writable size prefix.
    assert_eq!(
        unsafe {
            inkpod_snapshot_get_view(
                snapshot,
                (&raw mut short_output).cast::<InkpodSnapshotView>(),
            )
        },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );

    // SAFETY: Owner variables contain live handles.
    assert_eq!(
        unsafe { inkpod_snapshot_release(&mut snapshot) },
        INKPOD_STATUS_OK
    );
    assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
}

#[test]
fn abi_001_contains_panics_and_preserves_a_diagnostic() {
    clear_last_error();
    let status = ffi_boundary(|| panic!("intentional ABI containment test"));
    assert_eq!(status, INKPOD_STATUS_PANIC);

    let mut required = 0;
    // SAFETY: required is writable local storage.
    assert_eq!(
        unsafe { inkpod_error_message_size(&mut required) },
        INKPOD_STATUS_OK
    );
    assert!(required > 1);

    fail(INKPOD_STATUS_INVALID_ARGUMENT, &"界".repeat(ERROR_CAPACITY));
    // SAFETY: required and the subsequently sized buffer are writable.
    assert_eq!(
        unsafe { inkpod_error_message_size(&mut required) },
        INKPOD_STATUS_OK
    );
    let mut message = vec![0_u8; required as usize];
    let mut written = 0;
    // SAFETY: The buffer uses the exact queried capacity.
    assert_eq!(
        unsafe { inkpod_error_message_copy(message.as_mut_ptr(), required, &mut written) },
        INKPOD_STATUS_OK
    );
    assert!(std::str::from_utf8(&message[..written as usize]).is_ok());
}

#[test]
fn batched_stroke_snapshot_view_history_and_round_trip() {
    static PATH_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    let mut core = ptr::null_mut();
    // SAFETY: All pointers reference initialized local storage.
    assert_eq!(
        unsafe { inkpod_core_create(&config(), &mut core) },
        INKPOD_STATUS_OK
    );
    let create = InkpodCellCreateOptions {
        struct_size: size_of::<InkpodCellCreateOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
        document_uuid_high: 0x1234_5678_9abc_def0,
        document_uuid_low: 0x1032_5476_98ba_dcfe,
        width: 1920,
        height: 1080,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
    };
    let mut info = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..InkpodDocumentInfo::default()
    };
    // SAFETY: Core, options, and output are valid and non-overlapping.
    assert_eq!(
        unsafe { inkpod_core_new_cell(core, &create, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!((info.width, info.height), (1920, 1080));
    assert_eq!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
    let ids = (
        info.document_id,
        info.layer_id,
        info.main_plane_id,
        info.color_plane_id,
    );

    #[repr(C)]
    struct ExtendedStrokeSample {
        sample: InkpodStrokeSample,
        extension: u64,
    }
    let samples: Vec<_> = (0..256)
        .map(|index| ExtendedStrokeSample {
            sample: InkpodStrokeSample {
                struct_size: size_of::<ExtendedStrokeSample>() as u32,
                flags: 0,
                x: 10.0 + index as f32,
                y: 20.0,
                pressure: 0.5,
                reserved: 0,
            },
            extension: index,
        })
        .collect();
    let mut stroke = InkpodStrokeInput {
        struct_size: size_of::<InkpodStrokeInput>() as u32,
        tool: INKPOD_TOOL_PENCIL,
        plane: INKPOD_PLANE_MAIN_LINE,
        coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
        flags: 0,
        color_rgba: 0x0000_00ff,
        diameter: 1.0,
        samples: &samples[0].sample,
        sample_count: samples.len() as u64,
        sample_stride_bytes: size_of::<ExtendedStrokeSample>() as u64,
    };
    let mut dispatch = InkpodDispatchResult {
        struct_size: size_of::<InkpodDispatchResult>() as u32,
        reserved: 0,
        revision: 0,
        accepted_command_count: 0,
    };
    // SAFETY: One call borrows the complete 256-record sample span.
    assert_eq!(
        unsafe { inkpod_core_apply_stroke(core, &stroke, &mut dispatch) },
        INKPOD_STATUS_OK
    );
    assert_eq!(dispatch.accepted_command_count, 1);
    // SAFETY: Core and output are live owner-thread objects.
    assert_eq!(
        unsafe { inkpod_core_get_document_info(core, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
    let line_checksum = info.main_plane_checksum;

    stroke.plane = INKPOD_PLANE_COLOR;
    stroke.color_rgba = 0xdc_28_1e_ff;
    // SAFETY: The same complete sample span remains live.
    assert_eq!(
        unsafe { inkpod_core_apply_stroke(core, &stroke, &mut dispatch) },
        INKPOD_STATUS_OK
    );
    // SAFETY: Core and output are live owner-thread objects.
    assert_eq!(
        unsafe { inkpod_core_get_document_info(core, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.main_plane_checksum, line_checksum);
    assert_ne!(info.color_plane_checksum, 0);
    let color_checksum = info.color_plane_checksum;
    // SAFETY: History result storage is live and non-overlapping.
    assert_eq!(
        unsafe { inkpod_core_undo(core, &mut dispatch) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_core_redo(core, &mut dispatch) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_core_get_document_info(core, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.color_plane_checksum, color_checksum);

    let revision_before_view = info.document_revision;
    let view_input = InkpodViewInput {
        struct_size: size_of::<InkpodViewInput>() as u32,
        kind: INKPOD_VIEW_ZOOM_AT,
        flags: 0,
        value1: 2.0,
        value2: 320.0,
        value3: 240.0,
        value4: 0.0,
    };
    // SAFETY: Input/output/Core are complete owner-thread objects.
    assert_eq!(
        unsafe { inkpod_core_apply_view(core, &view_input, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.document_revision, revision_before_view);

    let options = InkpodSnapshotOptions {
        struct_size: size_of::<InkpodSnapshotOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
    };
    let mut snapshot = ptr::null_mut();
    // SAFETY: Core/options/output are valid and non-overlapping.
    assert_eq!(
        unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
        INKPOD_STATUS_OK
    );
    let mut view = InkpodSnapshotView {
        struct_size: size_of::<InkpodSnapshotView>() as u32,
        abi_version: 0,
        feature_flags: 0,
        revision: 0,
        tiles: ptr::null(),
        tile_count: 0,
        tile_stride_bytes: 0,
    };
    let mut transform = InkpodSnapshotTransform {
        struct_size: size_of::<InkpodSnapshotTransform>() as u32,
        ..InkpodSnapshotTransform::default()
    };
    // SAFETY: Snapshot and outputs remain live for both view calls.
    assert_eq!(
        unsafe { inkpod_snapshot_get_view(snapshot, &mut view) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_snapshot_get_transform(snapshot, &mut transform) },
        INKPOD_STATUS_OK
    );
    assert!(view.tile_count > 0 && !view.tiles.is_null());
    assert_eq!(transform.zoom, 2.0);
    assert_eq!(
        (transform.document_width, transform.document_height),
        (1920, 1080)
    );
    // SAFETY: Owner variable contains the live snapshot handle.
    assert_eq!(
        unsafe { inkpod_snapshot_release(&mut snapshot) },
        INKPOD_STATUS_OK
    );

    let path = std::env::temp_dir().join(format!(
        "inkpod-ffi-test-{}-{}.inkpod",
        std::process::id(),
        PATH_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let path = path.to_str().unwrap().as_bytes().to_vec();
    // SAFETY: UTF-8 path bytes and output remain live for this call.
    assert_eq!(
        unsafe { inkpod_core_save(core, path.as_ptr(), path.len() as u64, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
    // Discard the in-memory document, then reopen from the saved container.
    assert_eq!(
        unsafe { inkpod_core_new_cell(core, &create, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_ne!(info.document_id, ids.0);
    assert_eq!(
        unsafe { inkpod_core_open(core, path.as_ptr(), path.len() as u64, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        (
            info.document_id,
            info.layer_id,
            info.main_plane_id,
            info.color_plane_id
        ),
        ids
    );
    assert_eq!(info.main_plane_checksum, line_checksum);
    assert_eq!(info.color_plane_checksum, color_checksum);
    std::fs::remove_file(std::str::from_utf8(&path).unwrap()).unwrap();
    // SAFETY: Owner variable contains the live Core handle.
    assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
}

#[test]
fn live_stroke_abi_previews_then_commits_once_and_cancel_is_safe() {
    let mut core = ptr::null_mut();
    // SAFETY: Config and output storage are complete and non-overlapping.
    assert_eq!(
        unsafe { inkpod_core_create(&config(), &mut core) },
        INKPOD_STATUS_OK
    );
    let create = InkpodCellCreateOptions {
        struct_size: size_of::<InkpodCellCreateOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
        document_uuid_high: 7,
        document_uuid_low: 11,
        width: 64,
        height: 64,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
    };
    let mut info = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..InkpodDocumentInfo::default()
    };
    // SAFETY: Core/options/output are live and non-overlapping.
    assert_eq!(
        unsafe { inkpod_core_new_cell(core, &create, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
    let initial_revision = info.document_revision;
    let initial_checksum = info.main_plane_checksum;
    let begin_sample = InkpodStrokeSample {
        struct_size: size_of::<InkpodStrokeSample>() as u32,
        flags: 0,
        x: 4.0,
        y: 4.0,
        pressure: 1.0,
        reserved: 0,
    };
    let begin = InkpodStrokeInput {
        struct_size: size_of::<InkpodStrokeInput>() as u32,
        tool: INKPOD_TOOL_PENCIL,
        plane: INKPOD_PLANE_MAIN_LINE,
        coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
        flags: 0,
        color_rgba: 0x0000_00ff,
        diameter: 1.0,
        samples: &begin_sample,
        sample_count: 1,
        sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
    };
    // SAFETY: The first sample and Core remain live for the call.
    assert_eq!(
        unsafe { inkpod_core_stroke_begin(core, &begin) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_core_get_document_info(core, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.document_revision, initial_revision);
    assert_eq!(info.main_plane_checksum, initial_checksum);
    assert_eq!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);

    let appended = [
        InkpodStrokeSample {
            x: 12.0,
            ..begin_sample
        },
        InkpodStrokeSample {
            x: 20.0,
            ..begin_sample
        },
    ];
    let span = InkpodStrokeSampleSpan {
        struct_size: size_of::<InkpodStrokeSampleSpan>() as u32,
        reserved: 0,
        feature_flags: 0,
        samples: appended.as_ptr(),
        sample_count: appended.len() as u64,
        sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
    };
    // SAFETY: The strided sample span is complete and borrowed for this call.
    assert_eq!(
        unsafe { inkpod_core_stroke_append(core, &span) },
        INKPOD_STATUS_OK
    );
    let options = InkpodSnapshotOptions {
        struct_size: size_of::<InkpodSnapshotOptions>() as u32,
        reserved: 0,
        feature_flags: 0,
    };
    let mut snapshot = ptr::null_mut();
    // SAFETY: Core/options/output are live and non-overlapping.
    assert_eq!(
        unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
        INKPOD_STATUS_OK
    );
    let mut view = InkpodSnapshotView {
        struct_size: size_of::<InkpodSnapshotView>() as u32,
        abi_version: 0,
        feature_flags: 0,
        revision: 0,
        tiles: ptr::null(),
        tile_count: 0,
        tile_stride_bytes: 0,
    };
    assert_eq!(
        unsafe { inkpod_snapshot_get_view(snapshot, &mut view) },
        INKPOD_STATUS_OK
    );
    assert!(view.revision >= 1_u64 << 63 && view.tile_count == 1);
    assert_eq!(
        unsafe { inkpod_snapshot_release(&mut snapshot) },
        INKPOD_STATUS_OK
    );

    let mut result = InkpodDispatchResult {
        struct_size: size_of::<InkpodDispatchResult>() as u32,
        reserved: 0,
        revision: 0,
        accepted_command_count: 0,
    };
    // SAFETY: Core and result are live owner-thread objects.
    assert_eq!(
        unsafe { inkpod_core_stroke_end(core, &mut result) },
        INKPOD_STATUS_OK
    );
    assert_eq!(result.revision, initial_revision + 1);
    assert_eq!(
        unsafe { inkpod_core_get_document_info(core, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_ne!(info.main_plane_checksum, initial_checksum);
    assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO, 0);
    assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);

    assert_eq!(
        unsafe { inkpod_core_stroke_begin(core, &begin) },
        INKPOD_STATUS_OK
    );
    assert_eq!(unsafe { inkpod_core_stroke_cancel(core) }, INKPOD_STATUS_OK);
    assert_eq!(unsafe { inkpod_core_stroke_cancel(core) }, INKPOD_STATUS_OK);

    let committed_revision = info.document_revision;
    let committed_checksum = info.main_plane_checksum;
    assert_eq!(
        unsafe { inkpod_core_stroke_begin(core, &begin) },
        INKPOD_STATUS_OK
    );
    let short_span = InkpodStrokeSampleSpan {
        struct_size: size_of::<u32>() as u32,
        ..span
    };
    assert_eq!(
        unsafe { inkpod_core_stroke_append(core, &short_span) },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    assert_eq!(
        unsafe { inkpod_core_stroke_end(core, &mut result) },
        INKPOD_STATUS_INVALID_STATE
    );
    assert_eq!(
        unsafe { inkpod_core_get_document_info(core, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.document_revision, committed_revision);
    assert_eq!(info.main_plane_checksum, committed_checksum);

    assert_eq!(
        unsafe { inkpod_core_stroke_begin(core, &begin) },
        INKPOD_STATUS_OK
    );
    let mut short_result = InkpodDispatchResult {
        struct_size: size_of::<u32>() as u32,
        ..result
    };
    assert_eq!(
        unsafe { inkpod_core_stroke_end(core, &mut short_result) },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    assert_eq!(
        unsafe { inkpod_core_stroke_end(core, &mut result) },
        INKPOD_STATUS_INVALID_STATE
    );
    assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
}

#[test]
fn fill_eyedropper_check_and_recovery_abi_are_transactional() {
    unsafe {
        let mut core = ptr::null_mut();
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        let options = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            document_uuid_high: 1,
            document_uuid_low: 2,
            width: 8,
            height: 8,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut info = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut info),
            INKPOD_STATUS_OK
        );
        let created_revision = info.document_revision;
        let created_main_checksum = info.main_plane_checksum;
        let created_color_checksum = info.color_plane_checksum;
        let color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_8,
            red: 12,
            green: 34,
            blue: 56,
            alpha: 255,
        };
        let mut fill = InkpodFillInput {
            struct_size: size_of::<InkpodFillInput>() as u32,
            operation: INKPOD_FILL_SEED,
            flags: INKPOD_FILL_FLAG_OVERFLOW_ABORT,
            seed_x: 4,
            seed_y: 4,
            color,
            tolerance: 0,
            gap_close: 0,
            inclusion_mode: INKPOD_INCLUSION_NONE,
            selection: InkpodFrameRect::default(),
            inclusion_colors: ptr::null(),
            inclusion_color_count: 0,
            inclusion_color_stride_bytes: 0,
            extension_distance: 0,
            reserved: 0,
        };
        let mut result = InkpodFillResult {
            struct_size: size_of::<InkpodFillResult>() as u32,
            ..InkpodFillResult::default()
        };
        let mut short_fill = fill;
        short_fill.struct_size = size_of::<u32>() as u32;
        assert_eq!(
            inkpod_core_apply_fill(core, &short_fill, &mut result),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        let mut unknown_fill = fill;
        unknown_fill.operation = 99;
        assert_eq!(
            inkpod_core_apply_fill(core, &unknown_fill, &mut result),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut short_result = InkpodFillResult {
            struct_size: size_of::<u32>() as u32,
            ..InkpodFillResult::default()
        };
        assert_eq!(
            inkpod_core_apply_fill(core, &fill, &mut short_result),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(
            inkpod_core_apply_fill(core, &fill, &mut result),
            INKPOD_STATUS_FILL_OVERFLOW
        );
        assert_ne!(result.flags & INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE, 0);
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, created_revision);
        assert_eq!(info.color_plane_checksum, created_color_checksum);

        fill.flags = INKPOD_FILL_FLAG_SELECTION_PRESENT;
        fill.seed_x = 2;
        fill.seed_y = 2;
        fill.selection = InkpodFrameRect {
            x: 2,
            y: 2,
            width: 2,
            height: 2,
        };
        assert_eq!(
            inkpod_core_apply_fill(core, &fill, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(result.changed_pixel_count, 4);
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, created_revision + 1);
        assert_eq!(info.main_plane_checksum, created_main_checksum);
        assert_ne!(info.color_plane_checksum, created_color_checksum);

        let mut sampled = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        };
        assert_eq!(
            inkpod_core_eyedropper(core, INKPOD_EYEDROPPER_SELECTED_PLANE, 2, 2, &mut sampled,),
            INKPOD_STATUS_OK
        );
        assert_eq!(sampled.depth, INKPOD_COLOR_DEPTH_8);
        assert_eq!(
            [sampled.red, sampled.green, sampled.blue, sampled.alpha],
            [12, 34, 56, 255]
        );

        let revision_before_check = info.document_revision;
        let view_before_check = info.view_revision;
        assert_eq!(
            inkpod_core_set_color_check(core, INKPOD_COLOR_CHECK_NATIVE_ALPHA),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, revision_before_check);
        assert!(info.view_revision > view_before_check);

        let snapshot_options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
        };
        let mut check_snapshot = ptr::null_mut();
        assert_eq!(
            inkpod_core_build_snapshot(core, &snapshot_options, &mut check_snapshot),
            INKPOD_STATUS_OK
        );
        let mut check_view = InkpodSnapshotView {
            struct_size: size_of::<InkpodSnapshotView>() as u32,
            abi_version: 0,
            feature_flags: 0,
            revision: 0,
            tiles: ptr::null(),
            tile_count: 0,
            tile_stride_bytes: 0,
        };
        assert_eq!(
            inkpod_snapshot_get_view(check_snapshot, &mut check_view),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            check_view.feature_flags,
            INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA
                | INKPOD_SNAPSHOT_FEATURE_SOLID_WHITE_BASE
        );
        assert_eq!(
            inkpod_snapshot_release(&mut check_snapshot),
            INKPOD_STATUS_OK
        );

        let palette = [
            InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_8,
                red: 12,
                green: 34,
                blue: 56,
                alpha: 255,
            },
            InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_16,
                red: 1,
                green: 257,
                blue: 32_769,
                alpha: 65_534,
            },
        ];
        let palette_input = InkpodColorArray {
            struct_size: size_of::<InkpodColorArray>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            colors: palette.as_ptr(),
            color_count: palette.len() as u64,
            color_stride_bytes: size_of::<InkpodColorValue>() as u64,
        };
        let invalid_empty_palette = InkpodColorArray {
            colors: palette.as_ptr(),
            color_count: 0,
            color_stride_bytes: 0,
            ..palette_input
        };
        let mut palette_dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        assert_eq!(
            inkpod_core_palette_set(core, &invalid_empty_palette, &mut palette_dispatch,),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_palette_set(core, &palette_input, &mut palette_dispatch),
            INKPOD_STATUS_OK
        );
        let mut count_query = InkpodColorBuffer {
            struct_size: size_of::<InkpodColorBuffer>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            colors: ptr::null_mut(),
            color_capacity: 0,
            color_stride_bytes: 0,
            color_count: 0,
        };
        assert_eq!(
            inkpod_core_palette_get(core, &mut count_query),
            INKPOD_STATUS_OK
        );
        assert_eq!(count_query.color_count, palette.len() as u64);
        let mut too_small_record = InkpodColorValue {
            struct_size: 77,
            depth: 88,
            red: 99,
            green: 100,
            blue: 101,
            alpha: 102,
        };
        let mut too_small_buffer = InkpodColorBuffer {
            struct_size: size_of::<InkpodColorBuffer>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            colors: &mut too_small_record,
            color_capacity: 1,
            color_stride_bytes: size_of::<InkpodColorValue>() as u64,
            color_count: 0,
        };
        assert_eq!(
            inkpod_core_palette_get(core, &mut too_small_buffer),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(too_small_buffer.color_count, palette.len() as u64);
        assert_eq!(too_small_record.struct_size, 77);
        assert_eq!(too_small_record.depth, 88);
        let mut copied = [InkpodColorValue::default(); 2];
        let mut palette_buffer = InkpodColorBuffer {
            struct_size: size_of::<InkpodColorBuffer>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            colors: copied.as_mut_ptr(),
            color_capacity: copied.len() as u64,
            color_stride_bytes: size_of::<InkpodColorValue>() as u64,
            color_count: 0,
        };
        assert_eq!(
            inkpod_core_palette_get(core, &mut palette_buffer),
            INKPOD_STATUS_OK
        );
        assert_eq!(copied[0].depth, INKPOD_COLOR_DEPTH_8);
        assert_eq!(copied[0].red, 12);
        assert_eq!(copied[1].depth, INKPOD_COLOR_DEPTH_16);
        assert_eq!(copied[1].blue, 32_769);

        let mut sixteen_bit_fill = fill;
        sixteen_bit_fill.color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_16,
            red: 1,
            green: 257,
            blue: 32_769,
            alpha: 65_534,
        };
        let checksum_before_invalid = info.color_plane_checksum;
        assert_eq!(
            inkpod_core_apply_fill(core, &sixteen_bit_fill, &mut result),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.color_plane_checksum, checksum_before_invalid);

        let path = std::env::temp_dir().join(format!(
            "inkpod-ffi-test-recovery-{}-{}.inkpod",
            std::process::id(),
            info.document_revision
        ));
        let path_text = path.to_str().unwrap().as_bytes();
        assert_eq!(
            inkpod_core_autosave(core, path_text.as_ptr(), path_text.len() as u64, &mut info,),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);

        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_open_recovery(core, path_text.as_ptr(), path_text.len() as u64, &mut info,),
            INKPOD_STATUS_OK
        );
        assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_RECOVERED, 0);
        assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        let mut recovered_palette = [InkpodColorValue::default(); 2];
        let mut recovered_buffer = InkpodColorBuffer {
            struct_size: size_of::<InkpodColorBuffer>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            colors: recovered_palette.as_mut_ptr(),
            color_capacity: recovered_palette.len() as u64,
            color_stride_bytes: size_of::<InkpodColorValue>() as u64,
            color_count: 0,
        };
        assert_eq!(
            inkpod_core_palette_get(core, &mut recovered_buffer),
            INKPOD_STATUS_OK
        );
        assert_eq!(recovered_palette[1].depth, INKPOD_COLOR_DEPTH_16);
        assert_eq!(recovered_palette[1].alpha, 65_534);
        assert_eq!(
            inkpod_core_revert(core, &mut info),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn typed_tree_selection_clipboard_view_and_multiview_abi_are_connected() {
    unsafe {
        let mut core = ptr::null_mut();
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        let create = |width, height, uuid_low| InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 0x494e_4b50_4f44_4d33,
            document_uuid_low: uuid_low,
            width,
            height,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut info = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_new_cell(core, &create(8, 8, 1), &mut info),
            INKPOD_STATUS_OK
        );
        let base_layer = info.layer_id;
        let mut result = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        let mut object_id = 0;
        let mut edit = InkpodTreeEdit {
            struct_size: size_of::<InkpodTreeEdit>() as u32,
            operation: INKPOD_TREE_DUPLICATE_LAYER,
            flags: 0,
            object_id: base_layer,
            parent_id: 0,
            destination_index: 0,
            kind: 0,
            pixel_format: 0,
            opacity_milli: 0,
            name_utf8: ptr::null(),
            name_bytes: 0,
        };
        assert_eq!(
            inkpod_core_tree_edit(core, &edit, &mut result, &mut object_id),
            INKPOD_STATUS_OK
        );
        let duplicate = object_id;
        assert_ne!(duplicate, 0);
        edit.operation = INKPOD_TREE_REORDER_LAYER;
        edit.object_id = duplicate;
        edit.destination_index = 0;
        assert_eq!(
            inkpod_core_tree_edit(core, &edit, &mut result, &mut object_id),
            INKPOD_STATUS_OK
        );
        let mut node = InkpodNodeInfo {
            struct_size: size_of::<InkpodNodeInfo>() as u32,
            ..InkpodNodeInfo::default()
        };
        assert_eq!(
            inkpod_core_node_get(core, 0, u32::MAX, &mut node),
            INKPOD_STATUS_OK
        );
        assert_eq!(node.id, duplicate);
        assert_eq!(node.child_count, 2);
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_OK
        );
        let revision_before_plane_validation = info.document_revision;
        assert_eq!(
            inkpod_core_validate_plane_creation(
                ptr::null_mut(),
                base_layer,
                INKPOD_TYPED_PLANE_RASTER,
                INKPOD_STORAGE_RGBA8,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_validate_plane_creation(
                core,
                base_layer,
                INKPOD_TYPED_PLANE_RASTER,
                INKPOD_STORAGE_RGBA8,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_validate_plane_creation(
                core,
                base_layer,
                INKPOD_TYPED_PLANE_SELECTION,
                INKPOD_STORAGE_BINARY8,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, revision_before_plane_validation);
        let revision_before_invalid_tree = result.revision;
        let invalid_name = b"Invalid selection";
        let invalid_plane = InkpodTreeEdit {
            operation: INKPOD_TREE_CREATE_PLANE,
            parent_id: base_layer,
            kind: INKPOD_TYPED_PLANE_SELECTION,
            pixel_format: INKPOD_STORAGE_BINARY8,
            name_utf8: invalid_name.as_ptr(),
            name_bytes: invalid_name.len() as u64,
            ..edit
        };
        assert_eq!(
            inkpod_core_tree_edit(core, &invalid_plane, &mut result, &mut object_id),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(result.revision, revision_before_invalid_tree);
        edit.operation = INKPOD_TREE_DELETE_LAYER;
        assert_eq!(
            inkpod_core_tree_edit(core, &edit, &mut result, &mut object_id),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_undo(core, &mut result), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_redo(core, &mut result), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_undo(core, &mut result), INKPOD_STATUS_OK);

        assert_eq!(
            inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR),
            INKPOD_STATUS_OK
        );
        let sample = InkpodStrokeSample {
            struct_size: size_of::<InkpodStrokeSample>() as u32,
            flags: 0,
            x: 6.0,
            y: 6.0,
            pressure: 1.0,
            reserved: 0,
        };
        let stroke = InkpodStrokeInput {
            struct_size: size_of::<InkpodStrokeInput>() as u32,
            tool: INKPOD_TOOL_PENCIL,
            plane: INKPOD_PLANE_COLOR,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            flags: 0,
            color_rgba: 0x0c22_38ff,
            diameter: 1.0,
            samples: &sample,
            sample_count: 1,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        assert_eq!(
            inkpod_core_apply_stroke(core, &stroke, &mut result),
            INKPOD_STATUS_OK
        );
        let selected_color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_8,
            red: 12,
            green: 34,
            blue: 56,
            alpha: 255,
        };
        assert_eq!(
            inkpod_core_select_color(
                core,
                &selected_color,
                0,
                0,
                INKPOD_SELECTION_NEW,
                &mut result
            ),
            INKPOD_STATUS_OK
        );
        let selection = InkpodSelectionInput {
            struct_size: size_of::<InkpodSelectionInput>() as u32,
            shape: INKPOD_SELECTION_RECTANGLE,
            operation: INKPOD_SELECTION_NEW,
            reserved: 0,
            bounds: InkpodFrameRect {
                x: 6,
                y: 6,
                width: 1,
                height: 1,
            },
            points: ptr::null(),
            point_count: 0,
            point_stride_bytes: 0,
            diameter: 0.0,
            tolerance: 0,
            gap_close: 0,
            seed_x: 0,
            seed_y: 0,
        };
        assert_eq!(
            inkpod_core_apply_selection(core, &selection, &mut result),
            INKPOD_STATUS_OK
        );
        let invalid_point_free = InkpodSelectionInput {
            point_stride_bytes: size_of::<InkpodSelectionPoint>() as u64,
            ..selection
        };
        assert_eq!(
            inkpod_core_apply_selection(core, &invalid_point_free, &mut result),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let oversized_point = InkpodSelectionPoint {
            struct_size: (size_of::<InkpodSelectionPoint>() + 8) as u32,
            reserved: 0,
            x: 0.0,
            y: 0.0,
        };
        let invalid_strided_point = InkpodSelectionInput {
            shape: INKPOD_SELECTION_LASSO,
            points: &oversized_point,
            point_count: 1,
            point_stride_bytes: size_of::<InkpodSelectionPoint>() as u64,
            ..selection
        };
        assert_eq!(
            inkpod_core_apply_selection(core, &invalid_strided_point, &mut result),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        let mut clipboard = ptr::null_mut();
        assert_eq!(
            inkpod_core_clipboard_copy(core, &mut clipboard),
            INKPOD_STATUS_OK
        );
        assert!(!clipboard.is_null());

        assert_eq!(
            inkpod_core_new_cell(core, &create(4, 4, 2), &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_paste_begin(core, clipboard), INKPOD_STATUS_OK);
        assert_eq!(inkpod_clipboard_release(&mut clipboard), INKPOD_STATUS_OK);
        assert!(clipboard.is_null());
        let transform = InkpodFloatingTransform {
            struct_size: size_of::<InkpodFloatingTransform>() as u32,
            reserved: 0,
            translate_x: -4.0,
            translate_y: -4.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        };
        assert_eq!(
            inkpod_core_floating_transform(core, &transform),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_floating_commit(core, &mut result),
            INKPOD_STATUS_OK
        );
        let mut color = InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            ..InkpodColorValue::default()
        };
        assert_eq!(
            inkpod_core_eyedropper(core, INKPOD_EYEDROPPER_SELECTED_PLANE, 2, 2, &mut color),
            INKPOD_STATUS_OK
        );
        assert_eq!((color.red, color.green, color.blue), (12, 34, 56));
        assert_eq!(inkpod_core_undo(core, &mut result), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_eyedropper(core, INKPOD_EYEDROPPER_SELECTED_PLANE, 2, 2, &mut color),
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(inkpod_core_redo(core, &mut result), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_eyedropper(core, INKPOD_EYEDROPPER_SELECTED_PLANE, 2, 2, &mut color),
            INKPOD_STATUS_OK
        );
        assert_eq!((color.red, color.green, color.blue), (12, 34, 56));
        assert_eq!(inkpod_clipboard_release(&mut clipboard), INKPOD_STATUS_OK);

        let flip = InkpodViewInput {
            struct_size: size_of::<InkpodViewInput>() as u32,
            kind: INKPOD_VIEW_FLIP_HORIZONTAL,
            flags: 0,
            value1: 0.0,
            value2: 0.0,
            value3: 0.0,
            value4: 0.0,
        };
        assert_eq!(
            inkpod_core_get_document_info(core, &mut info),
            INKPOD_STATUS_OK
        );
        let document_revision = info.document_revision;
        assert_eq!(
            inkpod_core_apply_view(core, &flip, &mut info),
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, document_revision);
        assert_eq!(
            inkpod_core_mirror_document(core, 1, &mut result),
            INKPOD_STATUS_OK
        );
        assert!(result.revision > document_revision);

        let mut view_id = 0;
        assert_eq!(
            inkpod_core_view_create(core, &mut view_id),
            INKPOD_STATUS_OK
        );
        let secondary_pan = InkpodViewInput {
            struct_size: size_of::<InkpodViewInput>() as u32,
            kind: INKPOD_VIEW_PAN_BY,
            flags: 0,
            value1: 5.0,
            value2: 0.0,
            value3: 0.0,
            value4: 0.0,
        };
        assert_eq!(
            inkpod_core_view_apply(core, view_id, &secondary_pan),
            INKPOD_STATUS_OK
        );
        let snapshot_options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
        };
        let mut primary = ptr::null_mut();
        let mut secondary = ptr::null_mut();
        assert_eq!(
            inkpod_core_build_snapshot(core, &snapshot_options, &mut primary),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_build_snapshot_for_view(core, view_id, &snapshot_options, &mut secondary),
            INKPOD_STATUS_OK
        );
        let mut primary_view = InkpodSnapshotView {
            struct_size: size_of::<InkpodSnapshotView>() as u32,
            abi_version: 0,
            feature_flags: 0,
            revision: 0,
            tiles: ptr::null(),
            tile_count: 0,
            tile_stride_bytes: 0,
        };
        let mut secondary_view = InkpodSnapshotView { ..primary_view };
        let mut primary_transform = InkpodSnapshotTransform {
            struct_size: size_of::<InkpodSnapshotTransform>() as u32,
            ..InkpodSnapshotTransform::default()
        };
        let mut secondary_transform = InkpodSnapshotTransform {
            struct_size: size_of::<InkpodSnapshotTransform>() as u32,
            ..InkpodSnapshotTransform::default()
        };
        assert_eq!(
            inkpod_snapshot_get_view(primary, &mut primary_view),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_snapshot_get_view(secondary, &mut secondary_view),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_snapshot_get_transform(primary, &mut primary_transform),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_snapshot_get_transform(secondary, &mut secondary_transform),
            INKPOD_STATUS_OK
        );
        assert_eq!(primary_view.revision, secondary_view.revision);
        assert_ne!(primary_transform.pan_x, secondary_transform.pan_x);
        assert_eq!(inkpod_snapshot_release(&mut primary), INKPOD_STATUS_OK);
        assert_eq!(inkpod_snapshot_release(&mut secondary), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_view_close(core, view_id), INKPOD_STATUS_OK);

        let mut guide_id = 0;
        assert_eq!(
            inkpod_core_guide_add(core, INKPOD_GUIDE_VERTICAL, 2, &mut result, &mut guide_id),
            INKPOD_STATUS_OK
        );
        assert_ne!(guide_id, 0);
        assert_eq!(
            inkpod_core_guide_move(core, guide_id, 3, &mut result),
            INKPOD_STATUS_OK
        );
        let mut second_guide_id = 0;
        assert_eq!(
            inkpod_core_guide_add(
                core,
                INKPOD_GUIDE_HORIZONTAL,
                1,
                &mut result,
                &mut second_guide_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_ne!(guide_id, second_guide_id);
        let grid = InkpodGridInput {
            struct_size: size_of::<InkpodGridInput>() as u32,
            reserved: 0,
            origin_x: 0,
            origin_y: 0,
            spacing_x: 4,
            spacing_y: 4,
            subdivisions: 2,
            flags: 0,
        };
        assert_eq!(
            inkpod_core_grid_set(core, &grid, &mut result),
            INKPOD_STATUS_OK
        );
        let show_grid = InkpodViewInput {
            struct_size: size_of::<InkpodViewInput>() as u32,
            kind: INKPOD_VIEW_SET_GRID_VISIBLE,
            flags: 0,
            value1: 1.0,
            value2: 0.0,
            value3: 0.0,
            value4: 0.0,
        };
        assert_eq!(
            inkpod_core_apply_view(core, &show_grid, &mut info),
            INKPOD_STATUS_OK
        );
        let mut overlay_snapshot = ptr::null_mut();
        assert_eq!(
            inkpod_core_build_snapshot(core, &snapshot_options, &mut overlay_snapshot),
            INKPOD_STATUS_OK
        );
        let mut overlay = InkpodSnapshotOverlay {
            struct_size: size_of::<InkpodSnapshotOverlay>() as u32,
            ..InkpodSnapshotOverlay::default()
        };
        assert_eq!(
            inkpod_snapshot_get_overlay(overlay_snapshot, &mut overlay),
            INKPOD_STATUS_OK
        );
        assert_ne!(overlay.flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE, 0);
        assert_eq!((overlay.grid_spacing_x, overlay.grid_subdivisions), (4, 2));
        assert_eq!(overlay.guide_count, 2);
        assert!(!overlay.guides.is_null());
        let guide_ids = std::slice::from_raw_parts(overlay.guides, overlay.guide_count as usize);
        assert!(guide_ids.iter().any(|guide| guide.id == guide_id));
        assert!(guide_ids.iter().any(|guide| guide.id == second_guide_id));
        assert_eq!(
            inkpod_snapshot_release(&mut overlay_snapshot),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_snapshot_release(&mut overlay_snapshot),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_guide_delete(core, guide_id, &mut result),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_guide_delete_all(core, &mut result),
            INKPOD_STATUS_OK
        );
        let mut locator = InkpodLocatorOutput {
            struct_size: size_of::<InkpodLocatorOutput>() as u32,
            ..InkpodLocatorOutput::default()
        };
        assert_eq!(
            inkpod_core_locator_sample(core, 0, 1.0, 1.0, &mut locator),
            INKPOD_STATUS_OK
        );
        assert_eq!((locator.document_x, locator.document_y), (3, 1));
        let mut neighborhood = InkpodLocatorNeighborhoodBuffer {
            struct_size: size_of::<InkpodLocatorNeighborhoodBuffer>() as u32,
            radius: 1,
            ..InkpodLocatorNeighborhoodBuffer::default()
        };
        assert_eq!(
            inkpod_core_locator_neighborhood(core, 0, 1.0, 1.0, &mut neighborhood),
            INKPOD_STATUS_OK
        );
        assert_eq!((neighborhood.width, neighborhood.height), (3, 3));
        assert_eq!(neighborhood.required_bytes, 36);
        let mut short = [0_u8; 35];
        neighborhood.pixels_rgba8 = short.as_mut_ptr();
        neighborhood.pixel_capacity = short.len() as u64;
        assert_eq!(
            inkpod_core_locator_neighborhood(core, 0, 1.0, 1.0, &mut neighborhood),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        let mut pixels = [0_u8; 36];
        neighborhood.pixels_rgba8 = pixels.as_mut_ptr();
        neighborhood.pixel_capacity = pixels.len() as u64;
        assert_eq!(
            inkpod_core_locator_neighborhood(core, 0, 1.0, 1.0, &mut neighborhood),
            INKPOD_STATUS_OK
        );
        assert_eq!(neighborhood.required_bytes, pixels.len() as u64);
        neighborhood.radius = 17;
        assert_eq!(
            inkpod_core_locator_neighborhood(core, 0, 1.0, 1.0, &mut neighborhood),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            inkpod_core_shortcut_rebind(
                core,
                99,
                u32::from(b'Z'),
                INKPOD_SHORTCUT_MODIFIER_CONTROL
            ),
            INKPOD_STATUS_OK
        );
        let mut shortcut_command = 0;
        assert_eq!(
            inkpod_core_shortcut_resolve(
                core,
                u32::from(b'Z'),
                INKPOD_SHORTCUT_MODIFIER_CONTROL,
                &mut shortcut_command
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(shortcut_command, 99);
        shortcut_command = 123;
        assert_eq!(
            inkpod_core_shortcut_resolve(core, u32::from(b'Z'), 0x10, &mut shortcut_command),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(shortcut_command, 0);
        assert_eq!(inkpod_core_shortcut_reset(core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_shortcut_resolve(
                core,
                u32::from(b'Z'),
                INKPOD_SHORTCUT_MODIFIER_CONTROL,
                &mut shortcut_command
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(shortcut_command, 1);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn multi_stroke_shortcut_table_copies_resolves_and_rejects_conflicts() {
    unsafe {
        let mut core = ptr::null_mut();
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        let stroke = |key, modifiers| InkpodShortcutStroke {
            virtual_key: u32::from(key),
            modifiers,
        };
        let sequence = |command_id, strokes: &[InkpodShortcutStroke]| {
            let mut value = InkpodShortcutSequence {
                struct_size: size_of::<InkpodShortcutSequence>() as u32,
                command_id,
                stroke_count: strokes.len() as u32,
                ..InkpodShortcutSequence::default()
            };
            value.strokes[..strokes.len()].copy_from_slice(strokes);
            value
        };
        let defaults = [
            sequence(100, &[stroke(b'Q', 0), stroke(b'F', 0), stroke(b'A', 0)]),
            sequence(101, &[stroke(b'Q', 0), stroke(b'F', 0), stroke(b'B', 0)]),
            sequence(102, &[stroke(b'S', INKPOD_SHORTCUT_MODIFIER_CONTROL)]),
        ];
        assert_eq!(
            inkpod_core_shortcut_defaults_set(
                core,
                defaults.as_ptr(),
                defaults.len() as u64,
                size_of::<InkpodShortcutSequence>() as u64,
            ),
            INKPOD_STATUS_OK
        );

        let mut required = 0;
        assert_eq!(
            inkpod_core_shortcut_sequences_copy(core, ptr::null_mut(), 0, 0, &mut required),
            INKPOD_STATUS_OK
        );
        assert_eq!(required, defaults.len() as u64);
        let mut copied = [InkpodShortcutSequence::default(); 3];
        assert_eq!(
            inkpod_core_shortcut_sequences_copy(
                core,
                copied.as_mut_ptr(),
                copied.len() as u64,
                size_of::<InkpodShortcutSequence>() as u64,
                &mut required,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(copied[1].command_id, 101);

        let mut match_kind = INKPOD_SHORTCUT_MATCH_NONE;
        let mut command_id = 999;
        let prefix = [stroke(b'Q', 0), stroke(b'F', 0)];
        assert_eq!(
            inkpod_shortcut_sequence_resolve(
                copied.as_ptr(),
                copied.len() as u64,
                size_of::<InkpodShortcutSequence>() as u64,
                prefix.as_ptr(),
                prefix.len() as u32,
                &mut match_kind,
                &mut command_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(match_kind, INKPOD_SHORTCUT_MATCH_PREFIX);
        assert_eq!(command_id, 0);
        let exact = [stroke(b'Q', 0), stroke(b'F', 0), stroke(b'B', 0)];
        assert_eq!(
            inkpod_shortcut_sequence_resolve(
                copied.as_ptr(),
                copied.len() as u64,
                size_of::<InkpodShortcutSequence>() as u64,
                exact.as_ptr(),
                exact.len() as u32,
                &mut match_kind,
                &mut command_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(match_kind, INKPOD_SHORTCUT_MATCH_EXACT);
        assert_eq!(command_id, 101);
        let alignment = align_of::<InkpodShortcutSequence>();
        let overflowing_stride = ((isize::MAX as usize / alignment) + 1) * alignment;
        assert_eq!(
            inkpod_shortcut_sequence_resolve(
                copied.as_ptr(),
                2,
                overflowing_stride as u64,
                exact.as_ptr(),
                exact.len() as u32,
                &mut match_kind,
                &mut command_id,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let conflicting = [
            sequence(200, &[stroke(b'Q', 0), stroke(b'F', 0)]),
            sequence(201, &[stroke(b'Q', 0), stroke(b'F', 0), stroke(b'C', 0)]),
        ];
        assert_eq!(
            inkpod_core_shortcut_sequences_set(
                core,
                conflicting.as_ptr(),
                conflicting.len() as u64,
                size_of::<InkpodShortcutSequence>() as u64,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(inkpod_core_shortcut_reset(core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn vector_commands_snapshot_and_nested_span_validation_are_connected() {
    unsafe {
        let mut core = ptr::null_mut();
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        let create = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 0x494e_4b50_4f44_4d35,
            document_uuid_low: 1,
            width: 8,
            height: 8,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut info = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_new_cell(core, &create, &mut info),
            INKPOD_STATUS_OK
        );
        let name = b"Vector";
        let edit = InkpodTreeEdit {
            struct_size: size_of::<InkpodTreeEdit>() as u32,
            operation: INKPOD_TREE_CREATE_LAYER,
            flags: 0,
            object_id: 0,
            parent_id: 0,
            destination_index: 0,
            kind: INKPOD_LAYER_VECTOR_COLORING,
            pixel_format: 0,
            opacity_milli: 0,
            name_utf8: name.as_ptr(),
            name_bytes: name.len() as u64,
        };
        let mut dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        let mut layer_id = 0;
        assert_eq!(
            inkpod_core_tree_edit(core, &edit, &mut dispatch, &mut layer_id),
            INKPOD_STATUS_OK
        );
        assert_ne!(layer_id, 0);
        let mut node = InkpodNodeInfo {
            struct_size: size_of::<InkpodNodeInfo>() as u32,
            ..InkpodNodeInfo::default()
        };
        assert_eq!(
            inkpod_core_node_get(core, 1, 1, &mut node),
            INKPOD_STATUS_OK
        );
        assert_eq!(node.kind, INKPOD_TYPED_PLANE_COLOR_TRACE);
        let trace_plane_id = node.id;
        assert_eq!(
            inkpod_core_node_get(core, 1, 2, &mut node),
            INKPOD_STATUS_OK
        );
        assert_eq!(node.kind, INKPOD_TYPED_PLANE_VECTOR_FILL);
        let fill_plane_id = node.id;

        let point = |x, y| InkpodVectorPoint { x, y };
        let line = |p0: InkpodVectorPoint, p3: InkpodVectorPoint| InkpodVectorCubicSegment {
            struct_size: size_of::<InkpodVectorCubicSegment>() as u32,
            reserved: 0,
            p0,
            p1: point((p0.x * 2.0 + p3.x) / 3.0, (p0.y * 2.0 + p3.y) / 3.0),
            p2: point((p0.x + p3.x * 2.0) / 3.0, (p0.y + p3.y * 2.0) / 3.0),
            p3,
            width_start: 1.0,
            width_end: 2.0,
        };
        let corners = [
            point(1.0, 1.0),
            point(7.0, 1.0),
            point(7.0, 7.0),
            point(1.0, 7.0),
            point(1.0, 1.0),
        ];
        let segments: Vec<_> = corners
            .windows(2)
            .map(|pair| line(pair[0], pair[1]))
            .collect();
        let path_input = InkpodVectorPathInput {
            struct_size: size_of::<InkpodVectorPathInput>() as u32,
            reserved: 0,
            flags: INKPOD_VECTOR_PATH_CLOSED,
            plane_id: trace_plane_id,
            color: InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_8,
                red: 10,
                green: 20,
                blue: 30,
                alpha: 255,
            },
            segments: segments.as_ptr(),
            segment_count: segments.len() as u64,
            segment_stride_bytes: size_of::<InkpodVectorCubicSegment>() as u64,
        };
        let mut path_id = 0;
        assert_eq!(
            inkpod_core_vector_add_path(core, &path_input, &mut dispatch, &mut path_id),
            INKPOD_STATUS_OK
        );
        assert_ne!(path_id, 0);
        let boundary_path_id = path_id;
        let mut short_segment = segments[0];
        short_segment.struct_size = size_of::<u32>() as u32;
        let short_input = InkpodVectorPathInput {
            segments: &short_segment,
            segment_count: 1,
            ..path_input
        };
        let revision = dispatch.revision;
        let mut rejected_path_id = u64::MAX;
        assert_eq!(
            inkpod_core_vector_add_path(core, &short_input, &mut dispatch, &mut rejected_path_id,),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(rejected_path_id, 0);
        assert_eq!(dispatch.revision, revision);

        let mut too_thin_segments = segments.clone();
        too_thin_segments[0].width_start = 0.0001;
        too_thin_segments[0].width_end = 0.0001;
        let too_thin_input = InkpodVectorPathInput {
            segments: too_thin_segments.as_ptr(),
            ..path_input
        };
        rejected_path_id = u64::MAX;
        assert_eq!(
            inkpod_core_vector_add_path(
                core,
                &too_thin_input,
                &mut dispatch,
                &mut rejected_path_id,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(rejected_path_id, 0);
        assert_eq!(dispatch.revision, revision);

        let fill_input = InkpodVectorFillInput {
            struct_size: size_of::<InkpodVectorFillInput>() as u32,
            reserved: 0,
            feature_flags: 0,
            plane_id: fill_plane_id,
            color: InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_16,
                red: 60_000,
                green: 1_000,
                blue: 2_000,
                alpha: 50_000,
            },
            boundary_path_ids: &boundary_path_id,
            boundary_path_count: 1,
        };
        let mut fill_id = 0;
        assert_eq!(
            inkpod_core_vector_add_fill(core, &fill_input, &mut dispatch, &mut fill_id),
            INKPOD_STATUS_OK
        );
        assert_ne!(fill_id, 0);

        let selection_input = InkpodVectorSelectionInput {
            struct_size: size_of::<InkpodVectorSelectionInput>() as u32,
            mode: INKPOD_VECTOR_SELECT_FULLY_CONTAINED,
            feature_flags: 0,
            bounds: InkpodFrameRect {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
        };
        let mut selection_output = InkpodVectorSelectionBuffer {
            struct_size: size_of::<InkpodVectorSelectionBuffer>() as u32,
            reserved: 0,
            ranges: ptr::null_mut(),
            range_capacity: 0,
            range_count: 0,
            fill_ids: ptr::null_mut(),
            fill_capacity: 0,
            fill_count: 0,
        };
        assert_eq!(
            inkpod_core_vector_select(core, &selection_input, &mut selection_output),
            INKPOD_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(selection_output.range_count, 1);
        let mut selection_ranges = [InkpodVectorSelectionRange {
            struct_size: 0,
            reserved: u32::MAX,
            path_id: 0,
            start_million: u32::MAX,
            end_million: 0,
        }];
        selection_output.ranges = selection_ranges.as_mut_ptr();
        selection_output.range_capacity = selection_ranges.len() as u64;
        assert_eq!(
            inkpod_core_vector_select(core, &selection_input, &mut selection_output),
            INKPOD_STATUS_OK
        );
        assert_eq!(selection_ranges[0].path_id, path_id);
        assert_eq!(selection_ranges[0].start_million, 0);
        assert_eq!(selection_ranges[0].end_million, 1_000_000);

        let rasterize_input = InkpodVectorRasterizeInput {
            struct_size: size_of::<InkpodVectorRasterizeInput>() as u32,
            reserved: 0,
            feature_flags: INKPOD_VECTOR_RASTERIZE_ANTIALIAS,
            layer_id,
            scale: 2,
            reserved_2: 0,
        };
        let mut raster_output = InkpodVectorRasterBuffer {
            struct_size: size_of::<InkpodVectorRasterBuffer>() as u32,
            reserved: 0,
            pixels: ptr::null_mut(),
            pixel_capacity: 0,
            required_bytes: 0,
            width: 0,
            height: 0,
            stride_bytes: 0,
            reserved_2: 0,
        };
        assert_eq!(
            inkpod_core_vector_rasterize(core, &rasterize_input, &mut raster_output),
            INKPOD_STATUS_OK
        );
        assert_eq!((raster_output.width, raster_output.height), (16, 16));
        assert_eq!(raster_output.required_bytes, 16 * 16 * 4);
        let mut raster_pixels = vec![0_u8; raster_output.required_bytes as usize];
        raster_output.pixels = raster_pixels.as_mut_ptr();
        raster_output.pixel_capacity = raster_pixels.len() as u64;
        assert_eq!(
            inkpod_core_vector_rasterize(core, &rasterize_input, &mut raster_output),
            INKPOD_STATUS_OK
        );
        assert!(raster_pixels.iter().any(|value| *value != 0));

        let rasterize_layer_input = InkpodVectorRasterizeInput {
            scale: 1,
            ..rasterize_input
        };
        let rasterized_name = b"Rasterized";
        let mut raster_layer_id = 0_u64;
        assert_eq!(
            inkpod_core_vector_rasterize_to_layer(
                core,
                &rasterize_layer_input,
                rasterized_name.as_ptr(),
                rasterized_name.len() as u64,
                &mut dispatch,
                &mut raster_layer_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_ne!(raster_layer_id, 0);
        assert_eq!(dispatch.accepted_command_count, 1);

        assert_eq!(
            inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR),
            INKPOD_STATUS_OK
        );
        let sample = InkpodStrokeSample {
            struct_size: size_of::<InkpodStrokeSample>() as u32,
            flags: 0,
            x: 3.0,
            y: 3.0,
            pressure: 1.0,
            reserved: 0,
        };
        let stroke = InkpodStrokeInput {
            struct_size: size_of::<InkpodStrokeInput>() as u32,
            tool: INKPOD_TOOL_PENCIL,
            plane: INKPOD_PLANE_COLOR,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            flags: 0,
            color_rgba: 0x0102_03ff,
            diameter: 1.0,
            samples: &sample,
            sample_count: 1,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        assert_eq!(
            inkpod_core_apply_stroke(core, &stroke, &mut dispatch),
            INKPOD_STATUS_OK
        );
        let vectorize_input = InkpodRasterVectorizeInput {
            struct_size: size_of::<InkpodRasterVectorizeInput>() as u32,
            alpha_threshold: 1,
            feature_flags: 0,
            source_plane_id: info.color_plane_id,
            target_layer_id: layer_id,
        };
        let mut vectorized_fill_count = 0;
        let vector_source_input = InkpodRasterVectorizeInput {
            source_plane_id: trace_plane_id,
            ..vectorize_input
        };
        assert_eq!(
            inkpod_core_raster_vectorize(
                core,
                &vector_source_input,
                &mut dispatch,
                &mut vectorized_fill_count,
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(vectorized_fill_count, 0);
        assert_eq!(
            inkpod_core_raster_vectorize(
                core,
                &vectorize_input,
                &mut dispatch,
                &mut vectorized_fill_count,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(vectorized_fill_count, 1);

        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
        };
        let mut snapshot = ptr::null_mut();
        assert_eq!(
            inkpod_core_build_snapshot(core, &options, &mut snapshot),
            INKPOD_STATUS_OK
        );
        let mut vectors = InkpodSnapshotVectorView {
            struct_size: size_of::<InkpodSnapshotVectorView>() as u32,
            abi_version: 0,
            feature_flags: u64::MAX,
            segments: ptr::null(),
            segment_count: 0,
            segment_stride_bytes: 0,
            fills: ptr::null(),
            fill_count: 0,
            fill_stride_bytes: 0,
            boundary_path_ids: ptr::null(),
            boundary_path_count: 0,
        };
        assert_eq!(
            inkpod_snapshot_get_vectors(snapshot, &mut vectors),
            INKPOD_STATUS_OK
        );
        assert_eq!(vectors.abi_version, INKPOD_ABI_VERSION);
        assert_eq!(vectors.segment_count, 8);
        assert_eq!(vectors.fill_count, 2);
        assert_eq!(vectors.boundary_path_count, 2);
        assert!(!vectors.segments.is_null() && !vectors.fills.is_null());
        assert_eq!((*vectors.segments).path_id, boundary_path_id);
        assert_eq!((*vectors.fills).fill_id, fill_id);
        assert_eq!(*vectors.boundary_path_ids, boundary_path_id);
        assert_eq!(inkpod_snapshot_release(&mut snapshot), INKPOD_STATUS_OK);
        let atomic_new_vector_layer = InkpodRasterVectorizeInput {
            target_layer_id: 0,
            ..vectorize_input
        };
        let mut history_before_atomic_vectorize = InkpodHistoryInfo {
            struct_size: size_of::<InkpodHistoryInfo>() as u32,
            reserved: 0,
            cursor: 0,
            item_count: 0,
        };
        assert_eq!(
            inkpod_core_history_info(core, &mut history_before_atomic_vectorize),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_raster_vectorize(
                core,
                &atomic_new_vector_layer,
                &mut dispatch,
                &mut vectorized_fill_count,
            ),
            INKPOD_STATUS_OK
        );
        assert_eq!(vectorized_fill_count, 1);
        let mut history_after_atomic_vectorize = InkpodHistoryInfo {
            struct_size: size_of::<InkpodHistoryInfo>() as u32,
            reserved: 0,
            cursor: 0,
            item_count: 0,
        };
        assert_eq!(
            inkpod_core_history_info(core, &mut history_after_atomic_vectorize),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            history_after_atomic_vectorize.item_count,
            history_before_atomic_vectorize.item_count + 1
        );
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn filter_effect_adjustment_and_alpha_records_are_copied_and_atomic() {
    unsafe {
        let config = InkpodCoreConfig {
            struct_size: size_of::<InkpodCoreConfig>() as u32,
            abi_version: INKPOD_ABI_VERSION,
            feature_flags: 0,
        };
        let mut core = ptr::null_mut();
        assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
        let options = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 0x4d36_0000_0000_0001,
            document_uuid_low: 0x4d36_0000_0000_0002,
            width: 4,
            height: 4,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut document = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut document),
            INKPOD_STATUS_OK
        );
        let original = document.color_plane_checksum;
        let mut filter = InkpodFilterInput {
            struct_size: size_of::<InkpodFilterInput>() as u32,
            kind: INKPOD_FILTER_INVERT,
            feature_flags: 0,
            plane_id: document.color_plane_id,
            channel: INKPOD_FILTER_CHANNEL_RGB,
            interpolation: 0,
            parameter_0: 0,
            parameter_1: 0,
            parameter_2: 0,
            parameter_3: 0,
            parameter_4: 0,
            point_stride_bytes: 0,
            points: ptr::null(),
            point_count: 0,
        };
        let mut preview = InkpodFilterPreviewInfo {
            struct_size: size_of::<InkpodFilterPreviewInfo>() as u32,
            reserved: 0,
            plane_id: 0,
            base_checksum: 0,
            preview_checksum: 0,
            preview_revision: 0,
        };
        let mut short = filter;
        short.struct_size = size_of::<u32>() as u32;
        assert_eq!(
            inkpod_core_filter_preview_begin(core, &short, &mut preview),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(
            inkpod_core_filter_preview_begin(core, &filter, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(preview.base_checksum, original);
        assert_ne!(preview.preview_checksum, original);
        assert_eq!(
            inkpod_core_filter_preview_cancel(core, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(preview.preview_checksum, original);
        assert_eq!(
            inkpod_core_filter_preview_begin(core, &filter, &mut preview),
            INKPOD_STATUS_OK
        );
        let mut dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        assert_eq!(
            inkpod_core_filter_preview_apply(core, &mut dispatch),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_undo(core, &mut dispatch), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        assert_eq!(document.color_plane_checksum, original);

        let curve_points = [
            InkpodCurvePoint {
                struct_size: size_of::<InkpodCurvePoint>() as u32,
                reserved: 0,
                input: 0,
                output: 0,
            },
            InkpodCurvePoint {
                struct_size: size_of::<InkpodCurvePoint>() as u32,
                reserved: 0,
                input: 32_768,
                output: 40_000,
            },
            InkpodCurvePoint {
                struct_size: size_of::<InkpodCurvePoint>() as u32,
                reserved: 0,
                input: 65_535,
                output: 65_535,
            },
        ];
        filter.kind = INKPOD_FILTER_TONE_CURVE;
        filter.channel = INKPOD_FILTER_CHANNEL_RGB;
        filter.interpolation = INKPOD_CURVE_BEZIER;
        filter.points = curve_points.as_ptr();
        filter.point_count = curve_points.len() as u64;
        filter.point_stride_bytes = (size_of::<InkpodCurvePoint>() - 1) as u32;
        assert_eq!(
            inkpod_core_filter_preview_begin(core, &filter, &mut preview),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        filter.point_stride_bytes = size_of::<InkpodCurvePoint>() as u32;
        let mut oversized_points = curve_points;
        oversized_points[0].struct_size = (size_of::<InkpodCurvePoint>() + 8) as u32;
        filter.points = oversized_points.as_ptr();
        assert_eq!(
            inkpod_core_filter_preview_begin(core, &filter, &mut preview),
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        filter.points = curve_points.as_ptr();
        assert_eq!(
            inkpod_core_filter_preview_begin(core, &filter, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_filter_preview_cancel(core, &mut preview),
            INKPOD_STATUS_OK
        );
        filter.point_stride_bytes = 0;
        assert_eq!(
            inkpod_core_filter_preview_begin(core, &filter, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_filter_preview_cancel(core, &mut preview),
            INKPOD_STATUS_OK
        );

        filter.kind = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
        filter.interpolation = 0;
        filter.parameter_0 = 100;
        filter.parameter_1 = 200;
        filter.points = ptr::null();
        filter.point_count = 0;
        filter.point_stride_bytes = 0;
        let name = b"Adjustment";
        let mut layer_id = 0;
        assert_eq!(
            inkpod_core_adjustment_create(
                core,
                &filter,
                name.as_ptr(),
                name.len() as u64,
                &mut dispatch,
                &mut layer_id,
            ),
            INKPOD_STATUS_OK
        );
        assert_ne!(layer_id, 0);

        filter.parameter_0 = 200;
        filter.parameter_1 = -100;
        assert_eq!(
            inkpod_core_adjustment_update(core, layer_id, &filter, &mut dispatch),
            INKPOD_STATUS_OK
        );

        let color16 = |red, green, blue, alpha| InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_16,
            red,
            green,
            blue,
            alpha,
        };
        let stops = [
            InkpodGradientStop {
                struct_size: size_of::<InkpodGradientStop>() as u32,
                reserved: 0,
                position_milli: 0,
                reserved_2: 0,
                color: color16(65_535, 0, 0, 65_535),
            },
            InkpodGradientStop {
                struct_size: size_of::<InkpodGradientStop>() as u32,
                reserved: 0,
                position_milli: 500,
                reserved_2: 0,
                color: color16(0, 65_535, 0, 32_768),
            },
            InkpodGradientStop {
                struct_size: size_of::<InkpodGradientStop>() as u32,
                reserved: 0,
                position_milli: 1_000,
                reserved_2: 0,
                color: color16(0, 0, 65_535, 65_535),
            },
        ];
        let gradient = InkpodGradientInput {
            struct_size: size_of::<InkpodGradientInput>() as u32,
            kind: INKPOD_GRADIENT_LINEAR,
            feature_flags: 0,
            plane_id: document.color_plane_id,
            mode: INKPOD_GRADIENT_OVERWRITE,
            dither: 0,
            start_x_milli: 500,
            start_y_milli: 500,
            end_x_milli: 3_500,
            end_y_milli: 500,
            stops: stops.as_ptr(),
            stop_count: stops.len() as u64,
            stop_stride_bytes: size_of::<InkpodGradientStop>() as u64,
        };
        assert_eq!(
            inkpod_core_effect_gradient(core, &gradient, &mut dispatch),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        assert_ne!(document.color_plane_checksum, original);

        let airbrush = InkpodAirbrushInput {
            struct_size: size_of::<InkpodAirbrushInput>() as u32,
            reserved: 0,
            feature_flags: 0,
            plane_id: document.color_plane_id,
            center_x_milli: 2_000,
            center_y_milli: 2_000,
            radius_milli: 1_500,
            hardness_milli: 500,
            opacity_milli: 500,
            reserved_2: 0,
            color: color16(65_535, 65_535, 65_535, 65_535),
        };
        assert_eq!(
            inkpod_core_effect_airbrush(core, &airbrush, &mut dispatch),
            INKPOD_STATUS_OK
        );

        let boundary_colors = [color16(65_535, 0, 0, 65_535), color16(0, 0, 65_535, 65_535)];
        let boundary = InkpodBoundaryAirbrushInput {
            struct_size: size_of::<InkpodBoundaryAirbrushInput>() as u32,
            reserved: 0,
            feature_flags: 0,
            plane_id: document.color_plane_id,
            width: 1,
            strength_milli: 1_000,
            colors: InkpodColorArray {
                struct_size: size_of::<InkpodColorArray>() as u32,
                reserved: 0,
                feature_flags: 0,
                colors: boundary_colors.as_ptr(),
                color_count: boundary_colors.len() as u64,
                color_stride_bytes: size_of::<InkpodColorValue>() as u64,
            },
        };
        assert_eq!(
            inkpod_core_effect_boundary_airbrush(core, &boundary, &mut dispatch),
            INKPOD_STATUS_OK
        );

        let blur = InkpodBlurEffectInput {
            struct_size: size_of::<InkpodBlurEffectInput>() as u32,
            reserved: 0,
            feature_flags: 0,
            plane_id: document.color_plane_id,
            radius: 1,
            strength_milli: 500,
            reserved_2: 0,
            reserved_3: 0,
        };
        assert_eq!(
            inkpod_core_effect_blur(core, &blur, &mut dispatch),
            INKPOD_STATUS_OK
        );

        let stamp = InkpodStampInput {
            struct_size: size_of::<InkpodStampInput>() as u32,
            reserved: 0,
            feature_flags: 0,
            plane_id: document.color_plane_id,
            source_x: 0,
            source_y: 0,
            destination_x: 2,
            destination_y: 2,
            width: 2,
            height: 2,
            opacity_milli: 1_000,
            reserved_2: 0,
        };
        assert_eq!(
            inkpod_core_effect_stamp(core, &stamp, &mut dispatch),
            INKPOD_STATUS_OK
        );

        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        let before_alpha = document.color_plane_checksum;
        let alpha_pixels = [64_u8; 16];
        let mut alpha = InkpodAlphaEditInput {
            struct_size: size_of::<InkpodAlphaEditInput>() as u32,
            pixel_format: INKPOD_STORAGE_GRAYSCALE8,
            feature_flags: 0,
            plane_id: document.color_plane_id,
            width: 4,
            height: 4,
            reserved: 0,
            reserved_2: 0,
            pixels: alpha_pixels.as_ptr(),
            pixel_bytes: alpha_pixels.len() as u64,
            row_stride_bytes: 4,
        };
        alpha.row_stride_bytes = 3;
        assert_eq!(
            inkpod_core_alpha_edit(core, &alpha, &mut dispatch),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        alpha.row_stride_bytes = 4;
        assert_eq!(
            inkpod_core_alpha_edit(core, &alpha, &mut dispatch),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        assert_ne!(document.color_plane_checksum, before_alpha);
        assert_eq!(inkpod_core_undo(core, &mut dispatch), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        assert_eq!(document.color_plane_checksum, before_alpha);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}

#[test]
fn gesture_dust_task_ownership_and_cancel_are_connected() {
    let mut core = ptr::null_mut();
    // SAFETY: Every record and borrowed span remains live for its call.
    unsafe {
        assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
        let options = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 61,
            document_uuid_low: 62,
            width: 8,
            height: 8,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut document = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut document),
            INKPOD_STATUS_OK
        );
        let mut dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        let samples = [
            InkpodStrokeSample {
                struct_size: size_of::<InkpodStrokeSample>() as u32,
                flags: 0,
                x: 2.0,
                y: 2.0,
                pressure: 0.25,
                reserved: 0,
            },
            InkpodStrokeSample {
                struct_size: size_of::<InkpodStrokeSample>() as u32,
                flags: 0,
                x: 6.0,
                y: 2.0,
                pressure: 1.0,
                reserved: 0,
            },
        ];
        let airbrush = InkpodAirbrushGestureInput {
            struct_size: size_of::<InkpodAirbrushGestureInput>() as u32,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            feature_flags: INKPOD_EFFECT_FLAG_PRESSURE_SIZE | INKPOD_EFFECT_FLAG_PRESSURE_OPACITY,
            plane_id: document.color_plane_id,
            view_id: 0,
            radius_milli: 1_500,
            hardness_milli: 500,
            spacing_milli: 500,
            opacity_milli: 1_000,
            fade_milli: 100,
            continuous_dabs: 2,
            color: InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_16,
                red: 65_535,
                green: 0,
                blue: 0,
                alpha: 65_535,
            },
            samples: samples.as_ptr(),
            sample_count: samples.len() as u64,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        assert_eq!(
            inkpod_core_effect_airbrush_gesture(core, &airbrush, &mut dispatch),
            INKPOD_STATUS_OK
        );

        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        let before_cancel = document.color_plane_checksum;
        let filter = InkpodFilterInput {
            struct_size: size_of::<InkpodFilterInput>() as u32,
            kind: INKPOD_FILTER_INVERT,
            feature_flags: 0,
            plane_id: document.color_plane_id,
            channel: INKPOD_FILTER_CHANNEL_RGB,
            interpolation: INKPOD_CURVE_BEZIER,
            parameter_0: 0,
            parameter_1: 0,
            parameter_2: 0,
            parameter_3: 0,
            parameter_4: 0,
            point_stride_bytes: 0,
            points: ptr::null(),
            point_count: 0,
        };
        let mut task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut task), INKPOD_STATUS_OK);
        assert_eq!(inkpod_task_cancel(task), INKPOD_STATUS_OK);
        let mut preview = InkpodFilterPreviewInfo {
            struct_size: size_of::<InkpodFilterPreviewInfo>() as u32,
            reserved: 0,
            plane_id: 0,
            base_checksum: 0,
            preview_checksum: 0,
            preview_revision: 0,
        };
        assert_eq!(
            inkpod_core_filter_preview_begin_task(core, &filter, task, &mut preview),
            INKPOD_STATUS_CANCELLED
        );
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        assert_eq!(document.color_plane_checksum, before_cancel);
        let mut task_info = InkpodTaskInfo {
            struct_size: size_of::<InkpodTaskInfo>() as u32,
            state: 99,
            completed_work: 0,
            total_work: 0,
            reserved: 99,
        };
        assert_eq!(inkpod_task_query(task, &mut task_info), INKPOD_STATUS_OK);
        assert_eq!(task_info.state, INKPOD_TASK_CANCELLED);
        assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);
        assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);

        assert_eq!(inkpod_task_create(&mut task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_filter_preview_begin_task(core, &filter, task, &mut preview),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_filter_preview_apply(core, &mut dispatch),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        let before_cancelled_last = document.color_plane_checksum;
        assert_eq!(inkpod_task_create(&mut task), INKPOD_STATUS_OK);
        assert_eq!(inkpod_task_cancel(task), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_filter_apply_last_task(core, document.color_plane_id, task, &mut dispatch),
            INKPOD_STATUS_CANCELLED
        );
        assert_eq!(
            inkpod_core_get_document_info(core, &mut document),
            INKPOD_STATUS_OK
        );
        assert_eq!(document.color_plane_checksum, before_cancelled_last);
        assert_eq!(inkpod_task_release(&mut task), INKPOD_STATUS_OK);

        let mut dust_task = ptr::null_mut();
        assert_eq!(inkpod_task_create(&mut dust_task), INKPOD_STATUS_OK);
        let dust = InkpodDustInput {
            struct_size: size_of::<InkpodDustInput>() as u32,
            mode: INKPOD_DUST_REMOVE_FOREGROUND,
            feature_flags: 0,
            plane_id: document.color_plane_id,
            view_id: 0,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            shape: INKPOD_SELECTION_RECTANGLE,
            maximum_pixels: 1,
            use_region: 1,
            diameter: 1.0,
            samples: samples.as_ptr(),
            sample_count: samples.len() as u64,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        assert_eq!(
            inkpod_core_dust_remove(core, &dust, dust_task, &mut dispatch),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_task_query(dust_task, &mut task_info),
            INKPOD_STATUS_OK
        );
        assert_eq!(task_info.state, INKPOD_TASK_COMPLETED);
        assert!(task_info.total_work > 0);
        assert_eq!(inkpod_task_release(&mut dust_task), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
    }
}
