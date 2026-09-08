use super::*;
use inkpod_io::{GuardedPublishOutcome, PublishSource};

impl ScriptIoAdapter {
    fn with_output_ancestor<T>(
        &self,
        destination: &Path,
        action: impl FnOnce() -> inkpod_io::IoResult<T>,
    ) -> inkpod_io::IoResult<T> {
        match self.output_ancestor(destination)? {
            Some((approved, anchor)) => {
                self.manager
                    .with_directory_authority(&approved, &anchor, &self.context, action)
            }
            None => action(),
        }
    }

    fn output_ancestor(
        &self,
        destination: &Path,
    ) -> inkpod_io::IoResult<Option<(PathBuf, PathAuthority)>> {
        let grant = self
            .grants
            .iter()
            .filter(|(_, grant)| destination.starts_with(&grant.path))
            .max_by_key(|(_, grant)| grant.path.components().count());
        let Some((id, granted)) = grant else {
            // Duplicate output uses its input's already captured direct parent.
            return Ok(None);
        };
        let approved = self
            .paths
            .get(id)
            .ok_or(inkpod_io::IoError::ConfirmationRequired)?;
        if granted.is_directory {
            return Ok(Some((approved.clone(), granted.clone())));
        }
        // Missing output roots retain their original nearest existing ancestor,
        // even after this job has created and recorded a descendant directory.
        let mut approved_ancestor = approved.clone();
        for _ in granted
            .path
            .strip_prefix(&granted.parent_path)
            .map_err(|_| inkpod_io::IoError::ConfirmationRequired)?
            .components()
        {
            if !approved_ancestor.pop() {
                return Err(inkpod_io::IoError::ConfirmationRequired);
            }
        }
        let anchor = self
            .manager
            .observe_path_authority(&approved_ancestor, &self.context)?;
        if anchor.path != granted.parent_path
            || anchor.object != Some(granted.parent)
            || !anchor.is_directory
        {
            return Err(inkpod_io::IoError::ConfirmationRequired);
        }
        Ok(Some((approved_ancestor, anchor)))
    }
}

