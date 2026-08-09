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
        if options.feature_flags & !INKPOD_CELL_CREATE_INITIAL_LAYER_KIND != 0
            || (options.feature_flags == INKPOD_FEATURE_NONE && options.reserved != 0)
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "cell options contain unsupported flags or reserved values",
            );
        }
        let initial_layer_kind =
            if options.feature_flags & INKPOD_CELL_CREATE_INITIAL_LAYER_KIND != 0 {
                match parse_layer_kind(options.reserved) {
                    Ok(kind) => kind,
                    Err(status) => return status,
                }
            } else {
                LayerKind::BinaryColoring
            };
        let document_uuid =
            (u128::from(options.document_uuid_high) << 64) | u128::from(options.document_uuid_low);
        match core.core.new_cell_with_uuid_and_layer(
            options.width,
            options.height,
            options.dpi_x_milli,
            options.dpi_y_milli,
            document_uuid,
            initial_layer_kind,
        ) {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Builds one Rust-owned immutable cell-creation plan without consuming Core IDs.
///
/// # Safety
/// `options` must expose a complete readable record and `out_plan` must point to
/// writable owner storage. On success the caller releases the handle with
/// [`inkpod_cell_creation_plan_release`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cell_creation_plan_create(
    options: *const InkpodCellCreationOptions,
    out_plan: *mut *mut InkpodCellCreationPlan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_plan.is_null() || !is_aligned(out_plan) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "cell plan owner pointer is null or misaligned",
            );
        }
        // SAFETY: The caller provides writable owner storage.
        unsafe { out_plan.write(ptr::null_mut()) };
        // SAFETY: The caller contract provides a readable size-prefixed input.
        if let Err(status) = unsafe { validate_struct(options, "InkpodCellCreationOptions") } {
            return status;
        }
        // SAFETY: The validated complete input remains readable for this call.
        let options = unsafe { &*options };
        if options.feature_flags != INKPOD_FEATURE_NONE || options.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "cell creation options contain unsupported flags",
            );
        }
        let sizing = match options.sizing_mode {
            INKPOD_CELL_SIZING_IMAGE_PIXELS => CellSizing::ImagePixels {
                width: options.width,
                height: options.height,
            },
            INKPOD_CELL_SIZING_FRAME_MICROMETRES => CellSizing::FrameMicrometres {
                width: options.width,
                height: options.height,
            },
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "cell sizing mode is not defined",
                );
            }
        };
        let anchor = match options.anchor {
            INKPOD_FRAME_ANCHOR_TOP_LEFT => FrameAnchor::TopLeft,
            INKPOD_FRAME_ANCHOR_TOP_RIGHT => FrameAnchor::TopRight,
            INKPOD_FRAME_ANCHOR_CENTER => FrameAnchor::Center,
            INKPOD_FRAME_ANCHOR_BOTTOM_LEFT => FrameAnchor::BottomLeft,
            INKPOD_FRAME_ANCHOR_BOTTOM_RIGHT => FrameAnchor::BottomRight,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "frame anchor is not defined",
                );
            }
        };
        let initial_layer_kind = match parse_layer_kind(options.initial_layer_kind) {
            Ok(kind) => kind,
            Err(status) => return status,
        };
        let pixel_format = match parse_storage_format(options.pixel_format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let input = CellCreationOptions {
            sizing,
            dpi_x_milli: options.dpi_x_milli,
            dpi_y_milli: options.dpi_y_milli,
            margin_milli: options.margin_milli,
            safe_frame_ratio_milli: options.safe_frame_ratio_milli,
            maximum_close_ratio_milli: options.maximum_close_ratio_milli,
            anchor,
            initial_layer_kind,
            pixel_format,
            count: options.count,
        };
        match plan_cell_creation(&input) {
            Ok(plan) => {
                let handle = Box::new(InkpodCellCreationPlan {
                    plan,
                    sizing_mode: options.sizing_mode,
                });
                // SAFETY: The caller-owned pointer was validated above.
                unsafe { out_plan.write(Box::into_raw(handle)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Returns the bounded number of immutable items in a creation plan.
///
/// # Safety
/// `plan` must be a live plan handle and `out_count` writable aligned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cell_creation_plan_count(
    plan: *const InkpodCellCreationPlan,
    out_count: *mut u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if plan.is_null() || !is_aligned(plan) || out_count.is_null() || !is_aligned(out_count) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "cell plan or count output is null or misaligned",
            );
        }
        // SAFETY: The caller guarantees a live immutable handle and writable count.
        let count = unsafe { &*plan }.plan.len() as u32;
        unsafe { out_count.write(count) };
        INKPOD_STATUS_OK
    })
}

