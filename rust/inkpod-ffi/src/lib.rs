#![deny(unsafe_op_in_unsafe_fn)]

mod batch;

#[cfg(test)]
#[path = "../tests/unit/contracts.rs"]
mod ffi_contract_tests;

use inkpod_core::{
    ActivePlane, Adjustment, AirbrushGesture, AirbrushStroke, BoundaryAirbrush, Channel,
    ClipboardPayload, ClipboardPixel, ClipboardPlane, ColorBalance, ColorCheckMode, Command,
    CommonRasterFormat, CoordinateSpace, Core, CoreError, CurveInterpolation, CurvePoint,
    DocumentInfo, DocumentResize, DustMode, DustRemoval, EffectRegionKind, EyedropperSource,
    FillOperation, FillRequest, Filter, FloatingTransform, FrameMetadata, Gradient, GradientKind,
    GradientMode, GradientStop, GridConfig, GuideAxis, HsvAdjustment, InclusionMode, LayerKind,
    Levels, LightTableDisplayMode, LightTableItemInput, LightTableItemProperties, LightTableSource,
    MAX_COMMON_RASTER_BYTES, MAX_GRADIENT_STOPS, MAX_IMAGE_EDIT_PIXELS, MAX_RASTER_DIMENSION,
    Margins, MirrorAxis, MotionCheckConfig, MotionFrame, PaintTool, PixelFormat, PixelValue,
    PlaneType, PointF32, RectI32, RenderSnapshot, ResizeAnchor, RgbaRasterBytes, RotateDirection,
    SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE, SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA,
    SelectionLayerOperation, SelectionOperation, SelectionShape, SequenceCellInfo,
    SequenceCellSource, SequenceDirection, ShortcutBinding, Stamp, StampGesture, StampShape,
    Stroke, StrokeSample, TileRaster, VectorCubicSegment, VectorEraseMode, VectorPathInput,
    VectorSelectionMode, VectorWidthMode, ViewCommand,
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
mod document_edit;
mod effects;
mod lifecycle_document;
mod paint_history;
mod support;
mod vector_snapshot;

pub use abi::*;
pub(crate) use abi::{
    ERROR_CAPACITY, MAX_COMMAND_COUNT, MAX_NODE_NAME_BYTES, MAX_PALETTE_COLOR_COUNT,
    MAX_PATH_BYTES, MAX_SELECTION_POINT_COUNT, MAX_STROKE_SAMPLE_COUNT,
};
pub use animation::*;
pub use document_edit::*;
pub use effects::*;
pub use lifecycle_document::*;
pub(crate) use paint_history::parse_view_command;
pub use paint_history::*;
pub(crate) use support::*;
pub use vector_snapshot::*;
