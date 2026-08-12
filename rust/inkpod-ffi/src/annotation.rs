use super::*;

fn annotation_kind(value: u32) -> Result<AnnotationKind, u32> {
    match value {
        INKPOD_ANNOTATION_TEXT => Ok(AnnotationKind::Text),
        INKPOD_ANNOTATION_STROKE => Ok(AnnotationKind::Stroke),
        INKPOD_ANNOTATION_LEADER => Ok(AnnotationKind::Leader),
        INKPOD_ANNOTATION_VALUE => Ok(AnnotationKind::Value),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "annotation kind is unknown",
        )),
    }
}

fn annotation_output(value: u32) -> Result<AnnotationOutput, u32> {
    match value {
        INKPOD_ANNOTATION_OUTPUT_NORMAL => Ok(AnnotationOutput::Normal),
        INKPOD_ANNOTATION_OUTPUT_INSTRUCTION => Ok(AnnotationOutput::Instruction),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "annotation output kind is unknown",
        )),
    }
}

unsafe fn utf8_value(
    pointer: *const u8,
    byte_count: u64,
    maximum: usize,
    field: &'static str,
) -> Result<String, u32> {
    let length = usize::try_from(byte_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "annotation UTF-8 length is not representable",
        )
    })?;
    if length > maximum || length > isize::MAX as usize {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "annotation UTF-8 length exceeds its bound",
        ));
    }
    if length == 0 {
        if !pointer.is_null() {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "zero-length annotation UTF-8 span must be null",
            ));
        }
        return Ok(String::new());
    }
    if pointer.is_null() {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "annotation UTF-8 pointer is null",
        ));
    }
    // SAFETY: The public contract requires the bounded borrowed span to remain readable.
    let bytes = unsafe { slice::from_raw_parts(pointer, length) };
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| fail(INKPOD_STATUS_INVALID_ARGUMENT, field))
}

unsafe fn annotation_points(
    pointer: *const InkpodAnnotationPoint,
    count: u64,
    stride_bytes: u64,
) -> Result<Vec<AnnotationPoint>, u32> {
    let count = usize::try_from(count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "annotation point count is not representable",
        )
    })?;
    if count > inkpod_core::MAX_ANNOTATION_POINTS {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "annotation point count exceeds its bound",
        ));
    }
    if count == 0 {
        if !pointer.is_null() || stride_bytes != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "empty annotation point span must be null with zero stride",
            ));
        }
        return Ok(Vec::new());
    }
    let stride = usize::try_from(stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "annotation point stride is not representable",
        )
    })?;
    if pointer.is_null()
        || !is_aligned(pointer)
        || stride < size_of::<InkpodAnnotationPoint>()
        || stride % align_of::<InkpodAnnotationPoint>() != 0
        || count
            .checked_sub(1)
            .and_then(|last| last.checked_mul(stride))
            .and_then(|offset| offset.checked_add(size_of::<InkpodAnnotationPoint>()))
            .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "annotation point span is invalid",
        ));
    }
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The bounded strided span was validated above.
        let record = unsafe {
            pointer
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodAnnotationPoint>()
        };
        // SAFETY: Every record exposes a complete size prefix and current layout.
        unsafe { validate_struct(record, "InkpodAnnotationPoint") }?;
        // SAFETY: Complete readable record was validated above.
        let record = unsafe { &*record };
        if record.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "annotation point reserved field is not zero",
            ));
        }
        points.push(AnnotationPoint {
            x_milli: record.x_milli,
            y_milli: record.y_milli,
        });
    }
    Ok(points)
}

unsafe fn parse_annotation_input(
    input: *const InkpodAnnotationObjectInput,
) -> Result<AnnotationObjectInput, u32> {
    // SAFETY: Caller supplies a complete current input record.
    unsafe { validate_struct(input, "InkpodAnnotationObjectInput") }?;
    // SAFETY: Record was validated above.
    let input = unsafe { &*input };
    if input.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "annotation input feature flags are unsupported",
        ));
    }
    // SAFETY: Nested strings and point span are part of the borrowed input contract.
    let font_family_hint = unsafe {
        utf8_value(
            input.font_family_utf8,
            input.font_family_bytes,
            inkpod_core::MAX_ANNOTATION_FONT_FAMILY_BYTES,
            "annotation font family is not valid UTF-8",
        )
    }?;
    // SAFETY: Same borrowed input contract as above.
    let text = unsafe {
        utf8_value(
            input.text_utf8,
            input.text_bytes,
            inkpod_core::MAX_ANNOTATION_TEXT_BYTES,
            "annotation text is not valid UTF-8",
        )
    }?;
    // SAFETY: Same borrowed input contract as above.
    let points =
        unsafe { annotation_points(input.points, input.point_count, input.point_stride_bytes) }?;
    // SAFETY: Nested color is a complete field of the validated record.
    let color = unsafe { parse_color_value(&raw const input.color) }?;
    Ok(AnnotationObjectInput {
        layer_id: input.layer_id,
        kind: annotation_kind(input.kind)?,
        output: annotation_output(input.output)?,
        bounds: RectI32 {
            x: input.bounds.x,
            y: input.bounds.y,
            width: input.bounds.width,
            height: input.bounds.height,
        },
        font_family_hint,
        font_size_milli: input.font_size_milli,
        style_flags: input.style_flags,
        color,
        text,
        points,
        stroke_width_milli: input.stroke_width_milli,
    })
}

