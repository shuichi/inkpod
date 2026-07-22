#![forbid(unsafe_code)]

mod common_formats;
mod native_m4;
mod native_m5;
mod native_m6;

pub use common_formats::{
    CommonRaster, CommonRasterFormat, CommonRasterInfo, MAX_COMMON_RASTER_BYTES,
    decode_common_raster, encode_common_raster,
};
pub use native_m4::{FileLightTableItem, FileLightTableSet, FileM4Metadata, LightTableDisplayMode};
use native_m4::{decode_m4_metadata, encode_m4_metadata, validate_m4_metadata};
pub use native_m5::{
    FileM5Metadata, FileVectorFill, FileVectorPath, FileVectorPoint, FileVectorSegment,
    MAX_VECTOR_BOUNDARIES, MAX_VECTOR_FILLS, MAX_VECTOR_PATHS, MAX_VECTOR_SEGMENTS,
};
use native_m5::{decode_m5_metadata, encode_m5_metadata, validate_m5_metadata};
pub use native_m6::{FileAdjustmentLayer, FileM6Metadata, MAX_ADJUSTMENT_LAYERS};
use native_m6::{decode_m6_metadata, encode_m6_metadata, validate_m6_metadata};

use inkpod_image::{FNV_OFFSET, MAX_PALETTE_COLORS, PixelFormat, PixelValue, TileCoord, fnv_bytes};
use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAGIC: [u8; 8] = *b"INKPOD\0\0";
pub const FORMAT_VERSION: u32 = 1;
const HEADER_BYTES: usize = 32;
const FIXED_MANIFEST_BYTES: usize = 160;
const COLOR_METADATA_FIXED_BYTES: usize = 24;
const COLOR_VALUE_BYTES: usize = 16;
const PLANE_DESCRIPTOR_BYTES: usize = 32;
const BLOB_DESCRIPTOR_BYTES: usize = 48;
const CONTAINER_FLAG_M2_COLOR_METADATA: u32 = 1 << 0;
const CONTAINER_FLAG_M3_DOCUMENT_EDITING: u32 = 1 << 1;
const CONTAINER_FLAG_M4_PRODUCTION_WORKFLOW: u32 = 1 << 2;
const CONTAINER_FLAG_M5_VECTOR: u32 = 1 << 3;
const CONTAINER_FLAG_M6_IMAGE_EDITING: u32 = 1 << 4;
const MAX_FILE_BYTES: u64 = 1 << 30;
const MAX_MANIFEST_BYTES: u64 = 16 << 20;
const MAX_PLANES: usize = 4_096;
const MAX_BLOBS: usize = 262_144;
const MAX_LAYERS: usize = 4_096;
const MAX_GUIDES: usize = 4_096;
const MAX_NODE_NAME_BYTES: usize = 1_024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
    const fn code(self) -> u32 {
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

    fn from_code(value: u32) -> Result<Self, FormatError> {
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
    const fn code(self) -> u32 {
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

    fn from_code(value: u32) -> Result<Self, FormatError> {
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
pub struct FileM3Metadata {
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
pub struct CellFile {
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
    /// Additive M3 editing metadata. `None` represents an M0-M2 v1 file and is
    /// upgraded to the legacy one-layer tree by the Core on open.
    pub m3: Option<FileM3Metadata>,
    /// Additive M4 light-table/workflow metadata. Source rasters are blob-backed
    /// planes referenced by this section and remain outside the editable tree.
    pub m4: Option<FileM4Metadata>,
    /// Additive M5 vector geometry/topology. Vector plane descriptors remain
    /// in the typed M3 tree while this section owns their stable path/fill IDs.
    pub m5: Option<FileM5Metadata>,
    /// Additive M6 non-destructive adjustment parameters. Adjustment layers
    /// remain in the M3 tree and never own a raster payload.
    pub m6: Option<FileM6Metadata>,
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
struct BlobDescriptor {
    plane_index: u32,
    tile_x: u32,
    tile_y: u32,
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    offset: u64,
    length: u64,
    checksum: u64,
}

#[must_use]
pub fn checksum(bytes: &[u8]) -> u64 {
    fnv_bytes(FNV_OFFSET, bytes)
}

pub fn encode(document: &CellFile) -> Result<Vec<u8>, FormatError> {
    encode_with_color_metadata(document, true)
}

fn encode_with_color_metadata(
    document: &CellFile,
    include_color_metadata: bool,
) -> Result<Vec<u8>, FormatError> {
    validate_document(document)?;
    let m3_metadata = document.m3.as_ref().map(encode_m3_metadata).transpose()?;
    let m4_metadata = document.m4.as_ref().map(encode_m4_metadata).transpose()?;
    let m5_metadata = document.m5.as_ref().map(encode_m5_metadata).transpose()?;
    let m6_metadata = document.m6.as_ref().map(encode_m6_metadata).transpose()?;
    let blob_count = document.planes.iter().try_fold(0_usize, |count, plane| {
        count
            .checked_add(plane.tiles.len())
            .ok_or(FormatError::Invalid("blob count overflows"))
    })?;
    if blob_count > MAX_BLOBS {
        return Err(FormatError::Invalid("too many blobs"));
    }
    let color_metadata_len = if include_color_metadata {
        COLOR_METADATA_FIXED_BYTES
            .checked_add(
                document
                    .palette
                    .len()
                    .checked_mul(COLOR_VALUE_BYTES)
                    .ok_or(FormatError::Invalid("palette manifest overflows"))?,
            )
            .ok_or(FormatError::Invalid("color metadata length overflows"))?
    } else {
        if !document.palette.is_empty()
            || document.main_line_color != legacy_main_line_color(document)?
        {
            return Err(FormatError::Invalid(
                "legacy v1 manifest cannot store color metadata",
            ));
        }
        0
    };
    let manifest_len = FIXED_MANIFEST_BYTES
        .checked_add(color_metadata_len)
        .and_then(|value| {
            value.checked_add(
                m3_metadata
                    .as_ref()
                    .map_or(0, |bytes| bytes.len().saturating_add(8)),
            )
        })
        .and_then(|value| {
            value.checked_add(
                m4_metadata
                    .as_ref()
                    .map_or(0, |bytes| bytes.len().saturating_add(8)),
            )
        })
        .and_then(|value| {
            value.checked_add(
                m5_metadata
                    .as_ref()
                    .map_or(0, |bytes| bytes.len().saturating_add(8)),
            )
        })
        .and_then(|value| {
            value.checked_add(
                m6_metadata
                    .as_ref()
                    .map_or(0, |bytes| bytes.len().saturating_add(8)),
            )
        })
        .and_then(|value| {
            value.checked_add(document.planes.len().checked_mul(PLANE_DESCRIPTOR_BYTES)?)
        })
        .and_then(|value| value.checked_add(blob_count.checked_mul(BLOB_DESCRIPTOR_BYTES)?))
        .ok_or(FormatError::Invalid("manifest length overflows"))?;

    let mut descriptors = Vec::with_capacity(blob_count);
    let mut blobs = Vec::new();
    for (plane_index, plane) in document.planes.iter().enumerate() {
        for tile in &plane.tiles {
            let offset = u64::try_from(blobs.len())
                .map_err(|_| FormatError::Invalid("blob offset is not representable"))?;
            let length = u64::try_from(tile.bytes.len())
                .map_err(|_| FormatError::Invalid("blob length is not representable"))?;
            if offset
                .checked_add(length)
                .is_none_or(|end| end > MAX_FILE_BYTES)
            {
                return Err(FormatError::Invalid("blob area exceeds the bounded size"));
            }
            descriptors.push(BlobDescriptor {
                plane_index: plane_index as u32,
                tile_x: tile.coord.x,
                tile_y: tile.coord.y,
                width: tile.width,
                height: tile.height,
                pixel_format: plane.pixel_format,
                offset,
                length,
                checksum: checksum(&tile.bytes),
            });
            blobs.extend_from_slice(&tile.bytes);
        }
    }

    let total_len = HEADER_BYTES
        .checked_add(manifest_len)
        .and_then(|value| value.checked_add(blobs.len()))
        .ok_or(FormatError::Invalid("file length overflows"))?;
    if total_len as u64 > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("file exceeds the bounded size"));
    }

    let mut output = Vec::with_capacity(total_len);
    output.extend_from_slice(&MAGIC);
    push_u32(&mut output, FORMAT_VERSION);
    push_u32(
        &mut output,
        (if include_color_metadata {
            CONTAINER_FLAG_M2_COLOR_METADATA
        } else {
            0
        }) | if m3_metadata.is_some() {
            CONTAINER_FLAG_M3_DOCUMENT_EDITING
        } else {
            0
        } | if m4_metadata.is_some() {
            CONTAINER_FLAG_M4_PRODUCTION_WORKFLOW
        } else {
            0
        } | if m5_metadata.is_some() {
            CONTAINER_FLAG_M5_VECTOR
        } else {
            0
        } | if m6_metadata.is_some() {
            CONTAINER_FLAG_M6_IMAGE_EDITING
        } else {
            0
        },
    );
    push_u64(&mut output, manifest_len as u64);
    push_u64(&mut output, blob_count as u64);

    push_u64(&mut output, document.document_id);
    push_u64(&mut output, document.layer_id);
    push_u64(&mut output, document.main_plane_id);
    push_u64(&mut output, document.color_plane_id);
    output.extend_from_slice(&document.document_uuid);
    push_u32(&mut output, document.width);
    push_u32(&mut output, document.height);
    push_u32(&mut output, document.dpi_x_milli);
    push_u32(&mut output, document.dpi_y_milli);
    push_u32(&mut output, 1); // sRGB
    push_u32(&mut output, 0);
    for frame in [
        document.frames.hundred_frame,
        document.frames.reference_frame,
        document.frames.drawing_frame,
        document.frames.safe_frame,
    ] {
        push_i32(&mut output, frame.x);
        push_i32(&mut output, frame.y);
        push_i32(&mut output, frame.width);
        push_i32(&mut output, frame.height);
    }
    push_u32(&mut output, document.frames.margins.left);
    push_u32(&mut output, document.frames.margins.top);
    push_u32(&mut output, document.frames.margins.right);
    push_u32(&mut output, document.frames.margins.bottom);
    push_u32(&mut output, document.planes.len() as u32);
    push_u32(&mut output, blob_count as u32);

    if include_color_metadata {
        push_color_value(&mut output, document.main_line_color)?;
        push_u32(&mut output, document.palette.len() as u32);
        push_u32(&mut output, 0);
        for color in &document.palette {
            push_color_value(&mut output, *color)?;
        }
    }
    if let Some(metadata) = &m3_metadata {
        push_u32(
            &mut output,
            metadata
                .len()
                .try_into()
                .map_err(|_| FormatError::Invalid("M3 metadata length is not representable"))?,
        );
        push_u32(&mut output, 0);
        output.extend_from_slice(metadata);
    }
    if let Some(metadata) = &m4_metadata {
        push_u32(
            &mut output,
            metadata
                .len()
                .try_into()
                .map_err(|_| FormatError::Invalid("M4 metadata length is not representable"))?,
        );
        push_u32(&mut output, 0);
        output.extend_from_slice(metadata);
    }
    if let Some(metadata) = &m5_metadata {
        push_u32(
            &mut output,
            metadata
                .len()
                .try_into()
                .map_err(|_| FormatError::Invalid("M5 metadata length is not representable"))?,
        );
        push_u32(&mut output, 0);
        output.extend_from_slice(metadata);
    }
    if let Some(metadata) = &m6_metadata {
        push_u32(
            &mut output,
            metadata
                .len()
                .try_into()
                .map_err(|_| FormatError::Invalid("M6 metadata length is not representable"))?,
        );
        push_u32(&mut output, 0);
        output.extend_from_slice(metadata);
    }

    let mut first_blob = 0_u32;
    for plane in &document.planes {
        push_u64(&mut output, plane.id);
        push_u32(&mut output, plane.kind.code());
        push_u32(&mut output, pixel_format_code(plane.pixel_format));
        push_u32(&mut output, first_blob);
        push_u32(&mut output, plane.tiles.len() as u32);
        push_u32(&mut output, plane.width);
        push_u32(&mut output, plane.height);
        first_blob = first_blob
            .checked_add(plane.tiles.len() as u32)
            .ok_or(FormatError::Invalid("blob index overflows"))?;
    }
    for descriptor in descriptors {
        push_u32(&mut output, descriptor.plane_index);
        push_u32(&mut output, descriptor.tile_x);
        push_u32(&mut output, descriptor.tile_y);
        push_u32(&mut output, descriptor.width);
        push_u32(&mut output, descriptor.height);
        push_u32(&mut output, pixel_format_code(descriptor.pixel_format));
        push_u64(&mut output, descriptor.offset);
        push_u64(&mut output, descriptor.length);
        push_u64(&mut output, descriptor.checksum);
    }
    debug_assert_eq!(output.len(), HEADER_BYTES + manifest_len);
    output.extend_from_slice(&blobs);
    Ok(output)
}

pub fn decode(bytes: &[u8]) -> Result<CellFile, FormatError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("file exceeds the bounded size"));
    }
    let mut reader = Reader::new(bytes);
    if reader.take(8)? != MAGIC {
        return Err(FormatError::Invalid("magic does not match"));
    }
    if reader.u32()? != FORMAT_VERSION {
        return Err(FormatError::Unsupported("format version is not supported"));
    }
    let container_flags = reader.u32()?;
    if container_flags
        & !(CONTAINER_FLAG_M2_COLOR_METADATA
            | CONTAINER_FLAG_M3_DOCUMENT_EDITING
            | CONTAINER_FLAG_M4_PRODUCTION_WORKFLOW
            | CONTAINER_FLAG_M5_VECTOR
            | CONTAINER_FLAG_M6_IMAGE_EDITING)
        != 0
    {
        return Err(FormatError::Unsupported(
            "required container flags are unknown",
        ));
    }
    let manifest_len = reader.u64()?;
    let header_blob_count = reader.u64()?;
    if manifest_len < FIXED_MANIFEST_BYTES as u64 || manifest_len > MAX_MANIFEST_BYTES {
        return Err(FormatError::Invalid("manifest length is outside bounds"));
    }
    let manifest_end = HEADER_BYTES
        .checked_add(
            usize::try_from(manifest_len)
                .map_err(|_| FormatError::Invalid("manifest length is not representable"))?,
        )
        .ok_or(FormatError::Invalid("manifest end overflows"))?;
    if manifest_end > bytes.len() {
        return Err(FormatError::Invalid("manifest is truncated"));
    }

    let document_id = reader.u64()?;
    let layer_id = reader.u64()?;
    let main_plane_id = reader.u64()?;
    let color_plane_id = reader.u64()?;
    let document_uuid: [u8; 16] = reader
        .take(16)?
        .try_into()
        .map_err(|_| FormatError::Invalid("document UUID is truncated"))?;
    let width = reader.u32()?;
    let height = reader.u32()?;
    let dpi_x_milli = reader.u32()?;
    let dpi_y_milli = reader.u32()?;
    if reader.u32()? != 1 {
        return Err(FormatError::Unsupported("required color space is unknown"));
    }
    if reader.u32()? != 0 {
        return Err(FormatError::Unsupported(
            "manifest reserved field is not zero",
        ));
    }
    let mut rects = [RectI32::default(); 4];
    for rect in &mut rects {
        *rect = RectI32 {
            x: reader.i32()?,
            y: reader.i32()?,
            width: reader.i32()?,
            height: reader.i32()?,
        };
    }
    let margins = Margins {
        left: reader.u32()?,
        top: reader.u32()?,
        right: reader.u32()?,
        bottom: reader.u32()?,
    };
    let plane_count = reader.u32()? as usize;
    let manifest_blob_count = reader.u32()? as usize;
    if plane_count == 0 || plane_count > MAX_PLANES {
        return Err(FormatError::Invalid("plane count is outside bounds"));
    }
    if manifest_blob_count > MAX_BLOBS || header_blob_count != manifest_blob_count as u64 {
        return Err(FormatError::Invalid("blob count is inconsistent"));
    }
    let (main_line_color, palette) = if container_flags & CONTAINER_FLAG_M2_COLOR_METADATA != 0 {
        let main_line_color = reader.color_value()?;
        let palette_count = reader.u32()? as usize;
        if reader.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "color metadata reserved field is not zero",
            ));
        }
        if palette_count > MAX_PALETTE_COLORS {
            return Err(FormatError::Invalid("palette count exceeds its bound"));
        }
        let mut palette = Vec::with_capacity(palette_count);
        for _ in 0..palette_count {
            palette.push(reader.color_value()?);
        }
        (Some(main_line_color), palette)
    } else {
        (None, Vec::new())
    };
    let color_metadata_len = if container_flags & CONTAINER_FLAG_M2_COLOR_METADATA != 0 {
        COLOR_METADATA_FIXED_BYTES
            .checked_add(
                palette
                    .len()
                    .checked_mul(COLOR_VALUE_BYTES)
                    .ok_or(FormatError::Invalid("palette manifest overflows"))?,
            )
            .ok_or(FormatError::Invalid("color metadata length overflows"))?
    } else {
        0
    };
    let (m3, m3_metadata_len) = if container_flags & CONTAINER_FLAG_M3_DOCUMENT_EDITING != 0 {
        let byte_count = reader.u32()? as usize;
        if reader.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "M3 metadata reserved field is not zero",
            ));
        }
        if byte_count > MAX_MANIFEST_BYTES as usize {
            return Err(FormatError::Invalid("M3 metadata exceeds its bound"));
        }
        let metadata = decode_m3_metadata(reader.take(byte_count)?)?;
        (Some(metadata), byte_count.saturating_add(8))
    } else {
        (None, 0)
    };
    let (m4, m4_metadata_len) = if container_flags & CONTAINER_FLAG_M4_PRODUCTION_WORKFLOW != 0 {
        let byte_count = reader.u32()? as usize;
        if reader.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "M4 metadata reserved field is not zero",
            ));
        }
        if byte_count > MAX_MANIFEST_BYTES as usize {
            return Err(FormatError::Invalid("M4 metadata exceeds its bound"));
        }
        let metadata = decode_m4_metadata(reader.take(byte_count)?)?;
        (Some(metadata), byte_count.saturating_add(8))
    } else {
        (None, 0)
    };
    let (m5, m5_metadata_len) = if container_flags & CONTAINER_FLAG_M5_VECTOR != 0 {
        let byte_count = reader.u32()? as usize;
        if reader.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "M5 metadata reserved field is not zero",
            ));
        }
        if byte_count > MAX_MANIFEST_BYTES as usize {
            return Err(FormatError::Invalid("M5 metadata exceeds its bound"));
        }
        let metadata = decode_m5_metadata(reader.take(byte_count)?)?;
        (Some(metadata), byte_count.saturating_add(8))
    } else {
        (None, 0)
    };
    let (m6, m6_metadata_len) = if container_flags & CONTAINER_FLAG_M6_IMAGE_EDITING != 0 {
        let byte_count = reader.u32()? as usize;
        if reader.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "M6 metadata reserved field is not zero",
            ));
        }
        if byte_count > MAX_MANIFEST_BYTES as usize {
            return Err(FormatError::Invalid("M6 metadata exceeds its bound"));
        }
        let metadata = decode_m6_metadata(reader.take(byte_count)?)?;
        (Some(metadata), byte_count.saturating_add(8))
    } else {
        (None, 0)
    };
    let expected_manifest_len = FIXED_MANIFEST_BYTES
        .checked_add(color_metadata_len)
        .and_then(|value| value.checked_add(m3_metadata_len))
        .and_then(|value| value.checked_add(m4_metadata_len))
        .and_then(|value| value.checked_add(m5_metadata_len))
        .and_then(|value| value.checked_add(m6_metadata_len))
        .and_then(|value| value.checked_add(plane_count.checked_mul(PLANE_DESCRIPTOR_BYTES)?))
        .and_then(|value| {
            value.checked_add(manifest_blob_count.checked_mul(BLOB_DESCRIPTOR_BYTES)?)
        })
        .ok_or(FormatError::Invalid("manifest length overflows"))?;
    if expected_manifest_len != manifest_len as usize {
        return Err(FormatError::Invalid(
            "manifest length does not match its counts",
        ));
    }

    struct PlaneDescriptor {
        id: u64,
        kind: PlaneKind,
        pixel_format: PixelFormat,
        first_blob: usize,
        blob_count: usize,
        width: u32,
        height: u32,
    }
    let mut plane_descriptors = Vec::with_capacity(plane_count);
    let mut ids = BTreeSet::new();
    for id in [document_id, layer_id, main_plane_id, color_plane_id] {
        if id == 0 || !ids.insert(id) {
            return Err(FormatError::Invalid(
                "stable IDs must be nonzero and unique",
            ));
        }
    }
    let mut plane_ids = BTreeSet::new();
    for _ in 0..plane_count {
        let id = reader.u64()?;
        if id == 0 || !plane_ids.insert(id) {
            return Err(FormatError::Invalid("plane ID is invalid"));
        }
        plane_descriptors.push(PlaneDescriptor {
            id,
            kind: PlaneKind::from_code(reader.u32()?)?,
            pixel_format: pixel_format_from_code(reader.u32()?)?,
            first_blob: reader.u32()? as usize,
            blob_count: reader.u32()? as usize,
            width: reader.u32()?,
            height: reader.u32()?,
        });
    }
    let mut next_blob = 0_usize;
    for descriptor in &plane_descriptors {
        if descriptor.first_blob != next_blob {
            return Err(FormatError::Invalid(
                "plane blob ranges are not contiguous and ordered",
            ));
        }
        next_blob = next_blob
            .checked_add(descriptor.blob_count)
            .ok_or(FormatError::Invalid("plane blob range overflows"))?;
    }
    if next_blob != manifest_blob_count {
        return Err(FormatError::Invalid(
            "plane blob ranges do not cover the manifest",
        ));
    }
    let mut blob_descriptors = Vec::with_capacity(manifest_blob_count);
    for _ in 0..manifest_blob_count {
        blob_descriptors.push(BlobDescriptor {
            plane_index: reader.u32()?,
            tile_x: reader.u32()?,
            tile_y: reader.u32()?,
            width: reader.u32()?,
            height: reader.u32()?,
            pixel_format: pixel_format_from_code(reader.u32()?)?,
            offset: reader.u64()?,
            length: reader.u64()?,
            checksum: reader.u64()?,
        });
    }
    if reader.position != manifest_end {
        return Err(FormatError::Invalid(
            "manifest cursor did not end at its boundary",
        ));
    }

    let blob_area = &bytes[manifest_end..];
    let mut next_offset = 0_u64;
    for blob in &blob_descriptors {
        if blob.offset != next_offset {
            return Err(FormatError::Invalid(
                "blob ranges are not contiguous and ordered",
            ));
        }
        next_offset = next_offset
            .checked_add(blob.length)
            .ok_or(FormatError::Invalid("blob range overflows"))?;
    }
    if next_offset != blob_area.len() as u64 {
        return Err(FormatError::Invalid(
            "blob ranges do not cover the file blob area",
        ));
    }
    let mut planes = Vec::with_capacity(plane_count);
    for (plane_index, descriptor) in plane_descriptors.into_iter().enumerate() {
        if descriptor.kind != PlaneKind::LightTable
            && (descriptor.width != width || descriptor.height != height)
        {
            return Err(FormatError::Invalid(
                "plane dimensions do not match the document",
            ));
        }
        let end_blob = descriptor
            .first_blob
            .checked_add(descriptor.blob_count)
            .ok_or(FormatError::Invalid("plane blob range overflows"))?;
        if end_blob > blob_descriptors.len() {
            return Err(FormatError::Invalid(
                "plane blob range is outside the manifest",
            ));
        }
        let mut tile_coords = BTreeSet::new();
        let mut tiles = Vec::with_capacity(descriptor.blob_count);
        for blob in &blob_descriptors[descriptor.first_blob..end_blob] {
            if blob.plane_index as usize != plane_index
                || blob.pixel_format != descriptor.pixel_format
            {
                return Err(FormatError::Invalid(
                    "blob references the wrong plane or format",
                ));
            }
            let coord = TileCoord {
                x: blob.tile_x,
                y: blob.tile_y,
            };
            if !tile_coords.insert(coord) {
                return Err(FormatError::Invalid("duplicate tile coordinates"));
            }
            validate_tile_shape(
                descriptor.width,
                descriptor.height,
                descriptor.pixel_format,
                coord,
                blob.width,
                blob.height,
                blob.length,
            )?;
            let start = usize::try_from(blob.offset)
                .map_err(|_| FormatError::Invalid("blob offset is not representable"))?;
            let length = usize::try_from(blob.length)
                .map_err(|_| FormatError::Invalid("blob length is not representable"))?;
            let end = start
                .checked_add(length)
                .ok_or(FormatError::Invalid("blob range overflows"))?;
            let data = blob_area
                .get(start..end)
                .ok_or(FormatError::Invalid("blob range is outside the file"))?;
            if checksum(data) != blob.checksum {
                return Err(FormatError::ChecksumMismatch);
            }
            tiles.push(FileTile {
                coord,
                width: blob.width,
                height: blob.height,
                bytes: data.to_vec(),
            });
        }
        planes.push(FilePlane {
            id: descriptor.id,
            kind: descriptor.kind,
            pixel_format: descriptor.pixel_format,
            width: descriptor.width,
            height: descriptor.height,
            tiles,
        });
    }

    let main_line_color = match main_line_color {
        Some(color) => color,
        None => legacy_main_line_color_for_planes(&planes)?,
    };
    let document = CellFile {
        document_uuid,
        document_id,
        layer_id,
        main_plane_id,
        color_plane_id,
        width,
        height,
        dpi_x_milli,
        dpi_y_milli,
        frames: FrameMetadata {
            hundred_frame: rects[0],
            reference_frame: rects[1],
            drawing_frame: rects[2],
            safe_frame: rects[3],
            margins,
        },
        main_line_color,
        palette,
        planes,
        m3,
        m4,
        m5,
        m6,
    };
    validate_document(&document)?;
    Ok(document)
}

