#![deny(unsafe_op_in_unsafe_fn)]

mod m7;

use inkpod_core::{
    ActivePlane, Adjustment, AirbrushGesture, AirbrushStroke, BoundaryAirbrush, Channel,
    ClipboardPayload, ClipboardPixel, ClipboardPlane, ColorBalance, ColorCheckMode, Command,
    CommonRasterFormat, CoordinateSpace, Core, CoreError, CurveInterpolation, CurvePoint,
    DocumentInfo, DocumentResize, DustMode, DustRemoval, EffectRegionKind, EyedropperSource,
    FillOperation, FillRequest, Filter, FloatingTransform, FrameMetadata, Gradient, GradientKind,
    GradientMode, GradientStop, GridConfig, GuideAxis, HsvAdjustment, InclusionMode, LayerKind,
    Levels, LightTableDisplayMode, LightTableItemInput, LightTableItemProperties, LightTableSource,
    MAX_COMMON_RASTER_BYTES, MAX_GRADIENT_STOPS, MAX_IMAGE_EDIT_PIXELS, MAX_RASTER_DIMENSION,
    Margins, MirrorAxis, MotionCheckConfig, MotionFrame, PaintTool, PixelFormat, PixelValue,
    PlaneType, PointF32, RectI32, RenderSnapshot, ResizeAnchor, RgbaRasterBytes, RotateDirection,
    SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE, SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA,
    SelectionLayerOperation, SelectionOperation, SelectionShape, SequenceCellInfo,
    SequenceCellSource, SequenceDirection, ShortcutBinding, Stamp, StampGesture, StampShape,
    Stroke, StrokeSample, TileRaster, VectorCubicSegment, VectorEraseMode, VectorPathInput,
    VectorSelectionMode, VectorWidthMode, ViewCommand,
};
use std::cell::RefCell;
use std::mem::{align_of, size_of};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr;
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::thread::{self, ThreadId};

pub const INKPOD_ABI_VERSION: u32 = 1;
pub const INKPOD_FEATURE_NONE: u64 = 0;

pub const INKPOD_STATUS_OK: u32 = 0;
pub const INKPOD_STATUS_INVALID_ARGUMENT: u32 = 1;
pub const INKPOD_STATUS_INCOMPATIBLE_ABI: u32 = 2;
pub const INKPOD_STATUS_BUFFER_TOO_SMALL: u32 = 3;
pub const INKPOD_STATUS_UNSUPPORTED: u32 = 4;
pub const INKPOD_STATUS_PANIC: u32 = 5;
pub const INKPOD_STATUS_WRONG_THREAD: u32 = 6;
pub const INKPOD_STATUS_IO_ERROR: u32 = 7;
pub const INKPOD_STATUS_INVALID_STATE: u32 = 8;
pub const INKPOD_STATUS_NO_DOCUMENT: u32 = 9;
pub const INKPOD_STATUS_CANCELLED: u32 = 10;
pub const INKPOD_STATUS_FILL_OVERFLOW: u32 = 11;
pub const INKPOD_STATUS_UNSAVED_CHANGES: u32 = 12;

pub const INKPOD_COMMAND_NO_OP: u32 = 0;
pub const INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8: u32 = 1;
pub const INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE: u64 =
    SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE;
pub const INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA: u64 =
    SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA;
pub const INKPOD_PLANE_MAIN_LINE: u32 = 1;
pub const INKPOD_PLANE_COLOR: u32 = 2;
pub const INKPOD_TOOL_PENCIL: u32 = 1;
pub const INKPOD_TOOL_BRUSH: u32 = 2;
pub const INKPOD_TOOL_ERASER: u32 = 3;
pub const INKPOD_COORDINATE_SPACE_DOCUMENT: u32 = 1;
pub const INKPOD_COORDINATE_SPACE_DEVICE: u32 = 2;
pub const INKPOD_STROKE_FLAG_AUTO_ERASE: u64 = 1 << 0;
pub const INKPOD_STROKE_FLAG_PRESSURE_SIZE: u64 = 1 << 1;
pub const INKPOD_DOCUMENT_FLAG_DIRTY: u32 = 1 << 0;
pub const INKPOD_DOCUMENT_FLAG_CAN_UNDO: u32 = 1 << 1;
pub const INKPOD_DOCUMENT_FLAG_CAN_REDO: u32 = 1 << 2;
pub const INKPOD_DOCUMENT_FLAG_RECOVERED: u32 = 1 << 3;
pub const INKPOD_HISTORY_ITEM_APPLIED: u32 = 1 << 0;
pub const INKPOD_VIEW_PAN_BY: u32 = 1;
pub const INKPOD_VIEW_ZOOM_AT: u32 = 2;
pub const INKPOD_VIEW_FIT: u32 = 3;
pub const INKPOD_VIEW_ONE_TO_ONE: u32 = 4;
pub const INKPOD_VIEW_VIEWPORT_RESIZED: u32 = 5;
pub const INKPOD_VIEW_BOX_ZOOM: u32 = 6;
pub const INKPOD_VIEW_FLIP_HORIZONTAL: u32 = 7;
pub const INKPOD_VIEW_FLIP_VERTICAL: u32 = 8;
pub const INKPOD_VIEW_SET_RULER_VISIBLE: u32 = 9;
pub const INKPOD_VIEW_SET_GUIDES_VISIBLE: u32 = 10;
pub const INKPOD_VIEW_SET_GRID_VISIBLE: u32 = 11;
pub const INKPOD_VIEW_SET_SNAP_ENABLED: u32 = 12;
pub const INKPOD_VIEW_SET_GUIDE_SNAP_ENABLED: u32 = 15;
pub const INKPOD_VIEW_SET_GRID_SNAP_ENABLED: u32 = 16;
pub const INKPOD_VIEW_SET_TRANSPARENT_VISIBLE: u32 = 13;
pub const INKPOD_VIEW_SET_ALPHA_VISIBLE: u32 = 14;
pub const INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL: u32 = 1 << 0;
pub const INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL: u32 = 1 << 1;
pub const INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE: u32 = 1 << 0;
pub const INKPOD_SNAPSHOT_OVERLAY_GUIDES_VISIBLE: u32 = 1 << 1;
pub const INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE: u32 = 1 << 2;
pub const INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED: u32 = 1 << 3;
pub const INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW: u32 = 1 << 4;
pub const INKPOD_SNAPSHOT_OVERLAY_ALPHA_VIEW: u32 = 1 << 5;
pub const INKPOD_SHORTCUT_MODIFIER_CONTROL: u32 = 1 << 0;
pub const INKPOD_SHORTCUT_MODIFIER_SHIFT: u32 = 1 << 1;
pub const INKPOD_SHORTCUT_MODIFIER_ALT: u32 = 1 << 2;
pub const INKPOD_SHORTCUT_MODIFIER_EXTENDED: u32 = 1 << 3;
pub const INKPOD_COLOR_DEPTH_8: u32 = 8;
pub const INKPOD_COLOR_DEPTH_16: u32 = 16;
pub const INKPOD_COLOR_DEPTH_BINARY: u32 = 1;
pub const INKPOD_COLOR_DEPTH_GRAYSCALE_8: u32 = 2;
pub const INKPOD_COLOR_DEPTH_GRAYSCALE_16: u32 = 3;
pub const INKPOD_COMMON_RASTER_PNG: u32 = 1;
pub const INKPOD_COMMON_RASTER_TIFF: u32 = 2;
pub const INKPOD_COMMON_RASTER_TGA: u32 = 3;
pub const INKPOD_COMMON_RASTER_BMP: u32 = 4;
pub const INKPOD_FILL_SEED: u32 = 1;
pub const INKPOD_FILL_CLOSED_REGION: u32 = 2;
pub const INKPOD_FILL_EXTENSION: u32 = 3;
pub const INKPOD_FILL_FLAG_DETACHED_REGIONS: u64 = 1 << 0;
pub const INKPOD_FILL_FLAG_OVERFLOW_ABORT: u64 = 1 << 1;
pub const INKPOD_FILL_FLAG_TRANSPARENT_ONLY: u64 = 1 << 2;
pub const INKPOD_FILL_FLAG_SELECTION_PRESENT: u64 = 1 << 3;
pub const INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY: u64 = 1 << 4;
pub const INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR: u64 = 1 << 5;
pub const INKPOD_FILL_FLAG_DOCUMENT_SELECTION: u64 = 1 << 6;
pub const INKPOD_INCLUSION_NONE: u32 = 0;
pub const INKPOD_INCLUSION_SPECIFIED: u32 = 1;
pub const INKPOD_INCLUSION_EXCEPT_SPECIFIED: u32 = 2;
pub const INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE: u32 = 1 << 0;
pub const INKPOD_EYEDROPPER_TOPMOST_NONTRANSPARENT: u32 = 1;
pub const INKPOD_EYEDROPPER_SELECTED_PLANE: u32 = 2;
pub const INKPOD_EYEDROPPER_COMPOSITE: u32 = 3;
pub const INKPOD_EYEDROPPER_LIGHT_TABLE_TOPMOST: u32 = 4;
pub const INKPOD_COLOR_CHECK_OFF: u32 = 0;
pub const INKPOD_COLOR_CHECK_LEGACY_WHITE: u32 = 1;
pub const INKPOD_COLOR_CHECK_NATIVE_ALPHA: u32 = 2;
pub const INKPOD_LAYER_BINARY_COLORING: u32 = 1;
pub const INKPOD_LAYER_GRAYSCALE_COLORING: u32 = 2;
pub const INKPOD_LAYER_RASTER: u32 = 3;
pub const INKPOD_LAYER_SELECTION: u32 = 4;
pub const INKPOD_LAYER_FRAME: u32 = 5;
pub const INKPOD_LAYER_VANISHING_POINT: u32 = 6;
pub const INKPOD_LAYER_ADJUSTMENT: u32 = 7;
pub const INKPOD_LAYER_TEXT: u32 = 8;
pub const INKPOD_LAYER_ANNOTATION: u32 = 9;
pub const INKPOD_LAYER_VECTOR_COLORING: u32 = 10;
pub const INKPOD_TYPED_PLANE_MAIN_LINE: u32 = 1;
pub const INKPOD_TYPED_PLANE_COLOR: u32 = 2;
pub const INKPOD_TYPED_PLANE_RASTER: u32 = 3;
pub const INKPOD_TYPED_PLANE_SELECTION: u32 = 4;
pub const INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE: u32 = 5;
pub const INKPOD_TYPED_PLANE_COLOR_TRACE: u32 = 6;
pub const INKPOD_TYPED_PLANE_VECTOR_FILL: u32 = 7;
pub const INKPOD_STORAGE_BINARY8: u32 = 1;
pub const INKPOD_STORAGE_GRAYSCALE8: u32 = 2;
pub const INKPOD_STORAGE_GRAYSCALE16: u32 = 3;
pub const INKPOD_STORAGE_RGBA8: u32 = 4;
pub const INKPOD_STORAGE_RGBA16: u32 = 5;
pub const INKPOD_LIGHT_TABLE_COLOR: u32 = 1;
pub const INKPOD_LIGHT_TABLE_MONOTONE: u32 = 2;
pub const INKPOD_LIGHT_TABLE_HALFTONE: u32 = 3;
pub const INKPOD_LIGHT_TABLE_ITEM_VISIBLE: u32 = 1 << 0;
pub const INKPOD_LIGHT_TABLE_SET_ACTIVE: u32 = 1 << 1;
pub const INKPOD_LIGHT_TABLE_CREATE_SET: u32 = 1;
pub const INKPOD_LIGHT_TABLE_DUPLICATE_SET: u32 = 2;
pub const INKPOD_LIGHT_TABLE_DELETE_SET: u32 = 3;
pub const INKPOD_LIGHT_TABLE_RENAME_SET: u32 = 4;
pub const INKPOD_LIGHT_TABLE_REORDER_SET: u32 = 5;
pub const INKPOD_LIGHT_TABLE_SET_ACTIVE_OPERATION: u32 = 6;
pub const INKPOD_LIGHT_TABLE_REMOVE_ITEM: u32 = 7;
pub const INKPOD_LIGHT_TABLE_REORDER_ITEM: u32 = 8;
pub const INKPOD_LIGHT_TABLE_UPDATE_ITEM: u32 = 9;
pub const INKPOD_SEQUENCE_PREVIOUS: u32 = 1;
pub const INKPOD_SEQUENCE_NEXT: u32 = 2;
pub const INKPOD_SEQUENCE_FLAG_LOOP: u32 = 1 << 0;
pub const INKPOD_MOTION_FLAG_LOOP: u64 = 1 << 0;
pub const INKPOD_MOTION_FLAG_INCLUDE_SELECTION: u64 = 1 << 1;
pub const INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE: u64 = 1 << 2;
pub const INKPOD_MOTION_FRAME_PAUSED: u32 = 1 << 0;
pub const INKPOD_MOTION_FRAME_INCLUDE_SELECTION: u32 = 1 << 1;
pub const INKPOD_MOTION_FRAME_INCLUDE_LIGHT_TABLE: u32 = 1 << 2;
pub const INKPOD_VECTOR_PATH_CLOSED: u64 = 1 << 0;
pub const INKPOD_VECTOR_ERASE_PARTIAL: u32 = 1;
pub const INKPOD_VECTOR_ERASE_TO_INTERSECTION: u32 = 2;
pub const INKPOD_VECTOR_ERASE_WHOLE_PATH: u32 = 3;
pub const INKPOD_VECTOR_WIDTH_ADD: u32 = 1;
pub const INKPOD_VECTOR_WIDTH_SUBTRACT: u32 = 2;
pub const INKPOD_VECTOR_WIDTH_SCALE: u32 = 3;
pub const INKPOD_VECTOR_WIDTH_CONSTANT: u32 = 4;
pub const INKPOD_VECTOR_SELECT_CUT_BY_SELECTION: u32 = 1;
pub const INKPOD_VECTOR_SELECT_TOUCHING: u32 = 2;
pub const INKPOD_VECTOR_SELECT_FULLY_CONTAINED: u32 = 3;
pub const INKPOD_VECTOR_SELECT_LINE: u32 = 4;
pub const INKPOD_VECTOR_SELECT_WHOLE_LINE: u32 = 5;
pub const INKPOD_VECTOR_SELECT_TO_INTERSECTION: u32 = 6;
pub const INKPOD_VECTOR_SELECT_FILL_BOUNDARY: u32 = 7;
pub const INKPOD_VECTOR_SELECT_FILL: u32 = 8;
pub const INKPOD_VECTOR_RASTERIZE_ANTIALIAS: u64 = 1 << 0;
pub const INKPOD_SNAPSHOT_VECTOR_CLOSED: u32 = 1 << 0;
pub const INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE: u32 = 1 << 1;
pub const INKPOD_FILTER_SHARPEN_WEAK: u32 = 1;
pub const INKPOD_FILTER_SHARPEN_STRONG: u32 = 2;
pub const INKPOD_FILTER_BLUR_WEAK: u32 = 3;
pub const INKPOD_FILTER_BLUR_STRONG: u32 = 4;
pub const INKPOD_FILTER_GAUSSIAN_BLUR: u32 = 5;
pub const INKPOD_FILTER_INVERT: u32 = 6;
pub const INKPOD_FILTER_AUTO_CONTRAST: u32 = 7;
pub const INKPOD_FILTER_BRIGHTNESS_CONTRAST: u32 = 8;
pub const INKPOD_FILTER_TONE_CURVE: u32 = 9;
pub const INKPOD_FILTER_LEVELS: u32 = 10;
pub const INKPOD_FILTER_HSV: u32 = 11;
pub const INKPOD_FILTER_COLOR_BALANCE: u32 = 12;
pub const INKPOD_FILTER_UNSHARP_MASK: u32 = 13;
pub const INKPOD_FILTER_CHANNEL_RGB: u32 = 1;
pub const INKPOD_FILTER_CHANNEL_RED: u32 = 2;
pub const INKPOD_FILTER_CHANNEL_GREEN: u32 = 3;
pub const INKPOD_FILTER_CHANNEL_BLUE: u32 = 4;
pub const INKPOD_CURVE_BEZIER: u32 = 1;
pub const INKPOD_CURVE_BSPLINE: u32 = 2;
pub const INKPOD_GRADIENT_LINEAR: u32 = 1;
pub const INKPOD_GRADIENT_RADIAL: u32 = 2;
pub const INKPOD_GRADIENT_COMPOSITE: u32 = 1;
pub const INKPOD_GRADIENT_OVERWRITE: u32 = 2;
pub const INKPOD_GRADIENT_FLAG_CONSTRAIN_45: u64 = 1 << 0;
pub const INKPOD_EFFECT_FLAG_PRESSURE_SIZE: u64 = 1 << 0;
pub const INKPOD_EFFECT_FLAG_PRESSURE_OPACITY: u64 = 1 << 1;
pub const INKPOD_STAMP_ROUND: u32 = 1;
pub const INKPOD_STAMP_SQUARE: u32 = 2;
pub const INKPOD_DUST_REMOVE_FOREGROUND: u32 = 1;
pub const INKPOD_DUST_FILL_TRANSPARENT_HOLES: u32 = 2;
pub const INKPOD_DUST_REPLACE_COLOR_OUTLIERS: u32 = 3;
pub const INKPOD_M6_TASK_READY: u32 = 0;
pub const INKPOD_M6_TASK_RUNNING: u32 = 1;
pub const INKPOD_M6_TASK_COMPLETED: u32 = 2;
pub const INKPOD_M6_TASK_CANCELLED: u32 = 3;
pub const INKPOD_M6_TASK_FAILED: u32 = 4;
pub const INKPOD_TREE_CREATE_LAYER: u32 = 1;
pub const INKPOD_TREE_DUPLICATE_LAYER: u32 = 2;
pub const INKPOD_TREE_DELETE_LAYER: u32 = 3;
pub const INKPOD_TREE_REORDER_LAYER: u32 = 4;
pub const INKPOD_TREE_SET_LAYER_PROPERTIES: u32 = 5;
pub const INKPOD_TREE_CREATE_PLANE: u32 = 6;
pub const INKPOD_TREE_DUPLICATE_PLANE: u32 = 7;
pub const INKPOD_TREE_DELETE_PLANE: u32 = 8;
pub const INKPOD_TREE_REORDER_PLANE: u32 = 9;
pub const INKPOD_TREE_SET_PLANE_PROPERTIES: u32 = 10;
pub const INKPOD_TREE_CONVERT_LAYER: u32 = 11;
pub const INKPOD_TREE_MERGE_LAYER: u32 = 12;
pub const INKPOD_TREE_CONVERT_PLANE: u32 = 13;
pub const INKPOD_TREE_MERGE_PLANE: u32 = 14;
pub const INKPOD_NODE_VISIBLE: u64 = 1 << 0;
pub const INKPOD_NODE_EDITABLE: u64 = 1 << 1;
pub const INKPOD_SELECTION_RECTANGLE: u32 = 1;
pub const INKPOD_SELECTION_ELLIPSE: u32 = 2;
pub const INKPOD_SELECTION_LASSO: u32 = 3;
pub const INKPOD_SELECTION_POLYLINE: u32 = 4;
pub const INKPOD_SELECTION_TRACE: u32 = 5;
pub const INKPOD_SELECTION_WAND: u32 = 6;
pub const INKPOD_SELECTION_NEW: u32 = 1;
pub const INKPOD_SELECTION_ADD: u32 = 2;
pub const INKPOD_SELECTION_SUBTRACT: u32 = 3;
pub const INKPOD_SELECTION_INTERSECT: u32 = 4;
pub const INKPOD_SELECTION_ADJUST_INVERT: u32 = 1;
pub const INKPOD_SELECTION_ADJUST_EXPAND: u32 = 2;
pub const INKPOD_SELECTION_ADJUST_SHRINK: u32 = 3;
pub const INKPOD_SELECTION_LAYER_REPLACE: u32 = 1;
pub const INKPOD_SELECTION_LAYER_ADD: u32 = 2;
pub const INKPOD_SELECTION_LAYER_SUBTRACT: u32 = 3;
pub const INKPOD_GUIDE_HORIZONTAL: u32 = 1;
pub const INKPOD_GUIDE_VERTICAL: u32 = 2;
const MAX_COMMAND_COUNT: u64 = 65_536;
const MAX_STROKE_SAMPLE_COUNT: u64 = 1_048_576;
const MAX_PATH_BYTES: u64 = 32_768;
const MAX_PALETTE_COLOR_COUNT: u64 = 4_096;
const MAX_SELECTION_POINT_COUNT: u64 = 1_048_576;
const MAX_NODE_NAME_BYTES: u64 = 1_024;
const ERROR_CAPACITY: usize = 512;

#[repr(C)]
pub struct InkpodCoreConfig {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
}

#[repr(C)]
pub struct InkpodCommand {
    pub struct_size: u32,
    pub kind: u32,
    pub flags: u64,
}

#[repr(C)]
pub struct InkpodCommandBatch {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub commands: *const InkpodCommand,
    pub command_count: u64,
    pub command_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodDispatchResult {
    pub struct_size: u32,
    pub reserved: u32,
    pub revision: u64,
    pub accepted_command_count: u64,
}

#[repr(C)]
pub struct InkpodCellCreateOptions {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodFrameRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodDocumentInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub document_revision: u64,
    pub view_revision: u64,
    pub document_id: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub layer_id: u64,
    pub main_plane_id: u64,
    pub color_plane_id: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub hundred_frame: InkpodFrameRect,
    pub reference_frame: InkpodFrameRect,
    pub drawing_frame: InkpodFrameRect,
    pub safe_frame: InkpodFrameRect,
    pub margin_left: u32,
    pub margin_top: u32,
    pub margin_right: u32,
    pub margin_bottom: u32,
    pub active_plane: u32,
    pub reserved: u32,
    pub main_plane_checksum: u64,
    pub color_plane_checksum: u64,
}

#[repr(C)]
pub struct InkpodPaperFramesInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub hundred_frame: InkpodFrameRect,
    pub reference_frame: InkpodFrameRect,
    pub drawing_frame: InkpodFrameRect,
    pub safe_frame: InkpodFrameRect,
    pub margin_left: u32,
    pub margin_top: u32,
    pub margin_right: u32,
    pub margin_bottom: u32,
}

#[repr(C)]
pub struct InkpodHistoryInfo {
    pub struct_size: u32,
    pub reserved: u32,
    pub cursor: u64,
    pub item_count: u64,
}

#[repr(C)]
pub struct InkpodHistoryItem {
    pub struct_size: u32,
    pub flags: u32,
    pub index: u64,
    pub name_utf8: *mut u8,
    pub name_capacity: u64,
    pub name_bytes: u64,
}

#[repr(C)]
pub struct InkpodStrokeSample {
    pub struct_size: u32,
    pub flags: u32,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub reserved: u32,
}

#[repr(C)]
pub struct InkpodStrokeInput {
    pub struct_size: u32,
    pub tool: u32,
    pub plane: u32,
    pub coordinate_space: u32,
    pub flags: u64,
    pub color_rgba: u32,
    pub diameter: f32,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodStrokeSampleSpan {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodViewInput {
    pub struct_size: u32,
    pub kind: u32,
    pub flags: u64,
    pub value1: f64,
    pub value2: f64,
    pub value3: f64,
    pub value4: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodColorValue {
    pub struct_size: u32,
    pub depth: u32,
    pub red: u16,
    pub green: u16,
    pub blue: u16,
    pub alpha: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodColorArray {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub colors: *const InkpodColorValue,
    pub color_count: u64,
    pub color_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodColorBuffer {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub colors: *mut InkpodColorValue,
    pub color_capacity: u64,
    pub color_stride_bytes: u64,
    pub color_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodFillInput {
    pub struct_size: u32,
    pub operation: u32,
    pub flags: u64,
    pub seed_x: u32,
    pub seed_y: u32,
    pub color: InkpodColorValue,
    pub tolerance: u16,
    pub gap_close: u16,
    pub inclusion_mode: u32,
    pub selection: InkpodFrameRect,
    pub inclusion_colors: *const InkpodColorValue,
    pub inclusion_color_count: u64,
    pub inclusion_color_stride_bytes: u64,
    pub extension_distance: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodFillResult {
    pub struct_size: u32,
    pub flags: u32,
    pub revision: u64,
    pub changed_pixel_count: u64,
    pub leak_x: u32,
    pub leak_y: u32,
}

#[repr(C)]
pub struct InkpodSnapshotOptions {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
}

#[repr(C)]
pub struct InkpodSnapshotTile {
    pub struct_size: u32,
    pub pixel_format: u32,
    pub tile_id: u64,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub reserved: u32,
    pub pixels: *const u8,
    pub pixel_bytes: u64,
    pub tile_revision: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodSnapshotView {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
    pub revision: u64,
    pub tiles: *const InkpodSnapshotTile,
    pub tile_count: u64,
    pub tile_stride_bytes: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodSnapshotTransform {
    pub struct_size: u32,
    pub flags: u32,
    pub view_revision: u64,
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub document_width: u32,
    pub document_height: u32,
}

#[repr(C)]
pub struct InkpodSnapshotGuide {
    pub struct_size: u32,
    pub axis: u32,
    pub position: i32,
    pub reserved: u32,
    pub id: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodSnapshotOverlay {
    pub struct_size: u32,
    pub flags: u32,
    pub grid_origin_x: i32,
    pub grid_origin_y: i32,
    pub grid_spacing_x: u32,
    pub grid_spacing_y: u32,
    pub grid_subdivisions: u32,
    pub reserved: u32,
    pub guides: *const InkpodSnapshotGuide,
    pub guide_count: u64,
    pub guide_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodVectorPoint {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorCubicSegment {
    pub struct_size: u32,
    pub reserved: u32,
    pub p0: InkpodVectorPoint,
    pub p1: InkpodVectorPoint,
    pub p2: InkpodVectorPoint,
    pub p3: InkpodVectorPoint,
    pub width_start: f32,
    pub width_end: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorPathInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub flags: u64,
    pub plane_id: u64,
    pub color: InkpodColorValue,
    pub segments: *const InkpodVectorCubicSegment,
    pub segment_count: u64,
    pub segment_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorFillInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub color: InkpodColorValue,
    pub boundary_path_ids: *const u64,
    pub boundary_path_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorEraseInput {
    pub struct_size: u32,
    pub mode: u32,
    pub plane_id: u64,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorWidthInput {
    pub struct_size: u32,
    pub mode: u32,
    pub feature_flags: u64,
    pub path_ids: *const u64,
    pub path_count: u64,
    pub parameter: f32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorSelectionInput {
    pub struct_size: u32,
    pub mode: u32,
    pub feature_flags: u64,
    pub bounds: InkpodFrameRect,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorSelectionRange {
    pub struct_size: u32,
    pub reserved: u32,
    pub path_id: u64,
    pub start_million: u32,
    pub end_million: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorSelectionBuffer {
    pub struct_size: u32,
    pub reserved: u32,
    pub ranges: *mut InkpodVectorSelectionRange,
    pub range_capacity: u64,
    pub range_count: u64,
    pub fill_ids: *mut u64,
    pub fill_capacity: u64,
    pub fill_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorRasterizeInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub layer_id: u64,
    pub scale: u32,
    pub reserved_2: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodVectorRasterBuffer {
    pub struct_size: u32,
    pub reserved: u32,
    pub pixels: *mut u8,
    pub pixel_capacity: u64,
    pub required_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub reserved_2: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodRasterVectorizeInput {
    pub struct_size: u32,
    pub alpha_threshold: u32,
    pub feature_flags: u64,
    pub source_plane_id: u64,
    pub target_layer_id: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodCurvePoint {
    pub struct_size: u32,
    pub reserved: u32,
    pub input: u32,
    pub output: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodFilterInput {
    pub struct_size: u32,
    pub kind: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub channel: u32,
    pub interpolation: u32,
    pub parameter_0: i32,
    pub parameter_1: i32,
    pub parameter_2: i32,
    pub parameter_3: i32,
    pub parameter_4: i32,
    pub point_stride_bytes: u32,
    pub points: *const InkpodCurvePoint,
    pub point_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodFilterPreviewInfo {
    pub struct_size: u32,
    pub reserved: u32,
    pub plane_id: u64,
    pub base_checksum: u64,
    pub preview_checksum: u64,
    pub preview_revision: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodGradientStop {
    pub struct_size: u32,
    pub reserved: u32,
    pub position_milli: u32,
    pub reserved_2: u32,
    pub color: InkpodColorValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodGradientInput {
    pub struct_size: u32,
    pub kind: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub mode: u32,
    pub dither: u32,
    pub start_x_milli: i64,
    pub start_y_milli: i64,
    pub end_x_milli: i64,
    pub end_y_milli: i64,
    pub stops: *const InkpodGradientStop,
    pub stop_count: u64,
    pub stop_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodAirbrushInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub center_x_milli: i64,
    pub center_y_milli: i64,
    pub radius_milli: u32,
    pub hardness_milli: u32,
    pub opacity_milli: u32,
    pub reserved_2: u32,
    pub color: InkpodColorValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodBoundaryAirbrushInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub width: u32,
    pub strength_milli: u32,
    pub colors: InkpodColorArray,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodBlurEffectInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub radius: u32,
    pub strength_milli: u32,
    pub reserved_2: u32,
    pub reserved_3: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodStampInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub source_x: i32,
    pub source_y: i32,
    pub destination_x: i32,
    pub destination_y: i32,
    pub width: u32,
    pub height: u32,
    pub opacity_milli: u32,
    pub reserved_2: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodAlphaEditInput {
    pub struct_size: u32,
    pub pixel_format: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub width: u32,
    pub height: u32,
    pub reserved: u32,
    pub reserved_2: u32,
    pub pixels: *const u8,
    pub pixel_bytes: u64,
    pub row_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodAirbrushGestureInput {
    pub struct_size: u32,
    pub coordinate_space: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub view_id: u64,
    pub radius_milli: u32,
    pub hardness_milli: u32,
    pub spacing_milli: u32,
    pub opacity_milli: u32,
    pub fade_milli: u32,
    pub continuous_dabs: u32,
    pub color: InkpodColorValue,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodStampGestureInput {
    pub struct_size: u32,
    pub coordinate_space: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub view_id: u64,
    pub source: InkpodStrokeSample,
    pub radius_milli: u32,
    pub hardness_milli: u32,
    pub spacing_milli: u32,
    pub opacity_milli: u32,
    pub shape: u32,
    pub reserved: u32,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodBlurToolInput {
    pub struct_size: u32,
    pub coordinate_space: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub view_id: u64,
    pub radius: u32,
    pub strength_milli: u32,
    pub shape: u32,
    pub diameter: f32,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodDustInput {
    pub struct_size: u32,
    pub mode: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub view_id: u64,
    pub coordinate_space: u32,
    pub shape: u32,
    pub maximum_pixels: u32,
    pub use_region: u32,
    pub diameter: f32,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodM6TaskInfo {
    pub struct_size: u32,
    pub state: u32,
    pub completed_work: u64,
    pub total_work: u64,
    pub reserved: u64,
}

#[repr(C)]
pub struct InkpodSnapshotVectorSegment {
    pub struct_size: u32,
    pub flags: u32,
    pub path_id: u64,
    pub plane_id: u64,
    pub z_order: u32,
    pub segment_index: u32,
    pub segment_count: u32,
    pub color_rgba: u32,
    pub p0: InkpodVectorPoint,
    pub p1: InkpodVectorPoint,
    pub p2: InkpodVectorPoint,
    pub p3: InkpodVectorPoint,
    pub width_start: f32,
    pub width_end: f32,
}

#[repr(C)]
pub struct InkpodSnapshotVectorFill {
    pub struct_size: u32,
    pub reserved: u32,
    pub fill_id: u64,
    pub plane_id: u64,
    pub z_order: u32,
    pub color_rgba: u32,
    pub first_boundary_path: u64,
    pub boundary_path_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodSnapshotVectorView {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
    pub segments: *const InkpodSnapshotVectorSegment,
    pub segment_count: u64,
    pub segment_stride_bytes: u64,
    pub fills: *const InkpodSnapshotVectorFill,
    pub fill_count: u64,
    pub fill_stride_bytes: u64,
    pub boundary_path_ids: *const u64,
    pub boundary_path_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodTreeEdit {
    pub struct_size: u32,
    pub operation: u32,
    pub flags: u64,
    pub object_id: u64,
    pub parent_id: u64,
    pub destination_index: u32,
    pub kind: u32,
    pub pixel_format: u32,
    pub opacity_milli: u32,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodNodeInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub id: u64,
    pub parent_id: u64,
    pub kind: u32,
    pub pixel_format: u32,
    pub opacity_milli: u32,
    pub index: u32,
    pub child_count: u32,
    pub reserved: u32,
    pub name_utf8: *mut u8,
    pub name_capacity: u64,
    pub name_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodSelectionPoint {
    pub struct_size: u32,
    pub reserved: u32,
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodSelectionInput {
    pub struct_size: u32,
    pub shape: u32,
    pub operation: u32,
    pub reserved: u32,
    pub bounds: InkpodFrameRect,
    pub points: *const InkpodSelectionPoint,
    pub point_count: u64,
    pub point_stride_bytes: u64,
    pub diameter: f32,
    pub tolerance: u16,
    pub gap_close: u16,
    pub seed_x: u32,
    pub seed_y: u32,
}

#[repr(C)]
pub struct InkpodFloatingTransform {
    pub struct_size: u32,
    pub reserved: u32,
    pub translate_x: f64,
    pub translate_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation_degrees: f64,
}

#[repr(C)]
pub struct InkpodDocumentResizeInput {
    pub struct_size: u32,
    pub anchor: u32,
    pub flags: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
}

#[repr(C)]
pub struct InkpodClipboardRasterBuffer {
    pub struct_size: u32,
    pub reserved: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub pixels_rgba8: *mut u8,
    pub pixel_capacity: u64,
    pub required_bytes: u64,
    pub row_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodClipboardRgbaInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub pixels_rgba8: *const u8,
    pub pixel_bytes: u64,
    pub row_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodGridInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub spacing_x: u32,
    pub spacing_y: u32,
    pub subdivisions: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodLocatorOutput {
    pub struct_size: u32,
    pub flags: u32,
    pub document_x: i32,
    pub document_y: i32,
    pub selection: InkpodFrameRect,
    pub color: InkpodColorValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodM4RasterInput {
    pub struct_size: u32,
    pub pixel_format: u32,
    pub flags: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub source_revision: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub reference_frame: InkpodFrameRect,
    pub pixels: *const u8,
    pub pixel_bytes: u64,
    pub row_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodLightTableItemInput {
    pub struct_size: u32,
    pub flags: u32,
    pub opacity_milli: u32,
    pub display_mode: u32,
    pub display_color: InkpodColorValue,
    pub translate_x_milli: i32,
    pub translate_y_milli: i32,
    pub scale_x_milli: u32,
    pub scale_y_milli: u32,
    pub rotation_milli_degrees: i32,
    pub reserved: u32,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
    pub source: InkpodM4RasterInput,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodLightTableEdit {
    pub struct_size: u32,
    pub operation: u32,
    pub object_id: u64,
    pub destination_index: u32,
    pub flags: u32,
    pub opacity_milli: u32,
    pub display_mode: u32,
    pub display_color: InkpodColorValue,
    pub translate_x_milli: i32,
    pub translate_y_milli: i32,
    pub scale_x_milli: u32,
    pub scale_y_milli: u32,
    pub rotation_milli_degrees: i32,
    pub reserved: u32,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodLightTableSetInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub id: u64,
    pub opacity_milli: u32,
    pub item_count: u32,
    pub name_utf8: *mut u8,
    pub name_capacity: u64,
    pub name_bytes: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodLightTableItemInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub id: u64,
    pub source_plane_id: u64,
    pub source_document_uuid_high: u64,
    pub source_document_uuid_low: u64,
    pub source_revision: u64,
    pub opacity_milli: u32,
    pub effective_opacity_milli: u32,
    pub display_mode: u32,
    pub display_color: InkpodColorValue,
    pub translate_x_milli: i32,
    pub translate_y_milli: i32,
    pub scale_x_milli: u32,
    pub scale_y_milli: u32,
    pub rotation_milli_degrees: i32,
    pub reserved: u32,
    pub name_utf8: *mut u8,
    pub name_capacity: u64,
    pub name_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodSequenceCellInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
    pub source: InkpodM4RasterInput,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodSequenceInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub cells: *const InkpodSequenceCellInput,
    pub cell_count: u64,
    pub cell_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodNamedBytesInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
    pub bytes: *const u8,
    pub byte_count: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodSequenceCellInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub sequence_index: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub cell_number: u32,
    pub width: u32,
    pub height: u32,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
    pub reserved: u32,
    pub thumbnail_checksum: u64,
    pub name_utf8: *mut u8,
    pub name_capacity: u64,
    pub name_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodMotionCheckInput {
    pub struct_size: u32,
    pub fps: u32,
    pub flags: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodMotionFrame {
    pub struct_size: u32,
    pub flags: u32,
    pub sequence_index: u64,
    pub cell_number: u32,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
    pub reserved: u32,
    pub thumbnail_checksum: u64,
}

pub struct InkpodCore {
    owner_thread: ThreadId,
    core: Core,
}

pub struct InkpodSnapshot {
    snapshot: RenderSnapshot,
    tiles: Box<[InkpodSnapshotTile]>,
    guides: Box<[InkpodSnapshotGuide]>,
    vector_segments: Box<[InkpodSnapshotVectorSegment]>,
    vector_fills: Box<[InkpodSnapshotVectorFill]>,
    vector_boundary_path_ids: Box<[u64]>,
}

pub struct InkpodClipboard {
    payload: ClipboardPayload,
}

pub struct InkpodByteBuffer {
    bytes: Box<[u8]>,
}

struct EncodedSequenceFile {
    name: Box<[u8]>,
    bytes: Box<[u8]>,
}

pub struct InkpodEncodedSequence {
    files: Vec<EncodedSequenceFile>,
}

pub struct InkpodM6Task {
    state: AtomicU32,
    cancelled: AtomicBool,
    completed_work: AtomicU64,
    total_work: AtomicU64,
}

impl InkpodM6Task {
    fn new() -> Self {
        Self {
            state: AtomicU32::new(INKPOD_M6_TASK_READY),
            cancelled: AtomicBool::new(false),
            completed_work: AtomicU64::new(0),
            total_work: AtomicU64::new(0),
        }
    }

    fn begin(&self) -> bool {
        if self
            .state
            .compare_exchange(
                INKPOD_M6_TASK_READY,
                INKPOD_M6_TASK_RUNNING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            return true;
        }
        self.state
            .compare_exchange(
                INKPOD_M6_TASK_CANCELLED,
                INKPOD_M6_TASK_RUNNING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn progress(&self, completed: u64, total: u64) -> bool {
        self.total_work.store(total, Ordering::Release);
        self.completed_work
            .store(completed.min(total), Ordering::Release);
        !self.cancelled.load(Ordering::Acquire)
    }

    fn finish(&self, status: u32) {
        let state = match status {
            INKPOD_STATUS_OK => INKPOD_M6_TASK_COMPLETED,
            INKPOD_STATUS_CANCELLED => INKPOD_M6_TASK_CANCELLED,
            _ => INKPOD_M6_TASK_FAILED,
        };
        self.state.store(state, Ordering::Release);
    }
}

fn snapshot_handle(snapshot: RenderSnapshot) -> Box<InkpodSnapshot> {
    let tiles: Box<[InkpodSnapshotTile]> = snapshot
        .tiles()
        .iter()
        .map(|tile| InkpodSnapshotTile {
            struct_size: size_of::<InkpodSnapshotTile>() as u32,
            pixel_format: INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8,
            tile_id: tile.tile_id(),
            origin_x: tile.origin_x(),
            origin_y: tile.origin_y(),
            width: tile.width(),
            height: tile.height(),
            stride_bytes: tile.stride_bytes(),
            reserved: 0,
            pixels: tile.pixels().as_ptr(),
            pixel_bytes: tile.pixels().len() as u64,
            tile_revision: tile.tile_revision(),
        })
        .collect();
    let guides = snapshot
        .guides()
        .iter()
        .map(|guide| InkpodSnapshotGuide {
            struct_size: size_of::<InkpodSnapshotGuide>() as u32,
            axis: match guide.axis {
                GuideAxis::Horizontal => INKPOD_GUIDE_HORIZONTAL,
                GuideAxis::Vertical => INKPOD_GUIDE_VERTICAL,
            },
            position: guide.position,
            reserved: 0,
            id: guide.id,
        })
        .collect();
    let vector_segments = snapshot
        .vector_segments()
        .iter()
        .map(|segment| InkpodSnapshotVectorSegment {
            struct_size: size_of::<InkpodSnapshotVectorSegment>() as u32,
            flags: (if segment.closed {
                INKPOD_SNAPSHOT_VECTOR_CLOSED
            } else {
                0
            }) | if segment.stroke_visible {
                INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE
            } else {
                0
            },
            path_id: segment.path_id,
            plane_id: segment.plane_id,
            z_order: segment.z_order,
            segment_index: segment.segment_index,
            segment_count: segment.segment_count,
            color_rgba: pack_rgba(segment.color_rgba),
            p0: vector_point(segment.cubic.p0),
            p1: vector_point(segment.cubic.p1),
            p2: vector_point(segment.cubic.p2),
            p3: vector_point(segment.cubic.p3),
            width_start: segment.cubic.width_start,
            width_end: segment.cubic.width_end,
        })
        .collect();
    let vector_boundary_path_ids: Box<[u64]> = snapshot
        .vector_fills()
        .iter()
        .flat_map(|fill| fill.boundary_path_ids.iter().copied())
        .collect();
    let mut first_boundary_path = 0_u64;
    let vector_fills = snapshot
        .vector_fills()
        .iter()
        .map(|fill| {
            let output = InkpodSnapshotVectorFill {
                struct_size: size_of::<InkpodSnapshotVectorFill>() as u32,
                reserved: 0,
                fill_id: fill.fill_id,
                plane_id: fill.plane_id,
                z_order: fill.z_order,
                color_rgba: pack_rgba(fill.color_rgba),
                first_boundary_path,
                boundary_path_count: fill.boundary_path_ids.len() as u64,
            };
            first_boundary_path += fill.boundary_path_ids.len() as u64;
            output
        })
        .collect();
    Box::new(InkpodSnapshot {
        snapshot,
        tiles,
        guides,
        vector_segments,
        vector_fills,
        vector_boundary_path_ids,
    })
}

// SAFETY: Every raw pointer in `tiles` borrows an immutable pixel allocation
// owned by `snapshot`. Both fields remain immovable inside the Box returned over
// the ABI, and callers externally synchronize view/release as documented.
unsafe impl Send for InkpodSnapshot {}
// SAFETY: The same immutable ownership invariant permits concurrent reads; no
// function mutates a published snapshot.
unsafe impl Sync for InkpodSnapshot {}

struct ErrorSlot {
    bytes: [u8; ERROR_CAPACITY],
    len: usize,
}

impl ErrorSlot {
    const fn new() -> Self {
        Self {
            bytes: [0; ERROR_CAPACITY],
            len: 0,
        }
    }

    fn set(&mut self, message: &str) {
        let mut length = message.len().min(ERROR_CAPACITY - 1);
        while !message.is_char_boundary(length) {
            length -= 1;
        }
        self.bytes[..length].copy_from_slice(&message.as_bytes()[..length]);
        self.len = length;
        self.bytes[length] = 0;
    }

    fn clear(&mut self) {
        self.len = 0;
        self.bytes[0] = 0;
    }
}

thread_local! {
    static LAST_ERROR: RefCell<ErrorSlot> = const { RefCell::new(ErrorSlot::new()) };
}

fn clear_last_error() {
    LAST_ERROR.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            slot.clear();
        }
    });
}

fn fail(status: u32, message: &str) -> u32 {
    LAST_ERROR.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            slot.set(message);
        }
    });
    status
}

fn ffi_boundary(operation: impl FnOnce() -> u32) -> u32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(status) => status,
        Err(_) => fail(
            INKPOD_STATUS_PANIC,
            "a panic was contained at the inkpod C ABI boundary",
        ),
    }
}

fn is_aligned<T>(pointer: *const T) -> bool {
    (pointer as usize) % align_of::<T>() == 0
}

// SAFETY: `pointer` must expose a readable u32 size prefix. When that prefix
// advertises `size_of::<T>()` or more, the caller must provide that full range.
unsafe fn validate_struct<T>(pointer: *const T, type_name: &str) -> Result<u32, u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{type_name} is null or misaligned"),
        ));
    }

    // SAFETY: Exported-function contracts require every public structure
    // pointer to expose at least its readable u32 size prefix. Reading only the
    // prefix avoids creating a full T reference before its size is validated.
    let struct_size = unsafe { pointer.cast::<u32>().read() };
    if struct_size < size_of::<T>() as u32 {
        return Err(fail(
            INKPOD_STATUS_INCOMPATIBLE_ABI,
            &format!("{type_name}.struct_size is too small"),
        ));
    }
    Ok(struct_size)
}

fn assert_snapshot_thread_contract() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<InkpodSnapshot>();
}

fn validate_core_thread(core: &InkpodCore) -> u32 {
    if core.owner_thread == thread::current().id() {
        INKPOD_STATUS_OK
    } else {
        fail(
            INKPOD_STATUS_WRONG_THREAD,
            "InkpodCore must be used and destroyed on its creating thread",
        )
    }
}

fn map_core_error(error: CoreError) -> u32 {
    let status = match error {
        CoreError::NoDocument => INKPOD_STATUS_NO_DOCUMENT,
        CoreError::InvalidArgument(_) | CoreError::Raster(_) | CoreError::Fill(_) => {
            INKPOD_STATUS_INVALID_ARGUMENT
        }
        CoreError::FillOverflow { .. } => INKPOD_STATUS_FILL_OVERFLOW,
        CoreError::Cancelled => INKPOD_STATUS_CANCELLED,
        CoreError::UnsavedChanges => INKPOD_STATUS_UNSAVED_CHANGES,
        CoreError::InvalidState(_) => INKPOD_STATUS_INVALID_STATE,
        CoreError::Format(_) => INKPOD_STATUS_IO_ERROR,
    };
    fail(status, &error.to_string())
}

fn frame_rect(rect: inkpod_core::RectI32) -> InkpodFrameRect {
    InkpodFrameRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

const fn pack_rgba(color: [u8; 4]) -> u32 {
    ((color[0] as u32) << 24)
        | ((color[1] as u32) << 16)
        | ((color[2] as u32) << 8)
        | color[3] as u32
}

const fn vector_point(point: PointF32) -> InkpodVectorPoint {
    InkpodVectorPoint {
        x: point.x,
        y: point.y,
    }
}

fn write_document_info(output: &mut InkpodDocumentInfo, info: DocumentInfo) {
    output.flags = (if info.dirty {
        INKPOD_DOCUMENT_FLAG_DIRTY
    } else {
        0
    }) | (if info.can_undo {
        INKPOD_DOCUMENT_FLAG_CAN_UNDO
    } else {
        0
    }) | (if info.can_redo {
        INKPOD_DOCUMENT_FLAG_CAN_REDO
    } else {
        0
    }) | (if info.recovered {
        INKPOD_DOCUMENT_FLAG_RECOVERED
    } else {
        0
    });
    output.document_revision = info.document_revision;
    output.view_revision = info.view_revision;
    output.document_id = info.document_id;
    output.document_uuid_high = (info.document_uuid >> 64) as u64;
    output.document_uuid_low = info.document_uuid as u64;
    output.layer_id = info.layer_id;
    output.main_plane_id = info.main_plane_id;
    output.color_plane_id = info.color_plane_id;
    output.width = info.width;
    output.height = info.height;
    output.dpi_x_milli = info.dpi_x_milli;
    output.dpi_y_milli = info.dpi_y_milli;
    output.hundred_frame = frame_rect(info.frames.hundred_frame);
    output.reference_frame = frame_rect(info.frames.reference_frame);
    output.drawing_frame = frame_rect(info.frames.drawing_frame);
    output.safe_frame = frame_rect(info.frames.safe_frame);
    output.margin_left = info.frames.margins.left;
    output.margin_top = info.frames.margins.top;
    output.margin_right = info.frames.margins.right;
    output.margin_bottom = info.frames.margins.bottom;
    output.active_plane = match info.active_plane {
        ActivePlane::MainLine => INKPOD_PLANE_MAIN_LINE,
        ActivePlane::Color => INKPOD_PLANE_COLOR,
    };
    output.reserved = 0;
    output.main_plane_checksum = info.main_plane_checksum;
    output.color_plane_checksum = info.color_plane_checksum;
}

fn write_dispatch_result(result: &mut InkpodDispatchResult, outcome: inkpod_core::DispatchOutcome) {
    result.reserved = 0;
    result.revision = outcome.revision();
    result.accepted_command_count = outcome.accepted_commands();
}

fn parse_plane(value: u32) -> Result<ActivePlane, u32> {
    match value {
        INKPOD_PLANE_MAIN_LINE => Ok(ActivePlane::MainLine),
        INKPOD_PLANE_COLOR => Ok(ActivePlane::Color),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "plane is not a defined M1 plane",
        )),
    }
}

fn parse_filter_channel(value: u32) -> Result<Channel, u32> {
    match value {
        INKPOD_FILTER_CHANNEL_RGB => Ok(Channel::Rgb),
        INKPOD_FILTER_CHANNEL_RED => Ok(Channel::Red),
        INKPOD_FILTER_CHANNEL_GREEN => Ok(Channel::Green),
        INKPOD_FILTER_CHANNEL_BLUE => Ok(Channel::Blue),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "filter channel is unknown",
        )),
    }
}

fn parse_curve_interpolation(value: u32) -> Result<CurveInterpolation, u32> {
    match value {
        INKPOD_CURVE_BEZIER => Ok(CurveInterpolation::Bezier),
        INKPOD_CURVE_BSPLINE => Ok(CurveInterpolation::BSpline),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "curve interpolation is unknown",
        )),
    }
}

unsafe fn parse_filter_input(input: &InkpodFilterInput) -> Result<Filter, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "filter input contains unsupported feature flags",
        ));
    }
    let points = if input.point_count == 0 {
        if !input.points.is_null() || input.point_stride_bytes != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "zero curve point count requires a null pointer and zero stride",
            ));
        }
        Vec::new()
    } else {
        if input.points.is_null() || !is_aligned(input.points) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "curve point storage is null or misaligned",
            ));
        }
        let count = usize::try_from(input.point_count).map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "curve point count is not representable",
            )
        })?;
        let stride = if input.point_stride_bytes == 0 {
            size_of::<InkpodCurvePoint>()
        } else {
            input.point_stride_bytes as usize
        };
        let storage = count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodCurvePoint>()));
        if input.point_count > inkpod_core::MAX_CURVE_POINTS as u64
            || stride < size_of::<InkpodCurvePoint>()
            || stride % align_of::<InkpodCurvePoint>() != 0
            || storage.is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "curve point count, stride, or storage size is invalid",
            ));
        }
        let mut points = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: The checked count/stride span is readable by contract.
            let pointer = unsafe {
                input
                    .points
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodCurvePoint>()
            };
            // SAFETY: Every strided record exposes a readable size prefix.
            let struct_size = unsafe { validate_struct(pointer, "InkpodCurvePoint") }?;
            if u64::from(struct_size) > stride as u64 {
                return Err(fail(
                    INKPOD_STATUS_INCOMPATIBLE_ABI,
                    "InkpodCurvePoint.struct_size exceeds point stride",
                ));
            }
            // SAFETY: The complete known record is readable after validation.
            let record = unsafe { &*pointer };
            if record.reserved != 0
                || record.input > u16::MAX.into()
                || record.output > u16::MAX.into()
            {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "curve point record is invalid",
                ));
            }
            points.push(CurvePoint {
                input: record.input as u16,
                output: record.output as u16,
            });
        }
        points
    };
    let no_points = || {
        if points.is_empty() {
            Ok(())
        } else {
            Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "this filter does not accept curve points",
            ))
        }
    };
    match input.kind {
        INKPOD_FILTER_SHARPEN_WEAK => {
            no_points()?;
            Ok(Filter::SharpenWeak)
        }
        INKPOD_FILTER_SHARPEN_STRONG => {
            no_points()?;
            Ok(Filter::SharpenStrong)
        }
        INKPOD_FILTER_BLUR_WEAK => {
            no_points()?;
            Ok(Filter::BlurWeak)
        }
        INKPOD_FILTER_BLUR_STRONG => {
            no_points()?;
            Ok(Filter::BlurStrong)
        }
        INKPOD_FILTER_GAUSSIAN_BLUR => {
            no_points()?;
            Ok(Filter::GaussianBlur {
                radius: input.parameter_0.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "Gaussian radius is negative",
                    )
                })?,
                strength_milli: input.parameter_1.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "Gaussian strength is negative",
                    )
                })?,
            })
        }
        INKPOD_FILTER_INVERT => {
            no_points()?;
            Ok(Filter::Invert {
                channel: parse_filter_channel(input.channel)?,
            })
        }
        INKPOD_FILTER_AUTO_CONTRAST => {
            no_points()?;
            Ok(Filter::AutoContrast)
        }
        INKPOD_FILTER_BRIGHTNESS_CONTRAST => {
            no_points()?;
            Ok(Filter::BrightnessContrast {
                brightness_milli: input.parameter_0,
                contrast_milli: input.parameter_1,
            })
        }
        INKPOD_FILTER_TONE_CURVE => Ok(Filter::ToneCurve {
            channel: parse_filter_channel(input.channel)?,
            interpolation: parse_curve_interpolation(input.interpolation)?,
            points,
        }),
        INKPOD_FILTER_LEVELS => {
            no_points()?;
            Ok(Filter::Levels(Levels {
                channel: parse_filter_channel(input.channel)?,
                input_shadow: input.parameter_0.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "levels input shadow is invalid",
                    )
                })?,
                input_gamma_milli: input
                    .parameter_1
                    .try_into()
                    .map_err(|_| fail(INKPOD_STATUS_INVALID_ARGUMENT, "levels gamma is invalid"))?,
                input_highlight: input.parameter_2.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "levels input highlight is invalid",
                    )
                })?,
                output_shadow: input.parameter_3.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "levels output shadow is invalid",
                    )
                })?,
                output_highlight: input.parameter_4.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "levels output highlight is invalid",
                    )
                })?,
            }))
        }
        INKPOD_FILTER_HSV => {
            no_points()?;
            Ok(Filter::Hsv(HsvAdjustment {
                hue_degrees_milli: input.parameter_0,
                saturation_milli: input.parameter_1,
                value_milli: input.parameter_2,
            }))
        }
        INKPOD_FILTER_COLOR_BALANCE => {
            no_points()?;
            Ok(Filter::ColorBalance(ColorBalance {
                red_milli: input.parameter_0,
                green_milli: input.parameter_1,
                blue_milli: input.parameter_2,
            }))
        }
        INKPOD_FILTER_UNSHARP_MASK => {
            no_points()?;
            Ok(Filter::UnsharpMask {
                radius: input.parameter_0.try_into().map_err(|_| {
                    fail(INKPOD_STATUS_INVALID_ARGUMENT, "unsharp radius is negative")
                })?,
                amount_milli: input.parameter_1.try_into().map_err(|_| {
                    fail(INKPOD_STATUS_INVALID_ARGUMENT, "unsharp amount is negative")
                })?,
                threshold: input.parameter_2.try_into().map_err(|_| {
                    fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "unsharp threshold is invalid",
                    )
                })?,
            })
        }
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "filter kind is unknown",
        )),
    }
}

