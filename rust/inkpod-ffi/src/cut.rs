use super::*;

fn validate_cut_thread(cut: &InkpodCut) -> u32 {
    if cut.owner_thread == thread::current().id() {
        INKPOD_STATUS_OK
    } else {
        fail(
            INKPOD_STATUS_WRONG_THREAD,
            "InkpodCut must be used and destroyed on its creating thread",
        )
    }
}

unsafe fn utf8_text(span: InkpodUtf8Span, field: &str) -> Result<String, u32> {
    if span.byte_count > inkpod_core::MAX_CUT_TEXT_BYTES as u64 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} exceeds the bounded UTF-8 length"),
        ));
    }
    if span.byte_count == 0 {
        return Ok(String::new());
    }
    if span.bytes.is_null() {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} pointer is null"),
        ));
    }
    let length = usize::try_from(span.byte_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} length is not representable"),
        )
    })?;
    // SAFETY: The exported-function contract requires this complete span to be readable.
    let bytes = unsafe { slice::from_raw_parts(span.bytes, length) };
    let value = std::str::from_utf8(bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} is not valid UTF-8"),
        )
    })?;
    if bytes.contains(&0) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} contains an embedded NUL"),
        ));
    }
    Ok(value.to_owned())
}

unsafe fn parse_metadata(input: *const InkpodCutMetadataInput) -> Result<CutMetadata, u32> {
    // SAFETY: The caller supplies a readable size-prefixed record.
    unsafe { validate_struct(input, "InkpodCutMetadataInput")? };
    // SAFETY: The complete record was validated above.
    let input = unsafe { &*input };
    Ok(CutMetadata {
        // SAFETY: Each advertised input span is readable for this call.
        work_title: unsafe { utf8_text(input.work_title, "Cut work title")? },
        // SAFETY: Each advertised input span is readable for this call.
        episode: unsafe { utf8_text(input.episode, "Cut episode")? },
        // SAFETY: Each advertised input span is readable for this call.
        scene: unsafe { utf8_text(input.scene, "Cut scene")? },
        // SAFETY: Each advertised input span is readable for this call.
        cut_name: unsafe { utf8_text(input.cut_name, "Cut name")? },
        // SAFETY: Each advertised input span is readable for this call.
        instruction: unsafe { utf8_text(input.instruction, "Cut instruction")? },
        duration_frames: input.duration_frames,
    })
}

fn parse_defaults(input: &InkpodCutDefaultsInput) -> Result<CutDefaults, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "Cut defaults contain unsupported flags or reserved values",
        ));
    }
    let sizing = match input.sizing_mode {
        INKPOD_CELL_SIZING_IMAGE_PIXELS => CellSizing::ImagePixels {
            width: input.width,
            height: input.height,
        },
        INKPOD_CELL_SIZING_FRAME_MICROMETRES => CellSizing::FrameMicrometres {
            width: input.width,
            height: input.height,
        },
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Cut default sizing mode is not defined",
            ));
        }
    };
    let anchor = match input.anchor {
        INKPOD_FRAME_ANCHOR_TOP_LEFT => FrameAnchor::TopLeft,
        INKPOD_FRAME_ANCHOR_TOP_RIGHT => FrameAnchor::TopRight,
        INKPOD_FRAME_ANCHOR_CENTER => FrameAnchor::Center,
        INKPOD_FRAME_ANCHOR_BOTTOM_LEFT => FrameAnchor::BottomLeft,
        INKPOD_FRAME_ANCHOR_BOTTOM_RIGHT => FrameAnchor::BottomRight,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Cut default frame anchor is not defined",
            ));
        }
    };
    Ok(CutDefaults {
        sizing,
        dpi_x_milli: input.dpi_x_milli,
        dpi_y_milli: input.dpi_y_milli,
        margin_milli: input.margin_milli,
        safe_frame_ratio_milli: input.safe_frame_ratio_milli,
        maximum_close_ratio_milli: input.maximum_close_ratio_milli,
        anchor,
        initial_layer_kind: parse_layer_kind(input.initial_layer_kind)?,
        pixel_format: parse_storage_format(input.pixel_format)?,
    })
}

