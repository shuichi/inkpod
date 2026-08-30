//! Caller-owned C ABI projection and atomic updates for Core-owned EditorState.

use super::*;

fn editor_tool(code: u32) -> Result<EditorTool, u32> {
    let tool = match code {
        1 => EditorTool::Pencil,
        2 => EditorTool::Brush,
        3 => EditorTool::Eraser,
        1_001 => EditorTool::Fill,
        1_002 => EditorTool::Eyedropper,
        1_003 => EditorTool::BoxZoom,
        1_004 => EditorTool::GuideMove,
        1_005 => EditorTool::Selection,
        1_006 => EditorTool::FloatingTransform,
        1_007 => EditorTool::LightTableMove,
        1_008 => EditorTool::ColorReplace,
        1_009 => EditorTool::ShootingFrame,
        1_011 => EditorTool::GeometryLine,
        1_012 => EditorTool::GeometryCurve,
        1_013 => EditorTool::GeometryRectangle,
        1_014 => EditorTool::GeometryEllipse,
        1_015 => EditorTool::GeometryPolygon,
        1_016 => EditorTool::GeometryPolyline,
        1_101 => EditorTool::EffectGradient,
        1_102 => EditorTool::EffectAirbrush,
        1_103 => EditorTool::EffectBlur,
        1_104 => EditorTool::EffectStamp,
        1_105 => EditorTool::EffectDust,
        1_106 => EditorTool::EffectAlphaGradient,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "editor tool is unknown",
            ));
        }
    };
    Ok(tool)
}

const fn fill_operation_code(operation: FillOperation) -> u32 {
    match operation {
        FillOperation::Seed => INKPOD_FILL_SEED,
        FillOperation::ClosedRegion => INKPOD_FILL_CLOSED_REGION,
        FillOperation::Extend => INKPOD_FILL_EXTENSION,
    }
}

fn fill_operation(code: u32) -> Result<FillOperation, u32> {
    match code {
        INKPOD_FILL_SEED => Ok(FillOperation::Seed),
        INKPOD_FILL_CLOSED_REGION => Ok(FillOperation::ClosedRegion),
        INKPOD_FILL_EXTENSION => Ok(FillOperation::Extend),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "editor fill operation is unknown",
        )),
    }
}

const fn inclusion_mode_code(mode: InclusionMode) -> u32 {
    match mode {
        InclusionMode::None => INKPOD_INCLUSION_NONE,
        InclusionMode::Specified => INKPOD_INCLUSION_SPECIFIED,
        InclusionMode::ExceptSpecified => INKPOD_INCLUSION_EXCEPT_SPECIFIED,
    }
}

fn inclusion_mode(code: u32) -> Result<InclusionMode, u32> {
    match code {
        INKPOD_INCLUSION_NONE => Ok(InclusionMode::None),
        INKPOD_INCLUSION_SPECIFIED => Ok(InclusionMode::Specified),
        INKPOD_INCLUSION_EXCEPT_SPECIFIED => Ok(InclusionMode::ExceptSpecified),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "editor inclusion mode is unknown",
        )),
    }
}

const fn selection_shape_code(shape: EditorSelectionShape) -> u32 {
    match shape {
        EditorSelectionShape::Rectangle => INKPOD_SELECTION_RECTANGLE,
        EditorSelectionShape::Ellipse => INKPOD_SELECTION_ELLIPSE,
        EditorSelectionShape::Lasso => INKPOD_SELECTION_LASSO,
        EditorSelectionShape::Polyline => INKPOD_SELECTION_POLYLINE,
        EditorSelectionShape::Trace => INKPOD_SELECTION_TRACE,
        EditorSelectionShape::Wand => INKPOD_SELECTION_WAND,
    }
}

fn selection_shape(code: u32) -> Result<EditorSelectionShape, u32> {
    match code {
        INKPOD_SELECTION_RECTANGLE => Ok(EditorSelectionShape::Rectangle),
        INKPOD_SELECTION_ELLIPSE => Ok(EditorSelectionShape::Ellipse),
        INKPOD_SELECTION_LASSO => Ok(EditorSelectionShape::Lasso),
        INKPOD_SELECTION_POLYLINE => Ok(EditorSelectionShape::Polyline),
        INKPOD_SELECTION_TRACE => Ok(EditorSelectionShape::Trace),
        INKPOD_SELECTION_WAND => Ok(EditorSelectionShape::Wand),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "editor selection shape is unknown",
        )),
    }
}

