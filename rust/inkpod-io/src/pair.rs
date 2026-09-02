//! Recoverable publication of a native document and its raster companion.
//! Two independent file replacements are not a filesystem atomic transaction.

mod codec;
#[cfg(test)]
mod tests;

use crate::backend;
use crate::companion::{native_candidates_resolved, raster_candidates_resolved};
use crate::file_lock::lock_unpoisoned;
use crate::{FileIdentity, FileStamp, IoError, IoManager, IoResult, JobContext, JobPhase};
use inkpod_format::CommonRasterFormat;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub const PAIR_JOURNAL_VERSION: u32 = 2;
const MAX_NATIVE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_JOURNAL_BYTES: u64 = 32 * 1024;
static PAIR_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
struct Proof {
    stamp: FileStamp,
    digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Member {
    name: String,
    stage: String,
    backup: String,
    original: Option<Proof>,
    replacement: Proof,
    backup_proof: Option<Proof>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Record {
    committed: bool,
    native: Member,
    raster: Member,
}

#[derive(Clone, Debug)]
struct AliasProof {
    native_candidates: Vec<PathBuf>,
    raster_candidates: Vec<PathBuf>,
    raster_format: CommonRasterFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairRecovery {
    NotNeeded,
    PreparedDiscarded,
    RolledBack,
    Completed,
}

/// Verified final member stamps after a failed live installation restored the
/// pre-install bytes. `None` means that member was absent before preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestoredPair {
    pub native: Option<FileStamp>,
    pub raster: Option<FileStamp>,
    pub native_missing: Option<FileIdentity>,
    pub raster_missing: Option<FileIdentity>,
}

/// Durable result of a live pair installation. A rolled-back outcome carries
/// the original operation error separately from optional authority-repair
/// stamps. Stamps are withheld when the directory alias proof no longer holds.
#[derive(Debug)]
pub enum PairInstallOutcome {
    Installed {
        native: FileStamp,
        raster: FileStamp,
    },
    RolledBack {
        error: IoError,
        restored: Option<RestoredPair>,
    },
    FailedAfterPublication {
        error: IoError,
    },
}

/// Deterministic pair-publication faults available only to crate tests and
/// dependants that explicitly enable the non-default `test-support` feature.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairInstallFault {
    /// Fail after the native member is durably published but before the raster
    /// member is installed, forcing the normal pair rollback path.
    AfterNativePublication,
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
    commit_stage_proof: Proof,
    rollback_stage_proof: Proof,
    record: Record,
    alias_proof: AliasProof,
    discard_on_drop: bool,
}

#[derive(Clone, Copy)]
enum PairExpectation {
    None,
    Committed(Option<FileStamp>, Option<FileStamp>),
    Planned {
        native_missing: crate::FileIdentity,
        raster: FileStamp,
    },
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
        self.prepare_pair_with_expectation(
            native,
            raster,
            context,
            native_writer,
            raster_bytes,
            overwrite,
            PairExpectation::None,
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
        let expected = expected.map_or(PairExpectation::None, |(native, raster)| {
            PairExpectation::Committed(native, raster)
        });
        self.prepare_pair_with_expectation(
            native,
            raster,
            context,
            native_writer,
            raster_bytes,
            overwrite,
            expected,
        )
    }

