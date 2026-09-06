use inkpod_core::inkscript::abi_bridge::*;
use inkpod_core::inkscript::*;
use inkpod_core::{Core, GridConfig, PixelFormat};
use inkpod_format::{CommonRaster, CommonRasterFormat, encode_common_raster};
use inkpod_io::{IoConfig, IoManager};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

static PREVIEW_TEST: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug)]
enum InputReadFault {
    Fingerprint(ScriptRunAdapterError),
    Read(ScriptRunAdapterError),
    MismatchedFingerprint,
    MismatchedBytes,
}

struct FaultInputAdapter {
    inner: ScriptIoAdapter,
    fault: InputReadFault,
    injected: bool,
}

// Delegate every unrelated runtime contract to the real shared adapter. Only the
// requested input-read observation is replaced; real output methods stay unused.
macro_rules! forward_run_adapter {
    ($(fn $name:ident($($argument:ident: $ty:ty),*) -> $result:ty;)*) => {
        $(fn $name(&mut self, $($argument: $ty),*) -> $result {
            ScriptRunAdapter::$name(&mut self.inner, $($argument),*)
        })*
    };
}

impl ScriptRunAdapter for FaultInputAdapter {
    forward_run_adapter! {
        fn authority_generation() -> Result<u64, ScriptRunAdapterError>;
        fn open_session_set_generation() -> Result<u64, ScriptRunAdapterError>;
        fn session_is_current(session_id: u64, session_generation: u64, source_generation: u64) -> Result<bool, ScriptRunAdapterError>;
        fn atomic_capabilities(destination: &ValidatedPathIdentity) -> Result<ScriptAtomicCapabilities, ScriptRunAdapterError>;
        fn prepare_destination(destination: &ValidatedPathIdentity, known_job_directories: &[ValidatedPathIdentity], cancelled: &mut dyn FnMut() -> bool) -> Result<ScriptPreparedDestination, ScriptRunAdapterError>;
        fn revalidate_destination(destination: &ValidatedPathIdentity) -> Result<ValidatedPathIdentity, ScriptRunAdapterError>;
        fn create_same_volume_temporary(destination: &ValidatedPathIdentity, cancelled: &mut dyn FnMut() -> bool) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError>;
        fn write_flush_close_temporary(temporary: ScriptTemporaryIdentity, bytes: &[u8], cancelled: &mut dyn FnMut() -> bool) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError>;
        fn revalidate_closed_temporary(temporary: ScriptTemporaryIdentity) -> Result<ScriptTemporaryIdentity, ScriptRunAdapterError>;
        fn acquire_overwrite_guard(source: &NativeInputFingerprint) -> Result<ScriptOverwriteGuard, ScriptRunAdapterError>;
        fn fingerprint_under_guard(guard: ScriptOverwriteGuard, source: &NativeInputFingerprint) -> Result<NativeInputFingerprint, ScriptRunAdapterError>;
        fn release_overwrite_guard(guard: ScriptOverwriteGuard) -> ();
        fn atomic_install(temporary: ScriptTemporaryIdentity, destination: &ValidatedPathIdentity, overwrite_guard: Option<ScriptOverwriteGuard>, cancelled: &mut dyn FnMut() -> bool) -> Result<ScriptAtomicInstallResult, ScriptRunAdapterError>;
        fn cleanup_closed_temporary(temporary: ScriptTemporaryIdentity) -> ();
    }

    fn fingerprint_native(
        &mut self,
        expected: &NativeInputFingerprint,
    ) -> Result<NativeInputFingerprint, ScriptRunAdapterError> {
        if let InputReadFault::Fingerprint(error) = self.fault {
            self.injected = true;
            return Err(error);
        }
        if matches!(self.fault, InputReadFault::MismatchedFingerprint) {
            self.injected = true;
            assert_ne!(expected.content_digest(), [0xA5; 32]);
            return Ok(NativeInputFingerprint::new(
                expected.path().clone(),
                expected.display_label().to_owned(),
                expected.display_number(),
                expected.document_uuid(),
                expected.logical_length(),
                [0xA5; 32],
                expected.change_token(),
                expected.supports_atomic_overwrite(),
            )
            .unwrap());
        }
        ScriptRunAdapter::fingerprint_native(&mut self.inner, expected)
    }

