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
