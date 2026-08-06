//! Append-only canonical procedure journal and runtime history reconstruction.

use super::*;
use crate::primitive::canonical_document_state;
use std::sync::Arc;

const MAX_JOURNAL_EVENTS: usize = 2_097_152;
const MAX_JOURNAL_BRANCHES: u64 = 65_536;
pub(crate) const MAX_JOURNAL_COMMITS: u64 = 1_048_576;

macro_rules! journal_id {
    ($name:ident, $docs:literal) => {
        #[doc = $docs]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(transparent)]
        pub struct $name(u64);

        impl $name {
            /// Returns the fixed-width numeric representation.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }

            pub(crate) const fn from_raw(value: u64) -> Self {
                Self(value)
            }

            pub(crate) const fn checked_next(self) -> Option<Self> {
                match self.0.checked_add(1) {
                    Some(value) if value <= MAX_PERSISTENT_NUMERIC_ID => Some(Self(value)),
                    None | Some(_) => None,
                }
            }
        }
    };
}

journal_id!(
    JournalEventId,
    "A nonzero, monotonically allocated identifier in one document journal. IDs remain unique until that document is replaced."
);
journal_id!(
    BranchId,
    "A nonzero identifier for one retained branch in a document journal. IDs remain unique until that document is replaced."
);

impl JournalEventId {
    pub(crate) const fn first() -> Self {
        Self(1)
    }
}

impl BranchId {
    /// Root branch created with a document's Genesis state.
    pub const ROOT: Self = Self(1);

    pub(crate) const fn first_unallocated() -> Self {
        Self(2)
    }
}

/// Kind of a history cursor movement recorded in the procedure journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum HistoryMoveKind {
    /// Moves from one committed state to its parent.
    Undo = 1,
    /// Moves from a state to its next state on the active branch.
    Redo = 2,
    /// Moves directly to another state on the selected active branch path.
    Jump = 3,
}

/// One committed canonical procedure record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalCommit {
    event_id: JournalEventId,
    procedure: Arc<CanonicalProcedure>,
    parent_state_id: StateId,
    committed_state_id: StateId,
    branch_id: BranchId,
}

impl JournalCommit {
    pub(crate) fn from_persistent(
        event_id: JournalEventId,
        procedure: Arc<CanonicalProcedure>,
        parent_state_id: StateId,
        committed_state_id: StateId,
        branch_id: BranchId,
    ) -> Self {
        Self {
            event_id,
            procedure,
            parent_state_id,
            committed_state_id,
            branch_id,
        }
    }
    /// Returns this record's monotonic event ID.
    #[must_use]
    pub const fn event_id(&self) -> JournalEventId {
        self.event_id
    }

    /// Borrows the canonical procedure retained by this commit.
    #[must_use]
    pub fn procedure(&self) -> &CanonicalProcedure {
        &self.procedure
    }

    /// Returns the state on which the procedure depends.
    #[must_use]
    pub const fn parent_state_id(&self) -> StateId {
        self.parent_state_id
    }

    /// Returns the state created by the procedure.
    #[must_use]
    pub const fn committed_state_id(&self) -> StateId {
        self.committed_state_id
    }

    /// Returns the branch whose tail this commit extends.
    #[must_use]
    pub const fn branch_id(&self) -> BranchId {
        self.branch_id
    }
}

/// One actual Undo, Redo, or history-jump record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalHistoryMove {
    event_id: JournalEventId,
    kind: HistoryMoveKind,
    source_state_id: StateId,
    destination_state_id: StateId,
    active_branch_id: BranchId,
}

impl JournalHistoryMove {
    pub(crate) const fn from_persistent(
        event_id: JournalEventId,
        kind: HistoryMoveKind,
        source_state_id: StateId,
        destination_state_id: StateId,
        active_branch_id: BranchId,
    ) -> Self {
        Self {
            event_id,
            kind,
            source_state_id,
            destination_state_id,
            active_branch_id,
        }
    }
    /// Returns this record's monotonic event ID.
    #[must_use]
    pub const fn event_id(self) -> JournalEventId {
        self.event_id
    }

    /// Returns the cursor movement kind.
    #[must_use]
    pub const fn kind(self) -> HistoryMoveKind {
        self.kind
    }

    /// Returns the state active before the move.
    #[must_use]
    pub const fn source_state_id(self) -> StateId {
        self.source_state_id
    }

    /// Returns the state active after the move.
    #[must_use]
    pub const fn destination_state_id(self) -> StateId {
        self.destination_state_id
    }

    /// Returns the branch active after the move.
    #[must_use]
    pub const fn active_branch_id(self) -> BranchId {
        self.active_branch_id
    }
}

/// One retained-redo-tail branch-cut record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalBranchCut {
    event_id: JournalEventId,
    fork_state_id: StateId,
    old_active_tail_state_id: StateId,
    new_branch_id: BranchId,
    deactivated_branch_id: BranchId,
}

