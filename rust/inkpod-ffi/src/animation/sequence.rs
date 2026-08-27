use super::*;

/// Copies a bounded sequence-cell span and naturally sorts it in Core.
///
/// # Safety
/// Core, input, every strided cell record, name, and raster row must remain
/// complete and readable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_set(
    core: *mut InkpodCore,
    input: *const InkpodSequenceInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodSequenceInput") } {
            return status;
        }
        // SAFETY: Complete input was validated above.
        let input = unsafe { &*input };
        if input.reserved != 0
            || input.feature_flags != 0
            || input.cell_count == 0
            || input.cell_count > 10_000
            || input.cells.is_null()
            || !is_aligned(input.cells)
        {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence header is invalid");
        }
        let count = match usize::try_from(input.cell_count) {
            Ok(count) => count,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence count is not representable",
                );
            }
        };
        let stride = match usize::try_from(input.cell_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodSequenceCellInput>()
                    && stride % align_of::<InkpodSequenceCellInput>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence stride is invalid");
            }
        };
        let storage = count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodSequenceCellInput>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence span overflows");
        }
        let mut cells = Vec::with_capacity(count);
        let mut total_raster_bytes = 0_usize;
        for index in 0..count {
            // SAFETY: Checked span makes every record prefix readable.
            let pointer = unsafe {
                input
                    .cells
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodSequenceCellInput>()
            };
            let advertised = match unsafe { validate_struct(pointer, "InkpodSequenceCellInput") } {
                Ok(size) => size,
                Err(status) => return status,
            };
            if advertised as usize > stride {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence cell size exceeds its stride",
                );
            }
            // SAFETY: Complete record was validated above.
            let record = unsafe { &*pointer };
            if record.reserved != 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence cell reserved field is not zero",
                );
            }
            let name = match unsafe { name_from_utf8(record.name_utf8, record.name_bytes) } {
                Ok(name) => name.to_owned(),
                Err(status) => return status,
            };
            let raster = match unsafe { parse_raster_source(&record.source) } {
                Ok(raster) => raster,
                Err(status) => return status,
            };
            total_raster_bytes = match total_raster_bytes.checked_add(raster.pixels.len()) {
                Some(total) if total <= MAX_COMMON_RASTER_BYTES => total,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "sequence raster bytes exceed their cumulative bound",
                    );
                }
            };
            let mut cell = match SequenceCellSource::from_rgba_bytes(
                name,
                raster.document_uuid,
                RgbaRasterBytes {
                    width: raster.width,
                    height: raster.height,
                    pixel_format: raster.pixel_format,
                    dpi_x_milli: raster.dpi_x_milli,
                    dpi_y_milli: raster.dpi_y_milli,
                    pixels: raster.pixels,
                },
            ) {
                Ok(cell) => cell,
                Err(error) => return map_core_error(error),
            };
            cell.source_generation = raster.source_revision;
            cell.frames.reference_frame = raster.reference_frame;
            cells.push(cell);
        }
        // SAFETY: Live owner-thread Core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.set_sequence(cells) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Decodes a bounded naturally sorted sequence of common-raster files.
