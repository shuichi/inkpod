//! Stable public command, input, result, and view data types.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// A command accepted by [`Core::dispatch`].
pub enum Command {
    /// Performs no document or view mutation while still counting as accepted.
    NoOp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Result metadata shared by synchronous Core operations.
///
/// A successful document change increments `revision`; a no-op reports the
/// current revision. Errors return no outcome and publish no partial state.
pub struct DispatchOutcome {
    pub(super) revision: u64,
    pub(super) accepted_commands: u64,
}

impl DispatchOutcome {
    /// Returns the document revision after the operation.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// Returns the number of accepted input commands.
    #[must_use]
    pub const fn accepted_commands(self) -> u64 {
        self.accepted_commands
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// The conventional raster plane targeted by paint operations.
pub enum ActivePlane {
    /// The protected main-line plane.
    MainLine,
    /// The color plane.
    Color,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Semantic role of a plane within a layer.
pub enum PlaneType {
    /// Raster main-line artwork.
    MainLine,
    /// Raster color artwork.
    Color,
    /// General-purpose raster artwork.
    Raster,
    /// Document selection mask.
    Selection,
    /// Vector main-line artwork.
    VectorMainLine,
    /// Vector color-trace artwork.
    ColorTrace,
    /// Vector fill artwork.
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
/// Public metadata for one document plane.
pub struct PlaneInfo {
    /// Stable plane ID, valid until the plane is deleted.
    pub id: u64,
    /// Semantic plane role.
    pub kind: PlaneType,
    /// Stored pixel format for raster-compatible data.
    pub pixel_format: PixelFormat,
    /// User-visible plane name.
    pub name: String,
    /// Whether the plane contributes to normal rendering.
    pub visible: bool,
    /// Whether editing commands may target the plane.
    pub editable: bool,
    /// Opacity in the inclusive range `0..=1000`.
    pub opacity_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Public metadata and ordered plane topology for one document layer.
pub struct LayerInfo {
    /// Stable layer ID, valid until the layer is deleted.
    pub id: u64,
    /// Semantic layer kind.
    pub kind: LayerKind,
    /// User-visible layer name.
    pub name: String,
    /// Whether the layer contributes to normal rendering.
    pub visible: bool,
    /// Whether editing commands may target the layer.
    pub editable: bool,
    /// Opacity in the inclusive range `0..=1000`.
    pub opacity_milli: u32,
    /// Planes in document stacking order.
    pub planes: Vec<PlaneInfo>,
}

/// A bounded, aspect-preserving, straight-alpha RGBA8 preview of one layer.
///
/// The preview is derived without changing visibility, selection, revision, or
/// history. `pixels` is packed top-to-bottom with `stride_bytes == width * 4`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerThumbnail {
    /// Document revision from which the preview was derived.
    pub revision: u64,
    /// Stable ID of the previewed layer.
    pub layer_id: u64,
    /// Preview width in pixels.
    pub width: u32,
    /// Preview height in pixels.
    pub height: u32,
    /// Number of bytes between adjacent preview rows.
    pub stride_bytes: u32,
    /// Owned straight-alpha RGBA8 pixel bytes.
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Boolean operation used to combine a shape with the document selection.
pub enum SelectionOperation {
    /// Replaces the existing selection.
    New,
    /// Adds the shape to the existing selection.
    Add,
    /// Removes the shape from the existing selection.
    Subtract,
    /// Keeps only the intersection with the existing selection.
    Intersect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// A two-dimensional point with coordinates expressed by the enclosing API.
pub struct PointF32 {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
/// Geometry used to build a document-space selection mask.
pub enum SelectionShape {
    /// Half-open document-pixel rectangle.
    Rectangle(RectI32),
    /// Ellipse inscribed in a half-open document-pixel rectangle.
    Ellipse(RectI32),
    /// Closed polygon formed by document-space points.
    Lasso(Vec<PointF32>),
    /// Closed polygonal path formed by document-space points.
    Polyline(Vec<PointF32>),
    /// Stroked document-space path.
    Trace {
        /// Ordered path samples in document coordinates.
        points: Vec<PointF32>,
        /// Positive brush diameter in document pixels.
        diameter: f32,
    },
    /// Color-similarity selection seeded at one document pixel.
    Wand {
        /// Seed x-coordinate in document pixels.
        x: u32,
        /// Seed y-coordinate in document pixels.
        y: u32,
        /// Inclusive channel tolerance used by the fill search.
        tolerance: u16,
        /// Maximum bounded gap-closing distance.
        gap_close: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Boolean operation used when merging a source selection layer.
pub enum SelectionLayerOperation {
    /// Replaces the destination selection.
    Replace,
    /// Adds source coverage to the destination.
    Add,
    /// Removes source coverage from the destination.
    Subtract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Axis about which document or view content is mirrored.
pub enum MirrorAxis {
    /// Mirrors left to right.
    Horizontal,
    /// Mirrors top to bottom.
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Direction of a right-angle document rotation.
pub enum RotateDirection {
    /// Rotates 90 degrees counter-clockwise.
    Left90,
    /// Rotates 90 degrees clockwise.
    Right90,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Fixed point retained while resizing a document canvas.
pub enum ResizeAnchor {
    /// Retains the top-left point.
    TopLeft,
    /// Retains the top-right point.
    TopRight,
    /// Retains the center point.
    Center,
    /// Retains the bottom-left point.
    BottomLeft,
    /// Retains the bottom-right point.
    BottomRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Validated target dimensions and resolution for a document resize.
pub struct DocumentResize {
    /// Target width in document pixels.
    pub width: u32,
    /// Target height in document pixels.
    pub height: u32,
    /// Target horizontal resolution in thousandths of a DPI.
    pub dpi_x_milli: u32,
    /// Target vertical resolution in thousandths of a DPI.
    pub dpi_y_milli: u32,
    /// Whether existing raster content is resampled to the new dimensions.
    pub resample: bool,
    /// Anchor controlling placement when content is not resampled.
    pub anchor: ResizeAnchor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// One horizontal or vertical document guide.
pub struct Guide {
    /// Stable guide ID, valid until the guide is deleted.
    pub id: u64,
    /// Guide orientation.
    pub axis: GuideAxis,
    /// Signed position in document pixels along the guide's normal axis.
    pub position: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Document-space grid configuration used for display and snapping.
pub struct GridConfig {
    /// Horizontal origin in document pixels.
    pub origin_x: i32,
    /// Vertical origin in document pixels.
    pub origin_y: i32,
    /// Positive horizontal spacing in document pixels.
    pub spacing_x: u32,
    /// Positive vertical spacing in document pixels.
    pub spacing_y: u32,
    /// Number of subdivisions per major grid interval.
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
/// Affine transform applied to a staged floating selection.
pub struct FloatingTransform {
    /// Horizontal translation in document pixels.
    pub translate_x: f64,
    /// Vertical translation in document pixels.
    pub translate_y: f64,
    /// Horizontal scale factor.
    pub scale_x: f64,
    /// Vertical scale factor.
    pub scale_y: f64,
    /// Clockwise rotation in degrees.
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
/// One non-transparent clipboard sample in document-relative coordinates.
pub struct ClipboardPixel {
    /// Horizontal coordinate relative to the clipboard plane origin.
    pub x: i32,
    /// Vertical coordinate relative to the clipboard plane origin.
    pub y: i32,
    /// Straight-alpha pixel value.
    pub value: PixelValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Clipboard samples and metadata for one plane.
pub struct ClipboardPlane {
    /// Semantic role of the source plane.
    pub kind: PlaneType,
    /// Pixel format of `pixels`.
    pub pixel_format: PixelFormat,
    /// Horizontal plane origin in source document coordinates.
    pub origin_x: i32,
    /// Vertical plane origin in source document coordinates.
    pub origin_y: i32,
    /// Owned non-transparent pixel samples.
    pub pixels: Vec<ClipboardPixel>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Application clipboard payload preserving document-space plane structure.
pub struct ClipboardPayload {
    /// UUID of the source document.
    pub source_document_uuid: u128,
    /// Half-open source bounds in document pixels.
    pub bounds: RectI32,
    /// Owned planes copied from the source selection.
    pub planes: Vec<ClipboardPlane>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Read-only locator result at one device-space pointer position.
pub struct LocatorSample {
    /// Resolved x-coordinate in document pixels.
    pub document_x: i32,
    /// Resolved y-coordinate in document pixels.
    pub document_y: i32,
    /// Current half-open selection bounds, if non-empty.
    pub selection_bounds: Option<RectI32>,
    /// Sampled active-plane color when the point is inside the document.
    pub color: Option<PixelValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Bounded straight-RGBA8 neighborhood centered on one locator sample.
pub struct LocatorNeighborhood {
    /// Document-space x-coordinate of the first returned pixel.
    pub origin_x: i32,
    /// Document-space y-coordinate of the first returned pixel.
    pub origin_y: i32,
    /// Square output width in pixels.
    pub width: u32,
    /// Square output height in pixels.
    pub height: u32,
    /// Packed row-major straight RGBA8 pixels. Out-of-document pixels are transparent.
    pub pixels_rgba8: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Legacy single-stroke shortcut binding.
pub struct ShortcutBinding {
    /// Application command identifier.
    pub command_id: u32,
    /// Platform-normalized virtual-key code.
    pub virtual_key: u32,
    /// Bitwise combination of `SHORTCUT_MODIFIER_*` values.
    pub modifiers: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// One normalized key stroke in a shortcut sequence.
pub struct ShortcutStroke {
    /// Platform-normalized virtual-key code.
    pub virtual_key: u32,
    /// Bitwise combination of `SHORTCUT_MODIFIER_*` values.
    pub modifiers: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Prefix-free multi-stroke shortcut binding.
pub struct ShortcutSequenceBinding {
    /// Application command identifier.
    pub command_id: u32,
    /// Ordered, non-empty stroke sequence.
    pub strokes: Vec<ShortcutStroke>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Result of resolving entered strokes against shortcut bindings.
pub enum ShortcutSequenceMatch {
    /// No binding starts with the entered strokes.
    None,
    /// At least one longer binding starts with the entered strokes.
    Prefix,
    /// An exact binding was found, containing its command ID.
    Exact(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Raster paint tool used by a stroke.
pub enum PaintTool {
    /// Hard-edged pencil.
    Pencil,
    /// Soft or pressure-sensitive brush.
    Brush,
    /// Eraser targeting the selected plane.
    Eraser,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Region-growth algorithm used by [`FillRequest`].
pub enum FillOperation {
    /// Flood fill from one seed pixel.
    Seed,
    /// Fill a region enclosed by the supplied selection geometry.
    ClosedRegion,
    /// Extend existing color into adjacent pixels.
    Extend,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Bounded input to a transactional raster fill operation.
pub struct FillRequest {
    /// Fill algorithm to run.
    pub operation: FillOperation,
    /// Seed x-coordinate in document pixels.
    pub seed_x: u32,
    /// Seed y-coordinate in document pixels.
    pub seed_y: u32,
    /// Straight-alpha fill color.
    pub color: PixelValue,
    /// Optional half-open document region limiting the operation.
    pub selection: Option<RectI32>,
    /// Whether the current document selection also limits the operation.
    pub use_document_selection: bool,
    /// Inclusive channel comparison tolerance.
    pub tolerance: u16,
    /// Whether disconnected matching regions may be filled.
    pub detached_regions: bool,
    /// Whether bounded work overflow returns an error instead of a partial fill.
    pub overflow_abort: bool,
    /// Maximum bounded gap-closing distance.
    pub gap_close: u8,
    /// Whether only transparent destination pixels may change.
    pub transparent_only: bool,
    /// Inclusion-color matching mode.
    pub inclusion_mode: InclusionMode,
    /// Colors used by `inclusion_mode`.
    pub inclusion_colors: Vec<PixelValue>,
    /// Extension distance in document pixels for [`FillOperation::Extend`].
    pub extension_distance: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Result of a fill, including committed pixel count.
pub struct FillOutcome {
    /// Revision and accepted-command metadata.
    pub dispatch: DispatchOutcome,
    /// Number of destination pixels changed by the committed operation.
    pub changed_pixels: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Coordinate system used by pointer and stroke input.
pub enum CoordinateSpace {
    /// Document pixels before view transformation.
    Document,
    /// Canvas client device pixels after view transformation.
    Device,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// One ordered pointer sample in a stroke.
pub struct StrokeSample {
    /// Horizontal coordinate in the stroke's [`CoordinateSpace`].
    pub x: f32,
    /// Vertical coordinate in the stroke's [`CoordinateSpace`].
    pub y: f32,
    /// Normalized pressure in the inclusive range `0.0..=1.0`.
    pub pressure: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Geometry kind used to bound local effect operations.
pub enum EffectRegionKind {
    /// A stroked trace region.
    Trace,
    /// A rectangular region.
    Rectangle,
    /// A polygonal region.
    Polyline,
    /// A freehand closed region.
    Lasso,
}

#[derive(Clone, Debug, PartialEq)]
/// Complete input for an atomic raster stroke.
///
/// [`Core::apply_stroke`] commits all samples as one history entry. Invalid
/// input or processing failure leaves the document, revision, and history intact.
pub struct Stroke {
    /// Paint algorithm.
    pub tool: PaintTool,
    /// Conventional raster plane to target.
    pub plane: ActivePlane,
    /// Straight-alpha RGBA8 stroke color.
    pub color: [u8; 4],
    /// Positive diameter in document pixels.
    pub diameter: f32,
    /// Whether matching existing color is erased instead of painted.
    pub auto_erase: bool,
    /// Whether pressure scales the diameter.
    pub pressure_size: bool,
    /// Coordinate system of every sample.
    pub coordinate_space: CoordinateSpace,
    /// Ordered pointer samples; at least one sample is required.
    pub samples: Vec<StrokeSample>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// A view-only command that never changes document revision, history, or dirty state.
pub enum ViewCommand {
    /// Translates the view by a device-pixel delta.
    PanBy {
        /// Horizontal device-pixel delta.
        device_dx: f64,
        /// Vertical device-pixel delta.
        device_dy: f64,
    },
    /// Multiplies zoom while retaining a device-space focal point.
    ZoomAt {
        /// Positive zoom multiplier.
        factor: f64,
        /// Focal x-coordinate in device pixels.
        device_x: f64,
        /// Focal y-coordinate in device pixels.
        device_y: f64,
    },
    /// Fits the whole document into a device-pixel viewport.
    Fit {
        /// Positive viewport width in device pixels.
        viewport_width: f64,
        /// Positive viewport height in device pixels.
        viewport_height: f64,
    },
    /// Selects one document pixel per device pixel and centers the document.
    OneToOne {
        /// Positive viewport width in device pixels.
        viewport_width: f64,
        /// Positive viewport height in device pixels.
        viewport_height: f64,
    },
    /// Records a new viewport and recomputes automatic view modes.
    ViewportResized {
        /// Positive viewport width in device pixels.
        viewport_width: f64,
        /// Positive viewport height in device pixels.
        viewport_height: f64,
    },
    /// Fits a half-open document rectangle into a device-pixel viewport.
    BoxZoom {
        /// Non-empty half-open rectangle in document pixels.
        document_rect: RectI32,
        /// Positive viewport width in device pixels.
        viewport_width: f64,
        /// Positive viewport height in device pixels.
        viewport_height: f64,
    },
    /// Toggles a non-destructive view flip.
    Flip {
        /// View axis to toggle.
        axis: MirrorAxis,
    },
    /// Sets ruler visibility.
    SetRulerVisible(bool),
    /// Sets guide visibility.
    SetGuidesVisible(bool),
    /// Sets grid visibility.
    SetGridVisible(bool),
    /// Enables or disables all snapping.
    SetSnapEnabled(bool),
    /// Enables or disables guide snapping.
    SetGuideSnapEnabled(bool),
    /// Enables or disables grid snapping.
    SetGridSnapEnabled(bool),
    /// Selects transparent-composite visualization.
    SetTransparentView(bool),
    /// Selects alpha-channel visualization.
    SetAlphaView(bool),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Policy controlling how the current zoom and pan are maintained.
pub enum ViewMode {
    /// Zoom and pan are explicitly controlled by the caller.
    Manual,
    /// The whole document is fitted to the viewport.
    Fit,
    /// One document pixel maps to one device pixel.
    OneToOne,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Immutable public view state expressed in Canvas device pixels.
pub struct ViewState {
    pub(super) zoom: ZoomFactor,
    pub(super) pan: DeviceOffsetF64,
    pub(super) revision: ViewRevision,
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
    pub(super) viewport: DeviceSizeF64,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: ZoomFactor::ONE,
            pan: DeviceOffsetF64::ZERO,
            revision: ViewRevision::from_raw(0),
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
            viewport: DeviceSizeF64::ONE,
        }
    }
}

impl ViewState {
    /// Returns the document-to-device scale factor in `0.01..=64.0`.
    #[must_use]
    pub const fn zoom(self) -> f64 {
        self.zoom.get()
    }

    /// Returns horizontal pan in device pixels.
    #[must_use]
    pub const fn pan_x(self) -> f64 {
        self.pan.x
    }

    /// Returns vertical pan in device pixels.
    #[must_use]
    pub const fn pan_y(self) -> f64 {
        self.pan.y
    }

    /// Returns the view-only revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision.get()
    }

    /// Returns the active view maintenance policy.
    #[must_use]
    pub const fn mode(self) -> ViewMode {
        self.mode
    }

    /// Reports whether horizontal view flip is enabled.
    #[must_use]
    pub const fn flip_horizontal(self) -> bool {
        self.flip_horizontal
    }

    /// Reports whether vertical view flip is enabled.
    #[must_use]
    pub const fn flip_vertical(self) -> bool {
        self.flip_vertical
    }

    /// Reports whether rulers are visible.
    #[must_use]
    pub const fn ruler_visible(self) -> bool {
        self.ruler_visible
    }

    /// Reports whether guides are visible.
    #[must_use]
    pub const fn guides_visible(self) -> bool {
        self.guides_visible
    }

    /// Reports whether the grid is visible.
    #[must_use]
    pub const fn grid_visible(self) -> bool {
        self.grid_visible
    }

    /// Reports whether snapping is globally enabled.
    #[must_use]
    pub const fn snap_enabled(self) -> bool {
        self.snap_enabled
    }

    /// Reports whether guide snapping is enabled.
    #[must_use]
    pub const fn guide_snap_enabled(self) -> bool {
        self.guide_snap_enabled
    }

    /// Reports whether grid snapping is enabled.
    #[must_use]
    pub const fn grid_snap_enabled(self) -> bool {
        self.grid_snap_enabled
    }

    /// Reports whether transparent-composite visualization is enabled.
    #[must_use]
    pub const fn transparent_view(self) -> bool {
        self.transparent_view
    }

    /// Reports whether alpha-channel visualization is enabled.
    #[must_use]
    pub const fn alpha_view(self) -> bool {
        self.alpha_view
    }

    /// Returns the current viewport width in device pixels.
    #[must_use]
    pub const fn viewport_width(self) -> f64 {
        self.viewport.width
    }

    /// Returns the current viewport height in device pixels.
    #[must_use]
    pub const fn viewport_height(self) -> f64 {
        self.viewport.height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Read-only summary of the active document and its observable state.
pub struct DocumentInfo {
    /// Revision of committed document content and metadata.
    pub document_revision: u64,
    /// Independent revision of view-only state.
    pub view_revision: u64,
    /// Stable document ID within this Core instance.
    pub document_id: u64,
    /// Persistent document UUID stored in the native file.
    pub document_uuid: u128,
    /// Stable ID of the active layer.
    pub layer_id: u64,
    /// Stable ID of the active layer's main-line plane.
    pub main_plane_id: u64,
    /// Stable ID of the active layer's color plane.
    pub color_plane_id: u64,
    /// Document width in pixels.
    pub width: u32,
    /// Document height in pixels.
    pub height: u32,
    /// Horizontal resolution in thousandths of a DPI.
    pub dpi_x_milli: u32,
    /// Vertical resolution in thousandths of a DPI.
    pub dpi_y_milli: u32,
    /// Frame and margin metadata in document pixels.
    pub frames: FrameMetadata,
    /// Whether current history state differs from the normal-save savepoint.
    pub dirty: bool,
    /// Whether one history step can be undone.
    pub can_undo: bool,
    /// Whether one history step can be redone.
    pub can_redo: bool,
    /// Conventional active raster plane.
    pub active_plane: ActivePlane,
    /// Whether the document was opened from recovery data.
    pub recovered: bool,
    /// Deterministic checksum of the active main-line plane.
    pub main_plane_checksum: u64,
    /// Deterministic checksum of the active color plane.
    pub color_plane_checksum: u64,
}
