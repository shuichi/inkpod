use super::*;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_source_identity(
    core: *mut InkpodCore,
    sequence_index: u32,
    out_identity: *mut InkpodSequenceSourceIdentity,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(out_identity.cast_const(), "InkpodSequenceSourceIdentity") }
        {
            return status;
        }
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let cell = match core.core.sequence_cell_metadata(sequence_index as usize) {
            Ok(cell) => cell,
            Err(error) => return map_core_error(error),
        };
        let output = unsafe { &mut *out_identity };
        output.reserved = 0;
        output.document_uuid_high = (cell.document_uuid >> 64) as u64;
        output.document_uuid_low = cell.document_uuid as u64;
        output.source_generation = cell.source_generation;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_batch_extract_color_pairs(
    core: *mut InkpodCore,
    old_identity: *const InkpodSequenceSourceIdentity,
    new_identity: *const InkpodSequenceSourceIdentity,
    out_preview: *mut *mut InkpodBatchPairPreview,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_preview.is_null() || !is_aligned(out_preview)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch pair extraction pointer is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(old_identity, "old InkpodSequenceSourceIdentity") }
        {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(new_identity, "new InkpodSequenceSourceIdentity") }
        {
            return status;
        }
        if !unsafe { out_preview.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch pair preview output already owns a handle",
            );
        }
        let parse_identity = |record: &InkpodSequenceSourceIdentity| {
            let document_uuid = (u128::from(record.document_uuid_high) << 64)
                | u128::from(record.document_uuid_low);
            if record.reserved != 0 || document_uuid == 0 || record.source_generation == 0 {
                Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence source identity contains invalid fields",
                ))
            } else {
                Ok(SequenceSourceIdentity {
                    document_uuid,
                    source_generation: record.source_generation,
                })
            }
        };
        let old_identity = match parse_identity(unsafe { &*old_identity }) {
            Ok(identity) => identity,
            Err(status) => return status,
        };
        let new_identity = match parse_identity(unsafe { &*new_identity }) {
            Ok(identity) => identity,
            Err(status) => return status,
        };
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core
            .core
            .extract_batch_color_pairs(old_identity, new_identity)
        {
            Ok(extraction) => {
                unsafe {
                    out_preview.write(Box::into_raw(Box::new(InkpodBatchPairPreview {
                        extraction,
                    })))
                };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_pair_preview_get_info(
    preview: *const InkpodBatchPairPreview,
    out_info: *mut InkpodBatchPairPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch pair preview is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodBatchPairPreviewInfo") }
        {
            return status;
        }
        let extraction = &unsafe { &*preview }.extraction;
        let output = unsafe { &mut *out_info };
        output.pixel_format = storage_format_code(extraction.pixel_format);
        output.width = extraction.width;
        output.height = extraction.height;
        output.ambiguity_count = extraction.ambiguity_count;
        output.reserved = 0;
        output.candidate_count = extraction.candidates.len() as u64;
        output.unchanged_pixel_count = extraction.unchanged_pixel_count;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_pair_preview_get_candidate(
    preview: *const InkpodBatchPairPreview,
    index: u64,
    out_candidate: *mut InkpodBatchPairCandidate,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch pair preview is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_candidate.cast_const(), "InkpodBatchPairCandidate") }
        {
            return status;
        }
        let Ok(index) = usize::try_from(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch pair candidate index is not representable",
            );
        };
        let Some(candidate) = unsafe { &*preview }.extraction.candidates.get(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch pair candidate index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_candidate };
        output.flags = if candidate.ambiguous {
            INKPOD_BATCH_PAIR_CANDIDATE_AMBIGUOUS
        } else {
            0
        };
        output.old_color = match color_value_record(candidate.old) {
            Ok(color) => color,
            Err(status) => return status,
        };
        output.new_color = match color_value_record(candidate.new) {
            Ok(color) => color,
            Err(status) => return status,
        };
        output.pixel_count = candidate.pixel_count;
        output.bounds_x = candidate.affected_bounds.x;
        output.bounds_y = candidate.affected_bounds.y;
        output.bounds_width = candidate.affected_bounds.width;
        output.bounds_height = candidate.affected_bounds.height;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_pair_preview_release(
    preview: *mut *mut InkpodBatchPairPreview,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch pair preview owner pointer is null or misaligned",
            );
        }
        let handle = unsafe { preview.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch pair preview handle is misaligned",
            );
        }
        unsafe { preview.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_create(
    input: *const InkpodBatchGraphInput,
    out_graph: *mut *mut InkpodBatchGraph,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_graph.is_null() || !is_aligned(out_graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller supplies readable/writable owner storage.
        if !unsafe { out_graph.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch graph output already owns a handle",
            );
        }
        let graph = match unsafe { parse_graph_input(input) } {
            Ok(graph) => graph,
            Err(status) => return status,
        };
        if let Err(error) = graph.validate() {
            return map_core_error(error);
        }
        // SAFETY: A unique Rust owner is transferred to caller storage.
        unsafe { out_graph.write(Box::into_raw(Box::new(InkpodBatchGraph { graph }))) };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_load(
    path_utf8: *const u8,
    path_bytes: u64,
    out_graph: *mut *mut InkpodBatchGraph,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_graph.is_null() || !is_aligned(out_graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph owner pointer is null or misaligned",
            );
        }
        if !unsafe { out_graph.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch graph output already owns a handle",
            );
        }
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        match BatchGraph::load(path) {
            Ok(graph) => {
                unsafe { out_graph.write(Box::into_raw(Box::new(InkpodBatchGraph { graph }))) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_save(
    graph: *const InkpodBatchGraph,
    path_utf8: *const u8,
    path_bytes: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if graph.is_null() || !is_aligned(graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph is null or misaligned",
            );
        }
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        match unsafe { &*graph }.graph.save(path) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_info(
    graph: *const InkpodBatchGraph,
    out_info: *mut InkpodBatchGraphInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if graph.is_null() || !is_aligned(graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodBatchGraphInfo") }
        {
            return status;
        }
        let graph = &unsafe { &*graph }.graph;
        let output = unsafe { &mut *out_info };
        output.version = graph.version;
        output.input_count = graph.inputs.len() as u64;
        output.operation_count = graph.operations.len() as u64;
        output.output_destination = output_policy_value(graph.output.destination);
        output.output_format = output_format_value(graph.output.format);
        output.failure_policy = failure_policy_value(graph.output.failure_policy);
        output.output_flags = output_flags(&graph.output);
        output.name_utf8 = graph.name.as_bytes().as_ptr();
        output.name_bytes = graph.name.len() as u64;
        output.output_folder_utf8 = graph.output.folder.as_bytes().as_ptr();
        output.output_folder_bytes = graph.output.folder.len() as u64;
        output.naming_template_utf8 = graph.output.naming_template.as_bytes().as_ptr();
        output.naming_template_bytes = graph.output.naming_template.len() as u64;
        output.wait_milliseconds = graph.output.wait_milliseconds;
        output.reserved = 0;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_release(graph: *mut *mut InkpodBatchGraph) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if graph.is_null() || !is_aligned(graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph owner pointer is null or misaligned",
            );
        }
        let handle = unsafe { graph.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph handle is misaligned",
            );
        }
        unsafe { graph.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_batch_preview(
    core: *mut InkpodCore,
    graph: *const InkpodBatchGraph,
    run_scope: u32,
    out_preview: *mut *mut InkpodBatchPreview,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || graph.is_null()
            || !is_aligned(graph)
            || out_preview.is_null()
            || !is_aligned(out_preview)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview handle pointer is null or misaligned",
            );
        }
        if !unsafe { out_preview.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch preview output already owns a handle",
            );
        }
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let scope = match scope(run_scope) {
            Ok(scope) => scope,
            Err(status) => return status,
        };
        match core.core.batch_preview(&unsafe { &*graph }.graph, scope) {
            Ok(preview) => {
                let items = preview
                    .items
                    .into_iter()
                    .map(|item| OwnedPreviewItem {
                        input_name: item.input_name.into_bytes().into_boxed_slice(),
                        output_path: bytes_for_path(item.output_path),
                        warning: item.warnings.join("\n").into_bytes().into_boxed_slice(),
                    })
                    .collect();
                unsafe { out_preview.write(Box::into_raw(Box::new(InkpodBatchPreview { items }))) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_preview_count(
    preview: *const InkpodBatchPreview,
    out_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null()
            || !is_aligned(preview)
            || out_count.is_null()
            || !is_aligned(out_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview or count pointer is null or misaligned",
            );
        }
        unsafe { out_count.write((&*preview).items.len() as u64) };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_preview_get(
    preview: *const InkpodBatchPreview,
    index: u64,
    out_item: *mut InkpodBatchPreviewItem,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_item.cast_const(), "InkpodBatchPreviewItem") }
        {
            return status;
        }
        let Ok(index) = usize::try_from(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview index is not representable",
            );
        };
        let Some(item) = unsafe { &*preview }.items.get(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_item };
        output.flags = if item.warning.is_empty() {
            0
        } else {
            INKPOD_BATCH_PREVIEW_HAS_WARNING
        };
        output.input_name = item.input_name.as_ptr();
        output.input_name_bytes = item.input_name.len() as u64;
        output.output_path = item.output_path.as_ptr();
        output.output_path_bytes = item.output_path.len() as u64;
        output.warning = item.warning.as_ptr();
        output.warning_bytes = item.warning.len() as u64;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_preview_release(
    preview: *mut *mut InkpodBatchPreview,
) -> u32 {
    // SAFETY: Forwarded from this exported ownership contract.
    unsafe { release_preview(preview) }
}

unsafe fn release_preview(preview: *mut *mut InkpodBatchPreview) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview owner pointer is null or misaligned",
            );
        }
        let handle = unsafe { preview.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview handle is misaligned",
            );
        }
        unsafe { preview.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_batch_execute(
    core: *mut InkpodCore,
    graph: *const InkpodBatchGraph,
    run_scope: u32,
    flags: u64,
    task: *mut InkpodTask,
    out_report: *mut *mut InkpodBatchReport,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || graph.is_null()
            || !is_aligned(graph)
            || task.is_null()
            || !is_aligned(task)
            || out_report.is_null()
            || !is_aligned(out_report)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch execute handle pointer is null or misaligned",
            );
        }
        if flags & !(INKPOD_BATCH_RUN_DRY | INKPOD_BATCH_RUN_PREVIEW_CONFIRMED) != 0 {
            return fail(INKPOD_STATUS_UNSUPPORTED, "batch run flags are unsupported");
        }
        if !unsafe { out_report.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch report output already owns a handle",
            );
        }
        let core = unsafe { &mut *core };
        let task = unsafe { &*task };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let scope = match scope(run_scope) {
            Ok(scope) => scope,
            Err(status) => return status,
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "batch task is not READY");
        }
        let result = core.core.batch_execute(
            &unsafe { &*graph }.graph,
            BatchRunOptions {
                scope,
                dry_run: flags & INKPOD_BATCH_RUN_DRY != 0,
                preview_confirmed: flags & INKPOD_BATCH_RUN_PREVIEW_CONFIRMED != 0,
            },
            |completed, total| task.progress(completed, total),
        );
        finish_batch_report(result, task, out_report)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_batch_contact_sheet_preview(
    core: *mut InkpodCore,
    graph: *const InkpodBatchGraph,
    task: *mut InkpodTask,
    out_report: *mut *mut InkpodBatchReport,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || graph.is_null()
            || !is_aligned(graph)
            || task.is_null()
            || !is_aligned(task)
            || out_report.is_null()
            || !is_aligned(out_report)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch contact-sheet preview handle pointer is null or misaligned",
            );
        }
        if !unsafe { out_report.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch contact-sheet preview report output already owns a handle",
            );
        }
        let core = unsafe { &mut *core };
        let task = unsafe { &*task };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if !task.begin() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch contact-sheet preview task is not READY",
            );
        }
        let result = core
            .core
            .batch_contact_sheet_preview(&unsafe { &*graph }.graph, |completed, total| {
                task.progress(completed, total)
            });
        finish_batch_report(result, task, out_report)
    })
}

