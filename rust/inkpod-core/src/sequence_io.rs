//! Detached autosave-before-switch preparation and atomic owner publication.

use crate::{
    Core, CoreError, DocumentInfo, DocumentSaveSnapshot, DocumentSaveToken, SequenceSwitchRequest,
};
use inkpod_format::NativeFile;
use inkpod_io::RecoveryPairProof;
use std::path::PathBuf;

fn recovery_pair_conflict(error: CoreError) -> CoreError {
    if error == CoreError::Cancelled {
        CoreError::Cancelled
    } else {
        CoreError::FileConflict
    }
}

fn validate_recovery_pair_lineage(recovery: &Core, resolved: &Core) -> Result<(), CoreError> {
    let recovery_document = recovery.document.as_ref().ok_or(CoreError::FileConflict)?;
    let resolved_document = resolved.document.as_ref().ok_or(CoreError::FileConflict)?;
    if recovery_document.uuid != resolved_document.uuid
        || recovery.raster_file_format != resolved.raster_file_format
    {
        return Err(CoreError::FileConflict);
    }
    let recovery_genesis = recovery.genesis.as_ref().ok_or(CoreError::FileConflict)?;
    let resolved_genesis = resolved.genesis.as_ref().ok_or(CoreError::FileConflict)?;
    let recovery_digest = crate::primitive::canonical_document_state(&recovery_genesis.document)
        .map_err(recovery_pair_conflict)?
        .1;
    let resolved_digest = crate::primitive::canonical_document_state(&resolved_genesis.document)
        .map_err(recovery_pair_conflict)?
        .1;
    let raster_source_matches = match (
        recovery_genesis.raster_source,
        resolved_genesis.raster_source,
    ) {
        (None, None) => true,
        (Some(recovery), Some(resolved)) => {
            recovery.plane_id == resolved.plane_id && recovery.asset_id == resolved.asset_id
        }
        (None, Some(_)) | (Some(_), None) => false,
    };
    if recovery_digest != resolved_digest || !raster_source_matches {
        return Err(CoreError::FileConflict);
    }

    // A recovery may extend the pair's saved journal, but the independently
    // resolved pair must be the exact document/editor savepoint encoded by the
    // recovery. UUID and Genesis alone are not a lost-update fence: another
    // coherent pair can retain both while replacing the saved history/state.
    // Require a clean resolved baseline, its complete journal as an exact
    // recovery prefix, and both saved-state digests before adopting authority.
    let resolved_editor = resolved
        .editor_session
        .as_ref()
        .ok_or(CoreError::FileConflict)?;
    let recovery_editor = recovery
        .editor_session
        .as_ref()
        .ok_or(CoreError::FileConflict)?;
    if resolved.savepoint != Some(resolved.current_state)
        || resolved_editor.savepoint != Some(resolved_editor.digest)
        || recovery.savepoint != Some(resolved.current_state)
        || recovery_editor.savepoint != Some(resolved_editor.digest)
        || !recovery.journal.starts_with(&resolved.journal)
    {
        return Err(CoreError::FileConflict);
    }
    let recovery_savepoint_digest = if resolved.current_state == crate::StateId::GENESIS {
        recovery_digest
    } else {
        recovery
            .journal
            .iter()
            .find_map(|entry| match entry {
                crate::JournalEntry::Commit(commit)
                    if commit.committed_state_id() == resolved.current_state =>
                {
                    Some(commit.procedure().post_state_digest())
                }
                crate::JournalEntry::Commit(_)
                | crate::JournalEntry::HistoryMove(_)
                | crate::JournalEntry::BranchCut(_) => None,
            })
            .ok_or(CoreError::FileConflict)?
    };
    if recovery_savepoint_digest
        != resolved
            .document_state_digest()
            .map_err(recovery_pair_conflict)?
    {
        return Err(CoreError::FileConflict);
    }
    Ok(())
}

/// Captured source document and immutable sequence for an asynchronous switch.
///
/// This value owns COW metadata/payload references and no live Core handle. It
/// may move to a worker. Capture performs no encoding, replay, or filesystem I/O.
#[derive(Debug)]
pub struct SequenceSwitchSnapshot {
    document: DocumentSaveSnapshot,
    request: SequenceSwitchRequest,
    sequence_revision: u64,
}

/// Validated target and source recovery data awaiting owner-thread publication.
///
/// When source recovery is present, the I/O owner must durably install it and
/// its association before committing this value. Taking the DTO is not proof
/// of durability. File I/O may explicitly omit recovery for a clean source.
#[derive(Debug)]
pub struct PreparedSequenceSwitch {
    source_recovery: Option<NativeFile>,
    target: Option<Box<Core>>,
    target_document_uuid: u128,
    request: SequenceSwitchRequest,
    sequence_revision: u64,
    token: DocumentSaveToken,
}