impl JournalBranchCut {
    pub(crate) const fn from_persistent(
        event_id: JournalEventId,
        fork_state_id: StateId,
        old_active_tail_state_id: StateId,
        new_branch_id: BranchId,
        deactivated_branch_id: BranchId,
    ) -> Self {
        Self {
            event_id,
            fork_state_id,
            old_active_tail_state_id,
            new_branch_id,
            deactivated_branch_id,
        }
    }
    /// Returns this record's monotonic event ID.
    #[must_use]
    pub const fn event_id(self) -> JournalEventId {
        self.event_id
    }

    /// Returns the state at which the new branch forks.
    #[must_use]
    pub const fn fork_state_id(self) -> StateId {
        self.fork_state_id
    }

    /// Returns the retained tail of the branch that left the normal redo UI.
    #[must_use]
    pub const fn old_active_tail_state_id(self) -> StateId {
        self.old_active_tail_state_id
    }

    /// Returns the newly allocated active branch ID.
    #[must_use]
    pub const fn new_branch_id(self) -> BranchId {
        self.new_branch_id
    }

    /// Returns the branch deactivated by the cut.
    #[must_use]
    pub const fn deactivated_branch_id(self) -> BranchId {
        self.deactivated_branch_id
    }
}

/// Closed append-only journal record vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalEntry {
    /// One canonical document primitive commit.
    Commit(JournalCommit),
    /// One actual history cursor movement.
    HistoryMove(JournalHistoryMove),
    /// One branch cut immediately preceding its new-branch commit.
    BranchCut(JournalBranchCut),
}

/// Read-only history and journal high-level state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalState {
    complete: bool,
    current_state_id: StateId,
    savepoint_state_id: Option<StateId>,
    active_branch_id: BranchId,
    active_branch_tail_state_id: StateId,
    history_cursor: usize,
    visible_history_count: usize,
}

impl JournalState {
    /// Whether every current document commit is represented by a canonical record.
    ///
    /// Every production document mutation closes through a canonical procedure,
    /// so a live document always reports `true`.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.complete
    }

    /// Returns the state selected by the history cursor.
    #[must_use]
    pub const fn current_state_id(self) -> StateId {
        self.current_state_id
    }

    /// Returns the normal-save state, if one exists.
    #[must_use]
    pub const fn savepoint_state_id(self) -> Option<StateId> {
        self.savepoint_state_id
    }

    /// Returns the active retained branch.
    #[must_use]
    pub const fn active_branch_id(self) -> BranchId {
        self.active_branch_id
    }

    /// Returns the active branch's newest committed state.
    #[must_use]
    pub const fn active_branch_tail_state_id(self) -> StateId {
        self.active_branch_tail_state_id
    }

    /// Returns the active visible history cursor.
    #[must_use]
    pub const fn history_cursor(self) -> usize {
        self.history_cursor
    }

    /// Returns the number of visible entries on the active branch path.
    #[must_use]
    pub const fn visible_history_count(self) -> usize {
        self.visible_history_count
    }
}

/// Result of rebuilding the current state from Genesis and the append-only journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalReplayInfo {
    document_state_digest: DocumentStateDigest,
    current_state_id: StateId,
    active_branch_id: BranchId,
    history_cursor: usize,
    visible_history_count: usize,
}

impl JournalReplayInfo {
    /// Returns the rebuilt semantic document-state digest.
    #[must_use]
    pub const fn document_state_digest(self) -> DocumentStateDigest {
        self.document_state_digest
    }

    /// Returns the rebuilt current state.
    #[must_use]
    pub const fn current_state_id(self) -> StateId {
        self.current_state_id
    }

    /// Returns the rebuilt active branch.
    #[must_use]
    pub const fn active_branch_id(self) -> BranchId {
        self.active_branch_id
    }

    /// Returns the rebuilt visible history cursor.
    #[must_use]
    pub const fn history_cursor(self) -> usize {
        self.history_cursor
    }

    /// Returns the rebuilt visible history length.
    #[must_use]
    pub const fn visible_history_count(self) -> usize {
        self.visible_history_count
    }
}

pub(super) struct CanonicalCommitPlan {
    events: [Option<JournalEntry>; 2],
    event_count: usize,
    branch_id: BranchId,
    committed_state_id: StateId,
    following_event_id: JournalEventId,
    following_branch_id: BranchId,
    fork: Option<(BranchId, StateId)>,
}

pub(super) struct PreparedHistoryMove {
    movement: JournalHistoryMove,
    following_event_id: JournalEventId,
}

impl CanonicalCommitPlan {
    pub(super) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }
}

struct ReplayNode {
    parent: Option<StateId>,
    document: CellDocument,
    history_entry: Option<HistoryEntry>,
    next_id: StableIdCursor,
}

pub(super) struct RebuiltRuntime {
    pub(super) document: CellDocument,
    pub(super) history: Vec<HistoryEntry>,
    pub(super) history_cursor: usize,
    pub(super) next_id: StableIdCursor,
    pub(super) info: JournalReplayInfo,
}

impl Core {
    /// Borrows the append-only canonical procedure/history-control journal.
    ///
    /// Querying the journal does not change document, history, revision, dirty,
    /// savepoint, cache, or any persistent ID. The slice is the complete
    /// canonical representation of the live document history.
    #[must_use]
    pub fn journal_entries(&self) -> &[JournalEntry] {
        &self.journal
    }

