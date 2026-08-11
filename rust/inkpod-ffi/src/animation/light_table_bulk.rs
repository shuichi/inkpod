use super::*;

fn bulk_direction(value: u32) -> Result<LightTableBulkDirection, u32> {
    match value {
        INKPOD_LIGHT_TABLE_BULK_PREVIOUS => Ok(LightTableBulkDirection::Previous),
        INKPOD_LIGHT_TABLE_BULK_NEXT => Ok(LightTableBulkDirection::Next),
        INKPOD_LIGHT_TABLE_BULK_BOTH => Ok(LightTableBulkDirection::Both),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "light-table bulk direction is not defined",
        )),
    }
}

unsafe fn parse_bulk_request(
    input: *const InkpodLightTableBulkRequest,
) -> Result<LightTableBulkRegistrationRequest, u32> {
    // SAFETY: The caller guarantees that `input` is readable for this call.
    unsafe { validate_struct(input, "InkpodLightTableBulkRequest") }?;
    // SAFETY: The complete aligned record was validated above.
    let input = unsafe { &*input };
    if input.reserved != 0 || input.feature_flags != 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "light-table bulk request reserved fields are nonzero",
        ));
    }
    Ok(LightTableBulkRegistrationRequest {
        target_set_id: input.target_set_id,
        direction: bulk_direction(input.direction)?,
        neighbor_count: input.neighbor_count,
        base_opacity_milli: input.base_opacity_milli,
        distance_step_milli: input.distance_step_milli,
        base_document_revision: input.base_document_revision,
        sequence_revision: input.sequence_revision,
        active_document_uuid: (u128::from(input.active_document_uuid_high) << 64)
            | u128::from(input.active_document_uuid_low),
        active_source_generation: input.active_source_generation,
    })
}

/// Captures a caller-owned stale-detecting Light Table bulk request.
///
/// # Safety
/// Core and output must be complete, aligned, non-overlapping records on the
/// Core owner thread. The output remains caller-owned and contains no pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_bulk_request(
    core: *mut InkpodCore,
    target_set_id: u64,
    direction: u32,
    neighbor_count: u32,
    base_opacity_milli: u32,
    distance_step_milli: u32,
    output: *mut InkpodLightTableBulkRequest,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodLightTableBulkRequest") }
        {
            return status;
        }
        let direction = match bulk_direction(direction) {
            Ok(direction) => direction,
            Err(status) => return status,
        };
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let request = match core.core.light_table_bulk_registration_request(
            target_set_id,
            direction,
            neighbor_count,
            base_opacity_milli,
            distance_step_milli,
        ) {
            Ok(request) => request,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Writable output storage was validated above.
        let output = unsafe { &mut *output };
        output.direction = request.direction as u32;
        output.target_set_id = request.target_set_id;
        output.neighbor_count = request.neighbor_count;
        output.base_opacity_milli = request.base_opacity_milli;
        output.distance_step_milli = request.distance_step_milli;
        output.reserved = 0;
        output.base_document_revision = request.base_document_revision;
        output.sequence_revision = request.sequence_revision;
        output.active_document_uuid_high = (request.active_document_uuid >> 64) as u64;
        output.active_document_uuid_low = request.active_document_uuid as u64;
        output.active_source_generation = request.active_source_generation;
        output.feature_flags = 0;
        INKPOD_STATUS_OK
    })
}

