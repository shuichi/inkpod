//! Recoverable publication of a native document and its raster companion.
//! Two independent file replacements are not a filesystem atomic transaction.

mod codec;
#[cfg(test)]
mod tests;

use crate::backend;
use crate::file_lock::lock_unpoisoned;
use crate::{FileStamp, IoError, IoManager, IoResult, JobContext, JobPhase};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub const PAIR_JOURNAL_VERSION: u32 = 1;
const MAX_NATIVE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_JOURNAL_BYTES: u64 = 32 * 1024;
static PAIR_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
struct Proof {
    stamp: FileStamp,
    digest: [u8; 32],
}

#[derive(Clone, Debug)]
struct Member {
    name: String,
    stage: String,
    backup: String,
    original: Option<Proof>,
    replacement: Proof,
    backup_proof: Option<Proof>,
}

#[derive(Clone, Debug)]
struct Record {
    native: Member,
    raster: Member,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairRecovery {
    NotNeeded,
    PreparedDiscarded,
    RolledBack,
    Completed,
}

struct PairOwner {
    manager: IoManager,
    native: PathBuf,
}

impl PairOwner {
    fn acquire(manager: &IoManager, native: &Path) -> IoResult<Arc<Self>> {
        let native = backend::lock_path(native);
        if !lock_unpoisoned(&manager.inner.pair_owners).insert(native.clone()) {
            return Err(IoError::ResourceBusy(
                "another paired save or recovery owns this destination",
            ));
        }
        Ok(Arc::new(Self {
            manager: manager.clone(),
            native,
        }))
    }
}

impl Drop for PairOwner {
    fn drop(&mut self) {
        lock_unpoisoned(&self.manager.inner.pair_owners).remove(&self.native);
    }
}

/// Both output files and old-file backups are durable before this value is
/// returned. Install performs stamp checks and renames under ordered locks.
/// Dropping an uninstalled value discards only its verified private artifacts.
/// A failed/uncertain install retains its journal for `recover_pairs`.
pub struct PreparedPair {
    owner: Arc<PairOwner>,
    parent: PathBuf,
    journal: PathBuf,
    journal_proof: Proof,
    record: Record,
    overwrite: bool,
    discard_on_drop: bool,
}

impl std::fmt::Debug for PreparedPair {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedPair")
            .field("native_bytes", &self.record.native.replacement.stamp.length)
            .field("raster_bytes", &self.record.raster.replacement.stamp.length)
            .finish_non_exhaustive()
    }
}

impl IoManager {
    /// Stages a normal-save pair without changing either destination. Existing
    /// pending journals are never overwritten; open/recovery must resolve them.
    /// Both destinations must be same-stem files in the same physical directory.
    pub fn prepare_pair(
        &self,
        native: &Path,
        raster: &Path,
        context: &JobContext,
        native_writer: impl FnOnce(&mut File) -> IoResult<()>,
        raster_bytes: &[u8],
        overwrite: bool,
    ) -> IoResult<PreparedPair> {
        self.prepare_pair_checked(
            native,
            raster,
            context,
            native_writer,
            raster_bytes,
            overwrite,
            None,
        )
    }