    /// Stages the first save of a raster-derived pair using its open-time proof.
    ///
    /// Unlike committed normal-save authority, this requires the selected
    /// raster to retain its exact stamp and the native destination to retain its
    /// exact missing-path identity. Either member changing requires renewed
    /// confirmation and no output is staged.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_planned_pair_checked(
        &self,
        native: &Path,
        raster: &Path,
        context: &JobContext,
        native_writer: impl FnOnce(&mut File) -> IoResult<()>,
        raster_bytes: &[u8],
        expected_native_missing: crate::FileIdentity,
        expected_raster: FileStamp,
    ) -> IoResult<PreparedPair> {
        self.prepare_pair_with_expectation(
            native,
            raster,
            context,
            native_writer,
            raster_bytes,
            false,
            PairExpectation::Planned {
                native_missing: expected_native_missing,
                raster: expected_raster,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_pair_with_expectation(
        &self,
        native: &Path,
        raster: &Path,
        context: &JobContext,
        native_writer: impl FnOnce(&mut File) -> IoResult<()>,
        raster_bytes: &[u8],
        overwrite: bool,
        expected: PairExpectation,
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
        let raster_format = raster
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(CommonRasterFormat::from_extension)
            .ok_or(IoError::InvalidInput(
                "paired raster destination format is unsupported",
            ))?;
        let parent = native
            .parent()
            .ok_or(IoError::InvalidInput("pair directory is missing"))?
            .to_path_buf();
        let journal = journal_path(&native)?;
        let commit = commit_path(&journal);
        let commit_stage = commit_stage_path(&journal);
        let rollback = rollback_path(&journal);
        let rollback_stage = rollback_stage_path(&journal);
        let owner = PairOwner::acquire(self, &native)?;
        // A previous durable publication may have left only its bounded
        // evidence cleanup unfinished. Resolve that record while retaining the
        // same pair owner, so a successful Save never strands this live
        // destination until the document is reopened.
        recover_pairs_owned(self, &native, context)?;
        let targets = [
            native.clone(),
            raster.clone(),
            journal.clone(),
            commit,
            commit_stage.clone(),
            rollback,
            rollback_stage.clone(),
        ];
        let (record, journal_proof, commit_stage_proof, rollback_stage_proof, alias_proof) =
            self.with_file_locks(&targets, context, |_| {
                if journal.try_exists()?
                    || commit_path(&journal).try_exists()?
                    || commit_stage.try_exists()?
                    || rollback_path(&journal).try_exists()?
                    || rollback_stage.try_exists()?
                {
                    return Err(IoError::ResourceBusy(
                        "paired save requires pending-journal recovery",
                    ));
                }
                let native_stamp = optional_stamp(&native)?;
                let raster_stamp = optional_stamp(&raster)?;
                let alias_proof = capture_alias_proof(
                    &native,
                    &raster,
                    raster_format,
                    native_stamp,
                    raster_stamp,
                    context,
                )?;
                match expected {
                    PairExpectation::Committed(expected_native, expected_raster) => {
                        if expected_native != native_stamp || expected_raster != raster_stamp {
                            return Err(IoError::ConfirmationRequired);
                        }
                    }
                    PairExpectation::Planned {
                        native_missing,
                        raster,
                    } => {
                        if native_stamp.is_some()
                            || backend::missing_identity(&native) != native_missing
                            || raster_stamp != Some(raster)
                        {
                            return Err(IoError::ConfirmationRequired);
                        }
                    }
                    PairExpectation::None => {}
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
                    && matches!(expected, PairExpectation::None)
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
                let journal_stage = format!("{token}.journal-new");

                context.set_phase(JobPhase::Writing);
                let mut file = scratch.create(parent.join(&native_stage))?;
                native_writer(&mut file)?;
                file.flush()?;
                file.sync_all()?;
                drop(file);
                let native_replacement =
                    proof(&parent.join(&native_stage), MAX_NATIVE_BYTES, context)?;
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
                    committed: false,
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
                // may change. The committed-phase payload is also staged and
                // flushed now, but is not authoritative until its final atomic
                // publication after both members and all final fences verify.
                let journal_bytes = codec::encode(&record)?;
                let committed_record = committed_record(&record);
                let commit_bytes = codec::encode(&committed_record)?;
                // The prepared journal is rollback authority. Publish it from
                // a token-private, fully flushed file so Windows also crosses
                // a WRITE_THROUGH rename boundary before either final member
                // can change. Only then create the deterministic commit stage:
                // a crash can leave a recoverable journal without that stage,
                // never an unowned deterministic stage without its journal.
                let journal_stage = parent.join(journal_stage);
                let mut file = scratch.create(journal_stage.clone())?;
                file.write_all(&journal_bytes)?;
                file.flush()?;
                file.sync_all()?;
                drop(file);
                scratch.publish(&journal_stage, &journal)?;
                let mut file = scratch.create(commit_stage.clone())?;
                file.write_all(&commit_bytes)?;
                file.flush()?;
                file.sync_all()?;
                drop(file);
                let mut file = scratch.create(rollback_stage.clone())?;
                file.write_all(&journal_bytes)?;
                file.flush()?;
                file.sync_all()?;
                drop(file);
                let journal_proof = proof(&journal, MAX_JOURNAL_BYTES, context)?;
                let commit_stage_proof = proof(&commit_stage, MAX_JOURNAL_BYTES, context)?;
                let rollback_stage_proof = proof(&rollback_stage, MAX_JOURNAL_BYTES, context)?;
                verify_alias_proof(&native, &raster, &alias_proof, context)?;
                scratch.release();
                Ok((
                    record,
                    journal_proof,
                    commit_stage_proof,
                    rollback_stage_proof,
                    alias_proof,
                ))
            })?;
        Ok(PreparedPair {
            owner,
            parent,
            journal,
            journal_proof,
            commit_stage_proof,
            rollback_stage_proof,
            record,
            alias_proof,
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
        recover_pairs_owned(self, &native, context)
    }
}

/// Resolves one normalized pair while the caller retains its `PairOwner`.
/// Keeping ownership across recovery and a following prepare closes the gap in
/// which another in-process pair job could publish a fresh journal.
fn recover_pairs_owned(
    manager: &IoManager,
    native: &Path,
    context: &JobContext,
) -> IoResult<PairRecovery> {
    context.set_phase(JobPhase::Reading);
    let journal = journal_path(native)?;
    let parent = native
        .parent()
        .ok_or(IoError::InvalidInput("pair directory is missing"))?;
    if !journal.try_exists()? {
        let commit = commit_path(&journal);
        let rollback = rollback_path(&journal);
        let commit_exists = commit.try_exists()?;
        let rollback_exists = rollback.try_exists()?;
        if commit_exists {
            // Committed cleanup removes the prepared journal before the
            // marker. A crash in that interval remains recoverable because
            // the marker contains the complete checksummed record.
            let bytes = read_journal(&commit)?;
            let mut record = codec::decode(&bytes)?;
            if !record.committed {
                return Err(IoError::InvalidInput(
                    "orphan paired commit marker has an invalid phase",
                ));
            }
            let commit_proof = proof(&commit, MAX_JOURNAL_BYTES, context)?;
            if commit_proof.digest != *blake3::hash(&bytes).as_bytes() {
                return Err(IoError::ChangedDuringRead);
            }
            record.committed = false;
            if parent.join(&record.native.name) != native {
                return Err(IoError::InvalidInput(
                    "paired commit marker belongs to a different native file",
                ));
            }
            validate_targets(native, &parent.join(&record.raster.name))?;
            let targets = record_paths(parent, &journal, &record);
            reject_symlinks(&targets)?;
            return manager.with_file_locks(&targets, context, |_| {
                verify_proof(&commit, &commit_proof, context)?;
                recover_record(manager, parent, &journal, &record, context)
            });
        }
        if rollback_exists {
            let bytes = read_journal(&rollback)?;
            let record = codec::decode(&bytes)?;
            if record.committed {
                return Err(IoError::InvalidInput(
                    "orphan paired rollback marker has an invalid phase",
                ));
            }
            let rollback_proof = proof(&rollback, MAX_JOURNAL_BYTES, context)?;
            if rollback_proof.digest != *blake3::hash(&bytes).as_bytes() {
                return Err(IoError::ChangedDuringRead);
            }
            if parent.join(&record.native.name) != native {
                return Err(IoError::InvalidInput(
                    "paired rollback marker belongs to a different native file",
                ));
            }
            validate_targets(native, &parent.join(&record.raster.name))?;
            let targets = record_paths(parent, &journal, &record);
            reject_symlinks(&targets)?;
            return manager.with_file_locks(&targets, context, |_| {
                verify_proof(&rollback, &rollback_proof, context)?;
                recover_record(manager, parent, &journal, &record, context)
            });
        }
        if commit_stage_path(&journal).try_exists()?
            || rollback_stage_path(&journal).try_exists()?
        {
            return Err(IoError::InvalidInput(
                "paired commit evidence exists without its prepared journal",
            ));
        }
        return Ok(PairRecovery::NotNeeded);
    }
    let bytes = read_journal(&journal)?;
    let record = codec::decode(&bytes)?;
    if record.committed {
        return Err(IoError::InvalidInput(
            "paired prepared journal has an invalid commit phase",
        ));
    }
    if parent.join(&record.native.name) != native {
        return Err(IoError::InvalidInput(
            "paired save journal belongs to a different native file",
        ));
    }
    validate_targets(native, &parent.join(&record.raster.name))?;
    let journal_proof = proof(&journal, MAX_JOURNAL_BYTES, context)?;
    if journal_proof.digest != *blake3::hash(&bytes).as_bytes() {
        return Err(IoError::ChangedDuringRead);
    }
    let targets = record_paths(parent, &journal, &record);
    reject_symlinks(&targets)?;
    manager.with_file_locks(&targets, context, |_| {
        verify_proof(&journal, &journal_proof, context)?;
        recover_record(manager, parent, &journal, &record, context)
    })
}

impl PreparedPair {
    /// Complete physical stamps of the staged native/raster replacements.
    /// Successful same-volume installation preserves their identities; callers
    /// may reserve those future final identities before authorizing publication.
    #[must_use]
    pub fn replacement_stamps(&self) -> (FileStamp, FileStamp) {
        (
            self.record.native.replacement.stamp,
            self.record.raster.replacement.stamp,
        )
    }

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
        match self.install_with_outcome(context)? {
            PairInstallOutcome::Installed { native, raster } => Ok((native, raster)),
            PairInstallOutcome::RolledBack { error, .. }
            | PairInstallOutcome::FailedAfterPublication { error } => Err(error),
        }
    }

    /// Installs the pair while retaining verified rollback stamps for the live
    /// Core's runtime authority repair. A returned `RolledBack` is still an
    /// operation failure; it only proves that the pre-install bytes are stable
    /// at the returned current identities.
    pub fn install_with_outcome(self, context: &JobContext) -> IoResult<PairInstallOutcome> {
        self.install_outcome_inner(context, false, None, false, false, None)
    }

    /// Installs through the normal publication and rollback implementation,
    /// injecting one deterministic fault at the selected semantic boundary.
    /// This API is absent from default production builds.
    #[cfg(any(test, feature = "test-support"))]
    pub fn install_with_fault_outcome(
        self,
        context: &JobContext,
        fault: PairInstallFault,
    ) -> IoResult<PairInstallOutcome> {
        let fail_after_native = matches!(fault, PairInstallFault::AfterNativePublication);
        self.install_outcome_inner(context, fail_after_native, None, false, false, None)
    }

    #[cfg(test)]
    fn install_inner(
        self,
        context: &JobContext,
        fail_after_native: bool,
        alias_after_both_members: Option<&Path>,
        fail_final_proof: bool,
        fail_cleanup: bool,
    ) -> IoResult<(FileStamp, FileStamp)> {
        match self.install_outcome_inner(
            context,
            fail_after_native,
            alias_after_both_members,
            fail_final_proof,
            fail_cleanup,
            None,
        )? {
            PairInstallOutcome::Installed { native, raster } => Ok((native, raster)),
            PairInstallOutcome::RolledBack { error, .. }
            | PairInstallOutcome::FailedAfterPublication { error } => Err(error),
        }
    }

    fn install_outcome_inner(
        mut self,
        context: &JobContext,
        fail_after_native: bool,
        alias_after_both_members: Option<&Path>,
        fail_final_proof: bool,
        fail_cleanup: bool,
        external_after_native_delete: Option<&[u8]>,
    ) -> IoResult<PairInstallOutcome> {
        let manager = self.owner.manager.clone();
        let targets = record_paths(&self.parent, &self.journal, &self.record);
        manager.with_file_locks(&targets, context, |_| {
            if optional_stamp(&self.journal)? != Some(self.journal_proof.stamp) {
                return Err(IoError::ChangedDuringRead);
            }
            if optional_stamp(&commit_path(&self.journal))?.is_some()
                || optional_stamp(&commit_stage_path(&self.journal))?
                    != Some(self.commit_stage_proof.stamp)
                || optional_stamp(&rollback_path(&self.journal))?.is_some()
                || optional_stamp(&rollback_stage_path(&self.journal))?
                    != Some(self.rollback_stage_proof.stamp)
            {
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
            verify_alias_proof(
                &self.parent.join(&self.record.native.name),
                &self.parent.join(&self.record.raster.name),
                &self.alias_proof,
                context,
            )?;
            context.check_cancelled()?;
            self.discard_on_drop = false;
            context.set_phase(JobPhase::Installing);
            let mut publication_started = false;
            let result = (|| {
                // A durable rollback marker authorizes the temporary missing
                // state created by exact-delete + no-overwrite publication. It
                // precedes either final mutation, so a crash or external race
                // can never make recovery infer that an external deletion was
                // ours without evidence.
                publish_rollback_marker(
                    &self.journal,
                    &self.record,
                    Some(&self.rollback_stage_proof),
                )?;
                publication_started = true;
                install_member_exact(
                    &self.parent,
                    &self.record.native,
                    &JobContext::new(),
                    external_after_native_delete,
                )?;
                verify_half_installed_alias_invariant(
                    &self.parent.join(&self.record.native.name),
                    &self.parent.join(&self.record.raster.name),
                    self.alias_proof.raster_format,
                    true,
                    self.record.raster.original.is_some(),
                )?;
                if fail_after_native {
                    return Err(IoError::InvalidInput("injected pair installation failure"));
                }
                install_member_exact(&self.parent, &self.record.raster, &JobContext::new(), None)?;
                #[cfg(test)]
                if let Some(alias) = alias_after_both_members {
                    std::fs::write(alias, b"injected late companion alias")?;
                }
                #[cfg(not(test))]
                let _ = alias_after_both_members;
                #[cfg(test)]
                if fail_final_proof {
                    return Err(IoError::ChangedDuringRead);
                }
                #[cfg(not(test))]
                let _ = fail_final_proof;
                let final_context = JobContext::new();
                let native_stamp = installed_replacement_stamp(
                    &self.parent.join(&self.record.native.name),
                    &self.record.native.replacement,
                    &final_context,
                )?;
                let raster_stamp = installed_replacement_stamp(
                    &self.parent.join(&self.record.raster.name),
                    &self.record.raster.replacement,
                    &final_context,
                )?;
                verify_installed_alias_invariant(
                    &self.parent.join(&self.record.native.name),
                    &self.parent.join(&self.record.raster.name),
                    self.alias_proof.raster_format,
                )?;
                // The pair becomes publishable only after both installed files,
                // the directory-wide alias invariant, and directory durability
                // have been fenced. Every error before this point is rolled back.
                sync_directory(&self.parent)?;
                // Publish a separately durable committed-phase marker only after
                // every fallible pair/content/alias fence. The prepared journal
                // remains in place: recovery therefore rolls both replacements
                // back if a crash occurs before this single atomic publication,
                // and completes them only if the marker exists and matches it.
                publish_commit_marker(&self.journal, &self.commit_stage_proof)?;
                invalidate_record(&manager, &self.record);
                // Crossing the durable committed-phase publication commits the
                // disk transaction. No fallible save operation follows it.
                // Cleanup is evidence disposal, not part of the Core savepoint
                // commit. A failure retains the journal/artifacts for recovery
                // and must not turn a durable pair into an application failure.
                #[cfg(test)]
                let cleanup_result = if fail_cleanup {
                    Err(IoError::InvalidInput("injected pair cleanup failure"))
                } else {
                    cleanup_with_stamps(
                        &self.parent,
                        &self.journal,
                        &self.record,
                        &self.journal_proof,
                        &self.commit_stage_proof,
                        &self.rollback_stage_proof,
                    )
                };
                #[cfg(not(test))]
                let cleanup_result = {
                    let _ = fail_cleanup;
                    cleanup_with_stamps(
                        &self.parent,
                        &self.journal,
                        &self.record,
                        &self.journal_proof,
                        &self.commit_stage_proof,
                        &self.rollback_stage_proof,
                    )
                };
                let _ = cleanup_result;
                Ok((native_stamp, raster_stamp))
            })();
            match result {
                Ok((native, raster)) => Ok(PairInstallOutcome::Installed { native, raster }),
                Err(error) => {
                    if !publication_started {
                        recover_record(
                            &manager,
                            &self.parent,
                            &self.journal,
                            &self.record,
                            &JobContext::new(),
                        )?;
                        return Err(error);
                    }

                    // Publication may have changed one or both identities. A
                    // failed live install therefore uses an explicit rollback
                    // even when both finals look like replacements (which crash
                    // recovery otherwise classifies as completed). The restored
                    // bytes are reread under the pair locks before their current
                    // complete stamps may repair runtime authority.
                    let restored = match rollback_install_record(
                        &manager,
                        &self.parent,
                        &self.journal,
                        &self.record,
                        &self.journal_proof,
                        &self.commit_stage_proof,
                        &self.rollback_stage_proof,
                    ) {
                        Ok(restored) => restored,
                        Err(error) => {
                            return Ok(PairInstallOutcome::FailedAfterPublication { error });
                        }
                    };
                    let restored = verify_alias_proof(
                        &self.parent.join(&self.record.native.name),
                        &self.parent.join(&self.record.raster.name),
                        &self.alias_proof,
                        &JobContext::new(),
                    )
                    .is_ok()
                    .then_some(restored);
                    Ok(PairInstallOutcome::RolledBack { error, restored })
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
            let commit_stage_proof = self.commit_stage_proof.clone();
            let rollback_stage_proof = self.rollback_stage_proof.clone();
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
                    cleanup_with_stamps(
                        &parent,
                        &journal,
                        &record,
                        &journal_proof,
                        &commit_stage_proof,
                        &rollback_stage_proof,
                    )
                });
            });
        }
    }
}

#[derive(Default)]
struct Scratch {
    created: Vec<ScratchArtifact>,
}

struct ScratchArtifact {
    path: PathBuf,
    identity: FileIdentity,
    // An open Unix file description keeps an unlinked inode alive, preventing
    // its identity from being recycled for an external replacement while this
    // artifact is still tracked. Windows exact cleanup instead needs to open
    // the path with exclusive sharing, so it retains the existing path proof.
    #[cfg(unix)]
    _identity_guard: File,
}

impl Scratch {
    fn create(&mut self, path: PathBuf) -> IoResult<File> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        // The file may be rewritten after creation, so its complete stamp is
        // expected to change. Its physical identity is stable across those
        // writes and our same-volume publications, and is the authority used
        // by Drop to avoid deleting a later occupant of this pathname.
        let created_stamp = backend::stamp(&file)?;
        #[cfg(unix)]
        let identity_guard = match file.try_clone() {
            Ok(guard) => guard,
            Err(error) => {
                let _ = backend::remove_exact(&path, created_stamp);
                return Err(error.into());
            }
        };
        self.created.push(ScratchArtifact {
            path,
            identity: created_stamp.identity,
            #[cfg(unix)]
            _identity_guard: identity_guard,
        });
        Ok(file)
    }
    fn publish(&mut self, source: &Path, destination: &Path) -> IoResult<()> {
        let tracked = self
            .created
            .iter_mut()
            .find(|artifact| artifact.path == source)
            .ok_or(IoError::InvalidInput(
                "paired scratch publication source is not owned",
            ))?;
        if optional_stamp(source)?.map(|stamp| stamp.identity) != Some(tracked.identity) {
            return Err(IoError::ChangedDuringRead);
        }
        backend::replace(source, destination, false)?;
        tracked.path = destination.to_path_buf();
        if optional_stamp(destination)?.map(|stamp| stamp.identity) != Some(tracked.identity) {
            return Err(IoError::ChangedDuringRead);
        }
        Ok(())
    }
    fn release(&mut self) {
        self.created.clear();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for artifact in self.created.iter().rev() {
            let Ok(Some(actual)) = optional_stamp(&artifact.path) else {
                continue;
            };
            if actual.identity == artifact.identity {
                let _ = backend::remove_exact(&artifact.path, actual);
            }
        }
    }
}

fn capture_alias_proof(
    native: &Path,
    raster: &Path,
    raster_format: CommonRasterFormat,
    native_stamp: Option<FileStamp>,
    raster_stamp: Option<FileStamp>,
    context: &JobContext,
) -> IoResult<AliasProof> {
    let proof = AliasProof {
        native_candidates: native_candidates_resolved(raster, context)?,
        raster_candidates: raster_candidates_resolved(native, raster_format, context)?,
        raster_format,
    };
    if !candidate_set_names_only_member(&proof.native_candidates, native, native_stamp.is_some())
        || !candidate_set_names_only_member(
            &proof.raster_candidates,
            raster,
            raster_stamp.is_some(),
        )
    {
        return Err(IoError::ConfirmationRequired);
    }
    Ok(proof)
}

fn verify_alias_proof(
    native: &Path,
    raster: &Path,
    expected: &AliasProof,
    context: &JobContext,
) -> IoResult<()> {
    let native_candidates = native_candidates_resolved(raster, context)?;
    let raster_candidates = raster_candidates_resolved(native, expected.raster_format, context)?;
    if native_candidates != expected.native_candidates
        || raster_candidates != expected.raster_candidates
    {
        return Err(IoError::ConfirmationRequired);
    }
    Ok(())
}

fn verify_half_installed_alias_invariant(
    native: &Path,
    raster: &Path,
    raster_format: CommonRasterFormat,
    native_exists: bool,
    raster_exists: bool,
) -> IoResult<()> {
    // Publication has begun, so cancellation no longer interrupts validation or
    // rollback. A fresh context provides an uncancelled final directory fence.
    let context = JobContext::new();
    let native_candidates = native_candidates_resolved(raster, &context)?;
    let raster_candidates = raster_candidates_resolved(native, raster_format, &context)?;
    if !candidate_set_names_only_member(&native_candidates, native, native_exists)
        || !candidate_set_names_only_member(&raster_candidates, raster, raster_exists)
    {
        return Err(IoError::ConfirmationRequired);
    }
    Ok(())
}

fn verify_installed_alias_invariant(
    native: &Path,
    raster: &Path,
    raster_format: CommonRasterFormat,
) -> IoResult<()> {
    let context = JobContext::new();
    let native_candidates = native_candidates_resolved(raster, &context)?;
    let raster_candidates = raster_candidates_resolved(native, raster_format, &context)?;
    if !candidate_set_names_only_member(&native_candidates, native, true)
        || !candidate_set_names_only_member(&raster_candidates, raster, true)
    {
        return Err(IoError::ConfirmationRequired);
    }
    Ok(())
}

fn rollback_install_record(
    manager: &IoManager,
    parent: &Path,
    journal: &Path,
    record: &Record,
    journal_proof: &Proof,
    commit_stage_proof: &Proof,
    rollback_stage_proof: &Proof,
) -> IoResult<RestoredPair> {
    let context = JobContext::new();
    // Once the committed marker is visible the disk transaction has crossed
    // its durable publication boundary and must never be rolled back as an
    // ordinary live-install failure. An invalid marker is likewise uncertain
    // evidence and is retained for explicit recovery.
    if commit_marker_present(journal, record, &context)? {
        return Err(IoError::InvalidInput(
            "paired installation already published its commit marker",
        ));
    }
    let already_rolling_back = rollback_marker_present(journal, record, &context)?;
    let observed = [
        classify(parent, &record.native, &context)?,
        classify(parent, &record.raster, &context)?,
    ];
    if !already_rolling_back && observed.contains(&Current::Missing) {
        return Err(IoError::InvalidInput(
            "paired install target disappeared before rollback was authorized",
        ));
    }
    verify_rollback_backups(parent, record, observed, &context)?;
    publish_rollback_marker(journal, record, Some(rollback_stage_proof))?;
    let states = [
        classify(parent, &record.native, &context)?,
        classify(parent, &record.raster, &context)?,
    ];
    invalidate_record(manager, record);
    for (member, state) in [&record.native, &record.raster].into_iter().zip(states) {
        restore_member_exact(parent, member, state, &context)?;
    }
    sync_directory(parent)?;
    let restored = restored_pair_stamps(parent, record, &context)?;
    // Restored finals are authoritative before evidence disposal. Cleanup
    // failure leaves the journal for a later recovery and does not invalidate
    // the verified restored stamps.
    let _ = cleanup_with_stamps(
        parent,
        journal,
        record,
        journal_proof,
        commit_stage_proof,
        rollback_stage_proof,
    );
    Ok(restored)
}

fn verify_rollback_backups(
    parent: &Path,
    record: &Record,
    states: [Current; 2],
    context: &JobContext,
) -> IoResult<()> {
    for (member, state) in [&record.native, &record.raster].into_iter().zip(states) {
        if state != Current::Original && member.original.is_some() {
            let backup = member.backup_proof.as_ref().ok_or(IoError::InvalidInput(
                "paired rollback backup proof is missing",
            ))?;
            verify_proof(&parent.join(&member.backup), backup, context)?;
        }
    }
    Ok(())
}

fn restore_member_exact(
    parent: &Path,
    member: &Member,
    state: Current,
    context: &JobContext,
) -> IoResult<()> {
    if state == Current::Original {
        return Ok(());
    }
    let target = parent.join(&member.name);
    if state == Current::Replacement {
        let current = installed_replacement_stamp(&target, &member.replacement, context)?;
        backend::remove_exact(&target, current)?;
    }
    if member.original.is_some() {
        let backup = member.backup_proof.as_ref().ok_or(IoError::InvalidInput(
            "paired rollback backup proof is missing",
        ))?;
        let backup_path = parent.join(&member.backup);
        verify_proof(&backup_path, backup, context)?;
        // The target was either handle-bound deleted or was already missing
        // under a durable rollback marker. No-overwrite publication ensures an
        // external process can win the path race without being overwritten.
        backend::replace(&backup_path, &target, false)?;
        verify_proof(&target, backup, context)?;
    }
    Ok(())
}

fn restored_pair_stamps(
    parent: &Path,
    record: &Record,
    context: &JobContext,
) -> IoResult<RestoredPair> {
    let (native, native_missing) = restored_member_authority(parent, &record.native, context)?;
    let (raster, raster_missing) = restored_member_authority(parent, &record.raster, context)?;
    Ok(RestoredPair {
        native,
        raster,
        native_missing,
        raster_missing,
    })
}

fn restored_member_authority(
    parent: &Path,
    member: &Member,
    context: &JobContext,
) -> IoResult<(Option<FileStamp>, Option<FileIdentity>)> {
    let path = parent.join(&member.name);
    let Some(expected) = &member.original else {
        if optional_stamp(&path)?.is_some() {
            return Err(IoError::ChangedDuringRead);
        }
        return Ok((None, Some(backend::missing_identity(&path))));
    };
    let actual = proof(&path, expected.stamp.length.max(1), context)?;
    if actual.stamp.length != expected.stamp.length || actual.digest != expected.digest {
        return Err(IoError::ChangedDuringRead);
    }
    Ok((Some(actual.stamp), None))
}

fn candidate_set_names_only_member(
    candidates: &[PathBuf],
    member: &Path,
    member_exists: bool,
) -> bool {
    if !member_exists {
        return candidates.is_empty();
    }
    candidates.len() == 1 && backend::lock_path(&candidates[0]) == backend::lock_path(member)
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

fn commit_path(journal: &Path) -> PathBuf {
    journal.with_extension("commit")
}

fn commit_stage_path(journal: &Path) -> PathBuf {
    journal.with_extension("commit-new")
}

fn rollback_path(journal: &Path) -> PathBuf {
    journal.with_extension("rollback")
}

fn rollback_stage_path(journal: &Path) -> PathBuf {
    journal.with_extension("rollback-new")
}

fn committed_record(record: &Record) -> Record {
    let mut committed = record.clone();
    committed.committed = true;
    committed
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
    let mut paths = vec![
        journal.to_path_buf(),
        commit_path(journal),
        commit_stage_path(journal),
        rollback_path(journal),
        rollback_stage_path(journal),
    ];
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

fn verify_proof(path: &Path, expected: &Proof, context: &JobContext) -> IoResult<Proof> {
    let actual = proof(path, expected.stamp.length.max(1), context)?;
    if actual.stamp.identity != expected.stamp.identity
        || actual.stamp.length != expected.stamp.length
        || actual.digest != expected.digest
    {
        return Err(IoError::InvalidInput(
            "paired recovery artifact identity or digest changed",
        ));
    }
    Ok(actual)
}

fn verify_commit_artifact(path: &Path, prepared: &Record, context: &JobContext) -> IoResult<Proof> {
    let bytes = read_journal(path)?;
    let record = codec::decode(&bytes)?;
    if record != committed_record(prepared) {
        return Err(IoError::InvalidInput(
            "paired commit artifact does not match its prepared journal",
        ));
    }
    let actual = proof(path, MAX_JOURNAL_BYTES, context)?;
    if actual.digest != *blake3::hash(&bytes).as_bytes() {
        return Err(IoError::ChangedDuringRead);
    }
    Ok(actual)
}

fn verify_rollback_artifact(
    path: &Path,
    prepared: &Record,
    context: &JobContext,
) -> IoResult<Proof> {
    let bytes = read_journal(path)?;
    let record = codec::decode(&bytes)?;
    if &record != prepared {
        return Err(IoError::InvalidInput(
            "paired rollback artifact does not match its prepared journal",
        ));
    }
    let actual = proof(path, MAX_JOURNAL_BYTES, context)?;
    if actual.digest != *blake3::hash(&bytes).as_bytes() {
        return Err(IoError::ChangedDuringRead);
    }
    Ok(actual)
}

fn commit_marker_present(
    journal: &Path,
    prepared: &Record,
    context: &JobContext,
) -> IoResult<bool> {
    let path = commit_path(journal);
    if !path.try_exists()? {
        return Ok(false);
    }
    verify_commit_artifact(&path, prepared, context)?;
    Ok(true)
}

fn rollback_marker_present(
    journal: &Path,
    prepared: &Record,
    context: &JobContext,
) -> IoResult<bool> {
    let path = rollback_path(journal);
    if !path.try_exists()? {
        return Ok(false);
    }
    verify_rollback_artifact(&path, prepared, context)?;
    Ok(true)
}

fn publish_commit_marker(journal: &Path, stage_proof: &Proof) -> IoResult<()> {
    let context = JobContext::new();
    let stage = commit_stage_path(journal);
    verify_proof(&stage, stage_proof, &context)?;
    // `stage_proof` was captured from codec::encode(committed_record) during
    // preparation. Rename preserves its physical identity and bytes. The
    // backend's replace contract includes write-through/directory durability.
    backend::replace(&stage, &commit_path(journal), false)?;
    verify_proof(&commit_path(journal), stage_proof, &context)?;
    Ok(())
}

fn publish_rollback_marker(
    journal: &Path,
    prepared: &Record,
    stage_proof: Option<&Proof>,
) -> IoResult<()> {
    publish_rollback_marker_inner(journal, prepared, stage_proof, None)
}

fn publish_rollback_marker_inner(
    journal: &Path,
    prepared: &Record,
    stage_proof: Option<&Proof>,
    external_after_stage_verification: Option<&[u8]>,
) -> IoResult<()> {
    let context = JobContext::new();
    if commit_marker_present(journal, prepared, &context)? {
        return Err(IoError::InvalidInput(
            "paired commit marker forbids rollback publication",
        ));
    }
    if rollback_marker_present(journal, prepared, &context)? {
        return Ok(());
    }
    let stage = rollback_stage_path(journal);
    let exact_stage = if let Some(expected) = stage_proof {
        // A live prepared operation owns one exact staged object. A missing or
        // replaced pathname is a conflict; recreating it here would discard
        // evidence belonging to a later occupant.
        verify_proof(&stage, expected, &context)?
    } else if stage.try_exists()? {
        // Crash recovery has no in-memory creation stamp, but the complete
        // checksummed rollback record proves semantic ownership. Verification
        // also captures a stable object before the no-overwrite publication.
        verify_rollback_artifact(&stage, prepared, &context)?
    } else {
        let bytes = codec::encode(prepared)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        verify_rollback_artifact(&stage, prepared, &context)?
    };
    #[cfg(test)]
    if let Some(bytes) = external_after_stage_verification {
        std::fs::remove_file(&stage)?;
        std::fs::write(&stage, bytes)?;
    }
    #[cfg(not(test))]
    let _ = external_after_stage_verification;
    match backend::replace(&stage, &rollback_path(journal), false) {
        Ok(()) => {
            verify_proof(&rollback_path(journal), &exact_stage, &context)?;
            Ok(())
        }
        Err(error) => {
            if rollback_marker_present(journal, prepared, &context)? {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

fn install_member_exact(
    parent: &Path,
    member: &Member,
    context: &JobContext,
    external_after_delete: Option<&[u8]>,
) -> IoResult<()> {
    let stage = parent.join(&member.stage);
    verify_proof(&stage, &member.replacement, context)?;
    let target = parent.join(&member.name);
    if let Some(original) = &member.original {
        let actual = proof(&target, original.stamp.length.max(1), context)?;
        if actual.stamp.identity != original.stamp.identity
            || actual.stamp.length != original.stamp.length
            || actual.digest != original.digest
        {
            return Err(IoError::ChangedDuringRead);
        }
        // Windows binds this second complete-stamp check and deletion to one
        // exclusive handle. Publication is no-overwrite, so an external file
        // created in the gap wins the path instead of being overwritten.
        backend::remove_exact(&target, actual.stamp)?;
    } else if optional_stamp(&target)?.is_some() {
        return Err(IoError::ChangedDuringRead);
    }
    #[cfg(test)]
    if let Some(bytes) = external_after_delete {
        std::fs::write(&target, bytes)?;
    }
    #[cfg(not(test))]
    let _ = external_after_delete;
    backend::replace(&stage, &target, false)?;
    installed_replacement_stamp(&target, &member.replacement, context).map(|_| ())
}

fn installed_replacement_stamp(
    path: &Path,
    expected: &Proof,
    context: &JobContext,
) -> IoResult<FileStamp> {
    let actual = proof(path, expected.stamp.length.max(1), context)?;
    if actual.stamp.identity != expected.stamp.identity
        || actual.stamp.length != expected.stamp.length
        || actual.digest != expected.digest
    {
        return Err(IoError::ChangedDuringRead);
    }
    Ok(actual.stamp)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Current {
    Original,
    Replacement,
    Missing,
}

fn classify(parent: &Path, member: &Member, context: &JobContext) -> IoResult<Current> {
    let path = parent.join(&member.name);
    let Some(current) = optional_stamp(&path)? else {
        return if member.original.is_none() {
            Ok(Current::Original)
        } else {
            Ok(Current::Missing)
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
    if record.committed {
        return Err(IoError::InvalidInput(
            "paired recovery journal has an invalid commit phase",
        ));
    }
    let committed = commit_marker_present(journal, record, context)?;
    let rolling_back = rollback_marker_present(journal, record, context)?;
    let mut states = [
        classify(parent, &record.native, context)?,
        classify(parent, &record.raster, context)?,
    ];
    if committed {
        if states != [Current::Replacement, Current::Replacement] {
            return Err(IoError::InvalidInput(
                "paired committed marker does not have both replacement members",
            ));
        }
        cleanup(parent, journal, record, context)?;
        invalidate_record(manager, record);
        return Ok(PairRecovery::Completed);
    }
    if !rolling_back && states == [Current::Original, Current::Original] {
        cleanup(parent, journal, record, context)?;
        return Ok(PairRecovery::PreparedDiscarded);
    }
    if !rolling_back && states.contains(&Current::Missing) {
        return Err(IoError::InvalidInput(
            "paired recovery target disappeared before rollback was authorized",
        ));
    }
    verify_rollback_backups(parent, record, states, context)?;
    context.check_cancelled()?;
    // The rollback marker is durable before any handle-bound delete creates a
    // temporary missing-final state. Recovery can therefore resume that state
    // without confusing an external pre-rollback deletion with our own work.
    publish_rollback_marker(journal, record, None)?;
    states = [
        classify(parent, &record.native, &JobContext::new())?,
        classify(parent, &record.raster, &JobContext::new())?,
    ];
    for (member, state) in [&record.native, &record.raster].into_iter().zip(states) {
        restore_member_exact(parent, member, state, &JobContext::new())?;
    }
    invalidate_record(manager, record);
    cleanup(parent, journal, record, &JobContext::new())?;
    Ok(PairRecovery::RolledBack)
}

fn cleanup(parent: &Path, journal: &Path, record: &Record, context: &JobContext) -> IoResult<()> {
    // Verify every candidate before removing the first one. Each deletion then
    // repeats the complete-stamp check inside `remove_exact`, so an external
    // pathname replacement between verification and cleanup is retained.
    let mut auxiliaries = Vec::new();
    let commit = commit_path(journal);
    let committed = commit.try_exists()?;
    let rollback = rollback_path(journal);
    let rolling_back = rollback.try_exists()?;
    let commit_proof = committed
        .then(|| verify_commit_artifact(&commit, record, context))
        .transpose()?;
    let rollback_proof = rolling_back
        .then(|| verify_rollback_artifact(&rollback, record, context))
        .transpose()?;
    let primary = if let Some(proof) = &commit_proof {
        if let Some(rollback_proof) = &rollback_proof {
            // Commit is the single success boundary. A rollback marker may
            // legitimately coexist until post-commit cleanup removes it.
            auxiliaries.push((rollback.clone(), rollback_proof.stamp));
        }
        Some((commit.clone(), proof.stamp))
    } else {
        rollback_proof
            .as_ref()
            .map(|proof| (rollback.clone(), proof.stamp))
    };
    let commit_stage = commit_stage_path(journal);
    if commit_stage.try_exists()? {
        let proof = verify_commit_artifact(&commit_stage, record, context)?;
        auxiliaries.push((commit_stage, proof.stamp));
    }
    let rollback_stage = rollback_stage_path(journal);
    if rollback_stage.try_exists()? {
        let proof = verify_rollback_artifact(&rollback_stage, record, context)?;
        auxiliaries.push((rollback_stage, proof.stamp));
    }
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
            let actual = verify_proof(&path, expected, context)?;
            auxiliaries.push((path, actual.stamp));
        }
    }
    let journal_proof = if journal.try_exists()? {
        Some(verify_rollback_artifact(journal, record, context)?)
    } else {
        None
    };
    if primary.is_none() && journal_proof.is_none() {
        return Err(IoError::InvalidInput(
            "paired cleanup has no authoritative journal or phase marker",
        ));
    }
    context.check_cancelled()?;
    for (path, stamp) in auxiliaries {
        backend::remove_exact(&path, stamp)?;
    }
    if let Some((primary_path, primary_stamp)) = primary {
        // The phase marker is the last authority removed. If a crash occurs
        // after removing the prepared journal, recovery can finish from the
        // complete record held by this marker.
        if let Some(proof) = journal_proof {
            backend::remove_exact(journal, proof.stamp)?;
            sync_directory(parent)?;
        }
        backend::remove_exact(&primary_path, primary_stamp)?;
        sync_directory(parent)
    } else {
        backend::remove_exact(
            journal,
            journal_proof.ok_or(IoError::ChangedDuringRead)?.stamp,
        )?;
        sync_directory(parent)
    }
}

fn cleanup_with_stamps(
    parent: &Path,
    journal: &Path,
    record: &Record,
    journal_proof: &Proof,
    commit_stage_proof: &Proof,
    rollback_stage_proof: &Proof,
) -> IoResult<()> {
    let context = JobContext::new();
    let mut auxiliaries = Vec::new();
    let commit = commit_path(journal);
    let committed = commit.try_exists()?;
    let rollback = rollback_path(journal);
    let rolling_back = rollback.try_exists()?;
    let commit_proof = committed
        .then(|| verify_proof(&commit, commit_stage_proof, &context))
        .transpose()?;
    let rollback_proof = rolling_back
        .then(|| verify_proof(&rollback, rollback_stage_proof, &context))
        .transpose()?;
    let primary = if let Some(proof) = &commit_proof {
        if let Some(rollback_proof) = &rollback_proof {
            auxiliaries.push((rollback.clone(), rollback_proof.stamp));
        }
        Some((commit, proof.stamp))
    } else {
        rollback_proof.as_ref().map(|proof| (rollback, proof.stamp))
    };
    let commit_stage = commit_stage_path(journal);
    if commit_stage.try_exists()? {
        let actual = verify_proof(&commit_stage, commit_stage_proof, &context)?;
        auxiliaries.push((commit_stage, actual.stamp));
    }
    let rollback_stage = rollback_stage_path(journal);
    if rollback_stage.try_exists()? {
        let actual = verify_proof(&rollback_stage, rollback_stage_proof, &context)?;
        auxiliaries.push((rollback_stage, actual.stamp));
    }
    for member in [&record.native, &record.raster] {
        for (name, expected) in [
            (&member.stage, Some(&member.replacement)),
            (&member.backup, member.backup_proof.as_ref()),
        ] {
            let path = parent.join(name);
            let Some(expected) = expected else {
                if path.try_exists()? {
                    return Err(IoError::InvalidInput("unexpected paired recovery artifact"));
                }
                continue;
            };
            if path.try_exists()? {
                let actual = verify_proof(&path, expected, &context)?;
                auxiliaries.push((path, actual.stamp));
            }
        }
    }
    let journal_exists = journal.try_exists()?;
    let current_journal_proof = journal_exists
        .then(|| verify_proof(journal, journal_proof, &context))
        .transpose()?;
    if primary.is_none() && !journal_exists {
        return Err(IoError::InvalidInput(
            "paired cleanup has no authoritative journal or phase marker",
        ));
    }
    for (path, stamp) in auxiliaries {
        backend::remove_exact(&path, stamp)?;
    }
    if let Some((primary_path, primary_stamp)) = primary {
        if let Some(proof) = current_journal_proof {
            backend::remove_exact(journal, proof.stamp)?;
            sync_directory(parent)?;
        }
        backend::remove_exact(&primary_path, primary_stamp)?;
        sync_directory(parent)
    } else {
        backend::remove_exact(
            journal,
            current_journal_proof
                .ok_or(IoError::ChangedDuringRead)?
                .stamp,
        )?;
        sync_directory(parent)
    }
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
