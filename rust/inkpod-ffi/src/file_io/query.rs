use super::*;

pub(super) fn progress_flags(progress: &inkpod_core::FileIoProgress) -> u64 {
    (if progress.truncated {
        INKPOD_IO_RESULT_TRUNCATED
    } else {
        0
    }) | (if progress.installing {
        INKPOD_IO_RESULT_INSTALLING
    } else {
        0
    }) | (if progress.authority_repaired {
        INKPOD_IO_RESULT_AUTHORITY_REPAIRED
    } else {
        0
    }) | if progress.authority_revoked {
        INKPOD_IO_RESULT_AUTHORITY_REVOKED
    } else {
        0
    }
}

fn identity_record(identity: inkpod_io::FileIdentity, physical: bool) -> InkpodIoFileIdentity {
    InkpodIoFileIdentity {
        struct_size: size_of::<InkpodIoFileIdentity>() as u32,
        kind: if physical { 1 } else { 2 },
        volume: identity.volume,
        object_high: (identity.file >> 64) as u64,
        object_low: identity.file as u64,
    }
}

/// Queries retained cache allocations, including pinned/evicted leases.
/// # Safety
/// Manager is live; output is a complete writable size-prefixed record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_manager_get_cache_info(
    manager: *const InkpodIoManager,
    out_info: *mut InkpodIoCacheInfo,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller provides a readable prefix and writable output range.
        unsafe { validate_struct(out_info, "InkpodIoCacheInfo")? };
        // SAFETY: Live service lifetime is retained throughout the query.
        let handle = unsafe { manager_handle_ref(manager)? };
        let stats = handle.manager.cache_stats();
        let targets = handle.validated_targets.stats();
        let info = InkpodIoCacheInfo {
            struct_size: size_of::<InkpodIoCacheInfo>() as u32,
            reserved: 0,
            image_count: stats.images,
            encoded_bytes: stats.encoded_bytes,
            decoded_bytes: stats.decoded_bytes,
            physical_reads: stats.physical_reads,
            decodes: stats.decodes,
            cache_hits: stats.cache_hits,
            sequence_render_allocations: stats.sequence_render_allocations,
            sequence_render_bytes: stats.sequence_render_bytes,
            validated_target_maximum_bytes: targets.maximum_bytes,
            validated_target_bytes: targets.retained_bytes,
            validated_target_count: targets.target_count,
            validated_target_hits: targets.hits,
            validated_target_misses: targets.misses,
            validated_target_evictions: targets.evictions,
        };
        // SAFETY: Complete writable output validated above.
        unsafe { out_info.write(info) };
        Ok(INKPOD_STATUS_OK)
    })
}

/// Resolves physical identity (or normalized missing-path identity) in Rust.
/// # Safety
/// Manager/path span are live for the call; output is a writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_resolve_identity(
    manager: *const InkpodIoManager,
    path: *const u8,
    path_bytes: u64,
    out_identity: *mut InkpodIoFileIdentity,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Output advertises its complete writable range.
        unsafe { validate_struct(out_identity, "InkpodIoFileIdentity")? };
        // SAFETY: The bounded UTF-8 span remains readable through this call.
        let path = unsafe { path_from_utf8(path, path_bytes)? };
        // SAFETY: Service allocation is live and externally synchronized with release.
        let (identity, physical) = unsafe { manager_ref(manager)? }
            .resolve_identity(path)
            .map_err(|error| map_core_error(error.into()))?;
        // SAFETY: Full output validated before writing.
        unsafe { out_identity.write(identity_record(identity, physical)) };
        Ok(INKPOD_STATUS_OK)
    })
}