///
/// # Safety
/// Core and every strided named-byte record/span must remain live and readable
/// for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_import_encoded(
    core: *mut InkpodCore,
    format: u32,
    files: *const InkpodNamedBytesInput,
    file_count: u64,
    file_stride_bytes: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || files.is_null()
            || !is_aligned(files)
            || file_count == 0
            || file_count > 10_000
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "encoded sequence header is invalid",
            );
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let count = match usize::try_from(file_count) {
            Ok(count) => count,
            Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence count overflows"),
        };
        let stride = match usize::try_from(file_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodNamedBytesInput>()
                    && stride % align_of::<InkpodNamedBytesInput>() == 0 =>
            {
                stride
            }
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence stride is invalid"),
        };
        let storage = count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodNamedBytesInput>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence record span overflows",
            );
        }
        let mut decoded = Vec::with_capacity(count);
        let mut total_bytes = 0_usize;
        for index in 0..count {
            // SAFETY: The checked strided span makes every record prefix readable.
            let pointer = unsafe {
                files
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodNamedBytesInput>()
            };
            let advertised = match unsafe { validate_struct(pointer, "InkpodNamedBytesInput") } {
                Ok(size) => size,
                Err(status) => return status,
            };
            if advertised as usize > stride {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence record exceeds stride",
                );
            }
            // SAFETY: Complete record was validated above.
            let record = unsafe { &*pointer };
            if record.reserved != 0
                || record.bytes.is_null()
                || record.byte_count == 0
                || record.byte_count > MAX_COMMON_RASTER_BYTES as u64
            {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence file span is invalid",
                );
            }
            let name = match unsafe { name_from_utf8(record.name_utf8, record.name_bytes) } {
                Ok(name) => name.to_owned(),
                Err(status) => return status,
            };
            let length = match usize::try_from(record.byte_count) {
                Ok(length) => length,
                Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "file length overflows"),
            };
            total_bytes = match total_bytes.checked_add(length) {
                Some(total) if total <= MAX_COMMON_RASTER_BYTES => total,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "sequence bytes exceed bound",
                    );
                }
            };
            // SAFETY: Caller advertises this complete bounded byte span.
            let bytes = unsafe { slice::from_raw_parts(record.bytes, length) }.to_vec();
            decoded.push((name, bytes));
        }
        // SAFETY: Live owner-thread core was validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.import_sequence(format, decoded) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Decodes a bounded naturally sorted sequence whose common-raster format is
/// carried by each input record.
///
/// # Safety
/// Core and every strided named-raster record/span must remain live and readable
/// for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_import_mixed_encoded(
    core: *mut InkpodCore,
    files: *const InkpodNamedRasterInput,
    file_count: u64,
    file_stride_bytes: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || files.is_null()
            || !is_aligned(files)
            || file_count == 0
            || file_count > 10_000
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "mixed encoded sequence header is invalid",
            );
        }
        let count = match usize::try_from(file_count) {
            Ok(count) => count,
            Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence count overflows"),
        };
        let stride = match usize::try_from(file_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodNamedRasterInput>()
                    && stride % align_of::<InkpodNamedRasterInput>() == 0 =>
            {
                stride
            }
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence stride is invalid"),
        };
        let storage = count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodNamedRasterInput>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence record span overflows",
            );
        }
        let mut decoded = Vec::with_capacity(count);
        let mut total_bytes = 0_usize;
        for index in 0..count {
            // SAFETY: The checked strided span makes every record prefix readable.
            let pointer = unsafe {
                files
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodNamedRasterInput>()
            };
            let advertised = match unsafe { validate_struct(pointer, "InkpodNamedRasterInput") } {
                Ok(size) => size,
                Err(status) => return status,
            };
            if advertised as usize > stride {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence record exceeds stride",
                );
            }
            // SAFETY: Complete record was validated above.
            let record = unsafe { &*pointer };
            if record.reserved != 0
                || record.reserved2 != 0
                || record.bytes.is_null()
                || record.byte_count == 0
                || record.byte_count > MAX_COMMON_RASTER_BYTES as u64
            {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "mixed sequence file span is invalid",
                );
            }
            let format = match parse_common_raster_format(record.format) {
                Ok(format) => format,
                Err(status) => return status,
            };
            let name = match unsafe { name_from_utf8(record.name_utf8, record.name_bytes) } {
                Ok(name) => name.to_owned(),
                Err(status) => return status,
            };
            let length = match usize::try_from(record.byte_count) {
                Ok(length) => length,
                Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "file length overflows"),
            };
            total_bytes = match total_bytes.checked_add(length) {
                Some(total) if total <= MAX_COMMON_RASTER_BYTES => total,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "sequence bytes exceed bound",
                    );
                }
            };
            // SAFETY: Caller advertises this complete bounded byte span.
            let bytes = unsafe { slice::from_raw_parts(record.bytes, length) }.to_vec();
            decoded.push((name, format, bytes));
        }
        // SAFETY: Live owner-thread core was validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.import_mixed_sequence(decoded) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Encodes the configured sequence into a Rust-owned immutable file collection.
