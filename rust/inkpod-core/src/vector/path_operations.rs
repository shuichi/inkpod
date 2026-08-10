use super::geometry::*;
use super::model::*;
use super::*;
use crate::primitive::CanonicalInvocation;

impl Core {
    /// Returns required main-line, color-trace, and fill plane IDs for a vector layer.
    ///
    /// The tuple order is stable and the query does not mutate Core state.
    pub fn vector_layer_planes(&self, layer_id: u64) -> Result<(u64, u64, u64), CoreError> {
        let layer_id = LayerId::from_raw(layer_id);
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
            find(PlaneType::VectorMainLine)?.get(),
            find(PlaneType::ColorTrace)?.get(),
            find(PlaneType::VectorFill)?.get(),
        ))
    }

    /// Adds a validated path to an editable vector stroke plane.
    ///
    /// Success is one undoable edit and returns a new stable path ID. Invalid
    /// geometry, color, plane, or limits fail without consuming live state.
    pub fn vector_add_path(
        &mut self,
        plane_id: u64,
        input: VectorPathInput,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result = self.execute_canonical_invocation(CanonicalInvocation::VectorAddPath {
                plane_id,
                input,
            })?;
            let id = *result.output_ids.first().ok_or(CoreError::InvalidState(
                "vector-add-path primitive did not return its output ID",
            ))?;
            return Ok((result.dispatch, id));
        }
        self.ensure_no_active_stroke()?;
        let plane_id = PlaneId::from_raw(plane_id);
        let path = fixed_path(VectorPathId::from_raw(0), plane_id, input)?;
        let mut edit = self.begin_document_edit()?;
        let (before, after) = edit.documents();
        ensure_vector_stroke_plane(before, plane_id, true)?;
        before
            .vector
            .ensure_additional_limits(1, 0, path.segments.len(), 0)?;
        let mut next_id = self.next_id;
        let path_id = next_id.take_vector_path();
        after.vector.paths.push(VectorPath {
            id: path_id,
            ..path
        });
        let outcome = edit.commit(self)?;
        self.next_id = next_id;
        Ok((outcome, path_id.get()))
    }

    /// Adds a vector fill bounded by unique closed paths in the same layer.
    ///
    /// Success is one undoable edit and returns a new stable fill ID. Validation
    /// failure leaves vector state, revision, and history unchanged.
    pub fn vector_add_fill(
        &mut self,
        plane_id: u64,
        boundary_path_ids: &[u64],
        color: PixelValue,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result = self.execute_canonical_invocation(CanonicalInvocation::VectorAddFill {
                plane_id,
                boundary_path_ids: boundary_path_ids.to_vec(),
                color,
            })?;
            let id = *result.output_ids.first().ok_or(CoreError::InvalidState(
                "vector-add-fill primitive did not return its output ID",
            ))?;
            return Ok((result.dispatch, id));
        }
        self.ensure_no_active_stroke()?;
        let plane_id = PlaneId::from_raw(plane_id);
        let boundary_path_ids = boundary_path_ids
            .iter()
            .copied()
            .map(VectorPathId::from_raw)
            .collect::<Vec<_>>();
        if boundary_path_ids.is_empty() || boundary_path_ids.len() > MAX_VECTOR_BOUNDARIES {
            return Err(CoreError::InvalidArgument(
                "vector fill boundary count is outside bounds",
            ));
        }
        if color.rgba16().is_none() {
            return Err(CoreError::InvalidArgument("vector fill color must be RGBA"));
        }
        let mut edit = self.begin_document_edit()?;
        let (before, after) = edit.documents();
        let fill_layer = ensure_vector_fill_plane(before, plane_id, true)?;
        before
            .vector
            .ensure_additional_limits(0, 1, 0, boundary_path_ids.len())?;
        let mut unique = BTreeSet::new();
        for path_id in &boundary_path_ids {
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
            let path_layer = vector_layer_for_plane(before, path.plane_id)?;
            if path_layer != fill_layer {
                return Err(CoreError::InvalidArgument(
                    "fill boundary belongs to another vector layer",
                ));
            }
        }
        let mut next_id = self.next_id;
        let fill_id = next_id.take_vector_fill();
        after.vector.fills.push(VectorFill {
            id: fill_id,
            plane_id,
            color,
            boundary_path_ids,
        });
        let outcome = edit.commit(self)?;
        self.next_id = next_id;
        Ok((outcome, fill_id.get()))
    }

    /// Returns owned metadata and geometry for all paths in deterministic order.
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

    /// Returns owned metadata for all vector fills in deterministic order.
    pub fn vector_fills(&self) -> Result<Vec<VectorFillInfo>, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .vector
            .fills
            .iter()
            .map(|fill| VectorFillInfo {
                id: fill.id.get(),
                plane_id: fill.plane_id.get(),
                color: fill.color,
                boundary_path_ids: fill.boundary_path_ids.iter().map(|id| id.get()).collect(),
            })
            .collect())
    }

    /// Erases vector geometry near a document-space point.
    ///
    /// No hit is a no-op. A hit and any dependent fill removal are committed as
    /// one undoable edit; invalid geometry or limits fail atomically.
    pub fn vector_erase(
        &mut self,
        plane_id: u64,
        point: PointF32,
        radius: f32,
        mode: VectorEraseMode,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::VectorErase {
                    plane_id,
                    point,
                    radius,
                    mode,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let plane_id = PlaneId::from_raw(plane_id);
        if !point.x.is_finite()
            || !point.y.is_finite()
            || !radius.is_finite()
            || radius <= 0.0
            || radius > MAX_WIDTH
        {
            return Err(CoreError::InvalidArgument("vector eraser input is invalid"));
        }
        let mut edit = self.begin_document_edit()?;
        let (before, after) = edit.documents();
        ensure_vector_stroke_plane(before, plane_id, true)?;
        let touch = (f64::from(point.x), f64::from(point.y));
        let mut next_id = self.next_id;
        let mut replacements = BTreeMap::<VectorPathId, Vec<VectorPath>>::new();
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
        let mut paths = Vec::new();
        for path in &after.vector.paths {
            if let Some(pieces) = replacements.get(&path.id) {
                paths.extend(pieces.iter().cloned());
            } else {
                paths.push(path.clone());
            }
        }
        after.vector.paths = paths;
        after.vector.remove_connections_for_paths(&changed_ids);
        after.vector.fills.retain(|fill| {
            !fill
                .boundary_path_ids
                .iter()
                .any(|path_id| changed_ids.contains(path_id))
        });
        after.vector.ensure_limits()?;
        let outcome = edit.commit(self)?;
        self.next_id = next_id;
        Ok(outcome)
    }

    /// Connects the nearest pair of unconnected path endpoints within a gap.
    ///
    /// No eligible pair returns a no-op and `None`. Success adds one connector
    /// path as one undoable edit and returns its stable ID.
    pub fn vector_connect(
        &mut self,
        plane_id: u64,
        maximum_gap: f32,
    ) -> Result<(DispatchOutcome, Option<u64>), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result = self.execute_canonical_invocation(CanonicalInvocation::VectorConnect {
                plane_id,
                maximum_gap,
            })?;
            return Ok((result.dispatch, result.output_ids.first().copied()));
        }
        self.ensure_no_active_stroke()?;
        let plane_id = PlaneId::from_raw(plane_id);
        if !maximum_gap.is_finite() || maximum_gap <= 0.0 || maximum_gap > MAX_WIDTH {
            return Err(CoreError::InvalidArgument("vector connect gap is invalid"));
        }
        let mut edit = self.begin_document_edit()?;
        let (before, after) = edit.documents();
        ensure_vector_stroke_plane(before, plane_id, true)?;
        let paths: Vec<_> = before
            .vector
            .paths
            .iter()
            .filter(|path| path.plane_id == plane_id && !path.closed)
            .collect();
        let connected_endpoints = before.vector.connected_endpoint_ids();
        let mut best: Option<(f64, VectorPathId, bool, VectorPathId, bool)> = None;
        for (left_index, left) in paths.iter().enumerate() {
            for right in &paths[left_index + 1..] {
                for left_end in [false, true] {
                    let left_endpoint = VectorEndpointId {
                        path_id: left.id,
                        endpoint: if left_end {
                            VectorEndpoint::End
                        } else {
                            VectorEndpoint::Start
                        },
                    };
                    if connected_endpoints.contains(&left_endpoint) {
                        continue;
                    }
                    for right_end in [false, true] {
                        let right_endpoint = VectorEndpointId {
                            path_id: right.id,
                            endpoint: if right_end {
                                VectorEndpoint::End
                            } else {
                                VectorEndpoint::Start
                            },
                        };
                        if connected_endpoints.contains(&right_endpoint) {
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
        let connector_id = next_id.take_vector_path();
        after.vector.paths.push(VectorPath {
            id: connector_id,
            plane_id,
            color: left.color,
            closed: false,
            square_cross_section: left.square_cross_section,
            segments: vec![line_segment(start, end, start_width, end_width)],
        });
        let left_endpoint = VectorEndpointId {
            path_id: left_id,
            endpoint: if left_end {
                VectorEndpoint::End
            } else {
                VectorEndpoint::Start
            },
        };
        let right_endpoint = VectorEndpointId {
            path_id: right_id,
            endpoint: if right_end {
                VectorEndpoint::End
            } else {
                VectorEndpoint::Start
            },
        };
        after.vector.connect_endpoints(
            left_endpoint,
            VectorEndpointId {
                path_id: connector_id,
                endpoint: VectorEndpoint::Start,
            },
        )?;
        after.vector.connect_endpoints(
            VectorEndpointId {
                path_id: connector_id,
                endpoint: VectorEndpoint::End,
            },
            right_endpoint,
        )?;
        after.vector.ensure_limits()?;
        let outcome = edit.commit(self)?;
        self.next_id = next_id;
        Ok((outcome, Some(connector_id.get())))
    }

    /// Applies a width operation to a unique non-empty set of stable path IDs.
    ///
    /// An unchanged result is a no-op; success is one undoable edit.
    pub fn vector_correct_width(
        &mut self,
        path_ids: &[u64],
        mode: VectorWidthMode,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::VectorCorrectWidth {
                    path_ids: path_ids.to_vec(),
                    mode,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        if path_ids.is_empty() {
            return Err(CoreError::InvalidArgument("no vector paths were selected"));
        }
        let transform = width_transform(mode)?;
        let mut edit = self.begin_document_edit()?;
        let (before, after) = edit.documents();
        let selected: BTreeSet<_> = path_ids
            .iter()
            .copied()
            .map(VectorPathId::from_raw)
            .collect();
        if selected.len() != path_ids.len()
            || selected
                .iter()
                .any(|id| !before.vector.paths.iter().any(|path| path.id == *id))
        {
            return Err(CoreError::InvalidArgument(
                "vector path selection is invalid",
            ));
        }
        for path in after
            .vector
            .paths
            .iter_mut()
            .filter(|path| selected.contains(&path.id))
        {
            ensure_vector_stroke_plane(before, path.plane_id, true)?;
            for segment in &mut path.segments {
                segment.width_start_milli = transform(segment.width_start_milli)?;
                segment.width_end_milli = transform(segment.width_end_milli)?;
            }
        }
        if after == before {
            return Ok(self.noop_outcome());
        }
        edit.commit(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_overflow_does_not_consume_a_vector_id() {
        let mut core = Core::new();
        core.new_cell(4, 4, 96_000, 96_000).unwrap();
        let (_, layer_id) = core
            .create_layer(LayerKind::VectorColoring, "Vector")
            .unwrap();
        let main_plane_id = core
            .layers()
            .unwrap()
            .into_iter()
            .find(|layer| layer.id == layer_id)
            .unwrap()
            .planes
            .into_iter()
            .find(|plane| plane.kind == PlaneType::VectorMainLine)
            .unwrap()
            .id;
        let next_id = core.next_id;
        core.document_revision = DocumentRevision::from_raw(u64::MAX);
        let path = VectorPathInput {
            segments: vec![VectorCubicSegment {
                p0: PointF32 { x: 0.0, y: 3.0 },
                p1: PointF32 { x: 1.0, y: 3.0 },
                p2: PointF32 { x: 2.0, y: 3.0 },
                p3: PointF32 { x: 3.0, y: 3.0 },
                width_start: 1.0,
                width_end: 1.0,
            }],
            color: PixelValue::Rgba([0, 0, 0, 255]),
            closed: false,
        };
        assert_eq!(
            core.vector_add_path(main_plane_id, path),
            Err(CoreError::InvalidState("document revision overflow"))
        );
        assert_eq!(core.next_id, next_id);
    }
}
