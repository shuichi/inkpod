//! Document inspection and immutable render snapshots.

use super::*;

/// One immutable raster tile positioned in document pixel coordinates.
///
/// `origin_x..origin_x + width` and `origin_y..origin_y + height` are half-open
/// document ranges. Device zoom, pan, view flip, and OS DPI are not applied to
/// tile geometry or pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderTile {
    tile_id: u64,
    origin: DocumentPointI32,
    size: DocumentSizeU32,
    stride_bytes: u32,
    pixels: Arc<[u8]>,
    source_revision: RenderRevision,
    tile_revision: RenderRevision,
}

impl RenderTile {
    /// Returns the deterministic tile identifier derived from document tile coordinates.
    #[must_use]
    pub const fn tile_id(&self) -> u64 {
        self.tile_id
    }

    /// Returns the tile's horizontal origin in document pixels.
    #[must_use]
    pub const fn origin_x(&self) -> i32 {
        self.origin.x
    }

    /// Returns the tile's vertical origin in document pixels.
    #[must_use]
    pub const fn origin_y(&self) -> i32 {
        self.origin.y
    }

    /// Returns the populated tile width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.size.width
    }

    /// Returns the populated tile height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.size.height
    }

    /// Returns the byte distance between adjacent premultiplied BGRA8 rows.
    #[must_use]
    pub const fn stride_bytes(&self) -> u32 {
        self.stride_bytes
    }

    /// Borrows immutable premultiplied BGRA8 pixels for the lifetime of the tile.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns the cache revision, which changes only when this tile is recomposed.
    #[must_use]
    pub const fn tile_revision(&self) -> u64 {
        self.tile_revision.get()
    }
}

/// Immutable document render data with a separate device-pixel view transform.
///
/// Raster tile origins, vector control points, guides, and grid values are all
/// document coordinates. `view()` is the only document-to-device transform and
/// follows `device = flipped_document * zoom + pan`; pan and viewport use client
/// device pixels. Core does not apply OS DPI to that Canvas transform.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderSnapshot {
    revision: RenderRevision,
    feature_flags: u64,
    view: ViewState,
    document_size: DocumentSizeU32,
    guides: Vec<Guide>,
    grid: GridConfig,
    tiles: Vec<RenderTile>,
    vector_segments: Vec<RenderVectorSegment>,
    vector_fills: Vec<RenderVectorFill>,
}

impl RenderSnapshot {
    /// Returns the document or transient-preview revision represented by the snapshot.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision.get()
    }

    /// Returns a bitwise combination of `SNAPSHOT_FEATURE_*` flags.
    #[must_use]
    pub const fn feature_flags(&self) -> u64 {
        self.feature_flags
    }

    /// Returns the immutable view transform captured with the snapshot.
    #[must_use]
    pub const fn view(&self) -> ViewState {
        self.view
    }

    /// Returns document width in pixels.
    #[must_use]
    pub const fn document_width(&self) -> u32 {
        self.document_size.width
    }

    /// Returns document height in pixels.
    #[must_use]
    pub const fn document_height(&self) -> u32 {
        self.document_size.height
    }

    /// Borrows ordered document-space guides for the snapshot lifetime.
    #[must_use]
    pub fn guides(&self) -> &[Guide] {
        &self.guides
    }

    /// Returns the document-space grid configuration.
    #[must_use]
    pub const fn grid(&self) -> GridConfig {
        self.grid
    }

    /// Returns the number of raster tiles.
    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Borrows immutable raster tiles for the snapshot lifetime.
    #[must_use]
    pub fn tiles(&self) -> &[RenderTile] {
        &self.tiles
    }

    /// Borrows immutable document-space vector segments for the snapshot lifetime.
    #[must_use]
    pub fn vector_segments(&self) -> &[RenderVectorSegment] {
        &self.vector_segments
    }

    /// Borrows immutable document-space vector fills for the snapshot lifetime.
    #[must_use]
    pub fn vector_fills(&self) -> &[RenderVectorFill] {
        &self.vector_fills
    }
}

