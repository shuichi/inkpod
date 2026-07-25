use super::helpers::*;
use super::*;

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
        validate_node_name(name)?;
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
                name: unique_layer_name(&after.layers, name),
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
}
