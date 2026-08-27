use crate::file_lock::lock_unpoisoned;
use crate::{IoError, IoResult, LoadedImage};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum JobState {
    #[default]
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum JobPhase {
    #[default]
    Queued,
    Enumerating,
    Reading,
    Decoding,
    Writing,
    Installing,
    Finished,
}

/// A coherent, pointer-free polling snapshot. `loaded` counts successfully
/// decoded images, never intermediate work steps; `completed` includes failures.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JobProgress {
    pub state: JobState,
    pub phase: JobPhase,
    pub discovered: u64,
    pub queued: u64,
    pub reading: u64,
    pub read_completed: u64,
    pub loaded: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub completed: u64,
    pub total: u64,
    pub completed_bytes: u64,
}

pub(crate) struct JobControl {
    cancelled: Arc<AtomicBool>,
    progress: Mutex<JobProgress>,
}

/// Cancellation and progress shared by a job's independent file operations.
/// This value owns no document or frontend pointer and can cross worker threads.
#[derive(Clone)]
pub struct JobContext {
    pub(crate) control: Arc<JobControl>,
}

impl Default for JobContext {
    fn default() -> Self {
        Self::new()
    }
}

impl JobContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            control: Arc::new(JobControl {
                cancelled: Arc::new(AtomicBool::new(false)),
                progress: Mutex::new(JobProgress::default()),
            }),
        }
    }

    /// Shares cancellation with this operation, with independent progress for
    /// internal work such as preview output re-reading. Cancelling either context
    /// cancels both; dropping a context alone never requests cancellation.
    #[must_use]
    pub fn child(&self) -> Self {
        Self {
            control: Arc::new(JobControl {
                cancelled: self.control.cancelled.clone(),
                progress: Mutex::new(JobProgress::default()),
            }),
        }
    }

    pub fn cancel(&self) {
        self.control.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.control.cancelled.load(Ordering::Acquire)
    }

    pub fn check_cancelled(&self) -> IoResult<()> {
        if self.is_cancelled() {
            Err(IoError::Cancelled)
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn progress(&self) -> JobProgress {
        *lock_unpoisoned(&self.control.progress)
    }

    pub fn set_phase(&self, phase: JobPhase) {
        self.update(|progress| progress.phase = phase);
    }

    pub fn set_work(&self, completed: u64, total: u64) {
        self.update(|progress| {
            progress.completed = completed.min(total);
            progress.total = total;
        });
    }

    pub(crate) fn update(&self, action: impl FnOnce(&mut JobProgress)) {
        action(&mut lock_unpoisoned(&self.control.progress));
    }

    pub fn record_read_completed(&self) {
        self.update(|progress| progress.read_completed = progress.read_completed.saturating_add(1));
    }

    pub fn record_loaded(&self) {
        self.update(|progress| progress.loaded = progress.loaded.saturating_add(1));
    }

    pub fn set_counts(&self, discovered: u64, queued: u64, reading: u64) {
        self.update(|progress| {
            progress.discovered = discovered;
            progress.queued = queued;
            progress.reading = reading;
        });
    }
}

pub(crate) struct JobOutput<T> {
    pub(crate) result: Mutex<Option<IoResult<T>>>,
}

/// A one-shot job. Poll and cancel do not wait for file I/O; the result can be
/// taken exactly once. Dropping this owner requests cancellation, never joins.
pub struct IoJob<T> {
    pub(crate) context: JobContext,
    pub(crate) output: Arc<JobOutput<T>>,
}

impl<T> IoJob<T> {
    #[must_use]
    pub fn poll(&self) -> JobProgress {
        self.context.progress()
    }

    pub fn cancel(&self) {
        self.context.cancel();
    }

    pub fn try_take(&self) -> Option<IoResult<T>> {
        lock_unpoisoned(&self.output.result).take()
    }

    #[must_use]
    pub fn context(&self) -> JobContext {
        self.context.clone()
    }
}

impl<T> Drop for IoJob<T> {
    fn drop(&mut self) {
        self.context.cancel();
    }
}

pub struct ImageBatchItem {
    pub index: usize,
    pub path: PathBuf,
    pub result: IoResult<LoadedImage>,
}

pub(crate) struct BatchOutput {
    pub(crate) results: Mutex<Vec<Option<ImageBatchItem>>>,
}

/// Results retain discovery order even when disk and codec work finish out of
/// order. Taking results releases the batch's own pixel/cache leases.
pub struct ImageBatch {
    pub(crate) context: JobContext,
    pub(crate) output: Arc<BatchOutput>,
}

impl ImageBatch {
    #[must_use]
    pub fn poll(&self) -> JobProgress {
        self.context.progress()
    }

    pub fn cancel(&self) {
        self.context.cancel();
    }

    pub fn take_completed(&self, maximum: usize) -> Vec<ImageBatchItem> {
        let mut output = lock_unpoisoned(&self.output.results);
        output
            .iter_mut()
            .filter_map(Option::take)
            .take(maximum)
            .collect()
    }

    pub fn take(&self, index: usize) -> Option<ImageBatchItem> {
        lock_unpoisoned(&self.output.results)
            .get_mut(index)
            .and_then(Option::take)
    }

    #[must_use]
    pub fn context(&self) -> JobContext {
        self.context.clone()
    }
}

impl Drop for ImageBatch {
    fn drop(&mut self) {
        self.context.cancel();
    }
}
