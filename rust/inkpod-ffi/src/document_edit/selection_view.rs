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

/// Deletes every persistent document guide as one canonical primitive.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_guide_delete_all(
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
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.delete_all_guides() {
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

/// Copies a bounded composite RGBA8 neighborhood around a locator point.
///
/// # Safety
/// `core` must be live on its owner thread and `output` must expose a complete
/// writable record. Non-zero output capacity must describe writable,
/// non-overlapping byte storage valid for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_locator_neighborhood(
    core: *mut InkpodCore,
    view_id: u64,
    device_x: f64,
    device_y: f64,
    output: *mut InkpodLocatorNeighborhoodBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodLocatorNeighborhoodBuffer") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if output.reserved != 0 || output.reserved_2 != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "locator neighborhood reserved fields must be zero",
            );
        }
        let neighborhood = match core.core.locator_neighborhood(
            (view_id != 0).then_some(view_id),
            device_x,
            device_y,
            output.radius,
        ) {
            Ok(neighborhood) => neighborhood,
            Err(error) => return map_core_error(error),
        };
        output.width = neighborhood.width;
        output.height = neighborhood.height;
        output.origin_x = neighborhood.origin_x;
        output.origin_y = neighborhood.origin_y;
        output.required_bytes = neighborhood.pixels_rgba8.len() as u64;
        if output.pixel_capacity == 0 {
            if !output.pixels_rgba8.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity locator neighborhood buffer must be null",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if output.pixels_rgba8.is_null() || output.pixel_capacity > isize::MAX as u64 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "locator neighborhood output storage is invalid",
            );
        }
        if output.pixel_capacity < output.required_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "locator neighborhood output storage is too small",
            );
        }
        // SAFETY: The caller advertises enough writable, non-overlapping byte storage.
        unsafe {
            ptr::copy_nonoverlapping(
                neighborhood.pixels_rgba8.as_ptr(),
                output.pixels_rgba8,
                neighborhood.pixels_rgba8.len(),
            )
        };
        INKPOD_STATUS_OK
    })
}

fn shortcut_named_key(value: u32) -> Option<ShortcutNamedKey> {
    match value {
        INKPOD_SHORTCUT_NAMED_TAB => Some(ShortcutNamedKey::Tab),
        INKPOD_SHORTCUT_NAMED_RETURN => Some(ShortcutNamedKey::Return),
        INKPOD_SHORTCUT_NAMED_ESCAPE => Some(ShortcutNamedKey::Escape),
        INKPOD_SHORTCUT_NAMED_SPACE => Some(ShortcutNamedKey::Space),
        INKPOD_SHORTCUT_NAMED_BACKSPACE => Some(ShortcutNamedKey::Backspace),
        INKPOD_SHORTCUT_NAMED_DELETE => Some(ShortcutNamedKey::Delete),
        INKPOD_SHORTCUT_NAMED_LEFT => Some(ShortcutNamedKey::Left),
        INKPOD_SHORTCUT_NAMED_RIGHT => Some(ShortcutNamedKey::Right),
        INKPOD_SHORTCUT_NAMED_UP => Some(ShortcutNamedKey::Up),
        INKPOD_SHORTCUT_NAMED_DOWN => Some(ShortcutNamedKey::Down),
        INKPOD_SHORTCUT_NAMED_HOME => Some(ShortcutNamedKey::Home),
        INKPOD_SHORTCUT_NAMED_END => Some(ShortcutNamedKey::End),
        INKPOD_SHORTCUT_NAMED_PAGE_UP => Some(ShortcutNamedKey::PageUp),
        INKPOD_SHORTCUT_NAMED_PAGE_DOWN => Some(ShortcutNamedKey::PageDown),
        INKPOD_SHORTCUT_NAMED_F1..=INKPOD_SHORTCUT_NAMED_F24 => Some(ShortcutNamedKey::Function(
            (value - INKPOD_SHORTCUT_NAMED_F1 + 1) as u8,
        )),
        _ => None,
    }
}

