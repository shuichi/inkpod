use super::*;

pub(crate) fn selection_mask_for_shape(
    document: &CellDocument,
    active_plane_id: PlaneId,
    shape: &SelectionShape,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(document.width, document.height)?;
    let mut mask = TileRaster::new(document.width, document.height, PixelFormat::BinaryMask8)?;
    match shape {
        SelectionShape::Rectangle(rect) => {
            let clipped = clip_rect(*rect, document.width, document.height)?;
            for y in clipped.y..clipped.y + clipped.height {
                for x in clipped.x..clipped.x + clipped.width {
                    mask.set_pixel(x as u32, y as u32, PixelValue::Binary(255), revision)?;
                }
            }
        }
        SelectionShape::Ellipse(rect) => {
            let clipped = clip_rect(*rect, document.width, document.height)?;
            if rect.width <= 0 || rect.height <= 0 {
                return Err(CoreError::InvalidArgument("ellipse bounds are empty"));
            }
            let center_x = f64::from(rect.x) + f64::from(rect.width) / 2.0;
            let center_y = f64::from(rect.y) + f64::from(rect.height) / 2.0;
            let radius_x = f64::from(rect.width) / 2.0;
            let radius_y = f64::from(rect.height) / 2.0;
            for y in clipped.y..clipped.y + clipped.height {
                for x in clipped.x..clipped.x + clipped.width {
                    let normalized_x = (f64::from(x) + 0.5 - center_x) / radius_x;
                    let normalized_y = (f64::from(y) + 0.5 - center_y) / radius_y;
                    if normalized_x * normalized_x + normalized_y * normalized_y <= 1.0 {
                        mask.set_pixel(x as u32, y as u32, PixelValue::Binary(255), revision)?;
                    }
                }
            }
        }
        SelectionShape::Lasso(points) | SelectionShape::Polyline(points) => {
            validate_points(points, 3)?;
            for y in 0..document.height {
                for x in 0..document.width {
                    if point_in_polygon(f64::from(x) + 0.5, f64::from(y) + 0.5, points) {
                        mask.set_pixel(x, y, PixelValue::Binary(255), revision)?;
                    }
                }
            }
        }
        SelectionShape::Trace { points, diameter } => {
            validate_points(points, 1)?;
            if !diameter.is_finite() || *diameter <= 0.0 || *diameter > 4_096.0 {
                return Err(CoreError::InvalidArgument("trace diameter is invalid"));
            }
            let radius_squared = f64::from(*diameter) * f64::from(*diameter) / 4.0;
            for y in 0..document.height {
                for x in 0..document.width {
                    let px = f64::from(x) + 0.5;
                    let py = f64::from(y) + 0.5;
                    let selected = if points.len() == 1 {
                        distance_squared(px, py, f64::from(points[0].x), f64::from(points[0].y))
                            <= radius_squared
                    } else {
                        points.windows(2).any(|segment| {
                            distance_to_segment_squared(px, py, segment[0], segment[1])
                                <= radius_squared
                        })
                    };
                    if selected {
                        mask.set_pixel(x, y, PixelValue::Binary(255), revision)?;
                    }
                }
            }
        }
        SelectionShape::Wand {
            x,
            y,
            tolerance,
            gap_close,
        } => {
            if *x >= document.width || *y >= document.height || *gap_close > 64 {
                return Err(CoreError::InvalidArgument("wand settings are invalid"));
            }
            let source = document
                .plane_by_id(active_plane_id)
                .ok_or(CoreError::InvalidState("active plane is missing"))?;
            let target = source.raster.pixel(*x, *y)?;
            let mut visited = BTreeSet::new();
            let mut queue = VecDeque::from([(*x, *y)]);
            while let Some((candidate_x, candidate_y)) = queue.pop_front() {
                if !visited.insert((candidate_x, candidate_y)) {
                    continue;
                }
                let value = source.raster.pixel(candidate_x, candidate_y)?;
                if !pixel_within_tolerance(value, target, *tolerance) {
                    continue;
                }
                mask.set_pixel(candidate_x, candidate_y, PixelValue::Binary(255), revision)?;
                if candidate_x > 0 {
                    queue.push_back((candidate_x - 1, candidate_y));
                }
                if candidate_x + 1 < document.width {
                    queue.push_back((candidate_x + 1, candidate_y));
                }
                if candidate_y > 0 {
                    queue.push_back((candidate_x, candidate_y - 1));
                }
                if candidate_y + 1 < document.height {
                    queue.push_back((candidate_x, candidate_y + 1));
                }
            }
            if *gap_close > 0 {
                mask = morphology_selection(&mask, i32::from(*gap_close), revision)?;
                mask = morphology_selection(&mask, -i32::from(*gap_close), revision)?;
            }
        }
    }
    Ok(mask)
}

pub(crate) fn clip_rect(rect: RectI32, width: u32, height: u32) -> Result<RectI32, CoreError> {
    if rect.width <= 0 || rect.height <= 0 {
        return Err(CoreError::InvalidArgument("selection bounds are empty"));
    }
    let right = rect
        .x
        .checked_add(rect.width)
        .ok_or(CoreError::InvalidArgument("selection X bounds overflow"))?;
    let bottom = rect
        .y
        .checked_add(rect.height)
        .ok_or(CoreError::InvalidArgument("selection Y bounds overflow"))?;
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = right.min(width as i32);
    let bottom = bottom.min(height as i32);
    if left >= right || top >= bottom {
        return Err(CoreError::InvalidArgument(
            "selection is outside the document",
        ));
    }
    Ok(RectI32 {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

pub(crate) fn validate_points(points: &[PointF32], minimum: usize) -> Result<(), CoreError> {
    if points.len() < minimum
        || points.len() > 1_048_576
        || points.iter().any(|point| {
            !point.x.is_finite()
                || !point.y.is_finite()
                || point.x.abs() > MAX_STROKE_COORDINATE
                || point.y.abs() > MAX_STROKE_COORDINATE
        })
    {
        Err(CoreError::InvalidArgument(
            "selection point list is invalid",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn point_in_polygon(x: f64, y: f64, points: &[PointF32]) -> bool {
    let mut inside = false;
    let mut previous = points[points.len() - 1];
    for &current in points {
        let (x1, y1) = (f64::from(previous.x), f64::from(previous.y));
        let (x2, y2) = (f64::from(current.x), f64::from(current.y));
        if (y1 > y) != (y2 > y) {
            let crossing_x = (x2 - x1).mul_add((y - y1) / (y2 - y1), x1);
            if x < crossing_x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

pub(crate) fn distance_squared(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    (x1 - x2).mul_add(x1 - x2, (y1 - y2) * (y1 - y2))
}

pub(crate) fn distance_to_segment_squared(x: f64, y: f64, start: PointF32, end: PointF32) -> f64 {
    let start_x = f64::from(start.x);
    let start_y = f64::from(start.y);
    let delta_x = f64::from(end.x) - start_x;
    let delta_y = f64::from(end.y) - start_y;
    let length_squared = delta_x.mul_add(delta_x, delta_y * delta_y);
    if length_squared == 0.0 {
        return distance_squared(x, y, start_x, start_y);
    }
    let ratio =
        (((x - start_x) * delta_x + (y - start_y) * delta_y) / length_squared).clamp(0.0, 1.0);
    distance_squared(x, y, start_x + ratio * delta_x, start_y + ratio * delta_y)
}