/// Copies all immutable plan items to a caller-owned strided output.
///
/// # Safety
/// `plan` must be live. `output` must expose `capacity` writable records at the
/// supplied stride, each initialized with its exact `struct_size`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cell_creation_plan_copy(
    plan: *const InkpodCellCreationPlan,
    output: *mut InkpodCellCreationPlanItem,
    capacity: u32,
    stride_bytes: u64,
    out_written: *mut u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if plan.is_null() || !is_aligned(plan) || out_written.is_null() || !is_aligned(out_written)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "cell plan or written output is null or misaligned",
            );
        }
        // SAFETY: Writable scalar storage is required by the caller contract.
        unsafe { out_written.write(0) };
        // SAFETY: The caller guarantees a live immutable handle.
        let plan = unsafe { &*plan };
        let count = plan.plan.len();
        if capacity < count as u32
            || capacity > MAX_CELL_CREATION_COUNT
            || output.is_null()
            || !is_aligned(output)
            || stride_bytes < size_of::<InkpodCellCreationPlanItem>() as u64
            || stride_bytes > isize::MAX as u64
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "cell plan output span is invalid or too small",
            );
        }
        let total = (count.saturating_sub(1) as u64)
            .checked_mul(stride_bytes)
            .and_then(|bytes| bytes.checked_add(size_of::<InkpodCellCreationPlanItem>() as u64));
        if total.is_none_or(|bytes| bytes > isize::MAX as u64) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "cell plan output span overflows",
            );
        }
        for index in 0..count {
            // SAFETY: The validated total span contains this record start.
            let destination = unsafe {
                output
                    .cast::<u8>()
                    .add(index * stride_bytes as usize)
                    .cast::<InkpodCellCreationPlanItem>()
            };
            // SAFETY: Each advertised record prefix is readable by contract.
            if let Err(status) =
                unsafe { validate_struct(destination.cast_const(), "InkpodCellCreationPlanItem") }
            {
                return status;
            }
        }
        for (index, item) in plan.plan.iter().enumerate() {
            // SAFETY: The preceding pass validated the complete output span and
            // every advertised destination record before any record is changed.
            let destination = unsafe {
                output
                    .cast::<u8>()
                    .add(index * stride_bytes as usize)
                    .cast::<InkpodCellCreationPlanItem>()
            };
            let frames = item.frames();
            let record = InkpodCellCreationPlanItem {
                struct_size: size_of::<InkpodCellCreationPlanItem>() as u32,
                sizing_mode: plan.sizing_mode,
                width: item.width(),
                height: item.height(),
                dpi_x_milli: item.dpi_x_milli(),
                dpi_y_milli: item.dpi_y_milli(),
                initial_layer_kind: layer_kind_code(item.initial_layer_kind()),
                pixel_format: storage_format_code(item.pixel_format()),
                hundred_frame: frame_rect(frames.hundred_frame),
                reference_frame: frame_rect(frames.reference_frame),
                drawing_frame: frame_rect(frames.drawing_frame),
                safe_frame: frame_rect(frames.safe_frame),
                shooting_frame: frame_rect(frames.shooting_frame),
                maximum_close_frame: frame_rect(frames.maximum_close_frame),
                margin_left: frames.margins.left,
                margin_top: frames.margins.top,
                margin_right: frames.margins.right,
                margin_bottom: frames.margins.bottom,
            };
            // SAFETY: The destination record is writable and non-overlapping.
            unsafe { destination.write(record) };
        }
        // SAFETY: Writable scalar storage was validated above.
        unsafe { out_written.write(count as u32) };
        INKPOD_STATUS_OK
    })
}

/// Releases a Rust-owned immutable creation plan and nulls its owner pointer.
///
/// # Safety
/// `plan` must be writable owner storage containing null or one live plan handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_cell_creation_plan_release(
    plan: *mut *mut InkpodCellCreationPlan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if plan.is_null() || !is_aligned(plan) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "cell plan owner pointer is null or misaligned",
            );
        }
        // SAFETY: Owner storage is readable and writable by contract.
        let handle = unsafe { plan.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "cell plan handle is misaligned",
            );
        }
        unsafe { plan.write(ptr::null_mut()) };
        // SAFETY: Ownership originated from Box::into_raw and is consumed once.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Replaces one Core document from an immutable plan item and caller-supplied UUID.
///
/// # Safety
/// Handles must be live, `core` must be used on its owner thread, and `out_info`
/// must expose a complete writable size-prefixed record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_new_cell_from_plan(
    core: *mut InkpodCore,
    plan: *const InkpodCellCreationPlan,
    index: u32,
    document_uuid_high: u64,
    document_uuid_low: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || plan.is_null() || !is_aligned(plan) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or cell plan is null or misaligned",
            );
        }
        // SAFETY: The output has a readable advertised prefix by contract.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Live handles and writable output are required by contract.
        let core = unsafe { &mut *core };
        let plan = unsafe { &*plan };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let Some(item) = plan.plan.item(index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "cell plan item index is out of range",
            );
        };
        let uuid = (u128::from(document_uuid_high) << 64) | u128::from(document_uuid_low);
        match core.core.new_cell_from_creation_plan(item, uuid) {
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

/// Transactionally updates the six production frames and independent margins.
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
            shooting_frame: frame(input.shooting_frame),
            maximum_close_frame: frame(input.maximum_close_frame),
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
