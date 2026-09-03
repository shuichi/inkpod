use super::batch::{BatchPrepared, BatchWork};
use super::model::*;
use super::prepare;
use crate::persistence_task::PersistenceState;
use crate::{
    Core, CoreError, DocumentOpenToken, DocumentRevision, DocumentSaveToken, EditorRevision,
    LightTableItemInfo, LightTableItemInput, SequenceCellSource, StateId,
};
use inkpod_io::{
    ImageBatch, IoJob, IoManager, JobProgress, JobState, LoadedImage, PairInstallOutcome,
    PreparedPair, RecoveryArtifactProof, RestoredPair,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_JOB: AtomicU64 = AtomicU64::new(1);

pub(super) struct Target {
    authority: PersistenceState,
    uuid: Option<u128>,
    revision: DocumentRevision,
    state: StateId,
    editor: Option<EditorRevision>,
    sequence: Option<u64>,
    pair_authority: Option<Box<SavedPair>>,
    pair_plan: Option<Box<PlannedPair>>,
}

impl Target {
    fn capture(core: &Core) -> Self {
        Self {
            authority: core.persistence_state.clone(),
            uuid: core.document.as_ref().map(|document| document.uuid),
            revision: core.document_revision,
            state: core.current_state,
            editor: core.editor_session.as_ref().map(|editor| editor.revision),
            sequence: core.sequence.as_ref().map(|sequence| sequence.revision),
            pair_authority: core.io_pair_authority.clone().map(Box::new),
            pair_plan: core.io_pair_plan.clone().map(Box::new),
        }
    }

    pub(super) fn validate(&self, core: &Core, sequence_only: bool) -> Result<(), CoreError> {
        let current = Self::capture(core);
        if self.authority != current.authority
            || self.uuid != current.uuid
            || self.pair_authority != current.pair_authority
            || self.pair_plan != current.pair_plan
            || (sequence_only && self.sequence != current.sequence)
            || (!sequence_only
                && (self.revision != current.revision
                    || self.state != current.state
                    || self.editor != current.editor))
        {
            return Err(CoreError::InvalidState("file job target is stale"));
        }
        core.ensure_no_active_stroke()
    }
}

pub(super) enum Prepared {
    Open(
        Box<Core>,
        Option<inkpod_io::RecoveryCandidate>,
        Option<PathBuf>,
    ),
    Sequence {
        sources: Vec<SequenceCellSource>,
        residents: Vec<(u64, Box<Core>)>,
    },
    References(Vec<LoadedImage>),
    LightTable(LightTableItemInput),
    Pair(Box<PreparedPair>, DocumentSaveToken, PairRepairTarget),
    Output(Option<RecoveryArtifactProof>),
    Batch(Box<BatchPrepared>),
    Recovery(Vec<inkpod_io::RecoveryCandidate>),
    SequenceSwitch(Box<crate::PreparedSequenceSwitch>),
    NativeOutput(inkpod_format::NativeFile, DocumentSaveToken),
}

pub(super) enum PairRepairTarget {
    Unrelated,
    Revoke,
    Committed(SavedPair),
    Planned(PlannedPair),
}

impl PairRepairTarget {
    pub(super) const fn affects_current_authority(&self) -> bool {
        !matches!(self, Self::Unrelated)
    }
}

pub(super) enum PairAuthorityRepair {
    Committed(SavedPair),
    Planned(PlannedPair),
}

pub(super) enum InstallCompletion {
    Standard(Option<SavedPair>, Option<RecoveryArtifactProof>),
    PairInstalled(SavedPair),
    PairRolledBack {
        error: CoreError,
        restored: Option<RestoredPair>,
    },
    PairFailedAfterPublication(CoreError),
}

pub(super) struct Discovery {
    pub paths: Vec<PathBuf>,
    pub seed: Option<usize>,
    pub truncated: bool,
}

pub(super) enum Pending {
    Discover(IoJob<Result<Discovery, CoreError>>),
    Images(ImageBatch),
    Prepare(IoJob<Result<(Prepared, Vec<FileIoItem>), CoreError>>),
    Install(IoJob<Result<InstallCompletion, CoreError>>),
    BatchDiscover(IoJob<Result<(BatchWork, Vec<PathBuf>), CoreError>>),
    BatchImages(ImageBatch, Box<BatchWork>),
}

/// Application-owned asynchronous file request, with no borrowed live Core.
///
/// Polling only transfers bounded metadata/results; filesystem access, decode,
/// replay, conversion and encoding run on the manager's bounded worker pool.
/// `apply` must run on the originating Core's owner thread. Drop cancels queued
/// work; an authorized installation must be polled and finalized before drop.
pub struct FileIoJob {
    pub(super) manager: IoManager,
    pub(super) request: FileIoRequest,
    pub(super) target: Option<Target>,
    pub(super) open_token: Option<DocumentOpenToken>,
    pub(super) ready: Option<Prepared>,
    pub(super) save_token: Option<DocumentSaveToken>,
    pub(super) installed: Option<SavedPair>,
    pub(super) pair_repair_target: PairRepairTarget,
    pub(super) pair_authority_repair: Option<PairAuthorityRepair>,
    pub(super) pair_publication_started: bool,
    pub(super) recovery_artifact_proof: Option<RecoveryArtifactProof>,
    pub(super) sequence_install: Option<Box<crate::PreparedSequenceSwitch>>,
    pub(super) progress: FileIoProgress,
    pub(super) error: Option<CoreError>,
    pub(super) items: Vec<FileIoItem>,
    pub(super) pending: Option<Pending>,
    pub(super) validated_target_cache: Option<super::ValidatedTargetCache>,
    pub(super) batch_report: Option<crate::BatchRunReport>,
    pub(super) batch_preview: Option<crate::BatchPreview>,
    pub(super) recoveries: Vec<inkpod_io::RecoveryCandidate>,
    batch_prefetch_reads: u64,
    images: Vec<Option<LoadedImage>>,
    image_error: Option<CoreError>,
    #[cfg(test)]
    pair_install_fault: Option<inkpod_io::PairInstallFault>,
    seed: Option<usize>,
    reload: Option<LightTableItemInfo>,
    cancel_requested: bool,
}

impl FileIoJob {
    /// Captures bounded request metadata and starts detached work without waiting.
    /// Reference and recovery catalog operations permit `core == None`. A request rejected before
    /// acceptance changes no live state and performs no filesystem operation.
    pub fn start(
        core: Option<&Core>,
        manager: IoManager,
        mut request: FileIoRequest,
    ) -> Result<Self, CoreError> {
        prepare::validate_request(&request)?;
        let reference = matches!(
            request.kind,
            FileIoKind::ReferenceFiles
                | FileIoKind::ReferenceFolder
                | FileIoKind::RecoveryList
                | FileIoKind::RecoveryDiscard
                | FileIoKind::RecoveryProbe
        );
        if !reference {
            core.ok_or(CoreError::NoDocument)?
                .ensure_no_active_stroke()?;
        }
        if let Some(metadata) = &request.recovery_metadata {
            let document = core
                .and_then(|core| core.document.as_ref())
                .ok_or(CoreError::NoDocument)?;
            if metadata.document_uuid != document.uuid {
                return Err(CoreError::InvalidArgument(
                    "recovery metadata belongs to a different document",
                ));
            }
        }
        // An application autosave may be detached from a resident sequence
        // exchange, but recovery still needs the exact normal-pair proof owned
        // by Core. Enrich the copied metadata before the save snapshot is
        // handed to the worker. Standalone/native-only documents retain NONE.
        if request.kind == FileIoKind::Autosave
            && core
                .is_some_and(|core| core.io_pair_authority.is_some() || core.io_pair_plan.is_some())
            && let Some(metadata) = request.recovery_metadata.as_mut()
        {
            super::session::bind_sequence_recovery_pair(
                core.ok_or(CoreError::NoDocument)?,
                metadata,
            )?;
        }
        let open_token = if matches!(
            request.kind,
            FileIoKind::OpenNative
                | FileIoKind::OpenRecovery
                | FileIoKind::OpenRaster
                | FileIoKind::OpenRasterPair
        ) {
            Some(core.ok_or(CoreError::NoDocument)?.capture_document_open()?)
        } else {
            None
        };
        let reload = if request.kind == FileIoKind::LightTableReload {
            Some(
                core.ok_or(CoreError::NoDocument)?
                    .light_table_items()?
                    .into_iter()
                    .find(|item| item.id == request.object_id)
                    .ok_or(CoreError::InvalidArgument(
                        "light-table reload target is missing",
                    ))?,
            )
        } else {
            None
        };
        let kind = request.kind;
        let mut job = Self::allocate(core, manager, request)?;
        job.open_token = open_token;
        job.reload = reload;
        match kind {
            FileIoKind::SequenceAuto | FileIoKind::ReferenceFolder => {
                let manager = job.manager.clone();
                let request = job.request.clone();
                job.pending = Some(Pending::Discover(job.manager.submit(move |context| {
                    Ok(prepare::discover(&manager, &request, &context))
                })?));
            }
            FileIoKind::OpenNative | FileIoKind::OpenRecovery => {
                let manager = job.manager.clone();
                let request = job.request.clone();
                job.progress.total_count = 1;
                job.progress.discovered_count = 1;
                job.pending = Some(Pending::Prepare(job.manager.submit(move |context| {
                    Ok(prepare::native(&manager, &request, &context))
                })?));
            }
            FileIoKind::SavePair | FileIoKind::Autosave | FileIoKind::ExportRaster => {
                let snapshot = core.ok_or(CoreError::NoDocument)?.capture_document_save()?;
                let manager = job.manager.clone();
                let request = job.request.clone();
                let expected = core.and_then(|core| core.io_pair_authority.clone());
                let planned = core.and_then(|core| core.io_pair_plan.clone());
                job.pending = Some(Pending::Prepare(job.manager.submit(move |context| {
                    Ok(prepare::save(
                        &manager, &request, snapshot, expected, planned, &context,
                    ))
                })?));
            }
            FileIoKind::BatchPlan
            | FileIoKind::BatchRun
            | FileIoKind::BatchPreview
            | FileIoKind::SequenceSwitch
            | FileIoKind::CompactedCopy => {
                return Err(CoreError::InvalidArgument(
                    "Batch jobs require a captured graph",
                ));
            }
            FileIoKind::RecoveryList | FileIoKind::RecoveryDiscard | FileIoKind::RecoveryProbe => {
                let manager = job.manager.clone();
                let request = job.request.clone();
                job.pending = Some(Pending::Prepare(job.manager.submit(move |context| {
                    Ok(super::recovery::prepare(&manager, &request, &context))
                })?));
            }
            FileIoKind::ExportSequence => {
                let frozen = core.ok_or(CoreError::NoDocument)?.clone();
                let manager = job.manager.clone();
                let request = job.request.clone();
                job.pending = Some(Pending::Prepare(job.manager.submit(move |context| {
                    Ok(super::recovery::export_sequence(
                        frozen, &manager, &request, &context,
                    ))
                })?));
            }
            _ => job.start_images(job.request.paths.clone())?,
        }
        Ok(job)
    }

    /// Starts a request with one application-owned validated sidecar cache.
    ///
    /// Sequence discovery uses this cache while eagerly preparing complete
    /// inactive editing states. Other request kinds retain their normal behavior.
    pub fn start_with_validated_target_cache(
        core: Option<&Core>,
        manager: IoManager,
        target_cache: super::ValidatedTargetCache,
        request: FileIoRequest,
    ) -> Result<Self, CoreError> {
        let mut job = Self::start(core, manager, request)?;
        job.validated_target_cache = Some(target_cache);
        Ok(job)
    }

    pub(super) fn allocate(
        core: Option<&Core>,
        manager: IoManager,
        request: FileIoRequest,
    ) -> Result<Self, CoreError> {
        let id = NEXT_JOB
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| CoreError::InvalidState("file job IDs exhausted"))?;
        let kind = request.kind;
        Ok(Self {
            manager,
            request,
            target: core.map(Target::capture),
            open_token: None,
            ready: None,
            save_token: None,
            installed: None,
            pair_repair_target: PairRepairTarget::Unrelated,
            pair_authority_repair: None,
            pair_publication_started: false,
            recovery_artifact_proof: None,
            sequence_install: None,
            pending: None,
            validated_target_cache: None,
            images: Vec::new(),
            image_error: None,
            #[cfg(test)]
            pair_install_fault: None,
            batch_report: None,
            batch_preview: None,
            recoveries: Vec::new(),
            batch_prefetch_reads: 0,
            seed: None,
            reload: None,
            error: None,
            items: Vec::new(),
            cancel_requested: false,
            progress: FileIoProgress {
                job_id: id,
                kind,
                state: FileIoState::Queued,
                discovered_count: 0,
                total_count: 0,
                read_count: 0,
                loaded_count: 0,
                failed_count: 0,
                cancelled_count: 0,
                completed_work: 0,
                total_work: 0,
                result_count: 0,
                truncated: false,
                installing: false,
                authority_repaired: false,
                authority_revoked: false,
            },
        })
    }

    fn start_images(&mut self, paths: Vec<PathBuf>) -> Result<(), CoreError> {
        if paths.is_empty() {
            return Err(CoreError::InvalidArgument("no supported images found"));
        }
        self.progress.discovered_count = self.progress.discovered_count.max(paths.len() as u64);
        self.progress.total_count = paths.len() as u64;
        self.images = vec![None; paths.len()];
        self.pending = Some(Pending::Images(self.manager.submit_images(
            paths,
            self.request.force_reload || self.request.kind == FileIoKind::LightTableReload,
        )?));
        Ok(())
    }

    /// Nonblocking progress snapshot; this advances completed pipeline stages.
    /// Loaded count includes cache hits and is independent of operation work.
    pub fn poll(&mut self) -> FileIoProgress {
        if let Err(error) = self.advance() {
            self.fail(error);
        }
        self.progress.result_count = self.items.len() as u64;
        self.progress
    }

    fn observe(&mut self, progress: JobProgress) {
        self.progress.state = FileIoState::Running;
        self.progress.discovered_count = self.progress.discovered_count.max(progress.discovered);
        self.progress.read_count = self.progress.read_count.max(progress.read_completed);
        self.progress.loaded_count = self.progress.loaded_count.max(progress.loaded);
        self.progress.failed_count = self.progress.failed_count.max(progress.failed);
        self.progress.cancelled_count = self.progress.cancelled_count.max(progress.cancelled);
        self.progress.completed_work = progress.completed;
        self.progress.total_work = progress.total;
    }

    fn advance(&mut self) -> Result<(), CoreError> {
        let Some(pending) = self.pending.take() else {
            return Ok(());
        };
        match pending {
            Pending::BatchDiscover(job) => {
                self.observe(job.poll());
                if let Some(result) = job.try_take() {
                    let (work, paths) = result??;
                    if self.cancel_requested {
                        return Err(CoreError::Cancelled);
                    }
                    self.progress.total_count = work.graph.inputs.len() as u64;
                    self.progress.discovered_count = self.progress.total_count;
                    let raster_paths: Vec<_> = paths
                        .into_iter()
                        .filter(|path| prepare::raster_format(path).is_some())
                        .collect();
                    if raster_paths.is_empty() {
                        self.execute_batch(work)?;
                    } else {
                        self.pending = Some(Pending::BatchImages(
                            self.manager.submit_images(raster_paths, false)?,
                            Box::new(work),
                        ));
                    }
                } else {
                    self.pending = Some(Pending::BatchDiscover(job));
                }
            }
            Pending::BatchImages(batch, work) => {
                // Batch does not pin its whole input set: retain only the shared
                // capped LRU between prefetch and ordered operation execution.
                drop(batch.take_completed(128));
                let status = batch.poll();
                self.observe(status);
                if matches!(
                    status.state,
                    JobState::Completed | JobState::Failed | JobState::Cancelled
                ) {
                    drop(batch.take_completed(10_000));
                    if self.cancel_requested {
                        return Err(CoreError::Cancelled);
                    }
                    self.batch_prefetch_reads = status.read_completed;
                    // A failed input is reported by Batch's Stop/Continue policy,
                    // not promoted into an all-job failure by speculative prefetch.
                    self.execute_batch(*work)?;
                } else {
                    self.pending = Some(Pending::BatchImages(batch, work));
                }
            }
            Pending::Discover(job) => {
                self.observe(job.poll());
                if let Some(result) = job.try_take() {
                    let result = result??;
                    if self.cancel_requested {
                        return Err(CoreError::Cancelled);
                    }
                    self.seed = result.seed;
                    self.progress.truncated = result.truncated;
                    self.start_images(result.paths)?;
                } else {
                    self.pending = Some(Pending::Discover(job));
                }
            }
            Pending::Images(batch) => {
                for item in batch.take_completed(128) {
                    match item.result {
                        Ok(image) => self.images[item.index] = Some(image),
                        Err(error) => {
                            self.image_error.get_or_insert(error.into());
                        }
                    }
                }
                let status = batch.poll();
                self.observe(status);
                if matches!(
                    status.state,
                    JobState::Completed | JobState::Cancelled | JobState::Failed
                ) {
                    // The worker terminal state guarantees all result slots were published.
                    for item in batch.take_completed(10_000) {
                        match item.result {
                            Ok(image) => self.images[item.index] = Some(image),
                            Err(error) => {
                                self.image_error.get_or_insert(error.into());
                            }
                        }
                    }
                    if let Some(error) = self.image_error.take() {
                        return Err(error);
                    }
                    if self.cancel_requested {
                        return Err(CoreError::Cancelled);
                    }
                    let images = std::mem::take(&mut self.images)
                        .into_iter()
                        .collect::<Option<Vec<_>>>()
                        .ok_or(CoreError::InvalidState(
                            "image job finished without every result",
                        ))?;
                    let manager = self.manager.clone();
                    let request = self.request.clone();
                    let seed = self.seed;
                    let seed_uuid = self.target.as_ref().and_then(|target| target.uuid);
                    let reload = self.reload.take();
                    let target_cache = self.validated_target_cache.clone();
                    self.pending = Some(Pending::Prepare(self.manager.submit(move |context| {
                        Ok(prepare::images(
                            &manager,
                            &request,
                            prepare::ImagePreparation {
                                images,
                                seed,
                                seed_uuid,
                                reload,
                                target_cache,
                            },
                            &context,
                        ))
                    })?));
                } else {
                    self.pending = Some(Pending::Images(batch));
                }
            }
            Pending::Prepare(job) => {
                let mut status = job.poll();
                status.read_completed = status
                    .read_completed
                    .saturating_add(self.batch_prefetch_reads);
                self.observe(status);
                if let Some(result) = job.try_take() {
                    let (mut prepared, items) = result??;
                    // Explicit writes have their own filesystem commit point.
                    if self.cancel_requested && !matches!(prepared, Prepared::Output(_)) {
                        return Err(CoreError::Cancelled);
                    }
                    self.items = items;
                    if let Prepared::Output(proof) = &mut prepared {
                        self.recovery_artifact_proof = proof.take();
                    }
                    if let Prepared::Open(_, Some(candidate), _) = &prepared {
                        self.recoveries = vec![candidate.clone()];
                    }
                    if let Prepared::Recovery(recoveries) = prepared {
                        self.recoveries = recoveries;
                        self.progress.state = FileIoState::Complete;
                    } else {
                        self.ready = Some(prepared);
                        self.progress.state = FileIoState::Ready;
                    }
                } else {
                    self.pending = Some(Pending::Prepare(job));
                }
            }
            Pending::Install(job) => {
                self.observe(job.poll());
                if let Some(result) = job.try_take() {
                    // Even cancelled/failed installs need owner finalization to release its fence.
                    match result.map_err(CoreError::from).and_then(|result| result) {
                        Ok(InstallCompletion::Standard(pair, proof)) => {
                            self.installed = pair;
                            self.recovery_artifact_proof = proof;
                        }
                        Ok(InstallCompletion::PairInstalled(pair)) => {
                            self.installed = Some(pair);
                        }
                        Ok(InstallCompletion::PairRolledBack { error, restored }) => {
                            self.pair_publication_started = true;
                            if let Some(restored) = restored {
                                self.prepare_pair_authority_repair(restored);
                            }
                            self.error = Some(error);
                        }
                        Ok(InstallCompletion::PairFailedAfterPublication(error)) => {
                            self.pair_publication_started = true;
                            self.error = Some(error);
                        }
                        Err(error) => self.error = Some(error),
                    }
                    self.progress.state = FileIoState::Ready;
                } else {
                    self.pending = Some(Pending::Install(job));
                }
            }
        }
        Ok(())
    }

    /// Requests cancellation without waiting. During installation the worker
    /// finishes rollback or durable commit, then the owner must call `apply`.
    /// Cancelling a sequence switch after its durable worker result reached
    /// final `Ready` suppresses target publication but deliberately leaves the
    /// job ready for that mandatory owner finalization.
    pub fn cancel(&mut self) {
        self.cancel_requested = true;
        match &self.pending {
            Some(Pending::Discover(job)) => job.cancel(),
            Some(Pending::Images(job)) => job.cancel(),
            Some(Pending::Prepare(job)) => job.cancel(),
            Some(Pending::Install(job)) => job.cancel(),
            Some(Pending::BatchDiscover(job)) => job.cancel(),
            Some(Pending::BatchImages(job, _)) => job.cancel(),
            None if self.request.kind == FileIoKind::SequenceSwitch
                && self.progress.state == FileIoState::Ready
                && self.progress.installing =>
            {
                self.error.get_or_insert(CoreError::Cancelled);
            }
            None if self.request.kind == FileIoKind::SavePair
                && self.progress.state == FileIoState::Ready
                && self.progress.installing
                && self.progress.authority_repaired =>
            {
                // The frontend may be unable to copy the repaired item
                // identities after worker rollback. Do not let mandatory final
                // apply publish authority the frontend cannot mirror: retain
                // the original save error and fail closed through Revoke.
                self.pair_authority_repair = None;
                self.pair_repair_target = PairRepairTarget::Revoke;
                self.progress.authority_repaired = false;
            }
            None if self.progress.state == FileIoState::Ready && !self.progress.installing => {
                self.fail(CoreError::Cancelled)
            }
            None => {}
        }
    }

    /// Returns a bounded diagnostic owned by this request, never a global slot.
    #[must_use]
    pub fn error(&self) -> Option<&CoreError> {
        self.error.as_ref()
    }

    /// Borrows immutable metadata in the same order as the prepared catalog.
    pub fn item(&self, index: usize) -> Result<&FileIoItem, CoreError> {
        self.items.get(index).ok_or(CoreError::InvalidArgument(
            "file job item index is outside bounds",
        ))
    }

    /// Returns the exact native/metadata publication proof produced by a
    /// successful recovery write. It becomes available at the first `Ready`
    /// state after durable worker publication, before owner final apply, so a
    /// frontend can copy the fixed-size proof before publishing related state.
    /// Jobs that failed or did not publish recovery have none.
    pub fn recovery_artifact_proof(&self) -> Result<&RecoveryArtifactProof, CoreError> {
        if !matches!(
            self.progress.state,
            FileIoState::Ready | FileIoState::Complete
        ) || self.error.is_some()
        {
            return Err(CoreError::InvalidState(
                "recovery artifact proof is not complete",
            ));
        }
        self.recovery_artifact_proof
            .as_ref()
            .ok_or(CoreError::InvalidArgument(
                "file job did not publish a recovery artifact",
            ))
    }

    /// Borrows the effective metadata durably published with this job's
    /// recovery artifact. Sequence-switch capture may replace caller hints with
    /// Core-owned exact pair authority before submission; this query exposes
    /// that final record only when the matching artifact proof is available.
    pub fn published_recovery_metadata(&self) -> Result<&inkpod_io::RecoveryMetadata, CoreError> {
        self.recovery_artifact_proof()?;
        self.request
            .recovery_metadata
            .as_ref()
            .ok_or(CoreError::InvalidArgument(
                "file job did not publish recovery metadata",
            ))
    }

    /// True until the owner finalizes an authorized save installation.
    #[must_use]
    pub fn requires_finalization(&self) -> bool {
        self.progress.installing
    }

    pub(super) fn fail(&mut self, error: CoreError) {
        self.progress.state = if error == CoreError::Cancelled {
            FileIoState::Cancelled
        } else {
            FileIoState::Failed
        };
        self.error = Some(error);
        self.pending = None;
        self.ready = None;
        self.images.clear();
    }

    pub(super) fn install(
        &mut self,
        pair: PreparedPair,
        token: DocumentSaveToken,
        repair_target: PairRepairTarget,
    ) -> Result<(), CoreError> {
        let native_path = self.request.paths[0].clone();
        let raster_path = self
            .items
            .iter()
            .find(|item| item.format.is_some())
            .map(|item| item.path.clone())
            .ok_or(CoreError::InvalidState(
                "prepared pair raster destination is missing",
            ))?;
        #[cfg(test)]
        let pair_install_fault = self.pair_install_fault.take();
        let job = self.manager.submit(move |context| {
            #[cfg(test)]
            let outcome = match pair_install_fault {
                Some(fault) => pair.install_with_fault_outcome(&context, fault),
                None => pair.install_with_outcome(&context),
            };
            #[cfg(not(test))]
            let outcome = pair.install_with_outcome(&context);
            Ok(match outcome {
                Ok(PairInstallOutcome::Installed { native, raster }) => {
                    Ok(InstallCompletion::PairInstalled(SavedPair {
                        native_path,
                        native,
                        raster_path,
                        raster: Some(raster),
                        raster_missing: None,
                    }))
                }
                Ok(PairInstallOutcome::RolledBack { error, restored }) => {
                    Ok(InstallCompletion::PairRolledBack {
                        error: error.into(),
                        restored,
                    })
                }
                Ok(PairInstallOutcome::FailedAfterPublication { error }) => {
                    Ok(InstallCompletion::PairFailedAfterPublication(error.into()))
                }
                Err(error) => Err(error.into()),
            })
        })?;
        self.save_token = Some(token);
        self.pair_repair_target = repair_target;
        self.progress.installing = true;
        self.progress.state = FileIoState::Running;
        self.pending = Some(Pending::Install(job));
        Ok(())
    }

    fn prepare_pair_authority_repair(&mut self, restored: RestoredPair) {
        let target = std::mem::replace(&mut self.pair_repair_target, PairRepairTarget::Revoke);
        let repair = match target {
            PairRepairTarget::Committed(mut saved) => (|| {
                let native = restored.native?;
                if restored.native_missing.is_some()
                    || restored.raster.is_some() != saved.raster.is_some()
                    || restored.raster_missing.is_some() != saved.raster_missing.is_some()
                {
                    return None;
                }
                let (raster_identity, raster_physical) = match restored.raster {
                    Some(raster) => (raster.identity, true),
                    None => (restored.raster_missing?, false),
                };
                saved.native = native;
                saved.raster = restored.raster;
                saved.raster_missing = restored.raster_missing;
                self.update_pair_item_authorities(
                    &saved.native_path,
                    &saved.raster_path,
                    native.identity,
                    true,
                    raster_identity,
                    raster_physical,
                )
                .then_some(PairAuthorityRepair::Committed(saved))
            })(),
            PairRepairTarget::Planned(mut planned) => (|| {
                let raster = restored.raster?;
                if restored.native.is_some()
                    || restored.native_missing != Some(planned.native_missing)
                    || restored.raster_missing.is_some()
                {
                    return None;
                }
                planned.raster = raster;
                self.update_pair_item_authorities(
                    &planned.native_path,
                    &planned.raster_path,
                    planned.native_missing,
                    false,
                    raster.identity,
                    true,
                )
                .then_some(PairAuthorityRepair::Planned(planned))
            })(),
            PairRepairTarget::Unrelated => {
                self.pair_repair_target = PairRepairTarget::Unrelated;
                None
            }
            PairRepairTarget::Revoke => None,
        };
        if let Some(repair) = repair {
            self.pair_authority_repair = Some(repair);
            self.progress.authority_repaired = true;
        }
    }

    fn update_pair_item_authorities(
        &mut self,
        native_path: &std::path::Path,
        raster_path: &std::path::Path,
        native: inkpod_io::FileIdentity,
        native_physical: bool,
        raster: inkpod_io::FileIdentity,
        raster_physical: bool,
    ) -> bool {
        let Some(document_uuid) = self.target.as_ref().and_then(|target| target.uuid) else {
            return false;
        };
        let [native_item, raster_item] = self.items.as_mut_slice() else {
            return false;
        };
        let item_name_matches = |item: &FileIoItem| {
            item.path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                == item.name
        };
        if native_item.path != native_path
            || native_item.format.is_some()
            || native_item.source_generation != 1
            || native_item.document_uuid != document_uuid
            || !item_name_matches(native_item)
            || raster_item.path != raster_path
            || raster_item.format.is_none()
            || raster_item.source_generation != 1
            || raster_item.document_uuid != document_uuid
            || !item_name_matches(raster_item)
        {
            return false;
        }
        native_item.identity = native;
        native_item.identity_physical = native_physical;
        raster_item.identity = raster;
        raster_item.identity_physical = raster_physical;
        true
    }
}

