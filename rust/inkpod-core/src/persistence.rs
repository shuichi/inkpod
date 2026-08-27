//! Save, open, recovery, and revert operations.

use super::*;

impl Core {
    /// Atomically writes the active document as a native-only file.
    ///
    /// Success records the exact current document and editor states as savepoints,
    /// clears recovered status, and leaves document revision/history unchanged.
    /// The prospective savepoints are encoded before I/O but become live only
    /// after durable same-directory replacement succeeds. Failure leaves the
    /// previous file, path, and both Core savepoints unchanged.
    /// Application normal saves use detached paired preparation instead; this
    /// explicit primitive never creates a raster companion.
    pub fn save(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let next_authority = self.persistence_state.next()?;
        self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let editor = self.editor_session.as_ref().ok_or(CoreError::NoDocument)?;
        let document_savepoint = self.current_state;
        let editor_savepoint = editor.digest;
        let file = self.build_procedure_file(Some(document_savepoint), Some(editor_savepoint))?;
        inkpod_format::save_procedure_file_atomic(path, &file)?;
        self.savepoint = Some(document_savepoint);
        self.editor_session
            .as_mut()
            .ok_or(CoreError::NoDocument)?
            .savepoint = Some(editor_savepoint);
        self.current_path = Some(path.to_path_buf());
        self.persistence_state = next_authority;
        self.recovered = false;
        self.document_info()
    }

    /// Atomically writes recovery data without advancing the normal-save savepoint.
    ///
    /// Document revision, history, dirty state, current normal path, and recovered
    /// status are unchanged. Genesis, retained assets, journal branches, and
    /// EditorState are encoded without adopting the recovery path as authority.
    pub fn autosave(&self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let editor_savepoint = self
            .editor_session
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .savepoint;
        let file = self.build_procedure_file(self.savepoint, editor_savepoint)?;
        inkpod_format::save_recovery_procedure_file_atomic(path, &file)?;
        self.document_info()
    }

    /// Reads and fully validates a native document before replacing Core state.
    ///
    /// Success restores the complete journal, cursor, branches, ID authorities,
    /// EditorState, and persisted savepoints, records `path`, and rebases the
    /// runtime document revision. Read or validation failure retains the previous
    /// live Core unchanged.
    pub fn open(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        let token = self.capture_document_open()?;
        let file = inkpod_format::read_procedure_file(path)?;
        let staged = Self::from_native_file(file, false)?;
        self.adopt_opened_document(token, staged, Some(path))
    }

    /// Opens validated recovery data as a dirty recovered document.
    ///
    /// No normal-save path/savepoint is adopted. Failure leaves current Core state
    /// unchanged; success restores history and EditorState while marking both
    /// document and editor state dirty in a pathless recovered session.
    pub fn open_recovery(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        let token = self.capture_document_open()?;
        let file = inkpod_format::read_procedure_file(path)?;
        let staged = Self::from_native_file(file, true)?;
        self.adopt_opened_document(token, staged, None)
    }

    /// Compares recovery and normal-save timestamps using format-layer policy.
    ///
    /// This query does not mutate Core or either file.
    pub fn recovery_is_newer(
        &self,
        normal_path: &Path,
        recovery_path: &Path,
    ) -> Result<bool, CoreError> {
        Ok(inkpod_format::recovery_is_newer(
            normal_path,
            recovery_path,
        )?)
    }

    /// Removes a recovery artifact using the format layer's bounded path policy.
    ///
    /// This external file operation does not alter Core document/savepoint state.
    pub fn discard_recovery(&self, path: &Path) -> Result<(), CoreError> {
        inkpod_format::discard_recovery(path)?;
        Ok(())
    }

    /// Reopens the last successful normal-save path, discarding live edits.
    ///
    /// The operation uses [`Core::open`] atomic replacement semantics and is an
    /// error when no normal-save path is known.
    pub fn revert(&mut self) -> Result<DocumentInfo, CoreError> {
        let path = self
            .current_path
            .clone()
            .ok_or(CoreError::InvalidState("document has no normal-save path"))?;
        self.open(&path)
    }

    /// Returns bounded native persistence and checkpoint-policy diagnostics.
    ///
    /// The deterministic counters are derived from immutable assets and the
    /// authoritative journal. This query performs no replay, I/O, allocation
    /// authority change, savepoint update, or cache invalidation.
    pub fn persistence_info(&self) -> Result<PersistenceInfo, CoreError> {
        self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let counters = self.persistence_counters()?;
        let assets = self.assets.usage();
        Ok(PersistenceInfo {
            format_version: inkpod_format::FORMAT_VERSION,
            open_strategy: self.last_open_strategy,
            journal_event_count: self.journal.len() as u64,
            procedure_count: counters.procedure_count,
            replay_work: counters.replay_work,
            dirty_bytes: counters.dirty_bytes,
            asset_count: assets.asset_count,
            asset_bytes: assets.logical_payload_bytes,
            checkpoint_due: counters.checkpoint_due(),
        })
    }

    /// Builds a side-effect-free preview token for an explicit compacted copy.
    ///
    /// The returned counts are the history that the separate output will omit.
    /// No automatic squash occurs and the live document, journal, path,
    /// savepoints, dirty state, and assets remain unchanged.
    pub fn compaction_plan(&self) -> Result<CompactionPlan, CoreError> {
        self.ensure_no_active_stroke()?;
        self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let records = encode_journal_records(&self.journal)?;
        Ok(CompactionPlan {
            history_event_count: self.journal.len() as u64,
            history_procedure_count: self
                .journal
                .iter()
                .filter(|entry| matches!(entry, JournalEntry::Commit(_)))
                .count() as u64,
            document_digest: self.document_state_digest()?,
            editor_digest: self
                .editor_session
                .as_ref()
                .ok_or(CoreError::NoDocument)?
                .digest,
            journal_digest: journal_prefix_digest(&records),
        })
    }

    /// Writes a separate current-version file whose current state is a new Genesis.
    ///
    /// `plan` must be the exact value most recently presented for confirmation;
    /// any intervening document, editor, or journal change is rejected as stale.
    /// Success never changes the live Core or adopts `path` as its save target.
    /// The compacted file intentionally has no prior Undo/Redo or inactive branch.
    pub fn write_compacted_copy(&self, path: &Path, plan: CompactionPlan) -> Result<(), CoreError> {
        let file = self.build_compacted_native_file(plan)?;
        inkpod_format::save_procedure_file_atomic(path, &file)?;
        Ok(())
    }

    pub(super) fn build_compacted_native_file(
        &self,
        plan: CompactionPlan,
    ) -> Result<inkpod_format::NativeFile, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.compaction_plan()? != plan {
            return Err(CoreError::InvalidState("compaction plan is stale"));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let editor = self
            .editor_session
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .clone();
        let mut compacted = Core::new();
        compacted.raster_file_format = self.raster_file_format;
        compacted.document = Some(document.clone());
        compacted.genesis = Some(genesis::Genesis::new(document));
        compacted.document_revision = DocumentRevision::from_raw(1);
        compacted.next_id = self.next_id;
        compacted.reset_history(true);
        compacted.editor_session = Some(EditorSessionState {
            state: editor.state,
            revision: editor.revision,
            digest: editor.digest,
            savepoint: Some(editor.digest),
        });
        compacted.assets = self.assets_for_current_document()?;
        compacted.reset_view();
        compacted.build_procedure_file(Some(StateId::GENESIS), Some(editor.digest))
    }
}

// Shared implementation helpers for this responsibility.

