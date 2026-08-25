use super::*;

pub(super) unsafe fn utf8_text<'a>(
    pointer: *const u8,
    length: u64,
    allow_empty: bool,
    field: &str,
) -> Result<&'a str, u32> {
    if length == 0 {
        if allow_empty {
            return Ok("");
        }
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} is empty"),
        ));
    }
    if pointer.is_null() || length > MAX_BATCH_TEXT_BYTES {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} pointer or length is invalid"),
        ));
    }
    let length = usize::try_from(length).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} length is not representable"),
        )
    })?;
    // SAFETY: The caller contract requires this complete borrowed byte span.
    let bytes = unsafe { slice::from_raw_parts(pointer, length) };
    let text = std::str::from_utf8(bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} is not UTF-8"),
        )
    })?;
    if bytes.contains(&0) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} contains an embedded NUL"),
        ));
    }
    Ok(text)
}

pub(super) unsafe fn record_at<T>(
    base: *const T,
    count: u64,
    stride: u64,
    index: usize,
    maximum: usize,
    type_name: &str,
) -> Result<*const T, u32> {
    let count = usize::try_from(count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{type_name} count is not representable"),
        )
    })?;
    if count == 0 || count > maximum || index >= count || base.is_null() || !is_aligned(base) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{type_name} span is invalid"),
        ));
    }
    let stride = usize::try_from(stride).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{type_name} stride is not representable"),
        )
    })?;
    if stride < size_of::<T>() || stride % align_of::<T>() != 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{type_name} stride is invalid"),
        ));
    }
    let storage_bytes = count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<T>()));
    if storage_bytes.is_none_or(|bytes| bytes > isize::MAX as usize) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{type_name} storage size is invalid"),
        ));
    }
    let offset = index.checked_mul(stride).ok_or_else(|| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{type_name} offset overflows"),
        )
    })?;
    // SAFETY: The public span contract covers count records at this stride.
    let pointer = unsafe { base.cast::<u8>().add(offset).cast::<T>() };
    // SAFETY: The record's readable size prefix and full advertised body are required.
    let struct_size = unsafe { validate_struct(pointer, type_name) }?;
    if u64::from(struct_size) > stride as u64 {
        return Err(fail(
            INKPOD_STATUS_INCOMPATIBLE_ABI,
            &format!("{type_name}.struct_size exceeds its record stride"),
        ));
    }
    Ok(pointer)
}

