use crate::backend;
use crate::cache::{BudgetKind, ImageCache};
use crate::companion::CompanionDirectoryCache;
use crate::executor::{Executor, Work};
use crate::file_lock::{FileLocks, lock_unpoisoned};
use crate::image::{ByteLease, ImageLease};
use crate::job::{BatchOutput, JobOutput};
use crate::{
    CacheStats, DecodedLease, FileIdentity, FileStamp, ImageBatch, ImageBatchItem, IoConfig,
    IoError, IoJob, IoResult, JobContext, JobPhase, JobState, LoadedBytes, LoadedImage,
};
use inkpod_format::{
    CommonRasterFormat, common_raster_decode_allocation_limit, decode_common_raster,
};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) struct ManagerInner {
    pub(crate) config: IoConfig,
    pub(crate) cache: ImageCache,
    pub(crate) companion_directories: Mutex<CompanionDirectoryCache>,
    pub(crate) locks: FileLocks,
    pub(crate) pair_owners: Mutex<BTreeSet<PathBuf>>,
}

/// One application-owned service shared by all sessions and reference viewers.
/// Cloning shares workers, per-file coordination, encoded/decoded budgets, and
/// the sequence display subset. There is no process singleton and no dependency
/// on document or GUI ownership.
#[derive(Clone)]
pub struct IoManager {
    pub(crate) inner: Arc<ManagerInner>,
    executor: Arc<Executor>,
}

impl IoManager {
    pub fn new(config: IoConfig) -> IoResult<Self> {
        config.validate()?;
        let executor = Arc::new(Executor::new(config.worker_count, config.queue_capacity)?);
        Ok(Self {
            inner: Arc::new(ManagerInner {
                cache: ImageCache::new(config.clone()),
                companion_directories: Mutex::new(CompanionDirectoryCache::default()),
                locks: FileLocks::default(),
                pair_owners: Mutex::new(BTreeSet::new()),
                config,
            }),
            executor,
        })
    }

    /// Rejects new work and cooperatively cancels submitted jobs. Owners retain
    /// job results/leases until released. No live Core is accessed by shutdown.
    /// Existing image leases can still reserve memory without submitting I/O.
    pub fn shutdown(&self) {
        self.executor.shutdown();
    }

    /// Cancels jobs and waits for workers on a shutdown/engine thread. Do not call
    /// from UI polling or from a worker that depends on other queued jobs.
    pub fn shutdown_and_wait(&self) {
        self.executor.shutdown_and_wait();
    }

    pub(crate) fn enqueue_cleanup(&self, action: impl FnOnce() + Send + 'static) -> IoResult<()> {
        let context = JobContext::new();
        // Once admitted, cleanup remains best effort during shutdown. The
        // individual operation still decides whether closed-manager work is safe.
        let mut action = Some(action);
        self.executor.enqueue(
            vec![Box::new(move || {
                if let Some(action) = action.take() {
                    action();
                }
                false
            })],
            &context,
        )
    }

    #[must_use]
    pub fn cache_stats(&self) -> CacheStats {
        self.inner.cache.stats()
    }

    /// Removes cache ownership; leased allocations remain charged until dropped.
    pub fn clear_cache(&self) {
        self.inner.cache.clear();
        lock_unpoisoned(&self.inner.companion_directories).clear();
    }

