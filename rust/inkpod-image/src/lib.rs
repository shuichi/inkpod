#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;

pub const TILE_SIZE: u32 = 64;
pub const MAX_RASTER_DIMENSION: u32 = 1_048_576;
pub const MAX_FILL_PIXELS: u64 = 16_777_216;
pub const MAX_GAP_CLOSE: u8 = 64;
pub const MAX_INCLUSION_COLORS: usize = 6;
pub const MAX_PALETTE_COLORS: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    BinaryMask8,
    Grayscale8,
    Grayscale16,
    StraightRgba8,
    StraightRgba16,
    PremultipliedBgra8,
}

impl PixelFormat {
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::BinaryMask8 | Self::Grayscale8 => 1,
            Self::Grayscale16 => 2,
            Self::StraightRgba8 | Self::PremultipliedBgra8 => 4,
            Self::StraightRgba16 => 8,
        }
    }

    #[must_use]
    pub const fn is_color(self) -> bool {
        matches!(
            self,
            Self::StraightRgba8 | Self::StraightRgba16 | Self::PremultipliedBgra8
        )
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
    Grayscale8(u8),
    Grayscale16(u16),
    Rgba([u8; 4]),
    Rgba16([u16; 4]),
}

impl PixelValue {
    #[must_use]
    pub const fn is_zero(self) -> bool {
        match self {
            Self::Binary(value) => value == 0,
            Self::Grayscale8(value) => value == 0,
            Self::Grayscale16(value) => value == 0,
            Self::Rgba(value) => value[0] == 0 && value[1] == 0 && value[2] == 0 && value[3] == 0,
            Self::Rgba16(value) => value[0] == 0 && value[1] == 0 && value[2] == 0 && value[3] == 0,
        }
    }

    #[must_use]
    pub const fn is_transparent(self) -> bool {
        match self {
            Self::Rgba(value) => value[3] == 0,
            Self::Rgba16(value) => value[3] == 0,
            Self::Binary(value) | Self::Grayscale8(value) => value == 0,
            Self::Grayscale16(value) => value == 0,
        }
    }

    #[must_use]
    pub const fn is_exact_white(self) -> bool {
        match self {
            Self::Rgba(value) => value[0] == u8::MAX && value[1] == u8::MAX && value[2] == u8::MAX,
            Self::Rgba16(value) => {
                value[0] == u16::MAX && value[1] == u16::MAX && value[2] == u16::MAX
            }
            _ => false,
        }
    }