    /// Like `prepare_pair`, with authority captured by a previous successful
    /// open/save. Exact expected stamps permit normal save; differences require
    /// renewed user confirmation instead of silently overwriting external edits.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_pair_checked(
        &self,
        native: &Path,
        raster: &Path,
        context: &JobContext,
        native_writer: impl FnOnce(&mut File) -> IoResult<()>,
        raster_bytes: &[u8],
        overwrite: bool,
        expected: Option<(Option<FileStamp>, Option<FileStamp>)>,
    ) -> IoResult<PreparedPair> {
        self.check_running(context)?;
        if raster_bytes.len() as u64 > self.inner.config.max_file_bytes {
            return Err(IoError::LimitExceeded(
                "paired raster output exceeds its file limit",
            ));
        }
        let native = backend::resolve(native)?;
        let raster = backend::resolve(raster)?;
        validate_targets(&native, &raster)?;
        let parent = native
            .parent()
            .ok_or(IoError::InvalidInput("pair directory is missing"))?
            .to_path_buf();
        let journal = journal_path(&native)?;
        let owner = PairOwner::acquire(self, &native)?;
        let targets = [native.clone(), raster.clone(), journal.clone()];
        let (record, journal_proof) = self.with_file_locks(&targets, context, |_| {
            if journal.try_exists()? {
                return Err(IoError::ResourceBusy(
                    "paired save requires pending-journal recovery",
                ));
            }
            let native_stamp = optional_stamp(&native)?;
            let raster_stamp = optional_stamp(&raster)?;
            if expected.is_some_and(|expected| {
                expected.0 != native_stamp || (raster_stamp.is_some() && expected.1 != raster_stamp)
            }) {
                return Err(IoError::ConfirmationRequired);
            }
            if native_stamp
                .zip(raster_stamp)
                .is_some_and(|(left, right)| left.identity == right.identity)
            {
                return Err(IoError::InvalidInput(
                    "paired outputs alias the same physical file",
                ));
            }
            if !overwrite
                && expected.is_none()
                && (native_stamp.is_some() || raster_stamp.is_some())
            {
                return Err(IoError::ConfirmationRequired);
            }
            if native_stamp.is_some_and(|stamp| stamp.readonly)
                || raster_stamp.is_some_and(|stamp| stamp.readonly)
            {
                return Err(IoError::InvalidInput(
                    "paired save destination is occupied or read-only",
                ));
            }
            let sequence = PAIR_SEQUENCE
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| IoError::LimitExceeded("pair sequence exhausted"))?;
            let token = format!(".inkpod-pair-{}-{sequence}", std::process::id());
            let mut scratch = Scratch::default();
            let native_stage = format!("{token}.native-new");
            let raster_stage = format!("{token}.raster-new");
            let native_backup = format!("{token}.native-old");
            let raster_backup = format!("{token}.raster-old");

            context.set_phase(JobPhase::Writing);
            let mut file = scratch.create(parent.join(&native_stage))?;
            native_writer(&mut file)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            let native_replacement = proof(&parent.join(&native_stage), MAX_NATIVE_BYTES, context)?;
            let mut file = scratch.create(parent.join(&raster_stage))?;
            for chunk in raster_bytes.chunks(64 * 1024) {
                context.check_cancelled()?;
                file.write_all(chunk)?;
            }
            file.flush()?;
            file.sync_all()?;
            drop(file);
            let raster_replacement = proof(
                &parent.join(&raster_stage),
                self.inner.config.max_file_bytes,
                context,
            )?;
            let native_old = backup(
                &native,
                &parent.join(&native_backup),
                native_stamp,
                &mut scratch,
                context,
            )?;
            let raster_old = backup(
                &raster,
                &parent.join(&raster_backup),
                raster_stamp,
                &mut scratch,
                context,
            )?;
            let record = Record {
                native: Member {
                    name: leaf(&native)?,
                    stage: native_stage,
                    backup: native_backup,
                    original: native_old.as_ref().map(|(original, _)| original.clone()),
                    replacement: native_replacement,
                    backup_proof: native_old.map(|(_, copied)| copied),
                },
                raster: Member {
                    name: leaf(&raster)?,
                    stage: raster_stage,
                    backup: raster_backup,
                    original: raster_old.as_ref().map(|(original, _)| original.clone()),
                    replacement: raster_replacement,
                    backup_proof: raster_old.map(|(_, copied)| copied),
                },
            };
            context.check_cancelled()?;
            // A complete bounded journal becomes durable before any final path
            // may change. A torn journal is retained and rejected on recovery.
            let journal_bytes = codec::encode(&record)?;
            let mut file = scratch.create(journal.clone())?;
            file.write_all(&journal_bytes)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            sync_directory(&parent)?;
            let journal_proof = proof(&journal, MAX_JOURNAL_BYTES, context)?;
            scratch.release();
            Ok((record, journal_proof))
        })?;
        Ok(PreparedPair {
            owner,
            parent,
            journal,
            journal_proof,
            record,
            overwrite: overwrite || expected.is_some(),
            discard_on_drop: true,
        })
    }

    /// Resolves an interrupted pair by validating journal, filenames, identities,
    /// and content digests. Unknown/replaced files are a conflict: no target or
    /// evidence is removed. Prepared live jobs in this manager cannot be stolen.
    pub fn recover_pairs(&self, native: &Path, context: &JobContext) -> IoResult<PairRecovery> {
        self.check_running(context)?;
        let native = backend::resolve(native)?;
        // Recovery must retain the same owner as preparation until it has
        // validated and resolved the journal. A check without ownership lets
        // a new prepared job appear between the check and filesystem locks.
        let _owner = PairOwner::acquire(self, &native)?;
        context.set_phase(JobPhase::Reading);
        let journal = journal_path(&native)?;
        if !journal.try_exists()? {
            return Ok(PairRecovery::NotNeeded);
        }
        let parent = native
            .parent()
            .ok_or(IoError::InvalidInput("pair directory is missing"))?;
        let bytes = read_journal(&journal)?;
        let record = codec::decode(&bytes)?;
        if parent.join(&record.native.name) != native {
            return Err(IoError::InvalidInput(
                "paired save journal belongs to a different native file",
            ));
        }
        validate_targets(&native, &parent.join(&record.raster.name))?;
        let journal_proof = proof(&journal, MAX_JOURNAL_BYTES, context)?;
        if journal_proof.digest != *blake3::hash(&bytes).as_bytes() {
            return Err(IoError::ChangedDuringRead);
        }
        let targets = record_paths(parent, &journal, &record);
        reject_symlinks(&targets)?;
        self.with_file_locks(&targets, context, |_| {
            verify_proof(&journal, &journal_proof, context)?;
            recover_record(self, parent, &journal, &record, context)
        })
    }
}

