#![forbid(unsafe_code)]

mod adjustment;
mod batch;
mod common_formats;
mod light_table;
mod native;
mod vector;

pub use adjustment::{FileAdjustmentLayer, FileAdjustmentMetadata, MAX_ADJUSTMENT_LAYERS};
pub use batch::{
    BATCH_GRAPH_VERSION, FileBatchGraph, FileBatchInput, FileBatchOperation, FileBatchOutput,
    FileBatchTarget, decode_batch_graph, encode_batch_graph, read_batch_graph,
    save_batch_graph_atomic, save_batch_graph_atomic_with_cancel,
};
pub use common_formats::{
    CommonRaster, CommonRasterFormat, CommonRasterInfo, MAX_COMMON_RASTER_BYTES,
    decode_common_raster, encode_common_raster,
};
use inkpod_image::PixelValue;
#[cfg(test)]
use inkpod_image::{PixelFormat, TileCoord};
pub use light_table::{
    FileLightTableItem, FileLightTableMetadata, FileLightTableSet, LightTableDisplayMode,
};
#[cfg(test)]
use native::TEMP_SEQUENCE;
pub use native::{
    CellFile, FORMAT_VERSION, FileDocumentMetadata, FileGrid, FileGuide, FileLayer, FilePlane,
    FilePlaneProperties, FileTile, FormatError, FrameMetadata, GuideAxis, LayerKind, Margins,
    PlaneKind, RectI32, checksum, decode, discard_recovery, encode, read, recovery_is_newer,
    save_atomic, save_atomic_with_cancel, save_recovery_atomic,
};
use native::{
    MAX_MANIFEST_BYTES, MAX_NODE_NAME_BYTES, MAX_PLANES, Reader, push_color_value, push_i32,
    push_u32, push_u64,
};
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::sync::atomic::Ordering;
pub use vector::{
    FileVectorFill, FileVectorMetadata, FileVectorPath, FileVectorPoint, FileVectorSegment,
    MAX_VECTOR_BOUNDARIES, MAX_VECTOR_FILLS, MAX_VECTOR_PATHS, MAX_VECTOR_SEGMENTS,
};

#[cfg(test)]
#[path = "../tests/unit/native.rs"]
mod tests;