unsafe fn parse_defaults_ptr(input: *const InkpodCutDefaultsInput) -> Result<CutDefaults, u32> {
    // SAFETY: The caller supplies a readable size-prefixed record.
    unsafe { validate_struct(input, "InkpodCutDefaultsInput")? };
    // SAFETY: The complete record was validated above.
    parse_defaults(unsafe { &*input })
}

unsafe fn parse_members(request: &InkpodCutCreateRequest) -> Result<Vec<CutMember>, u32> {
    if request.member_count > inkpod_core::MAX_CUT_MEMBERS as u64 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Cut member count exceeds the public bound",
        ));
    }
    if request.member_count == 0 {
        return Ok(Vec::new());
    }
    if request.members.is_null()
        || request.member_stride_bytes < size_of::<InkpodCutMemberInput>() as u64
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Cut member span is null or has a short stride",
        ));
    }
    let mut members = Vec::with_capacity(request.member_count as usize);
    for index in 0..request.member_count {
        let offset = index
            .checked_mul(request.member_stride_bytes)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "Cut member span offset overflows",
                )
            })?;
        // SAFETY: The caller contract provides member_count records at the advertised stride.
        let pointer =
            unsafe { request.members.cast::<u8>().add(offset) }.cast::<InkpodCutMemberInput>();
        // SAFETY: The current strided record exposes its complete size-prefixed range.
        unsafe { validate_struct(pointer, "InkpodCutMemberInput")? };
        // SAFETY: The complete strided record was validated above.
        let member = unsafe { &*pointer };
        // SAFETY: The relative-path span is readable for this call.
        let relative_path = unsafe { utf8_text(member.relative_path, "Cut member path")? };
        members.push(CutMember {
            cell_id: member.cell_id,
            document_uuid: (u128::from(member.document_uuid_high) << 64)
                | u128::from(member.document_uuid_low),
            display_number: member.display_number,
            relative_path,
        });
    }
    Ok(members)
}

fn anchor_code(anchor: FrameAnchor) -> u32 {
    match anchor {
        FrameAnchor::TopLeft => INKPOD_FRAME_ANCHOR_TOP_LEFT,
        FrameAnchor::TopRight => INKPOD_FRAME_ANCHOR_TOP_RIGHT,
        FrameAnchor::Center => INKPOD_FRAME_ANCHOR_CENTER,
        FrameAnchor::BottomLeft => INKPOD_FRAME_ANCHOR_BOTTOM_LEFT,
        FrameAnchor::BottomRight => INKPOD_FRAME_ANCHOR_BOTTOM_RIGHT,
    }
}

fn write_cut_info(output: &mut InkpodCutInfo, info: inkpod_core::CutInfo) {
    output.flags = (u32::from(info.dirty) * INKPOD_CUT_FLAG_DIRTY)
        | (u32::from(info.can_undo) * INKPOD_CUT_FLAG_CAN_UNDO)
        | (u32::from(info.can_redo) * INKPOD_CUT_FLAG_CAN_REDO)
        | (u32::from(info.recovered) * INKPOD_CUT_FLAG_RECOVERED);
    output.cut_id = info.cut_id;
    output.cut_uuid_high = (info.cut_uuid >> 64) as u64;
    output.cut_uuid_low = info.cut_uuid as u64;
    output.revision = info.revision;
    output.state_id = info.state_id;
    output.member_count = info.member_count;
    output.reserved = 0;
    output.work_title_bytes = info.metadata.work_title.len() as u64;
    output.episode_bytes = info.metadata.episode.len() as u64;
    output.scene_bytes = info.metadata.scene.len() as u64;
    output.cut_name_bytes = info.metadata.cut_name.len() as u64;
    output.instruction_bytes = info.metadata.instruction.len() as u64;
    output.duration_frames = info.metadata.duration_frames;
    let (sizing_mode, width, height) = match info.defaults.sizing {
        CellSizing::ImagePixels { width, height } => {
            (INKPOD_CELL_SIZING_IMAGE_PIXELS, width, height)
        }
        CellSizing::FrameMicrometres { width, height } => {
            (INKPOD_CELL_SIZING_FRAME_MICROMETRES, width, height)
        }
    };
    output.sizing_mode = sizing_mode;
    output.width = width;
    output.height = height;
    output.dpi_x_milli = info.defaults.dpi_x_milli;
    output.dpi_y_milli = info.defaults.dpi_y_milli;
    output.margin_milli = info.defaults.margin_milli;
    output.safe_frame_ratio_milli = info.defaults.safe_frame_ratio_milli;
    output.maximum_close_ratio_milli = info.defaults.maximum_close_ratio_milli;
    output.anchor = anchor_code(info.defaults.anchor);
    output.initial_layer_kind = layer_kind_code(info.defaults.initial_layer_kind);
    output.pixel_format = storage_format_code(info.defaults.pixel_format);
}

