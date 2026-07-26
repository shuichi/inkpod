//! Core construction and top-level document lifecycle entry points.

use super::*;

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    #[must_use]
    pub fn new() -> Self {
        let shortcuts = default_shortcuts();
        Self {
            document: None,
            document_revision: 0,
            view: ViewState {
                zoom: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
                revision: 0,
                mode: ViewMode::Manual,
                flip_horizontal: false,
                flip_vertical: false,
                ruler_visible: false,
                guides_visible: true,
                grid_visible: false,
                snap_enabled: false,
                guide_snap_enabled: false,
                grid_snap_enabled: false,
                transparent_view: true,
                alpha_view: false,
                viewport_width: 1.0,
                viewport_height: 1.0,
            },
            history: Vec::new(),
            history_cursor: 0,
            current_state: 0,
            next_state: 1,
            savepoint: None,
            next_id: 1,
            current_path: None,
            recovered: false,
            active_stroke: None,
            filter_preview: None,
            last_filter: None,
            render_cache: BTreeMap::new(),
            next_render_tile_revision: 1,
            next_preview_revision: 1_u64 << 63,
            color_check: None,
            secondary_views: BTreeMap::new(),
            next_view_id: 1,
            floating: None,
            shortcut_defaults: shortcuts.clone(),
            shortcuts,
            sequence: None,
            motion_check: None,
            subpalette_index: None,
        }
    }

    #[must_use]
    pub fn dispatch(&mut self, commands: &[Command]) -> DispatchOutcome {
        DispatchOutcome {
            revision: self.document_revision,
            accepted_commands: commands.len() as u64,
        }
    }

    pub fn new_cell(
        &mut self,
        width: u32,
        height: u32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
    ) -> Result<DocumentInfo, CoreError> {
        let uuid = (u128::from(0x494e_4b50_4f44_4d31_u64) << 64) | u128::from(self.next_id);
        self.new_cell_with_uuid(width, height, dpi_x_milli, dpi_y_milli, uuid)
    }

    pub fn new_cell_with_uuid(
        &mut self,
        width: u32,
        height: u32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
        document_uuid: u128,
    ) -> Result<DocumentInfo, CoreError> {
        self.cancel_stroke();
        self.filter_preview = None;
        self.last_filter = None;
        self.render_cache.clear();
        let ids = DocumentIds {
            document: self.allocate_id(),
            layer: self.allocate_id(),
            main_plane: self.allocate_id(),
            color_plane: self.allocate_id(),
            selection_plane: self.allocate_id(),
            light_table_set: self.allocate_id(),
        };
        let document = CellDocument::new(
            ids,
            document_uuid,
            PaperSpec {
                width,
                height,
                dpi_x_milli,
                dpi_y_milli,
            },
        )?;
        self.document = Some(document);
        self.document_revision = self.next_document_revision()?;
        // A new blank cell is the initial in-memory savepoint even though it
        // does not have a normal-save path yet. Pathlessness controls whether
        // Save needs a destination; it must not make an unedited document
        // appear modified.
        self.reset_history(true);
        self.reset_view();
        self.current_path = None;
        self.recovered = false;
        self.color_check = None;
        self.secondary_views.clear();
        self.floating = None;
        self.sequence = None;
        self.motion_check = None;
        self.subpalette_index = None;
        self.document_info()
    }
}

/// Single-writer application core. Document and view revisions are independent.
#[derive(Debug)]
pub struct Core {
    pub(super) document: Option<CellDocument>,
    pub(super) document_revision: u64,
    pub(super) view: ViewState,
    pub(super) history: Vec<HistoryEntry>,
    pub(super) history_cursor: usize,
    pub(super) current_state: u64,
    pub(super) next_state: u64,
    pub(super) savepoint: Option<u64>,
    pub(super) next_id: u64,
    pub(super) current_path: Option<PathBuf>,
    pub(super) recovered: bool,
    pub(super) active_stroke: Option<StrokeSession>,
    pub(super) filter_preview: Option<effects::FilterPreview>,
    pub(super) last_filter: Option<Filter>,
    pub(super) render_cache: BTreeMap<TileCoord, RenderTile>,
    pub(super) next_render_tile_revision: u64,
    pub(super) next_preview_revision: u64,
    pub(super) color_check: Option<ColorCheckMode>,
    pub(super) secondary_views: BTreeMap<u64, ViewState>,
    pub(super) next_view_id: u64,
    pub(super) floating: Option<FloatingSelection>,
    pub(super) shortcut_defaults: BTreeMap<u32, Vec<ShortcutStroke>>,
    pub(super) shortcuts: BTreeMap<u32, Vec<ShortcutStroke>>,
    pub(super) sequence: Option<animation::SequenceState>,
    pub(super) motion_check: Option<animation::MotionCheckState>,
    pub(super) subpalette_index: Option<usize>,
}

