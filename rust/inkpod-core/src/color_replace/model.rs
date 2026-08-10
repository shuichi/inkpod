use super::*;

/// Explicit topology and protection mode for scoped color replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopedColorReplaceMode {
    /// Editable raster color or generic raster plane; main-line planes are rejected.
    RasterColor,
    /// Editable raster main-line plane.
    RasterMainLine,
    /// Whole stable paths on an editable vector color-trace plane.
    VectorColorLine,
    /// Whole stable paths on an editable vector main-line plane.
    VectorMainLine,
    /// Whole stable fill objects on an editable vector-fill plane.
    VectorFill,
}

/// Complete caller-owned input for one preview or canonical scoped replacement.
#[derive(Clone, Debug, PartialEq)]
pub struct ScopedColorReplaceRequest {
    /// Document revision observed when the interaction began.
    pub base_document_revision: u64,
    /// Stable destination plane ID.
    pub plane_id: u64,
    /// Explicit raster/vector and coloring/main-line mode.
    pub mode: ScopedColorReplaceMode,
    /// Exact native-depth source value, including alpha.
    pub target: PixelValue,
    /// Exact native-depth replacement value, including alpha.
    pub replacement: PixelValue,
    /// Optional pen, rectangle, polyline, or lasso region in document coordinates.
    ///
    /// A non-empty current document selection is always intersected with this
    /// region. With no region, selection alone is used; with neither, the full
    /// document is used.
    pub region: Option<SelectionShape>,
}

/// Read-only result for one scoped color-replacement preview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScopedColorReplacePreview {
    /// Document revision against which the preview was evaluated.
    pub base_document_revision: u64,
    /// Exact raster pixels that would change.
    pub matched_pixels: u64,
    /// Whole stable vector paths or fills that would change.
    pub matched_objects: u64,
    /// Smallest half-open raster match bounds or effective vector contact bounds.
    pub affected_bounds: Option<RectI32>,
}
