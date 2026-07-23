use super::{
    Adjustment, AirbrushGesture, AirbrushStroke, BoundaryAirbrush, CellDocument, CoordinateSpace,
    Core, CoreError, DispatchOutcome, DustRemoval, EffectRegionKind, EffectSample, Filter,
    Gradient, LayerKind, LayerNode, PixelFormat, PlaneType, PointF32, RectI32, SelectionOperation,
    SelectionShape, Stamp, StampGesture, StrokeSample, combine_selection_masks,
    document_samples_for_view, selection_mask_for_shape,
};
use inkpod_image::{
    TileRaster, apply_airbrush, apply_airbrush_gesture, apply_alpha_gradient,
    apply_boundary_airbrush, apply_dust_removal, apply_filter, apply_filter_with_progress,
    apply_gradient, apply_stamp, apply_stamp_gesture, edit_alpha,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilterPreviewInfo {
    pub plane_id: u64,
    pub base_checksum: u64,
    pub preview_checksum: u64,
    pub preview_revision: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct FilterPreview {
    pub(crate) plane_id: u64,
    pub(crate) base_document: CellDocument,
    pub(crate) preview_document: CellDocument,
    pub(crate) filter: Option<Filter>,
    pub(crate) preview_revision: u64,
}

impl Core {
    pub fn begin_filter_preview(
        &mut self,
        plane_id: u64,
        filter: Filter,
    ) -> Result<FilterPreviewInfo, CoreError> {
        self.begin_filter_preview_with_progress(plane_id, filter, |_, _| true)
    }

    pub fn begin_filter_preview_with_progress(
        &mut self,
        plane_id: u64,
        filter: Filter,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<FilterPreviewInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let base_revision = self.document_revision;
        let base_document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let preview_document = filter_document_with_progress(
            &base_document,
            plane_id,
            &filter,
            preview_revision,
            &mut progress,
        )?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "filter preview base revision became stale",
            ));
        }
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
            filter: Some(filter),
            preview_revision,
        });
        self.render_cache.clear();
        Ok(info)
    }

    pub fn update_filter_preview(
        &mut self,
        plane_id: u64,
        filter: Filter,
    ) -> Result<FilterPreviewInfo, CoreError> {
        self.update_filter_preview_with_progress(plane_id, filter, |_, _| true)
    }

    pub fn update_filter_preview_with_progress(
        &mut self,
        plane_id: u64,
        filter: Filter,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<FilterPreviewInfo, CoreError> {
        let base_revision = self.document_revision;
        let (active_plane_id, base_document) = self
            .filter_preview
            .as_ref()
            .map(|preview| (preview.plane_id, preview.base_document.clone()))
            .ok_or(CoreError::InvalidState("there is no active filter preview"))?;
        if plane_id != active_plane_id {
            return Err(CoreError::InvalidArgument(
                "filter update plane does not match the active preview",
            ));
        }
        let preview_revision = self.allocate_preview_revision()?;
        let preview_document = filter_document_with_progress(
            &base_document,
            plane_id,
            &filter,
            preview_revision,
            &mut progress,
        )?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "filter preview base revision became stale",
            ));
        }
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
            filter: Some(filter),
            preview_revision,
        });
        self.render_cache.clear();
        Ok(info)
    }

    pub fn cancel_filter_preview(&mut self) -> Result<FilterPreviewInfo, CoreError> {
        let preview = self
            .filter_preview
            .take()
            .ok_or(CoreError::InvalidState("there is no active filter preview"))?;
        self.render_cache.clear();
        let checksum = preview
            .base_document
            .plane_by_id(preview.plane_id)
            .ok_or(CoreError::InvalidState("preview plane no longer exists"))?
            .raster
            .checksum();
        Ok(FilterPreviewInfo {
            plane_id: preview.plane_id,
            base_checksum: checksum,
            preview_checksum: checksum,
            preview_revision: self.document_revision,
        })
    }

    pub fn apply_filter_preview(&mut self) -> Result<DispatchOutcome, CoreError> {
        let preview = self
            .filter_preview
            .as_ref()
            .cloned()
            .ok_or(CoreError::InvalidState("there is no active filter preview"))?;
        let result = self.commit_document_edit(preview.base_document, preview.preview_document);
        if result.is_ok() {
            self.filter_preview = None;
            if let Some(filter) = preview.filter {
                self.last_filter = Some(filter);
            }
        }
        result
    }

    pub fn apply_last_filter(&mut self, plane_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.apply_last_filter_with_progress(plane_id, |_, _| true)
    }

    pub fn apply_last_filter_with_progress(
        &mut self,
        plane_id: u64,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let base_revision = self.document_revision;
        let filter = self
            .last_filter
            .clone()
            .ok_or(CoreError::InvalidState("there is no last filter"))?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let after =
            filter_document_with_progress(&before, plane_id, &filter, revision, &mut progress)?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "last-filter base revision became stale",
            ));
        }
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn create_adjustment_layer(
        &mut self,
        name: &str,
        adjustment: Adjustment,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        super::validate_node_name(name)?;
        inkpod_image::apply_adjustment(super::PixelValue::Rgba([0; 4]), &adjustment)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.layers.len() >= super::MAX_LAYERS {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        let layer_id = self.next_id;
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(CoreError::InvalidState("stable ID overflow"))?;
        let mut after = before.clone();
        after.layers.insert(
            0,
            LayerNode {
                id: layer_id,
                kind: LayerKind::Adjustment,
                name: super::unique_layer_name(&after.layers, name),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                planes: Vec::new(),
            },
        );
        after.adjustments.insert(layer_id, adjustment);
        after.active_layer_id = layer_id;
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, layer_id))
    }

    pub fn update_adjustment_layer(
        &mut self,
        layer_id: u64,
        adjustment: Adjustment,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        inkpod_image::apply_adjustment(super::PixelValue::Rgba([0; 4]), &adjustment)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let layer = after
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if layer.kind != LayerKind::Adjustment {
            return Err(CoreError::InvalidArgument(
                "layer is not an adjustment layer",
            ));
        }
        after.adjustments.insert(layer_id, adjustment);
        self.commit_document_edit(before, after)
    }

    pub fn adjustment(&self, layer_id: u64) -> Result<&Adjustment, CoreError> {
        self.document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .adjustments
            .get(&layer_id)
            .ok_or(CoreError::InvalidArgument(
                "adjustment layer ID does not exist",
            ))
    }

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

    fn apply_masked_raster_operation<F>(
        &mut self,
        plane_id: u64,
        shape: &SelectionShape,
        operation: F,
    ) -> Result<DispatchOutcome, CoreError>
    where
        F: FnOnce(&TileRaster, &TileRaster, u64) -> Result<TileRaster, inkpod_image::RasterError>,
    {
        self.ensure_no_active_stroke()?;
        let base_revision = self.document_revision;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let plane = editable_color_plane(&before, plane_id)?;
        let mut mask = selection_mask_for_shape(&before, shape, revision)?;
        if before.selection.allocated_tile_count() != 0 {
            mask = combine_selection_masks(
                &before.selection,
                &mask,
                SelectionOperation::Intersect,
                revision,
            )?;
        }
        let raster = operation(&plane.raster, &mask, revision)?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "masked image-edit base revision became stale",
            ));
        }
        let mut after = before.clone();
        after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("operation plane disappeared"))?
            .raster = raster;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    fn apply_raster_operation<F>(
        &mut self,
        plane_id: u64,
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
        let base_revision = self.document_revision;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let plane = editable_color_plane(&before, plane_id)?;
        let selection = (before.selection.allocated_tile_count() != 0).then_some(&before.selection);
        let raster = operation(&plane.raster, selection, revision)?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "image-edit base revision became stale",
            ));
        }
        let mut after = before.clone();
        after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("operation plane disappeared"))?
            .raster = raster;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    fn apply_blur_tool_mask_to_plane(
        &mut self,
        plane_id: u64,
        mut mask: TileRaster,
        radius: u32,
        strength_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let base_revision = self.document_revision;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if mask.width() != before.width
            || mask.height() != before.height
            || mask.format() != PixelFormat::BinaryMask8
        {
            return Err(CoreError::InvalidArgument(
                "blur pressure mask does not match the document",
            ));
        }
        let revision = self.next_document_revision()?;
        let plane = editable_color_plane(&before, plane_id)?;
        if before.selection.allocated_tile_count() != 0 {
            mask = combine_selection_masks(
                &before.selection,
                &mask,
                SelectionOperation::Intersect,
                revision,
            )?;
        }
        let raster = apply_filter(
            &plane.raster,
            Some(&mask),
            &Filter::GaussianBlur {
                radius,
                strength_milli,
            },
            revision,
        )?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "blur-tool base revision became stale",
            ));
        }
        let mut after = before.clone();
        after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("operation plane disappeared"))?
            .raster = raster;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    fn pressure_trace_mask_for_view(
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
                .get(&view_id)
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
            CoordinateSpace::Device => f64::from(diameter) / view.zoom,
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
                    mask.set_pixel(x, y, super::PixelValue::Binary(255), self.document_revision)?;
                }
            }
        }
        Ok(mask)
    }

    fn effect_region_for_view(
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
                .get(&view_id)
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
                x: sample.x,
                y: sample.y,
            })
            .collect();
        match kind {
            EffectRegionKind::Trace => {
                let diameter = match coordinate_space {
                    CoordinateSpace::Document => diameter,
                    CoordinateSpace::Device => (f64::from(diameter) / view.zoom) as f32,
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

fn pressure_trace_contains(x: f64, y: f64, samples: &[StrokeSample], diameter: f64) -> bool {
    if samples.len() == 1 {
        let radius = diameter * f64::from(samples[0].pressure.clamp(0.0, 1.0)) / 2.0;
        return (x - f64::from(samples[0].x)).hypot(y - f64::from(samples[0].y)) <= radius;
    }
    samples.windows(2).any(|segment| {
        let start_x = f64::from(segment[0].x);
        let start_y = f64::from(segment[0].y);
        let dx = f64::from(segment[1].x) - start_x;
        let dy = f64::from(segment[1].y) - start_y;
        let length_squared = dx.mul_add(dx, dy * dy);
        let t = if length_squared <= f64::EPSILON {
            0.0
        } else {
            ((x - start_x).mul_add(dx, (y - start_y) * dy) / length_squared).clamp(0.0, 1.0)
        };
        let center_x = dx.mul_add(t, start_x);
        let center_y = dy.mul_add(t, start_y);
        let start_pressure = f64::from(segment[0].pressure.clamp(0.0, 1.0));
        let end_pressure = f64::from(segment[1].pressure.clamp(0.0, 1.0));
        let pressure = (end_pressure - start_pressure).mul_add(t, start_pressure);
        let radius = diameter * pressure / 2.0;
        (x - center_x).hypot(y - center_y) <= radius
    })
}

fn effect_samples(samples: Vec<StrokeSample>) -> Result<Vec<EffectSample>, CoreError> {
    samples
        .into_iter()
        .map(|sample| {
            let x = (f64::from(sample.x) * 1_000.0).round();
            let y = (f64::from(sample.y) * 1_000.0).round();
            if !(i64::MIN as f64..=i64::MAX as f64).contains(&x)
                || !(i64::MIN as f64..=i64::MAX as f64).contains(&y)
            {
                return Err(CoreError::InvalidArgument(
                    "effect sample coordinate is outside fixed-point bounds",
                ));
            }
            Ok(EffectSample {
                x_milli: x as i64,
                y_milli: y as i64,
                pressure_milli: (f64::from(sample.pressure) * 1_000.0)
                    .round()
                    .clamp(0.0, 1_000.0) as u32,
            })
        })
        .collect()
}

fn editable_color_plane(
    document: &CellDocument,
    plane_id: u64,
) -> Result<&super::PlaneNode, CoreError> {
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.planes.iter().any(|plane| plane.id == plane_id))
        .ok_or(CoreError::InvalidArgument("plane ID does not exist"))?;
    let plane = layer
        .planes
        .iter()
        .find(|plane| plane.id == plane_id)
        .expect("located containing layer");
    if !layer.editable || !plane.editable {
        return Err(CoreError::InvalidState("target plane is locked"));
    }
    if !matches!(plane.kind, PlaneType::Color | PlaneType::Raster)
        || !matches!(
            plane.raster.format(),
            PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
        )
    {
        return Err(CoreError::InvalidArgument(
            "target is not an editable RGBA raster plane",
        ));
    }
    Ok(plane)
}

fn filter_document_with_progress(
    base: &CellDocument,
    plane_id: u64,
    filter: &Filter,
    revision: u64,
    progress: &mut impl FnMut(u64, u64) -> bool,
) -> Result<CellDocument, CoreError> {
    let plane = editable_color_plane(base, plane_id)?;
    let selection = (base.selection.allocated_tile_count() != 0).then_some(&base.selection);
    let raster = apply_filter_with_progress(&plane.raster, selection, filter, revision, progress)?;
    let mut preview = base.clone();
    preview
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("preview plane disappeared"))?
        .raster = raster;
    Ok(preview)
}

fn preview_info(
    plane_id: u64,
    base: &CellDocument,
    preview: &CellDocument,
    preview_revision: u64,
) -> Result<FilterPreviewInfo, CoreError> {
    Ok(FilterPreviewInfo {
        plane_id,
        base_checksum: base
            .plane_by_id(plane_id)
            .ok_or(CoreError::InvalidState("preview plane disappeared"))?
            .raster
            .checksum(),
        preview_checksum: preview
            .plane_by_id(plane_id)
            .ok_or(CoreError::InvalidState("preview plane disappeared"))?
            .raster
            .checksum(),
        preview_revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Channel, CurveInterpolation, CurvePoint, DustMode, EffectSample, PixelValue, PointF32,
    };

    fn seeded_core() -> (Core, u64) {
        let mut core = Core::new();
        core.new_cell(4, 1, 96_000, 96_000).unwrap();
        let plane_id = core.document.as_ref().unwrap().primary_ids().2;
        let plane = core
            .document
            .as_mut()
            .unwrap()
            .plane_by_id_mut(plane_id)
            .unwrap();
        for (x, color) in [
            [20, 40, 60, 255],
            [80, 100, 120, 128],
            [160, 180, 200, 255],
            [220, 230, 240, 255],
        ]
        .into_iter()
        .enumerate()
        {
            plane
                .raster
                .set_pixel(x as u32, 0, PixelValue::Rgba(color), 1)
                .unwrap();
        }
        (core, plane_id)
    }

    #[test]
    fn m6_acceptance_cancel_restores_the_original_tile_checksum() {
        let (mut core, plane_id) = seeded_core();
        let original = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        let preview = core
            .begin_filter_preview(
                plane_id,
                Filter::Invert {
                    channel: Channel::Rgb,
                },
            )
            .unwrap();
        assert_eq!(preview.base_checksum, original);
        assert_ne!(preview.preview_checksum, original);
        assert_ne!(
            core.build_snapshot().revision(),
            core.document_info().unwrap().document_revision
        );
        let cancelled = core.cancel_filter_preview().unwrap();
        assert_eq!(cancelled.preview_checksum, original);
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            original
        );
    }

    #[test]
    fn m6_acceptance_apply_is_exactly_one_undo_unit_and_last_filter_reuses_it() {
        let (mut core, plane_id) = seeded_core();
        let original = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        core.begin_filter_preview(
            plane_id,
            Filter::BrightnessContrast {
                brightness_milli: 100,
                contrast_milli: 200,
            },
        )
        .unwrap();
        core.apply_filter_preview().unwrap();
        assert_eq!(core.history.len(), 1);
        let filtered = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        assert_ne!(filtered, original);
        core.undo().unwrap();
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            original
        );
        core.redo().unwrap();
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            filtered
        );
        core.apply_last_filter(plane_id).unwrap();
        assert_eq!(core.history.len(), 2);
    }

    #[test]
    fn m6_acceptance_adjustment_order_changes_composite_without_changing_source_plane() {
        let (mut core, plane_id) = seeded_core();
        let unadjusted = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();
        let original = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        let (_, brightness) = core
            .create_adjustment_layer(
                "Brightness",
                Adjustment::BrightnessContrast {
                    brightness_milli: 200,
                    contrast_milli: 0,
                },
            )
            .unwrap();
        let (_, curve) = core
            .create_adjustment_layer(
                "Curve",
                Adjustment::ToneCurve {
                    channel: Channel::Rgb,
                    interpolation: CurveInterpolation::Bezier,
                    points: vec![
                        CurvePoint {
                            input: 0,
                            output: 0,
                        },
                        CurvePoint {
                            input: 32_768,
                            output: 8_000,
                        },
                        CurvePoint {
                            input: 65_535,
                            output: 65_535,
                        },
                    ],
                },
            )
            .unwrap();
        let first = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();
        core.reorder_layer(brightness, 0).unwrap();
        let second = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();
        assert_ne!(first, second);
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            original
        );
        assert!(core.adjustment(curve).is_ok());

        core.set_layer_properties(brightness, true, true, 0, "Brightness")
            .unwrap();
        core.set_layer_properties(curve, false, true, 1_000, "Curve")
            .unwrap();
        assert_eq!(core.build_snapshot().tiles()[0].pixels()[..4], unadjusted);
        core.set_layer_properties(brightness, true, true, 1_000, "Brightness")
            .unwrap();
        core.set_layer_properties(curve, true, true, 1_000, "Curve")
            .unwrap();
        let second = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "inkpod-m6-adjustment-{}-{nonce}.inkpod",
            std::process::id()
        ));
        core.save(&path).unwrap();
        let mut reopened = Core::new();
        reopened.open(&path).unwrap();
        assert_eq!(
            reopened.adjustment(curve).unwrap(),
            core.adjustment(curve).unwrap()
        );
        assert_eq!(reopened.build_snapshot().tiles()[0].pixels()[..4], second);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn m6_acceptance_boundary_airbrush_preserves_uniform_regions() {
        let mut source = TileRaster::new(7, 1, PixelFormat::StraightRgba8).unwrap();
        for x in 0..7 {
            source
                .set_pixel(
                    x,
                    0,
                    PixelValue::Rgba(if x < 3 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    }),
                    1,
                )
                .unwrap();
        }
        let output = apply_boundary_airbrush(
            &source,
            None,
            &BoundaryAirbrush {
                colors: vec![[65_535, 0, 0, 65_535], [0, 0, 65_535, 65_535]],
                width: 1,
                strength_milli: 1_000,
            },
            2,
        )
        .unwrap();
        assert_eq!(source.pixel(0, 0).unwrap(), output.pixel(0, 0).unwrap());
        assert_eq!(source.pixel(6, 0).unwrap(), output.pixel(6, 0).unwrap());
        assert_ne!(source.pixel(2, 0).unwrap(), output.pixel(2, 0).unwrap());
    }

    #[test]
    fn generic_adjustment_tree_edits_remain_saveable_and_reject_ambiguous_merge() {
        let (mut core, _) = seeded_core();
        let (_, first) = core
            .create_layer(LayerKind::Adjustment, "Generic Adjustment")
            .unwrap();
        let (_, second) = core.duplicate_layer(first).unwrap();
        assert!(core.adjustment(first).is_ok());
        assert!(core.adjustment(second).is_ok());
        assert!(inkpod_format::encode(&core.document.as_ref().unwrap().to_file()).is_ok());
        assert!(matches!(
            core.merge_layer_into_below(second),
            Err(CoreError::InvalidArgument(_))
        ));
    }

    #[test]
    fn m6_noop_invalid_and_adjustment_update_history_are_transactional() {
        let (mut core, plane_id) = seeded_core();
        let history = core.history.len();
        core.begin_filter_preview(
            plane_id,
            Filter::BrightnessContrast {
                brightness_milli: 0,
                contrast_milli: 0,
            },
        )
        .unwrap();
        let outcome = core.apply_filter_preview().unwrap();
        assert_eq!(outcome.revision(), core.document_revision);
        assert_eq!(core.history.len(), history);

        assert!(matches!(
            core.begin_filter_preview(
                plane_id,
                Filter::BrightnessContrast {
                    brightness_milli: i32::MIN,
                    contrast_milli: 0,
                }
            ),
            Err(CoreError::Raster(_))
        ));
        assert_eq!(core.history.len(), history);

        let (_, adjustment_id) = core
            .create_adjustment_layer(
                "Editable",
                Adjustment::BrightnessContrast {
                    brightness_milli: 100,
                    contrast_milli: 0,
                },
            )
            .unwrap();
        let before_update = core.history.len();
        core.update_adjustment_layer(
            adjustment_id,
            Adjustment::BrightnessContrast {
                brightness_milli: 200,
                contrast_milli: -100,
            },
        )
        .unwrap();
        assert_eq!(core.history.len(), before_update + 1);
        core.undo().unwrap();
        assert_eq!(
            core.adjustment(adjustment_id).unwrap(),
            &Adjustment::BrightnessContrast {
                brightness_milli: 100,
                contrast_milli: 0,
            }
        );
        core.redo().unwrap();
        let updated = Adjustment::BrightnessContrast {
            brightness_milli: 200,
            contrast_milli: -100,
        };
        assert_eq!(core.adjustment(adjustment_id).unwrap(), &updated);
        let after_redo = core.history.len();
        let outcome = core
            .update_adjustment_layer(adjustment_id, updated)
            .unwrap();
        assert_eq!(outcome.revision(), core.document_revision);
        assert_eq!(core.history.len(), after_redo);
    }

    #[test]
    fn m6_full_effect_gestures_dust_and_alpha_are_atomic() {
        let (mut core, plane_id) = seeded_core();
        let original = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        let history = core.history.len();
        core.apply_airbrush_gesture_to_plane(
            plane_id,
            &AirbrushGesture {
                samples: vec![
                    EffectSample {
                        x_milli: 500,
                        y_milli: 500,
                        pressure_milli: 250,
                    },
                    EffectSample {
                        x_milli: 3_500,
                        y_milli: 500,
                        pressure_milli: 1_000,
                    },
                ],
                radius_milli: 500,
                hardness_milli: 1_000,
                spacing_milli: 500,
                opacity_milli: 1_000,
                fade_milli: 0,
                pressure_size: true,
                pressure_opacity: true,
                continuous_dabs: 1,
                color: [0, 0, 65_535, 65_535],
            },
        )
        .unwrap();
        assert_eq!(core.history.len(), history + 1);
        assert_ne!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            original
        );
        core.undo().unwrap();
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            original
        );

        core.apply_blur_tool_to_plane(
            plane_id,
            &SelectionShape::Trace {
                points: vec![PointF32 { x: 1.0, y: 0.5 }, PointF32 { x: 2.0, y: 0.5 }],
                diameter: 2.0,
            },
            1,
            1_000,
        )
        .unwrap();
        assert_eq!(core.history.len(), history + 1);
        core.undo().unwrap();

        let mut alpha_before = Vec::new();
        for x in 0..4 {
            alpha_before.push(
                core.document
                    .as_ref()
                    .unwrap()
                    .plane_by_id(plane_id)
                    .unwrap()
                    .raster
                    .pixel(x, 0)
                    .unwrap()
                    .rgba16()
                    .unwrap(),
            );
        }
        core.apply_alpha_gradient_to_plane(
            plane_id,
            &Gradient {
                kind: crate::GradientKind::Linear,
                mode: crate::GradientMode::Overwrite,
                start_x_milli: 500,
                start_y_milli: 500,
                end_x_milli: 3_500,
                end_y_milli: 500,
                dither: false,
                stops: vec![
                    crate::GradientStop {
                        position_milli: 0,
                        color: [0, 0, 0, 0],
                    },
                    crate::GradientStop {
                        position_milli: 500,
                        color: [0, 0, 0, 32_768],
                    },
                    crate::GradientStop {
                        position_milli: 1_000,
                        color: [0, 0, 0, 65_535],
                    },
                ],
            },
        )
        .unwrap();
        for (x, before) in alpha_before.into_iter().enumerate() {
            let after = core
                .document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .pixel(x as u32, 0)
                .unwrap()
                .rgba16()
                .unwrap();
            assert_eq!(&after[..3], &before[..3]);
        }
    }

    #[test]
    fn m6_worker_cancel_and_dust_never_commit_partial_results() {
        let (mut core, plane_id) = seeded_core();
        let revision = core.document_revision;
        let history = core.history.len();
        assert!(matches!(
            core.begin_filter_preview_with_progress(plane_id, Filter::AutoContrast, |_, _| false),
            Err(CoreError::Cancelled)
        ));
        assert!(core.filter_preview.is_none());
        assert_eq!(core.document_revision, revision);
        assert_eq!(core.history.len(), history);

        let checksum = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        let mut polls = 0;
        assert!(matches!(
            core.apply_dust_removal_to_plane(
                plane_id,
                Some(&SelectionShape::Rectangle(crate::RectI32 {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 1
                })),
                DustRemoval {
                    mode: DustMode::ReplaceColorOutliers,
                    maximum_pixels: 1
                },
                |_, _| {
                    polls += 1;
                    polls < 2
                },
            ),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(core.document_revision, revision);
        assert_eq!(core.history.len(), history);
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            checksum
        );
    }

    #[test]
    fn m6_blur_pen_pressure_varies_the_screen_fixed_region() {
        let samples = [
            StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 0.25,
            },
            StrokeSample {
                x: 5.0,
                y: 1.0,
                pressure: 1.0,
            },
        ];
        assert!(!pressure_trace_contains(1.0, 2.0, &samples, 4.0));
        assert!(pressure_trace_contains(5.0, 2.0, &samples, 4.0));
        assert!(pressure_trace_contains(3.0, 1.0, &samples, 4.0));
    }
}
