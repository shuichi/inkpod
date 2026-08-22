use super::*;

#[repr(C)]
pub struct InkpodBatchInput {
    pub struct_size: u32,
    pub kind: u32,
    pub feature_flags: u64,
    pub path_utf8: *const u8,
    pub path_bytes: u64,
    pub first_cell: u32,
    pub last_cell: u32,
    pub reserved: u64,
}

#[repr(C)]
pub struct InkpodBatchColorPairInput {
    pub struct_size: u32,
    pub enabled: u32,
    pub reserved: u64,
    pub old_color: InkpodColorValue,
    pub new_color: InkpodColorValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodBatchOperationInput {
    pub struct_size: u32,
    pub version: u32,
    pub kind: u32,
    pub reserved: u32,
    pub flags: u64,
    pub layer_id: u64,
    pub plane_id: u64,
    pub layer_kind: u32,
    pub plane_kind: u32,
    pub missing_policy: u32,
    pub reserved_2: u32,
    pub colors: InkpodColorArray,
    pub color_pairs: *const InkpodBatchColorPairInput,
    pub color_pair_count: u64,
    pub color_pair_stride_bytes: u64,
    pub reserved_3: u64,
}

#[repr(C)]
pub struct InkpodBatchGraphInput {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
    pub inputs: *const InkpodBatchInput,
    pub input_count: u64,
    pub input_stride_bytes: u64,
    pub operations: *const InkpodBatchOperationInput,
    pub operation_count: u64,
    pub operation_stride_bytes: u64,
    pub output_destination: u32,
    pub failure_policy: u32,
    pub output_flags: u64,
    pub output_folder_utf8: *const u8,
    pub output_folder_bytes: u64,
    pub naming_template_utf8: *const u8,
    pub naming_template_bytes: u64,
    pub output_format: u32,
    pub wait_milliseconds: u32,
    pub reserved: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodBatchGraphInfo {
    pub struct_size: u32,
    pub version: u32,
    pub input_count: u64,
    pub operation_count: u64,
    pub output_destination: u32,
    pub output_format: u32,
    pub failure_policy: u32,
    pub output_flags: u64,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
    pub output_folder_utf8: *const u8,
    pub output_folder_bytes: u64,
    pub naming_template_utf8: *const u8,
    pub naming_template_bytes: u64,
    pub wait_milliseconds: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodBatchOperationInfo {
    pub struct_size: u32,
    pub version: u32,
    pub kind: u32,
    pub reserved: u32,
    pub flags: u64,
    pub layer_id: u64,
    pub plane_id: u64,
    pub layer_kind: u32,
    pub plane_kind: u32,
    pub missing_policy: u32,
    pub reserved_2: u32,
    pub color_count: u64,
    pub color_pair_count: u64,
    pub reserved_3: [u64; 2],
}

#[repr(C)]
pub struct InkpodBatchPreviewItem {
    pub struct_size: u32,
    pub flags: u32,
    pub input_name: *const u8,
    pub input_name_bytes: u64,
    pub output_path: *const u8,
    pub output_path_bytes: u64,
    pub warning: *const u8,
    pub warning_bytes: u64,
}

#[repr(C)]
pub struct InkpodBatchReportInfo {
    pub struct_size: u32,
    pub cancelled: u32,
    pub item_count: u64,
    pub failure_count: u64,
    pub staged_result_count: u64,
}

#[repr(C)]
pub struct InkpodBatchReportItem {
    pub struct_size: u32,
    pub outcome: u32,
    pub input_name: *const u8,
    pub input_name_bytes: u64,
    pub output_path: *const u8,
    pub output_path_bytes: u64,
    pub message: *const u8,
    pub message_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSequenceSourceIdentity {
    pub struct_size: u32,
    pub reserved: u32,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub source_generation: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodBatchPairPreviewInfo {
    pub struct_size: u32,
    pub pixel_format: u32,
    pub width: u32,
    pub height: u32,
    pub ambiguity_count: u32,
    pub reserved: u32,
    pub candidate_count: u64,
    pub unchanged_pixel_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodBatchPairCandidate {
    pub struct_size: u32,
    pub flags: u32,
    pub old_color: InkpodColorValue,
    pub new_color: InkpodColorValue,
    pub pixel_count: u64,
    pub bounds_x: i32,
    pub bounds_y: i32,
    pub bounds_width: i32,
    pub bounds_height: i32,
}

pub struct InkpodBatchGraph {
    pub(super) graph: BatchGraph,
}

pub(super) struct OwnedPreviewItem {
    pub(super) input_name: Box<[u8]>,
    pub(super) output_path: Box<[u8]>,
    pub(super) warning: Box<[u8]>,
}

pub struct InkpodBatchPreview {
    pub(super) items: Vec<OwnedPreviewItem>,
}

pub(super) struct OwnedReportItem {
    pub(super) outcome: u32,
    pub(super) input_name: Box<[u8]>,
    pub(super) output_path: Box<[u8]>,
    pub(super) message: Box<[u8]>,
}

pub struct InkpodBatchReport {
    pub(super) items: Vec<OwnedReportItem>,
    pub(super) cancelled: bool,
    pub(super) owner_thread: ThreadId,
    pub(super) staged_results: Vec<Option<BatchStagedResult>>,
}

pub struct InkpodBatchPairPreview {
    pub(super) extraction: BatchPairExtraction,
}
