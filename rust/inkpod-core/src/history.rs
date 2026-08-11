//! Undo, redo, and history navigation.

use super::*;
use crate::primitive::CanonicalInvocation;
use crate::selection::mask_bounds;

impl Core {
    /// Reverts the most recently applied history entry.
    ///
    /// Success advances document revision and moves the history cursor by one.
    /// The operation is rejected during a stroke or when no entry is available.
    pub fn undo(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.history_cursor == 0 {
            return Err(CoreError::InvalidState("there is no command to undo"));
        }
        self.ensure_history_cache()?;
        let revision = self.next_document_revision()?;
        let entry = self.history[self.history_cursor - 1].clone();
        let movement = self.prepare_history_move(
            HistoryMoveKind::Undo,
            self.current_state,
            entry.before_state,
        )?;
        let mut document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let invalidate = apply_history_change(
            &mut document,
            entry
                .change
                .as_ref()
                .ok_or(CoreError::InvalidState("history runtime cache is missing"))?,
            false,
            revision,
        )?;
        let editor = self.stage_reconciled_editor_target(&document, None, None)?;
        self.document = Some(document);
        self.history_cursor -= 1;
        self.current_state = entry.before_state;
        self.document_revision = revision;
        if invalidate {
            self.render_cache.clear();
        }
        self.publish_history_move(movement);
        self.publish_editor_session(editor);
        Ok(DispatchOutcome {
            revision: revision.get(),
            accepted_commands: 1,
        })
    }

    /// Reapplies the next history entry.
    ///
    /// Success advances document revision and moves the history cursor by one.
    /// The operation is rejected during a stroke or when no redo entry exists.
    pub fn redo(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        self.ensure_history_cache()?;
        let Some(entry) = self.history.get(self.history_cursor).cloned() else {
            return Err(CoreError::InvalidState("there is no command to redo"));
        };
        let revision = self.next_document_revision()?;
        let movement = self.prepare_history_move(
            HistoryMoveKind::Redo,
            self.current_state,
            entry.after_state,
        )?;
        let mut document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let invalidate = apply_history_change(
            &mut document,
            entry
                .change
                .as_ref()
                .ok_or(CoreError::InvalidState("history runtime cache is missing"))?,
            true,
            revision,
        )?;
        let editor = self.stage_reconciled_editor_target(&document, None, None)?;
        self.document = Some(document);
        self.history_cursor += 1;
        self.current_state = entry.after_state;
        self.document_revision = revision;
        if invalidate {
            self.render_cache.clear();
        }
        self.publish_history_move(movement);
        self.publish_editor_session(editor);
        Ok(DispatchOutcome {
            revision: revision.get(),
            accepted_commands: 1,
        })
    }

    /// Returns owned metadata for every history entry in chronological order.
    #[must_use]
    pub fn history_entries(&self) -> Vec<HistoryEntryInfo> {
        self.history
            .iter()
            .enumerate()
            .map(|(index, entry)| HistoryEntryInfo {
                index,
                applied: index < self.history_cursor,
                label: entry.label,
            })
            .collect()
    }

    /// Returns the cursor separating applied entries from redo entries.
    #[must_use]
    pub const fn history_cursor(&self) -> usize {
        self.history_cursor
    }

