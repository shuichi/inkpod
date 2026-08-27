use super::model::{BatchSource, BatchSourceContent};
use super::operations::*;
use super::validation::{path_label, within_range};
use super::*;
use crate::animation::natural_cmp;
use inkpod_io::{IoError, IoManager, JobContext};
use std::collections::BTreeSet;
use std::time::Duration;

impl Core {
    /// Expands inputs, validates native-depth operations, and derives collision data.
    ///
    /// The receiver's document, revision, history, journal, dirty state, and savepoint
    /// are unchanged.
    pub fn batch_preview(
        &self,
        graph: &BatchGraph,
        scope: BatchRunScope,
    ) -> Result<BatchPreview, CoreError> {
        self.batch_preview_with_context(graph, scope, &JobContext::new())
    }

    pub(crate) fn batch_preview_with_context(
        &self,
        graph: &BatchGraph,
        scope: BatchRunScope,
        context: &JobContext,
    ) -> Result<BatchPreview, CoreError> {
        graph.validate()?;
        let manager = self.file_io_manager()?;
        let sources = self.resolve_batch_sources(graph, scope, &manager, context)?;
        let mut paths = BTreeSet::new();
        let mut items = Vec::with_capacity(sources.len());
        for (index, source) in sources.iter().enumerate() {
            let output_path = if graph.output.destination == BatchOutputDestination::Folder {
                Some(output_path_for(graph, source, index)?)
            } else {
                None
            };
            let mut warnings = Vec::new();
            if let Some(path) = &output_path {
                if !paths.insert(manager.resolve_identity(path)?.0) {
                    warnings.push("multiple inputs resolve to the same output path".to_owned());
                }
                if manager.exists(path, context)? {
                    warnings.push("output path already exists".to_owned());
                }
                if source.input_path.as_deref() == Some(path) {
                    warnings.push("output path resolves to the input path".to_owned());
                }
            }
            match working_core(source, &manager, context).and_then(|mut working| {
                working
                    .apply_batch_operations(&graph.operations, || context.is_cancelled())
                    .map(|_| ())
            }) {
                Ok(()) => {}
                Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                Err(error) => warnings.push(error.to_string()),
            }
            items.push(BatchPreviewItem {
                input_name: source.label.clone(),
                output_path,
                warnings,
            });
        }
        Ok(BatchPreview { items })
    }