fn filter_to_adjustment(filter: Filter) -> Result<Adjustment, u32> {
    match filter {
        Filter::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        } => Ok(Adjustment::BrightnessContrast {
            brightness_milli,
            contrast_milli,
        }),
        Filter::ToneCurve {
            channel,
            interpolation,
            points,
        } => Ok(Adjustment::ToneCurve {
            channel,
            interpolation,
            points,
        }),
        Filter::Levels(levels) => Ok(Adjustment::Levels(levels)),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "adjustment layers accept brightness/contrast, tone curve, or levels",
        )),
    }
}

fn write_filter_preview_info(
    output: &mut InkpodFilterPreviewInfo,
    info: inkpod_core::FilterPreviewInfo,
) {
    output.reserved = 0;
    output.plane_id = info.plane_id;
    output.base_checksum = info.base_checksum;
    output.preview_checksum = info.preview_checksum;
    output.preview_revision = info.preview_revision;
}

// SAFETY: `input` and every advertised strided stop record must remain readable
// for this call. All retained stop/color values are copied into the result.
unsafe fn parse_gradient_input(input: &InkpodGradientInput) -> Result<Gradient, u32> {
    if input.feature_flags & !INKPOD_GRADIENT_FLAG_CONSTRAIN_45 != 0 || input.dither > 1 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "gradient input contains unsupported flags or dither value",
        ));
    }
    let kind = match input.kind {
        INKPOD_GRADIENT_LINEAR => GradientKind::Linear,
        INKPOD_GRADIENT_RADIAL => GradientKind::Radial,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "gradient kind is unknown",
            ));
        }
    };
    let mode = match input.mode {
        INKPOD_GRADIENT_COMPOSITE => GradientMode::Composite,
        INKPOD_GRADIENT_OVERWRITE => GradientMode::Overwrite,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "gradient mode is unknown",
            ));
        }
    };
    let count = usize::try_from(input.stop_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "gradient stop count is not representable",
        )
    })?;
    let stride = usize::try_from(input.stop_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "gradient stop stride is not representable",
        )
    })?;
    let storage = count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodGradientStop>()));
    if !(3..=MAX_GRADIENT_STOPS).contains(&count)
        || input.stops.is_null()
        || !is_aligned(input.stops)
        || stride < size_of::<InkpodGradientStop>()
        || stride % align_of::<InkpodGradientStop>() != 0
        || storage.is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "gradient stop count, pointer, stride, or storage is invalid",
        ));
    }
    let mut stops = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The checked count/stride span is readable by contract.
        let pointer = unsafe {
            input
                .stops
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodGradientStop>()
        };
        // SAFETY: Every strided record exposes a readable size prefix.
        let struct_size = unsafe { validate_struct(pointer, "InkpodGradientStop") }?;
        if u64::from(struct_size) > input.stop_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodGradientStop.struct_size exceeds stop stride",
            ));
        }
        // SAFETY: The complete known record is readable after validation.
        let record = unsafe { &*pointer };
        if record.reserved != 0 || record.reserved_2 != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "gradient stop contains unsupported reserved values",
            ));
        }
        // SAFETY: The nested complete color record is part of the validated stop.
        let color = unsafe { parse_color_value(&record.color) }?
            .rgba16()
            .ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "gradient stop color must be RGBA8 or RGBA16",
                )
            })?;
        stops.push(GradientStop {
            position_milli: record.position_milli,
            color,
        });
    }
    let (end_x_milli, end_y_milli) = if input.feature_flags & INKPOD_GRADIENT_FLAG_CONSTRAIN_45 != 0
    {
        let dx = input.end_x_milli as f64 - input.start_x_milli as f64;
        let dy = input.end_y_milli as f64 - input.start_y_milli as f64;
        let length = dx.hypot(dy);
        let angle =
            (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
        let end_x = input.start_x_milli as f64 + length * angle.cos();
        let end_y = input.start_y_milli as f64 + length * angle.sin();
        if !(i64::MIN as f64..=i64::MAX as f64).contains(&end_x)
            || !(i64::MIN as f64..=i64::MAX as f64).contains(&end_y)
        {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "constrained gradient endpoint is outside bounds",
            ));
        }
        (end_x.round() as i64, end_y.round() as i64)
    } else {
        (input.end_x_milli, input.end_y_milli)
    };
    Ok(Gradient {
        kind,
        mode,
        start_x_milli: input.start_x_milli,
        start_y_milli: input.start_y_milli,
        end_x_milli,
        end_y_milli,
        dither: input.dither != 0,
        stops,
    })
}

// SAFETY: `input` and its borrowed nested color record are complete and readable.
unsafe fn parse_airbrush_input(input: &InkpodAirbrushInput) -> Result<AirbrushStroke, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 || input.reserved_2 != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "airbrush input contains unsupported flags or reserved values",
        ));
    }
    // SAFETY: The nested complete color record is part of the validated input.
    let color = unsafe { parse_color_value(&input.color) }?
        .rgba16()
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "airbrush color must be RGBA8 or RGBA16",
            )
        })?;
    Ok(AirbrushStroke {
        center_x_milli: input.center_x_milli,
        center_y_milli: input.center_y_milli,
        radius_milli: input.radius_milli,
        hardness_milli: input.hardness_milli,
        opacity_milli: input.opacity_milli,
        color,
    })
}

// SAFETY: `input` and every nested strided color record remain readable for
// this call. The returned colors own their copied values.
unsafe fn parse_boundary_airbrush_input(
    input: &InkpodBoundaryAirbrushInput,
) -> Result<BoundaryAirbrush, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "boundary-airbrush input contains unsupported flags or reserved values",
        ));
    }
    // SAFETY: The nested array exposes its complete size prefix inside input.
    unsafe { validate_struct(&input.colors, "InkpodColorArray") }?;
    // SAFETY: The nested array and all advertised records are readable by contract.
    let colors = unsafe { parse_color_array(&input.colors) }?;
    if !(2..=MAX_GRADIENT_STOPS).contains(&colors.len()) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "boundary-airbrush color count is outside bounds",
        ));
    }
    let colors = colors
        .into_iter()
        .map(|color| {
            color.rgba16().ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "boundary-airbrush colors must be RGBA8 or RGBA16",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BoundaryAirbrush {
        colors,
        width: input.width,
        strength_milli: input.strength_milli,
    })
}

// SAFETY: `input.pixels` advertises readable padded rows for this call. The
// returned sparse raster owns copied grayscale pixels.
unsafe fn parse_alpha_edit_input(input: &InkpodAlphaEditInput) -> Result<TileRaster, u32> {
    if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 || input.reserved_2 != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "alpha-edit input contains unsupported flags or reserved values",
        ));
    }
    let format = match input.pixel_format {
        INKPOD_STORAGE_GRAYSCALE8 => PixelFormat::Grayscale8,
        INKPOD_STORAGE_GRAYSCALE16 => PixelFormat::Grayscale16,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "alpha-edit storage must be grayscale8 or grayscale16",
            ));
        }
    };
    let pixels = u64::from(input.width)
        .checked_mul(u64::from(input.height))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "alpha-edit dimensions overflow",
            )
        })?;
    if input.width == 0
        || input.height == 0
        || input.width > MAX_RASTER_DIMENSION
        || input.height > MAX_RASTER_DIMENSION
        || pixels > MAX_IMAGE_EDIT_PIXELS
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "alpha-edit dimensions are outside bounds",
        ));
    }
    let bytes_per_pixel = format.bytes_per_pixel();
    let row_bytes = usize::try_from(input.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "alpha-edit row size overflows",
            )
        })?;
    let stride = usize::try_from(input.row_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "alpha-edit row stride is not representable",
        )
    })?;
    let height = input.height as usize;
    let required = height
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(row_bytes))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "alpha-edit byte range overflows",
            )
        })?;
    if input.pixels.is_null()
        || stride < row_bytes
        || required > isize::MAX as usize
        || required > MAX_COMMON_RASTER_BYTES
        || input.pixel_bytes < required as u64
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "alpha-edit pointer, stride, or byte length is invalid",
        ));
    }
    // SAFETY: `required` covers the final readable byte of every padded row.
    let bytes = unsafe { slice::from_raw_parts(input.pixels, required) };
    let mut raster = TileRaster::new(input.width, input.height, format)
        .map_err(|error| map_core_error(error.into()))?;
    for y in 0..height {
        for x in 0..input.width as usize {
            let offset = y * stride + x * bytes_per_pixel;
            let value = match format {
                PixelFormat::Grayscale8 => PixelValue::Grayscale8(bytes[offset]),
                PixelFormat::Grayscale16 => {
                    PixelValue::Grayscale16(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
                }
                _ => unreachable!("validated grayscale format"),
            };
            raster
                .set_pixel(x as u32, y as u32, value, 0)
                .map_err(|error| map_core_error(error.into()))?;
        }
    }
    Ok(raster)
}

fn parse_layer_kind(value: u32) -> Result<LayerKind, u32> {
    match value {
        INKPOD_LAYER_BINARY_COLORING => Ok(LayerKind::BinaryColoring),
        INKPOD_LAYER_GRAYSCALE_COLORING => Ok(LayerKind::GrayscaleColoring),
        INKPOD_LAYER_RASTER => Ok(LayerKind::Raster),
        INKPOD_LAYER_SELECTION => Ok(LayerKind::Selection),
        INKPOD_LAYER_FRAME => Ok(LayerKind::Frame),
        INKPOD_LAYER_VANISHING_POINT => Ok(LayerKind::VanishingPoint),
        INKPOD_LAYER_ADJUSTMENT => Ok(LayerKind::Adjustment),
        INKPOD_LAYER_TEXT => Ok(LayerKind::Text),
        INKPOD_LAYER_ANNOTATION => Ok(LayerKind::Annotation),
        INKPOD_LAYER_VECTOR_COLORING => Ok(LayerKind::VectorColoring),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "layer kind is not defined",
        )),
    }
}

fn layer_kind_code(value: LayerKind) -> u32 {
    match value {
        LayerKind::BinaryColoring => INKPOD_LAYER_BINARY_COLORING,
        LayerKind::GrayscaleColoring => INKPOD_LAYER_GRAYSCALE_COLORING,
        LayerKind::Raster => INKPOD_LAYER_RASTER,
        LayerKind::Selection => INKPOD_LAYER_SELECTION,
        LayerKind::Frame => INKPOD_LAYER_FRAME,
        LayerKind::VanishingPoint => INKPOD_LAYER_VANISHING_POINT,
        LayerKind::Adjustment => INKPOD_LAYER_ADJUSTMENT,
        LayerKind::Text => INKPOD_LAYER_TEXT,
        LayerKind::Annotation => INKPOD_LAYER_ANNOTATION,
        LayerKind::VectorColoring => INKPOD_LAYER_VECTOR_COLORING,
    }
}

fn parse_plane_type(value: u32) -> Result<PlaneType, u32> {
    match value {
        INKPOD_TYPED_PLANE_MAIN_LINE => Ok(PlaneType::MainLine),
        INKPOD_TYPED_PLANE_COLOR => Ok(PlaneType::Color),
        INKPOD_TYPED_PLANE_RASTER => Ok(PlaneType::Raster),
        INKPOD_TYPED_PLANE_SELECTION => Ok(PlaneType::Selection),
        INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE => Ok(PlaneType::VectorMainLine),
        INKPOD_TYPED_PLANE_COLOR_TRACE => Ok(PlaneType::ColorTrace),
        INKPOD_TYPED_PLANE_VECTOR_FILL => Ok(PlaneType::VectorFill),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "typed plane kind is not defined",
        )),
    }
}

fn plane_type_code(value: PlaneType) -> u32 {
    match value {
        PlaneType::MainLine => INKPOD_TYPED_PLANE_MAIN_LINE,
        PlaneType::Color => INKPOD_TYPED_PLANE_COLOR,
        PlaneType::Raster => INKPOD_TYPED_PLANE_RASTER,
        PlaneType::Selection => INKPOD_TYPED_PLANE_SELECTION,
        PlaneType::VectorMainLine => INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE,
        PlaneType::ColorTrace => INKPOD_TYPED_PLANE_COLOR_TRACE,
        PlaneType::VectorFill => INKPOD_TYPED_PLANE_VECTOR_FILL,
    }
}

fn parse_storage_format(value: u32) -> Result<PixelFormat, u32> {
    match value {
        INKPOD_STORAGE_BINARY8 => Ok(PixelFormat::BinaryMask8),
        INKPOD_STORAGE_GRAYSCALE8 => Ok(PixelFormat::Grayscale8),
        INKPOD_STORAGE_GRAYSCALE16 => Ok(PixelFormat::Grayscale16),
        INKPOD_STORAGE_RGBA8 => Ok(PixelFormat::StraightRgba8),
        INKPOD_STORAGE_RGBA16 => Ok(PixelFormat::StraightRgba16),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "storage pixel format is not defined",
        )),
    }
}

fn parse_common_raster_format(value: u32) -> Result<CommonRasterFormat, u32> {
    match value {
        INKPOD_COMMON_RASTER_PNG => Ok(CommonRasterFormat::Png),
        INKPOD_COMMON_RASTER_TIFF => Ok(CommonRasterFormat::Tiff),
        INKPOD_COMMON_RASTER_TGA => Ok(CommonRasterFormat::Tga),
        INKPOD_COMMON_RASTER_BMP => Ok(CommonRasterFormat::Bmp),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "common raster format is not defined",
        )),
    }
}

fn storage_format_code(value: PixelFormat) -> u32 {
    match value {
        PixelFormat::BinaryMask8 => INKPOD_STORAGE_BINARY8,
        PixelFormat::Grayscale8 => INKPOD_STORAGE_GRAYSCALE8,
        PixelFormat::Grayscale16 => INKPOD_STORAGE_GRAYSCALE16,
        PixelFormat::StraightRgba8 => INKPOD_STORAGE_RGBA8,
        PixelFormat::StraightRgba16 => INKPOD_STORAGE_RGBA16,
        PixelFormat::PremultipliedBgra8 => 0,
    }
}

fn parse_selection_operation(value: u32) -> Result<SelectionOperation, u32> {
    match value {
        INKPOD_SELECTION_NEW => Ok(SelectionOperation::New),
        INKPOD_SELECTION_ADD => Ok(SelectionOperation::Add),
        INKPOD_SELECTION_SUBTRACT => Ok(SelectionOperation::Subtract),
        INKPOD_SELECTION_INTERSECT => Ok(SelectionOperation::Intersect),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "selection operation is not defined",
        )),
    }
}

fn parse_tool(value: u32) -> Result<PaintTool, u32> {
    match value {
        INKPOD_TOOL_PENCIL => Ok(PaintTool::Pencil),
        INKPOD_TOOL_BRUSH => Ok(PaintTool::Brush),
        INKPOD_TOOL_ERASER => Ok(PaintTool::Eraser),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "tool is not a defined M1 paint tool",
        )),
    }
}

fn parse_coordinate_space(value: u32) -> Result<CoordinateSpace, u32> {
    match value {
        INKPOD_COORDINATE_SPACE_DOCUMENT => Ok(CoordinateSpace::Document),
        INKPOD_COORDINATE_SPACE_DEVICE => Ok(CoordinateSpace::Device),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "coordinate_space is not defined",
        )),
    }
}

// SAFETY: The caller must provide a readable, aligned strided span for every
// advertised record. Each record must expose at least its size prefix.
unsafe fn parse_stroke_samples(
    samples: *const InkpodStrokeSample,
    sample_count: u64,
    sample_stride_bytes: u64,
) -> Result<Vec<StrokeSample>, u32> {
    if sample_count == 0 || sample_count > MAX_STROKE_SAMPLE_COUNT {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke sample_count is outside bounds",
        ));
    }
    if samples.is_null() || !is_aligned(samples) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke samples are null or misaligned",
        ));
    }
    let sample_count = usize::try_from(sample_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke sample_count is not representable",
        )
    })?;
    let stride = match usize::try_from(sample_stride_bytes) {
        Ok(stride)
            if stride >= size_of::<InkpodStrokeSample>()
                && stride % align_of::<InkpodStrokeSample>() == 0 =>
        {
            stride
        }
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sample_stride_bytes is too small, misaligned, or not representable",
            ));
        }
    };
    let storage_bytes = sample_count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodStrokeSample>()));
    if storage_bytes.is_none_or(|bytes| bytes > isize::MAX as usize) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke sample storage size overflows",
        ));
    }

    let mut parsed = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        // SAFETY: The checked count/stride span is readable by contract.
        let sample_pointer = unsafe {
            samples
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodStrokeSample>()
        };
        // SAFETY: Each record exposes its readable size prefix.
        let sample_size = unsafe { validate_struct(sample_pointer, "InkpodStrokeSample") }?;
        if u64::from(sample_size) > sample_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodStrokeSample.struct_size exceeds sample_stride_bytes",
            ));
        }
        // SAFETY: The complete known record prefix is aligned and readable.
        let sample = unsafe { &*sample_pointer };
        if sample.flags != 0 || sample.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stroke sample contains unsupported flags or reserved values",
            ));
        }
        parsed.push(StrokeSample {
            x: sample.x,
            y: sample.y,
            pressure: sample.pressure,
        });
    }
    Ok(parsed)
}

// SAFETY: `input` must be a validated, complete public structure whose sample
// span satisfies the exported function contract.
unsafe fn parse_stroke_input(input: &InkpodStrokeInput) -> Result<Stroke, u32> {
    if input.flags & !(INKPOD_STROKE_FLAG_AUTO_ERASE | INKPOD_STROKE_FLAG_PRESSURE_SIZE) != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "stroke input contains unsupported flags",
        ));
    }
    // SAFETY: Forwarded from this helper's caller contract.
    let samples = unsafe {
        parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
    }?;
    let tool = parse_tool(input.tool)?;
    let plane = parse_plane(input.plane)?;
    let coordinate_space = parse_coordinate_space(input.coordinate_space)?;
    Ok(Stroke {
        tool,
        plane,
        color: [
            (input.color_rgba >> 24) as u8,
            (input.color_rgba >> 16) as u8,
            (input.color_rgba >> 8) as u8,
            input.color_rgba as u8,
        ],
        diameter: input.diameter,
        auto_erase: input.flags & INKPOD_STROKE_FLAG_AUTO_ERASE != 0,
        pressure_size: input.flags & INKPOD_STROKE_FLAG_PRESSURE_SIZE != 0,
        coordinate_space,
        samples,
    })
}

fn parse_effect_region_kind(shape: u32) -> Result<EffectRegionKind, u32> {
    match shape {
        INKPOD_SELECTION_TRACE => Ok(EffectRegionKind::Trace),
        INKPOD_SELECTION_RECTANGLE => Ok(EffectRegionKind::Rectangle),
        INKPOD_SELECTION_POLYLINE => Ok(EffectRegionKind::Polyline),
        INKPOD_SELECTION_LASSO => Ok(EffectRegionKind::Lasso),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "effect region must be pen, rectangle, polyline, or lasso",
        )),
    }
}

// SAFETY: `color` must expose a complete, readable InkpodColorValue prefix.
unsafe fn parse_color_value(color: *const InkpodColorValue) -> Result<PixelValue, u32> {
    // SAFETY: Forwarded from this helper's caller contract.
    unsafe { validate_struct(color, "InkpodColorValue") }?;
    // SAFETY: The complete known structure is readable after validation.
    let color = unsafe { &*color };
    match color.depth {
        INKPOD_COLOR_DEPTH_BINARY if color.red <= u16::from(u8::MAX) => {
            Ok(PixelValue::Binary(color.red as u8))
        }
        INKPOD_COLOR_DEPTH_GRAYSCALE_8 if color.red <= u16::from(u8::MAX) => {
            Ok(PixelValue::Grayscale8(color.red as u8))
        }
        INKPOD_COLOR_DEPTH_GRAYSCALE_16 => Ok(PixelValue::Grayscale16(color.red)),
        INKPOD_COLOR_DEPTH_8
            if [color.red, color.green, color.blue, color.alpha]
                .into_iter()
                .all(|channel| channel <= u16::from(u8::MAX)) =>
        {
            Ok(PixelValue::Rgba([
                color.red as u8,
                color.green as u8,
                color.blue as u8,
                color.alpha as u8,
            ]))
        }
        INKPOD_COLOR_DEPTH_16 => Ok(PixelValue::Rgba16([
            color.red,
            color.green,
            color.blue,
            color.alpha,
        ])),
        INKPOD_COLOR_DEPTH_BINARY | INKPOD_COLOR_DEPTH_GRAYSCALE_8
            if color.red > u16::from(u8::MAX) =>
        {
            Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "8-bit scalar color contains a value above 255",
            ))
        }
        INKPOD_COLOR_DEPTH_8 => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "8-bit color contains a channel above 255",
        )),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color depth is not 8 or 16 bits",
        )),
    }
}

unsafe fn parse_vector_path_input(input: &InkpodVectorPathInput) -> Result<VectorPathInput, u32> {
    if input.reserved != 0 || input.flags & !INKPOD_VECTOR_PATH_CLOSED != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "vector path input contains unsupported values",
        ));
    }
    let count = usize::try_from(input.segment_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector segment count is not representable",
        )
    })?;
    if count == 0 || count > 262_144 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector segment count is outside bounds",
        ));
    }
    let stride = usize::try_from(input.segment_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector segment stride is not representable",
        )
    })?;
    if input.segments.is_null()
        || !is_aligned(input.segments)
        || stride < size_of::<InkpodVectorCubicSegment>()
        || stride % align_of::<InkpodVectorCubicSegment>() != 0
        || count
            .checked_mul(stride)
            .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector segment span is null, misaligned, or outside bounds",
        ));
    }
    let mut segments = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The validated borrowed strided span contains this record.
        let pointer = unsafe {
            input
                .segments
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodVectorCubicSegment>()
        };
        // SAFETY: Every record exposes its readable size prefix.
        let size = unsafe { validate_struct(pointer, "InkpodVectorCubicSegment") }?;
        if u64::from(size) > input.segment_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "vector segment struct_size exceeds its stride",
            ));
        }
        // SAFETY: The complete known record is readable after validation.
        let segment = unsafe { &*pointer };
        if segment.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector segment reserved field is not zero",
            ));
        }
        let point = |value: InkpodVectorPoint| PointF32 {
            x: value.x,
            y: value.y,
        };
        segments.push(VectorCubicSegment {
            p0: point(segment.p0),
            p1: point(segment.p1),
            p2: point(segment.p2),
            p3: point(segment.p3),
            width_start: segment.width_start,
            width_end: segment.width_end,
        });
    }
    // SAFETY: The nested color record is a complete field of the validated input.
    let color = unsafe { parse_color_value(&raw const input.color) }?;
    Ok(VectorPathInput {
        segments,
        color,
        closed: input.flags & INKPOD_VECTOR_PATH_CLOSED != 0,
    })
}

