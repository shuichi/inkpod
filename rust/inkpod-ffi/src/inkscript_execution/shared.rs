//! Owner-thread captures and planning through the shared Rust filesystem service.
use super::*;
use crate::file_io::{empty_owner, io_boundary, manager_ref};
use inkpod_core::inkscript::ScriptIoSequenceInput;
use inkpod_format::{MAX_INKSCRIPT_CONTAINER_ELEMENTS, MAX_INKSCRIPT_SOURCE_BYTES};
use inkpod_io::{IoConfig, IoManager};
use std::path::PathBuf;

pub struct InkpodInkScriptIo {
    owner_thread: ThreadId,
    core_generation: u64,
    adapter: ScriptIoAdapter,
    sequence: Option<SequenceCapture>,
}

pub(super) type SequenceCapture = (u64, u64, Vec<ScriptIoSequenceInput>);

// SAFETY: Caller advertises a live complete record with the common size/version/flags prefix.
unsafe fn input_record<'a, T>(pointer: *const T, name: &str) -> Result<&'a T, u32> {
    // SAFETY: The public caller exposes a readable size prefix and the advertised record range.
    let record = unsafe { checked_record(pointer, name)? };
    #[repr(C)]
    struct Prefix {
        size: u32,
        version: u32,
        flags: u64,
    }
    // SAFETY: All private callers use records with this exact initial layout and size >= 16.
    let prefix = unsafe { &*pointer.cast::<Prefix>() };
    if prefix.version != INKPOD_INKSCRIPT_RECORD_VERSION {
        return Err(fail(
            INKPOD_STATUS_INCOMPATIBLE_ABI,
            "InkScript shared record is not exact-current",
        ));
    }
    if prefix.flags != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "InkScript shared record has unknown flags",
        ));
    }
    Ok(record)
}

// SAFETY: Caller advertises a bounded strided span of records with readable size prefixes.
unsafe fn shared_records<T: Copy>(
    pointer: *const T,
    count: u64,
    stride: u64,
    maximum: usize,
    name: &str,
) -> Result<Vec<T>, u32> {
    if count > maximum as u64 {
        return Err(INKPOD_STATUS_INVALID_ARGUMENT);
    }
    if count == 0 {
        if !pointer.is_null() || stride != 0 {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        return Ok(Vec::new());
    }
    if pointer.is_null()
        || !is_aligned(pointer)
        || stride < size_of::<T>() as u64
        || stride % align_of::<T>() as u64 != 0
    {
        return Err(INKPOD_STATUS_INVALID_ARGUMENT);
    }
    let count = usize::try_from(count).map_err(|_| INKPOD_STATUS_INVALID_ARGUMENT)?;
    let stride = usize::try_from(stride).map_err(|_| INKPOD_STATUS_INVALID_ARGUMENT)?;
    let bytes = count
        .checked_mul(stride)
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or(INKPOD_STATUS_INVALID_ARGUMENT)?;
    let _ = bytes;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| INKPOD_STATUS_INVALID_ARGUMENT)?;
    for index in 0..count {
        // SAFETY: Checked stride arithmetic addresses this advertised record; only its size
        // prefix is read before validation, so short records are rejected without full reads.
        let record = unsafe { pointer.cast::<u8>().add(index * stride).cast::<T>() };
        // SAFETY: The caller's size-prefix contract applies at every record position.
        values.push(*unsafe { checked_record(record, name)? });
    }
    Ok(values)
}

fn path(span: InkpodInkScriptUtf8Span, optional: bool) -> Result<Option<PathBuf>, u32> {
    if span.byte_count == 0 && optional {
        return Ok(None);
    }
    let value = checked_utf8(span, "InkScript approved absolute path")?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "InkScript approved path must be absolute",
        ));
    }
    Ok(Some(path))
}

fn io_ref(
    core: &InkpodCore,
    pointer: *mut InkpodInkScriptIo,
) -> Result<&'static mut InkpodInkScriptIo, u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(INKPOD_STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: Opaque handle is live and uniquely used on its owning engine thread.
    let io = unsafe { &mut *pointer };
    validate_route(io.owner_thread, io.core_generation, core)?;
    Ok(io)
}

