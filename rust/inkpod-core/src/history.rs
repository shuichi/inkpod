//! Undo, redo, and history navigation.

use super::*;
use crate::selection::mask_bounds;

impl Core {
    pub fn undo(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.history_cursor == 0 {
            return Err(CoreError::InvalidState("there is no command to undo"));
        }
        let revision = self.next_document_revision()?;
        let entry = self.history[self.history_cursor - 1].clone();
        self.apply_history_values(&entry, false, revision)?;
        self.history_cursor -= 1;
        self.current_state = entry.before_state;
        self.document_revision = revision;
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn redo(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let Some(entry) = self.history.get(self.history_cursor).cloned() else {
            return Err(CoreError::InvalidState("there is no command to redo"));
        };
        let revision = self.next_document_revision()?;
        self.apply_history_values(&entry, true, revision)?;
        self.history_cursor += 1;
        self.current_state = entry.after_state;
        self.document_revision = revision;
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    #[must_use]
    pub fn history_entries(&self) -> Vec<HistoryEntryInfo> {
        self.history
            .iter()
            .enumerate()
            .map(|(index, entry)| HistoryEntryInfo {
                index,
                applied: index < self.history_cursor,
                label: match &entry.change {
                    HistoryChange::Pixels { .. } => "Raster edit",
                    HistoryChange::Palette { .. } => "Palette edit",
                    HistoryChange::MainLineColor { .. } => "Main-line color",
                    HistoryChange::Document { .. } => "Document edit",
                },
            })
            .collect()
    }

    #[must_use]
    pub const fn history_cursor(&self) -> usize {
        self.history_cursor
    }

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
        let revision = self.next_document_revision()?;
        let accepted_commands = self.history_cursor.abs_diff(target_cursor) as u64;
        while self.history_cursor > target_cursor {
            let entry = self.history[self.history_cursor - 1].clone();
            self.apply_history_values(&entry, false, revision)?;
            self.history_cursor -= 1;
            self.current_state = entry.before_state;
        }
        while self.history_cursor < target_cursor {
            let entry = self.history[self.history_cursor].clone();
            self.apply_history_values(&entry, true, revision)?;
            self.history_cursor += 1;
            self.current_state = entry.after_state;
        }
        self.document_revision = revision;
        Ok(DispatchOutcome {
            revision,
            accepted_commands,
        })
    }

    pub fn revert_active_plane_selection(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let path = self
            .current_path
            .clone()
            .ok_or(CoreError::InvalidState("document has no normal-save path"))?;
        let current = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let Some(bounds) = mask_bounds(&current.selection)? else {
            return Err(CoreError::InvalidState("selection is empty"));
        };
        let active_plane_id = current.active_plane_id;
        let file = inkpod_format::read(&path)?;
        let saved = CellDocument::from_file(file, self.document_revision)?;
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
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let raster = &mut document
            .plane_by_id_mut(active_plane_id)
            .ok_or(CoreError::InvalidState("active plane disappeared"))?
            .raster;
        let mut touched = BTreeSet::new();
        for change in &changes {
            raster.set_pixel(change.x, change.y, change.after, revision)?;
            touched.insert(TileCoord {
                x: change.x / TILE_SIZE,
                y: change.y / TILE_SIZE,
            });
        }
        for coord in touched {
            raster.remove_tile_if_empty(coord);
        }
        self.document_revision = revision;
        self.commit_pixel_history(active_plane_id, changes, after_state);
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct PixelChange {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) before: PixelValue,
    pub(super) after: PixelValue,
}

#[derive(Clone, Debug)]
pub(super) enum HistoryChange {
    Pixels {
        plane_id: u64,
        changes: Vec<PixelChange>,
    },
    Palette {
        before: Palette,
        after: Palette,
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

#[derive(Clone, Debug)]
pub(super) struct HistoryEntry {
    pub(super) change: HistoryChange,
    pub(super) before_state: u64,
    pub(super) after_state: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryEntryInfo {
    pub index: usize,
    pub applied: bool,
    pub label: &'static str,
}