pub fn read(path: &Path) -> Result<CellFile, FormatError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("file exceeds the bounded size"));
    }
    let input = OpenOptions::new().read(true).open(path)?;
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| FormatError::Invalid("file length is not representable"))?;
    let mut bytes = Vec::with_capacity(capacity);
    input.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("file exceeds the bounded size"));
    }
    decode(&bytes)
}

pub fn save_atomic(path: &Path, document: &CellFile) -> Result<(), FormatError> {
    save_atomic_with_cancel(path, document, || false)
}

/// Recovery uses the same bounded, atomic container write as a normal save.
/// Savepoint and normal-path semantics deliberately remain a Core concern.
pub fn save_recovery_atomic(path: &Path, document: &CellFile) -> Result<(), FormatError> {
    save_atomic(path, document)
}

pub fn recovery_is_newer(normal_path: &Path, recovery_path: &Path) -> Result<bool, FormatError> {
    let recovery = match fs::metadata(recovery_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let normal = match fs::metadata(normal_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    Ok(recovery.modified()? > normal.modified()?)
}

pub fn discard_recovery(path: &Path) -> Result<(), FormatError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn save_atomic_with_cancel(
    path: &Path,
    document: &CellFile,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(), FormatError> {
    if is_cancelled() {
        return Err(FormatError::Cancelled);
    }
    let bytes = encode(document)?;
    let (temporary_path, mut temporary) = create_temporary(path)?;
    let result = (|| {
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        temporary.sync_all()?;
        drop(temporary);
        if is_cancelled() {
            return Err(FormatError::Cancelled);
        }
        fs::rename(&temporary_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn create_temporary(path: &Path) -> Result<(PathBuf, std::fs::File), FormatError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    path.file_name()
        .ok_or(FormatError::Invalid("destination has no file name"))?;
    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_name = format!(".inkpod.tmp.{}.{}", std::process::id(), sequence);
        let temporary_path = parent.join(temporary_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(FormatError::Io(error)),
        }
    }
    Err(FormatError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not reserve a same-directory temporary file",
    )))
}

fn encode_m3_metadata(metadata: &FileM3Metadata) -> Result<Vec<u8>, FormatError> {
    validate_m3_metadata(metadata, None)?;
    let mut output = Vec::new();
    output.extend_from_slice(b"M3ED");
    push_u32(&mut output, 1);
    push_u64(&mut output, metadata.active_layer_id);
    push_u64(&mut output, metadata.active_plane_id);
    push_u64(&mut output, metadata.selection_plane_id);
    push_u32(&mut output, metadata.layers.len() as u32);
    push_u32(&mut output, metadata.guides.len() as u32);
    push_i32(&mut output, metadata.grid.origin_x);
    push_i32(&mut output, metadata.grid.origin_y);
    push_u32(&mut output, metadata.grid.spacing_x);
    push_u32(&mut output, metadata.grid.spacing_y);
    push_u32(&mut output, metadata.grid.subdivisions);
    push_u32(&mut output, 0);
    for layer in &metadata.layers {
        push_u64(&mut output, layer.id);
        push_u32(&mut output, layer.kind.code());
        push_u32(
            &mut output,
            u32::from(layer.visible) | (u32::from(layer.editable) << 1),
        );
        push_u32(&mut output, layer.opacity_milli);
        push_u32(&mut output, layer.name.len() as u32);
        push_u32(&mut output, layer.planes.len() as u32);
        push_u32(&mut output, 0);
        output.extend_from_slice(layer.name.as_bytes());
        for plane in &layer.planes {
            push_u64(&mut output, plane.id);
            push_u32(
                &mut output,
                u32::from(plane.visible) | (u32::from(plane.editable) << 1),
            );
            push_u32(&mut output, plane.opacity_milli);
            push_u32(&mut output, plane.name.len() as u32);
            push_u32(&mut output, 0);
            output.extend_from_slice(plane.name.as_bytes());
        }
    }
    for guide in &metadata.guides {
        push_u64(&mut output, guide.id);
        push_u32(
            &mut output,
            match guide.axis {
                GuideAxis::Horizontal => 1,
                GuideAxis::Vertical => 2,
            },
        );
        push_i32(&mut output, guide.position);
    }
    if output.len() > MAX_MANIFEST_BYTES as usize {
        return Err(FormatError::Invalid("M3 metadata exceeds its bound"));
    }
    Ok(output)
}

fn decode_m3_metadata(bytes: &[u8]) -> Result<FileM3Metadata, FormatError> {
    let mut reader = Reader::new(bytes);
    if reader.take(4)? != b"M3ED" || reader.u32()? != 1 {
        return Err(FormatError::Unsupported(
            "M3 metadata version is not supported",
        ));
    }
    let active_layer_id = reader.u64()?;
    let active_plane_id = reader.u64()?;
    let selection_plane_id = reader.u64()?;
    let layer_count = reader.u32()? as usize;
    let guide_count = reader.u32()? as usize;
    if layer_count == 0 || layer_count > MAX_LAYERS || guide_count > MAX_GUIDES {
        return Err(FormatError::Invalid(
            "M3 layer or guide count is outside bounds",
        ));
    }
    let grid = FileGrid {
        origin_x: reader.i32()?,
        origin_y: reader.i32()?,
        spacing_x: reader.u32()?,
        spacing_y: reader.u32()?,
        subdivisions: reader.u32()?,
    };
    if reader.u32()? != 0 {
        return Err(FormatError::Unsupported(
            "M3 grid reserved field is not zero",
        ));
    }
    let mut layers = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        let id = reader.u64()?;
        let kind = LayerKind::from_code(reader.u32()?)?;
        let flags = reader.u32()?;
        if flags & !3 != 0 {
            return Err(FormatError::Unsupported("unknown M3 layer flags"));
        }
        let opacity_milli = reader.u32()?;
        let name_len = reader.u32()? as usize;
        let plane_count = reader.u32()? as usize;
        if reader.u32()? != 0 || plane_count > MAX_PLANES {
            return Err(FormatError::Invalid("M3 layer descriptor is invalid"));
        }
        let name = read_name(&mut reader, name_len)?;
        let mut planes = Vec::with_capacity(plane_count);
        for _ in 0..plane_count {
            let plane_id = reader.u64()?;
            let plane_flags = reader.u32()?;
            if plane_flags & !3 != 0 {
                return Err(FormatError::Unsupported("unknown M3 plane flags"));
            }
            let plane_opacity = reader.u32()?;
            let plane_name_len = reader.u32()? as usize;
            if reader.u32()? != 0 {
                return Err(FormatError::Unsupported(
                    "M3 plane reserved field is not zero",
                ));
            }
            planes.push(FilePlaneProperties {
                id: plane_id,
                name: read_name(&mut reader, plane_name_len)?,
                visible: flags_bit(plane_flags, 0),
                editable: flags_bit(plane_flags, 1),
                opacity_milli: plane_opacity,
            });
        }
        layers.push(FileLayer {
            id,
            kind,
            name,
            visible: flags_bit(flags, 0),
            editable: flags_bit(flags, 1),
            opacity_milli,
            planes,
        });
    }
    let mut guides = Vec::with_capacity(guide_count);
    for _ in 0..guide_count {
        guides.push(FileGuide {
            id: reader.u64()?,
            axis: match reader.u32()? {
                1 => GuideAxis::Horizontal,
                2 => GuideAxis::Vertical,
                _ => return Err(FormatError::Unsupported("unknown guide axis")),
            },
            position: reader.i32()?,
        });
    }
    if reader.position != bytes.len() {
        return Err(FormatError::Invalid("M3 metadata has trailing bytes"));
    }
    let metadata = FileM3Metadata {
        active_layer_id,
        active_plane_id,
        selection_plane_id,
        layers,
        guides,
        grid,
    };
    validate_m3_metadata(&metadata, None)?;
    Ok(metadata)
}

const fn flags_bit(flags: u32, bit: u32) -> bool {
    flags & (1 << bit) != 0
}

fn read_name(reader: &mut Reader<'_>, length: usize) -> Result<String, FormatError> {
    if length == 0 || length > MAX_NODE_NAME_BYTES {
        return Err(FormatError::Invalid("node name length is outside bounds"));
    }
    let text = std::str::from_utf8(reader.take(length)?)
        .map_err(|_| FormatError::Invalid("node name is not valid UTF-8"))?;
    if text.chars().any(char::is_control) {
        return Err(FormatError::Invalid(
            "node name contains control characters",
        ));
    }
    Ok(text.to_owned())
}

fn validate_m3_metadata(
    metadata: &FileM3Metadata,
    file_planes: Option<&[FilePlane]>,
) -> Result<(), FormatError> {
    if metadata.layers.is_empty()
        || metadata.layers.len() > MAX_LAYERS
        || metadata.guides.len() > MAX_GUIDES
        || metadata.grid.spacing_x == 0
        || metadata.grid.spacing_y == 0
        || metadata.grid.spacing_x > 1_048_576
        || metadata.grid.spacing_y > 1_048_576
        || metadata.grid.subdivisions == 0
        || metadata.grid.subdivisions > 1_024
    {
        return Err(FormatError::Invalid(
            "M3 metadata values are outside bounds",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut active_layer_found = false;
    let mut active_plane_found = false;
    let mut referenced_planes = BTreeSet::new();
    for layer in &metadata.layers {
        validate_name(&layer.name)?;
        if layer.id == 0 || !ids.insert(layer.id) || layer.opacity_milli > 1_000 {
            return Err(FormatError::Invalid("M3 layer properties are invalid"));
        }
        active_layer_found |= layer.id == metadata.active_layer_id;
        for plane in &layer.planes {
            validate_name(&plane.name)?;
            if plane.id == 0
                || !ids.insert(plane.id)
                || !referenced_planes.insert(plane.id)
                || plane.opacity_milli > 1_000
            {
                return Err(FormatError::Invalid("M3 plane properties are invalid"));
            }
            active_plane_found |= plane.id == metadata.active_plane_id;
        }
    }
    for guide in &metadata.guides {
        if guide.id == 0 || !ids.insert(guide.id) {
            return Err(FormatError::Invalid("guide ID is invalid"));
        }
    }
    if metadata.selection_plane_id == 0
        || !ids.insert(metadata.selection_plane_id)
        || !active_layer_found
        || !active_plane_found
    {
        return Err(FormatError::Invalid("M3 active or selection ID is invalid"));
    }
    if let Some(planes) = file_planes {
        let plane_ids: BTreeSet<_> = planes
            .iter()
            .filter(|plane| plane.kind != PlaneKind::LightTable)
            .map(|plane| plane.id)
            .collect();
        referenced_planes.insert(metadata.selection_plane_id);
        if referenced_planes != plane_ids {
            return Err(FormatError::Invalid("M3 tree and plane payload IDs differ"));
        }
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), FormatError> {
    if name.is_empty() || name.len() > MAX_NODE_NAME_BYTES || name.chars().any(char::is_control) {
        Err(FormatError::Invalid("node name is invalid"))
    } else {
        Ok(())
    }
}

fn validate_document(document: &CellFile) -> Result<(), FormatError> {
    if document.width == 0
        || document.height == 0
        || document.width > inkpod_image::MAX_RASTER_DIMENSION
        || document.height > inkpod_image::MAX_RASTER_DIMENSION
        || document.dpi_x_milli == 0
        || document.dpi_y_milli == 0
    {
        return Err(FormatError::Invalid(
            "document dimensions or DPI are invalid",
        ));
    }
    if document.document_uuid.iter().all(|byte| *byte == 0) {
        return Err(FormatError::Invalid("document UUID must be nonzero"));
    }
    let ids = [
        document.document_id,
        document.layer_id,
        document.main_plane_id,
        document.color_plane_id,
    ];
    if ids.contains(&0) || ids.into_iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(FormatError::Invalid(
            "stable IDs must be nonzero and unique",
        ));
    }
    for frame in [
        document.frames.hundred_frame,
        document.frames.reference_frame,
        document.frames.drawing_frame,
        document.frames.safe_frame,
    ] {
        if frame.width <= 0 || frame.height <= 0 {
            return Err(FormatError::Invalid("frame dimensions must be positive"));
        }
    }
    if document
        .frames
        .margins
        .left
        .checked_add(document.frames.margins.right)
        .is_none_or(|horizontal| horizontal > document.width)
        || document
            .frames
            .margins
            .top
            .checked_add(document.frames.margins.bottom)
            .is_none_or(|vertical| vertical > document.height)
    {
        return Err(FormatError::Invalid("margins exceed document dimensions"));
    }
    if document.planes.len() < 2 || document.planes.len() > MAX_PLANES {
        return Err(FormatError::Invalid(
            "coloring cell plane count is outside bounds",
        ));
    }
    let main = document
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneKind::MainLine)
        .ok_or(FormatError::Invalid("main line plane is missing"))?;
    let color = document
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneKind::Color)
        .ok_or(FormatError::Invalid("color plane is missing"))?;
    if document.main_line_color.rgba16().is_none() {
        return Err(FormatError::Invalid("main-line base color must be RGBA"));
    }
    if document.palette.len() > MAX_PALETTE_COLORS
        || document
            .palette
            .iter()
            .any(|color| color.rgba16().is_none())
    {
        return Err(FormatError::Invalid(
            "palette count or color type is invalid",
        ));
    }
    if main.id != document.main_plane_id
        || !matches!(
            main.pixel_format,
            PixelFormat::BinaryMask8 | PixelFormat::Grayscale8 | PixelFormat::Grayscale16
        )
        || color.id != document.color_plane_id
        || !matches!(
            color.pixel_format,
            PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
        )
    {
        return Err(FormatError::Invalid(
            "plane ID or pixel format is inconsistent",
        ));
    }
    let mut plane_ids = BTreeSet::new();
    for plane in &document.planes {
        if plane.id == 0
            || !plane_ids.insert(plane.id)
            || plane.width == 0
            || plane.height == 0
            || plane.width > inkpod_image::MAX_RASTER_DIMENSION
            || plane.height > inkpod_image::MAX_RASTER_DIMENSION
            || (plane.kind != PlaneKind::LightTable
                && (plane.width != document.width || plane.height != document.height))
            || (plane.kind == PlaneKind::LightTable
                && !matches!(
                    plane.pixel_format,
                    PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
                ))
            || (matches!(
                plane.kind,
                PlaneKind::VectorMainLine | PlaneKind::ColorTrace | PlaneKind::VectorFill
            ) && (plane.pixel_format != PixelFormat::StraightRgba8 || !plane.tiles.is_empty()))
        {
            return Err(FormatError::Invalid("plane manifest is inconsistent"));
        }
        let mut coords = BTreeSet::new();
        for tile in &plane.tiles {
            if !coords.insert(tile.coord) {
                return Err(FormatError::Invalid("duplicate tile coordinates"));
            }
            validate_tile_shape(
                plane.width,
                plane.height,
                plane.pixel_format,
                tile.coord,
                tile.width,
                tile.height,
                tile.bytes.len() as u64,
            )?;
            if plane.pixel_format == PixelFormat::BinaryMask8
                && tile.bytes.iter().any(|value| !matches!(*value, 0 | 255))
            {
                return Err(FormatError::Invalid(
                    "binary mask contains an intermediate value",
                ));
            }
        }
    }
    if let Some(metadata) = &document.m3 {
        validate_m3_metadata(metadata, Some(&document.planes))?;
        let width = i32::try_from(document.width)
            .map_err(|_| FormatError::Invalid("document width exceeds guide range"))?;
        let height = i32::try_from(document.height)
            .map_err(|_| FormatError::Invalid("document height exceeds guide range"))?;
        if metadata.guides.iter().any(|guide| match guide.axis {
            GuideAxis::Horizontal => !(0..=height).contains(&guide.position),
            GuideAxis::Vertical => !(0..=width).contains(&guide.position),
        }) {
            return Err(FormatError::Invalid(
                "guide position is outside the document",
            ));
        }
        let selection = document
            .planes
            .iter()
            .find(|plane| plane.id == metadata.selection_plane_id)
            .ok_or(FormatError::Invalid("selection plane is missing"))?;
        if selection.kind != PlaneKind::Selection
            || selection.pixel_format != PixelFormat::BinaryMask8
        {
            return Err(FormatError::Invalid(
                "selection plane kind or format is invalid",
            ));
        }
    }
    if let Some(metadata) = &document.m4 {
        if document.m3.is_none() {
            return Err(FormatError::Invalid("M4 metadata requires the M3 tree"));
        }
        let source_plane_ids: BTreeSet<_> = document
            .planes
            .iter()
            .filter(|plane| plane.kind == PlaneKind::LightTable)
            .map(|plane| plane.id)
            .collect();
        validate_m4_metadata(metadata, Some(&source_plane_ids))?;
        let mut occupied_ids = BTreeSet::from([document.document_id]);
        if let Some(m3) = &document.m3 {
            for layer in &m3.layers {
                if !occupied_ids.insert(layer.id) {
                    return Err(FormatError::Invalid(
                        "M4 state collides with an existing stable ID",
                    ));
                }
                for plane in &layer.planes {
                    if !occupied_ids.insert(plane.id) {
                        return Err(FormatError::Invalid(
                            "M4 state collides with an existing stable ID",
                        ));
                    }
                }
            }
            for id in m3
                .guides
                .iter()
                .map(|guide| guide.id)
                .chain([m3.selection_plane_id])
            {
                if !occupied_ids.insert(id) {
                    return Err(FormatError::Invalid(
                        "M4 state collides with an existing stable ID",
                    ));
                }
            }
        }
        for source_plane_id in &source_plane_ids {
            if !occupied_ids.insert(*source_plane_id) {
                return Err(FormatError::Invalid(
                    "M4 source plane collides with document state",
                ));
            }
        }
        for set in &metadata.sets {
            if !occupied_ids.insert(set.id) {
                return Err(FormatError::Invalid(
                    "M4 set ID collides with document state",
                ));
            }
            for item in &set.items {
                if !occupied_ids.insert(item.id) {
                    return Err(FormatError::Invalid(
                        "M4 item ID collides with document state",
                    ));
                }
                let source = document
                    .planes
                    .iter()
                    .find(|plane| plane.id == item.source_plane_id)
                    .ok_or(FormatError::Invalid("M4 source plane is missing"))?;
                if source.kind != PlaneKind::LightTable {
                    return Err(FormatError::Invalid("M4 source plane kind is invalid"));
                }
            }
        }
    } else if document
        .planes
        .iter()
        .any(|plane| plane.kind == PlaneKind::LightTable)
    {
        return Err(FormatError::Invalid(
            "light-table planes require M4 metadata",
        ));
    }

    let adjustment_layer_ids: BTreeSet<_> = document
        .m3
        .iter()
        .flat_map(|metadata| metadata.layers.iter())
        .filter(|layer| layer.kind == LayerKind::Adjustment)
        .map(|layer| layer.id)
        .collect();
    if let Some(metadata) = &document.m6 {
        if document.m3.is_none() || adjustment_layer_ids.is_empty() {
            return Err(FormatError::Invalid(
                "M6 metadata requires an M3 adjustment layer",
            ));
        }
        validate_m6_metadata(metadata, Some(&adjustment_layer_ids))?;
    } else if !adjustment_layer_ids.is_empty() {
        return Err(FormatError::Invalid(
            "adjustment layers require M6 metadata",
        ));
    }

    let mut stroke_plane_ids = BTreeSet::new();
    let mut fill_plane_ids = BTreeSet::new();
    let mut vector_layer_for_plane = std::collections::BTreeMap::new();
    let mut has_vector_layer = false;
    if let Some(m3) = &document.m3 {
        for layer in &m3.layers {
            let payloads: Vec<_> = layer
                .planes
                .iter()
                .map(|properties| {
                    document
                        .planes
                        .iter()
                        .find(|plane| plane.id == properties.id)
                        .ok_or(FormatError::Invalid("M5 vector plane payload is missing"))
                })
                .collect::<Result<_, _>>()?;
            if layer.kind == LayerKind::VectorColoring {
                has_vector_layer = true;
                let main_count = payloads
                    .iter()
                    .filter(|plane| plane.kind == PlaneKind::VectorMainLine)
                    .count();
                let trace_count = payloads
                    .iter()
                    .filter(|plane| plane.kind == PlaneKind::ColorTrace)
                    .count();
                let fill_count = payloads
                    .iter()
                    .filter(|plane| plane.kind == PlaneKind::VectorFill)
                    .count();
                if main_count != 1
                    || trace_count == 0
                    || fill_count != 1
                    || payloads.iter().any(|plane| {
                        !matches!(
                            plane.kind,
                            PlaneKind::VectorMainLine
                                | PlaneKind::ColorTrace
                                | PlaneKind::VectorFill
                                | PlaneKind::Raster
                        )
                    })
                {
                    return Err(FormatError::Invalid(
                        "M5 vector layer and plane types are inconsistent",
                    ));
                }
                for plane in payloads {
                    match plane.kind {
                        PlaneKind::VectorMainLine | PlaneKind::ColorTrace => {
                            stroke_plane_ids.insert(plane.id);
                            vector_layer_for_plane.insert(plane.id, layer.id);
                        }
                        PlaneKind::VectorFill => {
                            fill_plane_ids.insert(plane.id);
                            vector_layer_for_plane.insert(plane.id, layer.id);
                        }
                        _ => {}
                    }
                }
            } else if payloads.iter().any(|plane| {
                matches!(
                    plane.kind,
                    PlaneKind::VectorMainLine | PlaneKind::ColorTrace | PlaneKind::VectorFill
                )
            }) {
                return Err(FormatError::Invalid(
                    "M5 vector plane belongs to a non-vector layer",
                ));
            }
        }
    }
    if let Some(metadata) = &document.m5 {
        if document.m3.is_none() || !has_vector_layer {
            return Err(FormatError::Invalid(
                "M5 metadata requires an M3 vector layer",
            ));
        }
        validate_m5_metadata(
            metadata,
            Some(&stroke_plane_ids),
            Some(&fill_plane_ids),
            Some(&vector_layer_for_plane),
        )?;
        let mut occupied_ids = BTreeSet::from([document.document_id]);
        if let Some(m3) = &document.m3 {
            for layer in &m3.layers {
                occupied_ids.insert(layer.id);
                for plane in &layer.planes {
                    occupied_ids.insert(plane.id);
                }
            }
            for id in m3
                .guides
                .iter()
                .map(|guide| guide.id)
                .chain([m3.selection_plane_id])
            {
                occupied_ids.insert(id);
            }
        }
        if let Some(m4) = &document.m4 {
            for set in &m4.sets {
                occupied_ids.insert(set.id);
                for item in &set.items {
                    occupied_ids.insert(item.id);
                    occupied_ids.insert(item.source_plane_id);
                }
            }
        }
        for path in &metadata.paths {
            if !occupied_ids.insert(path.id) {
                return Err(FormatError::Invalid(
                    "M5 path collides with an existing stable ID",
                ));
            }
        }
        for fill in &metadata.fills {
            if !occupied_ids.insert(fill.id) {
                return Err(FormatError::Invalid(
                    "M5 fill collides with an existing stable ID",
                ));
            }
            let fill_layer = vector_layer_for_plane
                .get(&fill.plane_id)
                .ok_or(FormatError::Invalid("M5 fill plane is missing"))?;
            for boundary_id in &fill.boundary_path_ids {
                let boundary = metadata
                    .paths
                    .iter()
                    .find(|path| path.id == *boundary_id)
                    .ok_or(FormatError::Invalid("M5 fill boundary is missing"))?;
                if vector_layer_for_plane.get(&boundary.plane_id) != Some(fill_layer) {
                    return Err(FormatError::Invalid(
                        "M5 fill boundary crosses vector layers",
                    ));
                }
            }
        }
    } else if has_vector_layer
        || document.planes.iter().any(|plane| {
            matches!(
                plane.kind,
                PlaneKind::VectorMainLine | PlaneKind::ColorTrace | PlaneKind::VectorFill
            )
        })
    {
        return Err(FormatError::Invalid("vector layers require M5 metadata"));
    }
    Ok(())
}

fn legacy_main_line_color(document: &CellFile) -> Result<PixelValue, FormatError> {
    legacy_main_line_color_for_planes(&document.planes)
}

fn legacy_main_line_color_for_planes(planes: &[FilePlane]) -> Result<PixelValue, FormatError> {
    let color = planes
        .iter()
        .find(|plane| plane.kind == PlaneKind::Color)
        .ok_or(FormatError::Invalid("color plane is missing"))?;
    Ok(if color.pixel_format == PixelFormat::StraightRgba16 {
        PixelValue::Rgba16([0, 0, 0, u16::MAX])
    } else {
        PixelValue::Rgba([0, 0, 0, u8::MAX])
    })
}

fn validate_tile_shape(
    raster_width: u32,
    raster_height: u32,
    format: PixelFormat,
    coord: TileCoord,
    width: u32,
    height: u32,
    length: u64,
) -> Result<(), FormatError> {
    let origin_x = coord
        .x
        .checked_mul(inkpod_image::TILE_SIZE)
        .ok_or(FormatError::Invalid("tile X origin overflows"))?;
    let origin_y = coord
        .y
        .checked_mul(inkpod_image::TILE_SIZE)
        .ok_or(FormatError::Invalid("tile Y origin overflows"))?;
    if origin_x >= raster_width || origin_y >= raster_height {
        return Err(FormatError::Invalid("tile origin is outside its plane"));
    }
    let expected_width = inkpod_image::TILE_SIZE.min(raster_width - origin_x);
    let expected_height = inkpod_image::TILE_SIZE.min(raster_height - origin_y);
    let expected_length = u64::from(expected_width)
        .checked_mul(u64::from(expected_height))
        .and_then(|pixels| pixels.checked_mul(format.bytes_per_pixel() as u64))
        .ok_or(FormatError::Invalid("tile byte length overflows"))?;
    if width != expected_width || height != expected_height || length != expected_length {
        return Err(FormatError::Invalid(
            "tile dimensions or byte length are inconsistent",
        ));
    }
    Ok(())
}

const fn pixel_format_code(format: PixelFormat) -> u32 {
    match format {
        PixelFormat::BinaryMask8 => 1,
        PixelFormat::StraightRgba8 => 2,
        PixelFormat::PremultipliedBgra8 => 3,
        PixelFormat::Grayscale8 => 4,
        PixelFormat::Grayscale16 => 5,
        PixelFormat::StraightRgba16 => 6,
    }
}

fn pixel_format_from_code(value: u32) -> Result<PixelFormat, FormatError> {
    match value {
        1 => Ok(PixelFormat::BinaryMask8),
        2 => Ok(PixelFormat::StraightRgba8),
        3 => Ok(PixelFormat::PremultipliedBgra8),
        4 => Ok(PixelFormat::Grayscale8),
        5 => Ok(PixelFormat::Grayscale16),
        6 => Ok(PixelFormat::StraightRgba16),
        _ => Err(FormatError::Unsupported("unknown required pixel format")),
    }
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_color_value(output: &mut Vec<u8>, color: PixelValue) -> Result<(), FormatError> {
    let (depth, channels) = match color {
        PixelValue::Rgba(value) => (8_u32, value.map(u16::from)),
        PixelValue::Rgba16(value) => (16_u32, value),
        _ => return Err(FormatError::Invalid("color metadata value is not RGBA")),
    };
    push_u32(output, depth);
    push_u32(output, 0);
    for channel in channels {
        output.extend_from_slice(&channel.to_le_bytes());
    }
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(FormatError::Invalid("input cursor overflows"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(FormatError::Invalid("input is truncated"))?;
        self.position = end;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, FormatError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| FormatError::Invalid("u32 is truncated"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32, FormatError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| FormatError::Invalid("i32 is truncated"))?;
        Ok(i32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, FormatError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| FormatError::Invalid("u64 is truncated"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn color_value(&mut self) -> Result<PixelValue, FormatError> {
        let depth = self.u32()?;
        if self.u32()? != 0 {
            return Err(FormatError::Unsupported(
                "color metadata record reserved field is not zero",
            ));
        }
        let mut channels = [0_u16; 4];
        for channel in &mut channels {
            let bytes: [u8; 2] = self
                .take(2)?
                .try_into()
                .map_err(|_| FormatError::Invalid("color metadata is truncated"))?;
            *channel = u16::from_le_bytes(bytes);
        }
        match depth {
            8 if channels
                .iter()
                .all(|channel| *channel <= u16::from(u8::MAX)) =>
            {
                Ok(PixelValue::Rgba(channels.map(|channel| channel as u8)))
            }
            16 => Ok(PixelValue::Rgba16(channels)),
            8 => Err(FormatError::Invalid(
                "8-bit color metadata contains a channel above 255",
            )),
            _ => Err(FormatError::Unsupported(
                "color metadata depth is not supported",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> CellFile {
        CellFile {
            document_uuid: [
                0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba,
                0xdc, 0xfe,
            ],
            document_id: 1,
            layer_id: 2,
            main_plane_id: 3,
            color_plane_id: 4,
            width: 65,
            height: 65,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
            frames: FrameMetadata {
                hundred_frame: RectI32 {
                    x: 0,
                    y: 0,
                    width: 65,
                    height: 65,
                },
                reference_frame: RectI32 {
                    x: 32,
                    y: 32,
                    width: 65,
                    height: 65,
                },
                drawing_frame: RectI32 {
                    x: 0,
                    y: 0,
                    width: 65,
                    height: 65,
                },
                safe_frame: RectI32 {
                    x: 3,
                    y: 3,
                    width: 59,
                    height: 59,
                },
                margins: Margins::default(),
            },
            main_line_color: PixelValue::Rgba([0, 0, 0, 255]),
            palette: vec![
                PixelValue::Rgba([12, 34, 56, 255]),
                PixelValue::Rgba16([1, 257, 32_769, 65_534]),
            ],
            planes: vec![
                FilePlane {
                    id: 3,
                    kind: PlaneKind::MainLine,
                    pixel_format: PixelFormat::BinaryMask8,
                    width: 65,
                    height: 65,
                    tiles: vec![FileTile {
                        coord: TileCoord { x: 1, y: 1 },
                        width: 1,
                        height: 1,
                        bytes: vec![255],
                    }],
                },
                FilePlane {
                    id: 4,
                    kind: PlaneKind::Color,
                    pixel_format: PixelFormat::StraightRgba8,
                    width: 65,
                    height: 65,
                    tiles: vec![FileTile {
                        coord: TileCoord { x: 1, y: 1 },
                        width: 1,
                        height: 1,
                        bytes: vec![1, 2, 3, 255],
                    }],
                },
            ],
            m3: None,
            m4: None,
            m5: None,
            m6: None,
        }
    }

    fn m3_fixture() -> CellFile {
        let mut document = fixture();
        document.planes.push(FilePlane {
            id: 5,
            kind: PlaneKind::Selection,
            pixel_format: PixelFormat::BinaryMask8,
            width: document.width,
            height: document.height,
            tiles: Vec::new(),
        });
        document.m3 = Some(FileM3Metadata {
            active_layer_id: 2,
            active_plane_id: 3,
            selection_plane_id: 5,
            layers: vec![FileLayer {
                id: 2,
                kind: LayerKind::BinaryColoring,
                name: "Coloring".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                planes: vec![
                    FilePlaneProperties {
                        id: 3,
                        name: "Main".to_owned(),
                        visible: true,
                        editable: true,
                        opacity_milli: 1_000,
                    },
                    FilePlaneProperties {
                        id: 4,
                        name: "Color".to_owned(),
                        visible: true,
                        editable: true,
                        opacity_milli: 1_000,
                    },
                ],
            }],
            guides: vec![FileGuide {
                id: 6,
                axis: GuideAxis::Vertical,
                position: 32,
            }],
            grid: FileGrid {
                origin_x: 0,
                origin_y: 0,
                spacing_x: 16,
                spacing_y: 16,
                subdivisions: 2,
            },
        });
        document
    }

    fn m4_fixture() -> CellFile {
        let mut document = m3_fixture();
        document.planes.push(FilePlane {
            id: 9,
            kind: PlaneKind::LightTable,
            pixel_format: PixelFormat::StraightRgba8,
            width: 4,
            height: 3,
            tiles: vec![FileTile {
                coord: TileCoord { x: 0, y: 0 },
                width: 4,
                height: 3,
                bytes: [10_u8, 20, 30, 255].repeat(12),
            }],
        });
        document.m4 = Some(FileM4Metadata {
            active_set_id: 7,
            sets: vec![FileLightTableSet {
                id: 7,
                name: "Default".to_owned(),
                global_opacity_milli: 500,
                items: vec![FileLightTableItem {
                    id: 8,
                    source_plane_id: 9,
                    source_document_uuid: 0x1234_u128.to_le_bytes(),
                    source_revision: 9,
                    source_reference_frame: RectI32 {
                        x: 2,
                        y: 1,
                        width: 4,
                        height: 3,
                    },
                    source_dpi_x_milli: 96_000,
                    source_dpi_y_milli: 96_000,
                    name: "Reference".to_owned(),
                    visible: true,
                    opacity_milli: 500,
                    display_mode: LightTableDisplayMode::Color,
                    display_color: PixelValue::Rgba([0, 128, 255, 255]),
                    translate_x_milli: 0,
                    translate_y_milli: 0,
                    scale_x_milli: 1_000,
                    scale_y_milli: 1_000,
                    rotation_milli_degrees: 0,
                }],
            }],
        });
        document
    }

    fn m5_fixture() -> CellFile {
        let mut document = m3_fixture();
        for (id, kind) in [
            (8, PlaneKind::VectorMainLine),
            (9, PlaneKind::ColorTrace),
            (10, PlaneKind::VectorFill),
        ] {
            document.planes.push(FilePlane {
                id,
                kind,
                pixel_format: PixelFormat::StraightRgba8,
                width: document.width,
                height: document.height,
                tiles: Vec::new(),
            });
        }
        document.m3.as_mut().unwrap().layers.push(FileLayer {
            id: 7,
            kind: LayerKind::VectorColoring,
            name: "Vector".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes: [(8, "Vector Main"), (9, "Color Trace"), (10, "Vector Fill")]
                .into_iter()
                .map(|(id, name)| FilePlaneProperties {
                    id,
                    name: name.to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                })
                .collect(),
        });
        let point = |x_milli, y_milli| FileVectorPoint { x_milli, y_milli };
        let line = |p0: FileVectorPoint, p3: FileVectorPoint| FileVectorSegment {
            p0,
            p1: FileVectorPoint {
                x_milli: (p0.x_milli * 2 + p3.x_milli) / 3,
                y_milli: (p0.y_milli * 2 + p3.y_milli) / 3,
            },
            p2: FileVectorPoint {
                x_milli: (p0.x_milli + p3.x_milli * 2) / 3,
                y_milli: (p0.y_milli + p3.y_milli * 2) / 3,
            },
            p3,
            width_start_milli: 1_000,
            width_end_milli: 2_000,
        };
        let corners = [
            point(1_000, 1_000),
            point(5_000, 1_000),
            point(5_000, 5_000),
            point(1_000, 5_000),
            point(1_000, 1_000),
        ];
        document.m5 = Some(FileM5Metadata {
            paths: vec![FileVectorPath {
                id: 11,
                plane_id: 9,
                color: PixelValue::Rgba16([1, 2, 3, 65_535]),
                closed: true,
                segments: corners
                    .windows(2)
                    .map(|pair| line(pair[0], pair[1]))
                    .collect(),
            }],
            fills: vec![FileVectorFill {
                id: 12,
                plane_id: 10,
                color: PixelValue::Rgba([20, 40, 60, 200]),
                boundary_path_ids: vec![11],
            }],
        });
        document
    }

    #[test]
    fn io_001_manifest_and_blobs_round_trip() {
        let document = fixture();
        let bytes = encode(&document).unwrap();
        assert_eq!(decode(&bytes).unwrap(), document);
    }

    #[test]
    fn m2_color_metadata_round_trips_and_legacy_v1_defaults_remain_readable() {
        let mut document = fixture();
        document.main_line_color = PixelValue::Rgba16([1_001, 2_002, 3_003, 65_535]);
        let decoded = decode(&encode(&document).unwrap()).unwrap();
        assert_eq!(decoded.main_line_color, document.main_line_color);
        assert_eq!(decoded.palette, document.palette);

        let mut legacy = fixture();
        legacy.palette.clear();
        legacy.main_line_color = PixelValue::Rgba([0, 0, 0, 255]);
        let legacy_bytes = encode_with_color_metadata(&legacy, false).unwrap();
        assert_eq!(decode(&legacy_bytes).unwrap(), legacy);
    }

    #[test]
    fn m2_grayscale_and_rgba16_tiles_round_trip_without_quantization() {
        let mut document = fixture();
        document.planes[0].pixel_format = PixelFormat::Grayscale16;
        document.planes[0].tiles[0].bytes = 0x1234_u16.to_le_bytes().to_vec();
        document.planes[1].pixel_format = PixelFormat::StraightRgba16;
        let exact = [1_u16, 257, 32_769, 65_534];
        document.planes[1].tiles[0].bytes = exact.into_iter().flat_map(u16::to_le_bytes).collect();

        let decoded = decode(&encode(&document).unwrap()).unwrap();
        assert_eq!(decoded, document);
        assert_eq!(decoded.planes[1].tiles[0].bytes[0..2], [1, 0]);
    }

    #[test]
    fn m3_metadata_rejects_out_of_bounds_guides_grid_and_unreferenced_payloads() {
        let document = m3_fixture();
        assert_eq!(decode(&encode(&document).unwrap()).unwrap(), document);

        let mut invalid_guide = document.clone();
        invalid_guide.m3.as_mut().unwrap().guides[0].position = 66;
        assert!(matches!(
            encode(&invalid_guide),
            Err(FormatError::Invalid(
                "guide position is outside the document"
            ))
        ));

        let mut invalid_grid = document.clone();
        invalid_grid.m3.as_mut().unwrap().grid.spacing_x = 1_048_577;
        assert!(matches!(
            encode(&invalid_grid),
            Err(FormatError::Invalid(
                "M3 metadata values are outside bounds"
            ))
        ));

        let mut unreferenced = document;
        let mut extra = unreferenced.planes[1].clone();
        extra.id = 7;
        unreferenced.planes.push(extra);
        assert!(matches!(
            encode(&unreferenced),
            Err(FormatError::Invalid("M3 tree and plane payload IDs differ"))
        ));
    }

    #[test]
    fn m4_metadata_round_trips_and_rejects_malformed_source_relationships() {
        let document = m4_fixture();
        let encoded = encode(&document).unwrap();
        assert_eq!(decode(&encoded).unwrap(), document);

        let mut invalid_opacity = m4_fixture();
        invalid_opacity.m4.as_mut().unwrap().sets[0].items[0].opacity_milli = 1_001;
        assert!(matches!(
            encode(&invalid_opacity),
            Err(FormatError::Invalid(_))
        ));

        let mut missing_source = m4_fixture();
        missing_source.planes.retain(|plane| plane.id != 9);
        assert!(matches!(
            encode(&missing_source),
            Err(FormatError::Invalid(_))
        ));

        let mut colliding_source = m4_fixture();
        colliding_source
            .planes
            .iter_mut()
            .find(|plane| plane.kind == PlaneKind::LightTable)
            .unwrap()
            .id = 6;
        colliding_source.m4.as_mut().unwrap().sets[0].items[0].source_plane_id = 6;
        assert!(matches!(
            encode(&colliding_source),
            Err(FormatError::Invalid(
                "M4 source plane collides with document state"
            ))
        ));

        let mut minimum_rotation = m4_fixture();
        minimum_rotation.m4.as_mut().unwrap().sets[0].items[0].rotation_milli_degrees = i32::MIN;
        assert!(matches!(
            encode(&minimum_rotation),
            Err(FormatError::Invalid("M4 item properties are invalid"))
        ));

        let mut no_tree = m4_fixture();
        no_tree.m3 = None;
        assert!(matches!(encode(&no_tree), Err(FormatError::Invalid(_))));
    }

    #[test]
    fn m5_vector_metadata_round_trips_and_rejects_malformed_topology() {
        let document = m5_fixture();
        assert_eq!(decode(&encode(&document).unwrap()).unwrap(), document);

        let mut missing_boundary = m5_fixture();
        missing_boundary.m5.as_mut().unwrap().fills[0].boundary_path_ids[0] = 99;
        assert!(matches!(
            encode(&missing_boundary),
            Err(FormatError::Invalid(_))
        ));

        let mut open_boundary = m5_fixture();
        open_boundary.m5.as_mut().unwrap().paths[0].closed = false;
        assert!(matches!(
            encode(&open_boundary),
            Err(FormatError::Invalid(_))
        ));

        let mut cross_layer = m5_fixture();
        for (id, kind) in [
            (14, PlaneKind::VectorMainLine),
            (15, PlaneKind::ColorTrace),
            (16, PlaneKind::VectorFill),
        ] {
            cross_layer.planes.push(FilePlane {
                id,
                kind,
                pixel_format: PixelFormat::StraightRgba8,
                width: cross_layer.width,
                height: cross_layer.height,
                tiles: Vec::new(),
            });
        }
        cross_layer.m3.as_mut().unwrap().layers.push(FileLayer {
            id: 13,
            kind: LayerKind::VectorColoring,
            name: "Other vector".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes: [(14, "Main"), (15, "Trace"), (16, "Fill")]
                .into_iter()
                .map(|(id, name)| FilePlaneProperties {
                    id,
                    name: name.to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                })
                .collect(),
        });
        cross_layer.m5.as_mut().unwrap().fills[0].plane_id = 16;
        assert!(matches!(
            encode(&cross_layer),
            Err(FormatError::Invalid(
                "M5 fill boundary crosses vector layers"
            ))
        ));

        let mut out_of_bounds = m5_fixture();
        out_of_bounds.m5.as_mut().unwrap().paths[0].segments[0]
            .p1
            .x_milli = i32::MAX;
        assert!(matches!(
            encode(&out_of_bounds),
            Err(FormatError::Invalid(
                "M5 segment coordinate is outside bounds"
            ))
        ));

        let mut colliding_path = m5_fixture();
        colliding_path.m5.as_mut().unwrap().paths[0].id = 6;
        colliding_path.m5.as_mut().unwrap().fills[0].boundary_path_ids[0] = 6;
        assert!(matches!(
            encode(&colliding_path),
            Err(FormatError::Invalid(
                "M5 path collides with an existing stable ID"
            ))
        ));

        let mut missing_metadata = m5_fixture();
        missing_metadata.m5 = None;
        assert!(matches!(
            encode(&missing_metadata),
            Err(FormatError::Invalid(_))
        ));
    }

    #[test]
    fn io_001_rejects_truncation_and_checksum_mismatch() {
        let mut bytes = encode(&fixture()).unwrap();
        assert!(decode(&bytes[..bytes.len() - 1]).is_err());
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        assert!(matches!(decode(&bytes), Err(FormatError::ChecksumMismatch)));
        let mut trailing = encode(&fixture()).unwrap();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
    }

    #[test]
    fn m6_adjustment_metadata_round_trips_and_rejects_malformed_relationships() {
        let mut document = m3_fixture();
        document.m3.as_mut().unwrap().layers.insert(
            0,
            FileLayer {
                id: 100,
                kind: LayerKind::Adjustment,
                name: "M6 Adjustment".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                planes: Vec::new(),
            },
        );
        document.m6 = Some(FileM6Metadata {
            adjustments: vec![FileAdjustmentLayer {
                layer_id: 100,
                adjustment: inkpod_image::Adjustment::BrightnessContrast {
                    brightness_milli: 125,
                    contrast_milli: -250,
                },
            }],
        });
        assert_eq!(decode(&encode(&document).unwrap()).unwrap(), document);

        let mut missing = document.clone();
        missing.m6 = None;
        assert!(matches!(
            encode(&missing),
            Err(FormatError::Invalid(
                "adjustment layers require M6 metadata"
            ))
        ));

        let mut duplicate = document.clone();
        let duplicate_adjustment = duplicate.m6.as_ref().unwrap().adjustments[0].clone();
        duplicate
            .m6
            .as_mut()
            .unwrap()
            .adjustments
            .push(duplicate_adjustment);
        assert!(matches!(
            encode(&duplicate),
            Err(FormatError::Invalid("M6 adjustment properties are invalid"))
        ));

        let mut wrong_layer = document.clone();
        wrong_layer.m6.as_mut().unwrap().adjustments[0].layer_id = 101;
        assert!(matches!(
            encode(&wrong_layer),
            Err(FormatError::Invalid("M6 adjustment properties are invalid"))
        ));

        let mut invalid_parameter = document;
        invalid_parameter.m6.as_mut().unwrap().adjustments[0].adjustment =
            inkpod_image::Adjustment::BrightnessContrast {
                brightness_milli: 1_001,
                contrast_milli: 0,
            };
        assert!(matches!(
            encode(&invalid_parameter),
            Err(FormatError::Invalid("M6 adjustment properties are invalid"))
        ));
    }

    #[test]
    fn io_001_atomic_save_cancel_keeps_existing_destination() {
        let directory = std::env::temp_dir().join(format!(
            "inkpod-format-test-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("cell.inkpod");
        fs::write(&path, b"original").unwrap();
        let mut checks = 0;
        let result = save_atomic_with_cancel(&path, &fixture(), || {
            checks += 1;
            checks == 2
        });
        assert!(matches!(result, Err(FormatError::Cancelled)));
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn io_001_atomic_save_replaces_an_existing_container() {
        let directory = std::env::temp_dir().join(format!(
            "inkpod-format-replace-test-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("cell.inkpod");
        let first = fixture();
        save_atomic(&path, &first).unwrap();
        let mut second = first.clone();
        second.planes[1].tiles[0].bytes = vec![9, 8, 7, 255];
        save_atomic(&path, &second).unwrap();
        assert_eq!(read(&path).unwrap(), second);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&directory).unwrap();
    }
}