fn write_dispatch(output: &mut InkpodDispatchResult, revision: u64, outcome: CutMutationOutcome) {
    output.reserved = 0;
    output.revision = revision;
    output.accepted_command_count = u64::from(outcome == CutMutationOutcome::Applied);
}

fn buffer_has_capacity(buffer: &InkpodUtf8Buffer, required: usize) -> bool {
    required == 0 || (!buffer.bytes.is_null() && buffer.capacity >= required as u64)
}

unsafe fn copy_buffer(buffer: &mut InkpodUtf8Buffer, value: &str) {
    buffer.byte_count = value.len() as u64;
    if !value.is_empty() {
        // SAFETY: Capacity and writable storage were validated before this helper is called.
        unsafe { ptr::copy_nonoverlapping(value.as_ptr(), buffer.bytes, value.len()) };
    }
}

/// Creates a Rust-owned Cut state machine. Cell files remain independently owned.
///
/// # Safety
/// All size-prefixed input records and advertised UTF-8/member spans must remain
/// readable for this call. `out_cut` is writable owner storage and receives null
/// on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_create(
    request: *const InkpodCutCreateRequest,
    out_cut: *mut *mut InkpodCut,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_cut.is_null() || !is_aligned(out_cut) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Cut owner output is invalid",
            );
        }
        // SAFETY: The caller supplies writable owner storage.
        unsafe { out_cut.write(ptr::null_mut()) };
        // SAFETY: The request exposes a readable size prefix and full advertised record.
        if let Err(status) = unsafe { validate_struct(request, "InkpodCutCreateRequest") } {
            return status;
        }
        // SAFETY: The complete request was validated above.
        let request = unsafe { &*request };
        if request.feature_flags != INKPOD_FEATURE_NONE || request.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "Cut create request contains unsupported flags",
            );
        }
        // SAFETY: Nested records and spans follow the exported caller contract.
        let metadata = match unsafe { parse_metadata(request.metadata) } {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: Nested records follow the exported caller contract.
        let defaults = match unsafe { parse_defaults_ptr(request.defaults) } {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: The strided member span follows the exported caller contract.
        let members = match unsafe { parse_members(request) } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let input = CutCreateRequest {
            cut_uuid: (u128::from(request.cut_uuid_high) << 64) | u128::from(request.cut_uuid_low),
            metadata,
            defaults,
            members,
        };
        match CutCore::new(input) {
            Ok(cut) => {
                let handle = Box::new(InkpodCut {
                    owner_thread: thread::current().id(),
                    cut,
                });
                // SAFETY: The validated output now receives the unique Box owner.
                unsafe { out_cut.write(Box::into_raw(handle)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

unsafe fn open_cut(
    path_utf8: *const u8,
    path_bytes: u64,
    out_cut: *mut *mut InkpodCut,
    recovery: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_cut.is_null() || !is_aligned(out_cut) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Cut owner output is invalid",
            );
        }
        // SAFETY: The caller supplies writable owner storage.
        unsafe { out_cut.write(ptr::null_mut()) };
        // SAFETY: The advertised path range remains readable for this call.
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        let operation = if recovery {
            CutCore::open_recovery(path)
        } else {
            CutCore::open(path)
        };
        match operation {
            Ok(cut) => {
                let handle = Box::new(InkpodCut {
                    owner_thread: thread::current().id(),
                    cut,
                });
                // SAFETY: The validated output now receives the unique Box owner.
                unsafe { out_cut.write(Box::into_raw(handle)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Opens a Cut descriptor and validates all same-directory Cell references.
///
/// # Safety
/// `path_utf8` must name `path_bytes` readable bytes. `out_cut` must be writable
/// owner storage and receives null on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_open(
    path_utf8: *const u8,
    path_bytes: u64,
    out_cut: *mut *mut InkpodCut,
) -> u32 {
    // SAFETY: This function forwards the identical exported caller contract.
    unsafe { open_cut(path_utf8, path_bytes, out_cut, false) }
}

/// Opens recovery data as a dirty Cut without adopting a normal savepoint.
///
/// # Safety
/// `path_utf8` must name `path_bytes` readable bytes. `out_cut` must be writable
/// owner storage and receives null on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_open_recovery(
    path_utf8: *const u8,
    path_bytes: u64,
    out_cut: *mut *mut InkpodCut,
) -> u32 {
    // SAFETY: This function forwards the identical exported caller contract.
    unsafe { open_cut(path_utf8, path_bytes, out_cut, true) }
}

/// Destroys one Cut on its owner thread and nulls caller owner storage.
///
/// # Safety
/// `cut` must be writable owner storage containing null or the unique live
/// pointer returned by a Cut create/open function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_destroy(cut: *mut *mut InkpodCut) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if cut.is_null() || !is_aligned(cut) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Cut owner pointer is invalid",
            );
        }
        // SAFETY: The caller supplies readable and writable owner storage.
        let handle = unsafe { cut.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "Cut handle is misaligned");
        }
        // SAFETY: The caller contract supplies a live uniquely owned handle.
        let cut_ref = unsafe { &*handle };
        let thread_status = validate_cut_thread(cut_ref);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: Nulling first makes repetition through this owner variable harmless.
        unsafe { cut.write(ptr::null_mut()) };
        // SAFETY: Ownership originated from Box::into_raw and is consumed once.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Queries immutable Cut identity, revisions, flags, defaults, and text lengths.
///
/// # Safety
/// `cut` must be a live owner-thread handle and `out_info` must expose a complete
/// writable size-prefixed record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_info(
    cut: *const InkpodCut,
    out_info: *mut InkpodCutInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if cut.is_null() || !is_aligned(cut) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "Cut handle is invalid");
        }
        // SAFETY: The output exposes its readable size prefix and full writable record.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodCutInfo") } {
            return status;
        }
        // SAFETY: Live handle and complete output are guaranteed by caller contract.
        let cut = unsafe { &*cut };
        let status = validate_cut_thread(cut);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        // SAFETY: The complete validated output is writable.
        write_cut_info(unsafe { &mut *out_info }, cut.cut.info());
        INKPOD_STATUS_OK
    })
}

