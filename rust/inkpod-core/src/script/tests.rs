use super::assets::{FrozenScriptAssets, ScriptAssetLimits, freeze_inkscript_assets};
use super::execute::run_inkscript_on_staged_core;
use super::*;
use crate::asset::{AssetStore, RasterAssetInput};
use crate::primitive::CanonicalInvocation;
use crate::{
    ActivePlane, AssetAlphaSemantics, AssetColorSpace, BatchColorPair, BrushShape, CoordinateSpace,
    Core, DEFAULT_DPI_MILLI, GeometryCrossSection, GeometryOptions, GeometryPrimitive,
    GeometryRequest, GridConfig, GuideAxis, LayerKind, MAX_PERSISTENT_NUMERIC_ID,
    NativeOpenStrategy, PaintTool, PixelFormat, PixelValue, PointF32, PrimitiveId,
    PrimitiveRequest, ProcedureId, StartColorPredicate, StateId, Stroke, StrokeSample,
};
use inkpod_format::{
    InkScriptRunParameterChoice, InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId,
    InkScriptValue, decode_procedure_file, encode_procedure_file,
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

fn complete_source_with_assets(bindings: &str, program: &str, assets: &str) -> InkScriptSource {
    source(format!(
        r#"inkscript 1;
requires {{ procedure_catalog = 1; replay_epoch = 23; }}
inputs {{ current_document; }}
parameters {{}}
bindings {{ {bindings} }}
program {{ {program} }}
output {{ policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "stroke-geometry"; start_number = 1; direction = ascending; }}
execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}
assets {{ {assets} }}
"#
    ))
}

fn asset_digest_text(id: crate::AssetId) -> String {
    let mut text = String::with_capacity(64);
    for byte in id.as_bytes() {
        use std::fmt::Write as _;
        write!(text, "{byte:02x}").unwrap();
    }
    text
}

fn rgba8_asset_id(pixels: Vec<u8>, width: u32, height: u32) -> crate::AssetId {
    let mut store = AssetStore::default();
    store
        .ingest_raster(RasterAssetInput {
            width,
            height,
            pixel_format: PixelFormat::StraightRgba8,
            color_space: Some(AssetColorSpace::Srgb),
            alpha_semantics: AssetAlphaSemantics::Straight,
            canonical_stride: u64::from(width) * 4,
            pixels,
            expected_id: None,
        })
        .unwrap()
        .id()
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
fn document_tree_create_results_feed_later_steps_and_round_trip_history() {
    let source_core = core();
    let before = source_core.document_state_digest().unwrap();
    let program = complete_source(
        "",
        "",
        r#"
step "Create layer" as created_layer {
    enabled = true;
    invoke create_layer { kind = raster; name = "Script layer"; };
}
step "Create plane" as created_plane {
    enabled = true;
    invoke create_plane {
        layer_id = $created_layer.layer;
        kind = raster;
        format = rgba8;
        name = "Script plane";
    };
}
step "Rename created plane" {
    enabled = true;
    invoke set_plane_properties {
        plane_id = $created_plane.plane;
        visible = true;
        editable = true;
        opacity_milli = 1000;
        name = "Result-linked plane";
    };
}
"#,
    );
    let program =
        compile_inkscript(&program, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    assert_eq!(program.budget.max_invocations, 3);
    assert_eq!(program.budget.max_output_ids, 2);

    let mut never_cancel = || false;
    let mut result = run_inkscript_dry(
        &program,
        capture_in_memory_input(&source_core).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(result.report.commit_count, 3);
    assert_eq!(result.report.results.len(), 2);
    assert_eq!(result.report.results[0].alias, "created_layer");
    assert_eq!(result.report.results[0].field, "layer");
    assert_eq!(result.report.results[1].alias, "created_plane");
    assert_eq!(result.report.results[1].field, "plane");
    let created_layer = result.report.results[0].persistent_id;
    let created_plane = result.report.results[1].persistent_id;
    let layer = result
        .staged
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| layer.id == created_layer)
        .unwrap();
    assert!(
        layer
            .planes
            .iter()
            .any(|plane| { plane.id == created_plane && plane.name == "Result-linked plane" })
    );
    let after = result.staged.document_state_digest().unwrap();
    for _ in 0..3 {
        result.staged.undo().unwrap();
    }
    assert_eq!(result.staged.document_state_digest().unwrap(), before);
    for _ in 0..3 {
        result.staged.redo().unwrap();
    }
    assert_eq!(result.staged.document_state_digest().unwrap(), after);

    let editor_digest = result.staged.editor_state().unwrap().digest;
    let native = result
        .staged
        .build_procedure_file(Some(result.staged.current_state), Some(editor_digest))
        .unwrap();
    let bytes = encode_procedure_file(&native).unwrap();
    let mut reopened = Core::from_procedure_file(decode_procedure_file(&bytes).unwrap()).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), after);
    assert_eq!(reopened.history_entries(), result.staged.history_entries());
    assert_eq!(reopened.next_id, result.staged.next_id);
    assert_eq!(reopened.next_procedure, result.staged.next_procedure);
    assert_eq!(reopened.next_state, result.staged.next_state);
    assert!(created_layer < reopened.next_id.next_raw());
    assert!(created_plane < reopened.next_id.next_raw());
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
    for _ in 0..3 {
        reopened.undo().unwrap();
    }
    assert_eq!(reopened.document_state_digest().unwrap(), before);
    for _ in 0..3 {
        reopened.redo().unwrap();
    }
    assert_eq!(reopened.document_state_digest().unwrap(), after);
}

