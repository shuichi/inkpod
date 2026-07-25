use super::geometry::*;
use super::*;

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
pub(super) struct VectorPath {
    pub(super) id: u64,
    pub(super) plane_id: u64,
    pub(super) color: PixelValue,
    pub(super) closed: bool,
    pub(super) segments: Vec<VectorSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VectorFill {
    pub(super) id: u64,
    pub(super) plane_id: u64,
    pub(super) color: PixelValue,
    pub(super) boundary_path_ids: Vec<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VectorState {
    pub(super) paths: Vec<VectorPath>,
    pub(super) fills: Vec<VectorFill>,
}

impl VectorState {
    pub(crate) fn transform_coordinates<F>(
        &mut self,
        mut transform: F,
        width_scale: f64,
    ) -> Result<(), CoreError>
    where
        F: FnMut(FixedPoint) -> Result<FixedPoint, CoreError>,
    {
        if !width_scale.is_finite() || width_scale <= 0.0 {
            return Err(CoreError::InvalidArgument(
                "vector width scale must be finite and positive",
            ));
        }
        for path in &mut self.paths {
            for segment in &mut path.segments {
                segment.p0 = transform(segment.p0)?;
                segment.p1 = transform(segment.p1)?;
                segment.p2 = transform(segment.p2)?;
                segment.p3 = transform(segment.p3)?;
                segment.width_start_milli =
                    scaled_vector_width(segment.width_start_milli, width_scale)?;
                segment.width_end_milli =
                    scaled_vector_width(segment.width_end_milli, width_scale)?;
            }
        }
        Ok(())
    }

    pub(crate) fn to_file(&self, has_vector_layer: bool) -> Option<FileVectorMetadata> {
        has_vector_layer.then(|| FileVectorMetadata {
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

    pub(crate) fn from_file(metadata: Option<&FileVectorMetadata>) -> Self {
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

    pub(super) fn raster_vectorize_run_capacity(&self) -> Result<usize, CoreError> {
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

fn scaled_vector_width(width_milli: u32, scale: f64) -> Result<u32, CoreError> {
    let scaled = f64::from(width_milli) * scale;
    if !scaled.is_finite() || scaled < 0.0 || scaled > f64::from(u32::MAX) {
        return Err(CoreError::InvalidArgument(
            "scaled vector width exceeds its range",
        ));
    }
    Ok(scaled.round() as u32)
}
