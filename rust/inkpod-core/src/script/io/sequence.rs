use super::*;

/// One host-approved member of an issue-time sequence capture. The enum carries
/// no OS handle and is never serialized into source or canonical procedures.
#[derive(Clone, Debug)]
pub enum ScriptIoSequenceInput {
    /// An existing session previously captured on this adapter.
    Session(u64),
    /// An explicitly approved absolute source path and its nonzero host generation.
    /// Supplying this variant grants read authority for that exact sequence member.
    File {
        /// The explicitly approved absolute source path.
        path: PathBuf,
        /// The nonzero source generation captured by the issuing host.
        source_generation: u64,
    },
}

impl ScriptIoAdapter {
    /// Captures a sequence using this shared manager's path identities and byte
    /// fingerprints, or previously captured immutable sessions. File paths are
    /// explicit host read grants, never source-relative or resolved against cwd.
    /// IDs and generations belong to the issuing host; all must be nonzero.
    ///
    /// Invalid/duplicate/over-limit members, failed reads and cancellation leave
    /// the prior sequence and its generation unchanged. Cancellation runs on the
    /// caller's thread, including bounded reads; callback panics unwind.
    pub fn capture_sequence_inputs(
        &mut self,
        sequence_id: u64,
        generation: u64,
        members: &[ScriptIoSequenceInput],
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), ScriptPlanError> {
        if cancelled() {
            return Err(ScriptPlanError::Cancelled);
        }
        if sequence_id == 0 || generation == 0 {
            return Err(ScriptPlanError::InvalidInput);
        }
        if members.len() > MAX_INKSCRIPT_INPUTS {
            return Err(ScriptPlanError::ResourceLimit);
        }
        let expected_generations = {
            let state = self
                .state
                .lock()
                .map_err(|_| ScriptPlanError::StaleAuthority)?;
            (state.authority_generation, state.sessions_generation)
        };
        let path_bytes = members.iter().try_fold(0_usize, |sum, member| {
            let length = match member {
                ScriptIoSequenceInput::Session(_) => 0,
                ScriptIoSequenceInput::File { path, .. } => path.as_os_str().len(),
            };
            sum.checked_add(length)
                .ok_or(ScriptPlanError::ResourceLimit)
        })?;
        if path_bytes > MAX_INKSCRIPT_SOURCE_BYTES {
            return Err(ScriptPlanError::ResourceLimit);
        }
        let mut snapshots = Vec::new();
        snapshots
            .try_reserve_exact(members.len())
            .map_err(|_| ScriptPlanError::ResourceLimit)?;
        for member in members {
            if cancelled() {
                return Err(ScriptPlanError::Cancelled);
            }
            snapshots.push(match member {
                ScriptIoSequenceInput::Session(id) => {
                    let session = self.sessions.get(id).ok_or(ScriptPlanError::StaleInput)?;
                    let state = self
                        .state
                        .lock()
                        .map_err(|_| ScriptPlanError::StaleAuthority)?;
                    if state.sessions.get(id)
                        != Some(&(
                            session.snapshot.session_generation(),
                            session.snapshot.source_generation(),
                        ))
                    {
                        return Err(ScriptPlanError::StaleInput);
                    }
                    ScriptSequenceMemberSnapshot::Session(session.snapshot.clone())
                }
                ScriptIoSequenceInput::File {
                    path,
                    source_generation,
                } => {
                    if !path.is_absolute() {
                        return Err(ScriptPlanError::AuthorityMismatch);
                    }
                    if *source_generation == 0 {
                        return Err(ScriptPlanError::InvalidInput);
                    }
                    let fingerprint = self
                        .read_fingerprint_cancellable(path, cancelled)
                        .map_err(|error| {
                            if error == ScriptRunAdapterError::Cancelled {
                                ScriptPlanError::Cancelled
                            } else {
                                ScriptPlanError::Adapter(plan_error(error))
                            }
                        })?
                        .0;
                    ScriptSequenceMemberSnapshot::File {
                        source_generation: *source_generation,
                        fingerprint,
                    }
                }
            });
        }
        if cancelled() {
            return Err(ScriptPlanError::Cancelled);
        }
        self.capture_sequence_if_current(
            ScriptSequenceSnapshot::new(sequence_id, generation, snapshots)?,
            Some(expected_generations),
        )
    }
}
