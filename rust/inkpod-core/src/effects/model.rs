use super::*;

/// An explicit line edit on one stable plane. The document-space region is
/// intersected with the existing selection; `None` uses that selection or the
/// entire plane. Construction options apply only to the supplied region.
#[derive(Clone, Debug, PartialEq)]
pub struct LineCorrectionRequest {
    /// Stable destination plane ID in this document.
    pub plane_id: u64,
    /// Optional document-space operation region.
    pub region: Option<SelectionShape>,
    /// Brush geometry captured at gesture begin, including screen-size zoom.
    pub construction: crate::SelectionConstructionOptions,
    /// Correction mode, units, and background policy.
    pub correction: crate::LineCorrection,
}

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
    LineCorrection(LineCorrectionRequest),
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
