use super::{
    CellDocument, Core, CoreError, DispatchOutcome, LayerKind, PixelFormat, PixelValue, PlaneType,
    PointF32, RectI32,
};
use inkpod_format::{
    FileM5Metadata, FileVectorFill, FileVectorPath, FileVectorPoint, FileVectorSegment,
    MAX_VECTOR_BOUNDARIES, MAX_VECTOR_FILLS, MAX_VECTOR_PATHS, MAX_VECTOR_SEGMENTS,
};
use inkpod_image::{
    VECTOR_UNITS_PER_PIXEL as UNITS_PER_PIXEL, VectorFixedCubic as VectorSegment,
    VectorFixedPoint as FixedPoint, VectorFlatSample as FlatSample, flatten_vector_path,
    sub_vector_cubic, vector_distance_to_segment, vector_fixed_xy, vector_lerp, vector_line_cubic,
    vector_line_intersection, vector_path_intersections, vector_point_at, vector_source_over,
    vector_squared_distance,
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_COORDINATE: f64 = 2_000_000.0;
const MAX_WIDTH: f32 = 4_096.0;
const FLATTEN_STEPS: usize = 64;
const RASTER_STEPS: usize = 32;
const MAX_VECTOR_RASTER_PIXELS: u64 = 16_777_216;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorCubicSegment {
    pub p0: PointF32,
    pub p1: PointF32,
    pub p2: PointF32,
    pub p3: PointF32,
    pub width_start: f32,
    pub width_end: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorPathInput {
    pub segments: Vec<VectorCubicSegment>,
    pub color: PixelValue,
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorPathInfo {
    pub id: u64,
    pub plane_id: u64,
    pub segments: Vec<VectorCubicSegment>,
    pub color: PixelValue,
    pub closed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorFillInfo {
    pub id: u64,
    pub plane_id: u64,
    pub color: PixelValue,
    pub boundary_path_ids: Vec<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VectorEraseMode {
    Partial,
    ToIntersection,
    WholePath,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VectorWidthMode {
    Add(f32),
    Subtract(f32),
    Scale(f32),
    Constant(f32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VectorSelectionMode {
    CutBySelection,
    Touching,
    FullyContained,
    Line,
    WholeLine,
    ToIntersection,
    FillBoundary,
    Fill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VectorSelectionRange {
    pub path_id: u64,
    pub start_million: u32,
    pub end_million: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VectorSelectionResult {
    pub path_ranges: Vec<VectorSelectionRange>,
    pub fill_ids: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorRaster {
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderVectorSegment {
    pub path_id: u64,
    pub plane_id: u64,
    pub z_order: u32,
    pub segment_index: u32,
    pub segment_count: u32,
    pub color_rgba: [u8; 4],
    pub closed: bool,
    pub stroke_visible: bool,
    pub cubic: VectorCubicSegment,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderVectorFill {
    pub fill_id: u64,
    pub plane_id: u64,
    pub z_order: u32,
    pub color_rgba: [u8; 4],
    pub boundary_path_ids: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VectorPath {
    id: u64,
    plane_id: u64,
    color: PixelValue,
    closed: bool,
    segments: Vec<VectorSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VectorFill {
    id: u64,
    plane_id: u64,
    color: PixelValue,
    boundary_path_ids: Vec<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VectorState {
    paths: Vec<VectorPath>,
    fills: Vec<VectorFill>,
}

impl VectorState {
    pub(crate) fn to_file(&self, has_vector_layer: bool) -> Option<FileM5Metadata> {
        has_vector_layer.then(|| FileM5Metadata {
            paths: self
                .paths
                .iter()
                .map(|path| FileVectorPath {
                    id: path.id,
                    plane_id: path.plane_id,
                    color: path.color,
                    closed: path.closed,
                    segments: path
                        .segments
                        .iter()
                        .map(|segment| FileVectorSegment {
                            p0: file_point(segment.p0),
                            p1: file_point(segment.p1),
                            p2: file_point(segment.p2),
                            p3: file_point(segment.p3),
                            width_start_milli: segment.width_start_milli,
                            width_end_milli: segment.width_end_milli,
                        })
                        .collect(),
                })
                .collect(),
            fills: self
                .fills
                .iter()
                .map(|fill| FileVectorFill {
                    id: fill.id,
                    plane_id: fill.plane_id,
                    color: fill.color,
                    boundary_path_ids: fill.boundary_path_ids.clone(),
                })
                .collect(),
        })
    }

    pub(crate) fn from_file(metadata: Option<&FileM5Metadata>) -> Self {
        metadata.map_or_else(Self::default, |metadata| Self {
            paths: metadata
                .paths
                .iter()
                .map(|path| VectorPath {
                    id: path.id,
                    plane_id: path.plane_id,
                    color: path.color,
                    closed: path.closed,
                    segments: path
                        .segments
                        .iter()
                        .map(|segment| VectorSegment {
                            p0: fixed_file_point(segment.p0),
                            p1: fixed_file_point(segment.p1),
                            p2: fixed_file_point(segment.p2),
                            p3: fixed_file_point(segment.p3),
                            width_start_milli: segment.width_start_milli,
                            width_end_milli: segment.width_end_milli,
                        })
                        .collect(),
                })
                .collect(),
            fills: metadata
                .fills
                .iter()
                .map(|fill| VectorFill {
                    id: fill.id,
                    plane_id: fill.plane_id,
                    color: fill.color,
                    boundary_path_ids: fill.boundary_path_ids.clone(),
                })
                .collect(),
        })
    }

    pub(crate) fn maximum_id(&self) -> u64 {
        self.paths
            .iter()
            .map(|path| path.id)
            .chain(self.fills.iter().map(|fill| fill.id))
            .max()
            .unwrap_or(0)
    }

    fn object_counts(&self) -> Result<(usize, usize, usize, usize), CoreError> {
        let segment_count = self.paths.iter().try_fold(0_usize, |count, path| {
            count.checked_add(path.segments.len())
        });
        let boundary_count = self.fills.iter().try_fold(0_usize, |count, fill| {
            count.checked_add(fill.boundary_path_ids.len())
        });
        match (segment_count, boundary_count) {
            (Some(segment_count), Some(boundary_count)) => Ok((
                self.paths.len(),
                self.fills.len(),
                segment_count,
                boundary_count,
            )),
            _ => Err(CoreError::InvalidState("vector object count overflows")),
        }
    }

    pub(crate) fn ensure_additional_limits(
        &self,
        additional_paths: usize,
        additional_fills: usize,
        additional_segments: usize,
        additional_boundaries: usize,
    ) -> Result<(), CoreError> {
        let (paths, fills, segments, boundaries) = self.object_counts()?;
        if paths
            .checked_add(additional_paths)
            .is_none_or(|count| count > MAX_VECTOR_PATHS)
            || fills
                .checked_add(additional_fills)
                .is_none_or(|count| count > MAX_VECTOR_FILLS)
            || segments
                .checked_add(additional_segments)
                .is_none_or(|count| count > MAX_VECTOR_SEGMENTS)
            || boundaries
                .checked_add(additional_boundaries)
                .is_none_or(|count| count > MAX_VECTOR_BOUNDARIES)
        {
            return Err(CoreError::InvalidState("vector object limit reached"));
        }
        Ok(())
    }

    pub(crate) fn ensure_limits(&self) -> Result<(), CoreError> {
        self.ensure_additional_limits(0, 0, 0, 0)
    }

    fn raster_vectorize_run_capacity(&self) -> Result<usize, CoreError> {
        let (paths, fills, segments, boundaries) = self.object_counts()?;
        if paths > MAX_VECTOR_PATHS
            || fills > MAX_VECTOR_FILLS
            || segments > MAX_VECTOR_SEGMENTS
            || boundaries > MAX_VECTOR_BOUNDARIES
        {
            return Err(CoreError::InvalidState("vector object limit reached"));
        }
        Ok((MAX_VECTOR_PATHS - paths)
            .min(MAX_VECTOR_FILLS - fills)
            .min((MAX_VECTOR_SEGMENTS - segments) / 4)
            .min(MAX_VECTOR_BOUNDARIES - boundaries))
    }

    pub(crate) fn remove_plane(&mut self, plane_id: u64) {
        let removed: BTreeSet<_> = self
            .paths
            .iter()
            .filter(|path| path.plane_id == plane_id)
            .map(|path| path.id)
            .collect();
        self.paths.retain(|path| path.plane_id != plane_id);
        self.fills.retain(|fill| {
            fill.plane_id != plane_id
                && !fill
                    .boundary_path_ids
                    .iter()
                    .any(|path_id| removed.contains(path_id))
        });
    }

    pub(crate) fn remove_layer(&mut self, document: &CellDocument, layer_id: u64) {
        if let Some(layer) = document.layers.iter().find(|layer| layer.id == layer_id) {
            for plane in &layer.planes {
                self.remove_plane(plane.id);
            }
        }
    }

    pub(crate) fn duplicate_planes(&mut self, plane_map: &BTreeMap<u64, u64>, next_id: &mut u64) {
        let source_paths: Vec<_> = self
            .paths
            .iter()
            .filter(|path| plane_map.contains_key(&path.plane_id))
            .cloned()
            .collect();
        let mut path_map = BTreeMap::new();
        for mut path in source_paths {
            let source_id = path.id;
            path.id = take_id(next_id);
            path.plane_id = plane_map[&path.plane_id];
            path_map.insert(source_id, path.id);
            self.paths.push(path);
        }
        let source_fills: Vec<_> = self
            .fills
            .iter()
            .filter(|fill| plane_map.contains_key(&fill.plane_id))
            .cloned()
            .collect();
        for mut fill in source_fills {
            fill.id = take_id(next_id);
            fill.plane_id = plane_map[&fill.plane_id];
            fill.boundary_path_ids = fill
                .boundary_path_ids
                .iter()
                .filter_map(|path_id| path_map.get(path_id).copied())
                .collect();
            if !fill.boundary_path_ids.is_empty() {
                self.fills.push(fill);
            }
        }
    }

    pub(crate) fn reassign_plane(&mut self, old_plane_id: u64, new_plane_id: u64) {
        for path in self
            .paths
            .iter_mut()
            .filter(|path| path.plane_id == old_plane_id)
        {
            path.plane_id = new_plane_id;
        }
        for fill in self
            .fills
            .iter_mut()
            .filter(|fill| fill.plane_id == old_plane_id)
        {
            fill.plane_id = new_plane_id;
        }
    }

    pub(crate) fn render_items(
        &self,
        document: &CellDocument,
    ) -> (Vec<RenderVectorSegment>, Vec<RenderVectorFill>) {
        let mut segments = Vec::new();
        let mut fills = Vec::new();
        for (z_order, layer) in document.layers.iter().rev().enumerate() {
            if !layer.visible || layer.kind != LayerKind::VectorColoring {
                continue;
            }
            for fill in self.fills.iter().filter(|fill| {
                layer.planes.iter().any(|plane| {
                    plane.id == fill.plane_id
                        && plane.visible
                        && plane.kind == PlaneType::VectorFill
                })
            }) {
                let plane = layer
                    .planes
                    .iter()
                    .find(|plane| plane.id == fill.plane_id)
                    .expect("matched vector fill plane");
                fills.push(RenderVectorFill {
                    fill_id: fill.id,
                    plane_id: fill.plane_id,
                    z_order: z_order as u32,
                    color_rgba: display_color(fill.color, layer.opacity_milli, plane.opacity_milli),
                    boundary_path_ids: fill.boundary_path_ids.clone(),
                });
            }
            // Match raster coloring semantics: color-trace planes paint first
            // and the protected main-line plane paints last within the layer.
            for plane_kind in [PlaneType::ColorTrace, PlaneType::VectorMainLine] {
                for plane in layer.planes.iter().filter(|plane| plane.kind == plane_kind) {
                    for path in self.paths.iter().filter(|path| path.plane_id == plane.id) {
                        let color =
                            display_color(path.color, layer.opacity_milli, plane.opacity_milli);
                        for (index, segment) in path.segments.iter().enumerate() {
                            segments.push(RenderVectorSegment {
                                path_id: path.id,
                                plane_id: path.plane_id,
                                z_order: z_order as u32,
                                segment_index: index as u32,
                                segment_count: path.segments.len() as u32,
                                color_rgba: color,
                                closed: path.closed,
                                stroke_visible: plane.visible,
                                cubic: public_segment(*segment),
                            });
                        }
                    }
                }
            }
        }
        (segments, fills)
    }
}

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

fn fixed_path(id: u64, plane_id: u64, input: VectorPathInput) -> Result<VectorPath, CoreError> {
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
        segments,
    })
}

fn fixed_point(point: PointF32) -> Result<FixedPoint, CoreError> {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || f64::from(point.x).abs() > MAX_COORDINATE
        || f64::from(point.y).abs() > MAX_COORDINATE
    {
        return Err(CoreError::InvalidArgument("vector point is outside bounds"));
    }
    Ok(fixed_xy_point(f64::from(point.x), f64::from(point.y)))
}

fn fixed_xy_point(x: f64, y: f64) -> FixedPoint {
    FixedPoint {
        x_milli: (x * UNITS_PER_PIXEL).round() as i32,
        y_milli: (y * UNITS_PER_PIXEL).round() as i32,
    }
}

fn fixed_width(width: f32) -> Result<u32, CoreError> {
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

fn ensure_vector_stroke_plane(
    document: &CellDocument,
    plane_id: u64,
    editable: bool,
) -> Result<u64, CoreError> {
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

fn ensure_vector_fill_plane(
    document: &CellDocument,
    plane_id: u64,
    editable: bool,
) -> Result<u64, CoreError> {
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

fn vector_layer_for_plane(document: &CellDocument, plane_id: u64) -> Result<u64, CoreError> {
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

fn path_info(path: &VectorPath) -> VectorPathInfo {
    VectorPathInfo {
        id: path.id,
        plane_id: path.plane_id,
        segments: path.segments.iter().copied().map(public_segment).collect(),
        color: path.color,
        closed: path.closed,
    }
}

fn public_segment(segment: VectorSegment) -> VectorCubicSegment {
    VectorCubicSegment {
        p0: public_point(segment.p0),
        p1: public_point(segment.p1),
        p2: public_point(segment.p2),
        p3: public_point(segment.p3),
        width_start: segment.width_start_milli as f32 / 1_000.0,
        width_end: segment.width_end_milli as f32 / 1_000.0,
    }
}

fn public_point(point: FixedPoint) -> PointF32 {
    PointF32 {
        x: point.x_milli as f32 / 1_000.0,
        y: point.y_milli as f32 / 1_000.0,
    }
}

fn file_point(point: FixedPoint) -> FileVectorPoint {
    FileVectorPoint {
        x_milli: point.x_milli,
        y_milli: point.y_milli,
    }
}

fn fixed_file_point(point: FileVectorPoint) -> FixedPoint {
    FixedPoint {
        x_milli: point.x_milli,
        y_milli: point.y_milli,
    }
}

fn rgba8(color: PixelValue) -> [u8; 4] {
    match color {
        PixelValue::Rgba(value) => value,
        PixelValue::Rgba16(value) => value.map(|channel| ((u32::from(channel) + 128) / 257) as u8),
        _ => [0, 0, 0, 0],
    }
}

fn display_color(color: PixelValue, layer_opacity: u32, plane_opacity: u32) -> [u8; 4] {
    let mut value = rgba8(color);
    value[3] = ((u64::from(value[3]) * u64::from(layer_opacity) * u64::from(plane_opacity)
        + 500_000)
        / 1_000_000) as u8;
    value
}

fn flatten_path(path: &VectorPath, steps: usize) -> Vec<FlatSample> {
    flatten_vector_path(&path.segments, steps)
}

fn closest_path_parameter(
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

fn eraser_interval(path: &VectorPath, center: (f64, f64), radius: f64) -> Option<(f64, f64)> {
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

fn refine_circle_boundary(
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

fn point_at_path(path: &VectorPath, parameter: f64) -> Option<(f64, f64)> {
    vector_point_at(&path.segments, parameter)
}

fn remaining_pieces(path: &VectorPath, start: f64, end: f64, next_id: &mut u64) -> Vec<VectorPath> {
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
                let id = *next_id;
                *next_id = next_id.saturating_add(1).max(1);
                id
            };
            suffix.closed = false;
            pieces.push(suffix);
        }
    }
    pieces
}

fn subpath(path: &VectorPath, start: f64, end: f64) -> Option<VectorPath> {
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
        segments,
    })
}

fn subsegment(segment: VectorSegment, start: f64, end: f64) -> VectorSegment {
    sub_vector_cubic(segment, start, end)
}

fn path_intersections(left: &VectorPath, right: &VectorPath) -> Vec<f64> {
    vector_path_intersections(&left.segments, &right.segments, FLATTEN_STEPS)
}

fn line_intersection(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
) -> Option<(f64, f64)> {
    vector_line_intersection(a, b, c, d)
}

fn endpoint(path: &VectorPath, end: bool) -> FixedPoint {
    if end {
        path.segments.last().expect("path has segments").p3
    } else {
        path.segments[0].p0
    }
}

fn endpoint_width(path: &VectorPath, end: bool) -> u32 {
    if end {
        path.segments
            .last()
            .expect("path has segments")
            .width_end_milli
    } else {
        path.segments[0].width_start_milli
    }
}

fn endpoint_is_unconnected(paths: &[&VectorPath], path_id: u64, point: FixedPoint) -> bool {
    paths.iter().all(|other| {
        other.id == path_id
            || [endpoint(other, false), endpoint(other, true)]
                .into_iter()
                .all(|other_point| {
                    squared_distance(fixed_xy(point), fixed_xy(other_point)) > 1.0e-12
                })
    })
}

fn line_segment(
    start: FixedPoint,
    end: FixedPoint,
    start_width: u32,
    end_width: u32,
) -> VectorSegment {
    vector_line_cubic(start, end, start_width, end_width)
}

fn width_transform(
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

fn selection_range(path: &VectorPath, start: f64, end: f64) -> VectorSelectionRange {
    let total = path_length_t(path);
    VectorSelectionRange {
        path_id: path.id,
        start_million: ((start / total).clamp(0.0, 1.0) * 1_000_000.0).round() as u32,
        end_million: ((end / total).clamp(0.0, 1.0) * 1_000_000.0).round() as u32,
    }
}

fn full_selection(path_id: u64) -> VectorSelectionRange {
    VectorSelectionRange {
        path_id,
        start_million: 0,
        end_million: 1_000_000,
    }
}

fn path_length_t(path: &VectorPath) -> f64 {
    path.segments.len() as f64
}

fn point_in_rect(point: (f64, f64), rect: (f64, f64, f64, f64)) -> bool {
    point.0 >= rect.0 && point.0 <= rect.2 && point.1 >= rect.1 && point.1 <= rect.3
}

fn segment_rect_intersections(
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

fn sampled_bounds(
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

fn point_in_sampled_fill(boundaries: &[Vec<FlatSample>], point: (f64, f64)) -> bool {
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

fn point_on_sampled_stroke(samples: &[FlatSample], point: (f64, f64)) -> bool {
    samples.windows(2).any(|pair| {
        let (distance, fraction) = distance_to_segment(point, pair[0].point, pair[1].point);
        let width = lerp(pair[0].width, pair[1].width, fraction);
        distance <= width * 0.5
    })
}

fn point_in_fill(state: &VectorState, fill: &VectorFill, point: (f64, f64)) -> bool {
    let boundaries = fill
        .boundary_path_ids
        .iter()
        .filter_map(|path_id| state.paths.iter().find(|path| path.id == *path_id))
        .map(|path| flatten_path(path, RASTER_STEPS))
        .collect::<Vec<_>>();
    point_in_sampled_fill(&boundaries, point)
}

fn distance_to_segment(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> (f64, f64) {
    vector_distance_to_segment(point, start, end)
}

fn source_over_rgba(destination: [u8; 4], source: [u8; 4]) -> [u8; 4] {
    vector_source_over(destination, source)
}

fn fixed_xy(point: FixedPoint) -> (f64, f64) {
    vector_fixed_xy(point)
}

fn squared_distance(left: (f64, f64), right: (f64, f64)) -> f64 {
    vector_squared_distance(left, right)
}

fn lerp(left: f64, right: f64, amount: f64) -> f64 {
    vector_lerp(left, right, amount)
}

fn take_id(next_id: &mut u64) -> u64 {
    let id = *next_id;
    *next_id = next_id.saturating_add(1).max(1);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_object_limits_cover_every_persisted_collection() {
        let state = VectorState::default();
        assert!(
            state
                .ensure_additional_limits(
                    MAX_VECTOR_PATHS,
                    MAX_VECTOR_FILLS,
                    MAX_VECTOR_SEGMENTS,
                    MAX_VECTOR_BOUNDARIES,
                )
                .is_ok()
        );
        for additions in [
            (MAX_VECTOR_PATHS + 1, 0, 0, 0),
            (0, MAX_VECTOR_FILLS + 1, 0, 0),
            (0, 0, MAX_VECTOR_SEGMENTS + 1, 0),
            (0, 0, 0, MAX_VECTOR_BOUNDARIES + 1),
        ] {
            assert!(matches!(
                state.ensure_additional_limits(additions.0, additions.1, additions.2, additions.3,),
                Err(CoreError::InvalidState("vector object limit reached"))
            ));
        }
        assert_eq!(state.raster_vectorize_run_capacity().unwrap(), 65_536);
    }
}
