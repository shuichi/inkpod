use crate::animation::{MAX_SEQUENCE_CELLS, natural_cmp, parse_cell_number};
use crate::document::validate_node_name;
use crate::{
    CommonRasterFormat, Core, CoreError, DEFAULT_DPI_MILLI, PixelValue, RenderSnapshot,
    ViewCommand, ViewState,
};
use std::cmp::Ordering;
use std::collections::BTreeSet;

/// Maximum number of external image entries accepted by one subpalette catalog.
pub const MAX_SUBPALETTE_ITEMS: usize = MAX_SEQUENCE_CELLS;

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
}

/// Workspace-scoped, read-only external-image catalog used by the subpalette.
///
/// The catalog owns no OS path and never changes a user document, history,
/// dirty state, savepoint, or replay state. Source replacement and image decode
/// publish atomically. Zoom, pan, and viewport changes affect only the private
/// view. The type is single-writer and must be externally thread-affined by its
/// frontend or ABI owner.
pub struct SubpaletteCatalog {
    core: Core,
    view_id: u64,
    items: Vec<SubpaletteItem>,
    next_item_id: u64,
    catalog_revision: u64,
    active_index: Option<usize>,
    viewport_width: f64,
    viewport_height: f64,
}

impl SubpaletteCatalog {
    fn empty_core(viewport_width: f64, viewport_height: f64) -> Result<(Core, u64), CoreError> {
        let mut core = Core::new();
        core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)?;
        let view_id = core.create_view()?;
        core.apply_view_for(
            view_id,
            ViewCommand::ViewportResized {
                viewport_width,
                viewport_height,
            },
        )?;
        Ok((core, view_id))
    }

    /// Creates an empty catalog with one private read-only view.
    ///
    /// The internal one-pixel document is an implementation detail used only to
    /// host the existing view machinery; it is never exposed or mutated by
    /// subpalette operations.
    pub fn new() -> Result<Self, CoreError> {
        let (core, view_id) = Self::empty_core(1.0, 1.0)?;
        Ok(Self {
            core,
            view_id,
            items: Vec::new(),
            next_item_id: 1,
            catalog_revision: 0,
            active_index: None,
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
        let (core, view_id) = Self::empty_core(self.viewport_width, self.viewport_height)?;

        self.items = items;
        self.core = core;
        self.view_id = view_id;
        self.next_item_id = next_item_id;
        self.catalog_revision = next_revision;
        self.active_index = None;
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
        let (core, view_id) = Self::empty_core(self.viewport_width, self.viewport_height)?;
        self.catalog_revision = next_revision;
        self.items.clear();
        self.active_index = None;
        self.core = core;
        self.view_id = view_id;
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

        let mut staged_core = Core::new();
        staged_core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)?;
        let staged_view_id = staged_core.create_view()?;
        let synthetic_name = format!("subpalette-{}.png", item_id.get());
        staged_core.import_sequence(format, vec![(synthetic_name, bytes)])?;
        staged_core.set_subpalette_cell(0)?;
        staged_core.apply_subpalette_view_for(
            staged_view_id,
            ViewCommand::ViewportResized {
                viewport_width: self.viewport_width,
                viewport_height: self.viewport_height,
            },
        )?;
        staged_core.apply_subpalette_view_for(
            staged_view_id,
            ViewCommand::Fit {
                viewport_width: self.viewport_width,
                viewport_height: self.viewport_height,
            },
        )?;

        self.core = staged_core;
        self.view_id = staged_view_id;
        self.active_index = Some(index);
        Ok(self.info())
    }

    /// Applies a private view-only command to the loaded image.
    pub fn apply_view(&mut self, command: ViewCommand) -> Result<ViewState, CoreError> {
        if self.active_index.is_none() {
            return Err(CoreError::InvalidState("subpalette has no decoded image"));
        }
        let view = self.core.apply_subpalette_view_for(self.view_id, command)?;
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
        self.core
            .subpalette_view_sample(self.view_id, device_x, device_y)
    }

    /// Builds an immutable read-only snapshot of the loaded image.
    pub fn build_snapshot(&self) -> Result<RenderSnapshot, CoreError> {
        if self.active_index.is_none() {
            return Err(CoreError::InvalidState("subpalette has no decoded image"));
        }
        self.core.build_subpalette_snapshot_for(self.view_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PixelFormat, SequenceDirection};
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
}