fn validate_result_capacity(
    result: &mut InkpodAnnotationEditResult,
    required: usize,
) -> Result<(), u32> {
    result.created_count = required as u64;
    if result.feature_flags != INKPOD_FEATURE_NONE || result.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "annotation result contains unsupported input values",
        ));
    }
    if result.created_capacity == 0 {
        if !result.created_ids.is_null() {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "zero-capacity annotation result must use a null ID pointer",
            ));
        }
        if required != 0 {
            return Err(fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "annotation result ID buffer is too small",
            ));
        }
        return Ok(());
    }
    if result.created_ids.is_null()
        || !is_aligned(result.created_ids)
        || result.created_capacity < required as u64
        || usize::try_from(result.created_capacity)
            .ok()
            .and_then(|capacity| capacity.checked_mul(size_of::<u64>()))
            .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            if result.created_capacity < required as u64 {
                INKPOD_STATUS_BUFFER_TOO_SMALL
            } else {
                INKPOD_STATUS_INVALID_ARGUMENT
            },
            "annotation result ID storage is invalid or too small",
        ));
    }
    Ok(())
}

fn write_annotation_result(
    result: &mut InkpodAnnotationEditResult,
    outcome: &inkpod_core::AnnotationEditOutcome,
) {
    result.feature_flags = INKPOD_FEATURE_NONE;
    result.reserved = 0;
    result.revision = outcome.revision();
    result.created_count = outcome.created_object_ids().len() as u64;
    for (index, id) in outcome.created_object_ids().iter().enumerate() {
        // SAFETY: Capacity and aligned writable storage were validated before commit.
        unsafe { result.created_ids.add(index).write(*id) };
    }
}

