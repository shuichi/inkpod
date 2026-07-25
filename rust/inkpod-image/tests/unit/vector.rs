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
