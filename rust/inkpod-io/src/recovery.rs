//! Private recovery artifacts. Normal-save pair semantics are deliberately not
//! applied to these pathless, dirty-document recovery snapshots.

mod codec;
mod model;

pub use codec::{decode_recovery_metadata, encode_recovery_metadata};
pub use model::{
    RECOVERY_METADATA_VERSION, RecoveryArtifactProof, RecoveryArtifactStamp, RecoveryCandidate,
    RecoveryIdentity, RecoveryIdentityKind, RecoveryMetadata, RecoveryPairProof,
};

use crate::backend;
use crate::{FileStamp, IoError, IoManager, IoResult, JobContext, JobPhase, LockedFiles};
use model::MAX_METADATA_BYTES;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_CANDIDATES: usize = 4096;
const MAX_DIRECTORY_ENTRIES: usize = 1_000_000;
const MAX_RETAINED_METADATA: usize = 32 * 1024 * 1024;
const MAX_NATIVE_BYTES: u64 = 1024 * 1024 * 1024;
const FILETIME_UNIX_EPOCH: i128 = 116_444_736_000_000_000;

/// Constructs the sidecar name without touching the filesystem. The native
/// recovery extension must be `.inkpod`; arbitrary external files are rejected.
pub fn recovery_metadata_path(recovery_path: &Path) -> IoResult<PathBuf> {
    validate_native_path(recovery_path)?;
    let mut path = recovery_path.as_os_str().to_os_string();
    path.push(".metadata");
    let path = PathBuf::from(path);
    validate_path_length(&path)?;
    Ok(path)
}

impl IoManager {
    /// Creates the private recovery directory and returns its UUID-based native
    /// name. A sequence source generation, when present, must be nonzero.
    pub fn recovery_path(
        &self,
        root: &Path,
        document_uuid: u128,
        source_generation: Option<u64>,
        context: &JobContext,
    ) -> IoResult<PathBuf> {
        self.check_running(context)?;
        if document_uuid == 0 || source_generation == Some(0) {
            return Err(IoError::InvalidInput(
                "recovery UUID or source generation is zero",
            ));
        }
        let root = backend::resolve(root)?;
        let name = match source_generation {
            Some(generation) => format!("{document_uuid:032x}-sequence-{generation:016x}.inkpod"),
            None => format!("{document_uuid:032x}.inkpod"),
        };
        let path = root.join(name);
        recovery_metadata_path(&path)?;
        self.create_dir_all(&root, context)?;
        Ok(path)
    }

    /// Writes one new native recovery attempt plus metadata on the worker that
    /// calls this API. Neither member may already exist: callers rotate to a
    /// fresh attempt path and publish its returned proof only after this method
    /// succeeds. Consequently a failed attempt cannot overwrite the previously
    /// published recovery generation. A failure after the first member was
    /// installed may leave only this new, unassociated attempt for later cleanup.
    /// Success means both writes were flushed and installed; it never advances
    /// normal-save path authority or document savepoints.
    pub fn write_recovery(
        &self,
        recovery_path: &Path,
        metadata: &RecoveryMetadata,
        context: &JobContext,
        native_writer: impl FnOnce(&mut File) -> IoResult<()>,
    ) -> IoResult<RecoveryArtifactProof> {
        self.check_running(context)?;
        let metadata = metadata_with_time(metadata)?;
        let bytes = encode_recovery_metadata(&metadata)?;
        let (native, sidecar) = artifact_paths(recovery_path)?;
        self.create_dir_all(parent(&native)?, context)?;
        self.with_file_locks(&[native.clone(), sidecar.clone()], context, |files| {
            validate_locked_artifacts(files, &[&native, &sidecar])?;
            if files.exists(&native)? || files.exists(&sidecar)? {
                return Err(IoError::ConfirmationRequired);
            }
            files.write_new_atomic(&native, |file| {
                native_writer(file)?;
                if file.metadata()?.len() > MAX_NATIVE_BYTES {
                    return Err(IoError::LimitExceeded(
                        "native recovery exceeds its byte bound",
                    ));
                }
                Ok(())
            })?;
            files.write_new_atomic(&sidecar, |file| {
                for chunk in bytes.chunks(64 * 1024) {
                    context.check_cancelled()?;
                    file.write_all(chunk)?;
                }
                Ok(())
            })?;
            recovery_artifact_proof(files, &native, &sidecar)
        })
    }

