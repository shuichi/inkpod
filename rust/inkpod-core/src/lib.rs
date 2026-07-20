#![forbid(unsafe_code)]

use inkpod_format::{CellFile, FilePlane, FileTile, FormatError, PlaneKind};
use inkpod_image::{
    PixelFormat, PixelValue, RasterError, TILE_SIZE, TileCoord, TileData, TileRaster,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const CORE_FEATURES: u64 = 1;
pub const DEFAULT_DPI_MILLI: u32 = 96_000;
const MAX_STROKE_SAMPLES: usize = 1_048_576;
const MAX_BRUSH_DIAMETER: f32 = 256.0;
const MAX_STROKE_COORDINATE: f32 = 16_777_216.0;
const MAX_STROKE_WORK: u64 = 16_777_216;
const MIN_ZOOM: f64 = 0.01;
const MAX_ZOOM: f64 = 64.0;

pub use inkpod_format::{FrameMetadata, Margins, RectI32};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    NoOp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchOutcome {
    revision: u64,
    accepted_commands: u64,
}

impl DispatchOutcome {
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn accepted_commands(self) -> u64 {
        self.accepted_commands
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivePlane {
    MainLine,
    Color,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaintTool {
    Pencil,
    Brush,
    Eraser,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinateSpace {
    Document,
    Device,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeSample {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub tool: PaintTool,
    pub plane: ActivePlane,
    pub color: [u8; 4],
    pub diameter: f32,
    pub auto_erase: bool,
    pub pressure_size: bool,
    pub coordinate_space: CoordinateSpace,
    pub samples: Vec<StrokeSample>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewCommand {
    PanBy {
        device_dx: f64,
        device_dy: f64,
    },
    ZoomAt {
        factor: f64,
        device_x: f64,
        device_y: f64,
    },
    Fit {
        viewport_width: f64,
        viewport_height: f64,
    },
    OneToOne {
        viewport_width: f64,
        viewport_height: f64,
    },
    ViewportResized {
        viewport_width: f64,
        viewport_height: f64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewMode {
    Manual,
    Fit,
    OneToOne,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    zoom: f64,
    pan_x: f64,
    pan_y: f64,
    revision: u64,
    mode: ViewMode,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            revision: 0,
            mode: ViewMode::Manual,
        }
    }
}

impl ViewState {
    #[must_use]
    pub const fn zoom(self) -> f64 {
        self.zoom
    }

    #[must_use]
    pub const fn pan_x(self) -> f64 {
        self.pan_x
    }

    #[must_use]
    pub const fn pan_y(self) -> f64 {
        self.pan_y
    }

    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn mode(self) -> ViewMode {
        self.mode
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreError {
    NoDocument,
    InvalidArgument(&'static str),
    InvalidState(&'static str),
    Raster(RasterError),
    Format(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDocument => formatter.write_str("no cell document is open"),
            Self::InvalidArgument(message) => write!(formatter, "invalid argument: {message}"),
            Self::InvalidState(message) => write!(formatter, "invalid state: {message}"),
            Self::Raster(error) => write!(formatter, "raster error: {error}"),
            Self::Format(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<RasterError> for CoreError {
    fn from(error: RasterError) -> Self {
        Self::Raster(error)
    }
}

impl From<FormatError> for CoreError {
    fn from(error: FormatError) -> Self {
        Self::Format(error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentInfo {
    pub document_revision: u64,
    pub view_revision: u64,
    pub document_id: u64,
    pub document_uuid: u128,
    pub layer_id: u64,
    pub main_plane_id: u64,
    pub color_plane_id: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub frames: FrameMetadata,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub active_plane: ActivePlane,
    pub main_plane_checksum: u64,
    pub color_plane_checksum: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderTile {
    tile_id: u64,
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
    stride_bytes: u32,
    pixels: Arc<[u8]>,
    tile_revision: u64,
}

impl RenderTile {
    #[must_use]
    pub const fn tile_id(&self) -> u64 {
        self.tile_id
    }

    #[must_use]
    pub const fn origin_x(&self) -> i32 {
        self.origin_x
    }

    #[must_use]
    pub const fn origin_y(&self) -> i32 {
        self.origin_y
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn stride_bytes(&self) -> u32 {
        self.stride_bytes
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[must_use]
    pub const fn tile_revision(&self) -> u64 {
        self.tile_revision
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderSnapshot {
    revision: u64,
    view: ViewState,
    document_width: u32,
    document_height: u32,
    tiles: Vec<RenderTile>,
}

impl RenderSnapshot {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn view(&self) -> ViewState {
        self.view
    }

    #[must_use]
    pub const fn document_width(&self) -> u32 {
        self.document_width
    }

    #[must_use]
    pub const fn document_height(&self) -> u32 {
        self.document_height
    }

    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    #[must_use]
    pub fn tiles(&self) -> &[RenderTile] {
        &self.tiles
    }
}

#[derive(Clone, Debug)]
struct CellDocument {
    uuid: u128,
    id: u64,
    layer_id: u64,
    main_plane_id: u64,
    color_plane_id: u64,
    width: u32,
    height: u32,
    dpi_x_milli: u32,
    dpi_y_milli: u32,
    frames: FrameMetadata,
    main_plane: TileRaster,
    color_plane: TileRaster,
    active_plane: ActivePlane,
}

#[derive(Clone, Copy, Debug)]
struct DocumentIds {
    document: u64,
    layer: u64,
    main_plane: u64,
    color_plane: u64,
}

#[derive(Clone, Copy, Debug)]
struct PaperSpec {
    width: u32,
    height: u32,
    dpi_x_milli: u32,
    dpi_y_milli: u32,
}

impl CellDocument {
    fn new(ids: DocumentIds, uuid: u128, paper: PaperSpec) -> Result<Self, CoreError> {
        if paper.dpi_x_milli == 0 || paper.dpi_y_milli == 0 {
            return Err(CoreError::InvalidArgument("DPI must be nonzero"));
        }
        if uuid == 0 {
            return Err(CoreError::InvalidArgument("document UUID must be nonzero"));
        }
        let full = RectI32 {
            x: 0,
            y: 0,
            width: paper
                .width
                .try_into()
                .map_err(|_| CoreError::InvalidArgument("width exceeds frame range"))?,
            height: paper
                .height
                .try_into()
                .map_err(|_| CoreError::InvalidArgument("height exceeds frame range"))?,
        };
        let inset_x = (paper.width / 20) as i32;
        let inset_y = (paper.height / 20) as i32;
        let frames = FrameMetadata {
            hundred_frame: full,
            reference_frame: RectI32 {
                x: (paper.width / 2) as i32,
                y: (paper.height / 2) as i32,
                width: full.width,
                height: full.height,
            },
            drawing_frame: full,
            safe_frame: RectI32 {
                x: inset_x,
                y: inset_y,
                width: full.width - inset_x * 2,
                height: full.height - inset_y * 2,
            },
            margins: Margins::default(),
        };
        Ok(Self {
            uuid,
            id: ids.document,
            layer_id: ids.layer,
            main_plane_id: ids.main_plane,
            color_plane_id: ids.color_plane,
            width: paper.width,
            height: paper.height,
            dpi_x_milli: paper.dpi_x_milli,
            dpi_y_milli: paper.dpi_y_milli,
            frames,
            main_plane: TileRaster::new(paper.width, paper.height, PixelFormat::BinaryMask8)?,
            color_plane: TileRaster::new(paper.width, paper.height, PixelFormat::StraightRgba8)?,
            active_plane: ActivePlane::MainLine,
        })
    }

    fn to_file(&self) -> CellFile {
        CellFile {
            document_uuid: self.uuid.to_le_bytes(),
            document_id: self.id,
            layer_id: self.layer_id,
            main_plane_id: self.main_plane_id,
            color_plane_id: self.color_plane_id,
            width: self.width,
            height: self.height,
            dpi_x_milli: self.dpi_x_milli,
            dpi_y_milli: self.dpi_y_milli,
            frames: self.frames,
            planes: vec![
                raster_to_file_plane(self.main_plane_id, PlaneKind::MainLine, &self.main_plane),
                raster_to_file_plane(self.color_plane_id, PlaneKind::Color, &self.color_plane),
            ],
        }
    }

    fn from_file(file: CellFile, revision: u64) -> Result<Self, CoreError> {
        let main_file = file
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneKind::MainLine)
            .ok_or(CoreError::InvalidState("main line plane is missing"))?;
        let color_file = file
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneKind::Color)
            .ok_or(CoreError::InvalidState("color plane is missing"))?;
        let main_plane = file_plane_to_raster(main_file, revision)?;
        let color_plane = file_plane_to_raster(color_file, revision)?;
        Ok(Self {
            uuid: u128::from_le_bytes(file.document_uuid),
            id: file.document_id,
            layer_id: file.layer_id,
            main_plane_id: file.main_plane_id,
            color_plane_id: file.color_plane_id,
            width: file.width,
            height: file.height,
            dpi_x_milli: file.dpi_x_milli,
            dpi_y_milli: file.dpi_y_milli,
            frames: file.frames,
            main_plane,
            color_plane,
            active_plane: ActivePlane::MainLine,
        })
    }

    fn raster(&self, plane: ActivePlane) -> &TileRaster {
        match plane {
            ActivePlane::MainLine => &self.main_plane,
            ActivePlane::Color => &self.color_plane,
        }
    }

    fn raster_mut(&mut self, plane: ActivePlane) -> &mut TileRaster {
        match plane {
            ActivePlane::MainLine => &mut self.main_plane,
            ActivePlane::Color => &mut self.color_plane,
        }
    }
}

#[derive(Clone, Debug)]
struct PixelChange {
    x: u32,
    y: u32,
    before: PixelValue,
    after: PixelValue,
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    plane: ActivePlane,
    changes: Vec<PixelChange>,
    before_state: u64,
    after_state: u64,
}

#[derive(Clone, Debug)]
struct StrokeSession {
    stroke: Stroke,
    desired: PixelValue,
    preview_document: CellDocument,
    changes: BTreeMap<(u32, u32), PixelChange>,
    last_sample: Option<StrokeSample>,
    sample_count: usize,
    work: u64,
    preview_revision: u64,
}

type StagedPixels = BTreeMap<(u32, u32), PixelValue>;

/// Single-writer application core. Document and view revisions are independent.
#[derive(Debug)]
pub struct Core {
    document: Option<CellDocument>,
    document_revision: u64,
    view: ViewState,
    history: Vec<HistoryEntry>,
    history_cursor: usize,
    current_state: u64,
    next_state: u64,
    savepoint: Option<u64>,
    next_id: u64,
    current_path: Option<PathBuf>,
    active_stroke: Option<StrokeSession>,
    render_cache: BTreeMap<TileCoord, RenderTile>,
    next_preview_revision: u64,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            document: None,
            document_revision: 0,
            view: ViewState {
                zoom: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
                revision: 0,
                mode: ViewMode::Manual,
            },
            history: Vec::new(),
            history_cursor: 0,
            current_state: 0,
            next_state: 1,
            savepoint: None,
            next_id: 1,
            current_path: None,
            active_stroke: None,
            render_cache: BTreeMap::new(),
            next_preview_revision: 1_u64 << 63,
        }
    }

    #[must_use]
    pub fn dispatch(&mut self, commands: &[Command]) -> DispatchOutcome {
        DispatchOutcome {
            revision: self.document_revision,
            accepted_commands: commands.len() as u64,
        }
    }

    pub fn new_cell(
        &mut self,
        width: u32,
        height: u32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
    ) -> Result<DocumentInfo, CoreError> {
        let uuid = (u128::from(0x494e_4b50_4f44_4d31_u64) << 64) | u128::from(self.next_id);
        self.new_cell_with_uuid(width, height, dpi_x_milli, dpi_y_milli, uuid)
    }

    pub fn new_cell_with_uuid(
        &mut self,
        width: u32,
        height: u32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
        document_uuid: u128,
    ) -> Result<DocumentInfo, CoreError> {
        self.cancel_stroke();
        self.render_cache.clear();
        let ids = DocumentIds {
            document: self.allocate_id(),
            layer: self.allocate_id(),
            main_plane: self.allocate_id(),
            color_plane: self.allocate_id(),
        };
        let document = CellDocument::new(
            ids,
            document_uuid,
            PaperSpec {
                width,
                height,
                dpi_x_milli,
                dpi_y_milli,
            },
        )?;
        self.document = Some(document);
        self.document_revision = self.next_document_revision()?;
        self.reset_history(false);
        self.reset_view();
        self.current_path = None;
        self.document_info()
    }

    pub fn set_active_plane(&mut self, plane: ActivePlane) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        document.active_plane = plane;
        Ok(())
    }

    pub fn apply_stroke(&mut self, stroke: &Stroke) -> Result<DispatchOutcome, CoreError> {
        self.begin_stroke(stroke)?;
        match self.end_stroke() {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.cancel_stroke();
                Err(error)
            }
        }
    }

    pub fn begin_stroke(&mut self, stroke: &Stroke) -> Result<(), CoreError> {
        if self.active_stroke.is_some() {
            return Err(CoreError::InvalidState(
                "a stroke transaction is already active",
            ));
        }
        validate_stroke(stroke)?;
        let samples =
            document_samples_for_view(self.view, stroke.coordinate_space, &stroke.samples)?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let desired = stroke_value(stroke, &document, &samples)?;
        let preview_revision = self.allocate_preview_revision()?;
        let mut settings = stroke.clone();
        settings.samples.clear();
        let mut session = StrokeSession {
            stroke: settings,
            desired,
            preview_document: document,
            changes: BTreeMap::new(),
            last_sample: None,
            sample_count: 0,
            work: 0,
            preview_revision,
        };
        session.append_document_samples(&samples, preview_revision)?;
        self.active_stroke = Some(session);
        Ok(())
    }

    pub fn append_stroke(&mut self, samples: &[StrokeSample]) -> Result<(), CoreError> {
        if samples.is_empty() {
            return Err(CoreError::InvalidArgument(
                "stroke append contains no samples",
            ));
        }
        let mut session = self.active_stroke.take().ok_or(CoreError::InvalidState(
            "there is no active stroke transaction",
        ))?;
        let samples =
            document_samples_for_view(self.view, session.stroke.coordinate_space, samples)?;
        let preview_revision = self.allocate_preview_revision()?;
        match session.append_document_samples(&samples, preview_revision) {
            Ok(()) => {
                self.active_stroke = Some(session);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub fn end_stroke(&mut self) -> Result<DispatchOutcome, CoreError> {
        let session = self.active_stroke.take().ok_or(CoreError::InvalidState(
            "there is no active stroke transaction",
        ))?;
        if session.changes.is_empty() {
            return Ok(DispatchOutcome {
                revision: self.document_revision,
                accepted_commands: 1,
            });
        }

        let after_state = self.allocate_state()?;
        let revision = self.next_document_revision()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        document.active_plane = session.stroke.plane;
        let raster = document.raster_mut(session.stroke.plane);
        let mut changes = Vec::with_capacity(session.changes.len());
        let mut touched_tiles = BTreeSet::new();
        for ((x, y), change) in session.changes {
            raster.set_pixel(x, y, change.after, revision)?;
            touched_tiles.insert(TileCoord {
                x: x / TILE_SIZE,
                y: y / TILE_SIZE,
            });
            changes.push(change);
        }
        for coord in touched_tiles {
            raster.remove_tile_if_empty(coord);
        }
        self.document_revision = revision;
        self.commit_history(session.stroke.plane, changes, after_state);
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn cancel_stroke(&mut self) {
        self.active_stroke = None;
    }

    #[must_use]
    pub const fn stroke_is_active(&self) -> bool {
        self.active_stroke.is_some()
    }

    pub fn undo(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.history_cursor == 0 {
            return Err(CoreError::InvalidState("there is no command to undo"));
        }
        let revision = self.next_document_revision()?;
        let entry = self.history[self.history_cursor - 1].clone();
        self.apply_history_values(&entry, false, revision)?;
        self.history_cursor -= 1;
        self.current_state = entry.before_state;
        self.document_revision = revision;
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn redo(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let Some(entry) = self.history.get(self.history_cursor).cloned() else {
            return Err(CoreError::InvalidState("there is no command to redo"));
        };
        let revision = self.next_document_revision()?;
        self.apply_history_values(&entry, true, revision)?;
        self.history_cursor += 1;
        self.current_state = entry.after_state;
        self.document_revision = revision;
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn save(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        inkpod_format::save_atomic(path, &document.to_file())?;
        self.savepoint = Some(self.current_state);
        self.current_path = Some(path.to_path_buf());
        self.document_info()
    }

    pub fn open(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let file = inkpod_format::read(path)?;
        let revision = self.next_document_revision()?;
        let document = CellDocument::from_file(file, revision)?;
        let max_id = [
            document.id,
            document.layer_id,
            document.main_plane_id,
            document.color_plane_id,
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        self.next_id = self.next_id.max(max_id.saturating_add(1));
        self.document = Some(document);
        self.render_cache.clear();
        self.document_revision = revision;
        self.reset_history(true);
        self.reset_view();
        self.current_path = Some(path.to_path_buf());
        self.document_info()
    }

    pub fn revert(&mut self) -> Result<DocumentInfo, CoreError> {
        let path = self
            .current_path
            .clone()
            .ok_or(CoreError::InvalidState("document has no normal-save path"))?;
        self.open(&path)
    }

    pub fn apply_view(&mut self, command: ViewCommand) -> Result<ViewState, CoreError> {
        if self.active_stroke.is_some() {
            return Err(CoreError::InvalidState(
                "view cannot change during an active stroke transaction",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let (next_zoom, next_pan_x, next_pan_y, next_mode) = match command {
            ViewCommand::PanBy {
                device_dx,
                device_dy,
            } if device_dx.is_finite() && device_dy.is_finite() => (
                self.view.zoom,
                self.view.pan_x + device_dx,
                self.view.pan_y + device_dy,
                ViewMode::Manual,
            ),
            ViewCommand::ZoomAt {
                factor,
                device_x,
                device_y,
            } if factor.is_finite()
                && factor > 0.0
                && device_x.is_finite()
                && device_y.is_finite() =>
            {
                let zoom = (self.view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                let ratio = zoom / self.view.zoom;
                (
                    zoom,
                    device_x - (device_x - self.view.pan_x) * ratio,
                    device_y - (device_y - self.view.pan_y) * ratio,
                    ViewMode::Manual,
                )
            }
            ViewCommand::Fit {
                viewport_width,
                viewport_height,
            } if valid_viewport(viewport_width, viewport_height) => {
                let zoom = (viewport_width / f64::from(document.width))
                    .min(viewport_height / f64::from(document.height))
                    .mul_add(0.95, 0.0)
                    .clamp(MIN_ZOOM, MAX_ZOOM);
                (
                    zoom,
                    (viewport_width - f64::from(document.width) * zoom) / 2.0,
                    (viewport_height - f64::from(document.height) * zoom) / 2.0,
                    ViewMode::Fit,
                )
            }
            ViewCommand::OneToOne {
                viewport_width,
                viewport_height,
            } if valid_viewport(viewport_width, viewport_height) => (
                1.0,
                (viewport_width - f64::from(document.width)) / 2.0,
                (viewport_height - f64::from(document.height)) / 2.0,
                ViewMode::OneToOne,
            ),
            ViewCommand::ViewportResized {
                viewport_width,
                viewport_height,
            } if valid_viewport(viewport_width, viewport_height) => match self.view.mode {
                ViewMode::Manual => (
                    self.view.zoom,
                    self.view.pan_x,
                    self.view.pan_y,
                    ViewMode::Manual,
                ),
                ViewMode::Fit => {
                    let zoom = (viewport_width / f64::from(document.width))
                        .min(viewport_height / f64::from(document.height))
                        .mul_add(0.95, 0.0)
                        .clamp(MIN_ZOOM, MAX_ZOOM);
                    (
                        zoom,
                        (viewport_width - f64::from(document.width) * zoom) / 2.0,
                        (viewport_height - f64::from(document.height) * zoom) / 2.0,
                        ViewMode::Fit,
                    )
                }
                ViewMode::OneToOne => (
                    1.0,
                    (viewport_width - f64::from(document.width)) / 2.0,
                    (viewport_height - f64::from(document.height)) / 2.0,
                    ViewMode::OneToOne,
                ),
            },
            _ => {
                return Err(CoreError::InvalidArgument(
                    "view command contains invalid values",
                ));
            }
        };
        if !next_zoom.is_finite()
            || !view_translation_is_supported(next_pan_x)
            || !view_translation_is_supported(next_pan_y)
        {
            return Err(CoreError::InvalidArgument(
                "view command result is outside the finite supported range",
            ));
        }
        if next_zoom != self.view.zoom
            || next_pan_x != self.view.pan_x
            || next_pan_y != self.view.pan_y
            || next_mode != self.view.mode
        {
            self.view.revision = self
                .view
                .revision
                .checked_add(1)
                .ok_or(CoreError::InvalidState("view revision overflow"))?;
            self.view.zoom = next_zoom;
            self.view.pan_x = next_pan_x;
            self.view.pan_y = next_pan_y;
            self.view.mode = next_mode;
        }
        Ok(self.view)
    }

    pub fn document_info(&self) -> Result<DocumentInfo, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        Ok(DocumentInfo {
            document_revision: self.document_revision,
            view_revision: self.view.revision,
            document_id: document.id,
            document_uuid: document.uuid,
            layer_id: document.layer_id,
            main_plane_id: document.main_plane_id,
            color_plane_id: document.color_plane_id,
            width: document.width,
            height: document.height,
            dpi_x_milli: document.dpi_x_milli,
            dpi_y_milli: document.dpi_y_milli,
            frames: document.frames,
            dirty: self.savepoint != Some(self.current_state),
            can_undo: self.history_cursor > 0,
            can_redo: self.history_cursor < self.history.len(),
            active_plane: document.active_plane,
            main_plane_checksum: document.main_plane.checksum(),
            color_plane_checksum: document.color_plane.checksum(),
        })
    }

    pub fn plane_pixel(&self, plane: ActivePlane, x: u32, y: u32) -> Result<PixelValue, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .raster(plane)
            .pixel(x, y)?)
    }

    #[must_use]
    pub fn build_snapshot(&mut self) -> RenderSnapshot {
        let mut cache = std::mem::take(&mut self.render_cache);
        let Some(document) = self
            .active_stroke
            .as_ref()
            .map(|session| &session.preview_document)
            .or(self.document.as_ref())
        else {
            cache.clear();
            self.render_cache = cache;
            return RenderSnapshot {
                revision: self.document_revision,
                view: self.view,
                document_width: 0,
                document_height: 0,
                tiles: Vec::new(),
            };
        };
        let snapshot_revision = self
            .active_stroke
            .as_ref()
            .map_or(self.document_revision, |session| session.preview_revision);
        let coords: BTreeSet<_> = document
            .main_plane
            .allocated_coords()
            .chain(document.color_plane.allocated_coords())
            .collect();
        let mut tiles = Vec::with_capacity(coords.len());
        for coord in &coords {
            let source_revision = document
                .main_plane
                .tile_revision(*coord)
                .max(document.color_plane.tile_revision(*coord));
            if cache
                .get(coord)
                .is_none_or(|tile| tile.tile_revision != source_revision)
            {
                if let Some(tile) = compose_tile(document, *coord) {
                    cache.insert(*coord, tile);
                } else {
                    cache.remove(coord);
                }
            }
            if let Some(tile) = cache.get(coord) {
                tiles.push(tile.clone());
            }
        }
        cache.retain(|coord, _| coords.contains(coord));
        let document_width = document.width;
        let document_height = document.height;
        self.render_cache = cache;
        RenderSnapshot {
            revision: snapshot_revision,
            view: self.view,
            document_width,
            document_height,
            tiles,
        }
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1).max(1);
        id
    }

    fn next_document_revision(&self) -> Result<u64, CoreError> {
        self.document_revision
            .checked_add(1)
            .ok_or(CoreError::InvalidState("document revision overflow"))
    }

    fn allocate_preview_revision(&mut self) -> Result<u64, CoreError> {
        let revision = self.next_preview_revision;
        self.next_preview_revision = self
            .next_preview_revision
            .checked_add(1)
            .ok_or(CoreError::InvalidState("preview revision overflow"))?;
        Ok(revision)
    }

    fn ensure_no_active_stroke(&self) -> Result<(), CoreError> {
        if self.active_stroke.is_some() {
            Err(CoreError::InvalidState(
                "operation is not allowed during an active stroke transaction",
            ))
        } else {
            Ok(())
        }
    }

    fn allocate_state(&mut self) -> Result<u64, CoreError> {
        let state = self.next_state;
        self.next_state = self
            .next_state
            .checked_add(1)
            .ok_or(CoreError::InvalidState("history state overflow"))?;
        Ok(state)
    }

    fn reset_history(&mut self, saved: bool) {
        self.history.clear();
        self.history_cursor = 0;
        self.current_state = self.next_state;
        self.next_state = self.next_state.saturating_add(1);
        self.savepoint = saved.then_some(self.current_state);
    }

    fn reset_view(&mut self) {
        let revision = self.view.revision.saturating_add(1);
        self.view = ViewState {
            revision,
            ..ViewState::default()
        };
    }

    fn commit_history(&mut self, plane: ActivePlane, changes: Vec<PixelChange>, after_state: u64) {
        self.history.truncate(self.history_cursor);
        let before_state = self.current_state;
        self.history.push(HistoryEntry {
            plane,
            changes,
            before_state,
            after_state,
        });
        self.history_cursor = self.history.len();
        self.current_state = after_state;
    }

    fn apply_history_values(
        &mut self,
        entry: &HistoryEntry,
        use_after: bool,
        revision: u64,
    ) -> Result<(), CoreError> {
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        document.active_plane = entry.plane;
        let raster = document.raster_mut(entry.plane);
        let mut touched = BTreeSet::new();
        for change in &entry.changes {
            raster.set_pixel(
                change.x,
                change.y,
                if use_after {
                    change.after
                } else {
                    change.before
                },
                revision,
            )?;
            touched.insert(TileCoord {
                x: change.x / TILE_SIZE,
                y: change.y / TILE_SIZE,
            });
        }
        for coord in touched {
            raster.remove_tile_if_empty(coord);
        }
        Ok(())
    }
}

impl StrokeSession {
    fn append_document_samples(
        &mut self,
        samples: &[StrokeSample],
        preview_revision: u64,
    ) -> Result<(), CoreError> {
        let next_count = self
            .sample_count
            .checked_add(samples.len())
            .ok_or(CoreError::InvalidArgument("stroke sample count overflows"))?;
        if next_count > MAX_STROKE_SAMPLES {
            return Err(CoreError::InvalidArgument(
                "stroke sample count is outside bounds",
            ));
        }
        validate_stroke_samples(samples)?;

        let mut raster_samples =
            Vec::with_capacity(samples.len() + usize::from(self.last_sample.is_some()));
        if let Some(last) = self.last_sample {
            raster_samples.push(last);
        }
        raster_samples.extend_from_slice(samples);
        let mut incremental = self.stroke.clone();
        incremental.samples = raster_samples;
        let (staged, work) = stage_stroke_pixels_with_work(
            &self.preview_document,
            &incremental,
            &incremental.samples,
            self.desired,
            self.work,
        )?;

        let raster = self.preview_document.raster_mut(self.stroke.plane);
        let mut touched_tiles = BTreeSet::new();
        for ((x, y), after) in staged {
            let current = raster.pixel(x, y)?;
            if current == after {
                continue;
            }
            let before = self
                .changes
                .get(&(x, y))
                .map_or(current, |change| change.before);
            raster.set_pixel(x, y, after, preview_revision)?;
            let coord = TileCoord {
                x: x / TILE_SIZE,
                y: y / TILE_SIZE,
            };
            touched_tiles.insert(coord);
            if before == after {
                self.changes.remove(&(x, y));
            } else {
                self.changes.insert(
                    (x, y),
                    PixelChange {
                        x,
                        y,
                        before,
                        after,
                    },
                );
            }
        }
        for coord in touched_tiles {
            raster.remove_tile_if_empty(coord);
        }
        self.last_sample = samples.last().copied().or(self.last_sample);
        self.sample_count = next_count;
        self.work = work;
        self.preview_revision = preview_revision;
        Ok(())
    }
}

fn document_samples_for_view(
    view: ViewState,
    coordinate_space: CoordinateSpace,
    samples: &[StrokeSample],
) -> Result<Vec<StrokeSample>, CoreError> {
    validate_stroke_samples(samples)?;
    match coordinate_space {
        CoordinateSpace::Document => Ok(samples.to_vec()),
        CoordinateSpace::Device => {
            if view.zoom <= 0.0 {
                return Err(CoreError::InvalidState("view zoom is invalid"));
            }
            samples
                .iter()
                .map(|sample| {
                    let x = (f64::from(sample.x) - view.pan_x) / view.zoom;
                    let y = (f64::from(sample.y) - view.pan_y) / view.zoom;
                    if !stroke_coordinate_is_supported(x) || !stroke_coordinate_is_supported(y) {
                        return Err(CoreError::InvalidArgument(
                            "device-to-document stroke coordinate is outside bounds",
                        ));
                    }
                    Ok(StrokeSample {
                        x: x as f32,
                        y: y as f32,
                        pressure: sample.pressure,
                    })
                })
                .collect()
        }
    }
}

fn raster_to_file_plane(id: u64, kind: PlaneKind, raster: &TileRaster) -> FilePlane {
    let tiles = raster
        .allocated_coords()
        .filter_map(|coord| raster.tile_data(coord))
        .map(|tile| FileTile {
            coord: tile.coord,
            width: tile.width,
            height: tile.height,
            bytes: tile.bytes,
        })
        .collect();
    FilePlane {
        id,
        kind,
        pixel_format: raster.format(),
        width: raster.width(),
        height: raster.height(),
        tiles,
    }
}

fn file_plane_to_raster(plane: &FilePlane, revision: u64) -> Result<TileRaster, CoreError> {
    let mut raster = TileRaster::new(plane.width, plane.height, plane.pixel_format)?;
    for tile in &plane.tiles {
        raster.insert_tile(TileData {
            coord: tile.coord,
            width: tile.width,
            height: tile.height,
            bytes: tile.bytes.clone(),
            revision,
        })?;
    }
    Ok(raster)
}

fn validate_stroke(stroke: &Stroke) -> Result<(), CoreError> {
    if stroke.samples.is_empty() || stroke.samples.len() > MAX_STROKE_SAMPLES {
        return Err(CoreError::InvalidArgument(
            "stroke sample count is outside bounds",
        ));
    }
    if !stroke.diameter.is_finite()
        || stroke.diameter <= 0.0
        || stroke.diameter > MAX_BRUSH_DIAMETER
    {
        return Err(CoreError::InvalidArgument(
            "stroke diameter is outside bounds",
        ));
    }
    validate_stroke_samples(&stroke.samples)
}

fn validate_stroke_samples(samples: &[StrokeSample]) -> Result<(), CoreError> {
    if samples.iter().any(|sample| {
        !sample.x.is_finite()
            || !sample.y.is_finite()
            || sample.x.abs() > MAX_STROKE_COORDINATE
            || sample.y.abs() > MAX_STROKE_COORDINATE
            || !sample.pressure.is_finite()
            || !(0.0..=1.0).contains(&sample.pressure)
    }) {
        return Err(CoreError::InvalidArgument(
            "stroke sample contains invalid values",
        ));
    }
    Ok(())
}

fn stroke_value(
    stroke: &Stroke,
    document: &CellDocument,
    samples: &[StrokeSample],
) -> Result<PixelValue, CoreError> {
    let draw_value = match stroke.plane {
        ActivePlane::MainLine => PixelValue::Binary(255),
        ActivePlane::Color => PixelValue::Rgba(stroke.color),
    };
    let erase_value = match stroke.plane {
        ActivePlane::MainLine => PixelValue::Binary(0),
        ActivePlane::Color => PixelValue::Rgba([0; 4]),
    };
    if stroke.tool == PaintTool::Eraser {
        return Ok(erase_value);
    }
    if stroke.tool == PaintTool::Pencil && stroke.auto_erase {
        let first = samples[0];
        let x = first.x.round() as i64;
        let y = first.y.round() as i64;
        if x >= 0
            && y >= 0
            && x < i64::from(document.width)
            && y < i64::from(document.height)
            && document.raster(stroke.plane).pixel(x as u32, y as u32)? == draw_value
        {
            return Ok(erase_value);
        }
    }
    Ok(draw_value)
}

fn stage_stroke_pixels_with_work(
    document: &CellDocument,
    stroke: &Stroke,
    samples: &[StrokeSample],
    value: PixelValue,
    initial_work: u64,
) -> Result<(StagedPixels, u64), CoreError> {
    let mut stager = StrokeStager {
        document,
        stroke,
        value,
        maximum_radius: stroke_maximum_radius(stroke),
        work: initial_work,
        staged: BTreeMap::new(),
    };
    let mut previous = samples[0];
    stager.stage_segment(previous, previous)?;
    for sample in &samples[1..] {
        stager.stage_segment(previous, *sample)?;
        previous = *sample;
    }
    Ok((stager.staged, stager.work))
}

struct StrokeStager<'a> {
    document: &'a CellDocument,
    stroke: &'a Stroke,
    value: PixelValue,
    maximum_radius: i64,
    work: u64,
    staged: BTreeMap<(u32, u32), PixelValue>,
}

impl StrokeStager<'_> {
    fn stage_segment(&mut self, start: StrokeSample, end: StrokeSample) -> Result<(), CoreError> {
        let Some((start, end)) =
            clip_segment_to_document(self.document, start, end, self.maximum_radius)
        else {
            return Ok(());
        };
        let mut x0 = start.x.round() as i64;
        let mut y0 = start.y.round() as i64;
        let x1 = end.x.round() as i64;
        let y1 = end.y.round() as i64;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        let steps = dx.max(-dy).max(1);
        let mut step = 0_i64;
        loop {
            let interpolation = step as f32 / steps as f32;
            let pressure = start.pressure + (end.pressure - start.pressure) * interpolation;
            self.stage_dab(x0, y0, pressure)?;
            if x0 == x1 && y0 == y1 {
                break;
            }
            let double_error = error * 2;
            if double_error >= dy {
                error += dy;
                x0 += sx;
            }
            if double_error <= dx {
                error += dx;
                y0 += sy;
            }
            step += 1;
        }
        Ok(())
    }

    fn stage_dab(&mut self, center_x: i64, center_y: i64, pressure: f32) -> Result<(), CoreError> {
        let radius = if self.stroke.tool == PaintTool::Pencil {
            0
        } else {
            let scale = if self.stroke.pressure_size {
                pressure.max(0.01)
            } else {
                1.0
            };
            ((self.stroke.diameter * scale - 1.0) / 2.0).ceil().max(0.0) as i64
        };
        let diameter = u64::try_from(radius * 2 + 1)
            .map_err(|_| CoreError::InvalidArgument("stroke radius is not representable"))?;
        let dab_work = diameter
            .checked_mul(diameter)
            .ok_or(CoreError::InvalidArgument(
                "stroke rasterization work overflows",
            ))?;
        self.work = self
            .work
            .checked_add(dab_work)
            .ok_or(CoreError::InvalidArgument(
                "stroke rasterization work overflows",
            ))?;
        if self.work > MAX_STROKE_WORK {
            return Err(CoreError::InvalidArgument(
                "stroke rasterization work exceeds the bounded limit",
            ));
        }
        let radius_squared = radius * radius;
        for offset_y in -radius..=radius {
            for offset_x in -radius..=radius {
                if offset_x * offset_x + offset_y * offset_y > radius_squared {
                    continue;
                }
                let x = center_x + offset_x;
                let y = center_y + offset_y;
                if x >= 0
                    && y >= 0
                    && x < i64::from(self.document.width)
                    && y < i64::from(self.document.height)
                {
                    self.staged.insert((x as u32, y as u32), self.value);
                }
            }
        }
        Ok(())
    }
}

fn stroke_maximum_radius(stroke: &Stroke) -> i64 {
    if stroke.tool == PaintTool::Pencil {
        return 0;
    }
    let pressure = if stroke.pressure_size {
        stroke
            .samples
            .iter()
            .map(|sample| sample.pressure)
            .fold(0.01_f32, f32::max)
    } else {
        1.0
    };
    ((stroke.diameter * pressure - 1.0) / 2.0).ceil().max(0.0) as i64
}

fn clip_segment_to_document(
    document: &CellDocument,
    start: StrokeSample,
    end: StrokeSample,
    radius: i64,
) -> Option<(StrokeSample, StrokeSample)> {
    let start_x = f64::from(start.x);
    let start_y = f64::from(start.y);
    let delta_x = f64::from(end.x) - start_x;
    let delta_y = f64::from(end.y) - start_y;
    let radius = radius as f64;
    let minimum_x = -radius;
    let minimum_y = -radius;
    let maximum_x = f64::from(document.width - 1) + radius;
    let maximum_y = f64::from(document.height - 1) + radius;
    let mut lower = 0.0_f64;
    let mut upper = 1.0_f64;

    for (coefficient, distance) in [
        (-delta_x, start_x - minimum_x),
        (delta_x, maximum_x - start_x),
        (-delta_y, start_y - minimum_y),
        (delta_y, maximum_y - start_y),
    ] {
        if coefficient == 0.0 {
            if distance < 0.0 {
                return None;
            }
            continue;
        }
        let ratio = distance / coefficient;
        if coefficient < 0.0 {
            if ratio > upper {
                return None;
            }
            lower = lower.max(ratio);
        } else {
            if ratio < lower {
                return None;
            }
            upper = upper.min(ratio);
        }
    }

    let interpolate = |ratio: f64| StrokeSample {
        x: (start_x + delta_x * ratio) as f32,
        y: (start_y + delta_y * ratio) as f32,
        pressure: start.pressure + (end.pressure - start.pressure) * ratio as f32,
    };
    Some((interpolate(lower), interpolate(upper)))
}

fn compose_tile(document: &CellDocument, coord: TileCoord) -> Option<RenderTile> {
    let origin_x = coord.x.checked_mul(TILE_SIZE)?;
    let origin_y = coord.y.checked_mul(TILE_SIZE)?;
    if origin_x >= document.width || origin_y >= document.height {
        return None;
    }
    let width = TILE_SIZE.min(document.width - origin_x);
    let height = TILE_SIZE.min(document.height - origin_y);
    let stride = width.checked_mul(4)?;
    let capacity = usize::try_from(stride.checked_mul(height)?).ok()?;
    let mut pixels = Vec::with_capacity(capacity);
    for y in 0..height {
        for x in 0..width {
            let color = match document
                .color_plane
                .pixel(origin_x + x, origin_y + y)
                .ok()?
            {
                PixelValue::Rgba(value) => value,
                PixelValue::Binary(_) => return None,
            };
            let line = match document.main_plane.pixel(origin_x + x, origin_y + y).ok()? {
                PixelValue::Binary(value) => value,
                PixelValue::Rgba(_) => return None,
            };
            let inverse_line = 255_u32 - u32::from(line);
            let color_alpha = u32::from(color[3]);
            let output_alpha = u32::from(line) + (color_alpha * inverse_line + 127) / 255;
            let premultiply = |channel: u8| -> u8 {
                let color_premultiplied = (u32::from(channel) * color_alpha + 127) / 255;
                ((color_premultiplied * inverse_line + 127) / 255) as u8
            };
            pixels.extend_from_slice(&[
                premultiply(color[2]),
                premultiply(color[1]),
                premultiply(color[0]),
                output_alpha as u8,
            ]);
        }
    }
    if pixels.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        return None;
    }
    Some(RenderTile {
        tile_id: (u64::from(coord.y) << 32) | u64::from(coord.x) | (1_u64 << 63),
        origin_x: origin_x as i32,
        origin_y: origin_y as i32,
        width,
        height,
        stride_bytes: stride,
        pixels: Arc::from(pixels),
        tile_revision: document
            .main_plane
            .tile_revision(coord)
            .max(document.color_plane.tile_revision(coord)),
    })
}

fn valid_viewport(width: f64, height: f64) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}

fn view_translation_is_supported(value: f64) -> bool {
    value.is_finite() && value.abs() <= f64::from(MAX_STROKE_COORDINATE)
}

fn stroke_coordinate_is_supported(value: f64) -> bool {
    value.is_finite() && value.abs() <= f64::from(MAX_STROKE_COORDINATE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_PATH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn line_stroke(samples: Vec<StrokeSample>) -> Stroke {
        Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::MainLine,
            color: [0, 0, 0, 255],
            diameter: 1.0,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples,
        }
    }

    fn color_stroke(tool: PaintTool, diameter: f32, sample: StrokeSample) -> Stroke {
        Stroke {
            tool,
            plane: ActivePlane::Color,
            color: [12, 34, 56, 255],
            diameter,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples: vec![sample],
        }
    }

    #[test]
    fn m0_empty_snapshot_remains_stable() {
        let mut core = Core::new();
        let first = core.build_snapshot();
        let second = core.build_snapshot();
        assert_eq!(first, second);
        assert_eq!(first.revision(), 0);
        assert_eq!(first.tile_count(), 0);
    }

    #[test]
    fn m0_noop_batch_does_not_change_document_revision() {
        let mut core = Core::new();
        let outcome = core.dispatch(&[Command::NoOp, Command::NoOp]);
        assert_eq!(outcome.accepted_commands(), 2);
        assert_eq!(outcome.revision(), 0);
        assert_eq!(core.build_snapshot().revision(), 0);
    }

    #[test]
    fn m1_acceptance_saved_drawing_vertical_slice() {
        let mut core = Core::new();
        let created = core
            .new_cell(1920, 1080, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        assert!(created.dirty);

        let samples: Vec<_> = (0..128)
            .map(|index| StrokeSample {
                x: 100.0 + index as f32,
                y: 100.0 + (index / 4) as f32,
                pressure: 0.5,
            })
            .collect();
        core.apply_stroke(&line_stroke(samples)).unwrap();
        let line_checksum = core.document_info().unwrap().main_plane_checksum;
        assert_ne!(line_checksum, created.main_plane_checksum);

        core.set_active_plane(ActivePlane::Color).unwrap();
        let mut color_stroke = line_stroke(vec![
            StrokeSample {
                x: 120.0,
                y: 140.0,
                pressure: 1.0,
            },
            StrokeSample {
                x: 220.0,
                y: 160.0,
                pressure: 1.0,
            },
        ]);
        color_stroke.plane = ActivePlane::Color;
        color_stroke.color = [220, 40, 30, 255];
        core.apply_stroke(&color_stroke).unwrap();
        let after_color = core.document_info().unwrap();
        assert_eq!(after_color.main_plane_checksum, line_checksum);
        assert_ne!(
            after_color.color_plane_checksum,
            created.color_plane_checksum
        );

        let colored_pixel = core.plane_pixel(ActivePlane::Color, 150, 146).unwrap();
        core.undo().unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 150, 146).unwrap(),
            PixelValue::Rgba([0; 4])
        );
        core.redo().unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 150, 146).unwrap(),
            colored_pixel
        );

        let revision_before_view = core.document_info().unwrap().document_revision;
        core.apply_view(ViewCommand::PanBy {
            device_dx: 10.0,
            device_dy: -5.0,
        })
        .unwrap();
        core.apply_view(ViewCommand::ZoomAt {
            factor: 2.0,
            device_x: 320.0,
            device_y: 240.0,
        })
        .unwrap();
        let after_view = core.document_info().unwrap();
        assert_eq!(after_view.document_revision, revision_before_view);
        assert!(after_view.view_revision > after_color.view_revision);

        let path = std::env::temp_dir().join(format!(
            "inkpod-core-m1-{}-{}.inkpod",
            std::process::id(),
            TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let saved = core.save(&path).unwrap();
        assert!(!saved.dirty);
        let expected_snapshot = core.build_snapshot();
        drop(core);

        let mut reopened_core = Core::new();
        let reopened = reopened_core.open(&path).unwrap();
        assert_eq!(reopened.document_id, saved.document_id);
        assert_eq!(reopened.document_uuid, saved.document_uuid);
        assert_eq!(reopened.layer_id, saved.layer_id);
        assert_eq!(reopened.main_plane_id, saved.main_plane_id);
        assert_eq!(reopened.color_plane_id, saved.color_plane_id);
        assert_eq!(reopened.frames, saved.frames);
        assert_eq!(reopened.main_plane_checksum, saved.main_plane_checksum);
        assert_eq!(reopened.color_plane_checksum, saved.color_plane_checksum);
        assert_eq!(
            reopened_core.build_snapshot().tiles().len(),
            expected_snapshot.tiles().len()
        );
        assert!(!reopened.dirty);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn paint_001_brush_eraser_auto_erase_and_pressure_are_transactional() {
        let mut core = Core::new();
        core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();

        let center = StrokeSample {
            x: 20.0,
            y: 20.0,
            pressure: 1.0,
        };
        let mut brush = color_stroke(PaintTool::Brush, 9.0, center);
        brush.pressure_size = true;
        core.apply_stroke(&brush).unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 24, 20).unwrap(),
            PixelValue::Rgba([12, 34, 56, 255])
        );

        let eraser = color_stroke(PaintTool::Eraser, 9.0, center);
        core.apply_stroke(&eraser).unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 24, 20).unwrap(),
            PixelValue::Rgba([0; 4])
        );
        core.undo().unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 24, 20).unwrap(),
            PixelValue::Rgba([12, 34, 56, 255])
        );

        core.undo().unwrap();
        brush.samples[0].pressure = 0.0;
        core.apply_stroke(&brush).unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 21, 20).unwrap(),
            PixelValue::Rgba([0; 4])
        );

        let point = StrokeSample {
            x: 5.0,
            y: 6.0,
            pressure: 1.0,
        };
        core.apply_stroke(&line_stroke(vec![point])).unwrap();
        let mut auto_erase = line_stroke(vec![point]);
        auto_erase.auto_erase = true;
        core.apply_stroke(&auto_erase).unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 5, 6).unwrap(),
            PixelValue::Binary(0)
        );
        core.undo().unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 5, 6).unwrap(),
            PixelValue::Binary(255)
        );
    }

    #[test]
    fn abi_002_snapshot_composites_visible_main_line_over_color() {
        let mut core = Core::new();
        core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_stroke(&line_stroke(vec![StrokeSample {
            x: 10.0,
            y: 10.0,
            pressure: 1.0,
        }]))
        .unwrap();
        let main_checksum = core.document_info().unwrap().main_plane_checksum;

        let mut color = color_stroke(
            PaintTool::Pencil,
            1.0,
            StrokeSample {
                x: 10.0,
                y: 10.0,
                pressure: 1.0,
            },
        );
        color.samples.push(StrokeSample {
            x: 20.0,
            y: 10.0,
            pressure: 1.0,
        });
        core.apply_stroke(&color).unwrap();
        assert_eq!(
            core.document_info().unwrap().main_plane_checksum,
            main_checksum
        );

        let snapshot = core.build_snapshot();
        assert_eq!(snapshot.tile_count(), 1);
        let tile = &snapshot.tiles()[0];
        let pixel = |x: usize, y: usize| {
            let offset = y * tile.stride_bytes() as usize + x * 4;
            &tile.pixels()[offset..offset + 4]
        };
        assert_eq!(pixel(10, 10), [0, 0, 0, 255]);
        assert_eq!(pixel(20, 10), [56, 34, 12, 255]);
    }

    #[test]
    fn invalid_view_and_excessive_stroke_work_do_not_commit_partial_state() {
        let mut core = Core::new();
        let created = core
            .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        assert!(matches!(
            core.apply_view(ViewCommand::PanBy {
                device_dx: f64::MAX,
                device_dy: 0.0,
            }),
            Err(CoreError::InvalidArgument(_))
        ));
        let after_view = core.document_info().unwrap();
        assert_eq!(after_view.document_revision, created.document_revision);
        assert_eq!(after_view.view_revision, created.view_revision);

        let extreme = line_stroke(vec![StrokeSample {
            x: f32::MAX,
            y: 0.0,
            pressure: 1.0,
        }]);
        assert!(matches!(
            core.apply_stroke(&extreme),
            Err(CoreError::InvalidArgument(_))
        ));

        let mut excessive = color_stroke(
            PaintTool::Brush,
            MAX_BRUSH_DIAMETER,
            StrokeSample {
                x: 32.0,
                y: 32.0,
                pressure: 1.0,
            },
        );
        excessive.samples = vec![excessive.samples[0]; 300];
        assert!(matches!(
            core.apply_stroke(&excessive),
            Err(CoreError::InvalidArgument(_))
        ));
        let after_strokes = core.document_info().unwrap();
        assert_eq!(after_strokes.document_revision, created.document_revision);
        assert_eq!(
            after_strokes.color_plane_checksum,
            created.color_plane_checksum
        );
        assert!(!after_strokes.can_undo);
    }

    #[test]
    fn off_canvas_segment_is_clipped_before_rasterization_work_is_counted() {
        let mut core = Core::new();
        core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_stroke(&line_stroke(vec![
            StrokeSample {
                x: -10_000_000.0,
                y: 32.0,
                pressure: 1.0,
            },
            StrokeSample {
                x: 10_000_000.0,
                y: 32.0,
                pressure: 1.0,
            },
        ]))
        .unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 0, 32).unwrap(),
            PixelValue::Binary(255)
        );
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 63, 32).unwrap(),
            PixelValue::Binary(255)
        );
    }

    #[test]
    fn hist_001_redo_branch_is_discarded_and_savepoint_tracks_undo() {
        let mut core = Core::new();
        core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_stroke(&line_stroke(vec![StrokeSample {
            x: 1.0,
            y: 1.0,
            pressure: 1.0,
        }]))
        .unwrap();
        core.undo().unwrap();
        core.apply_stroke(&line_stroke(vec![StrokeSample {
            x: 2.0,
            y: 2.0,
            pressure: 1.0,
        }]))
        .unwrap();
        assert!(!core.document_info().unwrap().can_redo);
    }

    #[test]
    fn hist_001_savepoint_undo_redo_and_revert_are_distinct() {
        let path = std::env::temp_dir().join(format!(
            "inkpod-core-savepoint-{}-{}.inkpod",
            std::process::id(),
            TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut core = Core::new();
        core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_stroke(&line_stroke(vec![StrokeSample {
            x: 1.0,
            y: 1.0,
            pressure: 1.0,
        }]))
        .unwrap();
        assert!(!core.save(&path).unwrap().dirty);
        core.apply_stroke(&line_stroke(vec![StrokeSample {
            x: 2.0,
            y: 2.0,
            pressure: 1.0,
        }]))
        .unwrap();
        assert!(core.document_info().unwrap().dirty);
        core.undo().unwrap();
        assert!(!core.document_info().unwrap().dirty);
        core.redo().unwrap();
        assert!(core.document_info().unwrap().dirty);
        core.revert().unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 2, 2).unwrap(),
            PixelValue::Binary(0)
        );
        assert!(!core.document_info().unwrap().dirty);
        fs::remove_file(&path).unwrap();
        assert!(!path.exists());
        assert!(!core.save(&path).unwrap().dirty);
        assert!(path.exists());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn live_stroke_preview_is_visible_before_one_atomic_commit() {
        let mut core = Core::new();
        let created = core
            .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let first = line_stroke(vec![StrokeSample {
            x: 8.0,
            y: 8.0,
            pressure: 1.0,
        }]);
        core.begin_stroke(&first).unwrap();
        let during_begin = core.document_info().unwrap();
        assert_eq!(during_begin.document_revision, created.document_revision);
        assert_eq!(
            during_begin.main_plane_checksum,
            created.main_plane_checksum
        );
        assert_eq!(during_begin.dirty, created.dirty);
        assert!(!during_begin.can_undo);
        assert_eq!(core.build_snapshot().tile_count(), 1);
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 8, 8).unwrap(),
            PixelValue::Binary(0)
        );

        core.append_stroke(&[StrokeSample {
            x: 24.0,
            y: 8.0,
            pressure: 1.0,
        }])
        .unwrap();
        let preview = core.build_snapshot();
        assert!(preview.revision() >= 1_u64 << 63);
        assert_eq!(core.document_info().unwrap(), during_begin);

        core.end_stroke().unwrap();
        let committed = core.document_info().unwrap();
        assert_eq!(committed.document_revision, created.document_revision + 1);
        assert!(committed.dirty && committed.can_undo);
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 24, 8).unwrap(),
            PixelValue::Binary(255)
        );
        core.undo().unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 8, 8).unwrap(),
            PixelValue::Binary(0)
        );
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 24, 8).unwrap(),
            PixelValue::Binary(0)
        );
    }

    #[test]
    fn cancelling_live_stroke_restores_base_snapshot_without_revision_change() {
        let mut core = Core::new();
        let created = core
            .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.begin_stroke(&line_stroke(vec![StrokeSample {
            x: 12.0,
            y: 12.0,
            pressure: 1.0,
        }]))
        .unwrap();
        assert_eq!(core.build_snapshot().tile_count(), 1);
        core.cancel_stroke();
        assert_eq!(core.build_snapshot().tile_count(), 0);
        assert_eq!(core.document_info().unwrap(), created);
    }

    #[test]
    fn failed_live_append_discards_preview_without_partial_commit() {
        let mut core = Core::new();
        let created = core
            .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let first = StrokeSample {
            x: 32.0,
            y: 32.0,
            pressure: 1.0,
        };
        core.begin_stroke(&color_stroke(PaintTool::Brush, MAX_BRUSH_DIAMETER, first))
            .unwrap();
        let excessive = vec![first; 300];
        assert!(matches!(
            core.append_stroke(&excessive),
            Err(CoreError::InvalidArgument(_))
        ));
        assert!(!core.stroke_is_active());
        assert_eq!(core.build_snapshot().tile_count(), 0);
        assert_eq!(core.document_info().unwrap(), created);
    }

    #[test]
    fn viewport_resize_refits_only_persistent_fit_or_one_to_one_modes() {
        let mut core = Core::new();
        core.new_cell(200, 100, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let fit = core
            .apply_view(ViewCommand::Fit {
                viewport_width: 400.0,
                viewport_height: 300.0,
            })
            .unwrap();
        assert_eq!(fit.mode(), ViewMode::Fit);
        let resized = core
            .apply_view(ViewCommand::ViewportResized {
                viewport_width: 800.0,
                viewport_height: 600.0,
            })
            .unwrap();
        assert_eq!(resized.mode(), ViewMode::Fit);
        assert!(resized.zoom() > fit.zoom());

        core.apply_view(ViewCommand::PanBy {
            device_dx: 10.0,
            device_dy: 5.0,
        })
        .unwrap();
        let manual = core
            .apply_view(ViewCommand::ViewportResized {
                viewport_width: 640.0,
                viewport_height: 480.0,
            })
            .unwrap();
        assert_eq!(manual.mode(), ViewMode::Manual);
        let repeated = core
            .apply_view(ViewCommand::ViewportResized {
                viewport_width: 320.0,
                viewport_height: 240.0,
            })
            .unwrap();
        assert_eq!(repeated, manual);
    }
}
