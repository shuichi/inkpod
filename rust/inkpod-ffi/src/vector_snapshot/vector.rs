use super::*;

/// Adds one bounded cubic, variable-width vector path as a single history
/// transaction. The borrowed strided segment span is copied before return.
///
/// # Safety
/// Core/input/result/output storage must be complete, aligned, live,
/// non-overlapping owner-thread objects for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_add_path(
    core: *mut InkpodCore,
    input: *const InkpodVectorPathInput,
    result: *mut InkpodDispatchResult,
    out_path_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_path_id.is_null() || !is_aligned(out_path_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector path core or output is null or misaligned",
            );
        }
        // SAFETY: Output storage is writable by contract.
        unsafe { out_path_id.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorPathInput") } {
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
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The validated input owns the complete borrowed nested span.
        let parsed = match unsafe { parse_vector_path_input(input) } {
            Ok(parsed) => parsed,
            Err(status) => return status,
        };
        match core.core.vector_add_path(input.plane_id, parsed) {
            Ok((outcome, path_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output storage was checked above.
                unsafe { out_path_id.write(path_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Adds a fill whose borrowed boundary-ID span is copied before return.
///
/// # Safety
/// All pointers must be complete, aligned, live, and non-overlapping for this
/// owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_add_fill(
    core: *mut InkpodCore,
    input: *const InkpodVectorFillInput,
    result: *mut InkpodDispatchResult,
    out_fill_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_fill_id.is_null() || !is_aligned(out_fill_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector fill core or output is null or misaligned",
            );
        }
        // SAFETY: Output storage is writable by contract.
        unsafe { out_fill_id.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorFillInput") } {
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
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector fill input contains unsupported values",
            );
        }
        let count = match usize::try_from(input.boundary_path_count) {
            Ok(count) if (1..=262_144).contains(&count) => count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "vector fill boundary count is outside bounds",
                );
            }
        };
        if input.boundary_path_ids.is_null()
            || !is_aligned(input.boundary_path_ids)
            || count
                .checked_mul(size_of::<u64>())
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector fill boundary span is invalid",
            );
        }
        // SAFETY: The bounded aligned span is readable for this call.
        let boundaries = unsafe { slice::from_raw_parts(input.boundary_path_ids, count) }.to_vec();
        // SAFETY: The nested color record is a complete field of the input.
        let color = match unsafe { parse_color_value(&raw const input.color) } {
            Ok(color) => color,
            Err(status) => return status,
        };
        match core
            .core
            .vector_add_fill(input.plane_id, &boundaries, color)
        {
            Ok((outcome, fill_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output storage was checked above.
                unsafe { out_fill_id.write(fill_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one partial/intersection/full vector erase transaction.
///
/// # Safety
/// Core/input/result must be complete, aligned, live, and non-overlapping on
/// the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_erase(
    core: *mut InkpodCore,
    input: *const InkpodVectorEraseInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorEraseInput") } {
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
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector erase reserved field is not zero",
            );
        }
        let mode = match parse_vector_erase_mode(input.mode) {
            Ok(mode) => mode,
            Err(status) => return status,
        };
        match core.core.vector_erase(
            input.plane_id,
            PointF32 {
                x: input.x,
                y: input.y,
            },
            input.radius,
            mode,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Connects the deterministic nearest endpoint pair within `maximum_gap`.
/// A zero output ID means the command was a successful no-op.
///
/// # Safety
/// Core/result/output must be complete, aligned, live, non-overlapping
/// owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_connect(
    core: *mut InkpodCore,
    plane_id: u64,
    maximum_gap: f32,
    result: *mut InkpodDispatchResult,
    out_path_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_path_id.is_null() || !is_aligned(out_path_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector connect core or output is null or misaligned",
            );
        }
        // SAFETY: Output storage is writable by contract.
        unsafe { out_path_id.write(0) };
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.vector_connect(plane_id, maximum_gap) {
            Ok((outcome, path_id)) => {
                write_dispatch_result(result, outcome);
                if let Some(path_id) = path_id {
                    // SAFETY: Writable output storage was checked above.
                    unsafe { out_path_id.write(path_id) };
                }
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one width correction to a borrowed path-ID span.
///
/// # Safety
/// Core/input/result and nested ID storage must be complete, aligned, live,
/// and non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_correct_width(
    core: *mut InkpodCore,
    input: *const InkpodVectorWidthInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorWidthInput") } {
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
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector width input contains unsupported values",
            );
        }
        let count = match usize::try_from(input.path_count) {
            Ok(count) if (1..=65_536).contains(&count) => count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "vector width path count is outside bounds",
                );
            }
        };
        if input.path_ids.is_null()
            || !is_aligned(input.path_ids)
            || count
                .checked_mul(size_of::<u64>())
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector width path span is invalid",
            );
        }
        // SAFETY: The bounded aligned span is readable for this call.
        let path_ids = unsafe { slice::from_raw_parts(input.path_ids, count) }.to_vec();
        let mode = match parse_vector_width_mode(input.mode, input.parameter) {
            Ok(mode) => mode,
            Err(status) => return status,
        };
        match core.core.vector_correct_width(&path_ids, mode) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Queries deterministic vector selection ranges into caller-owned buffers.
