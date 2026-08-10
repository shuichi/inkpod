//! Typed document-session editor state and immutable built-in defaults.

use crate::{
    BrushShape, CoordinateSpace, DEFAULT_DPI_MILLI, FillOperation, InclusionMode, PixelValue,
    RangeInterpretation, SelectionOperation, StartColorPredicate, StrokeSample, TraceBrushShape,
    VectorEraseMode, VectorSelectionMode,
};
use std::collections::BTreeMap;

/// Immutable built-in values used before a document exists and copied at document creation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorDefaults {
    /// Built-in dimensions and resolution presented by the new-document dialog.
    pub initial_document: InitialDocumentSpec,
    /// Built-in editor state copied into each newly opened document session.
    pub state: EditorState,
}

impl EditorDefaults {
    pub(crate) fn built_in() -> Self {
        let mut tool_styles = BTreeMap::new();
        for tool in EditorTool::ALL {
            let color = if tool == EditorTool::Pencil {
                Some(PixelValue::Rgba([0, 0, 0, 255]))
            } else if tool.consumes_color() {
                Some(PixelValue::Rgba([220, 40, 30, 255]))
            } else {
                None
            };
            let diameter_q16 = if tool == EditorTool::Pencil {
                1_i64 << 16
            } else {
                8_i64 << 16
            };
            tool_styles.insert(
                tool,
                EditorToolStyle {
                    color,
                    diameter_q16,
                },
            );
        }
        Self {
            initial_document: InitialDocumentSpec {
                width: 1_920,
                height: 1_080,
                dpi_x_milli: DEFAULT_DPI_MILLI,
                dpi_y_milli: DEFAULT_DPI_MILLI,
            },
            state: EditorState {
                active_tool: EditorTool::Pencil,
                last_color_consuming_tool: Some(EditorTool::Pencil),
                tool_styles,
                brush: EditorBrushOptions::default(),
                fill: EditorFillOptions::default(),
                selection: EditorSelectionOptions::default(),
                vector: EditorVectorOptions::default(),
                target: None,
                edit_targets: Vec::new(),
                palette_cursor: None,
            },
        }
    }
}

/// Built-in new-document values owned by Rust Core rather than application preferences.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitialDocumentSpec {
    /// Width in document pixels.
    pub width: u32,
    /// Height in document pixels.
    pub height: u32,
    /// Horizontal resolution in thousandths of a DPI.
    pub dpi_x_milli: u32,
    /// Vertical resolution in thousandths of a DPI.
    pub dpi_y_milli: u32,
}

/// Stable built-in editor tool catalog used by editor-state frames and frontends.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u32)]
pub enum EditorTool {
    /// One-pixel-oriented raster pencil.
    Pencil = 1,
    /// Pressure-capable raster brush.
    Brush = 2,
    /// Raster eraser.
    Eraser = 3,
    /// Bounded raster fill command.
    Fill = 1_001,
    /// Color sampler.
    Eyedropper = 1_002,
    /// Box zoom command.
    BoxZoom = 1_003,
    /// Guide mover.
    GuideMove = 1_004,
    /// Selection-mask command.
    Selection = 1_005,
    /// Floating-selection transformer.
    FloatingTransform = 1_006,
    /// Light Table mover.
    LightTableMove = 1_007,
    /// Gradient effect command.
    EffectGradient = 1_101,
    /// Airbrush effect command.
    EffectAirbrush = 1_102,
    /// Blur effect command.
    EffectBlur = 1_103,
    /// Stamp effect command.
    EffectStamp = 1_104,
    /// Dust-removal effect command.
    EffectDust = 1_105,
    /// Alpha-gradient effect command.
    EffectAlphaGradient = 1_106,
    /// Vector line command.
    VectorLine = 1_201,
    /// Vector curve command.
    VectorCurve = 1_202,
    /// Vector rectangle command.
    VectorRectangle = 1_203,
    /// Vector ellipse command.
    VectorEllipse = 1_204,
    /// Vector polyline command.
    VectorPolyline = 1_205,
    /// Vector eraser command.
    VectorEraser = 1_206,
}