/// Copies explicit absolute-path approvals; a null manager creates an owned default shared service.
/// # Safety
/// Core is owner-thread live; records/spans and the empty output owner are valid during this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_io_create(
    core: *mut InkpodCore,
    manager: *mut InkpodIoManager,
    request: *const InkpodInkScriptIoRequest,
    output: *mut *mut InkpodInkScriptIo,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller supplies initialized owner storage and bounded input records.
        unsafe {
            empty_owner(output)?;
        }
        let core = validate_execution_core(core)?;
        // SAFETY: Request and all advertised path records remain readable for this call.
        let request = unsafe { input_record(request, "InkpodInkScriptIoRequest")? };
        // SAFETY: Checked strided path span is copied before any owner is published.
        let records = unsafe {
            shared_records(
                request.approved_paths,
                request.path_count,
                request.path_stride_bytes,
                2 * MAX_INKSCRIPT_INPUTS + MAX_INKSCRIPT_CONTAINER_ELEMENTS + 1,
                "InkScript approved paths",
            )?
        };
        let mut paths = Vec::new();
        let mut total_bytes = 0u64;
        paths
            .try_reserve_exact(records.len())
            .map_err(|_| INKPOD_STATUS_INVALID_ARGUMENT)?;
        for value in records {
            total_bytes = total_bytes
                .checked_add(value.path.byte_count)
                .filter(|total| *total <= MAX_INKSCRIPT_SOURCE_BYTES as u64)
                .ok_or(INKPOD_STATUS_INVALID_ARGUMENT)?;
            // SAFETY: Copied path record retains its borrowed UTF-8 bytes until this call returns.
            unsafe {
                input_record(&value, "InkpodInkScriptApprovedPath")?;
            }
            paths.push((
                value.intent_id,
                path(value.path, false)?.ok_or(INKPOD_STATUS_INVALID_ARGUMENT)?,
            ));
        }
        let capacity = usize::try_from(request.new_tab_capacity)
            .map_err(|_| INKPOD_STATUS_INVALID_ARGUMENT)?;
        let manager = if manager.is_null() {
            IoManager::new(IoConfig::default()).map_err(|_| INKPOD_STATUS_INVALID_STATE)?
        } else {
            // SAFETY: Live shared manager is borrowed only long enough to clone its service owner.
            unsafe { manager_ref(manager)? }.clone()
        };
        let adapter = ScriptIoAdapter::new(manager, paths, capacity).map_err(map_plan_error)?;
        let io = Box::new(InkpodInkScriptIo {
            owner_thread: thread::current().id(),
            core_generation: core.objects.generation(),
            adapter,
            sequence: None,
        });
        // SAFETY: Validated empty owner is unique.
        unsafe {
            output.write(Box::into_raw(io));
        }
        Ok(INKPOD_STATUS_OK)
    })
}

/// Captures an immutable session, including document/editor history and savepoints.
/// # Safety
/// Both Core handles and the adapter are live on the same owner thread. Input spans are borrowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_io_capture_session(
    core: *mut InkpodCore,
    io: *mut InkpodInkScriptIo,
    request: *const InkpodInkScriptIoSession,
) -> u32 {
    io_boundary(|| {
        let owner = validate_execution_core(core)?;
        let io = io_ref(owner, io)?;
        // SAFETY: Complete request/UTF-8 spans are borrowed only during this call.
        let request = unsafe { checked_record(request, "InkpodInkScriptIoSession")? };
        if request.version != INKPOD_INKSCRIPT_RECORD_VERSION {
            return Err(INKPOD_STATUS_INCOMPATIBLE_ABI);
        }
        if request.feature_flags & !INKPOD_INKSCRIPT_SESSION_USE_CORE_BACKING != 0 {
            return Err(INKPOD_STATUS_UNSUPPORTED);
        }
        if request.reserved != 0 {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        let label = checked_utf8(request.label, "InkScript session label")?;
        let backing = path(request.backing_path, true)?;
        let session = validate_execution_core(request.session_core)?;
        if request.feature_flags & INKPOD_INKSCRIPT_SESSION_USE_CORE_BACKING != 0 {
            if backing.is_some() {
                return Err(INKPOD_STATUS_INVALID_ARGUMENT);
            }
            io.adapter
                .capture_session_from_core(
                    request.session_id,
                    request.session_generation,
                    request.source_generation,
                    label,
                    request.display_number,
                    &session.core,
                )
                .map_err(map_plan_error)?;
        } else {
            io.adapter
                .capture_session(
                    request.session_id,
                    request.session_generation,
                    request.source_generation,
                    label,
                    request.display_number,
                    backing,
                    &session.core,
                )
                .map_err(map_plan_error)?;
        }
        Ok(INKPOD_STATUS_OK)
    })
}

/// Invalidates a closed/replaced session or revoked backing; zero revokes all path authority.
/// # Safety
/// Core and adapter are matching live owner-thread handles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_io_invalidate(
    core: *mut InkpodCore,
    io: *mut InkpodInkScriptIo,
    session_id: u64,
) -> u32 {
    io_boundary(|| {
        let core = validate_execution_core(core)?;
        let io = io_ref(core, io)?;
        if session_id == 0 {
            io.adapter.invalidate_authority()
        } else {
            io.adapter.invalidate_session(session_id)
        }
        .map_err(map_plan_error)?;
        Ok(INKPOD_STATUS_OK)
    })
}

