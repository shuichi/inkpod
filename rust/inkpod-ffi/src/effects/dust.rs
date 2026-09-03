use super::*;

/// Runs bounded dust removal with progress/cancellation and atomic commit.
///
/// # Safety
/// The Core/input/result records, optional embedded region span, and READY task
/// must remain live until this owner-thread call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_dust_remove(
    core: *mut InkpodCore,
    input: *const InkpodDustInput,
    task: *mut InkpodTask,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodDustInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live records and task are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let task = unsafe { &*task };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || input.use_region > 1 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "dust-removal input contains unsupported fields",
            );
        }
        let mode = match input.mode {
            INKPOD_DUST_REMOVE_FOREGROUND => DustMode::RemoveForeground,
            INKPOD_DUST_FILL_TRANSPARENT_HOLES => DustMode::FillTransparentHoles,
            INKPOD_DUST_REPLACE_COLOR_OUTLIERS => DustMode::ReplaceColorOutliers,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "dust-removal mode is unknown",
                );
            }
        };
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let kind = if input.use_region != 0 {
            match parse_effect_region_kind(input.shape) {
                Ok(value) => Some(value),
                Err(status) => return status,
            }
        } else {
            None
        };
        let samples = if input.use_region != 0 {
            match unsafe {
                parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
            } {
                Ok(value) => value,
                Err(status) => return status,
            }
        } else {
            if input.sample_count != 0 || !input.samples.is_null() || input.sample_stride_bytes != 0
            {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "full-image dust removal must not carry region samples",
                );
            }
            Vec::new()
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "task is not READY");
        }
        let status = match core.core.apply_dust_removal_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            kind,
            &samples,
            input.diameter,
            DustRemoval {
                background: Default::default(),
                mode,
                maximum_pixels: input.maximum_pixels,
            },
            |completed, total| task.progress(completed, total),
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}

/// Begins a non-committing dust-removal preview with progress/cancellation.
///
/// # Safety
/// The dust-remove safety requirements apply; output is a complete writable
/// preview-info record. Apply/cancel uses the filter-preview apply/cancel API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_dust_preview_begin(
    core: *mut InkpodCore,
    input: *const InkpodDustInput,
    task: *mut InkpodTask,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodDustInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records and task are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let task = unsafe { &*task };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || input.use_region > 1 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "dust-removal input contains unsupported fields",
            );
        }
        let mode = match input.mode {
            INKPOD_DUST_REMOVE_FOREGROUND => DustMode::RemoveForeground,
            INKPOD_DUST_FILL_TRANSPARENT_HOLES => DustMode::FillTransparentHoles,
            INKPOD_DUST_REPLACE_COLOR_OUTLIERS => DustMode::ReplaceColorOutliers,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "dust-removal mode is unknown",
                );
            }
        };
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let kind = if input.use_region != 0 {
            match parse_effect_region_kind(input.shape) {
                Ok(value) => Some(value),
                Err(status) => return status,
            }
        } else {
            None
        };
        let samples = if input.use_region != 0 {
            match unsafe {
                parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
            } {
                Ok(value) => value,
                Err(status) => return status,
            }
        } else {
            if input.sample_count != 0 || !input.samples.is_null() || input.sample_stride_bytes != 0
            {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "full-image dust removal must not carry region samples",
                );
            }
            Vec::new()
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "task is not READY");
        }
        let status = match core.core.begin_dust_preview_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            kind,
            &samples,
            input.diameter,
            DustRemoval {
                background: Default::default(),
                mode,
                maximum_pixels: input.maximum_pixels,
            },
            |completed, total| task.progress(completed, total),
        ) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}
