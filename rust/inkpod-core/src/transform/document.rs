use super::frame::*;
use super::numeric::*;
use super::raster::*;
use crate::document::bounded_document_pixels;
use crate::primitive::CanonicalInvocation;
use crate::*;

impl Core {
    /// Mirrors all document content and document-space metadata about an axis.
    ///
    /// This is a destructive document transform, distinct from view flip. Success
    /// is one undoable edit; any raster failure is atomic.
    pub fn mirror_document(&mut self, axis: MirrorAxis) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::MirrorDocument { axis })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let after = edit.working_mut();
        for plane in after
            .layers
            .iter_mut()
            .flat_map(|layer| layer.planes.iter_mut())
        {
            plane.raster = mirror_raster(&plane.raster, axis, revision.get())?;
        }
        after.selection = mirror_raster(&after.selection, axis, revision.get())?;
        for saved_selection in &mut after.saved_selection_masks {
            saved_selection.raster = mirror_raster(&saved_selection.raster, axis, revision.get())?;
        }
        after.fill_protection = mirror_raster(&after.fill_protection, axis, revision.get())?;
        let document_size = DocumentSizeU32::new(after.width, after.height);
        mirror_frame_metadata(&mut after.frames, document_size, axis)?;
        if let Some(frame) = &mut after.shooting_frame {
            crate::shooting_frame::mirror_shooting_frame(frame, document_size, axis)?;
        }
        for guide in &mut after.guides {
            match (axis, guide.axis) {
                (MirrorAxis::Horizontal, GuideAxis::Vertical) => {
                    guide.position = i32::try_from(document_size.width).map_err(|_| {
                        CoreError::InvalidState("document width exceeds guide range")
                    })? - guide.position;
                }
                (MirrorAxis::Vertical, GuideAxis::Horizontal) => {
                    guide.position = i32::try_from(document_size.height).map_err(|_| {
                        CoreError::InvalidState("document height exceeds guide range")
                    })? - guide.position;
                }
                _ => {}
            }
        }
        edit.commit(self)
    }

    /// Rotates all document content and metadata by 90 degrees.
    ///
    /// Width/height and DPI axes are exchanged. Success is one undoable edit;
    /// processing failure leaves the original document intact.
    pub fn rotate_document(
        &mut self,
        direction: RotateDirection,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::RotateDocument { direction })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        for plane in after
            .layers
            .iter_mut()
            .flat_map(|layer| layer.planes.iter_mut())
        {
            plane.raster = rotate_raster(&plane.raster, direction, revision.get())?;
        }
        after.selection = rotate_raster(&after.selection, direction, revision.get())?;
        for saved_selection in &mut after.saved_selection_masks {
            saved_selection.raster =
                rotate_raster(&saved_selection.raster, direction, revision.get())?;
        }
        after.fill_protection = rotate_raster(&after.fill_protection, direction, revision.get())?;
        let before_size = DocumentSizeU32::new(before.width, before.height);
        rotate_frame_metadata(&mut after.frames, before_size, direction)?;
        if let Some(frame) = &mut after.shooting_frame {
            crate::shooting_frame::rotate_shooting_frame(frame, before_size, direction)?;
        }
        rotate_guides(&mut after.guides, before_size, direction)?;
        let old_grid = after.grid;
        after.grid.origin_x = match direction {
            RotateDirection::Left90 => old_grid.origin_y,
            RotateDirection::Right90 => {
                i32::try_from(before_size.height)
                    .map_err(|_| CoreError::InvalidState("document height exceeds grid range"))?
                    - old_grid.origin_y
            }
        };
        after.grid.origin_y = match direction {
            RotateDirection::Left90 => {
                i32::try_from(before_size.width)
                    .map_err(|_| CoreError::InvalidState("document width exceeds grid range"))?
                    - old_grid.origin_x
            }
            RotateDirection::Right90 => old_grid.origin_x,
        };
        after.grid.spacing_x = old_grid.spacing_y;
        after.grid.spacing_y = old_grid.spacing_x;
        after.width = before_size.height;
        after.height = before_size.width;
        after.dpi_x_milli = before.dpi_y_milli;
        after.dpi_y_milli = before.dpi_x_milli;
        edit.commit(self)
    }

    /// Resizes the document canvas and optionally resamples content.
    ///
    /// Dimensions, DPI, allocation work, coordinates, and derived metadata are
    /// validated before atomic commit. Identical settings are a no-op; success is
    /// one undoable edit and failure never publishes partial planes.
    pub fn resize_document(
        &mut self,
        resize: DocumentResize,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ResizeDocument { resize })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        bounded_document_pixels(resize.width, resize.height)?;
        if resize.width == 0
            || resize.height == 0
            || resize.dpi_x_milli == 0
            || resize.dpi_y_milli == 0
        {
            return Err(CoreError::InvalidArgument(
                "document dimensions and DPI must be nonzero",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if document.width == resize.width
            && document.height == resize.height
            && document.dpi_x_milli == resize.dpi_x_milli
            && document.dpi_y_milli == resize.dpi_y_milli
        {
            return Ok(self.noop_outcome());
        }
        let before_size = DocumentSizeU32::new(document.width, document.height);
        let after_size = DocumentSizeU32::new(resize.width, resize.height);
        if resize.resample {
            crate::shooting_frame::validate_resample_shooting_frame(
                document.shooting_frame,
                before_size,
                after_size,
            )?;
        }
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (_, after) = edit.documents();
        if resize.resample {
            if let Some(frame) = &mut after.shooting_frame {
                crate::shooting_frame::resample_shooting_frame(frame, before_size, after_size)?;
            }
            for plane in after
                .layers
                .iter_mut()
                .flat_map(|layer| layer.planes.iter_mut())
            {
                plane.raster = resample_raster_nearest(
                    &plane.raster,
                    resize.width,
                    resize.height,
                    revision.get(),
                )?;
            }
            after.selection = resample_raster_nearest(
                &after.selection,
                resize.width,
                resize.height,
                revision.get(),
            )?;
            for saved_selection in &mut after.saved_selection_masks {
                saved_selection.raster = resample_raster_nearest(
                    &saved_selection.raster,
                    resize.width,
                    resize.height,
                    revision.get(),
                )?;
            }
            after.fill_protection = resample_raster_nearest(
                &after.fill_protection,
                resize.width,
                resize.height,
                revision.get(),
            )?;
            let scale = DocumentScaleF64::between(before_size, after_size);
            scale_frame_metadata(&mut after.frames, scale)?;
            for guide in &mut after.guides {
                guide.position = checked_scaled_i32(
                    guide.position,
                    if guide.axis == GuideAxis::Vertical {
                        scale.x
                    } else {
                        scale.y
                    },
                )?;
            }
            after.grid.origin_x = checked_scaled_i32(after.grid.origin_x, scale.x)?;
            after.grid.origin_y = checked_scaled_i32(after.grid.origin_y, scale.y)?;
            after.grid.spacing_x = checked_scaled_spacing(after.grid.spacing_x, scale.x)?;
            after.grid.spacing_y = checked_scaled_spacing(after.grid.spacing_y, scale.y)?;
        } else {
            let offset = resize_anchor_offset(before_size, after_size, resize.anchor)?;
            if let Some(frame) = &mut after.shooting_frame {
                crate::shooting_frame::translate_shooting_frame(frame, offset)?;
            }
            for plane in after
                .layers
                .iter_mut()
                .flat_map(|layer| layer.planes.iter_mut())
            {
                plane.raster = place_raster(&plane.raster, after_size, offset, revision.get())?;
            }
            after.selection = place_raster(&after.selection, after_size, offset, revision.get())?;
            for saved_selection in &mut after.saved_selection_masks {
                saved_selection.raster =
                    place_raster(&saved_selection.raster, after_size, offset, revision.get())?;
            }
            after.fill_protection =
                place_raster(&after.fill_protection, after_size, offset, revision.get())?;
            translate_frame_metadata(&mut after.frames, offset)?;
            for guide in &mut after.guides {
                guide.position = guide
                    .position
                    .checked_add(if guide.axis == GuideAxis::Vertical {
                        offset.x
                    } else {
                        offset.y
                    })
                    .ok_or(CoreError::InvalidArgument("translated guide overflowed"))?;
            }
            after.grid.origin_x =
                after
                    .grid
                    .origin_x
                    .checked_add(offset.x)
                    .ok_or(CoreError::InvalidArgument(
                        "translated grid origin overflowed",
                    ))?;
            after.grid.origin_y =
                after
                    .grid
                    .origin_y
                    .checked_add(offset.y)
                    .ok_or(CoreError::InvalidArgument(
                        "translated grid origin overflowed",
                    ))?;
        }
        after.guides.retain(|guide| {
            let limit = if guide.axis == GuideAxis::Vertical {
                resize.width
            } else {
                resize.height
            };
            guide.position >= 0
                && u32::try_from(guide.position).is_ok_and(|position| position <= limit)
        });
        clamp_margins(&mut after.frames.margins, resize.width, resize.height);
        after.width = resize.width;
        after.height = resize.height;
        after.dpi_x_milli = resize.dpi_x_milli;
        after.dpi_y_milli = resize.dpi_y_milli;
        edit.commit(self)
    }
}