impl PreparedPair {
    /// Call only after the document/session save fence has been acquired. All
    /// expensive encoding, hashing, and backups are already complete. Cancellation
    /// is honored before the first publication; thereafter commit/rollback must
    /// finish, even if cancellation arrives during a rename.
    pub fn install(self, context: &JobContext) -> IoResult<()> {
        self.install_with_stamps(context).map(|_| ())
    }

    /// Returns the installed native/raster stamps captured before releasing the
    /// pair locks, suitable for a subsequent normal-save authority check.
    pub fn install_with_stamps(self, context: &JobContext) -> IoResult<(FileStamp, FileStamp)> {
        self.install_inner(context, false)
    }

    fn install_inner(
        mut self,
        context: &JobContext,
        fail_after_raster: bool,
    ) -> IoResult<(FileStamp, FileStamp)> {
        let manager = self.owner.manager.clone();
        let targets = record_paths(&self.parent, &self.journal, &self.record);
        manager.with_file_locks(&targets, context, |_| {
            if optional_stamp(&self.journal)? != Some(self.journal_proof.stamp) {
                return Err(IoError::ChangedDuringRead);
            }
            for member in [&self.record.native, &self.record.raster] {
                if optional_stamp(&self.parent.join(&member.name))?
                    != member.original.as_ref().map(|value| value.stamp)
                    || optional_stamp(&self.parent.join(&member.stage))?
                        != Some(member.replacement.stamp)
                    || optional_stamp(&self.parent.join(&member.backup))?
                        != member.backup_proof.as_ref().map(|value| value.stamp)
                {
                    return Err(IoError::ChangedDuringRead);
                }
            }
            context.check_cancelled()?;
            self.discard_on_drop = false;
            context.set_phase(JobPhase::Installing);
            let result = (|| {
                backend::replace(
                    &self.parent.join(&self.record.raster.stage),
                    &self.parent.join(&self.record.raster.name),
                    self.overwrite,
                )?;
                if fail_after_raster {
                    return Err(IoError::InvalidInput("injected pair installation failure"));
                }
                backend::replace(
                    &self.parent.join(&self.record.native.stage),
                    &self.parent.join(&self.record.native.name),
                    self.overwrite,
                )?;
                let native_stamp =
                    backend::stamp(&File::open(self.parent.join(&self.record.native.name))?)?;
                let raster_stamp =
                    backend::stamp(&File::open(self.parent.join(&self.record.raster.name))?)?;
                invalidate_record(&manager, &self.record);
                // Final files are committed. Cleanup errors leave the valid
                // journal so reopen can finish removing only verified artifacts.
                cleanup_with_stamps(&self.parent, &self.journal, &self.record)?;
                Ok((native_stamp, raster_stamp))
            })();
            match result {
                Ok(stamps) => Ok(stamps),
                Err(error) => {
                    // Recover uses hashes only on this exceptional path. A failed
                    // rollback retains evidence and reports its specific conflict.
                    recover_record(
                        &manager,
                        &self.parent,
                        &self.journal,
                        &self.record,
                        &JobContext::new(),
                    )?;
                    Err(error)
                }
            }
        })
    }
}