fn document_tree_no_op_fixture(base: &Core) -> InkScriptSource {
    let info = base.document_info().unwrap();
    let uuid = document_uuid(info.document_uuid);
    let binding = format!(
        r#"let target = select layer {{ source_document_uuid = uuid"{uuid}"; persistent_id = {}; }};"#,
        info.layer_id
    );
    complete_source(
        "",
        &binding,
        r#"
step "No-op targets" as unchanged {
    enabled = true;
    invoke edit_targets {
        targets = [layer_target($target)];
        command = set_target_visibility(true);
    };
}
"#,
    )
}

#[test]
fn metadata_color_guide_results_round_trip_native_history_ids_and_savepoints() {
    let source_core = core();
    let before = source_core.document_state_digest().unwrap();
    let source_next_id = source_core.next_id.next_raw();
    let program = complete_source(
        "",
        "",
        r#"
step "Main-line color" {
    enabled = true;
    invoke set_main_line_color { color = rgba16(257, 514, 771, 65535); };
}
step "Palette" {
    enabled = true;
    invoke replace_palette { colors = [rgba8(1, 2, 3, 4), rgba16(5, 6, 7, 8),]; };
}
step "Color chart" {
    enabled = true;
    invoke replace_color_chart {
        entries = [
            { color = rgba8(10, 20, 30, 40); name = chart_name_text("Eight"); },
            { color = rgba16(11, 22, 33, 44); name = chart_name_scalars([78, 0, 85, 76,]); },
        ];
        locked = true;
    };
}
step "Add guide" as created {
    enabled = true;
    invoke add_guide { axis = vertical; position = 2; };
}
step "Move created guide" {
    enabled = true;
    invoke move_guide { guide_id = $created.guide; position = 3; };
}
step "Grid" {
    enabled = true;
    invoke set_grid {
        grid = { origin_x = -1; origin_y = 2; spacing_x = 7; spacing_y = 9; subdivisions = 3; };
    };
}
step "Grid no-op" {
    enabled = true;
    invoke set_grid {
        grid = { origin_x = -1; origin_y = 2; spacing_x = 7; spacing_y = 9; subdivisions = 3; };
    };
}
"#,
    );
    let program =
        compile_inkscript(&program, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    assert_eq!(program.budget.max_invocations, 7);
    assert_eq!(program.budget.max_output_ids, 1);

    let mut never_cancel = || false;
    let mut result = run_inkscript_dry(
        &program,
        capture_in_memory_input(&source_core).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(result.report.commit_count, 6);
    assert_eq!(result.report.results.len(), 1);
    assert_eq!(result.report.results[0].alias, "created");
    assert_eq!(result.report.results[0].field, "guide");
    let guide_id = result.report.results[0].persistent_id;
    assert_eq!(guide_id, source_next_id);
    assert!(guide_id < result.staged.next_id.next_raw());
    assert_eq!(
        result.staged.main_line_color().unwrap(),
        PixelValue::Rgba16([257, 514, 771, u16::MAX])
    );
    assert_eq!(
        result.staged.palette().unwrap(),
        [
            PixelValue::Rgba([1, 2, 3, 4]),
            PixelValue::Rgba16([5, 6, 7, 8]),
        ]
    );
    assert_eq!(
        result.staged.color_chart().unwrap().entries()[1].name,
        "N\0UL"
    );
    assert!(result.staged.color_chart().unwrap().locked());
    assert_eq!(
        result.staged.guides().unwrap(),
        [crate::Guide {
            id: guide_id,
            axis: GuideAxis::Vertical,
            position: 3,
        }]
    );
    assert_eq!(
        result.staged.grid().unwrap(),
        GridConfig {
            origin_x: -1,
            origin_y: 2,
            spacing_x: 7,
            spacing_y: 9,
            subdivisions: 3,
        }
    );

    let after = result.staged.document_state_digest().unwrap();
    for _ in 0..6 {
        result.staged.undo().unwrap();
    }
    assert_eq!(result.staged.document_state_digest().unwrap(), before);
    assert_eq!(result.staged.next_id.next_raw(), source_next_id + 1);
    for _ in 0..6 {
        result.staged.redo().unwrap();
    }
    assert_eq!(result.staged.document_state_digest().unwrap(), after);

    let editor_digest = result.staged.editor_state().unwrap().digest;
    let native = result
        .staged
        .build_procedure_file(Some(result.staged.current_state), Some(editor_digest))
        .unwrap();
    let bytes = encode_procedure_file(&native).unwrap();
    let mut reopened = Core::from_procedure_file(decode_procedure_file(&bytes).unwrap()).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), after);
    assert_eq!(reopened.history_entries(), result.staged.history_entries());
    assert_eq!(reopened.next_id, result.staged.next_id);
    assert_eq!(reopened.next_procedure, result.staged.next_procedure);
    assert_eq!(reopened.next_state, result.staged.next_state);
    assert_eq!(
        reopened.next_journal_event,
        result.staged.next_journal_event
    );
    assert_eq!(reopened.next_branch, result.staged.next_branch);
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
    assert_eq!(reopened.color_chart().unwrap().entries()[1].name, "N\0UL");
    for _ in 0..6 {
        reopened.undo().unwrap();
    }
    assert_eq!(reopened.document_state_digest().unwrap(), before);
    for _ in 0..6 {
        reopened.redo().unwrap();
    }
    assert_eq!(reopened.document_state_digest().unwrap(), after);
}