/// Copies a side-effect-free preview into a caller-owned strided entry buffer.
///
/// # Safety
/// Core, request, preview-info, and any advertised entry records must be live,
/// aligned, complete, writable where applicable, and non-overlapping. With a
/// null entry pointer, zero capacity and zero stride perform the size query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_bulk_preview(
    core: *mut InkpodCore,
    request: *const InkpodLightTableBulkRequest,
    entries: *mut InkpodLightTableBulkPreviewEntry,
    entry_capacity: u64,
    entry_stride_bytes: u64,
    output: *mut InkpodLightTableBulkPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        let request = match unsafe { parse_bulk_request(request) } {
            Ok(request) => request,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodLightTableBulkPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live Core was validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let preview = match core.core.preview_light_table_bulk_registration(&request) {
            Ok(preview) => preview,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Writable output storage was validated above.
        let output = unsafe { &mut *output };
        output.reserved = 0;
        output.target_set_id = preview.target_set_id;
        output.entry_count = preview.entries.len() as u64;
        output.add_count = preview.add_count;
        output.skip_count = preview.skip_count;

        if entry_capacity == 0 {
            return if entries.is_null() && entry_stride_bytes == 0 {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity light-table bulk preview must use null/zero span",
                )
            };
        }
        if entries.is_null() || !is_aligned(entries) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table bulk preview entry pointer is null or misaligned",
            );
        }
        if entry_capacity < preview.entries.len() as u64 {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "light-table bulk preview entry buffer is too small",
            );
        }
        if entry_stride_bytes < size_of::<InkpodLightTableBulkPreviewEntry>() as u64 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table bulk preview entry stride is too small",
            );
        }
        let byte_count = match (preview.entries.len() as u64).checked_mul(entry_stride_bytes) {
            Some(byte_count) if byte_count <= isize::MAX as u64 => byte_count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "light-table bulk preview entry span overflows",
                );
            }
        };
        let _ = byte_count;
        let stride = entry_stride_bytes as usize;
        for index in 0..preview.entries.len() {
            // SAFETY: The checked strided span is caller-advertised writable storage.
            let entry = unsafe {
                entries
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodLightTableBulkPreviewEntry>()
            };
            if !is_aligned(entry) {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "light-table bulk preview strided entry is misaligned",
                );
            }
            let struct_size = match unsafe {
                validate_struct(entry.cast_const(), "InkpodLightTableBulkPreviewEntry")
            } {
                Ok(struct_size) => struct_size,
                Err(status) => return status,
            };
            if u64::from(struct_size) > entry_stride_bytes {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "light-table bulk preview entry size exceeds its stride",
                );
            }
        }
        for (index, source) in preview.entries.iter().enumerate() {
            // SAFETY: Every destination record was prevalidated above.
            let output = unsafe {
                &mut *entries
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodLightTableBulkPreviewEntry>()
            };
            output.action = match source.action {
                LightTableBulkRegistrationAction::Add => INKPOD_LIGHT_TABLE_BULK_ADD,
                LightTableBulkRegistrationAction::SkipExisting => {
                    INKPOD_LIGHT_TABLE_BULK_SKIP_EXISTING
                }
            };
            output.sequence_index = source.sequence_index;
            output.cell_number = source.cell_number;
            output.distance = source.distance;
            output.opacity_milli = source.opacity_milli;
            output.document_uuid_high = (source.document_uuid >> 64) as u64;
            output.document_uuid_low = source.document_uuid as u64;
            output.source_generation = source.source_generation;
            output.existing_source_revision = source.existing_source_revision.unwrap_or(0);
            output.flags = if source.existing_source_revision.is_some() {
                INKPOD_LIGHT_TABLE_BULK_HAS_EXISTING_REVISION
            } else {
                0
            };
        }
        INKPOD_STATUS_OK
    })
}

/// Commits a captured request and copies added stable item IDs to caller storage.
///
/// # Safety
/// Core/request/result/summary and an optional ID buffer must be complete,
/// aligned, writable where applicable, non-overlapping owner-thread records.
/// Capacity is validated against a fresh preview before the one atomic commit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_bulk_register(
    core: *mut InkpodCore,
    request: *const InkpodLightTableBulkRequest,
    result: *mut InkpodDispatchResult,
    summary: *mut InkpodLightTableBulkSummary,
    out_item_ids: *mut u64,
    item_id_capacity: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        let request = match unsafe { parse_bulk_request(request) } {
            Ok(request) => request,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(summary.cast_const(), "InkpodLightTableBulkSummary") }
        {
            return status;
        }
        // SAFETY: Complete live Core was validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let preview = match core.core.preview_light_table_bulk_registration(&request) {
            Ok(preview) => preview,
            Err(error) => return map_core_error(error),
        };
        if u64::from(preview.add_count) > item_id_capacity {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "light-table bulk item-ID buffer is too small",
            );
        }
        if preview.add_count == 0 {
            if !out_item_ids.is_null() || item_id_capacity != 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "light-table bulk no-op ID span must be null/zero",
                );
            }
        } else if out_item_ids.is_null() || !is_aligned(out_item_ids) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table bulk item-ID output is null or misaligned",
            );
        }
        let (outcome, committed) = match core.core.light_table_bulk_register(request) {
            Ok(result) => result,
            Err(error) => return map_core_error(error),
        };
        if committed.added_item_ids.len() != preview.add_count as usize {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "light-table bulk Core output count is inconsistent",
            );
        }
        if !committed.added_item_ids.is_empty() {
            // SAFETY: Sufficient aligned writable capacity was validated above.
            unsafe {
                ptr::copy_nonoverlapping(
                    committed.added_item_ids.as_ptr(),
                    out_item_ids,
                    committed.added_item_ids.len(),
                )
            };
        }
        // SAFETY: Complete writable records were validated above.
        let result = unsafe { &mut *result };
        let summary = unsafe { &mut *summary };
        write_dispatch_result(result, outcome);
        summary.reserved = 0;
        summary.target_set_id = preview.target_set_id;
        summary.add_count = committed.add_count;
        summary.skip_count = committed.skip_count;
        summary.item_id_count = committed.added_item_ids.len() as u64;
        INKPOD_STATUS_OK
    })
}
