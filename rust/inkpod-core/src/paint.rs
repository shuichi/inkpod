//! Fill, color sampling, palette, and color-check operations.

use super::*;
use crate::document::ensure_editable_plane;
use crate::selection::{combine_selection_masks, selection_from_rect};

impl Core {
    /// Applies a fill atomically to the active editable raster plane.
    pub fn apply_fill(&mut self, request: &FillRequest) -> Result<FillOutcome, CoreError> {
        self.apply_fill_with_cancel(request, || false)
    }

    /// Applies a fill with optional visible light-table boundary/color sampling.
    ///
    /// Success creates at most one history entry; a zero-pixel result is a no-op.
    /// Invalid input and fill failure leave document state unchanged.
    pub fn apply_fill_with_light_table(
        &mut self,
        request: &FillRequest,
        use_boundary: bool,
        use_sampled_color: bool,
    ) -> Result<FillOutcome, CoreError> {
        self.apply_fill_internal(request, use_boundary, use_sampled_color, || false)
    }

    /// Applies a light-table-aware fill with cooperative cancellation.
    ///
    /// The callback may be invoked repeatedly. Cancellation returns
    /// [`CoreError::Cancelled`] and never commits partial pixels, revision, or history.
    pub fn apply_fill_with_light_table_and_cancel(
        &mut self,
        request: &FillRequest,
        use_boundary: bool,
        use_sampled_color: bool,
        is_cancelled: impl FnMut() -> bool,
    ) -> Result<FillOutcome, CoreError> {
        self.apply_fill_internal(request, use_boundary, use_sampled_color, is_cancelled)
    }

