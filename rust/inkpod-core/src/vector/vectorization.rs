use super::geometry::*;
use super::model::*;
use super::*;

impl Core {
    /// Converts opaque regions of an RGBA8 raster plane into vector paths/fills.
    ///
    /// Generated objects receive stable IDs. Success is one undoable atomic edit;
    /// validation, bounds, or stale-revision failure commits no partial geometry.
    pub fn vectorize_raster_plane(
        &mut self,
        source_plane_id: u64,
        target_vector_layer_id: u64,
        alpha_threshold: u8,
    ) -> Result<(DispatchOutcome, Vec<u64>), CoreError> {
        self.ensure_no_active_stroke()?;
        let source_plane_id = PlaneId::from_raw(source_plane_id);
        let target_vector_layer_id = LayerId::from_raw(target_vector_layer_id);
        let base_revision = self.document_revision;
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
            let path_id = next_id.take_vector_path();
            let fill_id = next_id.take_vector_fill();
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
        let revision = self.next_document_revision()?;
        let outcome = self.commit_deferred_document_edit(before, after, base_revision, revision)?;
        self.next_id = next_id;
        Ok((
            outcome,
            fill_ids.into_iter().map(VectorFillId::get).collect(),
        ))
    }
}
