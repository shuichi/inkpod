use super::job::{FileIoJob, Prepared};
use super::model::{FileIoItem, FileIoKind, FileIoRequest};
use super::prepare::format_extension;
use crate::{Core, CoreError};
use inkpod_io::{IoManager, JobContext, RecoveryArtifactProof, RecoveryCandidate};
use std::io::ErrorKind;
use std::path::PathBuf;

fn exact_discard_error(error: inkpod_io::IoError) -> CoreError {
    match error {
        inkpod_io::IoError::ChangedDuringRead | inkpod_io::IoError::ConfirmationRequired => {
            CoreError::FileConflict
        }
        inkpod_io::IoError::Io(error) if error.kind() == ErrorKind::NotFound => {
            CoreError::FileConflict
        }
        other => other.into(),
    }
}

pub(super) fn prepare(
    manager: &IoManager,
    request: &FileIoRequest,
    context: &JobContext,
) -> Result<(Prepared, Vec<FileIoItem>), CoreError> {
    let candidates = match request.kind {
        FileIoKind::RecoveryList => manager.list_recovery_candidates(&request.paths[0], context)?,
        FileIoKind::RecoveryDiscard => {
            manager.discard_recovery(&request.paths[0], context)?;
            Vec::new()
        }
        FileIoKind::RecoveryProbe => {
            if request.paths.len() != 2 {
                return Err(CoreError::InvalidArgument(
                    "recovery probe requires normal and recovery paths",
                ));
            }
            if manager.recovery_is_newer(&request.paths[0], &request.paths[1], context)? {
                let metadata = manager
                    .read_recovery_metadata(&request.paths[1], context)
                    .ok();
                vec![RecoveryCandidate {
                    recovery_path: request.paths[1].clone(),
                    metadata_path: inkpod_io::recovery_metadata_path(&request.paths[1])?,
                    modified_time_100ns: 0,
                    metadata,
                    metadata_error: None,
                }]
            } else {
                Vec::new()
            }
        }
        _ => return Err(CoreError::InvalidArgument("invalid recovery request")),
    };
    let items = candidates
        .iter()
        .map(|candidate| {
            let (identity, identity_physical) =
                manager.resolve_identity(&candidate.recovery_path)?;
            Ok(FileIoItem {
                path: candidate.recovery_path.clone(),
                name: candidate
                    .recovery_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_owned(),
                format: None,
                identity,
                identity_physical,
                source_generation: 1,
                document_uuid: candidate
                    .metadata
                    .as_ref()
                    .map_or(0, |metadata| metadata.document_uuid),
            })
        })
        .collect::<Result<Vec<_>, inkpod_io::IoError>>()?;
    Ok((Prepared::Recovery(candidates), items))
}

impl FileIoJob {
    /// Starts proof-checked cleanup of one obsolete append-only recovery
    /// generation. The job owns the copied path and proof, does not borrow a
    /// live Core, and leaves changed/mixed artifacts untouched.
    pub fn start_recovery_discard_exact(
        manager: IoManager,
        path: PathBuf,
        proof: RecoveryArtifactProof,
    ) -> Result<Self, CoreError> {
        let request = FileIoRequest::new(FileIoKind::RecoveryDiscard, vec![path.clone()]);
        super::prepare::validate_request(&request)?;
        let worker_manager = manager.clone();
        let mut job = Self::allocate(None, manager.clone(), request)?;
        job.pending = Some(super::job::Pending::Prepare(manager.submit(
            move |context| {
                let result = (|| {
                    worker_manager
                        .discard_recovery_with_proof(&path, proof, &context)
                        .map_err(exact_discard_error)?;
                    Ok((Prepared::Recovery(Vec::new()), Vec::new()))
                })();
                Ok(result)
            },
        )?));
        Ok(job)
    }

    /// Borrows bounded typed metadata for one completed recovery candidate.
    /// Missing/corrupt metadata does not erase the corresponding native candidate.
    pub fn recovery(&self, index: usize) -> Result<&RecoveryCandidate, CoreError> {
        self.recoveries.get(index).ok_or(CoreError::InvalidArgument(
            "recovery item index is outside bounds",
        ))
    }
}

pub(super) fn export_sequence(
    core: Core,
    manager: &IoManager,
    request: &FileIoRequest,
    context: &JobContext,
) -> Result<(Prepared, Vec<FileIoItem>), CoreError> {
    let format = request.raster_format.ok_or(CoreError::InvalidArgument(
        "sequence export format is missing",
    ))?;
    let directory = &request.paths[0];
    manager.create_dir_all(directory, context)?;
    let sequence = core
        .sequence
        .as_ref()
        .ok_or(CoreError::InvalidState("no sequence is configured"))?;
    let prefix = request
        .paths
        .get(1)
        .map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .ok_or(CoreError::InvalidArgument(
                    "sequence output prefix is invalid",
                ))
        })
        .transpose()?;
    let destinations = sequence
        .cells
        .iter()
        .map(|cell| {
            let stem = std::path::Path::new(&cell.name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .ok_or(CoreError::InvalidArgument(
                    "sequence output name is invalid",
                ))?;
            let name = prefix.map_or_else(|| stem.to_owned(), |prefix| format!("{prefix}-{stem}"));
            Ok(directory.join(format!("{name}.{}", format_extension(format))))
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    let mut seen = std::collections::BTreeSet::new();
    for destination in &destinations {
        let identity = manager.resolve_identity(destination)?.0;
        if !seen.insert(identity) {
            return Err(CoreError::InvalidArgument(
                "sequence outputs alias the same file",
            ));
        }
        if !request.overwrite_confirmed && manager.exists(destination, context)? {
            return Err(CoreError::FileConflict);
        }
    }
    let total = destinations.len() as u64;
    for (index, (source, destination)) in sequence.cells.iter().zip(destinations).enumerate() {
        context.check_cancelled()?;
        let bytes = source.encode_raster(format, request.composite_white)?;
        if request.overwrite_confirmed {
            manager.write_bytes_atomic(&destination, &bytes, context)?;
        } else {
            manager.write_new_atomic(&destination, context, |writer| {
                use std::io::Write;
                writer.write_all(&bytes)?;
                Ok(())
            })?;
        }
        context.set_work(index as u64 + 1, total);
    }
    Ok((Prepared::Output(None), Vec::new()))
}
