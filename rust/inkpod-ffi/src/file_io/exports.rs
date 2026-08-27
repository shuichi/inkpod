use super::*;

/// Creates one shared bounded filesystem service. Null config selects defaults.
/// # Safety
/// Non-null config exposes its size-prefixed range; output is an empty owner slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_manager_create(
    config: *const InkpodIoConfig,
    out_manager: *mut *mut InkpodIoManager,
) -> u32 {
    io_boundary(|| {
        // SAFETY: The public contract supplies readable owner storage.
        unsafe { empty_owner(out_manager)? };
        let config = if config.is_null() {
            IoConfig::default()
        } else {
            // SAFETY: The public contract supplies the advertised config range.
            unsafe { validate_struct(config, "InkpodIoConfig")? };
            // SAFETY: The complete record was checked above.
            let config = unsafe { &*config };
            if config.reserved != 0 {
                return Err(fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "I/O config reserved field is nonzero",
                ));
            }
            IoConfig {
                worker_count: config.worker_count as usize,
                queue_capacity: config.queue_capacity as usize,
                max_images: config.max_images as usize,
                max_file_bytes: config.max_file_bytes,
                max_encoded_bytes: config.max_encoded_bytes,
                max_decoded_bytes: config.max_decoded_bytes,
            }
        };
        let manager = IoManager::new(config).map_err(|error| map_core_error(error.into()))?;
        // SAFETY: This transfers Box ownership to the validated empty output slot.
        unsafe { out_manager.write(Box::into_raw(Box::new(InkpodIoManager { manager }))) };
        Ok(INKPOD_STATUS_OK)
    })
}

/// Cancels and drains the service; call after every installation is finalized.
/// # Safety
/// Pointer is writable owner storage containing null or one live manager; no concurrent calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_manager_release(manager: *mut *mut InkpodIoManager) -> u32 {
    io_boundary(|| {
        if manager.is_null() || !is_aligned(manager) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid manager owner",
            ));
        }
        // SAFETY: The caller supplies a readable/writable owner slot.
        let pointer = unsafe { manager.read() };
        if pointer.is_null() {
            return Ok(INKPOD_STATUS_OK);
        }
        // SAFETY: Opaque allocation validity is part of the release contract.
        let service = unsafe { manager_ref(pointer)? };
        service.shutdown_and_wait();
        // SAFETY: The unique Box is consumed exactly once and owner cleared first.
        unsafe {
            manager.write(ptr::null_mut());
            drop(Box::from_raw(pointer));
        }
        Ok(INKPOD_STATUS_OK)
    })
}

/// Binds a shared service to a single-writer Core without accessing the filesystem.
/// # Safety
/// Both handles are live; Core is exclusively held on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_bind_io_manager(
    core: *mut InkpodCore,
    manager: *mut InkpodIoManager,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Handle validity and owner access are contractual.
        let core = unsafe { owner_core(core)? };
        // SAFETY: Manager remains live through this call and is cloned internally.
        core.core
            .bind_file_io(unsafe { manager_ref(manager)? }.clone())
            .map_err(map_core_error)?;
        Ok(INKPOD_STATUS_OK)
    })
}

/// Copies path spans and submits bounded worker work. No file bytes cross this ABI.
/// # Safety
/// Handles/spans are valid until return; Core owner affinity applies unless null for references.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_io_submit(
    core: *mut InkpodCore,
    manager: *mut InkpodIoManager,
    request: *const InkpodIoRequest,
    out_job: *mut *mut InkpodIoJob,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller provides readable/writable output and request spans.
        unsafe { empty_owner(out_job)? };
        // SAFETY: Span copying is bounded before any request is accepted.
        let request = unsafe { parse::request(request)? };
        // SAFETY: The live manager is retained by value in the accepted job.
        let manager = unsafe { manager_ref(manager)? }.clone();
        let core = if core.is_null() {
            None
        } else {
            // SAFETY: Non-null Core is exclusively owned by the calling thread.
            Some(&unsafe { owner_core(core)? }.core)
        };
        let job = FileIoJob::start(core, manager, request).map_err(map_core_error)?;
        // SAFETY: Ownership is transferred to the validated empty output slot.
        unsafe {
            out_job.write(Box::into_raw(Box::new(InkpodIoJob {
                job: Mutex::new(job),
                owner_thread: thread::current().id(),
            })))
        };
        Ok(INKPOD_STATUS_OK)
    })
}

/// Requests cancellation without waiting for workers or accessing Core state.
/// # Safety
/// Job is live and release does not race this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_cancel(job: *mut InkpodIoJob) -> u32 {
    io_boundary(|| {
        // SAFETY: The live job is synchronized internally.
        unsafe { job_lock(job)? }.cancel();
        Ok(INKPOD_STATUS_OK)
    })
}

