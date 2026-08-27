use super::job::{Discovery, Prepared};
use super::model::*;
use crate::{
    CommonRasterFormat, Core, CoreError, DocumentSaveSnapshot, LightTableItemInfo,
    LightTableItemInput, LightTableSource, RectI32, SequenceCellSource,
};
use inkpod_io::{FileIdentity, IoManager, JobContext, LoadedImage};
use std::path::Path;

const MAX_NATIVE_BYTES: u64 = 1_073_741_824;

pub(super) fn validate_request(request: &FileIoRequest) -> Result<(), CoreError> {
    let multiple = matches!(
        request.kind,
        FileIoKind::SequenceFiles | FileIoKind::ReferenceFiles
    );
    let maximum = if multiple {
        10_000
    } else if matches!(
        request.kind,
        FileIoKind::RecoveryProbe | FileIoKind::ExportSequence
    ) {
        2
    } else {
        1
    };
    if request.paths.is_empty()
        || request.paths.len() > maximum
        || request.paths.iter().any(|path| {
            path.as_os_str().is_empty()
                || path
                    .to_str()
                    .is_none_or(|text| text.len() > 32_768 || text.contains('\0'))
        })
        || request
            .paths
            .iter()
            .map(|path| path.as_os_str().len())
            .sum::<usize>()
            > 16 * 1024 * 1024
    {
        return Err(CoreError::InvalidArgument(
            "file request paths exceed bounds",
        ));
    }
    if request.kind == FileIoKind::LightTableReload && request.object_id == 0 {
        return Err(CoreError::InvalidArgument(
            "light-table reload requires an item ID",
        ));
    }
    if request.kind == FileIoKind::SavePair
        && !request.paths[0]
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("inkpod"))
    {
        return Err(CoreError::InvalidArgument(
            "normal save destination must use .inkpod",
        ));
    }
    if request.kind == FileIoKind::ExportRaster && request.raster_format.is_none() {
        return Err(CoreError::InvalidArgument(
            "raster export format is missing",
        ));
    }
    Ok(())
}

pub(super) fn discover(
    manager: &IoManager,
    request: &FileIoRequest,
    context: &JobContext,
) -> Result<Discovery, CoreError> {
    if request.kind == FileIoKind::SequenceAuto {
        let found = manager.discover_sequence(&request.paths[0], context)?;
        Ok(Discovery {
            paths: found.paths,
            seed: Some(found.seed_index),
            truncated: found.truncated,
        })
    } else {
        let mut paths: Vec<_> = manager
            .list_files(&request.paths[0], 100_000, context)?
            .into_iter()
            .filter(|path| raster_format(path).is_some())
            .collect();
        if paths.len() > 10_000 {
            return Err(CoreError::InvalidArgument(
                "reference folder exceeds 10000 images",
            ));
        }
        paths.sort_by(|left, right| {
            crate::animation::natural_cmp(
                left.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(""),
                right
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(""),
            )
        });
        Ok(Discovery {
            paths,
            seed: None,
            truncated: false,
        })
    }
}

pub(super) fn raster_format(path: &Path) -> Option<CommonRasterFormat> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(CommonRasterFormat::from_extension)
}

fn source_uuid(identity: FileIdentity) -> u128 {
    let mut hash = blake3::Hasher::new();
    hash.update(b"inkpod runtime file source UUID v1");
    hash.update(&identity.volume.to_le_bytes());
    hash.update(&identity.file.to_le_bytes());
    let mut uuid = [0_u8; 16];
    uuid.copy_from_slice(&hash.finalize().as_bytes()[..16]);
    u128::from_le_bytes(uuid).max(1)
}

