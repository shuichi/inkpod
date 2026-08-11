//! Cut metadata, defaults, membership, history, and descriptor persistence.

use super::*;
use inkpod_format::{
    FileCutDefaults, FileCutDescriptor, FileCutHistoryEntry, FileCutMember, FileCutMetadata,
};
use std::collections::BTreeSet;
use std::path::{Component, Path};

/// Maximum number of individually referenced Cell documents in one Cut.
pub const MAX_CUT_MEMBERS: usize = 64;
/// Maximum UTF-8 byte length of one Cut metadata field.
pub const MAX_CUT_TEXT_BYTES: usize = 4096;

/// Stable identity of one Cut. Zero is invalid.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CutId(u64);

impl CutId {
    /// Returns the fixed-width persisted value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// User-editable Cut metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CutMetadata {
    /// Work or production title.
    pub work_title: String,
    /// Episode identifier.
    pub episode: String,
    /// Scene identifier.
    pub scene: String,
    /// Cut name shown in the UI.
    pub cut_name: String,
    /// Production instruction text.
    pub instruction: String,
    /// Cut duration in frames.
    pub duration_frames: u32,
}

/// Cell creation defaults owned by a Cut.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CutDefaults {
    /// Interpretation and dimensions of the default size.
    pub sizing: CellSizing,
    /// Horizontal resolution in thousandths of a DPI.
    pub dpi_x_milli: u32,
    /// Vertical resolution in thousandths of a DPI.
    pub dpi_y_milli: u32,
    /// Per-edge margin in thousandths of the 100% frame.
    pub margin_milli: u32,
    /// Safe-frame ratio in thousandths.
    pub safe_frame_ratio_milli: u32,
    /// Maximum-close-frame ratio in thousandths.
    pub maximum_close_ratio_milli: u32,
    /// Reference and maximum-close alignment anchor.
    pub anchor: FrameAnchor,
    /// Initial layer kind copied to a new Cell.
    pub initial_layer_kind: LayerKind,
    /// Initial exact-depth color storage format.
    pub pixel_format: PixelFormat,
}

impl CutDefaults {
    /// Produces the existing canonical Cell creation options with an explicit count.
    pub fn cell_creation_options(self, count: u32) -> Result<CellCreationOptions, CoreError> {
        let options = CellCreationOptions {
            sizing: self.sizing,
            dpi_x_milli: self.dpi_x_milli,
            dpi_y_milli: self.dpi_y_milli,
            margin_milli: self.margin_milli,
            safe_frame_ratio_milli: self.safe_frame_ratio_milli,
            maximum_close_ratio_milli: self.maximum_close_ratio_milli,
            anchor: self.anchor,
            initial_layer_kind: self.initial_layer_kind,
            pixel_format: self.pixel_format,
            count,
        };
        plan_cell_creation(&options)?;
        Ok(options)
    }
}

/// One ordered reference to an independently saved Cell document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CutMember {
    /// Stable Cell ID stored by the referenced Cell document.
    pub cell_id: u64,
    /// Persistent UUID stored by the referenced Cell document.
    pub document_uuid: u128,
    /// Positive presentation number within the Cut.
    pub display_number: u32,
    /// UTF-8 file name relative to the Cut descriptor directory.
    pub relative_path: String,
}

/// Complete immutable input used to create a Cut Genesis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CutCreateRequest {
    /// Persistent nonzero Cut UUID supplied by the frontend authority adapter.
    pub cut_uuid: u128,
    /// Initial Cut metadata.
    pub metadata: CutMetadata,
    /// Initial Cell creation defaults.
    pub defaults: CutDefaults,
    /// Initial ordered Cell membership.
    pub members: Vec<CutMember>,
}

/// One revision-bound metadata/defaults edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CutUpdateRequest {
    /// Session-local revision captured when the command was issued.
    pub base_revision: u64,
    /// Replacement Cut metadata.
    pub metadata: CutMetadata,
    /// Replacement defaults. Existing Cells are not modified.
    pub defaults: CutDefaults,
}

/// Observable result class for Cut mutation commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CutMutationOutcome {
    /// One canonical procedure was committed.
    Applied,
    /// The command produced no semantic change.
    NoOp,
}

