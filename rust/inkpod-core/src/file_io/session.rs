use super::job::{FileIoJob, InstallCompletion, Pending, Prepared};
use super::model::{FileIoItem, FileIoKind, FileIoRequest, FileIoState};
use crate::sequence_io::ResolvedRecoveryPairTarget;
use crate::{
    CompactionPlan, Core, CoreError, DocumentSaveToken, PreparedSequenceSwitch,
    SequenceSwitchRequest,
};
use inkpod_format::NativeFile;
use inkpod_io::{
    IoManager, JobContext, RecoveryArtifactProof, RecoveryIdentity, RecoveryIdentityKind,
    RecoveryMetadata, RecoveryPairProof,
};
use std::path::PathBuf;

fn write_native(
    manager: &IoManager,
    path: &std::path::Path,
    file: &NativeFile,
    metadata: Option<&RecoveryMetadata>,
    new_file: bool,
    context: &JobContext,
) -> Result<Option<RecoveryArtifactProof>, CoreError> {
    let writer = |writer: &mut std::fs::File| {
        inkpod_format::write_procedure_to_writer(writer, file, || context.is_cancelled())?;
        Ok(())
    };
    if let Some(metadata) = metadata {
        return Ok(Some(
            manager.write_recovery(path, metadata, context, writer)?,
        ));
    } else if new_file {
        manager.write_new_atomic(path, context, writer)?;
    } else {
        manager.write_atomic(path, context, writer)?;
    }
    Ok(None)
}

fn bind_sequence_recovery_pair(
    core: &Core,
    metadata: &mut RecoveryMetadata,
) -> Result<(), CoreError> {
    match (
        core.current_path.as_ref(),
        core.io_pair_authority.as_ref(),
        core.io_pair_plan.as_ref(),
    ) {
        (Some(current), Some(authority), None) if *current == authority.native_path => {
            metadata.original_path = authority
                .native_path
                .to_str()
                .ok_or(CoreError::InvalidArgument(
                    "sequence source native path is not UTF-8",
                ))?
                .to_owned();
            metadata.source_path = authority
                .raster_path
                .to_str()
                .ok_or(CoreError::InvalidArgument(
                    "sequence source raster path is not UTF-8",
                ))?
                .to_owned();
            metadata.original_identity = RecoveryIdentity {
                kind: RecoveryIdentityKind::PhysicalFile,
                volume_serial: authority.native.identity.volume,
                file_id: authority.native.identity.file.to_le_bytes(),
                ..RecoveryIdentity::default()
            };
            metadata.pair_proof = Some(match (authority.raster, authority.raster_missing) {
                (Some(raster), None) => RecoveryPairProof::Committed {
                    native: authority.native,
                    raster,
                },
                (None, Some(raster_missing)) => RecoveryPairProof::RepairNeeded {
                    native: authority.native,
                    raster_missing,
                },
                _ => {
                    return Err(CoreError::InvalidState(
                        "sequence source raster authority is inconsistent",
                    ));
                }
            });
        }
        (None, None, Some(plan)) => {
            let native = plan.native_path.to_str().ok_or(CoreError::InvalidArgument(
                "sequence planned native path is not UTF-8",
            ))?;
            metadata.original_path.clear();
            metadata.source_path = plan
                .raster_path
                .to_str()
                .ok_or(CoreError::InvalidArgument(
                    "sequence source raster path is not UTF-8",
                ))?
                .to_owned();
            metadata.original_identity = RecoveryIdentity {
                kind: RecoveryIdentityKind::NormalizedPath,
                normalized_path: native.to_owned(),
                ..RecoveryIdentity::default()
            };
            metadata.pair_proof = Some(RecoveryPairProof::Planned {
                native_missing: plan.native_missing,
                raster: plan.raster,
            });
        }
        (None, None, None) => {
            // Untitled, explicit-import, and standalone recovery documents
            // deliberately have no normal-save authority. Sequence navigation
            // remains available, but the resulting generation restores
            // pathless and requires Save As rather than synthesizing authority.
            metadata.pair_proof = None;
        }
        _ => {
            return Err(CoreError::InvalidState(
                "sequence source has no coherent raster-pair authority",
            ));
        }
    }
    Ok(())
}

