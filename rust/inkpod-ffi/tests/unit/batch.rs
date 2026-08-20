use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn color(value: [u16; 4]) -> InkpodColorValue {
    InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        depth: INKPOD_COLOR_DEPTH_8,
        red: value[0],
        green: value[1],
        blue: value[2],
        alpha: value[3],
    }
}

#[test]
fn graph_preview_dry_run_and_owned_report_cross_ffi() {
    let directory = std::env::temp_dir().join(format!(
        "inkpod-test-ffi-{}-{}",
        std::process::id(),
        TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let folder = directory.to_string_lossy().into_owned();
    let name = b"ffi-batch";
    let basename = b"cell";
    let input = InkpodBatchInput {
        struct_size: size_of::<InkpodBatchInput>() as u32,
        kind: INKPOD_BATCH_INPUT_CURRENT_SEQUENCE,
        feature_flags: INKPOD_FEATURE_NONE,
        path_utf8: ptr::null(),
        path_bytes: 0,
        first_cell: 0,
        last_cell: 0,
        reserved: 0,
    };
    let pair = InkpodBatchColorPairInput {
        struct_size: size_of::<InkpodBatchColorPairInput>() as u32,
        enabled: 1,
        reserved: 0,
        old_color: color([0, 0, 0, 0]),
        new_color: color([255, 0, 0, 255]),
    };
    let operation = InkpodBatchOperationInput {
        struct_size: size_of::<InkpodBatchOperationInput>() as u32,
        version: BATCH_OPERATION_VERSION,
        kind: INKPOD_BATCH_OPERATION_COLOR_REPLACE,
        reserved: 0,
        flags: INKPOD_BATCH_OPERATION_ENABLED,
        layer_id: 0,
        plane_id: 0,
        layer_kind: INKPOD_LAYER_BINARY_COLORING,
        plane_kind: INKPOD_TYPED_PLANE_COLOR,
        missing_policy: INKPOD_BATCH_MISSING_ERROR,
        reserved_2: 0,
        parameters: [0; 8],
        color_0: color([0, 0, 0, 0]),
        color_1: color([0, 0, 0, 0]),
        colors: InkpodColorArray {
            struct_size: size_of::<InkpodColorArray>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            colors: ptr::null(),
            color_count: 0,
            color_stride_bytes: 0,
        },
        filter: ptr::null(),
        color_pairs: &pair,
        color_pair_count: 1,
        color_pair_stride_bytes: size_of::<InkpodBatchColorPairInput>() as u64,
        seeds: ptr::null(),
        seed_count: 0,
        seed_stride_bytes: 0,
        reserved_3: 0,
    };
    let graph_input = InkpodBatchGraphInput {
        struct_size: size_of::<InkpodBatchGraphInput>() as u32,
        version: INKPOD_BATCH_GRAPH_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
        name_utf8: name.as_ptr(),
        name_bytes: name.len() as u64,
        inputs: &input,
        input_count: 1,
        input_stride_bytes: size_of::<InkpodBatchInput>() as u64,
        operations: &operation,
        operation_count: 1,
        operation_stride_bytes: size_of::<InkpodBatchOperationInput>() as u64,
        output_policy: INKPOD_BATCH_OUTPUT_NEW_SAVE,
        failure_policy: INKPOD_BATCH_FAILURE_CONTINUE,
        output_flags: 0,
        output_folder_utf8: folder.as_ptr(),
        output_folder_bytes: folder.len() as u64,
        basename_utf8: basename.as_ptr(),
        basename_bytes: basename.len() as u64,
        start_number: 1,
        wait_milliseconds: 0,
        reserved: 0,
    };
    let mut graph = ptr::null_mut();
    assert_eq!(
        unsafe { inkpod_batch_graph_create(&graph_input, &mut graph) },
        INKPOD_STATUS_OK
    );
    let mut info = InkpodBatchGraphInfo {
        struct_size: size_of::<InkpodBatchGraphInfo>() as u32,
        version: 0,
        input_count: 0,
        operation_count: 0,
        output_policy: 0,
        failure_policy: 0,
        output_flags: 0,
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_info(graph, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!((info.input_count, info.operation_count), (1, 1));
    let mut operation_info = InkpodBatchOperationInfo {
        struct_size: size_of::<InkpodBatchOperationInfo>() as u32,
        ..InkpodBatchOperationInfo::default()
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation(graph, 0, &mut operation_info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(operation_info.kind, INKPOD_BATCH_OPERATION_COLOR_REPLACE);
    assert_eq!(operation_info.color_pair_count, 1);
    let mut queried_pair = InkpodBatchColorPairInput {
        struct_size: size_of::<InkpodBatchColorPairInput>() as u32,
        enabled: 0,
        reserved: u64::MAX,
        old_color: color([0, 0, 0, 0]),
        new_color: color([0, 0, 0, 0]),
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation_color_pair(graph, 0, 0, &mut queried_pair) },
        INKPOD_STATUS_OK
    );
    assert_eq!(queried_pair.enabled, 1);
    assert_eq!(queried_pair.new_color.red, 255);

    let unresolved_operation = InkpodBatchOperationInput {
        flags: INKPOD_BATCH_OPERATION_ENABLED | INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN,
        ..operation
    };
    let mut run_graph = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_batch_graph_clone_with_operations(
                graph,
                &unresolved_operation,
                1,
                size_of::<InkpodBatchOperationInput>() as u64,
                &mut run_graph,
            )
        },
        INKPOD_STATUS_INVALID_STATE
    );
    assert!(run_graph.is_null());
    assert_eq!(
        unsafe {
            inkpod_batch_graph_clone_with_operations(
                graph,
                &operation,
                1,
                size_of::<InkpodBatchOperationInput>() as u64,
                &mut run_graph,
            )
        },
        INKPOD_STATUS_OK
    );
    assert!(!run_graph.is_null());

    let mut core = InkpodCore {
        owner_thread: thread::current().id(),
        core: Core::new(),
        objects: crate::v3::ObjectRegistry::new().expect("test Core generation"),
    };
    core.core.new_cell(2, 2, 96_000, 96_000).unwrap();
    let mut preview = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_core_batch_preview(&mut core, graph, INKPOD_BATCH_SCOPE_ALL, &mut preview)
        },
        INKPOD_STATUS_OK
    );
    let mut preview_count = 0;
    assert_eq!(
        unsafe { inkpod_batch_preview_count(preview, &mut preview_count) },
        INKPOD_STATUS_OK
    );
    assert_eq!(preview_count, 1);
    let mut preview_item = InkpodBatchPreviewItem {
        struct_size: size_of::<InkpodBatchPreviewItem>() as u32,
        flags: 0,
        input_name: ptr::null(),
        input_name_bytes: 0,
        output_path: ptr::null(),
        output_path_bytes: 0,
        warning: ptr::null(),
        warning_bytes: 0,
    };
    assert_eq!(
        unsafe { inkpod_batch_preview_get(preview, 0, &mut preview_item) },
        INKPOD_STATUS_OK
    );
    assert!(preview_item.input_name_bytes != 0);
    assert_eq!(
        unsafe { inkpod_batch_preview_release(&mut preview) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_preview_release(&mut preview) },
        INKPOD_STATUS_OK
    );

    let mut task = ptr::null_mut();
    let mut report = ptr::null_mut();
    assert_eq!(
        unsafe { inkpod_batch_task_create(&mut task) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe {
            inkpod_core_batch_execute(
                &mut core,
                graph,
                INKPOD_BATCH_SCOPE_ALL,
                INKPOD_BATCH_RUN_DRY | INKPOD_BATCH_RUN_PREVIEW_CONFIRMED,
                task,
                &mut report,
            )
        },
        INKPOD_STATUS_OK
    );
    let mut task_info = InkpodTaskInfo {
        struct_size: size_of::<InkpodTaskInfo>() as u32,
        state: 0,
        completed_work: 0,
        total_work: 0,
        reserved: 0,
    };
    assert_eq!(
        unsafe { inkpod_batch_task_query(task, &mut task_info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(task_info.state, INKPOD_TASK_COMPLETED);
    let mut report_info = InkpodBatchReportInfo {
        struct_size: size_of::<InkpodBatchReportInfo>() as u32,
        cancelled: 0,
        item_count: 0,
        failure_count: 0,
        reserved: u64::MAX,
    };
    assert_eq!(
        unsafe { inkpod_batch_report_get_info(report, &mut report_info) },
        INKPOD_STATUS_OK
    );
    assert_eq!((report_info.item_count, report_info.failure_count), (1, 0));
    let mut report_item = InkpodBatchReportItem {
        struct_size: size_of::<InkpodBatchReportItem>() as u32,
        outcome: 0,
        input_name: ptr::null(),
        input_name_bytes: 0,
        output_path: ptr::null(),
        output_path_bytes: 0,
        message: ptr::null(),
        message_bytes: 0,
    };
    assert_eq!(
        unsafe { inkpod_batch_report_get(report, 0, &mut report_item) },
        INKPOD_STATUS_OK
    );
    assert_eq!(report_item.outcome, INKPOD_BATCH_ITEM_DRY_RUN);
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);

    let settings = directory.join("settings.inkbatch");
    let settings_text = settings.to_string_lossy();
    assert_eq!(
        unsafe {
            inkpod_batch_graph_save(
                graph,
                settings_text.as_bytes().as_ptr(),
                settings_text.len() as u64,
            )
        },
        INKPOD_STATUS_OK
    );
    let mut reopened = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_batch_graph_load(
                settings_text.as_bytes().as_ptr(),
                settings_text.len() as u64,
                &mut reopened,
            )
        },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_report_release(&mut report) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_task_release(&mut task) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_graph_release(&mut reopened) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_graph_release(&mut run_graph) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_graph_release(&mut graph) },
        INKPOD_STATUS_OK
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn graph_operation_queries_restore_seed_separation_and_curve_rows() {
    let graph = InkpodBatchGraph {
        graph: BatchGraph {
            version: INKPOD_BATCH_GRAPH_VERSION,
            name: "query-rows".to_owned(),
            inputs: vec![BatchInputSelector::current_sequence()],
            operations: vec![
                BatchOperation {
                    version: BATCH_OPERATION_VERSION,
                    enabled: true,
                    configure_each_run: true,
                    target: Some(BatchTargetSelector::color_plane()),
                    kind: BatchOperationKind::ContinuousFill(vec![BatchSeed {
                        enabled: false,
                        x: 7,
                        y: 9,
                        color: PixelValue::Rgba([1, 2, 3, 255]),
                        tolerance: 4,
                        gap_close: 2,
                        expected_source: Some(PixelValue::Rgba([9, 8, 7, 255])),
                    }]),
                },
                BatchOperation {
                    version: BATCH_OPERATION_VERSION,
                    enabled: true,
                    configure_each_run: false,
                    target: Some(BatchTargetSelector::color_plane()),
                    kind: BatchOperationKind::Separation(BatchSeparation {
                        colors: vec![
                            PixelValue::Rgba([10, 20, 30, 255]),
                            PixelValue::Rgba([40, 50, 60, 128]),
                        ],
                        replacement: PixelValue::Rgba([70, 80, 90, 255]),
                        invert: true,
                        destination: BatchSeparationDestination::ColorPlane,
                    }),
                },
                BatchOperation {
                    version: BATCH_OPERATION_VERSION,
                    enabled: true,
                    configure_each_run: false,
                    target: Some(BatchTargetSelector::color_plane()),
                    kind: BatchOperationKind::Filter(Filter::ToneCurve {
                        channel: Channel::Blue,
                        interpolation: CurveInterpolation::BSpline,
                        points: vec![
                            CurvePoint {
                                input: 1,
                                output: 2,
                            },
                            CurvePoint {
                                input: 3,
                                output: 4,
                            },
                        ],
                    }),
                },
            ],
            output: BatchOutputSettings::default(),
        },
    };

    let mut info = InkpodBatchOperationInfo {
        struct_size: size_of::<InkpodBatchOperationInfo>() as u32,
        ..InkpodBatchOperationInfo::default()
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation(&graph, 0, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.kind, INKPOD_BATCH_OPERATION_CONTINUOUS_FILL);
    assert_eq!(info.seed_count, 1);
    assert_ne!(info.flags & INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN, 0);
    let mut seed = InkpodBatchSeedInput {
        struct_size: size_of::<InkpodBatchSeedInput>() as u32,
        flags: 0,
        x: 0,
        y: 0,
        tolerance: 0,
        gap_close: 0,
        reserved: u64::MAX,
        fill_color: color([0, 0, 0, 0]),
        expected_color: color([0, 0, 0, 0]),
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation_seed(&graph, 0, 0, &mut seed) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        (seed.x, seed.y, seed.tolerance, seed.gap_close),
        (7, 9, 4, 2)
    );
    assert_eq!(seed.flags, INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR);
    assert_eq!(seed.expected_color.red, 9);

    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation(&graph, 1, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.color_count, 2);
    assert_eq!(info.parameters[1], INKPOD_BATCH_SEPARATION_COLOR_PLANE);
    let mut separated_color = color([0, 0, 0, 0]);
    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation_color(&graph, 1, 1, &mut separated_color) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        (
            separated_color.red,
            separated_color.green,
            separated_color.blue,
            separated_color.alpha,
        ),
        (40, 50, 60, 128)
    );

    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation(&graph, 2, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.filter_kind, INKPOD_FILTER_TONE_CURVE);
    assert_eq!(info.filter_channel, INKPOD_FILTER_CHANNEL_BLUE);
    assert_eq!(info.filter_interpolation, INKPOD_CURVE_BSPLINE);
    assert_eq!(info.curve_point_count, 2);
    let mut point = InkpodCurvePoint {
        struct_size: size_of::<InkpodCurvePoint>() as u32,
        reserved: u32::MAX,
        input: 0,
        output: 0,
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation_curve_point(&graph, 2, 1, &mut point) },
        INKPOD_STATUS_OK
    );
    assert_eq!((point.input, point.output), (3, 4));

    #[repr(C, align(8))]
    struct ShortInfo {
        struct_size: u32,
    }
    let mut short = ShortInfo {
        struct_size: size_of::<ShortInfo>() as u32,
    };
    assert_eq!(
        unsafe {
            inkpod_batch_graph_get_operation(
                &graph,
                0,
                (&raw mut short).cast::<InkpodBatchOperationInfo>(),
            )
        },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
}

#[test]
fn ffi_rejects_short_graph_and_cancelled_task_is_idempotent() {
    #[repr(C, align(8))]
    struct Short {
        struct_size: u32,
    }
    let short = Short {
        struct_size: size_of::<Short>() as u32,
    };
    let mut graph = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_batch_graph_create(
                (&raw const short).cast::<InkpodBatchGraphInput>(),
                &mut graph,
            )
        },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    assert!(graph.is_null());

    let mut input_record = InkpodBatchInput {
        struct_size: size_of::<InkpodBatchInput>() as u32,
        kind: INKPOD_BATCH_INPUT_CURRENT_SEQUENCE,
        feature_flags: INKPOD_FEATURE_NONE,
        path_utf8: ptr::null(),
        path_bytes: 0,
        first_cell: 0,
        last_cell: 0,
        reserved: 0,
    };
    let oversized_stride = (isize::MAX as u64).saturating_add(1);
    assert_eq!(
        unsafe {
            record_at(
                &input_record,
                2,
                oversized_stride,
                0,
                MAX_BATCH_INPUTS,
                "InkpodBatchInput",
            )
        },
        Err(INKPOD_STATUS_INVALID_ARGUMENT)
    );
    input_record.struct_size = (size_of::<InkpodBatchInput>() + 8) as u32;
    assert_eq!(
        unsafe {
            record_at(
                &input_record,
                1,
                size_of::<InkpodBatchInput>() as u64,
                0,
                MAX_BATCH_INPUTS,
                "InkpodBatchInput",
            )
        },
        Err(INKPOD_STATUS_INCOMPATIBLE_ABI)
    );

    let filter_storage =
        vec![0_u8; size_of::<InkpodFilterInput>() + align_of::<InkpodFilterInput>()];
    let filter_offset = (0..align_of::<InkpodFilterInput>())
        .find(|offset| {
            (filter_storage.as_ptr() as usize + offset) % align_of::<InkpodFilterInput>() != 0
        })
        .unwrap();
    // SAFETY: The offset remains within filter_storage; the deliberately
    // misaligned pointer must be rejected before any record field is read.
    let misaligned_filter = unsafe {
        filter_storage
            .as_ptr()
            .add(filter_offset)
            .cast::<InkpodFilterInput>()
    };
    let filter_operation = InkpodBatchOperationInput {
        struct_size: size_of::<InkpodBatchOperationInput>() as u32,
        version: BATCH_OPERATION_VERSION,
        kind: INKPOD_BATCH_OPERATION_FILTER,
        reserved: 0,
        flags: INKPOD_BATCH_OPERATION_ENABLED,
        layer_id: 0,
        plane_id: 0,
        layer_kind: INKPOD_LAYER_BINARY_COLORING,
        plane_kind: INKPOD_TYPED_PLANE_COLOR,
        missing_policy: INKPOD_BATCH_MISSING_ERROR,
        reserved_2: 0,
        parameters: [0; 8],
        color_0: color([0, 0, 0, 0]),
        color_1: color([0, 0, 0, 0]),
        colors: InkpodColorArray {
            struct_size: size_of::<InkpodColorArray>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            colors: ptr::null(),
            color_count: 0,
            color_stride_bytes: 0,
        },
        filter: misaligned_filter,
        color_pairs: ptr::null(),
        color_pair_count: 0,
        color_pair_stride_bytes: 0,
        seeds: ptr::null(),
        seed_count: 0,
        seed_stride_bytes: 0,
        reserved_3: 0,
    };
    assert_eq!(
        unsafe { parse_operation(&filter_operation) }.unwrap_err(),
        INKPOD_STATUS_INVALID_ARGUMENT
    );

    let mut task = ptr::null_mut();
    assert_eq!(
        unsafe { inkpod_batch_task_create(&mut task) },
        INKPOD_STATUS_OK
    );
    assert_eq!(unsafe { inkpod_batch_task_cancel(task) }, INKPOD_STATUS_OK);
    assert_eq!(unsafe { inkpod_batch_task_cancel(task) }, INKPOD_STATUS_OK);
    assert_eq!(
        unsafe { inkpod_batch_task_release(&mut task) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_task_release(&mut task) },
        INKPOD_STATUS_OK
    );
}

#[test]
fn owned_pair_preview_reports_ambiguity_and_rejects_short_records() {
    let source = |name: &str, uuid: u128, generation: u64, pixels: Vec<u8>| {
        let mut cell = SequenceCellSource::from_rgba_bytes(
            name,
            uuid,
            RgbaRasterBytes {
                width: 2,
                height: 1,
                pixel_format: PixelFormat::StraightRgba8,
                dpi_x_milli: None,
                dpi_y_milli: None,
                pixels,
            },
        )
        .unwrap();
        cell.source_generation = generation;
        cell
    };
    let mut core = InkpodCore {
        owner_thread: thread::current().id(),
        core: Core::new(),
        objects: crate::v3::ObjectRegistry::new().expect("test Core generation"),
    };
    core.core
        .set_sequence(vec![
            source("A001", 0x101, 3, vec![10, 20, 30, 40, 10, 20, 30, 40]),
            source("A002", 0x202, 4, vec![1, 2, 3, 255, 4, 5, 6, 255]),
        ])
        .unwrap();
    let old_identity = InkpodSequenceSourceIdentity {
        struct_size: size_of::<InkpodSequenceSourceIdentity>() as u32,
        reserved: 0,
        document_uuid_high: 0,
        document_uuid_low: 0x101,
        source_generation: 3,
    };
    let new_identity = InkpodSequenceSourceIdentity {
        struct_size: size_of::<InkpodSequenceSourceIdentity>() as u32,
        reserved: 0,
        document_uuid_high: 0,
        document_uuid_low: 0x202,
        source_generation: 4,
    };
    let mut preview = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_core_batch_extract_color_pairs(
                &mut core,
                &old_identity,
                &new_identity,
                &mut preview,
            )
        },
        INKPOD_STATUS_OK
    );
    let mut info = InkpodBatchPairPreviewInfo {
        struct_size: size_of::<InkpodBatchPairPreviewInfo>() as u32,
        ..Default::default()
    };
    assert_eq!(
        unsafe { inkpod_batch_pair_preview_get_info(preview, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(info.pixel_format, INKPOD_STORAGE_RGBA8);
    assert_eq!((info.candidate_count, info.ambiguity_count), (2, 1));
    let mut candidate = InkpodBatchPairCandidate {
        struct_size: size_of::<InkpodBatchPairCandidate>() as u32,
        ..Default::default()
    };
    assert_eq!(
        unsafe { inkpod_batch_pair_preview_get_candidate(preview, 0, &mut candidate) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        candidate.flags & INKPOD_BATCH_PAIR_CANDIDATE_AMBIGUOUS,
        INKPOD_BATCH_PAIR_CANDIDATE_AMBIGUOUS
    );
    assert_eq!(candidate.old_color.alpha, 40);

    #[repr(C, align(8))]
    struct ShortInfo {
        struct_size: u32,
    }
    let mut short = ShortInfo {
        struct_size: size_of::<ShortInfo>() as u32,
    };
    assert_eq!(
        unsafe {
            inkpod_batch_pair_preview_get_info(
                preview,
                (&raw mut short).cast::<InkpodBatchPairPreviewInfo>(),
            )
        },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    assert_eq!(
        unsafe { inkpod_batch_pair_preview_release(&mut preview) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_pair_preview_release(&mut preview) },
        INKPOD_STATUS_OK
    );
}
