use super::*;

/// Applies one copied linear/radial multi-stop gradient as one Undo unit.
///
/// # Safety
/// Core/input/result and every advertised stop/color record must be complete,
/// aligned, readable, non-overlapping, and live for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_gradient(
    core: *mut InkpodCore,
    input: *const InkpodGradientInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodGradientInput") } {
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
        let gradient = match unsafe { parse_gradient_input(input) } {
            Ok(gradient) => gradient,
            Err(status) => return status,
        };
        match core.core.apply_gradient_to_plane(input.plane_id, &gradient) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one copied airbrush dab as one Undo unit.
///
/// # Safety
/// Core/input/result must satisfy the normal owner-thread ABI contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_airbrush(
    core: *mut InkpodCore,
    input: *const InkpodAirbrushInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodAirbrushInput") } {
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
        let stroke = match unsafe { parse_airbrush_input(input) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core.core.apply_airbrush_to_plane(input.plane_id, stroke) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a copied, pressure-aware airbrush gesture as one Undo unit.
///
/// # Safety
/// Core/input/result and the advertised sample span must be complete and live
/// for this owner-thread call. No borrowed pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_airbrush_gesture(
    core: *mut InkpodCore,
    input: *const InkpodAirbrushGestureInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodAirbrushGestureInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete records and borrowed spans are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags
            & !(INKPOD_EFFECT_FLAG_PRESSURE_SIZE | INKPOD_EFFECT_FLAG_PRESSURE_OPACITY)
            != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "airbrush gesture contains unsupported flags",
            );
        }
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let color = match unsafe { parse_color_value(&input.color) } {
            Ok(value) => match value.rgba16() {
                Some(value) => value,
                None => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "airbrush gesture color must be RGBA",
                    );
                }
            },
            Err(status) => return status,
        };
        let gesture = AirbrushGesture {
            samples: Vec::new(),
            radius_milli: input.radius_milli,
            hardness_milli: input.hardness_milli,
            spacing_milli: input.spacing_milli,
            opacity_milli: input.opacity_milli,
            fade_milli: input.fade_milli,
            pressure_size: input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_SIZE != 0,
            pressure_opacity: input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_OPACITY != 0,
            continuous_dabs: input.continuous_dabs,
            color,
        };
        match core.core.apply_airbrush_gesture_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            &samples,
            gesture,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies the copied boundary-color airbrush effect as one Undo unit.
///
/// # Safety
/// Core/input/result and every advertised color record follow the normal
/// owner-thread span contract and are borrowed only for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_boundary_airbrush(
    core: *mut InkpodCore,
    input: *const InkpodBoundaryAirbrushInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodBoundaryAirbrushInput") } {
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
        let effect = match unsafe { parse_boundary_airbrush_input(input) } {
            Ok(effect) => effect,
            Err(status) => return status,
        };
        match core
            .core
            .apply_boundary_airbrush_to_plane(input.plane_id, &effect)
        {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a bounded Gaussian blur effect as one Undo unit.
///
/// # Safety
/// Core/input/result must satisfy the normal owner-thread ABI contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_blur(
    core: *mut InkpodCore,
    input: *const InkpodBlurEffectInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodBlurEffectInput") } {
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
        if input.feature_flags != INKPOD_FEATURE_NONE
            || input.reserved != 0
            || input.reserved_2 != 0
            || input.reserved_3 != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "blur-effect input contains unsupported flags or reserved values",
            );
        }
        match core
            .core
            .apply_blur_to_plane(input.plane_id, input.radius, input.strength_milli)
        {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one bounded offset stamp from the immutable source plane state.
///
/// # Safety
/// Core/input/result must satisfy the normal owner-thread ABI contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_stamp(
    core: *mut InkpodCore,
    input: *const InkpodStampInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodStampInput") } {
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
        if input.feature_flags != INKPOD_FEATURE_NONE
            || input.reserved != 0
            || input.reserved_2 != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stamp input contains unsupported flags or reserved values",
            );
        }
        let stamp = Stamp {
            source_x: input.source_x,
            source_y: input.source_y,
            destination_x: input.destination_x,
            destination_y: input.destination_y,
            width: input.width,
            height: input.height,
            opacity_milli: input.opacity_milli,
        };
        match core.core.apply_stamp_to_plane(input.plane_id, stamp) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a copied pressure-aware clone-stamp gesture as one Undo unit.
///
/// # Safety
/// The airbrush-gesture safety requirements apply, including the embedded source.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_stamp_gesture(
    core: *mut InkpodCore,
    input: *const InkpodStampGestureInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodStampGestureInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live records and spans are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.reserved != 0
            || input.feature_flags
                & !(INKPOD_EFFECT_FLAG_PRESSURE_SIZE | INKPOD_EFFECT_FLAG_PRESSURE_OPACITY)
                != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stamp gesture contains unsupported flags",
            );
        }
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let source = match unsafe {
            parse_stroke_samples(&input.source, 1, size_of::<InkpodStrokeSample>() as u64)
        } {
            Ok(mut value) => value.remove(0),
            Err(status) => return status,
        };
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let gesture = StampGesture {
            source_x_milli: 0,
            source_y_milli: 0,
            samples: Vec::new(),
            radius_milli: input.radius_milli,
            hardness_milli: input.hardness_milli,
            spacing_milli: input.spacing_milli,
            opacity_milli: input.opacity_milli,
            shape: match input.shape {
                INKPOD_STAMP_ROUND => StampShape::Round,
                INKPOD_STAMP_SQUARE => StampShape::Square,
                _ => {
                    return fail(INKPOD_STATUS_INVALID_ARGUMENT, "stamp shape is unknown");
                }
            },
            pressure_size: input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_SIZE != 0,
            pressure_opacity: input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_OPACITY != 0,
        };
        match core.core.apply_stamp_gesture_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            source,
            &samples,
            gesture,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies the blur tool inside a copied pen/rectangle/polyline/lasso region.
///
/// # Safety
/// Core/input/result and the embedded region span must remain live for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_blur_tool(
    core: *mut InkpodCore,
    input: *const InkpodBlurToolInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodBlurToolInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live records and embedded spans are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags & !INKPOD_EFFECT_FLAG_PRESSURE_SIZE != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "blur tool contains unsupported fields",
            );
        }
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let kind = match parse_effect_region_kind(input.shape) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        match core.core.apply_blur_tool_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            kind,
            &samples,
            input.diameter,
            input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_SIZE != 0,
            input.radius,
            input.strength_milli,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
