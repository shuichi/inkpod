use super::geometry::*;
use super::model::*;
use super::*;
use crate::EditorTarget;
use crate::primitive::CanonicalInvocation;

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
        if !self.canonical_invocation_is_active() {
            let result =
                self.execute_canonical_invocation(CanonicalInvocation::VectorizeRasterPlane {
                    source_plane_id,
                    target_vector_layer_id,
                    alpha_threshold,
                })?;
            return Ok((result.dispatch, result.output_ids));
        }
        self.ensure_no_active_stroke()?;
        let source_plane_id = PlaneId::from_raw(source_plane_id);
        let target_vector_layer_id = LayerId::from_raw(target_vector_layer_id);
        let base_revision = self.document_revision;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let runs = collect_vectorization_runs(&before, source_plane_id, alpha_threshold)?;
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
        if runs.is_empty() {
            return Ok((self.noop_outcome(), Vec::new()));
        }
        let mut after = before.clone();
        let mut next_id = self.next_id;
        let fill_ids =
            append_vectorization_runs(&mut after, trace_plane, fill_plane, &runs, &mut next_id);
        after.vector.ensure_limits()?;
        let revision = self.next_document_revision()?;
        let outcome = self.commit_deferred_document_edit(before, after, base_revision, revision)?;
        self.next_id = next_id;
        Ok((
            outcome,
            fill_ids.into_iter().map(VectorFillId::get).collect(),
        ))
    }

    /// Creates one vector-coloring layer and vectorizes a raster plane into it.
    ///
    /// Layer topology, paths, and fills publish as one canonical history unit.
    /// An empty source is a no-op and does not allocate a layer or stable IDs.
    pub fn vectorize_raster_plane_into_new_layer(
        &mut self,
        source_plane_id: u64,
        alpha_threshold: u8,
        name: &str,
    ) -> Result<(DispatchOutcome, u64, Vec<u64>), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result = self.execute_canonical_invocation(
                CanonicalInvocation::VectorizeRasterPlaneIntoNewLayer {
                    source_plane_id,
                    alpha_threshold,
                    name: name.to_owned(),
                },
            )?;
            let Some((&layer_id, fill_ids)) = result.output_ids.split_first() else {
                return Ok((result.dispatch, 0, Vec::new()));
            };
            return Ok((result.dispatch, layer_id, fill_ids.to_vec()));
        }
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        let source_plane_id = PlaneId::from_raw(source_plane_id);
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.layers.len() >= crate::MAX_LAYERS {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        let runs = collect_vectorization_runs(&before, source_plane_id, alpha_threshold)?;
        if runs.is_empty() {
            return Ok((self.noop_outcome(), 0, Vec::new()));
        }

        let mut next_id = self.next_id;
        let layer_id = next_id.take_layer();
        let main_plane_id = next_id.take_plane();
        let trace_plane_id = next_id.take_plane();
        let fill_plane_id = next_id.take_plane();
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let after = edit.working_mut();
        let raster = || TileRaster::new(after.width, after.height, PixelFormat::StraightRgba8);
        let planes = vec![
            PlaneNode {
                id: main_plane_id,
                kind: PlaneType::VectorMainLine,
                name: "Vector Main Line".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster: raster()?,
            },
            PlaneNode {
                id: trace_plane_id,
                kind: PlaneType::ColorTrace,
                name: "Color Trace".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster: raster()?,
            },
            PlaneNode {
                id: fill_plane_id,
                kind: PlaneType::VectorFill,
                name: "Vector Fill".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster: raster()?,
            },
        ];
        after.layers.push(LayerNode {
            id: layer_id,
            kind: LayerKind::VectorColoring,
            name: unique_layer_name(&after.layers, name),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes,
        });
        let fill_ids =
            append_vectorization_runs(after, trace_plane_id, fill_plane_id, &runs, &mut next_id);
        after.vector.ensure_limits()?;
        edit.prefer_editor_target(EditorTarget {
            layer_id: layer_id.get(),
            plane_id: main_plane_id.get(),
        });
        let outcome = edit.commit(self)?;
        debug_assert_eq!(outcome.revision, revision.get());
        self.next_id = next_id;
        Ok((
            outcome,
            layer_id.get(),
            fill_ids.into_iter().map(VectorFillId::get).collect(),
        ))
    }
}

type VectorizationRun = (u32, u32, u32, [u8; 4]);

fn collect_vectorization_runs(
    document: &CellDocument,
    source_plane_id: PlaneId,
    alpha_threshold: u8,
) -> Result<Vec<VectorizationRun>, CoreError> {
    let source = document
        .plane_by_id(source_plane_id)
        .ok_or(CoreError::InvalidArgument(
            "source raster plane does not exist",
        ))?;
    if !matches!(source.kind, PlaneType::Color | PlaneType::Raster)
        || source.raster.format() != PixelFormat::StraightRgba8
    {
        return Err(CoreError::InvalidArgument(
            "raster-to-vector conversion requires an RGBA8 raster plane",
        ));
    }
    let run_capacity = document.vector.raster_vectorize_run_capacity()?;
    let mut runs = Vec::new();
    for y in 0..document.height {
        let mut x = 0;
        while x < document.width {
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
            while x < document.width && source.raster.pixel(x, y)? == PixelValue::Rgba(color) {
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
    Ok(runs)
}

fn append_vectorization_runs(
    document: &mut CellDocument,
    trace_plane: PlaneId,
    fill_plane: PlaneId,
    runs: &[VectorizationRun],
    next_id: &mut StableIdCursor,
) -> Vec<VectorFillId> {
    let mut fill_ids = Vec::with_capacity(runs.len());
    for &(start, end, y, color) in runs {
        let path_id = next_id.take_vector_path();
        let fill_id = next_id.take_vector_fill();
        let points = [
            fixed_xy_point(f64::from(start), f64::from(y)),
            fixed_xy_point(f64::from(end), f64::from(y)),
            fixed_xy_point(f64::from(end), f64::from(y + 1)),
            fixed_xy_point(f64::from(start), f64::from(y + 1)),
        ];
        let width = 1;
        document.vector.paths.push(VectorPath {
            id: path_id,
            plane_id: trace_plane,
            color: PixelValue::Rgba([0, 0, 0, 0]),
            closed: true,
            square_cross_section: false,
            segments: vec![
                line_segment(points[0], points[1], width, width),
                line_segment(points[1], points[2], width, width),
                line_segment(points[2], points[3], width, width),
                line_segment(points[3], points[0], width, width),
            ],
        });
        document.vector.fills.push(VectorFill {
            id: fill_id,
            plane_id: fill_plane,
            color: PixelValue::Rgba(color),
            boundary_path_ids: vec![path_id],
        });
        fill_ids.push(fill_id);
    }
    fill_ids
}
