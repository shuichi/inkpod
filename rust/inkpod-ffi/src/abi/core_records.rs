#[repr(C)]
pub struct InkpodCoreConfig {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
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
#[derive(Clone, Copy)]
pub struct InkpodCellCreationOptions {
    pub struct_size: u32,
    pub sizing_mode: u32,
    pub feature_flags: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub margin_milli: u32,
    pub safe_frame_ratio_milli: u32,
    pub maximum_close_ratio_milli: u32,
    pub anchor: u32,
    pub initial_layer_kind: u32,
    pub pixel_format: u32,
    pub count: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InkpodFrameRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InkpodCellCreationPlanItem {
    pub struct_size: u32,
    pub sizing_mode: u32,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub initial_layer_kind: u32,
    pub pixel_format: u32,
    pub hundred_frame: InkpodFrameRect,
    pub reference_frame: InkpodFrameRect,
    pub drawing_frame: InkpodFrameRect,
    pub safe_frame: InkpodFrameRect,
    pub shooting_frame: InkpodFrameRect,
    pub maximum_close_frame: InkpodFrameRect,
    pub margin_left: u32,
    pub margin_top: u32,
    pub margin_right: u32,
    pub margin_bottom: u32,
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
    pub shooting_frame: InkpodFrameRect,
    pub maximum_close_frame: InkpodFrameRect,
    pub margin_left: u32,
    pub margin_top: u32,
    pub margin_right: u32,
    pub margin_bottom: u32,
    pub active_plane: u32,
    pub reserved: u32,
    pub main_plane_checksum: u64,
    pub color_plane_checksum: u64,
    pub cell_id: u64,
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
    pub shooting_frame: InkpodFrameRect,
    pub maximum_close_frame: InkpodFrameRect,
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
    pub shape: u32,
    pub smoothing: u16,
    pub reserved_2: u16,
    pub start_color: u32,
    pub reserved_3: u32,
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
pub struct InkpodReplayContract {
    pub struct_size: u32,
    pub replay_epoch: u32,
    pub procedure_format_version: u32,
    pub canonical_numeric_version: u32,
    pub primitive_count: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub primitive_catalog_digest: [u8; 32],
}

#[repr(C)]
pub struct InkpodCanonicalDigest {
    pub struct_size: u32,
    pub algorithm: u32,
    pub bytes: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodPersistenceInfo {
    pub struct_size: u32,
    pub format_version: u32,
    pub open_strategy: u32,
    pub flags: u32,
    pub feature_flags: u64,
    pub journal_event_count: u64,
    pub procedure_count: u64,
    pub replay_work: u64,
    pub dirty_bytes: u64,
    pub asset_count: u64,
    pub asset_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodCompactionPlan {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub history_event_count: u64,
    pub history_procedure_count: u64,
    pub document_digest: [u8; 32],
    pub editor_digest: [u8; 32],
    pub journal_digest: [u8; 32],
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
#[derive(Clone, Copy, Default)]
pub struct InkpodColorChartInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub feature_flags: u64,
    pub entry_count: u64,
    pub selected_index: u64,
    pub page: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodColorChartPreviewSummary {
    pub struct_size: u32,
    pub flags: u32,
    pub feature_flags: u64,
    pub base_document_revision: u64,
    pub entry_count: u64,
    pub source_unique_color_count: u64,
    pub retained_color_count: u32,
    pub added_color_count: u32,
    pub removed_color_count: u32,
    pub reserved: u32,
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
#[derive(Clone, Copy)]
pub struct InkpodSnapshotRenderPass {
    pub struct_size: u32,
    pub kind: u32,
    pub layer_id: u64,
    pub plane_id: u64,
    pub opacity_milli: u32,
    pub reserved: u32,
    pub first_item: u64,
    pub item_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodSnapshotRenderPlan {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
    pub passes: *const InkpodSnapshotRenderPass,
    pub pass_count: u64,
    pub pass_stride_bytes: u64,
    pub adjustment_luts_rgb8: *const u8,
    pub adjustment_lut_count: u64,
    pub adjustment_lut_stride_bytes: u64,
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
