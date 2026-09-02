use super::job::{FileIoJob, PairAuthorityRepair, Prepared};
use super::model::*;
use crate::{CommonRasterFormat, Core, CoreError, SubpaletteCatalog, SubpaletteCatalogInfo};

fn validate_installed_pair_items(
    items: &[FileIoItem],
    saved: &SavedPair,
    format: CommonRasterFormat,
    document_uuid: u128,
) -> Result<(), CoreError> {
    let raster = saved.raster.ok_or(CoreError::InvalidState(
        "installed save pair raster stamp is missing",
    ))?;
    let [native_item, raster_item] = items else {
        return Err(CoreError::InvalidState(
            "installed save pair result shape is invalid",
        ));
    };
    let name_matches = |item: &FileIoItem| {
        item.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            == item.name
    };
    let shared_valid = |item: &FileIoItem| {
        item.identity_physical
            && item.source_generation == 1
            && item.document_uuid == document_uuid
            && name_matches(item)
    };
    if !shared_valid(native_item)
        || native_item.path != saved.native_path
        || native_item.format.is_some()
        || native_item.identity != saved.native.identity
        || !shared_valid(raster_item)
        || raster_item.path != saved.raster_path
        || raster_item.format != Some(format)
        || raster_item.identity != raster.identity
    {
        return Err(CoreError::InvalidState(
            "installed save pair result does not match its durable stamps",
        ));
    }
    Ok(())
}

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
                if self.error.is_some() {
                    // Failure/cancellation publishes no target. Validate only
                    // the originating Core lifetime before clearing its fence:
                    // a stale document/editor/sequence stamp must not strand
                    // an already-finished installation in READY forever.
                    prepared.validate_owner(core)?;
                    if !core.io_install_pending {
                        return Err(CoreError::InvalidState(
                            "document install fence is not active",
                        ));
                    }
                    let Some(error) = self.error.take() else {
                        return Err(CoreError::InvalidState(
                            "sequence installation error disappeared",
                        ));
                    };
                    core.io_install_pending = false;
                    self.progress.installing = false;
                    self.sequence_install = None;
                    return Err(error);
                }
                core.validate_prepared_sequence_switch(prepared)?;
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
            if self.error.is_some() {
                // A failed/rolled-back worker result publishes no document or
                // savepoint. Finalize it on the originating Core lifetime even
                // if its full save stamp became stale; otherwise the install
                // fence and restored runtime authority could be stranded. A
                // different Core can never consume this failure result.
                token.validate_owner(core)?;
                if !core.io_install_pending {
                    return Err(CoreError::InvalidState(
                        "document install fence is not active",
                    ));
                }
                let Some(error) = self.error.take() else {
                    return Err(CoreError::InvalidState(
                        "worker installation error disappeared",
                    ));
                };
                if self.request.kind == FileIoKind::SavePair && self.pair_publication_started {
                    match self.pair_authority_repair.take() {
                        Some(PairAuthorityRepair::Committed(saved)) => {
                            core.io_pair_authority = Some(saved);
                            core.io_pair_plan = None;
                        }
                        Some(PairAuthorityRepair::Planned(planned)) => {
                            core.io_pair_authority = None;
                            core.io_pair_plan = Some(planned);
                        }
                        None if self.pair_repair_target.affects_current_authority() => {
                            core.io_pair_authority = None;
                            core.io_pair_plan = None;
                            core.current_path = None;
                            core.revoke_sequence_preservation_baseline();
                            // The failed publication invalidated the only
                            // normal-save authority. Preserve document/history
                            // content but clear both savepoints so close and
                            // sequence navigation cannot discard the exact state
                            // before a successful Save As establishes a new pair.
                            core.savepoint = None;
                            if let Some(editor) = core.editor_session.as_mut() {
                                editor.savepoint = None;
                            }
                            self.progress.authority_revoked = true;
                        }
                        None => {}
                    }
                }
                core.io_install_pending = false;
                self.progress.installing = false;
                self.save_token = None;
                return Err(error);
            }
            // Successful disk publication still requires the complete lifetime,
            // revision, state, editor, savepoint, path, and format fence.
            core.validate_document_save(token)?;
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
            let document_before = core.document_info()?;
            let format = core.raster_file_format()?;
            let saved = self.installed.as_ref().ok_or(CoreError::InvalidState(
                "installed save pair result is missing",
            ))?;
            if self.request.paths.first() != Some(&saved.native_path) {
                return Err(CoreError::InvalidState(
                    "installed save pair destination is inconsistent",
                ));
            }
            validate_installed_pair_items(
                &self.items,
                saved,
                format,
                document_before.document_uuid,
            )?;
            // Allocate the ABI-facing owner before the Core save commit. From
            // this point through return, publication only moves owned values or
            // writes fixed-width fields and cannot report a new local failure.
            let mut document_output = Box::new(document_before);
            let saved = self.installed.take().ok_or(CoreError::InvalidState(
                "installed save pair result is missing",
            ))?;
            let token = self
                .save_token
                .take()
                .ok_or(CoreError::InvalidState("save token is missing"))?;
            // `commit_document_save` performs every fallible query/allocation
            // before its first live mutation, so success is the Core commit
            // point and no error can be returned after it.
            let document = core.commit_document_save(token, &self.request.paths[0])?;
            *document_output = document;
            core.io_pair_authority = Some(saved);
            core.io_pair_plan = None;
            self.progress.installing = false;
            self.progress.state = FileIoState::Complete;
            return Ok(FileIoApply::Complete {
                document: Some(document_output),
                object_id: 0,
            });
        }
        if matches!(self.ready, Some(Prepared::References(_))) {
            return Err(CoreError::InvalidArgument(
                "reference result requires a reference catalog",
            ));
        }
        let sequence_only = matches!(self.ready, Some(Prepared::Sequence { .. }));
        if !matches!(self.ready, Some(Prepared::Output(_))) {
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
            Prepared::Open(staged, _, normal_path) => {
                let token = self
                    .open_token
                    .take()
                    .ok_or(CoreError::InvalidState("open token is missing"))?;
                if self.request.revert_current {
                    core.adopt_reloaded_document(token, *staged, normal_path.as_deref())?;
                } else {
                    core.adopt_opened_document(token, *staged, normal_path.as_deref())?;
                }
            }
            Prepared::Sequence { sources, residents } => {
                core.set_sequence_with_residents(sources, residents)?
            }
            Prepared::LightTable(input) => {
                if self.request.kind == FileIoKind::LightTableReload {
                    core.light_table_update_item(self.request.object_id, input)?;
                    object_id = self.request.object_id;
                } else {
                    object_id = core.light_table_add_item(input)?.1;
                }
            }
            Prepared::Pair(pair, token, repair_target) => {
                core.validate_document_save(&token)?;
                // Submit can fail without installing anything. Fence before the
                // caller may dispatch another edit on this single-writer Core.
                self.install(*pair, token, repair_target)?;
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
            Prepared::Output(_) => {}
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
                    staged.io_pair_plan = core.io_pair_plan.clone();
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