const fn selection_operation_code(operation: SelectionOperation) -> u32 {
    match operation {
        SelectionOperation::New => INKPOD_SELECTION_NEW,
        SelectionOperation::Add => INKPOD_SELECTION_ADD,
        SelectionOperation::Subtract => INKPOD_SELECTION_SUBTRACT,
        SelectionOperation::Intersect => INKPOD_SELECTION_INTERSECT,
    }
}

fn selection_operation(code: u32) -> Result<SelectionOperation, u32> {
    match code {
        INKPOD_SELECTION_NEW => Ok(SelectionOperation::New),
        INKPOD_SELECTION_ADD => Ok(SelectionOperation::Add),
        INKPOD_SELECTION_SUBTRACT => Ok(SelectionOperation::Subtract),
        INKPOD_SELECTION_INTERSECT => Ok(SelectionOperation::Intersect),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "editor selection operation is unknown",
        )),
    }
}

const fn range_interpretation_code(value: RangeInterpretation) -> u32 {
    match value {
        RangeInterpretation::Normal => INKPOD_RANGE_NORMAL,
        RangeInterpretation::Tight => INKPOD_RANGE_TIGHT,
        RangeInterpretation::EnclosedInterior => INKPOD_RANGE_ENCLOSED_INTERIOR,
        RangeInterpretation::Drawing => INKPOD_RANGE_DRAWING,
        RangeInterpretation::Boundary => INKPOD_RANGE_BOUNDARY,
    }
}

fn range_interpretation(code: u32) -> Result<RangeInterpretation, u32> {
    match code {
        INKPOD_RANGE_NORMAL => Ok(RangeInterpretation::Normal),
        INKPOD_RANGE_TIGHT => Ok(RangeInterpretation::Tight),
        INKPOD_RANGE_ENCLOSED_INTERIOR => Ok(RangeInterpretation::EnclosedInterior),
        INKPOD_RANGE_DRAWING => Ok(RangeInterpretation::Drawing),
        INKPOD_RANGE_BOUNDARY => Ok(RangeInterpretation::Boundary),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "editor raster range interpretation is unknown",
        )),
    }
}

const fn trace_shape_code(value: TraceBrushShape) -> u32 {
    match value {
        TraceBrushShape::Round => INKPOD_TRACE_ROUND,
        TraceBrushShape::Square => INKPOD_TRACE_SQUARE,
    }
}

fn trace_shape(code: u32) -> Result<TraceBrushShape, u32> {
    match code {
        INKPOD_TRACE_ROUND => Ok(TraceBrushShape::Round),
        INKPOD_TRACE_SQUARE => Ok(TraceBrushShape::Square),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "editor trace brush shape is unknown",
        )),
    }
}

const fn brush_shape_code(value: BrushShape) -> u32 {
    match value {
        BrushShape::Round => INKPOD_BRUSH_ROUND,
        BrushShape::Square => INKPOD_BRUSH_SQUARE,
    }
}

const fn start_color_code(value: StartColorPredicate) -> u32 {
    match value {
        StartColorPredicate::Any => INKPOD_START_COLOR_ANY,
        StartColorPredicate::ExactNative => INKPOD_START_COLOR_EXACT_NATIVE,
    }
}

fn write_brush(brush: EditorBrushOptions) -> InkpodEditorBrushOptions {
    InkpodEditorBrushOptions {
        struct_size: size_of::<InkpodEditorBrushOptions>() as u32,
        shape: brush_shape_code(brush.shape),
        smoothing: brush.smoothing,
        reserved: 0,
        start_color: start_color_code(brush.start_color),
        reserved2: 0,
    }
}

fn parse_brush(input: &InkpodEditorBrushOptions) -> Result<EditorBrushOptions, u32> {
    if input.struct_size < size_of::<InkpodEditorBrushOptions>() as u32
        || input.reserved != 0
        || input.reserved2 != 0
        || input.smoothing > 1_000
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "editor brush options are malformed",
        ));
    }
    Ok(EditorBrushOptions {
        shape: parse_brush_shape(input.shape)?,
        smoothing: input.smoothing,
        start_color: parse_start_color_predicate(input.start_color)?,
    })
}

