#![forbid(unsafe_code)]

use inkpod_image::{FNV_OFFSET, PixelFormat, TileCoord, fnv_bytes};
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
const PLANE_DESCRIPTOR_BYTES: usize = 32;
const BLOB_DESCRIPTOR_BYTES: usize = 48;
const MAX_FILE_BYTES: u64 = 1 << 30;
const MAX_MANIFEST_BYTES: u64 = 16 << 20;
const MAX_PLANES: usize = 4_096;
const MAX_BLOBS: usize = 262_144;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaneKind {
    MainLine,
    Color,
}

impl PlaneKind {
    const fn code(self) -> u32 {
        match self {
            Self::MainLine => 1,
            Self::Color => 2,
        }
    }

    fn from_code(value: u32) -> Result<Self, FormatError> {
        match value {
            1 => Ok(Self::MainLine),
            2 => Ok(Self::Color),
            _ => Err(FormatError::Unsupported("unknown required plane kind")),
        }
    }
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
    pub planes: Vec<FilePlane>,
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
    validate_document(document)?;
    let blob_count = document.planes.iter().try_fold(0_usize, |count, plane| {
        count
            .checked_add(plane.tiles.len())
            .ok_or(FormatError::Invalid("blob count overflows"))
    })?;
    if blob_count > MAX_BLOBS {
        return Err(FormatError::Invalid("too many blobs"));
    }
    let manifest_len = FIXED_MANIFEST_BYTES
        .checked_add(
            document
                .planes
                .len()
                .checked_mul(PLANE_DESCRIPTOR_BYTES)
                .ok_or(FormatError::Invalid("plane manifest overflows"))?,
        )
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
    push_u32(&mut output, 0);
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
    if reader.u32()? != 0 {
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
    let expected_manifest_len = FIXED_MANIFEST_BYTES
        .checked_add(
            plane_count
                .checked_mul(PLANE_DESCRIPTOR_BYTES)
                .ok_or(FormatError::Invalid("plane manifest overflows"))?,
        )
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
        if descriptor.width != width || descriptor.height != height {
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
        planes,
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
    if document.planes.len() != 2 {
        return Err(FormatError::Invalid(
            "M1 cell must contain exactly two planes",
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
    if main.id != document.main_plane_id
        || main.pixel_format != PixelFormat::BinaryMask8
        || color.id != document.color_plane_id
        || color.pixel_format != PixelFormat::StraightRgba8
    {
        return Err(FormatError::Invalid(
            "plane ID or pixel format is inconsistent",
        ));
    }
    let mut plane_ids = BTreeSet::new();
    for plane in &document.planes {
        if plane.id == 0
            || !plane_ids.insert(plane.id)
            || plane.width != document.width
            || plane.height != document.height
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
    Ok(())
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
    }
}

fn pixel_format_from_code(value: u32) -> Result<PixelFormat, FormatError> {
    match value {
        1 => Ok(PixelFormat::BinaryMask8),
        2 => Ok(PixelFormat::StraightRgba8),
        3 => Ok(PixelFormat::PremultipliedBgra8),
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
        }
    }

    #[test]
    fn io_001_manifest_and_blobs_round_trip() {
        let document = fixture();
        let bytes = encode(&document).unwrap();
        assert_eq!(decode(&bytes).unwrap(), document);
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