/// Copies current metadata to caller buffers. Required byte counts are written
/// before returning `INKPOD_STATUS_BUFFER_TOO_SMALL`; no partial text is copied.
///
/// # Safety
/// `cut` must be a live owner-thread handle. `output` and each nonempty advertised
/// destination range must remain writable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_metadata_copy(
    cut: *const InkpodCut,
    output: *mut InkpodCutMetadataBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if cut.is_null() || !is_aligned(cut) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "Cut handle is invalid");
        }
        // SAFETY: The output exposes its readable size prefix and full writable record.
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodCutMetadataBuffer") }
        {
            return status;
        }
        // SAFETY: The complete handle and output are live by contract.
        let cut = unsafe { &*cut };
        let status = validate_cut_thread(cut);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let metadata = cut.cut.info().metadata;
        // SAFETY: The complete validated output record is writable.
        let output = unsafe { &mut *output };
        output.duration_frames = metadata.duration_frames;
        output.work_title.byte_count = metadata.work_title.len() as u64;
        output.episode.byte_count = metadata.episode.len() as u64;
        output.scene.byte_count = metadata.scene.len() as u64;
        output.cut_name.byte_count = metadata.cut_name.len() as u64;
        output.instruction.byte_count = metadata.instruction.len() as u64;
        if !buffer_has_capacity(&output.work_title, metadata.work_title.len())
            || !buffer_has_capacity(&output.episode, metadata.episode.len())
            || !buffer_has_capacity(&output.scene, metadata.scene.len())
            || !buffer_has_capacity(&output.cut_name, metadata.cut_name.len())
            || !buffer_has_capacity(&output.instruction, metadata.instruction.len())
        {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "one or more Cut metadata buffers are too small",
            );
        }
        // SAFETY: Every destination has enough writable capacity.
        unsafe {
            copy_buffer(&mut output.work_title, &metadata.work_title);
            copy_buffer(&mut output.episode, &metadata.episode);
            copy_buffer(&mut output.scene, &metadata.scene);
            copy_buffer(&mut output.cut_name, &metadata.cut_name);
            copy_buffer(&mut output.instruction, &metadata.instruction);
        }
        INKPOD_STATUS_OK
    })
}

