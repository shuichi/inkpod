pub const INKPOD_STATUS_PENDING: u32 = 14;
pub const INKPOD_STATUS_FILE_CONFLICT: u32 = 15;
pub const INKPOD_IO_OPEN_NATIVE: u32 = 1;
pub const INKPOD_IO_OPEN_RECOVERY: u32 = 2;
pub const INKPOD_IO_OPEN_RASTER: u32 = 3;
pub const INKPOD_IO_SEQUENCE_AUTO: u32 = 4;
pub const INKPOD_IO_SEQUENCE_FILES: u32 = 5;
pub const INKPOD_IO_REFERENCE_FILES: u32 = 6;
pub const INKPOD_IO_REFERENCE_FOLDER: u32 = 7;
pub const INKPOD_IO_LIGHT_TABLE_ADD: u32 = 8;
pub const INKPOD_IO_LIGHT_TABLE_RELOAD: u32 = 9;
pub const INKPOD_IO_SAVE_PAIR: u32 = 10;
pub const INKPOD_IO_AUTOSAVE: u32 = 11;
pub const INKPOD_IO_EXPORT_RASTER: u32 = 12;
pub const INKPOD_IO_BATCH_PLAN: u32 = 13;
pub const INKPOD_IO_BATCH_RUN: u32 = 14;
pub const INKPOD_IO_BATCH_PREVIEW: u32 = 15;
pub const INKPOD_IO_RECOVERY_LIST: u32 = 16;
pub const INKPOD_IO_RECOVERY_DISCARD: u32 = 17;
pub const INKPOD_IO_RECOVERY_PROBE: u32 = 18;
pub const INKPOD_IO_EXPORT_SEQUENCE: u32 = 19;
pub const INKPOD_IO_SEQUENCE_SWITCH: u32 = 20;
pub const INKPOD_IO_COMPACTED_COPY: u32 = 21;
pub const INKPOD_IO_OPEN_RASTER_PAIR: u32 = 22;
pub const INKPOD_IO_QUEUED: u32 = 0;
pub const INKPOD_IO_RUNNING: u32 = 1;
pub const INKPOD_IO_READY: u32 = 2;
pub const INKPOD_IO_COMPLETE: u32 = 3;
pub const INKPOD_IO_FAILED: u32 = 4;
pub const INKPOD_IO_CANCELLED: u32 = 5;
pub const INKPOD_IO_FORCE_RELOAD: u64 = 1;
pub const INKPOD_IO_COMPOSITE_WHITE: u64 = 2;
pub const INKPOD_IO_OVERWRITE_CONFIRMED: u64 = 4;
pub const INKPOD_IO_INSTRUCTIONS: u64 = 8;
pub const INKPOD_IO_REVERT_CURRENT: u64 = 16;
pub const INKPOD_IO_RESULT_TRUNCATED: u64 = 1;
pub const INKPOD_IO_RESULT_INSTALLING: u64 = 2;
pub const INKPOD_IO_RESULT_CUT_DESCRIPTOR: u64 = 4;
pub const INKPOD_IO_RESULT_AUTHORITY_REPAIRED: u64 = 8;
pub const INKPOD_IO_RESULT_AUTHORITY_REVOKED: u64 = 16;
pub const INKPOD_IO_RECOVERY_ARTIFACT_READONLY: u32 = 1;
pub const INKPOD_IO_RECOVERY_PAIR_NONE: u32 = 0;
pub const INKPOD_IO_RECOVERY_PAIR_COMMITTED: u32 = 1;
pub const INKPOD_IO_RECOVERY_PAIR_PLANNED: u32 = 2;
pub const INKPOD_IO_RECOVERY_PAIR_REPAIR_NEEDED: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodIoConfig {
    pub struct_size: u32,
    pub worker_count: u32,
    pub queue_capacity: u32,
    pub max_images: u32,
    pub max_file_bytes: u64,
    pub max_encoded_bytes: u64,
    pub max_decoded_bytes: u64,
    pub reserved: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodIoPath {
    pub struct_size: u32,
    pub reserved: u32,
    pub path: *const u8,
    pub path_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodIoRequest {
    pub struct_size: u32,
    pub kind: u32,
    pub flags: u64,
    pub paths: *const InkpodIoPath,
    pub path_count: u64,
    pub path_stride_bytes: u64,
    pub object_id: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub raster_format: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodIoRecoveryMetadata {
    pub struct_size: u32,
    pub flags: u32,
    pub session_id: u64,
    pub generation: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub written_time_100ns: u64,
    pub modified_time_100ns: u64,
    pub identity_kind: u32,
    pub reserved: u32,
    pub identity_volume: u64,
    pub identity_object_high: u64,
    pub identity_object_low: u64,
    pub original_path: InkpodIoPath,
    pub source_path: InkpodIoPath,
    pub identity_path: InkpodIoPath,
    pub pair_proof: InkpodIoRecoveryPairProof,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodIoJobInfo {
    pub struct_size: u32,
    pub state: u32,
    pub kind: u32,
    pub status: u32,
    pub job_id: u64,
    pub discovered_count: u64,
    pub total_count: u64,
    pub read_count: u64,
    pub loaded_count: u64,
    pub failed_count: u64,
    pub cancelled_count: u64,
    pub completed_work: u64,
    pub total_work: u64,
    pub result_count: u64,
    pub flags: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodIoFileIdentity {
    pub struct_size: u32,
    pub kind: u32,
    pub volume: u64,
    pub object_high: u64,
    pub object_low: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodIoRecoveryArtifactStamp {
    pub struct_size: u32,
    pub flags: u32,
    pub identity: InkpodIoFileIdentity,
    pub length: u64,
    pub modified_high: u64,
    pub modified_low: u64,
    pub changed_high: u64,
    pub changed_low: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodIoRecoveryArtifactProof {
    pub struct_size: u32,
    pub reserved: u32,
    pub native: InkpodIoRecoveryArtifactStamp,
    pub metadata: InkpodIoRecoveryArtifactStamp,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodIoRecoveryPairProof {
    pub struct_size: u32,
    pub kind: u32,
    pub native: InkpodIoRecoveryArtifactStamp,
    pub raster: InkpodIoRecoveryArtifactStamp,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodIoItemInfo {
    pub struct_size: u32,
    pub raster_format: u32,
    pub source_generation: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub path_bytes: u64,
    pub name_bytes: u64,
    pub identity: InkpodIoFileIdentity,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodIoCacheInfo {
    pub struct_size: u32,
    pub reserved: u32,
    pub image_count: u64,
    pub encoded_bytes: u64,
    pub decoded_bytes: u64,
    pub physical_reads: u64,
    pub decodes: u64,
    pub cache_hits: u64,
    pub sequence_render_allocations: u64,
    pub sequence_render_bytes: u64,
}
