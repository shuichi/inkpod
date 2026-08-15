use super::*;
use crate::primitive::CanonicalInvocation;
use crate::{
    BatchColorPair, Core, DEFAULT_DPI_MILLI, LayerKind, PixelValue, PrimitiveId, ProcedureId,
    StateId,
};
use inkpod_format::{
    InkScriptRunParameterChoice, InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId,
    InkScriptValue, encode_procedure_file,
};

fn document_uuid(value: u128) -> String {
    let hex = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn source(text: String) -> InkScriptSource {
    InkScriptSource::new(InkScriptSourceId::new(109), text.as_bytes())
        .expect("fixture source must be valid UTF-8")
}

fn complete_source(parameters: &str, bindings: &str, program: &str) -> InkScriptSource {
    source(format!(
        r#"inkscript 1;
requires {{ procedure_catalog = 1; replay_epoch = 23; }}
inputs {{ current_document; }}
parameters {{ {parameters} }}
bindings {{ {bindings} }}
program {{ {program} }}
output {{ policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "dry"; start_number = 1; direction = ascending; }}
execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}
"#
    ))
}

fn core() -> Core {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core
}

fn success_source(core: &Core) -> InkScriptSource {
    let info = core.document_info().unwrap();
    complete_source(
        r#"param new_name: string = "Stored" { ask = each_run; };"#,
        &format!(
            r#"
let target_layer = select layer {{ source_document_uuid = uuid"{}"; persistent_id = {}; }};
let paint = select plane {{ layer = $target_layer; plane_kind = color; cardinality = one; missing = error; }};
"#,
            document_uuid(info.document_uuid),
            info.layer_id
        ),
        r#"
assert selection { empty = true; bounds = none; };
step "Replace transparent pixels" {
    enabled = true;
    invoke replace_raster_colors {
        plane_id = $paint;
        pairs = [
            { enabled = true; old = rgba8(0, 0, 0, 0); new = rgba8(12, 34, 56, 255); },
        ];
    };
}
step "Rename" {
    enabled = true;
    invoke set_plane_properties {
        plane_id = $paint;
        visible = true;
        editable = true;
        opacity_milli = 1000;
        name = $new_name;
    };
}
step "Repeat rename" {
    enabled = true;
    invoke set_plane_properties {
        plane_id = $paint;
        visible = true;
        editable = true;
        opacity_milli = 1000;
        name = $new_name;
    };
}
"#,
    )
}

fn accepted_parameters() -> InkScriptRunParameterDecision {
    InkScriptRunParameterDecision::Resolve(vec![InkScriptRunParameterChoice::Override {
        name: "new_name".to_owned(),
        value: InkScriptValue::String("Renamed".to_owned()),
    }])
}

fn compile_success(core: &Core) -> StaticScriptProgram {
    compile_inkscript(&success_source(core), accepted_parameters()).unwrap()
}

fn compile_native_success(core: &Core) -> StaticScriptProgram {
    let current = success_source(core);
    let native = source(current.text().replace(
        "inputs { current_document; }",
        "inputs { file \"fixture.inkpod\"; }",
    ));
    compile_inkscript(&native, accepted_parameters()).unwrap()
}

fn assert_same_document(left: &Core, right: &Core) {
    assert_eq!(
        left.document_state_digest().unwrap(),
        right.document_state_digest().unwrap()
    );
    assert_eq!(
        left.document_info().unwrap(),
        right.document_info().unwrap()
    );
    assert_eq!(left.history_entries(), right.history_entries());
    assert_eq!(
        left.journal_state().unwrap(),
        right.journal_state().unwrap()
    );
    assert_eq!(left.next_id, right.next_id);
    assert_eq!(left.next_procedure, right.next_procedure);
    assert_eq!(left.next_state, right.next_state);
    assert_eq!(left.next_journal_event, right.next_journal_event);
    assert_eq!(left.next_branch, right.next_branch);
    assert_eq!(left.savepoint, right.savepoint);
    assert_eq!(
        left.editor_session.as_ref().unwrap().savepoint,
        right.editor_session.as_ref().unwrap().savepoint
    );
}

