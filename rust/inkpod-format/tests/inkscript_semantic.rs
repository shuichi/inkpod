use inkpod_format::{
    InkScriptCommandSchema, InkScriptFieldSchema, InkScriptGeneratedNames, InkScriptRecordSchema,
    InkScriptSchemaDefault, InkScriptSchemaView, InkScriptSemanticErrorCode,
    InkScriptSemanticSection, InkScriptSource, InkScriptSourceId, build_inkscript_semantic,
    emit_inkscript_canonical, parse_inkscript, parse_inkscript_value,
};

const TEST_OPTIONS_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("zeta", "u32", 0),
    InkScriptFieldSchema::optional("alpha", "bool", InkScriptSchemaDefault::Boolean(false), 1),
];
const TEST_RECORDS: &[InkScriptRecordSchema] = &[InkScriptRecordSchema::new(
    "test_options",
    TEST_OPTIONS_FIELDS,
)];
const TEST_COMMAND_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("count", "i64", 0),
    InkScriptFieldSchema::required("ratio", "q16", 1),
    InkScriptFieldSchema::required("text", "string", 2),
    InkScriptFieldSchema::required("options", "test_options", 3),
    InkScriptFieldSchema::required("values", "list<i32>", 4),
    InkScriptFieldSchema::required("payload", "base64", 5),
    InkScriptFieldSchema::optional("note", "nullable<string>", InkScriptSchemaDefault::None, 6),
];
const LITERAL_COMMAND_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("uuid_value", "uuid", 0),
    InkScriptFieldSchema::required("digest_value", "digest", 1),
    InkScriptFieldSchema::required("point_value", "point", 2),
    InkScriptFieldSchema::required("reference_value", "entity_ref", 3),
    InkScriptFieldSchema::required("asset_value", "asset_ref", 4),
];
const TEST_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::new("test_command", TEST_COMMAND_FIELDS),
    InkScriptCommandSchema::new("literal_command", LITERAL_COMMAND_FIELDS),
];

fn source(bytes: &[u8]) -> InkScriptSource {
    InkScriptSource::new(InkScriptSourceId::new(73), bytes).expect("fixture must be UTF-8")
}

fn schema() -> InkScriptSchemaView<'static> {
    InkScriptSchemaView::exact_current(TEST_RECORDS, TEST_COMMANDS)
        .expect("bounded test schema must compose")
}

#[test]
fn semantic_ast_and_canonical_file_round_trip_use_registry_order_and_values() {
    let original = source(
        concat!(
            "\u{feff}inkscript 2;\r\n",
            "// canonical output must not retain this\r\n",
            "execution { preview_before_save = true; wait_ms = -0; failure = continue; }\r\n",
            "output { direction = ascending; start_number = 1; basename = \"cell\"; ",
            "cell_folder = false; folder = \"out\"; format = inkpod; policy = duplicate; }\r\n",
            "program {\r\n",
            "  assert document { width = 1920; source_document_uuid = none; color_space = srgb; };\r\n",
            "  step \"Normalize \\u{26}\" as result {\r\n",
            "    invoke test_command {\r\n",
            "      note = none; payload = base64\"\"\" Q U J D \"\"\";\r\n",
            "      values = [-0, 2,]; options = { alpha = false; zeta = 7; };\r\n",
            "      text = \"line\\n\\u{1}\"; ratio = -0.5000; count = -0;\r\n",
            "    };\r\n",
            "    editor_group = \"pair\"; enabled = true;\r\n",
            "  }\r\n",
            "}\r\n",
            "inputs { file \"a.inkpod\" { cells = all; }; current_document {}; }\r\n",
            "requires { replay_epoch = 25; procedure_catalog = 4; }\r\n",
        )
        .as_bytes(),
    );
    let parsed = parse_inkscript(&original);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());

    let ast = build_inkscript_semantic(&parsed, &schema()).expect("semantic conversion");
    assert!(matches!(
        ast.sections().first(),
        Some(InkScriptSemanticSection::Requires(_))
    ));
    let canonical = emit_inkscript_canonical(&ast, &schema()).expect("canonical emission");
    assert_eq!(
        std::str::from_utf8(&canonical).unwrap(),
        concat!(
            "inkscript 2;\n",
            "\n",
            "requires {\n",
            "    procedure_catalog = 4;\n",
            "    replay_epoch = 25;\n",
            "}\n",
            "\n",
            "inputs {\n",
            "    file \"a.inkpod\";\n",
            "    current_document;\n",
            "}\n",
            "\n",
            "program {\n",
            "    assert document {\n",
            "        width = 1920;\n",
            "        color_space = srgb;\n",
            "    };\n",
            "    step \"Normalize &\" as result {\n",
            "        enabled = true;\n",
            "        editor_group = \"pair\";\n",
            "        invoke test_command {\n",
            "            count = 0;\n",
            "            ratio = -0.5;\n",
            "            text = \"line\\n\\u{1}\";\n",
            "            options = {\n",
            "                zeta = 7;\n",
            "            };\n",
            "            values = [\n",
            "                0,\n",
            "                2,\n",
            "            ];\n",
            "            payload = base64\"\"\"\n",
            "                QUJD\n",
            "            \"\"\";\n",
            "        };\n",
            "    }\n",
            "}\n",
            "\n",
            "output {\n",
            "    policy = duplicate;\n",
            "    format = inkpod;\n",
            "    folder = \"out\";\n",
            "    cell_folder = false;\n",
            "    basename = \"cell\";\n",
            "    start_number = 1;\n",
            "    direction = ascending;\n",
            "}\n",
            "\n",
            "execution {\n",
            "    failure = continue;\n",
            "    wait_ms = 0;\n",
            "    preview_before_save = true;\n",
            "}\n",
        )
    );
    assert!(!canonical.starts_with(&[0xef, 0xbb, 0xbf]));
    assert!(!canonical.windows(2).any(|window| window == b"\r\n"));
    assert!(!canonical.windows(2).any(|window| window == b"//"));

    let canonical_source = source(&canonical);
    let reparsed = parse_inkscript(&canonical_source);
    assert!(reparsed.is_valid(), "{:?}", reparsed.diagnostics());
    let round_trip =
        build_inkscript_semantic(&reparsed, &schema()).expect("canonical semantic conversion");
    assert_eq!(round_trip, ast);

    let mut lossless = Vec::new();
    parsed.cst().write_lossless(&mut lossless).unwrap();
    assert_eq!(lossless, original.bytes());
}

