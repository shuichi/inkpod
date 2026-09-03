//! Domain-separated canonical EditorState and target EDIT frame encoding.

use super::model::*;
use crate::{
    BrushShape, CoreError, FillOperation, InclusionMode, PixelValue, RangeInterpretation,
    SelectionOperation, StartColorPredicate, TraceBrushShape,
};
use std::collections::BTreeMap;

const FRAME_SCHEMA: u32 = 7;
const STATE_FIELD_COUNT: usize = 13;
const EDIT_FIELD_COUNT: usize = 4;
const DIGEST_CONTEXT: &str = "org.inkpod.digest.editor-state.v2";
const MAX_DIAMETER_Q16: i64 = 256_i64 << 16;
const MAX_SELECTION_DIAMETER_Q16: i64 = 4_096_i64 << 16;
const MAX_INCLUSION_COLORS: usize = 6;
const MAX_EDIT_FRAME_BYTES: usize = 4 * 1_024 * 1_024;

pub(crate) struct DecodedEditFrame {
    pub(crate) revision: EditorRevision,
    pub(crate) state: EditorState,
    pub(crate) digest: EditorStateDigest,
}

pub(crate) fn state_digest(state: &EditorState) -> EditorStateDigest {
    let frame = encode_state_frame(state);
    EditorStateDigest(blake3::derive_key(DIGEST_CONTEXT, &frame))
}

pub(crate) fn encode_edit_frame(session: &EditorSessionState) -> Vec<u8> {
    let state_frame = encode_state_frame(&session.state);
    encode_frame(&[
        Some(FRAME_SCHEMA.to_le_bytes().to_vec()),
        Some(session.revision.get().to_le_bytes().to_vec()),
        Some(state_frame),
        Some(session.digest.0.to_vec()),
    ])
}

pub(crate) fn decode_edit_frame(bytes: &[u8]) -> Result<DecodedEditFrame, CoreError> {
    if bytes.len() > MAX_EDIT_FRAME_BYTES {
        return malformed("EDIT frame exceeds 4 MiB");
    }
    let fields = decode_frame(bytes, EDIT_FIELD_COUNT)?;
    if read_u32(required(fields[0])?)? != FRAME_SCHEMA {
        return malformed("unsupported EDIT schema");
    }
    let revision = read_u64(required(fields[1])?)?;
    if revision == 0 {
        return malformed("zero editor revision");
    }
    let state = decode_state_frame(required(fields[2])?)?;
    let stored_digest = required(fields[3])?;
    if stored_digest.len() != 32 {
        return malformed("editor digest length");
    }
    let mut digest_bytes = [0_u8; 32];
    digest_bytes.copy_from_slice(stored_digest);
    let digest = EditorStateDigest(digest_bytes);
    if digest != state_digest(&state) {
        return malformed("editor digest mismatch");
    }
    Ok(DecodedEditFrame {
        revision: EditorRevision::from_raw(revision),
        state,
        digest,
    })
}

pub(crate) fn validate_state(state: &EditorState) -> Result<(), CoreError> {
    if state.edit_targets.len() > MAX_EDIT_TARGETS {
        return Err(CoreError::InvalidArgument(
            "editor edit-target count exceeds the supported maximum",
        ));
    }
    let mut unique = std::collections::BTreeSet::new();
    if state
        .edit_targets
        .iter()
        .any(|target| !unique.insert(*target))
    {
        return Err(CoreError::InvalidArgument(
            "editor edit targets must be unique",
        ));
    }
    if state.tool_styles.len() != EditorTool::ALL.len() {
        return Err(CoreError::InvalidArgument(
            "editor state must contain one style for every tool",
        ));
    }
    for tool in EditorTool::ALL {
        let style = state
            .tool_styles
            .get(&tool)
            .ok_or(CoreError::InvalidArgument("editor tool style is missing"))?;
        validate_diameter(style.diameter_q16)?;
        match style.color {
            Some(color) if tool.consumes_color() => validate_color(color)?,
            None if !tool.consumes_color() => {}
            _ => {
                return Err(CoreError::InvalidArgument(
                    "editor tool color ownership is invalid",
                ));
            }
        }
    }
    if !state
        .last_color_consuming_tool
        .is_none_or(EditorTool::consumes_color)
    {
        return Err(CoreError::InvalidArgument(
            "last color-consuming tool is invalid",
        ));
    }
    if state.active_tool.consumes_color()
        && state.last_color_consuming_tool != Some(state.active_tool)
    {
        return Err(CoreError::InvalidArgument(
            "an active color-consuming tool must be the last color tool",
        ));
    }
    if state.brush.smoothing > 1_000 {
        return Err(CoreError::InvalidArgument(
            "editor brush smoothing exceeds 1000",
        ));
    }
    validate_fill(&state.fill)?;
    validate_selection_diameter(state.selection.diameter_q16)?;
    if state.selection.aspect_ratio_q16 > (4_096_u32 << 16) {
        return Err(CoreError::InvalidArgument(
            "editor selection aspect ratio exceeds 4096:1",
        ));
    }
    Ok(())
}

