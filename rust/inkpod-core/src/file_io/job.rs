use super::batch::{BatchPrepared, BatchWork};
use super::model::*;
use super::prepare;
use crate::persistence_task::PersistenceState;
use crate::{
    Core, CoreError, DocumentOpenToken, DocumentRevision, DocumentSaveToken, EditorRevision,
    LightTableItemInfo, LightTableItemInput, SequenceCellSource, StateId,
};
use inkpod_io::{ImageBatch, IoJob, IoManager, JobProgress, JobState, LoadedImage, PreparedPair};
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
        }
    }

    pub(super) fn validate(&self, core: &Core, sequence_only: bool) -> Result<(), CoreError> {
        let current = Self::capture(core);
        if self.authority != current.authority
            || self.uuid != current.uuid
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
    Open(Box<Core>, Option<inkpod_io::RecoveryCandidate>),
    Sequence(Vec<SequenceCellSource>),
    References(Vec<LoadedImage>),
    LightTable(LightTableItemInput),
    Pair(Box<PreparedPair>, DocumentSaveToken),
    Output,
    Batch(Box<BatchPrepared>),
    Recovery(Vec<inkpod_io::RecoveryCandidate>),
    CutDescriptor,
    SequenceSwitch(Box<crate::PreparedSequenceSwitch>),
    NativeOutput(inkpod_format::NativeFile, DocumentSaveToken),
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
    Install(IoJob<Result<Option<SavedPair>, CoreError>>),
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
    pub(super) sequence_install: Option<Box<crate::PreparedSequenceSwitch>>,
    pub(super) progress: FileIoProgress,
    pub(super) error: Option<CoreError>,
    pub(super) items: Vec<FileIoItem>,
    pub(super) pending: Option<Pending>,
    pub(super) batch_report: Option<crate::BatchRunReport>,
    pub(super) batch_preview: Option<crate::BatchPreview>,
    pub(super) recoveries: Vec<inkpod_io::RecoveryCandidate>,
    batch_prefetch_reads: u64,
    images: Vec<Option<LoadedImage>>,
    image_error: Option<CoreError>,
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
        request: FileIoRequest,
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
        let open_token = if matches!(
            request.kind,
            FileIoKind::OpenNative | FileIoKind::OpenRecovery | FileIoKind::OpenRaster
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
                job.pending = Some(Pending::Prepare(job.manager.submit(move |context| {
                    Ok(prepare::save(
                        &manager, &request, snapshot, expected, &context,
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
            sequence_install: None,
            pending: None,
            images: Vec::new(),
            image_error: None,
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
                cut_descriptor: false,
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
                    self.pending = Some(Pending::Prepare(self.manager.submit(move |context| {
                        Ok(prepare::images(
                            &manager, &request, images, seed, seed_uuid, reload, &context,
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
                    let (prepared, items) = result??;
                    // Explicit writes have their own filesystem commit point.
                    if self.cancel_requested && !matches!(prepared, Prepared::Output) {
                        return Err(CoreError::Cancelled);
                    }
                    self.items = items;
                    if let Prepared::Open(_, Some(candidate)) = &prepared {
                        self.recoveries = vec![candidate.clone()];
                    }
                    if let Prepared::Recovery(recoveries) = prepared {
                        self.recoveries = recoveries;
                        self.progress.state = FileIoState::Complete;
                    } else if matches!(prepared, Prepared::CutDescriptor) {
                        self.progress.cut_descriptor = true;
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
                        Ok(pair) => self.installed = pair,
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
    pub fn cancel(&mut self) {
        self.cancel_requested = true;
        match &self.pending {
            Some(Pending::Discover(job)) => job.cancel(),
            Some(Pending::Images(job)) => job.cancel(),
            Some(Pending::Prepare(job)) => job.cancel(),
            Some(Pending::Install(job)) => job.cancel(),
            Some(Pending::BatchDiscover(job)) => job.cancel(),
            Some(Pending::BatchImages(job, _)) => job.cancel(),
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
    ) -> Result<(), CoreError> {
        let native_path = self.request.paths[0].clone();
        let job = self.manager.submit(move |context| {
            Ok(pair
                .install_with_stamps(&context)
                .map(|(native, raster)| {
                    Some(SavedPair {
                        native_path,
                        native,
                        raster: Some(raster),
                    })
                })
                .map_err(CoreError::from))
        })?;
        self.save_token = Some(token);
        self.progress.installing = true;
        self.progress.state = FileIoState::Running;
        self.pending = Some(Pending::Install(job));
        Ok(())
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