    /// Moves directly to a history cursor by applying undo or redo entries.
    ///
    /// The valid range is `0..=history_entries().len()`. The current cursor is a
    /// no-op; a real move advances document revision once and preserves savepoint
    /// identity so dirty state follows the selected history state.
    pub fn jump_history(&mut self, target_cursor: usize) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if target_cursor > self.history.len() {
            return Err(CoreError::InvalidArgument(
                "history target is outside the available range",
            ));
        }
        if target_cursor == self.history_cursor {
            return Ok(self.noop_outcome());
        }
        self.ensure_history_cache()?;
        let revision = self.next_document_revision()?;
        let accepted_commands = self.history_cursor.abs_diff(target_cursor) as u64;
        let destination_state = if target_cursor == 0 {
            StateId::GENESIS
        } else {
            self.history[target_cursor - 1].after_state
        };
        let movement = self.prepare_history_move(
            HistoryMoveKind::Jump,
            self.current_state,
            destination_state,
        )?;
        let mut document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut cursor = self.history_cursor;
        let mut invalidate = false;
        while cursor > target_cursor {
            let entry = &self.history[cursor - 1];
            invalidate |= apply_history_change(
                &mut document,
                entry
                    .change
                    .as_ref()
                    .ok_or(CoreError::InvalidState("history runtime cache is missing"))?,
                false,
                revision,
            )?;
            cursor -= 1;
        }
        while cursor < target_cursor {
            let entry = &self.history[cursor];
            invalidate |= apply_history_change(
                &mut document,
                entry
                    .change
                    .as_ref()
                    .ok_or(CoreError::InvalidState("history runtime cache is missing"))?,
                true,
                revision,
            )?;
            cursor += 1;
        }
        let editor = self.stage_reconciled_editor_target(&document, None, None)?;
        self.document = Some(document);
        self.history_cursor = target_cursor;
        self.current_state = destination_state;
        self.document_revision = revision;
        if invalidate {
            self.render_cache.clear();
        }
        self.publish_history_move(movement);
        self.publish_editor_session(editor);
        Ok(DispatchOutcome {
            revision: revision.get(),
            accepted_commands,
        })
    }

    /// Restores the selected pixels of the active plane from the normal-save file.
    ///
    /// The saved document must match the open document and the selection must be
    /// non-empty. Unchanged pixels are a no-op; a change is one undoable edit.
    /// Read/validation failure leaves live document state untouched.
    pub fn revert_active_plane_selection(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let path = self
            .current_path
            .clone()
            .ok_or(CoreError::InvalidState("document has no normal-save path"))?;
        let (_, active_plane_id) = self.active_editor_target_ids()?;
        let current = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let Some(bounds) = mask_bounds(&current.selection)? else {
            return Err(CoreError::InvalidState("selection is empty"));
        };
        let saved = self.staged_saved_document(&path)?;
        if saved.uuid != current.uuid
            || saved.width != current.width
            || saved.height != current.height
        {
            return Err(CoreError::InvalidState(
                "saved document no longer matches the open document",
            ));
        }
        let current_plane = current
            .plane_by_id(active_plane_id)
            .ok_or(CoreError::InvalidState("active plane no longer exists"))?;
        let saved_plane = saved
            .plane_by_id(active_plane_id)
            .ok_or(CoreError::InvalidState(
                "active plane does not exist in the saved document",
            ))?;
        if current_plane.kind != saved_plane.kind
            || current_plane.raster.format() != saved_plane.raster.format()
        {
            return Err(CoreError::InvalidState(
                "active plane is incompatible with the saved document",
            ));
        }
        let end_x = bounds
            .x
            .checked_add(bounds.width)
            .ok_or(CoreError::InvalidState("selection bounds overflow"))?;
        let end_y = bounds
            .y
            .checked_add(bounds.height)
            .ok_or(CoreError::InvalidState("selection bounds overflow"))?;
        let mut changes = Vec::new();
        for y in bounds.y..end_y {
            for x in bounds.x..end_x {
                let x = u32::try_from(x)
                    .map_err(|_| CoreError::InvalidState("selection X is negative"))?;
                let y = u32::try_from(y)
                    .map_err(|_| CoreError::InvalidState("selection Y is negative"))?;
                if current.selection.pixel(x, y)? == PixelValue::Binary(0) {
                    continue;
                }
                let before = current_plane.raster.pixel(x, y)?;
                let after = saved_plane.raster.pixel(x, y)?;
                if before != after {
                    changes.push(PixelChange {
                        x,
                        y,
                        before,
                        after,
                    });
                }
            }
        }
        if changes.is_empty() {
            return Ok(self.noop_outcome());
        }
        self.execute_canonical_invocation(CanonicalInvocation::RestoreSelectedPixels {
            plane_id: active_plane_id.get(),
            changes,
        })
        .map(|result| result.dispatch)
    }

    pub(crate) fn restore_selected_pixels(
        &mut self,
        plane_id: u64,
        changes: &[PixelChange],
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if changes.is_empty() {
            return Ok(self.noop_outcome());
        }
        let plane_id = PlaneId::from_raw(plane_id);
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        let source = before
            .plane_by_id(plane_id)
            .ok_or(CoreError::InvalidArgument(
                "restore plane ID does not exist",
            ))?;
        let mut coordinates = BTreeSet::new();
        for change in changes {
            if !coordinates.insert((change.x, change.y))
                || source.raster.pixel(change.x, change.y)? != change.before
            {
                return Err(CoreError::InvalidArgument(
                    "restore pixel precondition does not match the document",
                ));
            }
        }
        let raster = &mut after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("restore plane disappeared"))?
            .raster;
        let mut touched = BTreeSet::new();
        for change in changes {
            raster.set_pixel(change.x, change.y, change.after, revision.get())?;
            touched.insert(TileCoord {
                x: change.x / TILE_SIZE,
                y: change.y / TILE_SIZE,
            });
        }
        for coord in touched {
            raster.remove_tile_if_empty(coord);
        }
        edit.commit(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PixelChange {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) before: PixelValue,
    pub(super) after: PixelValue,
}

