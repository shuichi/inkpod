use super::*;

pub(crate) fn write_subpalette_info(
    output: &mut InkpodSubpaletteInfo,
    info: SubpaletteCatalogInfo,
) {
    output.item_count = info.item_count;
    output.catalog_revision = info.catalog_revision;
    output.active_index = info.active_index.unwrap_or(INKPOD_SUBPALETTE_INDEX_NONE);
    output.reserved = 0;
    output.flags = (if info.image_loaded {
        INKPOD_SUBPALETTE_INFO_IMAGE_LOADED
    } else {
        INKPOD_FEATURE_NONE
    }) | if info.cache_complete {
        INKPOD_SUBPALETTE_INFO_CACHE_COMPLETE
    } else {
        INKPOD_FEATURE_NONE
    };
}

fn validate_subpalette(
    subpalette: *mut InkpodSubpalette,
) -> Result<&'static mut InkpodSubpalette, u32> {
    if subpalette.is_null() || !is_aligned(subpalette) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "subpalette is null or misaligned",
        ));
    }
    // SAFETY: The caller contract requires a live handle returned by create.
    let subpalette = unsafe { &mut *subpalette };
    let status = validate_subpalette_thread(subpalette);
    if status != INKPOD_STATUS_OK {
        return Err(status);
    }
    Ok(subpalette)
}

fn parse_subpalette_view(input: &InkpodViewInput) -> Result<ViewCommand, u32> {
    if input.flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "subpalette view input contains unsupported flags",
        ));
    }
    match input.kind {
        INKPOD_VIEW_PAN_BY => Ok(ViewCommand::PanBy {
            device_dx: input.value1,
            device_dy: input.value2,
        }),
        INKPOD_VIEW_ZOOM_AT => Ok(ViewCommand::ZoomAt {
            factor: input.value1,
            device_x: input.value2,
            device_y: input.value3,
        }),
        INKPOD_VIEW_FIT => Ok(ViewCommand::Fit {
            viewport_width: input.value1,
            viewport_height: input.value2,
        }),
        INKPOD_VIEW_ONE_TO_ONE => Ok(ViewCommand::OneToOne {
            viewport_width: input.value1,
            viewport_height: input.value2,
        }),
        INKPOD_VIEW_VIEWPORT_RESIZED => Ok(ViewCommand::ViewportResized {
            viewport_width: input.value1,
            viewport_height: input.value2,
        }),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "view command is not supported by a subpalette",
        )),
    }
}

/// Creates one empty owner-thread-affined external-image subpalette.
///
/// # Safety
/// `out_subpalette` must be writable storage for one handle pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_create(
    out_subpalette: *mut *mut InkpodSubpalette,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_subpalette.is_null() || !is_aligned(out_subpalette) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "subpalette output owner is null or misaligned",
            );
        }
        // SAFETY: Writable owner storage is required by contract.
        unsafe { out_subpalette.write(ptr::null_mut()) };
        let catalog = match SubpaletteCatalog::new() {
            Ok(catalog) => catalog,
            Err(error) => return map_core_error(error),
        };
        let handle = Box::new(InkpodSubpalette {
            owner_thread: thread::current().id(),
            catalog,
        });
        // SAFETY: The validated owner storage receives one Box owner.
        unsafe { out_subpalette.write(Box::into_raw(handle)) };
        INKPOD_STATUS_OK
    })
}

/// Releases a subpalette and nulls its owner pointer. A repeated null release is a no-op.
///
/// # Safety
/// The owner must contain null or the unique live handle returned by create.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_release(owner: *mut *mut InkpodSubpalette) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if owner.is_null() || !is_aligned(owner) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "subpalette owner is null or misaligned",
            );
        }
        // SAFETY: Readable owner storage is required by contract.
        let handle = unsafe { owner.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "subpalette handle is misaligned",
            );
        }
        // SAFETY: A live unique handle is required by contract.
        let status = validate_subpalette_thread(unsafe { &*handle });
        if status != INKPOD_STATUS_OK {
            return status;
        }
        // SAFETY: Nulling precedes consuming the unique Box owner.
        unsafe { owner.write(ptr::null_mut()) };
        // SAFETY: Ownership originated in Box::into_raw and is consumed once.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Atomically replaces external source metadata and clears the decoded selection.
