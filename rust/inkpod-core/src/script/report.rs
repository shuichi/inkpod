use crate::DocumentStateDigest;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScriptStatementOutcome {
    AssertPassed,
    Disabled,
    Skipped,
    NoOp,
    Committed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScriptResultValue {
    pub(crate) alias: String,
    pub(crate) field: String,
    pub(crate) output_id_ordinal: u16,
    pub(crate) persistent_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScriptDryRunReport {
    pub(crate) statements: Vec<ScriptStatementOutcome>,
    pub(crate) commit_count: u64,
    pub(crate) results: Vec<ScriptResultValue>,
    pub(crate) final_state_digest: DocumentStateDigest,
    pub(crate) final_revision: u64,
    pub(crate) next_stable_id: u64,
}