impl EditorTool {
    pub(crate) const ALL: [Self; 22] = [
        Self::Pencil,
        Self::Brush,
        Self::Eraser,
        Self::Fill,
        Self::Eyedropper,
        Self::BoxZoom,
        Self::GuideMove,
        Self::Selection,
        Self::FloatingTransform,
        Self::LightTableMove,
        Self::EffectGradient,
        Self::EffectAirbrush,
        Self::EffectBlur,
        Self::EffectStamp,
        Self::EffectDust,
        Self::EffectAlphaGradient,
        Self::VectorLine,
        Self::VectorCurve,
        Self::VectorRectangle,
        Self::VectorEllipse,
        Self::VectorPolyline,
        Self::VectorEraser,
    ];

    pub(crate) const fn consumes_color(self) -> bool {
        matches!(
            self,
            Self::Pencil
                | Self::Brush
                | Self::Fill
                | Self::Selection
                | Self::EffectAirbrush
                | Self::VectorLine
                | Self::VectorCurve
                | Self::VectorRectangle
                | Self::VectorEllipse
                | Self::VectorPolyline
        )
    }

    pub(crate) fn from_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| *tool as u32 == code)
    }
}

/// Exact-depth current color and Q16.16 diameter retained for one tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorToolStyle {
    /// Straight-alpha RGBA8/RGBA16 color, or `None` for a colorless tool.
    pub color: Option<PixelValue>,
    /// Diameter in signed Q16.16 document pixels.
    pub diameter_q16: i64,
}

/// Core-owned options captured when a raster brush stroke begins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorBrushOptions {
    /// Canonical dab footprint.
    pub shape: BrushShape,
    /// Causal smoothing strength in the inclusive range `0..=1000`.
    pub smoothing: u16,
    /// Immutable native-depth start-value predicate.
    pub start_color: StartColorPredicate,
}

impl Default for EditorBrushOptions {
    fn default() -> Self {
        Self {
            shape: BrushShape::Round,
            smoothing: 0,
            start_color: StartColorPredicate::Any,
        }
    }
}

/// Core-owned fill command options copied into a procedure when that command starts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorFillOptions {
    /// Fill algorithm.
    pub operation: FillOperation,
    /// Inclusive normalized 16-bit channel tolerance.
    pub tolerance: u16,
    /// Maximum virtual gap width in pixels.
    pub gap_close: u8,
    /// Extension distance in pixels.
    pub extension_distance: u32,
    /// Inclusion-color rule.
    pub inclusion_mode: InclusionMode,
    /// Exact-depth colors used by the inclusion rule.
    pub inclusion_colors: Vec<PixelValue>,
    /// Whether reaching an image edge aborts the whole fill.
    pub overflow_abort: bool,
    /// Whether disconnected regions are processed.
    pub detached_regions: bool,
    /// Whether only transparent pixels are eligible.
    pub transparent_only: bool,
    /// Whether the document selection bounds the fill.
    pub use_document_selection: bool,
    /// Whether visible Light Table geometry contributes boundaries.
    pub light_table_boundary: bool,
    /// Whether visible Light Table color contributes sampling.
    pub light_table_color: bool,
}

impl Default for EditorFillOptions {
    fn default() -> Self {
        Self {
            operation: FillOperation::Seed,
            tolerance: 0,
            gap_close: 0,
            extension_distance: 1,
            inclusion_mode: InclusionMode::None,
            inclusion_colors: Vec::new(),
            overflow_abort: true,
            detached_regions: false,
            transparent_only: false,
            use_document_selection: false,
            light_table_boundary: false,
            light_table_color: false,
        }
    }
}

/// Geometry selected by the Core-owned selection tool options.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EditorSelectionShape {
    /// Axis-aligned rectangle.
    Rectangle = 1,
    /// Axis-aligned ellipse.
    Ellipse = 2,
    /// Freehand lasso.
    Lasso = 3,
    /// Ordered polygonal path.
    Polyline = 4,
    /// Stroked trace selection.
    Trace = 5,
    /// Tolerance-based contiguous region.
    Wand = 6,
}

impl EditorSelectionShape {
    pub(crate) const fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::Rectangle),
            2 => Some(Self::Ellipse),
            3 => Some(Self::Lasso),
            4 => Some(Self::Polyline),
            5 => Some(Self::Trace),
            6 => Some(Self::Wand),
            _ => None,
        }
    }
}

