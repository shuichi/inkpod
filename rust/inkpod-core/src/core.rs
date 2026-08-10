//! Core construction and top-level document lifecycle entry points.

use super::*;

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    /// Returns immutable identity and base-surface metadata for active Genesis.
    ///
    /// This query does not change document, history, revisions, asset retention,
    /// or renderer caches.
    pub fn genesis_info(&self) -> Result<GenesisInfo, CoreError> {
        self.genesis
            .as_ref()
            .map(genesis::Genesis::info)
            .ok_or(CoreError::NoDocument)
    }

    /// Returns deterministic logical usage for the active Core asset store.
    ///
    /// The count includes assets retained by inactive journal branches and redo
    /// tails. This read-only query does not perform garbage collection.
    #[must_use]
    pub fn asset_store_usage(&self) -> AssetStoreUsage {
        self.assets.usage()
    }

    /// Returns metadata for one registered immutable asset identity.
    #[must_use]
    pub fn asset_info(&self, id: AssetId) -> Option<AssetInfo> {
        self.assets.info(id)
    }

    /// Returns every registered asset in ascending [`AssetId`] order.
    #[must_use]
    pub fn asset_infos(&self) -> Vec<AssetInfo> {
        self.assets.infos()
    }

    pub(super) fn allocate_plane_id(&mut self) -> PlaneId {
        self.next_id.take_plane()
    }

    pub(super) fn allocate_guide_id(&mut self) -> GuideId {
        self.next_id.take_guide()
    }

    pub(super) fn allocate_light_table_set_id(&mut self) -> LightTableSetId {
        self.next_id.take_light_table_set()
    }

    pub(super) fn allocate_light_table_item_id(&mut self) -> LightTableItemId {
        self.next_id.take_light_table_item()
    }

    /// Creates an empty single-writer Core with no open document.
    #[must_use]
    pub fn new() -> Self {
        let shortcuts = default_shortcuts();
        let editor_defaults = EditorDefaults::built_in();
        Self {
            assets: asset::AssetStore::default(),
            document: None,
            document_revision: DocumentRevision::from_raw(0),
            view: ViewState::default(),
            history: Vec::new(),
            history_cursor: 0,
            staged_history: None,
            current_state: StateId::from_raw(0),
            next_state: StateId::GENESIS,
            next_procedure: ProcedureId::first(),
            savepoint: None,
            genesis: None,
            journal: Vec::new(),
            canonical_state_cache: std::cell::RefCell::new(None),
            active_branch: BranchId::ROOT,
            next_journal_event: JournalEventId::first(),
            next_branch: BranchId::first_unallocated(),
            branch_tails: Vec::with_capacity(1),
            next_id: StableIdCursor::first(),
            current_path: None,
            recovered: false,
            active_stroke: None,
            filter_preview: None,
            last_filter: None,
            render_cache: BTreeMap::new(),
            next_render_tile_revision: RenderRevision::from_raw(1),
            next_preview_revision: PreviewRevision::from_raw(1_u64 << 63),
            color_check: None,
            secondary_views: BTreeMap::new(),
            next_view_id: ViewId::from_raw(1),
            floating: None,
            shortcut_defaults: shortcuts.clone(),
            shortcuts,
            sequence: None,
            motion_check: None,
            subpalette_index: None,
            editor_defaults,
            editor_session: None,
            native_opaque_sections: Vec::new(),
            last_open_strategy: NativeOpenStrategy::NotOpened,
            canonical_invocation_active: false,
        }
    }

    /// Replaces the current document with a blank cell using a generated UUID.
    ///
    /// Dimensions and DPI must satisfy the image limits. Success cancels active
    /// transient sessions, resets history and view state, and establishes a clean
    /// in-memory savepoint. Validation failure leaves the current document intact.
    pub fn new_cell(
        &mut self,
        width: u32,
        height: u32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
    ) -> Result<DocumentInfo, CoreError> {
        let uuid = (u128::from(0x494e_4b50_4f44_4d31_u64) << 64)
            | u128::from(self.next_document_revision()?.get());
        self.new_cell_with_uuid_and_layer(
            width,
            height,
            dpi_x_milli,
            dpi_y_milli,
            uuid,
            LayerKind::BinaryColoring,
        )
    }

    /// Replaces the current document with a blank cell using `document_uuid`.
    ///
    /// This has the same revision, history, dirty, and cancellation semantics as
    /// [`Core::new_cell`], while allowing a caller-controlled persistent UUID.
    pub fn new_cell_with_uuid(
        &mut self,
        width: u32,
        height: u32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
        document_uuid: u128,
    ) -> Result<DocumentInfo, CoreError> {
        self.new_cell_with_uuid_and_layer(
            width,
            height,
            dpi_x_milli,
            dpi_y_milli,
            document_uuid,
            LayerKind::BinaryColoring,
        )
    }

    /// Replaces the current document with a blank cell and typed initial layer.
    ///
    /// The complete Genesis topology is validated and allocated before live
    /// state is published. This session-replacement operation resets history;
    /// it never creates an intermediate default-layer document or a procedure.
    pub fn new_cell_with_uuid_and_layer(
        &mut self,
        width: u32,
        height: u32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
        document_uuid: u128,
        initial_layer_kind: LayerKind,
    ) -> Result<DocumentInfo, CoreError> {
        self.new_cell_with_creation_spec(
            width,
            height,
            dpi_x_milli,
            dpi_y_milli,
            document_uuid,
            initial_layer_kind,
            PixelFormat::StraightRgba8,
            None,
        )
    }

    /// Replaces the current document from one validated immutable creation-plan item.
    ///
    /// Planning owns no Core state. This commit validates and constructs the complete
    /// Genesis privately, consumes stable IDs only on success, resets history to a
    /// clean savepoint, and never publishes an intermediate document.
    pub fn new_cell_from_creation_plan(
        &mut self,
        item: &CellCreationPlanItem,
        document_uuid: u128,
    ) -> Result<DocumentInfo, CoreError> {
        self.new_cell_with_creation_spec(
            item.width(),
            item.height(),
            item.dpi_x_milli(),
            item.dpi_y_milli(),
            document_uuid,
            item.initial_layer_kind(),
            item.pixel_format(),
            Some(item.frames()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_cell_with_creation_spec(
        &mut self,
        width: u32,
        height: u32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
        document_uuid: u128,
        initial_layer_kind: LayerKind,
        color_format: PixelFormat,
        frames: Option<FrameMetadata>,
    ) -> Result<DocumentInfo, CoreError> {
        // Construct the complete replacement and its advanced cursor privately
        // so invalid paper/UUID input cannot consume stable IDs or disturb the
        // current live document. The established public surface keeps object
        // IDs monotonic across document replacement within one Core session.
        let mut next_id = self.next_id;
        let ids = DocumentIds {
            document: next_id.take_document(),
            layer: next_id.take_layer(),
            main_plane: next_id.take_plane(),
            color_plane: next_id.take_plane(),
            selection_plane: next_id.take_plane(),
            light_table_set: next_id.take_light_table_set(),
            cell: next_id.take_cell(),
        };
        let mut initial_layer_id = ids.layer;
        let mut document = CellDocument::new(
            ids,
            document_uuid,
            PaperSpec {
                width,
                height,
                dpi_x_milli,
                dpi_y_milli,
            },
        )?;
        if let Some(frames) = frames {
            document.frames = frames;
        }
        let base_color = document.layers[0]
            .planes
            .iter_mut()
            .find(|plane| plane.kind == PlaneType::Color)
            .ok_or(CoreError::InvalidState(
                "blank coloring base has no color plane",
            ))?;
        base_color.raster = TileRaster::new(width, height, color_format)?;
        if initial_layer_kind == LayerKind::GrayscaleColoring {
            document.layers[0] = document::build_layer_node_with_format(
                initial_layer_kind,
                "Layer 1",
                initial_layer_id,
                width,
                height,
                color_format,
                &mut next_id,
            )?;
        } else if initial_layer_kind != LayerKind::BinaryColoring {
            initial_layer_id = next_id.take_layer();
            let requested = document::build_layer_node_with_format(
                initial_layer_kind,
                "Layer 1",
                initial_layer_id,
                width,
                height,
                color_format,
                &mut next_id,
            )?;
            // Non-coloring layers cannot replace the required coloring base.
            // Put the requested layer first so reset_editor_state selects it.
            document.layers.insert(0, requested);
            if initial_layer_kind == LayerKind::Adjustment {
                document.adjustments.insert(
                    initial_layer_id,
                    Adjustment::BrightnessContrast {
                        brightness_milli: 0,
                        contrast_milli: 0,
                    },
                );
            }
        }
        let revision = self.next_document_revision()?;

        self.cancel_stroke();
        self.filter_preview = None;
        self.last_filter = None;
        self.render_cache.clear();
        self.next_id = next_id;
        self.assets = asset::AssetStore::default();
        self.document = Some(document);
        self.document_revision = revision;
        *self.canonical_state_cache.get_mut() = None;
        // A new blank cell is the initial in-memory savepoint even though it
        // does not have a normal-save path yet. Pathlessness controls whether
        // Save needs a destination; it must not make an unedited document
        // appear modified.
        self.reset_history(true);
        self.reset_view();
        self.current_path = None;
        self.recovered = false;
        self.native_opaque_sections.clear();
        self.last_open_strategy = NativeOpenStrategy::NotOpened;
        self.color_check = None;
        self.secondary_views.clear();
        self.floating = None;
        self.sequence = None;
        self.motion_check = None;
        self.subpalette_index = None;
        self.reset_editor_state(true);
        self.document_info()
    }
}

/// Single-writer application core. Document and view revisions are independent.
#[derive(Clone, Debug)]
pub struct Core {
    pub(super) assets: asset::AssetStore,
    pub(super) document: Option<CellDocument>,
    pub(super) document_revision: DocumentRevision,
    pub(super) view: ViewState,
    pub(super) history: Vec<HistoryEntry>,
    pub(super) history_cursor: usize,
    pub(super) staged_history: Option<StagedHistoryEntry>,
    pub(super) current_state: StateId,
    pub(super) next_state: StateId,
    pub(super) next_procedure: ProcedureId,
    pub(super) savepoint: Option<StateId>,
    pub(super) genesis: Option<genesis::Genesis>,
    pub(super) journal: Vec<JournalEntry>,
    pub(super) canonical_state_cache:
        std::cell::RefCell<Option<primitive::CanonicalDocumentStateCache>>,
    pub(super) active_branch: BranchId,
    pub(super) next_journal_event: JournalEventId,
    pub(super) next_branch: BranchId,
    pub(super) branch_tails: Vec<StateId>,
    pub(super) next_id: StableIdCursor,
    pub(super) current_path: Option<PathBuf>,
    pub(super) recovered: bool,
    pub(super) active_stroke: Option<StrokeSession>,
    pub(super) filter_preview: Option<effects::FilterPreview>,
    pub(super) last_filter: Option<Filter>,
    pub(super) render_cache: BTreeMap<(u64, TileCoord), RenderTile>,
    pub(super) next_render_tile_revision: RenderRevision,
    pub(super) next_preview_revision: PreviewRevision,
    pub(super) color_check: Option<ColorCheckMode>,
    pub(super) secondary_views: BTreeMap<ViewId, ViewState>,
    pub(super) next_view_id: ViewId,
    pub(super) floating: Option<FloatingSelection>,
    pub(super) shortcut_defaults: BTreeMap<u32, Vec<ShortcutStroke>>,
    pub(super) shortcuts: BTreeMap<u32, Vec<ShortcutStroke>>,
    pub(super) sequence: Option<animation::SequenceState>,
    pub(super) motion_check: Option<animation::MotionCheckState>,
    pub(super) subpalette_index: Option<usize>,
    pub(super) editor_defaults: EditorDefaults,
    pub(super) editor_session: Option<EditorSessionState>,
    pub(super) native_opaque_sections: Vec<NativeSection>,
    pub(super) last_open_strategy: NativeOpenStrategy,
    pub(super) canonical_invocation_active: bool,
}

/// One synchronous document edit staged independently from the live Core state.
///
/// Only `working` is exposed mutably. Committing consumes the edit, verifies
/// that its base revision is still current, and publishes document, history,
/// revision, and cache changes as one operation.
#[derive(Debug)]
pub(super) struct DocumentEdit {
    before: CellDocument,
    working: CellDocument,
    base_revision: DocumentRevision,
    commit_revision: DocumentRevision,
    preferred_editor_target: Option<EditorTarget>,
    preferred_edit_targets: Option<Vec<EditTarget>>,
    preserve_render_cache_by_raster_revision: bool,
}

impl DocumentEdit {
    fn begin(core: &Core) -> Result<Self, CoreError> {
        let before = core.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        Ok(Self {
            working: before.clone(),
            before,
            base_revision: core.document_revision,
            commit_revision: core.next_document_revision()?,
            preferred_editor_target: None,
            preferred_edit_targets: None,
            preserve_render_cache_by_raster_revision: false,
        })
    }

    fn from_staged(
        before: CellDocument,
        working: CellDocument,
        base_revision: DocumentRevision,
        commit_revision: DocumentRevision,
    ) -> Self {
        Self {
            before,
            working,
            base_revision,
            commit_revision,
            preferred_editor_target: None,
            preferred_edit_targets: None,
            preserve_render_cache_by_raster_revision: false,
        }
    }

    pub(super) const fn working_mut(&mut self) -> &mut CellDocument {
        &mut self.working
    }

    pub(super) const fn documents(&mut self) -> (&CellDocument, &mut CellDocument) {
        (&self.before, &mut self.working)
    }

    pub(super) const fn revision(&self) -> DocumentRevision {
        self.commit_revision
    }

    pub(super) fn prefer_editor_target(&mut self, target: EditorTarget) {
        self.preferred_editor_target = Some(target);
    }

    pub(super) fn prefer_edit_targets(&mut self, targets: Vec<EditTarget>) {
        self.preferred_edit_targets = Some(targets);
    }

    pub(super) fn preserve_render_cache_by_raster_revision(&mut self) {
        self.preserve_render_cache_by_raster_revision = true;
    }

    pub(super) fn commit(self, core: &mut Core) -> Result<DispatchOutcome, CoreError> {
        if !core.canonical_invocation_is_active() {
            return Err(CoreError::InvalidState(
                "document edit commit requires a canonical primitive",
            ));
        }
        if core.document_revision != self.base_revision {
            return Err(CoreError::InvalidState(
                "document edit base revision is stale",
            ));
        }
        if self.base_revision.checked_next() != Some(self.commit_revision) {
            return Err(CoreError::InvalidState(
                "document edit commit revision does not follow its base",
            ));
        }
        if self.before == self.working {
            return Ok(core.noop_outcome());
        }

        let editor = core.stage_reconciled_editor_target(
            &self.working,
            self.preferred_editor_target,
            self.preferred_edit_targets.as_deref(),
        )?;
        let after_state = core.allocate_state()?;
        let change = HistoryChange::Document {
            before: Box::new(self.before),
            after: Box::new(self.working.clone()),
        };

        core.document = Some(self.working);
        core.document_revision = self.commit_revision;
        if !self.preserve_render_cache_by_raster_revision {
            core.render_cache.clear();
        }
        core.commit_history_change(change, after_state);
        core.publish_editor_session(editor);
        Ok(DispatchOutcome {
            revision: self.commit_revision.get(),
            accepted_commands: 1,
        })
    }
}

impl Core {
    pub(super) const fn noop_outcome(&self) -> DispatchOutcome {
        DispatchOutcome {
            revision: self.document_revision.get(),
            accepted_commands: 1,
        }
    }

    pub(super) fn begin_document_edit(&self) -> Result<DocumentEdit, CoreError> {
        DocumentEdit::begin(self)
    }

    /// Publishes work staged by an explicitly excluded session, cancellable,
    /// reload, or potentially long-running path through the same atomic
    /// commit boundary without changing that path's ownership design.
    pub(super) fn commit_deferred_document_edit(
        &mut self,
        before: CellDocument,
        working: CellDocument,
        base_revision: DocumentRevision,
        commit_revision: DocumentRevision,
    ) -> Result<DispatchOutcome, CoreError> {
        DocumentEdit::from_staged(before, working, base_revision, commit_revision).commit(self)
    }

    pub(super) fn commit_deferred_document_edit_with_target(
        &mut self,
        before: CellDocument,
        working: CellDocument,
        base_revision: DocumentRevision,
        commit_revision: DocumentRevision,
        target: EditorTarget,
    ) -> Result<DispatchOutcome, CoreError> {
        let mut edit = DocumentEdit::from_staged(before, working, base_revision, commit_revision);
        edit.prefer_editor_target(target);
        edit.commit(self)
    }

    pub(super) fn commit_deferred_document_edit_current(
        &mut self,
        before: CellDocument,
        working: CellDocument,
    ) -> Result<DispatchOutcome, CoreError> {
        let base_revision = self.document_revision;
        let commit_revision = self.next_document_revision()?;
        self.commit_deferred_document_edit(before, working, base_revision, commit_revision)
    }

    pub(super) fn next_document_revision(&self) -> Result<DocumentRevision, CoreError> {
        self.document_revision
            .checked_next()
            .ok_or(CoreError::InvalidState("document revision overflow"))
    }

    pub(super) fn allocate_preview_revision(&mut self) -> Result<PreviewRevision, CoreError> {
        let revision = self.next_preview_revision;
        self.next_preview_revision = self
            .next_preview_revision
            .checked_next()
            .ok_or(CoreError::InvalidState("preview revision overflow"))?;
        Ok(revision)
    }

    pub(super) fn ensure_no_active_stroke(&self) -> Result<(), CoreError> {
        if self.active_stroke.is_some() || self.filter_preview.is_some() {
            Err(CoreError::InvalidState(
                "operation is not allowed during an active preview transaction",
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn ensure_no_active_raster_stroke(&self) -> Result<(), CoreError> {
        if self.active_stroke.is_some() {
            Err(CoreError::InvalidState(
                "operation is not allowed during an active stroke transaction",
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn allocate_state(&mut self) -> Result<StateId, CoreError> {
        // Restore optional inverse data before staging a canonical commit.
        self.ensure_history_cache()?;
        let state = self.next_state;
        self.next_state = self
            .next_state
            .checked_next()
            .ok_or(CoreError::InvalidState("history state overflow"))?;
        Ok(state)
    }

    pub(super) fn reset_history(&mut self, saved: bool) {
        self.history.clear();
        self.history_cursor = 0;
        self.staged_history = None;
        // Persistent state/procedure IDs belong to the logical document. A
        // replacement document always starts at the closed Genesis values,
        // independently of the Core session's prior document.
        self.current_state = StateId::GENESIS;
        self.next_state = StateId::from_raw(2);
        self.next_procedure = ProcedureId::first();
        self.savepoint = saved.then_some(self.current_state);
        self.reset_journal();
    }

    pub(super) fn reset_view(&mut self) {
        let revision = self.view.revision.saturating_next();
        self.view = ViewState {
            revision,
            ..ViewState::default()
        };
    }

    pub(super) fn commit_pixel_history(
        &mut self,
        plane_id: PlaneId,
        changes: Vec<PixelChange>,
        after_state: StateId,
    ) {
        self.commit_history_change(HistoryChange::Pixels { plane_id, changes }, after_state);
    }

    pub(super) fn commit_history_change(&mut self, change: HistoryChange, after_state: StateId) {
        debug_assert!(
            self.canonical_invocation_active,
            "document history may only be staged by a canonical invocation"
        );
        debug_assert!(
            self.staged_history.is_none(),
            "a canonical invocation may stage only one history entry"
        );
        let before_state = self.current_state;
        let label = change.label();
        self.staged_history = Some(StagedHistoryEntry {
            change,
            label,
            before_state,
            after_state,
        });
        self.current_state = after_state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized_core() -> Core {
        let mut core = Core::new();
        core.new_cell(4, 4, 96_000, 96_000).unwrap();
        core.canonical_invocation_active = true;
        core
    }

    #[test]
    fn unchanged_transaction_preserves_revision_history_state_and_cache() {
        let mut core = initialized_core();
        let _ = core.build_snapshot();
        let before_revision = core.document_revision;
        let before_history = core.history.clone();
        let before_state = core.current_state;
        let before_next_state = core.next_state;
        let before_cache = core.render_cache.clone();

        let edit = core.begin_document_edit().unwrap();
        let outcome = edit.commit(&mut core).unwrap();

        assert_eq!(outcome.revision, before_revision.get());
        assert_eq!(core.document_revision, before_revision);
        assert_eq!(core.history.len(), before_history.len());
        assert_eq!(core.current_state, before_state);
        assert_eq!(core.next_state, before_next_state);
        assert_eq!(core.render_cache, before_cache);
    }

    #[test]
    fn changed_transaction_commits_once_and_undo_redo_uses_the_right_sides() {
        let mut core = initialized_core();
        let _ = core.build_snapshot();
        let before = core.document.clone().unwrap();
        let before_revision = core.document_revision;
        let mut edit = core.begin_document_edit().unwrap();
        edit.working_mut().grid.origin_x = 7;
        let expected = edit.working_mut().clone();

        let outcome = edit.commit(&mut core).unwrap();

        assert_eq!(
            outcome.revision,
            before_revision.checked_next().unwrap().get()
        );
        assert_eq!(core.history.len(), 0);
        assert_eq!(core.history_cursor, 0);
        assert!(core.staged_history.is_some());
        assert!(core.render_cache.is_empty());
        assert_eq!(core.document.as_ref(), Some(&expected));
        let staged = core.staged_history.as_ref().unwrap();
        let HistoryChange::Document {
            before: staged_before,
            after: staged_after,
        } = &staged.change
        else {
            panic!("changed document transaction staged the wrong history kind");
        };
        assert_eq!(staged_before.as_ref(), &before);
        assert_eq!(staged_after.as_ref(), &expected);
    }

    #[test]
    fn stale_and_overflow_failures_publish_no_partial_state() {
        let mut stale_core = initialized_core();
        let _ = stale_core.build_snapshot();
        let mut stale_edit = stale_core.begin_document_edit().unwrap();
        stale_edit.working_mut().grid.origin_y = 9;
        stale_core.document_revision = stale_core.document_revision.checked_next().unwrap();
        let stale_document = stale_core.document.clone();
        let stale_history_len = stale_core.history.len();
        let stale_next_state = stale_core.next_state;
        let stale_cache = stale_core.render_cache.clone();

        assert!(matches!(
            stale_edit.commit(&mut stale_core),
            Err(CoreError::InvalidState(
                "document edit base revision is stale"
            ))
        ));
        assert_eq!(stale_core.document, stale_document);
        assert_eq!(stale_core.history.len(), stale_history_len);
        assert_eq!(stale_core.next_state, stale_next_state);
        assert_eq!(stale_core.render_cache, stale_cache);

        let mut history_overflow_core = initialized_core();
        let _ = history_overflow_core.build_snapshot();
        let mut overflow_edit = history_overflow_core.begin_document_edit().unwrap();
        overflow_edit.working_mut().grid.origin_x = 11;
        history_overflow_core.next_state = StateId::from_raw(u64::MAX);
        let overflow_document = history_overflow_core.document.clone();
        let overflow_revision = history_overflow_core.document_revision;
        let overflow_cache = history_overflow_core.render_cache.clone();

        assert!(matches!(
            overflow_edit.commit(&mut history_overflow_core),
            Err(CoreError::InvalidState("history state overflow"))
        ));
        assert_eq!(history_overflow_core.document, overflow_document);
        assert_eq!(history_overflow_core.document_revision, overflow_revision);
        assert_eq!(history_overflow_core.history.len(), 0);
        assert_eq!(
            history_overflow_core.next_state,
            StateId::from_raw(u64::MAX)
        );
        assert_eq!(history_overflow_core.render_cache, overflow_cache);

        let mut revision_overflow_core = initialized_core();
        revision_overflow_core.document_revision = DocumentRevision::from_raw(u64::MAX);
        let revision_document = revision_overflow_core.document.clone();
        let revision_history = revision_overflow_core.history.clone();
        assert!(matches!(
            revision_overflow_core.begin_document_edit(),
            Err(CoreError::InvalidState("document revision overflow"))
        ));
        assert_eq!(revision_overflow_core.document, revision_document);
        assert_eq!(revision_overflow_core.history.len(), revision_history.len());
    }

    #[test]
    fn commit_after_undo_truncates_the_redo_branch() {
        let mut core = initialized_core();
        core.canonical_invocation_active = false;
        for origin in [1, 2] {
            let mut grid = core.grid().unwrap();
            grid.origin_x = origin;
            core.set_grid(grid).unwrap();
        }
        core.undo().unwrap();
        assert_eq!(core.history_cursor, 1);

        let mut grid = core.grid().unwrap();
        grid.origin_y = 3;
        core.set_grid(grid).unwrap();

        assert_eq!(core.history.len(), 2);
        assert_eq!(core.history_cursor, 2);
        assert!(core.redo().is_err());
    }

    #[test]
    fn primitive_metadata_commits_preserve_history_labels_and_cache_policy() {
        let mut core = initialized_core();
        let _ = core.build_snapshot();
        let cache = core.render_cache.clone();
        core.replace_palette(&[PixelValue::Rgba([1, 2, 3, 255])])
            .unwrap();

        assert_eq!(core.history_entries()[0].label, "Palette edit");
        assert_eq!(core.render_cache, cache);

        core.set_main_line_color(PixelValue::Rgba([4, 5, 6, 255]))
            .unwrap();

        assert_eq!(core.history_entries()[1].label, "Main-line color");
        assert!(core.render_cache.is_empty());
    }

    #[test]
    fn invalid_primitive_metadata_preserves_the_complete_commit_state() {
        let mut core = initialized_core();
        let before = core.document.clone();
        let before_revision = core.document_revision;
        let before_state = core.next_state;

        assert!(matches!(
            core.replace_palette(&[PixelValue::Binary(255)]),
            Err(CoreError::Raster(_))
        ));
        assert_eq!(core.document, before);
        assert_eq!(core.document_revision, before_revision);
        assert_eq!(core.next_state, before_state);
        assert!(core.history.is_empty());
    }
}
