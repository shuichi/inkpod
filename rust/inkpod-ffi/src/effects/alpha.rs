use super::*;

/// Replaces only the target plane alpha from copied grayscale rows.
///
/// # Safety
/// Core/input/result and every advertised pixel row must be complete, readable,
/// non-overlapping, and live for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_alpha_edit(
    core: *mut InkpodCore,
    input: *const InkpodAlphaEditInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodAlphaEditInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let alpha = match unsafe { parse_alpha_edit_input(input) } {
            Ok(alpha) => alpha,
            Err(status) => return status,
        };
        match core.core.edit_plane_alpha(input.plane_id, &alpha) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a multi-stop gradient to alpha only, preserving every RGB channel.
///
/// # Safety
/// The gradient-effect safety requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_alpha_gradient(
    core: *mut InkpodCore,
    input: *const InkpodGradientInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodGradientInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete records and borrowed stop span are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let gradient = match unsafe { parse_gradient_input(input) } {
            Ok(value) => value,
            Err(status) => return status,
        };
        match core
            .core
            .apply_alpha_gradient_to_plane(input.plane_id, &gradient)
        {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