/// Read-only summary of one Cut and its independent history/savepoint state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CutInfo {
    /// Stable Cut ID.
    pub cut_id: u64,
    /// Persistent Cut UUID.
    pub cut_uuid: u128,
    /// Session-local stale-request revision.
    pub revision: u64,
    /// Persistent current Cut state ID.
    pub state_id: u64,
    /// Current metadata.
    pub metadata: CutMetadata,
    /// Current Cell creation defaults.
    pub defaults: CutDefaults,
    /// Ordered member count.
    pub member_count: u32,
    /// Whether current state differs from the normal-save savepoint.
    pub dirty: bool,
    /// Whether one Cut history step can be undone.
    pub can_undo: bool,
    /// Whether one Cut history step can be redone.
    pub can_redo: bool,
    /// Whether this Cut was opened as recovery data.
    pub recovered: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CutHistoryEntry {
    procedure_id: u64,
    base_state_id: u64,
    committed_state_id: u64,
    before_metadata: CutMetadata,
    before_defaults: CutDefaults,
    after_metadata: CutMetadata,
    after_defaults: CutDefaults,
}

/// Single-writer Cut state machine, independent from every Cell `Core`.
#[derive(Clone, Debug)]
pub struct CutCore {
    cut_id: CutId,
    cut_uuid: u128,
    genesis_metadata: CutMetadata,
    genesis_defaults: CutDefaults,
    metadata: CutMetadata,
    defaults: CutDefaults,
    members: Vec<CutMember>,
    active_history: Vec<CutHistoryEntry>,
    inactive_history: Vec<CutHistoryEntry>,
    history_cursor: usize,
    current_state_id: u64,
    savepoint_state_id: Option<u64>,
    next_state_id: u64,
    next_procedure_id: u64,
    revision: u64,
    recovered: bool,
}

impl CutCore {
    /// Creates a new unsaved Cut without consuming any Cell-owned ID.
    pub fn new(request: CutCreateRequest) -> Result<Self, CoreError> {
        validate_cut_uuid(request.cut_uuid)?;
        validate_metadata(&request.metadata)?;
        request.defaults.cell_creation_options(1)?;
        validate_members(&request.members)?;
        Ok(Self {
            cut_id: CutId(1),
            cut_uuid: request.cut_uuid,
            genesis_metadata: request.metadata.clone(),
            genesis_defaults: request.defaults,
            metadata: request.metadata,
            defaults: request.defaults,
            members: request.members,
            active_history: Vec::new(),
            inactive_history: Vec::new(),
            history_cursor: 0,
            current_state_id: 1,
            savepoint_state_id: None,
            next_state_id: 2,
            next_procedure_id: 1,
            revision: 0,
            recovered: false,
        })
    }