    fn read_native(
        &mut self,
        expected: &NativeInputFingerprint,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptNativeRead, ScriptRunAdapterError> {
        self.injected = true;
        match self.fault {
            InputReadFault::Read(error) => Err(error),
            InputReadFault::MismatchedBytes => Ok(ScriptNativeRead::new(
                Vec::new(),
                expected.clone(),
                expected.clone(),
            )),
            InputReadFault::Fingerprint(_) | InputReadFault::MismatchedFingerprint => {
                ScriptRunAdapter::read_native(&mut self.inner, expected, cancelled)
            }
        }
    }
}

#[test]
fn preview_input_read_failures_cancel_and_fingerprint_mismatch_remain_distinct() {
    let _serial = PREVIEW_TEST.lock().unwrap();
    let manager = IoManager::new(IoConfig::default()).unwrap();
    let source_directory = manager
        .create_temporary_directory(
            "inkpod-script-preview-read-source",
            &inkpod_io::JobContext::new(),
        )
        .unwrap();
    let path = source_directory.path().join("A001.inkpod");
    core().save(&path).unwrap();
    let original = fs::read(&path).unwrap();
    let program = source(
        "profile = batch; file \"A001.inkpod\";",
        "",
        "policy = new_tabs;",
        "continue",
    );
    let paths = program
        .path_intents()
        .iter()
        .map(|intent| (intent.id(), source_directory.path().join(intent.text())))
        .collect();
    let mut adapter = ScriptIoAdapter::new(manager.clone(), paths, 1).unwrap();
    let authority = adapter.authority(&program, None, None).unwrap();
    let plan = plan_inkscript(
        &program,
        &authority,
        &mut adapter,
        &mut [],
        ScriptPlanLimits::exact_current(),
        &mut || false,
    )
    .unwrap();
    let previous = temporaries();
    let faults = [
        ScriptRunAdapterError::Cancelled,
        ScriptRunAdapterError::Io,
        ScriptRunAdapterError::Unavailable,
        ScriptRunAdapterError::InvalidData,
        ScriptRunAdapterError::UnsupportedAtomicInstall,
        ScriptRunAdapterError::UnsupportedAtomicOverwrite,
    ]
    .into_iter()
    .flat_map(|error| {
        [
            InputReadFault::Fingerprint(error),
            InputReadFault::Read(error),
        ]
    })
    .chain([
        InputReadFault::MismatchedFingerprint,
        InputReadFault::MismatchedBytes,
    ]);
    for fault in faults {
        let mut adapter = FaultInputAdapter {
            inner: adapter.clone(),
            fault,
            injected: false,
        };
        let mut token = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
        let result = preview_inkscript_images(
            &program,
            &plan,
            &mut token,
            &manager,
            &mut adapter,
            ScriptImagePreviewLimits::exact_current(),
            |_, _| true,
        );
        assert!(adapter.injected, "{fault:?}");
        match fault {
            InputReadFault::Fingerprint(ScriptRunAdapterError::Cancelled)
            | InputReadFault::Read(ScriptRunAdapterError::Cancelled) => {
                assert!(
                    matches!(result, Err(ScriptImagePreviewError::Cancelled)),
                    "{fault:?}: {result:?}"
                );
            }
            InputReadFault::Fingerprint(expected) | InputReadFault::Read(expected) => {
                assert!(
                    matches!(result, Err(ScriptImagePreviewError::InputRead(actual)) if actual == expected),
                    "{fault:?}: {result:?}"
                );
            }
            InputReadFault::MismatchedFingerprint | InputReadFault::MismatchedBytes => {
                assert!(
                    matches!(
                        result,
                        Err(ScriptImagePreviewError::Stale(
                            ScriptItemFailure::StaleInput
                        ))
                    ),
                    "{fault:?}: {result:?}"
                );
            }
        }
        assert_eq!(temporaries(), previous, "{fault:?}");
        assert_eq!(fs::read(&path).unwrap(), original, "{fault:?}");
    }
    source_directory.cleanup().unwrap();
}

#[test]
fn file_preview_runs_from_complete_copies_after_originals_change() {
    let _serial = PREVIEW_TEST.lock().unwrap();
    let manager = IoManager::new(IoConfig::default()).unwrap();
    let source_directory = manager
        .create_temporary_directory(
            "inkpod-script-preview-source",
            &inkpod_io::JobContext::new(),
        )
        .unwrap();
    let first = source_directory.path().join("A001.inkpod");
    let second = source_directory.path().join("A002.inkpod");
    core().save(&first).unwrap();
    core().save(&second).unwrap();
    let program = source(
        "profile = batch; file \"A001.inkpod\"; file \"A002.inkpod\";",
        "",
        "policy = new_tabs;",
        "continue",
    );
    let paths = program
        .path_intents()
        .iter()
        .map(|intent| (intent.id(), source_directory.path().join(intent.text())))
        .collect();
    let mut adapter = ScriptIoAdapter::new(manager.clone(), paths, 2).unwrap();
    let authority = adapter.authority(&program, None, None).unwrap();
    let plan = plan_inkscript(
        &program,
        &authority,
        &mut adapter,
        &mut [],
        ScriptPlanLimits::exact_current(),
        &mut || false,
    )
    .unwrap();
    let mut token = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
    let mut replaced = false;
    let result = preview_inkscript_images(
        &program,
        &plan,
        &mut token,
        &manager,
        &mut adapter,
        ScriptImagePreviewLimits::exact_current(),
        |completed, _| {
            if completed == 2 && !replaced {
                fs::write(&first, b"external edit after copies").unwrap();
                fs::write(&second, b"external edit after copies").unwrap();
                replaced = true;
            }
            true
        },
    )
    .unwrap();
    assert!(replaced);
    assert!(
        result
            .report()
            .items
            .iter()
            .all(|item| item.outcome == ScriptItemOutcome::Staged)
    );
    assert_eq!(fs::read(first).unwrap(), b"external edit after copies");
    assert_eq!(fs::read(second).unwrap(), b"external edit after copies");
    source_directory.cleanup().unwrap();
}

fn source(inputs: &str, steps: &str, output: &str, failure: &str) -> StaticScriptProgram {
    let text = format!(
        "inkscript 3; requires {{ procedure_catalog = 8; replay_epoch = 29; }} inputs {{ {inputs} }} program {{ {steps} }} output {{ {output} }} execution {{ failure = {failure}; wait_ms = 0; preview_before_save = false; }}"
    );
    compile_inkscript(
        &InkScriptSource::new(InkScriptSourceId::new(4101), text.as_bytes()).unwrap(),
        InkScriptRunParameterDecision::Resolve(vec![]),
    )
    .unwrap()
}
fn core() -> Core {
    let raster = CommonRaster::new(
        2,
        1,
        PixelFormat::StraightRgba8,
        None,
        None,
        vec![255, 0, 0, 255, 0, 0, 0, 0],
    )
    .unwrap();
    let bytes = encode_common_raster(CommonRasterFormat::Png, &raster, false).unwrap();
    let mut core = Core::new();
    core.import_common_raster(CommonRasterFormat::Png, &bytes, 0x4101)
        .unwrap();
    core
}
fn adapter(program: &StaticScriptProgram, live: &Core) -> ScriptIoAdapter {
    let root = std::env::temp_dir().join(format!("inkpod-preview-unused-{}", std::process::id()));
    let paths = program
        .path_intents()
        .iter()
        .map(|intent| (intent.id(), root.join(intent.text())))
        .collect();
    let mut adapter =
        ScriptIoAdapter::new(IoManager::new(IoConfig::default()).unwrap(), paths, 4).unwrap();
    adapter
        .capture_session(41, 2, 7, "current-cell.inkpod".into(), 1, None, live)
        .unwrap();
    adapter
}
fn plan(
    program: &StaticScriptProgram,
    adapter: &mut ScriptIoAdapter,
) -> (ScriptExecutionPlan, ScriptConfirmationToken) {
    let authority = adapter.authority(program, Some(41), None).unwrap();
    let plan = plan_inkscript(
        program,
        &authority,
        adapter,
        &mut [],
        ScriptPlanLimits::exact_current(),
        &mut || false,
    )
    .unwrap();
    let token = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
    (plan, token)
}
fn temporaries() -> BTreeSet<PathBuf> {
    fs::read_dir(std::env::temp_dir().join("inkpod-file-io"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("inkpod-script-preview-{}-", std::process::id()))
        })
        .collect()
}

