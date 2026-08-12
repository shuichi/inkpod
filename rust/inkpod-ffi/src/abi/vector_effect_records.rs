use super::*;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodVectorPoint {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodGeometryPoint {
    pub struct_size: u32,
    pub reserved: u32,
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodGeometryPointResolveInput {
    pub struct_size: u32,
    pub coordinate_space: u32,
    pub feature_flags: u64,
    pub view_id: u64,
    pub expected_view_revision: u64,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodGeometryPointResolveResult {
    pub struct_size: u32,
    pub reserved: u32,
    pub view_revision: u64,
    pub point_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodGeometryInput {
    pub struct_size: u32,
    pub primitive: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub base_revision: u64,
    pub outline_color: InkpodColorValue,
    pub fill_color: InkpodColorValue,
    pub outline_width: f32,
    pub aspect_ratio_q16: u32,
    pub polygon_sides: u32,
    pub rotation_turns: u32,
    pub points: *const InkpodGeometryPoint,
    pub point_count: u64,
    pub point_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodGeometryPreviewInfo {
    pub struct_size: u32,
    pub reserved: u32,
    pub plane_id: u64,
    pub base_revision: u64,
    pub preview_revision: u64,
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
pub struct InkpodTaskInfo {
    pub struct_size: u32,
    pub state: u32,
    pub completed_work: u64,
    pub total_work: u64,
    pub reserved: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
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
#[derive(Clone, Copy)]
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
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotAnnotation {
    pub struct_size: u32,
    pub kind: u32,
    pub feature_flags: u64,
    pub object_id: u64,
    pub layer_id: u64,
    pub output: u32,
    pub style_flags: u32,
    pub bounds: InkpodFrameRect,
    pub font_size_milli: u32,
    pub stroke_width_milli: u32,
    pub color: InkpodColorValue,
    pub font_utf8_offset: u64,
    pub font_utf8_bytes: u64,
    pub text_utf8_offset: u64,
    pub text_utf8_bytes: u64,
    pub first_point: u64,
    pub point_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotAnnotationView {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
    pub objects: *const InkpodSnapshotAnnotation,
    pub object_count: u64,
    pub object_stride_bytes: u64,
    pub utf8_bytes: *const u8,
    pub utf8_byte_count: u64,
    pub points: *const InkpodAnnotationPoint,
    pub point_count: u64,
    pub point_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotShootingFrameView {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
    pub frames: *const InkpodShootingFrameInfo,
    pub frame_count: u64,
    pub frame_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotRadialGuide {
    pub struct_size: u32,
    pub angle_milli_degrees: u32,
    pub feature_flags: u64,
    pub point_id: u64,
    pub start_x_milli: i64,
    pub start_y_milli: i64,
    pub end_x_milli: i64,
    pub end_y_milli: i64,
    pub opacity_milli: u32,
    pub reserved: u32,
    pub color: InkpodColorValue,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotVanishingPointView {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
    pub points: *const InkpodVanishingPointInfo,
    pub point_count: u64,
    pub point_stride_bytes: u64,
    pub radial_guides: *const InkpodSnapshotRadialGuide,
    pub radial_guide_count: u64,
    pub radial_guide_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotVectorEndpoint {
    pub struct_size: u32,
    pub endpoint: u32,
    pub path_id: u64,
    pub plane_id: u64,
    pub point: InkpodVectorPoint,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotVectorDiagnostics {
    pub struct_size: u32,
    pub flags: u32,
    pub feature_flags: u64,
    pub endpoints: *const InkpodSnapshotVectorEndpoint,
    pub endpoint_count: u64,
    pub endpoint_stride_bytes: u64,
}