const ASSET_CHUNK_BYTES: usize = 4 * 1_024 * 1_024;
const MAX_PERSISTED_ARGUMENT_BYTES: usize = 1_024 * 1_024;
const MAX_PERSISTED_PAYLOAD_BYTES: usize = 32 * 1_024 * 1_024;
const MAX_PERSISTED_RECORD_BYTES: usize = 40 * 1_024 * 1_024;
const CHECKPOINT_PROCEDURE_INTERVAL: u64 = 256;
const CHECKPOINT_REPLAY_WORK_INTERVAL: u64 = 1_000_000;
const CHECKPOINT_DIRTY_BYTES_INTERVAL: u64 = 8 * 1_024 * 1_024;
const JOURNAL_PREFIX_CONTEXT: &str = "org.inkpod.digest.journal-prefix.v1";
const ASSET_CHUNK_CONTEXT: &str = "org.inkpod.digest.asset-chunk.v1";

#[derive(Clone, Copy)]
struct PersistenceCounters {
    procedure_count: u64,
    replay_work: u64,
    dirty_bytes: u64,
}

impl PersistenceCounters {
    const fn checkpoint_due(self) -> bool {
        self.procedure_count >= CHECKPOINT_PROCEDURE_INTERVAL
            || self.replay_work >= CHECKPOINT_REPLAY_WORK_INTERVAL
            || self.dirty_bytes >= CHECKPOINT_DIRTY_BYTES_INTERVAL
    }
}

#[derive(Clone, Copy)]
struct PersistentMeta {
    document_uuid: [u8; 16],
    current_state: StateId,
    history_cursor: usize,
    active_branch: BranchId,
    document_savepoint: Option<StateId>,
    editor_savepoint: Option<EditorStateDigest>,
    next_stable_id: u64,
    next_procedure: ProcedureId,
    next_state: StateId,
    next_event: JournalEventId,
    next_branch: BranchId,
    procedure_count: u64,
    event_count: u64,
    asset_count: u64,
    document_digest: DocumentStateDigest,
    editor_digest: EditorStateDigest,
    journal_digest: [u8; 32],
    raster_file_format: CommonRasterFormat,
}

struct DecodedCheckpoint {
    replay_epoch: u32,
    prefix_event_count: u64,
    prefix_procedure_count: u64,
    prefix_digest: [u8; 32],
    state_id: StateId,
    state_digest: DocumentStateDigest,
    next_stable_id: u64,
    active_branch: BranchId,
    history_cursor: usize,
    replay_work: u64,
    dirty_bytes: u64,
    document: CellDocument,
}

impl Core {
    fn persistence_counters(&self) -> Result<PersistenceCounters, CoreError> {
        let mut counters = PersistenceCounters {
            procedure_count: 0,
            replay_work: 0,
            dirty_bytes: 0,
        };
        for entry in &self.journal {
            let JournalEntry::Commit(commit) = entry else {
                continue;
            };
            let procedure = commit.procedure();
            counters.procedure_count =
                counters
                    .procedure_count
                    .checked_add(1)
                    .ok_or(CoreError::InvalidState(
                        "checkpoint procedure count overflows",
                    ))?;
            counters.replay_work = counters
                .replay_work
                .checked_add(1)
                .and_then(|value| value.checked_add(procedure.canonical_arguments().len() as u64))
                .and_then(|value| value.checked_add(procedure.canonical_payload().len() as u64))
                .ok_or(CoreError::InvalidState("checkpoint replay work overflows"))?;
            counters.dirty_bytes = counters
                .dirty_bytes
                .checked_add(procedure.canonical_arguments().len() as u64)
                .and_then(|value| value.checked_add(procedure.canonical_payload().len() as u64))
                .ok_or(CoreError::InvalidState("checkpoint dirty bytes overflow"))?;
            for id in procedure.asset_ids() {
                let record = self.assets.get(*id).ok_or(CoreError::InvalidState(
                    "checkpoint policy references a missing asset",
                ))?;
                let descriptor = record.descriptor();
                counters.replay_work = counters
                    .replay_work
                    .checked_add(descriptor.logical_element_count)
                    .ok_or(CoreError::InvalidState("checkpoint replay work overflows"))?;
                counters.dirty_bytes = counters
                    .dirty_bytes
                    .checked_add(descriptor.logical_payload_length)
                    .ok_or(CoreError::InvalidState("checkpoint dirty bytes overflow"))?;
            }
        }
        Ok(counters)
    }

    pub(super) fn build_procedure_file(
        &self,
        document_savepoint: Option<StateId>,
        editor_savepoint: Option<EditorStateDigest>,
    ) -> Result<inkpod_format::NativeFile, CoreError> {
        self.verify_journal_replay()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let genesis = self
            .genesis
            .as_ref()
            .ok_or(CoreError::InvalidState("journal Genesis is missing"))?;
        let editor = self.editor_session.as_ref().ok_or(CoreError::NoDocument)?;
        let contract = replay_contract();
        if contract.procedure_format_version() != inkpod_format::FORMAT_VERSION
            || contract.replay_epoch() != ReplayEpoch::CURRENT
        {
            return Err(CoreError::InvalidState(
                "native format and replay contract do not match",
            ));
        }

        let proc_records = encode_journal_records(&self.journal)?;
        let journal_digest = journal_prefix_digest(&proc_records);
        let counters = self.persistence_counters()?;
        let assets = self.assets.persistent_records();
        let asset_records = encode_asset_records(&assets)?;
        let document_digest = self.document_state_digest()?;
        let genesis_digest = primitive::canonical_document_state(&genesis.document)?.1;
        let editor_frame = self.editor_state_frame()?;

        let meta = PersistentMeta {
            document_uuid: document.uuid.to_le_bytes(),
            current_state: self.current_state,
            history_cursor: self.history_cursor,
            active_branch: self.active_branch,
            document_savepoint,
            editor_savepoint,
            next_stable_id: self.next_id.next_raw(),
            next_procedure: self.next_procedure,
            next_state: self.next_state,
            next_event: self.next_journal_event,
            next_branch: self.next_branch,
            procedure_count: counters.procedure_count,
            event_count: self.journal.len() as u64,
            asset_count: assets.len() as u64,
            document_digest,
            editor_digest: editor.digest,
            journal_digest,
            raster_file_format: self.raster_file_format,
        };
        let mut meta_record = record(1, encode_meta(meta));
        meta_record.schema_version = 2;
        let mut sections = vec![
            critical_section(*b"META", vec![meta_record]),
            critical_section(
                *b"GENS",
                vec![record(
                    1,
                    encode_genesis(&genesis.document, genesis_digest)?,
                )],
            ),
            critical_section(*b"ASST", asset_records),
            critical_section(*b"PROC", proc_records),
            critical_section(*b"EDIT", vec![record(1, editor_frame)]),
        ];
        if counters.checkpoint_due() {
            sections.push(inkpod_format::NativeSection {
                fourcc: *b"CKPT",
                schema_version: 1,
                flags: 0,
                records: vec![record(
                    1,
                    encode_checkpoint(document, document_digest, journal_digest, meta, counters)?,
                )],
            });
        }
        sections.extend(self.native_opaque_sections.iter().cloned());
        Ok(inkpod_format::NativeFile {
            primitive_catalog_digest: *contract.primitive_catalog_digest(),
            sections,
        })
    }