#[test]
fn preview_copies_all_inputs_then_reopens_each_codec_and_cleans_before_publication() {
    let _serial = PREVIEW_TEST.lock().unwrap();
    let live = core();
    let before = live.document_info().unwrap();
    for format in ["inkpod", "png", "tiff", "tga", "bmp"] {
        let program = source(
            "profile = batch; current_document; current_document;",
            "",
            &format!(
                "policy = folder; format = {format}; folder = \"out\"; naming_template = \"{{index:2}}\";"
            ),
            "continue",
        );
        let mut adapter = adapter(&program, &live);
        let manager = adapter.manager().clone();
        let (plan, mut token) = plan(&program, &mut adapter);
        let previous = temporaries();
        let mut copied_before_output = false;
        let mut saw_clean_callback = false;
        let result = preview_inkscript_images(
            &program,
            &plan,
            &mut token,
            &manager,
            &mut adapter,
            ScriptImagePreviewLimits::exact_current(),
            |completed, total| {
                let current = temporaries();
                let added = current.difference(&previous).collect::<Vec<_>>();
                if completed == 2 && !added.is_empty() {
                    let files = fs::read_dir(added[0])
                        .unwrap()
                        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                        .collect::<Vec<_>>();
                    if files
                        .iter()
                        .filter(|name| name.starts_with("input-"))
                        .count()
                        == 2
                        && !files.iter().any(|name| name.starts_with("output-"))
                    {
                        copied_before_output = true;
                    }
                }
                if completed == total && added.is_empty() {
                    saw_clean_callback = true;
                }
                true
            },
        )
        .unwrap();
        assert!(copied_before_output);
        assert!(saw_clean_callback);
        assert_eq!(temporaries(), previous);
        assert!(
            result
                .report()
                .items
                .iter()
                .all(|item| item.outcome == ScriptItemOutcome::Staged)
        );
        assert!(!result.core().document_info().unwrap().dirty);
        assert!(matches!(
            result.core().clone().revert(),
            Err(inkpod_core::CoreError::InvalidState(
                "document has no normal-save path"
            ))
        ));
        assert_eq!(result.origin(), plan.command_context());
        let thumbnail = inkpod_format::decode_common_raster(
            CommonRasterFormat::Png,
            &result
                .core()
                .export_common_raster(CommonRasterFormat::Png, false)
                .unwrap(),
        )
        .unwrap();
        assert!(
            thumbnail
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel == [255, 0, 0, 255])
        );
        assert_eq!(live.document_info().unwrap(), before);
    }
}