    /// Opens and fully validates a Cut descriptor and every same-directory Cell reference.
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        let file = inkpod_format::read_cut_descriptor(path)?;
        let staged = Self::from_file(file, false)?;
        staged.validate_member_files(path)?;
        Ok(staged)
    }

    /// Opens Cut recovery data as dirty, without adopting a normal savepoint.
    pub fn open_recovery(path: &Path) -> Result<Self, CoreError> {
        let file = inkpod_format::read_cut_descriptor(path)?;
        let mut staged = Self::from_file(file, true)?;
        staged.validate_member_files(path)?;
        staged.savepoint_state_id = None;
        Ok(staged)
    }

    /// Returns current Cut state without exposing mutable references.
    #[must_use]
    pub fn info(&self) -> CutInfo {
        CutInfo {
            cut_id: self.cut_id.get(),
            cut_uuid: self.cut_uuid,
            revision: self.revision,
            state_id: self.current_state_id,
            metadata: self.metadata.clone(),
            defaults: self.defaults,
            member_count: self.members.len() as u32,
            dirty: self.savepoint_state_id != Some(self.current_state_id),
            can_undo: self.history_cursor != 0,
            can_redo: self.history_cursor < self.active_history.len(),
            recovered: self.recovered,
        }
    }

    /// Returns the immutable ordered Cell membership.
    #[must_use]
    pub fn members(&self) -> &[CutMember] {
        &self.members
    }

    /// Commits one metadata/defaults procedure, or reports a stable no-op.
    pub fn update(&mut self, request: CutUpdateRequest) -> Result<CutMutationOutcome, CoreError> {
        if request.base_revision != self.revision {
            return Err(CoreError::InvalidState("Cut base revision is stale"));
        }
        validate_metadata(&request.metadata)?;
        request.defaults.cell_creation_options(1)?;
        if request.metadata == self.metadata && request.defaults == self.defaults {
            return Ok(CutMutationOutcome::NoOp);
        }
        let committed_state_id = self.next_state_id;
        let next_state_id = self
            .next_state_id
            .checked_add(1)
            .ok_or(CoreError::InvalidState("Cut state ID overflows"))?;
        let procedure_id = self.next_procedure_id;
        let next_procedure_id = procedure_id
            .checked_add(1)
            .ok_or(CoreError::InvalidState("Cut procedure ID overflows"))?;
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(CoreError::InvalidState("Cut revision overflows"))?;
        let entry = CutHistoryEntry {
            procedure_id,
            base_state_id: self.current_state_id,
            committed_state_id,
            before_metadata: self.metadata.clone(),
            before_defaults: self.defaults,
            after_metadata: request.metadata.clone(),
            after_defaults: request.defaults,
        };
        if self.history_cursor < self.active_history.len() {
            self.inactive_history
                .extend(self.active_history.drain(self.history_cursor..));
        }
        self.active_history.push(entry);
        self.history_cursor += 1;
        self.metadata = request.metadata;
        self.defaults = request.defaults;
        self.current_state_id = committed_state_id;
        self.next_state_id = next_state_id;
        self.next_procedure_id = next_procedure_id;
        self.revision = next_revision;
        Ok(CutMutationOutcome::Applied)
    }

    /// Cancels a not-yet-committed dialog edit without changing any Cut state.
    #[must_use]
    pub const fn cancel_update(&self) -> CutMutationOutcome {
        CutMutationOutcome::NoOp
    }

    /// Moves one step backward in Cut-owned history.
    pub fn undo(&mut self) -> Result<CutMutationOutcome, CoreError> {
        if self.history_cursor == 0 {
            return Ok(CutMutationOutcome::NoOp);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CoreError::InvalidState("Cut revision overflows"))?;
        let entry = &self.active_history[self.history_cursor - 1];
        self.metadata = entry.before_metadata.clone();
        self.defaults = entry.before_defaults;
        self.current_state_id = entry.base_state_id;
        self.history_cursor -= 1;
        self.revision = revision;
        Ok(CutMutationOutcome::Applied)
    }

    /// Moves one step forward in Cut-owned history.
    pub fn redo(&mut self) -> Result<CutMutationOutcome, CoreError> {
        if self.history_cursor >= self.active_history.len() {
            return Ok(CutMutationOutcome::NoOp);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CoreError::InvalidState("Cut revision overflows"))?;
        let entry = &self.active_history[self.history_cursor];
        self.metadata = entry.after_metadata.clone();
        self.defaults = entry.after_defaults;
        self.current_state_id = entry.committed_state_id;
        self.history_cursor += 1;
        self.revision = revision;
        Ok(CutMutationOutcome::Applied)
    }

    /// Atomically saves the Cut descriptor after validating every referenced Cell file.
    pub fn save(&mut self, path: &Path) -> Result<CutInfo, CoreError> {
        self.validate_member_files(path)?;
        let prospective = self.to_file(Some(self.current_state_id));
        inkpod_format::save_cut_descriptor_atomic(path, &prospective)?;
        self.savepoint_state_id = Some(self.current_state_id);
        self.recovered = false;
        Ok(self.info())
    }

    /// Writes recovery data without advancing the normal Cut savepoint.
    pub fn autosave(&self, path: &Path) -> Result<CutInfo, CoreError> {
        self.validate_member_files(path)?;
        inkpod_format::save_cut_recovery_atomic(path, &self.to_file(self.savepoint_state_id))?;
        Ok(self.info())
    }

    fn validate_member_files(&self, descriptor_path: &Path) -> Result<(), CoreError> {
        let directory = descriptor_path.parent().ok_or(CoreError::InvalidArgument(
            "Cut descriptor has no directory",
        ))?;
        let directory = if directory.as_os_str().is_empty() {
            Path::new(".")
        } else {
            directory
        };
        let canonical_directory = std::fs::canonicalize(directory)
            .map_err(|error| CoreError::Format(error.to_string()))?;
        let descriptor_name = descriptor_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(CoreError::InvalidArgument(
                "Cut descriptor name is not UTF-8",
            ))?;
        for member in &self.members {
            if member.relative_path.eq_ignore_ascii_case(descriptor_name) {
                return Err(CoreError::InvalidArgument(
                    "Cut descriptor cannot reference itself as a Cell",
                ));
            }
            let member_path = directory.join(&member.relative_path);
            let canonical_member = std::fs::canonicalize(&member_path)
                .map_err(|error| CoreError::Format(error.to_string()))?;
            if canonical_member.parent() != Some(canonical_directory.as_path()) {
                return Err(CoreError::InvalidArgument(
                    "Cut Cell reference leaves the descriptor directory",
                ));
            }
            let mut cell = Core::new();
            let info = cell.open(&canonical_member)?;
            if info.cell_id != member.cell_id || info.document_uuid != member.document_uuid {
                return Err(CoreError::InvalidArgument(
                    "Cut Cell identity does not match its descriptor member",
                ));
            }
        }
        Ok(())
    }

    fn to_file(&self, savepoint: Option<u64>) -> FileCutDescriptor {
        FileCutDescriptor {
            cut_id: self.cut_id.get(),
            cut_uuid: self.cut_uuid.to_le_bytes(),
            current_state_id: self.current_state_id,
            savepoint_state_id: savepoint.unwrap_or(0),
            next_state_id: self.next_state_id,
            next_procedure_id: self.next_procedure_id,
            history_cursor: self.history_cursor as u32,
            genesis_metadata: metadata_to_file(&self.genesis_metadata),
            genesis_defaults: defaults_to_file(self.genesis_defaults),
            metadata: metadata_to_file(&self.metadata),
            defaults: defaults_to_file(self.defaults),
            members: self.members.iter().map(member_to_file).collect(),
            active_history: self.active_history.iter().map(history_to_file).collect(),
            inactive_history: self.inactive_history.iter().map(history_to_file).collect(),
        }
    }

    fn from_file(file: FileCutDescriptor, recovered: bool) -> Result<Self, CoreError> {
        let cut_uuid = u128::from_le_bytes(file.cut_uuid);
        validate_cut_uuid(cut_uuid)?;
        let genesis_metadata = metadata_from_file(file.genesis_metadata)?;
        let genesis_defaults = defaults_from_file(file.genesis_defaults)?;
        let metadata = metadata_from_file(file.metadata)?;
        let defaults = defaults_from_file(file.defaults)?;
        let members = file
            .members
            .into_iter()
            .map(member_from_file)
            .collect::<Result<Vec<_>, _>>()?;
        validate_members(&members)?;
        let active_history = file
            .active_history
            .into_iter()
            .map(history_from_file)
            .collect::<Result<Vec<_>, _>>()?;
        let inactive_history = file
            .inactive_history
            .into_iter()
            .map(history_from_file)
            .collect::<Result<Vec<_>, _>>()?;
        let cursor = file.history_cursor as usize;
        if cursor > active_history.len() {
            return Err(CoreError::Format(
                "Cut history cursor is invalid".to_owned(),
            ));
        }
        let mut replay_metadata = genesis_metadata.clone();
        let mut replay_defaults = genesis_defaults;
        let mut replay_state_id = 1_u64;
        let mut maximum_state_id = 1_u64;
        let mut maximum_procedure_id = 0_u64;
        for (index, entry) in active_history.iter().enumerate() {
            if entry.base_state_id != replay_state_id
                || entry.before_metadata != replay_metadata
                || entry.before_defaults != replay_defaults
            {
                return Err(CoreError::Format(
                    "Cut canonical history chain is invalid".to_owned(),
                ));
            }
            replay_metadata = entry.after_metadata.clone();
            replay_defaults = entry.after_defaults;
            replay_state_id = entry.committed_state_id;
            maximum_state_id = maximum_state_id.max(entry.committed_state_id);
            maximum_procedure_id = maximum_procedure_id.max(entry.procedure_id);
            if index + 1 == cursor
                && (replay_metadata != metadata
                    || replay_defaults != defaults
                    || replay_state_id != file.current_state_id)
            {
                return Err(CoreError::Format(
                    "Cut current state does not match replay cursor".to_owned(),
                ));
            }
        }
        if cursor == 0
            && (metadata != genesis_metadata
                || defaults != genesis_defaults
                || file.current_state_id != 1)
        {
            return Err(CoreError::Format(
                "Cut Genesis state does not match replay cursor".to_owned(),
            ));
        }
        for entry in &inactive_history {
            maximum_state_id = maximum_state_id
                .max(entry.base_state_id)
                .max(entry.committed_state_id);
            maximum_procedure_id = maximum_procedure_id.max(entry.procedure_id);
        }
        if file.next_state_id <= maximum_state_id || file.next_procedure_id <= maximum_procedure_id
        {
            return Err(CoreError::Format(
                "Cut ID high-watermark is invalid".to_owned(),
            ));
        }
        Ok(Self {
            cut_id: CutId(file.cut_id),
            cut_uuid,
            genesis_metadata,
            genesis_defaults,
            metadata,
            defaults,
            members,
            active_history,
            inactive_history,
            history_cursor: cursor,
            current_state_id: file.current_state_id,
            savepoint_state_id: (file.savepoint_state_id != 0).then_some(file.savepoint_state_id),
            next_state_id: file.next_state_id,
            next_procedure_id: file.next_procedure_id,
            revision: 0,
            recovered,
        })
    }
}

