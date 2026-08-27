use crate::animation::{MAX_SEQUENCE_CELLS, natural_cmp, parse_cell_number};
use crate::document::validate_node_name;
use crate::identity::RenderRevision;
use crate::reference_view::{ReferenceImage, ReferenceView};
use crate::{CommonRasterFormat, CoreError, PixelValue, RenderSnapshot, ViewCommand, ViewState};
use inkpod_format::decode_common_raster;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// Maximum number of external image entries accepted by one subpalette catalog.
pub const MAX_SUBPALETTE_ITEMS: usize = MAX_SEQUENCE_CELLS;

/// Maximum aggregate decoded pixel payload retained by one subpalette cache.
///
/// The limit bounds decoded reference pixels; no editable document or full tiled
/// copy is created. Managed image leases also retain their manager budget charge.
pub const MAX_SUBPALETTE_CACHE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Stable nonzero identity of one entry within a [`SubpaletteCatalog`].
///
/// An identity remains valid until that catalog is replaced. Replacements never
/// reuse an identity during the lifetime of the catalog.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SubpaletteItemId(u64);

impl SubpaletteItemId {
    /// Reconstructs a nonzero identity received from a fixed-width boundary.
    #[must_use]
    pub const fn from_raw(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// Returns the nonzero fixed-width value used at public boundaries.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Caller-supplied OS-neutral metadata for one external image source.
///
/// `source_token` is an opaque nonzero value returned unchanged to the caller.
/// It may identify frontend-owned path authority, but Rust never interprets or
/// persists it as a path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubpaletteSource {
    /// Nonzero caller token unique within one replacement request.
    pub source_token: u64,
    /// User-visible file name without an external directory requirement.
    pub name: String,
}

/// One borrowed encoded image used to populate a complete subpalette cache.
///
/// Bytes are decoded during [`SubpaletteCatalog::load_cached_images`] and are
/// never retained. `item_id` must identify exactly one item in the current
/// catalog, and the complete input span must cover every catalog item once.
#[derive(Clone, Copy, Debug)]
pub struct SubpaletteImageInput<'a> {
    /// Stable catalog item receiving this decoded image.
    pub item_id: SubpaletteItemId,
    /// Explicit common-raster codec for the borrowed bytes.
    pub format: CommonRasterFormat,
    /// Nonempty encoded PNG/TIFF/TGA/BMP bytes.
    pub bytes: &'a [u8],
}

/// Immutable metadata for one naturally ordered external image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubpaletteItem {
    /// Stable identity in this catalog lifetime.
    pub id: SubpaletteItemId,
    /// Opaque frontend token supplied with the source.
    pub source_token: u64,
    /// User-visible source name.
    pub name: String,
    /// Last decimal run in the file stem, when present and representable.
    pub cell_number: Option<u32>,
}

/// Side-effect-free catalog and selection summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubpaletteCatalogInfo {
    /// Monotonic catalog replacement revision. Zero means no source was installed.
    pub catalog_revision: u64,
    /// Number of naturally ordered source entries.
    pub item_count: u32,
    /// Selected item index, or `None` before a source image is decoded.
    pub active_index: Option<u32>,
    /// Whether the selected item has a successfully decoded raster.
    pub image_loaded: bool,
    /// Whether every catalog item is decoded and available without encoded input.
    pub cache_complete: bool,
}

/// Workspace-scoped, read-only external-image catalog used by the subpalette.
///
/// The catalog retains immutable memory images or I/O-manager image leases and never changes a user document, history,
/// dirty state, savepoint, or replay state. Source replacement and image decode
/// publish atomically. Zoom, pan, and viewport changes affect only the private
/// view. The type is single-writer and must be externally thread-affined by its
/// frontend or ABI owner.
pub struct SubpaletteCatalog {
    view: ReferenceView,
    images: BTreeMap<SubpaletteItemId, ReferenceImage>,
    next_image_revision: RenderRevision,
    items: Vec<SubpaletteItem>,
    next_item_id: u64,
    catalog_revision: u64,
    active_index: Option<usize>,
    cache_complete: bool,
    viewport_width: f64,
    viewport_height: f64,
}

