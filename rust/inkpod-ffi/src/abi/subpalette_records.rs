#[repr(C)]
#[derive(Clone, Copy)]
pub struct InkpodSubpaletteSourceInput {
    pub struct_size: u32,
    pub reserved: u32,
    pub source_token: u64,
    pub name_utf8: *const u8,
    pub name_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSubpaletteInfo {
    pub struct_size: u32,
    pub item_count: u32,
    pub catalog_revision: u64,
    pub active_index: u32,
    pub reserved: u32,
    pub flags: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodSubpaletteItemInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub item_id: u64,
    pub source_token: u64,
    pub cell_number: u32,
    pub reserved: u32,
    pub name_bytes: u64,
}