fn shortcut_named_key_value(value: ShortcutNamedKey) -> u32 {
    match value {
        ShortcutNamedKey::Tab => INKPOD_SHORTCUT_NAMED_TAB,
        ShortcutNamedKey::Return => INKPOD_SHORTCUT_NAMED_RETURN,
        ShortcutNamedKey::Escape => INKPOD_SHORTCUT_NAMED_ESCAPE,
        ShortcutNamedKey::Space => INKPOD_SHORTCUT_NAMED_SPACE,
        ShortcutNamedKey::Backspace => INKPOD_SHORTCUT_NAMED_BACKSPACE,
        ShortcutNamedKey::Delete => INKPOD_SHORTCUT_NAMED_DELETE,
        ShortcutNamedKey::Left => INKPOD_SHORTCUT_NAMED_LEFT,
        ShortcutNamedKey::Right => INKPOD_SHORTCUT_NAMED_RIGHT,
        ShortcutNamedKey::Up => INKPOD_SHORTCUT_NAMED_UP,
        ShortcutNamedKey::Down => INKPOD_SHORTCUT_NAMED_DOWN,
        ShortcutNamedKey::Home => INKPOD_SHORTCUT_NAMED_HOME,
        ShortcutNamedKey::End => INKPOD_SHORTCUT_NAMED_END,
        ShortcutNamedKey::PageUp => INKPOD_SHORTCUT_NAMED_PAGE_UP,
        ShortcutNamedKey::PageDown => INKPOD_SHORTCUT_NAMED_PAGE_DOWN,
        ShortcutNamedKey::Function(value) => INKPOD_SHORTCUT_NAMED_F1 + u32::from(value) - 1,
    }
}

unsafe fn parse_shortcut_stroke(
    stroke: *const InkpodShortcutStrokeV2,
) -> Result<ShortcutStroke, u32> {
    if stroke.is_null() || !is_aligned(stroke) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shortcut stroke is null or misaligned",
        ));
    }
    // SAFETY: The public contract requires a readable size prefix.
    unsafe { validate_struct(stroke, "InkpodShortcutStrokeV2")? };
    // SAFETY: Full size and alignment were validated above.
    let record = unsafe { stroke.read() };
    let key = match record.key_kind {
        INKPOD_SHORTCUT_KEY_UNICODE_SCALAR => char::from_u32(record.key_value)
            .map(ShortcutKey::UnicodeScalar)
            .ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "shortcut Unicode key is not a scalar",
                )
            })?,
        INKPOD_SHORTCUT_KEY_NAMED => shortcut_named_key(record.key_value)
            .map(ShortcutKey::Named)
            .ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "shortcut named key is not defined",
                )
            })?,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shortcut key kind is not defined",
            ));
        }
    };
    if record.modifiers
        & !(INKPOD_SHORTCUT_MODIFIER_PRIMARY
            | INKPOD_SHORTCUT_MODIFIER_SHIFT
            | INKPOD_SHORTCUT_MODIFIER_ALTERNATE
            | INKPOD_SHORTCUT_MODIFIER_CONTROL)
        != 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shortcut modifiers contain unsupported bits",
        ));
    }
    Ok(ShortcutStroke {
        key,
        modifiers: record.modifiers,
    })
}

fn write_shortcut_stroke(stroke: ShortcutStroke) -> InkpodShortcutStrokeV2 {
    let (key_kind, key_value) = match stroke.key {
        ShortcutKey::UnicodeScalar(value) => (INKPOD_SHORTCUT_KEY_UNICODE_SCALAR, u32::from(value)),
        ShortcutKey::Named(value) => (INKPOD_SHORTCUT_KEY_NAMED, shortcut_named_key_value(value)),
    };
    InkpodShortcutStrokeV2 {
        struct_size: size_of::<InkpodShortcutStrokeV2>() as u32,
        key_kind,
        key_value,
        modifiers: stroke.modifiers,
    }
}

