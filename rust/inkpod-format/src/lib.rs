#![forbid(unsafe_code)]

mod adjustment;
mod application_data;
mod batch;
mod common_formats;
mod light_table;
mod native;
mod procedure;
mod vector;

pub use adjustment::{FileAdjustmentLayer, FileAdjustmentMetadata, MAX_ADJUSTMENT_LAYERS};
pub use application_data::{
    ApplicationColor, COLOR_CHART_FORMAT_VERSION, FileColorChart, FileColorChartEntry, FilePalette,
    MAX_APPLICATION_COLORS, MAX_COLOR_CHART_NAME_BYTES, PALETTE_FORMAT_VERSION, decode_color_chart,
    decode_palette, encode_color_chart, encode_palette, read_color_chart, read_palette,
    save_color_chart_atomic, save_palette_atomic,
};
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
pub use native::{
    DocumentArchive, FileDocumentMetadata, FileGrid, FileGuide, FileLayer, FilePlane,
    FilePlaneProperties, FileTile, FormatError, FrameMetadata, GuideAxis, LayerKind, Margins,
    PlaneKind, RectI32, checksum, discard_recovery, recovery_is_newer,
};
use native::{
    MAX_MANIFEST_BYTES, MAX_NODE_NAME_BYTES, MAX_PLANES, Reader, push_color_value, push_i32,
    push_u32, push_u64,
};
#[cfg(test)]
use native::{TEMP_SEQUENCE, read, save_atomic, save_atomic_with_cancel};
#[cfg(test)]
use native::{decode_document_archive as decode, encode_document_archive as encode};
pub use native::{decode_document_archive, encode_document_archive};
pub use procedure::{
    FORMAT_VERSION, NativeFile, NativeRecord, NativeSection, OPAQUE_PRESERVE, SECTION_CRITICAL,
    decode_procedure_file, encode_procedure_file, read_procedure_file, save_procedure_file_atomic,
    save_procedure_file_atomic_with_cancel, save_recovery_procedure_file_atomic,
};
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::sync::atomic::Ordering;
pub use vector::{
    FileVectorConnection, FileVectorEndpoint, FileVectorFill, FileVectorMetadata, FileVectorPath,
    FileVectorPoint, FileVectorSegment, MAX_VECTOR_BOUNDARIES, MAX_VECTOR_CONNECTIONS,
    MAX_VECTOR_FILLS, MAX_VECTOR_PATHS, MAX_VECTOR_SEGMENTS,
};

#[cfg(test)]
#[path = "../tests/unit/native.rs"]
mod tests;
