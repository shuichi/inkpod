use super::model::{BatchSource, BatchSourceContent};
use super::operations::*;
use super::validation::{path_label, within_cell_range, within_range};
use super::*;
use crate::animation::natural_cmp;
use std::collections::BTreeMap;
use std::time::Duration;

impl Core {
    /// Expands inputs and derives output paths/warnings without writing outputs.
    ///
    /// The graph is fully validated and source documents may be decoded for
    /// inspection, but the active Core document, revision, history, and dirty state
    /// are unchanged.
    pub fn batch_preview(
        &self,
        graph: &BatchGraph,
        scope: BatchRunScope,
    ) -> Result<BatchPreview, CoreError> {
        graph.validate()?;
        let sources = self.resolve_batch_sources(graph, scope)?;
        let mut expected_seed_colors = BTreeMap::<(usize, usize), PixelValue>::new();
        let mut items = Vec::with_capacity(sources.len());
        for (source_index, source) in sources.iter().enumerate() {
            let output_path = output_path_for(graph, source, source_index).ok();
            let mut warnings = Vec::new();
            if let Some(path) = &output_path
                && graph.output.policy != BatchOutputPolicy::ExplicitOverwrite
                && source.input_path.as_ref() == Some(path)
            {
                warnings.push("default output policy would overwrite the input".to_owned());
            }
            let working = match working_core(source) {
                Ok(working) => working,
                Err(error) => {
                    warnings.push(error.to_string());
                    items.push(BatchPreviewItem {
                        input_name: source.label.clone(),
                        output_path,
                        warnings,
                    });
                    continue;
                }
            };
            for (operation_index, operation) in graph.operations.iter().enumerate() {
                if !operation.enabled {
                    continue;
                }
                if operation.configure_each_run {
                    warnings.push(format!(
                        "operation {} requires per-run confirmation",
                        operation_index + 1
                    ));
                }
                if let BatchOperationKind::ContinuousFill(seeds) = &operation.kind {
                    let Some(target) = operation.target.as_ref() else {
                        continue;
                    };
                    let Some((_, plane_id)) = resolve_target(&working, target)? else {
                        warnings.push(format!(
                            "operation {} target is absent and will be skipped",
                            operation_index + 1
                        ));
                        continue;
                    };
                    let document = working.document.as_ref().ok_or(CoreError::NoDocument)?;
                    let plane = document
                        .plane_by_id(PlaneId::from_raw(plane_id.ok_or(
                            CoreError::InvalidArgument("continuous fill requires a plane selector"),
                        )?))
                        .ok_or(CoreError::InvalidState("batch target plane disappeared"))?;
                    for (seed_index, seed) in seeds.iter().enumerate() {
                        if !seed.enabled {
                            continue;
                        }
                        let actual = plane.raster.pixel(seed.x, seed.y)?;
                        let expected = seed.expected_source.or_else(|| {
                            expected_seed_colors
                                .get(&(operation_index, seed_index))
                                .copied()
                        });
                        if let Some(expected) = expected
                            && actual != expected
                        {
                            warnings.push(format!(
                                "continuous-fill seed ({}, {}) moved to a different color in {}",
                                seed.x, seed.y, source.label
                            ));
                        }
                        expected_seed_colors
                            .entry((operation_index, seed_index))
                            .or_insert(actual);
                    }
                }
            }
            items.push(BatchPreviewItem {
                input_name: source.label.clone(),
                output_path,
                warnings,
            });
        }
        Ok(BatchPreview { items })
    }