impl FileIoJob {
    /// Resolves the selected sequence raster through the ordinary same-stem
    /// pair resolver, then prepares one fenced switch. Existing native history
    /// is replayed; a missing sidecar retains the resolver's planned-pair proof.
    /// The optional source recovery has the same durability contract as
    /// [`Self::start_sequence_switch`].
    pub fn start_sequence_raster_pair_switch(
        core: &Core,
        manager: IoManager,
        request: SequenceSwitchRequest,
        source_recovery: Option<PathBuf>,
        target_raster: PathBuf,
        metadata: Option<RecoveryMetadata>,
    ) -> Result<Self, CoreError> {
        Self::start_sequence_raster_pair_switch_with_cache(
            core,
            manager,
            super::ValidatedTargetCache::default(),
            request,
            source_recovery,
            target_raster,
            metadata,
        )
    }

    /// Uses an application-owned validated-target cache for a raster-pair switch.
    pub fn start_sequence_raster_pair_switch_with_cache(
        core: &Core,
        manager: IoManager,
        target_cache: super::ValidatedTargetCache,
        request: SequenceSwitchRequest,
        source_recovery: Option<PathBuf>,
        target_raster: PathBuf,
        mut metadata: Option<RecoveryMetadata>,
    ) -> Result<Self, CoreError> {
        if target_raster.as_os_str().is_empty() {
            return Err(CoreError::InvalidArgument(
                "sequence raster-pair target path is empty",
            ));
        }
        if request.requires_source_recovery() && source_recovery.is_none() {
            return Err(CoreError::InvalidArgument(
                "sequence source requires a recovery destination",
            ));
        }
        if request.requires_switch() && source_recovery.is_some() != metadata.is_some() {
            return Err(CoreError::InvalidArgument(
                "sequence source recovery requires typed metadata",
            ));
        }
        let include_source_recovery = source_recovery.is_some();
        if let Some(metadata) = &metadata {
            let document = core.document.as_ref().ok_or(CoreError::NoDocument)?;
            if metadata.document_uuid != document.uuid {
                return Err(CoreError::InvalidArgument(
                    "sequence recovery metadata belongs to a different document",
                ));
            }
        }
        if request.requires_switch()
            && let Some(metadata) = metadata.as_mut()
        {
            bind_sequence_recovery_pair(core, metadata)?;
        }
        let snapshot = core.capture_sequence_switch(request)?;
        let mut io_request = FileIoRequest::new(
            FileIoKind::SequenceSwitch,
            source_recovery.into_iter().collect(),
        );
        io_request.recovery_metadata = metadata;
        let mut job = Self::allocate(Some(core), manager.clone(), io_request)?;
        job.pending = Some(Pending::Prepare(manager.clone().submit(
            move |context| {
                let result = (|| {
                    let normalized = manager.normalize_path(&target_raster)?;
                    let stamp = manager.metadata(&normalized, &context)?;
                    let managed =
                        snapshot.managed_target_raster_from_stamp(&manager, &normalized, stamp)?;
                    let (prepared, items) = if let Some((format, generation, input)) = managed {
                        super::prepare::raster_pair_managed(
                            &manager,
                            super::prepare::ManagedPairRasterSource::new(
                                normalized, format, stamp, generation, input,
                            ),
                            request.target_document_uuid,
                            &context,
                            Some(&target_cache),
                            |_| snapshot.validate_target_source(),
                            |image| snapshot.managed_target_raster(&manager, image),
                        )?
                    } else {
                        let image = manager.read_image(&target_raster, &context)?;
                        super::prepare::raster_pair(
                            &manager,
                            &image,
                            request.target_document_uuid,
                            &context,
                            Some(&target_cache),
                            |_| snapshot.validate_target_source(),
                            |image| snapshot.managed_target_raster(&manager, image),
                        )?
                    };
                    let (staged, normal_path) = match prepared {
                        Prepared::Open(staged, None, normal_path) => (staged, normal_path),
                        _ => {
                            return Err(CoreError::InvalidState(
                                "raster-pair resolver returned an invalid sequence target",
                            ));
                        }
                    };
                    let target = snapshot.prepare_pair_target(
                        *staged,
                        normal_path,
                        include_source_recovery,
                        || context.is_cancelled(),
                    )?;
                    Ok((Prepared::SequenceSwitch(Box::new(target)), items))
                })();
                Ok(result)
            },
        )?));
        Ok(job)
    }