fn write_fill(output: &mut InkpodEditorFillOptions, fill: &EditorFillOptions) -> Result<(), u32> {
    if fill.inclusion_colors.len() > INKPOD_EDITOR_MAX_INCLUSION_COLORS {
        return Err(fail(
            INKPOD_STATUS_INVALID_STATE,
            "editor inclusion colors exceed the ABI bound",
        ));
    }
    *output = InkpodEditorFillOptions::default();
    output.struct_size = size_of::<InkpodEditorFillOptions>() as u32;
    output.operation = fill_operation_code(fill.operation);
    output.flags = (if fill.overflow_abort {
        INKPOD_EDITOR_FILL_OVERFLOW_ABORT
    } else {
        0
    }) | (if fill.detached_regions {
        INKPOD_EDITOR_FILL_DETACHED_REGIONS
    } else {
        0
    }) | (if fill.transparent_only {
        INKPOD_EDITOR_FILL_TRANSPARENT_ONLY
    } else {
        0
    }) | (if fill.use_document_selection {
        INKPOD_EDITOR_FILL_DOCUMENT_SELECTION
    } else {
        0
    }) | (if fill.light_table_boundary {
        INKPOD_EDITOR_FILL_LIGHT_TABLE_BOUNDARY
    } else {
        0
    }) | if fill.light_table_color {
        INKPOD_EDITOR_FILL_LIGHT_TABLE_COLOR
    } else {
        0
    };
    output.tolerance = fill.tolerance;
    output.gap_close = u16::from(fill.gap_close);
    output.inclusion_mode = inclusion_mode_code(fill.inclusion_mode);
    output.extension_distance = fill.extension_distance;
    output.inclusion_color_count = fill.inclusion_colors.len() as u32;
    for (destination, color) in output
        .inclusion_colors
        .iter_mut()
        .zip(&fill.inclusion_colors)
    {
        *destination = color_value_record(*color)?;
    }
    Ok(())
}

fn parse_fill(input: &InkpodEditorFillOptions) -> Result<EditorFillOptions, u32> {
    if input.struct_size < size_of::<InkpodEditorFillOptions>() as u32
        || input.flags & !INKPOD_EDITOR_FILL_FLAGS != 0
        || input.gap_close > u16::from(u8::MAX)
        || input.inclusion_color_count as usize > INKPOD_EDITOR_MAX_INCLUSION_COLORS
        || input.reserved != 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "editor fill options are malformed",
        ));
    }
    let mut colors = Vec::with_capacity(input.inclusion_color_count as usize);
    for color in &input.inclusion_colors[..input.inclusion_color_count as usize] {
        // SAFETY: The inline record is covered by the validated top-level range.
        colors.push(unsafe { parse_color_value(color) }?);
    }
    Ok(EditorFillOptions {
        operation: fill_operation(input.operation)?,
        tolerance: input.tolerance,
        gap_close: input.gap_close as u8,
        extension_distance: input.extension_distance,
        inclusion_mode: inclusion_mode(input.inclusion_mode)?,
        inclusion_colors: colors,
        overflow_abort: input.flags & INKPOD_EDITOR_FILL_OVERFLOW_ABORT != 0,
        detached_regions: input.flags & INKPOD_EDITOR_FILL_DETACHED_REGIONS != 0,
        transparent_only: input.flags & INKPOD_EDITOR_FILL_TRANSPARENT_ONLY != 0,
        use_document_selection: input.flags & INKPOD_EDITOR_FILL_DOCUMENT_SELECTION != 0,
        light_table_boundary: input.flags & INKPOD_EDITOR_FILL_LIGHT_TABLE_BOUNDARY != 0,
        light_table_color: input.flags & INKPOD_EDITOR_FILL_LIGHT_TABLE_COLOR != 0,
    })
}