/// A zero-capacity null span is a successful count query.
///
/// # Safety
/// Core/input/output and any advertised output spans must be complete, aligned,
/// live, writable, and non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_select(
    core: *mut InkpodCore,
    input: *const InkpodVectorSelectionInput,
    output: *mut InkpodVectorSelectionBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorSelectionInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodVectorSelectionBuffer") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || output.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector selection contains unsupported flags or reserved values",
            );
        }
        let mode = match parse_vector_selection_mode(input.mode) {
            Ok(mode) => mode,
            Err(status) => return status,
        };
        let selected = match core.core.vector_select(
            RectI32 {
                x: input.bounds.x,
                y: input.bounds.y,
                width: input.bounds.width,
                height: input.bounds.height,
            },
            mode,
        ) {
            Ok(selected) => selected,
            Err(error) => return map_core_error(error),
        };
        output.range_count = selected.path_ranges.len() as u64;
        output.fill_count = selected.fill_ids.len() as u64;
        if output.range_capacity == 0 {
            if !output.ranges.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity vector range output must be null",
                );
            }
        } else if output.range_capacity > 65_536
            || output.ranges.is_null()
            || !is_aligned(output.ranges)
            || usize::try_from(output.range_capacity)
                .ok()
                .and_then(|count| count.checked_mul(size_of::<InkpodVectorSelectionRange>()))
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector range output span is invalid",
            );
        }
        if output.fill_capacity == 0 {
            if !output.fill_ids.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity vector fill output must be null",
                );
            }
        } else if output.fill_capacity > 65_536
            || output.fill_ids.is_null()
            || !is_aligned(output.fill_ids)
            || usize::try_from(output.fill_capacity)
                .ok()
                .and_then(|count| count.checked_mul(size_of::<u64>()))
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector fill output span is invalid",
            );
        }
        if output.range_capacity < output.range_count || output.fill_capacity < output.fill_count {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "vector selection output capacity is too small",
            );
        }
        for (index, range) in selected.path_ranges.iter().enumerate() {
            let record = InkpodVectorSelectionRange {
                struct_size: size_of::<InkpodVectorSelectionRange>() as u32,
                reserved: 0,
                path_id: range.path_id,
                start_million: range.start_million,
                end_million: range.end_million,
            };
            // SAFETY: The caller-owned bounded output span is writable by contract.
            unsafe { output.ranges.add(index).write(record) };
        }
        if !selected.fill_ids.is_empty() {
            // SAFETY: Capacity and byte bounds were checked and the spans may not overlap.
            unsafe {
                ptr::copy_nonoverlapping(
                    selected.fill_ids.as_ptr(),
                    output.fill_ids,
                    selected.fill_ids.len(),
                )
            };
        }
        INKPOD_STATUS_OK
    })
}

