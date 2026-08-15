mod diagnostic;
mod emit;
mod lexer;
mod names;
mod parser;
mod schema;
mod source;
mod syntax;

pub use diagnostic::{
    InkScriptDiagnostic, InkScriptDiagnosticCode, InkScriptDiagnosticSeverity, InkScriptSourceId,
    InkScriptSourcePosition, InkScriptSourceRange, InkScriptSourceSpan,
};
pub use emit::emit_inkscript_canonical;
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
    InkScriptCommandSchema, InkScriptFieldSchema, InkScriptRecordSchema, InkScriptSchemaDefault,
    InkScriptSchemaView, InkScriptSemanticError, InkScriptSemanticErrorCode,
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
