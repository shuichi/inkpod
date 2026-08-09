//! Atomic Core query, update, frame, and savepoint operations for EditorState.

use super::codec::{
    decode_edit_frame, encode_edit_frame, state_digest, validate_color, validate_diameter,
    validate_state,
};
use super::model::*;
use crate::{CellDocument, Core, CoreError, LayerId, PlaneId};

impl Core {
    /// Returns an owned copy of immutable Rust built-in editor defaults.
    ///
    /// This query is valid before document creation and has no side effects.
    #[must_use]
    pub fn editor_defaults(&self) -> EditorDefaults {
        self.editor_defaults.clone()
    }

    /// Returns an owned editor-state snapshot for the current document session.
    ///
    /// The query has no effect on editor/document revisions, history, dirty state,
    /// savepoints, procedure journal, render content, or render caches.
    pub fn editor_state(&self) -> Result<EditorStateInfo, CoreError> {
        self.editor_session
            .as_ref()
            .map(Self::editor_info_from_session)
            .ok_or(CoreError::NoDocument)
    }

    /// Applies one typed EditorState update atomically at `expected_revision`.
    ///
    /// A semantic no-op preserves editor revision/digest/dirty. Success that
    /// changes meaning advances only EditorRevision and editor dirty state;
    /// document revision, StateId, history, journal, and render content are not
    /// changed. Invalid, stale, or overflowing updates publish nothing.
    pub fn update_editor_state(
        &mut self,
        expected_revision: EditorRevision,
        update: EditorStateUpdate,
    ) -> Result<EditorStateInfo, CoreError> {
        let current = self.editor_session.as_ref().ok_or(CoreError::NoDocument)?;
        if current.revision != expected_revision {
            return Err(CoreError::InvalidState(
                "editor state base revision is stale",
            ));
        }
        let mut next = current.state.clone();
        self.apply_editor_update(&mut next, update)?;
        validate_state(&next)?;
        if next == current.state {
            return Ok(Self::editor_info_from_session(current));
        }
        let revision = current
            .revision
            .checked_next()
            .ok_or(CoreError::InvalidState("editor revision overflow"))?;
        let digest = state_digest(&next);
        let savepoint = current.savepoint;
        self.editor_session = Some(EditorSessionState {
            state: next,
            revision,
            digest,
            savepoint,
        });
        self.editor_state()
    }

    /// Encodes the target canonical EDIT frame without changing any Core state.
    ///
    /// The frame contains EditorRevision, the canonical EditorState frame, and
    /// its domain-separated digest. Normal and recovery native saves write this
    /// exact frame to the required `EDIT` section.
    pub fn editor_state_frame(&self) -> Result<Vec<u8>, CoreError> {
        let session = self.editor_session.as_ref().ok_or(CoreError::NoDocument)?;
        Ok(encode_edit_frame(session))
    }

    /// Atomically restores a canonical EDIT frame into the current session.
    ///
    /// The frame's exact stable target must exist in the current document.
    /// Malformed, unknown-enum, digest-mismatched, or invalid-target input leaves
    /// the previous editor state untouched. `disposition` controls only the
    /// editor savepoint and does not change the decoded revision or digest.
    pub fn restore_editor_state_frame(
        &mut self,
        frame: &[u8],
        disposition: EditorFrameDisposition,
    ) -> Result<EditorStateInfo, CoreError> {
        if self.document.is_none() {
            return Err(CoreError::NoDocument);
        }
        let decoded = decode_edit_frame(frame)?;
        validate_state(&decoded.state)?;
        let target = decoded.state.target.ok_or(CoreError::InvalidArgument(
            "a document editor frame requires an active target",
        ))?;
        self.validate_editor_target(target)?;
        let normalized = self.normalize_edit_targets(&decoded.state.edit_targets)?;
        if normalized != decoded.state.edit_targets {
            return Err(CoreError::InvalidArgument(
                "editor edit targets are not in canonical document-tree order",
            ));
        }
        let savepoint = match disposition {
            EditorFrameDisposition::Saved => Some(decoded.digest),
            EditorFrameDisposition::Unsaved => None,
        };
        self.editor_session = Some(EditorSessionState {
            state: decoded.state,
            revision: decoded.revision,
            digest: decoded.digest,
            savepoint,
        });
        self.editor_state()
    }

