use super::*;

/// Undoes one committed transaction.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_undo(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { history_operation(core, result, false) }
}

/// Redoes one committed transaction.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_redo(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { history_operation(core, result, true) }
}

unsafe fn history_operation(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
    redo: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let operation = if redo {
            core.core.redo()
        } else {
            core.core.undo()
        };
        match operation {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Queries the current history cursor and bounded item count.
///
/// # Safety
/// Core and output must be live, aligned owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_history_info(
    core: *mut InkpodCore,
    out_info: *mut InkpodHistoryInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodHistoryInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let out = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        out.reserved = 0;
        out.cursor = core.core.history_cursor() as u64;
        out.item_count = core.core.history_entries().len() as u64;
        INKPOD_STATUS_OK
    })
}

/// Queries one history label into caller-owned UTF-8 storage.
///
/// # Safety
/// Core/output and any advertised name buffer must remain live and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_history_item(
    core: *mut InkpodCore,
    index: u64,
    out_item: *mut InkpodHistoryItem,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_item.cast_const(), "InkpodHistoryItem") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let out = unsafe { &mut *out_item };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let index = match usize::try_from(index) {
            Ok(value) => value,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "history index is not representable",
                );
            }
        };
        let entries = core.core.history_entries();
        let Some(entry) = entries.get(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history index is outside the available range",
            );
        };
        out.flags = if entry.applied {
            INKPOD_HISTORY_ITEM_APPLIED
        } else {
            0
        };
        out.index = entry.index as u64;
        out.name_bytes = entry.label.len() as u64;
        if out.name_capacity == 0 {
            if !out.name_utf8.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity history label buffer must be null",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if out.name_utf8.is_null() || out.name_capacity < out.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "history label buffer is too small",
            );
        }
        // SAFETY: Caller advertises complete writable storage for the copied label.
        unsafe { ptr::copy_nonoverlapping(entry.label.as_ptr(), out.name_utf8, entry.label.len()) };
        INKPOD_STATUS_OK
    })
}

/// Moves the history cursor to any available state.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_history_jump(
    core: *mut InkpodCore,
    target_cursor: u64,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let target = match usize::try_from(target_cursor) {
            Ok(value) => value,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "history target is not representable",
                );
            }
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.jump_history(target) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Restores the active plane inside the persistent selection from the normal savepoint.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_revert_active_selection(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.revert_active_plane_selection() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
