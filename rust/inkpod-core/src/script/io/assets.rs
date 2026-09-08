use super::*;
use crate::script::assets::{
    AuthorizedAssetIdentity, AuthorizedAssetReadError, AuthorizedAssetReader,
    AuthorizedAssetStream, ScriptAssetError,
};
use crate::script::compile::ScriptPathIntentSubject;
use inkpod_format::{MAX_INKSCRIPT_ASSET_TOTAL_BYTES, MAX_INKSCRIPT_EXTERNAL_ASSET_BYTES};
use inkpod_io::LoadedBytes;

impl ScriptIoAdapter {
    /// Builds an immutable plan from the source's explicitly approved paths and
    /// captured sessions. External assets are read through this adapter's shared
    /// manager, checked against path authority and frozen before canonical planning.
    /// No source, document, destination or publication state is changed.
    ///
    /// Cancellation is polled on the caller's thread, including file-lock waits
    /// and bounded asset reads. Failure or cancellation publishes no plan; callback
    /// panics unwind. `current_session` identifies the issue-time captured session;
    /// `script_path`, when present, is an approved absolute script-file path.
    pub fn plan(
        &mut self,
        program: &StaticScriptProgram,
        current_session: Option<u64>,
        script_path: Option<PathBuf>,
        limits: ScriptPlanLimits,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptExecutionPlan, ScriptPlanError> {
        if cancelled() {
            return Err(ScriptPlanError::Cancelled);
        }
        if script_path.as_ref().is_some_and(|path| !path.is_absolute()) {
            return Err(ScriptPlanError::AuthorityMismatch);
        }
        let authority = self.authority(program, current_session, script_path)?;
        let total = program
            .asset_summaries
            .values()
            .try_fold(0_u64, |sum, asset| {
                sum.checked_add(asset.logical_payload_bytes)
                    .ok_or(ScriptPlanError::ResourceLimit)
            })?;
        if total > MAX_INKSCRIPT_ASSET_TOTAL_BYTES {
            return Err(ScriptPlanError::ResourceLimit);
        }
        let mut readers = Vec::new();
        readers
            .try_reserve_exact(program.path_intents().len())
            .map_err(|_| ScriptPlanError::ResourceLimit)?;
        let mut remaining = MAX_INKSCRIPT_ASSET_TOTAL_BYTES;
        for intent in program.path_intents() {
            let ScriptPathIntentSubject::Asset(symbol) = intent.subject() else {
                continue;
            };
            if cancelled() {
                return Err(ScriptPlanError::Cancelled);
            }
            self.validate_grant(intent.id())
                .map_err(|_| ScriptPlanError::StaleInput)?;
            let path = self
                .paths
                .get(&intent.id())
                .ok_or(ScriptPlanError::MissingAuthority)?;
            let grant = self
                .grants
                .get(&intent.id())
                .ok_or(ScriptPlanError::MissingAuthority)?;
            let loaded = self
                .manager
                .read_bytes_cancellable(
                    path,
                    MAX_INKSCRIPT_EXTERNAL_ASSET_BYTES.min(remaining),
                    &self.context,
                    cancelled,
                )
                .map_err(asset_io_error)?;
            if grant.object != Some(loaded.identity()) {
                return Err(ScriptPlanError::Asset(
                    ScriptAssetError::StaleAuthorizedStream,
                ));
            }
            self.validate_grant(intent.id())
                .map_err(|_| ScriptPlanError::StaleInput)?;
            remaining = remaining
                .checked_sub(loaded.stamp().length)
                .ok_or(ScriptPlanError::ResourceLimit)?;
            let identity = AuthorizedAssetIdentity::new(
                object_id(loaded.identity()),
                1,
                loaded.stamp().length,
            );
            readers.push((
                symbol.as_str(),
                FrozenAssetReader {
                    loaded,
                    identity,
                    cursor: 0,
                },
            ));
        }
        let mut streams = Vec::new();
        streams
            .try_reserve_exact(readers.len())
            .map_err(|_| ScriptPlanError::ResourceLimit)?;
        for (symbol, reader) in &mut readers {
            streams.push(AuthorizedAssetStream::new(symbol, reader.identity, reader));
        }
        // The legacy adapter error enum intentionally has no cancellation arm.
        // Retain the callback decision so this shared-manager entry point preserves
        // cancellation even when a bounded I/O operation returns adapter failure.
        let mut cancellation_observed = false;
        let result = {
            let mut poll = || {
                cancellation_observed |= cancelled();
                cancellation_observed
            };
            plan_inkscript(program, &authority, self, &mut streams, limits, &mut poll)
        };
        if cancellation_observed {
            Err(ScriptPlanError::Cancelled)
        } else {
            result
        }
    }
}

// The manager validates the same opened object's complete stamp before and after
// its bounded read. The returned lease is immutable and still charged to that
// manager; the canonical asset ingester sees no path and never reopens it at run time.
struct FrozenAssetReader {
    loaded: LoadedBytes,
    identity: AuthorizedAssetIdentity,
    cursor: usize,
}

impl AuthorizedAssetReader for FrozenAssetReader {
    fn observe_identity(&mut self) -> Result<AuthorizedAssetIdentity, AuthorizedAssetReadError> {
        Ok(self.identity)
    }

    fn read_chunk(&mut self, target: &mut [u8]) -> Result<usize, AuthorizedAssetReadError> {
        let remaining = &self.loaded.bytes()[self.cursor..];
        let length = remaining.len().min(target.len());
        target[..length].copy_from_slice(&remaining[..length]);
        self.cursor += length;
        Ok(length)
    }
}

fn asset_io_error(error: inkpod_io::IoError) -> ScriptPlanError {
    match error {
        inkpod_io::IoError::Cancelled => ScriptPlanError::Cancelled,
        inkpod_io::IoError::LimitExceeded(_) | inkpod_io::IoError::ResourceBusy(_) => {
            ScriptPlanError::ResourceLimit
        }
        inkpod_io::IoError::ChangedDuringRead | inkpod_io::IoError::ConfirmationRequired => {
            ScriptPlanError::Asset(ScriptAssetError::StaleAuthorizedStream)
        }
        _ => ScriptPlanError::Asset(ScriptAssetError::StreamReadFailed),
    }
}
