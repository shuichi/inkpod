use crate::file_lock::lock_unpoisoned;
use crate::image::{ByteLease, ImageLease};
use crate::{FileIdentity, FileStamp, IoConfig, IoError, IoResult};
use inkpod_format::CommonRasterFormat;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Resident counters include consumer-pinned values and in-flight reservations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheStats {
    pub images: u64,
    pub encoded_bytes: u64,
    pub decoded_bytes: u64,
    pub cached_images: u64,
    pub physical_reads: u64,
    pub decodes: u64,
    pub cache_hits: u64,
    pub evictions: u64,
}

#[derive(Default)]
pub(crate) struct Counters {
    pub(crate) images: AtomicU64,
    pub(crate) encoded: AtomicU64,
    pub(crate) decoded: AtomicU64,
    pub(crate) reads: AtomicU64,
    pub(crate) decodes: AtomicU64,
    pub(crate) hits: AtomicU64,
    pub(crate) evictions: AtomicU64,
}

#[derive(Clone, Copy)]
pub(crate) enum BudgetKind {
    Image,
    Encoded,
    Decoded,
}

pub(crate) struct Reservation {
    counters: Arc<Counters>,
    kind: BudgetKind,
    amount: u64,
}

impl Reservation {
    pub(crate) fn reduce_to(&mut self, amount: u64) {
        if amount < self.amount {
            self.counter()
                .fetch_sub(self.amount - amount, Ordering::AcqRel);
            self.amount = amount;
        }
    }

    fn counter(&self) -> &AtomicU64 {
        match self.kind {
            BudgetKind::Image => &self.counters.images,
            BudgetKind::Encoded => &self.counters.encoded,
            BudgetKind::Decoded => &self.counters.decoded,
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.counter().fetch_sub(self.amount, Ordering::AcqRel);
    }
}

pub(crate) struct CacheEntry {
    path: PathBuf,
    pub(crate) stamp: FileStamp,
    pub(crate) generation: u64,
    pub(crate) bytes: ByteLease,
    pub(crate) decoded: Option<(CommonRasterFormat, ImageLease)>,
    access: u64,
}

#[derive(Default)]
struct CacheState {
    entries: BTreeMap<FileIdentity, CacheEntry>,
    sequence: u64,
    generation: u64,
}

pub(crate) struct ImageCache {
    state: Mutex<CacheState>,
    pub(crate) counters: Arc<Counters>,
    config: IoConfig,
}

impl ImageCache {
    pub(crate) fn new(config: IoConfig) -> Self {
        Self {
            state: Mutex::new(CacheState::default()),
            counters: Arc::new(Counters::default()),
            config,
        }
    }

    pub(crate) fn bytes(&self, stamp: FileStamp) -> Option<(ByteLease, u64)> {
        let mut state = lock_unpoisoned(&self.state);
        if state
            .entries
            .get(&stamp.identity)
            .is_some_and(|entry| entry.stamp != stamp)
        {
            state.entries.remove(&stamp.identity);
        }
        state.sequence = state.sequence.saturating_add(1);
        let access = state.sequence;
        let entry = state.entries.get_mut(&stamp.identity)?;
        entry.access = access;
        self.counters.hits.fetch_add(1, Ordering::Relaxed);
        Some((entry.bytes.clone(), entry.generation))
    }

    pub(crate) fn decoded(
        &self,
        stamp: FileStamp,
        format: CommonRasterFormat,
    ) -> Option<ImageLease> {
        let mut state = lock_unpoisoned(&self.state);
        state.sequence = state.sequence.saturating_add(1);
        let access = state.sequence;
        let entry = state.entries.get_mut(&stamp.identity)?;
        if entry.stamp != stamp {
            return None;
        }
        let (cached_format, raster) = entry.decoded.as_ref()?;
        if *cached_format != format {
            return None;
        }
        entry.access = access;
        Some(raster.clone())
    }

    pub(crate) fn reserve(&self, kind: BudgetKind, amount: u64) -> IoResult<Reservation> {
        let (counter, limit) = match kind {
            BudgetKind::Image => (&self.counters.images, self.config.max_images as u64),
            BudgetKind::Encoded => (&self.counters.encoded, self.config.max_encoded_bytes),
            BudgetKind::Decoded => (&self.counters.decoded, self.config.max_decoded_bytes),
        };
        if amount > limit {
            return Err(IoError::LimitExceeded(
                "image allocation exceeds its cache budget",
            ));
        }
        let mut state = lock_unpoisoned(&self.state);
        while counter.load(Ordering::Acquire) > limit - amount {
            let victim = state
                .entries
                .iter()
                .filter(|(_, entry)| {
                    entry.bytes.unpinned()
                        && entry
                            .decoded
                            .as_ref()
                            .is_none_or(|(_, image)| image.unpinned())
                })
                .min_by_key(|(_, entry)| entry.access)
                .map(|(identity, _)| *identity);
            let Some(victim) = victim else {
                return Err(IoError::ResourceBusy(
                    "image cache is pinned or its reservation budget is full",
                ));
            };
            state.entries.remove(&victim);
            self.counters.evictions.fetch_add(1, Ordering::Relaxed);
        }
        counter.fetch_add(amount, Ordering::AcqRel);
        Ok(Reservation {
            counters: Arc::clone(&self.counters),
            kind,
            amount,
        })
    }

    pub(crate) fn insert_bytes(
        &self,
        path: PathBuf,
        stamp: FileStamp,
        bytes: ByteLease,
    ) -> IoResult<u64> {
        let mut state = lock_unpoisoned(&self.state);
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or(IoError::LimitExceeded("file cache generation is exhausted"))?;
        state.sequence = state.sequence.saturating_add(1);
        let generation = state.generation;
        let access = state.sequence;
        state.entries.insert(
            stamp.identity,
            CacheEntry {
                path,
                stamp,
                generation,
                bytes,
                decoded: None,
                access,
            },
        );
        Ok(generation)
    }

    pub(crate) fn insert_decoded(
        &self,
        stamp: FileStamp,
        format: CommonRasterFormat,
        image: ImageLease,
    ) {
        let mut state = lock_unpoisoned(&self.state);
        if let Some(entry) = state
            .entries
            .get_mut(&stamp.identity)
            .filter(|entry| entry.stamp == stamp)
        {
            entry.decoded = Some((format, image));
        }
    }

    pub(crate) fn invalidate(&self, identity: FileIdentity) {
        lock_unpoisoned(&self.state).entries.remove(&identity);
    }

    pub(crate) fn clear(&self) {
        lock_unpoisoned(&self.state).entries.clear();
    }

    pub(crate) fn invalidate_under(&self, path: &Path) {
        lock_unpoisoned(&self.state)
            .entries
            .retain(|_, entry| !entry.path.starts_with(path));
    }

    pub(crate) fn stats(&self) -> CacheStats {
        CacheStats {
            images: self.counters.images.load(Ordering::Acquire),
            encoded_bytes: self.counters.encoded.load(Ordering::Acquire),
            decoded_bytes: self.counters.decoded.load(Ordering::Acquire),
            cached_images: lock_unpoisoned(&self.state).entries.len() as u64,
            physical_reads: self.counters.reads.load(Ordering::Acquire),
            decodes: self.counters.decodes.load(Ordering::Acquire),
            cache_hits: self.counters.hits.load(Ordering::Acquire),
            evictions: self.counters.evictions.load(Ordering::Acquire),
        }
    }
}
