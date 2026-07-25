use super::*;

pub(crate) fn parse_layer_kind(value: u32) -> Result<LayerKind, u32> {
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
            "layer kind is not defined",
        )),
    }
}

pub(crate) fn layer_kind_code(value: LayerKind) -> u32 {
    match value {
        LayerKind::BinaryColoring => INKPOD_LAYER_BINARY_COLORING,
        LayerKind::GrayscaleColoring => INKPOD_LAYER_GRAYSCALE_COLORING,
        LayerKind::Raster => INKPOD_LAYER_RASTER,
        LayerKind::Selection => INKPOD_LAYER_SELECTION,
        LayerKind::Frame => INKPOD_LAYER_FRAME,
        LayerKind::VanishingPoint => INKPOD_LAYER_VANISHING_POINT,
        LayerKind::Adjustment => INKPOD_LAYER_ADJUSTMENT,
        LayerKind::Text => INKPOD_LAYER_TEXT,
        LayerKind::Annotation => INKPOD_LAYER_ANNOTATION,
        LayerKind::VectorColoring => INKPOD_LAYER_VECTOR_COLORING,
    }
}

pub(crate) fn parse_plane_type(value: u32) -> Result<PlaneType, u32> {
    match value {
        INKPOD_TYPED_PLANE_MAIN_LINE => Ok(PlaneType::MainLine),
        INKPOD_TYPED_PLANE_COLOR => Ok(PlaneType::Color),
        INKPOD_TYPED_PLANE_RASTER => Ok(PlaneType::Raster),
        INKPOD_TYPED_PLANE_SELECTION => Ok(PlaneType::Selection),
        INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE => Ok(PlaneType::VectorMainLine),
        INKPOD_TYPED_PLANE_COLOR_TRACE => Ok(PlaneType::ColorTrace),
        INKPOD_TYPED_PLANE_VECTOR_FILL => Ok(PlaneType::VectorFill),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "typed plane kind is not defined",
        )),
    }
}

pub(crate) fn plane_type_code(value: PlaneType) -> u32 {
    match value {
        PlaneType::MainLine => INKPOD_TYPED_PLANE_MAIN_LINE,
        PlaneType::Color => INKPOD_TYPED_PLANE_COLOR,
        PlaneType::Raster => INKPOD_TYPED_PLANE_RASTER,
        PlaneType::Selection => INKPOD_TYPED_PLANE_SELECTION,
        PlaneType::VectorMainLine => INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE,
        PlaneType::ColorTrace => INKPOD_TYPED_PLANE_COLOR_TRACE,
        PlaneType::VectorFill => INKPOD_TYPED_PLANE_VECTOR_FILL,
    }
}

pub(crate) fn parse_storage_format(value: u32) -> Result<PixelFormat, u32> {
    match value {
        INKPOD_STORAGE_BINARY8 => Ok(PixelFormat::BinaryMask8),
        INKPOD_STORAGE_GRAYSCALE8 => Ok(PixelFormat::Grayscale8),
        INKPOD_STORAGE_GRAYSCALE16 => Ok(PixelFormat::Grayscale16),
        INKPOD_STORAGE_RGBA8 => Ok(PixelFormat::StraightRgba8),
        INKPOD_STORAGE_RGBA16 => Ok(PixelFormat::StraightRgba16),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "storage pixel format is not defined",
        )),
    }
}

pub(crate) fn parse_common_raster_format(value: u32) -> Result<CommonRasterFormat, u32> {
    match value {
        INKPOD_COMMON_RASTER_PNG => Ok(CommonRasterFormat::Png),
        INKPOD_COMMON_RASTER_TIFF => Ok(CommonRasterFormat::Tiff),
        INKPOD_COMMON_RASTER_TGA => Ok(CommonRasterFormat::Tga),
        INKPOD_COMMON_RASTER_BMP => Ok(CommonRasterFormat::Bmp),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "common raster format is not defined",
        )),
    }
}