///
/// # Safety
/// Every strided record and UTF-8 span must remain readable for this call only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_replace_sources(
    subpalette: *mut InkpodSubpalette,
    inputs: *const InkpodSubpaletteSourceInput,
    input_count: u64,
    input_stride_bytes: u64,
    out_info: *mut InkpodSubpaletteInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodSubpaletteInfo") }
        {
            return status;
        }
        let count = match usize::try_from(input_count) {
            Ok(count) if count > 0 && count <= inkpod_core::MAX_SUBPALETTE_ITEMS => count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "subpalette source count is outside bounds",
                );
            }
        };
        let stride = match usize::try_from(input_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodSubpaletteSourceInput>()
                    && stride % align_of::<InkpodSubpaletteSourceInput>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "subpalette source stride is invalid",
                );
            }
        };
        if inputs.is_null() || !is_aligned(inputs) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "subpalette source records are null or misaligned",
            );
        }
        let total_bytes = match (count - 1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodSubpaletteSourceInput>()))
        {
            Some(total) => total,
            None => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "source span overflows"),
        };
        if total_bytes > isize::MAX as usize {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "source span is too large");
        }

        let mut sources = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: Validated base, stride, count, and caller-provided readable span cover record.
            let record_ptr = unsafe { inputs.cast::<u8>().add(index * stride) }
                .cast::<InkpodSubpaletteSourceInput>();
            if let Err(status) =
                unsafe { validate_struct(record_ptr, "InkpodSubpaletteSourceInput") }
            {
                return status;
            }
            // SAFETY: Complete record readability is required by the public contract.
            let record = unsafe { &*record_ptr };
            if record.reserved != 0 || record.source_token == 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "subpalette source contains invalid reserved values or token",
                );
            }
            let name_len = match usize::try_from(record.name_bytes) {
                Ok(length) if length > 0 && (length as u64) <= MAX_NODE_NAME_BYTES => length,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "subpalette source name length is outside bounds",
                    );
                }
            };
            if record.name_utf8.is_null() {
                return fail(INKPOD_STATUS_INVALID_ARGUMENT, "source name is null");
            }
            // SAFETY: The caller contract exposes exactly name_bytes readable bytes.
            let name_bytes = unsafe { slice::from_raw_parts(record.name_utf8, name_len) };
            let name = match std::str::from_utf8(name_bytes) {
                Ok(name) => name.to_owned(),
                Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "source name is not UTF-8"),
            };
            sources.push(SubpaletteSource {
                source_token: record.source_token,
                name,
            });
        }

        let info = match subpalette.catalog.replace_sources(sources) {
            Ok(info) => info,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: The complete output record was validated above.
        write_subpalette_info(unsafe { &mut *out_info }, info);
        INKPOD_STATUS_OK
    })
}

/// Clears all sources and any decoded image without resetting identity authority.
///
/// # Safety
/// Handle and output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_clear(
    subpalette: *mut InkpodSubpalette,
    out_info: *mut InkpodSubpaletteInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodSubpaletteInfo") }
        {
            return status;
        }
        let info = match subpalette.catalog.clear() {
            Ok(info) => info,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Complete writable output was validated above.
        write_subpalette_info(unsafe { &mut *out_info }, info);
        INKPOD_STATUS_OK
    })
}

/// Queries the current catalog without changing selection or view state.
///
/// # Safety
/// Handle and output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_get_info(
    subpalette: *mut InkpodSubpalette,
    out_info: *mut InkpodSubpaletteInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodSubpaletteInfo") }
        {
            return status;
        }
        // SAFETY: Complete writable output was validated above.
        write_subpalette_info(unsafe { &mut *out_info }, subpalette.catalog.info());
        INKPOD_STATUS_OK
    })
}

/// Queries one naturally ordered item metadata record.
///
/// # Safety
/// Handle and output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_item_get(
    subpalette: *mut InkpodSubpalette,
    index: u32,
    out_item: *mut InkpodSubpaletteItemInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_item.cast_const(), "InkpodSubpaletteItemInfo") }
        {
            return status;
        }
        let item = match subpalette.catalog.item(index as usize) {
            Ok(item) => item,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Complete writable output was validated above.
        let output = unsafe { &mut *out_item };
        output.flags = if item.cell_number.is_some() {
            INKPOD_SUBPALETTE_ITEM_HAS_CELL_NUMBER
        } else {
            0
        };
        output.item_id = item.id.get();
        output.source_token = item.source_token;
        output.cell_number = item.cell_number.unwrap_or(0);
        output.reserved = 0;
        output.name_bytes = item.name.len() as u64;
        INKPOD_STATUS_OK
    })
}

