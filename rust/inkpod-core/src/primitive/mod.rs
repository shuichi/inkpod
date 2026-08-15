//! Canonical document primitive requests, procedures, replay, and state digests.

mod catalog;
mod digest;
mod display;
mod executor;
#[allow(
    dead_code,
    reason = "the private legacy-simple adapter stays disconnected until the compiler owner"
)]
pub(crate) mod inkscript;
#[allow(
    dead_code,
    reason = "the private legacy-image adapter stays disconnected until the compiler owner"
)]
pub(crate) mod inkscript_batch;
#[allow(
    dead_code,
    reason = "the private document-tree adapter is connected only through the script runner"
)]
pub(crate) mod inkscript_document_tree;
#[allow(
    dead_code,
    reason = "the private metadata/color/guide adapter is connected only through the script runner"
)]
pub(crate) mod inkscript_metadata;
mod inkscript_reference;
#[allow(
    dead_code,
    reason = "the private stroke/geometry/import adapter is connected only through the script runner"
)]
pub(crate) mod inkscript_stroke_geometry;
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
pub(crate) use display::display_procedure;
pub(crate) use executor::validate_persisted_procedure;
pub(crate) use inkscript::{LegacySimpleAdapterError, LegacySimpleScriptStep};
pub(crate) use inkscript_batch::{LegacyImageAdapterError, LegacyImageScriptStep};
pub(crate) use inkscript_document_tree::{DocumentTreeAdapterError, DocumentTreeScriptStep};
pub(crate) use inkscript_metadata::{MetadataColorGuideAdapterError, MetadataColorGuideScriptStep};
pub(crate) use inkscript_reference::{InkScriptEntityKind, InkScriptRuntimeReferences};
pub(crate) use inkscript_stroke_geometry::{
    StrokeGeometryImportAction, StrokeGeometryImportAdapterError,
};
pub(crate) use invocation::{CanonicalInvocation, InvocationResult, RuntimeInvocation};
use model::CanonicalPrimitive;
pub(crate) use model::CanonicalStrokeArguments;
pub(crate) use raster::{
    RasterStrokePreview, apply as apply_raster_stroke, begin_preview as begin_stroke_preview,
    canonicalize as canonicalize_stroke, canonicalize_exact as canonicalize_exact_stroke,
    validate_public_stroke as validate_stroke_request,
};
