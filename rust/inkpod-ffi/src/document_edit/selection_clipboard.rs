use super::*;

/// Applies one bounded selection shape and boolean operation.
///
/// # Safety
/// `core` must be live on its owner thread. `input` and `result` must be valid
/// non-overlapping records, and an advertised point span must be fully readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_selection(
    core: *mut InkpodCore,
    input: *const InkpodSelectionInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    unsafe { apply_selection_ffi(core, input, result, None) }
}

/// Applies one bounded selection gesture using its captured stable editor target.
///
/// # Safety
/// The pointer and span contract is identical to [`inkpod_core_apply_selection`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_selection_for_editor_target(
    core: *mut InkpodCore,
    layer_id: u64,
    plane_id: u64,
    input: *const InkpodSelectionInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    unsafe {
        apply_selection_ffi(
            core,
            input,
            result,
            Some(EditorTarget { layer_id, plane_id }),
        )
    }
}

unsafe fn apply_selection_ffi(
    core: *mut InkpodCore,
    input: *const InkpodSelectionInput,
    result: *mut InkpodDispatchResult,
    target: Option<EditorTarget>,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodSelectionInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects and ranges are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0
            || input.point_count > MAX_SELECTION_POINT_COUNT
            || input.construction_flags & !INKPOD_SELECTION_CONSTRUCTION_FLAGS != 0
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "selection reserved/count value is invalid",
            );
        }
        let operation = match parse_selection_operation(input.operation) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let needs_points = matches!(
            input.shape,
            INKPOD_SELECTION_LASSO
                | INKPOD_SELECTION_POLYLINE
                | INKPOD_SELECTION_TRACE
                | INKPOD_SELECTION_RECTANGLE
                | INKPOD_SELECTION_ELLIPSE
        );
        let mut points = Vec::new();
        let has_points = input.point_count != 0 || !input.points.is_null();
        if needs_points && has_points {
            if input.points.is_null() || !is_aligned(input.points) || input.point_count == 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection point span is invalid",
                );
            }
            let stride = match usize::try_from(input.point_stride_bytes) {
                Ok(stride)
                    if stride >= size_of::<InkpodSelectionPoint>()
                        && stride % align_of::<InkpodSelectionPoint>() == 0 =>
                {
                    stride
                }
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "selection point stride is invalid",
                    );
                }
            };
            let count = match usize::try_from(input.point_count) {
                Ok(count) => count,
                Err(_) => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "selection point count is not representable",
                    );
                }
            };
            if count
                .saturating_sub(1)
                .checked_mul(stride)
                .and_then(|offset| offset.checked_add(size_of::<InkpodSelectionPoint>()))
                .is_none_or(|bytes| bytes > isize::MAX as usize)
            {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection point span overflows",
                );
            }
            points.reserve(count);
            for index in 0..count {
                // SAFETY: Checked count/stride and caller-readable storage cover this record.
                let pointer = unsafe {
                    input
                        .points
                        .cast::<u8>()
                        .add(index * stride)
                        .cast::<InkpodSelectionPoint>()
                };
                if let Err(status) = unsafe { validate_struct(pointer, "InkpodSelectionPoint") } {
                    return status;
                }
                // SAFETY: Record prefix and containing storage were validated above.
                let point = unsafe { &*pointer };
                if point.struct_size as usize > stride {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "selection point record exceeds its stride",
                    );
                }
                if point.reserved != 0 || point.reserved2 != 0 {
                    return fail(
                        INKPOD_STATUS_UNSUPPORTED,
                        "selection point reserved value is not zero",
                    );
                }
                points.push(PointF32 {
                    x: point.x,
                    y: point.y,
                });
            }
        } else if input.point_count != 0 || !input.points.is_null() || input.point_stride_bytes != 0
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "point-free selection must not carry a point span",
            );
        }
        let interpretation = match input.interpretation {
            INKPOD_RANGE_NORMAL => RangeInterpretation::Normal,
            INKPOD_RANGE_TIGHT => RangeInterpretation::Tight,
            INKPOD_RANGE_ENCLOSED_INTERIOR => RangeInterpretation::EnclosedInterior,
            INKPOD_RANGE_DRAWING => RangeInterpretation::Drawing,
            INKPOD_RANGE_BOUNDARY => RangeInterpretation::Boundary,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection range interpretation is not defined",
                );
            }
        };
        let trace_shape = match input.trace_shape {
            INKPOD_TRACE_ROUND => TraceBrushShape::Round,
            INKPOD_TRACE_SQUARE => TraceBrushShape::Square,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection trace brush shape is not defined",
                );
            }
        };
        let options = SelectionConstructionOptions {
            aspect_ratio_q16: input.aspect_ratio_q16,
            from_center: input.construction_flags & INKPOD_SELECTION_FROM_CENTER != 0,
            constrain_rotation_45: input.construction_flags
                & INKPOD_SELECTION_CONSTRAIN_ROTATION_45
                != 0,
            rotation_turns: input.rotation_turns,
            trace: TraceBrushOptions {
                shape: trace_shape,
                pressure_size: input.construction_flags & INKPOD_SELECTION_TRACE_PRESSURE_SIZE != 0,
                screen_size: input.construction_flags & INKPOD_SELECTION_TRACE_SCREEN_SIZE != 0,
                view_zoom_q16: input.view_zoom_q16,
            },
        };
        let shape = match input.shape {
            INKPOD_SELECTION_RECTANGLE if points.len() == 2 => SelectionShape::RectangleGesture {
                anchor: points[0],
                current: points[1],
            },
            INKPOD_SELECTION_RECTANGLE if points.is_empty() => SelectionShape::Rectangle(RectI32 {
                x: input.bounds.x,
                y: input.bounds.y,
                width: input.bounds.width,
                height: input.bounds.height,
            }),
            INKPOD_SELECTION_ELLIPSE if points.len() == 2 => SelectionShape::EllipseGesture {
                anchor: points[0],
                current: points[1],
            },
            INKPOD_SELECTION_ELLIPSE if points.is_empty() => SelectionShape::Ellipse(RectI32 {
                x: input.bounds.x,
                y: input.bounds.y,
                width: input.bounds.width,
                height: input.bounds.height,
            }),
            INKPOD_SELECTION_LASSO => SelectionShape::Lasso(points),
            INKPOD_SELECTION_POLYLINE => SelectionShape::Polyline(points),
            INKPOD_SELECTION_TRACE => {
                let count = usize::try_from(input.point_count).unwrap_or(0);
                let mut samples = Vec::with_capacity(count);
                for index in 0..count {
                    // SAFETY: The point span was validated and remains borrowed for this call.
                    let point = unsafe {
                        &*input
                            .points
                            .cast::<u8>()
                            .add(index * input.point_stride_bytes as usize)
                            .cast::<InkpodSelectionPoint>()
                    };
                    samples.push(SelectionSample {
                        x: point.x,
                        y: point.y,
                        pressure: point.pressure,
                    });
                }
                SelectionShape::TraceBrush {
                    samples,
                    diameter: input.diameter,
                }
            }
            INKPOD_SELECTION_WAND => SelectionShape::Wand {
                x: input.seed_x,
                y: input.seed_y,
                tolerance: input.tolerance,
                gap_close: match u8::try_from(input.gap_close) {
                    Ok(value) => value,
                    Err(_) => {
                        return fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "wand gap close exceeds its bound",
                        );
                    }
                },
            },
            INKPOD_SELECTION_RECTANGLE | INKPOD_SELECTION_ELLIPSE => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "rectangle/ellipse selection requires zero or two points",
                );
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection shape is not defined",
                );
            }
        };
        let outcome = match target {
            Some(target) => core.core.apply_selection_with_options_for_editor_target(
                &shape,
                operation,
                interpretation,
                options,
                target,
            ),
            None => {
                core.core
                    .apply_selection_with_options(&shape, operation, interpretation, options)
            }
        };
        match outcome {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Selects equal or different pixels from the active typed plane.
///
/// # Safety
/// `core` must be live on its owner thread, `color` must expose a complete
/// readable record, and `result` must be writable and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_select_color(
    core: *mut InkpodCore,
    color: *const InkpodColorValue,
    tolerance: u16,
    different: u32,
    operation: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    unsafe { select_color_ffi(core, color, tolerance, different, operation, result, None) }
}

