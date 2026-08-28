//! Prospective sequence activation and its shared publication primitive.

use super::*;

/// The exact effect of selecting one natural-order sequence entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SequenceActivationKind {
    /// The entry is already active; no state or authority changes.
    NoOp,
    /// An unbound entry matches the current immutable Genesis base asset.
    /// Only its UUID/active sequence binding changes; edits and paths survive.
    Bind,
    /// A different immutable source replaces the editable document.
    Replace,
}

/// Side-effect-free resolution of one explicit sequence selection.
///
/// Indices are zero-based in the captured natural-order sequence. Source index
/// and generation are both absent when the current document is not yet bound.
/// The target identity describes the entry before binding, so a `Bind` keeps the
/// source document UUID instead of adopting the target's original UUID. Commit
/// revalidates every field; changing the document/editor state, sequence, or
/// immutable source identities invalidates the plan without publishing anything.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SequenceActivationPlan {
    /// Whether selection is a no-op, an initial binding, or a replacement.
    pub kind: SequenceActivationKind,
    /// Revision of the complete captured sequence catalog.
    pub sequence_revision: u64,
    /// UUID of the current editable document, including when unbound.
    pub source_document_uuid: u128,
    /// Revision of the current editable document.
    pub source_document_revision: u64,
    /// Revision of the current independent editor state.
    pub source_editor_revision: u64,
    /// Current sequence index, or `None` for an unbound document.
    pub source_index: Option<u32>,
    /// Current immutable sequence generation, absent exactly when unbound.
    pub source_generation: Option<u64>,
    /// Requested natural-order sequence index.
    pub target_index: u32,
    /// Target source UUID before an initial binding rewrites it.
    pub target_document_uuid: u128,
    /// Exact immutable target source generation.
    pub target_source_generation: u64,
}