    /// Returns a token for committing the current canonical editor state as saved.
    ///
    /// Producing a token has no side effects. A caller must commit it only after
    /// the corresponding EDIT frame has been durably stored by the target format.
    pub fn editor_savepoint_token(&self) -> Result<EditorSavepointToken, CoreError> {
        let session = self.editor_session.as_ref().ok_or(CoreError::NoDocument)?;
        Ok(EditorSavepointToken {
            revision: session.revision,
            digest: session.digest,
        })
    }

    /// Commits a previously queried exact editor state as the editor savepoint.
    ///
    /// A stale token fails atomically. Success changes only editor dirty state;
    /// revision, digest, document history, journal, and render content are stable.
    pub fn commit_editor_savepoint(
        &mut self,
        token: EditorSavepointToken,
    ) -> Result<EditorStateInfo, CoreError> {
        let session = self.editor_session.as_mut().ok_or(CoreError::NoDocument)?;
        if token.revision != session.revision || token.digest != session.digest {
            return Err(CoreError::InvalidState("editor savepoint token is stale"));
        }
        session.savepoint = Some(session.digest);
        self.editor_state()
    }

    pub(crate) fn reset_editor_state(&mut self, saved: bool) {
        let target = self.document.as_ref().and_then(Self::first_editor_target);
        let mut state = self.editor_defaults.state.clone();
        state.target = target;
        state.edit_targets.clear();
        let digest = state_digest(&state);
        self.editor_session = Some(EditorSessionState {
            state,
            revision: EditorRevision::INITIAL,
            digest,
            savepoint: saved.then_some(digest),
        });
    }

    pub(crate) fn editor_dirty(&self) -> bool {
        self.editor_session
            .as_ref()
            .is_some_and(|session| session.savepoint != Some(session.digest))
    }

    pub(crate) fn active_editor_target_ids(&self) -> Result<(LayerId, PlaneId), CoreError> {
        let target = self.active_editor_target()?;
        self.editor_target_ids(target)
    }

    pub(crate) fn active_editor_target(&self) -> Result<EditorTarget, CoreError> {
        self.editor_session
            .as_ref()
            .and_then(|session| session.state.target)
            .ok_or(CoreError::InvalidState("editor state has no active target"))
    }

    pub(crate) fn editor_target_ids(
        &self,
        target: EditorTarget,
    ) -> Result<(LayerId, PlaneId), CoreError> {
        self.validate_editor_target(target)?;
        Ok((
            LayerId::from_raw(target.layer_id),
            PlaneId::from_raw(target.plane_id),
        ))
    }

    /// Stages deterministic active-target reconciliation against a prospective document.
    ///
    /// Revision overflow and canonical digest work complete before the caller
    /// publishes either the document topology or this returned editor session.
    pub(crate) fn stage_reconciled_editor_target(
        &self,
        document: &CellDocument,
        preferred: Option<EditorTarget>,
        preferred_edit_targets: Option<&[EditTarget]>,
    ) -> Result<Option<EditorSessionState>, CoreError> {
        let resolved = preferred
            .filter(|target| Self::document_has_editor_target(document, *target))
            .or_else(|| {
                self.editor_session
                    .as_ref()
                    .and_then(|session| session.state.target)
                    .filter(|target| Self::document_has_editor_target(document, *target))
            })
            .or_else(|| Self::first_editor_target(document));
        let Some(session) = self.editor_session.as_ref() else {
            return Ok(None);
        };
        let source_targets = preferred_edit_targets.unwrap_or(&session.state.edit_targets);
        let edit_targets = Self::normalize_edit_targets_in(document, source_targets, false)?;
        if session.state.target == resolved && session.state.edit_targets == edit_targets {
            return Ok(None);
        }
        let revision = session
            .revision
            .checked_next()
            .ok_or(CoreError::InvalidState("editor revision overflow"))?;
        let mut state = session.state.clone();
        state.target = resolved;
        state.edit_targets = edit_targets;
        validate_state(&state)?;
        let digest = state_digest(&state);
        let savepoint = session.savepoint;
        Ok(Some(EditorSessionState {
            state,
            revision,
            digest,
            savepoint,
        }))
    }

    pub(crate) fn publish_editor_session(&mut self, session: Option<EditorSessionState>) {
        if let Some(session) = session {
            self.editor_session = Some(session);
        }
    }

