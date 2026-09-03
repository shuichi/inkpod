use super::*;

pub(super) fn kind(value: u32) -> Result<FileIoKind, u32> {
    Ok(match value {
        INKPOD_IO_OPEN_NATIVE => FileIoKind::OpenNative,
        INKPOD_IO_OPEN_RECOVERY => FileIoKind::OpenRecovery,
        INKPOD_IO_OPEN_RASTER => FileIoKind::OpenRaster,
        INKPOD_IO_SEQUENCE_AUTO => FileIoKind::SequenceAuto,
        INKPOD_IO_SEQUENCE_FILES => FileIoKind::SequenceFiles,
        INKPOD_IO_REFERENCE_FILES => FileIoKind::ReferenceFiles,
        INKPOD_IO_REFERENCE_FOLDER => FileIoKind::ReferenceFolder,
        INKPOD_IO_LIGHT_TABLE_ADD => FileIoKind::LightTableAdd,
        INKPOD_IO_LIGHT_TABLE_RELOAD => FileIoKind::LightTableReload,
        INKPOD_IO_SAVE_PAIR => FileIoKind::SavePair,
        INKPOD_IO_AUTOSAVE => FileIoKind::Autosave,
        INKPOD_IO_EXPORT_RASTER => FileIoKind::ExportRaster,
        INKPOD_IO_BATCH_PLAN => FileIoKind::BatchPlan,
        INKPOD_IO_BATCH_RUN => FileIoKind::BatchRun,
        INKPOD_IO_BATCH_PREVIEW => FileIoKind::BatchPreview,
        INKPOD_IO_RECOVERY_LIST => FileIoKind::RecoveryList,
        INKPOD_IO_RECOVERY_DISCARD => FileIoKind::RecoveryDiscard,
        INKPOD_IO_RECOVERY_PROBE => FileIoKind::RecoveryProbe,
        INKPOD_IO_EXPORT_SEQUENCE => FileIoKind::ExportSequence,
        INKPOD_IO_SEQUENCE_SWITCH => FileIoKind::SequenceSwitch,
        INKPOD_IO_COMPACTED_COPY => FileIoKind::CompactedCopy,
        INKPOD_IO_OPEN_RASTER_PAIR => FileIoKind::OpenRasterPair,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "unknown I/O operation kind",
            ));
        }
    })
}

pub(super) fn kind_code(kind: FileIoKind) -> u32 {
    match kind {
        FileIoKind::OpenNative => INKPOD_IO_OPEN_NATIVE,
        FileIoKind::OpenRecovery => INKPOD_IO_OPEN_RECOVERY,
        FileIoKind::OpenRaster => INKPOD_IO_OPEN_RASTER,
        FileIoKind::SequenceAuto => INKPOD_IO_SEQUENCE_AUTO,
        FileIoKind::SequenceFiles => INKPOD_IO_SEQUENCE_FILES,
        FileIoKind::ReferenceFiles => INKPOD_IO_REFERENCE_FILES,
        FileIoKind::ReferenceFolder => INKPOD_IO_REFERENCE_FOLDER,
        FileIoKind::LightTableAdd => INKPOD_IO_LIGHT_TABLE_ADD,
        FileIoKind::LightTableReload => INKPOD_IO_LIGHT_TABLE_RELOAD,
        FileIoKind::SavePair => INKPOD_IO_SAVE_PAIR,
        FileIoKind::Autosave => INKPOD_IO_AUTOSAVE,
        FileIoKind::ExportRaster => INKPOD_IO_EXPORT_RASTER,
        FileIoKind::BatchPlan => INKPOD_IO_BATCH_PLAN,
        FileIoKind::BatchRun => INKPOD_IO_BATCH_RUN,
        FileIoKind::BatchPreview => INKPOD_IO_BATCH_PREVIEW,
        FileIoKind::RecoveryList => INKPOD_IO_RECOVERY_LIST,
        FileIoKind::RecoveryDiscard => INKPOD_IO_RECOVERY_DISCARD,
        FileIoKind::RecoveryProbe => INKPOD_IO_RECOVERY_PROBE,
        FileIoKind::ExportSequence => INKPOD_IO_EXPORT_SEQUENCE,
        FileIoKind::SequenceSwitch => INKPOD_IO_SEQUENCE_SWITCH,
        FileIoKind::CompactedCopy => INKPOD_IO_COMPACTED_COPY,
        FileIoKind::OpenRasterPair => INKPOD_IO_OPEN_RASTER_PAIR,
    }
}