fn validate_cut_uuid(uuid: u128) -> Result<(), CoreError> {
    if uuid == 0 {
        return Err(CoreError::InvalidArgument("Cut UUID must be nonzero"));
    }
    Ok(())
}

fn validate_metadata(metadata: &CutMetadata) -> Result<(), CoreError> {
    for value in [
        &metadata.work_title,
        &metadata.episode,
        &metadata.scene,
        &metadata.cut_name,
        &metadata.instruction,
    ] {
        if value.len() > MAX_CUT_TEXT_BYTES || value.as_bytes().contains(&0) {
            return Err(CoreError::InvalidArgument("Cut metadata text is invalid"));
        }
    }
    if metadata.cut_name.is_empty() || metadata.duration_frames == 0 {
        return Err(CoreError::InvalidArgument(
            "Cut name and duration must be nonzero",
        ));
    }
    Ok(())
}

fn validate_members(members: &[CutMember]) -> Result<(), CoreError> {
    if members.len() > MAX_CUT_MEMBERS {
        return Err(CoreError::InvalidArgument("Cut member count exceeds limit"));
    }
    let mut identities = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for member in members {
        if member.cell_id == 0 || member.document_uuid == 0 || member.display_number == 0 {
            return Err(CoreError::InvalidArgument("Cut member identity is invalid"));
        }
        validate_relative_member_path(&member.relative_path)?;
        if !identities.insert((member.cell_id, member.document_uuid))
            || !paths.insert(member.relative_path.to_lowercase())
        {
            return Err(CoreError::InvalidArgument("Cut member is duplicated"));
        }
    }
    Ok(())
}