fn parse_vector_erase_mode(value: u32) -> Result<VectorEraseMode, u32> {
    match value {
        INKPOD_VECTOR_ERASE_PARTIAL => Ok(VectorEraseMode::Partial),
        INKPOD_VECTOR_ERASE_TO_INTERSECTION => Ok(VectorEraseMode::ToIntersection),
        INKPOD_VECTOR_ERASE_WHOLE_PATH => Ok(VectorEraseMode::WholePath),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector erase mode is not defined",
        )),
    }
}

fn parse_vector_width_mode(value: u32, parameter: f32) -> Result<VectorWidthMode, u32> {
    match value {
        INKPOD_VECTOR_WIDTH_ADD => Ok(VectorWidthMode::Add(parameter)),
        INKPOD_VECTOR_WIDTH_SUBTRACT => Ok(VectorWidthMode::Subtract(parameter)),
        INKPOD_VECTOR_WIDTH_SCALE => Ok(VectorWidthMode::Scale(parameter)),
        INKPOD_VECTOR_WIDTH_CONSTANT => Ok(VectorWidthMode::Constant(parameter)),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector width mode is not defined",
        )),
    }
}

fn parse_vector_selection_mode(value: u32) -> Result<VectorSelectionMode, u32> {
    match value {
        INKPOD_VECTOR_SELECT_CUT_BY_SELECTION => Ok(VectorSelectionMode::CutBySelection),
        INKPOD_VECTOR_SELECT_TOUCHING => Ok(VectorSelectionMode::Touching),
        INKPOD_VECTOR_SELECT_FULLY_CONTAINED => Ok(VectorSelectionMode::FullyContained),
        INKPOD_VECTOR_SELECT_LINE => Ok(VectorSelectionMode::Line),
        INKPOD_VECTOR_SELECT_WHOLE_LINE => Ok(VectorSelectionMode::WholeLine),
        INKPOD_VECTOR_SELECT_TO_INTERSECTION => Ok(VectorSelectionMode::ToIntersection),
        INKPOD_VECTOR_SELECT_FILL_BOUNDARY => Ok(VectorSelectionMode::FillBoundary),
        INKPOD_VECTOR_SELECT_FILL => Ok(VectorSelectionMode::Fill),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vector selection mode is not defined",
        )),
    }
}

fn write_color_value(output: &mut InkpodColorValue, color: PixelValue) -> Result<(), u32> {
    match color {
        PixelValue::Rgba(value) => {
            output.depth = INKPOD_COLOR_DEPTH_8;
            output.red = u16::from(value[0]);
            output.green = u16::from(value[1]);
            output.blue = u16::from(value[2]);
            output.alpha = u16::from(value[3]);
            Ok(())
        }
        PixelValue::Rgba16(value) => {
            output.depth = INKPOD_COLOR_DEPTH_16;
            output.red = value[0];
            output.green = value[1];
            output.blue = value[2];
            output.alpha = value[3];
            Ok(())
        }
        _ => Err(fail(
            INKPOD_STATUS_INVALID_STATE,
            "eyedropper returned a non-color value",
        )),
    }
}

fn clipboard_pixel_rgba8(color: PixelValue) -> [u8; 4] {
    match color {
        PixelValue::Binary(value) | PixelValue::Grayscale8(value) => [0, 0, 0, value],
        PixelValue::Grayscale16(value) => [0, 0, 0, (value / 257) as u8],
        PixelValue::Rgba(value) => value,
        PixelValue::Rgba16(value) => [
            (value[0] / 257) as u8,
            (value[1] / 257) as u8,
            (value[2] / 257) as u8,
            (value[3] / 257) as u8,
        ],
    }
}

fn color_value_record(color: PixelValue) -> Result<InkpodColorValue, u32> {
    let mut output = InkpodColorValue {
        struct_size: size_of::<InkpodColorValue>() as u32,
        ..InkpodColorValue::default()
    };
    write_color_value(&mut output, color)?;
    Ok(output)
}

// SAFETY: `input` and every advertised strided record must be complete and
// readable for this call.
unsafe fn parse_color_array(input: &InkpodColorArray) -> Result<Vec<PixelValue>, u32> {
    if input.reserved != 0 || input.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "color array contains unsupported flags or reserved values",
        ));
    }
    if input.color_count > MAX_PALETTE_COLOR_COUNT {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array count exceeds the bounded palette limit",
        ));
    }
    let count = usize::try_from(input.color_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array count is not representable",
        )
    })?;
    if count == 0 {
        if !input.colors.is_null() || input.color_stride_bytes != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "an empty color array must use a null pointer and zero stride",
            ));
        }
        return Ok(Vec::new());
    }
    if input.colors.is_null() || !is_aligned(input.colors) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array storage is null or misaligned",
        ));
    }
    let stride = usize::try_from(input.color_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array stride is not representable",
        )
    })?;
    if stride < size_of::<InkpodColorValue>() || stride % align_of::<InkpodColorValue>() != 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array stride is too small or misaligned",
        ));
    }
    let storage = count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodColorValue>()));
    if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "color array storage size overflows",
        ));
    }
    let mut colors = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The checked count/stride span is readable by contract.
        let pointer = unsafe {
            input
                .colors
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodColorValue>()
        };
        // SAFETY: Every record exposes a readable size prefix.
        let struct_size = unsafe { validate_struct(pointer, "InkpodColorValue") }?;
        if u64::from(struct_size) > input.color_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodColorValue.struct_size exceeds color array stride",
            ));
        }
        // SAFETY: The complete known record is readable after validation.
        colors.push(unsafe { parse_color_value(pointer) }?);
    }
    Ok(colors)
}

// SAFETY: `input` and its optional color span must be complete and readable for
// this call. Every advertised strided record exposes its own size prefix.
unsafe fn parse_fill_input(input: &InkpodFillInput) -> Result<FillRequest, u32> {
    const SUPPORTED_FLAGS: u64 = INKPOD_FILL_FLAG_DETACHED_REGIONS
        | INKPOD_FILL_FLAG_OVERFLOW_ABORT
        | INKPOD_FILL_FLAG_TRANSPARENT_ONLY
        | INKPOD_FILL_FLAG_SELECTION_PRESENT
        | INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY
        | INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR
        | INKPOD_FILL_FLAG_DOCUMENT_SELECTION;
    if input.flags & !SUPPORTED_FLAGS != 0 || input.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "fill input contains unsupported flags or reserved values",
        ));
    }
    let operation = match input.operation {
        INKPOD_FILL_SEED => FillOperation::Seed,
        INKPOD_FILL_CLOSED_REGION => FillOperation::ClosedRegion,
        INKPOD_FILL_EXTENSION => FillOperation::Extend,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill operation is not defined",
            ));
        }
    };
    let inclusion_mode = match input.inclusion_mode {
        INKPOD_INCLUSION_NONE => InclusionMode::None,
        INKPOD_INCLUSION_SPECIFIED => InclusionMode::Specified,
        INKPOD_INCLUSION_EXCEPT_SPECIFIED => InclusionMode::ExceptSpecified,
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion mode is not defined",
            ));
        }
    };
    let gap_close = u8::try_from(input.gap_close).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "fill gap-close value is not representable",
        )
    })?;
    // SAFETY: The embedded color resides inside the validated input.
    let color = unsafe { parse_color_value(ptr::addr_of!(input.color)) }?;
    if input.inclusion_color_count > 6 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "fill inclusion color count exceeds six",
        ));
    }
    let count = usize::try_from(input.inclusion_color_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "fill inclusion color count is not representable",
        )
    })?;
    let stride = if count == 0 {
        0
    } else {
        let stride = usize::try_from(input.inclusion_color_stride_bytes).map_err(|_| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion color stride is not representable",
            )
        })?;
        if stride < size_of::<InkpodColorValue>() || stride % align_of::<InkpodColorValue>() != 0 {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion color stride is too small or misaligned",
            ));
        }
        if input.inclusion_colors.is_null() || !is_aligned(input.inclusion_colors) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion colors are null or misaligned",
            ));
        }
        let storage = count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodColorValue>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "fill inclusion color storage size overflows",
            ));
        }
        stride
    };
    let mut inclusion_colors = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: The checked strided record span is readable by contract.
        let color_pointer = unsafe {
            input
                .inclusion_colors
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodColorValue>()
        };
        // SAFETY: Each record exposes a readable size prefix and complete body.
        let struct_size = unsafe { validate_struct(color_pointer, "InkpodColorValue") }?;
        if u64::from(struct_size) > input.inclusion_color_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodColorValue.struct_size exceeds inclusion color stride",
            ));
        }
        // SAFETY: The record is complete and validated.
        inclusion_colors.push(unsafe { parse_color_value(color_pointer) }?);
    }
    let selection_present = input.flags & INKPOD_FILL_FLAG_SELECTION_PRESENT != 0;
    let selection = selection_present.then_some(RectI32 {
        x: input.selection.x,
        y: input.selection.y,
        width: input.selection.width,
        height: input.selection.height,
    });
    if !selection_present
        && (input.selection.x != 0
            || input.selection.y != 0
            || input.selection.width != 0
            || input.selection.height != 0)
    {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "fill selection fields require the selection-present flag",
        ));
    }
    Ok(FillRequest {
        operation,
        seed_x: input.seed_x,
        seed_y: input.seed_y,
        color,
        selection,
        use_document_selection: input.flags & INKPOD_FILL_FLAG_DOCUMENT_SELECTION != 0,
        tolerance: input.tolerance,
        detached_regions: input.flags & INKPOD_FILL_FLAG_DETACHED_REGIONS != 0,
        overflow_abort: input.flags & INKPOD_FILL_FLAG_OVERFLOW_ABORT != 0,
        gap_close,
        transparent_only: input.flags & INKPOD_FILL_FLAG_TRANSPARENT_ONLY != 0,
        inclusion_mode,
        inclusion_colors,
        extension_distance: input.extension_distance,
    })
}

// SAFETY: `pointer` must identify `length` readable bytes for this call.
unsafe fn path_from_utf8<'a>(pointer: *const u8, length: u64) -> Result<&'a Path, u32> {
    if pointer.is_null() || length == 0 || length > MAX_PATH_BYTES {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "UTF-8 path is null, empty, or exceeds the bounded length",
        ));
    }
    let length = usize::try_from(length).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "UTF-8 path length is not representable",
        )
    })?;
    // SAFETY: The exported-function contract requires this readable range.
    let bytes = unsafe { slice::from_raw_parts(pointer, length) };
    let text = std::str::from_utf8(bytes)
        .map_err(|_| fail(INKPOD_STATUS_INVALID_ARGUMENT, "path is not valid UTF-8"))?;
    if text.as_bytes().contains(&0) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "path contains an embedded NUL",
        ));
    }
    Ok(Path::new(text))
}

unsafe fn name_from_utf8<'a>(pointer: *const u8, length: u64) -> Result<&'a str, u32> {
    if length == 0 || length > MAX_NODE_NAME_BYTES || pointer.is_null() {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "node name pointer or length is invalid",
        ));
    }
    let length = usize::try_from(length).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "node name length is not representable",
        )
    })?;
    // SAFETY: The exported caller contract requires this complete range to be readable.
    let bytes = unsafe { slice::from_raw_parts(pointer, length) };
    let text = std::str::from_utf8(bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "node name is not valid UTF-8",
        )
    })?;
    if text.as_bytes().contains(&0) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "node name contains an embedded NUL",
        ));
    }
    Ok(text)
}

struct ParsedM4Raster {
    document_uuid: u128,
    source_revision: u64,
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    dpi_x_milli: Option<u32>,
    dpi_y_milli: Option<u32>,
    reference_frame: RectI32,
    pixels: Vec<u8>,
}

// SAFETY: input and its advertised pixel rows must remain readable for this call.
unsafe fn parse_m4_raster(input: &InkpodM4RasterInput) -> Result<ParsedM4Raster, u32> {
    unsafe { validate_struct(input, "InkpodM4RasterInput") }?;
    if input.flags != 0 || input.source_revision == 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "M4 raster flags or source revision is invalid",
        ));
    }
    let document_uuid =
        (u128::from(input.document_uuid_high) << 64) | u128::from(input.document_uuid_low);
    if document_uuid == 0
        || input.width == 0
        || input.height == 0
        || input.width > MAX_RASTER_DIMENSION
        || input.height > MAX_RASTER_DIMENSION
        || input.reference_frame.width <= 0
        || input.reference_frame.height <= 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "M4 raster identity or dimensions are invalid",
        ));
    }
    let pixel_format = parse_storage_format(input.pixel_format)?;
    if !matches!(
        pixel_format,
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
    ) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "M4 raster must use straight RGBA8 or RGBA16",
        ));
    }
    if input.dpi_x_milli == 0 || input.dpi_y_milli == 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "M4 raster DPI must be nonzero",
        ));
    }
    let row_bytes = usize::try_from(input.width)
        .ok()
        .and_then(|width| width.checked_mul(pixel_format.bytes_per_pixel()))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M4 raster row length overflows",
            )
        })?;
    let stride = usize::try_from(input.row_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "M4 raster row stride is not representable",
        )
    })?;
    let height = input.height as usize;
    let required = height
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(row_bytes))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M4 raster byte range overflows",
            )
        })?;
    if stride < row_bytes
        || input.pixels.is_null()
        || required > isize::MAX as usize
        || input.pixel_bytes < required as u64
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "M4 raster pointer, stride, or byte length is invalid",
        ));
    }
    let compact_length = row_bytes.checked_mul(height).ok_or_else(|| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "M4 compact raster length overflows",
        )
    })?;
    if compact_length > MAX_COMMON_RASTER_BYTES || required > MAX_COMMON_RASTER_BYTES {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "M4 raster byte length exceeds its bound",
        ));
    }
    let mut pixels = Vec::with_capacity(compact_length);
    for row in 0..height {
        // SAFETY: required validated the final readable byte of every row.
        let source = unsafe { input.pixels.add(row * stride) };
        // SAFETY: Each row advertises at least row_bytes readable bytes.
        pixels.extend_from_slice(unsafe { slice::from_raw_parts(source, row_bytes) });
    }
    Ok(ParsedM4Raster {
        document_uuid,
        source_revision: input.source_revision,
        width: input.width,
        height: input.height,
        pixel_format,
        dpi_x_milli: Some(input.dpi_x_milli),
        dpi_y_milli: Some(input.dpi_y_milli),
        reference_frame: RectI32 {
            x: input.reference_frame.x,
            y: input.reference_frame.y,
            width: input.reference_frame.width,
            height: input.reference_frame.height,
        },
        pixels,
    })
}

fn parse_sequence_direction(value: u32) -> Result<SequenceDirection, u32> {
    match value {
        INKPOD_SEQUENCE_PREVIOUS => Ok(SequenceDirection::Previous),
        INKPOD_SEQUENCE_NEXT => Ok(SequenceDirection::Next),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "sequence direction is not defined",
        )),
    }
}

fn write_motion_frame(output: &mut InkpodMotionFrame, frame: MotionFrame) {
    output.flags = (if frame.paused {
        INKPOD_MOTION_FRAME_PAUSED
    } else {
        0
    }) | if frame.include_selection {
        INKPOD_MOTION_FRAME_INCLUDE_SELECTION
    } else {
        0
    } | if frame.include_light_table {
        INKPOD_MOTION_FRAME_INCLUDE_LIGHT_TABLE
    } else {
        0
    };
    output.sequence_index = frame.sequence_index as u64;
    output.cell_number = frame.cell_number;
    output.thumbnail_width = frame.thumbnail.width;
    output.thumbnail_height = frame.thumbnail.height;
    output.reserved = 0;
    output.thumbnail_checksum = frame.thumbnail.checksum;
}

#[unsafe(no_mangle)]
pub extern "C" fn inkpod_abi_version() -> u32 {
    INKPOD_ABI_VERSION
}

/// Creates a single-writer core handle.
///
/// # Safety
/// `config` must expose a readable size prefix and the byte range it advertises.
/// `out_core` must point to writable storage for one non-overlapping handle
/// pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_create(
    config: *const InkpodCoreConfig,
    out_core: *mut *mut InkpodCore,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_core.is_null() || !is_aligned(out_core) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_core is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires writable storage at out_core.
        unsafe { out_core.write(ptr::null_mut()) };

        // SAFETY: The exported API requires a readable public-structure prefix.
        if let Err(status) = unsafe { validate_struct(config, "InkpodCoreConfig") } {
            return status;
        }
        // SAFETY: The size prefix was validated and the caller contract makes
        // the complete configuration readable for this call.
        let config = unsafe { &*config };
        if config.abi_version != INKPOD_ABI_VERSION {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodCoreConfig.abi_version is unsupported",
            );
        }
        if config.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodCoreConfig contains unsupported feature flags",
            );
        }

        let handle = Box::new(InkpodCore {
            owner_thread: thread::current().id(),
            core: Core::new(),
        });
        // SAFETY: out_core is writable by contract and now receives Box ownership.
        unsafe { out_core.write(Box::into_raw(handle)) };
        INKPOD_STATUS_OK
    })
}

/// Destroys a core and nulls the caller's pointer. Repeating the call with the
/// same pointer variable is a safe no-op.
///
/// # Safety
/// `core` must point to writable storage that contains either null or a handle
/// returned by `inkpod_core_create` and not already destroyed through an alias.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_destroy(core: *mut *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core owner pointer is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires readable/writable pointer storage.
        let handle = unsafe { core.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core handle is misaligned");
        }
        // SAFETY: The caller contract guarantees a live handle from core_create.
        let core_ref = unsafe { &*handle };
        let thread_status = validate_core_thread(core_ref);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // Null first so a repeated call using the same owner variable is harmless.
        // SAFETY: The outer pointer is writable by contract.
        unsafe { core.write(ptr::null_mut()) };
        // SAFETY: Ownership came from Box::into_raw and is consumed exactly once.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Dispatches a validated command batch on the creating thread.
///
/// # Safety
/// All pointers must follow the non-overlapping sizes, strides, lifetimes, and
/// ownership rules declared in `include/inkpod/core_ffi.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_dispatch_batch(
    core: *mut InkpodCore,
    batch: *const InkpodCommandBatch,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The exported API requires readable/writable structure prefixes.
        if let Err(status) = unsafe { validate_struct(batch, "InkpodCommandBatch") } {
            return status;
        }
        // SAFETY: The result prefix is readable before the validated output is written.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }

        // SAFETY: Complete structures were size-checked, and valid live,
        // non-overlapping objects and output storage are required by contract.
        let core = unsafe { &mut *core };
        let batch = unsafe { &*batch };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if batch.reserved != 0 || batch.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "batch contains unsupported flags or reserved values",
            );
        }
        if batch.command_count > MAX_COMMAND_COUNT {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch command_count exceeds the bounded M0 limit",
            );
        }
        if batch.commands.is_null() && batch.command_count != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "commands is null for a non-empty batch",
            );
        }

        let command_count = match usize::try_from(batch.command_count) {
            Ok(count) => count,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch command_count cannot be represented on this platform",
                );
            }
        };
        let command_stride = match usize::try_from(batch.command_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodCommand>()
                    && stride % align_of::<InkpodCommand>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "command_stride_bytes is too small, misaligned, or not representable",
                );
            }
        };
        if !batch.commands.is_null() && !is_aligned(batch.commands) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "commands is misaligned");
        }
        let command_storage_bytes = command_count
            .saturating_sub(1)
            .checked_mul(command_stride)
            .and_then(|last_offset| last_offset.checked_add(size_of::<InkpodCommand>()));
        if command_storage_bytes.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch command storage size overflows",
            );
        }

        let mut domain_commands = Vec::with_capacity(command_count);
        for index in 0..command_count {
            let offset = index * command_stride;
            // SAFETY: The checked count/stride range fits isize, and the caller
            // promises readable command storage for that complete range.
            let command_pointer = unsafe {
                batch
                    .commands
                    .cast::<u8>()
                    .add(offset)
                    .cast::<InkpodCommand>()
            };
            // SAFETY: The containing range and element alignment were checked,
            // and the caller promises readable records for the complete span.
            let command_struct_size =
                match unsafe { validate_struct(command_pointer, "InkpodCommand") } {
                    Ok(struct_size) => struct_size,
                    Err(status) => return status,
                };
            if u64::from(command_struct_size) > batch.command_stride_bytes {
                return fail(
                    INKPOD_STATUS_INCOMPATIBLE_ABI,
                    "InkpodCommand.struct_size exceeds command_stride_bytes",
                );
            }
            // SAFETY: The element size, alignment, and containing storage were
            // validated before constructing the known ABI prefix reference.
            let command = unsafe { &*command_pointer };
            if command.flags != 0 {
                return fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "InkpodCommand.flags contains unsupported bits",
                );
            }
            if command.kind != INKPOD_COMMAND_NO_OP {
                return fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "InkpodCommand.kind is not defined by M0",
                );
            }
            domain_commands.push(Command::NoOp);
        }

        let outcome = core.core.dispatch(&domain_commands);
        result.reserved = 0;
        result.revision = outcome.revision();
        result.accepted_command_count = outcome.accepted_commands();
        INKPOD_STATUS_OK
    })
}

/// Creates a new M1 two-plane cell document.
///
/// # Safety
/// Pointers must reference live, non-overlapping objects with readable/writable
/// ranges described by their size prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_new_cell(
    core: *mut InkpodCore,
    options: *const InkpodCellCreateOptions,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structure pointers expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(options, "InkpodCellCreateOptions") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects and writable output are required by contract.
        let core = unsafe { &mut *core };
        let options = unsafe { &*options };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "cell options contain unsupported flags or reserved values",
            );
        }
        let document_uuid =
            (u128::from(options.document_uuid_high) << 64) | u128::from(options.document_uuid_low);
        match core.core.new_cell_with_uuid(
            options.width,
            options.height,
            options.dpi_x_milli,
            options.dpi_y_milli,
            document_uuid,
        ) {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies current document metadata and checksums.
///
/// # Safety
/// `core` must be live on its owner thread and `out_info` must expose its
/// complete writable advertised range without overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_document_info(
    core: *mut InkpodCore,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Live core and writable output are required by contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.document_info() {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Transactionally updates the four production frames and independent margins.
///
/// # Safety
/// `core` must be live on its owner thread, `input` must be a complete readable
/// record, and `result` must be complete writable non-overlapping storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_update_paper_frames(
    core: *mut InkpodCore,
    input: *const InkpodPaperFramesInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Readable/writable complete records are required by contract.
        if let Err(status) = unsafe { validate_struct(input, "InkpodPaperFramesInput") } {
            return status;
        }
        // SAFETY: Readable/writable complete records are required by contract.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Validation proved the advertised known prefixes readable.
        let input = unsafe { &*input };
        if input.reserved != 0 || input.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "paper-frame input contains unsupported flags",
            );
        }
        let frame = |value: InkpodFrameRect| RectI32 {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        };
        let frames = FrameMetadata {
            hundred_frame: frame(input.hundred_frame),
            reference_frame: frame(input.reference_frame),
            drawing_frame: frame(input.drawing_frame),
            safe_frame: frame(input.safe_frame),
            margins: Margins {
                left: input.margin_left,
                top: input.margin_top,
                right: input.margin_right,
                bottom: input.margin_bottom,
            },
        };
        // SAFETY: Live owner-thread objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.update_paper_frames(frames) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Switches the editable plane without mutating document pixels or revision.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_active_plane(core: *mut InkpodCore, plane: u32) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A live core is required by the caller contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let plane = match parse_plane(plane) {
            Ok(plane) => plane,
            Err(status) => return status,
        };
        match core.core.set_active_plane(plane) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Selects a stable-ID layer/plane pair without changing document pixels.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_active_node(
    core: *mut InkpodCore,
    layer_id: u64,
    plane_id: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A live core is required by the caller contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.set_active_node(layer_id, plane_id) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies seed/closed-region/extension fill as one all-or-nothing history
/// transaction. A leak returns INKPOD_STATUS_FILL_OVERFLOW and its candidate
/// coordinate without committing any pixel.
///
/// # Safety
/// Core/input/result and every optional strided color record must be complete,
/// live, aligned, readable/writable as applicable, and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_fill(
    core: *mut InkpodCore,
    input: *const InkpodFillInput,
    result: *mut InkpodFillResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodFillInput") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodFillResult") } {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        result.flags = 0;
        result.revision = 0;
        result.changed_pixel_count = 0;
        result.leak_x = 0;
        result.leak_y = 0;
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The input and optional strided span were validated above.
        let request = match unsafe { parse_fill_input(input) } {
            Ok(request) => request,
            Err(status) => return status,
        };
        match core.core.apply_fill_with_light_table(
            &request,
            input.flags & INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY != 0,
            input.flags & INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR != 0,
        ) {
            Ok(outcome) => {
                result.revision = outcome.dispatch.revision();
                result.changed_pixel_count = outcome.changed_pixels;
                INKPOD_STATUS_OK
            }
            Err(CoreError::FillOverflow { x, y }) => {
                result.flags = INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE;
                result.leak_x = x;
                result.leak_y = y;
                fail(
                    INKPOD_STATUS_FILL_OVERFLOW,
                    &format!("fill reached image edge at ({x}, {y})"),
                )
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples an exact 8/16-bit color from the requested M2 source.
///
/// # Safety
/// Core/output must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_eyedropper(
    core: *mut InkpodCore,
    source: u32,
    x: u32,
    y: u32,
    out_color: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let out_color = unsafe { &mut *out_color };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let source = match source {
            INKPOD_EYEDROPPER_TOPMOST_NONTRANSPARENT => EyedropperSource::TopmostNonTransparent,
            INKPOD_EYEDROPPER_SELECTED_PLANE => EyedropperSource::SelectedPlane,
            INKPOD_EYEDROPPER_COMPOSITE => EyedropperSource::Composite,
            INKPOD_EYEDROPPER_LIGHT_TABLE_TOPMOST => EyedropperSource::LightTableTopmost,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "eyedropper source is not defined",
                );
            }
        };
        match core.core.eyedropper(source, x, y) {
            Ok(color) => match write_color_value(out_color, color) {
                Ok(()) => INKPOD_STATUS_OK,
                Err(status) => status,
            },
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces the document palette as one exact-depth metadata transaction.
///
/// # Safety
/// Core/input/result and every strided color record must be complete, live,
/// aligned, non-overlapping owner-thread objects for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_palette_set(
    core: *mut InkpodCore,
    input: *const InkpodColorArray,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodColorArray") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live, non-overlapping objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The input and its complete strided span were validated above.
        let colors = match unsafe { parse_color_array(input) } {
            Ok(colors) => colors,
            Err(status) => return status,
        };
        match core.core.replace_palette(&colors) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies the exact-depth document palette into caller-owned strided storage.
/// A zero-capacity null buffer is a successful count query.
///
/// # Safety
/// Core/buffer and any advertised output records must be complete, writable,
/// aligned, live, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_palette_get(
    core: *mut InkpodCore,
    buffer: *mut InkpodColorBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(buffer.cast_const(), "InkpodColorBuffer") } {
            return status;
        }
        // SAFETY: Complete live, non-overlapping objects are required by contract.
        let core = unsafe { &mut *core };
        let buffer = unsafe { &mut *buffer };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if buffer.reserved != 0 || buffer.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "palette buffer contains unsupported flags or reserved values",
            );
        }
        let colors = match core.core.palette() {
            Ok(colors) => colors,
            Err(error) => return map_core_error(error),
        };
        buffer.color_count = colors.len() as u64;
        if buffer.color_capacity == 0 {
            if !buffer.colors.is_null() || buffer.color_stride_bytes != 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "a palette count query must use a null pointer and zero stride",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if buffer.color_capacity > MAX_PALETTE_COLOR_COUNT
            || buffer.colors.is_null()
            || !is_aligned(buffer.colors)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "palette output capacity or storage is invalid",
            );
        }
        let stride = match usize::try_from(buffer.color_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodColorValue>()
                    && stride % align_of::<InkpodColorValue>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "palette output stride is too small, misaligned, or not representable",
                );
            }
        };
        if buffer.color_capacity < colors.len() as u64 {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "palette output capacity is smaller than color_count",
            );
        }
        let storage = colors
            .len()
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodColorValue>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "palette output storage size overflows",
            );
        }
        for (index, color) in colors.iter().copied().enumerate() {
            let record = match color_value_record(color) {
                Ok(record) => record,
                Err(status) => return status,
            };
            // SAFETY: The checked caller-owned strided output range is writable.
            unsafe {
                buffer
                    .colors
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodColorValue>()
                    .write(record);
            }
        }
        INKPOD_STATUS_OK
    })
}

/// Extracts a bounded quantized unique-color chart and stores it as the document palette.
///
/// # Safety
/// Core/result must be complete live owner-thread records and must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_palette_generate(
    core: *mut InkpodCore,
    maximum_colors: u32,
    quantization_bits: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let quantization_bits = match u8::try_from(quantization_bits) {
            Ok(bits) => bits,
            Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "quantization exceeds u8"),
        };
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core
            .core
            .generate_palette_from_document(maximum_colors as usize, quantization_bits)
        {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Changes the base color used by a grayscale main-line plane.
///
/// # Safety
/// Core/color/result must be complete, live, aligned, and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_main_line_color(
    core: *mut InkpodCore,
    color: *const InkpodColorValue,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(color, "InkpodColorValue") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live, non-overlapping objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The complete input record was validated above.
        let color = match unsafe { parse_color_value(color) } {
            Ok(color) => color,
            Err(status) => return status,
        };
        match core.core.set_main_line_color(color) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies the exact-depth grayscale main-line base color.
///
/// # Safety
/// Core/output must be complete, live, aligned, and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_main_line_color(
    core: *mut InkpodCore,
    out_color: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") }
        {
            return status;
        }
        // SAFETY: Complete live, non-overlapping objects are required by contract.
        let core = unsafe { &mut *core };
        let out_color = unsafe { &mut *out_color };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.main_line_color() {
            Ok(color) => match write_color_value(out_color, color) {
                Ok(()) => INKPOD_STATUS_OK,
                Err(status) => status,
            },
            Err(error) => map_core_error(error),
        }
    })
}

/// Changes only the temporary coloring-check view; document revision/history
/// and pixel values remain untouched.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_color_check(core: *mut InkpodCore, mode: u32) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A complete live Core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let mode = match mode {
            INKPOD_COLOR_CHECK_OFF => None,
            INKPOD_COLOR_CHECK_LEGACY_WHITE => Some(ColorCheckMode::LegacyWhiteTransparency),
            INKPOD_COLOR_CHECK_NATIVE_ALPHA => Some(ColorCheckMode::NativeAlpha),
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "color-check mode is not defined",
                );
            }
        };
        match core.core.set_color_check(mode) {
            Ok(_) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies all samples from pointer-down through pointer-up as one transaction.
///
/// # Safety
/// The input, sample span, output, and Core must be live, aligned,
/// non-overlapping ranges for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_stroke(
    core: *mut InkpodCore,
    input: *const InkpodStrokeInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structure pointers expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodStrokeInput") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects and writable output are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The complete input and its borrowed sample span were validated above.
        let stroke = match unsafe { parse_stroke_input(input) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core.core.apply_stroke(&stroke) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Starts one Core-owned transient stroke transaction and stages the supplied
/// first sample batch without changing document revision, history, or dirty.
///
/// # Safety
/// Core/input/sample storage must satisfy the same contract as
/// `inkpod_core_apply_stroke` and remain live for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_begin(
    core: *mut InkpodCore,
    input: *const InkpodStrokeInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodStrokeInput") } {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The complete input and borrowed span were validated above.
        let stroke = match unsafe { parse_stroke_input(input) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core.core.begin_stroke(&stroke) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Appends one borrowed, strided sample batch to the active transient stroke.
/// Failure discards the Core-owned preview and commits no document state.
///
/// # Safety
/// Core/span/sample storage must be complete, live, aligned, non-overlapping
/// owner-thread objects for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_append(
    core: *mut InkpodCore,
    span: *const InkpodStrokeSampleSpan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        let span_status = unsafe { validate_struct(span, "InkpodStrokeSampleSpan") };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if let Err(status) = span_status {
            core.core.cancel_stroke();
            return status;
        }
        let span = unsafe { &*span };
        if span.reserved != 0 || span.feature_flags != INKPOD_FEATURE_NONE {
            core.core.cancel_stroke();
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stroke sample span contains unsupported values",
            );
        }
        // SAFETY: The borrowed span is validated by the exported contract.
        let samples = match unsafe {
            parse_stroke_samples(span.samples, span.sample_count, span.sample_stride_bytes)
        } {
            Ok(samples) => samples,
            Err(status) => {
                core.core.cancel_stroke();
                return status;
            }
        };
        match core.core.append_stroke(&samples) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits the active transient stroke as one document/history transaction.
///
/// # Safety
/// Core/result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_end(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The result exposes a readable size prefix before writing.
        let result_status = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if let Err(status) = result_status {
            core.core.cancel_stroke();
            return status;
        }
        let result = unsafe { &mut *result };
        match core.core.end_stroke() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Discards any active transient stroke. Calling with no active stroke is a
/// successful no-op.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_cancel(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A complete live Core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        core.core.cancel_stroke();
        INKPOD_STATUS_OK
    })
}

/// Undoes one committed transaction.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_undo(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { history_operation(core, result, false) }
}

/// Redoes one committed transaction.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_redo(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { history_operation(core, result, true) }
}

unsafe fn history_operation(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
    redo: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let operation = if redo {
            core.core.redo()
        } else {
            core.core.undo()
        };
        match operation {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Queries the current history cursor and bounded item count.
///
/// # Safety
/// Core and output must be live, aligned owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_history_info(
    core: *mut InkpodCore,
    out_info: *mut InkpodHistoryInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodHistoryInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let out = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        out.reserved = 0;
        out.cursor = core.core.history_cursor() as u64;
        out.item_count = core.core.history_entries().len() as u64;
        INKPOD_STATUS_OK
    })
}

/// Queries one history label into caller-owned UTF-8 storage.
///
/// # Safety
/// Core/output and any advertised name buffer must remain live and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_history_item(
    core: *mut InkpodCore,
    index: u64,
    out_item: *mut InkpodHistoryItem,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_item.cast_const(), "InkpodHistoryItem") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let out = unsafe { &mut *out_item };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let index = match usize::try_from(index) {
            Ok(value) => value,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "history index is not representable",
                );
            }
        };
        let entries = core.core.history_entries();
        let Some(entry) = entries.get(index) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "history index is outside the available range",
            );
        };
        out.flags = if entry.applied {
            INKPOD_HISTORY_ITEM_APPLIED
        } else {
            0
        };
        out.index = entry.index as u64;
        out.name_bytes = entry.label.len() as u64;
        if out.name_capacity == 0 {
            if !out.name_utf8.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity history label buffer must be null",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if out.name_utf8.is_null() || out.name_capacity < out.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "history label buffer is too small",
            );
        }
        // SAFETY: Caller advertises complete writable storage for the copied label.
        unsafe { ptr::copy_nonoverlapping(entry.label.as_ptr(), out.name_utf8, entry.label.len()) };
        INKPOD_STATUS_OK
    })
}

/// Moves the history cursor to any available state.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_history_jump(
    core: *mut InkpodCore,
    target_cursor: u64,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let target = match usize::try_from(target_cursor) {
            Ok(value) => value,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "history target is not representable",
                );
            }
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.jump_history(target) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Restores the active plane inside the persistent selection from the normal savepoint.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_revert_active_selection(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.revert_active_plane_selection() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Saves to a UTF-8 path using same-directory temporary-file replacement.
///
/// # Safety
/// Path bytes must remain readable, and all object/output pointers must remain
/// live and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_save(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { file_operation(core, path_utf8, path_bytes, out_info, false) }
}

/// Opens a versioned `.inkpod` file from a UTF-8 path.
///
/// # Safety
/// Path bytes must remain readable, and all object/output pointers must remain
/// live and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_open(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { file_operation(core, path_utf8, path_bytes, out_info, true) }
}

/// Writes a recovery container atomically without changing normal savepoint or
/// normal path.
///
/// # Safety
/// Path/Core/output follow the same contract as `inkpod_core_save`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_autosave(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { recovery_file_operation(core, path_utf8, path_bytes, out_info, false) }
}

/// Opens recovery content as a dirty, pathless document. It never inherits the
/// recovered file's former normal-save destination.
///
/// # Safety
/// Path/Core/output follow the same contract as `inkpod_core_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_open_recovery(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { recovery_file_operation(core, path_utf8, path_bytes, out_info, true) }
}

unsafe fn recovery_file_operation(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
    recover: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: The path range is readable for this call by contract.
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let operation = if recover {
            core.core.open_recovery(path)
        } else {
            core.core.autosave(path)
        };
        match operation {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

unsafe fn file_operation(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
    open: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: The path range is readable for this call by contract.
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let operation = if open {
            core.core.open(path)
        } else {
            core.core.save(path)
        };
        match operation {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Reopens the last normal-save path and discards unsaved changes.
///
/// # Safety
/// Core/output must be live owner-thread objects with non-overlapping storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_revert(
    core: *mut InkpodCore,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.revert() {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

fn parse_view_command(core: &Core, input: &InkpodViewInput) -> Result<ViewCommand, u32> {
    if input.flags != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "view input contains unsupported flags",
        ));
    }
    let command = match input.kind {
        INKPOD_VIEW_PAN_BY => ViewCommand::PanBy {
            device_dx: input.value1,
            device_dy: input.value2,
        },
        INKPOD_VIEW_ZOOM_AT => ViewCommand::ZoomAt {
            factor: input.value1,
            device_x: input.value2,
            device_y: input.value3,
        },
        INKPOD_VIEW_FIT => ViewCommand::Fit {
            viewport_width: input.value1,
            viewport_height: input.value2,
        },
        INKPOD_VIEW_ONE_TO_ONE => ViewCommand::OneToOne {
            viewport_width: input.value1,
            viewport_height: input.value2,
        },
        INKPOD_VIEW_VIEWPORT_RESIZED => ViewCommand::ViewportResized {
            viewport_width: input.value1,
            viewport_height: input.value2,
        },
        INKPOD_VIEW_BOX_ZOOM => {
            if !input.value1.is_finite()
                || !input.value2.is_finite()
                || !input.value3.is_finite()
                || !input.value4.is_finite()
                || input.value1 < f64::from(i32::MIN)
                || input.value1 > f64::from(i32::MAX)
                || input.value2 < f64::from(i32::MIN)
                || input.value2 > f64::from(i32::MAX)
                || input.value3 < f64::from(i32::MIN)
                || input.value3 > f64::from(i32::MAX)
                || input.value4 < f64::from(i32::MIN)
                || input.value4 > f64::from(i32::MAX)
            {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "box zoom rectangle is outside the document coordinate range",
                ));
            }
            let view = core.view_state();
            ViewCommand::BoxZoom {
                document_rect: RectI32 {
                    x: input.value1 as i32,
                    y: input.value2 as i32,
                    width: input.value3 as i32,
                    height: input.value4 as i32,
                },
                viewport_width: view.viewport_width(),
                viewport_height: view.viewport_height(),
            }
        }
        INKPOD_VIEW_FLIP_HORIZONTAL => ViewCommand::Flip {
            axis: MirrorAxis::Horizontal,
        },
        INKPOD_VIEW_FLIP_VERTICAL => ViewCommand::Flip {
            axis: MirrorAxis::Vertical,
        },
        INKPOD_VIEW_SET_RULER_VISIBLE => ViewCommand::SetRulerVisible(input.value1 != 0.0),
        INKPOD_VIEW_SET_GUIDES_VISIBLE => ViewCommand::SetGuidesVisible(input.value1 != 0.0),
        INKPOD_VIEW_SET_GRID_VISIBLE => ViewCommand::SetGridVisible(input.value1 != 0.0),
        INKPOD_VIEW_SET_SNAP_ENABLED => ViewCommand::SetSnapEnabled(input.value1 != 0.0),
        INKPOD_VIEW_SET_GUIDE_SNAP_ENABLED => ViewCommand::SetGuideSnapEnabled(input.value1 != 0.0),
        INKPOD_VIEW_SET_GRID_SNAP_ENABLED => ViewCommand::SetGridSnapEnabled(input.value1 != 0.0),
        INKPOD_VIEW_SET_TRANSPARENT_VISIBLE => ViewCommand::SetTransparentView(input.value1 != 0.0),
        INKPOD_VIEW_SET_ALPHA_VISIBLE => ViewCommand::SetAlphaView(input.value1 != 0.0),
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "view command kind is not defined",
            ));
        }
    };
    Ok(command)
}

