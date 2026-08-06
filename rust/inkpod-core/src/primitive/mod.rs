//! Canonical document primitive requests, procedures, replay, and state digests.

mod digest;
mod executor;
mod invocation;
mod model;
mod raster;

pub use model::{
    CanonicalProcedure, DocumentStateDigest, PrimitiveId, PrimitiveOutcome, PrimitiveRequest,
    ProcedureId, ReplayEpoch, StateId,
};

use digest::canonical_payload_digest;
pub(crate) use digest::{CanonicalDocumentStateCache, canonical_document_state};
pub(crate) use invocation::{CanonicalInvocation, InvocationResult};
use model::CanonicalPrimitive;
pub(crate) use model::CanonicalStrokeArguments;
pub(crate) use raster::{
    RasterStrokePreview, begin_preview as begin_stroke_preview,
    canonicalize as canonicalize_stroke, canonicalize_exact as canonicalize_exact_stroke,
    validate_public_stroke as validate_stroke_request,
};
