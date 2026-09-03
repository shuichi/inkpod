mod alpha;
mod brush;
mod common;
mod dust;
mod filter;
mod gradient;

#[cfg(test)]
use crate::{PixelFormat, PixelValue, RasterError, TileRaster};
pub use alpha::{apply_alpha_gradient, edit_alpha};
pub use brush::{
    apply_airbrush, apply_airbrush_gesture, apply_boundary_airbrush, apply_stamp,
    apply_stamp_gesture,
};
#[cfg(test)]
use common::from_rgba16;
pub use dust::apply_dust_removal;
pub use filter::{apply_filter, apply_filter_with_progress};
pub use gradient::apply_gradient;
pub const MAX_FILTER_RADIUS: u32 = 64;
pub const MAX_CURVE_POINTS: usize = 64;
pub const MAX_GRADIENT_STOPS: usize = 64;
/// Bounds allocations and synchronous work for one image-edit transaction.
/// 8192 x 8192 is the largest full-plane edit accepted by the current
/// implementation; radius-dependent effects have an additional work bound.
pub const MAX_IMAGE_EDIT_PIXELS: u64 = 67_108_864;
const MAX_IMAGE_EDIT_WORK: u128 = 1_100_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    Rgb,
    Red,
    Green,
    Blue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurveInterpolation {
    Bezier,
    BSpline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurvePoint {
    /// Input and output values use the full normalized 0..=65535 range.
    pub input: u16,
    pub output: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Levels {
    pub channel: Channel,
    pub input_shadow: u16,
    pub input_gamma_milli: u32,
    pub input_highlight: u16,
    pub output_shadow: u16,
    pub output_highlight: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HsvAdjustment {
    pub hue_degrees_milli: i32,
    pub saturation_milli: i32,
    pub value_milli: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorBalance {
    pub red_milli: i32,
    pub green_milli: i32,
    pub blue_milli: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Filter {
    SharpenWeak,
    SharpenStrong,
    BlurWeak,
    BlurStrong,
    GaussianBlur {
        radius: u32,
        strength_milli: u32,
    },
    UnsharpMask {
        radius: u32,
        amount_milli: u32,
        threshold: u16,
    },
    Invert {
        channel: Channel,
    },
    AutoContrast,
    BrightnessContrast {
        brightness_milli: i32,
        contrast_milli: i32,
    },
    ToneCurve {
        channel: Channel,
        interpolation: CurveInterpolation,
        points: Vec<CurvePoint>,
    },
    Levels(Levels),
    Hsv(HsvAdjustment),
    ColorBalance(ColorBalance),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GradientKind {
    Linear,
    Radial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GradientMode {
    Composite,
    Overwrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GradientStop {
    pub position_milli: u32,
    pub color: [u16; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub mode: GradientMode,
    pub start_x_milli: i64,
    pub start_y_milli: i64,
    pub end_x_milli: i64,
    pub end_y_milli: i64,
    pub dither: bool,
    pub stops: Vec<GradientStop>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AirbrushStroke {
    pub center_x_milli: i64,
    pub center_y_milli: i64,
    pub radius_milli: u32,
    pub hardness_milli: u32,
    pub opacity_milli: u32,
    pub color: [u16; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectSample {
    pub x_milli: i64,
    pub y_milli: i64,
    pub pressure_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AirbrushGesture {
    pub samples: Vec<EffectSample>,
    pub radius_milli: u32,
    pub hardness_milli: u32,
    pub spacing_milli: u32,
    pub opacity_milli: u32,
    pub fade_milli: u32,
    pub pressure_size: bool,
    pub pressure_opacity: bool,
    pub continuous_dabs: u32,
    pub color: [u16; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryAirbrush {
    pub colors: Vec<[u16; 4]>,
    pub width: u32,
    pub strength_milli: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stamp {
    pub source_x: i32,
    pub source_y: i32,
    pub destination_x: i32,
    pub destination_y: i32,
    pub width: u32,
    pub height: u32,
    pub opacity_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StampGesture {
    pub source_x_milli: i64,
    pub source_y_milli: i64,
    pub samples: Vec<EffectSample>,
    pub radius_milli: u32,
    pub hardness_milli: u32,
    pub spacing_milli: u32,
    pub opacity_milli: u32,
    pub shape: StampShape,
    pub pressure_size: bool,
    pub pressure_opacity: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StampShape {
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DustMode {
    RemoveForeground,
    FillTransparentHoles,
    ReplaceColorOutliers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DustRemoval {
    pub mode: DustMode,
    pub maximum_pixels: u32,
    pub background: crate::LineBackground,
}

#[cfg(test)]
#[path = "../../tests/unit/edit.rs"]
mod tests;