///
/// # Safety
/// Core must be live on its owner thread and `out_sequence` must be writable
/// null owner storage released by `inkpod_encoded_sequence_release`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_export_encoded(
    core: *mut InkpodCore,
    format: u32,
    composite_white: u32,
    out_sequence: *mut *mut InkpodEncodedSequence,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_sequence.is_null()
            || !is_aligned(out_sequence)
            || composite_white > 1
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence export pointer is invalid",
            );
        }
        // SAFETY: Caller provides readable/writable owner storage.
        if !unsafe { out_sequence.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence output already owns data",
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
        match core.core.export_sequence(format, composite_white != 0) {
            Ok(files) => {
                let files = files
                    .into_iter()
                    .map(|(name, bytes)| EncodedSequenceFile {
                        name: name.into_bytes().into_boxed_slice(),
                        bytes: bytes.into_boxed_slice(),
                    })
                    .collect();
                // SAFETY: Writable null owner storage was validated above.
                unsafe {
                    out_sequence.write(Box::into_raw(Box::new(InkpodEncodedSequence { files })))
                };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Returns the number of encoded files owned by a sequence handle.
///
/// # Safety
/// Handle must be live and output must be writable aligned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_encoded_sequence_count(
    sequence: *const InkpodEncodedSequence,
    out_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if sequence.is_null()
            || !is_aligned(sequence)
            || out_count.is_null()
            || !is_aligned(out_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence count pointer is invalid",
            );
        }
        // SAFETY: Live input and writable output are required by contract.
        unsafe { out_count.write((*sequence).files.len() as u64) };
        INKPOD_STATUS_OK
    })
}

/// Borrows one encoded sequence file name and data span until release.
///
/// # Safety
/// Handle must be live and all output pointers must be writable aligned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_encoded_sequence_get(
    sequence: *const InkpodEncodedSequence,
    index: u64,
    out_name: *mut *const u8,
    out_name_bytes: *mut u64,
    out_bytes: *mut *const u8,
    out_byte_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if sequence.is_null()
            || !is_aligned(sequence)
            || out_name.is_null()
            || !is_aligned(out_name)
            || out_name_bytes.is_null()
            || !is_aligned(out_name_bytes)
            || out_bytes.is_null()
            || !is_aligned(out_bytes)
            || out_byte_count.is_null()
            || !is_aligned(out_byte_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence view pointer is invalid",
            );
        }
        // SAFETY: Live handle was validated above.
        let sequence = unsafe { &*sequence };
        let Some(file) = sequence.files.get(index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence file index is outside bounds",
            );
        };
        // SAFETY: All output storage is writable and aligned by contract.
        unsafe {
            out_name.write(file.name.as_ptr());
            out_name_bytes.write(file.name.len() as u64);
            out_bytes.write(file.bytes.as_ptr());
            out_byte_count.write(file.bytes.len() as u64);
        }
        INKPOD_STATUS_OK
    })
}

/// Releases an encoded sequence handle and nulls caller storage.
///
/// # Safety
/// Owner storage must contain null or exactly one live handle from export.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_encoded_sequence_release(
    sequence: *mut *mut InkpodEncodedSequence,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if sequence.is_null() || !is_aligned(sequence) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence owner pointer is invalid",
            );
        }
        // SAFETY: Caller provides readable/writable unique owner storage.
        let handle = unsafe { sequence.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence handle is misaligned",
            );
        }
        // SAFETY: Null first, then consume the unique Box owner exactly once.
        unsafe { sequence.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Returns one naturally ordered sequence cell and deterministic thumbnail metadata.
///
/// # Safety
/// Core/output must be complete live owner-thread records and the optional name
/// buffer must be writable for its advertised capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_cell_get(
    core: *mut InkpodCore,
    index: u32,
    output: *mut InkpodSequenceCellInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodSequenceCellInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let cell = match core.core.sequence_cell(index as usize) {
            Ok(cell) => cell,
            Err(error) => return map_core_error(error),
        };
        output.flags = 0;
        output.sequence_index = u64::from(index);
        output.document_uuid_high = (cell.document_uuid >> 64) as u64;
        output.document_uuid_low = cell.document_uuid as u64;
        output.cell_number = cell.cell_number;
        output.width = cell.width;
        output.height = cell.height;
        output.thumbnail_width = cell.thumbnail.width;
        output.thumbnail_height = cell.thumbnail.height;
        output.reserved = 0;
        output.thumbnail_checksum = cell.thumbnail.checksum;
        output.name_bytes = cell.name.len() as u64;
        if output.name_capacity == 0 {
            return if output.name_utf8.is_null() {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity name buffer must be null",
                )
            };
        }
        if output.name_utf8.is_null() || output.name_capacity < output.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "sequence name buffer is too small",
            );
        }
        // SAFETY: Caller advertises sufficient writable name capacity.
        unsafe { ptr::copy_nonoverlapping(cell.name.as_ptr(), output.name_utf8, cell.name.len()) };
        INKPOD_STATUS_OK
    })
}

