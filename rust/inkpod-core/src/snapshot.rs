//! Document inspection and immutable render snapshots.

use super::*;
use inkpod_image::{source_over_rgba8, source_over_rgba16};

const COMPOSITE_DIGEST_CONTEXT: &str = "org.inkpod.digest.canonical-composite.v4";

/// Architecture-independent digest of one snapshot's document result.
///
/// View-only state, transient revision numbers, and cache revisions are excluded.
/// Raster pixels are hashed as their public premultiplied BGRA8 tile stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalCompositeDigest([u8; 32]);

impl CanonicalCompositeDigest {
    /// Returns the canonical 32-byte digest.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

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

/// Closed kind of one ordered immutable render pass.
///
/// Passes are stored in execution order from the bottom of the document toward
/// palette index zero. A layer begin/end pair scopes one layer's opacity so
/// overlapping child content is attenuated exactly once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderPassKind {
    /// Begins one visible logical layer group.
    LayerBegin,
    /// Draws a contiguous span of premultiplied raster tiles.
    RasterTiles,
    /// Applies one Core-resolved RGB lookup table to the accumulated result.
    Adjustment,
    /// Ends the current logical layer group.
    LayerEnd,
}

/// One immutable entry in a snapshot's ordered render plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderPass {
    kind: RenderPassKind,
    layer_id: u64,
    plane_id: u64,
    opacity_milli: u32,
    first_item: u64,
    item_count: u64,
}

impl RenderPass {
    /// Returns the closed pass kind.
    #[must_use]
    pub const fn kind(&self) -> RenderPassKind {
        self.kind
    }

    /// Returns the stable source layer ID, or zero for document-level content.
    #[must_use]
    pub const fn layer_id(&self) -> u64 {
        self.layer_id
    }

    /// Returns the stable source plane ID, or zero for group/adjustment passes.
    #[must_use]
    pub const fn plane_id(&self) -> u64 {
        self.plane_id
    }

    /// Returns layer opacity for `LayerBegin`; other pass kinds report 1000.
    #[must_use]
    pub const fn opacity_milli(&self) -> u32 {
        self.opacity_milli
    }

    /// Returns the first item in the kind-specific snapshot span.
    #[must_use]
    pub const fn first_item(&self) -> u64 {
        self.first_item
    }

    /// Returns the number of items in the kind-specific snapshot span.
    #[must_use]
    pub const fn item_count(&self) -> u64 {
        self.item_count
    }
}

/// Three exact 8-bit display lookup tables resolved by the Core for one
/// adjustment pass. Alpha is preserved by adjustment rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderAdjustmentLut {
    channels: [[u8; 256]; 3],
}

impl RenderAdjustmentLut {
    /// Borrows red, green, and blue lookup tables in that order.
    #[must_use]
    pub const fn channels(&self) -> &[[u8; 256]; 3] {
        &self.channels
    }
}