pub(crate) fn validate_color(color: PixelValue) -> Result<(), CoreError> {
    if matches!(color, PixelValue::Rgba(_) | PixelValue::Rgba16(_)) {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "editor colors must be straight-alpha RGBA8 or RGBA16",
        ))
    }
}

pub(crate) fn validate_diameter(diameter_q16: i64) -> Result<(), CoreError> {
    if (1..=MAX_DIAMETER_Q16).contains(&diameter_q16) {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "editor diameter must be positive and at most 256 document pixels",
        ))
    }
}

fn validate_selection_diameter(diameter_q16: i64) -> Result<(), CoreError> {
    if (1..=MAX_SELECTION_DIAMETER_Q16).contains(&diameter_q16) {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "editor selection diameter must be positive and at most 4096 document pixels",
        ))
    }
}

fn validate_fill(fill: &EditorFillOptions) -> Result<(), CoreError> {
    if fill.inclusion_colors.len() > MAX_INCLUSION_COLORS {
        return Err(CoreError::InvalidArgument(
            "fill inclusion color count exceeds the supported maximum",
        ));
    }
    for color in &fill.inclusion_colors {
        validate_color(*color)?;
    }
    match fill.inclusion_mode {
        InclusionMode::None if !fill.inclusion_colors.is_empty() => Err(
            CoreError::InvalidArgument("inclusion colors require an inclusion mode"),
        ),
        InclusionMode::Specified | InclusionMode::ExceptSpecified
            if fill.inclusion_colors.is_empty() =>
        {
            Err(CoreError::InvalidArgument(
                "an inclusion mode requires at least one color",
            ))
        }
        _ => Ok(()),
    }
}

fn encode_state_frame(state: &EditorState) -> Vec<u8> {
    let colors = encode_tool_colors(&state.tool_styles);
    let diameters = encode_tool_diameters(&state.tool_styles);
    let (layer, plane) = state.target.map_or((None, None), |target| {
        (
            Some(target.layer_id.to_le_bytes().to_vec()),
            Some(target.plane_id.to_le_bytes().to_vec()),
        )
    });
    encode_frame(&[
        Some(FRAME_SCHEMA.to_le_bytes().to_vec()),
        Some((state.active_tool as u32).to_le_bytes().to_vec()),
        state
            .last_color_consuming_tool
            .map(|tool| (tool as u32).to_le_bytes().to_vec()),
        Some(colors),
        Some(diameters),
        Some(encode_brush(state.brush)),
        Some(encode_fill(&state.fill)),
        Some(encode_selection(&state.selection)),
        layer,
        plane,
        state.palette_cursor.map(|cursor| {
            encode_frame(&[
                Some(cursor.group.to_le_bytes().to_vec()),
                Some(cursor.index.to_le_bytes().to_vec()),
            ])
        }),
        state.color_chart_cursor.map(|cursor| {
            encode_frame(&[
                Some(cursor.page.to_le_bytes().to_vec()),
                Some(cursor.index.to_le_bytes().to_vec()),
            ])
        }),
        Some(encode_sequence(state.edit_targets.iter().map(
            |target| match target {
                EditTarget::Layer(layer_id) => encode_frame(&[
                    Some(1_u32.to_le_bytes().to_vec()),
                    Some(layer_id.to_le_bytes().to_vec()),
                    None,
                ]),
                EditTarget::Plane(target) => encode_frame(&[
                    Some(2_u32.to_le_bytes().to_vec()),
                    Some(target.layer_id.to_le_bytes().to_vec()),
                    Some(target.plane_id.to_le_bytes().to_vec()),
                ]),
            },
        ))),
    ])
}

