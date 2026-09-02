//! Filesystem job orchestration. Workers never borrow a live single-writer Core.

mod apply;
mod batch;
mod job;
mod model;
mod prepare;
mod recovery;
mod session;
mod target_cache;

pub use job::FileIoJob;
pub use model::{FileIoApply, FileIoItem, FileIoKind, FileIoProgress, FileIoRequest, FileIoState};
pub(crate) use model::{PlannedPair, SavedPair};
pub use target_cache::{
    DEFAULT_VALIDATED_TARGET_CACHE_BYTES, MAX_VALIDATED_TARGET_CACHE_BYTES, MAX_VALIDATED_TARGETS,
    ValidatedTargetCache, ValidatedTargetCacheStats,
};

/// Resolves the current native/raster pair for the synchronous Core Revert API.
///
/// This deliberately shares the production native-open resolver instead of
/// reconstructing companion discovery, canonical raster validation, pair
/// recovery, or complete-stamp authority in the synchronous persistence layer.
/// The result remains detached; caller-side token validation publishes it.
pub(crate) fn prepare_pair_revert(
    manager: &inkpod_io::IoManager,
    path: &std::path::Path,
) -> Result<crate::Core, crate::CoreError> {
    let mut request = FileIoRequest::new(FileIoKind::OpenNative, vec![path.to_path_buf()]);
    request.force_reload = true;
    request.revert_current = true;
    prepare::validate_request(&request)?;
    let context = inkpod_io::JobContext::new();
    let (prepared, _) = prepare::native(manager, &request, &context)?;
    match prepared {
        job::Prepared::Open(staged, None, Some(normal_path)) if normal_path == path => Ok(*staged),
        _ => Err(crate::CoreError::InvalidState(
            "native pair resolver returned an invalid Revert result",
        )),
    }
}
