use super::geometry::*;
use super::model::*;
use super::*;
use crate::ScopedColorReplaceMode;
use inkpod_image::MAX_IMAGE_EDIT_PIXELS;

pub(crate) fn scoped_vector_color_replace_matches(
    document: &CellDocument,
    plane_id: PlaneId,
    mode: ScopedColorReplaceMode,
    target: PixelValue,
    mask: Option<&TileRaster>,
    bounds: Option<RectI32>,
) -> Result<Vec<u64>, CoreError> {
    let Some(bounds) = bounds else {
        return Ok(Vec::new());
    };
    let mut work = 0_u64;
    match mode {
        ScopedColorReplaceMode::VectorColorLine | ScopedColorReplaceMode::VectorMainLine => {
            let mut matches = Vec::new();
            for path in document
                .vector
                .paths
                .iter()
                .filter(|path| path.plane_id == plane_id && path.color == target)
            {
                if path_touches_region(path, mask, bounds, &mut work)? {
                    matches.push(path.id.get());
                }
            }
            Ok(matches)
        }
        ScopedColorReplaceMode::VectorFill => {
            let mut matches = Vec::new();
            for fill in document
                .vector
                .fills
                .iter()
                .filter(|fill| fill.plane_id == plane_id && fill.color == target)
            {
                if fill_touches_region(document, fill, mask, bounds, &mut work)? {
                    matches.push(fill.id.get());
                }
            }
            Ok(matches)
        }
        ScopedColorReplaceMode::RasterColor | ScopedColorReplaceMode::RasterMainLine => {
            Err(CoreError::InvalidArgument(
                "raster mode cannot be evaluated as vector color replacement",
            ))
        }
    }
}

pub(crate) fn apply_scoped_vector_color_replace(
    document: &mut CellDocument,
    mode: ScopedColorReplaceMode,
    matches: &[u64],
    replacement: PixelValue,
) {
    let matches = matches.iter().copied().collect::<BTreeSet<_>>();
    match mode {
        ScopedColorReplaceMode::VectorColorLine | ScopedColorReplaceMode::VectorMainLine => {
            for path in &mut document.vector.paths {
                if matches.contains(&path.id.get()) {
                    path.color = replacement;
                }
            }
        }
        ScopedColorReplaceMode::VectorFill => {
            for fill in &mut document.vector.fills {
                if matches.contains(&fill.id.get()) {
                    fill.color = replacement;
                }
            }
        }
        ScopedColorReplaceMode::RasterColor | ScopedColorReplaceMode::RasterMainLine => {}
    }
}