/// Applies one bounded strided annotation edit list as a single Core transaction.
///
/// # Safety
/// Core/result and all borrowed edit, object, UTF-8, color, and point records
/// must be complete, aligned, readable/writable as appropriate, and live until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_annotation_edit(
    core: *mut InkpodCore,
    expected_revision: u64,
    edits: *const InkpodAnnotationEdit,
    edit_count: u64,
    edit_stride_bytes: u64,
    result: *mut InkpodAnnotationEditResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(result.cast_const(), "InkpodAnnotationEditResult") }
        {
            return status;
        }
        let count = match usize::try_from(edit_count) {
            Ok(count) if (1..=inkpod_core::MAX_ANNOTATION_BATCH_EDITS).contains(&count) => count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "annotation edit count is outside bounds",
                );
            }
        };
        let stride = match usize::try_from(edit_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodAnnotationEdit>()
                    && stride % align_of::<InkpodAnnotationEdit>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "annotation edit stride is invalid",
                );
            }
        };
        if edits.is_null()
            || !is_aligned(edits)
            || count
                .checked_sub(1)
                .and_then(|last| last.checked_mul(stride))
                .and_then(|offset| offset.checked_add(size_of::<InkpodAnnotationEdit>()))
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "annotation edit span is invalid",
            );
        }
        // SAFETY: Result record was validated and is writable by contract.
        let result = unsafe { &mut *result };
        let mut parsed = Vec::with_capacity(count);
        let mut create_count = 0_usize;
        for index in 0..count {
            // SAFETY: Validated bounded strided span covers this record.
            let record = unsafe {
                edits
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodAnnotationEdit>()
            };
            if let Err(status) = unsafe { validate_struct(record, "InkpodAnnotationEdit") } {
                return status;
            }
            // SAFETY: Complete record validated above.
            let record = unsafe { &*record };
            if record.feature_flags != INKPOD_FEATURE_NONE {
                return fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "annotation edit feature flags are unsupported",
                );
            }
            let edit = match record.kind {
                INKPOD_ANNOTATION_EDIT_CREATE => {
                    if record.object_id != 0 || record.delta_x != 0 || record.delta_y != 0 {
                        return fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "annotation create control fields are invalid",
                        );
                    }
                    create_count += 1;
                    match unsafe { parse_annotation_input(record.input) } {
                        Ok(input) => AnnotationEdit::Create(input),
                        Err(status) => return status,
                    }
                }
                INKPOD_ANNOTATION_EDIT_UPDATE => {
                    if record.object_id == 0 || record.delta_x != 0 || record.delta_y != 0 {
                        return fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "annotation update control fields are invalid",
                        );
                    }
                    match unsafe { parse_annotation_input(record.input) } {
                        Ok(input) => AnnotationEdit::Update {
                            object_id: record.object_id,
                            input,
                        },
                        Err(status) => return status,
                    }
                }
                INKPOD_ANNOTATION_EDIT_MOVE => {
                    if record.object_id == 0 || !record.input.is_null() {
                        return fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "annotation move control fields are invalid",
                        );
                    }
                    AnnotationEdit::Move {
                        object_id: record.object_id,
                        delta_x: record.delta_x,
                        delta_y: record.delta_y,
                    }
                }
                INKPOD_ANNOTATION_EDIT_DELETE => {
                    if record.object_id == 0
                        || !record.input.is_null()
                        || record.delta_x != 0
                        || record.delta_y != 0
                    {
                        return fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "annotation delete control fields are invalid",
                        );
                    }
                    AnnotationEdit::Delete {
                        object_id: record.object_id,
                    }
                }
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "annotation edit kind is unknown",
                    );
                }
            };
            parsed.push(edit);
        }
        if let Err(status) = validate_result_capacity(result, create_count) {
            return status;
        }
        // SAFETY: Live aligned Core pointer was checked above.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.edit_annotations(expected_revision, &parsed) {
            Ok(outcome) => {
                write_annotation_result(result, &outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Begins a transient annotation stroke.
///
/// # Safety
/// Core and input must be live, complete, aligned owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_annotation_stroke_begin(
    core: *mut InkpodCore,
    input: *const InkpodAnnotationStrokeInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodAnnotationStrokeInput") } {
            return status;
        }
        // SAFETY: Complete record validated above.
        let input = unsafe { &*input };
        if input.feature_flags != INKPOD_FEATURE_NONE
            || input.reserved != 0
            || input.start.struct_size as usize != size_of::<InkpodAnnotationPoint>()
            || input.start.reserved != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "annotation stroke input contains unsupported fields",
            );
        }
        let color = match unsafe { parse_color_value(&raw const input.color) } {
            Ok(color) => color,
            Err(status) => return status,
        };
        let output = match annotation_output(input.output) {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: Live aligned Core pointer was checked above.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.begin_annotation_stroke(
            input.base_document_revision,
            input.layer_id,
            output,
            color,
            input.stroke_width_milli,
            AnnotationPoint {
                x_milli: input.start.x_milli,
                y_milli: input.start.y_milli,
            },
        ) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Appends one borrowed strided point batch to the active annotation stroke.
///
/// # Safety
/// Core and the complete point span must remain live and readable until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_annotation_stroke_append(
    core: *mut InkpodCore,
    points: *const InkpodAnnotationPoint,
    point_count: u64,
    point_stride_bytes: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        let points = match unsafe { annotation_points(points, point_count, point_stride_bytes) } {
            Ok(points) if !points.is_empty() => points,
            Ok(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "annotation stroke point batch is empty",
                );
            }
            Err(status) => return status,
        };
        // SAFETY: Live aligned Core pointer was checked above.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.append_annotation_stroke(&points) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Ends and commits the active annotation stroke.
