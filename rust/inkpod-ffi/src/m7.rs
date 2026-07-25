use super::*;
use inkpod_core::{
    BatchColorPair, BatchFailurePolicy, BatchGraph, BatchInputKind, BatchInputSelector,
    BatchItemOutcome, BatchMissingTargetPolicy, BatchOperation, BatchOperationKind,
    BatchOutputPolicy, BatchOutputSettings, BatchRunOptions, BatchRunScope, BatchSeed,
    BatchSeparation, BatchTargetSelector, DocumentResize, LayerKind, MirrorAxis, PixelFormat,
    PlaneType, ResizeAnchor, RotateDirection, VectorWidthMode,
};
use std::path::PathBuf;

pub const INKPOD_BATCH_INPUT_FILE: u32 = 1;
pub const INKPOD_BATCH_INPUT_FOLDER: u32 = 2;
pub const INKPOD_BATCH_INPUT_CURRENT_SEQUENCE: u32 = 3;

pub const INKPOD_BATCH_OUTPUT_DUPLICATE: u32 = 1;
pub const INKPOD_BATCH_OUTPUT_NEW_SAVE: u32 = 2;
pub const INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE: u32 = 3;
pub const INKPOD_BATCH_FAILURE_CONTINUE: u32 = 1;
pub const INKPOD_BATCH_FAILURE_STOP: u32 = 2;
pub const INKPOD_BATCH_MISSING_SKIP: u32 = 1;
pub const INKPOD_BATCH_MISSING_ERROR: u32 = 2;

pub const INKPOD_BATCH_OPERATION_COLOR_REPLACE: u32 = 1;
pub const INKPOD_BATCH_OPERATION_CONTINUOUS_FILL: u32 = 2;
pub const INKPOD_BATCH_OPERATION_SEPARATION: u32 = 3;
pub const INKPOD_BATCH_OPERATION_VISIBILITY: u32 = 4;
pub const INKPOD_BATCH_OPERATION_LINE_WIDTH: u32 = 5;
pub const INKPOD_BATCH_OPERATION_FILTER: u32 = 6;
pub const INKPOD_BATCH_OPERATION_BOUNDARY_AIRBRUSH: u32 = 7;
pub const INKPOD_BATCH_OPERATION_DUST_REMOVAL: u32 = 8;
pub const INKPOD_BATCH_OPERATION_MIRROR: u32 = 9;
pub const INKPOD_BATCH_OPERATION_ROTATE_90: u32 = 10;
pub const INKPOD_BATCH_OPERATION_RESIZE: u32 = 11;
pub const INKPOD_BATCH_OPERATION_CONVERT_PLANE: u32 = 12;

pub const INKPOD_BATCH_OPERATION_ENABLED: u64 = 1;
pub const INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN: u64 = 1 << 1;
pub const INKPOD_BATCH_OUTPUT_CELL_FOLDER: u64 = 1;
pub const INKPOD_BATCH_OUTPUT_DESCENDING: u64 = 1 << 1;
pub const INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE: u64 = 1 << 2;
pub const INKPOD_BATCH_SEPARATION_INVERT: i64 = 1;
pub const INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR: u32 = 1;

pub const INKPOD_BATCH_SCOPE_CURRENT: u32 = 1;
pub const INKPOD_BATCH_SCOPE_ALL: u32 = 2;
pub const INKPOD_BATCH_RUN_DRY: u64 = 1;
pub const INKPOD_BATCH_RUN_PREVIEW_CONFIRMED: u64 = 1 << 1;

pub const INKPOD_BATCH_ITEM_SUCCEEDED: u32 = 1;
pub const INKPOD_BATCH_ITEM_SKIPPED: u32 = 2;
pub const INKPOD_BATCH_ITEM_FAILED: u32 = 3;
pub const INKPOD_BATCH_ITEM_CANCELLED: u32 = 4;
pub const INKPOD_BATCH_ITEM_DRY_RUN: u32 = 5;
pub const INKPOD_BATCH_PREVIEW_HAS_WARNING: u32 = 1;

const MAX_BATCH_INPUTS: usize = 16_384;
const MAX_BATCH_OPERATIONS: usize = 1_024;
const MAX_BATCH_PAIRS: usize = 4_096;
const MAX_BATCH_SEEDS: usize = 4_096;
const MAX_BATCH_TEXT_BYTES: u64 = 32_768;

