use super::*;

pub struct InkpodColorChartFile {
    chart: FileColorChart,
}

fn application_color(value: PixelValue) -> Result<ApplicationColor, u32> {
    match value {
        PixelValue::Rgba([red, green, blue, alpha]) => Ok(ApplicationColor {
            depth: INKPOD_COLOR_DEPTH_8,
            red: u16::from(red),
            green: u16::from(green),
            blue: u16::from(blue),
            alpha: u16::from(alpha),
        }),
        PixelValue::Rgba16([red, green, blue, alpha]) => Ok(ApplicationColor {
            depth: INKPOD_COLOR_DEPTH_16,
            red,
            green,
            blue,
            alpha,
        }),
        PixelValue::Binary(_) | PixelValue::Grayscale8(_) | PixelValue::Grayscale16(_) => {
            Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "application palette colors must be RGBA8 or RGBA16",
            ))
        }
    }
}

fn pixel_value(value: ApplicationColor) -> Result<PixelValue, u32> {
    match value.depth {
        INKPOD_COLOR_DEPTH_8 => Ok(PixelValue::Rgba([
            value.red as u8,
            value.green as u8,
            value.blue as u8,
            value.alpha as u8,
        ])),
        INKPOD_COLOR_DEPTH_16 => Ok(PixelValue::Rgba16([
            value.red,
            value.green,
            value.blue,
            value.alpha,
        ])),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "application color depth is unsupported",
        )),
    }
}

fn map_format_error(error: impl Into<CoreError>) -> u32 {
    map_core_error(error.into())
}

/// Saves an application palette through the exact-current Rust codec.
///
/// # Safety
///
/// `path_utf8` must address `path_bytes` readable bytes for this call. `input`
/// must address a valid `InkpodColorArray`; its color storage must satisfy that
/// record's documented count, stride, alignment, and readable-lifetime rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_palette_file_save(
    path_utf8: *const u8,
    path_bytes: u64,
    input: *const InkpodColorArray,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) = unsafe { validate_struct(input, "InkpodColorArray") } {
            return status;
        }
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        let colors = match unsafe { parse_color_array(&*input) } {
            Ok(colors) => colors,
            Err(status) => return status,
        };
        let colors = match colors.into_iter().map(application_color).collect() {
            Ok(colors) => colors,
            Err(status) => return status,
        };
        match save_palette_atomic(path, &FilePalette { colors }) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_format_error(error),
        }
    })
}

/// Loads an application palette through the exact-current Rust codec.
///
/// # Safety
///
/// `path_utf8` must address `path_bytes` readable bytes for this call. `buffer`
/// must address a writable `InkpodColorBuffer`; when it supplies storage, that
/// storage must remain writable for the declared capacity and stride until the
/// call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_palette_file_load(
    path_utf8: *const u8,
    path_bytes: u64,
    buffer: *mut InkpodColorBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if let Err(status) = unsafe { validate_struct(buffer.cast_const(), "InkpodColorBuffer") } {
            return status;
        }
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        let palette = match read_palette(path) {
            Ok(palette) => palette,
            Err(error) => return map_format_error(error),
        };
        let buffer = unsafe { &mut *buffer };
        if buffer.reserved != 0 || buffer.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "palette file buffer contains unsupported flags or reserved values",
            );
        }
        buffer.color_count = palette.colors.len() as u64;
        if buffer.color_capacity == 0 {
            if !buffer.colors.is_null() || buffer.color_stride_bytes != 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "a palette file count query must use null storage and zero stride",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if buffer.color_capacity > MAX_PALETTE_COLOR_COUNT
            || buffer.colors.is_null()
            || !is_aligned(buffer.colors)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "palette file output capacity or storage is invalid",
            );
        }
        let stride = match usize::try_from(buffer.color_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodColorValue>()
                    && stride % align_of::<InkpodColorValue>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "palette file output stride is invalid",
                );
            }
        };
        if buffer.color_capacity < palette.colors.len() as u64 {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "palette file output capacity is smaller than color_count",
            );
        }
        let storage = palette
            .colors
            .len()
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodColorValue>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "palette file output storage size overflows",
            );
        }
        for (index, color) in palette.colors.into_iter().enumerate() {
            let record = match pixel_value(color).and_then(color_value_record) {
                Ok(record) => record,
                Err(status) => return status,
            };
            unsafe {
                buffer
                    .colors
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodColorValue>()
                    .write(record);
            }
        }
        INKPOD_STATUS_OK
    })
}

