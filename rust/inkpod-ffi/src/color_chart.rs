use super::*;

pub struct InkpodColorChartPreview {
    preview: ColorChartPreview,
}

const CHART_LOCKED: u32 = 1 << 0;
const CHART_HAS_SELECTION: u32 = 1 << 1;
const PREVIEW_EXCEEDS_MAXIMUM: u32 = 1 << 0;

unsafe fn parse_entries(
    entries: *const InkpodColorChartEntry,
    count: u64,
    stride: u64,
) -> Result<Vec<ColorChartEntry>, u32> {
    let count = usize::try_from(count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Color chart entry count is not representable",
        )
    })?;
    if count > inkpod_core::MAX_APPLICATION_COLORS {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Color chart entry count exceeds the supported maximum",
        ));
    }
    if count == 0 {
        return if entries.is_null() && stride == 0 {
            Ok(Vec::new())
        } else {
            Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "an empty Color chart must use null storage and zero stride",
            ))
        };
    }
    if entries.is_null() || !is_aligned(entries) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Color chart entry storage is null or misaligned",
        ));
    }
    let stride = usize::try_from(stride)
        .ok()
        .filter(|stride| {
            *stride >= size_of::<InkpodColorChartEntry>()
                && *stride % align_of::<InkpodColorChartEntry>() == 0
        })
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Color chart entry stride is too small or misaligned",
            )
        })?;
    if count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodColorChartEntry>()))
        .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Color chart entry storage overflows",
        ));
    }
    let mut parsed = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The checked caller-owned strided range covers this record.
        let raw = unsafe {
            entries
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodColorChartEntry>()
        };
        // SAFETY: The caller promises the complete strided record is readable.
        unsafe { validate_struct(raw, "InkpodColorChartEntry") }?;
        // SAFETY: Validation above established a complete aligned record.
        let record = unsafe { &*raw };
        if record.reserved != 0 || record.feature_flags != INKPOD_FEATURE_NONE {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "Color chart entry contains unsupported fields",
            ));
        }
        // SAFETY: The embedded complete color record is readable for this call.
        let color = unsafe { parse_color_value(&raw const record.color) }?;
        let name_length = usize::try_from(record.name_bytes)
            .ok()
            .filter(|length| (1..=MAX_COLOR_CHART_NAME_BYTES).contains(length))
            .ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "Color chart name length is outside bounds",
                )
            })?;
        if record.name_utf8.is_null() {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Color chart name storage is null",
            ));
        }
        // SAFETY: The caller promises `name_bytes` readable bytes for the call.
        let name =
            std::str::from_utf8(unsafe { slice::from_raw_parts(record.name_utf8, name_length) })
                .map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "Color chart name is not UTF-8",
                    )
                })?
                .to_owned();
        parsed.push(ColorChartEntry { color, name });
    }
    Ok(parsed)
}

struct EntryOutputs {
    out_color: *mut InkpodColorValue,
    name_utf8: *mut u8,
    name_capacity: u64,
    out_name_bytes: *mut u64,
    out_frequency: *mut u64,
}

unsafe fn copy_entry(color: PixelValue, name: &str, frequency: u64, output: EntryOutputs) -> u32 {
    let EntryOutputs {
        out_color,
        name_utf8,
        name_capacity,
        out_name_bytes,
        out_frequency,
    } = output;
    // SAFETY: Forwarded caller contract requires a complete writable record.
    if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") } {
        return status;
    }
    if out_name_bytes.is_null()
        || !is_aligned(out_name_bytes)
        || out_frequency.is_null()
        || !is_aligned(out_frequency)
    {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Color chart length or frequency output is null or misaligned",
        );
    }
    let color = match color_value_record(color) {
        Ok(color) => color,
        Err(status) => return status,
    };
    // SAFETY: Complete writable outputs were validated above.
    unsafe {
        out_color.write(color);
        out_name_bytes.write(name.len() as u64);
        out_frequency.write(frequency);
    }
    if name_capacity == 0 {
        if !name_utf8.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "a Color chart name size query must use null storage",
            );
        }
        return INKPOD_STATUS_OK;
    }
    if name_utf8.is_null() || name_capacity < name.len() as u64 {
        return fail(
            INKPOD_STATUS_BUFFER_TOO_SMALL,
            "Color chart name output is null or too small",
        );
    }
    // SAFETY: Capacity was checked against the exact UTF-8 byte length.
    unsafe { ptr::copy_nonoverlapping(name.as_ptr(), name_utf8, name.len()) };
    INKPOD_STATUS_OK
}