#[test]
fn canonical_fragment_is_deterministic_and_preserves_declaration_order() {
    let input = source(
        br#"inkscript_fragment 2;
program {
  step "second" { enabled = false; invoke test_command { count = 2; ratio = 1.00; text = "b"; options = { zeta = 2; }; values = []; payload = base64""""""; }; }
  step "first" { enabled = true; invoke test_command { count = 1; ratio = 0.0; text = "a"; options = { zeta = 1; }; values = []; payload = base64""""""; }; }
}
requires { replay_epoch = 25; procedure_catalog = 4; }
"#,
    );
    let parsed = parse_inkscript(&input);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    let ast = build_inkscript_semantic(&parsed, &schema()).unwrap();

    let first = emit_inkscript_canonical(&ast, &schema()).unwrap();
    let second = emit_inkscript_canonical(&ast, &schema()).unwrap();
    assert_eq!(first, second);
    let text = std::str::from_utf8(&first).unwrap();
    assert!(text.starts_with("inkscript_fragment 2;\n\nrequires"));
    assert!(text.find("step \"second\"").unwrap() < text.find("step \"first\"").unwrap());

    let canonical_source = source(&first);
    let reparsed = parse_inkscript(&canonical_source);
    let round_trip = build_inkscript_semantic(&reparsed, &schema()).unwrap();
    assert_eq!(round_trip, ast);
}

#[test]
fn compound_literals_references_and_constructors_round_trip_canonically() {
    let input = source(
        br#"inkscript_fragment 2;
requires { replay_epoch = 25; procedure_catalog = 4; }
program {
  step "literals" { enabled = true; invoke literal_command {
    asset_value = asset(blob);
    reference_value = $target.owner[0];
    point_value = point(1.00, -0);
    digest_value = blake3"0000000000000000000000000000000000000000000000000000000000000000";
    uuid_value = uuid"550e8400-e29b-41d4-a716-446655440000";
  }; }
}
"#,
    );
    let parsed = parse_inkscript(&input);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    let ast = build_inkscript_semantic(&parsed, &schema()).unwrap();
    let canonical = emit_inkscript_canonical(&ast, &schema()).unwrap();
    let text = std::str::from_utf8(&canonical).unwrap();
    assert!(text.contains("point_value = point(1.0, 0);"));
    assert!(text.contains("reference_value = $target.owner[0];"));
    assert!(text.contains("asset_value = asset(blob);"));
    assert!(text.find("uuid_value").unwrap() < text.find("digest_value").unwrap());

    let canonical_source = source(&canonical);
    let reparsed = parse_inkscript(&canonical_source);
    assert_eq!(build_inkscript_semantic(&reparsed, &schema()).unwrap(), ast);
}