#[test]
fn preview_stop_placeholders_cancel_resource_and_post_cleanup_stale_are_distinct() {
    let _serial = PREVIEW_TEST.lock().unwrap();
    let live = core();
    let steps = "step \"missing\" { enabled = true; invoke apply_batch_operations { operations = [{kind = erase; enabled = true; target = {kind = role; plane_kind = raster; missing = error;}; colors = [rgba8(255,0,0,255)];}]; }; }";
    let program = source(
        "profile = batch; current_document; current_document;",
        steps,
        "policy = new_tabs;",
        "stop",
    );
    let mut adapter = adapter(&program, &live);
    let manager = adapter.manager().clone();
    let (plan, mut token) = plan(&program, &mut adapter);
    let previous = temporaries();
    let result = preview_inkscript_images(
        &program,
        &plan,
        &mut token,
        &manager,
        &mut adapter,
        ScriptImagePreviewLimits::exact_current(),
        |_, _| true,
    )
    .unwrap();
    assert!(matches!(
        result.report().items[0].outcome,
        ScriptItemOutcome::Failed(_)
    ));
    assert_eq!(
        result.report().items[1].outcome,
        ScriptItemOutcome::NotStarted
    );
    let pixels = inkpod_format::decode_common_raster(
        CommonRasterFormat::Png,
        &result
            .core()
            .export_common_raster(CommonRasterFormat::Png, false)
            .unwrap(),
    )
    .unwrap()
    .pixels;
    assert!(pixels.chunks_exact(4).any(|p| p == [138, 42, 48, 255]));
    assert!(pixels.chunks_exact(4).any(|p| p == [88, 88, 92, 255]));
    for limits in [
        ScriptImagePreviewLimits::exact_current().with_temporary_bytes(1),
        ScriptImagePreviewLimits::exact_current().with_pixels(0),
    ] {
        let mut token = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
        assert!(matches!(
            preview_inkscript_images(
                &program,
                &plan,
                &mut token,
                &manager,
                &mut adapter,
                limits,
                |_, _| true
            ),
            Err(ScriptImagePreviewError::ResourceLimit)
        ));
    }
    let mut token = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
    assert!(matches!(
        preview_inkscript_images(
            &program,
            &plan,
            &mut token,
            &manager,
            &mut adapter,
            ScriptImagePreviewLimits::exact_current(),
            |completed, _| completed < 1
        ),
        Err(ScriptImagePreviewError::Cancelled)
    ));
    let mut token = issue_confirmation_token(&plan, ScriptRunScope::All).unwrap();
    let owner = adapter.clone();
    assert!(matches!(
        preview_inkscript_images(
            &program,
            &plan,
            &mut token,
            &manager,
            &mut adapter,
            ScriptImagePreviewLimits::exact_current(),
            |completed, total| {
                if completed == total && temporaries() == previous {
                    owner.invalidate_session(41).unwrap();
                }
                true
            }
        ),
        Err(ScriptImagePreviewError::Stale(_))
    ));
    assert_eq!(temporaries(), previous);
}

