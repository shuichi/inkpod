use super::helpers::*;
use super::*;

impl Core {
    pub fn apply_gradient_to_plane(
        &mut self,
        plane_id: u64,
        gradient: &Gradient,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_gradient(raster, selection, gradient, revision)
        })
    }

    pub fn apply_boundary_airbrush_to_plane(
        &mut self,
        plane_id: u64,
        effect: &BoundaryAirbrush,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_boundary_airbrush(raster, selection, effect, revision)
        })
    }

    pub fn apply_blur_to_plane(
        &mut self,
        plane_id: u64,
        radius: u32,
        strength_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        let filter = Filter::GaussianBlur {
            radius,
            strength_milli,
        };
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_filter(raster, selection, &filter, revision)
        })
    }

    pub fn apply_airbrush_to_plane(
        &mut self,
        plane_id: u64,
        stroke: AirbrushStroke,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_airbrush(raster, selection, stroke, revision)
        })
    }

    pub fn apply_airbrush_gesture_to_plane(
        &mut self,
        plane_id: u64,
        gesture: &AirbrushGesture,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_airbrush_gesture(raster, selection, gesture, revision)
        })
    }

    pub fn apply_airbrush_gesture_for_view(
        &mut self,
        view_id: u64,
        coordinate_space: CoordinateSpace,
        plane_id: u64,
        samples: &[StrokeSample],
        mut gesture: AirbrushGesture,
    ) -> Result<DispatchOutcome, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let view = if view_id == 0 {
            self.view
        } else {
            *self
                .secondary_views
                .get(&view_id)
                .ok_or(CoreError::InvalidArgument("view ID does not exist"))?
        };
        gesture.samples = effect_samples(document_samples_for_view(
            view,
            coordinate_space,
            samples,
            document.width,
            document.height,
        )?)?;
        self.apply_airbrush_gesture_to_plane(plane_id, &gesture)
    }

    pub fn apply_stamp_to_plane(
        &mut self,
        plane_id: u64,
        stamp: Stamp,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_stamp(raster, selection, stamp, revision)
        })
    }

    pub fn apply_stamp_gesture_to_plane(
        &mut self,
        plane_id: u64,
        gesture: &StampGesture,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_stamp_gesture(raster, selection, gesture, revision)
        })
    }

    pub fn apply_stamp_gesture_for_view(
        &mut self,
        view_id: u64,
        coordinate_space: CoordinateSpace,
        plane_id: u64,
        source: StrokeSample,
        samples: &[StrokeSample],
        mut gesture: StampGesture,
    ) -> Result<DispatchOutcome, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let view = if view_id == 0 {
            self.view
        } else {
            *self
                .secondary_views
                .get(&view_id)
                .ok_or(CoreError::InvalidArgument("view ID does not exist"))?
        };
        let source = document_samples_for_view(
            view,
            coordinate_space,
            &[source],
            document.width,
            document.height,
        )?;
        gesture.samples = effect_samples(document_samples_for_view(
            view,
            coordinate_space,
            samples,
            document.width,
            document.height,
        )?)?;
        let source = effect_samples(source)?
            .into_iter()
            .next()
            .ok_or(CoreError::InvalidArgument("stamp source is absent"))?;
        gesture.source_x_milli = source.x_milli;
        gesture.source_y_milli = source.y_milli;
        self.apply_stamp_gesture_to_plane(plane_id, &gesture)
    }

    pub fn apply_blur_tool_to_plane(
        &mut self,
        plane_id: u64,
        shape: &SelectionShape,
        radius: u32,
        strength_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        let filter = Filter::GaussianBlur {
            radius,
            strength_milli,
        };
        self.apply_masked_raster_operation(plane_id, shape, |raster, mask, revision| {
            apply_filter(raster, Some(mask), &filter, revision)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_blur_tool_for_view(
        &mut self,
        view_id: u64,
        coordinate_space: CoordinateSpace,
        plane_id: u64,
        kind: EffectRegionKind,
        samples: &[StrokeSample],
        diameter: f32,
        pressure_size: bool,
        radius: u32,
        strength_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        if pressure_size {
            if kind != EffectRegionKind::Trace {
                return Err(CoreError::InvalidArgument(
                    "blur pressure is supported only for the pen region",
                ));
            }
            let mask =
                self.pressure_trace_mask_for_view(view_id, coordinate_space, samples, diameter)?;
            return self.apply_blur_tool_mask_to_plane(plane_id, mask, radius, strength_milli);
        }
        let shape =
            self.effect_region_for_view(view_id, coordinate_space, kind, samples, diameter)?;
        self.apply_blur_tool_to_plane(plane_id, &shape, radius, strength_milli)
    }

    pub fn apply_dust_removal_to_plane(
        &mut self,
        plane_id: u64,
        shape: Option<&SelectionShape>,
        options: DustRemoval,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let base_revision = self.document_revision;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let plane = editable_color_plane(&before, plane_id)?;
        let mut operation_mask = match shape {
            Some(shape) => Some(selection_mask_for_shape(&before, shape, revision)?),
            None => None,
        };
        if before.selection.allocated_tile_count() != 0 {
            operation_mask = Some(match operation_mask {
                Some(mask) => combine_selection_masks(
                    &before.selection,
                    &mask,
                    SelectionOperation::Intersect,
                    revision,
                )?,
                None => before.selection.clone(),
            });
        }
        let raster = apply_dust_removal(
            &plane.raster,
            operation_mask.as_ref(),
            options,
            revision,
            &mut progress,
        )?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "dust-removal base revision became stale",
            ));
        }
        let mut after = before.clone();
        after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("operation plane disappeared"))?
            .raster = raster;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_dust_removal_for_view(
        &mut self,
        view_id: u64,
        coordinate_space: CoordinateSpace,
        plane_id: u64,
        kind: Option<EffectRegionKind>,
        samples: &[StrokeSample],
        diameter: f32,
        options: DustRemoval,
        progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<DispatchOutcome, CoreError> {
        let shape = kind
            .map(|kind| {
                self.effect_region_for_view(view_id, coordinate_space, kind, samples, diameter)
            })
            .transpose()?;
        self.apply_dust_removal_to_plane(plane_id, shape.as_ref(), options, progress)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_dust_preview_for_view(
        &mut self,
        view_id: u64,
        coordinate_space: CoordinateSpace,
        plane_id: u64,
        kind: Option<EffectRegionKind>,
        samples: &[StrokeSample],
        diameter: f32,
        options: DustRemoval,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<FilterPreviewInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let base_revision = self.document_revision;
        let base_document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let shape = kind
            .map(|kind| {
                self.effect_region_for_view(view_id, coordinate_space, kind, samples, diameter)
            })
            .transpose()?;
        let plane = editable_color_plane(&base_document, plane_id)?;
        let mut operation_mask = shape
            .as_ref()
            .map(|shape| selection_mask_for_shape(&base_document, shape, preview_revision))
            .transpose()?;
        if base_document.selection.allocated_tile_count() != 0 {
            operation_mask = Some(match operation_mask {
                Some(mask) => combine_selection_masks(
                    &base_document.selection,
                    &mask,
                    SelectionOperation::Intersect,
                    preview_revision,
                )?,
                None => base_document.selection.clone(),
            });
        }
        let raster = apply_dust_removal(
            &plane.raster,
            operation_mask.as_ref(),
            options,
            preview_revision,
            &mut progress,
        )?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "dust-removal preview base revision became stale",
            ));
        }
        let mut preview_document = base_document.clone();
        preview_document
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("operation plane disappeared"))?
            .raster = raster;
        let info = preview_info(
            plane_id,
            &base_document,
            &preview_document,
            preview_revision,
        )?;
        self.filter_preview = Some(FilterPreview {
            plane_id,
            base_document,
            preview_document,
            filter: None,
            preview_revision,
        });
        self.render_cache.clear();
        Ok(info)
    }

    pub fn edit_plane_alpha(
        &mut self,
        plane_id: u64,
        alpha: &TileRaster,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            edit_alpha(raster, selection, alpha, revision)
        })
    }

    pub fn apply_alpha_gradient_to_plane(
        &mut self,
        plane_id: u64,
        gradient: &Gradient,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_alpha_gradient(raster, selection, gradient, revision)
        })
    }
}
