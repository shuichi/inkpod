use super::LineBackground;
use crate::{MAX_IMAGE_EDIT_PIXELS, PixelFormat, RasterError, TileRaster};

pub(crate) const MAX_WORK: u64 = 1_100_000_000;

pub(crate) fn bounded_vec<T: Clone>(count: usize, value: T) -> Result<Vec<T>, RasterError> {
    let mut data = Vec::new();
    data.try_reserve_exact(count)
        .map_err(|_| RasterError::InvalidDimensions)?;
    data.resize(count, value);
    Ok(data)
}

pub(crate) fn neighbors(
    index: u32,
    width: u32,
    height: u32,
    diagonal: bool,
) -> impl Iterator<Item = u32> {
    let x = (index % width) as i64;
    let y = (index / width) as i64;
    [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ]
    .into_iter()
    .filter(move |&(dx, dy)| diagonal || dx == 0 || dy == 0)
    .filter_map(move |(dx, dy)| {
        let (nx, ny) = (x + dx, y + dy);
        (nx >= 0 && ny >= 0 && nx < i64::from(width) && ny < i64::from(height))
            .then_some((ny * i64::from(width) + nx) as u32)
    })
}

pub(crate) fn validate(
    source: &TileRaster,
    mask: Option<&TileRaster>,
) -> Result<usize, RasterError> {
    let count = u64::from(source.width()) * u64::from(source.height());
    if count > MAX_IMAGE_EDIT_PIXELS || source.format() == PixelFormat::PremultipliedBgra8 {
        return Err(RasterError::InvalidDimensions);
    }
    if let Some(mask) = mask {
        if mask.width() != source.width()
            || mask.height() != source.height()
            || mask.format() != PixelFormat::BinaryMask8
        {
            return Err(RasterError::PixelFormatMismatch);
        }
    }
    usize::try_from(count).map_err(|_| RasterError::InvalidDimensions)
}

pub(crate) fn selected(mask: Option<&TileRaster>, x: u32, y: u32) -> Result<bool, RasterError> {
    Ok(match mask {
        None => true,
        Some(m) => !m.pixel(x, y)?.is_transparent(),
    })
}

pub(crate) struct Grid {
    pub width: u32,
    pub height: u32,
    pub bits: Vec<u8>,
}
impl Grid {
    pub fn from_source(
        source: &TileRaster,
        background: LineBackground,
        progress: &mut impl FnMut(u64, u64) -> bool,
    ) -> Result<Self, RasterError> {
        let count = validate(source, None)?;
        let mut bits = bounded_vec(count, 0)?;
        for y in 0..source.height() {
            if !progress(u64::from(y), u64::from(source.height())) {
                return Err(RasterError::Cancelled);
            }
            for x in 0..source.width() {
                bits[(y * source.width() + x) as usize] =
                    u8::from(!background.contains(source.pixel(x, y)?));
            }
        }
        Ok(Self {
            width: source.width(),
            height: source.height(),
            bits,
        })
    }
    pub fn at(&self, x: i64, y: i64) -> u8 {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            0
        } else {
            self.bits[(y * i64::from(self.width) + x) as usize]
        }
    }
    /// Two-subiteration thinning with a sequential simple-point guard. Adjacent
    /// parallel candidates cannot jointly erase a small component or sever a loop.
    pub fn skeleton(
        &self,
        progress: &mut impl FnMut(u64, u64) -> bool,
    ) -> Result<Self, RasterError> {
        let mut bits = bounded_vec(self.bits.len(), 0)?;
        bits.copy_from_slice(&self.bits);
        let mut result = Self {
            width: self.width,
            height: self.height,
            bits,
        };
        let mut remove = bounded_vec(self.bits.len(), 0u8)?;
        let mut work = 0u64;
        loop {
            let mut changed = false;
            for phase in 0..2 {
                work = work
                    .checked_add(self.bits.len() as u64)
                    .ok_or(RasterError::InvalidDimensions)?;
                if work > MAX_WORK {
                    return Err(RasterError::InvalidDimensions);
                }
                remove.fill(0);
                for y in 0..self.height {
                    if !progress(work, u64::MAX) {
                        return Err(RasterError::Cancelled);
                    }
                    for x in 0..self.width {
                        let index = (y * self.width + x) as usize;
                        if result.bits[index] == 0 {
                            continue;
                        }
                        let (x, y) = (i64::from(x), i64::from(y));
                        let p = [
                            result.at(x, y - 1),
                            result.at(x + 1, y - 1),
                            result.at(x + 1, y),
                            result.at(x + 1, y + 1),
                            result.at(x, y + 1),
                            result.at(x - 1, y + 1),
                            result.at(x - 1, y),
                            result.at(x - 1, y - 1),
                        ];
                        let count: u8 = p.iter().sum();
                        let transitions =
                            (0..8).filter(|&i| p[i] == 0 && p[(i + 1) % 8] != 0).count();
                        let corners = if phase == 0 {
                            p[0] * p[2] * p[4] == 0 && p[2] * p[4] * p[6] == 0
                        } else {
                            p[0] * p[2] * p[6] == 0 && p[0] * p[4] * p[6] == 0
                        };
                        if (2..=6).contains(&count) && transitions == 1 && corners {
                            remove[index] = 1;
                        }
                    }
                }
                for (index, &candidate) in remove.iter().enumerate() {
                    if candidate == 0 {
                        continue;
                    }
                    let (x, y) = (
                        (index as u32 % self.width) as i64,
                        (index as u32 / self.width) as i64,
                    );
                    let p = [
                        result.at(x, y - 1),
                        result.at(x + 1, y - 1),
                        result.at(x + 1, y),
                        result.at(x + 1, y + 1),
                        result.at(x, y + 1),
                        result.at(x - 1, y + 1),
                        result.at(x - 1, y),
                        result.at(x - 1, y - 1),
                    ];
                    let count: u8 = p.iter().sum();
                    if (2..=6).contains(&count)
                        && (0..8).filter(|&i| p[i] == 0 && p[(i + 1) % 8] != 0).count() == 1
                    {
                        result.bits[index] = 0;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Ok(result)
    }
}
