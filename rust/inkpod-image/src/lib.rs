#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

pub const TILE_SIZE: u32 = 64;
pub const MAX_RASTER_DIMENSION: u32 = 1_048_576;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    BinaryMask8,
    StraightRgba8,
    PremultipliedBgra8,
}

impl PixelFormat {
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::BinaryMask8 => 1,
            Self::StraightRgba8 | Self::PremultipliedBgra8 => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TileCoord {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelValue {
    Binary(u8),
    Rgba([u8; 4]),
}

impl PixelValue {
    #[must_use]
    pub const fn is_zero(self) -> bool {
        match self {
            Self::Binary(value) => value == 0,
            Self::Rgba(value) => value[0] == 0 && value[1] == 0 && value[2] == 0 && value[3] == 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RasterError {
    InvalidDimensions,
    PixelOutOfBounds,
    PixelFormatMismatch,
    InvalidTile,
}

impl fmt::Display for RasterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidDimensions => "raster dimensions are zero or exceed the bounded limit",
            Self::PixelOutOfBounds => "pixel coordinate is outside the raster",
            Self::PixelFormatMismatch => "pixel value does not match the raster pixel format",
            Self::InvalidTile => "tile coordinates or byte length are invalid",
        })
    }
}

impl std::error::Error for RasterError {}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Tile {
    bytes: Vec<u8>,
    revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TileData {
    pub coord: TileCoord,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    pub revision: u64,
}

/// Sparse raster whose allocated tiles are shared until the next write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TileRaster {
    width: u32,
    height: u32,
    format: PixelFormat,
    tiles: BTreeMap<TileCoord, Arc<Tile>>,
}

impl TileRaster {
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Result<Self, RasterError> {
        if width == 0
            || height == 0
            || width > MAX_RASTER_DIMENSION
            || height > MAX_RASTER_DIMENSION
        {
            return Err(RasterError::InvalidDimensions);
        }
        Ok(Self {
            width,
            height,
            format,
            tiles: BTreeMap::new(),
        })
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn format(&self) -> PixelFormat {
        self.format
    }

    #[must_use]
    pub fn allocated_tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn allocated_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles.keys().copied()
    }

    #[must_use]
    pub fn tile_revision(&self, coord: TileCoord) -> u64 {
        self.tiles.get(&coord).map_or(0, |tile| tile.revision)
    }

    pub fn pixel(&self, x: u32, y: u32) -> Result<PixelValue, RasterError> {
        self.validate_pixel(x, y)?;
        let coord = TileCoord {
            x: x / TILE_SIZE,
            y: y / TILE_SIZE,
        };
        let local_x = x % TILE_SIZE;
        let local_y = y % TILE_SIZE;
        let Some(tile) = self.tiles.get(&coord) else {
            return Ok(self.zero_pixel());
        };
        Ok(self.read_tile_pixel(tile, local_x, local_y))
    }

    /// Writes one pixel and returns its previous value. A zero/transparent write
    /// to an unallocated tile remains sparse.
    pub fn set_pixel(
        &mut self,
        x: u32,
        y: u32,
        value: PixelValue,
        revision: u64,
    ) -> Result<PixelValue, RasterError> {
        self.validate_pixel(x, y)?;
        self.validate_value(value)?;
        let coord = TileCoord {
            x: x / TILE_SIZE,
            y: y / TILE_SIZE,
        };
        let local_x = x % TILE_SIZE;
        let local_y = y % TILE_SIZE;
        let previous = self.pixel(x, y)?;
        if previous == value {
            return Ok(previous);
        }
        if !self.tiles.contains_key(&coord) && value.is_zero() {
            return Ok(previous);
        }

        let bytes_per_pixel = self.format.bytes_per_pixel();
        let tile_bytes = (TILE_SIZE as usize)
            .checked_mul(TILE_SIZE as usize)
            .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
            .ok_or(RasterError::InvalidTile)?;
        let tile = self.tiles.entry(coord).or_insert_with(|| {
            Arc::new(Tile {
                bytes: vec![0; tile_bytes],
                revision,
            })
        });
        let tile = Arc::make_mut(tile);
        let offset = ((local_y as usize * TILE_SIZE as usize) + local_x as usize) * bytes_per_pixel;
        match value {
            PixelValue::Binary(value) => tile.bytes[offset] = value,
            PixelValue::Rgba(value) => tile.bytes[offset..offset + 4].copy_from_slice(&value),
        }
        tile.revision = revision;
        Ok(previous)
    }