    /// Returns persistent state, branch, cursor, and journal metadata.
    ///
    /// An empty Core has no document namespace and therefore returns `None`
    /// rather than exposing a zero sentinel through the nonzero ID types.
    #[must_use]
    pub fn journal_state(&self) -> Option<JournalState> {
        self.document.as_ref()?;
        Some(JournalState {
            complete: true,
            current_state_id: self.current_state,
            savepoint_state_id: self.savepoint,
            active_branch_id: self.active_branch,
            active_branch_tail_state_id: self.branch_tail(self.active_branch)?,
            history_cursor: self.history_cursor,
            visible_history_count: self.history.len(),
        })
    }

    /// Rebuilds the current semantic state from Genesis and the journal privately.
    ///
    /// The live Core is never changed. A malformed graph, failed procedure
    /// replay, or digest mismatch is returned as an error.
    pub fn verify_journal_replay(&self) -> Result<JournalReplayInfo, CoreError> {
        let rebuilt = self.rebuild_runtime_from_journal()?;
        self.validate_rebuilt_runtime(&rebuilt)?;
        Ok(rebuilt.info)
    }

    /// Releases optional runtime inverse/COW history data after validating replay.
    ///
    /// The document, visible history, cursor, journal, revisions, dirty state,
    /// savepoint, and persistent IDs remain unchanged. A later Undo/Redo/jump
    /// reconstructs the cache from Genesis and the journal before moving.
    pub fn release_history_cache(&mut self) -> Result<(), CoreError> {
        self.verify_journal_replay()?;
        for entry in &mut self.history {
            entry.change = None;
        }
        Ok(())
    }

    pub(super) fn reset_journal(&mut self) {
        self.journal.clear();
        self.active_branch = BranchId::ROOT;
        self.next_journal_event = JournalEventId::first();
        self.next_branch = BranchId::first_unallocated();
        self.branch_tails.clear();
        self.branch_tails.push(StateId::GENESIS);
        self.genesis = self.document.clone().map(genesis::Genesis::new);
    }

    pub(super) fn prepare_canonical_commit(
        &mut self,
        procedure: Arc<CanonicalProcedure>,
    ) -> Result<CanonicalCommitPlan, CoreError> {
        if procedure.procedure_id().get() > MAX_JOURNAL_COMMITS {
            return Err(CoreError::InvalidState("journal procedure limit exceeded"));
        }
        if procedure.base_state_id() != self.current_state
            || procedure.committed_state_id() != self.next_state
        {
            return Err(CoreError::InvalidState(
                "canonical procedure does not match journal state",
            ));
        }
        let active_tail = self
            .branch_tail(self.active_branch)
            .ok_or(CoreError::InvalidState("active journal branch is missing"))?;
        let required_events = if active_tail == self.current_state {
            1
        } else {
            2
        };
        if self.journal.len().saturating_add(required_events) > MAX_JOURNAL_EVENTS {
            return Err(CoreError::InvalidState("journal event limit exceeded"));
        }
        let mut events = [None, None];
        let mut event_count = 0;

        let mut following_event = self.next_journal_event;
        let mut following_branch = self.next_branch;
        let (branch_id, fork) = if active_tail == self.current_state {
            (self.active_branch, None)
        } else {
            if following_branch.get() > MAX_JOURNAL_BRANCHES {
                return Err(CoreError::InvalidState("journal branch limit exceeded"));
            }
            if usize::try_from(following_branch.get())
                .ok()
                .and_then(|value| value.checked_sub(1))
                != Some(self.branch_tails.len())
            {
                return Err(CoreError::InvalidState(
                    "journal branch high-watermark is inconsistent",
                ));
            }
            self.branch_tails
                .try_reserve(1)
                .map_err(|_| CoreError::InvalidState("journal branch allocation failed"))?;
            let cut_event = following_event;
            following_event = following_event
                .checked_next()
                .ok_or(CoreError::InvalidState("journal event ID overflow"))?;
            let branch = following_branch;
            following_branch = following_branch
                .checked_next()
                .ok_or(CoreError::InvalidState("branch ID overflow"))?;
            events[event_count] = Some(JournalEntry::BranchCut(JournalBranchCut {
                event_id: cut_event,
                fork_state_id: self.current_state,
                old_active_tail_state_id: active_tail,
                new_branch_id: branch,
                deactivated_branch_id: self.active_branch,
            }));
            event_count += 1;
            (branch, Some((branch, self.current_state)))
        };

        let commit_event = following_event;
        following_event = following_event
            .checked_next()
            .ok_or(CoreError::InvalidState("journal event ID overflow"))?;
        let committed_state_id = procedure.committed_state_id();
        events[event_count] = Some(JournalEntry::Commit(JournalCommit {
            event_id: commit_event,
            parent_state_id: procedure.base_state_id(),
            committed_state_id,
            procedure,
            branch_id,
        }));
        event_count += 1;
        self.journal
            .try_reserve(event_count)
            .map_err(|_| CoreError::InvalidState("journal allocation failed"))?;
        Ok(CanonicalCommitPlan {
            events,
            event_count,
            branch_id,
            committed_state_id,
            following_event_id: following_event,
            following_branch_id: following_branch,
            fork,
        })
    }

