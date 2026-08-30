use super::*;

fn rgba8(value: [u16; 4]) -> InkpodColorValue {
    InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        depth: INKPOD_COLOR_DEPTH_8,
        red: value[0],
        green: value[1],
        blue: value[2],
        alpha: value[3],
    }
}

fn binary(value: u8) -> InkpodColorValue {
    InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        depth: INKPOD_COLOR_DEPTH_BINARY,
        red: u16::from(value),
        green: 0,
        blue: 0,
        alpha: 0,
    }
}

fn color_array(colors: &[InkpodColorValue]) -> InkpodColorArray {
    InkpodColorArray {
        struct_size: size_of::<InkpodColorArray>() as u32,
        reserved: 0,
        feature_flags: INKPOD_FEATURE_NONE,
        colors: colors.as_ptr(),
        color_count: colors.len() as u64,
        color_stride_bytes: size_of::<InkpodColorValue>() as u64,
    }
}

fn operation(
    kind: u32,
    colors: &[InkpodColorValue],
    pairs: &[InkpodBatchColorPairInput],
) -> InkpodBatchOperationInput {
    InkpodBatchOperationInput {
        struct_size: size_of::<InkpodBatchOperationInput>() as u32,
        version: BATCH_OPERATION_VERSION,
        kind,
        reserved: 0,
        flags: INKPOD_BATCH_OPERATION_ENABLED,
        layer_id: 0,
        plane_id: 0,
        plane_kind: INKPOD_TYPED_PLANE_COLOR,
        missing_policy: INKPOD_BATCH_MISSING_ERROR,
        reserved_2: 0,
        colors: color_array(colors),
        color_pairs: pairs.as_ptr(),
        color_pair_count: pairs.len() as u64,
        color_pair_stride_bytes: size_of::<InkpodBatchColorPairInput>() as u64,
        reserved_3: 0,
        additional_targets: ptr::null(),
        additional_target_count: 0,
        additional_target_stride_bytes: size_of::<InkpodBatchTargetInput>() as u64,
        reserved_4: 0,
    }
}

fn graph_input(
    inputs: &[InkpodBatchInput],
    operations: &[InkpodBatchOperationInput],
    output_destination: u32,
) -> InkpodBatchGraphInput {
    static NAME: &[u8] = b"batch-v5-ffi";
    static TEMPLATE: &[u8] = b"{stem}_{index:3}";
    InkpodBatchGraphInput {
        struct_size: size_of::<InkpodBatchGraphInput>() as u32,
        version: INKPOD_BATCH_GRAPH_VERSION,
        feature_flags: INKPOD_FEATURE_NONE,
        name_utf8: NAME.as_ptr(),
        name_bytes: NAME.len() as u64,
        inputs: inputs.as_ptr(),
        input_count: inputs.len() as u64,
        input_stride_bytes: size_of::<InkpodBatchInput>() as u64,
        operations: operations.as_ptr(),
        operation_count: operations.len() as u64,
        operation_stride_bytes: size_of::<InkpodBatchOperationInput>() as u64,
        output_destination,
        failure_policy: INKPOD_BATCH_FAILURE_STOP,
        output_flags: 0,
        output_folder_utf8: ptr::null(),
        output_folder_bytes: 0,
        naming_template_utf8: TEMPLATE.as_ptr(),
        naming_template_bytes: TEMPLATE.len() as u64,
        output_format: INKPOD_BATCH_FORMAT_INKPOD,
        wait_milliseconds: 0,
        reserved: 0,
    }
}

fn active_document_input() -> InkpodBatchInput {
    InkpodBatchInput {
        struct_size: size_of::<InkpodBatchInput>() as u32,
        kind: INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT,
        feature_flags: INKPOD_FEATURE_NONE,
        path_utf8: ptr::null(),
        path_bytes: 0,
        first_cell: 0,
        last_cell: 0,
        reserved: 0,
    }
}

