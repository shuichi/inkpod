use super::job::{FileIoJob, Pending, Prepared};
use super::model::{FileIoKind, FileIoRequest, FileIoState};
use crate::{
    BatchGraph, BatchOutputDestination, BatchPreview, BatchRunOptions, BatchRunReport,
    BatchRunScope, Core, CoreError,
};
use inkpod_io::IoManager;

pub(super) struct BatchWork {
    pub core: Box<Core>,
    pub graph: BatchGraph,
    pub options: BatchRunOptions,
    pub capacity: usize,
    pub kind: FileIoKind,
}

pub(super) struct BatchPrepared {
    pub active: Option<Box<Core>>,
    pub preview: Option<BatchPreview>,
    pub report: Option<BatchRunReport>,
}

impl FileIoJob {
    /// Captures a Batch graph and immutable issue-time Core state, then expands
    /// paths and validates images in workers. Raster prefetch shares the global
    /// capped cache; ordered operations retain existing Stop/Continue behavior.
    /// No filesystem preflight or processing occurs on the issuing owner thread.
    pub fn start_batch(
        core: &Core,
        manager: IoManager,
        graph: BatchGraph,
        kind: FileIoKind,
        options: BatchRunOptions,
        new_tab_capacity: usize,
    ) -> Result<Self, CoreError> {
        if !matches!(
            kind,
            FileIoKind::BatchPlan | FileIoKind::BatchRun | FileIoKind::BatchPreview
        ) {
            return Err(CoreError::InvalidArgument("invalid Batch I/O job kind"));
        }
        core.ensure_no_active_stroke()?;
        graph.validate()?;
        let mut frozen = core.clone_for_staging();
        frozen.bind_file_io(manager.clone())?;
        frozen.render_cache.clear();
        let mut work = BatchWork {
            core: Box::new(frozen),
            graph,
            options,
            capacity: new_tab_capacity,
            kind,
        };
        let mut job = Self::allocate(
            Some(core),
            manager.clone(),
            FileIoRequest::new(kind, Vec::new()),
        )?;
        job.pending = Some(Pending::BatchDiscover(manager.clone().submit(
            move |context| {
                let result = (|| {
                    let paths = work.core.batch_freeze_inputs(
                        &mut work.graph,
                        options.scope,
                        &manager,
                        &context,
                    )?;
                    work.options.scope = BatchRunScope::All;
                    Ok((work, paths))
                })();
                Ok(result)
            },
        )?));
        Ok(job)
    }

    pub(super) fn execute_batch(&mut self, mut work: BatchWork) -> Result<(), CoreError> {
        self.pending = Some(Pending::Prepare(self.manager.submit(move |context| {
            let result = (|| {
                context.check_cancelled()?;
                let mut prepared = BatchPrepared {
                    active: None,
                    preview: None,
                    report: None,
                };
                match work.kind {
                    FileIoKind::BatchPlan => {
                        prepared.preview = Some(work.core.batch_preview_with_context(
                            &work.graph,
                            work.options.scope,
                            &context,
                        )?);
                    }
                    FileIoKind::BatchPreview => {
                        prepared.report =
                            Some(work.core.batch_contact_sheet_preview_with_context(
                                &work.graph,
                                &context,
                                |completed, total| {
                                    context.set_work(completed, total);
                                    !context.is_cancelled()
                                },
                            )?);
                    }
                    FileIoKind::BatchRun => {
                        let report = work.core.batch_execute_with_context(
                            &work.graph,
                            work.options,
                            work.capacity,
                            &context,
                            |completed, total| {
                                context.set_work(completed, total);
                                !context.is_cancelled()
                            },
                        )?;
                        if work.graph.output.destination == BatchOutputDestination::ActiveDocument
                            && !work.options.dry_run
                            && !report.cancelled
                            && report.failure_count() == 0
                        {
                            prepared.active = Some(work.core);
                        }
                        prepared.report = Some(report);
                    }
                    _ => return Err(CoreError::InvalidArgument("invalid Batch worker kind")),
                }
                Ok((Prepared::Batch(Box::new(prepared)), Vec::new()))
            })();
            Ok(result)
        })?));
        Ok(())
    }

    /// Transfers a completed immutable Batch preflight result exactly once.
    pub fn take_batch_preview(&mut self) -> Result<BatchPreview, CoreError> {
        if self.progress.state != FileIoState::Complete {
            return Err(CoreError::InvalidState("Batch result is not applied"));
        }
        self.batch_preview.take().ok_or(CoreError::InvalidState(
            "Batch preview is absent or already taken",
        ))
    }

    /// Transfers a completed Batch report and its staged new-tab results once.
    /// Folder output success is not undone by releasing this report.
    pub fn take_batch_report(&mut self) -> Result<BatchRunReport, CoreError> {
        if self.progress.state != FileIoState::Complete {
            return Err(CoreError::InvalidState("Batch result is not applied"));
        }
        self.batch_report.take().ok_or(CoreError::InvalidState(
            "Batch report is absent or already taken",
        ))
    }
}
