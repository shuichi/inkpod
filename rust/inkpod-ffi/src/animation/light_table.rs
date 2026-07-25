use super::*;

/// Copies one persistent light-table item into the active set.
///
/// # Safety
/// Core, input, nested raster/name storage, result, and item-ID output must be
/// complete, non-overlapping records valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_add_item(
    core: *mut InkpodCore,
    input: *const InkpodLightTableItemInput,
    result: *mut InkpodDispatchResult,
    out_item_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_item_id.is_null() || !is_aligned(out_item_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table pointer is invalid",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodLightTableItemInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete input was validated above.
        let input = unsafe { &*input };
        if input.flags & !INKPOD_LIGHT_TABLE_ITEM_VISIBLE != 0 || input.reserved != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table flags or reserved field is invalid",
            );
        }
        let display_mode = match input.display_mode {
            INKPOD_LIGHT_TABLE_COLOR => LightTableDisplayMode::Color,
            INKPOD_LIGHT_TABLE_MONOTONE => LightTableDisplayMode::Monotone,
            INKPOD_LIGHT_TABLE_HALFTONE => LightTableDisplayMode::Halftone,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "light-table display mode is not defined",
                );
            }
        };
        let display_color = match unsafe { parse_color_value(&input.display_color) } {
            Ok(color) => color,
            Err(status) => return status,
        };
        let name = match unsafe { name_from_utf8(input.name_utf8, input.name_bytes) } {
            Ok(name) => name.to_owned(),
            Err(status) => return status,
        };
        let source = match unsafe { parse_raster_source(&input.source) } {
            Ok(source) => source,
            Err(status) => return status,
        };
        let source = match LightTableSource::from_rgba_bytes(
            source.document_uuid,
            source.source_revision,
            source.reference_frame,
            RgbaRasterBytes {
                width: source.width,
                height: source.height,
                pixel_format: source.pixel_format,
                dpi_x_milli: source.dpi_x_milli,
                dpi_y_milli: source.dpi_y_milli,
                pixels: source.pixels,
            },
        ) {
            Ok(source) => source,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Live owner-thread Core and writable result are required.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.light_table_add_item(LightTableItemInput {
            name,
            source,
            visible: input.flags & INKPOD_LIGHT_TABLE_ITEM_VISIBLE != 0,
            opacity_milli: input.opacity_milli,
            display_mode,
            display_color,
            translate_x_milli: input.translate_x_milli,
            translate_y_milli: input.translate_y_milli,
            scale_x_milli: input.scale_x_milli,
            scale_y_milli: input.scale_y_milli,
            rotation_milli_degrees: input.rotation_milli_degrees,
        }) {
            Ok((outcome, item_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output was validated above.
                unsafe { out_item_id.write(item_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Changes persistent active-set opacity as one document transaction.
///
/// # Safety
/// Core and result must be complete live records on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_set_global_opacity(
    core: *mut InkpodCore,
    opacity_milli: u32,
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
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.light_table_set_global_opacity(opacity_milli) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a set/item management edit as one document history transaction.
///
/// # Safety
/// All pointers must be complete, aligned, live, non-overlapping owner-thread
/// records. A name span is required only for create/rename-set operations.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_edit(
    core: *mut InkpodCore,
    input: *const InkpodLightTableEdit,
    result: *mut InkpodDispatchResult,
    out_object_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_object_id.is_null()
            || !is_aligned(out_object_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table edit pointer is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodLightTableEdit") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete records were validated above.
        let input = unsafe { &*input };
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let edit_result = match input.operation {
            INKPOD_LIGHT_TABLE_CREATE_SET => {
                let name = match unsafe { name_from_utf8(input.name_utf8, input.name_bytes) } {
                    Ok(name) => name.to_owned(),
                    Err(status) => return status,
                };
                core.core.light_table_create_set(name)
            }
            INKPOD_LIGHT_TABLE_DUPLICATE_SET => {
                core.core.light_table_duplicate_set(input.object_id)
            }
            INKPOD_LIGHT_TABLE_DELETE_SET => core
                .core
                .light_table_delete_set(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_LIGHT_TABLE_RENAME_SET => {
                let name = match unsafe { name_from_utf8(input.name_utf8, input.name_bytes) } {
                    Ok(name) => name.to_owned(),
                    Err(status) => return status,
                };
                core.core
                    .light_table_rename_set(input.object_id, name)
                    .map(|outcome| (outcome, input.object_id))
            }
            INKPOD_LIGHT_TABLE_REORDER_SET => core
                .core
                .light_table_reorder_set(input.object_id, input.destination_index as usize)
                .map(|outcome| (outcome, input.object_id)),
            INKPOD_LIGHT_TABLE_SET_ACTIVE_OPERATION => core
                .core
                .light_table_set_active(input.object_id)
                .map(|outcome| (outcome, input.object_id)),
            INKPOD_LIGHT_TABLE_REMOVE_ITEM => core
                .core
                .light_table_remove_item(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_LIGHT_TABLE_REORDER_ITEM => core
                .core
                .light_table_reorder_item(input.object_id, input.destination_index as usize)
                .map(|outcome| (outcome, input.object_id)),
            INKPOD_LIGHT_TABLE_UPDATE_ITEM => {
                if input.flags & !INKPOD_LIGHT_TABLE_ITEM_VISIBLE != 0 || input.reserved != 0 {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "light-table item property flags are invalid",
                    );
                }
                let display_mode = match input.display_mode {
                    INKPOD_LIGHT_TABLE_COLOR => LightTableDisplayMode::Color,
                    INKPOD_LIGHT_TABLE_MONOTONE => LightTableDisplayMode::Monotone,
                    INKPOD_LIGHT_TABLE_HALFTONE => LightTableDisplayMode::Halftone,
                    _ => {
                        return fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "light-table display mode is not defined",
                        );
                    }
                };
                let display_color = match unsafe { parse_color_value(&input.display_color) } {
                    Ok(color) => color,
                    Err(status) => return status,
                };
                core.core
                    .light_table_update_item_properties(
                        input.object_id,
                        LightTableItemProperties {
                            visible: input.flags & INKPOD_LIGHT_TABLE_ITEM_VISIBLE != 0,
                            opacity_milli: input.opacity_milli,
                            display_mode,
                            display_color,
                            translate_x_milli: input.translate_x_milli,
                            translate_y_milli: input.translate_y_milli,
                            scale_x_milli: input.scale_x_milli,
                            scale_y_milli: input.scale_y_milli,
                            rotation_milli_degrees: input.rotation_milli_degrees,
                        },
                    )
                    .map(|outcome| (outcome, input.object_id))
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "light-table edit operation is not defined",
                );
            }
        };
        match edit_result {
            Ok((outcome, object_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable aligned storage was validated above.
                unsafe { out_object_id.write(object_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Returns one persistent light-table set by display order.
///
/// # Safety
/// Core and output must be complete live owner-thread records. The optional
/// UTF-8 name buffer remains caller-owned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_set_get(
    core: *mut InkpodCore,
    index: u32,
    output: *mut InkpodLightTableSetInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodLightTableSetInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let sets = match core.core.light_table_sets() {
            Ok(sets) => sets,
            Err(error) => return map_core_error(error),
        };
        let Some(set) = sets.get(index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table set index is outside bounds",
            );
        };
        output.flags = if set.active {
            INKPOD_LIGHT_TABLE_SET_ACTIVE
        } else {
            0
        };
        output.id = set.id;
        output.opacity_milli = set.global_opacity_milli;
        output.item_count = set.item_count as u32;
        output.name_bytes = set.name.len() as u64;
        if output.name_capacity == 0 {
            return if output.name_utf8.is_null() {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity set name buffer must be null",
                )
            };
        }
        if output.name_utf8.is_null() || output.name_capacity < output.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "light-table set name buffer is too small",
            );
        }
        // SAFETY: Caller advertises sufficient writable name capacity.
        unsafe { ptr::copy_nonoverlapping(set.name.as_ptr(), output.name_utf8, set.name.len()) };
        INKPOD_STATUS_OK
    })
}

/// Returns one item from the active light-table set by display order.
///
/// # Safety
/// Core and output must be complete live owner-thread records. The optional
/// UTF-8 name buffer remains caller-owned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_item_get(
    core: *mut InkpodCore,
    index: u32,
    output: *mut InkpodLightTableItemInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodLightTableItemInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let items = match core.core.light_table_items() {
            Ok(items) => items,
            Err(error) => return map_core_error(error),
        };
        let Some(item) = items.get(index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table item index is outside bounds",
            );
        };
        output.flags = if item.visible {
            INKPOD_LIGHT_TABLE_ITEM_VISIBLE
        } else {
            0
        };
        output.id = item.id;
        output.source_plane_id = item.source_plane_id;
        output.source_document_uuid_high = (item.source_document_uuid >> 64) as u64;
        output.source_document_uuid_low = item.source_document_uuid as u64;
        output.source_revision = item.source_revision;
        output.opacity_milli = item.opacity_milli;
        output.effective_opacity_milli = item.effective_opacity_milli;
        output.display_mode = match item.display_mode {
            LightTableDisplayMode::Color => INKPOD_LIGHT_TABLE_COLOR,
            LightTableDisplayMode::Monotone => INKPOD_LIGHT_TABLE_MONOTONE,
            LightTableDisplayMode::Halftone => INKPOD_LIGHT_TABLE_HALFTONE,
        };
        if let Err(status) = write_color_value(&mut output.display_color, item.display_color) {
            return status;
        }
        output.translate_x_milli = item.translate_x_milli;
        output.translate_y_milli = item.translate_y_milli;
        output.scale_x_milli = item.scale_x_milli;
        output.scale_y_milli = item.scale_y_milli;
        output.rotation_milli_degrees = item.rotation_milli_degrees;
        output.reserved = 0;
        output.name_bytes = item.name.len() as u64;
        if output.name_capacity == 0 {
            return if output.name_utf8.is_null() {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity item name buffer must be null",
                )
            };
        }
        if output.name_utf8.is_null() || output.name_capacity < output.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "light-table item name buffer is too small",
            );
        }
        // SAFETY: Caller advertises sufficient writable name capacity.
        unsafe { ptr::copy_nonoverlapping(item.name.as_ptr(), output.name_utf8, item.name.len()) };
        INKPOD_STATUS_OK
    })
}

