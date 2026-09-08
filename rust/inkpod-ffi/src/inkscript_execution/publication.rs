//! Owned staged publication and image-preview execution through the shared Rust adapter.
use super::*;
use crate::file_io::{empty_owner, io_boundary};
use inkpod_core::inkscript::{
    ScriptImagePreviewError, ScriptImagePreviewLimits, ScriptStagedResult, ScriptStagedResultKind,
    preview_inkscript_images,
};
pub struct InkpodInkScriptStagedResult {
    owner_thread: ThreadId,
    core_generation: u64,
    payload: StagedPayload,
    guard: PublicationGuard,
    cancelled: std::sync::Arc<AtomicBool>,
}

pub(super) struct PreviewWork {
    pub program: StaticScriptProgram,
    pub plan: ScriptExecutionPlan,
    pub confirmation: ScriptConfirmationToken,
    pub maximum_bytes: u64,
}

pub(super) enum StagedPayload {
    Document(Box<ScriptStagedResult>),
    Preview(Box<Core>),
}

#[derive(Clone)]
pub(super) struct PublicationGuard {
    adapter: ScriptIoAdapter,
    authority_generation: u64,
    sessions_generation: u64,
    origin: Option<(u64, u64, u64)>,
}

impl PublicationGuard {
    pub(super) fn new(
        mut adapter: ScriptIoAdapter,
        origin: &ScriptCommandContext,
    ) -> Result<Self, u32> {
        Ok(Self {
            authority_generation: ScriptRunAdapter::authority_generation(&mut adapter)
                .map_err(|_| INKPOD_STATUS_INVALID_STATE)?,
            sessions_generation: ScriptRunAdapter::open_session_set_generation(&mut adapter)
                .map_err(|_| INKPOD_STATUS_INVALID_STATE)?,
            origin: origin.current_session_identity(),
            adapter,
        })
    }
    fn validate(&mut self) -> Result<(), u32> {
        if ScriptRunAdapter::authority_generation(&mut self.adapter)
            .map_err(|_| INKPOD_STATUS_INVALID_STATE)?
            != self.authority_generation
            || ScriptRunAdapter::open_session_set_generation(&mut self.adapter)
                .map_err(|_| INKPOD_STATUS_INVALID_STATE)?
                != self.sessions_generation
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript staged publication authority is stale",
            ));
        }
        if let Some((id, generation, source)) = self.origin {
            if !self
                .adapter
                .session_is_current(id, generation, source)
                .map_err(|_| INKPOD_STATUS_INVALID_STATE)?
            {
                return Err(fail(
                    INKPOD_STATUS_INVALID_STATE,
                    "InkScript staged publication session is stale",
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn advance_preview(task: &InkpodInkScriptRunTask, data: &mut RunTaskOwnerData) -> u32 {
    let Some(mut work) = data.preview.take() else {
        return INKPOD_STATUS_INVALID_STATE;
    };
    let Some(shared) = &data.shared_io else {
        return INKPOD_STATUS_INVALID_STATE;
    };
    let manager = shared.manager().clone();
    let mut limits = ScriptImagePreviewLimits::exact_current();
    if work.maximum_bytes != 0 {
        limits = limits.with_temporary_bytes(work.maximum_bytes);
    }
    let result = preview_inkscript_images(
        &work.program,
        &work.plan,
        &mut work.confirmation,
        &manager,
        data.adapter.as_mut(),
        limits,
        |completed, total| {
            task.completed_work.store(completed, Ordering::Release);
            task.total_work.store(total, Ordering::Release);
            !task.cancelled.load(Ordering::Acquire)
        },
    );
    let status = match result {
        Ok(preview) => {
            data.report = Some(preview.report().clone());
            let (core, origin) = preview.into_display();
            data.origin = origin;
            data.results = Some(vec![Some(StagedPayload::Preview(Box::new(core)))]);
            data.result_count = 1;
            INKPOD_STATUS_OK
        }
        Err(ScriptImagePreviewError::Cancelled) => INKPOD_STATUS_CANCELLED,
        Err(ScriptImagePreviewError::ResourceLimit) => INKPOD_STATUS_INVALID_ARGUMENT,
        Err(error) => fail(
            INKPOD_STATUS_INVALID_STATE,
            &format!("InkScript image preview failed: {error:?}"),
        ),
    };
    let state = match status {
        INKPOD_STATUS_OK => INKPOD_TASK_COMPLETED,
        INKPOD_STATUS_CANCELLED => INKPOD_TASK_CANCELLED,
        _ => INKPOD_TASK_FAILED,
    };
    data.terminal_status = status;
    task.state.store(state, Ordering::Release);
    data.pending_event = Some(TaskEventData {
        kind: INKPOD_INKSCRIPT_EVENT_RUN_COMPLETE,
        task_state: state,
        ordinal: 0,
        completed: task.completed_work.load(Ordering::Acquire),
        total: task.total_work.load(Ordering::Acquire),
        wait_milliseconds: 0,
        outcome: 0,
        failure: 0,
    });
    status
}

fn task_data(
    core: &InkpodCore,
    pointer: *mut InkpodInkScriptRunTask,
) -> Result<
    (
        &'static InkpodInkScriptRunTask,
        &'static mut RunTaskOwnerData,
    ),
    u32,
> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(INKPOD_STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: Task is live and release is excluded by the caller.
    let task = unsafe { &*pointer };
    validate_route(task.owner_thread, task.core_generation, core)?;
    if !matches!(
        task.state.load(Ordering::Acquire),
        INKPOD_TASK_COMPLETED | INKPOD_TASK_CANCELLED
    ) {
        return Err(INKPOD_STATUS_INVALID_STATE);
    }
    // SAFETY: Only the validated owner thread accesses owner_data; query/cancel touch atomics.
    Ok((task, unsafe { &mut *task.owner_data.get() }))
}

/// Returns completed publication slots; dry-run has none. Taking a slot does not renumber others.
/// # Safety
/// Core/task match their live owner thread; output points to writable non-overlapping u64 storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_run_task_result_count(
    core: *mut InkpodCore,
    task: *mut InkpodInkScriptRunTask,
    output: *mut u64,
) -> u32 {
    io_boundary(|| {
        if output.is_null() || !is_aligned(output) {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        let core = validate_execution_core(core)?;
        let (_, data) = task_data(core, task)?;
        let count = data.result_count;
        // SAFETY: Caller supplies validated writable output storage.
        unsafe {
            output.write(count);
        }
        Ok(INKPOD_STATUS_OK)
    })
}

/// Transfers one owner after runtime authority validation; a stale route publishes nothing.
/// # Safety
/// Core/task are matching live owner-thread handles. Outputs are initialized and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_run_task_take_staged_result(
    core: *mut InkpodCore,
    task: *mut InkpodInkScriptRunTask,
    index: u64,
    output: *mut *mut InkpodInkScriptStagedResult,
    info: *mut InkpodInkScriptStagedInfo,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller supplies empty owner storage and a size-prefixed output DTO.
        unsafe {
            empty_owner(output)?;
            validate_struct(info.cast_const(), "InkpodInkScriptStagedInfo")?;
        }
        // SAFETY: The complete output DTO is now readable and writable.
        let info = unsafe { &mut *info };
        if info.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return Err(INKPOD_STATUS_INCOMPATIBLE_ABI);
        }
        if info.feature_flags != 0 || info.reserved != 0 {
            return Err(INKPOD_STATUS_UNSUPPORTED);
        }
        let core = validate_execution_core(core)?;
        let (task, data) = task_data(core, task)?;
        if let Some(inner) = &mut data.task {
            let results = inner
                .take_staged_results(data.adapter.as_mut())
                .map_err(|_| INKPOD_STATUS_INVALID_STATE)?;
            if data.results.is_none() {
                data.results = Some(
                    results
                        .into_iter()
                        .map(|result| Some(StagedPayload::Document(Box::new(result))))
                        .collect(),
                );
            }
        }
        let mut guard = data
            .publication_guard
            .clone()
            .ok_or(INKPOD_STATUS_INVALID_STATE)?;
        guard.validate()?;
        let index = usize::try_from(index).map_err(|_| INKPOD_STATUS_INVALID_ARGUMENT)?;
        let slot = data
            .results
            .as_mut()
            .and_then(|results| results.get_mut(index))
            .ok_or(INKPOD_STATUS_INVALID_ARGUMENT)?;
        let cancellation_blocks = match slot.as_ref() {
            Some(StagedPayload::Preview(_)) => true,
            Some(StagedPayload::Document(value)) => {
                value.kind() == ScriptStagedResultKind::ActiveDocument
            }
            None => false,
        };
        if task.cancelled.load(Ordering::Acquire) && cancellation_blocks {
            return Err(INKPOD_STATUS_CANCELLED);
        }
        let payload = slot.take().ok_or(INKPOD_STATUS_INVALID_STATE)?;
        let (kind, ordinal) = match &payload {
            StagedPayload::Document(result) => (
                match result.kind() {
                    ScriptStagedResultKind::ActiveDocument => {
                        INKPOD_INKSCRIPT_STAGED_ACTIVE_DOCUMENT
                    }
                    ScriptStagedResultKind::NewTab => INKPOD_INKSCRIPT_STAGED_NEW_TAB,
                },
                result.ordinal() as u64,
            ),
            StagedPayload::Preview(_) => (INKPOD_INKSCRIPT_STAGED_IMAGE_PREVIEW, 0),
        };
        let (session_id, session_generation, source_generation) = guard.origin.unwrap_or((0, 0, 0));
        let result = Box::new(InkpodInkScriptStagedResult {
            owner_thread: task.owner_thread,
            core_generation: task.core_generation,
            payload,
            guard,
            cancelled: task.cancelled.clone(),
        });
        *info = InkpodInkScriptStagedInfo {
            struct_size: size_of::<InkpodInkScriptStagedInfo>() as u32,
            version: INKPOD_INKSCRIPT_RECORD_VERSION,
            kind,
            reserved: 0,
            feature_flags: 0,
            ordinal,
            session_id,
            session_generation,
            source_generation,
        };
        // SAFETY: Successful construction transfers a unique Rust owner.
        unsafe {
            output.write(Box::into_raw(result));
        }
        Ok(INKPOD_STATUS_OK)
    })
}