impl SubpaletteCatalog {
    /// Creates an empty catalog with one private read-only view.
    ///
    /// No editable Core, Genesis, document identity, history, or savepoint is
    /// created for this reference-only view.
    pub fn new() -> Result<Self, CoreError> {
        Ok(Self {
            view: ReferenceView::new(1.0, 1.0)?,
            images: BTreeMap::new(),
            next_image_revision: RenderRevision::from_raw(1),
            items: Vec::new(),
            next_item_id: 1,
            catalog_revision: 0,
            active_index: None,
            cache_complete: false,
            viewport_width: 1.0,
            viewport_height: 1.0,
        })
    }

    /// Atomically replaces the catalog and clears the decoded selection.
    ///
    /// Numbered names sort by parsed cell number and then natural name.
    /// Unnumbered names follow numbered entries in natural-name order. Invalid,
    /// duplicate-token, empty, over-capacity, or revision-overflow input changes
    /// no state and consumes no item identity. Equal leaf names are allowed so
    /// files selected from different directories remain distinct.
    pub fn replace_sources(
        &mut self,
        sources: Vec<SubpaletteSource>,
    ) -> Result<SubpaletteCatalogInfo, CoreError> {
        if sources.is_empty() || sources.len() > MAX_SUBPALETTE_ITEMS {
            return Err(CoreError::InvalidArgument(
                "subpalette source count is outside bounds",
            ));
        }
        let next_revision = self
            .catalog_revision
            .checked_add(1)
            .ok_or(CoreError::InvalidState(
                "subpalette catalog revision overflow",
            ))?;
        let required_ids = u64::try_from(sources.len())
            .map_err(|_| CoreError::InvalidArgument("subpalette source count overflows"))?;
        let next_item_id = self
            .next_item_id
            .checked_add(required_ids)
            .ok_or(CoreError::InvalidState("subpalette item ID overflow"))?;

        let mut tokens = BTreeSet::new();
        for source in &sources {
            if source.source_token == 0 || !tokens.insert(source.source_token) {
                return Err(CoreError::InvalidArgument(
                    "subpalette source token is zero or duplicated",
                ));
            }
            validate_node_name(&source.name)?;
        }

        let first_id = self.next_item_id;
        let mut items: Vec<_> = sources
            .into_iter()
            .enumerate()
            .map(|(index, source)| SubpaletteItem {
                id: SubpaletteItemId(first_id + index as u64),
                source_token: source.source_token,
                cell_number: parse_cell_number(&source.name),
                name: source.name,
            })
            .collect();
        items.sort_by(|left, right| match (left.cell_number, right.cell_number) {
            (Some(left_number), Some(right_number)) => left_number
                .cmp(&right_number)
                .then_with(|| natural_cmp(&left.name, &right.name)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => natural_cmp(&left.name, &right.name),
        });
        let view = ReferenceView::new(self.viewport_width, self.viewport_height)?;

        self.items = items;
        self.view = view;
        self.images.clear();
        self.next_item_id = next_item_id;
        self.catalog_revision = next_revision;
        self.active_index = None;
        self.cache_complete = false;
        Ok(self.info())
    }

    /// Clears every source and decoded raster without resetting ID authority.
    pub fn clear(&mut self) -> Result<SubpaletteCatalogInfo, CoreError> {
        if self.items.is_empty() && self.active_index.is_none() {
            return Ok(self.info());
        }
        let next_revision = self
            .catalog_revision
            .checked_add(1)
            .ok_or(CoreError::InvalidState(
                "subpalette catalog revision overflow",
            ))?;
        let view = ReferenceView::new(self.viewport_width, self.viewport_height)?;
        self.catalog_revision = next_revision;
        self.items.clear();
        self.active_index = None;
        self.cache_complete = false;
        self.view = view;
        self.images.clear();
        Ok(self.info())
    }

    /// Returns an immutable catalog summary.
    #[must_use]
    pub fn info(&self) -> SubpaletteCatalogInfo {
        SubpaletteCatalogInfo {
            catalog_revision: self.catalog_revision,
            item_count: self.items.len() as u32,
            active_index: self.active_index.map(|index| index as u32),
            image_loaded: self.active_index.is_some(),
            cache_complete: self.cache_complete,
        }
    }

    /// Returns naturally ordered item metadata without decoding image content.
    pub fn item(&self, index: usize) -> Result<&SubpaletteItem, CoreError> {
        self.items.get(index).ok_or(CoreError::InvalidArgument(
            "subpalette item index is outside bounds",
        ))
    }

    /// Resolves one adjacent entry without changing selection or decoding data.
    ///
    /// Endpoints stop at the first or last item. Before an image is loaded,
    /// either direction resolves the first item.
    pub fn adjacent_item(
        &self,
        direction: crate::SequenceDirection,
    ) -> Result<&SubpaletteItem, CoreError> {
        if self.items.is_empty() {
            return Err(CoreError::InvalidState(
                "subpalette catalog contains no items",
            ));
        }
        let index = match (self.active_index, direction) {
            (None, _) => 0,
            (Some(0), crate::SequenceDirection::Previous) => 0,
            (Some(index), crate::SequenceDirection::Previous) => index - 1,
            (Some(index), crate::SequenceDirection::Next) => (index + 1).min(self.items.len() - 1),
        };
        Ok(&self.items[index])
    }

    /// Decodes and atomically selects one catalog image.
    ///
    /// `bytes` are borrowed conceptually and moved into the private decoder for
    /// this call. A missing ID, unsupported format, malformed raster, allocation
    /// failure, or view update failure retains the previously selected image.
    pub fn load_image(
        &mut self,
        item_id: SubpaletteItemId,
        format: CommonRasterFormat,
        bytes: Vec<u8>,
    ) -> Result<SubpaletteCatalogInfo, CoreError> {
        let index = self
            .items
            .iter()
            .position(|item| item.id == item_id)
            .ok_or(CoreError::InvalidArgument(
                "subpalette item ID does not exist",
            ))?;

        let next_revision =
            self.next_image_revision
                .get()
                .checked_add(1)
                .ok_or(CoreError::InvalidState(
                    "reference image revision overflows",
                ))?;
        let image = ReferenceImage::memory(
            decode_common_raster(format, &bytes)?,
            self.next_image_revision.get(),
        )?;
        let view = ReferenceView::fitted(image.size(), self.viewport_width, self.viewport_height)?;
        self.images = BTreeMap::from([(item_id, image)]);
        self.view = view;
        self.next_image_revision = RenderRevision::from_raw(next_revision);
        self.active_index = Some(index);
        self.cache_complete = false;
        Ok(self.info())
    }

    /// Decodes every catalog image into immutable reference storage and selects
    /// `active_item_id` atomically.
    ///
    /// Input order is irrelevant; stable item IDs restore natural catalog order.
    /// Every current item must appear exactly once. The aggregate decoded pixel
    /// payload may not exceed [`MAX_SUBPALETTE_CACHE_BYTES`]. Success releases no
    /// caller memory because encoded bytes are only borrowed for this call.
    /// Missing/duplicate IDs, decode/allocation/view failure, or aggregate-size
    /// overflow preserves the previous decoded cache, active item, view, and
    /// catalog revision.
    pub fn load_cached_images(
        &mut self,
        inputs: &[SubpaletteImageInput<'_>],
        active_item_id: SubpaletteItemId,
    ) -> Result<SubpaletteCatalogInfo, CoreError> {
        if inputs.len() != self.items.len() || inputs.is_empty() {
            return Err(CoreError::InvalidArgument(
                "subpalette cache input count does not match the catalog",
            ));
        }
        let active_index = self
            .items
            .iter()
            .position(|item| item.id == active_item_id)
            .ok_or(CoreError::InvalidArgument(
                "subpalette active cache item does not exist",
            ))?;
        let mut by_id = BTreeMap::new();
        for input in inputs {
            if input.bytes.is_empty() || by_id.insert(input.item_id, input).is_some() {
                return Err(CoreError::InvalidArgument(
                    "subpalette cache input is empty or duplicated",
                ));
            }
        }

        let next_revision =
            self.next_image_revision
                .get()
                .checked_add(1)
                .ok_or(CoreError::InvalidState(
                    "reference image revision overflows",
                ))?;
        let mut images = BTreeMap::new();
        let mut decoded_bytes = 0_u64;
        for item in &self.items {
            let input = by_id.remove(&item.id).ok_or(CoreError::InvalidArgument(
                "subpalette cache input omits a catalog item",
            ))?;
            let raster = decode_common_raster(input.format, input.bytes)?;
            decoded_bytes = decoded_bytes
                .checked_add(raster.pixels.len() as u64)
                .filter(|total| *total <= MAX_SUBPALETTE_CACHE_BYTES)
                .ok_or(CoreError::InvalidArgument(
                    "subpalette decoded cache exceeds its aggregate byte bound",
                ))?;
            images.insert(
                item.id,
                ReferenceImage::memory(raster, self.next_image_revision.get())?,
            );
        }
        if !by_id.is_empty() {
            return Err(CoreError::InvalidArgument(
                "subpalette cache input contains an unknown item",
            ));
        }

        let active = images.get(&active_item_id).ok_or(CoreError::InvalidState(
            "subpalette active image is missing",
        ))?;
        let view = ReferenceView::fitted(active.size(), self.viewport_width, self.viewport_height)?;
        self.images = images;
        self.view = view;
        self.next_image_revision = RenderRevision::from_raw(next_revision);
        self.active_index = Some(active_index);
        self.cache_complete = true;
        Ok(self.info())
    }

    /// Atomically installs a complete set of manager-owned decoded image leases.
    ///
    /// All names, identities, pixel layouts, and aggregate decoded bytes are
    /// validated before replacing the old catalog. The first fitted view also
    /// reserves and builds its display pixels before publication, so an old
    /// catalog and retained snapshots remain valid when that budget is exhausted.
    /// No encoded input is copied,
    /// no image is decoded again, and no editable Core/Genesis is created. The
    /// leases remain charged to their shared I/O manager for this catalog's
    /// lifetime. The first naturally ordered image is selected and fitted.
    pub fn replace_loaded_images(
        &mut self,
        images: Vec<inkpod_io::LoadedImage>,
    ) -> Result<SubpaletteCatalogInfo, CoreError> {
        if images.is_empty() || images.len() > MAX_SUBPALETTE_ITEMS {
            return Err(CoreError::InvalidArgument(
                "subpalette source count is outside bounds",
            ));
        }
        let mut identities = BTreeSet::new();
        let mut decoded_bytes = 0_u64;
        let mut sources = Vec::with_capacity(images.len());
        for (index, image) in images.iter().enumerate() {
            if !identities.insert(image.identity()) {
                return Err(CoreError::InvalidArgument(
                    "subpalette source identity is duplicated",
                ));
            }
            image.raster().validate()?;
            decoded_bytes = decoded_bytes
                .checked_add(image.raster().pixels.len() as u64)
                .filter(|bytes| *bytes <= MAX_SUBPALETTE_CACHE_BYTES)
                .ok_or(CoreError::InvalidArgument(
                    "subpalette decoded cache exceeds its aggregate byte bound",
                ))?;
            sources.push(SubpaletteSource {
                source_token: index as u64 + 1,
                name: image.name().to_owned(),
            });
        }
        let mut staged = Self::new()?;
        staged.next_item_id = self.next_item_id;
        staged.catalog_revision = self.catalog_revision;
        staged.next_image_revision = self.next_image_revision;
        staged.viewport_width = self.viewport_width;
        staged.viewport_height = self.viewport_height;
        staged.replace_sources(sources)?;
        let next_revision =
            staged
                .next_image_revision
                .get()
                .checked_add(1)
                .ok_or(CoreError::InvalidState(
                    "reference image revision overflows",
                ))?;
        let ids_by_token: BTreeMap<_, _> = staged
            .items
            .iter()
            .map(|item| (item.source_token, item.id))
            .collect();
        for (index, image) in images.into_iter().enumerate() {
            let id =
                ids_by_token
                    .get(&(index as u64 + 1))
                    .copied()
                    .ok_or(CoreError::InvalidState(
                        "subpalette staged source disappeared",
                    ))?;
            staged.images.insert(
                id,
                ReferenceImage::managed(image, staged.next_image_revision.get())?,
            );
        }
        let first = staged
            .images
            .get(&staged.items[0].id)
            .ok_or(CoreError::InvalidState(
                "subpalette staged first image disappeared",
            ))?;
        staged.view =
            ReferenceView::fitted(first.size(), staged.viewport_width, staged.viewport_height)?;
        // A source-only reservation is insufficient: the old renderer snapshot
        // can retain its display pixels while the new catalog is being staged.
        // Keep the prepared tiles in the staged view and publish only when its
        // first snapshot can be built inside the same global pixel budget.
        staged.view.snapshot(0, first)?;
        staged.active_index = Some(0);
        staged.cache_complete = true;
        staged.next_image_revision = RenderRevision::from_raw(next_revision);
        *self = staged;
        Ok(self.info())
    }

    /// Selects one already-decoded cache item without reading or decoding bytes.
    ///
    /// Selection fits the private view to the newly active image and prepares
    /// its first snapshot before publishing. Invalid IDs, an incomplete cache,
    /// or insufficient display-pixel budget preserve the previous item and view.
    pub fn select_cached_image(
        &mut self,
        item_id: SubpaletteItemId,
    ) -> Result<SubpaletteCatalogInfo, CoreError> {
        if !self.cache_complete {
            return Err(CoreError::InvalidState(
                "subpalette decoded cache is incomplete",
            ));
        }
        let index = self
            .items
            .iter()
            .position(|item| item.id == item_id)
            .ok_or(CoreError::InvalidArgument(
                "subpalette cached item ID does not exist",
            ))?;
        if self.active_index == Some(index) {
            return Ok(self.info());
        }

        let image = self.images.get(&item_id).ok_or(CoreError::InvalidState(
            "subpalette cached image is missing",
        ))?;
        let view =
            self.view
                .prepare_selection(index, image, self.viewport_width, self.viewport_height)?;
        self.view = view;
        self.active_index = Some(index);
        Ok(self.info())
    }

    /// Applies a private view-only command to the loaded image.
    pub fn apply_view(&mut self, command: ViewCommand) -> Result<ViewState, CoreError> {
        if self.active_index.is_none() {
            return Err(CoreError::InvalidState("subpalette has no decoded image"));
        }
        let image = self.active_image()?;
        let size = image.size();
        let view = self.view.apply(command, size)?;
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
            } => {
                self.viewport_width = viewport_width;
                self.viewport_height = viewport_height;
            }
            _ => {}
        }
        Ok(view)
    }

