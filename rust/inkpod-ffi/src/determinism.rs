use super::*;

/// Copies the immutable build/replay contract on the Core owner thread.
///
/// # Safety
/// `core` must be a live aligned Core handle and `output` must expose a complete,
/// writable, non-overlapping `InkpodReplayContract`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_replay_contract(
    core: *mut InkpodCore,
    output: *mut InkpodReplayContract,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(output.cast_const(), "InkpodReplayContract") }
        {
            return status;
        }
        let core = unsafe { &*core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let contract = inkpod_core::replay_contract();
        let output = unsafe { &mut *output };
        output.replay_epoch = contract.replay_epoch().get();
        output.procedure_format_version = contract.procedure_format_version();
        output.canonical_numeric_version = contract.canonical_numeric_version();
        output.primitive_count = contract.primitive_count();
        output.reserved = 0;
        output.feature_flags = INKPOD_FEATURE_NONE;
        output.primitive_catalog_digest = *contract.primitive_catalog_digest();
        INKPOD_STATUS_OK
    })
}

/// Copies a view-independent canonical digest of an immutable snapshot.
///
/// # Safety
/// `snapshot` must be live and externally synchronized; `output` must expose a
/// complete writable, non-overlapping `InkpodCanonicalDigest`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_canonical_digest(
    snapshot: *const InkpodSnapshot,
    output: *mut InkpodCanonicalDigest,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodCanonicalDigest") }
        {
            return status;
        }
        let snapshot = unsafe { &*snapshot };
        let digest = match snapshot.snapshot.canonical_composite_digest() {
            Ok(digest) => digest,
            Err(error) => return map_core_error(error),
        };
        let output = unsafe { &mut *output };
        output.algorithm = INKPOD_DIGEST_BLAKE3_256;
        output.bytes = digest.as_bytes();
        INKPOD_STATUS_OK
    })
}
