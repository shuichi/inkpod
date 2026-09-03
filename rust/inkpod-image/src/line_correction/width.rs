use super::grid::{Grid, MAX_WORK, bounded_vec, selected, validate};
use super::{LineBackground, LineWidthMode};
use crate::{RasterError, TileRaster};

pub(super) fn disk(dx: i64, dy: i64, width: u32) -> bool {
    let bias = i64::from(width % 2 == 0);
    let (x, y) = (2 * dx + bias, 2 * dy + bias);
    x * x + y * y <= i64::from(width) * i64::from(width)
}

/// Circular native-depth morphology or centerline reconstruction. Writes only the mask.
/// Radius is one-sided for thicken/thin, full diameter for uniform. Values are 1..=256.
pub fn apply_line_width(
    source: &TileRaster,
    mask: Option<&TileRaster>,
    mode: LineWidthMode,
    amount: u32,
    background: LineBackground,
    revision: u64,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    let count = validate(source, mask)?;
    background.validate(source.format())?;
    if amount == 0 || amount > 256 {
        return Err(RasterError::InvalidDimensions);
    }
    if mode == LineWidthMode::Uniform {
        return uniform(source, mask, amount, background, revision, &mut progress);
    }
    let radius = i64::from(amount);
    if (count as u64)
        .checked_mul((2 * u64::from(amount) + 1).pow(2))
        .is_none_or(|v| v > MAX_WORK)
    {
        return Err(RasterError::InvalidDimensions);
    }
    let mut result = source.clone();
    for y in 0..source.height() {
        if !progress(u64::from(y), u64::from(source.height())) {
            return Err(RasterError::Cancelled);
        }
        for x in 0..source.width() {
            if !selected(mask, x, y)? {
                continue;
            }
            let mut value = source.pixel(x, y)?;
            let mut coverage = background.coverage(value);
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy > radius * radius {
                        continue;
                    }
                    let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
                    let candidate = if nx < 0
                        || ny < 0
                        || nx >= i64::from(source.width())
                        || ny >= i64::from(source.height())
                    {
                        background.empty(source.format())
                    } else {
                        source.pixel(nx as u32, ny as u32)?
                    };
                    let amount = background.coverage(candidate);
                    if (mode == LineWidthMode::Thicken && amount > coverage)
                        || (mode == LineWidthMode::Thin && amount < coverage)
                    {
                        value = candidate;
                        coverage = amount;
                    }
                }
            }
            result.set_pixel(x, y, value, revision)?;
        }
    }
    if !progress(1, 1) {
        return Err(RasterError::Cancelled);
    }
    Ok(result)
}

fn uniform(
    source: &TileRaster,
    mask: Option<&TileRaster>,
    width: u32,
    background: LineBackground,
    revision: u64,
    progress: &mut impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    let original = Grid::from_source(source, background, progress)?;
    let skeleton = original.skeleton(progress)?;
    let background_owner = nearest_background(&original, progress)?;
    let points = skeleton.bits.iter().filter(|&&v| v != 0).count() as u64;
    if points
        .checked_mul((u64::from(width) + 2).pow(2))
        .is_none_or(|v| v > MAX_WORK)
    {
        return Err(RasterError::InvalidDimensions);
    }
    let mut owner = bounded_vec(original.bits.len(), u32::MAX)?;
    let mut distance = bounded_vec(original.bits.len(), u32::MAX)?;
    let radius = i64::from(width.div_ceil(2));
    for (index, &value) in skeleton.bits.iter().enumerate() {
        if index % 4096 == 0 && !progress(index as u64, skeleton.bits.len() as u64) {
            return Err(RasterError::Cancelled);
        }
        if value == 0 {
            continue;
        }
        let (x, y) = (
            (index as u32 % source.width()) as i64,
            (index as u32 / source.width()) as i64,
        );
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let (nx, ny) = (x + dx, y + dy);
                if !disk(dx, dy, width)
                    || nx < 0
                    || ny < 0
                    || nx >= i64::from(source.width())
                    || ny >= i64::from(source.height())
                {
                    continue;
                }
                let target = (ny * i64::from(source.width()) + nx) as usize;
                let d = (dx * dx + dy * dy) as u32;
                if d < distance[target] {
                    distance[target] = d;
                    owner[target] = index as u32;
                }
            }
        }
    }
    let mut result = source.clone();
    for (index, &owner) in owner.iter().enumerate() {
        let (x, y) = (index as u32 % source.width(), index as u32 / source.width());
        if index % 4096 == 0 && !progress(index as u64, original.bits.len() as u64) {
            return Err(RasterError::Cancelled);
        }
        if !selected(mask, x, y)? {
            continue;
        }
        let pixel = if owner != u32::MAX {
            source.pixel(owner % source.width(), owner / source.width())?
        } else if original.bits[index] != 0 {
            let at = background_owner[index];
            if at == u32::MAX {
                background.empty(source.format())
            } else {
                background
                    .normalized_background(source.pixel(at % source.width(), at / source.width())?)
            }
        } else {
            continue;
        };
        result.set_pixel(x, y, pixel, revision)?;
    }
    if !progress(1, 1) {
        return Err(RasterError::Cancelled);
    }
    Ok(result)
}

// Multi-source, four-neighbor expansion supplies the actual nearby background
// when a centerline reconstruction removes old ink. White+transparent mode
// therefore never paints opaque white over a transparent-background drawing.
fn nearest_background(
    grid: &Grid,
    progress: &mut impl FnMut(u64, u64) -> bool,
) -> Result<Vec<u32>, RasterError> {
    let mut owner = bounded_vec(grid.bits.len(), u32::MAX)?;
    let mut queue = std::collections::VecDeque::new();
    for (index, &ink) in grid.bits.iter().enumerate() {
        if index % 4096 == 0 && !progress(index as u64, grid.bits.len() as u64) {
            return Err(RasterError::Cancelled);
        }
        if ink == 0 {
            continue;
        }
        if let Some(at) = super::grid::neighbors(index as u32, grid.width, grid.height, false)
            .find(|&n| grid.bits[n as usize] == 0)
        {
            owner[index] = at;
            enqueue_background(&mut queue, index as u32)?;
        }
    }
    let mut visited = 0;
    while let Some(index) = queue.pop_front() {
        visited += 1;
        if visited % 4096 == 0 && !progress(visited, grid.bits.len() as u64) {
            return Err(RasterError::Cancelled);
        }
        for next in super::grid::neighbors(index, grid.width, grid.height, false) {
            if grid.bits[next as usize] == 0 || owner[next as usize] != u32::MAX {
                continue;
            }
            owner[next as usize] = owner[index as usize];
            enqueue_background(&mut queue, next)?;
        }
    }
    Ok(owner)
}

fn enqueue_background(
    queue: &mut std::collections::VecDeque<u32>,
    index: u32,
) -> Result<(), RasterError> {
    if queue.len() >= 1_048_576 {
        return Err(RasterError::InvalidDimensions);
    }
    queue
        .try_reserve(1)
        .map_err(|_| RasterError::InvalidDimensions)?;
    queue.push_back(index);
    Ok(())
}
