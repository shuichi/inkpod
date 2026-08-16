use inkpod_format::{
    INKSCRIPT_FILE_VERSION, INKSCRIPT_PROCEDURE_CATALOG_VERSION, INKSCRIPT_REQUIRED_REPLAY_EPOCH,
    InkScriptCellSelection, InkScriptEnvelopeErrorCode, InkScriptExecutionFailure,
    InkScriptInputDeclarationKind, InkScriptNumberDirection, InkScriptOutput,
    InkScriptOutputFormat, InkScriptPathIntentAccess, InkScriptSchemaView,
    InkScriptSemanticErrorCode, InkScriptSource, InkScriptSourceId,
    build_inkscript_orchestration_envelope, build_inkscript_semantic, emit_inkscript_canonical,
    parse_inkscript,
};

fn source(bytes: &[u8]) -> InkScriptSource {
    InkScriptSource::new(InkScriptSourceId::new(91), bytes).expect("fixture must be UTF-8")
}

fn schema() -> InkScriptSchemaView<'static> {
    InkScriptSchemaView::exact_current(&[], &[]).expect("language-only schema must compose")
}

fn envelope(
    bytes: &[u8],
) -> Result<inkpod_format::InkScriptOrchestrationEnvelope, inkpod_format::InkScriptEnvelopeError> {
    let source = source(bytes);
    let parsed = parse_inkscript(&source);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    let semantic = build_inkscript_semantic(&parsed, &schema()).expect("semantic conversion");
    build_inkscript_orchestration_envelope(&semantic)
}

fn complete_file(requires: &str, inputs: &str, output: &str, execution: &str) -> String {
    format!(
        "inkscript 2;\nrequires {{ {requires} }}\ninputs {{ {inputs} }}\nprogram {{}}\noutput {{ {output} }}\nexecution {{ {execution} }}\n"
    )
}