#[test]
fn invalid_syntax_and_missing_command_schema_never_fallback() {
    let invalid = source(b"inkscript_fragment 2; requires {} // missing program");
    let parsed = parse_inkscript(&invalid);
    let error = build_inkscript_semantic(&parsed, &schema()).unwrap_err();
    assert_eq!(error.code(), InkScriptSemanticErrorCode::InvalidSyntax);

    let valid = source(
        br#"inkscript_fragment 2;
requires { procedure_catalog = 4; replay_epoch = 25; }
program { step "x" { enabled = true; invoke test_command { count = 0; ratio = 0.0; text = ""; options = { zeta = 1; }; values = []; payload = base64""""""; }; } }
"#,
    );
    let parsed = parse_inkscript(&valid);
    let ast = build_inkscript_semantic(&parsed, &schema()).unwrap();
    let language_only = InkScriptSchemaView::exact_current(&[], &[]).unwrap();
    let error = emit_inkscript_canonical(&ast, &language_only).unwrap_err();
    assert_eq!(
        error.code(),
        InkScriptSemanticErrorCode::UnknownCommandSchema
    );
    assert_eq!(error.code().as_str(), "unknown_command_schema");
}

#[test]
fn schema_composition_is_closed_bounded_and_rejects_ambiguous_order() {
    const DUPLICATE_FIELDS: &[InkScriptFieldSchema] = &[
        InkScriptFieldSchema::required("a", "u32", 0),
        InkScriptFieldSchema::required("b", "u32", 0),
    ];
    const DUPLICATE_RECORDS: &[InkScriptRecordSchema] = &[
        InkScriptRecordSchema::new("same", DUPLICATE_FIELDS),
        InkScriptRecordSchema::new("same", &[]),
    ];
    const UNKNOWN_TYPE_FIELDS: &[InkScriptFieldSchema] =
        &[InkScriptFieldSchema::required("field", "not_registered", 0)];

    assert_eq!(
        InkScriptSchemaView::exact_current(
            &[InkScriptRecordSchema::new("bad", DUPLICATE_FIELDS)],
            &[]
        )
        .unwrap_err()
        .code(),
        InkScriptSemanticErrorCode::InvalidSchema
    );
    assert_eq!(
        InkScriptSchemaView::exact_current(DUPLICATE_RECORDS, &[])
            .unwrap_err()
            .code(),
        InkScriptSemanticErrorCode::InvalidSchema
    );
    assert_eq!(
        InkScriptSchemaView::exact_current(&[InkScriptRecordSchema::new("u32", &[])], &[])
            .unwrap_err()
            .code(),
        InkScriptSemanticErrorCode::InvalidSchema
    );
    assert_eq!(
        InkScriptSchemaView::exact_current(
            &[InkScriptRecordSchema::new("bad_type", UNKNOWN_TYPE_FIELDS)],
            &[],
        )
        .unwrap_err()
        .code(),
        InkScriptSemanticErrorCode::InvalidSchema
    );
}

#[test]
fn generated_names_use_stable_occurrence_order_and_minimum_suffixes() {
    let mut names = InkScriptGeneratedNames::new(["layer_1", "layer_3", "target"])
        .expect("existing names are valid");
    assert_eq!(names.next_numbered("layer").unwrap(), "layer_2");
    assert_eq!(names.next_numbered("layer").unwrap(), "layer_4");
    assert_eq!(names.reserve_or_rename("target").unwrap(), "target_2");
    assert_eq!(names.reserve_or_rename("fresh").unwrap(), "fresh");
    assert_eq!(names.reserve_or_rename("fresh").unwrap(), "fresh_2");
    assert_eq!(
        names.reserve_or_rename("Not-An-Identifier").unwrap_err(),
        InkScriptSemanticErrorCode::InvalidGeneratedName
    );
}

#[test]
fn standalone_value_parser_reuses_the_bounded_exact_value_grammar() {
    let source = InkScriptSource::new(
        InkScriptSourceId::new(91),
        br#"{ color = rgba16(1, 2, 3, 65535); points = [point(q16(-1), q16(2))]; }"#,
    )
    .unwrap();
    let value = parse_inkscript_value(&source).expect("closed override value");
    assert!(matches!(value, inkpod_format::InkScriptValue::Record(_)));

    for invalid in [b"".as_slice(), b"[1,,2]", b"true trailing", b"{"] {
        let source = InkScriptSource::new(InkScriptSourceId::new(92), invalid).unwrap();
        assert!(parse_inkscript_value(&source).is_err());
    }
}