impl Core {
    /// Binds the process-owned filesystem manager. No filesystem access occurs.
    /// Each production Core in the same application must share this manager.
    pub fn bind_file_io(&mut self, manager: IoManager) -> Result<(), CoreError> {
        if self.io_install_pending {
            return Err(CoreError::InvalidState("save installation is pending"));
        }
        self.io_manager = Some(manager);
        Ok(())
    }

    /// Clones the bound manager; standalone callers receive an isolated default
    /// service. This creates no process-global mutable document or active handle.
    pub fn file_io_manager(&self) -> Result<IoManager, CoreError> {
        match &self.io_manager {
            Some(manager) => Ok(manager.clone()),
            None => Ok(IoManager::new(inkpod_io::IoConfig::default())?),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkpod_io::{FileIdentity, FileStamp, IoConfig, JobContext};
    use std::time::Duration;

    fn wait_for_file_job(job: &mut FileIoJob) -> FileIoProgress {
        for _ in 0..10_000 {
            let progress = job.poll();
            if !matches!(progress.state, FileIoState::Queued | FileIoState::Running) {
                return progress;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("file job did not finish within the test deadline");
    }

    fn complete_pair_save(core: &mut Core, manager: &IoManager, native: &std::path::Path) {
        let mut job = FileIoJob::start(
            Some(core),
            manager.clone(),
            FileIoRequest::new(FileIoKind::SavePair, vec![native.to_path_buf()]),
        )
        .unwrap();
        assert_eq!(
            wait_for_file_job(&mut job).state,
            FileIoState::Ready,
            "{:?}",
            job.error()
        );
        assert!(matches!(job.apply(core).unwrap(), FileIoApply::Pending));
        assert_eq!(
            wait_for_file_job(&mut job).state,
            FileIoState::Ready,
            "{:?}",
            job.error()
        );
        assert!(matches!(
            job.apply(core).unwrap(),
            FileIoApply::Complete { .. }
        ));
    }

    fn stamp(file: u128, length: u64) -> FileStamp {
        FileStamp {
            identity: FileIdentity { volume: 7, file },
            length,
            modified: i128::from(file as u64),
            changed: i128::from(file as u64),
            readonly: false,
        }
    }

    fn item(
        path: PathBuf,
        format: Option<crate::CommonRasterFormat>,
        identity: FileIdentity,
        uuid: u128,
    ) -> FileIoItem {
        FileIoItem {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap()
                .to_owned(),
            path,
            format,
            identity,
            identity_physical: true,
            source_generation: 1,
            document_uuid: uuid,
            sequence_resident_native: None,
        }
    }

    #[test]
    fn repaired_pair_authority_stales_an_already_ready_revert() {
        let manager = IoManager::new(IoConfig::default()).unwrap();
        let native_path = PathBuf::from("revert-repaired.inkpod");
        let raster_path = PathBuf::from("revert-repaired.png");
        let mut core = Core::new();
        core.new_cell(2, 2, 144_000, 144_000).unwrap();
        core.current_path = Some(native_path.clone());
        core.io_pair_authority = Some(SavedPair {
            native_path: native_path.clone(),
            native: stamp(200, 100),
            raster_path: raster_path.clone(),
            raster: Some(stamp(201, 200)),
            raster_missing: None,
        });
        let (_, save_token) = core
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap();

        let before_document = core.document_info().unwrap();
        let before_history = core.history_entries().to_vec();
        let mut request = FileIoRequest::new(FileIoKind::OpenNative, vec![native_path.clone()]);
        request.force_reload = true;
        request.revert_current = true;
        let mut revert = FileIoJob::allocate(Some(&core), manager.clone(), request).unwrap();
        revert.open_token = Some(core.capture_document_open().unwrap());
        revert.ready = Some(Prepared::Open(
            Box::new(core.clone_for_staging()),
            None,
            Some(native_path.clone()),
        ));
        revert.progress.state = FileIoState::Ready;

        // Model the fixed-width authority publication performed by a failed
        // same-pair save after exact rollback. Document/editor/path state is
        // unchanged, but the restored filesystem observations are newer.
        let repaired = SavedPair {
            native_path,
            native: stamp(210, 100),
            raster_path,
            raster: Some(stamp(211, 200)),
            raster_missing: None,
        };
        core.io_pair_authority = Some(repaired.clone());

        assert_eq!(
            core.validate_document_save(&save_token),
            Err(CoreError::InvalidState("document file request is stale"))
        );
        assert!(matches!(
            revert.apply(&mut core),
            Err(CoreError::InvalidState("file job target is stale"))
        ));
        assert_eq!(core.document_info().unwrap(), before_document);
        assert_eq!(core.history_entries(), before_history);
        assert_eq!(core.io_pair_authority, Some(repaired));
        manager.shutdown_and_wait();
    }

    #[test]
    fn rolled_back_same_target_repairs_only_runtime_authority_and_item_identities() {
        let manager = IoManager::new(IoConfig::default()).unwrap();
        let native_path = PathBuf::from("authority.inkpod");
        let raster_path = PathBuf::from("authority.png");
        let mut core = Core::new();
        let document = core.new_cell(2, 2, 144_000, 144_000).unwrap();
        let old = SavedPair {
            native_path: native_path.clone(),
            native: stamp(10, 100),
            raster_path: raster_path.clone(),
            raster: Some(stamp(11, 200)),
            raster_missing: None,
        };
        core.io_pair_authority = Some(old.clone());
        let (_, token) = core
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap();
        let request = FileIoRequest::new(FileIoKind::SavePair, vec![native_path.clone()]);
        let mut job = FileIoJob::allocate(Some(&core), manager.clone(), request).unwrap();
        let future_native = stamp(20, 101);
        let future_raster = stamp(21, 201);
        job.items = vec![
            item(
                native_path.clone(),
                None,
                future_native.identity,
                document.document_uuid,
            ),
            item(
                raster_path.clone(),
                Some(crate::CommonRasterFormat::Png),
                future_raster.identity,
                document.document_uuid,
            ),
        ];
        job.pair_repair_target = PairRepairTarget::Committed(old);
        let restored_native = stamp(30, 100);
        let restored_raster = stamp(31, 200);
        job.prepare_pair_authority_repair(RestoredPair {
            native: Some(restored_native),
            raster: Some(restored_raster),
            native_missing: None,
            raster_missing: None,
        });
        assert!(job.progress.authority_repaired);
        assert_eq!(job.items[0].identity, restored_native.identity);
        assert_eq!(job.items[1].identity, restored_raster.identity);
        assert_eq!(job.items[0].name, "authority.inkpod");
        assert_eq!(job.items[1].format, Some(crate::CommonRasterFormat::Png));

        let before_document = core.document_info().unwrap();
        let before_history = core.history_entries().to_vec();
        let before_path = core.current_path.clone();
        let before_savepoint = core.savepoint;
        job.save_token = Some(token);
        job.pair_publication_started = true;
        job.error = Some(CoreError::FileConflict);
        job.progress.state = FileIoState::Ready;
        job.progress.installing = true;
        core.io_install_pending = true;

        let mut wrong_core = core.clone();
        assert!(matches!(
            job.apply(&mut wrong_core),
            Err(CoreError::InvalidState(_))
        ));
        assert!(job.error.is_some());
        assert!(job.progress.installing);
        assert!(core.io_install_pending);
        // Simulate a same-owner stamp divergence after the worker has already
        // rolled disk publication back. Failure finalization must still clear
        // the original fence and repair only runtime pair authority.
        core.persistence_state = core.persistence_state.next().unwrap();
        assert!(matches!(job.apply(&mut core), Err(CoreError::FileConflict)));
        assert_eq!(core.document_info().unwrap(), before_document);
        assert_eq!(core.history_entries(), before_history);
        assert_eq!(core.current_path, before_path);
        assert_eq!(core.savepoint, before_savepoint);
        let repaired = core.io_pair_authority.as_ref().unwrap();
        assert_eq!(repaired.native.identity, restored_native.identity);
        assert_eq!(repaired.raster.unwrap().identity, restored_raster.identity);
        assert!(core.io_pair_plan.is_none());
        assert!(!core.io_install_pending);
        assert!(job.progress.authority_repaired);
        assert!(!job.progress.authority_revoked);
        assert!(!job.progress.installing);
        assert!(!job.poll().authority_revoked);
        assert!(!core.sequence_source_recovery_required());
        manager.shutdown_and_wait();
    }

    #[test]
    fn native_first_raster_fault_preserves_both_savepoints_and_retries() {
        let manager = IoManager::new(IoConfig::default()).unwrap();
        let directory = manager
            .create_temporary_directory("core-pair-fault", &JobContext::new())
            .unwrap();
        let native_path = directory.path().join("cell.inkpod");
        let raster_path = directory.path().join("cell.png");
        let mut core = Core::new();
        core.new_cell(2, 2, 144_000, 144_000).unwrap();
        complete_pair_save(&mut core, &manager, &native_path);

        let original_native = std::fs::read(&native_path).unwrap();
        let original_raster = std::fs::read(&raster_path).unwrap();
        core.set_main_line_color(crate::PixelValue::Rgba([21, 42, 63, 255]))
            .unwrap();
        core.update_editor_state(
            core.editor_state().unwrap().revision,
            crate::EditorStateUpdate::SetActiveTool(crate::EditorTool::Eraser),
        )
        .unwrap();

        let before_document = core.document_info().unwrap();
        let before_digest = core.document_state_digest().unwrap();
        let before_history = core.history_entries().to_vec();
        let before_journal = core.journal_entries().to_vec();
        let before_journal_state = core.journal_state().unwrap();
        let before_editor = core.editor_state().unwrap();
        let before_editor_frame = core.editor_state_frame().unwrap();
        let before_document_savepoint = core.savepoint;
        let before_editor_savepoint = core.editor_session.as_ref().unwrap().savepoint;
        let before_path = core.current_path.clone();
        assert!(before_document.dirty);
        assert!(before_editor.dirty);
        assert_ne!(before_document_savepoint, Some(core.current_state));
        assert_ne!(before_editor_savepoint, Some(before_editor.digest));

        let mut failed = FileIoJob::start(
            Some(&core),
            manager.clone(),
            FileIoRequest::new(FileIoKind::SavePair, vec![native_path.clone()]),
        )
        .unwrap();
        // The I/O crate owns this semantic boundary. Core only selects the
        // typed, non-default test fault and otherwise follows the real job,
        // pair publication, rollback, and owner-finalization path.
        failed.pair_install_fault = Some(inkpod_io::PairInstallFault::AfterNativePublication);
        assert_eq!(
            wait_for_file_job(&mut failed).state,
            FileIoState::Ready,
            "{:?}",
            failed.error()
        );
        assert!(matches!(
            failed.apply(&mut core).unwrap(),
            FileIoApply::Pending
        ));
        let progress = wait_for_file_job(&mut failed);
        assert_eq!(progress.state, FileIoState::Ready, "{:?}", failed.error());
        assert!(failed.pair_publication_started);
        assert!(progress.authority_repaired);
        assert!(failed.apply(&mut core).is_err());
        assert!(!failed.requires_finalization());
        let progress = failed.poll();
        assert_eq!(progress.state, FileIoState::Failed);
        assert!(progress.authority_repaired);

        assert_eq!(core.document_info().unwrap(), before_document);
        assert_eq!(core.document_state_digest().unwrap(), before_digest);
        assert_eq!(core.history_entries(), before_history);
        assert_eq!(core.journal_entries(), before_journal);
        assert_eq!(core.journal_state(), Some(before_journal_state));
        assert_eq!(core.editor_state().unwrap(), before_editor);
        assert_eq!(core.editor_state_frame().unwrap(), before_editor_frame);
        assert_eq!(core.savepoint, before_document_savepoint);
        assert_eq!(
            core.editor_session.as_ref().unwrap().savepoint,
            before_editor_savepoint
        );
        assert_eq!(core.current_path, before_path);
        assert_eq!(std::fs::read(&native_path).unwrap(), original_native);
        assert_eq!(std::fs::read(&raster_path).unwrap(), original_raster);

        complete_pair_save(&mut core, &manager, &native_path);
        assert!(!core.document_info().unwrap().dirty);
        assert!(!core.editor_state().unwrap().dirty);
        assert_eq!(core.document_state_digest().unwrap(), before_digest);
        assert_eq!(core.history_entries(), before_history);
        assert_eq!(core.journal_entries(), before_journal);
        assert_eq!(core.editor_state_frame().unwrap(), before_editor_frame);
        assert_eq!(core.savepoint, Some(core.current_state));
        assert_eq!(
            core.editor_session.as_ref().unwrap().savepoint,
            Some(before_editor.digest)
        );
        assert_ne!(std::fs::read(&native_path).unwrap(), original_native);

        let disk_raster = inkpod_format::decode_common_raster(
            crate::CommonRasterFormat::Png,
            &std::fs::read(&raster_path).unwrap(),
        )
        .unwrap();
        let expected_raster = inkpod_format::decode_common_raster(
            crate::CommonRasterFormat::Png,
            &core
                .export_common_raster(crate::CommonRasterFormat::Png, false)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(disk_raster, expected_raster);

        let mut reopened = Core::new();
        reopened.open(&native_path).unwrap();
        assert_eq!(reopened.document_state_digest().unwrap(), before_digest);
        assert_eq!(reopened.history_entries(), before_history);
        assert_eq!(reopened.journal_entries(), before_journal);
        assert_eq!(reopened.editor_state_frame().unwrap(), before_editor_frame);
        assert!(!reopened.document_info().unwrap().dirty);
        assert!(!reopened.editor_state().unwrap().dirty);
        directory.cleanup().unwrap();
        manager.shutdown_and_wait();
    }

    #[test]
    fn cancelling_repaired_final_ready_revokes_unpublishable_authority() {
        let manager = IoManager::new(IoConfig::default()).unwrap();
        let native_path = PathBuf::from("cancel-repair.inkpod");
        let raster_path = PathBuf::from("cancel-repair.png");
        let mut core = Core::new();
        let document = core.new_cell(2, 2, 144_000, 144_000).unwrap();
        let old = SavedPair {
            native_path: native_path.clone(),
            native: stamp(110, 100),
            raster_path: raster_path.clone(),
            raster: Some(stamp(111, 200)),
            raster_missing: None,
        };
        core.current_path = Some(native_path.clone());
        core.io_pair_authority = Some(old.clone());
        let (_, token) = core
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap();
        let request = FileIoRequest::new(FileIoKind::SavePair, vec![native_path.clone()]);
        let mut job = FileIoJob::allocate(Some(&core), manager.clone(), request).unwrap();
        job.items = vec![
            item(
                native_path.clone(),
                None,
                stamp(120, 101).identity,
                document.document_uuid,
            ),
            item(
                raster_path.clone(),
                Some(crate::CommonRasterFormat::Png),
                stamp(121, 201).identity,
                document.document_uuid,
            ),
        ];
        job.pair_repair_target = PairRepairTarget::Committed(old);
        job.prepare_pair_authority_repair(RestoredPair {
            native: Some(stamp(130, 100)),
            raster: Some(stamp(131, 200)),
            native_missing: None,
            raster_missing: None,
        });
        job.save_token = Some(token);
        job.pair_publication_started = true;
        job.error = Some(CoreError::FileConflict);
        job.progress.state = FileIoState::Ready;
        job.progress.installing = true;
        core.io_install_pending = true;
        assert!(job.progress.authority_repaired);

        job.cancel();
        assert!(!job.progress.authority_repaired);
        assert!(job.pair_authority_repair.is_none());
        assert!(matches!(&job.pair_repair_target, PairRepairTarget::Revoke));
        assert_eq!(job.error(), Some(&CoreError::FileConflict));
        assert!(matches!(job.apply(&mut core), Err(CoreError::FileConflict)));
        assert!(core.io_pair_authority.is_none());
        assert!(core.io_pair_plan.is_none());
        assert!(core.current_path.is_none());
        assert!(core.document_info().unwrap().dirty);
        assert!(core.editor_state().unwrap().dirty);
        assert!(job.progress.authority_revoked);
        assert!(!job.progress.authority_repaired);
        assert!(!job.progress.installing);
        assert!(job.poll().authority_revoked);
        assert!(core.sequence_source_recovery_required());
        manager.shutdown_and_wait();
    }

    #[test]
    fn planned_rollback_restores_missing_native_identity_without_changing_metadata() {
        let manager = IoManager::new(IoConfig::default()).unwrap();
        let native_path = PathBuf::from("planned.inkpod");
        let raster_path = PathBuf::from("planned.png");
        let mut core = Core::new();
        let document = core.new_cell(1, 1, 96_000, 96_000).unwrap();
        let missing = FileIdentity {
            volume: u64::MAX,
            file: 99,
        };
        let planned = PlannedPair {
            native_path: native_path.clone(),
            native_missing: missing,
            raster_path: raster_path.clone(),
            raster: stamp(40, 50),
        };
        core.io_pair_plan = Some(planned.clone());
        let (_, token) = core
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap();
        let request = FileIoRequest::new(FileIoKind::SavePair, vec![native_path.clone()]);
        let mut job = FileIoJob::allocate(Some(&core), manager.clone(), request).unwrap();
        job.items = vec![
            item(
                native_path,
                None,
                stamp(50, 60).identity,
                document.document_uuid,
            ),
            item(
                raster_path,
                Some(crate::CommonRasterFormat::Png),
                stamp(51, 70).identity,
                document.document_uuid,
            ),
        ];
        job.pair_repair_target = PairRepairTarget::Planned(planned);
        let restored_raster = stamp(60, 50);
        job.prepare_pair_authority_repair(RestoredPair {
            native: None,
            raster: Some(restored_raster),
            native_missing: Some(missing),
            raster_missing: None,
        });
        assert!(job.progress.authority_repaired);
        assert_eq!(job.items[0].identity, missing);
        assert!(!job.items[0].identity_physical);
        assert_eq!(job.items[1].identity, restored_raster.identity);
        assert!(job.items[1].identity_physical);
        let Some(PairAuthorityRepair::Planned(repaired)) = job.pair_authority_repair.as_ref()
        else {
            panic!("planned repair candidate is missing");
        };
        assert_eq!(repaired.native_missing, missing);
        assert_eq!(repaired.raster, restored_raster);
        job.save_token = Some(token);
        job.pair_publication_started = true;
        job.error = Some(CoreError::FileConflict);
        job.progress.state = FileIoState::Ready;
        job.progress.installing = true;
        core.io_install_pending = true;
        assert!(matches!(job.apply(&mut core), Err(CoreError::FileConflict)));
        assert_eq!(core.document_info().unwrap(), document);
        let repaired = core.io_pair_plan.as_ref().unwrap();
        assert_eq!(repaired.native_missing, missing);
        assert_eq!(repaired.raster, restored_raster);
        assert!(core.io_pair_authority.is_none());
        assert!(job.progress.authority_repaired);
        assert!(!job.progress.authority_revoked);
        assert!(!job.progress.installing);
        assert!(!job.poll().authority_revoked);
        assert!(!core.sequence_source_recovery_required());
        manager.shutdown_and_wait();
    }

    #[test]
    fn post_publication_revoke_is_terminal_and_distinct_from_other_failures() {
        let make_core_and_authority = || {
            let mut core = Core::new();
            core.new_cell(2, 2, 144_000, 144_000).unwrap();
            let saved = SavedPair {
                native_path: PathBuf::from("authority.inkpod"),
                native: stamp(70, 100),
                raster_path: PathBuf::from("authority.png"),
                raster: Some(stamp(71, 200)),
                raster_missing: None,
            };
            core.current_path = Some(saved.native_path.clone());
            core.io_pair_authority = Some(saved);
            core.io_pair_plan = None;
            core
        };
        let finalize = |core: &mut Core,
                        manager: &IoManager,
                        target: PairRepairTarget,
                        publication_started: bool| {
            let (_, token) = core
                .capture_document_save()
                .unwrap()
                .prepare_native_save(false, || false)
                .unwrap();
            let mut job = FileIoJob::allocate(
                Some(core),
                manager.clone(),
                FileIoRequest::new(
                    FileIoKind::SavePair,
                    vec![PathBuf::from("authority.inkpod")],
                ),
            )
            .unwrap();
            job.save_token = Some(token);
            job.pair_repair_target = target;
            job.pair_publication_started = publication_started;
            job.error = Some(CoreError::FileConflict);
            job.progress.state = FileIoState::Ready;
            job.progress.installing = true;
            core.io_install_pending = true;
            assert!(matches!(job.apply(core), Err(CoreError::FileConflict)));
            job
        };

        let manager = IoManager::new(IoConfig::default()).unwrap();

        let mut revoked_core = make_core_and_authority();
        let before_document = revoked_core.document_info().unwrap();
        let before_history = revoked_core.history_entries().to_vec();
        let before_journal = revoked_core.journal_entries().to_vec();
        let mut revoked = finalize(&mut revoked_core, &manager, PairRepairTarget::Revoke, true);
        assert!(revoked_core.io_pair_authority.is_none());
        assert!(revoked_core.io_pair_plan.is_none());
        assert!(revoked_core.current_path.is_none());
        assert!(revoked_core.document_info().unwrap().dirty);
        assert!(revoked_core.editor_state().unwrap().dirty);
        assert_eq!(revoked_core.history_entries(), before_history);
        assert_eq!(revoked_core.journal_entries(), before_journal);
        assert_eq!(
            revoked_core.document_info().unwrap().document_revision,
            before_document.document_revision
        );
        assert_eq!(
            revoked_core.revert(),
            Err(CoreError::InvalidState("document has no normal-save path"))
        );
        assert!(revoked.progress.authority_revoked);
        assert!(!revoked.progress.authority_repaired);
        assert_eq!(revoked.progress.state, FileIoState::Failed);
        assert!(!revoked.progress.installing);
        assert!(revoked.poll().authority_revoked);
        assert!(revoked_core.sequence_source_recovery_required());

        let uuid = revoked_core.document_info().unwrap().document_uuid;
        let raster = crate::CommonRaster::new(
            2,
            2,
            crate::PixelFormat::StraightRgba8,
            Some(144_000),
            Some(144_000),
            [0, 0, 0, 0].repeat(4),
        )
        .unwrap();
        let first =
            crate::SequenceCellSource::from_common_raster("authority1.png", uuid, &raster).unwrap();
        let second =
            crate::SequenceCellSource::from_common_raster("authority2.png", uuid + 1, &raster)
                .unwrap();
        revoked_core.set_sequence(vec![first, second]).unwrap();
        let request = revoked_core
            .sequence_switch_request(1, crate::SequenceSwitchPolicy::AutosaveBeforeSwitch)
            .unwrap();
        assert!(matches!(
            FileIoJob::start_sequence_switch(
                &revoked_core,
                manager.clone(),
                request,
                None,
                None,
                None,
            ),
            Err(CoreError::InvalidArgument(
                "sequence source requires a recovery destination"
            ))
        ));

        let (_, save_as_token) = revoked_core
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap();
        let replacement_path = PathBuf::from("replacement.inkpod");
        revoked_core
            .commit_document_save(save_as_token, &replacement_path)
            .unwrap();
        revoked_core.io_pair_authority = Some(SavedPair {
            native_path: replacement_path.clone(),
            native: stamp(80, 300),
            raster_path: replacement_path.with_extension("png"),
            raster: Some(stamp(81, 400)),
            raster_missing: None,
        });
        assert_eq!(revoked_core.current_path, Some(replacement_path));
        assert!(!revoked_core.document_info().unwrap().dirty);
        assert!(!revoked_core.editor_state().unwrap().dirty);
        assert!(!revoked_core.sequence_source_recovery_required());

        let mut prepublication_core = make_core_and_authority();
        let mut prepublication = finalize(
            &mut prepublication_core,
            &manager,
            PairRepairTarget::Revoke,
            false,
        );
        assert!(prepublication_core.io_pair_authority.is_some());
        assert_eq!(
            prepublication_core.current_path,
            Some(PathBuf::from("authority.inkpod"))
        );
        assert!(!prepublication_core.document_info().unwrap().dirty);
        assert!(!prepublication_core.sequence_source_recovery_required());
        assert!(!prepublication.progress.authority_revoked);
        assert!(!prepublication.poll().authority_revoked);

        let mut unrelated_core = make_core_and_authority();
        let mut unrelated = finalize(
            &mut unrelated_core,
            &manager,
            PairRepairTarget::Unrelated,
            true,
        );
        assert!(unrelated_core.io_pair_authority.is_some());
        assert_eq!(
            unrelated_core.current_path,
            Some(PathBuf::from("authority.inkpod"))
        );
        assert!(!unrelated_core.document_info().unwrap().dirty);
        assert!(!unrelated_core.sequence_source_recovery_required());
        assert!(!unrelated.progress.authority_revoked);
        assert!(!unrelated.poll().authority_revoked);

        manager.shutdown_and_wait();
    }
}
