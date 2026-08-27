use super::job::{FileIoJob, Pending, Prepared};
use super::model::{FileIoItem, FileIoKind, FileIoRequest, FileIoState};
use crate::{
    CompactionPlan, Core, CoreError, DocumentSaveToken, PreparedSequenceSwitch,
    SequenceSwitchRequest,
};
use inkpod_format::NativeFile;
use inkpod_io::{IoManager, JobContext, RecoveryMetadata};
use std::path::PathBuf;

fn write_native(
    manager: &IoManager,
    path: &std::path::Path,
    file: &NativeFile,
    metadata: Option<&RecoveryMetadata>,
    new_file: bool,
    context: &JobContext,
) -> Result<(), CoreError> {
    let writer = |writer: &mut std::fs::File| {
        inkpod_format::write_procedure_to_writer(writer, file, || context.is_cancelled())?;
        Ok(())
    };
    if let Some(metadata) = metadata {
        manager.write_recovery(path, metadata, context, writer)?;
    } else if new_file {
        manager.write_new_atomic(path, context, writer)?;
    } else {
        manager.write_atomic(path, context, writer)?;
    }
    Ok(())
}

impl FileIoJob {
    /// Prepares a sequence switch without blocking the owner. The source recovery
    /// is installed only after owner validation/fencing; then final apply switches
    /// once. Target recovery is optional and never changes the source's savepoint.
    pub fn start_sequence_switch(
        core: &Core,
        manager: IoManager,
        request: SequenceSwitchRequest,
        source_recovery: Option<PathBuf>,
        target_recovery: Option<PathBuf>,
        metadata: Option<RecoveryMetadata>,
    ) -> Result<Self, CoreError> {
        if request.requires_switch() && source_recovery.is_none() {
            return Err(CoreError::InvalidArgument(
                "sequence switch requires a source recovery destination",
            ));
        }
        let requires_switch = request.requires_switch();
        if let Some(metadata) = &metadata {
            let document = core.document.as_ref().ok_or(CoreError::NoDocument)?;
            if metadata.document_uuid != document.uuid {
                return Err(CoreError::InvalidArgument(
                    "sequence recovery metadata belongs to a different document",
                ));
            }
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
                    let native = if let Some(path) = target_recovery.filter(|_| requires_switch) {
                        if manager.exists(&path, &context)? {
                            let native = manager.with_reader(
                                &path,
                                1024 * 1024 * 1024,
                                &context,
                                |reader| {
                                    Ok(inkpod_format::read_procedure_from_reader(reader, || {
                                        context.is_cancelled()
                                    })?)
                                },
                            )?;
                            Some(native)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let target = snapshot.prepare(native, || context.is_cancelled())?;
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
                    .map(|()| None),
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
            Ok(write_native(&manager, &path, &file, None, true, &context).map(|()| None))
        })?;
        self.save_token = Some(token);
        self.pending = Some(Pending::Install(pending));
        self.progress.installing = true;
        self.progress.state = FileIoState::Running;
        Ok(())
    }
}