#[test]
fn typed_envelope_round_trips_all_input_kinds_and_path_intents_without_io() {
    let input = source(
        br#"inkscript 2;
execution { preview_before_save = true; wait_ms = 25; failure = continue; }
output { direction = descending; start_number = 7; basename = "painted"; cell_folder = true; folder = "missing/out"; format = inkpod; policy = duplicate; }
program {}
inputs {
    file "missing/A001.inkpod" { cells = range(2, 4); };
    folder "missing/cut" { recursive = false; cells = all; };
    current_document;
    current_sequence { cells = range(8, 9); };
}
meta {
    description = "typed envelope";
    name = "Batch";
    extensions = [
        { value = "approved"; key = "org.example.review-note"; },
        { key = "com.example.owner"; value = "ink"; },
    ];
}
requires { replay_epoch = 23; procedure_catalog = 2; }
"#,
    );
    let parsed = parse_inkscript(&input);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    let semantic = build_inkscript_semantic(&parsed, &schema()).unwrap();
    let unchanged = semantic.clone();
    let typed = build_inkscript_orchestration_envelope(&semantic).unwrap();

    assert_eq!(typed.file_version(), INKSCRIPT_FILE_VERSION);
    assert_eq!(
        typed.requirements().procedure_catalog_version(),
        INKSCRIPT_PROCEDURE_CATALOG_VERSION
    );
    assert_eq!(
        typed.requirements().replay_epoch(),
        INKSCRIPT_REQUIRED_REPLAY_EPOCH
    );
    assert_eq!(typed.metadata().name(), Some("Batch"));
    assert_eq!(typed.metadata().description(), Some("typed envelope"));
    assert_eq!(typed.metadata().extensions().len(), 2);
    assert_eq!(
        typed.metadata().extensions()[0].key(),
        "org.example.review-note"
    );
    assert_eq!(typed.metadata().extensions()[0].value(), "approved");

    assert_eq!(typed.inputs().len(), 4);
    assert_eq!(
        typed.inputs()[0].kind(),
        InkScriptInputDeclarationKind::File
    );
    assert_eq!(typed.inputs()[0].path_text(), Some("missing/A001.inkpod"));
    assert_eq!(
        typed.inputs()[0].cells(),
        InkScriptCellSelection::Inclusive { first: 2, last: 4 }
    );
    assert_eq!(
        typed.inputs()[1].kind(),
        InkScriptInputDeclarationKind::Folder
    );
    assert_eq!(typed.inputs()[1].cells(), InkScriptCellSelection::All);
    assert_eq!(
        typed.inputs()[2].kind(),
        InkScriptInputDeclarationKind::CurrentDocument
    );
    assert_eq!(
        typed.inputs()[3].kind(),
        InkScriptInputDeclarationKind::CurrentSequence
    );

    let InkScriptOutput::Duplicate(numbered) = typed.output() else {
        panic!("expected duplicate output")
    };
    assert_eq!(typed.output().format(), InkScriptOutputFormat::Inkpod);
    assert_eq!(numbered.folder(), "missing/out");
    assert!(numbered.cell_folder());
    assert_eq!(numbered.basename(), "painted");
    assert_eq!(numbered.start_number(), 7);
    assert_eq!(numbered.direction(), InkScriptNumberDirection::Descending);
    assert_eq!(
        typed.execution().failure(),
        InkScriptExecutionFailure::Continue
    );
    assert_eq!(typed.execution().wait_ms(), 25);
    assert!(typed.execution().preview_before_save());

    let preview = typed.path_intent_preview();
    assert_eq!(preview.intents().len(), 3);
    assert_eq!(
        preview.intents()[0].access(),
        InkScriptPathIntentAccess::Read
    );
    assert_eq!(preview.intents()[0].input_index(), Some(0));
    assert_eq!(preview.intents()[0].text(), "missing/A001.inkpod");
    assert_eq!(
        preview.intents()[1].access(),
        InkScriptPathIntentAccess::Enumerate
    );
    assert_eq!(preview.intents()[1].input_index(), Some(1));
    assert_eq!(preview.intents()[1].text(), "missing/cut");
    assert_eq!(
        preview.intents()[2].access(),
        InkScriptPathIntentAccess::Create
    );
    assert_eq!(preview.intents()[2].input_index(), None);
    assert_eq!(preview.intents()[2].text(), "missing/out");

    let second = build_inkscript_orchestration_envelope(&semantic).unwrap();
    assert_eq!(second, typed);
    assert_eq!(semantic, unchanged, "typed conversion is a pure query");

    let canonical = emit_inkscript_canonical(&semantic, &schema()).unwrap();
    let canonical_source = source(&canonical);
    let reparsed = parse_inkscript(&canonical_source);
    let reparsed_semantic = build_inkscript_semantic(&reparsed, &schema()).unwrap();
    let round_trip = build_inkscript_orchestration_envelope(&reparsed_semantic).unwrap();
    assert_eq!(round_trip, typed);
    assert!(
        !String::from_utf8(canonical)
            .unwrap()
            .contains("inkpod_version")
    );

    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<inkpod_format::InkScriptOrchestrationEnvelope>();
}

#[test]
fn exact_current_versions_and_complete_file_boundary_fail_closed() {
    assert_eq!(INKSCRIPT_FILE_VERSION, 2);
    assert_eq!(INKSCRIPT_PROCEDURE_CATALOG_VERSION, 2);
    assert_eq!(INKSCRIPT_REQUIRED_REPLAY_EPOCH, 23);

    let noncurrent_file = source(
        b"inkscript 1; requires { procedure_catalog = 2; replay_epoch = 23; } inputs {} program {} output { policy = explicit_overwrite; format = inkpod; } execution { failure = stop; wait_ms = 0; preview_before_save = true; }",
    );
    assert!(!parse_inkscript(&noncurrent_file).is_valid());

    for (requires, expected) in [
        (
            "procedure_catalog = 1; replay_epoch = 23;",
            InkScriptEnvelopeErrorCode::NonCurrentProcedureCatalog,
        ),
        (
            "procedure_catalog = 2; replay_epoch = 24;",
            InkScriptEnvelopeErrorCode::NonCurrentReplayEpoch,
        ),
    ] {
        let text = complete_file(
            requires,
            "file \"a.inkpod\";",
            "policy = explicit_overwrite; format = inkpod;",
            "failure = stop; wait_ms = 0; preview_before_save = true;",
        );
        let error = envelope(text.as_bytes()).unwrap_err();
        assert_eq!(error.code(), expected);
        assert!(!error.path().is_empty());
        assert!(!error.code().as_str().is_empty());
    }

    let fragment = source(
        b"inkscript_fragment 2; requires { procedure_catalog = 2; replay_epoch = 23; } program {}",
    );
    let parsed = parse_inkscript(&fragment);
    let semantic = build_inkscript_semantic(&parsed, &schema()).unwrap();
    assert_eq!(
        build_inkscript_orchestration_envelope(&semantic)
            .unwrap_err()
            .code(),
        InkScriptEnvelopeErrorCode::NotCompleteFile
    );
}

