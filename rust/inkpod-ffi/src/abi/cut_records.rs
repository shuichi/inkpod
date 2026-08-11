#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodUtf8Span {
    pub bytes: *const u8,
    pub byte_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodUtf8Buffer {
    pub bytes: *mut u8,
    pub capacity: u64,
    pub byte_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodCutMetadataInput {
    pub struct_size: u32,
    pub duration_frames: u32,
    pub work_title: InkpodUtf8Span,
    pub episode: InkpodUtf8Span,
    pub scene: InkpodUtf8Span,
    pub cut_name: InkpodUtf8Span,
    pub instruction: InkpodUtf8Span,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodCutMetadataBuffer {
    pub struct_size: u32,
    pub duration_frames: u32,
    pub work_title: InkpodUtf8Buffer,
    pub episode: InkpodUtf8Buffer,
    pub scene: InkpodUtf8Buffer,
    pub cut_name: InkpodUtf8Buffer,
    pub instruction: InkpodUtf8Buffer,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodCutDefaultsInput {
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
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodCutMemberInput {
    pub struct_size: u32,
    pub display_number: u32,
    pub cell_id: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub relative_path: InkpodUtf8Span,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodCutCreateRequest {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub cut_uuid_high: u64,
    pub cut_uuid_low: u64,
    pub metadata: *const InkpodCutMetadataInput,
    pub defaults: *const InkpodCutDefaultsInput,
    pub members: *const InkpodCutMemberInput,
    pub member_count: u64,
    pub member_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodCutUpdateRequest {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub base_revision: u64,
    pub metadata: *const InkpodCutMetadataInput,
    pub defaults: *const InkpodCutDefaultsInput,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodCutInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub cut_id: u64,
    pub cut_uuid_high: u64,
    pub cut_uuid_low: u64,
    pub revision: u64,
    pub state_id: u64,
    pub member_count: u32,
    pub reserved: u32,
    pub work_title_bytes: u64,
    pub episode_bytes: u64,
    pub scene_bytes: u64,
    pub cut_name_bytes: u64,
    pub instruction_bytes: u64,
    pub duration_frames: u32,
    pub sizing_mode: u32,
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
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodCutMemberInfo {
    pub struct_size: u32,
    pub display_number: u32,
    pub cell_id: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub relative_path: InkpodUtf8Buffer,
}
