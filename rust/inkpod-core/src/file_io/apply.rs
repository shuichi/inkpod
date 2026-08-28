use super::job::{FileIoJob, Prepared};
use super::model::*;
use crate::{Core, CoreError, SubpaletteCatalog, SubpaletteCatalogInfo};

impl FileIoJob {
    /// Publishes a prepared document result on its original owner. Stale, failed
    /// or cancelled candidates never replace live state. A normal save returns
    /// Pending after authorization and requires one final apply after install.
    pub fn apply(&mut self, core: &mut Core) -> Result<FileIoApply, CoreError> {
        self.poll();
        if self.progress.state != FileIoState::Ready {
            // A premature/repeated apply is a caller error, not a failure of
            // accepted asynchronous work or an already completed result.
            return Err(self
                .error
                .clone()
                .unwrap_or(CoreError::InvalidState("file job is not ready")));
        }
        let result = self.apply_ready(core);
        if let Err(error) = &result {
            if !self.progress.installing {
                self.fail(error.clone());
            }
        }
        result
    }

    fn apply_ready(&mut self, core: &mut Core) -> Result<FileIoApply, CoreError> {
        if self.progress.state != FileIoState::Ready {
            return Err(self
                .error
                .clone()
                .unwrap_or(CoreError::InvalidState("file job is not ready")));
        }
        if self.progress.installing {
            if self.request.kind == FileIoKind::SequenceSwitch {
                let prepared = self
                    .sequence_install
                    .as_ref()
                    .ok_or(CoreError::InvalidState(
                        "sequence installation result is missing",
                    ))?;
                core.validate_prepared_sequence_switch(prepared)?;
                if let Some(error) = self.error.take() {
                    core.io_install_pending = false;
                    self.progress.installing = false;
                    self.sequence_install = None;
                    return Err(error);
                }
                let prepared = self.sequence_install.take().ok_or(CoreError::InvalidState(
                    "sequence installation result is missing",
                ))?;
                let document = core.commit_prepared_sequence_switch(*prepared)?;
                self.progress.installing = false;
                self.progress.state = FileIoState::Complete;
                return Ok(FileIoApply::Complete {
                    document: Some(Box::new(document)),
                    object_id: 0,
                });
            }
            let token = self
                .save_token
                .as_ref()
                .ok_or(CoreError::InvalidState("save token is missing"))?;
            // This validates lifetime as well as revisions. Never un-fence a different Core.
            core.validate_document_save(token)?;
            if let Some(error) = self.error.take() {
                core.io_install_pending = false;
                self.progress.installing = false;
                self.save_token = None;
                return Err(error);
            }
            if self.request.kind == FileIoKind::CompactedCopy {
                core.io_install_pending = false;
                self.progress.installing = false;
                self.save_token = None;
                self.progress.state = FileIoState::Complete;
                return Ok(FileIoApply::Complete {
                    document: core.document_info().ok().map(Box::new),
                    object_id: 0,
                });
            }
            let token = self
                .save_token
                .take()
                .ok_or(CoreError::InvalidState("save token is missing"))?;
            let document = core.commit_document_save(token, &self.request.paths[0])?;
            if let Some(saved) = self.installed.take() {
                let format = core.raster_file_format()?;
                let uuid = document.document_uuid;
                let raster_path = saved
                    .native_path
                    .with_extension(super::prepare::format_extension(format));
                self.items = vec![FileIoItem {
                    path: saved.native_path.clone(),
                    name: saved
                        .native_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("")
                        .to_owned(),
                    format: None,
                    identity: saved.native.identity,
                    identity_physical: true,
                    source_generation: 1,
                    document_uuid: uuid,
                }];
                if let Some(raster) = saved.raster {
                    self.items.push(FileIoItem {
                        name: raster_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("")
                            .to_owned(),
                        path: raster_path,
                        format: Some(format),
                        identity: raster.identity,
                        identity_physical: true,
                        source_generation: 1,
                        document_uuid: uuid,
                    });
                }
                core.io_pair_authority = Some(saved);
            }
            self.progress.installing = false;
            self.progress.state = FileIoState::Complete;
            return Ok(FileIoApply::Complete {
                document: Some(Box::new(document)),
                object_id: 0,
            });
        }
        if matches!(self.ready, Some(Prepared::References(_))) {
            return Err(CoreError::InvalidArgument(
                "reference result requires a reference catalog",
            ));
        }
        let sequence_only = matches!(self.ready, Some(Prepared::Sequence(_)));
        if !matches!(self.ready, Some(Prepared::Output)) {
            self.target
                .as_ref()
                .ok_or(CoreError::NoDocument)?
                .validate(core, sequence_only)?;
        }
        let prepared = self
            .ready
            .take()
            .ok_or(CoreError::InvalidState("prepared file result is missing"))?;
        let mut object_id = 0;
        match prepared {
            Prepared::Open(staged, _) => {
                let path = if self.request.kind == FileIoKind::OpenNative {
                    Some(self.request.paths[0].as_path())
                } else {
                    None
                };
                let token = self
                    .open_token
                    .take()
                    .ok_or(CoreError::InvalidState("open token is missing"))?;
                core.adopt_opened_document(token, *staged, path)?;
            }
            Prepared::Sequence(sources) => core.set_sequence(sources)?,
            Prepared::LightTable(input) => {
                if self.request.kind == FileIoKind::LightTableReload {
                    core.light_table_update_item(self.request.object_id, input)?;
                    object_id = self.request.object_id;
                } else {
                    object_id = core.light_table_add_item(input)?.1;
                }
            }
            Prepared::Pair(pair, token) => {
                core.validate_document_save(&token)?;
                // Submit can fail without installing anything. Fence before the
                // caller may dispatch another edit on this single-writer Core.
                self.install(*pair, token)?;
                core.io_install_pending = true;
                return Ok(FileIoApply::Pending);
            }
            Prepared::NativeOutput(file, token) => {
                core.validate_document_save(&token)?;
                self.install_native(file, token)?;
                core.io_install_pending = true;
                return Ok(FileIoApply::Pending);
            }
            Prepared::SequenceSwitch(prepared) => {
                core.validate_prepared_sequence_switch(&prepared)?;
                if self.install_sequence(prepared)? {
                    core.io_install_pending = true;
                    return Ok(FileIoApply::Pending);
                }
                let prepared = self.sequence_install.take().ok_or(CoreError::InvalidState(
                    "prepared sequence target is missing",
                ))?;
                core.commit_prepared_sequence_switch(*prepared)?;
            }
            Prepared::Output => {}
            Prepared::Batch(result) => {
                if let Some(mut staged) = result.active {
                    // Batch has already used the canonical executor on its COW
                    // candidate. Publish that one transaction while retaining
                    // independently changed view/session display state.
                    staged.view = core.view;
                    staged.secondary_views = core.secondary_views.clone();
                    staged.next_view_id = core.next_view_id;
                    staged.next_render_tile_revision = core.next_render_tile_revision;
                    staged.next_preview_revision = core.next_preview_revision;
                    staged.render_cache.clear();
                    staged.color_check = core.color_check;
                    staged.sequence = core.sequence.clone();
                    staged.motion_check = core.motion_check.clone();
                    staged.subpalette_index = core.subpalette_index;
                    staged.editor_defaults = core.editor_defaults.clone();
                    staged.shortcuts = core.shortcuts.clone();
                    staged.shortcut_defaults = core.shortcut_defaults.clone();
                    staged.new_cell_raster_format = core.new_cell_raster_format;
                    staged.io_manager = core.io_manager.clone();
                    staged.io_pair_authority = core.io_pair_authority.clone();
                    staged.persistence_state = core.persistence_state.clone();
                    staged.io_install_pending = core.io_install_pending;
                    *core = *staged;
                }
                self.batch_report = result.report;
                self.batch_preview = result.preview;
            }
            Prepared::References(_) => unreachable!("checked before taking reference result"),
            Prepared::Recovery(_) => {
                unreachable!("independent recovery result completes during poll")
            }
            Prepared::CutDescriptor => unreachable!("Cut probe completes without Cell adoption"),
        }
        self.progress.state = FileIoState::Complete;
        Ok(FileIoApply::Complete {
            document: core.document_info().ok().map(Box::new),
            object_id,
        })
    }

    /// Atomically replaces a read-only catalog with its fully decoded candidate.
    /// The frontend must also validate its captured pane ID/generation before
    /// calling this owner-thread operation. A failure retains the old catalog.
    pub fn apply_reference(
        &mut self,
        catalog: &mut SubpaletteCatalog,
    ) -> Result<SubpaletteCatalogInfo, CoreError> {
        self.poll();
        if self.progress.state != FileIoState::Ready {
            return Err(self
                .error
                .clone()
                .unwrap_or(CoreError::InvalidState("reference job is not ready")));
        }
        if !matches!(self.ready, Some(Prepared::References(_))) {
            return Err(CoreError::InvalidArgument("job has no reference result"));
        }
        let Some(Prepared::References(images)) = self.ready.take() else {
            unreachable!()
        };
        match catalog.replace_loaded_images(images) {
            Ok(info) => {
                self.progress.state = FileIoState::Complete;
                Ok(info)
            }
            Err(error) => {
                self.fail(error.clone());
                Err(error)
            }
        }
    }
}
