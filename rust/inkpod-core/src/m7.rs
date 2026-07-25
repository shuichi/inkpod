use super::{
    ActivePlane, BoundaryAirbrush, CellDocument, Core, CoreError, DocumentResize, DustRemoval,
    FillOperation, FillRequest, Filter, InclusionMode, LayerKind, MAX_IMAGE_EDIT_PIXELS,
    MirrorAxis, PixelFormat, PixelValue, PlaneType, ResizeAnchor, RotateDirection, TILE_SIZE,
    TileCoord, VectorWidthMode,
};
use crate::m4::{SequenceCellSource, natural_cmp, parse_cell_number};
use inkpod_format::{
    BATCH_GRAPH_VERSION, FileBatchGraph, FileBatchInput, FileBatchOperation, FileBatchOutput,
    FileBatchTarget, read_batch_graph, save_batch_graph_atomic,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const BATCH_OPERATION_VERSION: u32 = 1;
const MAX_BATCH_COLOR_PAIRS: usize = 4_096;
const MAX_BATCH_SEEDS: usize = 4_096;
const MAX_BATCH_COLORS: usize = 4_096;
const MAX_BATCH_INPUTS: usize = 16_384;
const MAX_BATCH_OPERATIONS: usize = 1_024;
const MAX_BATCH_NAME_BYTES: usize = 1_024;
const MAX_BATCH_PATH_BYTES: usize = 32_768;
const BATCH_WAIT_POLL_MILLISECONDS: u32 = 50;

const INPUT_FILE: u32 = 1;
const INPUT_FOLDER: u32 = 2;
const INPUT_CURRENT_SEQUENCE: u32 = 3;

const OUTPUT_DUPLICATE: u32 = 1;
const OUTPUT_NEW_SAVE: u32 = 2;
const OUTPUT_OVERWRITE: u32 = 3;
const OUTPUT_NATIVE_INKPOD: u32 = 1;
const FAILURE_CONTINUE: u32 = 1;
const FAILURE_STOP: u32 = 2;
const MISSING_SKIP: u32 = 1;
const MISSING_ERROR: u32 = 2;

const OP_COLOR_REPLACE: u32 = 1;
const OP_CONTINUOUS_FILL: u32 = 2;
const OP_SEPARATION: u32 = 3;
const OP_VISIBILITY: u32 = 4;
const OP_LINE_WIDTH: u32 = 5;
const OP_FILTER: u32 = 6;
const OP_BOUNDARY_AIRBRUSH: u32 = 7;
const OP_DUST_REMOVAL: u32 = 8;
const OP_MIRROR: u32 = 9;
const OP_ROTATE_90: u32 = 10;
const OP_RESIZE: u32 = 11;
const OP_CONVERT_PLANE: u32 = 12;

const OP_ENABLED: u64 = 1;
const OP_CONFIGURE_EACH_RUN: u64 = 1 << 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchInputKind {
    File,
    Folder,
    CurrentSequence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchInputSelector {
    pub kind: BatchInputKind,
    pub path: String,
    /// Zero means unbounded.
    pub first_cell: u32,
    /// Zero means unbounded.
    pub last_cell: u32,
}

impl BatchInputSelector {
    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self {
            kind: BatchInputKind::File,
            path: path.into(),
            first_cell: 0,
            last_cell: 0,
        }
    }

    #[must_use]
    pub fn current_sequence() -> Self {
        Self {
            kind: BatchInputKind::CurrentSequence,
            path: String::new(),
            first_cell: 0,
            last_cell: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchMissingTargetPolicy {
    Skip,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchTargetSelector {
    pub layer_id: Option<u64>,
    pub plane_id: Option<u64>,
    pub layer_kind: Option<LayerKind>,
    pub plane_kind: Option<PlaneType>,
    pub missing_policy: BatchMissingTargetPolicy,
}

impl BatchTargetSelector {
    #[must_use]
    pub const fn color_plane() -> Self {
        Self {
            layer_id: None,
            plane_id: None,
            layer_kind: Some(LayerKind::BinaryColoring),
            plane_kind: Some(PlaneType::Color),
            missing_policy: BatchMissingTargetPolicy::Error,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchColorPair {
    pub enabled: bool,
    pub old: PixelValue,
    pub new: PixelValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchSeed {
    pub x: u32,
    pub y: u32,
    pub color: PixelValue,
    pub tolerance: u16,
    pub gap_close: u8,
    pub expected_source: Option<PixelValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchSeparation {
    pub colors: Vec<PixelValue>,
    pub replacement: PixelValue,
    pub invert: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BatchOperationKind {
    ColorReplace(Vec<BatchColorPair>),
    ContinuousFill(Vec<BatchSeed>),
    Separation(BatchSeparation),
    Visibility {
        visible: bool,
    },
    LineWidth(VectorWidthMode),
    Filter(Filter),
    BoundaryAirbrush(BoundaryAirbrush),
    DustRemoval(DustRemoval),
    Mirror(MirrorAxis),
    Rotate90(RotateDirection),
    Resize(DocumentResize),
    ConvertPlane {
        destination_kind: PlaneType,
        destination_format: PixelFormat,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchOperation {
    pub version: u32,
    pub enabled: bool,
    pub configure_each_run: bool,
    pub target: Option<BatchTargetSelector>,
    pub kind: BatchOperationKind,
}

impl BatchOperation {
    pub fn swap_color_replacements(&mut self) -> Result<(), CoreError> {
        let BatchOperationKind::ColorReplace(pairs) = &mut self.kind else {
            return Err(CoreError::InvalidArgument(
                "batch operation is not color replacement",
            ));
        };
        for pair in pairs {
            std::mem::swap(&mut pair.old, &mut pair.new);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchOutputPolicy {
    Duplicate,
    NewSave,
    ExplicitOverwrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchFailurePolicy {
    Continue,
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchOutputSettings {
    pub policy: BatchOutputPolicy,
    pub folder: String,
    pub cell_folder: bool,
    pub basename: String,
    pub start_number: u32,
    pub descending: bool,
    pub failure_policy: BatchFailurePolicy,
    pub wait_milliseconds: u32,
    pub preview_before_save: bool,
}

impl Default for BatchOutputSettings {
    fn default() -> Self {
        Self {
            policy: BatchOutputPolicy::Duplicate,
            folder: String::new(),
            cell_folder: false,
            basename: String::new(),
            start_number: 1,
            descending: false,
            failure_policy: BatchFailurePolicy::Continue,
            wait_milliseconds: 0,
            preview_before_save: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchGraph {
    pub version: u32,
    pub name: String,
    pub inputs: Vec<BatchInputSelector>,
    pub operations: Vec<BatchOperation>,
    pub output: BatchOutputSettings,
}

impl BatchGraph {
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        save_batch_graph_atomic(path, &self.to_file()?)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, CoreError> {
        Self::from_file(read_batch_graph(path)?)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.version != BATCH_GRAPH_VERSION {
            return Err(CoreError::InvalidArgument(
                "batch graph version is unsupported",
            ));
        }
        validate_component(&self.name, true)?;
        if self.inputs.is_empty() || self.inputs.len() > MAX_BATCH_INPUTS {
            return Err(CoreError::InvalidArgument(
                "batch input count is outside bounds",
            ));
        }
        if self.operations.is_empty() || self.operations.len() > MAX_BATCH_OPERATIONS {
            return Err(CoreError::InvalidArgument(
                "batch operation count is outside bounds",
            ));
        }
        for input in &self.inputs {
            if input.first_cell != 0 && input.last_cell != 0 && input.first_cell > input.last_cell {
                return Err(CoreError::InvalidArgument("batch input range is reversed"));
            }
            match input.kind {
                BatchInputKind::File | BatchInputKind::Folder if input.path.is_empty() => {
                    return Err(CoreError::InvalidArgument(
                        "batch file or folder input path is empty",
                    ));
                }
                BatchInputKind::CurrentSequence if !input.path.is_empty() => {
                    return Err(CoreError::InvalidArgument(
                        "current-sequence input must not contain a path",
                    ));
                }
                _ => {}
            }
            validate_path(&input.path)?;
        }
        if self.output.wait_milliseconds > 3_600_000 {
            return Err(CoreError::InvalidArgument(
                "batch wait duration exceeds one hour",
            ));
        }
        validate_component(&self.output.basename, false)?;
        validate_path(&self.output.folder)?;
        for operation in &self.operations {
            validate_operation(operation)?;
        }
        Ok(())
    }

    fn to_file(&self) -> Result<FileBatchGraph, CoreError> {
        self.validate()?;
        Ok(FileBatchGraph {
            version: self.version,
            name: self.name.clone(),
            inputs: self
                .inputs
                .iter()
                .map(|input| FileBatchInput {
                    kind: input_kind_code(input.kind),
                    path: input.path.clone(),
                    first_cell: input.first_cell,
                    last_cell: input.last_cell,
                })
                .collect(),
            operations: self
                .operations
                .iter()
                .map(operation_to_file)
                .collect::<Result<Vec<_>, _>>()?,
            output: FileBatchOutput {
                policy: output_policy_code(self.output.policy),
                folder: self.output.folder.clone(),
                cell_folder: self.output.cell_folder,
                format: OUTPUT_NATIVE_INKPOD,
                basename: self.output.basename.clone(),
                start_number: self.output.start_number,
                descending: self.output.descending,
                failure_policy: failure_policy_code(self.output.failure_policy),
                wait_milliseconds: self.output.wait_milliseconds,
                preview_before_save: self.output.preview_before_save,
            },
        })
    }

    fn from_file(file: FileBatchGraph) -> Result<Self, CoreError> {
        if file.output.format != OUTPUT_NATIVE_INKPOD {
            return Err(CoreError::InvalidArgument(
                "batch output format is unsupported",
            ));
        }
        let graph = Self {
            version: file.version,
            name: file.name,
            inputs: file
                .inputs
                .into_iter()
                .map(|input| {
                    Ok(BatchInputSelector {
                        kind: parse_input_kind(input.kind)?,
                        path: input.path,
                        first_cell: input.first_cell,
                        last_cell: input.last_cell,
                    })
                })
                .collect::<Result<Vec<_>, CoreError>>()?,
            operations: file
                .operations
                .into_iter()
                .map(operation_from_file)
                .collect::<Result<Vec<_>, _>>()?,
            output: BatchOutputSettings {
                policy: parse_output_policy(file.output.policy)?,
                folder: file.output.folder,
                cell_folder: file.output.cell_folder,
                basename: file.output.basename,
                start_number: file.output.start_number,
                descending: file.output.descending,
                failure_policy: parse_failure_policy(file.output.failure_policy)?,
                wait_milliseconds: file.output.wait_milliseconds,
                preview_before_save: file.output.preview_before_save,
            },
        };
        graph.validate()?;
        Ok(graph)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchRunScope {
    Current,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchRunOptions {
    pub scope: BatchRunScope,
    pub dry_run: bool,
    pub preview_confirmed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchPreviewItem {
    pub input_name: String,
    pub output_path: Option<PathBuf>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchPreview {
    pub items: Vec<BatchPreviewItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchItemOutcome {
    Succeeded,
    Skipped,
    Failed,
    Cancelled,
    DryRun,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchItemResult {
    pub input_name: String,
    pub output_path: Option<PathBuf>,
    pub outcome: BatchItemOutcome,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRunReport {
    pub items: Vec<BatchItemResult>,
    pub cancelled: bool,
}

impl BatchRunReport {
    #[must_use]
    pub fn failure_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.outcome == BatchItemOutcome::Failed)
            .count()
    }
}

#[derive(Clone)]
enum BatchSourceContent {
    Path(PathBuf),
    Document(Box<CellDocument>),
    Sequence(SequenceCellSource),
}

#[derive(Clone)]
struct BatchSource {
    label: String,
    input_path: Option<PathBuf>,
    content: BatchSourceContent,
}

impl Core {
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
                        .plane_by_id(plane_id.ok_or(CoreError::InvalidArgument(
                            "continuous fill requires a plane selector",
                        ))?)
                        .ok_or(CoreError::InvalidState("batch target plane disappeared"))?;
                    for (seed_index, seed) in seeds.iter().enumerate() {
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

    pub fn batch_execute(
        &self,
        graph: &BatchGraph,
        options: BatchRunOptions,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<BatchRunReport, CoreError> {
        graph.validate()?;
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
                            content: BatchSourceContent::Document(Box::new(document)),
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
                    BatchSourceContent::Document(document) => {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationResult {
    Applied,
    Skipped,
}

fn apply_operation(
    core: &mut Core,
    operation: &BatchOperation,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<OperationResult, CoreError> {
    validate_operation(operation)?;
    let target = match operation.target.as_ref() {
        Some(selector) => match resolve_target(core, selector)? {
            Some(target) => Some(target),
            None => return Ok(OperationResult::Skipped),
        },
        None => None,
    };
    let target_plane = || {
        target
            .and_then(|(_, plane)| plane)
            .ok_or(CoreError::InvalidArgument(
                "batch operation requires a target plane",
            ))
    };
    match &operation.kind {
        BatchOperationKind::ColorReplace(pairs) => {
            apply_color_replacement(core, target_plane()?, pairs, &mut progress)?;
        }
        BatchOperationKind::ContinuousFill(seeds) => {
            let (layer_id, plane_id) = target.ok_or(CoreError::InvalidArgument(
                "continuous fill requires a stable target",
            ))?;
            let plane_id = plane_id.ok_or(CoreError::InvalidArgument(
                "continuous fill requires a target plane",
            ))?;
            core.set_active_node(layer_id, plane_id)?;
            for (index, seed) in seeds.iter().enumerate() {
                if !progress(index as u64, seeds.len() as u64) {
                    return Err(CoreError::Cancelled);
                }
                core.apply_fill_with_cancel(
                    &FillRequest {
                        operation: FillOperation::Seed,
                        seed_x: seed.x,
                        seed_y: seed.y,
                        color: seed.color,
                        selection: None,
                        use_document_selection: false,
                        tolerance: seed.tolerance,
                        detached_regions: false,
                        overflow_abort: true,
                        gap_close: seed.gap_close,
                        transparent_only: false,
                        inclusion_mode: InclusionMode::None,
                        inclusion_colors: Vec::new(),
                        extension_distance: 0,
                    },
                    || !progress(index as u64, seeds.len() as u64),
                )?;
            }
        }
        BatchOperationKind::Separation(options) => {
            apply_separation(core, target_plane()?, options, &mut progress)?;
        }
        BatchOperationKind::Visibility { visible } => {
            let (layer_id, plane_id) = target.ok_or(CoreError::InvalidArgument(
                "visibility requires a stable target",
            ))?;
            let layers = core.layers()?;
            let layer = layers
                .iter()
                .find(|layer| layer.id == layer_id)
                .ok_or(CoreError::InvalidState("batch layer target disappeared"))?;
            if let Some(plane_id) = plane_id {
                let plane = layer
                    .planes
                    .iter()
                    .find(|plane| plane.id == plane_id)
                    .ok_or(CoreError::InvalidState("batch plane target disappeared"))?;
                core.set_plane_properties(
                    plane.id,
                    *visible,
                    plane.editable,
                    plane.opacity_milli,
                    &plane.name,
                )?;
            } else {
                core.set_layer_properties(
                    layer.id,
                    *visible,
                    layer.editable,
                    layer.opacity_milli,
                    &layer.name,
                )?;
            }
        }
        BatchOperationKind::LineWidth(mode) => {
            let plane_id = target_plane()?;
            let ids: Vec<_> = core
                .vector_paths()?
                .into_iter()
                .filter(|path| path.plane_id == plane_id)
                .map(|path| path.id)
                .collect();
            if ids.is_empty() {
                return Err(CoreError::InvalidArgument(
                    "line-width target has no vector paths",
                ));
            }
            core.vector_correct_width(&ids, *mode)?;
        }
        BatchOperationKind::Filter(filter) => {
            let plane_id = target_plane()?;
            core.begin_filter_preview_with_progress(plane_id, filter.clone(), &mut progress)?;
            core.apply_filter_preview()?;
        }
        BatchOperationKind::BoundaryAirbrush(effect) => {
            core.apply_boundary_airbrush_to_plane(target_plane()?, effect)?;
        }
        BatchOperationKind::DustRemoval(options) => {
            core.apply_dust_removal_to_plane(target_plane()?, None, *options, &mut progress)?;
        }
        BatchOperationKind::Mirror(axis) => {
            core.mirror_document(*axis)?;
        }
        BatchOperationKind::Rotate90(direction) => {
            core.rotate_document(*direction)?;
        }
        BatchOperationKind::Resize(resize) => {
            core.resize_document(*resize)?;
        }
        BatchOperationKind::ConvertPlane {
            destination_kind,
            destination_format,
        } => {
            core.convert_plane(target_plane()?, *destination_kind, *destination_format)?;
        }
    }
    Ok(OperationResult::Applied)
}

fn apply_color_replacement(
    core: &mut Core,
    plane_id: u64,
    pairs: &[BatchColorPair],
    progress: &mut impl FnMut(u64, u64) -> bool,
) -> Result<(), CoreError> {
    let before = core.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
    let source = before
        .plane_by_id(plane_id)
        .ok_or(CoreError::InvalidArgument(
            "batch plane target does not exist",
        ))?;
    let work = u64::from(source.raster.width())
        .checked_mul(u64::from(source.raster.height()))
        .ok_or(CoreError::InvalidArgument("batch raster work overflows"))?;
    if work > MAX_IMAGE_EDIT_PIXELS {
        return Err(CoreError::InvalidArgument(
            "batch raster exceeds the bounded work limit",
        ));
    }
    let revision = core.next_document_revision()?;
    let mut after = before.clone();
    let raster = &mut after
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("batch plane target disappeared"))?
        .raster;
    let mut touched = BTreeSet::new();
    for y in 0..raster.height() {
        if !progress(u64::from(y), u64::from(raster.height()).max(1)) {
            return Err(CoreError::Cancelled);
        }
        for x in 0..raster.width() {
            let value = raster.pixel(x, y)?;
            if let Some(replacement) = pairs
                .iter()
                .find(|pair| pair.enabled && pair.old == value)
                .map(|pair| pair.new)
            {
                ensure_pixel_matches_format(replacement, raster.format())?;
                raster.set_pixel(x, y, replacement, revision)?;
                touched.insert(TileCoord {
                    x: x / TILE_SIZE,
                    y: y / TILE_SIZE,
                });
            }
        }
    }
    for coord in touched {
        raster.remove_tile_if_empty(coord);
    }
    core.commit_document_edit_with_revision(before, after, revision)?;
    Ok(())
}

fn apply_separation(
    core: &mut Core,
    plane_id: u64,
    options: &BatchSeparation,
    progress: &mut impl FnMut(u64, u64) -> bool,
) -> Result<(), CoreError> {
    let before = core.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
    let source = before
        .plane_by_id(plane_id)
        .ok_or(CoreError::InvalidArgument(
            "batch plane target does not exist",
        ))?;
    ensure_pixel_matches_format(options.replacement, source.raster.format())?;
    let empty = empty_pixel(source.raster.format());
    let revision = core.next_document_revision()?;
    let mut after = before.clone();
    let raster = &mut after
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("batch plane target disappeared"))?
        .raster;
    for y in 0..raster.height() {
        if !progress(u64::from(y), u64::from(raster.height()).max(1)) {
            return Err(CoreError::Cancelled);
        }
        for x in 0..raster.width() {
            let value = raster.pixel(x, y)?;
            let selected = options.colors.contains(&value) ^ options.invert;
            raster.set_pixel(
                x,
                y,
                if selected { options.replacement } else { empty },
                revision,
            )?;
        }
    }
    core.commit_document_edit_with_revision(before, after, revision)?;
    Ok(())
}

fn resolve_target(
    core: &Core,
    selector: &BatchTargetSelector,
) -> Result<Option<(u64, Option<u64>)>, CoreError> {
    let layers = core.layers()?;
    let layer = layers.iter().find(|layer| {
        selector.layer_id.is_none_or(|id| layer.id == id)
            && selector.layer_kind.is_none_or(|kind| layer.kind == kind)
    });
    let Some(layer) = layer else {
        return missing_target(selector.missing_policy);
    };
    let plane = if selector.plane_id.is_none() && selector.plane_kind.is_none() {
        None
    } else {
        layer.planes.iter().find(|plane| {
            selector.plane_id.is_none_or(|id| plane.id == id)
                && selector.plane_kind.is_none_or(|kind| plane.kind == kind)
        })
    };
    if (selector.plane_id.is_some() || selector.plane_kind.is_some()) && plane.is_none() {
        return missing_target(selector.missing_policy);
    }
    Ok(Some((layer.id, plane.map(|plane| plane.id))))
}

fn missing_target(
    policy: BatchMissingTargetPolicy,
) -> Result<Option<(u64, Option<u64>)>, CoreError> {
    match policy {
        BatchMissingTargetPolicy::Skip => Ok(None),
        BatchMissingTargetPolicy::Error => Err(CoreError::InvalidArgument(
            "batch stable target does not exist in this cell",
        )),
    }
}

fn working_core(source: &BatchSource) -> Result<Core, CoreError> {
    match &source.content {
        BatchSourceContent::Path(path) => {
            let mut core = Core::new();
            core.open(path)?;
            Ok(core)
        }
        BatchSourceContent::Document(document) => Ok(core_from_document(document.as_ref().clone())),
        BatchSourceContent::Sequence(cell) => {
            let mut core = Core::new();
            core.new_cell_with_uuid(
                cell.raster.width(),
                cell.raster.height(),
                cell.dpi_x_milli,
                cell.dpi_y_milli,
                cell.document_uuid,
            )?;
            let revision = core.next_document_revision()?;
            let document = core.document.as_mut().ok_or(CoreError::NoDocument)?;
            document.frames = cell.frames;
            document
                .raster_mut(ActivePlane::Color)
                .clone_from(&cell.raster);
            core.document_revision = revision;
            core.reset_history(true);
            Ok(core)
        }
    }
}

fn core_from_document(document: CellDocument) -> Core {
    let mut core = Core::new();
    core.next_id = document.max_stable_id().saturating_add(1).max(1);
    core.document_revision = 1;
    core.document = Some(document);
    core.reset_history(true);
    core
}

fn output_path_for(
    graph: &BatchGraph,
    source: &BatchSource,
    index: usize,
) -> Result<PathBuf, CoreError> {
    if graph.output.policy == BatchOutputPolicy::ExplicitOverwrite {
        return source.input_path.clone().ok_or(CoreError::InvalidArgument(
            "explicit overwrite requires a file-backed input",
        ));
    }
    let source_stem = Path::new(&source.label)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("cell");
    let number = if graph.output.descending {
        graph.output.start_number.saturating_sub(index as u32)
    } else {
        graph.output.start_number.saturating_add(index as u32)
    };
    let base_folder = if graph.output.folder.is_empty() {
        source
            .input_path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf()
    } else {
        PathBuf::from(&graph.output.folder)
    };
    if base_folder.as_os_str().is_empty() {
        return Err(CoreError::InvalidArgument(
            "batch output folder is required for an in-memory input",
        ));
    }
    let folder = if graph.output.cell_folder {
        base_folder.join(source_stem)
    } else {
        base_folder
    };
    let file_name = match graph.output.policy {
        BatchOutputPolicy::Duplicate if graph.output.basename.is_empty() => {
            format!("{source_stem}_batch.inkpod")
        }
        BatchOutputPolicy::Duplicate | BatchOutputPolicy::NewSave => {
            let basename = if graph.output.basename.is_empty() {
                "cell"
            } else {
                &graph.output.basename
            };
            format!("{basename}_{number:04}.inkpod")
        }
        BatchOutputPolicy::ExplicitOverwrite => unreachable!(),
    };
    Ok(folder.join(file_name))
}

fn save_batch_output(
    working: &Core,
    graph: &BatchGraph,
    source: &BatchSource,
    path: &Path,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(), CoreError> {
    if graph.output.policy != BatchOutputPolicy::ExplicitOverwrite {
        if source.input_path.as_deref() == Some(path) {
            return Err(CoreError::InvalidState(
                "non-overwrite batch policy resolved to the input path",
            ));
        }
        if path.exists() {
            return Err(CoreError::InvalidState(
                "non-overwrite batch output already exists",
            ));
        }
    }
    if is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|error| CoreError::Format(error.to_string()))?;
    }
    let document = working.document.as_ref().ok_or(CoreError::NoDocument)?;
    inkpod_format::save_atomic_with_cancel(path, &document.to_file(), &mut is_cancelled)?;
    Ok(())
}

fn cancelled_item(source: &BatchSource, output_path: Option<PathBuf>) -> BatchItemResult {
    BatchItemResult {
        input_name: source.label.clone(),
        output_path,
        outcome: BatchItemOutcome::Cancelled,
        message: "cancelled before atomic commit".to_owned(),
    }
}

fn within_range(name: &str, input: &BatchInputSelector) -> bool {
    parse_cell_number(name).is_none_or(|number| within_cell_range(number, input))
}

fn within_cell_range(number: u32, input: &BatchInputSelector) -> bool {
    (input.first_cell == 0 || number >= input.first_cell)
        && (input.last_cell == 0 || number <= input.last_cell)
}

fn path_label(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cell.inkpod")
}

fn validate_component(value: &str, required: bool) -> Result<(), CoreError> {
    if (required && value.is_empty())
        || value.len() > MAX_BATCH_NAME_BYTES
        || value.as_bytes().contains(&0)
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(CoreError::InvalidArgument(
            "batch name or basename is invalid",
        ));
    }
    Ok(())
}

fn validate_path(value: &str) -> Result<(), CoreError> {
    if value.len() > MAX_BATCH_PATH_BYTES || value.as_bytes().contains(&0) {
        return Err(CoreError::InvalidArgument("batch path is invalid"));
    }
    Ok(())
}

fn validate_operation(operation: &BatchOperation) -> Result<(), CoreError> {
    if operation.version != BATCH_OPERATION_VERSION {
        return Err(CoreError::InvalidArgument(
            "batch operation version is unsupported",
        ));
    }
    let requires_target = !matches!(
        operation.kind,
        BatchOperationKind::Mirror(_)
            | BatchOperationKind::Rotate90(_)
            | BatchOperationKind::Resize(_)
    );
    if requires_target && operation.target.is_none() {
        return Err(CoreError::InvalidArgument(
            "batch operation target selector is empty",
        ));
    }
    if let Some(target) = operation.target {
        if target.layer_id.is_none() && target.layer_kind.is_none() {
            return Err(CoreError::InvalidArgument(
                "batch target layer selector is empty",
            ));
        }
        let requires_plane = !matches!(operation.kind, BatchOperationKind::Visibility { .. });
        if requires_plane && target.plane_id.is_none() && target.plane_kind.is_none() {
            return Err(CoreError::InvalidArgument(
                "batch target plane selector is empty",
            ));
        }
    }
    match &operation.kind {
        BatchOperationKind::ColorReplace(pairs) => {
            if pairs.is_empty() || pairs.len() > MAX_BATCH_COLOR_PAIRS {
                return Err(CoreError::InvalidArgument(
                    "batch color-pair count is outside bounds",
                ));
            }
        }
        BatchOperationKind::ContinuousFill(seeds) => {
            if seeds.is_empty() || seeds.len() > MAX_BATCH_SEEDS {
                return Err(CoreError::InvalidArgument(
                    "batch fill-seed count is outside bounds",
                ));
            }
        }
        BatchOperationKind::Separation(options) => {
            if options.colors.is_empty() || options.colors.len() > MAX_BATCH_COLORS {
                return Err(CoreError::InvalidArgument(
                    "batch separation color count is outside bounds",
                ));
            }
        }
        BatchOperationKind::BoundaryAirbrush(effect)
            if effect.colors.len() < 2 || effect.colors.len() > MAX_BATCH_COLORS =>
        {
            return Err(CoreError::InvalidArgument(
                "batch boundary-airbrush color count is outside bounds",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn ensure_pixel_matches_format(value: PixelValue, format: PixelFormat) -> Result<(), CoreError> {
    if matches!(
        (value, format),
        (PixelValue::Binary(_), PixelFormat::BinaryMask8)
            | (PixelValue::Grayscale8(_), PixelFormat::Grayscale8)
            | (PixelValue::Grayscale16(_), PixelFormat::Grayscale16)
            | (PixelValue::Rgba(_), PixelFormat::StraightRgba8)
            | (PixelValue::Rgba16(_), PixelFormat::StraightRgba16)
    ) {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "batch color depth does not match the target plane",
        ))
    }
}

const fn empty_pixel(format: PixelFormat) -> PixelValue {
    match format {
        PixelFormat::BinaryMask8 => PixelValue::Binary(0),
        PixelFormat::Grayscale8 => PixelValue::Grayscale8(0),
        PixelFormat::Grayscale16 => PixelValue::Grayscale16(0),
        PixelFormat::StraightRgba8 => PixelValue::Rgba([0; 4]),
        PixelFormat::StraightRgba16 => PixelValue::Rgba16([0; 4]),
        PixelFormat::PremultipliedBgra8 => PixelValue::Rgba([0; 4]),
    }
}

fn input_kind_code(kind: BatchInputKind) -> u32 {
    match kind {
        BatchInputKind::File => INPUT_FILE,
        BatchInputKind::Folder => INPUT_FOLDER,
        BatchInputKind::CurrentSequence => INPUT_CURRENT_SEQUENCE,
    }
}

fn parse_input_kind(value: u32) -> Result<BatchInputKind, CoreError> {
    match value {
        INPUT_FILE => Ok(BatchInputKind::File),
        INPUT_FOLDER => Ok(BatchInputKind::Folder),
        INPUT_CURRENT_SEQUENCE => Ok(BatchInputKind::CurrentSequence),
        _ => Err(CoreError::InvalidArgument("batch input kind is unknown")),
    }
}

fn output_policy_code(policy: BatchOutputPolicy) -> u32 {
    match policy {
        BatchOutputPolicy::Duplicate => OUTPUT_DUPLICATE,
        BatchOutputPolicy::NewSave => OUTPUT_NEW_SAVE,
        BatchOutputPolicy::ExplicitOverwrite => OUTPUT_OVERWRITE,
    }
}

fn parse_output_policy(value: u32) -> Result<BatchOutputPolicy, CoreError> {
    match value {
        OUTPUT_DUPLICATE => Ok(BatchOutputPolicy::Duplicate),
        OUTPUT_NEW_SAVE => Ok(BatchOutputPolicy::NewSave),
        OUTPUT_OVERWRITE => Ok(BatchOutputPolicy::ExplicitOverwrite),
        _ => Err(CoreError::InvalidArgument("batch output policy is unknown")),
    }
}

fn failure_policy_code(policy: BatchFailurePolicy) -> u32 {
    match policy {
        BatchFailurePolicy::Continue => FAILURE_CONTINUE,
        BatchFailurePolicy::Stop => FAILURE_STOP,
    }
}

fn parse_failure_policy(value: u32) -> Result<BatchFailurePolicy, CoreError> {
    match value {
        FAILURE_CONTINUE => Ok(BatchFailurePolicy::Continue),
        FAILURE_STOP => Ok(BatchFailurePolicy::Stop),
        _ => Err(CoreError::InvalidArgument(
            "batch failure policy is unknown",
        )),
    }
}

fn operation_to_file(operation: &BatchOperation) -> Result<FileBatchOperation, CoreError> {
    validate_operation(operation)?;
    let (kind, payload) = encode_operation_kind(&operation.kind)?;
    let target = operation
        .target
        .map_or(FileBatchTarget::default(), |target| FileBatchTarget {
            layer_id: target.layer_id.unwrap_or(0),
            plane_id: target.plane_id.unwrap_or(0),
            layer_kind: target.layer_kind.map_or(0, layer_kind_code),
            plane_kind: target.plane_kind.map_or(0, plane_kind_code),
            missing_policy: match target.missing_policy {
                BatchMissingTargetPolicy::Skip => MISSING_SKIP,
                BatchMissingTargetPolicy::Error => MISSING_ERROR,
            },
        });
    Ok(FileBatchOperation {
        version: operation.version,
        kind,
        flags: (if operation.enabled { OP_ENABLED } else { 0 })
            | (if operation.configure_each_run {
                OP_CONFIGURE_EACH_RUN
            } else {
                0
            }),
        target,
        payload,
    })
}

fn operation_from_file(file: FileBatchOperation) -> Result<BatchOperation, CoreError> {
    if file.flags & !(OP_ENABLED | OP_CONFIGURE_EACH_RUN) != 0 {
        return Err(CoreError::InvalidArgument(
            "batch operation flags are invalid",
        ));
    }
    let target = if file.target == FileBatchTarget::default() {
        None
    } else {
        Some(BatchTargetSelector {
            layer_id: (file.target.layer_id != 0).then_some(file.target.layer_id),
            plane_id: (file.target.plane_id != 0).then_some(file.target.plane_id),
            layer_kind: (file.target.layer_kind != 0)
                .then(|| parse_layer_kind(file.target.layer_kind))
                .transpose()?,
            plane_kind: (file.target.plane_kind != 0)
                .then(|| parse_plane_kind(file.target.plane_kind))
                .transpose()?,
            missing_policy: match file.target.missing_policy {
                MISSING_SKIP => BatchMissingTargetPolicy::Skip,
                MISSING_ERROR => BatchMissingTargetPolicy::Error,
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "batch missing-target policy is unknown",
                    ));
                }
            },
        })
    };
    let operation = BatchOperation {
        version: file.version,
        enabled: file.flags & OP_ENABLED != 0,
        configure_each_run: file.flags & OP_CONFIGURE_EACH_RUN != 0,
        target,
        kind: decode_operation_kind(file.kind, &file.payload)?,
    };
    validate_operation(&operation)?;
    Ok(operation)
}

fn encode_operation_kind(kind: &BatchOperationKind) -> Result<(u32, Vec<u8>), CoreError> {
    let mut output = PayloadWriter::default();
    let code = match kind {
        BatchOperationKind::ColorReplace(pairs) => {
            output.u32(pairs.len() as u32);
            for pair in pairs {
                output.u32(u32::from(pair.enabled));
                output.pixel(pair.old);
                output.pixel(pair.new);
            }
            OP_COLOR_REPLACE
        }
        BatchOperationKind::ContinuousFill(seeds) => {
            output.u32(seeds.len() as u32);
            for seed in seeds {
                output.u32(seed.x);
                output.u32(seed.y);
                output.pixel(seed.color);
                output.u32(u32::from(seed.tolerance));
                output.u32(u32::from(seed.gap_close));
                output.u32(u32::from(seed.expected_source.is_some()));
                output.pixel(seed.expected_source.unwrap_or(PixelValue::Rgba([0; 4])));
            }
            OP_CONTINUOUS_FILL
        }
        BatchOperationKind::Separation(options) => {
            output.u32(options.colors.len() as u32);
            for color in &options.colors {
                output.pixel(*color);
            }
            output.pixel(options.replacement);
            output.u32(u32::from(options.invert));
            OP_SEPARATION
        }
        BatchOperationKind::Visibility { visible } => {
            output.u32(u32::from(*visible));
            OP_VISIBILITY
        }
        BatchOperationKind::LineWidth(mode) => {
            let (mode, value) = match mode {
                VectorWidthMode::Add(value) => (1, *value),
                VectorWidthMode::Subtract(value) => (2, *value),
                VectorWidthMode::Scale(value) => (3, *value),
                VectorWidthMode::Constant(value) => (4, *value),
            };
            output.u32(mode);
            output.u32(value.to_bits());
            OP_LINE_WIDTH
        }
        BatchOperationKind::Filter(filter) => {
            encode_filter(&mut output, filter)?;
            OP_FILTER
        }
        BatchOperationKind::BoundaryAirbrush(effect) => {
            output.u32(effect.colors.len() as u32);
            for color in &effect.colors {
                for component in color {
                    output.u32(u32::from(*component));
                }
            }
            output.u32(effect.width);
            output.u32(effect.strength_milli);
            OP_BOUNDARY_AIRBRUSH
        }
        BatchOperationKind::DustRemoval(options) => {
            output.u32(match options.mode {
                super::DustMode::RemoveForeground => 1,
                super::DustMode::FillTransparentHoles => 2,
                super::DustMode::ReplaceColorOutliers => 3,
            });
            output.u32(options.maximum_pixels);
            OP_DUST_REMOVAL
        }
        BatchOperationKind::Mirror(axis) => {
            output.u32(match axis {
                MirrorAxis::Horizontal => 1,
                MirrorAxis::Vertical => 2,
            });
            OP_MIRROR
        }
        BatchOperationKind::Rotate90(direction) => {
            output.u32(match direction {
                RotateDirection::Left90 => 1,
                RotateDirection::Right90 => 2,
            });
            OP_ROTATE_90
        }
        BatchOperationKind::Resize(resize) => {
            output.u32(resize.width);
            output.u32(resize.height);
            output.u32(resize.dpi_x_milli);
            output.u32(resize.dpi_y_milli);
            output.u32(u32::from(resize.resample));
            output.u32(resize_anchor_code(resize.anchor));
            OP_RESIZE
        }
        BatchOperationKind::ConvertPlane {
            destination_kind,
            destination_format,
        } => {
            output.u32(plane_kind_code(*destination_kind));
            output.u32(pixel_format_code(*destination_format));
            OP_CONVERT_PLANE
        }
    };
    Ok((code, output.bytes))
}

fn decode_operation_kind(code: u32, payload: &[u8]) -> Result<BatchOperationKind, CoreError> {
    let mut input = PayloadReader::new(payload);
    let kind = match code {
        OP_COLOR_REPLACE => {
            let count = input.count(MAX_BATCH_COLOR_PAIRS)?;
            let mut pairs = Vec::with_capacity(count);
            for _ in 0..count {
                pairs.push(BatchColorPair {
                    enabled: input.boolean()?,
                    old: input.pixel()?,
                    new: input.pixel()?,
                });
            }
            BatchOperationKind::ColorReplace(pairs)
        }
        OP_CONTINUOUS_FILL => {
            let count = input.count(MAX_BATCH_SEEDS)?;
            let mut seeds = Vec::with_capacity(count);
            for _ in 0..count {
                let x = input.u32()?;
                let y = input.u32()?;
                let color = input.pixel()?;
                let tolerance = u16::try_from(input.u32()?)
                    .map_err(|_| CoreError::InvalidArgument("batch fill tolerance is invalid"))?;
                let gap_close = u8::try_from(input.u32()?)
                    .map_err(|_| CoreError::InvalidArgument("batch gap-close value is invalid"))?;
                let has_expected = input.boolean()?;
                let expected = input.pixel()?;
                seeds.push(BatchSeed {
                    x,
                    y,
                    color,
                    tolerance,
                    gap_close,
                    expected_source: has_expected.then_some(expected),
                });
            }
            BatchOperationKind::ContinuousFill(seeds)
        }
        OP_SEPARATION => {
            let count = input.count(MAX_BATCH_COLORS)?;
            let mut colors = Vec::with_capacity(count);
            for _ in 0..count {
                colors.push(input.pixel()?);
            }
            BatchOperationKind::Separation(BatchSeparation {
                colors,
                replacement: input.pixel()?,
                invert: input.boolean()?,
            })
        }
        OP_VISIBILITY => BatchOperationKind::Visibility {
            visible: input.boolean()?,
        },
        OP_LINE_WIDTH => {
            let mode = input.u32()?;
            let value = f32::from_bits(input.u32()?);
            BatchOperationKind::LineWidth(match mode {
                1 => VectorWidthMode::Add(value),
                2 => VectorWidthMode::Subtract(value),
                3 => VectorWidthMode::Scale(value),
                4 => VectorWidthMode::Constant(value),
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "batch line-width mode is unknown",
                    ));
                }
            })
        }
        OP_FILTER => BatchOperationKind::Filter(decode_filter(&mut input)?),
        OP_BOUNDARY_AIRBRUSH => {
            let count = input.count(MAX_BATCH_COLORS)?;
            let mut colors = Vec::with_capacity(count);
            for _ in 0..count {
                let mut color = [0_u16; 4];
                for component in &mut color {
                    *component = u16::try_from(input.u32()?).map_err(|_| {
                        CoreError::InvalidArgument("batch boundary color is invalid")
                    })?;
                }
                colors.push(color);
            }
            BatchOperationKind::BoundaryAirbrush(BoundaryAirbrush {
                colors,
                width: input.u32()?,
                strength_milli: input.u32()?,
            })
        }
        OP_DUST_REMOVAL => BatchOperationKind::DustRemoval(DustRemoval {
            mode: match input.u32()? {
                1 => super::DustMode::RemoveForeground,
                2 => super::DustMode::FillTransparentHoles,
                3 => super::DustMode::ReplaceColorOutliers,
                _ => return Err(CoreError::InvalidArgument("batch dust mode is unknown")),
            },
            maximum_pixels: input.u32()?,
        }),
        OP_MIRROR => BatchOperationKind::Mirror(match input.u32()? {
            1 => MirrorAxis::Horizontal,
            2 => MirrorAxis::Vertical,
            _ => return Err(CoreError::InvalidArgument("batch mirror axis is unknown")),
        }),
        OP_ROTATE_90 => BatchOperationKind::Rotate90(match input.u32()? {
            1 => RotateDirection::Left90,
            2 => RotateDirection::Right90,
            _ => {
                return Err(CoreError::InvalidArgument(
                    "batch rotation direction is unknown",
                ));
            }
        }),
        OP_RESIZE => BatchOperationKind::Resize(DocumentResize {
            width: input.u32()?,
            height: input.u32()?,
            dpi_x_milli: input.u32()?,
            dpi_y_milli: input.u32()?,
            resample: input.boolean()?,
            anchor: parse_resize_anchor(input.u32()?)?,
        }),
        OP_CONVERT_PLANE => BatchOperationKind::ConvertPlane {
            destination_kind: parse_plane_kind(input.u32()?)?,
            destination_format: parse_pixel_format(input.u32()?)?,
        },
        _ => {
            return Err(CoreError::InvalidArgument(
                "batch operation kind is unknown",
            ));
        }
    };
    input.finish()?;
    Ok(kind)
}

fn encode_filter(output: &mut PayloadWriter, filter: &Filter) -> Result<(), CoreError> {
    match filter {
        Filter::SharpenWeak => output.u32(1),
        Filter::SharpenStrong => output.u32(2),
        Filter::BlurWeak => output.u32(3),
        Filter::BlurStrong => output.u32(4),
        Filter::GaussianBlur {
            radius,
            strength_milli,
        } => {
            output.u32(5);
            output.u32(*radius);
            output.u32(*strength_milli);
        }
        Filter::UnsharpMask {
            radius,
            amount_milli,
            threshold,
        } => {
            output.u32(6);
            output.u32(*radius);
            output.u32(*amount_milli);
            output.u32(u32::from(*threshold));
        }
        Filter::Invert { channel } => {
            output.u32(7);
            output.u32(channel_code(*channel));
        }
        Filter::AutoContrast => output.u32(8),
        Filter::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        } => {
            output.u32(9);
            output.i32(*brightness_milli);
            output.i32(*contrast_milli);
        }
        Filter::ToneCurve {
            channel,
            interpolation,
            points,
        } => {
            output.u32(10);
            output.u32(channel_code(*channel));
            output.u32(match interpolation {
                super::CurveInterpolation::Bezier => 1,
                super::CurveInterpolation::BSpline => 2,
            });
            output.u32(points.len() as u32);
            for point in points {
                output.u32(u32::from(point.input));
                output.u32(u32::from(point.output));
            }
        }
        Filter::Levels(levels) => {
            output.u32(11);
            output.u32(channel_code(levels.channel));
            output.u32(u32::from(levels.input_shadow));
            output.u32(levels.input_gamma_milli);
            output.u32(u32::from(levels.input_highlight));
            output.u32(u32::from(levels.output_shadow));
            output.u32(u32::from(levels.output_highlight));
        }
        Filter::Hsv(hsv) => {
            output.u32(12);
            output.i32(hsv.hue_degrees_milli);
            output.i32(hsv.saturation_milli);
            output.i32(hsv.value_milli);
        }
        Filter::ColorBalance(balance) => {
            output.u32(13);
            output.i32(balance.red_milli);
            output.i32(balance.green_milli);
            output.i32(balance.blue_milli);
        }
    }
    Ok(())
}

fn decode_filter(input: &mut PayloadReader<'_>) -> Result<Filter, CoreError> {
    Ok(match input.u32()? {
        1 => Filter::SharpenWeak,
        2 => Filter::SharpenStrong,
        3 => Filter::BlurWeak,
        4 => Filter::BlurStrong,
        5 => Filter::GaussianBlur {
            radius: input.u32()?,
            strength_milli: input.u32()?,
        },
        6 => Filter::UnsharpMask {
            radius: input.u32()?,
            amount_milli: input.u32()?,
            threshold: u16::try_from(input.u32()?)
                .map_err(|_| CoreError::InvalidArgument("batch filter threshold is invalid"))?,
        },
        7 => Filter::Invert {
            channel: parse_channel(input.u32()?)?,
        },
        8 => Filter::AutoContrast,
        9 => Filter::BrightnessContrast {
            brightness_milli: input.i32()?,
            contrast_milli: input.i32()?,
        },
        10 => {
            let channel = parse_channel(input.u32()?)?;
            let interpolation = match input.u32()? {
                1 => super::CurveInterpolation::Bezier,
                2 => super::CurveInterpolation::BSpline,
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "batch curve interpolation is unknown",
                    ));
                }
            };
            let count = input.count(super::MAX_CURVE_POINTS)?;
            let mut points = Vec::with_capacity(count);
            for _ in 0..count {
                points.push(super::CurvePoint {
                    input: u16::try_from(input.u32()?)
                        .map_err(|_| CoreError::InvalidArgument("batch curve input is invalid"))?,
                    output: u16::try_from(input.u32()?)
                        .map_err(|_| CoreError::InvalidArgument("batch curve output is invalid"))?,
                });
            }
            Filter::ToneCurve {
                channel,
                interpolation,
                points,
            }
        }
        11 => Filter::Levels(super::Levels {
            channel: parse_channel(input.u32()?)?,
            input_shadow: input.u16()?,
            input_gamma_milli: input.u32()?,
            input_highlight: input.u16()?,
            output_shadow: input.u16()?,
            output_highlight: input.u16()?,
        }),
        12 => Filter::Hsv(super::HsvAdjustment {
            hue_degrees_milli: input.i32()?,
            saturation_milli: input.i32()?,
            value_milli: input.i32()?,
        }),
        13 => Filter::ColorBalance(super::ColorBalance {
            red_milli: input.i32()?,
            green_milli: input.i32()?,
            blue_milli: input.i32()?,
        }),
        _ => return Err(CoreError::InvalidArgument("batch filter kind is unknown")),
    })
}

fn channel_code(channel: super::Channel) -> u32 {
    match channel {
        super::Channel::Rgb => 1,
        super::Channel::Red => 2,
        super::Channel::Green => 3,
        super::Channel::Blue => 4,
    }
}

fn parse_channel(value: u32) -> Result<super::Channel, CoreError> {
    match value {
        1 => Ok(super::Channel::Rgb),
        2 => Ok(super::Channel::Red),
        3 => Ok(super::Channel::Green),
        4 => Ok(super::Channel::Blue),
        _ => Err(CoreError::InvalidArgument(
            "batch filter channel is unknown",
        )),
    }
}

fn layer_kind_code(kind: LayerKind) -> u32 {
    match kind {
        LayerKind::BinaryColoring => 1,
        LayerKind::GrayscaleColoring => 2,
        LayerKind::Raster => 3,
        LayerKind::Selection => 4,
        LayerKind::Frame => 5,
        LayerKind::VanishingPoint => 6,
        LayerKind::Adjustment => 7,
        LayerKind::Text => 8,
        LayerKind::Annotation => 9,
        LayerKind::VectorColoring => 10,
    }
}

fn parse_layer_kind(value: u32) -> Result<LayerKind, CoreError> {
    match value {
        1 => Ok(LayerKind::BinaryColoring),
        2 => Ok(LayerKind::GrayscaleColoring),
        3 => Ok(LayerKind::Raster),
        4 => Ok(LayerKind::Selection),
        5 => Ok(LayerKind::Frame),
        6 => Ok(LayerKind::VanishingPoint),
        7 => Ok(LayerKind::Adjustment),
        8 => Ok(LayerKind::Text),
        9 => Ok(LayerKind::Annotation),
        10 => Ok(LayerKind::VectorColoring),
        _ => Err(CoreError::InvalidArgument("batch layer kind is unknown")),
    }
}

fn plane_kind_code(kind: PlaneType) -> u32 {
    match kind {
        PlaneType::MainLine => 1,
        PlaneType::Color => 2,
        PlaneType::Raster => 3,
        PlaneType::Selection => 4,
        PlaneType::VectorMainLine => 5,
        PlaneType::ColorTrace => 6,
        PlaneType::VectorFill => 7,
    }
}

fn parse_plane_kind(value: u32) -> Result<PlaneType, CoreError> {
    match value {
        1 => Ok(PlaneType::MainLine),
        2 => Ok(PlaneType::Color),
        3 => Ok(PlaneType::Raster),
        4 => Ok(PlaneType::Selection),
        5 => Ok(PlaneType::VectorMainLine),
        6 => Ok(PlaneType::ColorTrace),
        7 => Ok(PlaneType::VectorFill),
        _ => Err(CoreError::InvalidArgument("batch plane kind is unknown")),
    }
}

fn pixel_format_code(format: PixelFormat) -> u32 {
    match format {
        PixelFormat::BinaryMask8 => 1,
        PixelFormat::Grayscale8 => 2,
        PixelFormat::Grayscale16 => 3,
        PixelFormat::StraightRgba8 => 4,
        PixelFormat::StraightRgba16 => 5,
        PixelFormat::PremultipliedBgra8 => 6,
    }
}

fn parse_pixel_format(value: u32) -> Result<PixelFormat, CoreError> {
    match value {
        1 => Ok(PixelFormat::BinaryMask8),
        2 => Ok(PixelFormat::Grayscale8),
        3 => Ok(PixelFormat::Grayscale16),
        4 => Ok(PixelFormat::StraightRgba8),
        5 => Ok(PixelFormat::StraightRgba16),
        6 => Ok(PixelFormat::PremultipliedBgra8),
        _ => Err(CoreError::InvalidArgument(
            "batch destination pixel format is unknown",
        )),
    }
}

fn resize_anchor_code(anchor: ResizeAnchor) -> u32 {
    match anchor {
        ResizeAnchor::TopLeft => 1,
        ResizeAnchor::TopRight => 2,
        ResizeAnchor::Center => 3,
        ResizeAnchor::BottomLeft => 4,
        ResizeAnchor::BottomRight => 5,
    }
}

fn parse_resize_anchor(value: u32) -> Result<ResizeAnchor, CoreError> {
    match value {
        1 => Ok(ResizeAnchor::TopLeft),
        2 => Ok(ResizeAnchor::TopRight),
        3 => Ok(ResizeAnchor::Center),
        4 => Ok(ResizeAnchor::BottomLeft),
        5 => Ok(ResizeAnchor::BottomRight),
        _ => Err(CoreError::InvalidArgument("batch resize anchor is unknown")),
    }
}

#[derive(Default)]
struct PayloadWriter {
    bytes: Vec<u8>,
}

impl PayloadWriter {
    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn pixel(&mut self, value: PixelValue) {
        match value {
            PixelValue::Binary(value) => {
                self.u32(1);
                self.u32(u32::from(value));
            }
            PixelValue::Grayscale8(value) => {
                self.u32(2);
                self.u32(u32::from(value));
            }
            PixelValue::Grayscale16(value) => {
                self.u32(3);
                self.u32(u32::from(value));
            }
            PixelValue::Rgba(value) => {
                self.u32(4);
                for component in value {
                    self.u32(u32::from(component));
                }
            }
            PixelValue::Rgba16(value) => {
                self.u32(5);
                for component in value {
                    self.u32(u32::from(component));
                }
            }
        }
    }
}

struct PayloadReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> PayloadReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn u32(&mut self) -> Result<u32, CoreError> {
        let end = self
            .cursor
            .checked_add(4)
            .ok_or(CoreError::InvalidArgument(
                "batch operation payload offset overflows",
            ))?;
        let bytes: [u8; 4] = self
            .bytes
            .get(self.cursor..end)
            .ok_or(CoreError::InvalidArgument(
                "batch operation payload is truncated",
            ))?
            .try_into()
            .map_err(|_| CoreError::InvalidArgument("batch u32 payload is truncated"))?;
        self.cursor = end;
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32, CoreError> {
        Ok(i32::from_le_bytes(self.u32()?.to_le_bytes()))
    }

    fn u16(&mut self) -> Result<u16, CoreError> {
        u16::try_from(self.u32()?)
            .map_err(|_| CoreError::InvalidArgument("batch u16 payload is invalid"))
    }

    fn boolean(&mut self) -> Result<bool, CoreError> {
        match self.u32()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CoreError::InvalidArgument(
                "batch boolean payload is invalid",
            )),
        }
    }

    fn count(&mut self, maximum: usize) -> Result<usize, CoreError> {
        let count = self.u32()? as usize;
        if count > maximum {
            return Err(CoreError::InvalidArgument(
                "batch payload count exceeds the bounded limit",
            ));
        }
        Ok(count)
    }

    fn pixel(&mut self) -> Result<PixelValue, CoreError> {
        match self.u32()? {
            1 => Ok(PixelValue::Binary(u8::try_from(self.u32()?).map_err(
                |_| CoreError::InvalidArgument("batch binary color is invalid"),
            )?)),
            2 => Ok(PixelValue::Grayscale8(u8::try_from(self.u32()?).map_err(
                |_| CoreError::InvalidArgument("batch grayscale color is invalid"),
            )?)),
            3 => Ok(PixelValue::Grayscale16(self.u16()?)),
            4 => {
                let mut value = [0_u8; 4];
                for component in &mut value {
                    *component = u8::try_from(self.u32()?)
                        .map_err(|_| CoreError::InvalidArgument("batch RGBA8 color is invalid"))?;
                }
                Ok(PixelValue::Rgba(value))
            }
            5 => {
                let mut value = [0_u16; 4];
                for component in &mut value {
                    *component = self.u16()?;
                }
                Ok(PixelValue::Rgba16(value))
            }
            _ => Err(CoreError::InvalidArgument(
                "batch pixel payload kind is unknown",
            )),
        }
    }

    fn finish(&self) -> Result<(), CoreError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(CoreError::InvalidArgument(
                "batch operation payload has trailing bytes",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Channel, ColorBalance, CurveInterpolation, CurvePoint, DustMode, HsvAdjustment, Levels,
        PaintTool, Stroke, StrokeSample,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn temp_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "inkpod-m7-{label}-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn saved_cell(path: &Path, color: [u8; 4]) {
        let mut core = Core::new();
        core.new_cell(4, 4, 96_000, 96_000).unwrap();
        core.set_active_plane(ActivePlane::Color).unwrap();
        core.apply_stroke(&Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::Color,
            color,
            diameter: 1.0,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: crate::CoordinateSpace::Document,
            samples: vec![StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            }],
        })
        .unwrap();
        core.save(path).unwrap();
    }

    fn replace_graph(input: &Path, output: &Path) -> BatchGraph {
        BatchGraph {
            version: BATCH_GRAPH_VERSION,
            name: "replace-set".to_owned(),
            inputs: vec![BatchInputSelector::file(input.to_string_lossy())],
            operations: vec![BatchOperation {
                version: BATCH_OPERATION_VERSION,
                enabled: true,
                configure_each_run: false,
                target: Some(BatchTargetSelector::color_plane()),
                kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
                    enabled: true,
                    old: PixelValue::Rgba([10, 20, 30, 255]),
                    new: PixelValue::Rgba([30, 20, 10, 255]),
                }]),
            }],
            output: BatchOutputSettings {
                folder: output.to_string_lossy().into_owned(),
                ..BatchOutputSettings::default()
            },
        }
    }

    #[test]
    fn m7_acceptance_dry_run_writes_no_files() {
        let directory = temp_directory("dry-run");
        let input = directory.join("cell1.inkpod");
        let output = directory.join("new-output");
        saved_cell(&input, [10, 20, 30, 255]);
        let core = Core::new();
        let report = core
            .batch_execute(
                &replace_graph(&input, &output),
                BatchRunOptions {
                    scope: BatchRunScope::All,
                    dry_run: true,
                    preview_confirmed: true,
                },
                |_, _| true,
            )
            .unwrap();
        assert_eq!(report.items[0].outcome, BatchItemOutcome::DryRun);
        assert!(!output.exists());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn m7_acceptance_default_output_never_overwrites_input() {
        let directory = temp_directory("default-output");
        let input = directory.join("cell1.inkpod");
        saved_cell(&input, [10, 20, 30, 255]);
        let original = fs::read(&input).unwrap();
        let mut graph = replace_graph(&input, &directory);
        graph.output.folder.clear();
        let report = Core::new()
            .batch_execute(
                &graph,
                BatchRunOptions {
                    scope: BatchRunScope::All,
                    dry_run: false,
                    preview_confirmed: true,
                },
                |_, _| true,
            )
            .unwrap();
        let output = directory.join("cell1_batch.inkpod");
        assert_eq!(
            report.items[0].output_path.as_deref(),
            Some(output.as_path())
        );
        assert!(output.exists());
        assert_eq!(fs::read(&input).unwrap(), original);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn m7_acceptance_cancelled_file_leaves_no_temporary_output() {
        let directory = temp_directory("cancel");
        let input = directory.join("cell1.inkpod");
        let output = directory.join("output");
        saved_cell(&input, [10, 20, 30, 255]);
        let mut save_polls = 0_u32;
        let report = Core::new()
            .batch_execute(
                &replace_graph(&input, &output),
                BatchRunOptions {
                    scope: BatchRunScope::All,
                    dry_run: false,
                    preview_confirmed: true,
                },
                |completed, total| {
                    if completed + 1 == total {
                        save_polls += 1;
                        return save_polls < 2;
                    }
                    true
                },
            )
            .unwrap();
        assert!(report.cancelled);
        assert!(!output.join("cell1_batch.inkpod").exists());
        if output.exists() {
            assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
        }
        assert!(fs::read_dir(&directory).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".inkpod.tmp.")
        }));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn m7_acceptance_failure_policy_records_and_continues_or_stops() {
        let directory = temp_directory("failure-policy");
        let good = directory.join("cell2.inkpod");
        let bad = directory.join("cell1.inkpod");
        saved_cell(&good, [10, 20, 30, 255]);
        fs::write(&bad, b"not an inkpod document").unwrap();
        let output = directory.join("out");
        let mut graph = replace_graph(&good, &output);
        graph.inputs = vec![BatchInputSelector {
            kind: BatchInputKind::Folder,
            path: directory.to_string_lossy().into_owned(),
            first_cell: 0,
            last_cell: 0,
        }];
        let continued = Core::new()
            .batch_execute(
                &graph,
                BatchRunOptions {
                    scope: BatchRunScope::All,
                    dry_run: true,
                    preview_confirmed: true,
                },
                |_, _| true,
            )
            .unwrap();
        assert_eq!(continued.items.len(), 2);
        assert_eq!(continued.failure_count(), 1);
        assert_eq!(continued.items[0].outcome, BatchItemOutcome::Failed);
        assert_eq!(continued.items[1].outcome, BatchItemOutcome::DryRun);

        graph.output.failure_policy = BatchFailurePolicy::Stop;
        let stopped = Core::new()
            .batch_execute(
                &graph,
                BatchRunOptions {
                    scope: BatchRunScope::All,
                    dry_run: true,
                    preview_confirmed: true,
                },
                |_, _| true,
            )
            .unwrap();
        assert_eq!(stopped.items.len(), 1);
        assert_eq!(stopped.failure_count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn m7_acceptance_color_replacement_swap_round_trips() {
        let directory = temp_directory("replacement-roundtrip");
        let input = directory.join("cell1.inkpod");
        let settings = directory.join("replace.inkbatch");
        saved_cell(&input, [10, 20, 30, 255]);
        let mut graph = replace_graph(&input, &directory);
        graph.operations[0].swap_color_replacements().unwrap();
        graph.save(&settings).unwrap();
        let reopened = BatchGraph::load(&settings).unwrap();
        assert_eq!(reopened, graph);
        let BatchOperationKind::ColorReplace(pairs) = &reopened.operations[0].kind else {
            panic!("replacement operation disappeared");
        };
        assert_eq!(pairs[0].old, PixelValue::Rgba([30, 20, 10, 255]));
        assert_eq!(pairs[0].new, PixelValue::Rgba([10, 20, 30, 255]));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn m7_acceptance_continuous_fill_preview_warns_when_seed_moves_color() {
        let directory = temp_directory("seed-preview");
        let first = directory.join("cell1.inkpod");
        let second = directory.join("cell2.inkpod");
        saved_cell(&first, [10, 20, 30, 255]);
        saved_cell(&second, [50, 60, 70, 255]);
        let graph = BatchGraph {
            version: BATCH_GRAPH_VERSION,
            name: "fill-preview".to_owned(),
            inputs: vec![BatchInputSelector {
                kind: BatchInputKind::Folder,
                path: directory.to_string_lossy().into_owned(),
                first_cell: 0,
                last_cell: 0,
            }],
            operations: vec![BatchOperation {
                version: BATCH_OPERATION_VERSION,
                enabled: true,
                configure_each_run: false,
                target: Some(BatchTargetSelector::color_plane()),
                kind: BatchOperationKind::ContinuousFill(vec![BatchSeed {
                    x: 1,
                    y: 1,
                    color: PixelValue::Rgba([255, 0, 0, 255]),
                    tolerance: 0,
                    gap_close: 0,
                    expected_source: None,
                }]),
            }],
            output: BatchOutputSettings {
                folder: directory.join("out").to_string_lossy().into_owned(),
                ..BatchOutputSettings::default()
            },
        };
        let preview = Core::new()
            .batch_preview(&graph, BatchRunScope::All)
            .unwrap();
        assert_eq!(preview.items.len(), 2);
        assert!(preview.items[0].warnings.is_empty());
        assert!(
            preview.items[1]
                .warnings
                .iter()
                .any(|warning| warning.contains("moved to a different color"))
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn m7_review_rejects_empty_or_type_mismatched_target_selectors() {
        let operation = BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                layer_kind: None,
                plane_kind: None,
                missing_policy: BatchMissingTargetPolicy::Skip,
            }),
            kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([0; 4]),
                new: PixelValue::Rgba([1, 2, 3, 4]),
            }]),
        };
        assert!(matches!(
            validate_operation(&operation),
            Err(CoreError::InvalidArgument(
                "batch target layer selector is empty"
            ))
        ));

        let mut core = Core::new();
        core.new_cell(2, 2, 96_000, 96_000).unwrap();
        let layers = core.layers().unwrap();
        let coloring = layers
            .iter()
            .find(|layer| layer.kind == LayerKind::BinaryColoring)
            .unwrap();
        let color_plane = coloring
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Color)
            .unwrap();
        let selector = BatchTargetSelector {
            layer_id: Some(coloring.id),
            plane_id: Some(color_plane.id),
            layer_kind: Some(LayerKind::VectorColoring),
            plane_kind: Some(PlaneType::Color),
            missing_policy: BatchMissingTargetPolicy::Skip,
        };
        assert_eq!(resolve_target(&core, &selector).unwrap(), None);
    }

    #[test]
    fn m7_review_current_scope_selects_the_open_file_instead_of_the_first_file() {
        let directory = temp_directory("current-file-scope");
        let first = directory.join("cell1.inkpod");
        let current = directory.join("cell2.inkpod");
        saved_cell(&first, [10, 20, 30, 255]);
        saved_cell(&current, [40, 50, 60, 255]);
        let mut core = Core::new();
        core.open(&current).unwrap();
        let mut graph = replace_graph(&first, &directory.join("out"));
        graph.inputs = vec![BatchInputSelector {
            kind: BatchInputKind::Folder,
            path: directory.to_string_lossy().into_owned(),
            first_cell: 0,
            last_cell: 0,
        }];
        let report = core
            .batch_execute(
                &graph,
                BatchRunOptions {
                    scope: BatchRunScope::Current,
                    dry_run: true,
                    preview_confirmed: true,
                },
                |_, _| true,
            )
            .unwrap();
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].input_name, "cell2.inkpod");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn m7_review_file_wait_polls_cancellation_without_sleeping_the_full_interval() {
        let directory = temp_directory("wait-cancel");
        let first = directory.join("cell1.inkpod");
        let second = directory.join("cell2.inkpod");
        saved_cell(&first, [10, 20, 30, 255]);
        saved_cell(&second, [10, 20, 30, 255]);
        let mut graph = replace_graph(&first, &directory.join("out"));
        graph.inputs = vec![BatchInputSelector {
            kind: BatchInputKind::Folder,
            path: directory.to_string_lossy().into_owned(),
            first_cell: 0,
            last_cell: 0,
        }];
        graph.output.wait_milliseconds = 1_000;
        let started = std::time::Instant::now();
        let mut first_item_completion_polls = 0_u32;
        let report = Core::new()
            .batch_execute(
                &graph,
                BatchRunOptions {
                    scope: BatchRunScope::All,
                    dry_run: true,
                    preview_confirmed: true,
                },
                |completed, total| {
                    if completed == 3 && total == 6 {
                        first_item_completion_polls += 1;
                        return first_item_completion_polls == 1;
                    }
                    true
                },
            )
            .unwrap();
        assert!(report.cancelled);
        assert_eq!(report.items.len(), 1);
        assert!(started.elapsed() < Duration::from_millis(750));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn m7_review_every_operation_and_filter_variant_round_trips() {
        let directory = temp_directory("catalog-roundtrip");
        let settings = directory.join("catalog.inkbatch");
        let target = || Some(BatchTargetSelector::color_plane());
        let mut operations: Vec<_> = vec![
            Filter::SharpenWeak,
            Filter::SharpenStrong,
            Filter::BlurWeak,
            Filter::BlurStrong,
            Filter::GaussianBlur {
                radius: 2,
                strength_milli: 500,
            },
            Filter::UnsharpMask {
                radius: 2,
                amount_milli: 750,
                threshold: 12,
            },
            Filter::Invert {
                channel: Channel::Green,
            },
            Filter::AutoContrast,
            Filter::BrightnessContrast {
                brightness_milli: -100,
                contrast_milli: 200,
            },
            Filter::ToneCurve {
                channel: Channel::Blue,
                interpolation: CurveInterpolation::BSpline,
                points: vec![
                    CurvePoint {
                        input: 0,
                        output: 1,
                    },
                    CurvePoint {
                        input: u16::MAX,
                        output: u16::MAX - 1,
                    },
                ],
            },
            Filter::Levels(Levels {
                channel: Channel::Red,
                input_shadow: 1,
                input_gamma_milli: 1_100,
                input_highlight: u16::MAX - 1,
                output_shadow: 2,
                output_highlight: u16::MAX - 2,
            }),
            Filter::Hsv(HsvAdjustment {
                hue_degrees_milli: 45_000,
                saturation_milli: 100,
                value_milli: -100,
            }),
            Filter::ColorBalance(ColorBalance {
                red_milli: 100,
                green_milli: -100,
                blue_milli: 50,
            }),
        ]
        .into_iter()
        .map(|filter| BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: target(),
            kind: BatchOperationKind::Filter(filter),
        })
        .collect();
        let operation = |target, kind| BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target,
            kind,
        };
        operations.extend([
            operation(
                target(),
                BatchOperationKind::ColorReplace(vec![BatchColorPair {
                    enabled: true,
                    old: PixelValue::Rgba([1, 2, 3, 4]),
                    new: PixelValue::Rgba([4, 3, 2, 1]),
                }]),
            ),
            operation(
                target(),
                BatchOperationKind::ContinuousFill(vec![BatchSeed {
                    x: 1,
                    y: 2,
                    color: PixelValue::Rgba([10, 20, 30, 255]),
                    tolerance: 5,
                    gap_close: 1,
                    expected_source: Some(PixelValue::Rgba([1, 1, 1, 255])),
                }]),
            ),
            operation(
                target(),
                BatchOperationKind::Separation(BatchSeparation {
                    colors: vec![PixelValue::Rgba([1, 2, 3, 255])],
                    replacement: PixelValue::Rgba([9, 8, 7, 255]),
                    invert: true,
                }),
            ),
            operation(
                Some(BatchTargetSelector {
                    layer_id: None,
                    plane_id: None,
                    layer_kind: Some(LayerKind::BinaryColoring),
                    plane_kind: None,
                    missing_policy: BatchMissingTargetPolicy::Skip,
                }),
                BatchOperationKind::Visibility { visible: false },
            ),
            operation(
                Some(BatchTargetSelector {
                    layer_id: None,
                    plane_id: None,
                    layer_kind: Some(LayerKind::VectorColoring),
                    plane_kind: Some(PlaneType::VectorMainLine),
                    missing_policy: BatchMissingTargetPolicy::Skip,
                }),
                BatchOperationKind::LineWidth(VectorWidthMode::Add(0.5)),
            ),
            operation(
                Some(BatchTargetSelector {
                    layer_id: None,
                    plane_id: None,
                    layer_kind: Some(LayerKind::VectorColoring),
                    plane_kind: Some(PlaneType::VectorMainLine),
                    missing_policy: BatchMissingTargetPolicy::Skip,
                }),
                BatchOperationKind::LineWidth(VectorWidthMode::Subtract(0.25)),
            ),
            operation(
                Some(BatchTargetSelector {
                    layer_id: None,
                    plane_id: None,
                    layer_kind: Some(LayerKind::VectorColoring),
                    plane_kind: Some(PlaneType::VectorMainLine),
                    missing_policy: BatchMissingTargetPolicy::Skip,
                }),
                BatchOperationKind::LineWidth(VectorWidthMode::Scale(1.5)),
            ),
            operation(
                Some(BatchTargetSelector {
                    layer_id: None,
                    plane_id: None,
                    layer_kind: Some(LayerKind::VectorColoring),
                    plane_kind: Some(PlaneType::VectorMainLine),
                    missing_policy: BatchMissingTargetPolicy::Skip,
                }),
                BatchOperationKind::LineWidth(VectorWidthMode::Constant(2.0)),
            ),
            operation(
                target(),
                BatchOperationKind::BoundaryAirbrush(BoundaryAirbrush {
                    colors: vec![[0, 0, 0, u16::MAX], [u16::MAX; 4]],
                    width: 3,
                    strength_milli: 750,
                }),
            ),
            operation(
                target(),
                BatchOperationKind::DustRemoval(DustRemoval {
                    mode: DustMode::RemoveForeground,
                    maximum_pixels: 4,
                }),
            ),
            operation(None, BatchOperationKind::Mirror(MirrorAxis::Horizontal)),
            operation(None, BatchOperationKind::Rotate90(RotateDirection::Right90)),
            operation(
                None,
                BatchOperationKind::Resize(DocumentResize {
                    width: 16,
                    height: 12,
                    dpi_x_milli: 96_000,
                    dpi_y_milli: 120_000,
                    resample: true,
                    anchor: ResizeAnchor::BottomRight,
                }),
            ),
            operation(
                target(),
                BatchOperationKind::ConvertPlane {
                    destination_kind: PlaneType::Raster,
                    destination_format: PixelFormat::StraightRgba8,
                },
            ),
        ]);
        let graph = BatchGraph {
            version: BATCH_GRAPH_VERSION,
            name: "catalog".to_owned(),
            inputs: vec![BatchInputSelector::current_sequence()],
            operations,
            output: BatchOutputSettings::default(),
        };
        graph.save(&settings).unwrap();
        assert_eq!(BatchGraph::load(&settings).unwrap(), graph);
        fs::remove_dir_all(directory).unwrap();
    }
}