    pub(crate) fn from_procedure_file(file: inkpod_format::NativeFile) -> Result<Self, CoreError> {
        inkpod_format::validate_procedure_file(&file)?;
        let contract = replay_contract();
        if file.primitive_catalog_digest != *contract.primitive_catalog_digest() {
            return Err(format_error(
                "native primitive catalog digest does not match this build",
            ));
        }
        let meta_record = singleton_payload(&file.sections, *b"META")?;
        let meta = decode_meta(meta_record)?;
        let genesis_record = singleton_payload(&file.sections, *b"GENS")?;
        let (mut genesis_document, genesis_digest) = decode_genesis(genesis_record)?;
        if genesis_document.uuid.to_le_bytes() != meta.document_uuid {
            return Err(format_error("META and GENS document UUIDs differ"));
        }

        let asset_section = section(&file.sections, *b"ASST")?;
        let mut assets = decode_asset_records(&asset_section.records)?;
        if assets.usage().asset_count != meta.asset_count {
            return Err(format_error("META asset count does not match ASST"));
        }
        if let BaseSurface::Asset(id) = genesis_document.base_surface
            && assets.get(id).is_none()
        {
            return Err(format_error("Genesis base asset is missing"));
        }
        genesis_document.light_table.intern_into(&mut assets)?;
        let actual_genesis_digest = primitive::canonical_document_state(&genesis_document)?.1;
        if actual_genesis_digest != genesis_digest {
            return Err(format_error("GENS document-state digest does not match"));
        }

        let proc_section = section(&file.sections, *b"PROC")?;
        let journal = decode_journal_records(&proc_section.records, &assets)?;
        if journal.len() as u64 != meta.event_count
            || journal
                .iter()
                .filter(|entry| matches!(entry, JournalEntry::Commit(_)))
                .count() as u64
                != meta.procedure_count
            || journal_prefix_digest(&proc_section.records) != meta.journal_digest
        {
            return Err(format_error(
                "META journal counts or digest do not match PROC",
            ));
        }
        let mut checkpoint = file
            .sections
            .iter()
            .find(|section| section.fourcc == *b"CKPT")
            .map(|section| {
                if section.records.len() != 1 {
                    return Err(format_error("CKPT record count is invalid"));
                }
                decode_checkpoint(&section.records[0].payload)
            })
            .transpose()?;

        let mut staged = Core::new();
        staged.raster_file_format = meta.raster_file_format;
        staged.assets = assets;
        staged.document = Some(genesis_document.clone());
        staged.genesis = Some(genesis::Genesis::new(genesis_document));
        staged.document_revision = DocumentRevision::from_raw(1);
        staged.journal = journal;
        staged.current_state = meta.current_state;
        staged.savepoint = meta.document_savepoint;
        staged.active_branch = meta.active_branch;
        staged.next_id = StableIdCursor::from_next_raw(meta.next_stable_id);
        staged.next_procedure = meta.next_procedure;
        staged.next_state = meta.next_state;
        staged.next_journal_event = meta.next_event;
        staged.next_branch = meta.next_branch;
        staged.branch_tails = branch_tails_from_journal(&staged.journal, meta.next_branch)?;
        if let Some(checkpoint) = checkpoint.as_mut() {
            checkpoint
                .document
                .light_table
                .intern_into(&mut staged.assets)?;
            if let BaseSurface::Asset(id) = checkpoint.document.base_surface
                && staged.assets.get(id).is_none()
            {
                return Err(format_error("checkpoint base asset is missing"));
            }
        }
        let counters = staged.persistence_counters()?;
        let checkpoint_matches = checkpoint.as_ref().is_some_and(|checkpoint| {
            let digest = primitive::canonical_document_state(&checkpoint.document)
                .map(|value| value.1)
                .ok();
            checkpoint.replay_epoch == ReplayEpoch::CURRENT.get()
                && checkpoint.prefix_event_count == meta.event_count
                && checkpoint.prefix_procedure_count == meta.procedure_count
                && checkpoint.prefix_digest == meta.journal_digest
                && checkpoint.state_id == meta.current_state
                && checkpoint.state_digest == meta.document_digest
                && digest == Some(checkpoint.state_digest)
                && checkpoint.next_stable_id == meta.next_stable_id
                && checkpoint.active_branch == meta.active_branch
                && checkpoint.history_cursor == meta.history_cursor
                && checkpoint.replay_work == counters.replay_work
                && checkpoint.dirty_bytes == counters.dirty_bytes
                && checkpoint.document.uuid.to_le_bytes() == meta.document_uuid
        });
        let rebuilt = if checkpoint_matches {
            let checkpoint = checkpoint.expect("matching checkpoint is present");
            let rebuilt = staged.rebuild_runtime_from_checkpoint(
                checkpoint.document,
                StableIdCursor::from_next_raw(checkpoint.next_stable_id),
            )?;
            staged.last_open_strategy = NativeOpenStrategy::Checkpoint;
            rebuilt
        } else {
            let rebuilt = staged.rebuild_runtime_from_journal()?;
            staged.last_open_strategy = NativeOpenStrategy::FullReplay;
            rebuilt
        };
        if rebuilt.next_id.next_raw() != meta.next_stable_id
            || rebuilt.history_cursor != meta.history_cursor
            || rebuilt.info.document_state_digest() != meta.document_digest
        {
            return Err(format_error(
                "META high-watermark, cursor, or document digest does not match replay",
            ));
        }
        staged.document = Some(rebuilt.document);
        staged.history = rebuilt.history;
        staged.history_cursor = rebuilt.history_cursor;
        staged.next_id = rebuilt.next_id;
        staged.canonical_state_cache.get_mut().take();
        staged.render_cache.clear();

        let edit_record = singleton_payload(&file.sections, *b"EDIT")?;
        let editor = editor::codec::decode_edit_frame(edit_record)?;
        if editor.digest != meta.editor_digest {
            return Err(format_error("META editor digest does not match EDIT"));
        }
        let target = editor.state.target.ok_or(CoreError::Format(
            "persisted EditorState target is absent".to_owned(),
        ))?;
        staged.validate_editor_target(target)?;
        staged.editor_session = Some(EditorSessionState {
            state: editor.state,
            revision: editor.revision,
            digest: editor.digest,
            savepoint: meta.editor_savepoint,
        });
        staged.native_opaque_sections = file
            .sections
            .into_iter()
            .filter(|section| section.flags == inkpod_format::OPAQUE_PRESERVE)
            .collect();
        staged.current_path = None;
        staged.recovered = false;
        staged.reset_view();
        Ok(staged)
    }

    pub(super) fn staged_saved_document(&self, path: &Path) -> Result<CellDocument, CoreError> {
        let file = inkpod_format::read_procedure_file(path)?;
        Self::from_procedure_file(file)?
            .document
            .ok_or(CoreError::NoDocument)
    }
}

fn critical_section(
    fourcc: [u8; 4],
    records: Vec<inkpod_format::NativeRecord>,
) -> inkpod_format::NativeSection {
    inkpod_format::NativeSection {
        fourcc,
        schema_version: if fourcc == *b"META" { 2 } else { 1 },
        flags: inkpod_format::SECTION_CRITICAL,
        records,
    }
}

fn record(kind: u16, payload: Vec<u8>) -> inkpod_format::NativeRecord {
    inkpod_format::NativeRecord {
        kind,
        schema_version: 1,
        flags: 0,
        payload,
    }
}

fn section(
    sections: &[inkpod_format::NativeSection],
    fourcc: [u8; 4],
) -> Result<&inkpod_format::NativeSection, CoreError> {
    sections
        .iter()
        .find(|section| section.fourcc == fourcc)
        .ok_or_else(|| format_error("required native section is missing"))
}

fn singleton_payload(
    sections: &[inkpod_format::NativeSection],
    fourcc: [u8; 4],
) -> Result<&[u8], CoreError> {
    let section = section(sections, fourcc)?;
    if section.records.len() != 1 {
        return Err(format_error("native singleton section count is invalid"));
    }
    Ok(&section.records[0].payload)
}

