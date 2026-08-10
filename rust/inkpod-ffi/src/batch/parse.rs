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
        || input.output_flags
            & !(INKPOD_BATCH_OUTPUT_CELL_FOLDER
                | INKPOD_BATCH_OUTPUT_DESCENDING
                | INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE)
            != 0
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
    let basename = unsafe {
        utf8_text(
            input.basename_utf8,
            input.basename_bytes,
            true,
            "batch basename",
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
            INKPOD_BATCH_INPUT_CURRENT_SEQUENCE => BatchInputKind::CurrentSequence,
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch input kind is unknown",
                ));
            }
        };
        let allow_empty = kind == BatchInputKind::CurrentSequence;
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
            policy: match input.output_policy {
                INKPOD_BATCH_OUTPUT_DUPLICATE => BatchOutputPolicy::Duplicate,
                INKPOD_BATCH_OUTPUT_NEW_SAVE => BatchOutputPolicy::NewSave,
                INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE => BatchOutputPolicy::ExplicitOverwrite,
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch output policy is unknown",
                    ));
                }
            },
            folder: folder.to_owned(),
            cell_folder: input.output_flags & INKPOD_BATCH_OUTPUT_CELL_FOLDER != 0,
            basename: basename.to_owned(),
            start_number: input.start_number,
            descending: input.output_flags & INKPOD_BATCH_OUTPUT_DESCENDING != 0,
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
        || record.flags
            & !(INKPOD_BATCH_OPERATION_ENABLED | INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN)
            != 0
    {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "batch operation contains unsupported flags or reserved fields",
        ));
    }
    let target = parse_target(record)?;
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
        INKPOD_BATCH_OPERATION_CONTINUOUS_FILL => {
            let count = checked_count(record.seed_count, MAX_BATCH_SEEDS, "fill seed")?;
            let mut seeds = Vec::with_capacity(count);
            for index in 0..count {
                let pointer = unsafe {
                    record_at(
                        record.seeds,
                        record.seed_count,
                        record.seed_stride_bytes,
                        index,
                        MAX_BATCH_SEEDS,
                        "InkpodBatchSeedInput",
                    )
                }?;
                // SAFETY: record_at validated the complete record.
                let seed = unsafe { &*pointer };
                if seed.flags & !(INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR | INKPOD_BATCH_SEED_ENABLED)
                    != 0
                    || seed.reserved != 0
                {
                    return Err(fail(
                        INKPOD_STATUS_UNSUPPORTED,
                        "batch fill seed contains unsupported fields",
                    ));
                }
                seeds.push(BatchSeed {
                    enabled: seed.flags & INKPOD_BATCH_SEED_ENABLED != 0,
                    x: seed.x,
                    y: seed.y,
                    color: unsafe { parse_color_value(ptr::addr_of!(seed.fill_color)) }?,
                    tolerance: u16::try_from(seed.tolerance).map_err(|_| {
                        fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "batch fill tolerance is invalid",
                        )
                    })?,
                    gap_close: u8::try_from(seed.gap_close).map_err(|_| {
                        fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "batch fill gap-close value is invalid",
                        )
                    })?,
                    expected_source: if seed.flags & INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR != 0 {
                        Some(unsafe { parse_color_value(ptr::addr_of!(seed.expected_color)) }?)
                    } else {
                        None
                    },
                });
            }
            BatchOperationKind::ContinuousFill(seeds)
        }
        INKPOD_BATCH_OPERATION_SEPARATION => BatchOperationKind::Separation(BatchSeparation {
            colors: unsafe { parse_color_array(&record.colors) }?,
            replacement: unsafe { parse_color_value(ptr::addr_of!(record.color_0)) }?,
            invert: match record.parameters[0] {
                0 => false,
                INKPOD_BATCH_SEPARATION_INVERT => true,
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch separation flags are invalid",
                    ));
                }
            },
            destination: match record.parameters[1] {
                INKPOD_BATCH_SEPARATION_REPLACE_SOURCE => BatchSeparationDestination::ReplaceSource,
                INKPOD_BATCH_SEPARATION_SELECTION_MASK => BatchSeparationDestination::SelectionMask,
                INKPOD_BATCH_SEPARATION_MAIN_LINE_PLANE => {
                    BatchSeparationDestination::MainLinePlane
                }
                INKPOD_BATCH_SEPARATION_COLOR_PLANE => BatchSeparationDestination::ColorPlane,
                INKPOD_BATCH_SEPARATION_NATIVE_FILE => BatchSeparationDestination::NativeFile,
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch separation destination is invalid",
                    ));
                }
            },
        }),
        INKPOD_BATCH_OPERATION_VISIBILITY => BatchOperationKind::Visibility {
            visible: parameter_bool(record.parameters[0], "batch visibility")?,
        },
        INKPOD_BATCH_OPERATION_LINE_WIDTH => {
            let value = record.parameters[1] as f32 / 1_000.0;
            BatchOperationKind::LineWidth(match record.parameters[0] {
                1 => VectorWidthMode::Add(value),
                2 => VectorWidthMode::Subtract(value),
                3 => VectorWidthMode::Scale(value),
                4 => VectorWidthMode::Constant(value),
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch line-width mode is unknown",
                    ));
                }
            })
        }
        INKPOD_BATCH_OPERATION_FILTER => {
            if record.filter.is_null() {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch filter input is null",
                ));
            }
            // SAFETY: The nested filter pointer must expose a complete aligned
            // record before it can be converted to a Rust reference.
            unsafe { validate_struct(record.filter, "InkpodFilterInput") }?;
            BatchOperationKind::Filter(unsafe { parse_filter_input(&*record.filter) }?)
        }
        INKPOD_BATCH_OPERATION_BOUNDARY_AIRBRUSH => {
            let colors = unsafe { parse_color_array(&record.colors) }?
                .into_iter()
                .map(pixel_to_rgba16)
                .collect::<Result<Vec<_>, _>>()?;
            BatchOperationKind::BoundaryAirbrush(BoundaryAirbrush {
                colors,
                width: parameter_u32(record.parameters[0], "batch boundary width")?,
                strength_milli: parameter_u32(record.parameters[1], "batch boundary strength")?,
            })
        }
        INKPOD_BATCH_OPERATION_DUST_REMOVAL => BatchOperationKind::DustRemoval(DustRemoval {
            mode: match record.parameters[0] {
                1 => DustMode::RemoveForeground,
                2 => DustMode::FillTransparentHoles,
                3 => DustMode::ReplaceColorOutliers,
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch dust mode is unknown",
                    ));
                }
            },
            maximum_pixels: parameter_u32(record.parameters[1], "batch dust maximum pixels")?,
        }),
        INKPOD_BATCH_OPERATION_MIRROR => BatchOperationKind::Mirror(match record.parameters[0] {
            1 => MirrorAxis::Horizontal,
            2 => MirrorAxis::Vertical,
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch mirror axis is unknown",
                ));
            }
        }),
        INKPOD_BATCH_OPERATION_ROTATE_90 => {
            BatchOperationKind::Rotate90(match record.parameters[0] {
                1 => RotateDirection::Left90,
                2 => RotateDirection::Right90,
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch rotation direction is unknown",
                    ));
                }
            })
        }
        INKPOD_BATCH_OPERATION_RESIZE => BatchOperationKind::Resize(DocumentResize {
            width: parameter_u32(record.parameters[0], "batch resize width")?,
            height: parameter_u32(record.parameters[1], "batch resize height")?,
            dpi_x_milli: parameter_u32(record.parameters[2], "batch resize X DPI")?,
            dpi_y_milli: parameter_u32(record.parameters[3], "batch resize Y DPI")?,
            resample: parameter_bool(record.parameters[4], "batch resize resample")?,
            anchor: match record.parameters[5] {
                1 => ResizeAnchor::TopLeft,
                2 => ResizeAnchor::TopRight,
                3 => ResizeAnchor::Center,
                4 => ResizeAnchor::BottomLeft,
                5 => ResizeAnchor::BottomRight,
                _ => {
                    return Err(fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "batch resize anchor is unknown",
                    ));
                }
            },
        }),
        INKPOD_BATCH_OPERATION_CONVERT_PLANE => BatchOperationKind::ConvertPlane {
            destination_kind: parse_plane_kind(record.parameters[0])?,
            destination_format: parse_storage_format(record.parameters[1])?,
        },
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
        configure_each_run: record.flags & INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN != 0,
        target,
        kind,
    })
}