fn write_editor_state(
    output: &mut InkpodEditorStateInfo,
    info: Option<&EditorStateInfo>,
    state: &EditorState,
) -> Result<(), u32> {
    *output = InkpodEditorStateInfo::default();
    output.struct_size = size_of::<InkpodEditorStateInfo>() as u32;
    output.feature_flags = INKPOD_FEATURE_NONE;
    if let Some(info) = info {
        output.editor_revision = info.revision.get();
        output.editor_digest.copy_from_slice(info.digest.as_bytes());
        if info.dirty {
            output.flags |= INKPOD_EDITOR_STATE_DIRTY;
        }
    }
    output.active_tool = state.active_tool as u32;
    if let Some(tool) = state.last_color_consuming_tool {
        output.flags |= INKPOD_EDITOR_STATE_HAS_LAST_COLOR_TOOL;
        output.last_color_consuming_tool = tool as u32;
    }
    if let Some(color) = state.current_color() {
        output.flags |= INKPOD_EDITOR_STATE_HAS_CURRENT_COLOR;
        output.current_color = color_value_record(color)?;
    } else {
        output.current_color.struct_size = size_of::<InkpodColorValue>() as u32;
    }
    output.current_diameter_q16 = state.current_diameter_q16();
    if let Some(target) = state.target {
        output.flags |= INKPOD_EDITOR_STATE_HAS_TARGET;
        output.active_layer_id = target.layer_id;
        output.active_plane_id = target.plane_id;
    }
    if let Some(cursor) = state.palette_cursor {
        output.flags |= INKPOD_EDITOR_STATE_HAS_PALETTE_CURSOR;
        output.palette_group = cursor.group;
        output.palette_index = cursor.index;
    }
    write_fill(&mut output.fill, &state.fill)?;
    output.selection = InkpodEditorSelectionOptions {
        struct_size: size_of::<InkpodEditorSelectionOptions>() as u32,
        shape: selection_shape_code(state.selection.shape),
        operation: selection_operation_code(state.selection.operation),
        reserved: 0,
        tolerance: state.selection.tolerance,
        gap_close: u16::from(state.selection.gap_close),
        reserved2: 0,
        diameter_q16: state.selection.diameter_q16,
        interpretation: range_interpretation_code(state.selection.interpretation),
        aspect_ratio_q16: state.selection.aspect_ratio_q16,
        construction_flags: (if state.selection.from_center {
            INKPOD_SELECTION_FROM_CENTER
        } else {
            0
        }) | (if state.selection.constrain_rotation_45 {
            INKPOD_SELECTION_CONSTRAIN_ROTATION_45
        } else {
            0
        }) | (if state.selection.trace_pressure_size {
            INKPOD_SELECTION_TRACE_PRESSURE_SIZE
        } else {
            0
        }) | (if state.selection.trace_screen_size {
            INKPOD_SELECTION_TRACE_SCREEN_SIZE
        } else {
            0
        }),
        rotation_turns: state.selection.rotation_turns,
        trace_shape: trace_shape_code(state.selection.trace_shape),
    };
    output.brush = write_brush(state.brush);
    Ok(())
}

fn parse_selection(input: &InkpodEditorSelectionOptions) -> Result<EditorSelectionOptions, u32> {
    if input.struct_size < size_of::<InkpodEditorSelectionOptions>() as u32
        || input.reserved != 0
        || input.reserved2 != 0
        || input.gap_close > u16::from(u8::MAX)
        || input.construction_flags & !INKPOD_SELECTION_CONSTRUCTION_FLAGS != 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "editor selection options are malformed",
        ));
    }
    Ok(EditorSelectionOptions {
        shape: selection_shape(input.shape)?,
        operation: selection_operation(input.operation)?,
        tolerance: input.tolerance,
        gap_close: input.gap_close as u8,
        diameter_q16: input.diameter_q16,
        interpretation: range_interpretation(input.interpretation)?,
        aspect_ratio_q16: input.aspect_ratio_q16,
        from_center: input.construction_flags & INKPOD_SELECTION_FROM_CENTER != 0,
        constrain_rotation_45: input.construction_flags & INKPOD_SELECTION_CONSTRAIN_ROTATION_45
            != 0,
        rotation_turns: input.rotation_turns,
        trace_shape: trace_shape(input.trace_shape)?,
        trace_pressure_size: input.construction_flags & INKPOD_SELECTION_TRACE_PRESSURE_SIZE != 0,
        trace_screen_size: input.construction_flags & INKPOD_SELECTION_TRACE_SCREEN_SIZE != 0,
    })
}

