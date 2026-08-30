#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Platform-independent document, editing, history, and rendering state for inkpod.
//! [`Core`] is a single-writer state machine. A successful document change creates one history entry, a semantic no-op leaves those values unchanged, and an error never publishes partial state. View-only operations advance the view revision without changing document history or dirty/savepoint state.
//! Stable object identifiers are unique within a [`Core`] instance and remain valid for the lifetime of the referenced object. Public coordinates are in
//! document pixels unless an item explicitly says that it uses device pixels.
//!
//! # Example
//! ```
//! use inkpod_core::{Core, DEFAULT_DPI_MILLI};
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
mod cell_creation;
mod color_chart;
mod color_replace;
mod coordinate;
mod core;
mod cut;
mod document;
mod editor;
mod effects;
mod error;
mod file_io;
mod genesis;
mod geometry;
mod history;
pub mod history_visualization;
mod identity;
pub mod inkscript;
mod journal;
mod output_color_guard;
mod paint;
mod persistence;
mod persistence_task;
mod primitive;
mod reference_view;
mod resource;
mod script;
mod selection;
mod shooting_frame;
mod snapshot;
mod stroke;
mod subpalette;
mod thumbnail;
mod transform;
mod view;
pub use animation::{
    LightTableBulkDirection, LightTableBulkRegistrationAction, LightTableBulkRegistrationEntry,
    LightTableBulkRegistrationPreview, LightTableBulkRegistrationRequest,
    LightTableBulkRegistrationSummary, LightTableDisplayMode, LightTableItemInfo,
    LightTableItemInput, LightTableItemProperties, LightTableSetInfo, LightTableSource,
    MotionCheckConfig, MotionFrame, RgbaRasterBytes, SequenceActivationKind,
    SequenceActivationPlan, SequenceCatalogInfo, SequenceCellInfo, SequenceCellMetadata,
    SequenceCellSource, SequenceDirection, SequenceEndpointPolicy, SequenceRenderSourceIdentity,
    SequenceStepPlan, SequenceStepResult, SequenceSwitchPolicy, SequenceSwitchRequest, Thumbnail,
};
pub use api::*;
pub use asset::{
    AssetAlphaSemantics, AssetColorSpace, AssetDescriptor, AssetId, AssetInfo, AssetKind,
    AssetStoreUsage, CanonicalStreamInput, RasterAssetInput,
};
pub use batch::{
    BATCH_OPERATION_VERSION, BatchColorPair, BatchFailurePolicy, BatchGraph, BatchInputKind,
    BatchInputSelector, BatchItemOutcome, BatchItemResult, BatchMissingTargetPolicy,
    BatchOperation, BatchOperationKind, BatchOutputDestination, BatchOutputFormat,
    BatchOutputSettings, BatchPairCandidate, BatchPairExtraction, BatchPairResolution,
    BatchPreview, BatchPreviewItem, BatchRunOptions, BatchRunReport, BatchRunScope,
    BatchSeparation, BatchSeparationDestination, BatchStagedResult, BatchTargetSelector,
    SequenceSourceIdentity,
};
pub use cell_creation::{
    CellCreationOptions, CellCreationPlan, CellCreationPlanItem, CellSizing, FrameAnchor,
    MAX_CELL_CREATION_COUNT, MAX_CELL_CREATION_DPI_MILLI, plan_cell_creation,
};
pub use color_chart::*;
pub use color_replace::*;
pub(crate) use coordinate::*;
pub use core::Core;
pub use cut::*;
use document::{CellDocument, DocumentIds, LayerNode, PaperSpec, PlaneNode};
use editor::EditorSessionState;
pub use editor::{
    ColorChartCursor, EditTarget, EditorBrushOptions, EditorDefaults, EditorFillOptions,
    EditorFrameDisposition, EditorRevision, EditorSavepointToken, EditorSelectionOptions,
    EditorSelectionShape, EditorState, EditorStateDigest, EditorStateInfo, EditorStateUpdate,
    EditorStrokeInput, EditorTarget, EditorTool, EditorToolStyle, InitialDocumentSpec,
    MAX_EDIT_TARGETS, PaletteCursor,
};
pub use effects::FilterPreviewInfo;
pub use error::CoreError;
pub use file_io::{
    FileIoApply, FileIoItem, FileIoJob, FileIoKind, FileIoProgress, FileIoRequest, FileIoState,
};
pub use genesis::{BaseSurface, GenesisInfo};
pub use geometry::{
    GeometryCommit, GeometryCrossSection, GeometryOptions, GeometryPointResolution,
    GeometryPreviewInfo, GeometryPrimitive, GeometryRequest, GeometrySnapMode, MAX_GEOMETRY_POINTS,
};
use history::{HistoryChange, HistoryEntry, PixelChange, StagedHistoryEntry};
pub use history::{HistoryEntryInfo, HistoryEntryKind};
pub(crate) use identity::*;
pub use inkpod_format::CommonRasterFormat;
use inkpod_format::NativeSection;
use inkpod_format::{
    CommonRaster, DocumentArchive, FileDocumentMetadata, FileGrid, FileGuide, FileLayer, FilePlane,
    FilePlaneProperties, FileShootingFrame, FileShootingFrameAnchor, FileTile, FormatError,
    PlaneKind as FilePlaneKind,
};
pub use inkpod_image::RasterRangeInterpretation as RangeInterpretation;
use inkpod_image::{
    ColorCheckCategory, FillError, FillOptions, MAX_FILL_PIXELS, Palette, PlaneSample, RasterError,
    TILE_SIZE, TileCoord, TileData, TileView, closed_region_fill_with_protection_and_cancel,
    color_check_category, extend_fill_with_protection_and_cancel, eyedropper,
    seed_fill_with_protection_and_cancel,
};
pub use journal::{
    BranchId, HistoryMoveKind, HistoryVisualizationBuilder, HistoryVisualizationProgress,
    JournalBranchCut, JournalCommit, JournalEntry, JournalEventId, JournalHistoryMove,
    JournalReplayInfo, JournalState,
};
use persistence::{file_plane_to_raster, raster_to_file_plane};
pub use persistence_task::{
    DocumentOpenToken, DocumentSaveSnapshot, DocumentSaveToken, PreparedDocumentSave,
};
pub use primitive::{
    CANONICAL_NUMERIC_VERSION, CanonicalProcedure, DocumentStateDigest, PROCEDURE_FORMAT_VERSION,
    PrimitiveId, PrimitiveOutcome, PrimitiveRequest, ProcedureId, ReplayContract, ReplayEpoch,
    StateId, replay_contract,
};
use selection::FloatingSelection;
pub use shooting_frame::*;
pub use snapshot::{
    CanonicalCompositeDigest, RenderPass, RenderPassKind, RenderSnapshot, RenderTile,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use stroke::StrokeSession;
pub use subpalette::{
    MAX_SUBPALETTE_CACHE_BYTES, MAX_SUBPALETTE_ITEMS, SubpaletteCatalog, SubpaletteCatalogInfo,
    SubpaletteImageInput, SubpaletteItem, SubpaletteItemId, SubpaletteSource,
};
use view::default_shortcuts;
mod limits;
pub use limits::*;
mod sequence_io;
pub use sequence_io::{PreparedSequenceSwitch, SequenceSwitchSnapshot};
const MAX_STROKE_SAMPLES: usize = 1_048_576;
const MAX_BRUSH_DIAMETER: f32 = 256.0;
const MAX_STROKE_COORDINATE: f32 = 16_777_216.0;
const MAX_STROKE_WORK: u64 = 16_777_216;
const MAX_PERSISTENT_NUMERIC_ID: u64 = 0x7fff_ffff_ffff_ffff;
const MIN_ZOOM: f64 = 0.01;
const MAX_ZOOM: f64 = 64.0;
pub use inkpod_format::{
    ApplicationColor, FileColorChart, FileColorChartEntry, FilePalette, FrameMetadata, GuideAxis,
    MAX_APPLICATION_COLORS, MAX_COLOR_CHART_NAME_BYTES, MAX_COMMON_RASTER_BYTES, Margins, RectI32,
    read_color_chart, read_palette, save_color_chart_atomic, save_palette_atomic,
};
pub use inkpod_image::{
    AirbrushGesture, AirbrushStroke, BoundaryAirbrush, Channel, ColorBalance, ColorCheckMode,
    CurveInterpolation, CurvePoint, DustMode, DustRemoval, EffectSample, EyedropperSource, Filter,
    Gradient, GradientKind, GradientMode, GradientStop, HsvAdjustment, InclusionMode, Levels,
    MAX_CURVE_POINTS, MAX_GRADIENT_STOPS, MAX_IMAGE_EDIT_PIXELS, MAX_RASTER_DIMENSION, PixelFormat,
    PixelValue, Stamp, StampGesture, StampShape, TileRaster,
};
