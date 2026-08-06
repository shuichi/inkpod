use super::raster::validate_reference_frame;
use super::*;
use crate::asset::{AssetRecord, AssetStore};
use crate::persistence::{file_plane_to_raster, raster_to_file_plane};
use inkpod_format::{FileLightTableItem, FileLightTableMetadata, FileLightTableSet};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Owned tightly packed raster bytes accepted by animation import helpers.
pub struct RgbaRasterBytes {
    /// Raster width in pixels.
    pub width: u32,
    /// Raster height in pixels.
    pub height: u32,
    /// Pixel encoding represented by `pixels`.
    pub pixel_format: PixelFormat,
    /// Optional horizontal resolution in thousandths of a DPI.
    pub dpi_x_milli: Option<u32>,
    /// Optional vertical resolution in thousandths of a DPI.
    pub dpi_y_milli: Option<u32>,
    /// Tightly packed top-to-bottom pixel bytes in `pixel_format`.
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug)]
/// Validated immutable raster source used by a light-table item.
pub struct LightTableSource {
    /// Persistent UUID of the source document.
    pub document_uuid: u128,
    /// Nonzero source content revision used for cache invalidation.
    pub source_revision: u64,
    /// Reference frame in source document pixels used for alignment.
    pub reference_frame: RectI32,
    /// Horizontal source resolution in thousandths of a DPI.
    pub dpi_x_milli: u32,
    /// Vertical source resolution in thousandths of a DPI.
    pub dpi_y_milli: u32,
    asset: Arc<AssetRecord>,
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
}

impl PartialEq for LightTableSource {
    fn eq(&self, other: &Self) -> bool {
        self.document_uuid == other.document_uuid
            && self.source_revision == other.source_revision
            && self.reference_frame == other.reference_frame
            && self.dpi_x_milli == other.dpi_x_milli
            && self.dpi_y_milli == other.dpi_y_milli
            && self.asset.id() == other.asset.id()
    }
}

impl Eq for LightTableSource {}

impl LightTableSource {
    /// Validates and converts owned tightly packed raster bytes into a source.
    pub fn from_rgba_bytes(
        document_uuid: u128,
        source_revision: u64,
        reference_frame: RectI32,
        raster: RgbaRasterBytes,
    ) -> Result<Self, CoreError> {
        let raster = CommonRaster::new(
            raster.width,
            raster.height,
            raster.pixel_format,
            raster.dpi_x_milli,
            raster.dpi_y_milli,
            raster.pixels,
        )?;
        Self::from_common_raster(document_uuid, source_revision, reference_frame, &raster)
    }

    /// Validates and copies a common raster into a tiled light-table source.
    ///
    /// UUID and revision must be nonzero and the reference frame must have positive
    /// dimensions. Failure returns no partially constructed source.
    pub fn from_common_raster(
        document_uuid: u128,
        source_revision: u64,
        reference_frame: RectI32,
        raster: &CommonRaster,
    ) -> Result<Self, CoreError> {
        raster.validate()?;
        let canonical_stride = u64::from(raster.info.width)
            .checked_mul(raster.info.pixel_format.bytes_per_pixel() as u64)
            .ok_or(CoreError::InvalidArgument(
                "light-table source stride overflows",
            ))?;
        let mut store = AssetStore::default();
        let asset = store.ingest_raster(RasterAssetInput {
            width: raster.info.width,
            height: raster.info.height,
            pixel_format: raster.info.pixel_format,
            color_space: Some(AssetColorSpace::Srgb),
            alpha_semantics: AssetAlphaSemantics::Straight,
            canonical_stride,
            pixels: raster.pixels.clone(),
            expected_id: None,
        })?;
        Self::from_record(
            document_uuid,
            source_revision,
            reference_frame,
            raster.info.dpi_x_milli.unwrap_or(DEFAULT_DPI_MILLI),
            raster.info.dpi_y_milli.unwrap_or(DEFAULT_DPI_MILLI),
            asset,
        )
    }

    pub(super) fn from_tile_raster(
        document_uuid: u128,
        source_revision: u64,
        reference_frame: RectI32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
        raster: TileRaster,
    ) -> Result<Self, CoreError> {
        let mut store = AssetStore::default();
        let asset = store.ingest_tile_raster(&raster, None)?;
        Self::from_record(
            document_uuid,
            source_revision,
            reference_frame,
            dpi_x_milli,
            dpi_y_milli,
            asset,
        )
    }