/// Applies a logical view command without changing document revision/history.
///
/// # Safety
/// Core/input/output must be complete, live, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_view(
    core: *mut InkpodCore,
    input: *const InkpodViewInput,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodViewInput") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let command = match parse_view_command(&core.core, input) {
            Ok(command) => command,
            Err(status) => return status,
        };
        if let Err(error) = core.core.apply_view(command) {
            return map_core_error(error);
        }
        match core.core.document_info() {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Builds one immutable snapshot owned by Rust.
///
/// # Safety
/// `core` must be live, `options` must expose its advertised readable byte
/// range, and `out_snapshot` must point to non-overlapping handle storage that
/// does not currently contain a live snapshot handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_build_snapshot(
    core: *mut InkpodCore,
    options: *const InkpodSnapshotOptions,
    out_snapshot: *mut *mut InkpodSnapshot,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_snapshot.is_null() || !is_aligned(out_snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_snapshot is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires writable output pointer storage.
        unsafe { out_snapshot.write(ptr::null_mut()) };
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The exported API requires a readable public-structure prefix.
        if let Err(status) = unsafe { validate_struct(options, "InkpodSnapshotOptions") } {
            return status;
        }

        // SAFETY: Live/readable objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let options = unsafe { &*options };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "snapshot options contain unsupported values",
            );
        }

        let snapshot = snapshot_handle(core.core.build_snapshot());
        // SAFETY: The output is writable and receives Box ownership.
        unsafe { out_snapshot.write(Box::into_raw(snapshot)) };
        INKPOD_STATUS_OK
    })
}

/// Adds one bounded cubic, variable-width vector path as a single history
/// transaction. The borrowed strided segment span is copied before return.
///
/// # Safety
/// Core/input/result/output storage must be complete, aligned, live,
/// non-overlapping owner-thread objects for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_add_path(
    core: *mut InkpodCore,
    input: *const InkpodVectorPathInput,
    result: *mut InkpodDispatchResult,
    out_path_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_path_id.is_null() || !is_aligned(out_path_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector path core or output is null or misaligned",
            );
        }
        // SAFETY: Output storage is writable by contract.
        unsafe { out_path_id.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorPathInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The validated input owns the complete borrowed nested span.
        let parsed = match unsafe { parse_vector_path_input(input) } {
            Ok(parsed) => parsed,
            Err(status) => return status,
        };
        match core.core.vector_add_path(input.plane_id, parsed) {
            Ok((outcome, path_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output storage was checked above.
                unsafe { out_path_id.write(path_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Adds a fill whose borrowed boundary-ID span is copied before return.
///
/// # Safety
/// All pointers must be complete, aligned, live, and non-overlapping for this
/// owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_add_fill(
    core: *mut InkpodCore,
    input: *const InkpodVectorFillInput,
    result: *mut InkpodDispatchResult,
    out_fill_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_fill_id.is_null() || !is_aligned(out_fill_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector fill core or output is null or misaligned",
            );
        }
        // SAFETY: Output storage is writable by contract.
        unsafe { out_fill_id.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorFillInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector fill input contains unsupported values",
            );
        }
        let count = match usize::try_from(input.boundary_path_count) {
            Ok(count) if (1..=262_144).contains(&count) => count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "vector fill boundary count is outside bounds",
                );
            }
        };
        if input.boundary_path_ids.is_null()
            || !is_aligned(input.boundary_path_ids)
            || count
                .checked_mul(size_of::<u64>())
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector fill boundary span is invalid",
            );
        }
        // SAFETY: The bounded aligned span is readable for this call.
        let boundaries = unsafe { slice::from_raw_parts(input.boundary_path_ids, count) }.to_vec();
        // SAFETY: The nested color record is a complete field of the input.
        let color = match unsafe { parse_color_value(&raw const input.color) } {
            Ok(color) => color,
            Err(status) => return status,
        };
        match core
            .core
            .vector_add_fill(input.plane_id, &boundaries, color)
        {
            Ok((outcome, fill_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output storage was checked above.
                unsafe { out_fill_id.write(fill_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one partial/intersection/full vector erase transaction.
///
/// # Safety
/// Core/input/result must be complete, aligned, live, and non-overlapping on
/// the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_erase(
    core: *mut InkpodCore,
    input: *const InkpodVectorEraseInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorEraseInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector erase reserved field is not zero",
            );
        }
        let mode = match parse_vector_erase_mode(input.mode) {
            Ok(mode) => mode,
            Err(status) => return status,
        };
        match core.core.vector_erase(
            input.plane_id,
            PointF32 {
                x: input.x,
                y: input.y,
            },
            input.radius,
            mode,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Connects the deterministic nearest endpoint pair within `maximum_gap`.
/// A zero output ID means the command was a successful no-op.
///
/// # Safety
/// Core/result/output must be complete, aligned, live, non-overlapping
/// owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_connect(
    core: *mut InkpodCore,
    plane_id: u64,
    maximum_gap: f32,
    result: *mut InkpodDispatchResult,
    out_path_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_path_id.is_null() || !is_aligned(out_path_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector connect core or output is null or misaligned",
            );
        }
        // SAFETY: Output storage is writable by contract.
        unsafe { out_path_id.write(0) };
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.vector_connect(plane_id, maximum_gap) {
            Ok((outcome, path_id)) => {
                write_dispatch_result(result, outcome);
                if let Some(path_id) = path_id {
                    // SAFETY: Writable output storage was checked above.
                    unsafe { out_path_id.write(path_id) };
                }
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one width correction to a borrowed path-ID span.
///
/// # Safety
/// Core/input/result and nested ID storage must be complete, aligned, live,
/// and non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_correct_width(
    core: *mut InkpodCore,
    input: *const InkpodVectorWidthInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorWidthInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector width input contains unsupported values",
            );
        }
        let count = match usize::try_from(input.path_count) {
            Ok(count) if (1..=65_536).contains(&count) => count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "vector width path count is outside bounds",
                );
            }
        };
        if input.path_ids.is_null()
            || !is_aligned(input.path_ids)
            || count
                .checked_mul(size_of::<u64>())
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector width path span is invalid",
            );
        }
        // SAFETY: The bounded aligned span is readable for this call.
        let path_ids = unsafe { slice::from_raw_parts(input.path_ids, count) }.to_vec();
        let mode = match parse_vector_width_mode(input.mode, input.parameter) {
            Ok(mode) => mode,
            Err(status) => return status,
        };
        match core.core.vector_correct_width(&path_ids, mode) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Queries deterministic vector selection ranges into caller-owned buffers.
/// A zero-capacity null span is a successful count query.
///
/// # Safety
/// Core/input/output and any advertised output spans must be complete, aligned,
/// live, writable, and non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_select(
    core: *mut InkpodCore,
    input: *const InkpodVectorSelectionInput,
    output: *mut InkpodVectorSelectionBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorSelectionInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodVectorSelectionBuffer") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || output.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector selection contains unsupported flags or reserved values",
            );
        }
        let mode = match parse_vector_selection_mode(input.mode) {
            Ok(mode) => mode,
            Err(status) => return status,
        };
        let selected = match core.core.vector_select(
            RectI32 {
                x: input.bounds.x,
                y: input.bounds.y,
                width: input.bounds.width,
                height: input.bounds.height,
            },
            mode,
        ) {
            Ok(selected) => selected,
            Err(error) => return map_core_error(error),
        };
        output.range_count = selected.path_ranges.len() as u64;
        output.fill_count = selected.fill_ids.len() as u64;
        if output.range_capacity == 0 {
            if !output.ranges.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity vector range output must be null",
                );
            }
        } else if output.range_capacity > 65_536
            || output.ranges.is_null()
            || !is_aligned(output.ranges)
            || usize::try_from(output.range_capacity)
                .ok()
                .and_then(|count| count.checked_mul(size_of::<InkpodVectorSelectionRange>()))
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector range output span is invalid",
            );
        }
        if output.fill_capacity == 0 {
            if !output.fill_ids.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity vector fill output must be null",
                );
            }
        } else if output.fill_capacity > 65_536
            || output.fill_ids.is_null()
            || !is_aligned(output.fill_ids)
            || usize::try_from(output.fill_capacity)
                .ok()
                .and_then(|count| count.checked_mul(size_of::<u64>()))
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector fill output span is invalid",
            );
        }
        if output.range_capacity < output.range_count || output.fill_capacity < output.fill_count {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "vector selection output capacity is too small",
            );
        }
        for (index, range) in selected.path_ranges.iter().enumerate() {
            let record = InkpodVectorSelectionRange {
                struct_size: size_of::<InkpodVectorSelectionRange>() as u32,
                reserved: 0,
                path_id: range.path_id,
                start_million: range.start_million,
                end_million: range.end_million,
            };
            // SAFETY: The caller-owned bounded output span is writable by contract.
            unsafe { output.ranges.add(index).write(record) };
        }
        if !selected.fill_ids.is_empty() {
            // SAFETY: Capacity and byte bounds were checked and the spans may not overlap.
            unsafe {
                ptr::copy_nonoverlapping(
                    selected.fill_ids.as_ptr(),
                    output.fill_ids,
                    selected.fill_ids.len(),
                )
            };
        }
        INKPOD_STATUS_OK
    })
}

/// Rasterizes one vector layer into caller-owned straight RGBA8 storage. A
/// zero-capacity null buffer is a successful size query.
///
/// # Safety
/// Core/input/output and any advertised pixel range must be complete, aligned,
/// live, writable, and non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_rasterize(
    core: *mut InkpodCore,
    input: *const InkpodVectorRasterizeInput,
    output: *mut InkpodVectorRasterBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorRasterizeInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodVectorRasterBuffer") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0
            || input.reserved_2 != 0
            || input.feature_flags & !INKPOD_VECTOR_RASTERIZE_ANTIALIAS != 0
            || output.reserved != 0
            || output.reserved_2 != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector rasterize contains unsupported flags or reserved values",
            );
        }
        let (width, height, stride_bytes, required_bytes) =
            match core.core.vector_raster_layout(input.layer_id, input.scale) {
                Ok(layout) => layout,
                Err(error) => return map_core_error(error),
            };
        output.required_bytes = required_bytes;
        output.width = width;
        output.height = height;
        output.stride_bytes = stride_bytes;
        if output.pixel_capacity == 0 {
            if !output.pixels.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity vector raster output must be null",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if output.pixels.is_null()
            || output.pixel_capacity > isize::MAX as u64
            || output.pixel_capacity < output.required_bytes
        {
            return fail(
                if output.pixel_capacity < output.required_bytes {
                    INKPOD_STATUS_BUFFER_TOO_SMALL
                } else {
                    INKPOD_STATUS_INVALID_ARGUMENT
                },
                "vector raster output storage is invalid or too small",
            );
        }
        let raster = match core.core.rasterize_vector_layer(
            input.layer_id,
            input.scale,
            input.feature_flags & INKPOD_VECTOR_RASTERIZE_ANTIALIAS != 0,
        ) {
            Ok(raster) => raster,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: The caller advertises enough writable byte storage and it may
        // not overlap Core/input/output memory.
        unsafe {
            ptr::copy_nonoverlapping(raster.pixels.as_ptr(), output.pixels, raster.pixels.len())
        };
        INKPOD_STATUS_OK
    })
}

/// Rasterizes one vector layer at document scale into a new RGBA8 raster
/// layer, preserving the source and committing one history unit.
///
/// # Safety
/// Core/input/name/result/output storage must be complete, aligned, live, and
/// non-overlapping on the Core owner thread. The name bytes are borrowed only
/// for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vector_rasterize_to_layer(
    core: *mut InkpodCore,
    input: *const InkpodVectorRasterizeInput,
    name_utf8: *const u8,
    name_bytes: u64,
    result: *mut InkpodDispatchResult,
    out_layer_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_layer_id.is_null()
            || !is_aligned(out_layer_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vector rasterize-to-layer core or output is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by the caller contract.
        unsafe { out_layer_id.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodVectorRasterizeInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live structures and name span are required by the
        // exported contract and validated before they are borrowed.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let name = match unsafe { name_from_utf8(name_utf8, name_bytes) } {
            Ok(name) => name,
            Err(status) => return status,
        };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0
            || input.reserved_2 != 0
            || input.scale != 1
            || input.feature_flags & !INKPOD_VECTOR_RASTERIZE_ANTIALIAS != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "vector rasterize-to-layer requires scale 1 and supported flags",
            );
        }
        match core.core.rasterize_vector_layer_to_document(
            input.layer_id,
            input.feature_flags & INKPOD_VECTOR_RASTERIZE_ANTIALIAS != 0,
            name,
        ) {
            Ok((outcome, layer_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output storage was validated above.
                unsafe { out_layer_id.write(layer_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Converts bounded RGBA8 raster runs into vector paths/fills as one history
/// transaction and reports the number of created fills.
///
/// # Safety
/// Core/input/result/count storage must be complete, aligned, live, writable,
/// and non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_raster_vectorize(
    core: *mut InkpodCore,
    input: *const InkpodRasterVectorizeInput,
    result: *mut InkpodDispatchResult,
    out_fill_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_fill_count.is_null()
            || !is_aligned(out_fill_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "raster vectorize core or output is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by contract.
        unsafe { out_fill_count.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodRasterVectorizeInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || input.alpha_threshold > u8::MAX.into() {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "raster vectorize contains unsupported flags or alpha threshold",
            );
        }
        match core.core.vectorize_raster_plane(
            input.source_plane_id,
            input.target_layer_id,
            input.alpha_threshold as u8,
        ) {
            Ok((outcome, fill_ids)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output storage was checked above.
                unsafe { out_fill_count.write(fill_ids.len() as u64) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies the immutable, batched view descriptor for a live snapshot.
///
/// # Safety
/// `snapshot` must be live and externally synchronized; `out_view` must expose
/// its advertised writable byte range without overlapping the snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_view(
    snapshot: *const InkpodSnapshot,
    out_view: *mut InkpodSnapshotView,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        // SAFETY: The output prefix is readable before the validated view is written.
        if let Err(status) = unsafe { validate_struct(out_view.cast_const(), "InkpodSnapshotView") }
        {
            return status;
        }
        // SAFETY: A live snapshot and complete, writable, non-overlapping view
        // are required by contract; the view size was checked above.
        let snapshot = unsafe { &*snapshot };
        let out_view = unsafe { &mut *out_view };

        out_view.abi_version = INKPOD_ABI_VERSION;
        out_view.feature_flags = snapshot.snapshot.feature_flags();
        out_view.revision = snapshot.snapshot.revision();
        out_view.tiles = if snapshot.tiles.is_empty() {
            ptr::null()
        } else {
            snapshot.tiles.as_ptr()
        };
        out_view.tile_count = snapshot.tiles.len() as u64;
        out_view.tile_stride_bytes = size_of::<InkpodSnapshotTile>() as u64;
        INKPOD_STATUS_OK
    })
}

/// Copies the immutable document-to-device transform carried by a snapshot.
///
/// # Safety
/// `snapshot` must be live and externally synchronized; `out_transform` must
/// expose its complete writable advertised range without overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_transform(
    snapshot: *const InkpodSnapshot,
    out_transform: *mut InkpodSnapshotTransform,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(out_transform.cast_const(), "InkpodSnapshotTransform") }
        {
            return status;
        }
        // SAFETY: Live snapshot and writable output are required by contract.
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *out_transform };
        let view = snapshot.snapshot.view();
        output.flags = (if view.flip_horizontal() {
            INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL
        } else {
            0
        }) | if view.flip_vertical() {
            INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL
        } else {
            0
        };
        output.view_revision = view.revision();
        output.zoom = view.zoom();
        output.pan_x = view.pan_x();
        output.pan_y = view.pan_y();
        output.document_width = snapshot.snapshot.document_width();
        output.document_height = snapshot.snapshot.document_height();
        INKPOD_STATUS_OK
    })
}

/// Copies immutable ruler, guide, grid, snap, and transparent-view overlay data.
///
/// # Safety
/// `snapshot` must be live and externally synchronized; `out_overlay` must
/// expose its complete writable advertised range without overlap. The guide
/// span remains borrowed from `snapshot` until that snapshot is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_overlay(
    snapshot: *const InkpodSnapshot,
    out_overlay: *mut InkpodSnapshotOverlay,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(out_overlay.cast_const(), "InkpodSnapshotOverlay") }
        {
            return status;
        }
        // SAFETY: Live snapshot and writable output are required by contract.
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *out_overlay };
        let view = snapshot.snapshot.view();
        let grid = snapshot.snapshot.grid();
        output.flags = (if view.ruler_visible() {
            INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE
        } else {
            0
        }) | (if view.guides_visible() {
            INKPOD_SNAPSHOT_OVERLAY_GUIDES_VISIBLE
        } else {
            0
        }) | (if view.grid_visible() {
            INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE
        } else {
            0
        }) | (if view.snap_enabled() {
            INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED
        } else {
            0
        }) | (if view.transparent_view() {
            INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW
        } else {
            0
        }) | if view.alpha_view() {
            INKPOD_SNAPSHOT_OVERLAY_ALPHA_VIEW
        } else {
            0
        };
        output.grid_origin_x = grid.origin_x;
        output.grid_origin_y = grid.origin_y;
        output.grid_spacing_x = grid.spacing_x;
        output.grid_spacing_y = grid.spacing_y;
        output.grid_subdivisions = grid.subdivisions;
        output.reserved = 0;
        output.guides = if snapshot.guides.is_empty() {
            ptr::null()
        } else {
            snapshot.guides.as_ptr()
        };
        output.guide_count = snapshot.guides.len() as u64;
        output.guide_stride_bytes = size_of::<InkpodSnapshotGuide>() as u64;
        INKPOD_STATUS_OK
    })
}

/// Copies immutable flattened vector spans. All pointers borrow storage owned
/// by `snapshot` and remain valid only until that snapshot is released.
///
/// # Safety
/// Snapshot/output must be complete, aligned, live, externally synchronized,
/// writable/non-overlapping objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_vectors(
    snapshot: *const InkpodSnapshot,
    out_vectors: *mut InkpodSnapshotVectorView,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_vectors.cast_const(), "InkpodSnapshotVectorView") }
        {
            return status;
        }
        // SAFETY: Live snapshot and writable output are required by contract.
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *out_vectors };
        output.abi_version = INKPOD_ABI_VERSION;
        output.feature_flags = INKPOD_FEATURE_NONE;
        output.segments = if snapshot.vector_segments.is_empty() {
            ptr::null()
        } else {
            snapshot.vector_segments.as_ptr()
        };
        output.segment_count = snapshot.vector_segments.len() as u64;
        output.segment_stride_bytes = size_of::<InkpodSnapshotVectorSegment>() as u64;
        output.fills = if snapshot.vector_fills.is_empty() {
            ptr::null()
        } else {
            snapshot.vector_fills.as_ptr()
        };
        output.fill_count = snapshot.vector_fills.len() as u64;
        output.fill_stride_bytes = size_of::<InkpodSnapshotVectorFill>() as u64;
        output.boundary_path_ids = if snapshot.vector_boundary_path_ids.is_empty() {
            ptr::null()
        } else {
            snapshot.vector_boundary_path_ids.as_ptr()
        };
        output.boundary_path_count = snapshot.vector_boundary_path_ids.len() as u64;
        INKPOD_STATUS_OK
    })
}

/// Releases a snapshot and nulls the caller's pointer. Snapshots may be viewed
/// and released from a renderer thread after publication.
///
/// # Safety
/// `snapshot` must point to writable storage containing null or a handle
/// returned by `inkpod_core_build_snapshot` and not released through an alias.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_release(snapshot: *mut *mut InkpodSnapshot) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        assert_snapshot_thread_contract();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot owner pointer is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires readable/writable pointer storage.
        let handle = unsafe { snapshot.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot handle is misaligned",
            );
        }
        // SAFETY: The outer pointer is writable; nulling precedes the ownership drop.
        unsafe { snapshot.write(ptr::null_mut()) };
        // SAFETY: Ownership came from Box::into_raw and is consumed exactly once.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Returns the required UTF-8 error buffer size, including its trailing NUL.
///
/// # Safety
/// `out_required_bytes` must point to writable `u64` storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_error_message_size(out_required_bytes: *mut u64) -> u32 {
    ffi_boundary(|| {
        if out_required_bytes.is_null() || !is_aligned(out_required_bytes) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_required_bytes is null or misaligned",
            );
        }
        let required = LAST_ERROR.with(|slot| {
            slot.try_borrow()
                .map_or(1, |slot| u64::try_from(slot.len + 1).unwrap_or(1))
        });
        // SAFETY: Writable output storage is required by the caller contract.
        unsafe { out_required_bytes.write(required) };
        INKPOD_STATUS_OK
    })
}

/// Copies the current thread's UTF-8 error text and a trailing NUL.
///
/// # Safety
/// `buffer` must reference `buffer_capacity` writable bytes and `out_written_bytes`
/// must point to writable `u64` storage. The two regions must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_error_message_copy(
    buffer: *mut u8,
    buffer_capacity: u64,
    out_written_bytes: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        if out_written_bytes.is_null() || !is_aligned(out_written_bytes) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_written_bytes is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by the caller contract.
        unsafe { out_written_bytes.write(0) };
        let capacity = match usize::try_from(buffer_capacity) {
            Ok(capacity) => capacity,
            Err(_) => return INKPOD_STATUS_BUFFER_TOO_SMALL,
        };
        let required = LAST_ERROR.with(|slot| slot.try_borrow().map_or(1, |slot| slot.len + 1));
        if capacity < required || buffer.is_null() {
            return INKPOD_STATUS_BUFFER_TOO_SMALL;
        }

        let copied = LAST_ERROR.with(|slot| {
            let Ok(slot) = slot.try_borrow() else {
                return 0;
            };
            // SAFETY: The caller supplies at least len + 1 writable bytes. The
            // thread-local source cannot overlap caller memory.
            unsafe {
                ptr::copy_nonoverlapping(slot.bytes.as_ptr(), buffer, slot.len + 1);
            }
            slot.len
        });
        // SAFETY: Writable output storage is required by the caller contract.
        unsafe { out_written_bytes.write(copied as u64) };
        INKPOD_STATUS_OK
    })
}

/// Applies one typed M3 layer/plane edit. Name bytes are borrowed only for the
/// call. `out_object_id` receives a created/duplicated ID or zero.
///
/// # Safety
/// `core` must be a live handle used on its owner thread. `input` and `result`
/// must expose complete non-overlapping records, any advertised name range must
/// be readable, and `out_object_id` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_tree_edit(
    core: *mut InkpodCore,
    input: *const InkpodTreeEdit,
    result: *mut InkpodDispatchResult,
    out_object_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_object_id.is_null()
            || !is_aligned(out_object_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "tree edit pointer is null or misaligned",
            );
        }
        // SAFETY: Public structure pointers expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodTreeEdit") } {
            return status;
        }
        // SAFETY: Result prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects and output storage are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        // SAFETY: out_object_id is writable by contract and was validated above.
        unsafe { out_object_id.write(0) };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.flags & !(INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE) != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "tree edit contains unsupported flags",
            );
        }
        let name = if matches!(
            input.operation,
            INKPOD_TREE_CREATE_LAYER
                | INKPOD_TREE_SET_LAYER_PROPERTIES
                | INKPOD_TREE_CREATE_PLANE
                | INKPOD_TREE_SET_PLANE_PROPERTIES
        ) {
            // SAFETY: The input contract includes the advertised name byte range.
            match unsafe { name_from_utf8(input.name_utf8, input.name_bytes) } {
                Ok(name) => Some(name),
                Err(status) => return status,
            }
        } else {
            None
        };
        let operation: Result<(inkpod_core::DispatchOutcome, u64), CoreError> = match input
            .operation
        {
            INKPOD_TREE_CREATE_LAYER => match parse_layer_kind(input.kind) {
                Ok(kind) => core.core.create_layer(kind, name.expect("name parsed")),
                Err(status) => return status,
            },
            INKPOD_TREE_DUPLICATE_LAYER => core.core.duplicate_layer(input.object_id),
            INKPOD_TREE_DELETE_LAYER => core
                .core
                .delete_layer(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_REORDER_LAYER => core
                .core
                .reorder_layer(input.object_id, input.destination_index as usize)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_SET_LAYER_PROPERTIES => core
                .core
                .set_layer_properties(
                    input.object_id,
                    input.flags & INKPOD_NODE_VISIBLE != 0,
                    input.flags & INKPOD_NODE_EDITABLE != 0,
                    input.opacity_milli,
                    name.expect("name parsed"),
                )
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_CREATE_PLANE => {
                let kind = match parse_plane_type(input.kind) {
                    Ok(kind) => kind,
                    Err(status) => return status,
                };
                let format = match parse_storage_format(input.pixel_format) {
                    Ok(format) => format,
                    Err(status) => return status,
                };
                core.core
                    .create_plane(input.parent_id, kind, format, name.expect("name parsed"))
            }
            INKPOD_TREE_DUPLICATE_PLANE => core.core.duplicate_plane(input.object_id),
            INKPOD_TREE_DELETE_PLANE => core
                .core
                .delete_plane(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_REORDER_PLANE => core
                .core
                .reorder_plane(input.object_id, input.destination_index as usize)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_SET_PLANE_PROPERTIES => core
                .core
                .set_plane_properties(
                    input.object_id,
                    input.flags & INKPOD_NODE_VISIBLE != 0,
                    input.flags & INKPOD_NODE_EDITABLE != 0,
                    input.opacity_milli,
                    name.expect("name parsed"),
                )
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_CONVERT_LAYER => match parse_layer_kind(input.kind) {
                Ok(kind) => core
                    .core
                    .convert_layer(input.object_id, kind)
                    .map(|outcome| (outcome, 0)),
                Err(status) => return status,
            },
            INKPOD_TREE_MERGE_LAYER => core
                .core
                .merge_layer_into_below(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_TREE_CONVERT_PLANE => {
                let kind = match parse_plane_type(input.kind) {
                    Ok(kind) => kind,
                    Err(status) => return status,
                };
                let format = match parse_storage_format(input.pixel_format) {
                    Ok(format) => format,
                    Err(status) => return status,
                };
                core.core
                    .convert_plane(input.object_id, kind, format)
                    .map(|outcome| (outcome, 0))
            }
            INKPOD_TREE_MERGE_PLANE => core
                .core
                .merge_plane_into_below(input.object_id)
                .map(|outcome| (outcome, 0)),
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "tree edit operation is not defined",
                );
            }
        };
        match operation {
            Ok((outcome, object_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: out_object_id is writable by the exported contract.
                unsafe { out_object_id.write(object_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Queries a layer (`plane_index == UINT32_MAX`) or one of its planes. Name
/// storage remains caller-owned and `name_bytes` always receives the required
/// byte count excluding a terminator.
///
/// # Safety
/// `core` must be a live handle used on its owner thread. `out_info` must be a
/// writable complete record and its optional name buffer must cover the
/// advertised capacity without overlapping Core storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_node_get(
    core: *mut InkpodCore,
    layer_index: u32,
    plane_index: u32,
    out_info: *mut InkpodNodeInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodNodeInfo") } {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let out = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let layers = match core.core.layers() {
            Ok(layers) => layers,
            Err(error) => return map_core_error(error),
        };
        let Some(layer) = layers.get(layer_index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "layer index is outside the tree",
            );
        };
        let (id, parent_id, kind, pixel_format, opacity, flags, child_count, name) =
            if plane_index == u32::MAX {
                (
                    layer.id,
                    0,
                    layer_kind_code(layer.kind),
                    0,
                    layer.opacity_milli,
                    u32::from(layer.visible) | (u32::from(layer.editable) << 1),
                    layer.planes.len() as u32,
                    layer.name.as_str(),
                )
            } else {
                let Some(plane) = layer.planes.get(plane_index as usize) else {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "plane index is outside its layer",
                    );
                };
                (
                    plane.id,
                    layer.id,
                    plane_type_code(plane.kind),
                    storage_format_code(plane.pixel_format),
                    plane.opacity_milli,
                    u32::from(plane.visible) | (u32::from(plane.editable) << 1),
                    0,
                    plane.name.as_str(),
                )
            };
        out.flags = flags;
        out.id = id;
        out.parent_id = parent_id;
        out.kind = kind;
        out.pixel_format = pixel_format;
        out.opacity_milli = opacity;
        out.index = if plane_index == u32::MAX {
            layer_index
        } else {
            plane_index
        };
        out.child_count = child_count;
        out.reserved = 0;
        out.name_bytes = name.len() as u64;
        if out.name_capacity == 0 {
            if !out.name_utf8.is_null() {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity name buffer must be null",
                );
            }
            return INKPOD_STATUS_OK;
        }
        if out.name_utf8.is_null() || out.name_capacity < out.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "node name buffer is too small",
            );
        }
        // SAFETY: Caller provides the complete writable capacity advertised in the output record.
        unsafe { ptr::copy_nonoverlapping(name.as_ptr(), out.name_utf8, name.len()) };
        INKPOD_STATUS_OK
    })
}

/// Applies one bounded selection shape and boolean operation.
///
/// # Safety
/// `core` must be live on its owner thread. `input` and `result` must be valid
/// non-overlapping records, and an advertised point span must be fully readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_selection(
    core: *mut InkpodCore,
    input: *const InkpodSelectionInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodSelectionInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects and ranges are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0 || input.point_count > MAX_SELECTION_POINT_COUNT {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "selection reserved/count value is invalid",
            );
        }
        let operation = match parse_selection_operation(input.operation) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        let needs_points = matches!(
            input.shape,
            INKPOD_SELECTION_LASSO | INKPOD_SELECTION_POLYLINE | INKPOD_SELECTION_TRACE
        );
        let mut points = Vec::new();
        if needs_points {
            if input.points.is_null() || !is_aligned(input.points) || input.point_count == 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection point span is invalid",
                );
            }
            let stride = match usize::try_from(input.point_stride_bytes) {
                Ok(stride)
                    if stride >= size_of::<InkpodSelectionPoint>()
                        && stride % align_of::<InkpodSelectionPoint>() == 0 =>
                {
                    stride
                }
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "selection point stride is invalid",
                    );
                }
            };
            let count = match usize::try_from(input.point_count) {
                Ok(count) => count,
                Err(_) => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "selection point count is not representable",
                    );
                }
            };
            if count
                .saturating_sub(1)
                .checked_mul(stride)
                .and_then(|offset| offset.checked_add(size_of::<InkpodSelectionPoint>()))
                .is_none_or(|bytes| bytes > isize::MAX as usize)
            {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection point span overflows",
                );
            }
            points.reserve(count);
            for index in 0..count {
                // SAFETY: Checked count/stride and caller-readable storage cover this record.
                let pointer = unsafe {
                    input
                        .points
                        .cast::<u8>()
                        .add(index * stride)
                        .cast::<InkpodSelectionPoint>()
                };
                if let Err(status) = unsafe { validate_struct(pointer, "InkpodSelectionPoint") } {
                    return status;
                }
                // SAFETY: Record prefix and containing storage were validated above.
                let point = unsafe { &*pointer };
                if point.struct_size as usize > stride {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "selection point record exceeds its stride",
                    );
                }
                if point.reserved != 0 {
                    return fail(
                        INKPOD_STATUS_UNSUPPORTED,
                        "selection point reserved value is not zero",
                    );
                }
                points.push(PointF32 {
                    x: point.x,
                    y: point.y,
                });
            }
        } else if input.point_count != 0 || !input.points.is_null() || input.point_stride_bytes != 0
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "point-free selection must not carry a point span",
            );
        }
        let shape = match input.shape {
            INKPOD_SELECTION_RECTANGLE => SelectionShape::Rectangle(RectI32 {
                x: input.bounds.x,
                y: input.bounds.y,
                width: input.bounds.width,
                height: input.bounds.height,
            }),
            INKPOD_SELECTION_ELLIPSE => SelectionShape::Ellipse(RectI32 {
                x: input.bounds.x,
                y: input.bounds.y,
                width: input.bounds.width,
                height: input.bounds.height,
            }),
            INKPOD_SELECTION_LASSO => SelectionShape::Lasso(points),
            INKPOD_SELECTION_POLYLINE => SelectionShape::Polyline(points),
            INKPOD_SELECTION_TRACE => SelectionShape::Trace {
                points,
                diameter: input.diameter,
            },
            INKPOD_SELECTION_WAND => SelectionShape::Wand {
                x: input.seed_x,
                y: input.seed_y,
                tolerance: input.tolerance,
                gap_close: match u8::try_from(input.gap_close) {
                    Ok(value) => value,
                    Err(_) => {
                        return fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "wand gap close exceeds its bound",
                        );
                    }
                },
            },
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection shape is not defined",
                );
            }
        };
        match core.core.apply_selection(&shape, operation) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Selects equal or different pixels from the active typed plane.
///
/// # Safety
/// `core` must be live on its owner thread, `color` must expose a complete
/// readable record, and `result` must be writable and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_select_color(
    core: *mut InkpodCore,
    color: *const InkpodColorValue,
    tolerance: u16,
    different: u32,
    operation: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let color = match unsafe { parse_color_value(color) } {
            Ok(color) => color,
            Err(status) => return status,
        };
        let different = match different {
            0 => false,
            1 => true,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "different-selection flag is not boolean",
                );
            }
        };
        let operation = match parse_selection_operation(operation) {
            Ok(operation) => operation,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core
            .core
            .select_color(color, tolerance, different, operation)
        {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies the current typed selection into a Rust-owned clipboard handle.
///
/// # Safety
/// `core` must be live on its owner thread and `out_clipboard` must be writable
/// non-overlapping storage for one handle pointer. That storage must not contain
/// a live clipboard handle, because this function overwrites it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_clipboard_copy(
    core: *mut InkpodCore,
    out_clipboard: *mut *mut InkpodClipboard,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_clipboard.is_null()
            || !is_aligned(out_clipboard)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard copy pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides writable handle storage.
        unsafe { out_clipboard.write(ptr::null_mut()) };
        // SAFETY: Caller contract requires one live owner-thread core.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.copy_selection() {
            Ok(payload) => {
                let clipboard = Box::new(InkpodClipboard { payload });
                // SAFETY: Output storage receives exactly one Rust Box owner.
                unsafe { out_clipboard.write(Box::into_raw(clipboard)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Renders the first typed clipboard plane into caller-owned straight RGBA8.
/// A null buffer with zero capacity performs a size query.
///
/// # Safety
/// `clipboard` must remain live for the call and `output` must be a complete
/// writable record whose advertised pixel range is writable and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_clipboard_render_rgba8(
    clipboard: *const InkpodClipboard,
    output: *mut InkpodClipboardRasterBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if clipboard.is_null() || !is_aligned(clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard handle is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodClipboardRasterBuffer") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let clipboard = unsafe { &*clipboard };
        let output = unsafe { &mut *output };
        if output.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "clipboard raster flags are not supported",
            );
        }
        let Some(plane) = clipboard.payload.planes.first() else {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "clipboard has no plane payload",
            );
        };
        let width = match u32::try_from(clipboard.payload.bounds.width) {
            Ok(width) if width != 0 => width,
            _ => return fail(INKPOD_STATUS_INVALID_STATE, "clipboard width is invalid"),
        };
        let height = match u32::try_from(clipboard.payload.bounds.height) {
            Ok(height) if height != 0 => height,
            _ => return fail(INKPOD_STATUS_INVALID_STATE, "clipboard height is invalid"),
        };
        let packed_stride = match u64::from(width).checked_mul(4) {
            Some(stride) => stride,
            None => return fail(INKPOD_STATUS_INVALID_STATE, "clipboard stride overflows"),
        };
        output.origin_x = clipboard.payload.bounds.x;
        output.origin_y = clipboard.payload.bounds.y;
        output.width = width;
        output.height = height;
        if output.pixels_rgba8.is_null() && output.pixel_capacity == 0 {
            output.row_stride_bytes = packed_stride;
            output.required_bytes = match packed_stride.checked_mul(u64::from(height)) {
                Some(bytes) => bytes,
                None => return fail(INKPOD_STATUS_INVALID_STATE, "clipboard bytes overflow"),
            };
            return INKPOD_STATUS_OK;
        }
        let stride = if output.row_stride_bytes == 0 {
            packed_stride
        } else {
            output.row_stride_bytes
        };
        if stride < packed_stride {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard row stride is too small",
            );
        }
        let required = match stride.checked_mul(u64::from(height)) {
            Some(bytes) => bytes,
            None => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "clipboard bytes overflow"),
        };
        output.required_bytes = required;
        output.row_stride_bytes = stride;
        if output.pixels_rgba8.is_null() || output.pixel_capacity < required {
            return INKPOD_STATUS_BUFFER_TOO_SMALL;
        }
        let required = match usize::try_from(required) {
            Ok(required) => required,
            Err(_) => return INKPOD_STATUS_BUFFER_TOO_SMALL,
        };
        // SAFETY: Caller advertises a writable output region of `required` bytes.
        let pixels = unsafe { slice::from_raw_parts_mut(output.pixels_rgba8, required) };
        pixels.fill(0);
        for pixel in &plane.pixels {
            let relative_x = i64::from(pixel.x) - i64::from(output.origin_x);
            let relative_y = i64::from(pixel.y) - i64::from(output.origin_y);
            if relative_x < 0
                || relative_y < 0
                || relative_x >= i64::from(width)
                || relative_y >= i64::from(height)
            {
                continue;
            }
            let offset = match u64::try_from(relative_y)
                .ok()
                .and_then(|y| y.checked_mul(stride))
                .and_then(|row| {
                    u64::try_from(relative_x)
                        .ok()
                        .and_then(|x| x.checked_mul(4))
                        .and_then(|column| row.checked_add(column))
                })
                .and_then(|offset| usize::try_from(offset).ok())
            {
                Some(offset) if offset + 4 <= pixels.len() => offset,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_STATE,
                        "clipboard pixel offset overflows",
                    );
                }
            };
            pixels[offset..offset + 4].copy_from_slice(&clipboard_pixel_rgba8(pixel.value));
        }
        INKPOD_STATUS_OK
    })
}

