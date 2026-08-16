//! Exact-current InkScript compilation, initial binding, and staged execution.

#![allow(
    dead_code,
    reason = "later planning and ABI owners remain crate-private"
)]

mod assets;
pub(crate) mod bind;
mod catalog;
mod compile;
mod execute;
mod export;
#[cfg(test)]
mod performance;
mod plan;
mod report;
mod run;

pub use assets::ScriptAssetError;
pub use bind::InkScriptBindingError;
pub use catalog::CatalogError;
pub use compile::{
    ScriptBudget, ScriptCompileError, ScriptCompileLimits, ScriptPathIntentSubject,
    ScriptStaticPathIntent, StaticScriptProgram, compile_inkscript, compile_inkscript_with_limits,
};
pub use execute::{
    CapturedScriptInput, InMemoryInputFingerprint, ScriptDryRunResult, ScriptRunError,
    capture_in_memory_fingerprint, capture_in_memory_input, capture_in_memory_input_at,
    native_script_input, run_inkscript_dry,
};
pub use export::{
    InkScriptExportError, InkScriptExportLimits, InkScriptExportPortability,
    InkScriptFragmentExport, export_inkscript_fragment, export_inkscript_fragment_with_limits,
};
pub use report::{ScriptDryRunReport, ScriptResultValue, ScriptStatementOutcome};

#[cfg(test)]
mod tests;
