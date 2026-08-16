use super::assets::{
    AuthorizedAssetStream, FrozenScriptAssets, ScriptAssetError, ScriptAssetLimits,
    ScriptAssetSource, ScriptAssetUsage, freeze_inkscript_assets,
};
use super::compile::{ScriptPathIntentSubject, ScriptStaticPathIntent, StaticScriptProgram};
use crate::{Core, DocumentStateDigest, EditorStateDigest};
use inkpod_format::{
    InkScriptCellSelection, InkScriptInputDeclarationKind, InkScriptNumberDirection,
    InkScriptOutput, InkScriptPathIntentAccess, MAX_INKSCRIPT_INPUTS, MAX_INKSCRIPT_STRING_BYTES,
    MAX_INKSCRIPT_WAIT_MS, encode_procedure_file,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

const PLAN_DIGEST_CONTEXT: &str = "inkpod.inkscript.execution-plan.v1";
const CONFIRMATION_DIGEST_CONTEXT: &str = "inkpod.inkscript.confirmation.v1";
const MAX_FOLDER_ENTRIES: u64 = 1_048_576;
const MAX_FOLDER_NAME_BYTES: u64 = 256 * 1024 * 1024;
const MAX_FOLDER_WORK_UNITS: u64 = 1_048_576;
const MAX_FOLDER_DEPTH: u32 = 64;
const MAX_NATIVE_INPUT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_PLANNED_OUTPUT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_PLANNED_INVOCATIONS: u64 = 1_048_576;

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ValidatedPathIdentity {
    canonical_key: String,
    volume_id: [u8; 16],
    object_id: Option<[u8; 32]>,
    object_generation: Option<u64>,
    alias_key: [u8; 32],
    parent_object_id: [u8; 32],
    parent_generation: u64,
    parent_alias_key: [u8; 32],
    expected_absent: bool,
}

impl ValidatedPathIdentity {
    pub fn existing(
        canonical_key: String,
        volume_id: [u8; 16],
        object_id: [u8; 32],
        alias_key: [u8; 32],
        parent_object_id: [u8; 32],
        parent_alias_key: [u8; 32],
    ) -> Result<Self, ScriptPlanError> {
        let value = Self {
            canonical_key,
            volume_id,
            object_id: Some(object_id),
            object_generation: Some(1),
            alias_key,
            parent_object_id,
            parent_generation: 1,
            parent_alias_key,
            expected_absent: false,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn expected_absent(
        canonical_key: String,
        volume_id: [u8; 16],
        parent_object_id: [u8; 32],
        alias_key: [u8; 32],
        parent_alias_key: [u8; 32],
    ) -> Result<Self, ScriptPlanError> {
        let value = Self {
            canonical_key,
            volume_id,
            object_id: None,
            object_generation: None,
            alias_key,
            parent_object_id,
            parent_generation: 1,
            parent_alias_key,
            expected_absent: true,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn canonical_key(&self) -> &str {
        &self.canonical_key
    }

    pub const fn object_id(&self) -> Option<[u8; 32]> {
        self.object_id
    }

    pub const fn object_generation(&self) -> Option<u64> {
        self.object_generation
    }

    pub const fn volume_id(&self) -> [u8; 16] {
        self.volume_id
    }

    pub const fn parent_object_id(&self) -> [u8; 32] {
        self.parent_object_id
    }

    pub const fn parent_generation(&self) -> u64 {
        self.parent_generation
    }

    pub const fn parent_alias_key(&self) -> [u8; 32] {
        self.parent_alias_key
    }

    pub const fn alias_key(&self) -> [u8; 32] {
        self.alias_key
    }

    pub const fn is_expected_absent(&self) -> bool {
        self.expected_absent
    }

    pub fn matches_exact(&self, other: &Self) -> bool {
        self == other
    }

    pub fn with_generations(
        mut self,
        object_generation: Option<u64>,
        parent_generation: u64,
    ) -> Result<Self, ScriptPlanError> {
        self.object_generation = object_generation;
        self.parent_generation = parent_generation;
        self.validate()?;
        Ok(self)
    }

    fn validate(&self) -> Result<(), ScriptPlanError> {
        if self.canonical_key.is_empty()
            || self.canonical_key.len() > MAX_INKSCRIPT_STRING_BYTES
            || !self.canonical_key.contains(":/")
            || self.canonical_key.contains('\\')
            || self.volume_id == [0; 16]
            || self.alias_key == [0; 32]
            || self.parent_object_id == [0; 32]
            || self.parent_generation == 0
            || self.parent_alias_key == [0; 32]
            || (self.expected_absent != self.object_id.is_none())
            || (self.expected_absent != self.object_generation.is_none())
            || self.object_id == Some([0; 32])
            || self.object_generation == Some(0)
        {
            return Err(ScriptPlanError::InvalidPathIdentity);
        }
        Ok(())
    }

    fn same_object(&self, other: &Self) -> bool {
        self.volume_id == other.volume_id
            && self.object_id.is_some()
            && self.object_id == other.object_id
            && self.object_generation == other.object_generation
    }

    fn aliases(&self, other: &Self) -> bool {
        self.alias_key == other.alias_key || self.same_object(other)
    }

    fn same_existing_identity(&self, other: &Self) -> bool {
        self.alias_key == other.alias_key && self.same_object(other)
    }

    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        hash_bytes(hasher, self.canonical_key.as_bytes());
        hasher.update(&self.volume_id);
        hasher.update(self.object_id.as_ref().unwrap_or(&[0; 32]));
        hasher.update(&self.object_generation.unwrap_or(0).to_le_bytes());
        hasher.update(&self.alias_key);
        hasher.update(&self.parent_object_id);
        hasher.update(&self.parent_generation.to_le_bytes());
        hasher.update(&self.parent_alias_key);
        hasher.update(&[u8::from(self.expected_absent)]);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct NativeInputFingerprint {
    path: ValidatedPathIdentity,
    display_label: String,
    display_number: u32,
    document_uuid: u128,
    logical_length: u64,
    content_digest: [u8; 32],
    change_token: Option<[u8; 32]>,
    supports_atomic_overwrite: bool,
}

impl NativeInputFingerprint {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: ValidatedPathIdentity,
        display_label: String,
        display_number: u32,
        document_uuid: u128,
        logical_length: u64,
        content_digest: [u8; 32],
        change_token: Option<[u8; 32]>,
        supports_atomic_overwrite: bool,
    ) -> Result<Self, ScriptPlanError> {
        path.validate()?;
        if path.expected_absent
            || !valid_native_filename(&display_label)
            || display_number == 0
            || document_uuid == 0
            || logical_length == 0
            || content_digest == [0; 32]
            || change_token == Some([0; 32])
        {
            return Err(ScriptPlanError::InvalidInput);
        }
        Ok(Self {
            path,
            display_label,
            display_number,
            document_uuid,
            logical_length,
            content_digest,
            change_token,
            supports_atomic_overwrite,
        })
    }

    pub const fn path(&self) -> &ValidatedPathIdentity {
        &self.path
    }

    pub const fn document_uuid(&self) -> u128 {
        self.document_uuid
    }

    pub const fn logical_length(&self) -> u64 {
        self.logical_length
    }

    pub const fn content_digest(&self) -> [u8; 32] {
        self.content_digest
    }

    pub const fn supports_atomic_overwrite(&self) -> bool {
        self.supports_atomic_overwrite
    }

    pub fn matches_exact(&self, other: &Self) -> bool {
        self == other
    }

    pub fn display_label(&self) -> &str {
        &self.display_label
    }

    pub const fn display_number(&self) -> u32 {
        self.display_number
    }

    pub const fn change_token(&self) -> Option<[u8; 32]> {
        self.change_token
    }
}

#[derive(Clone, Debug)]
#[doc(hidden)]
pub struct ScriptSessionSnapshot {
    session_id: u64,
    session_generation: u64,
    source_generation: u64,
    display_label: String,
    display_number: u32,
    backing_path: Option<ValidatedPathIdentity>,
    document_uuid: u128,
    document_revision: u64,
    state_digest: DocumentStateDigest,
    editor_revision: u64,
    editor_digest: EditorStateDigest,
    estimated_native_bytes: u64,
    core: Box<Core>,
}

impl ScriptSessionSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        session_id: u64,
        session_generation: u64,
        source_generation: u64,
        display_label: String,
        display_number: u32,
        backing_path: Option<ValidatedPathIdentity>,
        core: &Core,
    ) -> Result<Self, ScriptPlanError> {
        let info = core
            .document_info()
            .map_err(|_| ScriptPlanError::InvalidInput)?;
        let editor = core
            .editor_state()
            .map_err(|_| ScriptPlanError::InvalidInput)?;
        let native = core
            .build_procedure_file(
                core.savepoint,
                core.editor_session
                    .as_ref()
                    .and_then(|session| session.savepoint),
            )
            .map_err(|_| ScriptPlanError::InvalidInput)?;
        let estimated_native_bytes = u64::try_from(
            encode_procedure_file(&native)
                .map_err(|_| ScriptPlanError::InvalidInput)?
                .len(),
        )
        .map_err(|_| ScriptPlanError::ResourceLimit)?;
        if session_id == 0
            || session_generation == 0
            || source_generation == 0
            || display_number == 0
            || !valid_native_filename(&display_label)
            || backing_path
                .as_ref()
                .is_some_and(|path| path.expected_absent)
        {
            return Err(ScriptPlanError::InvalidInput);
        }
        let mut snapshot_core = core.clone();
        snapshot_core.current_path = None;
        Ok(Self {
            session_id,
            session_generation,
            source_generation,
            display_label,
            display_number,
            backing_path,
            document_uuid: info.document_uuid,
            document_revision: info.document_revision,
            state_digest: core
                .document_state_digest()
                .map_err(|_| ScriptPlanError::InvalidInput)?,
            editor_revision: editor.revision.get(),
            editor_digest: editor.digest,
            estimated_native_bytes,
            core: Box::new(snapshot_core),
        })
    }

    fn validate_self(&self) -> Result<(), ScriptPlanError> {
        let info = self
            .core
            .document_info()
            .map_err(|_| ScriptPlanError::InvalidInput)?;
        let editor = self
            .core
            .editor_state()
            .map_err(|_| ScriptPlanError::InvalidInput)?;
        if info.document_uuid != self.document_uuid
            || info.document_revision != self.document_revision
            || self.core.document_state_digest().ok() != Some(self.state_digest)
            || editor.revision.get() != self.editor_revision
            || editor.digest != self.editor_digest
        {
            return Err(ScriptPlanError::StaleInput);
        }
        Ok(())
    }

    pub(super) const fn session_id(&self) -> u64 {
        self.session_id
    }

    pub(super) const fn session_generation(&self) -> u64 {
        self.session_generation
    }

    pub(super) const fn source_generation(&self) -> u64 {
        self.source_generation
    }

    pub(super) fn clone_staged_core(&self) -> Result<Core, ScriptPlanError> {
        self.validate_self()?;
        Ok((*self.core).clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptSessionExpectation {
    session_id: u64,
    session_generation: u64,
    source_generation: u64,
    document_uuid: u128,
    document_revision: u64,
    state_digest: DocumentStateDigest,
    editor_revision: u64,
    editor_digest: EditorStateDigest,
    estimated_native_bytes: u64,
}

impl ScriptSessionExpectation {
    pub fn from_snapshot(snapshot: &ScriptSessionSnapshot) -> Result<Self, ScriptPlanError> {
        snapshot.validate_self()?;
        Ok(Self {
            session_id: snapshot.session_id,
            session_generation: snapshot.session_generation,
            source_generation: snapshot.source_generation,
            document_uuid: snapshot.document_uuid,
            document_revision: snapshot.document_revision,
            state_digest: snapshot.state_digest,
            editor_revision: snapshot.editor_revision,
            editor_digest: snapshot.editor_digest,
            estimated_native_bytes: snapshot.estimated_native_bytes,
        })
    }

    fn matches(&self, snapshot: &ScriptSessionSnapshot) -> bool {
        self.session_id == snapshot.session_id
            && self.session_generation == snapshot.session_generation
            && self.source_generation == snapshot.source_generation
            && self.document_uuid == snapshot.document_uuid
            && self.document_revision == snapshot.document_revision
            && self.state_digest == snapshot.state_digest
            && self.editor_revision == snapshot.editor_revision
            && self.editor_digest == snapshot.editor_digest
            && self.estimated_native_bytes == snapshot.estimated_native_bytes
    }
}

#[derive(Clone, Debug)]
#[doc(hidden)]
pub enum ScriptSequenceMemberSnapshot {
    Session(ScriptSessionSnapshot),
    File {
        source_generation: u64,
        fingerprint: NativeInputFingerprint,
    },
}

impl ScriptSequenceMemberSnapshot {
    fn source_generation(&self) -> u64 {
        match self {
            Self::Session(value) => value.source_generation,
            Self::File {
                source_generation, ..
            } => *source_generation,
        }
    }

    fn document_uuid(&self) -> u128 {
        match self {
            Self::Session(value) => value.document_uuid,
            Self::File { fingerprint, .. } => fingerprint.document_uuid,
        }
    }
}

#[derive(Clone, Debug)]
#[doc(hidden)]
pub struct ScriptSequenceSnapshot {
    sequence_id: u64,
    generation: u64,
    members: Vec<ScriptSequenceMemberSnapshot>,
}

impl ScriptSequenceSnapshot {
    pub fn new(
        sequence_id: u64,
        generation: u64,
        members: Vec<ScriptSequenceMemberSnapshot>,
    ) -> Result<Self, ScriptPlanError> {
        if sequence_id == 0 || generation == 0 || members.len() > MAX_INKSCRIPT_INPUTS {
            return Err(ScriptPlanError::InvalidInput);
        }
        let mut identities = BTreeSet::new();
        for member in &members {
            if member.source_generation() == 0
                || member.document_uuid() == 0
                || !identities.insert((member.document_uuid(), member.source_generation()))
            {
                return Err(ScriptPlanError::InvalidInput);
            }
        }
        Ok(Self {
            sequence_id,
            generation,
            members,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ScriptSequenceMemberExpectation {
    Session {
        session_id: u64,
        session_generation: u64,
        document_uuid: u128,
        source_generation: u64,
    },
    File {
        document_uuid: u128,
        source_generation: u64,
        path_alias: [u8; 32],
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptSequenceExpectation {
    sequence_id: u64,
    generation: u64,
    members: Vec<ScriptSequenceMemberExpectation>,
}

impl ScriptSequenceExpectation {
    pub fn from_snapshot(snapshot: &ScriptSequenceSnapshot) -> Result<Self, ScriptPlanError> {
        if snapshot.sequence_id == 0 || snapshot.generation == 0 {
            return Err(ScriptPlanError::InvalidInput);
        }
        Ok(Self {
            sequence_id: snapshot.sequence_id,
            generation: snapshot.generation,
            members: snapshot
                .members
                .iter()
                .map(|member| match member {
                    ScriptSequenceMemberSnapshot::Session(value) => {
                        ScriptSequenceMemberExpectation::Session {
                            session_id: value.session_id,
                            session_generation: value.session_generation,
                            document_uuid: value.document_uuid,
                            source_generation: value.source_generation,
                        }
                    }
                    ScriptSequenceMemberSnapshot::File {
                        source_generation,
                        fingerprint,
                    } => ScriptSequenceMemberExpectation::File {
                        document_uuid: fingerprint.document_uuid,
                        source_generation: *source_generation,
                        path_alias: fingerprint.path.alias_key,
                    },
                })
                .collect(),
        })
    }

    fn matches(&self, snapshot: &ScriptSequenceSnapshot) -> bool {
        self.sequence_id == snapshot.sequence_id
            && self.generation == snapshot.generation
            && self.members.len() == snapshot.members.len()
            && self
                .members
                .iter()
                .zip(&snapshot.members)
                .all(|(expected, actual)| match (expected, actual) {
                    (
                        ScriptSequenceMemberExpectation::Session {
                            session_id,
                            session_generation,
                            document_uuid,
                            source_generation,
                        },
                        ScriptSequenceMemberSnapshot::Session(value),
                    ) => {
                        *session_id == value.session_id
                            && *session_generation == value.session_generation
                            && *document_uuid == value.document_uuid
                            && *source_generation == value.source_generation
                    }
                    (
                        ScriptSequenceMemberExpectation::File {
                            document_uuid,
                            source_generation,
                            path_alias,
                        },
                        ScriptSequenceMemberSnapshot::File {
                            source_generation: actual_generation,
                            fingerprint,
                        },
                    ) => {
                        *document_uuid == fingerprint.document_uuid
                            && *source_generation == *actual_generation
                            && *path_alias == fingerprint.path.alias_key
                    }
                    _ => false,
                })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptCommandContext {
    current_document: Option<ScriptSessionExpectation>,
    current_sequence: Option<ScriptSequenceExpectation>,
}

impl ScriptCommandContext {
    pub const fn new(
        current_document: Option<ScriptSessionExpectation>,
        current_sequence: Option<ScriptSequenceExpectation>,
    ) -> Self {
        Self {
            current_document,
            current_sequence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct AuthorityGrant {
    intent_id: u64,
    access: InkScriptPathIntentAccess,
    authority_id: [u8; 32],
    generation: u64,
    resolved: ValidatedPathIdentity,
}

impl AuthorityGrant {
    pub fn new(
        intent_id: u64,
        access: InkScriptPathIntentAccess,
        authority_id: [u8; 32],
        generation: u64,
        resolved: ValidatedPathIdentity,
    ) -> Result<Self, ScriptPlanError> {
        if intent_id == 0 || authority_id == [0; 32] || generation == 0 {
            return Err(ScriptPlanError::AuthorityMismatch);
        }
        resolved.validate()?;
        Ok(Self {
            intent_id,
            access,
            authority_id,
            generation,
            resolved,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct AuthoritySnapshot {
    static_compile_digest: [u8; 32],
    path_intent_digest: [u8; 32],
    generation: u64,
    grants: Vec<AuthorityGrant>,
    command_context: ScriptCommandContext,
    open_session_set_generation: u64,
    script_path: Option<ValidatedPathIdentity>,
}

impl AuthoritySnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        static_compile_digest: [u8; 32],
        path_intent_digest: [u8; 32],
        generation: u64,
        mut grants: Vec<AuthorityGrant>,
        command_context: ScriptCommandContext,
        open_session_set_generation: u64,
        script_path: Option<ValidatedPathIdentity>,
    ) -> Result<Self, ScriptPlanError> {
        if static_compile_digest == [0; 32]
            || path_intent_digest == [0; 32]
            || generation == 0
            || open_session_set_generation == 0
            || script_path
                .as_ref()
                .is_some_and(|path| path.expected_absent)
        {
            return Err(ScriptPlanError::AuthorityMismatch);
        }
        grants.sort_by_key(|grant| grant.intent_id);
        Ok(Self {
            static_compile_digest,
            path_intent_digest,
            generation,
            grants,
            command_context,
            open_session_set_generation,
            script_path,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct OpenSessionRecord {
    session_id: u64,
    session_generation: u64,
    document_uuid: u128,
    backing_path: ValidatedPathIdentity,
}

impl OpenSessionRecord {
    pub fn new(
        session_id: u64,
        session_generation: u64,
        document_uuid: u128,
        backing_path: ValidatedPathIdentity,
    ) -> Result<Self, ScriptPlanError> {
        if session_id == 0
            || session_generation == 0
            || document_uuid == 0
            || backing_path.expected_absent
        {
            return Err(ScriptPlanError::InvalidInput);
        }
        Ok(Self {
            session_id,
            session_generation,
            document_uuid,
            backing_path,
        })
    }

    pub const fn session_id(&self) -> u64 {
        self.session_id
    }

    pub const fn session_generation(&self) -> u64 {
        self.session_generation
    }

    pub const fn document_uuid(&self) -> u128 {
        self.document_uuid
    }

    pub const fn backing_path(&self) -> &ValidatedPathIdentity {
        &self.backing_path
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[doc(hidden)]
pub struct OpenSessionSetSnapshot {
    generation: u64,
    sessions: Vec<OpenSessionRecord>,
}

impl OpenSessionSetSnapshot {
    pub fn new(generation: u64, sessions: Vec<OpenSessionRecord>) -> Result<Self, ScriptPlanError> {
        if generation == 0 {
            return Err(ScriptPlanError::StaleAuthority);
        }
        let mut session_ids = BTreeSet::new();
        for (index, session) in sessions.iter().enumerate() {
            if !session_ids.insert(session.session_id)
                || sessions[..index]
                    .iter()
                    .any(|other| other.backing_path.aliases(&session.backing_path))
            {
                return Err(ScriptPlanError::InvalidInput);
            }
        }
        Ok(Self {
            generation,
            sessions,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct FolderScan {
    observed_entries: u64,
    normalized_name_bytes: u64,
    work_units: u64,
    maximum_depth: u32,
    matching_files: Vec<NativeInputFingerprint>,
}

impl FolderScan {
    pub fn new(
        observed_entries: u64,
        normalized_name_bytes: u64,
        work_units: u64,
        maximum_depth: u32,
        matching_files: Vec<NativeInputFingerprint>,
    ) -> Result<Self, ScriptPlanError> {
        if matching_files.len() as u64 > observed_entries
            || work_units < observed_entries
            || maximum_depth == 0
        {
            return Err(ScriptPlanError::InvalidInput);
        }
        Ok(Self {
            observed_entries,
            normalized_name_bytes,
            work_units,
            maximum_depth,
            matching_files,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptPlanAdapterError {
    Unavailable,
    InvalidData,
    Failure,
}

#[doc(hidden)]
pub trait ScriptPlanAdapter {
    fn authority_generation(&mut self) -> Result<u64, ScriptPlanAdapterError>;
    fn open_session_set(&mut self) -> Result<OpenSessionSetSnapshot, ScriptPlanAdapterError>;
    fn resolve_file(
        &mut self,
        intent_id: u64,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<NativeInputFingerprint, ScriptPlanAdapterError>;
    fn enumerate_folder(
        &mut self,
        intent_id: u64,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<FolderScan, ScriptPlanAdapterError>;
    fn capture_current_document(
        &mut self,
        expected: &ScriptSessionExpectation,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError>;
    fn capture_current_sequence(
        &mut self,
        expected: &ScriptSequenceExpectation,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSequenceSnapshot, ScriptPlanAdapterError>;
    fn capture_open_session(
        &mut self,
        session: &OpenSessionRecord,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError>;
    fn resolve_destination(
        &mut self,
        request: &ScriptDestinationRequest,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ValidatedPathIdentity, ScriptPlanAdapterError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptDestinationBase {
    AuthorizedRoot {
        intent_id: u64,
        root: ValidatedPathIdentity,
    },
    InputParent {
        input_path: ValidatedPathIdentity,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptDestinationRequest {
    base: ScriptDestinationBase,
    relative_components: Vec<String>,
}

impl ScriptDestinationRequest {
    pub const fn base(&self) -> &ScriptDestinationBase {
        &self.base
    }

    pub fn relative_components(&self) -> &[String] {
        &self.relative_components
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptPlanLimits {
    expanded_inputs: u64,
    folder_entries: u64,
    folder_name_bytes: u64,
    folder_work_units: u64,
    folder_depth: u32,
    native_input_bytes: u64,
    planned_output_bytes: u64,
    invocations: u64,
}

impl ScriptPlanLimits {
    pub const fn exact_current() -> Self {
        Self {
            expanded_inputs: MAX_INKSCRIPT_INPUTS as u64,
            folder_entries: MAX_FOLDER_ENTRIES,
            folder_name_bytes: MAX_FOLDER_NAME_BYTES,
            folder_work_units: MAX_FOLDER_WORK_UNITS,
            folder_depth: MAX_FOLDER_DEPTH,
            native_input_bytes: MAX_NATIVE_INPUT_BYTES,
            planned_output_bytes: MAX_PLANNED_OUTPUT_BYTES,
            invocations: MAX_PLANNED_INVOCATIONS,
        }
    }

    pub const fn with_folder_entries(mut self, maximum: u64) -> Self {
        self.folder_entries = lowered(maximum, MAX_FOLDER_ENTRIES);
        self
    }
}

const fn lowered(requested: u64, exact: u64) -> u64 {
    if requested == 0 {
        1
    } else if requested < exact {
        requested
    } else {
        exact
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScriptPlanUsage {
    folder_entries: u64,
    folder_name_bytes: u64,
    folder_work_units: u64,
    native_input_bytes: u64,
    snapshot_input_bytes: u64,
    planned_output_bytes: u64,
    asset: ScriptAssetUsage,
}

#[cfg(test)]
impl ScriptPlanUsage {
    pub(super) const fn native_input_bytes(self) -> u64 {
        self.native_input_bytes
    }

    pub(super) const fn asset(self) -> ScriptAssetUsage {
        self.asset
    }
}

#[derive(Clone, Debug)]
pub(crate) enum PlannedInputSource {
    Session(ScriptSessionSnapshot),
    File(NativeInputFingerprint),
}

#[derive(Clone, Debug)]
pub(crate) struct ScriptPlannedInput {
    input_index: usize,
    display_label: String,
    source_stem: Option<String>,
    path_order_key: String,
    document_uuid: u128,
    source: PlannedInputSource,
}

impl ScriptPlannedInput {
    pub(super) fn path(&self) -> Option<&ValidatedPathIdentity> {
        match &self.source {
            PlannedInputSource::Session(snapshot) => snapshot.backing_path.as_ref(),
            PlannedInputSource::File(fingerprint) => Some(&fingerprint.path),
        }
    }

    fn display_number(&self) -> u32 {
        match &self.source {
            PlannedInputSource::Session(snapshot) => snapshot.display_number,
            PlannedInputSource::File(fingerprint) => fingerprint.display_number,
        }
    }

    pub(super) const fn source(&self) -> &PlannedInputSource {
        &self.source
    }

    pub(super) fn display_label(&self) -> &str {
        &self.display_label
    }

    pub(super) const fn document_uuid(&self) -> u128 {
        self.document_uuid
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptExecutionPreviewItem {
    display_label: String,
    output_name: String,
    destination_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScriptExecutionPreview {
    items: Vec<ScriptExecutionPreviewItem>,
}

#[derive(Clone, Debug)]
#[doc(hidden)]
pub struct ScriptExecutionPlan {
    items: Vec<ScriptPlannedInput>,
    destinations: Vec<ValidatedPathIdentity>,
    frozen_assets: FrozenScriptAssets,
    authority_generation: u64,
    open_session_set_generation: u64,
    plan_digest: [u8; 32],
    preview: ScriptExecutionPreview,
    usage: ScriptPlanUsage,
    static_compile_digest: [u8; 32],
    path_intent_digest: [u8; 32],
}

impl ScriptExecutionPlan {
    pub(super) fn matches_program(&self, program: &StaticScriptProgram) -> bool {
        self.static_compile_digest == program.static_compile_digest
            && self.path_intent_digest == program.path_intent_digest
    }

    pub(super) fn items(&self) -> &[ScriptPlannedInput] {
        &self.items
    }

    pub(super) fn destinations(&self) -> &[ValidatedPathIdentity] {
        &self.destinations
    }

    pub(super) const fn frozen_assets(&self) -> &FrozenScriptAssets {
        &self.frozen_assets
    }

    pub(super) const fn authority_generation(&self) -> u64 {
        self.authority_generation
    }

    pub(super) const fn open_session_set_generation(&self) -> u64 {
        self.open_session_set_generation
    }

    pub fn preview_items(&self) -> &[ScriptExecutionPreviewItem] {
        &self.preview.items
    }

    pub const fn plan_digest(&self) -> [u8; 32] {
        self.plan_digest
    }

    pub fn input_count(&self) -> usize {
        self.items.len()
    }

    #[cfg(test)]
    pub(super) const fn performance_usage(&self) -> ScriptPlanUsage {
        self.usage
    }
}

impl ScriptExecutionPreviewItem {
    pub fn display_label(&self) -> &str {
        &self.display_label
    }

    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn destination_key(&self) -> &str {
        &self.destination_key
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptRunScope {
    All,
    CurrentDocument(u128),
    CurrentFile([u8; 32]),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct ScriptConfirmationToken {
    plan_digest: [u8; 32],
    scope: ScriptRunScope,
    token_digest: [u8; 32],
    consumed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ScriptConsumedConfirmation {
    plan_digest: [u8; 32],
    scope: ScriptRunScope,
    token_digest: [u8; 32],
}

impl ScriptConsumedConfirmation {
    pub(super) const fn scope(&self) -> &ScriptRunScope {
        &self.scope
    }

    pub(super) fn matches(&self, plan: &ScriptExecutionPlan) -> bool {
        self.plan_digest == plan.plan_digest && self.token_digest != [0; 32]
    }
}

impl ScriptConfirmationToken {
    pub(crate) fn consume_for(
        &mut self,
        plan: &ScriptExecutionPlan,
    ) -> Result<[u8; 32], ScriptPlanError> {
        if self.consumed {
            return Err(ScriptPlanError::ConfirmationConsumed);
        }
        if self.plan_digest != plan.plan_digest {
            return Err(ScriptPlanError::StaleConfirmation);
        }
        self.consumed = true;
        Ok(self.token_digest)
    }

    pub(super) fn consume_for_run(
        &mut self,
        plan: &ScriptExecutionPlan,
    ) -> Result<ScriptConsumedConfirmation, ScriptPlanError> {
        let token_digest = self.consume_for(plan)?;
        Ok(ScriptConsumedConfirmation {
            plan_digest: self.plan_digest,
            scope: self.scope.clone(),
            token_digest,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ScriptPlanError {
    Cancelled,
    AuthorityMismatch,
    MissingAuthority,
    DuplicateAuthority,
    StaleAuthority,
    Adapter(ScriptPlanAdapterError),
    InvalidPathIdentity,
    InvalidInput,
    StaleInput,
    DuplicateInput,
    OutputCollision,
    OpenSessionOverwrite,
    UnsupportedAtomicOverwrite,
    NumberOverflow,
    ResourceLimit,
    Asset(ScriptAssetError),
    InvalidScope,
    ConfirmationConsumed,
    StaleConfirmation,
}

impl From<ScriptPlanAdapterError> for ScriptPlanError {
    fn from(value: ScriptPlanAdapterError) -> Self {
        Self::Adapter(value)
    }
}

impl From<ScriptAssetError> for ScriptPlanError {
    fn from(value: ScriptAssetError) -> Self {
        match value {
            ScriptAssetError::Cancelled => Self::Cancelled,
            _ => Self::Asset(value),
        }
    }
}

/// Builds one immutable execution plan through the caller's OS/session adapter.
#[doc(hidden)]
pub fn plan_inkscript(
    program: &StaticScriptProgram,
    authority: &AuthoritySnapshot,
    adapter: &mut dyn ScriptPlanAdapter,
    asset_streams: &mut [AuthorizedAssetStream<'_>],
    limits: ScriptPlanLimits,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<ScriptExecutionPlan, ScriptPlanError> {
    poll_cancel(cancelled)?;
    let grants = validate_authority(program, authority)?;
    if adapter.authority_generation()? != authority.generation {
        return Err(ScriptPlanError::StaleAuthority);
    }
    poll_cancel(cancelled)?;
    let session_set = adapter.open_session_set()?;
    if session_set.generation != authority.open_session_set_generation {
        return Err(ScriptPlanError::StaleAuthority);
    }
    poll_cancel(cancelled)?;

    validate_asset_stream_authority(program, &grants, asset_streams)?;
    let frozen_assets = freeze_inkscript_assets(
        program.model.assets(),
        asset_streams,
        ScriptAssetLimits::exact_current(),
        cancelled,
    )?;

    let mut usage = ScriptPlanUsage {
        asset: frozen_assets.usage(),
        ..ScriptPlanUsage::default()
    };
    let mut items = Vec::new();
    for (input_index, declaration) in program.envelope.inputs().iter().enumerate() {
        poll_cancel(cancelled)?;
        match declaration.kind() {
            InkScriptInputDeclarationKind::File => {
                let grant = intent_grant(
                    program,
                    &grants,
                    &ScriptPathIntentSubject::Input(input_index),
                    InkScriptPathIntentAccess::Read,
                )?;
                let fingerprint = adapter.resolve_file(grant.intent_id, cancelled)?;
                if !fingerprint.path.same_existing_identity(&grant.resolved) {
                    return Err(ScriptPlanError::StaleInput);
                }
                add_native_bytes(&mut usage, fingerprint.logical_length, limits)?;
                if selected(declaration.cells(), fingerprint.display_number) {
                    items.push(file_to_planned(
                        input_index,
                        fingerprint,
                        &session_set,
                        adapter,
                        cancelled,
                        matches!(
                            program.envelope.output(),
                            InkScriptOutput::ExplicitOverwrite
                        ),
                    )?);
                }
            }
            InkScriptInputDeclarationKind::Folder => {
                let grant = intent_grant(
                    program,
                    &grants,
                    &ScriptPathIntentSubject::Input(input_index),
                    InkScriptPathIntentAccess::Enumerate,
                )?;
                let scan = adapter.enumerate_folder(grant.intent_id, cancelled)?;
                add_folder_usage(&mut usage, &scan, limits)?;
                for fingerprint in scan.matching_files {
                    poll_cancel(cancelled)?;
                    add_native_bytes(&mut usage, fingerprint.logical_length, limits)?;
                    if selected(declaration.cells(), fingerprint.display_number) {
                        items.push(file_to_planned(
                            input_index,
                            fingerprint,
                            &session_set,
                            adapter,
                            cancelled,
                            false,
                        )?);
                    }
                }
            }
            InkScriptInputDeclarationKind::CurrentDocument => {
                let expected = authority
                    .command_context
                    .current_document
                    .as_ref()
                    .ok_or(ScriptPlanError::StaleInput)?;
                let snapshot = adapter.capture_current_document(expected, cancelled)?;
                snapshot.validate_self()?;
                if !expected.matches(&snapshot) {
                    return Err(ScriptPlanError::StaleInput);
                }
                add_snapshot_bytes(&mut usage, snapshot.estimated_native_bytes, limits)?;
                items.push(session_to_planned(input_index, snapshot)?);
            }
            InkScriptInputDeclarationKind::CurrentSequence => {
                let expected = authority
                    .command_context
                    .current_sequence
                    .as_ref()
                    .ok_or(ScriptPlanError::StaleInput)?;
                let sequence = adapter.capture_current_sequence(expected, cancelled)?;
                if !expected.matches(&sequence) {
                    return Err(ScriptPlanError::StaleInput);
                }
                for member in sequence.members {
                    poll_cancel(cancelled)?;
                    let display_number = match &member {
                        ScriptSequenceMemberSnapshot::Session(value) => value.display_number,
                        ScriptSequenceMemberSnapshot::File { fingerprint, .. } => {
                            fingerprint.display_number
                        }
                    };
                    if !selected(declaration.cells(), display_number) {
                        continue;
                    }
                    match member {
                        ScriptSequenceMemberSnapshot::Session(snapshot) => {
                            snapshot.validate_self()?;
                            add_snapshot_bytes(
                                &mut usage,
                                snapshot.estimated_native_bytes,
                                limits,
                            )?;
                            items.push(session_to_planned(input_index, snapshot)?);
                        }
                        ScriptSequenceMemberSnapshot::File { fingerprint, .. } => {
                            add_native_bytes(&mut usage, fingerprint.logical_length, limits)?;
                            items.push(file_to_planned(
                                input_index,
                                fingerprint,
                                &session_set,
                                adapter,
                                cancelled,
                                false,
                            )?);
                        }
                    }
                }
            }
        }
        if items.len() as u64 > limits.expanded_inputs {
            return Err(ScriptPlanError::ResourceLimit);
        }
    }
    if items.is_empty() {
        return Err(ScriptPlanError::InvalidInput);
    }
    reject_duplicate_inputs(&items)?;
    items.sort_by(compare_inputs);
    validate_aggregate_plan(program, items.len(), &mut usage, limits)?;
    let requests = build_destination_requests(program, &items, &grants)?;
    let destinations = resolve_destinations(
        program,
        authority,
        &session_set,
        &items,
        requests,
        adapter,
        cancelled,
    )?;
    let preview = ScriptExecutionPreview {
        items: items
            .iter()
            .zip(&destinations)
            .map(|(input, destination)| ScriptExecutionPreviewItem {
                display_label: input.display_label.clone(),
                output_name: final_component(destination.canonical_key())
                    .unwrap_or_default()
                    .to_owned(),
                destination_key: destination.canonical_key.clone(),
            })
            .collect(),
    };
    let plan_digest = plan_digest(
        program,
        authority,
        &session_set,
        &items,
        &destinations,
        &frozen_assets,
    );
    Ok(ScriptExecutionPlan {
        items,
        destinations,
        frozen_assets,
        authority_generation: authority.generation,
        open_session_set_generation: session_set.generation,
        plan_digest,
        preview,
        usage,
        static_compile_digest: program.static_compile_digest,
        path_intent_digest: program.path_intent_digest,
    })
}

/// Issues one plan- and scope-bound confirmation token.
#[doc(hidden)]
pub fn issue_confirmation_token(
    plan: &ScriptExecutionPlan,
    scope: ScriptRunScope,
) -> Result<ScriptConfirmationToken, ScriptPlanError> {
    let matches = match &scope {
        ScriptRunScope::All => plan.items.len(),
        ScriptRunScope::CurrentDocument(uuid) => plan
            .items
            .iter()
            .filter(|item| item.document_uuid == *uuid)
            .count(),
        ScriptRunScope::CurrentFile(alias) => plan
            .items
            .iter()
            .filter(|item| item.path().is_some_and(|path| path.alias_key == *alias))
            .count(),
    };
    if !matches!(scope, ScriptRunScope::All) && matches != 1 {
        return Err(ScriptPlanError::InvalidScope);
    }
    let mut hasher = blake3::Hasher::new_derive_key(CONFIRMATION_DIGEST_CONTEXT);
    hasher.update(&plan.plan_digest);
    hasher.update(&plan.authority_generation.to_le_bytes());
    hasher.update(&plan.open_session_set_generation.to_le_bytes());
    match &scope {
        ScriptRunScope::All => {
            hasher.update(&[0]);
        }
        ScriptRunScope::CurrentDocument(uuid) => {
            hasher.update(&[1]);
            hasher.update(&uuid.to_le_bytes());
        }
        ScriptRunScope::CurrentFile(alias) => {
            hasher.update(&[2]);
            hasher.update(alias);
        }
    };
    Ok(ScriptConfirmationToken {
        plan_digest: plan.plan_digest,
        scope,
        token_digest: *hasher.finalize().as_bytes(),
        consumed: false,
    })
}

fn validate_authority<'a>(
    program: &StaticScriptProgram,
    authority: &'a AuthoritySnapshot,
) -> Result<BTreeMap<u64, &'a AuthorityGrant>, ScriptPlanError> {
    if authority.static_compile_digest != program.static_compile_digest
        || authority.path_intent_digest != program.path_intent_digest
        || authority.generation == 0
        || authority.grants.len() != program.path_intents.len()
    {
        return Err(ScriptPlanError::AuthorityMismatch);
    }
    let mut grants = BTreeMap::new();
    for grant in &authority.grants {
        if grant.generation != authority.generation
            || grants.insert(grant.intent_id, grant).is_some()
        {
            return Err(ScriptPlanError::DuplicateAuthority);
        }
    }
    for intent in &program.path_intents {
        let grant = grants
            .get(&intent.id())
            .ok_or(ScriptPlanError::MissingAuthority)?;
        if grant.access != intent.access() {
            return Err(ScriptPlanError::AuthorityMismatch);
        }
    }
    Ok(grants)
}

fn intent_grant<'a>(
    program: &StaticScriptProgram,
    grants: &'a BTreeMap<u64, &'a AuthorityGrant>,
    subject: &ScriptPathIntentSubject,
    access: InkScriptPathIntentAccess,
) -> Result<&'a AuthorityGrant, ScriptPlanError> {
    let intent = program
        .path_intents
        .iter()
        .find(|intent| intent.access() == access && intent.subject() == subject)
        .ok_or(ScriptPlanError::MissingAuthority)?;
    grants
        .get(&intent.id())
        .copied()
        .ok_or(ScriptPlanError::MissingAuthority)
}

fn validate_asset_stream_authority(
    program: &StaticScriptProgram,
    grants: &BTreeMap<u64, &AuthorityGrant>,
    streams: &[AuthorizedAssetStream<'_>],
) -> Result<(), ScriptPlanError> {
    for stream in streams {
        let subject = ScriptPathIntentSubject::Asset(stream.asset_symbol().to_owned());
        let grant = intent_grant(program, grants, &subject, InkScriptPathIntentAccess::Read)?;
        let identity = stream.authorized_identity();
        if grant.resolved.object_id() != Some(identity.object())
            || grant.resolved.object_generation != Some(identity.generation())
            || identity.logical_length() == 0
        {
            return Err(ScriptPlanError::StaleInput);
        }
    }
    Ok(())
}

fn add_folder_usage(
    usage: &mut ScriptPlanUsage,
    scan: &FolderScan,
    limits: ScriptPlanLimits,
) -> Result<(), ScriptPlanError> {
    usage.folder_entries = checked_add(usage.folder_entries, scan.observed_entries)?;
    usage.folder_name_bytes = checked_add(usage.folder_name_bytes, scan.normalized_name_bytes)?;
    usage.folder_work_units = checked_add(usage.folder_work_units, scan.work_units)?;
    if usage.folder_entries > limits.folder_entries
        || usage.folder_name_bytes > limits.folder_name_bytes
        || usage.folder_work_units > limits.folder_work_units
        || scan.maximum_depth > limits.folder_depth
    {
        return Err(ScriptPlanError::ResourceLimit);
    }
    Ok(())
}

fn add_native_bytes(
    usage: &mut ScriptPlanUsage,
    amount: u64,
    limits: ScriptPlanLimits,
) -> Result<(), ScriptPlanError> {
    usage.native_input_bytes = checked_add(usage.native_input_bytes, amount)?;
    if usage.native_input_bytes > limits.native_input_bytes {
        return Err(ScriptPlanError::ResourceLimit);
    }
    Ok(())
}

fn add_snapshot_bytes(
    usage: &mut ScriptPlanUsage,
    amount: u64,
    limits: ScriptPlanLimits,
) -> Result<(), ScriptPlanError> {
    usage.snapshot_input_bytes = checked_add(usage.snapshot_input_bytes, amount)?;
    if usage.snapshot_input_bytes > limits.planned_output_bytes {
        return Err(ScriptPlanError::ResourceLimit);
    }
    Ok(())
}

fn selected(selection: InkScriptCellSelection, number: u32) -> bool {
    match selection {
        InkScriptCellSelection::All => true,
        InkScriptCellSelection::Inclusive { first, last } => (first..=last).contains(&number),
    }
}

fn file_to_planned(
    input_index: usize,
    fingerprint: NativeInputFingerprint,
    session_set: &OpenSessionSetSnapshot,
    adapter: &mut dyn ScriptPlanAdapter,
    cancelled: &mut dyn FnMut() -> bool,
    overwrite: bool,
) -> Result<ScriptPlannedInput, ScriptPlanError> {
    if let Some(open) = session_set
        .sessions
        .iter()
        .find(|session| session.backing_path.aliases(&fingerprint.path))
    {
        if overwrite {
            return Err(ScriptPlanError::OpenSessionOverwrite);
        }
        let snapshot = adapter.capture_open_session(open, cancelled)?;
        snapshot.validate_self()?;
        if snapshot.session_id != open.session_id
            || snapshot.session_generation != open.session_generation
            || snapshot.document_uuid != open.document_uuid
            || snapshot
                .backing_path
                .as_ref()
                .is_none_or(|path| !path.aliases(&fingerprint.path))
        {
            return Err(ScriptPlanError::StaleInput);
        }
        return session_to_planned(input_index, snapshot);
    }
    let source_stem = source_stem(&fingerprint.display_label)?;
    Ok(ScriptPlannedInput {
        input_index,
        display_label: fingerprint.display_label.clone(),
        source_stem: Some(source_stem),
        path_order_key: fingerprint.path.canonical_key.clone(),
        document_uuid: fingerprint.document_uuid,
        source: PlannedInputSource::File(fingerprint),
    })
}

fn session_to_planned(
    input_index: usize,
    snapshot: ScriptSessionSnapshot,
) -> Result<ScriptPlannedInput, ScriptPlanError> {
    let source_stem = snapshot
        .backing_path
        .as_ref()
        .map(|_| source_stem(&snapshot.display_label))
        .transpose()?;
    Ok(ScriptPlannedInput {
        input_index,
        display_label: snapshot.display_label.clone(),
        source_stem,
        path_order_key: snapshot
            .backing_path
            .as_ref()
            .map_or_else(String::new, |path| path.canonical_key.clone()),
        document_uuid: snapshot.document_uuid,
        source: PlannedInputSource::Session(snapshot),
    })
}

fn reject_duplicate_inputs(items: &[ScriptPlannedInput]) -> Result<(), ScriptPlanError> {
    for (index, item) in items.iter().enumerate() {
        if items[..index].iter().any(|other| {
            item.document_uuid == other.document_uuid
                || match (item.path(), other.path()) {
                    (Some(left), Some(right)) => left.aliases(right),
                    _ => false,
                }
        }) {
            return Err(ScriptPlanError::DuplicateInput);
        }
    }
    Ok(())
}

fn compare_inputs(left: &ScriptPlannedInput, right: &ScriptPlannedInput) -> Ordering {
    crate::animation::natural_cmp(&left.display_label, &right.display_label)
        .then_with(|| {
            left.display_label
                .as_bytes()
                .cmp(right.display_label.as_bytes())
        })
        .then_with(|| {
            left.path_order_key
                .as_bytes()
                .cmp(right.path_order_key.as_bytes())
        })
        .then_with(|| {
            left.document_uuid
                .to_le_bytes()
                .cmp(&right.document_uuid.to_le_bytes())
        })
}

fn validate_aggregate_plan(
    program: &StaticScriptProgram,
    item_count: usize,
    usage: &mut ScriptPlanUsage,
    limits: ScriptPlanLimits,
) -> Result<(), ScriptPlanError> {
    let count = item_count as u64;
    let invocations = program
        .budget
        .max_invocations
        .checked_mul(count)
        .ok_or(ScriptPlanError::ResourceLimit)?;
    if invocations > limits.invocations {
        return Err(ScriptPlanError::ResourceLimit);
    }
    let wait = u64::from(program.envelope.execution().wait_ms())
        .checked_mul(count.saturating_sub(1))
        .ok_or(ScriptPlanError::ResourceLimit)?;
    if wait > u64::from(MAX_INKSCRIPT_WAIT_MS) {
        return Err(ScriptPlanError::ResourceLimit);
    }
    let output_per_item = usage
        .native_input_bytes
        .checked_add(usage.snapshot_input_bytes)
        .ok_or(ScriptPlanError::ResourceLimit)?
        .checked_add(
            program
                .budget
                .max_output_growth
                .checked_mul(count)
                .ok_or(ScriptPlanError::ResourceLimit)?,
        )
        .ok_or(ScriptPlanError::ResourceLimit)?;
    usage.planned_output_bytes = output_per_item
        .checked_mul(2)
        .ok_or(ScriptPlanError::ResourceLimit)?;
    if usage.planned_output_bytes > limits.planned_output_bytes {
        return Err(ScriptPlanError::ResourceLimit);
    }
    Ok(())
}

fn build_destination_requests(
    program: &StaticScriptProgram,
    items: &[ScriptPlannedInput],
    grants: &BTreeMap<u64, &AuthorityGrant>,
) -> Result<Vec<Option<ScriptDestinationRequest>>, ScriptPlanError> {
    let mut output = Vec::with_capacity(items.len());
    match program.envelope.output() {
        InkScriptOutput::ExplicitOverwrite => {
            for item in items {
                let PlannedInputSource::File(fingerprint) = &item.source else {
                    return Err(ScriptPlanError::OpenSessionOverwrite);
                };
                if !fingerprint.supports_atomic_overwrite || fingerprint.change_token.is_none() {
                    return Err(ScriptPlanError::UnsupportedAtomicOverwrite);
                }
                let replace = intent_grant(
                    program,
                    grants,
                    &ScriptPathIntentSubject::Input(item.input_index),
                    InkScriptPathIntentAccess::Replace,
                )?;
                if !replace.resolved.same_existing_identity(&fingerprint.path) {
                    return Err(ScriptPlanError::StaleInput);
                }
                output.push(None);
            }
        }
        InkScriptOutput::Duplicate(settings) | InkScriptOutput::NewSave(settings) => {
            let root = intent_grant(
                program,
                grants,
                &ScriptPathIntentSubject::OutputRoot,
                InkScriptPathIntentAccess::Create,
            )?;
            for (ordinal, item) in items.iter().enumerate() {
                let number = match settings.direction() {
                    InkScriptNumberDirection::Ascending => settings
                        .start_number()
                        .checked_add(ordinal as u32)
                        .ok_or(ScriptPlanError::NumberOverflow)?,
                    InkScriptNumberDirection::Descending => settings
                        .start_number()
                        .checked_sub(ordinal as u32)
                        .ok_or(ScriptPlanError::NumberOverflow)?,
                };
                let name = output_name(program.envelope.output(), item, number)?;
                let mut relative_components = Vec::new();
                if settings.cell_folder() {
                    relative_components.push(
                        item.source_stem
                            .clone()
                            .ok_or(ScriptPlanError::InvalidInput)?,
                    );
                }
                relative_components.push(name);
                let base = if settings.folder().is_empty() {
                    let path = item.path().ok_or(ScriptPlanError::InvalidInput)?;
                    if root.resolved.alias_key != path.parent_alias_key {
                        return Err(ScriptPlanError::AuthorityMismatch);
                    }
                    ScriptDestinationBase::InputParent {
                        input_path: path.clone(),
                    }
                } else {
                    ScriptDestinationBase::AuthorizedRoot {
                        intent_id: root.intent_id,
                        root: root.resolved.clone(),
                    }
                };
                output.push(Some(ScriptDestinationRequest {
                    base,
                    relative_components,
                }));
            }
        }
    }
    Ok(output)
}

fn output_name(
    output: &InkScriptOutput,
    item: &ScriptPlannedInput,
    number: u32,
) -> Result<String, ScriptPlanError> {
    let value = match output {
        InkScriptOutput::Duplicate(settings) if settings.basename().is_empty() => format!(
            "{}_batch.inkpod",
            item.source_stem
                .as_deref()
                .ok_or(ScriptPlanError::InvalidInput)?
        ),
        InkScriptOutput::Duplicate(settings) | InkScriptOutput::NewSave(settings) => {
            let basename = if settings.basename().is_empty() {
                "cell"
            } else {
                settings.basename()
            };
            format!("{basename}_{number:04}.inkpod")
        }
        InkScriptOutput::ExplicitOverwrite => return Err(ScriptPlanError::InvalidInput),
    };
    if !valid_native_filename(&value) {
        return Err(ScriptPlanError::InvalidInput);
    }
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
fn resolve_destinations(
    program: &StaticScriptProgram,
    authority: &AuthoritySnapshot,
    session_set: &OpenSessionSetSnapshot,
    items: &[ScriptPlannedInput],
    requests: Vec<Option<ScriptDestinationRequest>>,
    adapter: &mut dyn ScriptPlanAdapter,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Vec<ValidatedPathIdentity>, ScriptPlanError> {
    let mut destinations = Vec::with_capacity(items.len());
    for (item, request) in items.iter().zip(requests) {
        poll_cancel(cancelled)?;
        let destination = match request {
            None => item.path().cloned().ok_or(ScriptPlanError::InvalidInput)?,
            Some(request) => {
                let path = adapter.resolve_destination(&request, cancelled)?;
                path.validate()?;
                if !path.expected_absent {
                    return Err(ScriptPlanError::OutputCollision);
                }
                path
            }
        };
        if matches!(
            program.envelope.output(),
            InkScriptOutput::ExplicitOverwrite
        ) {
            if session_set
                .sessions
                .iter()
                .any(|session| session.backing_path.aliases(&destination))
            {
                return Err(ScriptPlanError::OpenSessionOverwrite);
            }
        } else if items
            .iter()
            .filter_map(ScriptPlannedInput::path)
            .any(|path| path.aliases(&destination))
            || session_set
                .sessions
                .iter()
                .any(|session| session.backing_path.aliases(&destination))
            || authority
                .script_path
                .as_ref()
                .is_some_and(|path| path.aliases(&destination))
            || authority.grants.iter().any(|grant| {
                matches!(
                    program
                        .path_intents
                        .iter()
                        .find(|intent| intent.id() == grant.intent_id)
                        .map(ScriptStaticPathIntent::subject),
                    Some(ScriptPathIntentSubject::Asset(_))
                ) && grant.resolved.aliases(&destination)
            })
        {
            return Err(ScriptPlanError::OutputCollision);
        }
        if destinations
            .iter()
            .any(|other: &ValidatedPathIdentity| other.aliases(&destination))
        {
            return Err(ScriptPlanError::OutputCollision);
        }
        destinations.push(destination);
    }
    Ok(destinations)
}

fn plan_digest(
    program: &StaticScriptProgram,
    authority: &AuthoritySnapshot,
    session_set: &OpenSessionSetSnapshot,
    items: &[ScriptPlannedInput],
    destinations: &[ValidatedPathIdentity],
    assets: &FrozenScriptAssets,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(PLAN_DIGEST_CONTEXT);
    hasher.update(&program.static_compile_digest);
    hasher.update(&program.path_intent_digest);
    hasher.update(&authority.generation.to_le_bytes());
    hasher.update(&session_set.generation.to_le_bytes());
    for grant in &authority.grants {
        hasher.update(&grant.intent_id.to_le_bytes());
        hasher.update(&grant.authority_id);
        hasher.update(&grant.generation.to_le_bytes());
    }
    for item in items {
        hasher.update(&(item.input_index as u64).to_le_bytes());
        hash_bytes(&mut hasher, item.display_label.as_bytes());
        hasher.update(&item.document_uuid.to_le_bytes());
        match &item.source {
            PlannedInputSource::Session(snapshot) => {
                hasher.update(&[0]);
                hasher.update(&snapshot.session_id.to_le_bytes());
                hasher.update(&snapshot.session_generation.to_le_bytes());
                hasher.update(&snapshot.source_generation.to_le_bytes());
                hasher.update(&snapshot.document_revision.to_le_bytes());
                hasher.update(snapshot.state_digest.as_bytes());
                hasher.update(&snapshot.editor_revision.to_le_bytes());
                hasher.update(snapshot.editor_digest.as_bytes());
                hasher.update(&snapshot.estimated_native_bytes.to_le_bytes());
                if let Some(path) = &snapshot.backing_path {
                    path.hash_into(&mut hasher);
                }
            }
            PlannedInputSource::File(fingerprint) => {
                hasher.update(&[1]);
                fingerprint.path.hash_into(&mut hasher);
                hasher.update(&fingerprint.logical_length.to_le_bytes());
                hasher.update(&fingerprint.content_digest);
                hasher.update(fingerprint.change_token.as_ref().unwrap_or(&[0; 32]));
            }
        }
    }
    for destination in destinations {
        destination.hash_into(&mut hasher);
    }
    for asset in assets.plan_records() {
        hash_bytes(&mut hasher, asset.symbol.as_bytes());
        hasher.update(asset.asset_id.as_bytes());
        hasher.update(&asset.descriptor.logical_payload_length.to_le_bytes());
        hasher.update(&[match asset.source {
            ScriptAssetSource::Inline => 0,
            ScriptAssetSource::Authorized => 1,
        }]);
        if let Some(identity) = asset.authorized_identity {
            hasher.update(&identity.object());
            hasher.update(&identity.generation().to_le_bytes());
            hasher.update(&identity.logical_length().to_le_bytes());
        }
    }
    *hasher.finalize().as_bytes()
}

fn checked_add(left: u64, right: u64) -> Result<u64, ScriptPlanError> {
    left.checked_add(right)
        .ok_or(ScriptPlanError::ResourceLimit)
}

fn poll_cancel(cancelled: &mut dyn FnMut() -> bool) -> Result<(), ScriptPlanError> {
    if cancelled() {
        Err(ScriptPlanError::Cancelled)
    } else {
        Ok(())
    }
}

fn source_stem(label: &str) -> Result<String, ScriptPlanError> {
    let stem = label
        .strip_suffix(".inkpod")
        .filter(|value| !value.is_empty())
        .ok_or(ScriptPlanError::InvalidInput)?;
    if !valid_component(stem) {
        return Err(ScriptPlanError::InvalidInput);
    }
    Ok(stem.to_owned())
}

fn valid_native_filename(value: &str) -> bool {
    value.len() <= MAX_INKSCRIPT_STRING_BYTES
        && value
            .strip_suffix(".inkpod")
            .is_some_and(|stem| !stem.is_empty() && valid_component(stem))
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.ends_with([' ', '.'])
        && !value
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
}

fn final_component(path: &str) -> Option<&str> {
    path.rsplit('/').next().filter(|value| !value.is_empty())
}

fn hash_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetStore, RasterAssetInput};
    use crate::script::assets::{
        AuthorizedAssetIdentity, AuthorizedAssetReadError, AuthorizedAssetReader,
    };
    use crate::script::compile::compile_inkscript;
    use crate::{AssetAlphaSemantics, AssetColorSpace, Core, DEFAULT_DPI_MILLI, PixelFormat};
    use inkpod_format::{InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId};
    use std::collections::BTreeMap;

    fn source(inputs: &str, output: &str, assets: &str) -> InkScriptSource {
        InkScriptSource::new(
            InkScriptSourceId::new(211),
            format!(
                r#"inkscript 2;
requires {{ procedure_catalog = 2; replay_epoch = 23; }}
inputs {{ {inputs} }}
program {{}}
output {{ {output} }}
execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}
assets {{ {assets} }}
"#
            )
            .as_bytes(),
        )
        .unwrap()
    }

    fn duplicate_output(basename: &str, start: u32, descending: bool) -> String {
        format!(
            "policy = duplicate; format = inkpod; folder = \"out\"; cell_folder = false; basename = \"{basename}\"; start_number = {start}; direction = {};",
            if descending {
                "descending"
            } else {
                "ascending"
            }
        )
    }

    fn compile(inputs: &str, output: &str, assets: &str) -> StaticScriptProgram {
        compile_inkscript(
            &source(inputs, output, assets),
            InkScriptRunParameterDecision::Resolve(Vec::new()),
        )
        .unwrap()
    }

    fn existing(key: &str, object: u8, parent: u8) -> ValidatedPathIdentity {
        ValidatedPathIdentity::existing(
            key.to_owned(),
            [1; 16],
            [object; 32],
            alias(key),
            [parent; 32],
            alias(&format!("{key}/..")),
        )
        .unwrap()
    }

    fn absent(key: &str, parent: u8) -> ValidatedPathIdentity {
        ValidatedPathIdentity::expected_absent(
            key.to_owned(),
            [1; 16],
            [parent; 32],
            alias(key),
            alias(&format!("{key}/..")),
        )
        .unwrap()
    }

    fn alias(text: &str) -> [u8; 32] {
        *blake3::hash(text.as_bytes()).as_bytes()
    }

    fn file(key: &str, label: &str, number: u32, object: u8) -> NativeInputFingerprint {
        NativeInputFingerprint::new(
            existing(key, object, object.wrapping_add(80)),
            label.to_owned(),
            number,
            u128::from(object) + 10,
            128,
            alias(&format!("content:{key}")),
            Some(alias(&format!("change:{key}"))),
            true,
        )
        .unwrap()
    }

    fn grant_for(intent: &ScriptStaticPathIntent) -> AuthorityGrant {
        let resolved = match intent.subject() {
            ScriptPathIntentSubject::Input(index) => existing(
                &format!("root:/intent/input/{index}"),
                10 + *index as u8,
                70,
            ),
            ScriptPathIntentSubject::Asset(name) => {
                existing(&format!("root:/asset/{name}"), 50, 71)
            }
            ScriptPathIntentSubject::OutputRoot => existing("root:/out", 60, 72),
        };
        AuthorityGrant::new(
            intent.id(),
            intent.access(),
            alias(&format!("authority:{}", intent.id())),
            9,
            resolved,
        )
        .unwrap()
    }

    fn authority(
        program: &StaticScriptProgram,
        context: ScriptCommandContext,
        session_set_generation: u64,
    ) -> AuthoritySnapshot {
        AuthoritySnapshot::new(
            program.static_compile_digest,
            program.path_intent_digest,
            9,
            program.path_intents.iter().map(grant_for).collect(),
            context,
            session_set_generation,
            None,
        )
        .unwrap()
    }

    #[derive(Default)]
    struct TestAdapter {
        authority_generation: u64,
        session_set: OpenSessionSetSnapshot,
        files: BTreeMap<u64, NativeInputFingerprint>,
        folders: BTreeMap<u64, FolderScan>,
        current_document: Option<ScriptSessionSnapshot>,
        current_sequence: Option<ScriptSequenceSnapshot>,
        open_sessions: BTreeMap<u64, ScriptSessionSnapshot>,
        destination_calls: u64,
        fail_destination: bool,
        destination_override: Option<ValidatedPathIdentity>,
    }

    impl ScriptPlanAdapter for TestAdapter {
        fn authority_generation(&mut self) -> Result<u64, ScriptPlanAdapterError> {
            Ok(self.authority_generation)
        }

        fn open_session_set(&mut self) -> Result<OpenSessionSetSnapshot, ScriptPlanAdapterError> {
            Ok(self.session_set.clone())
        }

        fn resolve_file(
            &mut self,
            intent_id: u64,
            _cancelled: &mut dyn FnMut() -> bool,
        ) -> Result<NativeInputFingerprint, ScriptPlanAdapterError> {
            self.files
                .get(&intent_id)
                .cloned()
                .ok_or(ScriptPlanAdapterError::Unavailable)
        }

        fn enumerate_folder(
            &mut self,
            intent_id: u64,
            _cancelled: &mut dyn FnMut() -> bool,
        ) -> Result<FolderScan, ScriptPlanAdapterError> {
            self.folders
                .get(&intent_id)
                .cloned()
                .ok_or(ScriptPlanAdapterError::Unavailable)
        }

        fn capture_current_document(
            &mut self,
            _expected: &ScriptSessionExpectation,
            _cancelled: &mut dyn FnMut() -> bool,
        ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
            self.current_document
                .clone()
                .ok_or(ScriptPlanAdapterError::Unavailable)
        }

        fn capture_current_sequence(
            &mut self,
            _expected: &ScriptSequenceExpectation,
            _cancelled: &mut dyn FnMut() -> bool,
        ) -> Result<ScriptSequenceSnapshot, ScriptPlanAdapterError> {
            self.current_sequence
                .clone()
                .ok_or(ScriptPlanAdapterError::Unavailable)
        }

        fn capture_open_session(
            &mut self,
            session: &OpenSessionRecord,
            _cancelled: &mut dyn FnMut() -> bool,
        ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
            self.open_sessions
                .get(&session.session_id)
                .cloned()
                .ok_or(ScriptPlanAdapterError::Unavailable)
        }

        fn resolve_destination(
            &mut self,
            request: &ScriptDestinationRequest,
            _cancelled: &mut dyn FnMut() -> bool,
        ) -> Result<ValidatedPathIdentity, ScriptPlanAdapterError> {
            self.destination_calls += 1;
            if self.fail_destination {
                return Err(ScriptPlanAdapterError::Failure);
            }
            if let Some(path) = &self.destination_override {
                return Ok(path.clone());
            }
            let key = format!("root:/out/{}", request.relative_components.join("/"));
            Ok(absent(&key, 60))
        }
    }

    fn ready_adapter() -> TestAdapter {
        TestAdapter {
            authority_generation: 9,
            session_set: OpenSessionSetSnapshot::new(4, Vec::new()).unwrap(),
            ..TestAdapter::default()
        }
    }

    #[test]
    fn plan_orders_filters_and_names_like_the_batch_contract_then_consumes_confirmation_once() {
        let program = compile(
            r#"folder "in" { cells = range(1, 20); recursive = false; };"#,
            &duplicate_output("", 7, false),
            "",
        );
        let folder_intent = program
            .path_intents
            .iter()
            .find(|intent| matches!(intent.subject(), ScriptPathIntentSubject::Input(0)))
            .unwrap();
        let mut auth = authority(&program, ScriptCommandContext::default(), 4);
        auth.grants
            .iter_mut()
            .find(|grant| grant.intent_id == folder_intent.id())
            .unwrap()
            .resolved = existing("root:/in", 10, 70);
        let mut adapter = ready_adapter();
        adapter.folders.insert(
            folder_intent.id(),
            FolderScan::new(
                7,
                80,
                8,
                1,
                vec![
                    file("root:/in/cell10.inkpod", "cell10.inkpod", 10, 3),
                    file("root:/in/Cell2.inkpod", "Cell2.inkpod", 2, 2),
                    file("root:/in/cell1.inkpod", "cell1.inkpod", 1, 1),
                    file("root:/in/cell21.inkpod", "cell21.inkpod", 21, 4),
                ],
            )
            .unwrap(),
        );
        let mut never_cancel = || false;
        let plan = plan_inkscript(
            &program,
            &auth,
            &mut adapter,
            &mut [],
            ScriptPlanLimits::exact_current(),
            &mut never_cancel,
        )
        .unwrap();

        assert_eq!(
            plan.preview
                .items
                .iter()
                .map(|item| (item.display_label.as_str(), item.output_name.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("cell1.inkpod", "cell1_batch.inkpod"),
                ("Cell2.inkpod", "Cell2_batch.inkpod"),
                ("cell10.inkpod", "cell10_batch.inkpod"),
            ]
        );
        assert_eq!(plan.usage.folder_entries, 7);
        assert_eq!(plan.usage.folder_name_bytes, 80);
        assert_eq!(adapter.destination_calls, 3);
        assert_ne!(plan.plan_digest, program.static_compile_digest);

        let mut token = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
        let current_alias = plan.items[0].path().unwrap().alias_key;
        let current_token =
            issue_confirmation_token(&plan, ScriptRunScope::CurrentFile(current_alias)).unwrap();
        assert_ne!(token.token_digest, current_token.token_digest);
        assert!(matches!(
            issue_confirmation_token(&plan, ScriptRunScope::CurrentDocument(u128::MAX)),
            Err(ScriptPlanError::InvalidScope)
        ));
        assert!(token.consume_for(&plan).is_ok());
        assert_eq!(
            token.consume_for(&plan),
            Err(ScriptPlanError::ConfirmationConsumed)
        );
        fn assert_send<T: Send>() {}
        assert_send::<ScriptExecutionPlan>();

        let mut changed_authority = auth.clone();
        changed_authority.grants[0].authority_id = [0xA5; 32];
        let changed_plan = plan_inkscript(
            &program,
            &changed_authority,
            &mut adapter,
            &mut [],
            ScriptPlanLimits::exact_current(),
            &mut never_cancel,
        )
        .unwrap();
        assert_eq!(program.static_compile_digest, auth.static_compile_digest);
        assert_eq!(program.path_intent_digest, auth.path_intent_digest);
        assert_ne!(changed_plan.plan_digest, plan.plan_digest);
    }

    #[test]
    fn dirty_pathless_current_snapshot_is_frozen_and_stale_document_or_sequence_is_rejected() {
        let mut live = Core::new();
        live.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let layer = live.document_info().unwrap().layer_id;
        live.set_layer_properties(layer, true, true, 1_000, "dirty")
            .unwrap();
        live.current_path = Some(std::path::PathBuf::from("frontend-owned.inkpod"));
        let current = ScriptSessionSnapshot::capture(
            12,
            3,
            1,
            "current-cell.inkpod".to_owned(),
            1,
            None,
            &live,
        )
        .unwrap();
        let expectation = ScriptSessionExpectation::from_snapshot(&current).unwrap();
        let before = (
            live.document_state_digest().unwrap(),
            live.document_info().unwrap(),
            live.history_entries(),
        );
        let program = compile(
            "current_document;",
            &duplicate_output("named", 1, false),
            "",
        );
        let context = ScriptCommandContext {
            current_document: Some(expectation.clone()),
            current_sequence: None,
        };
        let auth = authority(&program, context, 4);
        let mut adapter = ready_adapter();
        adapter.current_document = Some(current.clone());
        let mut never_cancel = || false;
        let plan = plan_inkscript(
            &program,
            &auth,
            &mut adapter,
            &mut [],
            ScriptPlanLimits::exact_current(),
            &mut never_cancel,
        )
        .unwrap();
        let PlannedInputSource::Session(snapshot) = &plan.items[0].source else {
            panic!("current document must be an immutable session snapshot")
        };
        assert!(snapshot.core.document_info().unwrap().dirty);
        assert!(snapshot.core.current_path.is_none());
        assert!(live.current_path.is_some());
        assert_eq!(snapshot.core.history_entries(), live.history_entries());
        assert_eq!(live.document_state_digest().unwrap(), before.0);
        assert_eq!(live.document_info().unwrap(), before.1);
        assert_eq!(live.history_entries(), before.2);

        let mut stale_adapter = ready_adapter();
        let mut stale = current;
        stale.session_generation += 1;
        stale_adapter.current_document = Some(stale);
        assert!(matches!(
            plan_inkscript(
                &program,
                &auth,
                &mut stale_adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::StaleInput)
        ));

        let sequence_program = compile(
            "current_sequence { cells = all; };",
            &duplicate_output("seq", 1, false),
            "",
        );
        let sequence =
            ScriptSequenceSnapshot::new(21, 5, vec![current_for_sequence(&live, 31, 1)]).unwrap();
        let sequence_expectation = ScriptSequenceExpectation::from_snapshot(&sequence).unwrap();
        let sequence_auth = authority(
            &sequence_program,
            ScriptCommandContext {
                current_document: None,
                current_sequence: Some(sequence_expectation),
            },
            4,
        );
        let mut sequence_adapter = ready_adapter();
        let mut changed_sequence = sequence;
        changed_sequence.generation += 1;
        sequence_adapter.current_sequence = Some(changed_sequence);
        assert!(matches!(
            plan_inkscript(
                &sequence_program,
                &sequence_auth,
                &mut sequence_adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::StaleInput)
        ));
    }

    fn current_for_sequence(
        core: &Core,
        session: u64,
        display: u32,
    ) -> ScriptSequenceMemberSnapshot {
        ScriptSequenceMemberSnapshot::Session(
            ScriptSessionSnapshot::capture(
                session,
                1,
                1,
                format!("cell{display}.inkpod"),
                display,
                Some(existing(
                    &format!("root:/seq/cell{display}.inkpod"),
                    display as u8,
                    90,
                )),
                core,
            )
            .unwrap(),
        )
    }

    #[test]
    fn alias_replacement_open_session_and_external_asset_change_fail_before_plan_publication() {
        let overwrite = "policy = explicit_overwrite; format = inkpod;";
        let program = compile(r#"file "one.inkpod";"#, overwrite, "");
        let input_intent = program
            .path_intents
            .iter()
            .find(|intent| matches!(intent.subject(), ScriptPathIntentSubject::Input(0)))
            .unwrap();
        let input = file("root:/one.inkpod", "one.inkpod", 1, 7);
        let mut auth = authority(&program, ScriptCommandContext::default(), 4);
        auth.grants
            .iter_mut()
            .find(|grant| grant.intent_id == input_intent.id())
            .unwrap()
            .resolved = input.path.clone();
        let open = OpenSessionRecord::new(99, 2, input.document_uuid, input.path.clone()).unwrap();
        let mut adapter = ready_adapter();
        adapter.session_set = OpenSessionSetSnapshot::new(4, vec![open]).unwrap();
        adapter.files.insert(input_intent.id(), input);
        let mut never_cancel = || false;
        assert!(matches!(
            plan_inkscript(
                &program,
                &auth,
                &mut adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::OpenSessionOverwrite)
        ));

        let mut replacement_adapter = ready_adapter();
        replacement_adapter.files.insert(
            input_intent.id(),
            file("root:/one.inkpod", "one.inkpod", 1, 8),
        );
        assert!(matches!(
            plan_inkscript(
                &program,
                &auth,
                &mut replacement_adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::StaleInput)
        ));

        let duplicate_program = compile(
            r#"file "one.inkpod"; file "alias.inkpod";"#,
            &duplicate_output("copy", 1, false),
            "",
        );
        let mut duplicate_auth = authority(&duplicate_program, ScriptCommandContext::default(), 4);
        let shared = file("root:/one.inkpod", "one.inkpod", 1, 7);
        let input_intents = duplicate_program
            .path_intents
            .iter()
            .filter(|intent| {
                intent.access() == InkScriptPathIntentAccess::Read
                    && matches!(intent.subject(), ScriptPathIntentSubject::Input(_))
            })
            .collect::<Vec<_>>();
        let mut duplicate_adapter = ready_adapter();
        for intent in input_intents {
            duplicate_auth
                .grants
                .iter_mut()
                .find(|grant| grant.intent_id == intent.id())
                .unwrap()
                .resolved = shared.path.clone();
            duplicate_adapter.files.insert(intent.id(), shared.clone());
        }
        assert!(matches!(
            plan_inkscript(
                &duplicate_program,
                &duplicate_auth,
                &mut duplicate_adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::DuplicateInput)
        ));

        let collision_program = compile(
            r#"file "one.inkpod";"#,
            &duplicate_output("copy", 1, false),
            "",
        );
        let collision_intent = collision_program
            .path_intents
            .iter()
            .find(|intent| {
                intent.access() == InkScriptPathIntentAccess::Read
                    && matches!(intent.subject(), ScriptPathIntentSubject::Input(0))
            })
            .unwrap();
        let collision_input = file("root:/one.inkpod", "one.inkpod", 1, 7);
        let mut collision_auth = authority(&collision_program, ScriptCommandContext::default(), 4);
        collision_auth
            .grants
            .iter_mut()
            .find(|grant| grant.intent_id == collision_intent.id())
            .unwrap()
            .resolved = collision_input.path.clone();
        let mut collision_adapter = ready_adapter();
        collision_adapter
            .files
            .insert(collision_intent.id(), collision_input);
        collision_adapter.destination_override = Some(absent("root:/one.inkpod", 60));
        assert!(matches!(
            plan_inkscript(
                &collision_program,
                &collision_auth,
                &mut collision_adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::OutputCollision)
        ));

        let asset_bytes = [1_u8, 2, 3, 4];
        let asset_id = rgba_asset_id(&asset_bytes);
        let asset_source = format!(
            r#"asset paint {{
kind = "canonical_raster";
asset_id = blake3"{}";
descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 4; element_count = 1; }};
data_file = "paint.bin";
}};"#,
            hex(asset_id.as_bytes())
        );
        let asset_program = compile(
            "current_document;",
            &duplicate_output("asset", 1, false),
            &asset_source,
        );
        let mut core = Core::new();
        core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let current = ScriptSessionSnapshot::capture(
            2,
            1,
            1,
            "current-cell.inkpod".to_owned(),
            1,
            None,
            &core,
        )
        .unwrap();
        let asset_auth = authority(
            &asset_program,
            ScriptCommandContext {
                current_document: Some(ScriptSessionExpectation::from_snapshot(&current).unwrap()),
                current_sequence: None,
            },
            4,
        );
        let asset_intent = asset_program
            .path_intents
            .iter()
            .find(|intent| matches!(intent.subject(), ScriptPathIntentSubject::Asset(_)))
            .unwrap();
        let expected_object = asset_auth
            .grants
            .iter()
            .find(|grant| grant.intent_id == asset_intent.id())
            .unwrap()
            .resolved
            .object_id()
            .unwrap();
        let expected_generation = asset_auth
            .grants
            .iter()
            .find(|grant| grant.intent_id == asset_intent.id())
            .unwrap()
            .resolved
            .object_generation
            .unwrap();
        let authorized = AuthorizedAssetIdentity::new(expected_object, expected_generation, 4);
        let mut reader = MemoryAssetReader {
            bytes: asset_bytes.to_vec(),
            cursor: 0,
            before: authorized,
            after: AuthorizedAssetIdentity::new([0xEE; 32], expected_generation, 4),
        };
        let mut streams = [AuthorizedAssetStream::new("paint", authorized, &mut reader)];
        let mut asset_adapter = ready_adapter();
        asset_adapter.current_document = Some(current);
        assert!(matches!(
            plan_inkscript(
                &asset_program,
                &asset_auth,
                &mut asset_adapter,
                &mut streams,
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::Asset(
                ScriptAssetError::StaleAuthorizedStream
            ))
        ));
    }

    #[test]
    fn checked_numbering_folder_resource_and_cancel_fail_without_destination_side_effects() {
        for invalid_input in [
            r#"file "../outside.inkpod";"#,
            r#"file "https://example.invalid/cell.inkpod";"#,
            r#"file "";"#,
        ] {
            assert!(matches!(
                compile_inkscript(
                    &source(invalid_input, &duplicate_output("named", 1, false), ""),
                    InkScriptRunParameterDecision::Resolve(Vec::new()),
                ),
                Err(super::super::compile::ScriptCompileError::InvalidPathIntent)
            ));
        }

        let overflow_program = compile(
            r#"folder "in" { cells = all; recursive = false; };"#,
            &duplicate_output("named", u32::MAX, false),
            "",
        );
        let intent = overflow_program
            .path_intents
            .iter()
            .find(|intent| matches!(intent.subject(), ScriptPathIntentSubject::Input(0)))
            .unwrap();
        let mut auth = authority(&overflow_program, ScriptCommandContext::default(), 4);
        auth.grants
            .iter_mut()
            .find(|grant| grant.intent_id == intent.id())
            .unwrap()
            .resolved = existing("root:/in", 10, 70);
        let mut adapter = ready_adapter();
        adapter.folders.insert(
            intent.id(),
            FolderScan::new(
                2,
                20,
                3,
                1,
                vec![
                    file("root:/in/a1.inkpod", "a1.inkpod", 1, 1),
                    file("root:/in/a2.inkpod", "a2.inkpod", 2, 2),
                ],
            )
            .unwrap(),
        );
        let mut never_cancel = || false;
        assert!(matches!(
            plan_inkscript(
                &overflow_program,
                &auth,
                &mut adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::NumberOverflow)
        ));
        assert_eq!(adapter.destination_calls, 0);

        let underflow_program = compile(
            r#"folder "in" { cells = all; recursive = false; };"#,
            &duplicate_output("named", 0, true),
            "",
        );
        let underflow_intent = underflow_program
            .path_intents
            .iter()
            .find(|intent| matches!(intent.subject(), ScriptPathIntentSubject::Input(0)))
            .unwrap();
        let mut underflow_auth = authority(&underflow_program, ScriptCommandContext::default(), 4);
        underflow_auth
            .grants
            .iter_mut()
            .find(|grant| grant.intent_id == underflow_intent.id())
            .unwrap()
            .resolved = existing("root:/in", 10, 70);
        let mut underflow_adapter = ready_adapter();
        underflow_adapter.folders.insert(
            underflow_intent.id(),
            FolderScan::new(
                2,
                20,
                3,
                1,
                vec![
                    file("root:/in/a1.inkpod", "a1.inkpod", 1, 1),
                    file("root:/in/a2.inkpod", "a2.inkpod", 2, 2),
                ],
            )
            .unwrap(),
        );
        assert!(matches!(
            plan_inkscript(
                &underflow_program,
                &underflow_auth,
                &mut underflow_adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::NumberOverflow)
        ));
        assert_eq!(underflow_adapter.destination_calls, 0);

        let mut limited_adapter = ready_adapter();
        limited_adapter.folders.insert(
            intent.id(),
            FolderScan::new(6, 60, 7, 1, Vec::new()).unwrap(),
        );
        assert!(matches!(
            plan_inkscript(
                &overflow_program,
                &auth,
                &mut limited_adapter,
                &mut [],
                ScriptPlanLimits::exact_current().with_folder_entries(5),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::ResourceLimit)
        ));
        assert_eq!(limited_adapter.destination_calls, 0);

        for scan in [
            FolderScan::new(1, MAX_FOLDER_NAME_BYTES + 1, 2, 1, Vec::new()).unwrap(),
            FolderScan::new(1, 1, MAX_FOLDER_WORK_UNITS + 1, 1, Vec::new()).unwrap(),
            FolderScan::new(1, 1, 2, MAX_FOLDER_DEPTH + 1, Vec::new()).unwrap(),
        ] {
            let mut resource_adapter = ready_adapter();
            resource_adapter.folders.insert(intent.id(), scan);
            assert!(matches!(
                plan_inkscript(
                    &overflow_program,
                    &auth,
                    &mut resource_adapter,
                    &mut [],
                    ScriptPlanLimits::exact_current(),
                    &mut never_cancel,
                ),
                Err(ScriptPlanError::ResourceLimit)
            ));
            assert_eq!(resource_adapter.destination_calls, 0);
        }

        let failure_program = compile(
            r#"folder "in" { cells = all; recursive = false; };"#,
            &duplicate_output("named", 1, false),
            "",
        );
        let failure_intent = failure_program
            .path_intents
            .iter()
            .find(|intent| matches!(intent.subject(), ScriptPathIntentSubject::Input(0)))
            .unwrap();
        let mut failure_auth = authority(&failure_program, ScriptCommandContext::default(), 4);
        failure_auth
            .grants
            .iter_mut()
            .find(|grant| grant.intent_id == failure_intent.id())
            .unwrap()
            .resolved = existing("root:/in", 10, 70);
        let mut failure_adapter = ready_adapter();
        failure_adapter.fail_destination = true;
        failure_adapter.folders.insert(
            failure_intent.id(),
            FolderScan::new(
                1,
                10,
                2,
                1,
                vec![file("root:/in/a1.inkpod", "a1.inkpod", 1, 1)],
            )
            .unwrap(),
        );
        assert!(matches!(
            plan_inkscript(
                &failure_program,
                &failure_auth,
                &mut failure_adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptPlanError::Adapter(ScriptPlanAdapterError::Failure))
        ));
        assert_eq!(failure_adapter.destination_calls, 1);

        let mut cancel_adapter = ready_adapter();
        cancel_adapter.folders.insert(
            intent.id(),
            FolderScan::new(
                1,
                10,
                2,
                1,
                vec![file("root:/in/a1.inkpod", "a1.inkpod", 1, 1)],
            )
            .unwrap(),
        );
        let mut polls = 0;
        let mut cancel = || {
            polls += 1;
            polls == 4
        };
        assert!(matches!(
            plan_inkscript(
                &overflow_program,
                &auth,
                &mut cancel_adapter,
                &mut [],
                ScriptPlanLimits::exact_current(),
                &mut cancel,
            ),
            Err(ScriptPlanError::Cancelled)
        ));
        assert_eq!(cancel_adapter.destination_calls, 0);
    }

    fn rgba_asset_id(payload: &[u8]) -> crate::AssetId {
        let mut store = AssetStore::default();
        store
            .ingest_raster(RasterAssetInput {
                width: 1,
                height: 1,
                pixel_format: PixelFormat::StraightRgba8,
                color_space: Some(AssetColorSpace::Srgb),
                alpha_semantics: AssetAlphaSemantics::Straight,
                canonical_stride: 4,
                pixels: payload.to_vec(),
                expected_id: None,
            })
            .unwrap()
            .id()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    struct MemoryAssetReader {
        bytes: Vec<u8>,
        cursor: usize,
        before: AuthorizedAssetIdentity,
        after: AuthorizedAssetIdentity,
    }

    impl AuthorizedAssetReader for MemoryAssetReader {
        fn observe_identity(
            &mut self,
        ) -> Result<AuthorizedAssetIdentity, AuthorizedAssetReadError> {
            Ok(if self.cursor == 0 {
                self.before
            } else {
                self.after
            })
        }

        fn read_chunk(&mut self, target: &mut [u8]) -> Result<usize, AuthorizedAssetReadError> {
            let count = target.len().min(self.bytes.len() - self.cursor);
            target[..count].copy_from_slice(&self.bytes[self.cursor..self.cursor + count]);
            self.cursor += count;
            Ok(count)
        }
    }
}
