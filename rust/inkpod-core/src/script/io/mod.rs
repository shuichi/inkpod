//! Shared Rust I/O adapter. The host supplies explicit approved absolute paths
//! and frozen sessions; all filesystem identity and publication stays in inkpod-io.

use super::compile::StaticScriptProgram;
use super::plan::*;
use super::run::*;
use crate::Core;
use inkpod_format::{
    MAX_INKSCRIPT_CONTAINER_ELEMENTS, MAX_INKSCRIPT_INPUTS, MAX_INKSCRIPT_SOURCE_BYTES,
};
use inkpod_io::{FileIdentity, FileStamp, IoManager, JobContext, PathAuthority};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod assets;
mod planning;
mod running;
mod sequence;
pub use sequence::ScriptIoSequenceInput;

const MAX_APPROVED_PATHS: usize = 2 * MAX_INKSCRIPT_INPUTS + MAX_INKSCRIPT_CONTAINER_ELEMENTS + 1;

#[derive(Clone)]
struct Session {
    snapshot: ScriptSessionSnapshot,
    backing: Option<ValidatedPathIdentity>,
    pair_alias: Option<ValidatedPathIdentity>,
    uuid: u128,
}
#[derive(Default)]
struct LiveState {
    authority_generation: u64,
    sessions_generation: u64,
    sessions: BTreeMap<u64, (u64, u64)>,
    sequence_revision: u64,
}

/// Core-only shared-manager implementation of the existing plan/run adapters.
/// Construction grants no authority: callers explicitly map every path intent to
/// an approved absolute path. Clones share invalidation state, so owner-thread
/// close/source-replacement notifications invalidate a run without OS handles.
#[derive(Clone)]
pub struct ScriptIoAdapter {
    manager: IoManager,
    context: JobContext,
    paths: BTreeMap<u64, PathBuf>,
    identities: BTreeMap<String, PathAuthority>,
    grants: BTreeMap<u64, PathAuthority>,
    sessions: BTreeMap<u64, Session>,
    current_sequence: Option<ScriptSequenceSnapshot>,
    sequence_revision: u64,
    state: Arc<Mutex<LiveState>>,
    new_tab_capacity: usize,
    pending_created: Vec<ValidatedPathIdentity>,
}

impl ScriptIoAdapter {
    /// Accepts an explicit approved absolute path for each path-intent ID. No
    /// source path is resolved against cwd. Duplicate/zero IDs are rejected.
    pub fn new(
        manager: IoManager,
        approved_paths: Vec<(u64, PathBuf)>,
        new_tab_capacity: usize,
    ) -> Result<Self, ScriptPlanError> {
        if approved_paths.len() > MAX_APPROVED_PATHS
            || approved_paths
                .iter()
                .try_fold(0usize, |sum, (_, path)| {
                    sum.checked_add(path.as_os_str().len())
                })
                .is_none_or(|sum| sum > MAX_INKSCRIPT_SOURCE_BYTES)
        {
            return Err(ScriptPlanError::ResourceLimit);
        }
        let mut paths = BTreeMap::new();
        for (id, path) in approved_paths {
            if id == 0 || !path.is_absolute() || paths.insert(id, path).is_some() {
                return Err(ScriptPlanError::AuthorityMismatch);
            }
        }
        if new_tab_capacity > MAX_INKSCRIPT_INPUTS {
            return Err(ScriptPlanError::ResourceLimit);
        }
        Ok(Self {
            manager,
            context: JobContext::new(),
            paths,
            identities: BTreeMap::new(),
            grants: BTreeMap::new(),
            sessions: BTreeMap::new(),
            current_sequence: None,
            sequence_revision: 0,
            state: Arc::new(Mutex::new(LiveState {
                authority_generation: 1,
                sessions_generation: 1,
                sessions: BTreeMap::new(),
                sequence_revision: 0,
            })),
            new_tab_capacity,
            pending_created: Vec::new(),
        })
    }

    /// Shared manager used for codec, temporary storage and contact-sheet work.
    pub fn manager(&self) -> &IoManager {
        &self.manager
    }

