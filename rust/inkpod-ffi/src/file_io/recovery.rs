use super::*;
use inkpod_io::{RecoveryIdentity, RecoveryIdentityKind, RecoveryMetadata};

// SAFETY: Embedded path exposes its size prefix and advertised UTF-8 bytes.
unsafe fn optional_path(path: &InkpodIoPath) -> Result<String, u32> {
    // SAFETY: Input is a live embedded ABI record.
    unsafe { validate_struct(path, "InkpodIoPath")? };
    if path.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "recovery path reserved field is nonzero",
        ));
    }
    if path.path_bytes == 0 {
        return Ok(String::new());
    }
    // SAFETY: Caller supplies the advertised bounded string range.
    Ok(unsafe { path_from_utf8(path.path, path.path_bytes)? }
        .to_str()
        .ok_or_else(|| fail(INKPOD_STATUS_INVALID_ARGUMENT, "invalid recovery path"))?
        .to_owned())
}

// SAFETY: Record and embedded text spans are readable throughout this call.
pub(super) unsafe fn parse_metadata(
    pointer: *const InkpodIoRecoveryMetadata,
) -> Result<RecoveryMetadata, u32> {
    // SAFETY: Public record exposes a readable size prefix.
    unsafe { validate_struct(pointer, "InkpodIoRecoveryMetadata")? };
    // SAFETY: Complete record was validated above.
    let value = unsafe { &*pointer };
    if value.flags & !1 != 0 || value.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "recovery metadata flags are invalid",
        ));
    }
    let kind = match value.identity_kind {
        0 => RecoveryIdentityKind::None,
        1 => RecoveryIdentityKind::PhysicalFile,
        2 => RecoveryIdentityKind::NormalizedPath,
        3 => RecoveryIdentityKind::Untitled,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "unknown recovery identity kind",
            ));
        }
    };
    let object =
        (u128::from(value.identity_object_high) << 64) | u128::from(value.identity_object_low);
    let metadata = RecoveryMetadata {
        session_id: value.session_id,
        generation: value.generation,
        document_uuid: (u128::from(value.document_uuid_high) << 64)
            | u128::from(value.document_uuid_low),
        written_time_100ns: value.written_time_100ns,
        original_identity: RecoveryIdentity {
            kind,
            volume_serial: value.identity_volume,
            file_id: if kind == RecoveryIdentityKind::PhysicalFile {
                object.to_le_bytes()
            } else {
                [0; 16]
            },
            uuid: if kind == RecoveryIdentityKind::Untitled {
                object
            } else {
                0
            },
            // SAFETY: Bounded text is copied before submission.
            normalized_path: unsafe { optional_path(&value.identity_path)? },
        },
        // SAFETY: Both source strings remain readable until copied.
        original_path: unsafe { optional_path(&value.original_path)? },
        // SAFETY: Same lifetime/bounds contract as the original path.
        source_path: unsafe { optional_path(&value.source_path)? },
    };
    let mut validated = metadata.clone();
    // Autosave may ask the Rust writer to supply the timestamp. The pure codec
    // still rejects an unset timestamp when explicitly asked to encode it.
    if validated.written_time_100ns == 0 {
        validated.written_time_100ns = 1;
    }
    inkpod_io::encode_recovery_metadata(&validated)
        .map_err(|error| map_core_error(error.into()))?;
    Ok(metadata)
}

/// Encodes current-version typed recovery metadata without filesystem access.
/// # Safety
/// Metadata/text input is readable; output size and nonzero-capacity buffer are writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_recovery_metadata_encode(
    metadata: *const InkpodIoRecoveryMetadata,
    buffer: *mut u8,
    capacity: u64,
    out_required_bytes: *mut u64,
) -> u32 {
    io_boundary(|| {
        if out_required_bytes.is_null() || !is_aligned(out_required_bytes) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid encoded metadata size output",
            ));
        }
        // SAFETY: Size-prefixed record/text spans remain readable during parsing.
        let metadata = unsafe { parse_metadata(metadata)? };
        let bytes = inkpod_io::encode_recovery_metadata(&metadata)
            .map_err(|error| map_core_error(error.into()))?;
        // SAFETY: The caller supplies aligned writable scalar storage.
        unsafe { out_required_bytes.write(bytes.len() as u64) };
        if capacity == 0 {
            return Ok(INKPOD_STATUS_OK);
        }
        if buffer.is_null()
            || capacity < bytes.len() as u64
            || capacity > isize::MAX as u64
            || (buffer as usize).checked_add(capacity as usize).is_none()
        {
            return Err(fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "encoded metadata buffer is too small or invalid",
            ));
        }
        // SAFETY: Complete buffer bound was checked and source does not alias output.
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len()) };
        Ok(INKPOD_STATUS_OK)
    })
}