pub(super) unsafe fn parse_graph_input(
    input: *const InkpodBatchGraphInput,
) -> Result<BatchGraph, u32> {
    // SAFETY: Forwarded from the exported function contract.
    unsafe { validate_struct(input, "InkpodBatchGraphInput") }?;
    // SAFETY: The complete known record is readable after validation.
    let input = unsafe { &*input };
    if input.feature_flags != INKPOD_FEATURE_NONE
        || input.reserved != 0
        || input.output_flags & !INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE != 0
    {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "batch graph contains unsupported flags",
        ));
    }
    // SAFETY: Borrowed UTF-8 spans are required by the graph record contract.
    let name = unsafe { utf8_text(input.name_utf8, input.name_bytes, false, "batch name") }?;
    let folder = unsafe {
        utf8_text(
            input.output_folder_utf8,
            input.output_folder_bytes,
            true,
            "batch output folder",
        )
    }?;
    let naming_template = unsafe {
        utf8_text(
            input.naming_template_utf8,
            input.naming_template_bytes,
            true,
            "batch naming template",
        )
    }?;
    let input_count = usize::try_from(input.input_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch input count is not representable",
        )
    })?;
    if input_count == 0 || input_count > MAX_BATCH_INPUTS {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch input count is outside bounds",
        ));
    }
    let mut selectors = Vec::with_capacity(input_count);
    for index in 0..input_count {
        let pointer = unsafe {
            record_at(
                input.inputs,
                input.input_count,
                input.input_stride_bytes,
                index,
                MAX_BATCH_INPUTS,
                "InkpodBatchInput",
            )
        }?;
        // SAFETY: record_at validated the complete record.
        let record = unsafe { &*pointer };
        if record.feature_flags != INKPOD_FEATURE_NONE || record.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "batch input contains unsupported flags",
            ));
        }
        let kind = match record.kind {
            INKPOD_BATCH_INPUT_FILE => BatchInputKind::File,
            INKPOD_BATCH_INPUT_FOLDER => BatchInputKind::Folder,
            INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT => BatchInputKind::ActiveDocument,
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch input kind is unknown",
                ));
            }
        };
        let allow_empty = kind == BatchInputKind::ActiveDocument;
        let path = unsafe {
            utf8_text(
                record.path_utf8,
                record.path_bytes,
                allow_empty,
                "batch input path",
            )
        }?;
        selectors.push(BatchInputSelector {
            kind,
            path: path.to_owned(),
            first_cell: record.first_cell,
            last_cell: record.last_cell,
        });
    }
    let operations = unsafe {
        parse_operation_records(
            input.operations,
            input.operation_count,
            input.operation_stride_bytes,
        )
    }?;
    Ok(BatchGraph {
        version: input.version,
        name: name.to_owned(),
        inputs: selectors,
        operations,
        output: BatchOutputSettings {
            destination: match input.output_destination {
                INKPOD_BATCH_OUTPUT_FOLDER => BatchOutputDestination::Folder,
                INKPOD_BATCH_OUTPUT_ACTIVE_DOCUMENT => BatchOutputDestination::ActiveDocument,
                INKPOD_BATCH_OUTPUT_NEW_TABS => BatchOutputDestination::NewTabs,
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch output policy is unknown",
                    ));
                }
            },
            format: match input.output_format {
                INKPOD_BATCH_FORMAT_INKPOD => BatchOutputFormat::Inkpod,
                INKPOD_BATCH_FORMAT_PNG => BatchOutputFormat::Png,
                INKPOD_BATCH_FORMAT_TIFF => BatchOutputFormat::Tiff,
                INKPOD_BATCH_FORMAT_TGA => BatchOutputFormat::Tga,
                INKPOD_BATCH_FORMAT_BMP => BatchOutputFormat::Bmp,
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch output format is unknown",
                    ));
                }
            },
            folder: folder.to_owned(),
            naming_template: naming_template.to_owned(),
            failure_policy: match input.failure_policy {
                INKPOD_BATCH_FAILURE_CONTINUE => BatchFailurePolicy::Continue,
                INKPOD_BATCH_FAILURE_STOP => BatchFailurePolicy::Stop,
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch failure policy is unknown",
                    ));
                }
            },
            wait_milliseconds: input.wait_milliseconds,
            preview_before_save: input.output_flags & INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE != 0,
        },
    })
}

pub(super) unsafe fn parse_operation_records(
    records: *const InkpodBatchOperationInput,
    record_count: u64,
    record_stride_bytes: u64,
) -> Result<Vec<BatchOperation>, u32> {
    let operation_count = usize::try_from(record_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch operation count is not representable",
        )
    })?;
    if operation_count == 0 || operation_count > MAX_BATCH_OPERATIONS {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch operation count is outside bounds",
        ));
    }
    let mut operations = Vec::with_capacity(operation_count);
    for index in 0..operation_count {
        let pointer = unsafe {
            record_at(
                records,
                record_count,
                record_stride_bytes,
                index,
                MAX_BATCH_OPERATIONS,
                "InkpodBatchOperationInput",
            )
        }?;
        // SAFETY: record_at validated this complete record and every nested parser copies spans.
        operations.push(unsafe { parse_operation(&*pointer) }?);
    }
    Ok(operations)
}

