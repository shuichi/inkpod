use crate::{FileColorChart, FileLightTableMetadata};
use inkpod_image::{FNV_OFFSET, PixelFormat, PixelValue, TileCoord, fnv_bytes};
use std::fmt;
#[cfg(test)]
use std::sync::atomic::AtomicU64;
pub(super) const MAGIC: [u8; 8] = *b"INKPOD\0\0";
/// Current development format. Increment for every serialized schema change
/// until the user declares a format freeze; older versions are not migrated.
pub const DOCUMENT_ARCHIVE_VERSION: u32 = 7;
pub(super) const DOCUMENT_METADATA_MAGIC: [u8; 4] = *b"DOCM";
pub(super) const DOCUMENT_METADATA_VERSION: u32 = 8;
pub(super) const HEADER_BYTES: usize = 32;
pub(super) const FIXED_MANIFEST_BYTES: usize = 200;
pub(super) const COLOR_METADATA_FIXED_BYTES: usize = 24;
pub(super) const COLOR_VALUE_BYTES: usize = 16;
pub(super) const PLANE_DESCRIPTOR_BYTES: usize = 32;
pub(super) const BLOB_DESCRIPTOR_BYTES: usize = 48;
pub(super) const CONTAINER_FLAG_COLOR_METADATA: u32 = 1 << 0;
pub(super) const CONTAINER_FLAG_DOCUMENT_METADATA: u32 = 1 << 1;
pub(super) const CONTAINER_FLAG_LIGHT_TABLE_METADATA: u32 = 1 << 2;
pub(super) const MAX_FILE_BYTES: u64 = 1 << 30;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 16 << 20;
pub(crate) const MAX_PLANES: usize = 4_096;
pub(super) const MAX_BLOBS: usize = 262_144;
pub(super) const MAX_LAYERS: usize = 4_096;
pub(super) const MAX_GUIDES: usize = 4_096;
pub(super) const MAX_SAVED_SELECTION_MASKS: usize = 4_096;
pub(crate) const MAX_NODE_NAME_BYTES: usize = 1_024;
#[cfg(test)]
pub(crate) static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaneKind {
    MainLine,
    Color,
    Raster,
    CurrentSelection,
    LightTable,
    FillProtection,
    SavedSelection,
}

impl PlaneKind {
    pub(super) const fn code(self) -> u32 {
        match self {
            Self::MainLine => 1,
            Self::Color => 2,
            Self::Raster => 3,
            Self::CurrentSelection => 4,
            Self::LightTable => 5,
            Self::FillProtection => 6,
            Self::SavedSelection => 7,
        }
    }

    pub(super) fn from_code(value: u32) -> Result<Self, FormatError> {
        match value {
            1 => Ok(Self::MainLine),
            2 => Ok(Self::Color),
            3 => Ok(Self::Raster),
            4 => Ok(Self::CurrentSelection),
            5 => Ok(Self::LightTable),
            6 => Ok(Self::FillProtection),
            7 => Ok(Self::SavedSelection),
            _ => Err(FormatError::Unsupported("unknown required plane kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilePlaneProperties {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub editable: bool,
    pub opacity_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileLayer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub editable: bool,
    pub opacity_milli: u32,
    pub planes: Vec<FilePlaneProperties>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileGuide {
    pub id: u64,
    pub axis: GuideAxis,
    pub position: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileGrid {
    pub origin_x: i32,
    pub origin_y: i32,
    pub spacing_x: u32,
    pub spacing_y: u32,
    pub subdivisions: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileDocumentMetadata {
    pub active_layer_id: u64,
    pub active_plane_id: u64,
    pub selection_plane_id: u64,
    pub fill_protection_plane_id: u64,
    pub layers: Vec<FileLayer>,
    pub guides: Vec<FileGuide>,
    pub grid: FileGrid,
    /// Independent named Color chart stored with the document.
    pub color_chart: FileColorChart,
    /// Whether document-changing Color chart commands are locked.
    pub color_chart_locked: bool,
    /// Optional independent angled shooting-frame instruction overlay.
    pub shooting_frame: Option<FileShootingFrame>,
    /// Named document-owned selection masks keyed by stable ID.
    pub saved_selections: Vec<FileSavedSelection>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileSavedSelection {
    pub id: u64,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileShootingFrameAnchor {
    TopLeft,
    TopRight,
    Center,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileShootingFrame {
    pub id: u64,
    pub center_x_milli: i64,
    pub center_y_milli: i64,
    pub width_milli: u64,
    pub height_milli: u64,
    pub rotation_turns: u32,
    pub anchor: FileShootingFrameAnchor,
    pub visible: bool,
    pub include_in_instruction_export: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RectI32 {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Margins {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameMetadata {
    pub hundred_frame: RectI32,
    pub reference_frame: RectI32,
    pub drawing_frame: RectI32,
    pub safe_frame: RectI32,
    pub shooting_frame: RectI32,
    pub maximum_close_frame: RectI32,
    pub margins: Margins,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileTile {
    pub coord: TileCoord,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilePlane {
    pub id: u64,
    pub kind: PlaneKind,
    pub pixel_format: PixelFormat,
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<FileTile>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentArchive {
    pub document_uuid: [u8; 16],
    pub document_id: u64,
    pub cell_id: u64,
    pub layer_id: u64,
    pub main_plane_id: u64,
    pub color_plane_id: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub frames: FrameMetadata,
    pub main_line_color: PixelValue,
    pub palette: Vec<PixelValue>,
    pub planes: Vec<FilePlane>,
    /// Decoded document metadata slot. The exact-current v32 contract requires
    /// `Some`; `None` exists only so malformed/missing DOCM input can be rejected
    /// at the validation boundary without synthesizing a compatibility tree.
    pub document_metadata: Option<FileDocumentMetadata>,
    /// Additive light-table/workflow metadata. Source rasters are blob-backed
    /// planes referenced by this section and remain outside the editable tree.
    pub light_table_metadata: Option<FileLightTableMetadata>,
}

#[derive(Debug)]
pub enum FormatError {
    Io(std::io::Error),
    Cancelled,
    Invalid(&'static str),
    Unsupported(&'static str),
    ChecksumMismatch,
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Cancelled => formatter.write_str("save was cancelled before commit"),
            Self::Invalid(message) => write!(formatter, "invalid .inkpod file: {message}"),
            Self::Unsupported(message) => {
                write!(formatter, "unsupported .inkpod feature: {message}")
            }
            Self::ChecksumMismatch => formatter.write_str(".inkpod blob checksum mismatch"),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<std::io::Error> for FormatError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Copy)]
pub(super) struct BlobDescriptor {
    pub(super) plane_index: u32,
    pub(super) tile_x: u32,
    pub(super) tile_y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) pixel_format: PixelFormat,
    pub(super) offset: u64,
    pub(super) length: u64,
    pub(super) checksum: u64,
}

#[must_use]
pub fn checksum(bytes: &[u8]) -> u64 {
    fnv_bytes(FNV_OFFSET, bytes)
}
