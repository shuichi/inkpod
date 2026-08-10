use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Checksums and transient revision for the active filter preview.
pub struct FilterPreviewInfo {
    /// Stable destination plane ID.
    pub plane_id: u64,
    /// Checksum of the unmodified base plane.
    pub base_checksum: u64,
    /// Checksum of the currently previewed plane.
    pub preview_checksum: u64,
    /// Transient render revision; it is not a committed document revision.
    pub preview_revision: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum PreviewProcedure {
    Filter(Filter),
    Dust {
        shape: Option<SelectionShape>,
        options: DustRemoval,
    },
    Geometry(crate::geometry::CanonicalGeometry),
}

#[derive(Clone, Debug)]
pub(crate) struct FilterPreview {
    pub(crate) plane_id: PlaneId,
    pub(crate) base_revision: DocumentRevision,
    pub(crate) base_document: CellDocument,
    pub(crate) preview_document: CellDocument,
    pub(crate) procedure: PreviewProcedure,
    pub(crate) preview_revision: PreviewRevision,
}
