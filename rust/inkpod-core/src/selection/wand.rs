use super::*;

pub(super) fn wand_mask(
    source: &TileRaster,
    x: u32,
    y: u32,
    tolerance: u16,
    gap: u8,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    let count = bounded_document_pixels(source.width(), source.height())? as usize;
    let target = source.pixel(x, y)?;
    let mut boundary = TileRaster::new(source.width(), source.height(), PixelFormat::BinaryMask8)?;
    if gap > 0 {
        for row in 0..source.height() {
            for col in 0..source.width() {
                if !pixel_within_tolerance(source.pixel(col, row)?, target, tolerance) {
                    boundary.set_pixel(col, row, PixelValue::Binary(255), revision)?;
                }
            }
        }
        boundary =
            inkpod_image::virtual_gap_barrier(&boundary, u32::from(gap), revision, |_, _| true)?;
    }
    let mut visited = Vec::new();
    visited
        .try_reserve_exact(count)
        .map_err(|_| CoreError::InvalidArgument("wand visitation allocation failed"))?;
    visited.resize(count, 0_u8);
    let mut queue = VecDeque::new();
    let mut result = TileRaster::new(source.width(), source.height(), PixelFormat::BinaryMask8)?;
    let seed = y * source.width() + x;
    queue.push_back(seed);
    visited[seed as usize] = 1;
    while let Some(index) = queue.pop_front() {
        let x = index % source.width();
        let y = index / source.width();
        if boundary.pixel(x, y)? == PixelValue::Binary(255)
            || !pixel_within_tolerance(source.pixel(x, y)?, target, tolerance)
        {
            continue;
        }
        result.set_pixel(x, y, PixelValue::Binary(255), revision)?;
        let neighbors = [
            x.checked_sub(1).map(|x| y * source.width() + x),
            (x + 1 < source.width()).then_some(index + 1),
            y.checked_sub(1).map(|y| y * source.width() + x),
            (y + 1 < source.height()).then_some(index + source.width()),
        ];
        for next in neighbors.into_iter().flatten() {
            if visited[next as usize] != 0 {
                continue;
            }
            if queue.len() >= 1_048_576 {
                return Err(CoreError::InvalidArgument("wand queue exceeds work limit"));
            }
            queue
                .try_reserve(1)
                .map_err(|_| CoreError::InvalidArgument("wand queue allocation failed"))?;
            visited[next as usize] = 1;
            queue.push_back(next);
        }
    }
    Ok(result)
}
