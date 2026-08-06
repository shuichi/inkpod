use super::*;

/// Generation-scoped identity for one ABI-v3 Rust-owned object.
#[repr(C)]
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct InkpodObjectId {
    pub struct_size: u32,
    pub object_type: u32,
    pub feature_flags: u64,
    pub generation: u64,
    pub value: u64,
}

/// Fixed-width, pointer-free request for one canonical primitive invocation.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodPrimitiveRequestV3 {
    pub struct_size: u32,
    pub opcode: u32,
    pub schema_version: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub base_revision: u64,
    pub target_id: u64,
    pub payload_id: InkpodObjectId,
    pub tool: u32,
    pub plane: u32,
    pub coordinate_space: u32,
    pub reserved_2: u32,
    pub stroke_flags: u64,
    pub color: InkpodColorValue,
    pub diameter: f32,
    pub reserved_3: u32,
}

/// Result of one ABI-v3 canonical primitive invocation.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodPrimitiveResultV3 {
    pub struct_size: u32,
    pub flags: u32,
    pub revision: u64,
    pub accepted_command_count: u64,
    pub procedure_id: u64,
    pub committed_state_id: u64,
    pub opcode: u32,
    pub schema_version: u32,
}

/// Borrowed raster bytes copied into one immutable ABI-v3 asset object.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodRasterAssetInputV3 {
    pub struct_size: u32,
    pub pixel_format: u32,
    pub feature_flags: u64,
    pub width: u32,
    pub height: u32,
    pub reserved: u32,
    pub reserved_2: u32,
    pub row_stride_bytes: u64,
    pub pixels: *const u8,
    pub pixel_bytes: u64,
}

/// Read-only metadata for one live ABI-v3 object.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodObjectInfoV3 {
    pub struct_size: u32,
    pub object_type: u32,
    pub feature_flags: u64,
    pub generation: u64,
    pub value: u64,
    pub element_count: u64,
    pub byte_count: u64,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u64,
    pub revision: u64,
}

/// Pointer-free metadata for one immutable ABI-v3 render snapshot.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotInfoV3 {
    pub struct_size: u32,
    pub transform_flags: u32,
    pub feature_flags: u64,
    pub revision: u64,
    pub view_revision: u64,
    pub tile_count: u64,
    pub guide_count: u64,
    pub vector_segment_count: u64,
    pub vector_fill_count: u64,
    pub vector_boundary_path_count: u64,
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub document_width: u32,
    pub document_height: u32,
}

/// Pointer-free descriptor copied from one ABI-v3 snapshot tile batch.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSnapshotTileInfoV3 {
    pub struct_size: u32,
    pub pixel_format: u32,
    pub tile_id: u64,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub reserved: u32,
    pub pixel_bytes: u64,
    pub tile_revision: u64,
}

/// One bounded caller-owned byte-copy operation.
#[repr(C)]
#[derive(Default)]
pub struct InkpodBufferCopyV3 {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub offset: u64,
    pub bytes: *mut u8,
    pub byte_capacity: u64,
    pub written_bytes: u64,
    pub total_bytes: u64,
}
