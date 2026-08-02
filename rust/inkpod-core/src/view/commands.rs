use super::coordinates::*;
use crate::*;

impl Core {
    /// Applies a command to the primary view.
    ///
    /// Inputs use Canvas client device pixels. A semantic no-op keeps view revision;
    /// a real change advances only view revision. Invalid input or an active stroke
    /// leaves all view and document state unchanged.
    pub fn apply_view(&mut self, command: ViewCommand) -> Result<ViewState, CoreError> {
        if self.active_stroke.is_some() {
            return Err(CoreError::InvalidState(
                "view cannot change during an active stroke transaction",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let document_size = DocumentSizeU32::new(document.width, document.height);
        self.apply_view_for_document_size(command, document_size)
    }

    pub(super) fn apply_view_for_document_size(
        &mut self,
        command: ViewCommand,
        document_size: DocumentSizeU32,
    ) -> Result<ViewState, CoreError> {
        let command_viewport = match command {
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
            } => Some(
                DeviceSizeF64::new(viewport_width, viewport_height).map_err(|_| {
                    CoreError::InvalidArgument("view command contains invalid values")
                })?,
            ),
            ViewCommand::BoxZoom {
                viewport_width,
                viewport_height,
                ..
            } => Some(
                DeviceSizeF64::new(viewport_width, viewport_height).map_err(|_| {
                    CoreError::InvalidArgument("view command contains invalid values")
                })?,
            ),
            _ => None,
        };
        let mut toggle_candidate = self.view;
        let mut clear_render_cache = false;
        let toggle_changed = match command {
            ViewCommand::Flip { axis } => {
                match axis {
                    MirrorAxis::Horizontal => {
                        toggle_candidate.flip_horizontal = !toggle_candidate.flip_horizontal
                    }
                    MirrorAxis::Vertical => {
                        toggle_candidate.flip_vertical = !toggle_candidate.flip_vertical
                    }
                }
                true
            }
            ViewCommand::SetRulerVisible(value) => {
                let changed = toggle_candidate.ruler_visible != value;
                toggle_candidate.ruler_visible = value;
                changed
            }
            ViewCommand::SetGuidesVisible(value) => {
                let changed = toggle_candidate.guides_visible != value;
                toggle_candidate.guides_visible = value;
                changed
            }
            ViewCommand::SetGridVisible(value) => {
                let changed = toggle_candidate.grid_visible != value;
                toggle_candidate.grid_visible = value;
                changed
            }
            ViewCommand::SetSnapEnabled(value) => {
                let changed = toggle_candidate.snap_enabled != value;
                toggle_candidate.snap_enabled = value;
                toggle_candidate.guide_snap_enabled = value;
                toggle_candidate.grid_snap_enabled = value;
                changed
            }
            ViewCommand::SetGuideSnapEnabled(value) => {
                let changed = toggle_candidate.guide_snap_enabled != value;
                toggle_candidate.guide_snap_enabled = value;
                toggle_candidate.snap_enabled = value || toggle_candidate.grid_snap_enabled;
                changed
            }
            ViewCommand::SetGridSnapEnabled(value) => {
                let changed = toggle_candidate.grid_snap_enabled != value;
                toggle_candidate.grid_snap_enabled = value;
                toggle_candidate.snap_enabled = value || toggle_candidate.guide_snap_enabled;
                changed
            }
            ViewCommand::SetTransparentView(value) => {
                let changed = toggle_candidate.transparent_view != value;
                toggle_candidate.transparent_view = value;
                changed
            }
            ViewCommand::SetAlphaView(value) => {
                let changed = toggle_candidate.alpha_view != value;
                toggle_candidate.alpha_view = value;
                clear_render_cache = changed;
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
                toggle_candidate.revision = toggle_candidate
                    .revision
                    .checked_next()
                    .ok_or(CoreError::InvalidState("view revision overflow"))?;
            }
            self.view = toggle_candidate;
            if clear_render_cache {
                self.render_cache.clear();
            }
            return Ok(self.view);
        }
        let result_outside_range = || {
            CoreError::InvalidArgument("view command result is outside the finite supported range")
        };
        let (next_zoom, next_pan, next_mode) = match command {
            ViewCommand::PanBy {
                device_dx,
                device_dy,
            } if device_dx.is_finite() && device_dy.is_finite() => {
                let pan =
                    DeviceOffsetF64::new(self.view.pan.x + device_dx, self.view.pan.y + device_dy)
                        .map_err(|_| result_outside_range())?;
                (self.view.zoom, pan, ViewMode::Manual)
            }
            ViewCommand::ZoomAt {
                factor,
                device_x,
                device_y,
            } if factor.is_finite()
                && factor > 0.0
                && device_x.is_finite()
                && device_y.is_finite() =>
            {
                let device_point = DevicePointF64::new(device_x, device_y).map_err(|_| {
                    CoreError::InvalidArgument("view command contains invalid values")
                })?;
                let document_point = device_to_document(self.view, document_size, device_point);
                let zoom = ZoomFactor::clamped(self.view.zoom.get() * factor)
                    .map_err(|_| result_outside_range())?;
                let pan = ViewTransform::pan_for_anchor(
                    document_size,
                    zoom,
                    self.view.flip_horizontal,
                    self.view.flip_vertical,
                    document_point,
                    device_point,
                )
                .map_err(|_| result_outside_range())?;
                (zoom, pan, ViewMode::Manual)
            }
            ViewCommand::Fit {
                viewport_width: _,
                viewport_height: _,
            } => {
                let viewport = command_viewport.expect("fit viewport was parsed");
                let zoom = ZoomFactor::clamped(
                    (viewport.width / f64::from(document_size.width))
                        .min(viewport.height / f64::from(document_size.height))
                        .mul_add(0.95, 0.0),
                )
                .map_err(|_| result_outside_range())?;
                let pan = centered_pan(document_size, viewport, zoom)
                    .map_err(|_| result_outside_range())?;
                (zoom, pan, ViewMode::Fit)
            }
            ViewCommand::OneToOne {
                viewport_width: _,
                viewport_height: _,
            } => {
                let viewport = command_viewport.expect("1:1 viewport was parsed");
                let pan = centered_pan(document_size, viewport, ZoomFactor::ONE)
                    .map_err(|_| result_outside_range())?;
                (ZoomFactor::ONE, pan, ViewMode::OneToOne)
            }
            ViewCommand::ViewportResized {
                viewport_width: _,
                viewport_height: _,
            } => match self.view.mode {
                ViewMode::Manual => (self.view.zoom, self.view.pan, ViewMode::Manual),
                ViewMode::Fit => {
                    let viewport = command_viewport.expect("resized viewport was parsed");
                    let zoom = ZoomFactor::clamped(
                        (viewport.width / f64::from(document_size.width))
                            .min(viewport.height / f64::from(document_size.height))
                            .mul_add(0.95, 0.0),
                    )
                    .map_err(|_| result_outside_range())?;
                    let pan = centered_pan(document_size, viewport, zoom)
                        .map_err(|_| result_outside_range())?;
                    (zoom, pan, ViewMode::Fit)
                }
                ViewMode::OneToOne => {
                    let viewport = command_viewport.expect("resized viewport was parsed");
                    let pan = centered_pan(document_size, viewport, ZoomFactor::ONE)
                        .map_err(|_| result_outside_range())?;
                    (ZoomFactor::ONE, pan, ViewMode::OneToOne)
                }
            },
            ViewCommand::BoxZoom {
                document_rect,
                viewport_width: _,
                viewport_height: _,
            } => {
                let document_rect = DocumentRectI32::from_public(document_rect);
                if !document_rect.has_positive_size() {
                    return Err(CoreError::InvalidArgument(
                        "view command contains invalid values",
                    ));
                }
                let viewport = command_viewport.expect("box zoom viewport was parsed");
                let zoom = ZoomFactor::clamped(
                    (viewport.width / f64::from(document_rect.width))
                        .min(viewport.height / f64::from(document_rect.height)),
                )
                .map_err(|_| result_outside_range())?;
                let pan = box_zoom_pan(document_rect, viewport, zoom)
                    .map_err(|_| result_outside_range())?;
                (zoom, pan, ViewMode::Manual)
            }
            _ => {
                return Err(CoreError::InvalidArgument(
                    "view command contains invalid values",
                ));
            }
        };
        let mut candidate = self.view;
        if let Some(viewport) = command_viewport {
            candidate.viewport = viewport;
        }
        if next_zoom != self.view.zoom || next_pan != self.view.pan || next_mode != self.view.mode {
            candidate.revision = candidate
                .revision
                .checked_next()
                .ok_or(CoreError::InvalidState("view revision overflow"))?;
            candidate.zoom = next_zoom;
            candidate.pan = next_pan;
            candidate.mode = next_mode;
        }
        self.view = candidate;
        Ok(self.view)
    }

    /// Returns a copy of the primary immutable view state.
    #[must_use]
    pub const fn view_state(&self) -> ViewState {
        self.view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_boundaries_reject_non_finite_values_without_advancing_revision() {
        let mut core = Core::new();
        core.new_cell(16, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let maximum = core
            .apply_view(ViewCommand::ZoomAt {
                factor: f64::MAX,
                device_x: 0.0,
                device_y: 0.0,
            })
            .unwrap();
        assert_eq!(maximum.zoom(), MAX_ZOOM);
        let before = core.view_state();
        assert!(
            core.apply_view(ViewCommand::ZoomAt {
                factor: f64::NAN,
                device_x: 0.0,
                device_y: 0.0,
            })
            .is_err()
        );
        assert!(
            core.apply_view(ViewCommand::ViewportResized {
                viewport_width: f64::INFINITY,
                viewport_height: 10.0,
            })
            .is_err()
        );
        assert_eq!(core.view_state(), before);
        assert!(DevicePointF64::new(f64::NAN, 0.0).is_err());
        assert!(DeviceSizeF64::new(0.0, 10.0).is_err());
        assert!(DeviceSizeF64::new(10.0, f64::INFINITY).is_err());
    }

    #[test]
    fn view_revision_overflow_publishes_neither_toggle_nor_transform() {
        let mut core = Core::new();
        core.new_cell(16, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.view.revision = ViewRevision::from_raw(u64::MAX);
        let before = core.view_state();

        assert!(core.apply_view(ViewCommand::SetAlphaView(true)).is_err());
        assert_eq!(core.view_state(), before);
        assert!(
            core.apply_view(ViewCommand::PanBy {
                device_dx: 1.0,
                device_dy: -1.0,
            })
            .is_err()
        );
        assert_eq!(core.view_state(), before);
    }
}
