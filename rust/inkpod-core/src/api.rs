//! Stable public command, input, result, and view data types.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    NoOp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchOutcome {
    pub(super) revision: u64,
    pub(super) accepted_commands: u64,
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
    pub(super) const fn file_kind(self) -> FilePlaneKind {
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

    pub(super) const fn from_file(kind: FilePlaneKind) -> Self {
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
pub enum RotateDirection {
    Left90,
    Right90,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeAnchor {
    TopLeft,
    TopRight,
    Center,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentResize {
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub resample: bool,
    pub anchor: ResizeAnchor,
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
    pub use_document_selection: bool,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectRegionKind {
    Trace,
    Rectangle,
    Polyline,
    Lasso,
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
    SetGuideSnapEnabled(bool),
    SetGridSnapEnabled(bool),
    SetTransparentView(bool),
    SetAlphaView(bool),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewMode {
    Manual,
    Fit,
    OneToOne,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    pub(super) zoom: f64,
    pub(super) pan_x: f64,
    pub(super) pan_y: f64,
    pub(super) revision: u64,
    pub(super) mode: ViewMode,
    pub(super) flip_horizontal: bool,
    pub(super) flip_vertical: bool,
    pub(super) ruler_visible: bool,
    pub(super) guides_visible: bool,
    pub(super) grid_visible: bool,
    pub(super) snap_enabled: bool,
    pub(super) guide_snap_enabled: bool,
    pub(super) grid_snap_enabled: bool,
    pub(super) transparent_view: bool,
    pub(super) alpha_view: bool,
    pub(super) viewport_width: f64,
    pub(super) viewport_height: f64,
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
            guide_snap_enabled: false,
            grid_snap_enabled: false,
            transparent_view: true,
            alpha_view: false,
            viewport_width: 1.0,
            viewport_height: 1.0,
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
    pub const fn guide_snap_enabled(self) -> bool {
        self.guide_snap_enabled
    }

    #[must_use]
    pub const fn grid_snap_enabled(self) -> bool {
        self.grid_snap_enabled
    }

    #[must_use]
    pub const fn transparent_view(self) -> bool {
        self.transparent_view
    }

    #[must_use]
    pub const fn alpha_view(self) -> bool {
        self.alpha_view
    }

    #[must_use]
    pub const fn viewport_width(self) -> f64 {
        self.viewport_width
    }

    #[must_use]
    pub const fn viewport_height(self) -> f64 {
        self.viewport_height
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
