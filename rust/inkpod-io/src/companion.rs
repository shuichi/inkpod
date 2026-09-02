use crate::backend;
use crate::file_lock::lock_unpoisoned;
use crate::{IoError, IoManager, IoResult, JobContext};
use inkpod_format::CommonRasterFormat;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(test)]
use std::cell::Cell;

const MAXIMUM_DIRECTORY_ENTRIES: usize = 1_000_000;
const AMBIGUOUS_CANDIDATE_LIMIT: usize = 2;
const MAXIMUM_CACHED_DIRECTORIES: usize = 32;
const MAXIMUM_CACHED_COMPANION_ENTRIES: usize = 20_000;
const MAXIMUM_CACHED_COMPANION_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
struct CompanionDirectoryEntry {
    path: PathBuf,
    normalized_stem: String,
    extension: String,
}

struct CachedCompanionDirectory {
    observer: backend::DirectoryChangeObserver,
    entries: Arc<[CompanionDirectoryEntry]>,
    access: u64,
}

#[derive(Default)]
pub(crate) struct CompanionDirectoryCache {
    directories: BTreeMap<PathBuf, CachedCompanionDirectory>,
    sequence: u64,
}

impl CompanionDirectoryCache {
    pub(crate) fn clear(&mut self) {
        self.directories.clear();
    }

    fn get(&mut self, directory: &Path) -> Option<Arc<[CompanionDirectoryEntry]>> {
        let unchanged = self
            .directories
            .get(directory)?
            .observer
            .unchanged(directory)
            .unwrap_or(false);
        if !unchanged {
            self.directories.remove(directory);
            return None;
        }
        self.sequence = self.sequence.saturating_add(1);
        let access = self.sequence;
        let cached = self.directories.get_mut(directory)?;
        cached.access = access;
        Some(Arc::clone(&cached.entries))
    }

    fn insert(
        &mut self,
        directory: PathBuf,
        observer: backend::DirectoryChangeObserver,
        entries: Vec<CompanionDirectoryEntry>,
    ) {
        self.sequence = self.sequence.saturating_add(1);
        if !self.directories.contains_key(&directory)
            && self.directories.len() >= MAXIMUM_CACHED_DIRECTORIES
            && let Some(victim) = self
                .directories
                .iter()
                .min_by_key(|(_, cached)| cached.access)
                .map(|(path, _)| path.clone())
        {
            self.directories.remove(&victim);
        }
        self.directories.insert(
            directory,
            CachedCompanionDirectory {
                observer,
                entries: entries.into(),
                access: self.sequence,
            },
        );
    }
}

#[cfg(test)]
thread_local! {
    static DIRECTORY_ENUMERATIONS: Cell<usize> = const { Cell::new(0) };
}

impl IoManager {
    /// Finds existing same-directory, same-stem raster candidates for a native
    /// document. Returned paths retain the directory entry's exact spelling.
    /// Two results mean the companion authority is ambiguous; more are not
    /// collected because callers must reject that state.
    pub fn discover_raster_companion_candidates(
        &self,
        native: &Path,
        format: CommonRasterFormat,
        context: &JobContext,
    ) -> IoResult<Vec<PathBuf>> {
        self.check_running(context)?;
        let native = backend::resolve(native)?;
        discover_cached(self, &native, false, Some(format), context)
            .map(|(_, raster_candidates)| raster_candidates)
    }

    /// Finds existing same-directory, same-stem `.inkpod` candidates for a
    /// raster. Extension matching follows backend filename authority, including
    /// ASCII-insensitive extension matching on case-sensitive filesystems.
    /// Two results mean the native authority is ambiguous.
    pub fn discover_native_companion_candidates(
        &self,
        raster: &Path,
        context: &JobContext,
    ) -> IoResult<Vec<PathBuf>> {
        self.check_running(context)?;
        let raster = backend::resolve(raster)?;
        discover_cached(self, &raster, true, None, context)
            .map(|(native_candidates, _)| native_candidates)
    }

