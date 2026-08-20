#![forbid(unsafe_code)]

mod canonical;
mod edit;
mod fill;
mod output_color_guard;
mod palette;
mod pixel;
mod raster;
mod sampling;
mod selection;

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
pub use output_color_guard::{
    Bt709Ycbcr16, OutputColorGuardCategory, bt709_conservative_guard_category,
    bt709_conservative_ycbcr16,
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
pub use selection::{RasterRangeInterpretation, interpret_raster_selection};

#[cfg(test)]
#[path = "../tests/unit/output_color_guard.rs"]
mod output_color_guard_tests;
#[cfg(test)]
#[path = "../tests/unit/raster_fill.rs"]
mod tests;
pub use canonical::{
    CANONICAL_DOCUMENT_FRACTION_BITS, CANONICAL_DOCUMENT_ONE, Q30_ONE, canonical_pow_unit_u16,
    canonical_q16_from_f32, canonical_q16_from_f64, canonical_scaled_i64_from_f32,
    canonical_scaled_i64_from_f64, canonical_turns_from_degrees_f64, canonical_unit_u16_from_f32,
    ceil_div_i128, color_within_tolerance, div_round_ties_even_i128, floor_div_i128, integer_sqrt,
    premultiply_u8, rotate_q16, sin_cos_turns_q30, source_over_rgba8, source_over_rgba16,
};