/// Selects equal or different pixels from a captured stable editor target.
///
/// # Safety
/// The pointer contract is identical to [`inkpod_core_select_color`]. The
/// layer/plane pair must identify one current document plane.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_select_color_for_editor_target(
    core: *mut InkpodCore,
    layer_id: u64,
    plane_id: u64,
    color: *const InkpodColorValue,
    tolerance: u16,
    different: u32,
    operation: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    unsafe {
        select_color_ffi(
            core,
            color,
            tolerance,
            different,
            operation,
            result,
            Some(EditorTarget { layer_id, plane_id }),
        )
    }
}

unsafe fn select_color_ffi(
    core: *mut InkpodCore,
    color: *const InkpodColorValue,
    tolerance: u16,
    different: u32,
    operation: u32,
    result: *mut InkpodDispatchResult,
    target: Option<EditorTarget>,
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
        let color = match unsafe { parse_color_value(color) } {
            Ok(color) => color,
            Err(status) => return status,
        };
        let different = match different {
            0 => false,
            1 => true,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "different-selection flag is not boolean",
                );
            }
        };
        let operation = match parse_selection_operation(operation) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let outcome = match target {
            Some(target) => core
                .core
                .select_color_for_editor_target(color, tolerance, different, operation, target),
            None => core
                .core
                .select_color(color, tolerance, different, operation),
        };
        match outcome {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies the current typed selection into a Rust-owned clipboard handle.
///
/// # Safety
/// `core` must be live on its owner thread and `out_clipboard` must be writable
/// non-overlapping storage for one handle pointer. That storage must not contain
/// a live clipboard handle, because this function overwrites it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_clipboard_copy(
    core: *mut InkpodCore,
    out_clipboard: *mut *mut InkpodClipboard,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_clipboard.is_null()
            || !is_aligned(out_clipboard)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard copy pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides writable handle storage.
        unsafe { out_clipboard.write(ptr::null_mut()) };
        // SAFETY: Caller contract requires one live owner-thread core.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.copy_selection() {
            Ok(payload) => {
                let clipboard = Box::new(InkpodClipboard { payload });
                // SAFETY: Output storage receives exactly one Rust Box owner.
                unsafe { out_clipboard.write(Box::into_raw(clipboard)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Renders the first typed clipboard plane into caller-owned straight RGBA8.
/// A null buffer with zero capacity performs a size query.
///
/// # Safety
/// `clipboard` must remain live for the call and `output` must be a complete
/// writable record whose advertised pixel range is writable and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_clipboard_render_rgba8(
    clipboard: *const InkpodClipboard,
    output: *mut InkpodClipboardRasterBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if clipboard.is_null() || !is_aligned(clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard handle is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodClipboardRasterBuffer") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let clipboard = unsafe { &*clipboard };
        let output = unsafe { &mut *output };
        if output.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "clipboard raster flags are not supported",
            );
        }
        let Some(plane) = clipboard.payload.planes.first() else {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "clipboard has no plane payload",
            );
        };
        let width = match u32::try_from(clipboard.payload.bounds.width) {
            Ok(width) if width != 0 => width,
            _ => return fail(INKPOD_STATUS_INVALID_STATE, "clipboard width is invalid"),
        };
        let height = match u32::try_from(clipboard.payload.bounds.height) {
            Ok(height) if height != 0 => height,
            _ => return fail(INKPOD_STATUS_INVALID_STATE, "clipboard height is invalid"),
        };
        let packed_stride = match u64::from(width).checked_mul(4) {
            Some(stride) => stride,
            None => return fail(INKPOD_STATUS_INVALID_STATE, "clipboard stride overflows"),
        };
        output.origin_x = clipboard.payload.bounds.x;
        output.origin_y = clipboard.payload.bounds.y;
        output.width = width;
        output.height = height;
        if output.pixels_rgba8.is_null() && output.pixel_capacity == 0 {
            output.row_stride_bytes = packed_stride;
            output.required_bytes = match packed_stride.checked_mul(u64::from(height)) {
                Some(bytes) => bytes,
                None => return fail(INKPOD_STATUS_INVALID_STATE, "clipboard bytes overflow"),
            };
            return INKPOD_STATUS_OK;
        }
        let stride = if output.row_stride_bytes == 0 {
            packed_stride
        } else {
            output.row_stride_bytes
        };
        if stride < packed_stride {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard row stride is too small",
            );
        }
        let required = match stride.checked_mul(u64::from(height)) {
            Some(bytes) => bytes,
            None => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "clipboard bytes overflow"),
        };
        output.required_bytes = required;
        output.row_stride_bytes = stride;
        if output.pixels_rgba8.is_null() || output.pixel_capacity < required {
            return INKPOD_STATUS_BUFFER_TOO_SMALL;
        }
        let required = match usize::try_from(required) {
            Ok(required) => required,
            Err(_) => return INKPOD_STATUS_BUFFER_TOO_SMALL,
        };
        // SAFETY: Caller advertises a writable output region of `required` bytes.
        let pixels = unsafe { slice::from_raw_parts_mut(output.pixels_rgba8, required) };
        pixels.fill(0);
        for pixel in &plane.pixels {
            let relative_x = i64::from(pixel.x) - i64::from(output.origin_x);
            let relative_y = i64::from(pixel.y) - i64::from(output.origin_y);
            if relative_x < 0
                || relative_y < 0
                || relative_x >= i64::from(width)
                || relative_y >= i64::from(height)
            {
                continue;
            }
            let offset = match u64::try_from(relative_y)
                .ok()
                .and_then(|y| y.checked_mul(stride))
                .and_then(|row| {
                    u64::try_from(relative_x)
                        .ok()
                        .and_then(|x| x.checked_mul(4))
                        .and_then(|column| row.checked_add(column))
                })
                .and_then(|offset| usize::try_from(offset).ok())
            {
                Some(offset) if offset + 4 <= pixels.len() => offset,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_STATE,
                        "clipboard pixel offset overflows",
                    );
                }
            };
            pixels[offset..offset + 4].copy_from_slice(&clipboard_pixel_rgba8(pixel.value));
        }
        INKPOD_STATUS_OK
    })
}