pub(super) fn native(
    manager: &IoManager,
    request: &FileIoRequest,
    context: &JobContext,
) -> Result<(Prepared, Vec<FileIoItem>), CoreError> {
    let path = &request.paths[0];
    manager.recover_pairs(path, context)?;
    let (native, stamp) =
        manager.with_file_locks(std::slice::from_ref(path), context, |files| {
            let stamp = files.metadata(path)?;
            let file = files.with_reader(path, MAX_NATIVE_BYTES, |reader| {
                use std::io::{Read, Seek, SeekFrom};
                let mut magic = [0_u8; 8];
                reader.read_exact(&mut magic)?;
                reader.seek(SeekFrom::Start(0))?;
                if magic == *b"INKCUT\0\0" && request.kind == FileIoKind::OpenNative {
                    return Ok(None);
                }
                Ok(Some(inkpod_format::read_procedure_from_reader(
                    reader,
                    || context.is_cancelled(),
                )?))
            })?;
            if stamp != files.metadata(path)? {
                return Err(inkpod_io::IoError::ChangedDuringRead);
            }
            Ok((file, stamp))
        })?;
    context.check_cancelled()?;
    let Some(native) = native else {
        return Ok((
            Prepared::CutDescriptor,
            vec![FileIoItem {
                path: path.clone(),
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_owned(),
                format: None,
                identity: stamp.identity,
                identity_physical: true,
                source_generation: 1,
                document_uuid: 0,
            }],
        ));
    };
    let mut staged = Core::from_native_file(native, request.kind == FileIoKind::OpenRecovery)?;
    let uuid = staged.document_info()?.document_uuid;
    if request.kind == FileIoKind::OpenNative {
        let companion = path.with_extension(format_extension(staged.raster_file_format()?));
        let raster = if manager.exists(&companion, context)? {
            Some(manager.metadata(&companion, context)?)
        } else {
            None
        };
        staged.io_pair_authority = Some(SavedPair {
            native_path: path.clone(),
            native: stamp,
            raster,
        });
    }
    context.check_cancelled()?;
    context.record_loaded();
    let recovery = if request.kind == FileIoKind::OpenRecovery {
        let result = manager.read_recovery_metadata(path, context);
        let mut metadata_error = result.as_ref().err().map(ToString::to_string);
        let mut metadata = result.ok();
        if metadata
            .as_ref()
            .is_some_and(|value| value.document_uuid != uuid)
        {
            metadata = None;
            metadata_error = Some("recovery metadata belongs to a different document".to_owned());
        }
        Some(inkpod_io::RecoveryCandidate {
            recovery_path: path.clone(),
            metadata_path: inkpod_io::recovery_metadata_path(path)?,
            modified_time_100ns: 0,
            metadata,
            metadata_error,
        })
    } else {
        None
    };
    context.check_cancelled()?;
    Ok((
        Prepared::Open(Box::new(staged), recovery),
        vec![FileIoItem {
            path: path.clone(),
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_owned(),
            format: None,
            identity: stamp.identity,
            identity_physical: true,
            source_generation: 1,
            document_uuid: uuid,
        }],
    ))
}

pub(super) fn images(
    manager: &IoManager,
    request: &FileIoRequest,
    images: Vec<LoadedImage>,
    seed: Option<usize>,
    seed_uuid: Option<u128>,
    reload: Option<LightTableItemInfo>,
    context: &JobContext,
) -> Result<(Prepared, Vec<FileIoItem>), CoreError> {
    context.check_cancelled()?;
    let mut pairs: Vec<_> = images
        .into_iter()
        .enumerate()
        .map(|(index, image)| {
            let uuid = if request.document_uuid != 0 && request.kind == FileIoKind::OpenRaster {
                request.document_uuid
            } else if seed == Some(index) {
                seed_uuid.unwrap_or_else(|| source_uuid(image.identity()))
            } else {
                source_uuid(image.identity())
            };
            let item = FileIoItem {
                path: image.path().to_path_buf(),
                name: image.name().to_owned(),
                format: Some(image.format()),
                identity: image.identity(),
                identity_physical: true,
                source_generation: image.generation(),
                document_uuid: uuid,
            };
            (image, item)
        })
        .collect();
    pairs.sort_by(|left, right| crate::animation::natural_cmp(&left.1.name, &right.1.name));
    let items = pairs.iter().map(|(_, item)| item.clone()).collect();
    let prepared = match request.kind {
        FileIoKind::OpenRaster => {
            let (image, item) = &pairs[0];
            let mut staged = Core::new();
            staged.import_decoded_common_raster(
                image.format(),
                image.raster(),
                item.document_uuid,
            )?;
            Prepared::Open(Box::new(staged), None)
        }
        FileIoKind::SequenceAuto | FileIoKind::SequenceFiles => {
            let total = pairs.len() as u64;
            let mut sources = Vec::with_capacity(pairs.len());
            for (index, (image, item)) in pairs.into_iter().enumerate() {
                context.check_cancelled()?;
                sources.push(SequenceCellSource::from_loaded_image(
                    manager,
                    &image,
                    item.document_uuid,
                )?);
                context.set_work(index as u64 + 1, total);
            }
            Prepared::Sequence(sources)
        }
        FileIoKind::ReferenceFiles | FileIoKind::ReferenceFolder => {
            Prepared::References(pairs.into_iter().map(|(image, _)| image).collect())
        }
        FileIoKind::LightTableAdd | FileIoKind::LightTableReload => {
            let (image, item) = &pairs[0];
            let reference_frame = RectI32 {
                x: 0,
                y: 0,
                width: i32::try_from(image.raster().info.width)
                    .map_err(|_| CoreError::InvalidArgument("reference width overflows"))?,
                height: i32::try_from(image.raster().info.height)
                    .map_err(|_| CoreError::InvalidArgument("reference height overflows"))?,
            };
            let source = LightTableSource::from_common_raster(
                item.document_uuid,
                image.generation(),
                reference_frame,
                image.raster(),
            )?;
            let mut input = LightTableItemInput::new(&item.name, source);
            if let Some(old) = reload {
                input.name = old.name;
                input.visible = old.visible;
                input.opacity_milli = old.opacity_milli;
                input.display_mode = old.display_mode;
                input.display_color = old.display_color;
                input.translate_x_milli = old.translate_x_milli;
                input.translate_y_milli = old.translate_y_milli;
                input.scale_x_milli = old.scale_x_milli;
                input.scale_y_milli = old.scale_y_milli;
                input.rotation_milli_degrees = old.rotation_milli_degrees;
            }
            Prepared::LightTable(input)
        }
        _ => return Err(CoreError::InvalidArgument("invalid image job purpose")),
    };
    context.check_cancelled()?;
    Ok((prepared, items))
}

