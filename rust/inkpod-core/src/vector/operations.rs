use super::geometry::*;
use super::model::*;
use super::*;

impl Core {
    pub fn vector_layer_planes(&self, layer_id: u64) -> Result<(u64, u64, u64), CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let layer = document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id && layer.kind == LayerKind::VectorColoring)
            .ok_or(CoreError::InvalidArgument("vector layer ID does not exist"))?;
        let find = |kind| {
            layer
                .planes
                .iter()
                .find(|plane| plane.kind == kind)
                .map(|plane| plane.id)
                .ok_or(CoreError::InvalidState(
                    "vector layer is missing a required plane",
                ))
        };
        Ok((
            find(PlaneType::VectorMainLine)?,
            find(PlaneType::ColorTrace)?,
            find(PlaneType::VectorFill)?,
        ))
    }

    pub fn vector_add_path(
        &mut self,
        plane_id: u64,
        input: VectorPathInput,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let path = fixed_path(0, plane_id, input)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        ensure_vector_stroke_plane(&before, plane_id, true)?;
        before
            .vector
            .ensure_additional_limits(1, 0, path.segments.len(), 0)?;
        let mut next_id = self.next_id;
        let path_id = take_id(&mut next_id);
        let mut after = before.clone();
        after.vector.paths.push(VectorPath {
            id: path_id,
            ..path
        });
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, path_id))
    }

    pub fn vector_add_fill(
        &mut self,
        plane_id: u64,
        boundary_path_ids: &[u64],
        color: PixelValue,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        if boundary_path_ids.is_empty() || boundary_path_ids.len() > MAX_VECTOR_BOUNDARIES {
            return Err(CoreError::InvalidArgument(
                "vector fill boundary count is outside bounds",
            ));
        }
        if color.rgba16().is_none() {
            return Err(CoreError::InvalidArgument("vector fill color must be RGBA"));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let fill_layer = ensure_vector_fill_plane(&before, plane_id, true)?;
        before
            .vector
            .ensure_additional_limits(0, 1, 0, boundary_path_ids.len())?;
        let mut unique = BTreeSet::new();
        for path_id in boundary_path_ids {
            let path = before
                .vector
                .paths
                .iter()
                .find(|path| path.id == *path_id)
                .ok_or(CoreError::InvalidArgument(
                    "fill boundary path does not exist",
                ))?;
            if !path.closed || !unique.insert(*path_id) {
                return Err(CoreError::InvalidArgument(
                    "fill boundaries must be unique closed paths",
                ));
            }
            let path_layer = vector_layer_for_plane(&before, path.plane_id)?;
            if path_layer != fill_layer {
                return Err(CoreError::InvalidArgument(
                    "fill boundary belongs to another vector layer",
                ));
            }
        }
        let mut next_id = self.next_id;
        let fill_id = take_id(&mut next_id);
        let mut after = before.clone();
        after.vector.fills.push(VectorFill {
            id: fill_id,
            plane_id,
            color,
            boundary_path_ids: boundary_path_ids.to_vec(),
        });
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, fill_id))
    }

    pub fn vector_paths(&self) -> Result<Vec<VectorPathInfo>, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .vector
            .paths
            .iter()
            .map(path_info)
            .collect())
    }

    pub fn vector_fills(&self) -> Result<Vec<VectorFillInfo>, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .vector
            .fills
            .iter()
            .map(|fill| VectorFillInfo {
                id: fill.id,
                plane_id: fill.plane_id,
                color: fill.color,
                boundary_path_ids: fill.boundary_path_ids.clone(),
            })
            .collect())
    }

    pub fn vector_erase(
        &mut self,
        plane_id: u64,
        point: PointF32,
        radius: f32,
        mode: VectorEraseMode,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if !point.x.is_finite()
            || !point.y.is_finite()
            || !radius.is_finite()
            || radius <= 0.0
            || radius > MAX_WIDTH
        {
            return Err(CoreError::InvalidArgument("vector eraser input is invalid"));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        ensure_vector_stroke_plane(&before, plane_id, true)?;
        let touch = (f64::from(point.x), f64::from(point.y));
        let mut next_id = self.next_id;
        let mut replacements = BTreeMap::<u64, Vec<VectorPath>>::new();
        let mut changed_ids = BTreeSet::new();
        for path in before
            .vector
            .paths
            .iter()
            .filter(|path| path.plane_id == plane_id)
        {
            let Some(touch_t) = closest_path_parameter(path, touch, f64::from(radius)) else {
                continue;
            };
            let pieces = match mode {
                VectorEraseMode::WholePath => Vec::new(),
                VectorEraseMode::Partial => {
                    let Some((start, end)) = eraser_interval(path, touch, f64::from(radius)) else {
                        continue;
                    };
                    remaining_pieces(path, start, end, &mut next_id)
                }
                VectorEraseMode::ToIntersection => {
                    let mut intersections = Vec::new();
                    for other in before
                        .vector
                        .paths
                        .iter()
                        .filter(|other| other.id != path.id && other.plane_id == path.plane_id)
                    {
                        intersections.extend(path_intersections(path, other));
                    }
                    intersections.sort_by(f64::total_cmp);
                    intersections.dedup_by(|left, right| (*left - *right).abs() < 1.0e-7);
                    let start = intersections
                        .iter()
                        .copied()
                        .rfind(|value| *value < touch_t)
                        .unwrap_or(0.0);
                    let end = intersections
                        .iter()
                        .copied()
                        .find(|value| *value > touch_t)
                        .unwrap_or(path.segments.len() as f64);
                    remaining_pieces(path, start, end, &mut next_id)
                }
            };
            replacements.insert(path.id, pieces);
            changed_ids.insert(path.id);
        }
        if replacements.is_empty() {
            return Ok(self.noop_outcome());
        }
        let mut after = before.clone();
        let mut paths = Vec::new();
        for path in &after.vector.paths {
            if let Some(pieces) = replacements.get(&path.id) {
                paths.extend(pieces.iter().cloned());
            } else {
                paths.push(path.clone());
            }
        }
        after.vector.paths = paths;
        after.vector.fills.retain(|fill| {
            !fill
                .boundary_path_ids
                .iter()
                .any(|path_id| changed_ids.contains(path_id))
        });
        after.vector.ensure_limits()?;
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok(outcome)
    }

    pub fn vector_connect(
        &mut self,
        plane_id: u64,
        maximum_gap: f32,
    ) -> Result<(DispatchOutcome, Option<u64>), CoreError> {
        self.ensure_no_active_stroke()?;
        if !maximum_gap.is_finite() || maximum_gap <= 0.0 || maximum_gap > MAX_WIDTH {
            return Err(CoreError::InvalidArgument("vector connect gap is invalid"));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        ensure_vector_stroke_plane(&before, plane_id, true)?;
        let paths: Vec<_> = before
            .vector
            .paths
            .iter()
            .filter(|path| path.plane_id == plane_id && !path.closed)
            .collect();
        let mut best: Option<(f64, u64, bool, u64, bool)> = None;
        for (left_index, left) in paths.iter().enumerate() {
            for right in &paths[left_index + 1..] {
                for left_end in [false, true] {
                    if !endpoint_is_unconnected(&paths, left.id, endpoint(left, left_end)) {
                        continue;
                    }
                    for right_end in [false, true] {
                        if !endpoint_is_unconnected(&paths, right.id, endpoint(right, right_end)) {
                            continue;
                        }
                        let a = endpoint(left, left_end);
                        let b = endpoint(right, right_end);
                        let distance = squared_distance(fixed_xy(a), fixed_xy(b));
                        let key = (distance, left.id, left_end, right.id, right_end);
                        if distance <= f64::from(maximum_gap).powi(2)
                            && best.is_none_or(|candidate| key < candidate)
                        {
                            best = Some(key);
                        }
                    }
                }
            }
        }
        let Some((_, left_id, left_end, right_id, right_end)) = best else {
            return Ok((self.noop_outcome(), None));
        };
        before.vector.ensure_additional_limits(1, 0, 1, 0)?;
        let left = before
            .vector
            .paths
            .iter()
            .find(|path| path.id == left_id)
            .expect("selected connect path exists");
        let right = before
            .vector
            .paths
            .iter()
            .find(|path| path.id == right_id)
            .expect("selected connect path exists");
        let start = endpoint(left, left_end);
        let end = endpoint(right, right_end);
        let start_width = endpoint_width(left, left_end);
        let end_width = endpoint_width(right, right_end);
        let mut next_id = self.next_id;
        let connector_id = take_id(&mut next_id);
        let mut after = before.clone();
        after.vector.paths.push(VectorPath {
            id: connector_id,
            plane_id,
            color: left.color,
            closed: false,
            segments: vec![line_segment(start, end, start_width, end_width)],
        });
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, Some(connector_id)))
    }

    pub fn vector_correct_width(
        &mut self,
        path_ids: &[u64],
        mode: VectorWidthMode,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if path_ids.is_empty() {
            return Err(CoreError::InvalidArgument("no vector paths were selected"));
        }
        let transform = width_transform(mode)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let selected: BTreeSet<_> = path_ids.iter().copied().collect();
        if selected.len() != path_ids.len()
            || selected
                .iter()
                .any(|id| !before.vector.paths.iter().any(|path| path.id == *id))
        {
            return Err(CoreError::InvalidArgument(
                "vector path selection is invalid",
            ));
        }
        let mut after = before.clone();
        for path in after
            .vector
            .paths
            .iter_mut()
            .filter(|path| selected.contains(&path.id))
        {
            ensure_vector_stroke_plane(&before, path.plane_id, true)?;
            for segment in &mut path.segments {
                segment.width_start_milli = transform(segment.width_start_milli)?;
                segment.width_end_milli = transform(segment.width_end_milli)?;
            }
        }
        if after == before {
            return Ok(self.noop_outcome());
        }
        self.commit_document_edit(before, after)
    }

    pub fn vector_select(
        &self,
        bounds: RectI32,
        mode: VectorSelectionMode,
    ) -> Result<VectorSelectionResult, CoreError> {
        if bounds.width <= 0 || bounds.height <= 0 {
            return Err(CoreError::InvalidArgument(
                "vector selection bounds are empty",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let right = bounds
            .x
            .checked_add(bounds.width)
            .ok_or(CoreError::InvalidArgument(
                "vector selection bounds overflow",
            ))?;
        let bottom = bounds
            .y
            .checked_add(bounds.height)
            .ok_or(CoreError::InvalidArgument(
                "vector selection bounds overflow",
            ))?;
        let rect = (
            f64::from(bounds.x),
            f64::from(bounds.y),
            f64::from(right),
            f64::from(bottom),
        );
        let mut result = VectorSelectionResult::default();
        if mode == VectorSelectionMode::Fill {
            let center = ((rect.0 + rect.2) * 0.5, (rect.1 + rect.3) * 0.5);
            for fill in &document.vector.fills {
                if point_in_fill(&document.vector, fill, center) {
                    result.fill_ids.push(fill.id);
                }
            }
            return Ok(result);
        }
        if mode == VectorSelectionMode::FillBoundary {
            let center = ((rect.0 + rect.2) * 0.5, (rect.1 + rect.3) * 0.5);
            let mut selected = BTreeSet::new();
            for fill in &document.vector.fills {
                if point_in_fill(&document.vector, fill, center) {
                    selected.extend(fill.boundary_path_ids.iter().copied());
                }
            }
            for path_id in selected {
                result.path_ranges.push(full_selection(path_id));
            }
            return Ok(result);
        }
        for path in &document.vector.paths {
            let samples = flatten_path(path, FLATTEN_STEPS);
            let mut inside: Vec<_> = samples
                .iter()
                .filter(|sample| point_in_rect(sample.point, rect))
                .map(|sample| sample.parameter)
                .collect();
            for pair in samples.windows(2) {
                for fraction in segment_rect_intersections(pair[0].point, pair[1].point, rect) {
                    inside.push(lerp(pair[0].parameter, pair[1].parameter, fraction));
                }
            }
            inside.sort_by(f64::total_cmp);
            let touched = !inside.is_empty();
            let all_inside = samples
                .iter()
                .all(|sample| point_in_rect(sample.point, rect));
            let range = match mode {
                VectorSelectionMode::FullyContained if all_inside => {
                    Some((0.0, path_length_t(path)))
                }
                VectorSelectionMode::CutBySelection if touched => inside
                    .first()
                    .zip(inside.last())
                    .map(|(start, end)| (*start, *end)),
                VectorSelectionMode::ToIntersection if touched => {
                    let touch = ((rect.0 + rect.2) * 0.5, (rect.1 + rect.3) * 0.5);
                    let touch_t = closest_path_parameter(path, touch, f32::MAX as f64)
                        .unwrap_or(path_length_t(path) * 0.5);
                    let mut intersections = document
                        .vector
                        .paths
                        .iter()
                        .filter(|other| other.id != path.id && other.plane_id == path.plane_id)
                        .flat_map(|other| path_intersections(path, other))
                        .collect::<Vec<_>>();
                    intersections.sort_by(f64::total_cmp);
                    Some((
                        intersections
                            .iter()
                            .copied()
                            .rfind(|value| *value < touch_t)
                            .unwrap_or(0.0),
                        intersections
                            .iter()
                            .copied()
                            .find(|value| *value > touch_t)
                            .unwrap_or(path_length_t(path)),
                    ))
                }
                VectorSelectionMode::Touching
                | VectorSelectionMode::Line
                | VectorSelectionMode::WholeLine
                    if touched =>
                {
                    Some((0.0, path_length_t(path)))
                }
                _ => None,
            };
            if let Some((start, end)) = range {
                result.path_ranges.push(selection_range(path, start, end));
            }
        }
        Ok(result)
    }

    pub fn rasterize_vector_layer(
        &self,
        layer_id: u64,
        scale: u32,
        antialias: bool,
    ) -> Result<VectorRaster, CoreError> {
        let (width, height, stride_bytes, _) = self.vector_raster_layout(layer_id, scale)?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let layer = document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .expect("layout validated the vector layer");
        let mut pixels = vec![0_u8; stride_bytes as usize * height as usize];
        let fills: Vec<_> = document
            .vector
            .fills
            .iter()
            .filter_map(|fill| {
                let plane = layer
                    .planes
                    .iter()
                    .find(|plane| plane.id == fill.plane_id && plane.visible)?;
                let boundaries = fill
                    .boundary_path_ids
                    .iter()
                    .filter_map(|path_id| {
                        document
                            .vector
                            .paths
                            .iter()
                            .find(|path| path.id == *path_id)
                            .map(|path| flatten_path(path, RASTER_STEPS))
                    })
                    .collect::<Vec<_>>();
                let bounds = sampled_bounds(boundaries.iter().flatten().copied(), 0.0)?;
                Some((
                    display_color(fill.color, layer.opacity_milli, plane.opacity_milli),
                    bounds,
                    boundaries,
                ))
            })
            .collect();
        let mut paths = Vec::new();
        for plane_kind in [PlaneType::ColorTrace, PlaneType::VectorMainLine] {
            for plane in layer
                .planes
                .iter()
                .filter(|plane| plane.kind == plane_kind && plane.visible)
            {
                for path in document
                    .vector
                    .paths
                    .iter()
                    .filter(|path| path.plane_id == plane.id)
                {
                    let samples = flatten_path(path, RASTER_STEPS);
                    let padding = samples
                        .iter()
                        .map(|sample| sample.width * 0.5)
                        .fold(0.0_f64, f64::max);
                    if let Some(bounds) = sampled_bounds(samples.iter().copied(), padding) {
                        paths.push((
                            display_color(path.color, layer.opacity_milli, plane.opacity_milli),
                            bounds,
                            samples,
                        ));
                    }
                }
            }
        }
        let offsets: &[(f64, f64)] = if antialias {
            &[
                (0.125, 0.125),
                (0.375, 0.125),
                (0.625, 0.125),
                (0.875, 0.125),
                (0.125, 0.375),
                (0.375, 0.375),
                (0.625, 0.375),
                (0.875, 0.375),
                (0.125, 0.625),
                (0.375, 0.625),
                (0.625, 0.625),
                (0.875, 0.625),
                (0.125, 0.875),
                (0.375, 0.875),
                (0.625, 0.875),
                (0.875, 0.875),
            ]
        } else {
            &[(0.5, 0.5)]
        };
        for y in 0..height {
            for x in 0..width {
                let mut accumulated_premultiplied = [0_u64; 3];
                let mut accumulated_alpha = 0_u64;
                for offset in offsets {
                    let sample = (
                        (f64::from(x) + offset.0) / f64::from(scale),
                        (f64::from(y) + offset.1) / f64::from(scale),
                    );
                    let mut value = [0_u8; 4];
                    for (color, bounds, boundaries) in &fills {
                        if point_in_rect(sample, *bounds)
                            && point_in_sampled_fill(boundaries, sample)
                        {
                            value = source_over_rgba(value, *color);
                        }
                    }
                    for (color, bounds, samples) in &paths {
                        if point_in_rect(sample, *bounds)
                            && point_on_sampled_stroke(samples, sample)
                        {
                            value = source_over_rgba(value, *color);
                        }
                    }
                    accumulated_alpha += u64::from(value[3]);
                    for channel in 0..3 {
                        accumulated_premultiplied[channel] +=
                            u64::from(value[channel]) * u64::from(value[3]);
                    }
                }
                let offset = y as usize * stride_bytes as usize + x as usize * 4;
                for channel in 0..3 {
                    pixels[offset + channel] = (accumulated_premultiplied[channel]
                        + accumulated_alpha / 2)
                        .checked_div(accumulated_alpha)
                        .unwrap_or(0) as u8;
                }
                pixels[offset + 3] =
                    ((accumulated_alpha + offsets.len() as u64 / 2) / offsets.len() as u64) as u8;
            }
        }
        Ok(VectorRaster {
            width,
            height,
            stride_bytes,
            pixels,
        })
    }

    /// Rasterizes a vector-coloring layer into a new RGBA8 raster layer as one
    /// document transaction. The source vector geometry remains unchanged.
    pub fn rasterize_vector_layer_to_document(
        &mut self,
        layer_id: u64,
        antialias: bool,
        name: &str,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        let rasterized = self.rasterize_vector_layer(layer_id, 1, antialias)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let mut raster = TileRaster::new(
            rasterized.width,
            rasterized.height,
            PixelFormat::StraightRgba8,
        )?;
        for y in 0..rasterized.height {
            for x in 0..rasterized.width {
                let offset = y as usize * rasterized.stride_bytes as usize + x as usize * 4;
                let color = [
                    rasterized.pixels[offset],
                    rasterized.pixels[offset + 1],
                    rasterized.pixels[offset + 2],
                    rasterized.pixels[offset + 3],
                ];
                if color[3] != 0 {
                    raster.set_pixel(x, y, PixelValue::Rgba(color), revision)?;
                }
            }
        }
        let new_layer_id = self.allocate_id();
        let new_plane_id = self.allocate_id();
        let mut after = before.clone();
        after.layers.push(LayerNode {
            id: new_layer_id,
            kind: LayerKind::Raster,
            name: unique_layer_name(&after.layers, name),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes: vec![PlaneNode {
                id: new_plane_id,
                kind: PlaneType::Raster,
                name: "Rasterized".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster,
            }],
        });
        after.active_layer_id = new_layer_id;
        after.active_plane_id = new_plane_id;
        let outcome = self.commit_document_edit_with_revision(before, after, revision)?;
        Ok((outcome, new_layer_id))
    }

    pub fn vector_raster_layout(
        &self,
        layer_id: u64,
        scale: u32,
    ) -> Result<(u32, u32, u32, u64), CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id && layer.kind == LayerKind::VectorColoring)
            .ok_or(CoreError::InvalidArgument("vector layer ID does not exist"))?;
        if !(1..=16).contains(&scale) {
            return Err(CoreError::InvalidArgument(
                "vector raster scale is outside bounds",
            ));
        }
        let width = document
            .width
            .checked_mul(scale)
            .ok_or(CoreError::InvalidArgument("vector raster width overflows"))?;
        let height = document
            .height
            .checked_mul(scale)
            .ok_or(CoreError::InvalidArgument("vector raster height overflows"))?;
        let pixel_count = u64::from(width) * u64::from(height);
        if pixel_count > MAX_VECTOR_RASTER_PIXELS {
            return Err(CoreError::InvalidArgument(
                "vector raster exceeds its pixel bound",
            ));
        }
        let stride_bytes = width
            .checked_mul(4)
            .ok_or(CoreError::InvalidArgument("vector raster stride overflows"))?;
        Ok((width, height, stride_bytes, pixel_count * 4))
    }

    pub fn vectorize_raster_plane(
        &mut self,
        source_plane_id: u64,
        target_vector_layer_id: u64,
        alpha_threshold: u8,
    ) -> Result<(DispatchOutcome, Vec<u64>), CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let source = before
            .plane_by_id(source_plane_id)
            .ok_or(CoreError::InvalidArgument(
                "source raster plane does not exist",
            ))?;
        if !matches!(source.kind, PlaneType::Color | PlaneType::Raster)
            || !matches!(source.raster.format(), PixelFormat::StraightRgba8)
        {
            return Err(CoreError::InvalidArgument(
                "raster-to-vector conversion requires an RGBA8 raster plane",
            ));
        }
        let target = before
            .layers
            .iter()
            .find(|layer| {
                layer.id == target_vector_layer_id && layer.kind == LayerKind::VectorColoring
            })
            .ok_or(CoreError::InvalidArgument(
                "target vector layer does not exist",
            ))?;
        let trace_plane = target
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::ColorTrace)
            .map(|plane| plane.id)
            .ok_or(CoreError::InvalidState(
                "target vector trace plane is missing",
            ))?;
        let fill_plane = target
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::VectorFill)
            .map(|plane| plane.id)
            .ok_or(CoreError::InvalidState(
                "target vector fill plane is missing",
            ))?;
        ensure_vector_stroke_plane(&before, trace_plane, true)?;
        ensure_vector_fill_plane(&before, fill_plane, true)?;
        let run_capacity = before.vector.raster_vectorize_run_capacity()?;
        let mut runs = Vec::new();
        for y in 0..before.height {
            let mut x = 0;
            while x < before.width {
                let PixelValue::Rgba(color) = source.raster.pixel(x, y)? else {
                    return Err(CoreError::InvalidState(
                        "RGBA8 raster returned another depth",
                    ));
                };
                if color[3] == 0 || color[3] < alpha_threshold {
                    x += 1;
                    continue;
                }
                let start = x;
                x += 1;
                while x < before.width && source.raster.pixel(x, y)? == PixelValue::Rgba(color) {
                    x += 1;
                }
                runs.push((start, x, y, color));
                if runs.len() > run_capacity {
                    return Err(CoreError::InvalidState(
                        "raster-to-vector conversion exceeds object limits",
                    ));
                }
            }
        }
        if runs.is_empty() {
            return Ok((self.noop_outcome(), Vec::new()));
        }
        let mut after = before.clone();
        let mut fill_ids = Vec::with_capacity(runs.len());
        let mut next_id = self.next_id;
        for (start, end, y, color) in runs {
            let path_id = take_id(&mut next_id);
            let fill_id = take_id(&mut next_id);
            let points = [
                fixed_xy_point(f64::from(start), f64::from(y)),
                fixed_xy_point(f64::from(end), f64::from(y)),
                fixed_xy_point(f64::from(end), f64::from(y + 1)),
                fixed_xy_point(f64::from(start), f64::from(y + 1)),
            ];
            let width = 1;
            after.vector.paths.push(VectorPath {
                id: path_id,
                plane_id: trace_plane,
                color: PixelValue::Rgba([0, 0, 0, 0]),
                closed: true,
                segments: vec![
                    line_segment(points[0], points[1], width, width),
                    line_segment(points[1], points[2], width, width),
                    line_segment(points[2], points[3], width, width),
                    line_segment(points[3], points[0], width, width),
                ],
            });
            after.vector.fills.push(VectorFill {
                id: fill_id,
                plane_id: fill_plane,
                color: PixelValue::Rgba(color),
                boundary_path_ids: vec![path_id],
            });
            fill_ids.push(fill_id);
        }
        after.vector.ensure_limits()?;
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, fill_ids))
    }
}