#[test]
fn compiler_freezes_parameters_and_checks_cancel_invalid_and_aggregate_resources() {
    let core = core();
    let valid_source = success_source(&core);

    assert_eq!(
        compile_inkscript(&valid_source, InkScriptRunParameterDecision::Cancel),
        Err(ScriptCompileError::ParameterCancelled)
    );
    assert!(matches!(
        compile_inkscript(
            &valid_source,
            InkScriptRunParameterDecision::Resolve(Vec::new())
        ),
        Err(ScriptCompileError::Type(_))
    ));

    let first = compile_inkscript(&valid_source, accepted_parameters()).unwrap();
    let second = compile_inkscript(&valid_source, accepted_parameters()).unwrap();
    assert_eq!(first.static_compile_digest, second.static_compile_digest);
    assert_eq!(first.path_intent_digest, second.path_intent_digest);
    assert_eq!(first.budget.max_invocations, 3);
    assert_eq!(
        first.parameters["new_name"].kind(),
        &inkpod_format::InkScriptTypedValueKind::String("Renamed".to_owned())
    );
    assert!(
        first
            .frozen_arguments
            .iter()
            .all(|value| !value_contains_reference(value, "new_name"))
    );

    let limited = compile_inkscript_with_limits(
        &valid_source,
        accepted_parameters(),
        ScriptCompileLimits::exact_current().with_invocations(2),
    );
    assert_eq!(limited, Err(ScriptCompileError::ResourceLimit));

    let invalid =
        source("inkscript 1; requires { procedure_catalog = 1; replay_epoch = 23; }".to_owned());
    assert!(matches!(
        compile_inkscript(&invalid, InkScriptRunParameterDecision::Resolve(Vec::new())),
        Err(ScriptCompileError::Syntax) | Err(ScriptCompileError::Semantic(_))
    ));
}

#[test]
fn staged_memory_and_native_dry_runs_match_the_direct_canonical_route() {
    let source_core = core();
    let source_digest = source_core.document_state_digest().unwrap();
    let source_info = source_core.document_info().unwrap();
    let source_history = source_core.history_entries();
    let program = compile_success(&source_core);
    let captured = capture_in_memory_input(&source_core).unwrap();
    let mut never_cancel = || false;
    let mut scripted = run_inkscript_dry(&program, captured, &mut never_cancel).unwrap();

    assert_eq!(scripted.report.statements.len(), 4);
    assert_eq!(
        scripted.report.statements,
        vec![
            crate::script::report::ScriptStatementOutcome::AssertPassed,
            crate::script::report::ScriptStatementOutcome::Committed,
            crate::script::report::ScriptStatementOutcome::Committed,
            crate::script::report::ScriptStatementOutcome::NoOp,
        ]
    );
    assert_eq!(scripted.report.commit_count, 2);
    assert!(scripted.report.results.is_empty());
    assert_eq!(source_core.document_state_digest().unwrap(), source_digest);
    assert_eq!(source_core.document_info().unwrap(), source_info);
    assert_eq!(source_core.history_entries(), source_history);

    let info = source_core.document_info().unwrap();
    let mut direct = source_core.clone();
    direct
        .execute_canonical_invocation(CanonicalInvocation::ReplaceRasterColors {
            plane_id: info.color_plane_id,
            pairs: vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([0, 0, 0, 0]),
                new: PixelValue::Rgba([12, 34, 56, 255]),
            }],
        })
        .unwrap();
    for _ in 0..2 {
        direct
            .execute_canonical_invocation(CanonicalInvocation::SetPlaneProperties {
                plane_id: info.color_plane_id,
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                name: "Renamed".to_owned(),
            })
            .unwrap();
    }
    assert_same_document(&scripted.staged, &direct);
    assert_eq!(
        scripted.report.final_state_digest,
        direct.document_state_digest().unwrap()
    );
    assert_eq!(
        scripted.report.final_revision,
        direct.document_info().unwrap().document_revision
    );
    assert_eq!(scripted.report.next_stable_id, direct.next_id.next_raw());

    let final_digest = scripted.staged.document_state_digest().unwrap();
    scripted.staged.undo().unwrap();
    scripted.staged.undo().unwrap();
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        source_digest
    );
    scripted.staged.redo().unwrap();
    scripted.staged.redo().unwrap();
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        final_digest
    );

    let native = source_core
        .build_procedure_file(
            source_core.savepoint,
            source_core.editor_session.as_ref().unwrap().savepoint,
        )
        .unwrap();
    let bytes = encode_procedure_file(&native).unwrap();
    let native_program = compile_native_success(&source_core);
    let mut never_cancel = || false;
    let native_result = run_inkscript_dry(
        &native_program,
        native_script_input(&bytes),
        &mut never_cancel,
    )
    .unwrap();
    assert_same_document(&native_result.staged, &direct);
}