    /// Samples exact native-depth color through the private device-pixel view.
    pub fn sample(&self, device_x: f64, device_y: f64) -> Result<PixelValue, CoreError> {
        if self.active_index.is_none() {
            return Err(CoreError::InvalidState("subpalette has no decoded image"));
        }
        self.view.sample(self.active_image()?, device_x, device_y)
    }

    /// Builds an immutable read-only snapshot of the loaded image.
    ///
    /// For manager-loaded images, converted tile pixels are reserved against the
    /// shared decoded budget before allocation. Snapshot and individual tile
    /// clones retain that charge after cache/catalog release. A full budget
    /// returns an error without publishing a partial snapshot.
    pub fn build_snapshot(&mut self) -> Result<RenderSnapshot, CoreError> {
        if self.active_index.is_none() {
            return Err(CoreError::InvalidState("subpalette has no decoded image"));
        }
        let index = self
            .active_index
            .ok_or(CoreError::InvalidState("subpalette has no decoded image"))?;
        let image = self
            .images
            .get(&self.items[index].id)
            .ok_or(CoreError::InvalidState(
                "subpalette active image disappeared",
            ))?;
        self.view.snapshot(index, image)
    }

    fn active_image(&self) -> Result<&ReferenceImage, CoreError> {
        let index = self
            .active_index
            .ok_or(CoreError::InvalidState("subpalette has no decoded image"))?;
        self.images
            .get(&self.items[index].id)
            .ok_or(CoreError::InvalidState(
                "subpalette active image disappeared",
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_DPI_MILLI, PixelFormat, SequenceDirection};
    use inkpod_format::{CommonRaster, encode_common_raster};

    fn png(pixel: [u8; 4]) -> Vec<u8> {
        let raster = CommonRaster::new(
            1,
            1,
            PixelFormat::StraightRgba8,
            Some(DEFAULT_DPI_MILLI),
            Some(DEFAULT_DPI_MILLI),
            pixel.to_vec(),
        )
        .unwrap();
        encode_common_raster(CommonRasterFormat::Png, &raster, false).unwrap()
    }

    #[test]
    fn catalog_orders_cell_numbers_before_unnumbered_names() {
        let mut catalog = SubpaletteCatalog::new().unwrap();
        catalog
            .replace_sources(vec![
                SubpaletteSource {
                    source_token: 1,
                    name: "palette.png".into(),
                },
                SubpaletteSource {
                    source_token: 2,
                    name: "cell10.png".into(),
                },
                SubpaletteSource {
                    source_token: 3,
                    name: "cell2.png".into(),
                },
            ])
            .unwrap();
        assert_eq!(catalog.item(0).unwrap().cell_number, Some(2));
        assert_eq!(catalog.item(1).unwrap().cell_number, Some(10));
        assert_eq!(catalog.item(2).unwrap().cell_number, None);
        assert_eq!(
            catalog.adjacent_item(SequenceDirection::Next).unwrap().name,
            "cell2.png"
        );
    }

    #[test]
    fn replacement_failure_is_atomic_and_does_not_consume_ids() {
        let mut catalog = SubpaletteCatalog::new().unwrap();
        catalog
            .replace_sources(vec![SubpaletteSource {
                source_token: 7,
                name: "cell1.png".into(),
            }])
            .unwrap();
        let before = catalog.info();
        let first_id = catalog.item(0).unwrap().id;
        assert!(
            catalog
                .replace_sources(vec![
                    SubpaletteSource {
                        source_token: 9,
                        name: "cell2.png".into(),
                    },
                    SubpaletteSource {
                        source_token: 9,
                        name: "cell3.png".into(),
                    },
                ])
                .is_err()
        );
        assert_eq!(catalog.info(), before);
        assert_eq!(catalog.item(0).unwrap().id, first_id);
        catalog
            .replace_sources(vec![SubpaletteSource {
                source_token: 10,
                name: "cell4.png".into(),
            }])
            .unwrap();
        assert_eq!(catalog.item(0).unwrap().id.get(), first_id.get() + 1);
    }

    #[test]
    fn decode_sample_view_and_failure_preserve_the_loaded_image() {
        let mut catalog = SubpaletteCatalog::new().unwrap();
        catalog
            .replace_sources(vec![SubpaletteSource {
                source_token: 1,
                name: "sample.png".into(),
            }])
            .unwrap();
        let item = catalog.item(0).unwrap().id;
        catalog
            .load_image(item, CommonRasterFormat::Png, png([10, 20, 30, 255]))
            .unwrap();
        catalog
            .apply_view(ViewCommand::OneToOne {
                viewport_width: 1.0,
                viewport_height: 1.0,
            })
            .unwrap();
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba([10, 20, 30, 255])
        );
        let before = catalog.info();
        assert!(
            catalog
                .load_image(item, CommonRasterFormat::Png, b"not png".to_vec())
                .is_err()
        );
        assert_eq!(catalog.info(), before);
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba([10, 20, 30, 255])
        );
        assert_eq!(catalog.build_snapshot().unwrap().document_width(), 1);
    }

