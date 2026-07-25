use crate::{PixelFormat, PixelValue, RasterError, TileRaster};
use std::collections::VecDeque;
use std::fmt;

pub const MAX_FILL_PIXELS: u64 = 16_777_216;
pub const MAX_GAP_CLOSE: u8 = 64;
pub const MAX_INCLUSION_COLORS: usize = 6;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InclusionMode {
    None,
    Specified,
    ExceptSpecified,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FillOptions {
    /// Maximum per-channel difference in normalized 16-bit sRGB values.
    pub tolerance: u16,
    pub detached_regions: bool,
    pub overflow_abort: bool,
    pub gap_close: u8,
    pub transparent_only: bool,
    pub inclusion_mode: InclusionMode,
    pub inclusion_colors: Vec<PixelValue>,
}

impl Default for FillOptions {
    fn default() -> Self {
        Self {
            tolerance: 0,
            detached_regions: false,
            overflow_abort: false,
            gap_close: 0,
            transparent_only: false,
            inclusion_mode: InclusionMode::None,
            inclusion_colors: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelEdit {
    pub x: u32,
    pub y: u32,
    pub before: PixelValue,
    pub after: PixelValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FillPlan {
    pub edits: Vec<PixelEdit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FillError {
    Raster(RasterError),
    InvalidArgument(&'static str),
    Cancelled,
    WorkLimit,
    Overflow { x: u32, y: u32 },
}

impl fmt::Display for FillError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raster(error) => write!(formatter, "raster error: {error}"),
            Self::InvalidArgument(message) => write!(formatter, "invalid fill argument: {message}"),
            Self::Cancelled => formatter.write_str("fill was cancelled before commit"),
            Self::WorkLimit => formatter.write_str("fill exceeds the bounded work limit"),
            Self::Overflow { x, y } => write!(formatter, "fill reached image edge at ({x}, {y})"),
        }
    }
}

impl std::error::Error for FillError {}

impl From<RasterError> for FillError {
    fn from(error: RasterError) -> Self {
        Self::Raster(error)
    }
}

pub fn seed_fill(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    seed: (u32, u32),
    fill_color: PixelValue,
    options: &FillOptions,
) -> Result<FillPlan, FillError> {
    seed_fill_with_cancel(
        main_line,
        color_plane,
        selection,
        seed,
        fill_color,
        options,
        || false,
    )
}

pub fn seed_fill_with_cancel(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    seed: (u32, u32),
    fill_color: PixelValue,
    options: &FillOptions,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<FillPlan, FillError> {
    let pixel_count = validate_fill_inputs(main_line, color_plane, selection, fill_color, options)?;
    if seed.0 >= color_plane.width() || seed.1 >= color_plane.height() {
        return Err(FillError::InvalidArgument(
            "seed is outside the color plane",
        ));
    }
    let seed_value = color_plane.pixel(seed.0, seed.1)?;
    let mut visited = vec![false; pixel_count];
    let mut edits = Vec::new();
    let mut work = 0_u64;

    let starts: Box<dyn Iterator<Item = (u32, u32)>> = if options.detached_regions {
        Box::new(
            (0..color_plane.height()).flat_map(|y| (0..color_plane.width()).map(move |x| (x, y))),
        )
    } else {
        Box::new(std::iter::once(seed))
    };

    for start in starts {
        let start_index = pixel_index(color_plane.width(), start.0, start.1);
        if visited[start_index]
            || !candidate_pixel(
                main_line,
                color_plane,
                selection,
                start.0,
                start.1,
                seed_value,
                options,
            )?
            || virtual_gap_boundary(
                main_line,
                color_plane,
                selection,
                start.0,
                start.1,
                seed_value,
                options,
            )?
        {
            visited[start_index] = true;
            continue;
        }

        let mut queue = VecDeque::from([start]);
        visited[start_index] = true;
        while let Some((x, y)) = queue.pop_front() {
            work = work.checked_add(1).ok_or(FillError::WorkLimit)?;
            if work > MAX_FILL_PIXELS {
                return Err(FillError::WorkLimit);
            }
            if work % 1_024 == 0 && is_cancelled() {
                return Err(FillError::Cancelled);
            }
            if options.overflow_abort
                && (x == 0
                    || y == 0
                    || x + 1 == color_plane.width()
                    || y + 1 == color_plane.height())
            {
                return Err(FillError::Overflow { x, y });
            }

            let before = color_plane.pixel(x, y)?;
            if before != fill_color {
                edits.push(PixelEdit {
                    x,
                    y,
                    before,
                    after: fill_color,
                });
            }
            for (next_x, next_y) in neighbors(color_plane.width(), color_plane.height(), x, y) {
                let next_index = pixel_index(color_plane.width(), next_x, next_y);
                if visited[next_index] {
                    continue;
                }
                visited[next_index] = true;
                if candidate_pixel(
                    main_line,
                    color_plane,
                    selection,
                    next_x,
                    next_y,
                    seed_value,
                    options,
                )? && !virtual_gap_boundary(
                    main_line,
                    color_plane,
                    selection,
                    next_x,
                    next_y,
                    seed_value,
                    options,
                )? {
                    queue.push_back((next_x, next_y));
                }
            }
        }
    }
    if is_cancelled() {
        return Err(FillError::Cancelled);
    }
    edits.sort_by_key(|edit| (edit.y, edit.x));
    Ok(FillPlan { edits })
}

/// Fills every candidate component wholly enclosed by boundary pixels inside
/// `operation_mask`. Components touching the image edge or escaping the mask
/// through a non-boundary pixel are intentionally left unchanged.
pub fn closed_region_fill(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    operation_mask: &TileRaster,
    fill_color: PixelValue,
    options: &FillOptions,
) -> Result<FillPlan, FillError> {
    closed_region_fill_with_cancel(
        main_line,
        color_plane,
        operation_mask,
        fill_color,
        options,
        || false,
    )
}

pub fn closed_region_fill_with_cancel(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    operation_mask: &TileRaster,
    fill_color: PixelValue,
    options: &FillOptions,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<FillPlan, FillError> {
    let pixel_count = validate_fill_inputs(
        main_line,
        color_plane,
        Some(operation_mask),
        fill_color,
        options,
    )?;
    let mut visited = vec![false; pixel_count];
    let mut edits = Vec::new();
    let mut work = 0_u64;
    for y in 0..color_plane.height() {
        for x in 0..color_plane.width() {
            let index = pixel_index(color_plane.width(), x, y);
            if visited[index] || !selection_contains(operation_mask, x, y)? {
                visited[index] = true;
                continue;
            }
            let target = color_plane.pixel(x, y)?;
            if !candidate_pixel(
                main_line,
                color_plane,
                Some(operation_mask),
                x,
                y,
                target,
                options,
            )? || virtual_gap_boundary(
                main_line,
                color_plane,
                Some(operation_mask),
                x,
                y,
                target,
                options,
            )? {
                visited[index] = true;
                continue;
            }

            let mut component = Vec::new();
            let mut queue = VecDeque::from([(x, y)]);
            let mut escaped = false;
            visited[index] = true;
            while let Some((current_x, current_y)) = queue.pop_front() {
                work = work.checked_add(1).ok_or(FillError::WorkLimit)?;
                if work > MAX_FILL_PIXELS {
                    return Err(FillError::WorkLimit);
                }
                if work % 1_024 == 0 && is_cancelled() {
                    return Err(FillError::Cancelled);
                }
                component.push((current_x, current_y));
                if current_x == 0
                    || current_y == 0
                    || current_x + 1 == color_plane.width()
                    || current_y + 1 == color_plane.height()
                {
                    escaped = true;
                }
                for (next_x, next_y) in neighbors(
                    color_plane.width(),
                    color_plane.height(),
                    current_x,
                    current_y,
                ) {
                    if !selection_contains(operation_mask, next_x, next_y)? {
                        if !hard_boundary(
                            main_line,
                            color_plane,
                            None,
                            next_x,
                            next_y,
                            target,
                            options,
                        )? {
                            escaped = true;
                        }
                        continue;
                    }
                    let next_index = pixel_index(color_plane.width(), next_x, next_y);
                    if visited[next_index] {
                        continue;
                    }
                    if candidate_pixel(
                        main_line,
                        color_plane,
                        Some(operation_mask),
                        next_x,
                        next_y,
                        target,
                        options,
                    )? && !virtual_gap_boundary(
                        main_line,
                        color_plane,
                        Some(operation_mask),
                        next_x,
                        next_y,
                        target,
                        options,
                    )? {
                        visited[next_index] = true;
                        queue.push_back((next_x, next_y));
                    }
                }
            }
            if !escaped {
                for (edit_x, edit_y) in component {
                    let before = color_plane.pixel(edit_x, edit_y)?;
                    if before != fill_color {
                        edits.push(PixelEdit {
                            x: edit_x,
                            y: edit_y,
                            before,
                            after: fill_color,
                        });
                    }
                }
            }
        }
    }
    if is_cancelled() {
        return Err(FillError::Cancelled);
    }
    edits.sort_by_key(|edit| (edit.y, edit.x));
    Ok(FillPlan { edits })
}

/// Extends the seed color through transparent pixels inside `operation_mask`
/// for at most `maximum_distance` four-connected steps.
pub fn extend_fill(
    color_plane: &TileRaster,
    operation_mask: &TileRaster,
    seed: (u32, u32),
    maximum_distance: u32,
) -> Result<FillPlan, FillError> {
    extend_fill_with_cancel(color_plane, operation_mask, seed, maximum_distance, || {
        false
    })
}

pub fn extend_fill_with_cancel(
    color_plane: &TileRaster,
    operation_mask: &TileRaster,
    seed: (u32, u32),
    maximum_distance: u32,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<FillPlan, FillError> {
    validate_selection(color_plane, operation_mask)?;
    if seed.0 >= color_plane.width() || seed.1 >= color_plane.height() {
        return Err(FillError::InvalidArgument(
            "fill-extension seed is outside the plane",
        ));
    }
    let source = color_plane.pixel(seed.0, seed.1)?;
    if source.rgba16().is_none() || source.is_transparent() {
        return Err(FillError::InvalidArgument(
            "fill-extension seed must contain an opaque color",
        ));
    }
    let pixel_count = bounded_pixel_count(color_plane.width(), color_plane.height())?;
    let mut distance = vec![u32::MAX; pixel_count];
    let mut queue = VecDeque::from([seed]);
    distance[pixel_index(color_plane.width(), seed.0, seed.1)] = 0;
    let mut edits = Vec::new();
    let mut work = 0_u64;
    while let Some((x, y)) = queue.pop_front() {
        work = work.checked_add(1).ok_or(FillError::WorkLimit)?;
        if work > MAX_FILL_PIXELS {
            return Err(FillError::WorkLimit);
        }
        if work % 1_024 == 0 && is_cancelled() {
            return Err(FillError::Cancelled);
        }
        let current_distance = distance[pixel_index(color_plane.width(), x, y)];
        if current_distance >= maximum_distance {
            continue;
        }
        for (next_x, next_y) in neighbors(color_plane.width(), color_plane.height(), x, y) {
            let index = pixel_index(color_plane.width(), next_x, next_y);
            if distance[index] != u32::MAX || !selection_contains(operation_mask, next_x, next_y)? {
                continue;
            }
            let before = color_plane.pixel(next_x, next_y)?;
            if !before.is_transparent() {
                continue;
            }
            distance[index] = current_distance + 1;
            edits.push(PixelEdit {
                x: next_x,
                y: next_y,
                before,
                after: source,
            });
            queue.push_back((next_x, next_y));
        }
    }
    if is_cancelled() {
        return Err(FillError::Cancelled);
    }
    edits.sort_by_key(|edit| (edit.y, edit.x));
    Ok(FillPlan { edits })
}

fn validate_fill_inputs(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    fill_color: PixelValue,
    options: &FillOptions,
) -> Result<usize, FillError> {
    if main_line.width() != color_plane.width() || main_line.height() != color_plane.height() {
        return Err(FillError::InvalidArgument(
            "main-line and color-plane dimensions differ",
        ));
    }
    if !matches!(
        main_line.format(),
        PixelFormat::BinaryMask8 | PixelFormat::Grayscale8 | PixelFormat::Grayscale16
    ) || !matches!(
        color_plane.format(),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
    ) {
        return Err(FillError::InvalidArgument(
            "plane formats do not support coloring fill",
        ));
    }
    validate_color_for_format(color_plane.format(), fill_color)?;
    if options.gap_close > MAX_GAP_CLOSE {
        return Err(FillError::InvalidArgument(
            "gap-close value exceeds its bounded limit",
        ));
    }
    if options.inclusion_colors.len() > MAX_INCLUSION_COLORS {
        return Err(FillError::InvalidArgument("too many inclusion colors"));
    }
    if options
        .inclusion_colors
        .iter()
        .any(|color| color.rgba16().is_none())
    {
        return Err(FillError::InvalidArgument("inclusion color is not RGBA"));
    }
    if let Some(mask) = selection {
        validate_selection(color_plane, mask)?;
    }
    bounded_pixel_count(color_plane.width(), color_plane.height())
}

fn validate_selection(color_plane: &TileRaster, selection: &TileRaster) -> Result<(), FillError> {
    if selection.width() != color_plane.width()
        || selection.height() != color_plane.height()
        || selection.format() != PixelFormat::BinaryMask8
    {
        return Err(FillError::InvalidArgument(
            "selection must be a same-sized binary mask",
        ));
    }
    Ok(())
}

fn bounded_pixel_count(width: u32, height: u32) -> Result<usize, FillError> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(FillError::WorkLimit)?;
    if pixels > MAX_FILL_PIXELS {
        return Err(FillError::WorkLimit);
    }
    usize::try_from(pixels).map_err(|_| FillError::WorkLimit)
}

fn validate_color_for_format(format: PixelFormat, color: PixelValue) -> Result<(), FillError> {
    if matches!(
        (format, color),
        (PixelFormat::StraightRgba8, PixelValue::Rgba(_))
            | (PixelFormat::StraightRgba16, PixelValue::Rgba16(_))
    ) {
        Ok(())
    } else {
        Err(FillError::InvalidArgument(
            "fill color depth does not match the color plane",
        ))
    }
}

fn pixel_index(width: u32, x: u32, y: u32) -> usize {
    y as usize * width as usize + x as usize
}

fn neighbors(width: u32, height: u32, x: u32, y: u32) -> impl Iterator<Item = (u32, u32)> {
    let mut values = [(0, 0); 4];
    let mut count = 0;
    if x > 0 {
        values[count] = (x - 1, y);
        count += 1;
    }
    if x + 1 < width {
        values[count] = (x + 1, y);
        count += 1;
    }
    if y > 0 {
        values[count] = (x, y - 1);
        count += 1;
    }
    if y + 1 < height {
        values[count] = (x, y + 1);
        count += 1;
    }
    values.into_iter().take(count)
}

fn selection_contains(selection: &TileRaster, x: u32, y: u32) -> Result<bool, FillError> {
    Ok(matches!(selection.pixel(x, y)?, PixelValue::Binary(255)))
}

fn candidate_pixel(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    x: u32,
    y: u32,
    target: PixelValue,
    options: &FillOptions,
) -> Result<bool, FillError> {
    Ok(!hard_boundary(
        main_line,
        color_plane,
        selection,
        x,
        y,
        target,
        options,
    )?)
}

fn hard_boundary(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    x: u32,
    y: u32,
    target: PixelValue,
    options: &FillOptions,
) -> Result<bool, FillError> {
    if let Some(mask) = selection
        && !selection_contains(mask, x, y)?
    {
        return Ok(true);
    }
    // Binary main lines participate in legacy topology. Grayscale coverage is
    // display-only; a broken color-plane trace must still leak by specification.
    if matches!(main_line.pixel(x, y)?, PixelValue::Binary(255)) {
        return Ok(true);
    }
    let value = color_plane.pixel(x, y)?;
    let included = inclusion_matches(value, options);
    if options.transparent_only && !value.is_transparent() && !included {
        return Ok(true);
    }
    Ok(!within_tolerance(value, target, options.tolerance) && !included)
}

fn inclusion_matches(value: PixelValue, options: &FillOptions) -> bool {
    if value.is_transparent() {
        return false;
    }
    let listed = options
        .inclusion_colors
        .iter()
        .any(|color| within_tolerance(value, *color, options.tolerance));
    match options.inclusion_mode {
        InclusionMode::None => false,
        InclusionMode::Specified => listed,
        InclusionMode::ExceptSpecified => !listed,
    }
}

fn within_tolerance(left: PixelValue, right: PixelValue, tolerance: u16) -> bool {
    let (Some(left), Some(right)) = (left.rgba16(), right.rgba16()) else {
        return left == right;
    };
    left.into_iter()
        .zip(right)
        .all(|(left, right)| left.abs_diff(right) <= tolerance)
}

fn virtual_gap_boundary(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    x: u32,
    y: u32,
    target: PixelValue,
    options: &FillOptions,
) -> Result<bool, FillError> {
    if options.gap_close == 0
        || hard_boundary(main_line, color_plane, selection, x, y, target, options)?
    {
        return Ok(false);
    }
    let maximum = u32::from(options.gap_close) + 1;
    for ((negative_x, negative_y), (positive_x, positive_y)) in
        [((-1_i32, 0_i32), (1_i32, 0_i32)), ((0, -1), (0, 1))]
    {
        let negative = nearest_boundary_distance(
            main_line,
            color_plane,
            selection,
            x,
            y,
            negative_x,
            negative_y,
            maximum,
            target,
            options,
        )?;
        let positive = nearest_boundary_distance(
            main_line,
            color_plane,
            selection,
            x,
            y,
            positive_x,
            positive_y,
            maximum,
            target,
            options,
        )?;
        if let (Some(negative), Some(positive)) = (negative, positive)
            && negative + positive - 1 <= u32::from(options.gap_close)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn nearest_boundary_distance(
    main_line: &TileRaster,
    color_plane: &TileRaster,
    selection: Option<&TileRaster>,
    x: u32,
    y: u32,
    delta_x: i32,
    delta_y: i32,
    maximum: u32,
    target: PixelValue,
    options: &FillOptions,
) -> Result<Option<u32>, FillError> {
    for distance in 1..=maximum {
        let next_x = i64::from(x) + i64::from(delta_x) * i64::from(distance);
        let next_y = i64::from(y) + i64::from(delta_y) * i64::from(distance);
        if next_x < 0
            || next_y < 0
            || next_x >= i64::from(color_plane.width())
            || next_y >= i64::from(color_plane.height())
        {
            return Ok(None);
        }
        if hard_boundary(
            main_line,
            color_plane,
            selection,
            next_x as u32,
            next_y as u32,
            target,
            options,
        )? {
            return Ok(Some(distance));
        }
    }
    Ok(None)
}
