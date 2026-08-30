mod decode;
mod encode;
mod io;
mod model;
mod validate;

pub(crate) use decode::Reader;
pub use decode::decode_document_archive;
pub use encode::encode_document_archive;
pub(crate) use encode::{push_color_value, push_i32, push_u32, push_u64};
pub use io::{discard_recovery, recovery_is_newer};
#[cfg(test)]
pub(crate) use io::{read, save_atomic, save_atomic_with_cancel};
#[cfg(test)]
pub(crate) use model::TEMP_SEQUENCE;
pub use model::{
    DocumentArchive, FileDocumentMetadata, FileGrid, FileGuide, FileLayer, FilePlane,
    FilePlaneProperties, FileSavedSelection, FileShootingFrame, FileShootingFrameAnchor, FileTile,
    FormatError, FrameMetadata, GuideAxis, Margins, PlaneKind, RectI32, checksum,
};
pub(crate) use model::{MAX_MANIFEST_BYTES, MAX_NODE_NAME_BYTES, MAX_PLANES};
