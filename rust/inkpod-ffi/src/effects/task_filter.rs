use super::*;

/// Creates a one-shot thread-safe progress/cancellation task.
///
/// # Safety
/// `out_task` must be writable owner storage containing null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_task_create(out_task: *mut *mut InkpodTask) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_task.is_null() || !is_aligned(out_task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "task owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable owner storage.
        if !unsafe { out_task.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "task output already owns a handle",
            );
        }
        // SAFETY: The unique Rust owner is transferred to caller storage.
        unsafe { out_task.write(Box::into_raw(Box::new(InkpodTask::new()))) };
        INKPOD_STATUS_OK
    })
}

/// Queries an task from any thread.
///
/// # Safety
/// `task` must be a live handle and `out_info` a complete writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_task_query(
    task: *const InkpodTask,
    out_info: *mut InkpodTaskInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "task is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodTaskInfo") } {
            return status;
        }
        // SAFETY: Live task and writable complete output are required by contract.
        let task = unsafe { &*task };
        let output = unsafe { &mut *out_info };
        output.state = task.state.load(Ordering::Acquire);
        output.completed_work = task.completed_work.load(Ordering::Acquire);
        output.total_work = task.total_work.load(Ordering::Acquire);
        output.reserved = 0;
        INKPOD_STATUS_OK
    })
}

/// Requests cancellation from any thread. It is idempotent.
///
/// # Safety
/// `task` must be one live task handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_task_cancel(task: *mut InkpodTask) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "task is null or misaligned");
        }
        // SAFETY: A live task is required by contract and contains only atomics.
        let task = unsafe { &*task };
        task.cancelled.store(true, Ordering::Release);
        let _ = task.state.compare_exchange(
            INKPOD_TASK_READY,
            INKPOD_TASK_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        INKPOD_STATUS_OK
    })
}

/// Releases one Rust-owned task and nulls caller storage.
///
/// # Safety
/// Storage must contain null or one live, no-longer-borrowed task owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_task_release(task: *mut *mut InkpodTask) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "task owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable owner storage.
        let handle = unsafe { task.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "task handle is misaligned");
        }
        // SAFETY: Nulling precedes consuming the unique Box owner.
        unsafe { task.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Begins a non-committing filter preview from the current document state.
///
/// # Safety
/// Core/input/output must be complete, aligned, live, non-overlapping objects on
/// the Core owner thread. Any curve span is borrowed only for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_begin(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let filter = match unsafe { parse_filter_input(input) } {
            Ok(filter) => filter,
            Err(status) => return status,
        };
        match core.core.begin_filter_preview(input.plane_id, filter) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Begins a filter preview while publishing progress and honoring cancellation.
///
/// # Safety
/// The base preview requirements apply. `task` must be a live READY task kept
/// alive until this call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_begin_task(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    task: *mut InkpodTask,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let task = unsafe { &*task };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let filter = match unsafe { parse_filter_input(input) } {
            Ok(filter) => filter,
            Err(status) => return status,
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "task is not READY");
        }
        let status = match core.core.begin_filter_preview_with_progress(
            input.plane_id,
            filter,
            |completed, total| task.progress(completed, total),
        ) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}

/// Recomputes an active filter preview from its immutable base state.
///
/// # Safety
/// The same requirements as `inkpod_core_filter_preview_begin` apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_update(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let filter = match unsafe { parse_filter_input(input) } {
            Ok(filter) => filter,
            Err(status) => return status,
        };
        match core.core.update_filter_preview(input.plane_id, filter) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Updates a filter preview while publishing progress and honoring cancellation.
///
/// # Safety
/// The task and preview-begin-task requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_update_task(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    task: *mut InkpodTask,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let task = unsafe { &*task };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let filter = match unsafe { parse_filter_input(input) } {
            Ok(filter) => filter,
            Err(status) => return status,
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "task is not READY");
        }
        let status = match core.core.update_filter_preview_with_progress(
            input.plane_id,
            filter,
            |completed, total| task.progress(completed, total),
        ) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}

/// Cancels a preview without changing the document or history.
///
/// # Safety
/// Core/output must be complete, aligned, live, and non-overlapping on the Core
/// owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_cancel(
    core: *mut InkpodCore,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.cancel_filter_preview() {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits the current preview as one history unit.
///
/// # Safety
/// Core/result must be complete, aligned, live, and non-overlapping on the Core
/// owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_apply(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.apply_filter_preview() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies the last committed filter to another RGBA plane as one history unit.
///
/// # Safety
/// Core/result must satisfy the normal owner-thread dispatch contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_apply_last(
    core: *mut InkpodCore,
    plane_id: u64,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.apply_last_filter(plane_id) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies the last filter with progress/cancellation as one atomic history unit.
///
/// # Safety
/// Core/result follow the owner-thread contract. `task` must be a live READY
/// handle kept alive until this call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_apply_last_task(
    core: *mut InkpodCore,
    plane_id: u64,
    task: *mut InkpodTask,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let task = unsafe { &*task };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "task is not READY");
        }
        let status = match core
            .core
            .apply_last_filter_with_progress(plane_id, |completed, total| {
                task.progress(completed, total)
            }) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}
