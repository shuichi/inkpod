//! Canonical document primitive requests, procedures, replay, and state digests.

mod catalog;
mod digest;
mod executor;
mod invocation;
mod model;
mod raster;

pub use model::{
    CANONICAL_NUMERIC_VERSION, CanonicalProcedure, DocumentStateDigest, PROCEDURE_FORMAT_VERSION,
    PrimitiveId, PrimitiveOutcome, PrimitiveRequest, ProcedureId, ReplayContract, ReplayEpoch,
    StateId,
};

pub use catalog::replay_contract;

use digest::canonical_payload_digest;
pub(crate) use digest::{CanonicalDocumentStateCache, canonical_document_state};
pub(crate) use executor::validate_persisted_procedure;
pub(crate) use invocation::{CanonicalInvocation, InvocationResult, RuntimeInvocation};
use model::CanonicalPrimitive;
pub(crate) use model::CanonicalStrokeArguments;
pub(crate) use raster::{
    RasterStrokePreview, apply as apply_raster_stroke, begin_preview as begin_stroke_preview,
    canonicalize as canonicalize_stroke, canonicalize_exact as canonicalize_exact_stroke,
    validate_public_stroke as validate_stroke_request,
};
