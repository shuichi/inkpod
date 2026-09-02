use super::SavedPair;
use crate::{Core, CoreError};
use inkpod_io::FileStamp;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// Default application-wide budget for replayed and validated sidecar targets.
pub const DEFAULT_VALIDATED_TARGET_CACHE_BYTES: u64 = 1024 * 1024 * 1024;
/// Hard maximum application-wide budget for replayed and validated sidecar targets.
pub const MAX_VALIDATED_TARGET_CACHE_BYTES: u64 = 1024 * 1024 * 1024;
/// Hard maximum number of replayed and validated sidecar targets.
pub const MAX_VALIDATED_TARGETS: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// Read-only application-wide validated-target cache counters.
pub struct ValidatedTargetCacheStats {
    /// Configured byte budget. Zero disables the cache.
    pub maximum_bytes: u64,
    /// Conservative logical weight of entries retained by the cache.
    pub retained_bytes: u64,
    /// Number of retained targets, at most [`MAX_VALIDATED_TARGETS`].
    pub target_count: u64,
    /// Exact complete-stamp and path matches returned to callers.
    pub hits: u64,
    /// Lookups that could not return an exact target.
    pub misses: u64,
    /// Entries removed by replacement, capacity pressure, or limit changes.
    pub evictions: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedTargetKey {
    pub(crate) native_path: PathBuf,
    pub(crate) native: FileStamp,
    pub(crate) raster_path: PathBuf,
    pub(crate) raster: FileStamp,
}

struct Entry {
    key: ValidatedTargetKey,
    target: Core,
    weight: u64,
}

struct State {
    maximum_bytes: u64,
    retained_bytes: u64,
    entries: VecDeque<Entry>,
    hits: u64,
    misses: u64,
    evictions: u64,
}

/// Application-wide bounded LRU of fully replayed, pair-validated sidecar targets.
///
/// Entries contain detached clean [`Core`] values. A hit clones only COW graph and
/// tile ownership; immutable asset and tile payloads remain shared. Lookup keys use
/// both normalized pair paths and both complete file stamps. Cache publication does
/// not replace the resolver's final namespace and TOCTOU validation.
#[derive(Clone)]
pub struct ValidatedTargetCache {
    state: Arc<Mutex<State>>,
}

impl Default for ValidatedTargetCache {
    fn default() -> Self {
        Self::new(DEFAULT_VALIDATED_TARGET_CACHE_BYTES)
            .expect("the built-in validated-target cache limit is valid")
    }
}

impl ValidatedTargetCache {
    /// Creates an empty cache with a byte limit in `0..=1 GiB`.
    pub fn new(maximum_bytes: u64) -> Result<Self, CoreError> {
        if maximum_bytes > MAX_VALIDATED_TARGET_CACHE_BYTES {
            return Err(CoreError::InvalidArgument(
                "validated target cache exceeds 1 GiB",
            ));
        }
        Ok(Self {
            state: Arc::new(Mutex::new(State {
                maximum_bytes,
                retained_bytes: 0,
                entries: VecDeque::new(),
                hits: 0,
                misses: 0,
                evictions: 0,
            })),
        })
    }

    /// Changes the byte limit and immediately removes least-recently-used entries.
    ///
    /// Zero disables and empties the cache. Invalid limits leave all state unchanged.
    pub fn set_maximum_bytes(&self, maximum_bytes: u64) -> Result<(), CoreError> {
        if maximum_bytes > MAX_VALIDATED_TARGET_CACHE_BYTES {
            return Err(CoreError::InvalidArgument(
                "validated target cache exceeds 1 GiB",
            ));
        }
        let mut state = lock(&self.state);
        state.maximum_bytes = maximum_bytes;
        trim(&mut state);
        Ok(())
    }

    /// Returns current limits, logical retention, and semantic hit counters.
    #[must_use]
    pub fn stats(&self) -> ValidatedTargetCacheStats {
        let state = lock(&self.state);
        ValidatedTargetCacheStats {
            maximum_bytes: state.maximum_bytes,
            retained_bytes: state.retained_bytes,
            target_count: state.entries.len() as u64,
            hits: state.hits,
            misses: state.misses,
            evictions: state.evictions,
        }
    }