    /// Executes a validated graph using isolated working Core instances.
    ///
    /// `progress(completed, total)` may return `false` to cancel before the next
    /// commit/output boundary. Failed, cancelled, and stale operations never publish
    /// partial working documents; already completed output files are reported and
    /// are not rolled back. This method never mutates the receiver's active document.
    pub fn batch_execute(
        &self,
        graph: &BatchGraph,
        options: BatchRunOptions,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<BatchRunReport, CoreError> {
        graph.validate()?;
        if graph
            .operations
            .iter()
            .any(|operation| operation.enabled && operation.configure_each_run)
        {
            return Err(CoreError::InvalidState(
                "batch run contains unresolved per-run configuration",
            ));
        }
        if graph.output.preview_before_save && !options.dry_run && !options.preview_confirmed {
            return Err(CoreError::InvalidState(
                "batch output requires preview confirmation before save",
            ));
        }
        let sources = self.resolve_batch_sources(graph, options.scope)?;
        let enabled_operations = graph
            .operations
            .iter()
            .filter(|operation| operation.enabled)
            .count() as u64;
        let per_item = enabled_operations.saturating_add(2);
        let total = (sources.len() as u64).saturating_mul(per_item).max(1);
        let mut completed = 0_u64;
        let mut report = BatchRunReport {
            items: Vec::with_capacity(sources.len()),
            cancelled: false,
        };
        if !progress(0, total) {
            report.cancelled = true;
            return Ok(report);
        }
        for (source_index, source) in sources.iter().enumerate() {
            let output_path = output_path_for(graph, source, source_index).ok();
            let mut working = match working_core(source) {
                Ok(working) => working,
                Err(error) => {
                    report.items.push(BatchItemResult {
                        input_name: source.label.clone(),
                        output_path,
                        outcome: BatchItemOutcome::Failed,
                        message: error.to_string(),
                    });
                    completed = completed.saturating_add(per_item);
                    if graph.output.failure_policy == BatchFailurePolicy::Stop {
                        break;
                    }
                    continue;
                }
            };
            completed = completed.saturating_add(1);
            if !progress(completed, total) {
                report.items.push(cancelled_item(source, output_path));
                report.cancelled = true;
                break;
            }
            let mut skipped = false;
            let mut operation_failure = None;
            for operation in graph
                .operations
                .iter()
                .filter(|operation| operation.enabled)
            {
                match apply_operation(&mut working, operation, |done, work| {
                    let fraction = done.saturating_mul(1_000).checked_div(work).unwrap_or(0);
                    let staged = completed.saturating_mul(1_000).saturating_add(fraction);
                    progress(staged, total.saturating_mul(1_000))
                }) {
                    Ok(OperationResult::Applied) => {}
                    Ok(OperationResult::Skipped) => skipped = true,
                    Err(CoreError::Cancelled) => {
                        report
                            .items
                            .push(cancelled_item(source, output_path.clone()));
                        report.cancelled = true;
                        operation_failure = Some(CoreError::Cancelled);
                        break;
                    }
                    Err(error) => {
                        operation_failure = Some(error);
                        break;
                    }
                }
                completed = completed.saturating_add(1);
                if !progress(completed, total) {
                    report
                        .items
                        .push(cancelled_item(source, output_path.clone()));
                    report.cancelled = true;
                    operation_failure = Some(CoreError::Cancelled);
                    break;
                }
            }
            if report.cancelled {
                break;
            }
            if let Some(error) = operation_failure {
                report.items.push(BatchItemResult {
                    input_name: source.label.clone(),
                    output_path,
                    outcome: BatchItemOutcome::Failed,
                    message: error.to_string(),
                });
                completed = completed.saturating_add(1);
                if graph.output.failure_policy == BatchFailurePolicy::Stop {
                    break;
                }
                continue;
            }
            if options.dry_run {
                completed = completed.saturating_add(1);
                report.items.push(BatchItemResult {
                    input_name: source.label.clone(),
                    output_path,
                    outcome: BatchItemOutcome::DryRun,
                    message: "validated and processed in memory; no output was written".to_owned(),
                });
                let _ = progress(completed, total);
            } else {
                let path = match output_path.clone() {
                    Some(path) => path,
                    None => {
                        report.items.push(BatchItemResult {
                            input_name: source.label.clone(),
                            output_path: None,
                            outcome: BatchItemOutcome::Failed,
                            message: "batch output path is unavailable".to_owned(),
                        });
                        if graph.output.failure_policy == BatchFailurePolicy::Stop {
                            break;
                        }
                        continue;
                    }
                };
                let save_result = save_batch_output(&working, graph, source, &path, || {
                    !progress(completed, total)
                });
                match save_result {
                    Ok(()) => {
                        completed = completed.saturating_add(1);
                        report.items.push(BatchItemResult {
                            input_name: source.label.clone(),
                            output_path: Some(path),
                            outcome: if skipped {
                                BatchItemOutcome::Skipped
                            } else {
                                BatchItemOutcome::Succeeded
                            },
                            message: if skipped {
                                "completed with one or more missing targets skipped".to_owned()
                            } else {
                                "completed".to_owned()
                            },
                        });
                        let _ = progress(completed, total);
                    }
                    Err(CoreError::Cancelled) => {
                        report.items.push(cancelled_item(source, Some(path)));
                        report.cancelled = true;
                        break;
                    }
                    Err(error) => {
                        report.items.push(BatchItemResult {
                            input_name: source.label.clone(),
                            output_path: Some(path),
                            outcome: BatchItemOutcome::Failed,
                            message: error.to_string(),
                        });
                        if graph.output.failure_policy == BatchFailurePolicy::Stop {
                            break;
                        }
                    }
                }
            }
            if graph.output.wait_milliseconds != 0 && source_index + 1 < sources.len() {
                let mut remaining = graph.output.wait_milliseconds;
                while remaining != 0 {
                    if !progress(completed, total) {
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
        for input in &graph.inputs {
            match input.kind {
                BatchInputKind::File => {
                    let path = PathBuf::from(&input.path);
                    if within_range(path_label(&path), input) {
                        sources.push(BatchSource {
                            label: path_label(&path).to_owned(),
                            input_path: Some(path.clone()),
                            content: BatchSourceContent::Path(path),
                        });
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
                            && path
                                .extension()
                                .is_some_and(|extension| extension.eq_ignore_ascii_case("inkpod"))
                            && within_range(path_label(&path), input)
                        {
                            paths.push(path);
                        }
                    }
                    paths.sort_by(|left, right| natural_cmp(path_label(left), path_label(right)));
                    sources.extend(paths.into_iter().map(|path| BatchSource {
                        label: path_label(&path).to_owned(),
                        input_path: Some(path.clone()),
                        content: BatchSourceContent::Path(path),
                    }));
                }
                BatchInputKind::CurrentSequence => {
                    if let Some(sequence) = &self.sequence {
                        sources.extend(
                            sequence
                                .cells
                                .iter()
                                .filter(|cell| within_cell_range(cell.cell_number, input))
                                .cloned()
                                .map(|cell| BatchSource {
                                    label: cell.name.clone(),
                                    input_path: None,
                                    content: BatchSourceContent::Sequence(cell),
                                }),
                        );
                    } else {
                        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
                        let label = self.current_path.as_deref().map_or_else(
                            || "current-cell.inkpod".to_owned(),
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
        }
        sources.sort_by(|left, right| natural_cmp(&left.label, &right.label));
        if sources.is_empty() {
            return Err(CoreError::InvalidArgument(
                "batch input selector resolved to no cells",
            ));
        }
        if scope == BatchRunScope::Current {
            let current_uuid = self.document.as_ref().map(|document| document.uuid);
            let current_path = self.current_path.as_ref();
            let index = sources
                .iter()
                .position(|source| match &source.content {
                    BatchSourceContent::Document { document, .. } => {
                        current_uuid.is_some_and(|uuid| document.uuid == uuid)
                    }
                    BatchSourceContent::Sequence(cell) => {
                        current_uuid.is_some_and(|uuid| cell.document_uuid == uuid)
                    }
                    BatchSourceContent::Path(path) => current_path == Some(path),
                })
                .ok_or(CoreError::InvalidArgument(
                    "current cell is not included in the batch input",
                ))?;
            return Ok(vec![sources.remove(index)]);
        }
        Ok(sources)
    }
}