    /// Executes a validated Batch v4 graph.
    pub fn batch_execute(
        &mut self,
        graph: &BatchGraph,
        options: BatchRunOptions,
        progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<BatchRunReport, CoreError> {
        self.batch_execute_with_new_tab_capacity(graph, options, usize::MAX, progress)
    }

    /// Executes a graph after checking application-wide new-session capacity.
    pub fn batch_execute_with_new_tab_capacity(
        &mut self,
        graph: &BatchGraph,
        options: BatchRunOptions,
        new_tab_capacity: usize,
        progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<BatchRunReport, CoreError> {
        self.batch_execute_with_context(
            graph,
            options,
            new_tab_capacity,
            &JobContext::new(),
            progress,
        )
    }

    pub(crate) fn batch_execute_with_context(
        &mut self,
        graph: &BatchGraph,
        options: BatchRunOptions,
        new_tab_capacity: usize,
        context: &JobContext,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<BatchRunReport, CoreError> {
        graph.validate()?;
        let mut progress = |completed, total| !context.is_cancelled() && progress(completed, total);
        if graph.output.preview_before_save && !options.dry_run && !options.preview_confirmed {
            return Err(CoreError::InvalidState(
                "batch output requires preview confirmation before execution",
            ));
        }
        let manager = self.file_io_manager()?;
        let sources = self.resolve_batch_sources(graph, options.scope, &manager, context)?;
        if graph.output.destination == BatchOutputDestination::NewTabs
            && sources.len() > new_tab_capacity
        {
            return Err(CoreError::InvalidState(
                "batch new-tab output exceeds the application session capacity",
            ));
        }
        let output_paths = preflight_output_paths(graph, &sources, &manager, context)?;
        let total = sources.len().max(1) as u64;
        let mut report = BatchRunReport {
            items: Vec::with_capacity(sources.len()),
            cancelled: false,
            staged_results: Vec::new(),
        };
        if !progress(0, total) {
            report.cancelled = true;
            return Ok(report);
        }

        if graph.output.destination == BatchOutputDestination::ActiveDocument {
            if sources.len() != 1
                || !graph
                    .inputs
                    .iter()
                    .all(|input| input.kind == BatchInputKind::ActiveDocument)
            {
                return Err(CoreError::InvalidArgument(
                    "active-document output requires exactly one active-document input",
                ));
            }
            if options.dry_run {
                let mut working = working_core(&sources[0], &manager, context)?;
                working.apply_batch_operations(&graph.operations, || context.is_cancelled())?;
                report.items.push(BatchItemResult {
                    input_name: sources[0].label.clone(),
                    output_path: None,
                    outcome: BatchItemOutcome::DryRun,
                    message: "validated in memory; active document was unchanged".to_owned(),
                });
            } else {
                match self.apply_batch_operations(&graph.operations, || !progress(0, 1)) {
                    Ok(_) => report.items.push(BatchItemResult {
                        input_name: sources[0].label.clone(),
                        output_path: None,
                        outcome: BatchItemOutcome::Succeeded,
                        message: "applied to the issue-time active document as one Undo unit"
                            .to_owned(),
                    }),
                    Err(CoreError::Cancelled) => {
                        report.items.push(cancelled_item(&sources[0], None));
                        report.cancelled = true;
                    }
                    Err(error) => return Err(error),
                }
            }
            let _ = progress(1, 1);
            return Ok(report);
        }

        for (index, source) in sources.iter().enumerate() {
            let output_path = output_paths[index].clone();
            if !progress(index as u64, total) {
                report.items.push(cancelled_item(source, output_path));
                report.cancelled = true;
                break;
            }
            let result = (|| {
                let mut working = working_core(source, &manager, context)?;
                working
                    .apply_batch_operations(&graph.operations, || !progress(index as u64, total))?;
                if options.dry_run {
                    return Ok((BatchItemOutcome::DryRun, None));
                }
                match graph.output.destination {
                    BatchOutputDestination::Folder => {
                        let path = output_path.as_deref().ok_or(CoreError::InvalidState(
                            "batch folder output path is unavailable",
                        ))?;
                        save_batch_output(&working, graph, source, path, context, || {
                            !progress(index as u64, total)
                        })?;
                        Ok((BatchItemOutcome::Succeeded, None))
                    }
                    BatchOutputDestination::NewTabs => Ok((
                        BatchItemOutcome::Succeeded,
                        Some(stage_new_tab_result(working, index)?),
                    )),
                    BatchOutputDestination::ActiveDocument => unreachable!(),
                }
            })();
            match result {
                Ok((outcome, staged)) => {
                    if let Some(staged) = staged {
                        report.staged_results.push(staged);
                    }
                    report.items.push(BatchItemResult {
                        input_name: source.label.clone(),
                        output_path,
                        outcome,
                        message: if outcome == BatchItemOutcome::DryRun {
                            "validated and processed in memory; no output was written".to_owned()
                        } else {
                            "completed".to_owned()
                        },
                    });
                }
                Err(CoreError::Cancelled) => {
                    report.items.push(cancelled_item(source, output_path));
                    report.cancelled = true;
                    break;
                }
                Err(error) => {
                    report.items.push(BatchItemResult {
                        input_name: source.label.clone(),
                        output_path,
                        outcome: BatchItemOutcome::Failed,
                        message: error.to_string(),
                    });
                    if graph.output.failure_policy == BatchFailurePolicy::Stop {
                        break;
                    }
                }
            }
            let _ = progress((index + 1) as u64, total);
            if graph.output.wait_milliseconds != 0 && index + 1 < sources.len() {
                let mut remaining = graph.output.wait_milliseconds;
                while remaining != 0 {
                    if !progress((index + 1) as u64, total) {
                        report.cancelled = true;
                        break;
                    }
                    let interval = remaining.min(BATCH_WAIT_POLL_MILLISECONDS);
                    std::thread::sleep(Duration::from_millis(u64::from(interval)));
                    remaining -= interval;
                }
                if report.cancelled {
                    break;
                }
            }
        }
        Ok(report)
    }

    pub(super) fn resolve_batch_sources(
        &self,
        graph: &BatchGraph,
        scope: BatchRunScope,
        manager: &IoManager,
        context: &JobContext,
    ) -> Result<Vec<BatchSource>, CoreError> {
        let mut sources = Vec::new();
        let mut seen_paths = BTreeSet::new();
        for input in &graph.inputs {
            context.check_cancelled()?;
            match input.kind {
                BatchInputKind::File => {
                    let path = PathBuf::from(&input.path);
                    validate_supported_input_path(&path)?;
                    let identity = match manager.metadata(&path, context) {
                        Ok(metadata) => metadata.identity,
                        Err(IoError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                            return Err(CoreError::InvalidArgument("batch file input is missing"));
                        }
                        Err(error) => return Err(error.into()),
                    };
                    if within_range(path_label(&path), input) {
                        if !seen_paths.insert(identity) {
                            return Err(CoreError::InvalidArgument(
                                "batch input contains a duplicate file",
                            ));
                        }
                        sources.push(path_source(path));
                        check_source_count(sources.len())?;
                    }
                }
                BatchInputKind::Folder => {
                    let mut paths = Vec::new();
                    for path in manager.list_files(Path::new(&input.path), 1_000_000, context)? {
                        if is_supported_input_path(&path) && within_range(path_label(&path), input)
                        {
                            paths.push(path);
                            check_source_count(sources.len() + paths.len())?;
                        }
                    }
                    paths.sort_by(|left, right| natural_cmp(path_label(left), path_label(right)));
                    for path in paths {
                        if !seen_paths.insert(manager.metadata(&path, context)?.identity) {
                            return Err(CoreError::InvalidArgument(
                                "batch input contains a duplicate file",
                            ));
                        }
                        sources.push(path_source(path));
                    }
                }
                BatchInputKind::ActiveDocument => {
                    let document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
                    let label = self.current_path.as_deref().map_or_else(
                        || "active-document.inkpod".to_owned(),
                        |path| path_label(path).to_owned(),
                    );
                    sources.push(BatchSource {
                        label,
                        input_path: self.current_path.clone(),
                        content: BatchSourceContent::Document {
                            document: Box::new(document),
                            assets: self.assets.clone(),
                            raster_file_format: self.raster_file_format,
                        },
                    });
                    check_source_count(sources.len())?;
                }
            }
        }
        if sources.is_empty() {
            return Err(CoreError::InvalidArgument(
                "batch input selector resolved to no supported items",
            ));
        }
        if scope == BatchRunScope::Current {
            return Ok(vec![sources.remove(0)]);
        }
        Ok(sources)
    }

    pub(crate) fn batch_freeze_inputs(
        &self,
        graph: &mut BatchGraph,
        scope: BatchRunScope,
        manager: &IoManager,
        context: &JobContext,
    ) -> Result<Vec<PathBuf>, CoreError> {
        graph.validate()?;
        let sources = self.resolve_batch_sources(graph, scope, manager, context)?;
        let mut inputs = Vec::with_capacity(sources.len());
        let mut paths = Vec::new();
        for source in sources {
            match source.content {
                BatchSourceContent::Path(path) => {
                    let path_text = path.to_str().ok_or(CoreError::InvalidArgument(
                        "batch input path is not valid UTF-8",
                    ))?;
                    inputs.push(BatchInputSelector::file(path_text));
                    paths.push(path);
                }
                BatchSourceContent::Document { .. } => {
                    inputs.push(BatchInputSelector::active_document());
                }
            }
        }
        context.check_cancelled()?;
        graph.inputs = inputs;
        Ok(paths)
    }
}

fn check_source_count(count: usize) -> Result<(), CoreError> {
    if count > MAX_BATCH_INPUTS {
        return Err(CoreError::InvalidArgument(
            "batch resolved input count exceeds its bounded limit",
        ));
    }
    Ok(())
}

fn path_source(path: PathBuf) -> BatchSource {
    BatchSource {
        label: path_label(&path).to_owned(),
        input_path: Some(path.clone()),
        content: BatchSourceContent::Path(path),
    }
}

fn is_supported_input_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("inkpod")
                || CommonRasterFormat::from_extension(extension).is_some()
        })
}