/// One shared-resolver result used to fence sequence recovery authority.
pub(crate) struct ResolvedRecoveryPairTarget {
    pub(crate) core: Core,
    pub(crate) normal_path: Option<PathBuf>,
    pub(crate) proof: RecoveryPairProof,
    pub(crate) raster_missing: Option<(PathBuf, inkpod_io::FileIdentity)>,
}

impl SequenceSwitchSnapshot {
    /// Revalidates the target identity retained by the immutable switch snapshot.
    ///
    /// The pair resolver independently validates the current native/raster
    /// members, their canonical decoded content, and their final filesystem
    /// stamps. The sequence catalog raster is only a discovery/thumbnail source:
    /// comparing it to the current pair would incorrectly reject a cell after a
    /// successful normal save updated that pair on disk.
    pub(crate) fn validate_target_source(&self) -> Result<(), CoreError> {
        let source = self
            .document
            .core
            .sequence
            .as_ref()
            .and_then(|sequence| sequence.cells.get(self.request.target_index as usize))
            .ok_or(CoreError::InvalidState("sequence switch request is stale"))?;
        if source.document_uuid != self.request.target_document_uuid
            || source.source_generation != self.request.target_source_generation
        {
            return Err(CoreError::FileConflict);
        }
        Ok(())
    }

    pub(crate) fn managed_target_raster(
        &self,
        manager: &inkpod_io::IoManager,
        image: &inkpod_io::LoadedImage,
    ) -> Result<crate::asset::ManagedRasterDecision, CoreError> {
        let source = self
            .document
            .core
            .sequence
            .as_ref()
            .and_then(|sequence| sequence.cells.get(self.request.target_index as usize))
            .ok_or(CoreError::InvalidState("sequence switch request is stale"))?;
        Ok(source.managed_raster_input(manager, image).map_or(
            crate::asset::ManagedRasterDecision::Ineligible,
            crate::asset::ManagedRasterDecision::Reuse,
        ))
    }

    /// Prepares the old source recovery and a fully validated target without I/O.
    ///
    /// A supplied recovery must have the captured target UUID. Otherwise the
    /// immutable sequence raster is activated with the existing Core primitive.
    /// Same-cell requests are true no-ops and produce no recovery artifact.
    /// Cancellation is checked around native encoding and target replay; failure
    /// consumes only this detached value and never changes the live document.
    pub fn prepare(
        self,
        target_recovery: Option<NativeFile>,
        cancelled: impl FnMut() -> bool,
    ) -> Result<PreparedSequenceSwitch, CoreError> {
        self.prepare_impl(target_recovery, None, true, cancelled)
    }

    /// Omits recovery encoding only for a clean source or a same-cell no-op.
    /// The owner still validates the captured document/editor save token before
    /// publishing the prepared target; no normal savepoint is advanced.
    pub(crate) fn prepare_without_source_recovery(
        self,
        target_recovery: Option<NativeFile>,
        cancelled: impl FnMut() -> bool,
    ) -> Result<PreparedSequenceSwitch, CoreError> {
        self.prepare_impl(target_recovery, None, false, cancelled)
    }

    /// Uses a validated raster-pair resolution as the target while retaining
    /// this switch's immutable sequence catalog. `normal_path` is present for a
    /// replayed sidecar and absent for a pathless planned pair. The resolver
    /// must have completed its native/raster checks before supplying `target`.
    pub(crate) fn prepare_pair_target(
        self,
        mut target: Core,
        normal_path: Option<PathBuf>,
        include_source_recovery: bool,
        cancelled: impl FnMut() -> bool,
    ) -> Result<PreparedSequenceSwitch, CoreError> {
        if normal_path
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
            || target.recovered
        {
            return Err(CoreError::InvalidArgument(
                "sequence pair target path is invalid",
            ));
        }
        target.current_path = normal_path;
        self.prepare_impl(
            None,
            Some(Box::new(target)),
            include_source_recovery,
            cancelled,
        )
    }