/// Copies one item's UTF-8 display name without a trailing NUL.
///
/// # Safety
/// Handle and `out_written` must be live; a nonzero capacity requires a writable buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_item_name_copy(
    subpalette: *mut InkpodSubpalette,
    index: u32,
    buffer: *mut u8,
    capacity: u64,
    out_written: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if out_written.is_null() || !is_aligned(out_written) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "name length output is invalid",
            );
        }
        let item = match subpalette.catalog.item(index as usize) {
            Ok(item) => item,
            Err(error) => return map_core_error(error),
        };
        let required = item.name.len() as u64;
        // SAFETY: Writable scalar output is required by contract.
        unsafe { out_written.write(required) };
        if capacity < required {
            return INKPOD_STATUS_BUFFER_TOO_SMALL;
        }
        if required > 0 && buffer.is_null() {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "name buffer is null");
        }
        // SAFETY: Capacity covers the exact source length and ranges may not overlap by contract.
        unsafe { ptr::copy_nonoverlapping(item.name.as_ptr(), buffer, item.name.len()) };
        INKPOD_STATUS_OK
    })
}

/// Resolves a previous or next catalog item without changing the active image.
///
/// # Safety
/// Handle and item-ID output must be live owner-thread values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_adjacent_item(
    subpalette: *mut InkpodSubpalette,
    direction: u32,
    out_item_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if out_item_id.is_null() || !is_aligned(out_item_id) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "item ID output is invalid");
        }
        let direction = match direction {
            INKPOD_SEQUENCE_PREVIOUS => SequenceDirection::Previous,
            INKPOD_SEQUENCE_NEXT => SequenceDirection::Next,
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "direction is not defined"),
        };
        let item = match subpalette.catalog.adjacent_item(direction) {
            Ok(item) => item,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Writable scalar output is required by contract.
        unsafe { out_item_id.write(item.id.get()) };
        INKPOD_STATUS_OK
    })
}

/// Decodes one borrowed common-raster byte span and atomically selects its catalog item.
///
/// # Safety
/// Handle, byte span, and output must remain live for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_load_common_raster(
    subpalette: *mut InkpodSubpalette,
    item_id: u64,
    format: u32,
    bytes: *const u8,
    byte_count: u64,
    out_info: *mut InkpodSubpaletteInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodSubpaletteInfo") }
        {
            return status;
        }
        let item_id = match SubpaletteItemId::from_raw(item_id) {
            Some(item_id) => item_id,
            None => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "item ID is zero"),
        };
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let length = match usize::try_from(byte_count) {
            Ok(length) if length > 0 && length <= MAX_COMMON_RASTER_BYTES => length,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "raster byte count is outside bounds",
                );
            }
        };
        if bytes.is_null() {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "raster bytes are null");
        }
        // SAFETY: Caller exposes byte_count readable bytes for this call.
        let owned = unsafe { slice::from_raw_parts(bytes, length) }.to_vec();
        let info = match subpalette.catalog.load_image(item_id, format, owned) {
            Ok(info) => info,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Complete writable output was validated above.
        write_subpalette_info(unsafe { &mut *out_info }, info);
        INKPOD_STATUS_OK
    })
}

/// Decodes one complete borrowed mixed-format image span into a memory-resident cache.
///
/// # Safety
/// Handle, every strided record/byte span, and output must remain live for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_load_cached_rasters(
    subpalette: *mut InkpodSubpalette,
    inputs: *const InkpodSubpaletteRasterInput,
    input_count: u64,
    input_stride_bytes: u64,
    active_item_id: u64,
    out_info: *mut InkpodSubpaletteInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodSubpaletteInfo") }
        {
            return status;
        }
        let active_item_id = match SubpaletteItemId::from_raw(active_item_id) {
            Some(item_id) => item_id,
            None => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "active item ID is zero"),
        };
        let count = match usize::try_from(input_count) {
            Ok(count) if count > 0 && count <= inkpod_core::MAX_SUBPALETTE_ITEMS => count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "subpalette cache input count is outside bounds",
                );
            }
        };
        let stride = match usize::try_from(input_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodSubpaletteRasterInput>()
                    && stride % align_of::<InkpodSubpaletteRasterInput>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "subpalette cache input stride is invalid",
                );
            }
        };
        if inputs.is_null() || !is_aligned(inputs) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "subpalette cache records are null or misaligned",
            );
        }
        let total_record_bytes = match (count - 1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodSubpaletteRasterInput>()))
        {
            Some(total) if total <= isize::MAX as usize => total,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "cache record span overflows",
                );
            }
        };
        let _ = total_record_bytes;

        let mut borrowed = Vec::with_capacity(count);
        let mut encoded_bytes = 0_u64;
        for index in 0..count {
            // SAFETY: Validated base, stride, count, and readable caller span cover this record.
            let record_ptr = unsafe { inputs.cast::<u8>().add(index * stride) }
                .cast::<InkpodSubpaletteRasterInput>();
            if let Err(status) =
                unsafe { validate_struct(record_ptr, "InkpodSubpaletteRasterInput") }
            {
                return status;
            }
            // SAFETY: Complete record readability is required by the public contract.
            let record = unsafe { &*record_ptr };
            let item_id = match SubpaletteItemId::from_raw(record.item_id) {
                Some(item_id) => item_id,
                None => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "cache item ID is zero"),
            };
            let format = match parse_common_raster_format(record.format) {
                Ok(format) => format,
                Err(status) => return status,
            };
            let length = match usize::try_from(record.byte_count) {
                Ok(length) if length > 0 && length <= MAX_COMMON_RASTER_BYTES => length,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "cached raster byte count is outside bounds",
                    );
                }
            };
            encoded_bytes = match encoded_bytes.checked_add(record.byte_count) {
                Some(total) if total <= inkpod_core::MAX_SUBPALETTE_CACHE_BYTES => total,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "encoded subpalette cache exceeds its aggregate byte bound",
                    );
                }
            };
            if record.bytes.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "cached raster bytes are null",
                );
            }
            // SAFETY: Caller exposes byte_count readable bytes for this call only.
            let bytes = unsafe { slice::from_raw_parts(record.bytes, length) };
            borrowed.push(SubpaletteImageInput {
                item_id,
                format,
                bytes,
            });
        }

        let info = match subpalette
            .catalog
            .load_cached_images(&borrowed, active_item_id)
        {
            Ok(info) => info,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Complete writable output was validated above.
        write_subpalette_info(unsafe { &mut *out_info }, info);
        INKPOD_STATUS_OK
    })
}

