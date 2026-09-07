//! Runtime file authority for guarded publication. No authority is serialized.

use crate::{FileIdentity, FileStamp, IoError, IoManager, IoResult, JobContext, backend};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);

/// Exact physical object and nearest existing parent captured without writing.
/// Missing descendants retain their nearest existing ancestor as authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathAuthority {
    pub path: PathBuf,
    pub canonical_key: String,
    pub alias_key: [u8; 32],
    pub parent_alias_key: [u8; 32],
    pub object: Option<FileIdentity>,
    pub parent_path: PathBuf,
    pub parent: FileIdentity,
    pub is_directory: bool,
}

/// Complete file-source observation retained until output publication. Existing
/// destinations additionally require this exact source as their overwrite proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishSource {
    pub path: PathBuf,
    pub stamp: FileStamp,
    pub digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardedPublishOutcome {
    Installed,
    CancelledBeforeInstall,
}

impl IoManager {
    /// Keeps a checked directory object pinned while a caller enumerates and
    /// ingests its files. The original approved path is revalidated as well.
    pub fn with_directory_authority<T>(
        &self,
        approved_path: &Path,
        expected: &PathAuthority,
        context: &JobContext,
        action: impl FnOnce() -> IoResult<T>,
    ) -> IoResult<T> {
        if self.observe_path_authority(approved_path, context)? != *expected
            || !expected.is_directory
        {
            return Err(IoError::ConfirmationRequired);
        }
        let guard = backend::open_authority_directory(&expected.path, false)?;
        if backend::object_identity(&guard)?
            != expected.object.ok_or(IoError::ConfirmationRequired)?
        {
            return Err(IoError::ConfirmationRequired);
        }
        if self.observe_path_authority(approved_path, context)? != *expected {
            return Err(IoError::ConfirmationRequired);
        }
        let result = action()?;
        if self.observe_path_authority(approved_path, context)? != *expected {
            return Err(IoError::ConfirmationRequired);
        }
        Ok(result)
    }

    /// Creates descendants of one approved existing ancestor using only
    /// handle-relative backend creation. Existing ancestors stay pinned for the
    /// entire chain; cancellation leaves created directories explicitly reported.
    pub fn prepare_directory_chain(
        &self,
        destination: &PathAuthority,
        context: &JobContext,
        cancelled: &mut dyn FnMut() -> bool,
        created: &mut Vec<PathAuthority>,
    ) -> IoResult<PathAuthority> {
        if self.observe_path_authority(&destination.path, context)? != *destination {
            return Err(IoError::ConfirmationRequired);
        }
        let mut guards = vec![backend::open_authority_directory(
            &destination.parent_path,
            true,
        )?];
        if backend::object_identity(&guards[0])? != destination.parent {
            return Err(IoError::ConfirmationRequired);
        }
        let target_parent = destination
            .path
            .parent()
            .ok_or(IoError::InvalidInput("destination parent is missing"))?;
        let relative = target_parent
            .strip_prefix(&destination.parent_path)
            .map_err(|_| IoError::ConfirmationRequired)?;
        let mut path = destination.parent_path.clone();
        for component in relative.components() {
            if cancelled() {
                return Err(IoError::Cancelled);
            }
            context.check_cancelled()?;
            let parent_path = path.clone();
            path.push(component);
            let child = backend::create_authority_child(
                guards.last().ok_or(IoError::ConfirmationRequired)?,
                &path,
                true,
            )?;
            let identity = backend::object_identity(&child)?;
            let proof = PathAuthority {
                path: path.clone(),
                canonical_key: backend::canonical_path_key(&path)?,
                alias_key: backend::path_alias_key(&path)?,
                parent_alias_key: backend::path_alias_key(&parent_path)?,
                object: Some(identity),
                parent_path,
                parent: backend::object_identity(
                    guards.last().ok_or(IoError::ConfirmationRequired)?,
                )?,
                is_directory: true,
            };
            created.push(proof.clone());
            if self.observe_path_authority(&path, context)? != proof {
                return Err(IoError::ConfirmationRequired);
            }
            // Retain a write-capable parent handle for the next relative child.
            let parent = backend::open_authority_directory(&path, true)?;
            if backend::object_identity(&parent)? != identity {
                return Err(IoError::ConfirmationRequired);
            }
            guards.push(parent);
        }
        if cancelled() {
            return Err(IoError::Cancelled);
        }
        self.observe_path_authority(&destination.path, context)
    }