#[test]
fn binding_skip_cancel_stale_and_overflow_fail_atomically_without_id_consumption() {
    let base = core();
    let info = base.document_info().unwrap();
    let source_before = (
        base.document_state_digest().unwrap(),
        base.document_info().unwrap(),
        base.history_entries(),
        base.next_id,
        base.next_procedure,
        base.next_state,
    );

    let missing = complete_source(
        "",
        r#"let paint = select plane { name = "absent"; missing = error; };"#,
        r#"step "Missing" { enabled = true; invoke set_plane_properties { plane_id = $paint; visible = true; editable = true; opacity_milli = 1000; name = "Never"; }; }"#,
    );
    let missing =
        compile_inkscript(&missing, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_dry(
            &missing,
            capture_in_memory_input(&base).unwrap(),
            &mut never_cancel
        ),
        Err(ScriptRunError::Binding(
            crate::script::bind::InkScriptBindingError::MissingSelector
        ))
    ));

    let skipped = complete_source(
        "",
        r#"let paint = select plane { name = "absent"; missing = skip_dependents; };"#,
        r#"step "Skipped" { enabled = true; invoke set_plane_properties { plane_id = $paint; visible = true; editable = true; opacity_milli = 1000; name = "Never"; }; }"#,
    );
    let skipped =
        compile_inkscript(&skipped, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut never_cancel = || false;
    let skipped = run_inkscript_dry(
        &skipped,
        capture_in_memory_input(&base).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        skipped.report.statements,
        vec![crate::script::report::ScriptStatementOutcome::Skipped]
    );
    assert_eq!(skipped.report.commit_count, 0);
    assert_eq!(
        skipped.staged.document_state_digest().unwrap(),
        source_before.0
    );

    let ambiguous_core = {
        let mut value = base.clone();
        value.create_layer(LayerKind::Raster, "Second").unwrap();
        value
    };
    let ambiguous = complete_source(
        "",
        r#"let target = select layer { cardinality = one; missing = error; };"#,
        r#"step "Ambiguous" { enabled = true; invoke set_layer_properties { layer_id = $target; visible = true; editable = true; opacity_milli = 1000; name = "Never"; }; }"#,
    );
    let ambiguous = compile_inkscript(
        &ambiguous,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_dry(
            &ambiguous,
            capture_in_memory_input(&ambiguous_core).unwrap(),
            &mut never_cancel
        ),
        Err(ScriptRunError::Binding(
            crate::script::bind::InkScriptBindingError::AmbiguousSelector
        ))
    ));

    let program = compile_success(&base);
    let mut cancel = || true;
    assert!(matches!(
        run_inkscript_dry(
            &program,
            capture_in_memory_input(&base).unwrap(),
            &mut cancel
        ),
        Err(ScriptRunError::Cancelled)
    ));

    let mut checks = 0_u32;
    let mut cancel_after_first_step = || {
        checks += 1;
        checks == 5
    };
    assert!(matches!(
        run_inkscript_dry(
            &program,
            capture_in_memory_input(&base).unwrap(),
            &mut cancel_after_first_step
        ),
        Err(ScriptRunError::Cancelled)
    ));
    assert_eq!(base.document_state_digest().unwrap(), source_before.0);
    assert_eq!(base.next_procedure, source_before.4);
    assert_eq!(base.next_state, source_before.5);

    let mut stale_source = base.clone();
    let fingerprint = capture_in_memory_fingerprint(&stale_source).unwrap();
    stale_source
        .set_layer_properties(info.layer_id, true, true, 1_000, "Changed after capture")
        .unwrap();
    let captured = capture_in_memory_input_at(&stale_source, fingerprint);
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_dry(&program, captured, &mut never_cancel),
        Err(ScriptRunError::StaleInput)
    ));

    let mut overflowing = base.clone();
    overflowing.next_procedure = ProcedureId::from_raw(crate::MAX_PERSISTENT_NUMERIC_ID);
    overflowing.next_state = StateId::from_raw(crate::MAX_PERSISTENT_NUMERIC_ID);
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_dry(
            &program,
            capture_in_memory_input(&overflowing).unwrap(),
            &mut never_cancel
        ),
        Err(ScriptRunError::ResourceLimit)
    ));

    assert_eq!(base.document_state_digest().unwrap(), source_before.0);
    assert_eq!(base.document_info().unwrap(), source_before.1);
    assert_eq!(base.history_entries(), source_before.2);
    assert_eq!(base.next_id, source_before.3);
    assert_eq!(base.next_procedure, source_before.4);
    assert_eq!(base.next_state, source_before.5);

    let mut never_cancel = || false;
    let native_program = compile_native_success(&base);
    assert!(matches!(
        run_inkscript_dry(
            &native_program,
            native_script_input(b"not an inkpod file"),
            &mut never_cancel
        ),
        Err(ScriptRunError::Core(_))
    ));
}

#[test]
fn runtime_result_ordinals_and_thread_ownership_are_explicit() {
    crate::script::execute::test_result_materialization_contract();

    fn assert_send_sync<T: Send + Sync>() {}
    fn assert_send<T: Send>() {}
    assert_send_sync::<StaticScriptProgram>();
    assert_send_sync::<crate::script::report::ScriptDryRunReport>();
    assert_send::<ScriptDryRunResult>();
    assert_eq!(PrimitiveId::REPLACE_RASTER_COLORS.get(), 0x0005_0040);
}

fn value_contains_reference(value: &inkpod_format::InkScriptTypedValue, root: &str) -> bool {
    match value.kind() {
        inkpod_format::InkScriptTypedValueKind::Reference { root: actual, .. } => actual == root,
        inkpod_format::InkScriptTypedValueKind::Constructor { arguments, .. }
        | inkpod_format::InkScriptTypedValueKind::List(arguments) => arguments
            .iter()
            .any(|value| value_contains_reference(value, root)),
        inkpod_format::InkScriptTypedValueKind::Record(fields) => fields
            .values()
            .any(|value| value_contains_reference(value, root)),
        _ => false,
    }
}