/// Selects one already-decoded cached item without encoded input.
///
/// # Safety
/// Handle and output must remain live for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_select_cached_raster(
    subpalette: *mut InkpodSubpalette,
    item_id: u64,
    out_info: *mut InkpodSubpaletteInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodSubpaletteInfo") }
        {
            return status;
        }
        let item_id = match SubpaletteItemId::from_raw(item_id) {
            Some(item_id) => item_id,
            None => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "item ID is zero"),
        };
        let info = match subpalette.catalog.select_cached_image(item_id) {
            Ok(info) => info,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Complete writable output was validated above.
        write_subpalette_info(unsafe { &mut *out_info }, info);
        INKPOD_STATUS_OK
    })
}

/// Applies a view-only command to the private decoded image.
///
/// # Safety
/// Handle and input must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_view_apply(
    subpalette: *mut InkpodSubpalette,
    input: *const InkpodViewInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(input, "InkpodViewInput") } {
            return status;
        }
        // SAFETY: Complete readable input was validated above.
        let command = match parse_subpalette_view(unsafe { &*input }) {
            Ok(command) => command,
            Err(status) => return status,
        };
        match subpalette.catalog.apply_view(command) {
            Ok(_) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples one exact-depth color through subpalette device-pixel coordinates.
///
/// # Safety
/// Handle and output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_sample(
    subpalette: *mut InkpodSubpalette,
    device_x: f64,
    device_y: f64,
    out_color: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") }
        {
            return status;
        }
        let color = match subpalette.catalog.sample(device_x, device_y) {
            Ok(color) => color,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Complete writable output was validated above.
        match write_color_value(unsafe { &mut *out_color }, color) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(status) => status,
        }
    })
}

/// Builds a Rust-owned immutable snapshot of the decoded external image.
///
/// # Safety
/// Handle/options/output must be complete live owner-thread values. Release the result with
/// `inkpod_snapshot_release` on any externally synchronized thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_subpalette_build_snapshot(
    subpalette: *mut InkpodSubpalette,
    options: *const InkpodSnapshotOptions,
    out_snapshot: *mut *mut InkpodSnapshot,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let subpalette = match validate_subpalette(subpalette) {
            Ok(subpalette) => subpalette,
            Err(status) => return status,
        };
        if out_snapshot.is_null() || !is_aligned(out_snapshot) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "snapshot owner is invalid");
        }
        // SAFETY: Writable output owner is required by contract.
        unsafe { out_snapshot.write(ptr::null_mut()) };
        if let Err(status) = unsafe { validate_struct(options, "InkpodSnapshotOptions") } {
            return status;
        }
        // SAFETY: Complete options record was validated above.
        let options = unsafe { &*options };
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "snapshot options contain unsupported values",
            );
        }
        let snapshot = match subpalette.catalog.build_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Output receives exactly one Rust Box owner.
        unsafe { out_snapshot.write(Box::into_raw(snapshot_handle(snapshot))) };
        INKPOD_STATUS_OK
    })
}