/// Core-owned options for constructing a document selection mask.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSelectionOptions {
    /// Selection geometry.
    pub shape: EditorSelectionShape,
    /// Selection-mask algebra operation.
    pub operation: SelectionOperation,
    /// Inclusive normalized 16-bit channel tolerance.
    pub tolerance: u16,
    /// Maximum virtual gap width in pixels.
    pub gap_close: u8,
    /// Trace diameter in signed Q16.16 document pixels.
    pub diameter_q16: i64,
    /// Raster-content meaning applied to the geometric candidate.
    pub interpretation: RangeInterpretation,
    /// Width/height ratio in unsigned Q16.16, or zero for a free ratio.
    pub aspect_ratio_q16: u32,
    /// Whether rectangle/ellipse construction grows around the gesture anchor.
    pub from_center: bool,
    /// Whether rotation is rounded to the nearest 45 degrees.
    pub constrain_rotation_45: bool,
    /// Clockwise rotation in unsigned binary turns.
    pub rotation_turns: u32,
    /// Trace brush stamp shape.
    pub trace_shape: TraceBrushShape,
    /// Whether trace pressure scales its diameter.
    pub trace_pressure_size: bool,
    /// Whether the trace diameter is held constant in screen pixels.
    pub trace_screen_size: bool,
}

impl Default for EditorSelectionOptions {
    fn default() -> Self {
        Self {
            shape: EditorSelectionShape::Rectangle,
            operation: SelectionOperation::New,
            tolerance: 0,
            gap_close: 0,
            diameter_q16: 8_i64 << 16,
            interpretation: RangeInterpretation::Normal,
            aspect_ratio_q16: 0,
            from_center: false,
            constrain_rotation_45: false,
            rotation_turns: 0,
            trace_shape: TraceBrushShape::Round,
            trace_pressure_size: false,
            trace_screen_size: false,
        }
    }
}

/// Core-owned vector erase, selection, and tool options.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorVectorOptions {
    /// Vector eraser hit behavior.
    pub erase_mode: VectorEraseMode,
    /// Vector selection hit behavior.
    pub selection_mode: VectorSelectionMode,
}

impl Default for EditorVectorOptions {
    fn default() -> Self {
        Self {
            erase_mode: VectorEraseMode::Partial,
            selection_mode: VectorSelectionMode::Touching,
        }
    }
}

/// Stable active edit target in the current document namespace.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EditorTarget {
    /// Stable layer ID.
    pub layer_id: u64,
    /// Stable plane ID belonging to `layer_id`.
    pub plane_id: u64,
}

/// Stable layer or plane selected for grouped edit commands.
///
/// The set is stored in document tree order. A selected layer represents the
/// whole layer and therefore suppresses redundant child-plane targets.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EditTarget {
    /// One complete layer and all of its owned plane/vector content.
    Layer(u64),
    /// One plane identified together with its owning layer.
    Plane(EditorTarget),
}

/// Maximum number of grouped edit targets retained by one editor session.
pub const MAX_EDIT_TARGETS: usize = 4_096;

/// Cursor into palette presentation; palette content remains document-owned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaletteCursor {
    /// Zero-based palette group.
    pub group: u32,
    /// Zero-based entry within the group.
    pub index: u32,
}

/// Typed Core-owned editor state shared by every view of one document session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorState {
    /// Active command/tool.
    pub active_tool: EditorTool,
    /// Most recently active tool that consumes a current color.
    pub last_color_consuming_tool: Option<EditorTool>,
    pub(crate) tool_styles: BTreeMap<EditorTool, EditorToolStyle>,
    /// Raster brush options.
    pub brush: EditorBrushOptions,
    /// Fill options.
    pub fill: EditorFillOptions,
    /// Selection tool options.
    pub selection: EditorSelectionOptions,
    /// Vector tool options.
    pub vector: EditorVectorOptions,
    /// Stable active document target, absent only before a document exists.
    pub target: Option<EditorTarget>,
    /// Ordered, unique grouped edit targets, independent from the active target.
    pub edit_targets: Vec<EditTarget>,
    /// Palette presentation cursor.
    pub palette_cursor: Option<PaletteCursor>,
}

impl EditorState {
    /// Returns the exact style retained for `tool`.
    #[must_use]
    pub fn tool_style(&self, tool: EditorTool) -> Option<&EditorToolStyle> {
        self.tool_styles.get(&tool)
    }

    /// Returns a copy with all document-specific stable targets removed.
    #[must_use]
    pub fn without_target(&self) -> Self {
        let mut state = self.clone();
        state.target = None;
        state.edit_targets.clear();
        state
    }