#[test]
fn canonical_preview_uses_complete_dirty_snapshot_and_never_changes_source() {
    let _serial = PREVIEW_TEST.lock().unwrap();
    let mut live = core();
    live.set_grid(GridConfig {
        origin_x: 1,
        origin_y: 2,
        spacing_x: 8,
        spacing_y: 8,
        subdivisions: 2,
    })
    .unwrap();
    let original = live.document_state_digest().unwrap();
    let history = live.journal_entries().to_vec();
    let program = source("current_document;", "", "policy = new_tabs;", "continue");
    let mut adapter = adapter(&program, &live);
    let manager = adapter.manager().clone();
    let (plan, mut token) = plan(&program, &mut adapter);
    let result = preview_inkscript_images(
        &program,
        &plan,
        &mut token,
        &manager,
        &mut adapter,
        ScriptImagePreviewLimits::exact_current(),
        |_, _| true,
    )
    .unwrap();
    assert_eq!(
        result.report().items[0]
            .execution
            .as_ref()
            .unwrap()
            .final_state_digest(),
        original
    );
    assert_eq!(live.journal_entries(), history);
    assert_eq!(live.document_state_digest().unwrap(), original);
}

#[test]
fn cleanup_failure_never_returns_a_display_result() {
    let _serial = PREVIEW_TEST.lock().unwrap();
    let live = core();
    let program = source("current_document;", "", "policy = new_tabs;", "continue");
    let mut adapter = adapter(&program, &live);
    let manager = adapter.manager().clone();
    let (plan, mut token) = plan(&program, &mut adapter);
    let previous = temporaries();
    let mut displaced = None;
    let result = preview_inkscript_images(
        &program,
        &plan,
        &mut token,
        &manager,
        &mut adapter,
        ScriptImagePreviewLimits::exact_current(),
        |completed, total| {
            if completed == total && displaced.is_none() {
                // Move only this test's newly allocated directory after composition.
                // Cleanup must fail instead of publishing the completed display.
                let created: Vec<_> = temporaries().difference(&previous).cloned().collect();
                assert_eq!(created.len(), 1);
                let path = fs::canonicalize(&created[0]).unwrap();
                let root = fs::canonicalize(std::env::temp_dir().join("inkpod-file-io")).unwrap();
                assert!(path.starts_with(&root) && path != root);
                let moved = path.with_extension("cleanup-fault");
                assert!(!moved.exists());
                fs::rename(&path, &moved).unwrap();
                displaced = Some((path, moved));
            }
            true
        },
    );
    assert!(matches!(result, Err(ScriptImagePreviewError::Io(_))));
    manager.shutdown_and_wait();
    let (path, moved) = displaced.unwrap();
    fs::rename(moved, &path).unwrap();
    fs::remove_dir_all(path).unwrap();
    assert_eq!(temporaries(), previous);
}
