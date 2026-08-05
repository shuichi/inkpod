//! Animation workflow state: light table, sequences, and motion.

use super::*;
use crate::document::{bounded_document_pixels, validate_node_name};
use crate::snapshot::{blend_rgba_over, rgba8_for_display};
pub use inkpod_format::LightTableDisplayMode;
use inkpod_format::{decode_common_raster, encode_common_raster};
use std::cmp::Ordering;

const MAX_SEQUENCE_CELLS: usize = 10_000;
const MAX_LIGHT_TABLE_SETS: usize = 256;
const MAX_LIGHT_TABLE_ITEMS: usize = 4_096;
const THUMBNAIL_MAX_DIMENSION: u32 = 64;

mod io;
mod light_table;
mod light_table_operations;
mod ordering;
mod raster;
mod sequence;
mod sequence_operations;

pub(crate) use light_table::LightTableState;
pub use light_table::{
    LightTableItemInfo, LightTableItemInput, LightTableItemProperties, LightTableSetInfo,
    LightTableSource, RgbaRasterBytes,
};
pub(crate) use ordering::{natural_cmp, parse_cell_number};
#[cfg(test)]
pub(crate) use raster::base_raster_pixel;
pub use sequence::{
    MotionCheckConfig, MotionFrame, SequenceCellInfo, SequenceCellSource, SequenceDirection,
    Thumbnail,
};
pub(crate) use sequence::{MotionCheckState, SequenceState};
