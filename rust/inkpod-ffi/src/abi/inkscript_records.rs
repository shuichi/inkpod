#[repr(C)]
pub struct InkpodInkScriptSourceInput {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub controller_id: u64,
    pub session_generation: u64,
    pub source_id: u64,
    pub source_utf8: *const u8,
    pub source_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptSourceSummary {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub controller_id: u64,
    pub session_generation: u64,
    pub source_id: u64,
    pub source_bytes: u64,
    pub diagnostic_count: u64,
    pub document_kind: u32,
    pub reserved: u32,
    pub flags: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptUtf8Buffer {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub bytes: *mut u8,
    pub capacity_bytes: u64,
    pub written_bytes: u64,
    pub required_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptDiagnostic {
    pub struct_size: u32,
    pub version: u32,
    pub severity: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub source_id: u64,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub code_offset: u64,
    pub code_bytes: u64,
    pub message_offset: u64,
    pub message_bytes: u64,
    pub path_offset: u64,
    pub path_bytes: u64,
    pub hint_offset: u64,
    pub hint_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptDiagnosticBuffer {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub first_diagnostic: u64,
    pub records: *mut InkpodInkScriptDiagnostic,
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
pub struct InkpodInkScriptParameterChoice {
    pub struct_size: u32,
    pub version: u32,
    pub kind: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
    pub value_utf8: *const u8,
    pub value_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptCompileRequest {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub controller_id: u64,
    pub session_generation: u64,
    pub flags: u64,
    pub reserved: u64,
    pub parameter_choices: *const InkpodInkScriptParameterChoice,
    pub parameter_choice_count: u64,
    pub parameter_choice_stride_bytes: u64,
    pub max_invocations: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptProgramSummary {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub controller_id: u64,
    pub session_generation: u64,
    pub core_generation: u64,
    pub static_compile_digest: [u8; 32],
    pub path_intent_digest: [u8; 32],
    pub max_invocations: u64,
    pub max_output_ids: u64,
    pub max_asset_bytes: u64,
    pub max_work_units: u64,
    pub max_output_growth: u64,
    pub path_intent_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptJournalEvent {
    pub struct_size: u32,
    pub version: u32,
    pub event_id: u64,
    pub reserved: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodInkScriptExportRequest {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub controller_id: u64,
    pub session_generation: u64,
    pub flags: u64,
    pub reserved: u64,
    pub events: *const InkpodInkScriptJournalEvent,
    pub event_count: u64,
    pub event_stride_bytes: u64,
    pub max_commits: u64,
    pub max_source_bytes: u64,
    pub max_inline_asset_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodInkScriptFragmentSummary {
    pub struct_size: u32,
    pub version: u32,
    pub feature_flags: u64,
    pub controller_id: u64,
    pub session_generation: u64,
    pub core_generation: u64,
    pub base_state_id: u64,
    pub final_state_id: u64,
    pub commit_count: u64,
    pub portability: u32,
    pub reserved: u32,
    pub required_precondition_count: u64,
    pub text_bytes: u64,
}