impl ScriptRunAdapter for ScriptIoAdapter {
    fn authority_generation(&mut self) -> Result<u64, ScriptRunAdapterError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| ScriptRunAdapterError::Unavailable)?
            .authority_generation)
    }
    fn open_session_set_generation(&mut self) -> Result<u64, ScriptRunAdapterError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| ScriptRunAdapterError::Unavailable)?
            .sessions_generation)
    }
    fn session_is_current(
        &mut self,
        id: u64,
        generation: u64,
        source: u64,
    ) -> Result<bool, ScriptRunAdapterError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| ScriptRunAdapterError::Unavailable)?
            .sessions
            .get(&id)
            == Some(&(generation, source)))
    }
    fn read_native(
        &mut self,
        expected: &NativeInputFingerprint,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptNativeRead, ScriptRunAdapterError> {
        if cancelled() {
            return Err(ScriptRunAdapterError::Cancelled);
        }
        let path = self.known_path(expected.path())?;
        let (before, bytes) = self.read_fingerprint_cancellable(&path, cancelled)?;
        if cancelled() {
            return Err(ScriptRunAdapterError::Cancelled);
        }
        let after = self.read_fingerprint_cancellable(&path, cancelled)?.0;
        Ok(ScriptNativeRead::new(bytes, before, after))
    }
    fn fingerprint_native(
        &mut self,
        expected: &NativeInputFingerprint,
    ) -> Result<NativeInputFingerprint, ScriptRunAdapterError> {
        let path = self.known_path(expected.path())?;
        self.read_fingerprint(&path).map(|value| value.0)
    }
    fn atomic_capabilities(
        &mut self,
        _: &ValidatedPathIdentity,
    ) -> Result<ScriptAtomicCapabilities, ScriptRunAdapterError> {
        Ok(ScriptAtomicCapabilities {
            install: self.manager.supports_guarded_publication(),
            overwrite: self.manager.supports_guarded_publication(),
        })
    }
    fn prepare_destination(
        &mut self,
        destination: &ValidatedPathIdentity,
        known_job_directories: &[ValidatedPathIdentity],
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptPreparedDestination, ScriptRunAdapterError> {
        let path = self.known_path(destination)?;
        // Validate the nearest existing ancestor before creating anything.
        let observed = self.observe(&path)?;
        let known_parent = known_job_directories.iter().any(|directory| {
            directory.object_id() == Some(observed.parent_object_id())
                && directory.alias_key() == observed.parent_alias_key()
                && directory.object_generation() == Some(observed.parent_generation())
        });
        let created_parent_only = known_parent
            && destination.canonical_key() == observed.canonical_key()
            && destination.object_id() == observed.object_id()
            && destination.object_generation() == observed.object_generation()
            && destination.volume_id() == observed.volume_id()
            && destination.alias_key() == observed.alias_key()
            && destination.is_expected_absent() == observed.is_expected_absent();
        if !destination.matches_exact(&observed) && !created_parent_only {
            return Err(ScriptRunAdapterError::InvalidData);
        }
        for directory in known_job_directories {
            let known_path = self.known_path(directory)?;
            if path.starts_with(&known_path) {
                let current = self
                    .manager
                    .observe_path_authority(&known_path, &self.context)
                    .map_err(run_error)?;
                if path_identity(&current)? != *directory {
                    return Err(ScriptRunAdapterError::InvalidData);
                }
            }
        }
        self.pending_created.clear();
        let expected = self
            .identities
            .get(observed.canonical_key())
            .cloned()
            .ok_or(ScriptRunAdapterError::InvalidData)?;
        let mut created = Vec::new();
        let result = self.with_output_ancestor(&path, || {
            self.manager
                .prepare_directory_chain(&expected, &self.context, cancelled, &mut created)
        });
        for directory in created {
            let identity = path_identity(&directory)?;
            self.identities
                .insert(identity.canonical_key().to_owned(), directory);
            self.pending_created.push(identity);
        }
        let observed = result.map_err(run_error)?;
        let identity = path_identity(&observed)?;
        self.identities
            .insert(identity.canonical_key().to_owned(), observed);
        Ok(ScriptPreparedDestination::new(
            identity,
            std::mem::take(&mut self.pending_created),
        ))
    }
    fn take_created_directories(&mut self) -> Vec<ValidatedPathIdentity> {
        std::mem::take(&mut self.pending_created)
    }
    fn revalidate_destination(
        &mut self,
        destination: &ValidatedPathIdentity,
    ) -> Result<ValidatedPathIdentity, ScriptRunAdapterError> {
        let path = self.known_path(destination)?;
        self.observe(&path)
    }
    fn publish_encoded(
        &mut self,
        destination: &ValidatedPathIdentity,
        source: Option<&NativeInputFingerprint>,
        encoded: &[u8],
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Option<Result<ScriptAtomicInstallResult, ScriptRunAdapterError>> {
        Some((|| {
            let authority = self
                .identities
                .get(destination.canonical_key())
                .cloned()
                .ok_or(ScriptRunAdapterError::InvalidData)?;
            if path_identity(&authority)? != *destination {
                return Err(ScriptRunAdapterError::InvalidData);
            }
            let proof = source
                .map(|source| {
                    if !destination.is_expected_absent()
                        && (!source.is_native() || source.path() != destination)
                    {
                        return Err(ScriptRunAdapterError::InvalidData);
                    }
                    let source_path = self.known_path(source.path())?;
                    let loaded = self
                        .manager
                        .read_bytes(&source_path, 1024 * 1024 * 1024, &self.context)
                        .map_err(run_error)?;
                    if Some(stamp_token(loaded.stamp())) != source.change_token()
                        || blake3::hash(loaded.bytes()).as_bytes() != &source.content_digest()
                    {
                        return Err(ScriptRunAdapterError::InvalidData);
                    }
                    Ok(PublishSource {
                        path: source_path,
                        stamp: loaded.stamp(),
                        digest: source.content_digest(),
                    })
                })
                .transpose()?;
            let ancestor = self.output_ancestor(&authority.path).map_err(run_error)?;
            match self
                .manager
                .publish_guarded_with_ancestor(
                    &authority,
                    proof,
                    encoded,
                    &self.context,
                    cancelled,
                    ancestor
                        .as_ref()
                        .map(|(path, authority)| (path.as_path(), authority)),
                )
                .map_err(|error| {
                    if matches!(error, inkpod_io::IoError::UnsupportedAtomicPublication)
                        && !destination.is_expected_absent()
                    {
                        ScriptRunAdapterError::UnsupportedAtomicOverwrite
                    } else {
                        run_error(error)
                    }
                })? {
                GuardedPublishOutcome::Installed => Ok(if cancelled() {
                    ScriptAtomicInstallResult::InstalledAfterCancellation
                } else {
                    ScriptAtomicInstallResult::Installed
                }),
                GuardedPublishOutcome::CancelledBeforeInstall => {
                    Ok(ScriptAtomicInstallResult::CancelledBeforeLinearization)
                }
            }
        })())
    }
    fn fresh_document_identity(
        &mut self,
        excluded: &[u128],
    ) -> Result<u128, ScriptRunAdapterError> {
        let mut excluded = excluded.to_vec();
        excluded.extend(self.sessions.values().map(|session| session.uuid));
        super::super::identity::allocate_script_document_identity(excluded)
            .map_err(|_| ScriptRunAdapterError::Unavailable)
    }
    // The direct publication capability owns the complete lock/temporary scope.
    // The split callback ABI is deliberately unavailable for this adapter.
    fn create_same_volume_temporary(
        &mut self,
        _: &ValidatedPathIdentity,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        Err(ScriptRunAdapterError::Unavailable)
    }
    fn write_flush_close_temporary(
        &mut self,
        _: ScriptTemporaryIdentity,
        _: &[u8],
        _: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        Err(ScriptRunAdapterError::Unavailable)
    }
    fn revalidate_closed_temporary(
        &mut self,
        _: ScriptTemporaryIdentity,
    ) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError> {
        Err(ScriptRunAdapterError::Unavailable)
    }
    fn acquire_overwrite_guard(
        &mut self,
        _: &NativeInputFingerprint,
    ) -> Result<ScriptOverwriteGuard, ScriptRunAdapterError> {
        Err(ScriptRunAdapterError::Unavailable)
    }
    fn fingerprint_under_guard(
        &mut self,
        _: ScriptOverwriteGuard,
        _: &NativeInputFingerprint,
    ) -> Result<NativeInputFingerprint, ScriptRunAdapterError> {
        Err(ScriptRunAdapterError::Unavailable)
    }
    fn release_overwrite_guard(&mut self, _: ScriptOverwriteGuard) {}
    fn atomic_install(
        &mut self,
        _: ScriptTemporaryIdentity,
        _: &ValidatedPathIdentity,
        _: Option<ScriptOverwriteGuard>,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptAtomicInstallResult, ScriptRunAdapterError> {
        Err(ScriptRunAdapterError::Unavailable)
    }
    fn cleanup_closed_temporary(&mut self, _: ScriptTemporaryIdentity) {}
}
