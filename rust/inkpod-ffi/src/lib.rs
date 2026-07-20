#![deny(unsafe_op_in_unsafe_fn)]

use inkpod_core::{
    ActivePlane, ClipboardPayload, ColorCheckMode, Command, CoordinateSpace, Core, CoreError,
    DocumentInfo, EyedropperSource, FillOperation, FillRequest, FloatingTransform, GridConfig,
    GuideAxis, InclusionMode, LayerKind, MirrorAxis, PaintTool, PixelFormat, PixelValue, PlaneType,
    PointF32, RectI32, RenderSnapshot, SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE,
    SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA, SelectionLayerOperation, SelectionOperation,
    SelectionShape, ShortcutBinding, Stroke, StrokeSample, ViewCommand,
};
use std::cell::RefCell;
use std::mem::{align_of, size_of};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr;
use std::slice;
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
pub const INKPOD_VIEW_SET_TRANSPARENT_VISIBLE: u32 = 13;
pub const INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL: u32 = 1 << 0;
pub const INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL: u32 = 1 << 1;
pub const INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE: u32 = 1 << 0;
pub const INKPOD_SNAPSHOT_OVERLAY_GUIDES_VISIBLE: u32 = 1 << 1;
pub const INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE: u32 = 1 << 2;
pub const INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED: u32 = 1 << 3;
pub const INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW: u32 = 1 << 4;
pub const INKPOD_SHORTCUT_MODIFIER_CONTROL: u32 = 1 << 0;
pub const INKPOD_SHORTCUT_MODIFIER_SHIFT: u32 = 1 << 1;
pub const INKPOD_SHORTCUT_MODIFIER_ALT: u32 = 1 << 2;
pub const INKPOD_SHORTCUT_MODIFIER_EXTENDED: u32 = 1 << 3;
pub const INKPOD_COLOR_DEPTH_8: u32 = 8;
pub const INKPOD_COLOR_DEPTH_16: u32 = 16;
pub const INKPOD_FILL_SEED: u32 = 1;
pub const INKPOD_FILL_CLOSED_REGION: u32 = 2;
pub const INKPOD_FILL_EXTENSION: u32 = 3;
pub const INKPOD_FILL_FLAG_DETACHED_REGIONS: u64 = 1 << 0;
pub const INKPOD_FILL_FLAG_OVERFLOW_ABORT: u64 = 1 << 1;
pub const INKPOD_FILL_FLAG_TRANSPARENT_ONLY: u64 = 1 << 2;
pub const INKPOD_FILL_FLAG_SELECTION_PRESENT: u64 = 1 << 3;
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
pub const INKPOD_TYPED_PLANE_MAIN_LINE: u32 = 1;
pub const INKPOD_TYPED_PLANE_COLOR: u32 = 2;
pub const INKPOD_TYPED_PLANE_RASTER: u32 = 3;
pub const INKPOD_TYPED_PLANE_SELECTION: u32 = 4;
pub const INKPOD_STORAGE_BINARY8: u32 = 1;
pub const INKPOD_STORAGE_GRAYSCALE8: u32 = 2;
pub const INKPOD_STORAGE_GRAYSCALE16: u32 = 3;
pub const INKPOD_STORAGE_RGBA8: u32 = 4;
pub const INKPOD_STORAGE_RGBA16: u32 = 5;
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

pub struct InkpodCore {
    owner_thread: ThreadId,
    core: Core,
}

pub struct InkpodSnapshot {
    snapshot: RenderSnapshot,
    tiles: Box<[InkpodSnapshotTile]>,
    guides: Box<[InkpodSnapshotGuide]>,
}

pub struct InkpodClipboard {
    payload: ClipboardPayload,
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
    Box::new(InkpodSnapshot {
        snapshot,
        tiles,
        guides,
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
    }
}

fn parse_plane_type(value: u32) -> Result<PlaneType, u32> {
    match value {
        INKPOD_TYPED_PLANE_MAIN_LINE => Ok(PlaneType::MainLine),
        INKPOD_TYPED_PLANE_COLOR => Ok(PlaneType::Color),
        INKPOD_TYPED_PLANE_RASTER => Ok(PlaneType::Raster),
        INKPOD_TYPED_PLANE_SELECTION => Ok(PlaneType::Selection),
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

// SAFETY: `color` must expose a complete, readable InkpodColorValue prefix.
unsafe fn parse_color_value(color: *const InkpodColorValue) -> Result<PixelValue, u32> {
    // SAFETY: Forwarded from this helper's caller contract.
    unsafe { validate_struct(color, "InkpodColorValue") }?;
    // SAFETY: The complete known structure is readable after validation.
    let color = unsafe { &*color };
    match color.depth {
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
        | INKPOD_FILL_FLAG_SELECTION_PRESENT;
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
        match core.core.apply_fill(&request) {
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
            let info = core.document_info().map_err(map_core_error)?;
            ViewCommand::BoxZoom {
                document_rect: RectI32 {
                    x: input.value1 as i32,
                    y: input.value2 as i32,
                    width: input.value3 as i32,
                    height: input.value4 as i32,
                },
                viewport_width: f64::from(info.width),
                viewport_height: f64::from(info.height),
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
        INKPOD_VIEW_SET_TRANSPARENT_VISIBLE => ViewCommand::SetTransparentView(input.value1 != 0.0),
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
        }) | if view.transparent_view() {
            INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW
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
}