/// Returns the independent document Color chart summary and editor cursor.
///
/// # Safety
/// `core` must be a live owner-thread handle and `info` must point to a
/// complete writable record that does not overlap the Core allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_color_chart_info(
    core: *mut InkpodCore,
    info: *mut InkpodColorChartInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Caller supplies a complete writable info record.
        if let Err(status) = unsafe { validate_struct(info.cast_const(), "InkpodColorChartInfo") } {
            return status;
        }
        // SAFETY: Core was checked non-null and aligned.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let chart = match core.core.color_chart() {
            Ok(chart) => chart,
            Err(error) => return map_core_error(error),
        };
        let cursor = match core.core.editor_state() {
            Ok(editor) => editor.state.color_chart_cursor,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Info is a complete writable record.
        unsafe {
            (*info).flags = if chart.locked() { CHART_LOCKED } else { 0 }
                | if cursor.is_some() {
                    CHART_HAS_SELECTION
                } else {
                    0
                };
            (*info).feature_flags = INKPOD_FEATURE_NONE;
            (*info).entry_count = chart.entries().len() as u64;
            (*info).selected_index = cursor.map_or(0, |cursor| u64::from(cursor.index));
            (*info).page = cursor.map_or(0, |cursor| cursor.page);
            (*info).reserved = 0;
        }
        INKPOD_STATUS_OK
    })
}

/// Copies one document Color chart entry or queries its UTF-8 name size.
///
/// # Safety
/// `core` must be a live owner-thread handle. `out_color` and
/// `out_name_bytes` must be complete writable outputs; `name_utf8` must be null
/// for a size query or advertise `name_capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_color_chart_get(
    core: *mut InkpodCore,
    index: u64,
    out_color: *mut InkpodColorValue,
    name_utf8: *mut u8,
    name_capacity: u64,
    out_name_bytes: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Core was checked non-null and aligned.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let chart = match core.core.color_chart() {
            Ok(chart) => chart,
            Err(error) => return map_core_error(error),
        };
        let entry = match usize::try_from(index)
            .ok()
            .and_then(|index| chart.entries().get(index))
        {
            Some(entry) => entry,
            None => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "Color chart index is outside bounds",
                );
            }
        };
        let mut ignored_frequency = 0_u64;
        // SAFETY: Forwarded outputs follow this function's caller contract.
        unsafe {
            copy_entry(
                entry.color,
                &entry.name,
                0,
                EntryOutputs {
                    out_color,
                    name_utf8,
                    name_capacity,
                    out_name_bytes,
                    out_frequency: &mut ignored_frequency,
                },
            )
        }
    })
}