#[repr(C)]
pub struct InkpodBatchInput {
    pub struct_size: u32,
    pub kind: u32,
    pub feature_flags: u64,
    pub path_utf8: *const u8,
    pub path_bytes: u64,
    pub first_cell: u32,
    pub last_cell: u32,
    pub reserved: u64,
}

#[repr(C)]
pub struct InkpodBatchColorPairInput {
    pub struct_size: u32,
    pub enabled: u32,
    pub reserved: u64,
    pub old_color: InkpodColorValue,
    pub new_color: InkpodColorValue,
}

#[repr(C)]
pub struct InkpodBatchSeedInput {
    pub struct_size: u32,
    pub flags: u32,
    pub x: u32,
    pub y: u32,
    pub tolerance: u32,
    pub gap_close: u32,
    pub reserved: u64,
    pub fill_color: InkpodColorValue,
    pub expected_color: InkpodColorValue,
}

#[repr(C)]
pub struct InkpodBatchOperationInput {
    pub struct_size: u32,
    pub version: u32,
    pub kind: u32,
    pub reserved: u32,
    pub flags: u64,
    pub layer_id: u64,
    pub plane_id: u64,
    pub layer_kind: u32,
    pub plane_kind: u32,
    pub missing_policy: u32,
    pub reserved_2: u32,
    pub parameters: [i64; 8],
    pub color_0: InkpodColorValue,
    pub color_1: InkpodColorValue,
    pub colors: InkpodColorArray,
    pub filter: *const InkpodFilterInput,
    pub color_pairs: *const InkpodBatchColorPairInput,
    pub color_pair_count: u64,
    pub color_pair_stride_bytes: u64,
    pub seeds: *const InkpodBatchSeedInput,
    pub seed_count: u64,
    pub seed_stride_bytes: u64,
    pub reserved_3: u64,
}

#[repr(C)]
pub struct InkpodBatchGraphInput {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
    pub inputs: *const InkpodBatchInput,
    pub input_count: u64,
    pub input_stride_bytes: u64,
    pub operations: *const InkpodBatchOperationInput,
    pub operation_count: u64,
    pub operation_stride_bytes: u64,
    pub output_policy: u32,
    pub failure_policy: u32,
    pub output_flags: u64,
    pub output_folder_utf8: *const u8,
    pub output_folder_bytes: u64,
    pub basename_utf8: *const u8,
    pub basename_bytes: u64,
    pub start_number: u32,
    pub wait_milliseconds: u32,
    pub reserved: u64,
}

#[repr(C)]
pub struct InkpodBatchGraphInfo {
    pub struct_size: u32,
    pub version: u32,
    pub input_count: u64,
    pub operation_count: u64,
    pub output_policy: u32,
    pub failure_policy: u32,
    pub output_flags: u64,
}

#[repr(C)]
pub struct InkpodBatchPreviewItem {
    pub struct_size: u32,
    pub flags: u32,
    pub input_name: *const u8,
    pub input_name_bytes: u64,
    pub output_path: *const u8,
    pub output_path_bytes: u64,
    pub warning: *const u8,
    pub warning_bytes: u64,
}

#[repr(C)]
pub struct InkpodBatchReportInfo {
    pub struct_size: u32,
    pub cancelled: u32,
    pub item_count: u64,
    pub failure_count: u64,
    pub reserved: u64,
}

#[repr(C)]
pub struct InkpodBatchReportItem {
    pub struct_size: u32,
    pub outcome: u32,
    pub input_name: *const u8,
    pub input_name_bytes: u64,
    pub output_path: *const u8,
    pub output_path_bytes: u64,
    pub message: *const u8,
    pub message_bytes: u64,
}

pub struct InkpodBatchGraph {
    graph: BatchGraph,
}

struct OwnedPreviewItem {
    input_name: Box<[u8]>,
    output_path: Box<[u8]>,
    warning: Box<[u8]>,
}

pub struct InkpodBatchPreview {
    items: Vec<OwnedPreviewItem>,
}

struct OwnedReportItem {
    outcome: u32,
    input_name: Box<[u8]>,
    output_path: Box<[u8]>,
    message: Box<[u8]>,
}

pub struct InkpodBatchReport {
    items: Vec<OwnedReportItem>,
    cancelled: bool,
}

