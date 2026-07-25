use super::*;

// SAFETY: `input` and its optional color span must be complete and readable for
// this call. Every advertised strided record exposes its own size prefix.
pub(crate) unsafe fn parse_fill_input(input: &InkpodFillInput) -> Result<FillRequest, u32> {
    const SUPPORTED_FLAGS: u64 = INKPOD_FILL_FLAG_DETACHED_REGIONS
        | INKPOD_FILL_FLAG_OVERFLOW_ABORT
        | INKPOD_FILL_FLAG_TRANSPARENT_ONLY
        | INKPOD_FILL_FLAG_SELECTION_PRESENT
        | INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY
        | INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR
        | INKPOD_FILL_FLAG_DOCUMENT_SELECTION;
    if input.flags & !SUPPORTED_FLAGS != 0 || input.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "fill input contains unsupported flags or reserved values",
        ));
    }
    let operation = match input.operation {
        INKPOD_FILL_SEED => FillOperation::Seed,
        INKPOD_FILL_CLOSED_REGION => FillOperation::ClosedRegion,
        INKPOD_FILL_EXTENSION => FillOperation::Extend,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill operation is not defined",
            ));
        }
    };
    let inclusion_mode = match input.inclusion_mode {
        INKPOD_INCLUSION_NONE => InclusionMode::None,
        INKPOD_INCLUSION_SPECIFIED => InclusionMode::Specified,
        INKPOD_INCLUSION_EXCEPT_SPECIFIED => InclusionMode::ExceptSpecified,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion mode is not defined",
            ));
        }
    };
    let gap_close = u8::try_from(input.gap_close).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "fill gap-close value is not representable",
        )
    })?;
    // SAFETY: The embedded color resides inside the validated input.
    let color = unsafe { parse_color_value(ptr::addr_of!(input.color)) }?;
    if input.inclusion_color_count > 6 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "fill inclusion color count exceeds six",
        ));
    }
    let count = usize::try_from(input.inclusion_color_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "fill inclusion color count is not representable",
        )
    })?;
    let stride = if count == 0 {
        0
    } else {
        let stride = usize::try_from(input.inclusion_color_stride_bytes).map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion color stride is not representable",
            )
        })?;
        if stride < size_of::<InkpodColorValue>() || stride % align_of::<InkpodColorValue>() != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion color stride is too small or misaligned",
            ));
        }
        if input.inclusion_colors.is_null() || !is_aligned(input.inclusion_colors) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion colors are null or misaligned",
            ));
        }
        let storage = count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodColorValue>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion color storage size overflows",
            ));
        }
        stride
    };
    let mut inclusion_colors = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The checked strided record span is readable by contract.
        let color_pointer = unsafe {
            input
                .inclusion_colors
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodColorValue>()
        };
        // SAFETY: Each record exposes a readable size prefix and complete body.
        let struct_size = unsafe { validate_struct(color_pointer, "InkpodColorValue") }?;
        if u64::from(struct_size) > input.inclusion_color_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodColorValue.struct_size exceeds inclusion color stride",
            ));
        }
        // SAFETY: The record is complete and validated.
        inclusion_colors.push(unsafe { parse_color_value(color_pointer) }?);
    }
    let selection_present = input.flags & INKPOD_FILL_FLAG_SELECTION_PRESENT != 0;
    let selection = selection_present.then_some(RectI32 {
        x: input.selection.x,
        y: input.selection.y,
        width: input.selection.width,
        height: input.selection.height,
    });
    if !selection_present
        && (input.selection.x != 0
            || input.selection.y != 0
            || input.selection.width != 0
            || input.selection.height != 0)
    {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "fill selection fields require the selection-present flag",
        ));
    }
    Ok(FillRequest {
        operation,
        seed_x: input.seed_x,
        seed_y: input.seed_y,
        color,
        selection,
        use_document_selection: input.flags & INKPOD_FILL_FLAG_DOCUMENT_SELECTION != 0,
        tolerance: input.tolerance,
        detached_regions: input.flags & INKPOD_FILL_FLAG_DETACHED_REGIONS != 0,
        overflow_abort: input.flags & INKPOD_FILL_FLAG_OVERFLOW_ABORT != 0,
        gap_close,
        transparent_only: input.flags & INKPOD_FILL_FLAG_TRANSPARENT_ONLY != 0,
        inclusion_mode,
        inclusion_colors,
        extension_distance: input.extension_distance,
    })
}

// SAFETY: `pointer` must identify `length` readable bytes for this call.
pub(crate) unsafe fn path_from_utf8<'a>(pointer: *const u8, length: u64) -> Result<&'a Path, u32> {
    if pointer.is_null() || length == 0 || length > MAX_PATH_BYTES {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "UTF-8 path is null, empty, or exceeds the bounded length",
        ));
    }
    let length = usize::try_from(length).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "UTF-8 path length is not representable",
        )
    })?;
    // SAFETY: The exported-function contract requires this readable range.
    let bytes = unsafe { slice::from_raw_parts(pointer, length) };
    let text = std::str::from_utf8(bytes)
        .map_err(|_| fail(INKPOD_STATUS_INVALID_ARGUMENT, "path is not valid UTF-8"))?;
    if text.as_bytes().contains(&0) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "path contains an embedded NUL",
        ));
    }
    Ok(Path::new(text))
}