    /// Prepares a sequence switch without blocking the owner. The source recovery
    /// is installed only after owner validation/fencing; then final apply switches
    /// once. A target recovery resolves its metadata source through the ordinary
    /// raster-pair resolver and adopts that pair's runtime save authority only
    /// after UUID, Genesis, and original raster identity validation.
    /// An omitted source path skips source encoding and installation only when
    /// both document and EditorState are clean, the source is not recovered, and
    /// no repair-needed pair authority must be retained, or for a same-cell no-op.
    /// Supplying a source path retains recovery installation even when the source
    /// is clean so its undo/redo tail can move to a new recovery generation.
    pub fn start_sequence_switch(
        core: &Core,
        manager: IoManager,
        request: SequenceSwitchRequest,
        source_recovery: Option<PathBuf>,
        target_recovery: Option<(PathBuf, RecoveryArtifactProof)>,
        metadata: Option<RecoveryMetadata>,
    ) -> Result<Self, CoreError> {
        Self::start_sequence_switch_with_cache(
            core,
            manager,
            super::ValidatedTargetCache::default(),
            request,
            source_recovery,
            target_recovery,
            metadata,
        )
    }

    /// Uses an application-owned validated-target cache for recovery-aware switching.
    pub fn start_sequence_switch_with_cache(
        core: &Core,
        manager: IoManager,
        target_cache: super::ValidatedTargetCache,
        request: SequenceSwitchRequest,
        source_recovery: Option<PathBuf>,
        target_recovery: Option<(PathBuf, RecoveryArtifactProof)>,
        mut metadata: Option<RecoveryMetadata>,
    ) -> Result<Self, CoreError> {
        if request.requires_source_recovery() && source_recovery.is_none() {
            return Err(CoreError::InvalidArgument(
                "sequence source requires a recovery destination",
            ));
        }
        if request.requires_switch() && source_recovery.is_some() != metadata.is_some() {
            return Err(CoreError::InvalidArgument(
                "sequence source recovery requires typed metadata",
            ));
        }
        let requires_switch = request.requires_switch();
        let include_source_recovery = source_recovery.is_some();
        if let Some(metadata) = &metadata {
            let document = core.document.as_ref().ok_or(CoreError::NoDocument)?;
            if metadata.document_uuid != document.uuid {
                return Err(CoreError::InvalidArgument(
                    "sequence recovery metadata belongs to a different document",
                ));
            }
        }
        if request.requires_switch()
            && let Some(metadata) = metadata.as_mut()
        {
            bind_sequence_recovery_pair(core, metadata)?;
        }
        let snapshot = core.capture_sequence_switch(request)?;
        let mut io_request = FileIoRequest::new(
            FileIoKind::SequenceSwitch,
            source_recovery.into_iter().collect(),
        );
        io_request.recovery_metadata = metadata;
        let mut job = Self::allocate(Some(core), manager.clone(), io_request)?;
        job.pending = Some(Pending::Prepare(manager.clone().submit(
            move |context| {
                let result = (|| {
                    if let Some((path, proof)) = target_recovery.filter(|_| requires_switch) {
                        let (native, recovery_metadata) = manager
                            .read_recovery_with_proof(&path, proof, &context, |reader| {
                                Ok(inkpod_format::read_procedure_from_reader(reader, || {
                                    context.is_cancelled()
                                })?)
                            })
                            .map_err(|error| {
                                if matches!(error, inkpod_io::IoError::Cancelled) {
                                    CoreError::Cancelled
                                } else {
                                    CoreError::FileConflict
                                }
                            })?;
                        if recovery_metadata.document_uuid != request.target_document_uuid {
                            return Err(CoreError::FileConflict);
                        }
                        let Some(pair_proof) = recovery_metadata.pair_proof else {
                            let target = if include_source_recovery {
                                snapshot.prepare(Some(native), || context.is_cancelled())?
                            } else {
                                snapshot.prepare_without_source_recovery(Some(native), || {
                                    context.is_cancelled()
                                })?
                            };
                            return Ok((Prepared::SequenceSwitch(Box::new(target)), Vec::new()));
                        };
                        if let RecoveryPairProof::RepairNeeded {
                            native: expected_native,
                            raster_missing,
                        } = pair_proof
                        {
                            if recovery_metadata.original_path.is_empty()
                                || recovery_metadata.source_path.is_empty()
                            {
                                return Err(CoreError::FileConflict);
                            }
                            let native_path = PathBuf::from(&recovery_metadata.original_path);
                            let raster_path = PathBuf::from(&recovery_metadata.source_path);
                            let mut native_request = FileIoRequest::new(
                                FileIoKind::OpenNative,
                                vec![native_path.clone()],
                            );
                            native_request.force_reload = true;
                            let (prepared, mut items) =
                                super::prepare::native(&manager, &native_request, &context)
                                    .map_err(|error| {
                                        if error == CoreError::Cancelled {
                                            CoreError::Cancelled
                                        } else {
                                            CoreError::FileConflict
                                        }
                                    })?;
                            let (resolved, normal_path) = match prepared {
                                Prepared::Open(staged, None, normal_path) => (staged, normal_path),
                                _ => {
                                    return Err(CoreError::InvalidState(
                                        "native-pair resolver returned an invalid recovery target",
                                    ));
                                }
                            };
                            let [native_item, raster_item] = items.as_slice() else {
                                return Err(CoreError::FileConflict);
                            };
                            if normal_path.as_ref() != Some(&native_path)
                                || native_item.path != native_path
                                || !native_item.identity_physical
                                || native_item.identity != expected_native.identity
                                || raster_item.path != raster_path
                                || raster_item.identity_physical
                                || raster_item.identity != raster_missing
                            {
                                return Err(CoreError::FileConflict);
                            }
                            let target = snapshot.prepare_recovery_pair_target(
                                native,
                                ResolvedRecoveryPairTarget {
                                    core: *resolved,
                                    normal_path,
                                    proof: pair_proof,
                                    raster_missing: Some((raster_path, raster_missing)),
                                },
                                include_source_recovery,
                                || context.is_cancelled(),
                            )?;
                            items.swap(0, 1);
                            return Ok((Prepared::SequenceSwitch(Box::new(target)), items));
                        }
                        if recovery_metadata.source_path.is_empty() {
                            return Err(CoreError::FileConflict);
                        }
                        let source_path = PathBuf::from(recovery_metadata.source_path);
                        let normalized = manager
                            .normalize_path(&source_path)
                            .map_err(CoreError::from)?;
                        let stamp = manager
                            .metadata(&normalized, &context)
                            .map_err(CoreError::from)?;
                        let managed = snapshot.managed_target_raster_from_stamp(
                            &manager,
                            &normalized,
                            stamp,
                        )?;
                        let resolution = if let Some((format, generation, input)) = managed {
                            super::prepare::raster_pair_managed(
                                &manager,
                                super::prepare::ManagedPairRasterSource::new(
                                    normalized, format, stamp, generation, input,
                                ),
                                request.target_document_uuid,
                                &context,
                                Some(&target_cache),
                                |_| snapshot.validate_target_source(),
                                |image| snapshot.managed_target_raster(&manager, image),
                            )
                        } else {
                            let image = manager
                                .read_image(&source_path, &context)
                                .map_err(CoreError::from)?;
                            super::prepare::raster_pair(
                                &manager,
                                &image,
                                request.target_document_uuid,
                                &context,
                                Some(&target_cache),
                                |_| snapshot.validate_target_source(),
                                |_| Ok(crate::asset::ManagedRasterDecision::NotRequested),
                            )
                        };
                        let (prepared, items) = resolution.map_err(|error| {
                            if error == CoreError::Cancelled {
                                CoreError::Cancelled
                            } else {
                                CoreError::FileConflict
                            }
                        })?;
                        let (resolved, normal_path) = match prepared {
                            Prepared::Open(staged, None, normal_path) => (staged, normal_path),
                            _ => {
                                return Err(CoreError::InvalidState(
                                    "raster-pair resolver returned an invalid recovery target",
                                ));
                            }
                        };
                        let target = snapshot.prepare_recovery_pair_target(
                            native,
                            ResolvedRecoveryPairTarget {
                                core: *resolved,
                                normal_path,
                                proof: pair_proof,
                                raster_missing: None,
                            },
                            include_source_recovery,
                            || context.is_cancelled(),
                        )?;
                        return Ok((Prepared::SequenceSwitch(Box::new(target)), items));
                    }
                    let target = if include_source_recovery {
                        snapshot.prepare(None, || context.is_cancelled())?
                    } else {
                        snapshot.prepare_without_source_recovery(None, || context.is_cancelled())?
                    };
                    Ok((Prepared::SequenceSwitch(Box::new(target)), Vec::new()))
                })();
                Ok(result)
            },
        )?));
        Ok(job)
    }