impl Core {
    /// Returns a read-only summary of the active document.
    ///
    /// The query does not change revisions, history, dirty state, or caches.
    pub fn document_info(&self) -> Result<DocumentInfo, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let (layer_id, main_plane_id, color_plane_id) = document.primary_ids();
        Ok(DocumentInfo {
            document_revision: self.document_revision.get(),
            view_revision: self.view.revision.get(),
            document_id: document.id.get(),
            document_uuid: document.uuid,
            layer_id: layer_id.get(),
            main_plane_id: main_plane_id.get(),
            color_plane_id: color_plane_id.get(),
            width: document.width,
            height: document.height,
            dpi_x_milli: document.dpi_x_milli,
            dpi_y_milli: document.dpi_y_milli,
            frames: document.frames,
            dirty: self.savepoint != Some(self.current_state),
            can_undo: self.history_cursor > 0,
            can_redo: self.history_cursor < self.history.len(),
            active_plane: document.active_plane_role(),
            recovered: self.recovered,
            main_plane_checksum: document.raster(ActivePlane::MainLine).checksum(),
            color_plane_checksum: document.raster(ActivePlane::Color).checksum(),
        })
    }

    /// Reads one pixel from a conventional raster plane in document coordinates.
    ///
    /// `x` and `y` must be inside the half-open document bounds.
    pub fn plane_pixel(&self, plane: ActivePlane, x: u32, y: u32) -> Result<PixelValue, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .raster(plane)
            .pixel(x, y)?)
    }

    /// Builds an owned immutable rendering snapshot for the primary view.
    ///
    /// Active stroke/filter previews are represented without committing them.
    /// Reusable tile buffers are shared by ownership-safe [`Arc`] storage; the
    /// returned snapshot never exposes mutable Core state. Building may update
    /// render-cache revisions but not document/view revisions, history, or dirty state.
    #[must_use]
    pub fn build_snapshot(&mut self) -> RenderSnapshot {
        let mut cache = std::mem::take(&mut self.render_cache);
        let Some(document) = self
            .active_stroke
            .as_ref()
            .map(|session| &session.preview_document)
            .or_else(|| {
                self.filter_preview
                    .as_ref()
                    .map(|session| &session.preview_document)
            })
            .or(self.document.as_ref())
        else {
            cache.clear();
            self.render_cache = cache;
            return RenderSnapshot {
                revision: RenderRevision::from_raw(self.document_revision.get()),
                feature_flags: 0,
                view: self.view,
                document_size: DocumentSizeU32::new(0, 0),
                guides: Vec::new(),
                grid: GridConfig::default(),
                tiles: Vec::new(),
                vector_segments: Vec::new(),
                vector_fills: Vec::new(),
            };
        };
        let snapshot_revision = self
            .active_stroke
            .as_ref()
            .map(|session| RenderRevision::from_raw(session.preview_revision.get()))
            .or_else(|| {
                self.filter_preview
                    .as_ref()
                    .map(|session| RenderRevision::from_raw(session.preview_revision.get()))
            })
            .unwrap_or_else(|| RenderRevision::from_raw(self.document_revision.get()));
        let feature_flags = match self.color_check {
            Some(ColorCheckMode::LegacyWhiteTransparency) => {
                SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE
            }
            Some(ColorCheckMode::NativeAlpha) => SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA,
            None => 0,
        };
        let mut coords: BTreeSet<_> = document
            .layers
            .iter()
            .filter(|layer| layer.visible)
            .flat_map(|layer| layer.planes.iter())
            .filter(|plane| plane.visible)
            .flat_map(|plane| plane.raster.allocated_coords())
            .chain(document.selection.allocated_coords())
            .collect();
        if document.light_table.has_visible_items() || self.view.alpha_view {
            let tiles_x = document.width.div_ceil(TILE_SIZE);
            let tiles_y = document.height.div_ceil(TILE_SIZE);
            for y in 0..tiles_y {
                for x in 0..tiles_x {
                    coords.insert(TileCoord { x, y });
                }
            }
        }
        let mut tiles = Vec::with_capacity(coords.len());
        for coord in &coords {
            let source_revision = document
                .layers
                .iter()
                .filter(|layer| layer.visible)
                .flat_map(|layer| layer.planes.iter())
                .filter(|plane| plane.visible)
                .map(|plane| plane.raster.tile_revision(*coord))
                .max()
                .unwrap_or(0)
                .max(document.light_table.source_revision())
                .max(document.selection.tile_revision(*coord));
            let source_revision = RenderRevision::from_raw(source_revision);
            if cache
                .get(coord)
                .is_none_or(|tile| tile.source_revision != source_revision)
            {
                let tile_revision = self.next_render_tile_revision;
                self.next_render_tile_revision =
                    self.next_render_tile_revision.wrapping_next_nonzero();
                if let Some(tile) = compose_tile(
                    document,
                    *coord,
                    self.color_check,
                    self.view.alpha_view,
                    source_revision,
                    tile_revision,
                ) {
                    cache.insert(*coord, tile);
                } else {
                    cache.remove(coord);
                }
            }
            if let Some(tile) = cache.get(coord) {
                tiles.push(tile.clone());
            }
        }
        cache.retain(|coord, _| coords.contains(coord));
        let document_size = DocumentSizeU32::new(document.width, document.height);
        let (vector_segments, vector_fills) = if self.view.alpha_view {
            (Vec::new(), Vec::new())
        } else {
            document.vector.render_items(document)
        };
        self.render_cache = cache;
        RenderSnapshot {
            revision: snapshot_revision,
            feature_flags,
            view: self.view,
            document_size,
            guides: document.guides.clone(),
            grid: document.grid,
            tiles,
            vector_segments,
            vector_fills,
        }
    }
}