    pub(crate) fn from_record(
        document_uuid: u128,
        source_revision: u64,
        reference_frame: RectI32,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
        asset: Arc<AssetRecord>,
    ) -> Result<Self, CoreError> {
        if document_uuid == 0 || source_revision == 0 {
            return Err(CoreError::InvalidArgument(
                "light-table source identity is invalid",
            ));
        }
        if dpi_x_milli == 0 || dpi_y_milli == 0 {
            return Err(CoreError::InvalidArgument(
                "light-table source DPI is invalid",
            ));
        }
        validate_reference_frame(reference_frame)?;
        let raster = asset
            .raster()
            .ok_or(CoreError::InvalidState("light-table asset is not a raster"))?;
        Ok(Self {
            document_uuid,
            source_revision,
            reference_frame,
            dpi_x_milli,
            dpi_y_milli,
            width: raster.width(),
            height: raster.height(),
            pixel_format: raster.format(),
            asset,
        })
    }

    pub(crate) fn asset_id(&self) -> AssetId {
        self.asset.id()
    }

    pub(crate) fn intern_into(&mut self, store: &mut AssetStore) -> Result<AssetId, CoreError> {
        self.asset = store.intern_record(Arc::clone(&self.asset))?;
        Ok(self.asset.id())
    }

    pub(super) fn immutable_raster(&self) -> &TileRaster {
        self.asset
            .raster()
            .expect("a validated light-table source always owns a raster asset")
            .as_ref()
    }