    #[test]
    fn exact_depth_reference_sampling_and_visible_tiles_follow_the_shared_view_transform() {
        let pixels: Vec<_> = (0_u16..130)
            .flat_map(|x| [x + 1, x * 257, 65_534, 65_535])
            .flat_map(u16::to_le_bytes)
            .collect();
        let raster =
            CommonRaster::new(130, 1, PixelFormat::StraightRgba16, None, None, pixels).unwrap();
        let encoded = encode_common_raster(CommonRasterFormat::Png, &raster, false).unwrap();
        let mut catalog = SubpaletteCatalog::new().unwrap();
        catalog
            .replace_sources(vec![SubpaletteSource {
                source_token: 1,
                name: "reference16.png".into(),
            }])
            .unwrap();
        let item = catalog.item(0).unwrap().id;
        catalog
            .load_image(item, CommonRasterFormat::Png, encoded)
            .unwrap();
        catalog
            .apply_view(ViewCommand::OneToOne {
                viewport_width: 2.0,
                viewport_height: 1.0,
            })
            .unwrap();
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba16([65, 64 * 257, 65_534, 65_535])
        );
        let middle = catalog.build_snapshot().unwrap();
        assert_eq!(middle.tiles().len(), 1);
        assert_eq!(middle.tiles()[0].origin_x(), 64);
        catalog
            .apply_view(ViewCommand::PanBy {
                device_dx: 64.0,
                device_dy: 0.0,
            })
            .unwrap();
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba16([1, 0, 65_534, 65_535])
        );
        assert!(catalog.sample(130.0, 0.5).is_err());
        assert!(catalog.sample(f64::NAN, 0.5).is_err());
        catalog
            .apply_view(ViewCommand::Flip {
                axis: crate::MirrorAxis::Horizontal,
            })
            .unwrap();
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba16([130, 129 * 257, 65_534, 65_535])
        );
        let flipped = catalog.build_snapshot().unwrap();
        assert_eq!(flipped.tiles().len(), 1);
        assert_eq!(flipped.tiles()[0].origin_x(), 128);
        assert_eq!(flipped.tiles()[0].width(), 2);
        assert_eq!(middle.tiles()[0].origin_x(), 64);
        let before = flipped.view();
        assert!(
            catalog
                .apply_view(ViewCommand::ZoomAt {
                    factor: f64::NAN,
                    device_x: 0.5,
                    device_y: 0.5,
                })
                .is_err()
        );
        assert_eq!(catalog.build_snapshot().unwrap().view(), before);
    }

    #[test]
    fn managed_reference_replacement_is_atomic_and_keeps_cache_leases_charged() {
        let manager = inkpod_io::IoManager::new(inkpod_io::IoConfig {
            max_images: 2,
            // Two source pixels, old/new visible pixels, and the transient Arc copy.
            max_decoded_bytes: 20,
            worker_count: 1,
            ..inkpod_io::IoConfig::default()
        })
        .unwrap();
        let context = inkpod_io::JobContext::new();
        let temporary = manager
            .create_temporary_directory("subpalette-leases", &context)
            .unwrap();
        let later_path = temporary.path().join("cell10.png");
        let earlier_path = temporary.path().join("cell2.png");
        manager
            .write_bytes_atomic(&later_path, &png([10, 20, 30, 255]), &context)
            .unwrap();
        manager
            .write_bytes_atomic(&earlier_path, &png([40, 50, 60, 255]), &context)
            .unwrap();
        let later = manager.read_image(&later_path, &context).unwrap();
        let earlier = manager.read_image(&earlier_path, &context).unwrap();
        let duplicate = later.clone();
        let pressure = later.reserve_derived(4).unwrap();
        let mut catalog = SubpaletteCatalog::new().unwrap();
        catalog.replace_loaded_images(vec![later, earlier]).unwrap();
        assert_eq!(catalog.item(0).unwrap().source_token, 2);
        assert_eq!(catalog.item(1).unwrap().source_token, 1);
        let before = catalog.info();
        assert!(
            catalog
                .replace_loaded_images(vec![duplicate.clone(), duplicate])
                .is_err()
        );
        assert_eq!(catalog.info(), before);
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba([40, 50, 60, 255])
        );
        catalog
            .apply_view(ViewCommand::Flip {
                axis: crate::MirrorAxis::Horizontal,
            })
            .unwrap();
        let snapshot = catalog.build_snapshot().unwrap();
        manager.clear_cache();
        let pinned = manager.cache_stats();
        assert_eq!(pinned.cached_images, 0);
        assert_eq!(pinned.images, 2);
        assert_eq!(pinned.decoded_bytes, 16);
        let selected = catalog.info();
        let next_id = catalog.item(1).unwrap().id;
        assert!(matches!(
            catalog.select_cached_image(next_id),
            Err(CoreError::InvalidState(_))
        ));
        assert_eq!(catalog.info(), selected);
        assert_eq!(catalog.build_snapshot().unwrap(), snapshot);
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba([40, 50, 60, 255])
        );
        assert_eq!(manager.cache_stats().decoded_bytes, 16);
        drop(pressure);
        catalog.select_cached_image(next_id).unwrap();
        assert_eq!(catalog.info().active_index, Some(1));
        assert!(catalog.build_snapshot().unwrap().view().flip_horizontal);
        assert_eq!(manager.cache_stats().physical_reads, pinned.physical_reads);
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba([10, 20, 30, 255])
        );
        catalog.clear().unwrap();
        assert_eq!(manager.cache_stats().images, 1);
        assert_eq!(manager.cache_stats().decoded_bytes, 4);
        assert_eq!(snapshot.tiles()[0].pixels(), &[60, 50, 40, 255]);
        drop(snapshot);
        assert_eq!(manager.cache_stats().images, 0);
        assert_eq!(manager.cache_stats().decoded_bytes, 0);
        temporary.cleanup().unwrap();
    }

    #[test]
    fn complete_cache_switches_after_encoded_inputs_are_dropped() {
        let mut catalog = SubpaletteCatalog::new().unwrap();
        catalog
            .replace_sources(vec![
                SubpaletteSource {
                    source_token: 1,
                    name: "cell2.png".into(),
                },
                SubpaletteSource {
                    source_token: 2,
                    name: "cell1.png".into(),
                },
            ])
            .unwrap();
        let first = catalog.item(0).unwrap().id;
        let second = catalog.item(1).unwrap().id;
        let first_bytes = png([10, 20, 30, 255]);
        let second_bytes = png([40, 50, 60, 255]);
        let loaded = catalog
            .load_cached_images(
                &[
                    SubpaletteImageInput {
                        item_id: second,
                        format: CommonRasterFormat::Png,
                        bytes: &second_bytes,
                    },
                    SubpaletteImageInput {
                        item_id: first,
                        format: CommonRasterFormat::Png,
                        bytes: &first_bytes,
                    },
                ],
                first,
            )
            .unwrap();
        assert!(loaded.cache_complete);
        assert_eq!(loaded.active_index, Some(0));
        drop(first_bytes);
        drop(second_bytes);

        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba([10, 20, 30, 255])
        );
        let first_snapshot = catalog.build_snapshot().unwrap();
        let first_tile_id = first_snapshot.tiles()[0].tile_id();
        let first_tile_revision = first_snapshot.tiles()[0].tile_revision();
        let selected = catalog.select_cached_image(second).unwrap();
        assert_eq!(selected.active_index, Some(1));
        assert!(selected.cache_complete);
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba([40, 50, 60, 255])
        );
        let second_snapshot = catalog.build_snapshot().unwrap();
        assert_ne!(second_snapshot.tiles()[0].tile_id(), first_tile_id);
        catalog.select_cached_image(first).unwrap();
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba([10, 20, 30, 255])
        );
        let repeated = catalog.build_snapshot().unwrap();
        assert_eq!(repeated.tiles()[0].tile_id(), first_tile_id);
        assert_eq!(repeated.tiles()[0].tile_revision(), first_tile_revision);
    }

    #[test]
    fn complete_cache_failure_preserves_previous_cache_and_selection() {
        let mut catalog = SubpaletteCatalog::new().unwrap();
        catalog
            .replace_sources(vec![
                SubpaletteSource {
                    source_token: 1,
                    name: "cell1.png".into(),
                },
                SubpaletteSource {
                    source_token: 2,
                    name: "cell2.png".into(),
                },
            ])
            .unwrap();
        let first = catalog.item(0).unwrap().id;
        let second = catalog.item(1).unwrap().id;
        let first_bytes = png([1, 2, 3, 255]);
        let second_bytes = png([4, 5, 6, 255]);
        catalog
            .load_cached_images(
                &[
                    SubpaletteImageInput {
                        item_id: first,
                        format: CommonRasterFormat::Png,
                        bytes: &first_bytes,
                    },
                    SubpaletteImageInput {
                        item_id: second,
                        format: CommonRasterFormat::Png,
                        bytes: &second_bytes,
                    },
                ],
                second,
            )
            .unwrap();
        let before = catalog.info();
        assert!(
            catalog
                .load_cached_images(
                    &[
                        SubpaletteImageInput {
                            item_id: first,
                            format: CommonRasterFormat::Png,
                            bytes: &first_bytes,
                        },
                        SubpaletteImageInput {
                            item_id: second,
                            format: CommonRasterFormat::Png,
                            bytes: b"not png",
                        },
                    ],
                    first,
                )
                .is_err()
        );
        assert_eq!(catalog.info(), before);
        assert_eq!(
            catalog.sample(0.5, 0.5).unwrap(),
            PixelValue::Rgba([4, 5, 6, 255])
        );
    }
}
