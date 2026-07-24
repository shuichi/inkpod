use super::*;
pub use inkpod_format::LightTableDisplayMode;
use inkpod_format::{
    FileLightTableItem, FileLightTableSet, FileM4Metadata, decode_common_raster,
    encode_common_raster,
};
use std::cmp::Ordering;

const MAX_SEQUENCE_CELLS: usize = 10_000;
const MAX_LIGHT_TABLE_SETS: usize = 256;
const MAX_LIGHT_TABLE_ITEMS: usize = 4_096;
const THUMBNAIL_MAX_DIMENSION: u32 = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbaRasterBytes {
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub dpi_x_milli: Option<u32>,
    pub dpi_y_milli: Option<u32>,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LightTableSource {
    pub document_uuid: u128,
    pub source_revision: u64,
    pub reference_frame: RectI32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    raster: TileRaster,
}

impl LightTableSource {
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

    pub fn from_common_raster(
        document_uuid: u128,
        source_revision: u64,
        reference_frame: RectI32,
        raster: &CommonRaster,
    ) -> Result<Self, CoreError> {
        if document_uuid == 0 || source_revision == 0 {
            return Err(CoreError::InvalidArgument(
                "light-table source identity is invalid",
            ));
        }
        validate_reference_frame(reference_frame)?;
        let tile_raster = common_to_tile_raster(raster, source_revision)?;
        Ok(Self {
            document_uuid,
            source_revision,
            reference_frame,
            dpi_x_milli: raster.info.dpi_x_milli.unwrap_or(DEFAULT_DPI_MILLI),
            dpi_y_milli: raster.info.dpi_y_milli.unwrap_or(DEFAULT_DPI_MILLI),
            raster: tile_raster,
        })
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.raster.width()
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.raster.height()
    }

    #[must_use]
    pub const fn pixel_format(&self) -> PixelFormat {
        self.raster.format()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LightTableItemInput {
    pub name: String,
    pub source: LightTableSource,
    pub visible: bool,
    pub opacity_milli: u32,
    pub display_mode: LightTableDisplayMode,
    pub display_color: PixelValue,
    pub translate_x_milli: i32,
    pub translate_y_milli: i32,
    pub scale_x_milli: u32,
    pub scale_y_milli: u32,
    pub rotation_milli_degrees: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LightTableItemProperties {
    pub visible: bool,
    pub opacity_milli: u32,
    pub display_mode: LightTableDisplayMode,
    pub display_color: PixelValue,
    pub translate_x_milli: i32,
    pub translate_y_milli: i32,
    pub scale_x_milli: u32,
    pub scale_y_milli: u32,
    pub rotation_milli_degrees: i32,
}

impl LightTableItemInput {
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
pub struct LightTableItemInfo {
    pub id: u64,
    pub source_plane_id: u64,
    pub name: String,
    pub source_document_uuid: u128,
    pub source_revision: u64,
    pub visible: bool,
    pub opacity_milli: u32,
    pub effective_opacity_milli: u32,
    pub display_mode: LightTableDisplayMode,
    pub display_color: PixelValue,
    pub translate_x_milli: i32,
    pub translate_y_milli: i32,
    pub scale_x_milli: u32,
    pub scale_y_milli: u32,
    pub rotation_milli_degrees: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LightTableSetInfo {
    pub id: u64,
    pub name: String,
    pub active: bool,
    pub global_opacity_milli: u32,
    pub item_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LightTableItem {
    id: u64,
    source_plane_id: u64,
    name: String,
    source: LightTableSource,
    visible: bool,
    opacity_milli: u32,
    display_mode: LightTableDisplayMode,
    display_color: PixelValue,
    translate_x_milli: i32,
    translate_y_milli: i32,
    scale_x_milli: u32,
    scale_y_milli: u32,
    rotation_milli_degrees: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LightTableSet {
    id: u64,
    name: String,
    global_opacity_milli: u32,
    items: Vec<LightTableItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LightTableState {
    active_set_id: u64,
    sets: Vec<LightTableSet>,
}

impl LightTableState {
    pub(crate) fn new(default_set_id: u64) -> Self {
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

    fn active(&self) -> Option<&LightTableSet> {
        self.sets.iter().find(|set| set.id == self.active_set_id)
    }

    fn active_mut(&mut self) -> Option<&mut LightTableSet> {
        self.sets
            .iter_mut()
            .find(|set| set.id == self.active_set_id)
    }

    pub(crate) fn maximum_id(&self) -> u64 {
        self.sets
            .iter()
            .flat_map(|set| {
                std::iter::once(set.id).chain(
                    set.items
                        .iter()
                        .flat_map(|item| [item.id, item.source_plane_id]),
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

    pub(crate) fn source_revision(&self) -> u64 {
        self.active()
            .into_iter()
            .flat_map(|set| set.items.iter())
            .filter(|item| item.visible)
            .map(|item| item.source.source_revision)
            .max()
            .unwrap_or(0)
    }

    fn item_count(&self) -> usize {
        self.sets.iter().map(|set| set.items.len()).sum()
    }

    pub(crate) fn file_planes(&self) -> Vec<FilePlane> {
        self.sets
            .iter()
            .flat_map(|set| set.items.iter())
            .map(|item| {
                raster_to_file_plane(
                    item.source_plane_id,
                    FilePlaneKind::LightTable,
                    &item.source.raster,
                )
            })
            .collect()
    }

    pub(crate) fn to_file(&self) -> FileM4Metadata {
        FileM4Metadata {
            active_set_id: self.active_set_id,
            sets: self
                .sets
                .iter()
                .map(|set| FileLightTableSet {
                    id: set.id,
                    name: set.name.clone(),
                    global_opacity_milli: set.global_opacity_milli,
                    items: set
                        .items
                        .iter()
                        .map(|item| FileLightTableItem {
                            id: item.id,
                            source_plane_id: item.source_plane_id,
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
        metadata: Option<&FileM4Metadata>,
        planes: &[FilePlane],
        revision: u64,
        legacy_set_id: u64,
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
                    id: item.id,
                    source_plane_id: item.source_plane_id,
                    name: item.name.clone(),
                    source: LightTableSource {
                        document_uuid: u128::from_le_bytes(item.source_document_uuid),
                        source_revision: item.source_revision,
                        reference_frame: item.source_reference_frame,
                        dpi_x_milli: item.source_dpi_x_milli,
                        dpi_y_milli: item.source_dpi_y_milli,
                        raster: file_plane_to_raster(plane, revision)?,
                    },
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
                id: set.id,
                name: set.name.clone(),
                global_opacity_milli: set.global_opacity_milli,
                items,
            });
        }
        Ok(Self {
            active_set_id: metadata.active_set_id,
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

fn validate_item_input(input: &LightTableItemInput) -> Result<(), CoreError> {
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

fn validate_light_table_source(source: &LightTableSource) -> Result<(), CoreError> {
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

fn unique_light_table_set_name(sets: &[LightTableSet], requested: &str) -> String {
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

const fn effective_opacity(item: u32, global: u32) -> u32 {
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
    let destination_x = f64::from(x)
        - f64::from(destination_reference.x)
        - f64::from(item.translate_x_milli) / 1_000.0;
    let destination_y = f64::from(y)
        - f64::from(destination_reference.y)
        - f64::from(item.translate_y_milli) / 1_000.0;
    let radians = -f64::from(item.rotation_milli_degrees) / 1_000.0 * std::f64::consts::PI / 180.0;
    let cosine = radians.cos();
    let sine = radians.sin();
    let rotated_x = destination_x * cosine - destination_y * sine;
    let rotated_y = destination_x * sine + destination_y * cosine;
    let source_x = f64::from(item.source.reference_frame.x)
        + rotated_x * 1_000.0 / f64::from(item.scale_x_milli);
    let source_y = f64::from(item.source.reference_frame.y)
        + rotated_y * 1_000.0 / f64::from(item.scale_y_milli);
    if !source_x.is_finite() || !source_y.is_finite() {
        return Err(CoreError::InvalidState(
            "light-table transform produced a non-finite coordinate",
        ));
    }
    let source_x = source_x.round() as i64;
    let source_y = source_y.round() as i64;
    if source_x < 0
        || source_y < 0
        || source_x >= i64::from(item.source.width())
        || source_y >= i64::from(item.source.height())
    {
        return Ok(None);
    }
    let value = item.source.raster.pixel(source_x as u32, source_y as u32)?;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
    pub checksum: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequenceCellSource {
    pub name: String,
    pub cell_number: u32,
    pub document_uuid: u128,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub frames: FrameMetadata,
    raster: TileRaster,
}

impl SequenceCellSource {
    pub fn from_rgba_bytes(
        name: impl Into<String>,
        document_uuid: u128,
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
        Self::from_common_raster(name, document_uuid, &raster)
    }

    pub fn from_common_raster(
        name: impl Into<String>,
        document_uuid: u128,
        raster: &CommonRaster,
    ) -> Result<Self, CoreError> {
        let name = name.into();
        validate_node_name(&name)?;
        let cell_number = parse_cell_number(&name).ok_or(CoreError::InvalidArgument(
            "sequence name has no cell number",
        ))?;
        if document_uuid == 0 {
            return Err(CoreError::InvalidArgument(
                "sequence document UUID must be nonzero",
            ));
        }
        let width = raster.info.width;
        let height = raster.info.height;
        let reference_frame = RectI32 {
            x: (width / 2) as i32,
            y: (height / 2) as i32,
            width: width as i32,
            height: height as i32,
        };
        let full = RectI32 {
            x: 0,
            y: 0,
            width: width as i32,
            height: height as i32,
        };
        Ok(Self {
            name,
            cell_number,
            document_uuid,
            dpi_x_milli: raster.info.dpi_x_milli.unwrap_or(DEFAULT_DPI_MILLI),
            dpi_y_milli: raster.info.dpi_y_milli.unwrap_or(DEFAULT_DPI_MILLI),
            frames: FrameMetadata {
                hundred_frame: full,
                reference_frame,
                drawing_frame: full,
                safe_frame: full,
                margins: Margins::default(),
            },
            raster: common_to_tile_raster(raster, 1)?,
        })
    }

    pub fn thumbnail(&self) -> Result<Thumbnail, CoreError> {
        thumbnail_for_raster(&self.raster)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequenceCellInfo {
    pub name: String,
    pub cell_number: u32,
    pub document_uuid: u128,
    pub width: u32,
    pub height: u32,
    pub thumbnail: Thumbnail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SequenceState {
    cells: Vec<SequenceCellSource>,
    active_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SequenceDirection {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MotionCheckConfig {
    pub fps: u32,
    pub loop_playback: bool,
    pub include_selection: bool,
    pub include_light_table: bool,
}

impl Default for MotionCheckConfig {
    fn default() -> Self {
        Self {
            fps: 24,
            loop_playback: true,
            include_selection: false,
            include_light_table: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MotionFrame {
    pub sequence_index: usize,
    pub cell_number: u32,
    pub name: String,
    pub thumbnail: Thumbnail,
    pub paused: bool,
    pub fps: u32,
    pub include_selection: bool,
    pub include_light_table: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MotionCheckState {
    config: MotionCheckConfig,
    index: usize,
    paused: bool,
}

impl Core {
    pub fn import_common_raster(
        &mut self,
        format: CommonRasterFormat,
        bytes: &[u8],
        document_uuid: u128,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if document_uuid == 0 {
            return Err(CoreError::InvalidArgument(
                "common-raster document UUID must be nonzero",
            ));
        }
        let raster = decode_common_raster(format, bytes)?;
        let ids = DocumentIds {
            document: self.allocate_id(),
            layer: self.allocate_id(),
            main_plane: self.allocate_id(),
            color_plane: self.allocate_id(),
            selection_plane: self.allocate_id(),
            light_table_set: self.allocate_id(),
        };
        let mut document = CellDocument::new(
            ids,
            document_uuid,
            PaperSpec {
                width: raster.info.width,
                height: raster.info.height,
                dpi_x_milli: raster.info.dpi_x_milli.unwrap_or(DEFAULT_DPI_MILLI),
                dpi_y_milli: raster.info.dpi_y_milli.unwrap_or(DEFAULT_DPI_MILLI),
            },
        )?;
        document.plane_for_role_mut(ActivePlane::Color)?.raster =
            common_to_tile_raster(&raster, self.document_revision.max(1))?;
        let revision = self.next_document_revision()?;
        self.document = Some(document);
        self.document_revision = revision;
        self.render_cache.clear();
        self.reset_history(true);
        self.reset_view();
        self.current_path = None;
        self.recovered = false;
        self.floating = None;
        self.motion_check = None;
        self.sequence = None;
        self.subpalette_index = None;
        self.document_info()
    }

    pub fn export_common_raster(
        &self,
        format: CommonRasterFormat,
        composite_white: bool,
    ) -> Result<Vec<u8>, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let flattened = flatten_document(document, self.document_revision.max(1))?;
        let raster = tile_to_common(
            &flattened,
            Some(document.dpi_x_milli),
            Some(document.dpi_y_milli),
        )?;
        Ok(encode_common_raster(format, &raster, composite_white)?)
    }

    pub fn generate_palette_from_document(
        &mut self,
        maximum_colors: usize,
        quantization_bits: u8,
    ) -> Result<DispatchOutcome, CoreError> {
        if maximum_colors == 0 || maximum_colors > inkpod_image::MAX_PALETTE_COLORS {
            return Err(CoreError::InvalidArgument(
                "generated palette color limit is invalid",
            ));
        }
        if quantization_bits > 7 {
            return Err(CoreError::InvalidArgument(
                "palette quantization must retain at least one bit",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let flattened = flatten_document(document, self.document_revision.max(1))?;
        let mask = u8::MAX << quantization_bits;
        let mut unique = BTreeSet::new();
        for y in 0..flattened.height() {
            for x in 0..flattened.width() {
                let PixelValue::Rgba(mut rgba) = flattened.pixel(x, y)? else {
                    return Err(CoreError::InvalidState(
                        "flattened palette source is not RGBA8",
                    ));
                };
                if rgba[3] == 0 {
                    continue;
                }
                rgba[0] &= mask;
                rgba[1] &= mask;
                rgba[2] &= mask;
                rgba[3] &= mask;
                unique.insert(rgba);
                if unique.len() > maximum_colors {
                    return Err(CoreError::InvalidState(
                        "generated palette exceeds the configured maximum; increase quantization",
                    ));
                }
            }
        }
        let colors = unique.into_iter().map(PixelValue::Rgba).collect::<Vec<_>>();
        self.replace_palette(&colors)
    }

    pub fn update_paper_frames(
        &mut self,
        frames: FrameMetadata,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_frames(self.document.as_ref().ok_or(CoreError::NoDocument)?, frames)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        after.frames = frames;
        self.commit_document_edit(before, after)
    }

    pub fn light_table_set_global_opacity(
        &mut self,
        opacity_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if opacity_milli > 1_000 {
            return Err(CoreError::InvalidArgument(
                "light-table opacity exceeds one thousand",
            ));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        after
            .light_table
            .active_mut()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?
            .global_opacity_milli = opacity_milli;
        self.commit_document_edit(before, after)
    }

    pub fn light_table_create_set(
        &mut self,
        name: impl Into<String>,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let name = name.into();
        validate_node_name(&name)?;
        if self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .light_table
            .sets
            .len()
            >= MAX_LIGHT_TABLE_SETS
        {
            return Err(CoreError::InvalidState(
                "light-table set count exceeds its bound",
            ));
        }
        let id = self.allocate_id();
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let name = unique_light_table_set_name(&after.light_table.sets, &name);
        after.light_table.sets.push(LightTableSet {
            id,
            name,
            global_opacity_milli: 1_000,
            items: Vec::new(),
        });
        after.light_table.active_set_id = id;
        Ok((self.commit_document_edit(before, after)?, id))
    }

    pub fn light_table_duplicate_set(
        &mut self,
        set_id: u64,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let source = before
            .light_table
            .sets
            .iter()
            .find(|set| set.id == set_id)
            .cloned()
            .ok_or(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ))?;
        if before.light_table.sets.len() >= MAX_LIGHT_TABLE_SETS
            || before
                .light_table
                .item_count()
                .checked_add(source.items.len())
                .is_none_or(|count| count > MAX_LIGHT_TABLE_ITEMS)
        {
            return Err(CoreError::InvalidState(
                "duplicated light-table content exceeds its bound",
            ));
        }
        let new_set_id = self.allocate_id();
        let mut items = Vec::with_capacity(source.items.len());
        for mut item in source.items {
            item.id = self.allocate_id();
            item.source_plane_id = self.allocate_id();
            items.push(item);
        }
        let mut after = before.clone();
        let name = unique_light_table_set_name(&after.light_table.sets, &source.name);
        after.light_table.sets.push(LightTableSet {
            id: new_set_id,
            name,
            global_opacity_milli: source.global_opacity_milli,
            items,
        });
        after.light_table.active_set_id = new_set_id;
        Ok((self.commit_document_edit(before, after)?, new_set_id))
    }

    pub fn light_table_delete_set(&mut self, set_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.light_table.sets.len() == 1 {
            return Err(CoreError::InvalidState(
                "the final light-table set cannot be deleted",
            ));
        }
        let mut after = before.clone();
        let index = after
            .light_table
            .sets
            .iter()
            .position(|set| set.id == set_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ))?;
        after.light_table.sets.remove(index);
        if after.light_table.active_set_id == set_id {
            after.light_table.active_set_id = after.light_table.sets
                [index.min(after.light_table.sets.len().saturating_sub(1))]
            .id;
        }
        self.commit_document_edit(before, after)
    }

    pub fn light_table_rename_set(
        &mut self,
        set_id: u64,
        name: impl Into<String>,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let name = name.into();
        validate_node_name(&name)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let other_sets = after
            .light_table
            .sets
            .iter()
            .filter(|set| set.id != set_id)
            .cloned()
            .collect::<Vec<_>>();
        let unique = unique_light_table_set_name(&other_sets, &name);
        after
            .light_table
            .sets
            .iter_mut()
            .find(|set| set.id == set_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ))?
            .name = unique;
        self.commit_document_edit(before, after)
    }

    pub fn light_table_reorder_set(
        &mut self,
        set_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if destination_index >= before.light_table.sets.len() {
            return Err(CoreError::InvalidArgument(
                "light-table set destination is outside bounds",
            ));
        }
        let mut after = before.clone();
        let source_index = after
            .light_table
            .sets
            .iter()
            .position(|set| set.id == set_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ))?;
        let set = after.light_table.sets.remove(source_index);
        after.light_table.sets.insert(destination_index, set);
        self.commit_document_edit(before, after)
    }

    pub fn light_table_set_active(&mut self, set_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if !before.light_table.sets.iter().any(|set| set.id == set_id) {
            return Err(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ));
        }
        let mut after = before.clone();
        after.light_table.active_set_id = set_id;
        self.commit_document_edit(before, after)
    }

    pub fn light_table_add_item(
        &mut self,
        input: LightTableItemInput,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_item_input(&input)?;
        if self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .light_table
            .item_count()
            >= MAX_LIGHT_TABLE_ITEMS
        {
            return Err(CoreError::InvalidState(
                "light-table item count exceeds its bound",
            ));
        }
        let item_id = self.allocate_id();
        let source_plane_id = self.allocate_id();
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        after
            .light_table
            .active_mut()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?
            .items
            .insert(
                0,
                LightTableItem {
                    id: item_id,
                    source_plane_id,
                    name: input.name,
                    source: input.source,
                    visible: input.visible,
                    opacity_milli: input.opacity_milli,
                    display_mode: input.display_mode,
                    display_color: input.display_color,
                    translate_x_milli: input.translate_x_milli,
                    translate_y_milli: input.translate_y_milli,
                    scale_x_milli: input.scale_x_milli,
                    scale_y_milli: input.scale_y_milli,
                    rotation_milli_degrees: input.rotation_milli_degrees,
                },
            );
        let outcome = self.commit_document_edit(before, after)?;
        Ok((outcome, item_id))
    }

    pub fn light_table_add_common_raster(
        &mut self,
        format: CommonRasterFormat,
        bytes: &[u8],
        name: impl Into<String>,
        document_uuid: u128,
        source_revision: u64,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        let raster = decode_common_raster(format, bytes)?;
        let reference_frame = RectI32 {
            x: 0,
            y: 0,
            width: i32::try_from(raster.info.width)
                .map_err(|_| CoreError::InvalidArgument("reference width exceeds i32"))?,
            height: i32::try_from(raster.info.height)
                .map_err(|_| CoreError::InvalidArgument("reference height exceeds i32"))?,
        };
        let source = LightTableSource::from_common_raster(
            document_uuid,
            source_revision,
            reference_frame,
            &raster,
        )?;
        self.light_table_add_item(LightTableItemInput::new(name, source))
    }

    pub fn light_table_update_item_properties(
        &mut self,
        item_id: u64,
        properties: LightTableItemProperties,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let item = after
            .light_table
            .active_mut()
            .and_then(|set| set.items.iter_mut().find(|item| item.id == item_id))
            .ok_or(CoreError::InvalidArgument(
                "light-table item ID does not exist",
            ))?;
        let candidate = LightTableItemInput {
            name: item.name.clone(),
            source: item.source.clone(),
            visible: properties.visible,
            opacity_milli: properties.opacity_milli,
            display_mode: properties.display_mode,
            display_color: properties.display_color,
            translate_x_milli: properties.translate_x_milli,
            translate_y_milli: properties.translate_y_milli,
            scale_x_milli: properties.scale_x_milli,
            scale_y_milli: properties.scale_y_milli,
            rotation_milli_degrees: properties.rotation_milli_degrees,
        };
        validate_item_input(&candidate)?;
        item.visible = candidate.visible;
        item.opacity_milli = candidate.opacity_milli;
        item.display_mode = candidate.display_mode;
        item.display_color = candidate.display_color;
        item.translate_x_milli = candidate.translate_x_milli;
        item.translate_y_milli = candidate.translate_y_milli;
        item.scale_x_milli = candidate.scale_x_milli;
        item.scale_y_milli = candidate.scale_y_milli;
        item.rotation_milli_degrees = candidate.rotation_milli_degrees;
        self.commit_document_edit(before, after)
    }

    pub fn light_table_reload_common_raster(
        &mut self,
        item_id: u64,
        format: CommonRasterFormat,
        bytes: &[u8],
        document_uuid: u128,
        source_revision: u64,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let raster = decode_common_raster(format, bytes)?;
        let reference_frame = RectI32 {
            x: 0,
            y: 0,
            width: i32::try_from(raster.info.width)
                .map_err(|_| CoreError::InvalidArgument("reference width exceeds i32"))?,
            height: i32::try_from(raster.info.height)
                .map_err(|_| CoreError::InvalidArgument("reference height exceeds i32"))?,
        };
        let replacement = LightTableSource::from_common_raster(
            document_uuid,
            source_revision,
            reference_frame,
            &raster,
        )?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        after
            .light_table
            .active_mut()
            .and_then(|set| set.items.iter_mut().find(|item| item.id == item_id))
            .ok_or(CoreError::InvalidArgument(
                "light-table item ID does not exist",
            ))?
            .source = replacement;
        self.commit_document_edit(before, after)
    }

    pub fn light_table_items(&self) -> Result<Vec<LightTableItemInfo>, CoreError> {
        let state = &self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .light_table;
        let set = state
            .active()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?;
        Ok(set
            .items
            .iter()
            .map(|item| LightTableItemInfo {
                id: item.id,
                source_plane_id: item.source_plane_id,
                name: item.name.clone(),
                source_document_uuid: item.source.document_uuid,
                source_revision: item.source.source_revision,
                visible: item.visible,
                opacity_milli: item.opacity_milli,
                effective_opacity_milli: effective_opacity(
                    item.opacity_milli,
                    set.global_opacity_milli,
                ),
                display_mode: item.display_mode,
                display_color: item.display_color,
                translate_x_milli: item.translate_x_milli,
                translate_y_milli: item.translate_y_milli,
                scale_x_milli: item.scale_x_milli,
                scale_y_milli: item.scale_y_milli,
                rotation_milli_degrees: item.rotation_milli_degrees,
            })
            .collect())
    }

    pub fn light_table_sets(&self) -> Result<Vec<LightTableSetInfo>, CoreError> {
        let state = &self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .light_table;
        Ok(state
            .sets
            .iter()
            .map(|set| LightTableSetInfo {
                id: set.id,
                name: set.name.clone(),
                active: set.id == state.active_set_id,
                global_opacity_milli: set.global_opacity_milli,
                item_count: set.items.len(),
            })
            .collect())
    }

    pub fn light_table_update_item(
        &mut self,
        item_id: u64,
        input: LightTableItemInput,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_item_input(&input)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let item = after
            .light_table
            .active_mut()
            .and_then(|set| set.items.iter_mut().find(|item| item.id == item_id))
            .ok_or(CoreError::InvalidArgument(
                "light-table item ID does not exist",
            ))?;
        *item = LightTableItem {
            id: item.id,
            source_plane_id: item.source_plane_id,
            name: input.name,
            source: input.source,
            visible: input.visible,
            opacity_milli: input.opacity_milli,
            display_mode: input.display_mode,
            display_color: input.display_color,
            translate_x_milli: input.translate_x_milli,
            translate_y_milli: input.translate_y_milli,
            scale_x_milli: input.scale_x_milli,
            scale_y_milli: input.scale_y_milli,
            rotation_milli_degrees: input.rotation_milli_degrees,
        };
        self.commit_document_edit(before, after)
    }

    pub fn light_table_remove_item(&mut self, item_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let items = &mut after
            .light_table
            .active_mut()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?
            .items;
        let index =
            items
                .iter()
                .position(|item| item.id == item_id)
                .ok_or(CoreError::InvalidArgument(
                    "light-table item ID does not exist",
                ))?;
        items.remove(index);
        self.commit_document_edit(before, after)
    }

    pub fn light_table_reorder_item(
        &mut self,
        item_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let items = &mut after
            .light_table
            .active_mut()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?
            .items;
        if destination_index >= items.len() {
            return Err(CoreError::InvalidArgument(
                "light-table item destination is outside bounds",
            ));
        }
        let source_index =
            items
                .iter()
                .position(|item| item.id == item_id)
                .ok_or(CoreError::InvalidArgument(
                    "light-table item ID does not exist",
                ))?;
        let item = items.remove(source_index);
        items.insert(destination_index, item);
        self.commit_document_edit(before, after)
    }

    pub fn light_table_sample(&self, x: u32, y: u32) -> Result<PixelValue, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        document
            .light_table
            .sample(document.frames.reference_frame, x, y)?
            .ok_or(CoreError::InvalidState(
                "light-table sample is transparent or unavailable",
            ))
    }

    pub fn light_table_swap_with_active(
        &mut self,
        item_id: u64,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_info()?.dirty {
            return Err(CoreError::UnsavedChanges);
        }
        let current = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let set_index = current
            .light_table
            .sets
            .iter()
            .position(|set| set.id == current.light_table.active_set_id)
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?;
        let item_index = current.light_table.sets[set_index]
            .items
            .iter()
            .position(|item| item.id == item_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table item ID does not exist",
            ))?;
        let selected_source = current.light_table.sets[set_index].items[item_index]
            .source
            .clone();
        let outgoing = LightTableSource {
            document_uuid: current.uuid,
            source_revision: self.document_revision.max(1),
            reference_frame: current.frames.reference_frame,
            dpi_x_milli: current.dpi_x_milli,
            dpi_y_milli: current.dpi_y_milli,
            raster: flatten_document(&current, self.document_revision.max(1))?,
        };
        let ids = DocumentIds {
            document: self.allocate_id(),
            layer: self.allocate_id(),
            main_plane: self.allocate_id(),
            color_plane: self.allocate_id(),
            selection_plane: self.allocate_id(),
            light_table_set: self.allocate_id(),
        };
        let mut next = CellDocument::new(
            ids,
            selected_source.document_uuid,
            PaperSpec {
                width: selected_source.width(),
                height: selected_source.height(),
                dpi_x_milli: selected_source.dpi_x_milli,
                dpi_y_milli: selected_source.dpi_y_milli,
            },
        )?;
        next.frames.reference_frame = selected_source.reference_frame;
        next.plane_for_role_mut(ActivePlane::Color)?.raster = selected_source.raster;
        next.light_table = current.light_table;
        next.light_table.sets[set_index].items[item_index].source = outgoing;

        let revision = self.next_document_revision()?;
        self.document = Some(next);
        self.document_revision = revision;
        self.render_cache.clear();
        self.reset_history(true);
        self.reset_view();
        self.current_path = None;
        self.recovered = false;
        self.floating = None;
        self.motion_check = None;
        self.document_info()
    }

    pub fn set_sequence(&mut self, mut cells: Vec<SequenceCellSource>) -> Result<(), CoreError> {
        if cells.is_empty() || cells.len() > MAX_SEQUENCE_CELLS {
            return Err(CoreError::InvalidArgument(
                "sequence cell count is outside bounds",
            ));
        }
        for cell in &cells {
            validate_sequence_cell(cell)?;
        }
        cells.sort_by(|left, right| natural_cmp(&left.name, &right.name));
        if cells
            .windows(2)
            .any(|pair| pair[0].name.eq_ignore_ascii_case(&pair[1].name))
        {
            return Err(CoreError::InvalidArgument(
                "sequence contains duplicate names",
            ));
        }
        let current_uuid = self
            .document
            .as_ref()
            .map(|document| document.uuid)
            .unwrap_or(0);
        let active_index = cells
            .iter()
            .position(|cell| cell.document_uuid == current_uuid);
        self.sequence = Some(SequenceState {
            cells,
            active_index,
        });
        self.motion_check = None;
        self.subpalette_index = None;
        Ok(())
    }

    pub fn import_sequence(
        &mut self,
        format: CommonRasterFormat,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<(), CoreError> {
        if files.is_empty() || files.len() > MAX_SEQUENCE_CELLS {
            return Err(CoreError::InvalidArgument(
                "sequence import count is outside bounds",
            ));
        }
        let mut cells = Vec::with_capacity(files.len());
        for (index, (name, bytes)) in files.into_iter().enumerate() {
            let raster = decode_common_raster(format, &bytes)?;
            let uuid = (u128::from(0x494e_4b50_4f44_5334_u64) << 64)
                | u128::try_from(index + 1)
                    .map_err(|_| CoreError::InvalidState("sequence UUID index overflows"))?;
            cells.push(SequenceCellSource::from_common_raster(name, uuid, &raster)?);
        }
        self.set_sequence(cells)
    }

    pub fn sequence_cells(&self) -> Result<Vec<SequenceCellInfo>, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        sequence
            .cells
            .iter()
            .map(|cell| {
                Ok(SequenceCellInfo {
                    name: cell.name.clone(),
                    cell_number: cell.cell_number,
                    document_uuid: cell.document_uuid,
                    width: cell.raster.width(),
                    height: cell.raster.height(),
                    thumbnail: cell.thumbnail()?,
                })
            })
            .collect()
    }

    pub fn sequence_step(
        &mut self,
        direction: SequenceDirection,
        loop_sequence: bool,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_info()?.dirty {
            return Err(CoreError::UnsavedChanges);
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let count = sequence.cells.len();
        let current = sequence.active_index.unwrap_or(match direction {
            SequenceDirection::Previous => count,
            SequenceDirection::Next => usize::MAX,
        });
        let target = match direction {
            SequenceDirection::Previous => {
                if current == 0 {
                    if loop_sequence { count - 1 } else { 0 }
                } else {
                    current.min(count) - 1
                }
            }
            SequenceDirection::Next => {
                if current == usize::MAX {
                    0
                } else if current + 1 >= count {
                    if loop_sequence { 0 } else { count - 1 }
                } else {
                    current + 1
                }
            }
        };
        self.sequence_activate(target)
    }

    pub fn sequence_activate(&mut self, target: usize) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_info()?.dirty {
            return Err(CoreError::UnsavedChanges);
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        if target >= sequence.cells.len() {
            return Err(CoreError::InvalidArgument(
                "sequence target index is outside bounds",
            ));
        }
        if sequence.active_index == Some(target) {
            return self.document_info();
        }
        let source = sequence.cells[target].clone();
        let revision = self.next_document_revision()?;
        let document = self.document_from_sequence_source(&source, revision)?;
        self.document = Some(document);
        self.document_revision = revision;
        self.render_cache.clear();
        self.reset_history(true);
        self.reset_view();
        self.current_path = None;
        self.recovered = false;
        self.floating = None;
        self.sequence
            .as_mut()
            .ok_or(CoreError::InvalidState("sequence disappeared"))?
            .active_index = Some(target);
        self.document_info()
    }

    pub fn export_sequence(
        &self,
        format: CommonRasterFormat,
        composite_white: bool,
    ) -> Result<Vec<(String, Vec<u8>)>, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        sequence
            .cells
            .iter()
            .map(|cell| {
                let raster =
                    tile_to_common(&cell.raster, Some(cell.dpi_x_milli), Some(cell.dpi_y_milli))?;
                Ok((
                    cell.name.clone(),
                    encode_common_raster(format, &raster, composite_white)?,
                ))
            })
            .collect()
    }

    pub fn set_subpalette_cell(&mut self, index: usize) -> Result<(), CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        if index >= sequence.cells.len() {
            return Err(CoreError::InvalidArgument(
                "subpalette sequence index is outside bounds",
            ));
        }
        self.subpalette_index = Some(index);
        Ok(())
    }

    pub fn subpalette_sample(&self, x: u32, y: u32) -> Result<PixelValue, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let index = self
            .subpalette_index
            .ok_or(CoreError::InvalidState("subpalette has no registered cell"))?;
        Ok(sequence.cells[index].raster.pixel(x, y)?)
    }

    pub fn motion_check_start(
        &mut self,
        config: MotionCheckConfig,
    ) -> Result<MotionFrame, CoreError> {
        if !matches!(config.fps, 8 | 10 | 12 | 24 | 25 | 30) {
            return Err(CoreError::InvalidArgument(
                "motion-check FPS is unsupported",
            ));
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let index = sequence.active_index.unwrap_or(0);
        self.motion_check = Some(MotionCheckState {
            config,
            index,
            paused: false,
        });
        self.motion_frame()
    }

    pub fn motion_check_step(
        &mut self,
        direction: SequenceDirection,
    ) -> Result<MotionFrame, CoreError> {
        let count = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?
            .cells
            .len();
        let state = self
            .motion_check
            .as_mut()
            .ok_or(CoreError::InvalidState("motion check is not active"))?;
        state.index = match direction {
            SequenceDirection::Previous => {
                if state.index == 0 {
                    if state.config.loop_playback {
                        count - 1
                    } else {
                        0
                    }
                } else {
                    state.index - 1
                }
            }
            SequenceDirection::Next => {
                if state.index + 1 >= count {
                    if state.config.loop_playback {
                        0
                    } else {
                        count - 1
                    }
                } else {
                    state.index + 1
                }
            }
        };
        self.motion_frame()
    }

    pub fn motion_check_toggle_pause(&mut self) -> Result<MotionFrame, CoreError> {
        let state = self
            .motion_check
            .as_mut()
            .ok_or(CoreError::InvalidState("motion check is not active"))?;
        state.paused = !state.paused;
        self.motion_frame()
    }

    pub fn motion_check_stop(&mut self) {
        self.motion_check = None;
    }

    fn motion_frame(&self) -> Result<MotionFrame, CoreError> {
        let state = self
            .motion_check
            .as_ref()
            .ok_or(CoreError::InvalidState("motion check is not active"))?;
        let cell = &self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?
            .cells[state.index];
        Ok(MotionFrame {
            sequence_index: state.index,
            cell_number: cell.cell_number,
            name: cell.name.clone(),
            thumbnail: cell.thumbnail()?,
            paused: state.paused,
            fps: state.config.fps,
            include_selection: state.config.include_selection,
            include_light_table: state.config.include_light_table,
        })
    }

    fn document_from_sequence_source(
        &mut self,
        source: &SequenceCellSource,
        _revision: u64,
    ) -> Result<CellDocument, CoreError> {
        let ids = DocumentIds {
            document: self.allocate_id(),
            layer: self.allocate_id(),
            main_plane: self.allocate_id(),
            color_plane: self.allocate_id(),
            selection_plane: self.allocate_id(),
            light_table_set: self.allocate_id(),
        };
        let mut document = CellDocument::new(
            ids,
            source.document_uuid,
            PaperSpec {
                width: source.raster.width(),
                height: source.raster.height(),
                dpi_x_milli: source.dpi_x_milli,
                dpi_y_milli: source.dpi_y_milli,
            },
        )?;
        document.frames = source.frames;
        document.plane_for_role_mut(ActivePlane::Color)?.raster = source.raster.clone();
        Ok(document)
    }
}

fn validate_reference_frame(frame: RectI32) -> Result<(), CoreError> {
    if frame.width <= 0 || frame.height <= 0 {
        Err(CoreError::InvalidArgument(
            "reference frame dimensions must be positive",
        ))
    } else {
        Ok(())
    }
}

fn validate_sequence_cell(cell: &SequenceCellSource) -> Result<(), CoreError> {
    validate_node_name(&cell.name)?;
    if cell.document_uuid == 0
        || cell.dpi_x_milli == 0
        || cell.dpi_y_milli == 0
        || parse_cell_number(&cell.name) != Some(cell.cell_number)
    {
        return Err(CoreError::InvalidArgument(
            "sequence cell identity or DPI is invalid",
        ));
    }
    validate_frame_metadata(cell.raster.width(), cell.raster.height(), cell.frames)
}

fn flatten_document(document: &CellDocument, revision: u64) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(document.width, document.height)?;
    let mut raster = TileRaster::new(document.width, document.height, PixelFormat::StraightRgba8)?;
    for y in 0..document.height {
        for x in 0..document.width {
            let mut composite = [0_u8; 4];
            for layer in document.layers.iter().rev().filter(|layer| layer.visible) {
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
                    let value = plane.raster.pixel(x, y)?;
                    let mut rgba = match plane.kind {
                        PlaneType::MainLine => {
                            let coverage = match value {
                                PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value,
                                PixelValue::Grayscale16(value) => {
                                    ((u32::from(value) + 128) / 257) as u8
                                }
                                _ => {
                                    return Err(CoreError::InvalidState(
                                        "main-line source is invalid",
                                    ));
                                }
                            };
                            let mut line = rgba8_for_display(document.main_line_color)
                                .ok_or(CoreError::InvalidState("main-line color is not RGBA"))?;
                            line[3] =
                                ((u32::from(line[3]) * u32::from(coverage) + 127) / 255) as u8;
                            line
                        }
                        PlaneType::Color | PlaneType::Raster => rgba8_for_display(value)
                            .ok_or(CoreError::InvalidState("flatten source is not RGBA"))?,
                        PlaneType::Selection
                        | PlaneType::VectorMainLine
                        | PlaneType::ColorTrace
                        | PlaneType::VectorFill => continue,
                    };
                    rgba[3] = ((u32::from(rgba[3]) * plane.opacity_milli + 500) / 1_000) as u8;
                    layer_pixel = blend_rgba_over(layer_pixel, rgba);
                }
                layer_pixel[3] =
                    ((u32::from(layer_pixel[3]) * layer.opacity_milli + 500) / 1_000) as u8;
                composite = blend_rgba_over(composite, layer_pixel);
            }
            if composite != [0; 4] {
                raster.set_pixel(x, y, PixelValue::Rgba(composite), revision)?;
            }
        }
    }
    Ok(raster)
}

fn validate_frames(document: &CellDocument, frames: FrameMetadata) -> Result<(), CoreError> {
    validate_frame_metadata(document.width, document.height, frames)
}

fn validate_frame_metadata(
    width: u32,
    height: u32,
    frames: FrameMetadata,
) -> Result<(), CoreError> {
    for frame in [
        frames.hundred_frame,
        frames.reference_frame,
        frames.drawing_frame,
        frames.safe_frame,
    ] {
        validate_reference_frame(frame)?;
    }
    if frames
        .margins
        .left
        .checked_add(frames.margins.right)
        .is_none_or(|value| value > width)
        || frames
            .margins
            .top
            .checked_add(frames.margins.bottom)
            .is_none_or(|value| value > height)
    {
        return Err(CoreError::InvalidArgument(
            "paper margins exceed document dimensions",
        ));
    }
    Ok(())
}

fn common_to_tile_raster(raster: &CommonRaster, revision: u64) -> Result<TileRaster, CoreError> {
    raster.validate()?;
    let mut result = TileRaster::new(
        raster.info.width,
        raster.info.height,
        raster.info.pixel_format,
    )?;
    let bytes_per_pixel = raster.info.pixel_format.bytes_per_pixel();
    for y in 0..raster.info.height {
        for x in 0..raster.info.width {
            let offset = (y as usize * raster.info.width as usize + x as usize) * bytes_per_pixel;
            let value = match raster.info.pixel_format {
                PixelFormat::StraightRgba8 => PixelValue::Rgba(
                    raster.pixels[offset..offset + 4]
                        .try_into()
                        .map_err(|_| CoreError::InvalidState("RGBA8 pixel is truncated"))?,
                ),
                PixelFormat::StraightRgba16 => {
                    let mut channels = [0_u16; 4];
                    for (index, channel) in channels.iter_mut().enumerate() {
                        let start = offset + index * 2;
                        *channel =
                            u16::from_le_bytes([raster.pixels[start], raster.pixels[start + 1]]);
                    }
                    PixelValue::Rgba16(channels)
                }
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "common raster must be straight RGBA",
                    ));
                }
            };
            if !value.is_zero() {
                result.set_pixel(x, y, value, revision)?;
            }
        }
    }
    Ok(result)
}

fn tile_to_common(
    raster: &TileRaster,
    dpi_x_milli: Option<u32>,
    dpi_y_milli: Option<u32>,
) -> Result<CommonRaster, CoreError> {
    let mut pixels = Vec::with_capacity(
        raster.width() as usize * raster.height() as usize * raster.format().bytes_per_pixel(),
    );
    for y in 0..raster.height() {
        for x in 0..raster.width() {
            match raster.pixel(x, y)? {
                PixelValue::Rgba(value) => pixels.extend_from_slice(&value),
                PixelValue::Rgba16(value) => {
                    for channel in value {
                        pixels.extend_from_slice(&channel.to_le_bytes());
                    }
                }
                _ => {
                    return Err(CoreError::InvalidState(
                        "sequence raster is not straight RGBA",
                    ));
                }
            }
        }
    }
    Ok(CommonRaster::new(
        raster.width(),
        raster.height(),
        raster.format(),
        dpi_x_milli,
        dpi_y_milli,
        pixels,
    )?)
}

fn thumbnail_for_raster(raster: &TileRaster) -> Result<Thumbnail, CoreError> {
    let scale = (f64::from(raster.width()) / f64::from(THUMBNAIL_MAX_DIMENSION))
        .max(f64::from(raster.height()) / f64::from(THUMBNAIL_MAX_DIMENSION))
        .max(1.0);
    let width = (f64::from(raster.width()) / scale).round().max(1.0) as u32;
    let height = (f64::from(raster.height()) / scale).round().max(1.0) as u32;
    let mut rgba8 = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let source_x = ((f64::from(x) + 0.5) * scale)
                .floor()
                .min(f64::from(raster.width() - 1)) as u32;
            let source_y = ((f64::from(y) + 0.5) * scale)
                .floor()
                .min(f64::from(raster.height() - 1)) as u32;
            let pixel = rgba8_for_display(raster.pixel(source_x, source_y)?)
                .ok_or(CoreError::InvalidState("thumbnail source is not RGBA"))?;
            rgba8.extend_from_slice(&pixel);
        }
    }
    let checksum = inkpod_image::fnv_bytes(inkpod_image::FNV_OFFSET, &rgba8);
    Ok(Thumbnail {
        width,
        height,
        rgba8,
        checksum,
    })
}

fn parse_cell_number(name: &str) -> Option<u32> {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let bytes = stem.as_bytes();
    let end = bytes.iter().rposition(u8::is_ascii_digit)? + 1;
    let start = bytes[..end]
        .iter()
        .rposition(|byte| !byte.is_ascii_digit())
        .map_or(0, |index| index + 1);
    stem[start..end].parse().ok()
}

fn natural_cmp(left: &str, right: &str) -> Ordering {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left_bytes.len() && right_index < right_bytes.len() {
        if left_bytes[left_index].is_ascii_digit() && right_bytes[right_index].is_ascii_digit() {
            let left_end = digit_run_end(left_bytes, left_index);
            let right_end = digit_run_end(right_bytes, right_index);
            let left_digits = &left[left_index..left_end];
            let right_digits = &right[right_index..right_end];
            let left_trimmed = left_digits.trim_start_matches('0');
            let right_trimmed = right_digits.trim_start_matches('0');
            let left_value = if left_trimmed.is_empty() {
                "0"
            } else {
                left_trimmed
            };
            let right_value = if right_trimmed.is_empty() {
                "0"
            } else {
                right_trimmed
            };
            let order = left_value
                .len()
                .cmp(&right_value.len())
                .then_with(|| left_value.cmp(right_value))
                .then_with(|| left_digits.len().cmp(&right_digits.len()));
            if order != Ordering::Equal {
                return order;
            }
            left_index = left_end;
            right_index = right_end;
        } else {
            let order = left_bytes[left_index]
                .to_ascii_lowercase()
                .cmp(&right_bytes[right_index].to_ascii_lowercase());
            if order != Ordering::Equal {
                return order;
            }
            left_index += 1;
            right_index += 1;
        }
    }
    left_bytes.len().cmp(&right_bytes.len())
}

fn digit_run_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    end
}
