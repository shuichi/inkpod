use inkpod_format::{
    InkScriptCommandSchema, InkScriptRunParameterChoice, InkScriptRunParameterDecision,
    InkScriptSchemaView, InkScriptSource, InkScriptSourceId, InkScriptTypeDiagnosticCode,
    InkScriptTypedValueKind, InkScriptValue, build_inkscript_declaration_model, parse_inkscript,
    resolve_inkscript_run_parameters,
};

const TEST_COMMANDS: &[InkScriptCommandSchema] = &[InkScriptCommandSchema::new("noop", &[])];

fn source(bytes: &[u8]) -> InkScriptSource {
    InkScriptSource::new(InkScriptSourceId::new(105), bytes).expect("fixture must be UTF-8")
}

fn schema() -> InkScriptSchemaView<'static> {
    InkScriptSchemaView::exact_current(&[], TEST_COMMANDS).expect("test schema must compose")
}

fn analyze(
    bytes: &[u8],
) -> Result<inkpod_format::InkScriptDeclarationModel, inkpod_format::InkScriptTypeDiagnostic> {
    let source = source(bytes);
    let parsed = parse_inkscript(&source);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    build_inkscript_declaration_model(&parsed, &schema())
}

#[test]
fn registry_types_constructors_records_and_namespaces_compile_to_owned_values() {
    let input = source(
        br#"inkscript_fragment 2;
requires { procedure_catalog = 3; replay_epoch = 24; }
parameters {
    param replacement: pixel_value = rgba8(0, 64, 255, 255) { label = "Replacement"; ask = each_run; };
    param target_kind: layer_kind = raster;
    param optional_name: nullable<string> = none;
    param points: list<point> = [point(1.5, q16(-1))];
    param descriptor: canonical_raster_descriptor = {
        pixel_format = rgba8; color_space = srgb; alpha = straight;
        width = 2; height = 3; stride = 8; element_count = 6;
    };
    param half_tie: q16 = 0.00000762939453125;
    param three_half_tie: q16 = 0.00002288818359375;
}
bindings {
    let selected_layer = select layer { kind = $target_kind; };
    let selected_plane = select plane { layer = $selected_layer; };
}
program {}
assets {
    asset target_kind {
        asset_id = blake3"0000000000000000000000000000000000000000000000000000000000000000";
        kind = "canonical_raster";
        descriptor = {
            pixel_format = rgba8; color_space = srgb; alpha = straight;
            width = 1; height = 1; stride = 4; element_count = 1;
        };
        data = base64"""AAAAAA==""";
    };
}
"#,
    );
    let parsed = parse_inkscript(&input);
    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    let model = build_inkscript_declaration_model(&parsed, &schema()).unwrap();

    assert_eq!(model.parameters().len(), 7);
    assert_eq!(model.parameters()[0].name(), "replacement");
    assert_eq!(model.parameters()[0].declared_type().name(), "pixel_value");
    assert_eq!(model.parameters()[0].default_value().type_name(), "rgba8");
    assert!(model.parameters()[0].asks_each_run());
    assert_eq!(model.parameters()[0].label(), Some("Replacement"));
    assert_eq!(model.parameters()[3].declared_type().name(), "list<point>");
    assert_eq!(
        model.parameters()[4].default_value().type_name(),
        "canonical_raster_descriptor"
    );
    assert!(matches!(
        model.parameters()[5].default_value().kind(),
        InkScriptTypedValueKind::Q16(0)
    ));
    assert_eq!(
        model.parameters()[6].default_value().kind(),
        &InkScriptTypedValueKind::Q16(2)
    );

    assert_eq!(model.bindings().len(), 2);
    assert_eq!(model.bindings()[0].result_type().name(), "layer_ref");
    assert_eq!(model.bindings()[1].result_type().name(), "plane_ref");
    assert_eq!(model.assets().len(), 1);
    assert_eq!(model.assets()[0].name(), "target_kind");
    assert_eq!(
        model.assets()[0].body().type_name(),
        "canonical_raster_asset"
    );
    assert_eq!(model.source_id(), InkScriptSourceId::new(105));
    assert_eq!(model.parameters()[0].source_range().start().line(), 4);

    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<inkpod_format::InkScriptDeclarationModel>();
}