fn finish_batch_report(
    result: Result<BatchRunReport, CoreError>,
    task: &InkpodTask,
    out_report: *mut *mut InkpodBatchReport,
) -> u32 {
    match result {
        Ok(report) => {
            let cancelled = report.cancelled;
            let staged_results = report.staged_results.into_iter().map(Some).collect();
            let items = report
                .items
                .into_iter()
                .map(|item| OwnedReportItem {
                    outcome: match item.outcome {
                        BatchItemOutcome::Succeeded => INKPOD_BATCH_ITEM_SUCCEEDED,
                        BatchItemOutcome::Skipped => INKPOD_BATCH_ITEM_SKIPPED,
                        BatchItemOutcome::Failed => INKPOD_BATCH_ITEM_FAILED,
                        BatchItemOutcome::Cancelled => INKPOD_BATCH_ITEM_CANCELLED,
                        BatchItemOutcome::DryRun => INKPOD_BATCH_ITEM_DRY_RUN,
                    },
                    input_name: item.input_name.into_bytes().into_boxed_slice(),
                    output_path: bytes_for_path(item.output_path),
                    message: item.message.into_bytes().into_boxed_slice(),
                })
                .collect();
            // SAFETY: Both exported callers validate this pointer, its alignment, and
            // empty owner slot before beginning the one-shot task.
            unsafe {
                out_report.write(Box::into_raw(Box::new(InkpodBatchReport {
                    items,
                    cancelled,
                    owner_thread: thread::current().id(),
                    staged_results,
                })))
            };
            let status = if cancelled {
                INKPOD_STATUS_CANCELLED
            } else {
                INKPOD_STATUS_OK
            };
            task.finish(status);
            status
        }
        Err(error) => {
            let status = map_core_error(error);
            task.finish(status);
            status
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_report_get_info(
    report: *const InkpodBatchReport,
    out_info: *mut InkpodBatchReportInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if report.is_null() || !is_aligned(report) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodBatchReportInfo") }
        {
            return status;
        }
        let report = unsafe { &*report };
        let output = unsafe { &mut *out_info };
        output.cancelled = u32::from(report.cancelled);
        output.item_count = report.items.len() as u64;
        output.failure_count = report
            .items
            .iter()
            .filter(|item| item.outcome == INKPOD_BATCH_ITEM_FAILED)
            .count() as u64;
        output.staged_result_count = report.staged_results.len() as u64;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_report_get(
    report: *const InkpodBatchReport,
    index: u64,
    out_item: *mut InkpodBatchReportItem,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if report.is_null() || !is_aligned(report) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_item.cast_const(), "InkpodBatchReportItem") }
        {
            return status;
        }
        let Ok(index) = usize::try_from(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report index is not representable",
            );
        };
        let Some(item) = unsafe { &*report }.items.get(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_item };
        output.outcome = item.outcome;
        output.input_name = item.input_name.as_ptr();
        output.input_name_bytes = item.input_name.len() as u64;
        output.output_path = item.output_path.as_ptr();
        output.output_path_bytes = item.output_path.len() as u64;
        output.message = item.message.as_ptr();
        output.message_bytes = item.message.len() as u64;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_report_take_staged_result(
    report: *mut InkpodBatchReport,
    index: u64,
    out_generation: *mut u64,
    out_core: *mut *mut InkpodCore,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if report.is_null()
            || !is_aligned(report)
            || out_generation.is_null()
            || !is_aligned(out_generation)
            || out_core.is_null()
            || !is_aligned(out_core)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch staged-result pointer is null or misaligned",
            );
        }
        if !unsafe { out_core.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch staged-result output already owns a Core handle",
            );
        }
        let report = unsafe { &mut *report };
        if report.owner_thread != thread::current().id() {
            return fail(
                INKPOD_STATUS_WRONG_THREAD,
                "batch staged result must be taken on the report owner thread",
            );
        }
        let Ok(index) = usize::try_from(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch staged-result index is not representable",
            );
        };
        let Some(slot) = report.staged_results.get_mut(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch staged-result index is outside bounds",
            );
        };
        let Some(result) = slot.take() else {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch staged result was already taken",
            );
        };
        let objects = match crate::v3::ObjectRegistry::new() {
            Some(objects) => objects,
            None => {
                *slot = Some(result);
                return fail(
                    INKPOD_STATUS_INVALID_STATE,
                    "ABI-v3 Core generation space is exhausted",
                );
            }
        };
        let generation = result.generation();
        let handle = Box::new(InkpodCore {
            owner_thread: thread::current().id(),
            core: result.into_core(),
            objects,
        });
        unsafe {
            out_generation.write(generation);
            out_core.write(Box::into_raw(handle));
        }
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_report_release(report: *mut *mut InkpodBatchReport) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if report.is_null() || !is_aligned(report) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report owner pointer is null or misaligned",
            );
        }
        let handle = unsafe { report.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report handle is misaligned",
            );
        }
        unsafe { report.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_task_create(out_task: *mut *mut InkpodTask) -> u32 {
    // SAFETY: This is the same thread-safe task layout and ownership contract.
    unsafe { inkpod_task_create(out_task) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_task_query(
    task: *const InkpodTask,
    out_info: *mut InkpodTaskInfo,
) -> u32 {
    // SAFETY: This is the same thread-safe task layout and query contract.
    unsafe { inkpod_task_query(task, out_info) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_task_cancel(task: *mut InkpodTask) -> u32 {
    // SAFETY: This is the same thread-safe task layout and cancellation contract.
    unsafe { inkpod_task_cancel(task) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_task_release(task: *mut *mut InkpodTask) -> u32 {
    // SAFETY: This is the same thread-safe task layout and ownership contract.
    unsafe { inkpod_task_release(task) }
}