/// Copies one tightly packed straight-alpha RGBA8 sequence thumbnail.
///
/// # Safety
/// Core/output must be complete live owner-thread records and the optional pixel
/// buffer must be writable for its advertised capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_thumbnail_get(
    core: *mut InkpodCore,
    index: u32,
    output: *mut InkpodSequenceThumbnailBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodSequenceThumbnailBuffer") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if output.flags != 0 || output.reserved != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence thumbnail flags are invalid",
            );
        }
        let thumbnail = match core.core.sequence_cell(index as usize) {
            Ok(cell) => cell.thumbnail,
            Err(error) => return map_core_error(error),
        };
        let required = thumbnail.rgba8.len() as u64;
        output.width = thumbnail.width;
        output.height = thumbnail.height;
        output.stride_bytes = thumbnail.width.saturating_mul(4);
        output.checksum = thumbnail.checksum;
        output.required_bytes = required;
        if output.pixel_capacity == 0 {
            return if output.pixels_rgba8.is_null() {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity thumbnail buffer must be null",
                )
            };
        }
        if output.pixels_rgba8.is_null() || output.pixel_capacity < required {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "sequence thumbnail buffer is too small",
            );
        }
        // SAFETY: Caller advertises sufficient writable pixel capacity.
        unsafe {
            ptr::copy_nonoverlapping(
                thumbnail.rgba8.as_ptr(),
                output.pixels_rgba8,
                thumbnail.rgba8.len(),
            )
        };
        INKPOD_STATUS_OK
    })
}