    /// Captures an owned host session using the Core's current native backing
    /// path, or the observed raster of a planned pair. Both pair members remain
    /// one session identity, including a missing member reserved for normal Save.
    /// Backed sessions derive their display name and cell number from that path;
    /// `label` and `display_number` are fallbacks only for a pathless session.
    /// Core state without native or planned pair authority remains pathless; no
    /// backing authority is inferred from the label. Failure semantics match
    /// [`Self::capture_session`], which remains available for explicit captures.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_session_from_core(
        &mut self,
        session_id: u64,
        session_generation: u64,
        source_generation: u64,
        label: String,
        display_number: u32,
        core: &Core,
    ) -> Result<(), ScriptPlanError> {
        let (backing, pair_alias) = if let Some(pair) = &core.io_pair_authority {
            (
                Some(pair.native_path.clone()),
                Some(pair.raster_path.clone()),
            )
        } else if let Some(pair) = &core.io_pair_plan {
            (
                Some(pair.raster_path.clone()),
                Some(pair.native_path.clone()),
            )
        } else {
            (core.current_path.clone(), None)
        };
        let (label, display_number) = match &backing {
            Some(path) => {
                let label = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or(ScriptPlanError::InvalidInput)?
                    .to_owned();
                let number = self::display_number(&label);
                (label, number)
            }
            None => (label, display_number),
        };
        self.capture_session_with_alias(
            session_id,
            session_generation,
            source_generation,
            label,
            display_number,
            backing,
            pair_alias,
            core,
        )
    }

    /// Captures the issuing Core, including history and both savepoints. Caller
    /// supplies a nonzero view/session generation and invalidates it on source
    /// replacement, view replacement, close or explicit authority revocation.
    /// Ordinary document edits do not invalidate the immutable captured input;
    /// active publication separately validates its exact document-save token.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_session(
        &mut self,
        session_id: u64,
        session_generation: u64,
        source_generation: u64,
        label: String,
        display_number: u32,
        backing_path: Option<PathBuf>,
        core: &Core,
    ) -> Result<(), ScriptPlanError> {
        self.capture_session_with_alias(
            session_id,
            session_generation,
            source_generation,
            label,
            display_number,
            backing_path,
            None,
            core,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn capture_session_with_alias(
        &mut self,
        session_id: u64,
        session_generation: u64,
        source_generation: u64,
        label: String,
        display_number: u32,
        backing_path: Option<PathBuf>,
        pair_alias: Option<PathBuf>,
        core: &Core,
    ) -> Result<(), ScriptPlanError> {
        if self.sessions.len() >= MAX_INKSCRIPT_INPUTS && !self.sessions.contains_key(&session_id) {
            return Err(ScriptPlanError::ResourceLimit);
        }
        {
            let state = self
                .state
                .lock()
                .map_err(|_| ScriptPlanError::StaleAuthority)?;
            if state.sessions.len() >= MAX_INKSCRIPT_INPUTS
                && !state.sessions.contains_key(&session_id)
            {
                return Err(ScriptPlanError::ResourceLimit);
            }
        }
        let backing = backing_path
            .as_ref()
            .map(|path| self.observe(path))
            .transpose()
            .map_err(|_| ScriptPlanError::InvalidPathIdentity)?;
        let pair_alias = pair_alias
            .as_ref()
            .map(|path| self.observe(path))
            .transpose()
            .map_err(|_| ScriptPlanError::InvalidPathIdentity)?;
        let snapshot = ScriptSessionSnapshot::capture(
            session_id,
            session_generation,
            source_generation,
            label,
            display_number,
            backing.clone(),
            core,
        )?;
        let uuid = core
            .document_info()
            .map_err(|_| ScriptPlanError::InvalidInput)?
            .document_uuid;
        let mut state = self
            .state
            .lock()
            .map_err(|_| ScriptPlanError::StaleAuthority)?;
        if state.sessions.len() >= MAX_INKSCRIPT_INPUTS && !state.sessions.contains_key(&session_id)
        {
            return Err(ScriptPlanError::ResourceLimit);
        }
        state.sessions_generation = state
            .sessions_generation
            .checked_add(1)
            .ok_or(ScriptPlanError::ResourceLimit)?;
        state
            .sessions
            .insert(session_id, (session_generation, source_generation));
        self.sessions.insert(
            session_id,
            Session {
                snapshot,
                backing,
                pair_alias,
                uuid,
            },
        );
        Ok(())
    }

    /// Invalidates captured results after source/view replacement, close, or
    /// explicit authority revocation. Ordinary live document edits may retain the
    /// frozen input; exact active application checks them separately. This call
    /// does not change any document or filesystem.
    pub fn invalidate_session(&self, session_id: u64) -> Result<(), ScriptPlanError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ScriptPlanError::StaleAuthority)?;
        let generation = state
            .sessions_generation
            .checked_add(1)
            .ok_or(ScriptPlanError::ResourceLimit)?;
        state.sessions.remove(&session_id);
        state.sessions_generation = generation;
        Ok(())
    }

    /// Tests whether a captured session still belongs to the same Core lifetime,
    /// document UUID and native/pair file authority. Ordinary document/editor
    /// edits and history moves do not invalidate immutable inputs; active apply
    /// separately checks the complete persistence token. This query performs no
    /// allocation, file I/O, encoding or invalidation; absent/stale sessions return
    /// `false`. The host explicitly invalidates a rejected capture before use.
    pub fn validate_captured_session_backing(
        &self,
        session_id: u64,
        core: &Core,
    ) -> Result<bool, ScriptPlanError> {
        let Some(session) = self.sessions.get(&session_id) else {
            return Ok(false);
        };
        let state = self
            .state
            .lock()
            .map_err(|_| ScriptPlanError::StaleAuthority)?;
        Ok(state.sessions.get(&session_id)
            == Some(&(
                session.snapshot.session_generation(),
                session.snapshot.source_generation(),
            ))
            && session.snapshot.backing_matches(core))
    }

    /// Invalidates all grants when the owner revokes filesystem authority.
    pub fn invalidate_authority(&self) -> Result<(), ScriptPlanError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ScriptPlanError::StaleAuthority)?;
        state.authority_generation = state
            .authority_generation
            .checked_add(1)
            .ok_or(ScriptPlanError::ResourceLimit)?;
        Ok(())
    }

    /// Captures a host-resolved sequence; the sequence expectation remains bound
    /// to the exact source generations already defined by the planner.
    pub fn capture_sequence(
        &mut self,
        sequence: ScriptSequenceSnapshot,
    ) -> Result<(), ScriptPlanError> {
        self.capture_sequence_if_current(sequence, None)
    }

    fn capture_sequence_if_current(
        &mut self,
        sequence: ScriptSequenceSnapshot,
        expected_generations: Option<(u64, u64)>,
    ) -> Result<(), ScriptPlanError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ScriptPlanError::StaleAuthority)?;
        if expected_generations.is_some_and(|expected| {
            expected != (state.authority_generation, state.sessions_generation)
        }) {
            return Err(ScriptPlanError::StaleAuthority);
        }
        let sessions_generation = state
            .sessions_generation
            .checked_add(1)
            .ok_or(ScriptPlanError::ResourceLimit)?;
        let sequence_revision = state
            .sequence_revision
            .checked_add(1)
            .ok_or(ScriptPlanError::ResourceLimit)?;
        self.current_sequence = Some(sequence);
        state.sessions_generation = sessions_generation;
        state.sequence_revision = sequence_revision;
        self.sequence_revision = sequence_revision;
        Ok(())
    }

    /// Observes every approved path and binds the resulting grants to this exact
    /// compiled source. Existing script-file self-overwrite authority is optional.
    pub fn authority(
        &mut self,
        program: &StaticScriptProgram,
        current_session: Option<u64>,
        script_path: Option<PathBuf>,
    ) -> Result<AuthoritySnapshot, ScriptPlanError> {
        let (generation, sessions_generation) = {
            let state = self
                .state
                .lock()
                .map_err(|_| ScriptPlanError::StaleAuthority)?;
            (state.authority_generation, state.sessions_generation)
        };
        let mut grants = Vec::new();
        for intent in program.path_intents() {
            let path = self
                .paths
                .get(&intent.id())
                .cloned()
                .ok_or(ScriptPlanError::AuthorityMismatch)?;
            let observed = self
                .observe(&path)
                .map_err(|_| ScriptPlanError::InvalidPathIdentity)?;
            self.grants.insert(
                intent.id(),
                self.identities
                    .get(observed.canonical_key())
                    .cloned()
                    .ok_or(ScriptPlanError::InvalidPathIdentity)?,
            );
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"inkpod.shared-script-grant.v1");
            hasher.update(&intent.id().to_le_bytes());
            hasher.update(observed.canonical_key().as_bytes());
            grants.push(AuthorityGrant::new(
                intent.id(),
                intent.access(),
                *hasher.finalize().as_bytes(),
                generation,
                observed,
            )?);
        }
        let current = current_session
            .map(|id| {
                self.sessions
                    .get(&id)
                    .ok_or(ScriptPlanError::StaleInput)
                    .and_then(|session| ScriptSessionExpectation::from_snapshot(&session.snapshot))
            })
            .transpose()?;
        let sequence = self
            .current_sequence
            .as_ref()
            .map(ScriptSequenceExpectation::from_snapshot)
            .transpose()?;
        let script_path = script_path
            .as_ref()
            .map(|path| self.observe(path))
            .transpose()
            .map_err(|_| ScriptPlanError::InvalidPathIdentity)?;
        AuthoritySnapshot::new(
            *program.static_compile_digest(),
            *program.path_intent_digest(),
            generation,
            grants,
            ScriptCommandContext::new(current, sequence),
            sessions_generation,
            script_path,
        )
    }

    fn observe(&mut self, path: &Path) -> Result<ValidatedPathIdentity, ScriptRunAdapterError> {
        let value = self
            .manager
            .observe_path_authority(path, &self.context)
            .map_err(run_error)?;
        let identity = path_identity(&value)?;
        self.identities
            .insert(identity.canonical_key().to_owned(), value);
        Ok(identity)
    }
    fn known_path(
        &self,
        identity: &ValidatedPathIdentity,
    ) -> Result<PathBuf, ScriptRunAdapterError> {
        self.identities
            .get(identity.canonical_key())
            .map(|value| value.path.clone())
            .ok_or(ScriptRunAdapterError::InvalidData)
    }
    fn validate_grant(&self, id: u64) -> Result<(), ScriptPlanAdapterError> {
        let granted = self
            .grants
            .get(&id)
            .ok_or(ScriptPlanAdapterError::Unavailable)?;
        let approved = self
            .paths
            .get(&id)
            .ok_or(ScriptPlanAdapterError::Unavailable)?;
        let observed = self
            .manager
            .observe_path_authority(approved, &self.context)
            .map_err(|_| ScriptPlanAdapterError::Failure)?;
        if &observed != granted {
            return Err(ScriptPlanAdapterError::Failure);
        }
        Ok(())
    }
    fn read_fingerprint(
        &mut self,
        path: &Path,
    ) -> Result<(NativeInputFingerprint, Vec<u8>), ScriptRunAdapterError> {
        self.read_fingerprint_cancellable(path, &mut || false)
    }

    fn read_fingerprint_cancellable(
        &mut self,
        path: &Path,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(NativeInputFingerprint, Vec<u8>), ScriptRunAdapterError> {
        let observed = self.observe(path)?;
        let loaded = self
            .manager
            .read_bytes_cancellable(path, 1024 * 1024 * 1024, &self.context, cancelled)
            .map_err(run_error)?;
        if observed.object_id() != Some(object_id(loaded.identity())) {
            return Err(ScriptRunAdapterError::InvalidData);
        }
        let digest = *blake3::hash(loaded.bytes()).as_bytes();
        let label = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(ScriptRunAdapterError::InvalidData)?
            .to_owned();
        let token = Some(stamp_token(loaded.stamp()));
        let fingerprint = if path
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case("inkpod"))
        {
            let file = inkpod_format::decode_procedure_file(loaded.bytes())
                .map_err(|_| ScriptRunAdapterError::InvalidData)?;
            let core =
                Core::from_procedure_file(file).map_err(|_| ScriptRunAdapterError::InvalidData)?;
            let uuid = core
                .document_info()
                .map_err(|_| ScriptRunAdapterError::InvalidData)?
                .document_uuid;
            NativeInputFingerprint::new(
                observed,
                label.clone(),
                display_number(&label),
                uuid,
                loaded.bytes().len() as u64,
                digest,
                token,
                self.manager.supports_guarded_publication(),
            )
        } else {
            NativeInputFingerprint::new_raster(
                observed,
                label,
                loaded.bytes().len() as u64,
                digest,
                token,
            )
        }
        .map_err(|_| ScriptRunAdapterError::InvalidData)?;
        Ok((fingerprint, loaded.bytes().to_vec()))
    }
}