    /// Finds the same-stem native and raster candidate sets in one bounded
    /// directory enumeration. The native candidates are returned first. Each
    /// set retains the same two-entry ambiguity bound and exact path spelling as
    /// the corresponding single-purpose discovery method.
    pub fn discover_pair_companion_candidates(
        &self,
        raster: &Path,
        native: &Path,
        format: CommonRasterFormat,
        context: &JobContext,
    ) -> IoResult<(Vec<PathBuf>, Vec<PathBuf>)> {
        self.check_running(context)?;
        let raster = backend::resolve(raster)?;
        let native = backend::resolve(native)?;
        let (parent, raster_stem) = parent_and_normalized_stem(&raster)?;
        let (native_parent, native_stem) = parent_and_normalized_stem(&native)?;
        if native_parent != parent || native_stem != raster_stem {
            return Err(IoError::InvalidInput(
                "companion pair paths do not share a directory and stem",
            ));
        }
        discover_cached(self, &raster, true, Some(format), context)
    }
}

pub(crate) fn raster_candidates_resolved(
    native: &Path,
    format: CommonRasterFormat,
    context: &JobContext,
) -> IoResult<Vec<PathBuf>> {
    discover_resolved(native, context, |extension| {
        CommonRasterFormat::from_extension(extension) == Some(format)
    })
}

pub(crate) fn native_candidates_resolved(
    raster: &Path,
    context: &JobContext,
) -> IoResult<Vec<PathBuf>> {
    discover_resolved(raster, context, |extension| {
        extension.eq_ignore_ascii_case("inkpod")
    })
}