/// Switches to a sequence cell by natural-order index without discarding dirty data.
///
/// # Safety
/// Core and document-info output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_activate(
    core: *mut InkpodCore,
    index: u32,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.sequence_activate(index as usize) {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Captures an immutable source/target/revision token for a sequence switch.
///
/// # Safety
/// Core and request output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_switch_request(
    core: *mut InkpodCore,
    index: u32,
    policy: u32,
    out_request: *mut InkpodSequenceSwitchRequest,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(out_request.cast_const(), "InkpodSequenceSwitchRequest") }
        {
            return status;
        }
        let policy = match parse_sequence_switch_policy(policy) {
            Ok(policy) => policy,
            Err(status) => return status,
        };
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_request };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.sequence_switch_request(index as usize, policy) {
            Ok(request) => {
                write_sequence_switch_request(output, request);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits a validated autosave-before-switch token after durable source save.
///
/// # Safety
/// Core, request, and document-info output must be complete live owner-thread
/// records. The request is borrowed only for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_commit_autosaved_switch(
    core: *mut InkpodCore,
    request: *const InkpodSequenceSwitchRequest,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(request, "InkpodSequenceSwitchRequest") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let request = match parse_sequence_switch_request(unsafe { &*request }) {
            Ok(request) => request,
            Err(status) => return status,
        };
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.sequence_commit_autosaved_switch(request) {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Restores the requested target from one exact native recovery artifact.
///
/// # Safety
/// Core/request/output must be complete live owner-thread records and the UTF-8
/// path span must remain readable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_restore_autosaved_switch(
    core: *mut InkpodCore,
    request: *const InkpodSequenceSwitchRequest,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(request, "InkpodSequenceSwitchRequest") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records and readable path span are required by contract.
        let request = match parse_sequence_switch_request(unsafe { &*request }) {
            Ok(request) => request,
            Err(status) => return status,
        };
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.sequence_restore_autosaved_switch(request, path) {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

fn parse_sequence_switch_policy(policy: u32) -> Result<SequenceSwitchPolicy, u32> {
    match policy {
        INKPOD_SEQUENCE_SWITCH_PROMPT => Ok(SequenceSwitchPolicy::Prompt),
        INKPOD_SEQUENCE_SWITCH_AUTOSAVE => Ok(SequenceSwitchPolicy::AutosaveBeforeSwitch),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "sequence switch policy is not defined",
        )),
    }
}

pub(crate) fn parse_sequence_switch_request(
    input: &InkpodSequenceSwitchRequest,
) -> Result<SequenceSwitchRequest, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE
        || input.flags & !INKPOD_SEQUENCE_SWITCH_REQUIRED != 0
    {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "sequence switch request contains unsupported flags",
        ));
    }
    let request = SequenceSwitchRequest {
        policy: parse_sequence_switch_policy(input.policy)?,
        source_document_uuid: (u128::from(input.source_document_uuid_high) << 64)
            | u128::from(input.source_document_uuid_low),
        source_generation: input.source_generation,
        source_document_revision: input.source_document_revision,
        source_editor_revision: input.source_editor_revision,
        target_document_uuid: (u128::from(input.target_document_uuid_high) << 64)
            | u128::from(input.target_document_uuid_low),
        target_source_generation: input.target_source_generation,
        target_index: input.target_index,
    };
    let required = if request.requires_switch() {
        INKPOD_SEQUENCE_SWITCH_REQUIRED
    } else {
        0
    };
    if input.flags != required {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "sequence switch request flags do not match its identities",
        ));
    }
    Ok(request)
}

fn write_sequence_switch_request(
    output: &mut InkpodSequenceSwitchRequest,
    request: SequenceSwitchRequest,
) {
    output.policy = match request.policy {
        SequenceSwitchPolicy::Prompt => INKPOD_SEQUENCE_SWITCH_PROMPT,
        SequenceSwitchPolicy::AutosaveBeforeSwitch => INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
    };
    output.feature_flags = INKPOD_FEATURE_NONE;
    output.source_document_uuid_high = (request.source_document_uuid >> 64) as u64;
    output.source_document_uuid_low = request.source_document_uuid as u64;
    output.source_generation = request.source_generation;
    output.source_document_revision = request.source_document_revision;
    output.source_editor_revision = request.source_editor_revision;
    output.target_document_uuid_high = (request.target_document_uuid >> 64) as u64;
    output.target_document_uuid_low = request.target_document_uuid as u64;
    output.target_source_generation = request.target_source_generation;
    output.target_index = request.target_index;
    output.flags = if request.requires_switch() {
        INKPOD_SEQUENCE_SWITCH_REQUIRED
    } else {
        0
    };
}

fn parse_sequence_endpoint_policy(value: u32) -> Result<SequenceEndpointPolicy, u32> {
    match value {
        INKPOD_SEQUENCE_ENDPOINT_STOP => Ok(SequenceEndpointPolicy::Stop),
        INKPOD_SEQUENCE_ENDPOINT_WRAP => Ok(SequenceEndpointPolicy::Wrap),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "sequence endpoint policy is not defined",
        )),
    }
}

fn parse_sequence_step_result(value: u32) -> Result<SequenceStepResult, u32> {
    match value {
        INKPOD_SEQUENCE_STEP_EMPTY => Ok(SequenceStepResult::Empty),
        INKPOD_SEQUENCE_STEP_SINGLE_CELL => Ok(SequenceStepResult::SingleCell),
        INKPOD_SEQUENCE_STEP_STOPPED => Ok(SequenceStepResult::Stopped),
        INKPOD_SEQUENCE_STEP_ADVANCED => Ok(SequenceStepResult::Advanced),
        INKPOD_SEQUENCE_STEP_WRAPPED => Ok(SequenceStepResult::Wrapped),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "sequence step result is not defined",
        )),
    }
}