unsafe fn utf8_text<'a>(
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

unsafe fn record_at<T>(
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

unsafe fn parse_graph_input(input: *const InkpodBatchGraphInput) -> Result<BatchGraph, u32> {
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
    let operation_count = usize::try_from(input.operation_count).map_err(|_| {
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
                input.operations,
                input.operation_count,
                input.operation_stride_bytes,
                index,
                MAX_BATCH_OPERATIONS,
                "InkpodBatchOperationInput",
            )
        }?;
        // SAFETY: record_at validated this complete record and every nested parser copies spans.
        operations.push(unsafe { parse_operation(&*pointer) }?);
    }
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

unsafe fn parse_operation(record: &InkpodBatchOperationInput) -> Result<BatchOperation, u32> {
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
                if seed.flags & !INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR != 0 || seed.reserved != 0 {
                    return Err(fail(
                        INKPOD_STATUS_UNSUPPORTED,
                        "batch fill seed contains unsupported fields",
                    ));
                }
                seeds.push(BatchSeed {
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

fn parse_target(record: &InkpodBatchOperationInput) -> Result<Option<BatchTargetSelector>, u32> {
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

fn checked_count(value: u64, maximum: usize, field: &str) -> Result<usize, u32> {
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

fn parameter_u32(value: i64, field: &str) -> Result<u32, u32> {
    u32::try_from(value).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} is outside u32 range"),
        )
    })
}

fn parameter_bool(value: i64, field: &str) -> Result<bool, u32> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{field} is not boolean"),
        )),
    }
}

fn parse_layer_kind(value: u32) -> Result<LayerKind, u32> {
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

fn parse_plane_kind(value: i64) -> Result<PlaneType, u32> {
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

fn parse_storage_format(value: i64) -> Result<PixelFormat, u32> {
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

fn pixel_to_rgba16(value: PixelValue) -> Result<[u16; 4], u32> {
    match value {
        PixelValue::Rgba(value) => Ok(value.map(|component| u16::from(component) * 257)),
        PixelValue::Rgba16(value) => Ok(value),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch boundary color must be RGBA8 or RGBA16",
        )),
    }
}

fn scope(value: u32) -> Result<BatchRunScope, u32> {
    match value {
        INKPOD_BATCH_SCOPE_CURRENT => Ok(BatchRunScope::Current),
        INKPOD_BATCH_SCOPE_ALL => Ok(BatchRunScope::All),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "batch run scope is unknown",
        )),
    }
}

fn output_policy_value(value: BatchOutputPolicy) -> u32 {
    match value {
        BatchOutputPolicy::Duplicate => INKPOD_BATCH_OUTPUT_DUPLICATE,
        BatchOutputPolicy::NewSave => INKPOD_BATCH_OUTPUT_NEW_SAVE,
        BatchOutputPolicy::ExplicitOverwrite => INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE,
    }
}

fn failure_policy_value(value: BatchFailurePolicy) -> u32 {
    match value {
        BatchFailurePolicy::Continue => INKPOD_BATCH_FAILURE_CONTINUE,
        BatchFailurePolicy::Stop => INKPOD_BATCH_FAILURE_STOP,
    }
}

