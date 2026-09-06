use inkpod_format::{
    INKSCRIPT_FILE_VERSION, InkScriptCellSelection, InkScriptInputProfile, InkScriptOutput,
    InkScriptOutputFormat, InkScriptSchemaView, InkScriptSource, InkScriptSourceId,
    build_inkscript_orchestration_envelope, build_inkscript_semantic, emit_inkscript_canonical,
    parse_inkscript,
};

fn text(inputs: &str, output: &str) -> String {
    format!(
        "inkscript 3; requires {{ procedure_catalog = 8; replay_epoch = 29; }} inputs {{ {inputs} }} program {{}} output {{ {output} }} execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"
    )
}

fn envelope(
    inputs: &str,
    output: &str,
) -> Result<inkpod_format::InkScriptOrchestrationEnvelope, String> {
    let source =
        InkScriptSource::new(InkScriptSourceId::new(404), text(inputs, output).as_bytes()).unwrap();
    let parsed = parse_inkscript(&source);
    if !parsed.is_valid() {
        return Err(format!("{:?}", parsed.diagnostics()));
    }
    let schema = InkScriptSchemaView::exact_current(&[], &[]).unwrap();
    let semantic =
        build_inkscript_semantic(&parsed, &schema).map_err(|error| format!("{error:?}"))?;
    let envelope =
        build_inkscript_orchestration_envelope(&semantic).map_err(|error| error.to_string())?;
    let canonical = emit_inkscript_canonical(&semantic, &schema).unwrap();
    let source2 = InkScriptSource::new(InkScriptSourceId::new(405), &canonical).unwrap();
    let parsed2 = parse_inkscript(&source2);
    assert!(parsed2.is_valid(), "{:?}", parsed2.diagnostics());
    let semantic2 = build_inkscript_semantic(&parsed2, &schema).unwrap();
    assert_eq!(semantic, semantic2);
    assert_eq!(
        build_inkscript_orchestration_envelope(&semantic2).unwrap(),
        envelope
    );
    Ok(envelope)
}

#[test]
fn profile_is_explicit_closed_default_canonical_and_batch_ranges_are_unbounded() {
    let output = "policy = new_tabs;";
    assert_eq!(
        envelope("current_document;", output)
            .unwrap()
            .input_profile(),
        InkScriptInputProfile::Canonical
    );
    assert_eq!(
        envelope("profile = canonical; current_sequence;", output)
            .unwrap()
            .input_profile(),
        InkScriptInputProfile::Canonical
    );
    for (first, last) in [(0, 0), (0, 5), (5, 0), (5, 5), (5, 9)] {
        let inputs =
            format!("profile = batch; file \"a.png\" {{ cells = range({first},{last}); }};");
        let typed = envelope(&inputs, output).unwrap();
        assert_eq!(typed.input_profile(), InkScriptInputProfile::Batch);
        assert_eq!(
            typed.inputs()[0].cells(),
            InkScriptCellSelection::Inclusive { first, last }
        );
    }
    assert_eq!(
        envelope(
            "profile = batch; current_document { cells = range(5,0); };",
            output
        )
        .unwrap()
        .inputs()[0]
            .cells(),
        InkScriptCellSelection::All
    );
    for inputs in [
        "profile = batch; current_sequence;",
        "profile = unknown; current_document;",
        "profile = batch; profile = canonical; current_document;",
        "profile = canonical; file \"a.inkpod\" { cells = range(0,5); };",
        "file \"a.inkpod\" { cells = range(5,0); };",
        "profile = batch; file \"a.png\" { cells = range(9,5); };",
        "profile = batch; file \"a.png\" { cells = range(4294967296,0); };",
        "profile = batch; folder \"a\" { recursive = true; };",
        "current_document { cells = range(1,5); };",
    ] {
        assert!(envelope(inputs, output).is_err(), "{inputs}");
    }
}

