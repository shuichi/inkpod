#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
/// Natural-sequence side considered by Light Table bulk registration.
pub enum LightTableBulkDirection {
    /// Consider up to `neighbor_count` cells before the active cell.
    Previous = 1,
    /// Consider up to `neighbor_count` cells after the active cell.
    Next = 2,
    /// Consider up to `neighbor_count` cells on each side of the active cell.
    Both = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
/// Planned disposition of one natural-sequence neighbor.
pub enum LightTableBulkRegistrationAction {
    /// The source will be added to the target set.
    Add = 1,
    /// The source UUID is already present and will be preserved instead.
    SkipExisting = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Immutable issue-time token for Light Table bulk-registration preview and commit.
///
/// A request is valid only while the document revision, configured sequence
/// revision, active source identity, and target set remain unchanged. Callers may
/// preview it without mutation and then submit the exact token once. Stale or
/// invalid requests do not change document state, history, IDs, or dirty state.
pub struct LightTableBulkRegistrationRequest {
    /// Stable target-set ID in the active document.
    pub target_set_id: u64,
    /// Natural-sequence side selected by the caller.
    pub direction: LightTableBulkDirection,
    /// Maximum natural-neighbor distance considered on each selected side.
    pub neighbor_count: u32,
    /// Opacity assigned to distance one, in `0..=1000`.
    pub base_opacity_milli: u32,
    /// Opacity decrement per additional natural-neighbor distance, in `0..=1000`.
    pub distance_step_milli: u32,
    /// Document revision observed when the request was captured.
    pub base_document_revision: u64,
    /// Sequence-only revision observed when the request was captured.
    pub sequence_revision: u64,
    /// Persistent UUID of the active sequence source.
    pub active_document_uuid: u128,
    /// Immutable generation of the active sequence source.
    pub active_source_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One top-to-bottom entry in a Light Table bulk-registration preview.
pub struct LightTableBulkRegistrationEntry {
    /// Zero-based source index in natural sequence order.
    pub sequence_index: u32,
    /// Parsed user-visible source cell number.
    pub cell_number: u32,
    /// User-visible source name.
    pub name: String,
    /// Persistent UUID used by duplicate detection.
    pub document_uuid: u128,
    /// Immutable source generation that will become the Light Table source revision.
    pub source_generation: u64,
    /// Natural-neighbor distance from the active cell, starting at one.
    pub distance: u32,
    /// Linear opacity `max(0, base - step * (distance - 1))`.
    pub opacity_milli: u32,
    /// Whether commit adds this source or preserves an existing item.
    pub action: LightTableBulkRegistrationAction,
    /// Existing or earlier-planned source revision when `action` is `SkipExisting`.
    pub existing_source_revision: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Side-effect-free Light Table bulk-registration preview.
///
/// Entries are in final top-to-bottom Light Table order: later natural cells are
/// above earlier cells. Existing target-set items are not included and retain
/// their relative order below the newly added block.
pub struct LightTableBulkRegistrationPreview {
    /// Stable target-set ID captured by the request.
    pub target_set_id: u64,
    /// Candidate neighbors, including explicit duplicate skips.
    pub entries: Vec<LightTableBulkRegistrationEntry>,
    /// Number of entries that commit will add.
    pub add_count: u32,
    /// Number of entries that commit will preserve and skip.
    pub skip_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Result counts and stable IDs from one bulk-registration commit.
pub struct LightTableBulkRegistrationSummary {
    /// Number of new items committed.
    pub add_count: u32,
    /// Number of duplicate candidates skipped.
    pub skip_count: u32,
    /// Stable IDs of added items in final top-to-bottom order.
    pub added_item_ids: Vec<u64>,
}