    /// Returns physical authority for an existing file, or stable normalized path
    /// authority for a missing destination (including missing nested directories).
    /// The boolean distinguishes physical identity from the path-hash namespace.
    pub fn resolve_identity(&self, path: &Path) -> IoResult<(FileIdentity, bool)> {
        let path = backend::resolve(path)?;
        match File::open(&path) {
            Ok(file) => Ok((backend::stamp(&file)?.identity, true)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok((backend::missing_identity(&path), false))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Resolves an absolute normalized path through its longest existing
    /// ancestor. The result is runtime-only path authority and performs no file
    /// creation or mutation.
    pub fn normalize_path(&self, path: &Path) -> IoResult<PathBuf> {
        backend::resolve(path)
    }

    /// Reserves budget before a consumer allocates a second decoded/tiled copy.
    /// Keep the returned lease with that allocation (including its COW owners).
    pub fn reserve_derived_image(
        &self,
        source: &LoadedImage,
        bytes: u64,
    ) -> IoResult<DecodedLease> {
        if !Arc::ptr_eq(&self.inner, &source.cache_owner) {
            return Err(IoError::InvalidInput(
                "image belongs to another I/O manager",
            ));
        }
        source.reserve_derived(bytes)
    }

    pub(crate) fn check_running(&self, context: &JobContext) -> IoResult<()> {
        context.check_cancelled()?;
        if self.executor.is_shutdown() {
            Err(IoError::Shutdown)
        } else {
            Ok(())
        }
    }

    /// Executes one closure on the bounded pool. The closure must never wait for
    /// a child job on this pool; use `submit_images` for independent file work.
    /// Queue rejection does not execute or publish the closure.
    pub fn submit<T: Send + 'static>(
        &self,
        action: impl FnOnce(JobContext) -> IoResult<T> + Send + 'static,
    ) -> IoResult<IoJob<T>> {
        self.submit_with_context(JobContext::new(), action)
    }

    pub fn submit_with_context<T: Send + 'static>(
        &self,
        context: JobContext,
        action: impl FnOnce(JobContext) -> IoResult<T> + Send + 'static,
    ) -> IoResult<IoJob<T>> {
        self.check_running(&context)?;
        let output = Arc::new(JobOutput {
            result: Mutex::new(None),
        });
        let task_output = Arc::clone(&output);
        let task_context = context.clone();
        let mut action = Some(action);
        let work = Box::new(move || {
            let Some(action) = action.take() else {
                return false;
            };
            task_context.update(|progress| progress.state = JobState::Running);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                task_context.check_cancelled()?;
                action(task_context.clone())
            }))
            .unwrap_or(Err(IoError::WorkerPanicked));
            let error_state = result.as_ref().err();
            // Publish the result before the terminal state becomes observable.
            let terminal = match error_state {
                Some(IoError::Cancelled) => JobState::Cancelled,
                Some(_) => JobState::Failed,
                None => JobState::Completed,
            };
            *lock_unpoisoned(&task_output.result) = Some(result);
            task_context.update(|progress| {
                progress.state = terminal;
                progress.phase = JobPhase::Finished;
            });
            false
        }) as Work;
        self.executor.enqueue(vec![work], &context)?;
        Ok(IoJob { context, output })
    }

    /// Runs independent reads in bounded worker lanes which yield between files.
    /// No worker waits for a coordinator or queue capacity. Results are indexed
    /// in input order and can be drained while later files are loading.
    pub fn submit_images(&self, paths: Vec<PathBuf>, force_reload: bool) -> IoResult<ImageBatch> {
        self.submit_images_with_context(paths, force_reload, JobContext::new())
    }

    pub fn submit_images_with_context(
        &self,
        paths: Vec<PathBuf>,
        force_reload: bool,
        context: JobContext,
    ) -> IoResult<ImageBatch> {
        self.check_running(&context)?;
        if paths.is_empty() || paths.len() > 16_384 {
            return Err(IoError::LimitExceeded(
                "image job count is outside 1..=16384",
            ));
        }
        let count = paths.len();
        let lanes = self
            .inner
            .config
            .worker_count
            .min(self.inner.config.queue_capacity)
            .min(count);
        let paths = Arc::new(paths);
        let next = Arc::new(AtomicUsize::new(0));
        let remaining = Arc::new(AtomicUsize::new(lanes));
        let output = Arc::new(BatchOutput {
            results: Mutex::new((0..count).map(|_| None).collect()),
        });
        context.update(|progress| {
            progress.discovered = count as u64;
            progress.queued = count as u64;
            progress.total = count as u64;
        });
        let mut work = Vec::with_capacity(lanes);
        for _ in 0..lanes {
            let manager = self.clone();
            let paths = Arc::clone(&paths);
            let next = Arc::clone(&next);
            let remaining = Arc::clone(&remaining);
            let output = Arc::clone(&output);
            let context = context.clone();
            work.push(Box::new(move || {
                context.update(|progress| progress.state = JobState::Running);
                let index = next.fetch_add(1, Ordering::AcqRel);
                let Some(path) = paths.get(index) else {
                    if remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
                        context.update(|progress| {
                            progress.state = if progress.cancelled != 0 {
                                JobState::Cancelled
                            } else if progress.failed != 0 {
                                JobState::Failed
                            } else {
                                JobState::Completed
                            };
                            progress.phase = JobPhase::Finished;
                        });
                    }
                    return false;
                };
                context.update(|progress| {
                    progress.queued = progress.queued.saturating_sub(1);
                    progress.reading += 1;
                });
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    manager.read_image_with_reload(path, force_reload, &context)
                }))
                .unwrap_or(Err(IoError::WorkerPanicked));
                let cancelled = matches!(result, Err(IoError::Cancelled));
                let failed = result.is_err() && !cancelled;
                lock_unpoisoned(&output.results)[index] = Some(ImageBatchItem {
                    index,
                    path: path.clone(),
                    result,
                });
                context.update(|progress| {
                    progress.reading -= 1;
                    progress.completed += 1;
                    progress.failed += u64::from(failed);
                    progress.cancelled += u64::from(cancelled);
                });
                true
            }) as Work);
        }
        self.executor.enqueue(work, &context)?;
        Ok(ImageBatch { context, output })
    }

    pub fn read_image(&self, path: &Path, context: &JobContext) -> IoResult<LoadedImage> {
        self.read_image_with_reload(path, false, context)
    }

    pub fn read_image_with_reload(
        &self,
        path: &Path,
        force_reload: bool,
        context: &JobContext,
    ) -> IoResult<LoadedImage> {
        self.check_running(context)?;
        let format = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(CommonRasterFormat::from_extension)
            .ok_or(IoError::InvalidInput("image file extension is unsupported"))?;
        let result = self.with_file_locks(&[path.to_path_buf()], context, |files| {
            let path = files.resolve_member(path)?;
            let mut file = File::open(&path)?;
            let stamp = backend::stamp(&file)?;
            if force_reload {
                self.inner.cache.invalidate(stamp.identity);
            }
            let source = self.read_open_file(
                &mut file,
                path,
                stamp,
                self.inner.config.max_file_bytes,
                context,
            )?;
            let stamp = source.stamp();
            if let Some(raster) = self.inner.cache.decoded(stamp, format) {
                context.check_cancelled()?;
                return Ok(LoadedImage {
                    source,
                    format,
                    raster,
                    cache_owner: Arc::clone(&self.inner),
                });
            }
            context.set_phase(JobPhase::Decoding);
            let bytes = common_raster_decode_allocation_limit(format, source.bytes())?;
            let mut reservation = self.inner.cache.reserve(BudgetKind::Decoded, bytes)?;
            context.check_cancelled()?;
            let raster = decode_common_raster(format, source.bytes())?;
            if raster.pixels.capacity() as u64 > bytes {
                return Err(IoError::LimitExceeded(
                    "decoded image exceeded its reserved allocation",
                ));
            }
            context.check_cancelled()?;
            reservation.reduce_to(raster.pixels.capacity() as u64);
            let raster = ImageLease::new(raster, reservation, &source.lease);
            self.inner
                .cache
                .counters
                .decodes
                .fetch_add(1, Ordering::Relaxed);
            self.inner
                .cache
                .insert_decoded(stamp, format, raster.clone());
            Ok(LoadedImage {
                source,
                format,
                raster,
                cache_owner: Arc::clone(&self.inner),
            })
        })?;
        context.record_loaded();
        Ok(result)
    }

    /// Reads immutable bytes through the same physical-file lock and shared
    /// cache. `maximum_bytes` can lower, but cannot raise, the configured cap.
    pub fn read_bytes(
        &self,
        path: &Path,
        maximum_bytes: u64,
        context: &JobContext,
    ) -> IoResult<LoadedBytes> {
        self.with_file_locks(&[path.to_path_buf()], context, |files| {
            files.read_bytes(path, maximum_bytes)
        })
    }

    pub(crate) fn read_open_file(
        &self,
        file: &mut File,
        path: PathBuf,
        stamp: FileStamp,
        maximum_bytes: u64,
        context: &JobContext,
    ) -> IoResult<LoadedBytes> {
        let maximum_bytes = maximum_bytes.min(self.inner.config.max_file_bytes);
        if stamp.length > maximum_bytes {
            return Err(IoError::LimitExceeded(
                "encoded file exceeds its byte limit",
            ));
        }
        context.check_cancelled()?;
        if let Some((lease, generation)) = self.inner.cache.bytes(stamp) {
            return Ok(LoadedBytes {
                path,
                stamp,
                generation,
                lease,
            });
        }
        let slot = self.inner.cache.reserve(BudgetKind::Image, 1)?;
        let reservation = self
            .inner
            .cache
            .reserve(BudgetKind::Encoded, stamp.length)?;
        let length = usize::try_from(stamp.length)
            .map_err(|_| IoError::LimitExceeded("encoded file size is not addressable"))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| IoError::ResourceBusy("encoded file allocation failed"))?;
        if bytes.capacity() > length {
            return Err(IoError::ResourceBusy(
                "encoded allocation exceeded its reservation",
            ));
        }
        bytes.resize(length, 0);
        context.set_phase(JobPhase::Reading);
        for chunk in bytes.chunks_mut(64 * 1024) {
            context.check_cancelled()?;
            file.read_exact(chunk)?;
            context.update(|progress| {
                progress.completed_bytes =
                    progress.completed_bytes.saturating_add(chunk.len() as u64)
            });
        }
        let final_stamp = validate_or_retry_buffered_read(file, &path, stamp, &bytes, context)?;
        context.check_cancelled()?;
        self.inner
            .cache
            .counters
            .reads
            .fetch_add(1, Ordering::Relaxed);
        let lease = ByteLease::new(bytes, reservation, slot);
        let generation = self
            .inner
            .cache
            .insert_bytes(path.clone(), final_stamp, lease.clone())?;
        context.record_read_completed();
        Ok(LoadedBytes {
            path,
            stamp: final_stamp,
            generation,
            lease,
        })
    }
}

