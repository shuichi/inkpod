use inkpod_format::{
    InkScriptCst, InkScriptCstNode, InkScriptCstNodeKind, InkScriptDiagnosticCode,
    InkScriptDocumentKind, InkScriptLexerLimits, InkScriptParsed, InkScriptParserLimits,
    InkScriptSource, InkScriptSourceId, MAX_INKSCRIPT_CST_NODES, MAX_INKSCRIPT_NESTING_DEPTH,
    parse_inkscript, parse_inkscript_with_limits,
};

fn source(bytes: &[u8]) -> InkScriptSource {
    InkScriptSource::new(InkScriptSourceId::new(41), bytes).expect("fixture must be valid UTF-8")
}

fn diagnostic_codes(bytes: &[u8]) -> Vec<InkScriptDiagnosticCode> {
    let source = source(bytes);
    parse_inkscript(&source)
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect()
}

fn contains_kind(node: &InkScriptCstNode, kind: InkScriptCstNodeKind) -> bool {
    node.kind() == kind
        || node
            .children()
            .iter()
            .any(|child| contains_kind(child, kind))
}

#[test]
fn public_parser_accepts_a_complete_file_and_preserves_every_source_byte() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<InkScriptCst<'static>>();
    assert_send_sync::<InkScriptParsed<'static>>();

    let bytes = concat!(
        "\u{feff}inkscript 1;\r\n",
        "// retained comment\r\n",
        "meta { title = \"\\u{69}nkpod\"; }\r\n",
        "requires { procedure_catalog = 1; replay_epoch = 23; }\r\n",
        "inputs {\r\n",
        "  file \"a.inkpod\" {};\r\n",
        "  folder \"cells\" { recursive = false; };\r\n",
        "  current_document {};\r\n",
        "  current_sequence {};\r\n",
        "}\r\n",
        "parameters {\r\n",
        "  param levels: list<nullable<u32>> = [1, none,] { ask = false; };\r\n",
        "}\r\n",
        "bindings { let target = select layer { name = \"Color\"; }; }\r\n",
        "program {\r\n",
        "  assert document { expected = true; };\r\n",
        "  step \"First\" as result {\r\n",
        "    editor_group = \"pair-a\";\r\n",
        "    enabled = true;\r\n",
        "    invoke test_command {\r\n",
        "      target = $target.output[0];\r\n",
        "      payload = asset(blob);\r\n",
        "      point = point(1.0, -0,);\r\n",
        "      options = { enabled = true; };\r\n",
        "    };\r\n",
        "  }\r\n",
        "}\r\n",
        "output { policy = duplicate; format = inkpod; }\r\n",
        "execution { dry_run = false; }\r\n",
        "assets { asset blob { data = base64\"\"\"QUJD\"\"\"; }; }\r\n",
    )
    .as_bytes();
    let source = source(bytes);
    let parsed = parse_inkscript(&source);

    assert!(parsed.is_complete());
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    assert!(parsed.diagnostics().is_empty());
    assert_eq!(parsed.cst().document_kind(), InkScriptDocumentKind::File);
    assert_eq!(parsed.cst().root().kind(), InkScriptCstNodeKind::File);
    assert!(contains_kind(
        parsed.cst().root(),
        InkScriptCstNodeKind::StepStatement
    ));
    assert!(contains_kind(
        parsed.cst().root(),
        InkScriptCstNodeKind::EditorGroupMember
    ));
    assert!(
        parsed
            .cst()
            .tokens()
            .iter()
            .any(|token| token.kind().is_trivia())
    );

    let mut written = Vec::new();
    parsed
        .cst()
        .write_lossless(&mut written)
        .expect("Vec writes cannot fail");
    assert_eq!(written, bytes);
}

#[test]
fn public_parser_accepts_the_minimal_fragment_and_all_value_forms() {
    let source = source(
        br#"inkscript_fragment 1;
requires { procedure_catalog = 1; }
parameters { param p: nullable<list<u32>> = none; }
bindings { let item = select plane {}; }
program {
  assert ready {};
  step "values" {
    enabled = false;
    invoke command {
      bool_value = true;
      integer_value = -1;
      decimal_value = -0.25;
      string_value = "x";
      uuid_value = uuid"550e8400-e29b-41d4-a716-446655440000";
      digest_value = blake3"0000000000000000000000000000000000000000000000000000000000000000";
      enum_value = mode;
      constructor_value = point(1, 2);
      asset_value = asset(a);
      list_value = [];
      record_value = {};
      reference_value = $item.owner[0];
    };
  }
}
assets { asset a {}; }
"#,
    );
    let parsed = parse_inkscript(&source);

    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    assert_eq!(
        parsed.cst().document_kind(),
        InkScriptDocumentKind::Fragment
    );
    assert_eq!(parsed.cst().root().kind(), InkScriptCstNodeKind::Fragment);
    assert!(contains_kind(
        parsed.cst().root(),
        InkScriptCstNodeKind::List
    ));
    assert!(contains_kind(
        parsed.cst().root(),
        InkScriptCstNodeKind::Constructor
    ));
    assert!(contains_kind(
        parsed.cst().root(),
        InkScriptCstNodeKind::AssetReference
    ));
}

