//! C ABI adapters for batch processing.

use super::*;
#[cfg(test)]
use inkpod_core::BATCH_OPERATION_VERSION;
use inkpod_core::{
    BatchColorPair, BatchFailurePolicy, BatchGraph, BatchInputKind, BatchInputSelector,
    BatchItemOutcome, BatchMissingTargetPolicy, BatchOperation, BatchOperationKind,
    BatchOutputPolicy, BatchOutputSettings, BatchPairExtraction, BatchRunOptions, BatchRunScope,
    BatchSeed, BatchSeparation, BatchSeparationDestination, BatchTargetSelector, DocumentResize,
    LayerKind, MirrorAxis, PixelFormat, PlaneType, ResizeAnchor, RotateDirection,
    SequenceSourceIdentity,
};
use std::path::PathBuf;

pub const INKPOD_BATCH_INPUT_FILE: u32 = 1;
pub const INKPOD_BATCH_INPUT_FOLDER: u32 = 2;
pub const INKPOD_BATCH_INPUT_CURRENT_SEQUENCE: u32 = 3;

pub const INKPOD_BATCH_OUTPUT_DUPLICATE: u32 = 1;
pub const INKPOD_BATCH_OUTPUT_NEW_SAVE: u32 = 2;
pub const INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE: u32 = 3;
pub const INKPOD_BATCH_FAILURE_CONTINUE: u32 = 1;
pub const INKPOD_BATCH_FAILURE_STOP: u32 = 2;
pub const INKPOD_BATCH_MISSING_SKIP: u32 = 1;
pub const INKPOD_BATCH_MISSING_ERROR: u32 = 2;

pub const INKPOD_BATCH_OPERATION_COLOR_REPLACE: u32 = 1;
pub const INKPOD_BATCH_OPERATION_CONTINUOUS_FILL: u32 = 2;
pub const INKPOD_BATCH_OPERATION_SEPARATION: u32 = 3;
pub const INKPOD_BATCH_OPERATION_VISIBILITY: u32 = 4;
// Operation code 5 is retired and intentionally remains unassigned.
pub const INKPOD_BATCH_OPERATION_FILTER: u32 = 6;
pub const INKPOD_BATCH_OPERATION_BOUNDARY_AIRBRUSH: u32 = 7;
pub const INKPOD_BATCH_OPERATION_DUST_REMOVAL: u32 = 8;
pub const INKPOD_BATCH_OPERATION_MIRROR: u32 = 9;
pub const INKPOD_BATCH_OPERATION_ROTATE_90: u32 = 10;
pub const INKPOD_BATCH_OPERATION_RESIZE: u32 = 11;
pub const INKPOD_BATCH_OPERATION_CONVERT_PLANE: u32 = 12;

pub const INKPOD_BATCH_OPERATION_ENABLED: u64 = 1;
pub const INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN: u64 = 1 << 1;
pub const INKPOD_BATCH_OUTPUT_CELL_FOLDER: u64 = 1;
pub const INKPOD_BATCH_OUTPUT_DESCENDING: u64 = 1 << 1;
pub const INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE: u64 = 1 << 2;
pub const INKPOD_BATCH_SEPARATION_INVERT: i64 = 1;
pub const INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR: u32 = 1;
pub const INKPOD_BATCH_SEED_ENABLED: u32 = 1 << 1;
pub const INKPOD_BATCH_SEPARATION_REPLACE_SOURCE: i64 = 1;
pub const INKPOD_BATCH_SEPARATION_SELECTION_MASK: i64 = 2;
pub const INKPOD_BATCH_SEPARATION_MAIN_LINE_PLANE: i64 = 3;
pub const INKPOD_BATCH_SEPARATION_COLOR_PLANE: i64 = 4;
pub const INKPOD_BATCH_SEPARATION_NATIVE_FILE: i64 = 5;

pub const INKPOD_BATCH_SCOPE_CURRENT: u32 = 1;
pub const INKPOD_BATCH_SCOPE_ALL: u32 = 2;
pub const INKPOD_BATCH_RUN_DRY: u64 = 1;
pub const INKPOD_BATCH_RUN_PREVIEW_CONFIRMED: u64 = 1 << 1;

pub const INKPOD_BATCH_ITEM_SUCCEEDED: u32 = 1;
pub const INKPOD_BATCH_ITEM_SKIPPED: u32 = 2;
pub const INKPOD_BATCH_ITEM_FAILED: u32 = 3;
pub const INKPOD_BATCH_ITEM_CANCELLED: u32 = 4;
pub const INKPOD_BATCH_ITEM_DRY_RUN: u32 = 5;
pub const INKPOD_BATCH_PREVIEW_HAS_WARNING: u32 = 1;
#[cfg(test)]
pub const INKPOD_BATCH_GRAPH_VERSION: u32 = 2;
pub const INKPOD_BATCH_PAIR_CANDIDATE_AMBIGUOUS: u32 = 1;

const MAX_BATCH_INPUTS: usize = 16_384;
const MAX_BATCH_OPERATIONS: usize = 1_024;
const MAX_BATCH_PAIRS: usize = 4_096;
const MAX_BATCH_SEEDS: usize = 4_096;
const MAX_BATCH_TEXT_BYTES: u64 = 32_768;

mod exports;
mod parse;
mod query;
mod records;

#[cfg(test)]
pub(super) use exports::*;
use parse::*;
#[cfg(test)]
pub(super) use query::*;
pub use records::*;

#[cfg(test)]
#[path = "../../tests/unit/batch.rs"]
mod tests;