pub(crate) fn storage_format_code(value: PixelFormat) -> u32 {
    match value {
        PixelFormat::BinaryMask8 => INKPOD_STORAGE_BINARY8,
        PixelFormat::Grayscale8 => INKPOD_STORAGE_GRAYSCALE8,
        PixelFormat::Grayscale16 => INKPOD_STORAGE_GRAYSCALE16,
        PixelFormat::StraightRgba8 => INKPOD_STORAGE_RGBA8,
        PixelFormat::StraightRgba16 => INKPOD_STORAGE_RGBA16,
        PixelFormat::PremultipliedBgra8 => 0,
    }
}

pub(crate) fn parse_selection_operation(value: u32) -> Result<SelectionOperation, u32> {
    match value {
        INKPOD_SELECTION_NEW => Ok(SelectionOperation::New),
        INKPOD_SELECTION_ADD => Ok(SelectionOperation::Add),
        INKPOD_SELECTION_SUBTRACT => Ok(SelectionOperation::Subtract),
        INKPOD_SELECTION_INTERSECT => Ok(SelectionOperation::Intersect),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "selection operation is not defined",
        )),
    }
}

pub(crate) fn parse_tool(value: u32) -> Result<PaintTool, u32> {
    match value {
        INKPOD_TOOL_PENCIL => Ok(PaintTool::Pencil),
        INKPOD_TOOL_BRUSH => Ok(PaintTool::Brush),
        INKPOD_TOOL_ERASER => Ok(PaintTool::Eraser),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "tool is not a defined paint tool",
        )),
    }
}

pub(crate) fn parse_coordinate_space(value: u32) -> Result<CoordinateSpace, u32> {
    match value {
        INKPOD_COORDINATE_SPACE_DOCUMENT => Ok(CoordinateSpace::Document),
        INKPOD_COORDINATE_SPACE_DEVICE => Ok(CoordinateSpace::Device),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "coordinate_space is not defined",
        )),
    }
}

// SAFETY: The caller must provide a readable, aligned strided span for every
// advertised record. Each record must expose at least its size prefix.
pub(crate) unsafe fn parse_stroke_samples(
    samples: *const InkpodStrokeSample,
    sample_count: u64,
    sample_stride_bytes: u64,
) -> Result<Vec<StrokeSample>, u32> {
    if sample_count == 0 || sample_count > MAX_STROKE_SAMPLE_COUNT {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke sample_count is outside bounds",
        ));
    }
    if samples.is_null() || !is_aligned(samples) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke samples are null or misaligned",
        ));
    }
    let sample_count = usize::try_from(sample_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke sample_count is not representable",
        )
    })?;
    let stride = match usize::try_from(sample_stride_bytes) {
        Ok(stride)
            if stride >= size_of::<InkpodStrokeSample>()
                && stride % align_of::<InkpodStrokeSample>() == 0 =>
        {
            stride
        }
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sample_stride_bytes is too small, misaligned, or not representable",
            ));
        }
    };
    let storage_bytes = sample_count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodStrokeSample>()));
    if storage_bytes.is_none_or(|bytes| bytes > isize::MAX as usize) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke sample storage size overflows",
        ));
    }

    let mut parsed = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        // SAFETY: The checked count/stride span is readable by contract.
        let sample_pointer = unsafe {
            samples
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodStrokeSample>()
        };
        // SAFETY: Each record exposes its readable size prefix.
        let sample_size = unsafe { validate_struct(sample_pointer, "InkpodStrokeSample") }?;
        if u64::from(sample_size) > sample_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodStrokeSample.struct_size exceeds sample_stride_bytes",
            ));
        }
        // SAFETY: The complete known record prefix is aligned and readable.
        let sample = unsafe { &*sample_pointer };
        if sample.flags != 0 || sample.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stroke sample contains unsupported flags or reserved values",
            ));
        }
        parsed.push(StrokeSample {
            x: sample.x,
            y: sample.y,
            pressure: sample.pressure,
        });
    }
    Ok(parsed)
}

