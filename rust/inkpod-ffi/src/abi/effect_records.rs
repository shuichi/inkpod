use super::*;

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
#[derive(Clone, Copy, Default)]
pub struct InkpodLineCorrectionInput {
    pub struct_size: u32,
    pub mode: u32,
    pub feature_flags: u64,
    pub plane_id: u64,
    pub view_id: u64,
    pub expected_document_revision: u64,
    pub coordinate_space: u32,
    pub shape: u32,
    pub use_region: u32,
    pub background_mode: u32,
    pub gap: u32,
    pub line_width: u32,
    pub amount: u32,
    pub brush_shape: u32,
    pub pressure_size: u32,
    pub screen_size: u32,
    pub background_rgba: [u16; 4],
    pub diameter: f32,
    pub view_zoom_q16: i64,
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
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotShootingFrameView {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
    pub frames: *const InkpodShootingFrameInfo,
    pub frame_count: u64,
    pub frame_stride_bytes: u64,
}