#[derive(Clone, Debug)]
pub(super) enum HistoryChange {
    Pixels {
        plane_id: PlaneId,
        changes: Vec<PixelChange>,
    },
    Palette {
        before: Palette,
        after: Palette,
    },
    ColorChart {
        before: ColorChart,
        after: ColorChart,
    },
    MainLineColor {
        before: PixelValue,
        after: PixelValue,
    },
    Document {
        before: Box<CellDocument>,
        after: Box<CellDocument>,
    },
}

impl HistoryChange {
    pub(super) const fn label(&self) -> &'static str {
        match self {
            Self::Pixels { .. } => "Raster edit",
            Self::Palette { .. } => "Palette edit",
            Self::ColorChart { .. } => "Color chart edit",
            Self::MainLineColor { .. } => "Main-line color",
            Self::Document { .. } => "Document edit",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct HistoryEntry {
    pub(super) change: Option<HistoryChange>,
    pub(super) label: &'static str,
    pub(super) before_state: StateId,
    pub(super) after_state: StateId,
    pub(super) procedure: Arc<CanonicalProcedure>,
    pub(super) branch_id: BranchId,
}

#[derive(Clone, Debug)]
pub(super) struct StagedHistoryEntry {
    pub(super) change: HistoryChange,
    pub(super) label: &'static str,
    pub(super) before_state: StateId,
    pub(super) after_state: StateId,
}

pub(super) fn apply_history_change(
    document: &mut CellDocument,
    change: &HistoryChange,
    use_after: bool,
    revision: DocumentRevision,
) -> Result<bool, CoreError> {
    let mut invalidate_all = false;
    match change {
        HistoryChange::Pixels { plane_id, changes } => {
            let raster = &mut document
                .plane_by_id_mut(*plane_id)
                .ok_or(CoreError::InvalidState("history plane no longer exists"))?
                .raster;
            let mut touched = BTreeSet::new();
            for change in changes {
                raster.set_pixel(
                    change.x,
                    change.y,
                    if use_after {
                        change.after
                    } else {
                        change.before
                    },
                    revision.get(),
                )?;
                touched.insert(TileCoord {
                    x: change.x / TILE_SIZE,
                    y: change.y / TILE_SIZE,
                });
            }
            for coord in touched {
                raster.remove_tile_if_empty(coord);
            }
        }
        HistoryChange::Palette { before, after } => {
            document.palette = if use_after {
                after.clone()
            } else {
                before.clone()
            };
        }
        HistoryChange::ColorChart { before, after } => {
            document.color_chart = if use_after {
                after.clone()
            } else {
                before.clone()
            };
        }
        HistoryChange::MainLineColor { before, after } => {
            document.main_line_color = if use_after { *after } else { *before };
            invalidate_all = true;
        }
        HistoryChange::Document { before, after } => {
            *document = if use_after {
                (**after).clone()
            } else {
                (**before).clone()
            };
            invalidate_all = true;
        }
    }
    Ok(invalidate_all)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Read-only metadata for one history entry.
pub struct HistoryEntryInfo {
    /// Zero-based position in the history list.
    pub index: usize,
    /// Whether this entry is before the current history cursor.
    pub applied: bool,
    /// Stable user-facing category label for the change.
    pub label: &'static str,
}