/// Immutable document render data with a separate device-pixel view transform.
///
/// Raster tile origins, guides, and grid values are all
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
    render_passes: Vec<RenderPass>,
    adjustment_luts: Vec<RenderAdjustmentLut>,
    shooting_frames: Vec<ShootingFrameInfo>,
    vanishing_points: Vec<VanishingPointInfo>,
    radial_guides: Vec<RenderRadialGuide>,
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

    /// Borrows the immutable bottom-to-top render plan.
    #[must_use]
    pub fn render_passes(&self) -> &[RenderPass] {
        &self.render_passes
    }

    /// Borrows Core-resolved adjustment lookup tables referenced by passes.
    #[must_use]
    pub fn adjustment_luts(&self) -> &[RenderAdjustmentLut] {
        &self.adjustment_luts
    }

    /// Borrows the optional visible angled shooting-frame instruction overlay.
    #[must_use]
    pub fn shooting_frames(&self) -> &[ShootingFrameInfo] {
        &self.shooting_frames
    }

    /// Borrows visible vanishing-point handles captured by this snapshot.
    #[must_use]
    pub fn vanishing_points(&self) -> &[VanishingPointInfo] {
        &self.vanishing_points
    }

    /// Borrows viewport-clipped radial guide segments captured by this snapshot.
    #[must_use]
    pub fn radial_guides(&self) -> &[RenderRadialGuide] {
        &self.radial_guides
    }

    /// Computes the canonical document-result digest for this immutable snapshot.
    ///
    /// This operation is deterministic across supported architectures and does
    /// not alter revision, history, dirty state, or ownership. It returns an
    /// error only if internally produced snapshot state is invalid.
    pub fn canonical_composite_digest(&self) -> Result<CanonicalCompositeDigest, CoreError> {
        let mut hasher = blake3::Hasher::new_derive_key(COMPOSITE_DIGEST_CONTEXT);
        hasher.update(&4_u32.to_le_bytes());
        hasher.update(&self.feature_flags.to_le_bytes());
        hasher.update(&self.document_size.width.to_le_bytes());
        hasher.update(&self.document_size.height.to_le_bytes());
        hasher.update(&(self.tiles.len() as u64).to_le_bytes());
        for tile in &self.tiles {
            hasher.update(&tile.tile_id.to_le_bytes());
            hasher.update(&tile.origin.x.to_le_bytes());
            hasher.update(&tile.origin.y.to_le_bytes());
            hasher.update(&tile.size.width.to_le_bytes());
            hasher.update(&tile.size.height.to_le_bytes());
            hasher.update(&tile.stride_bytes.to_le_bytes());
            hasher.update(&(tile.pixels.len() as u64).to_le_bytes());
            hasher.update(&tile.pixels);
        }
        hasher.update(&(self.render_passes.len() as u64).to_le_bytes());
        for pass in &self.render_passes {
            let kind = match pass.kind {
                RenderPassKind::LayerBegin => 1_u32,
                RenderPassKind::RasterTiles => 2,
                RenderPassKind::Adjustment => 3,
                RenderPassKind::LayerEnd => 4,
            };
            hasher.update(&kind.to_le_bytes());
            hasher.update(&pass.layer_id.to_le_bytes());
            hasher.update(&pass.plane_id.to_le_bytes());
            hasher.update(&pass.opacity_milli.to_le_bytes());
            hasher.update(&pass.first_item.to_le_bytes());
            hasher.update(&pass.item_count.to_le_bytes());
        }
        hasher.update(&(self.adjustment_luts.len() as u64).to_le_bytes());
        for lut in &self.adjustment_luts {
            for channel in &lut.channels {
                hasher.update(channel);
            }
        }
        Ok(CanonicalCompositeDigest(*hasher.finalize().as_bytes()))
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
            cell_id: document.cell_id.get(),
            document_uuid: document.uuid,
            layer_id: layer_id.get(),
            main_plane_id: main_plane_id.get(),
            color_plane_id: color_plane_id.get(),
            width: document.width,
            height: document.height,
            dpi_x_milli: document.dpi_x_milli,
            dpi_y_milli: document.dpi_y_milli,
            frames: document.frames,
            dirty: self.savepoint != Some(self.current_state) || self.editor_dirty(),
            can_undo: self.history_cursor > 0,
            can_redo: self.history_cursor < self.history.len(),
            active_plane: document.active_plane_role(
                self.editor_session
                    .as_ref()
                    .and_then(|session| session.state.target)
                    .map(|target| PlaneId::from_raw(target.plane_id)),
            ),
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
                self.shooting_frame_preview
                    .as_ref()
                    .map(|session| &session.preview_document)
            })
            .or_else(|| {
                self.vanishing_point_preview
                    .as_ref()
                    .map(|session| &session.preview_document)
            })
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
                render_passes: Vec::new(),
                adjustment_luts: Vec::new(),
                shooting_frames: Vec::new(),
                vanishing_points: Vec::new(),
                radial_guides: Vec::new(),
            };
        };
        let snapshot_revision = self
            .active_stroke
            .as_ref()
            .map(|session| RenderRevision::from_raw(session.preview_revision.get()))
            .or_else(|| {
                self.shooting_frame_preview
                    .as_ref()
                    .map(|session| RenderRevision::from_raw(session.preview_revision.get()))
            })
            .or_else(|| {
                self.vanishing_point_preview
                    .as_ref()
                    .map(|session| RenderRevision::from_raw(session.preview_revision.get()))
            })
            .or_else(|| {
                self.filter_preview
                    .as_ref()
                    .map(|session| RenderRevision::from_raw(session.preview_revision.get()))
            })
            .unwrap_or_else(|| RenderRevision::from_raw(self.document_revision.get()));
        let mut feature_flags = match self.color_check {
            Some(ColorCheckMode::LegacyWhiteTransparency) => {
                SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE
            }
            Some(ColorCheckMode::NativeAlpha) => SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA,
            None => 0,
        };
        if document.base_surface == BaseSurface::SolidWhite {
            feature_flags |= SNAPSHOT_FEATURE_SOLID_WHITE_BASE;
        }
        let base_asset = match document.base_surface {
            BaseSurface::SolidWhite => None,
            BaseSurface::Asset(id) => self.assets.get(id),
        };
        let base_raster = base_asset.as_ref().and_then(|asset| asset.raster());
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
        if let Some(base_raster) = base_raster {
            let unallocated_pixels_are_visible = matches!(
                base_raster.format(),
                PixelFormat::Grayscale8 | PixelFormat::Grayscale16
            );
            if !unallocated_pixels_are_visible
                && !self.view.alpha_view
                && self.color_check.is_none()
            {
                coords.extend(base_raster.allocated_coords());
            } else {
                let tiles_x = document.width.div_ceil(TILE_SIZE);
                let tiles_y = document.height.div_ceil(TILE_SIZE);
                for y in 0..tiles_y {
                    for x in 0..tiles_x {
                        coords.push(TileCoord { x, y });
                    }
                }
            }
        }
        coords.sort_unstable();
        coords.dedup();
        let mut tiles = Vec::with_capacity(coords.len());
        for coord in &coords {
            let source_revision = revision_max_tile_source_revision(document, *coord);
            let cache_key = (0, *coord);
            if cache
                .get(&cache_key)
                .is_none_or(|tile| tile.source_revision != source_revision)
            {
                let tile_revision = self.next_render_tile_revision;
                self.next_render_tile_revision =
                    self.next_render_tile_revision.wrapping_next_nonzero();
                if let Some(tile) = compose_tile(
                    document,
                    base_raster.map(Arc::as_ref),
                    *coord,
                    self.color_check,
                    self.view.alpha_view,
                    source_revision,
                    tile_revision,
                ) {
                    cache.insert(cache_key, tile);
                } else {
                    cache.remove(&cache_key);
                }
            }
            if let Some(tile) = cache.get(&cache_key) {
                tiles.push(tile.clone());
            }
        }
        cache.retain(|(band, coord), _| *band == 0 && coords.binary_search(coord).is_ok());
        let document_size = DocumentSizeU32::new(document.width, document.height);
        self.render_cache = cache;
        let render_passes = (!tiles.is_empty())
            .then_some(RenderPass {
                kind: RenderPassKind::RasterTiles,
                layer_id: 0,
                plane_id: 0,
                opacity_milli: 1_000,
                first_item: 0,
                item_count: tiles.len() as u64,
            })
            .into_iter()
            .collect();
        RenderSnapshot {
            revision: snapshot_revision,
            feature_flags,
            view: self.view,
            document_size,
            guides: document.guides.clone(),
            grid: document.grid,
            tiles,
            render_passes,
            adjustment_luts: Vec::new(),
            shooting_frames: document
                .shooting_frame
                .filter(|frame| frame.input.visible)
                .map(|frame| vec![frame.info()])
                .unwrap_or_default(),
            vanishing_points: visible_vanishing_point_infos(document),
            radial_guides: build_radial_guides(document, self.view),
        }
    }

    /// Builds an immutable read-only snapshot of the registered subpalette cell.
    ///
    /// The source raster is never installed as the editable document. The supplied
    /// secondary view contributes only its independent zoom, pan, flip, and viewport
    /// state; document revisions, history, and dirty state are unchanged. Reference tiles may be
    /// added to the private Core render cache so repeated item selection reuses stable tile IDs and
    /// revisions.
    pub fn build_subpalette_snapshot_for(
        &mut self,
        view_id: u64,
    ) -> Result<RenderSnapshot, CoreError> {
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
        let cache_key = (1_u64 << 62) | index as u64;
        for coord in raster.allocated_coords() {
            let source_revision = RenderRevision::from_raw(raster.tile_revision(coord));
            if self
                .render_cache
                .get(&(cache_key, coord))
                .is_none_or(|tile| tile.source_revision != source_revision)
            {
                let tile_revision = self.next_render_tile_revision;
                self.next_render_tile_revision =
                    self.next_render_tile_revision.wrapping_next_nonzero();
                if let Some(tile_id) = subpalette_reference_tile_id(index, coord)
                    && let Some(tile) = compose_reference_tile(
                        raster,
                        coord,
                        source_revision,
                        tile_revision,
                        tile_id,
                    )
                {
                    self.render_cache.insert((cache_key, coord), tile);
                }
            }
            if let Some(tile) = self.render_cache.get(&(cache_key, coord)) {
                tiles.push(tile.clone());
            }
        }
        let tile_count = tiles.len() as u64;
        Ok(RenderSnapshot {
            revision: RenderRevision::from_raw(raster.checksum()),
            feature_flags: 0,
            view,
            document_size: DocumentSizeU32::new(raster.width(), raster.height()),
            guides: Vec::new(),
            grid: GridConfig::default(),
            tiles,
            render_passes: vec![RenderPass {
                kind: RenderPassKind::RasterTiles,
                layer_id: 0,
                plane_id: 0,
                opacity_milli: 1_000,
                first_item: 0,
                item_count: tile_count,
            }],
            adjustment_luts: Vec::new(),
            shooting_frames: Vec::new(),
            vanishing_points: Vec::new(),
            radial_guides: Vec::new(),
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

type OrderedContent = (Vec<RenderTile>, Vec<RenderPass>, Vec<RenderAdjustmentLut>);

#[allow(
    dead_code,
    reason = "retained for ordered raster adjustment validation"
)]
fn build_ordered_content(
    document: &CellDocument,
    base_raster: Option<&TileRaster>,
    cache: &mut BTreeMap<(u64, TileCoord), RenderTile>,
    next_tile_revision: &mut RenderRevision,
) -> OrderedContent {
    const BACKGROUND_CACHE_KEY: u64 = u64::MAX;
    const SELECTION_CACHE_KEY: u64 = u64::MAX - 1;
    let mut tiles = Vec::new();
    let mut passes = Vec::new();
    let mut adjustment_luts = Vec::new();
    let mut active_cache_keys = BTreeSet::new();
    let mut raster_pass_index = 0_u32;

    let mut background_coords = Vec::new();
    if let Some(raster) = base_raster {
        if matches!(
            raster.format(),
            PixelFormat::Grayscale8 | PixelFormat::Grayscale16
        ) {
            for y in 0..document.height.div_ceil(TILE_SIZE) {
                for x in 0..document.width.div_ceil(TILE_SIZE) {
                    background_coords.push(TileCoord { x, y });
                }
            }
        } else {
            background_coords.extend(raster.allocated_coords());
        }
    }
    if document.light_table.has_visible_items() {
        for y in 0..document.height.div_ceil(TILE_SIZE) {
            for x in 0..document.width.div_ceil(TILE_SIZE) {
                background_coords.push(TileCoord { x, y });
            }
        }
    }
    background_coords.sort_unstable();
    background_coords.dedup();
    if !background_coords.is_empty() {
        let first = tiles.len() as u64;
        for coord in background_coords {
            let cache_key = (BACKGROUND_CACHE_KEY, coord);
            active_cache_keys.insert(cache_key);
            let source_revision = revision_max_tile_source_revision(document, coord);
            if cache
                .get(&cache_key)
                .is_none_or(|tile| tile.source_revision != source_revision)
            {
                let tile_revision = *next_tile_revision;
                *next_tile_revision = next_tile_revision.wrapping_next_nonzero();
                if let Some(tile) = compose_background_tile(
                    document,
                    base_raster,
                    coord,
                    source_revision,
                    tile_revision,
                    ordered_tile_id(raster_pass_index, coord),
                ) {
                    cache.insert(cache_key, tile);
                } else {
                    cache.remove(&cache_key);
                }
            }
            if let Some(tile) = cache.get(&cache_key) {
                tiles.push(tile.clone());
            }
        }
        let count = tiles.len() as u64 - first;
        if count != 0 {
            passes.push(RenderPass {
                kind: RenderPassKind::RasterTiles,
                layer_id: 0,
                plane_id: 0,
                opacity_milli: 1_000,
                first_item: first,
                item_count: count,
            });
            raster_pass_index = raster_pass_index.saturating_add(1);
        }
    }

    for layer in document.layers.iter().rev().filter(|layer| layer.visible) {
        if let Some(adjustment) = document.adjustments.get(&layer.id) {
            let index = adjustment_luts.len() as u64;
            adjustment_luts.push(resolve_adjustment_lut(adjustment, layer.opacity_milli));
            passes.push(RenderPass {
                kind: RenderPassKind::Adjustment,
                layer_id: layer.id.get(),
                plane_id: 0,
                opacity_milli: 1_000,
                first_item: index,
                item_count: 1,
            });
            continue;
        }
        passes.push(RenderPass {
            kind: RenderPassKind::LayerBegin,
            layer_id: layer.id.get(),
            plane_id: 0,
            opacity_milli: layer.opacity_milli,
            first_item: 0,
            item_count: 0,
        });
        for plane in layer.planes.iter().rev() {
            match plane.kind {
                PlaneType::MainLine
                | PlaneType::Color
                | PlaneType::Raster
                | PlaneType::Selection => {
                    if !plane.visible || plane.opacity_milli == 0 {
                        continue;
                    }
                    let first = tiles.len() as u64;
                    let mut coords: Vec<_> = plane.raster.allocated_coords().collect();
                    coords.sort_unstable();
                    for coord in coords {
                        let cache_key = (plane.id.get(), coord);
                        active_cache_keys.insert(cache_key);
                        let source_revision = revision_max_tile_source_revision(document, coord);
                        if cache
                            .get(&cache_key)
                            .is_none_or(|tile| tile.source_revision != source_revision)
                        {
                            let tile_revision = *next_tile_revision;
                            *next_tile_revision = next_tile_revision.wrapping_next_nonzero();
                            if let Some(tile) = compose_raster_plane_tile(
                                document,
                                plane,
                                coord,
                                source_revision,
                                tile_revision,
                                ordered_tile_id(raster_pass_index, coord),
                            ) {
                                cache.insert(cache_key, tile);
                            } else {
                                cache.remove(&cache_key);
                            }
                        }
                        if let Some(tile) = cache.get(&cache_key) {
                            tiles.push(tile.clone());
                        }
                    }
                    let count = tiles.len() as u64 - first;
                    if count != 0 {
                        passes.push(RenderPass {
                            kind: RenderPassKind::RasterTiles,
                            layer_id: layer.id.get(),
                            plane_id: plane.id.get(),
                            opacity_milli: 1_000,
                            first_item: first,
                            item_count: count,
                        });
                        raster_pass_index = raster_pass_index.saturating_add(1);
                    }
                }
            }
        }
        passes.push(RenderPass {
            kind: RenderPassKind::LayerEnd,
            layer_id: layer.id.get(),
            plane_id: 0,
            opacity_milli: 1_000,
            first_item: 0,
            item_count: 0,
        });
    }

    let selection_coords: Vec<_> = document.selection.allocated_coords().collect();
    if !selection_coords.is_empty() {
        let first = tiles.len() as u64;
        for coord in selection_coords {
            let cache_key = (SELECTION_CACHE_KEY, coord);
            active_cache_keys.insert(cache_key);
            let source_revision = revision_max_tile_source_revision(document, coord);
            if cache
                .get(&cache_key)
                .is_none_or(|tile| tile.source_revision != source_revision)
            {
                let tile_revision = *next_tile_revision;
                *next_tile_revision = next_tile_revision.wrapping_next_nonzero();
                if let Some(tile) = compose_selection_overlay_tile(
                    document,
                    coord,
                    source_revision,
                    tile_revision,
                    ordered_tile_id(raster_pass_index, coord),
                ) {
                    cache.insert(cache_key, tile);
                } else {
                    cache.remove(&cache_key);
                }
            }
            if let Some(tile) = cache.get(&cache_key) {
                tiles.push(tile.clone());
            }
        }
        let count = tiles.len() as u64 - first;
        if count != 0 {
            passes.push(RenderPass {
                kind: RenderPassKind::RasterTiles,
                layer_id: 0,
                plane_id: document.selection_plane_id.get(),
                opacity_milli: 1_000,
                first_item: first,
                item_count: count,
            });
        }
    }
    cache.retain(|key, _| active_cache_keys.contains(key));
    (tiles, passes, adjustment_luts)
}

fn ordered_tile_id(pass_index: u32, coord: TileCoord) -> u64 {
    (1_u64 << 63) | (u64::from(pass_index) << 28) | (u64::from(coord.y) << 14) | u64::from(coord.x)
}

fn resolve_adjustment_lut(adjustment: &Adjustment, opacity_milli: u32) -> RenderAdjustmentLut {
    let mut channels = [[0_u8; 256]; 3];
    for channel in 0..3 {
        for value in 0_u16..=255 {
            let mut input = [0_u8; 4];
            input[channel] = value as u8;
            input[3] = u8::MAX;
            let adjusted = inkpod_image::apply_adjustment(PixelValue::Rgba(input), adjustment)
                .ok()
                .and_then(PixelValue::rgba16)
                .map(|rgba| ((u32::from(rgba[channel]) + 128) / 257) as u8)
                .unwrap_or(value as u8);
            channels[channel][value as usize] = ((u32::from(value) * (1_000 - opacity_milli)
                + u32::from(adjusted) * opacity_milli
                + 500)
                / 1_000) as u8;
        }
    }
    RenderAdjustmentLut { channels }
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

#[derive(Clone, Copy)]
struct PreparedBaseTile<'a> {
    format: PixelFormat,
    tile: Option<TileView<'a>>,
}

impl PreparedBaseTile<'_> {
    fn rgba(self, local_x: u32, local_y: u32) -> [u8; 4] {
        let Some(tile) = self.tile else {
            return match self.format {
                PixelFormat::Grayscale8 | PixelFormat::Grayscale16 => [0, 0, 0, u8::MAX],
                PixelFormat::BinaryMask8
                | PixelFormat::StraightRgba8
                | PixelFormat::StraightRgba16
                | PixelFormat::PremultipliedBgra8 => [0; 4],
            };
        };
        let bytes = tile.bytes();
        let row = local_y as usize * tile.row_stride_bytes() as usize;
        match self.format {
            PixelFormat::BinaryMask8 => [0, 0, 0, bytes[row + local_x as usize]],
            PixelFormat::Grayscale8 => {
                let value = bytes[row + local_x as usize];
                [value, value, value, u8::MAX]
            }
            PixelFormat::Grayscale16 => {
                let offset = row + local_x as usize * 2;
                let value = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
                let value = ((u32::from(value) + 128) / 257) as u8;
                [value, value, value, u8::MAX]
            }
            PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8 => {
                let offset = row + local_x as usize * 4;
                [
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]
            }
            PixelFormat::StraightRgba16 => {
                let offset = row + local_x as usize * 8;
                std::array::from_fn(|channel| {
                    let start = offset + channel * 2;
                    let value = u16::from_le_bytes([bytes[start], bytes[start + 1]]);
                    ((u32::from(value) + 128) / 257) as u8
                })
            }
        }
    }
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

fn compose_raster_plane_tile(
    document: &CellDocument,
    plane: &PlaneNode,
    coord: TileCoord,
    source_revision: RenderRevision,
    tile_revision: RenderRevision,
    tile_id: u64,
) -> Option<RenderTile> {
    let origin_x = coord.x.checked_mul(TILE_SIZE)?;
    let origin_y = coord.y.checked_mul(TILE_SIZE)?;
    if origin_x >= document.width || origin_y >= document.height {
        return None;
    }
    let width = TILE_SIZE.min(document.width - origin_x);
    let height = TILE_SIZE.min(document.height - origin_y);
    if !raster_covers_tile_rect(&plane.raster, origin_x, origin_y, width, height) {
        return None;
    }
    note_snapshot_payload_access();
    let tile = plane.raster.tile_view(coord)?;
    let kind = match (plane.kind, plane.raster.format()) {
        (PlaneType::MainLine, PixelFormat::BinaryMask8 | PixelFormat::Grayscale8) => {
            PreparedPlaneKind::MainLine8(rgba8_for_display(document.main_line_color)?)
        }
        (PlaneType::MainLine, PixelFormat::Grayscale16) => {
            PreparedPlaneKind::MainLine16(rgba8_for_display(document.main_line_color)?)
        }
        (
            PlaneType::Color | PlaneType::Raster,
            PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8,
        ) => PreparedPlaneKind::Color8,
        (PlaneType::Color | PlaneType::Raster, PixelFormat::StraightRgba16) => {
            PreparedPlaneKind::Color16
        }
        (PlaneType::Selection, PixelFormat::BinaryMask8) => PreparedPlaneKind::Selection8,
        _ => return None,
    };
    let prepared = PreparedPlaneTile {
        kind,
        opacity_milli: plane.opacity_milli,
        tile,
    };
    let mut straight = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let mut rgba = prepared.rgba(x, y);
            rgba[3] = ((u32::from(rgba[3]) * plane.opacity_milli + 500) / 1_000) as u8;
            straight.extend_from_slice(&rgba);
        }
    }
    render_tile_from_straight_rgba(
        coord,
        width,
        height,
        tile_id,
        straight,
        source_revision,
        tile_revision,
    )
}