#[test]
fn io_003_async_batch_plan_run_and_contact_sheet_transfer_owned_results() {
    let colors = [rgba8([0, 0, 0, 0])];
    let operations = [operation(INKPOD_BATCH_OPERATION_ERASE, &colors, &[])];
    let inputs = [active_document_input()];
    let graph_input = graph_input(&inputs, &operations, INKPOD_BATCH_OUTPUT_NEW_TABS);
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
        document_uuid_low: 100,
        width: 2,
        height: 2,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
    };
    let mut document = InkpodDocumentInfo {
        struct_size: size_of::<InkpodDocumentInfo>() as u32,
        ..Default::default()
    };
    let (mut core, mut manager, mut graph, mut job) = (
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    );
    // SAFETY: Every handle has unique owner storage on this thread. Complete
    // input records and spans stay alive until the synchronous copy returns.
    unsafe {
        assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_core_new_cell(core, &options, &mut document),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_io_manager_create(ptr::null(), &mut manager),
            INKPOD_STATUS_OK
        );
        assert_eq!(inkpod_core_bind_io_manager(core, manager), INKPOD_STATUS_OK);
        assert_eq!(
            inkpod_batch_graph_create(&graph_input, &mut graph),
            INKPOD_STATUS_OK
        );
        assert_eq!(
            inkpod_core_io_batch_submit(
                core,
                manager,
                graph,
                u32::MAX,
                INKPOD_BATCH_SCOPE_ALL,
                0,
                1,
                &mut job
            ),
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(job.is_null());
        for kind in [
            INKPOD_IO_BATCH_PLAN,
            INKPOD_IO_BATCH_RUN,
            INKPOD_IO_BATCH_PREVIEW,
        ] {
            assert_eq!(
                inkpod_core_io_batch_submit(
                    core,
                    manager,
                    graph,
                    kind,
                    INKPOD_BATCH_SCOPE_ALL,
                    INKPOD_BATCH_RUN_PREVIEW_CONFIRMED,
                    1,
                    &mut job
                ),
                INKPOD_STATUS_OK
            );
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                let mut progress = InkpodIoJobInfo {
                    struct_size: size_of::<InkpodIoJobInfo>() as u32,
                    ..Default::default()
                };
                assert_eq!(inkpod_io_job_poll(job, &mut progress), INKPOD_STATUS_OK);
                assert!(
                    !matches!(progress.state, INKPOD_IO_FAILED | INKPOD_IO_CANCELLED),
                    "Batch job failed: {}",
                    progress.status
                );
                if progress.state == INKPOD_IO_READY {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "Batch I/O job stalled"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert_eq!(
                inkpod_core_io_job_apply(core, job, &mut document, ptr::null_mut()),
                INKPOD_STATUS_OK
            );
            if kind == INKPOD_IO_BATCH_PLAN {
                let mut preview = ptr::null_mut();
                assert_eq!(
                    inkpod_io_job_take_batch_preview(job, &mut preview),
                    INKPOD_STATUS_OK
                );
                let mut count = 0;
                assert_eq!(
                    inkpod_batch_preview_count(preview, &mut count),
                    INKPOD_STATUS_OK
                );
                assert_eq!(count, 1);
                assert_eq!(inkpod_batch_preview_release(&mut preview), INKPOD_STATUS_OK);
                assert_eq!(
                    inkpod_io_job_take_batch_preview(job, &mut preview),
                    INKPOD_STATUS_INVALID_STATE
                );
            } else {
                let mut report = ptr::null_mut();
                assert_eq!(
                    inkpod_io_job_take_batch_report(job, &mut report),
                    INKPOD_STATUS_OK
                );
                let mut info = InkpodBatchReportInfo {
                    struct_size: size_of::<InkpodBatchReportInfo>() as u32,
                    cancelled: 0,
                    item_count: 0,
                    failure_count: 0,
                    staged_result_count: 0,
                };
                assert_eq!(
                    inkpod_batch_report_get_info(report, &mut info),
                    INKPOD_STATUS_OK
                );
                assert_eq!(
                    (
                        info.item_count,
                        info.failure_count,
                        info.staged_result_count
                    ),
                    (1, 0, 1)
                );
                let mut staged = ptr::null_mut();
                let mut generation = 0;
                assert_eq!(
                    inkpod_batch_report_take_staged_result(report, 0, &mut generation, &mut staged),
                    INKPOD_STATUS_OK
                );
                assert_ne!(generation, 0);
                assert_eq!(inkpod_core_destroy(&mut staged), INKPOD_STATUS_OK);
                assert_eq!(inkpod_batch_report_release(&mut report), INKPOD_STATUS_OK);
                assert_eq!(
                    inkpod_io_job_take_batch_report(job, &mut report),
                    INKPOD_STATUS_INVALID_STATE
                );
            }
            assert_eq!(inkpod_io_job_release(&mut job), INKPOD_STATUS_OK);
        }
        assert_eq!(inkpod_batch_graph_release(&mut graph), INKPOD_STATUS_OK);
        assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        assert_eq!(inkpod_io_manager_release(&mut manager), INKPOD_STATUS_OK);
    }
}