/// Creates a Rust-owned typed clipboard from caller-owned straight RGBA8.
///
/// # Safety
/// `input` and its pixel range must be readable for the call. `out_clipboard`
/// must be writable storage that does not already own a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_clipboard_create_rgba8(
    input: *const InkpodClipboardRgbaInput,
    out_clipboard: *mut *mut InkpodClipboard,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_clipboard.is_null() || !is_aligned(out_clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard output is null or misaligned",
            );
        }
        // SAFETY: Writable owner storage is required by contract.
        unsafe { out_clipboard.write(ptr::null_mut()) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodClipboardRgbaInput") } {
            return status;
        }
        // SAFETY: Complete input record was validated above.
        let input = unsafe { &*input };
        if input.reserved != 0 || input.width == 0 || input.height == 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard raster metadata is invalid",
            );
        }
        let pixel_count = match u64::from(input.width).checked_mul(u64::from(input.height)) {
            Some(count) if count <= 16_777_216 => count,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard raster exceeds work bound",
                );
            }
        };
        let packed_stride = u64::from(input.width) * 4;
        if input.row_stride_bytes < packed_stride {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard input stride is too small",
            );
        }
        let required = match input.row_stride_bytes.checked_mul(u64::from(input.height)) {
            Some(required) => required,
            None => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard input bytes overflow",
                );
            }
        };
        if input.pixels_rgba8.is_null() || input.pixel_bytes < required {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard input pixels are incomplete",
            );
        }
        let required = match usize::try_from(required) {
            Ok(required) => required,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard input is too large",
                );
            }
        };
        // SAFETY: Caller advertises a readable range covering `required` bytes.
        let source = unsafe { slice::from_raw_parts(input.pixels_rgba8, required) };
        let mut pixels = Vec::with_capacity(pixel_count as usize);
        for y in 0..input.height {
            for x in 0..input.width {
                let offset = (u64::from(y) * input.row_stride_bytes + u64::from(x) * 4) as usize;
                let rgba = [
                    source[offset],
                    source[offset + 1],
                    source[offset + 2],
                    source[offset + 3],
                ];
                if rgba != [0; 4] {
                    let pixel_x = match i64::from(input.origin_x).checked_add(i64::from(x)) {
                        Some(value) => value,
                        None => {
                            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "clipboard X overflows");
                        }
                    };
                    let pixel_y = match i64::from(input.origin_y).checked_add(i64::from(y)) {
                        Some(value) => value,
                        None => {
                            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "clipboard Y overflows");
                        }
                    };
                    let (pixel_x, pixel_y) = match (i32::try_from(pixel_x), i32::try_from(pixel_y))
                    {
                        (Ok(x), Ok(y)) => (x, y),
                        _ => {
                            return fail(
                                INKPOD_STATUS_INVALID_ARGUMENT,
                                "clipboard coordinate overflows",
                            );
                        }
                    };
                    pixels.push(ClipboardPixel {
                        x: pixel_x,
                        y: pixel_y,
                        value: PixelValue::Rgba(rgba),
                    });
                }
            }
        }
        let width = match i32::try_from(input.width) {
            Ok(width) => width,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard width exceeds i32",
                );
            }
        };
        let height = match i32::try_from(input.height) {
            Ok(height) => height,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "clipboard height exceeds i32",
                );
            }
        };
        let payload = ClipboardPayload {
            source_document_uuid: 1,
            bounds: RectI32 {
                x: input.origin_x,
                y: input.origin_y,
                width,
                height,
            },
            planes: vec![ClipboardPlane {
                kind: PlaneType::Raster,
                pixel_format: PixelFormat::StraightRgba8,
                origin_x: input.origin_x,
                origin_y: input.origin_y,
                pixels,
            }],
        };
        let clipboard = Box::new(InkpodClipboard { payload });
        // SAFETY: Output storage receives one unique Rust Box owner.
        unsafe { out_clipboard.write(Box::into_raw(clipboard)) };
        INKPOD_STATUS_OK
    })
}

/// Releases one Rust-owned clipboard handle and nulls caller storage.
///
/// # Safety
/// `clipboard` must be writable storage containing either null or exactly one
/// live handle previously returned by `inkpod_core_clipboard_copy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_clipboard_release(clipboard: *mut *mut InkpodClipboard) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if clipboard.is_null() || !is_aligned(clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller contract provides readable/writable owner storage.
        let handle = unsafe { clipboard.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "clipboard handle is misaligned",
            );
        }
        // SAFETY: Null first, then consume the unique Box owner exactly once.
        unsafe { clipboard.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Starts a coordinate-preserving floating paste.
///
/// # Safety
/// `core` must be live on its owner thread and `clipboard` must remain a live,
/// immutable clipboard handle for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_paste_begin(
    core: *mut InkpodCore,
    clipboard: *const InkpodClipboard,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || clipboard.is_null() || !is_aligned(clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "paste pointer is null or misaligned",
            );
        }
        // SAFETY: Live handles are required by the exported contract.
        let core = unsafe { &mut *core };
        let clipboard = unsafe { &*clipboard };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.begin_paste(&clipboard.payload) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Starts floating paste with explicit compatible or active-plane conversion routing.
///
/// # Safety
/// `core` and `clipboard` must remain live and aligned for the call, on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_paste_begin_mode(
    core: *mut InkpodCore,
    clipboard: *const InkpodClipboard,
    mode: u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || clipboard.is_null() || !is_aligned(clipboard) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "paste pointer is null or misaligned",
            );
        }
        // SAFETY: Live handles are required by the exported contract.
        let core = unsafe { &mut *core };
        let clipboard = unsafe { &*clipboard };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let result = match mode {
            1 => core.core.begin_paste(&clipboard.payload),
            2 => core
                .core
                .begin_paste_to_active_converted(&clipboard.payload),
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "paste mode is not defined"),
        };
        match result {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces the current floating paste transform.
///
/// # Safety
/// `core` must be live on its owner thread and `input` must expose a complete,
/// readable record that does not overlap Core storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_floating_transform(
    core: *mut InkpodCore,
    input: *const InkpodFloatingTransform,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structure exposes a readable size prefix.
        if let Err(status) = unsafe { validate_struct(input, "InkpodFloatingTransform") } {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.reserved != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "floating transform reserved value is not zero",
            );
        }
        match core.core.set_floating_transform(FloatingTransform {
            translate_x: input.translate_x,
            translate_y: input.translate_y,
            scale_x: input.scale_x,
            scale_y: input.scale_y,
            rotation_degrees: input.rotation_degrees,
        }) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits the current floating paste as one history transaction.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_floating_commit(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.commit_floating() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Cancels the current floating paste without editing the document.
///
/// # Safety
/// `core` must be a live handle used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_floating_cancel(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        core.core.cancel_floating();
        INKPOD_STATUS_OK
    })
}

/// Clears selected content from the active editable plane as one history transaction.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must be a complete writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_clear_selected_content(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.clear_selected_content() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Mirrors persistent document content as one history transaction.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_mirror_document(
    core: *mut InkpodCore,
    axis: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let axis = match axis {
            1 => MirrorAxis::Horizontal,
            2 => MirrorAxis::Vertical,
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "mirror axis is not defined"),
        };
        match core.core.mirror_document(axis) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Rotates persistent document content and metadata as one history transaction.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_rotate_document(
    core: *mut InkpodCore,
    direction: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let direction = match direction {
            1 => RotateDirection::Left90,
            2 => RotateDirection::Right90,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "rotate direction is not defined",
                );
            }
        };
        match core.core.rotate_document(direction) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Resizes/repositions or nearest-neighbor resamples persistent document data.
///
/// # Safety
/// `core`, `input`, and `result` must be complete, live, aligned,
/// non-overlapping records used on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_resize_document(
    core: *mut InkpodCore,
    input: *const InkpodDocumentResizeInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodDocumentResizeInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete records were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.flags & !1 != 0 {
            return fail(INKPOD_STATUS_UNSUPPORTED, "resize flags are not supported");
        }
        let anchor = match input.anchor {
            1 => ResizeAnchor::TopLeft,
            2 => ResizeAnchor::TopRight,
            3 => ResizeAnchor::Center,
            4 => ResizeAnchor::BottomLeft,
            5 => ResizeAnchor::BottomRight,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "resize anchor is not defined",
                );
            }
        };
        match core.core.resize_document(DocumentResize {
            width: input.width,
            height: input.height,
            dpi_x_milli: input.dpi_x_milli,
            dpi_y_milli: input.dpi_y_milli,
            resample: input.flags & 1 != 0,
            anchor,
        }) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Creates a secondary logical view of the current document.
///
/// # Safety
/// `core` must be live on its owner thread and `out_view_id` must be writable,
/// non-overlapping storage for one identifier.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_view_create(
    core: *mut InkpodCore,
    out_view_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_view_id.is_null() || !is_aligned(out_view_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "view create pointer is null or misaligned",
            );
        }
        // SAFETY: Live core and writable ID storage are required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.create_view() {
            Ok(id) => {
                // SAFETY: out_view_id is writable by contract.
                unsafe { out_view_id.write(id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a logical view command to one secondary view only.
///
/// # Safety
/// `core` must be live on its owner thread and `input` must expose a complete,
/// readable, non-overlapping record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_view_apply(
    core: *mut InkpodCore,
    view_id: u64,
    input: *const InkpodViewInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodViewInput") } {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let command = match parse_view_command(&core.core, input) {
            Ok(command) => command,
            Err(status) => return status,
        };
        match core.core.apply_view_for(view_id, command) {
            Ok(_) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Closes one secondary logical view.
///
/// # Safety
/// `core` must be a live handle used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_view_close(core: *mut InkpodCore, view_id: u64) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A live owner-thread handle is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.close_view(view_id) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Builds one immutable snapshot using a secondary view transform.
///
/// # Safety
/// `core` must be live on its owner thread, `options` must be a complete readable
/// record, and `out_snapshot` must be writable result storage that does not
/// currently contain a live snapshot handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_build_snapshot_for_view(
    core: *mut InkpodCore,
    view_id: u64,
    options: *const InkpodSnapshotOptions,
    out_snapshot: *mut *mut InkpodSnapshot,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_snapshot.is_null()
            || !is_aligned(out_snapshot)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "multi-view snapshot pointer is invalid",
            );
        }
        // SAFETY: Public structure exposes a readable size prefix.
        if let Err(status) = unsafe { validate_struct(options, "InkpodSnapshotOptions") } {
            return status;
        }
        // SAFETY: Caller provides writable output handle storage.
        unsafe { out_snapshot.write(ptr::null_mut()) };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let options = unsafe { &*options };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "snapshot options contain unsupported values",
            );
        }
        match core.core.build_snapshot_for(view_id) {
            Ok(snapshot) => {
                // SAFETY: Output storage receives exactly one Rust Box owner.
                unsafe { out_snapshot.write(Box::into_raw(snapshot_handle(snapshot))) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Inverts, expands, or shrinks the persistent selection mask.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_selection_adjust(
    core: *mut InkpodCore,
    operation: u32,
    pixels: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let operation = match operation {
            INKPOD_SELECTION_ADJUST_INVERT => core.core.invert_selection(),
            INKPOD_SELECTION_ADJUST_EXPAND => match i32::try_from(pixels) {
                Ok(pixels) => core.core.resize_selection(pixels),
                Err(_) => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "expand count is not representable",
                    );
                }
            },
            INKPOD_SELECTION_ADJUST_SHRINK => match i32::try_from(pixels) {
                Ok(pixels) => core.core.resize_selection(-pixels),
                Err(_) => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "shrink count is not representable",
                    );
                }
            },
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection adjustment is not defined",
                );
            }
        };
        match operation {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Clears the persistent selection mask as one undoable document transaction.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_selection_clear(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.clear_selection() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Creates a typed selection layer from the persistent selection mask.
///
/// # Safety
/// `core` must be live on its owner thread, the advertised UTF-8 name range must
/// be readable, and `result` plus `out_layer_id` must be writable and
/// non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_selection_to_layer(
    core: *mut InkpodCore,
    name_utf8: *const u8,
    name_bytes: u64,
    result: *mut InkpodDispatchResult,
    out_layer_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_layer_id.is_null()
            || !is_aligned(out_layer_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "selection-layer pointer is invalid",
            );
        }
        // SAFETY: Output prefix and name bytes follow the exported contract.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let name = match unsafe { name_from_utf8(name_utf8, name_bytes) } {
            Ok(name) => name,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.selection_to_layer(name) {
            Ok((outcome, id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: out_layer_id is writable by contract.
                unsafe { out_layer_id.write(id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Combines a typed selection layer with the persistent selection mask.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_selection_from_layer(
    core: *mut InkpodCore,
    layer_id: u64,
    operation: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let operation = match operation {
            INKPOD_SELECTION_LAYER_REPLACE => SelectionLayerOperation::Replace,
            INKPOD_SELECTION_LAYER_ADD => SelectionLayerOperation::Add,
            INKPOD_SELECTION_LAYER_SUBTRACT => SelectionLayerOperation::Subtract,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "selection-layer operation is not defined",
                );
            }
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.selection_from_layer(layer_id, operation) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Adds one persistent document guide.
///
/// # Safety
/// `core` must be live on its owner thread and `result` plus `out_guide_id` must
/// be complete writable records that do not overlap Core storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_guide_add(
    core: *mut InkpodCore,
    axis: u32,
    position: i32,
    result: *mut InkpodDispatchResult,
    out_guide_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_guide_id.is_null()
            || !is_aligned(out_guide_id)
        {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "guide pointer is invalid");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let axis = match axis {
            INKPOD_GUIDE_HORIZONTAL => GuideAxis::Horizontal,
            INKPOD_GUIDE_VERTICAL => GuideAxis::Vertical,
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "guide axis is not defined"),
        };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.add_guide(axis, position) {
            Ok((outcome, id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: out_guide_id is writable by contract.
                unsafe { out_guide_id.write(id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Moves one persistent document guide.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_guide_move(
    core: *mut InkpodCore,
    guide_id: u64,
    position: i32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.move_guide(guide_id, position) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Deletes one persistent document guide.
///
/// # Safety
/// `core` must be live on its owner thread and `result` must expose writable,
/// non-overlapping storage for a complete dispatch record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_guide_delete(
    core: *mut InkpodCore,
    guide_id: u64,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.delete_guide(guide_id) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces the persistent document grid configuration.
///
/// # Safety
/// `core` must be live on its owner thread, `input` must be fully readable, and
/// `result` must be writable and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_grid_set(
    core: *mut InkpodCore,
    input: *const InkpodGridInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodGridInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        if input.reserved != 0 || input.flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "grid input contains unsupported values",
            );
        }
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.set_grid(GridConfig {
            origin_x: input.origin_x,
            origin_y: input.origin_y,
            spacing_x: input.spacing_x,
            spacing_y: input.spacing_y,
            subdivisions: input.subdivisions,
        }) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples locator coordinates, selection bounds, and composite color.
///
/// # Safety
/// `core` must be live on its owner thread and `out_locator` must expose writable,
/// non-overlapping storage for a complete output record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_locator_sample(
    core: *mut InkpodCore,
    view_id: u64,
    device_x: f64,
    device_y: f64,
    out_locator: *mut InkpodLocatorOutput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(out_locator.cast_const(), "InkpodLocatorOutput") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_locator };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core
            .core
            .locator_sample((view_id != 0).then_some(view_id), device_x, device_y)
        {
            Ok(sample) => {
                output.flags = 0;
                output.document_x = sample.document_x;
                output.document_y = sample.document_y;
                output.selection = sample
                    .selection_bounds
                    .map_or(InkpodFrameRect::default(), frame_rect);
                if sample.selection_bounds.is_some() {
                    output.flags |= 1 << 0;
                }
                output.color = InkpodColorValue::default();
                output.color.struct_size = size_of::<InkpodColorValue>() as u32;
                if let Some(color) = sample.color {
                    if let Err(status) = write_color_value(&mut output.color, color) {
                        return status;
                    }
                    output.flags |= 1 << 1;
                }
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Rebinds one shortcut, replacing any conflicting binding deterministically.
///
/// # Safety
/// `core` must be a live handle used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_rebind(
    core: *mut InkpodCore,
    command_id: u32,
    virtual_key: u32,
    modifiers: u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.rebind_shortcut(ShortcutBinding {
            command_id,
            virtual_key,
            modifiers,
        }) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Resolves one normalized key chord through the current Core-owned bindings.
/// Zero means that the chord is currently unbound.
///
/// # Safety
/// `core` must be a live handle used on its owner thread and `out_command_id`
/// must point to writable `u32` storage that does not overlap the core.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_resolve(
    core: *mut InkpodCore,
    virtual_key: u32,
    modifiers: u32,
    out_command_id: *mut u32,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_command_id.is_null() || !is_aligned(out_command_id) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_command_id is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by contract.
        unsafe { out_command_id.write(0) };
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.resolve_shortcut(virtual_key, modifiers) {
            Ok(command_id) => {
                // SAFETY: Output storage was validated above.
                unsafe { out_command_id.write(command_id.unwrap_or(0)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Restores the built-in shortcut bindings.
///
/// # Safety
/// `core` must be a live handle used on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shortcut_reset(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        core.core.reset_shortcuts();
        INKPOD_STATUS_OK
    })
}

/// Replaces the active document with a bounded PNG/TIFF/TGA/BMP raster.
///
/// # Safety
/// `core` must be live on its owner thread, `bytes` must identify `byte_count`
/// readable bytes for this call, and `out_info` must be complete writable
/// storage. The UUID pair must not be zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_import_common_raster(
    core: *mut InkpodCore,
    format: u32,
    bytes: *const u8,
    byte_count: u64,
    document_uuid_high: u64,
    document_uuid_low: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_info.is_null() || !is_aligned(out_info) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster import pointer is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        if bytes.is_null() || byte_count == 0 || byte_count > MAX_COMMON_RASTER_BYTES as u64 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster input span is null, empty, or too large",
            );
        }
        let length = match usize::try_from(byte_count) {
            Ok(length) => length,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "common-raster input length is not representable",
                );
            }
        };
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let uuid = (u128::from(document_uuid_high) << 64) | u128::from(document_uuid_low);
        // SAFETY: The exported-function contract requires this bounded span readable.
        let bytes = unsafe { slice::from_raw_parts(bytes, length) };
        // SAFETY: Live owner-thread core and writable output were validated above.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.import_common_raster(format, bytes, uuid) {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Encodes the flattened visible document to a Rust-owned common-raster buffer.
///
/// # Safety
/// `core` must be live on its owner thread. `out_buffer` must be writable
/// storage containing null; the returned handle must be released by
/// `inkpod_byte_buffer_release`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_export_common_raster(
    core: *mut InkpodCore,
    format: u32,
    composite_white: u32,
    out_buffer: *mut *mut InkpodByteBuffer,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_buffer.is_null() || !is_aligned(out_buffer) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster export pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable owner storage.
        if !unsafe { out_buffer.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster output already owns a live buffer",
            );
        }
        if composite_white > 1 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "common-raster white-composite flag must be zero or one",
            );
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        // SAFETY: Live owner-thread core was validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.export_common_raster(format, composite_white != 0) {
            Ok(bytes) => {
                let handle = Box::new(InkpodByteBuffer {
                    bytes: bytes.into_boxed_slice(),
                });
                // SAFETY: Writable owner storage was validated and currently null.
                unsafe { out_buffer.write(Box::into_raw(handle)) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Borrows the immutable byte span owned by a common-raster buffer.
///
/// # Safety
/// `buffer` must be live. Both output pointers must be writable aligned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_byte_buffer_view(
    buffer: *const InkpodByteBuffer,
    out_bytes: *mut *const u8,
    out_byte_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if buffer.is_null()
            || !is_aligned(buffer)
            || out_bytes.is_null()
            || !is_aligned(out_bytes)
            || out_byte_count.is_null()
            || !is_aligned(out_byte_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "byte-buffer view pointer is null or misaligned",
            );
        }
        // SAFETY: Complete live input and writable outputs are required by contract.
        let buffer = unsafe { &*buffer };
        unsafe {
            out_bytes.write(buffer.bytes.as_ptr());
            out_byte_count.write(buffer.bytes.len() as u64);
        }
        INKPOD_STATUS_OK
    })
}

/// Releases one Rust-owned byte buffer and nulls caller storage.
///
/// # Safety
/// `buffer` must be writable storage containing null or one live handle returned
/// by `inkpod_core_export_common_raster`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_byte_buffer_release(buffer: *mut *mut InkpodByteBuffer) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if buffer.is_null() || !is_aligned(buffer) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "byte-buffer owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable unique owner storage.
        let handle = unsafe { buffer.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "byte-buffer handle is misaligned",
            );
        }
        // SAFETY: Null before consuming the unique Box owner exactly once.
        unsafe { buffer.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Copies one persistent light-table item into the active set.
///
/// # Safety
/// Core, input, nested raster/name storage, result, and item-ID output must be
/// complete, non-overlapping records valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_add_item(
    core: *mut InkpodCore,
    input: *const InkpodLightTableItemInput,
    result: *mut InkpodDispatchResult,
    out_item_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_item_id.is_null() || !is_aligned(out_item_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M4 light-table pointer is invalid",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodLightTableItemInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete input was validated above.
        let input = unsafe { &*input };
        if input.flags & !INKPOD_LIGHT_TABLE_ITEM_VISIBLE != 0 || input.reserved != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M4 light-table flags or reserved field is invalid",
            );
        }
        let display_mode = match input.display_mode {
            INKPOD_LIGHT_TABLE_COLOR => LightTableDisplayMode::Color,
            INKPOD_LIGHT_TABLE_MONOTONE => LightTableDisplayMode::Monotone,
            INKPOD_LIGHT_TABLE_HALFTONE => LightTableDisplayMode::Halftone,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "M4 light-table display mode is not defined",
                );
            }
        };
        let display_color = match unsafe { parse_color_value(&input.display_color) } {
            Ok(color) => color,
            Err(status) => return status,
        };
        let name = match unsafe { name_from_utf8(input.name_utf8, input.name_bytes) } {
            Ok(name) => name.to_owned(),
            Err(status) => return status,
        };
        let source = match unsafe { parse_m4_raster(&input.source) } {
            Ok(source) => source,
            Err(status) => return status,
        };
        let source = match LightTableSource::from_rgba_bytes(
            source.document_uuid,
            source.source_revision,
            source.reference_frame,
            RgbaRasterBytes {
                width: source.width,
                height: source.height,
                pixel_format: source.pixel_format,
                dpi_x_milli: source.dpi_x_milli,
                dpi_y_milli: source.dpi_y_milli,
                pixels: source.pixels,
            },
        ) {
            Ok(source) => source,
            Err(error) => return map_core_error(error),
        };
        // SAFETY: Live owner-thread Core and writable result are required.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.light_table_add_item(LightTableItemInput {
            name,
            source,
            visible: input.flags & INKPOD_LIGHT_TABLE_ITEM_VISIBLE != 0,
            opacity_milli: input.opacity_milli,
            display_mode,
            display_color,
            translate_x_milli: input.translate_x_milli,
            translate_y_milli: input.translate_y_milli,
            scale_x_milli: input.scale_x_milli,
            scale_y_milli: input.scale_y_milli,
            rotation_milli_degrees: input.rotation_milli_degrees,
        }) {
            Ok((outcome, item_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable output was validated above.
                unsafe { out_item_id.write(item_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Changes persistent active-set opacity as one document transaction.
///
/// # Safety
/// Core and result must be complete live records on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_set_global_opacity(
    core: *mut InkpodCore,
    opacity_milli: u32,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.light_table_set_global_opacity(opacity_milli) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a set/item management edit as one document history transaction.
///
/// # Safety
/// All pointers must be complete, aligned, live, non-overlapping owner-thread
/// records. A name span is required only for create/rename-set operations.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_edit(
    core: *mut InkpodCore,
    input: *const InkpodLightTableEdit,
    result: *mut InkpodDispatchResult,
    out_object_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_object_id.is_null()
            || !is_aligned(out_object_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table edit pointer is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodLightTableEdit") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete records were validated above.
        let input = unsafe { &*input };
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let edit_result = match input.operation {
            INKPOD_LIGHT_TABLE_CREATE_SET => {
                let name = match unsafe { name_from_utf8(input.name_utf8, input.name_bytes) } {
                    Ok(name) => name.to_owned(),
                    Err(status) => return status,
                };
                core.core.light_table_create_set(name)
            }
            INKPOD_LIGHT_TABLE_DUPLICATE_SET => {
                core.core.light_table_duplicate_set(input.object_id)
            }
            INKPOD_LIGHT_TABLE_DELETE_SET => core
                .core
                .light_table_delete_set(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_LIGHT_TABLE_RENAME_SET => {
                let name = match unsafe { name_from_utf8(input.name_utf8, input.name_bytes) } {
                    Ok(name) => name.to_owned(),
                    Err(status) => return status,
                };
                core.core
                    .light_table_rename_set(input.object_id, name)
                    .map(|outcome| (outcome, input.object_id))
            }
            INKPOD_LIGHT_TABLE_REORDER_SET => core
                .core
                .light_table_reorder_set(input.object_id, input.destination_index as usize)
                .map(|outcome| (outcome, input.object_id)),
            INKPOD_LIGHT_TABLE_SET_ACTIVE_OPERATION => core
                .core
                .light_table_set_active(input.object_id)
                .map(|outcome| (outcome, input.object_id)),
            INKPOD_LIGHT_TABLE_REMOVE_ITEM => core
                .core
                .light_table_remove_item(input.object_id)
                .map(|outcome| (outcome, 0)),
            INKPOD_LIGHT_TABLE_REORDER_ITEM => core
                .core
                .light_table_reorder_item(input.object_id, input.destination_index as usize)
                .map(|outcome| (outcome, input.object_id)),
            INKPOD_LIGHT_TABLE_UPDATE_ITEM => {
                if input.flags & !INKPOD_LIGHT_TABLE_ITEM_VISIBLE != 0 || input.reserved != 0 {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "light-table item property flags are invalid",
                    );
                }
                let display_mode = match input.display_mode {
                    INKPOD_LIGHT_TABLE_COLOR => LightTableDisplayMode::Color,
                    INKPOD_LIGHT_TABLE_MONOTONE => LightTableDisplayMode::Monotone,
                    INKPOD_LIGHT_TABLE_HALFTONE => LightTableDisplayMode::Halftone,
                    _ => {
                        return fail(
                            INKPOD_STATUS_INVALID_ARGUMENT,
                            "light-table display mode is not defined",
                        );
                    }
                };
                let display_color = match unsafe { parse_color_value(&input.display_color) } {
                    Ok(color) => color,
                    Err(status) => return status,
                };
                core.core
                    .light_table_update_item_properties(
                        input.object_id,
                        LightTableItemProperties {
                            visible: input.flags & INKPOD_LIGHT_TABLE_ITEM_VISIBLE != 0,
                            opacity_milli: input.opacity_milli,
                            display_mode,
                            display_color,
                            translate_x_milli: input.translate_x_milli,
                            translate_y_milli: input.translate_y_milli,
                            scale_x_milli: input.scale_x_milli,
                            scale_y_milli: input.scale_y_milli,
                            rotation_milli_degrees: input.rotation_milli_degrees,
                        },
                    )
                    .map(|outcome| (outcome, input.object_id))
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "light-table edit operation is not defined",
                );
            }
        };
        match edit_result {
            Ok((outcome, object_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable aligned storage was validated above.
                unsafe { out_object_id.write(object_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Returns one persistent light-table set by display order.
///
/// # Safety
/// Core and output must be complete live owner-thread records. The optional
/// UTF-8 name buffer remains caller-owned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_set_get(
    core: *mut InkpodCore,
    index: u32,
    output: *mut InkpodLightTableSetInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodLightTableSetInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let sets = match core.core.light_table_sets() {
            Ok(sets) => sets,
            Err(error) => return map_core_error(error),
        };
        let Some(set) = sets.get(index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table set index is outside bounds",
            );
        };
        output.flags = if set.active {
            INKPOD_LIGHT_TABLE_SET_ACTIVE
        } else {
            0
        };
        output.id = set.id;
        output.opacity_milli = set.global_opacity_milli;
        output.item_count = set.item_count as u32;
        output.name_bytes = set.name.len() as u64;
        if output.name_capacity == 0 {
            return if output.name_utf8.is_null() {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity set name buffer must be null",
                )
            };
        }
        if output.name_utf8.is_null() || output.name_capacity < output.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "light-table set name buffer is too small",
            );
        }
        // SAFETY: Caller advertises sufficient writable name capacity.
        unsafe { ptr::copy_nonoverlapping(set.name.as_ptr(), output.name_utf8, set.name.len()) };
        INKPOD_STATUS_OK
    })
}

/// Returns one item from the active light-table set by display order.
///
/// # Safety
/// Core and output must be complete live owner-thread records. The optional
/// UTF-8 name buffer remains caller-owned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_item_get(
    core: *mut InkpodCore,
    index: u32,
    output: *mut InkpodLightTableItemInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodLightTableItemInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let items = match core.core.light_table_items() {
            Ok(items) => items,
            Err(error) => return map_core_error(error),
        };
        let Some(item) = items.get(index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table item index is outside bounds",
            );
        };
        output.flags = if item.visible {
            INKPOD_LIGHT_TABLE_ITEM_VISIBLE
        } else {
            0
        };
        output.id = item.id;
        output.source_plane_id = item.source_plane_id;
        output.source_document_uuid_high = (item.source_document_uuid >> 64) as u64;
        output.source_document_uuid_low = item.source_document_uuid as u64;
        output.source_revision = item.source_revision;
        output.opacity_milli = item.opacity_milli;
        output.effective_opacity_milli = item.effective_opacity_milli;
        output.display_mode = match item.display_mode {
            LightTableDisplayMode::Color => INKPOD_LIGHT_TABLE_COLOR,
            LightTableDisplayMode::Monotone => INKPOD_LIGHT_TABLE_MONOTONE,
            LightTableDisplayMode::Halftone => INKPOD_LIGHT_TABLE_HALFTONE,
        };
        if let Err(status) = write_color_value(&mut output.display_color, item.display_color) {
            return status;
        }
        output.translate_x_milli = item.translate_x_milli;
        output.translate_y_milli = item.translate_y_milli;
        output.scale_x_milli = item.scale_x_milli;
        output.scale_y_milli = item.scale_y_milli;
        output.rotation_milli_degrees = item.rotation_milli_degrees;
        output.reserved = 0;
        output.name_bytes = item.name.len() as u64;
        if output.name_capacity == 0 {
            return if output.name_utf8.is_null() {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity item name buffer must be null",
                )
            };
        }
        if output.name_utf8.is_null() || output.name_capacity < output.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "light-table item name buffer is too small",
            );
        }
        // SAFETY: Caller advertises sufficient writable name capacity.
        unsafe { ptr::copy_nonoverlapping(item.name.as_ptr(), output.name_utf8, item.name.len()) };
        INKPOD_STATUS_OK
    })
}

/// Decodes and inserts one common-raster file into the active light-table set.
///
/// # Safety
/// Core/result/output must be valid owner-thread records and both byte/name
/// spans must remain readable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_add_common_raster(
    core: *mut InkpodCore,
    format: u32,
    bytes: *const u8,
    byte_count: u64,
    name_utf8: *const u8,
    name_bytes: u64,
    document_uuid_high: u64,
    document_uuid_low: u64,
    source_revision: u64,
    result: *mut InkpodDispatchResult,
    out_item_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_item_id.is_null()
            || !is_aligned(out_item_id)
            || bytes.is_null()
            || byte_count == 0
            || byte_count > MAX_COMMON_RASTER_BYTES as u64
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "light-table raster span is invalid",
            );
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let name = match unsafe { name_from_utf8(name_utf8, name_bytes) } {
            Ok(name) => name.to_owned(),
            Err(status) => return status,
        };
        let length = match usize::try_from(byte_count) {
            Ok(length) => length,
            Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "raster length overflows"),
        };
        // SAFETY: Caller provides this bounded readable byte span.
        let bytes = unsafe { slice::from_raw_parts(bytes, length) };
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let uuid = (u128::from(document_uuid_high) << 64) | u128::from(document_uuid_low);
        match core
            .core
            .light_table_add_common_raster(format, bytes, name, uuid, source_revision)
        {
            Ok((outcome, item_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable aligned output was validated above.
                unsafe { out_item_id.write(item_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces one item's source image while retaining its display properties.
///
/// # Safety
/// Core/result and the encoded byte span must be valid for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_reload_common_raster(
    core: *mut InkpodCore,
    item_id: u64,
    format: u32,
    bytes: *const u8,
    byte_count: u64,
    document_uuid_high: u64,
    document_uuid_low: u64,
    source_revision: u64,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || bytes.is_null()
            || byte_count == 0
            || byte_count > MAX_COMMON_RASTER_BYTES as u64
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "reload raster span is invalid",
            );
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let length = match usize::try_from(byte_count) {
            Ok(length) => length,
            Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "raster length overflows"),
        };
        // SAFETY: Caller provides this bounded readable byte span.
        let bytes = unsafe { slice::from_raw_parts(bytes, length) };
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let uuid = (u128::from(document_uuid_high) << 64) | u128::from(document_uuid_low);
        match core.core.light_table_reload_common_raster(
            item_id,
            format,
            bytes,
            uuid,
            source_revision,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples the transformed topmost light-table item in document coordinates.
///
/// # Safety
/// Core and output must be complete live records on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_sample(
    core: *mut InkpodCore,
    x: u32,
    y: u32,
    out_color: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_color.cast_const(), "InkpodColorValue") }
        {
            return status;
        }
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_color };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.light_table_sample(x, y) {
            Ok(color) => match write_color_value(output, color) {
                Ok(()) => INKPOD_STATUS_OK,
                Err(status) => status,
            },
            Err(error) => map_core_error(error),
        }
    })
}

/// Swaps the active edit image with one light-table item after dirty checking.
///
/// # Safety
/// Core and document-info output must be complete live records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_light_table_swap(
    core: *mut InkpodCore,
    item_id: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.light_table_swap_with_active(item_id) {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies a bounded sequence-cell span and naturally sorts it in Core.
///
/// # Safety
/// Core, input, every strided cell record, name, and raster row must remain
/// complete and readable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_set(
    core: *mut InkpodCore,
    input: *const InkpodSequenceInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodSequenceInput") } {
            return status;
        }
        // SAFETY: Complete input was validated above.
        let input = unsafe { &*input };
        if input.reserved != 0
            || input.feature_flags != 0
            || input.cell_count == 0
            || input.cell_count > 10_000
            || input.cells.is_null()
            || !is_aligned(input.cells)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M4 sequence header is invalid",
            );
        }
        let count = match usize::try_from(input.cell_count) {
            Ok(count) => count,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "M4 sequence count is not representable",
                );
            }
        };
        let stride = match usize::try_from(input.cell_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodSequenceCellInput>()
                    && stride % align_of::<InkpodSequenceCellInput>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "M4 sequence stride is invalid",
                );
            }
        };
        let storage = count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodSequenceCellInput>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "M4 sequence span overflows");
        }
        let mut cells = Vec::with_capacity(count);
        let mut total_raster_bytes = 0_usize;
        for index in 0..count {
            // SAFETY: Checked span makes every record prefix readable.
            let pointer = unsafe {
                input
                    .cells
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodSequenceCellInput>()
            };
            let advertised = match unsafe { validate_struct(pointer, "InkpodSequenceCellInput") } {
                Ok(size) => size,
                Err(status) => return status,
            };
            if advertised as usize > stride {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "M4 sequence cell size exceeds its stride",
                );
            }
            // SAFETY: Complete record was validated above.
            let record = unsafe { &*pointer };
            if record.reserved != 0 {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "M4 sequence cell reserved field is not zero",
                );
            }
            let name = match unsafe { name_from_utf8(record.name_utf8, record.name_bytes) } {
                Ok(name) => name.to_owned(),
                Err(status) => return status,
            };
            let raster = match unsafe { parse_m4_raster(&record.source) } {
                Ok(raster) => raster,
                Err(status) => return status,
            };
            total_raster_bytes = match total_raster_bytes.checked_add(raster.pixels.len()) {
                Some(total) if total <= MAX_COMMON_RASTER_BYTES => total,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "M4 sequence raster bytes exceed their cumulative bound",
                    );
                }
            };
            let mut cell = match SequenceCellSource::from_rgba_bytes(
                name,
                raster.document_uuid,
                RgbaRasterBytes {
                    width: raster.width,
                    height: raster.height,
                    pixel_format: raster.pixel_format,
                    dpi_x_milli: raster.dpi_x_milli,
                    dpi_y_milli: raster.dpi_y_milli,
                    pixels: raster.pixels,
                },
            ) {
                Ok(cell) => cell,
                Err(error) => return map_core_error(error),
            };
            cell.frames.reference_frame = raster.reference_frame;
            cells.push(cell);
        }
        // SAFETY: Live owner-thread Core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.set_sequence(cells) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Decodes a bounded naturally sorted sequence of common-raster files.
///
/// # Safety
/// Core and every strided named-byte record/span must remain live and readable
/// for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_import_encoded(
    core: *mut InkpodCore,
    format: u32,
    files: *const InkpodNamedBytesInput,
    file_count: u64,
    file_stride_bytes: u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || files.is_null()
            || !is_aligned(files)
            || file_count == 0
            || file_count > 10_000
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "encoded sequence header is invalid",
            );
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let count = match usize::try_from(file_count) {
            Ok(count) => count,
            Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence count overflows"),
        };
        let stride = match usize::try_from(file_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodNamedBytesInput>()
                    && stride % align_of::<InkpodNamedBytesInput>() == 0 =>
            {
                stride
            }
            _ => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence stride is invalid"),
        };
        let storage = count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|offset| offset.checked_add(size_of::<InkpodNamedBytesInput>()));
        if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence record span overflows",
            );
        }
        let mut decoded = Vec::with_capacity(count);
        let mut total_bytes = 0_usize;
        for index in 0..count {
            // SAFETY: The checked strided span makes every record prefix readable.
            let pointer = unsafe {
                files
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodNamedBytesInput>()
            };
            let advertised = match unsafe { validate_struct(pointer, "InkpodNamedBytesInput") } {
                Ok(size) => size,
                Err(status) => return status,
            };
            if advertised as usize > stride {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence record exceeds stride",
                );
            }
            // SAFETY: Complete record was validated above.
            let record = unsafe { &*pointer };
            if record.reserved != 0
                || record.bytes.is_null()
                || record.byte_count == 0
                || record.byte_count > MAX_COMMON_RASTER_BYTES as u64
            {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "sequence file span is invalid",
                );
            }
            let name = match unsafe { name_from_utf8(record.name_utf8, record.name_bytes) } {
                Ok(name) => name.to_owned(),
                Err(status) => return status,
            };
            let length = match usize::try_from(record.byte_count) {
                Ok(length) => length,
                Err(_) => return fail(INKPOD_STATUS_INVALID_ARGUMENT, "file length overflows"),
            };
            total_bytes = match total_bytes.checked_add(length) {
                Some(total) if total <= MAX_COMMON_RASTER_BYTES => total,
                _ => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "sequence bytes exceed bound",
                    );
                }
            };
            // SAFETY: Caller advertises this complete bounded byte span.
            let bytes = unsafe { slice::from_raw_parts(record.bytes, length) }.to_vec();
            decoded.push((name, bytes));
        }
        // SAFETY: Live owner-thread core was validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.import_sequence(format, decoded) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Encodes the configured sequence into a Rust-owned immutable file collection.