fn encode_meta(meta: PersistentMeta) -> Vec<u8> {
    encode_frame(&[
        Some(meta.document_uuid.to_vec()),
        Some(ReplayEpoch::CURRENT.get().to_le_bytes().to_vec()),
        Some(replay_contract().primitive_catalog_digest().to_vec()),
        Some(meta.current_state.get().to_le_bytes().to_vec()),
        Some((meta.history_cursor as u64).to_le_bytes().to_vec()),
        Some(meta.active_branch.get().to_le_bytes().to_vec()),
        meta.document_savepoint
            .map(|value| value.get().to_le_bytes().to_vec()),
        meta.editor_savepoint.map(|value| value.as_bytes().to_vec()),
        Some(meta.next_stable_id.to_le_bytes().to_vec()),
        Some(meta.next_procedure.get().to_le_bytes().to_vec()),
        Some(meta.next_state.get().to_le_bytes().to_vec()),
        Some(meta.next_event.get().to_le_bytes().to_vec()),
        Some(meta.next_branch.get().to_le_bytes().to_vec()),
        Some(meta.procedure_count.to_le_bytes().to_vec()),
        Some(meta.event_count.to_le_bytes().to_vec()),
        Some(meta.asset_count.to_le_bytes().to_vec()),
        Some(1_u64.to_le_bytes().to_vec()),
        Some(meta.document_digest.as_bytes().to_vec()),
        Some(meta.editor_digest.as_bytes().to_vec()),
        Some(meta.journal_digest.to_vec()),
        Some(
            raster_format_code(meta.raster_file_format)
                .to_le_bytes()
                .to_vec(),
        ),
    ])
}

fn decode_meta(bytes: &[u8]) -> Result<PersistentMeta, CoreError> {
    let fields = decode_frame(bytes, 21)?;
    let document_uuid = fixed::<16>(required(fields[0])?, "META document UUID")?;
    if read_u32(required(fields[1])?)? != ReplayEpoch::CURRENT.get()
        || fixed::<32>(required(fields[2])?, "META catalog digest")?
            != *replay_contract().primitive_catalog_digest()
        || read_u64(required(fields[16])?)? != 1
    {
        return Err(format_error(
            "META replay contract or editor count is invalid",
        ));
    }
    let editor_savepoint = fields[7]
        .map(|value| fixed::<32>(value, "META editor savepoint").map(EditorStateDigest))
        .transpose()?;
    let history_cursor = usize::try_from(read_u64(required(fields[4])?)?)
        .map_err(|_| format_error("META history cursor is not addressable"))?;
    Ok(PersistentMeta {
        document_uuid,
        current_state: state_id(required(fields[3])?)?,
        history_cursor,
        active_branch: branch_id(required(fields[5])?)?,
        document_savepoint: fields[6].map(state_id).transpose()?,
        editor_savepoint,
        next_stable_id: persistent_id(required(fields[8])?)?,
        next_procedure: ProcedureId::from_raw(persistent_id(required(fields[9])?)?),
        next_state: StateId::from_raw(persistent_id(required(fields[10])?)?),
        next_event: JournalEventId::from_raw(persistent_id(required(fields[11])?)?),
        next_branch: BranchId::from_raw(persistent_id(required(fields[12])?)?),
        procedure_count: read_u64(required(fields[13])?)?,
        event_count: read_u64(required(fields[14])?)?,
        asset_count: read_u64(required(fields[15])?)?,
        document_digest: DocumentStateDigest::from_bytes(fixed::<32>(
            required(fields[17])?,
            "META document digest",
        )?),
        editor_digest: EditorStateDigest(fixed::<32>(required(fields[18])?, "META editor digest")?),
        journal_digest: fixed::<32>(required(fields[19])?, "META journal digest")?,
        raster_file_format: raster_format_from_code(read_u32(required(fields[20])?)?)?,
    })
}

fn raster_format_code(format: CommonRasterFormat) -> u32 {
    match format {
        CommonRasterFormat::Png => 1,
        CommonRasterFormat::Tiff => 2,
        CommonRasterFormat::Tga => 3,
        CommonRasterFormat::Bmp => 4,
    }
}

fn raster_format_from_code(code: u32) -> Result<CommonRasterFormat, CoreError> {
    match code {
        1 => Ok(CommonRasterFormat::Png),
        2 => Ok(CommonRasterFormat::Tiff),
        3 => Ok(CommonRasterFormat::Tga),
        4 => Ok(CommonRasterFormat::Bmp),
        _ => Err(format_error("META raster file format is unknown")),
    }
}

fn encode_genesis(
    document: &CellDocument,
    digest: DocumentStateDigest,
) -> Result<Vec<u8>, CoreError> {
    let archive = encode_document_archive(document)?;
    Ok(encode_frame(&[
        Some(document.uuid.to_le_bytes().to_vec()),
        Some(StateId::GENESIS.get().to_le_bytes().to_vec()),
        Some(BranchId::ROOT.get().to_le_bytes().to_vec()),
        Some(archive),
        Some(digest.as_bytes().to_vec()),
    ]))
}

fn encode_document_archive(document: &CellDocument) -> Result<Vec<u8>, CoreError> {
    let mut archive = Vec::new();
    match document.base_surface {
        BaseSurface::SolidWhite => archive.push(1),
        BaseSurface::Asset(id) => {
            archive.push(2);
            archive.extend_from_slice(id.as_bytes());
        }
    }
    let cell_bytes = inkpod_format::encode_document_archive(&document.to_archive())?;
    archive.extend_from_slice(&(cell_bytes.len() as u64).to_le_bytes());
    archive.extend_from_slice(&cell_bytes);
    Ok(archive)
}

fn decode_genesis(bytes: &[u8]) -> Result<(CellDocument, DocumentStateDigest), CoreError> {
    let fields = decode_frame(bytes, 5)?;
    let uuid = fixed::<16>(required(fields[0])?, "GENS UUID")?;
    if read_u64(required(fields[1])?)? != StateId::GENESIS.get()
        || read_u64(required(fields[2])?)? != BranchId::ROOT.get()
    {
        return Err(format_error("GENS root identities are invalid"));
    }
    let document = decode_document_archive(required(fields[3])?)?;
    if document.uuid.to_le_bytes() != uuid {
        return Err(format_error("GENS UUID and document payload differ"));
    }
    let digest =
        DocumentStateDigest::from_bytes(fixed::<32>(required(fields[4])?, "GENS document digest")?);
    Ok((document, digest))
}

fn decode_document_archive(archive: &[u8]) -> Result<CellDocument, CoreError> {
    let mut reader = ByteReader::new(archive);
    let base_surface = match reader.u8()? {
        1 => BaseSurface::SolidWhite,
        2 => BaseSurface::Asset(AssetId::from_bytes(reader.fixed::<32>()?)),
        _ => return Err(format_error("GENS base-surface code is invalid")),
    };
    let length = reader.length()?;
    let cell = inkpod_format::decode_document_archive(reader.take(length)?)?;
    reader.finish()?;
    let mut document = CellDocument::from_archive(cell, DocumentRevision::from_raw(1))?;
    document.base_surface = base_surface;
    Ok(document)
}