#[test]
fn folder_formats_templates_and_staged_output_are_closed_and_round_trip() {
    for (format, expected) in [
        ("inkpod", InkScriptOutputFormat::Inkpod),
        ("png", InkScriptOutputFormat::Png),
        ("tiff", InkScriptOutputFormat::Tiff),
        ("tga", InkScriptOutputFormat::Tga),
        ("bmp", InkScriptOutputFormat::Bmp),
    ] {
        let output = format!(
            "policy = folder; format = {format}; folder = \"out\"; naming_template = \"{{stem}}_{{index:4}}\";"
        );
        let typed = envelope("profile = batch; file \"a.png\";", &output).unwrap();
        assert_eq!(typed.output().format(), expected);
        let InkScriptOutput::Folder(folder) = typed.output() else {
            panic!("folder expected")
        };
        assert_eq!(folder.folder(), "out");
        assert_eq!(folder.naming_template(), "{stem}_{index:4}");
        assert_eq!(typed.path_intent_preview().intents().len(), 2);
    }
    assert!(matches!(
        envelope("current_document;", "policy = active_document;")
            .unwrap()
            .output(),
        InkScriptOutput::ActiveDocument
    ));
    assert!(matches!(
        envelope("file \"a.png\";", "policy = new_tabs;")
            .unwrap()
            .output(),
        InkScriptOutput::NewTabs
    ));
    for output in [
        "policy = active_document; format = inkpod;",
        "policy = new_tabs; folder = \"out\";",
        "policy = folder; format = jpeg; folder = \"out\"; naming_template = \"{stem}\";",
        "policy = folder; format = png; folder = \"\"; naming_template = \"{stem}\";",
        "policy = folder; format = png; folder = \"out\"; naming_template = \"{stem}\"; cell_folder = true;",
        "policy = duplicate; format = png; folder = \"out\"; cell_folder = false; basename = \"\"; start_number = 1; direction = ascending;",
    ] {
        assert!(envelope("current_document;", output).is_err(), "{output}");
    }
    for inputs in [
        "file \"a.inkpod\";",
        "folder \"a\";",
        "current_sequence;",
        "current_document; current_document;",
    ] {
        assert!(
            envelope(inputs, "policy = active_document;").is_err(),
            "{inputs}"
        );
    }
}

#[test]
fn template_and_folder_bounds_reject_without_partial_envelope() {
    for template in [
        "",
        ".",
        "..",
        "{stem}.png",
        "../{stem}",
        "{stem}/{index:2}",
        "{index}",
        "{index:0}",
        "{index:13}",
        "{extension}",
        "{stem",
        "stem}",
    ] {
        let output = format!(
            "policy = folder; format = png; folder = \"out\"; naming_template = \"{template}\";"
        );
        assert!(
            envelope("profile = batch; file \"a.png\";", &output).is_err(),
            "{template}"
        );
    }
    for template in ["{index:1}", "{index:01}", "{index:12}", "画像_{stem}"] {
        let output = format!(
            "policy = folder; format = png; folder = \"out\"; naming_template = \"{template}\";"
        );
        assert!(envelope("file \"a.png\";", &output).is_ok(), "{template}");
    }
    let output = format!(
        "policy = folder; format = png; folder = \"out\"; naming_template = \"{}\";",
        "a".repeat(1024)
    );
    assert!(envelope("file \"a.png\";", &output).is_ok());
    let output = output.replace(&"a".repeat(1024), &"a".repeat(1025));
    assert!(envelope("file \"a.png\";", &output).is_err());
    let output = format!(
        "policy = folder; format = png; folder = \"{}\"; naming_template = \"{{stem}}\";",
        "a".repeat(32769)
    );
    assert!(envelope("file \"a.png\";", &output).is_err());
}

#[test]
fn exact_current_file_v3_rejects_v2_and_future_headers() {
    assert_eq!(INKSCRIPT_FILE_VERSION, 3);
    for version in [0, 1, 2, 4] {
        let text = text("current_document;", "policy = new_tabs;").replacen(
            "inkscript 3;",
            &format!("inkscript {version};"),
            1,
        );
        let source = InkScriptSource::new(InkScriptSourceId::new(406), text.as_bytes()).unwrap();
        assert!(!parse_inkscript(&source).is_valid());
        let fragment = format!(
            "inkscript_fragment {version}; requires {{ procedure_catalog = 8; replay_epoch = 29; }} program {{}}"
        );
        let source =
            InkScriptSource::new(InkScriptSourceId::new(407), fragment.as_bytes()).unwrap();
        assert!(!parse_inkscript(&source).is_valid());
    }
}
