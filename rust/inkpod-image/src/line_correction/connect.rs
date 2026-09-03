use super::LineBackground;
use super::grid::{Grid, MAX_WORK, neighbors, selected, validate};
use super::width::disk;
use crate::{PixelFormat, RasterError, TileRaster};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Endpoint {
    pixel: u32,
    x: i64,
    y: i64,
    dx: i64,
    dy: i64,
}

fn endpoints(
    grid: &Grid,
    progress: &mut impl FnMut(u64, u64) -> bool,
) -> Result<Vec<Endpoint>, RasterError> {
    let skeleton = grid.skeleton(progress)?;
    let mut output = Vec::new();
    for (i, &v) in skeleton.bits.iter().enumerate() {
        if i % 4096 == 0 && !progress(i as u64, grid.bits.len() as u64) {
            return Err(RasterError::Cancelled);
        }
        if v == 0 {
            continue;
        }
        let mut adjacent = neighbors(i as u32, grid.width, grid.height, true)
            .filter(|&n| skeleton.bits[n as usize] != 0);
        let Some(previous) = adjacent.next() else {
            continue;
        };
        if adjacent.next().is_some() {
            continue;
        }
        let (mut x, mut y) = (
            (i as u32 % grid.width) as i64,
            (i as u32 / grid.width) as i64,
        );
        let (dx, dy) = (
            x - i64::from(previous % grid.width),
            y - i64::from(previous / grid.width),
        );
        // Restore a thick line's actual end after centerline thinning.
        while grid.at(x + dx, y + dy) != 0 {
            x += dx;
            y += dy;
        }
        if output.len() >= 65_536 {
            return Err(RasterError::InvalidDimensions);
        }
        output
            .try_reserve(1)
            .map_err(|_| RasterError::InvalidDimensions)?;
        output.push(Endpoint {
            pixel: (y * i64::from(grid.width) + x) as u32,
            x,
            y,
            dx,
            dy,
        });
    }
    output.sort_by_key(|p| p.pixel);
    output.dedup_by_key(|p| p.pixel);
    Ok(output)
}

fn facing(a: Endpoint, b: Endpoint) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dot = a.dx * dx + a.dy * dy;
    dot > 0 && 2 * dot * dot >= (a.dx * a.dx + a.dy * a.dy) * (dx * dx + dy * dy)
}

fn path(a: Endpoint, b: Endpoint, width: u32) -> Vec<u32> {
    let (mut x, mut y) = (a.x, a.y);
    let (dx, dy) = ((b.x - x).abs(), -(b.y - y).abs());
    let (sx, sy) = ((b.x - x).signum(), (b.y - y).signum());
    let mut error = dx + dy;
    let mut output = Vec::with_capacity((dx.max(-dy) + 1) as usize);
    loop {
        output.push((y * i64::from(width) + x) as u32);
        if x == b.x && y == b.y {
            break;
        }
        let twice = 2 * error;
        if twice >= dy {
            error += dy;
            x += sx;
        }
        if twice <= dx {
            error += dx;
            y += sy;
        }
    }
    output
}

fn footprint(
    grid: &Grid,
    mask: Option<&TileRaster>,
    a: Endpoint,
    b: Endpoint,
    width: u32,
    work: &mut u64,
) -> Result<Option<Vec<u32>>, RasterError> {
    let line = path(a, b, grid.width);
    if line
        .iter()
        .skip(1)
        .take(line.len().saturating_sub(2))
        .any(|&p| grid.bits[p as usize] != 0)
    {
        return Ok(None);
    }
    let radius = i64::from(width.div_ceil(2));
    let count = (line.len() as u64) * (2 * radius as u64 + 1).pow(2);
    *work = work
        .checked_add(count)
        .ok_or(RasterError::InvalidDimensions)?;
    if *work > MAX_WORK {
        return Err(RasterError::InvalidDimensions);
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count as usize)
        .map_err(|_| RasterError::InvalidDimensions)?;
    for point in line {
        let (x, y) = (i64::from(point % grid.width), i64::from(point / grid.width));
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if !disk(dx, dy, width) {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= i64::from(grid.width) || ny >= i64::from(grid.height) {
                    return Ok(None);
                }
                if !selected(mask, nx as u32, ny as u32)? {
                    return Ok(None);
                }
                let pixel = (ny * i64::from(grid.width) + nx) as u32;
                // Existing ink is permitted only in the two attachment caps.
                if grid.bits[pixel as usize] != 0
                    && !disk(nx - a.x, ny - a.y, width)
                    && !disk(nx - b.x, ny - b.y, width)
                {
                    return Ok(None);
                }
                pixels.push(pixel);
            }
        }
    }
    pixels.sort_unstable();
    pixels.dedup();
    Ok(Some(pixels))
}