// Shared implementation helpers for this responsibility.

pub(super) fn compose_tile(
    document: &CellDocument,
    coord: TileCoord,
    color_check: Option<ColorCheckMode>,
    alpha_view: bool,
    source_revision: RenderRevision,
    tile_revision: RenderRevision,
) -> Option<RenderTile> {
    let origin_x = coord.x.checked_mul(TILE_SIZE)?;
    let origin_y = coord.y.checked_mul(TILE_SIZE)?;
    if origin_x >= document.width || origin_y >= document.height {
        return None;
    }
    let width = TILE_SIZE.min(document.width - origin_x);
    let height = TILE_SIZE.min(document.height - origin_y);
    let stride = width.checked_mul(4)?;
    let capacity = usize::try_from(stride.checked_mul(height)?).ok()?;
    let mut pixels = Vec::with_capacity(capacity);
    for y in 0..height {
        for x in 0..width {
            let document_x = origin_x + x;
            let document_y = origin_y + y;
            let mut composite = document
                .light_table
                .composite(document.frames.reference_frame, document_x, document_y)
                .unwrap_or([0_u8; 4]);
            // Layer index zero is the top of the palette. Composite from the
            // bottom towards the top so palette order and rendered order agree.
            for layer in document.layers.iter().rev().filter(|layer| layer.visible) {
                if let Some(adjustment) = document.adjustments.get(&layer.id) {
                    let adjusted =
                        inkpod_image::apply_adjustment(PixelValue::Rgba(composite), adjustment)
                            .ok()?
                            .rgba16()?
                            .map(|channel| ((u32::from(channel) + 128) / 257) as u8);
                    composite = std::array::from_fn(|channel| {
                        ((u32::from(composite[channel]) * (1_000 - layer.opacity_milli)
                            + u32::from(adjusted[channel]) * layer.opacity_milli
                            + 500)
                            / 1_000) as u8
                    });
                    continue;
                }
                let mut layer_pixel = [0_u8; 4];
                for plane in layer
                    .planes
                    .iter()
                    .filter(|plane| plane.visible && plane.kind != PlaneType::MainLine)
                    .chain(
                        layer
                            .planes
                            .iter()
                            .filter(|plane| plane.visible && plane.kind == PlaneType::MainLine),
                    )
                {
                    let value = plane.raster.pixel(document_x, document_y).ok()?;
                    let mut rgba = match plane.kind {
                        PlaneType::MainLine => {
                            let coverage = match value {
                                PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value,
                                PixelValue::Grayscale16(value) => {
                                    ((u32::from(value) + 128) / 257) as u8
                                }
                                _ => return None,
                            };
                            let mut line = rgba8_for_display(document.main_line_color)?;
                            line[3] =
                                ((u32::from(line[3]) * u32::from(coverage) + 127) / 255) as u8;
                            line
                        }
                        PlaneType::Color | PlaneType::Raster => rgba8_for_display(value)?,
                        PlaneType::Selection => {
                            let coverage = match value {
                                PixelValue::Binary(value) => value,
                                _ => return None,
                            };
                            [0, 160, 255, coverage / 3]
                        }
                        PlaneType::VectorMainLine
                        | PlaneType::ColorTrace
                        | PlaneType::VectorFill => [0, 0, 0, 0],
                    };
                    rgba[3] = ((u32::from(rgba[3]) * plane.opacity_milli + 500) / 1_000) as u8;
                    layer_pixel = blend_rgba_over(layer_pixel, rgba);
                }
                layer_pixel[3] =
                    ((u32::from(layer_pixel[3]) * layer.opacity_milli + 500) / 1_000) as u8;
                composite = blend_rgba_over(composite, layer_pixel);
            }
            if alpha_view {
                let alpha = composite[3];
                pixels.extend_from_slice(&[alpha, alpha, alpha, 255]);
                continue;
            }
            if let Some(mode) = color_check {
                let check_value = PixelValue::Rgba(composite);
                let check_pixel = match color_check_category(check_value, mode) {
                    ColorCheckCategory::ExactWhite => [255, 255, 255, 255],
                    ColorCheckCategory::Transparent => [255, 0, 255, 255],
                    ColorCheckCategory::Colored => [0, 0, 0, 255],
                };
                pixels.extend_from_slice(&check_pixel);
                continue;
            }
            if matches!(
                document.selection.pixel(document_x, document_y).ok()?,
                PixelValue::Binary(255)
            ) {
                composite = blend_rgba_over(composite, [0, 160, 255, 64]);
            }
            let alpha = u32::from(composite[3]);
            let premultiply =
                |channel: u8| -> u8 { ((u32::from(channel) * alpha + 127) / 255) as u8 };
            pixels.extend_from_slice(&[
                premultiply(composite[2]),
                premultiply(composite[1]),
                premultiply(composite[0]),
                composite[3],
            ]);
        }
    }
    if pixels.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        return None;
    }
    Some(RenderTile {
        tile_id: (u64::from(coord.y) << 32) | u64::from(coord.x) | (1_u64 << 63),
        origin: DocumentPointI32 {
            x: origin_x as i32,
            y: origin_y as i32,
        },
        size: DocumentSizeU32::new(width, height),
        stride_bytes: stride,
        pixels: Arc::from(pixels),
        source_revision,
        tile_revision,
    })
}

