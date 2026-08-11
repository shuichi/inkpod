use super::*;

const GEOMETRY_FLAGS: u64 = INKPOD_GEOMETRY_OUTLINE
    | INKPOD_GEOMETRY_FILL
    | INKPOD_GEOMETRY_CLOSE_PATH
    | INKPOD_GEOMETRY_BEZIER_SEGMENTS
    | INKPOD_GEOMETRY_CONSTRAIN_45_DEGREES
    | INKPOD_GEOMETRY_FROM_CENTER
    | INKPOD_GEOMETRY_TAPER_START
    | INKPOD_GEOMETRY_TAPER_END
    | INKPOD_GEOMETRY_SQUARE_CROSS_SECTION;
const GEOMETRY_RESOLVE_FLAGS: u64 = INKPOD_GEOMETRY_RESOLVE_BYPASS_SNAP;

/// Resolves bounded pointer samples to document-space geometry points.
///
/// # Safety
/// Every pointer must name a complete, aligned, live, non-overlapping record or
/// span on the Core owner thread. Input samples are copied and never retained.
/// Output points must be initialized with complete size-versioned records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_geometry_points_resolve(
    core: *mut InkpodCore,
    input: *const InkpodGeometryPointResolveInput,
    result: *mut InkpodGeometryPointResolveResult,
    points: *mut InkpodGeometryPoint,
    point_capacity: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "geometry point resolution core is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodGeometryPointResolveInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(result.cast_const(), "InkpodGeometryPointResolveResult") }
        {
            return status;
        }
        // SAFETY: Complete records were validated above.
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        result.reserved = 0;
        result.view_revision = 0;
        result.point_count = 0;
        if input.feature_flags & !GEOMETRY_RESOLVE_FLAGS != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "geometry point resolution contains unsupported flags",
            );
        }
        if input.sample_count == 0 || input.sample_count > inkpod_core::MAX_GEOMETRY_POINTS as u64 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "geometry point resolution sample count is outside bounds",
            );
        }
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: The exported contract requires the advertised strided span.
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: The complete live Core was validated above.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let snap_mode = if input.feature_flags & INKPOD_GEOMETRY_RESOLVE_BYPASS_SNAP != 0 {
            GeometrySnapMode::Bypass
        } else {
            GeometrySnapMode::UseViewState
        };
        let resolved = match core.core.resolve_geometry_points_for_view(
            input.view_id,
            input.expected_view_revision,
            coordinate_space,
            &samples,
            snap_mode,
        ) {
            Ok(value) => value,
            Err(error) => return map_core_error(error),
        };
        result.view_revision = resolved.view_revision;
        result.point_count = resolved.points.len() as u64;
        if point_capacity < result.point_count {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "geometry point output capacity is too small",
            );
        }
        if points.is_null() || !is_aligned(points) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "geometry point output span is null or misaligned",
            );
        }
        for index in 0..resolved.points.len() {
            // SAFETY: Capacity and the caller-provided live output span cover
            // every resolved point; each record prefix is validated before writes.
            let output = unsafe { points.add(index) };
            if let Err(status) =
                unsafe { validate_struct(output.cast_const(), "InkpodGeometryPoint") }
            {
                return status;
            }
        }
        for (index, point) in resolved.points.iter().enumerate() {
            // SAFETY: The full output span was validated without writes above.
            let output = unsafe { &mut *points.add(index) };
            output.reserved = 0;
            output.x = point.x;
            output.y = point.y;
        }
        INKPOD_STATUS_OK
    })
}

