use super::helpers::*;
use super::*;

impl Core {
    pub(super) fn apply_masked_raster_operation<F>(
        &mut self,
        plane_id: PlaneId,
        shape: &SelectionShape,
        operation: F,
    ) -> Result<DispatchOutcome, CoreError>
    where
        F: FnOnce(&TileRaster, &TileRaster, u64) -> Result<TileRaster, inkpod_image::RasterError>,
    {
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        let plane = editable_color_plane(before, plane_id)?;
        let mut mask = selection_mask_for_shape(
            before,
            plane_id,
            shape,
            RangeInterpretation::Normal,
            SelectionConstructionOptions::default(),
            revision.get(),
        )?;
        if before.selection.allocated_tile_count() != 0 {
            mask = combine_selection_masks(
                &before.selection,
                &mask,
                SelectionOperation::Intersect,
                revision.get(),
            )?;
        }
        let raster = operation(&plane.raster, &mask, revision.get())?;
        after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("operation plane disappeared"))?
            .raster = raster;
        edit.commit(self)
    }

    pub(super) fn apply_raster_operation<F>(
        &mut self,
        plane_id: PlaneId,
        operation: F,
    ) -> Result<DispatchOutcome, CoreError>
    where
        F: FnOnce(
            &TileRaster,
            Option<&TileRaster>,
            u64,
        ) -> Result<TileRaster, inkpod_image::RasterError>,
    {
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        let plane = editable_color_plane(before, plane_id)?;
        let selection = (before.selection.allocated_tile_count() != 0).then_some(&before.selection);
        let raster = operation(&plane.raster, selection, revision.get())?;
        after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("operation plane disappeared"))?
            .raster = raster;
        edit.commit(self)
    }

    pub(super) fn apply_blur_tool_mask_to_plane(
        &mut self,
        plane_id: PlaneId,
        mut mask: TileRaster,
        radius: u32,
        strength_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        if mask.width() != before.width
            || mask.height() != before.height
            || mask.format() != PixelFormat::BinaryMask8
        {
            return Err(CoreError::InvalidArgument(
                "blur pressure mask does not match the document",
            ));
        }
        let plane = editable_color_plane(before, plane_id)?;
        if before.selection.allocated_tile_count() != 0 {
            mask = combine_selection_masks(
                &before.selection,
                &mask,
                SelectionOperation::Intersect,
                revision.get(),
            )?;
        }
        let raster = apply_filter(
            &plane.raster,
            Some(&mask),
            &Filter::GaussianBlur {
                radius,
                strength_milli,
            },
            revision.get(),
        )?;
        after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("operation plane disappeared"))?
            .raster = raster;
        edit.commit(self)
    }

    pub(super) fn pressure_trace_mask_for_view(
        &self,
        view_id: u64,
        coordinate_space: CoordinateSpace,
        samples: &[StrokeSample],
        diameter: f32,
    ) -> Result<TileRaster, CoreError> {
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
        if samples.is_empty() {
            return Err(CoreError::InvalidArgument("blur pen region is empty"));
        }
        let diameter = match coordinate_space {
            CoordinateSpace::Document => f64::from(diameter),
            CoordinateSpace::Device => f64::from(diameter) / view.zoom.get(),
        };
        if !diameter.is_finite() || diameter <= 0.0 || diameter > 4_096.0 {
            return Err(CoreError::InvalidArgument("blur pen diameter is invalid"));
        }
        let mut mask = TileRaster::new(document.width, document.height, PixelFormat::BinaryMask8)?;
        for y in 0..document.height {
            for x in 0..document.width {
                if pressure_trace_contains(
                    f64::from(x) + 0.5,
                    f64::from(y) + 0.5,
                    &samples,
                    diameter,
                ) {
                    mask.set_pixel(
                        x,
                        y,
                        super::PixelValue::Binary(255),
                        self.document_revision.get(),
                    )?;
                }
            }
        }
        Ok(mask)
    }

    pub(super) fn effect_region_for_view(
        &self,
        view_id: u64,
        coordinate_space: CoordinateSpace,
        kind: EffectRegionKind,
        samples: &[StrokeSample],
        diameter: f32,
    ) -> Result<SelectionShape, CoreError> {
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
        let points: Vec<_> = samples
            .iter()
            .map(|sample| PointF32 {
                x: sample.point.x,
                y: sample.point.y,
            })
            .collect();
        match kind {
            EffectRegionKind::Trace => {
                let diameter = match coordinate_space {
                    CoordinateSpace::Document => diameter,
                    CoordinateSpace::Device => (f64::from(diameter) / view.zoom.get()) as f32,
                };
                Ok(SelectionShape::Trace { points, diameter })
            }
            EffectRegionKind::Rectangle => {
                let first = points
                    .first()
                    .ok_or(CoreError::InvalidArgument("rectangle region is empty"))?;
                let last = points.last().expect("first point exists");
                let left = first.x.min(last.x).floor();
                let top = first.y.min(last.y).floor();
                let right = first.x.max(last.x).ceil();
                let bottom = first.y.max(last.y).ceil();
                Ok(SelectionShape::Rectangle(RectI32 {
                    x: left as i32,
                    y: top as i32,
                    width: (right - left).max(1.0) as i32,
                    height: (bottom - top).max(1.0) as i32,
                }))
            }
            EffectRegionKind::Polyline => Ok(SelectionShape::Polyline(points)),
            EffectRegionKind::Lasso => Ok(SelectionShape::Lasso(points)),
        }
    }
}
