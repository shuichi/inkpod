use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilterPreviewInfo {
    pub plane_id: u64,
    pub base_checksum: u64,
    pub preview_checksum: u64,
    pub preview_revision: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct FilterPreview {
    pub(crate) plane_id: u64,
    pub(crate) base_document: CellDocument,
    pub(crate) preview_document: CellDocument,
    pub(crate) filter: Option<Filter>,
    pub(crate) preview_revision: u64,
}