fn validate_supported_input_path(path: &Path) -> Result<(), CoreError> {
    if is_supported_input_path(path) {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "batch input extension is unsupported",
        ))
    }
}

fn preflight_output_paths(
    graph: &BatchGraph,
    sources: &[BatchSource],
    manager: &IoManager,
    context: &JobContext,
) -> Result<Vec<Option<PathBuf>>, CoreError> {
    if graph.output.destination != BatchOutputDestination::Folder {
        return Ok(vec![None; sources.len()]);
    }
    let mut seen = BTreeSet::new();
    let mut paths = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let path = output_path_for(graph, source, index)?;
        if !seen.insert(manager.resolve_identity(&path)?.0) {
            return Err(CoreError::InvalidArgument(
                "batch output naming produces a duplicate path",
            ));
        }
        if source.input_path.as_deref() == Some(path.as_path()) {
            return Err(CoreError::InvalidArgument(
                "batch output naming resolves to an input path",
            ));
        }
        if manager.exists(&path, context)? {
            return Err(CoreError::InvalidState("batch output path already exists"));
        }
        paths.push(Some(path));
    }
    Ok(paths)
}

fn stage_new_tab_result(working: Core, index: usize) -> Result<BatchStagedResult, CoreError> {
    let mut document = working
        .document
        .as_ref()
        .ok_or(CoreError::NoDocument)?
        .clone();
    let source_uuid = document.uuid;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"org.inkpod.batch-new-tab-identity.v1");
    hasher.update(&source_uuid.to_le_bytes());
    hasher.update(&(index as u64).to_le_bytes());
    hasher.update(&working.document_revision.get().to_le_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    let mut uuid = u128::from_le_bytes(bytes);
    if uuid == 0 || uuid == source_uuid {
        uuid = source_uuid.wrapping_add(1).max(1);
    }
    document.uuid = uuid;
    let mut core = core_from_document(document, working.assets.clone())?;
    core.raster_file_format = working.raster_file_format;
    core.new_cell_raster_format = working.new_cell_raster_format;
    core.io_manager = working.io_manager.clone();
    core.current_path = None;
    core.savepoint = None;
    Ok(BatchStagedResult {
        generation: index as u64 + 1,
        core: Box::new(core),
    })
}