    pub(crate) fn lookup(&self, key: &ValidatedTargetKey) -> Option<Core> {
        let mut state = lock(&self.state);
        if state.maximum_bytes == 0 {
            state.misses = state.misses.saturating_add(1);
            return None;
        }
        invalidate_changed_pair(&mut state, key);
        let Some(index) = state.entries.iter().position(|entry| entry.key == *key) else {
            state.misses = state.misses.saturating_add(1);
            return None;
        };
        let entry = state
            .entries
            .remove(index)
            .expect("a located validated-target entry exists");
        let target = entry.target.clone();
        state.entries.push_front(entry);
        state.hits = state.hits.saturating_add(1);
        Some(target)
    }

    pub(crate) fn insert(&self, key: ValidatedTargetKey, target: &Core, native_bytes: u64) {
        let mut state = lock(&self.state);
        invalidate_changed_pair(&mut state, &key);
        remove_matching(&mut state, |entry| entry.key == key);
        if state.maximum_bytes == 0 || target.document_info().is_err() {
            return;
        }
        let usage = target.resource_usage();
        let asset_bytes = target.asset_store_usage().logical_payload_bytes;
        let weight = native_bytes
            .saturating_add(asset_bytes)
            .saturating_add(usage.document_tile_bytes)
            .saturating_add(usage.history_bytes)
            .saturating_add(usage.render_cache_bytes)
            .saturating_add(usage.cpu_staging_bytes)
            .saturating_add(usage.reference_light_table_bytes)
            .saturating_add(usage.sequence_source_bytes)
            .saturating_add(usage.thumbnail_cache_bytes)
            .max(1);
        if weight > state.maximum_bytes || !cacheable_target(target, &key) {
            return;
        }
        while state.entries.len() >= MAX_VALIDATED_TARGETS
            || state.retained_bytes > state.maximum_bytes - weight
        {
            evict_lru(&mut state);
        }
        state.retained_bytes = state.retained_bytes.saturating_add(weight);
        state.entries.push_front(Entry {
            key,
            target: target.clone(),
            weight,
        });
    }

