//! Detached file preparation and owner-thread publication tokens.

use super::*;

/// Runtime-only lifetime and generation for document file authority.
/// Cloning captures the same owner for tokens and internal staged candidates;
/// public `Core::clone` explicitly assigns a new owner instead.
#[derive(Clone, Debug)]
pub(super) struct PersistenceState {
    owner: Arc<()>,
    generation: PersistenceGeneration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PersistenceGeneration(u64);

impl PersistenceState {
    pub(super) fn new() -> Self {
        Self {
            owner: Arc::new(()),
            generation: PersistenceGeneration(1),
        }
    }

    pub(super) fn next(&self) -> Result<Self, CoreError> {
        Ok(Self {
            owner: Arc::clone(&self.owner),
            generation: PersistenceGeneration(self.generation.0.checked_add(1).ok_or(
                CoreError::InvalidState("file authority generation overflows"),
            )?),
        })
    }
}

impl PartialEq for PersistenceState {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}

impl Eq for PersistenceState {}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PersistenceStamp {
    authority: PersistenceState,
    document_uuid: Option<u128>,
    document_revision: DocumentRevision,
    state: StateId,
    editor: Option<(EditorRevision, EditorStateDigest, Option<EditorStateDigest>)>,
    document_savepoint: Option<StateId>,
    current_path: Option<PathBuf>,
    recovered: bool,
    raster_format: CommonRasterFormat,
}

impl PersistenceStamp {
    fn capture(core: &Core) -> Self {
        Self {
            authority: core.persistence_state.clone(),
            document_uuid: core.document.as_ref().map(|document| document.uuid),
            document_revision: core.document_revision,
            state: core.current_state,
            editor: core
                .editor_session
                .as_ref()
                .map(|editor| (editor.revision, editor.digest, editor.savepoint)),
            document_savepoint: core.savepoint,
            current_path: core.current_path.clone(),
            recovered: core.recovered,
            raster_format: core.raster_file_format,
        }
    }

    fn validate(&self, core: &Core) -> Result<(), CoreError> {
        if *self != Self::capture(core) {
            return Err(CoreError::InvalidState("document file request is stale"));
        }
        if core.active_stroke.is_some()
            || core.shooting_frame_preview.is_some()
            || core.filter_preview.is_some()
            || core.floating.is_some()
        {
            return Err(CoreError::InvalidState(
                "document file request conflicts with a preview transaction",
            ));
        }
        core.persistence_state.next()?;
        Ok(())
    }
}

/// Opaque expectation captured before asynchronous document open begins.
///
/// The token belongs to the originating Core lifetime and file generation.
/// It contains no borrowed storage and may be carried by a worker. A failed or
/// abandoned open consumes no document/history IDs and changes no live state.
#[derive(Clone, Debug)]
pub struct DocumentOpenToken {
    stamp: PersistenceStamp,
}

/// Opaque expectation identifying the document/editor states encoded for save.
///
/// The token is not proof that any files were installed. The I/O owner must
/// first validate it, fence document mutation during installation, and call
/// [`Core::commit_document_save`] only after every required output is durable.
#[derive(Clone, Debug)]
pub struct DocumentSaveToken {
    stamp: PersistenceStamp,
}

/// Immutable COW document state detached from its live Core for file preparation.
///
/// Capture clones bounded metadata and shares immutable asset/tile payloads. It
/// does not replay history, flatten pixels, encode a native file, or perform I/O.
/// The owned value can move to a worker; it contains no live frontend handle.
#[derive(Debug)]
pub struct DocumentSaveSnapshot {
    pub(super) core: Box<Core>,
    pub(super) token: DocumentSaveToken,
}

/// Both encoded outputs of one ordinary document save, prepared off the owner thread.
#[derive(Debug)]
pub struct PreparedDocumentSave {
    native: inkpod_format::NativeFile,
    raster_format: CommonRasterFormat,
    raster_bytes: Vec<u8>,
    token: DocumentSaveToken,
}

impl PreparedDocumentSave {
    /// Transfers the native DTO, companion format/bytes, and publication token.
    ///
    /// No file is written and no Core savepoint changes here. Native output can
    /// be streamed with `inkpod_format::write_procedure_to_writer`.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        inkpod_format::NativeFile,
        CommonRasterFormat,
        Vec<u8>,
        DocumentSaveToken,
    ) {
        (
            self.native,
            self.raster_format,
            self.raster_bytes,
            self.token,
        )
    }
}