#[test]
fn metadata_ranges_and_execution_bounds_reject_invalid_values_atomically() {
    let base_output = "policy = duplicate; format = inkpod; folder = \"\"; cell_folder = false; basename = \"\"; start_number = 0; direction = ascending;";
    let base_execution = "failure = continue; wait_ms = 3600000; preview_before_save = false;";

    for (meta, expected) in [
        (
            "meta { extensions = [{ key = \"invalid\"; value = \"x\"; }]; }",
            InkScriptEnvelopeErrorCode::InvalidMetadataExtension,
        ),
        (
            "meta { extensions = [{ key = \"org.example.x\"; value = \"1\"; }, { key = \"org.example.x\"; value = \"2\"; }]; }",
            InkScriptEnvelopeErrorCode::DuplicateMetadataExtension,
        ),
        (
            "meta { extensions = 1; }",
            InkScriptEnvelopeErrorCode::InvalidType,
        ),
    ] {
        let text = format!(
            "inkscript 2; requires {{ procedure_catalog = 2; replay_epoch = 23; }} {meta} inputs {{ file \"a.inkpod\"; }} program {{}} output {{ {base_output} }} execution {{ {base_execution} }}"
        );
        assert_eq!(envelope(text.as_bytes()).unwrap_err().code(), expected);
    }

    for (inputs, expected) in [
        (
            "file \"a.inkpod\" { cells = range(0, 1); };",
            InkScriptEnvelopeErrorCode::InvalidCellRange,
        ),
        (
            "folder \"cells\" { cells = range(4, 3); };",
            InkScriptEnvelopeErrorCode::InvalidCellRange,
        ),
        (
            "file \"a.inkpod\" { cells = range(1, 4294967296); };",
            InkScriptEnvelopeErrorCode::NumericOverflow,
        ),
        (
            "folder \"cells\" { recursive = true; };",
            InkScriptEnvelopeErrorCode::UnsupportedRecursiveInput,
        ),
        (
            "current_document { cells = range(1, 2); };",
            InkScriptEnvelopeErrorCode::InvalidCellRange,
        ),
    ] {
        let text = complete_file(
            "procedure_catalog = 2; replay_epoch = 23;",
            inputs,
            base_output,
            base_execution,
        );
        assert_eq!(envelope(text.as_bytes()).unwrap_err().code(), expected);
    }

    for execution in [
        "failure = continue; wait_ms = 3600001; preview_before_save = false;",
        "failure = continue; wait_ms = -1; preview_before_save = false;",
        "failure = continue; wait_ms = 4294967296; preview_before_save = false;",
    ] {
        let text = complete_file(
            "procedure_catalog = 2; replay_epoch = 23;",
            "file \"a.inkpod\";",
            base_output,
            execution,
        );
        let code = envelope(text.as_bytes()).unwrap_err().code();
        assert!(matches!(
            code,
            InkScriptEnvelopeErrorCode::InvalidExecutionPolicy
                | InkScriptEnvelopeErrorCode::NumericOverflow
        ));
    }

    let invalid = source(
        complete_file(
            "procedure_catalog = 2; replay_epoch = 23;",
            "file \"a.inkpod\" { cells = range(0, 1); };",
            base_output,
            base_execution,
        )
        .as_bytes(),
    );
    let parsed = parse_inkscript(&invalid);
    let semantic = build_inkscript_semantic(&parsed, &schema()).unwrap();
    let unchanged = semantic.clone();
    assert_eq!(
        build_inkscript_orchestration_envelope(&semantic)
            .unwrap_err()
            .code(),
        InkScriptEnvelopeErrorCode::InvalidCellRange
    );
    assert_eq!(
        semantic, unchanged,
        "failure must not mutate semantic input"
    );
}

