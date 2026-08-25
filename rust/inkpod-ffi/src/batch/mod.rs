//! C ABI adapters for batch processing.

use super::*;
#[cfg(test)]
use inkpod_core::BATCH_OPERATION_VERSION;
use inkpod_core::{
    BatchColorPair, BatchFailurePolicy, BatchGraph, BatchInputKind, BatchInputSelector,
    BatchItemOutcome, BatchMissingTargetPolicy, BatchOperation, BatchOperationKind,
    BatchOutputDestination, BatchOutputFormat, BatchOutputSettings, BatchPairExtraction,
    BatchRunOptions, BatchRunReport, BatchRunScope, BatchStagedResult, BatchTargetSelector,
    CoreError, LayerKind, PlaneType, SequenceSourceIdentity,
};
use std::path::PathBuf;

pub const INKPOD_BATCH_INPUT_FILE: u32 = 1;
pub const INKPOD_BATCH_INPUT_FOLDER: u32 = 2;
pub const INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT: u32 = 3;

pub const INKPOD_BATCH_OUTPUT_FOLDER: u32 = 1;
pub const INKPOD_BATCH_OUTPUT_ACTIVE_DOCUMENT: u32 = 2;
pub const INKPOD_BATCH_OUTPUT_NEW_TABS: u32 = 3;
pub const INKPOD_BATCH_FORMAT_INKPOD: u32 = 1;
pub const INKPOD_BATCH_FORMAT_PNG: u32 = 2;
pub const INKPOD_BATCH_FORMAT_TIFF: u32 = 3;
pub const INKPOD_BATCH_FORMAT_TGA: u32 = 4;
pub const INKPOD_BATCH_FORMAT_BMP: u32 = 5;
pub const INKPOD_BATCH_FAILURE_CONTINUE: u32 = 1;
pub const INKPOD_BATCH_FAILURE_STOP: u32 = 2;
pub const INKPOD_BATCH_MISSING_SKIP: u32 = 1;
pub const INKPOD_BATCH_MISSING_ERROR: u32 = 2;

pub const INKPOD_BATCH_OPERATION_COLOR_REPLACE: u32 = 1;
pub const INKPOD_BATCH_OPERATION_MOVE_TO_COLOR_PLANE: u32 = 2;
pub const INKPOD_BATCH_OPERATION_MASKING: u32 = 3;
pub const INKPOD_BATCH_OPERATION_ERASE: u32 = 4;

pub const INKPOD_BATCH_OPERATION_ENABLED: u64 = 1;
pub const INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE: u64 = 1;

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
pub const INKPOD_BATCH_GRAPH_VERSION: u32 = 4;
pub const INKPOD_BATCH_PAIR_CANDIDATE_AMBIGUOUS: u32 = 1;

const MAX_BATCH_INPUTS: usize = 16_384;
const MAX_BATCH_OPERATIONS: usize = 1_024;
const MAX_BATCH_TARGETS: usize = 64;
const MAX_BATCH_PAIRS: usize = 4_096;
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