fn compose_background_tile(
    document: &CellDocument,
    base_raster: Option<&TileRaster>,
    coord: TileCoord,
    source_revision: RenderRevision,
    tile_revision: RenderRevision,
    tile_id: u64,
) -> Option<RenderTile> {
    let origin_x = coord.x.checked_mul(TILE_SIZE)?;
    let origin_y = coord.y.checked_mul(TILE_SIZE)?;
    if origin_x >= document.width || origin_y >= document.height {
        return None;
    }
    let width = TILE_SIZE.min(document.width - origin_x);
    let height = TILE_SIZE.min(document.height - origin_y);
    let base = if let Some(raster) = base_raster {
        if !raster_covers_tile_rect(raster, origin_x, origin_y, width, height) {
            return None;
        }
        let tile = raster.tile_view(coord);
        if tile.is_some() {
            note_snapshot_payload_access();
        }
        Some(PreparedBaseTile {
            format: raster.format(),
            tile,
        })
    } else {
        None
    };
    let mut straight = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let document_x = origin_x + x;
            let document_y = origin_y + y;
            let mut rgba = base.map_or([0; 4], |tile| tile.rgba(x, y));
            if document.light_table.has_visible_items() {
                note_snapshot_payload_access();
                let reference = document
                    .light_table
                    .composite(document.frames.reference_frame, document_x, document_y)
                    .unwrap_or([0; 4]);
                rgba = blend_rgba_over(rgba, reference);
            }
            straight.extend_from_slice(&rgba);
        }
    }
    render_tile_from_straight_rgba(
        coord,
        width,
        height,
        tile_id,
        straight,
        source_revision,
        tile_revision,
    )
}

