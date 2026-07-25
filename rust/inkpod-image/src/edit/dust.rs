use super::common::*;
use super::*;
use crate::{RasterError, TileRaster};

pub fn apply_dust_removal(
    source: &TileRaster,
    operation_mask: Option<&TileRaster>,
    options: DustRemoval,
    revision: u64,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    validate_color_raster(source)?;
    validate_selection(source, operation_mask)?;
    if options.maximum_pixels == 0 || options.maximum_pixels > 65_536 {
        return Err(RasterError::InvalidDimensions);
    }
    let pixel_count = usize::try_from(u64::from(source.width()) * u64::from(source.height()))
        .map_err(|_| RasterError::InvalidDimensions)?;
    let total = u64::try_from(pixel_count).map_err(|_| RasterError::InvalidDimensions)?;
    let mut visited = vec![false; pixel_count];
    let mut result = source.clone();
    let mut completed = 0_u64;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let index = raster_index(source.width(), x, y)?;
            if visited[index] || !selected(operation_mask, x, y)? {
                completed += 1;
                continue;
            }
            let seed = source
                .pixel(x, y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            let eligible = match options.mode {
                DustMode::RemoveForeground => seed[3] != 0,
                DustMode::FillTransparentHoles => seed[3] == 0,
                DustMode::ReplaceColorOutliers => true,
            };
            if !eligible {
                visited[index] = true;
                completed += 1;
                continue;
            }
            let mut queue = std::collections::VecDeque::from([(x, y)]);
            let mut component = Vec::new();
            let mut oversized = false;
            let mut touches_boundary = false;
            while let Some((candidate_x, candidate_y)) = queue.pop_front() {
                let candidate_index = raster_index(source.width(), candidate_x, candidate_y)?;
                if visited[candidate_index] || !selected(operation_mask, candidate_x, candidate_y)?
                {
                    continue;
                }
                let value = source
                    .pixel(candidate_x, candidate_y)?
                    .rgba16()
                    .ok_or(RasterError::PixelFormatMismatch)?;
                let same_component = match options.mode {
                    DustMode::RemoveForeground => value[3] != 0,
                    DustMode::FillTransparentHoles => value[3] == 0,
                    DustMode::ReplaceColorOutliers => value == seed,
                };
                if !same_component {
                    continue;
                }
                visited[candidate_index] = true;
                if component.len() <= options.maximum_pixels as usize {
                    component.push((candidate_x, candidate_y));
                } else {
                    oversized = true;
                }
                touches_boundary |= candidate_x == 0
                    || candidate_y == 0
                    || candidate_x + 1 == source.width()
                    || candidate_y + 1 == source.height();
                for neighbor in
                    four_neighbors(candidate_x, candidate_y, source.width(), source.height())
                {
                    queue.push_back(neighbor);
                }
            }
            completed = completed.saturating_add(component.len() as u64);
            if !progress(completed.min(total), total) {
                return Err(RasterError::Cancelled);
            }
            if component.is_empty()
                || oversized
                || component.len() > options.maximum_pixels as usize
            {
                continue;
            }
            if options.mode == DustMode::FillTransparentHoles && touches_boundary {
                continue;
            }
            let replacement = match options.mode {
                DustMode::RemoveForeground => [0; 4],
                DustMode::FillTransparentHoles | DustMode::ReplaceColorOutliers => {
                    surrounding_average(source, &component, seed)?
                }
            };
            if options.mode == DustMode::ReplaceColorOutliers && replacement == seed {
                continue;
            }
            let replacement = from_rgba16(source.format(), replacement);
            for (component_x, component_y) in component {
                result.set_pixel(component_x, component_y, replacement, revision)?;
            }
        }
        if !progress(completed.min(total), total) {
            return Err(RasterError::Cancelled);
        }
    }
    if !progress(total, total) {
        return Err(RasterError::Cancelled);
    }
    Ok(result)
}

fn raster_index(width: u32, x: u32, y: u32) -> Result<usize, RasterError> {
    usize::try_from(u64::from(y) * u64::from(width) + u64::from(x))
        .map_err(|_| RasterError::InvalidDimensions)
}

fn four_neighbors(x: u32, y: u32, width: u32, height: u32) -> Vec<(u32, u32)> {
    let mut result = Vec::with_capacity(4);
    if x > 0 {
        result.push((x - 1, y));
    }
    if x + 1 < width {
        result.push((x + 1, y));
    }
    if y > 0 {
        result.push((x, y - 1));
    }
    if y + 1 < height {
        result.push((x, y + 1));
    }
    result
}

fn surrounding_average(
    source: &TileRaster,
    component: &[(u32, u32)],
    component_color: [u16; 4],
) -> Result<[u16; 4], RasterError> {
    let component_set = component
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut sums = [0_u128; 4];
    let mut count = 0_u128;
    for &(x, y) in component {
        for (neighbor_x, neighbor_y) in four_neighbors(x, y, source.width(), source.height()) {
            if component_set.contains(&(neighbor_x, neighbor_y)) {
                continue;
            }
            let value = source
                .pixel(neighbor_x, neighbor_y)?
                .rgba16()
                .ok_or(RasterError::PixelFormatMismatch)?;
            if value == component_color {
                continue;
            }
            for channel in 0..4 {
                sums[channel] += u128::from(value[channel]);
            }
            count += 1;
        }
    }
    if count == 0 {
        return Ok(component_color);
    }
    Ok(std::array::from_fn(|channel| {
        ((sums[channel] + count / 2) / count) as u16
    }))
}
