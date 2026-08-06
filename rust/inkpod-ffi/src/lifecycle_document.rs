use super::*;

#[unsafe(no_mangle)]
pub extern "C" fn inkpod_abi_version() -> u32 {
    INKPOD_ABI_VERSION
}

/// Creates a single-writer core handle.
///
/// # Safety
/// `config` must expose a readable size prefix and the byte range it advertises.
/// `out_core` must point to writable storage for one non-overlapping handle
/// pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_create(
    config: *const InkpodCoreConfig,
    out_core: *mut *mut InkpodCore,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_core.is_null() || !is_aligned(out_core) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_core is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires writable storage at out_core.
        unsafe { out_core.write(ptr::null_mut()) };

        // SAFETY: The exported API requires a readable public-structure prefix.
        if let Err(status) = unsafe { validate_struct(config, "InkpodCoreConfig") } {
            return status;
        }
        // SAFETY: The size prefix was validated and the caller contract makes
        // the complete configuration readable for this call.
        let config = unsafe { &*config };
        if config.abi_version != INKPOD_ABI_VERSION {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodCoreConfig.abi_version is unsupported",
            );
        }
        if config.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodCoreConfig contains unsupported feature flags",
            );
        }

        let objects = match crate::v3::ObjectRegistry::new() {
            Some(objects) => objects,
            None => {
                return fail(
                    INKPOD_STATUS_INVALID_STATE,
                    "ABI-v3 Core generation space is exhausted",
                );
            }
        };
        let handle = Box::new(InkpodCore {
            owner_thread: thread::current().id(),
            core: Core::new(),
            objects,
        });
        // SAFETY: out_core is writable by contract and now receives Box ownership.
        unsafe { out_core.write(Box::into_raw(handle)) };
        INKPOD_STATUS_OK
    })
}

/// Destroys a core and nulls the caller's pointer. Repeating the call with the
/// same pointer variable is a safe no-op.
///
/// # Safety
/// `core` must point to writable storage that contains either null or a handle
/// returned by `inkpod_core_create` and not already destroyed through an alias.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_destroy(core: *mut *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core owner pointer is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires readable/writable pointer storage.
        let handle = unsafe { core.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core handle is misaligned");
        }
        // SAFETY: The caller contract guarantees a live handle from core_create.
        let core_ref = unsafe { &*handle };
        let thread_status = validate_core_thread(core_ref);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // Null first so a repeated call using the same owner variable is harmless.
        // SAFETY: The outer pointer is writable by contract.
        unsafe { core.write(ptr::null_mut()) };
        // SAFETY: Ownership came from Box::into_raw and is consumed exactly once.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Dispatches a validated command batch on the creating thread.
///
/// # Safety
/// All pointers must follow the non-overlapping sizes, strides, lifetimes, and
/// ownership rules declared in `include/inkpod/core_ffi.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_dispatch_batch(
    core: *mut InkpodCore,
    batch: *const InkpodCommandBatch,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The exported API requires readable/writable structure prefixes.
        if let Err(status) = unsafe { validate_struct(batch, "InkpodCommandBatch") } {
            return status;
        }
        // SAFETY: The result prefix is readable before the validated output is written.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }

        // SAFETY: Complete structures were size-checked, and valid live,
        // non-overlapping objects and output storage are required by contract.
        let core = unsafe { &mut *core };
        let batch = unsafe { &*batch };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if batch.reserved != 0 || batch.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "batch contains unsupported flags or reserved values",
            );
        }
        if batch.command_count > MAX_COMMAND_COUNT {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch command_count exceeds the bounded command limit",
            );
        }
        if batch.commands.is_null() && batch.command_count != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "commands is null for a non-empty batch",
            );
        }

        let command_count = match usize::try_from(batch.command_count) {
            Ok(count) => count,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch command_count cannot be represented on this platform",
                );
            }
        };
        let command_stride = match usize::try_from(batch.command_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodCommand>()
                    && stride % align_of::<InkpodCommand>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "command_stride_bytes is too small, misaligned, or not representable",
                );
            }
        };
        if !batch.commands.is_null() && !is_aligned(batch.commands) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "commands is misaligned");
        }
        let command_storage_bytes = command_count
            .saturating_sub(1)
            .checked_mul(command_stride)
            .and_then(|last_offset| last_offset.checked_add(size_of::<InkpodCommand>()));
        if command_storage_bytes.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch command storage size overflows",
            );
        }

        let mut domain_commands = Vec::with_capacity(command_count);
        for index in 0..command_count {
            let offset = index * command_stride;
            // SAFETY: The checked count/stride range fits isize, and the caller
            // promises readable command storage for that complete range.
            let command_pointer = unsafe {
                batch
                    .commands
                    .cast::<u8>()
                    .add(offset)
                    .cast::<InkpodCommand>()
            };
            // SAFETY: The containing range and element alignment were checked,
            // and the caller promises readable records for the complete span.
            let command_struct_size =
                match unsafe { validate_struct(command_pointer, "InkpodCommand") } {
                    Ok(struct_size) => struct_size,
                    Err(status) => return status,
                };
            if u64::from(command_struct_size) > batch.command_stride_bytes {
                return fail(
                    INKPOD_STATUS_INCOMPATIBLE_ABI,
                    "InkpodCommand.struct_size exceeds command_stride_bytes",
                );
            }
            // SAFETY: The element size, alignment, and containing storage were
            // validated before constructing the known ABI prefix reference.
            let command = unsafe { &*command_pointer };
            if command.flags != 0 {
                return fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "InkpodCommand.flags contains unsupported bits",
                );
            }
            if command.kind != INKPOD_COMMAND_NO_OP {
                return fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "InkpodCommand.kind is not defined",
                );
            }
            domain_commands.push(Command::NoOp);
        }

        let outcome = core.core.dispatch(&domain_commands);
        result.reserved = 0;
        result.revision = outcome.revision();
        result.accepted_command_count = outcome.accepted_commands();
        INKPOD_STATUS_OK
    })
}

