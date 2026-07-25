use super::decode::decode;
use super::encode::encode;
use super::model::{CellFile, FormatError, MAX_FILE_BYTES, TEMP_SEQUENCE};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
pub fn read(path: &Path) -> Result<CellFile, FormatError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("file exceeds the bounded size"));
    }
    let input = OpenOptions::new().read(true).open(path)?;
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| FormatError::Invalid("file length is not representable"))?;
    let mut bytes = Vec::with_capacity(capacity);
    input.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("file exceeds the bounded size"));
    }
    decode(&bytes)
}

pub fn save_atomic(path: &Path, document: &CellFile) -> Result<(), FormatError> {
    save_atomic_with_cancel(path, document, || false)
}

/// Recovery uses the same bounded, atomic container write as a normal save.
/// Savepoint and normal-path semantics deliberately remain a Core concern.
pub fn save_recovery_atomic(path: &Path, document: &CellFile) -> Result<(), FormatError> {
    save_atomic(path, document)
}

pub fn recovery_is_newer(normal_path: &Path, recovery_path: &Path) -> Result<bool, FormatError> {
    let recovery = match fs::metadata(recovery_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let normal = match fs::metadata(normal_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    Ok(recovery.modified()? > normal.modified()?)
}

pub fn discard_recovery(path: &Path) -> Result<(), FormatError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn save_atomic_with_cancel(
    path: &Path,
    document: &CellFile,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(), FormatError> {
    if is_cancelled() {
        return Err(FormatError::Cancelled);
    }
    let bytes = encode(document)?;
    if is_cancelled() {
        return Err(FormatError::Cancelled);
    }
    let (temporary_path, mut temporary) = create_temporary(path)?;
    let result = (|| {
        for chunk in bytes.chunks(1_048_576) {
            if is_cancelled() {
                return Err(FormatError::Cancelled);
            }
            temporary.write_all(chunk)?;
        }
        temporary.flush()?;
        temporary.sync_all()?;
        drop(temporary);
        if is_cancelled() {
            return Err(FormatError::Cancelled);
        }
        fs::rename(&temporary_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn create_temporary(path: &Path) -> Result<(PathBuf, std::fs::File), FormatError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    path.file_name()
        .ok_or(FormatError::Invalid("destination has no file name"))?;
    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_name = format!(".inkpod.tmp.{}.{}", std::process::id(), sequence);
        let temporary_path = parent.join(temporary_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(FormatError::Io(error)),
        }
    }
    Err(FormatError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not reserve a same-directory temporary file",
    )))
}
