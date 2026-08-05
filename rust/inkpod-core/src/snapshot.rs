//! Document inspection and immutable render snapshots.

use super::*;

/// One immutable raster tile positioned in document pixel coordinates.
///
/// `origin_x..origin_x + width` and `origin_y..origin_y + height` are half-open
/// document ranges. Device zoom, pan, view flip, and OS DPI are not applied to
/// tile geometry or pixels.
#[derive(Clone, Debug)]
pub struct RenderTile {
    tile_id: u64,
    origin: DocumentPointI32,
    size: DocumentSizeU32,
    stride_bytes: u32,
    pixels: Arc<[u8]>,
    source_revision: RenderRevision,
    tile_revision: RenderRevision,
}

impl PartialEq for RenderTile {
    fn eq(&self, other: &Self) -> bool {
        self.tile_id == other.tile_id
            && self.origin == other.origin
            && self.size == other.size
            && self.stride_bytes == other.stride_bytes
            && self.pixels == other.pixels
            && self.tile_revision == other.tile_revision
    }
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
        let mut coords: Vec<_> = document
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
                    coords.push(TileCoord { x, y });
                }
            }
        }
        coords.sort_unstable();
        coords.dedup();
        let mut tiles = Vec::with_capacity(coords.len());
        for coord in &coords {
            let source_revision = revision_max_tile_source_revision(document, *coord);
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
        cache.retain(|coord, _| coords.binary_search(coord).is_ok());
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

    /// Builds an immutable read-only snapshot of the registered subpalette cell.
    ///
    /// The source raster is never installed as the editable document. The supplied
    /// secondary view contributes only its independent zoom, pan, flip, and viewport
    /// state; document revisions, history, dirty state, and render cache are unchanged.
    pub fn build_subpalette_snapshot_for(&self, view_id: u64) -> Result<RenderSnapshot, CoreError> {
        let view = *self
            .secondary_views
            .get(&ViewId::from_raw(view_id))
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))?;
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let index = self
            .subpalette_index
            .ok_or(CoreError::InvalidState("subpalette has no registered cell"))?;
        let cell = sequence
            .cells
            .get(index)
            .ok_or(CoreError::InvalidState("subpalette source disappeared"))?;
        let raster = &cell.raster;
        let mut tiles = Vec::new();
        for coord in raster.allocated_coords() {
            let source_revision = RenderRevision::from_raw(raster.tile_revision(coord));
            if let Some(tile) =
                compose_reference_tile(raster, coord, source_revision, source_revision)
            {
                tiles.push(tile);
            }
        }
        Ok(RenderSnapshot {
            revision: RenderRevision::from_raw(raster.checksum()),
            feature_flags: 0,
            view,
            document_size: DocumentSizeU32::new(raster.width(), raster.height()),
            guides: Vec::new(),
            grid: GridConfig::default(),
            tiles,
            vector_segments: Vec::new(),
            vector_fills: Vec::new(),
        })
    }
}

/// Returns the canonical revision-max cache identity without reading raster payloads.
///
/// This deliberately preserves the original performance contract: only scalar
/// revisions of visible plane tiles, the selection tile, and the Light Table
/// source are inspected while validating a cached composition.
fn revision_max_tile_source_revision(document: &CellDocument, coord: TileCoord) -> RenderRevision {
    let source_revision = document
        .layers
        .iter()
        .filter(|layer| layer.visible)
        .flat_map(|layer| layer.planes.iter())
        .filter(|plane| plane.visible)
        .map(|plane| plane.raster.tile_revision(coord))
        .max()
        .unwrap_or(0)
        .max(document.light_table.source_revision())
        .max(document.selection.tile_revision(coord));
    RenderRevision::from_raw(source_revision)
}

// Shared implementation helpers for this responsibility.

