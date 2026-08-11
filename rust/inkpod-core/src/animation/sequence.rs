use super::raster::{common_to_tile_raster, thumbnail_for_raster};
use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Small owned straight-alpha RGBA8 preview of a sequence cell.
pub struct Thumbnail {
    /// Preview width in pixels.
    pub width: u32,
    /// Preview height in pixels.
    pub height: u32,
    /// Tightly packed top-to-bottom straight-alpha RGBA8 bytes.
    pub rgba8: Vec<u8>,
    /// Deterministic checksum of `rgba8` and geometry.
    pub checksum: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Validated immutable flattened source for one sequence cell.
pub struct SequenceCellSource {
    /// User-visible name containing a parseable trailing cell number.
    pub name: String,
    /// Parsed cell number used for natural ordering and navigation.
    pub cell_number: u32,
    /// Persistent nonzero document UUID.
    pub document_uuid: u128,
    /// Nonzero generation of the immutable raster payload owned by Core.
    pub source_generation: u64,
    /// Horizontal resolution in thousandths of a DPI.
    pub dpi_x_milli: u32,
    /// Vertical resolution in thousandths of a DPI.
    pub dpi_y_milli: u32,
    /// Paper/frame alignment metadata in source document pixels.
    pub frames: FrameMetadata,
    pub(crate) raster: TileRaster,
}

impl SequenceCellSource {
    /// Validates and converts owned tightly packed raster bytes into a sequence cell.
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

    /// Validates and copies a common raster into a sequence cell.
    ///
    /// The name must contain a cell number and UUID must be nonzero.
    pub fn from_common_raster(
        name: impl Into<String>,
        document_uuid: u128,
        raster: &CommonRaster,
    ) -> Result<Self, CoreError> {
        Self::from_common_raster_with_generation(name, document_uuid, 1, raster)
    }

    /// Validates and copies a common raster with an explicit immutable source generation.
    ///
    /// The name must contain a cell number, and both UUID and generation must be nonzero.
    pub fn from_common_raster_with_generation(
        name: impl Into<String>,
        document_uuid: u128,
        source_generation: u64,
        raster: &CommonRaster,
    ) -> Result<Self, CoreError> {
        let name = name.into();
        validate_node_name(&name)?;
        let cell_number = parse_cell_number(&name).ok_or(CoreError::InvalidArgument(
            "sequence name has no cell number",
        ))?;
        if document_uuid == 0 || source_generation == 0 {
            return Err(CoreError::InvalidArgument(
                "sequence document UUID and source generation must be nonzero",
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
            source_generation,
            dpi_x_milli: raster.info.dpi_x_milli.unwrap_or(DEFAULT_DPI_MILLI),
            dpi_y_milli: raster.info.dpi_y_milli.unwrap_or(DEFAULT_DPI_MILLI),
            frames: FrameMetadata {
                hundred_frame: full,
                reference_frame,
                drawing_frame: full,
                safe_frame: full,
                shooting_frame: full,
                maximum_close_frame: full,
                margins: Margins::default(),
            },
            raster: common_to_tile_raster(raster, 1)?,
        })
    }

    /// Generates a bounded aspect-preserving thumbnail without mutating the source.
    pub fn thumbnail(&self) -> Result<Thumbnail, CoreError> {
        thumbnail_for_raster(&self.raster)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Public sequence-cell metadata with an owned thumbnail.
pub struct SequenceCellInfo {
    /// User-visible source name.
    pub name: String,
    /// Parsed cell number.
    pub cell_number: u32,
    /// Persistent document UUID.
    pub document_uuid: u128,
    /// Generation of the immutable source raster.
    pub source_generation: u64,
    /// Source width in pixels.
    pub width: u32,
    /// Source height in pixels.
    pub height: u32,
    /// Bounded preview image.
    pub thumbnail: Thumbnail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SequenceState {
    pub(crate) cells: Vec<SequenceCellSource>,
    pub(super) active_index: Option<usize>,
    pub(super) revision: u64,
}

impl SequenceState {
    pub(crate) fn logical_raster_usage(&self) -> (u64, u64) {
        self.cells
            .iter()
            .fold((0_u64, 0_u64), |(tiles, bytes), cell| {
                (
                    tiles.saturating_add(cell.raster.allocated_tile_count() as u64),
                    bytes.saturating_add(cell.raster.allocated_tile_bytes()),
                )
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Relative direction used for sequence navigation.
pub enum SequenceDirection {
    /// Selects the preceding cell in natural order.
    Previous,
    /// Selects the following cell in natural order.
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
/// User-selected dirty-cell policy captured in an immutable sequence switch request.
pub enum SequenceSwitchPolicy {
    /// The frontend must obtain confirmation and complete a normal save before switching.
    Prompt = 1,
    /// The frontend must durably write recovery data before committing the switch.
    AutosaveBeforeSwitch = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Immutable source, target, and revision token for an asynchronous sequence switch.
///
/// UUID and source-generation pairs belong to the configured sequence for the
/// lifetime of that sequence. A commit succeeds only while the active source,
/// target entry, and document revision still match this request. Creating this
/// value and rejected commits do not change document, history, dirty, savepoint,
/// path, or sequence state.
pub struct SequenceSwitchRequest {
    /// Policy selected when the command was issued.
    pub policy: SequenceSwitchPolicy,
    /// Persistent UUID of the source document being protected.
    pub source_document_uuid: u128,
    /// Immutable source-raster generation of the source sequence entry.
    pub source_generation: u64,
    /// Document revision that must still be active at commit time.
    pub source_document_revision: u64,
    /// Independent EditorState revision that must still be active at commit time.
    pub source_editor_revision: u64,
    /// Persistent UUID of the requested target document.
    pub target_document_uuid: u128,
    /// Immutable source-raster generation of the target sequence entry.
    pub target_source_generation: u64,
    /// Zero-based natural-order target index.
    pub target_index: u32,
}

impl SequenceSwitchRequest {
    /// Reports whether this request would change the active sequence entry.
    #[must_use]
    pub const fn requires_switch(self) -> bool {
        self.source_document_uuid != self.target_document_uuid
            || self.source_generation != self.target_source_generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Playback and composition settings for motion check.
pub struct MotionCheckConfig {
    /// Playback rate in frames per second.
    pub fps: u32,
    /// Whether playback wraps at sequence ends.
    pub loop_playback: bool,
    /// Whether selection visualization is included.
    pub include_selection: bool,
    /// Whether light-table compositing is included.
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
/// One immutable frame returned by motion-check playback.
pub struct MotionFrame {
    /// Zero-based index in natural sequence order.
    pub sequence_index: usize,
    /// Parsed source cell number.
    pub cell_number: u32,
    /// User-visible source name.
    pub name: String,
    /// Owned bounded preview for this frame.
    pub thumbnail: Thumbnail,
    /// Current paused state.
    pub paused: bool,
    /// Configured frames per second.
    pub fps: u32,
    /// Whether selection visualization is included.
    pub include_selection: bool,
    /// Whether light-table compositing is included.
    pub include_light_table: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MotionCheckState {
    pub(super) config: MotionCheckConfig,
    pub(super) index: usize,
    pub(super) paused: bool,
}
