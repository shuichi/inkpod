use crate::file_lock::lock_unpoisoned;
use crate::job::JobControl;
use crate::{IoError, IoResult, JobContext};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::{self, JoinHandle};

// Returning true yields a continuation after one independently bounded unit.
pub(crate) type Work = Box<dyn FnMut() -> bool + Send + 'static>;

struct QueueState {
    work: VecDeque<Work>,
    stopped: bool,
    controls: Vec<Weak<JobControl>>,
}

struct Queue {
    state: Mutex<QueueState>,
    changed: Condvar,
    capacity: usize,
}

pub(crate) struct Executor {
    queue: Arc<Queue>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl Executor {
    pub(crate) fn new(worker_count: usize, capacity: usize) -> IoResult<Self> {
        let executor = Self {
            queue: Arc::new(Queue {
                state: Mutex::new(QueueState {
                    work: VecDeque::new(),
                    stopped: false,
                    controls: Vec::new(),
                }),
                changed: Condvar::new(),
                capacity,
            }),
            workers: Mutex::new(Vec::new()),
        };
        for index in 0..worker_count {
            let queue = Arc::clone(&executor.queue);
            let worker = thread::Builder::new()
                .name(format!("inkpod-io-{index}"))
                .spawn(move || worker_loop(&queue))?;
            lock_unpoisoned(&executor.workers).push(worker);
        }
        Ok(executor)
    }

    pub(crate) fn enqueue(&self, work: Vec<Work>, context: &JobContext) -> IoResult<()> {
        let mut state = lock_unpoisoned(&self.queue.state);
        if state.stopped {
            return Err(IoError::Shutdown);
        }
        if work.len() > self.queue.capacity.saturating_sub(state.work.len()) {
            return Err(IoError::ResourceBusy("file I/O queue is full"));
        }
        state.controls.retain(|control| control.strong_count() != 0);
        state.controls.push(Arc::downgrade(&context.control));
        state.work.extend(work);
        self.queue.changed.notify_all();
        Ok(())
    }

    pub(crate) fn shutdown(&self) {
        let mut state = lock_unpoisoned(&self.queue.state);
        state.stopped = true;
        for control in state.controls.iter().filter_map(Weak::upgrade) {
            JobContext { control }.cancel();
        }
        self.queue.changed.notify_all();
    }

    pub(crate) fn is_shutdown(&self) -> bool {
        lock_unpoisoned(&self.queue.state).stopped
    }

    pub(crate) fn shutdown_and_wait(&self) {
        self.shutdown();
        let workers: Vec<_> = lock_unpoisoned(&self.workers).drain(..).collect();
        let current = thread::current().id();
        for worker in workers {
            if worker.thread().id() != current {
                let _ = worker.join();
            }
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.shutdown();
        // A result can release the final owner during UI polling, or on a pool
        // worker itself. Drop never joins. Explicit shutdown_and_wait provides
        // the blocking engine-thread drain; workers own their queue until exit.
        lock_unpoisoned(&self.workers).clear();
    }
}

fn worker_loop(queue: &Queue) {
    let mut continuation = None;
    loop {
        let mut work = if let Some(work) = continuation.take() {
            work
        } else {
            let mut state = lock_unpoisoned(&queue.state);
            loop {
                if let Some(work) = state.work.pop_front() {
                    break work;
                }
                if state.stopped {
                    return;
                }
                state = queue
                    .changed
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        // Each task contains its own unwind boundary and publishes failure.
        // This boundary also protects the fixed worker population from bugs in
        // a completion wrapper, without ever unwinding into a foreign caller.
        if matches!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut work)),
            Ok(true)
        ) {
            let mut state = lock_unpoisoned(&queue.state);
            // Swap with accepted work instead of submitting from a worker. This
            // preserves the queue bound even when it is full and never waits for
            // queue capacity while all workers own image-batch continuations.
            continuation = Some(if let Some(next) = state.work.pop_front() {
                state.work.push_back(work);
                next
            } else {
                work
            });
        }
    }
}