#[test]
fn type_constructor_record_and_numeric_failures_have_stable_source_ranges() {
    let cases = [
        (
            "param value: absent_type = 1;",
            InkScriptTypeDiagnosticCode::UnknownType,
        ),
        (
            "param value: point = missing_constructor(1, 2);",
            InkScriptTypeDiagnosticCode::UnknownConstructor,
        ),
        (
            "param value: point = point(1.0);",
            InkScriptTypeDiagnosticCode::ConstructorArity,
        ),
        (
            "param value: u32 = \"wrong\";",
            InkScriptTypeDiagnosticCode::TypeMismatch,
        ),
        (
            "param value: u32 = 4294967296;",
            InkScriptTypeDiagnosticCode::NumericOverflow,
        ),
        (
            "param value: rgba8 = rgba8(256, 0, 0, 255);",
            InkScriptTypeDiagnosticCode::ValueOutOfRange,
        ),
        (
            "param value: canonical_raster_descriptor = { pixel_format = rgba8; color_space = srgb; alpha = straight; width = \"bad\"; height = 1; stride = 4; element_count = 1; };",
            InkScriptTypeDiagnosticCode::TypeMismatch,
        ),
        (
            "param value: q16 = 99999999999999999999999999999999999999.5;",
            InkScriptTypeDiagnosticCode::NumericOverflow,
        ),
    ];

    for (declaration, expected) in cases {
        let text = format!(
            "inkscript_fragment 2;\nrequires {{ procedure_catalog = 3; replay_epoch = 24; }}\nparameters {{\n    {declaration}\n}}\nprogram {{}}\n"
        );
        let error = analyze(text.as_bytes()).unwrap_err();
        assert_eq!(error.code(), expected, "{declaration}");
        assert_eq!(error.source_id(), InkScriptSourceId::new(105));
        assert_eq!(error.range().start().line(), 4, "{declaration}");
        assert!(error.range().span().end() > error.range().span().start());
        assert!(!error.path().is_empty());
        assert!(!error.code().as_str().is_empty());
    }
}

#[test]
fn asset_payload_source_is_an_exactly_one_closed_choice() {
    for payload_fields in [
        "",
        r#"data = base64"""AAAAAA=="""; data_file = "payload.bin";"#,
    ] {
        let input = format!(
            r#"inkscript_fragment 2;
requires {{ procedure_catalog = 3; replay_epoch = 24; }}
program {{}}
assets {{
    asset payload {{
        asset_id = blake3"0000000000000000000000000000000000000000000000000000000000000000";
        kind = "canonical_raster";
        descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 4; element_count = 1; }};
        {payload_fields}
    }};
}}
"#
        );
        assert_eq!(
            analyze(input.as_bytes()).unwrap_err().code(),
            InkScriptTypeDiagnosticCode::InvalidStrictPrecondition
        );
    }
}

#[test]
fn value_and_asset_namespaces_reject_duplicates_undefined_forward_and_cycles() {
    let cases = [
        (
            "parameters { param same: u32 = 1; } bindings { let same = select layer {}; } program {}",
            InkScriptTypeDiagnosticCode::DuplicateValueSymbol,
        ),
        (
            "bindings { let target = select layer { kind = $missing; }; } program {}",
            InkScriptTypeDiagnosticCode::UndefinedValueSymbol,
        ),
        (
            "bindings { let plane = select plane { layer = $layer; }; let layer = select layer {}; } program {}",
            InkScriptTypeDiagnosticCode::ForwardReference,
        ),
        (
            "bindings { let plane = select plane { layer = $plane; }; } program {}",
            InkScriptTypeDiagnosticCode::DependencyCycle,
        ),
        (
            "parameters { param same: u32 = 1; } program { step \"x\" as same { enabled = true; invoke noop {}; } }",
            InkScriptTypeDiagnosticCode::DuplicateValueSymbol,
        ),
        (
            "program {} assets { asset duplicate { asset_id = blake3\"0000000000000000000000000000000000000000000000000000000000000000\"; kind = \"canonical_raster\"; descriptor = { pixel_format = rgba8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 4; element_count = 1; }; data = base64\"\"\"AAAAAA==\"\"\"; }; asset duplicate { asset_id = blake3\"0000000000000000000000000000000000000000000000000000000000000000\"; kind = \"canonical_raster\"; descriptor = { pixel_format = rgba8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 4; element_count = 1; }; data = base64\"\"\"AAAAAA==\"\"\"; }; }",
            InkScriptTypeDiagnosticCode::DuplicateAssetSymbol,
        ),
        (
            "parameters { param value: u32 = $later; param later: u32 = 1; } program {}",
            InkScriptTypeDiagnosticCode::LiteralRequired,
        ),
    ];

    for (body, expected) in cases {
        let text = format!(
            "inkscript_fragment 2;\nrequires {{ procedure_catalog = 3; replay_epoch = 24; }}\n{body}\n"
        );
        let error = analyze(text.as_bytes()).unwrap_err();
        assert_eq!(error.code(), expected, "{body}");
        assert!(error.range().start().line() >= 3);
    }
}