fn encode_checkpoint(
    document: &CellDocument,
    state_digest: DocumentStateDigest,
    prefix_digest: [u8; 32],
    meta: PersistentMeta,
    counters: PersistenceCounters,
) -> Result<Vec<u8>, CoreError> {
    Ok(encode_frame(&[
        Some(ReplayEpoch::CURRENT.get().to_le_bytes().to_vec()),
        Some(meta.event_count.to_le_bytes().to_vec()),
        Some(meta.procedure_count.to_le_bytes().to_vec()),
        Some(prefix_digest.to_vec()),
        Some(meta.current_state.get().to_le_bytes().to_vec()),
        Some(state_digest.as_bytes().to_vec()),
        Some(meta.next_stable_id.to_le_bytes().to_vec()),
        Some(meta.active_branch.get().to_le_bytes().to_vec()),
        Some((meta.history_cursor as u64).to_le_bytes().to_vec()),
        Some(counters.replay_work.to_le_bytes().to_vec()),
        Some(counters.dirty_bytes.to_le_bytes().to_vec()),
        Some(encode_document_archive(document)?),
    ]))
}

fn decode_checkpoint(bytes: &[u8]) -> Result<DecodedCheckpoint, CoreError> {
    let fields = decode_frame(bytes, 12)?;
    let history_cursor = usize::try_from(read_u64(required(fields[8])?)?)
        .map_err(|_| format_error("CKPT history cursor is not addressable"))?;
    Ok(DecodedCheckpoint {
        replay_epoch: read_u32(required(fields[0])?)?,
        prefix_event_count: read_u64(required(fields[1])?)?,
        prefix_procedure_count: read_u64(required(fields[2])?)?,
        prefix_digest: fixed::<32>(required(fields[3])?, "CKPT prefix digest")?,
        state_id: state_id(required(fields[4])?)?,
        state_digest: DocumentStateDigest::from_bytes(fixed::<32>(
            required(fields[5])?,
            "CKPT state digest",
        )?),
        next_stable_id: persistent_id(required(fields[6])?)?,
        active_branch: branch_id(required(fields[7])?)?,
        history_cursor,
        replay_work: read_u64(required(fields[9])?)?,
        dirty_bytes: read_u64(required(fields[10])?)?,
        document: decode_document_archive(required(fields[11])?)?,
    })
}

fn encode_asset_records(
    assets: &[(AssetId, AssetDescriptor, &[u8])],
) -> Result<Vec<inkpod_format::NativeRecord>, CoreError> {
    let mut records = Vec::new();
    for (id, descriptor, payload) in assets {
        if descriptor.logical_payload_length != payload.len() as u64 {
            return Err(format_error(
                "asset descriptor payload length is inconsistent",
            ));
        }
        let chunks = payload.chunks(ASSET_CHUNK_BYTES).collect::<Vec<_>>();
        let mut descriptor_bytes = Vec::new();
        descriptor_bytes.extend_from_slice(id.as_bytes());
        descriptor_bytes.extend_from_slice(&asset_kind_code(descriptor.kind).to_le_bytes());
        descriptor_bytes.extend_from_slice(
            &descriptor
                .pixel_format
                .map_or(0, pixel_format_code)
                .to_le_bytes(),
        );
        descriptor_bytes.extend_from_slice(
            &descriptor
                .color_space
                .map_or(0, asset_color_space_code)
                .to_le_bytes(),
        );
        descriptor_bytes.extend_from_slice(
            &descriptor
                .alpha_semantics
                .map_or(0, asset_alpha_code)
                .to_le_bytes(),
        );
        descriptor_bytes.extend_from_slice(&descriptor.width.unwrap_or(0).to_le_bytes());
        descriptor_bytes.extend_from_slice(&descriptor.height.unwrap_or(0).to_le_bytes());
        descriptor_bytes.extend_from_slice(&descriptor.canonical_stride.unwrap_or(0).to_le_bytes());
        descriptor_bytes.extend_from_slice(&descriptor.logical_element_count.to_le_bytes());
        descriptor_bytes.extend_from_slice(&descriptor.logical_payload_length.to_le_bytes());
        descriptor_bytes.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
        descriptor_bytes.extend_from_slice(&(ASSET_CHUNK_BYTES as u32).to_le_bytes());
        records.push(record(1, descriptor_bytes));
        let mut offset = 0_u64;
        for (index, chunk) in chunks.into_iter().enumerate() {
            let digest = asset_chunk_digest(*id, index as u32, offset, chunk);
            let mut bytes = Vec::new();
            bytes.extend_from_slice(id.as_bytes());
            bytes.extend_from_slice(&(index as u32).to_le_bytes());
            bytes.extend_from_slice(&0_u32.to_le_bytes());
            bytes.extend_from_slice(&offset.to_le_bytes());
            bytes.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&0_u32.to_le_bytes());
            bytes.extend_from_slice(&digest);
            bytes.extend_from_slice(chunk);
            records.push(record(2, bytes));
            offset = offset
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| format_error("asset chunk offset overflows"))?;
        }
    }
    Ok(records)
}

fn decode_asset_records(
    records: &[inkpod_format::NativeRecord],
) -> Result<asset::AssetStore, CoreError> {
    let mut store = asset::AssetStore::default();
    let mut index = 0_usize;
    let mut previous_id = None;
    while index < records.len() {
        let descriptor_record = &records[index];
        if descriptor_record.kind != 1 {
            return Err(format_error("ASST descriptor ordering is invalid"));
        }
        let mut reader = ByteReader::new(&descriptor_record.payload);
        let id = AssetId::from_bytes(reader.fixed::<32>()?);
        if previous_id.is_some_and(|previous| previous >= id) {
            return Err(format_error(
                "ASST asset identities are not strictly ordered",
            ));
        }
        previous_id = Some(id);
        let descriptor = AssetDescriptor {
            kind: decode_asset_kind(reader.u32()?)?,
            pixel_format: decode_optional_pixel_format(reader.u32()?)?,
            color_space: decode_optional_asset_color_space(reader.u32()?)?,
            alpha_semantics: decode_optional_asset_alpha(reader.u32()?)?,
            width: nonzero_optional(reader.u32()?),
            height: nonzero_optional(reader.u32()?),
            canonical_stride: nonzero_optional(reader.u64()?),
            logical_element_count: reader.u64()?,
            logical_payload_length: reader.u64()?,
        };
        let chunk_count = reader.u32()? as usize;
        if reader.u32()? != ASSET_CHUNK_BYTES as u32 {
            return Err(format_error("ASST chunk size is unsupported"));
        }
        reader.finish()?;
        index += 1;
        let capacity = usize::try_from(descriptor.logical_payload_length)
            .map_err(|_| format_error("ASST payload is not addressable"))?;
        let mut payload = Vec::new();
        payload
            .try_reserve_exact(capacity)
            .map_err(|_| format_error("ASST payload allocation failed"))?;
        for chunk_index in 0..chunk_count {
            let chunk_record = records
                .get(index)
                .ok_or_else(|| format_error("ASST chunk record is missing"))?;
            if chunk_record.kind != 2 {
                return Err(format_error("ASST chunk ordering is invalid"));
            }
            let mut chunk = ByteReader::new(&chunk_record.payload);
            if AssetId::from_bytes(chunk.fixed::<32>()?) != id
                || chunk.u32()? as usize != chunk_index
                || chunk.u32()? != 0
                || chunk.u64()? != payload.len() as u64
            {
                return Err(format_error("ASST chunk identity or offset is invalid"));
            }
            let length = chunk.u32()? as usize;
            if chunk.u32()? != 0 {
                return Err(format_error("ASST chunk reserved field is nonzero"));
            }
            let digest = chunk.fixed::<32>()?;
            let bytes = chunk.take(length)?;
            chunk.finish()?;
            if length == 0
                || length > ASSET_CHUNK_BYTES
                || (chunk_index + 1 != chunk_count && length != ASSET_CHUNK_BYTES)
                || asset_chunk_digest(id, chunk_index as u32, payload.len() as u64, bytes) != digest
            {
                return Err(format_error("ASST chunk length or digest is invalid"));
            }
            payload.extend_from_slice(bytes);
            index += 1;
        }
        if payload.len() as u64 != descriptor.logical_payload_length {
            return Err(format_error(
                "ASST payload length does not match descriptor",
            ));
        }
        store.ingest_persistent(id, descriptor, payload)?;
    }
    Ok(store)
}

