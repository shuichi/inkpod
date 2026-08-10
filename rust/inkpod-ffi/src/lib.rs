#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(all(
    target_env = "msvc",
    not(target_feature = "crt-static"),
    not(doc),
    not(doctest)
))]
compile_error!("inkpod-ffi requires the statically linked MSVC CRT");

mod application_data;
mod batch;

#[cfg(test)]
#[path = "../tests/unit/contracts.rs"]
mod ffi_contract_tests;

use inkpod_core::{
    ActivePlane, Adjustment, AirbrushGesture, AirbrushStroke, ApplicationColor,
    AssetAlphaSemantics, AssetColorSpace, BoundaryAirbrush, BrushShape, CellCreationOptions,
    CellCreationPlan, CellSizing, Channel, ClipboardPayload, ClipboardPixel, ClipboardPlane,
    ColorBalance, ColorCheckMode, CommonRasterFormat, CoordinateSpace, Core, CoreError,
    CurveInterpolation, CurvePoint, DocumentInfo, DocumentResize, DustMode, DustRemoval,
    EditTarget, EditTargetCommand, EditorBrushOptions, EditorDefaults, EditorFillOptions,
    EditorSelectionOptions, EditorSelectionShape, EditorState, EditorStateInfo, EditorStateUpdate,
    EditorStrokeInput, EditorTarget, EditorTool, EditorVectorOptions, EffectRegionKind,
    EyedropperSource, FileColorChart, FileColorChartEntry, FilePalette, FillOperation, FillRequest,
    Filter, FloatingTransform, FrameAnchor, FrameMetadata, GeometryCommit, GeometryCrossSection,
    GeometryOptions, GeometryPreviewInfo, GeometryPrimitive, GeometryRequest, Gradient,
    GradientKind, GradientMode, GradientStop, GridConfig, GuideAxis, HsvAdjustment, InclusionMode,
    LayerKind, Levels, LightTableDisplayMode, LightTableItemInput, LightTableItemProperties,
    LightTableSource, MAX_CELL_CREATION_COUNT, MAX_COLOR_CHART_NAME_BYTES, MAX_COMMON_RASTER_BYTES,
    MAX_GRADIENT_STOPS, MAX_IMAGE_EDIT_PIXELS, MAX_RASTER_DIMENSION, MAX_SHORTCUT_STROKES,
    MAX_SHORTCUTS, Margins, MirrorAxis, MotionCheckConfig, MotionFrame, PaintTool, PaletteCursor,
    PixelFormat, PixelValue, PlaneType, PointF32, PrimitiveOutcome, PrimitiveRequest,
    RangeInterpretation, RasterAssetInput, RectI32, RenderPassKind, RenderSnapshot, ResizeAnchor,
    ResourceUsage, RgbaRasterBytes, RotateDirection, SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE,
    SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA, SNAPSHOT_FEATURE_SOLID_WHITE_BASE,
    ScopedColorReplaceMode, ScopedColorReplacePreview, ScopedColorReplaceRequest,
    SelectionConstructionOptions, SelectionLayerOperation, SelectionOperation, SelectionSample,
    SelectionShape, SequenceCellSource, SequenceDirection, ShortcutBinding,
    ShortcutSequenceBinding, ShortcutStroke, Stamp, StampGesture, StampShape, StartColorPredicate,
    Stroke, StrokeSample, TileRaster, TraceBrushOptions, TraceBrushShape, VectorCenterlineMode,
    VectorCubicSegment, VectorEndpoint, VectorEraseMode, VectorPathInput, VectorSelectionMode,
    VectorWidthMode, ViewCommand, plan_cell_creation, read_color_chart, read_palette,
    save_color_chart_atomic, save_palette_atomic,
};
use std::cell::RefCell;
use std::mem::{align_of, size_of};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr;
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::thread::{self, ThreadId};

mod abi;
mod animation;
mod determinism;
mod document_edit;
mod editor_state;
mod effects;
mod lifecycle_document;
mod paint_history;
mod support;
mod v3;
mod vector_snapshot;

pub use abi::*;
pub(crate) use abi::{
    ERROR_CAPACITY, MAX_NODE_NAME_BYTES, MAX_PALETTE_COLOR_COUNT, MAX_PATH_BYTES,
    MAX_SELECTION_POINT_COUNT, MAX_STROKE_SAMPLE_COUNT,
};
pub use animation::*;
pub use application_data::*;
pub use determinism::*;
pub use document_edit::*;
pub use editor_state::*;
pub use effects::*;
pub use lifecycle_document::*;
pub(crate) use paint_history::parse_view_command;
pub use paint_history::*;
pub(crate) use support::*;
pub use v3::*;
pub use vector_snapshot::*;