fn parse_sequence_step_plan(input: &InkpodSequenceStepPlan) -> Result<SequenceStepPlan, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "sequence step plan contains unsupported flags",
        ));
    }
    let direction = parse_sequence_direction(input.direction)?;
    let endpoint_policy = parse_sequence_endpoint_policy(input.endpoint_policy)?;
    let result = parse_sequence_step_result(input.result_class)?;
    if result == SequenceStepResult::Empty {
        if input.sequence_revision != 0
            || input.source_index != INKPOD_SEQUENCE_INDEX_NONE
            || input.target_index != INKPOD_SEQUENCE_INDEX_NONE
            || input.source_document_uuid_high != 0
            || input.source_document_uuid_low != 0
            || input.source_generation != 0
            || input.target_document_uuid_high != 0
            || input.target_document_uuid_low != 0
            || input.target_generation != 0
            || input.source_cell_number != 0
            || input.target_cell_number != 0
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "empty sequence step plan contains an identity",
            ));
        }
        return Ok(SequenceStepPlan {
            direction,
            endpoint_policy,
            result,
            sequence_revision: 0,
            source_index: None,
            target_index: None,
            source_document_uuid: None,
            source_generation: None,
            target_document_uuid: None,
            target_generation: None,
            source_cell_number: None,
            target_cell_number: None,
        });
    }
    let source_document_uuid = (u128::from(input.source_document_uuid_high) << 64)
        | u128::from(input.source_document_uuid_low);
    let target_document_uuid = (u128::from(input.target_document_uuid_high) << 64)
        | u128::from(input.target_document_uuid_low);
    if input.sequence_revision == 0
        || input.source_index == INKPOD_SEQUENCE_INDEX_NONE
        || input.target_index == INKPOD_SEQUENCE_INDEX_NONE
        || source_document_uuid == 0
        || target_document_uuid == 0
        || input.source_generation == 0
        || input.target_generation == 0
        || input.source_cell_number == 0
        || input.target_cell_number == 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "sequence step plan identity is incomplete",
        ));
    }
    Ok(SequenceStepPlan {
        direction,
        endpoint_policy,
        result,
        sequence_revision: input.sequence_revision,
        source_index: Some(input.source_index),
        target_index: Some(input.target_index),
        source_document_uuid: Some(source_document_uuid),
        source_generation: Some(input.source_generation),
        target_document_uuid: Some(target_document_uuid),
        target_generation: Some(input.target_generation),
        source_cell_number: Some(input.source_cell_number),
        target_cell_number: Some(input.target_cell_number),
    })
}

fn write_sequence_step_plan(output: &mut InkpodSequenceStepPlan, plan: SequenceStepPlan) {
    output.direction = match plan.direction {
        SequenceDirection::Previous => INKPOD_SEQUENCE_PREVIOUS,
        SequenceDirection::Next => INKPOD_SEQUENCE_NEXT,
    };
    output.endpoint_policy = match plan.endpoint_policy {
        SequenceEndpointPolicy::Stop => INKPOD_SEQUENCE_ENDPOINT_STOP,
        SequenceEndpointPolicy::Wrap => INKPOD_SEQUENCE_ENDPOINT_WRAP,
    };
    output.result_class = match plan.result {
        SequenceStepResult::Empty => INKPOD_SEQUENCE_STEP_EMPTY,
        SequenceStepResult::SingleCell => INKPOD_SEQUENCE_STEP_SINGLE_CELL,
        SequenceStepResult::Stopped => INKPOD_SEQUENCE_STEP_STOPPED,
        SequenceStepResult::Advanced => INKPOD_SEQUENCE_STEP_ADVANCED,
        SequenceStepResult::Wrapped => INKPOD_SEQUENCE_STEP_WRAPPED,
    };
    output.feature_flags = INKPOD_FEATURE_NONE;
    output.sequence_revision = plan.sequence_revision;
    let source_uuid = plan.source_document_uuid.unwrap_or(0);
    output.source_document_uuid_high = (source_uuid >> 64) as u64;
    output.source_document_uuid_low = source_uuid as u64;
    output.source_generation = plan.source_generation.unwrap_or(0);
    let target_uuid = plan.target_document_uuid.unwrap_or(0);
    output.target_document_uuid_high = (target_uuid >> 64) as u64;
    output.target_document_uuid_low = target_uuid as u64;
    output.target_generation = plan.target_generation.unwrap_or(0);
    output.source_index = plan.source_index.unwrap_or(INKPOD_SEQUENCE_INDEX_NONE);
    output.target_index = plan.target_index.unwrap_or(INKPOD_SEQUENCE_INDEX_NONE);
    output.source_cell_number = plan.source_cell_number.unwrap_or(0);
    output.target_cell_number = plan.target_cell_number.unwrap_or(0);
}