fn encode_journal_records(
    journal: &[JournalEntry],
) -> Result<Vec<inkpod_format::NativeRecord>, CoreError> {
    journal
        .iter()
        .map(|entry| match entry {
            JournalEntry::Commit(commit) => encode_commit(commit),
            JournalEntry::HistoryMove(movement) => {
                let mut bytes = Vec::with_capacity(40);
                bytes.extend_from_slice(&movement.event_id().get().to_le_bytes());
                bytes.push(movement.kind() as u8);
                bytes.extend_from_slice(&[0; 7]);
                bytes.extend_from_slice(&movement.source_state_id().get().to_le_bytes());
                bytes.extend_from_slice(&movement.destination_state_id().get().to_le_bytes());
                bytes.extend_from_slice(&movement.active_branch_id().get().to_le_bytes());
                Ok(record(2, bytes))
            }
            JournalEntry::BranchCut(cut) => {
                let mut bytes = Vec::with_capacity(40);
                bytes.extend_from_slice(&cut.event_id().get().to_le_bytes());
                bytes.extend_from_slice(&cut.fork_state_id().get().to_le_bytes());
                bytes.extend_from_slice(&cut.old_active_tail_state_id().get().to_le_bytes());
                bytes.extend_from_slice(&cut.new_branch_id().get().to_le_bytes());
                bytes.extend_from_slice(&cut.deactivated_branch_id().get().to_le_bytes());
                Ok(record(3, bytes))
            }
        })
        .collect()
}

