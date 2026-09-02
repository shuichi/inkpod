use crate::{CommonRasterFormat, DocumentInfo};
use inkpod_io::FileIdentity;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SavedPair {
    pub native_path: PathBuf,
    pub native: inkpod_io::FileStamp,
    pub raster_path: PathBuf,
    pub raster: Option<inkpod_io::FileStamp>,
    /// Exact normalized-path authority when the recorded companion is absent.
    /// Exactly one of `raster` and `raster_missing` is present.
    pub raster_missing: Option<FileIdentity>,
}

/// Runtime-only authority captured while opening a raster whose native
/// sidecar does not yet exist. This proof permits exactly one first normal
/// save to materialize that pair if neither filesystem member has changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlannedPair {
    pub native_path: PathBuf,
    pub native_missing: FileIdentity,
    pub raster_path: PathBuf,
    pub raster: inkpod_io::FileStamp,
}

/// A filesystem purpose. Reference and explicit outputs never acquire normal-save authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileIoKind {
    /// Restore a complete current-version native document.
    OpenNative,
    /// Restore a pathless, dirty recovery document.
    OpenRecovery,
    /// Create editable Genesis from one raster source.
    OpenRaster,
    /// Open a raster editing pair, preferring a validated same-stem native sidecar.
    OpenRasterPair,
    /// Discover at most 1,000 neighboring sources without replacing the open document.
    SequenceAuto,
    /// Load the explicitly selected raster sequence.
    SequenceFiles,
    /// Replace a read-only reference catalog from selected paths.
    ReferenceFiles,
    /// Replace a read-only reference catalog from one nonrecursive directory.
    ReferenceFolder,
    /// Add a reference as one canonical Light Table edit.
    LightTableAdd,
    /// Reload an existing Light Table item while retaining its properties.
    LightTableReload,
    /// Save native history and its companion raster from one immutable state.
    SavePair,
    /// Write one native recovery artifact without changing normal savepoints.
    Autosave,
    /// Write one explicitly selected raster format, without adopting its path.
    ExportRaster,
    /// Validate an issue-time Batch graph without publishing document changes.
    BatchPlan,
    /// Execute an issue-time Batch graph.
    BatchRun,
    /// Produce an isolated Batch contact-sheet result.
    BatchPreview,
    /// Enumerate private native recovery artifacts and bounded typed sidecars.
    RecoveryList,
    /// Remove one recovery artifact and its sidecar through the shared service.
    RecoveryDiscard,
    /// Check whether a recovery artifact is newer than its normal source.
    RecoveryProbe,
    /// Write the captured sequence's raster outputs to one directory.
    ExportSequence,
    /// Autosave the captured source and switch to a validated sequence target.
    SequenceSwitch,
    /// Write an explicitly confirmed separate history-compacted native file.
    CompactedCopy,
}

/// Owned, bounded path request. Paths are runtime authority and never enter the journal.
#[derive(Clone, Debug)]
pub struct FileIoRequest {
    /// Semantic operation, not a frontend command ID.
    pub kind: FileIoKind,
    /// One destination/seed/folder or an explicit list of raster paths.
    pub paths: Vec<PathBuf>,
    /// Ignore stale cached content for an explicit reload.
    pub force_reload: bool,
    /// Revert the active native document while retaining its runtime-only sequence catalog.
    /// Valid only for a forced `OpenNative` of the current normal-save path.
    pub revert_current: bool,
    /// Composite against white for an explicit export only.
    pub composite_white: bool,
    /// The user authorized replacement of both existing normal-save destinations.
    pub overwrite_confirmed: bool,
    /// Include instructions only for an explicitly requested raster export.
    pub instructions: bool,
    /// Existing Light Table item ID for reload; zero for unrelated operations.
    pub object_id: u64,
    /// Optional externally generated UUID for a newly imported editable source.
    pub document_uuid: u128,
    /// Explicit export format; source decode always uses its validated format.
    pub raster_format: Option<CommonRasterFormat>,
    /// Runtime-only recovery association; never serialized into document history.
    pub recovery_metadata: Option<inkpod_io::RecoveryMetadata>,
}

