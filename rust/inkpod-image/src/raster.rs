use crate::{PixelFormat, PixelValue};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, OnceLock};

pub const TILE_SIZE: u32 = 64;
pub const MAX_RASTER_DIMENSION: u32 = 1_048_576;
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TileCoord {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RasterError {
    InvalidDimensions,
    PixelOutOfBounds,
    PixelFormatMismatch,
    InvalidTile,
    Cancelled,
}

impl fmt::Display for RasterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidDimensions => "raster dimensions are zero or exceed the bounded limit",
            Self::PixelOutOfBounds => "pixel coordinate is outside the raster",
            Self::PixelFormatMismatch => "pixel value does not match the raster pixel format",
            Self::InvalidTile => "tile coordinates or byte length are invalid",
            Self::Cancelled => "image edit was cancelled before commit",
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

/// Borrowed view of one allocated raster tile.
///
/// [`Self::bytes`] exposes the complete fixed-size tile backing store without
/// copying it. For an edge tile, only the rectangle described by
/// [`Self::width`] and [`Self::height`] is logical image content; callers must
/// advance rows by [`Self::row_stride_bytes`] and must not interpret padding as
/// document pixels. The borrow remains valid only while the source
/// [`TileRaster`] is immutably borrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileView<'a> {
    coord: TileCoord,
    width: u32,
    height: u32,
    row_stride_bytes: u32,
    bytes: &'a [u8],
    revision: u64,
}

impl<'a> TileView<'a> {
    /// Returns the tile coordinate within its raster.
    #[must_use]
    pub const fn coord(self) -> TileCoord {
        self.coord
    }

    /// Returns the logical width in pixels, excluding edge padding.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the logical height in pixels, excluding edge padding.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns the byte distance between adjacent rows in [`Self::bytes`].
    #[must_use]
    pub const fn row_stride_bytes(self) -> u32 {
        self.row_stride_bytes
    }

    /// Borrows the complete fixed-size tile backing bytes without allocation.
    #[must_use]
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the revision assigned by the most recent effective tile write.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// Sparse raster whose allocated tiles are shared until the next write.
#[derive(Clone)]
pub struct TileRaster {
    width: u32,
    height: u32,
    format: PixelFormat,
    // Immutable clones share the ordered tile index as well as each tile's
    // pixels. The first effective mutation detaches this metadata map, then
    // detaches only the tile whose pixels change.
    tiles: Arc<BTreeMap<TileCoord, Arc<Tile>>>,
    // Clones of one immutable raster share both cold and populated cache state.
    // A checksum-input mutation detaches this value before publishing pixels.
    checksum_cache: Arc<OnceLock<u64>>,
}

impl fmt::Debug for TileRaster {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TileRaster")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("tiles", &self.tiles)
            .finish()
    }
}

impl PartialEq for TileRaster {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.format == other.format
            && self.tiles == other.tiles
    }
}