fn path_touches_region(
    path: &VectorPath,
    mask: Option<&TileRaster>,
    region: RectI32,
    work: &mut u64,
) -> Result<bool, CoreError> {
    for pair in flatten_path(path, FLATTEN_STEPS).windows(2) {
        let radius = pair[0].width.max(pair[1].width) * 0.5;
        let left = ((pair[0].point.0.min(pair[1].point.0) - radius).floor() as i32).max(region.x);
        let top = ((pair[0].point.1.min(pair[1].point.1) - radius).floor() as i32).max(region.y);
        let right = ((pair[0].point.0.max(pair[1].point.0) + radius).ceil() as i32)
            .min(region.x + region.width);
        let bottom = ((pair[0].point.1.max(pair[1].point.1) + radius).ceil() as i32)
            .min(region.y + region.height);
        for y in top.max(0)..bottom.max(0) {
            for x in left.max(0)..right.max(0) {
                bounded_work(work)?;
                if region_contains(mask, x as u32, y as u32)?
                    && variable_stroke_segment_intersects_cell(pair, x, y)
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn fill_touches_region(
    document: &CellDocument,
    fill: &VectorFill,
    mask: Option<&TileRaster>,
    region: RectI32,
    work: &mut u64,
) -> Result<bool, CoreError> {
    let boundaries = fill
        .boundary_path_ids
        .iter()
        .filter_map(|path_id| {
            document
                .vector
                .paths
                .iter()
                .find(|path| path.id == *path_id)
        })
        .map(|path| flatten_path(path, RASTER_STEPS))
        .collect::<Vec<_>>();
    for y in region.y.max(0)..(region.y + region.height).max(0) {
        for x in region.x.max(0)..(region.x + region.width).max(0) {
            if region_contains(mask, x as u32, y as u32)?
                && sampled_fill_intersects_cell(&boundaries, x, y, work)?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn sampled_fill_intersects_cell(
    boundaries: &[Vec<FlatSample>],
    x: i32,
    y: i32,
    work: &mut u64,
) -> Result<bool, CoreError> {
    let left = f64::from(x);
    let top = f64::from(y);
    let right = left + 1.0;
    let bottom = top + 1.0;
    let inset = 0.000_25;
    let samples = [
        (left + 0.5, top + 0.5),
        (left + inset, top + inset),
        (right - inset, top + inset),
        (right - inset, bottom - inset),
        (left + inset, bottom - inset),
    ];
    for point in samples {
        let mut inside = false;
        for boundary in boundaries {
            for pair in boundary.windows(2) {
                bounded_work(work)?;
                let (a, b) = (pair[0].point, pair[1].point);
                if (a.1 > point.1) != (b.1 > point.1)
                    && point.0 < (b.0 - a.0) * (point.1 - a.1) / (b.1 - a.1) + a.0
                {
                    inside = !inside;
                }
            }
        }
        if inside {
            return Ok(true);
        }
    }
    for boundary in boundaries {
        for pair in boundary.windows(2) {
            bounded_work(work)?;
            if segment_enters_open_cell(pair[0].point, pair[1].point, (left, top, right, bottom)) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn segment_enters_open_cell(
    start: (f64, f64),
    end: (f64, f64),
    cell: (f64, f64, f64, f64),
) -> bool {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let mut enter = 0.0_f64;
    let mut exit = 1.0_f64;
    for (direction, distance) in [
        (-dx, start.0 - cell.0),
        (dx, cell.2 - start.0),
        (-dy, start.1 - cell.1),
        (dy, cell.3 - start.1),
    ] {
        if direction == 0.0 {
            if distance < 0.0 {
                return false;
            }
            continue;
        }
        let amount = distance / direction;
        if direction < 0.0 {
            enter = enter.max(amount);
        } else {
            exit = exit.min(amount);
        }
        if enter > exit {
            return false;
        }
    }
    let amount = (enter + exit) * 0.5;
    let point = (lerp(start.0, end.0, amount), lerp(start.1, end.1, amount));
    point.0 > cell.0 && point.0 < cell.2 && point.1 > cell.1 && point.1 < cell.3
}

fn region_contains(mask: Option<&TileRaster>, x: u32, y: u32) -> Result<bool, CoreError> {
    match mask {
        Some(mask) => Ok(mask.pixel(x, y)? == PixelValue::Binary(255)),
        None => Ok(true),
    }
}

fn bounded_work(work: &mut u64) -> Result<(), CoreError> {
    *work = work.checked_add(1).ok_or(CoreError::InvalidArgument(
        "scoped vector color replacement work overflows",
    ))?;
    if *work > MAX_IMAGE_EDIT_PIXELS {
        return Err(CoreError::InvalidArgument(
            "scoped vector color replacement exceeds the bounded work limit",
        ));
    }
    Ok(())
}

fn variable_stroke_segment_intersects_cell(pair: &[FlatSample], x: i32, y: i32) -> bool {
    let score = |amount: f64| {
        let point = (
            lerp(pair[0].point.0, pair[1].point.0, amount),
            lerp(pair[0].point.1, pair[1].point.1, amount),
        );
        let width = lerp(pair[0].width, pair[1].width, amount);
        distance_to_cell(point, x, y) - width * 0.5
    };
    let mut left = 0.0;
    let mut right = 1.0;
    for _ in 0..32 {
        let one_third = left + (right - left) / 3.0;
        let two_thirds = right - (right - left) / 3.0;
        if score(one_third) <= score(two_thirds) {
            right = two_thirds;
        } else {
            left = one_third;
        }
    }
    score(0.0).min(score(1.0)).min(score((left + right) * 0.5)) <= 0.0
}

fn distance_to_cell(point: (f64, f64), x: i32, y: i32) -> f64 {
    let left = f64::from(x);
    let top = f64::from(y);
    let right = left + 1.0;
    let bottom = top + 1.0;
    let dx = if point.0 < left {
        left - point.0
    } else if point.0 > right {
        point.0 - right
    } else {
        0.0
    };
    let dy = if point.1 < top {
        top - point.1
    } else if point.1 > bottom {
        point.1 - bottom
    } else {
        0.0
    };
    canonical_sqrt(dx * dx + dy * dy)
}

fn canonical_sqrt(value: f64) -> f64 {
    if value == 0.0 {
        return 0.0;
    }
    let mut estimate = if value >= 1.0 { value } else { 1.0 };
    for _ in 0..32 {
        estimate = 0.5 * (estimate + value / estimate);
    }
    estimate
}
