//! Deterministic, platform-independent vector geometry primitives.

pub const VECTOR_UNITS_PER_PIXEL: f64 = 1_000.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VectorFixedPoint {
    pub x_milli: i32,
    pub y_milli: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VectorFixedCubic {
    pub p0: VectorFixedPoint,
    pub p1: VectorFixedPoint,
    pub p2: VectorFixedPoint,
    pub p3: VectorFixedPoint,
    pub width_start_milli: u32,
    pub width_end_milli: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorFlatSample {
    pub point: (f64, f64),
    pub width: f64,
    pub parameter: f64,
}

pub fn vector_fixed_xy(point: VectorFixedPoint) -> (f64, f64) {
    (
        f64::from(point.x_milli) / VECTOR_UNITS_PER_PIXEL,
        f64::from(point.y_milli) / VECTOR_UNITS_PER_PIXEL,
    )
}

pub fn evaluate_vector_cubic(segment: VectorFixedCubic, amount: f64) -> (f64, f64) {
    let one = 1.0 - amount;
    let weights = [
        one * one * one,
        3.0 * one * one * amount,
        3.0 * one * amount * amount,
        amount * amount * amount,
    ];
    let points = [segment.p0, segment.p1, segment.p2, segment.p3];
    let mut result = (0.0, 0.0);
    for (weight, point) in weights.into_iter().zip(points) {
        let point = vector_fixed_xy(point);
        result.0 += weight * point.0;
        result.1 += weight * point.1;
    }
    result
}

pub fn flatten_vector_path(
    segments: &[VectorFixedCubic],
    steps_per_segment: usize,
) -> Vec<VectorFlatSample> {
    let mut samples = Vec::with_capacity(segments.len() * steps_per_segment + 1);
    for (segment_index, segment) in segments.iter().copied().enumerate() {
        for step in 0..=steps_per_segment {
            if segment_index != 0 && step == 0 {
                continue;
            }
            let amount = step as f64 / steps_per_segment as f64;
            samples.push(VectorFlatSample {
                point: evaluate_vector_cubic(segment, amount),
                width: vector_lerp(
                    f64::from(segment.width_start_milli) / VECTOR_UNITS_PER_PIXEL,
                    f64::from(segment.width_end_milli) / VECTOR_UNITS_PER_PIXEL,
                    amount,
                ),
                parameter: segment_index as f64 + amount,
            });
        }
    }
    samples
}

pub fn vector_point_at(segments: &[VectorFixedCubic], parameter: f64) -> Option<(f64, f64)> {
    if segments.is_empty() || parameter < 0.0 || parameter > segments.len() as f64 {
        return None;
    }
    let index = (parameter.floor() as usize).min(segments.len() - 1);
    let local = if parameter >= segments.len() as f64 {
        1.0
    } else {
        parameter - index as f64
    };
    Some(evaluate_vector_cubic(segments[index], local))
}

pub fn split_vector_cubic(
    segment: VectorFixedCubic,
    amount: f64,
) -> (VectorFixedCubic, VectorFixedCubic) {
    let point = |left: VectorFixedPoint, right: VectorFixedPoint| {
        let value = vector_lerp_point(vector_fixed_xy(left), vector_fixed_xy(right), amount);
        VectorFixedPoint {
            x_milli: (value.0 * VECTOR_UNITS_PER_PIXEL).round() as i32,
            y_milli: (value.1 * VECTOR_UNITS_PER_PIXEL).round() as i32,
        }
    };
    let a = point(segment.p0, segment.p1);
    let b = point(segment.p1, segment.p2);
    let c = point(segment.p2, segment.p3);
    let d = point(a, b);
    let e = point(b, c);
    let midpoint = point(d, e);
    let width = vector_lerp(
        f64::from(segment.width_start_milli),
        f64::from(segment.width_end_milli),
        amount,
    )
    .round() as u32;
    (
        VectorFixedCubic {
            p0: segment.p0,
            p1: a,
            p2: d,
            p3: midpoint,
            width_start_milli: segment.width_start_milli,
            width_end_milli: width,
        },
        VectorFixedCubic {
            p0: midpoint,
            p1: e,
            p2: c,
            p3: segment.p3,
            width_start_milli: width,
            width_end_milli: segment.width_end_milli,
        },
    )
}

pub fn sub_vector_cubic(segment: VectorFixedCubic, start: f64, end: f64) -> VectorFixedCubic {
    let (left, _) = split_vector_cubic(segment, end.clamp(0.0, 1.0));
    if start <= 0.0 {
        return left;
    }
    let relative = (start / end).clamp(0.0, 1.0);
    split_vector_cubic(left, relative).1
}

pub fn vector_line_cubic(
    start: VectorFixedPoint,
    end: VectorFixedPoint,
    width_start_milli: u32,
    width_end_milli: u32,
) -> VectorFixedCubic {
    let first = vector_lerp_point(vector_fixed_xy(start), vector_fixed_xy(end), 1.0 / 3.0);
    let second = vector_lerp_point(vector_fixed_xy(start), vector_fixed_xy(end), 2.0 / 3.0);
    VectorFixedCubic {
        p0: start,
        p1: VectorFixedPoint {
            x_milli: (first.0 * VECTOR_UNITS_PER_PIXEL).round() as i32,
            y_milli: (first.1 * VECTOR_UNITS_PER_PIXEL).round() as i32,
        },
        p2: VectorFixedPoint {
            x_milli: (second.0 * VECTOR_UNITS_PER_PIXEL).round() as i32,
            y_milli: (second.1 * VECTOR_UNITS_PER_PIXEL).round() as i32,
        },
        p3: end,
        width_start_milli,
        width_end_milli,
    }
}

pub fn vector_line_intersection(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
) -> Option<(f64, f64)> {
    let r = (b.0 - a.0, b.1 - a.1);
    let s = (d.0 - c.0, d.1 - c.1);
    let denominator = vector_cross(r, s);
    if denominator.abs() < 1.0e-12 {
        return None;
    }
    let delta = (c.0 - a.0, c.1 - a.1);
    let left = vector_cross(delta, s) / denominator;
    let right = vector_cross(delta, r) / denominator;
    ((-1.0e-9..=1.0 + 1.0e-9).contains(&left) && (-1.0e-9..=1.0 + 1.0e-9).contains(&right))
        .then_some((left.clamp(0.0, 1.0), right.clamp(0.0, 1.0)))
}

pub fn vector_path_intersections(
    left: &[VectorFixedCubic],
    right: &[VectorFixedCubic],
    steps_per_segment: usize,
) -> Vec<f64> {
    let left_samples = flatten_vector_path(left, steps_per_segment);
    let right_samples = flatten_vector_path(right, steps_per_segment);
    let mut intersections = Vec::new();
    for left_pair in left_samples.windows(2) {
        for right_pair in right_samples.windows(2) {
            if let Some((left_fraction, _)) = vector_line_intersection(
                left_pair[0].point,
                left_pair[1].point,
                right_pair[0].point,
                right_pair[1].point,
            ) {
                let value = vector_lerp(
                    left_pair[0].parameter,
                    left_pair[1].parameter,
                    left_fraction,
                );
                intersections.push((value * 1.0e9).round() / 1.0e9);
            }
        }
    }
    intersections.sort_by(f64::total_cmp);
    intersections.dedup_by(|left, right| (*left - *right).abs() < 1.0e-9);
    intersections
}

pub fn vector_distance_to_segment(
    point: (f64, f64),
    start: (f64, f64),
    end: (f64, f64),
) -> (f64, f64) {
    let delta = (end.0 - start.0, end.1 - start.1);
    let length_squared = delta.0 * delta.0 + delta.1 * delta.1;
    let fraction = if length_squared <= 1.0e-18 {
        0.0
    } else {
        (((point.0 - start.0) * delta.0 + (point.1 - start.1) * delta.1) / length_squared)
            .clamp(0.0, 1.0)
    };
    let closest = (start.0 + delta.0 * fraction, start.1 + delta.1 * fraction);
    (vector_squared_distance(point, closest).sqrt(), fraction)
}

pub fn vector_stroke_contains(
    segments: &[VectorFixedCubic],
    point: (f64, f64),
    steps_per_segment: usize,
) -> bool {
    flatten_vector_path(segments, steps_per_segment)
        .windows(2)
        .any(|pair| {
            let (distance, fraction) =
                vector_distance_to_segment(point, pair[0].point, pair[1].point);
            let width = vector_lerp(pair[0].width, pair[1].width, fraction);
            distance <= width * 0.5
        })
}

pub fn vector_source_over(destination: [u8; 4], source: [u8; 4]) -> [u8; 4] {
    let source_alpha = u32::from(source[3]);
    let destination_alpha = u32::from(destination[3]);
    let inverse = 255 - source_alpha;
    let output_alpha = source_alpha + (destination_alpha * inverse + 127) / 255;
    if output_alpha == 0 {
        return [0; 4];
    }
    let mut output = [0_u8; 4];
    for channel in 0..3 {
        let premultiplied = u32::from(source[channel]) * source_alpha
            + (u32::from(destination[channel]) * destination_alpha * inverse + 127) / 255;
        output[channel] = ((premultiplied + output_alpha / 2) / output_alpha) as u8;
    }
    output[3] = output_alpha as u8;
    output
}

pub fn vector_squared_distance(left: (f64, f64), right: (f64, f64)) -> f64 {
    (left.0 - right.0).powi(2) + (left.1 - right.1).powi(2)
}

pub fn vector_lerp(left: f64, right: f64, amount: f64) -> f64 {
    left + (right - left) * amount
}

pub fn vector_lerp_point(left: (f64, f64), right: (f64, f64), amount: f64) -> (f64, f64) {
    (
        vector_lerp(left.0, right.0, amount),
        vector_lerp(left.1, right.1, amount),
    )
}

fn vector_cross(left: (f64, f64), right: (f64, f64)) -> f64 {
    left.0 * right.1 - left.1 * right.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i32, y: i32) -> VectorFixedPoint {
        VectorFixedPoint {
            x_milli: x * 1_000,
            y_milli: y * 1_000,
        }
    }

    #[test]
    fn vector_geometry_split_intersection_and_hit_are_deterministic() {
        let horizontal = vector_line_cubic(point(0, 4), point(8, 4), 2_000, 2_000);
        let vertical = vector_line_cubic(point(4, 0), point(4, 8), 2_000, 2_000);
        assert_eq!(
            vector_path_intersections(&[horizontal], &[vertical], 64),
            vec![0.5]
        );
        let (left, right) = split_vector_cubic(horizontal, 0.5);
        assert_eq!(left.p3, point(4, 4));
        assert_eq!(right.p0, point(4, 4));
        assert!(vector_stroke_contains(&[horizontal], (4.0, 4.75), 32));
        assert!(!vector_stroke_contains(&[horizontal], (4.0, 5.25), 32));
    }
}