    fn apply_editor_update(
        &self,
        state: &mut EditorState,
        update: EditorStateUpdate,
    ) -> Result<(), CoreError> {
        match update {
            EditorStateUpdate::SetActiveTool(tool) => {
                state.active_tool = tool;
                if tool.consumes_color() {
                    state.last_color_consuming_tool = Some(tool);
                }
            }
            EditorStateUpdate::SetToolColor { tool, color } => {
                validate_color(color)?;
                if !tool.consumes_color() {
                    return Err(CoreError::InvalidArgument(
                        "the selected editor tool does not consume color",
                    ));
                }
                state
                    .tool_styles
                    .get_mut(&tool)
                    .ok_or(CoreError::InvalidArgument("unknown editor tool"))?
                    .color = Some(color);
            }
            EditorStateUpdate::SetToolDiameter { tool, diameter_q16 } => {
                validate_diameter(diameter_q16)?;
                state
                    .tool_styles
                    .get_mut(&tool)
                    .ok_or(CoreError::InvalidArgument("unknown editor tool"))?
                    .diameter_q16 = diameter_q16;
            }
            EditorStateUpdate::SetFillOptions(options) => state.fill = options,
            EditorStateUpdate::SetSelectionOptions(options) => state.selection = options,
            EditorStateUpdate::SetVectorOptions(options) => state.vector = options,
            EditorStateUpdate::SetActiveTarget(target) => {
                self.validate_editor_target(target)?;
                state.target = Some(target);
            }
            EditorStateUpdate::SetEditTargets(targets) => {
                state.edit_targets = self.normalize_edit_targets(&targets)?;
            }
            EditorStateUpdate::SetPaletteCursor(cursor) => state.palette_cursor = cursor,
        }
        Ok(())
    }

    pub(crate) fn validate_editor_target(&self, target: EditorTarget) -> Result<(), CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if Self::document_has_editor_target(document, target) {
            Ok(())
        } else {
            Err(CoreError::InvalidArgument(
                "editor target layer/plane pair does not exist",
            ))
        }
    }

    fn document_has_editor_target(document: &CellDocument, target: EditorTarget) -> bool {
        document.layers.iter().any(|layer| {
            layer.id.get() == target.layer_id
                && layer
                    .planes
                    .iter()
                    .any(|plane| plane.id.get() == target.plane_id)
        })
    }

    pub(crate) fn effective_edit_targets(&self) -> Result<Vec<EditTarget>, CoreError> {
        let state = &self
            .editor_session
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .state;
        if state.edit_targets.is_empty() {
            Ok(state.target.map(EditTarget::Plane).into_iter().collect())
        } else {
            Ok(state.edit_targets.clone())
        }
    }

    fn normalize_edit_targets(&self, targets: &[EditTarget]) -> Result<Vec<EditTarget>, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        Self::normalize_edit_targets_in(document, targets, true)
    }

    pub(crate) fn normalize_edit_targets_in(
        document: &CellDocument,
        targets: &[EditTarget],
        reject_missing: bool,
    ) -> Result<Vec<EditTarget>, CoreError> {
        if targets.len() > MAX_EDIT_TARGETS {
            return Err(CoreError::InvalidArgument(
                "editor edit-target count exceeds the supported maximum",
            ));
        }
        let requested = targets
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut normalized = Vec::with_capacity(requested.len());
        let mut matched = std::collections::BTreeSet::new();
        for layer in &document.layers {
            let layer_target = EditTarget::Layer(layer.id.get());
            if requested.contains(&layer_target) {
                normalized.push(layer_target);
                matched.insert(layer_target);
                for plane in &layer.planes {
                    let child = EditTarget::Plane(EditorTarget {
                        layer_id: layer.id.get(),
                        plane_id: plane.id.get(),
                    });
                    if requested.contains(&child) {
                        matched.insert(child);
                    }
                }
                continue;
            }
            for plane in &layer.planes {
                let target = EditTarget::Plane(EditorTarget {
                    layer_id: layer.id.get(),
                    plane_id: plane.id.get(),
                });
                if requested.contains(&target) {
                    normalized.push(target);
                    matched.insert(target);
                }
            }
        }
        if reject_missing && matched.len() != requested.len() {
            return Err(CoreError::InvalidArgument(
                "editor edit target does not exist in the document",
            ));
        }
        Ok(normalized)
    }

    fn first_editor_target(document: &CellDocument) -> Option<EditorTarget> {
        document.layers.iter().find_map(|layer| {
            layer.planes.first().map(|plane| EditorTarget {
                layer_id: layer.id.get(),
                plane_id: plane.id.get(),
            })
        })
    }

    fn editor_info_from_session(session: &EditorSessionState) -> EditorStateInfo {
        EditorStateInfo {
            revision: session.revision,
            digest: session.digest,
            dirty: session.savepoint != Some(session.digest),
            state: session.state.clone(),
        }
    }
}