    pub(crate) fn invalidate_pair_paths(&self, native_path: &Path, raster_path: &Path) {
        let mut state = lock(&self.state);
        remove_matching(&mut state, |entry| {
            entry.key.native_path == native_path && entry.key.raster_path == raster_path
        });
    }
}

fn cacheable_target(target: &Core, key: &ValidatedTargetKey) -> bool {
    target.document_info().is_ok_and(|document| !document.dirty)
        && !target.recovered
        && !target.io_install_pending
        && target.io_pair_plan.is_none()
        && target.io_pair_authority.as_ref().is_some_and(
            |SavedPair {
                 native_path,
                 native,
                 raster_path,
                 raster,
                 raster_missing,
             }| {
                native_path == &key.native_path
                    && *native == key.native
                    && raster_path == &key.raster_path
                    && *raster == Some(key.raster)
                    && raster_missing.is_none()
            },
        )
}

fn same_pair_path(left: &ValidatedTargetKey, right: &ValidatedTargetKey) -> bool {
    left.native_path == right.native_path && left.raster_path == right.raster_path
}

fn invalidate_changed_pair(state: &mut State, key: &ValidatedTargetKey) {
    remove_matching(state, |entry| {
        same_pair_path(&entry.key, key) && entry.key != *key
    });
}

fn remove_matching(state: &mut State, mut predicate: impl FnMut(&Entry) -> bool) {
    let mut index = 0;
    while index < state.entries.len() {
        if predicate(&state.entries[index]) {
            let entry = state
                .entries
                .remove(index)
                .expect("a selected validated-target entry exists");
            state.retained_bytes = state.retained_bytes.saturating_sub(entry.weight);
            state.evictions = state.evictions.saturating_add(1);
        } else {
            index += 1;
        }
    }
}

fn trim(state: &mut State) {
    while state.entries.len() > MAX_VALIDATED_TARGETS || state.retained_bytes > state.maximum_bytes
    {
        evict_lru(state);
    }
}

fn evict_lru(state: &mut State) {
    if let Some(entry) = state.entries.pop_back() {
        state.retained_bytes = state.retained_bytes.saturating_sub(entry.weight);
        state.evictions = state.evictions.saturating_add(1);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkpod_io::FileIdentity;

    fn stamp(index: u64) -> FileStamp {
        FileStamp {
            identity: FileIdentity {
                volume: 7,
                file: u128::from(index + 1),
            },
            length: 1,
            modified: i128::from(index),
            changed: i128::from(index),
            readonly: false,
        }
    }

    fn target(index: u64) -> (ValidatedTargetKey, Core) {
        let native_path = PathBuf::from(format!("frame-{index}.inkpod"));
        let raster_path = PathBuf::from(format!("frame-{index}.tga"));
        let native = stamp(index * 2);
        let raster = stamp(index * 2 + 1);
        let key = ValidatedTargetKey {
            native_path: native_path.clone(),
            native,
            raster_path: raster_path.clone(),
            raster,
        };
        let mut target = Core::new();
        target.new_cell(1, 1, 96_000, 96_000).unwrap();
        target.io_pair_authority = Some(SavedPair {
            native_path,
            native,
            raster_path,
            raster: Some(raster),
            raster_missing: None,
        });
        (key, target)
    }

    #[test]
    fn limit_is_bounded_disable_clears_and_invalid_update_is_atomic() {
        assert!(ValidatedTargetCache::new(MAX_VALIDATED_TARGET_CACHE_BYTES + 1).is_err());
        let cache = ValidatedTargetCache::default();
        let (key, target) = target(0);
        cache.insert(key, &target, 2);
        assert_eq!(cache.stats().target_count, 1);
        assert!(
            cache
                .set_maximum_bytes(MAX_VALIDATED_TARGET_CACHE_BYTES + 1)
                .is_err()
        );
        assert_eq!(
            cache.stats().maximum_bytes,
            DEFAULT_VALIDATED_TARGET_CACHE_BYTES
        );
        cache.set_maximum_bytes(1).unwrap();
        let stats = cache.stats();
        assert_eq!(stats.maximum_bytes, 1);
        assert_eq!(stats.target_count, 0);
        assert_eq!(stats.retained_bytes, 0);
        assert_eq!(stats.evictions, 1);
        cache.set_maximum_bytes(0).unwrap();
        let stats = cache.stats();
        assert_eq!(stats.maximum_bytes, 0);
        assert_eq!(stats.target_count, 0);
        assert_eq!(stats.retained_bytes, 0);
        assert_eq!(stats.evictions, 1);
    }

    #[test]
    fn lru_never_retains_more_than_sixty_four_exact_targets() {
        let cache = ValidatedTargetCache::new(MAX_VALIDATED_TARGET_CACHE_BYTES).unwrap();
        let mut keys = Vec::new();
        for index in 0..=MAX_VALIDATED_TARGETS as u64 {
            let (key, target) = target(index);
            cache.insert(key.clone(), &target, 1);
            keys.push(key);
        }
        let stats = cache.stats();
        assert_eq!(stats.target_count, MAX_VALIDATED_TARGETS as u64);
        assert_eq!(stats.evictions, 1);
        assert!(cache.lookup(&keys[0]).is_none());
        assert!(cache.lookup(keys.last().unwrap()).is_some());
        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn changed_stamp_invalidates_the_same_pair_path() {
        let cache = ValidatedTargetCache::default();
        let (key, target) = target(0);
        cache.insert(key.clone(), &target, 1);
        let mut changed = key;
        changed.raster.modified += 1;
        assert!(cache.lookup(&changed).is_none());
        let stats = cache.stats();
        assert_eq!(stats.target_count, 0);
        assert_eq!(stats.evictions, 1);
        assert_eq!(stats.misses, 1);
    }
}
