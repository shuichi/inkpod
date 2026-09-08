use super::*;

impl ScriptPlanAdapter for ScriptIoAdapter {
    fn authority_generation(&mut self) -> Result<u64, ScriptPlanAdapterError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| ScriptPlanAdapterError::Unavailable)?
            .authority_generation)
    }
    fn open_session_set(&mut self) -> Result<OpenSessionSetSnapshot, ScriptPlanAdapterError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ScriptPlanAdapterError::Unavailable)?;
        let mut records = Vec::new();
        for (id, session) in &self.sessions {
            if state.sessions.get(id)
                != Some(&(
                    session.snapshot.session_generation(),
                    session.snapshot.source_generation(),
                ))
            {
                continue;
            }
            if let Some(path) = &session.backing {
                records.push(
                    OpenSessionRecord::new(
                        *id,
                        session.snapshot.session_generation(),
                        session.uuid,
                        path.clone(),
                    )
                    .and_then(|record| record.with_pair_alias(session.pair_alias.clone()))
                    .map_err(|_| ScriptPlanAdapterError::InvalidData)?,
                );
            }
        }
        OpenSessionSetSnapshot::new(state.sessions_generation, records)
            .map_err(|_| ScriptPlanAdapterError::InvalidData)
    }
    fn resolve_file(
        &mut self,
        intent_id: u64,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<NativeInputFingerprint, ScriptPlanAdapterError> {
        if cancelled() {
            return Err(ScriptPlanAdapterError::Failure);
        }
        self.validate_grant(intent_id)?;
        let path = self
            .paths
            .get(&intent_id)
            .cloned()
            .ok_or(ScriptPlanAdapterError::InvalidData)?;
        self.read_fingerprint_cancellable(&path, cancelled)
            .map(|value| value.0)
            .map_err(plan_error)
    }
    fn enumerate_folder(
        &mut self,
        intent_id: u64,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<FolderScan, ScriptPlanAdapterError> {
        self.validate_grant(intent_id)?;
        let path = self
            .paths
            .get(&intent_id)
            .cloned()
            .ok_or(ScriptPlanAdapterError::InvalidData)?;
        let grant = self
            .grants
            .get(&intent_id)
            .cloned()
            .ok_or(ScriptPlanAdapterError::Unavailable)?;
        let manager = self.manager.clone();
        let context = self.context.clone();
        manager
            .with_directory_authority(&path, &grant, &context, || {
                Ok((|| {
                    let listing = manager
                        .list_directory(&grant.path, &context)
                        .map_err(|_| ScriptPlanAdapterError::Failure)?;
                    let mut matching = Vec::new();
                    for path in listing.regular_files {
                        if cancelled() {
                            return Err(ScriptPlanAdapterError::Failure);
                        }
                        let extension = path
                            .extension()
                            .and_then(|value| value.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        if matches!(
                            extension.as_str(),
                            "inkpod" | "png" | "tif" | "tiff" | "tga" | "bmp"
                        ) {
                            matching.push(
                                self.read_fingerprint_cancellable(&path, cancelled)
                                    .map_err(plan_error)?
                                    .0,
                            );
                            if matching.len() > 16_384 {
                                return Err(ScriptPlanAdapterError::Failure);
                            }
                        }
                    }
                    self.validate_grant(intent_id)?;
                    FolderScan::new(
                        listing.observed_entries,
                        listing.name_bytes,
                        listing.observed_entries,
                        1,
                        matching,
                    )
                    .map_err(|_| ScriptPlanAdapterError::InvalidData)
                })())
            })
            .map_err(|_| ScriptPlanAdapterError::Failure)?
    }
    fn capture_current_document(
        &mut self,
        expected: &ScriptSessionExpectation,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
        if cancelled() {
            return Err(ScriptPlanAdapterError::Failure);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| ScriptPlanAdapterError::Unavailable)?;
        self.sessions
            .iter()
            .find(|(id, session)| {
                state.sessions.get(id)
                    == Some(&(
                        session.snapshot.session_generation(),
                        session.snapshot.source_generation(),
                    ))
                    && ScriptSessionExpectation::from_snapshot(&session.snapshot).as_ref()
                        == Ok(expected)
            })
            .map(|(_, session)| session.snapshot.clone())
            .ok_or(ScriptPlanAdapterError::Unavailable)
    }
    fn capture_current_sequence(
        &mut self,
        expected: &ScriptSequenceExpectation,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSequenceSnapshot, ScriptPlanAdapterError> {
        if cancelled() {
            return Err(ScriptPlanAdapterError::Failure);
        }
        if self
            .state
            .lock()
            .map_err(|_| ScriptPlanAdapterError::Unavailable)?
            .sequence_revision
            != self.sequence_revision
        {
            return Err(ScriptPlanAdapterError::Unavailable);
        }
        self.current_sequence
            .as_ref()
            .filter(|value| {
                ScriptSequenceExpectation::from_snapshot(value).as_ref() == Ok(expected)
            })
            .cloned()
            .ok_or(ScriptPlanAdapterError::Unavailable)
    }
    fn capture_open_session(
        &mut self,
        record: &OpenSessionRecord,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
        let expected = ScriptSessionExpectation::from_snapshot(
            &self
                .sessions
                .get(&record.session_id())
                .ok_or(ScriptPlanAdapterError::Unavailable)?
                .snapshot,
        )
        .map_err(|_| ScriptPlanAdapterError::InvalidData)?;
        self.capture_current_document(&expected, cancelled)
    }
    fn resolve_destination(
        &mut self,
        request: &ScriptDestinationRequest,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ValidatedPathIdentity, ScriptPlanAdapterError> {
        if cancelled() {
            return Err(ScriptPlanAdapterError::Failure);
        }
        let mut path = match request.base() {
            ScriptDestinationBase::AuthorizedRoot { root, .. } => {
                self.known_path(root).map_err(plan_error)?
            }
            ScriptDestinationBase::InputParent { input_path } => self
                .known_path(input_path)
                .map_err(plan_error)?
                .parent()
                .ok_or(ScriptPlanAdapterError::InvalidData)?
                .to_path_buf(),
        };
        for component in request.relative_components() {
            if component.is_empty()
                || component == "."
                || component == ".."
                || component.contains(['/', '\\'])
            {
                return Err(ScriptPlanAdapterError::InvalidData);
            }
            path.push(component);
        }
        self.observe(&path).map_err(plan_error)
    }
    fn preflight_new_tabs(&mut self, count: usize) -> Result<(), ScriptPlanAdapterError> {
        if count > self.new_tab_capacity {
            Err(ScriptPlanAdapterError::Unavailable)
        } else {
            Ok(())
        }
    }
    fn preflight_active_document(
        &mut self,
        expected: &ScriptSessionExpectation,
    ) -> Result<(), ScriptPlanAdapterError> {
        self.capture_current_document(expected, &mut || false)
            .map(|_| ())
    }
}