fn validate_or_retry_buffered_read(
    first_file: &File,
    path: &Path,
    initial_stamp: FileStamp,
    bytes: &[u8],
    context: &JobContext,
) -> IoResult<FileStamp> {
    let first_final_stamp = backend::stamp(first_file)?;
    if first_final_stamp == initial_stamp {
        return Ok(first_final_stamp);
    }
    if !initial_stamp.same_read_extent(first_final_stamp) {
        return Err(IoError::ChangedDuringRead);
    }

    // A same-file, same-length timestamp/attribute transition does not prove
    // that the bytes changed, but it cannot be ignored either. Trust the first
    // pass only after a fresh pass matches every byte and keeps one full stamp.
    context.check_cancelled()?;
    let mut retry = File::open(path)?;
    let retry_start = backend::stamp(&retry)?;
    if !initial_stamp.same_read_extent(retry_start) {
        return Err(IoError::ChangedDuringRead);
    }
    let mut verification = [0_u8; 64 * 1024];
    for expected in bytes.chunks(verification.len()) {
        context.check_cancelled()?;
        let actual = &mut verification[..expected.len()];
        retry.read_exact(actual)?;
        if actual != expected {
            return Err(IoError::ChangedDuringRead);
        }
    }
    let retry_final = backend::stamp(&retry)?;
    if retry_start != retry_final {
        return Err(IoError::ChangedDuringRead);
    }
    context.check_cancelled()?;
    Ok(retry_final)
}