/// Creates a Rust-owned typed clipboard from caller-owned straight RGBA8.
///
/// # Safety
/// `input` and its pixel range must be readable for the call. `out_clipboard`
/// must be writable storage that does not already own a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_clipboard_create_rgba8(
    input: *const InkpodClipboardRgbaInput,
    out_clipboard: *mut *mut InkpodClipboard,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_clipboard.is_null() || !is_aligned(out_clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard output is null or misaligned",
            );
        }
        // SAFETY: Writable owner storage is required by contract.
        unsafe { out_clipboard.write(ptr::null_mut()) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodClipboardRgbaInput") } {
            return status;
        }
        // SAFETY: Complete input record was validated above.
        let input = unsafe { &*input };
        if input.reserved != 0 || input.width == 0 || input.height == 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard raster metadata is invalid",
            );
        }
        let pixel_count = match u64::from(input.width).checked_mul(u64::from(input.height)) {
            Some(count) if count <= 16_777_216 => count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard raster exceeds work bound",
                );
            }
        };
        let packed_stride = u64::from(input.width) * 4;
        if input.row_stride_bytes < packed_stride {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard input stride is too small",
            );
        }
        let required = match input.row_stride_bytes.checked_mul(u64::from(input.height)) {
            Some(required) => required,
            None => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard input bytes overflow",
                );
            }
        };
        if input.pixels_rgba8.is_null() || input.pixel_bytes < required {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard input pixels are incomplete",
            );
        }
        let required = match usize::try_from(required) {
            Ok(required) => required,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard input is too large",
                );
            }
        };
        // SAFETY: Caller advertises a readable range covering `required` bytes.
        let source = unsafe { slice::from_raw_parts(input.pixels_rgba8, required) };
        let mut pixels = Vec::with_capacity(pixel_count as usize);
        for y in 0..input.height {
            for x in 0..input.width {
                let offset = (u64::from(y) * input.row_stride_bytes + u64::from(x) * 4) as usize;
                let rgba = [
                    source[offset],
                    source[offset + 1],
                    source[offset + 2],
                    source[offset + 3],
                ];
                if rgba != [0; 4] {
                    let pixel_x = match i64::from(input.origin_x).checked_add(i64::from(x)) {
                        Some(value) => value,
                        None => {
                            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "clipboard X overflows");
                        }
                    };
                    let pixel_y = match i64::from(input.origin_y).checked_add(i64::from(y)) {
                        Some(value) => value,
                        None => {
                            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "clipboard Y overflows");
                        }
                    };
                    let (pixel_x, pixel_y) = match (i32::try_from(pixel_x), i32::try_from(pixel_y))
                    {
                        (Ok(x), Ok(y)) => (x, y),
                        _ => {
                            return fail(
                                INKPOD_STATUS_INVALID_ARGUMENT,
                                "clipboard coordinate overflows",
                            );
                        }
                    };
                    pixels.push(ClipboardPixel {
                        x: pixel_x,
                        y: pixel_y,
                        value: PixelValue::Rgba(rgba),
                    });
                }
            }
        }
        let width = match i32::try_from(input.width) {
            Ok(width) => width,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard width exceeds i32",
                );
            }
        };
        let height = match i32::try_from(input.height) {
            Ok(height) => height,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard height exceeds i32",
                );
            }
        };
        let payload = ClipboardPayload {
            source_document_uuid: 1,
            bounds: RectI32 {
                x: input.origin_x,
                y: input.origin_y,
                width,
                height,
            },
            planes: vec![ClipboardPlane {
                kind: PlaneType::Raster,
                pixel_format: PixelFormat::StraightRgba8,
                origin_x: input.origin_x,
                origin_y: input.origin_y,
                pixels,
                vector_paths: Vec::new(),
                vector_fills: Vec::new(),
            }],
        };
        let clipboard = Box::new(InkpodClipboard { payload });
        // SAFETY: Output storage receives one unique Rust Box owner.
        unsafe { out_clipboard.write(Box::into_raw(clipboard)) };
        INKPOD_STATUS_OK
    })
}