fn compose_selection_overlay_tile(
    document: &CellDocument,
    coord: TileCoord,
    source_revision: RenderRevision,
    tile_revision: RenderRevision,
    tile_id: u64,
) -> Option<RenderTile> {
    let origin_x = coord.x.checked_mul(TILE_SIZE)?;
    let origin_y = coord.y.checked_mul(TILE_SIZE)?;
    if origin_x >= document.width || origin_y >= document.height {
        return None;
    }
    let width = TILE_SIZE.min(document.width - origin_x);
    let height = TILE_SIZE.min(document.height - origin_y);
    note_snapshot_payload_access();
    let tile = document.selection.tile_view(coord)?;
    let mut straight = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        let row = y as usize * tile.row_stride_bytes() as usize;
        for x in 0..width {
            let coverage = tile.bytes()[row + x as usize];
            straight.extend_from_slice(&[0, 160, 255, coverage / 3]);
        }
    }
    render_tile_from_straight_rgba(
        coord,
        width,
        height,
        tile_id,
        straight,
        source_revision,
        tile_revision,
    )
}

fn render_tile_from_straight_rgba(
    coord: TileCoord,
    width: u32,
    height: u32,
    tile_id: u64,
    straight: Vec<u8>,
    source_revision: RenderRevision,
    tile_revision: RenderRevision,
) -> Option<RenderTile> {
    if straight.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        return None;
    }
    let mut pixels = Vec::with_capacity(straight.len());
    for rgba in straight.chunks_exact(4) {
        let alpha = u32::from(rgba[3]);
        let premultiply = |channel: u8| ((u32::from(channel) * alpha + 127) / 255) as u8;
        pixels.extend_from_slice(&[
            premultiply(rgba[2]),
            premultiply(rgba[1]),
            premultiply(rgba[0]),
            rgba[3],
        ]);
    }
    Some(RenderTile {
        tile_id,
        origin: DocumentPointI32 {
            x: coord.x.checked_mul(TILE_SIZE)? as i32,
            y: coord.y.checked_mul(TILE_SIZE)? as i32,
        },
        size: DocumentSizeU32::new(width, height),
        stride_bytes: width.checked_mul(4)?,
        pixels: Arc::from(pixels),
        source_revision,
        tile_revision,
    })
}

