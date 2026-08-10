use super::model::*;
use super::*;

pub(crate) fn fixed_path(
    id: VectorPathId,
    plane_id: PlaneId,
    input: VectorPathInput,
) -> Result<VectorPath, CoreError> {
    if input.segments.is_empty() || input.segments.len() > MAX_VECTOR_SEGMENTS {
        return Err(CoreError::InvalidArgument(
            "vector segment count is outside bounds",
        ));
    }
    if input.color.rgba16().is_none() {
        return Err(CoreError::InvalidArgument("vector path color must be RGBA"));
    }
    let mut segments = Vec::with_capacity(input.segments.len());
    for segment in input.segments {
        let segment = VectorSegment {
            p0: fixed_point(segment.p0)?,
            p1: fixed_point(segment.p1)?,
            p2: fixed_point(segment.p2)?,
            p3: fixed_point(segment.p3)?,
            width_start_milli: fixed_width(segment.width_start)?,
            width_end_milli: fixed_width(segment.width_end)?,
        };
        if segments
            .last()
            .is_some_and(|previous: &VectorSegment| previous.p3 != segment.p0)
        {
            return Err(CoreError::InvalidArgument(
                "vector path segments are not continuous",
            ));
        }
        segments.push(segment);
    }
    if input.closed && segments.last().is_none_or(|last| last.p3 != segments[0].p0) {
        return Err(CoreError::InvalidArgument(
            "closed vector path does not close",
        ));
    }
    Ok(VectorPath {
        id,
        plane_id,
        color: input.color,
        closed: input.closed,
        square_cross_section: false,
        segments,
    })
}

pub(super) fn fixed_point(point: PointF32) -> Result<FixedPoint, CoreError> {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || f64::from(point.x).abs() > MAX_COORDINATE
        || f64::from(point.y).abs() > MAX_COORDINATE
    {
        return Err(CoreError::InvalidArgument("vector point is outside bounds"));
    }
    Ok(fixed_xy_point(f64::from(point.x), f64::from(point.y)))
}

pub(super) fn fixed_xy_point(x: f64, y: f64) -> FixedPoint {
    FixedPoint {
        x_milli: (x * UNITS_PER_PIXEL).round() as i32,
        y_milli: (y * UNITS_PER_PIXEL).round() as i32,
    }
}

pub(super) fn fixed_width(width: f32) -> Result<u32, CoreError> {
    if !width.is_finite() || width <= 0.0 || width > MAX_WIDTH {
        return Err(CoreError::InvalidArgument("vector width is outside bounds"));
    }
    let fixed = (f64::from(width) * UNITS_PER_PIXEL).round() as u32;
    if fixed == 0 {
        return Err(CoreError::InvalidArgument(
            "vector width is below fixed-point precision",
        ));
    }
    Ok(fixed)
}

pub(crate) fn ensure_vector_stroke_plane(
    document: &CellDocument,
    plane_id: PlaneId,
    editable: bool,
) -> Result<LayerId, CoreError> {
    let layer_id = vector_layer_for_plane(document, plane_id)?;
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.id == layer_id)
        .expect("vector layer exists");
    let plane = layer
        .planes
        .iter()
        .find(|plane| plane.id == plane_id)
        .expect("vector plane exists");
    if !matches!(
        plane.kind,
        PlaneType::VectorMainLine | PlaneType::ColorTrace
    ) {
        return Err(CoreError::InvalidArgument(
            "vector path destination is not a stroke plane",
        ));
    }
    if editable && (!layer.editable || !plane.editable) {
        return Err(CoreError::InvalidState(
            "vector destination is not editable",
        ));
    }
    Ok(layer_id)
}

pub(crate) fn ensure_vector_fill_plane(
    document: &CellDocument,
    plane_id: PlaneId,
    editable: bool,
) -> Result<LayerId, CoreError> {
    let layer_id = vector_layer_for_plane(document, plane_id)?;
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.id == layer_id)
        .expect("vector layer exists");
    let plane = layer
        .planes
        .iter()
        .find(|plane| plane.id == plane_id)
        .expect("vector plane exists");
    if plane.kind != PlaneType::VectorFill {
        return Err(CoreError::InvalidArgument(
            "vector fill destination is not a fill plane",
        ));
    }
    if editable && (!layer.editable || !plane.editable) {
        return Err(CoreError::InvalidState(
            "vector destination is not editable",
        ));
    }
    Ok(layer_id)
}