#[test]
fn output_variants_are_closed_and_explicit_overwrite_intents_are_typed() {
    let common_requires = "procedure_catalog = 2; replay_epoch = 23;";
    let common_execution = "failure = stop; wait_ms = 0; preview_before_save = true;";
    let file_input = "file \"A001.inkpod\";";

    let new_save = complete_file(
        common_requires,
        file_input,
        "policy = new_save; format = inkpod; folder = \"out\"; cell_folder = false; basename = \"\"; start_number = 12; direction = ascending;",
        common_execution,
    );
    let typed = envelope(new_save.as_bytes()).unwrap();
    let InkScriptOutput::NewSave(numbered) = typed.output() else {
        panic!("expected new-save output")
    };
    assert_eq!(numbered.start_number(), 12);

    let overwrite = complete_file(
        common_requires,
        file_input,
        "policy = explicit_overwrite; format = inkpod;",
        common_execution,
    );
    let typed = envelope(overwrite.as_bytes()).unwrap();
    assert!(matches!(typed.output(), InkScriptOutput::ExplicitOverwrite));
    let preview = typed.path_intent_preview();
    assert_eq!(preview.intents().len(), 2);
    assert_eq!(
        preview.intents()[0].access(),
        InkScriptPathIntentAccess::Read
    );
    assert_eq!(
        preview.intents()[1].access(),
        InkScriptPathIntentAccess::Replace
    );
    assert_eq!(preview.intents()[1].text(), "A001.inkpod");

    for output in [
        "policy = duplicate; format = png; folder = \"\"; cell_folder = false; basename = \"\"; start_number = 0; direction = ascending;",
        "policy = duplicate; format = inkpod; folder = \"\"; cell_folder = false; basename = \"\"; start_number = 0; direction = sideways;",
    ] {
        let text = complete_file(common_requires, file_input, output, common_execution);
        assert_eq!(
            envelope(text.as_bytes()).unwrap_err().code(),
            InkScriptEnvelopeErrorCode::InvalidOutput
        );
    }

    let current_overwrite = complete_file(
        common_requires,
        "current_document;",
        "policy = explicit_overwrite; format = inkpod;",
        common_execution,
    );
    assert_eq!(
        envelope(current_overwrite.as_bytes()).unwrap_err().code(),
        InkScriptEnvelopeErrorCode::IncompatibleOutputPolicy
    );

    for output in [
        "policy = explicit_overwrite; format = inkpod; folder = \"out\";",
        "policy = duplicate; format = inkpod; folder = \"\"; cell_folder = false; basename = \"\"; start_number = 0; direction = ascending; native_version = 26;",
    ] {
        let text =
            source(complete_file(common_requires, file_input, output, common_execution).as_bytes());
        let parsed = parse_inkscript(&text);
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
        let error = build_inkscript_semantic(&parsed, &schema()).unwrap_err();
        assert_eq!(error.code(), InkScriptSemanticErrorCode::UnknownFieldSchema);
    }

    let unknown_policy = source(
        complete_file(
            common_requires,
            file_input,
            "policy = automatic; format = inkpod;",
            common_execution,
        )
        .as_bytes(),
    );
    let parsed = parse_inkscript(&unknown_policy);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    assert_eq!(
        build_inkscript_semantic(&parsed, &schema())
            .unwrap_err()
            .code(),
        InkScriptSemanticErrorCode::UnknownRecordSchema
    );
}
