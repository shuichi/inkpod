use super::*;

/// Creates a secondary logical view of the current document.
///
/// # Safety
/// `core` must be live on its owner thread and `out_view_id` must be writable,
/// non-overlapping storage for one identifier.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_view_create(
    core: *mut InkpodCore,
    out_view_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_view_id.is_null() || !is_aligned(out_view_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "view create pointer is null or misaligned",
            );
        }
        // SAFETY: Live core and writable ID storage are required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.create_view() {
            Ok(id) => {
                // SAFETY: out_view_id is writable by contract.
                unsafe { out_view_id.write(id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a logical view command to one secondary view only.
///
/// # Safety
/// `core` must be live on its owner thread and `input` must expose a complete,
/// readable, non-overlapping record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_view_apply(
    core: *mut InkpodCore,
    view_id: u64,
    input: *const InkpodViewInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodViewInput") } {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let command = match parse_view_command(&core.core, input) {
            Ok(command) => command,
            Err(status) => return status,
        };
        match core.core.apply_view_for(view_id, command) {
            Ok(_) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Closes one secondary logical view.
///
/// # Safety
/// `core` must be a live handle used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_view_close(core: *mut InkpodCore, view_id: u64) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A live owner-thread handle is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.close_view(view_id) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Builds one immutable snapshot using a secondary view transform.
///
/// # Safety
/// `core` must be live on its owner thread, `options` must be a complete readable
/// record, and `out_snapshot` must be writable result storage that does not
/// currently contain a live snapshot handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_build_snapshot_for_view(
    core: *mut InkpodCore,
    view_id: u64,
    options: *const InkpodSnapshotOptions,
    out_snapshot: *mut *mut InkpodSnapshot,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_snapshot.is_null()
            || !is_aligned(out_snapshot)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "multi-view snapshot pointer is invalid",
            );
        }
        // SAFETY: Public structure exposes a readable size prefix.
        if let Err(status) = unsafe { validate_struct(options, "InkpodSnapshotOptions") } {
            return status;
        }
        // SAFETY: Caller provides writable output handle storage.
        unsafe { out_snapshot.write(ptr::null_mut()) };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let options = unsafe { &*options };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "snapshot options contain unsupported values",
            );
        }
        match core.core.build_snapshot_for(view_id) {
            Ok(snapshot) => {
                // SAFETY: Output storage receives exactly one Rust Box owner.
                unsafe { out_snapshot.write(Box::into_raw(snapshot_handle(snapshot))) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