    #[must_use]
    pub const fn rgba16(self) -> Option<[u16; 4]> {
        match self {
            Self::Rgba(value) => Some([
                value[0] as u16 * 257,
                value[1] as u16 * 257,
                value[2] as u16 * 257,
                value[3] as u16 * 257,
            ]),
            Self::Rgba16(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InclusionMode {
    None,
    Specified,
    ExceptSpecified,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FillOptions {
    /// Maximum per-channel difference in normalized 16-bit sRGB values.
    pub tolerance: u16,
    pub detached_regions: bool,
    pub overflow_abort: bool,
    pub gap_close: u8,
    pub transparent_only: bool,
    pub inclusion_mode: InclusionMode,
    pub inclusion_colors: Vec<PixelValue>,
}

impl Default for FillOptions {
    fn default() -> Self {
        Self {
            tolerance: 0,
            detached_regions: false,
            overflow_abort: false,
            gap_close: 0,
            transparent_only: false,
            inclusion_mode: InclusionMode::None,
            inclusion_colors: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelEdit {
    pub x: u32,
    pub y: u32,
    pub before: PixelValue,
    pub after: PixelValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FillPlan {
    pub edits: Vec<PixelEdit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FillError {
    Raster(RasterError),
    InvalidArgument(&'static str),
    Cancelled,
    WorkLimit,
    Overflow { x: u32, y: u32 },
}

impl fmt::Display for FillError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raster(error) => write!(formatter, "raster error: {error}"),
            Self::InvalidArgument(message) => write!(formatter, "invalid fill argument: {message}"),
            Self::Cancelled => formatter.write_str("fill was cancelled before commit"),
            Self::WorkLimit => formatter.write_str("fill exceeds the bounded work limit"),
            Self::Overflow { x, y } => write!(formatter, "fill reached image edge at ({x}, {y})"),
        }
    }
}

impl std::error::Error for FillError {}

impl From<RasterError> for FillError {
    fn from(error: RasterError) -> Self {
        Self::Raster(error)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlaneSample<'a> {
    pub raster: &'a TileRaster,
    /// Binary/grayscale line planes return this exact base color to the
    /// eyedropper while display composition applies the stored coverage.
    pub base_color: Option<PixelValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EyedropperSource {
    TopmostNonTransparent,
    SelectedPlane,
    Composite,
    LightTableTopmost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorCheckMode {
    LegacyWhiteTransparency,
    NativeAlpha,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorCheckCategory {
    Colored,
    ExactWhite,
    Transparent,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Palette {
    colors: Vec<PixelValue>,
}

impl Palette {
    #[must_use]
    pub fn colors(&self) -> &[PixelValue] {
        &self.colors
    }

    pub fn push(&mut self, color: PixelValue) -> Result<(), RasterError> {
        if color.rgba16().is_none() {
            return Err(RasterError::PixelFormatMismatch);
        }
        if self.colors.len() >= MAX_PALETTE_COLORS {
            return Err(RasterError::InvalidTile);
        }
        self.colors.push(color);
        Ok(())
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

pub fn seed_fill(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    seed: (u32, u32),
    fill_color: PixelValue,
    options: &FillOptions,
) -> Result<FillPlan, FillError> {
    seed_fill_with_cancel(
        main_line,
        color_plane,
        selection,
        seed,
        fill_color,
        options,
        || false,
    )
}

pub fn seed_fill_with_cancel(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    seed: (u32, u32),
    fill_color: PixelValue,
    options: &FillOptions,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<FillPlan, FillError> {
    let pixel_count = validate_fill_inputs(main_line, color_plane, selection, fill_color, options)?;
    if seed.0 >= color_plane.width() || seed.1 >= color_plane.height() {
        return Err(FillError::InvalidArgument(
            "seed is outside the color plane",
        ));
    }
    let seed_value = color_plane.pixel(seed.0, seed.1)?;
    let mut visited = vec![false; pixel_count];
    let mut edits = Vec::new();
    let mut work = 0_u64;

    let starts: Box<dyn Iterator<Item = (u32, u32)>> = if options.detached_regions {
        Box::new(
            (0..color_plane.height()).flat_map(|y| (0..color_plane.width()).map(move |x| (x, y))),
        )
    } else {
        Box::new(std::iter::once(seed))
    };

    for start in starts {
        let start_index = pixel_index(color_plane.width(), start.0, start.1);
        if visited[start_index]
            || !candidate_pixel(
                main_line,
                color_plane,
                selection,
                start.0,
                start.1,
                seed_value,
                options,
            )?
            || virtual_gap_boundary(
                main_line,
                color_plane,
                selection,
                start.0,
                start.1,
                seed_value,
                options,
            )?
        {
            visited[start_index] = true;
            continue;
        }

        let mut queue = VecDeque::from([start]);
        visited[start_index] = true;
        while let Some((x, y)) = queue.pop_front() {
            work = work.checked_add(1).ok_or(FillError::WorkLimit)?;
            if work > MAX_FILL_PIXELS {
                return Err(FillError::WorkLimit);
            }
            if work % 1_024 == 0 && is_cancelled() {
                return Err(FillError::Cancelled);
            }
            if options.overflow_abort
                && (x == 0
                    || y == 0
                    || x + 1 == color_plane.width()
                    || y + 1 == color_plane.height())
            {
                return Err(FillError::Overflow { x, y });
            }

            let before = color_plane.pixel(x, y)?;
            if before != fill_color {
                edits.push(PixelEdit {
                    x,
                    y,
                    before,
                    after: fill_color,
                });
            }
            for (next_x, next_y) in neighbors(color_plane.width(), color_plane.height(), x, y) {
                let next_index = pixel_index(color_plane.width(), next_x, next_y);
                if visited[next_index] {
                    continue;
                }
                visited[next_index] = true;
                if candidate_pixel(
                    main_line,
                    color_plane,
                    selection,
                    next_x,
                    next_y,
                    seed_value,
                    options,
                )? && !virtual_gap_boundary(
                    main_line,
                    color_plane,
                    selection,
                    next_x,
                    next_y,
                    seed_value,
                    options,
                )? {
                    queue.push_back((next_x, next_y));
                }
            }
        }
    }
    if is_cancelled() {
        return Err(FillError::Cancelled);
    }
    edits.sort_by_key(|edit| (edit.y, edit.x));
    Ok(FillPlan { edits })
}

/// Fills every candidate component wholly enclosed by boundary pixels inside
/// `operation_mask`. Components touching the image edge or escaping the mask
/// through a non-boundary pixel are intentionally left unchanged.
pub fn closed_region_fill(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    operation_mask: &TileRaster,
    fill_color: PixelValue,
    options: &FillOptions,
) -> Result<FillPlan, FillError> {
    closed_region_fill_with_cancel(
        main_line,
        color_plane,
        operation_mask,
        fill_color,
        options,
        || false,
    )
}

pub fn closed_region_fill_with_cancel(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    operation_mask: &TileRaster,
    fill_color: PixelValue,
    options: &FillOptions,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<FillPlan, FillError> {
    let pixel_count = validate_fill_inputs(
        main_line,
        color_plane,
        Some(operation_mask),
        fill_color,
        options,
    )?;
    let mut visited = vec![false; pixel_count];
    let mut edits = Vec::new();
    let mut work = 0_u64;
    for y in 0..color_plane.height() {
        for x in 0..color_plane.width() {
            let index = pixel_index(color_plane.width(), x, y);
            if visited[index] || !selection_contains(operation_mask, x, y)? {
                visited[index] = true;
                continue;
            }
            let target = color_plane.pixel(x, y)?;
            if !candidate_pixel(
                main_line,
                color_plane,
                Some(operation_mask),
                x,
                y,
                target,
                options,
            )? || virtual_gap_boundary(
                main_line,
                color_plane,
                Some(operation_mask),
                x,
                y,
                target,
                options,
            )? {
                visited[index] = true;
                continue;
            }

            let mut component = Vec::new();
            let mut queue = VecDeque::from([(x, y)]);
            let mut escaped = false;
            visited[index] = true;
            while let Some((current_x, current_y)) = queue.pop_front() {
                work = work.checked_add(1).ok_or(FillError::WorkLimit)?;
                if work > MAX_FILL_PIXELS {
                    return Err(FillError::WorkLimit);
                }
                if work % 1_024 == 0 && is_cancelled() {
                    return Err(FillError::Cancelled);
                }
                component.push((current_x, current_y));
                if current_x == 0
                    || current_y == 0
                    || current_x + 1 == color_plane.width()
                    || current_y + 1 == color_plane.height()
                {
                    escaped = true;
                }
                for (next_x, next_y) in neighbors(
                    color_plane.width(),
                    color_plane.height(),
                    current_x,
                    current_y,
                ) {
                    if !selection_contains(operation_mask, next_x, next_y)? {
                        if !hard_boundary(
                            main_line,
                            color_plane,
                            None,
                            next_x,
                            next_y,
                            target,
                            options,
                        )? {
                            escaped = true;
                        }
                        continue;
                    }
                    let next_index = pixel_index(color_plane.width(), next_x, next_y);
                    if visited[next_index] {
                        continue;
                    }
                    if candidate_pixel(
                        main_line,
                        color_plane,
                        Some(operation_mask),
                        next_x,
                        next_y,
                        target,
                        options,
                    )? && !virtual_gap_boundary(
                        main_line,
                        color_plane,
                        Some(operation_mask),
                        next_x,
                        next_y,
                        target,
                        options,
                    )? {
                        visited[next_index] = true;
                        queue.push_back((next_x, next_y));
                    }
                }
            }
            if !escaped {
                for (edit_x, edit_y) in component {
                    let before = color_plane.pixel(edit_x, edit_y)?;
                    if before != fill_color {
                        edits.push(PixelEdit {
                            x: edit_x,
                            y: edit_y,
                            before,
                            after: fill_color,
                        });
                    }
                }
            }
        }
    }
    if is_cancelled() {
        return Err(FillError::Cancelled);
    }
    edits.sort_by_key(|edit| (edit.y, edit.x));
    Ok(FillPlan { edits })
}

/// Extends the seed color through transparent pixels inside `operation_mask`
/// for at most `maximum_distance` four-connected steps.
pub fn extend_fill(
    color_plane: &TileRaster,
    operation_mask: &TileRaster,
    seed: (u32, u32),
    maximum_distance: u32,
) -> Result<FillPlan, FillError> {
    extend_fill_with_cancel(color_plane, operation_mask, seed, maximum_distance, || {
        false
    })
}

pub fn extend_fill_with_cancel(
    color_plane: &TileRaster,
    operation_mask: &TileRaster,
    seed: (u32, u32),
    maximum_distance: u32,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<FillPlan, FillError> {
    validate_selection(color_plane, operation_mask)?;
    if seed.0 >= color_plane.width() || seed.1 >= color_plane.height() {
        return Err(FillError::InvalidArgument(
            "fill-extension seed is outside the plane",
        ));
    }
    let source = color_plane.pixel(seed.0, seed.1)?;
    if source.rgba16().is_none() || source.is_transparent() {
        return Err(FillError::InvalidArgument(
            "fill-extension seed must contain an opaque color",
        ));
    }
    let pixel_count = bounded_pixel_count(color_plane.width(), color_plane.height())?;
    let mut distance = vec![u32::MAX; pixel_count];
    let mut queue = VecDeque::from([seed]);
    distance[pixel_index(color_plane.width(), seed.0, seed.1)] = 0;
    let mut edits = Vec::new();
    let mut work = 0_u64;
    while let Some((x, y)) = queue.pop_front() {
        work = work.checked_add(1).ok_or(FillError::WorkLimit)?;
        if work > MAX_FILL_PIXELS {
            return Err(FillError::WorkLimit);
        }
        if work % 1_024 == 0 && is_cancelled() {
            return Err(FillError::Cancelled);
        }
        let current_distance = distance[pixel_index(color_plane.width(), x, y)];
        if current_distance >= maximum_distance {
            continue;
        }
        for (next_x, next_y) in neighbors(color_plane.width(), color_plane.height(), x, y) {
            let index = pixel_index(color_plane.width(), next_x, next_y);
            if distance[index] != u32::MAX || !selection_contains(operation_mask, next_x, next_y)? {
                continue;
            }
            let before = color_plane.pixel(next_x, next_y)?;
            if !before.is_transparent() {
                continue;
            }
            distance[index] = current_distance + 1;
            edits.push(PixelEdit {
                x: next_x,
                y: next_y,
                before,
                after: source,
            });
            queue.push_back((next_x, next_y));
        }
    }
    if is_cancelled() {
        return Err(FillError::Cancelled);
    }
    edits.sort_by_key(|edit| (edit.y, edit.x));
    Ok(FillPlan { edits })
}

pub fn eyedropper(
    source: EyedropperSource,
    x: u32,
    y: u32,
    selected: PlaneSample<'_>,
    top_to_bottom: &[PlaneSample<'_>],
    light_table_top_to_bottom: &[PlaneSample<'_>],
) -> Result<Option<PixelValue>, RasterError> {
    match source {
        EyedropperSource::SelectedPlane => sample_plane_exact(selected, x, y),
        EyedropperSource::TopmostNonTransparent => first_visible_sample(top_to_bottom, x, y),
        EyedropperSource::LightTableTopmost => {
            first_visible_sample(light_table_top_to_bottom, x, y)
        }
        EyedropperSource::Composite => {
            let mut composite = None;
            for plane in top_to_bottom.iter().rev() {
                if let Some(sample) = sample_plane_display(*plane, x, y)? {
                    composite = Some(match composite {
                        Some(background) => blend_over(background, sample),
                        None => sample,
                    });
                }
            }
            Ok(composite)
        }
    }
}

#[must_use]
pub const fn color_check_category(value: PixelValue, mode: ColorCheckMode) -> ColorCheckCategory {
    if matches!(mode, ColorCheckMode::NativeAlpha) && value.is_transparent() {
        ColorCheckCategory::Transparent
    } else if value.is_exact_white() {
        ColorCheckCategory::ExactWhite
    } else {
        ColorCheckCategory::Colored
    }
}

fn validate_fill_inputs(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    fill_color: PixelValue,
    options: &FillOptions,
) -> Result<usize, FillError> {
    if main_line.width() != color_plane.width() || main_line.height() != color_plane.height() {
        return Err(FillError::InvalidArgument(
            "main-line and color-plane dimensions differ",
        ));
    }
    if !matches!(
        main_line.format(),
        PixelFormat::BinaryMask8 | PixelFormat::Grayscale8 | PixelFormat::Grayscale16
    ) || !matches!(
        color_plane.format(),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
    ) {
        return Err(FillError::InvalidArgument(
            "plane formats do not support coloring fill",
        ));
    }
    validate_color_for_format(color_plane.format(), fill_color)?;
    if options.gap_close > MAX_GAP_CLOSE {
        return Err(FillError::InvalidArgument(
            "gap-close value exceeds its bounded limit",
        ));
    }
    if options.inclusion_colors.len() > MAX_INCLUSION_COLORS {
        return Err(FillError::InvalidArgument("too many inclusion colors"));
    }
    if options
        .inclusion_colors
        .iter()
        .any(|color| color.rgba16().is_none())
    {
        return Err(FillError::InvalidArgument("inclusion color is not RGBA"));
    }
    if let Some(mask) = selection {
        validate_selection(color_plane, mask)?;
    }
    bounded_pixel_count(color_plane.width(), color_plane.height())
}

fn validate_selection(color_plane: &TileRaster, selection: &TileRaster) -> Result<(), FillError> {
    if selection.width() != color_plane.width()
        || selection.height() != color_plane.height()
        || selection.format() != PixelFormat::BinaryMask8
    {
        return Err(FillError::InvalidArgument(
            "selection must be a same-sized binary mask",
        ));
    }
    Ok(())
}

fn bounded_pixel_count(width: u32, height: u32) -> Result<usize, FillError> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(FillError::WorkLimit)?;
    if pixels > MAX_FILL_PIXELS {
        return Err(FillError::WorkLimit);
    }
    usize::try_from(pixels).map_err(|_| FillError::WorkLimit)
}

fn validate_color_for_format(format: PixelFormat, color: PixelValue) -> Result<(), FillError> {
    if matches!(
        (format, color),
        (PixelFormat::StraightRgba8, PixelValue::Rgba(_))
            | (PixelFormat::StraightRgba16, PixelValue::Rgba16(_))
    ) {
        Ok(())
    } else {
        Err(FillError::InvalidArgument(
            "fill color depth does not match the color plane",
        ))
    }
}

fn pixel_index(width: u32, x: u32, y: u32) -> usize {
    y as usize * width as usize + x as usize
}

fn neighbors(width: u32, height: u32, x: u32, y: u32) -> impl Iterator<Item = (u32, u32)> {
    let mut values = [(0, 0); 4];
    let mut count = 0;
    if x > 0 {
        values[count] = (x - 1, y);
        count += 1;
    }
    if x + 1 < width {
        values[count] = (x + 1, y);
        count += 1;
    }
    if y > 0 {
        values[count] = (x, y - 1);
        count += 1;
    }
    if y + 1 < height {
        values[count] = (x, y + 1);
        count += 1;
    }
    values.into_iter().take(count)
}

fn selection_contains(selection: &TileRaster, x: u32, y: u32) -> Result<bool, FillError> {
    Ok(matches!(selection.pixel(x, y)?, PixelValue::Binary(255)))
}

fn candidate_pixel(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    x: u32,
    y: u32,
    target: PixelValue,
    options: &FillOptions,
) -> Result<bool, FillError> {
    Ok(!hard_boundary(
        main_line,
        color_plane,
        selection,
        x,
        y,
        target,
        options,
    )?)
}

fn hard_boundary(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    x: u32,
    y: u32,
    target: PixelValue,
    options: &FillOptions,
) -> Result<bool, FillError> {
    if let Some(mask) = selection
        && !selection_contains(mask, x, y)?
    {
        return Ok(true);
    }
    // Binary main lines participate in legacy topology. Grayscale coverage is
    // display-only; a broken color-plane trace must still leak by specification.
    if matches!(main_line.pixel(x, y)?, PixelValue::Binary(255)) {
        return Ok(true);
    }
    let value = color_plane.pixel(x, y)?;
    let included = inclusion_matches(value, options);
    if options.transparent_only && !value.is_transparent() && !included {
        return Ok(true);
    }
    Ok(!within_tolerance(value, target, options.tolerance) && !included)
}

fn inclusion_matches(value: PixelValue, options: &FillOptions) -> bool {
    if value.is_transparent() {
        return false;
    }
    let listed = options
        .inclusion_colors
        .iter()
        .any(|color| within_tolerance(value, *color, options.tolerance));
    match options.inclusion_mode {
        InclusionMode::None => false,
        InclusionMode::Specified => listed,
        InclusionMode::ExceptSpecified => !listed,
    }
}

fn within_tolerance(left: PixelValue, right: PixelValue, tolerance: u16) -> bool {
    let (Some(left), Some(right)) = (left.rgba16(), right.rgba16()) else {
        return left == right;
    };
    left.into_iter()
        .zip(right)
        .all(|(left, right)| left.abs_diff(right) <= tolerance)
}

fn virtual_gap_boundary(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    x: u32,
    y: u32,
    target: PixelValue,
    options: &FillOptions,
) -> Result<bool, FillError> {
    if options.gap_close == 0
        || hard_boundary(main_line, color_plane, selection, x, y, target, options)?
    {
        return Ok(false);
    }
    let maximum = u32::from(options.gap_close) + 1;
    for ((negative_x, negative_y), (positive_x, positive_y)) in
        [((-1_i32, 0_i32), (1_i32, 0_i32)), ((0, -1), (0, 1))]
    {
        let negative = nearest_boundary_distance(
            main_line,
            color_plane,
            selection,
            x,
            y,
            negative_x,
            negative_y,
            maximum,
            target,
            options,
        )?;
        let positive = nearest_boundary_distance(
            main_line,
            color_plane,
            selection,
            x,
            y,
            positive_x,
            positive_y,
            maximum,
            target,
            options,
        )?;
        if let (Some(negative), Some(positive)) = (negative, positive)
            && negative + positive - 1 <= u32::from(options.gap_close)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn nearest_boundary_distance(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    x: u32,
    y: u32,
    delta_x: i32,
    delta_y: i32,
    maximum: u32,
    target: PixelValue,
    options: &FillOptions,
) -> Result<Option<u32>, FillError> {
    for distance in 1..=maximum {
        let next_x = i64::from(x) + i64::from(delta_x) * i64::from(distance);
        let next_y = i64::from(y) + i64::from(delta_y) * i64::from(distance);
        if next_x < 0
            || next_y < 0
            || next_x >= i64::from(color_plane.width())
            || next_y >= i64::from(color_plane.height())
        {
            return Ok(None);
        }
        if hard_boundary(
            main_line,
            color_plane,
            selection,
            next_x as u32,
            next_y as u32,
            target,
            options,
        )? {
            return Ok(Some(distance));
        }
    }
    Ok(None)
}

fn first_visible_sample(
    planes: &[PlaneSample<'_>],
    x: u32,
    y: u32,
) -> Result<Option<PixelValue>, RasterError> {
    for plane in planes {
        if let Some(value) = sample_plane_exact(*plane, x, y)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn sample_plane_exact(
    plane: PlaneSample<'_>,
    x: u32,
    y: u32,
) -> Result<Option<PixelValue>, RasterError> {
    let value = plane.raster.pixel(x, y)?;
    match value {
        PixelValue::Binary(0) | PixelValue::Grayscale8(0) | PixelValue::Grayscale16(0) => Ok(None),
        PixelValue::Binary(_) | PixelValue::Grayscale8(_) | PixelValue::Grayscale16(_) => plane
            .base_color
            .ok_or(RasterError::PixelFormatMismatch)
            .map(Some),
        PixelValue::Rgba(value) if value[3] == 0 => Ok(None),
        PixelValue::Rgba16(value) if value[3] == 0 => Ok(None),
        PixelValue::Rgba(_) | PixelValue::Rgba16(_) => Ok(Some(value)),
    }
}

fn sample_plane_display(
    plane: PlaneSample<'_>,
    x: u32,
    y: u32,
) -> Result<Option<PixelValue>, RasterError> {
    let value = plane.raster.pixel(x, y)?;
    let coverage = match value {
        PixelValue::Binary(value) | PixelValue::Grayscale8(value) => u16::from(value) * 257,
        PixelValue::Grayscale16(value) => value,
        PixelValue::Rgba(value) => return Ok((value[3] != 0).then_some(PixelValue::Rgba(value))),
        PixelValue::Rgba16(value) => {
            return Ok((value[3] != 0).then_some(PixelValue::Rgba16(value)));
        }
    };
    if coverage == 0 {
        return Ok(None);
    }
    let base = plane.base_color.ok_or(RasterError::PixelFormatMismatch)?;
    Ok(Some(match base {
        PixelValue::Rgba(mut value) => {
            value[3] = ((u32::from(value[3]) * u32::from(coverage) + 32_767) / 65_535) as u8;
            PixelValue::Rgba(value)
        }
        PixelValue::Rgba16(mut value) => {
            value[3] = ((u32::from(value[3]) * u32::from(coverage) + 32_767) / 65_535) as u16;
            PixelValue::Rgba16(value)
        }
        _ => return Err(RasterError::PixelFormatMismatch),
    }))
}

fn blend_over(background: PixelValue, foreground: PixelValue) -> PixelValue {
    let background16 = background.rgba16().expect("validated RGBA background");
    let foreground16 = foreground.rgba16().expect("validated RGBA foreground");
    let foreground_alpha = u64::from(foreground16[3]);
    let inverse = u64::from(u16::MAX) - foreground_alpha;
    let background_alpha = u64::from(background16[3]);
    let output_alpha = foreground_alpha + (background_alpha * inverse + 32_767) / 65_535;
    let mut output = [0_u16; 4];
    output[3] = output_alpha as u16;
    if output_alpha != 0 {
        for channel in 0..3 {
            let foreground_premultiplied = u64::from(foreground16[channel]) * foreground_alpha;
            let background_premultiplied = u64::from(background16[channel]) * background_alpha;
            let numerator =
                foreground_premultiplied + (background_premultiplied * inverse + 32_767) / 65_535;
            output[channel] = (numerator + output_alpha / 2)
                .checked_div(output_alpha)
                .unwrap_or(0) as u16;
        }
    }
    if matches!(background, PixelValue::Rgba(_)) && matches!(foreground, PixelValue::Rgba(_)) {
        PixelValue::Rgba(output.map(|channel| ((u32::from(channel) + 128) / 257) as u8))
    } else {
        PixelValue::Rgba16(output)
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

    fn binary(width: u32, height: u32) -> TileRaster {
        TileRaster::new(width, height, PixelFormat::BinaryMask8).unwrap()
    }

    fn color8(width: u32, height: u32) -> TileRaster {
        TileRaster::new(width, height, PixelFormat::StraightRgba8).unwrap()
    }

    fn select_all(width: u32, height: u32) -> TileRaster {
        let mut selection = binary(width, height);
        for y in 0..height {
            for x in 0..width {
                selection
                    .set_pixel(x, y, PixelValue::Binary(255), 1)
                    .unwrap();
            }
        }
        selection
    }

    fn rectangle_boundary(raster: &mut TileRaster, left: u32, top: u32, right: u32, bottom: u32) {
        for x in left..=right {
            raster
                .set_pixel(x, top, PixelValue::Binary(255), 1)
                .unwrap();
            raster
                .set_pixel(x, bottom, PixelValue::Binary(255), 1)
                .unwrap();
        }
        for y in top..=bottom {
            raster
                .set_pixel(left, y, PixelValue::Binary(255), 1)
                .unwrap();
            raster
                .set_pixel(right, y, PixelValue::Binary(255), 1)
                .unwrap();
        }
    }

    fn apply_plan(raster: &mut TileRaster, plan: &FillPlan) {
        for edit in &plan.edits {
            assert_eq!(raster.pixel(edit.x, edit.y).unwrap(), edit.before);
            raster.set_pixel(edit.x, edit.y, edit.after, 2).unwrap();
        }
    }

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

    #[test]
    fn m2_golden_only_completely_closed_regions_are_filled() {
        let mut main = binary(9, 7);
        rectangle_boundary(&mut main, 1, 1, 4, 5);
        // The second outline is deliberately open at its top.
        for y in 1..=5 {
            main.set_pixel(6, y, PixelValue::Binary(255), 1).unwrap();
            main.set_pixel(8, y, PixelValue::Binary(255), 1).unwrap();
        }
        for x in 6..=8 {
            main.set_pixel(x, 5, PixelValue::Binary(255), 1).unwrap();
        }
        let mut color = color8(9, 7);
        let operation = select_all(9, 7);
        let fill = PixelValue::Rgba([30, 80, 200, 255]);
        let plan =
            closed_region_fill(&main, &color, &operation, fill, &FillOptions::default()).unwrap();
        apply_plan(&mut color, &plan);

        assert_eq!(color.pixel(2, 2).unwrap(), fill);
        assert_eq!(color.pixel(7, 2).unwrap(), PixelValue::Rgba([0; 4]));
        assert_eq!(color.pixel(0, 0).unwrap(), PixelValue::Rgba([0; 4]));
    }

    #[test]
    fn m2_golden_one_pixel_gap_leaks_at_zero_and_closes_at_one() {
        let mut main = binary(7, 7);
        rectangle_boundary(&mut main, 1, 1, 5, 5);
        main.set_pixel(3, 1, PixelValue::Binary(0), 2).unwrap();
        let color = color8(7, 7);
        let fill = PixelValue::Rgba([10, 20, 30, 255]);
        let mut options = FillOptions {
            overflow_abort: true,
            ..FillOptions::default()
        };
        assert!(matches!(
            seed_fill(&main, &color, None, (3, 3), fill, &options),
            Err(FillError::Overflow { .. })
        ));

        options.gap_close = 1;
        let plan = seed_fill(&main, &color, None, (3, 3), fill, &options).unwrap();
        assert!(plan.edits.iter().any(|edit| (edit.x, edit.y) == (3, 3)));
        assert!(plan.edits.iter().all(|edit| edit.y > 1 && edit.y < 5));
    }

    #[test]
    fn m2_golden_overflow_abort_and_cancel_never_mutate_the_source() {
        let main = binary(8, 8);
        let color = color8(8, 8);
        let checksum = color.checksum();
        let options = FillOptions {
            overflow_abort: true,
            ..FillOptions::default()
        };
        assert!(matches!(
            seed_fill(
                &main,
                &color,
                None,
                (4, 4),
                PixelValue::Rgba([1, 2, 3, 255]),
                &options,
            ),
            Err(FillError::Overflow { .. })
        ));
        assert_eq!(color.checksum(), checksum);
        assert_eq!(
            seed_fill_with_cancel(
                &main,
                &color,
                None,
                (4, 4),
                PixelValue::Rgba([1, 2, 3, 255]),
                &FillOptions::default(),
                || true,
            ),
            Err(FillError::Cancelled)
        );
        assert_eq!(color.checksum(), checksum);
    }

    #[test]
    fn m2_golden_inclusion_replaces_target_trace_but_preserves_other_trace() {
        let mut main = binary(9, 7);
        rectangle_boundary(&mut main, 1, 1, 7, 5);
        let mut color = color8(9, 7);
        let included = PixelValue::Rgba([255, 0, 0, 255]);
        let preserved = PixelValue::Rgba([0, 0, 255, 255]);
        for y in 2..5 {
            color.set_pixel(3, y, included, 1).unwrap();
            color.set_pixel(6, y, preserved, 1).unwrap();
        }
        let fill = PixelValue::Rgba([0, 180, 80, 255]);
        let options = FillOptions {
            inclusion_mode: InclusionMode::Specified,
            inclusion_colors: vec![included],
            ..FillOptions::default()
        };
        let plan = seed_fill(&main, &color, None, (2, 3), fill, &options).unwrap();
        apply_plan(&mut color, &plan);

        assert_eq!(color.pixel(3, 3).unwrap(), fill);
        assert_eq!(color.pixel(6, 3).unwrap(), preserved);
    }

    #[test]
    fn m2_golden_grayscale_display_coverage_and_base_color_eyedropper_agree() {
        let mut coverage = TileRaster::new(3, 3, PixelFormat::Grayscale8).unwrap();
        coverage
            .set_pixel(1, 1, PixelValue::Grayscale8(128), 1)
            .unwrap();
        let base = PixelValue::Rgba16([1_000, 2_000, 3_000, u16::MAX]);
        let plane = PlaneSample {
            raster: &coverage,
            base_color: Some(base),
        };
        assert_eq!(
            eyedropper(EyedropperSource::SelectedPlane, 1, 1, plane, &[plane], &[],).unwrap(),
            Some(base)
        );
        let composite = eyedropper(EyedropperSource::Composite, 1, 1, plane, &[plane], &[])
            .unwrap()
            .unwrap();
        let PixelValue::Rgba16(composite) = composite else {
            panic!("grayscale 16-bit base must retain its depth");
        };
        assert_eq!(&composite[..3], &[1_000, 2_000, 3_000]);
        assert_eq!(composite[3], 128 * 257);
    }

    #[test]
    fn m2_golden_selection_clips_every_fill_edit() {
        let main = binary(6, 6);
        let mut color = color8(6, 6);
        let mut selection = binary(6, 6);
        for y in 2..=3 {
            for x in 1..=2 {
                selection
                    .set_pixel(x, y, PixelValue::Binary(255), 1)
                    .unwrap();
            }
        }
        let fill = PixelValue::Rgba([80, 90, 100, 255]);
        let plan = seed_fill(
            &main,
            &color,
            Some(&selection),
            (1, 2),
            fill,
            &FillOptions::default(),
        )
        .unwrap();
        apply_plan(&mut color, &plan);
        assert_eq!(plan.edits.len(), 4);
        assert_eq!(color.pixel(1, 2).unwrap(), fill);
        assert_eq!(color.pixel(0, 2).unwrap(), PixelValue::Rgba([0; 4]));
        assert_eq!(color.pixel(3, 3).unwrap(), PixelValue::Rgba([0; 4]));
    }

    #[test]
    fn m2_golden_rgba16_palette_and_fill_are_never_implicitly_quantized() {
        let mut palette = Palette::default();
        let exact = PixelValue::Rgba16([1, 257, 32_769, 65_534]);
        palette.push(exact).unwrap();
        assert_eq!(palette.colors(), &[exact]);

        let main = binary(4, 4);
        let color = TileRaster::new(4, 4, PixelFormat::StraightRgba16).unwrap();
        let plan = seed_fill(&main, &color, None, (1, 1), exact, &FillOptions::default()).unwrap();
        assert!(plan.edits.iter().all(|edit| edit.after == exact));
    }

    #[test]
    fn m2_tolerance_detached_closed_extension_and_color_check_semantics() {
        let main = binary(5, 3);
        let mut color = color8(5, 3);
        color
            .set_pixel(0, 1, PixelValue::Rgba([10, 10, 10, 255]), 1)
            .unwrap();
        color
            .set_pixel(4, 1, PixelValue::Rgba([11, 10, 10, 255]), 1)
            .unwrap();
        let options = FillOptions {
            tolerance: 257,
            detached_regions: true,
            ..FillOptions::default()
        };
        let plan = seed_fill(
            &main,
            &color,
            None,
            (0, 1),
            PixelValue::Rgba([50, 60, 70, 255]),
            &options,
        )
        .unwrap();
        assert!(plan.edits.iter().any(|edit| (edit.x, edit.y) == (4, 1)));

        let mut extension = color8(5, 3);
        let source = PixelValue::Rgba([100, 110, 120, 255]);
        extension.set_pixel(1, 1, source, 1).unwrap();
        let operation = select_all(5, 3);
        let extension_plan = extend_fill(&extension, &operation, (1, 1), 2).unwrap();
        assert!(
            extension_plan
                .edits
                .iter()
                .all(|edit| edit.after == source && edit.x.abs_diff(1) + edit.y.abs_diff(1) <= 2)
        );

        assert_eq!(
            color_check_category(
                PixelValue::Rgba([255, 255, 255, 255]),
                ColorCheckMode::LegacyWhiteTransparency,
            ),
            ColorCheckCategory::ExactWhite
        );
        assert_eq!(
            color_check_category(
                PixelValue::Rgba([255, 255, 255, 0]),
                ColorCheckMode::NativeAlpha,
            ),
            ColorCheckCategory::Transparent
        );
        assert_eq!(
            color_check_category(
                PixelValue::Rgba([255, 255, 255, 0]),
                ColorCheckMode::LegacyWhiteTransparency,
            ),
            ColorCheckCategory::ExactWhite
        );
    }

    #[test]
    fn m2_closed_region_handles_colored_components_and_all_fill_plans_cancel_atomically() {
        let mut main = binary(7, 7);
        rectangle_boundary(&mut main, 1, 1, 5, 5);
        let mut color = color8(7, 7);
        let source = PixelValue::Rgba([20, 30, 40, 255]);
        for y in 2..5 {
            for x in 2..5 {
                color.set_pixel(x, y, source, 1).unwrap();
            }
        }
        let operation = select_all(7, 7);
        let fill = PixelValue::Rgba([100, 110, 120, 255]);
        let plan =
            closed_region_fill(&main, &color, &operation, fill, &FillOptions::default()).unwrap();
        assert_eq!(plan.edits.len(), 9);
        assert!(plan.edits.iter().all(|edit| edit.before == source));

        let transparent_only = FillOptions {
            transparent_only: true,
            ..FillOptions::default()
        };
        assert!(
            closed_region_fill(&main, &color, &operation, fill, &transparent_only)
                .unwrap()
                .edits
                .is_empty()
        );

        let checksum = color.checksum();
        assert_eq!(
            closed_region_fill_with_cancel(
                &main,
                &color,
                &operation,
                fill,
                &FillOptions::default(),
                || true,
            ),
            Err(FillError::Cancelled)
        );
        assert_eq!(
            extend_fill_with_cancel(&color, &operation, (2, 2), 2, || true),
            Err(FillError::Cancelled)
        );
        assert_eq!(color.checksum(), checksum);
    }
}