/// Applies one bounded raster or vector geometry request as one canonical edit.
///
/// # Safety
/// Every pointer must name a complete, aligned, live, non-overlapping record on
/// the Core owner thread. The strided point span is copied and never retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_geometry_apply(
    core: *mut InkpodCore,
    input: *const InkpodGeometryInput,
    result: *mut InkpodDispatchResult,
    out_path_id: *mut u64,
    out_fill_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_path_id.is_null()
            || !is_aligned(out_path_id)
            || out_fill_id.is_null()
            || !is_aligned(out_fill_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "geometry output ID storage is null or misaligned",
            );
        }
        // SAFETY: Writable outputs are required by contract and zeroed before work.
        unsafe {
            out_path_id.write(0);
            out_fill_id.write(0);
        }
        let (core, request, base_revision) = match unsafe { geometry_call(core, input) } {
            Ok(call) => call,
            Err(status) => return status,
        };
        if base_revision != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "one-shot geometry apply requires a zero base revision",
            );
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        match core.core.apply_geometry(&request) {
            Ok(commit) => {
                write_geometry_commit(commit, unsafe { &mut *result }, out_path_id, out_fill_id);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Begins an immutable-base geometry preview.
///
/// # Safety
/// The geometry-apply input requirements apply; `out_info` is a complete
/// writable record. `base_revision` must identify the current document.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_geometry_preview_begin(
    core: *mut InkpodCore,
    input: *const InkpodGeometryInput,
    out_info: *mut InkpodGeometryPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let (core, request, base_revision) = match unsafe { geometry_call(core, input) } {
            Ok(call) => call,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodGeometryPreviewInfo") }
        {
            return status;
        }
        match core.core.begin_geometry_preview(base_revision, &request) {
            Ok(info) => {
                write_geometry_preview(unsafe { &mut *out_info }, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Rebuilds the active geometry preview from its immutable base.
///
/// # Safety
/// The geometry-preview-begin requirements apply. Target and primitive cannot
/// change within a session.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_geometry_preview_update(
    core: *mut InkpodCore,
    input: *const InkpodGeometryInput,
    out_info: *mut InkpodGeometryPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let (core, request, base_revision) = match unsafe { geometry_call(core, input) } {
            Ok(call) => call,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodGeometryPreviewInfo") }
        {
            return status;
        }
        match core.core.update_geometry_preview(base_revision, &request) {
            Ok(info) => {
                write_geometry_preview(unsafe { &mut *out_info }, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits the active preview through the same canonical executor as apply.
///
/// # Safety
/// Core/result/ID outputs must be complete, aligned, live, and non-overlapping
/// on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_geometry_preview_commit(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
    out_path_id: *mut u64,
    out_fill_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_path_id.is_null()
            || !is_aligned(out_path_id)
            || out_fill_id.is_null()
            || !is_aligned(out_fill_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "geometry preview commit pointer is null or misaligned",
            );
        }
        // SAFETY: Writable outputs are required by contract.
        unsafe {
            out_path_id.write(0);
            out_fill_id.write(0);
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live Core was checked above.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.commit_geometry_preview() {
            Ok(commit) => {
                write_geometry_commit(commit, unsafe { &mut *result }, out_path_id, out_fill_id);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Cancels the active geometry preview without publishing document state.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_geometry_preview_cancel(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Complete live Core was checked above.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.cancel_geometry_preview() {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

unsafe fn geometry_call<'a>(
    core: *mut InkpodCore,
    input: *const InkpodGeometryInput,
) -> Result<(&'a mut InkpodCore, GeometryRequest, u64), u32> {
    if core.is_null() || !is_aligned(core) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "geometry core is null or misaligned",
        ));
    }
    // SAFETY: Callers require a complete readable input record.
    unsafe { validate_struct(input, "InkpodGeometryInput") }?;
    // SAFETY: Complete live objects were validated above. The caller confines
    // the returned exclusive borrow to the enclosing FFI call.
    let core = unsafe { &mut *core };
    let status = validate_core_thread(core);
    if status != INKPOD_STATUS_OK {
        return Err(status);
    }
    // SAFETY: Complete input record was validated above.
    let input = unsafe { &*input };
    let request = unsafe { parse_geometry_input(input) }?;
    Ok((core, request, input.base_revision))
}

unsafe fn parse_geometry_input(input: &InkpodGeometryInput) -> Result<GeometryRequest, u32> {
    if input.feature_flags & !GEOMETRY_FLAGS != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "geometry input contains unsupported flags",
        ));
    }
    let primitive = match input.primitive {
        INKPOD_GEOMETRY_LINE => GeometryPrimitive::Line,
        INKPOD_GEOMETRY_CURVE => GeometryPrimitive::Curve,
        INKPOD_GEOMETRY_RECTANGLE => GeometryPrimitive::Rectangle,
        INKPOD_GEOMETRY_ELLIPSE => GeometryPrimitive::Ellipse,
        INKPOD_GEOMETRY_POLYGON => GeometryPrimitive::Polygon,
        INKPOD_GEOMETRY_POLYLINE => GeometryPrimitive::Polyline,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "geometry primitive is not defined",
            ));
        }
    };
    let count = usize::try_from(input.point_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "geometry point count is not representable",
        )
    })?;
    let stride = usize::try_from(input.point_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "geometry point stride is not representable",
        )
    })?;
    if count == 0
        || count > inkpod_core::MAX_GEOMETRY_POINTS
        || input.points.is_null()
        || !is_aligned(input.points)
        || stride < size_of::<InkpodGeometryPoint>()
        || stride % align_of::<InkpodGeometryPoint>() != 0
        || count
            .checked_mul(stride)
            .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "geometry point span is null, misaligned, or outside bounds",
        ));
    }
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The bounded, aligned strided span contains this record prefix.
        let point = unsafe {
            input
                .points
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodGeometryPoint>()
        };
        // SAFETY: Each advertised point record exposes a readable size prefix.
        let size = unsafe { validate_struct(point, "InkpodGeometryPoint") }?;
        if u64::from(size) > input.point_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "geometry point struct_size exceeds its stride",
            ));
        }
        // SAFETY: The complete known record is readable after validation.
        let point = unsafe { &*point };
        if point.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "geometry point reserved field is not zero",
            ));
        }
        points.push(PointF32 {
            x: point.x,
            y: point.y,
        });
    }
    // SAFETY: Nested color records are complete fields of the validated input.
    let outline_color = unsafe { parse_color_value(&raw const input.outline_color) }?;
    let fill_color = unsafe { parse_color_value(&raw const input.fill_color) }?;
    let polygon_sides = u16::try_from(input.polygon_sides).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "geometry polygon side count is not representable",
        )
    })?;
    Ok(GeometryRequest {
        plane_id: input.plane_id,
        primitive,
        points,
        outline_color,
        fill_color,
        outline_width: input.outline_width,
        options: GeometryOptions {
            outline: input.feature_flags & INKPOD_GEOMETRY_OUTLINE != 0,
            fill: input.feature_flags & INKPOD_GEOMETRY_FILL != 0,
            close_path: input.feature_flags & INKPOD_GEOMETRY_CLOSE_PATH != 0,
            bezier_segments: input.feature_flags & INKPOD_GEOMETRY_BEZIER_SEGMENTS != 0,
            constrain_45_degrees: input.feature_flags & INKPOD_GEOMETRY_CONSTRAIN_45_DEGREES != 0,
            from_center: input.feature_flags & INKPOD_GEOMETRY_FROM_CENTER != 0,
            taper_start: input.feature_flags & INKPOD_GEOMETRY_TAPER_START != 0,
            taper_end: input.feature_flags & INKPOD_GEOMETRY_TAPER_END != 0,
            cross_section: if input.feature_flags & INKPOD_GEOMETRY_SQUARE_CROSS_SECTION != 0 {
                GeometryCrossSection::Square
            } else {
                GeometryCrossSection::Round
            },
            aspect_ratio_q16: input.aspect_ratio_q16,
            polygon_sides,
            rotation_turns: input.rotation_turns,
        },
    })
}

fn write_geometry_preview(output: &mut InkpodGeometryPreviewInfo, info: GeometryPreviewInfo) {
    output.reserved = 0;
    output.plane_id = info.plane_id;
    output.base_revision = info.base_revision;
    output.preview_revision = info.preview_revision;
}

fn write_geometry_commit(
    commit: GeometryCommit,
    result: &mut InkpodDispatchResult,
    out_path_id: *mut u64,
    out_fill_id: *mut u64,
) {
    write_dispatch_result(result, commit.dispatch);
    // SAFETY: Callers validated both writable scalar outputs.
    unsafe {
        out_path_id.write(commit.path_id);
        out_fill_id.write(commit.fill_id);
    }
}