/// Copies immutable Rust-owned built-in defaults into caller-owned storage.
///
/// # Safety
/// `core` must be a live owner-thread handle and `output` a complete writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_editor_defaults(
    core: *mut InkpodCore,
    output: *mut InkpodEditorDefaults,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(output.cast_const(), "InkpodEditorDefaults") }
        {
            return status;
        }
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let defaults: EditorDefaults = core.core.editor_defaults();
        let mut value = InkpodEditorDefaults {
            struct_size: size_of::<InkpodEditorDefaults>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
            width: defaults.initial_document.width,
            height: defaults.initial_document.height,
            dpi_x_milli: defaults.initial_document.dpi_x_milli,
            dpi_y_milli: defaults.initial_document.dpi_y_milli,
            state: InkpodEditorStateInfo::default(),
        };
        if let Err(status) = write_editor_state(&mut value.state, None, &defaults.state) {
            return status;
        }
        unsafe { output.write(value) };
        INKPOD_STATUS_OK
    })
}

/// Copies the current session EditorState without changing any Core state.
///
/// # Safety
/// `core` must be a live owner-thread handle and `output` a complete writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_editor_state(
    core: *mut InkpodCore,
    output: *mut InkpodEditorStateInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodEditorStateInfo") }
        {
            return status;
        }
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let info = match core.core.editor_state() {
            Ok(info) => info,
            Err(error) => return map_core_error(error),
        };
        let mut value = InkpodEditorStateInfo::default();
        if let Err(status) = write_editor_state(&mut value, Some(&info), &info.state) {
            return status;
        }
        unsafe { output.write(value) };
        INKPOD_STATUS_OK
    })
}

/// Applies one typed EditorState update against an exact base EditorRevision.
///
/// # Safety
/// All pointers must name complete nonoverlapping records and `core` must be used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_update_editor_state(
    core: *mut InkpodCore,
    input: *const InkpodEditorStateUpdate,
    output: *mut InkpodEditorStateInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodEditorStateUpdate") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodEditorStateInfo") }
        {
            return status;
        }
        let input = unsafe { &*input };
        if input.reserved != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "editor update reserved field is nonzero",
            );
        }
        let update = match input.kind {
            INKPOD_EDITOR_UPDATE_ACTIVE_TOOL if input.flags == 0 => {
                editor_tool(input.tool).map(EditorStateUpdate::SetActiveTool)
            }
            INKPOD_EDITOR_UPDATE_TOOL_COLOR if input.flags == 0 => editor_tool(input.tool)
                .and_then(|tool| {
                    let color = unsafe { parse_color_value(&input.color) }?;
                    Ok(EditorStateUpdate::SetToolColor { tool, color })
                }),
            INKPOD_EDITOR_UPDATE_TOOL_DIAMETER if input.flags == 0 => {
                editor_tool(input.tool).map(|tool| EditorStateUpdate::SetToolDiameter {
                    tool,
                    diameter_q16: input.diameter_q16,
                })
            }
            INKPOD_EDITOR_UPDATE_BRUSH_OPTIONS if input.flags == 0 => {
                parse_brush(&input.brush).map(EditorStateUpdate::SetBrushOptions)
            }
            INKPOD_EDITOR_UPDATE_FILL_OPTIONS if input.flags == 0 => {
                parse_fill(&input.fill).map(EditorStateUpdate::SetFillOptions)
            }
            INKPOD_EDITOR_UPDATE_SELECTION_OPTIONS if input.flags == 0 => {
                parse_selection(&input.selection).map(EditorStateUpdate::SetSelectionOptions)
            }
            INKPOD_EDITOR_UPDATE_ACTIVE_TARGET if input.flags == 0 => Ok(
                EditorStateUpdate::SetActiveTarget(inkpod_core::EditorTarget {
                    layer_id: input.active_layer_id,
                    plane_id: input.active_plane_id,
                }),
            ),
            INKPOD_EDITOR_UPDATE_PALETTE_CURSOR
                if input.flags & !INKPOD_EDITOR_UPDATE_PALETTE_CURSOR_PRESENT == 0 =>
            {
                Ok(EditorStateUpdate::SetPaletteCursor(
                    (input.flags & INKPOD_EDITOR_UPDATE_PALETTE_CURSOR_PRESENT != 0).then_some(
                        PaletteCursor {
                            group: input.palette_group,
                            index: input.palette_index,
                        },
                    ),
                ))
            }
            _ => Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "editor update kind or flags are unknown",
            )),
        };
        let update = match update {
            Ok(update) => update,
            Err(status) => return status,
        };
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let base_revision = match core.core.editor_state() {
            Ok(info) if info.revision.get() == input.expected_editor_revision => info.revision,
            Ok(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_STATE,
                    "editor state base revision is stale",
                );
            }
            Err(error) => return map_core_error(error),
        };
        let info = match core.core.update_editor_state(base_revision, update) {
            Ok(info) => info,
            Err(error) => return map_core_error(error),
        };
        let mut value = InkpodEditorStateInfo::default();
        if let Err(status) = write_editor_state(&mut value, Some(&info), &info.state) {
            return status;
        }
        unsafe { output.write(value) };
        INKPOD_STATUS_OK
    })
}