// SAFETY: `input` must be a validated, complete public structure whose sample
// span satisfies the exported function contract.
pub(crate) unsafe fn parse_stroke_input(input: &InkpodStrokeInput) -> Result<Stroke, u32> {
    if input.flags & !(INKPOD_STROKE_FLAG_AUTO_ERASE | INKPOD_STROKE_FLAG_PRESSURE_SIZE) != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "stroke input contains unsupported flags",
        ));
    }
    // SAFETY: Forwarded from this helper's caller contract.
    let samples = unsafe {
        parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
    }?;
    let tool = parse_tool(input.tool)?;
    let plane = parse_plane(input.plane)?;
    let coordinate_space = parse_coordinate_space(input.coordinate_space)?;
    Ok(Stroke {
        tool,
        plane,
        color: [
            (input.color_rgba >> 24) as u8,
            (input.color_rgba >> 16) as u8,
            (input.color_rgba >> 8) as u8,
            input.color_rgba as u8,
        ],
        diameter: input.diameter,
        auto_erase: input.flags & INKPOD_STROKE_FLAG_AUTO_ERASE != 0,
        pressure_size: input.flags & INKPOD_STROKE_FLAG_PRESSURE_SIZE != 0,
        coordinate_space,
        samples,
    })
}

pub(crate) fn parse_effect_region_kind(shape: u32) -> Result<EffectRegionKind, u32> {
    match shape {
        INKPOD_SELECTION_TRACE => Ok(EffectRegionKind::Trace),
        INKPOD_SELECTION_RECTANGLE => Ok(EffectRegionKind::Rectangle),
        INKPOD_SELECTION_POLYLINE => Ok(EffectRegionKind::Polyline),
        INKPOD_SELECTION_LASSO => Ok(EffectRegionKind::Lasso),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "effect region must be pen, rectangle, polyline, or lasso",
        )),
    }
}

// SAFETY: `color` must expose a complete, readable InkpodColorValue prefix.
pub(crate) unsafe fn parse_color_value(color: *const InkpodColorValue) -> Result<PixelValue, u32> {
    // SAFETY: Forwarded from this helper's caller contract.
    unsafe { validate_struct(color, "InkpodColorValue") }?;
    // SAFETY: The complete known structure is readable after validation.
    let color = unsafe { &*color };
    match color.depth {
        INKPOD_COLOR_DEPTH_BINARY if color.red <= u16::from(u8::MAX) => {
            Ok(PixelValue::Binary(color.red as u8))
        }
        INKPOD_COLOR_DEPTH_GRAYSCALE_8 if color.red <= u16::from(u8::MAX) => {
            Ok(PixelValue::Grayscale8(color.red as u8))
        }
        INKPOD_COLOR_DEPTH_GRAYSCALE_16 => Ok(PixelValue::Grayscale16(color.red)),
        INKPOD_COLOR_DEPTH_8
            if [color.red, color.green, color.blue, color.alpha]
                .into_iter()
                .all(|channel| channel <= u16::from(u8::MAX)) =>
        {
            Ok(PixelValue::Rgba([
                color.red as u8,
                color.green as u8,
                color.blue as u8,
                color.alpha as u8,
            ]))
        }
        INKPOD_COLOR_DEPTH_16 => Ok(PixelValue::Rgba16([
            color.red,
            color.green,
            color.blue,
            color.alpha,
        ])),
        INKPOD_COLOR_DEPTH_BINARY | INKPOD_COLOR_DEPTH_GRAYSCALE_8
            if color.red > u16::from(u8::MAX) =>
        {
            Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "8-bit scalar color contains a value above 255",
            ))
        }
        INKPOD_COLOR_DEPTH_8 => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "8-bit color contains a channel above 255",
        )),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color depth is not 8 or 16 bits",
        )),
    }
}