pub(super) unsafe fn parse_operation(
    record: &InkpodBatchOperationInput,
) -> Result<BatchOperation, u32> {
    if record.reserved != 0
        || record.reserved_2 != 0
        || record.reserved_3 != 0
        || record.reserved_4 != 0
        || record.flags & !INKPOD_BATCH_OPERATION_ENABLED != 0
    {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "batch operation contains unsupported flags or reserved fields",
        ));
    }
    let target = parse_target(record)?;
    let additional_target_count =
        usize::try_from(record.additional_target_count).map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch additional target count is not representable",
            )
        })?;
    if additional_target_count >= MAX_BATCH_TARGETS {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch target count is outside bounds",
        ));
    }
    let mut additional_targets = Vec::with_capacity(additional_target_count);
    for index in 0..additional_target_count {
        let pointer = unsafe {
            record_at(
                record.additional_targets,
                record.additional_target_count,
                record.additional_target_stride_bytes,
                index,
                MAX_BATCH_TARGETS - 1,
                "InkpodBatchTargetInput",
            )
        }?;
        // SAFETY: record_at validated this complete target record.
        additional_targets.push(parse_target_record(unsafe { &*pointer })?);
    }
    let kind = match record.kind {
        INKPOD_BATCH_OPERATION_COLOR_REPLACE => {
            let count = checked_count(record.color_pair_count, MAX_BATCH_PAIRS, "color pair")?;
            let mut pairs = Vec::with_capacity(count);
            for index in 0..count {
                let pointer = unsafe {
                    record_at(
                        record.color_pairs,
                        record.color_pair_count,
                        record.color_pair_stride_bytes,
                        index,
                        MAX_BATCH_PAIRS,
                        "InkpodBatchColorPairInput",
                    )
                }?;
                // SAFETY: record_at validated the complete record.
                let pair = unsafe { &*pointer };
                if pair.enabled > 1 || pair.reserved != 0 {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch color pair fields are invalid",
                    ));
                }
                pairs.push(BatchColorPair {
                    enabled: pair.enabled != 0,
                    old: unsafe { parse_color_value(ptr::addr_of!(pair.old_color)) }?,
                    new: unsafe { parse_color_value(ptr::addr_of!(pair.new_color)) }?,
                });
            }
            BatchOperationKind::ColorReplace(pairs)
        }
        INKPOD_BATCH_OPERATION_MOVE_TO_COLOR_PLANE => {
            BatchOperationKind::MoveToColorPlane(unsafe { parse_color_array(&record.colors) }?)
        }
        INKPOD_BATCH_OPERATION_MASKING => {
            BatchOperationKind::Masking(unsafe { parse_color_array(&record.colors) }?)
        }
        INKPOD_BATCH_OPERATION_ERASE => {
            BatchOperationKind::Erase(unsafe { parse_color_array(&record.colors) }?)
        }
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch operation kind is unknown",
            ));
        }
    };
    Ok(BatchOperation {
        version: record.version,
        enabled: record.flags & INKPOD_BATCH_OPERATION_ENABLED != 0,
        target,
        additional_targets,
        kind,
    })
}

pub(super) fn parse_target(record: &InkpodBatchOperationInput) -> Result<BatchTargetSelector, u32> {
    parse_target_fields(
        record.layer_id,
        record.plane_id,
        record.layer_kind,
        record.plane_kind,
        record.missing_policy,
    )
}

fn parse_target_record(record: &InkpodBatchTargetInput) -> Result<BatchTargetSelector, u32> {
    if record.reserved != 0 || record.feature_flags != 0 || record.reserved_2 != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "batch target contains unsupported flags or reserved fields",
        ));
    }
    parse_target_fields(
        record.layer_id,
        record.plane_id,
        record.layer_kind,
        record.plane_kind,
        record.missing_policy,
    )
}

fn parse_target_fields(
    layer_id: u64,
    plane_id: u64,
    layer_kind: u32,
    plane_kind: u32,
    missing_policy: u32,
) -> Result<BatchTargetSelector, u32> {
    Ok(BatchTargetSelector {
        layer_id: (layer_id != 0).then_some(layer_id),
        plane_id: (plane_id != 0).then_some(plane_id),
        layer_kind: (layer_kind != 0)
            .then(|| parse_layer_kind(layer_kind))
            .transpose()?,
        plane_kind: (plane_kind != 0)
            .then(|| parse_plane_kind(i64::from(plane_kind)))
            .transpose()?,
        missing_policy: match missing_policy {
            INKPOD_BATCH_MISSING_SKIP => BatchMissingTargetPolicy::Skip,
            INKPOD_BATCH_MISSING_ERROR => BatchMissingTargetPolicy::Error,
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch missing-target policy is unknown",
                ));
            }
        },
    })
}

