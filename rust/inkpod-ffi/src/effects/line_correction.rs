use super::*;
use inkpod_core::{LineBackground, LineCorrection, LineCorrectionRequest, LineWidthMode};

#[cfg(test)]
#[path = "../../tests/unit/line_correction.rs"]
mod tests;

/// Applies one captured line edit on the Core owner thread. Borrowed input and
/// samples are consumed before return; only success writes the result record.
///
/// # Safety
/// All records and the READY task must be live, aligned, correctly sized, and
/// exclusively owned for this call. The sample span must cover count/stride.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_line_correct(
    core: *mut InkpodCore,
    input: *const InkpodLineCorrectionInput,
    task: *mut InkpodTask,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        // SAFETY: Caller supplies the complete live records stated above.
        unsafe { run(core, input, task, result, std::ptr::null_mut(), false) }
    })
}

/// Builds an isolated line-edit preview. Apply/cancel uses the existing filter
/// preview API; neither cancelled nor failed work publishes partial content.
///
/// # Safety
/// The same ownership/lifetime/thread requirements as `inkpod_core_line_correct`
/// apply; output must be one complete writable preview-info record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_line_preview_begin(
    core: *mut InkpodCore,
    input: *const InkpodLineCorrectionInput,
    task: *mut InkpodTask,
    output: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        // SAFETY: Caller supplies the complete live records stated above.
        unsafe { run(core, input, task, std::ptr::null_mut(), output, true) }
    })
}

unsafe fn run(
    core: *mut InkpodCore,
    input: *const InkpodLineCorrectionInput,
    task: *mut InkpodTask,
    result: *mut InkpodDispatchResult,
    output: *mut InkpodFilterPreviewInfo,
    preview: bool,
) -> u32 {
    clear_last_error();
    if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "null or misaligned line-edit handle",
        );
    }
    // SAFETY: Validation reads only the caller-provided size before the body.
    if let Err(status) = unsafe { validate_struct(input, "InkpodLineCorrectionInput") } {
        return status;
    }
    let validated = if preview {
        // SAFETY: Output size is validated before dereferencing its body.
        unsafe { validate_struct(output.cast_const(), "InkpodFilterPreviewInfo") }
    } else {
        // SAFETY: Output size is validated before dereferencing its body.
        unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
    };
    if let Err(status) = validated {
        return status;
    }
    // SAFETY: Above checks and the live-record contract cover these references.
    let (core, input, task) = unsafe { (&mut *core, &*input, &*task) };
    let status = validate_core_thread(core);
    if status != INKPOD_STATUS_OK {
        return status;
    }
    if input.feature_flags != 0 {
        return fail(INKPOD_STATUS_UNSUPPORTED, "unsupported line-edit flags");
    }
    if input.use_region > 1
        || input.pressure_size > 1
        || input.screen_size > 1
        || input.view_zoom_q16 <= 0
    {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "invalid line-edit construction",
        );
    }
    let document = match core.core.document_info() {
        Ok(info) => info,
        Err(error) => return map_core_error(error),
    };
    if document.document_revision != input.expected_document_revision {
        return fail(
            INKPOD_STATUS_INVALID_STATE,
            "line-edit document revision is stale",
        );
    }
    let background = match input.background_mode {
        INKPOD_LINE_BACKGROUND_DEFAULT if input.background_rgba == [0; 4] => {
            LineBackground::PlaneDefault
        }
        INKPOD_LINE_BACKGROUND_TRANSPARENT if input.background_rgba == [0; 4] => {
            LineBackground::Transparent
        }
        INKPOD_LINE_BACKGROUND_COLOR => LineBackground::TransparentOrColor(input.background_rgba),
        _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "invalid line background"),
    };
    let correction = match input.mode {
        INKPOD_LINE_REMOVE_DUST | INKPOD_LINE_FILL_HOLES | INKPOD_LINE_REPLACE_OUTLIERS
            if input.gap == 0 && input.line_width == 0 =>
        {
            LineCorrection::Dust(DustRemoval {
                mode: match input.mode {
                    INKPOD_LINE_REMOVE_DUST => DustMode::RemoveForeground,
                    INKPOD_LINE_FILL_HOLES => DustMode::FillTransparentHoles,
                    _ => DustMode::ReplaceColorOutliers,
                },
                maximum_pixels: input.amount,
                background,
            })
        }
        INKPOD_LINE_CONNECT if input.amount == 0 => LineCorrection::Connect {
            gap: input.gap,
            width: input.line_width,
            background,
        },
        INKPOD_LINE_THICKEN | INKPOD_LINE_THIN | INKPOD_LINE_UNIFORM
            if input.gap == 0 && input.line_width == 0 =>
        {
            LineCorrection::Width {
                mode: match input.mode {
                    INKPOD_LINE_THICKEN => LineWidthMode::Thicken,
                    INKPOD_LINE_THIN => LineWidthMode::Thin,
                    _ => LineWidthMode::Uniform,
                },
                amount: input.amount,
                background,
            }
        }
        _ => {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid line correction or inactive parameters",
            );
        }
    };
    let brush_shape = match input.brush_shape {
        INKPOD_TRACE_ROUND => TraceBrushShape::Round,
        INKPOD_TRACE_SQUARE => TraceBrushShape::Square,
        _ => {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid line-region brush shape",
            );
        }
    };
    let space = match parse_coordinate_space(input.coordinate_space) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let region = if input.use_region != 0 {
        let kind = match parse_effect_region_kind(input.shape) {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: The input span remains borrowed only during this call.
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        match core.core.line_correction_region_for_view(
            input.view_id,
            space,
            kind,
            &samples,
            input.diameter,
        ) {
            Ok(value) => Some(value),
            Err(error) => return map_core_error(error),
        }
    } else {
        if input.sample_count != 0 || !input.samples.is_null() || input.sample_stride_bytes != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "global line edit carries a sample span",
            );
        }
        None
    };
    let request = LineCorrectionRequest {
        plane_id: input.plane_id,
        region,
        construction: SelectionConstructionOptions {
            trace: TraceBrushOptions {
                shape: brush_shape,
                pressure_size: input.pressure_size != 0,
                screen_size: input.screen_size != 0,
                view_zoom_q16: input.view_zoom_q16,
            },
            ..Default::default()
        },
        correction,
    };
    if !task.begin() {
        return fail(INKPOD_STATUS_INVALID_STATE, "task is not READY");
    }
    let status = if preview {
        match core
            .core
            .begin_line_correction_preview(&request, |done, total| task.progress(done, total))
        {
            Ok(info) => {
                // SAFETY: Validated writable output is published only on success.
                write_filter_preview_info(unsafe { &mut *output }, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    } else {
        match core
            .core
            .apply_line_correction(&request, |done, total| task.progress(done, total))
        {
            Ok(outcome) => {
                // SAFETY: Validated writable output is published only on success.
                write_dispatch_result(unsafe { &mut *result }, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    };
    task.finish(status);
    status
}
