use super::*;

/// Saves to a UTF-8 path using same-directory temporary-file replacement.
///
/// # Safety
/// Path bytes must remain readable, and all object/output pointers must remain
/// live and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_save(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { file_operation(core, path_utf8, path_bytes, out_info, false) }
}

/// Opens a versioned `.inkpod` file from a UTF-8 path.
///
/// # Safety
/// Path bytes must remain readable, and all object/output pointers must remain
/// live and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_open(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { file_operation(core, path_utf8, path_bytes, out_info, true) }
}

/// Writes a recovery container atomically without changing normal savepoint or
/// normal path.
///
/// # Safety
/// Path/Core/output follow the same contract as `inkpod_core_save`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_autosave(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { recovery_file_operation(core, path_utf8, path_bytes, out_info, false) }
}

/// Opens recovery content as a dirty, pathless document. It never inherits the
/// recovered file's former normal-save destination.
///
/// # Safety
/// Path/Core/output follow the same contract as `inkpod_core_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_open_recovery(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { recovery_file_operation(core, path_utf8, path_bytes, out_info, true) }
}

unsafe fn recovery_file_operation(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
    recover: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: The path range is readable for this call by contract.
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let operation = if recover {
            core.core.open_recovery(path)
        } else {
            core.core.autosave(path)
        };
        match operation {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

unsafe fn file_operation(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
    open: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: The path range is readable for this call by contract.
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let operation = if open {
            core.core.open(path)
        } else {
            core.core.save(path)
        };
        match operation {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Reopens the last normal-save path and discards unsaved changes.
///
/// # Safety
/// Core/output must be live owner-thread objects with non-overlapping storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_revert(
    core: *mut InkpodCore,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.revert() {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