///
/// # Safety
/// Core and result must be live, complete, aligned owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_annotation_stroke_end(
    core: *mut InkpodCore,
    result: *mut InkpodAnnotationEditResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(result.cast_const(), "InkpodAnnotationEditResult") }
        {
            return status;
        }
        // SAFETY: Complete writable result was validated above.
        let result = unsafe { &mut *result };
        if let Err(status) = validate_result_capacity(result, 1) {
            return status;
        }
        // SAFETY: Live aligned Core pointer was checked above.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.end_annotation_stroke() {
            Ok(outcome) => {
                write_annotation_result(result, &outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Cancels the active annotation stroke without committing.
///
/// # Safety
/// Core must be a live aligned owner-thread object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_annotation_stroke_cancel(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live aligned Core pointer was checked above.
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.cancel_annotation_stroke() {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Borrows immutable annotation records and their UTF-8/geometry pools.
///
/// # Safety
/// Snapshot/output must be live, complete, aligned, externally synchronized,
/// and non-overlapping. Borrowed spans expire when the snapshot is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_annotations(
    snapshot: *const InkpodSnapshot,
    out_view: *mut InkpodSnapshotAnnotationView,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_view.cast_const(), "InkpodSnapshotAnnotationView") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *out_view };
        output.abi_version = INKPOD_ABI_VERSION;
        output.feature_flags = INKPOD_FEATURE_NONE;
        output.objects = if snapshot.annotations.is_empty() {
            ptr::null()
        } else {
            snapshot.annotations.as_ptr()
        };
        output.object_count = snapshot.annotations.len() as u64;
        output.object_stride_bytes = size_of::<InkpodSnapshotAnnotation>() as u64;
        output.utf8_bytes = if snapshot.annotation_utf8.is_empty() {
            ptr::null()
        } else {
            snapshot.annotation_utf8.as_ptr()
        };
        output.utf8_byte_count = snapshot.annotation_utf8.len() as u64;
        output.points = if snapshot.annotation_points.is_empty() {
            ptr::null()
        } else {
            snapshot.annotation_points.as_ptr()
        };
        output.point_count = snapshot.annotation_points.len() as u64;
        output.point_stride_bytes = size_of::<InkpodAnnotationPoint>() as u64;
        INKPOD_STATUS_OK
    })
}

unsafe fn copy_snapshot_utf8(
    snapshot: *const InkpodSnapshot,
    object_index: u64,
    font: bool,
    buffer: *mut u8,
    capacity: u64,
    out_required: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null()
            || !is_aligned(snapshot)
            || out_required.is_null()
            || !is_aligned(out_required)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "annotation UTF-8 copy pointer is null or misaligned",
            );
        }
        // SAFETY: Writable output was validated above.
        unsafe { out_required.write(0) };
        // SAFETY: Live snapshot was validated above.
        let snapshot = unsafe { &*snapshot };
        let Some(object) = snapshot.annotations.get(object_index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "annotation object index is outside bounds",
            );
        };
        let (offset, length) = if font {
            (object.font_utf8_offset, object.font_utf8_bytes)
        } else {
            (object.text_utf8_offset, object.text_utf8_bytes)
        };
        // SAFETY: Writable output was validated above.
        unsafe { out_required.write(length) };
        if capacity == 0 {
            return if buffer.is_null() {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity annotation UTF-8 buffer must be null",
                )
            };
        }
        if buffer.is_null() || capacity < length || capacity > isize::MAX as u64 {
            return fail(
                if capacity < length {
                    INKPOD_STATUS_BUFFER_TOO_SMALL
                } else {
                    INKPOD_STATUS_INVALID_ARGUMENT
                },
                "annotation UTF-8 buffer is invalid or too small",
            );
        }
        let start = offset as usize;
        let end = start.saturating_add(length as usize);
        let Some(bytes) = snapshot.annotation_utf8.get(start..end) else {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "snapshot annotation UTF-8 range is invalid",
            );
        };
        // SAFETY: Caller advertises enough writable capacity and source is immutable.
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len()) };
        INKPOD_STATUS_OK
    })
}

/// Two-stage copy of one snapshot annotation's font-family UTF-8 bytes.
///
/// # Safety
/// Snapshot and outputs follow the same lifetime/alignment rules as the view API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_annotation_copy_font_family(
    snapshot: *const InkpodSnapshot,
    object_index: u64,
    buffer: *mut u8,
    capacity: u64,
    out_required: *mut u64,
) -> u32 {
    // SAFETY: Forwarded unchanged to the checked shared implementation.
    unsafe { copy_snapshot_utf8(snapshot, object_index, true, buffer, capacity, out_required) }
}

/// Two-stage copy of one snapshot annotation's text UTF-8 bytes.
///
/// # Safety
/// Snapshot and outputs follow the same lifetime/alignment rules as the view API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_annotation_copy_text(
    snapshot: *const InkpodSnapshot,
    object_index: u64,
    buffer: *mut u8,
    capacity: u64,
    out_required: *mut u64,
) -> u32 {
    // SAFETY: Forwarded unchanged to the checked shared implementation.
    unsafe {
        copy_snapshot_utf8(
            snapshot,
            object_index,
            false,
            buffer,
            capacity,
            out_required,
        )
    }
}