    /// Observes explicit absolute authority. Relative paths never fall back to cwd.
    pub fn observe_path_authority(
        &self,
        path: &Path,
        context: &JobContext,
    ) -> IoResult<PathAuthority> {
        self.check_running(context)?;
        if !path.is_absolute() {
            return Err(IoError::InvalidInput("authority path must be absolute"));
        }
        let path = backend::resolve(path)?;
        let metadata = match std::fs::metadata(&path) {
            Ok(value) => Some(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let object = if metadata.is_some() {
            Some(backend::path_object_identity(&path)?)
        } else {
            None
        };
        let mut parent_path = path.parent().unwrap_or(&path).to_path_buf();
        while !parent_path.is_dir() {
            parent_path = parent_path
                .parent()
                .ok_or(IoError::InvalidInput("authority has no directory ancestor"))?
                .to_path_buf();
        }
        let parent = backend::path_object_identity(&parent_path)?;
        let canonical_key = backend::canonical_path_key(&path)?;
        let alias_key = backend::path_alias_key(&path)?;
        let parent_alias_key = backend::path_alias_key(&parent_path)?;
        Ok(PathAuthority {
            path,
            canonical_key,
            alias_key,
            parent_alias_key,
            object,
            parent_path,
            parent,
            is_directory: metadata.is_some_and(|value| value.is_dir()),
        })
    }

    /// True only where the private backend can anchor rename to a locked parent
    /// handle and deny external source writers during the final digest check.
    pub const fn supports_guarded_publication(&self) -> bool {
        backend::supports_guarded_publication()
    }

    /// Writes, flushes and closes one same-directory temporary, revalidates source
    /// bytes under an OS writer-excluding guard, then atomically publishes through
    /// the captured parent handle. No-replace installs never overwrite a race.
    /// Every file source is finally revalidated; overwrite requires the source
    /// path to equal destination. A snapshot-only input may omit file source.
    /// Overwrite retains a guard against source write opens through installation,
    /// but permits source rename/delete so the atomic replacement can succeed.
    /// Name changes observed at the final authority check reject publication;
    /// an external replacement after that check can still be overwritten. This
    /// is an optimistic conflict check, not an atomic compare-and-replace.
    /// Cancellation before install deletes the temporary; successful publication
    /// is never reported as cancelled. Other items are outside this transaction.
    pub fn publish_guarded(
        &self,
        destination: &PathAuthority,
        source: Option<PublishSource>,
        bytes: &[u8],
        context: &JobContext,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> IoResult<GuardedPublishOutcome> {
        self.publish_guarded_with_ancestor(destination, source, bytes, context, cancelled, None)
    }

    /// Like `publish_guarded`, also pins the original approved directory ancestor
    /// and rechecks its original path immediately before publication. The optional
    /// pair contains the approved path and its captured physical authority. No
    /// fallible authority observation occurs after atomic installation.
    pub fn publish_guarded_with_ancestor(
        &self,
        destination: &PathAuthority,
        source: Option<PublishSource>,
        bytes: &[u8],
        context: &JobContext,
        cancelled: &mut dyn FnMut() -> bool,
        ancestor: Option<(&Path, &PathAuthority)>,
    ) -> IoResult<GuardedPublishOutcome> {
        if !self.supports_guarded_publication() {
            return Err(IoError::InvalidInput(
                "guarded publication is unsupported on this platform",
            ));
        }
        let source = source
            .map(|mut value| {
                if !value.path.is_absolute() {
                    return Err(IoError::InvalidInput("source authority must be absolute"));
                }
                value.path = backend::resolve(&value.path)?;
                Ok(value)
            })
            .transpose()?;
        let overwrite = destination.object.is_some();
        if destination.is_directory
            || (overwrite
                && source
                    .as_ref()
                    .is_none_or(|value| value.path != destination.path))
        {
            return Err(IoError::InvalidInput(
                "publication policy does not match authority",
            ));
        }
        let mut lock_paths = vec![destination.path.clone()];
        if let Some(source) = &source {
            lock_paths.push(source.path.clone());
        }
        self.with_file_locks(&lock_paths, context, |_| {
            let _ancestor_guard = ancestor
                .map(|(approved, expected)| {
                    if !expected.is_directory
                        || !destination.path.starts_with(&expected.path)
                        || self.observe_path_authority(approved, context)? != *expected
                    {
                        return Err(IoError::ConfirmationRequired);
                    }
                    let guard = backend::open_authority_directory(&expected.path, false)?;
                    if Some(backend::object_identity(&guard)?) != expected.object
                        || self.observe_path_authority(approved, context)? != *expected
                    {
                        return Err(IoError::ConfirmationRequired);
                    }
                    Ok(guard)
                })
                .transpose()?;
            let observed = self.observe_path_authority(&destination.path, context)?;
            if observed != *destination {
                return Err(IoError::ConfirmationRequired);
            }
            if destination.path.parent() != Some(destination.parent_path.as_path()) {
                return Err(IoError::InvalidInput(
                    "publication parent has not been prepared",
                ));
            }
            let parent = backend::open_authority_directory(&destination.parent_path, true)?;
            if backend::object_identity(&parent)? != destination.parent {
                return Err(IoError::ConfirmationRequired);
            }
            let temporary = reserve_temporary(&destination.parent_path, &parent)?;
            let mut owner = TemporaryOwner {
                parent: &parent,
                path: temporary.0,
                file: Some(temporary.1),
                identity: temporary.2,
                installed: false,
            };
            let file = owner
                .file
                .as_mut()
                .ok_or(IoError::InvalidInput("temporary is closed"))?;
            for chunk in bytes.chunks(64 * 1024) {
                if cancelled() {
                    return Ok(GuardedPublishOutcome::CancelledBeforeInstall);
                }
                context.check_cancelled()?;
                file.write_all(chunk)?;
            }
            file.flush()?;
            file.sync_all()?;
            let temporary_stamp = backend::stamp(file)?;
            drop(owner.file.take());
            let rename_file = backend::open_authority_temporary(&parent, &owner.path)?;
            if backend::stamp(&rename_file)? != temporary_stamp {
                return Err(IoError::ConfirmationRequired);
            }
            let guard = if let Some(expected) = &source {
                let mut file = backend::open_authority_source(&expected.path, overwrite)?;
                if backend::stamp(&file)? != expected.stamp {
                    return Err(IoError::ConfirmationRequired);
                }
                let mut hasher = blake3::Hasher::new();
                let mut chunk = [0u8; 64 * 1024];
                loop {
                    if cancelled() {
                        return Ok(GuardedPublishOutcome::CancelledBeforeInstall);
                    }
                    context.check_cancelled()?;
                    let count = file.read(&mut chunk)?;
                    if count == 0 {
                        break;
                    }
                    hasher.update(&chunk[..count]);
                }
                if hasher.finalize().as_bytes() != &expected.digest
                    || backend::stamp(&file)? != expected.stamp
                {
                    return Err(IoError::ConfirmationRequired);
                }
                Some(file)
            } else {
                None
            };
            if cancelled() {
                return Ok(GuardedPublishOutcome::CancelledBeforeInstall);
            }
            context.check_cancelled()?;
            if self.observe_path_authority(&destination.path, context)? != *destination {
                return Err(IoError::ConfirmationRequired);
            }
            // Retain source WRITE exclusion through rename. An overwrite source
            // shares DELETE, so this final path check detects observed name
            // changes but cannot exclude a later external name replacement.
            if let Some(expected) = &source {
                if backend::stamp(&File::open(&expected.path)?)? != expected.stamp {
                    return Err(IoError::ConfirmationRequired);
                }
            }
            if let Some((approved, expected)) = ancestor {
                if self.observe_path_authority(approved, context)? != *expected {
                    return Err(IoError::ConfirmationRequired);
                }
            }
            backend::rename_with_authority(&rename_file, &parent, &destination.path, overwrite)?;
            drop(guard);
            owner.installed = true;
            if let Some(expected) = &source {
                self.inner.cache.invalidate(expected.stamp.identity);
            }
            Ok(GuardedPublishOutcome::Installed)
        })
    }
}

struct TemporaryOwner<'a> {
    parent: &'a File,
    path: PathBuf,
    file: Option<File>,
    identity: FileIdentity,
    installed: bool,
}
impl Drop for TemporaryOwner<'_> {
    fn drop(&mut self) {
        drop(self.file.take());
        if !self.installed {
            let _ = backend::remove_authority_temporary(self.parent, &self.path, self.identity);
        }
    }
}
fn reserve_temporary(
    parent: &Path,
    parent_handle: &File,
) -> IoResult<(PathBuf, File, FileIdentity)> {
    for _ in 0..128 {
        let sequence = NEXT_TEMPORARY
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map_err(|_| IoError::LimitExceeded("temporary sequence exhausted"))?;
        let path = parent.join(format!(
            ".inkpod-script-{}-{sequence}.tmp",
            std::process::id()
        ));
        match backend::create_authority_child(parent_handle, &path, false) {
            Ok(file) => {
                let identity = backend::stamp(&file)?.identity;
                return Ok((path, file, identity));
            }
            Err(IoError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(IoError::ResourceBusy("could not reserve temporary output"))
}