/// Rebinds one shortcut, replacing any conflicting binding deterministically.
///
/// # Safety
/// `core` must be a live handle used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_rebind_v2(
    core: *mut InkpodCore,
    command_id: u32,
    stroke: *const InkpodShortcutStrokeV2,
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
        // SAFETY: The caller provides one readable size-versioned stroke record.
        let stroke = match unsafe { parse_shortcut_stroke(stroke) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core
            .core
            .rebind_shortcut(ShortcutBinding { command_id, stroke })
        {
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
pub unsafe extern "C" fn inkpod_core_shortcut_resolve_v2(
    core: *mut InkpodCore,
    stroke: *const InkpodShortcutStrokeV2,
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
        // SAFETY: The caller provides one readable size-versioned stroke record.
        let stroke = match unsafe { parse_shortcut_stroke(stroke) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core.core.resolve_shortcut(stroke) {
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

unsafe fn parse_shortcut_sequences(
    sequences: *const InkpodShortcutSequenceV2,
    sequence_count: u64,
    sequence_stride_bytes: u64,
) -> Result<Vec<ShortcutSequenceBinding>, u32> {
    let count = usize::try_from(sequence_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shortcut sequence_count is not representable",
        )
    })?;
    if count > MAX_SHORTCUTS {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shortcut sequence_count exceeds the limit",
        ));
    }
    if count == 0 {
        if !sequences.is_null() || sequence_stride_bytes != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "empty shortcut input must use a null pointer and zero stride",
            ));
        }
        return Ok(Vec::new());
    }
    let stride = usize::try_from(sequence_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shortcut sequence stride is not representable",
        )
    })?;
    if sequences.is_null()
        || !is_aligned(sequences)
        || stride < size_of::<InkpodShortcutSequenceV2>()
        || stride % align_of::<InkpodShortcutSequenceV2>() != 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shortcut sequence pointer or stride is invalid",
        ));
    }
    if (count - 1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodShortcutSequenceV2>()))
        .filter(|span| *span <= isize::MAX as usize)
        .is_none()
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shortcut sequence span overflows",
        ));
    }
    let mut parsed = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The caller advertises count records separated by validated stride.
        let pointer = unsafe {
            sequences
                .cast::<u8>()
                .add(index.checked_mul(stride).ok_or_else(|| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "shortcut sequence offset overflow",
                    )
                })?)
                .cast::<InkpodShortcutSequenceV2>()
        };
        // SAFETY: The pointer exposes a readable size prefix by the public contract.
        let struct_size = unsafe { validate_struct(pointer, "InkpodShortcutSequenceV2")? };
        if u64::from(struct_size) > sequence_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shortcut sequence struct_size exceeds stride",
            ));
        }
        // SAFETY: Full size and alignment were validated above.
        let record = unsafe { pointer.read() };
        let stroke_count = usize::try_from(record.stroke_count).map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shortcut stroke_count is not representable",
            )
        })?;
        if stroke_count == 0 || stroke_count > MAX_SHORTCUT_STROKES {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shortcut stroke_count is invalid",
            ));
        }
        let mut strokes = Vec::with_capacity(stroke_count);
        for stroke in &record.strokes[..stroke_count] {
            // SAFETY: The nested record is part of the validated readable sequence.
            strokes.push(unsafe { parse_shortcut_stroke(stroke) }?);
        }
        parsed.push(ShortcutSequenceBinding {
            command_id: record.command_id,
            strokes,
        });
    }
    Ok(parsed)
}

fn write_shortcut_sequence(binding: &ShortcutSequenceBinding) -> InkpodShortcutSequenceV2 {
    let mut output = InkpodShortcutSequenceV2 {
        struct_size: size_of::<InkpodShortcutSequenceV2>() as u32,
        command_id: binding.command_id,
        stroke_count: binding.strokes.len() as u32,
        reserved: 0,
        strokes: [InkpodShortcutStrokeV2::default(); 4],
    };
    for (destination, source) in output.strokes.iter_mut().zip(&binding.strokes) {
        *destination = write_shortcut_stroke(*source);
    }
    output
}

/// Installs the complete application-provided default shortcut table.
///
/// # Safety
/// `core` and the borrowed strided sequence span must satisfy the public header contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_defaults_set_v2(
    core: *mut InkpodCore,
    sequences: *const InkpodShortcutSequenceV2,
    sequence_count: u64,
    sequence_stride_bytes: u64,
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
        // SAFETY: The public function contract supplies the readable strided span.
        let bindings = match unsafe {
            parse_shortcut_sequences(sequences, sequence_count, sequence_stride_bytes)
        } {
            Ok(bindings) => bindings,
            Err(status) => return status,
        };
        match core.core.set_shortcut_defaults(&bindings) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces the complete active shortcut table without changing defaults.
///
/// # Safety
/// `core` and the borrowed strided sequence span must satisfy the public header contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_sequences_set_v2(
    core: *mut InkpodCore,
    sequences: *const InkpodShortcutSequenceV2,
    sequence_count: u64,
    sequence_stride_bytes: u64,
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
        // SAFETY: The public function contract supplies the readable strided span.
        let bindings = match unsafe {
            parse_shortcut_sequences(sequences, sequence_count, sequence_stride_bytes)
        } {
            Ok(bindings) => bindings,
            Err(status) => return status,
        };
        match core.core.replace_shortcut_sequences(&bindings) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies the complete active shortcut table into caller-owned storage.