/// Replaces the complete document Color chart as one canonical edit.
///
/// # Safety
/// `core` must be a live owner-thread handle, `result` a complete writable
/// record, and `entries` must advertise `entry_count` complete strided records.
/// Every entry name span is borrowed only for this call and must be readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_color_chart_set(
    core: *mut InkpodCore,
    entries: *const InkpodColorChartEntry,
    entry_count: u64,
    entry_stride_bytes: u64,
    locked: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || locked > 1 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or Color chart lock value is invalid",
            );
        }
        // SAFETY: Caller supplies a complete writable result record.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Borrowed span validity is the caller contract.
        let entries = match unsafe { parse_entries(entries, entry_count, entry_stride_bytes) } {
            Ok(entries) => entries,
            Err(status) => return status,
        };
        // SAFETY: Core was checked non-null and aligned.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.replace_color_chart(&entries, locked != 0) {
            Ok(outcome) => {
                // SAFETY: Result is a complete writable record.
                unsafe { write_dispatch_result(&mut *result, outcome) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Creates an immutable Color chart comparison preview owned by Rust.
///
/// # Safety
/// `core` must be a live owner-thread handle, `summary` a complete writable
/// record, and `out_preview` writable owner storage containing null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_color_chart_preview_create(
    core: *mut InkpodCore,
    maximum_colors: u32,
    quantization_bits: u32,
    summary: *mut InkpodColorChartPreviewSummary,
    out_preview: *mut *mut InkpodColorChartPreview,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_preview.is_null() || !is_aligned(out_preview)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Color chart preview input or owner output is invalid",
            );
        }
        // SAFETY: Owner slot is aligned and readable.
        if !unsafe { out_preview.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "Color chart preview output already owns a handle",
            );
        }
        // SAFETY: Caller supplies a complete writable summary.
        if let Err(status) =
            unsafe { validate_struct(summary.cast_const(), "InkpodColorChartPreviewSummary") }
        {
            return status;
        }
        let bits = match u8::try_from(quantization_bits) {
            Ok(bits) => bits,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "Color chart quantization exceeds u8",
                );
            }
        };
        // SAFETY: Core was checked non-null and aligned.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let preview = match core
            .core
            .preview_color_chart_generation(maximum_colors as usize, bits)
        {
            Ok(preview) => preview,
            Err(error) => return map_core_error(error),
        };
        let comparison = preview.summary();
        // SAFETY: Summary and owner slots are complete writable outputs.
        unsafe {
            (*summary).flags = if comparison.exceeds_maximum {
                PREVIEW_EXCEEDS_MAXIMUM
            } else {
                0
            };
            (*summary).feature_flags = INKPOD_FEATURE_NONE;
            (*summary).base_document_revision = preview.base_document_revision();
            (*summary).entry_count = preview.entries().len() as u64;
            (*summary).source_unique_color_count = comparison.source_unique_colors;
            (*summary).retained_color_count = comparison.retained_colors;
            (*summary).added_color_count = comparison.added_colors;
            (*summary).removed_color_count = comparison.removed_colors;
            (*summary).reserved = 0;
            out_preview.write(Box::into_raw(Box::new(InkpodColorChartPreview { preview })));
        }
        INKPOD_STATUS_OK
    })
}

/// Creates an immutable Color chart preview with cooperative cancellation.
///
/// # Safety
/// The synchronous-preview requirements apply, and `task` must be a live
/// one-shot task handle that remains allocated for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_color_chart_preview_create_task(
    core: *mut InkpodCore,
    maximum_colors: u32,
    quantization_bits: u32,
    task: *mut InkpodTask,
    summary: *mut InkpodColorChartPreviewSummary,
    out_preview: *mut *mut InkpodColorChartPreview,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || task.is_null()
            || !is_aligned(task)
            || out_preview.is_null()
            || !is_aligned(out_preview)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Color chart preview task input or owner output is invalid",
            );
        }
        // SAFETY: Owner slot is aligned and readable.
        if !unsafe { out_preview.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "Color chart preview output already owns a handle",
            );
        }
        // SAFETY: Caller supplies a complete writable summary.
        if let Err(status) =
            unsafe { validate_struct(summary.cast_const(), "InkpodColorChartPreviewSummary") }
        {
            return status;
        }
        let bits = match u8::try_from(quantization_bits) {
            Ok(bits) => bits,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "Color chart quantization exceeds u8",
                );
            }
        };
        // SAFETY: Live task and Core handles are required by the caller contract.
        let task = unsafe { &*task };
        if !task.begin() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "Color chart preview task has already run",
            );
        }
        // SAFETY: Core was checked non-null and aligned.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            task.finish(thread_status);
            return thread_status;
        }
        let generated = core.core.preview_color_chart_generation_with_cancel(
            maximum_colors as usize,
            bits,
            |completed, total| task.progress(completed, total),
        );
        let preview = match generated {
            Ok(preview) => preview,
            Err(error) => {
                let status = map_core_error(error);
                task.finish(status);
                return status;
            }
        };
        let comparison = preview.summary();
        // SAFETY: Summary and owner slots are complete writable outputs.
        unsafe {
            (*summary).flags = if comparison.exceeds_maximum {
                PREVIEW_EXCEEDS_MAXIMUM
            } else {
                0
            };
            (*summary).feature_flags = INKPOD_FEATURE_NONE;
            (*summary).base_document_revision = preview.base_document_revision();
            (*summary).entry_count = preview.entries().len() as u64;
            (*summary).source_unique_color_count = comparison.source_unique_colors;
            (*summary).retained_color_count = comparison.retained_colors;
            (*summary).added_color_count = comparison.added_colors;
            (*summary).removed_color_count = comparison.removed_colors;
            (*summary).reserved = 0;
            out_preview.write(Box::into_raw(Box::new(InkpodColorChartPreview { preview })));
        }
        task.finish(INKPOD_STATUS_OK);
        INKPOD_STATUS_OK
    })
}