    /// Returns source width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns source height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the source pixel format.
    #[must_use]
    pub const fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Complete input for a new light-table item.
pub struct LightTableItemInput {
    /// User-visible item name.
    pub name: String,
    /// Owned immutable source raster and identity.
    pub source: LightTableSource,
    /// Whether the item is individually visible.
    pub visible: bool,
    /// Item opacity in `0..=1000` before set opacity is applied.
    pub opacity_milli: u32,
    /// Color/display treatment.
    pub display_mode: LightTableDisplayMode,
    /// Straight-alpha display color used by non-color modes.
    pub display_color: PixelValue,
    /// Horizontal document translation in thousandths of a pixel.
    pub translate_x_milli: i32,
    /// Vertical document translation in thousandths of a pixel.
    pub translate_y_milli: i32,
    /// Positive horizontal scale in thousandths (`1000 == 1.0`).
    pub scale_x_milli: u32,
    /// Positive vertical scale in thousandths (`1000 == 1.0`).
    pub scale_y_milli: u32,
    /// Clockwise rotation in thousandths of a degree.
    pub rotation_milli_degrees: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Mutable display properties of an existing light-table item.
pub struct LightTableItemProperties {
    /// Whether the item is individually visible.
    pub visible: bool,
    /// Item opacity in `0..=1000` before set opacity is applied.
    pub opacity_milli: u32,
    /// Color/display treatment.
    pub display_mode: LightTableDisplayMode,
    /// Straight-alpha display color used by non-color modes.
    pub display_color: PixelValue,
    /// Horizontal document translation in thousandths of a pixel.
    pub translate_x_milli: i32,
    /// Vertical document translation in thousandths of a pixel.
    pub translate_y_milli: i32,
    /// Positive horizontal scale in thousandths (`1000 == 1.0`).
    pub scale_x_milli: u32,
    /// Positive vertical scale in thousandths (`1000 == 1.0`).
    pub scale_y_milli: u32,
    /// Clockwise rotation in thousandths of a degree.
    pub rotation_milli_degrees: i32,
}

impl LightTableItemInput {
    /// Creates a visible, fully opaque, untransformed color-mode item.
    #[must_use]
    pub fn new(name: impl Into<String>, source: LightTableSource) -> Self {
        Self {
            name: name.into(),
            source,
            visible: true,
            opacity_milli: 1_000,
            display_mode: LightTableDisplayMode::Color,
            display_color: PixelValue::Rgba([0, 128, 255, 255]),
            translate_x_milli: 0,
            translate_y_milli: 0,
            scale_x_milli: 1_000,
            scale_y_milli: 1_000,
            rotation_milli_degrees: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Public metadata for one item in the active light-table set.
pub struct LightTableItemInfo {
    /// Stable item ID, valid until removal.
    pub id: u64,
    /// Stable internal source-plane ID used by persistence and rendering.
    pub source_plane_id: u64,
    /// User-visible item name.
    pub name: String,
    /// Persistent UUID of the source document.
    pub source_document_uuid: u128,
    /// Source revision used for reload/cache comparison.
    pub source_revision: u64,
    /// Whether the item is individually visible.
    pub visible: bool,
    /// Item opacity in `0..=1000`.
    pub opacity_milli: u32,
    /// Combined item/set opacity in `0..=1000`.
    pub effective_opacity_milli: u32,
    /// Color/display treatment.
    pub display_mode: LightTableDisplayMode,
    /// Straight-alpha display color.
    pub display_color: PixelValue,
    /// Horizontal document translation in thousandths of a pixel.
    pub translate_x_milli: i32,
    /// Vertical document translation in thousandths of a pixel.
    pub translate_y_milli: i32,
    /// Horizontal scale in thousandths.
    pub scale_x_milli: u32,
    /// Vertical scale in thousandths.
    pub scale_y_milli: u32,
    /// Clockwise rotation in thousandths of a degree.
    pub rotation_milli_degrees: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Public metadata for one light-table set.
pub struct LightTableSetInfo {
    /// Stable set ID, valid until deletion.
    pub id: u64,
    /// User-visible set name.
    pub name: String,
    /// Whether this is the active set.
    pub active: bool,
    /// Set opacity in `0..=1000`.
    pub global_opacity_milli: u32,
    /// Number of contained items.
    pub item_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LightTableItem {
    pub(super) id: LightTableItemId,
    pub(super) source_plane_id: PlaneId,
    pub(super) name: String,
    pub(super) source: LightTableSource,
    pub(super) visible: bool,
    pub(super) opacity_milli: u32,
    pub(super) display_mode: LightTableDisplayMode,
    pub(super) display_color: PixelValue,
    pub(super) translate_x_milli: i32,
    pub(super) translate_y_milli: i32,
    pub(super) scale_x_milli: u32,
    pub(super) scale_y_milli: u32,
    pub(super) rotation_milli_degrees: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LightTableSet {
    pub(super) id: LightTableSetId,
    pub(super) name: String,
    pub(super) global_opacity_milli: u32,
    pub(super) items: Vec<LightTableItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LightTableState {
    pub(super) active_set_id: LightTableSetId,
    pub(super) sets: Vec<LightTableSet>,
}

impl LightTableState {
    pub(crate) fn new(default_set_id: LightTableSetId) -> Self {
        Self {
            active_set_id: default_set_id,
            sets: vec![LightTableSet {
                id: default_set_id,
                name: "Default".to_owned(),
                global_opacity_milli: 1_000,
                items: Vec::new(),
            }],
        }
    }

    pub(super) fn active(&self) -> Option<&LightTableSet> {
        self.sets.iter().find(|set| set.id == self.active_set_id)
    }

    pub(super) fn active_mut(&mut self) -> Option<&mut LightTableSet> {
        self.sets
            .iter_mut()
            .find(|set| set.id == self.active_set_id)
    }

    pub(crate) fn maximum_id(&self) -> u64 {
        self.sets
            .iter()
            .flat_map(|set| {
                std::iter::once(set.id.get()).chain(
                    set.items
                        .iter()
                        .flat_map(|item| [item.id.get(), item.source_plane_id.get()]),
                )
            })
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn has_visible_items(&self) -> bool {
        self.active().is_some_and(|set| {
            set.global_opacity_milli != 0
                && set
                    .items
                    .iter()
                    .any(|item| item.visible && item.opacity_milli != 0)
        })
    }

    pub(crate) fn logical_raster_usage(&self) -> (u64, u64) {
        self.sets
            .iter()
            .flat_map(|set| &set.items)
            .fold((0_u64, 0_u64), |(tiles, bytes), item| {
                let raster = item.source.immutable_raster();
                (
                    tiles.saturating_add(raster.allocated_tile_count() as u64),
                    bytes.saturating_add(raster.allocated_tile_bytes()),
                )
            })
    }

    pub(crate) fn source_revision(&self) -> u64 {
        self.active()
            .into_iter()
            .flat_map(|set| set.items.iter())
            .filter(|item| item.visible)
            .map(|item| item.source.source_revision)
            .max()
            .unwrap_or(0)
    }

    pub(super) fn item_count(&self) -> usize {
        self.sets.iter().map(|set| set.items.len()).sum()
    }

    pub(crate) fn intern_into(&mut self, store: &mut AssetStore) -> Result<(), CoreError> {
        for item in self.sets.iter_mut().flat_map(|set| &mut set.items) {
            item.source.intern_into(store)?;
        }
        Ok(())
    }

    pub(crate) fn asset_ids(&self) -> impl Iterator<Item = AssetId> + '_ {
        self.sets
            .iter()
            .flat_map(|set| &set.items)
            .map(|item| item.source.asset_id())
    }

    pub(crate) fn asset_ids_by_source_plane(&self) -> Result<BTreeMap<u64, AssetId>, CoreError> {
        let mut assets = BTreeMap::new();
        for item in self.sets.iter().flat_map(|set| &set.items) {
            if assets
                .insert(item.source_plane_id.get(), item.source.asset_id())
                .is_some()
            {
                return Err(CoreError::InvalidState(
                    "canonical light-table source plane ID is duplicated",
                ));
            }
        }
        Ok(assets)
    }

    pub(crate) fn file_planes(&self) -> Vec<FilePlane> {
        self.sets
            .iter()
            .flat_map(|set| set.items.iter())
            .map(|item| {
                raster_to_file_plane(
                    item.source_plane_id.get(),
                    FilePlaneKind::LightTable,
                    item.source.immutable_raster(),
                )
            })
            .collect()
    }

    pub(crate) fn to_file(&self) -> FileLightTableMetadata {
        FileLightTableMetadata {
            active_set_id: self.active_set_id.get(),
            sets: self
                .sets
                .iter()
                .map(|set| FileLightTableSet {
                    id: set.id.get(),
                    name: set.name.clone(),
                    global_opacity_milli: set.global_opacity_milli,
                    items: set
                        .items
                        .iter()
                        .map(|item| FileLightTableItem {
                            id: item.id.get(),
                            source_plane_id: item.source_plane_id.get(),
                            source_document_uuid: item.source.document_uuid.to_le_bytes(),
                            source_revision: item.source.source_revision,
                            source_reference_frame: item.source.reference_frame,
                            source_dpi_x_milli: item.source.dpi_x_milli,
                            source_dpi_y_milli: item.source.dpi_y_milli,
                            name: item.name.clone(),
                            visible: item.visible,
                            opacity_milli: item.opacity_milli,
                            display_mode: item.display_mode,
                            display_color: item.display_color,
                            translate_x_milli: item.translate_x_milli,
                            translate_y_milli: item.translate_y_milli,
                            scale_x_milli: item.scale_x_milli,
                            scale_y_milli: item.scale_y_milli,
                            rotation_milli_degrees: item.rotation_milli_degrees,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    pub(crate) fn from_file(
        metadata: Option<&FileLightTableMetadata>,
        planes: &[FilePlane],
        revision: DocumentRevision,
        legacy_set_id: LightTableSetId,
    ) -> Result<Self, CoreError> {
        let Some(metadata) = metadata else {
            return Ok(Self::new(legacy_set_id));
        };
        let mut sets = Vec::with_capacity(metadata.sets.len());
        for set in &metadata.sets {
            let mut items = Vec::with_capacity(set.items.len());
            for item in &set.items {
                let plane = planes
                    .iter()
                    .find(|plane| plane.id == item.source_plane_id)
                    .ok_or(CoreError::InvalidState(
                        "light-table source payload is missing",
                    ))?;
                if plane.kind != FilePlaneKind::LightTable {
                    return Err(CoreError::InvalidState(
                        "light-table source payload has the wrong type",
                    ));
                }
                items.push(LightTableItem {
                    id: LightTableItemId::from_raw(item.id),
                    source_plane_id: PlaneId::from_raw(item.source_plane_id),
                    name: item.name.clone(),
                    source: LightTableSource::from_tile_raster(
                        u128::from_le_bytes(item.source_document_uuid),
                        item.source_revision,
                        item.source_reference_frame,
                        item.source_dpi_x_milli,
                        item.source_dpi_y_milli,
                        file_plane_to_raster(plane, revision.get())?,
                    )?,
                    visible: item.visible,
                    opacity_milli: item.opacity_milli,
                    display_mode: item.display_mode,
                    display_color: item.display_color,
                    translate_x_milli: item.translate_x_milli,
                    translate_y_milli: item.translate_y_milli,
                    scale_x_milli: item.scale_x_milli,
                    scale_y_milli: item.scale_y_milli,
                    rotation_milli_degrees: item.rotation_milli_degrees,
                });
            }
            sets.push(LightTableSet {
                id: LightTableSetId::from_raw(set.id),
                name: set.name.clone(),
                global_opacity_milli: set.global_opacity_milli,
                items,
            });
        }
        Ok(Self {
            active_set_id: LightTableSetId::from_raw(metadata.active_set_id),
            sets,
        })
    }

    pub(crate) fn sample(
        &self,
        destination_reference: RectI32,
        x: u32,
        y: u32,
    ) -> Result<Option<PixelValue>, CoreError> {
        let Some(set) = self.active() else {
            return Ok(None);
        };
        for item in &set.items {
            if !item.visible || item.opacity_milli == 0 || set.global_opacity_milli == 0 {
                continue;
            }
            let Some((source, source_x, source_y)) =
                sample_item_source(item, destination_reference, x, y)?
            else {
                continue;
            };
            let mut value = if item.display_mode == LightTableDisplayMode::Color {
                source
            } else {
                PixelValue::Rgba(display_item_pixel(item, source, source_x, source_y)?)
            };
            let effective = effective_opacity(item.opacity_milli, set.global_opacity_milli);
            match &mut value {
                PixelValue::Rgba(rgba) => {
                    rgba[3] = ((u32::from(rgba[3]) * effective + 500) / 1_000) as u8;
                }
                PixelValue::Rgba16(rgba) => {
                    rgba[3] = ((u64::from(rgba[3]) * u64::from(effective) + 500) / 1_000) as u16;
                }
                _ => {
                    return Err(CoreError::InvalidState(
                        "light-table source is not straight RGBA",
                    ));
                }
            }
            if !value.is_transparent() {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    pub(crate) fn composite(
        &self,
        destination_reference: RectI32,
        x: u32,
        y: u32,
    ) -> Option<[u8; 4]> {
        let set = self.active()?;
        let mut composite = [0_u8; 4];
        for item in set.items.iter().rev() {
            if !item.visible || item.opacity_milli == 0 || set.global_opacity_milli == 0 {
                continue;
            }
            let Some(mut rgba) = sample_item(item, destination_reference, x, y)
                .ok()
                .flatten()
            else {
                continue;
            };
            let effective = effective_opacity(item.opacity_milli, set.global_opacity_milli);
            rgba[3] = ((u32::from(rgba[3]) * effective + 500) / 1_000) as u8;
            composite = blend_rgba_over(composite, rgba);
        }
        (composite[3] != 0).then_some(composite)
    }
}

pub(super) fn validate_item_input(input: &LightTableItemInput) -> Result<(), CoreError> {
    validate_node_name(&input.name)?;
    validate_light_table_source(&input.source)?;
    if input.opacity_milli > 1_000
        || input.scale_x_milli == 0
        || input.scale_y_milli == 0
        || input.scale_x_milli > 64_000
        || input.scale_y_milli > 64_000
        || input.rotation_milli_degrees.unsigned_abs() > 360_000
        || input.display_color.rgba16().is_none()
    {
        return Err(CoreError::InvalidArgument(
            "light-table item properties are invalid",
        ));
    }
    Ok(())
}

pub(super) fn validate_light_table_source(source: &LightTableSource) -> Result<(), CoreError> {
    if source.document_uuid == 0
        || source.source_revision == 0
        || source.dpi_x_milli == 0
        || source.dpi_y_milli == 0
    {
        return Err(CoreError::InvalidArgument(
            "light-table source identity or DPI is invalid",
        ));
    }
    validate_reference_frame(source.reference_frame)
}

pub(super) fn unique_light_table_set_name(sets: &[LightTableSet], requested: &str) -> String {
    if !sets.iter().any(|set| set.name == requested) {
        return requested.to_owned();
    }
    for suffix in 2..=256 {
        let candidate = format!("{requested} {suffix}");
        if !sets.iter().any(|set| set.name == candidate) {
            return candidate;
        }
    }
    format!("{requested} {}", sets.len() + 1)
}

pub(super) const fn effective_opacity(item: u32, global: u32) -> u32 {
    (item * global + 500) / 1_000
}

fn sample_item(
    item: &LightTableItem,
    destination_reference: RectI32,
    x: u32,
    y: u32,
) -> Result<Option<[u8; 4]>, CoreError> {
    let Some((value, source_x, source_y)) = sample_item_source(item, destination_reference, x, y)?
    else {
        return Ok(None);
    };
    Ok(Some(display_item_pixel(item, value, source_x, source_y)?))
}

fn sample_item_source(
    item: &LightTableItem,
    destination_reference: RectI32,
    x: u32,
    y: u32,
) -> Result<Option<(PixelValue, i64, i64)>, CoreError> {
    use inkpod_image::{CANONICAL_DOCUMENT_ONE, div_round_ties_even_i128, rotate_q16};

    let local_q16 = |coordinate: u32, origin: i32, translation_milli: i64| {
        let milli = (i128::from(coordinate) - i128::from(origin))
            .checked_mul(1_000)?
            .checked_sub(i128::from(translation_milli))?;
        div_round_ties_even_i128(
            milli.checked_mul(i128::from(CANONICAL_DOCUMENT_ONE))?,
            1_000,
        )?
        .try_into()
        .ok()
    };
    let local_x = local_q16(
        x,
        destination_reference.x,
        i64::from(item.translate_x_milli),
    )
    .ok_or(CoreError::InvalidState("light-table transform overflowed"))?;
    let local_y = local_q16(
        y,
        destination_reference.y,
        i64::from(item.translate_y_milli),
    )
    .ok_or(CoreError::InvalidState("light-table transform overflowed"))?;
    let turns = div_round_ties_even_i128(
        -i128::from(item.rotation_milli_degrees) * (1_i128 << 32),
        360_000,
    )
    .ok_or(CoreError::InvalidState("light-table angle overflowed"))?
    .rem_euclid(1_i128 << 32) as u32;
    let (rotated_x, rotated_y) = rotate_q16(local_x, local_y, turns)
        .ok_or(CoreError::InvalidState("light-table rotation overflowed"))?;
    let source_coordinate = |frame: i32, rotated: i64, scale_milli: u32| {
        let scaled = div_round_ties_even_i128(
            i128::from(rotated).checked_mul(1_000)?,
            i128::from(scale_milli),
        )?;
        let q16 = i128::from(frame)
            .checked_mul(i128::from(CANONICAL_DOCUMENT_ONE))?
            .checked_add(scaled)?;
        div_round_ties_even_i128(q16, i128::from(CANONICAL_DOCUMENT_ONE))?
            .try_into()
            .ok()
    };
    let source_x = source_coordinate(item.source.reference_frame.x, rotated_x, item.scale_x_milli)
        .ok_or(CoreError::InvalidState(
            "light-table source coordinate overflowed",
        ))?;
    let source_y = source_coordinate(item.source.reference_frame.y, rotated_y, item.scale_y_milli)
        .ok_or(CoreError::InvalidState(
            "light-table source coordinate overflowed",
        ))?;
    if source_x < 0
        || source_y < 0
        || source_x >= i64::from(item.source.width())
        || source_y >= i64::from(item.source.height())
    {
        return Ok(None);
    }
    let value = item
        .source
        .immutable_raster()
        .pixel(source_x as u32, source_y as u32)?;
    if value.is_transparent() {
        return Ok(None);
    }
    Ok(Some((value, source_x, source_y)))
}

fn display_item_pixel(
    item: &LightTableItem,
    value: PixelValue,
    source_x: i64,
    source_y: i64,
) -> Result<[u8; 4], CoreError> {
    let mut rgba = rgba8_for_display(value)
        .ok_or(CoreError::InvalidState("light-table source is not RGBA"))?;
    match item.display_mode {
        LightTableDisplayMode::Color => {}
        LightTableDisplayMode::Monotone => {
            let luminance = (u32::from(rgba[0]) * 54
                + u32::from(rgba[1]) * 183
                + u32::from(rgba[2]) * 19
                + 128)
                / 256;
            let tint = rgba8_for_display(item.display_color).ok_or(CoreError::InvalidState(
                "light-table display color is invalid",
            ))?;
            for channel in 0..3 {
                rgba[channel] = ((luminance * u32::from(tint[channel]) + 127) / 255) as u8;
            }
        }
        LightTableDisplayMode::Halftone => {
            let luminance = (u32::from(rgba[0]) + u32::from(rgba[1]) + u32::from(rgba[2])) / 3;
            let threshold = if (source_x + source_y) & 1 == 0 {
                96
            } else {
                160
            };
            let value = if luminance >= threshold { 255 } else { 0 };
            rgba[..3].fill(value);
        }
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u64, source: LightTableSource) -> LightTableItem {
        LightTableItem {
            id: LightTableItemId::from_raw(id),
            source_plane_id: PlaneId::from_raw(id + 100),
            name: format!("Item {id}"),
            source,
            visible: true,
            opacity_milli: 1_000,
            display_mode: LightTableDisplayMode::Color,
            display_color: PixelValue::Rgba([0, 0, 0, 255]),
            translate_x_milli: 0,
            translate_y_milli: 0,
            scale_x_milli: 1_000,
            scale_y_milli: 1_000,
            rotation_milli_degrees: 0,
        }
    }

    fn source(uuid: u128, frame: RectI32, pixels: Vec<u8>) -> LightTableSource {
        LightTableSource::from_rgba_bytes(
            uuid,
            1,
            frame,
            RgbaRasterBytes {
                width: 3,
                height: 3,
                pixel_format: PixelFormat::StraightRgba8,
                dpi_x_milli: Some(DEFAULT_DPI_MILLI),
                dpi_y_milli: Some(DEFAULT_DPI_MILLI),
                pixels,
            },
        )
        .unwrap()
    }

    #[test]
    fn reference_frame_alignment_and_topmost_item_order_are_stable() {
        let mut aligned_pixels = vec![0_u8; 3 * 3 * 4];
        let center_offset = 16;
        aligned_pixels[center_offset..center_offset + 4].copy_from_slice(&[10, 20, 30, 255]);
        let aligned = item(
            1,
            source(
                1,
                RectI32 {
                    x: 1,
                    y: 1,
                    width: 3,
                    height: 3,
                },
                aligned_pixels,
            ),
        );
        let top = item(
            2,
            source(
                2,
                RectI32 {
                    x: 1,
                    y: 1,
                    width: 3,
                    height: 3,
                },
                [200, 100, 50, 255].repeat(9),
            ),
        );
        let state = LightTableState {
            active_set_id: LightTableSetId::from_raw(9),
            sets: vec![LightTableSet {
                id: LightTableSetId::from_raw(9),
                name: "Default".to_owned(),
                global_opacity_milli: 1_000,
                items: vec![top, aligned],
            }],
        };
        let destination = RectI32 {
            x: 4,
            y: 4,
            width: 3,
            height: 3,
        };
        assert_eq!(
            state.sample(destination, 4, 4).unwrap(),
            Some(PixelValue::Rgba([200, 100, 50, 255]))
        );
        let aligned_only = LightTableState {
            active_set_id: LightTableSetId::from_raw(9),
            sets: vec![LightTableSet {
                id: LightTableSetId::from_raw(9),
                name: "Default".to_owned(),
                global_opacity_milli: 1_000,
                items: vec![state.sets[0].items[1].clone()],
            }],
        };
        assert_eq!(
            aligned_only.sample(destination, 4, 4).unwrap(),
            Some(PixelValue::Rgba([10, 20, 30, 255]))
        );
        assert_eq!(aligned_only.sample(destination, 3, 3).unwrap(), None);
    }

    #[test]
    fn asset_identity_excludes_source_provenance_alignment_and_dpi() {
        let pixels = [17, 34, 51, 68].repeat(4);
        let first_raster = CommonRaster::new(
            2,
            2,
            PixelFormat::StraightRgba8,
            Some(96_000),
            Some(96_000),
            pixels.clone(),
        )
        .unwrap();
        let second_raster = CommonRaster::new(
            2,
            2,
            PixelFormat::StraightRgba8,
            Some(300_000),
            Some(300_000),
            pixels,
        )
        .unwrap();
        let mut first = LightTableSource::from_common_raster(
            1,
            7,
            RectI32 {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            &first_raster,
        )
        .unwrap();
        let mut second = LightTableSource::from_common_raster(
            2,
            99,
            RectI32 {
                x: 10,
                y: -20,
                width: 4,
                height: 5,
            },
            &second_raster,
        )
        .unwrap();

        assert_eq!(first.asset_id(), second.asset_id());
        assert_ne!(first, second);

        let mut store = AssetStore::default();
        first.intern_into(&mut store).unwrap();
        second.intern_into(&mut store).unwrap();
        assert!(Arc::ptr_eq(&first.asset, &second.asset));
        assert_eq!(store.usage().asset_count, 1);
        assert_eq!(store.usage().logical_payload_bytes, 16);
    }
}