fn decode_state_frame(bytes: &[u8]) -> Result<EditorState, CoreError> {
    let fields = decode_frame(bytes, STATE_FIELD_COUNT)?;
    if read_u32(required(fields[0])?)? != FRAME_SCHEMA {
        return malformed("unsupported EditorState schema");
    }
    let active_tool = decode_tool(required(fields[1])?)?;
    let last_color_consuming_tool = fields[2].map(decode_tool).transpose()?;
    let colors = decode_tool_colors(required(fields[3])?)?;
    let diameters = decode_tool_diameters(required(fields[4])?)?;
    if colors.len() != EditorTool::ALL.len() || diameters.len() != EditorTool::ALL.len() {
        return malformed("incomplete tool style maps");
    }
    let mut tool_styles = BTreeMap::new();
    for tool in EditorTool::ALL {
        let color = colors
            .get(&tool)
            .copied()
            .ok_or_else(|| format_error("missing tool color"))?;
        let diameter_q16 = *diameters
            .get(&tool)
            .ok_or_else(|| format_error("missing tool diameter"))?;
        tool_styles.insert(
            tool,
            EditorToolStyle {
                color,
                diameter_q16,
            },
        );
    }
    let target = match (fields[8], fields[9]) {
        (Some(layer), Some(plane)) => Some(EditorTarget {
            layer_id: read_u64(layer)?,
            plane_id: read_u64(plane)?,
        }),
        (None, None) => None,
        _ => return malformed("partial editor target"),
    };
    let palette_cursor = fields[10]
        .map(|field| -> Result<PaletteCursor, CoreError> {
            let cursor = decode_frame(field, 2)?;
            Ok(PaletteCursor {
                group: read_u32(required(cursor[0])?)?,
                index: read_u32(required(cursor[1])?)?,
            })
        })
        .transpose()?;
    let color_chart_cursor = fields[11]
        .map(|field| -> Result<ColorChartCursor, CoreError> {
            let cursor = decode_frame(field, 2)?;
            Ok(ColorChartCursor {
                page: read_u32(required(cursor[0])?)?,
                index: read_u32(required(cursor[1])?)?,
            })
        })
        .transpose()?;
    let edit_targets = decode_sequence(required(fields[12])?, MAX_EDIT_TARGETS)?
        .into_iter()
        .map(|record| {
            let fields = decode_frame(record, 3)?;
            let kind = read_u32(required(fields[0])?)?;
            let layer_id = read_u64(required(fields[1])?)?;
            match (kind, fields[2]) {
                (1, None) => Ok(EditTarget::Layer(layer_id)),
                (2, Some(plane_id)) => Ok(EditTarget::Plane(EditorTarget {
                    layer_id,
                    plane_id: read_u64(plane_id)?,
                })),
                _ => malformed("invalid editor edit-target record"),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let state = EditorState {
        active_tool,
        last_color_consuming_tool,
        tool_styles,
        brush: decode_brush(required(fields[5])?)?,
        fill: decode_fill(required(fields[6])?)?,
        selection: decode_selection(required(fields[7])?)?,
        target,
        edit_targets,
        palette_cursor,
        color_chart_cursor,
    };
    validate_state(&state).map_err(|error| format_error(&error.to_string()))?;
    Ok(state)
}

fn encode_brush(brush: EditorBrushOptions) -> Vec<u8> {
    encode_frame(&[
        Some((brush.shape as u32).to_le_bytes().to_vec()),
        Some(brush.smoothing.to_le_bytes().to_vec()),
        Some((brush.start_color as u32).to_le_bytes().to_vec()),
    ])
}

fn decode_brush(bytes: &[u8]) -> Result<EditorBrushOptions, CoreError> {
    let fields = decode_frame(bytes, 3)?;
    let shape = match read_u32(required(fields[0])?)? {
        1 => BrushShape::Round,
        2 => BrushShape::Square,
        _ => return malformed("unknown editor brush shape"),
    };
    let smoothing = read_u16(required(fields[1])?)?;
    if smoothing > 1_000 {
        return malformed("editor brush smoothing exceeds 1000");
    }
    let start_color = match read_u32(required(fields[2])?)? {
        0 => StartColorPredicate::Any,
        1 => StartColorPredicate::ExactNative,
        _ => return malformed("unknown editor start-color predicate"),
    };
    Ok(EditorBrushOptions {
        shape,
        smoothing,
        start_color,
    })
}

fn encode_tool_colors(styles: &BTreeMap<EditorTool, EditorToolStyle>) -> Vec<u8> {
    encode_sequence(EditorTool::ALL.into_iter().map(|tool| {
        encode_frame(&[
            Some((tool as u32).to_le_bytes().to_vec()),
            styles
                .get(&tool)
                .and_then(|style| style.color)
                .map(encode_color),
        ])
    }))
}

fn decode_tool_colors(bytes: &[u8]) -> Result<BTreeMap<EditorTool, Option<PixelValue>>, CoreError> {
    let entries = decode_sequence(bytes, EditorTool::ALL.len())?;
    if entries.len() != EditorTool::ALL.len() {
        return malformed("tool color count");
    }
    let mut colors = BTreeMap::new();
    for (expected, entry) in EditorTool::ALL.into_iter().zip(entries) {
        let fields = decode_frame(entry, 2)?;
        let tool = decode_tool(required(fields[0])?)?;
        if tool != expected || colors.contains_key(&tool) {
            return malformed("tool colors are not in canonical order");
        }
        let color = fields[1].map(decode_color).transpose()?;
        colors.insert(tool, color);
    }
    Ok(colors)
}

fn encode_tool_diameters(styles: &BTreeMap<EditorTool, EditorToolStyle>) -> Vec<u8> {
    encode_sequence(EditorTool::ALL.into_iter().map(|tool| {
        let diameter = styles.get(&tool).map_or(0, |style| style.diameter_q16);
        encode_frame(&[
            Some((tool as u32).to_le_bytes().to_vec()),
            Some(diameter.to_le_bytes().to_vec()),
        ])
    }))
}

fn decode_tool_diameters(bytes: &[u8]) -> Result<BTreeMap<EditorTool, i64>, CoreError> {
    let entries = decode_sequence(bytes, EditorTool::ALL.len())?;
    if entries.len() != EditorTool::ALL.len() {
        return malformed("tool diameter count");
    }
    let mut diameters = BTreeMap::new();
    for (expected, entry) in EditorTool::ALL.into_iter().zip(entries) {
        let fields = decode_frame(entry, 2)?;
        let tool = decode_tool(required(fields[0])?)?;
        if tool != expected || diameters.contains_key(&tool) {
            return malformed("tool diameters are not in canonical order");
        }
        diameters.insert(tool, read_i64(required(fields[1])?)?);
    }
    Ok(diameters)
}

fn encode_fill(fill: &EditorFillOptions) -> Vec<u8> {
    encode_frame(&[
        Some(fill_operation_code(fill.operation).to_le_bytes().to_vec()),
        Some(fill.tolerance.to_le_bytes().to_vec()),
        Some(vec![fill.gap_close]),
        Some(fill.extension_distance.to_le_bytes().to_vec()),
        Some(
            inclusion_mode_code(fill.inclusion_mode)
                .to_le_bytes()
                .to_vec(),
        ),
        Some(encode_sequence(
            fill.inclusion_colors.iter().copied().map(encode_color),
        )),
        Some(vec![u8::from(fill.overflow_abort)]),
        Some(vec![u8::from(fill.detached_regions)]),
        Some(vec![u8::from(fill.transparent_only)]),
        Some(vec![u8::from(fill.use_document_selection)]),
        Some(vec![u8::from(fill.light_table_boundary)]),
        Some(vec![u8::from(fill.light_table_color)]),
    ])
}

fn decode_fill(bytes: &[u8]) -> Result<EditorFillOptions, CoreError> {
    let fields = decode_frame(bytes, 12)?;
    let inclusion_colors = decode_sequence(required(fields[5])?, MAX_INCLUSION_COLORS)?
        .into_iter()
        .map(decode_color)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EditorFillOptions {
        operation: decode_fill_operation(read_u32(required(fields[0])?)?)?,
        tolerance: read_u16(required(fields[1])?)?,
        gap_close: read_u8(required(fields[2])?)?,
        extension_distance: read_u32(required(fields[3])?)?,
        inclusion_mode: decode_inclusion_mode(read_u32(required(fields[4])?)?)?,
        inclusion_colors,
        overflow_abort: read_bool(required(fields[6])?)?,
        detached_regions: read_bool(required(fields[7])?)?,
        transparent_only: read_bool(required(fields[8])?)?,
        use_document_selection: read_bool(required(fields[9])?)?,
        light_table_boundary: read_bool(required(fields[10])?)?,
        light_table_color: read_bool(required(fields[11])?)?,
    })
}

fn encode_selection(selection: &EditorSelectionOptions) -> Vec<u8> {
    encode_frame(&[
        Some((selection.shape as u32).to_le_bytes().to_vec()),
        Some(
            selection_operation_code(selection.operation)
                .to_le_bytes()
                .to_vec(),
        ),
        Some(selection.tolerance.to_le_bytes().to_vec()),
        Some(vec![selection.gap_close]),
        Some(selection.diameter_q16.to_le_bytes().to_vec()),
        Some((selection.interpretation as u32).to_le_bytes().to_vec()),
        Some(selection.aspect_ratio_q16.to_le_bytes().to_vec()),
        Some(vec![u8::from(selection.from_center)]),
        Some(vec![u8::from(selection.constrain_rotation_45)]),
        Some(selection.rotation_turns.to_le_bytes().to_vec()),
        Some((selection.trace_shape as u32).to_le_bytes().to_vec()),
        Some(vec![u8::from(selection.trace_pressure_size)]),
        Some(vec![u8::from(selection.trace_screen_size)]),
    ])
}

fn decode_selection(bytes: &[u8]) -> Result<EditorSelectionOptions, CoreError> {
    let fields = decode_frame(bytes, 13)?;
    let shape = EditorSelectionShape::from_code(read_u32(required(fields[0])?)?)
        .ok_or_else(|| format_error("unknown selection shape"))?;
    Ok(EditorSelectionOptions {
        shape,
        operation: decode_selection_operation(read_u32(required(fields[1])?)?)?,
        tolerance: read_u16(required(fields[2])?)?,
        gap_close: read_u8(required(fields[3])?)?,
        diameter_q16: read_i64(required(fields[4])?)?,
        interpretation: match read_u32(required(fields[5])?)? {
            1 => RangeInterpretation::Normal,
            2 => RangeInterpretation::Tight,
            3 => RangeInterpretation::EnclosedInterior,
            4 => RangeInterpretation::Drawing,
            5 => RangeInterpretation::Boundary,
            _ => return malformed("unknown raster range interpretation"),
        },
        aspect_ratio_q16: read_u32(required(fields[6])?)?,
        from_center: read_bool(required(fields[7])?)?,
        constrain_rotation_45: read_bool(required(fields[8])?)?,
        rotation_turns: read_u32(required(fields[9])?)?,
        trace_shape: match read_u32(required(fields[10])?)? {
            1 => TraceBrushShape::Round,
            2 => TraceBrushShape::Square,
            _ => return malformed("unknown trace brush shape"),
        },
        trace_pressure_size: read_bool(required(fields[11])?)?,
        trace_screen_size: read_bool(required(fields[12])?)?,
    })
}

fn encode_color(color: PixelValue) -> Vec<u8> {
    match color {
        PixelValue::Rgba(value) => {
            let mut bytes = Vec::with_capacity(5);
            bytes.push(1);
            bytes.extend_from_slice(&value);
            bytes
        }
        PixelValue::Rgba16(value) => {
            let mut bytes = Vec::with_capacity(9);
            bytes.push(2);
            for channel in value {
                bytes.extend_from_slice(&channel.to_le_bytes());
            }
            bytes
        }
        PixelValue::Binary(_) | PixelValue::Grayscale8(_) | PixelValue::Grayscale16(_) => {
            Vec::new()
        }
    }
}

fn decode_color(bytes: &[u8]) -> Result<PixelValue, CoreError> {
    match bytes {
        [1, red, green, blue, alpha] => Ok(PixelValue::Rgba([*red, *green, *blue, *alpha])),
        [2, channels @ ..] if channels.len() == 8 => {
            let mut value = [0_u16; 4];
            for (index, channel) in channels.chunks_exact(2).enumerate() {
                value[index] = u16::from_le_bytes(channel.try_into().expect("fixed chunk"));
            }
            Ok(PixelValue::Rgba16(value))
        }
        _ => malformed("invalid exact-depth editor color"),
    }
}

fn encode_sequence(elements: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    let elements: Vec<_> = elements.into_iter().collect();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(elements.len() as u64).to_le_bytes());
    for element in elements {
        bytes.extend_from_slice(&(element.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&element);
    }
    bytes
}

fn decode_sequence(bytes: &[u8], maximum: usize) -> Result<Vec<&[u8]>, CoreError> {
    let mut cursor = Cursor::new(bytes);
    let count = cursor.length()?;
    if count > maximum {
        return malformed("canonical sequence count exceeds its bound");
    }
    let mut elements = Vec::with_capacity(count);
    for _ in 0..count {
        let length = cursor.length()?;
        elements.push(cursor.take(length)?);
    }
    cursor.finish()?;
    Ok(elements)
}

fn encode_frame(fields: &[Option<Vec<u8>>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&FRAME_SCHEMA.to_le_bytes());
    bytes.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    for (index, value) in fields.iter().enumerate() {
        bytes.extend_from_slice(&((index + 1) as u32).to_le_bytes());
        bytes.push(u8::from(value.is_some()));
        bytes.extend_from_slice(&[0; 3]);
        bytes.extend_from_slice(&(value.as_ref().map_or(0, Vec::len) as u64).to_le_bytes());
        if let Some(value) = value {
            bytes.extend_from_slice(value);
        }
    }
    bytes
}

fn decode_frame(bytes: &[u8], field_count: usize) -> Result<Vec<Option<&[u8]>>, CoreError> {
    let mut cursor = Cursor::new(bytes);
    if cursor.u32()? != FRAME_SCHEMA || cursor.u32()? != field_count as u32 {
        return malformed("canonical frame prefix");
    }
    let mut fields = Vec::with_capacity(field_count);
    for ordinal in 1..=field_count {
        if cursor.u32()? != ordinal as u32 {
            return malformed("canonical field order");
        }
        let present = cursor.u8()?;
        if cursor.take(3)? != [0, 0, 0] {
            return malformed("nonzero canonical reserved bytes");
        }
        let length = cursor.length()?;
        fields.push(match present {
            0 if length == 0 => None,
            1 => Some(cursor.take(length)?),
            _ => return malformed("canonical field presence or length"),
        });
    }
    cursor.finish()?;
    Ok(fields)
}

fn required(field: Option<&[u8]>) -> Result<&[u8], CoreError> {
    field.ok_or_else(|| format_error("required canonical field is absent"))
}

fn decode_tool(bytes: &[u8]) -> Result<EditorTool, CoreError> {
    EditorTool::from_code(read_u32(bytes)?).ok_or_else(|| format_error("unknown editor tool enum"))
}

fn read_u8(bytes: &[u8]) -> Result<u8, CoreError> {
    bytes
        .first()
        .copied()
        .filter(|_| bytes.len() == 1)
        .ok_or_else(|| format_error("u8 field length"))
}

fn read_u16(bytes: &[u8]) -> Result<u16, CoreError> {
    bytes
        .try_into()
        .map(u16::from_le_bytes)
        .map_err(|_| format_error("u16 field length"))
}

fn read_u32(bytes: &[u8]) -> Result<u32, CoreError> {
    bytes
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| format_error("u32 field length"))
}

fn read_u64(bytes: &[u8]) -> Result<u64, CoreError> {
    bytes
        .try_into()
        .map(u64::from_le_bytes)
        .map_err(|_| format_error("u64 field length"))
}

fn read_i64(bytes: &[u8]) -> Result<i64, CoreError> {
    bytes
        .try_into()
        .map(i64::from_le_bytes)
        .map_err(|_| format_error("i64 field length"))
}

fn read_bool(bytes: &[u8]) -> Result<bool, CoreError> {
    match read_u8(bytes)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => malformed("non-canonical boolean"),
    }
}