fn validate_relative_member_path(value: &str) -> Result<(), CoreError> {
    if value.is_empty()
        || value.len() > 255
        || value.contains(['/', '\\', ':'])
        || value == "."
        || value == ".."
        || !value.to_ascii_lowercase().ends_with(".inkpod")
    {
        return Err(CoreError::InvalidArgument(
            "Cut member path must be one relative .inkpod file name",
        ));
    }
    let mut components = Path::new(value).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(CoreError::InvalidArgument(
            "Cut member path is not relative",
        ));
    }
    Ok(())
}

fn metadata_to_file(value: &CutMetadata) -> FileCutMetadata {
    FileCutMetadata {
        work_title: value.work_title.clone(),
        episode: value.episode.clone(),
        scene: value.scene.clone(),
        cut_name: value.cut_name.clone(),
        instruction: value.instruction.clone(),
        duration_frames: value.duration_frames,
    }
}

fn metadata_from_file(value: FileCutMetadata) -> Result<CutMetadata, CoreError> {
    let value = CutMetadata {
        work_title: value.work_title,
        episode: value.episode,
        scene: value.scene,
        cut_name: value.cut_name,
        instruction: value.instruction,
        duration_frames: value.duration_frames,
    };
    validate_metadata(&value)?;
    Ok(value)
}

