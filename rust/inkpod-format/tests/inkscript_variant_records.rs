use inkpod_format::{
    InkScriptCommandSchema, InkScriptEnumSchema, InkScriptFieldSchema, InkScriptRecordSchema,
    InkScriptSchemaView, InkScriptSource, InkScriptSourceId, build_inkscript_declaration_model,
    parse_inkscript,
};

const FIELDS: &[InkScriptFieldSchema] = &[
    InkScriptFieldSchema::required("kind", "variant_kind", 0),
    InkScriptFieldSchema::conditional(
        "values",
        "list<u32>",
        1,
        &["present-for:kind=many", "length:1..2"],
    ),
    InkScriptFieldSchema::conditional("value", "u32", 2, &["present-for:kind=one"]),
];
const ARGUMENTS: &[InkScriptFieldSchema] = &[InkScriptFieldSchema::required(
    "argument",
    "variant_record",
    0,
)];

#[test]
fn catalog_variant_fields_are_required_only_for_the_selected_kind() {
    let enums = [InkScriptEnumSchema::new("variant_kind", &["many", "one"])];
    let records = [InkScriptRecordSchema::new("variant_record", FIELDS)];
    let commands = [InkScriptCommandSchema::new("variant_test", ARGUMENTS)];
    let schema =
        InkScriptSchemaView::exact_current_with_catalog(&enums, &[], &records, &commands).unwrap();
    for (record, valid) in [
        ("kind = many; values = [1,2];", true),
        ("kind = one; value = 1;", true),
        ("kind = many;", false),
        ("kind = many; values = [1]; value = 2;", false),
        ("kind = one; values = [1];", false),
        ("kind = many; values = [];", false),
        ("kind = many; values = [1,2,3];", false),
    ] {
        let text = format!(
            "inkscript_fragment 3; requires {{ procedure_catalog = {}; replay_epoch = {}; }} program {{ step \"variant\" {{ enabled = true; invoke variant_test {{ argument = {{ {record} }}; }}; }} }}",
            inkpod_format::INKSCRIPT_PROCEDURE_CATALOG_VERSION,
            inkpod_format::INKSCRIPT_REQUIRED_REPLAY_EPOCH
        );
        let source = InkScriptSource::new(InkScriptSourceId::new(701), text.as_bytes()).unwrap();
        let parsed = parse_inkscript(&source);
        assert_eq!(
            build_inkscript_declaration_model(&parsed, &schema).is_ok(),
            valid,
            "{record}"
        );
    }
}

#[test]
fn disabled_nested_batch_references_remain_in_fragment_dependency_closure() {
    use inkpod_format::{
        InkScriptCommandResultSchema, InkScriptFragmentRequest, InkScriptFragmentSelection,
        InkScriptResultAvailability, InkScriptTypeDiagnosticCode, close_inkscript_fragment,
    };
    // A bounded public catalog view isolates the language-owned closure contract. Core tests
    // separately compile the production Batch catalog and check enabled-only runtime skipping.
    const ENUMS: &[InkScriptEnumSchema] = &[
        InkScriptEnumSchema::new("batch_operation_kind", &["erase"]),
        InkScriptEnumSchema::new("batch_target_kind", &["references"]),
    ];
    const RECORDS: &[InkScriptRecordSchema] = &[
        InkScriptRecordSchema::new(
            "batch_target",
            &[
                InkScriptFieldSchema::required("kind", "batch_target_kind", 0),
                InkScriptFieldSchema::required("layer", "nullable<layer_ref>", 1),
                InkScriptFieldSchema::required("plane", "plane_ref", 2),
            ],
        ),
        InkScriptRecordSchema::new(
            "batch_operation",
            &[
                InkScriptFieldSchema::required("kind", "batch_operation_kind", 0),
                InkScriptFieldSchema::required("enabled", "bool", 1),
                InkScriptFieldSchema::required("target", "batch_target", 2),
                InkScriptFieldSchema::required("colors", "list<pixel_value>", 3),
            ],
        ),
    ];
    const COMMANDS: &[InkScriptCommandSchema] = &[
        InkScriptCommandSchema::with_results(
            "duplicate_plane",
            &[InkScriptFieldSchema::required("plane_id", "plane_ref", 0)],
            &[InkScriptCommandResultSchema::scalar(
                "plane",
                "plane_ref",
                InkScriptResultAvailability::AlwaysOnSuccess,
                0,
            )],
        ),
        InkScriptCommandSchema::new(
            "apply_batch_operations",
            &[InkScriptFieldSchema::required(
                "operations",
                "list<batch_operation>",
                0,
            )],
        ),
    ];
    let schema =
        InkScriptSchemaView::exact_current_with_catalog(ENUMS, &[], RECORDS, COMMANDS).unwrap();
    let text = format!(
        "inkscript_fragment 3; requires {{ procedure_catalog = {}; replay_epoch = {}; }} bindings {{ let existing = select plane {{ cardinality = first; }}; }} program {{ step \"Producer\" as made {{ enabled = true; invoke duplicate_plane {{ plane_id = $existing; }}; }} step \"Batch\" {{ enabled = true; invoke apply_batch_operations {{ operations = [{{ kind = erase; enabled = false; target = {{ kind = references; layer = none; plane = $made.plane; }}; colors = [rgba8(0,0,0,0)]; }}]; }}; }} }}",
        inkpod_format::INKSCRIPT_PROCEDURE_CATALOG_VERSION,
        inkpod_format::INKSCRIPT_REQUIRED_REPLAY_EPOCH
    );
    let source = InkScriptSource::new(InkScriptSourceId::new(702), text.as_bytes()).unwrap();
    let parsed = parse_inkscript(&source);
    let model = build_inkscript_declaration_model(&parsed, &schema).unwrap();
    assert!(
        model
            .dependency_edges()
            .iter()
            .any(|edge| edge.dependency().name() == "made")
    );
    let error = close_inkscript_fragment(
        &parsed,
        &schema,
        &InkScriptFragmentRequest::new(InkScriptFragmentSelection::StepRange {
            first: 1,
            last_inclusive: 1,
        }),
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        InkScriptTypeDiagnosticCode::ExternalMutationDependency
    );
    let closed = close_inkscript_fragment(
        &parsed,
        &schema,
        &InkScriptFragmentRequest::new(InkScriptFragmentSelection::StepRange {
            first: 0,
            last_inclusive: 1,
        }),
    )
    .unwrap();
    let canonical = std::str::from_utf8(closed.canonical_bytes()).unwrap();
    assert!(canonical.contains("enabled = false"));
    assert!(canonical.contains("plane = $made.plane"));
    let canonical_source =
        InkScriptSource::new(InkScriptSourceId::new(703), closed.canonical_bytes()).unwrap();
    let canonical_parsed = parse_inkscript(&canonical_source);
    assert_eq!(
        build_inkscript_declaration_model(&canonical_parsed, &schema)
            .unwrap()
            .steps()
            .len(),
        2
    );
}
