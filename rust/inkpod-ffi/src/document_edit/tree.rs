use super::*;

/// Applies one typed layer/plane edit. Name bytes are borrowed only for the
/// call. `out_object_id` receives a created/duplicated ID or zero.
///
/// # Safety
/// `core` must be a live handle used on its owner thread. `input` and `result`
/// must expose complete non-overlapping records, any advertised name range must
/// be readable, and `out_object_id` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_tree_edit(
    core: *mut InkpodCore,
    input: *const InkpodTreeEdit,
    result: *mut InkpodDispatchResult,
    out_object_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_object_id.is_null()
            || !is_aligned(out_object_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "tree edit pointer is null or misaligned",
            );
        }
        // SAFETY: Public structure pointers expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodTreeEdit") } {
            return status;
        }
        // SAFETY: Result prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects and output storage are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        // SAFETY: out_object_id is writable by contract and was validated above.
        unsafe { out_object_id.write(0) };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.flags & !(INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE) != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "tree edit contains unsupported flags",
            );
        }
        let name = if matches!(
            input.operation,
            INKPOD_TREE_CREATE_LAYER
                | INKPOD_TREE_SET_LAYER_PROPERTIES
                | INKPOD_TREE_CREATE_PLANE
                | INKPOD_TREE_SET_PLANE_PROPERTIES
        ) {
            // SAFETY: The input contract includes the advertised name byte range.
            match unsafe { name_from_utf8(input.name_utf8, input.name_bytes) } {
                Ok(name) => Some(name),
                Err(status) => return status,
            }
        } else {
            None
        };
        let operation: Result<(inkpod_core::DispatchOutcome, u64), CoreError> = match input
            .operation
        {
            INKPOD_TREE_CREATE_LAYER => match parse_layer_kind(input.kind) {
                Ok(kind) => core.core.create_layer(kind, name.expect("name parsed")),
                Err(status) => return status,
            },
            INKPOD_TREE_DUPLICATE_LAYER => core.core.duplicate_layer(input.object_id),
            INKPOD_TREE_DELETE_LAYER => core
                .core
                .delete_layer(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_REORDER_LAYER => core
                .core
                .reorder_layer(input.object_id, input.destination_index as usize)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_SET_LAYER_PROPERTIES => core
                .core
                .set_layer_properties(
                    input.object_id,
                    input.flags & INKPOD_NODE_VISIBLE != 0,
                    input.flags & INKPOD_NODE_EDITABLE != 0,
                    input.opacity_milli,
                    name.expect("name parsed"),
                )
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_CREATE_PLANE => {
                let kind = match parse_plane_type(input.kind) {
                    Ok(kind) => kind,
                    Err(status) => return status,
                };
                let format = match parse_storage_format(input.pixel_format) {
                    Ok(format) => format,
                    Err(status) => return status,
                };
                core.core
                    .create_plane(input.parent_id, kind, format, name.expect("name parsed"))
            }
            INKPOD_TREE_DUPLICATE_PLANE => core.core.duplicate_plane(input.object_id),
            INKPOD_TREE_DELETE_PLANE => core
                .core
                .delete_plane(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_REORDER_PLANE => core
                .core
                .reorder_plane(input.object_id, input.destination_index as usize)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_SET_PLANE_PROPERTIES => core
                .core
                .set_plane_properties(
                    input.object_id,
                    input.flags & INKPOD_NODE_VISIBLE != 0,
                    input.flags & INKPOD_NODE_EDITABLE != 0,
                    input.opacity_milli,
                    name.expect("name parsed"),
                )
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_CONVERT_LAYER => match parse_layer_kind(input.kind) {
                Ok(kind) => core
                    .core
                    .convert_layer(input.object_id, kind)
                    .map(|outcome| (outcome, 0)),
                Err(status) => return status,
            },
            INKPOD_TREE_MERGE_LAYER => core
                .core
                .merge_layer_into_below(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_CONVERT_PLANE => {
                let kind = match parse_plane_type(input.kind) {
                    Ok(kind) => kind,
                    Err(status) => return status,
                };
                let format = match parse_storage_format(input.pixel_format) {
                    Ok(format) => format,
                    Err(status) => return status,
                };
                core.core
                    .convert_plane(input.object_id, kind, format)
                    .map(|outcome| (outcome, 0))
            }
            INKPOD_TREE_MERGE_PLANE => core
                .core
                .merge_plane_into_below(input.object_id)
                .map(|outcome| (outcome, 0)),
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "tree edit operation is not defined",
                );
            }
        };
        match operation {
            Ok((outcome, object_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: out_object_id is writable by the exported contract.
                unsafe { out_object_id.write(object_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Queries a layer (`plane_index == UINT32_MAX`) or one of its planes. Name
/// storage remains caller-owned and `name_bytes` always receives the required
/// byte count excluding a terminator.
///
/// # Safety
/// `core` must be a live handle used on its owner thread. `out_info` must be a
/// writable complete record and its optional name buffer must cover the
/// advertised capacity without overlapping Core storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_node_get(
    core: *mut InkpodCore,
    layer_index: u32,
    plane_index: u32,
    out_info: *mut InkpodNodeInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodNodeInfo") } {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let out = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let layers = match core.core.layers() {
            Ok(layers) => layers,
            Err(error) => return map_core_error(error),
        };
        let Some(layer) = layers.get(layer_index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "layer index is outside the tree",
            );
        };
        let (id, parent_id, kind, pixel_format, opacity, flags, child_count, name) =
            if plane_index == u32::MAX {
                (
                    layer.id,
                    0,
                    layer_kind_code(layer.kind),
                    0,
                    layer.opacity_milli,
                    u32::from(layer.visible) | (u32::from(layer.editable) << 1),
                    layer.planes.len() as u32,
                    layer.name.as_str(),
                )
            } else {
                let Some(plane) = layer.planes.get(plane_index as usize) else {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "plane index is outside its layer",
                    );
                };
                (
                    plane.id,
                    layer.id,
                    plane_type_code(plane.kind),
                    storage_format_code(plane.pixel_format),
                    plane.opacity_milli,
                    u32::from(plane.visible) | (u32::from(plane.editable) << 1),
                    0,
                    plane.name.as_str(),
                )
            };
        out.flags = flags;
        out.id = id;
        out.parent_id = parent_id;
        out.kind = kind;
        out.pixel_format = pixel_format;
        out.opacity_milli = opacity;
        out.index = if plane_index == u32::MAX {
            layer_index
        } else {
            plane_index
        };
        out.child_count = child_count;
        out.reserved = 0;
        out.name_bytes = name.len() as u64;
        if out.name_capacity == 0 {
            if !out.name_utf8.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity name buffer must be null",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if out.name_utf8.is_null() || out.name_capacity < out.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "node name buffer is too small",
            );
        }
        // SAFETY: Caller provides the complete writable capacity advertised in the output record.
        unsafe { ptr::copy_nonoverlapping(name.as_ptr(), out.name_utf8, name.len()) };
        INKPOD_STATUS_OK
    })
}
