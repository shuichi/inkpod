use super::*;
use inkpod_io::{
    FileIdentity, FileStamp, RecoveryArtifactProof, RecoveryArtifactStamp, RecoveryIdentity,
    RecoveryIdentityKind, RecoveryMetadata, RecoveryPairProof,
};

// SAFETY: The complete size-prefixed stamp and its embedded identity are readable.
unsafe fn validate_artifact_stamp_record(stamp: &InkpodIoRecoveryArtifactStamp) -> Result<(), u32> {
    // SAFETY: Both records are embedded in a validated live parent input.
    unsafe {
        validate_struct(stamp, "InkpodIoRecoveryArtifactStamp")?;
        validate_struct(&stamp.identity, "InkpodIoFileIdentity")?;
    }
    Ok(())
}

// SAFETY: The complete size-prefixed stamp and its embedded identity are readable.
unsafe fn parse_artifact_stamp(
    stamp: &InkpodIoRecoveryArtifactStamp,
) -> Result<RecoveryArtifactStamp, u32> {
    // SAFETY: Both records are embedded in a validated live proof input.
    unsafe { validate_artifact_stamp_record(stamp)? };
    let object =
        (u128::from(stamp.identity.object_high) << 64) | u128::from(stamp.identity.object_low);
    if stamp.flags & !INKPOD_IO_RECOVERY_ARTIFACT_READONLY != 0
        || stamp.identity.kind != 1
        || (stamp.identity.volume == 0 && object == 0)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "recovery artifact stamp is not an exact physical-file stamp",
        ));
    }
    Ok(RecoveryArtifactStamp {
        identity: FileIdentity {
            volume: stamp.identity.volume,
            file: object,
        },
        length: stamp.length,
        modified: ((u128::from(stamp.modified_high) << 64) | u128::from(stamp.modified_low))
            as i128,
        changed: ((u128::from(stamp.changed_high) << 64) | u128::from(stamp.changed_low)) as i128,
        readonly: stamp.flags & INKPOD_IO_RECOVERY_ARTIFACT_READONLY != 0,
    })
}

// SAFETY: The proof input exposes its advertised complete record.
pub(super) unsafe fn parse_artifact_proof(
    pointer: *const InkpodIoRecoveryArtifactProof,
) -> Result<RecoveryArtifactProof, u32> {
    // SAFETY: Public record exposes a readable size prefix.
    unsafe { validate_struct(pointer, "InkpodIoRecoveryArtifactProof")? };
    // SAFETY: Complete proof range was validated above.
    let proof = unsafe { &*pointer };
    if proof.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "recovery artifact proof reserved field is nonzero",
        ));
    }
    Ok(RecoveryArtifactProof {
        // SAFETY: Embedded stamp records remain live with the proof.
        native: unsafe { parse_artifact_stamp(&proof.native)? },
        // SAFETY: Same embedded-record contract as the native stamp.
        metadata: unsafe { parse_artifact_stamp(&proof.metadata)? },
    })
}

fn artifact_stamp_to_abi(stamp: RecoveryArtifactStamp) -> InkpodIoRecoveryArtifactStamp {
    let modified = stamp.modified as u128;
    let changed = stamp.changed as u128;
    InkpodIoRecoveryArtifactStamp {
        struct_size: size_of::<InkpodIoRecoveryArtifactStamp>() as u32,
        flags: if stamp.readonly {
            INKPOD_IO_RECOVERY_ARTIFACT_READONLY
        } else {
            0
        },
        identity: InkpodIoFileIdentity {
            struct_size: size_of::<InkpodIoFileIdentity>() as u32,
            kind: 1,
            volume: stamp.identity.volume,
            object_high: (stamp.identity.file >> 64) as u64,
            object_low: stamp.identity.file as u64,
        },
        length: stamp.length,
        modified_high: (modified >> 64) as u64,
        modified_low: modified as u64,
        changed_high: (changed >> 64) as u64,
        changed_low: changed as u64,
    }
}

fn artifact_proof_to_abi(proof: RecoveryArtifactProof) -> InkpodIoRecoveryArtifactProof {
    InkpodIoRecoveryArtifactProof {
        struct_size: size_of::<InkpodIoRecoveryArtifactProof>() as u32,
        reserved: 0,
        native: artifact_stamp_to_abi(proof.native),
        metadata: artifact_stamp_to_abi(proof.metadata),
    }
}