#[cfg(test)]
std::thread_local! {
    static SNAPSHOT_PAYLOAD_ACCESS_COUNT: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

#[inline(always)]
fn note_snapshot_payload_access() {
    #[cfg(test)]
    SNAPSHOT_PAYLOAD_ACCESS_COUNT.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
fn reset_snapshot_payload_access_count() {
    SNAPSHOT_PAYLOAD_ACCESS_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
fn snapshot_payload_access_count() -> usize {
    SNAPSHOT_PAYLOAD_ACCESS_COUNT.with(std::cell::Cell::get)
}

#[derive(Clone, Copy)]
enum PreparedPlaneKind {
    MainLine8([u8; 4]),
    MainLine16([u8; 4]),
    Color8,
    Color16,
    Selection8,
}

#[derive(Clone, Copy)]
struct PreparedPlaneTile<'a> {
    kind: PreparedPlaneKind,
    opacity_milli: u32,
    tile: TileView<'a>,
}

impl PreparedPlaneTile<'_> {
    fn rgba(self, local_x: u32, local_y: u32) -> [u8; 4] {
        let bytes = self.tile.bytes();
        let row = local_y as usize * self.tile.row_stride_bytes() as usize;
        match self.kind {
            PreparedPlaneKind::MainLine8(mut color) => {
                let coverage = bytes[row + local_x as usize];
                color[3] = ((u32::from(color[3]) * u32::from(coverage) + 127) / 255) as u8;
                color
            }
            PreparedPlaneKind::MainLine16(mut color) => {
                let offset = row + local_x as usize * 2;
                let coverage = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
                let coverage = ((u32::from(coverage) + 128) / 257) as u8;
                color[3] = ((u32::from(color[3]) * u32::from(coverage) + 127) / 255) as u8;
                color
            }
            PreparedPlaneKind::Color8 => {
                let offset = row + local_x as usize * 4;
                [
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]
            }
            PreparedPlaneKind::Color16 => {
                let offset = row + local_x as usize * 8;
                std::array::from_fn(|channel| {
                    let start = offset + channel * 2;
                    let value = u16::from_le_bytes([bytes[start], bytes[start + 1]]);
                    ((u32::from(value) + 128) / 257) as u8
                })
            }
            PreparedPlaneKind::Selection8 => {
                let coverage = bytes[row + local_x as usize];
                [0, 160, 255, coverage / 3]
            }
        }
    }
}

struct PreparedLayer<'a> {
    opacity_milli: u32,
    adjustment: Option<&'a Adjustment>,
    planes: Vec<PreparedPlaneTile<'a>>,
}

fn raster_covers_tile_rect(
    raster: &TileRaster,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
) -> bool {
    origin_x
        .checked_add(width)
        .is_some_and(|right| right <= raster.width())
        && origin_y
            .checked_add(height)
            .is_some_and(|bottom| bottom <= raster.height())
}

