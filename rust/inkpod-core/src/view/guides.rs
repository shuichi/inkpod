use super::{device_to_document, stroke_coordinate_is_supported};
use crate::primitive::CanonicalInvocation;
use crate::*;

impl Core {
    /// Borrows document guides in deterministic display order.
    pub fn guides(&self) -> Result<&[Guide], CoreError> {
        Ok(&self.document.as_ref().ok_or(CoreError::NoDocument)?.guides)
    }

    /// Adds a guide and returns its stable ID.
    ///
    /// Position is in document pixels and may lie on the far paper edge. Success
    /// is one undoable edit; invalid positions and limits fail atomically.
    pub fn add_guide(
        &mut self,
        axis: GuideAxis,
        position: i32,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result = self
                .execute_canonical_invocation(CanonicalInvocation::AddGuide { axis, position })?;
            let id = *result.output_ids.first().ok_or(CoreError::InvalidState(
                "add-guide primitive did not return its output ID",
            ))?;
            return Ok((result.dispatch, id));
        }
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if document.guides.len() >= MAX_GUIDES {
            return Err(CoreError::InvalidState("guide limit reached"));
        }
        validate_guide_position(document, axis, position)?;
        let id = self.allocate_guide_id();
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        after.guides.push(Guide {
            id: id.get(),
            axis,
            position,
        });
        after
            .guides
            .sort_by_key(|guide| (guide.axis as u8, guide.position, guide.id));
        let outcome = edit.commit(self)?;
        Ok((outcome, id.get()))
    }

    /// Moves a guide to a document-pixel position on the same axis.
    ///
    /// An unchanged position is a no-op; a change is one undoable edit.
    pub fn move_guide(
        &mut self,
        guide_id: u64,
        position: i32,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::MoveGuide { guide_id, position })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let guide = document
            .guides
            .iter()
            .find(|guide| guide.id == guide_id)
            .ok_or(CoreError::InvalidArgument("guide ID does not exist"))?;
        validate_guide_position(document, guide.axis, position)?;
        if guide.position == position {
            return Ok(self.noop_outcome());
        }
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        after
            .guides
            .iter_mut()
            .find(|guide| guide.id == guide_id)
            .expect("guide existence checked")
            .position = position;
        after
            .guides
            .sort_by_key(|guide| (guide.axis as u8, guide.position, guide.id));
        edit.commit(self)
    }

    /// Deletes a guide by stable ID as one undoable document edit.
    pub fn delete_guide(&mut self, guide_id: u64) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::DeleteGuide { guide_id })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let index = document
            .guides
            .iter()
            .position(|guide| guide.id == guide_id)
            .ok_or(CoreError::InvalidArgument("guide ID does not exist"))?;
        let mut edit = self.begin_document_edit()?;
        edit.working_mut().guides.remove(index);
        edit.commit(self)
    }

    /// Deletes all document guides as one atomic, undoable edit.
    ///
    /// An already empty guide list is a semantic no-op.
    pub fn delete_all_guides(&mut self) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::DeleteAllGuides)
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        if self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .guides
            .is_empty()
        {
            return Ok(self.noop_outcome());
        }
        let mut edit = self.begin_document_edit()?;
        edit.working_mut().guides.clear();
        edit.commit(self)
    }

    /// Returns the current document-space grid configuration.
    pub fn grid(&self) -> Result<GridConfig, CoreError> {
        Ok(self.document.as_ref().ok_or(CoreError::NoDocument)?.grid)
    }

    /// Replaces the grid configuration as one undoable document edit.
    ///
    /// Identical input is a no-op. Invalid zero or excessive spacing/subdivision
    /// values fail without changing revision or history.
    pub fn set_grid(&mut self, grid: GridConfig) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::SetGrid { grid })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        validate_grid(grid)?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if document.grid == grid {
            return Ok(self.noop_outcome());
        }
        let mut edit = self.begin_document_edit()?;
        edit.working_mut().grid = grid;
        edit.commit(self)
    }

    /// Applies enabled guide/grid snapping to a finite document-space point.
    ///
    /// This query does not change document or view state.
    pub fn snap_document_point(&self, x: f64, y: f64) -> Result<(f64, f64), CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let point = DocumentPointF64::new(x, y)
            .map_err(|_| CoreError::InvalidArgument("snap point is not finite"))?;
        let snapped = snap_document_point(document, self.view, point);
        Ok((snapped.x, snapped.y))
    }

    /// Resolves bounded pointer samples through one immutable view into geometry points.
    ///
    /// View ID zero selects the primary view; other IDs select a live secondary
    /// view. A nonzero expected revision rejects stale input. Device coordinates
    /// are transformed before optional guide/grid snapping, then every result is
    /// clamped to the inclusive far paper edge. This query does not change
    /// document, view, history, dirty, savepoint, or journal state.
    pub fn resolve_geometry_points_for_view(
        &self,
        view_id: u64,
        expected_view_revision: u64,
        coordinate_space: CoordinateSpace,
        samples: &[StrokeSample],
        snap_mode: GeometrySnapMode,
    ) -> Result<GeometryPointResolution, CoreError> {
        if samples.is_empty() || samples.len() > MAX_GEOMETRY_POINTS {
            return Err(CoreError::InvalidArgument(
                "geometry point count is outside bounds",
            ));
        }
        if samples.iter().any(|sample| {
            !sample.x.is_finite()
                || !sample.y.is_finite()
                || sample.x.abs() > MAX_STROKE_COORDINATE
                || sample.y.abs() > MAX_STROKE_COORDINATE
                || !sample.pressure.is_finite()
                || !(0.0..=1.0).contains(&sample.pressure)
        }) {
            return Err(CoreError::InvalidArgument(
                "geometry sample contains invalid values",
            ));
        }

        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let view = if view_id == 0 {
            self.view
        } else {
            *self
                .secondary_views
                .get(&ViewId::from_raw(view_id))
                .ok_or(CoreError::InvalidArgument("view ID does not exist"))?
        };
        if expected_view_revision != 0 && expected_view_revision != view.revision.get() {
            return Err(CoreError::InvalidState("view revision is stale"));
        }

        let document_size = DocumentSizeU32::new(document.width, document.height);
        let mut points = Vec::with_capacity(samples.len());
        for sample in samples {
            let point = match coordinate_space {
                CoordinateSpace::Document => {
                    DocumentPointF64::new(f64::from(sample.x), f64::from(sample.y))?
                }
                CoordinateSpace::Device => device_to_document(
                    view,
                    document_size,
                    DevicePointF64::new(f64::from(sample.x), f64::from(sample.y))?,
                ),
            };
            if !stroke_coordinate_is_supported(point.x) || !stroke_coordinate_is_supported(point.y)
            {
                return Err(CoreError::InvalidArgument(
                    "geometry coordinate is outside bounds",
                ));
            }
            let bounded = DocumentPointF64 {
                x: point.x.clamp(0.0, f64::from(document.width)),
                y: point.y.clamp(0.0, f64::from(document.height)),
            };
            let resolved = match snap_mode {
                GeometrySnapMode::UseViewState => snap_document_point(document, view, bounded),
                GeometrySnapMode::Bypass => bounded,
            };
            points.push(PointF32 {
                x: resolved.x.clamp(0.0, f64::from(document.width)) as f32,
                y: resolved.y.clamp(0.0, f64::from(document.height)) as f32,
            });
        }
        Ok(GeometryPointResolution {
            view_revision: view.revision.get(),
            points,
        })
    }
}