fn defaults_to_file(value: CutDefaults) -> FileCutDefaults {
    let (sizing_mode, size_a, size_b) = match value.sizing {
        CellSizing::ImagePixels { width, height } => (1, width, height),
        CellSizing::FrameMicrometres { width, height } => (2, width, height),
    };
    FileCutDefaults {
        sizing_mode,
        size_a,
        size_b,
        dpi_x_milli: value.dpi_x_milli,
        dpi_y_milli: value.dpi_y_milli,
        margin_milli: value.margin_milli,
        safe_frame_ratio_milli: value.safe_frame_ratio_milli,
        maximum_close_ratio_milli: value.maximum_close_ratio_milli,
        anchor: frame_anchor_code(value.anchor),
        initial_layer_kind: layer_kind_code(value.initial_layer_kind),
        pixel_format: pixel_format_code(value.pixel_format),
    }
}

fn defaults_from_file(value: FileCutDefaults) -> Result<CutDefaults, CoreError> {
    let sizing = match value.sizing_mode {
        1 => CellSizing::ImagePixels {
            width: value.size_a,
            height: value.size_b,
        },
        2 => CellSizing::FrameMicrometres {
            width: value.size_a,
            height: value.size_b,
        },
        _ => return Err(CoreError::Format("Cut sizing mode is unknown".to_owned())),
    };
    let defaults = CutDefaults {
        sizing,
        dpi_x_milli: value.dpi_x_milli,
        dpi_y_milli: value.dpi_y_milli,
        margin_milli: value.margin_milli,
        safe_frame_ratio_milli: value.safe_frame_ratio_milli,
        maximum_close_ratio_milli: value.maximum_close_ratio_milli,
        anchor: frame_anchor_from_code(value.anchor)?,
        initial_layer_kind: layer_kind_from_code(value.initial_layer_kind)?,
        pixel_format: pixel_format_from_code(value.pixel_format)?,
    };
    defaults.cell_creation_options(1)?;
    Ok(defaults)
}

fn member_to_file(value: &CutMember) -> FileCutMember {
    FileCutMember {
        cell_id: value.cell_id,
        document_uuid: value.document_uuid.to_le_bytes(),
        display_number: value.display_number,
        relative_path: value.relative_path.clone(),
    }
}

fn member_from_file(value: FileCutMember) -> Result<CutMember, CoreError> {
    let member = CutMember {
        cell_id: value.cell_id,
        document_uuid: u128::from_le_bytes(value.document_uuid),
        display_number: value.display_number,
        relative_path: value.relative_path,
    };
    validate_members(std::slice::from_ref(&member))?;
    Ok(member)
}

fn history_to_file(value: &CutHistoryEntry) -> FileCutHistoryEntry {
    FileCutHistoryEntry {
        procedure_id: value.procedure_id,
        base_state_id: value.base_state_id,
        committed_state_id: value.committed_state_id,
        before_metadata: metadata_to_file(&value.before_metadata),
        before_defaults: defaults_to_file(value.before_defaults),
        after_metadata: metadata_to_file(&value.after_metadata),
        after_defaults: defaults_to_file(value.after_defaults),
    }
}

fn history_from_file(value: FileCutHistoryEntry) -> Result<CutHistoryEntry, CoreError> {
    Ok(CutHistoryEntry {
        procedure_id: value.procedure_id,
        base_state_id: value.base_state_id,
        committed_state_id: value.committed_state_id,
        before_metadata: metadata_from_file(value.before_metadata)?,
        before_defaults: defaults_from_file(value.before_defaults)?,
        after_metadata: metadata_from_file(value.after_metadata)?,
        after_defaults: defaults_from_file(value.after_defaults)?,
    })
}

const fn frame_anchor_code(value: FrameAnchor) -> u32 {
    match value {
        FrameAnchor::TopLeft => 1,
        FrameAnchor::TopRight => 2,
        FrameAnchor::Center => 3,
        FrameAnchor::BottomLeft => 4,
        FrameAnchor::BottomRight => 5,
    }
}

fn frame_anchor_from_code(value: u32) -> Result<FrameAnchor, CoreError> {
    match value {
        1 => Ok(FrameAnchor::TopLeft),
        2 => Ok(FrameAnchor::TopRight),
        3 => Ok(FrameAnchor::Center),
        4 => Ok(FrameAnchor::BottomLeft),
        5 => Ok(FrameAnchor::BottomRight),
        _ => Err(CoreError::Format("Cut frame anchor is unknown".to_owned())),
    }
}