fn edit_target_record(target: EditTarget) -> InkpodEditTarget {
    let (kind, layer_id, plane_id) = match target {
        EditTarget::Layer(layer_id) => (INKPOD_EDIT_TARGET_LAYER, layer_id, 0),
        EditTarget::Plane(target) => (INKPOD_EDIT_TARGET_PLANE, target.layer_id, target.plane_id),
    };
    InkpodEditTarget {
        struct_size: size_of::<InkpodEditTarget>() as u32,
        kind,
        layer_id,
        plane_id,
        reserved: 0,
    }
}

unsafe fn parse_edit_targets(
    records: *const InkpodEditTarget,
    count: u64,
    stride_bytes: u64,
) -> Result<Vec<EditTarget>, u32> {
    let count = usize::try_from(count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "edit-target count is not representable",
        )
    })?;
    if count > inkpod_core::MAX_EDIT_TARGETS {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "edit-target count exceeds the Core limit",
        ));
    }
    if count == 0 {
        if !records.is_null() || stride_bytes != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "an empty edit-target span must use null storage and zero stride",
            ));
        }
        return Ok(Vec::new());
    }
    let stride = usize::try_from(stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "edit-target stride is not representable",
        )
    })?;
    if records.is_null()
        || !is_aligned(records)
        || stride < size_of::<InkpodEditTarget>()
        || stride % align_of::<InkpodEditTarget>() != 0
        || count.checked_mul(stride).is_none()
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "edit-target pointer, count, or stride is invalid",
        ));
    }
    let mut targets = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The caller contract and checked count/stride cover this record.
        let record = unsafe {
            &*(records
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodEditTarget>())
        };
        if record.struct_size < size_of::<InkpodEditTarget>() as u32
            || u64::from(record.struct_size) > stride_bytes
            || record.reserved != 0
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "edit-target record is incomplete or malformed",
            ));
        }
        targets.push(match (record.kind, record.plane_id) {
            (INKPOD_EDIT_TARGET_LAYER, 0) => EditTarget::Layer(record.layer_id),
            (INKPOD_EDIT_TARGET_PLANE, plane_id) if plane_id != 0 => {
                EditTarget::Plane(EditorTarget {
                    layer_id: record.layer_id,
                    plane_id,
                })
            }
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "edit-target kind or IDs are invalid",
                ));
            }
        });
    }
    Ok(targets)
}

unsafe fn write_edit_targets(
    targets: &[EditTarget],
    records: *mut InkpodEditTarget,
    capacity: u64,
    stride_bytes: u64,
    out_count: *mut u64,
) -> u32 {
    if out_count.is_null() || !is_aligned(out_count) {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "edit-target count output is null or misaligned",
        );
    }
    // SAFETY: The caller provides writable count storage.
    unsafe { out_count.write(targets.len() as u64) };
    if capacity == 0 {
        if !records.is_null() || stride_bytes != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "zero edit-target capacity requires null storage and zero stride",
            );
        }
        return if targets.is_empty() {
            INKPOD_STATUS_OK
        } else {
            INKPOD_STATUS_BUFFER_TOO_SMALL
        };
    }
    let capacity = match usize::try_from(capacity) {
        Ok(capacity) => capacity,
        Err(_) => {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "edit-target capacity is not representable",
            );
        }
    };
    let stride = match usize::try_from(stride_bytes) {
        Ok(stride)
            if stride >= size_of::<InkpodEditTarget>()
                && stride % align_of::<InkpodEditTarget>() == 0 =>
        {
            stride
        }
        _ => {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "edit-target output stride is too small or misaligned",
            );
        }
    };
    if records.is_null() || !is_aligned(records) || capacity.checked_mul(stride).is_none() {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "edit-target output storage is invalid",
        );
    }
    if capacity < targets.len() {
        return INKPOD_STATUS_BUFFER_TOO_SMALL;
    }
    for (index, target) in targets.iter().enumerate() {
        let value = edit_target_record(*target);
        // SAFETY: Checked writable strided storage covers each output record.
        unsafe {
            records
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodEditTarget>()
                .write(value)
        };
    }
    INKPOD_STATUS_OK
}