fn display_number(label: &str) -> u32 {
    let stem = Path::new(label)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(label);
    let bytes = stem.as_bytes();
    let Some(last) = bytes.iter().rposition(u8::is_ascii_digit) else {
        return 1;
    };
    let mut first = last;
    while first > 0 && bytes[first - 1].is_ascii_digit() {
        first -= 1;
    }
    stem[first..=last]
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(1)
}
fn object_id(identity: FileIdentity) -> [u8; 32] {
    let mut value = [0; 32];
    value[..8].copy_from_slice(&identity.volume.to_le_bytes());
    value[8..24].copy_from_slice(&identity.file.to_le_bytes());
    value[31] = 1;
    value
}
fn volume_id(identity: FileIdentity) -> [u8; 16] {
    let mut value = [0; 16];
    value[..8].copy_from_slice(&identity.volume.to_le_bytes());
    value[15] = 1;
    value
}
fn path_identity(value: &PathAuthority) -> Result<ValidatedPathIdentity, ScriptRunAdapterError> {
    let result = match value.object {
        Some(object) => ValidatedPathIdentity::existing(
            value.canonical_key.clone(),
            volume_id(object),
            object_id(object),
            value.alias_key,
            object_id(value.parent),
            value.parent_alias_key,
        ),
        None => ValidatedPathIdentity::expected_absent(
            value.canonical_key.clone(),
            volume_id(value.parent),
            object_id(value.parent),
            value.alias_key,
            value.parent_alias_key,
        ),
    };
    result.map_err(|_| ScriptRunAdapterError::InvalidData)
}
fn stamp_token(stamp: FileStamp) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(&object_id(stamp.identity));
    hash.update(&stamp.length.to_le_bytes());
    hash.update(&stamp.modified.to_le_bytes());
    hash.update(&stamp.changed.to_le_bytes());
    hash.update(&[u8::from(stamp.readonly)]);
    *hash.finalize().as_bytes()
}
fn run_error(error: inkpod_io::IoError) -> ScriptRunAdapterError {
    match error {
        inkpod_io::IoError::Cancelled => ScriptRunAdapterError::Cancelled,
        inkpod_io::IoError::UnsupportedAtomicPublication => {
            ScriptRunAdapterError::UnsupportedAtomicInstall
        }
        inkpod_io::IoError::ConfirmationRequired | inkpod_io::IoError::ChangedDuringRead => {
            ScriptRunAdapterError::InvalidData
        }
        _ => ScriptRunAdapterError::Io,
    }
}
fn plan_error(_: ScriptRunAdapterError) -> ScriptPlanAdapterError {
    ScriptPlanAdapterError::Failure
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_generation_overflow_and_session_limit_leave_prior_state_intact() {
        let mut adapter = ScriptIoAdapter::new(
            IoManager::new(inkpod_io::IoConfig::default()).unwrap(),
            vec![],
            1,
        )
        .unwrap();
        let mut core = Core::new();
        core.new_cell(1, 1, crate::DEFAULT_DPI_MILLI, crate::DEFAULT_DPI_MILLI)
            .unwrap();
        adapter
            .capture_session(1, 1, 1, "current-cell.inkpod".into(), 1, None, &core)
            .unwrap();
        {
            let mut state = adapter.state.lock().unwrap();
            state.sessions_generation = u64::MAX;
        }
        assert_eq!(
            adapter.invalidate_session(1),
            Err(ScriptPlanError::ResourceLimit)
        );
        assert_eq!(
            adapter.state.lock().unwrap().sessions.get(&1),
            Some(&(1, 1))
        );
        {
            let mut state = adapter.state.lock().unwrap();
            state.sessions_generation = 9;
            state.sequence_revision = u64::MAX;
        }
        assert_eq!(
            adapter.capture_sequence(ScriptSequenceSnapshot::new(1, 1, vec![]).unwrap()),
            Err(ScriptPlanError::ResourceLimit)
        );
        assert!(adapter.current_sequence.is_none());
        assert_eq!(adapter.state.lock().unwrap().sessions_generation, 9);
        {
            let mut state = adapter.state.lock().unwrap();
            state.sessions = (1..=MAX_INKSCRIPT_INPUTS as u64)
                .map(|id| (id, (1, 1)))
                .collect();
        }
        assert_eq!(
            adapter.capture_session(
                MAX_INKSCRIPT_INPUTS as u64 + 1,
                1,
                1,
                "current-cell.inkpod".into(),
                1,
                None,
                &core
            ),
            Err(ScriptPlanError::ResourceLimit)
        );
        assert!(
            adapter
                .capture_session(1, 2, 2, "current-cell.inkpod".into(), 1, None, &core)
                .is_ok()
        );
    }
}
