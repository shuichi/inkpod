use super::execute::{InMemoryInputFingerprint, ScriptRunError, capture_in_memory_fingerprint};
use super::plan::ScriptSessionSnapshot;
use crate::{Core, CoreError};
use inkpod_format::{CommonRasterFormat, InkScriptOutputFormat, encode_procedure_file};

/// Publication destination of an owned, staged document result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptStagedResultKind {
    /// One canonical transaction for the exact issue-time session.
    ActiveDocument,
    /// A fresh, pathless and dirty document with reconstructed history/editor state.
    NewTab,
}

/// Owns one result until its owner thread either publishes or drops it.
/// Active results cannot be transferred as an unrelated new tab.
#[derive(Debug)]
pub struct ScriptStagedResult {
    ordinal: usize,
    core: Core,
    active: Option<ActivePublication>,
}

#[derive(Debug)]
struct ActivePublication {
    session: (u64, u64, u64),
    document: InMemoryInputFingerprint,
    persistence: crate::DocumentSaveToken,
    changed: bool,
}

impl ScriptStagedResult {
    pub(super) fn active(
        ordinal: usize,
        core: Core,
        source: &ScriptSessionSnapshot,
    ) -> Result<Self, ScriptRunError> {
        let base = source
            .clone_staged_core()
            .map_err(|_| ScriptRunError::StaleInput)?;
        let document = capture_in_memory_fingerprint(&base)?;
        let changed = capture_in_memory_fingerprint(&core)? != document;
        Ok(Self {
            ordinal,
            core,
            active: Some(ActivePublication {
                session: (
                    source.session_id(),
                    source.session_generation(),
                    source.source_generation(),
                ),
                document,
                persistence: source.publication_token(),
                changed,
            }),
        })
    }

    pub(super) fn new_tab(
        ordinal: usize,
        working: &Core,
        identity: u128,
    ) -> Result<Self, CoreError> {
        let mut document = working
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .clone();
        document.uuid = identity;
        let mut core = crate::batch::core_from_document(document, working.assets.clone())?;
        core.raster_file_format = working.raster_file_format;
        core.io_manager = working.io_manager.clone();
        core.savepoint = None;
        Ok(Self {
            ordinal,
            core,
            active: None,
        })
    }

    /// Returns the zero-based ordinal in the immutable input plan.
    pub const fn ordinal(&self) -> usize {
        self.ordinal
    }
    /// Returns the required publication route.
    pub const fn kind(&self) -> ScriptStagedResultKind {
        if self.active.is_some() {
            ScriptStagedResultKind::ActiveDocument
        } else {
            ScriptStagedResultKind::NewTab
        }
    }
    /// Borrows the immutable staged state without transferring publication ownership.
    pub const fn core(&self) -> &Core {
        &self.core
    }
    /// Transfers a pathless new document exactly once; active results are rejected.
    pub fn into_new_tab(self) -> Result<Core, ScriptRunError> {
        if self.active.is_some() {
            return Err(ScriptRunError::InvalidInput);
        }
        Ok(self.core)
    }
    /// Publishes one active transaction after checking the session, generation and complete
    /// document/editor fingerprint again. Stale input returns an error without changing `core`.
    /// View-only state and path authority are retained; both savepoints remain unchanged.
    /// The owner supplies its current cancellation state, including cancellation after the
    /// worker finished. It is polled on entry and immediately before publication; cancellation
    /// (including for a no-op) or dropping this result leaves the live Core unchanged.
    pub fn apply_active(
        self,
        core: &mut Core,
        session_id: u64,
        session_generation: u64,
        source_generation: u64,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), ScriptRunError> {
        if cancelled() {
            return Err(ScriptRunError::Cancelled);
        }
        let Some(expected) = self.active else {
            return Err(ScriptRunError::InvalidInput);
        };
        if (session_id, session_generation, source_generation) != expected.session
            || capture_in_memory_fingerprint(core)? != expected.document
            || core.validate_document_save(&expected.persistence).is_err()
        {
            return Err(ScriptRunError::StaleInput);
        }
        core.ensure_no_active_stroke()?;
        if cancelled() {
            return Err(ScriptRunError::Cancelled);
        }
        if !expected.changed {
            return Ok(());
        }
        core.publish_staged_document_edit(self.core);
        Ok(())
    }
}

pub(super) fn materialize(core: &Core) -> Result<Core, CoreError> {
    let mut result = crate::batch::core_from_document(
        core.document.as_ref().ok_or(CoreError::NoDocument)?.clone(),
        core.assets.clone(),
    )?;
    result.raster_file_format = core.raster_file_format;
    result.io_manager = core.io_manager.clone();
    Ok(result)
}

pub(super) const fn raster_format(format: InkScriptOutputFormat) -> Option<CommonRasterFormat> {
    match format {
        InkScriptOutputFormat::Inkpod => None,
        InkScriptOutputFormat::Png => Some(CommonRasterFormat::Png),
        InkScriptOutputFormat::Tiff => Some(CommonRasterFormat::Tiff),
        InkScriptOutputFormat::Tga => Some(CommonRasterFormat::Tga),
        InkScriptOutputFormat::Bmp => Some(CommonRasterFormat::Bmp),
    }
}

pub(super) fn encode_output(
    core: &Core,
    format: InkScriptOutputFormat,
) -> Result<Vec<u8>, CoreError> {
    if let Some(format) = raster_format(format) {
        core.export_common_raster(format, false)
    } else {
        let editor = core.editor_state()?.digest;
        let file = core.build_procedure_file(Some(core.current_state), Some(editor))?;
        encode_procedure_file(&file).map_err(|error| CoreError::Format(error.to_string()))
    }
}