/// Saves a named color chart through the exact-current Rust codec.
///
/// # Safety
///
/// `path_utf8` must address `path_bytes` readable bytes for this call. When
/// `entry_count` is nonzero, `entries` must address that many readable
/// `InkpodColorChartEntry` records at `entry_stride_bytes`; every entry's name
/// pointer must address its declared readable byte length until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_color_chart_file_save(
    path_utf8: *const u8,
    path_bytes: u64,
    entries: *const InkpodColorChartEntry,
    entry_count: u64,
    entry_stride_bytes: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        if entry_count > MAX_PALETTE_COLOR_COUNT {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "color chart entry count exceeds the bounded limit",
            );
        }
        let count = match usize::try_from(entry_count) {
            Ok(count) => count,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "color chart entry count is not representable",
                );
            }
        };
        if count == 0 {
            if !entries.is_null() || entry_stride_bytes != 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "an empty color chart must use null storage and zero stride",
                );
            }
        } else if entries.is_null() || !is_aligned(entries) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "color chart entry storage is null or misaligned",
            );
        }
        let stride = match usize::try_from(entry_stride_bytes) {
            Ok(stride)
                if count == 0
                    || (stride >= size_of::<InkpodColorChartEntry>()
                        && stride % align_of::<InkpodColorChartEntry>() == 0) =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "color chart entry stride is invalid",
                );
            }
        };
        if count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodColorChartEntry>()))
            .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "color chart entry storage size overflows",
            );
        }
        let mut chart_entries = Vec::with_capacity(count);
        for index in 0..count {
            let entry = unsafe {
                entries
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodColorChartEntry>()
            };
            let struct_size = match unsafe { validate_struct(entry, "InkpodColorChartEntry") } {
                Ok(size) => size,
                Err(status) => return status,
            };
            let entry = unsafe { &*entry };
            if u64::from(struct_size) > entry_stride_bytes
                || entry.reserved != 0
                || entry.feature_flags != INKPOD_FEATURE_NONE
            {
                return fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "color chart entry contains unsupported fields",
                );
            }
            let color = match unsafe { parse_color_value(&raw const entry.color) }
                .and_then(application_color)
            {
                Ok(color) => color,
                Err(status) => return status,
            };
            let name_bytes = match usize::try_from(entry.name_bytes) {
                Ok(length) if length > 0 && length <= MAX_COLOR_CHART_NAME_BYTES => length,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "color chart name length is outside bounds",
                    );
                }
            };
            if entry.name_utf8.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "color chart name storage is null",
                );
            }
            let name = match std::str::from_utf8(unsafe {
                slice::from_raw_parts(entry.name_utf8, name_bytes)
            }) {
                Ok(name) => name.to_owned(),
                Err(_) => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "color chart name is not UTF-8",
                    );
                }
            };
            chart_entries.push(FileColorChartEntry { color, name });
        }
        match save_color_chart_atomic(
            path,
            &FileColorChart {
                entries: chart_entries,
            },
        ) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_format_error(error),
        }
    })
}

/// Loads a named color chart into a Rust-owned opaque handle.
///
/// # Safety
///
/// `path_utf8` must address `path_bytes` readable bytes for this call.
/// `out_chart` must be aligned, writable, and initially contain null. On
/// success, the caller must release the returned handle exactly once with
/// [`inkpod_color_chart_file_release`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_color_chart_file_load(
    path_utf8: *const u8,
    path_bytes: u64,
    out_chart: *mut *mut InkpodColorChartFile,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_chart.is_null() || !is_aligned(out_chart) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "color chart owner pointer is null or misaligned",
            );
        }
        if !unsafe { out_chart.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "color chart output already owns a handle",
            );
        }
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        match read_color_chart(path) {
            Ok(chart) => {
                unsafe { out_chart.write(Box::into_raw(Box::new(InkpodColorChartFile { chart }))) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_format_error(error),
        }
    })
}

/// Returns the number of entries in a loaded color chart.
///
/// # Safety
///
/// `chart` must be a live handle returned by
/// [`inkpod_color_chart_file_load`] and must not be released concurrently.
/// `out_count` must be aligned and writable for one `u64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_color_chart_file_count(
    chart: *const InkpodColorChartFile,
    out_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if chart.is_null() || !is_aligned(chart) || out_count.is_null() || !is_aligned(out_count) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "color chart handle or count output is null or misaligned",
            );
        }
        unsafe { out_count.write((&*chart).chart.entries.len() as u64) };
        INKPOD_STATUS_OK
    })
}

/// Copies one color-chart entry and its UTF-8 name.
///
/// # Safety
///
/// `chart` must be a live handle returned by
/// [`inkpod_color_chart_file_load`] and must not be released concurrently.
/// `out_color` and `out_name_bytes` must address writable records. When
/// `name_capacity` is nonzero, `name_utf8` must address that many writable
/// bytes until return; a size query uses null storage and zero capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_color_chart_file_get(
    chart: *const InkpodColorChartFile,
    index: u64,
    out_color: *mut InkpodColorValue,
    name_utf8: *mut u8,
    name_capacity: u64,
    out_name_bytes: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if chart.is_null() || !is_aligned(chart) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "color chart handle is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") }
        {
            return status;
        }
        if out_name_bytes.is_null() || !is_aligned(out_name_bytes) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "color chart name length output is null or misaligned",
            );
        }
        let entry = match usize::try_from(index)
            .ok()
            .and_then(|index| unsafe { &*chart }.chart.entries.get(index))
        {
            Some(entry) => entry,
            None => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "color chart index is outside bounds",
                );
            }
        };
        let color = match pixel_value(entry.color).and_then(color_value_record) {
            Ok(color) => color,
            Err(status) => return status,
        };
        unsafe {
            out_color.write(color);
            out_name_bytes.write(entry.name.len() as u64);
        }
        if name_capacity == 0 {
            if !name_utf8.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "a color chart name size query must use null storage",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if name_utf8.is_null() || name_capacity < entry.name.len() as u64 {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "color chart name output is null or too small",
            );
        }
        unsafe { ptr::copy_nonoverlapping(entry.name.as_ptr(), name_utf8, entry.name.len()) };
        INKPOD_STATUS_OK
    })
}

/// Releases a Rust-owned color-chart handle and nulls its owner slot.
///
/// # Safety
///
/// `chart` must be an aligned, writable owner slot containing a live handle
/// returned by [`inkpod_color_chart_file_load`]. No concurrent access to the
/// handle or owner slot is permitted. Each successful load is released exactly
/// once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_color_chart_file_release(
    chart: *mut *mut InkpodColorChartFile,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if chart.is_null() || !is_aligned(chart) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "color chart owner pointer is null or misaligned",
            );
        }
        let handle = unsafe { chart.read() };
        if handle.is_null() || !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "color chart handle is null or misaligned",
            );
        }
        unsafe {
            drop(Box::from_raw(handle));
            chart.write(ptr::null_mut());
        }
        INKPOD_STATUS_OK
    })
}
