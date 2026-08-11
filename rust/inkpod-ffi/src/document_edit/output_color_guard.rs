//! Output-color guard C ABI adapter.

use super::*;

/// Selects committed visible composite pixels outside a closed output-color guard.
///
/// # Safety
/// `core`, `request`, `task`, and `result` must be non-null, aligned, and point to
/// complete live records for this owner-thread call. All records remain caller
/// owned and are borrowed only until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_select_output_color_guard(
    core: *mut InkpodCore,
    request: *const InkpodOutputColorGuardRequest,
    task: *mut InkpodTask,
    result: *mut InkpodOutputColorGuardResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(request, "InkpodOutputColorGuardRequest") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(result.cast_const(), "InkpodOutputColorGuardResult") }
        {
            return status;
        }
        // SAFETY: Complete live records are required by the function contract.
        let core = unsafe { &mut *core };
        let request = unsafe { &*request };
        let task = unsafe { &*task };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if request.reserved != 0 || request.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "output-color guard request contains unsupported fields",
            );
        }
        let profile = match request.profile {
            INKPOD_OUTPUT_COLOR_GUARD_BT709_CONSERVATIVE_YCBCR => {
                OutputColorGuardProfile::Bt709ConservativeYCbCr
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "output-color guard profile is unknown",
                );
            }
        };
        let operation = match parse_selection_operation(request.operation) {
            Ok(value) => value,
            Err(status) => return status,
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "task is not READY");
        }
        let status = match core.core.select_output_color_guard_with_cancel(
            profile,
            operation,
            request.base_document_revision,
            |completed, total| task.progress(completed, total),
        ) {
            Ok(outcome) => {
                result.reserved = 0;
                result.feature_flags = INKPOD_FEATURE_NONE;
                result.revision = outcome.dispatch.revision();
                result.accepted_command_count = outcome.dispatch.accepted_commands();
                result.scanned_pixel_count = outcome.summary.scanned_pixel_count;
                result.selected_pixel_count = outcome.summary.selected_pixel_count;
                result.transparent_pixel_count = outcome.summary.transparent_pixel_count;
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}
