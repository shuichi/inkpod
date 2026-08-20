use super::*;

/// Returns a read-only exact match summary for one scoped color replacement.
///
/// # Safety
/// `core` must be live on its owner thread. `input`, its optional strided point
/// span, and `output` must be readable/writable as documented and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_preview_scoped_color_replace(
    core: *mut InkpodCore,
    input: *const InkpodScopedColorReplaceInput,
    output: *mut InkpodScopedColorReplacePreview,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodScopedColorReplaceInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodScopedColorReplacePreview") }
        {
            return status;
        }
        let request = match unsafe { parse_scoped_color_replace(input) } {
            Ok(request) => request,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.preview_scoped_color_replace(&request) {
            Ok(preview) => {
                // SAFETY: The complete output record was validated above.
                write_preview(unsafe { &mut *output }, preview);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits one scoped color replacement through the canonical Core executor.
///
/// # Safety
/// The input contract is identical to [`inkpod_core_preview_scoped_color_replace`].
/// `result` must expose a writable complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_scoped_color_replace(
    core: *mut InkpodCore,
    input: *const InkpodScopedColorReplaceInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodScopedColorReplaceInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let request = match unsafe { parse_scoped_color_replace(input) } {
            Ok(request) => request,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.apply_scoped_color_replace(request) {
            Ok(outcome) => {
                // SAFETY: The complete output record was validated above.
                write_dispatch_result(unsafe { &mut *result }, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

unsafe fn parse_scoped_color_replace(
    input: *const InkpodScopedColorReplaceInput,
) -> Result<ScopedColorReplaceRequest, u32> {
    // SAFETY: The caller validates the complete record prefix.
    let input = unsafe { &*input };
    if input.feature_flags & !INKPOD_COLOR_REPLACE_FLAGS != 0
        || input.reserved != 0
        || input.reserved_2 != 0
        || input.plane_id == 0
        || input.base_document_revision == 0
        || input.point_count > MAX_SELECTION_POINT_COUNT
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "scoped color replacement flags, reserved values, ID, revision, or count are invalid",
        ));
    }
    let mode = match input.mode {
        INKPOD_COLOR_REPLACE_RASTER_COLOR => ScopedColorReplaceMode::RasterColor,
        INKPOD_COLOR_REPLACE_RASTER_MAIN_LINE => ScopedColorReplaceMode::RasterMainLine,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "scoped color replacement mode is not defined",
            ));
        }
    };
    let target = unsafe { parse_color_value(ptr::addr_of!(input.target_color)) }?;
    let replacement = unsafe { parse_color_value(ptr::addr_of!(input.replacement_color)) }?;
    let has_region = input.feature_flags & INKPOD_COLOR_REPLACE_HAS_REGION != 0;
    let region = if has_region {
        Some(unsafe { parse_region(input) }?)
    } else {
        if input.shape != 0
            || input.bounds != InkpodFrameRect::default()
            || !input.points.is_null()
            || input.point_count != 0
            || input.point_stride_bytes != 0
            || input.diameter != 0.0
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "region-free scoped color replacement carries region data",
            ));
        }
        None
    };
    Ok(ScopedColorReplaceRequest {
        base_document_revision: input.base_document_revision,
        plane_id: input.plane_id,
        mode,
        target,
        replacement,
        region,
    })
}

unsafe fn parse_region(input: &InkpodScopedColorReplaceInput) -> Result<SelectionShape, u32> {
    let needs_points = matches!(
        input.shape,
        INKPOD_SELECTION_TRACE | INKPOD_SELECTION_POLYLINE | INKPOD_SELECTION_LASSO
    );
    if !needs_points {
        if input.shape != INKPOD_SELECTION_RECTANGLE
            || !input.points.is_null()
            || input.point_count != 0
            || input.point_stride_bytes != 0
            || input.diameter != 0.0
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "scoped color replacement region shape or point-free fields are invalid",
            ));
        }
        return Ok(SelectionShape::Rectangle(RectI32 {
            x: input.bounds.x,
            y: input.bounds.y,
            width: input.bounds.width,
            height: input.bounds.height,
        }));
    }
    if input.bounds != InkpodFrameRect::default()
        || input.points.is_null()
        || !is_aligned(input.points)
        || input.point_count == 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "scoped color replacement point span is invalid",
        ));
    }
    let stride = usize::try_from(input.point_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "scoped color replacement point stride is not representable",
        )
    })?;
    if stride < size_of::<InkpodSelectionPoint>()
        || stride % align_of::<InkpodSelectionPoint>() != 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "scoped color replacement point stride is invalid",
        ));
    }
    let count = usize::try_from(input.point_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "scoped color replacement point count is not representable",
        )
    })?;
    if count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodSelectionPoint>()))
        .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "scoped color replacement point span overflows",
        ));
    }
    let mut points = Vec::new();
    points.try_reserve(count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_STATE,
            "scoped color replacement point allocation failed",
        )
    })?;
    for index in 0..count {
        // SAFETY: Count, stride, and total span were validated above.
        let pointer = unsafe {
            input
                .points
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodSelectionPoint>()
        };
        unsafe { validate_struct(pointer, "InkpodSelectionPoint") }?;
        // SAFETY: The complete strided point record is readable.
        let point = unsafe { &*pointer };
        if point.struct_size as usize > stride || point.reserved != 0 || point.reserved2 != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "scoped color replacement point record is invalid",
            ));
        }
        points.push(PointF32 {
            x: point.x,
            y: point.y,
        });
    }
    match input.shape {
        INKPOD_SELECTION_TRACE if input.diameter.is_finite() && input.diameter > 0.0 => {
            Ok(SelectionShape::Trace {
                points,
                diameter: input.diameter,
            })
        }
        INKPOD_SELECTION_POLYLINE => Ok(SelectionShape::Polyline(points)),
        INKPOD_SELECTION_LASSO => Ok(SelectionShape::Lasso(points)),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "scoped color replacement region is invalid",
        )),
    }
}

fn write_preview(output: &mut InkpodScopedColorReplacePreview, preview: ScopedColorReplacePreview) {
    output.feature_flags = u32::from(preview.affected_bounds.is_some());
    output.base_document_revision = preview.base_document_revision;
    output.matched_pixels = preview.matched_pixels;
    output.affected_bounds = preview
        .affected_bounds
        .map(|bounds| InkpodFrameRect {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
        })
        .unwrap_or_default();
}
