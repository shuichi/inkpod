use inkpod_format::{
    InkScriptAnalysisLimits, InkScriptCommandResultSchema, InkScriptCommandSchema,
    InkScriptDependencyNodeKind, InkScriptExternalResultBinding, InkScriptFieldSchema,
    InkScriptFragmentRequest, InkScriptFragmentSelection, InkScriptReferenceSegment,
    InkScriptResultAvailability, InkScriptSchemaView, InkScriptSemanticErrorCode, InkScriptSource,
    InkScriptSourceId, InkScriptTypeDiagnosticCode, InkScriptTypedValueKind, InkScriptValue,
    build_inkscript_declaration_model, build_inkscript_declaration_model_with_limits,
    close_inkscript_fragment, parse_inkscript,
};

const CREATE_RESULTS: &[InkScriptCommandResultSchema] = &[
    InkScriptCommandResultSchema::scalar(
        "layer",
        "layer_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    ),
    InkScriptCommandResultSchema::ordered_list(
        "planes",
        "plane_ref",
        InkScriptResultAvailability::OnlyOnChange,
        1,
    ),
];
const USE_LAYER_FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("layer", "layer_ref", 0),
    InkScriptFieldSchema::required("value", "u32", 1),
    InkScriptFieldSchema::required("payload", "asset_ref", 2),
];
const USE_PLANE_FIELDS: &[InkScriptFieldSchema] =
    &[InkScriptFieldSchema::required("plane", "plane_ref", 0)];
const USE_PLANE_LIST_FIELDS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "planes",
    "list<plane_ref>",
    0,
)];
const TEST_COMMANDS: &[InkScriptCommandSchema] = &[
    InkScriptCommandSchema::with_results("create_test", &[], CREATE_RESULTS),
    InkScriptCommandSchema::new("use_layer_test", USE_LAYER_FIELDS),
    InkScriptCommandSchema::new("use_plane_test", USE_PLANE_FIELDS),
    InkScriptCommandSchema::new("use_plane_list_test", USE_PLANE_LIST_FIELDS),
];

fn source(bytes: &[u8]) -> InkScriptSource {
    InkScriptSource::new(InkScriptSourceId::new(106), bytes).expect("fixture must be UTF-8")
}

fn schema() -> InkScriptSchemaView<'static> {
    InkScriptSchemaView::exact_current(&[], TEST_COMMANDS).expect("test schema must compose")
}

fn lossless(parsed: &inkpod_format::InkScriptParsed<'_>) -> Vec<u8> {
    let mut bytes = Vec::new();
    parsed.cst().write_lossless(&mut bytes).unwrap();
    bytes
}

fn analyze(
    bytes: &[u8],
) -> Result<inkpod_format::InkScriptDeclarationModel, inkpod_format::InkScriptTypeDiagnostic> {
    let source = source(bytes);
    let parsed = parse_inkscript(&source);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    build_inkscript_declaration_model(&parsed, &schema())
}

fn asset(name: &str) -> String {
    format!(
        "asset {name} {{ asset_id = blake3\"0000000000000000000000000000000000000000000000000000000000000000\"; kind = \"canonical_raster\"; descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 4; element_count = 1; }}; data = base64\"\"\"AAAAAA==\"\"\"; }};"
    )
}