fn stroke_geometry_import_fixture() -> (
    Core,
    StaticScriptProgram,
    FrozenScriptAssets,
    crate::AssetId,
    Vec<u8>,
) {
    let mut base = Core::new();
    base.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let (_, vector_layer_id) = base
        .create_layer(LayerKind::VectorColoring, "Vector geometry")
        .unwrap();
    let (vector_plane_id, _, _) = base.vector_layer_planes(vector_layer_id).unwrap();
    let info = base.document_info().unwrap();
    let pixels = vec![
        12, 34, 56, 255, 12, 34, 56, 255, 12, 34, 56, 255, 12, 34, 56, 255,
    ];
    let asset_id = rgba8_asset_id(pixels.clone(), 2, 2);
    let bindings = format!(
        r#"
let paint = select plane {{ source_document_uuid = uuid"{}"; persistent_id = {}; }};
let vector_paint = select plane {{ source_document_uuid = uuid"{}"; persistent_id = {vector_plane_id}; }};
"#,
        document_uuid(info.document_uuid),
        info.color_plane_id,
        document_uuid(info.document_uuid),
    );
    let program = complete_source_with_assets(
        &bindings,
        r#"
step "Stroke" {
    enabled = true;
    invoke apply_raster_stroke {
        plane_id = $paint;
        stroke = {
            tool = brush;
            color = rgba8(90, 80, 70, 255);
            diameter = q16(65536);
            shape = round;
            smoothing = 0;
            start_color = any;
            auto_erase = false;
            pressure_size = true;
            samples = [
                { x = q16(32768); y = q16(32768); pressure = 65535; },
                { x = q16(98304); y = q16(32768); pressure = 32768; },
            ];
        };
    };
}
step "Import" {
    enabled = true;
    invoke import_raster_asset { plane_id = $paint; raster = asset(paint_asset); };
}
step "Import no-op" {
    enabled = true;
    invoke import_raster_asset { plane_id = $paint; raster = asset(paint_asset); };
}
step "Geometry" as created {
    enabled = true;
    invoke apply_geometry {
        plane_id = $vector_paint;
        primitive = line;
        segments = [{
            p0 = point(q16(0), q16(65536));
            p1 = point(q16(21845), q16(65536));
            p2 = point(q16(43691), q16(65536));
            p3 = point(q16(65536), q16(65536));
            width_start = q16(65536);
            width_end = q16(65536);
        }];
        fill_boundary = [];
        outline_color = rgba8(200, 100, 50, 255);
        fill_color = rgba8(0, 0, 0, 0);
        outline_width = q16(65536);
        cross_section = round;
        outline = true;
        fill = false;
        closed = false;
    };
}
"#,
        &format!(
            r#"asset paint_asset {{
                asset_id = blake3"{}";
                kind = "canonical_raster";
                descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = 2; height = 2; stride = 8; element_count = 4; }};
                data = base64"""DCI4/wwiOP8MIjj/DCI4/w==""";
            }};"#,
            asset_digest_text(asset_id)
        ),
    );
    let program =
        compile_inkscript(&program, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut never_cancel = || false;
    let assets = freeze_inkscript_assets(
        program.model.assets(),
        &mut [],
        ScriptAssetLimits::exact_current(),
        &mut never_cancel,
    )
    .unwrap();
    (base, program, assets, asset_id, pixels)
}

