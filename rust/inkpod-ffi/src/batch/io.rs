use super::*;
use crate::file_io::{empty_owner, io_boundary, job_lock, manager_ref, owner_core};
use inkpod_core::{FileIoJob, FileIoKind};
use std::sync::Mutex;

/// Copies a Batch graph into a detached, asynchronous path-only file job.
/// # Safety
/// Handles are live, Core is exclusively accessed on its owner thread, and output is empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_io_batch_submit(
    core: *mut InkpodCore,
    manager: *mut InkpodIoManager,
    graph: *const InkpodBatchGraph,
    kind: u32,
    run_scope: u32,
    flags: u64,
    new_tab_capacity: u64,
    out_job: *mut *mut InkpodIoJob,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller supplies writable readable owner storage.
        unsafe { empty_owner(out_job)? };
        if graph.is_null() || !is_aligned(graph) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid Batch graph handle",
            ));
        }
        if flags & !(INKPOD_BATCH_RUN_DRY | INKPOD_BATCH_RUN_PREVIEW_CONFIRMED) != 0 {
            return Err(fail(INKPOD_STATUS_UNSUPPORTED, "unknown Batch job flags"));
        }
        let kind = match kind {
            INKPOD_IO_BATCH_PLAN => FileIoKind::BatchPlan,
            INKPOD_IO_BATCH_RUN => FileIoKind::BatchRun,
            INKPOD_IO_BATCH_PREVIEW => FileIoKind::BatchPreview,
            _ => {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "unknown Batch job kind",
                ));
            }
        };
        let options = BatchRunOptions {
            scope: scope(run_scope)?,
            dry_run: flags & INKPOD_BATCH_RUN_DRY != 0,
            preview_confirmed: flags & INKPOD_BATCH_RUN_PREVIEW_CONFIRMED != 0,
        };
        let capacity = usize::try_from(new_tab_capacity).map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "Batch session capacity overflows",
            )
        })?;
        // SAFETY: Graph is immutable/live; Core owner-thread and manager validity are checked.
        let job = FileIoJob::start_batch(
            &unsafe { owner_core(core)? }.core,
            unsafe { manager_ref(manager)? }.clone(),
            unsafe { &*graph }.graph.clone(),
            kind,
            options,
            capacity,
        )
        .map_err(map_core_error)?;
        // SAFETY: The validated empty output receives ownership exactly once.
        unsafe {
            out_job.write(Box::into_raw(Box::new(InkpodIoJob {
                job: Mutex::new(job),
                owner_thread: thread::current().id(),
            })))
        };
        Ok(INKPOD_STATUS_OK)
    })
}

// SAFETY: Live job handle supplied by the caller; release cannot race this check.
unsafe fn validate_owner(job: *const InkpodIoJob) -> Result<(), u32> {
    if job.is_null() || !is_aligned(job) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "invalid Batch I/O job",
        ));
    }
    // SAFETY: The handle's immutable owner affinity is fixed at submission.
    if unsafe { &*job }.owner_thread != thread::current().id() {
        return Err(fail(
            INKPOD_STATUS_WRONG_THREAD,
            "Batch results must transfer on the job owner thread",
        ));
    }
    Ok(())
}

/// Transfers a completed plan into the existing immutable Batch preview ABI.
/// # Safety
/// Job is live on its owner thread and output is an empty writable owner slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_take_batch_preview(
    job: *mut InkpodIoJob,
    out_preview: *mut *mut InkpodBatchPreview,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Public caller supplies live handle and output owner range.
        unsafe {
            empty_owner(out_preview)?;
            validate_owner(job)?;
        }
        // SAFETY: The one-shot result is synchronized by the job mutex.
        let preview = unsafe { job_lock(job)? }
            .take_batch_preview()
            .map_err(map_core_error)?;
        // SAFETY: Ownership transfers to the prevalidated empty pointer slot.
        unsafe { out_preview.write(Box::into_raw(preview_handle(preview))) };
        Ok(INKPOD_STATUS_OK)
    })
}

/// Transfers a completed run/contact-sheet report, including owned staged tabs.
/// # Safety
/// Job is live on its owner thread and output is an empty writable owner slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_take_batch_report(
    job: *mut InkpodIoJob,
    out_report: *mut *mut InkpodBatchReport,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Both pointer owners are valid as documented by the public ABI.
        unsafe {
            empty_owner(out_report)?;
            validate_owner(job)?;
        }
        // SAFETY: The job owns and synchronizes its one-shot result.
        let report = unsafe { job_lock(job)? }
            .take_batch_report()
            .map_err(map_core_error)?;
        // SAFETY: Ownership transfers once to the prevalidated empty slot.
        unsafe { out_report.write(Box::into_raw(report_handle(report))) };
        Ok(INKPOD_STATUS_OK)
    })
}

pub(super) fn preview_handle(preview: inkpod_core::BatchPreview) -> Box<InkpodBatchPreview> {
    Box::new(InkpodBatchPreview {
        items: preview
            .items
            .into_iter()
            .map(|item| OwnedPreviewItem {
                input_name: item.input_name.into_bytes().into_boxed_slice(),
                output_path: bytes_for_path(item.output_path),
                warning: item.warnings.join("\n").into_bytes().into_boxed_slice(),
            })
            .collect(),
    })
}

pub(super) fn report_handle(report: BatchRunReport) -> Box<InkpodBatchReport> {
    Box::new(InkpodBatchReport {
        cancelled: report.cancelled,
        owner_thread: thread::current().id(),
        staged_results: report.staged_results.into_iter().map(Some).collect(),
        items: report
            .items
            .into_iter()
            .map(|item| OwnedReportItem {
                outcome: match item.outcome {
                    BatchItemOutcome::Succeeded => INKPOD_BATCH_ITEM_SUCCEEDED,
                    BatchItemOutcome::Skipped => INKPOD_BATCH_ITEM_SKIPPED,
                    BatchItemOutcome::Failed => INKPOD_BATCH_ITEM_FAILED,
                    BatchItemOutcome::Cancelled => INKPOD_BATCH_ITEM_CANCELLED,
                    BatchItemOutcome::DryRun => INKPOD_BATCH_ITEM_DRY_RUN,
                },
                input_name: item.input_name.into_bytes().into_boxed_slice(),
                output_path: bytes_for_path(item.output_path),
                message: item.message.into_bytes().into_boxed_slice(),
            })
            .collect(),
    })
}
