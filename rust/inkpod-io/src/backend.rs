use crate::{IoError, IoResult};
use std::fs::File;
use std::path::{Component, Path, PathBuf};

#[cfg(windows)]
mod windows;

/// Physical file identity. Hard links on the same volume have the same identity.
/// It is runtime authority only and must never be serialized into a procedure.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct FileIdentity {
    pub volume: u64,
    pub file: u128,
}

/// File identity and a version observation, captured from an open handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileStamp {
    pub identity: FileIdentity,
    pub length: u64,
    pub modified: i128,
    pub changed: i128,
    pub readonly: bool,
}

impl FileStamp {
    /// Classifies a complete-stamp mismatch that can be checked by rereading the
    /// same byte extent. This is only a reason to retry; it does not prove byte
    /// equality or a stable content revision.
    pub(crate) const fn same_read_extent(self, other: Self) -> bool {
        self.identity.volume == other.identity.volume
            && self.identity.file == other.identity.file
            && self.length == other.length
    }
}

pub(crate) fn stamp(file: &File) -> IoResult<FileStamp> {
    #[cfg(windows)]
    {
        windows::stamp(file)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(IoError::InvalidInput("image input is not a regular file"));
        }
        Ok(FileStamp {
            identity: FileIdentity {
                volume: metadata.dev(),
                file: u128::from(metadata.ino()),
            },
            length: metadata.len(),
            modified: i128::from(metadata.mtime()) * 1_000_000_000
                + i128::from(metadata.mtime_nsec()),
            changed: i128::from(metadata.ctime()) * 1_000_000_000
                + i128::from(metadata.ctime_nsec()),
            readonly: metadata.permissions().readonly(),
        })
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = file;
        Err(IoError::InvalidInput(
            "physical file identities are unavailable on this platform",
        ))
    }
}

pub(crate) fn resolve(path: &Path) -> IoResult<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(IoError::InvalidInput("file path is empty"));
    }
    if path.as_os_str().len() > 32_768 {
        return Err(IoError::LimitExceeded("file path exceeds its length limit"));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    let mut existing = normalized.as_path();
    let mut missing = Vec::new();
    loop {
        match std::fs::canonicalize(existing) {
            Ok(mut base) => {
                for component in missing.iter().rev() {
                    base.push(component);
                }
                return Ok(base);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing
                    .file_name()
                    .ok_or(IoError::InvalidInput("file path has no existing ancestor"))?;
                missing.push(name.to_os_string());
                existing = existing
                    .parent()
                    .ok_or(IoError::InvalidInput("file path has no parent"))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub(crate) fn lock_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.as_os_str().to_string_lossy().to_uppercase())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

pub(crate) fn normalized_leaf(name: &str) -> String {
    #[cfg(windows)]
    {
        name.to_uppercase()
    }
    #[cfg(not(windows))]
    {
        name.to_owned()
    }
}

pub(crate) fn missing_identity(path: &Path) -> FileIdentity {
    let key = lock_path(path);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"inkpod.missing-file-path.v1");
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        for unit in key.as_os_str().encode_wide() {
            hasher.update(&unit.to_le_bytes());
        }
    }
    #[cfg(not(windows))]
    {
        hasher.update(key.as_os_str().as_encoded_bytes());
    }
    let digest = hasher.finalize();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    FileIdentity {
        volume: u64::MAX,
        file: u128::from_le_bytes(id),
    }
}

pub(crate) fn replace(source: &Path, destination: &Path, overwrite: bool) -> IoResult<()> {
    #[cfg(windows)]
    {
        windows::replace(source, destination, overwrite)
    }
    #[cfg(not(windows))]
    {
        if overwrite {
            std::fs::rename(source, destination)?;
        } else {
            // Both files are in the destination directory: link is an atomic
            // no-replace publication, unlike an exists/rename race.
            std::fs::hard_link(source, destination)?;
            std::fs::remove_file(source)?;
        }
        if let Some(parent) = destination.parent() {
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{FileIdentity, FileStamp};

    #[test]
    fn retry_extent_classifier_does_not_claim_byte_equality() {
        let stamp = FileStamp {
            identity: FileIdentity {
                volume: 7,
                file: 11,
            },
            length: 13,
            modified: 17,
            changed: 19,
            readonly: false,
        };
        assert!(stamp.same_read_extent(FileStamp {
            changed: 23,
            readonly: true,
            ..stamp
        }));
        assert!(stamp.same_read_extent(FileStamp {
            modified: 18,
            ..stamp
        }));
        assert!(!stamp.same_read_extent(FileStamp {
            identity: FileIdentity {
                volume: 7,
                file: 12,
            },
            ..stamp
        }));
        assert!(!stamp.same_read_extent(FileStamp {
            length: 14,
            ..stamp
        }));
    }
}
