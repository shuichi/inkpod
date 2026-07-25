//! Fill, color sampling, palette, and color-check operations.

use super::*;
use crate::document::ensure_editable_role;
use crate::selection::{combine_selection_masks, selection_from_rect};

impl Core {
    pub fn apply_fill(&mut self, request: &FillRequest) -> Result<FillOutcome, CoreError> {
        self.apply_fill_with_cancel(request, || false)
    }

    pub fn apply_fill_with_light_table(
        &mut self,
        request: &FillRequest,
        use_boundary: bool,
        use_sampled_color: bool,
    ) -> Result<FillOutcome, CoreError> {
        self.apply_fill_internal(request, use_boundary, use_sampled_color, || false)
    }

    pub fn apply_fill_with_light_table_and_cancel(
        &mut self,
        request: &FillRequest,
        use_boundary: bool,
        use_sampled_color: bool,
        is_cancelled: impl FnMut() -> bool,
    ) -> Result<FillOutcome, CoreError> {
        self.apply_fill_internal(request, use_boundary, use_sampled_color, is_cancelled)
    }

    pub fn apply_fill_with_cancel(
        &mut self,
        request: &FillRequest,
        is_cancelled: impl FnMut() -> bool,
    ) -> Result<FillOutcome, CoreError> {
        self.apply_fill_internal(request, false, false, is_cancelled)
    }

    fn apply_fill_internal(
        &mut self,
        request: &FillRequest,
        use_light_table_boundary: bool,
        use_light_table_color: bool,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<FillOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        ensure_editable_role(document, ActivePlane::Color)?;
        let document_pixels = u64::from(document.width)
            .checked_mul(u64::from(document.height))
            .ok_or(CoreError::InvalidArgument("fill work size overflows"))?;
        if document_pixels > MAX_FILL_PIXELS {
            return Err(CoreError::InvalidArgument(
                "fill document exceeds the bounded work limit",
            ));
        }
        let operation_selection = request
            .selection
            .map(|rect| {
                selection_from_rect(document.width, document.height, rect, &mut is_cancelled)
            })
            .transpose()?;
        let selection = match (request.use_document_selection, operation_selection) {
            (true, Some(operation)) => Some(combine_selection_masks(
                &document.selection,
                &operation,
                SelectionOperation::Intersect,
                self.document_revision,
            )?),
            (true, None) => Some(document.selection.clone()),
            (false, operation) => operation,
        };
        let options = FillOptions {
            tolerance: request.tolerance,
            detached_regions: request.detached_regions,
            overflow_abort: request.overflow_abort,
            gap_close: request.gap_close,
            transparent_only: request.transparent_only,
            inclusion_mode: request.inclusion_mode,
            inclusion_colors: request.inclusion_colors.clone(),
        };
        let light_boundary = if use_light_table_boundary {
            let mut raster = document.raster(ActivePlane::MainLine).clone();
            for y in 0..document.height {
                if is_cancelled() {
                    return Err(CoreError::Cancelled);
                }
                for x in 0..document.width {
                    if document
                        .light_table
                        .sample(document.frames.reference_frame, x, y)?
                        .is_some()
                    {
                        let boundary = match raster.format() {
                            PixelFormat::BinaryMask8 => PixelValue::Binary(255),
                            PixelFormat::Grayscale8 => PixelValue::Grayscale8(255),
                            PixelFormat::Grayscale16 => PixelValue::Grayscale16(u16::MAX),
                            _ => {
                                return Err(CoreError::InvalidState(
                                    "main-line format cannot hold a light-table boundary",
                                ));
                            }
                        };
                        raster.set_pixel(x, y, boundary, self.document_revision)?;
                    }
                }
            }
            Some(raster)
        } else {
            None
        };
        let main_line = light_boundary
            .as_ref()
            .unwrap_or_else(|| document.raster(ActivePlane::MainLine));
        let fill_color = if use_light_table_color {
            let sampled = document
                .light_table
                .sample(
                    document.frames.reference_frame,
                    request.seed_x,
                    request.seed_y,
                )?
                .ok_or(CoreError::InvalidState(
                    "light-table fill color is unavailable at the seed",
                ))?;
            match (document.raster(ActivePlane::Color).format(), sampled) {
                (PixelFormat::StraightRgba8, PixelValue::Rgba(value)) => PixelValue::Rgba(value),
                (PixelFormat::StraightRgba16, PixelValue::Rgba16(value)) => {
                    PixelValue::Rgba16(value)
                }
                (PixelFormat::StraightRgba16, PixelValue::Rgba(value)) => PixelValue::Rgba16([
                    u16::from(value[0]) * 257,
                    u16::from(value[1]) * 257,
                    u16::from(value[2]) * 257,
                    u16::from(value[3]) * 257,
                ]),
                _ => {
                    return Err(CoreError::InvalidState(
                        "light-table fill color does not match the color plane",
                    ));
                }
            }
        } else {
            request.color
        };
        let plan = match request.operation {
            FillOperation::Seed => seed_fill_with_cancel(
                main_line,
                document.raster(ActivePlane::Color),
                selection.as_ref(),
                (request.seed_x, request.seed_y),
                fill_color,
                &options,
                &mut is_cancelled,
            )?,
            FillOperation::ClosedRegion => {
                let operation = selection.as_ref().ok_or(CoreError::InvalidArgument(
                    "closed-region fill requires an operation selection",
                ))?;
                closed_region_fill_with_cancel(
                    main_line,
                    document.raster(ActivePlane::Color),
                    operation,
                    fill_color,
                    &options,
                    &mut is_cancelled,
                )?
            }
            FillOperation::Extend => {
                let operation = selection.as_ref().ok_or(CoreError::InvalidArgument(
                    "fill extension requires an operation selection",
                ))?;
                extend_fill_with_cancel(
                    document.raster(ActivePlane::Color),
                    operation,
                    (request.seed_x, request.seed_y),
                    request.extension_distance,
                    &mut is_cancelled,
                )?
            }
        };
        if plan.edits.is_empty() {
            return Ok(FillOutcome {
                dispatch: DispatchOutcome {
                    revision: self.document_revision,
                    accepted_commands: 1,
                },
                changed_pixels: 0,
            });
        }

        let changed_pixels = u64::try_from(plan.edits.len())
            .map_err(|_| CoreError::InvalidState("fill edit count is not representable"))?;
        let mut next_color = document.raster(ActivePlane::Color).clone();
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        let mut changes = Vec::with_capacity(plan.edits.len());
        let mut touched = BTreeSet::new();
        for edit in plan.edits {
            next_color.set_pixel(edit.x, edit.y, edit.after, revision)?;
            touched.insert(TileCoord {
                x: edit.x / TILE_SIZE,
                y: edit.y / TILE_SIZE,
            });
            changes.push(PixelChange {
                x: edit.x,
                y: edit.y,
                before: edit.before,
                after: edit.after,
            });
        }
        for coord in touched {
            next_color.remove_tile_if_empty(coord);
        }
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let color_plane = document.plane_for_role_mut(ActivePlane::Color)?;
        let color_plane_id = color_plane.id;
        color_plane.raster = next_color;
        document.active_plane_id = color_plane_id;
        self.document_revision = revision;
        self.commit_pixel_history(color_plane_id, changes, after_state);
        Ok(FillOutcome {
            dispatch: DispatchOutcome {
                revision,
                accepted_commands: 1,
            },
            changed_pixels,
        })
    }