/// Queries one ordered Cut member and copies its relative UTF-8 path.
///
/// # Safety
/// `cut` must be a live owner-thread handle. `output` and its nonempty advertised
/// path range must remain writable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_member_get(
    cut: *const InkpodCut,
    index: u32,
    output: *mut InkpodCutMemberInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if cut.is_null() || !is_aligned(cut) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "Cut handle is invalid");
        }
        // SAFETY: The output exposes its readable size prefix and full writable record.
        if let Err(status) = unsafe { validate_struct(output.cast_const(), "InkpodCutMemberInfo") }
        {
            return status;
        }
        // SAFETY: The complete handle and output are live by contract.
        let cut = unsafe { &*cut };
        let status = validate_cut_thread(cut);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let Some(member) = cut.cut.members().get(index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Cut member index is out of range",
            );
        };
        // SAFETY: The complete validated output record is writable.
        let output = unsafe { &mut *output };
        output.display_number = member.display_number;
        output.cell_id = member.cell_id;
        output.document_uuid_high = (member.document_uuid >> 64) as u64;
        output.document_uuid_low = member.document_uuid as u64;
        output.relative_path.byte_count = member.relative_path.len() as u64;
        if !buffer_has_capacity(&output.relative_path, member.relative_path.len()) {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "Cut member path buffer is too small",
            );
        }
        // SAFETY: The destination has enough writable capacity.
        unsafe { copy_buffer(&mut output.relative_path, &member.relative_path) };
        INKPOD_STATUS_OK
    })
}

fn sequence_identity(
    cell_id: u64,
    document_uuid_high: u64,
    document_uuid_low: u64,
) -> Result<SequenceMemberId, u32> {
    SequenceMemberId::new(
        cell_id,
        (u128::from(document_uuid_high) << 64) | u128::from(document_uuid_low),
    )
    .map_err(map_core_error)
}

