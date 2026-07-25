use super::*;

/// Applies seed/closed-region/extension fill as one all-or-nothing history
/// transaction. A leak returns INKPOD_STATUS_FILL_OVERFLOW and its candidate
/// coordinate without committing any pixel.
///
/// # Safety
/// Core/input/result and every optional strided color record must be complete,
/// live, aligned, readable/writable as applicable, and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_fill(
    core: *mut InkpodCore,
    input: *const InkpodFillInput,
    result: *mut InkpodFillResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodFillInput") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodFillResult") } {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        result.flags = 0;
        result.revision = 0;
        result.changed_pixel_count = 0;
        result.leak_x = 0;
        result.leak_y = 0;
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The input and optional strided span were validated above.
        let request = match unsafe { parse_fill_input(input) } {
            Ok(request) => request,
            Err(status) => return status,
        };
        match core.core.apply_fill_with_light_table(
            &request,
            input.flags & INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY != 0,
            input.flags & INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR != 0,
        ) {
            Ok(outcome) => {
                result.revision = outcome.dispatch.revision();
                result.changed_pixel_count = outcome.changed_pixels;
                INKPOD_STATUS_OK
            }
            Err(CoreError::FillOverflow { x, y }) => {
                result.flags = INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE;
                result.leak_x = x;
                result.leak_y = y;
                fail(
                    INKPOD_STATUS_FILL_OVERFLOW,
                    &format!("fill reached image edge at ({x}, {y})"),
                )
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples an exact 8/16-bit color from the requested source.
///
/// # Safety
/// Core/output must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_eyedropper(
    core: *mut InkpodCore,
    source: u32,
    x: u32,
    y: u32,
    out_color: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let out_color = unsafe { &mut *out_color };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let source = match source {
            INKPOD_EYEDROPPER_TOPMOST_NONTRANSPARENT => EyedropperSource::TopmostNonTransparent,
            INKPOD_EYEDROPPER_SELECTED_PLANE => EyedropperSource::SelectedPlane,
            INKPOD_EYEDROPPER_COMPOSITE => EyedropperSource::Composite,
            INKPOD_EYEDROPPER_LIGHT_TABLE_TOPMOST => EyedropperSource::LightTableTopmost,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "eyedropper source is not defined",
                );
            }
        };
        match core.core.eyedropper(source, x, y) {
            Ok(color) => match write_color_value(out_color, color) {
                Ok(()) => INKPOD_STATUS_OK,
                Err(status) => status,
            },
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces the document palette as one exact-depth metadata transaction.
///
/// # Safety
/// Core/input/result and every strided color record must be complete, live,
/// aligned, non-overlapping owner-thread objects for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_palette_set(
    core: *mut InkpodCore,
    input: *const InkpodColorArray,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodColorArray") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live, non-overlapping objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The input and its complete strided span were validated above.
        let colors = match unsafe { parse_color_array(input) } {
            Ok(colors) => colors,
            Err(status) => return status,
        };
        match core.core.replace_palette(&colors) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies the exact-depth document palette into caller-owned strided storage.
/// A zero-capacity null buffer is a successful count query.
///
/// # Safety
/// Core/buffer and any advertised output records must be complete, writable,
/// aligned, live, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_palette_get(
    core: *mut InkpodCore,
    buffer: *mut InkpodColorBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(buffer.cast_const(), "InkpodColorBuffer") } {
            return status;
        }
        // SAFETY: Complete live, non-overlapping objects are required by contract.
        let core = unsafe { &mut *core };
        let buffer = unsafe { &mut *buffer };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if buffer.reserved != 0 || buffer.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "palette buffer contains unsupported flags or reserved values",
            );
        }
        let colors = match core.core.palette() {
            Ok(colors) => colors,
            Err(error) => return map_core_error(error),
        };
        buffer.color_count = colors.len() as u64;
        if buffer.color_capacity == 0 {
            if !buffer.colors.is_null() || buffer.color_stride_bytes != 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "a palette count query must use a null pointer and zero stride",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if buffer.color_capacity > MAX_PALETTE_COLOR_COUNT
            || buffer.colors.is_null()
            || !is_aligned(buffer.colors)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "palette output capacity or storage is invalid",
            );
        }
        let stride = match usize::try_from(buffer.color_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodColorValue>()
                    && stride % align_of::<InkpodColorValue>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "palette output stride is too small, misaligned, or not representable",
                );
            }
        };
        if buffer.color_capacity < colors.len() as u64 {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "palette output capacity is smaller than color_count",
            );
        }
        let storage = colors
            .len()
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodColorValue>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "palette output storage size overflows",
            );
        }
        for (index, color) in colors.iter().copied().enumerate() {
            let record = match color_value_record(color) {
                Ok(record) => record,
                Err(status) => return status,
            };
            // SAFETY: The checked caller-owned strided output range is writable.
            unsafe {
                buffer
                    .colors
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodColorValue>()
                    .write(record);
            }
        }
        INKPOD_STATUS_OK
    })
}