fn imported_raster_input(pixels: Vec<u8>, expected_id: crate::AssetId) -> RasterAssetInput {
    RasterAssetInput {
        width: 2,
        height: 2,
        pixel_format: PixelFormat::StraightRgba8,
        color_space: Some(AssetColorSpace::Srgb),
        alpha_semantics: AssetAlphaSemantics::Straight,
        canonical_stride: 8,
        pixels,
        expected_id: Some(expected_id),
    }
}

#[test]
fn stroke_geometry_import_execute_typed_assets_and_round_trip_native_history() {
    let (base, program, assets, asset_id, pixels) = stroke_geometry_import_fixture();
    assert_eq!(program.budget.max_invocations, 4);
    assert_eq!(program.budget.max_output_ids, 2);
    assert_eq!(program.budget.max_asset_bytes, 88);
    assert_eq!(assets.asset_id("paint_asset"), Some(asset_id));
    assert_eq!(assets.usage().logical_payload_bytes, 16);

    let base_digest = base.document_state_digest().unwrap();
    let mut never_cancel = || false;
    let mut scripted =
        run_inkscript_on_staged_core(&program, base.clone(), Some(&assets), &mut never_cancel)
            .unwrap();
    assert_eq!(scripted.report.commit_count, 3);
    assert_eq!(
        scripted.report.statements,
        [
            crate::script::report::ScriptStatementOutcome::Committed,
            crate::script::report::ScriptStatementOutcome::Committed,
            crate::script::report::ScriptStatementOutcome::NoOp,
            crate::script::report::ScriptStatementOutcome::Committed,
        ]
    );
    assert_eq!(scripted.report.results.len(), 1);
    assert_eq!(scripted.report.results[0].alias, "created");
    assert_eq!(scripted.report.results[0].field, "paths");
    let path_id = scripted.report.results[0].persistent_id;
    assert!(path_id < scripted.staged.next_id.next_raw());

    let mut direct = base.clone();
    let expected_revision = direct.document_info().unwrap().document_revision;
    direct
        .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision,
            target_plane_id: direct.document_info().unwrap().color_plane_id,
            stroke: Stroke {
                tool: PaintTool::Brush,
                plane: ActivePlane::Color,
                color: [90, 80, 70, 255],
                diameter: 1.0,
                shape: BrushShape::Round,
                smoothing: 0,
                start_color: StartColorPredicate::Any,
                auto_erase: false,
                pressure_size: true,
                coordinate_space: CoordinateSpace::Document,
                samples: vec![
                    StrokeSample {
                        x: 0.5,
                        y: 0.5,
                        pressure: 1.0,
                    },
                    StrokeSample {
                        x: 1.5,
                        y: 0.5,
                        pressure: 32768.0 / 65535.0,
                    },
                ],
            },
        })
        .unwrap();
    let expected_revision = direct.document_info().unwrap().document_revision;
    let color_plane_id = direct.document_info().unwrap().color_plane_id;
    direct
        .execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision,
            target_plane_id: color_plane_id,
            raster: imported_raster_input(pixels.clone(), asset_id),
        })
        .unwrap();
    let expected_revision = direct.document_info().unwrap().document_revision;
    let no_op = direct
        .execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision,
            target_plane_id: color_plane_id,
            raster: imported_raster_input(pixels, asset_id),
        })
        .unwrap();
    assert!(no_op.procedure().is_none());
    let vector_plane_id = base
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| layer.kind == LayerKind::VectorColoring)
        .unwrap()
        .planes[0]
        .id;
    let geometry = GeometryRequest {
        plane_id: vector_plane_id,
        primitive: GeometryPrimitive::Line,
        points: vec![PointF32 { x: 0.0, y: 1.0 }, PointF32 { x: 1.0, y: 1.0 }],
        outline_color: PixelValue::Rgba([200, 100, 50, 255]),
        fill_color: PixelValue::Rgba([0, 0, 0, 0]),
        outline_width: 1.0,
        options: GeometryOptions {
            outline: true,
            fill: false,
            close_path: false,
            bezier_segments: false,
            constrain_45_degrees: false,
            from_center: false,
            taper_start: false,
            taper_end: false,
            cross_section: GeometryCrossSection::Round,
            aspect_ratio_q16: 0,
            polygon_sides: 3,
            rotation_turns: 0,
        },
    };
    let direct_path = direct.apply_geometry(&geometry).unwrap().path_id;
    assert_eq!(path_id, direct_path);
    assert_same_document(&scripted.staged, &direct);

    let after = scripted.staged.document_state_digest().unwrap();
    for _ in 0..3 {
        scripted.staged.undo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        base_digest
    );
    for _ in 0..3 {
        scripted.staged.redo().unwrap();
    }
    assert_eq!(scripted.staged.document_state_digest().unwrap(), after);
    scripted.staged.release_history_cache().unwrap();
    assert_eq!(
        scripted
            .staged
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        after
    );

    let editor_digest = scripted.staged.editor_state().unwrap().digest;
    let native = scripted
        .staged
        .build_procedure_file(Some(scripted.staged.current_state), Some(editor_digest))
        .unwrap();
    let bytes = encode_procedure_file(&native).unwrap();
    let mut reopened = Core::from_procedure_file(decode_procedure_file(&bytes).unwrap()).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), after);
    assert_eq!(
        reopened.history_entries(),
        scripted.staged.history_entries()
    );
    assert_eq!(reopened.next_id, scripted.staged.next_id);
    assert_eq!(reopened.next_procedure, scripted.staged.next_procedure);
    assert_eq!(reopened.next_state, scripted.staged.next_state);
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
    for _ in 0..3 {
        reopened.undo().unwrap();
    }
    assert_eq!(reopened.document_state_digest().unwrap(), base_digest);
    for _ in 0..3 {
        reopened.redo().unwrap();
    }
    assert_eq!(reopened.document_state_digest().unwrap(), after);
}

