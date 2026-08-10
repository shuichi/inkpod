use super::*;
use inkpod_image::{
    CANONICAL_DOCUMENT_ONE, canonical_q16_from_f32, div_round_ties_even_i128,
    interpret_raster_selection, rotate_q16,
};

pub(crate) fn selection_mask_for_shape(
    document: &CellDocument,
    active_plane_id: PlaneId,
    shape: &SelectionShape,
    interpretation: RangeInterpretation,
    options: SelectionConstructionOptions,
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
            mask_oriented_geometry(
                document,
                &mut mask,
                rect_geometry(*rect, options)?,
                true,
                revision,
            )?;
        }
        SelectionShape::RectangleGesture { anchor, current } => {
            mask_oriented_geometry(
                document,
                &mut mask,
                gesture_geometry(*anchor, *current, options)?,
                false,
                revision,
            )?;
        }
        SelectionShape::EllipseGesture { anchor, current } => {
            mask_oriented_geometry(
                document,
                &mut mask,
                gesture_geometry(*anchor, *current, options)?,
                true,
                revision,
            )?;
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
        SelectionShape::TraceBrush { samples, diameter } => {
            validate_selection_samples(samples)?;
            let diameter = effective_trace_diameter(*diameter, options.trace)?;
            for y in 0..document.height {
                for x in 0..document.width {
                    let px = f64::from(x) + 0.5;
                    let py = f64::from(y) + 0.5;
                    if trace_brush_contains(px, py, samples, diameter, options.trace) {
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
    let source = document
        .plane_by_id(active_plane_id)
        .ok_or(CoreError::InvalidState("active plane is missing"))?;
    interpret_raster_selection(&source.raster, &mask, interpretation, revision)
        .map_err(CoreError::Raster)
}

#[derive(Clone, Copy)]
struct OrientedGeometry {
    center_x_q16: i64,
    center_y_q16: i64,
    half_width_q16: i64,
    half_height_q16: i64,
    rotation_turns: u32,
}

fn rect_geometry(
    rect: RectI32,
    options: SelectionConstructionOptions,
) -> Result<OrientedGeometry, CoreError> {
    if rect.width <= 0 || rect.height <= 0 {
        return Err(CoreError::InvalidArgument("selection bounds are empty"));
    }
    let one = CANONICAL_DOCUMENT_ONE;
    let center_x_q16 = i64::from(rect.x)
        .checked_mul(one)
        .and_then(|value| value.checked_add(i64::from(rect.width) * one / 2))
        .ok_or(CoreError::InvalidArgument("selection X bounds overflow"))?;
    let center_y_q16 = i64::from(rect.y)
        .checked_mul(one)
        .and_then(|value| value.checked_add(i64::from(rect.height) * one / 2))
        .ok_or(CoreError::InvalidArgument("selection Y bounds overflow"))?;
    apply_geometry_options(
        center_x_q16,
        center_y_q16,
        i64::from(rect.width) * one / 2,
        i64::from(rect.height) * one / 2,
        options,
    )
}

fn gesture_geometry(
    anchor: PointF32,
    current: PointF32,
    options: SelectionConstructionOptions,
) -> Result<OrientedGeometry, CoreError> {
    let anchor_x = canonical_q16_from_f32(anchor.x)
        .ok_or(CoreError::InvalidArgument("selection anchor X is invalid"))?;
    let anchor_y = canonical_q16_from_f32(anchor.y)
        .ok_or(CoreError::InvalidArgument("selection anchor Y is invalid"))?;
    let current_x = canonical_q16_from_f32(current.x)
        .ok_or(CoreError::InvalidArgument("selection current X is invalid"))?;
    let current_y = canonical_q16_from_f32(current.y)
        .ok_or(CoreError::InvalidArgument("selection current Y is invalid"))?;
    let delta_x = current_x
        .checked_sub(anchor_x)
        .ok_or(CoreError::InvalidArgument(
            "selection horizontal delta overflow",
        ))?;
    let delta_y = current_y
        .checked_sub(anchor_y)
        .ok_or(CoreError::InvalidArgument(
            "selection vertical delta overflow",
        ))?;
    let (center_x, center_y, half_width, half_height) = if options.from_center {
        (
            anchor_x,
            anchor_y,
            delta_x.unsigned_abs() as i64,
            delta_y.unsigned_abs() as i64,
        )
    } else {
        (
            anchor_x + delta_x / 2,
            anchor_y + delta_y / 2,
            (delta_x.unsigned_abs() / 2) as i64,
            (delta_y.unsigned_abs() / 2) as i64,
        )
    };
    apply_geometry_options(center_x, center_y, half_width, half_height, options)
}

fn apply_geometry_options(
    center_x_q16: i64,
    center_y_q16: i64,
    mut half_width_q16: i64,
    mut half_height_q16: i64,
    options: SelectionConstructionOptions,
) -> Result<OrientedGeometry, CoreError> {
    if half_width_q16 <= 0 || half_height_q16 <= 0 {
        return Err(CoreError::InvalidArgument("selection bounds are empty"));
    }
    if options.aspect_ratio_q16 > (4_096_u32 << 16) {
        return Err(CoreError::InvalidArgument(
            "selection aspect ratio is invalid",
        ));
    }
    if options.aspect_ratio_q16 != 0 {
        let desired_width = div_round_ties_even_i128(
            i128::from(half_height_q16) * i128::from(options.aspect_ratio_q16),
            i128::from(1_u32 << 16),
        )
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(CoreError::InvalidArgument(
            "selection aspect ratio overflowed",
        ))?;
        if desired_width >= half_width_q16 {
            half_width_q16 = desired_width;
        } else {
            half_height_q16 = div_round_ties_even_i128(
                i128::from(half_width_q16) * i128::from(1_u32 << 16),
                i128::from(options.aspect_ratio_q16),
            )
            .and_then(|value| i64::try_from(value).ok())
            .ok_or(CoreError::InvalidArgument(
                "selection aspect ratio overflowed",
            ))?;
        }
    }
    let rotation_turns = if options.constrain_rotation_45 {
        const TURN: u64 = UINT32_CYCLE;
        let step = TURN / 8;
        (((u64::from(options.rotation_turns) + step / 2) / step * step) % TURN) as u32
    } else {
        options.rotation_turns
    };
    Ok(OrientedGeometry {
        center_x_q16,
        center_y_q16,
        half_width_q16,
        half_height_q16,
        rotation_turns,
    })
}

const UINT32_CYCLE: u64 = 1_u64 << 32;

fn mask_oriented_geometry(
    document: &CellDocument,
    mask: &mut TileRaster,
    geometry: OrientedGeometry,
    ellipse: bool,
    revision: u64,
) -> Result<(), CoreError> {
    let one = CANONICAL_DOCUMENT_ONE;
    let half_w = i128::from(geometry.half_width_q16);
    let half_h = i128::from(geometry.half_height_q16);
    for y in 0..document.height {
        for x in 0..document.width {
            let pixel_x = i64::from(x) * one + one / 2 - geometry.center_x_q16;
            let pixel_y = i64::from(y) * one + one / 2 - geometry.center_y_q16;
            let (local_x, local_y) =
                rotate_q16(pixel_x, pixel_y, geometry.rotation_turns.wrapping_neg())
                    .ok_or(CoreError::InvalidArgument("selection rotation overflowed"))?;
            let inside = if ellipse {
                let x2 = i128::from(local_x) * i128::from(local_x);
                let y2 = i128::from(local_y) * i128::from(local_y);
                x2 * half_h * half_h + y2 * half_w * half_w <= half_w * half_w * half_h * half_h
            } else {
                i128::from(local_x).abs() <= half_w && i128::from(local_y).abs() <= half_h
            };
            if inside {
                mask.set_pixel(x, y, PixelValue::Binary(255), revision)?;
            }
        }
    }
    Ok(())
}

fn validate_selection_samples(samples: &[SelectionSample]) -> Result<(), CoreError> {
    if samples.is_empty()
        || samples.len() > 1_048_576
        || samples.iter().any(|sample| {
            !sample.x.is_finite()
                || !sample.y.is_finite()
                || !sample.pressure.is_finite()
                || !(0.0..=1.0).contains(&sample.pressure)
                || sample.x.abs() > MAX_STROKE_COORDINATE
                || sample.y.abs() > MAX_STROKE_COORDINATE
        })
    {
        Err(CoreError::InvalidArgument(
            "selection sample list is invalid",
        ))
    } else {
        Ok(())
    }
}

fn effective_trace_diameter(diameter: f32, options: TraceBrushOptions) -> Result<f64, CoreError> {
    if !diameter.is_finite() || diameter <= 0.0 || diameter > 4_096.0 {
        return Err(CoreError::InvalidArgument("trace diameter is invalid"));
    }
    if options.screen_size {
        if options.view_zoom_q16 <= 0 {
            return Err(CoreError::InvalidArgument("trace view zoom is invalid"));
        }
        let zoom = options.view_zoom_q16 as f64 / 65_536.0;
        Ok(f64::from(diameter) / zoom)
    } else {
        Ok(f64::from(diameter))
    }
}

fn trace_brush_contains(
    x: f64,
    y: f64,
    samples: &[SelectionSample],
    diameter: f64,
    options: TraceBrushOptions,
) -> bool {
    let radius = |pressure: f32| {
        diameter
            * if options.pressure_size {
                f64::from(pressure)
            } else {
                1.0
            }
            / 2.0
    };
    let contains_stamp = |sample: SelectionSample| {
        let dx = (x - f64::from(sample.x)).abs();
        let dy = (y - f64::from(sample.y)).abs();
        let r = radius(sample.pressure);
        match options.shape {
            TraceBrushShape::Round => dx.mul_add(dx, dy * dy) <= r * r,
            TraceBrushShape::Square => dx.max(dy) <= r,
        }
    };
    if samples.iter().copied().any(contains_stamp) {
        return true;
    }
    samples.windows(2).any(|pair| {
        let start = PointF32 {
            x: pair[0].x,
            y: pair[0].y,
        };
        let end = PointF32 {
            x: pair[1].x,
            y: pair[1].y,
        };
        let start_x = f64::from(start.x);
        let start_y = f64::from(start.y);
        let delta_x = f64::from(end.x) - start_x;
        let delta_y = f64::from(end.y) - start_y;
        let length_squared = delta_x.mul_add(delta_x, delta_y * delta_y);
        let ratio = if length_squared == 0.0 {
            0.0
        } else {
            (((x - start_x) * delta_x + (y - start_y) * delta_y) / length_squared).clamp(0.0, 1.0)
        };
        let nearest_x = start_x + ratio * delta_x;
        let nearest_y = start_y + ratio * delta_y;
        let pressure =
            f64::from(pair[0].pressure) + ratio * f64::from(pair[1].pressure - pair[0].pressure);
        let r = diameter * if options.pressure_size { pressure } else { 1.0 } / 2.0;
        let dx = (x - nearest_x).abs();
        let dy = (y - nearest_y).abs();
        match options.shape {
            TraceBrushShape::Round => dx.mul_add(dx, dy * dy) <= r * r,
            TraceBrushShape::Square => dx.max(dy) <= r,
        }
    })
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
