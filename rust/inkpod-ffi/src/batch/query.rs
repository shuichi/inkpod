use super::*;

fn operation_at<'a>(graph: *const InkpodBatchGraph, index: u64) -> Result<&'a BatchOperation, u32> {
    if graph.is_null() || !is_aligned(graph) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch graph is null or misaligned",
        ));
    }
    let index = usize::try_from(index).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch operation index is not representable",
        )
    })?;
    unsafe { &*graph }
        .graph
        .operations
        .get(index)
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch operation index is outside bounds",
            )
        })
}

fn operation_info(
    operation: &BatchOperation,
    struct_size: u32,
) -> Result<InkpodBatchOperationInfo, u32> {
    let mut output = InkpodBatchOperationInfo {
        struct_size,
        version: operation.version,
        flags: if operation.enabled {
            INKPOD_BATCH_OPERATION_ENABLED
        } else {
            0
        },
        ..InkpodBatchOperationInfo::default()
    };
    let target = operation.target;
    output.layer_id = target.layer_id.unwrap_or(0);
    output.plane_id = target.plane_id.unwrap_or(0);
    output.plane_kind = target.plane_kind.map_or(0, plane_type_code);
    output.missing_policy = match target.missing_policy {
        BatchMissingTargetPolicy::Skip => INKPOD_BATCH_MISSING_SKIP,
        BatchMissingTargetPolicy::Error => INKPOD_BATCH_MISSING_ERROR,
    };
    output.target_count = 1_u64.saturating_add(operation.additional_targets.len() as u64);
    match &operation.kind {
        BatchOperationKind::ColorReplace(pairs) => {
            output.kind = INKPOD_BATCH_OPERATION_COLOR_REPLACE;
            output.color_pair_count = pairs.len() as u64;
        }
        BatchOperationKind::MoveToColorPlane(colors) => {
            output.kind = INKPOD_BATCH_OPERATION_MOVE_TO_COLOR_PLANE;
            output.color_count = colors.len() as u64;
        }
        BatchOperationKind::Masking(colors) => {
            output.kind = INKPOD_BATCH_OPERATION_MASKING;
            output.color_count = colors.len() as u64;
        }
        BatchOperationKind::Erase(colors) => {
            output.kind = INKPOD_BATCH_OPERATION_ERASE;
            output.color_count = colors.len() as u64;
        }
    }
    Ok(output)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_input(
    graph: *const InkpodBatchGraph,
    index: u64,
    out_input: *mut InkpodBatchInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if graph.is_null() || !is_aligned(graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(out_input.cast_const(), "InkpodBatchInput") }
        {
            return status;
        }
        let Ok(index) = usize::try_from(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch input index is not representable",
            );
        };
        let Some(input) = unsafe { &*graph }.graph.inputs.get(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch input index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_input };
        output.kind = match input.kind {
            BatchInputKind::File => INKPOD_BATCH_INPUT_FILE,
            BatchInputKind::Folder => INKPOD_BATCH_INPUT_FOLDER,
            BatchInputKind::ActiveDocument => INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT,
        };
        output.feature_flags = INKPOD_FEATURE_NONE;
        output.path_utf8 = input.path.as_bytes().as_ptr();
        output.path_bytes = input.path.len() as u64;
        output.first_cell = input.first_cell;
        output.last_cell = input.last_cell;
        output.reserved = 0;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_operation(
    graph: *const InkpodBatchGraph,
    index: u64,
    out_info: *mut InkpodBatchOperationInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodBatchOperationInfo") }
        {
            return status;
        }
        let operation = match operation_at(graph, index) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let info = match operation_info(operation, unsafe { (*out_info).struct_size }) {
            Ok(info) => info,
            Err(status) => return status,
        };
        unsafe { out_info.write(info) };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_operation_target(
    graph: *const InkpodBatchGraph,
    operation_index: u64,
    target_index: u64,
    out_target: *mut InkpodBatchTargetInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) =
            unsafe { validate_struct(out_target.cast_const(), "InkpodBatchTargetInput") }
        {
            return status;
        }
        let operation = match operation_at(graph, operation_index) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let target = if target_index == 0 {
            Some(&operation.target)
        } else {
            usize::try_from(target_index - 1)
                .ok()
                .and_then(|index| operation.additional_targets.get(index))
        };
        let Some(target) = target else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch target index is outside bounds",
            );
        };
        let output = InkpodBatchTargetInput {
            struct_size: unsafe { (*out_target).struct_size },
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            layer_id: target.layer_id.unwrap_or(0),
            plane_id: target.plane_id.unwrap_or(0),
            plane_kind: target.plane_kind.map_or(0, plane_type_code),
            missing_policy: match target.missing_policy {
                BatchMissingTargetPolicy::Skip => INKPOD_BATCH_MISSING_SKIP,
                BatchMissingTargetPolicy::Error => INKPOD_BATCH_MISSING_ERROR,
            },
            reserved_2: 0,
        };
        unsafe { out_target.write(output) };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_operation_color(
    graph: *const InkpodBatchGraph,
    operation_index: u64,
    color_index: u64,
    out_color: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") }
        {
            return status;
        }
        let operation = match operation_at(graph, operation_index) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let Ok(index) = usize::try_from(color_index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch color index is not representable",
            );
        };
        let colors = match &operation.kind {
            BatchOperationKind::MoveToColorPlane(colors)
            | BatchOperationKind::Masking(colors)
            | BatchOperationKind::Erase(colors) => colors,
            BatchOperationKind::ColorReplace(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch color-replacement operation has no color list",
                );
            }
        };
        let Some(color) = colors.get(index).copied() else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch color index is outside bounds",
            );
        };
        match write_color_value(unsafe { &mut *out_color }, color) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(status) => status,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_operation_color_pair(
    graph: *const InkpodBatchGraph,
    operation_index: u64,
    pair_index: u64,
    out_pair: *mut InkpodBatchColorPairInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) =
            unsafe { validate_struct(out_pair.cast_const(), "InkpodBatchColorPairInput") }
        {
            return status;
        }
        let operation = match operation_at(graph, operation_index) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let Ok(index) = usize::try_from(pair_index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch color-pair index is not representable",
            );
        };
        let BatchOperationKind::ColorReplace(pairs) = &operation.kind else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch operation has no color pairs",
            );
        };
        let Some(pair) = pairs.get(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch color-pair index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_pair };
        output.enabled = u32::from(pair.enabled);
        output.reserved = 0;
        output.old_color = match color_value_record(pair.old) {
            Ok(value) => value,
            Err(status) => return status,
        };
        output.new_color = match color_value_record(pair.new) {
            Ok(value) => value,
            Err(status) => return status,
        };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_clone_with_operations(
    graph: *const InkpodBatchGraph,
    operations: *const InkpodBatchOperationInput,
    operation_count: u64,
    operation_stride_bytes: u64,
    out_graph: *mut *mut InkpodBatchGraph,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if graph.is_null() || !is_aligned(graph) || out_graph.is_null() || !is_aligned(out_graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch clone pointer is null or misaligned",
            );
        }
        if !unsafe { out_graph.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch clone output already owns a handle",
            );
        }
        let parsed = match unsafe {
            parse_operation_records(operations, operation_count, operation_stride_bytes)
        } {
            Ok(operations) => operations,
            Err(status) => return status,
        };
        let mut graph = unsafe { &*graph }.graph.clone();
        graph.operations = parsed;
        if let Err(error) = graph.validate() {
            return map_core_error(error);
        }
        unsafe { out_graph.write(Box::into_raw(Box::new(InkpodBatchGraph { graph }))) };
        INKPOD_STATUS_OK
    })
}
