//! Batch processing graph and execution.

use super::{
    AssetAlphaSemantics, AssetColorSpace, CellDocument, Core, CoreError, DEFAULT_DPI_MILLI,
    DispatchOutcome, LayerKind, MAX_IMAGE_EDIT_PIXELS, PixelFormat, PixelValue, PlaneType,
    RasterAssetInput, RectI32, TILE_SIZE, TileCoord,
};
use crate::animation::parse_cell_number;
use crate::asset;
use crate::identity::*;
use inkpod_format::{
    BATCH_GRAPH_VERSION, CommonRasterFormat, FileBatchGraph, FileBatchInput, FileBatchOperation,
    FileBatchOutput, FileBatchTarget, read_batch_graph, save_batch_graph_atomic,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Version required in every [`BatchOperation`] payload.
pub const BATCH_OPERATION_VERSION: u32 = 3;
const MAX_BATCH_COLOR_PAIRS: usize = 4_096;
const MAX_BATCH_COLORS: usize = 4_096;
const MAX_BATCH_INPUTS: usize = 16_384;
const MAX_BATCH_OPERATIONS: usize = 1_024;
const MAX_BATCH_NAME_BYTES: usize = 1_024;
const MAX_BATCH_PATH_BYTES: usize = 32_768;
const BATCH_WAIT_POLL_MILLISECONDS: u32 = 50;

const INPUT_FILE: u32 = 1;
const INPUT_FOLDER: u32 = 2;
const INPUT_ACTIVE_DOCUMENT: u32 = 3;

const OUTPUT_FOLDER: u32 = 1;
const OUTPUT_ACTIVE_DOCUMENT: u32 = 2;
const OUTPUT_NEW_TABS: u32 = 3;
const OUTPUT_NATIVE_INKPOD: u32 = 1;
const OUTPUT_PNG: u32 = 2;
const OUTPUT_TIFF: u32 = 3;
const OUTPUT_TGA: u32 = 4;
const OUTPUT_BMP: u32 = 5;
const FAILURE_CONTINUE: u32 = 1;
const FAILURE_STOP: u32 = 2;
const MISSING_SKIP: u32 = 1;
const MISSING_ERROR: u32 = 2;

const OP_COLOR_REPLACE: u32 = 1;
const OP_MOVE_TO_COLOR_PLANE: u32 = 2;
const OP_MASKING: u32 = 3;
const OP_ERASE: u32 = 4;

const OP_ENABLED: u64 = 1;

mod codec;
mod contact_sheet;
mod execute;
mod model;
mod operations;
mod pairs;
mod validation;

pub(crate) use operations::apply_batch_operations_canonical;
pub(crate) use operations::{apply_color_replacement, apply_separation};
pub(crate) use validation::validate_operation;

pub use model::{
    BatchColorPair, BatchFailurePolicy, BatchGraph, BatchInputKind, BatchInputSelector,
    BatchItemOutcome, BatchItemResult, BatchMissingTargetPolicy, BatchOperation,
    BatchOperationKind, BatchOutputDestination, BatchOutputFormat, BatchOutputSettings,
    BatchPairCandidate, BatchPairExtraction, BatchPairResolution, BatchPreview, BatchPreviewItem,
    BatchRunOptions, BatchRunReport, BatchRunScope, BatchSeparation, BatchSeparationDestination,
    BatchStagedResult, BatchTargetSelector, SequenceSourceIdentity,
};