pub(crate) unsafe fn parse_vector_path_input(
    input: &InkpodVectorPathInput,
) -> Result<VectorPathInput, u32> {
    if input.reserved != 0 || input.flags & !INKPOD_VECTOR_PATH_CLOSED != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "vector path input contains unsupported values",
        ));
    }
    let count = usize::try_from(input.segment_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector segment count is not representable",
        )
    })?;
    if count == 0 || count > 262_144 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector segment count is outside bounds",
        ));
    }
    let stride = usize::try_from(input.segment_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector segment stride is not representable",
        )
    })?;
    if input.segments.is_null()
        || !is_aligned(input.segments)
        || stride < size_of::<InkpodVectorCubicSegment>()
        || stride % align_of::<InkpodVectorCubicSegment>() != 0
        || count
            .checked_mul(stride)
            .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector segment span is null, misaligned, or outside bounds",
        ));
    }
    let mut segments = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The validated borrowed strided span contains this record.
        let pointer = unsafe {
            input
                .segments
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodVectorCubicSegment>()
        };
        // SAFETY: Every record exposes its readable size prefix.
        let size = unsafe { validate_struct(pointer, "InkpodVectorCubicSegment") }?;
        if u64::from(size) > input.segment_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "vector segment struct_size exceeds its stride",
            ));
        }
        // SAFETY: The complete known record is readable after validation.
        let segment = unsafe { &*pointer };
        if segment.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector segment reserved field is not zero",
            ));
        }
        let point = |value: InkpodVectorPoint| PointF32 {
            x: value.x,
            y: value.y,
        };
        segments.push(VectorCubicSegment {
            p0: point(segment.p0),
            p1: point(segment.p1),
            p2: point(segment.p2),
            p3: point(segment.p3),
            width_start: segment.width_start,
            width_end: segment.width_end,
        });
    }
    // SAFETY: The nested color record is a complete field of the validated input.
    let color = unsafe { parse_color_value(&raw const input.color) }?;
    Ok(VectorPathInput {
        segments,
        color,
        closed: input.flags & INKPOD_VECTOR_PATH_CLOSED != 0,
    })
}

pub(crate) fn parse_vector_erase_mode(value: u32) -> Result<VectorEraseMode, u32> {
    match value {
        INKPOD_VECTOR_ERASE_PARTIAL => Ok(VectorEraseMode::Partial),
        INKPOD_VECTOR_ERASE_TO_INTERSECTION => Ok(VectorEraseMode::ToIntersection),
        INKPOD_VECTOR_ERASE_WHOLE_PATH => Ok(VectorEraseMode::WholePath),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector erase mode is not defined",
        )),
    }
}

pub(crate) fn parse_vector_width_mode(value: u32, parameter: f32) -> Result<VectorWidthMode, u32> {
    match value {
        INKPOD_VECTOR_WIDTH_ADD => Ok(VectorWidthMode::Add(parameter)),
        INKPOD_VECTOR_WIDTH_SUBTRACT => Ok(VectorWidthMode::Subtract(parameter)),
        INKPOD_VECTOR_WIDTH_SCALE => Ok(VectorWidthMode::Scale(parameter)),
        INKPOD_VECTOR_WIDTH_CONSTANT => Ok(VectorWidthMode::Constant(parameter)),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector width mode is not defined",
        )),
    }
}

pub(crate) fn parse_vector_selection_mode(value: u32) -> Result<VectorSelectionMode, u32> {
    match value {
        INKPOD_VECTOR_SELECT_CUT_BY_SELECTION => Ok(VectorSelectionMode::CutBySelection),
        INKPOD_VECTOR_SELECT_TOUCHING => Ok(VectorSelectionMode::Touching),
        INKPOD_VECTOR_SELECT_FULLY_CONTAINED => Ok(VectorSelectionMode::FullyContained),
        INKPOD_VECTOR_SELECT_LINE => Ok(VectorSelectionMode::Line),
        INKPOD_VECTOR_SELECT_WHOLE_LINE => Ok(VectorSelectionMode::WholeLine),
        INKPOD_VECTOR_SELECT_TO_INTERSECTION => Ok(VectorSelectionMode::ToIntersection),
        INKPOD_VECTOR_SELECT_FILL_BOUNDARY => Ok(VectorSelectionMode::FillBoundary),
        INKPOD_VECTOR_SELECT_FILL => Ok(VectorSelectionMode::Fill),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector selection mode is not defined",
        )),
    }
}

