use crate::DocumentStateDigest;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Outcome of one enabled, disabled, asserted, skipped, no-op, or committed program statement.
pub enum ScriptStatementOutcome {
    /// An initial-state assertion passed.
    AssertPassed,
    /// A statically disabled step was not executed.
    Disabled,
    /// Dependency or missing-policy evaluation skipped the statement.
    Skipped,
    /// The invocation succeeded without publishing a new history state.
    NoOp,
    /// The invocation published exactly one canonical commit.
    Committed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One stable identifier materialized from a typed command result field.
pub struct ScriptResultValue {
    pub(crate) alias: String,
    pub(crate) field: String,
    pub(crate) output_id_ordinal: u16,
    pub(crate) persistent_id: u64,
}

impl ScriptResultValue {
    /// Returns the step-result alias.
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// Returns the exact catalog result-field name.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// Returns the canonical output-ID ordinal within the invocation.
    pub const fn output_id_ordinal(&self) -> u16 {
        self.output_id_ordinal
    }

    /// Returns the nonzero persistent identifier created by the invocation.
    pub const fn persistent_id(&self) -> u64 {
        self.persistent_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Deterministic report for one successfully staged input document.
pub struct ScriptDryRunReport {
    pub(crate) statements: Vec<ScriptStatementOutcome>,
    pub(crate) commit_count: u64,
    pub(crate) results: Vec<ScriptResultValue>,
    pub(crate) final_state_digest: DocumentStateDigest,
    pub(crate) final_revision: u64,
    pub(crate) next_stable_id: u64,
}

impl ScriptDryRunReport {
    /// Returns statement outcomes in source order.
    pub fn statements(&self) -> &[ScriptStatementOutcome] {
        &self.statements
    }

    /// Returns the number of canonical commits produced by the staged run.
    pub const fn commit_count(&self) -> u64 {
        self.commit_count
    }

    /// Returns typed persistent-ID results in statement, field, and element order.
    pub fn results(&self) -> &[ScriptResultValue] {
        &self.results
    }

    /// Returns the final canonical document-state digest.
    pub const fn final_state_digest(&self) -> DocumentStateDigest {
        self.final_state_digest
    }

    /// Returns the final document revision.
    pub const fn final_revision(&self) -> u64 {
        self.final_revision
    }

    /// Returns the next nonzero persistent stable identifier after staging.
    pub const fn next_stable_id(&self) -> u64 {
        self.next_stable_id
    }
}
