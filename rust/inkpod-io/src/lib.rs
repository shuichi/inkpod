//! Application-owned, bounded image I/O. No mutable document state lives here.

mod backend;
mod cache;
mod companion;
mod config;
mod error;
mod executor;
mod file_lock;
mod image;
mod job;
mod manager;
mod pair;
mod recovery;
mod sequence;
mod temporary;
mod transaction;

pub use backend::{FileIdentity, FileStamp};
pub use cache::{CacheStats, MAX_SEQUENCE_RENDER_ALLOCATIONS, MAX_SEQUENCE_RENDER_BYTES};
pub use config::IoConfig;
pub use error::{IoError, IoResult};
pub use image::{DecodedLease, LoadedBytes, LoadedImage, RetainedDecodedRaster};
pub use job::{ImageBatch, ImageBatchItem, IoJob, JobContext, JobPhase, JobProgress, JobState};
pub use manager::IoManager;
#[cfg(any(test, feature = "test-support"))]
pub use pair::PairInstallFault;
pub use pair::{
    PAIR_JOURNAL_VERSION, PairInstallOutcome, PairRecovery, PreparedPair, RestoredPair,
};
pub use recovery::{
    RECOVERY_METADATA_VERSION, RecoveryArtifactProof, RecoveryArtifactStamp, RecoveryCandidate,
    RecoveryIdentity, RecoveryIdentityKind, RecoveryMetadata, RecoveryPairProof,
    decode_recovery_metadata, encode_recovery_metadata, recovery_metadata_path,
};
pub use sequence::SequenceDiscovery;
pub use temporary::TemporaryDirectory;
pub use transaction::LockedFiles;
