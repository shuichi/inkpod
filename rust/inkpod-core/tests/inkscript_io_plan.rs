use inkpod_core::inkscript::abi_bridge::*;
use inkpod_core::inkscript::{
    InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId, ScriptCompileError,
    ScriptPathIntentSubject, StaticScriptProgram, compile_inkscript,
};
use inkpod_core::{Core, DEFAULT_DPI_MILLI};
use std::collections::BTreeMap;

fn path(key: &str, object: u8) -> ValidatedPathIdentity {
    ValidatedPathIdentity::existing(
        key.into(),
        [1; 16],
        [object; 32],
        digest(key),
        [99; 32],
        [98; 32],
    )
    .unwrap()
}

fn digest(text: &str) -> [u8; 32] {
    *blake3::hash(text.as_bytes()).as_bytes()
}

fn file(label: &str, object: u8, uuid: u128) -> NativeInputFingerprint {
    NativeInputFingerprint::new(
        path(&format!("root:/{label}"), object),
        label.into(),
        1,
        uuid,
        128,
        digest(label),
        Some([7; 32]),
        true,
    )
    .unwrap()
}

fn program(profile: &str, inputs: &str, output: &str) -> StaticScriptProgram {
    compile_program(profile, inputs, output).unwrap()
}

fn compile_program(
    profile: &str,
    inputs: &str,
    output: &str,
) -> Result<StaticScriptProgram, ScriptCompileError> {
    let source = format!(
        "inkscript 3; requires {{ procedure_catalog = 8; replay_epoch = 29; }} inputs {{ profile = {profile}; {inputs} }} program {{}} output {{ {output} }} execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"
    );
    compile_inkscript(
        &InkScriptSource::new(InkScriptSourceId::new(840), source.as_bytes()).unwrap(),
        InkScriptRunParameterDecision::Resolve(vec![]),
    )
}

const FOLDER: &str =
    "policy = folder; format = png; folder = \"out\"; naming_template = \"{stem}_{index:2}\";";

#[derive(Default)]
struct Adapter {
    files: BTreeMap<String, NativeInputFingerprint>,
    folders: BTreeMap<String, FolderScan>,
    intents: BTreeMap<u64, String>,
    current: Option<ScriptSessionSnapshot>,
    open: Vec<OpenSessionRecord>,
    captured_open: usize,
    capacity: usize,
    capacity_requested: usize,
    destination_calls: usize,
    authority_reads: usize,
    stale_after_first_read: bool,
}