    /// Captures explicit compaction confirmation and prepares its new native DTO
    /// off-thread. No output, normal savepoint or path is adopted by preparation.
    pub fn start_compacted_copy(
        core: &Core,
        manager: IoManager,
        path: PathBuf,
        plan: CompactionPlan,
    ) -> Result<Self, CoreError> {
        if path.as_os_str().is_empty() {
            return Err(CoreError::InvalidArgument("compacted copy path is empty"));
        }
        if core.current_path.as_ref() == Some(&path) {
            return Err(CoreError::InvalidArgument(
                "compacted copy requires a separate path",
            ));
        }
        let snapshot = core.capture_compacted_copy(plan)?;
        let destination = path.clone();
        let worker_manager = manager.clone();
        let mut job = Self::allocate(
            Some(core),
            manager.clone(),
            FileIoRequest::new(FileIoKind::CompactedCopy, vec![path]),
        )?;
        job.pending = Some(Pending::Prepare(manager.submit(move |context| {
            let result = (|| {
                let (file, token) =
                    snapshot.prepare_compacted_copy(plan, || context.is_cancelled())?;
                let (identity, identity_physical) =
                    worker_manager.resolve_identity(&destination)?;
                let item = FileIoItem {
                    name: destination
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("")
                        .to_owned(),
                    path: destination,
                    format: None,
                    identity,
                    identity_physical,
                    source_generation: 1,
                    document_uuid: 0,
                };
                Ok((Prepared::NativeOutput(file, token), vec![item]))
            })();
            Ok(result)
        })?));
        Ok(job)
    }