pub(super) fn compose_tile(
    document: &CellDocument,
    base_raster: Option<&TileRaster>,
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
    let base_tile = if let Some(raster) = base_raster {
        if !raster_covers_tile_rect(raster, origin_x, origin_y, width, height) {
            return None;
        }
        let tile = raster.tile_view(coord);
        if tile.is_some() {
            note_snapshot_payload_access();
        }
        Some(PreparedBaseTile {
            format: raster.format(),
            tile,
        })
    } else {
        None
    };
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
            let mut composite = base_tile.map_or_else(
                || {
                    if document.base_surface == BaseSurface::SolidWhite
                        && (alpha_view || color_check.is_some())
                    {
                        [u8::MAX; 4]
                    } else {
                        [0_u8; 4]
                    }
                },
                |tile| tile.rgba(x, y),
            );
            if has_light_table {
                note_snapshot_payload_access();
                let reference = document
                    .light_table
                    .composite(document.frames.reference_frame, document_x, document_y)
                    .unwrap_or([0_u8; 4]);
                composite = blend_rgba_over(composite, reference);
            }
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
    tile_id: u64,
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
        tile_id,
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

fn subpalette_reference_tile_id(index: usize, coord: TileCoord) -> Option<u64> {
    let index = u16::try_from(index).ok()?;
    let x = u16::try_from(coord.x).ok()?;
    let y = u16::try_from(coord.y).ok()?;
    Some((1_u64 << 62) | (u64::from(index) << 32) | (u64::from(y) << 16) | u64::from(x))
}

pub(super) fn blend_rgba_over(background: [u8; 4], foreground: [u8; 4]) -> [u8; 4] {
    source_over_rgba8(background, foreground)
}

pub(super) fn blend_rgba16_over(background: [u16; 4], foreground: [u16; 4]) -> [u16; 4] {
    source_over_rgba16(background, foreground)
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
                let expected_composite = if alpha_view || color_check.is_some() {
                    blend_rgba_over_reference([u8::MAX; 4], expected_composite)
                } else {
                    expected_composite
                };
                let tile = compose_tile(
                    document,
                    None,
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
            shape: BrushShape::Round,
            smoothing: 0,
            start_color: StartColorPredicate::Any,
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
            "6ff0bb8ba0871c57d848f28b79984086121ff3d6299b7a1ba8a3481becce88f8",
            "primary snapshot validation call graph changed; audit payload/hash access before updating this lock"
        );
    }

    #[test]
    fn prepared_asset_base_tiles_match_canonical_pixel_display_semantics() {
        let cases = [
            (PixelFormat::BinaryMask8, PixelValue::Binary(255)),
            (PixelFormat::Grayscale8, PixelValue::Grayscale8(173)),
            (PixelFormat::Grayscale16, PixelValue::Grayscale16(40_000)),
            (
                PixelFormat::StraightRgba8,
                PixelValue::Rgba([12, 34, 56, 177]),
            ),
            (
                PixelFormat::StraightRgba16,
                PixelValue::Rgba16([1_000, 20_000, 50_000, 40_000]),
            ),
        ];

        for (format, value) in cases {
            let mut raster = TileRaster::new(65, 66, format).unwrap();
            raster.set_pixel(64, 65, value, 7).unwrap();
            for (coord, x, y) in [
                (TileCoord { x: 0, y: 0 }, 0, 0),
                (TileCoord { x: 1, y: 1 }, 64, 64),
                (TileCoord { x: 1, y: 1 }, 64, 65),
            ] {
                let prepared = PreparedBaseTile {
                    format,
                    tile: raster.tile_view(coord),
                };
                assert_eq!(
                    prepared.rgba(x % TILE_SIZE, y % TILE_SIZE),
                    animation::base_raster_pixel(&raster, x, y).unwrap(),
                    "format={format:?}, point=({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn sparse_asset_base_cache_hits_do_not_read_payloads_across_view_changes() {
        const WIDTH: u32 = 1_025;
        const HEIGHT: u32 = 1_025;
        let mut pixels = vec![0_u8; WIDTH as usize * HEIGHT as usize * 4];
        let final_pixel = pixels.len() - 4;
        pixels[final_pixel..].copy_from_slice(&[9, 8, 7, 255]);
        let mut core = Core::new();
        core.new_cell_from_raster_asset(
            RasterAssetInput {
                width: WIDTH,
                height: HEIGHT,
                pixel_format: PixelFormat::StraightRgba8,
                color_space: Some(AssetColorSpace::Srgb),
                alpha_semantics: AssetAlphaSemantics::Straight,
                canonical_stride: u64::from(WIDTH) * 4,
                pixels,
                expected_id: None,
            },
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            0x5a01,
        )
        .unwrap();

        reset_snapshot_payload_access_count();
        let initial = core.build_snapshot();
        assert_eq!(initial.tiles().len(), 1);
        assert!(snapshot_payload_access_count() > 0);
        assert_eq!(core.render_cache.len(), 1);
        let initial_revision = initial.tiles()[0].tile_revision();
        assert_eq!(core.resource_usage().render_cache_tile_count, 1);
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
            assert_eq!(snapshot.tiles().len(), 1);
            assert_eq!(snapshot.tiles()[0].tile_revision(), initial_revision);
        }
        assert_eq!(core.next_render_tile_revision, next_render_revision);
        assert_eq!(snapshot_payload_access_count(), 0);
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
    fn tile_coord_collection_deduplicates_and_preserves_order() {
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
