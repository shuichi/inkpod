use super::*;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodAnnotationPoint {
    pub struct_size: u32,
    pub reserved: u32,
    pub x_milli: i32,
    pub y_milli: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodAnnotationObjectInput {
    pub struct_size: u32,
    pub kind: u32,
    pub feature_flags: u64,
    pub layer_id: u64,
    pub output: u32,
    pub style_flags: u32,
    pub bounds: InkpodFrameRect,
    pub font_family_utf8: *const u8,
    pub font_family_bytes: u64,
    pub font_size_milli: u32,
    pub stroke_width_milli: u32,
    pub color: InkpodColorValue,
    pub text_utf8: *const u8,
    pub text_bytes: u64,
    pub points: *const InkpodAnnotationPoint,
    pub point_count: u64,
    pub point_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodAnnotationEdit {
    pub struct_size: u32,
    pub kind: u32,
    pub feature_flags: u64,
    pub object_id: u64,
    pub input: *const InkpodAnnotationObjectInput,
    pub delta_x: i32,
    pub delta_y: i32,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodAnnotationEditResult {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub revision: u64,
    pub created_ids: *mut u64,
    pub created_capacity: u64,
    pub created_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodAnnotationStrokeInput {
    pub struct_size: u32,
    pub output: u32,
    pub feature_flags: u64,
    pub base_document_revision: u64,
    pub layer_id: u64,
    pub color: InkpodColorValue,
    pub stroke_width_milli: u32,
    pub reserved: u32,
    pub start: InkpodAnnotationPoint,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodShootingFrameInput {
    pub struct_size: u32,
    pub anchor: u32,
    pub feature_flags: u64,
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_degrees: f64,
    pub visible: u32,
    pub include_in_instruction_export: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodShootingFramePoint {
    pub x_milli: i64,
    pub y_milli: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodShootingFrameInfo {
    pub struct_size: u32,
    pub anchor: u32,
    pub feature_flags: u64,
    pub frame_id: u64,
    pub center_x_milli: i64,
    pub center_y_milli: i64,
    pub width_milli: u64,
    pub height_milli: u64,
    pub rotation_turns: u32,
    pub visible: u32,
    pub include_in_instruction_export: u32,
    pub reserved: u32,
    pub corners: [InkpodShootingFramePoint; 4],
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
#[derive(Default)]
pub struct InkpodLayerThumbnailBuffer {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub layer_id: u64,
    pub maximum_width: u32,
    pub maximum_height: u32,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub reserved_2: u32,
    pub revision: u64,
    pub pixels_rgba8: *mut u8,
    pub pixel_capacity: u64,
    pub required_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSelectionPoint {
    pub struct_size: u32,
    pub reserved: u32,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub reserved2: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
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
    pub interpretation: u32,
    pub aspect_ratio_q16: u32,
    pub construction_flags: u64,
    pub rotation_turns: u32,
    pub trace_shape: u32,
    pub view_zoom_q16: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodOutputColorGuardRequest {
    pub struct_size: u32,
    pub profile: u32,
    pub operation: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub base_document_revision: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodOutputColorGuardResult {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub revision: u64,
    pub accepted_command_count: u64,
    pub scanned_pixel_count: u64,
    pub selected_pixel_count: u64,
    pub transparent_pixel_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodScopedColorReplaceInput {
    pub struct_size: u32,
    pub mode: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub base_document_revision: u64,
    pub target_color: InkpodColorValue,
    pub replacement_color: InkpodColorValue,
    pub shape: u32,
    pub reserved: u32,
    pub bounds: InkpodFrameRect,
    pub points: *const InkpodSelectionPoint,
    pub point_count: u64,
    pub point_stride_bytes: u64,
    pub diameter: f32,
    pub reserved_2: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodScopedColorReplacePreview {
    pub struct_size: u32,
    pub feature_flags: u32,
    pub base_document_revision: u64,
    pub matched_pixels: u64,
    pub matched_objects: u64,
    pub affected_bounds: InkpodFrameRect,
}

#[repr(C)]
pub struct InkpodFloatingTransform {
    pub struct_size: u32,
    pub anchor: u32,
    pub target_x: f64,
    pub target_y: f64,
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
#[derive(Default)]
pub struct InkpodLocatorNeighborhoodBuffer {
    pub struct_size: u32,
    pub radius: u32,
    pub width: u32,
    pub height: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub reserved: u32,
    pub reserved_2: u32,
    pub pixels_rgba8: *mut u8,
    pub pixel_capacity: u64,
    pub required_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodRasterSourceInput {
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
    pub source: InkpodRasterSourceInput,
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InkpodLightTableBulkRequest {
    pub struct_size: u32,
    pub direction: u32,
    pub target_set_id: u64,
    pub neighbor_count: u32,
    pub base_opacity_milli: u32,
    pub distance_step_milli: u32,
    pub reserved: u32,
    pub base_document_revision: u64,
    pub sequence_revision: u64,
    pub active_document_uuid_high: u64,
    pub active_document_uuid_low: u64,
    pub active_source_generation: u64,
    pub feature_flags: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InkpodLightTableBulkPreviewInfo {
    pub struct_size: u32,
    pub reserved: u32,
    pub target_set_id: u64,
    pub entry_count: u64,
    pub add_count: u32,
    pub skip_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InkpodLightTableBulkPreviewEntry {
    pub struct_size: u32,
    pub action: u32,
    pub sequence_index: u32,
    pub cell_number: u32,
    pub distance: u32,
    pub opacity_milli: u32,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub source_generation: u64,
    pub existing_source_revision: u64,
    pub flags: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InkpodLightTableBulkSummary {
    pub struct_size: u32,
    pub reserved: u32,
    pub target_set_id: u64,
    pub add_count: u32,
    pub skip_count: u32,
    pub item_id_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodSequenceCellInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
    pub source: InkpodRasterSourceInput,
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
#[derive(Clone, Copy)]
pub struct InkpodNamedRasterInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub format: u32,
    pub reserved2: u32,
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
#[derive(Default)]
pub struct InkpodSequenceThumbnailBuffer {
    pub struct_size: u32,
    pub flags: u32,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub reserved: u32,
    pub checksum: u64,
    pub pixels_rgba8: *mut u8,
    pub pixel_capacity: u64,
    pub required_bytes: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodDocumentThumbnailBuffer {
    pub struct_size: u32,
    pub flags: u32,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub reserved: u32,
    pub checksum: u64,
    pub pixels_rgba8: *mut u8,
    pub pixel_capacity: u64,
    pub required_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSequenceSwitchRequest {
    pub struct_size: u32,
    pub policy: u32,
    pub feature_flags: u64,
    pub source_document_uuid_high: u64,
    pub source_document_uuid_low: u64,
    pub source_generation: u64,
    pub source_document_revision: u64,
    pub source_editor_revision: u64,
    pub target_document_uuid_high: u64,
    pub target_document_uuid_low: u64,
    pub target_source_generation: u64,
    pub target_index: u32,
    pub flags: u32,
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
