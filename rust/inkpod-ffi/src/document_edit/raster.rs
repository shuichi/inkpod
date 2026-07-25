use super::*;

/// Replaces the active document with a bounded PNG/TIFF/TGA/BMP raster.
///
/// # Safety
/// `core` must be live on its owner thread, `bytes` must identify `byte_count`
/// readable bytes for this call, and `out_info` must be complete writable
/// storage. The UUID pair must not be zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_import_common_raster(
    core: *mut InkpodCore,
    format: u32,
    bytes: *const u8,
    byte_count: u64,
    document_uuid_high: u64,
    document_uuid_low: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_info.is_null() || !is_aligned(out_info) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster import pointer is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        if bytes.is_null() || byte_count == 0 || byte_count > MAX_COMMON_RASTER_BYTES as u64 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster input span is null, empty, or too large",
            );
        }
        let length = match usize::try_from(byte_count) {
            Ok(length) => length,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "common-raster input length is not representable",
                );
            }
        };
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let uuid = (u128::from(document_uuid_high) << 64) | u128::from(document_uuid_low);
        // SAFETY: The exported-function contract requires this bounded span readable.
        let bytes = unsafe { slice::from_raw_parts(bytes, length) };
        // SAFETY: Live owner-thread core and writable output were validated above.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.import_common_raster(format, bytes, uuid) {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Encodes the flattened visible document to a Rust-owned common-raster buffer.
///
/// # Safety
/// `core` must be live on its owner thread. `out_buffer` must be writable
/// storage containing null; the returned handle must be released by
/// `inkpod_byte_buffer_release`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_export_common_raster(
    core: *mut InkpodCore,
    format: u32,
    composite_white: u32,
    out_buffer: *mut *mut InkpodByteBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_buffer.is_null() || !is_aligned(out_buffer) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster export pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable owner storage.
        if !unsafe { out_buffer.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster output already owns a live buffer",
            );
        }
        if composite_white > 1 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster white-composite flag must be zero or one",
            );
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        // SAFETY: Live owner-thread core was validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.export_common_raster(format, composite_white != 0) {
            Ok(bytes) => {
                let handle = Box::new(InkpodByteBuffer {
                    bytes: bytes.into_boxed_slice(),
                });
                // SAFETY: Writable owner storage was validated and currently null.
                unsafe { out_buffer.write(Box::into_raw(handle)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Borrows the immutable byte span owned by a common-raster buffer.
///
/// # Safety
/// `buffer` must be live. Both output pointers must be writable aligned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_byte_buffer_view(
    buffer: *const InkpodByteBuffer,
    out_bytes: *mut *const u8,
    out_byte_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if buffer.is_null()
            || !is_aligned(buffer)
            || out_bytes.is_null()
            || !is_aligned(out_bytes)
            || out_byte_count.is_null()
            || !is_aligned(out_byte_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "byte-buffer view pointer is null or misaligned",
            );
        }
        // SAFETY: Complete live input and writable outputs are required by contract.
        let buffer = unsafe { &*buffer };
        unsafe {
            out_bytes.write(buffer.bytes.as_ptr());
            out_byte_count.write(buffer.bytes.len() as u64);
        }
        INKPOD_STATUS_OK
    })
}

/// Releases one Rust-owned byte buffer and nulls caller storage.
///
/// # Safety
/// `buffer` must be writable storage containing null or one live handle returned
/// by `inkpod_core_export_common_raster`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_byte_buffer_release(buffer: *mut *mut InkpodByteBuffer) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if buffer.is_null() || !is_aligned(buffer) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "byte-buffer owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable unique owner storage.
        let handle = unsafe { buffer.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "byte-buffer handle is misaligned",
            );
        }
        // SAFETY: Null before consuming the unique Box owner exactly once.
        unsafe { buffer.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}
