//! Image effects, adjustment previews, and editing gestures.

use super::{
    Adjustment, AirbrushGesture, AirbrushStroke, BoundaryAirbrush, CellDocument, CoordinateSpace,
    Core, CoreError, DispatchOutcome, DustRemoval, EffectRegionKind, EffectSample, Filter,
    Gradient, LayerKind, LayerNode, MAX_LAYERS, PixelFormat, PixelValue, PlaneNode, PlaneType,
    PointF32, RectI32, SelectionOperation, SelectionShape, Stamp, StampGesture, StrokeSample,
};
use crate::document::{unique_layer_name, validate_node_name};
use crate::selection::{combine_selection_masks, selection_mask_for_shape};
use crate::stroke::document_samples_for_view;
use inkpod_image::{
    TileRaster, apply_airbrush, apply_airbrush_gesture, apply_alpha_gradient,
    apply_boundary_airbrush, apply_dust_removal, apply_filter, apply_filter_with_progress,
    apply_gradient, apply_stamp, apply_stamp_gesture, edit_alpha,
};

mod helpers;
mod model;
mod operations;
mod preview;
mod tools;

#[cfg(test)]
use helpers::pressure_trace_contains;
pub(crate) use model::FilterPreview;
pub use model::FilterPreviewInfo;

#[cfg(test)]
#[path = "../../tests/unit/effects.rs"]
mod tests;