    pub(super) fn publish_canonical_commit(&mut self, plan: CanonicalCommitPlan) {
        if let Some((branch, fork_state)) = plan.fork {
            self.active_branch = branch;
            self.branch_tails.push(fork_state);
        }
        self.set_branch_tail(self.active_branch, plan.committed_state_id);
        self.next_journal_event = plan.following_event_id;
        self.next_branch = plan.following_branch_id;
        self.journal
            .extend(plan.events.into_iter().take(plan.event_count).flatten());
    }

    pub(super) fn prepare_history_move(
        &mut self,
        kind: HistoryMoveKind,
        source_state_id: StateId,
        destination_state_id: StateId,
    ) -> Result<Option<PreparedHistoryMove>, CoreError> {
        if self.journal.len() >= MAX_JOURNAL_EVENTS {
            return Err(CoreError::InvalidState("journal event limit exceeded"));
        }
        let following = self
            .next_journal_event
            .checked_next()
            .ok_or(CoreError::InvalidState("journal event ID overflow"))?;
        self.journal
            .try_reserve(1)
            .map_err(|_| CoreError::InvalidState("journal allocation failed"))?;
        let movement = JournalHistoryMove {
            event_id: self.next_journal_event,
            kind,
            source_state_id,
            destination_state_id,
            active_branch_id: self.active_branch,
        };
        Ok(Some(PreparedHistoryMove {
            movement,
            following_event_id: following,
        }))
    }

    pub(super) fn publish_history_move(&mut self, prepared: Option<PreparedHistoryMove>) {
        if let Some(prepared) = prepared {
            self.journal
                .push(JournalEntry::HistoryMove(prepared.movement));
            self.next_journal_event = prepared.following_event_id;
        }
    }

    pub(super) fn ensure_history_cache(&mut self) -> Result<(), CoreError> {
        if self.history.iter().all(|entry| entry.change.is_some()) {
            return Ok(());
        }
        let rebuilt = self.rebuild_runtime_from_journal()?;
        self.validate_rebuilt_runtime(&rebuilt)?;
        self.history = rebuilt.history;
        self.history_cursor = rebuilt.history_cursor;
        Ok(())
    }

