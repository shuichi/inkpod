//! Immutable C ABI snapshot of canonical procedure-history visualization rows.

use super::*;

/// Replays the current in-memory journal into one Rust-owned immutable row set.
///
/// # Safety
/// `core` must be live on its owner thread. `out_visualization` must be aligned,
/// writable storage containing null. On success the caller owns the returned
/// handle until [`inkpod_history_visualization_release`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_history_visualization_create(
    core: *mut InkpodCore,
    out_visualization: *mut *mut InkpodHistoryVisualization,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_visualization.is_null()
            || !is_aligned(out_visualization)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history visualization pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable owner storage.
        if !unsafe { out_visualization.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history visualization output already owns a live handle",
            );
        }
        // SAFETY: The complete live Core pointer was validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.history_visualization_rows() {
            Ok(rows) => {
                let handle = Box::new(InkpodHistoryVisualization {
                    rows: rows.into_boxed_slice(),
                });
                // SAFETY: Output storage is writable and currently null.
                unsafe { out_visualization.write(Box::into_raw(handle)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Replays the journal into an immutable row set with cooperative cancellation.
///
/// # Safety
/// Core/output ownership matches [`inkpod_core_history_visualization_create`].
/// `task` must be one live ready task, may be cancelled from any thread, and is
/// externally synchronized against release for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_history_visualization_create_with_task(
    core: *mut InkpodCore,
    task: *mut InkpodTask,
    out_visualization: *mut *mut InkpodHistoryVisualization,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || task.is_null()
            || !is_aligned(task)
            || out_visualization.is_null()
            || !is_aligned(out_visualization)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history visualization task pointer is null or misaligned",
            );
        }
        if !unsafe { out_visualization.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history visualization output already owns a live handle",
            );
        }
        // SAFETY: Complete live handles were validated above.
        let task = unsafe { &*task };
        if !task.begin() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "history visualization task has already run",
            );
        }
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            task.finish(thread_status);
            return thread_status;
        }
        match core
            .core
            .history_visualization_rows_with_progress(|completed, total| {
                task.progress(completed, total)
            }) {
            Ok(rows) => {
                let handle = Box::new(InkpodHistoryVisualization {
                    rows: rows.into_boxed_slice(),
                });
                unsafe { out_visualization.write(Box::into_raw(handle)) };
                task.finish(INKPOD_STATUS_OK);
                INKPOD_STATUS_OK
            }
            Err(error) => {
                let status = map_core_error(error);
                task.finish(status);
                status
            }
        }
    })
}

/// Returns the number of immutable Commit rows in a visualization snapshot.
///
/// # Safety
/// `visualization` must be live and externally synchronized against release;
/// `out_row_count` must be aligned writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_history_visualization_row_count(
    visualization: *const InkpodHistoryVisualization,
    out_row_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if visualization.is_null()
            || !is_aligned(visualization)
            || out_row_count.is_null()
            || !is_aligned(out_row_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history visualization count pointer is null or misaligned",
            );
        }
        // SAFETY: Live immutable input and writable output are required by contract.
        let visualization = unsafe { &*visualization };
        unsafe { out_row_count.write(visualization.rows.len() as u64) };
        INKPOD_STATUS_OK
    })
}

/// Copies metadata, UTF-8 text, and straight-alpha RGBA8 pixels for one row.
///
/// # Safety
/// `visualization` must be live and synchronized against release. `output` must
/// be a complete writable record. Each advertised nonzero-capacity span must be
/// writable; zero capacity requires a null pointer and performs a size query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_history_visualization_row_get(
    visualization: *const InkpodHistoryVisualization,
    row_index: u64,
    output: *mut InkpodHistoryVisualizationRowBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if visualization.is_null() || !is_aligned(visualization) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history visualization handle is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodHistoryVisualizationRowBuffer") }
        {
            return status;
        }
        // SAFETY: Complete live input and writable output were validated above.
        let visualization = unsafe { &*visualization };
        let output = unsafe { &mut *output };
        if output.flags != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history visualization row flags are invalid",
            );
        }
        let index = match usize::try_from(row_index) {
            Ok(index) => index,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "history visualization row index is not representable",
                );
            }
        };
        let row = match visualization.rows.get(index) {
            Some(row) => row,
            None => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "history visualization row index is out of range",
                );
            }
        };

        output.journal_event_id = row.journal_event_id.get();
        output.procedure_id = row.procedure_id.get();
        output.committed_state_id = row.committed_state_id.get();
        output.branch_id = row.branch_id.get();
        output.primitive_id = row.primitive_id.get();
        output.thumbnail_width = row.thumbnail.width;
        output.thumbnail_height = row.thumbnail.height;
        output.thumbnail_stride_bytes = row.thumbnail.width.saturating_mul(4);
        output.thumbnail_checksum = row.thumbnail.checksum;
        output.primitive_name_bytes = row.primitive_name.len() as u64;
        output.arguments_bytes = row.arguments.len() as u64;
        output.thumbnail_bytes = row.thumbnail.rgba8.len() as u64;

        let spans = [
            (
                output.primitive_name_utf8,
                output.primitive_name_capacity,
                row.primitive_name.as_bytes(),
                "primitive name",
            ),
            (
                output.arguments_utf8,
                output.arguments_capacity,
                row.arguments.as_bytes(),
                "arguments",
            ),
            (
                output.thumbnail_rgba8,
                output.thumbnail_capacity,
                row.thumbnail.rgba8.as_slice(),
                "thumbnail",
            ),
        ];
        let size_query = spans.iter().all(|(_, capacity, _, _)| *capacity == 0);
        for (pointer, capacity, bytes, label) in spans {
            if capacity == 0 {
                if !pointer.is_null() {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        &format!("zero-capacity history {label} buffer must be null"),
                    );
                }
            } else if pointer.is_null() || capacity > isize::MAX as u64 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    &format!("history {label} buffer is invalid"),
                );
            }
            if !size_query && capacity < bytes.len() as u64 {
                return fail(
                    INKPOD_STATUS_BUFFER_TOO_SMALL,
                    "one or more history visualization buffers are too small",
                );
            }
        }

        if size_query {
            return INKPOD_STATUS_OK;
        }

        for (pointer, _, bytes, _) in spans {
            if !bytes.is_empty() {
                // SAFETY: Capacity checks above establish a complete writable span.
                unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len()) };
            }
        }
        INKPOD_STATUS_OK
    })
}

/// Releases one immutable visualization handle and nulls caller owner storage.
///
/// # Safety
/// `visualization` must be writable storage containing null or a uniquely owned
/// live handle returned by [`inkpod_core_history_visualization_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_history_visualization_release(
    visualization: *mut *mut InkpodHistoryVisualization,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if visualization.is_null() || !is_aligned(visualization) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history visualization owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable unique owner storage.
        let handle = unsafe { visualization.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history visualization handle is misaligned",
            );
        }
        // SAFETY: Null before consuming the unique Box exactly once.
        unsafe { visualization.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}