    /// Applies a fill with cooperative cancellation and no light-table sampling.
    ///
    /// Cancellation and failure are atomic; a successful change is one undo unit.
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
        let target_plane_id = document.plane_for_paint_role(ActivePlane::Color)?.id;
        ensure_editable_plane(document, target_plane_id)?;
        let target_raster = &document
            .plane_by_id(target_plane_id)
            .ok_or(CoreError::InvalidState(
                "fill target plane no longer exists",
            ))?
            .raster;
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
                self.document_revision.get(),
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
                        raster.set_pixel(x, y, boundary, self.document_revision.get())?;
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
            match (target_raster.format(), sampled) {
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
                target_raster,
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
                    target_raster,
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
                    target_raster,
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
                    revision: self.document_revision.get(),
                    accepted_commands: 1,
                },
                changed_pixels: 0,
            });
        }

        let changed_pixels = u64::try_from(plan.edits.len())
            .map_err(|_| CoreError::InvalidState("fill edit count is not representable"))?;
        let mut next_color = target_raster.clone();
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        let mut changes = Vec::with_capacity(plan.edits.len());
        let mut touched = BTreeSet::new();
        for edit in plan.edits {
            next_color.set_pixel(edit.x, edit.y, edit.after, revision.get())?;
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
        document
            .plane_by_id_mut(target_plane_id)
            .ok_or(CoreError::InvalidState(
                "fill target plane no longer exists",
            ))?
            .raster = next_color;
        document.active_plane_id = target_plane_id;
        self.document_revision = revision;
        self.commit_pixel_history(target_plane_id, changes, after_state);
        Ok(FillOutcome {
            dispatch: DispatchOutcome {
                revision: revision.get(),
                accepted_commands: 1,
            },
            changed_pixels,
        })
    }

    /// Samples a color at one in-bounds document pixel from the requested source.
    ///
    /// Sampling is read-only and does not affect revisions, history, or dirty state.
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

    /// Borrows palette colors for the lifetime of the Core borrow.
    pub fn palette(&self) -> Result<&[PixelValue], CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .palette
            .colors())
    }

    /// Atomically replaces the document palette.
    ///
    /// An identical palette is a no-op; a change is one undoable edit. Invalid
    /// formats or palette limits fail without partial replacement.
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
            return Ok(self.noop_outcome());
        }
        let mut edit = self.begin_document_edit()?;
        edit.working_mut().palette = after;
        edit.commit_palette(self)
    }

    /// Returns the straight-alpha RGBA main-line display color.
    pub fn main_line_color(&self) -> Result<PixelValue, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .main_line_color)
    }

    /// Replaces the main-line display color as one undoable metadata edit.
    ///
    /// Only RGBA values are accepted; identical color is a no-op.
    pub fn set_main_line_color(&mut self, color: PixelValue) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if color.rgba16().is_none() {
            return Err(CoreError::InvalidArgument(
                "main-line base color must be RGBA",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let main_line_plane_id = document.plane_for_role(ActivePlane::MainLine)?.id;
        ensure_editable_plane(document, main_line_plane_id)?;
        if !matches!(
            document.raster(ActivePlane::MainLine).format(),
            PixelFormat::BinaryMask8 | PixelFormat::Grayscale8 | PixelFormat::Grayscale16
        ) {
            return Err(CoreError::InvalidState(
                "main-line base color requires a binary or grayscale main plane",
            ));
        }
        let before = document.main_line_color;
        if before == color {
            return Ok(self.noop_outcome());
        }
        let mut edit = self.begin_document_edit()?;
        edit.working_mut().main_line_color = color;
        edit.commit_main_line_color(self)
    }

    /// Selects a non-destructive color-check render mode.
    ///
    /// A change advances only view revision and invalidates render cache; document
    /// revision, history, dirty state, and savepoint are unchanged.
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
                .checked_next()
                .ok_or(CoreError::InvalidState("view revision overflow"))?;
            self.render_cache.clear();
        }
        Ok(self.view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn binary_main_line_color_is_editable_and_undoable() {
        let mut core = Core::new();
        core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let original = core.main_line_color().unwrap();
        let replacement = PixelValue::Rgba([17, 34, 51, 255]);
        core.set_main_line_color(replacement).unwrap();
        assert_eq!(core.main_line_color().unwrap(), replacement);
        core.undo().unwrap();
        assert_eq!(core.main_line_color().unwrap(), original);
        core.redo().unwrap();
        assert_eq!(core.main_line_color().unwrap(), replacement);
    }

    #[test]
    fn grayscale_eyedropper_and_color_check_are_view_only() {
        let mut core = Core::new();
        core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let document = core.document.as_mut().unwrap();
        document.layers[0].kind = LayerKind::GrayscaleColoring;
        document.layers[0].planes[0].raster =
            TileRaster::new(4, 4, PixelFormat::Grayscale8).unwrap();
        document.layers[0].planes[0]
            .raster
            .set_pixel(1, 1, PixelValue::Grayscale8(128), 2)
            .unwrap();
        document.active_plane_id = document.layers[0].planes[0].id;
        let line_color = PixelValue::Rgba16([1_001, 2_002, 3_003, 65_535]);
        core.set_main_line_color(line_color).unwrap();
        assert_eq!(
            core.eyedropper(EyedropperSource::SelectedPlane, 1, 1)
                .unwrap(),
            line_color
        );
        let normal_snapshot = core.build_snapshot();
        let normal_tile_revision = normal_snapshot.tiles()[0].tile_revision();
        let before = core.document_info().unwrap();
        core.set_color_check(Some(ColorCheckMode::NativeAlpha))
            .unwrap();
        let after = core.document_info().unwrap();
        assert_eq!(after.document_revision, before.document_revision);
        assert_eq!(after.main_plane_checksum, before.main_plane_checksum);
        assert!(after.view_revision > before.view_revision);
        let check_snapshot = core.build_snapshot();
        assert_eq!(
            check_snapshot.feature_flags(),
            SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA
        );
        assert_ne!(
            check_snapshot.tiles()[0].tile_revision(),
            normal_tile_revision
        );

        let palette = [
            PixelValue::Rgba([12, 34, 56, 255]),
            PixelValue::Rgba16([1, 257, 32_769, 65_534]),
        ];
        core.replace_palette(&palette).unwrap();
        assert_eq!(core.palette().unwrap(), palette);
        core.undo().unwrap();
        assert!(core.palette().unwrap().is_empty());
        core.redo().unwrap();
        assert_eq!(core.palette().unwrap(), palette);

        let path = std::env::temp_dir().join(format!(
            "inkpod-core-test-color-metadata-{}-{}.inkpod",
            std::process::id(),
            core.document_info().unwrap().document_revision
        ));
        core.save(&path).unwrap();
        let mut reopened = Core::new();
        reopened.open(&path).unwrap();
        assert_eq!(reopened.main_line_color().unwrap(), line_color);
        assert_eq!(reopened.palette().unwrap(), palette);
        fs::remove_file(path).unwrap();
    }
}