/// Rasterizes one vector layer into caller-owned straight RGBA8 storage. A
/// zero-capacity null buffer is a successful size query.
///
/// # Safety
/// Core/input/output and any advertised pixel range must be complete, aligned,
/// live, writable, and non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_rasterize(
    core: *mut InkpodCore,
    input: *const InkpodVectorRasterizeInput,
    output: *mut InkpodVectorRasterBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorRasterizeInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodVectorRasterBuffer") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0
            || input.reserved_2 != 0
            || input.feature_flags & !INKPOD_VECTOR_RASTERIZE_ANTIALIAS != 0
            || output.reserved != 0
            || output.reserved_2 != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector rasterize contains unsupported flags or reserved values",
            );
        }
        let (width, height, stride_bytes, required_bytes) =
            match core.core.vector_raster_layout(input.layer_id, input.scale) {
                Ok(layout) => layout,
                Err(error) => return map_core_error(error),
            };
        output.required_bytes = required_bytes;
        output.width = width;
        output.height = height;
        output.stride_bytes = stride_bytes;
        if output.pixel_capacity == 0 {
            if !output.pixels.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity vector raster output must be null",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if output.pixels.is_null()
            || output.pixel_capacity > isize::MAX as u64
            || output.pixel_capacity < output.required_bytes
        {
            return fail(
                if output.pixel_capacity < output.required_bytes {
                    INKPOD_STATUS_BUFFER_TOO_SMALL
                } else {
                    INKPOD_STATUS_INVALID_ARGUMENT
                },
                "vector raster output storage is invalid or too small",
            );
        }
        let raster = match core.core.rasterize_vector_layer(
            input.layer_id,
            input.scale,
            input.feature_flags & INKPOD_VECTOR_RASTERIZE_ANTIALIAS != 0,
        ) {
            Ok(raster) => raster,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: The caller advertises enough writable byte storage and it may
        // not overlap Core/input/output memory.
        unsafe {
            ptr::copy_nonoverlapping(raster.pixels.as_ptr(), output.pixels, raster.pixels.len())
        };
        INKPOD_STATUS_OK
    })
}

/// Rasterizes one vector layer at document scale into a new RGBA8 raster
/// layer, preserving the source and committing one history unit.
///
/// # Safety
/// Core/input/name/result/output storage must be complete, aligned, live, and
/// non-overlapping on the Core owner thread. The name bytes are borrowed only
/// for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_rasterize_to_layer(
    core: *mut InkpodCore,
    input: *const InkpodVectorRasterizeInput,
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
                "vector rasterize-to-layer core or output is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by the caller contract.
        unsafe { out_layer_id.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorRasterizeInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live structures and name span are required by the
        // exported contract and validated before they are borrowed.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let name = match unsafe { name_from_utf8(name_utf8, name_bytes) } {
            Ok(name) => name,
            Err(status) => return status,
        };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0
            || input.reserved_2 != 0
            || input.scale != 1
            || input.feature_flags & !INKPOD_VECTOR_RASTERIZE_ANTIALIAS != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector rasterize-to-layer requires scale 1 and supported flags",
            );
        }
        match core.core.rasterize_vector_layer_to_document(
            input.layer_id,
            input.feature_flags & INKPOD_VECTOR_RASTERIZE_ANTIALIAS != 0,
            name,
        ) {
            Ok((outcome, layer_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output storage was validated above.
                unsafe { out_layer_id.write(layer_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Converts bounded RGBA8 raster runs into vector paths/fills as one history
/// transaction and reports the number of created fills.
///
/// # Safety
/// Core/input/result/count storage must be complete, aligned, live, writable,
/// and non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_raster_vectorize(
    core: *mut InkpodCore,
    input: *const InkpodRasterVectorizeInput,
    result: *mut InkpodDispatchResult,
    out_fill_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_fill_count.is_null()
            || !is_aligned(out_fill_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "raster vectorize core or output is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by contract.
        unsafe { out_fill_count.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodRasterVectorizeInput") } {
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
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || input.alpha_threshold > u8::MAX.into() {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "raster vectorize contains unsupported flags or alpha threshold",
            );
        }
        match core.core.vectorize_raster_plane(
            input.source_plane_id,
            input.target_layer_id,
            input.alpha_threshold as u8,
        ) {
            Ok((outcome, fill_ids)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output storage was checked above.
                unsafe { out_fill_count.write(fill_ids.len() as u64) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