///
/// # Safety
/// Core must be live on its owner thread and `out_sequence` must be writable
/// null owner storage released by `inkpod_encoded_sequence_release`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_export_encoded(
    core: *mut InkpodCore,
    format: u32,
    composite_white: u32,
    out_sequence: *mut *mut InkpodEncodedSequence,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_sequence.is_null()
            || !is_aligned(out_sequence)
            || composite_white > 1
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence export pointer is invalid",
            );
        }
        // SAFETY: Caller provides readable/writable owner storage.
        if !unsafe { out_sequence.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence output already owns data",
            );
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        // SAFETY: Live owner-thread core was validated above.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.export_sequence(format, composite_white != 0) {
            Ok(files) => {
                let files = files
                    .into_iter()
                    .map(|(name, bytes)| EncodedSequenceFile {
                        name: name.into_bytes().into_boxed_slice(),
                        bytes: bytes.into_boxed_slice(),
                    })
                    .collect();
                // SAFETY: Writable null owner storage was validated above.
                unsafe {
                    out_sequence.write(Box::into_raw(Box::new(InkpodEncodedSequence { files })))
                };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Returns the number of encoded files owned by a sequence handle.
///
/// # Safety
/// Handle must be live and output must be writable aligned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_encoded_sequence_count(
    sequence: *const InkpodEncodedSequence,
    out_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if sequence.is_null()
            || !is_aligned(sequence)
            || out_count.is_null()
            || !is_aligned(out_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence count pointer is invalid",
            );
        }
        // SAFETY: Live input and writable output are required by contract.
        unsafe { out_count.write((*sequence).files.len() as u64) };
        INKPOD_STATUS_OK
    })
}

/// Borrows one encoded sequence file name and data span until release.
///
/// # Safety
/// Handle must be live and all output pointers must be writable aligned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_encoded_sequence_get(
    sequence: *const InkpodEncodedSequence,
    index: u64,
    out_name: *mut *const u8,
    out_name_bytes: *mut u64,
    out_bytes: *mut *const u8,
    out_byte_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if sequence.is_null()
            || !is_aligned(sequence)
            || out_name.is_null()
            || !is_aligned(out_name)
            || out_name_bytes.is_null()
            || !is_aligned(out_name_bytes)
            || out_bytes.is_null()
            || !is_aligned(out_bytes)
            || out_byte_count.is_null()
            || !is_aligned(out_byte_count)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence view pointer is invalid",
            );
        }
        // SAFETY: Live handle was validated above.
        let sequence = unsafe { &*sequence };
        let Some(file) = sequence.files.get(index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence file index is outside bounds",
            );
        };
        // SAFETY: All output storage is writable and aligned by contract.
        unsafe {
            out_name.write(file.name.as_ptr());
            out_name_bytes.write(file.name.len() as u64);
            out_bytes.write(file.bytes.as_ptr());
            out_byte_count.write(file.bytes.len() as u64);
        }
        INKPOD_STATUS_OK
    })
}

/// Releases an encoded sequence handle and nulls caller storage.
///
/// # Safety
/// Owner storage must contain null or exactly one live handle from export.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_encoded_sequence_release(
    sequence: *mut *mut InkpodEncodedSequence,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if sequence.is_null() || !is_aligned(sequence) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence owner pointer is invalid",
            );
        }
        // SAFETY: Caller provides readable/writable unique owner storage.
        let handle = unsafe { sequence.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence handle is misaligned",
            );
        }
        // SAFETY: Null first, then consume the unique Box owner exactly once.
        unsafe { sequence.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Returns one naturally ordered sequence cell and deterministic thumbnail metadata.
///
/// # Safety
/// Core/output must be complete live owner-thread records and the optional name
/// buffer must be writable for its advertised capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_cell_get(
    core: *mut InkpodCore,
    index: u32,
    output: *mut InkpodSequenceCellInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodSequenceCellInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let cells: Vec<SequenceCellInfo> = match core.core.sequence_cells() {
            Ok(cells) => cells,
            Err(error) => return map_core_error(error),
        };
        let Some(cell) = cells.get(index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sequence cell index is outside bounds",
            );
        };
        output.flags = 0;
        output.sequence_index = u64::from(index);
        output.document_uuid_high = (cell.document_uuid >> 64) as u64;
        output.document_uuid_low = cell.document_uuid as u64;
        output.cell_number = cell.cell_number;
        output.width = cell.width;
        output.height = cell.height;
        output.thumbnail_width = cell.thumbnail.width;
        output.thumbnail_height = cell.thumbnail.height;
        output.reserved = 0;
        output.thumbnail_checksum = cell.thumbnail.checksum;
        output.name_bytes = cell.name.len() as u64;
        if output.name_capacity == 0 {
            return if output.name_utf8.is_null() {
                INKPOD_STATUS_OK
            } else {
                fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "zero-capacity name buffer must be null",
                )
            };
        }
        if output.name_utf8.is_null() || output.name_capacity < output.name_bytes {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "sequence name buffer is too small",
            );
        }
        // SAFETY: Caller advertises sufficient writable name capacity.
        unsafe { ptr::copy_nonoverlapping(cell.name.as_ptr(), output.name_utf8, cell.name.len()) };
        INKPOD_STATUS_OK
    })
}

/// Switches to a sequence cell by natural-order index without discarding dirty data.
///
/// # Safety
/// Core and document-info output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_activate(
    core: *mut InkpodCore,
    index: u32,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.sequence_activate(index as usize) {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Registers one sequence cell as the exact-depth subpalette source.
///
/// # Safety
/// Core must be live on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_subpalette_set(core: *mut InkpodCore, index: u32) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.set_subpalette_cell(index as usize) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Samples one exact-depth subpalette pixel.
///
/// # Safety
/// Core/output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_subpalette_sample(
    core: *mut InkpodCore,
    x: u32,
    y: u32,
    output: *mut InkpodColorValue,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(output.cast_const(), "InkpodColorValue") } {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *output };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.subpalette_sample(x, y) {
            Ok(color) => match write_color_value(output, color) {
                Ok(()) => INKPOD_STATUS_OK,
                Err(status) => status,
            },
            Err(error) => map_core_error(error),
        }
    })
}

/// Switches to a previous/next naturally ordered sequence cell.
///
/// # Safety
/// Core and document-info output must be complete live records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_sequence_step(
    core: *mut InkpodCore,
    direction: u32,
    flags: u32,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if flags & !INKPOD_SEQUENCE_FLAG_LOOP != 0 {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "sequence flags are invalid");
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        let direction = match parse_sequence_direction(direction) {
            Ok(direction) => direction,
            Err(status) => return status,
        };
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core
            .core
            .sequence_step(direction, flags & INKPOD_SEQUENCE_FLAG_LOOP != 0)
        {
            Ok(info) => {
                write_document_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Starts motion check at the active or first sequence cell.
///
/// # Safety
/// Core, input, and frame output must be complete live records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_motion_check_start(
    core: *mut InkpodCore,
    input: *const InkpodMotionCheckInput,
    out_frame: *mut InkpodMotionFrame,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodMotionCheckInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(out_frame.cast_const(), "InkpodMotionFrame") }
        {
            return status;
        }
        // SAFETY: Complete records were validated above.
        let input = unsafe { &*input };
        if input.flags
            & !(INKPOD_MOTION_FLAG_LOOP
                | INKPOD_MOTION_FLAG_INCLUDE_SELECTION
                | INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE)
            != 0
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "motion-check flags are invalid",
            );
        }
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_frame };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.motion_check_start(MotionCheckConfig {
            fps: input.fps,
            loop_playback: input.flags & INKPOD_MOTION_FLAG_LOOP != 0,
            include_selection: input.flags & INKPOD_MOTION_FLAG_INCLUDE_SELECTION != 0,
            include_light_table: input.flags & INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE != 0,
        }) {
            Ok(frame) => {
                write_motion_frame(output, frame);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Steps an active motion-check session.
///
/// # Safety
/// Core and frame output must be complete live records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_motion_check_step(
    core: *mut InkpodCore,
    direction: u32,
    out_frame: *mut InkpodMotionFrame,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_frame.cast_const(), "InkpodMotionFrame") }
        {
            return status;
        }
        let direction = match parse_sequence_direction(direction) {
            Ok(direction) => direction,
            Err(status) => return status,
        };
        // SAFETY: Complete live records are required by contract.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_frame };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.motion_check_step(direction) {
            Ok(frame) => {
                write_motion_frame(output, frame);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Stops motion check. It is idempotent.
///
/// # Safety
/// Core must be live on its owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_motion_check_stop(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Live owner-thread Core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        core.core.motion_check_stop();
        INKPOD_STATUS_OK
    })
}

/// Toggles pause for an active motion-check session and returns its frame.
///
/// # Safety
/// Core/output must be complete live owner-thread records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_motion_check_toggle_pause(
    core: *mut InkpodCore,
    out_frame: *mut InkpodMotionFrame,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(out_frame.cast_const(), "InkpodMotionFrame") }
        {
            return status;
        }
        // SAFETY: Complete live records were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_frame };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.motion_check_toggle_pause() {
            Ok(frame) => {
                write_motion_frame(output, frame);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Creates a one-shot thread-safe M6 progress/cancellation task.
///
/// # Safety
/// `out_task` must be writable owner storage containing null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_m6_task_create(out_task: *mut *mut InkpodM6Task) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_task.is_null() || !is_aligned(out_task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M6 task owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable owner storage.
        if !unsafe { out_task.read() }.is_null() {
            return fail(
                INKPOD_STATUS_INVALID_STATE,
                "M6 task output already owns a handle",
            );
        }
        // SAFETY: The unique Rust owner is transferred to caller storage.
        unsafe { out_task.write(Box::into_raw(Box::new(InkpodM6Task::new()))) };
        INKPOD_STATUS_OK
    })
}

/// Queries an M6 task from any thread.
///
/// # Safety
/// `task` must be a live handle and `out_info` a complete writable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_m6_task_query(
    task: *const InkpodM6Task,
    out_info: *mut InkpodM6TaskInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M6 task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodM6TaskInfo") } {
            return status;
        }
        // SAFETY: Live task and writable complete output are required by contract.
        let task = unsafe { &*task };
        let output = unsafe { &mut *out_info };
        output.state = task.state.load(Ordering::Acquire);
        output.completed_work = task.completed_work.load(Ordering::Acquire);
        output.total_work = task.total_work.load(Ordering::Acquire);
        output.reserved = 0;
        INKPOD_STATUS_OK
    })
}

/// Requests cancellation from any thread. It is idempotent.
///
/// # Safety
/// `task` must be one live task handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_m6_task_cancel(task: *mut InkpodM6Task) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M6 task is null or misaligned",
            );
        }
        // SAFETY: A live task is required by contract and contains only atomics.
        let task = unsafe { &*task };
        task.cancelled.store(true, Ordering::Release);
        let _ = task.state.compare_exchange(
            INKPOD_M6_TASK_READY,
            INKPOD_M6_TASK_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        INKPOD_STATUS_OK
    })
}

/// Releases one Rust-owned M6 task and nulls caller storage.
///
/// # Safety
/// Storage must contain null or one live, no-longer-borrowed task owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_m6_task_release(task: *mut *mut InkpodM6Task) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M6 task owner pointer is null or misaligned",
            );
        }
        // SAFETY: Caller provides readable/writable owner storage.
        let handle = unsafe { task.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "M6 task handle is misaligned",
            );
        }
        // SAFETY: Nulling precedes consuming the unique Box owner.
        unsafe { task.write(ptr::null_mut()) };
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Begins a non-committing M6 filter preview from the current document state.
///
/// # Safety
/// Core/input/output must be complete, aligned, live, non-overlapping objects on
/// the Core owner thread. Any curve span is borrowed only for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_begin(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let filter = match unsafe { parse_filter_input(input) } {
            Ok(filter) => filter,
            Err(status) => return status,
        };
        match core.core.begin_filter_preview(input.plane_id, filter) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Begins a filter preview while publishing progress and honoring cancellation.
///
/// # Safety
/// The base preview requirements apply. `task` must be a live READY task kept
/// alive until this call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_begin_task(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    task: *mut InkpodM6Task,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or M6 task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let task = unsafe { &*task };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let filter = match unsafe { parse_filter_input(input) } {
            Ok(filter) => filter,
            Err(status) => return status,
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "M6 task is not READY");
        }
        let status = match core.core.begin_filter_preview_with_progress(
            input.plane_id,
            filter,
            |completed, total| task.progress(completed, total),
        ) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}

/// Recomputes an active M6 preview from its immutable base state.
///
/// # Safety
/// The same requirements as `inkpod_core_filter_preview_begin` apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_update(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let filter = match unsafe { parse_filter_input(input) } {
            Ok(filter) => filter,
            Err(status) => return status,
        };
        match core.core.update_filter_preview(input.plane_id, filter) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Updates a filter preview while publishing progress and honoring cancellation.
///
/// # Safety
/// The task and preview-begin-task requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_update_task(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    task: *mut InkpodM6Task,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or M6 task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let task = unsafe { &*task };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let filter = match unsafe { parse_filter_input(input) } {
            Ok(filter) => filter,
            Err(status) => return status,
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "M6 task is not READY");
        }
        let status = match core.core.update_filter_preview_with_progress(
            input.plane_id,
            filter,
            |completed, total| task.progress(completed, total),
        ) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}

/// Cancels a preview without changing the document or history.
///
/// # Safety
/// Core/output must be complete, aligned, live, and non-overlapping on the Core
/// owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_cancel(
    core: *mut InkpodCore,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.cancel_filter_preview() {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits the current preview as one history unit.
///
/// # Safety
/// Core/result must be complete, aligned, live, and non-overlapping on the Core
/// owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_preview_apply(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.apply_filter_preview() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies the last committed filter to another RGBA plane as one history unit.
///
/// # Safety
/// Core/result must satisfy the normal owner-thread dispatch contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_apply_last(
    core: *mut InkpodCore,
    plane_id: u64,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.apply_last_filter(plane_id) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies the last filter with progress/cancellation as one atomic history unit.
///
/// # Safety
/// Core/result follow the owner-thread contract. `task` must be a live READY
/// handle kept alive until this call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_filter_apply_last_task(
    core: *mut InkpodCore,
    plane_id: u64,
    task: *mut InkpodM6Task,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or M6 task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let task = unsafe { &*task };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "M6 task is not READY");
        }
        let status = match core
            .core
            .apply_last_filter_with_progress(plane_id, |completed, total| {
                task.progress(completed, total)
            }) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}

/// Creates a persisted, non-destructive adjustment layer. Name and curve
/// storage are copied before return.
///
/// # Safety
/// All advertised objects/spans must be complete, aligned, live, and
/// non-overlapping on the Core owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_adjustment_create(
    core: *mut InkpodCore,
    input: *const InkpodFilterInput,
    name_utf8: *const u8,
    name_length: u64,
    result: *mut InkpodDispatchResult,
    out_layer_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_layer_id.is_null()
            || !is_aligned(out_layer_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "adjustment core or layer output is null or misaligned",
            );
        }
        // SAFETY: Writable output storage is required by contract.
        unsafe { out_layer_id.write(0) };
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        if name_utf8.is_null()
            || name_length == 0
            || name_length > 1_024
            || usize::try_from(name_length).is_err()
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "adjustment name span is invalid",
            );
        }
        // SAFETY: The caller advertises a bounded readable byte span borrowed
        // only for this call.
        let name_bytes = unsafe { slice::from_raw_parts(name_utf8, name_length as usize) };
        let name = match std::str::from_utf8(name_bytes) {
            Ok(name) => name,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "adjustment name is not UTF-8",
                );
            }
        };
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let adjustment = match unsafe { parse_filter_input(input) }.and_then(filter_to_adjustment) {
            Ok(adjustment) => adjustment,
            Err(status) => return status,
        };
        match core.core.create_adjustment_layer(name, adjustment) {
            Ok((outcome, layer_id)) => {
                write_dispatch_result(result, outcome);
                // SAFETY: Writable aligned storage was validated above.
                unsafe { out_layer_id.write(layer_id) };
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces one persisted adjustment parameter record as one Undo unit.
///
/// # Safety
/// All objects and optional curve storage follow the owner-thread, alignment,
/// non-overlap, and per-call borrowing contract used by adjustment creation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_adjustment_update(
    core: *mut InkpodCore,
    layer_id: u64,
    input: *const InkpodFilterInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodFilterInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let adjustment = match unsafe { parse_filter_input(input) }.and_then(filter_to_adjustment) {
            Ok(adjustment) => adjustment,
            Err(status) => return status,
        };
        match core.core.update_adjustment_layer(layer_id, adjustment) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one copied linear/radial multi-stop gradient as one Undo unit.
///
/// # Safety
/// Core/input/result and every advertised stop/color record must be complete,
/// aligned, readable, non-overlapping, and live for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_gradient(
    core: *mut InkpodCore,
    input: *const InkpodGradientInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodGradientInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let gradient = match unsafe { parse_gradient_input(input) } {
            Ok(gradient) => gradient,
            Err(status) => return status,
        };
        match core.core.apply_gradient_to_plane(input.plane_id, &gradient) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one copied airbrush dab as one Undo unit.
///
/// # Safety
/// Core/input/result must satisfy the normal owner-thread ABI contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_airbrush(
    core: *mut InkpodCore,
    input: *const InkpodAirbrushInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodAirbrushInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let stroke = match unsafe { parse_airbrush_input(input) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core.core.apply_airbrush_to_plane(input.plane_id, stroke) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a copied, pressure-aware airbrush gesture as one Undo unit.
///
/// # Safety
/// Core/input/result and the advertised sample span must be complete and live
/// for this owner-thread call. No borrowed pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_airbrush_gesture(
    core: *mut InkpodCore,
    input: *const InkpodAirbrushGestureInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodAirbrushGestureInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete records and borrowed spans are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags
            & !(INKPOD_EFFECT_FLAG_PRESSURE_SIZE | INKPOD_EFFECT_FLAG_PRESSURE_OPACITY)
            != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "airbrush gesture contains unsupported flags",
            );
        }
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let color = match unsafe { parse_color_value(&input.color) } {
            Ok(value) => match value.rgba16() {
                Some(value) => value,
                None => {
                    return fail(
                        INKPOD_STATUS_INVALID_ARGUMENT,
                        "airbrush gesture color must be RGBA",
                    );
                }
            },
            Err(status) => return status,
        };
        let gesture = AirbrushGesture {
            samples: Vec::new(),
            radius_milli: input.radius_milli,
            hardness_milli: input.hardness_milli,
            spacing_milli: input.spacing_milli,
            opacity_milli: input.opacity_milli,
            fade_milli: input.fade_milli,
            pressure_size: input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_SIZE != 0,
            pressure_opacity: input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_OPACITY != 0,
            continuous_dabs: input.continuous_dabs,
            color,
        };
        match core.core.apply_airbrush_gesture_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            &samples,
            gesture,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies the copied boundary-color airbrush effect as one Undo unit.
///
/// # Safety
/// Core/input/result and every advertised color record follow the normal
/// owner-thread span contract and are borrowed only for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_boundary_airbrush(
    core: *mut InkpodCore,
    input: *const InkpodBoundaryAirbrushInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodBoundaryAirbrushInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let effect = match unsafe { parse_boundary_airbrush_input(input) } {
            Ok(effect) => effect,
            Err(status) => return status,
        };
        match core
            .core
            .apply_boundary_airbrush_to_plane(input.plane_id, &effect)
        {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a bounded Gaussian blur effect as one Undo unit.
///
/// # Safety
/// Core/input/result must satisfy the normal owner-thread ABI contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_blur(
    core: *mut InkpodCore,
    input: *const InkpodBlurEffectInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodBlurEffectInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE
            || input.reserved != 0
            || input.reserved_2 != 0
            || input.reserved_3 != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "blur-effect input contains unsupported flags or reserved values",
            );
        }
        match core
            .core
            .apply_blur_to_plane(input.plane_id, input.radius, input.strength_milli)
        {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one bounded offset stamp from the immutable source plane state.
///
/// # Safety
/// Core/input/result must satisfy the normal owner-thread ABI contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_stamp(
    core: *mut InkpodCore,
    input: *const InkpodStampInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodStampInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE
            || input.reserved != 0
            || input.reserved_2 != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stamp input contains unsupported flags or reserved values",
            );
        }
        let stamp = Stamp {
            source_x: input.source_x,
            source_y: input.source_y,
            destination_x: input.destination_x,
            destination_y: input.destination_y,
            width: input.width,
            height: input.height,
            opacity_milli: input.opacity_milli,
        };
        match core.core.apply_stamp_to_plane(input.plane_id, stamp) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a copied pressure-aware clone-stamp gesture as one Undo unit.
///
/// # Safety
/// The airbrush-gesture safety requirements apply, including the embedded source.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_stamp_gesture(
    core: *mut InkpodCore,
    input: *const InkpodStampGestureInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodStampGestureInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live records and spans are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.reserved != 0
            || input.feature_flags
                & !(INKPOD_EFFECT_FLAG_PRESSURE_SIZE | INKPOD_EFFECT_FLAG_PRESSURE_OPACITY)
                != 0
        {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stamp gesture contains unsupported flags",
            );
        }
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let source = match unsafe {
            parse_stroke_samples(&input.source, 1, size_of::<InkpodStrokeSample>() as u64)
        } {
            Ok(mut value) => value.remove(0),
            Err(status) => return status,
        };
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let gesture = StampGesture {
            source_x_milli: 0,
            source_y_milli: 0,
            samples: Vec::new(),
            radius_milli: input.radius_milli,
            hardness_milli: input.hardness_milli,
            spacing_milli: input.spacing_milli,
            opacity_milli: input.opacity_milli,
            shape: match input.shape {
                INKPOD_STAMP_ROUND => StampShape::Round,
                INKPOD_STAMP_SQUARE => StampShape::Square,
                _ => {
                    return fail(INKPOD_STATUS_INVALID_ARGUMENT, "stamp shape is unknown");
                }
            },
            pressure_size: input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_SIZE != 0,
            pressure_opacity: input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_OPACITY != 0,
        };
        match core.core.apply_stamp_gesture_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            source,
            &samples,
            gesture,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies the blur tool inside a copied pen/rectangle/polyline/lasso region.
///
/// # Safety
/// Core/input/result and the embedded region span must remain live for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_effect_blur_tool(
    core: *mut InkpodCore,
    input: *const InkpodBlurToolInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodBlurToolInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live records and embedded spans are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags & !INKPOD_EFFECT_FLAG_PRESSURE_SIZE != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "blur tool contains unsupported fields",
            );
        }
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let kind = match parse_effect_region_kind(input.shape) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(value) => value,
            Err(status) => return status,
        };
        match core.core.apply_blur_tool_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            kind,
            &samples,
            input.diameter,
            input.feature_flags & INKPOD_EFFECT_FLAG_PRESSURE_SIZE != 0,
            input.radius,
            input.strength_milli,
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Runs bounded dust removal with progress/cancellation and atomic commit.
///
/// # Safety
/// The Core/input/result records, optional embedded region span, and READY task
/// must remain live until this owner-thread call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_dust_remove(
    core: *mut InkpodCore,
    input: *const InkpodDustInput,
    task: *mut InkpodM6Task,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or M6 task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodDustInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live records and task are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let task = unsafe { &*task };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || input.use_region > 1 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "dust-removal input contains unsupported fields",
            );
        }
        let mode = match input.mode {
            INKPOD_DUST_REMOVE_FOREGROUND => DustMode::RemoveForeground,
            INKPOD_DUST_FILL_TRANSPARENT_HOLES => DustMode::FillTransparentHoles,
            INKPOD_DUST_REPLACE_COLOR_OUTLIERS => DustMode::ReplaceColorOutliers,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "dust-removal mode is unknown",
                );
            }
        };
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let kind = if input.use_region != 0 {
            match parse_effect_region_kind(input.shape) {
                Ok(value) => Some(value),
                Err(status) => return status,
            }
        } else {
            None
        };
        let samples = if input.use_region != 0 {
            match unsafe {
                parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
            } {
                Ok(value) => value,
                Err(status) => return status,
            }
        } else {
            if input.sample_count != 0 || !input.samples.is_null() || input.sample_stride_bytes != 0
            {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "full-image dust removal must not carry region samples",
                );
            }
            Vec::new()
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "M6 task is not READY");
        }
        let status = match core.core.apply_dust_removal_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            kind,
            &samples,
            input.diameter,
            DustRemoval {
                mode,
                maximum_pixels: input.maximum_pixels,
            },
            |completed, total| task.progress(completed, total),
        ) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}

/// Begins a non-committing dust-removal preview with progress/cancellation.
///
/// # Safety
/// The dust-remove safety requirements apply; output is a complete writable
/// preview-info record. Apply/cancel uses the filter-preview apply/cancel API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_dust_preview_begin(
    core: *mut InkpodCore,
    input: *const InkpodDustInput,
    task: *mut InkpodM6Task,
    out_info: *mut InkpodFilterPreviewInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || task.is_null() || !is_aligned(task) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core or M6 task is null or misaligned",
            );
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodDustInput") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodFilterPreviewInfo") }
        {
            return status;
        }
        // SAFETY: Complete live records and task are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let task = unsafe { &*task };
        let output = unsafe { &mut *out_info };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        if input.feature_flags != INKPOD_FEATURE_NONE || input.use_region > 1 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "dust-removal input contains unsupported fields",
            );
        }
        let mode = match input.mode {
            INKPOD_DUST_REMOVE_FOREGROUND => DustMode::RemoveForeground,
            INKPOD_DUST_FILL_TRANSPARENT_HOLES => DustMode::FillTransparentHoles,
            INKPOD_DUST_REPLACE_COLOR_OUTLIERS => DustMode::ReplaceColorOutliers,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "dust-removal mode is unknown",
                );
            }
        };
        let coordinate_space = match parse_coordinate_space(input.coordinate_space) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let kind = if input.use_region != 0 {
            match parse_effect_region_kind(input.shape) {
                Ok(value) => Some(value),
                Err(status) => return status,
            }
        } else {
            None
        };
        let samples = if input.use_region != 0 {
            match unsafe {
                parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
            } {
                Ok(value) => value,
                Err(status) => return status,
            }
        } else {
            if input.sample_count != 0 || !input.samples.is_null() || input.sample_stride_bytes != 0
            {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "full-image dust removal must not carry region samples",
                );
            }
            Vec::new()
        };
        if !task.begin() {
            return fail(INKPOD_STATUS_INVALID_STATE, "M6 task is not READY");
        }
        let status = match core.core.begin_dust_preview_for_view(
            input.view_id,
            coordinate_space,
            input.plane_id,
            kind,
            &samples,
            input.diameter,
            DustRemoval {
                mode,
                maximum_pixels: input.maximum_pixels,
            },
            |completed, total| task.progress(completed, total),
        ) {
            Ok(info) => {
                write_filter_preview_info(output, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        };
        task.finish(status);
        status
    })
}

/// Replaces only the target plane alpha from copied grayscale rows.
///
/// # Safety
/// Core/input/result and every advertised pixel row must be complete, readable,
/// non-overlapping, and live for this owner-thread call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_alpha_edit(
    core: *mut InkpodCore,
    input: *const InkpodAlphaEditInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodAlphaEditInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects were validated above.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let alpha = match unsafe { parse_alpha_edit_input(input) } {
            Ok(alpha) => alpha,
            Err(status) => return status,
        };
        match core.core.edit_plane_alpha(input.plane_id, &alpha) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a multi-stop gradient to alpha only, preserving every RGB channel.