pub(super) fn blend_rgba_over(background: [u8; 4], foreground: [u8; 4]) -> [u8; 4] {
    let foreground_alpha = u32::from(foreground[3]);
    let background_alpha = u32::from(background[3]);
    let inverse = 255 - foreground_alpha;
    let output_alpha = foreground_alpha + (background_alpha * inverse + 127) / 255;
    if output_alpha == 0 {
        return [0; 4];
    }
    let channel = |index: usize| -> u8 {
        let foreground_premultiplied = u32::from(foreground[index]) * foreground_alpha;
        let background_premultiplied = u32::from(background[index]) * background_alpha;
        ((foreground_premultiplied
            + (background_premultiplied * inverse + 127) / 255
            + output_alpha / 2)
            / output_alpha) as u8
    };
    [channel(0), channel(1), channel(2), output_alpha as u8]
}

pub(super) fn blend_rgba16_over(background: [u16; 4], foreground: [u16; 4]) -> [u16; 4] {
    let foreground_alpha = u64::from(foreground[3]);
    let background_alpha = u64::from(background[3]);
    let inverse = u64::from(u16::MAX) - foreground_alpha;
    let output_alpha =
        foreground_alpha + (background_alpha * inverse + 32_767) / u64::from(u16::MAX);
    if output_alpha == 0 {
        return [0; 4];
    }
    let channel = |index: usize| -> u16 {
        let foreground_premultiplied = u64::from(foreground[index]) * foreground_alpha;
        let background_premultiplied = u64::from(background[index]) * background_alpha;
        ((foreground_premultiplied
            + (background_premultiplied * inverse + 32_767) / u64::from(u16::MAX)
            + output_alpha / 2)
            / output_alpha) as u16
    };
    [channel(0), channel(1), channel(2), output_alpha as u16]
}

pub(super) fn rgba8_for_display(value: PixelValue) -> Option<[u8; 4]> {
    match value {
        PixelValue::Rgba(value) => Some(value),
        PixelValue::Rgba16(value) => {
            Some(value.map(|channel| ((u32::from(channel) + 128) / 257) as u8))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_stroke(x: f32, y: f32, color: [u8; 4]) -> Stroke {
        Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::Color,
            color,
            diameter: 1.0,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples: vec![StrokeSample {
                x,
                y,
                pressure: 1.0,
            }],
        }
    }

    #[test]
    fn dirty_tile_rebuild_reuses_untouched_revisions_and_separates_view_overlay() {
        let mut core = Core::new();
        core.new_cell(128, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_stroke(&color_stroke(1.0, 1.0, [10, 20, 30, 255]))
            .unwrap();
        let first = core.build_snapshot();
        assert_eq!(first.tiles().len(), 1);
        let first_revision = first.tiles()[0].tile_revision();
        assert_eq!(
            core.build_snapshot().tiles()[0].tile_revision(),
            first_revision
        );

        core.apply_stroke(&color_stroke(70.0, 1.0, [40, 50, 60, 255]))
            .unwrap();
        let second = core.build_snapshot();
        assert_eq!(second.tiles().len(), 2);
        assert_eq!(second.tiles()[0].tile_revision(), first_revision);
        let second_tile_revision = second.tiles()[1].tile_revision();

        let document_revision = core.document_info().unwrap().document_revision;
        core.apply_view(ViewCommand::SetGridVisible(true)).unwrap();
        let view_only = core.build_snapshot();
        assert_eq!(view_only.revision(), document_revision);
        assert_eq!(view_only.tiles()[0].tile_revision(), first_revision);
        assert_eq!(view_only.tiles()[1].tile_revision(), second_tile_revision);

        core.apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 70,
                y: 0,
                width: 1,
                height: 2,
            }),
            SelectionOperation::New,
        )
        .unwrap();
        let overlay = core.build_snapshot();
        assert_ne!(overlay.tiles()[0].tile_revision(), first_revision);
        assert_ne!(overlay.tiles()[1].tile_revision(), second_tile_revision);
    }
}