    /// Returns the active exact-depth color, falling back to the last color tool.
    #[must_use]
    pub fn current_color(&self) -> Option<PixelValue> {
        self.tool_styles
            .get(&self.active_tool)
            .and_then(|style| style.color)
            .or_else(|| {
                self.last_color_consuming_tool
                    .and_then(|tool| self.tool_styles.get(&tool))
                    .and_then(|style| style.color)
            })
    }

    /// Returns the active tool diameter in signed Q16.16 document pixels.
    #[must_use]
    pub fn current_diameter_q16(&self) -> i64 {
        self.tool_styles
            .get(&self.active_tool)
            .map_or(0, |style| style.diameter_q16)
    }
}

/// BLAKE3-256 digest of the canonical semantic EditorState frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct EditorStateDigest(pub(crate) [u8; 32]);

impl EditorStateDigest {
    /// Borrows the fixed 32 digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Read-only editor-state query result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorStateInfo {
    /// Session-local editor revision, independent of document revision.
    pub revision: EditorRevision,
    /// Digest of canonical semantic editor-state fields.
    pub digest: EditorStateDigest,
    /// Whether the digest differs from the editor savepoint.
    pub dirty: bool,
    /// Owned state snapshot suitable for presentation caching.
    pub state: EditorState,
}

/// One typed, atomic editor-state update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorStateUpdate {
    /// Selects an active tool and updates the last color tool when applicable.
    SetActiveTool(EditorTool),
    /// Replaces one tool's exact-depth current color.
    SetToolColor {
        /// Tool whose current color changes.
        tool: EditorTool,
        /// Straight-alpha RGBA8/RGBA16 color.
        color: PixelValue,
    },
    /// Replaces one tool's Q16.16 diameter.
    SetToolDiameter {
        /// Tool whose diameter changes.
        tool: EditorTool,
        /// Positive Q16.16 document-pixel diameter.
        diameter_q16: i64,
    },
    /// Replaces raster brush shape, smoothing, and start-color options.
    SetBrushOptions(EditorBrushOptions),
    /// Replaces fill options.
    SetFillOptions(EditorFillOptions),
    /// Replaces selection options.
    SetSelectionOptions(EditorSelectionOptions),
    /// Replaces vector options.
    SetVectorOptions(EditorVectorOptions),
    /// Selects a stable active layer/plane pair.
    SetActiveTarget(EditorTarget),
    /// Replaces the grouped edit-target set after document-tree normalization.
    SetEditTargets(Vec<EditTarget>),
    /// Replaces or clears the palette cursor.
    SetPaletteCursor(Option<PaletteCursor>),
}

/// Whether a restored canonical EDIT frame establishes an editor savepoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorFrameDisposition {
    /// The frame is known to be durably stored and becomes clean.
    Saved,
    /// The frame is recovered or otherwise not a normal-save savepoint.
    Unsaved,
}

/// Opaque token authorizing an editor savepoint commit for one exact state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorSavepointToken {
    pub(crate) revision: EditorRevision,
    pub(crate) digest: EditorStateDigest,
}

/// Input whose tool settings and stable target are captured when an editor stroke begins.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorStrokeInput {
    /// Optional raster tool whose Core-owned style is captured; `None` uses the active tool.
    pub tool: Option<EditorTool>,
    /// Coordinate space used by every sample.
    pub coordinate_space: CoordinateSpace,
    /// Whether matching existing pixels are erased.
    pub auto_erase: bool,
    /// Whether pressure scales the captured diameter.
    pub pressure_size: bool,
    /// Ordered pointer samples owned by the input.
    pub samples: Vec<StrokeSample>,
}

/// Session-local semantic EditorState revision, independent of document revision.
///
/// Revision one is assigned when a document session receives its copied built-in
/// state. Only semantic EditorState changes advance it; `u64::MAX` cannot advance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct EditorRevision(u64);

impl EditorRevision {
    pub(crate) const INITIAL: Self = Self(1);

    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Returns the fixed-width numeric representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EditorSessionState {
    pub(crate) state: EditorState,
    pub(crate) revision: EditorRevision,
    pub(crate) digest: EditorStateDigest,
    pub(crate) savepoint: Option<EditorStateDigest>,
}