fn discover_cached(
    manager: &IoManager,
    anchor: &Path,
    include_native: bool,
    raster_format: Option<CommonRasterFormat>,
    context: &JobContext,
) -> IoResult<(Vec<PathBuf>, Vec<PathBuf>)> {
    let (parent, stem) = parent_and_normalized_stem(anchor)?;
    if let Some(entries) = lock_unpoisoned(&manager.inner.companion_directories).get(parent) {
        context.check_cancelled()?;
        return Ok(select_candidates(
            &entries,
            &stem,
            include_native,
            raster_format,
        ));
    }

    let mut native_candidates = Vec::new();
    let mut raster_candidates = Vec::new();
    let mut cached_entries = Vec::new();
    let mut cached_entry_bytes = 0_usize;
    let observer = backend::DirectoryChangeObserver::new(parent).ok();
    let mut cacheable = observer.is_some();
    let mut complete = true;
    record_directory_enumeration();
    for (index, entry) in std::fs::read_dir(parent)?.enumerate() {
        context.check_cancelled()?;
        if index >= MAXIMUM_DIRECTORY_ENTRIES {
            return Err(IoError::LimitExceeded(
                "companion directory exceeds its entry limit",
            ));
        }
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(candidate_stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        let normalized_stem = backend::normalized_leaf(candidate_stem);
        let native = extension.eq_ignore_ascii_case("inkpod");
        let raster = CommonRasterFormat::from_extension(extension);
        if cacheable && (native || raster.is_some()) {
            let entry_bytes = path
                .as_os_str()
                .len()
                .checked_add(normalized_stem.len())
                .and_then(|bytes| bytes.checked_add(extension.len()));
            let next_cached_bytes = entry_bytes
                .and_then(|bytes| cached_entry_bytes.checked_add(bytes))
                .filter(|bytes| *bytes <= MAXIMUM_CACHED_COMPANION_BYTES);
            if cached_entries.len() < MAXIMUM_CACHED_COMPANION_ENTRIES
                && let Some(next_cached_bytes) = next_cached_bytes
            {
                cached_entries.push(CompanionDirectoryEntry {
                    path: path.clone(),
                    normalized_stem: normalized_stem.clone(),
                    extension: extension.to_owned(),
                });
                cached_entry_bytes = next_cached_bytes;
            } else {
                cacheable = false;
                cached_entries.clear();
            }
        }
        if normalized_stem != stem {
            continue;
        }
        if include_native && native && native_candidates.len() < AMBIGUOUS_CANDIDATE_LIMIT {
            native_candidates.push(path.clone());
        }
        if raster_format.is_some_and(|format| raster == Some(format))
            && raster_candidates.len() < AMBIGUOUS_CANDIDATE_LIMIT
        {
            raster_candidates.push(path);
        }
        let native_complete =
            !include_native || native_candidates.len() == AMBIGUOUS_CANDIDATE_LIMIT;
        let raster_complete =
            raster_format.is_none() || raster_candidates.len() == AMBIGUOUS_CANDIDATE_LIMIT;
        if native_complete && raster_complete {
            complete = false;
            break;
        }
    }
    native_candidates.sort();
    raster_candidates.sort();

    if complete
        && cacheable
        && let Some(observer) = observer
    {
        if !observer.unchanged(parent)? {
            return Err(IoError::ChangedDuringRead);
        }
        lock_unpoisoned(&manager.inner.companion_directories).insert(
            parent.to_path_buf(),
            observer,
            cached_entries,
        );
    }
    context.check_cancelled()?;
    Ok((native_candidates, raster_candidates))
}

fn select_candidates(
    entries: &[CompanionDirectoryEntry],
    stem: &str,
    include_native: bool,
    raster_format: Option<CommonRasterFormat>,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut native_candidates = Vec::new();
    let mut raster_candidates = Vec::new();
    for entry in entries.iter().filter(|entry| entry.normalized_stem == stem) {
        if include_native
            && entry.extension.eq_ignore_ascii_case("inkpod")
            && native_candidates.len() < AMBIGUOUS_CANDIDATE_LIMIT
        {
            native_candidates.push(entry.path.clone());
        }
        if raster_format.is_some_and(|format| {
            CommonRasterFormat::from_extension(&entry.extension) == Some(format)
        }) && raster_candidates.len() < AMBIGUOUS_CANDIDATE_LIMIT
        {
            raster_candidates.push(entry.path.clone());
        }
    }
    native_candidates.sort();
    raster_candidates.sort();
    (native_candidates, raster_candidates)
}

fn discover_resolved(
    anchor: &Path,
    context: &JobContext,
    accepts_extension: impl Fn(&str) -> bool,
) -> IoResult<Vec<PathBuf>> {
    let (parent, stem) = parent_and_normalized_stem(anchor)?;
    let mut candidates = Vec::new();
    record_directory_enumeration();
    for (index, entry) in std::fs::read_dir(parent)?.enumerate() {
        context.check_cancelled()?;
        if index >= MAXIMUM_DIRECTORY_ENTRIES {
            return Err(IoError::LimitExceeded(
                "companion directory exceeds its entry limit",
            ));
        }
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(candidate_stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if backend::normalized_leaf(candidate_stem) == stem && accepts_extension(extension) {
            candidates.push(path);
            if candidates.len() == AMBIGUOUS_CANDIDATE_LIMIT {
                break;
            }
        }
    }
    candidates.sort();
    Ok(candidates)
}

fn parent_and_normalized_stem(anchor: &Path) -> IoResult<(&Path, String)> {
    let parent = anchor
        .parent()
        .ok_or(IoError::InvalidInput("companion path has no directory"))?;
    let stem = anchor
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or(IoError::InvalidInput(
            "companion path stem is not valid UTF-8",
        ))?;
    Ok((parent, backend::normalized_leaf(stem)))
}

fn record_directory_enumeration() {
    #[cfg(test)]
    DIRECTORY_ENUMERATIONS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
mod tests {
    #[test]
    fn paired_discovery_matches_individual_contracts_with_one_enumeration() {
        use super::*;
        use crate::IoConfig;
        use std::fs;

        let manager = IoManager::new(IoConfig::default()).unwrap();
        let context = JobContext::new();
        let directory = manager
            .create_temporary_directory("companion-paired", &context)
            .unwrap();
        let native = directory.path().join("source.inkpod");
        let raster = directory.path().join("source.TIFF");
        let raster_alias = directory.path().join("source.tif");
        fs::write(&native, b"native").unwrap();
        fs::write(&raster, b"raster").unwrap();
        fs::write(&raster_alias, b"raster alias").unwrap();
        fs::write(directory.path().join("source.png"), b"other format").unwrap();
        fs::write(directory.path().join("other.tiff"), b"other stem").unwrap();

        DIRECTORY_ENUMERATIONS.with(|count| count.set(0));
        let (native_candidates, raster_candidates) = manager
            .discover_pair_companion_candidates(
                &raster,
                &native,
                CommonRasterFormat::Tiff,
                &context,
            )
            .unwrap();
        assert_eq!(
            DIRECTORY_ENUMERATIONS.with(Cell::get),
            1,
            "paired discovery must use one directory enumeration"
        );
        assert_eq!(native_candidates, vec![native.clone()]);
        let mut expected_rasters = vec![raster.clone(), raster_alias.clone()];
        expected_rasters.sort();
        assert_eq!(raster_candidates, expected_rasters);
        assert_eq!(
            manager
                .discover_native_companion_candidates(&raster, &context)
                .unwrap(),
            native_candidates
        );
        assert_eq!(
            manager
                .discover_raster_companion_candidates(&native, CommonRasterFormat::Tiff, &context,)
                .unwrap(),
            raster_candidates
        );

        DIRECTORY_ENUMERATIONS.with(|count| count.set(0));
        assert!(matches!(
            manager.discover_pair_companion_candidates(
                &raster,
                &directory.path().join("other.inkpod"),
                CommonRasterFormat::Tiff,
                &context,
            ),
            Err(IoError::InvalidInput(_))
        ));
        assert_eq!(DIRECTORY_ENUMERATIONS.with(Cell::get), 0);
        drop(directory);
        manager.shutdown_and_wait();
    }

    #[test]
    fn paired_discovery_reuses_unchanged_inventory_and_revalidates_directory_changes() {
        use super::*;
        use crate::IoConfig;
        use std::fs;

        let manager = IoManager::new(IoConfig::default()).unwrap();
        let context = JobContext::new();
        let directory = manager
            .create_temporary_directory("companion-inventory", &context)
            .unwrap();
        if backend::DirectoryChangeObserver::new(directory.path()).is_err() {
            drop(directory);
            manager.shutdown_and_wait();
            return;
        }
        let native = directory.path().join("cell.inkpod");
        let raster = directory.path().join("cell.tiff");
        let raster_alias = directory.path().join("cell.tif");
        fs::write(&native, b"native").unwrap();
        fs::write(&raster, b"raster").unwrap();

        DIRECTORY_ENUMERATIONS.with(|count| count.set(0));
        let discover = || {
            manager
                .discover_pair_companion_candidates(
                    &raster,
                    &native,
                    CommonRasterFormat::Tiff,
                    &context,
                )
                .unwrap()
        };

        let (native_candidates, raster_candidates) = discover();
        assert_eq!(native_candidates, vec![native.clone()]);
        assert_eq!(raster_candidates, vec![raster.clone()]);
        assert_eq!(DIRECTORY_ENUMERATIONS.with(Cell::get), 1);

        for _ in 0..3 {
            assert_eq!(discover(), (vec![native.clone()], vec![raster.clone()]));
        }
        assert_eq!(
            DIRECTORY_ENUMERATIONS.with(Cell::get),
            1,
            "unchanged companion revisits must not enumerate the directory again"
        );

        fs::write(&raster_alias, b"ambiguous raster alias").unwrap();
        let (native_candidates, mut raster_candidates) = discover();
        raster_candidates.sort();
        let mut expected_rasters = vec![raster.clone(), raster_alias.clone()];
        expected_rasters.sort();
        assert_eq!(native_candidates, vec![native.clone()]);
        assert_eq!(raster_candidates, expected_rasters);
        assert_eq!(
            DIRECTORY_ENUMERATIONS.with(Cell::get),
            2,
            "adding a directory entry must invalidate the inventory"
        );

        let renamed_alias = directory.path().join("other.tif");
        fs::rename(&raster_alias, &renamed_alias).unwrap();
        assert_eq!(discover(), (vec![native.clone()], vec![raster.clone()]));
        assert_eq!(
            DIRECTORY_ENUMERATIONS.with(Cell::get),
            3,
            "renaming a directory entry must invalidate the inventory"
        );

        fs::rename(&renamed_alias, &raster_alias).unwrap();
        let (_, mut raster_candidates) = discover();
        raster_candidates.sort();
        assert_eq!(raster_candidates, expected_rasters);
        assert_eq!(
            DIRECTORY_ENUMERATIONS.with(Cell::get),
            4,
            "renaming a candidate back must invalidate the inventory"
        );

        fs::remove_file(&raster_alias).unwrap();
        assert_eq!(discover(), (vec![native.clone()], vec![raster.clone()]));
        assert_eq!(
            DIRECTORY_ENUMERATIONS.with(Cell::get),
            5,
            "removing a directory entry must invalidate the inventory"
        );
        assert_eq!(discover(), (vec![native.clone()], vec![raster.clone()]));
        assert_eq!(
            DIRECTORY_ENUMERATIONS.with(Cell::get),
            5,
            "the revalidated inventory must also be reusable"
        );

        drop(directory);
        manager.shutdown_and_wait();
    }

    #[test]
    fn clear_cache_discards_the_companion_inventory() {
        use super::*;
        use crate::IoConfig;
        use std::fs;

        let manager = IoManager::new(IoConfig::default()).unwrap();
        let context = JobContext::new();
        let directory = manager
            .create_temporary_directory("companion-clear-cache", &context)
            .unwrap();
        if backend::DirectoryChangeObserver::new(directory.path()).is_err() {
            drop(directory);
            manager.shutdown_and_wait();
            return;
        }
        let native = directory.path().join("cell.inkpod");
        let raster = directory.path().join("cell.png");
        fs::write(&native, b"native").unwrap();
        fs::write(&raster, b"raster").unwrap();

        DIRECTORY_ENUMERATIONS.with(|count| count.set(0));
        let discover = || {
            manager
                .discover_pair_companion_candidates(
                    &raster,
                    &native,
                    CommonRasterFormat::Png,
                    &context,
                )
                .unwrap()
        };
        assert_eq!(discover(), (vec![native.clone()], vec![raster.clone()]));
        assert_eq!(discover(), (vec![native.clone()], vec![raster.clone()]));
        assert_eq!(DIRECTORY_ENUMERATIONS.with(Cell::get), 1);

        manager.clear_cache();
        assert_eq!(discover(), (vec![native.clone()], vec![raster.clone()]));
        assert_eq!(
            DIRECTORY_ENUMERATIONS.with(Cell::get),
            2,
            "clear_cache must discard cached companion directory inventories"
        );

        drop(directory);
        manager.shutdown_and_wait();
    }

    #[test]
    fn companion_inventory_cache_evicts_the_least_recently_used_directory() {
        use super::*;
        use crate::{IoConfig, JobContext};
        use std::fs;

        fn insert(cache: &mut CompanionDirectoryCache, path: PathBuf) {
            let observer = backend::DirectoryChangeObserver::new(&path).unwrap();
            cache.insert(path, observer, Vec::new());
        }

        let manager = IoManager::new(IoConfig::default()).unwrap();
        let context = JobContext::new();
        let root = manager
            .create_temporary_directory("companion-lru", &context)
            .unwrap();
        if backend::DirectoryChangeObserver::new(root.path()).is_err() {
            drop(root);
            manager.shutdown_and_wait();
            return;
        }
        let mut paths = Vec::new();
        for index in 0..=MAXIMUM_CACHED_DIRECTORIES {
            let path = root.path().join(format!("directory-{index}"));
            fs::create_dir(&path).unwrap();
            paths.push(path);
        }
        let mut cache = CompanionDirectoryCache::default();
        for path in paths.iter().take(MAXIMUM_CACHED_DIRECTORIES) {
            insert(&mut cache, path.clone());
        }
        assert_eq!(cache.directories.len(), MAXIMUM_CACHED_DIRECTORIES);

        assert!(cache.get(&paths[0]).is_some());
        insert(&mut cache, paths[MAXIMUM_CACHED_DIRECTORIES].clone());
        assert_eq!(cache.directories.len(), MAXIMUM_CACHED_DIRECTORIES);
        assert!(cache.get(&paths[1]).is_none());
        assert!(cache.get(&paths[0]).is_some());
        assert!(cache.get(&paths[MAXIMUM_CACHED_DIRECTORIES]).is_some());
        drop(cache);
        drop(root);
        manager.shutdown_and_wait();
    }

    #[test]
    fn discovery_retains_case_and_reports_ambiguity() {
        use super::*;
        use crate::IoConfig;
        use std::fs;

        let manager = IoManager::new(IoConfig::default()).unwrap();
        let context = JobContext::new();
        let directory = manager
            .create_temporary_directory("companion-case", &context)
            .unwrap();
        let native = directory.path().join("source.inkpod");
        let raster = directory.path().join("source.PNG");
        fs::write(&native, b"native").unwrap();
        fs::write(&raster, b"raster").unwrap();

        assert_eq!(
            raster_candidates_resolved(&native, CommonRasterFormat::Png, &context).unwrap(),
            vec![raster.clone()]
        );
        assert_eq!(
            native_candidates_resolved(&raster, &context).unwrap(),
            vec![native.clone()]
        );

        let second_raster = directory.path().join("source.png");
        let second_native = directory.path().join("source.INKPOD");
        fs::write(&second_raster, b"raster").unwrap();
        fs::write(&second_native, b"native").unwrap();
        let first_identity = manager.resolve_identity(&raster).unwrap().0;
        let second_identity = manager.resolve_identity(&second_raster).unwrap().0;
        if first_identity == second_identity {
            drop(directory);
            manager.shutdown_and_wait();
            return;
        }
        assert_eq!(
            raster_candidates_resolved(&native, CommonRasterFormat::Png, &context)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            native_candidates_resolved(&raster, &context).unwrap().len(),
            2
        );
        drop(directory);
        manager.shutdown_and_wait();
    }
}