    pub fn eyedropper(
        &self,
        source: EyedropperSource,
        x: u32,
        y: u32,
    ) -> Result<PixelValue, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if source == EyedropperSource::LightTableTopmost {
            return document
                .light_table
                .sample(document.frames.reference_frame, x, y)?
                .ok_or(CoreError::InvalidState(
                    "eyedropper source is transparent or unavailable",
                ));
        }
        let line = PlaneSample {
            raster: document.raster(ActivePlane::MainLine),
            base_color: Some(document.main_line_color),
        };
        let color = PlaneSample {
            raster: document.raster(ActivePlane::Color),
            base_color: None,
        };
        let selected = match document.active_plane_role() {
            ActivePlane::MainLine => line,
            ActivePlane::Color => color,
        };
        eyedropper(source, x, y, selected, &[line, color], &[])?.ok_or(CoreError::InvalidState(
            "eyedropper source is transparent or unavailable",
        ))
    }

    pub fn palette(&self) -> Result<&[PixelValue], CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .palette
            .colors())
    }

    pub fn replace_palette(&mut self, colors: &[PixelValue]) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let mut after = Palette::default();
        for color in colors {
            after.push(*color)?;
        }
        let before = self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .palette
            .clone();
        if before == after {
            return Ok(DispatchOutcome {
                revision: self.document_revision,
                accepted_commands: 1,
            });
        }
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        self.document.as_mut().ok_or(CoreError::NoDocument)?.palette = after.clone();
        self.document_revision = revision;
        self.commit_history_change(HistoryChange::Palette { before, after }, after_state);
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn main_line_color(&self) -> Result<PixelValue, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .main_line_color)
    }

    pub fn set_main_line_color(&mut self, color: PixelValue) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if color.rgba16().is_none() {
            return Err(CoreError::InvalidArgument(
                "main-line base color must be RGBA",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        ensure_editable_role(document, ActivePlane::MainLine)?;
        if !matches!(
            document.raster(ActivePlane::MainLine).format(),
            PixelFormat::Grayscale8 | PixelFormat::Grayscale16
        ) {
            return Err(CoreError::InvalidState(
                "main-line base color is editable only for a grayscale main plane",
            ));
        }
        let before = document.main_line_color;
        if before == color {
            return Ok(DispatchOutcome {
                revision: self.document_revision,
                accepted_commands: 1,
            });
        }
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        self.document
            .as_mut()
            .ok_or(CoreError::NoDocument)?
            .main_line_color = color;
        self.document_revision = revision;
        self.render_cache.clear();
        self.commit_history_change(
            HistoryChange::MainLineColor {
                before,
                after: color,
            },
            after_state,
        );
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn set_color_check(
        &mut self,
        mode: Option<ColorCheckMode>,
    ) -> Result<ViewState, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document.is_none() {
            return Err(CoreError::NoDocument);
        }
        if self.color_check != mode {
            self.color_check = mode;
            self.view.revision = self
                .view
                .revision
                .checked_add(1)
                .ok_or(CoreError::InvalidState("view revision overflow"))?;
            self.render_cache.clear();
        }
        Ok(self.view)
    }
}