fn encode_commit(commit: &JournalCommit) -> Result<inkpod_format::NativeRecord, CoreError> {
    let procedure = commit.procedure();
    let arguments = procedure.canonical_arguments();
    let payload = procedure.canonical_payload();
    if arguments.len() > MAX_PERSISTED_ARGUMENT_BYTES || payload.len() > MAX_PERSISTED_PAYLOAD_BYTES
    {
        return Err(format_error("PROC argument or payload exceeds limit"));
    }
    let argument_count = u32::from(!arguments.is_empty());
    let argument_bytes = if arguments.is_empty() {
        0
    } else {
        16_u64 + arguments.len() as u64
    };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&commit.event_id().get().to_le_bytes());
    bytes.extend_from_slice(&procedure.procedure_id().get().to_le_bytes());
    bytes.extend_from_slice(&procedure.primitive_id().get().to_le_bytes());
    bytes.extend_from_slice(&procedure.primitive_schema_version().to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&procedure.replay_epoch().get().to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&commit.parent_state_id().get().to_le_bytes());
    bytes.extend_from_slice(&commit.committed_state_id().get().to_le_bytes());
    bytes.extend_from_slice(&commit.branch_id().get().to_le_bytes());
    bytes.extend_from_slice(&argument_count.to_le_bytes());
    bytes.extend_from_slice(&(procedure.asset_ids().len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(procedure.input_ids().len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(procedure.output_ids().len() as u32).to_le_bytes());
    bytes.extend_from_slice(&argument_bytes.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(procedure.pre_state_digest().as_bytes());
    bytes.extend_from_slice(procedure.post_state_digest().as_bytes());
    bytes.extend_from_slice(procedure.canonical_payload_digest());
    if !arguments.is_empty() {
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&7_u16.to_le_bytes());
        bytes.push(1);
        bytes.push(0);
        bytes.extend_from_slice(&(arguments.len() as u64).to_le_bytes());
        bytes.extend_from_slice(arguments);
    }
    for (index, id) in procedure.asset_ids().iter().enumerate() {
        bytes.extend_from_slice(&((index + 1) as u32).to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(id.as_bytes());
    }
    for (index, id) in procedure.input_ids().iter().enumerate() {
        bytes.extend_from_slice(&((index + 1) as u32).to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    for (index, id) in procedure.output_ids().iter().enumerate() {
        bytes.extend_from_slice(&((index + 1) as u32).to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    bytes.extend_from_slice(payload);
    if bytes.len() > MAX_PERSISTED_RECORD_BYTES {
        return Err(format_error("PROC record exceeds limit"));
    }
    Ok(record(1, bytes))
}

fn decode_journal_records(
    records: &[inkpod_format::NativeRecord],
    assets: &asset::AssetStore,
) -> Result<Vec<JournalEntry>, CoreError> {
    let mut journal = Vec::new();
    journal
        .try_reserve_exact(records.len())
        .map_err(|_| format_error("PROC journal allocation failed"))?;
    for record in records {
        journal.push(match record.kind {
            1 => JournalEntry::Commit(decode_commit(&record.payload, assets)?),
            2 => {
                let mut reader = ByteReader::new(&record.payload);
                let event = JournalEventId::from_raw(reader.persistent_id()?);
                let kind = match reader.u8()? {
                    1 => HistoryMoveKind::Undo,
                    2 => HistoryMoveKind::Redo,
                    3 => HistoryMoveKind::Jump,
                    _ => return Err(format_error("PROC HistoryMove kind is invalid")),
                };
                if reader.take(7)? != [0; 7] {
                    return Err(format_error("PROC HistoryMove reserved bytes are nonzero"));
                }
                let source = StateId::from_raw(reader.persistent_id()?);
                let destination = StateId::from_raw(reader.persistent_id()?);
                let branch = BranchId::from_raw(reader.persistent_id()?);
                reader.finish()?;
                JournalEntry::HistoryMove(JournalHistoryMove::from_persistent(
                    event,
                    kind,
                    source,
                    destination,
                    branch,
                ))
            }
            3 => {
                let mut reader = ByteReader::new(&record.payload);
                let value = JournalBranchCut::from_persistent(
                    JournalEventId::from_raw(reader.persistent_id()?),
                    StateId::from_raw(reader.persistent_id()?),
                    StateId::from_raw(reader.persistent_id()?),
                    BranchId::from_raw(reader.persistent_id()?),
                    BranchId::from_raw(reader.persistent_id()?),
                );
                reader.finish()?;
                JournalEntry::BranchCut(value)
            }
            _ => return Err(format_error("PROC record kind is unsupported")),
        });
    }
    Ok(journal)
}

fn decode_commit(bytes: &[u8], assets: &asset::AssetStore) -> Result<JournalCommit, CoreError> {
    if bytes.len() > MAX_PERSISTED_RECORD_BYTES {
        return Err(format_error("PROC record exceeds limit"));
    }
    let mut reader = ByteReader::new(bytes);
    let event_id = JournalEventId::from_raw(reader.persistent_id()?);
    let procedure_id = ProcedureId::from_raw(reader.persistent_id()?);
    let primitive_id = PrimitiveId::from_raw(reader.u32()?);
    let schema_version = reader.u16()?;
    if reader.u16()? != 0 || reader.u32()? != ReplayEpoch::CURRENT.get() || reader.u32()? != 0 {
        return Err(format_error(
            "PROC Commit schema or replay epoch is invalid",
        ));
    }
    let parent = StateId::from_raw(reader.persistent_id()?);
    let committed = StateId::from_raw(reader.persistent_id()?);
    let branch = BranchId::from_raw(reader.persistent_id()?);
    let argument_count = reader.u32()? as usize;
    let asset_count = reader.u32()? as usize;
    let input_count = reader.u32()? as usize;
    let output_count = reader.u32()? as usize;
    let argument_bytes = reader.length()?;
    let payload_length = reader.length()?;
    if argument_bytes > MAX_PERSISTED_ARGUMENT_BYTES + 16
        || payload_length > MAX_PERSISTED_PAYLOAD_BYTES
    {
        return Err(format_error("PROC argument or payload exceeds limit"));
    }
    let pre_digest = DocumentStateDigest::from_bytes(reader.fixed::<32>()?);
    let post_digest = DocumentStateDigest::from_bytes(reader.fixed::<32>()?);
    let payload_digest = reader.fixed::<32>()?;
    let arguments = match argument_count {
        0 if argument_bytes == 0 => Vec::new(),
        1 => {
            let start = reader.position();
            if reader.u32()? != 1 || reader.u16()? != 7 || reader.u8()? != 1 || reader.u8()? != 0 {
                return Err(format_error("PROC canonical argument record is invalid"));
            }
            let length = reader.length()?;
            let value = reader.take(length)?.to_vec();
            if reader.position() - start != argument_bytes {
                return Err(format_error(
                    "PROC canonical argument length is inconsistent",
                ));
            }
            value
        }
        _ => return Err(format_error("PROC canonical argument count is invalid")),
    };
    let mut asset_ids = Vec::with_capacity(asset_count);
    for ordinal in 1..=asset_count {
        if reader.u32()? != ordinal as u32 || reader.u32()? != 0 {
            return Err(format_error("PROC asset role ordering is invalid"));
        }
        asset_ids.push(AssetId::from_bytes(reader.fixed::<32>()?));
    }
    let input_ids = decode_id_roles(&mut reader, input_count)?;
    let output_ids = decode_id_roles(&mut reader, output_count)?;
    let payload = reader.take(payload_length)?.to_vec();
    reader.finish()?;
    let runtime_invocation = primitive::RuntimeInvocation::from_persistent(
        primitive_id,
        schema_version,
        &arguments,
        assets,
    )?;
    let procedure = Arc::new(CanonicalProcedure {
        procedure_id,
        primitive_id,
        primitive_schema_version: schema_version,
        replay_epoch: ReplayEpoch::CURRENT,
        base_state_id: parent,
        committed_state_id: committed,
        input_ids,
        output_ids,
        asset_ids,
        canonical_arguments: arguments,
        canonical_payload: payload,
        canonical_payload_digest: payload_digest,
        pre_state_digest: pre_digest,
        post_state_digest: post_digest,
        runtime_invocation,
    });
    Ok(JournalCommit::from_persistent(
        event_id, procedure, parent, committed, branch,
    ))
}

fn decode_id_roles(reader: &mut ByteReader<'_>, count: usize) -> Result<Vec<u64>, CoreError> {
    let mut ids = Vec::with_capacity(count);
    for ordinal in 1..=count {
        if reader.u32()? != ordinal as u32 {
            return Err(format_error("PROC stable-ID role ordering is invalid"));
        }
        reader.u32()?;
        ids.push(reader.persistent_id()?);
    }
    Ok(ids)
}

fn branch_tails_from_journal(
    journal: &[JournalEntry],
    next_branch: BranchId,
) -> Result<Vec<StateId>, CoreError> {
    let count = usize::try_from(next_branch.get())
        .ok()
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| format_error("branch high-watermark is not addressable"))?;
    let mut tails = vec![StateId::GENESIS; count];
    for entry in journal {
        match entry {
            JournalEntry::Commit(commit) => {
                let index = usize::try_from(commit.branch_id().get() - 1)
                    .map_err(|_| format_error("branch ID is not addressable"))?;
                *tails
                    .get_mut(index)
                    .ok_or_else(|| format_error("Commit branch exceeds high-watermark"))? =
                    commit.committed_state_id();
            }
            JournalEntry::BranchCut(cut) => {
                let index = usize::try_from(cut.new_branch_id().get() - 1)
                    .map_err(|_| format_error("branch ID is not addressable"))?;
                *tails
                    .get_mut(index)
                    .ok_or_else(|| format_error("BranchCut exceeds high-watermark"))? =
                    cut.fork_state_id();
            }
            JournalEntry::HistoryMove(_) => {}
        }
    }
    Ok(tails)
}

fn journal_prefix_digest(records: &[inkpod_format::NativeRecord]) -> [u8; 32] {
    let mut sequence = Vec::new();
    sequence.extend_from_slice(&(records.len() as u64).to_le_bytes());
    for record in records {
        let bytes = native_record_bytes(record);
        sequence.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        sequence.extend_from_slice(&bytes);
    }
    let mut hasher = blake3::Hasher::new_derive_key(JOURNAL_PREFIX_CONTEXT);
    hasher.update(&1_u32.to_le_bytes());
    hasher.update(&2_u32.to_le_bytes());
    hash_field(&mut hasher, 1, &(records.len() as u64).to_le_bytes());
    hash_field(&mut hasher, 2, &sequence);
    *hasher.finalize().as_bytes()
}

fn native_record_bytes(record: &inkpod_format::NativeRecord) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&record.kind.to_le_bytes());
    bytes.extend_from_slice(&record.schema_version.to_le_bytes());
    bytes.extend_from_slice(&record.flags.to_le_bytes());
    bytes.extend_from_slice(&(record.payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&record.payload);
    bytes
}

fn asset_chunk_digest(id: AssetId, index: u32, offset: u64, bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(ASSET_CHUNK_CONTEXT);
    hasher.update(&1_u32.to_le_bytes());
    hasher.update(&4_u32.to_le_bytes());
    hash_field(&mut hasher, 1, id.as_bytes());
    hash_field(&mut hasher, 2, &index.to_le_bytes());
    hash_field(&mut hasher, 3, &offset.to_le_bytes());
    hash_field(&mut hasher, 4, bytes);
    *hasher.finalize().as_bytes()
}

fn hash_field(hasher: &mut blake3::Hasher, ordinal: u32, bytes: &[u8]) {
    hasher.update(&ordinal.to_le_bytes());
    hasher.update(&[1, 0, 0, 0]);
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn encode_frame(fields: &[Option<Vec<u8>>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    for (index, field) in fields.iter().enumerate() {
        bytes.extend_from_slice(&((index + 1) as u32).to_le_bytes());
        bytes.push(u8::from(field.is_some()));
        bytes.extend_from_slice(&[0; 3]);
        bytes.extend_from_slice(&(field.as_ref().map_or(0, Vec::len) as u64).to_le_bytes());
        if let Some(field) = field {
            bytes.extend_from_slice(field);
        }
    }
    bytes
}

fn decode_frame(bytes: &[u8], count: usize) -> Result<Vec<Option<&[u8]>>, CoreError> {
    let mut reader = ByteReader::new(bytes);
    if reader.u32()? != 1 || reader.u32()? != count as u32 {
        return Err(format_error("canonical frame prefix is invalid"));
    }
    let mut fields = Vec::with_capacity(count);
    for ordinal in 1..=count {
        if reader.u32()? != ordinal as u32 {
            return Err(format_error("canonical frame ordinal is invalid"));
        }
        let present = reader.u8()?;
        if reader.take(3)? != [0; 3] {
            return Err(format_error("canonical frame reserved bytes are nonzero"));
        }
        let length = reader.length()?;
        fields.push(match (present, length) {
            (0, 0) => None,
            (1, length) => Some(reader.take(length)?),
            _ => return Err(format_error("canonical frame presence is invalid")),
        });
    }
    reader.finish()?;
    Ok(fields)
}

fn required(field: Option<&[u8]>) -> Result<&[u8], CoreError> {
    field.ok_or_else(|| format_error("required canonical frame field is absent"))
}

fn fixed<const N: usize>(bytes: &[u8], label: &'static str) -> Result<[u8; N], CoreError> {
    bytes
        .try_into()
        .map_err(|_| CoreError::Format(format!("{label} has the wrong length")))
}

fn read_u32(bytes: &[u8]) -> Result<u32, CoreError> {
    Ok(u32::from_le_bytes(fixed::<4>(bytes, "u32 field")?))
}

fn read_u64(bytes: &[u8]) -> Result<u64, CoreError> {
    Ok(u64::from_le_bytes(fixed::<8>(bytes, "u64 field")?))
}

fn persistent_id(bytes: &[u8]) -> Result<u64, CoreError> {
    let value = read_u64(bytes)?;
    if value == 0 || value > MAX_PERSISTENT_NUMERIC_ID {
        return Err(format_error("persistent ID is outside bounds"));
    }
    Ok(value)
}

fn state_id(bytes: &[u8]) -> Result<StateId, CoreError> {
    Ok(StateId::from_raw(persistent_id(bytes)?))
}

fn branch_id(bytes: &[u8]) -> Result<BranchId, CoreError> {
    Ok(BranchId::from_raw(persistent_id(bytes)?))
}

fn format_error(message: &'static str) -> CoreError {
    CoreError::Format(message.to_owned())
}

fn asset_kind_code(kind: AssetKind) -> u32 {
    match kind {
        AssetKind::CanonicalRaster => 1,
        AssetKind::CanonicalSampleStream => 3,
    }
}

fn decode_asset_kind(value: u32) -> Result<AssetKind, CoreError> {
    match value {
        1 => Ok(AssetKind::CanonicalRaster),
        3 => Ok(AssetKind::CanonicalSampleStream),
        _ => Err(format_error("ASST asset kind is invalid")),
    }
}

fn pixel_format_code(format: PixelFormat) -> u32 {
    match format {
        PixelFormat::BinaryMask8 => 1,
        PixelFormat::Grayscale8 => 2,
        PixelFormat::Grayscale16 => 3,
        PixelFormat::StraightRgba8 => 4,
        PixelFormat::StraightRgba16 => 5,
        PixelFormat::PremultipliedBgra8 => 6,
    }
}

fn decode_optional_pixel_format(value: u32) -> Result<Option<PixelFormat>, CoreError> {
    match value {
        0 => Ok(None),
        1 => Ok(Some(PixelFormat::BinaryMask8)),
        2 => Ok(Some(PixelFormat::Grayscale8)),
        3 => Ok(Some(PixelFormat::Grayscale16)),
        4 => Ok(Some(PixelFormat::StraightRgba8)),
        5 => Ok(Some(PixelFormat::StraightRgba16)),
        _ => Err(format_error("ASST pixel format is invalid")),
    }
}

fn asset_color_space_code(value: AssetColorSpace) -> u32 {
    match value {
        AssetColorSpace::Srgb => 1,
    }
}

fn decode_optional_asset_color_space(value: u32) -> Result<Option<AssetColorSpace>, CoreError> {
    match value {
        0 => Ok(None),
        1 => Ok(Some(AssetColorSpace::Srgb)),
        _ => Err(format_error("ASST color space is invalid")),
    }
}

fn asset_alpha_code(value: AssetAlphaSemantics) -> u32 {
    match value {
        AssetAlphaSemantics::Opaque => 1,
        AssetAlphaSemantics::Straight => 2,
        AssetAlphaSemantics::CoverageMask => 3,
    }
}

fn decode_optional_asset_alpha(value: u32) -> Result<Option<AssetAlphaSemantics>, CoreError> {
    match value {
        0 => Ok(None),
        1 => Ok(Some(AssetAlphaSemantics::Opaque)),
        2 => Ok(Some(AssetAlphaSemantics::Straight)),
        3 => Ok(Some(AssetAlphaSemantics::CoverageMask)),
        _ => Err(format_error("ASST alpha semantics are invalid")),
    }
}

fn nonzero_optional<T>(value: T) -> Option<T>
where
    T: Default + PartialEq,
{
    (value != T::default()).then_some(value)
}

struct ByteReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> ByteReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn position(&self) -> usize {
        self.cursor
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], CoreError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or_else(|| format_error("native payload cursor overflows"))?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| format_error("native payload is truncated"))?;
        self.cursor = end;
        Ok(value)
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], CoreError> {
        fixed::<N>(self.take(N)?, "native fixed field")
    }

    fn u8(&mut self) -> Result<u8, CoreError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, CoreError> {
        Ok(u16::from_le_bytes(self.fixed()?))
    }

    fn u32(&mut self) -> Result<u32, CoreError> {
        Ok(u32::from_le_bytes(self.fixed()?))
    }

    fn u64(&mut self) -> Result<u64, CoreError> {
        Ok(u64::from_le_bytes(self.fixed()?))
    }

    fn persistent_id(&mut self) -> Result<u64, CoreError> {
        let value = self.u64()?;
        if value == 0 || value > MAX_PERSISTENT_NUMERIC_ID {
            return Err(format_error("persistent ID is outside bounds"));
        }
        Ok(value)
    }

    fn length(&mut self) -> Result<usize, CoreError> {
        usize::try_from(self.u64()?)
            .map_err(|_| format_error("native payload length is not addressable"))
    }

    fn finish(self) -> Result<(), CoreError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(format_error("native payload has trailing bytes"))
        }
    }
}

pub(super) fn raster_to_file_plane(id: u64, kind: FilePlaneKind, raster: &TileRaster) -> FilePlane {
    let tiles = raster
        .allocated_coords()
        .filter_map(|coord| raster.tile_data(coord))
        .map(|tile| FileTile {
            coord: tile.coord,
            width: tile.width,
            height: tile.height,
            bytes: tile.bytes,
        })
        .collect();
    FilePlane {
        id,
        kind,
        pixel_format: raster.format(),
        width: raster.width(),
        height: raster.height(),
        tiles,
    }
}

pub(super) fn file_plane_to_raster(
    plane: &FilePlane,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    let mut raster = TileRaster::new(plane.width, plane.height, plane.pixel_format)?;
    for tile in &plane.tiles {
        raster.insert_tile(TileData {
            coord: tile.coord,
            width: tile.width,
            height: tile.height,
            bytes: tile.bytes.clone(),
            revision,
        })?;
    }
    Ok(raster)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_policy_uses_each_closed_threshold_without_changing_ranges() {
        let below = PersistenceCounters {
            procedure_count: CHECKPOINT_PROCEDURE_INTERVAL - 1,
            replay_work: CHECKPOINT_REPLAY_WORK_INTERVAL - 1,
            dirty_bytes: CHECKPOINT_DIRTY_BYTES_INTERVAL - 1,
        };
        assert!(!below.checkpoint_due());
        for counters in [
            PersistenceCounters {
                procedure_count: CHECKPOINT_PROCEDURE_INTERVAL,
                ..below
            },
            PersistenceCounters {
                replay_work: CHECKPOINT_REPLAY_WORK_INTERVAL,
                ..below
            },
            PersistenceCounters {
                dirty_bytes: CHECKPOINT_DIRTY_BYTES_INTERVAL,
                ..below
            },
        ] {
            assert!(counters.checkpoint_due());
        }
    }
}