#[test]
fn public_parser_rejects_noncurrent_versions_without_a_compatibility_reader() {
    for bytes in [
        &b"inkscript 0; requires {} inputs {} program {} output {} execution {}"[..],
        &b"inkscript 2; requires {} inputs {} program {} output {} execution {}"[..],
        &b"inkscript_fragment 0; requires {} program {}"[..],
    ] {
        assert_eq!(
            diagnostic_codes(bytes),
            vec![InkScriptDiagnosticCode::UnsupportedVersion]
        );
    }
}

#[test]
fn public_parser_rejects_duplicate_sections_fields_and_step_members() {
    let source = source(
        br#"inkscript 1;
requires { version = 1; version = 1; }
requires {}
inputs {}
program {
  step "duplicate" {
    enabled = true;
    enabled = false;
    invoke command {};
    invoke command {};
    editor_group = "g";
    editor_group = "g";
  }
}
output {}
execution {}
"#,
    );
    let parsed = parse_inkscript(&source);
    let codes = parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect::<Vec<_>>();

    assert!(!parsed.is_valid());
    assert!(codes.contains(&InkScriptDiagnosticCode::DuplicateField));
    assert!(codes.contains(&InkScriptDiagnosticCode::DuplicateSection));
    assert_eq!(
        codes
            .iter()
            .filter(|code| **code == InkScriptDiagnosticCode::DuplicateMember)
            .count(),
        3
    );
}

#[test]
fn public_parser_rejects_missing_sections_and_step_members() {
    let file_codes = diagnostic_codes(
        br#"inkscript 1;
requires {}
program {
  step "missing invoke" { enabled = true; }
  step "missing enabled" { invoke command {}; }
}
"#,
    );
    assert_eq!(
        file_codes
            .iter()
            .filter(|code| **code == InkScriptDiagnosticCode::MissingMember)
            .count(),
        2
    );
    assert_eq!(
        file_codes
            .iter()
            .filter(|code| **code == InkScriptDiagnosticCode::MissingSection)
            .count(),
        3
    );

    assert_eq!(
        diagnostic_codes(b"inkscript_fragment 1; requires {}"),
        vec![InkScriptDiagnosticCode::MissingSection]
    );
}

#[test]
fn fragment_rejects_file_only_sections_and_reserved_identifiers() {
    let codes = diagnostic_codes(
        br#"inkscript_fragment 1;
requires {}
inputs {}
program {
  assert true {};
  step "bad" as false { enabled = true; invoke asset {}; }
}
"#,
    );

    assert!(codes.contains(&InkScriptDiagnosticCode::SectionNotAllowed));
    assert_eq!(
        codes
            .iter()
            .filter(|code| **code == InkScriptDiagnosticCode::ReservedIdentifier)
            .count(),
        3
    );
}

#[test]
fn parser_recovers_with_error_nodes_and_never_rewrites_invalid_source() {
    let bytes = b"inkscript 1;\r\nrequires { broken true; ok = 1; }\r\n@\r\nprogram {}\r\n";
    let source = source(bytes);
    let parsed = parse_inkscript(&source);

    assert!(parsed.is_complete());
    assert!(!parsed.is_valid());
    assert!(contains_kind(
        parsed.cst().root(),
        InkScriptCstNodeKind::Error
    ));
    assert!(parsed.diagnostics().len() >= 2);
    let mut written = Vec::new();
    parsed.cst().write_lossless(&mut written).unwrap();
    assert_eq!(written, bytes);
}

#[test]
fn parser_truncation_corpus_is_deterministic_and_always_lossless() {
    let valid = b"inkscript_fragment 1; requires { x = [1, 2]; } program { assert ready {}; }";
    for length in 0..=valid.len() {
        let bytes = &valid[..length];
        let source = source(bytes);
        let first = parse_inkscript(&source);
        let second = parse_inkscript(&source);
        assert_eq!(first.diagnostics(), second.diagnostics());
        let mut written = Vec::new();
        first.cst().write_lossless(&mut written).unwrap();
        assert_eq!(written, bytes);
    }
}

#[test]
fn parser_diagnostic_recovery_stops_at_the_caller_lowered_limit() {
    let lexer_limits = InkScriptLexerLimits::exact_current().with_diagnostic_limit(3);
    let limits = InkScriptParserLimits::exact_current().with_lexer_limits(lexer_limits);
    let source =
        source(b"inkscript_fragment 1; requires {} requires {} requires {} program {} program {}");
    let parsed = parse_inkscript_with_limits(&source, limits);

    assert!(!parsed.is_complete());
    assert_eq!(parsed.diagnostics().len(), 3);
    assert_eq!(
        parsed.diagnostics()[2].code(),
        InkScriptDiagnosticCode::ParserDiagnosticLimitExceeded
    );
}

