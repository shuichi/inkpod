//! View, guide, grid, locator, and shortcut operations.

use super::*;
use crate::selection::mask_bounds;

impl Core {
    pub fn guides(&self) -> Result<&[Guide], CoreError> {
        Ok(&self.document.as_ref().ok_or(CoreError::NoDocument)?.guides)
    }

    pub fn add_guide(
        &mut self,
        axis: GuideAxis,
        position: i32,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.guides.len() >= MAX_GUIDES {
            return Err(CoreError::InvalidState("guide limit reached"));
        }
        validate_guide_position(&before, axis, position)?;
        let id = self.allocate_id();
        let mut after = before.clone();
        after.guides.push(Guide { id, axis, position });
        after
            .guides
            .sort_by_key(|guide| (guide.axis as u8, guide.position, guide.id));
        let outcome = self.commit_document_edit(before, after)?;
        Ok((outcome, id))
    }

    pub fn move_guide(
        &mut self,
        guide_id: u64,
        position: i32,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let guide = before
            .guides
            .iter()
            .find(|guide| guide.id == guide_id)
            .ok_or(CoreError::InvalidArgument("guide ID does not exist"))?;
        validate_guide_position(&before, guide.axis, position)?;
        if guide.position == position {
            return Ok(self.noop_outcome());
        }
        let mut after = before.clone();
        after
            .guides
            .iter_mut()
            .find(|guide| guide.id == guide_id)
            .expect("guide existence checked")
            .position = position;
        after
            .guides
            .sort_by_key(|guide| (guide.axis as u8, guide.position, guide.id));
        self.commit_document_edit(before, after)
    }

    pub fn delete_guide(&mut self, guide_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let index = before
            .guides
            .iter()
            .position(|guide| guide.id == guide_id)
            .ok_or(CoreError::InvalidArgument("guide ID does not exist"))?;
        let mut after = before.clone();
        after.guides.remove(index);
        self.commit_document_edit(before, after)
    }

    pub fn grid(&self) -> Result<GridConfig, CoreError> {
        Ok(self.document.as_ref().ok_or(CoreError::NoDocument)?.grid)
    }

