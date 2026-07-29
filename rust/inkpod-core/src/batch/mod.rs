//! Batch processing graph and execution.

use super::{
    ActivePlane, BoundaryAirbrush, CellDocument, Channel, ColorBalance, Core, CoreError,
    CurveInterpolation, CurvePoint, DocumentResize, DustMode, DustRemoval, FillOperation,
    FillRequest, Filter, HsvAdjustment, InclusionMode, LayerKind, Levels, MAX_CURVE_POINTS,
    MAX_IMAGE_EDIT_PIXELS, MirrorAxis, PixelFormat, PixelValue, PlaneType, ResizeAnchor,
    RotateDirection, TILE_SIZE, TileCoord, VectorWidthMode,
};
use crate::animation::{SequenceCellSource, parse_cell_number};
use inkpod_format::{
    BATCH_GRAPH_VERSION, FileBatchGraph, FileBatchInput, FileBatchOperation, FileBatchOutput,
    FileBatchTarget, read_batch_graph, save_batch_graph_atomic,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const BATCH_OPERATION_VERSION: u32 = 1;
const MAX_BATCH_COLOR_PAIRS: usize = 4_096;
const MAX_BATCH_SEEDS: usize = 4_096;
const MAX_BATCH_COLORS: usize = 4_096;
const MAX_BATCH_INPUTS: usize = 16_384;
const MAX_BATCH_OPERATIONS: usize = 1_024;
const MAX_BATCH_NAME_BYTES: usize = 1_024;
const MAX_BATCH_PATH_BYTES: usize = 32_768;
const BATCH_WAIT_POLL_MILLISECONDS: u32 = 50;

const INPUT_FILE: u32 = 1;
const INPUT_FOLDER: u32 = 2;
const INPUT_CURRENT_SEQUENCE: u32 = 3;

const OUTPUT_DUPLICATE: u32 = 1;
const OUTPUT_NEW_SAVE: u32 = 2;
const OUTPUT_OVERWRITE: u32 = 3;
const OUTPUT_NATIVE_INKPOD: u32 = 1;
const FAILURE_CONTINUE: u32 = 1;
const FAILURE_STOP: u32 = 2;
const MISSING_SKIP: u32 = 1;
const MISSING_ERROR: u32 = 2;

const OP_COLOR_REPLACE: u32 = 1;
const OP_CONTINUOUS_FILL: u32 = 2;
const OP_SEPARATION: u32 = 3;
const OP_VISIBILITY: u32 = 4;
const OP_LINE_WIDTH: u32 = 5;
const OP_FILTER: u32 = 6;
const OP_BOUNDARY_AIRBRUSH: u32 = 7;
const OP_DUST_REMOVAL: u32 = 8;
const OP_MIRROR: u32 = 9;
const OP_ROTATE_90: u32 = 10;
const OP_RESIZE: u32 = 11;
const OP_CONVERT_PLANE: u32 = 12;

const OP_ENABLED: u64 = 1;
const OP_CONFIGURE_EACH_RUN: u64 = 1 << 1;

mod codec;
mod execute;
mod model;
mod operations;
mod validation;

pub use model::{
    BatchColorPair, BatchFailurePolicy, BatchGraph, BatchInputKind, BatchInputSelector,
    BatchItemOutcome, BatchItemResult, BatchMissingTargetPolicy, BatchOperation,
    BatchOperationKind, BatchOutputPolicy, BatchOutputSettings, BatchPreview, BatchPreviewItem,
    BatchRunOptions, BatchRunReport, BatchRunScope, BatchSeed, BatchSeparation,
    BatchTargetSelector,
};
