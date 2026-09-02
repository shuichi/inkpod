use crate::{FileIdentity, FileStamp, IoError, IoResult};
use std::path::PathBuf;

/// Current metadata format only; pre-freeze records do not have a compatibility
/// reader. Metadata is a recovery hint and never confers normal-save authority.
pub const RECOVERY_METADATA_VERSION: u32 = 4;
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
    /// Exact normal-pair authority held when this recovery generation was
    /// captured. This runtime filesystem proof is metadata only; it never
    /// enters the document procedure journal.
    pub pair_proof: Option<RecoveryPairProof>,
    /// 100 ns ticks since 1601-01-01 UTC, matching Windows FILETIME. A writer
    /// accepts zero to sample the clock in Rust; persisted records are nonzero.
    pub written_time_100ns: u64,
}

/// Exact pair observation retained with one recovery generation.
///
/// A committed proof binds both physical members. A planned proof binds the
/// selected physical raster and the normalized identity of the still-missing
/// same-stem native destination. Recovery may adopt current pair authority only
/// when the shared resolver reproduces this complete proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryPairProof {
    /// Both normal-save members existed at recovery capture time.
    Committed {
        /// Exact native member observation.
        native: FileStamp,
        /// Exact raster member observation.
        raster: FileStamp,
    },
    /// The raster existed while the inferred native member was still absent.
    Planned {
        /// Normalized-path identity of the missing native destination.
        native_missing: FileIdentity,
        /// Exact raster member observation.
        raster: FileStamp,
    },
    /// The native exists while its recorded raster companion is absent.
    RepairNeeded {
        /// Exact native member observation.
        native: FileStamp,
        /// Normalized-path identity of the missing raster destination.
        raster_missing: FileIdentity,
    },
}

impl RecoveryPairProof {
    fn validate(self) -> IoResult<()> {
        let (native, raster) = match self {
            Self::Committed { native, raster } => {
                validate_pair_stamp(native)?;
                (native.identity, raster)
            }
            Self::Planned {
                native_missing,
                raster,
            } => {
                validate_pair_identity(native_missing)?;
                if native_missing.volume != u64::MAX {
                    return Err(IoError::InvalidInput(
                        "planned recovery pair identity is not normalized-path authority",
                    ));
                }
                (native_missing, raster)
            }
            Self::RepairNeeded {
                native,
                raster_missing,
            } => {
                validate_pair_stamp(native)?;
                validate_pair_identity(raster_missing)?;
                if raster_missing.volume != u64::MAX {
                    return Err(IoError::InvalidInput(
                        "repair-needed recovery pair identity is not normalized-path authority",
                    ));
                }
                (native.identity, missing_stamp(raster_missing))
            }
        };
        if raster.length == 0 {
            if !matches!(self, Self::RepairNeeded { .. }) {
                return Err(IoError::InvalidInput(
                    "recovery pair proof file length is zero",
                ));
            }
        } else {
            validate_pair_stamp(raster)?;
        }
        if native == raster.identity {
            return Err(IoError::InvalidInput(
                "recovery pair proof aliases its members",
            ));
        }
        Ok(())
    }
}

fn missing_stamp(identity: FileIdentity) -> FileStamp {
    FileStamp {
        identity,
        length: 0,
        modified: 0,
        changed: 0,
        readonly: false,
    }
}

fn validate_pair_identity(identity: FileIdentity) -> IoResult<()> {
    if identity.volume == 0 && identity.file == 0 {
        return Err(IoError::InvalidInput(
            "recovery pair proof identity is zero",
        ));
    }
    Ok(())
}

fn validate_pair_stamp(stamp: FileStamp) -> IoResult<()> {
    validate_pair_identity(stamp.identity)?;
    if stamp.identity.volume == u64::MAX {
        return Err(IoError::InvalidInput(
            "physical recovery pair stamp uses normalized-path authority",
        ));
    }
    if stamp.length == 0 {
        return Err(IoError::InvalidInput(
            "recovery pair proof file length is zero",
        ));
    }
    Ok(())
}

/// One exact observation of a published recovery member.
///
/// This is runtime-only authority. Every field of the shared [`FileStamp`]
/// contract is retained so recovery validation never weakens the ordinary
/// complete-stamp comparison used by the file service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryArtifactStamp {
    pub identity: FileIdentity,
    pub length: u64,
    pub modified: i128,
    pub changed: i128,
    pub readonly: bool,
}

impl From<FileStamp> for RecoveryArtifactStamp {
    fn from(stamp: FileStamp) -> Self {
        Self {
            identity: stamp.identity,
            length: stamp.length,
            modified: stamp.modified,
            changed: stamp.changed,
            readonly: stamp.readonly,
        }
    }
}

impl From<RecoveryArtifactStamp> for FileStamp {
    fn from(stamp: RecoveryArtifactStamp) -> Self {
        Self {
            identity: stamp.identity,
            length: stamp.length,
            modified: stamp.modified,
            changed: stamp.changed,
            readonly: stamp.readonly,
        }
    }
}

/// Exact publication proof for the native recovery and its metadata sidecar.
///
/// The proof is meaningful only together with the separately retained recovery
/// path. It is never serialized into document history or recovery metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryArtifactProof {
    pub native: RecoveryArtifactStamp,
    pub metadata: RecoveryArtifactStamp,
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
        if let Some(pair_proof) = self.pair_proof {
            pair_proof.validate()?;
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
