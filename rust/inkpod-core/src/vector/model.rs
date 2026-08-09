use super::geometry::*;
use super::*;

/// A cubic vector segment stored in document coordinates.
///
/// View zoom, pan, flip, and OS DPI are applied only by the renderer and never
/// mutate these control points or document-space widths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorCubicSegment {
    /// Segment start point in document pixels.
    pub p0: PointF32,
    /// First cubic control point in document pixels.
    pub p1: PointF32,
    /// Second cubic control point in document pixels.
    pub p2: PointF32,
    /// Segment end point in document pixels.
    pub p3: PointF32,
    /// Positive stroke width at `p0`, in document pixels.
    pub width_start: f32,
    /// Positive stroke width at `p3`, in document pixels.
    pub width_end: f32,
}

#[derive(Clone, Debug, PartialEq)]
/// Caller-owned geometry and appearance for a new vector path.
pub struct VectorPathInput {
    /// Ordered, contiguous cubic segments in document coordinates.
    pub segments: Vec<VectorCubicSegment>,
    /// Straight-alpha path color.
    pub color: PixelValue,
    /// Whether the final point connects back to the first.
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq)]
/// Public metadata and owned geometry for one vector path.
pub struct VectorPathInfo {
    /// Stable path ID, valid until the path is erased or its layer is removed.
    pub id: u64,
    /// Stable owning plane ID.
    pub plane_id: u64,
    /// Ordered cubic segments in document coordinates.
    pub segments: Vec<VectorCubicSegment>,
    /// Straight-alpha path color.
    pub color: PixelValue,
    /// Whether the path is closed.
    pub closed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Public metadata for a vector fill bounded by paths.
pub struct VectorFillInfo {
    /// Stable fill ID, valid until the fill is deleted or its layer is removed.
    pub id: u64,
    /// Stable owning plane ID.
    pub plane_id: u64,
    /// Straight-alpha fill color.
    pub color: PixelValue,
    /// Ordered stable IDs of boundary paths.
    pub boundary_path_ids: Vec<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Extent of a vector eraser operation.
pub enum VectorEraseMode {
    /// Removes only the hit portion and retains remaining path pieces.
    Partial,
    /// Removes from the hit to the nearest intersection.
    ToIntersection,
    /// Removes every path touched by the eraser.
    WholePath,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Operation applied to vector stroke widths.
pub enum VectorWidthMode {
    /// Adds the supplied document-pixel width.
    Add(f32),
    /// Subtracts the supplied document-pixel width.
    Subtract(f32),
    /// Multiplies widths by the supplied positive factor.
    Scale(f32),
    /// Sets a constant positive document-pixel width.
    Constant(f32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Hit-testing rule used to select vector paths and fills.
pub enum VectorSelectionMode {
    /// Selects only ranges inside and cuts at the selection boundary.
    CutBySelection,
    /// Selects paths touching the selection.
    Touching,
    /// Selects paths fully contained by the selection.
    FullyContained,
    /// Selects hit path ranges.
    Line,
    /// Selects whole hit paths.
    WholeLine,
    /// Selects from a hit to the nearest intersection.
    ToIntersection,
    /// Selects paths used as fill boundaries.
    FillBoundary,
    /// Selects fills inside the selection.
    Fill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Parametric selected range on one vector path.
pub struct VectorSelectionRange {
    /// Stable path ID.
    pub path_id: u64,
    /// Start parameter in millionths of the full path (`0..=1_000_000`).
    pub start_million: u32,
    /// End parameter in millionths of the full path (`0..=1_000_000`).
    pub end_million: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Deterministically ordered vector selection result.
pub struct VectorSelectionResult {
    /// Selected path ranges.
    pub path_ranges: Vec<VectorSelectionRange>,
    /// Stable IDs of selected fills.
    pub fill_ids: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Owned straight-alpha RGBA8 rasterization of vector content.
pub struct VectorRaster {
    /// Raster width in pixels.
    pub width: u32,
    /// Raster height in pixels.
    pub height: u32,
    /// Byte distance between adjacent rows.
    pub stride_bytes: u32,
    /// Top-to-bottom straight-alpha RGBA8 bytes.
    pub pixels: Vec<u8>,
}

/// One immutable document-coordinate vector segment in a render snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderVectorSegment {
    /// Stable source path ID.
    pub path_id: u64,
    /// Stable source plane ID.
    pub plane_id: u64,
    /// Deterministic stacking order within the snapshot.
    pub z_order: u32,
    /// Zero-based index within the path.
    pub segment_index: u32,
    /// Total number of segments in the path.
    pub segment_count: u32,
    /// Straight-alpha RGBA8 stroke color.
    pub color_rgba: [u8; 4],
    /// Whether the source path is closed.
    pub closed: bool,
    /// Whether the source plane is currently visible.
    pub stroke_visible: bool,
    /// Document-coordinate cubic geometry and widths.
    pub cubic: VectorCubicSegment,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One immutable document-coordinate vector fill in a render snapshot.
pub struct RenderVectorFill {
    /// Stable source fill ID.
    pub fill_id: u64,
    /// Stable source plane ID.
    pub plane_id: u64,
    /// Deterministic stacking order within the snapshot.
    pub z_order: u32,
    /// Straight-alpha RGBA8 fill color.
    pub color_rgba: [u8; 4],
    /// Stable source path IDs forming the boundary.
    pub boundary_path_ids: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VectorPath {
    pub(super) id: VectorPathId,
    pub(super) plane_id: PlaneId,
    pub(super) color: PixelValue,
    pub(super) closed: bool,
    pub(super) segments: Vec<VectorSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VectorFill {
    pub(super) id: VectorFillId,
    pub(super) plane_id: PlaneId,
    pub(super) color: PixelValue,
    pub(super) boundary_path_ids: Vec<VectorPathId>,
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
                    id: path.id.get(),
                    plane_id: path.plane_id.get(),
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
                    id: fill.id.get(),
                    plane_id: fill.plane_id.get(),
                    color: fill.color,
                    boundary_path_ids: fill.boundary_path_ids.iter().map(|id| id.get()).collect(),
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
                    id: VectorPathId::from_raw(path.id),
                    plane_id: PlaneId::from_raw(path.plane_id),
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
                    id: VectorFillId::from_raw(fill.id),
                    plane_id: PlaneId::from_raw(fill.plane_id),
                    color: fill.color,
                    boundary_path_ids: fill
                        .boundary_path_ids
                        .iter()
                        .copied()
                        .map(VectorPathId::from_raw)
                        .collect(),
                })
                .collect(),
        })
    }

    pub(crate) fn maximum_id(&self) -> u64 {
        self.paths
            .iter()
            .map(|path| path.id.get())
            .chain(self.fills.iter().map(|fill| fill.id.get()))
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

    pub(crate) fn remove_plane(&mut self, plane_id: PlaneId) {
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

    pub(crate) fn remove_layer(&mut self, document: &CellDocument, layer_id: LayerId) {
        if let Some(layer) = document.layers.iter().find(|layer| layer.id == layer_id) {
            for plane in &layer.planes {
                self.remove_plane(plane.id);
            }
        }
    }

    pub(crate) fn duplicate_planes(
        &mut self,
        plane_map: &BTreeMap<PlaneId, PlaneId>,
        next_id: &mut StableIdCursor,
    ) {
        let source_paths: Vec<_> = self
            .paths
            .iter()
            .filter(|path| plane_map.contains_key(&path.plane_id))
            .cloned()
            .collect();
        let mut path_map = BTreeMap::new();
        for mut path in source_paths {
            let source_id = path.id;
            path.id = next_id.take_vector_path();
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
            fill.id = next_id.take_vector_fill();
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

    pub(crate) fn paste_clipboard_paths<F>(
        &mut self,
        source: &ClipboardPlane,
        destination_plane_id: PlaneId,
        mut transform_segment: F,
        next_id: &mut StableIdCursor,
        path_map: &mut BTreeMap<u64, VectorPathId>,
    ) -> Result<(), CoreError>
    where
        F: FnMut(VectorCubicSegment) -> Result<VectorCubicSegment, CoreError>,
    {
        let segment_count = source
            .vector_paths
            .iter()
            .try_fold(0_usize, |count, path| {
                count.checked_add(path.segments.len())
            })
            .ok_or(CoreError::InvalidState(
                "clipboard vector segment count overflows",
            ))?;
        self.ensure_additional_limits(source.vector_paths.len(), 0, segment_count, 0)?;
        for path in &source.vector_paths {
            let id = next_id.take_vector_path();
            let input = VectorPathInput {
                segments: path
                    .segments
                    .iter()
                    .copied()
                    .map(&mut transform_segment)
                    .collect::<Result<Vec<_>, _>>()?,
                color: path.color,
                closed: path.closed,
            };
            self.paths.push(super::geometry::fixed_path(
                id,
                destination_plane_id,
                input,
            )?);
            if path_map.insert(path.id, id).is_some() {
                return Err(CoreError::InvalidArgument(
                    "clipboard contains a duplicate vector path id",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn paste_clipboard_fills(
        &mut self,
        source: &ClipboardPlane,
        destination_plane_id: PlaneId,
        next_id: &mut StableIdCursor,
        path_map: &BTreeMap<u64, VectorPathId>,
    ) -> Result<(), CoreError> {
        let boundary_count = source
            .vector_fills
            .iter()
            .try_fold(0_usize, |count, fill| {
                count.checked_add(fill.boundary_path_ids.len())
            })
            .ok_or(CoreError::InvalidState(
                "clipboard vector boundary count overflows",
            ))?;
        self.ensure_additional_limits(0, source.vector_fills.len(), 0, boundary_count)?;
        for fill in &source.vector_fills {
            let boundary_path_ids = fill
                .boundary_path_ids
                .iter()
                .map(|path_id| {
                    path_map
                        .get(path_id)
                        .copied()
                        .ok_or(CoreError::InvalidArgument(
                            "clipboard vector fill references a missing copied path",
                        ))
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.fills.push(VectorFill {
                id: next_id.take_vector_fill(),
                plane_id: destination_plane_id,
                color: fill.color,
                boundary_path_ids,
            });
        }
        Ok(())
    }

    pub(crate) fn reassign_plane(&mut self, old_plane_id: PlaneId, new_plane_id: PlaneId) {
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
                    fill_id: fill.id.get(),
                    plane_id: fill.plane_id.get(),
                    z_order: z_order as u32,
                    color_rgba: display_color(fill.color, layer.opacity_milli, plane.opacity_milli),
                    boundary_path_ids: fill.boundary_path_ids.iter().map(|id| id.get()).collect(),
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
                                path_id: path.id.get(),
                                plane_id: path.plane_id.get(),
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

    pub(crate) fn render_plane_items(
        &self,
        plane: &PlaneNode,
        z_order: u32,
    ) -> (Vec<RenderVectorSegment>, Vec<RenderVectorFill>) {
        let mut segments = Vec::new();
        let mut fills = Vec::new();
        if plane.kind == PlaneType::VectorFill && plane.visible {
            for fill in self.fills.iter().filter(|fill| fill.plane_id == plane.id) {
                fills.push(RenderVectorFill {
                    fill_id: fill.id.get(),
                    plane_id: fill.plane_id.get(),
                    z_order,
                    color_rgba: display_color(fill.color, 1_000, plane.opacity_milli),
                    boundary_path_ids: fill.boundary_path_ids.iter().map(|id| id.get()).collect(),
                });
            }
        }
        if matches!(
            plane.kind,
            PlaneType::ColorTrace | PlaneType::VectorMainLine
        ) {
            for path in self.paths.iter().filter(|path| path.plane_id == plane.id) {
                let color = display_color(path.color, 1_000, plane.opacity_milli);
                for (index, segment) in path.segments.iter().enumerate() {
                    segments.push(RenderVectorSegment {
                        path_id: path.id.get(),
                        plane_id: path.plane_id.get(),
                        z_order,
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