    pub fn set_grid(&mut self, grid: GridConfig) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_grid(grid)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.grid == grid {
            return Ok(self.noop_outcome());
        }
        let mut after = before.clone();
        after.grid = grid;
        self.commit_document_edit(before, after)
    }

    pub fn snap_document_point(&self, x: f64, y: f64) -> Result<(f64, f64), CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if !x.is_finite() || !y.is_finite() {
            return Err(CoreError::InvalidArgument("snap point is not finite"));
        }
        if !self.view.snap_enabled {
            return Ok((x, y));
        }
        let grid = document.grid;
        let snap_axis = |value: f64, origin: i32, spacing: u32| {
            let step = f64::from(spacing) / f64::from(grid.subdivisions);
            f64::from(origin) + ((value - f64::from(origin)) / step).round() * step
        };
        let mut snapped = if self.view.grid_snap_enabled {
            (
                snap_axis(x, grid.origin_x, grid.spacing_x),
                snap_axis(y, grid.origin_y, grid.spacing_y),
            )
        } else {
            (x, y)
        };
        if self.view.guide_snap_enabled {
            for guide in &document.guides {
                match guide.axis {
                    GuideAxis::Vertical if (x - f64::from(guide.position)).abs() <= 4.0 => {
                        snapped.0 = f64::from(guide.position);
                    }
                    GuideAxis::Horizontal if (y - f64::from(guide.position)).abs() <= 4.0 => {
                        snapped.1 = f64::from(guide.position);
                    }
                    _ => {}
                }
            }
        }
        Ok(snapped)
    }

    pub fn create_view(&mut self) -> Result<u64, CoreError> {
        if self.document.is_none() {
            return Err(CoreError::NoDocument);
        }
        let id = self.next_view_id;
        self.next_view_id = self
            .next_view_id
            .checked_add(1)
            .ok_or(CoreError::InvalidState("view ID overflow"))?;
        self.secondary_views.insert(id, self.view);
        Ok(id)
    }

    pub fn close_view(&mut self, view_id: u64) -> Result<(), CoreError> {
        self.secondary_views
            .remove(&view_id)
            .map(|_| ())
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))
    }

    pub fn apply_view_for(
        &mut self,
        view_id: u64,
        command: ViewCommand,
    ) -> Result<ViewState, CoreError> {
        let original = self.view;
        self.view = *self
            .secondary_views
            .get(&view_id)
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))?;
        let result = self.apply_view(command);
        let updated = self.view;
        self.view = original;
        if result.is_ok() {
            self.secondary_views.insert(view_id, updated);
        }
        result.map(|_| updated)
    }

    pub fn build_snapshot_for(&mut self, view_id: u64) -> Result<RenderSnapshot, CoreError> {
        let selected = *self
            .secondary_views
            .get(&view_id)
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))?;
        let original = self.view;
        self.view = selected;
        let snapshot = self.build_snapshot();
        self.view = original;
        Ok(snapshot)
    }

    pub fn locator_sample(
        &self,
        view_id: Option<u64>,
        device_x: f64,
        device_y: f64,
    ) -> Result<LocatorSample, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let view = match view_id {
            Some(id) => *self
                .secondary_views
                .get(&id)
                .ok_or(CoreError::InvalidArgument("view ID does not exist"))?,
            None => self.view,
        };
        let (x, y) = device_to_document(view, document.width, document.height, device_x, device_y)?;
        let document_x = x.floor() as i32;
        let document_y = y.floor() as i32;
        let color = if document_x >= 0
            && document_y >= 0
            && document_x < document.width as i32
            && document_y < document.height as i32
        {
            self.eyedropper(
                EyedropperSource::Composite,
                document_x as u32,
                document_y as u32,
            )
            .ok()
        } else {
            None
        };
        Ok(LocatorSample {
            document_x,
            document_y,
            selection_bounds: mask_bounds(&document.selection)?,
            color,
        })
    }

    pub fn shortcut_bindings(&self) -> Vec<ShortcutBinding> {
        self.shortcuts
            .iter()
            .filter_map(|(command_id, strokes)| {
                (strokes.len() == 1).then_some(ShortcutBinding {
                    command_id: *command_id,
                    virtual_key: strokes[0].virtual_key,
                    modifiers: strokes[0].modifiers,
                })
            })
            .collect()
    }

    pub fn rebind_shortcut(&mut self, binding: ShortcutBinding) -> Result<(), CoreError> {
        self.rebind_shortcut_sequence(ShortcutSequenceBinding {
            command_id: binding.command_id,
            strokes: vec![ShortcutStroke {
                virtual_key: binding.virtual_key,
                modifiers: binding.modifiers,
            }],
        })
    }

    pub fn resolve_shortcut(
        &self,
        virtual_key: u32,
        modifiers: u32,
    ) -> Result<Option<u32>, CoreError> {
        match self.resolve_shortcut_sequence(&[ShortcutStroke {
            virtual_key,
            modifiers,
        }])? {
            ShortcutSequenceMatch::Exact(command_id) => Ok(Some(command_id)),
            ShortcutSequenceMatch::None | ShortcutSequenceMatch::Prefix => Ok(None),
        }
    }

    pub fn reset_shortcuts(&mut self) {
        self.shortcuts.clone_from(&self.shortcut_defaults);
    }

    pub fn shortcut_sequences(&self) -> Vec<ShortcutSequenceBinding> {
        self.shortcuts
            .iter()
            .map(|(command_id, strokes)| ShortcutSequenceBinding {
                command_id: *command_id,
                strokes: strokes.clone(),
            })
            .collect()
    }

    pub fn set_shortcut_defaults(
        &mut self,
        bindings: &[ShortcutSequenceBinding],
    ) -> Result<(), CoreError> {
        let replacement = validate_shortcut_sequences(bindings)?;
        self.shortcut_defaults = replacement.clone();
        self.shortcuts = replacement;
        Ok(())
    }

    pub fn replace_shortcut_sequences(
        &mut self,
        bindings: &[ShortcutSequenceBinding],
    ) -> Result<(), CoreError> {
        self.shortcuts = validate_shortcut_sequences(bindings)?;
        Ok(())
    }

    pub fn rebind_shortcut_sequence(
        &mut self,
        binding: ShortcutSequenceBinding,
    ) -> Result<(), CoreError> {
        validate_shortcut_sequence(&binding)?;
        if self.shortcuts.len() >= MAX_SHORTCUTS
            && !self.shortcuts.contains_key(&binding.command_id)
        {
            return Err(CoreError::InvalidState("shortcut limit reached"));
        }
        self.shortcuts.retain(|command, candidate| {
            *command == binding.command_id
                || !shortcut_sequences_conflict(candidate, &binding.strokes)
        });
        self.shortcuts.insert(binding.command_id, binding.strokes);
        Ok(())
    }

    pub fn resolve_shortcut_sequence(
        &self,
        strokes: &[ShortcutStroke],
    ) -> Result<ShortcutSequenceMatch, CoreError> {
        validate_shortcut_strokes(strokes)?;
        if let Some(command_id) = self.shortcuts.iter().find_map(|(command_id, candidate)| {
            (candidate.as_slice() == strokes).then_some(*command_id)
        }) {
            return Ok(ShortcutSequenceMatch::Exact(command_id));
        }
        if self
            .shortcuts
            .values()
            .any(|candidate| candidate.starts_with(strokes))
        {
            Ok(ShortcutSequenceMatch::Prefix)
        } else {
            Ok(ShortcutSequenceMatch::None)
        }
    }
}