/// Decodes bounded current-version recovery metadata into caller-owned text storage.
/// # Safety
/// Input bytes are readable and do not overlap the writable record/text/size outputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_recovery_metadata_decode(
    bytes: *const u8,
    length: u64,
    out_metadata: *mut InkpodIoRecoveryMetadata,
    text_buffer: *mut u8,
    text_capacity: u64,
    out_required_text_bytes: *mut u64,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Public output advertises a readable prefix and full writable range.
        unsafe { validate_struct(out_metadata, "InkpodIoRecoveryMetadata")? };
        if bytes.is_null()
            || length == 0
            || length > 512 * 1024
            || (bytes as usize).checked_add(length as usize).is_none()
            || out_required_text_bytes.is_null()
            || !is_aligned(out_required_text_bytes)
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid recovery codec span",
            ));
        }
        // SAFETY: The validated length bounds the caller's readable input.
        let metadata = inkpod_io::decode_recovery_metadata(unsafe {
            slice::from_raw_parts(bytes, length as usize)
        })
        .map_err(|error| map_core_error(error.into()))?;
        // SAFETY: Output storage was validated and the caller guarantees nonoverlap.
        unsafe {
            copy_metadata(
                Some(&metadata),
                0,
                false,
                out_metadata,
                text_buffer,
                text_capacity,
                out_required_text_bytes,
            )
        }
    })
}

/// Captures native recovery and its typed sidecar in one Rust-owned file job.
/// # Safety
/// Core is on its owner thread; manager, metadata/path spans and empty output are live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_io_autosave_submit(
    core: *mut InkpodCore,
    manager: *mut InkpodIoManager,
    path: *const u8,
    path_bytes: u64,
    metadata: *const InkpodIoRecoveryMetadata,
    out_job: *mut *mut InkpodIoJob,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller supplies an empty writable owner slot and bounded spans.
        unsafe { empty_owner(out_job)? };
        // SAFETY: Path is copied before any background work.
        let mut request = FileIoRequest::new(
            FileIoKind::Autosave,
            vec![unsafe { path_from_utf8(path, path_bytes)? }.to_path_buf()],
        );
        // SAFETY: Size-prefixed metadata and embedded strings are readable by contract.
        request.recovery_metadata = Some(unsafe { parse_metadata(metadata)? });
        // SAFETY: Live Core affinity/service ownership are validated here.
        let job = FileIoJob::start(
            Some(&unsafe { owner_core(core)? }.core),
            unsafe { manager_ref(manager)? }.clone(),
            request,
        )
        .map_err(map_core_error)?;
        // SAFETY: Unique job ownership transfers to the validated empty slot.
        unsafe {
            out_job.write(Box::into_raw(Box::new(InkpodIoJob {
                job: Mutex::new(job),
                owner_thread: thread::current().id(),
            })))
        };
        Ok(INKPOD_STATUS_OK)
    })
}

