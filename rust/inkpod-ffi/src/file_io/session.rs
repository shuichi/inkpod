use super::*;
use std::path::PathBuf;

// SAFETY: Non-null input exposes a readable size prefix and bounded path span.
unsafe fn optional_path(pointer: *const InkpodIoPath) -> Result<Option<PathBuf>, u32> {
    if pointer.is_null() {
        return Ok(None);
    }
    // SAFETY: The advertised record range remains readable for this call.
    unsafe { validate_struct(pointer, "InkpodIoPath")? };
    // SAFETY: Complete size/alignment checked above.
    let value = unsafe { &*pointer };
    if value.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "recovery path reserved field is nonzero",
        ));
    }
    if value.path_bytes == 0 {
        return Ok(None);
    }
    // SAFETY: The borrowed path bytes are copied synchronously.
    Ok(Some(
        unsafe { path_from_utf8(value.path, value.path_bytes)? }.to_path_buf(),
    ))
}

/// Captures a sequence switch with optional target recovery and typed source association.
/// # Safety
/// Core is on its owner thread; all input records/spans and empty output are valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_io_sequence_switch_submit(
    core: *mut InkpodCore,
    manager: *mut InkpodIoManager,
    request: *const InkpodSequenceSwitchRequest,
    source_recovery: *const InkpodIoPath,
    target_recovery: *const InkpodIoPath,
    target_recovery_proof: *const InkpodIoRecoveryArtifactProof,
    metadata: *const InkpodIoRecoveryMetadata,
    out_job: *mut *mut InkpodIoJob,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Writable owner storage and a readable input prefix are contractual.
        unsafe {
            empty_owner(out_job)?;
            validate_struct(request, "InkpodSequenceSwitchRequest")?;
        }
        // SAFETY: The complete request record was validated above.
        let raw_request = unsafe { &*request };
        if raw_request.feature_flags & !INKPOD_SEQUENCE_SWITCH_TARGET_RASTER_PAIR != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "sequence switch request contains unsupported submit flags",
            ));
        }
        let resolve_raster_pair =
            raw_request.feature_flags & INKPOD_SEQUENCE_SWITCH_TARGET_RASTER_PAIR != 0;
        let mut core_request = *raw_request;
        core_request.feature_flags = INKPOD_FEATURE_NONE;
        let request = crate::animation::parse_sequence_switch_request(&core_request)?;
        // SAFETY: Optional path spans remain readable until copied here.
        let source = unsafe { optional_path(source_recovery)? };
        // SAFETY: Same optional path contract as the source.
        let target = unsafe { optional_path(target_recovery)? };
        let target_proof = if target_recovery_proof.is_null() {
            None
        } else {
            // SAFETY: Non-null proof exposes its complete size-prefixed stamps.
            Some(unsafe { recovery::parse_artifact_proof(target_recovery_proof)? })
        };
        let metadata = if metadata.is_null() {
            None
        } else {
            // SAFETY: Non-null metadata exposes all size-prefixed text spans.
            Some(unsafe { recovery::parse_metadata(metadata)? })
        };
        if !request.requires_switch()
            && (target.is_some() || target_proof.is_some() || resolve_raster_pair)
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "same-cell sequence no-op does not accept a target artifact",
            ));
        }
        // SAFETY: The live handles satisfy owner affinity and shared service lifetime.
        let owner = &unsafe { owner_core(core)? }.core;
        let manager = unsafe { manager_ref(manager)? }.clone();
        let job = if resolve_raster_pair {
            if target_proof.is_some() {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence raster-pair target does not accept a recovery proof",
                ));
            }
            let target = target.ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence raster-pair target path is missing",
                )
            })?;
            FileIoJob::start_sequence_raster_pair_switch(
                owner, manager, request, source, target, metadata,
            )
        } else {
            let target = if request.requires_switch() {
                match (target, target_proof) {
                    (Some(path), Some(proof)) => Some((path, proof)),
                    (None, None) => None,
                    _ => {
                        return Err(fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "explicit sequence recovery requires its exact proof",
                        ));
                    }
                }
            } else {
                None
            };
            FileIoJob::start_sequence_switch(owner, manager, request, source, target, metadata)
        }
        .map_err(map_core_error)?;
        // SAFETY: Unique ownership transfers to the validated empty output slot.
        unsafe {
            out_job.write(Box::into_raw(Box::new(InkpodIoJob {
                job: Mutex::new(job),
                owner_thread: thread::current().id(),
            })))
        };
        Ok(INKPOD_STATUS_OK)
    })
}

/// Captures an explicitly confirmed separate history-compacted native output.
/// # Safety
/// Core is owner-thread exclusive; path/plan are readable and output owner is empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_io_compacted_copy_submit(
    core: *mut InkpodCore,
    manager: *mut InkpodIoManager,
    path: *const u8,
    path_bytes: u64,
    plan: *const InkpodCompactionPlan,
    out_job: *mut *mut InkpodIoJob,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller supplies readable prefixes and writable owner storage.
        unsafe {
            empty_owner(out_job)?;
            validate_struct(plan, "InkpodCompactionPlan")?;
        }
        // SAFETY: The complete confirmation record was validated above.
        let plan = unsafe { &*plan };
        if plan.reserved != 0 || plan.feature_flags != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "unknown compaction confirmation flags",
            ));
        }
        let plan = inkpod_core::CompactionPlan::from_confirmation(
            plan.history_event_count,
            plan.history_procedure_count,
            plan.document_digest,
            plan.editor_digest,
            plan.journal_digest,
        );
        // SAFETY: Path span is borrowed only while copying its bounded value.
        let path = unsafe { path_from_utf8(path, path_bytes)? }.to_path_buf();
        // SAFETY: Both opaque handles are live for this owner-thread call.
        let job = FileIoJob::start_compacted_copy(
            &unsafe { owner_core(core)? }.core,
            unsafe { manager_ref(manager)? }.clone(),
            path,
            plan,
        )
        .map_err(map_core_error)?;
        // SAFETY: Transfers one owned job to the prevalidated empty slot.
        unsafe {
            out_job.write(Box::into_raw(Box::new(InkpodIoJob {
                job: Mutex::new(job),
                owner_thread: thread::current().id(),
            })))
        };
        Ok(INKPOD_STATUS_OK)
    })
}