fn prepare_layers_for_tile<'a>(
    document: &'a CellDocument,
    coord: TileCoord,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
) -> Option<Vec<PreparedLayer<'a>>> {
    let mut layers = Vec::with_capacity(document.layers.len());
    // Layer index zero is the top of the palette. Resolve tile storage once in
    // bottom-to-top order so the pixel loop performs neither map lookups nor
    // document-to-tile division.
    for layer in document.layers.iter().rev().filter(|layer| layer.visible) {
        if let Some(adjustment) = document.adjustments.get(&layer.id) {
            layers.push(PreparedLayer {
                opacity_milli: layer.opacity_milli,
                adjustment: Some(adjustment),
                planes: Vec::new(),
            });
            continue;
        }

        let mut planes = Vec::with_capacity(layer.planes.len());
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
            if !raster_covers_tile_rect(&plane.raster, origin_x, origin_y, width, height) {
                return None;
            }
            note_snapshot_payload_access();
            let tile = plane.raster.tile_view(coord);
            let kind = match (plane.kind, plane.raster.format()) {
                (PlaneType::MainLine, PixelFormat::BinaryMask8 | PixelFormat::Grayscale8) => Some(
                    PreparedPlaneKind::MainLine8(rgba8_for_display(document.main_line_color)?),
                ),
                (PlaneType::MainLine, PixelFormat::Grayscale16) => Some(
                    PreparedPlaneKind::MainLine16(rgba8_for_display(document.main_line_color)?),
                ),
                (
                    PlaneType::Color | PlaneType::Raster,
                    PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8,
                ) => Some(PreparedPlaneKind::Color8),
                (PlaneType::Color | PlaneType::Raster, PixelFormat::StraightRgba16) => {
                    Some(PreparedPlaneKind::Color16)
                }
                (PlaneType::Selection, PixelFormat::BinaryMask8) => {
                    Some(PreparedPlaneKind::Selection8)
                }
                (PlaneType::VectorMainLine | PlaneType::ColorTrace | PlaneType::VectorFill, _) => {
                    None
                }
                _ => return None,
            };
            let (Some(kind), Some(tile)) = (kind, tile) else {
                continue;
            };
            if plane.opacity_milli != 0 {
                planes.push(PreparedPlaneTile {
                    kind,
                    opacity_milli: plane.opacity_milli,
                    tile,
                });
            }
        }
        layers.push(PreparedLayer {
            opacity_milli: layer.opacity_milli,
            adjustment: None,
            planes,
        });
    }
    Some(layers)
}

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
    let layers = prepare_layers_for_tile(document, coord, origin_x, origin_y, width, height)?;
    let has_light_table = document.light_table.has_visible_items();
    let selection_tile = if !alpha_view && color_check.is_none() {
        if !raster_covers_tile_rect(&document.selection, origin_x, origin_y, width, height) {
            return None;
        }
        note_snapshot_payload_access();
        let tile = document.selection.tile_view(coord);
        (document.selection.format() == PixelFormat::BinaryMask8)
            .then_some(tile)
            .flatten()
    } else {
        None
    };
    let mut pixels = Vec::with_capacity(capacity);
    for y in 0..height {
        for x in 0..width {
            let document_x = origin_x + x;
            let document_y = origin_y + y;
            let mut composite = if has_light_table {
                note_snapshot_payload_access();
                document
                    .light_table
                    .composite(document.frames.reference_frame, document_x, document_y)
                    .unwrap_or([0_u8; 4])
            } else {
                [0_u8; 4]
            };
            for layer in &layers {
                if let Some(adjustment) = layer.adjustment {
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
                for plane in &layer.planes {
                    let mut rgba = plane.rgba(x, y);
                    if plane.opacity_milli != 1_000 {
                        rgba[3] = ((u32::from(rgba[3]) * plane.opacity_milli + 500) / 1_000) as u8;
                    }
                    layer_pixel = blend_rgba_over(layer_pixel, rgba);
                }
                if layer.opacity_milli != 1_000 {
                    layer_pixel[3] =
                        ((u32::from(layer_pixel[3]) * layer.opacity_milli + 500) / 1_000) as u8;
                }
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
            if selection_tile.is_some_and(|tile| {
                tile.bytes()[y as usize * tile.row_stride_bytes() as usize + x as usize] == 255
            }) {
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

fn compose_reference_tile(
    raster: &TileRaster,
    coord: TileCoord,
    source_revision: RenderRevision,
    tile_revision: RenderRevision,
) -> Option<RenderTile> {
    let origin_x = coord.x.checked_mul(TILE_SIZE)?;
    let origin_y = coord.y.checked_mul(TILE_SIZE)?;
    if origin_x >= raster.width() || origin_y >= raster.height() {
        return None;
    }
    let width = TILE_SIZE.min(raster.width() - origin_x);
    let height = TILE_SIZE.min(raster.height() - origin_y);
    let stride = width.checked_mul(4)?;
    let capacity = usize::try_from(stride.checked_mul(height)?).ok()?;
    let mut pixels = Vec::with_capacity(capacity);
    for y in 0..height {
        for x in 0..width {
            note_snapshot_payload_access();
            let rgba = rgba8_for_display(raster.pixel(origin_x + x, origin_y + y).ok()?)?;
            let alpha = u32::from(rgba[3]);
            let premultiply = |channel: u8| ((u32::from(channel) * alpha + 127) / 255) as u8;
            pixels.extend_from_slice(&[
                premultiply(rgba[2]),
                premultiply(rgba[1]),
                premultiply(rgba[0]),
                rgba[3],
            ]);
        }
    }
    if pixels.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        return None;
    }
    Some(RenderTile {
        tile_id: (u64::from(coord.y) << 32) | u64::from(coord.x) | (1_u64 << 62),
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
    if foreground_alpha == 0 {
        return if background_alpha == 0 {
            [0; 4]
        } else {
            background
        };
    }
    if foreground_alpha == 255 || background_alpha == 0 {
        return foreground;
    }
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

    fn blend_rgba_over_reference(background: [u8; 4], foreground: [u8; 4]) -> [u8; 4] {
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

    fn legacy_single_plane_composite(
        document: &CellDocument,
        plane: &PlaneNode,
        x: u32,
        y: u32,
    ) -> [u8; 4] {
        let value = plane.raster.pixel(x, y).unwrap();
        let mut rgba = match plane.kind {
            PlaneType::MainLine => {
                let coverage = match value {
                    PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value,
                    PixelValue::Grayscale16(value) => ((u32::from(value) + 128) / 257) as u8,
                    _ => panic!("test fixture uses an invalid main-line format"),
                };
                let mut line = rgba8_for_display(document.main_line_color).unwrap();
                line[3] = ((u32::from(line[3]) * u32::from(coverage) + 127) / 255) as u8;
                line
            }
            PlaneType::Color | PlaneType::Raster => rgba8_for_display(value).unwrap(),
            PlaneType::Selection => {
                let PixelValue::Binary(coverage) = value else {
                    panic!("test fixture uses an invalid selection format");
                };
                [0, 160, 255, coverage / 3]
            }
            PlaneType::VectorMainLine | PlaneType::ColorTrace | PlaneType::VectorFill => [0; 4],
        };
        rgba[3] = ((u32::from(rgba[3]) * plane.opacity_milli + 500) / 1_000) as u8;
        let mut layer_pixel = blend_rgba_over_reference([0; 4], rgba);
        let layer = &document.layers[0];
        layer_pixel[3] = ((u32::from(layer_pixel[3]) * layer.opacity_milli + 500) / 1_000) as u8;
        blend_rgba_over_reference([0; 4], layer_pixel)
    }

    fn expected_snapshot_pixel(
        mut composite: [u8; 4],
        color_check: Option<ColorCheckMode>,
        alpha_view: bool,
        selected: bool,
    ) -> [u8; 4] {
        if alpha_view {
            let alpha = composite[3];
            return [alpha, alpha, alpha, 255];
        }
        if let Some(mode) = color_check {
            return match color_check_category(PixelValue::Rgba(composite), mode) {
                ColorCheckCategory::ExactWhite => [255, 255, 255, 255],
                ColorCheckCategory::Transparent => [255, 0, 255, 255],
                ColorCheckCategory::Colored => [0, 0, 0, 255],
            };
        }
        if selected {
            composite = blend_rgba_over_reference(composite, [0, 160, 255, 64]);
        }
        let alpha = u32::from(composite[3]);
        let premultiply = |channel: u8| ((u32::from(channel) * alpha + 127) / 255) as u8;
        [
            premultiply(composite[2]),
            premultiply(composite[1]),
            premultiply(composite[0]),
            composite[3],
        ]
    }

    #[test]
    fn blend_fast_paths_are_bit_exact_with_the_original_formula() {
        for background_alpha in 0..=u8::MAX {
            for foreground_alpha in 0..=u8::MAX {
                let background = [17, 93, 241, background_alpha];
                let foreground = [250, 33, 71, foreground_alpha];
                assert_eq!(
                    blend_rgba_over(background, foreground),
                    blend_rgba_over_reference(background, foreground),
                    "alpha pair ({background_alpha}, {foreground_alpha})"
                );
            }
        }
    }

    #[test]
    fn prepared_tile_composition_matches_pixel_reader_for_formats_edges_and_modes() {
        let cases = [
            (
                PlaneType::MainLine,
                PixelFormat::BinaryMask8,
                PixelValue::Binary(255),
            ),
            (
                PlaneType::MainLine,
                PixelFormat::Grayscale8,
                PixelValue::Grayscale8(173),
            ),
            (
                PlaneType::MainLine,
                PixelFormat::Grayscale16,
                PixelValue::Grayscale16(40_000),
            ),
            (
                PlaneType::Color,
                PixelFormat::StraightRgba8,
                PixelValue::Rgba([12, 34, 56, 177]),
            ),
            (
                PlaneType::Raster,
                PixelFormat::PremultipliedBgra8,
                PixelValue::Rgba([11, 22, 33, 144]),
            ),
            (
                PlaneType::Color,
                PixelFormat::StraightRgba16,
                PixelValue::Rgba16([1_000, 20_000, 50_000, 40_000]),
            ),
            (
                PlaneType::Selection,
                PixelFormat::BinaryMask8,
                PixelValue::Binary(255),
            ),
        ];
        let modes = [
            (None, false),
            (None, true),
            (Some(ColorCheckMode::LegacyWhiteTransparency), false),
            (Some(ColorCheckMode::NativeAlpha), false),
        ];

        for (kind, format, value) in cases {
            let mut core = Core::new();
            core.new_cell(65, 66, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
                .unwrap();
            let document = core.document.as_mut().unwrap();
            document.main_line_color = PixelValue::Rgba([20, 40, 60, 200]);
            document.layers[0].opacity_milli = 631;
            let mut raster = TileRaster::new(65, 66, format).unwrap();
            raster.set_pixel(64, 65, value, 17).unwrap();
            let mut plane = document.layers[0].planes[0].clone();
            plane.kind = kind;
            plane.opacity_milli = 777;
            plane.raster = raster;
            document.layers[0].planes = vec![plane];
            document
                .selection
                .set_pixel(64, 65, PixelValue::Binary(255), 18)
                .unwrap();

            let expected_composite =
                legacy_single_plane_composite(document, &document.layers[0].planes[0], 64, 65);
            for (color_check, alpha_view) in modes {
                let tile = compose_tile(
                    document,
                    TileCoord { x: 1, y: 1 },
                    color_check,
                    alpha_view,
                    RenderRevision::from_raw(17),
                    RenderRevision::from_raw(19),
                )
                .unwrap();
                assert_eq!((tile.width(), tile.height()), (1, 2));
                let second_row = tile.stride_bytes() as usize;
                assert_eq!(
                    &tile.pixels()[second_row..second_row + 4],
                    &expected_snapshot_pixel(expected_composite, color_check, alpha_view, true,),
                    "kind={kind:?}, format={format:?}, color_check={color_check:?}, alpha_view={alpha_view}"
                );
            }
        }
    }

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

    #[test]
    fn revision_max_identity_matches_the_canonical_scalar_formula() {
        let mut core = Core::new();
        core.new_cell(128, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_stroke(&color_stroke(1.0, 1.0, [10, 20, 30, 255]))
            .unwrap();
        core.apply_stroke(&color_stroke(70.0, 1.0, [40, 50, 60, 255]))
            .unwrap();
        core.apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 70,
                y: 0,
                width: 1,
                height: 1,
            }),
            SelectionOperation::New,
        )
        .unwrap();

        let document = core.document.as_ref().unwrap();
        for coord in [TileCoord { x: 0, y: 0 }, TileCoord { x: 1, y: 0 }] {
            let expected = document
                .layers
                .iter()
                .filter(|layer| layer.visible)
                .flat_map(|layer| layer.planes.iter())
                .filter(|plane| plane.visible)
                .map(|plane| plane.raster.tile_revision(coord))
                .max()
                .unwrap_or(0)
                .max(document.light_table.source_revision())
                .max(document.selection.tile_revision(coord));
            assert_eq!(
                revision_max_tile_source_revision(document, coord).get(),
                expected
            );
        }
    }

    #[test]
    fn revision_max_cache_validation_reads_only_scalar_revisions() {
        let source = include_str!("snapshot.rs");
        let build_snapshot_body = source
            .split_once("pub fn build_snapshot(&mut self) -> RenderSnapshot {")
            .expect("primary snapshot builder must remain explicit")
            .1
            .split_once("/// Builds an immutable read-only snapshot")
            .expect("secondary snapshot builder must follow the primary builder")
            .0;
        let helper_body = source
            .split_once("fn revision_max_tile_source_revision(")
            .expect("revision-max helper must remain explicit")
            .1
            .split_once("// Shared implementation helpers")
            .expect("composition helpers must follow the revision-max helper")
            .0;

        assert!(helper_body.contains("plane.raster.tile_revision(coord)"));
        assert!(helper_body.contains("document.light_table.source_revision()"));
        assert!(helper_body.contains("document.selection.tile_revision(coord)"));
        assert!(helper_body.matches(".max(").count() >= 3);
        assert!(build_snapshot_body.contains("revision_max_tile_source_revision"));
        for validation_source in [build_snapshot_body, helper_body] {
            for forbidden in [
                "blake3",
                "tile_data(",
                ".tile_view(",
                ".pixel(",
                ".pixels(",
                "checksum(",
                "tile_cache_state",
                "digest",
                "generation",
                "tombstone",
            ] {
                assert!(
                    !validation_source.contains(forbidden),
                    "revision-max validation must not contain {forbidden}"
                );
            }
        }
        let validation_call_graph =
            format!("{build_snapshot_body}\n{helper_body}").replace("\r\n", "\n");
        assert_eq!(
            blake3::hash(validation_call_graph.as_bytes())
                .to_hex()
                .to_string(),
            "00bc9e15811dcd06282b69d24e95aa8653f3169bd79f18844100116b4d0039eb",
            "primary snapshot validation call graph changed; audit payload/hash access before updating this lock"
        );
    }

    #[test]
    fn repeated_wheel_zoom_snapshots_do_not_recompose_cached_tiles() {
        let mut core = Core::new();
        core.new_cell(128, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_stroke(&color_stroke(1.0, 1.0, [10, 20, 30, 255]))
            .unwrap();
        core.apply_stroke(&color_stroke(70.0, 1.0, [40, 50, 60, 255]))
            .unwrap();
        reset_snapshot_payload_access_count();
        let initial = core.build_snapshot();
        assert!(snapshot_payload_access_count() > 0);
        let initial_revisions: Vec<_> = initial
            .tiles()
            .iter()
            .map(RenderTile::tile_revision)
            .collect();
        let next_render_revision = core.next_render_tile_revision;
        reset_snapshot_payload_access_count();

        for step in 0..128 {
            core.apply_view(ViewCommand::ZoomAt {
                factor: if step % 2 == 0 { 1.01 } else { 1.0 / 1.01 },
                device_x: 0.5,
                device_y: 0.5,
            })
            .unwrap();
            let snapshot = core.build_snapshot();
            assert_eq!(
                snapshot
                    .tiles()
                    .iter()
                    .map(RenderTile::tile_revision)
                    .collect::<Vec<_>>(),
                initial_revisions
            );
        }
        assert_eq!(core.next_render_tile_revision, next_render_revision);
        assert_eq!(snapshot_payload_access_count(), 0);
    }

    #[test]
    fn vector_coord_collection_deduplicates_and_preserves_tilecoord_order() {
        let mut core = Core::new();
        core.new_cell(130, 65, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_stroke(&color_stroke(1.0, 1.0, [10, 20, 30, 255]))
            .unwrap();
        core.apply_view(ViewCommand::SetAlphaView(true)).unwrap();

        let snapshot = core.build_snapshot();
        assert_eq!(snapshot.tile_count(), 6);
        assert_eq!(
            snapshot
                .tiles()
                .iter()
                .map(|tile| (tile.origin_x(), tile.origin_y()))
                .collect::<Vec<_>>(),
            vec![(0, 0), (0, 64), (64, 0), (64, 64), (128, 0), (128, 64),]
        );
    }
}