pub(super) fn save(
    manager: &IoManager,
    request: &FileIoRequest,
    snapshot: DocumentSaveSnapshot,
    expected: Option<SavedPair>,
    context: &JobContext,
) -> Result<(Prepared, Vec<FileIoItem>), CoreError> {
    let path = &request.paths[0];
    let mut items = Vec::new();
    let prepared = match request.kind {
        FileIoKind::SavePair => {
            let (native, format, bytes, token) = snapshot
                .prepare_normal_save(|| context.is_cancelled())?
                .into_parts();
            let companion = path.with_extension(format_extension(format));
            let expected = expected
                .filter(|saved| saved.native_path == *path && !request.overwrite_confirmed)
                .map(|saved| (Some(saved.native), saved.raster));
            let pair = manager.prepare_pair_checked(
                path,
                &companion,
                context,
                |writer| {
                    Ok(
                        inkpod_format::write_procedure_to_writer(writer, &native, || {
                            context.is_cancelled()
                        })
                        .map(|_| ())?,
                    )
                },
                &bytes,
                request.overwrite_confirmed,
                expected,
            )?;
            for (destination, format) in [(path, None), (&companion, Some(format))] {
                let (identity, identity_physical) = manager.resolve_identity(destination)?;
                items.push(FileIoItem {
                    path: destination.clone(),
                    name: destination
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("")
                        .to_owned(),
                    format,
                    identity,
                    identity_physical,
                    source_generation: 1,
                    document_uuid: 0,
                });
            }
            Prepared::Pair(Box::new(pair), token)
        }
        FileIoKind::Autosave => {
            let (native, _) = snapshot.prepare_native_save(true, || context.is_cancelled())?;
            if let Some(metadata) = &request.recovery_metadata {
                manager.write_recovery(path, metadata, context, |writer| {
                    Ok(
                        inkpod_format::write_procedure_to_writer(writer, &native, || {
                            context.is_cancelled()
                        })
                        .map(|_| ())?,
                    )
                })?;
            } else {
                manager.write_atomic(path, context, |writer| {
                    Ok(
                        inkpod_format::write_procedure_to_writer(writer, &native, || {
                            context.is_cancelled()
                        })
                        .map(|_| ())?,
                    )
                })?;
            }
            Prepared::Output
        }
        FileIoKind::ExportRaster => {
            let format = request
                .raster_format
                .ok_or(CoreError::InvalidArgument("missing export format"))?;
            let bytes = snapshot.prepare_raster_export(
                format,
                request.composite_white,
                request.instructions,
                || context.is_cancelled(),
            )?;
            manager.write_bytes_atomic(path, &bytes, context)?;
            Prepared::Output
        }
        _ => return Err(CoreError::InvalidArgument("invalid file write purpose")),
    };
    Ok((prepared, items))
}

pub(super) fn format_extension(format: CommonRasterFormat) -> &'static str {
    match format {
        CommonRasterFormat::Png => "png",
        CommonRasterFormat::Tiff => "tif",
        CommonRasterFormat::Tga => "tga",
        CommonRasterFormat::Bmp => "bmp",
    }
}
