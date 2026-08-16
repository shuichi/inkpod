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

/// Internal safe-Rust bridge used by the versioned C ABI implementation.
///
/// These values keep OS handles and callbacks outside Core. They are intentionally hidden from
/// the normal Rust documentation; the stable ownership and thread contract is the C header.
#[doc(hidden)]
pub mod abi_bridge {
    pub use crate::script::{
        AuthorityGrant, AuthoritySnapshot, AuthorizedAssetIdentity, AuthorizedAssetReadError,
        AuthorizedAssetReader, AuthorizedAssetStream, FolderScan, NativeInputFingerprint,
        OpenSessionRecord, OpenSessionSetSnapshot, ScriptAtomicCapabilities,
        ScriptAtomicInstallResult, ScriptCommandContext, ScriptConfirmationToken,
        ScriptDestinationBase, ScriptDestinationRequest, ScriptExecutionPlan,
        ScriptExecutionPreviewItem, ScriptItemFailure, ScriptItemOutcome, ScriptNativeRead,
        ScriptOverwriteGuard, ScriptPlanAdapter, ScriptPlanAdapterError, ScriptPlanError,
        ScriptPlanLimits, ScriptPreparedDestination, ScriptRunAdapter, ScriptRunAdapterError,
        ScriptRunAdvance, ScriptRunItemReport, ScriptRunLimits, ScriptRunMode, ScriptRunReport,
        ScriptRunScope, ScriptRunStartError, ScriptRunTask, ScriptSequenceExpectation,
        ScriptSequenceMemberSnapshot, ScriptSequenceSnapshot, ScriptSessionExpectation,
        ScriptSessionSnapshot, ScriptTemporaryIdentity, ValidatedPathIdentity,
        issue_confirmation_token, plan_inkscript, start_inkscript_run,
    };
}
pub use inkpod_format::{
    InkScriptRunParameterChoice, InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId,
    InkScriptValue,
};