pub(super) fn format_code(format: CommonRasterFormat) -> u32 {
    match format {
        CommonRasterFormat::Png => INKPOD_COMMON_RASTER_PNG,
        CommonRasterFormat::Tiff => INKPOD_COMMON_RASTER_TIFF,
        CommonRasterFormat::Tga => INKPOD_COMMON_RASTER_TGA,
        CommonRasterFormat::Bmp => INKPOD_COMMON_RASTER_BMP,
    }
}

// SAFETY: The request and its advertised spans are readable until this call returns.
pub(super) unsafe fn request(pointer: *const InkpodIoRequest) -> Result<FileIoRequest, u32> {
    // SAFETY: Size-prefix readability is part of the caller contract.
    unsafe { validate_struct(pointer, "InkpodIoRequest")? };
    // SAFETY: Complete struct range was validated above.
    let request = unsafe { &*pointer };
    if request.flags
        & !(INKPOD_IO_FORCE_RELOAD
            | INKPOD_IO_COMPOSITE_WHITE
            | INKPOD_IO_OVERWRITE_CONFIRMED
            | INKPOD_IO_REVERT_CURRENT)
        != 0
        || request.reserved != 0
    {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "I/O flags or reserved values are unsupported",
        ));
    }
    let kind = kind(request.kind)?;
    let count = usize::try_from(request.path_count)
        .ok()
        .filter(|count| (1..=10_000).contains(count))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "I/O path count is outside bounds",
            )
        })?;
    let stride = usize::try_from(request.path_stride_bytes)
        .ok()
        .filter(|stride| {
            *stride >= size_of::<InkpodIoPath>() && *stride % align_of::<InkpodIoPath>() == 0
        })
        .ok_or_else(|| fail(INKPOD_STATUS_INVALID_ARGUMENT, "I/O path stride is invalid"))?;
    let span = stride
        .checked_mul(count)
        .filter(|span| *span <= isize::MAX as usize)
        .ok_or_else(|| fail(INKPOD_STATUS_INVALID_ARGUMENT, "I/O path span overflows"))?;
    if request.paths.is_null()
        || !is_aligned(request.paths)
        || (request.paths as usize).checked_add(span).is_none()
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "I/O path span is null or misaligned",
        ));
    }
    let mut paths = Vec::with_capacity(count);
    let mut total_bytes = 0_u64;
    for index in 0..count {
        // SAFETY: Advertised array span/stride have checked bounds and alignment.
        let path = unsafe {
            request
                .paths
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodIoPath>()
        };
        // SAFETY: Each element exposes a readable size prefix by contract.
        unsafe { validate_struct(path, "InkpodIoPath")? };
        // SAFETY: The element's whole struct was validated above.
        let path = unsafe { &*path };
        if path.reserved != 0 || path.struct_size as usize > stride {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "I/O path record does not fit its stride",
            ));
        }
        total_bytes = total_bytes
            .checked_add(path.path_bytes)
            .filter(|bytes| *bytes <= 16 * 1024 * 1024)
            .ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "I/O path text exceeds its aggregate limit",
                )
            })?;
        // SAFETY: Caller supplies the bounded UTF-8 span; parsing copies it now.
        paths.push(unsafe { path_from_utf8(path.path, path.path_bytes)? }.to_path_buf());
    }
    let mut parsed = FileIoRequest::new(kind, paths);
    parsed.force_reload = request.flags & INKPOD_IO_FORCE_RELOAD != 0;
    parsed.revert_current = request.flags & INKPOD_IO_REVERT_CURRENT != 0;
    parsed.composite_white = request.flags & INKPOD_IO_COMPOSITE_WHITE != 0;
    parsed.overwrite_confirmed = request.flags & INKPOD_IO_OVERWRITE_CONFIRMED != 0;
    parsed.object_id = request.object_id;
    parsed.document_uuid =
        (u128::from(request.document_uuid_high) << 64) | u128::from(request.document_uuid_low);
    parsed.raster_format = if request.raster_format == 0 {
        None
    } else {
        Some(parse_common_raster_format(request.raster_format)?)
    };
    Ok(parsed)
}