impl FileIoRequest {
    /// Creates a request with conservative defaults and no overwrite authorization.
    #[must_use]
    pub fn new(kind: FileIoKind, paths: Vec<PathBuf>) -> Self {
        Self {
            kind,
            paths,
            force_reload: false,
            revert_current: false,
            composite_white: false,
            overwrite_confirmed: false,
            instructions: false,
            object_id: 0,
            document_uuid: 0,
            raster_format: None,
            recovery_metadata: None,
        }
    }
}

/// Pointer-free immutable source metadata; identity belongs to the file runtime only.
#[derive(Clone, Debug)]
pub struct FileIoItem {
    /// Absolute resolved source path; never a replay dependency.
    pub path: PathBuf,
    /// Original user-visible file name.
    pub name: String,
    /// Common raster format, absent for native files.
    pub format: Option<CommonRasterFormat>,
    /// Physical file object or normalized-path authority.
    pub identity: FileIdentity,
    /// Whether identity denotes a physical file rather than a missing destination path.
    pub identity_physical: bool,
    /// Nonzero immutable source generation.
    pub source_generation: u64,
    /// Source document identity independent of completion order.
    pub document_uuid: u128,
}

/// Public lifecycle of an I/O job. Ready still requires owner-thread validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileIoState {
    /// Accepted but not yet running.
    Queued,
    /// Reading, decoding, preparing, or installing in workers.
    Running,
    /// A detached result is available for an owner-thread apply.
    Ready,
    /// Applied or an explicit output completed successfully.
    Complete,
    /// Failed without publishing a document candidate.
    Failed,
    /// Cancelled before publication or the install linearization point.
    Cancelled,
}

/// Coherent nonblocking job status. Image counters are separate from processing work.
#[derive(Clone, Copy, Debug)]
pub struct FileIoProgress {
    /// Runtime-local, monotonically issued job identity.
    pub job_id: u64,
    /// Captured operation kind.
    pub kind: FileIoKind,
    /// Current lifecycle state.
    pub state: FileIoState,
    /// Number found during discovery.
    pub discovered_count: u64,
    /// Selected image count; zero until enumeration finishes.
    pub total_count: u64,
    /// Number of completed physical reads.
    pub read_count: u64,
    /// Images decoded and validated, including cache hits.
    pub loaded_count: u64,
    /// Failed image count.
    pub failed_count: u64,
    /// Cancelled image count.
    pub cancelled_count: u64,
    /// Completed operation work, independent of image counts.
    pub completed_work: u64,
    /// Total operation work.
    pub total_work: u64,
    /// Immutable metadata entries ready to query.
    pub result_count: u64,
    /// Discovery was limited to the documented automatic-sequence neighborhood.
    pub truncated: bool,
    /// An authorized save is installing; its owner must finalize before closing.
    pub installing: bool,
    /// The source is a Cut descriptor; the frontend should route to its Cut owner.
    pub cut_descriptor: bool,
    /// A failed pair installation restored bytes under new identities and has a
    /// verified same-target runtime authority repair pending or applied.
    pub authority_repaired: bool,
    /// A failed pair installation crossed its disk-publication point and owner
    /// finalization revoked the affected runtime pair authority. The frontend
    /// must discard matching path/identity aliases and require Save As next.
    pub authority_revoked: bool,
}

/// Result of one owner-thread apply attempt.
#[derive(Clone, Debug)]
pub enum FileIoApply {
    /// Save installation was authorized; polling and final apply are still required.
    Pending,
    /// The operation completed. Reference jobs use their separate catalog apply.
    Complete {
        /// Current document state, if this operation has a document owner.
        document: Option<Box<DocumentInfo>>,
        /// Newly created or reloaded Light Table item ID; zero otherwise.
        object_id: u64,
    },
}