impl Core {
    /// Classifies an explicit sequence selection without mutating any state.
    ///
    /// Dirty documents may be queried. Only an unbound sequence checks whether
    /// the target's canonical raster, DPI, and frames match the immutable Genesis
    /// asset. Already-bound entries use their exact index/identity, not pixel
    /// equality. Invalid index, missing document/catalog, and active preview or
    /// file installation return an error. No file I/O, IDs, revisions, history,
    /// savepoints, paths, or sequence bindings are changed.
    pub fn resolve_sequence_activation(
        &self,
        target: usize,
    ) -> Result<SequenceActivationPlan, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let target_source = sequence
            .cells
            .get(target)
            .ok_or(CoreError::InvalidArgument(
                "sequence target index is outside bounds",
            ))?;
        let source = sequence
            .active_index
            .map(|index| {
                sequence
                    .cells
                    .get(index)
                    .filter(|source| source.document_uuid == document.uuid)
                    .ok_or(CoreError::InvalidState(
                        "active document identity does not match the sequence source",
                    ))
            })
            .transpose()?;
        let kind = if sequence.active_index == Some(target) {
            SequenceActivationKind::NoOp
        } else if source.is_none() && self.sequence_source_matches_current_asset(target_source)? {
            SequenceActivationKind::Bind
        } else {
            SequenceActivationKind::Replace
        };
        Ok(SequenceActivationPlan {
            kind,
            sequence_revision: sequence.revision,
            source_document_uuid: document.uuid,
            source_document_revision: self.document_revision.get(),
            source_editor_revision: self
                .editor_session
                .as_ref()
                .ok_or(CoreError::NoDocument)?
                .revision
                .get(),
            source_index: sequence
                .active_index
                .map(u32::try_from)
                .transpose()
                .map_err(|_| CoreError::InvalidState("sequence source index overflows"))?,
            source_generation: source.map(|source| source.source_generation),
            target_index: u32::try_from(target)
                .map_err(|_| CoreError::InvalidState("sequence target index overflows"))?,
            target_document_uuid: target_source.document_uuid,
            target_source_generation: target_source.source_generation,
        })
    }

    /// Commits a still-current explicit selection after frontend confirmation.
    ///
    /// No-op and initial binding retain dirty state, normal path authority,
    /// history, editor state, and all document/view revisions. Replacement
    /// requires a clean document savepoint and resets the document/history,
    /// and adopts reconciled editor choices as the new cell's clean initial
    /// state. Same-size views retain zoom/pan/flip; different sizes use each
    /// view's existing automatic or manual resize policy before publication.
    /// The previous native/raster save authority is revoked. Stale,
    /// invalid, unsaved, or failed activation changes nothing. This method
    /// performs no filesystem I/O.
    pub fn commit_sequence_activation(
        &mut self,
        plan: SequenceActivationPlan,
    ) -> Result<DocumentInfo, CoreError> {
        let current = self.resolve_sequence_activation(plan.target_index as usize)?;
        if current != plan {
            return Err(CoreError::InvalidState("sequence activation plan is stale"));
        }
        self.activate_normal_sequence_plan(current)
    }

    /// Activates a sequence cell by zero-based natural-order index.
    ///
    /// The current entry and an initial binding preserve dirty edits and file
    /// authority. Replacing the document requires a clean document savepoint;
    /// editor-only dirty does not block it because inherited editor choices
    /// become the new cell's clean initial state. No normal-save path is adopted.
    /// Failure publishes no partial state.
    pub fn sequence_activate(&mut self, target: usize) -> Result<DocumentInfo, CoreError> {
        let plan = self.resolve_sequence_activation(target)?;
        self.activate_normal_sequence_plan(plan)
    }

    fn activate_normal_sequence_plan(
        &mut self,
        plan: SequenceActivationPlan,
    ) -> Result<DocumentInfo, CoreError> {
        if plan.kind == SequenceActivationKind::Replace
            && self.savepoint != Some(self.current_state)
        {
            return Err(CoreError::UnsavedChanges);
        }
        self.publish_sequence_activation(plan)
    }

    /// Autosave navigation has already protected dirty source state externally.
    pub(super) fn sequence_activate_impl(
        &mut self,
        target: usize,
    ) -> Result<DocumentInfo, CoreError> {
        let plan = self.resolve_sequence_activation(target)?;
        self.publish_sequence_activation(plan)
    }

    fn publish_sequence_activation(
        &mut self,
        plan: SequenceActivationPlan,
    ) -> Result<DocumentInfo, CoreError> {
        let target = plan.target_index as usize;
        if plan.kind == SequenceActivationKind::NoOp {
            return self.document_info();
        }
        if plan.kind == SequenceActivationKind::Bind {
            let sequence = self
                .sequence
                .as_mut()
                .ok_or(CoreError::InvalidState("sequence disappeared"))?;
            sequence.cells[target].document_uuid = plan.source_document_uuid;
            sequence.active_index = Some(target);
            return self.document_info();
        }
        let source = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("sequence disappeared"))?
            .cells[target]
            .clone();
        let revision = self.next_document_revision()?;
        let mut next_id = self.next_id;
        let document = Self::document_from_sequence_source(&source, revision, &mut next_id)?;
        let (next_view, next_secondary_views) =
            self.stage_sequence_views(DocumentSizeU32::new(document.width, document.height))?;
        let mut editor = self
            .stage_reconciled_editor_target(&document, None, None)?
            .or_else(|| self.editor_session.clone())
            .ok_or(CoreError::NoDocument)?;
        // Rebinding inherited choices to a fresh cell's stable IDs is loading,
        // not an editor change. Establish only this staged cell's initial
        // baseline; do not mark the outgoing cell saved or grant file authority.
        editor.savepoint = Some(editor.digest);
        let next_file_authority = self.persistence_state.next()?;
        self.sequence
            .as_mut()
            .ok_or(CoreError::InvalidState("sequence disappeared"))?
            .active_index = Some(target);
        self.document = Some(document);
        self.document_revision = revision;
        self.raster_file_format = source.raster_file_format;
        self.io_pair_authority = None;
        self.persistence_state = next_file_authority;
        self.next_id = next_id;
        self.assets = asset::AssetStore::default();
        self.render_cache.clear();
        self.reset_history(true);
        self.view = next_view;
        self.secondary_views = next_secondary_views;
        self.current_path = None;
        self.recovered = false;
        self.floating = None;
        self.publish_editor_session(Some(editor));
        self.register_pristine_sequence_source(&source);
        self.document_info()
    }

    pub(crate) fn stage_sequence_views(
        &self,
        document_size: DocumentSizeU32,
    ) -> Result<(ViewState, BTreeMap<ViewId, ViewState>), CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if document_size == DocumentSizeU32::new(document.width, document.height) {
            return Ok((self.view, self.secondary_views.clone()));
        }
        let resize = |state: ViewState| {
            view::apply_view_state(
                state,
                ViewCommand::ViewportResized {
                    viewport_width: state.viewport.width,
                    viewport_height: state.viewport.height,
                },
                document_size,
            )
            .map(|(state, _)| state)
        };
        let primary = resize(self.view)?;
        let secondary = self
            .secondary_views
            .iter()
            .map(|(id, state)| resize(*state).map(|state| (*id, state)))
            .collect::<Result<_, _>>()?;
        Ok((primary, secondary))
    }

    fn sequence_source_matches_current_asset(
        &self,
        source: &SequenceCellSource,
    ) -> Result<bool, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let BaseSurface::Asset(base_id) = document.base_surface else {
            return Ok(false);
        };
        if document.dpi_x_milli != source.dpi_x_milli
            || document.dpi_y_milli != source.dpi_y_milli
            || document.frames != source.frames
        {
            return Ok(false);
        }
        let mut candidate = asset::AssetStore::default();
        Ok(candidate.ingest_tile_raster(&source.raster, None)?.id() == base_id)
    }
}
