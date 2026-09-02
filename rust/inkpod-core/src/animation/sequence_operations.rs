use super::raster::*;
use super::sequence::*;
use super::*;

impl Core {
    /// Installs validated sequence sources in deterministic natural-name order.
    ///
    /// This changes sequence-only state, clears motion/subpalette state, and does
    /// not change the active document, revision, history, dirty state, or savepoint.
    pub fn set_sequence(&mut self, mut cells: Vec<SequenceCellSource>) -> Result<(), CoreError> {
        if self.io_install_pending {
            return Err(CoreError::InvalidState("file installation is pending"));
        }
        if cells.is_empty() || cells.len() > MAX_SEQUENCE_CELLS {
            return Err(CoreError::InvalidArgument(
                "sequence cell count is outside bounds",
            ));
        }
        for cell in &cells {
            validate_sequence_cell(cell)?;
        }
        cells.sort_by(|left, right| natural_cmp(&left.name, &right.name));
        if cells
            .windows(2)
            .any(|pair| pair[0].name.eq_ignore_ascii_case(&pair[1].name))
        {
            return Err(CoreError::InvalidArgument(
                "sequence contains duplicate names",
            ));
        }
        for (index, cell) in cells.iter().enumerate() {
            if cells[..index].iter().any(|previous| {
                previous.document_uuid == cell.document_uuid
                    && previous.source_generation == cell.source_generation
            }) {
                return Err(CoreError::InvalidArgument(
                    "sequence contains a duplicate source identity",
                ));
            }
        }
        let current_uuid = self
            .document
            .as_ref()
            .map(|document| document.uuid)
            .unwrap_or(0);
        let active_index = cells
            .iter()
            .position(|cell| cell.document_uuid == current_uuid);
        let revision = self
            .sequence
            .as_ref()
            .map_or(Some(1), |sequence| sequence.revision.checked_add(1))
            .ok_or(CoreError::InvalidState("sequence revision overflows"))?;
        self.sequence = Some(SequenceState {
            cells,
            active_index,
            revision,
        });
        self.sequence_render_catalog_changed();
        self.motion_check = None;
        self.subpalette_index = None;
        Ok(())
    }

    /// Decodes named common-raster files and atomically installs them as a sequence.
    ///
    /// Any decode or validation failure retains the previous sequence state.
    pub fn import_sequence(
        &mut self,
        format: CommonRasterFormat,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<(), CoreError> {
        self.import_mixed_sequence(
            files
                .into_iter()
                .map(|(name, bytes)| (name, format, bytes))
                .collect(),
        )
    }

    /// Decodes named common-raster files with per-file formats and atomically
    /// installs them as a sequence.
    ///
    /// Any decode or validation failure retains the previous sequence state.
    pub fn import_mixed_sequence(
        &mut self,
        files: Vec<(String, CommonRasterFormat, Vec<u8>)>,
    ) -> Result<(), CoreError> {
        if files.is_empty() || files.len() > MAX_SEQUENCE_CELLS {
            return Err(CoreError::InvalidArgument(
                "sequence import count is outside bounds",
            ));
        }
        let mut cells = Vec::with_capacity(files.len());
        for (index, (name, format, bytes)) in files.into_iter().enumerate() {
            let raster = decode_common_raster(format, &bytes)?;
            let uuid = (u128::from(0x494e_4b50_4f44_5334_u64) << 64)
                | u128::try_from(index + 1)
                    .map_err(|_| CoreError::InvalidState("sequence UUID index overflows"))?;
            let mut source = SequenceCellSource::from_common_raster(name, uuid, &raster)?;
            source.raster_file_format = format;
            cells.push(source);
        }
        self.set_sequence(cells)
    }

    fn sequence_source_at(&self, index: usize) -> Result<&SequenceCellSource, CoreError> {
        self.sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?
            .cells
            .get(index)
            .ok_or(CoreError::InvalidArgument(
                "sequence target index is outside bounds",
            ))
    }

    /// Returns catalog invalidation metadata without allocating or reading pixels.
    /// Missing catalogs return zero count/revision/owner and no active index.
    /// This query has no document, editor, history, revision, or savepoint effects.
    #[must_use]
    pub fn sequence_catalog_info(&self) -> SequenceCatalogInfo {
        self.sequence.as_ref().map_or(
            SequenceCatalogInfo {
                revision: 0,
                owner_generation: 0,
                cell_count: 0,
                active_index: None,
            },
            |sequence| SequenceCatalogInfo {
                revision: sequence.revision,
                owner_generation: self.sequence_render_cache.owner_generation(),
                cell_count: sequence.cells.len() as u32,
                active_index: sequence.active_index.map(|index| index as u32),
            },
        )
    }

    /// Borrows metadata for one natural-order cell without inspecting pixel payloads.
    ///
    /// An absent catalog or out-of-range index is an error. This query does not
    /// generate a thumbnail, allocate, or change revision/history/savepoints.
    pub fn sequence_cell_metadata(
        &self,
        index: usize,
    ) -> Result<SequenceCellMetadata<'_>, CoreError> {
        let cell = self.sequence_source_at(index)?;
        Ok(SequenceCellMetadata {
            name: &cell.name,
            cell_number: cell.cell_number,
            document_uuid: cell.document_uuid,
            source_generation: cell.source_generation,
            width: cell.raster.width(),
            height: cell.raster.height(),
            thumbnail_width: cell.thumbnail.width,
            thumbnail_height: cell.thumbnail.height,
            thumbnail_checksum: cell.thumbnail.checksum,
        })
    }

