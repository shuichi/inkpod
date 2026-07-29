//! Vector path and fill state.

use super::{
    CellDocument, Core, CoreError, DispatchOutcome, LayerKind, LayerNode, LayerThumbnail,
    PixelFormat, PixelValue, PlaneNode, PlaneType, PointF32, RectI32, TileRaster,
};
use crate::document::{unique_layer_name, validate_node_name};
use inkpod_format::{
    FileVectorFill, FileVectorMetadata, FileVectorPath, FileVectorPoint, FileVectorSegment,
    MAX_VECTOR_BOUNDARIES, MAX_VECTOR_FILLS, MAX_VECTOR_PATHS, MAX_VECTOR_SEGMENTS,
};
use inkpod_image::{
    VECTOR_UNITS_PER_PIXEL as UNITS_PER_PIXEL, VectorFixedCubic as VectorSegment,
    VectorFixedPoint as FixedPoint, VectorFlatSample as FlatSample, flatten_vector_path,
    sub_vector_cubic, vector_distance_to_segment, vector_fixed_xy, vector_lerp, vector_line_cubic,
    vector_line_intersection, vector_path_intersections, vector_point_at, vector_source_over,
    vector_squared_distance,
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_COORDINATE: f64 = 2_000_000.0;
const MAX_WIDTH: f32 = 4_096.0;
const FLATTEN_STEPS: usize = 64;
const RASTER_STEPS: usize = 32;
const MAX_VECTOR_RASTER_PIXELS: u64 = 16_777_216;

mod geometry;
mod model;
mod operations;

pub(crate) use model::VectorState;
pub use model::{
    RenderVectorFill, RenderVectorSegment, VectorCubicSegment, VectorEraseMode, VectorFillInfo,
    VectorPathInfo, VectorPathInput, VectorRaster, VectorSelectionMode, VectorSelectionRange,
    VectorSelectionResult, VectorWidthMode,
};

#[cfg(test)]
#[path = "../../tests/unit/vector_state.rs"]
mod tests;