///
/// # Safety
/// Outputs must satisfy the query/buffer contract in the public header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_sequences_copy_v2(
    core: *mut InkpodCore,
    out_sequences: *mut InkpodShortcutSequenceV2,
    sequence_capacity: u64,
    sequence_stride_bytes: u64,
    out_sequence_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_sequence_count.is_null() || !is_aligned(out_sequence_count) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_sequence_count is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by contract.
        unsafe { out_sequence_count.write(0) };
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let bindings = core.core.shortcut_sequences();
        let required = bindings.len() as u64;
        // SAFETY: Writable output storage was validated above.
        unsafe { out_sequence_count.write(required) };
        if out_sequences.is_null() {
            return if sequence_capacity == 0 && sequence_stride_bytes == 0 {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "null shortcut output requires zero capacity and stride",
                )
            };
        }
        let stride = match usize::try_from(sequence_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodShortcutSequenceV2>()
                    && stride % align_of::<InkpodShortcutSequenceV2>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "shortcut output stride is invalid",
                );
            }
        };
        if !is_aligned(out_sequences) || sequence_capacity < required {
            return if sequence_capacity < required {
                INKPOD_STATUS_BUFFER_TOO_SMALL
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "shortcut output is misaligned",
                )
            };
        }
        if !bindings.is_empty()
            && (bindings.len() - 1)
                .checked_mul(stride)
                .and_then(|offset| offset.checked_add(size_of::<InkpodShortcutSequenceV2>()))
                .filter(|span| *span <= isize::MAX as usize)
                .is_none()
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shortcut output span overflows",
            );
        }
        for (index, binding) in bindings.iter().enumerate() {
            // SAFETY: Capacity and stride were validated for every output record.
            let pointer = unsafe {
                out_sequences
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodShortcutSequenceV2>()
            };
            // SAFETY: The destination record is writable and non-overlapping.
            unsafe { pointer.write(write_shortcut_sequence(binding)) };
        }
        INKPOD_STATUS_OK
    })
}

/// Resolves a normalized multi-stroke input against a Core-produced table without thread hopping.
///
/// # Safety
/// All input/output pointers must satisfy the public header contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_shortcut_sequence_resolve_v2(
    sequences: *const InkpodShortcutSequenceV2,
    sequence_count: u64,
    sequence_stride_bytes: u64,
    strokes: *const InkpodShortcutStrokeV2,
    stroke_count: u32,
    out_match: *mut u32,
    out_command_id: *mut u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_match.is_null()
            || !is_aligned(out_match)
            || out_command_id.is_null()
            || !is_aligned(out_command_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shortcut resolve output is invalid",
            );
        }
        // SAFETY: Both outputs are writable by contract.
        unsafe {
            out_match.write(INKPOD_SHORTCUT_MATCH_NONE);
            out_command_id.write(0);
        }
        let count = stroke_count as usize;
        if count == 0 || count > MAX_SHORTCUT_STROKES || strokes.is_null() || !is_aligned(strokes) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shortcut input strokes are invalid",
            );
        }
        // SAFETY: The caller provides exactly stroke_count readable fixed-size records.
        let input_records = unsafe { slice::from_raw_parts(strokes, count) };
        let mut input = Vec::with_capacity(count);
        for record in input_records {
            // SAFETY: Each record is inside the caller-advertised readable span.
            match unsafe { parse_shortcut_stroke(record) } {
                Ok(stroke) => input.push(stroke),
                Err(status) => return status,
            }
        }
        // SAFETY: The public contract supplies the readable strided table.
        let bindings = match unsafe {
            parse_shortcut_sequences(sequences, sequence_count, sequence_stride_bytes)
        } {
            Ok(bindings) => bindings,
            Err(status) => return status,
        };
        if bindings.is_empty() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shortcut sequence table is empty",
            );
        }
        let mut prefix = false;
        for binding in &bindings {
            if binding.command_id == 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "shortcut sequence command is invalid",
                );
            }
            let starts_with = binding.strokes.starts_with(&input);
            if starts_with && binding.strokes.len() == count {
                // SAFETY: Both outputs are writable by contract.
                unsafe {
                    out_match.write(INKPOD_SHORTCUT_MATCH_EXACT);
                    out_command_id.write(binding.command_id);
                }
                return INKPOD_STATUS_OK;
            }
            prefix |= starts_with && binding.strokes.len() > count;
        }
        if prefix {
            // SAFETY: out_match is writable by contract.
            unsafe { out_match.write(INKPOD_SHORTCUT_MATCH_PREFIX) };
        }
        INKPOD_STATUS_OK
    })
}