/// Registers one sequence cell as the exact-depth subpalette source.
///
/// # Safety
/// Core must be live on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_subpalette_set(core: *mut InkpodCore, index: u32) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.set_subpalette_cell(index as usize) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples one exact-depth subpalette pixel.
///
/// # Safety
/// Core/output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_subpalette_sample(
    core: *mut InkpodCore,
    x: u32,
    y: u32,
    output: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(output.cast_const(), "InkpodColorValue") } {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.subpalette_sample(x, y) {
            Ok(color) => match write_color_value(output, color) {
                Ok(()) => INKPOD_STATUS_OK,
                Err(status) => status,
            },
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a view-only command to the registered subpalette source.
///
/// # Safety
/// Core and input must be complete live owner-thread records. `view_id` must
/// identify a live secondary view created by this Core.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_subpalette_view_apply(
    core: *mut InkpodCore,
    view_id: u64,
    input: *const InkpodViewInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodViewInput") } {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let command = match parse_view_command(&core.core, input) {
            Ok(command) => command,
            Err(status) => return status,
        };
        match core.core.apply_subpalette_view_for(view_id, command) {
            Ok(_) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples one exact-depth subpalette pixel through its independent view.
///
/// # Safety
/// Core/output must be complete live owner-thread records and `view_id` must
/// identify a live secondary view.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_subpalette_view_sample(
    core: *mut InkpodCore,
    view_id: u64,
    device_x: f64,
    device_y: f64,
    output: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(output.cast_const(), "InkpodColorValue") } {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core
            .core
            .subpalette_view_sample(view_id, device_x, device_y)
        {
            Ok(color) => match write_color_value(output, color) {
                Ok(()) => INKPOD_STATUS_OK,
                Err(status) => status,
            },
            Err(error) => map_core_error(error),
        }
    })
}