fn validate_shortcut_sequences(
    bindings: &[ShortcutSequenceBinding],
) -> Result<BTreeMap<u32, Vec<ShortcutStroke>>, CoreError> {
    if bindings.len() > MAX_SHORTCUTS {
        return Err(CoreError::InvalidArgument("too many shortcut bindings"));
    }
    let mut replacement = BTreeMap::new();
    for binding in bindings {
        validate_shortcut_sequence(binding)?;
        if replacement
            .insert(binding.command_id, binding.strokes.clone())
            .is_some()
        {
            return Err(CoreError::InvalidArgument("duplicate shortcut command"));
        }
    }
    let sequences = replacement.values().collect::<Vec<_>>();
    for (index, sequence) in sequences.iter().enumerate() {
        if sequences[index + 1..]
            .iter()
            .any(|candidate| shortcut_sequences_conflict(sequence, candidate))
        {
            return Err(CoreError::InvalidArgument("shortcut sequences conflict"));
        }
    }
    Ok(replacement)
}

fn validate_shortcut_sequence(binding: &ShortcutSequenceBinding) -> Result<(), CoreError> {
    if binding.command_id == 0 {
        return Err(CoreError::InvalidArgument("shortcut command is invalid"));
    }
    validate_shortcut_strokes(&binding.strokes)
}

fn validate_shortcut_strokes(strokes: &[ShortcutStroke]) -> Result<(), CoreError> {
    if strokes.is_empty() || strokes.len() > MAX_SHORTCUT_STROKES {
        return Err(CoreError::InvalidArgument(
            "shortcut stroke count is invalid",
        ));
    }
    if strokes
        .iter()
        .any(|stroke| stroke.virtual_key == 0 || stroke.modifiers & !SHORTCUT_MODIFIER_MASK != 0)
    {
        return Err(CoreError::InvalidArgument("shortcut stroke is invalid"));
    }
    Ok(())
}

fn shortcut_sequences_conflict(left: &[ShortcutStroke], right: &[ShortcutStroke]) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