fn fill_operation_code(value: FillOperation) -> u32 {
    match value {
        FillOperation::Seed => 1,
        FillOperation::ClosedRegion => 2,
        FillOperation::Extend => 3,
    }
}

fn decode_fill_operation(code: u32) -> Result<FillOperation, CoreError> {
    match code {
        1 => Ok(FillOperation::Seed),
        2 => Ok(FillOperation::ClosedRegion),
        3 => Ok(FillOperation::Extend),
        _ => malformed("unknown fill operation enum"),
    }
}

fn inclusion_mode_code(value: InclusionMode) -> u32 {
    match value {
        InclusionMode::None => 0,
        InclusionMode::Specified => 1,
        InclusionMode::ExceptSpecified => 2,
    }
}

fn decode_inclusion_mode(code: u32) -> Result<InclusionMode, CoreError> {
    match code {
        0 => Ok(InclusionMode::None),
        1 => Ok(InclusionMode::Specified),
        2 => Ok(InclusionMode::ExceptSpecified),
        _ => malformed("unknown inclusion mode enum"),
    }
}

fn selection_operation_code(value: SelectionOperation) -> u32 {
    match value {
        SelectionOperation::New => 1,
        SelectionOperation::Add => 2,
        SelectionOperation::Subtract => 3,
        SelectionOperation::Intersect => 4,
    }
}

