use super::helpers::*;
use super::*;

impl Core {
    /// Starts an isolated filter preview for an editable plane.
    ///
    /// Preview pixels use a transient revision and do not affect history, dirty
    /// state, or savepoint until [`Core::apply_filter_preview`].
    pub fn begin_filter_preview(
        &mut self,
        plane_id: u64,
        filter: Filter,
    ) -> Result<FilterPreviewInfo, CoreError> {
        self.begin_filter_preview_with_progress(plane_id, filter, |_, _| true)
    }

    /// Starts a filter preview with cooperative progress/cancellation.
    ///
    /// Returning `false` cancels before preview publication. Failure, cancellation,
    /// or stale base revision leaves the previous live document unchanged.
    pub fn begin_filter_preview_with_progress(
        &mut self,
        plane_id: u64,
        filter: Filter,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<FilterPreviewInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let plane_id = PlaneId::from_raw(plane_id);
        let base_revision = self.document_revision;
        let base_document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let preview_document = filter_document_with_progress(
            &base_document,
            plane_id,
            &filter,
            RenderRevision::from_raw(preview_revision.get()),
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

    /// Recomputes the active preview from its original base using a new filter.
    pub fn update_filter_preview(
        &mut self,
        plane_id: u64,
        filter: Filter,
    ) -> Result<FilterPreviewInfo, CoreError> {
        self.update_filter_preview_with_progress(plane_id, filter, |_, _| true)
    }

    /// Recomputes an active preview with cooperative progress/cancellation.
    ///
    /// A failed update retains the previously published preview and never commits
    /// partial output to the live document.
    pub fn update_filter_preview_with_progress(
        &mut self,
        plane_id: u64,
        filter: Filter,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<FilterPreviewInfo, CoreError> {
        let plane_id = PlaneId::from_raw(plane_id);
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
            RenderRevision::from_raw(preview_revision.get()),
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

    /// Discards the active preview and reports the restored base checksum.
    ///
    /// Document revision, history, dirty state, and savepoint are unchanged.
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
            plane_id: preview.plane_id.get(),
            base_checksum: checksum,
            preview_checksum: checksum,
            preview_revision: self.document_revision.get(),
        })
    }

    /// Commits the active preview as one undoable document edit.
    ///
    /// Stale revision or validation failure does not publish preview content and
    /// leaves the preview available for cancellation or retry.
    pub fn apply_filter_preview(&mut self) -> Result<DispatchOutcome, CoreError> {
        let preview = self
            .filter_preview
            .as_ref()
            .cloned()
            .ok_or(CoreError::InvalidState("there is no active filter preview"))?;
        let result = self
            .commit_deferred_document_edit_current(preview.base_document, preview.preview_document);
        if result.is_ok() {
            self.filter_preview = None;
            if let Some(filter) = preview.filter {
                self.last_filter = Some(filter);
            }
        }
        result
    }

    /// Reapplies the most recently committed filter to `plane_id`.
    pub fn apply_last_filter(&mut self, plane_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.apply_last_filter_with_progress(plane_id, |_, _| true)
    }

    /// Reapplies the last filter with cooperative progress/cancellation.
    ///
    /// Cancellation, failure, and stale revision are atomic; success is one undo unit.
    pub fn apply_last_filter_with_progress(
        &mut self,
        plane_id: u64,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let plane_id = PlaneId::from_raw(plane_id);
        let base_revision = self.document_revision;
        let filter = self
            .last_filter
            .clone()
            .ok_or(CoreError::InvalidState("there is no last filter"))?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let after = filter_document_with_progress(
            &before,
            plane_id,
            &filter,
            RenderRevision::from_raw(revision.get()),
            &mut progress,
        )?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "last-filter base revision became stale",
            ));
        }
        self.commit_deferred_document_edit(before, after, base_revision, revision)
    }

    /// Creates an adjustment layer with validated parameters.
    ///
    /// Success is one undoable edit and returns a new stable layer ID.
    pub fn create_adjustment_layer(
        &mut self,
        name: &str,
        adjustment: Adjustment,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        inkpod_image::apply_adjustment(super::PixelValue::Rgba([0; 4]), &adjustment)?;
        if self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .layers
            .len()
            >= super::MAX_LAYERS
        {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        let mut next_id = self.next_id;
        let layer_id = next_id.take_layer();
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        after.layers.insert(
            0,
            LayerNode {
                id: layer_id,
                kind: LayerKind::Adjustment,
                name: unique_layer_name(&after.layers, name),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                planes: Vec::new(),
            },
        );
        after.adjustments.insert(layer_id, adjustment);
        after.active_layer_id = layer_id;
        let outcome = edit.commit(self)?;
        self.next_id = next_id;
        Ok((outcome, layer_id.get()))
    }

    /// Replaces an adjustment layer's parameters as one undoable edit.
    ///
    /// Invalid parameters or a non-adjustment layer fail atomically.
    pub fn update_adjustment_layer(
        &mut self,
        layer_id: u64,
        adjustment: Adjustment,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let layer_id = LayerId::from_raw(layer_id);
        inkpod_image::apply_adjustment(super::PixelValue::Rgba([0; 4]), &adjustment)?;
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
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
        edit.commit(self)
    }

    /// Borrows adjustment parameters for the lifetime of the Core borrow.
    ///
    /// This query does not affect revisions, history, or dirty state.
    pub fn adjustment(&self, layer_id: u64) -> Result<&Adjustment, CoreError> {
        let layer_id = LayerId::from_raw(layer_id);
        self.document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .adjustments
            .get(&layer_id)
            .ok_or(CoreError::InvalidArgument(
                "adjustment layer ID does not exist",
            ))
    }
}