impl Core {
    pub fn apply_view(&mut self, command: ViewCommand) -> Result<ViewState, CoreError> {
        if self.active_stroke.is_some() {
            return Err(CoreError::InvalidState(
                "view cannot change during an active stroke transaction",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        match command {
            ViewCommand::Fit {
                viewport_width,
                viewport_height,
            }
            | ViewCommand::OneToOne {
                viewport_width,
                viewport_height,
            }
            | ViewCommand::ViewportResized {
                viewport_width,
                viewport_height,
            } if valid_viewport(viewport_width, viewport_height) => {
                self.view.viewport_width = viewport_width;
                self.view.viewport_height = viewport_height;
            }
            _ => {}
        }
        let toggle_changed = match command {
            ViewCommand::Flip { axis } => {
                match axis {
                    MirrorAxis::Horizontal => {
                        self.view.flip_horizontal = !self.view.flip_horizontal
                    }
                    MirrorAxis::Vertical => self.view.flip_vertical = !self.view.flip_vertical,
                }
                true
            }
            ViewCommand::SetRulerVisible(value) => {
                let changed = self.view.ruler_visible != value;
                self.view.ruler_visible = value;
                changed
            }
            ViewCommand::SetGuidesVisible(value) => {
                let changed = self.view.guides_visible != value;
                self.view.guides_visible = value;
                changed
            }
            ViewCommand::SetGridVisible(value) => {
                let changed = self.view.grid_visible != value;
                self.view.grid_visible = value;
                changed
            }
            ViewCommand::SetSnapEnabled(value) => {
                let changed = self.view.snap_enabled != value;
                self.view.snap_enabled = value;
                self.view.guide_snap_enabled = value;
                self.view.grid_snap_enabled = value;
                changed
            }
            ViewCommand::SetGuideSnapEnabled(value) => {
                let changed = self.view.guide_snap_enabled != value;
                self.view.guide_snap_enabled = value;
                self.view.snap_enabled = value || self.view.grid_snap_enabled;
                changed
            }
            ViewCommand::SetGridSnapEnabled(value) => {
                let changed = self.view.grid_snap_enabled != value;
                self.view.grid_snap_enabled = value;
                self.view.snap_enabled = value || self.view.guide_snap_enabled;
                changed
            }
            ViewCommand::SetTransparentView(value) => {
                let changed = self.view.transparent_view != value;
                self.view.transparent_view = value;
                changed
            }
            ViewCommand::SetAlphaView(value) => {
                let changed = self.view.alpha_view != value;
                self.view.alpha_view = value;
                if changed {
                    self.render_cache.clear();
                }
                changed
            }
            _ => false,
        };
        if matches!(
            command,
            ViewCommand::Flip { .. }
                | ViewCommand::SetRulerVisible(_)
                | ViewCommand::SetGuidesVisible(_)
                | ViewCommand::SetGridVisible(_)
                | ViewCommand::SetSnapEnabled(_)
                | ViewCommand::SetGuideSnapEnabled(_)
                | ViewCommand::SetGridSnapEnabled(_)
                | ViewCommand::SetTransparentView(_)
                | ViewCommand::SetAlphaView(_)
        ) {
            if toggle_changed {
                self.view.revision = self
                    .view
                    .revision
                    .checked_add(1)
                    .ok_or(CoreError::InvalidState("view revision overflow"))?;
            }
            return Ok(self.view);
        }
        let (next_zoom, next_pan_x, next_pan_y, next_mode) = match command {
            ViewCommand::PanBy {
                device_dx,
                device_dy,
            } if device_dx.is_finite() && device_dy.is_finite() => (
                self.view.zoom,
                self.view.pan_x + device_dx,
                self.view.pan_y + device_dy,
                ViewMode::Manual,
            ),
            ViewCommand::ZoomAt {
                factor,
                device_x,
                device_y,
            } if factor.is_finite()
                && factor > 0.0
                && device_x.is_finite()
                && device_y.is_finite() =>
            {
                let zoom = (self.view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                let ratio = zoom / self.view.zoom;
                (
                    zoom,
                    device_x - (device_x - self.view.pan_x) * ratio,
                    device_y - (device_y - self.view.pan_y) * ratio,
                    ViewMode::Manual,
                )
            }
            ViewCommand::Fit {
                viewport_width,
                viewport_height,
            } if valid_viewport(viewport_width, viewport_height) => {
                let zoom = (viewport_width / f64::from(document.width))
                    .min(viewport_height / f64::from(document.height))
                    .mul_add(0.95, 0.0)
                    .clamp(MIN_ZOOM, MAX_ZOOM);
                (
                    zoom,
                    (viewport_width - f64::from(document.width) * zoom) / 2.0,
                    (viewport_height - f64::from(document.height) * zoom) / 2.0,
                    ViewMode::Fit,
                )
            }
            ViewCommand::OneToOne {
                viewport_width,
                viewport_height,
            } if valid_viewport(viewport_width, viewport_height) => (
                1.0,
                (viewport_width - f64::from(document.width)) / 2.0,
                (viewport_height - f64::from(document.height)) / 2.0,
                ViewMode::OneToOne,
            ),
            ViewCommand::ViewportResized {
                viewport_width,
                viewport_height,
            } if valid_viewport(viewport_width, viewport_height) => match self.view.mode {
                ViewMode::Manual => (
                    self.view.zoom,
                    self.view.pan_x,
                    self.view.pan_y,
                    ViewMode::Manual,
                ),
                ViewMode::Fit => {
                    let zoom = (viewport_width / f64::from(document.width))
                        .min(viewport_height / f64::from(document.height))
                        .mul_add(0.95, 0.0)
                        .clamp(MIN_ZOOM, MAX_ZOOM);
                    (
                        zoom,
                        (viewport_width - f64::from(document.width) * zoom) / 2.0,
                        (viewport_height - f64::from(document.height) * zoom) / 2.0,
                        ViewMode::Fit,
                    )
                }
                ViewMode::OneToOne => (
                    1.0,
                    (viewport_width - f64::from(document.width)) / 2.0,
                    (viewport_height - f64::from(document.height)) / 2.0,
                    ViewMode::OneToOne,
                ),
            },
            ViewCommand::BoxZoom {
                document_rect,
                viewport_width,
                viewport_height,
            } if valid_viewport(viewport_width, viewport_height)
                && document_rect.width > 0
                && document_rect.height > 0 =>
            {
                let zoom = (viewport_width / f64::from(document_rect.width))
                    .min(viewport_height / f64::from(document_rect.height))
                    .clamp(MIN_ZOOM, MAX_ZOOM);
                (
                    zoom,
                    (viewport_width - f64::from(document_rect.width) * zoom) / 2.0
                        - f64::from(document_rect.x) * zoom,
                    (viewport_height - f64::from(document_rect.height) * zoom) / 2.0
                        - f64::from(document_rect.y) * zoom,
                    ViewMode::Manual,
                )
            }
            _ => {
                return Err(CoreError::InvalidArgument(
                    "view command contains invalid values",
                ));
            }
        };
        if !next_zoom.is_finite()
            || !view_translation_is_supported(next_pan_x)
            || !view_translation_is_supported(next_pan_y)
        {
            return Err(CoreError::InvalidArgument(
                "view command result is outside the finite supported range",
            ));
        }
        if next_zoom != self.view.zoom
            || next_pan_x != self.view.pan_x
            || next_pan_y != self.view.pan_y
            || next_mode != self.view.mode
        {
            self.view.revision = self
                .view
                .revision
                .checked_add(1)
                .ok_or(CoreError::InvalidState("view revision overflow"))?;
            self.view.zoom = next_zoom;
            self.view.pan_x = next_pan_x;
            self.view.pan_y = next_pan_y;
            self.view.mode = next_mode;
        }
        Ok(self.view)
    }

    #[must_use]
    pub const fn view_state(&self) -> ViewState {
        self.view
    }
}

// Shared implementation helpers for this responsibility.

pub(super) fn validate_guide_position(
    document: &CellDocument,
    axis: GuideAxis,
    position: i32,
) -> Result<(), CoreError> {
    let limit = match axis {
        GuideAxis::Horizontal => document.height,
        GuideAxis::Vertical => document.width,
    };
    if position < 0
        || u32::try_from(position)
            .ok()
            .is_none_or(|value| value > limit)
    {
        Err(CoreError::InvalidArgument(
            "guide position is outside paper",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn validate_grid(grid: GridConfig) -> Result<(), CoreError> {
    if grid.spacing_x == 0
        || grid.spacing_y == 0
        || grid.spacing_x > 1_048_576
        || grid.spacing_y > 1_048_576
        || grid.subdivisions == 0
        || grid.subdivisions > 1_024
    {
        Err(CoreError::InvalidArgument("grid values are outside bounds"))
    } else {
        Ok(())
    }
}

pub(super) fn default_shortcuts() -> BTreeMap<u32, Vec<ShortcutStroke>> {
    [
        ShortcutBinding {
            command_id: 1,
            virtual_key: u32::from(b'Z'),
            modifiers: 1,
        },
        ShortcutBinding {
            command_id: 2,
            virtual_key: u32::from(b'Y'),
            modifiers: 1,
        },
        ShortcutBinding {
            command_id: 3,
            virtual_key: u32::from(b'C'),
            modifiers: 1,
        },
        ShortcutBinding {
            command_id: 4,
            virtual_key: u32::from(b'V'),
            modifiers: 1,
        },
    ]
    .into_iter()
    .map(|binding| {
        (
            binding.command_id,
            vec![ShortcutStroke {
                virtual_key: binding.virtual_key,
                modifiers: binding.modifiers,
            }],
        )
    })
    .collect()
}

pub(super) fn device_to_document(
    view: ViewState,
    width: u32,
    height: u32,
    device_x: f64,
    device_y: f64,
) -> Result<(f64, f64), CoreError> {
    if !device_x.is_finite() || !device_y.is_finite() {
        return Err(CoreError::InvalidArgument(
            "device coordinate is not finite",
        ));
    }
    let mut x = (device_x - view.pan_x) / view.zoom;
    let mut y = (device_y - view.pan_y) / view.zoom;
    if view.flip_horizontal {
        x = f64::from(width) - x;
    }
    if view.flip_vertical {
        y = f64::from(height) - y;
    }
    Ok((x, y))
}

pub(super) fn valid_viewport(width: f64, height: f64) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}

pub(super) fn view_translation_is_supported(value: f64) -> bool {
    value.is_finite() && value.abs() <= f64::from(MAX_STROKE_COORDINATE)
}

pub(super) fn stroke_coordinate_is_supported(value: f64) -> bool {
    value.is_finite() && value.abs() <= f64::from(MAX_STROKE_COORDINATE)
}