/// Validates captured document identity and backing authority against a live Core.
/// Ordinary edits/history/savepoints are ignored; changed backing invalidates the session.
/// # Safety
/// Owner, adapter, and live session Core are used exclusively on the same owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_io_validate_session(
    core: *mut InkpodCore,
    io: *mut InkpodInkScriptIo,
    session_id: u64,
    session: *mut InkpodCore,
) -> u32 {
    io_boundary(|| {
        let owner = validate_execution_core(core)?;
        let io = io_ref(owner, io)?;
        let session = validate_execution_core(session)?;
        if !io
            .adapter
            .validate_captured_session_backing(session_id, &session.core)
            .map_err(map_plan_error)?
        {
            io.adapter
                .invalidate_session(session_id)
                .map_err(map_plan_error)?;
            return Err(fail(
                INKPOD_STATUS_INVALID_STATE,
                "InkScript captured session backing changed",
            ));
        }
        Ok(INKPOD_STATUS_OK)
    })
}

/// Copies a bounded issue-time sequence declaration. The plan task captures files under cancel.
/// # Safety
/// Parent/adapter and initialized strided member records are live on their owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_io_capture_sequence(
    core: *mut InkpodCore,
    io: *mut InkpodInkScriptIo,
    request: *const InkpodInkScriptIoSequenceRequest,
) -> u32 {
    io_boundary(|| {
        let core = validate_execution_core(core)?;
        let io = io_ref(core, io)?;
        // SAFETY: Request and bounded nested member records remain borrowed for this call.
        let request = unsafe { input_record(request, "InkpodInkScriptIoSequenceRequest")? };
        if request.sequence_id == 0 || request.generation == 0 || request.member_count == 0 {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        // SAFETY: The advertised strided span is checked before copying records.
        let records = unsafe {
            shared_records(
                request.members,
                request.member_count,
                request.member_stride_bytes,
                MAX_INKSCRIPT_INPUTS,
                "InkScript shared sequence",
            )?
        };
        let mut members = Vec::new();
        let mut total_bytes = 0u64;
        members
            .try_reserve_exact(records.len())
            .map_err(|_| INKPOD_STATUS_INVALID_ARGUMENT)?;
        for member in records {
            total_bytes = total_bytes
                .checked_add(member.path.byte_count)
                .filter(|total| *total <= MAX_INKSCRIPT_SOURCE_BYTES as u64)
                .ok_or(INKPOD_STATUS_INVALID_ARGUMENT)?;
            if member.version != INKPOD_INKSCRIPT_RECORD_VERSION {
                return Err(INKPOD_STATUS_INCOMPATIBLE_ABI);
            }
            if member.feature_flags != 0 || member.reserved != 0 {
                return Err(INKPOD_STATUS_UNSUPPORTED);
            }
            match member.kind {
                INKPOD_INKSCRIPT_SESSION_MEMBER
                    if member.session_id != 0
                        && member.path.byte_count == 0
                        && member.source_generation == 0 =>
                {
                    members.push(ScriptIoSequenceInput::Session(member.session_id))
                }
                INKPOD_INKSCRIPT_FILE_MEMBER
                    if member.session_id == 0 && member.source_generation != 0 =>
                {
                    members.push(ScriptIoSequenceInput::File {
                        path: path(member.path, false)?.ok_or(INKPOD_STATUS_INVALID_ARGUMENT)?,
                        source_generation: member.source_generation,
                    })
                }
                _ => return Err(INKPOD_STATUS_INVALID_ARGUMENT),
            }
        }
        io.adapter.invalidate_authority().map_err(map_plan_error)?;
        io.sequence = Some((request.sequence_id, request.generation, members));
        Ok(INKPOD_STATUS_OK)
    })
}

/// Releases one owner; detached plan/run clones keep their shared service and invalidation state.
/// # Safety
/// Output owner is unique and synchronized; release occurs on its live parent Core's owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_io_release(
    core: *mut InkpodCore,
    owner: *mut *mut InkpodInkScriptIo,
) -> u32 {
    io_boundary(|| {
        let core = validate_execution_core(core)?;
        if owner.is_null() || !is_aligned(owner) {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        // SAFETY: Unique owner storage is readable and writable.
        let pointer = unsafe { owner.read() };
        if pointer.is_null() {
            return Ok(INKPOD_STATUS_OK);
        }
        io_ref(core, pointer)?;
        // SAFETY: Matching owner handle is consumed exactly once.
        unsafe {
            drop(Box::from_raw(pointer));
            owner.write(ptr::null_mut());
        }
        Ok(INKPOD_STATUS_OK)
    })
}

/// Starts the existing planner using shared Rust I/O and a frozen issuing context.
/// # Safety
/// Handles belong to this live owner Core; request spans and empty output storage are valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_inkscript_shared_plan_task_create(
    core: *mut InkpodCore,
    program: *const InkpodInkScriptProgram,
    io: *mut InkpodInkScriptIo,
    request: *const InkpodInkScriptSharedPlanRequest,
    output: *mut *mut InkpodInkScriptPlanTask,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller owns valid empty output storage.
        unsafe {
            empty_owner(output)?;
        }
        let core = validate_execution_core(core)?;
        let io = io_ref(core, io)?;
        if program.is_null() || !is_aligned(program) {
            return Err(INKPOD_STATUS_INVALID_ARGUMENT);
        }
        // SAFETY: Program is a live immutable owner-thread handle.
        let program = unsafe { &*program };
        validate_route(program.owner_thread, program.core_generation, core)?;
        // SAFETY: Request advertises its readable initialized record and path span.
        let request = unsafe { input_record(request, "InkpodInkScriptSharedPlanRequest")? };
        if request.controller_id != program.controller_id
            || request.session_generation != program.session_generation
        {
            return Err(INKPOD_STATUS_INVALID_STATE);
        }
        let task = Box::new(InkpodInkScriptPlanTask {
            owner_thread: thread::current().id(),
            core_generation: core.objects.generation(),
            controller_id: request.controller_id,
            session_generation: request.session_generation,
            authority_generation: 0,
            open_session_set_generation: 0,
            program: program.program.clone(),
            grants: Vec::new(),
            script_path: None,
            maximum_folder_entries: request.maximum_folder_entries,
            host: None,
            current_session: (request.current_session_id != 0)
                .then_some(request.current_session_id),
            shared_script_path: path(request.script_path, true)?,
            state: AtomicU32::new(INKPOD_TASK_READY),
            cancelled: AtomicBool::new(false),
            completed_work: AtomicU64::new(0),
            total_work: AtomicU64::new(1),
            owner_data: UnsafeCell::new(PlanTaskOwnerData {
                terminal_status: INKPOD_STATUS_INVALID_STATE,
                pending_event: None,
                plan: None,
                shared_io: Some(io.adapter.clone()),
                sequence: io.sequence.clone(),
            }),
        });
        // SAFETY: Construction succeeded; publish exactly one owner.
        unsafe {
            output.write(Box::into_raw(task));
        }
        Ok(INKPOD_STATUS_OK)
    })
}
