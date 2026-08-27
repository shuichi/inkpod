//! Filesystem job orchestration. Workers never borrow a live single-writer Core.

mod apply;
mod batch;
mod job;
mod model;
mod prepare;
mod recovery;
mod session;

pub use job::FileIoJob;
pub(crate) use model::SavedPair;
pub use model::{FileIoApply, FileIoItem, FileIoKind, FileIoProgress, FileIoRequest, FileIoState};