pub(crate) unsafe fn name_from_utf8<'a>(pointer: *const u8, length: u64) -> Result<&'a str, u32> {
    if length == 0 || length > MAX_NODE_NAME_BYTES || pointer.is_null() {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "node name pointer or length is invalid",
        ));
    }
    let length = usize::try_from(length).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "node name length is not representable",
        )
    })?;
    // SAFETY: The exported caller contract requires this complete range to be readable.
    let bytes = unsafe { slice::from_raw_parts(pointer, length) };
    let text = std::str::from_utf8(bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "node name is not valid UTF-8",
        )
    })?;
    if text.as_bytes().contains(&0) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "node name contains an embedded NUL",
        ));
    }
    Ok(text)
}

pub(crate) struct ParsedRasterSource {
    pub(crate) document_uuid: u128,
    pub(crate) source_revision: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixel_format: PixelFormat,
    pub(crate) dpi_x_milli: Option<u32>,
    pub(crate) dpi_y_milli: Option<u32>,
    pub(crate) reference_frame: RectI32,
    pub(crate) pixels: Vec<u8>,
}

// SAFETY: input and its advertised pixel rows must remain readable for this call.
pub(crate) unsafe fn parse_raster_source(
    input: &InkpodRasterSourceInput,
) -> Result<ParsedRasterSource, u32> {
    unsafe { validate_struct(input, "InkpodRasterSourceInput") }?;
    if input.flags != 0 || input.source_revision == 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "raster source flags or source revision is invalid",
        ));
    }
    let document_uuid =
        (u128::from(input.document_uuid_high) << 64) | u128::from(input.document_uuid_low);
    if document_uuid == 0
        || input.width == 0
        || input.height == 0
        || input.width > MAX_RASTER_DIMENSION
        || input.height > MAX_RASTER_DIMENSION
        || input.reference_frame.width <= 0
        || input.reference_frame.height <= 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "raster source identity or dimensions are invalid",
        ));
    }
    let pixel_format = parse_storage_format(input.pixel_format)?;
    if !matches!(
        pixel_format,
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
    ) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "raster source must use straight RGBA8 or RGBA16",
        ));
    }
    if input.dpi_x_milli == 0 || input.dpi_y_milli == 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "raster source DPI must be nonzero",
        ));
    }
    let row_bytes = usize::try_from(input.width)
        .ok()
        .and_then(|width| width.checked_mul(pixel_format.bytes_per_pixel()))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "raster source row length overflows",
            )
        })?;
    let stride = usize::try_from(input.row_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "raster source row stride is not representable",
        )
    })?;
    let height = input.height as usize;
    let required = height
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(row_bytes))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "raster source byte range overflows",
            )
        })?;
    if stride < row_bytes
        || input.pixels.is_null()
        || required > isize::MAX as usize
        || input.pixel_bytes < required as u64
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "raster source pointer, stride, or byte length is invalid",
        ));
    }
    let compact_length = row_bytes.checked_mul(height).ok_or_else(|| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "compact raster source length overflows",
        )
    })?;
    if compact_length > MAX_COMMON_RASTER_BYTES || required > MAX_COMMON_RASTER_BYTES {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "raster source byte length exceeds its bound",
        ));
    }
    let mut pixels = Vec::with_capacity(compact_length);
    for row in 0..height {
        // SAFETY: required validated the final readable byte of every row.
        let source = unsafe { input.pixels.add(row * stride) };
        // SAFETY: Each row advertises at least row_bytes readable bytes.
        pixels.extend_from_slice(unsafe { slice::from_raw_parts(source, row_bytes) });
    }
    Ok(ParsedRasterSource {
        document_uuid,
        source_revision: input.source_revision,
        width: input.width,
        height: input.height,
        pixel_format,
        dpi_x_milli: Some(input.dpi_x_milli),
        dpi_y_milli: Some(input.dpi_y_milli),
        reference_frame: RectI32 {
            x: input.reference_frame.x,
            y: input.reference_frame.y,
            width: input.reference_frame.width,
            height: input.reference_frame.height,
        },
        pixels,
    })
}

pub(crate) fn parse_sequence_direction(value: u32) -> Result<SequenceDirection, u32> {
    match value {
        INKPOD_SEQUENCE_PREVIOUS => Ok(SequenceDirection::Previous),
        INKPOD_SEQUENCE_NEXT => Ok(SequenceDirection::Next),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "sequence direction is not defined",
        )),
    }
}

pub(crate) fn write_motion_frame(output: &mut InkpodMotionFrame, frame: MotionFrame) {
    output.flags = (if frame.paused {
        INKPOD_MOTION_FRAME_PAUSED
    } else {
        0
    }) | if frame.include_selection {
        INKPOD_MOTION_FRAME_INCLUDE_SELECTION
    } else {
        0
    } | if frame.include_light_table {
        INKPOD_MOTION_FRAME_INCLUDE_LIGHT_TABLE
    } else {
        0
    };
    output.sequence_index = frame.sequence_index as u64;
    output.cell_number = frame.cell_number;
    output.thumbnail_width = frame.thumbnail.width;
    output.thumbnail_height = frame.thumbnail.height;
    output.reserved = 0;
    output.thumbnail_checksum = frame.thumbnail.checksum;
}
