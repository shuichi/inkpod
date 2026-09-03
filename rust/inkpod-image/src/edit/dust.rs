use super::DustRemoval;
use crate::line_correction::{bounded_vec, neighbors};
use crate::{DustMode, PixelValue, RasterError, TileRaster};
use std::collections::VecDeque;

/// Classifies complete source components; only wholly selected components change.
/// Foreground/outliers use eight neighbors, holes four. Cancellation returns no raster.
pub fn apply_dust_removal(
    source: &TileRaster,
    operation_mask: Option<&TileRaster>,
    options: DustRemoval,
    revision: u64,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<TileRaster, RasterError> {
    let count = crate::line_correction::grid::validate(source, operation_mask)?;
    options.background.validate(source.format())?;
    if options.maximum_pixels == 0 || options.maximum_pixels > 65_536 {
        return Err(RasterError::InvalidDimensions);
    }
    let mut visited = bounded_vec(count, 0u8)?;
    let mut result = source.clone();
    let mut queue = VecDeque::<u32>::new();
    let mut component = Vec::new();
    component
        .try_reserve_exact(options.maximum_pixels as usize)
        .map_err(|_| RasterError::InvalidDimensions)?;
    let mut completed = 0u64;
    for index in 0..count as u32 {
        if visited[index as usize] != 0 {
            continue;
        }
        if !progress(completed, count as u64) {
            return Err(RasterError::Cancelled);
        }
        let seed = source.pixel(index % source.width(), index / source.width())?;
        let is_background = options.background.contains(seed);
        if (options.mode == DustMode::RemoveForeground && is_background)
            || (options.mode == DustMode::FillTransparentHoles && !is_background)
        {
            visited[index as usize] = 1;
            completed += 1;
            continue;
        }
        let same = |value: PixelValue| match options.mode {
            DustMode::RemoveForeground => !options.background.contains(value),
            DustMode::FillTransparentHoles => options.background.contains(value),
            DustMode::ReplaceColorOutliers => {
                options.background.normalized_background(value)
                    == options.background.normalized_background(seed)
            }
        };
        queue.clear();
        queue
            .try_reserve(1)
            .map_err(|_| RasterError::InvalidDimensions)?;
        queue.push_back(index);
        visited[index as usize] = 1;
        component.clear();
        let mut size = 0u64;
        let mut contained = true;
        let mut edge = false;
        let mut surrounding = None;
        let mut ambiguous = false;
        let mut sums = [0u64; 4];
        let mut surround_count = 0u64;
        while let Some(current) = queue.pop_front() {
            let (x, y) = (current % source.width(), current / source.width());
            size += 1;
            completed += 1;
            if size <= u64::from(options.maximum_pixels) {
                component.push(current);
            }
            contained &= crate::line_correction::grid::selected(operation_mask, x, y)?;
            edge |= x == 0 || y == 0 || x + 1 == source.width() || y + 1 == source.height();
            if completed % 4096 == 0 && !progress(completed, count as u64) {
                return Err(RasterError::Cancelled);
            }
            for neighbor in neighbors(
                current,
                source.width(),
                source.height(),
                options.mode != DustMode::FillTransparentHoles,
            ) {
                let value = source.pixel(neighbor % source.width(), neighbor / source.width())?;
                if same(value) {
                    if visited[neighbor as usize] == 0 {
                        if queue.len() >= 1_048_576 {
                            return Err(RasterError::InvalidDimensions);
                        }
                        queue
                            .try_reserve(1)
                            .map_err(|_| RasterError::InvalidDimensions)?;
                        visited[neighbor as usize] = 1;
                        queue.push_back(neighbor);
                    }
                } else {
                    let (nx, ny) = (neighbor % source.width(), neighbor / source.width());
                    if x.abs_diff(nx) + y.abs_diff(ny) != 1 {
                        continue;
                    }
                    let value = options.background.normalized_background(value);
                    if let Some(previous) = surrounding {
                        ambiguous |= previous != value;
                    } else {
                        surrounding = Some(value);
                    }
                    let channels = value.rgba16().unwrap_or_else(|| {
                        let v = match value {
                            PixelValue::Binary(v) | PixelValue::Grayscale8(v) => u16::from(v) * 257,
                            PixelValue::Grayscale16(v) => v,
                            _ => 0,
                        };
                        [v; 4]
                    });
                    for channel in 0..4 {
                        sums[channel] += u64::from(channels[channel]);
                    }
                    surround_count += 1;
                }
            }
        }
        if size > u64::from(options.maximum_pixels)
            || !contained
            || surrounding.is_none()
            || (options.mode == DustMode::FillTransparentHoles && edge)
        {
            continue;
        }
        let replacement = if options.mode == DustMode::RemoveForeground {
            if ambiguous {
                continue;
            }
            surrounding.expect("nonempty surroundings")
        } else {
            let mean = sums.map(|v| ((v + surround_count / 2) / surround_count) as u16);
            match seed {
                PixelValue::Binary(_) => PixelValue::Binary(if mean[0] >= 32768 { 255 } else { 0 }),
                PixelValue::Grayscale8(_) => {
                    PixelValue::Grayscale8(((u32::from(mean[0]) + 128) / 257) as u8)
                }
                PixelValue::Grayscale16(_) => PixelValue::Grayscale16(mean[0]),
                PixelValue::Rgba(_) => {
                    PixelValue::Rgba(mean.map(|v| ((u32::from(v) + 128) / 257) as u8))
                }
                PixelValue::Rgba16(_) => PixelValue::Rgba16(mean),
            }
        };
        for &pixel in &component {
            result.set_pixel(
                pixel % source.width(),
                pixel / source.width(),
                replacement,
                revision,
            )?;
        }
    }
    if !progress(count as u64, count as u64) {
        return Err(RasterError::Cancelled);
    }
    Ok(result)
}