#[test]
fn typed_steps_results_groups_and_all_reference_edges_are_owned_and_ordered() {
    let empty = analyze(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } program {}",
    )
    .unwrap();
    assert!(empty.steps().is_empty());
    assert!(empty.groups().is_empty());
    assert!(empty.dependency_edges().is_empty());

    let text = format!(
        r#"inkscript_fragment 2;
requires {{ procedure_catalog = 3; replay_epoch = 24; }}
parameters {{ param threshold: u32 = 7; }}
bindings {{ let target = select layer {{}}; let target_plane = select plane {{ layer = $target; }}; }}
program {{
    step "Create" as made {{ enabled = true; editor_group = "pair"; invoke create_test {{}}; }}
    step "Use layer" {{ enabled = true; editor_group = "pair"; invoke use_layer_test {{ layer = $made.layer; value = $threshold; payload = asset(image); }}; }}
    step "Use plane" {{ enabled = true; invoke use_plane_test {{ plane = $made.planes[0]; }}; }}
}}
assets {{ {} }}
"#,
        asset("image")
    );
    let model = analyze(text.as_bytes()).unwrap();

    assert_eq!(model.steps().len(), 3);
    assert_eq!(model.steps()[0].command(), "create_test");
    assert_eq!(model.steps()[0].result_alias(), Some("made"));
    assert_eq!(model.steps()[0].results().len(), 2);
    assert_eq!(model.steps()[0].results()[0].name(), "layer");
    assert_eq!(
        model.steps()[0].results()[0].value_type().name(),
        "layer_ref"
    );
    assert_eq!(
        model.steps()[0].results()[1].availability(),
        InkScriptResultAvailability::OnlyOnChange
    );
    assert_eq!(
        model.steps()[1].arguments().type_name(),
        "use_layer_test_invocation"
    );
    assert_eq!(model.groups().len(), 1);
    assert_eq!(model.groups()[0].key(), "pair");
    assert_eq!(model.groups()[0].first_step(), 0);
    assert_eq!(model.groups()[0].step_count(), 2);

    let dependency_kinds = model
        .dependency_edges()
        .iter()
        .map(|edge| edge.dependency().kind())
        .collect::<Vec<_>>();
    assert!(dependency_kinds.contains(&InkScriptDependencyNodeKind::Parameter));
    assert!(dependency_kinds.contains(&InkScriptDependencyNodeKind::Binding));
    assert!(dependency_kinds.contains(&InkScriptDependencyNodeKind::StepResult));
    assert!(dependency_kinds.contains(&InkScriptDependencyNodeKind::Asset));
    assert!(matches!(
        model.steps()[2].arguments().kind(),
        InkScriptTypedValueKind::Record(fields)
            if fields["plane"].type_name() == "plane_ref"
    ));

    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<inkpod_format::InkScriptDeclarationModel>();
}

#[test]
fn result_field_index_availability_and_cardinality_fail_with_source_ranges() {
    let cases = [
        (
            "step \"Create\" as made { enabled = true; invoke create_test {}; } step \"Use\" { enabled = true; invoke use_plane_test { plane = $made.missing; }; }",
            InkScriptTypeDiagnosticCode::UnknownResultField,
        ),
        (
            "step \"Create\" as made { enabled = true; invoke create_test {}; } step \"Use\" { enabled = true; invoke use_plane_test { plane = $made.layer[0]; }; }",
            InkScriptTypeDiagnosticCode::InvalidResultIndex,
        ),
        (
            "step \"Create\" as made { enabled = true; invoke create_test {}; } step \"Use\" { enabled = true; invoke use_plane_test { plane = $made.planes[18446744073709551616]; }; }",
            InkScriptTypeDiagnosticCode::NumericOverflow,
        ),
        (
            "step \"Create\" as made { enabled = true; invoke create_test {}; } step \"Use\" { enabled = true; invoke use_plane_test { plane = $made.planes; }; }",
            InkScriptTypeDiagnosticCode::ResultCardinalityMismatch,
        ),
        (
            "step \"Create\" as made { enabled = false; invoke create_test {}; } step \"Use\" { enabled = true; invoke use_plane_test { plane = $made.planes[0]; }; }",
            InkScriptTypeDiagnosticCode::UnavailableResult,
        ),
        (
            "step \"Use\" { enabled = true; invoke use_plane_test { plane = $made.planes[0]; }; } step \"Create\" as made { enabled = true; invoke create_test {}; }",
            InkScriptTypeDiagnosticCode::ForwardReference,
        ),
    ];

    for (program, expected) in cases {
        let text = format!(
            "inkscript_fragment 2;\nrequires {{ procedure_catalog = 3; replay_epoch = 24; }}\nprogram {{ {program} }}\n"
        );
        let error = analyze(text.as_bytes()).unwrap_err();
        assert_eq!(error.code(), expected, "{program}");
        assert_eq!(error.source_id(), InkScriptSourceId::new(106));
        assert_eq!(error.range().start().line(), 3);
        assert!(error.range().span().end() > error.range().span().start());
        assert!(!error.path().is_empty());
    }

    let valid_list = analyze(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } program { step \"Create\" as made { enabled = true; invoke create_test {}; } step \"Use\" { enabled = true; invoke use_plane_list_test { planes = $made.planes; }; } }",
    )
    .unwrap();
    assert_eq!(valid_list.steps().len(), 2);
}