#[test]
fn each_run_resolution_is_explicit_immutable_and_atomic_on_cancel_or_invalid_input() {
    let empty = analyze(
        b"inkscript_fragment 2; requires { procedure_catalog = 3; replay_epoch = 24; } program {}",
    )
    .unwrap();
    assert!(empty.parameters().is_empty());
    assert!(empty.bindings().is_empty());
    assert!(empty.assets().is_empty());
    assert!(
        resolve_inkscript_run_parameters(
            &empty,
            &schema(),
            InkScriptRunParameterDecision::Resolve(Vec::new()),
        )
        .unwrap()
        .expect("an empty declaration model has an empty immutable run copy")
        .values()
        .is_empty()
    );

    let model = analyze(
        br#"inkscript_fragment 2;
requires { procedure_catalog = 3; replay_epoch = 24; }
parameters {
    param width: u32 = 1920 { ask = each_run; };
    param color: rgba8 = rgba8(1, 2, 3, 255) { ask = each_run; };
    param fixed: bool = true;
}
program {}
"#,
    )
    .unwrap();
    let unchanged = model.clone();

    assert_eq!(
        resolve_inkscript_run_parameters(&model, &schema(), InkScriptRunParameterDecision::Cancel,)
            .unwrap(),
        None
    );
    assert_eq!(model, unchanged);

    let run = resolve_inkscript_run_parameters(
        &model,
        &schema(),
        InkScriptRunParameterDecision::Resolve(vec![
            InkScriptRunParameterChoice::Override {
                name: "width".to_owned(),
                value: InkScriptValue::Integer("2048".to_owned()),
            },
            InkScriptRunParameterChoice::AcceptDefault {
                name: "color".to_owned(),
            },
        ]),
    )
    .unwrap()
    .expect("resolved values create one immutable run copy");
    assert_eq!(run.values().len(), 3);
    assert!(matches!(
        run.values()[0].value().kind(),
        InkScriptTypedValueKind::U32(2048)
    ));
    assert_eq!(
        run.values()[1].value(),
        model.parameters()[1].default_value()
    );
    assert!(matches!(
        run.values()[2].value().kind(),
        InkScriptTypedValueKind::Boolean(true)
    ));
    assert_eq!(model, unchanged);

    for decision in [
        InkScriptRunParameterDecision::Resolve(vec![InkScriptRunParameterChoice::AcceptDefault {
            name: "width".to_owned(),
        }]),
        InkScriptRunParameterDecision::Resolve(vec![
            InkScriptRunParameterChoice::Override {
                name: "width".to_owned(),
                value: InkScriptValue::String("bad".to_owned()),
            },
            InkScriptRunParameterChoice::AcceptDefault {
                name: "color".to_owned(),
            },
        ]),
        InkScriptRunParameterDecision::Resolve(vec![InkScriptRunParameterChoice::AcceptDefault {
            name: "fixed".to_owned(),
        }]),
    ] {
        assert!(resolve_inkscript_run_parameters(&model, &schema(), decision).is_err());
        assert_eq!(model, unchanged);
    }
}