fn empty_pair_stamp(kind: u32, identity: FileIdentity) -> InkpodIoRecoveryArtifactStamp {
    InkpodIoRecoveryArtifactStamp {
        struct_size: size_of::<InkpodIoRecoveryArtifactStamp>() as u32,
        flags: 0,
        identity: InkpodIoFileIdentity {
            struct_size: size_of::<InkpodIoFileIdentity>() as u32,
            kind,
            volume: identity.volume,
            object_high: (identity.file >> 64) as u64,
            object_low: identity.file as u64,
        },
        ..InkpodIoRecoveryArtifactStamp::default()
    }
}

fn pair_proof_to_abi(proof: Option<RecoveryPairProof>) -> InkpodIoRecoveryPairProof {
    let zero = FileIdentity { volume: 0, file: 0 };
    match proof {
        None => InkpodIoRecoveryPairProof {
            struct_size: size_of::<InkpodIoRecoveryPairProof>() as u32,
            kind: INKPOD_IO_RECOVERY_PAIR_NONE,
            native: empty_pair_stamp(0, zero),
            raster: empty_pair_stamp(0, zero),
        },
        Some(RecoveryPairProof::Committed { native, raster }) => InkpodIoRecoveryPairProof {
            struct_size: size_of::<InkpodIoRecoveryPairProof>() as u32,
            kind: INKPOD_IO_RECOVERY_PAIR_COMMITTED,
            native: artifact_stamp_to_abi(native.into()),
            raster: artifact_stamp_to_abi(raster.into()),
        },
        Some(RecoveryPairProof::Planned {
            native_missing,
            raster,
        }) => InkpodIoRecoveryPairProof {
            struct_size: size_of::<InkpodIoRecoveryPairProof>() as u32,
            kind: INKPOD_IO_RECOVERY_PAIR_PLANNED,
            native: empty_pair_stamp(2, native_missing),
            raster: artifact_stamp_to_abi(raster.into()),
        },
        Some(RecoveryPairProof::RepairNeeded {
            native,
            raster_missing,
        }) => InkpodIoRecoveryPairProof {
            struct_size: size_of::<InkpodIoRecoveryPairProof>() as u32,
            kind: INKPOD_IO_RECOVERY_PAIR_REPAIR_NEEDED,
            native: artifact_stamp_to_abi(native.into()),
            raster: empty_pair_stamp(2, raster_missing),
        },
    }
}

fn abi_stamp_is_zero(stamp: &InkpodIoRecoveryArtifactStamp) -> bool {
    stamp.flags == 0
        && stamp.identity.kind == 0
        && stamp.identity.volume == 0
        && stamp.identity.object_high == 0
        && stamp.identity.object_low == 0
        && stamp.length == 0
        && stamp.modified_high == 0
        && stamp.modified_low == 0
        && stamp.changed_high == 0
        && stamp.changed_low == 0
}

// SAFETY: Pair proof and both nested size-prefixed stamps are readable.
unsafe fn parse_pair_proof(
    proof: &InkpodIoRecoveryPairProof,
) -> Result<Option<RecoveryPairProof>, u32> {
    // SAFETY: The parent metadata record and embedded proof remain live.
    unsafe {
        validate_struct(proof, "InkpodIoRecoveryPairProof")?;
        validate_artifact_stamp_record(&proof.native)?;
        validate_artifact_stamp_record(&proof.raster)?;
    }
    match proof.kind {
        INKPOD_IO_RECOVERY_PAIR_NONE
            if abi_stamp_is_zero(&proof.native) && abi_stamp_is_zero(&proof.raster) =>
        {
            Ok(None)
        }
        INKPOD_IO_RECOVERY_PAIR_COMMITTED => Ok(Some(RecoveryPairProof::Committed {
            // SAFETY: Nested records were validated above.
            native: FileStamp::from(unsafe { parse_artifact_stamp(&proof.native)? }),
            // SAFETY: Same complete physical-stamp contract.
            raster: FileStamp::from(unsafe { parse_artifact_stamp(&proof.raster)? }),
        })),
        INKPOD_IO_RECOVERY_PAIR_PLANNED => {
            let object = (u128::from(proof.native.identity.object_high) << 64)
                | u128::from(proof.native.identity.object_low);
            if proof.native.flags != 0
                || proof.native.identity.kind != 2
                || proof.native.identity.volume != u64::MAX
                || object == 0
                || proof.native.length != 0
                || proof.native.modified_high != 0
                || proof.native.modified_low != 0
                || proof.native.changed_high != 0
                || proof.native.changed_low != 0
            {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "planned recovery pair native proof is inconsistent",
                ));
            }
            Ok(Some(RecoveryPairProof::Planned {
                native_missing: FileIdentity {
                    volume: u64::MAX,
                    file: object,
                },
                // SAFETY: Nested raster record was validated above.
                raster: FileStamp::from(unsafe { parse_artifact_stamp(&proof.raster)? }),
            }))
        }
        INKPOD_IO_RECOVERY_PAIR_REPAIR_NEEDED => {
            let object = (u128::from(proof.raster.identity.object_high) << 64)
                | u128::from(proof.raster.identity.object_low);
            if proof.raster.flags != 0
                || proof.raster.identity.kind != 2
                || proof.raster.identity.volume != u64::MAX
                || object == 0
                || proof.raster.length != 0
                || proof.raster.modified_high != 0
                || proof.raster.modified_low != 0
                || proof.raster.changed_high != 0
                || proof.raster.changed_low != 0
            {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "repair-needed recovery pair raster proof is inconsistent",
                ));
            }
            Ok(Some(RecoveryPairProof::RepairNeeded {
                // SAFETY: Nested native record was validated above.
                native: FileStamp::from(unsafe { parse_artifact_stamp(&proof.native)? }),
                raster_missing: FileIdentity {
                    volume: u64::MAX,
                    file: object,
                },
            }))
        }
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "recovery pair proof kind or payload is invalid",
        )),
    }
}

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
        // SAFETY: Embedded size-prefixed pair proof remains live with metadata.
        pair_proof: unsafe { parse_pair_proof(&value.pair_proof)? },
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

