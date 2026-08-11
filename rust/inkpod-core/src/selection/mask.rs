use super::*;

pub(crate) fn combine_selection_masks(
    base: &TileRaster,
    candidate: &TileRaster,
    operation: SelectionOperation,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    if base.width() != candidate.width() || base.height() != candidate.height() {
        return Err(CoreError::InvalidArgument("selection dimensions differ"));
    }
    bounded_document_pixels(base.width(), base.height())?;
    let mut output = TileRaster::new(base.width(), base.height(), PixelFormat::BinaryMask8)?;
    for y in 0..base.height() {
        for x in 0..base.width() {
            let left = matches!(base.pixel(x, y)?, PixelValue::Binary(255));
            let right = matches!(candidate.pixel(x, y)?, PixelValue::Binary(255));
            let selected = match operation {
                SelectionOperation::New => right,
                SelectionOperation::Add => left || right,
                SelectionOperation::Subtract => left && !right,
                SelectionOperation::Intersect => left && right,
            };
            if selected {
                output.set_pixel(x, y, PixelValue::Binary(255), revision)?;
            }
        }
    }
    Ok(output)
}

pub(crate) fn selection_masks_have_same_coverage(
    left: &TileRaster,
    right: &TileRaster,
) -> Result<bool, CoreError> {
    if left.width() != right.width() || left.height() != right.height() {
        return Ok(false);
    }
    bounded_document_pixels(left.width(), left.height())?;
    for y in 0..left.height() {
        for x in 0..left.width() {
            if left.pixel(x, y)? != right.pixel(x, y)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn invert_selection_mask(
    source: &TileRaster,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut output = TileRaster::new(source.width(), source.height(), PixelFormat::BinaryMask8)?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            if matches!(source.pixel(x, y)?, PixelValue::Binary(0)) {
                output.set_pixel(x, y, PixelValue::Binary(255), revision)?;
            }
        }
    }
    Ok(output)
}

pub(crate) fn morphology_selection(
    source: &TileRaster,
    pixels: i32,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    let document_pixels = bounded_document_pixels(source.width(), source.height())?;
    let steps = u64::from(pixels.unsigned_abs());
    if document_pixels.saturating_mul(steps.max(1)) > MAX_FILL_PIXELS {
        return Err(CoreError::InvalidArgument(
            "selection morphology exceeds the bounded work limit",
        ));
    }
    let mut current = source.clone();
    for _ in 0..steps {
        let mut next = TileRaster::new(source.width(), source.height(), PixelFormat::BinaryMask8)?;
        for y in 0..source.height() {
            for x in 0..source.width() {
                let selected = |candidate_x: u32, candidate_y: u32| {
                    matches!(
                        current.pixel(candidate_x, candidate_y),
                        Ok(PixelValue::Binary(255))
                    )
                };
                let value = if pixels > 0 {
                    selected(x, y)
                        || x.checked_sub(1).is_some_and(|left| selected(left, y))
                        || (x + 1 < source.width() && selected(x + 1, y))
                        || y.checked_sub(1).is_some_and(|top| selected(x, top))
                        || (y + 1 < source.height() && selected(x, y + 1))
                } else {
                    selected(x, y)
                        && x > 0
                        && selected(x - 1, y)
                        && x + 1 < source.width()
                        && selected(x + 1, y)
                        && y > 0
                        && selected(x, y - 1)
                        && y + 1 < source.height()
                        && selected(x, y + 1)
                };
                if value {
                    next.set_pixel(x, y, PixelValue::Binary(255), revision)?;
                }
            }
        }
        current = next;
    }
    Ok(current)
}

fn mask_bounds_with_scanned_tiles(mask: &TileRaster) -> Result<(Option<RectI32>, u64), CoreError> {
    bounded_document_pixels(mask.width(), mask.height())?;
    let mut min_x = mask.width();
    let mut min_y = mask.height();
    let mut max_x = 0;
    let mut max_y = 0;
    let mut any = false;
    let mut scanned_tiles = 0_u64;
    for coord in mask.allocated_coords() {
        let tile = mask.tile_view(coord).ok_or(CoreError::InvalidState(
            "allocated selection tile is invalid",
        ))?;
        scanned_tiles = scanned_tiles
            .checked_add(1)
            .ok_or(CoreError::InvalidState("selection tile count overflow"))?;
        let origin_x = coord
            .x
            .checked_mul(TILE_SIZE)
            .ok_or(CoreError::InvalidState("selection tile x-origin overflow"))?;
        let origin_y = coord
            .y
            .checked_mul(TILE_SIZE)
            .ok_or(CoreError::InvalidState("selection tile y-origin overflow"))?;
        let stride = usize::try_from(tile.row_stride_bytes())
            .map_err(|_| CoreError::InvalidState("selection tile stride is invalid"))?;
        let width = usize::try_from(tile.width())
            .map_err(|_| CoreError::InvalidState("selection tile width is invalid"))?;
        for local_y in 0..tile.height() {
            let row_start = usize::try_from(local_y)
                .ok()
                .and_then(|row| row.checked_mul(stride))
                .ok_or(CoreError::InvalidState(
                    "selection tile row offset overflow",
                ))?;
            let row_end = row_start.checked_add(width).ok_or(CoreError::InvalidState(
                "selection tile row length overflow",
            ))?;
            let row = tile
                .bytes()
                .get(row_start..row_end)
                .ok_or(CoreError::InvalidState("selection tile row is truncated"))?;
            let Some(first) = row.iter().position(|value| *value == 255) else {
                continue;
            };
            let last =
                row.iter()
                    .rposition(|value| *value == 255)
                    .ok_or(CoreError::InvalidState(
                        "selected row is missing its final selected pixel",
                    ))?;
            let x = origin_x
                .checked_add(u32::try_from(first).map_err(|_| {
                    CoreError::InvalidState("selection tile x-coordinate is invalid")
                })?)
                .ok_or(CoreError::InvalidState("selection x-coordinate overflow"))?;
            let last_x = origin_x
                .checked_add(u32::try_from(last).map_err(|_| {
                    CoreError::InvalidState("selection tile x-coordinate is invalid")
                })?)
                .ok_or(CoreError::InvalidState("selection x-coordinate overflow"))?;
            let y = origin_y
                .checked_add(local_y)
                .ok_or(CoreError::InvalidState("selection y-coordinate overflow"))?;
            if x < mask.width() && last_x < mask.width() && y < mask.height() {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(last_x);
                max_y = max_y.max(y);
            }
        }
    }
    if !any {
        return Ok((None, scanned_tiles));
    }
    Ok((
        Some(RectI32 {
            x: min_x as i32,
            y: min_y as i32,
            width: (max_x - min_x + 1) as i32,
            height: (max_y - min_y + 1) as i32,
        }),
        scanned_tiles,
    ))
}

pub(crate) fn mask_bounds(mask: &TileRaster) -> Result<Option<RectI32>, CoreError> {
    mask_bounds_with_scanned_tiles(mask).map(|(bounds, _)| bounds)
}

pub(crate) fn color_selection_mask(
    source: &TileRaster,
    color: PixelValue,
    tolerance: u16,
    different: bool,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut output = TileRaster::new(source.width(), source.height(), PixelFormat::BinaryMask8)?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            if pixel_within_tolerance(source.pixel(x, y)?, color, tolerance) != different {
                output.set_pixel(x, y, PixelValue::Binary(255), revision)?;
            }
        }
    }
    Ok(output)
}