    /// Restores an exact recovery document/history while adopting only the
    /// runtime save authority from its independently resolved raster pair.
    ///
    /// The recovery and pair must have the same document UUID, raster format,
    /// canonical Genesis document, and immutable Genesis raster identity. A
    /// replayed sidecar supplies committed authority; a sidecar-less raster
    /// supplies planned authority. Any mismatch is an external file conflict.
    pub(crate) fn prepare_recovery_pair_target(
        self,
        native: NativeFile,
        resolution: ResolvedRecoveryPairTarget,
        include_source_recovery: bool,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<PreparedSequenceSwitch, CoreError> {
        let ResolvedRecoveryPairTarget {
            core: mut resolved,
            normal_path,
            proof: expected_pair,
            raster_missing: resolved_raster_missing,
        } = resolution;
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        // Pair-backed sequence recovery is an internal cell snapshot, not a
        // standalone crash-recovery open. Preserve its serialized document and
        // editor savepoints: edited sources return dirty, while a clean
        // repair-needed source stays clean after navigation-only round-trip.
        let mut recovery = Core::from_native_file(native, false).map_err(recovery_pair_conflict)?;
        if recovery.document_info()?.document_uuid != self.request.target_document_uuid
            || resolved.document_info()?.document_uuid != self.request.target_document_uuid
        {
            return Err(CoreError::FileConflict);
        }
        validate_recovery_pair_lineage(&recovery, &resolved)?;
        // Pair-backed recovery is an internal navigation snapshot. Preserve
        // clean navigation as clean, but retain the user-visible recovered
        // marker when the snapshot actually carries unsaved document/editor
        // changes. Standalone recovery remains unconditionally recovered in
        // `prepare_impl`'s separate authority-none path.
        recovery.recovered = recovery.document_info()?.dirty;

        let authority_matches = match expected_pair {
            RecoveryPairProof::Committed {
                native: expected_native,
                raster: expected_raster,
            } => normal_path.as_ref().is_some_and(|path| {
                !path.as_os_str().is_empty()
                    && resolved
                        .io_pair_authority
                        .as_ref()
                        .is_some_and(|authority| {
                            authority.native_path == *path
                                && authority.native == expected_native
                                && authority.raster == Some(expected_raster)
                        })
                    && resolved.io_pair_plan.is_none()
            }),
            RecoveryPairProof::Planned {
                native_missing,
                raster,
            } => {
                normal_path.is_none()
                    && resolved.io_pair_authority.is_none()
                    && resolved.io_pair_plan.as_ref().is_some_and(|plan| {
                        plan.native_missing == native_missing && plan.raster == raster
                    })
            }
            RecoveryPairProof::RepairNeeded {
                native: expected_native,
                raster_missing,
            } => normal_path.as_ref().is_some_and(|path| {
                !path.as_os_str().is_empty()
                    && resolved_raster_missing
                        .as_ref()
                        .is_some_and(|(_, identity)| *identity == raster_missing)
                    && resolved
                        .io_pair_authority
                        .as_ref()
                        .is_some_and(|authority| {
                            authority.native_path == *path
                                && authority.native == expected_native
                                && authority.raster.is_none()
                                && authority.raster_missing == Some(raster_missing)
                                && resolved_raster_missing.as_ref().is_some_and(
                                    |(raster_path, _)| authority.raster_path == *raster_path,
                                )
                        })
                    && resolved.io_pair_plan.is_none()
            }),
        };
        if !authority_matches {
            return Err(CoreError::FileConflict);
        }
        recovery.current_path = normal_path;
        recovery.io_pair_authority = resolved.io_pair_authority.take();
        recovery.io_pair_plan = resolved.io_pair_plan.take();
        self.prepare_impl(
            None,
            Some(Box::new(recovery)),
            include_source_recovery,
            cancelled,
        )
    }

    fn prepare_impl(
        self,
        target_recovery: Option<NativeFile>,
        target_normal: Option<Box<Core>>,
        include_source_recovery: bool,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<PreparedSequenceSwitch, CoreError> {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        let DocumentSaveSnapshot { mut core, token } = self.document;
        if !self.request.requires_switch() {
            return Ok(PreparedSequenceSwitch {
                source_recovery: None,
                target: None,
                target_document_uuid: self.request.target_document_uuid,
                request: self.request,
                sequence_revision: self.sequence_revision,
                token,
            });
        }
        let target_recovery = if let Some(native) = target_recovery {
            let target = Core::from_native_file(native, true).map_err(|error| {
                if error == CoreError::Cancelled {
                    CoreError::Cancelled
                } else {
                    CoreError::FileConflict
                }
            })?;
            if target.document_info()?.document_uuid != self.request.target_document_uuid {
                return Err(CoreError::FileConflict);
            }
            Some(target)
        } else {
            None
        };
        let source_recovery = if include_source_recovery {
            let editor_savepoint = core
                .editor_session
                .as_ref()
                .ok_or(CoreError::NoDocument)?
                .savepoint;
            Some(core.build_procedure_file(core.savepoint, editor_savepoint)?)
        } else {
            if core.savepoint != Some(core.current_state) || core.editor_dirty() {
                return Err(CoreError::InvalidArgument(
                    "dirty sequence source requires a recovery destination",
                ));
            }
            None
        };
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        if target_recovery.is_some() && target_normal.is_some() {
            return Err(CoreError::InvalidArgument(
                "sequence target cannot be both recovery and normal pair",
            ));
        }
        if let Some(target) = target_recovery {
            core.sequence_restore_prepared_target(self.request, target)?;
        } else if let Some(target) = target_normal {
            core.sequence_restore_prepared_pair_target(self.request, *target)?;
        } else {
            core.sequence_commit_autosaved_switch(self.request)?;
        }
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        let target_document_uuid = core.document_info()?.document_uuid;
        Ok(PreparedSequenceSwitch {
            source_recovery,
            target: Some(core),
            target_document_uuid,
            request: self.request,
            sequence_revision: self.sequence_revision,
            token,
        })
    }
}

impl PreparedSequenceSwitch {
    /// Validates only the originating Core lifetime.
    ///
    /// This deliberately omits the document/editor/sequence revision checks
    /// used by a successful commit. A failed or explicitly cancelled durable
    /// source-recovery installation still has to release the original owner's
    /// install fence even if another semantic stamp is stale.
    pub(crate) fn validate_owner(&self, core: &Core) -> Result<(), CoreError> {
        self.token.validate_owner(core)
    }

    /// Transfers the native-only source recovery for durable external storage.
    /// Returns `None` for a same-cell no-op, an explicitly omitted clean source,
    /// or after the DTO was already taken.
    pub fn take_source_recovery(&mut self) -> Option<NativeFile> {
        self.source_recovery.take()
    }
}

impl Core {
    /// Captures an autosave-before-switch request and its source state.
    ///
    /// Invalid/stale identity, active preview, or pending installation is rejected
    /// without mutation. Capture retains the immutable sequence sources but does
    /// not decode, replay, flatten, encode, or access the filesystem.
    pub fn capture_sequence_switch(
        &self,
        request: SequenceSwitchRequest,
    ) -> Result<SequenceSwitchSnapshot, CoreError> {
        self.validate_autosaved_sequence_switch_identity(request)?;
        let sequence_revision = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?
            .revision;
        Ok(SequenceSwitchSnapshot {
            document: self.capture_file_snapshot(true)?,
            request,
            sequence_revision,
        })
    }

    /// Revalidates a prepared switch against its originating document and sequence.
    ///
    /// The source document/editor states, file authority, and entire sequence
    /// revision must still match. This query changes nothing and is permitted
    /// while the I/O owner's installation fence is held.
    pub fn validate_prepared_sequence_switch(
        &self,
        prepared: &PreparedSequenceSwitch,
    ) -> Result<(), CoreError> {
        self.validate_document_save(&prepared.token)?;
        self.validate_autosaved_sequence_switch_identity(prepared.request)?;
        if self.sequence.as_ref().map(|sequence| sequence.revision)
            != Some(prepared.sequence_revision)
        {
            return Err(CoreError::InvalidState("sequence switch request is stale"));
        }
        if let Some(target) = &prepared.target {
            if target.document_info()?.document_uuid != prepared.target_document_uuid {
                return Err(CoreError::InvalidState(
                    "prepared sequence target is invalid",
                ));
            }
        }
        Ok(())
    }

    /// Publishes a prepared target after any source recovery is durable.
    ///
    /// The caller must fence mutation between validation and recovery install.
    /// This method performs no I/O. Stale/error leaves every live field unchanged;
    /// success replaces the document once, preserving current application I/O,
    /// creation defaults, view identities, and reference selection. Same-size
    /// views are retained; different sizes use the current view resize policy
    /// before publication. A same-cell request advances nothing;
    /// an explicitly omitted clean source needs no recovery installation.
    pub fn commit_prepared_sequence_switch(
        &mut self,
        prepared: PreparedSequenceSwitch,
    ) -> Result<DocumentInfo, CoreError> {
        self.validate_prepared_sequence_switch(&prepared)?;
        let Some(mut staged) = prepared.target else {
            self.io_install_pending = false;
            return self.document_info();
        };
        let document = staged.document.as_ref().ok_or(CoreError::NoDocument)?;
        let (next_view, next_secondary_views) = self
            .stage_sequence_views(crate::DocumentSizeU32::new(document.width, document.height))?;
        staged.inherit_file_runtime(self)?;
        staged.secondary_views = next_secondary_views;
        staged.next_view_id = self.next_view_id;
        staged.color_check = self.color_check;
        staged.next_render_tile_revision = self.next_render_tile_revision;
        staged.next_preview_revision = self.next_preview_revision;
        staged.editor_defaults = self.editor_defaults.clone();
        staged.shortcuts = self.shortcuts.clone();
        staged.shortcut_defaults = self.shortcut_defaults.clone();
        staged.subpalette_index = self.subpalette_index;
        staged.motion_check = None;
        staged.view = next_view;
        staged.render_cache.clear();
        *self = *staged;
        self.document_info()
    }
}
