//! Detached autosave-before-switch preparation and atomic owner publication.

use crate::{
    Core, CoreError, DocumentInfo, DocumentSaveSnapshot, DocumentSaveToken, SequenceSwitchRequest,
};
use inkpod_format::NativeFile;

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
/// The I/O owner must durably install the source recovery and its association
/// before committing this value. Taking the DTO is not proof of durability.
#[derive(Debug)]
pub struct PreparedSequenceSwitch {
    source_recovery: Option<NativeFile>,
    target: Option<Box<Core>>,
    request: SequenceSwitchRequest,
    sequence_revision: u64,
    token: DocumentSaveToken,
}

impl SequenceSwitchSnapshot {
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
                request: self.request,
                sequence_revision: self.sequence_revision,
                token,
            });
        }
        let editor_savepoint = core
            .editor_session
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .savepoint;
        let source_recovery = core.build_procedure_file(core.savepoint, editor_savepoint)?;
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        if let Some(native) = target_recovery {
            let target = Core::from_native_file(native, true)?;
            core.sequence_restore_prepared_target(self.request, target)?;
        } else {
            core.sequence_commit_autosaved_switch(self.request)?;
        }
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        Ok(PreparedSequenceSwitch {
            source_recovery: Some(source_recovery),
            target: Some(core),
            request: self.request,
            sequence_revision: self.sequence_revision,
            token,
        })
    }
}

impl PreparedSequenceSwitch {
    /// Transfers the native-only source recovery for durable external storage.
    /// Returns `None` for a same-cell no-op or after the DTO was already taken.
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
            if target.document_info()?.document_uuid != prepared.request.target_document_uuid {
                return Err(CoreError::InvalidState(
                    "prepared sequence target is invalid",
                ));
            }
        }
        Ok(())
    }

    /// Publishes a prepared target after the source recovery is durable.
    ///
    /// The caller must fence mutation between validation and recovery install.
    /// This method performs no I/O. Stale/error leaves every live field unchanged;
    /// success replaces the document once, preserving current application I/O,
    /// creation defaults, view identities, and reference selection. Primary view
    /// resets as for a synchronous switch. A same-cell request advances nothing.
    pub fn commit_prepared_sequence_switch(
        &mut self,
        prepared: PreparedSequenceSwitch,
    ) -> Result<DocumentInfo, CoreError> {
        self.validate_prepared_sequence_switch(&prepared)?;
        let Some(mut staged) = prepared.target else {
            self.io_install_pending = false;
            return self.document_info();
        };
        staged.inherit_file_runtime(self)?;
        staged.io_pair_authority = None;
        staged.secondary_views = self.secondary_views.clone();
        staged.next_view_id = self.next_view_id;
        staged.color_check = self.color_check;
        staged.next_render_tile_revision = self.next_render_tile_revision;
        staged.next_preview_revision = self.next_preview_revision;
        staged.editor_defaults = self.editor_defaults.clone();
        staged.shortcuts = self.shortcuts.clone();
        staged.shortcut_defaults = self.shortcut_defaults.clone();
        staged.subpalette_index = self.subpalette_index;
        staged.motion_check = None;
        staged.view = self.view;
        staged.reset_view();
        staged.render_cache.clear();
        *self = *staged;
        self.document_info()
    }
}