pub(crate) fn geometry_fill_plane_for_stroke(
    document: &CellDocument,
    stroke_plane_id: PlaneId,
) -> Result<PlaneId, CoreError> {
    let layer_id = ensure_vector_stroke_plane(document, stroke_plane_id, true)?;
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.id == layer_id)
        .expect("validated vector layer exists");
    let fill = layer
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneType::VectorFill)
        .ok_or(CoreError::InvalidState("vector layer has no fill plane"))?;
    if !fill.editable {
        return Err(CoreError::InvalidState(
            "vector fill destination is not editable",
        ));
    }
    Ok(fill.id)
}

pub(crate) fn stage_geometry_path(
    document: &mut CellDocument,
    path_id: VectorPathId,
    plane_id: PlaneId,
    input: VectorPathInput,
    square_cross_section: bool,
) -> Result<(), CoreError> {
    ensure_vector_stroke_plane(document, plane_id, true)?;
    document
        .vector
        .ensure_additional_limits(1, 0, input.segments.len(), 0)?;
    let mut path = fixed_path(path_id, plane_id, input)?;
    path.square_cross_section = square_cross_section;
    document.vector.paths.push(path);
    Ok(())
}

pub(crate) fn stage_geometry_fill(
    document: &mut CellDocument,
    fill_id: VectorFillId,
    plane_id: PlaneId,
    boundary_path_id: VectorPathId,
    color: PixelValue,
) -> Result<(), CoreError> {
    ensure_vector_fill_plane(document, plane_id, true)?;
    if color.rgba16().is_none() {
        return Err(CoreError::InvalidArgument("vector fill color must be RGBA"));
    }
    document.vector.ensure_additional_limits(0, 1, 0, 1)?;
    document.vector.fills.push(VectorFill {
        id: fill_id,
        plane_id,
        color,
        boundary_path_ids: vec![boundary_path_id],
    });
    Ok(())
}

pub(super) fn vector_layer_for_plane(
    document: &CellDocument,
    plane_id: PlaneId,
) -> Result<LayerId, CoreError> {
    document
        .layers
        .iter()
        .find(|layer| {
            layer.kind == LayerKind::VectorColoring
                && layer.planes.iter().any(|plane| plane.id == plane_id)
        })
        .map(|layer| layer.id)
        .ok_or(CoreError::InvalidArgument("vector plane ID does not exist"))
}

pub(super) fn path_info(path: &VectorPath) -> VectorPathInfo {
    VectorPathInfo {
        id: path.id.get(),
        plane_id: path.plane_id.get(),
        segments: path.segments.iter().copied().map(public_segment).collect(),
        color: path.color,
        closed: path.closed,
        square_cross_section: path.square_cross_section,
    }
}

pub(super) fn public_segment(segment: VectorSegment) -> VectorCubicSegment {
    VectorCubicSegment {
        p0: public_point(segment.p0),
        p1: public_point(segment.p1),
        p2: public_point(segment.p2),
        p3: public_point(segment.p3),
        width_start: segment.width_start_milli as f32 / 1_000.0,
        width_end: segment.width_end_milli as f32 / 1_000.0,
    }
}

pub(super) fn public_point(point: FixedPoint) -> PointF32 {
    PointF32 {
        x: point.x_milli as f32 / 1_000.0,
        y: point.y_milli as f32 / 1_000.0,
    }
}

pub(super) fn file_point(point: FixedPoint) -> FileVectorPoint {
    FileVectorPoint {
        x_milli: point.x_milli,
        y_milli: point.y_milli,
    }
}

pub(super) fn fixed_file_point(point: FileVectorPoint) -> FixedPoint {
    FixedPoint {
        x_milli: point.x_milli,
        y_milli: point.y_milli,
    }
}

pub(super) fn rgba8(color: PixelValue) -> [u8; 4] {
    match color {
        PixelValue::Rgba(value) => value,
        PixelValue::Rgba16(value) => value.map(|channel| ((u32::from(channel) + 128) / 257) as u8),
        _ => [0, 0, 0, 0],
    }
}