/// Builds one Rust-owned immutable snapshot of the registered subpalette source.
///
/// # Safety
/// Core/options/output must be complete live owner-thread records. The returned
/// owner must be released with `inkpod_snapshot_release`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_subpalette_build_snapshot(
    core: *mut InkpodCore,
    view_id: u64,
    options: *const InkpodSnapshotOptions,
    out_snapshot: *mut *mut InkpodSnapshot,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_snapshot.is_null()
            || !is_aligned(out_snapshot)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "subpalette snapshot pointer is invalid",
            );
        }
        if let Err(status) = unsafe { validate_struct(options, "InkpodSnapshotOptions") } {
            return status;
        }
        // SAFETY: Caller provides writable output handle storage.
        unsafe { out_snapshot.write(ptr::null_mut()) };
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let options = unsafe { &*options };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "snapshot options contain unsupported values",
            );
        }
        match core.core.build_subpalette_snapshot_for(view_id) {
            Ok(snapshot) => {
                // SAFETY: Output storage receives exactly one Rust Box owner.
                unsafe { out_snapshot.write(Box::into_raw(snapshot_handle(snapshot))) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Switches to a previous/next naturally ordered sequence cell.
///
/// # Safety
/// Core and document-info output must be complete live records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_step(
    core: *mut InkpodCore,
    direction: u32,
    flags: u32,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if flags & !INKPOD_SEQUENCE_FLAG_LOOP != 0 {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence flags are invalid");
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        let direction = match parse_sequence_direction(direction) {
            Ok(direction) => direction,
            Err(status) => return status,
        };
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core
            .core
            .sequence_step(direction, flags & INKPOD_SEQUENCE_FLAG_LOOP != 0)
        {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Resolves one previous/next command into an immutable identity and revision plan.
///
/// Empty, single-cell, stopped, adjacent, and wrapped outcomes are explicit and
/// resolving never changes document or editor state.
///
/// # Safety
/// Core and plan output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_step_resolve(
    core: *mut InkpodCore,
    direction: u32,
    endpoint_policy: u32,
    out_plan: *mut InkpodSequenceStepPlan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(out_plan.cast_const(), "InkpodSequenceStepPlan") }
        {
            return status;
        }
        let direction = match parse_sequence_direction(direction) {
            Ok(direction) => direction,
            Err(status) => return status,
        };
        let endpoint_policy = match parse_sequence_endpoint_policy(endpoint_policy) {
            Ok(policy) => policy,
            Err(status) => return status,
        };
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_plan };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.resolve_sequence_step(direction, endpoint_policy) {
            Ok(plan) => {
                write_sequence_step_plan(output, plan);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits a previously resolved sequence-step plan if it is still current.
///
/// The borrowed plan is re-resolved by Core. A stale identity/revision, dirty
/// switching source, invalid field, or failure leaves document and editor state unchanged.
///
/// # Safety
/// Core, borrowed plan, and document-info output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_step_commit(
    core: *mut InkpodCore,
    plan: *const InkpodSequenceStepPlan,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(plan, "InkpodSequenceStepPlan") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete records were validated and inputs are borrowed for this call.
        let plan = match parse_sequence_step_plan(unsafe { &*plan }) {
            Ok(plan) => plan,
            Err(status) => return status,
        };
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.commit_sequence_step(plan) {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Starts motion check at the active or first sequence cell.
///
/// # Safety
/// Core, input, and frame output must be complete live records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_motion_check_start(
    core: *mut InkpodCore,
    input: *const InkpodMotionCheckInput,
    out_frame: *mut InkpodMotionFrame,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodMotionCheckInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(out_frame.cast_const(), "InkpodMotionFrame") }
        {
            return status;
        }
        // SAFETY: Complete records were validated above.
        let input = unsafe { &*input };
        if input.flags
            & !(INKPOD_MOTION_FLAG_LOOP
                | INKPOD_MOTION_FLAG_INCLUDE_SELECTION
                | INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE)
            != 0
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "motion-check flags are invalid",
            );
        }
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_frame };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.motion_check_start(MotionCheckConfig {
            fps: input.fps,
            loop_playback: input.flags & INKPOD_MOTION_FLAG_LOOP != 0,
            include_selection: input.flags & INKPOD_MOTION_FLAG_INCLUDE_SELECTION != 0,
            include_light_table: input.flags & INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE != 0,
        }) {
            Ok(frame) => {
                write_motion_frame(output, frame);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Steps an active motion-check session.
///
/// # Safety
/// Core and frame output must be complete live records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_motion_check_step(
    core: *mut InkpodCore,
    direction: u32,
    out_frame: *mut InkpodMotionFrame,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_frame.cast_const(), "InkpodMotionFrame") }
        {
            return status;
        }
        let direction = match parse_sequence_direction(direction) {
            Ok(direction) => direction,
            Err(status) => return status,
        };
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_frame };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.motion_check_step(direction) {
            Ok(frame) => {
                write_motion_frame(output, frame);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Stops motion check. It is idempotent.
///
/// # Safety
/// Core must be live on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_motion_check_stop(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread Core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        core.core.motion_check_stop();
        INKPOD_STATUS_OK
    })
}

/// Toggles pause for an active motion-check session and returns its frame.
///
/// # Safety
/// Core/output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_motion_check_toggle_pause(
    core: *mut InkpodCore,
    out_frame: *mut InkpodMotionFrame,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_frame.cast_const(), "InkpodMotionFrame") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_frame };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.motion_check_toggle_pause() {
            Ok(frame) => {
                write_motion_frame(output, frame);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}