impl Drop for PreparedPair {
    fn drop(&mut self) {
        if self.discard_on_drop {
            let manager = self.owner.manager.clone();
            let cleanup_manager = manager.clone();
            let owner = Arc::clone(&self.owner);
            let targets = record_paths(&self.parent, &self.journal, &self.record);
            let parent = self.parent.clone();
            let journal = self.journal.clone();
            let journal_proof = self.journal_proof.clone();
            let record = self.record.clone();
            // Drop can run while UI polling releases a stale/cancelled candidate.
            // Queue rejection/shutdown retains the durable journal for recovery.
            let _ = manager.enqueue_cleanup(move || {
                let _owner = owner;
                let context = JobContext::new();
                let _ = cleanup_manager.with_file_locks(&targets, &context, |_| {
                    if optional_stamp(&journal)? != Some(journal_proof.stamp) {
                        return Err(IoError::ChangedDuringRead);
                    }
                    cleanup_with_stamps(&parent, &journal, &record)
                });
            });
        }
    }
}

#[derive(Default)]
struct Scratch {
    created: Vec<PathBuf>,
}

impl Scratch {
    fn create(&mut self, path: PathBuf) -> IoResult<File> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.created.push(path);
        Ok(file)
    }
    fn release(&mut self) {
        self.created.clear();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for path in self.created.iter().rev() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn validate_targets(native: &Path, raster: &Path) -> IoResult<()> {
    let same_stem = native
        .file_stem()
        .and_then(|stem| stem.to_str())
        .zip(raster.file_stem().and_then(|stem| stem.to_str()))
        .is_some_and(|(native, raster)| {
            backend::normalized_leaf(native) == backend::normalized_leaf(raster)
        });
    if native == raster
        || native.parent() != raster.parent()
        || !same_stem
        || !native
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("inkpod"))
        || raster
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(inkpod_format::CommonRasterFormat::from_extension)
            .is_none()
    {
        return Err(IoError::InvalidInput(
            "paired outputs must be same-stem native and raster files in one directory",
        ));
    }
    Ok(())
}

fn leaf(path: &Path) -> IoResult<String> {
    let value = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(IoError::InvalidInput("paired file name is not valid UTF-8"))?;
    validate_leaf(value)?;
    Ok(value.to_owned())
}

fn validate_leaf(value: &str) -> IoResult<()> {
    let mut components = Path::new(value).components();
    if value.is_empty()
        || value.len() > 4096
        || value
            .bytes()
            .any(|byte| matches!(byte, 0 | b'/' | b'\\' | b':'))
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || value.ends_with(['.', ' '])
    {
        return Err(IoError::InvalidInput("paired journal file name is unsafe"));
    }
    Ok(())
}