fn snap_document_point(
    document: &CellDocument,
    view: ViewState,
    point: DocumentPointF64,
) -> DocumentPointF64 {
    if !view.snap_enabled {
        return point;
    }
    let grid = document.grid;
    let snap_axis = |value: f64, origin: i32, spacing: u32| {
        let step = f64::from(spacing) / f64::from(grid.subdivisions);
        f64::from(origin) + ((value - f64::from(origin)) / step).round() * step
    };
    let mut snapped = if view.grid_snap_enabled {
        DocumentPointF64 {
            x: snap_axis(point.x, grid.origin_x, grid.spacing_x),
            y: snap_axis(point.y, grid.origin_y, grid.spacing_y),
        }
    } else {
        point
    };
    if view.guide_snap_enabled {
        for guide in &document.guides {
            match guide.axis {
                GuideAxis::Vertical if (point.x - f64::from(guide.position)).abs() <= 4.0 => {
                    snapped.x = f64::from(guide.position);
                }
                GuideAxis::Horizontal if (point.y - f64::from(guide.position)).abs() <= 4.0 => {
                    snapped.y = f64::from(guide.position);
                }
                _ => {}
            }
        }
    }
    snapped
}
pub(super) fn validate_guide_position(
    document: &CellDocument,
    axis: GuideAxis,
    position: i32,
) -> Result<(), CoreError> {
    let limit = match axis {
        GuideAxis::Horizontal => document.height,
        GuideAxis::Vertical => document.width,
    };
    if position < 0
        || u32::try_from(position)
            .ok()
            .is_none_or(|value| value > limit)
    {
        Err(CoreError::InvalidArgument(
            "guide position is outside paper",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn validate_grid(grid: GridConfig) -> Result<(), CoreError> {
    if grid.spacing_x == 0
        || grid.spacing_y == 0
        || grid.spacing_x > 1_048_576
        || grid.spacing_y > 1_048_576
        || grid.subdivisions == 0
        || grid.subdivisions > 1_024
    {
        Err(CoreError::InvalidArgument("grid values are outside bounds"))
    } else {
        Ok(())
    }
}
