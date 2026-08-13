//! Read-only snapshots used by the native procedure-history viewer.

use crate::{BranchId, JournalEventId, PrimitiveId, ProcedureId, StateId, Thumbnail};

/// One committed canonical procedure and the visible composite produced by it.
///
/// Rows are returned in append-only [`JournalEventId`] order. The strings and
/// straight-alpha RGBA8 thumbnail are owned by the result, so callers may keep
/// the snapshot while the live document continues changing. Building rows is a
/// query: it does not change document or editor revisions, history, dirty state,
/// savepoints, caches, or persistent identity allocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryVisualizationRow {
    /// Append-only event containing this commit.
    pub journal_event_id: JournalEventId,
    /// Monotonic canonical procedure identity.
    pub procedure_id: ProcedureId,
    /// Stable built-in primitive identity.
    pub primitive_id: PrimitiveId,
    /// Persistent state produced by the procedure.
    pub committed_state_id: StateId,
    /// Retained journal branch extended by the procedure.
    pub branch_id: BranchId,
    /// Canonical primitive catalog name.
    pub primitive_name: String,
    /// Deterministic typed `field=value` argument presentation.
    pub arguments: String,
    /// Bounded visible composite after the procedure committed.
    pub thumbnail: Thumbnail,
}