fn decode_selection_operation(code: u32) -> Result<SelectionOperation, CoreError> {
    match code {
        1 => Ok(SelectionOperation::New),
        2 => Ok(SelectionOperation::Add),
        3 => Ok(SelectionOperation::Subtract),
        4 => Ok(SelectionOperation::Intersect),
        _ => malformed("unknown selection operation enum"),
    }
}

fn malformed<T>(message: &str) -> Result<T, CoreError> {
    Err(format_error(message))
}

fn format_error(message: &str) -> CoreError {
    CoreError::Format(format!("invalid canonical EDIT frame: {message}"))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], CoreError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| format_error("length overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| format_error("truncated canonical value"))?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, CoreError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, CoreError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("fixed slice"),
        ))
    }

    fn u64(&mut self) -> Result<u64, CoreError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("fixed slice"),
        ))
    }

    fn length(&mut self) -> Result<usize, CoreError> {
        usize::try_from(self.u64()?).map_err(|_| format_error("length exceeds address space"))
    }

    fn finish(self) -> Result<(), CoreError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            malformed("trailing canonical bytes")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_default_state_digest_is_locked() {
        let mut state = EditorDefaults::built_in().state;
        state.target = Some(EditorTarget {
            layer_id: 2,
            plane_id: 3,
        });
        assert_eq!(
            state_digest(&state).as_bytes(),
            &[
                // Native v34 adds the two raster line tools to the persisted defaults.
                43, 152, 29, 233, 184, 7, 234, 250, 180, 35, 90, 209, 252, 192, 71, 163, 188, 35,
                243, 250, 107, 49, 43, 93, 222, 35, 102, 0, 156, 148, 218, 49,
            ]
        );
    }

    #[test]
    fn edit_frame_rejects_bytes_past_the_logical_limit() {
        assert!(matches!(
            decode_edit_frame(&vec![0; MAX_EDIT_FRAME_BYTES + 1]),
            Err(CoreError::Format(_))
        ));
    }
}