pub(super) fn parse_target(
    record: &InkpodBatchOperationInput,
) -> Result<Option<BatchTargetSelector>, u32> {
    if record.layer_id == 0
        && record.plane_id == 0
        && record.layer_kind == 0
        && record.plane_kind == 0
        && record.missing_policy == 0
    {
        return Ok(None);
    }
    Ok(Some(BatchTargetSelector {
        layer_id: (record.layer_id != 0).then_some(record.layer_id),
        plane_id: (record.plane_id != 0).then_some(record.plane_id),
        layer_kind: (record.layer_kind != 0)
            .then(|| parse_layer_kind(record.layer_kind))
            .transpose()?,
        plane_kind: (record.plane_kind != 0)
            .then(|| parse_plane_kind(i64::from(record.plane_kind)))
            .transpose()?,
        missing_policy: match record.missing_policy {
            INKPOD_BATCH_MISSING_SKIP => BatchMissingTargetPolicy::Skip,
            INKPOD_BATCH_MISSING_ERROR => BatchMissingTargetPolicy::Error,
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch missing-target policy is unknown",
                ));
            }
        },
    }))
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

pub(super) fn parameter_u32(value: i64, field: &str) -> Result<u32, u32> {
    u32::try_from(value).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} is outside u32 range"),
        )
    })
}