#[test]
fn result_schema_and_unknown_commands_fail_closed_without_debug_name_fallback() {
    const UNKNOWN_RESULT: &[InkScriptCommandResultSchema] =
        &[InkScriptCommandResultSchema::scalar(
            "value",
            "rust_debug_variant",
            InkScriptResultAvailability::AlwaysOnSuccess,
            0,
        )];
    const DUPLICATE_RESULTS: &[InkScriptCommandResultSchema] = &[
        InkScriptCommandResultSchema::scalar(
            "value",
            "u32",
            InkScriptResultAvailability::AlwaysOnSuccess,
            0,
        ),
        InkScriptCommandResultSchema::scalar(
            "value",
            "u32",
            InkScriptResultAvailability::AlwaysOnSuccess,
            1,
        ),
    ];
    for commands in [
        &[InkScriptCommandSchema::with_results(
            "bad_test",
            &[],
            UNKNOWN_RESULT,
        )][..],
        &[InkScriptCommandSchema::with_results(
            "bad_test",
            &[],
            DUPLICATE_RESULTS,
        )][..],
    ] {
        assert_eq!(
            InkScriptSchemaView::exact_current(&[], commands)
                .unwrap_err()
                .code(),
            InkScriptSemanticErrorCode::InvalidSchema
        );
    }

    let input = source(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } program { step \"Unknown\" { enabled = true; invoke rust_debug_variant {}; } }",
    );
    let parsed = parse_inkscript(&input);
    let error = build_inkscript_declaration_model(&parsed, &schema()).unwrap_err();
    assert_eq!(
        error.code(),
        InkScriptTypeDiagnosticCode::InvalidSemanticModel
    );
}

#[test]
fn dependency_resource_limit_rejects_without_partial_model() {
    let input = source(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } parameters { param value: u32 = 1; } bindings { let target = select layer {}; } program { step \"Create\" as made { enabled = true; invoke create_test {}; } step \"Use\" { enabled = true; invoke use_layer_test { layer = $made.layer; value = $value; payload = asset(image); }; } } assets { asset image { asset_id = blake3\"0000000000000000000000000000000000000000000000000000000000000000\"; kind = \"canonical_raster\"; descriptor = { pixel_format = rgba8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 4; element_count = 1; }; data = base64\"\"\"AAAAAA==\"\"\"; }; }",
    );
    let parsed = parse_inkscript(&input);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    let unchanged = lossless(&parsed);
    let error = build_inkscript_declaration_model_with_limits(
        &parsed,
        &schema(),
        InkScriptAnalysisLimits::exact_current().with_dependency_edges(2),
    )
    .unwrap_err();
    assert_eq!(error.code(), InkScriptTypeDiagnosticCode::ResourceLimit);
    assert_eq!(lossless(&parsed), unchanged);
}

#[test]
fn selectors_and_asserts_fail_closed_and_share_the_dependency_graph() {
    let strict_without_uuid = analyze(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } bindings { let target = select layer { persistent_id = 7; }; } program {}",
    )
    .unwrap_err();
    assert_eq!(
        strict_without_uuid.code(),
        InkScriptTypeDiagnosticCode::InvalidStrictPrecondition
    );

    let forbidden_all = analyze(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } bindings { let frame = select shooting_frame { cardinality = all; }; } program {}",
    )
    .unwrap_err();
    assert_eq!(
        forbidden_all.code(),
        InkScriptTypeDiagnosticCode::ValueOutOfRange
    );

    let zero_width = analyze(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } program { assert document { width = 0; }; }",
    )
    .unwrap_err();
    assert_eq!(
        zero_width.code(),
        InkScriptTypeDiagnosticCode::ValueOutOfRange
    );

    let list_target = analyze(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } bindings { let targets = select layer { cardinality = all; }; } program { assert object { target = $targets; }; }",
    )
    .unwrap_err();
    assert_eq!(
        list_target.code(),
        InkScriptTypeDiagnosticCode::ResultCardinalityMismatch
    );

    let model = analyze(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } bindings { let target = select layer {}; } program { assert object { target = $target; visible = true; }; step \"Use\" { enabled = true; invoke use_layer_test { layer = $target; value = 1; payload = asset(image); }; } } assets { asset image { asset_id = blake3\"0000000000000000000000000000000000000000000000000000000000000000\"; kind = \"canonical_raster\"; descriptor = { pixel_format = rgba8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 4; element_count = 1; }; data = base64\"\"\"AAAAAA==\"\"\"; }; }",
    )
    .unwrap();
    assert!(model.dependency_edges().iter().any(|edge| {
        edge.consumer().kind() == InkScriptDependencyNodeKind::Assert
            && edge.dependency().kind() == InkScriptDependencyNodeKind::Binding
    }));
}