/// Polls detached work without filesystem I/O or mutable Core access.
/// # Safety
/// Job is live and output is a complete writable record; release cannot race.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_poll(
    job: *const InkpodIoJob,
    out_info: *mut InkpodIoJobInfo,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Public output record exposes the advertised readable/writable range.
        unsafe { validate_struct(out_info, "InkpodIoJobInfo")? };
        // SAFETY: Live job is mutex protected while advancing detached stages.
        let mut job = unsafe { job_lock(job)? };
        let progress = job.poll();
        let info = InkpodIoJobInfo {
            struct_size: size_of::<InkpodIoJobInfo>() as u32,
            state: match progress.state {
                FileIoState::Queued => INKPOD_IO_QUEUED,
                FileIoState::Running => INKPOD_IO_RUNNING,
                FileIoState::Ready => INKPOD_IO_READY,
                FileIoState::Complete => INKPOD_IO_COMPLETE,
                FileIoState::Failed => INKPOD_IO_FAILED,
                FileIoState::Cancelled => INKPOD_IO_CANCELLED,
            },
            kind: parse::kind_code(progress.kind),
            status: job
                .error()
                .cloned()
                .map_or(INKPOD_STATUS_OK, map_core_error),
            job_id: progress.job_id,
            discovered_count: progress.discovered_count,
            total_count: progress.total_count,
            read_count: progress.read_count,
            loaded_count: progress.loaded_count,
            failed_count: progress.failed_count,
            cancelled_count: progress.cancelled_count,
            completed_work: progress.completed_work,
            total_work: progress.total_work,
            result_count: progress.result_count,
            flags: progress_flags(&progress),
        };
        // SAFETY: Caller output was validated before query.
        unsafe { out_info.write(info) };
        Ok(INKPOD_STATUS_OK)
    })
}

/// Copies immutable item path/name bytes, excluding NUL. Zero capacity queries sizes.
/// # Safety
/// Job/record/buffer spans are valid, mutually nonoverlapping and writable as advertised.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_get_item(
    job: *const InkpodIoJob,
    index: u64,
    out_info: *mut InkpodIoItemInfo,
    path: *mut u8,
    path_capacity: u64,
    name: *mut u8,
    name_capacity: u64,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller output advertises its complete writable range.
        unsafe { validate_struct(out_info, "InkpodIoItemInfo")? };
        // SAFETY: Job is live until the end of this bounded query.
        let job = unsafe { job_lock(job)? };
        let item = job
            .item(
                usize::try_from(index)
                    .map_err(|_| fail(INKPOD_STATUS_INVALID_ARGUMENT, "item index overflows"))?,
            )
            .map_err(map_core_error)?;
        let path_text = item
            .path
            .to_str()
            .ok_or_else(|| fail(INKPOD_STATUS_IO_ERROR, "result path is not UTF-8"))?;
        let info = InkpodIoItemInfo {
            struct_size: size_of::<InkpodIoItemInfo>() as u32,
            raster_format: item.format.map_or(0, parse::format_code),
            source_generation: item.source_generation,
            document_uuid_high: (item.document_uuid >> 64) as u64,
            document_uuid_low: item.document_uuid as u64,
            path_bytes: path_text.len() as u64,
            name_bytes: item.name.len() as u64,
            identity: identity_record(item.identity, item.identity_physical),
        };
        // SAFETY: Write sizes even for the query/short-capacity case.
        unsafe { out_info.write(info) };
        // SAFETY: Buffer lengths are checked before copying and caller promises nonoverlap.
        unsafe {
            copy_span(path_text.as_bytes(), path, path_capacity)?;
            copy_span(item.name.as_bytes(), name, name_capacity)?;
        }
        Ok(INKPOD_STATUS_OK)
    })
}

