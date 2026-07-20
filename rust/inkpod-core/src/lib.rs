#![forbid(unsafe_code)]

/// Feature bits supported by the M0 core.
pub const CORE_FEATURES: u64 = 0;

/// A typed command accepted by the platform-independent core.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    /// Explicitly perform no mutation. This is used to exercise batching and
    /// validation before document-editing commands arrive in M1.
    NoOp,
}

/// Result of applying one command batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchOutcome {
    revision: u64,
    accepted_commands: u64,
}

impl DispatchOutcome {
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn accepted_commands(self) -> u64 {
        self.accepted_commands
    }
}

/// Immutable renderer input. M0 intentionally contains no drawing items.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderSnapshot {
    revision: u64,
}

impl RenderSnapshot {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn tile_count(&self) -> usize {
        0
    }
}

/// Single-writer application core. OS and frontend types do not enter this API.
#[derive(Debug, Default)]
pub struct Core {
    document_revision: u64,
}

impl Core {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            document_revision: 0,
        }
    }

    /// Validates and applies a complete command batch.
    ///
    /// M0 only defines a real no-op command, so a valid batch leaves the
    /// document revision unchanged. Later milestones add transactional edits.
    #[must_use]
    pub fn dispatch(&mut self, commands: &[Command]) -> DispatchOutcome {
        DispatchOutcome {
            revision: self.document_revision,
            accepted_commands: commands.len() as u64,
        }
    }

    #[must_use]
    pub const fn build_snapshot(&self) -> RenderSnapshot {
        RenderSnapshot {
            revision: self.document_revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Core};

    #[test]
    fn empty_snapshot_is_stable_and_has_no_tiles() {
        let core = Core::new();
        let first = core.build_snapshot();
        let second = core.build_snapshot();

        assert_eq!(first, second);
        assert_eq!(first.revision(), 0);
        assert_eq!(first.tile_count(), 0);
    }

    #[test]
    fn noop_batch_does_not_change_document_revision() {
        let mut core = Core::new();
        let outcome = core.dispatch(&[Command::NoOp, Command::NoOp]);

        assert_eq!(outcome.accepted_commands(), 2);
        assert_eq!(outcome.revision(), 0);
        assert_eq!(core.build_snapshot().revision(), 0);
    }
}
