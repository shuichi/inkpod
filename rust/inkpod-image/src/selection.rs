//! Deterministic raster-content interpretation for selection masks.

use std::collections::VecDeque;

use crate::{PixelFormat, PixelValue, RasterError, TileRaster};

/// Meaning applied to a geometric candidate when raster content is interpreted.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u32)]
pub enum RasterRangeInterpretation {
    /// Uses the geometric candidate without reading source pixels.
    #[default]
    Normal = 1,
    /// Removes uncovered pixels reachable from the candidate's outer edge.
    Tight = 2,
    /// Selects only uncovered pixels enclosed by covered source pixels.
    EnclosedInterior = 3,
    /// Selects covered source pixels inside the candidate.
    Drawing = 4,
    /// Selects covered source pixels adjacent to uncovered pixels or the paper edge.
    Boundary = 5,
}

/// Applies a content interpretation to a binary geometric candidate.
///
/// Coverage is nonzero for binary/grayscale pixels and nonzero alpha for straight
/// RGBA pixels. The search is iterative and 4-connected. Work and allocation are
/// bounded by the already validated raster dimensions; failures return without
/// modifying either input.
pub fn interpret_raster_selection(
    source: &TileRaster,
    candidate: &TileRaster,
    interpretation: RasterRangeInterpretation,
    revision: u64,
) -> Result<TileRaster, RasterError> {
    if source.width() != candidate.width()
        || source.height() != candidate.height()
        || candidate.format() != PixelFormat::BinaryMask8
    {
        return Err(RasterError::InvalidDimensions);
    }
    if interpretation == RasterRangeInterpretation::Normal {
        return Ok(candidate.clone());
    }

    let width = source.width();
    let height = source.height();
    let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| RasterError::InvalidDimensions)?;
    let mut exterior = Vec::new();
    exterior
        .try_reserve_exact(pixel_count)
        .map_err(|_| RasterError::InvalidDimensions)?;
    exterior.resize(pixel_count, false);
    let mut queue = VecDeque::new();
    queue
        .try_reserve(pixel_count.min(65_536))
        .map_err(|_| RasterError::InvalidDimensions)?;

    let index = |x: u32, y: u32| -> usize { y as usize * width as usize + x as usize };
    let selected = |x: u32, y: u32| -> Result<bool, RasterError> {
        Ok(matches!(candidate.pixel(x, y)?, PixelValue::Binary(255)))
    };
    let covered = |x: u32, y: u32| -> Result<bool, RasterError> {
        Ok(match source.pixel(x, y)? {
            PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value != 0,
            PixelValue::Grayscale16(value) => value != 0,
            PixelValue::Rgba(value) => value[3] != 0,
            PixelValue::Rgba16(value) => value[3] != 0,
        })
    };

    // Candidate-edge uncovered pixels form the exterior seed set. This also
    // covers a candidate touching the final valid paper pixel.
    for y in 0..height {
        for x in 0..width {
            if !selected(x, y)? || covered(x, y)? {
                continue;
            }
            let edge = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
            let touches_outside_candidate = edge
                || (x > 0 && !selected(x - 1, y)?)
                || (x + 1 < width && !selected(x + 1, y)?)
                || (y > 0 && !selected(x, y - 1)?)
                || (y + 1 < height && !selected(x, y + 1)?);
            if touches_outside_candidate {
                exterior[index(x, y)] = true;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        for (next_x, next_y) in [
            (x.wrapping_sub(1), y),
            (x.saturating_add(1), y),
            (x, y.wrapping_sub(1)),
            (x, y.saturating_add(1)),
        ] {
            if next_x >= width || next_y >= height {
                continue;
            }
            let next = index(next_x, next_y);
            if exterior[next] || !selected(next_x, next_y)? || covered(next_x, next_y)? {
                continue;
            }
            exterior[next] = true;
            queue.push_back((next_x, next_y));
        }
    }

    let mut output = TileRaster::new(width, height, PixelFormat::BinaryMask8)?;
    for y in 0..height {
        for x in 0..width {
            if !selected(x, y)? {
                continue;
            }
            let is_covered = covered(x, y)?;
            let is_enclosed = !is_covered && !exterior[index(x, y)];
            let boundary = if is_covered {
                x == 0
                    || y == 0
                    || x + 1 == width
                    || y + 1 == height
                    || (x > 0 && !covered(x - 1, y)?)
                    || (x + 1 < width && !covered(x + 1, y)?)
                    || (y > 0 && !covered(x, y - 1)?)
                    || (y + 1 < height && !covered(x, y + 1)?)
            } else {
                false
            };
            let keep = match interpretation {
                RasterRangeInterpretation::Normal => true,
                RasterRangeInterpretation::Tight => is_covered || is_enclosed,
                RasterRangeInterpretation::EnclosedInterior => is_enclosed,
                RasterRangeInterpretation::Drawing => is_covered,
                RasterRangeInterpretation::Boundary => boundary,
            };
            if keep {
                output.set_pixel(x, y, PixelValue::Binary(255), revision)?;
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(format: PixelFormat) -> TileRaster {
        let mut raster = TileRaster::new(5, 5, format).unwrap();
        for (x, y) in [
            (1, 1),
            (2, 1),
            (3, 1),
            (1, 2),
            (3, 2),
            (1, 3),
            (2, 3),
            (3, 3),
        ] {
            let value = match format {
                PixelFormat::BinaryMask8 => PixelValue::Binary(255),
                PixelFormat::Grayscale8 => PixelValue::Grayscale8(128),
                PixelFormat::Grayscale16 => PixelValue::Grayscale16(32_768),
                PixelFormat::StraightRgba8 | PixelFormat::PremultipliedBgra8 => {
                    PixelValue::Rgba([10, 20, 30, 128])
                }
                PixelFormat::StraightRgba16 => PixelValue::Rgba16([1, 2, 3, 32_768]),
            };
            raster.set_pixel(x, y, value, 1).unwrap();
        }
        raster
    }

    fn selected_count(mask: &TileRaster) -> usize {
        let mut count = 0;
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                count += usize::from(matches!(mask.pixel(x, y).unwrap(), PixelValue::Binary(255)));
            }
        }
        count
    }

    #[test]
    fn range_interpretations_are_exact_for_8_and_16_bit_coverage() {
        for format in [
            PixelFormat::BinaryMask8,
            PixelFormat::Grayscale8,
            PixelFormat::Grayscale16,
            PixelFormat::StraightRgba8,
            PixelFormat::StraightRgba16,
        ] {
            let source = fixture(format);
            let mut candidate = TileRaster::new(5, 5, PixelFormat::BinaryMask8).unwrap();
            for y in 0..5 {
                for x in 0..5 {
                    candidate
                        .set_pixel(x, y, PixelValue::Binary(255), 1)
                        .unwrap();
                }
            }
            let interior = interpret_raster_selection(
                &source,
                &candidate,
                RasterRangeInterpretation::EnclosedInterior,
                2,
            )
            .unwrap();
            assert_eq!(interior.pixel(2, 2).unwrap(), PixelValue::Binary(255));
            assert_eq!(selected_count(&interior), 1);
            let drawing = interpret_raster_selection(
                &source,
                &candidate,
                RasterRangeInterpretation::Drawing,
                2,
            )
            .unwrap();
            assert_eq!(selected_count(&drawing), 8);
            let tight = interpret_raster_selection(
                &source,
                &candidate,
                RasterRangeInterpretation::Tight,
                2,
            )
            .unwrap();
            assert_eq!(selected_count(&tight), 9);
        }
    }

    #[test]
    fn empty_edge_single_pixel_tile_boundary_and_final_pixel_are_bounded() {
        let mut source = TileRaster::new(65, 3, PixelFormat::StraightRgba16).unwrap();
        let mut candidate = TileRaster::new(65, 3, PixelFormat::BinaryMask8).unwrap();
        for y in 0..3 {
            for x in 0..65 {
                candidate
                    .set_pixel(x, y, PixelValue::Binary(255), 1)
                    .unwrap();
            }
        }
        for interpretation in [
            RasterRangeInterpretation::Tight,
            RasterRangeInterpretation::EnclosedInterior,
            RasterRangeInterpretation::Drawing,
            RasterRangeInterpretation::Boundary,
        ] {
            assert_eq!(
                selected_count(
                    &interpret_raster_selection(&source, &candidate, interpretation, 2).unwrap()
                ),
                0
            );
        }

        for (x, y) in [(0, 0), (63, 1), (64, 2)] {
            source
                .set_pixel(x, y, PixelValue::Rgba16([1, 2, 3, 1]), 2)
                .unwrap();
        }
        source
            .set_pixel(64, 1, PixelValue::Rgba16([1, 2, 3, 0]), 2)
            .unwrap();
        for interpretation in [
            RasterRangeInterpretation::Tight,
            RasterRangeInterpretation::Drawing,
            RasterRangeInterpretation::Boundary,
        ] {
            let result =
                interpret_raster_selection(&source, &candidate, interpretation, 3).unwrap();
            assert_eq!(selected_count(&result), 3);
            assert_eq!(result.pixel(64, 2).unwrap(), PixelValue::Binary(255));
            assert_eq!(result.pixel(64, 1).unwrap(), PixelValue::Binary(0));
        }
    }
}