/// Extracts a bounded quantized unique-color chart and stores it as the document palette.
///
/// # Safety
/// Core/result must be complete live owner-thread records and must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_palette_generate(
    core: *mut InkpodCore,
    maximum_colors: u32,
    quantization_bits: u32,
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
        let quantization_bits = match u8::try_from(quantization_bits) {
            Ok(bits) => bits,
            Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "quantization exceeds u8"),
        };
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core
            .core
            .generate_palette_from_document(maximum_colors as usize, quantization_bits)
        {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Changes the base color used by a grayscale main-line plane.
///
/// # Safety
/// Core/color/result must be complete, live, aligned, and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_main_line_color(
    core: *mut InkpodCore,
    color: *const InkpodColorValue,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(color, "InkpodColorValue") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live, non-overlapping objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The complete input record was validated above.
        let color = match unsafe { parse_color_value(color) } {
            Ok(color) => color,
            Err(status) => return status,
        };
        match core.core.set_main_line_color(color) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies the exact-depth grayscale main-line base color.
///
/// # Safety
/// Core/output must be complete, live, aligned, and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_main_line_color(
    core: *mut InkpodCore,
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
        // SAFETY: Complete live, non-overlapping objects are required by contract.
        let core = unsafe { &mut *core };
        let out_color = unsafe { &mut *out_color };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.main_line_color() {
            Ok(color) => match write_color_value(out_color, color) {
                Ok(()) => INKPOD_STATUS_OK,
                Err(status) => status,
            },
            Err(error) => map_core_error(error),
        }
    })
}

/// Changes only the temporary coloring-check view; document revision/history
/// and pixel values remain untouched.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_color_check(core: *mut InkpodCore, mode: u32) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A complete live Core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let mode = match mode {
            INKPOD_COLOR_CHECK_OFF => None,
            INKPOD_COLOR_CHECK_LEGACY_WHITE => Some(ColorCheckMode::LegacyWhiteTransparency),
            INKPOD_COLOR_CHECK_NATIVE_ALPHA => Some(ColorCheckMode::NativeAlpha),
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "color-check mode is not defined",
                );
            }
        };
        match core.core.set_color_check(mode) {
            Ok(_) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies all samples from pointer-down through pointer-up as one transaction.
///
/// # Safety
/// The input, sample span, output, and Core must be live, aligned,
/// non-overlapping ranges for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_stroke(
    core: *mut InkpodCore,
    input: *const InkpodStrokeInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structure pointers expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodStrokeInput") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects and writable output are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The complete input and its borrowed sample span were validated above.
        let stroke = match unsafe { parse_stroke_input(input) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core.core.apply_stroke(&stroke) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Starts one Core-owned transient stroke transaction and stages the supplied
/// first sample batch without changing document revision, history, or dirty.
///
/// # Safety
/// Core/input/sample storage must satisfy the same contract as
/// `inkpod_core_apply_stroke` and remain live for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_begin(
    core: *mut InkpodCore,
    input: *const InkpodStrokeInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodStrokeInput") } {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The complete input and borrowed span were validated above.
        let stroke = match unsafe { parse_stroke_input(input) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core.core.begin_stroke(&stroke) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Appends one borrowed, strided sample batch to the active transient stroke.
/// Failure discards the Core-owned preview and commits no document state.
///
/// # Safety
/// Core/span/sample storage must be complete, live, aligned, non-overlapping
/// owner-thread objects for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_append(
    core: *mut InkpodCore,
    span: *const InkpodStrokeSampleSpan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        let span_status = unsafe { validate_struct(span, "InkpodStrokeSampleSpan") };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if let Err(status) = span_status {
            core.core.cancel_stroke();
            return status;
        }
        let span = unsafe { &*span };
        if span.reserved != 0 || span.feature_flags != INKPOD_FEATURE_NONE {
            core.core.cancel_stroke();
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stroke sample span contains unsupported values",
            );
        }
        // SAFETY: The borrowed span is validated by the exported contract.
        let samples = match unsafe {
            parse_stroke_samples(span.samples, span.sample_count, span.sample_stride_bytes)
        } {
            Ok(samples) => samples,
            Err(status) => {
                core.core.cancel_stroke();
                return status;
            }
        };
        match core.core.append_stroke(&samples) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits the active transient stroke as one document/history transaction.
///
/// # Safety
/// Core/result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_end(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The result exposes a readable size prefix before writing.
        let result_status = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if let Err(status) = result_status {
            core.core.cancel_stroke();
            return status;
        }
        let result = unsafe { &mut *result };
        match core.core.end_stroke() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Discards any active transient stroke. Calling with no active stroke is a
/// successful no-op.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_cancel(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A complete live Core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        core.core.cancel_stroke();
        INKPOD_STATUS_OK
    })
}