/// Releases one Rust-owned clipboard handle and nulls caller storage.
///
/// # Safety
/// `clipboard` must be writable storage containing either null or exactly one
/// live handle previously returned by `inkpod_core_clipboard_copy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_clipboard_release(clipboard: *mut *mut InkpodClipboard) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if clipboard.is_null() || !is_aligned(clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller contract provides readable/writable owner storage.
        let handle = unsafe { clipboard.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard handle is misaligned",
            );
        }
        // SAFETY: Null first, then consume the unique Box owner exactly once.
        unsafe { clipboard.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Starts a coordinate-preserving floating paste.
///
/// # Safety
/// `core` must be live on its owner thread and `clipboard` must remain a live,
/// immutable clipboard handle for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_paste_begin(
    core: *mut InkpodCore,
    clipboard: *const InkpodClipboard,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || clipboard.is_null() || !is_aligned(clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "paste pointer is null or misaligned",
            );
        }
        // SAFETY: Live handles are required by the exported contract.
        let core = unsafe { &mut *core };
        let clipboard = unsafe { &*clipboard };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.begin_paste(&clipboard.payload) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Starts floating paste with explicit compatible or active-plane conversion routing.
///
/// # Safety
/// `core` and `clipboard` must remain live and aligned for the call, on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_paste_begin_mode(
    core: *mut InkpodCore,
    clipboard: *const InkpodClipboard,
    mode: u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || clipboard.is_null() || !is_aligned(clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "paste pointer is null or misaligned",
            );
        }
        // SAFETY: Live handles are required by the exported contract.
        let core = unsafe { &mut *core };
        let clipboard = unsafe { &*clipboard };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let result = match mode {
            1 => core.core.begin_paste(&clipboard.payload),
            2 => core
                .core
                .begin_paste_to_active_converted(&clipboard.payload),
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "paste mode is not defined"),
        };
        match result {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Starts a converted floating paste whose typed destination plane is created
/// only when the floating transaction is committed.
///
/// # Safety
/// `core`, `clipboard`, and `target` must remain live, aligned, and readable for
/// the call on the Core owner thread. Any target name span is borrowed only for
/// this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_paste_begin_new_plane(
    core: *mut InkpodCore,
    clipboard: *const InkpodClipboard,
    target: *const InkpodTreeEdit,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || clipboard.is_null() || !is_aligned(clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "paste pointer is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(target, "InkpodTreeEdit") } {
            return status;
        }
        // SAFETY: Live handles and a complete target record are required by contract.
        let core = unsafe { &mut *core };
        let clipboard = unsafe { &*clipboard };
        let target = unsafe { &*target };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if target.operation != INKPOD_TREE_CREATE_PLANE
            || target.object_id != 0
            || target.destination_index != 0
            || target.flags != (INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "new-plane paste target is not a canonical create-plane request",
            );
        }
        let kind = match parse_plane_type(target.kind) {
            Ok(kind) => kind,
            Err(status) => return status,
        };
        let format = match parse_storage_format(target.pixel_format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        // SAFETY: The input contract includes the advertised name byte range.
        let name = match unsafe { name_from_utf8(target.name_utf8, target.name_bytes) } {
            Ok(name) => name,
            Err(status) => return status,
        };
        match core.core.begin_paste_to_new_plane_converted(
            &clipboard.payload,
            target.parent_id,
            kind,
            format,
            name,
            target.opacity_milli,
        ) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces the current floating paste transform.
///
/// # Safety
/// `core` must be live on its owner thread and `input` must expose a complete,
/// readable record that does not overlap Core storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_floating_transform(
    core: *mut InkpodCore,
    input: *const InkpodFloatingTransform,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structure exposes a readable size prefix.
        if let Err(status) = unsafe { validate_struct(input, "InkpodFloatingTransform") } {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let anchor = match input.anchor {
            INKPOD_TRANSFORM_ANCHOR_TOP_LEFT => FloatingTransformAnchor::TopLeft,
            INKPOD_TRANSFORM_ANCHOR_TOP_RIGHT => FloatingTransformAnchor::TopRight,
            INKPOD_TRANSFORM_ANCHOR_CENTER => FloatingTransformAnchor::Center,
            INKPOD_TRANSFORM_ANCHOR_BOTTOM_LEFT => FloatingTransformAnchor::BottomLeft,
            INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT => FloatingTransformAnchor::BottomRight,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "floating transform anchor is invalid",
                );
            }
        };
        match core.core.set_floating_transform(FloatingTransform {
            anchor,
            target_x: input.target_x,
            target_y: input.target_y,
            scale_x: input.scale_x,
            scale_y: input.scale_y,
            rotation_degrees: input.rotation_degrees,
        }) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits the current floating paste as one history transaction.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_floating_commit(
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
        match core.core.commit_floating() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Cancels the current floating paste without editing the document.
///
/// # Safety
/// `core` must be a live handle used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_floating_cancel(core: *mut InkpodCore) -> u32 {
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
        core.core.cancel_floating();
        INKPOD_STATUS_OK
    })
}

/// Clears selected content from the active editable plane as one history transaction.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must be a complete writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_clear_selected_content(
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
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.clear_selected_content() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
