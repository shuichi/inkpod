use super::*;

/// Creates a persisted, non-destructive adjustment layer. Name and curve
/// storage are copied before return.
///
/// # Safety
/// All advertised objects/spans must be complete, aligned, live, and
/// non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_adjustment_create(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    name_utf8: *const u8,
    name_length: u64,
    result: *mut InkpodDispatchResult,
    out_layer_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_layer_id.is_null()
            || !is_aligned(out_layer_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "adjustment core or layer output is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by contract.
        unsafe { out_layer_id.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        if name_utf8.is_null()
            || name_length == 0
            || name_length > 1_024
            || usize::try_from(name_length).is_err()
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "adjustment name span is invalid",
            );
        }
        // SAFETY: The caller advertises a bounded readable byte span borrowed
        // only for this call.
        let name_bytes = unsafe { slice::from_raw_parts(name_utf8, name_length as usize) };
        let name = match std::str::from_utf8(name_bytes) {
            Ok(name) => name,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "adjustment name is not UTF-8",
                );
            }
        };
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let adjustment = match unsafe { parse_filter_input(input) }.and_then(filter_to_adjustment) {
            Ok(adjustment) => adjustment,
            Err(status) => return status,
        };
        match core.core.create_adjustment_layer(name, adjustment) {
            Ok((outcome, layer_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable aligned storage was validated above.
                unsafe { out_layer_id.write(layer_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces one persisted adjustment parameter record as one Undo unit.
///
/// # Safety
/// All objects and optional curve storage follow the owner-thread, alignment,
/// non-overlap, and per-call borrowing contract used by adjustment creation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_adjustment_update(
    core: *mut InkpodCore,
    layer_id: u64,
    input: *const InkpodFilterInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
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
        let adjustment = match unsafe { parse_filter_input(input) }.and_then(filter_to_adjustment) {
            Ok(adjustment) => adjustment,
            Err(status) => return status,
        };
        match core.core.update_adjustment_layer(layer_id, adjustment) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