/// Cancels and releases a job. An authorized installation must first be finalized.
/// # Safety
/// Owner storage is writable, contains null/live job, and no calls race release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_release(job: *mut *mut InkpodIoJob) -> u32 {
    io_boundary(|| {
        if job.is_null() || !is_aligned(job) {
            return Err(fail(INKPOD_STATUS_INVALID_ARGUMENT, "invalid job owner"));
        }
        // SAFETY: Caller supplies readable owner storage.
        let pointer = unsafe { job.read() };
        if pointer.is_null() {
            return Ok(INKPOD_STATUS_OK);
        }
        {
            // SAFETY: The allocation is live until unique release below.
            let mut state = unsafe { job_lock(pointer)? };
            if state.requires_finalization() {
                return Err(fail(
                    INKPOD_STATUS_INVALID_STATE,
                    "save installation must be finalized before release",
                ));
            }
            state.cancel();
        }
        // SAFETY: Clear the owner before consuming its uniquely owned Box.
        unsafe {
            job.write(ptr::null_mut());
            drop(Box::from_raw(pointer));
        }
        Ok(INKPOD_STATUS_OK)
    })
}

/// Applies one prepared result. PENDING means installation needs polling and final apply.
/// # Safety
/// Core/job are live, Core is on its owner thread, optional outputs are writable records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_io_job_apply(
    core: *mut InkpodCore,
    job: *mut InkpodIoJob,
    out_document: *mut InkpodDocumentInfo,
    out_object_id: *mut u64,
) -> u32 {
    io_boundary(|| {
        if !out_document.is_null() {
            // SAFETY: The non-null record exposes its readable prefix and writable range.
            unsafe { validate_struct(out_document, "InkpodDocumentInfo")? };
        }
        if !out_object_id.is_null() && !is_aligned(out_object_id) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "I/O object output is misaligned",
            ));
        }
        // SAFETY: The Core's validity/exclusivity are contractual.
        let core = unsafe { owner_core(core)? };
        // SAFETY: The job's detached state is internally synchronized.
        match unsafe { job_lock(job)? }
            .apply(&mut core.core)
            .map_err(map_core_error)?
        {
            FileIoApply::Pending => Ok(INKPOD_STATUS_PENDING),
            FileIoApply::Complete {
                document,
                object_id,
            } => {
                if !out_document.is_null() {
                    if let Some(document) = document {
                        // SAFETY: Complete output record was validated before mutation.
                        write_document_info(unsafe { &mut *out_document }, *document);
                    }
                }
                if !out_object_id.is_null() {
                    // SAFETY: Caller provides writable aligned scalar storage.
                    unsafe { out_object_id.write(object_id) };
                }
                Ok(INKPOD_STATUS_OK)
            }
        }
    })
}

/// Replaces a reference catalog without creating an editable Core document.
/// # Safety
/// Catalog is live/exclusive on its owner thread; job and output are valid ranges.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_io_job_apply(
    subpalette: *mut InkpodSubpalette,
    job: *mut InkpodIoJob,
    out_info: *mut InkpodSubpaletteInfo,
) -> u32 {
    io_boundary(|| {
        if subpalette.is_null() || !is_aligned(subpalette) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid subpalette handle",
            ));
        }
        // SAFETY: Caller supplies a writable full record with readable prefix.
        unsafe { validate_struct(out_info, "InkpodSubpaletteInfo")? };
        // SAFETY: Live catalog is exclusively owned by this thread.
        let subpalette = unsafe { &mut *subpalette };
        let status = validate_subpalette_thread(subpalette);
        if status != INKPOD_STATUS_OK {
            return Err(status);
        }
        // SAFETY: Detached job state is protected by its mutex.
        let info = unsafe { job_lock(job)? }
            .apply_reference(&mut subpalette.catalog)
            .map_err(map_core_error)?;
        // SAFETY: Output validated above before publication.
        crate::subpalette::write_subpalette_info(unsafe { &mut *out_info }, info);
        Ok(INKPOD_STATUS_OK)
    })
}

/// Sets only future blank-document raster companion defaults.
/// # Safety
/// Core is live and exclusively accessed on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_new_cell_raster_format(
    core: *mut InkpodCore,
    format: u32,
) -> u32 {
    io_boundary(|| {
        let format = parse_common_raster_format(format)?;
        // SAFETY: Owner-thread handle access checked before mutation.
        unsafe { owner_core(core)? }
            .core
            .set_new_cell_raster_format(format);
        Ok(INKPOD_STATUS_OK)
    })
}

/// Queries the active document's persisted raster companion format.
/// # Safety
/// Core is live on its owner thread; output is writable/aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_raster_file_format(
    core: *mut InkpodCore,
    out_format: *mut u32,
) -> u32 {
    io_boundary(|| {
        if out_format.is_null() || !is_aligned(out_format) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid raster format output",
            ));
        }
        // SAFETY: Checked owner-thread access; output is writable by contract.
        let format = unsafe { owner_core(core)? }
            .core
            .raster_file_format()
            .map_err(map_core_error)?;
        // SAFETY: Aligned output storage is valid for one u32.
        unsafe { out_format.write(parse::format_code(format)) };
        Ok(INKPOD_STATUS_OK)
    })
}