#[test]
fn parser_enforces_node_nesting_list_reference_and_container_limits() {
    assert_eq!(MAX_INKSCRIPT_CST_NODES, 2_097_152);
    assert_eq!(MAX_INKSCRIPT_NESTING_DEPTH, 64);

    let cases = [
        (
            InkScriptParserLimits::exact_current().with_node_limit(4),
            b"inkscript_fragment 1; requires {} program {}".as_slice(),
            InkScriptDiagnosticCode::NodeLimitExceeded,
        ),
        (
            InkScriptParserLimits::exact_current().with_nesting_depth_limit(2),
            b"inkscript_fragment 1; requires { x = [[[1]]]; } program {}".as_slice(),
            InkScriptDiagnosticCode::NestingLimitExceeded,
        ),
        (
            InkScriptParserLimits::exact_current().with_total_list_element_limit(2),
            b"inkscript_fragment 1; requires { x = [1, 2, 3]; } program {}".as_slice(),
            InkScriptDiagnosticCode::ListElementLimitExceeded,
        ),
        (
            InkScriptParserLimits::exact_current().with_reference_segment_limit(2),
            b"inkscript_fragment 1; requires { x = $a.b[0].c; } program {}".as_slice(),
            InkScriptDiagnosticCode::ReferenceSegmentLimitExceeded,
        ),
        (
            InkScriptParserLimits::exact_current().with_container_element_limit(2),
            b"inkscript_fragment 1; requires { a = 1; b = 2; c = 3; } program {}".as_slice(),
            InkScriptDiagnosticCode::ContainerElementLimitExceeded,
        ),
    ];

    for (limits, bytes, expected) in cases {
        let source = source(bytes);
        let parsed = parse_inkscript_with_limits(&source, limits);
        assert!(!parsed.is_complete());
        assert_eq!(parsed.diagnostics().last().unwrap().code(), expected);
    }
}

#[test]
fn parser_enforces_section_and_declaration_limits_without_truncation() {
    let cases = [
        (
            InkScriptParserLimits::exact_current().with_section_limit(1),
            b"inkscript_fragment 1; requires {} program {}".as_slice(),
            InkScriptDiagnosticCode::SectionLimitExceeded,
        ),
        (
            InkScriptParserLimits::exact_current().with_input_limit(1),
            b"inkscript 1; requires {} inputs { current_document; current_sequence; } program {} output {} execution {}".as_slice(),
            InkScriptDiagnosticCode::InputLimitExceeded,
        ),
        (
            InkScriptParserLimits::exact_current().with_parameter_limit(1),
            b"inkscript_fragment 1; requires {} parameters { param a: u32 = 1; param b: u32 = 2; } program {}".as_slice(),
            InkScriptDiagnosticCode::ParameterLimitExceeded,
        ),
        (
            InkScriptParserLimits::exact_current().with_binding_limit(1),
            b"inkscript_fragment 1; requires {} bindings { let a = select layer {}; let b = select layer {}; } program {}".as_slice(),
            InkScriptDiagnosticCode::BindingLimitExceeded,
        ),
        (
            InkScriptParserLimits::exact_current().with_program_statement_limit(1),
            b"inkscript_fragment 1; requires {} program { assert a {}; assert b {}; }".as_slice(),
            InkScriptDiagnosticCode::ProgramStatementLimitExceeded,
        ),
    ];

    for (limits, bytes, expected) in cases {
        let source = source(bytes);
        let parsed = parse_inkscript_with_limits(&source, limits);
        assert!(!parsed.is_complete());
        assert_eq!(parsed.diagnostics().last().unwrap().code(), expected);
    }
}

#[test]
fn editor_groups_are_nonempty_and_must_form_one_contiguous_run() {
    assert_eq!(
        diagnostic_codes(
            br#"inkscript_fragment 1; requires {} program {
step "a" { enabled = true; editor_group = ""; invoke c {}; }
}"#,
        ),
        vec![InkScriptDiagnosticCode::InvalidEditorGroup]
    );

    let codes = diagnostic_codes(
        br#"inkscript_fragment 1; requires {} program {
step "a" { enabled = true; editor_group = "g"; invoke c {}; }
step "b" { enabled = true; invoke c {}; }
step "c" { enabled = true; editor_group = "\u{67}"; invoke c {}; }
}"#,
    );
    assert_eq!(
        codes,
        vec![InkScriptDiagnosticCode::NoncontiguousEditorGroup]
    );
}

#[test]
fn empty_source_is_an_invalid_but_lossless_no_op() {
    let source = source(b"");
    let parsed = parse_inkscript(&source);
    assert!(parsed.is_complete());
    assert!(!parsed.is_valid());
    assert_eq!(
        parsed.diagnostics()[0].code(),
        InkScriptDiagnosticCode::UnexpectedToken
    );
    let mut written = Vec::new();
    parsed.cst().write_lossless(&mut written).unwrap();
    assert!(written.is_empty());
}