/// Copies the persisted grouped edit-target set through caller-owned strided storage.
///
/// # Safety
/// `core` must be a live owner-thread handle and `out_count` writable. When
/// `capacity` is nonzero, `records` must name writable strided storage covering
/// that many complete records; zero capacity requires a null record pointer and
/// zero stride. The storage is borrowed only for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_edit_targets(
    core: *mut InkpodCore,
    records: *mut InkpodEditTarget,
    capacity: u64,
    stride_bytes: u64,
    out_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let targets = match core.core.editor_state() {
            Ok(info) => info.state.edit_targets,
            Err(error) => return map_core_error(error),
        };
        unsafe { write_edit_targets(&targets, records, capacity, stride_bytes, out_count) }
    })
}

/// Copies the side-effect-free capability matrix for the effective target set.
///
/// # Safety
/// `core` must be a live owner-thread handle and `output` a complete writable
/// record that does not overlap the Core handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_edit_target_capabilities(
    core: *mut InkpodCore,
    output: *mut InkpodEditTargetCapabilities,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodEditTargetCapabilities") }
        {
            return status;
        }
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let capabilities = match core.core.edit_target_capabilities() {
            Ok(capabilities) => capabilities,
            Err(error) => return map_core_error(error),
        };
        unsafe {
            output.write(InkpodEditTargetCapabilities {
                struct_size: size_of::<InkpodEditTargetCapabilities>() as u32,
                can_duplicate: u32::from(capabilities.duplicate),
                can_delete: u32::from(capabilities.delete),
                can_set_visibility: u32::from(capabilities.visibility),
                can_set_editability: u32::from(capabilities.editability),
                can_merge: u32::from(capabilities.merge),
                can_convert_planes: u32::from(capabilities.convert_planes),
                reserved: 0,
            })
        };
        INKPOD_STATUS_OK
    })
}

/// Replaces grouped edit targets against one exact EditorRevision.
///
/// # Safety
/// `core` must be a live owner-thread handle and `output` a complete writable
/// record. For a nonempty input, `records` must name readable strided storage
/// covering `count` complete records; an empty input requires a null pointer and
/// zero stride. All storage is borrowed only for this call and must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_edit_targets(
    core: *mut InkpodCore,
    expected_editor_revision: u64,
    records: *const InkpodEditTarget,
    count: u64,
    stride_bytes: u64,
    output: *mut InkpodEditorStateInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodEditorStateInfo") }
        {
            return status;
        }
        let targets = match unsafe { parse_edit_targets(records, count, stride_bytes) } {
            Ok(targets) => targets,
            Err(status) => return status,
        };
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let base_revision = match core.core.editor_state() {
            Ok(info) if info.revision.get() == expected_editor_revision => info.revision,
            Ok(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_STATE,
                    "editor state base revision is stale",
                );
            }
            Err(error) => return map_core_error(error),
        };
        let info = match core
            .core
            .update_editor_state(base_revision, EditorStateUpdate::SetEditTargets(targets))
        {
            Ok(info) => info,
            Err(error) => return map_core_error(error),
        };
        let mut value = InkpodEditorStateInfo::default();
        if let Err(status) = write_editor_state(&mut value, Some(&info), &info.state) {
            return status;
        }
        unsafe { output.write(value) };
        INKPOD_STATUS_OK
    })
}