impl DocumentSaveSnapshot {
    /// Prepares an explicitly confirmed, native-only history-compacted copy.
    ///
    /// The complete plan is revalidated against the captured state before the
    /// new Genesis is built. Cancellation or a stale plan yields no output.
    /// The returned token must still be validated against the live owner before
    /// installing a new destination; it must not be committed as a normal save.
    pub fn prepare_compacted_copy(
        self,
        plan: CompactionPlan,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<(inkpod_format::NativeFile, DocumentSaveToken), CoreError> {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        let native = self.core.build_compacted_native_file(plan)?;
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        Ok((native, self.token))
    }

    /// Encodes an explicit raster export from the captured immutable document.
    ///
    /// This consumes only the detached snapshot and never updates live path or
    /// savepoint authority. Cancellation is checked before and after the bounded
    /// encode. `instructions` selects the existing instruction-overlay export
    /// semantics; the requested codec may differ from the persisted save format.
    pub fn prepare_raster_export(
        self,
        format: CommonRasterFormat,
        composite_white: bool,
        instructions: bool,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Vec<u8>, CoreError> {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        let bytes = if instructions {
            self.core
                .export_instruction_common_raster(format, composite_white)?
        } else {
            self.core.export_common_raster(format, composite_white)?
        };
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        Ok(bytes)
    }

    /// Prepares normal native and same-format raster outputs without I/O.
    ///
    /// Both outputs describe exactly the captured state. Native savepoints are
    /// prospective; live savepoints remain unchanged. Unsupported output depth,
    /// encoding failure, or cancellation returns no prepared output. In
    /// particular TGA/BMP never silently quantize an RGBA16 document.
    pub fn prepare_normal_save(
        self,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<PreparedDocumentSave, CoreError> {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        let raster_format = self.core.raster_file_format;
        let raster_bytes = self.core.export_native_save_raster(raster_format)?;
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        let (native, token) = self.prepare_native_save(false, &mut cancelled)?;
        Ok(PreparedDocumentSave {
            native,
            raster_format,
            raster_bytes,
            token,
        })
    }

    /// Prepares a native-only output without adopting any path or savepoint.
    ///
    /// `recovery` retains the previous document/editor savepoints; otherwise the
    /// captured current states become the prospective savepoints. The returned
    /// token may authorize normal save publication only; recovery completion
    /// must never call [`Core::commit_document_save`].
    pub fn prepare_native_save(
        self,
        recovery: bool,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<(inkpod_format::NativeFile, DocumentSaveToken), CoreError> {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        let editor = self
            .core
            .editor_session
            .as_ref()
            .ok_or(CoreError::NoDocument)?;
        let (document_savepoint, editor_savepoint) = if recovery {
            (self.core.savepoint, editor.savepoint)
        } else {
            (Some(self.core.current_state), Some(editor.digest))
        };
        let native = self
            .core
            .build_procedure_file(document_savepoint, editor_savepoint)?;
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        Ok((native, self.token))
    }
}

impl Core {
    /// Returns the persisted format used for the normal-save raster companion.
    /// This query performs no I/O and changes no document/editor state.
    pub fn raster_file_format(&self) -> Result<CommonRasterFormat, CoreError> {
        self.document.as_ref().ok_or(CoreError::NoDocument)?;
        Ok(self.raster_file_format)
    }

    /// Selects the format of subsequently created blank or memory-raster cells.
    ///
    /// This application default starts as PNG, is not itself persisted in the
    /// document, and never changes an existing document, history, revision, dirty
    /// flag, or savepoint. Raster-file import records its actual format instead.
    pub fn set_new_cell_raster_format(&mut self, format: CommonRasterFormat) {
        self.new_cell_raster_format = format;
    }

    /// Captures a revision/authority expectation before asynchronous open.
    /// Empty Core instances are accepted. Active previews/installations are not.
    pub fn capture_document_open(&self) -> Result<DocumentOpenToken, CoreError> {
        self.ensure_no_active_stroke()?;
        let stamp = PersistenceStamp::capture(self);
        stamp.validate(self)?;
        Ok(DocumentOpenToken { stamp })
    }

    /// Fully validates/replays one native DTO into an isolated, pathless Core.
    ///
    /// This constructor may run on a worker; it touches no existing Core. A
    /// recovery result clears both savepoints and is dirty/recovered. Ordinary
    /// results retain their encoded savepoints until the owner adopts a path.
    pub fn from_native_file(
        file: inkpod_format::NativeFile,
        recovered: bool,
    ) -> Result<Self, CoreError> {
        let mut staged = Self::from_procedure_file(file)?;
        if recovered {
            staged.recovered = true;
            staged.savepoint = None;
            staged
                .editor_session
                .as_mut()
                .ok_or(CoreError::NoDocument)?
                .savepoint = None;
        }
        Ok(staged)
    }

    /// Adopts a fully staged document exactly once at the owner-thread boundary.
    ///
    /// Stale token, preview, invalid staged state, or authority overflow leaves
    /// both Cores unchanged from the caller's perspective (the staged value is
    /// consumed). Success retains this Core's I/O service and creation defaults,
    /// advances runtime file authority, and performs one state replacement.
    /// Recovery results reject normal paths; ordinary native results may adopt
    /// `path`. Raster results remain pathless and use frontend source identity.
    pub fn adopt_opened_document(
        &mut self,
        token: DocumentOpenToken,
        mut staged: Core,
        path: Option<&Path>,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        token.stamp.validate(self)?;
        staged.ensure_no_active_stroke()?;
        staged.document_info()?;
        if path.is_some_and(|path| path.as_os_str().is_empty())
            || (staged.recovered && path.is_some())
        {
            return Err(CoreError::InvalidArgument(
                "invalid opened document path authority",
            ));
        }
        staged.current_path = path.map(Path::to_path_buf);
        if staged.current_path.is_none() {
            staged.io_pair_authority = None;
        }
        staged.inherit_file_runtime(self)?;
        *self = staged;
        self.document_info()
    }

    /// Captures immutable save input without encoding, replay, or filesystem I/O.
    /// Active preview/floating state and pending file installation are rejected.
    pub fn capture_document_save(&self) -> Result<DocumentSaveSnapshot, CoreError> {
        self.capture_file_snapshot(false)
    }

    /// Captures an explicitly confirmed compaction without encoding or I/O.
    ///
    /// Cheap count/editor checks reject an already stale plan here. Full
    /// document/journal digest validation runs in the detached preparation
    /// phase, so capture does not scan pixel or journal argument payloads.
    /// Success changes no live state and preserves all history until the caller
    /// separately installs the prepared copy at a new path.
    pub fn capture_compacted_copy(
        &self,
        plan: CompactionPlan,
    ) -> Result<DocumentSaveSnapshot, CoreError> {
        let editor = self.editor_session.as_ref().ok_or(CoreError::NoDocument)?;
        let procedure_count = self
            .journal
            .iter()
            .filter(|entry| matches!(entry, JournalEntry::Commit(_)))
            .count() as u64;
        if plan.history_event_count != self.journal.len() as u64
            || plan.history_procedure_count != procedure_count
            || plan.editor_digest != editor.digest
        {
            return Err(CoreError::InvalidState("compaction plan is stale"));
        }
        self.capture_document_save()
    }

    pub(super) fn capture_file_snapshot(
        &self,
        retain_sequence: bool,
    ) -> Result<DocumentSaveSnapshot, CoreError> {
        self.ensure_no_active_stroke()?;
        self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let stamp = PersistenceStamp::capture(self);
        stamp.validate(self)?;
        let mut frozen = self.clone_for_staging();
        frozen.io_manager = None;
        frozen.render_cache.clear();
        frozen.secondary_views.clear();
        if !retain_sequence {
            frozen.sequence = None;
            frozen.sequence_render_cache.clear_retained();
        }
        frozen.motion_check = None;
        frozen.subpalette_index = None;
        Ok(DocumentSaveSnapshot {
            core: Box::new(frozen),
            token: DocumentSaveToken { stamp },
        })
    }

    /// Revalidates a prepared save before external installation begins.
    /// This query is allowed while the installation fence is held and mutates nothing.
    pub fn validate_document_save(&self, token: &DocumentSaveToken) -> Result<(), CoreError> {
        token.stamp.validate(self)
    }

    /// Publishes normal-save authority after the I/O owner installed every output.
    ///
    /// The I/O owner must validate the token and fence edits before installation;
    /// this method itself performs no filesystem operation or durability check.
    /// Stale/invalid/overflow failure changes no live field. Success advances
    /// file authority and both savepoints, clears recovered/install-pending state,
    /// and leaves document/editor revisions, history, journal, and stable IDs intact.
    pub fn commit_document_save(
        &mut self,
        token: DocumentSaveToken,
        path: &Path,
    ) -> Result<DocumentInfo, CoreError> {
        token.stamp.validate(self)?;
        if path.as_os_str().is_empty() {
            return Err(CoreError::InvalidArgument("normal save path is empty"));
        }
        let next_authority = self.persistence_state.next()?;
        let current_path = path.to_path_buf();
        let editor = self.editor_session.as_mut().ok_or(CoreError::NoDocument)?;
        self.savepoint = Some(self.current_state);
        editor.savepoint = Some(editor.digest);
        self.current_path = Some(current_path);
        self.persistence_state = next_authority;
        self.recovered = false;
        self.io_install_pending = false;
        self.document_info()
    }

    /// Preserves application-owned file services across staged document replacement.
    pub(super) fn inherit_file_runtime(&mut self, previous: &Core) -> Result<(), CoreError> {
        self.persistence_state = previous.persistence_state.next()?;
        self.new_cell_raster_format = previous.new_cell_raster_format;
        self.io_manager = previous.io_manager.clone();
        self.io_install_pending = false;
        Ok(())
    }
}