    pub fn remove_tile_if_empty(&mut self, coord: TileCoord) {
        if self
            .tiles
            .get(&coord)
            .is_some_and(|tile| tile.bytes.iter().all(|byte| *byte == 0))
        {
            self.tiles.remove(&coord);
        }
    }

    #[must_use]
    pub fn tile_data(&self, coord: TileCoord) -> Option<TileData> {
        let tile = self.tiles.get(&coord)?;
        let (width, height) = self.tile_dimensions(coord)?;
        let bytes_per_pixel = self.format.bytes_per_pixel();
        let row_bytes = width as usize * bytes_per_pixel;
        let mut bytes = Vec::with_capacity(row_bytes * height as usize);
        for row in 0..height as usize {
            let start = row * TILE_SIZE as usize * bytes_per_pixel;
            bytes.extend_from_slice(&tile.bytes[start..start + row_bytes]);
        }
        Some(TileData {
            coord,
            width,
            height,
            bytes,
            revision: tile.revision,
        })
    }

    pub fn insert_tile(&mut self, data: TileData) -> Result<(), RasterError> {
        let Some((width, height)) = self.tile_dimensions(data.coord) else {
            return Err(RasterError::InvalidTile);
        };
        if data.width != width || data.height != height {
            return Err(RasterError::InvalidTile);
        }
        let bytes_per_pixel = self.format.bytes_per_pixel();
        let compact_row = width as usize * bytes_per_pixel;
        let expected = compact_row
            .checked_mul(height as usize)
            .ok_or(RasterError::InvalidTile)?;
        if data.bytes.len() != expected {
            return Err(RasterError::InvalidTile);
        }
        if self.format == PixelFormat::BinaryMask8
            && data.bytes.iter().any(|value| !matches!(*value, 0 | 255))
        {
            return Err(RasterError::PixelFormatMismatch);
        }
        let full_len = TILE_SIZE as usize * TILE_SIZE as usize * bytes_per_pixel;
        let mut bytes = vec![0; full_len];
        for row in 0..height as usize {
            let source = row * compact_row;
            let destination = row * TILE_SIZE as usize * bytes_per_pixel;
            bytes[destination..destination + compact_row]
                .copy_from_slice(&data.bytes[source..source + compact_row]);
        }
        if bytes.iter().any(|byte| *byte != 0) {
            self.tiles.insert(
                data.coord,
                Arc::new(Tile {
                    bytes,
                    revision: data.revision,
                }),
            );
        }
        Ok(())
    }

    #[must_use]
    pub fn checksum(&self) -> u64 {
        let mut checksum = FNV_OFFSET;
        checksum = fnv_bytes(checksum, &self.width.to_le_bytes());
        checksum = fnv_bytes(checksum, &self.height.to_le_bytes());
        checksum = fnv_bytes(checksum, &[self.format as u8]);
        for (coord, tile) in &self.tiles {
            checksum = fnv_bytes(checksum, &coord.x.to_le_bytes());
            checksum = fnv_bytes(checksum, &coord.y.to_le_bytes());
            checksum = fnv_bytes(checksum, &tile.bytes);
        }
        checksum
    }

    fn validate_pixel(&self, x: u32, y: u32) -> Result<(), RasterError> {
        if x >= self.width || y >= self.height {
            Err(RasterError::PixelOutOfBounds)
        } else {
            Ok(())
        }
    }

    fn validate_value(&self, value: PixelValue) -> Result<(), RasterError> {
        match (self.format, value) {
            (PixelFormat::BinaryMask8, PixelValue::Binary(0 | 255))
            | (PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8, PixelValue::Rgba(_)) => {
                Ok(())
            }
            _ => Err(RasterError::PixelFormatMismatch),
        }
    }

    const fn zero_pixel(&self) -> PixelValue {
        match self.format {
            PixelFormat::BinaryMask8 => PixelValue::Binary(0),
            PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8 => {
                PixelValue::Rgba([0; 4])
            }
        }
    }