impl std::fmt::Debug for IoManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IoManager")
            .field("config", &self.inner.config)
            .field("cache", &self.cache_stats())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::validate_or_retry_buffered_read;
    use crate::{IoError, JobContext, backend};
    use std::fs::{self, File, FileTimes};
    use std::io::Read;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    static PATH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn metadata_only_transition_retries_buffered_read_and_returns_stable_stamp() {
        let number = PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "inkpod-buffered-read-retry-{}-{number}.tga",
            std::process::id()
        ));
        let expected = b"bounded encoded TGA fixture";
        fs::write(&path, expected).unwrap();

        let mut first = File::open(&path).unwrap();
        let initial_stamp = backend::stamp(&first).unwrap();
        let mut bytes = Vec::new();
        first.read_to_end(&mut bytes).unwrap();
        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut changed_permissions = original_permissions.clone();
        changed_permissions.set_readonly(!original_permissions.readonly());
        fs::set_permissions(&path, changed_permissions.clone()).unwrap();

        let result = validate_or_retry_buffered_read(
            &first,
            &path,
            initial_stamp,
            &bytes,
            &JobContext::new(),
        );
        fs::set_permissions(&path, original_permissions).unwrap();
        drop(first);
        fs::remove_file(&path).unwrap();

        let stable_stamp = result.unwrap();
        assert_eq!(bytes, expected);
        assert_ne!(stable_stamp, initial_stamp);
        assert!(
            stable_stamp.same_read_extent(initial_stamp),
            "metadata-only retry changed identity or byte length"
        );
        assert_eq!(stable_stamp.readonly, changed_permissions.readonly());
    }

    #[test]
    fn modification_time_transition_retries_identical_buffered_bytes() {
        let number = PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "inkpod-buffered-read-modified-{}-{number}.tga",
            std::process::id()
        ));
        let expected = b"unchanged encoded TGA bytes";
        fs::write(&path, expected).unwrap();

        let mut first = File::open(&path).unwrap();
        let initial_stamp = backend::stamp(&first).unwrap();
        let mut bytes = Vec::new();
        first.read_to_end(&mut bytes).unwrap();
        let shifted_modified =
            fs::metadata(&path).unwrap().modified().unwrap() + Duration::from_secs(3);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(shifted_modified))
            .unwrap();

        let result = validate_or_retry_buffered_read(
            &first,
            &path,
            initial_stamp,
            &bytes,
            &JobContext::new(),
        );
        drop(first);
        fs::remove_file(&path).unwrap();

        let stable_stamp = result.unwrap();
        assert_eq!(bytes, expected);
        assert_ne!(stable_stamp.modified, initial_stamp.modified);
        assert!(stable_stamp.same_read_extent(initial_stamp));
    }

    #[test]
    fn buffered_retry_rejects_same_size_timestamp_preserved_rewrite() {
        let number = PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "inkpod-buffered-read-rewrite-{}-{number}.tga",
            std::process::id()
        ));
        let original = [0x11_u8; 64];
        let replacement = [0x22_u8; 64];
        fs::write(&path, original).unwrap();
        let original_modified = fs::metadata(&path).unwrap().modified().unwrap();
        let original_permissions = fs::metadata(&path).unwrap().permissions();

        let mut first = File::open(&path).unwrap();
        let initial_stamp = backend::stamp(&first).unwrap();
        let mut bytes = Vec::new();
        first.read_to_end(&mut bytes).unwrap();
        fs::write(&path, replacement).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(original_modified))
            .unwrap();
        let mut changed_permissions = original_permissions.clone();
        changed_permissions.set_readonly(!original_permissions.readonly());
        fs::set_permissions(&path, changed_permissions).unwrap();
        let rewritten_stamp = backend::stamp(&first).unwrap();
        assert_ne!(rewritten_stamp, initial_stamp);
        assert!(rewritten_stamp.same_read_extent(initial_stamp));

        let result = validate_or_retry_buffered_read(
            &first,
            &path,
            initial_stamp,
            &bytes,
            &JobContext::new(),
        );
        fs::set_permissions(&path, original_permissions).unwrap();
        drop(first);
        fs::remove_file(&path).unwrap();

        assert!(matches!(result, Err(IoError::ChangedDuringRead)));
        assert_eq!(bytes, original);
    }
}
