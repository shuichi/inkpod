use super::*;

/// Mirrors persistent document content as one history transaction.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_mirror_document(
    core: *mut InkpodCore,
    axis: u32,
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
        let axis = match axis {
            1 => MirrorAxis::Horizontal,
            2 => MirrorAxis::Vertical,
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "mirror axis is not defined"),
        };
        match core.core.mirror_document(axis) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Rotates persistent document content and metadata as one history transaction.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_rotate_document(
    core: *mut InkpodCore,
    direction: u32,
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
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let direction = match direction {
            1 => RotateDirection::Left90,
            2 => RotateDirection::Right90,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "rotate direction is not defined",
                );
            }
        };
        match core.core.rotate_document(direction) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Resizes/repositions or nearest-neighbor resamples persistent document data.
///
/// # Safety
/// `core`, `input`, and `result` must be complete, live, aligned,
/// non-overlapping records used on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_resize_document(
    core: *mut InkpodCore,
    input: *const InkpodDocumentResizeInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodDocumentResizeInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete records were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.flags & !1 != 0 {
            return fail(INKPOD_STATUS_UNSUPPORTED, "resize flags are not supported");
        }
        let anchor = match input.anchor {
            1 => ResizeAnchor::TopLeft,
            2 => ResizeAnchor::TopRight,
            3 => ResizeAnchor::Center,
            4 => ResizeAnchor::BottomLeft,
            5 => ResizeAnchor::BottomRight,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "resize anchor is not defined",
                );
            }
        };
        match core.core.resize_document(DocumentResize {
            width: input.width,
            height: input.height,
            dpi_x_milli: input.dpi_x_milli,
            dpi_y_milli: input.dpi_y_milli,
            resample: input.flags & 1 != 0,
            anchor,
        }) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
