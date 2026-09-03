use super::helpers::*;
use super::*;
use crate::primitive::{CanonicalInvocation, InvocationResult};
use crate::{LineBackground, LineCorrection, LineCorrectionRequest, SelectionSample};

pub(super) fn resolved_line_background(
    background: LineBackground,
    kind: PlaneType,
) -> LineBackground {
    match (background, kind) {
        (LineBackground::PlaneDefault, PlaneType::MainLine) => {
            LineBackground::TransparentOrColor([u16::MAX; 4])
        }
        (LineBackground::PlaneDefault, _) => LineBackground::Transparent,
        _ => background,
    }
}

impl Core {
    fn resolved_line_request(
        &self,
        request: &LineCorrectionRequest,
    ) -> Result<LineCorrectionRequest, CoreError> {
        let samples = match request.region.as_ref() {
            Some(SelectionShape::Lasso(points) | SelectionShape::Polyline(points))
            | Some(SelectionShape::Trace { points, .. }) => points.len(),
            Some(SelectionShape::TraceBrush { samples, .. }) => samples.len(),
            _ => 0,
        };
        if samples > 1_048_576 {
            return Err(CoreError::InvalidArgument(
                "line region sample limit exceeded",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let plane = editable_line_plane(document, PlaneId::from_raw(request.plane_id))?;
        let mut request = request.clone();
        let background = match &mut request.correction {
            LineCorrection::Dust(options) => &mut options.background,
            LineCorrection::Connect { background, .. }
            | LineCorrection::Width { background, .. } => background,
        };
        *background = resolved_line_background(*background, plane.kind);
        Ok(request)
    }

    /// Applies an explicit raster line edit as one canonical transaction.
    /// Main-line protection for ordinary coloring does not block this command;
    /// plane/layer locks do. No-op, invalid, cancelled, stale, or failed work
    /// publishes no pixels, history, journal entry, or document revision.
    pub fn apply_line_correction(
        &mut self,
        request: &LineCorrectionRequest,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<DispatchOutcome, CoreError> {
        let request = self.resolved_line_request(request)?;
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation_with(
                    CanonicalInvocation::ApplyLineCorrection {
                        request: request.clone(),
                    },
                    move |staged| {
                        staged
                            .apply_line_correction_internal(&request, &mut progress)
                            .map(InvocationResult::dispatch)
                    },
                )
                .map(|result| result.dispatch);
        }
        self.apply_line_correction_internal(&request, &mut progress)
    }

    fn apply_line_correction_internal(
        &mut self,
        request: &LineCorrectionRequest,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let base_revision = self.document_revision;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let after = corrected_document(&before, request, revision.get(), progress)?;
        self.commit_deferred_document_edit(before, after, base_revision, revision)
    }

    /// Computes a cancellable preview without changing the document or its
    /// history. `apply_filter_preview` commits one undo unit; cancellation
    /// discards only transient content. All raster formats retain native depth.
    pub fn begin_line_correction_preview(
        &mut self,
        request: &LineCorrectionRequest,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<FilterPreviewInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let request = self.resolved_line_request(request)?;
        let plane_id = PlaneId::from_raw(request.plane_id);
        let base_revision = self.document_revision;
        let base_document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let preview_document = corrected_document(
            &base_document,
            &request,
            preview_revision.get(),
            &mut progress,
        )?;
        let info = preview_info(
            plane_id,
            &base_document,
            &preview_document,
            preview_revision,
        )?;
        self.filter_preview = Some(FilterPreview {
            plane_id,
            base_revision,
            base_document,
            preview_document,
            preview_revision,
            procedure: PreviewProcedure::LineCorrection(request),
        });
        self.render_cache.clear();
        Ok(info)
    }

    /// Converts a bounded gesture to document-space line-edit geometry. Diameter
    /// units come from the construction options, independently of sample space;
    /// screen-size zoom must be captured at gesture begin. Rectangles share the
    /// canonical Q16 selection geometry and pixel-center rule. No document mutation.
    #[allow(clippy::too_many_arguments)]
    pub fn line_correction_region_for_view(
        &self,
        view_id: u64,
        coordinate_space: CoordinateSpace,
        kind: EffectRegionKind,
        samples: &[StrokeSample],
        diameter: f32,
    ) -> Result<SelectionShape, CoreError> {
        if !matches!(kind, EffectRegionKind::Trace | EffectRegionKind::Rectangle) {
            return self.effect_region_for_view(view_id, coordinate_space, kind, samples, diameter);
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
        let samples = document_samples_for_view(
            view,
            coordinate_space,
            samples,
            document.width,
            document.height,
        )?;
        if kind == EffectRegionKind::Rectangle {
            let first = samples
                .first()
                .ok_or(CoreError::InvalidArgument("rectangle region is empty"))?;
            let last = samples.last().expect("first sample exists");
            return Ok(SelectionShape::RectangleGesture {
                anchor: PointF32 {
                    x: first.point.x,
                    y: first.point.y,
                },
                current: PointF32 {
                    x: last.point.x,
                    y: last.point.y,
                },
            });
        }
        Ok(SelectionShape::TraceBrush {
            samples: samples
                .into_iter()
                .map(|sample| SelectionSample {
                    x: sample.point.x,
                    y: sample.point.y,
                    pressure: sample.pressure,
                })
                .collect(),
            diameter,
        })
    }
}

fn corrected_document(
    base: &CellDocument,
    request: &LineCorrectionRequest,
    revision: u64,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<CellDocument, CoreError> {
    let plane_id = PlaneId::from_raw(request.plane_id);
    let plane = editable_line_plane(base, plane_id)?;
    let mut mask = request
        .region
        .as_ref()
        .map(|shape| {
            selection_mask_for_shape(
                base,
                plane_id,
                shape,
                RangeInterpretation::Normal,
                request.construction,
                revision,
            )
        })
        .transpose()?;
    if base.selection.allocated_tile_count() != 0 {
        mask = Some(match mask {
            Some(mask) => combine_selection_masks(
                &base.selection,
                &mask,
                SelectionOperation::Intersect,
                revision,
            )?,
            None => base.selection.clone(),
        });
    }
    let raster = match request.correction {
        LineCorrection::Dust(options) => {
            apply_dust_removal(&plane.raster, mask.as_ref(), options, revision, progress)?
        }
        LineCorrection::Connect {
            gap,
            width,
            background,
        } => inkpod_image::apply_line_connection(
            &plane.raster,
            mask.as_ref(),
            gap,
            width,
            background,
            revision,
            progress,
        )?,
        LineCorrection::Width {
            mode,
            amount,
            background,
        } => inkpod_image::apply_line_width(
            &plane.raster,
            mask.as_ref(),
            mode,
            amount,
            background,
            revision,
            progress,
        )?,
    };
    let mut after = base.clone();
    after
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("line plane disappeared"))?
        .raster = raster;
    Ok(after)
}
