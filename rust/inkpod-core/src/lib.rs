#![forbid(unsafe_code)]

mod m4;
mod m5;

pub use m4::{
    LightTableDisplayMode, LightTableItemInfo, LightTableItemInput, LightTableSetInfo,
    LightTableSource, MotionCheckConfig, MotionFrame, RgbaRasterBytes, SequenceCellInfo,
    SequenceCellSource, SequenceDirection, Thumbnail,
};
pub use m5::{
    RenderVectorFill, RenderVectorSegment, VectorCubicSegment, VectorEraseMode, VectorFillInfo,
    VectorPathInfo, VectorPathInput, VectorRaster, VectorSelectionMode, VectorSelectionRange,
    VectorSelectionResult, VectorWidthMode,
};

use inkpod_format::{
    CellFile, CommonRaster, CommonRasterFormat, FileGrid, FileGuide, FileLayer, FileM3Metadata,
    FilePlane, FilePlaneProperties, FileTile, FormatError, PlaneKind as FilePlaneKind,
};
use inkpod_image::{
    ColorCheckCategory, FillError, FillOptions, MAX_FILL_PIXELS, Palette, PlaneSample, RasterError,
    TILE_SIZE, TileCoord, TileData, TileRaster, closed_region_fill_with_cancel,
    color_check_category, extend_fill_with_cancel, eyedropper, seed_fill_with_cancel,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const CORE_FEATURES: u64 = 1;
pub const SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE: u64 = 1 << 0;
pub const SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA: u64 = 1 << 1;
pub const DEFAULT_DPI_MILLI: u32 = 96_000;
pub const MAX_LAYERS: usize = 4_096;
pub const MAX_PLANES_PER_LAYER: usize = 4_096;
pub const MAX_GUIDES: usize = 4_096;
pub const MAX_SHORTCUTS: usize = 1_024;
pub const SHORTCUT_MODIFIER_CONTROL: u32 = 1 << 0;
pub const SHORTCUT_MODIFIER_SHIFT: u32 = 1 << 1;
pub const SHORTCUT_MODIFIER_ALT: u32 = 1 << 2;
pub const SHORTCUT_MODIFIER_EXTENDED: u32 = 1 << 3;
pub const SHORTCUT_MODIFIER_MASK: u32 = SHORTCUT_MODIFIER_CONTROL
    | SHORTCUT_MODIFIER_SHIFT
    | SHORTCUT_MODIFIER_ALT
    | SHORTCUT_MODIFIER_EXTENDED;
const MAX_STROKE_SAMPLES: usize = 1_048_576;
const MAX_BRUSH_DIAMETER: f32 = 256.0;
const MAX_STROKE_COORDINATE: f32 = 16_777_216.0;
const MAX_STROKE_WORK: u64 = 16_777_216;
const MIN_ZOOM: f64 = 0.01;
const MAX_ZOOM: f64 = 64.0;

pub use inkpod_format::{
    FrameMetadata, GuideAxis, LayerKind, MAX_COMMON_RASTER_BYTES, Margins, RectI32,
};
pub use inkpod_image::{
    ColorCheckMode, EyedropperSource, InclusionMode, MAX_RASTER_DIMENSION, PixelFormat, PixelValue,
};

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
pub enum PlaneType {
    MainLine,
    Color,
    Raster,
    Selection,
    VectorMainLine,
    ColorTrace,
    VectorFill,
}

impl PlaneType {
    const fn file_kind(self) -> FilePlaneKind {
        match self {
            Self::MainLine => FilePlaneKind::MainLine,
            Self::Color => FilePlaneKind::Color,
            Self::Raster => FilePlaneKind::Raster,
            Self::Selection => FilePlaneKind::Selection,
            Self::VectorMainLine => FilePlaneKind::VectorMainLine,
            Self::ColorTrace => FilePlaneKind::ColorTrace,
            Self::VectorFill => FilePlaneKind::VectorFill,
        }
    }

    const fn from_file(kind: FilePlaneKind) -> Self {
        match kind {
            FilePlaneKind::MainLine => Self::MainLine,
            FilePlaneKind::Color => Self::Color,
            FilePlaneKind::Raster => Self::Raster,
            FilePlaneKind::Selection => Self::Selection,
            FilePlaneKind::LightTable => Self::Raster,
            FilePlaneKind::VectorMainLine => Self::VectorMainLine,
            FilePlaneKind::ColorTrace => Self::ColorTrace,
            FilePlaneKind::VectorFill => Self::VectorFill,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaneInfo {
    pub id: u64,
    pub kind: PlaneType,
    pub pixel_format: PixelFormat,
    pub name: String,
    pub visible: bool,
    pub editable: bool,
    pub opacity_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerInfo {
    pub id: u64,
    pub kind: LayerKind,
    pub name: String,
    pub visible: bool,
    pub editable: bool,
    pub opacity_milli: u32,
    pub planes: Vec<PlaneInfo>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionOperation {
    New,
    Add,
    Subtract,
    Intersect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointF32 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SelectionShape {
    Rectangle(RectI32),
    Ellipse(RectI32),
    Lasso(Vec<PointF32>),
    Polyline(Vec<PointF32>),
    Trace {
        points: Vec<PointF32>,
        diameter: f32,
    },
    Wand {
        x: u32,
        y: u32,
        tolerance: u16,
        gap_close: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionLayerOperation {
    Replace,
    Add,
    Subtract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirrorAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Guide {
    pub id: u64,
    pub axis: GuideAxis,
    pub position: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridConfig {
    pub origin_x: i32,
    pub origin_y: i32,
    pub spacing_x: u32,
    pub spacing_y: u32,
    pub subdivisions: u32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            origin_x: 0,
            origin_y: 0,
            spacing_x: 16,
            spacing_y: 16,
            subdivisions: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatingTransform {
    pub translate_x: f64,
    pub translate_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation_degrees: f64,
}

impl Default for FloatingTransform {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardPixel {
    pub x: i32,
    pub y: i32,
    pub value: PixelValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardPlane {
    pub kind: PlaneType,
    pub pixel_format: PixelFormat,
    pub origin_x: i32,
    pub origin_y: i32,
    pub pixels: Vec<ClipboardPixel>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardPayload {
    pub source_document_uuid: u128,
    pub bounds: RectI32,
    pub planes: Vec<ClipboardPlane>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocatorSample {
    pub document_x: i32,
    pub document_y: i32,
    pub selection_bounds: Option<RectI32>,
    pub color: Option<PixelValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShortcutBinding {
    pub command_id: u32,
    pub virtual_key: u32,
    pub modifiers: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaintTool {
    Pencil,
    Brush,
    Eraser,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FillOperation {
    Seed,
    ClosedRegion,
    Extend,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FillRequest {
    pub operation: FillOperation,
    pub seed_x: u32,
    pub seed_y: u32,
    pub color: PixelValue,
    pub selection: Option<RectI32>,
    pub tolerance: u16,
    pub detached_regions: bool,
    pub overflow_abort: bool,
    pub gap_close: u8,
    pub transparent_only: bool,
    pub inclusion_mode: InclusionMode,
    pub inclusion_colors: Vec<PixelValue>,
    pub extension_distance: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FillOutcome {
    pub dispatch: DispatchOutcome,
    pub changed_pixels: u64,
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
    BoxZoom {
        document_rect: RectI32,
        viewport_width: f64,
        viewport_height: f64,
    },
    Flip {
        axis: MirrorAxis,
    },
    SetRulerVisible(bool),
    SetGuidesVisible(bool),
    SetGridVisible(bool),
    SetSnapEnabled(bool),
    SetTransparentView(bool),
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
    flip_horizontal: bool,
    flip_vertical: bool,
    ruler_visible: bool,
    guides_visible: bool,
    grid_visible: bool,
    snap_enabled: bool,
    transparent_view: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            revision: 0,
            mode: ViewMode::Manual,
            flip_horizontal: false,
            flip_vertical: false,
            ruler_visible: false,
            guides_visible: true,
            grid_visible: false,
            snap_enabled: false,
            transparent_view: true,
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

    #[must_use]
    pub const fn flip_horizontal(self) -> bool {
        self.flip_horizontal
    }

    #[must_use]
    pub const fn flip_vertical(self) -> bool {
        self.flip_vertical
    }

    #[must_use]
    pub const fn ruler_visible(self) -> bool {
        self.ruler_visible
    }

    #[must_use]
    pub const fn guides_visible(self) -> bool {
        self.guides_visible
    }

    #[must_use]
    pub const fn grid_visible(self) -> bool {
        self.grid_visible
    }

    #[must_use]
    pub const fn snap_enabled(self) -> bool {
        self.snap_enabled
    }

    #[must_use]
    pub const fn transparent_view(self) -> bool {
        self.transparent_view
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreError {
    NoDocument,
    InvalidArgument(&'static str),
    InvalidState(&'static str),
    Raster(RasterError),
    Fill(FillError),
    FillOverflow { x: u32, y: u32 },
    Cancelled,
    UnsavedChanges,
    Format(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDocument => formatter.write_str("no cell document is open"),
            Self::InvalidArgument(message) => write!(formatter, "invalid argument: {message}"),
            Self::InvalidState(message) => write!(formatter, "invalid state: {message}"),
            Self::Raster(error) => write!(formatter, "raster error: {error}"),
            Self::Fill(error) => write!(formatter, "fill error: {error}"),
            Self::FillOverflow { x, y } => {
                write!(formatter, "fill reached image edge at ({x}, {y})")
            }
            Self::Cancelled => formatter.write_str("operation was cancelled before commit"),
            Self::UnsavedChanges => formatter
                .write_str("the active cell has unsaved changes and cannot be switched silently"),
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

impl From<FillError> for CoreError {
    fn from(error: FillError) -> Self {
        match error {
            FillError::Overflow { x, y } => Self::FillOverflow { x, y },
            FillError::Cancelled => Self::Cancelled,
            other => Self::Fill(other),
        }
    }
}

impl From<FormatError> for CoreError {
    fn from(error: FormatError) -> Self {
        if matches!(error, FormatError::Cancelled) {
            Self::Cancelled
        } else {
            Self::Format(error.to_string())
        }
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
    pub recovered: bool,
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
    source_revision: u64,
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
    feature_flags: u64,
    view: ViewState,
    document_width: u32,
    document_height: u32,
    guides: Vec<Guide>,
    grid: GridConfig,
    tiles: Vec<RenderTile>,
    vector_segments: Vec<RenderVectorSegment>,
    vector_fills: Vec<RenderVectorFill>,
}

impl RenderSnapshot {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn feature_flags(&self) -> u64 {
        self.feature_flags
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
    pub fn guides(&self) -> &[Guide] {
        &self.guides
    }

    #[must_use]
    pub const fn grid(&self) -> GridConfig {
        self.grid
    }

    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    #[must_use]
    pub fn tiles(&self) -> &[RenderTile] {
        &self.tiles
    }

    #[must_use]
    pub fn vector_segments(&self) -> &[RenderVectorSegment] {
        &self.vector_segments
    }

    #[must_use]
    pub fn vector_fills(&self) -> &[RenderVectorFill] {
        &self.vector_fills
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlaneNode {
    id: u64,
    kind: PlaneType,
    name: String,
    visible: bool,
    editable: bool,
    opacity_milli: u32,
    raster: TileRaster,
}

impl PlaneNode {
    fn info(&self) -> PlaneInfo {
        PlaneInfo {
            id: self.id,
            kind: self.kind,
            pixel_format: self.raster.format(),
            name: self.name.clone(),
            visible: self.visible,
            editable: self.editable,
            opacity_milli: self.opacity_milli,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LayerNode {
    id: u64,
    kind: LayerKind,
    name: String,
    visible: bool,
    editable: bool,
    opacity_milli: u32,
    planes: Vec<PlaneNode>,
}

impl LayerNode {
    fn info(&self) -> LayerInfo {
        LayerInfo {
            id: self.id,
            kind: self.kind,
            name: self.name.clone(),
            visible: self.visible,
            editable: self.editable,
            opacity_milli: self.opacity_milli,
            planes: self.planes.iter().map(PlaneNode::info).collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CellDocument {
    uuid: u128,
    id: u64,
    width: u32,
    height: u32,
    dpi_x_milli: u32,
    dpi_y_milli: u32,
    frames: FrameMetadata,
    main_line_color: PixelValue,
    palette: Palette,
    layers: Vec<LayerNode>,
    active_layer_id: u64,
    active_plane_id: u64,
    selection_plane_id: u64,
    selection: TileRaster,
    guides: Vec<Guide>,
    grid: GridConfig,
    light_table: m4::LightTableState,
    vector: m5::VectorState,
}

#[derive(Clone, Copy, Debug)]
struct DocumentIds {
    document: u64,
    layer: u64,
    main_plane: u64,
    color_plane: u64,
    selection_plane: u64,
    light_table_set: u64,
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
        let main_plane = PlaneNode {
            id: ids.main_plane,
            kind: PlaneType::MainLine,
            name: "Main Line".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            raster: TileRaster::new(paper.width, paper.height, PixelFormat::BinaryMask8)?,
        };
        let color_plane = PlaneNode {
            id: ids.color_plane,
            kind: PlaneType::Color,
            name: "Color".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            raster: TileRaster::new(paper.width, paper.height, PixelFormat::StraightRgba8)?,
        };
        Ok(Self {
            uuid,
            id: ids.document,
            width: paper.width,
            height: paper.height,
            dpi_x_milli: paper.dpi_x_milli,
            dpi_y_milli: paper.dpi_y_milli,
            frames,
            main_line_color: PixelValue::Rgba([0, 0, 0, 255]),
            palette: Palette::default(),
            layers: vec![LayerNode {
                id: ids.layer,
                kind: LayerKind::BinaryColoring,
                name: "Coloring Layer".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                planes: vec![main_plane, color_plane],
            }],
            active_layer_id: ids.layer,
            active_plane_id: ids.main_plane,
            selection_plane_id: ids.selection_plane,
            selection: TileRaster::new(paper.width, paper.height, PixelFormat::BinaryMask8)?,
            guides: Vec::new(),
            grid: GridConfig::default(),
            light_table: m4::LightTableState::new(ids.light_table_set),
            vector: m5::VectorState::default(),
        })
    }

    fn to_file(&self) -> CellFile {
        let (layer_id, main_plane_id, color_plane_id) = self.primary_ids();
        let mut planes: Vec<_> = self
            .layers
            .iter()
            .flat_map(|layer| layer.planes.iter())
            .map(|plane| raster_to_file_plane(plane.id, plane.kind.file_kind(), &plane.raster))
            .collect();
        planes.push(raster_to_file_plane(
            self.selection_plane_id,
            FilePlaneKind::Selection,
            &self.selection,
        ));
        planes.extend(self.light_table.file_planes());
        CellFile {
            document_uuid: self.uuid.to_le_bytes(),
            document_id: self.id,
            layer_id,
            main_plane_id,
            color_plane_id,
            width: self.width,
            height: self.height,
            dpi_x_milli: self.dpi_x_milli,
            dpi_y_milli: self.dpi_y_milli,
            frames: self.frames,
            main_line_color: self.main_line_color,
            palette: self.palette.colors().to_vec(),
            planes,
            m3: Some(FileM3Metadata {
                active_layer_id: self.active_layer_id,
                active_plane_id: self.active_plane_id,
                selection_plane_id: self.selection_plane_id,
                layers: self
                    .layers
                    .iter()
                    .map(|layer| FileLayer {
                        id: layer.id,
                        kind: layer.kind,
                        name: layer.name.clone(),
                        visible: layer.visible,
                        editable: layer.editable,
                        opacity_milli: layer.opacity_milli,
                        planes: layer
                            .planes
                            .iter()
                            .map(|plane| FilePlaneProperties {
                                id: plane.id,
                                name: plane.name.clone(),
                                visible: plane.visible,
                                editable: plane.editable,
                                opacity_milli: plane.opacity_milli,
                            })
                            .collect(),
                    })
                    .collect(),
                guides: self
                    .guides
                    .iter()
                    .map(|guide| FileGuide {
                        id: guide.id,
                        axis: guide.axis,
                        position: guide.position,
                    })
                    .collect(),
                grid: FileGrid {
                    origin_x: self.grid.origin_x,
                    origin_y: self.grid.origin_y,
                    spacing_x: self.grid.spacing_x,
                    spacing_y: self.grid.spacing_y,
                    subdivisions: self.grid.subdivisions,
                },
            }),
            m4: Some(self.light_table.to_file()),
            m5: self.vector.to_file(
                self.layers
                    .iter()
                    .any(|layer| layer.kind == LayerKind::VectorColoring),
            ),
        }
    }

    fn from_file(file: CellFile, revision: u64) -> Result<Self, CoreError> {
        let main_file = file
            .planes
            .iter()
            .find(|plane| plane.kind == FilePlaneKind::MainLine)
            .ok_or(CoreError::InvalidState("main line plane is missing"))?;
        let color_file = file
            .planes
            .iter()
            .find(|plane| plane.kind == FilePlaneKind::Color)
            .ok_or(CoreError::InvalidState("color plane is missing"))?;
        let mut palette = Palette::default();
        for color in &file.palette {
            palette.push(*color)?;
        }
        let (layers, active_layer_id, active_plane_id, selection_plane_id, selection, guides, grid) =
            if let Some(metadata) = &file.m3 {
                let mut layers = Vec::with_capacity(metadata.layers.len());
                for layer in &metadata.layers {
                    let mut planes = Vec::with_capacity(layer.planes.len());
                    for properties in &layer.planes {
                        let payload = file
                            .planes
                            .iter()
                            .find(|plane| plane.id == properties.id)
                            .ok_or(CoreError::InvalidState("layer plane payload is missing"))?;
                        planes.push(PlaneNode {
                            id: properties.id,
                            kind: PlaneType::from_file(payload.kind),
                            name: properties.name.clone(),
                            visible: properties.visible,
                            editable: properties.editable,
                            opacity_milli: properties.opacity_milli,
                            raster: file_plane_to_raster(payload, revision)?,
                        });
                    }
                    validate_layer_kind(layer.kind, &planes)?;
                    layers.push(LayerNode {
                        id: layer.id,
                        kind: layer.kind,
                        name: layer.name.clone(),
                        visible: layer.visible,
                        editable: layer.editable,
                        opacity_milli: layer.opacity_milli,
                        planes,
                    });
                }
                let selection_file = file
                    .planes
                    .iter()
                    .find(|plane| plane.id == metadata.selection_plane_id)
                    .ok_or(CoreError::InvalidState("selection payload is missing"))?;
                (
                    layers,
                    metadata.active_layer_id,
                    metadata.active_plane_id,
                    metadata.selection_plane_id,
                    file_plane_to_raster(selection_file, revision)?,
                    metadata
                        .guides
                        .iter()
                        .map(|guide| Guide {
                            id: guide.id,
                            axis: guide.axis,
                            position: guide.position,
                        })
                        .collect(),
                    GridConfig {
                        origin_x: metadata.grid.origin_x,
                        origin_y: metadata.grid.origin_y,
                        spacing_x: metadata.grid.spacing_x,
                        spacing_y: metadata.grid.spacing_y,
                        subdivisions: metadata.grid.subdivisions,
                    },
                )
            } else {
                let selection_plane_id = file
                    .planes
                    .iter()
                    .map(|plane| plane.id)
                    .chain([file.document_id, file.layer_id])
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(CoreError::InvalidState("selection ID overflow"))?;
                let layer_kind = if matches!(
                    main_file.pixel_format,
                    PixelFormat::Grayscale8 | PixelFormat::Grayscale16
                ) {
                    LayerKind::GrayscaleColoring
                } else {
                    LayerKind::BinaryColoring
                };
                (
                    vec![LayerNode {
                        id: file.layer_id,
                        kind: layer_kind,
                        name: "Coloring Layer".to_owned(),
                        visible: true,
                        editable: true,
                        opacity_milli: 1_000,
                        planes: vec![
                            PlaneNode {
                                id: file.main_plane_id,
                                kind: PlaneType::MainLine,
                                name: "Main Line".to_owned(),
                                visible: true,
                                editable: true,
                                opacity_milli: 1_000,
                                raster: file_plane_to_raster(main_file, revision)?,
                            },
                            PlaneNode {
                                id: file.color_plane_id,
                                kind: PlaneType::Color,
                                name: "Color".to_owned(),
                                visible: true,
                                editable: true,
                                opacity_milli: 1_000,
                                raster: file_plane_to_raster(color_file, revision)?,
                            },
                        ],
                    }],
                    file.layer_id,
                    file.main_plane_id,
                    selection_plane_id,
                    TileRaster::new(file.width, file.height, PixelFormat::BinaryMask8)?,
                    Vec::new(),
                    GridConfig::default(),
                )
            };
        let legacy_light_table_set_id = file
            .planes
            .iter()
            .map(|plane| plane.id)
            .chain(file.m3.iter().flat_map(|metadata| {
                metadata
                    .layers
                    .iter()
                    .map(|layer| layer.id)
                    .chain(metadata.guides.iter().map(|guide| guide.id))
            }))
            .chain([file.document_id, selection_plane_id])
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(CoreError::InvalidState("light-table set ID overflow"))?;
        let light_table = m4::LightTableState::from_file(
            file.m4.as_ref(),
            &file.planes,
            revision,
            legacy_light_table_set_id,
        )?;
        let vector = m5::VectorState::from_file(file.m5.as_ref());
        Ok(Self {
            uuid: u128::from_le_bytes(file.document_uuid),
            id: file.document_id,
            width: file.width,
            height: file.height,
            dpi_x_milli: file.dpi_x_milli,
            dpi_y_milli: file.dpi_y_milli,
            frames: file.frames,
            main_line_color: file.main_line_color,
            palette,
            layers,
            active_layer_id,
            active_plane_id,
            selection_plane_id,
            selection,
            guides,
            grid,
            light_table,
            vector,
        })
    }

    fn primary_layer(&self) -> &LayerNode {
        self.layers
            .iter()
            .find(|layer| {
                layer
                    .planes
                    .iter()
                    .any(|plane| plane.kind == PlaneType::MainLine)
                    && layer
                        .planes
                        .iter()
                        .any(|plane| plane.kind == PlaneType::Color)
            })
            .expect("validated coloring document must retain a coloring layer")
    }

    fn primary_ids(&self) -> (u64, u64, u64) {
        let layer = self.primary_layer();
        let main = layer
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::MainLine)
            .expect("validated coloring layer has main plane");
        let color = layer
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Color)
            .expect("validated coloring layer has color plane");
        (layer.id, main.id, color.id)
    }

    fn plane_for_role(&self, role: ActivePlane) -> Result<&PlaneNode, CoreError> {
        let kind = match role {
            ActivePlane::MainLine => PlaneType::MainLine,
            ActivePlane::Color => PlaneType::Color,
        };
        self.layers
            .iter()
            .find(|layer| layer.id == self.active_layer_id)
            .and_then(|layer| layer.planes.iter().find(|plane| plane.kind == kind))
            .or_else(|| {
                self.layers
                    .iter()
                    .flat_map(|layer| layer.planes.iter())
                    .find(|plane| plane.kind == kind)
            })
            .ok_or(CoreError::InvalidState(
                "requested plane role is unavailable",
            ))
    }

    fn plane_for_role_mut(&mut self, role: ActivePlane) -> Result<&mut PlaneNode, CoreError> {
        let kind = match role {
            ActivePlane::MainLine => PlaneType::MainLine,
            ActivePlane::Color => PlaneType::Color,
        };
        let preferred = self.active_layer_id;
        let preferred_index = self.layers.iter().position(|layer| layer.id == preferred);
        if let Some(index) = preferred_index
            && let Some(plane_index) = self.layers[index]
                .planes
                .iter()
                .position(|plane| plane.kind == kind)
        {
            return Ok(&mut self.layers[index].planes[plane_index]);
        }
        for layer in &mut self.layers {
            if let Some(index) = layer.planes.iter().position(|plane| plane.kind == kind) {
                return Ok(&mut layer.planes[index]);
            }
        }
        Err(CoreError::InvalidState(
            "requested plane role is unavailable",
        ))
    }

    fn raster(&self, plane: ActivePlane) -> &TileRaster {
        &self
            .plane_for_role(plane)
            .expect("validated coloring document must retain required planes")
            .raster
    }

    fn raster_mut(&mut self, plane: ActivePlane) -> &mut TileRaster {
        &mut self
            .plane_for_role_mut(plane)
            .expect("validated coloring document must retain required planes")
            .raster
    }

    fn active_plane_role(&self) -> ActivePlane {
        self.layers
            .iter()
            .flat_map(|layer| layer.planes.iter())
            .find(|plane| plane.id == self.active_plane_id)
            .map_or(ActivePlane::Color, |plane| match plane.kind {
                PlaneType::MainLine => ActivePlane::MainLine,
                _ => ActivePlane::Color,
            })
    }

    fn plane_by_id(&self, id: u64) -> Option<&PlaneNode> {
        self.layers
            .iter()
            .flat_map(|layer| layer.planes.iter())
            .find(|plane| plane.id == id)
    }

    fn plane_by_id_mut(&mut self, id: u64) -> Option<&mut PlaneNode> {
        self.layers
            .iter_mut()
            .flat_map(|layer| layer.planes.iter_mut())
            .find(|plane| plane.id == id)
    }

    fn max_stable_id(&self) -> u64 {
        self.layers
            .iter()
            .flat_map(|layer| {
                std::iter::once(layer.id).chain(layer.planes.iter().map(|plane| plane.id))
            })
            .chain(self.guides.iter().map(|guide| guide.id))
            .chain([self.light_table.maximum_id()])
            .chain([self.vector.maximum_id()])
            .chain([self.id, self.selection_plane_id])
            .max()
            .unwrap_or(0)
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
enum HistoryChange {
    Pixels {
        plane_id: u64,
        changes: Vec<PixelChange>,
    },
    Palette {
        before: Palette,
        after: Palette,
    },
    MainLineColor {
        before: PixelValue,
        after: PixelValue,
    },
    Document {
        before: Box<CellDocument>,
        after: Box<CellDocument>,
    },
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    change: HistoryChange,
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

#[derive(Clone, Debug)]
struct FloatingSelection {
    payload: ClipboardPayload,
    destination_plane_id: u64,
    transform: FloatingTransform,
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
    recovered: bool,
    active_stroke: Option<StrokeSession>,
    render_cache: BTreeMap<TileCoord, RenderTile>,
    next_render_tile_revision: u64,
    next_preview_revision: u64,
    color_check: Option<ColorCheckMode>,
    secondary_views: BTreeMap<u64, ViewState>,
    next_view_id: u64,
    floating: Option<FloatingSelection>,
    shortcuts: BTreeMap<u32, ShortcutBinding>,
    sequence: Option<m4::SequenceState>,
    motion_check: Option<m4::MotionCheckState>,
    subpalette_index: Option<usize>,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    #[must_use]
    pub fn new() -> Self {
        Self {
            document: None,
            document_revision: 0,
            view: ViewState {
                zoom: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
                revision: 0,
                mode: ViewMode::Manual,
                flip_horizontal: false,
                flip_vertical: false,
                ruler_visible: false,
                guides_visible: true,
                grid_visible: false,
                snap_enabled: false,
                transparent_view: true,
            },
            history: Vec::new(),
            history_cursor: 0,
            current_state: 0,
            next_state: 1,
            savepoint: None,
            next_id: 1,
            current_path: None,
            recovered: false,
            active_stroke: None,
            render_cache: BTreeMap::new(),
            next_render_tile_revision: 1,
            next_preview_revision: 1_u64 << 63,
            color_check: None,
            secondary_views: BTreeMap::new(),
            next_view_id: 1,
            floating: None,
            shortcuts: default_shortcuts(),
            sequence: None,
            motion_check: None,
            subpalette_index: None,
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
            selection_plane: self.allocate_id(),
            light_table_set: self.allocate_id(),
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
        self.recovered = false;
        self.color_check = None;
        self.secondary_views.clear();
        self.floating = None;
        self.sequence = None;
        self.motion_check = None;
        self.subpalette_index = None;
        self.document_info()
    }

    pub fn set_active_plane(&mut self, plane: ActivePlane) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let kind = match plane {
            ActivePlane::MainLine => PlaneType::MainLine,
            ActivePlane::Color => PlaneType::Color,
        };
        let (layer_id, plane_id) = document
            .layers
            .iter()
            .find_map(|layer| {
                layer
                    .planes
                    .iter()
                    .find(|candidate| candidate.kind == kind)
                    .map(|candidate| (layer.id, candidate.id))
            })
            .ok_or(CoreError::InvalidState(
                "requested plane role is unavailable",
            ))?;
        document.active_layer_id = layer_id;
        document.active_plane_id = plane_id;
        Ok(())
    }

    pub fn layers(&self) -> Result<Vec<LayerInfo>, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .layers
            .iter()
            .map(LayerNode::info)
            .collect())
    }

    pub fn set_active_node(&mut self, layer_id: u64, plane_id: u64) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let layer = document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if !layer.planes.iter().any(|plane| plane.id == plane_id) {
            return Err(CoreError::InvalidArgument(
                "plane ID does not belong to the requested layer",
            ));
        }
        document.active_layer_id = layer_id;
        document.active_plane_id = plane_id;
        Ok(())
    }

    pub fn create_layer(
        &mut self,
        kind: LayerKind,
        name: &str,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.layers.len() >= MAX_LAYERS {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        let layer_id = self.allocate_id();
        let mut planes = Vec::new();
        let (width, height) = (before.width, before.height);
        match kind {
            LayerKind::BinaryColoring | LayerKind::GrayscaleColoring => {
                let main_id = self.allocate_id();
                let color_id = self.allocate_id();
                planes.push(PlaneNode {
                    id: main_id,
                    kind: PlaneType::MainLine,
                    name: "Main Line".to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                    raster: TileRaster::new(
                        width,
                        height,
                        if kind == LayerKind::BinaryColoring {
                            PixelFormat::BinaryMask8
                        } else {
                            PixelFormat::Grayscale8
                        },
                    )?,
                });
                planes.push(PlaneNode {
                    id: color_id,
                    kind: PlaneType::Color,
                    name: "Color".to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                    raster: TileRaster::new(width, height, PixelFormat::StraightRgba8)?,
                });
            }
            LayerKind::Raster => planes.push(PlaneNode {
                id: self.allocate_id(),
                kind: PlaneType::Raster,
                name: "Raster".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster: TileRaster::new(width, height, PixelFormat::StraightRgba8)?,
            }),
            LayerKind::Selection => planes.push(PlaneNode {
                id: self.allocate_id(),
                kind: PlaneType::Selection,
                name: "Selection".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster: TileRaster::new(width, height, PixelFormat::BinaryMask8)?,
            }),
            LayerKind::VectorColoring => {
                for (kind, name) in [
                    (PlaneType::VectorMainLine, "Vector Main Line"),
                    (PlaneType::ColorTrace, "Color Trace"),
                    (PlaneType::VectorFill, "Vector Fill"),
                ] {
                    planes.push(PlaneNode {
                        id: self.allocate_id(),
                        kind,
                        name: name.to_owned(),
                        visible: true,
                        editable: true,
                        opacity_milli: 1_000,
                        raster: TileRaster::new(width, height, PixelFormat::StraightRgba8)?,
                    });
                }
            }
            LayerKind::Frame
            | LayerKind::VanishingPoint
            | LayerKind::Adjustment
            | LayerKind::Text
            | LayerKind::Annotation => {}
        }
        validate_layer_kind(kind, &planes)?;
        let mut after = before.clone();
        after.layers.push(LayerNode {
            id: layer_id,
            kind,
            name: unique_layer_name(&after.layers, name),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes,
        });
        after.active_layer_id = layer_id;
        if let Some(plane) = after.layers.last().and_then(|layer| layer.planes.first()) {
            after.active_plane_id = plane.id;
        }
        let outcome = self.commit_document_edit(before, after)?;
        Ok((outcome, layer_id))
    }

    pub fn duplicate_layer(&mut self, layer_id: u64) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.layers.len() >= MAX_LAYERS {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        let index = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        let mut next_id = self.next_id;
        let allocate_id = |next_id: &mut u64| {
            let id = *next_id;
            *next_id = next_id.saturating_add(1).max(1);
            id
        };
        let mut duplicate = before.layers[index].clone();
        duplicate.id = allocate_id(&mut next_id);
        duplicate.name = unique_layer_name(&before.layers, &format!("{} Copy", duplicate.name));
        let mut plane_map = BTreeMap::new();
        for plane in &mut duplicate.planes {
            let source_id = plane.id;
            plane.id = allocate_id(&mut next_id);
            plane_map.insert(source_id, plane.id);
            plane.name = format!("{} Copy", plane.name);
        }
        let duplicate_id = duplicate.id;
        let active_plane_id = duplicate.planes.first().map(|plane| plane.id);
        let mut after = before.clone();
        after.vector.duplicate_planes(&plane_map, &mut next_id);
        after.vector.ensure_limits()?;
        after.layers.insert(index + 1, duplicate);
        after.active_layer_id = duplicate_id;
        if let Some(id) = active_plane_id {
            after.active_plane_id = id;
        }
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, duplicate_id))
    }

    pub fn delete_layer(&mut self, layer_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let index = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if is_coloring_layer(before.layers[index].kind)
            && before
                .layers
                .iter()
                .filter(|layer| is_coloring_layer(layer.kind))
                .count()
                == 1
        {
            return Err(CoreError::InvalidState(
                "the final coloring layer cannot be deleted",
            ));
        }
        let mut after = before.clone();
        after.vector.remove_layer(&before, layer_id);
        after.layers.remove(index);
        if after.active_layer_id == layer_id {
            let replacement = after
                .layers
                .get(index.min(after.layers.len().saturating_sub(1)))
                .ok_or(CoreError::InvalidState("document must retain a layer"))?;
            after.active_layer_id = replacement.id;
            after.active_plane_id = replacement
                .planes
                .first()
                .map_or(after.primary_ids().1, |plane| plane.id);
        }
        if after.plane_by_id(after.active_plane_id).is_none() {
            after.active_plane_id = after
                .layers
                .iter()
                .find(|layer| layer.id == after.active_layer_id)
                .and_then(|layer| layer.planes.first())
                .map_or(after.primary_ids().1, |plane| plane.id);
        }
        self.commit_document_edit(before, after)
    }

    pub fn reorder_layer(
        &mut self,
        layer_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let source = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if destination_index >= before.layers.len() {
            return Err(CoreError::InvalidArgument(
                "layer destination index is outside the tree",
            ));
        }
        if source == destination_index {
            return Ok(self.noop_outcome());
        }
        let mut after = before.clone();
        let layer = after.layers.remove(source);
        after.layers.insert(destination_index, layer);
        self.commit_document_edit(before, after)
    }

    pub fn set_layer_properties(
        &mut self,
        layer_id: u64,
        visible: bool,
        editable: bool,
        opacity_milli: u32,
        name: &str,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        if opacity_milli > 1_000 {
            return Err(CoreError::InvalidArgument("opacity exceeds 1000"));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let layer = after
            .layers
            .iter_mut()
            .find(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        layer.visible = visible;
        layer.editable = editable;
        layer.opacity_milli = opacity_milli;
        layer.name = name.to_owned();
        if after.layers == before.layers {
            return Ok(self.noop_outcome());
        }
        self.commit_document_edit(before, after)
    }

    pub fn create_plane(
        &mut self,
        layer_id: u64,
        kind: PlaneType,
        format: PixelFormat,
        name: &str,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        validate_plane_format(kind, format)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let layer_index = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if before.layers[layer_index].planes.len() >= MAX_PLANES_PER_LAYER {
            return Err(CoreError::InvalidState("plane limit reached"));
        }
        let plane_id = self.allocate_id();
        let mut after = before.clone();
        after.layers[layer_index].planes.push(PlaneNode {
            id: plane_id,
            kind,
            name: name.to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            raster: TileRaster::new(after.width, after.height, format)?,
        });
        validate_layer_kind(
            after.layers[layer_index].kind,
            &after.layers[layer_index].planes,
        )?;
        after.active_layer_id = layer_id;
        after.active_plane_id = plane_id;
        let outcome = self.commit_document_edit(before, after)?;
        Ok((outcome, plane_id))
    }

    pub fn duplicate_plane(&mut self, plane_id: u64) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let (layer_index, plane_index) = find_plane_indices(&before, plane_id)?;
        if before.layers[layer_index].planes.len() >= MAX_PLANES_PER_LAYER {
            return Err(CoreError::InvalidState("plane limit reached"));
        }
        if matches!(
            before.layers[layer_index].planes[plane_index].kind,
            PlaneType::MainLine
                | PlaneType::Color
                | PlaneType::VectorMainLine
                | PlaneType::VectorFill
        ) {
            return Err(CoreError::InvalidState(
                "required singleton planes cannot be duplicated",
            ));
        }
        let mut duplicate = before.layers[layer_index].planes[plane_index].clone();
        let source_plane_id = duplicate.id;
        let mut next_id = self.next_id;
        let duplicate_id = next_id;
        next_id = next_id.saturating_add(1).max(1);
        duplicate.id = duplicate_id;
        duplicate.name = unique_plane_name(
            &before.layers[layer_index].planes,
            &format!("{} Copy", duplicate.name),
        );
        let mut after = before.clone();
        let mut plane_map = BTreeMap::new();
        plane_map.insert(source_plane_id, duplicate_id);
        after.vector.duplicate_planes(&plane_map, &mut next_id);
        after.vector.ensure_limits()?;
        after.layers[layer_index]
            .planes
            .insert(plane_index + 1, duplicate);
        after.active_layer_id = after.layers[layer_index].id;
        after.active_plane_id = duplicate_id;
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, duplicate_id))
    }

    pub fn delete_plane(&mut self, plane_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let (layer_index, plane_index) = find_plane_indices(&before, plane_id)?;
        let mut after = before.clone();
        after.vector.remove_plane(plane_id);
        after.layers[layer_index].planes.remove(plane_index);
        validate_layer_kind(
            after.layers[layer_index].kind,
            &after.layers[layer_index].planes,
        )?;
        if after.active_plane_id == plane_id {
            after.active_plane_id = after.layers[layer_index]
                .planes
                .first()
                .map_or(after.primary_ids().1, |plane| plane.id);
        }
        self.commit_document_edit(before, after)
    }

    pub fn reorder_plane(
        &mut self,
        plane_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let (layer_index, source) = find_plane_indices(&before, plane_id)?;
        if destination_index >= before.layers[layer_index].planes.len() {
            return Err(CoreError::InvalidArgument(
                "plane destination index is outside its layer",
            ));
        }
        if source == destination_index {
            return Ok(self.noop_outcome());
        }
        let mut after = before.clone();
        let plane = after.layers[layer_index].planes.remove(source);
        after.layers[layer_index]
            .planes
            .insert(destination_index, plane);
        self.commit_document_edit(before, after)
    }

    pub fn set_plane_properties(
        &mut self,
        plane_id: u64,
        visible: bool,
        editable: bool,
        opacity_milli: u32,
        name: &str,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        if opacity_milli > 1_000 {
            return Err(CoreError::InvalidArgument("opacity exceeds 1000"));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let plane = after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidArgument("plane ID does not exist"))?;
        plane.visible = visible;
        plane.editable = editable;
        plane.opacity_milli = opacity_milli;
        plane.name = name.to_owned();
        if after.layers == before.layers {
            return Ok(self.noop_outcome());
        }
        self.commit_document_edit(before, after)
    }

    pub fn convert_layer(
        &mut self,
        layer_id: u64,
        destination: LayerKind,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let index = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        let source = before.layers[index].kind;
        if source == destination {
            return Ok(self.noop_outcome());
        }
        if !matches!(
            (source, destination),
            (LayerKind::BinaryColoring, LayerKind::GrayscaleColoring)
                | (LayerKind::GrayscaleColoring, LayerKind::BinaryColoring)
        ) {
            return Err(CoreError::InvalidArgument(
                "requested layer conversion would lose unsupported semantics",
            ));
        }
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        let main = after.layers[index]
            .planes
            .iter_mut()
            .find(|plane| plane.kind == PlaneType::MainLine)
            .ok_or(CoreError::InvalidState("coloring layer has no main plane"))?;
        main.raster = convert_main_line_raster(
            &main.raster,
            destination == LayerKind::GrayscaleColoring,
            revision,
        )?;
        after.layers[index].kind = destination;
        validate_layer_kind(destination, &after.layers[index].planes)?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn merge_layer_into_below(&mut self, layer_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let upper = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if upper + 1 >= before.layers.len() {
            return Err(CoreError::InvalidArgument("layer has no lower sibling"));
        }
        let lower = upper + 1;
        if before.layers[upper].kind != before.layers[lower].kind
            || before.layers[upper].planes.len() != before.layers[lower].planes.len()
            || before.layers[upper]
                .planes
                .iter()
                .zip(&before.layers[lower].planes)
                .any(|(left, right)| {
                    left.kind != right.kind || left.raster.format() != right.raster.format()
                })
        {
            return Err(CoreError::InvalidArgument(
                "only layers with compatible type and plane topology can merge",
            ));
        }
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        let source_planes = after.layers[upper].planes.clone();
        let lower_id = after.layers[lower].id;
        let lower_plane_id = after.layers[lower]
            .planes
            .first()
            .map_or(after.primary_ids().1, |plane| plane.id);
        let mut plane_reassignments = Vec::new();
        for (destination, source) in after.layers[lower].planes.iter_mut().zip(&source_planes) {
            merge_raster(&mut destination.raster, &source.raster, revision)?;
            plane_reassignments.push((source.id, destination.id));
        }
        for (source_id, destination_id) in plane_reassignments {
            after.vector.reassign_plane(source_id, destination_id);
        }
        after.layers.remove(upper);
        after.active_layer_id = lower_id;
        after.active_plane_id = lower_plane_id;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn apply_selection(
        &mut self,
        shape: &SelectionShape,
        operation: SelectionOperation,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let candidate = selection_mask_for_shape(&before, shape, revision)?;
        let mut after = before.clone();
        after.selection =
            combine_selection_masks(&before.selection, &candidate, operation, revision)?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn invert_selection(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        after.selection = invert_selection_mask(&before.selection, revision)?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn resize_selection(&mut self, pixels: i32) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if pixels == i32::MIN || pixels.unsigned_abs() > 4_096 {
            return Err(CoreError::InvalidArgument(
                "selection expansion is outside its bound",
            ));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        after.selection = morphology_selection(&before.selection, pixels, revision)?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn select_color(
        &mut self,
        color: PixelValue,
        tolerance: u16,
        different: bool,
        operation: SelectionOperation,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let source = before
            .plane_by_id(before.active_plane_id)
            .ok_or(CoreError::InvalidState("active plane is missing"))?;
        let revision = self.next_document_revision()?;
        let candidate =
            color_selection_mask(&source.raster, color, tolerance, different, revision)?;
        let mut after = before.clone();
        after.selection =
            combine_selection_masks(&before.selection, &candidate, operation, revision)?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn selection_bounds(&self) -> Result<Option<RectI32>, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        mask_bounds(&document.selection)
    }

    pub fn selection_to_layer(&mut self, name: &str) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.layers.len() >= MAX_LAYERS {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        if mask_bounds(&before.selection)?.is_none() {
            return Err(CoreError::InvalidState("selection is empty"));
        }
        let layer_id = self.allocate_id();
        let plane_id = self.allocate_id();
        let mut after = before.clone();
        after.layers.push(LayerNode {
            id: layer_id,
            kind: LayerKind::Selection,
            name: unique_layer_name(&after.layers, name),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes: vec![PlaneNode {
                id: plane_id,
                kind: PlaneType::Selection,
                name: "Selection".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster: before.selection.clone(),
            }],
        });
        after.active_layer_id = layer_id;
        after.active_plane_id = plane_id;
        let outcome = self.commit_document_edit(before, after)?;
        Ok((outcome, layer_id))
    }

    pub fn selection_from_layer(
        &mut self,
        layer_id: u64,
        operation: SelectionLayerOperation,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let layer = before
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        let mask = layer
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Selection)
            .ok_or(CoreError::InvalidArgument(
                "layer does not contain a selection plane",
            ))?;
        let revision = self.next_document_revision()?;
        let selection_operation = match operation {
            SelectionLayerOperation::Replace => SelectionOperation::New,
            SelectionLayerOperation::Add => SelectionOperation::Add,
            SelectionLayerOperation::Subtract => SelectionOperation::Subtract,
        };
        let mut after = before.clone();
        after.selection = combine_selection_masks(
            &before.selection,
            &mask.raster,
            selection_operation,
            revision,
        )?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn copy_selection(&self) -> Result<ClipboardPayload, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let bounds = mask_bounds(&document.selection)?
            .ok_or(CoreError::InvalidState("selection is empty"))?;
        let plane = document
            .plane_by_id(document.active_plane_id)
            .ok_or(CoreError::InvalidState("active plane is missing"))?;
        if !matches!(
            plane.kind,
            PlaneType::MainLine | PlaneType::Color | PlaneType::Raster | PlaneType::Selection
        ) {
            return Err(CoreError::InvalidState("active plane is not copyable"));
        }
        let mut pixels = Vec::new();
        for y in bounds.y..bounds.y + bounds.height {
            for x in bounds.x..bounds.x + bounds.width {
                let (Ok(x_u32), Ok(y_u32)) = (u32::try_from(x), u32::try_from(y)) else {
                    continue;
                };
                if !matches!(
                    document.selection.pixel(x_u32, y_u32)?,
                    PixelValue::Binary(255)
                ) {
                    continue;
                }
                let value = plane.raster.pixel(x_u32, y_u32)?;
                if !value.is_zero() {
                    pixels.push(ClipboardPixel { x, y, value });
                }
            }
        }
        Ok(ClipboardPayload {
            source_document_uuid: document.uuid,
            bounds,
            planes: vec![ClipboardPlane {
                kind: plane.kind,
                pixel_format: plane.raster.format(),
                origin_x: bounds.x,
                origin_y: bounds.y,
                pixels,
            }],
        })
    }

    pub fn begin_paste(&mut self, payload: &ClipboardPayload) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        if self.floating.is_some() {
            return Err(CoreError::InvalidState("floating paste is already active"));
        }
        if payload.planes.is_empty() || payload.planes.len() > MAX_PLANES_PER_LAYER {
            return Err(CoreError::InvalidArgument(
                "clipboard plane count is invalid",
            ));
        }
        if payload.bounds.width <= 0
            || payload.bounds.height <= 0
            || payload.bounds.x.checked_add(payload.bounds.width).is_none()
            || payload
                .bounds
                .y
                .checked_add(payload.bounds.height)
                .is_none()
        {
            return Err(CoreError::InvalidArgument(
                "clipboard bounds are outside the supported range",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let active_destination = document
            .plane_by_id(document.active_plane_id)
            .ok_or(CoreError::InvalidState("active plane is missing"))?;
        let compatible_source = |destination: &PlaneNode| {
            payload.planes.iter().find(|plane| {
                plane.kind == destination.kind && plane.pixel_format == destination.raster.format()
            })
        };
        let active_layer = document.layers.iter().find(|layer| {
            layer
                .planes
                .iter()
                .any(|plane| plane.id == active_destination.id)
        });
        let (destination, source) =
            compatible_source(active_destination)
                .map(|source| (active_destination, source))
                .or_else(|| {
                    active_layer.and_then(|layer| {
                        layer.planes.iter().find_map(|plane| {
                            compatible_source(plane).map(|source| (plane, source))
                        })
                    })
                })
                .or_else(|| {
                    document.layers.iter().find_map(|layer| {
                        layer.planes.iter().find_map(|plane| {
                            compatible_source(plane).map(|source| (plane, source))
                        })
                    })
                })
                .ok_or(CoreError::InvalidArgument(
                    "clipboard has no compatible typed destination payload",
                ))?;
        if source.pixels.len() as u64 > MAX_FILL_PIXELS {
            return Err(CoreError::InvalidArgument(
                "clipboard payload exceeds work limit",
            ));
        }
        if source.pixels.iter().any(|pixel| {
            pixel.x < payload.bounds.x
                || pixel.y < payload.bounds.y
                || pixel.x >= payload.bounds.x + payload.bounds.width
                || pixel.y >= payload.bounds.y + payload.bounds.height
        }) {
            return Err(CoreError::InvalidArgument(
                "clipboard pixel lies outside its bounds",
            ));
        }
        self.floating = Some(FloatingSelection {
            payload: payload.clone(),
            destination_plane_id: destination.id,
            transform: FloatingTransform::default(),
        });
        Ok(())
    }

    pub fn set_floating_transform(
        &mut self,
        transform: FloatingTransform,
    ) -> Result<(), CoreError> {
        validate_floating_transform(transform)?;
        self.floating
            .as_mut()
            .ok_or(CoreError::InvalidState("there is no floating paste"))?
            .transform = transform;
        Ok(())
    }

    pub fn cancel_floating(&mut self) {
        self.floating = None;
    }

    pub fn commit_floating(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let floating = self
            .floating
            .clone()
            .ok_or(CoreError::InvalidState("there is no floating paste"))?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let destination =
            document
                .plane_by_id(floating.destination_plane_id)
                .ok_or(CoreError::InvalidState(
                    "paste destination no longer exists",
                ))?;
        ensure_editable_plane(document, floating.destination_plane_id)?;
        let source = floating
            .payload
            .planes
            .iter()
            .find(|plane| {
                plane.kind == destination.kind && plane.pixel_format == destination.raster.format()
            })
            .ok_or(CoreError::InvalidArgument(
                "compatible clipboard plane is missing",
            ))?;
        let mut staged = BTreeMap::new();
        let center_x = f64::from(floating.payload.bounds.x)
            + f64::from(floating.payload.bounds.width - 1) / 2.0;
        let center_y = f64::from(floating.payload.bounds.y)
            + f64::from(floating.payload.bounds.height - 1) / 2.0;
        let radians = floating.transform.rotation_degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        let transform_point = |x: f64, y: f64| {
            let local_x = (x - center_x) * floating.transform.scale_x;
            let local_y = (y - center_y) * floating.transform.scale_y;
            (
                center_x + local_x * cos - local_y * sin + floating.transform.translate_x,
                center_y + local_x * sin + local_y * cos + floating.transform.translate_y,
            )
        };
        let left = f64::from(floating.payload.bounds.x);
        let top = f64::from(floating.payload.bounds.y);
        let right = f64::from(floating.payload.bounds.x + floating.payload.bounds.width - 1);
        let bottom = f64::from(floating.payload.bounds.y + floating.payload.bounds.height - 1);
        let corners = [
            transform_point(left, top),
            transform_point(right, top),
            transform_point(left, bottom),
            transform_point(right, bottom),
        ];
        if corners
            .iter()
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
        {
            return Err(CoreError::InvalidArgument("floating transform overflowed"));
        }
        let min_x = corners
            .iter()
            .map(|corner| corner.0)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as i64;
        let max_x = corners
            .iter()
            .map(|corner| corner.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(f64::from(document.width.saturating_sub(1))) as i64;
        let min_y = corners
            .iter()
            .map(|corner| corner.1)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as i64;
        let max_y = corners
            .iter()
            .map(|corner| corner.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(f64::from(document.height.saturating_sub(1))) as i64;
        if min_x <= max_x && min_y <= max_y {
            let work = u64::try_from(max_x - min_x + 1)
                .ok()
                .and_then(|width| {
                    u64::try_from(max_y - min_y + 1)
                        .ok()
                        .and_then(|height| width.checked_mul(height))
                })
                .ok_or(CoreError::InvalidArgument("floating work size overflows"))?;
            if work > MAX_FILL_PIXELS {
                return Err(CoreError::InvalidArgument(
                    "floating transform exceeds the bounded work limit",
                ));
            }
            let source_pixels: BTreeMap<_, _> = source
                .pixels
                .iter()
                .map(|pixel| ((pixel.x, pixel.y), pixel.value))
                .collect();
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let translated_x = x as f64 - center_x - floating.transform.translate_x;
                    let translated_y = y as f64 - center_y - floating.transform.translate_y;
                    let source_x = center_x
                        + (translated_x * cos + translated_y * sin) / floating.transform.scale_x;
                    let source_y = center_y
                        + (-translated_x * sin + translated_y * cos) / floating.transform.scale_y;
                    let source_coord = (source_x.round() as i32, source_y.round() as i32);
                    if let Some(value) = source_pixels.get(&source_coord) {
                        staged.insert((x as u32, y as u32), *value);
                    }
                }
            }
        }
        if staged.is_empty() {
            return Err(CoreError::InvalidState(
                "floating selection contains no content inside the destination paper",
            ));
        }
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let plane = document
            .plane_by_id_mut(floating.destination_plane_id)
            .ok_or(CoreError::InvalidState(
                "paste destination no longer exists",
            ))?;
        let mut changes = Vec::with_capacity(staged.len());
        for ((x, y), source_value) in staged {
            let before = plane.raster.pixel(x, y)?;
            let after = paste_value(before, source_value, plane.kind)?;
            if before != after {
                plane.raster.set_pixel(x, y, after, revision)?;
                changes.push(PixelChange {
                    x,
                    y,
                    before,
                    after,
                });
            }
        }
        if changes.is_empty() {
            self.floating = None;
            return Ok(self.noop_outcome());
        }
        document.active_plane_id = floating.destination_plane_id;
        self.document_revision = revision;
        self.commit_pixel_history(floating.destination_plane_id, changes, after_state);
        self.floating = None;
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn mirror_document(&mut self, axis: MirrorAxis) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        for plane in after
            .layers
            .iter_mut()
            .flat_map(|layer| layer.planes.iter_mut())
        {
            plane.raster = mirror_raster(&plane.raster, axis, revision)?;
        }
        after.selection = mirror_raster(&after.selection, axis, revision)?;
        mirror_frame_metadata(&mut after.frames, after.width, after.height, axis)?;
        for guide in &mut after.guides {
            match (axis, guide.axis) {
                (MirrorAxis::Horizontal, GuideAxis::Vertical) => {
                    guide.position = i32::try_from(after.width).map_err(|_| {
                        CoreError::InvalidState("document width exceeds guide range")
                    })? - guide.position;
                }
                (MirrorAxis::Vertical, GuideAxis::Horizontal) => {
                    guide.position = i32::try_from(after.height).map_err(|_| {
                        CoreError::InvalidState("document height exceeds guide range")
                    })? - guide.position;
                }
                _ => {}
            }
        }
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn guides(&self) -> Result<&[Guide], CoreError> {
        Ok(&self.document.as_ref().ok_or(CoreError::NoDocument)?.guides)
    }

    pub fn add_guide(
        &mut self,
        axis: GuideAxis,
        position: i32,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.guides.len() >= MAX_GUIDES {
            return Err(CoreError::InvalidState("guide limit reached"));
        }
        validate_guide_position(&before, axis, position)?;
        let id = self.allocate_id();
        let mut after = before.clone();
        after.guides.push(Guide { id, axis, position });
        after
            .guides
            .sort_by_key(|guide| (guide.axis as u8, guide.position, guide.id));
        let outcome = self.commit_document_edit(before, after)?;
        Ok((outcome, id))
    }

    pub fn move_guide(
        &mut self,
        guide_id: u64,
        position: i32,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let guide = before
            .guides
            .iter()
            .find(|guide| guide.id == guide_id)
            .ok_or(CoreError::InvalidArgument("guide ID does not exist"))?;
        validate_guide_position(&before, guide.axis, position)?;
        if guide.position == position {
            return Ok(self.noop_outcome());
        }
        let mut after = before.clone();
        after
            .guides
            .iter_mut()
            .find(|guide| guide.id == guide_id)
            .expect("guide existence checked")
            .position = position;
        after
            .guides
            .sort_by_key(|guide| (guide.axis as u8, guide.position, guide.id));
        self.commit_document_edit(before, after)
    }

    pub fn delete_guide(&mut self, guide_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let index = before
            .guides
            .iter()
            .position(|guide| guide.id == guide_id)
            .ok_or(CoreError::InvalidArgument("guide ID does not exist"))?;
        let mut after = before.clone();
        after.guides.remove(index);
        self.commit_document_edit(before, after)
    }

    pub fn grid(&self) -> Result<GridConfig, CoreError> {
        Ok(self.document.as_ref().ok_or(CoreError::NoDocument)?.grid)
    }

    pub fn set_grid(&mut self, grid: GridConfig) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_grid(grid)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.grid == grid {
            return Ok(self.noop_outcome());
        }
        let mut after = before.clone();
        after.grid = grid;
        self.commit_document_edit(before, after)
    }

    pub fn snap_document_point(&self, x: f64, y: f64) -> Result<(f64, f64), CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if !x.is_finite() || !y.is_finite() {
            return Err(CoreError::InvalidArgument("snap point is not finite"));
        }
        if !self.view.snap_enabled {
            return Ok((x, y));
        }
        let grid = document.grid;
        let snap_axis = |value: f64, origin: i32, spacing: u32| {
            let step = f64::from(spacing) / f64::from(grid.subdivisions);
            f64::from(origin) + ((value - f64::from(origin)) / step).round() * step
        };
        let mut snapped = (
            snap_axis(x, grid.origin_x, grid.spacing_x),
            snap_axis(y, grid.origin_y, grid.spacing_y),
        );
        for guide in &document.guides {
            match guide.axis {
                GuideAxis::Vertical if (x - f64::from(guide.position)).abs() <= 4.0 => {
                    snapped.0 = f64::from(guide.position);
                }
                GuideAxis::Horizontal if (y - f64::from(guide.position)).abs() <= 4.0 => {
                    snapped.1 = f64::from(guide.position);
                }
                _ => {}
            }
        }
        Ok(snapped)
    }

    pub fn create_view(&mut self) -> Result<u64, CoreError> {
        if self.document.is_none() {
            return Err(CoreError::NoDocument);
        }
        let id = self.next_view_id;
        self.next_view_id = self
            .next_view_id
            .checked_add(1)
            .ok_or(CoreError::InvalidState("view ID overflow"))?;
        self.secondary_views.insert(id, self.view);
        Ok(id)
    }

    pub fn close_view(&mut self, view_id: u64) -> Result<(), CoreError> {
        self.secondary_views
            .remove(&view_id)
            .map(|_| ())
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))
    }

    pub fn apply_view_for(
        &mut self,
        view_id: u64,
        command: ViewCommand,
    ) -> Result<ViewState, CoreError> {
        let original = self.view;
        self.view = *self
            .secondary_views
            .get(&view_id)
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))?;
        let result = self.apply_view(command);
        let updated = self.view;
        self.view = original;
        if result.is_ok() {
            self.secondary_views.insert(view_id, updated);
        }
        result.map(|_| updated)
    }

    pub fn build_snapshot_for(&mut self, view_id: u64) -> Result<RenderSnapshot, CoreError> {
        let selected = *self
            .secondary_views
            .get(&view_id)
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))?;
        let original = self.view;
        self.view = selected;
        let snapshot = self.build_snapshot();
        self.view = original;
        Ok(snapshot)
    }

    pub fn locator_sample(
        &self,
        view_id: Option<u64>,
        device_x: f64,
        device_y: f64,
    ) -> Result<LocatorSample, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let view = match view_id {
            Some(id) => *self
                .secondary_views
                .get(&id)
                .ok_or(CoreError::InvalidArgument("view ID does not exist"))?,
            None => self.view,
        };
        let (x, y) = device_to_document(view, document.width, document.height, device_x, device_y)?;
        let document_x = x.floor() as i32;
        let document_y = y.floor() as i32;
        let color = if document_x >= 0
            && document_y >= 0
            && document_x < document.width as i32
            && document_y < document.height as i32
        {
            self.eyedropper(
                EyedropperSource::Composite,
                document_x as u32,
                document_y as u32,
            )
            .ok()
        } else {
            None
        };
        Ok(LocatorSample {
            document_x,
            document_y,
            selection_bounds: mask_bounds(&document.selection)?,
            color,
        })
    }

    pub fn shortcut_bindings(&self) -> Vec<ShortcutBinding> {
        self.shortcuts.values().copied().collect()
    }

    pub fn rebind_shortcut(&mut self, binding: ShortcutBinding) -> Result<(), CoreError> {
        if binding.command_id == 0
            || binding.virtual_key == 0
            || binding.modifiers & !SHORTCUT_MODIFIER_MASK != 0
        {
            return Err(CoreError::InvalidArgument("shortcut binding is invalid"));
        }
        if self.shortcuts.len() >= MAX_SHORTCUTS
            && !self.shortcuts.contains_key(&binding.command_id)
        {
            return Err(CoreError::InvalidState("shortcut limit reached"));
        }
        self.shortcuts.retain(|command, candidate| {
            *command == binding.command_id
                || candidate.virtual_key != binding.virtual_key
                || candidate.modifiers != binding.modifiers
        });
        self.shortcuts.insert(binding.command_id, binding);
        Ok(())
    }

    pub fn resolve_shortcut(
        &self,
        virtual_key: u32,
        modifiers: u32,
    ) -> Result<Option<u32>, CoreError> {
        if virtual_key == 0 || modifiers & !SHORTCUT_MODIFIER_MASK != 0 {
            return Err(CoreError::InvalidArgument("shortcut input is invalid"));
        }
        Ok(self.shortcuts.values().find_map(|binding| {
            (binding.virtual_key == virtual_key && binding.modifiers == modifiers)
                .then_some(binding.command_id)
        }))
    }

    pub fn reset_shortcuts(&mut self) {
        self.shortcuts = default_shortcuts();
    }

    pub fn apply_fill(&mut self, request: &FillRequest) -> Result<FillOutcome, CoreError> {
        self.apply_fill_with_cancel(request, || false)
    }

    pub fn apply_fill_with_light_table(
        &mut self,
        request: &FillRequest,
        use_boundary: bool,
        use_sampled_color: bool,
    ) -> Result<FillOutcome, CoreError> {
        self.apply_fill_internal(request, use_boundary, use_sampled_color, || false)
    }

    pub fn apply_fill_with_light_table_and_cancel(
        &mut self,
        request: &FillRequest,
        use_boundary: bool,
        use_sampled_color: bool,
        is_cancelled: impl FnMut() -> bool,
    ) -> Result<FillOutcome, CoreError> {
        self.apply_fill_internal(request, use_boundary, use_sampled_color, is_cancelled)
    }

    pub fn apply_fill_with_cancel(
        &mut self,
        request: &FillRequest,
        is_cancelled: impl FnMut() -> bool,
    ) -> Result<FillOutcome, CoreError> {
        self.apply_fill_internal(request, false, false, is_cancelled)
    }

    fn apply_fill_internal(
        &mut self,
        request: &FillRequest,
        use_light_table_boundary: bool,
        use_light_table_color: bool,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<FillOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        ensure_editable_role(document, ActivePlane::Color)?;
        let document_pixels = u64::from(document.width)
            .checked_mul(u64::from(document.height))
            .ok_or(CoreError::InvalidArgument("fill work size overflows"))?;
        if document_pixels > MAX_FILL_PIXELS {
            return Err(CoreError::InvalidArgument(
                "fill document exceeds the bounded work limit",
            ));
        }
        let selection = request
            .selection
            .map(|rect| {
                selection_from_rect(document.width, document.height, rect, &mut is_cancelled)
            })
            .transpose()?;
        let options = FillOptions {
            tolerance: request.tolerance,
            detached_regions: request.detached_regions,
            overflow_abort: request.overflow_abort,
            gap_close: request.gap_close,
            transparent_only: request.transparent_only,
            inclusion_mode: request.inclusion_mode,
            inclusion_colors: request.inclusion_colors.clone(),
        };
        let light_boundary = if use_light_table_boundary {
            let mut raster = document.raster(ActivePlane::MainLine).clone();
            for y in 0..document.height {
                if is_cancelled() {
                    return Err(CoreError::Cancelled);
                }
                for x in 0..document.width {
                    if document
                        .light_table
                        .sample(document.frames.reference_frame, x, y)?
                        .is_some()
                    {
                        let boundary = match raster.format() {
                            PixelFormat::BinaryMask8 => PixelValue::Binary(255),
                            PixelFormat::Grayscale8 => PixelValue::Grayscale8(255),
                            PixelFormat::Grayscale16 => PixelValue::Grayscale16(u16::MAX),
                            _ => {
                                return Err(CoreError::InvalidState(
                                    "main-line format cannot hold a light-table boundary",
                                ));
                            }
                        };
                        raster.set_pixel(x, y, boundary, self.document_revision)?;
                    }
                }
            }
            Some(raster)
        } else {
            None
        };
        let main_line = light_boundary
            .as_ref()
            .unwrap_or_else(|| document.raster(ActivePlane::MainLine));
        let fill_color = if use_light_table_color {
            let sampled = document
                .light_table
                .sample(
                    document.frames.reference_frame,
                    request.seed_x,
                    request.seed_y,
                )?
                .ok_or(CoreError::InvalidState(
                    "light-table fill color is unavailable at the seed",
                ))?;
            match (document.raster(ActivePlane::Color).format(), sampled) {
                (PixelFormat::StraightRgba8, PixelValue::Rgba(value)) => PixelValue::Rgba(value),
                (PixelFormat::StraightRgba16, PixelValue::Rgba16(value)) => {
                    PixelValue::Rgba16(value)
                }
                (PixelFormat::StraightRgba16, PixelValue::Rgba(value)) => PixelValue::Rgba16([
                    u16::from(value[0]) * 257,
                    u16::from(value[1]) * 257,
                    u16::from(value[2]) * 257,
                    u16::from(value[3]) * 257,
                ]),
                _ => {
                    return Err(CoreError::InvalidState(
                        "light-table fill color does not match the color plane",
                    ));
                }
            }
        } else {
            request.color
        };
        let plan = match request.operation {
            FillOperation::Seed => seed_fill_with_cancel(
                main_line,
                document.raster(ActivePlane::Color),
                selection.as_ref(),
                (request.seed_x, request.seed_y),
                fill_color,
                &options,
                &mut is_cancelled,
            )?,
            FillOperation::ClosedRegion => {
                let operation = selection.as_ref().ok_or(CoreError::InvalidArgument(
                    "closed-region fill requires an operation selection",
                ))?;
                closed_region_fill_with_cancel(
                    main_line,
                    document.raster(ActivePlane::Color),
                    operation,
                    fill_color,
                    &options,
                    &mut is_cancelled,
                )?
            }
            FillOperation::Extend => {
                let operation = selection.as_ref().ok_or(CoreError::InvalidArgument(
                    "fill extension requires an operation selection",
                ))?;
                extend_fill_with_cancel(
                    document.raster(ActivePlane::Color),
                    operation,
                    (request.seed_x, request.seed_y),
                    request.extension_distance,
                    &mut is_cancelled,
                )?
            }
        };
        if plan.edits.is_empty() {
            return Ok(FillOutcome {
                dispatch: DispatchOutcome {
                    revision: self.document_revision,
                    accepted_commands: 1,
                },
                changed_pixels: 0,
            });
        }

        let changed_pixels = u64::try_from(plan.edits.len())
            .map_err(|_| CoreError::InvalidState("fill edit count is not representable"))?;
        let mut next_color = document.raster(ActivePlane::Color).clone();
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        let mut changes = Vec::with_capacity(plan.edits.len());
        let mut touched = BTreeSet::new();
        for edit in plan.edits {
            next_color.set_pixel(edit.x, edit.y, edit.after, revision)?;
            touched.insert(TileCoord {
                x: edit.x / TILE_SIZE,
                y: edit.y / TILE_SIZE,
            });
            changes.push(PixelChange {
                x: edit.x,
                y: edit.y,
                before: edit.before,
                after: edit.after,
            });
        }
        for coord in touched {
            next_color.remove_tile_if_empty(coord);
        }
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let color_plane = document.plane_for_role_mut(ActivePlane::Color)?;
        let color_plane_id = color_plane.id;
        color_plane.raster = next_color;
        document.active_plane_id = color_plane_id;
        self.document_revision = revision;
        self.commit_pixel_history(color_plane_id, changes, after_state);
        Ok(FillOutcome {
            dispatch: DispatchOutcome {
                revision,
                accepted_commands: 1,
            },
            changed_pixels,
        })
    }

    pub fn eyedropper(
        &self,
        source: EyedropperSource,
        x: u32,
        y: u32,
    ) -> Result<PixelValue, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if source == EyedropperSource::LightTableTopmost {
            return document
                .light_table
                .sample(document.frames.reference_frame, x, y)?
                .ok_or(CoreError::InvalidState(
                    "eyedropper source is transparent or unavailable",
                ));
        }
        let line = PlaneSample {
            raster: document.raster(ActivePlane::MainLine),
            base_color: Some(document.main_line_color),
        };
        let color = PlaneSample {
            raster: document.raster(ActivePlane::Color),
            base_color: None,
        };
        let selected = match document.active_plane_role() {
            ActivePlane::MainLine => line,
            ActivePlane::Color => color,
        };
        eyedropper(source, x, y, selected, &[line, color], &[])?.ok_or(CoreError::InvalidState(
            "eyedropper source is transparent or unavailable",
        ))
    }

    pub fn palette(&self) -> Result<&[PixelValue], CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .palette
            .colors())
    }

    pub fn replace_palette(&mut self, colors: &[PixelValue]) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let mut after = Palette::default();
        for color in colors {
            after.push(*color)?;
        }
        let before = self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .palette
            .clone();
        if before == after {
            return Ok(DispatchOutcome {
                revision: self.document_revision,
                accepted_commands: 1,
            });
        }
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        self.document.as_mut().ok_or(CoreError::NoDocument)?.palette = after.clone();
        self.document_revision = revision;
        self.commit_history_change(HistoryChange::Palette { before, after }, after_state);
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn main_line_color(&self) -> Result<PixelValue, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .main_line_color)
    }

    pub fn set_main_line_color(&mut self, color: PixelValue) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if color.rgba16().is_none() {
            return Err(CoreError::InvalidArgument(
                "main-line base color must be RGBA",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        ensure_editable_role(document, ActivePlane::MainLine)?;
        if !matches!(
            document.raster(ActivePlane::MainLine).format(),
            PixelFormat::Grayscale8 | PixelFormat::Grayscale16
        ) {
            return Err(CoreError::InvalidState(
                "main-line base color is editable only for a grayscale main plane",
            ));
        }
        let before = document.main_line_color;
        if before == color {
            return Ok(DispatchOutcome {
                revision: self.document_revision,
                accepted_commands: 1,
            });
        }
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        self.document
            .as_mut()
            .ok_or(CoreError::NoDocument)?
            .main_line_color = color;
        self.document_revision = revision;
        self.render_cache.clear();
        self.commit_history_change(
            HistoryChange::MainLineColor {
                before,
                after: color,
            },
            after_state,
        );
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn set_color_check(
        &mut self,
        mode: Option<ColorCheckMode>,
    ) -> Result<ViewState, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document.is_none() {
            return Err(CoreError::NoDocument);
        }
        if self.color_check != mode {
            self.color_check = mode;
            self.view.revision = self
                .view
                .revision
                .checked_add(1)
                .ok_or(CoreError::InvalidState("view revision overflow"))?;
            self.render_cache.clear();
        }
        Ok(self.view)
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
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        ensure_editable_role(&document, stroke.plane)?;
        let samples = document_samples_for_view(
            self.view,
            stroke.coordinate_space,
            &stroke.samples,
            document.width,
            document.height,
        )?;
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
        let samples = document_samples_for_view(
            self.view,
            session.stroke.coordinate_space,
            samples,
            session.preview_document.width,
            session.preview_document.height,
        )?;
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
        let plane_id = document.plane_for_role(session.stroke.plane)?.id;
        document.active_plane_id = plane_id;
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
        self.commit_pixel_history(plane_id, changes, after_state);
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
        self.recovered = false;
        self.document_info()
    }

    pub fn autosave(&self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        inkpod_format::save_recovery_atomic(path, &document.to_file())?;
        self.document_info()
    }

    pub fn open(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let file = inkpod_format::read(path)?;
        let revision = self.next_document_revision()?;
        let document = CellDocument::from_file(file, revision)?;
        let max_id = document.max_stable_id();
        self.next_id = self.next_id.max(max_id.saturating_add(1));
        self.document = Some(document);
        self.render_cache.clear();
        self.document_revision = revision;
        self.reset_history(true);
        self.reset_view();
        self.current_path = Some(path.to_path_buf());
        self.recovered = false;
        self.color_check = None;
        self.secondary_views.clear();
        self.floating = None;
        self.sequence = None;
        self.motion_check = None;
        self.subpalette_index = None;
        self.document_info()
    }

    pub fn open_recovery(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let file = inkpod_format::read(path)?;
        let revision = self.next_document_revision()?;
        let document = CellDocument::from_file(file, revision)?;
        let max_id = document.max_stable_id();
        self.next_id = self.next_id.max(max_id.saturating_add(1));
        self.document = Some(document);
        self.render_cache.clear();
        self.document_revision = revision;
        // Recovery content is deliberately an unsaved document. A subsequent
        // explicit save needs a caller-selected destination.
        self.reset_history(false);
        self.reset_view();
        self.current_path = None;
        self.recovered = true;
        self.color_check = None;
        self.secondary_views.clear();
        self.floating = None;
        self.sequence = None;
        self.motion_check = None;
        self.subpalette_index = None;
        self.document_info()
    }

    pub fn recovery_is_newer(
        &self,
        normal_path: &Path,
        recovery_path: &Path,
    ) -> Result<bool, CoreError> {
        Ok(inkpod_format::recovery_is_newer(
            normal_path,
            recovery_path,
        )?)
    }

    pub fn discard_recovery(&self, path: &Path) -> Result<(), CoreError> {
        inkpod_format::discard_recovery(path)?;
        Ok(())
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
        let toggle_changed = match command {
            ViewCommand::Flip { axis } => {
                match axis {
                    MirrorAxis::Horizontal => {
                        self.view.flip_horizontal = !self.view.flip_horizontal
                    }
                    MirrorAxis::Vertical => self.view.flip_vertical = !self.view.flip_vertical,
                }
                true
            }
            ViewCommand::SetRulerVisible(value) => {
                let changed = self.view.ruler_visible != value;
                self.view.ruler_visible = value;
                changed
            }
            ViewCommand::SetGuidesVisible(value) => {
                let changed = self.view.guides_visible != value;
                self.view.guides_visible = value;
                changed
            }
            ViewCommand::SetGridVisible(value) => {
                let changed = self.view.grid_visible != value;
                self.view.grid_visible = value;
                changed
            }
            ViewCommand::SetSnapEnabled(value) => {
                let changed = self.view.snap_enabled != value;
                self.view.snap_enabled = value;
                changed
            }
            ViewCommand::SetTransparentView(value) => {
                let changed = self.view.transparent_view != value;
                self.view.transparent_view = value;
                changed
            }
            _ => false,
        };
        if matches!(
            command,
            ViewCommand::Flip { .. }
                | ViewCommand::SetRulerVisible(_)
                | ViewCommand::SetGuidesVisible(_)
                | ViewCommand::SetGridVisible(_)
                | ViewCommand::SetSnapEnabled(_)
                | ViewCommand::SetTransparentView(_)
        ) {
            if toggle_changed {
                self.view.revision = self
                    .view
                    .revision
                    .checked_add(1)
                    .ok_or(CoreError::InvalidState("view revision overflow"))?;
            }
            return Ok(self.view);
        }
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
            ViewCommand::BoxZoom {
                document_rect,
                viewport_width,
                viewport_height,
            } if valid_viewport(viewport_width, viewport_height)
                && document_rect.width > 0
                && document_rect.height > 0 =>
            {
                let zoom = (viewport_width / f64::from(document_rect.width))
                    .min(viewport_height / f64::from(document_rect.height))
                    .clamp(MIN_ZOOM, MAX_ZOOM);
                (
                    zoom,
                    (viewport_width - f64::from(document_rect.width) * zoom) / 2.0
                        - f64::from(document_rect.x) * zoom,
                    (viewport_height - f64::from(document_rect.height) * zoom) / 2.0
                        - f64::from(document_rect.y) * zoom,
                    ViewMode::Manual,
                )
            }
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
        let (layer_id, main_plane_id, color_plane_id) = document.primary_ids();
        Ok(DocumentInfo {
            document_revision: self.document_revision,
            view_revision: self.view.revision,
            document_id: document.id,
            document_uuid: document.uuid,
            layer_id,
            main_plane_id,
            color_plane_id,
            width: document.width,
            height: document.height,
            dpi_x_milli: document.dpi_x_milli,
            dpi_y_milli: document.dpi_y_milli,
            frames: document.frames,
            dirty: self.savepoint != Some(self.current_state),
            can_undo: self.history_cursor > 0,
            can_redo: self.history_cursor < self.history.len(),
            active_plane: document.active_plane_role(),
            recovered: self.recovered,
            main_plane_checksum: document.raster(ActivePlane::MainLine).checksum(),
            color_plane_checksum: document.raster(ActivePlane::Color).checksum(),
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
                feature_flags: 0,
                view: self.view,
                document_width: 0,
                document_height: 0,
                guides: Vec::new(),
                grid: GridConfig::default(),
                tiles: Vec::new(),
                vector_segments: Vec::new(),
                vector_fills: Vec::new(),
            };
        };
        let snapshot_revision = self
            .active_stroke
            .as_ref()
            .map_or(self.document_revision, |session| session.preview_revision);
        let feature_flags = match self.color_check {
            Some(ColorCheckMode::LegacyWhiteTransparency) => {
                SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE
            }
            Some(ColorCheckMode::NativeAlpha) => SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA,
            None => 0,
        };
        let mut coords: BTreeSet<_> = document
            .layers
            .iter()
            .filter(|layer| layer.visible)
            .flat_map(|layer| layer.planes.iter())
            .filter(|plane| plane.visible)
            .flat_map(|plane| plane.raster.allocated_coords())
            .chain(document.selection.allocated_coords())
            .collect();
        if document.light_table.has_visible_items() {
            let tiles_x = document.width.div_ceil(TILE_SIZE);
            let tiles_y = document.height.div_ceil(TILE_SIZE);
            for y in 0..tiles_y {
                for x in 0..tiles_x {
                    coords.insert(TileCoord { x, y });
                }
            }
        }
        let mut tiles = Vec::with_capacity(coords.len());
        for coord in &coords {
            let source_revision = document
                .layers
                .iter()
                .filter(|layer| layer.visible)
                .flat_map(|layer| layer.planes.iter())
                .filter(|plane| plane.visible)
                .map(|plane| plane.raster.tile_revision(*coord))
                .max()
                .unwrap_or(0)
                .max(document.light_table.source_revision())
                .max(document.selection.tile_revision(*coord));
            if cache
                .get(coord)
                .is_none_or(|tile| tile.source_revision != source_revision)
            {
                let tile_revision = self.next_render_tile_revision;
                self.next_render_tile_revision =
                    self.next_render_tile_revision.wrapping_add(1).max(1);
                if let Some(tile) = compose_tile(
                    document,
                    *coord,
                    self.color_check,
                    source_revision,
                    tile_revision,
                ) {
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
        let (vector_segments, vector_fills) = document.vector.render_items(document);
        self.render_cache = cache;
        RenderSnapshot {
            revision: snapshot_revision,
            feature_flags,
            view: self.view,
            document_width,
            document_height,
            guides: document.guides.clone(),
            grid: document.grid,
            tiles,
            vector_segments,
            vector_fills,
        }
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1).max(1);
        id
    }

    const fn noop_outcome(&self) -> DispatchOutcome {
        DispatchOutcome {
            revision: self.document_revision,
            accepted_commands: 1,
        }
    }

    fn commit_document_edit(
        &mut self,
        before: CellDocument,
        after: CellDocument,
    ) -> Result<DispatchOutcome, CoreError> {
        let revision = self.next_document_revision()?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    fn commit_document_edit_with_revision(
        &mut self,
        before: CellDocument,
        after: CellDocument,
        revision: u64,
    ) -> Result<DispatchOutcome, CoreError> {
        if before == after {
            return Ok(self.noop_outcome());
        }
        let after_state = self.allocate_state()?;
        self.document = Some(after.clone());
        self.document_revision = revision;
        self.render_cache.clear();
        self.commit_history_change(
            HistoryChange::Document {
                before: Box::new(before),
                after: Box::new(after),
            },
            after_state,
        );
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
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

    fn commit_pixel_history(&mut self, plane_id: u64, changes: Vec<PixelChange>, after_state: u64) {
        self.commit_history_change(HistoryChange::Pixels { plane_id, changes }, after_state);
    }

    fn commit_history_change(&mut self, change: HistoryChange, after_state: u64) {
        self.history.truncate(self.history_cursor);
        let before_state = self.current_state;
        self.history.push(HistoryEntry {
            change,
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
        match &entry.change {
            HistoryChange::Pixels { plane_id, changes } => {
                document.active_plane_id = *plane_id;
                let raster = &mut document
                    .plane_by_id_mut(*plane_id)
                    .ok_or(CoreError::InvalidState("history plane no longer exists"))?
                    .raster;
                let mut touched = BTreeSet::new();
                for change in changes {
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
            }
            HistoryChange::Palette { before, after } => {
                document.palette = if use_after {
                    after.clone()
                } else {
                    before.clone()
                };
            }
            HistoryChange::MainLineColor { before, after } => {
                document.main_line_color = if use_after { *after } else { *before };
                self.render_cache.clear();
            }
            HistoryChange::Document { before, after } => {
                *document = if use_after {
                    (**after).clone()
                } else {
                    (**before).clone()
                };
                self.render_cache.clear();
            }
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
    width: u32,
    height: u32,
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
                    let (x, y) = device_to_document(
                        view,
                        width,
                        height,
                        f64::from(sample.x),
                        f64::from(sample.y),
                    )?;
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

fn raster_to_file_plane(id: u64, kind: FilePlaneKind, raster: &TileRaster) -> FilePlane {
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

fn selection_from_rect(
    width: u32,
    height: u32,
    rect: RectI32,
    is_cancelled: &mut impl FnMut() -> bool,
) -> Result<TileRaster, CoreError> {
    if rect.width <= 0 || rect.height <= 0 || rect.x < 0 || rect.y < 0 {
        return Err(CoreError::InvalidArgument(
            "selection rectangle must have a nonnegative origin and positive size",
        ));
    }
    let right = u32::try_from(rect.x)
        .ok()
        .and_then(|x| x.checked_add(rect.width as u32))
        .ok_or(CoreError::InvalidArgument("selection rectangle overflows"))?;
    let bottom = u32::try_from(rect.y)
        .ok()
        .and_then(|y| y.checked_add(rect.height as u32))
        .ok_or(CoreError::InvalidArgument("selection rectangle overflows"))?;
    if right > width || bottom > height {
        return Err(CoreError::InvalidArgument(
            "selection rectangle is outside the document",
        ));
    }
    let mut selection = TileRaster::new(width, height, PixelFormat::BinaryMask8)?;
    let mut work = 0_u64;
    for y in rect.y as u32..bottom {
        for x in rect.x as u32..right {
            work = work
                .checked_add(1)
                .ok_or(CoreError::InvalidArgument("selection work size overflows"))?;
            if work % 1_024 == 0 && is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            selection.set_pixel(x, y, PixelValue::Binary(255), 0)?;
        }
    }
    if is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(selection)
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
    let format = document.raster(stroke.plane).format();
    let (draw_value, erase_value) = match (stroke.plane, format) {
        (ActivePlane::MainLine, PixelFormat::BinaryMask8) => {
            (PixelValue::Binary(255), PixelValue::Binary(0))
        }
        (ActivePlane::MainLine, PixelFormat::Grayscale8) => {
            (PixelValue::Grayscale8(u8::MAX), PixelValue::Grayscale8(0))
        }
        (ActivePlane::MainLine, PixelFormat::Grayscale16) => (
            PixelValue::Grayscale16(u16::MAX),
            PixelValue::Grayscale16(0),
        ),
        (ActivePlane::Color, PixelFormat::StraightRgba8) => {
            (PixelValue::Rgba(stroke.color), PixelValue::Rgba([0; 4]))
        }
        (ActivePlane::Color, PixelFormat::StraightRgba16) => (
            PixelValue::Rgba16(stroke.color.map(|channel| u16::from(channel) * 257)),
            PixelValue::Rgba16([0; 4]),
        ),
        _ => {
            return Err(CoreError::InvalidState(
                "active plane pixel format does not support painting",
            ));
        }
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

fn validate_node_name(name: &str) -> Result<(), CoreError> {
    if name.is_empty() || name.len() > 1_024 || name.chars().any(char::is_control) {
        Err(CoreError::InvalidArgument("node name is invalid"))
    } else {
        Ok(())
    }
}

const fn is_coloring_layer(kind: LayerKind) -> bool {
    matches!(
        kind,
        LayerKind::BinaryColoring | LayerKind::GrayscaleColoring
    )
}

fn validate_plane_format(kind: PlaneType, format: PixelFormat) -> Result<(), CoreError> {
    let valid = match kind {
        PlaneType::MainLine => matches!(
            format,
            PixelFormat::BinaryMask8 | PixelFormat::Grayscale8 | PixelFormat::Grayscale16
        ),
        PlaneType::Color | PlaneType::Raster => matches!(
            format,
            PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
        ),
        PlaneType::Selection => format == PixelFormat::BinaryMask8,
        PlaneType::VectorMainLine | PlaneType::ColorTrace | PlaneType::VectorFill => {
            format == PixelFormat::StraightRgba8
        }
    };
    if valid {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "pixel format is not allowed for the plane type",
        ))
    }
}

fn validate_layer_kind(kind: LayerKind, planes: &[PlaneNode]) -> Result<(), CoreError> {
    for plane in planes {
        validate_plane_format(plane.kind, plane.raster.format())?;
    }
    let count = |kind| planes.iter().filter(|plane| plane.kind == kind).count();
    let valid = match kind {
        LayerKind::BinaryColoring => {
            count(PlaneType::MainLine) == 1
                && count(PlaneType::Color) == 1
                && count(PlaneType::Selection) == 0
                && planes
                    .iter()
                    .find(|plane| plane.kind == PlaneType::MainLine)
                    .is_some_and(|plane| plane.raster.format() == PixelFormat::BinaryMask8)
        }
        LayerKind::GrayscaleColoring => {
            count(PlaneType::MainLine) == 1
                && count(PlaneType::Color) == 1
                && count(PlaneType::Selection) == 0
                && planes
                    .iter()
                    .find(|plane| plane.kind == PlaneType::MainLine)
                    .is_some_and(|plane| {
                        matches!(
                            plane.raster.format(),
                            PixelFormat::Grayscale8 | PixelFormat::Grayscale16
                        )
                    })
        }
        LayerKind::Raster => {
            !planes.is_empty() && planes.iter().all(|plane| plane.kind == PlaneType::Raster)
        }
        LayerKind::Selection => {
            !planes.is_empty()
                && planes
                    .iter()
                    .all(|plane| plane.kind == PlaneType::Selection)
        }
        LayerKind::VectorColoring => {
            count(PlaneType::VectorMainLine) == 1
                && count(PlaneType::ColorTrace) >= 1
                && count(PlaneType::VectorFill) == 1
                && planes.iter().all(|plane| {
                    matches!(
                        plane.kind,
                        PlaneType::VectorMainLine
                            | PlaneType::ColorTrace
                            | PlaneType::VectorFill
                            | PlaneType::Raster
                    )
                })
        }
        LayerKind::Frame
        | LayerKind::VanishingPoint
        | LayerKind::Adjustment
        | LayerKind::Text
        | LayerKind::Annotation => planes.is_empty(),
    };
    if valid {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "layer and plane types form a disallowed combination",
        ))
    }
}

fn unique_layer_name(layers: &[LayerNode], requested: &str) -> String {
    if !layers.iter().any(|layer| layer.name == requested) {
        return requested.to_owned();
    }
    for suffix in 2..=MAX_LAYERS {
        let candidate = format!("{requested} {suffix}");
        if !layers.iter().any(|layer| layer.name == candidate) {
            return candidate;
        }
    }
    format!("{requested} {}", layers.len() + 1)
}

fn unique_plane_name(planes: &[PlaneNode], requested: &str) -> String {
    if !planes.iter().any(|plane| plane.name == requested) {
        return requested.to_owned();
    }
    for suffix in 2..=MAX_PLANES_PER_LAYER {
        let candidate = format!("{requested} {suffix}");
        if !planes.iter().any(|plane| plane.name == candidate) {
            return candidate;
        }
    }
    format!("{requested} {}", planes.len() + 1)
}

fn find_plane_indices(document: &CellDocument, plane_id: u64) -> Result<(usize, usize), CoreError> {
    document
        .layers
        .iter()
        .enumerate()
        .find_map(|(layer_index, layer)| {
            layer
                .planes
                .iter()
                .position(|plane| plane.id == plane_id)
                .map(|plane_index| (layer_index, plane_index))
        })
        .ok_or(CoreError::InvalidArgument("plane ID does not exist"))
}

fn ensure_editable_plane(document: &CellDocument, plane_id: u64) -> Result<(), CoreError> {
    let (layer_index, plane_index) = find_plane_indices(document, plane_id)?;
    if !document.layers[layer_index].editable
        || !document.layers[layer_index].planes[plane_index].editable
    {
        Err(CoreError::InvalidState(
            "active layer or plane is not editable",
        ))
    } else {
        Ok(())
    }
}

fn ensure_editable_role(document: &CellDocument, role: ActivePlane) -> Result<(), CoreError> {
    ensure_editable_plane(document, document.plane_for_role(role)?.id)
}

fn bounded_document_pixels(width: u32, height: u32) -> Result<u64, CoreError> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(CoreError::InvalidArgument("document pixel count overflows"))?;
    if pixels > MAX_FILL_PIXELS {
        Err(CoreError::InvalidArgument(
            "operation exceeds the bounded document work limit",
        ))
    } else {
        Ok(pixels)
    }
}

fn convert_main_line_raster(
    source: &TileRaster,
    grayscale: bool,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut destination = TileRaster::new(
        source.width(),
        source.height(),
        if grayscale {
            PixelFormat::Grayscale8
        } else {
            PixelFormat::BinaryMask8
        },
    )?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = match source.pixel(x, y)? {
                PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value,
                PixelValue::Grayscale16(value) => ((u32::from(value) + 128) / 257) as u8,
                _ => return Err(CoreError::InvalidState("main-line plane format is invalid")),
            };
            let value = if grayscale {
                PixelValue::Grayscale8(value)
            } else {
                PixelValue::Binary(if value >= 128 { 255 } else { 0 })
            };
            destination.set_pixel(x, y, value, revision)?;
        }
    }
    Ok(destination)
}

fn merge_raster(
    destination: &mut TileRaster,
    source: &TileRaster,
    revision: u64,
) -> Result<(), CoreError> {
    if destination.width() != source.width()
        || destination.height() != source.height()
        || destination.format() != source.format()
    {
        return Err(CoreError::InvalidArgument("merge raster formats differ"));
    }
    bounded_document_pixels(source.width(), source.height())?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let source_value = source.pixel(x, y)?;
            if source_value.is_zero() {
                continue;
            }
            let before = destination.pixel(x, y)?;
            let after = paste_value(
                before,
                source_value,
                match source.format() {
                    PixelFormat::BinaryMask8
                    | PixelFormat::Grayscale8
                    | PixelFormat::Grayscale16 => PlaneType::MainLine,
                    _ => PlaneType::Raster,
                },
            )?;
            destination.set_pixel(x, y, after, revision)?;
        }
    }
    Ok(())
}

fn mirror_raster(
    source: &TileRaster,
    axis: MirrorAxis,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut destination = TileRaster::new(source.width(), source.height(), source.format())?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = source.pixel(x, y)?;
            if value.is_zero() {
                continue;
            }
            let (destination_x, destination_y) = match axis {
                MirrorAxis::Horizontal => (source.width() - 1 - x, y),
                MirrorAxis::Vertical => (x, source.height() - 1 - y),
            };
            destination.set_pixel(destination_x, destination_y, value, revision)?;
        }
    }
    Ok(destination)
}

fn mirror_frame_metadata(
    frames: &mut FrameMetadata,
    width: u32,
    height: u32,
    axis: MirrorAxis,
) -> Result<(), CoreError> {
    let width = i32::try_from(width)
        .map_err(|_| CoreError::InvalidState("document width exceeds frame range"))?;
    let height = i32::try_from(height)
        .map_err(|_| CoreError::InvalidState("document height exceeds frame range"))?;
    for frame in [
        &mut frames.hundred_frame,
        &mut frames.reference_frame,
        &mut frames.drawing_frame,
        &mut frames.safe_frame,
    ] {
        match axis {
            MirrorAxis::Horizontal => frame.x = width - frame.x - frame.width,
            MirrorAxis::Vertical => frame.y = height - frame.y - frame.height,
        }
    }
    Ok(())
}

fn paste_value(
    destination: PixelValue,
    source: PixelValue,
    kind: PlaneType,
) -> Result<PixelValue, CoreError> {
    match (kind, destination, source) {
        (PlaneType::MainLine, PixelValue::Binary(left), PixelValue::Binary(right)) => {
            Ok(PixelValue::Binary(left.max(right)))
        }
        (PlaneType::MainLine, PixelValue::Grayscale8(left), PixelValue::Grayscale8(right)) => {
            Ok(PixelValue::Grayscale8(left.max(right)))
        }
        (PlaneType::MainLine, PixelValue::Grayscale16(left), PixelValue::Grayscale16(right)) => {
            Ok(PixelValue::Grayscale16(left.max(right)))
        }
        (_, PixelValue::Rgba(left), PixelValue::Rgba(right)) => {
            Ok(PixelValue::Rgba(blend_rgba_over(left, right)))
        }
        (_, PixelValue::Rgba16(left), PixelValue::Rgba16(right)) => {
            Ok(PixelValue::Rgba16(blend_rgba16_over(left, right)))
        }
        (_, left, right) if std::mem::discriminant(&left) == std::mem::discriminant(&right) => {
            Ok(if right.is_transparent() { left } else { right })
        }
        _ => Err(CoreError::InvalidArgument(
            "clipboard pixel type does not match destination",
        )),
    }
}

fn selection_mask_for_shape(
    document: &CellDocument,
    shape: &SelectionShape,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(document.width, document.height)?;
    let mut mask = TileRaster::new(document.width, document.height, PixelFormat::BinaryMask8)?;
    match shape {
        SelectionShape::Rectangle(rect) => {
            let clipped = clip_rect(*rect, document.width, document.height)?;
            for y in clipped.y..clipped.y + clipped.height {
                for x in clipped.x..clipped.x + clipped.width {
                    mask.set_pixel(x as u32, y as u32, PixelValue::Binary(255), revision)?;
                }
            }
        }
        SelectionShape::Ellipse(rect) => {
            let clipped = clip_rect(*rect, document.width, document.height)?;
            if rect.width <= 0 || rect.height <= 0 {
                return Err(CoreError::InvalidArgument("ellipse bounds are empty"));
            }
            let center_x = f64::from(rect.x) + f64::from(rect.width) / 2.0;
            let center_y = f64::from(rect.y) + f64::from(rect.height) / 2.0;
            let radius_x = f64::from(rect.width) / 2.0;
            let radius_y = f64::from(rect.height) / 2.0;
            for y in clipped.y..clipped.y + clipped.height {
                for x in clipped.x..clipped.x + clipped.width {
                    let normalized_x = (f64::from(x) + 0.5 - center_x) / radius_x;
                    let normalized_y = (f64::from(y) + 0.5 - center_y) / radius_y;
                    if normalized_x * normalized_x + normalized_y * normalized_y <= 1.0 {
                        mask.set_pixel(x as u32, y as u32, PixelValue::Binary(255), revision)?;
                    }
                }
            }
        }
        SelectionShape::Lasso(points) | SelectionShape::Polyline(points) => {
            validate_points(points, 3)?;
            for y in 0..document.height {
                for x in 0..document.width {
                    if point_in_polygon(f64::from(x) + 0.5, f64::from(y) + 0.5, points) {
                        mask.set_pixel(x, y, PixelValue::Binary(255), revision)?;
                    }
                }
            }
        }
        SelectionShape::Trace { points, diameter } => {
            validate_points(points, 1)?;
            if !diameter.is_finite() || *diameter <= 0.0 || *diameter > 4_096.0 {
                return Err(CoreError::InvalidArgument("trace diameter is invalid"));
            }
            let radius_squared = f64::from(*diameter) * f64::from(*diameter) / 4.0;
            for y in 0..document.height {
                for x in 0..document.width {
                    let px = f64::from(x) + 0.5;
                    let py = f64::from(y) + 0.5;
                    let selected = if points.len() == 1 {
                        distance_squared(px, py, f64::from(points[0].x), f64::from(points[0].y))
                            <= radius_squared
                    } else {
                        points.windows(2).any(|segment| {
                            distance_to_segment_squared(px, py, segment[0], segment[1])
                                <= radius_squared
                        })
                    };
                    if selected {
                        mask.set_pixel(x, y, PixelValue::Binary(255), revision)?;
                    }
                }
            }
        }
        SelectionShape::Wand {
            x,
            y,
            tolerance,
            gap_close,
        } => {
            if *x >= document.width || *y >= document.height || *gap_close > 64 {
                return Err(CoreError::InvalidArgument("wand settings are invalid"));
            }
            let source = document
                .plane_by_id(document.active_plane_id)
                .ok_or(CoreError::InvalidState("active plane is missing"))?;
            let target = source.raster.pixel(*x, *y)?;
            let mut visited = BTreeSet::new();
            let mut queue = VecDeque::from([(*x, *y)]);
            while let Some((candidate_x, candidate_y)) = queue.pop_front() {
                if !visited.insert((candidate_x, candidate_y)) {
                    continue;
                }
                let value = source.raster.pixel(candidate_x, candidate_y)?;
                if !pixel_within_tolerance(value, target, *tolerance) {
                    continue;
                }
                mask.set_pixel(candidate_x, candidate_y, PixelValue::Binary(255), revision)?;
                if candidate_x > 0 {
                    queue.push_back((candidate_x - 1, candidate_y));
                }
                if candidate_x + 1 < document.width {
                    queue.push_back((candidate_x + 1, candidate_y));
                }
                if candidate_y > 0 {
                    queue.push_back((candidate_x, candidate_y - 1));
                }
                if candidate_y + 1 < document.height {
                    queue.push_back((candidate_x, candidate_y + 1));
                }
            }
            if *gap_close > 0 {
                mask = morphology_selection(&mask, i32::from(*gap_close), revision)?;
                mask = morphology_selection(&mask, -i32::from(*gap_close), revision)?;
            }
        }
    }
    Ok(mask)
}

fn clip_rect(rect: RectI32, width: u32, height: u32) -> Result<RectI32, CoreError> {
    if rect.width <= 0 || rect.height <= 0 {
        return Err(CoreError::InvalidArgument("selection bounds are empty"));
    }
    let right = rect
        .x
        .checked_add(rect.width)
        .ok_or(CoreError::InvalidArgument("selection X bounds overflow"))?;
    let bottom = rect
        .y
        .checked_add(rect.height)
        .ok_or(CoreError::InvalidArgument("selection Y bounds overflow"))?;
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = right.min(width as i32);
    let bottom = bottom.min(height as i32);
    if left >= right || top >= bottom {
        return Err(CoreError::InvalidArgument(
            "selection is outside the document",
        ));
    }
    Ok(RectI32 {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

fn validate_points(points: &[PointF32], minimum: usize) -> Result<(), CoreError> {
    if points.len() < minimum
        || points.len() > 1_048_576
        || points.iter().any(|point| {
            !point.x.is_finite()
                || !point.y.is_finite()
                || point.x.abs() > MAX_STROKE_COORDINATE
                || point.y.abs() > MAX_STROKE_COORDINATE
        })
    {
        Err(CoreError::InvalidArgument(
            "selection point list is invalid",
        ))
    } else {
        Ok(())
    }
}

fn point_in_polygon(x: f64, y: f64, points: &[PointF32]) -> bool {
    let mut inside = false;
    let mut previous = points[points.len() - 1];
    for &current in points {
        let (x1, y1) = (f64::from(previous.x), f64::from(previous.y));
        let (x2, y2) = (f64::from(current.x), f64::from(current.y));
        if (y1 > y) != (y2 > y) {
            let crossing_x = (x2 - x1).mul_add((y - y1) / (y2 - y1), x1);
            if x < crossing_x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn distance_squared(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    (x1 - x2).mul_add(x1 - x2, (y1 - y2) * (y1 - y2))
}

fn distance_to_segment_squared(x: f64, y: f64, start: PointF32, end: PointF32) -> f64 {
    let start_x = f64::from(start.x);
    let start_y = f64::from(start.y);
    let delta_x = f64::from(end.x) - start_x;
    let delta_y = f64::from(end.y) - start_y;
    let length_squared = delta_x.mul_add(delta_x, delta_y * delta_y);
    if length_squared == 0.0 {
        return distance_squared(x, y, start_x, start_y);
    }
    let ratio =
        (((x - start_x) * delta_x + (y - start_y) * delta_y) / length_squared).clamp(0.0, 1.0);
    distance_squared(x, y, start_x + ratio * delta_x, start_y + ratio * delta_y)
}

fn combine_selection_masks(
    base: &TileRaster,
    candidate: &TileRaster,
    operation: SelectionOperation,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    if base.width() != candidate.width() || base.height() != candidate.height() {
        return Err(CoreError::InvalidArgument("selection dimensions differ"));
    }
    bounded_document_pixels(base.width(), base.height())?;
    let mut output = TileRaster::new(base.width(), base.height(), PixelFormat::BinaryMask8)?;
    for y in 0..base.height() {
        for x in 0..base.width() {
            let left = matches!(base.pixel(x, y)?, PixelValue::Binary(255));
            let right = matches!(candidate.pixel(x, y)?, PixelValue::Binary(255));
            let selected = match operation {
                SelectionOperation::New => right,
                SelectionOperation::Add => left || right,
                SelectionOperation::Subtract => left && !right,
                SelectionOperation::Intersect => left && right,
            };
            if selected {
                output.set_pixel(x, y, PixelValue::Binary(255), revision)?;
            }
        }
    }
    Ok(output)
}

fn invert_selection_mask(source: &TileRaster, revision: u64) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut output = TileRaster::new(source.width(), source.height(), PixelFormat::BinaryMask8)?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            if matches!(source.pixel(x, y)?, PixelValue::Binary(0)) {
                output.set_pixel(x, y, PixelValue::Binary(255), revision)?;
            }
        }
    }
    Ok(output)
}

fn morphology_selection(
    source: &TileRaster,
    pixels: i32,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    let document_pixels = bounded_document_pixels(source.width(), source.height())?;
    let steps = u64::from(pixels.unsigned_abs());
    if document_pixels.saturating_mul(steps.max(1)) > MAX_FILL_PIXELS {
        return Err(CoreError::InvalidArgument(
            "selection morphology exceeds the bounded work limit",
        ));
    }
    let mut current = source.clone();
    for _ in 0..steps {
        let mut next = TileRaster::new(source.width(), source.height(), PixelFormat::BinaryMask8)?;
        for y in 0..source.height() {
            for x in 0..source.width() {
                let selected = |candidate_x: u32, candidate_y: u32| {
                    matches!(
                        current.pixel(candidate_x, candidate_y),
                        Ok(PixelValue::Binary(255))
                    )
                };
                let value = if pixels > 0 {
                    selected(x, y)
                        || x.checked_sub(1).is_some_and(|left| selected(left, y))
                        || (x + 1 < source.width() && selected(x + 1, y))
                        || y.checked_sub(1).is_some_and(|top| selected(x, top))
                        || (y + 1 < source.height() && selected(x, y + 1))
                } else {
                    selected(x, y)
                        && x > 0
                        && selected(x - 1, y)
                        && x + 1 < source.width()
                        && selected(x + 1, y)
                        && y > 0
                        && selected(x, y - 1)
                        && y + 1 < source.height()
                        && selected(x, y + 1)
                };
                if value {
                    next.set_pixel(x, y, PixelValue::Binary(255), revision)?;
                }
            }
        }
        current = next;
    }
    Ok(current)
}

fn mask_bounds(mask: &TileRaster) -> Result<Option<RectI32>, CoreError> {
    bounded_document_pixels(mask.width(), mask.height())?;
    let mut min_x = mask.width();
    let mut min_y = mask.height();
    let mut max_x = 0;
    let mut max_y = 0;
    let mut any = false;
    for y in 0..mask.height() {
        for x in 0..mask.width() {
            if matches!(mask.pixel(x, y)?, PixelValue::Binary(255)) {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if !any {
        return Ok(None);
    }
    Ok(Some(RectI32 {
        x: min_x as i32,
        y: min_y as i32,
        width: (max_x - min_x + 1) as i32,
        height: (max_y - min_y + 1) as i32,
    }))
}

fn color_selection_mask(
    source: &TileRaster,
    color: PixelValue,
    tolerance: u16,
    different: bool,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut output = TileRaster::new(source.width(), source.height(), PixelFormat::BinaryMask8)?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            if pixel_within_tolerance(source.pixel(x, y)?, color, tolerance) != different {
                output.set_pixel(x, y, PixelValue::Binary(255), revision)?;
            }
        }
    }
    Ok(output)
}

fn pixel_within_tolerance(left: PixelValue, right: PixelValue, tolerance: u16) -> bool {
    let channels = |value| -> Option<[u16; 4]> {
        match value {
            PixelValue::Binary(value) | PixelValue::Grayscale8(value) => {
                let value = u16::from(value) * 257;
                Some([value, value, value, u16::MAX])
            }
            PixelValue::Grayscale16(value) => Some([value, value, value, u16::MAX]),
            PixelValue::Rgba(value) => Some(value.map(|channel| u16::from(channel) * 257)),
            PixelValue::Rgba16(value) => Some(value),
        }
    };
    let (Some(left), Some(right)) = (channels(left), channels(right)) else {
        return false;
    };
    left.into_iter()
        .zip(right)
        .all(|(left, right)| left.abs_diff(right) <= tolerance)
}

fn validate_floating_transform(transform: FloatingTransform) -> Result<(), CoreError> {
    if !transform.translate_x.is_finite()
        || !transform.translate_y.is_finite()
        || !transform.scale_x.is_finite()
        || !transform.scale_y.is_finite()
        || !transform.rotation_degrees.is_finite()
        || transform.scale_x.abs() < 0.000_001
        || transform.scale_y.abs() < 0.000_001
        || transform.scale_x.abs() > 1_024.0
        || transform.scale_y.abs() > 1_024.0
        || transform.translate_x.abs() > f64::from(MAX_STROKE_COORDINATE)
        || transform.translate_y.abs() > f64::from(MAX_STROKE_COORDINATE)
        || transform.rotation_degrees.abs() > 36_000.0
    {
        Err(CoreError::InvalidArgument(
            "floating transform is outside supported bounds",
        ))
    } else {
        Ok(())
    }
}

fn validate_guide_position(
    document: &CellDocument,
    axis: GuideAxis,
    position: i32,
) -> Result<(), CoreError> {
    let limit = match axis {
        GuideAxis::Horizontal => document.height,
        GuideAxis::Vertical => document.width,
    };
    if position < 0
        || u32::try_from(position)
            .ok()
            .is_none_or(|value| value > limit)
    {
        Err(CoreError::InvalidArgument(
            "guide position is outside paper",
        ))
    } else {
        Ok(())
    }
}

fn validate_grid(grid: GridConfig) -> Result<(), CoreError> {
    if grid.spacing_x == 0
        || grid.spacing_y == 0
        || grid.spacing_x > 1_048_576
        || grid.spacing_y > 1_048_576
        || grid.subdivisions == 0
        || grid.subdivisions > 1_024
    {
        Err(CoreError::InvalidArgument("grid values are outside bounds"))
    } else {
        Ok(())
    }
}

fn default_shortcuts() -> BTreeMap<u32, ShortcutBinding> {
    [
        ShortcutBinding {
            command_id: 1,
            virtual_key: u32::from(b'Z'),
            modifiers: 1,
        },
        ShortcutBinding {
            command_id: 2,
            virtual_key: u32::from(b'Y'),
            modifiers: 1,
        },
        ShortcutBinding {
            command_id: 3,
            virtual_key: u32::from(b'C'),
            modifiers: 1,
        },
        ShortcutBinding {
            command_id: 4,
            virtual_key: u32::from(b'V'),
            modifiers: 1,
        },
    ]
    .into_iter()
    .map(|binding| (binding.command_id, binding))
    .collect()
}

fn device_to_document(
    view: ViewState,
    width: u32,
    height: u32,
    device_x: f64,
    device_y: f64,
) -> Result<(f64, f64), CoreError> {
    if !device_x.is_finite() || !device_y.is_finite() {
        return Err(CoreError::InvalidArgument(
            "device coordinate is not finite",
        ));
    }
    let mut x = (device_x - view.pan_x) / view.zoom;
    let mut y = (device_y - view.pan_y) / view.zoom;
    if view.flip_horizontal {
        x = f64::from(width) - x;
    }
    if view.flip_vertical {
        y = f64::from(height) - y;
    }
    Ok((x, y))
}

fn compose_tile(
    document: &CellDocument,
    coord: TileCoord,
    color_check: Option<ColorCheckMode>,
    source_revision: u64,
    tile_revision: u64,
) -> Option<RenderTile> {
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
            let document_x = origin_x + x;
            let document_y = origin_y + y;
            let mut composite = document
                .light_table
                .composite(document.frames.reference_frame, document_x, document_y)
                .unwrap_or([0_u8; 4]);
            // Layer index zero is the top of the palette. Composite from the
            // bottom towards the top so palette order and rendered order agree.
            for layer in document.layers.iter().rev().filter(|layer| layer.visible) {
                let mut layer_pixel = [0_u8; 4];
                for plane in layer
                    .planes
                    .iter()
                    .filter(|plane| plane.visible && plane.kind != PlaneType::MainLine)
                    .chain(
                        layer
                            .planes
                            .iter()
                            .filter(|plane| plane.visible && plane.kind == PlaneType::MainLine),
                    )
                {
                    let value = plane.raster.pixel(document_x, document_y).ok()?;
                    let mut rgba = match plane.kind {
                        PlaneType::MainLine => {
                            let coverage = match value {
                                PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value,
                                PixelValue::Grayscale16(value) => {
                                    ((u32::from(value) + 128) / 257) as u8
                                }
                                _ => return None,
                            };
                            let mut line = rgba8_for_display(document.main_line_color)?;
                            line[3] =
                                ((u32::from(line[3]) * u32::from(coverage) + 127) / 255) as u8;
                            line
                        }
                        PlaneType::Color | PlaneType::Raster => rgba8_for_display(value)?,
                        PlaneType::Selection => {
                            let coverage = match value {
                                PixelValue::Binary(value) => value,
                                _ => return None,
                            };
                            [0, 160, 255, coverage / 3]
                        }
                        PlaneType::VectorMainLine
                        | PlaneType::ColorTrace
                        | PlaneType::VectorFill => [0, 0, 0, 0],
                    };
                    rgba[3] = ((u32::from(rgba[3]) * plane.opacity_milli + 500) / 1_000) as u8;
                    layer_pixel = blend_rgba_over(layer_pixel, rgba);
                }
                layer_pixel[3] =
                    ((u32::from(layer_pixel[3]) * layer.opacity_milli + 500) / 1_000) as u8;
                composite = blend_rgba_over(composite, layer_pixel);
            }
            if let Some(mode) = color_check {
                let check_value = PixelValue::Rgba(composite);
                let check_pixel = match color_check_category(check_value, mode) {
                    ColorCheckCategory::ExactWhite => [255, 255, 255, 255],
                    ColorCheckCategory::Transparent => [255, 0, 255, 255],
                    ColorCheckCategory::Colored => [0, 0, 0, 255],
                };
                pixels.extend_from_slice(&check_pixel);
                continue;
            }
            if matches!(
                document.selection.pixel(document_x, document_y).ok()?,
                PixelValue::Binary(255)
            ) {
                composite = blend_rgba_over(composite, [0, 160, 255, 64]);
            }
            let alpha = u32::from(composite[3]);
            let premultiply =
                |channel: u8| -> u8 { ((u32::from(channel) * alpha + 127) / 255) as u8 };
            pixels.extend_from_slice(&[
                premultiply(composite[2]),
                premultiply(composite[1]),
                premultiply(composite[0]),
                composite[3],
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
        source_revision,
        tile_revision,
    })
}

fn blend_rgba_over(background: [u8; 4], foreground: [u8; 4]) -> [u8; 4] {
    let foreground_alpha = u32::from(foreground[3]);
    let background_alpha = u32::from(background[3]);
    let inverse = 255 - foreground_alpha;
    let output_alpha = foreground_alpha + (background_alpha * inverse + 127) / 255;
    if output_alpha == 0 {
        return [0; 4];
    }
    let channel = |index: usize| -> u8 {
        let foreground_premultiplied = u32::from(foreground[index]) * foreground_alpha;
        let background_premultiplied = u32::from(background[index]) * background_alpha;
        ((foreground_premultiplied
            + (background_premultiplied * inverse + 127) / 255
            + output_alpha / 2)
            / output_alpha) as u8
    };
    [channel(0), channel(1), channel(2), output_alpha as u8]
}

fn blend_rgba16_over(background: [u16; 4], foreground: [u16; 4]) -> [u16; 4] {
    let foreground_alpha = u64::from(foreground[3]);
    let background_alpha = u64::from(background[3]);
    let inverse = u64::from(u16::MAX) - foreground_alpha;
    let output_alpha =
        foreground_alpha + (background_alpha * inverse + 32_767) / u64::from(u16::MAX);
    if output_alpha == 0 {
        return [0; 4];
    }
    let channel = |index: usize| -> u16 {
        let foreground_premultiplied = u64::from(foreground[index]) * foreground_alpha;
        let background_premultiplied = u64::from(background[index]) * background_alpha;
        ((foreground_premultiplied
            + (background_premultiplied * inverse + 32_767) / u64::from(u16::MAX)
            + output_alpha / 2)
            / output_alpha) as u16
    };
    [channel(0), channel(1), channel(2), output_alpha as u16]
}

fn rgba8_for_display(value: PixelValue) -> Option<[u8; 4]> {
    match value {
        PixelValue::Rgba(value) => Some(value),
        PixelValue::Rgba16(value) => {
            Some(value.map(|channel| ((u32::from(channel) + 128) / 257) as u8))
        }
        _ => None,
    }
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

    fn fill_request(seed_x: u32, seed_y: u32, color: [u8; 4]) -> FillRequest {
        FillRequest {
            operation: FillOperation::Seed,
            seed_x,
            seed_y,
            color: PixelValue::Rgba(color),
            selection: None,
            tolerance: 0,
            detached_regions: false,
            overflow_abort: false,
            gap_close: 0,
            transparent_only: false,
            inclusion_mode: InclusionMode::None,
            inclusion_colors: Vec::new(),
            extension_distance: 0,
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
    fn m2_fill_is_one_atomic_history_unit_and_never_changes_main_line() {
        let mut core = Core::new();
        core.new_cell(9, 9, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        for samples in [
            vec![
                StrokeSample {
                    x: 1.0,
                    y: 1.0,
                    pressure: 1.0,
                },
                StrokeSample {
                    x: 7.0,
                    y: 1.0,
                    pressure: 1.0,
                },
            ],
            vec![
                StrokeSample {
                    x: 7.0,
                    y: 1.0,
                    pressure: 1.0,
                },
                StrokeSample {
                    x: 7.0,
                    y: 7.0,
                    pressure: 1.0,
                },
            ],
            vec![
                StrokeSample {
                    x: 7.0,
                    y: 7.0,
                    pressure: 1.0,
                },
                StrokeSample {
                    x: 1.0,
                    y: 7.0,
                    pressure: 1.0,
                },
            ],
            vec![
                StrokeSample {
                    x: 1.0,
                    y: 7.0,
                    pressure: 1.0,
                },
                StrokeSample {
                    x: 1.0,
                    y: 1.0,
                    pressure: 1.0,
                },
            ],
        ] {
            core.apply_stroke(&line_stroke(samples)).unwrap();
        }
        let before = core.document_info().unwrap();
        let fill = PixelValue::Rgba([20, 90, 180, 255]);
        let outcome = core
            .apply_fill(&fill_request(4, 4, [20, 90, 180, 255]))
            .unwrap();
        let after = core.document_info().unwrap();
        assert_eq!(outcome.changed_pixels, 25);
        assert_eq!(after.document_revision, before.document_revision + 1);
        assert_eq!(after.main_plane_checksum, before.main_plane_checksum);
        assert_eq!(core.plane_pixel(ActivePlane::Color, 4, 4).unwrap(), fill);

        core.undo().unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 4, 4).unwrap(),
            PixelValue::Rgba([0; 4])
        );
        core.redo().unwrap();
        assert_eq!(core.plane_pixel(ActivePlane::Color, 4, 4).unwrap(), fill);
    }

    #[test]
    fn m2_overflow_invalid_cancel_and_noop_do_not_commit_partial_fill() {
        let mut core = Core::new();
        let created = core
            .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let mut request = fill_request(4, 4, [1, 2, 3, 255]);
        request.overflow_abort = true;
        assert!(matches!(
            core.apply_fill(&request),
            Err(CoreError::FillOverflow { .. })
        ));
        assert_eq!(core.document_info().unwrap(), created);

        request.overflow_abort = false;
        assert!(matches!(
            core.apply_fill_with_cancel(&request, || true),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(core.document_info().unwrap(), created);

        request.selection = Some(RectI32 {
            x: 2,
            y: 2,
            width: 2,
            height: 2,
        });
        request.seed_x = 2;
        request.seed_y = 2;
        let first = core.apply_fill(&request).unwrap();
        assert_eq!(first.changed_pixels, 4);
        let before_noop = core.document_info().unwrap();
        let second = core.apply_fill(&request).unwrap();
        assert_eq!(second.changed_pixels, 0);
        assert_eq!(core.document_info().unwrap(), before_noop);
    }

    #[test]
    fn m2_autosave_recovery_never_inherits_or_overwrites_normal_path() {
        let suffix = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir();
        let normal = directory.join(format!(
            "inkpod-m2-normal-{}-{suffix}.inkpod",
            std::process::id()
        ));
        let recovery = directory.join(format!(
            "inkpod-m2-recovery-{}-{suffix}.inkpod",
            std::process::id()
        ));
        let restored = directory.join(format!(
            "inkpod-m2-restored-{}-{suffix}.inkpod",
            std::process::id()
        ));

        let mut core = Core::new();
        core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.save(&normal).unwrap();
        let normal_bytes = fs::read(&normal).unwrap();
        let mut request = fill_request(3, 3, [9, 8, 7, 255]);
        request.selection = Some(RectI32 {
            x: 2,
            y: 2,
            width: 2,
            height: 2,
        });
        core.apply_fill(&request).unwrap();
        let before_autosave = core.document_info().unwrap();
        let after_autosave = core.autosave(&recovery).unwrap();
        assert_eq!(after_autosave, before_autosave);
        assert!(after_autosave.dirty);
        assert_eq!(fs::read(&normal).unwrap(), normal_bytes);

        let mut recovered = Core::new();
        let recovered_info = recovered.open_recovery(&recovery).unwrap();
        assert!(recovered_info.recovered);
        assert!(recovered_info.dirty);
        assert!(matches!(
            recovered.revert(),
            Err(CoreError::InvalidState(_))
        ));
        assert_eq!(fs::read(&normal).unwrap(), normal_bytes);
        recovered.save(&restored).unwrap();
        assert_eq!(fs::read(&normal).unwrap(), normal_bytes);
        assert_ne!(fs::read(&restored).unwrap(), normal_bytes);

        fs::remove_file(normal).unwrap();
        fs::remove_file(recovery).unwrap();
        fs::remove_file(restored).unwrap();
    }

    #[test]
    fn m2_grayscale_eyedropper_and_color_check_are_view_only() {
        let mut core = Core::new();
        core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let document = core.document.as_mut().unwrap();
        document.layers[0].kind = LayerKind::GrayscaleColoring;
        document.layers[0].planes[0].raster =
            TileRaster::new(4, 4, PixelFormat::Grayscale8).unwrap();
        document.layers[0].planes[0]
            .raster
            .set_pixel(1, 1, PixelValue::Grayscale8(128), 2)
            .unwrap();
        document.active_plane_id = document.layers[0].planes[0].id;
        let line_color = PixelValue::Rgba16([1_001, 2_002, 3_003, 65_535]);
        core.set_main_line_color(line_color).unwrap();
        assert_eq!(
            core.eyedropper(EyedropperSource::SelectedPlane, 1, 1)
                .unwrap(),
            line_color
        );
        let normal_snapshot = core.build_snapshot();
        let normal_tile_revision = normal_snapshot.tiles()[0].tile_revision();
        let before = core.document_info().unwrap();
        core.set_color_check(Some(ColorCheckMode::NativeAlpha))
            .unwrap();
        let after = core.document_info().unwrap();
        assert_eq!(after.document_revision, before.document_revision);
        assert_eq!(after.main_plane_checksum, before.main_plane_checksum);
        assert!(after.view_revision > before.view_revision);
        let check_snapshot = core.build_snapshot();
        assert_eq!(
            check_snapshot.feature_flags(),
            SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA
        );
        assert_ne!(
            check_snapshot.tiles()[0].tile_revision(),
            normal_tile_revision
        );

        let palette = [
            PixelValue::Rgba([12, 34, 56, 255]),
            PixelValue::Rgba16([1, 257, 32_769, 65_534]),
        ];
        core.replace_palette(&palette).unwrap();
        assert_eq!(core.palette().unwrap(), palette);
        core.undo().unwrap();
        assert!(core.palette().unwrap().is_empty());
        core.redo().unwrap();
        assert_eq!(core.palette().unwrap(), palette);

        let path = std::env::temp_dir().join(format!(
            "inkpod-core-m2-color-metadata-{}-{}.inkpod",
            std::process::id(),
            core.document_info().unwrap().document_revision
        ));
        core.save(&path).unwrap();
        let mut reopened = Core::new();
        reopened.open(&path).unwrap();
        assert_eq!(reopened.main_line_color().unwrap(), line_color);
        assert_eq!(reopened.palette().unwrap(), palette);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn m2_fill_rejects_oversized_documents_before_materializing_selection() {
        let mut core = Core::new();
        core.new_cell(
            inkpod_image::MAX_RASTER_DIMENSION,
            inkpod_image::MAX_RASTER_DIMENSION,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
        )
        .unwrap();
        let mut request = fill_request(0, 0, [1, 2, 3, 255]);
        request.selection = Some(RectI32 {
            x: 0,
            y: 0,
            width: i32::try_from(inkpod_image::MAX_RASTER_DIMENSION).unwrap(),
            height: i32::try_from(inkpod_image::MAX_RASTER_DIMENSION).unwrap(),
        });
        assert!(matches!(
            core.apply_fill(&request),
            Err(CoreError::InvalidArgument(
                "fill document exceeds the bounded work limit"
            ))
        ));
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

    #[test]
    fn m3_acceptance_layer_tree_undo_redo_save_reopen_and_validation() {
        let mut core = Core::new();
        let created = core
            .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let base_layer = created.layer_id;
        let (_, duplicate) = core.duplicate_layer(base_layer).unwrap();
        core.undo().unwrap();
        assert_eq!(core.layers().unwrap().len(), 1);
        core.redo().unwrap();
        assert_eq!(core.layers().unwrap().len(), 2);
        core.reorder_layer(duplicate, 0).unwrap();
        core.undo().unwrap();
        assert_eq!(core.layers().unwrap()[1].id, duplicate);
        core.redo().unwrap();
        let saved_order: Vec<_> = core
            .layers()
            .unwrap()
            .iter()
            .map(|layer| layer.id)
            .collect();
        assert_eq!(saved_order, vec![duplicate, base_layer]);

        let path = std::env::temp_dir().join(format!(
            "inkpod-m3-tree-{}-{}.inkpod",
            std::process::id(),
            TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        core.save(&path).unwrap();
        core.delete_layer(duplicate).unwrap();
        assert_eq!(core.layers().unwrap().len(), 1);
        core.undo().unwrap();
        assert_eq!(
            core.layers()
                .unwrap()
                .iter()
                .map(|layer| layer.id)
                .collect::<Vec<_>>(),
            saved_order
        );
        core.redo().unwrap();
        assert_eq!(core.layers().unwrap().len(), 1);
        core.save(&path).unwrap();

        let mut reopened = Core::new();
        reopened.open(&path).unwrap();
        assert_eq!(reopened.layers().unwrap().len(), 1);
        reopened.undo().unwrap_err();

        let revision = reopened.document_info().unwrap().document_revision;
        assert!(matches!(
            reopened.create_plane(
                base_layer,
                PlaneType::Selection,
                PixelFormat::BinaryMask8,
                "Invalid Selection"
            ),
            Err(CoreError::InvalidArgument(_))
        ));
        assert_eq!(
            reopened.document_info().unwrap().document_revision,
            revision
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn m3_acceptance_selection_boolean_property_and_authoring_tools() {
        fn mask(bits: u8) -> TileRaster {
            let mut mask = TileRaster::new(8, 1, PixelFormat::BinaryMask8).unwrap();
            for x in 0..8 {
                if bits & (1 << x) != 0 {
                    mask.set_pixel(x, 0, PixelValue::Binary(255), 1).unwrap();
                }
            }
            mask
        }
        for left in 0_u8..=u8::MAX {
            for right in [0_u8, 0x55, 0xaa, u8::MAX] {
                let left_mask = mask(left);
                let right_mask = mask(right);
                for (operation, expected) in [
                    (SelectionOperation::New, right),
                    (SelectionOperation::Add, left | right),
                    (SelectionOperation::Subtract, left & !right),
                    (SelectionOperation::Intersect, left & right),
                ] {
                    let combined =
                        combine_selection_masks(&left_mask, &right_mask, operation, 2).unwrap();
                    for x in 0..8 {
                        assert_eq!(
                            matches!(combined.pixel(x, 0).unwrap(), PixelValue::Binary(255)),
                            expected & (1 << x) != 0
                        );
                    }
                }
            }
        }

        let mut core = Core::new();
        core.new_cell(12, 12, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_selection(
            &SelectionShape::Ellipse(RectI32 {
                x: 2,
                y: 2,
                width: 8,
                height: 6,
            }),
            SelectionOperation::New,
        )
        .unwrap();
        assert!(core.selection_bounds().unwrap().is_some());
        core.apply_selection(
            &SelectionShape::Polyline(vec![
                PointF32 { x: 1.0, y: 1.0 },
                PointF32 { x: 10.0, y: 1.0 },
                PointF32 { x: 5.0, y: 10.0 },
            ]),
            SelectionOperation::New,
        )
        .unwrap();
        assert!(core.selection_bounds().unwrap().is_some());
        core.apply_selection(
            &SelectionShape::Wand {
                x: 0,
                y: 0,
                tolerance: 0,
                gap_close: 0,
            },
            SelectionOperation::New,
        )
        .unwrap();
        assert_eq!(
            core.selection_bounds().unwrap(),
            Some(RectI32 {
                x: 0,
                y: 0,
                width: 12,
                height: 12,
            })
        );
        core.select_color(PixelValue::Binary(0), 0, false, SelectionOperation::New)
            .unwrap();
        assert_eq!(core.selection_bounds().unwrap().unwrap().width, 12);
        core.select_color(PixelValue::Binary(0), 0, true, SelectionOperation::New)
            .unwrap();
        assert_eq!(core.selection_bounds().unwrap(), None);
        core.apply_selection(
            &SelectionShape::Lasso(vec![
                PointF32 { x: 2.0, y: 2.0 },
                PointF32 { x: 9.0, y: 2.0 },
                PointF32 { x: 6.0, y: 9.0 },
            ]),
            SelectionOperation::New,
        )
        .unwrap();
        let lasso = core.selection_bounds().unwrap().unwrap();
        assert!(lasso.width >= 6 && lasso.height >= 6);
        core.apply_selection(
            &SelectionShape::Trace {
                points: vec![PointF32 { x: 0.0, y: 0.0 }, PointF32 { x: 11.0, y: 11.0 }],
                diameter: 1.5,
            },
            SelectionOperation::Add,
        )
        .unwrap();
        core.resize_selection(1).unwrap();
        core.resize_selection(-1).unwrap();
        let saved_bounds = core.selection_bounds().unwrap();
        let (_, selection_layer) = core.selection_to_layer("Saved Selection").unwrap();
        core.invert_selection().unwrap();
        core.selection_from_layer(selection_layer, SelectionLayerOperation::Replace)
            .unwrap();
        assert_eq!(core.selection_bounds().unwrap(), saved_bounds);
    }

    #[test]
    fn m3_acceptance_coordinate_preserving_typed_paste_and_floating_transform() {
        let mut source = Core::new();
        source
            .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        source.set_active_plane(ActivePlane::Color).unwrap();
        source
            .apply_stroke(&color_stroke(
                PaintTool::Pencil,
                1.0,
                StrokeSample {
                    x: 6.0,
                    y: 6.0,
                    pressure: 1.0,
                },
            ))
            .unwrap();
        source
            .apply_selection(
                &SelectionShape::Rectangle(RectI32 {
                    x: 6,
                    y: 6,
                    width: 1,
                    height: 1,
                }),
                SelectionOperation::New,
            )
            .unwrap();
        let payload = source.copy_selection().unwrap();
        assert_eq!(payload.bounds.x, 6);
        assert_eq!(payload.planes[0].pixels[0].x, 6);

        let mut destination = Core::new();
        destination
            .new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        destination.begin_paste(&payload).unwrap();
        assert!(matches!(
            destination.commit_floating(),
            Err(CoreError::InvalidState(_))
        ));
        destination
            .set_floating_transform(FloatingTransform {
                translate_x: -4.0,
                translate_y: -4.0,
                ..FloatingTransform::default()
            })
            .unwrap();
        destination.commit_floating().unwrap();
        assert_eq!(
            destination.plane_pixel(ActivePlane::Color, 2, 2).unwrap(),
            PixelValue::Rgba([12, 34, 56, 255])
        );
        destination.undo().unwrap();
        assert!(
            destination
                .plane_pixel(ActivePlane::Color, 2, 2)
                .unwrap()
                .is_zero()
        );
        let transform_payload = ClipboardPayload {
            source_document_uuid: 1,
            bounds: RectI32 {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            planes: vec![ClipboardPlane {
                kind: PlaneType::Color,
                pixel_format: PixelFormat::StraightRgba8,
                origin_x: 0,
                origin_y: 0,
                pixels: vec![
                    ClipboardPixel {
                        x: 0,
                        y: 0,
                        value: PixelValue::Rgba([255, 0, 0, 255]),
                    },
                    ClipboardPixel {
                        x: 1,
                        y: 0,
                        value: PixelValue::Rgba([0, 0, 255, 255]),
                    },
                ],
            }],
        };
        destination.begin_paste(&transform_payload).unwrap();
        destination
            .set_floating_transform(FloatingTransform {
                translate_x: 1.0,
                scale_x: 2.0,
                ..FloatingTransform::default()
            })
            .unwrap();
        destination.commit_floating().unwrap();
        assert_eq!(
            (0..4)
                .map(|x| destination.plane_pixel(ActivePlane::Color, x, 0).unwrap())
                .collect::<Vec<_>>(),
            vec![
                PixelValue::Rgba([255, 0, 0, 255]),
                PixelValue::Rgba([255, 0, 0, 255]),
                PixelValue::Rgba([0, 0, 255, 255]),
                PixelValue::Rgba([0, 0, 255, 255]),
            ]
        );
        destination.undo().unwrap();
        destination.begin_paste(&transform_payload).unwrap();
        destination
            .set_floating_transform(FloatingTransform {
                rotation_degrees: 180.0,
                ..FloatingTransform::default()
            })
            .unwrap();
        destination.commit_floating().unwrap();
        assert_eq!(
            destination.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
            PixelValue::Rgba([0, 0, 255, 255])
        );
        assert_eq!(
            destination.plane_pixel(ActivePlane::Color, 1, 0).unwrap(),
            PixelValue::Rgba([255, 0, 0, 255])
        );
        destination.undo().unwrap();
        let revision = destination.document_info().unwrap().document_revision;
        destination.begin_paste(&payload).unwrap();
        destination.cancel_floating();
        assert!(matches!(
            destination.commit_floating(),
            Err(CoreError::InvalidState(_))
        ));
        assert_eq!(
            destination.document_info().unwrap().document_revision,
            revision
        );
    }

    #[test]
    fn m3_acceptance_view_flip_and_destructive_mirror_have_separate_revisions() {
        let mut core = Core::new();
        core.new_cell(8, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_stroke(&line_stroke(vec![StrokeSample {
            x: 1.0,
            y: 1.0,
            pressure: 1.0,
        }]))
        .unwrap();
        let before = core.document_info().unwrap();
        let view = core
            .apply_view(ViewCommand::Flip {
                axis: MirrorAxis::Horizontal,
            })
            .unwrap();
        let after_view = core.document_info().unwrap();
        assert_eq!(after_view.document_revision, before.document_revision);
        assert!(after_view.view_revision > before.view_revision);
        assert!(view.flip_horizontal());
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 1, 1).unwrap(),
            PixelValue::Binary(255)
        );

        core.mirror_document(MirrorAxis::Horizontal).unwrap();
        let after_mirror = core.document_info().unwrap();
        assert!(after_mirror.document_revision > after_view.document_revision);
        assert_eq!(after_mirror.view_revision, after_view.view_revision);
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 6, 1).unwrap(),
            PixelValue::Binary(255)
        );
        core.undo().unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::MainLine, 1, 1).unwrap(),
            PixelValue::Binary(255)
        );
    }

    #[test]
    fn m3_acceptance_multi_view_locator_guides_grid_and_shortcuts() {
        let mut core = Core::new();
        core.new_cell(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let secondary = core.create_view().unwrap();
        core.apply_view_for(
            secondary,
            ViewCommand::BoxZoom {
                document_rect: RectI32 {
                    x: 4,
                    y: 4,
                    width: 8,
                    height: 8,
                },
                viewport_width: 160.0,
                viewport_height: 160.0,
            },
        )
        .unwrap();
        core.apply_stroke(&line_stroke(vec![StrokeSample {
            x: 3.0,
            y: 3.0,
            pressure: 1.0,
        }]))
        .unwrap();
        let primary = core.build_snapshot();
        let other = core.build_snapshot_for(secondary).unwrap();
        assert_eq!(primary.revision(), other.revision());
        assert_ne!(primary.view(), other.view());

        let (_, guide_id) = core.add_guide(GuideAxis::Vertical, 5).unwrap();
        core.set_grid(GridConfig {
            origin_x: 0,
            origin_y: 0,
            spacing_x: 8,
            spacing_y: 8,
            subdivisions: 2,
        })
        .unwrap();
        assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (5.2, 7.8));
        core.apply_view(ViewCommand::SetSnapEnabled(true)).unwrap();
        assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (5.0, 8.0));
        core.move_guide(guide_id, 6).unwrap();
        assert_eq!(core.guides().unwrap()[0].position, 6);
        let overlay_snapshot = core.build_snapshot();
        assert_eq!(overlay_snapshot.guides()[0].position, 6);
        assert_eq!(overlay_snapshot.grid().subdivisions, 2);

        let locator = core.locator_sample(None, 3.0, 3.0).unwrap();
        assert_eq!((locator.document_x, locator.document_y), (3, 3));
        core.rebind_shortcut(ShortcutBinding {
            command_id: 99,
            virtual_key: u32::from(b'Z'),
            modifiers: 1,
        })
        .unwrap();
        assert_eq!(
            core.resolve_shortcut(u32::from(b'Z'), SHORTCUT_MODIFIER_CONTROL)
                .unwrap(),
            Some(99)
        );
        assert!(
            !core
                .shortcut_bindings()
                .iter()
                .any(|binding| binding.command_id == 1)
        );
        assert!(
            core.shortcut_bindings()
                .iter()
                .any(|binding| binding.command_id == 99)
        );
        core.reset_shortcuts();
        assert_eq!(
            core.resolve_shortcut(u32::from(b'Z'), SHORTCUT_MODIFIER_CONTROL)
                .unwrap(),
            Some(1)
        );
        assert!(
            core.shortcut_bindings()
                .iter()
                .any(|binding| binding.command_id == 1)
        );
    }

    #[test]
    fn m3_tree_order_merge_names_and_active_ids_remain_consistent() {
        let mut core = Core::new();
        let created = core
            .new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.set_active_plane(ActivePlane::Color).unwrap();
        core.apply_stroke(&color_stroke(
            PaintTool::Pencil,
            1.0,
            StrokeSample {
                x: 0.0,
                y: 0.0,
                pressure: 1.0,
            },
        ))
        .unwrap();
        let (_, top) = core.duplicate_layer(created.layer_id).unwrap();
        core.reorder_layer(top, 0).unwrap();
        {
            let document = core.document.as_mut().unwrap();
            let top_color = document.layers[0]
                .planes
                .iter_mut()
                .find(|plane| plane.kind == PlaneType::Color)
                .unwrap();
            top_color
                .raster
                .set_pixel(0, 0, PixelValue::Rgba([0, 0, 255, 128]), 99)
                .unwrap();
        }
        core.render_cache.clear();
        assert_eq!(core.build_snapshot().tiles()[0].pixels(), [156, 17, 6, 255]);
        core.merge_layer_into_below(top).unwrap();
        assert_eq!(core.layers().unwrap().len(), 1);
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
            PixelValue::Rgba([6, 17, 156, 255])
        );
        assert_eq!(
            paste_value(
                PixelValue::Rgba16([u16::MAX, 0, 0, u16::MAX]),
                PixelValue::Rgba16([0, 0, u16::MAX, 32_768]),
                PlaneType::Raster,
            )
            .unwrap(),
            PixelValue::Rgba16([32_767, 0, 32_768, u16::MAX])
        );

        let (_, raster_layer) = core.create_layer(LayerKind::Raster, "Raster").unwrap();
        let raster_plane = core
            .layers()
            .unwrap()
            .iter()
            .find(|layer| layer.id == raster_layer)
            .unwrap()
            .planes[0]
            .id;
        core.duplicate_plane(raster_plane).unwrap();
        core.duplicate_plane(raster_plane).unwrap();
        let raster_names: BTreeSet<_> = core
            .layers()
            .unwrap()
            .iter()
            .find(|layer| layer.id == raster_layer)
            .unwrap()
            .planes
            .iter()
            .map(|plane| plane.name.clone())
            .collect();
        assert_eq!(raster_names.len(), 3);

        let (_, duplicate_coloring) = core.duplicate_layer(created.layer_id).unwrap();
        core.create_layer(LayerKind::Frame, "Frame").unwrap();
        core.delete_layer(duplicate_coloring).unwrap();
        assert!(core.document_info().is_ok());
        assert!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(core.document.as_ref().unwrap().active_plane_id)
                .is_some()
        );
    }

    #[test]
    fn m3_editable_layer_and_plane_flags_guard_pixel_commands() {
        let mut core = Core::new();
        let created = core
            .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.set_active_plane(ActivePlane::Color).unwrap();
        core.set_plane_properties(created.color_plane_id, true, false, 1_000, "Color")
            .unwrap();
        let locked_revision = core.document_info().unwrap().document_revision;
        assert!(matches!(
            core.apply_stroke(&color_stroke(
                PaintTool::Pencil,
                1.0,
                StrokeSample {
                    x: 1.0,
                    y: 1.0,
                    pressure: 1.0,
                }
            )),
            Err(CoreError::InvalidState(_))
        ));
        assert_eq!(
            core.document_info().unwrap().document_revision,
            locked_revision
        );

        core.set_plane_properties(created.color_plane_id, true, true, 1_000, "Color")
            .unwrap();
        core.set_layer_properties(created.layer_id, true, false, 1_000, "Coloring")
            .unwrap();
        let locked_revision = core.document_info().unwrap().document_revision;
        assert!(matches!(
            core.apply_fill(&fill_request(0, 0, [1, 2, 3, 255])),
            Err(CoreError::InvalidState(_))
        ));
        assert_eq!(
            core.document_info().unwrap().document_revision,
            locked_revision
        );
    }

    fn m4_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> CommonRaster {
        CommonRaster::new(
            width,
            height,
            PixelFormat::StraightRgba8,
            Some(DEFAULT_DPI_MILLI),
            Some(DEFAULT_DPI_MILLI),
            pixels,
        )
        .unwrap()
    }

    fn m4_rgba16(width: u32, height: u32, channels: Vec<u16>) -> CommonRaster {
        CommonRaster::new(
            width,
            height,
            PixelFormat::StraightRgba16,
            Some(DEFAULT_DPI_MILLI),
            Some(DEFAULT_DPI_MILLI),
            channels.into_iter().flat_map(u16::to_le_bytes).collect(),
        )
        .unwrap()
    }

    fn m4_source(
        name: &str,
        uuid: u128,
        width: u32,
        height: u32,
        pixel: [u8; 4],
    ) -> SequenceCellSource {
        let mut pixels = vec![0_u8; width as usize * height as usize * 4];
        pixels[..4].copy_from_slice(&pixel);
        SequenceCellSource::from_common_raster(name, uuid, &m4_rgba8(width, height, pixels))
            .unwrap()
    }

    #[test]
    fn m4_acceptance_reference_frame_aligns_different_cell_sizes_and_reopens() {
        let mut core = Core::new();
        core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let mut pixels = vec![0_u8; 4 * 4 * 4];
        pixels[..4].copy_from_slice(&[10, 20, 30, 255]);
        let source_offset = (2 * 4 + 2) * 4;
        pixels[source_offset..source_offset + 4].copy_from_slice(&[200, 40, 20, 255]);
        let source_corner_offset = (3 * 4 + 3) * 4;
        pixels[source_corner_offset..source_corner_offset + 4].copy_from_slice(&[50, 60, 70, 255]);
        let source = LightTableSource::from_common_raster(
            0x1111,
            7,
            RectI32 {
                x: 2,
                y: 2,
                width: 4,
                height: 4,
            },
            &m4_rgba8(4, 4, pixels),
        )
        .unwrap();
        core.light_table_add_item(LightTableItemInput::new("small reference", source))
            .unwrap();
        assert_eq!(
            core.light_table_sample(4, 4).unwrap(),
            PixelValue::Rgba([200, 40, 20, 255])
        );
        assert_eq!(
            core.light_table_sample(2, 2).unwrap(),
            PixelValue::Rgba([10, 20, 30, 255])
        );
        assert_eq!(
            core.light_table_sample(5, 5).unwrap(),
            PixelValue::Rgba([50, 60, 70, 255])
        );
        assert!(matches!(
            core.light_table_sample(0, 0),
            Err(CoreError::InvalidState(_))
        ));
        let snapshot = core.build_snapshot();
        let tile = &snapshot.tiles()[0];
        let mut golden = vec![0_u8; 8 * 8 * 4];
        golden[(2 * 8 + 2) * 4..(2 * 8 + 2) * 4 + 4].copy_from_slice(&[30, 20, 10, 255]);
        golden[(4 * 8 + 4) * 4..(4 * 8 + 4) * 4 + 4].copy_from_slice(&[20, 40, 200, 255]);
        golden[(5 * 8 + 5) * 4..(5 * 8 + 5) * 4 + 4].copy_from_slice(&[70, 60, 50, 255]);
        assert_eq!(tile.stride_bytes(), 8 * 4);
        assert_eq!(tile.pixels(), golden);

        let path = std::env::temp_dir().join(format!(
            "inkpod-m4-reference-{}-{}.inkpod",
            std::process::id(),
            core.document_info().unwrap().document_revision
        ));
        let _ = std::fs::remove_file(&path);
        core.save(&path).unwrap();
        let mut reopened = Core::new();
        reopened.open(&path).unwrap();
        assert_eq!(
            reopened.light_table_sample(4, 4).unwrap(),
            PixelValue::Rgba([200, 40, 20, 255])
        );
        assert_eq!(
            reopened.light_table_sample(2, 2).unwrap(),
            PixelValue::Rgba([10, 20, 30, 255])
        );
        assert_eq!(
            reopened.light_table_sample(5, 5).unwrap(),
            PixelValue::Rgba([50, 60, 70, 255])
        );
        let before_swap = reopened.light_table_items().unwrap();
        assert_eq!(before_swap.len(), 1);
        let old_uuid = reopened.document_info().unwrap().document_uuid;
        let swapped = reopened
            .light_table_swap_with_active(before_swap[0].id)
            .unwrap();
        assert_eq!(swapped.document_uuid, 0x1111);
        assert_eq!((swapped.width, swapped.height), (4, 4));
        let after_swap = reopened.light_table_items().unwrap();
        assert_eq!(after_swap[0].id, before_swap[0].id);
        assert_eq!(after_swap[0].opacity_milli, before_swap[0].opacity_milli);
        assert_eq!(after_swap[0].source_document_uuid, old_uuid);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn m4_acceptance_individual_and_global_opacity_multiply_to_twenty_five_percent() {
        let mut core = Core::new();
        core.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let source = LightTableSource::from_common_raster(
            0x2222,
            1,
            RectI32 {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
            &m4_rgba8(2, 2, [100, 120, 140, 255].repeat(4)),
        )
        .unwrap();
        let mut input = LightTableItemInput::new("half", source);
        input.opacity_milli = 500;
        core.light_table_add_item(input).unwrap();
        core.light_table_set_global_opacity(500).unwrap();
        let items = core.light_table_items().unwrap();
        assert_eq!(items[0].effective_opacity_milli, 250);
        assert_eq!(
            core.light_table_sample(1, 1).unwrap(),
            PixelValue::Rgba([100, 120, 140, 64])
        );
    }

    #[test]
    fn m4_light_table_color_sampling_preserves_exact_rgba16() {
        let mut core = Core::new();
        core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let source = LightTableSource::from_common_raster(
            0x2424,
            1,
            RectI32 {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &m4_rgba16(1, 1, vec![1, 257, 32_769, 65_535]),
        )
        .unwrap();
        core.light_table_add_item(LightTableItemInput::new("RGBA16", source))
            .unwrap();
        assert_eq!(
            core.eyedropper(EyedropperSource::LightTableTopmost, 0, 0)
                .unwrap(),
            PixelValue::Rgba16([1, 257, 32_769, 65_535])
        );
        core.light_table_set_global_opacity(500).unwrap();
        assert_eq!(
            core.light_table_sample(0, 0).unwrap(),
            PixelValue::Rgba16([1, 257, 32_769, 32_768])
        );
        let before_fill = core.document_info().unwrap();
        assert!(matches!(
            core.apply_fill_with_light_table(&fill_request(0, 0, [10, 20, 30, 255]), false, true,),
            Err(CoreError::InvalidState(_))
        ));
        assert_eq!(core.document_info().unwrap(), before_fill);
    }

    #[test]
    fn m4_light_table_set_item_management_is_transactional_and_stable_id_based() {
        let mut core = Core::new();
        core.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let default_set_id = core.light_table_sets().unwrap()[0].id;
        let (_, set_id) = core.light_table_create_set("References").unwrap();
        let path = std::env::temp_dir().join(format!(
            "inkpod-m4-active-set-{}-{}.inkpod",
            std::process::id(),
            core.document_info().unwrap().document_revision
        ));
        let _ = std::fs::remove_file(&path);
        core.save(&path).unwrap();
        let before_active_switch = core.document_info().unwrap();
        let active_switch = core.light_table_set_active(default_set_id).unwrap();
        assert_eq!(
            active_switch.revision(),
            before_active_switch.document_revision + 1
        );
        assert!(core.document_info().unwrap().dirty);
        assert!(
            core.light_table_sets()
                .unwrap()
                .iter()
                .any(|set| set.id == default_set_id && set.active)
        );
        core.undo().unwrap();
        assert!(!core.document_info().unwrap().dirty);
        assert!(
            core.light_table_sets()
                .unwrap()
                .iter()
                .any(|set| set.id == set_id && set.active)
        );
        let source = LightTableSource::from_common_raster(
            0x2525,
            1,
            RectI32 {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
            &m4_rgba8(2, 2, [20, 40, 60, 255].repeat(4)),
        )
        .unwrap();
        let mut invalid_source = source.clone();
        invalid_source.document_uuid = 0;
        let before_invalid_source = core.document_info().unwrap();
        assert!(matches!(
            core.light_table_add_item(LightTableItemInput::new("Invalid", invalid_source)),
            Err(CoreError::InvalidArgument(_))
        ));
        assert_eq!(core.document_info().unwrap(), before_invalid_source);
        let mut invalid_rotation = LightTableItemInput::new("Invalid", source.clone());
        invalid_rotation.rotation_milli_degrees = i32::MIN;
        let before_invalid = core.document_info().unwrap();
        assert!(matches!(
            core.light_table_add_item(invalid_rotation),
            Err(CoreError::InvalidArgument(_))
        ));
        assert_eq!(core.document_info().unwrap(), before_invalid);
        let (_, item_id) = core
            .light_table_add_item(LightTableItemInput::new("Item", source.clone()))
            .unwrap();
        let (_, duplicate_id) = core.light_table_duplicate_set(set_id).unwrap();
        assert_ne!(duplicate_id, set_id);
        let duplicate_item_id = core.light_table_items().unwrap()[0].id;
        assert_ne!(duplicate_item_id, item_id);
        core.light_table_rename_set(duplicate_id, "References")
            .unwrap();
        core.light_table_reorder_set(duplicate_id, 0).unwrap();
        core.light_table_set_active(set_id).unwrap();
        let mut update = LightTableItemInput::new("Moved", source);
        update.translate_x_milli = -1_000;
        core.light_table_update_item(item_id, update).unwrap();
        assert_eq!(
            core.light_table_sample(0, 1).unwrap(),
            PixelValue::Rgba([20, 40, 60, 255])
        );
        core.light_table_remove_item(item_id).unwrap();
        assert!(core.light_table_items().unwrap().is_empty());
        core.undo().unwrap();
        assert_eq!(core.light_table_items().unwrap()[0].id, item_id);
        core.redo().unwrap();
        assert!(core.light_table_items().unwrap().is_empty());
        core.light_table_delete_set(set_id).unwrap();
        core.light_table_delete_set(duplicate_id).unwrap();
        let sets = core.light_table_sets().unwrap();
        assert_eq!(sets.len(), 1);
        let final_set_id = sets[0].id;
        assert!(core.light_table_delete_set(final_set_id).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn m4_acceptance_light_table_fill_boundary_is_read_only() {
        let mut core = Core::new();
        core.new_cell(5, 5, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let mut pixels = vec![0_u8; 5 * 5 * 4];
        for y in 0..5 {
            let offset = (y * 5 + 2) * 4;
            pixels[offset..offset + 4].copy_from_slice(&[10, 20, 30, 255]);
        }
        let source = LightTableSource::from_common_raster(
            0x3333,
            9,
            RectI32 {
                x: 2,
                y: 2,
                width: 5,
                height: 5,
            },
            &m4_rgba8(5, 5, pixels),
        )
        .unwrap();
        core.light_table_add_item(LightTableItemInput::new("boundary", source))
            .unwrap();
        let before_item = core.light_table_items().unwrap()[0].clone();
        let before_sample = core.light_table_sample(2, 2).unwrap();
        let before_cancel = core.document_info().unwrap();
        let mut cancellation_polls = 0;
        assert_eq!(
            core.apply_fill_with_light_table_and_cancel(
                &fill_request(0, 2, [200, 0, 0, 255]),
                true,
                false,
                || {
                    cancellation_polls += 1;
                    cancellation_polls == 2
                },
            ),
            Err(CoreError::Cancelled)
        );
        assert_eq!(core.document_info().unwrap(), before_cancel);
        assert_eq!(core.light_table_items().unwrap()[0], before_item);
        assert_eq!(core.light_table_sample(2, 2).unwrap(), before_sample);
        let outcome = core
            .apply_fill_with_light_table(&fill_request(0, 2, [200, 0, 0, 255]), true, false)
            .unwrap();
        assert_eq!(outcome.changed_pixels, 10);
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 1, 2).unwrap(),
            PixelValue::Rgba([200, 0, 0, 255])
        );
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 3, 2).unwrap(),
            PixelValue::Rgba([0, 0, 0, 0])
        );
        assert_eq!(core.light_table_items().unwrap()[0], before_item);
        assert_eq!(core.light_table_sample(2, 2).unwrap(), before_sample);
        core.undo().unwrap();
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, 1, 2).unwrap(),
            PixelValue::Rgba([0, 0, 0, 0])
        );
        assert_eq!(core.light_table_sample(2, 2).unwrap(), before_sample);
    }

    #[test]
    fn m4_acceptance_sequence_switch_rejects_unsaved_document_without_discarding_it() {
        let mut core = Core::new();
        let current = core
            .new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.set_sequence(vec![
            m4_source("cell1.png", current.document_uuid, 2, 2, [1, 2, 3, 255]),
            m4_source("cell2.png", 0x4444, 3, 2, [4, 5, 6, 255]),
        ])
        .unwrap();
        let before = core.document_info().unwrap();
        assert_eq!(
            core.sequence_step(SequenceDirection::Next, false),
            Err(CoreError::UnsavedChanges)
        );
        let after_rejection = core.document_info().unwrap();
        assert_eq!(after_rejection.document_uuid, before.document_uuid);
        assert_eq!(after_rejection.document_revision, before.document_revision);
        assert!(after_rejection.dirty);

        let path = std::env::temp_dir().join(format!(
            "inkpod-m4-switch-{}-{}.inkpod",
            std::process::id(),
            before.document_revision
        ));
        let _ = std::fs::remove_file(&path);
        core.save(&path).unwrap();
        let switched = core.sequence_step(SequenceDirection::Next, false).unwrap();
        assert_eq!(switched.document_uuid, 0x4444);
        assert_eq!((switched.width, switched.height), (3, 2));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn m4_acceptance_sequence_gaps_natural_order_thumbnails_subpalette_and_motion() {
        let mut core = Core::new();
        core.set_sequence(vec![
            m4_source("cut10.png", 10, 2, 1, [10, 0, 0, 255]),
            m4_source("cut1.png", 1, 1, 1, [1, 0, 0, 255]),
            m4_source("cut3.png", 3, 3, 1, [3, 0, 0, 255]),
        ])
        .unwrap();
        let cells = core.sequence_cells().unwrap();
        assert_eq!(
            cells
                .iter()
                .map(|cell| cell.cell_number)
                .collect::<Vec<_>>(),
            vec![1, 3, 10]
        );
        assert!(cells.iter().all(|cell| cell.thumbnail.checksum != 0));
        core.set_subpalette_cell(1).unwrap();
        assert_eq!(
            core.subpalette_sample(0, 0).unwrap(),
            PixelValue::Rgba([3, 0, 0, 255])
        );
        let first = core
            .motion_check_start(MotionCheckConfig {
                fps: 24,
                loop_playback: true,
                include_selection: true,
                include_light_table: true,
            })
            .unwrap();
        assert_eq!(first.cell_number, 1);
        assert_eq!(first.fps, 24);
        assert!(first.include_selection && first.include_light_table);
        assert_eq!(
            core.motion_check_step(SequenceDirection::Next)
                .unwrap()
                .cell_number,
            3
        );
        assert_eq!(
            core.motion_check_step(SequenceDirection::Next)
                .unwrap()
                .cell_number,
            10
        );
        assert_eq!(
            core.motion_check_step(SequenceDirection::Next)
                .unwrap()
                .cell_number,
            1
        );
        assert!(core.motion_check_toggle_pause().unwrap().paused);

        let exported = core
            .export_sequence(CommonRasterFormat::Png, false)
            .unwrap();
        assert_eq!(exported.len(), 3);
        let mut imported = Core::new();
        imported
            .import_sequence(CommonRasterFormat::Png, exported)
            .unwrap();
        assert_eq!(
            imported
                .sequence_cells()
                .unwrap()
                .iter()
                .map(|cell| cell.cell_number)
                .collect::<Vec<_>>(),
            vec![1, 3, 10]
        );
    }

    #[test]
    fn m4_rejects_a_mutated_common_raster_before_indexing_its_pixels() {
        let mut malformed = m4_rgba8(1, 1, vec![1, 2, 3, 4]);
        malformed.pixels.clear();
        assert!(matches!(
            LightTableSource::from_common_raster(
                0x5151,
                1,
                RectI32 {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                &malformed,
            ),
            Err(CoreError::Format(_))
        ));

        let mut invalid_cell = m4_source("cell1.png", 0x6161, 1, 1, [1, 2, 3, 255]);
        invalid_cell.frames.reference_frame.width = 0;
        let mut core = Core::new();
        assert!(matches!(
            core.set_sequence(vec![invalid_cell]),
            Err(CoreError::InvalidArgument(_))
        ));
        assert!(matches!(
            core.sequence_cells(),
            Err(CoreError::InvalidState(_))
        ));
    }

    fn vector_line(
        start: (f32, f32),
        end: (f32, f32),
        width_start: f32,
        width_end: f32,
        color: [u8; 4],
    ) -> VectorPathInput {
        let third_x = (end.0 - start.0) / 3.0;
        let third_y = (end.1 - start.1) / 3.0;
        VectorPathInput {
            segments: vec![VectorCubicSegment {
                p0: PointF32 {
                    x: start.0,
                    y: start.1,
                },
                p1: PointF32 {
                    x: start.0 + third_x,
                    y: start.1 + third_y,
                },
                p2: PointF32 {
                    x: start.0 + third_x * 2.0,
                    y: start.1 + third_y * 2.0,
                },
                p3: PointF32 { x: end.0, y: end.1 },
                width_start,
                width_end,
            }],
            color: PixelValue::Rgba(color),
            closed: false,
        }
    }

    fn vector_rectangle(x0: f32, y0: f32, x1: f32, y1: f32) -> VectorPathInput {
        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
        let mut segments = Vec::new();
        for pair in corners.windows(2) {
            segments.extend(vector_line(pair[0], pair[1], 1.0, 1.0, [0, 0, 0, 255]).segments);
        }
        VectorPathInput {
            segments,
            color: PixelValue::Rgba([0, 0, 0, 255]),
            closed: true,
        }
    }

    fn vector_core(width: u32, height: u32) -> (Core, u64, u64, u64, u64) {
        let mut core = Core::new();
        core.new_cell(width, height, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let (_, layer_id) = core
            .create_layer(LayerKind::VectorColoring, "Vector")
            .unwrap();
        let (main_id, trace_id, fill_id) = core.vector_layer_planes(layer_id).unwrap();
        (core, layer_id, main_id, trace_id, fill_id)
    }

    #[test]
    fn m5_acceptance_zoom_never_changes_core_vector_geometry() {
        let (mut core, _, main_id, _, _) = vector_core(32, 32);
        core.vector_add_path(
            main_id,
            VectorPathInput {
                segments: vec![VectorCubicSegment {
                    p0: PointF32 { x: 2.0, y: 3.0 },
                    p1: PointF32 { x: 8.0, y: 1.0 },
                    p2: PointF32 { x: 16.0, y: 20.0 },
                    p3: PointF32 { x: 28.0, y: 24.0 },
                    width_start: 1.25,
                    width_end: 4.75,
                }],
                color: PixelValue::Rgba16([257, 2_000, 40_000, 65_535]),
                closed: false,
            },
        )
        .unwrap();
        let revision = core.document_info().unwrap().document_revision;
        let paths_before = core.vector_paths().unwrap();
        let snapshot_before = core.build_snapshot();
        core.apply_view(ViewCommand::ZoomAt {
            factor: 8.0,
            device_x: 11.0,
            device_y: 13.0,
        })
        .unwrap();
        let snapshot_after = core.build_snapshot();
        assert_eq!(core.document_info().unwrap().document_revision, revision);
        assert_eq!(core.vector_paths().unwrap(), paths_before);
        assert_eq!(
            snapshot_before.vector_segments(),
            snapshot_after.vector_segments()
        );
        assert_eq!(snapshot_before.vector_segments().len(), 1);
        assert_ne!(snapshot_before.view().zoom, snapshot_after.view().zoom);
    }

    #[test]
    fn m5_acceptance_partial_erase_changes_only_the_touched_stroke() {
        let (mut core, _, main_id, _, _) = vector_core(12, 10);
        let (_, touched_id) = core
            .vector_add_path(
                main_id,
                vector_line((1.0, 2.0), (11.0, 2.0), 1.0, 3.0, [10, 20, 30, 255]),
            )
            .unwrap();
        let (_, protected_id) = core
            .vector_add_path(
                main_id,
                vector_line((1.0, 7.0), (11.0, 7.0), 2.0, 2.0, [90, 80, 70, 255]),
            )
            .unwrap();
        let protected_before = core
            .vector_paths()
            .unwrap()
            .into_iter()
            .find(|path| path.id == protected_id)
            .unwrap();
        core.vector_erase(
            main_id,
            PointF32 { x: 6.0, y: 2.0 },
            1.0,
            VectorEraseMode::Partial,
        )
        .unwrap();
        let paths = core.vector_paths().unwrap();
        assert_eq!(
            paths.iter().find(|path| path.id == protected_id),
            Some(&protected_before)
        );
        assert_eq!(
            paths.iter().filter(|path| path.plane_id == main_id).count(),
            3
        );
        assert!(paths.iter().any(|path| path.id == touched_id));
        core.undo().unwrap();
        assert_eq!(core.vector_paths().unwrap().len(), 2);
        core.redo().unwrap();
        assert_eq!(core.vector_paths().unwrap().len(), 3);
        core.vector_erase(
            main_id,
            PointF32 { x: 6.0, y: 7.0 },
            1.0,
            VectorEraseMode::WholePath,
        )
        .unwrap();
        assert!(
            !core
                .vector_paths()
                .unwrap()
                .iter()
                .any(|path| path.id == protected_id)
        );
        core.undo().unwrap();
        assert!(
            core.vector_paths()
                .unwrap()
                .iter()
                .any(|path| path.id == protected_id)
        );
    }

    #[test]
    fn m5_acceptance_intersection_erase_cut_points_are_deterministic() {
        fn erased() -> Vec<VectorPathInfo> {
            let (mut core, _, main_id, _, _) = vector_core(10, 10);
            core.vector_add_path(
                main_id,
                vector_line((1.0, 5.0), (9.0, 5.0), 1.0, 1.0, [0, 0, 0, 255]),
            )
            .unwrap();
            core.vector_add_path(
                main_id,
                vector_line((3.0, 1.0), (3.0, 9.0), 1.0, 1.0, [1, 2, 3, 255]),
            )
            .unwrap();
            core.vector_add_path(
                main_id,
                vector_line((7.0, 1.0), (7.0, 9.0), 1.0, 1.0, [4, 5, 6, 255]),
            )
            .unwrap();
            core.vector_erase(
                main_id,
                PointF32 { x: 5.0, y: 5.0 },
                0.25,
                VectorEraseMode::ToIntersection,
            )
            .unwrap();
            core.vector_paths().unwrap()
        }
        let first = erased();
        let second = erased();
        assert_eq!(first, second);
        let horizontal: Vec<_> = first
            .iter()
            .filter(|path| path.color == PixelValue::Rgba([0, 0, 0, 255]))
            .collect();
        assert_eq!(horizontal.len(), 2);
        assert_eq!(horizontal[0].segments[0].p3, PointF32 { x: 3.0, y: 5.0 });
        assert_eq!(horizontal[1].segments[0].p0, PointF32 { x: 7.0, y: 5.0 });
        assert_eq!(
            first
                .iter()
                .filter(|path| path.color != PixelValue::Rgba([0, 0, 0, 255]))
                .count(),
            2
        );
    }

    #[test]
    fn m5_acceptance_fill_topology_survives_save_and_reopen() {
        let (mut core, layer_id, _, trace_id, fill_plane_id) = vector_core(8, 8);
        let (_, boundary_id) = core
            .vector_add_path(trace_id, vector_rectangle(1.0, 1.0, 7.0, 7.0))
            .unwrap();
        let (_, fill_id) = core
            .vector_add_fill(
                fill_plane_id,
                &[boundary_id],
                PixelValue::Rgba16([60_000, 1_000, 2_000, 50_000]),
            )
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "inkpod-core-m5-topology-{}-{}.inkpod",
            std::process::id(),
            TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let before_paths = core.vector_paths().unwrap();
        let before_fills = core.vector_fills().unwrap();
        core.save(&path).unwrap();
        let mut reopened = Core::new();
        reopened.open(&path).unwrap();
        assert_eq!(reopened.vector_paths().unwrap(), before_paths);
        assert_eq!(reopened.vector_fills().unwrap(), before_fills);
        assert_eq!(reopened.vector_fills().unwrap()[0].id, fill_id);
        let reopened_layer = reopened
            .layers()
            .unwrap()
            .into_iter()
            .find(|layer| layer.id == layer_id)
            .unwrap();
        assert_eq!(reopened_layer.kind, LayerKind::VectorColoring);
        let snapshot = reopened.build_snapshot();
        assert_eq!(snapshot.vector_fills().len(), 1);
        assert_eq!(snapshot.vector_segments().len(), 4);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn m5_acceptance_rasterize_antialias_pixel_center_and_scale_golden() {
        let (mut core, layer_id, main_id, _, _) = vector_core(4, 4);
        core.vector_add_path(
            main_id,
            vector_line((0.0, 1.0), (4.0, 1.0), 1.0, 1.0, [255, 0, 0, 255]),
        )
        .unwrap();
        let no_aa = core.rasterize_vector_layer(layer_id, 1, false).unwrap();
        let red = [255_u8, 0, 0, 255];
        let clear = [0_u8; 4];
        assert_eq!(
            no_aa.pixels,
            [
                red, red, red, red, red, red, red, red, clear, clear, clear, clear, clear, clear,
                clear, clear
            ]
            .concat()
        );
        let aa = core.rasterize_vector_layer(layer_id, 1, true).unwrap();
        let half_red = [255_u8, 0, 0, 128];
        assert_eq!(
            aa.pixels,
            [
                half_red, half_red, half_red, half_red, half_red, half_red, half_red, half_red,
                clear, clear, clear, clear, clear, clear, clear, clear
            ]
            .concat()
        );
        let scaled = core.rasterize_vector_layer(layer_id, 2, false).unwrap();
        assert_eq!(
            (scaled.width, scaled.height, scaled.stride_bytes),
            (8, 8, 32)
        );
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 32 + x * 4;
                let expected = if y == 1 || y == 2 { red } else { clear };
                assert_eq!(&scaled.pixels[offset..offset + 4], &expected);
            }
        }
    }

    #[test]
    fn vector_002_connect_width_select_and_raster_vector_conversion_are_transactional() {
        let (mut core, layer_id, main_id, trace_id, _) = vector_core(4, 4);
        let revision = core.document_info().unwrap().document_revision;
        let mut too_thin = vector_line((0.0, 0.0), (1.0, 0.0), 0.0001, 1.0, [0, 0, 0, 255]);
        too_thin.segments[0].width_end = 0.0001;
        assert!(matches!(
            core.vector_add_path(main_id, too_thin),
            Err(CoreError::InvalidArgument(_))
        ));
        assert_eq!(core.document_info().unwrap().document_revision, revision);

        let (_, left_id) = core
            .vector_add_path(
                main_id,
                vector_line((0.0, 1.0), (1.0, 1.0), 1.0, 1.0, [0, 0, 0, 255]),
            )
            .unwrap();
        let (_, right_id) = core
            .vector_add_path(
                main_id,
                vector_line((2.0, 1.0), (3.0, 1.0), 1.0, 1.0, [0, 0, 0, 255]),
            )
            .unwrap();
        let (_, connector_id) = core.vector_connect(main_id, 1.5).unwrap();
        let connector_id = connector_id.unwrap();
        let revision_after_connect = core.document_info().unwrap().document_revision;
        let (outcome, repeated_connector) = core.vector_connect(main_id, 1.5).unwrap();
        assert!(repeated_connector.is_none());
        assert_eq!(outcome.revision(), revision_after_connect);
        core.vector_correct_width(
            &[left_id, right_id, connector_id],
            VectorWidthMode::Add(1.0),
        )
        .unwrap();
        core.vector_correct_width(
            &[left_id, right_id, connector_id],
            VectorWidthMode::Subtract(0.5),
        )
        .unwrap();
        core.vector_correct_width(
            &[left_id, right_id, connector_id],
            VectorWidthMode::Scale(2.0),
        )
        .unwrap();
        core.vector_correct_width(
            &[left_id, right_id, connector_id],
            VectorWidthMode::Constant(2.0),
        )
        .unwrap();
        assert!(core.vector_paths().unwrap().iter().all(|path| {
            path.segments
                .iter()
                .all(|segment| segment.width_start == 2.0)
        }));
        let revision_after_width = core.document_info().unwrap().document_revision;
        let outcome = core
            .vector_correct_width(
                &[left_id, right_id, connector_id],
                VectorWidthMode::Constant(2.0),
            )
            .unwrap();
        assert_eq!(outcome.revision(), revision_after_width);
        let selected = core
            .vector_select(
                RectI32 {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 3,
                },
                VectorSelectionMode::Touching,
            )
            .unwrap();
        assert_eq!(selected.path_ranges.len(), 3);

        // Path creation order must not put a later color trace over the
        // protected main-line plane.
        core.vector_add_path(
            trace_id,
            vector_line((0.0, 1.0), (3.0, 1.0), 1.0, 1.0, [255, 0, 0, 255]),
        )
        .unwrap();
        let snapshot = core.build_snapshot();
        assert_eq!(snapshot.vector_segments()[0].plane_id, trace_id);
        assert!(
            snapshot.vector_segments()[1..]
                .iter()
                .all(|segment| segment.plane_id == main_id)
        );
        let rasterized = core.rasterize_vector_layer(layer_id, 1, false).unwrap();
        assert_eq!(&rasterized.pixels[0..4], &[0, 0, 0, 255]);

        let (_, raster_layer_id) = core.create_layer(LayerKind::Raster, "Source").unwrap();
        let raster_plane_id = core
            .layers()
            .unwrap()
            .into_iter()
            .find(|layer| layer.id == raster_layer_id)
            .unwrap()
            .planes[0]
            .id;
        let before_empty_conversion = core.document_info().unwrap().document_revision;
        let (outcome, fill_ids) = core
            .vectorize_raster_plane(raster_plane_id, layer_id, 0)
            .unwrap();
        assert!(fill_ids.is_empty());
        assert_eq!(outcome.revision(), before_empty_conversion);
        assert_eq!(
            core.document_info().unwrap().document_revision,
            before_empty_conversion
        );
        core.document
            .as_mut()
            .unwrap()
            .layers
            .iter_mut()
            .find(|layer| layer.id == layer_id)
            .unwrap()
            .editable = false;
        assert!(matches!(
            core.vectorize_raster_plane(raster_plane_id, layer_id, 1),
            Err(CoreError::InvalidState(_))
        ));
        core.document
            .as_mut()
            .unwrap()
            .layers
            .iter_mut()
            .find(|layer| layer.id == layer_id)
            .unwrap()
            .editable = true;
        core.document
            .as_mut()
            .unwrap()
            .plane_by_id_mut(raster_plane_id)
            .unwrap()
            .raster
            .set_pixel(0, 0, PixelValue::Rgba([7, 8, 9, 255]), 99)
            .unwrap();
        let before_revision = core.document_info().unwrap().document_revision;
        let (outcome, fill_ids) = core
            .vectorize_raster_plane(raster_plane_id, layer_id, 1)
            .unwrap();
        assert_eq!(outcome.revision(), before_revision + 1);
        assert_eq!(fill_ids.len(), 1);
        core.undo().unwrap();
        assert_eq!(core.vector_fills().unwrap().len(), 0);
        core.redo().unwrap();
        assert_eq!(core.vector_fills().unwrap().len(), 1);

        let next_id = core.next_id;
        core.document_revision = u64::MAX;
        assert!(matches!(
            core.vector_add_path(
                main_id,
                vector_line((0.0, 3.0), (3.0, 3.0), 1.0, 1.0, [0, 0, 0, 255]),
            ),
            Err(CoreError::InvalidState("document revision overflow"))
        ));
        assert_eq!(core.next_id, next_id);
    }

    #[test]
    fn vector_002_all_selection_modes_have_deterministic_ranges_and_ids() {
        let (mut core, _, main_id, trace_id, fill_plane_id) = vector_core(8, 8);
        let (_, horizontal_id) = core
            .vector_add_path(
                main_id,
                vector_line((0.0, 4.0), (6.0, 4.0), 1.0, 1.0, [0, 0, 0, 255]),
            )
            .unwrap();
        for x in [1.0, 5.0] {
            core.vector_add_path(
                main_id,
                vector_line((x, 0.0), (x, 8.0), 1.0, 1.0, [0, 0, 0, 255]),
            )
            .unwrap();
        }
        let (_, boundary_id) = core
            .vector_add_path(trace_id, vector_rectangle(1.0, 1.0, 7.0, 7.0))
            .unwrap();
        let (_, fill_id) = core
            .vector_add_fill(
                fill_plane_id,
                &[boundary_id],
                PixelValue::Rgba([20, 40, 60, 255]),
            )
            .unwrap();
        let center = RectI32 {
            x: 2,
            y: 3,
            width: 2,
            height: 2,
        };

        let cut = core
            .vector_select(center, VectorSelectionMode::CutBySelection)
            .unwrap();
        assert_eq!(
            cut.path_ranges,
            vec![VectorSelectionRange {
                path_id: horizontal_id,
                start_million: 333_333,
                end_million: 666_667,
            }]
        );
        for mode in [
            VectorSelectionMode::Touching,
            VectorSelectionMode::Line,
            VectorSelectionMode::WholeLine,
        ] {
            assert_eq!(
                core.vector_select(center, mode).unwrap().path_ranges,
                vec![VectorSelectionRange {
                    path_id: horizontal_id,
                    start_million: 0,
                    end_million: 1_000_000,
                }]
            );
        }
        assert_eq!(
            core.vector_select(center, VectorSelectionMode::ToIntersection)
                .unwrap()
                .path_ranges,
            vec![VectorSelectionRange {
                path_id: horizontal_id,
                start_million: 166_667,
                end_million: 833_333,
            }]
        );
        assert_eq!(
            core.vector_select(center, VectorSelectionMode::FillBoundary)
                .unwrap()
                .path_ranges,
            vec![VectorSelectionRange {
                path_id: boundary_id,
                start_million: 0,
                end_million: 1_000_000,
            }]
        );
        assert_eq!(
            core.vector_select(center, VectorSelectionMode::Fill)
                .unwrap()
                .fill_ids,
            vec![fill_id]
        );
        let contained = core
            .vector_select(
                RectI32 {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                VectorSelectionMode::FullyContained,
            )
            .unwrap();
        assert_eq!(contained.path_ranges.len(), 4);
        assert!(
            contained
                .path_ranges
                .iter()
                .all(|range| range.start_million == 0 && range.end_million == 1_000_000)
        );
    }
}
