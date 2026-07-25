use super::*;

/// Inverts, expands, or shrinks the persistent selection mask.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_selection_adjust(
    core: *mut InkpodCore,
    operation: u32,
    pixels: u32,
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
        let operation = match operation {
            INKPOD_SELECTION_ADJUST_INVERT => core.core.invert_selection(),
            INKPOD_SELECTION_ADJUST_EXPAND => match i32::try_from(pixels) {
                Ok(pixels) => core.core.resize_selection(pixels),
                Err(_) => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "expand count is not representable",
                    );
                }
            },
            INKPOD_SELECTION_ADJUST_SHRINK => match i32::try_from(pixels) {
                Ok(pixels) => core.core.resize_selection(-pixels),
                Err(_) => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "shrink count is not representable",
                    );
                }
            },
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection adjustment is not defined",
                );
            }
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

/// Clears the persistent selection mask as one undoable document transaction.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_selection_clear(
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
        match core.core.clear_selection() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Creates a typed selection layer from the persistent selection mask.
///
/// # Safety
/// `core` must be live on its owner thread, the advertised UTF-8 name range must
/// be readable, and `result` plus `out_layer_id` must be writable and
/// non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_selection_to_layer(
    core: *mut InkpodCore,
    name_utf8: *const u8,
    name_bytes: u64,
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
                "selection-layer pointer is invalid",
            );
        }
        // SAFETY: Output prefix and name bytes follow the exported contract.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let name = match unsafe { name_from_utf8(name_utf8, name_bytes) } {
            Ok(name) => name,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.selection_to_layer(name) {
            Ok((outcome, id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: out_layer_id is writable by contract.
                unsafe { out_layer_id.write(id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Combines a typed selection layer with the persistent selection mask.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_selection_from_layer(
    core: *mut InkpodCore,
    layer_id: u64,
    operation: u32,
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
        let operation = match operation {
            INKPOD_SELECTION_LAYER_REPLACE => SelectionLayerOperation::Replace,
            INKPOD_SELECTION_LAYER_ADD => SelectionLayerOperation::Add,
            INKPOD_SELECTION_LAYER_SUBTRACT => SelectionLayerOperation::Subtract,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection-layer operation is not defined",
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
        match core.core.selection_from_layer(layer_id, operation) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Adds one persistent document guide.
///
/// # Safety
/// `core` must be live on its owner thread and `result` plus `out_guide_id` must
/// be complete writable records that do not overlap Core storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_guide_add(
    core: *mut InkpodCore,
    axis: u32,
    position: i32,
    result: *mut InkpodDispatchResult,
    out_guide_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_guide_id.is_null()
            || !is_aligned(out_guide_id)
        {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "guide pointer is invalid");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let axis = match axis {
            INKPOD_GUIDE_HORIZONTAL => GuideAxis::Horizontal,
            INKPOD_GUIDE_VERTICAL => GuideAxis::Vertical,
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "guide axis is not defined"),
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.add_guide(axis, position) {
            Ok((outcome, id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: out_guide_id is writable by contract.
                unsafe { out_guide_id.write(id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Moves one persistent document guide.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_guide_move(
    core: *mut InkpodCore,
    guide_id: u64,
    position: i32,
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
        match core.core.move_guide(guide_id, position) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Deletes one persistent document guide.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_guide_delete(
    core: *mut InkpodCore,
    guide_id: u64,
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
        match core.core.delete_guide(guide_id) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces the persistent document grid configuration.
///
/// # Safety
/// `core` must be live on its owner thread, `input` must be fully readable, and
/// `result` must be writable and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_grid_set(
    core: *mut InkpodCore,
    input: *const InkpodGridInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodGridInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        if input.reserved != 0 || input.flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "grid input contains unsupported values",
            );
        }
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.set_grid(GridConfig {
            origin_x: input.origin_x,
            origin_y: input.origin_y,
            spacing_x: input.spacing_x,
            spacing_y: input.spacing_y,
            subdivisions: input.subdivisions,
        }) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples locator coordinates, selection bounds, and composite color.
///
/// # Safety
/// `core` must be live on its owner thread and `out_locator` must expose writable,
/// non-overlapping storage for a complete output record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_locator_sample(
    core: *mut InkpodCore,
    view_id: u64,
    device_x: f64,
    device_y: f64,
    out_locator: *mut InkpodLocatorOutput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(out_locator.cast_const(), "InkpodLocatorOutput") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_locator };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core
            .core
            .locator_sample((view_id != 0).then_some(view_id), device_x, device_y)
        {
            Ok(sample) => {
                output.flags = 0;
                output.document_x = sample.document_x;
                output.document_y = sample.document_y;
                output.selection = sample
                    .selection_bounds
                    .map_or(InkpodFrameRect::default(), frame_rect);
                if sample.selection_bounds.is_some() {
                    output.flags |= 1 << 0;
                }
                output.color = InkpodColorValue::default();
                output.color.struct_size = size_of::<InkpodColorValue>() as u32;
                if let Some(color) = sample.color {
                    if let Err(status) = write_color_value(&mut output.color, color) {
                        return status;
                    }
                    output.flags |= 1 << 1;
                }
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Rebinds one shortcut, replacing any conflicting binding deterministically.
///
/// # Safety
/// `core` must be a live handle used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_rebind(
    core: *mut InkpodCore,
    command_id: u32,
    virtual_key: u32,
    modifiers: u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.rebind_shortcut(ShortcutBinding {
            command_id,
            virtual_key,
            modifiers,
        }) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Resolves one normalized key chord through the current Core-owned bindings.
/// Zero means that the chord is currently unbound.
///
/// # Safety
/// `core` must be a live handle used on its owner thread and `out_command_id`
/// must point to writable `u32` storage that does not overlap the core.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_resolve(
    core: *mut InkpodCore,
    virtual_key: u32,
    modifiers: u32,
    out_command_id: *mut u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_command_id.is_null() || !is_aligned(out_command_id) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_command_id is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by contract.
        unsafe { out_command_id.write(0) };
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.resolve_shortcut(virtual_key, modifiers) {
            Ok(command_id) => {
                // SAFETY: Output storage was validated above.
                unsafe { out_command_id.write(command_id.unwrap_or(0)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Restores the built-in shortcut bindings.
///
/// # Safety
/// `core` must be a live handle used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_reset(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        core.core.reset_shortcuts();
        INKPOD_STATUS_OK
    })
}
