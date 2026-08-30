use super::helpers::*;
use super::*;
use crate::primitive::{CanonicalInvocation, InvocationResult};

impl Core {
    /// Applies a gradient to an editable raster plane as one undoable edit.
    pub fn apply_gradient_to_plane(
        &mut self,
        plane_id: u64,
        gradient: &Gradient,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplyGradient {
                    plane_id,
                    gradient: gradient.clone(),
                })
                .map(|result| result.dispatch);
        }
        self.apply_raster_operation(
            PlaneId::from_raw(plane_id),
            |raster, selection, revision| apply_gradient(raster, selection, gradient, revision),
        )
    }

    /// Applies a boundary-aware airbrush effect as one atomic undoable edit.
    pub fn apply_boundary_airbrush_to_plane(
        &mut self,
        plane_id: u64,
        effect: &BoundaryAirbrush,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplyBoundaryAirbrush {
                    plane_id,
                    effect: effect.clone(),
                })
                .map(|result| result.dispatch);
        }
        self.apply_raster_operation(
            PlaneId::from_raw(plane_id),
            |raster, selection, revision| {
                apply_boundary_airbrush(raster, selection, effect, revision)
            },
        )
    }

    /// Applies Gaussian blur to a plane, limited by the document selection.
    ///
    /// Invalid parameters or processing failure leave live state unchanged.
    pub fn apply_blur_to_plane(
        &mut self,
        plane_id: u64,
        radius: u32,
        strength_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplyBlur {
                    plane_id,
                    radius,
                    strength_milli,
                })
                .map(|result| result.dispatch);
        }
        let filter = Filter::GaussianBlur {
            radius,
            strength_milli,
        };
        self.apply_raster_operation(
            PlaneId::from_raw(plane_id),
            |raster, selection, revision| apply_filter(raster, selection, &filter, revision),
        )
    }

    /// Applies one airbrush stroke to a plane as one undoable edit.
    pub fn apply_airbrush_to_plane(
        &mut self,
        plane_id: u64,
        stroke: AirbrushStroke,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplyAirbrush {
                    plane_id,
                    stroke,
                })
                .map(|result| result.dispatch);
        }
        self.apply_raster_operation(
            PlaneId::from_raw(plane_id),
            |raster, selection, revision| apply_airbrush(raster, selection, stroke, revision),
        )
    }

    /// Applies a complete document-coordinate airbrush gesture atomically.
    pub fn apply_airbrush_gesture_to_plane(
        &mut self,
        plane_id: u64,
        gesture: &AirbrushGesture,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplyAirbrushGesture {
                    plane_id,
                    gesture: gesture.clone(),
                })
                .map(|result| result.dispatch);
        }
        self.apply_raster_operation(
            PlaneId::from_raw(plane_id),
            |raster, selection, revision| {
                apply_airbrush_gesture(raster, selection, gesture, revision)
            },
        )
    }

    /// Converts samples through a primary (`view_id == 0`) or secondary view and
    /// applies one airbrush gesture as an atomic edit.
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
                .get(&ViewId::from_raw(view_id))
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

    /// Applies one stamp to a plane as one undoable edit.
    pub fn apply_stamp_to_plane(
        &mut self,
        plane_id: u64,
        stamp: Stamp,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplyStamp { plane_id, stamp })
                .map(|result| result.dispatch);
        }
        self.apply_raster_operation(
            PlaneId::from_raw(plane_id),
            |raster, selection, revision| apply_stamp(raster, selection, stamp, revision),
        )
    }

    /// Applies a complete document-coordinate stamp gesture atomically.
    pub fn apply_stamp_gesture_to_plane(
        &mut self,
        plane_id: u64,
        gesture: &StampGesture,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplyStampGesture {
                    plane_id,
                    gesture: gesture.clone(),
                })
                .map(|result| result.dispatch);
        }
        self.apply_raster_operation(
            PlaneId::from_raw(plane_id),
            |raster, selection, revision| apply_stamp_gesture(raster, selection, gesture, revision),
        )
    }

    /// Converts source/destination samples through a selected view and applies one
    /// stamp gesture as an atomic undoable edit.
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
                .get(&ViewId::from_raw(view_id))
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

    /// Applies blur inside a document-space shape as one undoable edit.
    pub fn apply_blur_tool_to_plane(
        &mut self,
        plane_id: u64,
        shape: &SelectionShape,
        radius: u32,
        strength_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplyBlurTool {
                    plane_id,
                    shape: shape.clone(),
                    radius,
                    strength_milli,
                })
                .map(|result| result.dispatch);
        }
        let filter = Filter::GaussianBlur {
            radius,
            strength_milli,
        };
        self.apply_masked_raster_operation(
            PlaneId::from_raw(plane_id),
            shape,
            |raster, mask, revision| apply_filter(raster, Some(mask), &filter, revision),
        )
    }

    #[allow(clippy::too_many_arguments)]
    /// Builds an effect region from primary/secondary-view samples and blurs it.
    ///
    /// Coordinates use `coordinate_space`; pressure sizing is valid only for trace
    /// regions. Conversion or effect failure is atomic.
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
            if !self.canonical_invocation_is_active() {
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
                let diameter = match coordinate_space {
                    CoordinateSpace::Document => diameter,
                    CoordinateSpace::Device => (f64::from(diameter) / view.zoom.get()) as f32,
                };
                return self
                    .execute_canonical_invocation(CanonicalInvocation::ApplyBlurPressureTrace {
                        plane_id,
                        samples,
                        diameter,
                        radius,
                        strength_milli,
                    })
                    .map(|result| result.dispatch);
            }
            let mask =
                self.pressure_trace_mask_for_view(view_id, coordinate_space, samples, diameter)?;
            return self.apply_blur_tool_mask_to_plane(
                PlaneId::from_raw(plane_id),
                mask,
                radius,
                strength_milli,
            );
        }
        let shape =
            self.effect_region_for_view(view_id, coordinate_space, kind, samples, diameter)?;
        self.apply_blur_tool_to_plane(plane_id, &shape, radius, strength_milli)
    }

    /// Removes bounded dust regions from a plane with cooperative cancellation.
    ///
    /// The optional shape intersects the document selection. Cancellation, stale
    /// revision, or processing failure never commits a partial raster.
    pub fn apply_dust_removal_to_plane(
        &mut self,
        plane_id: u64,
        shape: Option<&SelectionShape>,
        options: DustRemoval,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            let shape = shape.cloned();
            let staged_shape = shape.clone();
            return self
                .execute_canonical_invocation_with(
                    CanonicalInvocation::ApplyDustRemoval {
                        plane_id,
                        shape,
                        options,
                    },
                    move |staged| {
                        staged
                            .apply_dust_removal_internal(
                                plane_id,
                                staged_shape.as_ref(),
                                options,
                                &mut progress,
                            )
                            .map(InvocationResult::dispatch)
                    },
                )
                .map(|result| result.dispatch);
        }
        self.apply_dust_removal_internal(plane_id, shape, options, &mut progress)
    }

    fn apply_dust_removal_internal(
        &mut self,
        plane_id: u64,
        shape: Option<&SelectionShape>,
        options: DustRemoval,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let plane_id = PlaneId::from_raw(plane_id);
        let base_revision = self.document_revision;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let plane = editable_rgba_plane(&before, plane_id)?;
        let mut operation_mask = match shape {
            Some(shape) => Some(selection_mask_for_shape(
                &before,
                plane_id,
                shape,
                RangeInterpretation::Normal,
                SelectionConstructionOptions::default(),
                revision.get(),
            )?),
            None => None,
        };
        if before.selection.allocated_tile_count() != 0 {
            operation_mask = Some(match operation_mask {
                Some(mask) => combine_selection_masks(
                    &before.selection,
                    &mask,
                    SelectionOperation::Intersect,
                    revision.get(),
                )?,
                None => before.selection.clone(),
            });
        }
        let raster = apply_dust_removal(
            &plane.raster,
            operation_mask.as_ref(),
            options,
            revision.get(),
            progress,
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
        self.commit_deferred_document_edit(before, after, base_revision, revision)
    }

    #[allow(clippy::too_many_arguments)]
    /// Converts an optional view-space region and applies atomic dust removal.
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
    /// Starts a cancellable dust-removal preview for a view-derived region.
    ///
    /// Success publishes only transient preview state; cancellation, failure, and
    /// stale revision leave the live document and any history unchanged.
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
        let plane_id = PlaneId::from_raw(plane_id);
        let base_revision = self.document_revision;
        let base_document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let shape = kind
            .map(|kind| {
                self.effect_region_for_view(view_id, coordinate_space, kind, samples, diameter)
            })
            .transpose()?;
        let plane = editable_rgba_plane(&base_document, plane_id)?;
        let mut operation_mask = shape
            .as_ref()
            .map(|shape| {
                selection_mask_for_shape(
                    &base_document,
                    plane_id,
                    shape,
                    RangeInterpretation::Normal,
                    SelectionConstructionOptions::default(),
                    preview_revision.get(),
                )
            })
            .transpose()?;
        if base_document.selection.allocated_tile_count() != 0 {
            operation_mask = Some(match operation_mask {
                Some(mask) => combine_selection_masks(
                    &base_document.selection,
                    &mask,
                    SelectionOperation::Intersect,
                    preview_revision.get(),
                )?,
                None => base_document.selection.clone(),
            });
        }
        let raster = apply_dust_removal(
            &plane.raster,
            operation_mask.as_ref(),
            options,
            preview_revision.get(),
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
            base_revision,
            base_document,
            preview_document,
            procedure: PreviewProcedure::Dust { shape, options },
            preview_revision,
        });
        self.render_cache.clear();
        Ok(info)
    }

    /// Replaces a plane's alpha from a same-sized mask as one undoable edit.
    pub fn edit_plane_alpha(
        &mut self,
        plane_id: u64,
        alpha: &TileRaster,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::EditPlaneAlpha {
                    plane_id,
                    alpha: alpha.clone(),
                })
                .map(|result| result.dispatch);
        }
        self.apply_raster_operation(
            PlaneId::from_raw(plane_id),
            |raster, selection, revision| edit_alpha(raster, selection, alpha, revision),
        )
    }

    /// Applies a gradient only to plane alpha as one undoable edit.
    pub fn apply_alpha_gradient_to_plane(
        &mut self,
        plane_id: u64,
        gradient: &Gradient,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplyAlphaGradient {
                    plane_id,
                    gradient: gradient.clone(),
                })
                .map(|result| result.dispatch);
        }
        self.apply_raster_operation(
            PlaneId::from_raw(plane_id),
            |raster, selection, revision| {
                apply_alpha_gradient(raster, selection, gradient, revision)
            },
        )
    }
}
