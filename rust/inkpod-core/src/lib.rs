#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Platform-independent document, editing, history, and rendering state for inkpod.
//!
//! [`Core`] is a single-writer state machine. Document-changing operations are
//! transactional: a successful change advances the document revision and creates
//! one history entry, a semantic no-op leaves those values unchanged, and an error
//! never publishes partial state. View-only operations advance the view revision
//! without changing document history or dirty/savepoint state.
//!
//! Stable object identifiers are unique within a [`Core`] instance and remain
//! valid for the lifetime of the referenced object. Public coordinates are in
//! document pixels unless an item explicitly says that it uses device pixels.
//!
//! # Example
//!
//! ```
//! use inkpod_core::{Core, DEFAULT_DPI_MILLI};
//!
//! let mut core = Core::new();
//! let document = core.new_cell(64, 48, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)?;
//! assert_eq!((document.width, document.height), (64, 48));
//! assert!(!document.dirty);
//! # Ok::<(), inkpod_core::CoreError>(())
//! ```

mod animation;
mod api;
mod asset;
mod asset_operations;
mod batch;
mod coordinate;
mod core;
mod document;
mod editor;
mod effects;
mod error;
mod genesis;
mod history;
mod identity;
mod journal;
mod paint;
mod persistence;
mod primitive;
mod resource;
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
pub use asset::{
    AssetAlphaSemantics, AssetColorSpace, AssetDescriptor, AssetId, AssetInfo, AssetKind,
    AssetStoreUsage, CanonicalStreamInput, RasterAssetInput,
};
pub use batch::{
    BATCH_OPERATION_VERSION, BatchColorPair, BatchFailurePolicy, BatchGraph, BatchInputKind,
    BatchInputSelector, BatchItemOutcome, BatchItemResult, BatchMissingTargetPolicy,
    BatchOperation, BatchOperationKind, BatchOutputPolicy, BatchOutputSettings, BatchPreview,
    BatchPreviewItem, BatchRunOptions, BatchRunReport, BatchRunScope, BatchSeed, BatchSeparation,
    BatchTargetSelector,
};
pub use core::Core;
pub use editor::{
    EditorDefaults, EditorFillOptions, EditorFrameDisposition, EditorRevision,
    EditorSavepointToken, EditorSelectionOptions, EditorSelectionShape, EditorState,
    EditorStateDigest, EditorStateInfo, EditorStateUpdate, EditorStrokeInput, EditorTarget,
    EditorTool, EditorToolStyle, EditorVectorOptions, InitialDocumentSpec, PaletteCursor,
};
pub use effects::FilterPreviewInfo;
pub use error::CoreError;
pub use genesis::{BaseSurface, GenesisInfo};
pub use history::HistoryEntryInfo;
pub use inkpod_format::CommonRasterFormat;
pub use journal::{
    BranchId, HistoryMoveKind, JournalBranchCut, JournalCommit, JournalEntry, JournalEventId,
    JournalHistoryMove, JournalReplayInfo, JournalState,
};
pub use primitive::{
    CANONICAL_NUMERIC_VERSION, CanonicalProcedure, DocumentStateDigest, PROCEDURE_FORMAT_VERSION,
    PrimitiveId, PrimitiveOutcome, PrimitiveRequest, ProcedureId, ReplayContract, ReplayEpoch,
    StateId, replay_contract,
};
pub use snapshot::{
    CanonicalCompositeDigest, RenderAdjustmentLut, RenderPass, RenderPassKind, RenderSnapshot,
    RenderTile,
};
pub use vector::{
    RenderVectorFill, RenderVectorSegment, VectorCubicSegment, VectorEraseMode, VectorFillInfo,
    VectorPathInfo, VectorPathInput, VectorRaster, VectorSelectionMode, VectorSelectionRange,
    VectorSelectionResult, VectorWidthMode,
};

pub(crate) use coordinate::*;
use document::{CellDocument, DocumentIds, LayerNode, PaperSpec, PlaneNode};
use editor::EditorSessionState;
use history::{HistoryChange, HistoryEntry, PixelChange, StagedHistoryEntry};
pub(crate) use identity::*;
use persistence::{file_plane_to_raster, raster_to_file_plane};
use selection::FloatingSelection;
use stroke::StrokeSession;
use view::default_shortcuts;