pub(super) fn checked_count(value: u64, maximum: usize, field: &str) -> Result<usize, u32> {
    let count = usize::try_from(value).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} count is not representable"),
        )
    })?;
    if count == 0 || count > maximum {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} count is outside bounds"),
        ));
    }
    Ok(count)
}

pub(super) fn parse_layer_kind(value: u32) -> Result<LayerKind, u32> {
    match value {
        INKPOD_LAYER_BINARY_COLORING => Ok(LayerKind::BinaryColoring),
        INKPOD_LAYER_GRAYSCALE_COLORING => Ok(LayerKind::GrayscaleColoring),
        INKPOD_LAYER_RASTER => Ok(LayerKind::Raster),
        INKPOD_LAYER_SELECTION => Ok(LayerKind::Selection),
        INKPOD_LAYER_FRAME => Ok(LayerKind::Frame),
        INKPOD_LAYER_VANISHING_POINT => Ok(LayerKind::VanishingPoint),
        INKPOD_LAYER_ADJUSTMENT => Ok(LayerKind::Adjustment),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch layer kind is unknown",
        )),
    }
}

pub(super) fn parse_plane_kind(value: i64) -> Result<PlaneType, u32> {
    match u32::try_from(value).ok() {
        Some(INKPOD_TYPED_PLANE_MAIN_LINE) => Ok(PlaneType::MainLine),
        Some(INKPOD_TYPED_PLANE_COLOR) => Ok(PlaneType::Color),
        Some(INKPOD_TYPED_PLANE_RASTER) => Ok(PlaneType::Raster),
        Some(INKPOD_TYPED_PLANE_SELECTION) => Ok(PlaneType::Selection),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch plane kind is unknown",
        )),
    }
}

pub(super) fn scope(value: u32) -> Result<BatchRunScope, u32> {
    match value {
        INKPOD_BATCH_SCOPE_CURRENT => Ok(BatchRunScope::Current),
        INKPOD_BATCH_SCOPE_ALL => Ok(BatchRunScope::All),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch run scope is unknown",
        )),
    }
}

pub(super) fn output_policy_value(value: BatchOutputDestination) -> u32 {
    match value {
        BatchOutputDestination::Folder => INKPOD_BATCH_OUTPUT_FOLDER,
        BatchOutputDestination::ActiveDocument => INKPOD_BATCH_OUTPUT_ACTIVE_DOCUMENT,
        BatchOutputDestination::NewTabs => INKPOD_BATCH_OUTPUT_NEW_TABS,
    }
}

pub(super) const fn output_format_value(value: BatchOutputFormat) -> u32 {
    match value {
        BatchOutputFormat::Inkpod => INKPOD_BATCH_FORMAT_INKPOD,
        BatchOutputFormat::Png => INKPOD_BATCH_FORMAT_PNG,
        BatchOutputFormat::Tiff => INKPOD_BATCH_FORMAT_TIFF,
        BatchOutputFormat::Tga => INKPOD_BATCH_FORMAT_TGA,
        BatchOutputFormat::Bmp => INKPOD_BATCH_FORMAT_BMP,
    }
}

pub(super) fn failure_policy_value(value: BatchFailurePolicy) -> u32 {
    match value {
        BatchFailurePolicy::Continue => INKPOD_BATCH_FAILURE_CONTINUE,
        BatchFailurePolicy::Stop => INKPOD_BATCH_FAILURE_STOP,
    }
}

pub(super) fn output_flags(value: &BatchOutputSettings) -> u64 {
    if value.preview_before_save {
        INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE
    } else {
        0
    }
}

pub(super) fn bytes_for_path(path: Option<PathBuf>) -> Box<[u8]> {
    path.map_or_else(
        || Vec::new().into_boxed_slice(),
        |path| {
            path.to_string_lossy()
                .into_owned()
                .into_bytes()
                .into_boxed_slice()
        },
    )
}