impl ScriptPlanAdapter for Adapter {
    fn authority_generation(&mut self) -> Result<u64, ScriptPlanAdapterError> {
        self.authority_reads += 1;
        Ok(if self.stale_after_first_read && self.authority_reads > 1 {
            2
        } else {
            1
        })
    }
    fn open_session_set(&mut self) -> Result<OpenSessionSetSnapshot, ScriptPlanAdapterError> {
        Ok(OpenSessionSetSnapshot::new(1, self.open.clone()).unwrap())
    }
    fn resolve_file(
        &mut self,
        id: u64,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<NativeInputFingerprint, ScriptPlanAdapterError> {
        self.files
            .get(&self.intents[&id])
            .cloned()
            .ok_or(ScriptPlanAdapterError::Unavailable)
    }
    fn enumerate_folder(
        &mut self,
        id: u64,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<FolderScan, ScriptPlanAdapterError> {
        self.folders
            .get(&self.intents[&id])
            .cloned()
            .ok_or(ScriptPlanAdapterError::Unavailable)
    }
    fn capture_current_document(
        &mut self,
        _: &ScriptSessionExpectation,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
        self.current
            .clone()
            .ok_or(ScriptPlanAdapterError::Unavailable)
    }
    fn capture_current_sequence(
        &mut self,
        _: &ScriptSequenceExpectation,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSequenceSnapshot, ScriptPlanAdapterError> {
        Err(ScriptPlanAdapterError::Unavailable)
    }
    fn capture_open_session(
        &mut self,
        _: &OpenSessionRecord,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<ScriptSessionSnapshot, ScriptPlanAdapterError> {
        self.captured_open += 1;
        self.current
            .clone()
            .ok_or(ScriptPlanAdapterError::Unavailable)
    }
    fn resolve_destination(
        &mut self,
        request: &ScriptDestinationRequest,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<ValidatedPathIdentity, ScriptPlanAdapterError> {
        self.destination_calls += 1;
        let name = format!("root:/out/{}", request.relative_components().join("/"));
        Ok(ValidatedPathIdentity::expected_absent(
            name.clone(),
            [1; 16],
            [99; 32],
            digest(&name),
            [98; 32],
        )
        .unwrap())
    }
    fn preflight_new_tabs(&mut self, count: usize) -> Result<(), ScriptPlanAdapterError> {
        self.capacity_requested = count;
        if count <= self.capacity {
            Ok(())
        } else {
            Err(ScriptPlanAdapterError::Unavailable)
        }
    }
}

fn plan(
    program: &StaticScriptProgram,
    adapter: &mut Adapter,
) -> Result<ScriptExecutionPlan, ScriptPlanError> {
    let mut grants = vec![];
    for intent in program.path_intents() {
        adapter.intents.insert(intent.id(), intent.text().into());
        let identity = match intent.subject() {
            ScriptPathIntentSubject::Input(_) => adapter
                .files
                .get(intent.text())
                .map(|f| f.path().clone())
                .unwrap_or_else(|| path(&format!("root:/{}", intent.text()), 88)),
            _ => path("root:/out", 89),
        };
        grants.push(
            AuthorityGrant::new(
                intent.id(),
                intent.access(),
                digest(intent.text()),
                1,
                identity,
            )
            .unwrap(),
        );
    }
    let context = ScriptCommandContext::new(
        adapter
            .current
            .as_ref()
            .map(|s| ScriptSessionExpectation::from_snapshot(s).unwrap()),
        None,
    );
    let authority = AuthoritySnapshot::new(
        *program.static_compile_digest(),
        *program.path_intent_digest(),
        1,
        grants,
        context,
        1,
        None,
    )
    .unwrap();
    plan_inkscript(
        program,
        &authority,
        adapter,
        &mut [],
        ScriptPlanLimits::exact_current(),
        &mut || false,
    )
}

fn adapter_with_files(files: Vec<NativeInputFingerprint>) -> Adapter {
    Adapter {
        files: files
            .into_iter()
            .map(|f| (f.display_label().to_owned(), f))
            .collect(),
        ..Adapter::default()
    }
}

#[test]
fn batch_keeps_declaration_order_and_folder_ties_ignore_enumeration_order() {
    let batch = program(
        "batch",
        "file \"z9.inkpod\"; folder \"in\"; file \"a1.inkpod\";",
        FOLDER,
    );
    let canonical = program(
        "canonical",
        "file \"z9.inkpod\"; folder \"in\"; file \"a1.inkpod\";",
        FOLDER,
    );
    for reverse in [false, true] {
        let mut adapter =
            adapter_with_files(vec![file("z9.inkpod", 1, 1), file("a1.inkpod", 2, 2)]);
        let mut files = vec![
            file("b10.inkpod", 3, 3),
            file("b2.inkpod", 4, 4),
            file("B2.inkpod", 5, 5),
        ];
        if reverse {
            files.reverse();
        }
        adapter
            .folders
            .insert("in".into(), FolderScan::new(3, 100, 4, 1, files).unwrap());
        let planned = plan(&batch, &mut adapter).unwrap();
        assert_eq!(
            planned
                .preview_items()
                .iter()
                .map(|p| p.display_label())
                .collect::<Vec<_>>(),
            [
                "z9.inkpod",
                "B2.inkpod",
                "b2.inkpod",
                "b10.inkpod",
                "a1.inkpod"
            ]
        );
        assert_eq!(planned.preview_items()[0].output_name(), "z9_01.png");
        let planned = plan(&canonical, &mut adapter).unwrap();
        assert_eq!(planned.preview_items()[0].display_label(), "a1.inkpod");
    }
}

#[test]
fn batch_open_range_uses_last_stem_run_and_preserves_missing_or_overflow() {
    let source = program(
        "batch",
        "folder \"in\" { cells = range(5, 0); recursive = false; };",
        FOLDER,
    );
    let files = [
        "a4x.inkpod",
        "a5x.inkpod",
        "a0009x.inkpod",
        "plain.inkpod",
        "a99999999999999.inkpod",
    ]
    .into_iter()
    .enumerate()
    .map(|(n, s)| file(s, n as u8 + 1, n as u128 + 1))
    .collect();
    let mut adapter = Adapter::default();
    adapter
        .folders
        .insert("in".into(), FolderScan::new(5, 100, 6, 1, files).unwrap());
    let result = plan(&source, &mut adapter).unwrap();
    assert_eq!(result.input_count(), 4);
    assert!(
        result
            .preview_items()
            .iter()
            .all(|i| i.display_label() != "a4x.inkpod")
    );
}

#[test]
fn batch_duplicate_uuid_is_allowed_but_file_alias_is_rejected() {
    let input = "file \"one.inkpod\"; file \"two.inkpod\";";
    let mut adapter =
        adapter_with_files(vec![file("one.inkpod", 1, 12), file("two.inkpod", 2, 12)]);
    assert_eq!(
        plan(&program("batch", input, FOLDER), &mut adapter)
            .unwrap()
            .input_count(),
        2
    );
    assert!(matches!(
        plan(&program("canonical", input, FOLDER), &mut adapter),
        Err(ScriptPlanError::DuplicateInput)
    ));
    adapter
        .files
        .insert("two.inkpod".into(), file("one.inkpod", 1, 12));
    assert!(matches!(
        plan(&program("batch", input, FOLDER), &mut adapter),
        Err(ScriptPlanError::DuplicateInput)
    ));
}

#[test]
fn pathless_active_batch_names_and_duplicates_are_retained_but_canonical_stem_is_invalid() {
    let mut core = Core::new();
    core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let current =
        ScriptSessionSnapshot::capture(1, 1, 1, "current-cell.inkpod".into(), 1, None, &core)
            .unwrap();
    let mut adapter = Adapter {
        current: Some(current),
        ..Adapter::default()
    };
    let planned = plan(
        &program("batch", "current_document; current_document;", FOLDER),
        &mut adapter,
    )
    .unwrap();
    assert_eq!(planned.input_count(), 2);
    assert_eq!(
        planned.preview_items()[0].output_name(),
        "active-document_01.png"
    );
    assert!(matches!(
        plan(
            &program("canonical", "current_document;", FOLDER),
            &mut adapter
        ),
        Err(ScriptPlanError::InvalidInput)
    ));
    let index_only = FOLDER.replace("{stem}_{index:2}", "{index:2}");
    assert!(
        plan(
            &program("canonical", "current_document;", &index_only),
            &mut adapter
        )
        .is_ok()
    );
}

#[test]
fn new_tabs_preflight_is_bounded_and_never_resolves_file_destinations() {
    let source = program(
        "batch",
        "file \"one.inkpod\"; file \"two.inkpod\";",
        "policy = new_tabs;",
    );
    let mut adapter = adapter_with_files(vec![file("one.inkpod", 1, 1), file("two.inkpod", 2, 2)]);
    adapter.capacity = 1;
    assert!(matches!(
        plan(&source, &mut adapter),
        Err(ScriptPlanError::Adapter(
            ScriptPlanAdapterError::Unavailable
        ))
    ));
    assert_eq!(adapter.capacity_requested, 2);
    assert_eq!(adapter.destination_calls, 0);
    adapter.capacity = 2;
    let result = plan(&source, &mut adapter).unwrap();
    assert_eq!(result.input_count(), 2);
    assert_eq!(adapter.destination_calls, 0);
    assert!(
        result
            .preview_items()
            .iter()
            .all(|p| p.destination_key().is_empty())
    );
}

#[test]
fn raster_fingerprints_keep_bytes_identity_and_canonical_display_number_fallback() {
    for (label, number) in [
        ("a0012x.png", 12),
        ("plain.tiff", 1),
        ("a0.bmp", 1),
        ("a999999999999.tga", 1),
    ] {
        let fingerprint = NativeInputFingerprint::new_raster(
            path(&format!("root:/{label}"), 1),
            label.into(),
            128,
            [17; 32],
            Some([7; 32]),
        )
        .unwrap();
        assert!(!fingerprint.is_native());
        assert_eq!(fingerprint.display_number(), number);
        assert_eq!(fingerprint.document_uuid(), u128::from_le_bytes([17; 16]));
    }
}

#[test]
fn batch_reads_open_file_disk_while_canonical_captures_live_session() {
    let mut core = Core::new();
    core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let source_file = file(
        "disk.inkpod",
        1,
        core.document_info().unwrap().document_uuid,
    );
    let open = OpenSessionRecord::new(
        1,
        1,
        core.document_info().unwrap().document_uuid,
        source_file.path().clone(),
    )
    .unwrap();
    let current = ScriptSessionSnapshot::capture(
        1,
        1,
        1,
        "live.inkpod".into(),
        1,
        Some(source_file.path().clone()),
        &core,
    )
    .unwrap();
    let mut adapter = adapter_with_files(vec![source_file]);
    adapter.open.push(open);
    adapter.current = Some(current);
    let batch = plan(
        &program("batch", "file \"disk.inkpod\";", FOLDER),
        &mut adapter,
    )
    .unwrap();
    assert_eq!(batch.preview_items()[0].display_label(), "disk.inkpod");
    assert_eq!(adapter.captured_open, 0);
    let canonical = plan(
        &program("canonical", "file \"disk.inkpod\";", FOLDER),
        &mut adapter,
    )
    .unwrap();
    assert_eq!(canonical.preview_items()[0].display_label(), "live.inkpod");
    assert_eq!(adapter.captured_open, 1);
}

#[test]
fn folder_collision_and_raster_overwrite_fail_before_execution() {
    let output = FOLDER.replace("{stem}_{index:2}", "constant");
    let mut adapter = adapter_with_files(vec![file("one.inkpod", 1, 1), file("two.inkpod", 2, 2)]);
    assert!(matches!(
        plan(
            &program(
                "batch",
                "file \"one.inkpod\"; file \"two.inkpod\";",
                &output
            ),
            &mut adapter
        ),
        Err(ScriptPlanError::OutputCollision)
    ));
    let raster = NativeInputFingerprint::new_raster(
        path("root:/one.png", 3),
        "one.png".into(),
        128,
        [17; 32],
        Some([7; 32]),
    )
    .unwrap();
    let overwrite = "policy = explicit_overwrite; format = inkpod;";
    assert!(matches!(
        compile_program("batch", "file \"one.png\";", overwrite),
        Err(ScriptCompileError::Envelope(
            inkpod_format::InkScriptEnvelopeErrorCode::IncompatibleOutputPolicy
        ))
    ));
    adapter.folders.insert(
        "in".into(),
        FolderScan::new(1, 7, 2, 1, vec![raster]).unwrap(),
    );
    assert!(matches!(
        plan(&program("batch", "folder \"in\";", overwrite), &mut adapter),
        Err(ScriptPlanError::UnsupportedAtomicOverwrite)
    ));
}

#[test]
fn file_plan_retains_original_target_and_confirmation_digest_binds_that_context() {
    let mut core = Core::new();
    core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let current =
        ScriptSessionSnapshot::capture(1, 1, 1, "current-cell.inkpod".into(), 1, None, &core)
            .unwrap();
    let expected = ScriptSessionExpectation::from_snapshot(&current).unwrap();
    let source = program("batch", "file \"one.inkpod\";", FOLDER);
    let mut adapter = adapter_with_files(vec![file("one.inkpod", 1, 12)]);
    adapter.current = Some(current);
    let first = plan(&source, &mut adapter).unwrap();
    assert_eq!(first.command_context().current_document(), Some(&expected));
    adapter.current = Some(
        ScriptSessionSnapshot::capture(2, 1, 1, "current-cell.inkpod".into(), 1, None, &core)
            .unwrap(),
    );
    let second = plan(&source, &mut adapter).unwrap();
    assert_ne!(first.plan_digest(), second.plan_digest());
    assert_ne!(first.command_context(), second.command_context());
    assert_eq!(first.preview_items(), second.preview_items());
}

#[test]
fn authority_changed_during_destination_planning_prevents_plan_publication() {
    let source = program("batch", "file \"one.inkpod\";", FOLDER);
    let mut adapter = adapter_with_files(vec![file("one.inkpod", 1, 12)]);
    adapter.stale_after_first_read = true;
    assert!(matches!(
        plan(&source, &mut adapter),
        Err(ScriptPlanError::StaleAuthority)
    ));
    assert_eq!(adapter.destination_calls, 1);
    assert_eq!(adapter.authority_reads, 2);
}