    pub(super) fn install_sequence(
        &mut self,
        mut prepared: Box<PreparedSequenceSwitch>,
    ) -> Result<bool, CoreError> {
        let Some(file) = prepared.take_source_recovery() else {
            self.sequence_install = Some(prepared);
            return Ok(false);
        };
        let path = self
            .request
            .paths
            .first()
            .cloned()
            .ok_or(CoreError::InvalidArgument("missing source recovery path"))?;
        let manager = self.manager.clone();
        let metadata = self.request.recovery_metadata.clone();
        let pending = self.manager.submit(move |context| {
            Ok(
                write_native(&manager, &path, &file, metadata.as_ref(), false, &context)
                    .map(|proof| InstallCompletion::Standard(None, proof)),
            )
        })?;
        self.sequence_install = Some(prepared);
        self.pending = Some(Pending::Install(pending));
        self.progress.installing = true;
        self.progress.state = FileIoState::Running;
        Ok(true)
    }

    pub(super) fn install_native(
        &mut self,
        file: NativeFile,
        token: DocumentSaveToken,
    ) -> Result<(), CoreError> {
        let path = self.request.paths[0].clone();
        let manager = self.manager.clone();
        let pending = self.manager.submit(move |context| {
            Ok(write_native(&manager, &path, &file, None, true, &context)
                .map(|proof| InstallCompletion::Standard(None, proof)))
        })?;
        self.save_token = Some(token);
        self.pending = Some(Pending::Install(pending));
        self.progress.installing = true;
        self.progress.state = FileIoState::Running;
        Ok(())
    }
}