#[test]
fn stroke_geometry_import_cancel_stale_and_overflow_are_atomic() {
    let (base, program, assets, _, _) = stroke_geometry_import_fixture();
    let before = (
        base.document_state_digest().unwrap(),
        base.document_info().unwrap(),
        base.history_entries(),
        base.next_id,
        base.next_procedure,
        base.next_state,
    );

    let mut cancel = || true;
    assert_eq!(
        run_inkscript_on_staged_core(&program, base.clone(), Some(&assets), &mut cancel)
            .unwrap_err(),
        ScriptRunError::Cancelled
    );
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut never_cancel).unwrap_err(),
        ScriptRunError::InvalidStep
    );

    let mut stale = base.clone();
    let fingerprint = capture_in_memory_fingerprint(&stale).unwrap();
    stale.add_guide(GuideAxis::Vertical, 1).unwrap();
    let stale_before = stale.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_dry(
            &program,
            capture_in_memory_input_at(&stale, fingerprint),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::StaleInput
    );
    assert_eq!(stale.document_state_digest().unwrap(), stale_before);

    let mut overflow = base.clone();
    overflow.next_id = crate::identity::StableIdCursor::from_next_raw(MAX_PERSISTENT_NUMERIC_ID);
    let overflow_before = (
        overflow.document_state_digest().unwrap(),
        overflow.document_info().unwrap(),
        overflow.history_entries(),
        overflow.next_id,
    );
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&program, overflow.clone(), Some(&assets), &mut never_cancel)
            .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        (
            overflow.document_state_digest().unwrap(),
            overflow.document_info().unwrap(),
            overflow.history_entries(),
            overflow.next_id,
        ),
        overflow_before
    );
    assert_eq!(
        (
            base.document_state_digest().unwrap(),
            base.document_info().unwrap(),
            base.history_entries(),
            base.next_id,
            base.next_procedure,
            base.next_state,
        ),
        before
    );
}

