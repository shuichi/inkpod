use super::*;

use super::codec::{
    failure_policy_code, input_kind_code, operation_from_file, operation_to_file,
    output_policy_code, parse_failure_policy, parse_input_kind, parse_output_policy,
};
use super::validation::{validate_component, validate_operation, validate_path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Source category expanded by a batch graph.
pub enum BatchInputKind {
    /// One native document file.
    File,
    /// Native document files in one folder.
    Folder,
    /// Cells currently installed in Core sequence state.
    CurrentSequence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One bounded batch input source and optional inclusive cell-number range.
pub struct BatchInputSelector {
    /// Source category.
    pub kind: BatchInputKind,
    /// UTF-8 file/folder path; empty only for current-sequence input.
    pub path: String,
    /// Zero means unbounded.
    pub first_cell: u32,
    /// Zero means unbounded.
    pub last_cell: u32,
}

impl BatchInputSelector {
    /// Creates an unbounded single-file selector.
    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self {
            kind: BatchInputKind::File,
            path: path.into(),
            first_cell: 0,
            last_cell: 0,
        }
    }

    /// Creates an unbounded selector for current Core sequence state.
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
/// Behavior when an operation target cannot be resolved in an input document.
pub enum BatchMissingTargetPolicy {
    /// Skip the operation for that item.
    Skip,
    /// Fail the batch item.
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Optional stable-ID and semantic filters used to select a batch target plane.
pub struct BatchTargetSelector {
    /// Exact stable layer ID, when known across all inputs.
    pub layer_id: Option<u64>,
    /// Exact stable plane ID, when known across all inputs.
    pub plane_id: Option<u64>,
    /// Required semantic layer kind.
    pub layer_kind: Option<LayerKind>,
    /// Required semantic plane kind.
    pub plane_kind: Option<PlaneType>,
    /// Policy when no plane matches all supplied filters.
    pub missing_policy: BatchMissingTargetPolicy,
}

impl BatchTargetSelector {
    /// Selects the color plane of a binary-coloring layer and errors if absent.
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
/// One optionally enabled exact color replacement.
pub struct BatchColorPair {
    /// Whether this pair participates in the operation.
    pub enabled: bool,
    /// Source pixel value.
    pub old: PixelValue,
    /// Replacement pixel value.
    pub new: PixelValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One document-pixel seed used by continuous batch fill.
pub struct BatchSeed {
    /// Whether this seed participates in the operation.
    pub enabled: bool,
    /// Seed x-coordinate in document pixels.
    pub x: u32,
    /// Seed y-coordinate in document pixels.
    pub y: u32,
    /// Straight-alpha fill color.
    pub color: PixelValue,
    /// Inclusive channel tolerance.
    pub tolerance: u16,
    /// Maximum bounded gap-closing distance.
    pub gap_close: u8,
    /// Optional guard color that must match the source seed.
    pub expected_source: Option<PixelValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Palette of colors separated into a replacement value.
pub struct BatchSeparation {
    /// Source colors to match.
    pub colors: Vec<PixelValue>,
    /// Replacement pixel value.
    pub replacement: PixelValue,
    /// Whether matching logic is inverted.
    pub invert: bool,
    /// Destination receiving the separated pixels.
    pub destination: BatchSeparationDestination,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Semantic destination for one color-separation operation.
pub enum BatchSeparationDestination {
    /// Replaces the selected source plane.
    ReplaceSource,
    /// Replaces the document selection mask.
    SelectionMask,
    /// Replaces the main-line plane in the source layer.
    MainLinePlane,
    /// Replaces the color plane in the source layer.
    ColorPlane,
    /// Writes the separated result through the normal native batch output path.
    NativeFile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Stable identity of an immutable sequence raster owned by Core.
pub struct SequenceSourceIdentity {
    /// Persistent nonzero source document UUID.
    pub document_uuid: u128,
    /// Nonzero immutable source generation.
    pub source_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// One exact old/new pair candidate extracted from aligned sequence cells.
pub struct BatchPairCandidate {
    /// Exact source pixel value, including alpha.
    pub old: PixelValue,
    /// Exact destination pixel value, including alpha.
    pub new: PixelValue,
    /// Number of aligned pixels exhibiting this transition.
    pub pixel_count: u64,
    /// Half-open document-pixel bounds of all matching coordinates.
    pub affected_bounds: RectI32,
    /// Whether this old value maps to more than one destination value.
    pub ambiguous: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Explicit resolution for one ambiguous old color group.
pub struct BatchPairResolution {
    /// Ambiguous source value being resolved.
    pub old: PixelValue,
    /// Chosen candidate, or `None` to exclude the old value.
    pub selected_new: Option<PixelValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Deterministic exact-pixel comparison of two immutable sequence cells.
pub struct BatchPairExtraction {
    /// Compared raster width.
    pub width: u32,
    /// Compared raster height.
    pub height: u32,
    /// Shared native pixel format.
    pub pixel_format: PixelFormat,
    /// Number of exactly unchanged aligned pixels.
    pub unchanged_pixel_count: u64,
    /// Number of old-value groups requiring an explicit decision.
    pub ambiguity_count: u32,
    /// Ordered exact transition candidates.
    pub candidates: Vec<BatchPairCandidate>,
}

impl BatchPairExtraction {
    /// Produces enabled replacement rows after every ambiguous group is resolved.
    ///
    /// Unambiguous groups are included automatically. Unknown, duplicate, missing,
    /// or non-candidate resolutions return an error without changing Core state.
    pub fn resolved_pairs(
        &self,
        resolutions: &[BatchPairResolution],
    ) -> Result<Vec<BatchColorPair>, CoreError> {
        super::pairs::resolve_pairs(self, resolutions)
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Operation payload supported by a batch graph.
pub enum BatchOperationKind {
    /// Replaces enabled exact color pairs.
    ColorReplace(Vec<BatchColorPair>),
    /// Applies ordered seed-fill requests.
    ContinuousFill(Vec<BatchSeed>),
    /// Separates matching colors.
    Separation(BatchSeparation),
    /// Sets target visibility.
    Visibility {
        /// New visibility state.
        visible: bool,
    },
    /// Applies an image filter.
    Filter(Filter),
    /// Applies boundary-aware airbrush settings.
    BoundaryAirbrush(BoundaryAirbrush),
    /// Removes bounded dust regions.
    DustRemoval(DustRemoval),
    /// Mirrors the document.
    Mirror(MirrorAxis),
    /// Rotates the document by 90 degrees.
    Rotate90(RotateDirection),
    /// Resizes the document.
    Resize(DocumentResize),
    /// Converts a raster plane.
    ConvertPlane {
        /// Destination semantic plane kind.
        destination_kind: PlaneType,
        /// Destination pixel format.
        destination_format: PixelFormat,
    },
}

#[derive(Clone, Debug, PartialEq)]
/// Versioned, optionally enabled operation in a batch graph.
pub struct BatchOperation {
    /// Must equal [`BATCH_OPERATION_VERSION`].
    pub version: u32,
    /// Whether execution includes this operation.
    pub enabled: bool,
    /// Whether a frontend must request parameters for every run.
    pub configure_each_run: bool,
    /// Optional plane target selector.
    pub target: Option<BatchTargetSelector>,
    /// Operation-specific payload.
    pub kind: BatchOperationKind,
}

impl BatchOperation {
    /// Swaps old/new values in every color-replacement pair.
    ///
    /// Non-color-replacement operations return an error without mutation.
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
/// Policy used to derive or authorize batch output paths.
pub enum BatchOutputPolicy {
    /// Writes a duplicate without replacing the input.
    Duplicate,
    /// Writes a newly named normal-save output.
    NewSave,
    /// Explicitly authorizes replacement of the input path.
    ExplicitOverwrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Batch behavior after an individual item fails.
pub enum BatchFailurePolicy {
    /// Record the failure and continue with later items.
    Continue,
    /// Stop before processing the next item.
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Output naming, confirmation, pacing, and failure settings for a batch graph.
pub struct BatchOutputSettings {
    /// Output-path policy.
    pub policy: BatchOutputPolicy,
    /// Optional output folder path.
    pub folder: String,
    /// Whether each input receives a cell subfolder.
    pub cell_folder: bool,
    /// Optional output basename.
    pub basename: String,
    /// First generated numeric suffix.
    pub start_number: u32,
    /// Whether generated numbers descend.
    pub descending: bool,
    /// Behavior after item failure.
    pub failure_policy: BatchFailurePolicy,
    /// Bounded delay between items, in milliseconds.
    pub wait_milliseconds: u32,
    /// Whether execution requires preview confirmation before saving.
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
/// Complete versioned batch-processing graph.
pub struct BatchGraph {
    /// Native batch graph version.
    pub version: u32,
    /// User-visible graph name.
    pub name: String,
    /// Ordered input selectors.
    pub inputs: Vec<BatchInputSelector>,
    /// Ordered operations applied to each input.
    pub operations: Vec<BatchOperation>,
    /// Output and failure settings.
    pub output: BatchOutputSettings,
}

impl BatchGraph {
    /// Validates and atomically writes the graph to `path`.
    ///
    /// The destination is not partially replaced on validation or I/O failure.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        save_batch_graph_atomic(path, &self.to_file()?)?;
        Ok(())
    }

    /// Reads and fully validates a batch graph from `path`.
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        Self::from_file(read_batch_graph(path)?)
    }

    /// Validates graph versions, bounds, paths, targets, and operation payloads.
    ///
    /// This is a read-only check and does not access input documents.
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
        if self.output.policy == BatchOutputPolicy::ExplicitOverwrite
            && self.operations.iter().any(|operation| {
                operation.enabled
                    && matches!(
                        operation.kind,
                        BatchOperationKind::Separation(BatchSeparation {
                            destination: BatchSeparationDestination::NativeFile,
                            ..
                        })
                    )
            })
        {
            return Err(CoreError::InvalidArgument(
                "native-file separation cannot overwrite its input",
            ));
        }
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
/// Subset of expanded batch inputs to process.
pub enum BatchRunScope {
    /// Processes only the first/current expanded item.
    Current,
    /// Processes every expanded item.
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Per-run controls independent of persistent graph settings.
pub struct BatchRunOptions {
    /// Expanded-input scope.
    pub scope: BatchRunScope,
    /// Whether to validate and simulate without writing outputs.
    pub dry_run: bool,
    /// Whether a required preview has been explicitly confirmed.
    pub preview_confirmed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Planned input/output mapping and warnings for one batch item.
pub struct BatchPreviewItem {
    /// Stable display label for the expanded input.
    pub input_name: String,
    /// Derived output path, or `None` when no output would be written.
    pub output_path: Option<PathBuf>,
    /// Non-fatal validation or overwrite warnings.
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Read-only expansion and validation preview for a batch run.
pub struct BatchPreview {
    /// Preview items in deterministic execution order.
    pub items: Vec<BatchPreviewItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Terminal result class for one batch item.
pub enum BatchItemOutcome {
    /// All enabled operations and output completed.
    Succeeded,
    /// Policy intentionally skipped the item.
    Skipped,
    /// Validation, processing, or output failed.
    Failed,
    /// Cancellation was observed before item commit/output completion.
    Cancelled,
    /// Dry-run validation completed without output.
    DryRun,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Terminal report entry for one expanded batch item.
pub struct BatchItemResult {
    /// Stable display label for the input.
    pub input_name: String,
    /// Attempted or planned output path.
    pub output_path: Option<PathBuf>,
    /// Terminal result class.
    pub outcome: BatchItemOutcome,
    /// Human-readable diagnostic or status text.
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Complete deterministic report for a batch execution.
pub struct BatchRunReport {
    /// Item results in execution order.
    pub items: Vec<BatchItemResult>,
    /// Whether cancellation stopped the run.
    pub cancelled: bool,
}

impl BatchRunReport {
    /// Counts items whose terminal outcome is [`BatchItemOutcome::Failed`].
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
    Document {
        document: Box<CellDocument>,
        assets: asset::AssetStore,
    },
    Sequence(SequenceCellSource),
}

#[derive(Clone)]
pub(super) struct BatchSource {
    pub(super) label: String,
    pub(super) input_path: Option<PathBuf>,
    pub(super) content: BatchSourceContent,
}
