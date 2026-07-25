use super::raster::{common_to_tile_raster, thumbnail_for_raster};
use super::*;

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
    pub(crate) raster: TileRaster,
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
    pub(crate) cells: Vec<SequenceCellSource>,
    pub(super) active_index: Option<usize>,
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
    pub(super) config: MotionCheckConfig,
    pub(super) index: usize,
    pub(super) paused: bool,
}