unsafe fn parse_sequence_operations(
    request: &InkpodCutSequenceEditRequest,
    result: &mut InkpodCutSequenceEditResult,
) -> Result<Vec<SequenceEditOperation>, u32> {
    if request.operation_count > inkpod_core::MAX_SEQUENCE_EDIT_OPERATIONS as u64 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Cut sequence operation count exceeds the public bound",
        ));
    }
    result.operation_count = request.operation_count as u32;
    if request.operation_count == 0 {
        return Ok(Vec::new());
    }
    if request.operations.is_null()
        || request.operation_stride_bytes < size_of::<InkpodCutSequenceEditOperation>() as u64
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Cut sequence operation span is null or has a short stride",
        ));
    }
    let mut operations = Vec::with_capacity(request.operation_count as usize);
    for index in 0..request.operation_count {
        result.failed_operation_index = index as u32;
        let offset = index
            .checked_mul(request.operation_stride_bytes)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "Cut sequence operation span offset overflows",
                )
            })?;
        // SAFETY: The caller contract exposes operation_count records at this stride.
        let pointer = unsafe { request.operations.cast::<u8>().add(offset) }
            .cast::<InkpodCutSequenceEditOperation>();
        // SAFETY: The current record exposes its complete readable size-prefixed range.
        unsafe { validate_struct(pointer, "InkpodCutSequenceEditOperation")? };
        // SAFETY: The complete record was validated above.
        let operation = unsafe { &*pointer };
        if operation.feature_flags != INKPOD_FEATURE_NONE || operation.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "Cut sequence operation contains unsupported flags",
            ));
        }
        let parsed = match operation.kind {
            INKPOD_CUT_SEQUENCE_INSERT => {
                let member = sequence_identity(
                    operation.cell_id,
                    operation.document_uuid_high,
                    operation.document_uuid_low,
                )?;
                // SAFETY: The operation path span is readable for this call.
                let relative_path =
                    unsafe { utf8_text(operation.relative_path, "Cut sequence member path")? };
                SequenceEditOperation::Insert {
                    position: operation.position,
                    member: CutMember {
                        cell_id: member.cell_id(),
                        document_uuid: member.document_uuid(),
                        display_number: operation.display_number,
                        relative_path,
                    },
                }
            }
            INKPOD_CUT_SEQUENCE_REMOVE => SequenceEditOperation::Remove {
                member: sequence_identity(
                    operation.cell_id,
                    operation.document_uuid_high,
                    operation.document_uuid_low,
                )?,
            },
            INKPOD_CUT_SEQUENCE_MOVE_BEFORE | INKPOD_CUT_SEQUENCE_MOVE_AFTER => {
                let member = sequence_identity(
                    operation.cell_id,
                    operation.document_uuid_high,
                    operation.document_uuid_low,
                )?;
                let anchor = sequence_identity(
                    operation.anchor_cell_id,
                    operation.anchor_document_uuid_high,
                    operation.anchor_document_uuid_low,
                )?;
                if operation.kind == INKPOD_CUT_SEQUENCE_MOVE_BEFORE {
                    SequenceEditOperation::MoveBefore { member, anchor }
                } else {
                    SequenceEditOperation::MoveAfter { member, anchor }
                }
            }
            INKPOD_CUT_SEQUENCE_RENUMBER_RANGE => SequenceEditOperation::RenumberRange {
                start: operation.position,
                count: operation.count,
                first_number: operation.first_number,
                step: operation.step,
            },
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "Cut sequence operation kind is not defined",
                ));
            }
        };
        operations.push(parsed);
    }
    result.failed_operation_index = INKPOD_CUT_SEQUENCE_REQUEST_ERROR_INDEX;
    Ok(operations)
}

fn write_sequence_result(
    output: &mut InkpodCutSequenceEditResult,
    info: inkpod_core::CutInfo,
    operation_count: u32,
    outcome: CutMutationOutcome,
) {
    output.flags =
        u32::from(outcome == CutMutationOutcome::Applied) * INKPOD_CUT_SEQUENCE_EDIT_APPLIED;
    output.revision = info.revision;
    output.state_id = info.state_id;
    output.member_count = info.member_count;
    output.operation_count = operation_count;
    output.failed_operation_index = INKPOD_CUT_SEQUENCE_REQUEST_ERROR_INDEX;
    output.reserved = 0;
}