#[test]
fn fragment_closure_rebinds_external_results_without_adding_mutations_and_renames_atomically() {
    let text = format!(
        r#"inkscript_fragment 2;
requires {{ procedure_catalog = 3; replay_epoch = 24; }}
parameters {{ param threshold: u32 = 7; param unused: u32 = 9; }}
program {{
    step "Outside" as outside {{ enabled = true; invoke create_test {{}}; }}
    step "Use outside" {{ enabled = true; editor_group = "selected"; invoke use_layer_test {{ layer = $outside.layer; value = $threshold; payload = asset(image); }}; }}
    step "Inside producer" as inside {{ enabled = true; editor_group = "selected"; invoke create_test {{}}; }}
    step "Inside consumer" {{ enabled = true; editor_group = "selected"; invoke use_plane_test {{ plane = $inside.planes[0]; }}; }}
    step "Use duplicate asset" {{ enabled = true; editor_group = "selected"; invoke use_layer_test {{ layer = $outside.layer; value = $threshold; payload = asset(image_copy); }}; }}
}}
assets {{ {} {} }}
"#,
        asset("image"),
        asset("image_copy")
    );
    let input = source(text.as_bytes());
    let parsed = parse_inkscript(&input);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    let unchanged = lossless(&parsed);

    let rejected = close_inkscript_fragment(
        &parsed,
        &schema(),
        &InkScriptFragmentRequest::new(InkScriptFragmentSelection::EditorGroup(
            "selected".to_owned(),
        )),
    )
    .unwrap_err();
    assert_eq!(
        rejected.code(),
        InkScriptTypeDiagnosticCode::ExternalMutationDependency
    );
    assert_eq!(lossless(&parsed), unchanged);

    let strict = InkScriptExternalResultBinding::new(
        "outside",
        vec![InkScriptReferenceSegment::Field("layer".to_owned())],
        "outside_layer",
        "layer",
        vec![
            (
                "source_document_uuid".to_owned(),
                InkScriptValue::Uuid("00112233-4455-6677-8899-aabbccddeeff".to_owned()),
            ),
            (
                "persistent_id".to_owned(),
                InkScriptValue::Integer("7".to_owned()),
            ),
        ],
    );
    let request = InkScriptFragmentRequest::new(InkScriptFragmentSelection::EditorGroup(
        "selected".to_owned(),
    ))
    .with_external_result_bindings(vec![strict])
    .with_reserved_value_names(vec![
        "threshold".to_owned(),
        "outside_layer".to_owned(),
        "inside".to_owned(),
    ])
    .with_reserved_asset_names(vec!["image".to_owned()])
    .with_reserved_group_keys(vec!["selected".to_owned()]);

    let first = close_inkscript_fragment(&parsed, &schema(), &request).unwrap();
    let second = close_inkscript_fragment(&parsed, &schema(), &request).unwrap();
    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    let canonical = std::str::from_utf8(first.canonical_bytes()).unwrap();
    assert!(!canonical.contains("step \"Outside\""));
    assert!(!canonical.contains("param unused"));
    assert!(canonical.contains("param threshold_2"));
    assert!(canonical.contains("let outside_layer_2 = select layer"));
    assert!(canonical.contains("layer = $outside_layer_2;"));
    assert!(canonical.contains("as inside_2"));
    assert!(canonical.contains("plane = $inside_2.planes[0];"));
    assert!(canonical.contains("editor_group = \"selected_2\";"));
    assert!(canonical.contains("asset(image_2)"));
    assert!(canonical.contains("asset image_2"));
    assert!(!canonical.contains("image_copy"));

    let output_source = source(first.canonical_bytes());
    let output_parsed = parse_inkscript(&output_source);
    assert!(
        output_parsed.is_valid(),
        "{:?}",
        output_parsed.diagnostics()
    );
    let output_model = build_inkscript_declaration_model(&output_parsed, &schema()).unwrap();
    assert_eq!(output_model.steps().len(), 4);
    assert_eq!(output_model.parameters().len(), 1);
    assert_eq!(output_model.bindings().len(), 1);
    assert_eq!(output_model.assets().len(), 1);
    assert_eq!(lossless(&parsed), unchanged);
}