pub(crate) fn write_color_value(
    output: &mut InkpodColorValue,
    color: PixelValue,
) -> Result<(), u32> {
    match color {
        PixelValue::Rgba(value) => {
            output.depth = INKPOD_COLOR_DEPTH_8;
            output.red = u16::from(value[0]);
            output.green = u16::from(value[1]);
            output.blue = u16::from(value[2]);
            output.alpha = u16::from(value[3]);
            Ok(())
        }
        PixelValue::Rgba16(value) => {
            output.depth = INKPOD_COLOR_DEPTH_16;
            output.red = value[0];
            output.green = value[1];
            output.blue = value[2];
            output.alpha = value[3];
            Ok(())
        }
        _ => Err(fail(
            INKPOD_STATUS_INVALID_STATE,
            "eyedropper returned a non-color value",
        )),
    }
}

pub(crate) fn clipboard_pixel_rgba8(color: PixelValue) -> [u8; 4] {
    match color {
        PixelValue::Binary(value) | PixelValue::Grayscale8(value) => [0, 0, 0, value],
        PixelValue::Grayscale16(value) => [0, 0, 0, (value / 257) as u8],
        PixelValue::Rgba(value) => value,
        PixelValue::Rgba16(value) => [
            (value[0] / 257) as u8,
            (value[1] / 257) as u8,
            (value[2] / 257) as u8,
            (value[3] / 257) as u8,
        ],
    }
}

pub(crate) fn color_value_record(color: PixelValue) -> Result<InkpodColorValue, u32> {
    let mut output = InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        ..InkpodColorValue::default()
    };
    write_color_value(&mut output, color)?;
    Ok(output)
}

// SAFETY: `input` and every advertised strided record must be complete and
// readable for this call.
pub(crate) unsafe fn parse_color_array(input: &InkpodColorArray) -> Result<Vec<PixelValue>, u32> {
    if input.reserved != 0 || input.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "color array contains unsupported flags or reserved values",
        ));
    }
    if input.color_count > MAX_PALETTE_COLOR_COUNT {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array count exceeds the bounded palette limit",
        ));
    }
    let count = usize::try_from(input.color_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array count is not representable",
        )
    })?;
    if count == 0 {
        if !input.colors.is_null() || input.color_stride_bytes != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "an empty color array must use a null pointer and zero stride",
            ));
        }
        return Ok(Vec::new());
    }
    if input.colors.is_null() || !is_aligned(input.colors) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array storage is null or misaligned",
        ));
    }
    let stride = usize::try_from(input.color_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array stride is not representable",
        )
    })?;
    if stride < size_of::<InkpodColorValue>() || stride % align_of::<InkpodColorValue>() != 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array stride is too small or misaligned",
        ));
    }
    let storage = count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodColorValue>()));
    if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array storage size overflows",
        ));
    }
    let mut colors = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The checked count/stride span is readable by contract.
        let pointer = unsafe {
            input
                .colors
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodColorValue>()
        };
        // SAFETY: Every record exposes a readable size prefix.
        let struct_size = unsafe { validate_struct(pointer, "InkpodColorValue") }?;
        if u64::from(struct_size) > input.color_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodColorValue.struct_size exceeds color array stride",
            ));
        }
        // SAFETY: The complete known record is readable after validation.
        colors.push(unsafe { parse_color_value(pointer) }?);
    }
    Ok(colors)
}