    /// Reads and validates one exact recovery publication under the shared
    /// native/metadata locks. Both members must match `expected` before any
    /// decode and again after both decoders complete. Missing, replaced, or
    /// concurrently changed members therefore never yield a candidate value.
    pub fn read_recovery_with_proof<T>(
        &self,
        recovery_path: &Path,
        expected: RecoveryArtifactProof,
        context: &JobContext,
        native_reader: impl FnOnce(&mut File) -> IoResult<T>,
    ) -> IoResult<(T, RecoveryMetadata)> {
        let (native, sidecar) = artifact_paths(recovery_path)?;
        self.with_file_locks(&[native.clone(), sidecar.clone()], context, |files| {
            validate_locked_artifacts(files, &[&native, &sidecar])?;
            if recovery_artifact_proof(files, &native, &sidecar)? != expected {
                return Err(IoError::ChangedDuringRead);
            }
            let metadata = files.with_reader(&sidecar, MAX_METADATA_BYTES as u64, |file| {
                let length = usize::try_from(file.metadata()?.len())
                    .map_err(|_| IoError::LimitExceeded("recovery metadata length overflow"))?;
                if length > MAX_METADATA_BYTES {
                    return Err(IoError::LimitExceeded(
                        "recovery metadata exceeds its byte bound",
                    ));
                }
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(length)
                    .map_err(|_| IoError::ResourceBusy("recovery metadata allocation failed"))?;
                let mut buffer = [0_u8; 64 * 1024];
                loop {
                    context.check_cancelled()?;
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    if bytes
                        .len()
                        .checked_add(read)
                        .is_none_or(|size| size > length)
                    {
                        return Err(IoError::ChangedDuringRead);
                    }
                    bytes.extend_from_slice(&buffer[..read]);
                }
                decode_recovery_metadata(&bytes)
            })?;
            let native_value = files.with_reader(&native, MAX_NATIVE_BYTES, native_reader)?;
            if recovery_artifact_proof(files, &native, &sidecar)? != expected {
                return Err(IoError::ChangedDuringRead);
            }
            Ok((native_value, metadata))
        })
    }

    /// Atomically writes metadata, filling a zero timestamp from the Rust clock.
    /// This does not create or alter the corresponding native recovery file.
    pub fn write_recovery_metadata(
        &self,
        recovery_path: &Path,
        metadata: &RecoveryMetadata,
        context: &JobContext,
    ) -> IoResult<()> {
        self.check_running(context)?;
        let metadata = metadata_with_time(metadata)?;
        let bytes = encode_recovery_metadata(&metadata)?;
        let (native, sidecar) = artifact_paths(recovery_path)?;
        self.create_dir_all(parent(&native)?, context)?;
        self.with_file_locks(&[native.clone(), sidecar.clone()], context, |files| {
            validate_locked_artifacts(files, &[&native, &sidecar])?;
            files.write_bytes_atomic(&sidecar, &bytes)
        })
    }

    /// Reads one bounded metadata record with the native/sidecar coordination
    /// locks. Metadata is not admitted into the image cache or its image slots.
    pub fn read_recovery_metadata(
        &self,
        recovery_path: &Path,
        context: &JobContext,
    ) -> IoResult<RecoveryMetadata> {
        let (native, sidecar) = artifact_paths(recovery_path)?;
        self.with_file_locks(&[native.clone(), sidecar.clone()], context, |files| {
            validate_locked_artifacts(files, &[&native, &sidecar])?;
            files.with_reader(&sidecar, MAX_METADATA_BYTES as u64, |file| {
                let length = usize::try_from(file.metadata()?.len())
                    .map_err(|_| IoError::LimitExceeded("recovery metadata length overflow"))?;
                if length > MAX_METADATA_BYTES {
                    return Err(IoError::LimitExceeded(
                        "recovery metadata exceeds its byte bound",
                    ));
                }
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(length)
                    .map_err(|_| IoError::ResourceBusy("recovery metadata allocation failed"))?;
                let mut buffer = [0_u8; 64 * 1024];
                loop {
                    context.check_cancelled()?;
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    if bytes
                        .len()
                        .checked_add(read)
                        .is_none_or(|size| size > length)
                    {
                        return Err(IoError::ChangedDuringRead);
                    }
                    bytes.extend_from_slice(&buffer[..read]);
                }
                decode_recovery_metadata(&bytes)
            })
        })
    }