impl Core {
    pub(super) fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1).max(1);
        id
    }

    pub(super) const fn noop_outcome(&self) -> DispatchOutcome {
        DispatchOutcome {
            revision: self.document_revision,
            accepted_commands: 1,
        }
    }

    pub(super) fn commit_document_edit(
        &mut self,
        before: CellDocument,
        after: CellDocument,
    ) -> Result<DispatchOutcome, CoreError> {
        let revision = self.next_document_revision()?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub(super) fn commit_document_edit_with_revision(
        &mut self,
        before: CellDocument,
        after: CellDocument,
        revision: u64,
    ) -> Result<DispatchOutcome, CoreError> {
        if before == after {
            return Ok(self.noop_outcome());
        }
        let after_state = self.allocate_state()?;
        self.document = Some(after.clone());
        self.document_revision = revision;
        self.render_cache.clear();
        self.commit_history_change(
            HistoryChange::Document {
                before: Box::new(before),
                after: Box::new(after),
            },
            after_state,
        );
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub(super) fn next_document_revision(&self) -> Result<u64, CoreError> {
        self.document_revision
            .checked_add(1)
            .ok_or(CoreError::InvalidState("document revision overflow"))
    }

    pub(super) fn allocate_preview_revision(&mut self) -> Result<u64, CoreError> {
        let revision = self.next_preview_revision;
        self.next_preview_revision = self
            .next_preview_revision
            .checked_add(1)
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

    pub(super) fn allocate_state(&mut self) -> Result<u64, CoreError> {
        let state = self.next_state;
        self.next_state = self
            .next_state
            .checked_add(1)
            .ok_or(CoreError::InvalidState("history state overflow"))?;
        Ok(state)
    }

    pub(super) fn reset_history(&mut self, saved: bool) {
        self.history.clear();
        self.history_cursor = 0;
        self.current_state = self.next_state;
        self.next_state = self.next_state.saturating_add(1);
        self.savepoint = saved.then_some(self.current_state);
    }

    pub(super) fn reset_view(&mut self) {
        let revision = self.view.revision.saturating_add(1);
        self.view = ViewState {
            revision,
            ..ViewState::default()
        };
    }

    pub(super) fn commit_pixel_history(
        &mut self,
        plane_id: u64,
        changes: Vec<PixelChange>,
        after_state: u64,
    ) {
        self.commit_history_change(HistoryChange::Pixels { plane_id, changes }, after_state);
    }

    pub(super) fn commit_history_change(&mut self, change: HistoryChange, after_state: u64) {
        self.history.truncate(self.history_cursor);
        let before_state = self.current_state;
        self.history.push(HistoryEntry {
            change,
            before_state,
            after_state,
        });
        self.history_cursor = self.history.len();
        self.current_state = after_state;
    }

    pub(super) fn apply_history_values(
        &mut self,
        entry: &HistoryEntry,
        use_after: bool,
        revision: u64,
    ) -> Result<(), CoreError> {
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        match &entry.change {
            HistoryChange::Pixels { plane_id, changes } => {
                document.active_plane_id = *plane_id;
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
                        revision,
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
            HistoryChange::MainLineColor { before, after } => {
                document.main_line_color = if use_after { *after } else { *before };
                self.render_cache.clear();
            }
            HistoryChange::Document { before, after } => {
                *document = if use_after {
                    (**after).clone()
                } else {
                    (**before).clone()
                };
                self.render_cache.clear();
            }
        }
        Ok(())
    }
}