    /// Borrows the cached preview for one natural-order cell without resampling.
    ///
    /// The returned bytes are immutable and valid for the lifetime of the catalog
    /// borrow. Missing catalogs and out-of-range indices are errors; this query
    /// changes no document, editor, history, revision, or savepoint state.
    pub fn sequence_thumbnail(&self, index: usize) -> Result<&Thumbnail, CoreError> {
        Ok(self.sequence_source_at(index)?.thumbnail.as_ref())
    }

    /// Returns owned metadata and a copy of the cached thumbnail for one cell.
    pub fn sequence_cell(&self, index: usize) -> Result<SequenceCellInfo, CoreError> {
        let cell = self.sequence_source_at(index)?;
        Ok(SequenceCellInfo {
            name: cell.name.clone(),
            cell_number: cell.cell_number,
            document_uuid: cell.document_uuid,
            source_generation: cell.source_generation,
            width: cell.raster.width(),
            height: cell.raster.height(),
            thumbnail: cell.thumbnail()?,
        })
    }

    /// Returns owned metadata and thumbnails in natural sequence order.
    pub fn sequence_cells(&self) -> Result<Vec<SequenceCellInfo>, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        (0..sequence.cells.len())
            .map(|index| self.sequence_cell(index))
            .collect()
    }

    /// Resolves an adjacent sequence target without changing document or editor state.
    ///
    /// No configured sequence returns an explicit `Empty` plan. An existing sequence
    /// requires the active document to match one immutable sequence identity. Empty,
    /// single-cell, stopped, adjacent, and wrapped results are distinct. Resolving a
    /// request never changes document revision, history, journal, dirty state, or
    /// savepoints.
    pub fn resolve_sequence_step(
        &self,
        direction: SequenceDirection,
        endpoint_policy: SequenceEndpointPolicy,
    ) -> Result<SequenceStepPlan, CoreError> {
        self.ensure_no_active_stroke()?;
        let Some(sequence) = self.sequence.as_ref() else {
            return Ok(SequenceStepPlan {
                direction,
                endpoint_policy,
                result: SequenceStepResult::Empty,
                sequence_revision: 0,
                source_index: None,
                target_index: None,
                source_document_uuid: None,
                source_generation: None,
                target_document_uuid: None,
                target_generation: None,
                source_cell_number: None,
                target_cell_number: None,
            });
        };
        let source_index = sequence.active_index.ok_or(CoreError::InvalidState(
            "active document is not bound to a sequence entry",
        ))?;
        let source = sequence
            .cells
            .get(source_index)
            .ok_or(CoreError::InvalidState("sequence active index is invalid"))?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if document.uuid != source.document_uuid {
            return Err(CoreError::InvalidState(
                "active document identity does not match the sequence source",
            ));
        }
        let count = sequence.cells.len();
        let (target_index, result) = if count == 1 {
            (source_index, SequenceStepResult::SingleCell)
        } else {
            match direction {
                SequenceDirection::Previous if source_index == 0 => match endpoint_policy {
                    SequenceEndpointPolicy::Stop => (source_index, SequenceStepResult::Stopped),
                    SequenceEndpointPolicy::Wrap => (count - 1, SequenceStepResult::Wrapped),
                },
                SequenceDirection::Previous => (source_index - 1, SequenceStepResult::Advanced),
                SequenceDirection::Next if source_index + 1 == count => match endpoint_policy {
                    SequenceEndpointPolicy::Stop => (source_index, SequenceStepResult::Stopped),
                    SequenceEndpointPolicy::Wrap => (0, SequenceStepResult::Wrapped),
                },
                SequenceDirection::Next => (source_index + 1, SequenceStepResult::Advanced),
            }
        };
        let target = sequence
            .cells
            .get(target_index)
            .ok_or(CoreError::InvalidState(
                "resolved sequence target is invalid",
            ))?;
        Ok(SequenceStepPlan {
            direction,
            endpoint_policy,
            result,
            sequence_revision: sequence.revision,
            source_index: Some(
                u32::try_from(source_index)
                    .map_err(|_| CoreError::InvalidState("sequence source index overflows"))?,
            ),
            target_index: Some(
                u32::try_from(target_index)
                    .map_err(|_| CoreError::InvalidState("sequence target index overflows"))?,
            ),
            source_document_uuid: Some(source.document_uuid),
            source_generation: Some(source.source_generation),
            target_document_uuid: Some(target.document_uuid),
            target_generation: Some(target.source_generation),
            source_cell_number: Some(source.cell_number),
            target_cell_number: Some(target.cell_number),
        })
    }

    /// Commits one previously resolved normal sequence navigation request.
    ///
    /// Empty, single-cell, and stopped plans are semantic no-ops and return current
    /// document information even if the document is dirty. A plan that changes the
    /// active cell requires a clean document. Any stale identity/revision, unsaved
    /// change, overflow, or activation failure leaves document, editor state, history,
    /// journal, dirty state, savepoint, and sequence state unchanged.
    pub fn commit_sequence_step(
        &mut self,
        plan: SequenceStepPlan,
    ) -> Result<DocumentInfo, CoreError> {
        let current = self.resolve_sequence_step(plan.direction, plan.endpoint_policy)?;
        if current != plan {
            return Err(CoreError::InvalidState("sequence step request is stale"));
        }
        if !plan.requires_switch() {
            return self.document_info();
        }
        if self.savepoint != Some(self.current_state) {
            return Err(CoreError::UnsavedChanges);
        }
        let target = plan
            .target_index
            .ok_or(CoreError::InvalidState("sequence step target is missing"))?;
        self.sequence_activate_impl(target as usize)
    }

    /// Activates the adjacent sequence cell, optionally wrapping at the ends.
    ///
    /// Unsaved document changes and active strokes are rejected. Editor-only dirty
    /// does not block a switch. Endpoint no-op keeps the current document; a switch
    /// installs a clean document, resets history/view, preserves non-target
    /// EditorState, and deterministically reconciles its stable target as the
    /// new cell's clean editor baseline without adopting a normal-save path.
    pub fn sequence_step(
        &mut self,
        direction: SequenceDirection,
        loop_sequence: bool,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document.is_none() {
            return Err(CoreError::NoDocument);
        }
        if self.savepoint != Some(self.current_state) {
            return Err(CoreError::UnsavedChanges);
        }
        let endpoint_policy = if loop_sequence {
            SequenceEndpointPolicy::Wrap
        } else {
            SequenceEndpointPolicy::Stop
        };
        let plan = self.resolve_sequence_step(direction, endpoint_policy)?;
        self.commit_sequence_step(plan)
    }

    /// Captures the exact source, target, policy, and document revision for an
    /// asynchronous sequence switch.
    ///
    /// The configured sequence must already identify the active document. The
    /// request is side-effect free and remains usable only while its source
    /// revision and both sequence identities remain current.
    pub fn sequence_switch_request(
        &self,
        target: usize,
        policy: SequenceSwitchPolicy,
    ) -> Result<SequenceSwitchRequest, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let source_index = sequence.active_index.ok_or(CoreError::InvalidState(
            "active document is not bound to a sequence entry",
        ))?;
        let source = sequence
            .cells
            .get(source_index)
            .ok_or(CoreError::InvalidState("sequence active index is invalid"))?;
        let target_source = sequence
            .cells
            .get(target)
            .ok_or(CoreError::InvalidArgument(
                "sequence target index is outside bounds",
            ))?;
        if source.document_uuid != document.uuid {
            return Err(CoreError::InvalidState(
                "active document identity does not match the sequence source",
            ));
        }
        let requires_switch = source.document_uuid != target_source.document_uuid
            || source.source_generation != target_source.source_generation;
        Ok(SequenceSwitchRequest {
            policy,
            source_document_uuid: source.document_uuid,
            source_generation: source.source_generation,
            source_document_revision: self.document_revision.get(),
            source_editor_revision: self
                .editor_session
                .as_ref()
                .ok_or(CoreError::NoDocument)?
                .revision
                .get(),
            target_document_uuid: target_source.document_uuid,
            target_source_generation: target_source.source_generation,
            target_index: u32::try_from(target)
                .map_err(|_| CoreError::InvalidArgument("sequence target index overflows"))?,
            source_recovery_required: requires_switch && self.sequence_source_recovery_required(),
        })
    }

    /// Commits an autosave-before-switch request after the frontend has durably
    /// written the source recovery artifact and metadata.
    ///
    /// This operation does not itself perform I/O. A stale, invalid, or non-autosave
    /// request publishes nothing. Success activates the immutable flattened target;
    /// the source document remains dirty only in its external recovery association.
    pub fn sequence_commit_autosaved_switch(
        &mut self,
        request: SequenceSwitchRequest,
    ) -> Result<DocumentInfo, CoreError> {
        self.validate_autosaved_sequence_switch_request(request)?;
        if !request.requires_switch() {
            return self.document_info();
        }
        self.sequence_activate_impl(request.target_index as usize)
    }

    /// Restores an exact target cell from a validated native recovery artifact.
    ///
    /// The active source and configured target must still match `request`. The
    /// artifact is fully decoded, validated, and replayed before one live-state
    /// replacement. Success keeps no normal path/savepoint, marks the restored
    /// document and EditorState dirty/recovered, and preserves the configured
    /// sequence. Failure leaves all live Core state unchanged.
    pub fn sequence_restore_autosaved_switch(
        &mut self,
        request: SequenceSwitchRequest,
        path: &Path,
    ) -> Result<DocumentInfo, CoreError> {
        self.validate_autosaved_sequence_switch_request(request)?;
        if !request.requires_switch() {
            return self.document_info();
        }
        let file = inkpod_format::read_procedure_file(path)?;
        let staged = Self::from_native_file(file, true)?;
        self.sequence_restore_prepared_target(request, staged)
    }

    pub(crate) fn sequence_restore_prepared_target(
        &mut self,
        request: SequenceSwitchRequest,
        mut staged: Core,
    ) -> Result<DocumentInfo, CoreError> {
        self.validate_autosaved_sequence_switch_request(request)?;
        let restored_uuid = staged.document.as_ref().ok_or(CoreError::NoDocument)?.uuid;
        if restored_uuid != request.target_document_uuid {
            return Err(CoreError::InvalidArgument(
                "recovery artifact does not match the sequence target",
            ));
        }
        staged.current_path = None;
        staged.recovered = true;
        staged.savepoint = None;
        staged
            .editor_session
            .as_mut()
            .ok_or(CoreError::NoDocument)?
            .savepoint = None;
        let mut sequence = self
            .sequence
            .clone()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        sequence.active_index = Some(request.target_index as usize);
        staged.sequence = Some(sequence);
        staged.sequence_render_cache = self.sequence_render_cache.clone();
        staged.sequence_render_cache.invalidate_document();
        let document = staged.document.as_ref().ok_or(CoreError::NoDocument)?;
        let (view, secondary_views) =
            self.stage_sequence_views(DocumentSizeU32::new(document.width, document.height))?;
        staged.view = view;
        staged.secondary_views = secondary_views;
        staged.next_view_id = self.next_view_id;
        staged.motion_check = None;
        staged.subpalette_index = self.subpalette_index;
        staged.inherit_file_runtime(self)?;
        *self = staged;
        self.document_info()
    }

    /// Adopts an already resolved raster pair as one sequence target. A replayed
    /// sidecar retains path/overwrite authority; a sidecar-less raster remains
    /// pathless while retaining its runtime planned-pair proof. An exact recovery
    /// may carry either authority while retaining its dirty recovery savepoints.
    /// The runtime sequence entry is rebound to the staged document UUID because
    /// raster file identities may change after atomic pair replacement.
    pub(crate) fn sequence_restore_prepared_pair_target(
        &mut self,
        request: SequenceSwitchRequest,
        mut staged: Core,
    ) -> Result<DocumentInfo, CoreError> {
        self.validate_autosaved_sequence_switch_request(request)?;
        let committed = staged.current_path.is_some()
            && staged.io_pair_authority.is_some()
            && staged.io_pair_plan.is_none();
        let planned = staged.current_path.is_none()
            && staged.io_pair_authority.is_none()
            && staged.io_pair_plan.is_some();
        if !committed && !planned {
            return Err(CoreError::InvalidArgument(
                "sequence pair target authority is inconsistent",
            ));
        }
        let restored_uuid = staged.document.as_ref().ok_or(CoreError::NoDocument)?.uuid;
        let mut sequence = self
            .sequence
            .clone()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let target = sequence
            .cells
            .get_mut(request.target_index as usize)
            .ok_or(CoreError::InvalidState("sequence switch request is stale"))?;
        if target.document_uuid != request.target_document_uuid
            || target.source_generation != request.target_source_generation
        {
            return Err(CoreError::InvalidState("sequence switch request is stale"));
        }
        target.document_uuid = restored_uuid;
        let target_source = target.clone();
        sequence.active_index = Some(request.target_index as usize);
        staged.sequence = Some(sequence);
        staged.sequence_render_cache = self.sequence_render_cache.clone();
        staged.sequence_render_cache.invalidate_document();
        let document = staged.document.as_ref().ok_or(CoreError::NoDocument)?;
        let (view, secondary_views) =
            self.stage_sequence_views(DocumentSizeU32::new(document.width, document.height))?;
        staged.view = view;
        staged.secondary_views = secondary_views;
        staged.next_view_id = self.next_view_id;
        staged.motion_check = None;
        staged.subpalette_index = self.subpalette_index;
        staged.inherit_file_runtime(self)?;
        // The common pair resolver has already proved that this staged clean
        // document exactly represents the selected immutable raster. Restore
        // the pristine identity invalidated with the outgoing document so the
        // first snapshot can admit it to the bounded sequence render cache.
        staged.register_pristine_sequence_source(&target_source);
        *self = staged;
        self.document_info()
    }

    fn validate_autosaved_sequence_switch_request(
        &self,
        request: SequenceSwitchRequest,
    ) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        self.validate_autosaved_sequence_switch_identity(request)
    }

    pub(crate) fn validate_autosaved_sequence_switch_identity(
        &self,
        request: SequenceSwitchRequest,
    ) -> Result<(), CoreError> {
        if request.policy != SequenceSwitchPolicy::AutosaveBeforeSwitch {
            return Err(CoreError::InvalidArgument(
                "sequence switch request does not use autosave policy",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let source = sequence
            .active_index
            .and_then(|index| sequence.cells.get(index))
            .ok_or(CoreError::InvalidState("sequence switch request is stale"))?;
        let target = sequence
            .cells
            .get(request.target_index as usize)
            .ok_or(CoreError::InvalidState("sequence switch request is stale"))?;
        if self.document_revision.get() != request.source_document_revision
            || self
                .editor_session
                .as_ref()
                .ok_or(CoreError::NoDocument)?
                .revision
                .get()
                != request.source_editor_revision
            || document.uuid != request.source_document_uuid
            || source.document_uuid != request.source_document_uuid
            || source.source_generation != request.source_generation
            || target.document_uuid != request.target_document_uuid
            || target.source_generation != request.target_source_generation
            || request.source_recovery_required
                != (request.requires_switch() && self.sequence_source_recovery_required())
        {
            return Err(CoreError::InvalidState("sequence switch request is stale"));
        }
        Ok(())
    }

    /// Encodes every installed sequence source without mutating Core state.
    pub fn export_sequence(
        &self,
        format: CommonRasterFormat,
        composite_white: bool,
    ) -> Result<Vec<(String, Vec<u8>)>, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        sequence
            .cells
            .iter()
            .map(|cell| {
                let raster =
                    tile_to_common(&cell.raster, Some(cell.dpi_x_milli), Some(cell.dpi_y_milli))?;
                Ok((
                    cell.name.clone(),
                    encode_common_raster(format, &raster, composite_white)?,
                ))
            })
            .collect()
    }

    /// Registers one sequence cell as the read-only subpalette sampling source.
    pub fn set_subpalette_cell(&mut self, index: usize) -> Result<(), CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        if index >= sequence.cells.len() {
            return Err(CoreError::InvalidArgument(
                "subpalette sequence index is outside bounds",
            ));
        }
        self.subpalette_index = Some(index);
        Ok(())
    }

    /// Samples the registered subpalette cell at an in-bounds source pixel.
    pub fn subpalette_sample(&self, x: u32, y: u32) -> Result<PixelValue, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let index = self
            .subpalette_index
            .ok_or(CoreError::InvalidState("subpalette has no registered cell"))?;
        Ok(sequence.cells[index].raster.pixel(x, y)?)
    }

    /// Starts sequence motion-check playback using a supported FPS.
    ///
    /// Playback state is transient and does not change document revisions/history.
    pub fn motion_check_start(
        &mut self,
        config: MotionCheckConfig,
    ) -> Result<MotionFrame, CoreError> {
        if !matches!(config.fps, 8 | 10 | 12 | 24 | 25 | 30) {
            return Err(CoreError::InvalidArgument(
                "motion-check FPS is unsupported",
            ));
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let index = sequence.active_index.unwrap_or(0);
        self.motion_check = Some(MotionCheckState {
            config,
            index,
            paused: false,
        });
        self.motion_frame()
    }

    /// Moves motion-check playback one frame in the requested direction.
    pub fn motion_check_step(
        &mut self,
        direction: SequenceDirection,
    ) -> Result<MotionFrame, CoreError> {
        let count = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?
            .cells
            .len();
        let state = self
            .motion_check
            .as_mut()
            .ok_or(CoreError::InvalidState("motion check is not active"))?;
        state.index = match direction {
            SequenceDirection::Previous => {
                if state.index == 0 {
                    if state.config.loop_playback {
                        count - 1
                    } else {
                        0
                    }
                } else {
                    state.index - 1
                }
            }
            SequenceDirection::Next => {
                if state.index + 1 >= count {
                    if state.config.loop_playback {
                        0
                    } else {
                        count - 1
                    }
                } else {
                    state.index + 1
                }
            }
        };
        self.motion_frame()
    }

    /// Toggles motion-check pause state and returns the current frame.
    pub fn motion_check_toggle_pause(&mut self) -> Result<MotionFrame, CoreError> {
        let state = self
            .motion_check
            .as_mut()
            .ok_or(CoreError::InvalidState("motion check is not active"))?;
        state.paused = !state.paused;
        self.motion_frame()
    }

    /// Stops motion-check playback; calling it while stopped is a no-op.
    pub fn motion_check_stop(&mut self) {
        self.motion_check = None;
    }

    fn motion_frame(&self) -> Result<MotionFrame, CoreError> {
        let state = self
            .motion_check
            .as_ref()
            .ok_or(CoreError::InvalidState("motion check is not active"))?;
        let cell = &self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?
            .cells[state.index];
        Ok(MotionFrame {
            sequence_index: state.index,
            cell_number: cell.cell_number,
            name: cell.name.clone(),
            thumbnail: cell.thumbnail()?,
            paused: state.paused,
            fps: state.config.fps,
            include_selection: state.config.include_selection,
            include_light_table: state.config.include_light_table,
        })
    }

    pub(super) fn document_from_sequence_source(
        source: &SequenceCellSource,
        _revision: DocumentRevision,
        next_id: &mut crate::identity::StableIdCursor,
    ) -> Result<CellDocument, CoreError> {
        let ids = DocumentIds {
            document: next_id.take_document(),
            layer: next_id.take_layer(),
            main_plane: next_id.take_plane(),
            color_plane: next_id.take_plane(),
            selection_plane: next_id.take_plane(),
            fill_protection_plane: next_id.take_plane(),
            light_table_set: next_id.take_light_table_set(),
            cell: next_id.take_cell(),
        };
        let mut document = CellDocument::new(
            ids,
            source.document_uuid,
            PaperSpec {
                width: source.raster.width(),
                height: source.raster.height(),
                dpi_x_milli: source.dpi_x_milli,
                dpi_y_milli: source.dpi_y_milli,
            },
        )?;
        document.frames = source.frames;
        initialize_imported_main_line(&mut document, source.raster.clone())?;
        Ok(document)
    }
}