pub(super) fn display_color(color: PixelValue, layer_opacity: u32, plane_opacity: u32) -> [u8; 4] {
    let mut value = rgba8(color);
    value[3] = ((u64::from(value[3]) * u64::from(layer_opacity) * u64::from(plane_opacity)
        + 500_000)
        / 1_000_000) as u8;
    value
}

pub(super) fn flatten_path(path: &VectorPath, steps: usize) -> Vec<FlatSample> {
    flatten_vector_path(&path.segments, steps)
}

pub(super) fn closest_path_parameter(
    path: &VectorPath,
    point: (f64, f64),
    maximum_distance: f64,
) -> Option<f64> {
    flatten_path(path, FLATTEN_STEPS)
        .windows(2)
        .map(|pair| {
            let (distance, fraction) = distance_to_segment(point, pair[0].point, pair[1].point);
            (
                distance,
                lerp(pair[0].parameter, pair[1].parameter, fraction),
            )
        })
        .filter(|(distance, _)| *distance <= maximum_distance)
        .min_by(|left, right| left.0.total_cmp(&right.0).then(left.1.total_cmp(&right.1)))
        .map(|(_, parameter)| parameter)
}

pub(super) fn eraser_interval(
    path: &VectorPath,
    center: (f64, f64),
    radius: f64,
) -> Option<(f64, f64)> {
    let samples = flatten_path(path, FLATTEN_STEPS * 2);
    let touch = closest_path_parameter(path, center, radius)?;
    let mut inside: Vec<_> = samples
        .iter()
        .filter(|sample| squared_distance(sample.point, center) <= radius * radius)
        .map(|sample| sample.parameter)
        .collect();
    inside.push(touch);
    inside.sort_by(f64::total_cmp);
    let mut start = *inside.first()?;
    let mut end = *inside.last()?;
    let step = 1.0 / (FLATTEN_STEPS * 2) as f64;
    while start > 0.0
        && point_at_path(path, (start - step).max(0.0))
            .is_some_and(|point| squared_distance(point, center) <= radius * radius)
    {
        start = (start - step).max(0.0);
    }
    while end < path_length_t(path)
        && point_at_path(path, (end + step).min(path_length_t(path)))
            .is_some_and(|point| squared_distance(point, center) <= radius * radius)
    {
        end = (end + step).min(path_length_t(path));
    }
    if start > 0.0 {
        start = refine_circle_boundary(path, center, radius, (start - step).max(0.0), start);
    }
    if end < path_length_t(path) {
        end = refine_circle_boundary(
            path,
            center,
            radius,
            end,
            (end + step).min(path_length_t(path)),
        );
    }
    Some((start, end))
}

pub(super) fn refine_circle_boundary(
    path: &VectorPath,
    center: (f64, f64),
    radius: f64,
    mut left: f64,
    mut right: f64,
) -> f64 {
    let left_inside = point_at_path(path, left)
        .is_some_and(|point| squared_distance(point, center) <= radius * radius);
    for _ in 0..24 {
        let middle = (left + right) * 0.5;
        let middle_inside = point_at_path(path, middle)
            .is_some_and(|point| squared_distance(point, center) <= radius * radius);
        if middle_inside == left_inside {
            left = middle;
        } else {
            right = middle;
        }
    }
    ((left + right) * 0.5 * 1.0e9).round() / 1.0e9
}

pub(super) fn point_at_path(path: &VectorPath, parameter: f64) -> Option<(f64, f64)> {
    vector_point_at(&path.segments, parameter)
}

pub(super) fn remaining_pieces(
    path: &VectorPath,
    start: f64,
    end: f64,
    next_id: &mut StableIdCursor,
) -> Vec<VectorPath> {
    let mut pieces = Vec::new();
    if start > 1.0e-9 {
        if let Some(mut prefix) = subpath(path, 0.0, start) {
            prefix.id = path.id;
            prefix.closed = false;
            pieces.push(prefix);
        }
    }
    if end < path_length_t(path) - 1.0e-9 {
        if let Some(mut suffix) = subpath(path, end, path_length_t(path)) {
            suffix.id = if pieces.is_empty() {
                path.id
            } else {
                next_id.take_vector_path()
            };
            suffix.closed = false;
            pieces.push(suffix);
        }
    }
    pieces
}