pub(crate) fn pixel_within_tolerance(left: PixelValue, right: PixelValue, tolerance: u16) -> bool {
    let channels = |value| -> Option<[u16; 4]> {
        match value {
            PixelValue::Binary(value) | PixelValue::Grayscale8(value) => {
                let value = u16::from(value) * 257;
                Some([value, value, value, u16::MAX])
            }
            PixelValue::Grayscale16(value) => Some([value, value, value, u16::MAX]),
            PixelValue::Rgba(value) => Some(value.map(|channel| u16::from(channel) * 257)),
            PixelValue::Rgba16(value) => Some(value),
        }
    };
    let (Some(left), Some(right)) = (channels(left), channels(right)) else {
        return false;
    };
    left.into_iter()
        .zip(right)
        .all(|(left, right)| left.abs_diff(right) <= tolerance)
}

pub(crate) fn validate_floating_transform(transform: FloatingTransform) -> Result<(), CoreError> {
    if !transform.translate_x.is_finite()
        || !transform.translate_y.is_finite()
        || !transform.scale_x.is_finite()
        || !transform.scale_y.is_finite()
        || !transform.rotation_degrees.is_finite()
        || transform.scale_x.abs() < 0.000_001
        || transform.scale_y.abs() < 0.000_001
        || transform.scale_x.abs() > 1_024.0
        || transform.scale_y.abs() > 1_024.0
        || transform.translate_x.abs() > f64::from(MAX_STROKE_COORDINATE)
        || transform.translate_y.abs() > f64::from(MAX_STROKE_COORDINATE)
        || transform.rotation_degrees.abs() > 36_000.0
    {
        Err(CoreError::InvalidArgument(
            "floating transform is outside supported bounds",
        ))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FloatingSelection {
    pub(crate) payload: ClipboardPayload,
    pub(crate) destination: FloatingDestination,
    pub(crate) transform: FloatingTransform,
    pub(crate) asset_ids: Vec<AssetId>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FloatingDestination {
    ExistingPlanes(Vec<PlaneId>),
    NewPlane {
        layer_id: LayerId,
        kind: PlaneType,
        format: PixelFormat,
        name: String,
        opacity_milli: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(bits: u8) -> TileRaster {
        let mut mask = TileRaster::new(8, 1, PixelFormat::BinaryMask8).unwrap();
        for x in 0..8 {
            if bits & (1 << x) != 0 {
                mask.set_pixel(x, 0, PixelValue::Binary(255), 1).unwrap();
            }
        }
        mask
    }

    #[test]
    fn selection_boolean_property_covers_all_masks() {
        for left in 0_u8..=u8::MAX {
            for right in [0_u8, 0x55, 0xaa, u8::MAX] {
                let left_mask = mask(left);
                let right_mask = mask(right);
                for (operation, expected) in [
                    (SelectionOperation::New, right),
                    (SelectionOperation::Add, left | right),
                    (SelectionOperation::Subtract, left & !right),
                    (SelectionOperation::Intersect, left & right),
                ] {
                    let combined =
                        combine_selection_masks(&left_mask, &right_mask, operation, 2).unwrap();
                    for x in 0..8 {
                        assert_eq!(
                            matches!(combined.pixel(x, 0).unwrap(), PixelValue::Binary(255)),
                            expected & (1 << x) != 0
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn mask_bounds_reads_only_allocated_tiles() {
        let empty = TileRaster::new(4096, 4096, PixelFormat::BinaryMask8).unwrap();
        assert_eq!(mask_bounds_with_scanned_tiles(&empty).unwrap(), (None, 0));

        let mut sparse = empty;
        sparse
            .set_pixel(17, 23, PixelValue::Binary(255), 1)
            .unwrap();
        sparse
            .set_pixel(4095, 4094, PixelValue::Binary(255), 1)
            .unwrap();
        assert_eq!(
            mask_bounds_with_scanned_tiles(&sparse).unwrap(),
            (
                Some(RectI32 {
                    x: 17,
                    y: 23,
                    width: 4079,
                    height: 4072,
                }),
                2,
            )
        );
    }
}