#[test]
fn guide_strict_binding_and_semantic_rebind_are_initial_state_exact() {
    let mut source_core = core();
    let source_guide = source_core.add_guide(GuideAxis::Horizontal, 1).unwrap().1;
    let info = source_core.document_info().unwrap();
    let strict = complete_source(
        "",
        &format!(
            r#"let target = select guide {{ source_document_uuid = uuid"{}"; persistent_id = {source_guide}; }};"#,
            document_uuid(info.document_uuid)
        ),
        r#"step "Strict" { enabled = true; invoke move_guide { guide_id = $target; position = 2; }; }"#,
    );
    let strict =
        compile_inkscript(&strict, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut never_cancel = || false;
    let strict_result = run_inkscript_dry(
        &strict,
        capture_in_memory_input(&source_core).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(strict_result.staged.guides().unwrap()[0].position, 2);

    let mut rebound_core = Core::new();
    rebound_core
        .new_cell_with_uuid(
            4,
            4,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            info.document_uuid + 1,
        )
        .unwrap();
    assert_ne!(
        rebound_core.document_info().unwrap().document_uuid,
        info.document_uuid
    );
    rebound_core.add_guide(GuideAxis::Horizontal, 1).unwrap();
    let rebound_before = rebound_core.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_dry(
            &strict,
            capture_in_memory_input(&rebound_core).unwrap(),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::Binding(crate::script::bind::InkScriptBindingError::StalePrecondition)
    );
    assert_eq!(
        rebound_core.document_state_digest().unwrap(),
        rebound_before
    );

    let semantic = complete_source(
        "",
        "let target = select guide { axis = horizontal; position = 1; cardinality = one; missing = error; };",
        r#"step "Rebound" { enabled = true; invoke move_guide { guide_id = $target; position = 2; }; }"#,
    );
    let semantic = compile_inkscript(
        &semantic,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    let rebound = run_inkscript_dry(
        &semantic,
        capture_in_memory_input(&rebound_core).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(rebound.report.commit_count, 1);
    assert_eq!(rebound.staged.guides().unwrap()[0].position, 2);
    assert_eq!(
        rebound_core.document_state_digest().unwrap(),
        rebound_before
    );
}

#[test]
fn metadata_color_guide_cancel_stale_and_id_overflow_are_atomic() {
    let add = complete_source(
        "",
        "",
        r#"step "Add" as created { enabled = true; invoke add_guide { axis = horizontal; position = 1; }; }"#,
    );
    let add = compile_inkscript(&add, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();

    let base = core();
    let before = (
        base.document_state_digest().unwrap(),
        base.document_info().unwrap(),
        base.history_entries(),
        base.next_id,
    );
    let mut cancel = || true;
    assert_eq!(
        run_inkscript_dry(&add, capture_in_memory_input(&base).unwrap(), &mut cancel,).unwrap_err(),
        ScriptRunError::Cancelled
    );
    assert_eq!(
        (
            base.document_state_digest().unwrap(),
            base.document_info().unwrap(),
            base.history_entries(),
            base.next_id,
        ),
        before
    );

    let mut stale = core();
    let fingerprint = capture_in_memory_fingerprint(&stale).unwrap();
    stale.add_guide(GuideAxis::Vertical, 2).unwrap();
    let stale_before = stale.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_dry(
            &add,
            capture_in_memory_input_at(&stale, fingerprint),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::StaleInput
    );
    assert_eq!(stale.document_state_digest().unwrap(), stale_before);

    let mut overflow = core();
    overflow.next_id = crate::identity::StableIdCursor::from_next_raw(MAX_PERSISTENT_NUMERIC_ID);
    let overflow_before = (
        overflow.document_state_digest().unwrap(),
        overflow.document_info().unwrap(),
        overflow.history_entries(),
        overflow.next_id,
    );
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_dry(
            &add,
            capture_in_memory_input(&overflow).unwrap(),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        (
            overflow.document_state_digest().unwrap(),
            overflow.document_info().unwrap(),
            overflow.history_entries(),
            overflow.next_id,
        ),
        overflow_before
    );
}

#[test]
fn document_tree_no_op_result_cancel_and_missing_result_are_atomic() {
    let base = core();
    let info = base.document_info().unwrap();
    let uuid = document_uuid(info.document_uuid);
    let binding = format!(
        r#"let target = select layer {{ source_document_uuid = uuid"{uuid}"; persistent_id = {}; }};"#,
        info.layer_id
    );
    let no_op = document_tree_no_op_fixture(&base);
    let no_op =
        compile_inkscript(&no_op, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    assert_eq!(no_op.budget.max_output_ids, 1);
    let before = (
        base.document_state_digest().unwrap(),
        base.document_info().unwrap(),
        base.history_entries(),
        base.next_id,
    );
    let mut never_cancel = || false;
    let no_op_result = run_inkscript_dry(
        &no_op,
        capture_in_memory_input(&base).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        no_op_result.report.statements,
        vec![crate::script::report::ScriptStatementOutcome::NoOp]
    );
    assert!(no_op_result.report.results.is_empty());
    assert_eq!(no_op_result.report.commit_count, 0);
    assert_eq!(no_op_result.staged.next_id, before.3);

    let missing_result = complete_source(
        "",
        &binding,
        r#"
step "No-op targets" as unchanged {
    enabled = true;
    invoke edit_targets {
        targets = [layer_target($target)];
        command = set_target_visibility(true);
    };
}
step "Use absent list item" {
    enabled = true;
    invoke delete_layer { layer_id = $unchanged.layers[0]; };
}
"#,
    );
    let missing_result = compile_inkscript(
        &missing_result,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_dry(
            &missing_result,
            capture_in_memory_input(&base).unwrap(),
            &mut never_cancel,
        ),
        Err(ScriptRunError::MissingResult)
    ));

    let mut cancel = || true;
    assert!(matches!(
        run_inkscript_dry(&no_op, capture_in_memory_input(&base).unwrap(), &mut cancel,),
        Err(ScriptRunError::Cancelled)
    ));
    assert_eq!(
        (
            base.document_state_digest().unwrap(),
            base.document_info().unwrap(),
            base.history_entries(),
            base.next_id,
        ),
        before
    );
}

#[test]
fn document_tree_mixed_edit_target_results_remain_typed_and_ordered() {
    let mut base = core();
    let (_, raster_layer_id) = base
        .create_layer(LayerKind::Raster, "Raster owner")
        .unwrap();
    let (_, target_layer_id) = base
        .create_layer(LayerKind::Raster, "Layer target")
        .unwrap();
    let info = base.document_info().unwrap();
    let raster_plane_id = base
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| layer.id == raster_layer_id)
        .unwrap()
        .planes[0]
        .id;
    let uuid = document_uuid(info.document_uuid);
    let bindings = format!(
        r#"
let source_layer = select layer {{ source_document_uuid = uuid"{uuid}"; persistent_id = {}; }};
let plane_owner = select layer {{ source_document_uuid = uuid"{uuid}"; persistent_id = {raster_layer_id}; }};
let source_plane = select plane {{ source_document_uuid = uuid"{uuid}"; persistent_id = {raster_plane_id}; }};
"#,
        target_layer_id
    );
    let program = complete_source(
        "",
        &bindings,
        r#"
step "Duplicate mixed targets" as copies {
    enabled = true;
    invoke edit_targets {
        targets = [plane_target($plane_owner, $source_plane), layer_target($source_layer)];
        command = duplicate_targets();
    };
}
step "Delete copied plane" {
    enabled = true;
    invoke delete_plane { plane_id = $copies.planes[0]; };
}
step "Delete copied layer" {
    enabled = true;
    invoke delete_layer { layer_id = $copies.layers[0]; };
}
"#,
    );
    let program =
        compile_inkscript(&program, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    assert_eq!(program.budget.max_invocations, 3);
    assert_eq!(program.budget.max_output_ids, 2);
    let before_digest = base.document_state_digest().unwrap();
    let before_next_id = base.next_id.next_raw();
    let mut never_cancel = || false;
    let mut result = run_inkscript_dry(
        &program,
        capture_in_memory_input(&base).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(result.report.commit_count, 3);
    assert_eq!(result.report.results.len(), 2);
    assert_eq!(result.report.results[0].field, "layers");
    assert_eq!(result.report.results[1].field, "planes");
    assert_eq!(result.report.results[0].output_id_ordinal, 1);
    assert_eq!(result.report.results[1].output_id_ordinal, 0);
    assert!(result.report.results[0].persistent_id >= before_next_id);
    assert!(result.report.results[0].persistent_id > result.report.results[1].persistent_id);
    assert_eq!(
        result.staged.document_state_digest().unwrap(),
        before_digest
    );
    let final_next_id = result.staged.next_id.next_raw();
    assert!(final_next_id > result.report.results[1].persistent_id);
    for _ in 0..3 {
        result.staged.undo().unwrap();
    }
    for _ in 0..3 {
        result.staged.redo().unwrap();
    }
    assert_eq!(
        result.staged.document_state_digest().unwrap(),
        before_digest
    );
    assert_eq!(result.staged.next_id.next_raw(), final_next_id);
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
    assert_send_sync::<FrozenScriptAssets>();
    assert_send_sync::<crate::script::report::ScriptDryRunReport>();
    assert_send::<ScriptDryRunResult>();
    assert_eq!(PrimitiveId::REPLACE_RASTER_COLORS.get(), 0x0005_0040);
}

#[test]
fn sequential_multi_item_native_run_contracts() {
    crate::script::run::test_sequential_multi_item_native_run_contracts();
}

#[test]
fn run_failure_cancel_and_install_race_contracts() {
    crate::script::run::test_failure_cancel_and_install_race_contracts();
}

#[test]
fn run_authority_overwrite_and_temporary_identity_contracts() {
    crate::script::run::test_authority_overwrite_and_temporary_identity_contracts();
}

#[test]
fn dirty_pathless_dry_run_and_saved_snapshot_contracts() {
    crate::script::run::test_dirty_pathless_dry_run_and_saved_snapshot_contracts();
}

#[test]
#[ignore = "release-only InkScript quick performance contract"]
fn approved_quick_performance_contract() {
    super::performance::run_approved_quick();
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