pub(super) fn subpath(path: &VectorPath, start: f64, end: f64) -> Option<VectorPath> {
    if end - start <= 1.0e-9 {
        return None;
    }
    let first = start.floor() as usize;
    let last_parameter = (end - 1.0e-12).max(start);
    let last = (last_parameter.floor() as usize).min(path.segments.len() - 1);
    let mut segments: Vec<VectorSegment> = Vec::new();
    for index in first..=last {
        let local_start = if index == first {
            start - index as f64
        } else {
            0.0
        };
        let local_end = if index == last {
            (end - index as f64).min(1.0)
        } else {
            1.0
        };
        if local_end - local_start > 1.0e-9 {
            let mut segment = subsegment(path.segments[index], local_start, local_end);
            if let Some(previous) = segments.last() {
                segment.p0 = previous.p3;
            }
            segments.push(segment);
        }
    }
    (!segments.is_empty()).then_some(VectorPath {
        id: path.id,
        plane_id: path.plane_id,
        color: path.color,
        closed: false,
        square_cross_section: path.square_cross_section,
        segments,
    })
}

pub(super) fn subsegment(segment: VectorSegment, start: f64, end: f64) -> VectorSegment {
    sub_vector_cubic(segment, start, end)
}

pub(super) fn path_intersections(left: &VectorPath, right: &VectorPath) -> Vec<f64> {
    vector_path_intersections(&left.segments, &right.segments, FLATTEN_STEPS)
}

pub(super) fn line_intersection(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
) -> Option<(f64, f64)> {
    vector_line_intersection(a, b, c, d)
}

pub(super) fn endpoint(path: &VectorPath, end: bool) -> FixedPoint {
    if end {
        path.segments.last().expect("path has segments").p3
    } else {
        path.segments[0].p0
    }
}

pub(super) fn endpoint_width(path: &VectorPath, end: bool) -> u32 {
    if end {
        path.segments
            .last()
            .expect("path has segments")
            .width_end_milli
    } else {
        path.segments[0].width_start_milli
    }
}

pub(super) fn line_segment(
    start: FixedPoint,
    end: FixedPoint,
    start_width: u32,
    end_width: u32,
) -> VectorSegment {
    vector_line_cubic(start, end, start_width, end_width)
}

pub(super) fn width_transform(
    mode: VectorWidthMode,
) -> Result<impl Fn(u32) -> Result<u32, CoreError>, CoreError> {
    let parameter = match mode {
        VectorWidthMode::Add(value)
        | VectorWidthMode::Subtract(value)
        | VectorWidthMode::Scale(value)
        | VectorWidthMode::Constant(value) => value,
    };
    if !parameter.is_finite() || parameter <= 0.0 || parameter > MAX_WIDTH {
        return Err(CoreError::InvalidArgument(
            "vector width correction parameter is invalid",
        ));
    }
    Ok(move |width| {
        let value = match mode {
            VectorWidthMode::Add(value) => f64::from(width) + f64::from(value) * UNITS_PER_PIXEL,
            VectorWidthMode::Subtract(value) => {
                f64::from(width) - f64::from(value) * UNITS_PER_PIXEL
            }
            VectorWidthMode::Scale(value) => f64::from(width) * f64::from(value),
            VectorWidthMode::Constant(value) => f64::from(value) * UNITS_PER_PIXEL,
        };
        if value < 1.0 || value > f64::from(MAX_WIDTH) * UNITS_PER_PIXEL {
            return Err(CoreError::InvalidArgument(
                "vector width correction exceeds bounds",
            ));
        }
        Ok(value.round() as u32)
    })
}

pub(super) fn selection_range(path: &VectorPath, start: f64, end: f64) -> VectorSelectionRange {
    let total = path_length_t(path);
    VectorSelectionRange {
        path_id: path.id.get(),
        start_million: ((start / total).clamp(0.0, 1.0) * 1_000_000.0).round() as u32,
        end_million: ((end / total).clamp(0.0, 1.0) * 1_000_000.0).round() as u32,
    }
}

