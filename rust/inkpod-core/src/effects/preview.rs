use super::helpers::*;
use super::*;
use crate::primitive::{CanonicalInvocation, InvocationResult};

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
            base_revision,
            base_document,
            preview_document,
            procedure: PreviewProcedure::Filter(filter),
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
        let (active_plane_id, active_base_revision, base_document) = self
            .filter_preview
            .as_ref()
            .filter(|preview| !matches!(&preview.procedure, PreviewProcedure::Geometry(_)))
            .map(|preview| {
                (
                    preview.plane_id,
                    preview.base_revision,
                    preview.base_document.clone(),
                )
            })
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
        if self.document_revision != base_revision || base_revision != active_base_revision {
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
            base_revision,
            base_document,
            preview_document,
            procedure: PreviewProcedure::Filter(filter),
            preview_revision,
        });
        self.render_cache.clear();
        Ok(info)
    }

    /// Discards the active preview and reports the restored base checksum.
    ///
    /// Document revision, history, dirty state, and savepoint are unchanged.
    pub fn cancel_filter_preview(&mut self) -> Result<FilterPreviewInfo, CoreError> {
        if self
            .filter_preview
            .as_ref()
            .is_some_and(|preview| matches!(&preview.procedure, PreviewProcedure::Geometry(_)))
        {
            return Err(CoreError::InvalidState("there is no active filter preview"));
        }
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
        if !self.canonical_invocation_is_active() {
            let invocation = match &preview.procedure {
                PreviewProcedure::LineCorrection(request) => {
                    CanonicalInvocation::ApplyLineCorrection {
                        request: request.clone(),
                    }
                }
                PreviewProcedure::Filter(filter) => CanonicalInvocation::ApplyFilter {
                    plane_id: preview.plane_id.get(),
                    filter: filter.clone(),
                },
                PreviewProcedure::Dust { shape, options } => {
                    CanonicalInvocation::ApplyDustRemoval {
                        plane_id: preview.plane_id.get(),
                        shape: shape.clone(),
                        options: *options,
                    }
                }
                PreviewProcedure::Geometry(_) => {
                    return Err(CoreError::InvalidState("there is no active filter preview"));
                }
            };
            return self
                .execute_canonical_invocation_with(invocation, |staged| {
                    staged
                        .apply_filter_preview()
                        .map(InvocationResult::dispatch)
                })
                .map(|result| result.dispatch);
        }
        let result = self
            .commit_deferred_document_edit_current(preview.base_document, preview.preview_document);
        if result.is_ok() {
            self.filter_preview = None;
            if let PreviewProcedure::Filter(filter) = preview.procedure {
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
        if !self.canonical_invocation_is_active() {
            let filter = self
                .last_filter
                .clone()
                .ok_or(CoreError::InvalidState("there is no last filter"))?;
            return self
                .execute_canonical_invocation_with(
                    CanonicalInvocation::ApplyFilter { plane_id, filter },
                    move |staged| {
                        staged
                            .apply_last_filter_internal(plane_id, &mut progress)
                            .map(InvocationResult::dispatch)
                    },
                )
                .map(|result| result.dispatch);
        }
        self.apply_last_filter_internal(plane_id, &mut progress)
    }

    fn apply_last_filter_internal(
        &mut self,
        plane_id: u64,
        progress: &mut dyn FnMut(u64, u64) -> bool,
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
            progress,
        )?;
        if self.document_revision != base_revision {
            return Err(CoreError::InvalidState(
                "last-filter base revision became stale",
            ));
        }
        self.commit_deferred_document_edit(before, after, base_revision, revision)
    }
}
