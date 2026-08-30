//! Image effects, filter previews, and editing gestures.

use super::{
    AirbrushGesture, AirbrushStroke, BoundaryAirbrush, CellDocument, CoordinateSpace, Core,
    CoreError, DispatchOutcome, DustRemoval, EffectRegionKind, EffectSample, Filter, Gradient,
    PixelFormat, PixelValue, PlaneNode, PlaneType, PointF32, RangeInterpretation, RectI32,
    SelectionConstructionOptions, SelectionOperation, SelectionShape, Stamp, StampGesture,
    StrokeSample,
};
use crate::identity::*;
use crate::selection::{combine_selection_masks, selection_mask_for_shape};
use crate::stroke::{DocumentStrokeSample, document_samples_for_view};
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

pub use model::FilterPreviewInfo;
pub(crate) use model::{FilterPreview, PreviewProcedure};