/// Decodes and inserts one common-raster file into the active light-table set.
///
/// # Safety
/// Core/result/output must be valid owner-thread records and both byte/name
/// spans must remain readable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_add_common_raster(
    core: *mut InkpodCore,
    format: u32,
    bytes: *const u8,
    byte_count: u64,
    name_utf8: *const u8,
    name_bytes: u64,
    document_uuid_high: u64,
    document_uuid_low: u64,
    source_revision: u64,
    result: *mut InkpodDispatchResult,
    out_item_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_item_id.is_null()
            || !is_aligned(out_item_id)
            || bytes.is_null()
            || byte_count == 0
            || byte_count > MAX_COMMON_RASTER_BYTES as u64
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table raster span is invalid",
            );
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let name = match unsafe { name_from_utf8(name_utf8, name_bytes) } {
            Ok(name) => name.to_owned(),
            Err(status) => return status,
        };
        let length = match usize::try_from(byte_count) {
            Ok(length) => length,
            Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "raster length overflows"),
        };
        // SAFETY: Caller provides this bounded readable byte span.
        let bytes = unsafe { slice::from_raw_parts(bytes, length) };
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let uuid = (u128::from(document_uuid_high) << 64) | u128::from(document_uuid_low);
        match core
            .core
            .light_table_add_common_raster(format, bytes, name, uuid, source_revision)
        {
            Ok((outcome, item_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable aligned output was validated above.
                unsafe { out_item_id.write(item_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces one item's source image while retaining its display properties.
///
/// # Safety
/// Core/result and the encoded byte span must be valid for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_reload_common_raster(
    core: *mut InkpodCore,
    item_id: u64,
    format: u32,
    bytes: *const u8,
    byte_count: u64,
    document_uuid_high: u64,
    document_uuid_low: u64,
    source_revision: u64,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || bytes.is_null()
            || byte_count == 0
            || byte_count > MAX_COMMON_RASTER_BYTES as u64
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "reload raster span is invalid",
            );
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let length = match usize::try_from(byte_count) {
            Ok(length) => length,
            Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "raster length overflows"),
        };
        // SAFETY: Caller provides this bounded readable byte span.
        let bytes = unsafe { slice::from_raw_parts(bytes, length) };
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let uuid = (u128::from(document_uuid_high) << 64) | u128::from(document_uuid_low);
        match core.core.light_table_reload_common_raster(
            item_id,
            format,
            bytes,
            uuid,
            source_revision,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples the transformed topmost light-table item in document coordinates.
///
/// # Safety
/// Core and output must be complete live records on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_sample(
    core: *mut InkpodCore,
    x: u32,
    y: u32,
    out_color: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") }
        {
            return status;
        }
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_color };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.light_table_sample(x, y) {
            Ok(color) => match write_color_value(output, color) {
                Ok(()) => INKPOD_STATUS_OK,
                Err(status) => status,
            },
            Err(error) => map_core_error(error),
        }
    })
}

/// Swaps the active edit image with one light-table item after dirty checking.
///
/// # Safety
/// Core and document-info output must be complete live records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_swap(
    core: *mut InkpodCore,
    item_id: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.light_table_swap_with_active(item_id) {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
