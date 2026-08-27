//! Versioned path-only file I/O adapters; no filesystem or document logic here.

use super::*;
use inkpod_core::{FileIoApply, FileIoJob, FileIoKind, FileIoRequest, FileIoState};
use inkpod_io::{IoConfig, IoManager};
use std::sync::{Mutex, MutexGuard};

mod boundary;
mod exports;
mod parse;
mod query;
mod recovery;
mod session;

#[cfg(test)]
#[path = "../../tests/unit/file_io.rs"]
mod tests;

pub use boundary::{InkpodIoJob, InkpodIoManager};
pub(crate) use boundary::{empty_owner, io_boundary, job_lock, manager_ref, owner_core};
pub use exports::*;
pub use query::*;
pub use recovery::*;
pub use session::*;