/// Submits proof-checked cleanup of one obsolete append-only recovery attempt.
/// Changed, missing, or mixed members are retained and reported as a conflict.
/// # Safety
/// Core may be null because cleanup has no document result. When non-null it is
/// on its owner thread; manager, path/proof and empty output are live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_io_recovery_discard_exact_submit(
    core: *mut InkpodCore,
    manager: *mut InkpodIoManager,
    path: *const u8,
    path_bytes: u64,
    proof: *const InkpodIoRecoveryArtifactProof,
    out_job: *mut *mut InkpodIoJob,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller supplies an empty writable owner slot and bounded inputs.
        unsafe { empty_owner(out_job)? };
        if !core.is_null() {
            // SAFETY: A supplied Core remains subject to the ordinary owner-thread contract.
            let _ = unsafe { owner_core(core)? };
        }
        // SAFETY: Both inputs are copied into Rust-owned values before return.
        let path = unsafe { path_from_utf8(path, path_bytes)? }.to_path_buf();
        // SAFETY: Complete proof and nested size-prefixed stamps are readable.
        let proof = unsafe { parse_artifact_proof(proof)? };
        // SAFETY: The live manager is cloned into the accepted worker job.
        let job = FileIoJob::start_recovery_discard_exact(
            unsafe { manager_ref(manager)? }.clone(),
            path,
            proof,
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

/// Copies the one exact proof retained after durable worker publication. It is
/// available at `Ready` before owner final apply; callers publish associated
/// frontend state only after that final apply succeeds.
/// # Safety
/// Job is live and the complete size-prefixed output record is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_io_job_get_recovery_artifact_proof(
    job: *const InkpodIoJob,
    out_proof: *mut InkpodIoRecoveryArtifactProof,
) -> u32 {
    io_boundary(|| {
        // SAFETY: Caller supplies a readable prefix and full writable range.
        unsafe { validate_struct(out_proof, "InkpodIoRecoveryArtifactProof")? };
        // SAFETY: Job lifetime is synchronized against release for this call.
        let job = unsafe { job_lock(job)? };
        let proof = *job.recovery_artifact_proof().map_err(map_core_error)?;
        // SAFETY: Complete writable output was validated before the copy.
        unsafe { out_proof.write(artifact_proof_to_abi(proof)) };
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
        let published = if index == 0 {
            job.published_recovery_metadata().ok()
        } else {
            None
        };
        let candidate = if published.is_none() {
            Some(job.recovery(index).map_err(map_core_error)?)
        } else {
            None
        };
        // SAFETY: The output record, scalar and spans were validated at entry.
        unsafe {
            copy_metadata(
                published.or_else(|| candidate.and_then(|value| value.metadata.as_ref())),
                candidate.map_or(0, |value| value.modified_time_100ns),
                candidate.is_some_and(|value| value.metadata_error.is_some()),
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
        pair_proof: pair_proof_to_abi(metadata.and_then(|data| data.pair_proof)),
    };
    // SAFETY: Complete output record was validated at entry.
    unsafe { out_metadata.write(output) };
    Ok(INKPOD_STATUS_OK)
}
