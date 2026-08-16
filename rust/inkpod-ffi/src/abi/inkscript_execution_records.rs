use super::*;
use core::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptUtf8Span {
    pub bytes: *const u8,
    pub byte_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptPathIdentity {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub canonical_key: InkpodInkScriptUtf8Span,
    pub volume_id: [u8; 16],
    pub object_id: [u8; 32],
    pub object_generation: u64,
    pub alias_key: [u8; 32],
    pub parent_object_id: [u8; 32],
    pub parent_generation: u64,
    pub parent_alias_key: [u8; 32],
    pub flags: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptNativeFingerprint {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub path: *const InkpodInkScriptPathIdentity,
    pub display_label: InkpodInkScriptUtf8Span,
    pub display_number: u32,
    pub flags: u32,
    pub document_uuid_low: u64,
    pub document_uuid_high: u64,
    pub logical_length: u64,
    pub content_digest: [u8; 32],
    pub change_token: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptSessionInput {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub core: *mut InkpodCore,
    pub session_id: u64,
    pub session_generation: u64,
    pub source_generation: u64,
    pub display_label: InkpodInkScriptUtf8Span,
    pub display_number: u32,
    pub reserved: u32,
    pub backing_path: *const InkpodInkScriptPathIdentity,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptSequenceMember {
    pub struct_size: u32,
    pub version: u32,
    pub kind: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub source_generation: u64,
    pub session: *const InkpodInkScriptSessionInput,
    pub fingerprint: *const InkpodInkScriptNativeFingerprint,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptOpenSession {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub session_id: u64,
    pub session_generation: u64,
    pub document_uuid_low: u64,
    pub document_uuid_high: u64,
    pub backing_path: *const InkpodInkScriptPathIdentity,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptAuthorityGrant {
    pub struct_size: u32,
    pub version: u32,
    pub access: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub intent_id: u64,
    pub authority_id: [u8; 32],
    pub authority_generation: u64,
    pub resolved: *const InkpodInkScriptPathIdentity,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptTemporaryIdentity {
    pub volume_id: [u8; 16],
    pub parent_object_id: [u8; 32],
    pub parent_generation: u64,
    pub object_id: [u8; 32],
    pub object_generation: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptHostRequest {
    pub struct_size: u32,
    pub version: u32,
    pub operation: u32,
    pub flags: u32,
    pub feature_flags: u64,
    pub intent_id: u64,
    pub session_id: u64,
    pub session_generation: u64,
    pub source_generation: u64,
    pub byte_offset: u64,
    pub byte_capacity: u64,
    pub asset_symbol: InkpodInkScriptUtf8Span,
    pub identity: *const InkpodInkScriptPathIdentity,
    pub fingerprint: *const InkpodInkScriptNativeFingerprint,
    pub relative_components: *const InkpodInkScriptUtf8Span,
    pub relative_component_count: u64,
    pub known_directories: *const InkpodInkScriptPathIdentity,
    pub known_directory_count: u64,
    pub temporary: InkpodInkScriptTemporaryIdentity,
    pub overwrite_guard: [u8; 32],
    pub bytes: *const u8,
    pub byte_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptHostResponse {
    pub struct_size: u32,
    pub version: u32,
    pub flags: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub generation: u64,
    pub secondary_generation: u64,
    pub sequence_id: u64,
    pub records: *const c_void,
    pub record_count: u64,
    pub record_stride_bytes: u64,
    pub observed_entries: u64,
    pub normalized_name_bytes: u64,
    pub work_units: u64,
    pub maximum_depth: u32,
    pub result_kind: u32,
    pub identity: *const InkpodInkScriptPathIdentity,
    pub fingerprint: *const InkpodInkScriptNativeFingerprint,
    pub fingerprint_after: *const InkpodInkScriptNativeFingerprint,
    pub session: *const InkpodInkScriptSessionInput,
    pub bytes: *const u8,
    pub byte_count: u64,
    pub temporary: InkpodInkScriptTemporaryIdentity,
    pub overwrite_guard: [u8; 32],
}

pub type InkpodInkScriptHostCall = Option<
    unsafe extern "C" fn(
        context: *mut c_void,
        request: *const InkpodInkScriptHostRequest,
        response: *mut InkpodInkScriptHostResponse,
    ) -> u32,
>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptHostAdapter {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub context: *mut c_void,
    pub call: InkpodInkScriptHostCall,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptPlanTaskRequest {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub controller_id: u64,
    pub session_generation: u64,
    pub authority_generation: u64,
    pub open_session_set_generation: u64,
    pub grants: *const InkpodInkScriptAuthorityGrant,
    pub grant_count: u64,
    pub grant_stride_bytes: u64,
    pub script_path: *const InkpodInkScriptPathIdentity,
    pub maximum_folder_entries: u64,
    pub host: InkpodInkScriptHostAdapter,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptPathIntent {
    pub struct_size: u32,
    pub version: u32,
    pub access: u32,
    pub subject_kind: u32,
    pub feature_flags: u64,
    pub intent_id: u64,
    pub subject_index: u64,
    pub text_offset: u64,
    pub text_bytes: u64,
    pub subject_offset: u64,
    pub subject_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptPathIntentBuffer {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub records: *mut InkpodInkScriptPathIntent,
    pub record_capacity: u64,
    pub record_stride_bytes: u64,
    pub utf8: *mut u8,
    pub utf8_capacity_bytes: u64,
    pub records_written: u64,
    pub required_records: u64,
    pub utf8_written_bytes: u64,
    pub required_utf8_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptPlanSummary {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub controller_id: u64,
    pub session_generation: u64,
    pub core_generation: u64,
    pub plan_digest: [u8; 32],
    pub item_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptPreviewItem {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub ordinal: u64,
    pub input_offset: u64,
    pub input_bytes: u64,
    pub output_offset: u64,
    pub output_bytes: u64,
    pub destination_offset: u64,
    pub destination_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptPreviewBuffer {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub first_item: u64,
    pub records: *mut InkpodInkScriptPreviewItem,
    pub record_capacity: u64,
    pub record_stride_bytes: u64,
    pub utf8: *mut u8,
    pub utf8_capacity_bytes: u64,
    pub records_written: u64,
    pub required_records: u64,
    pub utf8_written_bytes: u64,
    pub required_utf8_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptConfirmationRequest {
    pub struct_size: u32,
    pub version: u32,
    pub scope: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub document_uuid_low: u64,
    pub document_uuid_high: u64,
    pub file_alias: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptRunRequest {
    pub struct_size: u32,
    pub version: u32,
    pub mode: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub controller_id: u64,
    pub session_generation: u64,
    pub maximum_output_bytes: u64,
    pub host: InkpodInkScriptHostAdapter,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptTaskEvent {
    pub struct_size: u32,
    pub version: u32,
    pub kind: u32,
    pub task_state: u32,
    pub feature_flags: u64,
    pub ordinal: u64,
    pub completed_items: u64,
    pub total_items: u64,
    pub wait_milliseconds: u32,
    pub outcome: u32,
    pub failure: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptReportSummary {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub flags: u64,
    pub item_count: u64,
    pub created_directory_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptReportItem {
    pub struct_size: u32,
    pub version: u32,
    pub outcome: u32,
    pub failure: u32,
    pub feature_flags: u64,
    pub ordinal: u64,
    pub input_offset: u64,
    pub input_bytes: u64,
    pub destination_offset: u64,
    pub destination_bytes: u64,
    pub commit_count: u64,
    pub final_revision: u64,
    pub next_stable_id: u64,
    pub final_state_digest: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptReportBuffer {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub first_item: u64,
    pub records: *mut InkpodInkScriptReportItem,
    pub record_capacity: u64,
    pub record_stride_bytes: u64,
    pub utf8: *mut u8,
    pub utf8_capacity_bytes: u64,
    pub records_written: u64,
    pub required_records: u64,
    pub utf8_written_bytes: u64,
    pub required_utf8_bytes: u64,
}