/// Applies one grouped target command and returns any tree-ordered output targets.
///
/// # Safety
/// `core` must be a live owner-thread handle; `input`, `result`, and
/// `out_output_count` must name complete nonoverlapping records. When output
/// capacity is nonzero, `output_targets` must name writable strided storage
/// covering that many complete records; zero capacity requires a null pointer
/// and zero stride. All caller storage is borrowed only for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_edit_target_command(
    core: *mut InkpodCore,
    input: *const InkpodEditTargetCommand,
    result: *mut InkpodDispatchResult,
    output_targets: *mut InkpodEditTarget,
    output_capacity: u64,
    output_stride_bytes: u64,
    out_output_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodEditTargetCommand") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let input = unsafe { &*input };
        if input.reserved != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "edit-target command reserved field is nonzero",
            );
        }
        let command = match input.operation {
            INKPOD_EDIT_TARGET_DUPLICATE if input.flags == 0 => EditTargetCommand::Duplicate,
            INKPOD_EDIT_TARGET_DELETE if input.flags == 0 => EditTargetCommand::Delete,
            INKPOD_EDIT_TARGET_SET_VISIBILITY if input.flags <= 1 => {
                EditTargetCommand::SetVisibility(input.flags != 0)
            }
            INKPOD_EDIT_TARGET_SET_EDITABILITY if input.flags <= 1 => {
                EditTargetCommand::SetEditability(input.flags != 0)
            }
            INKPOD_EDIT_TARGET_CONVERT_PLANES if input.flags == 0 => {
                let format = match parse_storage_format(input.pixel_format) {
                    Ok(format) => format,
                    Err(status) => return status,
                };
                EditTargetCommand::ConvertPlanes { format }
            }
            INKPOD_EDIT_TARGET_MERGE if input.flags == 0 => EditTargetCommand::Merge,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "edit-target command is malformed",
                );
            }
        };
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let required = match command {
            EditTargetCommand::Duplicate => match core.core.editor_state() {
                Ok(info) if info.state.edit_targets.is_empty() => 1,
                Ok(info) => info.state.edit_targets.len(),
                Err(error) => return map_core_error(error),
            },
            EditTargetCommand::Merge => 1,
            _ => 0,
        };
        if output_capacity < required as u64 {
            let placeholders = vec![EditTarget::Layer(0); required];
            return unsafe {
                write_edit_targets(
                    &placeholders,
                    output_targets,
                    output_capacity,
                    output_stride_bytes,
                    out_output_count,
                )
            };
        }
        let command_result = match core.core.apply_edit_target_command(command) {
            Ok(result) => result,
            Err(error) => return map_core_error(error),
        };
        let write_status = unsafe {
            write_edit_targets(
                &command_result.output_targets,
                output_targets,
                output_capacity,
                output_stride_bytes,
                out_output_count,
            )
        };
        if write_status != INKPOD_STATUS_OK {
            return write_status;
        }
        write_dispatch_result(unsafe { &mut *result }, command_result.dispatch);
        INKPOD_STATUS_OK
    })
}

/// Starts a transient raster stroke from exact values captured from Core-owned document/editor state.
///
/// MainLine targets use the document main-line color; Color/Raster targets use
/// the selected tool's independently retained paint color.
///
/// # Safety
/// `core` and `input` must be live owner-thread objects. The strided sample span
/// is borrowed only for this call and is copied before the function returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_editor_stroke_begin(
    core: *mut InkpodCore,
    input: *const InkpodEditorStrokeInput,
) -> u32 {
    unsafe { inkpod_core_editor_stroke_begin_for_view(core, 0, input) }
}

/// Starts a transient raster stroke through a primary or secondary Core view.
///
/// `view_id == 0` selects the primary view. Device-coordinate samples use the
/// selected view captured at begin for the entire stroke.
///
/// # Safety
/// `core` and `input` must be live owner-thread objects. The strided sample span
/// is borrowed only for this call and is copied before the function returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_editor_stroke_begin_for_view(
    core: *mut InkpodCore,
    view_id: u64,
    input: *const InkpodEditorStrokeInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodEditorStrokeInput") } {
            return status;
        }
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let input = unsafe { &*input };
        if input.reserved != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "editor stroke reserved field is nonzero",
            );
        }
        if input.flags & !(INKPOD_STROKE_FLAG_AUTO_ERASE | INKPOD_STROKE_FLAG_PRESSURE_SIZE) != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "editor stroke input contains unsupported flags",
            );
        }
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(samples) => samples,
            Err(status) => return status,
        };
        let editor_input = EditorStrokeInput {
            tool: if input.tool == 0 {
                None
            } else {
                match editor_tool(input.tool) {
                    Ok(tool) => Some(tool),
                    Err(status) => return status,
                }
            },
            coordinate_space,
            auto_erase: input.flags & INKPOD_STROKE_FLAG_AUTO_ERASE != 0,
            pressure_size: input.flags & INKPOD_STROKE_FLAG_PRESSURE_SIZE != 0,
            samples,
        };
        match core
            .core
            .begin_editor_stroke_for_view(view_id, &editor_input)
        {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}
