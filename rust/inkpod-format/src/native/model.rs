use crate::{FileAdjustmentMetadata, FileLightTableMetadata, FileVectorMetadata};
use inkpod_image::{FNV_OFFSET, PixelFormat, PixelValue, TileCoord, fnv_bytes};
use std::fmt;
#[cfg(test)]
use std::sync::atomic::AtomicU64;
pub(super) const MAGIC: [u8; 8] = *b"INKPOD\0\0";
/// Current development format. Increment for every serialized schema change
/// until the user declares a format freeze; older versions are not migrated.
pub const DOCUMENT_ARCHIVE_VERSION: u32 = 1;
pub(super) const DOCUMENT_METADATA_MAGIC: [u8; 4] = *b"DOCM";
pub(super) const HEADER_BYTES: usize = 32;
pub(super) const FIXED_MANIFEST_BYTES: usize = 160;
pub(super) const COLOR_METADATA_FIXED_BYTES: usize = 24;
pub(super) const COLOR_VALUE_BYTES: usize = 16;
pub(super) const PLANE_DESCRIPTOR_BYTES: usize = 32;
pub(super) const BLOB_DESCRIPTOR_BYTES: usize = 48;
pub(super) const CONTAINER_FLAG_COLOR_METADATA: u32 = 1 << 0;
pub(super) const CONTAINER_FLAG_DOCUMENT_METADATA: u32 = 1 << 1;
pub(super) const CONTAINER_FLAG_LIGHT_TABLE_METADATA: u32 = 1 << 2;
pub(super) const CONTAINER_FLAG_VECTOR_METADATA: u32 = 1 << 3;
pub(super) const CONTAINER_FLAG_ADJUSTMENT_METADATA: u32 = 1 << 4;
pub(super) const MAX_FILE_BYTES: u64 = 1 << 30;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 16 << 20;
pub(crate) const MAX_PLANES: usize = 4_096;
pub(super) const MAX_BLOBS: usize = 262_144;
pub(super) const MAX_LAYERS: usize = 4_096;
pub(super) const MAX_GUIDES: usize = 4_096;
pub(crate) const MAX_NODE_NAME_BYTES: usize = 1_024;
#[cfg(test)]
pub(crate) static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaneKind {
    MainLine,
    Color,
    Raster,
    Selection,
    LightTable,
    VectorMainLine,
    ColorTrace,
    VectorFill,
}

impl PlaneKind {
    pub(super) const fn code(self) -> u32 {
        match self {
            Self::MainLine => 1,
            Self::Color => 2,
            Self::Raster => 3,
            Self::Selection => 4,
            Self::LightTable => 5,
            Self::VectorMainLine => 6,
            Self::ColorTrace => 7,
            Self::VectorFill => 8,
        }
    }

    pub(super) fn from_code(value: u32) -> Result<Self, FormatError> {
        match value {
            1 => Ok(Self::MainLine),
            2 => Ok(Self::Color),
            3 => Ok(Self::Raster),
            4 => Ok(Self::Selection),
            5 => Ok(Self::LightTable),
            6 => Ok(Self::VectorMainLine),
            7 => Ok(Self::ColorTrace),
            8 => Ok(Self::VectorFill),
            _ => Err(FormatError::Unsupported("unknown required plane kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerKind {
    BinaryColoring,
    GrayscaleColoring,
    Raster,
    Selection,
    Frame,
    VanishingPoint,
    Adjustment,
    Text,
    Annotation,
    VectorColoring,
}

impl LayerKind {
    pub(super) const fn code(self) -> u32 {
        match self {
            Self::BinaryColoring => 1,
            Self::GrayscaleColoring => 2,
            Self::Raster => 3,
            Self::Selection => 4,
            Self::Frame => 5,
            Self::VanishingPoint => 6,
            Self::Adjustment => 7,
            Self::Text => 8,
            Self::Annotation => 9,
            Self::VectorColoring => 10,
        }
    }

    pub(super) fn from_code(value: u32) -> Result<Self, FormatError> {
        match value {
            1 => Ok(Self::BinaryColoring),
            2 => Ok(Self::GrayscaleColoring),
            3 => Ok(Self::Raster),
            4 => Ok(Self::Selection),
            5 => Ok(Self::Frame),
            6 => Ok(Self::VanishingPoint),
            7 => Ok(Self::Adjustment),
            8 => Ok(Self::Text),
            9 => Ok(Self::Annotation),
            10 => Ok(Self::VectorColoring),
            _ => Err(FormatError::Unsupported("unknown required layer kind")),
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
    pub kind: LayerKind,
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
    pub layers: Vec<FileLayer>,
    pub guides: Vec<FileGuide>,
    pub grid: FileGrid,
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
    /// Optional typed document metadata. `None` is valid only for the base two-plane
    /// document representation.
    pub document_metadata: Option<FileDocumentMetadata>,
    /// Additive light-table/workflow metadata. Source rasters are blob-backed
    /// planes referenced by this section and remain outside the editable tree.
    pub light_table_metadata: Option<FileLightTableMetadata>,
    /// Additive vector geometry/topology. Vector plane descriptors remain
    /// in the typed document layer tree while this section owns their stable path/fill IDs.
    pub vector_metadata: Option<FileVectorMetadata>,
    /// Optional non-destructive adjustment parameters. Adjustment layers remain
    /// in the document layer tree and never own a raster payload.
    pub adjustment_metadata: Option<FileAdjustmentMetadata>,
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