/// Creates a new two-plane cell document.
///
/// # Safety
/// Pointers must reference live, non-overlapping objects with readable/writable
/// ranges described by their size prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_new_cell(
    core: *mut InkpodCore,
    options: *const InkpodCellCreateOptions,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structure pointers expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(options, "InkpodCellCreateOptions") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects and writable output are required by contract.
        let core = unsafe { &mut *core };
        let options = unsafe { &*options };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "cell options contain unsupported flags or reserved values",
            );
        }
        let document_uuid =
            (u128::from(options.document_uuid_high) << 64) | u128::from(options.document_uuid_low);
        match core.core.new_cell_with_uuid(
            options.width,
            options.height,
            options.dpi_x_milli,
            options.dpi_y_milli,
            document_uuid,
        ) {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies current document metadata and checksums.
///
/// # Safety
/// `core` must be live on its owner thread and `out_info` must expose its
/// complete writable advertised range without overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_document_info(
    core: *mut InkpodCore,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Live core and writable output are required by contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.document_info() {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies deterministic logical resource usage for the current Core session.
///
/// # Safety
/// `core` must be live on its owner thread and `out_usage` must expose its
/// complete writable advertised range without overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_resource_usage(
    core: *mut InkpodCore,
    out_usage: *mut InkpodResourceUsage,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(out_usage.cast_const(), "InkpodResourceUsage") }
        {
            return status;
        }
        // SAFETY: Live core and writable output are required by contract.
        let core = unsafe { &mut *core };
        let out_usage = unsafe { &mut *out_usage };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        write_resource_usage(out_usage, core.core.resource_usage());
        INKPOD_STATUS_OK
    })
}

/// Transactionally updates the four production frames and independent margins.
///
/// # Safety
/// `core` must be live on its owner thread, `input` must be a complete readable
/// record, and `result` must be complete writable non-overlapping storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_update_paper_frames(
    core: *mut InkpodCore,
    input: *const InkpodPaperFramesInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Readable/writable complete records are required by contract.
        if let Err(status) = unsafe { validate_struct(input, "InkpodPaperFramesInput") } {
            return status;
        }
        // SAFETY: Readable/writable complete records are required by contract.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Validation proved the advertised known prefixes readable.
        let input = unsafe { &*input };
        if input.reserved != 0 || input.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "paper-frame input contains unsupported flags",
            );
        }
        let frame = |value: InkpodFrameRect| RectI32 {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        };
        let frames = FrameMetadata {
            hundred_frame: frame(input.hundred_frame),
            reference_frame: frame(input.reference_frame),
            drawing_frame: frame(input.drawing_frame),
            safe_frame: frame(input.safe_frame),
            margins: Margins {
                left: input.margin_left,
                top: input.margin_top,
                right: input.margin_right,
                bottom: input.margin_bottom,
            },
        };
        // SAFETY: Live owner-thread objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.update_paper_frames(frames) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Switches the editable plane without mutating document pixels or revision.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_active_plane(core: *mut InkpodCore, plane: u32) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A live core is required by the caller contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let plane = match parse_plane(plane) {
            Ok(plane) => plane,
            Err(status) => return status,
        };
        match core.core.set_active_plane(plane) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Selects a stable-ID layer/plane pair without changing document pixels.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_active_node(
    core: *mut InkpodCore,
    layer_id: u64,
    plane_id: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A live core is required by the caller contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.set_active_node(layer_id, plane_id) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}