    fn read_tile_pixel(&self, tile: &Tile, local_x: u32, local_y: u32) -> PixelValue {
        let bytes_per_pixel = self.format.bytes_per_pixel();
        let offset = ((local_y as usize * TILE_SIZE as usize) + local_x as usize) * bytes_per_pixel;
        match self.format {
            PixelFormat::BinaryMask8 => PixelValue::Binary(tile.bytes[offset]),
            PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8 => PixelValue::Rgba([
                tile.bytes[offset],
                tile.bytes[offset + 1],
                tile.bytes[offset + 2],
                tile.bytes[offset + 3],
            ]),
        }
    }

    fn tile_dimensions(&self, coord: TileCoord) -> Option<(u32, u32)> {
        let origin_x = coord.x.checked_mul(TILE_SIZE)?;
        let origin_y = coord.y.checked_mul(TILE_SIZE)?;
        if origin_x >= self.width || origin_y >= self.height {
            return None;
        }
        Some((
            TILE_SIZE.min(self.width - origin_x),
            TILE_SIZE.min(self.height - origin_y),
        ))
    }
}

pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[must_use]
pub fn fnv_bytes(mut checksum: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        checksum ^= u64::from(*byte);
        checksum = checksum.wrapping_mul(FNV_PRIME);
    }
    checksum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_tiles_are_copy_on_write_and_edge_tiles_are_compact() {
        let mut raster = TileRaster::new(65, 65, PixelFormat::StraightRgba8).unwrap();
        assert_eq!(raster.allocated_tile_count(), 0);
        raster
            .set_pixel(64, 64, PixelValue::Rgba([1, 2, 3, 255]), 7)
            .unwrap();
        let mut copy = raster.clone();
        copy.set_pixel(64, 64, PixelValue::Rgba([4, 5, 6, 255]), 8)
            .unwrap();

        assert_eq!(
            raster.pixel(64, 64).unwrap(),
            PixelValue::Rgba([1, 2, 3, 255])
        );
        assert_eq!(
            copy.pixel(64, 64).unwrap(),
            PixelValue::Rgba([4, 5, 6, 255])
        );
        let data = raster.tile_data(TileCoord { x: 1, y: 1 }).unwrap();
        assert_eq!((data.width, data.height, data.bytes.len()), (1, 1, 4));
    }

    #[test]
    fn transparent_write_does_not_allocate_and_empty_tile_can_be_removed() {
        let mut raster = TileRaster::new(128, 128, PixelFormat::BinaryMask8).unwrap();
        raster.set_pixel(2, 3, PixelValue::Binary(0), 1).unwrap();
        assert_eq!(raster.allocated_tile_count(), 0);
        raster.set_pixel(2, 3, PixelValue::Binary(255), 2).unwrap();
        raster.set_pixel(2, 3, PixelValue::Binary(0), 3).unwrap();
        raster.remove_tile_if_empty(TileCoord { x: 0, y: 0 });
        assert_eq!(raster.allocated_tile_count(), 0);
    }

    #[test]
    fn straight_alpha_preserves_rgb_when_alpha_is_zero() {
        let mut raster = TileRaster::new(64, 64, PixelFormat::StraightRgba8).unwrap();
        let value = PixelValue::Rgba([12, 34, 56, 0]);
        raster.set_pixel(1, 2, value, 1).unwrap();
        assert_eq!(raster.pixel(1, 2).unwrap(), value);
        assert_eq!(raster.allocated_tile_count(), 1);
    }

    #[test]
    fn binary_mask_rejects_intermediate_values() {
        let mut raster = TileRaster::new(64, 64, PixelFormat::BinaryMask8).unwrap();
        assert_eq!(
            raster.set_pixel(0, 0, PixelValue::Binary(1), 1),
            Err(RasterError::PixelFormatMismatch)
        );
        assert_eq!(
            raster.insert_tile(TileData {
                coord: TileCoord { x: 0, y: 0 },
                width: 64,
                height: 64,
                bytes: vec![1; 64 * 64],
                revision: 1,
            }),
            Err(RasterError::PixelFormatMismatch)
        );
    }
}
