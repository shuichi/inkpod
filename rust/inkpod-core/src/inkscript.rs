//! Exact-current InkScript compile, bind, and private staged-run API.
//!
//! This Rust-only surface owns compiled values and reports, borrows captured input, never mutates
//! the source [`crate::Core`], and performs no OS path access. Multi-item authority/install
//! adapters, the C ABI, and Windows product routing remain intentionally unavailable until their
//! owner milestones.

pub use crate::script::{
    CapturedScriptInput, CatalogError, InMemoryInputFingerprint, InkScriptBindingError,
    InkScriptExportError, InkScriptExportLimits, InkScriptExportPortability,
    InkScriptFragmentExport, ScriptAssetError, ScriptBudget, ScriptCompileError,
    ScriptCompileLimits, ScriptDryRunReport, ScriptDryRunResult, ScriptPathIntentSubject,
    ScriptResultValue, ScriptRunError, ScriptStatementOutcome, ScriptStaticPathIntent,
    StaticScriptProgram, capture_in_memory_fingerprint, capture_in_memory_input,
    capture_in_memory_input_at, compile_inkscript, compile_inkscript_with_limits,
    export_inkscript_fragment, export_inkscript_fragment_with_limits, native_script_input,
    run_inkscript_dry,
};
pub use inkpod_format::{
    InkScriptRunParameterChoice, InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId,
    InkScriptValue,
};