    pub(super) fn rebuild_runtime_from_journal(&self) -> Result<RebuiltRuntime, CoreError> {
        if self.journal.len() > MAX_JOURNAL_EVENTS {
            return Err(CoreError::InvalidState("journal event limit exceeded"));
        }
        if self.next_procedure.get() > MAX_JOURNAL_COMMITS + 1 {
            return Err(CoreError::InvalidState("journal procedure limit exceeded"));
        }
        let mut genesis = self
            .genesis
            .as_ref()
            .map(|genesis| genesis.document.clone())
            .ok_or(CoreError::InvalidState("journal Genesis is missing"))?;
        let mut detached_assets = self
            .assets
            .detached_archive_round_trip(self.asset_retention_roots())?;
        genesis.light_table.intern_into(&mut detached_assets)?;
        let mut genesis_next_id = StableIdCursor::first();
        genesis_next_id.advance_past_raw(genesis.max_stable_id());
        let mut nodes = BTreeMap::new();
        nodes.insert(
            StateId::GENESIS,
            ReplayNode {
                parent: None,
                document: genesis,
                history_entry: None,
                next_id: genesis_next_id,
            },
        );
        let mut branches = BTreeMap::new();
        branches.insert(BranchId::ROOT, StateId::GENESIS);
        let mut current_state = StateId::GENESIS;
        let mut active_branch = BranchId::ROOT;
        let mut expected_event = JournalEventId::first();
        let mut expected_procedure = ProcedureId::first();
        let mut expected_state = StateId::GENESIS
            .checked_next()
            .ok_or(CoreError::InvalidState("Genesis state cannot advance"))?;
        let mut next_branch = BranchId::first_unallocated();
        let mut pending_cut: Option<(BranchId, StateId)> = None;

        for record in &self.journal {
            let event_id = match record {
                JournalEntry::Commit(commit) => commit.event_id,
                JournalEntry::HistoryMove(movement) => movement.event_id,
                JournalEntry::BranchCut(cut) => cut.event_id,
            };
            if event_id != expected_event {
                return Err(CoreError::InvalidState(
                    "journal event IDs are not contiguous",
                ));
            }
            expected_event = expected_event
                .checked_next()
                .ok_or(CoreError::InvalidState("journal event ID overflow"))?;

            match record {
                JournalEntry::Commit(commit) => {
                    if commit.procedure.procedure_id().get() > MAX_JOURNAL_COMMITS {
                        return Err(CoreError::InvalidState("journal procedure limit exceeded"));
                    }
                    if commit.procedure.procedure_id() != expected_procedure
                        || commit.committed_state_id != expected_state
                        || commit.parent_state_id != current_state
                        || commit.procedure.base_state_id() != commit.parent_state_id
                        || commit.procedure.committed_state_id() != commit.committed_state_id
                        || commit.branch_id != active_branch
                        || branches.get(&active_branch).copied() != Some(current_state)
                    {
                        return Err(CoreError::InvalidState(
                            "journal Commit violates procedure, state, or branch ordering",
                        ));
                    }
                    if let Some((branch, fork)) = pending_cut.take() {
                        if branch != commit.branch_id || fork != commit.parent_state_id {
                            return Err(CoreError::InvalidState(
                                "BranchCut is not followed by its matching Commit",
                            ));
                        }
                    }
                    let parent = nodes
                        .get(&commit.parent_state_id)
                        .ok_or(CoreError::InvalidState("journal parent state is missing"))?;
                    let mut replay = Core::new();
                    replay.assets = detached_assets.clone();
                    replay.document = Some(parent.document.clone());
                    replay.document_revision = DocumentRevision::from_raw(1);
                    replay.current_state = commit.parent_state_id;
                    replay.next_state = commit.committed_state_id;
                    replay.next_procedure = commit.procedure.procedure_id();
                    replay.next_id = parent.next_id;
                    replay.genesis = Some(genesis::Genesis::new(parent.document.clone()));
                    replay.branch_tails.clear();
                    replay.branch_tails.push(commit.parent_state_id);
                    replay.replay_procedure(&commit.procedure)?;
                    let mut cached = replay.history.pop().ok_or(CoreError::InvalidState(
                        "procedure replay has no history cache",
                    ))?;
                    cached.before_state = commit.parent_state_id;
                    cached.after_state = commit.committed_state_id;
                    cached.procedure = Some(Arc::clone(&commit.procedure));
                    cached.branch_id = commit.branch_id;
                    let next_id = replay.next_id;
                    let document = replay.document.ok_or(CoreError::NoDocument)?;
                    nodes.insert(
                        commit.committed_state_id,
                        ReplayNode {
                            parent: Some(commit.parent_state_id),
                            document,
                            history_entry: Some(cached),
                            next_id,
                        },
                    );
                    branches.insert(active_branch, commit.committed_state_id);
                    current_state = commit.committed_state_id;
                    expected_procedure = expected_procedure
                        .checked_next()
                        .ok_or(CoreError::InvalidState("procedure ID overflow"))?;
                    expected_state = expected_state
                        .checked_next()
                        .ok_or(CoreError::InvalidState("history state overflow"))?;
                }
                JournalEntry::HistoryMove(movement) => {
                    let branch_changes = movement.active_branch_id != active_branch;
                    if pending_cut.is_some()
                        || movement.source_state_id != current_state
                        || (movement.source_state_id == movement.destination_state_id
                            && !branch_changes)
                        || (branch_changes && movement.kind != HistoryMoveKind::Jump)
                        || !branches.contains_key(&movement.active_branch_id)
                        || !is_ancestor(
                            movement.destination_state_id,
                            branches[&movement.active_branch_id],
                            &nodes,
                        )
                    {
                        return Err(CoreError::InvalidState(
                            "journal HistoryMove violates branch ancestry",
                        ));
                    }
                    match movement.kind {
                        HistoryMoveKind::Undo
                            if nodes
                                .get(&movement.source_state_id)
                                .and_then(|node| node.parent)
                                != Some(movement.destination_state_id) =>
                        {
                            return Err(CoreError::InvalidState(
                                "Undo does not move to the parent state",
                            ));
                        }
                        HistoryMoveKind::Redo
                            if nodes
                                .get(&movement.destination_state_id)
                                .and_then(|node| node.parent)
                                != Some(movement.source_state_id) =>
                        {
                            return Err(CoreError::InvalidState(
                                "Redo does not move to a child state",
                            ));
                        }
                        HistoryMoveKind::Undo | HistoryMoveKind::Redo | HistoryMoveKind::Jump => {}
                    }
                    current_state = movement.destination_state_id;
                    active_branch = movement.active_branch_id;
                }
                JournalEntry::BranchCut(cut) => {
                    if pending_cut.is_some()
                        || cut.deactivated_branch_id != active_branch
                        || cut.fork_state_id != current_state
                        || branches.get(&active_branch).copied()
                            != Some(cut.old_active_tail_state_id)
                        || cut.fork_state_id == cut.old_active_tail_state_id
                        || cut.new_branch_id != next_branch
                        || cut.new_branch_id.get() > MAX_JOURNAL_BRANCHES
                        || branches.contains_key(&cut.new_branch_id)
                        || !is_ancestor(cut.fork_state_id, cut.old_active_tail_state_id, &nodes)
                    {
                        return Err(CoreError::InvalidState(
                            "journal BranchCut violates branch ordering",
                        ));
                    }
                    branches.insert(cut.new_branch_id, cut.fork_state_id);
                    active_branch = cut.new_branch_id;
                    pending_cut = Some((cut.new_branch_id, cut.fork_state_id));
                    next_branch = next_branch
                        .checked_next()
                        .ok_or(CoreError::InvalidState("branch ID overflow"))?;
                }
            }
        }
        if pending_cut.is_some() {
            return Err(CoreError::InvalidState(
                "journal ends with an uncommitted BranchCut",
            ));
        }
        if self
            .savepoint
            .is_some_and(|savepoint| !nodes.contains_key(&savepoint))
        {
            return Err(CoreError::InvalidState(
                "journal savepoint state is missing",
            ));
        }
        if current_state != self.current_state
            || active_branch != self.active_branch
            || !self.branch_tails_match(&branches)
            || expected_event != self.next_journal_event
            || expected_procedure != self.next_procedure
            || expected_state != self.next_state
            || next_branch != self.next_branch
        {
            return Err(CoreError::InvalidState(
                "journal replay high-watermarks do not match Core state",
            ));
        }

        let tail = branches[&active_branch];
        let mut state_path = Vec::new();
        let mut state = tail;
        while state != StateId::GENESIS {
            state_path.push(state);
            state = nodes
                .get(&state)
                .and_then(|node| node.parent)
                .ok_or(CoreError::InvalidState("active branch ancestry is broken"))?;
        }
        state_path.reverse();
        let history = state_path
            .iter()
            .map(|state| {
                nodes[state]
                    .history_entry
                    .clone()
                    .ok_or(CoreError::InvalidState("history cache is missing"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let history_cursor = if current_state == StateId::GENESIS {
            0
        } else {
            state_path
                .iter()
                .position(|state| *state == current_state)
                .map(|index| index + 1)
                .ok_or(CoreError::InvalidState(
                    "current state is not on the active branch",
                ))?
        };
        let current_node = nodes
            .get(&current_state)
            .ok_or(CoreError::InvalidState("current journal state is missing"))?;
        let document = current_node.document.clone();
        let next_id = current_node.next_id;
        let (_, document_state_digest) = canonical_document_state(&document)?;
        Ok(RebuiltRuntime {
            document,
            history,
            history_cursor,
            next_id,
            info: JournalReplayInfo {
                document_state_digest,
                current_state_id: current_state,
                active_branch_id: active_branch,
                history_cursor,
                visible_history_count: state_path.len(),
            },
        })
    }

    fn validate_rebuilt_runtime(&self, rebuilt: &RebuiltRuntime) -> Result<(), CoreError> {
        if rebuilt.info.document_state_digest != self.document_state_digest()? {
            return Err(CoreError::InvalidState(
                "journal replay does not match the live document state",
            ));
        }
        if rebuilt.history_cursor != self.history_cursor
            || rebuilt.history.len() != self.history.len()
            || rebuilt
                .history
                .iter()
                .zip(&self.history)
                .any(|(rebuilt, live)| !same_history_skeleton(rebuilt, live))
        {
            return Err(CoreError::InvalidState(
                "journal replay does not match the live history state",
            ));
        }
        Ok(())
    }

    fn branch_tail(&self, branch_id: BranchId) -> Option<StateId> {
        let index = usize::try_from(branch_id.get()).ok()?.checked_sub(1)?;
        self.branch_tails.get(index).copied()
    }

    pub(super) fn set_branch_tail(&mut self, branch_id: BranchId, state_id: StateId) {
        let index = usize::try_from(branch_id.get())
            .ok()
            .and_then(|value| value.checked_sub(1))
            .expect("journal branch IDs fit the platform index type");
        *self
            .branch_tails
            .get_mut(index)
            .expect("active journal branch exists") = state_id;
    }

    fn branch_tails_match(&self, branches: &BTreeMap<BranchId, StateId>) -> bool {
        branches.len() == self.branch_tails.len()
            && self.branch_tails.iter().enumerate().all(|(index, tail)| {
                let branch_id = BranchId((index as u64) + 1);
                branches.get(&branch_id) == Some(tail)
            })
    }
}

fn same_history_skeleton(left: &HistoryEntry, right: &HistoryEntry) -> bool {
    left.label == right.label
        && left.before_state == right.before_state
        && left.after_state == right.after_state
        && left.procedure == right.procedure
        && left.branch_id == right.branch_id
}

fn is_ancestor(
    ancestor: StateId,
    descendant: StateId,
    nodes: &BTreeMap<StateId, ReplayNode>,
) -> bool {
    let mut cursor = Some(descendant);
    while let Some(state) = cursor {
        if state == ancestor {
            return true;
        }
        cursor = nodes.get(&state).and_then(|node| node.parent);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized_core() -> Core {
        let mut core = Core::new();
        core.new_cell_with_uuid(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x4a4f_5552)
            .unwrap();
        core
    }

    fn set_main_line(core: &mut Core, value: u8) -> Result<PrimitiveOutcome, CoreError> {
        core.execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: core.document_revision.get(),
            color: PixelValue::Rgba([value, value, value, 255]),
        })
    }

    #[test]
    fn canonical_history_entry_references_the_journal_procedure() {
        let mut core = initialized_core();
        set_main_line(&mut core, 7).unwrap();
        let history_procedure = core.history[0].procedure.as_ref().unwrap();
        let JournalEntry::Commit(commit) = &core.journal[0] else {
            panic!("first journal record must be a commit");
        };
        assert!(Arc::ptr_eq(history_procedure, &commit.procedure));
    }

    #[test]
    fn journal_event_overflow_leaves_commit_state_untouched() {
        let mut core = initialized_core();
        core.next_journal_event = JournalEventId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
        let before_document = core.document.clone();
        let before_revision = core.document_revision;
        let before_history_len = core.history.len();
        let before_journal = core.journal.clone();
        let before_current = core.current_state;
        let before_next_state = core.next_state;
        let before_next_procedure = core.next_procedure;
        let before_next_event = core.next_journal_event;
        let before_branches = core.branch_tails.clone();

        assert!(matches!(
            set_main_line(&mut core, 8),
            Err(CoreError::InvalidState("journal event ID overflow"))
        ));
        assert_eq!(core.document, before_document);
        assert_eq!(core.document_revision, before_revision);
        assert_eq!(core.history.len(), before_history_len);
        assert_eq!(core.journal, before_journal);
        assert_eq!(core.current_state, before_current);
        assert_eq!(core.next_state, before_next_state);
        assert_eq!(core.next_procedure, before_next_procedure);
        assert_eq!(core.next_journal_event, before_next_event);
        assert_eq!(core.branch_tails, before_branches);
    }

    #[test]
    fn procedure_limit_failure_leaves_commit_state_untouched() {
        let mut core = initialized_core();
        core.next_procedure = ProcedureId::from_raw(MAX_JOURNAL_COMMITS + 1);
        let before_document = core.document.clone();
        let before_revision = core.document_revision;
        let before_history_len = core.history.len();
        let before_journal = core.journal.clone();
        let before_current = core.current_state;
        let before_next_state = core.next_state;
        let before_next_procedure = core.next_procedure;
        let before_next_event = core.next_journal_event;

        assert!(matches!(
            set_main_line(&mut core, 8),
            Err(CoreError::InvalidState("journal procedure limit exceeded"))
        ));
        assert_eq!(core.document, before_document);
        assert_eq!(core.document_revision, before_revision);
        assert_eq!(core.history.len(), before_history_len);
        assert_eq!(core.journal, before_journal);
        assert_eq!(core.current_state, before_current);
        assert_eq!(core.next_state, before_next_state);
        assert_eq!(core.next_procedure, before_next_procedure);
        assert_eq!(core.next_journal_event, before_next_event);
    }

    #[test]
    fn procedure_limit_accepts_the_exact_boundary_only() {
        let mut source = initialized_core();
        let mut procedure = set_main_line(&mut source, 8).unwrap().procedure.unwrap();
        Arc::make_mut(&mut procedure).procedure_id = ProcedureId::from_raw(MAX_JOURNAL_COMMITS);

        let mut target = initialized_core();
        target.next_procedure = ProcedureId::from_raw(MAX_JOURNAL_COMMITS);
        assert!(
            target
                .prepare_canonical_commit(Arc::clone(&procedure))
                .is_ok()
        );

        Arc::make_mut(&mut procedure).procedure_id = ProcedureId::from_raw(MAX_JOURNAL_COMMITS + 1);
        assert!(matches!(
            target.prepare_canonical_commit(procedure),
            Err(CoreError::InvalidState("journal procedure limit exceeded"))
        ));
    }

    #[test]
    fn branch_limit_failure_cannot_publish_a_lone_branch_cut() {
        let mut core = initialized_core();
        set_main_line(&mut core, 8).unwrap();
        set_main_line(&mut core, 9).unwrap();
        core.undo().unwrap();
        core.next_branch = BranchId::from_raw(MAX_JOURNAL_BRANCHES + 1);
        let before_document = core.document.clone();
        let before_revision = core.document_revision;
        let before_history_len = core.history.len();
        let before_cursor = core.history_cursor;
        let before_journal = core.journal.clone();
        let before_state = core.current_state;
        let before_next_state = core.next_state;
        let before_next_procedure = core.next_procedure;
        let before_next_event = core.next_journal_event;
        let before_next_branch = core.next_branch;
        let before_branches = core.branch_tails.clone();

        assert!(matches!(
            set_main_line(&mut core, 10),
            Err(CoreError::InvalidState("journal branch limit exceeded"))
        ));
        assert_eq!(core.document, before_document);
        assert_eq!(core.document_revision, before_revision);
        assert_eq!(core.history.len(), before_history_len);
        assert_eq!(core.history_cursor, before_cursor);
        assert_eq!(core.journal, before_journal);
        assert_eq!(core.current_state, before_state);
        assert_eq!(core.next_state, before_next_state);
        assert_eq!(core.next_procedure, before_next_procedure);
        assert_eq!(core.next_journal_event, before_next_event);
        assert_eq!(core.next_branch, before_next_branch);
        assert_eq!(core.branch_tails, before_branches);
    }

    #[test]
    fn second_event_overflow_cannot_publish_a_lone_branch_cut() {
        let mut core = initialized_core();
        set_main_line(&mut core, 8).unwrap();
        set_main_line(&mut core, 9).unwrap();
        core.undo().unwrap();
        core.next_journal_event = JournalEventId::from_raw(MAX_PERSISTENT_NUMERIC_ID - 1);
        let before_document = core.document.clone();
        let before_revision = core.document_revision;
        let before_history_len = core.history.len();
        let before_cursor = core.history_cursor;
        let before_journal = core.journal.clone();
        let before_state = core.current_state;
        let before_next_state = core.next_state;
        let before_next_procedure = core.next_procedure;
        let before_next_event = core.next_journal_event;
        let before_next_branch = core.next_branch;
        let before_branches = core.branch_tails.clone();

        assert!(matches!(
            set_main_line(&mut core, 10),
            Err(CoreError::InvalidState("journal event ID overflow"))
        ));
        assert_eq!(core.document, before_document);
        assert_eq!(core.document_revision, before_revision);
        assert_eq!(core.history.len(), before_history_len);
        assert_eq!(core.history_cursor, before_cursor);
        assert_eq!(core.journal, before_journal);
        assert_eq!(core.current_state, before_state);
        assert_eq!(core.next_state, before_next_state);
        assert_eq!(core.next_procedure, before_next_procedure);
        assert_eq!(core.next_journal_event, before_next_event);
        assert_eq!(core.next_branch, before_next_branch);
        assert_eq!(core.branch_tails, before_branches);
    }

    #[test]
    fn history_move_event_overflow_leaves_cursor_and_document_untouched() {
        let mut core = initialized_core();
        set_main_line(&mut core, 8).unwrap();
        core.next_journal_event = JournalEventId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
        let before_document = core.document.clone();
        let before_revision = core.document_revision;
        let before_cursor = core.history_cursor;
        let before_state = core.current_state;
        let before_journal = core.journal.clone();

        assert!(matches!(
            core.undo(),
            Err(CoreError::InvalidState("journal event ID overflow"))
        ));
        assert_eq!(core.document, before_document);
        assert_eq!(core.document_revision, before_revision);
        assert_eq!(core.history_cursor, before_cursor);
        assert_eq!(core.current_state, before_state);
        assert_eq!(core.journal, before_journal);
        assert_eq!(
            core.next_journal_event,
            JournalEventId::from_raw(MAX_PERSISTENT_NUMERIC_ID)
        );
    }

    #[test]
    fn replay_rejects_redundant_branch_cut_at_the_active_tail() {
        let mut core = initialized_core();
        set_main_line(&mut core, 8).unwrap();
        set_main_line(&mut core, 9).unwrap();

        let JournalEntry::Commit(second) = &mut core.journal[1] else {
            panic!("second journal record must be a commit");
        };
        second.event_id = JournalEventId::from_raw(3);
        second.branch_id = BranchId::from_raw(2);
        core.journal.insert(
            1,
            JournalEntry::BranchCut(JournalBranchCut {
                event_id: JournalEventId::from_raw(2),
                fork_state_id: StateId::from_raw(2),
                old_active_tail_state_id: StateId::from_raw(2),
                new_branch_id: BranchId::from_raw(2),
                deactivated_branch_id: BranchId::ROOT,
            }),
        );
        core.active_branch = BranchId::from_raw(2);
        core.next_journal_event = JournalEventId::from_raw(4);
        core.next_branch = BranchId::from_raw(3);
        core.branch_tails[0] = StateId::from_raw(2);
        core.branch_tails.push(StateId::from_raw(3));
        core.history[1].branch_id = BranchId::from_raw(2);

        assert!(matches!(
            core.verify_journal_replay(),
            Err(CoreError::InvalidState(
                "journal BranchCut violates branch ordering"
            ))
        ));
    }

    #[test]
    fn replay_accepts_jump_to_a_retained_inactive_branch() {
        let mut core = initialized_core();
        set_main_line(&mut core, 8).unwrap();
        set_main_line(&mut core, 9).unwrap();
        core.undo().unwrap();
        set_main_line(&mut core, 10).unwrap();

        core.journal
            .push(JournalEntry::HistoryMove(JournalHistoryMove {
                event_id: core.next_journal_event,
                kind: HistoryMoveKind::Jump,
                source_state_id: StateId::from_raw(4),
                destination_state_id: StateId::from_raw(3),
                active_branch_id: BranchId::ROOT,
            }));
        core.next_journal_event = core.next_journal_event.checked_next().unwrap();
        core.current_state = StateId::from_raw(3);
        core.active_branch = BranchId::ROOT;

        let rebuilt = core.rebuild_runtime_from_journal().unwrap();
        assert_eq!(rebuilt.info.current_state_id(), StateId::from_raw(3));
        assert_eq!(rebuilt.info.active_branch_id(), BranchId::ROOT);
        assert_eq!(rebuilt.info.history_cursor(), 2);
        assert_eq!(rebuilt.info.visible_history_count(), 2);
    }

    #[test]
    fn cache_release_rejects_live_history_skeleton_drift() {
        let mut core = initialized_core();
        set_main_line(&mut core, 8).unwrap();
        core.history[0].label = "forged history label";

        assert!(matches!(
            core.verify_journal_replay(),
            Err(CoreError::InvalidState(
                "journal replay does not match the live history state"
            ))
        ));
        assert!(matches!(
            core.release_history_cache(),
            Err(CoreError::InvalidState(
                "journal replay does not match the live history state"
            ))
        ));
        assert!(core.history[0].change.is_some());
    }
}