/// Copies typed recovery metadata and three packed UTF-8 path spans.
/// # Safety
/// Job is live; record, size scalar and nonzero buffer are writable and nonoverlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_get_recovery_metadata(
    job: *const InkpodIoJob,
    index: u64,
    out_metadata: *mut InkpodIoRecoveryMetadata,
    buffer: *mut u8,
    capacity: u64,
    out_required_bytes: *mut u64,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller supplies a readable prefix and full writable range.
        unsafe { validate_struct(out_metadata, "InkpodIoRecoveryMetadata")? };
        if out_required_bytes.is_null() || !is_aligned(out_required_bytes) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "invalid recovery metadata size output",
            ));
        }
        // SAFETY: Job is synchronized and release cannot race this call.
        let job = unsafe { job_lock(job)? };
        let index = usize::try_from(index)
            .map_err(|_| fail(INKPOD_STATUS_INVALID_ARGUMENT, "recovery index overflows"))?;
        let candidate = job.recovery(index).map_err(map_core_error)?;
        // SAFETY: The output record, scalar and spans were validated at entry.
        unsafe {
            copy_metadata(
                candidate.metadata.as_ref(),
                candidate.modified_time_100ns,
                candidate.metadata_error.is_some(),
                out_metadata,
                buffer,
                capacity,
                out_required_bytes,
            )
        }
    })
}

// SAFETY: Record/size scalar are writable; buffer has the advertised capacity.
unsafe fn copy_metadata(
    metadata: Option<&RecoveryMetadata>,
    modified_time: u64,
    metadata_error: bool,
    out_metadata: *mut InkpodIoRecoveryMetadata,
    buffer: *mut u8,
    capacity: u64,
    out_required_bytes: *mut u64,
) -> Result<u32, u32> {
    let texts = [
        metadata.map_or("", |data| data.original_path.as_str()),
        metadata.map_or("", |data| data.source_path.as_str()),
        metadata.map_or("", |data| data.original_identity.normalized_path.as_str()),
    ];
    let required = texts.iter().map(|text| text.len() as u64).sum::<u64>();
    // SAFETY: Writable aligned scalar was checked above.
    unsafe { out_required_bytes.write(required) };
    if capacity != 0
        && (buffer.is_null()
            || capacity < required
            || capacity > isize::MAX as u64
            || (buffer as usize).checked_add(capacity as usize).is_none())
    {
        return Err(fail(
            INKPOD_STATUS_BUFFER_TOO_SMALL,
            "recovery text buffer is too small or invalid",
        ));
    }
    let mut paths = [InkpodIoPath {
        struct_size: size_of::<InkpodIoPath>() as u32,
        reserved: 0,
        path: ptr::null(),
        path_bytes: 0,
    }; 3];
    let mut offset = 0;
    for (index, text) in texts.iter().enumerate() {
        paths[index].path_bytes = text.len() as u64;
        if capacity != 0 && !text.is_empty() {
            // SAFETY: Packed total fits the buffer and does not overlap source.
            unsafe {
                ptr::copy_nonoverlapping(text.as_ptr(), buffer.add(offset), text.len());
                paths[index].path = buffer.add(offset);
            }
        }
        offset += text.len();
    }
    let identity = metadata.map(|data| &data.original_identity);
    let kind = identity.map_or(0, |identity| identity.kind as u32);
    let object = identity.map_or(0, |identity| {
        if identity.kind == RecoveryIdentityKind::Untitled {
            identity.uuid
        } else {
            u128::from_le_bytes(identity.file_id)
        }
    });
    let uuid = metadata.map_or(0, |data| data.document_uuid);
    let output = InkpodIoRecoveryMetadata {
        struct_size: size_of::<InkpodIoRecoveryMetadata>() as u32,
        flags: u32::from(metadata.is_some()) | (u32::from(metadata_error) << 1),
        session_id: metadata.map_or(0, |data| data.session_id),
        generation: metadata.map_or(0, |data| data.generation),
        document_uuid_high: (uuid >> 64) as u64,
        document_uuid_low: uuid as u64,
        written_time_100ns: metadata.map_or(0, |data| data.written_time_100ns),
        modified_time_100ns: modified_time,
        identity_kind: kind,
        reserved: 0,
        identity_volume: identity.map_or(0, |identity| identity.volume_serial),
        identity_object_high: (object >> 64) as u64,
        identity_object_low: object as u64,
        original_path: paths[0],
        source_path: paths[1],
        identity_path: paths[2],
    };
    // SAFETY: Complete output record was validated at entry.
    unsafe { out_metadata.write(output) };
    Ok(INKPOD_STATUS_OK)
}
