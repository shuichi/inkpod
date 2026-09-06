//! Exact-current InkScript compile, bind, and private staged-run API.
//!
//! Compilation and dry-run own immutable values and isolated document results. The shared
//! [`ScriptIoAdapter`] delegates filesystem work to `inkpod-io`; image previews use temporary
//! copies and clean them before returning. Active output requires an explicit owner-thread
//! [`ScriptStagedResult::apply_active`] with the original session and complete persistence token.
//! New staged-result C ABI/Windows integration and product cutover remain separate gates.

pub use crate::script::{
    CapturedScriptInput, CatalogError, InMemoryInputFingerprint, InkScriptBindingError,
    InkScriptExportError, InkScriptExportLimits, InkScriptExportPortability,
    InkScriptFragmentExport, ScriptAssetError, ScriptBudget, ScriptCompileError,
    ScriptCompileLimits, ScriptDryRunReport, ScriptDryRunResult, ScriptImagePreviewError,
    ScriptImagePreviewLimits, ScriptImagePreviewResult, ScriptIoAdapter, ScriptPathIntentSubject,
    ScriptResultValue, ScriptRunError, ScriptStagedResult, ScriptStagedResultKind,
    ScriptStatementOutcome, ScriptStaticPathIntent, StaticScriptProgram,
    capture_in_memory_fingerprint, capture_in_memory_input, capture_in_memory_input_at,
    compile_inkscript, compile_inkscript_with_limits, export_inkscript_fragment,
    export_inkscript_fragment_with_limits, native_script_input, preview_inkscript_images,
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
        ScriptPlanLimits, ScriptPlannedDestination, ScriptPreparedDestination, ScriptRunAdapter,
        ScriptRunAdapterError, ScriptRunAdvance, ScriptRunItemReport, ScriptRunLimits,
        ScriptRunMode, ScriptRunReport, ScriptRunScope, ScriptRunStartError, ScriptRunTask,
        ScriptSequenceExpectation, ScriptSequenceMemberSnapshot, ScriptSequenceSnapshot,
        ScriptSessionExpectation, ScriptSessionSnapshot, ScriptTemporaryIdentity,
        ValidatedPathIdentity, issue_confirmation_token, plan_inkscript, start_inkscript_run,
    };
}
pub use inkpod_format::{
    InkScriptRunParameterChoice, InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId,
    InkScriptValue,
};