impl Eq for TileRaster {}

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
            tiles: Arc::new(BTreeMap::new()),
            checksum_cache: Arc::new(OnceLock::new()),
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

    /// Returns logical bytes retained by allocated tile payloads.
    ///
    /// Cloned rasters share these payloads copy-on-write, so summing this value
    /// across clones intentionally reports logical retention rather than unique
    /// process allocation.
    #[must_use]
    pub fn allocated_tile_bytes(&self) -> u64 {
        self.tiles.values().fold(0_u64, |bytes, tile| {
            bytes.saturating_add(tile.bytes.len() as u64)
        })
    }

    pub fn allocated_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles.keys().copied()
    }

    #[must_use]
    pub fn tile_revision(&self, coord: TileCoord) -> u64 {
        self.tiles.get(&coord).map_or(0, |tile| tile.revision)
    }

    /// Borrows one allocated tile without copying or compacting its pixels.
    ///
    /// The returned bytes contain the full [`TILE_SIZE`] by [`TILE_SIZE`]
    /// backing store. Use the view's logical dimensions and row stride when
    /// reading an edge tile. Returns `None` for an unallocated or out-of-range
    /// coordinate. This read-only query does not change tile revisions.
    #[must_use]
    pub fn tile_view(&self, coord: TileCoord) -> Option<TileView<'_>> {
        let tile = self.tiles.get(&coord)?;
        let (width, height) = self.tile_dimensions(coord)?;
        let bytes_per_pixel = u32::try_from(self.format.bytes_per_pixel()).ok()?;
        let row_stride_bytes = TILE_SIZE.checked_mul(bytes_per_pixel)?;
        Some(TileView {
            coord,
            width,
            height,
            row_stride_bytes,
            bytes: &tile.bytes,
            revision: tile.revision,
        })
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
        self.invalidate_checksum();
        let tile = Arc::make_mut(&mut self.tiles)
            .entry(coord)
            .or_insert_with(|| {
                Arc::new(Tile {
                    bytes: vec![0; tile_bytes],
                    revision,
                })
            });
        let tile = Arc::make_mut(tile);
        let offset = ((local_y as usize * TILE_SIZE as usize) + local_x as usize) * bytes_per_pixel;
        match value {
            PixelValue::Binary(value) => tile.bytes[offset] = value,
            PixelValue::Grayscale8(value) => tile.bytes[offset] = value,
            PixelValue::Grayscale16(value) => {
                tile.bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
            }
            PixelValue::Rgba(value) => tile.bytes[offset..offset + 4].copy_from_slice(&value),
            PixelValue::Rgba16(value) => {
                for (index, channel) in value.into_iter().enumerate() {
                    let start = offset + index * 2;
                    tile.bytes[start..start + 2].copy_from_slice(&channel.to_le_bytes());
                }
            }
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
            self.invalidate_checksum();
            Arc::make_mut(&mut self.tiles).remove(&coord);
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
            // Tile revisions are deliberately excluded from the exact checksum.
            // Replacing identical pixels may update that revision without
            // invalidating the immutable raster's cached pixel checksum.
            if self
                .tiles
                .get(&data.coord)
                .is_none_or(|tile| tile.bytes != bytes)
            {
                self.invalidate_checksum();
            }
            Arc::make_mut(&mut self.tiles).insert(
                data.coord,
                Arc::new(Tile {
                    bytes,
                    revision: data.revision,
                }),
            );
        }
        Ok(())
    }

    /// Returns the exact FNV checksum of dimensions, format, and allocated tiles.
    ///
    /// The tile-coordinate order and complete backing bytes, including edge
    /// padding and retained all-zero tiles, are significant; tile revisions are
    /// not. The result is computed once for an immutable raster and shared with
    /// its COW clones, even when they were cloned before the first query. Pixel
    /// or allocation changes invalidate only the changed raster's cache. Querying
    /// does not change pixels, revisions, equality, or serialized tile data.
    #[must_use]
    pub fn checksum(&self) -> u64 {
        *self.checksum_cache.get_or_init(|| {
            #[cfg(test)]
            checksum_cache_tests::record_computation();
            let mut checksum = FNV_OFFSET;
            checksum = fnv_bytes(checksum, &self.width.to_le_bytes());
            checksum = fnv_bytes(checksum, &self.height.to_le_bytes());
            checksum = fnv_bytes(checksum, &[self.format as u8]);
            for (coord, tile) in self.tiles.iter() {
                checksum = fnv_bytes(checksum, &coord.x.to_le_bytes());
                checksum = fnv_bytes(checksum, &coord.y.to_le_bytes());
                #[cfg(test)]
                checksum_cache_tests::record_payload_bytes(tile.bytes.len() as u64);
                checksum = fnv_bytes(checksum, &tile.bytes);
            }
            checksum
        })
    }

    fn invalidate_checksum(&mut self) {
        if let Some(cache) = Arc::get_mut(&mut self.checksum_cache) {
            cache.take();
        } else {
            self.checksum_cache = Arc::new(OnceLock::new());
        }
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
            | (PixelFormat::Grayscale8, PixelValue::Grayscale8(_))
            | (PixelFormat::Grayscale16, PixelValue::Grayscale16(_))
            | (PixelFormat::StraightRgba16, PixelValue::Rgba16(_))
            | (PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8, PixelValue::Rgba(_)) => {
                Ok(())
            }
            _ => Err(RasterError::PixelFormatMismatch),
        }
    }

    const fn zero_pixel(&self) -> PixelValue {
        match self.format {
            PixelFormat::BinaryMask8 => PixelValue::Binary(0),
            PixelFormat::Grayscale8 => PixelValue::Grayscale8(0),
            PixelFormat::Grayscale16 => PixelValue::Grayscale16(0),
            PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8 => {
                PixelValue::Rgba([0; 4])
            }
            PixelFormat::StraightRgba16 => PixelValue::Rgba16([0; 4]),
        }
    }

    fn read_tile_pixel(&self, tile: &Tile, local_x: u32, local_y: u32) -> PixelValue {
        let bytes_per_pixel = self.format.bytes_per_pixel();
        let offset = ((local_y as usize * TILE_SIZE as usize) + local_x as usize) * bytes_per_pixel;
        match self.format {
            PixelFormat::BinaryMask8 => PixelValue::Binary(tile.bytes[offset]),
            PixelFormat::Grayscale8 => PixelValue::Grayscale8(tile.bytes[offset]),
            PixelFormat::Grayscale16 => PixelValue::Grayscale16(u16::from_le_bytes([
                tile.bytes[offset],
                tile.bytes[offset + 1],
            ])),
            PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8 => PixelValue::Rgba([
                tile.bytes[offset],
                tile.bytes[offset + 1],
                tile.bytes[offset + 2],
                tile.bytes[offset + 3],
            ]),
            PixelFormat::StraightRgba16 => {
                let channel = |index: usize| {
                    let start = offset + index * 2;
                    u16::from_le_bytes([tile.bytes[start], tile.bytes[start + 1]])
                };
                PixelValue::Rgba16([channel(0), channel(1), channel(2), channel(3)])
            }
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
mod checksum_cache_tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Barrier;

    thread_local! {
        static CHECKSUM_WORK: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
    }

    pub(super) fn record_computation() {
        CHECKSUM_WORK.with(|work| {
            let (computations, bytes) = work.get();
            work.set((computations + 1, bytes));
        });
    }

    pub(super) fn record_payload_bytes(additional: u64) {
        CHECKSUM_WORK.with(|work| {
            let (computations, bytes) = work.get();
            work.set((computations, bytes + additional));
        });
    }

    fn reset_work() {
        CHECKSUM_WORK.with(|work| work.set((0, 0)));
    }

    fn work() -> (u64, u64) {
        CHECKSUM_WORK.with(Cell::get)
    }

    fn raster() -> TileRaster {
        let mut raster = TileRaster::new(65, 66, PixelFormat::StraightRgba8).unwrap();
        raster
            .set_pixel(64, 2, PixelValue::Rgba([1, 2, 3, 255]), 1)
            .unwrap();
        raster
            .set_pixel(1, 65, PixelValue::Rgba([4, 5, 6, 255]), 1)
            .unwrap();
        raster
    }

    #[test]
    fn immutable_clones_share_checksum_work_before_and_after_initialization() {
        let raster = raster();
        let cold_clone = raster.clone();
        reset_work();
        let checksum = cold_clone.checksum();
        let expected_work = (1, raster.allocated_tile_bytes());
        assert_eq!(work(), expected_work);
        for _ in 0..8 {
            assert_eq!(raster.checksum(), checksum);
            assert_eq!(raster.clone().checksum(), checksum);
        }
        assert_eq!(work(), expected_work);
    }

    #[test]
    fn modifying_a_cold_clone_separates_its_future_checksum() {
        let original = raster();
        let mut changed = original.clone();
        changed
            .set_pixel(64, 2, PixelValue::Rgba([7, 8, 9, 255]), 2)
            .unwrap();
        reset_work();
        let changed_checksum = changed.checksum();
        let original_checksum = original.checksum();
        assert_ne!(changed_checksum, original_checksum);
        let expected_work = (2, original.allocated_tile_bytes() * 2);
        assert_eq!(work(), expected_work);
        assert_eq!(changed.clone().checksum(), changed_checksum);
        assert_eq!(original.clone().checksum(), original_checksum);
        assert_eq!(work(), expected_work);
    }

    #[test]
    fn unchanged_or_invalid_writes_keep_checksum_warm_and_cow_edits_detach() {
        let original = raster();
        reset_work();
        let checksum = original.checksum();
        let original_work = work();
        let mut changed = original.clone();
        let value = changed.pixel(64, 2).unwrap();
        assert_eq!(changed.set_pixel(64, 2, value, 2), Ok(value));
        assert_eq!(
            changed.set_pixel(65, 2, value, 2),
            Err(RasterError::PixelOutOfBounds)
        );
        assert_eq!(
            changed.set_pixel(64, 2, PixelValue::Binary(255), 2),
            Err(RasterError::PixelFormatMismatch)
        );
        changed.remove_tile_if_empty(TileCoord { x: 0, y: 0 });
        changed.remove_tile_if_empty(TileCoord { x: 1, y: 0 });
        let mut unchanged_pixels = changed.tile_data(TileCoord { x: 1, y: 0 }).unwrap();
        unchanged_pixels.revision = 2;
        changed.insert_tile(unchanged_pixels.clone()).unwrap();
        unchanged_pixels.bytes.fill(0);
        changed.insert_tile(unchanged_pixels.clone()).unwrap();
        unchanged_pixels.bytes.pop();
        assert_eq!(
            changed.insert_tile(unchanged_pixels),
            Err(RasterError::InvalidTile)
        );
        assert_eq!(changed.checksum(), checksum);
        assert_eq!(work(), original_work);
        assert_eq!(changed.tile_revision(TileCoord { x: 1, y: 0 }), 2);
        assert_eq!(original.tile_revision(TileCoord { x: 1, y: 0 }), 1);

        changed
            .set_pixel(64, 2, PixelValue::Rgba([7, 8, 9, 255]), 3)
            .unwrap();
        assert_eq!(original.checksum(), checksum);
        assert_eq!(work(), original_work);
        assert_ne!(changed.checksum(), checksum);
        assert_eq!(work(), (2, original.allocated_tile_bytes() * 2));
    }

    #[test]
    fn concurrent_cold_clones_initialize_the_shared_checksum_once() {
        let original = raster();
        let barrier = Barrier::new(4);
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let raster = original.clone();
                    let barrier = &barrier;
                    scope.spawn(move || {
                        reset_work();
                        barrier.wait();
                        (raster.checksum(), work())
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        reset_work();
        let checksum = original.checksum();
        assert_eq!(work(), (0, 0));
        assert!(results.iter().all(|(value, _)| *value == checksum));
        let total = results
            .into_iter()
            .fold((0, 0), |(count, bytes), (_, work)| {
                (count + work.0, bytes + work.1)
            });
        assert_eq!(total, (1, original.allocated_tile_bytes()));
    }

    #[test]
    fn raster_clone_shares_map_and_first_write_detaches_only_touched_tile() {
        let original = raster();
        let left = TileCoord { x: 1, y: 0 };
        let right = TileCoord { x: 0, y: 1 };
        let mut changed = original.clone();

        assert!(Arc::ptr_eq(&original.tiles, &changed.tiles));
        assert!(Arc::ptr_eq(
            original.tiles.get(&left).unwrap(),
            changed.tiles.get(&left).unwrap()
        ));
        assert!(Arc::ptr_eq(
            original.tiles.get(&right).unwrap(),
            changed.tiles.get(&right).unwrap()
        ));

        let unchanged = changed.pixel(64, 2).unwrap();
        changed.set_pixel(64, 2, unchanged, 2).unwrap();
        assert!(Arc::ptr_eq(&original.tiles, &changed.tiles));

        changed
            .set_pixel(64, 2, PixelValue::Rgba([7, 8, 9, 255]), 2)
            .unwrap();
        assert!(!Arc::ptr_eq(&original.tiles, &changed.tiles));
        assert!(!Arc::ptr_eq(
            original.tiles.get(&left).unwrap(),
            changed.tiles.get(&left).unwrap()
        ));
        assert!(Arc::ptr_eq(
            original.tiles.get(&right).unwrap(),
            changed.tiles.get(&right).unwrap()
        ));
        assert_eq!(original.tile_revision(left), 1);
        assert_eq!(changed.tile_revision(left), 2);
        assert_eq!(original.checksum(), raster().checksum());
    }

    #[test]
    fn map_level_cow_covers_insert_and_remove_without_detaching_no_ops() {
        let original = raster();
        let mut changed = original.clone();
        changed.remove_tile_if_empty(TileCoord { x: 0, y: 0 });
        assert!(Arc::ptr_eq(&original.tiles, &changed.tiles));

        let mut inserted = changed.tile_data(TileCoord { x: 1, y: 0 }).unwrap();
        inserted.revision = 9;
        changed.insert_tile(inserted).unwrap();
        assert!(!Arc::ptr_eq(&original.tiles, &changed.tiles));
        assert_eq!(original.tile_revision(TileCoord { x: 1, y: 0 }), 1);
        assert_eq!(changed.tile_revision(TileCoord { x: 1, y: 0 }), 9);

        let mut removable = TileRaster::new(1, 1, PixelFormat::StraightRgba8).unwrap();
        removable.tiles = Arc::new(BTreeMap::from([(
            TileCoord { x: 0, y: 0 },
            Arc::new(Tile {
                bytes: vec![0; TILE_SIZE as usize * TILE_SIZE as usize * 4],
                revision: 3,
            }),
        )]));
        let retained = removable.clone();
        removable.remove_tile_if_empty(TileCoord { x: 0, y: 0 });
        assert_eq!(removable.allocated_tile_count(), 0);
        assert_eq!(retained.allocated_tile_count(), 1);
        assert!(!Arc::ptr_eq(&removable.tiles, &retained.tiles));
    }
}
