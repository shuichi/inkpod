use super::*;

use super::codec::{
    failure_policy_code, input_kind_code, operation_from_file, operation_to_file,
    output_policy_code, parse_failure_policy, parse_input_kind, parse_output_policy,
};
use super::validation::{validate_component, validate_operation, validate_path};

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
pub(super) enum BatchSourceContent {
    Path(PathBuf),
    Document(Box<CellDocument>),
    Sequence(SequenceCellSource),
}

#[derive(Clone)]
pub(super) struct BatchSource {
    pub(super) label: String,
    pub(super) input_path: Option<PathBuf>,
    pub(super) content: BatchSourceContent,
}
