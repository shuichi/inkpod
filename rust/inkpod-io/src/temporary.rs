use crate::backend;
use crate::{IoError, IoManager, IoResult, JobContext};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// An exclusively created job directory. Explicit cleanup reports failures;
/// Drop queues last-resort cleanup without blocking the caller. Explicit cleanup
/// must complete before a preview result is published as successful.
pub struct TemporaryDirectory {
    manager: IoManager,
    // Shared allocation root. A job owns only `path`; removing this root would
    // race another allocator between its create and canonicalize operations.
    base: PathBuf,
    path: PathBuf,
    active: bool,
}

impl TemporaryDirectory {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cleanup(mut self) -> IoResult<()> {
        self.manager
            .remove_tree(&self.path, &self.base, &JobContext::new())?;
        self.active = false;
        Ok(())
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if self.active {
            let manager = self.manager.clone();
            let path = self.path.clone();
            let base = self.base.clone();
            let _ = self.manager.enqueue_cleanup(move || {
                let _ = manager.remove_tree(&path, &base, &JobContext::new());
            });
        }
    }
}

impl IoManager {
    pub fn create_temporary_directory(
        &self,
        prefix: &str,
        context: &JobContext,
    ) -> IoResult<TemporaryDirectory> {
        self.check_running(context)?;
        if prefix.is_empty()
            || prefix.len() > 64
            || !prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(IoError::InvalidInput(
                "temporary directory prefix is invalid",
            ));
        }
        let base = std::env::temp_dir().join("inkpod-file-io");
        std::fs::create_dir_all(&base)?;
        let base = std::fs::canonicalize(base)?;
        for _ in 0..128 {
            context.check_cancelled()?;
            let sequence = DIRECTORY_SEQUENCE
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| IoError::LimitExceeded("temporary directory sequence exhausted"))?;
            let path = base.join(format!("{prefix}-{}-{sequence}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(TemporaryDirectory {
                        manager: self.clone(),
                        base,
                        path,
                        active: true,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(IoError::ResourceBusy(
            "temporary directory namespace exhausted",
        ))
    }

    /// Copies a frozen encoded source to a new file under source/destination
    /// coordination, with chunk cancellation and a caller-selected file bound.
    /// Callers reserve their aggregate temporary disk budget before this call.
    pub fn copy_file(
        &self,
        source: &Path,
        destination: &Path,
        maximum_bytes: u64,
        context: &JobContext,
    ) -> IoResult<u64> {
        self.copy_file_with_cancel(source, destination, maximum_bytes, context, || false)
    }

    pub fn copy_file_with_cancel(
        &self,
        source: &Path,
        destination: &Path,
        maximum_bytes: u64,
        context: &JobContext,
        mut cancelled: impl FnMut() -> bool,
    ) -> IoResult<u64> {
        if source == destination {
            return Err(IoError::InvalidInput("copy source equals destination"));
        }
        self.with_file_locks(
            &[source.to_path_buf(), destination.to_path_buf()],
            context,
            |files| {
                let mut input = File::open(files.resolve_member(source)?)?;
                let before = backend::stamp(&input)?;
                if before.length > maximum_bytes {
                    return Err(IoError::LimitExceeded("copied file exceeds its bound"));
                }
                let mut length = 0_u64;
                files.write_new_atomic(destination, |output| {
                    let mut buffer = [0_u8; 64 * 1024];
                    loop {
                        context.check_cancelled()?;
                        if cancelled() {
                            return Err(IoError::Cancelled);
                        }
                        let read = input.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        length = length
                            .checked_add(read as u64)
                            .filter(|length| *length <= maximum_bytes)
                            .ok_or(IoError::LimitExceeded("copied file grew beyond its bound"))?;
                        output.write_all(&buffer[..read])?;
                    }
                    if backend::stamp(&input)? != before || length != before.length {
                        return Err(IoError::ChangedDuringRead);
                    }
                    Ok(())
                })?;
                self.inner
                    .cache
                    .counters
                    .reads
                    .fetch_add(1, Ordering::Relaxed);
                context.record_read_completed();
                Ok(length)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IoConfig;
    use std::time::{Duration, Instant};

    #[test]
    fn cleanup_preserves_the_shared_allocation_root() {
        let manager = IoManager::new(IoConfig {
            worker_count: 1,
            ..IoConfig::default()
        })
        .unwrap();
        let context = JobContext::new();
        let fixture = manager
            .create_temporary_directory("temporary-root-contract", &context)
            .unwrap();
        let shared_root = fixture.path().join("shared-root");
        let explicit_path = shared_root.join("explicit-job");
        std::fs::create_dir_all(&explicit_path).unwrap();
        let temporary = TemporaryDirectory {
            manager: manager.clone(),
            base: std::fs::canonicalize(&shared_root).unwrap(),
            path: std::fs::canonicalize(&explicit_path).unwrap(),
            active: true,
        };

        temporary.cleanup().unwrap();

        assert!(!explicit_path.exists());
        assert!(
            shared_root.is_dir(),
            "one job cleanup must not remove the root used by other allocators"
        );

        let deferred_path = shared_root.join("deferred-job");
        std::fs::create_dir(&deferred_path).unwrap();
        drop(TemporaryDirectory {
            manager: manager.clone(),
            base: std::fs::canonicalize(&shared_root).unwrap(),
            path: std::fs::canonicalize(&deferred_path).unwrap(),
            active: true,
        });
        let barrier = manager.submit(|_| Ok(())).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(result) = barrier.try_take() {
                result.unwrap();
                break;
            }
            assert!(
                Instant::now() < deadline,
                "deferred temporary cleanup did not finish"
            );
            std::thread::yield_now();
        }
        assert!(!deferred_path.exists());
        assert!(
            shared_root.is_dir(),
            "deferred job cleanup must not remove the shared allocation root"
        );

        std::fs::remove_dir(&shared_root).unwrap();
        drop(fixture);
        manager.shutdown_and_wait();
    }
}
