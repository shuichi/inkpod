use super::job::{Discovery, PairRepairTarget, Prepared};
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
        FileIoKind::RecoveryProbe | FileIoKind::ExportSequence | FileIoKind::SavePair
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
    if request.revert_current && (request.kind != FileIoKind::OpenNative || !request.force_reload) {
        return Err(CoreError::InvalidArgument(
            "current-document revert requires a forced native open",
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

fn pair_conflict(error: CoreError) -> CoreError {
    if error == CoreError::Cancelled {
        CoreError::Cancelled
    } else {
        CoreError::FileConflict
    }
}

fn io_pair_conflict(error: inkpod_io::IoError) -> CoreError {
    pair_conflict(error.into())
}

fn validate_canonical_companion(
    staged: &Core,
    format: CommonRasterFormat,
    image: &LoadedImage,
) -> Result<(), CoreError> {
    if staged.document_info()?.dirty {
        return Err(CoreError::FileConflict);
    }
    let encoded = staged.export_native_save_raster(format)?;
    let expected = inkpod_format::decode_common_raster(format, &encoded)?;
    if !crate::raster_pair_validation::canonical_raster_pair_eq(
        format,
        &expected,
        image.format(),
        image.raster(),
    )? {
        return Err(CoreError::FileConflict);
    }
    Ok(())
}

enum RasterPairNativeProof {
    Existing(inkpod_io::FileStamp),
    Missing(FileIdentity),
}

struct NativeCompanionProof {
    format: CommonRasterFormat,
    candidates: Vec<std::path::PathBuf>,
    companion: std::path::PathBuf,
    raster: Option<inkpod_io::FileStamp>,
    identity: FileIdentity,
    identity_physical: bool,
}

/// Resolves one editable raster and its same-stem native candidate as a coherent pair.
///
/// The raster item is always first and the native candidate is always second. A
/// missing native is represented by its normalized-path identity and leaves the
/// staged raster Genesis pathless. An existing native is fully replayed and its
/// normal-save composite must decode exactly to the selected raster before the
/// staged Core adopts committed pair authority.
pub(super) fn raster_pair(
    manager: &IoManager,
    image: &LoadedImage,
    document_uuid: u128,
    context: &JobContext,
    validate_final_image: impl FnOnce(&LoadedImage) -> Result<(), CoreError>,
    managed_final_image: impl FnOnce(
        &LoadedImage,
    ) -> Result<crate::asset::ManagedRasterDecision, CoreError>,
) -> Result<(Prepared, Vec<FileIoItem>), CoreError> {
    let raster_path = image.path().to_path_buf();
    let default_native_path = raster_path.with_extension("inkpod");
    let (native_candidates, raster_candidates) = manager
        .discover_pair_companion_candidates(
            &raster_path,
            &default_native_path,
            image.format(),
            context,
        )
        .map_err(io_pair_conflict)?;
    if native_candidates.len() > 1 {
        return Err(CoreError::FileConflict);
    }
    let native_path = native_candidates
        .first()
        .cloned()
        .unwrap_or(default_native_path);
    if raster_candidates.as_slice() != [raster_path.clone()] {
        return Err(CoreError::FileConflict);
    }
    let recovery = manager
        .recover_pairs(&native_path, context)
        .map_err(io_pair_conflict)?;
    let image = if recovery == inkpod_io::PairRecovery::NotNeeded {
        image.clone()
    } else {
        manager
            .read_image_with_reload(&raster_path, true, context)
            .map_err(io_pair_conflict)?
    };
    let (native_candidates, raster_candidates) = manager
        .discover_pair_companion_candidates(&raster_path, &native_path, image.format(), context)
        .map_err(io_pair_conflict)?;
    if native_candidates.len() > 1 {
        return Err(CoreError::FileConflict);
    }
    let recovered_native_path = native_candidates
        .first()
        .cloned()
        .unwrap_or_else(|| raster_path.with_extension("inkpod"));
    if recovered_native_path != native_path {
        return Err(CoreError::FileConflict);
    }
    if raster_candidates.as_slice() != [raster_path.clone()] {
        return Err(CoreError::FileConflict);
    }
    validate_final_image(&image)?;
    let loaded_native = manager
        .with_file_locks(
            &[raster_path.clone(), native_path.clone()],
            context,
            |files| {
                if files.metadata(&raster_path)? != image.source().stamp() {
                    return Err(inkpod_io::IoError::ChangedDuringRead);
                }
                if !files.exists(&native_path)? {
                    return Ok(None);
                }
                let stamp = files.metadata(&native_path)?;
                let file = files.with_reader(&native_path, MAX_NATIVE_BYTES, |reader| {
                    Ok(inkpod_format::read_procedure_from_reader(reader, || {
                        context.is_cancelled()
                    })?)
                })?;
                if stamp != files.metadata(&native_path)?
                    || files.metadata(&raster_path)? != image.source().stamp()
                {
                    return Err(inkpod_io::IoError::ChangedDuringRead);
                }
                Ok(Some((file, stamp)))
            },
        )
        .map_err(io_pair_conflict)?;
    context.check_cancelled()?;

    let (staged, normal_path, native_identity, native_physical, uuid, native_proof) =
        if let Some((native, native_stamp)) = loaded_native {
            let mut staged = Core::from_native_file(native, false).map_err(pair_conflict)?;
            let uuid = staged.document_info().map_err(pair_conflict)?.document_uuid;
            let format = staged.raster_file_format().map_err(pair_conflict)?;
            validate_canonical_companion(&staged, format, &image).map_err(pair_conflict)?;
            staged.io_pair_authority = Some(SavedPair {
                native_path: native_path.clone(),
                native: native_stamp,
                raster_path: raster_path.clone(),
                raster: Some(image.source().stamp()),
                raster_missing: None,
            });
            (
                staged,
                Some(native_path.clone()),
                native_stamp.identity,
                true,
                uuid,
                RasterPairNativeProof::Existing(native_stamp),
            )
        } else {
            let (identity, physical) = manager
                .resolve_identity(&native_path)
                .map_err(io_pair_conflict)?;
            if physical {
                return Err(CoreError::FileConflict);
            }
            // Provenance-based reuse is deliberately classified only after the
            // resolver has established that there is no sidecar to replay. The
            // result selects an equivalent construction strategy; it never
            // changes pair authority or pristine-source registration.
            let managed_raster = managed_final_image(&image)?;
            let mut staged = Core::new();
            match managed_raster {
                crate::asset::ManagedRasterDecision::Reuse(raster) => {
                    staged.import_managed_sequence_raster(image.format(), raster, document_uuid)?;
                }
                crate::asset::ManagedRasterDecision::Ineligible => {
                    staged.import_decoded_common_raster(
                        image.format(),
                        image.raster(),
                        document_uuid,
                    )?;
                    staged.record_cow_fallback(
                        image.raster().pixels.len() as u64,
                        u64::from(image.raster().info.width)
                            .saturating_mul(u64::from(image.raster().info.height)),
                    );
                }
                crate::asset::ManagedRasterDecision::NotRequested => {
                    staged.import_decoded_common_raster(
                        image.format(),
                        image.raster(),
                        document_uuid,
                    )?;
                }
            }
            staged.io_pair_plan = Some(PlannedPair {
                native_path: native_path.clone(),
                native_missing: identity,
                raster_path: raster_path.clone(),
                raster: image.source().stamp(),
            });
            (
                staged,
                None,
                identity,
                false,
                document_uuid,
                RasterPairNativeProof::Missing(identity),
            )
        };
    context.check_cancelled()?;

    let raster_item = FileIoItem {
        path: raster_path.clone(),
        name: image.name().to_owned(),
        format: Some(image.format()),
        identity: image.identity(),
        identity_physical: true,
        source_generation: image.generation(),
        document_uuid: uuid,
    };
    let native_item = FileIoItem {
        name: native_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_owned(),
        path: native_path.clone(),
        format: None,
        identity: native_identity,
        identity_physical: native_physical,
        source_generation: 1,
        document_uuid: uuid,
    };
    match native_proof {
        RasterPairNativeProof::Existing(expected_native) => manager
            .with_file_locks(
                &[raster_path.clone(), native_path.clone()],
                context,
                |files| {
                    if files.metadata(&raster_path)? != image.source().stamp()
                        || files.metadata(&native_path)? != expected_native
                    {
                        return Err(inkpod_io::IoError::ChangedDuringRead);
                    }
                    Ok(())
                },
            )
            .map_err(io_pair_conflict)?,
        RasterPairNativeProof::Missing(expected_identity) => {
            manager
                .with_file_locks(
                    &[raster_path.clone(), native_path.clone()],
                    context,
                    |files| {
                        if files.metadata(&raster_path)? != image.source().stamp()
                            || files.exists(&native_path)?
                        {
                            return Err(inkpod_io::IoError::ChangedDuringRead);
                        }
                        Ok(())
                    },
                )
                .map_err(io_pair_conflict)?;
            let (identity, physical) = manager
                .resolve_identity(&native_path)
                .map_err(io_pair_conflict)?;
            if physical || identity != expected_identity {
                return Err(CoreError::FileConflict);
            }
        }
    }
    let final_candidates = manager
        .discover_pair_companion_candidates(&raster_path, &native_path, image.format(), context)
        .map_err(io_pair_conflict)?;
    if final_candidates != (native_candidates, raster_candidates) {
        return Err(CoreError::FileConflict);
    }
    context.check_cancelled()?;
    Ok((
        Prepared::Open(Box::new(staged), None, normal_path),
        vec![raster_item, native_item],
    ))
}

pub(super) fn native(
    manager: &IoManager,
    request: &FileIoRequest,
    context: &JobContext,
) -> Result<(Prepared, Vec<FileIoItem>), CoreError> {
    let path = &request.paths[0];
    let direct_native_candidates = if request.kind == FileIoKind::OpenNative {
        // Preserve the ordinary I/O contract for a path that was already
        // absent when the request began. Once this proof succeeds, any later
        // disappearance or same-stem drift is a pair-authority conflict.
        let _selected_stamp = manager.metadata(path, context)?;
        let selected = manager.normalize_path(path).map_err(io_pair_conflict)?;
        let candidates = manager
            .discover_native_companion_candidates(path, context)
            .map_err(io_pair_conflict)?;
        if candidates.as_slice() != [selected] {
            return Err(CoreError::FileConflict);
        }
        Some(candidates)
    } else {
        None
    };
    let recovered = manager.recover_pairs(path, context);
    if request.kind == FileIoKind::OpenNative {
        recovered.map_err(io_pair_conflict)?;
    } else {
        recovered?;
    }
    if let Some(expected) = &direct_native_candidates
        && manager
            .discover_native_companion_candidates(path, context)
            .map_err(io_pair_conflict)?
            != *expected
    {
        return Err(CoreError::FileConflict);
    }
    let loaded = manager.with_file_locks(std::slice::from_ref(path), context, |files| {
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
    });
    let (native, stamp) = if request.kind == FileIoKind::OpenNative {
        loaded.map_err(io_pair_conflict)?
    } else {
        loaded?
    };
    context.check_cancelled()?;
    let Some(native) = native else {
        if let Some(expected) = &direct_native_candidates
            && manager
                .discover_native_companion_candidates(path, context)
                .map_err(io_pair_conflict)?
                != *expected
        {
            return Err(CoreError::FileConflict);
        }
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
    let companion_proof = if request.kind == FileIoKind::OpenNative {
        let format = Core::procedure_file_raster_format(&native).map_err(pair_conflict)?;
        let candidates = manager
            .discover_raster_companion_candidates(path, format, context)
            .map_err(io_pair_conflict)?;
        if candidates.len() > 1 {
            return Err(CoreError::FileConflict);
        }
        let companion = candidates
            .first()
            .cloned()
            .unwrap_or_else(|| path.with_extension(format_extension(format)));
        let raster = manager
            .with_file_locks(&[path.clone(), companion.clone()], context, |files| {
                if files.metadata(path)? != stamp {
                    return Err(inkpod_io::IoError::ChangedDuringRead);
                }
                if files.exists(&companion)? {
                    Ok(Some(files.metadata(&companion)?))
                } else {
                    Ok(None)
                }
            })
            .map_err(io_pair_conflict)?;
        let (identity, identity_physical) = if let Some(raster) = raster {
            (raster.identity, true)
        } else {
            let (identity, physical) = manager
                .resolve_identity(&companion)
                .map_err(io_pair_conflict)?;
            if physical {
                return Err(CoreError::FileConflict);
            }
            (identity, false)
        };
        Some(NativeCompanionProof {
            format,
            candidates,
            companion,
            raster,
            identity,
            identity_physical,
        })
    } else {
        None
    };
    let staged = Core::from_native_file(native, request.kind == FileIoKind::OpenRecovery);
    let mut staged = if request.kind == FileIoKind::OpenNative {
        staged.map_err(pair_conflict)?
    } else {
        staged?
    };
    let uuid = staged.document_info()?.document_uuid;
    let mut companion_recorded_loaded = false;
    let mut companion_item = None;
    if let Some(proof) = companion_proof {
        if staged.raster_file_format().map_err(pair_conflict)? != proof.format {
            return Err(CoreError::FileConflict);
        }
        let source_generation = if let Some(raster_stamp) = proof.raster {
            let image = manager
                .read_image_with_reload(&proof.companion, request.force_reload, context)
                .map_err(io_pair_conflict)?;
            companion_recorded_loaded = true;
            if image.source().stamp() != raster_stamp {
                return Err(CoreError::FileConflict);
            }
            validate_canonical_companion(&staged, proof.format, &image).map_err(pair_conflict)?;
            image.generation()
        } else {
            1
        };
        manager
            .with_file_locks(&[path.clone(), proof.companion.clone()], context, |files| {
                if files.metadata(path)? != stamp {
                    return Err(inkpod_io::IoError::ChangedDuringRead);
                }
                let actual = if files.exists(&proof.companion)? {
                    Some(files.metadata(&proof.companion)?)
                } else {
                    None
                };
                if actual != proof.raster {
                    return Err(inkpod_io::IoError::ChangedDuringRead);
                }
                Ok(())
            })
            .map_err(io_pair_conflict)?;
        if manager
            .discover_raster_companion_candidates(path, proof.format, context)
            .map_err(io_pair_conflict)?
            != proof.candidates
        {
            return Err(CoreError::FileConflict);
        }
        staged.io_pair_authority = Some(SavedPair {
            native_path: path.clone(),
            native: stamp,
            raster_path: proof.companion.clone(),
            raster: proof.raster,
            raster_missing: (!proof.identity_physical).then_some(proof.identity),
        });
        companion_item = Some(FileIoItem {
            path: proof.companion.clone(),
            name: proof
                .companion
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_owned(),
            format: Some(proof.format),
            identity: proof.identity,
            identity_physical: proof.identity_physical,
            source_generation,
            document_uuid: uuid,
        });
    }
    context.check_cancelled()?;
    if !companion_recorded_loaded {
        context.record_loaded();
    }
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
    let mut items = vec![FileIoItem {
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
    }];
    if let Some(companion) = companion_item {
        items.push(companion);
    }
    if let Some(expected) = &direct_native_candidates
        && manager
            .discover_native_companion_candidates(path, context)
            .map_err(io_pair_conflict)?
            != *expected
    {
        return Err(CoreError::FileConflict);
    }
    Ok((
        Prepared::Open(
            Box::new(staged),
            recovery,
            (request.kind == FileIoKind::OpenNative).then(|| path.clone()),
        ),
        items,
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
            let uuid = if request.document_uuid != 0
                && matches!(
                    request.kind,
                    FileIoKind::OpenRaster | FileIoKind::OpenRasterPair
                ) {
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
    if request.kind == FileIoKind::OpenRasterPair {
        let (image, item) = &pairs[0];
        return raster_pair(
            manager,
            image,
            item.document_uuid,
            context,
            |_| Ok(()),
            |_| Ok(crate::asset::ManagedRasterDecision::NotRequested),
        );
    }
    let prepared = match request.kind {
        FileIoKind::OpenRaster => {
            let (image, item) = &pairs[0];
            let mut staged = Core::new();
            staged.import_decoded_common_raster(
                image.format(),
                image.raster(),
                item.document_uuid,
            )?;
            Prepared::Open(Box::new(staged), None, None)
        }
        FileIoKind::OpenRasterPair => {
            unreachable!("raster-pair open returns before the image-purpose match")
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
    planned: Option<PlannedPair>,
    context: &JobContext,
) -> Result<(Prepared, Vec<FileIoItem>), CoreError> {
    let path = &request.paths[0];
    let mut items = Vec::new();
    let prepared = match request.kind {
        FileIoKind::SavePair => {
            let (native, format, bytes, token) = snapshot
                .prepare_normal_save(|| context.is_cancelled())?
                .into_parts();
            let document_uuid = token.document_uuid()?;
            let normalized_native = manager.normalize_path(path)?;
            let companion = request
                .paths
                .get(1)
                .cloned()
                .or_else(|| {
                    expected.as_ref().and_then(|saved| {
                        (manager.normalize_path(&saved.native_path).ok().as_ref()
                            == Some(&normalized_native))
                        .then(|| saved.raster_path.clone())
                    })
                })
                .or_else(|| {
                    planned.as_ref().and_then(|planned| {
                        (planned.native_path == normalized_native)
                            .then(|| planned.raster_path.clone())
                    })
                })
                .unwrap_or_else(|| path.with_extension(format_extension(format)));
            if raster_format(&companion) != Some(format) {
                return Err(CoreError::InvalidArgument(
                    "normal save raster destination format does not match the document",
                ));
            }
            let normalized_companion = manager.normalize_path(&companion)?;
            let same_committed = expected.as_ref().filter(|saved| {
                manager.normalize_path(&saved.native_path).ok().as_ref() == Some(&normalized_native)
                    && manager.normalize_path(&saved.raster_path).ok().as_ref()
                        == Some(&normalized_companion)
            });
            let same_planned = planned.as_ref().filter(|planned| {
                planned.native_path == normalized_native
                    && planned.raster_path == normalized_companion
            });
            let repair_target = if request.overwrite_confirmed {
                if same_committed.is_some() || same_planned.is_some() {
                    PairRepairTarget::Revoke
                } else {
                    PairRepairTarget::Unrelated
                }
            } else if let Some(saved) = same_committed {
                PairRepairTarget::Committed(saved.clone())
            } else if let Some(planned) = same_planned {
                PairRepairTarget::Planned(planned.clone())
            } else {
                PairRepairTarget::Unrelated
            };
            let expected = expected
                .filter(|saved| {
                    manager.normalize_path(&saved.native_path).ok().as_ref()
                        == Some(&normalized_native)
                        && manager.normalize_path(&saved.raster_path).ok().as_ref()
                            == Some(&normalized_companion)
                        && !request.overwrite_confirmed
                })
                .map(|saved| (Some(saved.native), saved.raster));
            let planned = if expected.is_none() && !request.overwrite_confirmed {
                planned.filter(|planned| {
                    normalized_native == planned.native_path
                        && normalized_companion == planned.raster_path
                })
            } else {
                None
            };
            let write_native = |writer: &mut std::fs::File| {
                Ok(
                    inkpod_format::write_procedure_to_writer(writer, &native, || {
                        context.is_cancelled()
                    })
                    .map(|_| ())?,
                )
            };
            let pair = if let Some(planned) = planned {
                manager.prepare_planned_pair_checked(
                    path,
                    &companion,
                    context,
                    write_native,
                    &bytes,
                    planned.native_missing,
                    planned.raster,
                )?
            } else {
                manager.prepare_pair_checked(
                    path,
                    &companion,
                    context,
                    write_native,
                    &bytes,
                    request.overwrite_confirmed,
                    expected,
                )?
            };
            let (native_replacement, raster_replacement) = pair.replacement_stamps();
            for ((destination, format), replacement) in [
                ((path, None), native_replacement),
                ((&companion, Some(format)), raster_replacement),
            ] {
                items.push(FileIoItem {
                    path: destination.clone(),
                    name: destination
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("")
                        .to_owned(),
                    format,
                    identity: replacement.identity,
                    identity_physical: true,
                    source_generation: 1,
                    document_uuid,
                });
            }
            Prepared::Pair(Box::new(pair), token, repair_target)
        }
        FileIoKind::Autosave => {
            let (native, _) = snapshot.prepare_native_save(true, || context.is_cancelled())?;
            let proof = if let Some(metadata) = &request.recovery_metadata {
                Some(manager.write_recovery(path, metadata, context, |writer| {
                    Ok(
                        inkpod_format::write_procedure_to_writer(writer, &native, || {
                            context.is_cancelled()
                        })
                        .map(|_| ())?,
                    )
                })?)
            } else {
                manager.write_atomic(path, context, |writer| {
                    Ok(
                        inkpod_format::write_procedure_to_writer(writer, &native, || {
                            context.is_cancelled()
                        })
                        .map(|_| ())?,
                    )
                })?;
                None
            };
            Prepared::Output(proof)
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
            Prepared::Output(None)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_native_pair_errors_preserve_cancel_and_normalize_races() {
        assert_eq!(
            io_pair_conflict(inkpod_io::IoError::ChangedDuringRead),
            CoreError::FileConflict
        );
        assert_eq!(
            io_pair_conflict(inkpod_io::IoError::InvalidInput("bad journal")),
            CoreError::FileConflict
        );
        assert_eq!(
            io_pair_conflict(inkpod_io::IoError::Cancelled),
            CoreError::Cancelled
        );
    }
}