fn journal_path(native: &Path) -> IoResult<PathBuf> {
    let name = leaf(native)?;
    // Windows case aliases must address one journal even before a target exists.
    let key = backend::normalized_leaf(&name);
    let digest = blake3::hash(key.as_bytes());
    let token: String = digest.as_bytes()[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(native.with_file_name(format!(".inkpod-pair-{token}.journal")))
}

fn optional_stamp(path: &Path) -> IoResult<Option<FileStamp>> {
    match File::open(path) {
        Ok(file) => Ok(Some(backend::stamp(&file)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn proof(path: &Path, maximum: u64, context: &JobContext) -> IoResult<Proof> {
    let mut file = File::open(path)?;
    let stamp = backend::stamp(&file)?;
    if stamp.length > maximum {
        return Err(IoError::LimitExceeded(
            "paired save artifact exceeds its byte bound",
        ));
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        context.check_cancelled()?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .filter(|total| *total <= maximum)
            .ok_or(IoError::LimitExceeded(
                "paired artifact grew beyond its byte bound",
            ))?;
        hasher.update(&buffer[..read]);
    }
    if total != stamp.length || backend::stamp(&file)? != stamp {
        return Err(IoError::ChangedDuringRead);
    }
    Ok(Proof {
        stamp,
        digest: *hasher.finalize().as_bytes(),
    })
}

fn backup(
    source: &Path,
    destination: &Path,
    original: Option<FileStamp>,
    scratch: &mut Scratch,
    context: &JobContext,
) -> IoResult<Option<(Proof, Proof)>> {
    let Some(original) = original else {
        return Ok(None);
    };
    if original.length > MAX_NATIVE_BYTES {
        return Err(IoError::LimitExceeded(
            "paired backup exceeds its byte limit",
        ));
    }
    let mut input = File::open(source)?;
    if backend::stamp(&input)? != original {
        return Err(IoError::ChangedDuringRead);
    }
    let mut output = scratch.create(destination.to_path_buf())?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        context.check_cancelled()?;
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .filter(|total| *total <= MAX_NATIVE_BYTES)
            .ok_or(IoError::LimitExceeded(
                "paired backup grew beyond its byte limit",
            ))?;
        output.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
    }
    if total != original.length || backend::stamp(&input)? != original {
        return Err(IoError::ChangedDuringRead);
    }
    output.flush()?;
    output.sync_all()?;
    let copied = backend::stamp(&output)?;
    let digest = *hasher.finalize().as_bytes();
    Ok(Some((
        Proof {
            stamp: original,
            digest,
        },
        Proof {
            stamp: copied,
            digest,
        },
    )))
}

fn read_journal(path: &Path) -> IoResult<Vec<u8>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_JOURNAL_BYTES {
        return Err(IoError::LimitExceeded("paired journal is too large"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_JOURNAL_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(IoError::LimitExceeded(
            "paired journal grew beyond its bound",
        ));
    }
    Ok(bytes)
}

fn record_paths(parent: &Path, journal: &Path, record: &Record) -> Vec<PathBuf> {
    let mut paths = vec![journal.to_path_buf()];
    for member in [&record.native, &record.raster] {
        paths.extend([
            parent.join(&member.name),
            parent.join(&member.stage),
            parent.join(&member.backup),
        ]);
    }
    paths
}

fn reject_symlinks(paths: &[PathBuf]) -> IoResult<()> {
    for path in paths {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(IoError::InvalidInput(
                    "paired recovery target is a symbolic link",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn verify_proof(path: &Path, expected: &Proof, context: &JobContext) -> IoResult<()> {
    let actual = proof(path, expected.stamp.length.max(1), context)?;
    if actual.stamp.identity != expected.stamp.identity
        || actual.stamp.length != expected.stamp.length
        || actual.digest != expected.digest
    {
        return Err(IoError::InvalidInput(
            "paired recovery artifact identity or digest changed",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Current {
    Original,
    Replacement,
}

fn classify(parent: &Path, member: &Member, context: &JobContext) -> IoResult<Current> {
    let path = parent.join(&member.name);
    let Some(current) = optional_stamp(&path)? else {
        return if member.original.is_none() {
            Ok(Current::Original)
        } else {
            Err(IoError::InvalidInput("paired recovery target is missing"))
        };
    };
    if current.identity == member.replacement.stamp.identity {
        verify_proof(&path, &member.replacement, context)?;
        return Ok(Current::Replacement);
    }
    for original in [member.original.as_ref(), member.backup_proof.as_ref()]
        .into_iter()
        .flatten()
    {
        if current.identity == original.stamp.identity {
            verify_proof(&path, original, context)?;
            return Ok(Current::Original);
        }
    }
    Err(IoError::InvalidInput(
        "paired recovery target was replaced externally",
    ))
}

fn recover_record(
    manager: &IoManager,
    parent: &Path,
    journal: &Path,
    record: &Record,
    context: &JobContext,
) -> IoResult<PairRecovery> {
    let native = classify(parent, &record.native, context)?;
    let raster = classify(parent, &record.raster, context)?;
    if native == Current::Replacement && raster == Current::Replacement {
        cleanup(parent, journal, record, context)?;
        invalidate_record(manager, record);
        return Ok(PairRecovery::Completed);
    }
    if native == Current::Original && raster == Current::Original {
        cleanup(parent, journal, record, context)?;
        return Ok(PairRecovery::PreparedDiscarded);
    }
    // Verify every required backup before changing either final path.
    for (member, state) in [(&record.native, native), (&record.raster, raster)] {
        if state == Current::Replacement {
            if let Some(backup) = &member.backup_proof {
                verify_proof(&parent.join(&member.backup), backup, context)?;
            }
        }
    }
    context.check_cancelled()?;
    // From here cancellation cannot strand a half-restored pair.
    for (member, state) in [(&record.native, native), (&record.raster, raster)] {
        if state != Current::Replacement {
            continue;
        }
        if member.original.is_some() {
            backend::replace(
                &parent.join(&member.backup),
                &parent.join(&member.name),
                true,
            )?;
        } else {
            std::fs::remove_file(parent.join(&member.name))?;
        }
    }
    invalidate_record(manager, record);
    cleanup(parent, journal, record, &JobContext::new())?;
    Ok(PairRecovery::RolledBack)
}

fn cleanup(parent: &Path, journal: &Path, record: &Record, context: &JobContext) -> IoResult<()> {
    let mut verified = Vec::new();
    for member in [&record.native, &record.raster] {
        for (name, expected) in [
            (&member.stage, Some(&member.replacement)),
            (&member.backup, member.backup_proof.as_ref()),
        ] {
            let path = parent.join(name);
            if !path.try_exists()? {
                continue;
            }
            let expected =
                expected.ok_or(IoError::InvalidInput("unexpected paired recovery artifact"))?;
            verify_proof(&path, expected, context)?;
            verified.push(path);
        }
    }
    context.check_cancelled()?;
    for path in verified {
        std::fs::remove_file(path)?;
    }
    std::fs::remove_file(journal)?;
    sync_directory(parent)
}

fn cleanup_with_stamps(parent: &Path, journal: &Path, record: &Record) -> IoResult<()> {
    let mut verified = Vec::new();
    for member in [&record.native, &record.raster] {
        for (name, expected) in [
            (&member.stage, Some(&member.replacement)),
            (&member.backup, member.backup_proof.as_ref()),
        ] {
            let path = parent.join(name);
            let Some(actual) = optional_stamp(&path)? else {
                continue;
            };
            if Some(actual) != expected.map(|proof| proof.stamp) {
                return Err(IoError::ChangedDuringRead);
            }
            verified.push(path);
        }
    }
    for path in verified {
        std::fs::remove_file(path)?;
    }
    std::fs::remove_file(journal)?;
    sync_directory(parent)
}

fn invalidate_record(manager: &IoManager, record: &Record) {
    for member in [&record.native, &record.raster] {
        manager
            .inner
            .cache
            .invalidate(member.replacement.stamp.identity);
        if let Some(original) = &member.original {
            manager.inner.cache.invalidate(original.stamp.identity);
        }
        if let Some(backup) = &member.backup_proof {
            manager.inner.cache.invalidate(backup.stamp.identity);
        }
    }
}

fn sync_directory(path: &Path) -> IoResult<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
