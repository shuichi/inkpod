use super::*;

/// Returns deterministic native-format/checkpoint policy diagnostics.
///
/// # Safety
/// Core/output must be live owner-thread objects with non-overlapping storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_persistence_info(
    core: *mut InkpodCore,
    out_info: *mut InkpodPersistenceInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodPersistenceInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &*core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.persistence_info() {
            Ok(info) => {
                output.format_version = info.format_version;
                output.open_strategy = match info.open_strategy {
                    inkpod_core::NativeOpenStrategy::NotOpened => INKPOD_NATIVE_OPEN_NOT_OPENED,
                    inkpod_core::NativeOpenStrategy::FullReplay => INKPOD_NATIVE_OPEN_FULL_REPLAY,
                    inkpod_core::NativeOpenStrategy::Checkpoint => INKPOD_NATIVE_OPEN_CHECKPOINT,
                };
                output.flags = if info.checkpoint_due {
                    INKPOD_PERSISTENCE_CHECKPOINT_DUE
                } else {
                    0
                };
                output.feature_flags = INKPOD_FEATURE_NONE;
                output.journal_event_count = info.journal_event_count;
                output.procedure_count = info.procedure_count;
                output.replay_work = info.replay_work;
                output.dirty_bytes = info.dirty_bytes;
                output.asset_count = info.asset_count;
                output.asset_bytes = info.asset_bytes;
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Returns the exact history-loss confirmation token for a compacted copy.
///
/// # Safety
/// Core/output must be live owner-thread objects with non-overlapping storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_compaction_plan(
    core: *mut InkpodCore,
    out_plan: *mut InkpodCompactionPlan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(out_plan.cast_const(), "InkpodCompactionPlan") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &*core };
        let output = unsafe { &mut *out_plan };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.compaction_plan() {
            Ok(plan) => {
                write_compaction_plan(output, plan);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Writes a separate compacted copy after exact token confirmation.
///
/// # Safety
/// Path and plan bytes must remain readable, and Core must be a live owner-thread
/// object for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_write_compacted_copy(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    plan: *const InkpodCompactionPlan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The input exposes a readable public-structure prefix.
        if let Err(status) = unsafe { validate_struct(plan, "InkpodCompactionPlan") } {
            return status;
        }
        // SAFETY: The path range is readable for this call by contract.
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &*core };
        let input = unsafe { &*plan };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0 || input.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "compaction plan contains unsupported flags or reserved values",
            );
        }
        let current = match core.core.compaction_plan() {
            Ok(plan) => plan,
            Err(error) => return map_core_error(error),
        };
        if !compaction_plan_matches(input, current) {
            return fail(INKPOD_STATUS_INVALID_STATE, "compaction plan is stale");
        }
        match core.core.write_compacted_copy(path, current) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

fn write_compaction_plan(output: &mut InkpodCompactionPlan, plan: inkpod_core::CompactionPlan) {
    output.reserved = 0;
    output.feature_flags = INKPOD_FEATURE_NONE;
    output.history_event_count = plan.history_event_count;
    output.history_procedure_count = plan.history_procedure_count;
    output.document_digest = *plan.document_digest.as_bytes();
    output.editor_digest = *plan.editor_digest.as_bytes();
    output.journal_digest = plan.journal_digest;
}

fn compaction_plan_matches(
    input: &InkpodCompactionPlan,
    plan: inkpod_core::CompactionPlan,
) -> bool {
    input.history_event_count == plan.history_event_count
        && input.history_procedure_count == plan.history_procedure_count
        && input.document_digest == *plan.document_digest.as_bytes()
        && input.editor_digest == *plan.editor_digest.as_bytes()
        && input.journal_digest == plan.journal_digest
}

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

/// Writes a prospective normal-save file without advancing live savepoints.
///
/// # Safety
/// Core/path must remain live for this call. `out_prepared` must be a writable,
/// aligned owner slot; success transfers one Rust-owned token to that slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_prepare_save(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_prepared: *mut *mut InkpodPreparedSave,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_prepared.is_null() || !is_aligned(out_prepared) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "prepared save owner pointer is null or misaligned",
            );
        }
        // SAFETY: The caller provides one writable owner slot.
        unsafe { out_prepared.write(ptr::null_mut()) };
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The path range is readable for this call by contract.
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        // SAFETY: The caller contract requires a live Core owner-thread handle.
        let core = unsafe { &*core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.prepare_save(path) {
            Ok(token) => {
                let prepared = Box::new(InkpodPreparedSave { token });
                // SAFETY: The output owner slot was validated above.
                unsafe { out_prepared.write(Box::into_raw(prepared)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits a prepared normal save after platform publication succeeds.
///
/// # Safety
/// Core/path/token/output must remain live, aligned, and non-overlapping for
/// this call. Core must be called on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_commit_prepared_save(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    prepared: *const InkpodPreparedSave,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if prepared.is_null() || !is_aligned(prepared) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "prepared save token is null or misaligned",
            );
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
        // SAFETY: All complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let prepared = unsafe { &*prepared };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.commit_prepared_save(path, prepared.token) {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Releases one Rust-owned prepared-save token and clears its owner slot.
///
/// # Safety
/// `prepared` must be a writable aligned owner slot containing one live token.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_prepared_save_release(
    prepared: *mut *mut InkpodPreparedSave,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if prepared.is_null() || !is_aligned(prepared) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "prepared save owner pointer is null or misaligned",
            );
        }
        // SAFETY: The caller provides one readable/writable owner slot.
        let handle = unsafe { prepared.read() };
        if handle.is_null() || !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "prepared save handle is null or misaligned",
            );
        }
        // SAFETY: Ownership is unique and transferred back for this release.
        unsafe {
            drop(Box::from_raw(handle));
            prepared.write(ptr::null_mut());
        }
        INKPOD_STATUS_OK
    })
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