/// Copies normal-pair authority for one eagerly resident sequence item.
/// # Safety
/// Job/record/native-path buffer are live, writable, and mutually nonoverlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_get_sequence_resident(
    job: *const InkpodIoJob,
    index: u64,
    out_info: *mut InkpodIoSequenceResidentInfo,
    native_path: *mut u8,
    native_path_capacity: u64,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller output advertises its complete writable range.
        unsafe { validate_struct(out_info, "InkpodIoSequenceResidentInfo")? };
        // SAFETY: Job is live until the end of this bounded query.
        let job = unsafe { job_lock(job)? };
        let item = job
            .item(
                usize::try_from(index)
                    .map_err(|_| fail(INKPOD_STATUS_INVALID_ARGUMENT, "item index overflows"))?,
            )
            .map_err(map_core_error)?;
        let Some(native) = item.sequence_resident_native.as_ref() else {
            // Absence identifies a catalog entry beyond the bounded prepared
            // set; it is an ordinary successful query.
            unsafe {
                out_info.write(InkpodIoSequenceResidentInfo {
                    struct_size: size_of::<InkpodIoSequenceResidentInfo>() as u32,
                    source_generation: item.source_generation,
                    document_uuid_high: (item.document_uuid >> 64) as u64,
                    document_uuid_low: item.document_uuid as u64,
                    ..InkpodIoSequenceResidentInfo::default()
                });
            }
            return Ok(INKPOD_STATUS_OK);
        };
        let path_text = native
            .path
            .to_str()
            .ok_or_else(|| fail(INKPOD_STATUS_IO_ERROR, "resident native path is not UTF-8"))?;
        let info = InkpodIoSequenceResidentInfo {
            struct_size: size_of::<InkpodIoSequenceResidentInfo>() as u32,
            flags: INKPOD_IO_SEQUENCE_RESIDENT_AVAILABLE,
            source_generation: item.source_generation,
            document_uuid_high: (item.document_uuid >> 64) as u64,
            document_uuid_low: item.document_uuid as u64,
            native_path_bytes: path_text.len() as u64,
            native_identity: identity_record(native.identity, native.identity_physical),
        };
        // SAFETY: Sizes are returned even for a zero-capacity query.
        unsafe { out_info.write(info) };
        // SAFETY: The helper validates the writable span before copying.
        unsafe { copy_span(path_text.as_bytes(), native_path, native_path_capacity)? };
        Ok(INKPOD_STATUS_OK)
    })
}

// SAFETY: Nonzero-capacity buffer exposes its advertised writable span, without aliasing bytes.
unsafe fn copy_span(bytes: &[u8], buffer: *mut u8, capacity: u64) -> Result<(), u32> {
    if capacity == 0 {
        return Ok(());
    }
    if buffer.is_null()
        || capacity > isize::MAX as u64
        || (buffer as usize).checked_add(capacity as usize).is_none()
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "invalid output byte span",
        ));
    }
    if capacity < bytes.len() as u64 {
        return Err(fail(
            INKPOD_STATUS_BUFFER_TOO_SMALL,
            "output byte capacity is too small",
        ));
    }
    // SAFETY: Bounds checked above; lifetime/nonoverlap are caller requirements.
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len()) };
    Ok(())
}

/// Copies the job-local bounded UTF-8 diagnostic, excluding NUL; zero capacity queries size.
/// # Safety
/// Job is live; output scalar and nonzero-capacity buffer are valid writable spans.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_copy_error(
    job: *const InkpodIoJob,
    buffer: *mut u8,
    capacity: u64,
    out_required_bytes: *mut u64,
) -> u32 {
    io_boundary(|| {
        if out_required_bytes.is_null() || !is_aligned(out_required_bytes) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid error size output",
            ));
        }
        // SAFETY: Live job is synchronized for its diagnostic read.
        let job = unsafe { job_lock(job)? };
        let mut message = job.error().map(ToString::to_string).unwrap_or_default();
        let mut limit = message.len().min(ERROR_CAPACITY - 1);
        while !message.is_char_boundary(limit) {
            limit -= 1;
        }
        message.truncate(limit);
        // SAFETY: Writable scalar and byte span are supplied by the ABI caller.
        unsafe {
            out_required_bytes.write(message.len() as u64);
            copy_span(message.as_bytes(), buffer, capacity)?;
        }
        Ok(INKPOD_STATUS_OK)
    })
}
