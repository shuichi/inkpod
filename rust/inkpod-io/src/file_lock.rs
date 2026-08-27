use crate::{FileIdentity, IoError, IoResult, JobContext};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, TryLockError, Weak};
use std::time::Duration;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum LockKey {
    Path(PathBuf),
    Identity(FileIdentity),
}

#[derive(Default)]
pub(crate) struct FileLocks {
    entries: Mutex<BTreeMap<LockKey, Weak<Mutex<()>>>>,
}

impl FileLocks {
    pub(crate) fn acquire_owner(&self, key: LockKey) -> Arc<Mutex<()>> {
        let mut entries = lock_unpoisoned(&self.entries);
        if let Some(owner) = entries.get(&key).and_then(Weak::upgrade) {
            return owner;
        }
        // Dead lock records do not accumulate with a long image browsing session.
        entries.retain(|_, value| value.strong_count() != 0);
        let owner = Arc::new(Mutex::new(()));
        entries.insert(key, Arc::downgrade(&owner));
        owner
    }
}

pub(crate) fn lock_cancel<'a>(
    lock: &'a Mutex<()>,
    context: &JobContext,
) -> IoResult<MutexGuard<'a, ()>> {
    loop {
        context.check_cancelled()?;
        match lock.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::Poisoned(_)) => return Err(IoError::WorkerPanicked),
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
}

pub(crate) fn lock_unpoisoned<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
