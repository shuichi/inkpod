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
#[derive(Clone, Copy, Default)]
pub struct InkpodShortcutStroke {
    pub virtual_key: u32,
    pub modifiers: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodShortcutSequence {
    pub struct_size: u32,
    pub command_id: u32,
    pub stroke_count: u32,
    pub reserved: u32,
    pub strokes: [InkpodShortcutStroke; 4],
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
#[derive(Default)]
pub struct InkpodResourceUsage {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub document_tile_bytes: u64,
    pub document_tile_count: u64,
    pub history_bytes: u64,
    pub history_entry_count: u64,
    pub render_cache_bytes: u64,
    pub render_cache_tile_count: u64,
    pub cpu_staging_bytes: u64,
    pub reference_light_table_bytes: u64,
    pub reference_light_table_tile_count: u64,
    pub sequence_source_bytes: u64,
    pub sequence_source_tile_count: u64,
    pub thumbnail_cache_bytes: u64,
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
pub struct InkpodColorChartEntry {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub color: InkpodColorValue,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
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
#[derive(Clone, Copy)]
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