#[test]
fn current_abi_graph_exposes_only_the_four_batch_v5_operation_shapes() {
    let colors = [rgba8([1, 2, 3, 4])];
    let pairs = [InkpodBatchColorPairInput {
        struct_size: size_of::<InkpodBatchColorPairInput>() as u32,
        enabled: 1,
        reserved: 0,
        old_color: rgba8([1, 2, 3, 4]),
        new_color: rgba8([5, 6, 7, 8]),
    }];
    let additional_targets = [InkpodBatchTargetInput {
        struct_size: size_of::<InkpodBatchTargetInput>() as u32,
        plane_kind: INKPOD_TYPED_PLANE_RASTER,
        missing_policy: INKPOD_BATCH_MISSING_ERROR,
        ..Default::default()
    }];
    let mut replace = operation(INKPOD_BATCH_OPERATION_COLOR_REPLACE, &[], &pairs);
    replace.additional_targets = additional_targets.as_ptr();
    replace.additional_target_count = additional_targets.len() as u64;
    let operations = [
        replace,
        operation(INKPOD_BATCH_OPERATION_MOVE_TO_COLOR_PLANE, &colors, &[]),
        operation(INKPOD_BATCH_OPERATION_MASKING, &colors, &[]),
        operation(INKPOD_BATCH_OPERATION_ERASE, &colors, &[]),
    ];
    let inputs = [active_document_input()];
    let input = graph_input(&inputs, &operations, INKPOD_BATCH_OUTPUT_NEW_TABS);
    let mut graph = ptr::null_mut();
    assert_eq!(
        unsafe { inkpod_batch_graph_create(&input, &mut graph) },
        INKPOD_STATUS_OK
    );

    let mut graph_info = InkpodBatchGraphInfo {
        struct_size: size_of::<InkpodBatchGraphInfo>() as u32,
        ..Default::default()
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_info(graph, &mut graph_info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(graph_info.version, INKPOD_BATCH_GRAPH_VERSION);
    assert_eq!(graph_info.operation_count, 4);
    assert_eq!(graph_info.output_destination, INKPOD_BATCH_OUTPUT_NEW_TABS);
    assert_eq!(graph_info.output_format, INKPOD_BATCH_FORMAT_INKPOD);
    assert_eq!(
        unsafe { slice::from_raw_parts(graph_info.name_utf8, graph_info.name_bytes as usize) },
        b"batch-v5-ffi"
    );
    assert_eq!(
        unsafe {
            slice::from_raw_parts(
                graph_info.naming_template_utf8,
                graph_info.naming_template_bytes as usize,
            )
        },
        b"{stem}_{index:3}"
    );
    let mut queried_input = active_document_input();
    assert_eq!(
        unsafe { inkpod_batch_graph_get_input(graph, 0, &mut queried_input) },
        INKPOD_STATUS_OK
    );
    assert_eq!(queried_input.kind, INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT);
    assert_eq!(queried_input.path_bytes, 0);

    for (index, expected) in [
        INKPOD_BATCH_OPERATION_COLOR_REPLACE,
        INKPOD_BATCH_OPERATION_MOVE_TO_COLOR_PLANE,
        INKPOD_BATCH_OPERATION_MASKING,
        INKPOD_BATCH_OPERATION_ERASE,
    ]
    .into_iter()
    .enumerate()
    {
        let mut info = InkpodBatchOperationInfo {
            struct_size: size_of::<InkpodBatchOperationInfo>() as u32,
            ..Default::default()
        };
        assert_eq!(
            unsafe { inkpod_batch_graph_get_operation(graph, index as u64, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.kind, expected);
        if index == 0 {
            assert_eq!((info.color_pair_count, info.color_count), (1, 0));
            assert_eq!(info.target_count, 2);
        } else {
            assert_eq!((info.color_pair_count, info.color_count), (0, 1));
            assert_eq!(info.target_count, 1);
        }
    }

    let mut queried_target = InkpodBatchTargetInput {
        struct_size: size_of::<InkpodBatchTargetInput>() as u32,
        ..Default::default()
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation_target(graph, 0, 1, &mut queried_target) },
        INKPOD_STATUS_OK
    );
    assert_eq!(queried_target.plane_kind, INKPOD_TYPED_PLANE_RASTER);

    let mut pair = InkpodBatchColorPairInput {
        struct_size: size_of::<InkpodBatchColorPairInput>() as u32,
        enabled: 0,
        reserved: u64::MAX,
        old_color: rgba8([0; 4]),
        new_color: rgba8([0; 4]),
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation_color_pair(graph, 0, 0, &mut pair) },
        INKPOD_STATUS_OK
    );
    assert_eq!(pair.new_color.alpha, 8);
    let mut queried = rgba8([0; 4]);
    assert_eq!(
        unsafe { inkpod_batch_graph_get_operation_color(graph, 2, 0, &mut queried) },
        INKPOD_STATUS_OK
    );
    assert_eq!(queried.alpha, 4);

    let replacement = [operation(INKPOD_BATCH_OPERATION_ERASE, &colors, &[])];
    let mut cloned = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_batch_graph_clone_with_operations(
                graph,
                replacement.as_ptr(),
                replacement.len() as u64,
                size_of::<InkpodBatchOperationInput>() as u64,
                &mut cloned,
            )
        },
        INKPOD_STATUS_OK
    );
    let mut cloned_info = InkpodBatchGraphInfo {
        struct_size: size_of::<InkpodBatchGraphInfo>() as u32,
        ..Default::default()
    };
    assert_eq!(
        unsafe { inkpod_batch_graph_get_info(cloned, &mut cloned_info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(cloned_info.operation_count, 1);
    assert_eq!(
        unsafe { inkpod_batch_graph_release(&mut cloned) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_graph_release(&mut graph) },
        INKPOD_STATUS_OK
    );
}

#[test]
fn current_abi_rejects_short_unknown_and_invalid_stride_records() {
    let colors = [binary(0)];
    let operations = [operation(INKPOD_BATCH_OPERATION_ERASE, &colors, &[])];
    let inputs = [active_document_input()];
    let mut input = graph_input(&inputs, &operations, INKPOD_BATCH_OUTPUT_NEW_TABS);
    let mut graph = ptr::null_mut();
    input.struct_size = size_of::<u32>() as u32;
    assert_eq!(
        unsafe { inkpod_batch_graph_create(&input, &mut graph) },
        INKPOD_STATUS_INCOMPATIBLE_ABI
    );
    input.struct_size = size_of::<InkpodBatchGraphInput>() as u32;
    input.operation_stride_bytes = (size_of::<InkpodBatchOperationInput>() - 8) as u64;
    assert_eq!(
        unsafe { inkpod_batch_graph_create(&input, &mut graph) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    input.operation_stride_bytes = size_of::<InkpodBatchOperationInput>() as u64;
    let unknown = InkpodBatchOperationInput {
        kind: 99,
        ..operations[0]
    };
    input.operations = &unknown;
    assert_eq!(
        unsafe { inkpod_batch_graph_create(&input, &mut graph) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    assert!(graph.is_null());

    let pairs = [InkpodBatchColorPairInput {
        struct_size: size_of::<InkpodBatchColorPairInput>() as u32,
        enabled: 1,
        reserved: 0,
        old_color: rgba8([1, 2, 3, 255]),
        new_color: rgba8([4, 5, 6, 255]),
    }];
    let main_line_target = [InkpodBatchTargetInput {
        struct_size: size_of::<InkpodBatchTargetInput>() as u32,
        plane_kind: INKPOD_TYPED_PLANE_MAIN_LINE,
        missing_policy: INKPOD_BATCH_MISSING_ERROR,
        ..Default::default()
    }];
    let mut replace = operation(INKPOD_BATCH_OPERATION_COLOR_REPLACE, &[], &pairs);
    replace.additional_targets = main_line_target.as_ptr();
    replace.additional_target_count = main_line_target.len() as u64;
    input.operations = &replace;
    input.operation_count = 1;
    assert_eq!(
        unsafe { inkpod_batch_graph_create(&input, &mut graph) },
        INKPOD_STATUS_INVALID_ARGUMENT
    );
    assert!(graph.is_null());
}

#[test]
fn new_tab_result_is_taken_once_on_the_report_owner_thread() {
    let mut cancelled_task = ptr::null_mut();
    assert_eq!(
        unsafe { inkpod_batch_task_create(&mut cancelled_task) },
        INKPOD_STATUS_OK
    );
    let mut task_info = InkpodTaskInfo {
        struct_size: size_of::<InkpodTaskInfo>() as u32,
        state: u32::MAX,
        completed_work: u64::MAX,
        total_work: u64::MAX,
        reserved: u64::MAX,
    };
    assert_eq!(
        unsafe { inkpod_batch_task_query(cancelled_task, &mut task_info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(task_info.state, INKPOD_TASK_READY);
    assert_eq!(
        unsafe { inkpod_batch_task_cancel(cancelled_task) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_task_query(cancelled_task, &mut task_info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(task_info.state, INKPOD_TASK_CANCELLED);
    assert_eq!(
        unsafe { inkpod_batch_task_release(&mut cancelled_task) },
        INKPOD_STATUS_OK
    );

    let colors = [rgba8([0, 0, 0, 0])];
    let operations = [operation(INKPOD_BATCH_OPERATION_ERASE, &colors, &[])];
    let inputs = [active_document_input()];
    let input = graph_input(&inputs, &operations, INKPOD_BATCH_OUTPUT_NEW_TABS);
    let mut graph = ptr::null_mut();
    assert_eq!(
        unsafe { inkpod_batch_graph_create(&input, &mut graph) },
        INKPOD_STATUS_OK
    );
    let mut core = InkpodCore {
        owner_thread: thread::current().id(),
        core: Core::new(),
        objects: crate::v3::ObjectRegistry::new().expect("test Core generation"),
    };
    core.core.new_cell(2, 2, 96_000, 96_000).unwrap();
    let mut task = ptr::null_mut();
    assert_eq!(
        unsafe { inkpod_batch_task_create(&mut task) },
        INKPOD_STATUS_OK
    );
    let mut report = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_core_batch_execute(
                &mut core,
                graph,
                INKPOD_BATCH_SCOPE_ALL,
                INKPOD_BATCH_RUN_PREVIEW_CONFIRMED,
                task,
                &mut report,
            )
        },
        INKPOD_STATUS_OK
    );
    let mut info = InkpodBatchReportInfo {
        struct_size: size_of::<InkpodBatchReportInfo>() as u32,
        cancelled: 0,
        item_count: 0,
        failure_count: 0,
        staged_result_count: 0,
    };
    assert_eq!(
        unsafe { inkpod_batch_report_get_info(report, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        (
            info.item_count,
            info.failure_count,
            info.staged_result_count
        ),
        (1, 0, 1)
    );

    let report_address = report as usize;
    let wrong_thread_status = std::thread::spawn(move || {
        let mut generation = 0;
        let mut staged_core = ptr::null_mut();
        unsafe {
            inkpod_batch_report_take_staged_result(
                report_address as *mut InkpodBatchReport,
                0,
                &mut generation,
                &mut staged_core,
            )
        }
    })
    .join()
    .unwrap();
    assert_eq!(wrong_thread_status, INKPOD_STATUS_WRONG_THREAD);

    let mut generation = 0;
    let mut staged_core = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_batch_report_take_staged_result(report, 0, &mut generation, &mut staged_core)
        },
        INKPOD_STATUS_OK
    );
    assert_ne!(generation, 0);
    assert!(!staged_core.is_null());
    let mut second_core = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_batch_report_take_staged_result(report, 0, &mut generation, &mut second_core)
        },
        INKPOD_STATUS_INVALID_STATE
    );
    assert_eq!(
        unsafe { inkpod_core_destroy(&mut staged_core) },
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

    let mut preview_task = ptr::null_mut();
    assert_eq!(
        unsafe { inkpod_batch_task_create(&mut preview_task) },
        INKPOD_STATUS_OK
    );
    let mut preview_report = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_core_batch_contact_sheet_preview(
                &mut core,
                graph,
                preview_task,
                &mut preview_report,
            )
        },
        INKPOD_STATUS_OK
    );
    info.item_count = 0;
    info.failure_count = 0;
    info.staged_result_count = 0;
    assert_eq!(
        unsafe { inkpod_batch_report_get_info(preview_report, &mut info) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        (
            info.item_count,
            info.failure_count,
            info.staged_result_count
        ),
        (1, 0, 1)
    );
    let mut preview_generation = 0;
    let mut preview_core = ptr::null_mut();
    assert_eq!(
        unsafe {
            inkpod_batch_report_take_staged_result(
                preview_report,
                0,
                &mut preview_generation,
                &mut preview_core,
            )
        },
        INKPOD_STATUS_OK
    );
    assert_ne!(preview_generation, 0);
    assert!(!preview_core.is_null());
    assert!(
        !unsafe { &mut *preview_core }
            .core
            .document_info()
            .unwrap()
            .dirty
    );
    assert_eq!(
        unsafe { inkpod_core_destroy(&mut preview_core) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_report_release(&mut preview_report) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_task_release(&mut preview_task) },
        INKPOD_STATUS_OK
    );
    assert_eq!(
        unsafe { inkpod_batch_graph_release(&mut graph) },
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
    assert_ne!(candidate.flags & INKPOD_BATCH_PAIR_CANDIDATE_AMBIGUOUS, 0);
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