///
/// # Safety
/// The gradient-effect safety requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_alpha_gradient(
    core: *mut InkpodCore,
    input: *const InkpodGradientInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if let Err(status) = unsafe { validate_struct(input, "InkpodGradientInput") } {
            return status;
        }
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete records and borrowed stop span are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let gradient = match unsafe { parse_gradient_input(input) } {
            Ok(value) => value,
            Err(status) => return status,
        };
        match core
            .core
            .apply_alpha_gradient_to_plane(input.plane_id, &gradient)
        {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> InkpodCoreConfig {
        InkpodCoreConfig {
            struct_size: size_of::<InkpodCoreConfig>() as u32,
            abi_version: INKPOD_ABI_VERSION,
            feature_flags: INKPOD_FEATURE_NONE,
        }
    }

    #[test]
    fn m4_light_table_sequence_motion_and_dirty_switch_abi_are_connected() {
        let mut core = ptr::null_mut();
        let config = config();
        // SAFETY: Test records remain live and non-overlapping for each call.
        unsafe {
            assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
            let options = InkpodCellCreateOptions {
                struct_size: size_of::<InkpodCellCreateOptions>() as u32,
                reserved: 0,
                feature_flags: 0,
                document_uuid_high: 1,
                document_uuid_low: 1,
                width: 8,
                height: 8,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
            };
            let mut document = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..InkpodDocumentInfo::default()
            };
            assert_eq!(
                inkpod_core_new_cell(core, &options, &mut document),
                INKPOD_STATUS_OK
            );

            let mut light_pixels = [0_u8; 4 * 20];
            light_pixels[2 * 20 + 2 * 4..2 * 20 + 2 * 4 + 4].copy_from_slice(&[90, 80, 70, 255]);
            let raster = InkpodM4RasterInput {
                struct_size: size_of::<InkpodM4RasterInput>() as u32,
                pixel_format: INKPOD_STORAGE_RGBA8,
                flags: 0,
                document_uuid_high: 2,
                document_uuid_low: 2,
                source_revision: 3,
                width: 4,
                height: 4,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
                reference_frame: InkpodFrameRect {
                    x: 2,
                    y: 2,
                    width: 4,
                    height: 4,
                },
                pixels: light_pixels.as_ptr(),
                pixel_bytes: light_pixels.len() as u64,
                row_stride_bytes: 20,
            };
            let name = b"reference";
            let item = InkpodLightTableItemInput {
                struct_size: size_of::<InkpodLightTableItemInput>() as u32,
                flags: INKPOD_LIGHT_TABLE_ITEM_VISIBLE,
                opacity_milli: 500,
                display_mode: INKPOD_LIGHT_TABLE_COLOR,
                display_color: InkpodColorValue {
                    struct_size: size_of::<InkpodColorValue>() as u32,
                    depth: INKPOD_COLOR_DEPTH_8,
                    red: 0,
                    green: 128,
                    blue: 255,
                    alpha: 255,
                },
                translate_x_milli: 0,
                translate_y_milli: 0,
                scale_x_milli: 1_000,
                scale_y_milli: 1_000,
                rotation_milli_degrees: 0,
                reserved: 0,
                name_utf8: name.as_ptr(),
                name_bytes: name.len() as u64,
                source: raster,
            };
            let mut dispatch = InkpodDispatchResult {
                struct_size: size_of::<InkpodDispatchResult>() as u32,
                reserved: 0,
                revision: 0,
                accepted_command_count: 0,
            };
            let mut item_id = 0;
            assert_eq!(
                inkpod_core_light_table_add_item(core, &item, &mut dispatch, &mut item_id),
                INKPOD_STATUS_OK
            );
            assert_ne!(item_id, 0);
            light_pixels.fill(0);
            assert_eq!(
                inkpod_core_light_table_set_global_opacity(core, 500, &mut dispatch),
                INKPOD_STATUS_OK
            );
            let mut sampled = InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                ..InkpodColorValue::default()
            };
            assert_eq!(
                inkpod_core_light_table_sample(core, 4, 4, &mut sampled),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                (sampled.red, sampled.green, sampled.blue, sampled.alpha),
                (90, 80, 70, 64)
            );
            let fill_color = InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_8,
                red: 200,
                green: 10,
                blue: 20,
                alpha: 255,
            };
            let fill = InkpodFillInput {
                struct_size: size_of::<InkpodFillInput>() as u32,
                operation: INKPOD_FILL_SEED,
                flags: INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY,
                seed_x: 0,
                seed_y: 0,
                color: fill_color,
                tolerance: 0,
                gap_close: 0,
                inclusion_mode: INKPOD_INCLUSION_NONE,
                selection: InkpodFrameRect::default(),
                inclusion_colors: ptr::null(),
                inclusion_color_count: 0,
                inclusion_color_stride_bytes: 0,
                extension_distance: 0,
                reserved: 0,
            };
            let mut fill_result = InkpodFillResult {
                struct_size: size_of::<InkpodFillResult>() as u32,
                ..InkpodFillResult::default()
            };
            assert_eq!(
                inkpod_core_apply_fill(core, &fill, &mut fill_result),
                INKPOD_STATUS_OK
            );
            assert_eq!(fill_result.changed_pixel_count, 63);
            assert_eq!(
                inkpod_core_light_table_sample(core, 4, 4, &mut sampled),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                (sampled.red, sampled.green, sampled.blue, sampled.alpha),
                (90, 80, 70, 64)
            );
            let mut sixteen_pixels = [0_u8; 8];
            for (index, channel) in [1_u16, 257, 32_769, 65_535].into_iter().enumerate() {
                sixteen_pixels[index * 2..index * 2 + 2].copy_from_slice(&channel.to_le_bytes());
            }
            let sixteen_name = b"rgba16";
            let sixteen_item = InkpodLightTableItemInput {
                struct_size: size_of::<InkpodLightTableItemInput>() as u32,
                flags: INKPOD_LIGHT_TABLE_ITEM_VISIBLE,
                opacity_milli: 1_000,
                display_mode: INKPOD_LIGHT_TABLE_COLOR,
                display_color: InkpodColorValue {
                    struct_size: size_of::<InkpodColorValue>() as u32,
                    depth: INKPOD_COLOR_DEPTH_16,
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: u16::MAX,
                },
                translate_x_milli: 0,
                translate_y_milli: 0,
                scale_x_milli: 1_000,
                scale_y_milli: 1_000,
                rotation_milli_degrees: 0,
                reserved: 0,
                name_utf8: sixteen_name.as_ptr(),
                name_bytes: sixteen_name.len() as u64,
                source: InkpodM4RasterInput {
                    struct_size: size_of::<InkpodM4RasterInput>() as u32,
                    pixel_format: INKPOD_STORAGE_RGBA16,
                    flags: 0,
                    document_uuid_high: 3,
                    document_uuid_low: 3,
                    source_revision: 1,
                    width: 1,
                    height: 1,
                    dpi_x_milli: 96_000,
                    dpi_y_milli: 96_000,
                    reference_frame: InkpodFrameRect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    pixels: sixteen_pixels.as_ptr(),
                    pixel_bytes: sixteen_pixels.len() as u64,
                    row_stride_bytes: 8,
                },
            };
            let mut sixteen_item_id = 0;
            assert_eq!(
                inkpod_core_light_table_add_item(
                    core,
                    &sixteen_item,
                    &mut dispatch,
                    &mut sixteen_item_id,
                ),
                INKPOD_STATUS_OK
            );
            assert_ne!(sixteen_item_id, 0);
            sixteen_pixels.fill(0);
            assert_eq!(
                inkpod_core_light_table_sample(core, 4, 4, &mut sampled),
                INKPOD_STATUS_OK
            );
            assert_eq!(sampled.depth, INKPOD_COLOR_DEPTH_16);
            assert_eq!(
                (sampled.red, sampled.green, sampled.blue, sampled.alpha),
                (1, 257, 32_769, 32_768)
            );
            assert_eq!(
                inkpod_core_light_table_swap(core, item_id, &mut document),
                INKPOD_STATUS_UNSAVED_CHANGES
            );

            let mut sequence_pixels_a = [1_u8, 2, 3, 255];
            let mut sequence_pixels_b = [4_u8, 5, 6, 255];
            let names = [b"cell10.png".as_slice(), b"cell2.png".as_slice()];
            let pixels = [sequence_pixels_a.as_slice(), sequence_pixels_b.as_slice()];
            let mut cells = Vec::new();
            for index in 0..2 {
                cells.push(InkpodSequenceCellInput {
                    struct_size: size_of::<InkpodSequenceCellInput>() as u32,
                    reserved: 0,
                    name_utf8: names[index].as_ptr(),
                    name_bytes: names[index].len() as u64,
                    source: InkpodM4RasterInput {
                        struct_size: size_of::<InkpodM4RasterInput>() as u32,
                        pixel_format: INKPOD_STORAGE_RGBA8,
                        flags: 0,
                        document_uuid_high: 5,
                        document_uuid_low: index as u64 + 1,
                        source_revision: 1,
                        width: 1,
                        height: 1,
                        dpi_x_milli: 96_000,
                        dpi_y_milli: 96_000,
                        reference_frame: InkpodFrameRect {
                            x: 0,
                            y: 0,
                            width: 1,
                            height: 1,
                        },
                        pixels: pixels[index].as_ptr(),
                        pixel_bytes: 4,
                        row_stride_bytes: 4,
                    },
                });
            }
            let sequence = InkpodSequenceInput {
                struct_size: size_of::<InkpodSequenceInput>() as u32,
                reserved: 0,
                feature_flags: 0,
                cells: cells.as_ptr(),
                cell_count: cells.len() as u64,
                cell_stride_bytes: size_of::<InkpodSequenceCellInput>() as u64,
            };
            assert_eq!(inkpod_core_sequence_set(core, &sequence), INKPOD_STATUS_OK);
            sequence_pixels_a.fill(0);
            sequence_pixels_b.fill(0);
            assert_eq!(
                inkpod_core_sequence_step(core, INKPOD_SEQUENCE_NEXT, 0, &mut document),
                INKPOD_STATUS_UNSAVED_CHANGES
            );
            let motion = InkpodMotionCheckInput {
                struct_size: size_of::<InkpodMotionCheckInput>() as u32,
                fps: 24,
                flags: INKPOD_MOTION_FLAG_LOOP,
            };
            let mut frame = InkpodMotionFrame {
                struct_size: size_of::<InkpodMotionFrame>() as u32,
                ..InkpodMotionFrame::default()
            };
            assert_eq!(
                inkpod_core_motion_check_start(core, &motion, &mut frame),
                INKPOD_STATUS_OK
            );
            assert_eq!(frame.cell_number, 2);
            assert_ne!(frame.thumbnail_checksum, 0);
            assert_eq!(
                inkpod_core_motion_check_step(core, INKPOD_SEQUENCE_NEXT, &mut frame),
                INKPOD_STATUS_OK
            );
            assert_eq!(frame.cell_number, 10);
            assert_eq!(inkpod_core_motion_check_stop(core), INKPOD_STATUS_OK);
            assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        }
    }

    #[test]
    fn m4_ffi_rejects_nested_raster_bounds_and_extreme_rotation_without_mutation() {
        let mut core = ptr::null_mut();
        let config = config();
        // SAFETY: Test records remain live and non-overlapping for every call.
        unsafe {
            assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
            let options = InkpodCellCreateOptions {
                struct_size: size_of::<InkpodCellCreateOptions>() as u32,
                reserved: 0,
                feature_flags: 0,
                document_uuid_high: 7,
                document_uuid_low: 7,
                width: 1,
                height: 1,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
            };
            let mut before = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..InkpodDocumentInfo::default()
            };
            assert_eq!(
                inkpod_core_new_cell(core, &options, &mut before),
                INKPOD_STATUS_OK
            );
            let pixel = [1_u8, 2, 3, 255];
            let name = b"invalid";
            let raster = InkpodM4RasterInput {
                struct_size: size_of::<InkpodM4RasterInput>() as u32,
                pixel_format: INKPOD_STORAGE_RGBA8,
                flags: 0,
                document_uuid_high: 8,
                document_uuid_low: 8,
                source_revision: 1,
                width: 1,
                height: 1,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
                reference_frame: InkpodFrameRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                pixels: pixel.as_ptr(),
                pixel_bytes: pixel.len() as u64,
                row_stride_bytes: 4,
            };
            let base_item = InkpodLightTableItemInput {
                struct_size: size_of::<InkpodLightTableItemInput>() as u32,
                flags: INKPOD_LIGHT_TABLE_ITEM_VISIBLE,
                opacity_milli: 1_000,
                display_mode: INKPOD_LIGHT_TABLE_COLOR,
                display_color: InkpodColorValue {
                    struct_size: size_of::<InkpodColorValue>() as u32,
                    depth: INKPOD_COLOR_DEPTH_8,
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                },
                translate_x_milli: 0,
                translate_y_milli: 0,
                scale_x_milli: 1_000,
                scale_y_milli: 1_000,
                rotation_milli_degrees: 0,
                reserved: 0,
                name_utf8: name.as_ptr(),
                name_bytes: name.len() as u64,
                source: raster,
            };
            let mut dispatch = InkpodDispatchResult {
                struct_size: size_of::<InkpodDispatchResult>() as u32,
                reserved: 0,
                revision: 0,
                accepted_command_count: 0,
            };
            let mut item_id = 0;

            let mut short_nested = base_item;
            short_nested.source.struct_size = size_of::<InkpodM4RasterInput>() as u32 - 1;
            assert_eq!(
                inkpod_core_light_table_add_item(core, &short_nested, &mut dispatch, &mut item_id,),
                INKPOD_STATUS_INCOMPATIBLE_ABI
            );

            let mut oversized = base_item;
            oversized.source.width = MAX_RASTER_DIMENSION + 1;
            assert_eq!(
                inkpod_core_light_table_add_item(core, &oversized, &mut dispatch, &mut item_id,),
                INKPOD_STATUS_INVALID_ARGUMENT
            );

            let mut invalid_reference = base_item;
            invalid_reference.source.reference_frame.width = 0;
            assert_eq!(
                inkpod_core_light_table_add_item(
                    core,
                    &invalid_reference,
                    &mut dispatch,
                    &mut item_id,
                ),
                INKPOD_STATUS_INVALID_ARGUMENT
            );

            let mut extreme_rotation = base_item;
            extreme_rotation.rotation_milli_degrees = i32::MIN;
            assert_eq!(
                inkpod_core_light_table_add_item(
                    core,
                    &extreme_rotation,
                    &mut dispatch,
                    &mut item_id,
                ),
                INKPOD_STATUS_INVALID_ARGUMENT
            );

            let mut after = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..InkpodDocumentInfo::default()
            };
            assert_eq!(
                inkpod_core_get_document_info(core, &mut after),
                INKPOD_STATUS_OK
            );
            assert_eq!(after.document_revision, before.document_revision);
            assert_eq!(after.flags, before.flags);
            assert_eq!(item_id, 0);
            assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        }
    }

    #[test]
    fn abi_001_lifecycle_and_double_release_are_safe() {
        let mut core = ptr::null_mut();
        // SAFETY: All pointers reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        assert!(!core.is_null());

        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
        };
        let mut snapshot = ptr::null_mut();
        // SAFETY: The core is live and outputs point to local storage.
        assert_eq!(
            unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
            INKPOD_STATUS_OK
        );

        let mut view = InkpodSnapshotView {
            struct_size: size_of::<InkpodSnapshotView>() as u32,
            abi_version: 0,
            feature_flags: u64::MAX,
            revision: u64::MAX,
            tiles: ptr::null(),
            tile_count: u64::MAX,
            tile_stride_bytes: 0,
        };
        // SAFETY: Snapshot and output view are live for this call.
        assert_eq!(
            unsafe { inkpod_snapshot_get_view(snapshot, &mut view) },
            INKPOD_STATUS_OK
        );
        assert_eq!(view.abi_version, INKPOD_ABI_VERSION);
        assert_eq!(view.revision, 0);
        assert!(view.tiles.is_null());
        assert_eq!(view.tile_count, 0);
        assert_eq!(
            view.tile_stride_bytes,
            size_of::<InkpodSnapshotTile>() as u64
        );

        // SAFETY: Owner variables contain live handles, then null after first calls.
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }

    #[test]
    fn abi_001_dispatch_validates_commands_before_applying() {
        let mut core = ptr::null_mut();
        // SAFETY: All pointers reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        let command = InkpodCommand {
            struct_size: size_of::<InkpodCommand>() as u32,
            kind: INKPOD_COMMAND_NO_OP,
            flags: 0,
        };
        let batch = InkpodCommandBatch {
            struct_size: size_of::<InkpodCommandBatch>() as u32,
            reserved: 0,
            feature_flags: 0,
            commands: &command,
            command_count: 1,
            command_stride_bytes: size_of::<InkpodCommand>() as u64,
        };
        let mut result = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: u32::MAX,
            revision: u64::MAX,
            accepted_command_count: 0,
        };
        // SAFETY: The core and all batch/result storage are live for the call.
        assert_eq!(
            unsafe { inkpod_core_dispatch_batch(core, &batch, &mut result) },
            INKPOD_STATUS_OK
        );
        assert_eq!(result.revision, 0);
        assert_eq!(result.accepted_command_count, 1);

        #[repr(C)]
        struct ExtendedCommand {
            command: InkpodCommand,
            extension: u64,
        }
        let extended_commands = [
            ExtendedCommand {
                command: InkpodCommand {
                    struct_size: size_of::<ExtendedCommand>() as u32,
                    kind: INKPOD_COMMAND_NO_OP,
                    flags: 0,
                },
                extension: 1,
            },
            ExtendedCommand {
                command: InkpodCommand {
                    struct_size: size_of::<ExtendedCommand>() as u32,
                    kind: INKPOD_COMMAND_NO_OP,
                    flags: 0,
                },
                extension: 2,
            },
        ];
        let extended_batch = InkpodCommandBatch {
            commands: &extended_commands[0].command,
            command_count: extended_commands.len() as u64,
            command_stride_bytes: size_of::<ExtendedCommand>() as u64,
            ..batch
        };
        // SAFETY: The explicit stride describes both extended records.
        assert_eq!(
            unsafe { inkpod_core_dispatch_batch(core, &extended_batch, &mut result) },
            INKPOD_STATUS_OK
        );
        assert_eq!(result.accepted_command_count, 2);

        let invalid_stride_batch = InkpodCommandBatch {
            command_stride_bytes: (size_of::<InkpodCommand>() - 1) as u64,
            ..batch
        };
        // SAFETY: Storage is valid; the record stride is intentionally short.
        assert_eq!(
            unsafe { inkpod_core_dispatch_batch(core, &invalid_stride_batch, &mut result) },
            INKPOD_STATUS_INVALID_ARGUMENT
        );

        let invalid_command = InkpodCommand {
            kind: 99,
            ..command
        };
        let invalid_batch = InkpodCommandBatch {
            commands: &invalid_command,
            ..batch
        };
        // SAFETY: Storage is valid; the enum value is intentionally unsupported.
        assert_eq!(
            unsafe { inkpod_core_dispatch_batch(core, &invalid_batch, &mut result) },
            INKPOD_STATUS_UNSUPPORTED
        );

        let mut required = 0;
        // SAFETY: required is writable local storage.
        assert_eq!(
            unsafe { inkpod_error_message_size(&mut required) },
            INKPOD_STATUS_OK
        );
        assert!(required > 1);
        let mut message = vec![0_u8; required as usize];
        let mut written = 0;
        // SAFETY: message has the queried capacity and written is writable.
        assert_eq!(
            unsafe {
                inkpod_error_message_copy(message.as_mut_ptr(), message.len() as u64, &mut written)
            },
            INKPOD_STATUS_OK
        );
        assert!(written > 0);
        assert_eq!(message[written as usize], 0);

        // SAFETY: The owner variable contains the live handle.
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }

    #[test]
    fn abi_001_rejects_null_and_short_structures() {
        #[repr(C, align(8))]
        struct StructSizePrefix {
            struct_size: u32,
        }

        let mut core = ptr::null_mut();
        // SAFETY: Null input is intentionally tested; output is writable.
        assert_eq!(
            unsafe { inkpod_core_create(ptr::null(), &mut core) },
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(core.is_null());

        let short = StructSizePrefix { struct_size: 4 };
        // SAFETY: The deliberately short allocation contains the required size
        // prefix and is sufficiently aligned; no complete config is advertised.
        assert_eq!(
            unsafe { inkpod_core_create((&raw const short).cast::<InkpodCoreConfig>(), &mut core) },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert!(core.is_null());

        // SAFETY: All pointers reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        let mut result = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        // SAFETY: The short batch exposes only its aligned size prefix.
        assert_eq!(
            unsafe {
                inkpod_core_dispatch_batch(
                    core,
                    (&raw const short).cast::<InkpodCommandBatch>(),
                    &mut result,
                )
            },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );

        let empty_batch = InkpodCommandBatch {
            struct_size: size_of::<InkpodCommandBatch>() as u32,
            reserved: 0,
            feature_flags: 0,
            commands: ptr::null(),
            command_count: 0,
            command_stride_bytes: size_of::<InkpodCommand>() as u64,
        };
        let mut short_output = StructSizePrefix { struct_size: 4 };
        // SAFETY: The short result exposes only its writable size prefix.
        assert_eq!(
            unsafe {
                inkpod_core_dispatch_batch(
                    core,
                    &empty_batch,
                    (&raw mut short_output).cast::<InkpodDispatchResult>(),
                )
            },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );

        let mut snapshot = ptr::null_mut();
        // SAFETY: The short options expose only their aligned size prefix.
        assert_eq!(
            unsafe {
                inkpod_core_build_snapshot(
                    core,
                    (&raw const short).cast::<InkpodSnapshotOptions>(),
                    &mut snapshot,
                )
            },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert!(snapshot.is_null());

        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
        };
        // SAFETY: Inputs and output reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
            INKPOD_STATUS_OK
        );
        // SAFETY: The short view exposes only its writable size prefix.
        assert_eq!(
            unsafe {
                inkpod_snapshot_get_view(
                    snapshot,
                    (&raw mut short_output).cast::<InkpodSnapshotView>(),
                )
            },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );

        // SAFETY: Owner variables contain live handles.
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }

    #[test]
    fn abi_001_contains_panics_and_preserves_a_diagnostic() {
        clear_last_error();
        let status = ffi_boundary(|| panic!("intentional ABI containment test"));
        assert_eq!(status, INKPOD_STATUS_PANIC);

        let mut required = 0;
        // SAFETY: required is writable local storage.
        assert_eq!(
            unsafe { inkpod_error_message_size(&mut required) },
            INKPOD_STATUS_OK
        );
        assert!(required > 1);

        fail(INKPOD_STATUS_INVALID_ARGUMENT, &"界".repeat(ERROR_CAPACITY));
        // SAFETY: required and the subsequently sized buffer are writable.
        assert_eq!(
            unsafe { inkpod_error_message_size(&mut required) },
            INKPOD_STATUS_OK
        );
        let mut message = vec![0_u8; required as usize];
        let mut written = 0;
        // SAFETY: The buffer uses the exact queried capacity.
        assert_eq!(
            unsafe { inkpod_error_message_copy(message.as_mut_ptr(), required, &mut written) },
            INKPOD_STATUS_OK
        );
        assert!(std::str::from_utf8(&message[..written as usize]).is_ok());
    }

    #[test]
    fn m1_batched_stroke_snapshot_view_history_and_round_trip() {
        static PATH_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

        let mut core = ptr::null_mut();
        // SAFETY: All pointers reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        let create = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 0x1234_5678_9abc_def0,
            document_uuid_low: 0x1032_5476_98ba_dcfe,
            width: 1920,
            height: 1080,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut info = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        // SAFETY: Core, options, and output are valid and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_new_cell(core, &create, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!((info.width, info.height), (1920, 1080));
        assert_eq!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        let ids = (
            info.document_id,
            info.layer_id,
            info.main_plane_id,
            info.color_plane_id,
        );

        #[repr(C)]
        struct ExtendedStrokeSample {
            sample: InkpodStrokeSample,
            extension: u64,
        }
        let samples: Vec<_> = (0..256)
            .map(|index| ExtendedStrokeSample {
                sample: InkpodStrokeSample {
                    struct_size: size_of::<ExtendedStrokeSample>() as u32,
                    flags: 0,
                    x: 10.0 + index as f32,
                    y: 20.0,
                    pressure: 0.5,
                    reserved: 0,
                },
                extension: index,
            })
            .collect();
        let mut stroke = InkpodStrokeInput {
            struct_size: size_of::<InkpodStrokeInput>() as u32,
            tool: INKPOD_TOOL_PENCIL,
            plane: INKPOD_PLANE_MAIN_LINE,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            flags: 0,
            color_rgba: 0x0000_00ff,
            diameter: 1.0,
            samples: &samples[0].sample,
            sample_count: samples.len() as u64,
            sample_stride_bytes: size_of::<ExtendedStrokeSample>() as u64,
        };
        let mut dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        // SAFETY: One call borrows the complete 256-record sample span.
        assert_eq!(
            unsafe { inkpod_core_apply_stroke(core, &stroke, &mut dispatch) },
            INKPOD_STATUS_OK
        );
        assert_eq!(dispatch.accepted_command_count, 1);
        // SAFETY: Core and output are live owner-thread objects.
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        let line_checksum = info.main_plane_checksum;

        stroke.plane = INKPOD_PLANE_COLOR;
        stroke.color_rgba = 0xdc_28_1e_ff;
        // SAFETY: The same complete sample span remains live.
        assert_eq!(
            unsafe { inkpod_core_apply_stroke(core, &stroke, &mut dispatch) },
            INKPOD_STATUS_OK
        );
        // SAFETY: Core and output are live owner-thread objects.
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.main_plane_checksum, line_checksum);
        assert_ne!(info.color_plane_checksum, 0);
        let color_checksum = info.color_plane_checksum;
        // SAFETY: History result storage is live and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_undo(core, &mut dispatch) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_core_redo(core, &mut dispatch) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.color_plane_checksum, color_checksum);

        let revision_before_view = info.document_revision;
        let view_input = InkpodViewInput {
            struct_size: size_of::<InkpodViewInput>() as u32,
            kind: INKPOD_VIEW_ZOOM_AT,
            flags: 0,
            value1: 2.0,
            value2: 320.0,
            value3: 240.0,
            value4: 0.0,
        };
        // SAFETY: Input/output/Core are complete owner-thread objects.
        assert_eq!(
            unsafe { inkpod_core_apply_view(core, &view_input, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, revision_before_view);

        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
        };
        let mut snapshot = ptr::null_mut();
        // SAFETY: Core/options/output are valid and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
            INKPOD_STATUS_OK
        );
        let mut view = InkpodSnapshotView {
            struct_size: size_of::<InkpodSnapshotView>() as u32,
            abi_version: 0,
            feature_flags: 0,
            revision: 0,
            tiles: ptr::null(),
            tile_count: 0,
            tile_stride_bytes: 0,
        };
        let mut transform = InkpodSnapshotTransform {
            struct_size: size_of::<InkpodSnapshotTransform>() as u32,
            ..InkpodSnapshotTransform::default()
        };
        // SAFETY: Snapshot and outputs remain live for both view calls.
        assert_eq!(
            unsafe { inkpod_snapshot_get_view(snapshot, &mut view) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_snapshot_get_transform(snapshot, &mut transform) },
            INKPOD_STATUS_OK
        );
        assert!(view.tile_count > 0 && !view.tiles.is_null());
        assert_eq!(transform.zoom, 2.0);
        assert_eq!(
            (transform.document_width, transform.document_height),
            (1920, 1080)
        );
        // SAFETY: Owner variable contains the live snapshot handle.
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );

        let path = std::env::temp_dir().join(format!(
            "inkpod-ffi-m1-{}-{}.inkpod",
            std::process::id(),
            PATH_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let path = path.to_str().unwrap().as_bytes().to_vec();
        // SAFETY: UTF-8 path bytes and output remain live for this call.
        assert_eq!(
            unsafe { inkpod_core_save(core, path.as_ptr(), path.len() as u64, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        // Discard the in-memory document, then reopen from the saved container.
        assert_eq!(
            unsafe { inkpod_core_new_cell(core, &create, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_ne!(info.document_id, ids.0);
        assert_eq!(
            unsafe { inkpod_core_open(core, path.as_ptr(), path.len() as u64, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                info.document_id,
                info.layer_id,
                info.main_plane_id,
                info.color_plane_id
            ),
            ids
        );
        assert_eq!(info.main_plane_checksum, line_checksum);
        assert_eq!(info.color_plane_checksum, color_checksum);
        std::fs::remove_file(std::str::from_utf8(&path).unwrap()).unwrap();
        // SAFETY: Owner variable contains the live Core handle.
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }

    #[test]
    fn live_stroke_abi_previews_then_commits_once_and_cancel_is_safe() {
        let mut core = ptr::null_mut();
        // SAFETY: Config and output storage are complete and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        let create = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 7,
            document_uuid_low: 11,
            width: 64,
            height: 64,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut info = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        // SAFETY: Core/options/output are live and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_new_cell(core, &create, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        let initial_revision = info.document_revision;
        let initial_checksum = info.main_plane_checksum;
        let begin_sample = InkpodStrokeSample {
            struct_size: size_of::<InkpodStrokeSample>() as u32,
            flags: 0,
            x: 4.0,
            y: 4.0,
            pressure: 1.0,
            reserved: 0,
        };
        let begin = InkpodStrokeInput {
            struct_size: size_of::<InkpodStrokeInput>() as u32,
            tool: INKPOD_TOOL_PENCIL,
            plane: INKPOD_PLANE_MAIN_LINE,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            flags: 0,
            color_rgba: 0x0000_00ff,
            diameter: 1.0,
            samples: &begin_sample,
            sample_count: 1,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        // SAFETY: The first sample and Core remain live for the call.
        assert_eq!(
            unsafe { inkpod_core_stroke_begin(core, &begin) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, initial_revision);
        assert_eq!(info.main_plane_checksum, initial_checksum);
        assert_eq!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);

        let appended = [
            InkpodStrokeSample {
                x: 12.0,
                ..begin_sample
            },
            InkpodStrokeSample {
                x: 20.0,
                ..begin_sample
            },
        ];
        let span = InkpodStrokeSampleSpan {
            struct_size: size_of::<InkpodStrokeSampleSpan>() as u32,
            reserved: 0,
            feature_flags: 0,
            samples: appended.as_ptr(),
            sample_count: appended.len() as u64,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        // SAFETY: The strided sample span is complete and borrowed for this call.
        assert_eq!(
            unsafe { inkpod_core_stroke_append(core, &span) },
            INKPOD_STATUS_OK
        );
        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
        };
        let mut snapshot = ptr::null_mut();
        // SAFETY: Core/options/output are live and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
            INKPOD_STATUS_OK
        );
        let mut view = InkpodSnapshotView {
            struct_size: size_of::<InkpodSnapshotView>() as u32,
            abi_version: 0,
            feature_flags: 0,
            revision: 0,
            tiles: ptr::null(),
            tile_count: 0,
            tile_stride_bytes: 0,
        };
        assert_eq!(
            unsafe { inkpod_snapshot_get_view(snapshot, &mut view) },
            INKPOD_STATUS_OK
        );
        assert!(view.revision >= 1_u64 << 63 && view.tile_count == 1);
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );

        let mut result = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        // SAFETY: Core and result are live owner-thread objects.
        assert_eq!(
            unsafe { inkpod_core_stroke_end(core, &mut result) },
            INKPOD_STATUS_OK
        );
        assert_eq!(result.revision, initial_revision + 1);
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_ne!(info.main_plane_checksum, initial_checksum);
        assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO, 0);
        assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);

        assert_eq!(
            unsafe { inkpod_core_stroke_begin(core, &begin) },
            INKPOD_STATUS_OK
        );
        assert_eq!(unsafe { inkpod_core_stroke_cancel(core) }, INKPOD_STATUS_OK);
        assert_eq!(unsafe { inkpod_core_stroke_cancel(core) }, INKPOD_STATUS_OK);

        let committed_revision = info.document_revision;
        let committed_checksum = info.main_plane_checksum;
        assert_eq!(
            unsafe { inkpod_core_stroke_begin(core, &begin) },
            INKPOD_STATUS_OK
        );
        let short_span = InkpodStrokeSampleSpan {
            struct_size: size_of::<u32>() as u32,
            ..span
        };
        assert_eq!(
            unsafe { inkpod_core_stroke_append(core, &short_span) },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(
            unsafe { inkpod_core_stroke_end(core, &mut result) },
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, committed_revision);
        assert_eq!(info.main_plane_checksum, committed_checksum);

        assert_eq!(
            unsafe { inkpod_core_stroke_begin(core, &begin) },
            INKPOD_STATUS_OK
        );
        let mut short_result = InkpodDispatchResult {
            struct_size: size_of::<u32>() as u32,
            ..result
        };
        assert_eq!(
            unsafe { inkpod_core_stroke_end(core, &mut short_result) },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(
            unsafe { inkpod_core_stroke_end(core, &mut result) },
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }

    #[test]
    fn m2_fill_eyedropper_check_and_recovery_abi_are_transactional() {
        unsafe {
            let mut core = ptr::null_mut();
            assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
            let options = InkpodCellCreateOptions {
                struct_size: size_of::<InkpodCellCreateOptions>() as u32,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
                document_uuid_high: 1,
                document_uuid_low: 2,
                width: 8,
                height: 8,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
            };
            let mut info = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..InkpodDocumentInfo::default()
            };
            assert_eq!(
                inkpod_core_new_cell(core, &options, &mut info),
                INKPOD_STATUS_OK
            );
            let created_revision = info.document_revision;
            let created_main_checksum = info.main_plane_checksum;
            let created_color_checksum = info.color_plane_checksum;
            let color = InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_8,
                red: 12,
                green: 34,
                blue: 56,
                alpha: 255,
            };
            let mut fill = InkpodFillInput {
                struct_size: size_of::<InkpodFillInput>() as u32,
                operation: INKPOD_FILL_SEED,
                flags: INKPOD_FILL_FLAG_OVERFLOW_ABORT,
                seed_x: 4,
                seed_y: 4,
                color,
                tolerance: 0,
                gap_close: 0,
                inclusion_mode: INKPOD_INCLUSION_NONE,
                selection: InkpodFrameRect::default(),
                inclusion_colors: ptr::null(),
                inclusion_color_count: 0,
                inclusion_color_stride_bytes: 0,
                extension_distance: 0,
                reserved: 0,
            };
            let mut result = InkpodFillResult {
                struct_size: size_of::<InkpodFillResult>() as u32,
                ..InkpodFillResult::default()
            };
            let mut short_fill = fill;
            short_fill.struct_size = size_of::<u32>() as u32;
            assert_eq!(
                inkpod_core_apply_fill(core, &short_fill, &mut result),
                INKPOD_STATUS_INCOMPATIBLE_ABI
            );
            let mut unknown_fill = fill;
            unknown_fill.operation = 99;
            assert_eq!(
                inkpod_core_apply_fill(core, &unknown_fill, &mut result),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            let mut short_result = InkpodFillResult {
                struct_size: size_of::<u32>() as u32,
                ..InkpodFillResult::default()
            };
            assert_eq!(
                inkpod_core_apply_fill(core, &fill, &mut short_result),
                INKPOD_STATUS_INCOMPATIBLE_ABI
            );
            assert_eq!(
                inkpod_core_apply_fill(core, &fill, &mut result),
                INKPOD_STATUS_FILL_OVERFLOW
            );
            assert_ne!(result.flags & INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE, 0);
            assert_eq!(
                inkpod_core_get_document_info(core, &mut info),
                INKPOD_STATUS_OK
            );
            assert_eq!(info.document_revision, created_revision);
            assert_eq!(info.color_plane_checksum, created_color_checksum);

            fill.flags = INKPOD_FILL_FLAG_SELECTION_PRESENT;
            fill.seed_x = 2;
            fill.seed_y = 2;
            fill.selection = InkpodFrameRect {
                x: 2,
                y: 2,
                width: 2,
                height: 2,
            };
            assert_eq!(
                inkpod_core_apply_fill(core, &fill, &mut result),
                INKPOD_STATUS_OK
            );
            assert_eq!(result.changed_pixel_count, 4);
            assert_eq!(
                inkpod_core_get_document_info(core, &mut info),
                INKPOD_STATUS_OK
            );
            assert_eq!(info.document_revision, created_revision + 1);
            assert_eq!(info.main_plane_checksum, created_main_checksum);
            assert_ne!(info.color_plane_checksum, created_color_checksum);

            let mut sampled = InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                ..InkpodColorValue::default()
            };
            assert_eq!(
                inkpod_core_eyedropper(core, INKPOD_EYEDROPPER_SELECTED_PLANE, 2, 2, &mut sampled,),
                INKPOD_STATUS_OK
            );
            assert_eq!(sampled.depth, INKPOD_COLOR_DEPTH_8);
            assert_eq!(
                [sampled.red, sampled.green, sampled.blue, sampled.alpha],
                [12, 34, 56, 255]
            );

            let revision_before_check = info.document_revision;
            let view_before_check = info.view_revision;
            assert_eq!(
                inkpod_core_set_color_check(core, INKPOD_COLOR_CHECK_NATIVE_ALPHA),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_get_document_info(core, &mut info),
                INKPOD_STATUS_OK
            );
            assert_eq!(info.document_revision, revision_before_check);
            assert!(info.view_revision > view_before_check);

            let snapshot_options = InkpodSnapshotOptions {
                struct_size: size_of::<InkpodSnapshotOptions>() as u32,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
            };
            let mut check_snapshot = ptr::null_mut();
            assert_eq!(
                inkpod_core_build_snapshot(core, &snapshot_options, &mut check_snapshot),
                INKPOD_STATUS_OK
            );
            let mut check_view = InkpodSnapshotView {
                struct_size: size_of::<InkpodSnapshotView>() as u32,
                abi_version: 0,
                feature_flags: 0,
                revision: 0,
                tiles: ptr::null(),
                tile_count: 0,
                tile_stride_bytes: 0,
            };
            assert_eq!(
                inkpod_snapshot_get_view(check_snapshot, &mut check_view),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                check_view.feature_flags,
                INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA
            );
            assert_eq!(
                inkpod_snapshot_release(&mut check_snapshot),
                INKPOD_STATUS_OK
            );

            let palette = [
                InkpodColorValue {
                    struct_size: size_of::<InkpodColorValue>() as u32,
                    depth: INKPOD_COLOR_DEPTH_8,
                    red: 12,
                    green: 34,
                    blue: 56,
                    alpha: 255,
                },
                InkpodColorValue {
                    struct_size: size_of::<InkpodColorValue>() as u32,
                    depth: INKPOD_COLOR_DEPTH_16,
                    red: 1,
                    green: 257,
                    blue: 32_769,
                    alpha: 65_534,
                },
            ];
            let palette_input = InkpodColorArray {
                struct_size: size_of::<InkpodColorArray>() as u32,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
                colors: palette.as_ptr(),
                color_count: palette.len() as u64,
                color_stride_bytes: size_of::<InkpodColorValue>() as u64,
            };
            let invalid_empty_palette = InkpodColorArray {
                colors: palette.as_ptr(),
                color_count: 0,
                color_stride_bytes: 0,
                ..palette_input
            };
            let mut palette_dispatch = InkpodDispatchResult {
                struct_size: size_of::<InkpodDispatchResult>() as u32,
                reserved: 0,
                revision: 0,
                accepted_command_count: 0,
            };
            assert_eq!(
                inkpod_core_palette_set(core, &invalid_empty_palette, &mut palette_dispatch,),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            assert_eq!(
                inkpod_core_palette_set(core, &palette_input, &mut palette_dispatch),
                INKPOD_STATUS_OK
            );
            let mut count_query = InkpodColorBuffer {
                struct_size: size_of::<InkpodColorBuffer>() as u32,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
                colors: ptr::null_mut(),
                color_capacity: 0,
                color_stride_bytes: 0,
                color_count: 0,
            };
            assert_eq!(
                inkpod_core_palette_get(core, &mut count_query),
                INKPOD_STATUS_OK
            );
            assert_eq!(count_query.color_count, palette.len() as u64);
            let mut too_small_record = InkpodColorValue {
                struct_size: 77,
                depth: 88,
                red: 99,
                green: 100,
                blue: 101,
                alpha: 102,
            };
            let mut too_small_buffer = InkpodColorBuffer {
                struct_size: size_of::<InkpodColorBuffer>() as u32,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
                colors: &mut too_small_record,
                color_capacity: 1,
                color_stride_bytes: size_of::<InkpodColorValue>() as u64,
                color_count: 0,
            };
            assert_eq!(
                inkpod_core_palette_get(core, &mut too_small_buffer),
                INKPOD_STATUS_BUFFER_TOO_SMALL
            );
            assert_eq!(too_small_buffer.color_count, palette.len() as u64);
            assert_eq!(too_small_record.struct_size, 77);
            assert_eq!(too_small_record.depth, 88);
            let mut copied = [InkpodColorValue::default(); 2];
            let mut palette_buffer = InkpodColorBuffer {
                struct_size: size_of::<InkpodColorBuffer>() as u32,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
                colors: copied.as_mut_ptr(),
                color_capacity: copied.len() as u64,
                color_stride_bytes: size_of::<InkpodColorValue>() as u64,
                color_count: 0,
            };
            assert_eq!(
                inkpod_core_palette_get(core, &mut palette_buffer),
                INKPOD_STATUS_OK
            );
            assert_eq!(copied[0].depth, INKPOD_COLOR_DEPTH_8);
            assert_eq!(copied[0].red, 12);
            assert_eq!(copied[1].depth, INKPOD_COLOR_DEPTH_16);
            assert_eq!(copied[1].blue, 32_769);

            let mut sixteen_bit_fill = fill;
            sixteen_bit_fill.color = InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_16,
                red: 1,
                green: 257,
                blue: 32_769,
                alpha: 65_534,
            };
            let checksum_before_invalid = info.color_plane_checksum;
            assert_eq!(
                inkpod_core_apply_fill(core, &sixteen_bit_fill, &mut result),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            assert_eq!(
                inkpod_core_get_document_info(core, &mut info),
                INKPOD_STATUS_OK
            );
            assert_eq!(info.color_plane_checksum, checksum_before_invalid);

            let path = std::env::temp_dir().join(format!(
                "inkpod-ffi-m2-recovery-{}-{}.inkpod",
                std::process::id(),
                info.document_revision
            ));
            let path_text = path.to_str().unwrap().as_bytes();
            assert_eq!(
                inkpod_core_autosave(core, path_text.as_ptr(), path_text.len() as u64, &mut info,),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);

            assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
            assert_eq!(
                inkpod_core_open_recovery(
                    core,
                    path_text.as_ptr(),
                    path_text.len() as u64,
                    &mut info,
                ),
                INKPOD_STATUS_OK
            );
            assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_RECOVERED, 0);
            assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
            let mut recovered_palette = [InkpodColorValue::default(); 2];
            let mut recovered_buffer = InkpodColorBuffer {
                struct_size: size_of::<InkpodColorBuffer>() as u32,
                reserved: 0,
                feature_flags: INKPOD_FEATURE_NONE,
                colors: recovered_palette.as_mut_ptr(),
                color_capacity: recovered_palette.len() as u64,
                color_stride_bytes: size_of::<InkpodColorValue>() as u64,
                color_count: 0,
            };
            assert_eq!(
                inkpod_core_palette_get(core, &mut recovered_buffer),
                INKPOD_STATUS_OK
            );
            assert_eq!(recovered_palette[1].depth, INKPOD_COLOR_DEPTH_16);
            assert_eq!(recovered_palette[1].alpha, 65_534);
            assert_eq!(
                inkpod_core_revert(core, &mut info),
                INKPOD_STATUS_INVALID_STATE
            );
            assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn m3_typed_tree_selection_clipboard_view_and_multiview_abi_are_connected() {
        unsafe {
            let mut core = ptr::null_mut();
            assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
            let create = |width, height, uuid_low| InkpodCellCreateOptions {
                struct_size: size_of::<InkpodCellCreateOptions>() as u32,
                reserved: 0,
                feature_flags: 0,
                document_uuid_high: 0x494e_4b50_4f44_4d33,
                document_uuid_low: uuid_low,
                width,
                height,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
            };
            let mut info = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..InkpodDocumentInfo::default()
            };
            assert_eq!(
                inkpod_core_new_cell(core, &create(8, 8, 1), &mut info),
                INKPOD_STATUS_OK
            );
            let base_layer = info.layer_id;
            let mut result = InkpodDispatchResult {
                struct_size: size_of::<InkpodDispatchResult>() as u32,
                reserved: 0,
                revision: 0,
                accepted_command_count: 0,
            };
            let mut object_id = 0;
            let mut edit = InkpodTreeEdit {
                struct_size: size_of::<InkpodTreeEdit>() as u32,
                operation: INKPOD_TREE_DUPLICATE_LAYER,
                flags: 0,
                object_id: base_layer,
                parent_id: 0,
                destination_index: 0,
                kind: 0,
                pixel_format: 0,
                opacity_milli: 0,
                name_utf8: ptr::null(),
                name_bytes: 0,
            };
            assert_eq!(
                inkpod_core_tree_edit(core, &edit, &mut result, &mut object_id),
                INKPOD_STATUS_OK
            );
            let duplicate = object_id;
            assert_ne!(duplicate, 0);
            edit.operation = INKPOD_TREE_REORDER_LAYER;
            edit.object_id = duplicate;
            edit.destination_index = 0;
            assert_eq!(
                inkpod_core_tree_edit(core, &edit, &mut result, &mut object_id),
                INKPOD_STATUS_OK
            );
            let mut node = InkpodNodeInfo {
                struct_size: size_of::<InkpodNodeInfo>() as u32,
                ..InkpodNodeInfo::default()
            };
            assert_eq!(
                inkpod_core_node_get(core, 0, u32::MAX, &mut node),
                INKPOD_STATUS_OK
            );
            assert_eq!(node.id, duplicate);
            assert_eq!(node.child_count, 2);
            let revision_before_invalid_tree = result.revision;
            let invalid_name = b"Invalid selection";
            let invalid_plane = InkpodTreeEdit {
                operation: INKPOD_TREE_CREATE_PLANE,
                parent_id: base_layer,
                kind: INKPOD_TYPED_PLANE_SELECTION,
                pixel_format: INKPOD_STORAGE_BINARY8,
                name_utf8: invalid_name.as_ptr(),
                name_bytes: invalid_name.len() as u64,
                ..edit
            };
            assert_eq!(
                inkpod_core_tree_edit(core, &invalid_plane, &mut result, &mut object_id),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            assert_eq!(result.revision, revision_before_invalid_tree);
            edit.operation = INKPOD_TREE_DELETE_LAYER;
            assert_eq!(
                inkpod_core_tree_edit(core, &edit, &mut result, &mut object_id),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_core_undo(core, &mut result), INKPOD_STATUS_OK);
            assert_eq!(inkpod_core_redo(core, &mut result), INKPOD_STATUS_OK);
            assert_eq!(inkpod_core_undo(core, &mut result), INKPOD_STATUS_OK);

            assert_eq!(
                inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR),
                INKPOD_STATUS_OK
            );
            let sample = InkpodStrokeSample {
                struct_size: size_of::<InkpodStrokeSample>() as u32,
                flags: 0,
                x: 6.0,
                y: 6.0,
                pressure: 1.0,
                reserved: 0,
            };
            let stroke = InkpodStrokeInput {
                struct_size: size_of::<InkpodStrokeInput>() as u32,
                tool: INKPOD_TOOL_PENCIL,
                plane: INKPOD_PLANE_COLOR,
                coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
                flags: 0,
                color_rgba: 0x0c22_38ff,
                diameter: 1.0,
                samples: &sample,
                sample_count: 1,
                sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
            };
            assert_eq!(
                inkpod_core_apply_stroke(core, &stroke, &mut result),
                INKPOD_STATUS_OK
            );
            let selected_color = InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_8,
                red: 12,
                green: 34,
                blue: 56,
                alpha: 255,
            };
            assert_eq!(
                inkpod_core_select_color(
                    core,
                    &selected_color,
                    0,
                    0,
                    INKPOD_SELECTION_NEW,
                    &mut result
                ),
                INKPOD_STATUS_OK
            );
            let selection = InkpodSelectionInput {
                struct_size: size_of::<InkpodSelectionInput>() as u32,
                shape: INKPOD_SELECTION_RECTANGLE,
                operation: INKPOD_SELECTION_NEW,
                reserved: 0,
                bounds: InkpodFrameRect {
                    x: 6,
                    y: 6,
                    width: 1,
                    height: 1,
                },
                points: ptr::null(),
                point_count: 0,
                point_stride_bytes: 0,
                diameter: 0.0,
                tolerance: 0,
                gap_close: 0,
                seed_x: 0,
                seed_y: 0,
            };
            assert_eq!(
                inkpod_core_apply_selection(core, &selection, &mut result),
                INKPOD_STATUS_OK
            );
            let invalid_point_free = InkpodSelectionInput {
                point_stride_bytes: size_of::<InkpodSelectionPoint>() as u64,
                ..selection
            };
            assert_eq!(
                inkpod_core_apply_selection(core, &invalid_point_free, &mut result),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            let oversized_point = InkpodSelectionPoint {
                struct_size: (size_of::<InkpodSelectionPoint>() + 8) as u32,
                reserved: 0,
                x: 0.0,
                y: 0.0,
            };
            let invalid_strided_point = InkpodSelectionInput {
                shape: INKPOD_SELECTION_LASSO,
                points: &oversized_point,
                point_count: 1,
                point_stride_bytes: size_of::<InkpodSelectionPoint>() as u64,
                ..selection
            };
            assert_eq!(
                inkpod_core_apply_selection(core, &invalid_strided_point, &mut result),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            let mut clipboard = ptr::null_mut();
            assert_eq!(
                inkpod_core_clipboard_copy(core, &mut clipboard),
                INKPOD_STATUS_OK
            );
            assert!(!clipboard.is_null());

            assert_eq!(
                inkpod_core_new_cell(core, &create(4, 4, 2), &mut info),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_core_paste_begin(core, clipboard), INKPOD_STATUS_OK);
            let transform = InkpodFloatingTransform {
                struct_size: size_of::<InkpodFloatingTransform>() as u32,
                reserved: 0,
                translate_x: -4.0,
                translate_y: -4.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation_degrees: 0.0,
            };
            assert_eq!(
                inkpod_core_floating_transform(core, &transform),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_floating_commit(core, &mut result),
                INKPOD_STATUS_OK
            );
            let mut color = InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                ..InkpodColorValue::default()
            };
            assert_eq!(
                inkpod_core_eyedropper(core, INKPOD_EYEDROPPER_SELECTED_PLANE, 2, 2, &mut color),
                INKPOD_STATUS_OK
            );
            assert_eq!((color.red, color.green, color.blue), (12, 34, 56));
            assert_eq!(inkpod_clipboard_release(&mut clipboard), INKPOD_STATUS_OK);
            assert_eq!(inkpod_clipboard_release(&mut clipboard), INKPOD_STATUS_OK);

            let flip = InkpodViewInput {
                struct_size: size_of::<InkpodViewInput>() as u32,
                kind: INKPOD_VIEW_FLIP_HORIZONTAL,
                flags: 0,
                value1: 0.0,
                value2: 0.0,
                value3: 0.0,
                value4: 0.0,
            };
            assert_eq!(
                inkpod_core_get_document_info(core, &mut info),
                INKPOD_STATUS_OK
            );
            let document_revision = info.document_revision;
            assert_eq!(
                inkpod_core_apply_view(core, &flip, &mut info),
                INKPOD_STATUS_OK
            );
            assert_eq!(info.document_revision, document_revision);
            assert_eq!(
                inkpod_core_mirror_document(core, 1, &mut result),
                INKPOD_STATUS_OK
            );
            assert!(result.revision > document_revision);

            let mut view_id = 0;
            assert_eq!(
                inkpod_core_view_create(core, &mut view_id),
                INKPOD_STATUS_OK
            );
            let secondary_pan = InkpodViewInput {
                struct_size: size_of::<InkpodViewInput>() as u32,
                kind: INKPOD_VIEW_PAN_BY,
                flags: 0,
                value1: 5.0,
                value2: 0.0,
                value3: 0.0,
                value4: 0.0,
            };
            assert_eq!(
                inkpod_core_view_apply(core, view_id, &secondary_pan),
                INKPOD_STATUS_OK
            );
            let snapshot_options = InkpodSnapshotOptions {
                struct_size: size_of::<InkpodSnapshotOptions>() as u32,
                reserved: 0,
                feature_flags: 0,
            };
            let mut primary = ptr::null_mut();
            let mut secondary = ptr::null_mut();
            assert_eq!(
                inkpod_core_build_snapshot(core, &snapshot_options, &mut primary),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_build_snapshot_for_view(
                    core,
                    view_id,
                    &snapshot_options,
                    &mut secondary
                ),
                INKPOD_STATUS_OK
            );
            let mut primary_view = InkpodSnapshotView {
                struct_size: size_of::<InkpodSnapshotView>() as u32,
                abi_version: 0,
                feature_flags: 0,
                revision: 0,
                tiles: ptr::null(),
                tile_count: 0,
                tile_stride_bytes: 0,
            };
            let mut secondary_view = InkpodSnapshotView { ..primary_view };
            let mut primary_transform = InkpodSnapshotTransform {
                struct_size: size_of::<InkpodSnapshotTransform>() as u32,
                ..InkpodSnapshotTransform::default()
            };
            let mut secondary_transform = InkpodSnapshotTransform {
                struct_size: size_of::<InkpodSnapshotTransform>() as u32,
                ..InkpodSnapshotTransform::default()
            };
            assert_eq!(
                inkpod_snapshot_get_view(primary, &mut primary_view),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_snapshot_get_view(secondary, &mut secondary_view),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_snapshot_get_transform(primary, &mut primary_transform),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_snapshot_get_transform(secondary, &mut secondary_transform),
                INKPOD_STATUS_OK
            );
            assert_eq!(primary_view.revision, secondary_view.revision);
            assert_ne!(primary_transform.pan_x, secondary_transform.pan_x);
            assert_eq!(inkpod_snapshot_release(&mut primary), INKPOD_STATUS_OK);
            assert_eq!(inkpod_snapshot_release(&mut secondary), INKPOD_STATUS_OK);
            assert_eq!(inkpod_core_view_close(core, view_id), INKPOD_STATUS_OK);

            let mut guide_id = 0;
            assert_eq!(
                inkpod_core_guide_add(core, INKPOD_GUIDE_VERTICAL, 2, &mut result, &mut guide_id),
                INKPOD_STATUS_OK
            );
            assert_ne!(guide_id, 0);
            assert_eq!(
                inkpod_core_guide_move(core, guide_id, 3, &mut result),
                INKPOD_STATUS_OK
            );
            let grid = InkpodGridInput {
                struct_size: size_of::<InkpodGridInput>() as u32,
                reserved: 0,
                origin_x: 0,
                origin_y: 0,
                spacing_x: 4,
                spacing_y: 4,
                subdivisions: 2,
                flags: 0,
            };
            assert_eq!(
                inkpod_core_grid_set(core, &grid, &mut result),
                INKPOD_STATUS_OK
            );
            let show_grid = InkpodViewInput {
                struct_size: size_of::<InkpodViewInput>() as u32,
                kind: INKPOD_VIEW_SET_GRID_VISIBLE,
                flags: 0,
                value1: 1.0,
                value2: 0.0,
                value3: 0.0,
                value4: 0.0,
            };
            assert_eq!(
                inkpod_core_apply_view(core, &show_grid, &mut info),
                INKPOD_STATUS_OK
            );
            let mut overlay_snapshot = ptr::null_mut();
            assert_eq!(
                inkpod_core_build_snapshot(core, &snapshot_options, &mut overlay_snapshot),
                INKPOD_STATUS_OK
            );
            let mut overlay = InkpodSnapshotOverlay {
                struct_size: size_of::<InkpodSnapshotOverlay>() as u32,
                ..InkpodSnapshotOverlay::default()
            };
            assert_eq!(
                inkpod_snapshot_get_overlay(overlay_snapshot, &mut overlay),
                INKPOD_STATUS_OK
            );
            assert_ne!(overlay.flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE, 0);
            assert_eq!((overlay.grid_spacing_x, overlay.grid_subdivisions), (4, 2));
            assert_eq!(overlay.guide_count, 1);
            assert!(!overlay.guides.is_null());
            assert_eq!((*overlay.guides).id, guide_id);
            assert_eq!(
                inkpod_snapshot_release(&mut overlay_snapshot),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_snapshot_release(&mut overlay_snapshot),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_guide_delete(core, guide_id, &mut result),
                INKPOD_STATUS_OK
            );
            let mut locator = InkpodLocatorOutput {
                struct_size: size_of::<InkpodLocatorOutput>() as u32,
                ..InkpodLocatorOutput::default()
            };
            assert_eq!(
                inkpod_core_locator_sample(core, 0, 1.0, 1.0, &mut locator),
                INKPOD_STATUS_OK
            );
            assert_eq!((locator.document_x, locator.document_y), (3, 1));
            assert_eq!(
                inkpod_core_shortcut_rebind(
                    core,
                    99,
                    u32::from(b'Z'),
                    INKPOD_SHORTCUT_MODIFIER_CONTROL
                ),
                INKPOD_STATUS_OK
            );
            let mut shortcut_command = 0;
            assert_eq!(
                inkpod_core_shortcut_resolve(
                    core,
                    u32::from(b'Z'),
                    INKPOD_SHORTCUT_MODIFIER_CONTROL,
                    &mut shortcut_command
                ),
                INKPOD_STATUS_OK
            );
            assert_eq!(shortcut_command, 99);
            shortcut_command = 123;
            assert_eq!(
                inkpod_core_shortcut_resolve(core, u32::from(b'Z'), 0x10, &mut shortcut_command),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            assert_eq!(shortcut_command, 0);
            assert_eq!(inkpod_core_shortcut_reset(core), INKPOD_STATUS_OK);
            assert_eq!(
                inkpod_core_shortcut_resolve(
                    core,
                    u32::from(b'Z'),
                    INKPOD_SHORTCUT_MODIFIER_CONTROL,
                    &mut shortcut_command
                ),
                INKPOD_STATUS_OK
            );
            assert_eq!(shortcut_command, 1);
            assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        }
    }

    #[test]
    fn m5_vector_commands_snapshot_and_nested_span_validation_are_connected() {
        unsafe {
            let mut core = ptr::null_mut();
            assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
            let create = InkpodCellCreateOptions {
                struct_size: size_of::<InkpodCellCreateOptions>() as u32,
                reserved: 0,
                feature_flags: 0,
                document_uuid_high: 0x494e_4b50_4f44_4d35,
                document_uuid_low: 1,
                width: 8,
                height: 8,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
            };
            let mut info = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..InkpodDocumentInfo::default()
            };
            assert_eq!(
                inkpod_core_new_cell(core, &create, &mut info),
                INKPOD_STATUS_OK
            );
            let name = b"Vector";
            let edit = InkpodTreeEdit {
                struct_size: size_of::<InkpodTreeEdit>() as u32,
                operation: INKPOD_TREE_CREATE_LAYER,
                flags: 0,
                object_id: 0,
                parent_id: 0,
                destination_index: 0,
                kind: INKPOD_LAYER_VECTOR_COLORING,
                pixel_format: 0,
                opacity_milli: 0,
                name_utf8: name.as_ptr(),
                name_bytes: name.len() as u64,
            };
            let mut dispatch = InkpodDispatchResult {
                struct_size: size_of::<InkpodDispatchResult>() as u32,
                reserved: 0,
                revision: 0,
                accepted_command_count: 0,
            };
            let mut layer_id = 0;
            assert_eq!(
                inkpod_core_tree_edit(core, &edit, &mut dispatch, &mut layer_id),
                INKPOD_STATUS_OK
            );
            assert_ne!(layer_id, 0);
            let mut node = InkpodNodeInfo {
                struct_size: size_of::<InkpodNodeInfo>() as u32,
                ..InkpodNodeInfo::default()
            };
            assert_eq!(
                inkpod_core_node_get(core, 1, 1, &mut node),
                INKPOD_STATUS_OK
            );
            assert_eq!(node.kind, INKPOD_TYPED_PLANE_COLOR_TRACE);
            let trace_plane_id = node.id;
            assert_eq!(
                inkpod_core_node_get(core, 1, 2, &mut node),
                INKPOD_STATUS_OK
            );
            assert_eq!(node.kind, INKPOD_TYPED_PLANE_VECTOR_FILL);
            let fill_plane_id = node.id;

            let point = |x, y| InkpodVectorPoint { x, y };
            let line = |p0: InkpodVectorPoint, p3: InkpodVectorPoint| InkpodVectorCubicSegment {
                struct_size: size_of::<InkpodVectorCubicSegment>() as u32,
                reserved: 0,
                p0,
                p1: point((p0.x * 2.0 + p3.x) / 3.0, (p0.y * 2.0 + p3.y) / 3.0),
                p2: point((p0.x + p3.x * 2.0) / 3.0, (p0.y + p3.y * 2.0) / 3.0),
                p3,
                width_start: 1.0,
                width_end: 2.0,
            };
            let corners = [
                point(1.0, 1.0),
                point(7.0, 1.0),
                point(7.0, 7.0),
                point(1.0, 7.0),
                point(1.0, 1.0),
            ];
            let segments: Vec<_> = corners
                .windows(2)
                .map(|pair| line(pair[0], pair[1]))
                .collect();
            let path_input = InkpodVectorPathInput {
                struct_size: size_of::<InkpodVectorPathInput>() as u32,
                reserved: 0,
                flags: INKPOD_VECTOR_PATH_CLOSED,
                plane_id: trace_plane_id,
                color: InkpodColorValue {
                    struct_size: size_of::<InkpodColorValue>() as u32,
                    depth: INKPOD_COLOR_DEPTH_8,
                    red: 10,
                    green: 20,
                    blue: 30,
                    alpha: 255,
                },
                segments: segments.as_ptr(),
                segment_count: segments.len() as u64,
                segment_stride_bytes: size_of::<InkpodVectorCubicSegment>() as u64,
            };
            let mut path_id = 0;
            assert_eq!(
                inkpod_core_vector_add_path(core, &path_input, &mut dispatch, &mut path_id),
                INKPOD_STATUS_OK
            );
            assert_ne!(path_id, 0);
            let boundary_path_id = path_id;
            let mut short_segment = segments[0];
            short_segment.struct_size = size_of::<u32>() as u32;
            let short_input = InkpodVectorPathInput {
                segments: &short_segment,
                segment_count: 1,
                ..path_input
            };
            let revision = dispatch.revision;
            let mut rejected_path_id = u64::MAX;
            assert_eq!(
                inkpod_core_vector_add_path(
                    core,
                    &short_input,
                    &mut dispatch,
                    &mut rejected_path_id,
                ),
                INKPOD_STATUS_INCOMPATIBLE_ABI
            );
            assert_eq!(rejected_path_id, 0);
            assert_eq!(dispatch.revision, revision);

            let mut too_thin_segments = segments.clone();
            too_thin_segments[0].width_start = 0.0001;
            too_thin_segments[0].width_end = 0.0001;
            let too_thin_input = InkpodVectorPathInput {
                segments: too_thin_segments.as_ptr(),
                ..path_input
            };
            rejected_path_id = u64::MAX;
            assert_eq!(
                inkpod_core_vector_add_path(
                    core,
                    &too_thin_input,
                    &mut dispatch,
                    &mut rejected_path_id,
                ),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            assert_eq!(rejected_path_id, 0);
            assert_eq!(dispatch.revision, revision);

            let fill_input = InkpodVectorFillInput {
                struct_size: size_of::<InkpodVectorFillInput>() as u32,
                reserved: 0,
                feature_flags: 0,
                plane_id: fill_plane_id,
                color: InkpodColorValue {
                    struct_size: size_of::<InkpodColorValue>() as u32,
                    depth: INKPOD_COLOR_DEPTH_16,
                    red: 60_000,
                    green: 1_000,
                    blue: 2_000,
                    alpha: 50_000,
                },
                boundary_path_ids: &boundary_path_id,
                boundary_path_count: 1,
            };
            let mut fill_id = 0;
            assert_eq!(
                inkpod_core_vector_add_fill(core, &fill_input, &mut dispatch, &mut fill_id),
                INKPOD_STATUS_OK
            );
            assert_ne!(fill_id, 0);

            let selection_input = InkpodVectorSelectionInput {
                struct_size: size_of::<InkpodVectorSelectionInput>() as u32,
                mode: INKPOD_VECTOR_SELECT_FULLY_CONTAINED,
                feature_flags: 0,
                bounds: InkpodFrameRect {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 8,
                },
            };
            let mut selection_output = InkpodVectorSelectionBuffer {
                struct_size: size_of::<InkpodVectorSelectionBuffer>() as u32,
                reserved: 0,
                ranges: ptr::null_mut(),
                range_capacity: 0,
                range_count: 0,
                fill_ids: ptr::null_mut(),
                fill_capacity: 0,
                fill_count: 0,
            };
            assert_eq!(
                inkpod_core_vector_select(core, &selection_input, &mut selection_output),
                INKPOD_STATUS_BUFFER_TOO_SMALL
            );
            assert_eq!(selection_output.range_count, 1);
            let mut selection_ranges = [InkpodVectorSelectionRange {
                struct_size: 0,
                reserved: u32::MAX,
                path_id: 0,
                start_million: u32::MAX,
                end_million: 0,
            }];
            selection_output.ranges = selection_ranges.as_mut_ptr();
            selection_output.range_capacity = selection_ranges.len() as u64;
            assert_eq!(
                inkpod_core_vector_select(core, &selection_input, &mut selection_output),
                INKPOD_STATUS_OK
            );
            assert_eq!(selection_ranges[0].path_id, path_id);
            assert_eq!(selection_ranges[0].start_million, 0);
            assert_eq!(selection_ranges[0].end_million, 1_000_000);

            let rasterize_input = InkpodVectorRasterizeInput {
                struct_size: size_of::<InkpodVectorRasterizeInput>() as u32,
                reserved: 0,
                feature_flags: INKPOD_VECTOR_RASTERIZE_ANTIALIAS,
                layer_id,
                scale: 2,
                reserved_2: 0,
            };
            let mut raster_output = InkpodVectorRasterBuffer {
                struct_size: size_of::<InkpodVectorRasterBuffer>() as u32,
                reserved: 0,
                pixels: ptr::null_mut(),
                pixel_capacity: 0,
                required_bytes: 0,
                width: 0,
                height: 0,
                stride_bytes: 0,
                reserved_2: 0,
            };
            assert_eq!(
                inkpod_core_vector_rasterize(core, &rasterize_input, &mut raster_output),
                INKPOD_STATUS_OK
            );
            assert_eq!((raster_output.width, raster_output.height), (16, 16));
            assert_eq!(raster_output.required_bytes, 16 * 16 * 4);
            let mut raster_pixels = vec![0_u8; raster_output.required_bytes as usize];
            raster_output.pixels = raster_pixels.as_mut_ptr();
            raster_output.pixel_capacity = raster_pixels.len() as u64;
            assert_eq!(
                inkpod_core_vector_rasterize(core, &rasterize_input, &mut raster_output),
                INKPOD_STATUS_OK
            );
            assert!(raster_pixels.iter().any(|value| *value != 0));

            let rasterize_layer_input = InkpodVectorRasterizeInput {
                scale: 1,
                ..rasterize_input
            };
            let rasterized_name = b"Rasterized";
            let mut raster_layer_id = 0_u64;
            assert_eq!(
                inkpod_core_vector_rasterize_to_layer(
                    core,
                    &rasterize_layer_input,
                    rasterized_name.as_ptr(),
                    rasterized_name.len() as u64,
                    &mut dispatch,
                    &mut raster_layer_id,
                ),
                INKPOD_STATUS_OK
            );
            assert_ne!(raster_layer_id, 0);
            assert_eq!(dispatch.accepted_command_count, 1);

            assert_eq!(
                inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR),
                INKPOD_STATUS_OK
            );
            let sample = InkpodStrokeSample {
                struct_size: size_of::<InkpodStrokeSample>() as u32,
                flags: 0,
                x: 3.0,
                y: 3.0,
                pressure: 1.0,
                reserved: 0,
            };
            let stroke = InkpodStrokeInput {
                struct_size: size_of::<InkpodStrokeInput>() as u32,
                tool: INKPOD_TOOL_PENCIL,
                plane: INKPOD_PLANE_COLOR,
                coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
                flags: 0,
                color_rgba: 0x0102_03ff,
                diameter: 1.0,
                samples: &sample,
                sample_count: 1,
                sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
            };
            assert_eq!(
                inkpod_core_apply_stroke(core, &stroke, &mut dispatch),
                INKPOD_STATUS_OK
            );
            let vectorize_input = InkpodRasterVectorizeInput {
                struct_size: size_of::<InkpodRasterVectorizeInput>() as u32,
                alpha_threshold: 1,
                feature_flags: 0,
                source_plane_id: info.color_plane_id,
                target_layer_id: layer_id,
            };
            let mut vectorized_fill_count = 0;
            let vector_source_input = InkpodRasterVectorizeInput {
                source_plane_id: trace_plane_id,
                ..vectorize_input
            };
            assert_eq!(
                inkpod_core_raster_vectorize(
                    core,
                    &vector_source_input,
                    &mut dispatch,
                    &mut vectorized_fill_count,
                ),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            assert_eq!(vectorized_fill_count, 0);
            assert_eq!(
                inkpod_core_raster_vectorize(
                    core,
                    &vectorize_input,
                    &mut dispatch,
                    &mut vectorized_fill_count,
                ),
                INKPOD_STATUS_OK
            );
            assert_eq!(vectorized_fill_count, 1);

            let options = InkpodSnapshotOptions {
                struct_size: size_of::<InkpodSnapshotOptions>() as u32,
                reserved: 0,
                feature_flags: 0,
            };
            let mut snapshot = ptr::null_mut();
            assert_eq!(
                inkpod_core_build_snapshot(core, &options, &mut snapshot),
                INKPOD_STATUS_OK
            );
            let mut vectors = InkpodSnapshotVectorView {
                struct_size: size_of::<InkpodSnapshotVectorView>() as u32,
                abi_version: 0,
                feature_flags: u64::MAX,
                segments: ptr::null(),
                segment_count: 0,
                segment_stride_bytes: 0,
                fills: ptr::null(),
                fill_count: 0,
                fill_stride_bytes: 0,
                boundary_path_ids: ptr::null(),
                boundary_path_count: 0,
            };
            assert_eq!(
                inkpod_snapshot_get_vectors(snapshot, &mut vectors),
                INKPOD_STATUS_OK
            );
            assert_eq!(vectors.abi_version, INKPOD_ABI_VERSION);
            assert_eq!(vectors.segment_count, 8);
            assert_eq!(vectors.fill_count, 2);
            assert_eq!(vectors.boundary_path_count, 2);
            assert!(!vectors.segments.is_null() && !vectors.fills.is_null());
            assert_eq!((*vectors.segments).path_id, boundary_path_id);
            assert_eq!((*vectors.fills).fill_id, fill_id);
            assert_eq!(*vectors.boundary_path_ids, boundary_path_id);
            assert_eq!(inkpod_snapshot_release(&mut snapshot), INKPOD_STATUS_OK);
            assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        }
    }

    #[test]
    fn m6_filter_effect_adjustment_and_alpha_records_are_copied_and_atomic() {
        unsafe {
            let config = InkpodCoreConfig {
                struct_size: size_of::<InkpodCoreConfig>() as u32,
                abi_version: INKPOD_ABI_VERSION,
                feature_flags: 0,
            };
            let mut core = ptr::null_mut();
            assert_eq!(inkpod_core_create(&config, &mut core), INKPOD_STATUS_OK);
            let options = InkpodCellCreateOptions {
                struct_size: size_of::<InkpodCellCreateOptions>() as u32,
                reserved: 0,
                feature_flags: 0,
                document_uuid_high: 0x4d36_0000_0000_0001,
                document_uuid_low: 0x4d36_0000_0000_0002,
                width: 4,
                height: 4,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
            };
            let mut document = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..InkpodDocumentInfo::default()
            };
            assert_eq!(
                inkpod_core_new_cell(core, &options, &mut document),
                INKPOD_STATUS_OK
            );
            let original = document.color_plane_checksum;
            let mut filter = InkpodFilterInput {
                struct_size: size_of::<InkpodFilterInput>() as u32,
                kind: INKPOD_FILTER_INVERT,
                feature_flags: 0,
                plane_id: document.color_plane_id,
                channel: INKPOD_FILTER_CHANNEL_RGB,
                interpolation: 0,
                parameter_0: 0,
                parameter_1: 0,
                parameter_2: 0,
                parameter_3: 0,
                parameter_4: 0,
                point_stride_bytes: 0,
                points: ptr::null(),
                point_count: 0,
            };
            let mut preview = InkpodFilterPreviewInfo {
                struct_size: size_of::<InkpodFilterPreviewInfo>() as u32,
                reserved: 0,
                plane_id: 0,
                base_checksum: 0,
                preview_checksum: 0,
                preview_revision: 0,
            };
            let mut short = filter;
            short.struct_size = size_of::<u32>() as u32;
            assert_eq!(
                inkpod_core_filter_preview_begin(core, &short, &mut preview),
                INKPOD_STATUS_INCOMPATIBLE_ABI
            );
            assert_eq!(
                inkpod_core_filter_preview_begin(core, &filter, &mut preview),
                INKPOD_STATUS_OK
            );
            assert_eq!(preview.base_checksum, original);
            assert_ne!(preview.preview_checksum, original);
            assert_eq!(
                inkpod_core_filter_preview_cancel(core, &mut preview),
                INKPOD_STATUS_OK
            );
            assert_eq!(preview.preview_checksum, original);
            assert_eq!(
                inkpod_core_filter_preview_begin(core, &filter, &mut preview),
                INKPOD_STATUS_OK
            );
            let mut dispatch = InkpodDispatchResult {
                struct_size: size_of::<InkpodDispatchResult>() as u32,
                reserved: 0,
                revision: 0,
                accepted_command_count: 0,
            };
            assert_eq!(
                inkpod_core_filter_preview_apply(core, &mut dispatch),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_core_undo(core, &mut dispatch), INKPOD_STATUS_OK);
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            assert_eq!(document.color_plane_checksum, original);

            let curve_points = [
                InkpodCurvePoint {
                    struct_size: size_of::<InkpodCurvePoint>() as u32,
                    reserved: 0,
                    input: 0,
                    output: 0,
                },
                InkpodCurvePoint {
                    struct_size: size_of::<InkpodCurvePoint>() as u32,
                    reserved: 0,
                    input: 32_768,
                    output: 40_000,
                },
                InkpodCurvePoint {
                    struct_size: size_of::<InkpodCurvePoint>() as u32,
                    reserved: 0,
                    input: 65_535,
                    output: 65_535,
                },
            ];
            filter.kind = INKPOD_FILTER_TONE_CURVE;
            filter.channel = INKPOD_FILTER_CHANNEL_RGB;
            filter.interpolation = INKPOD_CURVE_BEZIER;
            filter.points = curve_points.as_ptr();
            filter.point_count = curve_points.len() as u64;
            filter.point_stride_bytes = (size_of::<InkpodCurvePoint>() - 1) as u32;
            assert_eq!(
                inkpod_core_filter_preview_begin(core, &filter, &mut preview),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            filter.point_stride_bytes = size_of::<InkpodCurvePoint>() as u32;
            let mut oversized_points = curve_points;
            oversized_points[0].struct_size = (size_of::<InkpodCurvePoint>() + 8) as u32;
            filter.points = oversized_points.as_ptr();
            assert_eq!(
                inkpod_core_filter_preview_begin(core, &filter, &mut preview),
                INKPOD_STATUS_INCOMPATIBLE_ABI
            );
            filter.points = curve_points.as_ptr();
            assert_eq!(
                inkpod_core_filter_preview_begin(core, &filter, &mut preview),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_filter_preview_cancel(core, &mut preview),
                INKPOD_STATUS_OK
            );
            filter.point_stride_bytes = 0;
            assert_eq!(
                inkpod_core_filter_preview_begin(core, &filter, &mut preview),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_filter_preview_cancel(core, &mut preview),
                INKPOD_STATUS_OK
            );

            filter.kind = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
            filter.interpolation = 0;
            filter.parameter_0 = 100;
            filter.parameter_1 = 200;
            filter.points = ptr::null();
            filter.point_count = 0;
            filter.point_stride_bytes = 0;
            let name = b"M6 Adjustment";
            let mut layer_id = 0;
            assert_eq!(
                inkpod_core_adjustment_create(
                    core,
                    &filter,
                    name.as_ptr(),
                    name.len() as u64,
                    &mut dispatch,
                    &mut layer_id,
                ),
                INKPOD_STATUS_OK
            );
            assert_ne!(layer_id, 0);

            filter.parameter_0 = 200;
            filter.parameter_1 = -100;
            assert_eq!(
                inkpod_core_adjustment_update(core, layer_id, &filter, &mut dispatch),
                INKPOD_STATUS_OK
            );

            let color16 = |red, green, blue, alpha| InkpodColorValue {
                struct_size: size_of::<InkpodColorValue>() as u32,
                depth: INKPOD_COLOR_DEPTH_16,
                red,
                green,
                blue,
                alpha,
            };
            let stops = [
                InkpodGradientStop {
                    struct_size: size_of::<InkpodGradientStop>() as u32,
                    reserved: 0,
                    position_milli: 0,
                    reserved_2: 0,
                    color: color16(65_535, 0, 0, 65_535),
                },
                InkpodGradientStop {
                    struct_size: size_of::<InkpodGradientStop>() as u32,
                    reserved: 0,
                    position_milli: 500,
                    reserved_2: 0,
                    color: color16(0, 65_535, 0, 32_768),
                },
                InkpodGradientStop {
                    struct_size: size_of::<InkpodGradientStop>() as u32,
                    reserved: 0,
                    position_milli: 1_000,
                    reserved_2: 0,
                    color: color16(0, 0, 65_535, 65_535),
                },
            ];
            let gradient = InkpodGradientInput {
                struct_size: size_of::<InkpodGradientInput>() as u32,
                kind: INKPOD_GRADIENT_LINEAR,
                feature_flags: 0,
                plane_id: document.color_plane_id,
                mode: INKPOD_GRADIENT_OVERWRITE,
                dither: 0,
                start_x_milli: 500,
                start_y_milli: 500,
                end_x_milli: 3_500,
                end_y_milli: 500,
                stops: stops.as_ptr(),
                stop_count: stops.len() as u64,
                stop_stride_bytes: size_of::<InkpodGradientStop>() as u64,
            };
            assert_eq!(
                inkpod_core_effect_gradient(core, &gradient, &mut dispatch),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            assert_ne!(document.color_plane_checksum, original);

            let airbrush = InkpodAirbrushInput {
                struct_size: size_of::<InkpodAirbrushInput>() as u32,
                reserved: 0,
                feature_flags: 0,
                plane_id: document.color_plane_id,
                center_x_milli: 2_000,
                center_y_milli: 2_000,
                radius_milli: 1_500,
                hardness_milli: 500,
                opacity_milli: 500,
                reserved_2: 0,
                color: color16(65_535, 65_535, 65_535, 65_535),
            };
            assert_eq!(
                inkpod_core_effect_airbrush(core, &airbrush, &mut dispatch),
                INKPOD_STATUS_OK
            );

            let boundary_colors = [color16(65_535, 0, 0, 65_535), color16(0, 0, 65_535, 65_535)];
            let boundary = InkpodBoundaryAirbrushInput {
                struct_size: size_of::<InkpodBoundaryAirbrushInput>() as u32,
                reserved: 0,
                feature_flags: 0,
                plane_id: document.color_plane_id,
                width: 1,
                strength_milli: 1_000,
                colors: InkpodColorArray {
                    struct_size: size_of::<InkpodColorArray>() as u32,
                    reserved: 0,
                    feature_flags: 0,
                    colors: boundary_colors.as_ptr(),
                    color_count: boundary_colors.len() as u64,
                    color_stride_bytes: size_of::<InkpodColorValue>() as u64,
                },
            };
            assert_eq!(
                inkpod_core_effect_boundary_airbrush(core, &boundary, &mut dispatch),
                INKPOD_STATUS_OK
            );

            let blur = InkpodBlurEffectInput {
                struct_size: size_of::<InkpodBlurEffectInput>() as u32,
                reserved: 0,
                feature_flags: 0,
                plane_id: document.color_plane_id,
                radius: 1,
                strength_milli: 500,
                reserved_2: 0,
                reserved_3: 0,
            };
            assert_eq!(
                inkpod_core_effect_blur(core, &blur, &mut dispatch),
                INKPOD_STATUS_OK
            );

            let stamp = InkpodStampInput {
                struct_size: size_of::<InkpodStampInput>() as u32,
                reserved: 0,
                feature_flags: 0,
                plane_id: document.color_plane_id,
                source_x: 0,
                source_y: 0,
                destination_x: 2,
                destination_y: 2,
                width: 2,
                height: 2,
                opacity_milli: 1_000,
                reserved_2: 0,
            };
            assert_eq!(
                inkpod_core_effect_stamp(core, &stamp, &mut dispatch),
                INKPOD_STATUS_OK
            );

            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            let before_alpha = document.color_plane_checksum;
            let alpha_pixels = [64_u8; 16];
            let mut alpha = InkpodAlphaEditInput {
                struct_size: size_of::<InkpodAlphaEditInput>() as u32,
                pixel_format: INKPOD_STORAGE_GRAYSCALE8,
                feature_flags: 0,
                plane_id: document.color_plane_id,
                width: 4,
                height: 4,
                reserved: 0,
                reserved_2: 0,
                pixels: alpha_pixels.as_ptr(),
                pixel_bytes: alpha_pixels.len() as u64,
                row_stride_bytes: 4,
            };
            alpha.row_stride_bytes = 3;
            assert_eq!(
                inkpod_core_alpha_edit(core, &alpha, &mut dispatch),
                INKPOD_STATUS_INVALID_ARGUMENT
            );
            alpha.row_stride_bytes = 4;
            assert_eq!(
                inkpod_core_alpha_edit(core, &alpha, &mut dispatch),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            assert_ne!(document.color_plane_checksum, before_alpha);
            assert_eq!(inkpod_core_undo(core, &mut dispatch), INKPOD_STATUS_OK);
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            assert_eq!(document.color_plane_checksum, before_alpha);
            assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        }
    }

    #[test]
    fn m6_gesture_dust_task_ownership_and_cancel_are_connected() {
        let mut core = ptr::null_mut();
        // SAFETY: Every record and borrowed span remains live for its call.
        unsafe {
            assert_eq!(inkpod_core_create(&config(), &mut core), INKPOD_STATUS_OK);
            let options = InkpodCellCreateOptions {
                struct_size: size_of::<InkpodCellCreateOptions>() as u32,
                reserved: 0,
                feature_flags: 0,
                document_uuid_high: 61,
                document_uuid_low: 62,
                width: 8,
                height: 8,
                dpi_x_milli: 96_000,
                dpi_y_milli: 96_000,
            };
            let mut document = InkpodDocumentInfo {
                struct_size: size_of::<InkpodDocumentInfo>() as u32,
                ..InkpodDocumentInfo::default()
            };
            assert_eq!(
                inkpod_core_new_cell(core, &options, &mut document),
                INKPOD_STATUS_OK
            );
            let mut dispatch = InkpodDispatchResult {
                struct_size: size_of::<InkpodDispatchResult>() as u32,
                reserved: 0,
                revision: 0,
                accepted_command_count: 0,
            };
            let samples = [
                InkpodStrokeSample {
                    struct_size: size_of::<InkpodStrokeSample>() as u32,
                    flags: 0,
                    x: 2.0,
                    y: 2.0,
                    pressure: 0.25,
                    reserved: 0,
                },
                InkpodStrokeSample {
                    struct_size: size_of::<InkpodStrokeSample>() as u32,
                    flags: 0,
                    x: 6.0,
                    y: 2.0,
                    pressure: 1.0,
                    reserved: 0,
                },
            ];
            let airbrush = InkpodAirbrushGestureInput {
                struct_size: size_of::<InkpodAirbrushGestureInput>() as u32,
                coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
                feature_flags: INKPOD_EFFECT_FLAG_PRESSURE_SIZE
                    | INKPOD_EFFECT_FLAG_PRESSURE_OPACITY,
                plane_id: document.color_plane_id,
                view_id: 0,
                radius_milli: 1_500,
                hardness_milli: 500,
                spacing_milli: 500,
                opacity_milli: 1_000,
                fade_milli: 100,
                continuous_dabs: 2,
                color: InkpodColorValue {
                    struct_size: size_of::<InkpodColorValue>() as u32,
                    depth: INKPOD_COLOR_DEPTH_16,
                    red: 65_535,
                    green: 0,
                    blue: 0,
                    alpha: 65_535,
                },
                samples: samples.as_ptr(),
                sample_count: samples.len() as u64,
                sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
            };
            assert_eq!(
                inkpod_core_effect_airbrush_gesture(core, &airbrush, &mut dispatch),
                INKPOD_STATUS_OK
            );

            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            let before_cancel = document.color_plane_checksum;
            let filter = InkpodFilterInput {
                struct_size: size_of::<InkpodFilterInput>() as u32,
                kind: INKPOD_FILTER_INVERT,
                feature_flags: 0,
                plane_id: document.color_plane_id,
                channel: INKPOD_FILTER_CHANNEL_RGB,
                interpolation: INKPOD_CURVE_BEZIER,
                parameter_0: 0,
                parameter_1: 0,
                parameter_2: 0,
                parameter_3: 0,
                parameter_4: 0,
                point_stride_bytes: 0,
                points: ptr::null(),
                point_count: 0,
            };
            let mut task = ptr::null_mut();
            assert_eq!(inkpod_m6_task_create(&mut task), INKPOD_STATUS_OK);
            assert_eq!(inkpod_m6_task_cancel(task), INKPOD_STATUS_OK);
            let mut preview = InkpodFilterPreviewInfo {
                struct_size: size_of::<InkpodFilterPreviewInfo>() as u32,
                reserved: 0,
                plane_id: 0,
                base_checksum: 0,
                preview_checksum: 0,
                preview_revision: 0,
            };
            assert_eq!(
                inkpod_core_filter_preview_begin_task(core, &filter, task, &mut preview),
                INKPOD_STATUS_CANCELLED
            );
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            assert_eq!(document.color_plane_checksum, before_cancel);
            let mut task_info = InkpodM6TaskInfo {
                struct_size: size_of::<InkpodM6TaskInfo>() as u32,
                state: 99,
                completed_work: 0,
                total_work: 0,
                reserved: 99,
            };
            assert_eq!(inkpod_m6_task_query(task, &mut task_info), INKPOD_STATUS_OK);
            assert_eq!(task_info.state, INKPOD_M6_TASK_CANCELLED);
            assert_eq!(inkpod_m6_task_release(&mut task), INKPOD_STATUS_OK);
            assert_eq!(inkpod_m6_task_release(&mut task), INKPOD_STATUS_OK);

            assert_eq!(inkpod_m6_task_create(&mut task), INKPOD_STATUS_OK);
            assert_eq!(
                inkpod_core_filter_preview_begin_task(core, &filter, task, &mut preview),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_core_filter_preview_apply(core, &mut dispatch),
                INKPOD_STATUS_OK
            );
            assert_eq!(inkpod_m6_task_release(&mut task), INKPOD_STATUS_OK);
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            let before_cancelled_last = document.color_plane_checksum;
            assert_eq!(inkpod_m6_task_create(&mut task), INKPOD_STATUS_OK);
            assert_eq!(inkpod_m6_task_cancel(task), INKPOD_STATUS_OK);
            assert_eq!(
                inkpod_core_filter_apply_last_task(
                    core,
                    document.color_plane_id,
                    task,
                    &mut dispatch
                ),
                INKPOD_STATUS_CANCELLED
            );
            assert_eq!(
                inkpod_core_get_document_info(core, &mut document),
                INKPOD_STATUS_OK
            );
            assert_eq!(document.color_plane_checksum, before_cancelled_last);
            assert_eq!(inkpod_m6_task_release(&mut task), INKPOD_STATUS_OK);

            let mut dust_task = ptr::null_mut();
            assert_eq!(inkpod_m6_task_create(&mut dust_task), INKPOD_STATUS_OK);
            let dust = InkpodDustInput {
                struct_size: size_of::<InkpodDustInput>() as u32,
                mode: INKPOD_DUST_REMOVE_FOREGROUND,
                feature_flags: 0,
                plane_id: document.color_plane_id,
                view_id: 0,
                coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
                shape: INKPOD_SELECTION_RECTANGLE,
                maximum_pixels: 1,
                use_region: 1,
                diameter: 1.0,
                samples: samples.as_ptr(),
                sample_count: samples.len() as u64,
                sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
            };
            assert_eq!(
                inkpod_core_dust_remove(core, &dust, dust_task, &mut dispatch),
                INKPOD_STATUS_OK
            );
            assert_eq!(
                inkpod_m6_task_query(dust_task, &mut task_info),
                INKPOD_STATUS_OK
            );
            assert_eq!(task_info.state, INKPOD_M6_TASK_COMPLETED);
            assert!(task_info.total_work > 0);
            assert_eq!(inkpod_m6_task_release(&mut dust_task), INKPOD_STATUS_OK);
            assert_eq!(inkpod_core_destroy(&mut core), INKPOD_STATUS_OK);
        }
    }
}
