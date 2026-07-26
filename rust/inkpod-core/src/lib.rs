#![forbid(unsafe_code)]

mod animation;
mod api;
mod batch;
mod core;
mod document;
mod effects;
mod error;
mod history;
mod paint;
mod persistence;
mod selection;
mod snapshot;
mod stroke;
mod transform;
mod vector;
mod view;

pub use animation::{
    LightTableDisplayMode, LightTableItemInfo, LightTableItemInput, LightTableItemProperties,
    LightTableSetInfo, LightTableSource, MotionCheckConfig, MotionFrame, RgbaRasterBytes,
    SequenceCellInfo, SequenceCellSource, SequenceDirection, Thumbnail,
};
pub use api::*;
pub use batch::{
    BATCH_OPERATION_VERSION, BatchColorPair, BatchFailurePolicy, BatchGraph, BatchInputKind,
    BatchInputSelector, BatchItemOutcome, BatchItemResult, BatchMissingTargetPolicy,
    BatchOperation, BatchOperationKind, BatchOutputPolicy, BatchOutputSettings, BatchPreview,
    BatchPreviewItem, BatchRunOptions, BatchRunReport, BatchRunScope, BatchSeed, BatchSeparation,
    BatchTargetSelector,
};
pub use core::Core;
pub use effects::FilterPreviewInfo;
pub use error::CoreError;
pub use history::HistoryEntryInfo;
pub use inkpod_format::CommonRasterFormat;
pub use snapshot::{RenderSnapshot, RenderTile};
pub use vector::{
    RenderVectorFill, RenderVectorSegment, VectorCubicSegment, VectorEraseMode, VectorFillInfo,
    VectorPathInfo, VectorPathInput, VectorRaster, VectorSelectionMode, VectorSelectionRange,
    VectorSelectionResult, VectorWidthMode,
};

use document::{CellDocument, DocumentIds, LayerNode, PaperSpec, PlaneNode};
use history::{HistoryChange, HistoryEntry, PixelChange};
use persistence::{file_plane_to_raster, raster_to_file_plane};
use selection::{FloatingSelection, StagedPixels};
use stroke::StrokeSession;
use view::default_shortcuts;

use inkpod_format::{
    CellFile, CommonRaster, FileAdjustmentLayer, FileAdjustmentMetadata, FileDocumentMetadata,
    FileGrid, FileGuide, FileLayer, FilePlane, FilePlaneProperties, FileTile, FormatError,
    PlaneKind as FilePlaneKind,
};
use inkpod_image::{
    ColorCheckCategory, FillError, FillOptions, MAX_FILL_PIXELS, Palette, PlaneSample, RasterError,
    TILE_SIZE, TileCoord, TileData, VectorFixedPoint, closed_region_fill_with_cancel,
    color_check_category, extend_fill_with_cancel, eyedropper, seed_fill_with_cancel,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const CORE_FEATURES: u64 = 1;
pub const SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE: u64 = 1 << 0;
pub const SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA: u64 = 1 << 1;
pub const DEFAULT_DPI_MILLI: u32 = 96_000;
pub const MAX_LAYERS: usize = 4_096;
pub const MAX_PLANES_PER_LAYER: usize = 4_096;
pub const MAX_GUIDES: usize = 4_096;
pub const MAX_SHORTCUTS: usize = 1_024;
pub const MAX_SHORTCUT_STROKES: usize = 4;
pub const SHORTCUT_MODIFIER_CONTROL: u32 = 1 << 0;
pub const SHORTCUT_MODIFIER_SHIFT: u32 = 1 << 1;
pub const SHORTCUT_MODIFIER_ALT: u32 = 1 << 2;
pub const SHORTCUT_MODIFIER_EXTENDED: u32 = 1 << 3;
pub const SHORTCUT_MODIFIER_MASK: u32 = SHORTCUT_MODIFIER_CONTROL
    | SHORTCUT_MODIFIER_SHIFT
    | SHORTCUT_MODIFIER_ALT
    | SHORTCUT_MODIFIER_EXTENDED;
const MAX_STROKE_SAMPLES: usize = 1_048_576;
const MAX_BRUSH_DIAMETER: f32 = 256.0;
const MAX_STROKE_COORDINATE: f32 = 16_777_216.0;
const MAX_STROKE_WORK: u64 = 16_777_216;
const MIN_ZOOM: f64 = 0.01;
const MAX_ZOOM: f64 = 64.0;

pub use inkpod_format::{
    FrameMetadata, GuideAxis, LayerKind, MAX_COMMON_RASTER_BYTES, Margins, RectI32,
};
pub use inkpod_image::{
    Adjustment, AirbrushGesture, AirbrushStroke, BoundaryAirbrush, Channel, ColorBalance,
    ColorCheckMode, CurveInterpolation, CurvePoint, DustMode, DustRemoval, EffectSample,
    EyedropperSource, Filter, Gradient, GradientKind, GradientMode, GradientStop, HsvAdjustment,
    InclusionMode, Levels, MAX_CURVE_POINTS, MAX_GRADIENT_STOPS, MAX_IMAGE_EDIT_PIXELS,
    MAX_RASTER_DIMENSION, PixelFormat, PixelValue, Stamp, StampGesture, StampShape, TileRaster,
};

#[cfg(test)]
#[path = "../tests/unit/core/mod.rs"]
mod tests;