/// Copies one immutable preview entry or queries its UTF-8 name size.
///
/// # Safety
/// `preview` must be a live unreleased handle. All scalar record outputs must
/// be complete and writable; `name_utf8` must be null for a size query or
/// advertise `name_capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_color_chart_preview_get(
    preview: *const InkpodColorChartPreview,
    index: u64,
    out_color: *mut InkpodColorValue,
    name_utf8: *mut u8,
    name_capacity: u64,
    out_name_bytes: *mut u64,
    out_frequency: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Color chart preview handle is null or misaligned",
            );
        }
        // SAFETY: Caller supplies a live immutable preview handle.
        let preview = unsafe { &*preview };
        let entry = match usize::try_from(index)
            .ok()
            .and_then(|index| preview.preview.entries().get(index))
        {
            Some(entry) => entry,
            None => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "Color chart preview index is outside bounds",
                );
            }
        };
        // SAFETY: Forwarded outputs follow this function's caller contract.
        unsafe {
            copy_entry(
                entry.color,
                &entry.name,
                entry.frequency,
                EntryOutputs {
                    out_color,
                    name_utf8,
                    name_capacity,
                    out_name_bytes,
                    out_frequency,
                },
            )
        }
    })
}

/// Applies one live same-document, same-revision preview as one Undo unit.
///
/// # Safety
/// `core` must be a live owner-thread handle, `preview` a live immutable
/// unreleased handle, and `result` a complete non-overlapping writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_color_chart_preview_apply(
    core: *mut InkpodCore,
    preview: *const InkpodColorChartPreview,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Color chart preview apply input is invalid",
            );
        }
        // SAFETY: Caller supplies a complete writable result record.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live records were checked above.
        let core = unsafe { &mut *core };
        let preview = unsafe { &*preview };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.apply_color_chart_preview(&preview.preview) {
            Ok(outcome) => {
                // SAFETY: Result is a complete writable record.
                unsafe { write_dispatch_result(&mut *result, outcome) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Releases one Rust-owned immutable Color chart preview.
///
/// # Safety
/// `preview` must be writable owner storage containing exactly one live handle
/// returned by a preview-create function. On success the slot is set to null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_color_chart_preview_release(
    preview: *mut *mut InkpodColorChartPreview,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Color chart preview owner slot is null or misaligned",
            );
        }
        // SAFETY: Owner slot is aligned and readable.
        let handle = unsafe { preview.read() };
        if handle.is_null() || !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "Color chart preview handle is null or misaligned",
            );
        }
        // SAFETY: The live Box allocation is uniquely owned by this slot.
        unsafe {
            drop(Box::from_raw(handle));
            preview.write(ptr::null_mut());
        }
        INKPOD_STATUS_OK
    })
}