const fn layer_kind_code(value: LayerKind) -> u32 {
    match value {
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

fn layer_kind_from_code(value: u32) -> Result<LayerKind, CoreError> {
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
        _ => Err(CoreError::Format("Cut layer kind is unknown".to_owned())),
    }
}

const fn pixel_format_code(value: PixelFormat) -> u32 {
    match value {
        PixelFormat::BinaryMask8 => 1,
        PixelFormat::Grayscale8 => 2,
        PixelFormat::Grayscale16 => 3,
        PixelFormat::StraightRgba8 => 4,
        PixelFormat::StraightRgba16 => 5,
        PixelFormat::PremultipliedBgra8 => 6,
    }
}

fn pixel_format_from_code(value: u32) -> Result<PixelFormat, CoreError> {
    match value {
        1 => Ok(PixelFormat::BinaryMask8),
        2 => Ok(PixelFormat::Grayscale8),
        3 => Ok(PixelFormat::Grayscale16),
        4 => Ok(PixelFormat::StraightRgba8),
        5 => Ok(PixelFormat::StraightRgba16),
        6 => Ok(PixelFormat::PremultipliedBgra8),
        _ => Err(CoreError::Format("Cut pixel format is unknown".to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata(name: &str) -> CutMetadata {
        CutMetadata {
            work_title: "Inkpod".to_owned(),
            episode: "01".to_owned(),
            scene: "A".to_owned(),
            cut_name: name.to_owned(),
            instruction: String::new(),
            duration_frames: 24,
        }
    }

    fn test_defaults() -> CutDefaults {
        CutDefaults {
            sizing: CellSizing::ImagePixels {
                width: 32,
                height: 24,
            },
            dpi_x_milli: DEFAULT_DPI_MILLI,
            dpi_y_milli: DEFAULT_DPI_MILLI,
            margin_milli: 0,
            safe_frame_ratio_milli: 900,
            maximum_close_ratio_milli: 500,
            anchor: FrameAnchor::Center,
            initial_layer_kind: LayerKind::BinaryColoring,
            pixel_format: PixelFormat::StraightRgba8,
        }
    }

    fn test_cut() -> CutCore {
        CutCore::new(CutCreateRequest {
            cut_uuid: 1,
            metadata: test_metadata("C001"),
            defaults: test_defaults(),
            members: Vec::new(),
        })
        .unwrap()
    }

    fn changed_request(cut: &CutCore) -> CutUpdateRequest {
        CutUpdateRequest {
            base_revision: cut.revision,
            metadata: test_metadata("C002"),
            defaults: cut.defaults,
        }
    }

    #[test]
    fn cut_counter_overflow_is_atomic_for_update_undo_and_redo() {
        for counter in 0..3 {
            let mut cut = test_cut();
            match counter {
                0 => cut.next_state_id = u64::MAX,
                1 => cut.next_procedure_id = u64::MAX,
                2 => cut.revision = u64::MAX,
                _ => unreachable!(),
            }
            let before = cut.clone();
            assert!(matches!(
                cut.update(changed_request(&cut)),
                Err(CoreError::InvalidState(_))
            ));
            assert_eq!(cut.info(), before.info());
            assert_eq!(cut.active_history, before.active_history);
            assert_eq!(cut.inactive_history, before.inactive_history);
            assert_eq!(cut.next_state_id, before.next_state_id);
            assert_eq!(cut.next_procedure_id, before.next_procedure_id);
        }

        let mut undo_cut = test_cut();
        undo_cut.update(changed_request(&undo_cut)).unwrap();
        undo_cut.revision = u64::MAX;
        let undo_before = undo_cut.clone();
        assert!(matches!(undo_cut.undo(), Err(CoreError::InvalidState(_))));
        assert_eq!(undo_cut.info(), undo_before.info());

        let mut redo_cut = test_cut();
        redo_cut.update(changed_request(&redo_cut)).unwrap();
        redo_cut.undo().unwrap();
        redo_cut.revision = u64::MAX;
        let redo_before = redo_cut.clone();
        assert!(matches!(redo_cut.redo(), Err(CoreError::InvalidState(_))));
        assert_eq!(redo_cut.info(), redo_before.info());
    }
}
