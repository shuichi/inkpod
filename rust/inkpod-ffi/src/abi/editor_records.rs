use super::*;

/// Maximum exact-depth inclusion colors copied in one editor-state record.
pub const INKPOD_EDITOR_MAX_INCLUSION_COLORS: usize = 6;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditorFillOptions {
    pub struct_size: u32,
    pub operation: u32,
    pub flags: u64,
    pub tolerance: u16,
    pub gap_close: u16,
    pub inclusion_mode: u32,
    pub extension_distance: u32,
    pub inclusion_color_count: u32,
    pub reserved: u32,
    pub inclusion_colors: [InkpodColorValue; INKPOD_EDITOR_MAX_INCLUSION_COLORS],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditorSelectionOptions {
    pub struct_size: u32,
    pub shape: u32,
    pub operation: u32,
    pub reserved: u32,
    pub tolerance: u16,
    pub gap_close: u16,
    pub reserved2: u32,
    pub diameter_q16: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditorVectorOptions {
    pub struct_size: u32,
    pub erase_mode: u32,
    pub selection_mode: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditorStateInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub feature_flags: u64,
    pub editor_revision: u64,
    pub editor_digest: [u8; 32],
    pub active_tool: u32,
    pub last_color_consuming_tool: u32,
    pub current_color: InkpodColorValue,
    pub reserved: u32,
    pub current_diameter_q16: i64,
    pub active_layer_id: u64,
    pub active_plane_id: u64,
    pub palette_group: u32,
    pub palette_index: u32,
    pub fill: InkpodEditorFillOptions,
    pub selection: InkpodEditorSelectionOptions,
    pub vector: InkpodEditorVectorOptions,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditorDefaults {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub state: InkpodEditorStateInfo,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditorStateUpdate {
    pub struct_size: u32,
    pub kind: u32,
    pub expected_editor_revision: u64,
    pub flags: u64,
    pub tool: u32,
    pub reserved: u32,
    pub color: InkpodColorValue,
    pub diameter_q16: i64,
    pub active_layer_id: u64,
    pub active_plane_id: u64,
    pub palette_group: u32,
    pub palette_index: u32,
    pub fill: InkpodEditorFillOptions,
    pub selection: InkpodEditorSelectionOptions,
    pub vector: InkpodEditorVectorOptions,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditorStrokeInput {
    pub struct_size: u32,
    pub coordinate_space: u32,
    pub tool: u32,
    pub reserved: u32,
    pub flags: u64,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditTarget {
    pub struct_size: u32,
    pub kind: u32,
    pub layer_id: u64,
    pub plane_id: u64,
    pub reserved: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditTargetCommand {
    pub struct_size: u32,
    pub operation: u32,
    pub flags: u64,
    pub kind: u32,
    pub pixel_format: u32,
    pub reserved: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodEditTargetCapabilities {
    pub struct_size: u32,
    pub can_duplicate: u32,
    pub can_delete: u32,
    pub can_set_visibility: u32,
    pub can_set_editability: u32,
    pub can_merge: u32,
    pub can_convert_planes: u32,
    pub can_convert_layers: u32,
    pub reserved: u32,
}