pub(super) fn full_selection(path_id: VectorPathId) -> VectorSelectionRange {
    VectorSelectionRange {
        path_id: path_id.get(),
        start_million: 0,
        end_million: 1_000_000,
    }
}

pub(super) fn path_length_t(path: &VectorPath) -> f64 {
    path.segments.len() as f64
}

pub(super) fn point_in_rect(point: (f64, f64), rect: (f64, f64, f64, f64)) -> bool {
    point.0 >= rect.0 && point.0 <= rect.2 && point.1 >= rect.1 && point.1 <= rect.3
}

pub(super) fn segment_rect_intersections(
    start: (f64, f64),
    end: (f64, f64),
    rect: (f64, f64, f64, f64),
) -> Vec<f64> {
    let corners = [
        (rect.0, rect.1),
        (rect.2, rect.1),
        (rect.2, rect.3),
        (rect.0, rect.3),
    ];
    let mut intersections = (0..4)
        .filter_map(|index| {
            line_intersection(start, end, corners[index], corners[(index + 1) % 4])
                .map(|(fraction, _)| fraction)
        })
        .collect::<Vec<_>>();
    intersections.sort_by(f64::total_cmp);
    intersections.dedup_by(|left, right| (*left - *right).abs() < 1.0e-9);
    intersections
}

pub(super) fn sampled_bounds(
    mut samples: impl Iterator<Item = FlatSample>,
    padding: f64,
) -> Option<(f64, f64, f64, f64)> {
    let first = samples.next()?;
    let mut bounds = (first.point.0, first.point.1, first.point.0, first.point.1);
    for sample in samples {
        bounds.0 = bounds.0.min(sample.point.0);
        bounds.1 = bounds.1.min(sample.point.1);
        bounds.2 = bounds.2.max(sample.point.0);
        bounds.3 = bounds.3.max(sample.point.1);
    }
    Some((
        bounds.0 - padding,
        bounds.1 - padding,
        bounds.2 + padding,
        bounds.3 + padding,
    ))
}

pub(super) fn point_in_sampled_fill(boundaries: &[Vec<FlatSample>], point: (f64, f64)) -> bool {
    let mut inside = false;
    for samples in boundaries {
        for pair in samples.windows(2) {
            let (a, b) = (pair[0].point, pair[1].point);
            if (a.1 > point.1) != (b.1 > point.1)
                && point.0 < (b.0 - a.0) * (point.1 - a.1) / (b.1 - a.1) + a.0
            {
                inside = !inside;
            }
        }
    }
    inside
}

pub(super) fn point_on_sampled_stroke(samples: &[FlatSample], point: (f64, f64)) -> bool {
    samples.windows(2).any(|pair| {
        let (distance, fraction) = distance_to_segment(point, pair[0].point, pair[1].point);
        let width = lerp(pair[0].width, pair[1].width, fraction);
        distance <= width * 0.5
    })
}

pub(super) fn point_in_fill(state: &VectorState, fill: &VectorFill, point: (f64, f64)) -> bool {
    let boundaries = fill
        .boundary_path_ids
        .iter()
        .filter_map(|path_id| state.paths.iter().find(|path| path.id == *path_id))
        .map(|path| flatten_path(path, RASTER_STEPS))
        .collect::<Vec<_>>();
    point_in_sampled_fill(&boundaries, point)
}

pub(super) fn distance_to_segment(
    point: (f64, f64),
    start: (f64, f64),
    end: (f64, f64),
) -> (f64, f64) {
    vector_distance_to_segment(point, start, end)
}

pub(super) fn source_over_rgba(destination: [u8; 4], source: [u8; 4]) -> [u8; 4] {
    vector_source_over(destination, source)
}

pub(super) fn fixed_xy(point: FixedPoint) -> (f64, f64) {
    vector_fixed_xy(point)
}

pub(super) fn squared_distance(left: (f64, f64), right: (f64, f64)) -> f64 {
    vector_squared_distance(left, right)
}

pub(super) fn lerp(left: f64, right: f64, amount: f64) -> f64 {
    vector_lerp(left, right, amount)
}
