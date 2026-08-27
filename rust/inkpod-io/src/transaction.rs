use crate::backend;
use crate::file_lock::{LockKey, lock_cancel};
use crate::{FileStamp, IoError, IoManager, IoResult, JobContext, JobPhase, LoadedBytes};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// A synchronous filesystem transaction scope with all enlisted normalized paths
/// and existing physical identities locked in a deterministic order. Methods do
/// not acquire another file lock; nested manager calls must not be made here.
/// The caller chooses publication/rollback semantics across multiple files.
pub struct LockedFiles<'a> {
    manager: &'a IoManager,
    context: &'a JobContext,
    members: BTreeMap<PathBuf, PathBuf>,
}

impl IoManager {
    pub fn with_file_locks<T>(
        &self,
        paths: &[PathBuf],
        context: &JobContext,
        action: impl FnOnce(&LockedFiles<'_>) -> IoResult<T>,
    ) -> IoResult<T> {
        self.check_running(context)?;
        if paths.is_empty() || paths.len() > 16_384 {
            return Err(IoError::LimitExceeded(
                "file transaction target count is invalid",
            ));
        }
        let mut members = BTreeMap::new();
        for path in paths {
            context.check_cancelled()?;
            members.insert(path.clone(), backend::resolve(path)?);
        }
        let normalized: BTreeSet<_> = members.values().cloned().collect();
        let lock_paths: BTreeSet<_> = normalized
            .iter()
            .map(|path| backend::lock_path(path))
            .collect();
        let path_owners: Vec<_> = lock_paths
            .into_iter()
            .map(|path| self.inner.locks.acquire_owner(LockKey::Path(path)))
            .collect();
        let mut path_guards = Vec::with_capacity(path_owners.len());
        for owner in &path_owners {
            path_guards.push(lock_cancel(owner, context)?);
        }
        let mut identities = BTreeSet::new();
        for path in &normalized {
            match File::open(path) {
                Ok(file) => {
                    identities.insert(backend::stamp(&file)?.identity);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let identity_owners: Vec<_> = identities
            .into_iter()
            .map(|identity| self.inner.locks.acquire_owner(LockKey::Identity(identity)))
            .collect();
        let mut identity_guards = Vec::with_capacity(identity_owners.len());
        for owner in &identity_owners {
            identity_guards.push(lock_cancel(owner, context)?);
        }
        context.check_cancelled()?;
        action(&LockedFiles {
            manager: self,
            context,
            members,
        })
    }

    /// Provides a bounded, locked streaming reader. Native codec resource bounds
    /// are passed explicitly and do not force a full encoded cache allocation.
    pub fn with_reader<T>(
        &self,
        path: &Path,
        maximum_bytes: u64,
        context: &JobContext,
        action: impl FnOnce(&mut File) -> IoResult<T>,
    ) -> IoResult<T> {
        self.with_file_locks(&[path.to_path_buf()], context, |files| {
            files.with_reader(path, maximum_bytes, action)
        })
    }

    /// Same-directory temporary write, flush, close, then atomic replacement.
    /// Cancellation is honored before publication, never reported after commit.
    /// The temporary owner also closes and removes its file if the writer unwinds.
    pub fn write_atomic(
        &self,
        path: &Path,
        context: &JobContext,
        action: impl FnOnce(&mut File) -> IoResult<()>,
    ) -> IoResult<()> {
        self.with_file_locks(&[path.to_path_buf()], context, |files| {
            files.write_atomic(path, action)
        })
    }

    pub fn write_bytes_atomic(
        &self,
        path: &Path,
        bytes: &[u8],
        context: &JobContext,
    ) -> IoResult<()> {
        self.with_file_locks(&[path.to_path_buf()], context, |files| {
            files.write_bytes_atomic(path, bytes)
        })
    }

    pub fn write_new_atomic(
        &self,
        path: &Path,
        context: &JobContext,
        action: impl FnOnce(&mut File) -> IoResult<()>,
    ) -> IoResult<()> {
        self.with_file_locks(&[path.to_path_buf()], context, |files| {
            files.write_new_atomic(path, action)
        })
    }

    pub fn metadata(&self, path: &Path, context: &JobContext) -> IoResult<FileStamp> {
        self.with_file_locks(&[path.to_path_buf()], context, |files| files.metadata(path))
    }

    pub fn exists(&self, path: &Path, context: &JobContext) -> IoResult<bool> {
        self.with_file_locks(&[path.to_path_buf()], context, |files| files.exists(path))
    }

    pub fn remove(&self, path: &Path, context: &JobContext) -> IoResult<()> {
        self.with_file_locks(&[path.to_path_buf()], context, |files| files.remove(path))
    }

    pub fn rename(&self, source: &Path, destination: &Path, context: &JobContext) -> IoResult<()> {
        self.with_file_locks(
            &[source.to_path_buf(), destination.to_path_buf()],
            context,
            |files| files.rename(source, destination),
        )
    }

    pub fn create_dir_all(&self, path: &Path, context: &JobContext) -> IoResult<()> {
        self.check_running(context)?;
        std::fs::create_dir_all(path)?;
        Ok(())
    }

    pub fn create_dir(&self, path: &Path, context: &JobContext) -> IoResult<()> {
        self.check_running(context)?;
        std::fs::create_dir(path)?;
        Ok(())
    }

    /// Removes only a caller-owned subtree strictly below an explicitly supplied
    /// root. Canonical path validation rejects the root itself and escapes.
    pub fn remove_tree(
        &self,
        path: &Path,
        allowed_root: &Path,
        context: &JobContext,
    ) -> IoResult<()> {
        context.check_cancelled()?;
        let root = std::fs::canonicalize(allowed_root)?;
        let path = std::fs::canonicalize(path)?;
        if path == root || !path.starts_with(&root) {
            return Err(IoError::InvalidInput(
                "temporary cleanup escaped its owned root",
            ));
        }
        std::fs::remove_dir_all(&path)?;
        // Temporary decoded inputs must not linger after their job directory is
        // removed. Consumer leases remain valid and charged through their lifetime.
        self.inner.cache.invalidate_under(&path);
        Ok(())
    }

    pub fn remove_empty_dir(&self, path: &Path, context: &JobContext) -> IoResult<()> {
        self.check_running(context)?;
        std::fs::remove_dir(path)?;
        Ok(())
    }
}

impl LockedFiles<'_> {
    pub(crate) fn resolve_member(&self, path: &Path) -> IoResult<PathBuf> {
        self.members
            .get(path)
            .cloned()
            .or_else(|| {
                self.members
                    .values()
                    .find(|member| member.as_path() == path)
                    .cloned()
            })
            .ok_or(IoError::InvalidInput(
                "file transaction accessed an unenlisted target",
            ))
    }

    pub fn metadata(&self, path: &Path) -> IoResult<FileStamp> {
        self.context.check_cancelled()?;
        backend::stamp(&File::open(self.resolve_member(path)?)?)
    }

    pub fn exists(&self, path: &Path) -> IoResult<bool> {
        self.context.check_cancelled()?;
        Ok(self.resolve_member(path)?.try_exists()?)
    }

    pub fn read_bytes(&self, path: &Path, maximum_bytes: u64) -> IoResult<LoadedBytes> {
        let path = self.resolve_member(path)?;
        let mut file = File::open(&path)?;
        let stamp = backend::stamp(&file)?;
        self.manager
            .read_open_file(&mut file, path, stamp, maximum_bytes, self.context)
    }

    pub fn with_reader<T>(
        &self,
        path: &Path,
        maximum_bytes: u64,
        action: impl FnOnce(&mut File) -> IoResult<T>,
    ) -> IoResult<T> {
        self.context.check_cancelled()?;
        let mut file = File::open(self.resolve_member(path)?)?;
        let before = backend::stamp(&file)?;
        if before.length > maximum_bytes {
            return Err(IoError::LimitExceeded(
                "streaming file exceeds its byte limit",
            ));
        }
        self.context.set_phase(JobPhase::Reading);
        let result = action(&mut file)?;
        if backend::stamp(&file)? != before {
            return Err(IoError::ChangedDuringRead);
        }
        self.context.check_cancelled()?;
        self.manager
            .inner
            .cache
            .counters
            .reads
            .fetch_add(1, Ordering::Relaxed);
        self.context.record_read_completed();
        Ok(result)
    }

    pub fn write_atomic(
        &self,
        path: &Path,
        action: impl FnOnce(&mut File) -> IoResult<()>,
    ) -> IoResult<()> {
        self.write_with_policy(path, true, action)
    }

    pub fn write_new_atomic(
        &self,
        path: &Path,
        action: impl FnOnce(&mut File) -> IoResult<()>,
    ) -> IoResult<()> {
        self.write_with_policy(path, false, action)
    }

    pub fn write_bytes_atomic(&self, path: &Path, bytes: &[u8]) -> IoResult<()> {
        self.write_atomic(path, |file| {
            for chunk in bytes.chunks(64 * 1024) {
                self.context.check_cancelled()?;
                file.write_all(chunk)?;
            }
            Ok(())
        })
    }

    fn write_with_policy(
        &self,
        path: &Path,
        overwrite: bool,
        action: impl FnOnce(&mut File) -> IoResult<()>,
    ) -> IoResult<()> {
        self.context.check_cancelled()?;
        let destination = self.resolve_member(path)?;
        let previous = match File::open(&destination) {
            Ok(file) => Some(backend::stamp(&file)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if previous.is_some_and(|stamp| stamp.readonly) {
            return Err(IoError::InvalidInput("destination is read-only"));
        }
        let parent = destination
            .parent()
            .ok_or(IoError::InvalidInput("destination parent is missing"))?;
        let mut temporary = reserve_temporary(parent)?;
        self.context.set_phase(JobPhase::Writing);
        let file = temporary
            .file
            .as_mut()
            .ok_or(IoError::InvalidInput("temporary writer is already closed"))?;
        action(file)?;
        file.flush()?;
        file.sync_all()?;
        drop(temporary.file.take());
        self.context.check_cancelled()?;
        self.context.set_phase(JobPhase::Installing);
        backend::replace(&temporary.path, &destination, overwrite)?;
        temporary.published = true;
        if let Some(previous) = previous {
            self.manager.inner.cache.invalidate(previous.identity);
        }
        Ok(())
    }

    pub fn remove(&self, path: &Path) -> IoResult<()> {
        self.context.check_cancelled()?;
        let path = self.resolve_member(path)?;
        let stamp = match File::open(&path) {
            Ok(file) => Some(backend::stamp(&file)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        std::fs::remove_file(path)?;
        if let Some(stamp) = stamp {
            self.manager.inner.cache.invalidate(stamp.identity);
        }
        Ok(())
    }

    /// Renames within the transaction without replacing an existing destination.
    pub fn rename(&self, source: &Path, destination: &Path) -> IoResult<()> {
        self.rename_with_policy(source, destination, false)
    }

    pub fn replace(&self, source: &Path, destination: &Path) -> IoResult<()> {
        self.rename_with_policy(source, destination, true)
    }

    fn rename_with_policy(
        &self,
        source: &Path,
        destination: &Path,
        overwrite: bool,
    ) -> IoResult<()> {
        self.context.check_cancelled()?;
        let source = self.resolve_member(source)?;
        let destination = self.resolve_member(destination)?;
        let source_stamp = backend::stamp(&File::open(&source)?)?;
        let destination_stamp = File::open(&destination)
            .ok()
            .map(|file| backend::stamp(&file))
            .transpose()?;
        backend::replace(&source, &destination, overwrite)?;
        self.manager.inner.cache.invalidate(source_stamp.identity);
        if let Some(stamp) = destination_stamp {
            self.manager.inner.cache.invalidate(stamp.identity);
        }
        Ok(())
    }
}

struct TemporaryFile {
    path: PathBuf,
    file: Option<File>,
    published: bool,
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        // Close before deleting, including while a caller-supplied writer
        // unwinds. Windows may reject deleting an open temporary file.
        drop(self.file.take());
        if !self.published {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn reserve_temporary(parent: &Path) -> IoResult<TemporaryFile> {
    for _ in 0..128 {
        let sequence = TEMPORARY_SEQUENCE
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map_err(|_| IoError::LimitExceeded("temporary sequence exhausted"))?;
        let path = parent.join(format!(".inkpod-io-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                return Ok(TemporaryFile {
                    path,
                    file: Some(file),
                    published: false,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(IoError::ResourceBusy(
        "temporary file namespace is exhausted",
    ))
}