    /// Enumerates at most 4096 regular native candidates, newest first and path
    /// ascending on timestamp ties. Invalid or missing sidecars remain visible.
    /// Enumeration is bounded and cancellable; an absent directory is empty.
    pub fn list_recovery_candidates(
        &self,
        root: &Path,
        context: &JobContext,
    ) -> IoResult<Vec<RecoveryCandidate>> {
        self.check_running(context)?;
        let root = backend::resolve(root)?;
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        context.set_phase(JobPhase::Enumerating);
        let mut candidates = Vec::new();
        for (index, entry) in entries.enumerate() {
            context.check_cancelled()?;
            if index >= MAX_DIRECTORY_ENTRIES {
                return Err(IoError::LimitExceeded(
                    "recovery directory exceeds its entry bound",
                ));
            }
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let recovery_path = entry.path();
            if !is_native(&recovery_path) {
                continue;
            }
            if candidates.len() == MAX_CANDIDATES {
                return Err(IoError::LimitExceeded(
                    "too many native recovery candidates",
                ));
            }
            let metadata_path = recovery_metadata_path(&recovery_path)?;
            let modified_time_100ns = file_time(entry.metadata()?.modified()?)?;
            candidates
                .try_reserve(1)
                .map_err(|_| IoError::ResourceBusy("recovery list allocation failed"))?;
            candidates.push(RecoveryCandidate {
                recovery_path,
                metadata_path,
                modified_time_100ns,
                metadata: None,
                metadata_error: None,
            });
            context.set_counts(candidates.len() as u64, 0, 0);
        }
        candidates.sort_by(|left, right| {
            right
                .modified_time_100ns
                .cmp(&left.modified_time_100ns)
                .then_with(|| left.recovery_path.cmp(&right.recovery_path))
        });
        let mut retained = 0_usize;
        let count = candidates.len() as u64;
        for (index, candidate) in candidates.iter_mut().enumerate() {
            context.check_cancelled()?;
            match self.read_recovery_metadata(&candidate.recovery_path, context) {
                Ok(metadata) => {
                    if retained
                        .checked_add(metadata.allocation_bytes())
                        .is_some_and(|value| value <= MAX_RETAINED_METADATA)
                    {
                        retained += metadata.allocation_bytes();
                        candidate.metadata = Some(metadata);
                    } else {
                        candidate.metadata_error =
                            Some("recovery metadata list budget exceeded".into());
                    }
                }
                Err(IoError::Cancelled | IoError::Shutdown) => return Err(IoError::Cancelled),
                Err(error) => candidate.metadata_error = Some(error.to_string()),
            }
            context.set_work((index + 1) as u64, count);
        }
        Ok(candidates)
    }

    /// Removes the sidecar then native recovery under their shared file locks.
    /// Missing artifacts are a no-op. A failure never follows a sidecar symlink
    /// to delete a different file, and never deletes a non-native path.
    pub fn discard_recovery(&self, recovery_path: &Path, context: &JobContext) -> IoResult<()> {
        let (native, sidecar) = artifact_paths(recovery_path)?;
        self.with_file_locks(&[native.clone(), sidecar.clone()], context, |files| {
            validate_locked_artifacts(files, &[&native, &sidecar])?;
            files.remove(&sidecar)?;
            files.remove(&native)
        })
    }

    /// Removes an obsolete recovery attempt only while both members still
    /// match the exact proof returned by its successful publication. This is
    /// the safe cleanup path after a newer append-only generation has become
    /// authoritative; a replaced, missing, or mixed pair is left untouched.
    pub fn discard_recovery_with_proof(
        &self,
        recovery_path: &Path,
        expected: RecoveryArtifactProof,
        context: &JobContext,
    ) -> IoResult<()> {
        let (native, sidecar) = artifact_paths(recovery_path)?;
        self.with_file_locks(&[native.clone(), sidecar.clone()], context, |files| {
            validate_locked_artifacts(files, &[&native, &sidecar])?;
            if recovery_artifact_proof(files, &native, &sidecar)? != expected {
                return Err(IoError::ChangedDuringRead);
            }
            let native_stamp: FileStamp = expected.native.into();
            let metadata_stamp: FileStamp = expected.metadata.into();
            backend::remove_exact_pair(&native, native_stamp, &sidecar, metadata_stamp)?;
            self.inner.cache.invalidate(native_stamp.identity);
            self.inner.cache.invalidate(metadata_stamp.identity);
            Ok(())
        })
    }

