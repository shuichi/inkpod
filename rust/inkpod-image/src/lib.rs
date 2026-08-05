#![forbid(unsafe_code)]

mod edit;
mod fill;
mod palette;
mod pixel;
mod raster;
mod sampling;
mod vector;

pub use edit::{
    Adjustment, AirbrushGesture, AirbrushStroke, BoundaryAirbrush, Channel, ColorBalance,
    CurveInterpolation, CurvePoint, DustMode, DustRemoval, EffectSample, Filter, Gradient,
    GradientKind, GradientMode, GradientStop, HsvAdjustment, Levels, MAX_CURVE_POINTS,
    MAX_FILTER_RADIUS, MAX_GRADIENT_STOPS, MAX_IMAGE_EDIT_PIXELS, Stamp, StampGesture, StampShape,
    apply_adjustment, apply_airbrush, apply_airbrush_gesture, apply_alpha_gradient,
    apply_boundary_airbrush, apply_dust_removal, apply_filter, apply_filter_with_progress,
    apply_gradient, apply_stamp, apply_stamp_gesture, edit_alpha,
};
pub use fill::{
    FillError, FillOptions, FillPlan, InclusionMode, MAX_FILL_PIXELS, MAX_GAP_CLOSE,
    MAX_INCLUSION_COLORS, PixelEdit, closed_region_fill, closed_region_fill_with_cancel,
    extend_fill, extend_fill_with_cancel, seed_fill, seed_fill_with_cancel,
};
pub use palette::{MAX_PALETTE_COLORS, Palette};
pub use pixel::{PixelFormat, PixelValue};
pub use raster::{
    FNV_OFFSET, MAX_RASTER_DIMENSION, RasterError, TILE_SIZE, TileCoord, TileData, TileRaster,
    TileView, fnv_bytes,
};
pub use sampling::{
    ColorCheckCategory, ColorCheckMode, EyedropperSource, PlaneSample, color_check_category,
    eyedropper,
};
pub use vector::{
    VECTOR_UNITS_PER_PIXEL, VectorFixedCubic, VectorFixedPoint, VectorFlatSample,
    evaluate_vector_cubic, flatten_vector_path, split_vector_cubic, sub_vector_cubic,
    vector_distance_to_segment, vector_fixed_xy, vector_lerp, vector_lerp_point, vector_line_cubic,
    vector_line_intersection, vector_path_intersections, vector_point_at, vector_source_over,
    vector_squared_distance, vector_stroke_contains,
};

#[cfg(test)]
#[path = "../tests/unit/raster_fill.rs"]
mod tests;
