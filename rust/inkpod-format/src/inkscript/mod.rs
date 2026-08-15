mod bind;
mod catalog;
mod diagnostic;
mod emit;
mod envelope;
mod fragment;
mod lexer;
mod names;
mod parser;
mod schema;
mod source;
mod syntax;
mod types;

// Catalog ratification is the first point allowed to re-export this route. Keeping a typed private
// reference here includes it in normal builds without exposing a production catalog.
#[allow(dead_code)]
const PRIVATE_INKSCRIPT_PREPARATION_ROUTE: fn(
    &types::InkScriptDeclarationModel,
    &schema::InkScriptSchemaView<'_>,
    &catalog::InkScriptCatalogView,
    &bind::InkScriptInitialDocumentSnapshot,
) -> Result<
    bind::InkScriptInitialPreparation,
    bind::InkScriptBindingError,
> = bind::prepare_inkscript_initial_state;

pub use diagnostic::{
    InkScriptDiagnostic, InkScriptDiagnosticCode, InkScriptDiagnosticSeverity, InkScriptSourceId,
    InkScriptSourcePosition, InkScriptSourceRange, InkScriptSourceSpan,
};
pub use emit::emit_inkscript_canonical;
pub use envelope::{
    InkScriptCellSelection, InkScriptEnvelopeError, InkScriptEnvelopeErrorCode,
    InkScriptExecutionFailure, InkScriptExecutionPolicy, InkScriptInputDeclaration,
    InkScriptInputDeclarationKind, InkScriptMetadata, InkScriptMetadataExtension,
    InkScriptNumberDirection, InkScriptNumberedOutput, InkScriptOrchestrationEnvelope,
    InkScriptOutput, InkScriptOutputFormat, InkScriptPathIntent, InkScriptPathIntentAccess,
    InkScriptPathIntentPreview, InkScriptRequirements, MAX_INKSCRIPT_WAIT_MS,
    build_inkscript_orchestration_envelope,
};
pub use fragment::{
    InkScriptClosedFragment, InkScriptExternalResultBinding, InkScriptFragmentRequest,
    InkScriptFragmentSelection, close_inkscript_fragment,
};
pub use lexer::{
    InkScriptKeyword, InkScriptLexed, InkScriptPunctuation, InkScriptToken, InkScriptTokenKind,
    lex_inkscript, lex_inkscript_with_limits,
};
pub use names::InkScriptGeneratedNames;
pub use parser::{
    InkScriptCst, InkScriptCstNode, InkScriptCstNodeKind, InkScriptDocumentKind, InkScriptParsed,
    InkScriptParserLimits, MAX_INKSCRIPT_BINDINGS, MAX_INKSCRIPT_CONTAINER_ELEMENTS,
    MAX_INKSCRIPT_CST_NODES, MAX_INKSCRIPT_INPUTS, MAX_INKSCRIPT_LIST_ELEMENTS,
    MAX_INKSCRIPT_NESTING_DEPTH, MAX_INKSCRIPT_PARAMETERS, MAX_INKSCRIPT_PROGRAM_STATEMENTS,
    MAX_INKSCRIPT_REFERENCE_SEGMENTS, MAX_INKSCRIPT_SECTIONS, parse_inkscript,
    parse_inkscript_with_limits,
};
pub use schema::{
    INKSCRIPT_PROCEDURE_CATALOG_VERSION, INKSCRIPT_REQUIRED_REPLAY_EPOCH,
    InkScriptCommandResultSchema, InkScriptCommandSchema, InkScriptFieldSchema,
    InkScriptRecordSchema, InkScriptResultAvailability, InkScriptResultCardinality,
    InkScriptSchemaDefault, InkScriptSchemaView, InkScriptSemanticError,
    InkScriptSemanticErrorCode,
};
pub use source::{
    INKSCRIPT_FILE_VERSION, InkScriptLexerLimits, InkScriptLineMap, InkScriptSource,
    MAX_INKSCRIPT_DIAGNOSTICS, MAX_INKSCRIPT_IDENTIFIER_BYTES, MAX_INKSCRIPT_INLINE_ASSET_BYTES,
    MAX_INKSCRIPT_NUMERIC_BYTES, MAX_INKSCRIPT_SOURCE_BYTES, MAX_INKSCRIPT_STRING_BYTES,
    MAX_INKSCRIPT_TOKENS,
};
pub use syntax::{
    InkScriptAsset, InkScriptBinding, InkScriptInput, InkScriptInputKind, InkScriptParameter,
    InkScriptProgramStatement, InkScriptRecord, InkScriptReferenceSegment,
    InkScriptSemanticDocument, InkScriptSemanticSection, InkScriptTypeReference, InkScriptValue,
    build_inkscript_semantic,
};
pub use types::{
    InkScriptAnalysisLimits, InkScriptDeclarationModel, InkScriptDependencyEdge,
    InkScriptDependencyNode, InkScriptDependencyNodeKind, InkScriptResolvedType,
    InkScriptRunParameterChoice, InkScriptRunParameterDecision, InkScriptRunParameterValue,
    InkScriptRunParameters, InkScriptStepGroup, InkScriptTypeDiagnostic,
    InkScriptTypeDiagnosticCode, InkScriptTypedAsset, InkScriptTypedBinding,
    InkScriptTypedParameter, InkScriptTypedStep, InkScriptTypedStepResult, InkScriptTypedValue,
    InkScriptTypedValueKind, MAX_INKSCRIPT_DEPENDENCY_EDGES, build_inkscript_declaration_model,
    build_inkscript_declaration_model_with_limits, resolve_inkscript_run_parameters,
};