    /// Missing recovery is false; an existing recovery without a normal file is
    /// newer. Permission and malformed path errors are distinct from absence.
    pub fn recovery_is_newer(
        &self,
        normal: &Path,
        recovery: &Path,
        context: &JobContext,
    ) -> IoResult<bool> {
        validate_native_path(recovery)?;
        self.with_file_locks(
            &[normal.to_path_buf(), recovery.to_path_buf()],
            context,
            |files| {
                if !files.exists(recovery)? {
                    return Ok(false);
                }
                let recovered = files.metadata(recovery)?;
                if !files.exists(normal)? {
                    return Ok(true);
                }
                Ok(recovered.modified > files.metadata(normal)?.modified)
            },
        )
    }
}

fn metadata_with_time(metadata: &RecoveryMetadata) -> IoResult<RecoveryMetadata> {
    metadata.validate_input()?;
    let mut metadata = metadata.clone();
    if metadata.written_time_100ns == 0 {
        metadata.written_time_100ns = file_time(SystemTime::now())?;
    }
    metadata.validate()?;
    Ok(metadata)
}

fn file_time(time: SystemTime) -> IoResult<u64> {
    let ticks = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i128::try_from(duration.as_nanos() / 100)
            .ok()
            .and_then(|value| FILETIME_UNIX_EPOCH.checked_add(value)),
        Err(error) => i128::try_from(error.duration().as_nanos() / 100)
            .ok()
            .and_then(|value| FILETIME_UNIX_EPOCH.checked_sub(value)),
    };
    ticks
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value != 0)
        .ok_or(IoError::InvalidInput(
            "recovery file time is outside its range",
        ))
}

fn artifact_paths(recovery_path: &Path) -> IoResult<(PathBuf, PathBuf)> {
    validate_native_path(recovery_path)?;
    let name = recovery_path
        .file_name()
        .ok_or(IoError::InvalidInput("recovery filename is missing"))?;
    let directory = parent(recovery_path)?;
    let native = backend::resolve(directory)?.join(name);
    let sidecar = recovery_metadata_path(&native)?;
    reject_nonregular(&native)?;
    reject_nonregular(&sidecar)?;
    Ok((native, sidecar))
}

fn recovery_artifact_proof(
    files: &LockedFiles<'_>,
    native: &Path,
    sidecar: &Path,
) -> IoResult<RecoveryArtifactProof> {
    Ok(RecoveryArtifactProof {
        native: files.metadata(native)?.into(),
        metadata: files.metadata(sidecar)?.into(),
    })
}

fn parent(path: &Path) -> IoResult<&Path> {
    path.parent()
        .filter(|value| !value.as_os_str().is_empty())
        .or_else(|| (!path.is_absolute()).then_some(Path::new(".")))
        .ok_or(IoError::InvalidInput(
            "recovery parent directory is missing",
        ))
}

fn is_native(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("inkpod"))
}

fn validate_path_length(path: &Path) -> IoResult<()> {
    if path.as_os_str().is_empty() || path.as_os_str().len() > 32_767 {
        return Err(IoError::InvalidInput("recovery path is empty or too long"));
    }
    Ok(())
}

fn validate_native_path(path: &Path) -> IoResult<()> {
    validate_path_length(path)?;
    if !is_native(path) || path.file_stem().is_none_or(|stem| stem.is_empty()) {
        return Err(IoError::InvalidInput(
            "recovery path must name a native file",
        ));
    }
    Ok(())
}

fn reject_nonregular(path: &Path) -> IoResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => Err(
            IoError::InvalidInput("recovery artifact is not a regular file"),
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_locked_artifacts(files: &LockedFiles<'_>, paths: &[&Path]) -> IoResult<()> {
    for path in paths {
        reject_nonregular(path)?;
        if backend::lock_path(&files.resolve_member(path)?) != backend::lock_path(path) {
            return Err(IoError::ChangedDuringRead);
        }
    }
    Ok(())
}