/// Adds permanent native-depth ink between mutually unambiguous facing endpoints.
/// `gap` is 0..=64 empty grid steps; width is a full document diameter, 1..=256.
/// Source ink is never recolored. Both endpoints and the whole footprint must be selected.
pub fn apply_line_connection(
    source: &TileRaster,
    mask: Option<&TileRaster>,
    gap: u32,
    width: u32,
    background: LineBackground,
    revision: u64,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    validate(source, mask)?;
    background.validate(source.format())?;
    if gap > 64 || width == 0 || width > 256 {
        return Err(RasterError::InvalidDimensions);
    }
    if !progress(0, 1) {
        return Err(RasterError::Cancelled);
    }
    if gap == 0 {
        return Ok(source.clone());
    }
    let grid = Grid::from_source(source, background, &mut progress)?;
    let ends = endpoints(&grid, &mut progress)?;
    let bucket = i64::from(gap + 2);
    let mut buckets = BTreeMap::<(i64, i64), Vec<usize>>::new();
    for (i, end) in ends.iter().enumerate() {
        buckets
            .entry((end.x / bucket, end.y / bucket))
            .or_default()
            .push(i);
    }
    type Score = (u64, u64);
    let mut best = super::bounded_vec(ends.len(), None::<(Score, usize, bool)>)?;
    let mut work = 0u64;
    for (i, &a) in ends.iter().enumerate() {
        if !progress(i as u64, ends.len() as u64) {
            return Err(RasterError::Cancelled);
        }
        for by in -1..=1 {
            for bx in -1..=1 {
                let Some(indices) = buckets.get(&(a.x / bucket + bx, a.y / bucket + by)) else {
                    continue;
                };
                for &j in indices {
                    if j <= i {
                        continue;
                    }
                    work += 1;
                    if work > MAX_WORK {
                        return Err(RasterError::InvalidDimensions);
                    }
                    let b = ends[j];
                    let (dx, dy) = (a.x.abs_diff(b.x), a.y.abs_diff(b.y));
                    let distance = dx.max(dy);
                    if distance <= 1
                        || distance - 1 > u64::from(gap)
                        || !facing(a, b)
                        || !facing(b, a)
                    {
                        continue;
                    }
                    if footprint(&grid, mask, a, b, width, &mut work)?.is_none() {
                        continue;
                    }
                    let score = (distance - 1, dx * dx + dy * dy);
                    for (at, other) in [(i, j), (j, i)] {
                        match best[at] {
                            None => best[at] = Some((score, other, false)),
                            Some((prior, _, _)) if score < prior => {
                                best[at] = Some((score, other, false))
                            }
                            Some((prior, partner, _)) if score == prior => {
                                best[at] = Some((prior, partner, true))
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    let mut bridges = Vec::<(usize, usize, Vec<u32>, bool)>::new();
    let mut owners = BTreeMap::<u32, usize>::new();
    for i in 0..ends.len() {
        let Some((_, j, false)) = best[i] else {
            continue;
        };
        if j <= i || !matches!(best[j],Some((_,other,false)) if other==i) {
            continue;
        }
        let Some(pixels) = footprint(&grid, mask, ends[i], ends[j], width, &mut work)? else {
            continue;
        };
        let mut conflict = false;
        for &p in &pixels {
            if grid.bits[p as usize] != 0 {
                continue;
            }
            if owners.len() >= 1_048_576 {
                return Err(RasterError::InvalidDimensions);
            }
            if let Some(&other) = owners.get(&p) {
                bridges[other].3 = true;
                conflict = true;
            } else {
                owners.insert(p, bridges.len());
            }
        }
        bridges.push((i, j, pixels, conflict));
    }
    let mut result = source.clone();
    for (i, j, pixels, conflict) in bridges {
        if !progress(work, MAX_WORK) {
            return Err(RasterError::Cancelled);
        }
        if conflict {
            continue;
        }
        let a = source.pixel(ends[i].pixel % grid.width, ends[i].pixel / grid.width)?;
        let b = source.pixel(ends[j].pixel % grid.width, ends[j].pixel / grid.width)?;
        let color = if background.coverage(b) > background.coverage(a) {
            b
        } else {
            a
        };
        for pixel in pixels {
            if grid.bits[pixel as usize] == 0 {
                result.set_pixel(pixel % grid.width, pixel / grid.width, color, revision)?;
            }
        }
    }
    if !progress(1, 1) {
        return Err(RasterError::Cancelled);
    }
    Ok(result)
}

/// Returns an immutable-selection boundary with gaps closed before flood exploration.
/// The caller's source raster is unchanged. Boundary must be BinaryMask8.
pub fn virtual_gap_barrier(
    boundary: &TileRaster,
    gap: u32,
    revision: u64,
    progress: impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    if boundary.format() != PixelFormat::BinaryMask8 {
        return Err(RasterError::PixelFormatMismatch);
    }
    apply_line_connection(
        boundary,
        None,
        gap,
        1,
        LineBackground::Transparent,
        revision,
        progress,
    )
}