pub(super) fn parameter_bool(value: i64, field: &str) -> Result<bool, u32> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} is not boolean"),
        )),
    }
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
        INKPOD_LAYER_TEXT => Ok(LayerKind::Text),
        INKPOD_LAYER_ANNOTATION => Ok(LayerKind::Annotation),
        INKPOD_LAYER_VECTOR_COLORING => Ok(LayerKind::VectorColoring),
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
        Some(INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE) => Ok(PlaneType::VectorMainLine),
        Some(INKPOD_TYPED_PLANE_COLOR_TRACE) => Ok(PlaneType::ColorTrace),
        Some(INKPOD_TYPED_PLANE_VECTOR_FILL) => Ok(PlaneType::VectorFill),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch plane kind is unknown",
        )),
    }
}

pub(super) fn parse_storage_format(value: i64) -> Result<PixelFormat, u32> {
    match u32::try_from(value).ok() {
        Some(INKPOD_STORAGE_BINARY8) => Ok(PixelFormat::BinaryMask8),
        Some(INKPOD_STORAGE_GRAYSCALE8) => Ok(PixelFormat::Grayscale8),
        Some(INKPOD_STORAGE_GRAYSCALE16) => Ok(PixelFormat::Grayscale16),
        Some(INKPOD_STORAGE_RGBA8) => Ok(PixelFormat::StraightRgba8),
        Some(INKPOD_STORAGE_RGBA16) => Ok(PixelFormat::StraightRgba16),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch storage format is unknown",
        )),
    }
}

pub(super) fn pixel_to_rgba16(value: PixelValue) -> Result<[u16; 4], u32> {
    match value {
        PixelValue::Rgba(value) => Ok(value.map(|component| u16::from(component) * 257)),
        PixelValue::Rgba16(value) => Ok(value),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch boundary color must be RGBA8 or RGBA16",
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

pub(super) fn output_policy_value(value: BatchOutputPolicy) -> u32 {
    match value {
        BatchOutputPolicy::Duplicate => INKPOD_BATCH_OUTPUT_DUPLICATE,
        BatchOutputPolicy::NewSave => INKPOD_BATCH_OUTPUT_NEW_SAVE,
        BatchOutputPolicy::ExplicitOverwrite => INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE,
    }
}

pub(super) fn failure_policy_value(value: BatchFailurePolicy) -> u32 {
    match value {
        BatchFailurePolicy::Continue => INKPOD_BATCH_FAILURE_CONTINUE,
        BatchFailurePolicy::Stop => INKPOD_BATCH_FAILURE_STOP,
    }
}

pub(super) fn output_flags(value: &BatchOutputSettings) -> u64 {
    (if value.cell_folder {
        INKPOD_BATCH_OUTPUT_CELL_FOLDER
    } else {
        0
    }) | (if value.descending {
        INKPOD_BATCH_OUTPUT_DESCENDING
    } else {
        0
    }) | (if value.preview_before_save {
        INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE
    } else {
        0
    })
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