/// Commits one bounded ordered membership span as one Cut history transaction.
///
/// On an operation-local failure, `failed_operation_index` names the zero-based
/// record that failed. Request-level and final-state failures report
/// `INKPOD_CUT_SEQUENCE_REQUEST_ERROR_INDEX`. No operation pointer or path is
/// retained after this call.
///
/// # Safety
/// `cut` must be a live owner-thread handle. `request`, every advertised strided
/// operation/path range, and the complete writable `result` must remain valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_sequence_edit(
    cut: *mut InkpodCut,
    request: *const InkpodCutSequenceEditRequest,
    result: *mut InkpodCutSequenceEditResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if cut.is_null() || !is_aligned(cut) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "Cut handle is invalid");
        }
        // SAFETY: Public records expose complete readable size-prefixed ranges.
        if let Err(status) = unsafe { validate_struct(request, "InkpodCutSequenceEditRequest") } {
            return status;
        }
        // SAFETY: The result prefix is readable and the complete record writable.
        if let Err(status) =
            unsafe { validate_struct(result.cast_const(), "InkpodCutSequenceEditResult") }
        {
            return status;
        }
        // SAFETY: Complete validated records remain live for this call.
        let cut = unsafe { &mut *cut };
        let request = unsafe { &*request };
        // SAFETY: The validated result is writable.
        let result = unsafe { &mut *result };
        result.flags = 0;
        result.revision = cut.cut.info().revision;
        result.state_id = cut.cut.info().state_id;
        result.member_count = cut.cut.info().member_count;
        result.operation_count = 0;
        result.failed_operation_index = INKPOD_CUT_SEQUENCE_REQUEST_ERROR_INDEX;
        result.reserved = 0;
        let status = validate_cut_thread(cut);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if request.feature_flags != INKPOD_FEATURE_NONE || request.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "Cut sequence edit request contains unsupported flags",
            );
        }
        // SAFETY: The operation span and nested paths follow the exported contract.
        let operations = match unsafe { parse_sequence_operations(request, result) } {
            Ok(value) => value,
            Err(status) => return status,
        };
        match cut.cut.edit_sequence(SequenceEditRequest {
            base_revision: request.base_revision,
            operations,
        }) {
            Ok(outcome) => {
                write_sequence_result(
                    result,
                    cut.cut.info(),
                    request.operation_count as u32,
                    outcome,
                );
                INKPOD_STATUS_OK
            }
            Err(error) => {
                result.failed_operation_index = error.operation_index();
                map_core_error(error.into_error())
            }
        }
    })
}

/// Reports a cancelled interactive sequence edit as an observable stable no-op.
///
/// # Safety
/// `cut` must be a live owner-thread handle and `result` must expose a complete
/// writable size-prefixed record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_sequence_cancel(
    cut: *mut InkpodCut,
    result: *mut InkpodCutSequenceEditResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if cut.is_null() || !is_aligned(cut) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "Cut handle is invalid");
        }
        // SAFETY: The result prefix is readable and complete record writable.
        if let Err(status) =
            unsafe { validate_struct(result.cast_const(), "InkpodCutSequenceEditResult") }
        {
            return status;
        }
        // SAFETY: Live handle and complete result are guaranteed by contract.
        let cut = unsafe { &mut *cut };
        let status = validate_cut_thread(cut);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        // SAFETY: The complete validated result is writable.
        write_sequence_result(
            unsafe { &mut *result },
            cut.cut.info(),
            0,
            cut.cut.cancel_sequence_edit(),
        );
        INKPOD_STATUS_OK
    })
}