use inkpod_format::NativeSection;
use inkpod_format::{
    CommonRaster, DocumentArchive, FileAdjustmentLayer, FileAdjustmentMetadata,
    FileDocumentMetadata, FileGrid, FileGuide, FileLayer, FilePlane, FilePlaneProperties, FileTile,
    FormatError, PlaneKind as FilePlaneKind,
};
use inkpod_image::{
    ColorCheckCategory, FillError, FillOptions, MAX_FILL_PIXELS, Palette, PlaneSample, RasterError,
    TILE_SIZE, TileCoord, TileData, TileView, VectorFixedPoint, closed_region_fill_with_cancel,
    color_check_category, extend_fill_with_cancel, eyedropper, seed_fill_with_cancel,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Feature bits supported by this version of the Rust Core API.
pub const CORE_FEATURES: u64 = 1;
/// Snapshot feature bit indicating legacy-white color-check rendering.
pub const SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE: u64 = 1 << 0;
/// Snapshot feature bit indicating native-alpha color-check rendering.
pub const SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA: u64 = 1 << 1;
/// Snapshot feature bit identifying an allocation-free SolidWhite Genesis base.
pub const SNAPSHOT_FEATURE_SOLID_WHITE_BASE: u64 = 1 << 2;
/// Default horizontal and vertical resolution in thousandths of a DPI.
pub const DEFAULT_DPI_MILLI: u32 = 96_000;
/// Maximum number of layers accepted in one document.
pub const MAX_LAYERS: usize = 4_096;
/// Maximum number of planes accepted in one layer.
pub const MAX_PLANES_PER_LAYER: usize = 4_096;
/// Maximum number of document guides.
pub const MAX_GUIDES: usize = 4_096;
/// Maximum number of configured shortcut commands.
pub const MAX_SHORTCUTS: usize = 1_024;
/// Maximum number of key strokes in one shortcut sequence.
pub const MAX_SHORTCUT_STROKES: usize = 4;
/// Shortcut modifier bit for the Control key.
pub const SHORTCUT_MODIFIER_CONTROL: u32 = 1 << 0;
/// Shortcut modifier bit for the Shift key.
pub const SHORTCUT_MODIFIER_SHIFT: u32 = 1 << 1;
/// Shortcut modifier bit for the Alt key.
pub const SHORTCUT_MODIFIER_ALT: u32 = 1 << 2;
/// Shortcut modifier bit distinguishing extended virtual keys.
pub const SHORTCUT_MODIFIER_EXTENDED: u32 = 1 << 3;
/// Mask containing every supported shortcut modifier bit.
pub const SHORTCUT_MODIFIER_MASK: u32 = SHORTCUT_MODIFIER_CONTROL
    | SHORTCUT_MODIFIER_SHIFT
    | SHORTCUT_MODIFIER_ALT
    | SHORTCUT_MODIFIER_EXTENDED;
const MAX_STROKE_SAMPLES: usize = 1_048_576;
const MAX_BRUSH_DIAMETER: f32 = 256.0;
const MAX_STROKE_COORDINATE: f32 = 16_777_216.0;
const MAX_STROKE_WORK: u64 = 16_777_216;
const MAX_PERSISTENT_NUMERIC_ID: u64 = 0x7fff_ffff_ffff_ffff;
const MIN_ZOOM: f64 = 0.01;
const MAX_ZOOM: f64 = 64.0;

pub use inkpod_format::{
    ApplicationColor, FileColorChart, FileColorChartEntry, FilePalette, FrameMetadata, GuideAxis,
    LayerKind, MAX_APPLICATION_COLORS, MAX_COLOR_CHART_NAME_BYTES, MAX_COMMON_RASTER_BYTES,
    Margins, RectI32, read_color_chart, read_palette, save_color_chart_atomic, save_palette_atomic,
};
pub use inkpod_image::{
    Adjustment, AirbrushGesture, AirbrushStroke, BoundaryAirbrush, Channel, ColorBalance,
    ColorCheckMode, CurveInterpolation, CurvePoint, DustMode, DustRemoval, EffectSample,
    EyedropperSource, Filter, Gradient, GradientKind, GradientMode, GradientStop, HsvAdjustment,
    InclusionMode, Levels, MAX_CURVE_POINTS, MAX_GRADIENT_STOPS, MAX_IMAGE_EDIT_PIXELS,
    MAX_RASTER_DIMENSION, PixelFormat, PixelValue, Stamp, StampGesture, StampShape, TileRaster,
};