fn output_flags(value: &BatchOutputSettings) -> u64 {
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

fn bytes_for_path(path: Option<PathBuf>) -> Box<[u8]> {
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_create(
    input: *const InkpodBatchGraphInput,
    out_graph: *mut *mut InkpodBatchGraph,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_graph.is_null() || !is_aligned(out_graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller supplies readable/writable owner storage.
        if !unsafe { out_graph.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch graph output already owns a handle",
            );
        }
        let graph = match unsafe { parse_graph_input(input) } {
            Ok(graph) => graph,
            Err(status) => return status,
        };
        if let Err(error) = graph.validate() {
            return map_core_error(error);
        }
        // SAFETY: A unique Rust owner is transferred to caller storage.
        unsafe { out_graph.write(Box::into_raw(Box::new(InkpodBatchGraph { graph }))) };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_load(
    path_utf8: *const u8,
    path_bytes: u64,
    out_graph: *mut *mut InkpodBatchGraph,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_graph.is_null() || !is_aligned(out_graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph owner pointer is null or misaligned",
            );
        }
        if !unsafe { out_graph.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch graph output already owns a handle",
            );
        }
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        match BatchGraph::load(path) {
            Ok(graph) => {
                unsafe { out_graph.write(Box::into_raw(Box::new(InkpodBatchGraph { graph }))) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_save(
    graph: *const InkpodBatchGraph,
    path_utf8: *const u8,
    path_bytes: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if graph.is_null() || !is_aligned(graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph is null or misaligned",
            );
        }
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        match unsafe { &*graph }.graph.save(path) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_get_info(
    graph: *const InkpodBatchGraph,
    out_info: *mut InkpodBatchGraphInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if graph.is_null() || !is_aligned(graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodBatchGraphInfo") }
        {
            return status;
        }
        let graph = &unsafe { &*graph }.graph;
        let output = unsafe { &mut *out_info };
        output.version = graph.version;
        output.input_count = graph.inputs.len() as u64;
        output.operation_count = graph.operations.len() as u64;
        output.output_policy = output_policy_value(graph.output.policy);
        output.failure_policy = failure_policy_value(graph.output.failure_policy);
        output.output_flags = output_flags(&graph.output);
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_graph_release(graph: *mut *mut InkpodBatchGraph) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if graph.is_null() || !is_aligned(graph) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph owner pointer is null or misaligned",
            );
        }
        let handle = unsafe { graph.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch graph handle is misaligned",
            );
        }
        unsafe { graph.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_batch_preview(
    core: *mut InkpodCore,
    graph: *const InkpodBatchGraph,
    run_scope: u32,
    out_preview: *mut *mut InkpodBatchPreview,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || graph.is_null()
            || !is_aligned(graph)
            || out_preview.is_null()
            || !is_aligned(out_preview)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview handle pointer is null or misaligned",
            );
        }
        if !unsafe { out_preview.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch preview output already owns a handle",
            );
        }
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let scope = match scope(run_scope) {
            Ok(scope) => scope,
            Err(status) => return status,
        };
        match core.core.batch_preview(&unsafe { &*graph }.graph, scope) {
            Ok(preview) => {
                let items = preview
                    .items
                    .into_iter()
                    .map(|item| OwnedPreviewItem {
                        input_name: item.input_name.into_bytes().into_boxed_slice(),
                        output_path: bytes_for_path(item.output_path),
                        warning: item.warnings.join("\n").into_bytes().into_boxed_slice(),
                    })
                    .collect();
                unsafe { out_preview.write(Box::into_raw(Box::new(InkpodBatchPreview { items }))) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_preview_count(
    preview: *const InkpodBatchPreview,
    out_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null()
            || !is_aligned(preview)
            || out_count.is_null()
            || !is_aligned(out_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview or count pointer is null or misaligned",
            );
        }
        unsafe { out_count.write((&*preview).items.len() as u64) };
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_preview_get(
    preview: *const InkpodBatchPreview,
    index: u64,
    out_item: *mut InkpodBatchPreviewItem,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_item.cast_const(), "InkpodBatchPreviewItem") }
        {
            return status;
        }
        let Ok(index) = usize::try_from(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview index is not representable",
            );
        };
        let Some(item) = unsafe { &*preview }.items.get(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_item };
        output.flags = if item.warning.is_empty() {
            0
        } else {
            INKPOD_BATCH_PREVIEW_HAS_WARNING
        };
        output.input_name = item.input_name.as_ptr();
        output.input_name_bytes = item.input_name.len() as u64;
        output.output_path = item.output_path.as_ptr();
        output.output_path_bytes = item.output_path.len() as u64;
        output.warning = item.warning.as_ptr();
        output.warning_bytes = item.warning.len() as u64;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_preview_release(
    preview: *mut *mut InkpodBatchPreview,
) -> u32 {
    // SAFETY: Forwarded from this exported ownership contract.
    unsafe { release_preview(preview) }
}

unsafe fn release_preview(preview: *mut *mut InkpodBatchPreview) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if preview.is_null() || !is_aligned(preview) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview owner pointer is null or misaligned",
            );
        }
        let handle = unsafe { preview.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch preview handle is misaligned",
            );
        }
        unsafe { preview.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_batch_execute(
    core: *mut InkpodCore,
    graph: *const InkpodBatchGraph,
    run_scope: u32,
    flags: u64,
    task: *mut InkpodM6Task,
    out_report: *mut *mut InkpodBatchReport,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || graph.is_null()
            || !is_aligned(graph)
            || task.is_null()
            || !is_aligned(task)
            || out_report.is_null()
            || !is_aligned(out_report)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch execute handle pointer is null or misaligned",
            );
        }
        if flags & !(INKPOD_BATCH_RUN_DRY | INKPOD_BATCH_RUN_PREVIEW_CONFIRMED) != 0 {
            return fail(INKPOD_STATUS_UNSUPPORTED, "batch run flags are unsupported");
        }
        if !unsafe { out_report.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "batch report output already owns a handle",
            );
        }
        let core = unsafe { &mut *core };
        let task = unsafe { &*task };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let scope = match scope(run_scope) {
            Ok(scope) => scope,
            Err(status) => return status,
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "batch task is not READY");
        }
        let result = core.core.batch_execute(
            &unsafe { &*graph }.graph,
            BatchRunOptions {
                scope,
                dry_run: flags & INKPOD_BATCH_RUN_DRY != 0,
                preview_confirmed: flags & INKPOD_BATCH_RUN_PREVIEW_CONFIRMED != 0,
            },
            |completed, total| task.progress(completed, total),
        );
        match result {
            Ok(report) => {
                let cancelled = report.cancelled;
                let items = report
                    .items
                    .into_iter()
                    .map(|item| OwnedReportItem {
                        outcome: match item.outcome {
                            BatchItemOutcome::Succeeded => INKPOD_BATCH_ITEM_SUCCEEDED,
                            BatchItemOutcome::Skipped => INKPOD_BATCH_ITEM_SKIPPED,
                            BatchItemOutcome::Failed => INKPOD_BATCH_ITEM_FAILED,
                            BatchItemOutcome::Cancelled => INKPOD_BATCH_ITEM_CANCELLED,
                            BatchItemOutcome::DryRun => INKPOD_BATCH_ITEM_DRY_RUN,
                        },
                        input_name: item.input_name.into_bytes().into_boxed_slice(),
                        output_path: bytes_for_path(item.output_path),
                        message: item.message.into_bytes().into_boxed_slice(),
                    })
                    .collect();
                unsafe {
                    out_report.write(Box::into_raw(Box::new(InkpodBatchReport {
                        items,
                        cancelled,
                    })))
                };
                let status = if cancelled {
                    INKPOD_STATUS_CANCELLED
                } else {
                    INKPOD_STATUS_OK
                };
                task.finish(status);
                status
            }
            Err(error) => {
                let status = map_core_error(error);
                task.finish(status);
                status
            }
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_report_get_info(
    report: *const InkpodBatchReport,
    out_info: *mut InkpodBatchReportInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if report.is_null() || !is_aligned(report) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodBatchReportInfo") }
        {
            return status;
        }
        let report = unsafe { &*report };
        let output = unsafe { &mut *out_info };
        output.cancelled = u32::from(report.cancelled);
        output.item_count = report.items.len() as u64;
        output.failure_count = report
            .items
            .iter()
            .filter(|item| item.outcome == INKPOD_BATCH_ITEM_FAILED)
            .count() as u64;
        output.reserved = 0;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_report_get(
    report: *const InkpodBatchReport,
    index: u64,
    out_item: *mut InkpodBatchReportItem,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if report.is_null() || !is_aligned(report) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_item.cast_const(), "InkpodBatchReportItem") }
        {
            return status;
        }
        let Ok(index) = usize::try_from(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report index is not representable",
            );
        };
        let Some(item) = unsafe { &*report }.items.get(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report index is outside bounds",
            );
        };
        let output = unsafe { &mut *out_item };
        output.outcome = item.outcome;
        output.input_name = item.input_name.as_ptr();
        output.input_name_bytes = item.input_name.len() as u64;
        output.output_path = item.output_path.as_ptr();
        output.output_path_bytes = item.output_path.len() as u64;
        output.message = item.message.as_ptr();
        output.message_bytes = item.message.len() as u64;
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_report_release(report: *mut *mut InkpodBatchReport) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if report.is_null() || !is_aligned(report) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report owner pointer is null or misaligned",
            );
        }
        let handle = unsafe { report.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch report handle is misaligned",
            );
        }
        unsafe { report.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_task_create(out_task: *mut *mut InkpodM6Task) -> u32 {
    // SAFETY: This is the same thread-safe task layout and ownership contract.
    unsafe { inkpod_m6_task_create(out_task) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_task_query(
    task: *const InkpodM6Task,
    out_info: *mut InkpodM6TaskInfo,
) -> u32 {
    // SAFETY: This is the same thread-safe task layout and query contract.
    unsafe { inkpod_m6_task_query(task, out_info) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_task_cancel(task: *mut InkpodM6Task) -> u32 {
    // SAFETY: This is the same thread-safe task layout and cancellation contract.
    unsafe { inkpod_m6_task_cancel(task) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_batch_task_release(task: *mut *mut InkpodM6Task) -> u32 {
    // SAFETY: This is the same thread-safe task layout and ownership contract.
    unsafe { inkpod_m6_task_release(task) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn color(value: [u16; 4]) -> InkpodColorValue {
        InkpodColorValue {
            struct_size: size_of::<InkpodColorValue>() as u32,
            depth: INKPOD_COLOR_DEPTH_8,
            red: value[0],
            green: value[1],
            blue: value[2],
            alpha: value[3],
        }
    }

    #[test]
    fn m7_graph_preview_dry_run_and_owned_report_cross_ffi() {
        let directory = std::env::temp_dir().join(format!(
            "inkpod-m7-ffi-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let folder = directory.to_string_lossy().into_owned();
        let name = b"ffi-batch";
        let basename = b"cell";
        let input = InkpodBatchInput {
            struct_size: size_of::<InkpodBatchInput>() as u32,
            kind: INKPOD_BATCH_INPUT_CURRENT_SEQUENCE,
            feature_flags: INKPOD_FEATURE_NONE,
            path_utf8: ptr::null(),
            path_bytes: 0,
            first_cell: 0,
            last_cell: 0,
            reserved: 0,
        };
        let pair = InkpodBatchColorPairInput {
            struct_size: size_of::<InkpodBatchColorPairInput>() as u32,
            enabled: 1,
            reserved: 0,
            old_color: color([0, 0, 0, 0]),
            new_color: color([255, 0, 0, 255]),
        };
        let operation = InkpodBatchOperationInput {
            struct_size: size_of::<InkpodBatchOperationInput>() as u32,
            version: 1,
            kind: INKPOD_BATCH_OPERATION_COLOR_REPLACE,
            reserved: 0,
            flags: INKPOD_BATCH_OPERATION_ENABLED,
            layer_id: 0,
            plane_id: 0,
            layer_kind: INKPOD_LAYER_BINARY_COLORING,
            plane_kind: INKPOD_TYPED_PLANE_COLOR,
            missing_policy: INKPOD_BATCH_MISSING_ERROR,
            reserved_2: 0,
            parameters: [0; 8],
            color_0: color([0, 0, 0, 0]),
            color_1: color([0, 0, 0, 0]),
            colors: InkpodColorArray {
                struct_size: size_of::<InkpodColorArray>() as u32,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
                colors: ptr::null(),
                color_count: 0,
                color_stride_bytes: 0,
            },
            filter: ptr::null(),
            color_pairs: &pair,
            color_pair_count: 1,
            color_pair_stride_bytes: size_of::<InkpodBatchColorPairInput>() as u64,
            seeds: ptr::null(),
            seed_count: 0,
            seed_stride_bytes: 0,
            reserved_3: 0,
        };
        let graph_input = InkpodBatchGraphInput {
            struct_size: size_of::<InkpodBatchGraphInput>() as u32,
            version: 1,
            feature_flags: INKPOD_FEATURE_NONE,
            name_utf8: name.as_ptr(),
            name_bytes: name.len() as u64,
            inputs: &input,
            input_count: 1,
            input_stride_bytes: size_of::<InkpodBatchInput>() as u64,
            operations: &operation,
            operation_count: 1,
            operation_stride_bytes: size_of::<InkpodBatchOperationInput>() as u64,
            output_policy: INKPOD_BATCH_OUTPUT_NEW_SAVE,
            failure_policy: INKPOD_BATCH_FAILURE_CONTINUE,
            output_flags: 0,
            output_folder_utf8: folder.as_ptr(),
            output_folder_bytes: folder.len() as u64,
            basename_utf8: basename.as_ptr(),
            basename_bytes: basename.len() as u64,
            start_number: 1,
            wait_milliseconds: 0,
            reserved: 0,
        };
        let mut graph = ptr::null_mut();
        assert_eq!(
            unsafe { inkpod_batch_graph_create(&graph_input, &mut graph) },
            INKPOD_STATUS_OK
        );
        let mut info = InkpodBatchGraphInfo {
            struct_size: size_of::<InkpodBatchGraphInfo>() as u32,
            version: 0,
            input_count: 0,
            operation_count: 0,
            output_policy: 0,
            failure_policy: 0,
            output_flags: 0,
        };
        assert_eq!(
            unsafe { inkpod_batch_graph_get_info(graph, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!((info.input_count, info.operation_count), (1, 1));

        let mut core = InkpodCore {
            owner_thread: thread::current().id(),
            core: Core::new(),
        };
        core.core.new_cell(2, 2, 96_000, 96_000).unwrap();
        let mut preview = ptr::null_mut();
        assert_eq!(
            unsafe {
                inkpod_core_batch_preview(&mut core, graph, INKPOD_BATCH_SCOPE_ALL, &mut preview)
            },
            INKPOD_STATUS_OK
        );
        let mut preview_count = 0;
        assert_eq!(
            unsafe { inkpod_batch_preview_count(preview, &mut preview_count) },
            INKPOD_STATUS_OK
        );
        assert_eq!(preview_count, 1);
        let mut preview_item = InkpodBatchPreviewItem {
            struct_size: size_of::<InkpodBatchPreviewItem>() as u32,
            flags: 0,
            input_name: ptr::null(),
            input_name_bytes: 0,
            output_path: ptr::null(),
            output_path_bytes: 0,
            warning: ptr::null(),
            warning_bytes: 0,
        };
        assert_eq!(
            unsafe { inkpod_batch_preview_get(preview, 0, &mut preview_item) },
            INKPOD_STATUS_OK
        );
        assert!(preview_item.input_name_bytes != 0);
        assert_eq!(
            unsafe { inkpod_batch_preview_release(&mut preview) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_batch_preview_release(&mut preview) },
            INKPOD_STATUS_OK
        );

        let mut task = ptr::null_mut();
        let mut report = ptr::null_mut();
        assert_eq!(
            unsafe { inkpod_batch_task_create(&mut task) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe {
                inkpod_core_batch_execute(
                    &mut core,
                    graph,
                    INKPOD_BATCH_SCOPE_ALL,
                    INKPOD_BATCH_RUN_DRY | INKPOD_BATCH_RUN_PREVIEW_CONFIRMED,
                    task,
                    &mut report,
                )
            },
            INKPOD_STATUS_OK
        );
        let mut report_info = InkpodBatchReportInfo {
            struct_size: size_of::<InkpodBatchReportInfo>() as u32,
            cancelled: 0,
            item_count: 0,
            failure_count: 0,
            reserved: u64::MAX,
        };
        assert_eq!(
            unsafe { inkpod_batch_report_get_info(report, &mut report_info) },
            INKPOD_STATUS_OK
        );
        assert_eq!((report_info.item_count, report_info.failure_count), (1, 0));
        let mut report_item = InkpodBatchReportItem {
            struct_size: size_of::<InkpodBatchReportItem>() as u32,
            outcome: 0,
            input_name: ptr::null(),
            input_name_bytes: 0,
            output_path: ptr::null(),
            output_path_bytes: 0,
            message: ptr::null(),
            message_bytes: 0,
        };
        assert_eq!(
            unsafe { inkpod_batch_report_get(report, 0, &mut report_item) },
            INKPOD_STATUS_OK
        );
        assert_eq!(report_item.outcome, INKPOD_BATCH_ITEM_DRY_RUN);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);

        let settings = directory.join("settings.inkbatch");
        let settings_text = settings.to_string_lossy();
        assert_eq!(
            unsafe {
                inkpod_batch_graph_save(
                    graph,
                    settings_text.as_bytes().as_ptr(),
                    settings_text.len() as u64,
                )
            },
            INKPOD_STATUS_OK
        );
        let mut reopened = ptr::null_mut();
        assert_eq!(
            unsafe {
                inkpod_batch_graph_load(
                    settings_text.as_bytes().as_ptr(),
                    settings_text.len() as u64,
                    &mut reopened,
                )
            },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_batch_report_release(&mut report) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_batch_task_release(&mut task) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_batch_graph_release(&mut reopened) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_batch_graph_release(&mut graph) },
            INKPOD_STATUS_OK
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn m7_ffi_rejects_short_graph_and_cancelled_task_is_idempotent() {
        #[repr(C, align(8))]
        struct Short {
            struct_size: u32,
        }
        let short = Short {
            struct_size: size_of::<Short>() as u32,
        };
        let mut graph = ptr::null_mut();
        assert_eq!(
            unsafe {
                inkpod_batch_graph_create(
                    (&raw const short).cast::<InkpodBatchGraphInput>(),
                    &mut graph,
                )
            },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert!(graph.is_null());

        let mut input_record = InkpodBatchInput {
            struct_size: size_of::<InkpodBatchInput>() as u32,
            kind: INKPOD_BATCH_INPUT_CURRENT_SEQUENCE,
            feature_flags: INKPOD_FEATURE_NONE,
            path_utf8: ptr::null(),
            path_bytes: 0,
            first_cell: 0,
            last_cell: 0,
            reserved: 0,
        };
        let oversized_stride = (isize::MAX as u64).saturating_add(1);
        assert_eq!(
            unsafe {
                record_at(
                    &input_record,
                    2,
                    oversized_stride,
                    0,
                    MAX_BATCH_INPUTS,
                    "InkpodBatchInput",
                )
            },
            Err(INKPOD_STATUS_INVALID_ARGUMENT)
        );
        input_record.struct_size = (size_of::<InkpodBatchInput>() + 8) as u32;
        assert_eq!(
            unsafe {
                record_at(
                    &input_record,
                    1,
                    size_of::<InkpodBatchInput>() as u64,
                    0,
                    MAX_BATCH_INPUTS,
                    "InkpodBatchInput",
                )
            },
            Err(INKPOD_STATUS_INCOMPATIBLE_ABI)
        );

        let filter_storage =
            vec![0_u8; size_of::<InkpodFilterInput>() + align_of::<InkpodFilterInput>()];
        let filter_offset = (0..align_of::<InkpodFilterInput>())
            .find(|offset| {
                (filter_storage.as_ptr() as usize + offset) % align_of::<InkpodFilterInput>() != 0
            })
            .unwrap();
        // SAFETY: The offset remains within filter_storage; the deliberately
        // misaligned pointer must be rejected before any record field is read.
        let misaligned_filter = unsafe {
            filter_storage
                .as_ptr()
                .add(filter_offset)
                .cast::<InkpodFilterInput>()
        };
        let filter_operation = InkpodBatchOperationInput {
            struct_size: size_of::<InkpodBatchOperationInput>() as u32,
            version: 1,
            kind: INKPOD_BATCH_OPERATION_FILTER,
            reserved: 0,
            flags: INKPOD_BATCH_OPERATION_ENABLED,
            layer_id: 0,
            plane_id: 0,
            layer_kind: INKPOD_LAYER_BINARY_COLORING,
            plane_kind: INKPOD_TYPED_PLANE_COLOR,
            missing_policy: INKPOD_BATCH_MISSING_ERROR,
            reserved_2: 0,
            parameters: [0; 8],
            color_0: color([0, 0, 0, 0]),
            color_1: color([0, 0, 0, 0]),
            colors: InkpodColorArray {
                struct_size: size_of::<InkpodColorArray>() as u32,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
                colors: ptr::null(),
                color_count: 0,
                color_stride_bytes: 0,
            },
            filter: misaligned_filter,
            color_pairs: ptr::null(),
            color_pair_count: 0,
            color_pair_stride_bytes: 0,
            seeds: ptr::null(),
            seed_count: 0,
            seed_stride_bytes: 0,
            reserved_3: 0,
        };
        assert_eq!(
            unsafe { parse_operation(&filter_operation) }.unwrap_err(),
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let mut task = ptr::null_mut();
        assert_eq!(
            unsafe { inkpod_batch_task_create(&mut task) },
            INKPOD_STATUS_OK
        );
        assert_eq!(unsafe { inkpod_batch_task_cancel(task) }, INKPOD_STATUS_OK);
        assert_eq!(unsafe { inkpod_batch_task_cancel(task) }, INKPOD_STATUS_OK);
        assert_eq!(
            unsafe { inkpod_batch_task_release(&mut task) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_batch_task_release(&mut task) },
            INKPOD_STATUS_OK
        );
    }
}
