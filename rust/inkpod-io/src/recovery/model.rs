use crate::{IoError, IoResult};
use std::path::PathBuf;

/// Current metadata format only; pre-freeze records do not have a compatibility
/// reader. Metadata is a recovery hint and never confers normal-save authority.
pub const RECOVERY_METADATA_VERSION: u32 = 2;
pub(super) const MAX_METADATA_BYTES: usize = 512 * 1024;
pub(super) const MAX_PATH_UNITS: usize = 32_767;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RecoveryIdentityKind {
    #[default]
    None,
    PhysicalFile,
    NormalizedPath,
    Untitled,
}

/// Previous frontend identity, retained only to describe a recovery candidate.
/// Physical identifiers are in the same volume/file namespace as `FileIdentity`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryIdentity {
    pub kind: RecoveryIdentityKind,
    pub volume_serial: u64,
    pub file_id: [u8; 16],
    pub normalized_path: String,
    pub uuid: u128,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryMetadata {
    pub session_id: u64,
    pub generation: u64,
    pub document_uuid: u128,
    pub original_identity: RecoveryIdentity,
    pub original_path: String,
    pub source_path: String,
    /// 100 ns ticks since 1601-01-01 UTC, matching Windows FILETIME. A writer
    /// accepts zero to sample the clock in Rust; persisted records are nonzero.
    pub written_time_100ns: u64,
}

#[derive(Clone, Debug)]
pub struct RecoveryCandidate {
    pub recovery_path: PathBuf,
    pub metadata_path: PathBuf,
    pub modified_time_100ns: u64,
    /// Missing, malformed, obsolete, or unreadable metadata does not discard the
    /// native recovery file. The user can still attempt to open that candidate.
    pub metadata: Option<RecoveryMetadata>,
    pub metadata_error: Option<String>,
}

impl RecoveryMetadata {
    pub(super) fn validate(&self) -> IoResult<()> {
        self.validate_input()?;
        if self.written_time_100ns == 0 {
            return Err(IoError::InvalidInput("recovery metadata time is zero"));
        }
        Ok(())
    }

    pub(super) fn validate_input(&self) -> IoResult<()> {
        if self.session_id == 0 || self.generation == 0 || self.document_uuid == 0 {
            return Err(IoError::InvalidInput("recovery metadata identity is zero"));
        }
        validate_string(&self.original_path)?;
        validate_string(&self.source_path)?;
        validate_string(&self.original_identity.normalized_path)?;
        let identity = &self.original_identity;
        let has_physical = identity.volume_serial != 0 || identity.file_id != [0; 16];
        let valid = match identity.kind {
            RecoveryIdentityKind::None => {
                !has_physical && identity.normalized_path.is_empty() && identity.uuid == 0
            }
            RecoveryIdentityKind::PhysicalFile => {
                has_physical && identity.normalized_path.is_empty() && identity.uuid == 0
            }
            RecoveryIdentityKind::NormalizedPath => {
                !has_physical && !identity.normalized_path.is_empty() && identity.uuid == 0
            }
            RecoveryIdentityKind::Untitled => {
                !has_physical && identity.normalized_path.is_empty() && identity.uuid != 0
            }
        };
        if !valid {
            return Err(IoError::InvalidInput(
                "recovery metadata file identity is inconsistent",
            ));
        }
        Ok(())
    }

    pub(super) fn allocation_bytes(&self) -> usize {
        self.original_path.capacity()
            + self.source_path.capacity()
            + self.original_identity.normalized_path.capacity()
    }
}

pub(super) fn validate_string(value: &str) -> IoResult<()> {
    if value.len() > MAX_PATH_UNITS * 4
        || value.encode_utf16().count() > MAX_PATH_UNITS
        || value.contains('\0')
    {
        return Err(IoError::InvalidInput(
            "recovery metadata path is invalid or too long",
        ));
    }
    Ok(())
}
