use super::model::{BatchSource, BatchSourceContent};
use super::operations::*;
use super::validation::{path_label, within_range};
use super::*;
use crate::animation::natural_cmp;
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
        graph.validate()?;
        let sources = self.resolve_batch_sources(graph, scope)?;
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
                if !paths.insert(path.clone()) {
                    warnings.push("multiple inputs resolve to the same output path".to_owned());
                }
                if path.exists() {
                    warnings.push("output path already exists".to_owned());
                }
                if source.input_path.as_deref() == Some(path) {
                    warnings.push("output path resolves to the input path".to_owned());
                }
            }
            match working_core(source).and_then(|mut working| {
                working
                    .apply_batch_operations(&graph.operations, || false)
                    .map(|_| ())
            }) {
                Ok(()) => {}
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

    /// Executes a validated Batch v3 graph.
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
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<BatchRunReport, CoreError> {
        graph.validate()?;
        if graph.output.preview_before_save && !options.dry_run && !options.preview_confirmed {
            return Err(CoreError::InvalidState(
                "batch output requires preview confirmation before execution",
            ));
        }
        let sources = self.resolve_batch_sources(graph, options.scope)?;
        if graph.output.destination == BatchOutputDestination::NewTabs
            && sources.len() > new_tab_capacity
        {
            return Err(CoreError::InvalidState(
                "batch new-tab output exceeds the application session capacity",
            ));
        }
        let output_paths = preflight_output_paths(graph, &sources)?;
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
                let mut working = working_core(&sources[0])?;
                working.apply_batch_operations(&graph.operations, || false)?;
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
                let mut working = working_core(source)?;
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
                        save_batch_output(&working, graph, source, path, || {
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

    fn resolve_batch_sources(
        &self,
        graph: &BatchGraph,
        scope: BatchRunScope,
    ) -> Result<Vec<BatchSource>, CoreError> {
        let mut sources = Vec::new();
        let mut seen_paths = BTreeSet::new();
        for input in &graph.inputs {
            match input.kind {
                BatchInputKind::File => {
                    let path = PathBuf::from(&input.path);
                    validate_supported_input_path(&path)?;
                    if !path.is_file() {
                        return Err(CoreError::InvalidArgument("batch file input is missing"));
                    }
                    if within_range(path_label(&path), input) {
                        if !seen_paths.insert(path.clone()) {
                            return Err(CoreError::InvalidArgument(
                                "batch input contains a duplicate file",
                            ));
                        }
                        sources.push(path_source(path));
                    }
                }
                BatchInputKind::Folder => {
                    let mut paths = Vec::new();
                    for entry in fs::read_dir(&input.path)
                        .map_err(|error| CoreError::Format(error.to_string()))?
                    {
                        let path = entry
                            .map_err(|error| CoreError::Format(error.to_string()))?
                            .path();
                        if path.is_file()
                            && is_supported_input_path(&path)
                            && within_range(path_label(&path), input)
                        {
                            paths.push(path);
                        }
                    }
                    paths.sort_by(|left, right| natural_cmp(path_label(left), path_label(right)));
                    for path in paths {
                        if !seen_paths.insert(path.clone()) {
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
                        },
                    });
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
) -> Result<Vec<Option<PathBuf>>, CoreError> {
    if graph.output.destination != BatchOutputDestination::Folder {
        return Ok(vec![None; sources.len()]);
    }
    let mut seen = BTreeSet::new();
    let mut paths = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let path = output_path_for(graph, source, index)?;
        if !seen.insert(path.clone()) {
            return Err(CoreError::InvalidArgument(
                "batch output naming produces a duplicate path",
            ));
        }
        if source.input_path.as_deref() == Some(path.as_path()) {
            return Err(CoreError::InvalidArgument(
                "batch output naming resolves to an input path",
            ));
        }
        if path.exists() {
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
    core.current_path = None;
    core.savepoint = None;
    Ok(BatchStagedResult {
        generation: index as u64 + 1,
        core: Box::new(core),
    })
}
