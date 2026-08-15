#![forbid(unsafe_code)]

mod adjustment;
mod application_data;
mod batch;
mod common_formats;
mod cut;
mod inkscript;
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
pub use cut::{
    CUT_DESCRIPTOR_REPLAY_EPOCH, FileCutDefaults, FileCutDescriptor, FileCutHistoryEntry,
    FileCutMemberAsset, FileCutMembership, FileCutMetadata, decode_cut_descriptor,
    encode_cut_descriptor, read_cut_descriptor, save_cut_descriptor_atomic,
    save_cut_recovery_atomic,
};
use inkpod_image::PixelValue;
#[cfg(test)]
use inkpod_image::{PixelFormat, TileCoord};
pub use inkscript::{
    INKSCRIPT_FILE_VERSION, INKSCRIPT_PROCEDURE_CATALOG_VERSION, INKSCRIPT_REQUIRED_REPLAY_EPOCH,
    InkScriptAnalysisLimits, InkScriptAsset, InkScriptBinding, InkScriptCellSelection,
    InkScriptClosedFragment, InkScriptCommandResultSchema, InkScriptCommandSchema, InkScriptCst,
    InkScriptCstNode, InkScriptCstNodeKind, InkScriptDeclarationModel, InkScriptDependencyEdge,
    InkScriptDependencyNode, InkScriptDependencyNodeKind, InkScriptDiagnostic,
    InkScriptDiagnosticCode, InkScriptDiagnosticSeverity, InkScriptDocumentKind,
    InkScriptEnvelopeError, InkScriptEnvelopeErrorCode, InkScriptExecutionFailure,
    InkScriptExecutionPolicy, InkScriptExternalResultBinding, InkScriptFieldSchema,
    InkScriptFragmentRequest, InkScriptFragmentSelection, InkScriptGeneratedNames, InkScriptInput,
    InkScriptInputDeclaration, InkScriptInputDeclarationKind, InkScriptInputKind, InkScriptKeyword,
    InkScriptLexed, InkScriptLexerLimits, InkScriptLineMap, InkScriptMetadata,
    InkScriptMetadataExtension, InkScriptNumberDirection, InkScriptNumberedOutput,
    InkScriptOrchestrationEnvelope, InkScriptOutput, InkScriptOutputFormat, InkScriptParameter,
    InkScriptParsed, InkScriptParserLimits, InkScriptPathIntent, InkScriptPathIntentAccess,
    InkScriptPathIntentPreview, InkScriptProgramStatement, InkScriptPunctuation, InkScriptRecord,
    InkScriptRecordSchema, InkScriptReferenceSegment, InkScriptRequirements, InkScriptResolvedType,
    InkScriptResultAvailability, InkScriptResultCardinality, InkScriptRunParameterChoice,
    InkScriptRunParameterDecision, InkScriptRunParameterValue, InkScriptRunParameters,
    InkScriptSchemaDefault, InkScriptSchemaView, InkScriptSemanticDocument, InkScriptSemanticError,
    InkScriptSemanticErrorCode, InkScriptSemanticSection, InkScriptSource, InkScriptSourceId,
    InkScriptSourcePosition, InkScriptSourceRange, InkScriptSourceSpan, InkScriptStepGroup,
    InkScriptToken, InkScriptTokenKind, InkScriptTypeDiagnostic, InkScriptTypeDiagnosticCode,
    InkScriptTypeReference, InkScriptTypedAsset, InkScriptTypedBinding, InkScriptTypedParameter,
    InkScriptTypedStep, InkScriptTypedStepResult, InkScriptTypedValue, InkScriptTypedValueKind,
    InkScriptValue, MAX_INKSCRIPT_BINDINGS, MAX_INKSCRIPT_CONTAINER_ELEMENTS,
    MAX_INKSCRIPT_CST_NODES, MAX_INKSCRIPT_DEPENDENCY_EDGES, MAX_INKSCRIPT_DIAGNOSTICS,
    MAX_INKSCRIPT_IDENTIFIER_BYTES, MAX_INKSCRIPT_INLINE_ASSET_BYTES, MAX_INKSCRIPT_INPUTS,
    MAX_INKSCRIPT_LIST_ELEMENTS, MAX_INKSCRIPT_NESTING_DEPTH, MAX_INKSCRIPT_NUMERIC_BYTES,
    MAX_INKSCRIPT_PARAMETERS, MAX_INKSCRIPT_PROGRAM_STATEMENTS, MAX_INKSCRIPT_REFERENCE_SEGMENTS,
    MAX_INKSCRIPT_SECTIONS, MAX_INKSCRIPT_SOURCE_BYTES, MAX_INKSCRIPT_STRING_BYTES,
    MAX_INKSCRIPT_TOKENS, MAX_INKSCRIPT_WAIT_MS, build_inkscript_declaration_model,
    build_inkscript_declaration_model_with_limits, build_inkscript_orchestration_envelope,
    build_inkscript_semantic, close_inkscript_fragment, emit_inkscript_canonical, lex_inkscript,
    lex_inkscript_with_limits, parse_inkscript, parse_inkscript_with_limits,
    resolve_inkscript_run_parameters,
};
pub use light_table::{
    FileLightTableItem, FileLightTableMetadata, FileLightTableSet, LightTableDisplayMode,
};
pub use native::{
    DocumentArchive, FileAnnotationKind, FileAnnotationObject, FileAnnotationOutput,
    FileAnnotationPoint, FileDocumentMetadata, FileGrid, FileGuide, FileLayer, FilePlane,
    FilePlaneProperties, FileShootingFrame, FileShootingFrameAnchor, FileTile, FileVanishingPoint,
    FormatError, FrameMetadata, GuideAxis, LayerKind, Margins, PlaneKind, RectI32, checksum,
    discard_recovery, recovery_is_newer,
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