unsafe fn cut_file_operation(
    cut: *mut InkpodCut,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodCutInfo,
    recovery: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if cut.is_null() || !is_aligned(cut) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "Cut handle is invalid");
        }
        // SAFETY: The output exposes its readable size prefix and full writable record.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodCutInfo") } {
            return status;
        }
        // SAFETY: The advertised path span remains readable for this call.
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        // SAFETY: Live handle and complete output are guaranteed by caller contract.
        let cut = unsafe { &mut *cut };
        let status = validate_cut_thread(cut);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let operation = if recovery {
            cut.cut.autosave(path)
        } else {
            cut.cut.save(path)
        };
        match operation {
            Ok(info) => {
                // SAFETY: The complete validated output record is writable.
                write_cut_info(unsafe { &mut *out_info }, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Atomically saves the descriptor after validating all referenced Cell files.
///
/// # Safety
/// `cut` must be a live owner-thread handle, `path_utf8` must remain readable, and
/// `out_info` must expose a complete writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_save(
    cut: *mut InkpodCut,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodCutInfo,
) -> u32 {
    // SAFETY: This function forwards the identical exported caller contract.
    unsafe { cut_file_operation(cut, path_utf8, path_bytes, out_info, false) }
}

/// Writes Cut recovery data without advancing its normal savepoint.
///
/// # Safety
/// `cut` must be a live owner-thread handle, `path_utf8` must remain readable, and
/// `out_info` must expose a complete writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_autosave(
    cut: *mut InkpodCut,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodCutInfo,
) -> u32 {
    // SAFETY: This function forwards the identical exported caller contract.
    unsafe { cut_file_operation(cut, path_utf8, path_bytes, out_info, true) }
}

/// Commits one stale-bound Cut metadata/default procedure.
///
/// # Safety
/// `cut` must be a live owner-thread handle. `request`, its nested records/spans,
/// and the writable `result` record must remain valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_update(
    cut: *mut InkpodCut,
    request: *const InkpodCutUpdateRequest,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if cut.is_null() || !is_aligned(cut) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "Cut handle is invalid");
        }
        // SAFETY: Public records expose complete readable size-prefixed ranges.
        if let Err(status) = unsafe { validate_struct(request, "InkpodCutUpdateRequest") } {
            return status;
        }
        // SAFETY: The result prefix is readable and complete record writable.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete validated records remain live for this call.
        let cut = unsafe { &mut *cut };
        let request = unsafe { &*request };
        let status = validate_cut_thread(cut);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if request.feature_flags != INKPOD_FEATURE_NONE || request.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "Cut update request contains unsupported flags",
            );
        }
        // SAFETY: Nested records and spans follow the exported caller contract.
        let metadata = match unsafe { parse_metadata(request.metadata) } {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: Nested records follow the exported caller contract.
        let defaults = match unsafe { parse_defaults_ptr(request.defaults) } {
            Ok(value) => value,
            Err(status) => return status,
        };
        match cut.cut.update(CutUpdateRequest {
            base_revision: request.base_revision,
            metadata,
            defaults,
        }) {
            Ok(outcome) => {
                // SAFETY: The complete validated result is writable.
                write_dispatch(unsafe { &mut *result }, cut.cut.info().revision, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

unsafe fn cut_history_operation(
    cut: *mut InkpodCut,
    result: *mut InkpodDispatchResult,
    operation: u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if cut.is_null() || !is_aligned(cut) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "Cut handle is invalid");
        }
        // SAFETY: The result prefix is readable and complete record writable.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Live handle and complete result are guaranteed by contract.
        let cut = unsafe { &mut *cut };
        let status = validate_cut_thread(cut);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let outcome = match operation {
            0 => cut.cut.cancel_update(),
            1 => match cut.cut.undo() {
                Ok(value) => value,
                Err(error) => return map_core_error(error),
            },
            2 => match cut.cut.redo() {
                Ok(value) => value,
                Err(error) => return map_core_error(error),
            },
            _ => unreachable!(),
        };
        // SAFETY: The complete validated result is writable.
        write_dispatch(unsafe { &mut *result }, cut.cut.info().revision, outcome);
        INKPOD_STATUS_OK
    })
}

/// Reports a cancelled Cut dialog as a stable no-op.
///
/// # Safety
/// `cut` must be a live owner-thread handle and `result` must expose a complete
/// writable size-prefixed record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_cancel_update(
    cut: *mut InkpodCut,
    result: *mut InkpodDispatchResult,
) -> u32 {
    // SAFETY: This function forwards the identical exported caller contract.
    unsafe { cut_history_operation(cut, result, 0) }
}

/// Undoes one Cut-owned metadata/default history item.
///
/// # Safety
/// `cut` must be a live owner-thread handle and `result` must expose a complete
/// writable size-prefixed record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_undo(
    cut: *mut InkpodCut,
    result: *mut InkpodDispatchResult,
) -> u32 {
    // SAFETY: This function forwards the identical exported caller contract.
    unsafe { cut_history_operation(cut, result, 1) }
}

/// Redoes one Cut-owned metadata/default history item.
///
/// # Safety
/// `cut` must be a live owner-thread handle and `result` must expose a complete
/// writable size-prefixed record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cut_redo(
    cut: *mut InkpodCut,
    result: *mut InkpodDispatchResult,
) -> u32 {
    // SAFETY: This function forwards the identical exported caller contract.
    unsafe { cut_history_operation(cut, result, 2) }
}
