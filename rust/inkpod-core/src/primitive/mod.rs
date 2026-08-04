//! Canonical document primitive requests, procedures, replay, and state digests.

mod digest;
mod executor;
mod model;
mod raster;

pub use model::{
    CanonicalProcedure, DocumentStateDigest, PrimitiveId, PrimitiveOutcome, PrimitiveRequest,
    ProcedureId, ReplayEpoch, StateId,
};

use digest::{canonical_document_state, canonical_payload_digest};
use model::{CanonicalPrimitive, CanonicalStrokeArguments};
pub(crate) use raster::{
    RasterStrokePreview, begin_preview as begin_stroke_preview,
    canonicalize as canonicalize_stroke, validate_public_stroke as validate_stroke_request,
};