fn staged_owner(
    core: &InkpodCore,
    owner: *mut *mut InkpodInkScriptStagedResult,
) -> Result<*mut InkpodInkScriptStagedResult, u32> {
    if owner.is_null() || !is_aligned(owner) {
        return Err(INKPOD_STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: Caller provides live unique owner storage.
    let pointer = unsafe { owner.read() };
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(INKPOD_STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: Handle allocation remains live until a validated consuming operation.
    let result = unsafe { &*pointer };
    validate_route(result.owner_thread, result.core_generation, core)?;
    Ok(pointer)
}

/// Applies an active result only to the exact original session. Core errors consume the result.
/// # Safety
/// Both Core handles and result belong to this owner thread; owner storage is unique.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_staged_result_apply_active(
    core: *mut InkpodCore,
    owner: *mut *mut InkpodInkScriptStagedResult,
    target: *mut InkpodCore,
    session_id: u64,
    session_generation: u64,
    source_generation: u64,
    cancelled: u32,
) -> u32 {
    io_boundary(|| {
        if cancelled > 1 {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        let core = validate_execution_core(core)?;
        let pointer = staged_owner(core, owner)?;
        // SAFETY: Validated unique result is used only on its owner thread.
        let result = unsafe { &mut *pointer };
        if !matches!(&result.payload, StagedPayload::Document(value) if value.kind() == ScriptStagedResultKind::ActiveDocument)
        {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        result.guard.validate()?;
        let target = validate_execution_core(target)?;
        // SAFETY: All ownership/routing arguments passed validation; consume exactly once.
        let result = unsafe {
            owner.write(ptr::null_mut());
            Box::from_raw(pointer)
        };
        let token = result.cancelled;
        let StagedPayload::Document(result) = result.payload else {
            unreachable!()
        };
        (*result)
            .apply_active(
                &mut target.core,
                session_id,
                session_generation,
                source_generation,
                &mut || cancelled != 0 || token.load(Ordering::Acquire),
            )
            .map_err(|error| match error {
                inkpod_core::inkscript::ScriptRunError::Cancelled => INKPOD_STATUS_CANCELLED,
                _ => INKPOD_STATUS_INVALID_STATE,
            })?;
        Ok(INKPOD_STATUS_OK)
    })
}

/// Transfers a fresh tab or clean preview Core; active results cannot be adopted as unrelated tabs.
/// # Safety
/// Parent/result are live on their owner thread; Core output owner is empty and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_staged_result_take_core(
    core: *mut InkpodCore,
    owner: *mut *mut InkpodInkScriptStagedResult,
    output: *mut *mut InkpodCore,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller supplies empty owner storage.
        unsafe {
            empty_owner(output)?;
        }
        let core = validate_execution_core(core)?;
        let pointer = staged_owner(core, owner)?;
        // SAFETY: Live result is exclusively owned on this thread.
        let result = unsafe { &mut *pointer };
        if matches!(&result.payload, StagedPayload::Document(value) if value.kind() == ScriptStagedResultKind::ActiveDocument)
        {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        if matches!(result.payload, StagedPayload::Preview(_))
            && result.cancelled.load(Ordering::Acquire)
        {
            return Err(INKPOD_STATUS_CANCELLED);
        }
        result.guard.validate()?;
        let objects = crate::v3::ObjectRegistry::new().ok_or(INKPOD_STATUS_INVALID_STATE)?;
        // SAFETY: Validation/allocation succeeded; consume result and clear its owner.
        let result = unsafe {
            owner.write(ptr::null_mut());
            Box::from_raw(pointer)
        };
        let core = match result.payload {
            StagedPayload::Document(value) => (*value)
                .into_new_tab()
                .map_err(|_| INKPOD_STATUS_INVALID_STATE)?,
            StagedPayload::Preview(core) => *core,
        };
        let result = Box::new(InkpodCore {
            owner_thread: thread::current().id(),
            core,
            objects,
        });
        // SAFETY: Initialized Core is transferred exactly once to validated output storage.
        unsafe {
            output.write(Box::into_raw(result));
        }
        Ok(INKPOD_STATUS_OK)
    })
}

/// Drops an unpublished result; repeated release through the same null owner is a no-op.
/// # Safety
/// Parent Core is live on its owner thread and result owner storage is uniquely writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_staged_result_release(
    core: *mut InkpodCore,
    owner: *mut *mut InkpodInkScriptStagedResult,
) -> u32 {
    io_boundary(|| {
        let core = validate_execution_core(core)?;
        if owner.is_null() || !is_aligned(owner) {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        // SAFETY: Unique caller storage is readable/writable for the call.
        if unsafe { owner.read() }.is_null() {
            return Ok(INKPOD_STATUS_OK);
        }
        let pointer = staged_owner(core, owner)?;
        // SAFETY: Matching live owner is consumed once; no publication occurs.
        unsafe {
            drop(Box::from_raw(pointer));
            owner.write(ptr::null_mut());
        }
        Ok(INKPOD_STATUS_OK)
    })
}
