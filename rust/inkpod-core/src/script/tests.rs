use super::assets::{FrozenScriptAssets, ScriptAssetLimits, freeze_inkscript_assets};
use super::bind::InkScriptBindingError;
use super::catalog::InkScriptPortabilityClass;
use super::compile::{ScriptSchemas, catalog as runtime_catalog};
use super::execute::run_inkscript_on_staged_core;
use super::*;
use crate::asset::{AssetStore, RasterAssetInput};
use crate::primitive::CanonicalInvocation;
use crate::{
    ActivePlane, Adjustment, AirbrushGesture, AirbrushStroke, AnnotationEdit, AnnotationKind,
    AnnotationObjectInput, AnnotationOutput, AssetAlphaSemantics, AssetColorSpace, BatchColorPair,
    BrushShape, CellCreationOptions, CellSizing, CoordinateSpace, Core, DEFAULT_DPI_MILLI,
    EditorTarget, EffectSample, FillOperation, FillRequest, FrameAnchor, GeometryCrossSection,
    GeometryOptions, GeometryPrimitive, GeometryRequest, Gradient, GradientKind, GradientMode,
    GradientStop, GridConfig, GuideAxis, InclusionMode, LayerKind, LightTableDisplayMode,
    LightTableItemInput, LightTableItemProperties, LightTableSource, MAX_PERSISTENT_NUMERIC_ID,
    NativeOpenStrategy, OutputColorGuardProfile, PaintTool, PixelFormat, PixelValue, PointF32,
    PrimitiveId, PrimitiveRequest, ProcedureId, RangeInterpretation, RectI32, RgbaRasterBytes,
    ScopedColorReplaceMode, SelectionConstructionOptions, SelectionLayerOperation,
    SelectionOperation, SelectionShape, ShootingFrameAnchor, ShootingFrameEdit, ShootingFrameInput,
    Stamp, StampGesture, StampShape, StartColorPredicate, StateId, Stroke, StrokeSample,
    VanishingPointEdit, VanishingPointInput, VectorCubicSegment, VectorEraseMode, VectorPathInput,
    VectorWidthMode, plan_cell_creation,
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

fn assert_export_round_trip(base: &Core, scripted: &Core) {
    let base_journal_len = base.journal_entries().len();
    let selected = scripted
        .journal_entries()
        .iter()
        .skip(base_journal_len)
        .filter_map(|entry| match entry {
            crate::JournalEntry::Commit(commit) => Some(commit.event_id()),
            crate::JournalEntry::HistoryMove(_) | crate::JournalEntry::BranchCut(_) => None,
        })
        .collect::<Vec<_>>();
    assert!(!selected.is_empty());
    let mut never_cancel = || false;
    let exported = export_inkscript_fragment(scripted, &selected, &mut never_cancel)
        .unwrap_or_else(|error| {
            let individual = selected
                .iter()
                .map(|event| {
                    let procedure = scripted
                        .journal_entries()
                        .iter()
                        .find_map(|entry| match entry {
                            crate::JournalEntry::Commit(commit) if commit.event_id() == *event => {
                                Some(commit.procedure())
                            }
                            crate::JournalEntry::Commit(_)
                            | crate::JournalEntry::HistoryMove(_)
                            | crate::JournalEntry::BranchCut(_) => None,
                        })
                        .unwrap();
                    (
                        event,
                        procedure.primitive_id(),
                        procedure.input_ids().to_vec(),
                        procedure.asset_ids().to_vec(),
                        export_inkscript_fragment(scripted, &[*event], &mut never_cancel)
                            .map(|fragment| fragment.text().to_owned()),
                    )
                })
                .collect::<Vec<_>>();
            panic!("journal export failed for catalog fixture: {error:?}; {individual:?}")
        });
    let text = exported
        .text()
        .replacen("inkscript_fragment 2;", "inkscript 2;", 1)
        .replacen("program {", "inputs { current_document; }\nprogram {", 1);
    let text = format!(
        "{text}output {{ policy = duplicate; format = inkpod; folder = \"out\"; cell_folder = false; basename = \"export\"; start_number = 1; direction = ascending; }}\nexecution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}\n"
    );
    let source = InkScriptSource::new(InkScriptSourceId::new(2402), text.as_bytes()).unwrap();
    let program = compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new()))
        .unwrap_or_else(|error| panic!("exported fragment did not compile: {error:?}\n{text}"));
    let assets = freeze_inkscript_assets(
        program.model.assets(),
        &mut [],
        ScriptAssetLimits::exact_current(),
        &mut never_cancel,
    )
    .unwrap();
    let mut replay_base = base.clone();
    replay_base.release_history_cache().unwrap();
    let replay =
        run_inkscript_on_staged_core(&program, replay_base, Some(&assets), &mut never_cancel)
            .unwrap_or_else(|error| panic!("exported fragment did not replay: {error:?}\n{text}"));
    assert_eq!(
        replay.staged.document_state_digest().unwrap(),
        scripted.document_state_digest().unwrap(),
        "exported fragment final state diverged\n{text}"
    );
    assert_eq!(replay.staged.next_id, scripted.next_id);
    let expected = scripted
        .journal_entries()
        .iter()
        .skip(base_journal_len)
        .filter_map(|entry| match entry {
            crate::JournalEntry::Commit(commit) => Some(commit.procedure()),
            crate::JournalEntry::HistoryMove(_) | crate::JournalEntry::BranchCut(_) => None,
        })
        .collect::<Vec<_>>();
    let actual = replay
        .staged
        .journal_entries()
        .iter()
        .skip(base_journal_len)
        .filter_map(|entry| match entry {
            crate::JournalEntry::Commit(commit) => Some(commit.procedure()),
            crate::JournalEntry::HistoryMove(_) | crate::JournalEntry::BranchCut(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual, expected,
        "exported canonical procedures diverged\n{text}"
    );
}

fn complete_source(parameters: &str, bindings: &str, program: &str) -> InkScriptSource {
    source(format!(
        r#"inkscript 2;
requires {{ procedure_catalog = 2; replay_epoch = 23; }}
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
        r#"inkscript 2;
requires {{ procedure_catalog = 2; replay_epoch = 23; }}
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

fn gray8_asset_id(pixels: Vec<u8>, width: u32, height: u32) -> crate::AssetId {
    let mut store = AssetStore::default();
    store
        .ingest_raster(RasterAssetInput {
            width,
            height,
            pixel_format: PixelFormat::Grayscale8,
            color_space: None,
            alpha_semantics: AssetAlphaSemantics::Opaque,
            canonical_stride: u64::from(width),
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
        source("inkscript 2; requires { procedure_catalog = 2; replay_epoch = 23; }".to_owned());
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
    assert_export_round_trip(&base, &scripted.staged);
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

fn fill_gradient_fixture() -> (Core, StaticScriptProgram, u64) {
    let mut base = Core::new();
    let plan = plan_cell_creation(&CellCreationOptions {
        sizing: CellSizing::ImagePixels {
            width: 70,
            height: 3,
        },
        dpi_x_milli: DEFAULT_DPI_MILLI,
        dpi_y_milli: DEFAULT_DPI_MILLI,
        margin_milli: 0,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: FrameAnchor::Center,
        initial_layer_kind: LayerKind::BinaryColoring,
        pixel_format: PixelFormat::StraightRgba16,
        count: 1,
    })
    .unwrap();
    base.new_cell_from_creation_plan(plan.item(0).unwrap(), 0x4d31_3841_4649_4c4c)
        .unwrap();
    base.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 63,
            y: 1,
            width: 3,
            height: 1,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    let layers = base.layers().unwrap();
    let layer = layers
        .iter()
        .find(|layer| {
            layer
                .planes
                .iter()
                .any(|plane| plane.kind == crate::PlaneType::Color)
        })
        .unwrap();
    let plane_id = layer
        .planes
        .iter()
        .find(|plane| plane.kind == crate::PlaneType::Color)
        .unwrap()
        .id;
    let info = base.document_info().unwrap();
    let bindings = format!(
        r#"
let paint_layer = select layer {{ source_document_uuid = uuid"{}"; persistent_id = {}; }};
let paint = select plane {{ source_document_uuid = uuid"{}"; persistent_id = {plane_id}; }};
"#,
        document_uuid(info.document_uuid),
        layer.id,
        document_uuid(info.document_uuid),
    );
    let fill = r#"
invoke apply_fill {
    layer_id = $paint_layer;
    plane_id = $paint;
    request = {
        operation = seed;
        seed_x = 63;
        seed_y = 1;
        color = rgba16(1000, 2000, 3000, 65535);
        selection = none;
        use_document_selection = true;
        tolerance = 0;
        detached_regions = false;
        overflow_abort = true;
        gap_close = 0;
        transparent_only = false;
        inclusion_mode = no_inclusion;
        inclusion_colors = [];
        extension_distance = 0;
    };
    use_light_table_boundary = false;
    use_light_table_color = false;
};
"#;
    let gradient = r#"
invoke apply_gradient {
    plane_id = $paint;
    gradient = {
        kind = linear;
        mode = overwrite;
        start = point(q16(4161536), q16(98304));
        end = point(q16(4292608), q16(98304));
        dither = false;
        stops = [
            { position_milli = 0; color = rgba16(65535, 0, 0, 65535); },
            { position_milli = 500; color = rgba16(0, 65535, 0, 32768); },
            { position_milli = 1000; color = rgba16(0, 0, 65535, 65535); },
        ];
    };
};
"#;
    let program = complete_source(
        "",
        &bindings,
        &format!(
            "step \"Fill\" {{ enabled = true; {fill} }}\nstep \"Fill no-op\" {{ enabled = true; {fill} }}\nstep \"Gradient\" {{ enabled = true; {gradient} }}\nstep \"Gradient no-op\" {{ enabled = true; {gradient} }}"
        ),
    );
    let program =
        compile_inkscript(&program, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    (base, program, plane_id)
}

fn fill_gradient_request() -> FillRequest {
    FillRequest {
        operation: FillOperation::Seed,
        seed_x: 63,
        seed_y: 1,
        color: PixelValue::Rgba16([1000, 2000, 3000, 65535]),
        selection: None,
        use_document_selection: true,
        tolerance: 0,
        detached_regions: false,
        overflow_abort: true,
        gap_close: 0,
        transparent_only: false,
        inclusion_mode: InclusionMode::None,
        inclusion_colors: Vec::new(),
        extension_distance: 0,
    }
}

fn fill_gradient_spec() -> Gradient {
    Gradient {
        kind: GradientKind::Linear,
        mode: GradientMode::Overwrite,
        start_x_milli: 63_500,
        start_y_milli: 1_500,
        end_x_milli: 65_500,
        end_y_milli: 1_500,
        dither: false,
        stops: vec![
            GradientStop {
                position_milli: 0,
                color: [65535, 0, 0, 65535],
            },
            GradientStop {
                position_milli: 500,
                color: [0, 65535, 0, 32768],
            },
            GradientStop {
                position_milli: 1000,
                color: [0, 0, 65535, 65535],
            },
        ],
    }
}

#[test]
fn fill_gradient_execute_native_depth_q16_selection_tile_boundary_and_reopen() {
    let (base, program, plane_id) = fill_gradient_fixture();
    assert_eq!(program.budget.max_invocations, 4);
    assert_eq!(program.budget.max_output_ids, 0);
    assert_eq!(program.budget.max_asset_bytes, 160);
    assert_eq!(program.budget.max_work_units, 167_772_160);
    assert_eq!(program.budget.max_output_growth, 0);

    let mut references = crate::primitive::InkScriptRuntimeReferences::default();
    references
        .insert(
            "paint",
            crate::primitive::InkScriptEntityKind::Plane,
            plane_id,
        )
        .unwrap();
    let lowered = crate::primitive::FillGradientScriptStep::from_compiled(
        &program.model.steps()[2],
        &program.frozen_arguments[2],
        &references,
    )
    .unwrap()
    .to_canonical();
    assert_eq!(
        lowered,
        CanonicalInvocation::ApplyGradient {
            plane_id,
            gradient: fill_gradient_spec(),
        }
    );

    let base_digest = base.document_state_digest().unwrap();
    let base_next_id = base.next_id;
    let mut never_cancel = || false;
    let mut scripted =
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut never_cancel).unwrap();
    assert_export_round_trip(&base, &scripted.staged);
    assert_eq!(scripted.report.commit_count, 2);
    assert_eq!(
        scripted.report.statements,
        [
            crate::script::report::ScriptStatementOutcome::Committed,
            crate::script::report::ScriptStatementOutcome::NoOp,
            crate::script::report::ScriptStatementOutcome::Committed,
            crate::script::report::ScriptStatementOutcome::NoOp,
        ]
    );
    assert_eq!(scripted.staged.next_id, base_next_id);
    assert_eq!(
        scripted
            .staged
            .plane_pixel(ActivePlane::Color, 62, 1)
            .unwrap(),
        PixelValue::Rgba16([0; 4])
    );
    assert_eq!(
        scripted
            .staged
            .plane_pixel(ActivePlane::Color, 63, 1)
            .unwrap(),
        PixelValue::Rgba16([65535, 0, 0, 65535])
    );
    assert_eq!(
        scripted
            .staged
            .plane_pixel(ActivePlane::Color, 64, 1)
            .unwrap(),
        PixelValue::Rgba16([0, 65535, 0, 32768])
    );
    assert_eq!(
        scripted
            .staged
            .plane_pixel(ActivePlane::Color, 65, 1)
            .unwrap(),
        PixelValue::Rgba16([0, 0, 65535, 65535])
    );
    assert_eq!(
        scripted
            .staged
            .plane_pixel(ActivePlane::Color, 66, 1)
            .unwrap(),
        PixelValue::Rgba16([0; 4])
    );

    let mut direct = base.clone();
    assert!(
        direct
            .apply_fill(&fill_gradient_request())
            .unwrap()
            .changed_pixels
            > 0
    );
    assert_eq!(
        direct
            .apply_fill(&fill_gradient_request())
            .unwrap()
            .changed_pixels,
        0
    );
    let gradient = fill_gradient_spec();
    let before_gradient = direct.document_info().unwrap().document_revision;
    let changed = direct.apply_gradient_to_plane(plane_id, &gradient).unwrap();
    let no_op = direct.apply_gradient_to_plane(plane_id, &gradient).unwrap();
    assert_eq!(changed.revision(), before_gradient + 1);
    assert_eq!(changed.revision(), no_op.revision());
    assert_same_document(&scripted.staged, &direct);

    let after = scripted.staged.document_state_digest().unwrap();
    scripted.staged.undo().unwrap();
    scripted.staged.undo().unwrap();
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        base_digest
    );
    scripted.staged.redo().unwrap();
    scripted.staged.redo().unwrap();
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
    let reopened = Core::from_procedure_file(decode_procedure_file(&bytes).unwrap()).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), after);
    assert_eq!(reopened.next_id, scripted.staged.next_id);
    assert_eq!(reopened.next_procedure, scripted.staged.next_procedure);
    assert_eq!(reopened.next_state, scripted.staged.next_state);
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
}

#[test]
fn fill_gradient_cancel_invalid_stale_and_resource_failures_are_atomic() {
    let (base, program, plane_id) = fill_gradient_fixture();
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
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut cancel).unwrap_err(),
        ScriptRunError::Cancelled
    );
    let mut checks = 0_u8;
    let mut cancel_after_first_step = || {
        checks += 1;
        checks == 3
    };
    assert_eq!(
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut cancel_after_first_step,)
            .unwrap_err(),
        ScriptRunError::Cancelled
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

    let info = base.document_info().unwrap();
    let invalid = complete_source(
        "",
        &format!(
            "let paint = select plane {{ source_document_uuid = uuid\"{}\"; persistent_id = {plane_id}; }};",
            document_uuid(info.document_uuid)
        ),
        r#"step "Invalid stops" { enabled = true; invoke apply_gradient {
            plane_id = $paint;
            gradient = {
                kind = linear; mode = overwrite;
                start = point(q16(0), q16(0)); end = point(q16(65536), q16(0));
                dither = false;
                stops = [
                    { position_milli = 0; color = rgba16(0, 0, 0, 65535); },
                    { position_milli = 0; color = rgba16(65535, 65535, 65535, 65535); },
                ];
            };
        }; }"#,
    );
    let invalid =
        compile_inkscript(&invalid, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_on_staged_core(&invalid, base.clone(), None, &mut never_cancel),
        Err(ScriptRunError::Core(_))
    ));

    let stops = (0..65)
        .map(|index| format!("{{ position_milli = {index}; color = rgba16(0, 0, 0, 65535); }}"))
        .collect::<Vec<_>>()
        .join(",");
    let resource = complete_source(
        "",
        &format!(
            "let paint = select plane {{ source_document_uuid = uuid\"{}\"; persistent_id = {plane_id}; }};",
            document_uuid(info.document_uuid)
        ),
        &format!(
            r#"step "Too many stops" {{ enabled = true; invoke apply_gradient {{
                plane_id = $paint;
                gradient = {{ kind = linear; mode = overwrite;
                    start = point(q16(0), q16(0)); end = point(q16(65536), q16(0));
                    dither = false; stops = [{stops}];
                }};
            }}; }}"#
        ),
    );
    assert_eq!(
        compile_inkscript(
            &resource,
            InkScriptRunParameterDecision::Resolve(Vec::new()),
        ),
        Err(ScriptCompileError::Catalog(
            crate::script::catalog::CatalogError::ResourceLimit
        ))
    );
    let mut overflow = base.clone();
    overflow.next_procedure = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
    let overflow_before = (
        overflow.document_state_digest().unwrap(),
        overflow.document_info().unwrap(),
        overflow.history_entries(),
        overflow.next_id,
        overflow.next_procedure,
        overflow.next_state,
    );
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&program, overflow.clone(), None, &mut never_cancel)
            .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        (
            overflow.document_state_digest().unwrap(),
            overflow.document_info().unwrap(),
            overflow.history_entries(),
            overflow.next_id,
            overflow.next_procedure,
            overflow.next_state,
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

fn gesture_alpha_adjustment_fixture() -> (Core, StaticScriptProgram, FrozenScriptAssets, u64) {
    let mut base = Core::new();
    let plan = plan_cell_creation(&CellCreationOptions {
        sizing: CellSizing::ImagePixels {
            width: 8,
            height: 4,
        },
        dpi_x_milli: DEFAULT_DPI_MILLI,
        dpi_y_milli: DEFAULT_DPI_MILLI,
        margin_milli: 0,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: FrameAnchor::Center,
        initial_layer_kind: LayerKind::BinaryColoring,
        pixel_format: PixelFormat::StraightRgba16,
        count: 1,
    })
    .unwrap();
    base.new_cell_from_creation_plan(plan.item(0).unwrap(), 0x4d31_3842_4745_5354)
        .unwrap();
    let info = base.document_info().unwrap();
    base.apply_airbrush_to_plane(
        info.color_plane_id,
        AirbrushStroke {
            center_x_milli: 1_500,
            center_y_milli: 1_500,
            radius_milli: 1_400,
            hardness_milli: 700,
            opacity_milli: 1_000,
            color: [40_000, 2_000, 1_000, 65_535],
        },
    )
    .unwrap();
    base.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 1,
            y: 0,
            width: 6,
            height: 4,
        }),
        SelectionOperation::New,
    )
    .unwrap();

    let alpha_pixels = vec![
        0, 32, 64, 96, 128, 160, 192, 255, 255, 192, 160, 128, 96, 64, 32, 0, 0, 64, 128, 192, 255,
        192, 128, 64, 64, 128, 192, 255, 192, 128, 64, 0,
    ];
    let alpha_id = gray8_asset_id(alpha_pixels, 8, 4);
    let bindings = format!(
        r#"let paint = select plane {{ source_document_uuid = uuid"{}"; persistent_id = {}; }};"#,
        document_uuid(info.document_uuid),
        info.color_plane_id,
    );
    let program = complete_source_with_assets(
        &bindings,
        r#"
step "Scoped color" { enabled = true; invoke scoped_color_replace {
    plane_id = $paint; mode = raster_color;
    target = rgba16(0, 0, 0, 0); replacement = rgba16(0, 50000, 1000, 65535);
    region = { kind = rectangle; rect = rect(6, 0, 1, 1); anchor = none; current = none; points = []; samples = []; diameter = none; x = none; y = none; tolerance = none; gap_close = none; };
}; }
step "Airbrush" { enabled = true; invoke apply_airbrush {
    plane_id = $paint; stroke = { center = point(q16(294912), q16(98304)); radius_milli = 1200; hardness_milli = 650; opacity_milli = 800; color = rgba16(1000, 2000, 60000, 50000); };
}; }
step "Airbrush gesture" { enabled = true; invoke apply_airbrush_gesture {
    plane_id = $paint; gesture = {
        samples = [
            { position = point(q16(163840), q16(32768)); pressure_milli = 1000; },
            { position = point(q16(360448), q16(163840)); pressure_milli = 500; },
        ];
        radius_milli = 900; hardness_milli = 500; spacing_milli = 500; opacity_milli = 700;
        fade_milli = 100; pressure_size = true; pressure_opacity = true; continuous_dabs = 1;
        color = rgba16(60000, 3000, 2000, 60000);
    };
}; }
step "Stamp" { enabled = true; invoke apply_stamp {
    plane_id = $paint; stamp = { source_x = 1; source_y = 1; destination_x = 5; destination_y = 0; width = 1; height = 1; opacity_milli = 750; };
}; }
step "Stamp gesture" { enabled = true; invoke apply_stamp_gesture {
    plane_id = $paint; gesture = {
        source = point(q16(98304), q16(98304));
        samples = [
            { position = point(q16(229376), q16(98304)); pressure_milli = 1000; },
            { position = point(q16(294912), q16(163840)); pressure_milli = 750; },
        ];
        radius_milli = 700; hardness_milli = 700; spacing_milli = 600; opacity_milli = 800;
        shape = square; pressure_size = true; pressure_opacity = true;
    };
}; }
step "Blur" { enabled = true; invoke apply_blur { plane_id = $paint; radius = 1; strength_milli = 600; }; }
step "Blur tool" { enabled = true; invoke apply_blur_tool {
    plane_id = $paint;
    shape = { kind = rectangle; rect = rect(1, 0, 6, 4); anchor = none; current = none; points = []; samples = []; diameter = none; x = none; y = none; tolerance = none; gap_close = none; };
    radius = 1; strength_milli = 500;
}; }
step "Alpha asset" { enabled = true; invoke edit_plane_alpha { plane_id = $paint; alpha = asset(alpha_asset); }; }
step "Alpha gradient" { enabled = true; invoke apply_alpha_gradient {
    plane_id = $paint; gradient = {
        kind = linear; mode = overwrite; start = point(q16(98304), q16(98304)); end = point(q16(425984), q16(98304)); dither = false;
        stops = [
            { position_milli = 0; color = rgba16(0, 0, 0, 10000); },
            { position_milli = 500; color = rgba16(0, 0, 0, 35000); },
            { position_milli = 1000; color = rgba16(0, 0, 0, 60000); },
        ];
    };
}; }
step "Create adjustment" as created { enabled = true; invoke create_adjustment_layer {
    name = "InkScript adjustment";
    adjustment = { kind = brightness_contrast; brightness_milli = 100; contrast_milli = -200; channel = none; interpolation = none; points = []; levels = none; };
}; }
step "Update adjustment" { enabled = true; invoke update_adjustment_layer {
    layer_id = $created.layer;
    adjustment = { kind = levels; brightness_milli = none; contrast_milli = none; channel = none; interpolation = none; points = []; levels = { channel = rgb; input_shadow = 1000; input_gamma_milli = 1000; input_highlight = 64000; output_shadow = 500; output_highlight = 65000; }; };
}; }
step "Update adjustment no-op" { enabled = true; invoke update_adjustment_layer {
    layer_id = $created.layer;
    adjustment = { kind = levels; brightness_milli = none; contrast_milli = none; channel = none; interpolation = none; points = []; levels = { channel = rgb; input_shadow = 1000; input_gamma_milli = 1000; input_highlight = 64000; output_shadow = 500; output_highlight = 65000; }; };
}; }
"#,
        &format!(
            r#"asset alpha_asset {{
                asset_id = blake3"{}";
                kind = "canonical_raster";
                descriptor = {{ pixel_format = gray8; color_space = srgb; alpha = straight; width = 8; height = 4; stride = 8; element_count = 32; }};
                data = base64"""ACBAYICgwP//wKCAYEAgAABAgMD/wIBAQIDA/8CAQAA=""";
            }};"#,
            asset_digest_text(alpha_id)
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
    (base, program, assets, info.color_plane_id)
}

fn gesture_alpha_gradient() -> Gradient {
    Gradient {
        kind: GradientKind::Linear,
        mode: GradientMode::Overwrite,
        start_x_milli: 1_500,
        start_y_milli: 1_500,
        end_x_milli: 6_500,
        end_y_milli: 1_500,
        dither: false,
        stops: vec![
            GradientStop {
                position_milli: 0,
                color: [0, 0, 0, 10_000],
            },
            GradientStop {
                position_milli: 500,
                color: [0, 0, 0, 35_000],
            },
            GradientStop {
                position_milli: 1_000,
                color: [0, 0, 0, 60_000],
            },
        ],
    }
}

fn created_adjustment() -> Adjustment {
    Adjustment::BrightnessContrast {
        brightness_milli: 100,
        contrast_milli: -200,
    }
}

fn updated_adjustment() -> Adjustment {
    Adjustment::Levels(crate::Levels {
        channel: crate::Channel::Rgb,
        input_shadow: 1_000,
        input_gamma_milli: 1_000,
        input_highlight: 64_000,
        output_shadow: 500,
        output_highlight: 65_000,
    })
}

#[test]
fn gesture_alpha_adjustment_lowering_preserves_order_shape_depth_and_result_reference() {
    let (_, program, _, plane_id) = gesture_alpha_adjustment_fixture();
    assert_eq!(program.model.steps().len(), 12);
    assert_eq!(program.budget.max_invocations, 12);
    assert_eq!(program.budget.max_output_ids, 1);
    assert_eq!(program.budget.max_output_growth, 1);
    assert_eq!(program.budget.max_asset_bytes, 37_749_360);
    assert_eq!(program.budget.max_work_units, 2_472_629_795);

    let samples = vec![
        EffectSample {
            x_milli: 2_500,
            y_milli: 500,
            pressure_milli: 1_000,
        },
        EffectSample {
            x_milli: 5_500,
            y_milli: 2_500,
            pressure_milli: 500,
        },
    ];
    let stamp_samples = vec![
        EffectSample {
            x_milli: 3_500,
            y_milli: 1_500,
            pressure_milli: 1_000,
        },
        EffectSample {
            x_milli: 4_500,
            y_milli: 2_500,
            pressure_milli: 750,
        },
    ];
    let shape = SelectionShape::Rectangle(RectI32 {
        x: 1,
        y: 0,
        width: 6,
        height: 4,
    });
    let mut references = crate::primitive::InkScriptRuntimeReferences::default();
    references
        .insert(
            "paint",
            crate::primitive::InkScriptEntityKind::Plane,
            plane_id,
        )
        .unwrap();
    references
        .insert(
            "created.layer",
            crate::primitive::InkScriptEntityKind::Layer,
            999,
        )
        .unwrap();
    let expected = vec![
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::ScopedColorReplace {
                plane_id,
                mode: ScopedColorReplaceMode::RasterColor,
                target: PixelValue::Rgba16([0, 0, 0, 0]),
                replacement: PixelValue::Rgba16([0, 50_000, 1_000, 65_535]),
                region: Some(SelectionShape::Rectangle(RectI32 {
                    x: 6,
                    y: 0,
                    width: 1,
                    height: 1,
                })),
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::ApplyAirbrush {
                plane_id,
                stroke: AirbrushStroke {
                    center_x_milli: 4_500,
                    center_y_milli: 1_500,
                    radius_milli: 1_200,
                    hardness_milli: 650,
                    opacity_milli: 800,
                    color: [1_000, 2_000, 60_000, 50_000],
                },
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::ApplyAirbrushGesture {
                plane_id,
                gesture: AirbrushGesture {
                    samples,
                    radius_milli: 900,
                    hardness_milli: 500,
                    spacing_milli: 500,
                    opacity_milli: 700,
                    fade_milli: 100,
                    pressure_size: true,
                    pressure_opacity: true,
                    continuous_dabs: 1,
                    color: [60_000, 3_000, 2_000, 60_000],
                },
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::ApplyStamp {
                plane_id,
                stamp: Stamp {
                    source_x: 1,
                    source_y: 1,
                    destination_x: 5,
                    destination_y: 0,
                    width: 1,
                    height: 1,
                    opacity_milli: 750,
                },
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::ApplyStampGesture {
                plane_id,
                gesture: StampGesture {
                    source_x_milli: 1_500,
                    source_y_milli: 1_500,
                    samples: stamp_samples,
                    radius_milli: 700,
                    hardness_milli: 700,
                    spacing_milli: 600,
                    opacity_milli: 800,
                    shape: StampShape::Square,
                    pressure_size: true,
                    pressure_opacity: true,
                },
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::ApplyBlur {
                plane_id,
                radius: 1,
                strength_milli: 600,
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::ApplyBlurTool {
                plane_id,
                shape,
                radius: 1,
                strength_milli: 500,
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::EditAlpha {
            plane_id,
            asset_symbol: "alpha_asset".to_owned(),
        },
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::ApplyAlphaGradient {
                plane_id,
                gradient: gesture_alpha_gradient(),
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::CreateAdjustmentLayer {
                name: "InkScript adjustment".to_owned(),
                adjustment: created_adjustment(),
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::UpdateAdjustmentLayer {
                layer_id: 999,
                adjustment: updated_adjustment(),
            },
        ),
        crate::primitive::GestureAdjustmentScriptAction::Canonical(
            CanonicalInvocation::UpdateAdjustmentLayer {
                layer_id: 999,
                adjustment: updated_adjustment(),
            },
        ),
    ];
    let actual = program
        .model
        .steps()
        .iter()
        .zip(&program.frozen_arguments)
        .map(|(step, arguments)| {
            crate::primitive::GestureAdjustmentScriptAction::from_compiled(
                step,
                arguments,
                &references,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn gesture_alpha_adjustment_execute_matches_direct_and_round_trips_native_history() {
    let (base, program, assets, plane_id) = gesture_alpha_adjustment_fixture();
    assert_eq!(assets.usage().logical_payload_bytes, 32);
    assert_eq!(
        assets.raster("alpha_asset").unwrap().format(),
        PixelFormat::Grayscale8
    );
    assert_eq!(assets.raster("alpha_asset").unwrap().width(), 8);
    assert_eq!(assets.raster("alpha_asset").unwrap().height(), 4);
    let base_digest = base.document_state_digest().unwrap();
    let base_next_id = base.next_id;
    let outside_left = base.plane_pixel(ActivePlane::Color, 0, 1).unwrap();
    let outside_right = base.plane_pixel(ActivePlane::Color, 7, 1).unwrap();
    let mut never_cancel = || false;
    let mut scripted =
        run_inkscript_on_staged_core(&program, base.clone(), Some(&assets), &mut never_cancel)
            .unwrap();
    assert_export_round_trip(&base, &scripted.staged);
    assert_eq!(scripted.report.statements.len(), 12);
    assert_eq!(
        scripted.report.statements.last(),
        Some(&crate::script::report::ScriptStatementOutcome::NoOp)
    );
    assert_eq!(scripted.report.results.len(), 1);
    assert_eq!(scripted.report.results[0].alias, "created");
    assert_eq!(scripted.report.results[0].field, "layer");
    let adjustment_layer_id = scripted.report.results[0].persistent_id;
    assert!(adjustment_layer_id >= base_next_id.next_raw());
    assert!(adjustment_layer_id < scripted.staged.next_id.next_raw());
    assert_eq!(
        scripted
            .staged
            .plane_pixel(ActivePlane::Color, 0, 1)
            .unwrap(),
        outside_left
    );
    assert_eq!(
        scripted
            .staged
            .plane_pixel(ActivePlane::Color, 7, 1)
            .unwrap(),
        outside_right
    );

    let mut direct = base.clone();
    direct
        .execute_canonical_invocation(CanonicalInvocation::ScopedColorReplace {
            plane_id,
            mode: ScopedColorReplaceMode::RasterColor,
            target: PixelValue::Rgba16([0, 0, 0, 0]),
            replacement: PixelValue::Rgba16([0, 50_000, 1_000, 65_535]),
            region: Some(SelectionShape::Rectangle(RectI32 {
                x: 6,
                y: 0,
                width: 1,
                height: 1,
            })),
        })
        .unwrap();
    direct
        .apply_airbrush_to_plane(
            plane_id,
            AirbrushStroke {
                center_x_milli: 4_500,
                center_y_milli: 1_500,
                radius_milli: 1_200,
                hardness_milli: 650,
                opacity_milli: 800,
                color: [1_000, 2_000, 60_000, 50_000],
            },
        )
        .unwrap();
    direct
        .apply_airbrush_gesture_to_plane(
            plane_id,
            &AirbrushGesture {
                samples: vec![
                    EffectSample {
                        x_milli: 2_500,
                        y_milli: 500,
                        pressure_milli: 1_000,
                    },
                    EffectSample {
                        x_milli: 5_500,
                        y_milli: 2_500,
                        pressure_milli: 500,
                    },
                ],
                radius_milli: 900,
                hardness_milli: 500,
                spacing_milli: 500,
                opacity_milli: 700,
                fade_milli: 100,
                pressure_size: true,
                pressure_opacity: true,
                continuous_dabs: 1,
                color: [60_000, 3_000, 2_000, 60_000],
            },
        )
        .unwrap();
    direct
        .apply_stamp_to_plane(
            plane_id,
            Stamp {
                source_x: 1,
                source_y: 1,
                destination_x: 5,
                destination_y: 0,
                width: 1,
                height: 1,
                opacity_milli: 750,
            },
        )
        .unwrap();
    direct
        .apply_stamp_gesture_to_plane(
            plane_id,
            &StampGesture {
                source_x_milli: 1_500,
                source_y_milli: 1_500,
                samples: vec![
                    EffectSample {
                        x_milli: 3_500,
                        y_milli: 1_500,
                        pressure_milli: 1_000,
                    },
                    EffectSample {
                        x_milli: 4_500,
                        y_milli: 2_500,
                        pressure_milli: 750,
                    },
                ],
                radius_milli: 700,
                hardness_milli: 700,
                spacing_milli: 600,
                opacity_milli: 800,
                shape: StampShape::Square,
                pressure_size: true,
                pressure_opacity: true,
            },
        )
        .unwrap();
    direct.apply_blur_to_plane(plane_id, 1, 600).unwrap();
    direct
        .apply_blur_tool_to_plane(
            plane_id,
            &SelectionShape::Rectangle(RectI32 {
                x: 1,
                y: 0,
                width: 6,
                height: 4,
            }),
            1,
            500,
        )
        .unwrap();
    let PixelValue::Rgba16(before_alpha) = direct.plane_pixel(ActivePlane::Color, 1, 1).unwrap()
    else {
        panic!("native RGBA16 plane changed format before alpha edits");
    };
    direct
        .edit_plane_alpha(plane_id, assets.raster("alpha_asset").unwrap())
        .unwrap();
    direct
        .apply_alpha_gradient_to_plane(plane_id, &gesture_alpha_gradient())
        .unwrap();
    let (_, direct_adjustment_layer) = direct
        .create_adjustment_layer("InkScript adjustment", created_adjustment())
        .unwrap();
    direct
        .update_adjustment_layer(direct_adjustment_layer, updated_adjustment())
        .unwrap();
    let before_no_op = direct.document_info().unwrap().document_revision;
    let no_op = direct
        .update_adjustment_layer(direct_adjustment_layer, updated_adjustment())
        .unwrap();
    assert_eq!(no_op.revision(), before_no_op);
    let PixelValue::Rgba16(after_alpha) = direct.plane_pixel(ActivePlane::Color, 1, 1).unwrap()
    else {
        panic!("native RGBA16 plane changed format after alpha edits");
    };
    assert_eq!(after_alpha[..3], before_alpha[..3]);
    assert_eq!(after_alpha[3], 10_000);
    assert!(after_alpha[..3].iter().any(|channel| *channel > 255));
    assert_same_document(&scripted.staged, &direct);

    let after = scripted.staged.document_state_digest().unwrap();
    let commit_count = scripted.report.commit_count;
    assert_eq!(commit_count, 11);
    for _ in 0..commit_count {
        scripted.staged.undo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        base_digest
    );
    for _ in 0..commit_count {
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
    let reopened = Core::from_procedure_file(decode_procedure_file(&bytes).unwrap()).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), after);
    assert_eq!(reopened.next_id, scripted.staged.next_id);
    assert_eq!(reopened.next_procedure, scripted.staged.next_procedure);
    assert_eq!(reopened.next_state, scripted.staged.next_state);
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
}

#[test]
fn gesture_alpha_adjustment_cancel_invalid_stale_resource_and_overflow_are_atomic() {
    let (base, program, assets, plane_id) = gesture_alpha_adjustment_fixture();
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
    let mut checks = 0_u8;
    let mut cancel_after_first_step = || {
        checks += 1;
        checks == 3
    };
    assert_eq!(
        run_inkscript_on_staged_core(
            &program,
            base.clone(),
            Some(&assets),
            &mut cancel_after_first_step,
        )
        .unwrap_err(),
        ScriptRunError::Cancelled
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

    let info = base.document_info().unwrap();
    let invalid = complete_source(
        "",
        &format!(
            "let paint = select plane {{ source_document_uuid = uuid\"{}\"; persistent_id = {plane_id}; }};",
            document_uuid(info.document_uuid)
        ),
        r#"step "Invalid airbrush" { enabled = true; invoke apply_airbrush {
            plane_id = $paint;
            stroke = { center = point(q16(65536), q16(65536)); radius_milli = 0; hardness_milli = 500; opacity_milli = 500; color = rgba16(1, 2, 3, 4); };
        }; }"#,
    );
    let invalid =
        compile_inkscript(&invalid, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_on_staged_core(&invalid, base.clone(), None, &mut never_cancel),
        Err(ScriptRunError::Core(_))
    ));

    let points = (0..65)
        .map(|index| format!("{{ input = {index}; output = {index}; }}"))
        .collect::<Vec<_>>()
        .join(",");
    let resource = complete_source(
        "",
        "",
        &format!(
            r#"step "Too many curve points" {{ enabled = true; invoke create_adjustment_layer {{
                name = "oversized";
                adjustment = {{ kind = tone_curve; brightness_milli = none; contrast_milli = none; channel = rgb; interpolation = bezier; points = [{points}]; levels = none; }};
            }}; }}"#
        ),
    );
    assert_eq!(
        compile_inkscript(
            &resource,
            InkScriptRunParameterDecision::Resolve(Vec::new()),
        ),
        Err(ScriptCompileError::Catalog(
            crate::script::catalog::CatalogError::ResourceLimit
        ))
    );

    let mut id_overflow = base.clone();
    id_overflow.next_id = crate::identity::StableIdCursor::from_next_raw(MAX_PERSISTENT_NUMERIC_ID);
    let id_before = (
        id_overflow.document_state_digest().unwrap(),
        id_overflow.document_info().unwrap(),
        id_overflow.history_entries(),
        id_overflow.next_id,
        id_overflow.next_procedure,
        id_overflow.next_state,
    );
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_on_staged_core(
            &program,
            id_overflow.clone(),
            Some(&assets),
            &mut never_cancel,
        ),
        Err(ScriptRunError::Core(_)) | Err(ScriptRunError::ResourceLimit)
    ));
    assert_eq!(
        (
            id_overflow.document_state_digest().unwrap(),
            id_overflow.document_info().unwrap(),
            id_overflow.history_entries(),
            id_overflow.next_id,
            id_overflow.next_procedure,
            id_overflow.next_state,
        ),
        id_before
    );

    let mut procedure_overflow = base.clone();
    procedure_overflow.next_procedure = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
    let procedure_before = (
        procedure_overflow.document_state_digest().unwrap(),
        procedure_overflow.document_info().unwrap(),
        procedure_overflow.history_entries(),
        procedure_overflow.next_id,
        procedure_overflow.next_procedure,
        procedure_overflow.next_state,
    );
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(
            &program,
            procedure_overflow.clone(),
            Some(&assets),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        (
            procedure_overflow.document_state_digest().unwrap(),
            procedure_overflow.document_info().unwrap(),
            procedure_overflow.history_entries(),
            procedure_overflow.next_id,
            procedure_overflow.next_procedure,
            procedure_overflow.next_state,
        ),
        procedure_before
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

fn selection_floating_base() -> (Core, u64, u64) {
    let mut base = Core::new();
    base.new_cell_with_uuid(
        8,
        8,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        0x4d31_3953_454c_4543,
    )
    .unwrap();
    let info = base.document_info().unwrap();
    base.execute_canonical_invocation(CanonicalInvocation::ReplaceRasterColors {
        plane_id: info.color_plane_id,
        pairs: vec![BatchColorPair {
            enabled: true,
            old: PixelValue::Rgba([0, 0, 0, 0]),
            new: PixelValue::Rgba([255, 0, 0, 255]),
        }],
    })
    .unwrap();
    let info = base.document_info().unwrap();
    (base, info.layer_id, info.color_plane_id)
}

fn selection_program(base: &Core, layer_id: u64, plane_id: u64) -> StaticScriptProgram {
    let info = base.document_info().unwrap();
    let bindings = format!(
        r#"
let paint_layer = select layer {{ source_document_uuid = uuid"{}"; persistent_id = {layer_id}; }};
let paint = select plane {{ source_document_uuid = uuid"{}"; persistent_id = {plane_id}; }};
"#,
        document_uuid(info.document_uuid),
        document_uuid(info.document_uuid),
    );
    let shape = r#"{ kind = rectangle; rect = rect(0, 0, 2, 2); anchor = none; current = none; points = []; samples = []; diameter = none; x = none; y = none; tolerance = none; gap_close = none; }"#;
    let options = r#"{ aspect_ratio_q16 = 0; from_center = false; constrain_rotation_45 = false; rotation_turns = 0; trace = { shape = round; pressure_size = false; screen_size = false; view_zoom = q16(65536); }; }"#;
    let program = format!(
        r#"
step "Restore pixel" {{ enabled = true; invoke restore_selected_pixels {{
    plane_id = $paint;
    changes = [{{ x = 0; y = 0; before = rgba8(255, 0, 0, 255); after = rgba8(0, 255, 0, 255); }}];
}}; }}
step "Select rectangle" {{ enabled = true; invoke apply_selection {{
    shape = {shape}; operation = new; interpretation = normal; options = {options};
    target_layer_id = $paint_layer; target_plane_id = $paint;
}}; }}
step "Select rectangle no-op" {{ enabled = true; invoke apply_selection {{
    shape = {shape}; operation = new; interpretation = normal; options = {options};
    target_layer_id = $paint_layer; target_plane_id = $paint;
}}; }}
step "Expand" {{ enabled = true; invoke resize_selection {{ pixels = 1; }}; }}
step "Invert" {{ enabled = true; invoke invert_selection {{}}; }}
step "Clear" {{ enabled = true; invoke clear_selection {{}}; }}
step "Select green" {{ enabled = true; invoke select_color {{
    color = rgba8(0, 255, 0, 255); tolerance = 0; different = false; operation = new;
    target_layer_id = $paint_layer; target_plane_id = $paint;
}}; }}
step "Selection layer" as stored {{ enabled = true; invoke selection_to_layer {{ name = "Stored selection"; }}; }}
step "Clear before restore" {{ enabled = true; invoke clear_selection {{}}; }}
step "Selection from layer" {{ enabled = true; invoke selection_from_layer {{ layer_id = $stored.layer; operation = replace; }}; }}
step "Selection from layer no-op" {{ enabled = true; invoke selection_from_layer {{ layer_id = $stored.layer; operation = replace; }}; }}
step "Clear selected content" {{ enabled = true; invoke clear_selected_content {{ target_layer_id = $paint_layer; target_plane_id = $paint; }}; }}
step "Clear restored selection" {{ enabled = true; invoke clear_selection {{}}; }}
"#
    );
    let source = complete_source("", &bindings, &program);
    compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap()
}

#[test]
fn selection_family_bounds_results_direct_equivalence_and_native_reopen() {
    let (base, layer_id, plane_id) = selection_floating_base();
    let info = base.document_info().unwrap();
    let uuid = document_uuid(info.document_uuid);
    let shape = r#"{ kind = rectangle; rect = rect(0, 0, 2, 2); anchor = none; current = none; points = []; samples = []; diameter = none; x = none; y = none; tolerance = none; gap_close = none; }"#;
    let bounds_source = complete_source(
        "",
        &format!(
            r#"let paint_layer = select layer {{ source_document_uuid = uuid"{uuid}"; persistent_id = {layer_id}; }}; let paint = select plane {{ source_document_uuid = uuid"{uuid}"; persistent_id = {plane_id}; }};"#
        ),
        &format!(
            r#"step "Bounds" {{ enabled = true; invoke apply_selection {{ shape = {shape}; operation = new; interpretation = normal; options = {{ aspect_ratio_q16 = 0; from_center = false; constrain_rotation_45 = false; rotation_turns = 0; trace = {{ shape = round; pressure_size = false; screen_size = false; view_zoom = q16(65536); }}; }}; target_layer_id = $paint_layer; target_plane_id = $paint; }}; }}"#
        ),
    );
    let bounds_program = compile_inkscript(
        &bounds_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    let bounds =
        run_inkscript_on_staged_core(&bounds_program, base.clone(), None, &mut never_cancel)
            .unwrap();
    assert_eq!(
        bounds.staged.selection_bounds().unwrap(),
        Some(RectI32 {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        })
    );

    let program = selection_program(&base, layer_id, plane_id);
    assert_eq!(program.budget.max_invocations, 13);
    assert_eq!(program.budget.max_output_ids, 2);
    let base_digest = base.document_state_digest().unwrap();
    let base_next_id = base.next_id.next_raw();
    let mut never_cancel = || false;
    let mut scripted =
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut never_cancel).unwrap();
    assert_export_round_trip(&base, &scripted.staged);
    assert_eq!(scripted.report.commit_count, 11);
    assert_eq!(scripted.report.results.len(), 1);
    assert_eq!(scripted.report.results[0].field, "layer");
    assert_eq!(scripted.report.results[0].output_id_ordinal, 0);
    assert!(scripted.report.results[0].persistent_id >= base_next_id);
    assert_eq!(
        scripted.report.statements[2],
        crate::script::report::ScriptStatementOutcome::NoOp
    );
    assert_eq!(
        scripted.report.statements[10],
        crate::script::report::ScriptStatementOutcome::NoOp
    );
    assert_eq!(scripted.staged.selection_bounds().unwrap(), None);

    let target = EditorTarget { layer_id, plane_id };
    let shape_value = SelectionShape::Rectangle(RectI32 {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    });
    let mut direct = base.clone();
    direct
        .execute_canonical_invocation(CanonicalInvocation::RestoreSelectedPixels {
            plane_id,
            changes: vec![crate::history::PixelChange {
                x: 0,
                y: 0,
                before: PixelValue::Rgba([255, 0, 0, 255]),
                after: PixelValue::Rgba([0, 255, 0, 255]),
            }],
        })
        .unwrap();
    for _ in 0..2 {
        direct
            .execute_canonical_invocation(CanonicalInvocation::ApplySelection {
                shape: shape_value.clone(),
                operation: SelectionOperation::New,
                interpretation: RangeInterpretation::Normal,
                options: SelectionConstructionOptions::default(),
                target,
            })
            .unwrap();
    }
    direct
        .execute_canonical_invocation(CanonicalInvocation::ResizeSelection { pixels: 1 })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::InvertSelection)
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::ClearSelection)
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::SelectColor {
            color: PixelValue::Rgba([0, 255, 0, 255]),
            tolerance: 0,
            different: false,
            operation: SelectionOperation::New,
            target,
        })
        .unwrap();
    let stored = direct
        .execute_canonical_invocation(CanonicalInvocation::SelectionToLayer {
            name: "Stored selection".to_owned(),
        })
        .unwrap()
        .output_ids[0];
    direct
        .execute_canonical_invocation(CanonicalInvocation::ClearSelection)
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::SelectionFromLayer {
            layer_id: stored,
            operation: SelectionLayerOperation::Replace,
        })
        .unwrap();
    let before_noop_revision = direct.document_info().unwrap().document_revision;
    let no_op = direct
        .execute_canonical_invocation(CanonicalInvocation::SelectionFromLayer {
            layer_id: stored,
            operation: SelectionLayerOperation::Replace,
        })
        .unwrap();
    assert_eq!(no_op.dispatch.revision, before_noop_revision);
    assert_eq!(
        direct.document_info().unwrap().document_revision,
        before_noop_revision
    );
    direct
        .execute_canonical_invocation(CanonicalInvocation::ClearSelectedContent { target })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::ClearSelection)
        .unwrap();
    assert_same_document(&scripted.staged, &direct);

    let final_digest = scripted.staged.document_state_digest().unwrap();
    for _ in 0..scripted.report.commit_count {
        scripted.staged.undo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        base_digest
    );
    for _ in 0..scripted.report.commit_count {
        scripted.staged.redo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        final_digest
    );
    scripted.staged.release_history_cache().unwrap();
    assert_eq!(
        scripted
            .staged
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        final_digest
    );

    let editor_digest = scripted.staged.editor_state().unwrap().digest;
    let native = scripted
        .staged
        .build_procedure_file(Some(scripted.staged.current_state), Some(editor_digest))
        .unwrap();
    let reopened = Core::from_procedure_file(
        decode_procedure_file(&encode_procedure_file(&native).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), final_digest);
    assert_eq!(reopened.next_id, scripted.staged.next_id);
    assert_eq!(reopened.next_procedure, scripted.staged.next_procedure);
    assert_eq!(reopened.next_state, scripted.staged.next_state);
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
}

#[test]
fn output_color_guard_script_precondition_is_exact_and_atomic() {
    let (base, _layer_id, _plane_id) = selection_floating_base();
    let revision = base.document_info().unwrap().document_revision;
    let source = complete_source(
        "",
        "",
        &format!(
            r#"step "Guard" {{ enabled = true; invoke select_output_color_guard {{ profile = bt709_conservative_ycbcr; operation = new; base_revision = {revision}; }}; }}"#
        ),
    );
    let program =
        compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut direct = base.clone();
    direct
        .execute_canonical_invocation(CanonicalInvocation::SelectOutputColorGuard {
            profile: OutputColorGuardProfile::Bt709ConservativeYCbCr,
            operation: SelectionOperation::New,
            base_revision: revision,
        })
        .unwrap();
    let mut never_cancel = || false;
    let scripted =
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut never_cancel).unwrap();
    assert_same_document(&scripted.staged, &direct);

    let stale_source = complete_source(
        "",
        "",
        &format!(
            r#"step "Stale guard" {{ enabled = true; invoke select_output_color_guard {{ profile = bt709_conservative_ycbcr; operation = new; base_revision = {}; }}; }}"#,
            revision - 1
        ),
    );
    let stale = compile_inkscript(
        &stale_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let before = (
        base.document_state_digest().unwrap(),
        base.document_info().unwrap(),
        base.history_entries(),
        base.next_id,
        base.next_procedure,
        base.next_state,
    );
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_on_staged_core(&stale, base.clone(), None, &mut never_cancel),
        Err(ScriptRunError::Core(crate::CoreError::InvalidState(_)))
    ));
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

fn floating_program() -> (Core, StaticScriptProgram, FrozenScriptAssets, u64) {
    let (base, _layer_id, plane_id) = selection_floating_base();
    let info = base.document_info().unwrap();
    let pixels = vec![10, 20, 30, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let asset_id = rgba8_asset_id(pixels, 2, 2);
    let uuid = document_uuid(info.document_uuid);
    let program = complete_source_with_assets(
        &format!(
            r#"let paint = select plane {{ source_document_uuid = uuid"{uuid}"; persistent_id = {plane_id}; }};"#
        ),
        &format!(
            r#"step "Floating asset" {{ enabled = true; invoke commit_floating {{
                payload = {{
                    source_document_uuid = uuid"{uuid}";
                    bounds = rect(2, 2, 2, 2);
                    planes = [{{ kind = color; pixel_format = rgba8; origin_x = 2; origin_y = 2; raster = asset(float_asset); }}];
                }};
                destination = {{ kind = existing_planes; existing_plane_ids = [$paint]; new_layer_id = none; new_plane_kind = none; new_pixel_format = none; new_name = none; new_opacity_milli = none; }};
                transform = {{ anchor = center; target_x = q16(196608); target_y = q16(196608); scale_x = q16(65536); scale_y = q16(65536); rotation_turns = 0; }};
            }}; }}"#
        ),
        &format!(
            r#"asset float_asset {{
                asset_id = blake3"{}";
                kind = "canonical_raster";
                descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = 2; height = 2; stride = 8; element_count = 4; }};
                data = base64"""ChQe/wAAAAAAAAAAAAAAAA==""";
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
    (base, program, assets, plane_id)
}

#[test]
fn floating_asset_commit_is_owned_atomic_replayable_and_direct_exact() {
    let (base, program, assets, plane_id) = floating_program();
    assert_eq!(program.budget.max_invocations, 1);
    assert_eq!(program.budget.max_output_ids, 1);
    assert_eq!(program.budget.max_asset_bytes, 536_870_912);
    assert_eq!(program.budget.max_work_units, 1_100_000_000);
    assert_eq!(program.budget.max_output_growth, 67_108_864);
    let asset_id = assets.asset_id("float_asset").unwrap();

    let mut references = crate::primitive::InkScriptRuntimeReferences::default();
    references
        .insert(
            "paint",
            crate::primitive::InkScriptEntityKind::Plane,
            plane_id,
        )
        .unwrap();
    let action = crate::primitive::SelectionFloatingScriptAction::from_compiled(
        &program.model.steps()[0],
        &program.frozen_arguments[0],
        &references,
    )
    .unwrap();
    assert_eq!(action.asset_symbols(), vec!["float_asset"]);
    let lowered = action
        .to_canonical_with_rasters(&[assets.raster("float_asset").unwrap()])
        .unwrap();

    let mut direct = base.clone();
    direct.execute_canonical_invocation(lowered).unwrap();
    let mut never_cancel = || false;
    let mut scripted =
        run_inkscript_on_staged_core(&program, base.clone(), Some(&assets), &mut never_cancel)
            .unwrap();
    assert_eq!(assets.asset_id("float_asset"), Some(asset_id));
    assert_eq!(scripted.report.commit_count, 1);
    assert_eq!(scripted.report.results.len(), 0);
    assert_eq!(
        scripted
            .staged
            .plane_pixel(ActivePlane::Color, 2, 2)
            .unwrap(),
        PixelValue::Rgba([10, 20, 30, 255])
    );
    assert_same_document(&scripted.staged, &direct);

    let base_digest = base.document_state_digest().unwrap();
    let final_digest = scripted.staged.document_state_digest().unwrap();
    scripted.staged.undo().unwrap();
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        base_digest
    );
    scripted.staged.redo().unwrap();
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        final_digest
    );
    scripted.staged.release_history_cache().unwrap();
    assert_eq!(
        scripted
            .staged
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        final_digest
    );
    let editor_digest = scripted.staged.editor_state().unwrap().digest;
    let native = scripted
        .staged
        .build_procedure_file(Some(scripted.staged.current_state), Some(editor_digest))
        .unwrap();
    let reopened = Core::from_procedure_file(
        decode_procedure_file(&encode_procedure_file(&native).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), final_digest);
    assert_eq!(reopened.next_id, scripted.staged.next_id);
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
}

#[test]
fn selection_floating_cancel_invalid_stale_overflow_resource_and_asset_failures_are_atomic() {
    let (base, layer_id, plane_id) = selection_floating_base();
    let program = selection_program(&base, layer_id, plane_id);
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
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut cancel).unwrap_err(),
        ScriptRunError::Cancelled
    );
    let mut polls = 0_u32;
    let mut cancel_after_staging = || {
        polls += 1;
        polls == 5
    };
    assert_eq!(
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut cancel_after_staging,)
            .unwrap_err(),
        ScriptRunError::Cancelled
    );

    let invalid = complete_source(
        "",
        "",
        r#"step "Invalid resize" { enabled = true; invoke resize_selection { pixels = 4097; }; }"#,
    );
    let invalid =
        compile_inkscript(&invalid, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut never_cancel = || false;
    assert!(matches!(
        run_inkscript_on_staged_core(&invalid, base.clone(), None, &mut never_cancel),
        Err(ScriptRunError::Core(_))
    ));

    let mut stale = base.clone();
    let fingerprint = capture_in_memory_fingerprint(&stale).unwrap();
    stale.add_guide(GuideAxis::Vertical, 3).unwrap();
    let stale_digest = stale.document_state_digest().unwrap();
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
    assert_eq!(stale.document_state_digest().unwrap(), stale_digest);

    let mut id_overflow = base.clone();
    id_overflow.next_id = crate::identity::StableIdCursor::from_next_raw(MAX_PERSISTENT_NUMERIC_ID);
    let id_before = id_overflow.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&program, id_overflow.clone(), None, &mut never_cancel)
            .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(id_overflow.document_state_digest().unwrap(), id_before);

    let mut procedure_overflow = base.clone();
    procedure_overflow.next_procedure = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
    let procedure_before = procedure_overflow.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(
            &program,
            procedure_overflow.clone(),
            None,
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        procedure_overflow.document_state_digest().unwrap(),
        procedure_before
    );

    assert_eq!(
        compile_inkscript_with_limits(
            &complete_source(
                "",
                "",
                r#"step "One" { enabled = true; invoke clear_selection {}; } step "Two" { enabled = true; invoke clear_selection {}; }"#,
            ),
            InkScriptRunParameterDecision::Resolve(Vec::new()),
            ScriptCompileLimits::exact_current().with_invocations(1),
        ),
        Err(ScriptCompileError::ResourceLimit)
    );

    let (floating_base, _floating, assets, floating_plane_id) = floating_program();
    let floating_info = floating_base.document_info().unwrap();
    let floating_uuid = document_uuid(floating_info.document_uuid);
    let bad_source = complete_source_with_assets(
        &format!(
            r#"let paint = select plane {{ source_document_uuid = uuid"{floating_uuid}"; persistent_id = {floating_plane_id}; }};"#
        ),
        &format!(
            r#"step "Bad asset format" {{ enabled = true; invoke commit_floating {{ payload = {{ source_document_uuid = uuid"{floating_uuid}"; bounds = rect(2, 2, 2, 2); planes = [{{ kind = color; pixel_format = gray8; origin_x = 2; origin_y = 2; raster = asset(float_asset); }}]; }}; destination = {{ kind = existing_planes; existing_plane_ids = [$paint]; new_layer_id = none; new_plane_kind = none; new_pixel_format = none; new_name = none; new_opacity_milli = none; }}; transform = {{ anchor = center; target_x = q16(196608); target_y = q16(196608); scale_x = q16(65536); scale_y = q16(65536); rotation_turns = 0; }}; }}; }}"#
        ),
        &format!(
            r#"asset float_asset {{ asset_id = blake3"{}"; kind = "canonical_raster"; descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = 2; height = 2; stride = 8; element_count = 4; }}; data = base64"""ChQe/wAAAAAAAAAAAAAAAA=="""; }};"#,
            asset_digest_text(assets.asset_id("float_asset").unwrap())
        ),
    );
    let bad = compile_inkscript(
        &bad_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(
            &bad,
            floating_base.clone(),
            Some(&assets),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::InvalidStep
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

fn vector_script_base_with_uuid(uuid: u128) -> (Core, u64, u64, u64, u64) {
    let mut base = Core::new();
    base.new_cell_with_uuid(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
        .unwrap();
    let info = base.document_info().unwrap();
    base.execute_canonical_invocation(CanonicalInvocation::ReplaceRasterColors {
        plane_id: info.color_plane_id,
        pairs: vec![BatchColorPair {
            enabled: true,
            old: PixelValue::Rgba([0, 0, 0, 0]),
            new: PixelValue::Rgba([255, 0, 0, 255]),
        }],
    })
    .unwrap();
    let (_, vector_layer_id) = base
        .create_layer(LayerKind::VectorColoring, "Vector Script")
        .unwrap();
    let (main_plane_id, _trace_plane_id, fill_plane_id) =
        base.vector_layer_planes(vector_layer_id).unwrap();
    (
        base,
        info.color_plane_id,
        vector_layer_id,
        main_plane_id,
        fill_plane_id,
    )
}

fn vector_script_base() -> (Core, u64, u64, u64, u64) {
    vector_script_base_with_uuid(0x4d32_3056_4543_544f)
}

fn vector_segment(x0: f32, y0: f32, x1: f32, y1: f32) -> VectorCubicSegment {
    VectorCubicSegment {
        p0: PointF32 { x: x0, y: y0 },
        p1: PointF32 { x: x0, y: y0 },
        p2: PointF32 { x: x1, y: y1 },
        p3: PointF32 { x: x1, y: y1 },
        width_start: 1.0,
        width_end: 1.0,
    }
}

#[test]
fn vector_catalog_results_index_roles_direct_equivalence_and_native_reopen() {
    let (base, raster_plane_id, vector_layer_id, main_plane_id, fill_plane_id) =
        vector_script_base();
    let info = base.document_info().unwrap();
    let uuid = document_uuid(info.document_uuid);
    let bindings = format!(
        r#"
let raster = select plane {{ source_document_uuid = uuid"{uuid}"; persistent_id = {raster_plane_id}; }};
let vector_layer = select layer {{ source_document_uuid = uuid"{uuid}"; persistent_id = {vector_layer_id}; }};
let vector_main = select plane {{ source_document_uuid = uuid"{uuid}"; persistent_id = {main_plane_id}; }};
let vector_fill = select plane {{ source_document_uuid = uuid"{uuid}"; persistent_id = {fill_plane_id}; }};
"#
    );
    let segment = |x0: i64, y0: i64, x1: i64, y1: i64| {
        format!(
            "{{ p0 = point(q16({x0}), q16({y0})); p1 = point(q16({x0}), q16({y0})); p2 = point(q16({x1}), q16({y1})); p3 = point(q16({x1}), q16({y1})); width_start = q16(65536); width_end = q16(65536); }}"
        )
    };
    let square = [
        segment(65_536, 65_536, 196_608, 65_536),
        segment(196_608, 65_536, 196_608, 196_608),
        segment(196_608, 196_608, 65_536, 196_608),
        segment(65_536, 196_608, 65_536, 65_536),
    ]
    .join(", ");
    let program_text = format!(
        r#"
step "Closed path" as outline {{ enabled = true; invoke vector_add_path {{ plane_id = $vector_main; input = {{ segments = [{square}]; color = rgba16(1000, 2000, 3000, 65535); closed = true; }}; }}; }}
step "Fill" as colored {{ enabled = true; invoke vector_add_fill {{ plane_id = $vector_fill; boundary_path_ids = [$outline.paths[0]]; color = rgba16(4000, 5000, 6000, 65535); }}; }}
step "Open left" as left {{ enabled = true; invoke vector_add_path {{ plane_id = $vector_main; input = {{ segments = [{}]; color = rgba8(10, 20, 30, 255); closed = false; }}; }}; }}
step "Open right" as right {{ enabled = true; invoke vector_add_path {{ plane_id = $vector_main; input = {{ segments = [{}]; color = rgba8(10, 20, 30, 255); closed = false; }}; }}; }}
step "Connect" as connector {{ enabled = true; invoke vector_connect {{ plane_id = $vector_main; maximum_gap = q16(65536); }}; }}
step "Width" {{ enabled = true; invoke vector_correct_width {{ path_ids = [$left.paths[0], $connector.paths[0]]; width = {{ operation = add; value = q16(32768); }}; }}; }}
step "Erase no hit" {{ enabled = true; invoke vector_erase {{ plane_id = $vector_main; point = point(q16(458752), q16(458752)); radius = q16(32768); mode = whole_path; }}; }}
step "Rasterize" as rasterized {{ enabled = true; invoke rasterize_vector_layer {{ layer_id = $vector_layer; antialias = true; name = "Rasterized Vector"; }}; }}
step "Vectorize existing" as extracted {{ enabled = true; invoke vectorize_raster_plane {{ source_plane_id = $raster; target_vector_layer_id = $vector_layer; alpha_threshold = 1; }}; }}
step "Vectorize new" as traced {{ enabled = true; invoke vectorize_raster_plane_into_new_layer {{ source_plane_id = $raster; alpha_threshold = 1; name = "Traced Vector"; }}; }}
"#,
        segment(65_536, 262_144, 131_072, 262_144),
        segment(163_840, 262_144, 229_376, 262_144),
    );
    let source = complete_source("", &bindings, &program_text);
    let schemas = super::compile::ScriptSchemas::new();
    let runtime_catalog = super::compile::catalog(&schemas.commands).unwrap();
    assert_eq!(
        runtime_catalog
            .entry("vector_correct_width")
            .unwrap()
            .editor
            .legacy_projection,
        Some("line_width")
    );
    let program =
        compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut never_cancel = || false;
    let mut scripted =
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut never_cancel).unwrap();
    assert_export_round_trip(&base, &scripted.staged);

    assert_eq!(scripted.report.statements.len(), 10);
    assert_eq!(scripted.report.commit_count, 9);
    assert_eq!(
        scripted.report.statements[6],
        crate::script::report::ScriptStatementOutcome::NoOp
    );
    assert!(
        scripted
            .report
            .results
            .iter()
            .any(|result| result.alias == "traced"
                && result.field == "layer"
                && result.output_id_ordinal == 0)
    );
    let traced_fill_ordinals = scripted
        .report
        .results
        .iter()
        .filter(|result| result.alias == "traced" && result.field == "fills")
        .map(|result| result.output_id_ordinal)
        .collect::<Vec<_>>();
    assert!(!traced_fill_ordinals.is_empty());
    assert_eq!(traced_fill_ordinals[0], 1);
    assert!(
        traced_fill_ordinals
            .windows(2)
            .all(|pair| pair[1] == pair[0] + 1)
    );
    assert!(
        scripted
            .staged
            .vector_paths()
            .unwrap()
            .iter()
            .any(|path| path.color == PixelValue::Rgba16([1000, 2000, 3000, 65535]))
    );
    assert!(
        scripted
            .staged
            .vector_fills()
            .unwrap()
            .iter()
            .any(|fill| fill.color == PixelValue::Rgba16([4000, 5000, 6000, 65535]))
    );

    let mut direct = base.clone();
    let closed_id = direct
        .execute_canonical_invocation(CanonicalInvocation::VectorAddPath {
            plane_id: main_plane_id,
            input: VectorPathInput {
                segments: vec![
                    vector_segment(1.0, 1.0, 3.0, 1.0),
                    vector_segment(3.0, 1.0, 3.0, 3.0),
                    vector_segment(3.0, 3.0, 1.0, 3.0),
                    vector_segment(1.0, 3.0, 1.0, 1.0),
                ],
                color: PixelValue::Rgba16([1000, 2000, 3000, 65535]),
                closed: true,
            },
        })
        .unwrap()
        .output_ids[0];
    direct
        .execute_canonical_invocation(CanonicalInvocation::VectorAddFill {
            plane_id: fill_plane_id,
            boundary_path_ids: vec![closed_id],
            color: PixelValue::Rgba16([4000, 5000, 6000, 65535]),
        })
        .unwrap();
    let left_id = direct
        .execute_canonical_invocation(CanonicalInvocation::VectorAddPath {
            plane_id: main_plane_id,
            input: VectorPathInput {
                segments: vec![vector_segment(1.0, 4.0, 2.0, 4.0)],
                color: PixelValue::Rgba([10, 20, 30, 255]),
                closed: false,
            },
        })
        .unwrap()
        .output_ids[0];
    direct
        .execute_canonical_invocation(CanonicalInvocation::VectorAddPath {
            plane_id: main_plane_id,
            input: VectorPathInput {
                segments: vec![vector_segment(2.5, 4.0, 3.5, 4.0)],
                color: PixelValue::Rgba([10, 20, 30, 255]),
                closed: false,
            },
        })
        .unwrap();
    let connector_id = direct
        .execute_canonical_invocation(CanonicalInvocation::VectorConnect {
            plane_id: main_plane_id,
            maximum_gap: 1.0,
        })
        .unwrap()
        .output_ids[0];
    direct
        .execute_canonical_invocation(CanonicalInvocation::VectorCorrectWidth {
            path_ids: vec![left_id, connector_id],
            mode: VectorWidthMode::Add(0.5),
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::VectorErase {
            plane_id: main_plane_id,
            point: PointF32 { x: 7.0, y: 7.0 },
            radius: 0.5,
            mode: VectorEraseMode::WholePath,
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::RasterizeVectorLayer {
            layer_id: vector_layer_id,
            antialias: true,
            name: "Rasterized Vector".to_owned(),
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::VectorizeRasterPlane {
            source_plane_id: raster_plane_id,
            target_vector_layer_id: vector_layer_id,
            alpha_threshold: 1,
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::VectorizeRasterPlaneIntoNewLayer {
            source_plane_id: raster_plane_id,
            alpha_threshold: 1,
            name: "Traced Vector".to_owned(),
        })
        .unwrap();
    assert_same_document(&scripted.staged, &direct);

    let base_digest = base.document_state_digest().unwrap();
    let final_digest = scripted.staged.document_state_digest().unwrap();
    for _ in 0..scripted.report.commit_count {
        scripted.staged.undo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        base_digest
    );
    for _ in 0..scripted.report.commit_count {
        scripted.staged.redo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        final_digest
    );
    scripted.staged.release_history_cache().unwrap();
    assert_eq!(
        scripted
            .staged
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        final_digest
    );
    let editor_digest = scripted.staged.editor_state().unwrap().digest;
    let native = scripted
        .staged
        .build_procedure_file(Some(scripted.staged.current_state), Some(editor_digest))
        .unwrap();
    let reopened = Core::from_procedure_file(
        decode_procedure_file(&encode_procedure_file(&native).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), final_digest);
    assert_eq!(reopened.next_id, scripted.staged.next_id);
    assert_eq!(reopened.next_procedure, scripted.staged.next_procedure);
    assert_eq!(reopened.next_state, scripted.staged.next_state);
    assert_eq!(reopened.savepoint, Some(reopened.current_state));
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
}

#[test]
fn vector_strict_binding_and_semantic_rebound_are_initial_state_exact() {
    fn add_initial_path(base: &mut Core, plane_id: u64) -> u64 {
        base.execute_canonical_invocation(CanonicalInvocation::VectorAddPath {
            plane_id,
            input: VectorPathInput {
                segments: vec![vector_segment(1.0, 1.0, 3.0, 1.0)],
                color: PixelValue::Rgba([1, 2, 3, 255]),
                closed: false,
            },
        })
        .unwrap()
        .output_ids[0]
    }

    let (mut source_core, _raster, _layer, source_plane, _fill) = vector_script_base();
    let source_path = add_initial_path(&mut source_core, source_plane);
    let source_info = source_core.document_info().unwrap();
    let strict_source = complete_source(
        "",
        &format!(
            r#"let target = select vector_path {{ source_document_uuid = uuid"{}"; persistent_id = {source_path}; }};"#,
            document_uuid(source_info.document_uuid)
        ),
        r#"step "Strict width" { enabled = true; invoke vector_correct_width { path_ids = [$target]; width = { operation = constant; value = q16(131072); }; }; }"#,
    );
    let strict = compile_inkscript(
        &strict_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    let strict_result = run_inkscript_dry(
        &strict,
        capture_in_memory_input(&source_core).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        strict_result.staged.vector_paths().unwrap()[0].segments[0].width_start,
        2.0
    );

    let (mut rebound_core, _raster, _layer, rebound_plane, _fill) =
        vector_script_base_with_uuid(source_info.document_uuid + 1);
    add_initial_path(&mut rebound_core, rebound_plane);
    let rebound_before = rebound_core.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_dry(
            &strict,
            capture_in_memory_input(&rebound_core).unwrap(),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::Binding(InkScriptBindingError::StalePrecondition)
    );
    assert_eq!(
        rebound_core.document_state_digest().unwrap(),
        rebound_before
    );

    let rebound_source = complete_source(
        "",
        r#"
let vector_layer = select layer { name = "Vector Script"; };
let vector_main = select plane { layer = $vector_layer; plane_kind = vector_main_line; };
let target = select vector_path { plane = $vector_main; };
"#,
        r#"step "Rebound width" { enabled = true; invoke vector_correct_width { path_ids = [$target]; width = { operation = constant; value = q16(131072); }; }; }"#,
    );
    let rebound = compile_inkscript(
        &rebound_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    let rebound_result = run_inkscript_dry(
        &rebound,
        capture_in_memory_input(&rebound_core).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        rebound_result.staged.vector_paths().unwrap()[0].segments[0].width_start,
        2.0
    );
}

#[test]
fn vector_cancel_invalid_stale_overflow_and_resource_failures_are_atomic() {
    let (base, _raster, _layer, main_plane_id, _fill) = vector_script_base();
    let info = base.document_info().unwrap();
    let uuid = document_uuid(info.document_uuid);
    let bindings = format!(
        r#"let vector_main = select plane {{ source_document_uuid = uuid"{uuid}"; persistent_id = {main_plane_id}; }};"#
    );
    let path_step = |name: &str| {
        format!(
            r#"step "{name}" {{ enabled = true; invoke vector_add_path {{ plane_id = $vector_main; input = {{ segments = [{{ p0 = point(q16(65536), q16(65536)); p1 = point(q16(65536), q16(65536)); p2 = point(q16(131072), q16(65536)); p3 = point(q16(131072), q16(65536)); width_start = q16(65536); width_end = q16(65536); }}]; color = rgba8(1, 2, 3, 255); closed = false; }}; }}; }}"#
        )
    };
    let source = complete_source(
        "",
        &bindings,
        &format!("{} {}", path_step("One"), path_step("Two")),
    );
    let program =
        compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
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
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut cancel).unwrap_err(),
        ScriptRunError::Cancelled
    );
    let mut polls = 0_u32;
    let mut cancel_after_staging = || {
        polls += 1;
        polls == 3
    };
    assert_eq!(
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut cancel_after_staging,)
            .unwrap_err(),
        ScriptRunError::Cancelled
    );

    let invalid_source = complete_source(
        "",
        &bindings,
        &path_step("Invalid").replace("rgba8(1, 2, 3, 255)", "gray8(1)"),
    );
    let invalid = compile_inkscript(
        &invalid_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&invalid, base.clone(), None, &mut never_cancel).unwrap_err(),
        ScriptRunError::InvalidStep
    );

    let mut stale = base.clone();
    let fingerprint = capture_in_memory_fingerprint(&stale).unwrap();
    stale.add_guide(GuideAxis::Vertical, 2).unwrap();
    let stale_digest = stale.document_state_digest().unwrap();
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
    assert_eq!(stale.document_state_digest().unwrap(), stale_digest);

    let one_source = complete_source("", &bindings, &path_step("One"));
    let one = compile_inkscript(
        &one_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut id_overflow = base.clone();
    id_overflow.next_id = crate::identity::StableIdCursor::from_next_raw(MAX_PERSISTENT_NUMERIC_ID);
    let overflow_digest = id_overflow.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&one, id_overflow.clone(), None, &mut never_cancel)
            .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        id_overflow.document_state_digest().unwrap(),
        overflow_digest
    );

    let mut procedure_overflow = base.clone();
    procedure_overflow.next_procedure = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
    let procedure_digest = procedure_overflow.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&one, procedure_overflow.clone(), None, &mut never_cancel,)
            .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        procedure_overflow.document_state_digest().unwrap(),
        procedure_digest
    );

    assert_eq!(
        compile_inkscript_with_limits(
            &source,
            InkScriptRunParameterDecision::Resolve(Vec::new()),
            ScriptCompileLimits::exact_current().with_invocations(1),
        ),
        Err(ScriptCompileError::ResourceLimit)
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

fn annotation_frame_script_base_with_uuid(uuid: u128) -> (Core, u64, u64, u64) {
    let mut base = Core::new();
    base.new_cell_with_uuid(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
        .unwrap();
    let (_, text_layer) = base.create_layer(LayerKind::Text, "Script Text").unwrap();
    let (_, annotation_layer) = base
        .create_layer(LayerKind::Annotation, "Script Annotation")
        .unwrap();
    let (_, vanishing_layer) = base
        .create_layer(LayerKind::VanishingPoint, "Script Vanishing")
        .unwrap();
    (base, text_layer, annotation_layer, vanishing_layer)
}

fn annotation_frame_script_base() -> (Core, u64, u64, u64) {
    annotation_frame_script_base_with_uuid(0x4d32_3141_4e4e_4f54)
}

fn annotation_input(layer_id: u64) -> AnnotationObjectInput {
    AnnotationObjectInput {
        layer_id,
        kind: AnnotationKind::Text,
        output: AnnotationOutput::Instruction,
        bounds: RectI32 {
            x: 1,
            y: 2,
            width: 6,
            height: 4,
        },
        font_family_hint: "Inkpod Sans".to_owned(),
        font_size_milli: 12_000,
        style_flags: 1,
        color: PixelValue::Rgba16([1_000, 2_000, 3_000, 65_535]),
        text: "Frame note".to_owned(),
        points: Vec::new(),
        stroke_width_milli: 0,
    }
}

fn shooting_input() -> ShootingFrameInput {
    ShootingFrameInput {
        center_x_milli: 8_000,
        center_y_milli: 8_000,
        width_milli: 10_000,
        height_milli: 6_000,
        rotation_turns: 0x1000_0000,
        anchor: ShootingFrameAnchor::Center,
        visible: true,
        include_in_instruction_export: true,
    }
}

fn vanishing_input(layer_id: u64, x_milli: i64) -> VanishingPointInput {
    VanishingPointInput {
        layer_id,
        x_milli,
        y_milli: 7_000,
        interval_milli_degrees: 30_000,
        angle_milli_degrees: 15_000,
        color: PixelValue::Rgba16([4_000, 5_000, 6_000, 65_535]),
        opacity_milli: 750,
        visible: true,
    }
}

#[test]
fn annotation_frame_catalog_results_direct_equivalence_and_native_reopen() {
    let (base, text_layer, _annotation_layer, vanishing_layer) = annotation_frame_script_base();
    let base_next_id = base.next_id.next_raw();
    let info = base.document_info().unwrap();
    let uuid = document_uuid(info.document_uuid);
    let bindings = format!(
        r#"
let text_layer = select layer {{ source_document_uuid = uuid"{uuid}"; persistent_id = {text_layer}; }};
let vanishing_layer = select layer {{ source_document_uuid = uuid"{uuid}"; persistent_id = {vanishing_layer}; }};
"#
    );
    let program_text = r#"
step "Annotation" as annotation_created { enabled = true; invoke edit_annotations { edits = [{ operation = 1; object_id = none; input = { layer_id = $text_layer; kind = text; output = instruction; bounds = rect(1, 2, 6, 4); font_family_hint = "Inkpod Sans"; font_size_milli = 12000; style_flags = 1; color = rgba16(1000, 2000, 3000, 65535); text = "Frame note"; points = []; stroke_width_milli = 0; }; delta_x = 0; delta_y = 0; }]; }; }
step "Frame" as frame_created { enabled = true; invoke edit_shooting_frame { edit = { operation = 1; frame_id = none; input = { center_x_milli = 8000; center_y_milli = 8000; width_milli = 10000; height_milli = 6000; rotation_turns = 268435456; anchor = center; visible = true; include_in_instruction_export = true; }; }; }; }
step "Vanishing" as vanishing_created { enabled = true; invoke edit_vanishing_points { edits = [{ operation = 1; point_id = none; input = { layer_id = $vanishing_layer; x_milli = 3000; y_milli = 7000; interval_milli_degrees = 30000; angle_milli_degrees = 15000; color = rgba16(4000, 5000, 6000, 65535); opacity_milli = 750; visible = true; }; }, { operation = 1; point_id = none; input = { layer_id = $vanishing_layer; x_milli = 12000; y_milli = 7000; interval_milli_degrees = 30000; angle_milli_degrees = 15000; color = rgba16(4000, 5000, 6000, 65535); opacity_milli = 750; visible = true; }; }]; }; }
step "Annotation no-op" as annotation_same { enabled = true; invoke edit_annotations { edits = [{ operation = 2; object_id = $annotation_created.annotations[0]; input = { layer_id = $text_layer; kind = text; output = instruction; bounds = rect(1, 2, 6, 4); font_family_hint = "Inkpod Sans"; font_size_milli = 12000; style_flags = 1; color = rgba16(1000, 2000, 3000, 65535); text = "Frame note"; points = []; stroke_width_milli = 0; }; delta_x = 0; delta_y = 0; }]; }; }
step "Frame no-op" as frame_same { enabled = true; invoke edit_shooting_frame { edit = { operation = 2; frame_id = $frame_created.shooting_frames[0]; input = { center_x_milli = 8000; center_y_milli = 8000; width_milli = 10000; height_milli = 6000; rotation_turns = 268435456; anchor = center; visible = true; include_in_instruction_export = true; }; }; }; }
step "Vanishing no-op" as vanishing_same { enabled = true; invoke edit_vanishing_points { edits = [{ operation = 2; point_id = $vanishing_created.vanishing_points[1]; input = { layer_id = $vanishing_layer; x_milli = 12000; y_milli = 7000; interval_milli_degrees = 30000; angle_milli_degrees = 15000; color = rgba16(4000, 5000, 6000, 65535); opacity_milli = 750; visible = true; }; }]; }; }
step "Annotation move no-op" { enabled = true; invoke edit_annotations { edits = [{ operation = 3; object_id = $annotation_created.annotations[0]; input = none; delta_x = 0; delta_y = 0; }]; }; }
step "Delete annotation" { enabled = true; invoke edit_annotations { edits = [{ operation = 4; object_id = $annotation_created.annotations[0]; input = none; delta_x = 0; delta_y = 0; }]; }; }
step "Delete frame" { enabled = true; invoke edit_shooting_frame { edit = { operation = 3; frame_id = $frame_created.shooting_frames[0]; input = none; }; }; }
step "Delete vanishing points" { enabled = true; invoke edit_vanishing_points { edits = [{ operation = 4; point_id = none; input = none; }]; }; }
"#;
    let source = complete_source("", &bindings, program_text);
    let program =
        compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let schemas = ScriptSchemas::new();
    let catalog = runtime_catalog(&schemas.commands).unwrap();
    let portability = |command: &str, index: usize| {
        catalog
            .evaluate_portability(command, &program.frozen_arguments[index])
            .unwrap()
    };
    assert_eq!(
        portability("edit_annotations", 0).class,
        InkScriptPortabilityClass::RequiresBinding
    );
    assert_eq!(
        portability("edit_shooting_frame", 1).class,
        InkScriptPortabilityClass::Portable
    );
    assert_eq!(
        portability("edit_shooting_frame", 4).class,
        InkScriptPortabilityClass::RequiresBinding
    );
    assert_eq!(
        portability("edit_annotations", 6).class,
        InkScriptPortabilityClass::StrictSourceOnly
    );
    assert_eq!(
        portability("edit_vanishing_points", 9).class,
        InkScriptPortabilityClass::StrictSourceOnly
    );
    let mut never_cancel = || false;
    let mut scripted =
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut never_cancel).unwrap();
    assert_export_round_trip(&base, &scripted.staged);
    assert_eq!(scripted.report.commit_count, 6);
    assert_eq!(scripted.report.statements.len(), 10);
    assert!(
        scripted.report.statements[3..7]
            .iter()
            .all(|outcome| { *outcome == crate::script::report::ScriptStatementOutcome::NoOp })
    );
    assert!(
        scripted.report.statements[7..].iter().all(|outcome| {
            *outcome == crate::script::report::ScriptStatementOutcome::Committed
        })
    );
    assert_eq!(scripted.report.results.len(), 6);
    assert!(
        scripted
            .report
            .results
            .iter()
            .all(|result| result.alias != "annotation_same")
    );
    assert_eq!(
        scripted
            .report
            .results
            .iter()
            .filter(|result| matches!(result.alias.as_str(), "frame_same" | "vanishing_same"))
            .count(),
        2
    );
    let created_ids = scripted
        .report
        .results
        .iter()
        .filter(|result| result.alias.ends_with("_created"))
        .map(|result| result.persistent_id)
        .collect::<Vec<_>>();
    assert_eq!(
        created_ids,
        (base_next_id..base_next_id + 4).collect::<Vec<_>>()
    );
    assert_eq!(scripted.staged.next_id.next_raw(), base_next_id + 4);
    assert_eq!(scripted.report.next_stable_id, base_next_id + 4);
    assert_eq!(
        scripted
            .report
            .results
            .iter()
            .filter(|result| result.alias == "vanishing_created")
            .count(),
        2
    );

    let mut direct = base.clone();
    let annotation_id = direct
        .execute_canonical_invocation(CanonicalInvocation::EditAnnotations {
            edits: vec![AnnotationEdit::Create(annotation_input(text_layer))],
        })
        .unwrap()
        .output_ids[0];
    let frame_id = direct
        .execute_canonical_invocation(CanonicalInvocation::EditShootingFrame {
            edit: ShootingFrameEdit::Create(shooting_input()),
        })
        .unwrap()
        .output_ids[0];
    let vanishing_ids = direct
        .execute_canonical_invocation(CanonicalInvocation::EditVanishingPoints {
            edits: vec![
                VanishingPointEdit::Create(vanishing_input(vanishing_layer, 3_000)),
                VanishingPointEdit::Create(vanishing_input(vanishing_layer, 12_000)),
            ],
        })
        .unwrap()
        .output_ids;
    direct
        .execute_canonical_invocation(CanonicalInvocation::EditAnnotations {
            edits: vec![AnnotationEdit::Update {
                object_id: annotation_id,
                input: annotation_input(text_layer),
            }],
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::EditShootingFrame {
            edit: ShootingFrameEdit::Update {
                frame_id,
                input: shooting_input(),
            },
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::EditVanishingPoints {
            edits: vec![VanishingPointEdit::Update {
                point_id: vanishing_ids[1],
                input: vanishing_input(vanishing_layer, 12_000),
            }],
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::EditAnnotations {
            edits: vec![AnnotationEdit::Move {
                object_id: annotation_id,
                delta_x: 0,
                delta_y: 0,
            }],
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::EditAnnotations {
            edits: vec![AnnotationEdit::Delete {
                object_id: annotation_id,
            }],
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::EditShootingFrame {
            edit: ShootingFrameEdit::Delete { frame_id },
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::EditVanishingPoints {
            edits: vec![VanishingPointEdit::DeleteAll],
        })
        .unwrap();
    assert_same_document(&scripted.staged, &direct);

    let base_digest = base.document_state_digest().unwrap();
    let final_digest = scripted.staged.document_state_digest().unwrap();
    for _ in 0..scripted.report.commit_count {
        scripted.staged.undo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        base_digest
    );
    for _ in 0..scripted.report.commit_count {
        scripted.staged.redo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        final_digest
    );
    scripted.staged.release_history_cache().unwrap();
    assert_eq!(
        scripted
            .staged
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        final_digest
    );
    let editor_digest = scripted.staged.editor_state().unwrap().digest;
    let native = scripted
        .staged
        .build_procedure_file(Some(scripted.staged.current_state), Some(editor_digest))
        .unwrap();
    let reopened = Core::from_procedure_file(
        decode_procedure_file(&encode_procedure_file(&native).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), final_digest);
    assert_eq!(reopened.next_id, scripted.staged.next_id);
    assert_eq!(reopened.next_procedure, scripted.staged.next_procedure);
    assert_eq!(reopened.next_state, scripted.staged.next_state);
    assert_eq!(reopened.savepoint, Some(reopened.current_state));
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
}

#[test]
fn shooting_frame_document_owner_supports_exact_source_and_semantic_rebound() {
    fn add_frame(core: &mut Core) -> u64 {
        core.execute_canonical_invocation(CanonicalInvocation::EditShootingFrame {
            edit: ShootingFrameEdit::Create(shooting_input()),
        })
        .unwrap()
        .output_ids[0]
    }

    let (mut source_core, _, _, _) = annotation_frame_script_base();
    let frame_id = add_frame(&mut source_core);
    let info = source_core.document_info().unwrap();
    let update = r#"step "Update frame" { enabled = true; invoke edit_shooting_frame { edit = { operation = 2; frame_id = $frame; input = { center_x_milli = 9000; center_y_milli = 8000; width_milli = 10000; height_milli = 6000; rotation_turns = 268435456; anchor = center; visible = true; include_in_instruction_export = true; }; }; }; }"#;
    let strict_source = complete_source(
        "",
        &format!(
            r#"let frame = select shooting_frame {{ source_document_uuid = uuid"{}"; persistent_id = {frame_id}; }};"#,
            document_uuid(info.document_uuid)
        ),
        update,
    );
    let strict = compile_inkscript(
        &strict_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    let strict_result = run_inkscript_dry(
        &strict,
        capture_in_memory_input(&source_core).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        strict_result
            .staged
            .shooting_frame()
            .unwrap()
            .unwrap()
            .center_x_milli,
        9_000
    );

    let (mut rebound_core, _, _, _) =
        annotation_frame_script_base_with_uuid(info.document_uuid + 1);
    add_frame(&mut rebound_core);
    let before = rebound_core.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_dry(
            &strict,
            capture_in_memory_input(&rebound_core).unwrap(),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::Binding(InkScriptBindingError::StalePrecondition)
    );
    assert_eq!(rebound_core.document_state_digest().unwrap(), before);

    let rebound_source = complete_source("", r#"let frame = select shooting_frame {};"#, update);
    let rebound = compile_inkscript(
        &rebound_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    let rebound_result = run_inkscript_dry(
        &rebound,
        capture_in_memory_input(&rebound_core).unwrap(),
        &mut never_cancel,
    )
    .unwrap();
    assert_eq!(
        rebound_result
            .staged
            .shooting_frame()
            .unwrap()
            .unwrap()
            .center_x_milli,
        9_000
    );

    let invalid_owner_source = complete_source(
        "",
        r#"let layer = select layer { name = "Script Text"; }; let frame = select shooting_frame { layer = $layer; };"#,
        update,
    );
    assert!(
        compile_inkscript(
            &invalid_owner_source,
            InkScriptRunParameterDecision::Resolve(Vec::new())
        )
        .is_err()
    );
}

#[test]
fn annotation_frame_invalid_cancel_stale_overflow_and_resource_failures_are_atomic() {
    let (base, text_layer, _, _) = annotation_frame_script_base();
    let info = base.document_info().unwrap();
    let bindings = format!(
        r#"let text_layer = select layer {{ source_document_uuid = uuid"{}"; persistent_id = {text_layer}; }};"#,
        document_uuid(info.document_uuid)
    );
    let annotation_step = |name: &str| {
        format!(
            r#"step "{name}" {{ enabled = true; invoke edit_annotations {{ edits = [{{ operation = 1; object_id = none; input = {{ layer_id = $text_layer; kind = text; output = instruction; bounds = rect(1, 2, 6, 4); font_family_hint = "Inkpod Sans"; font_size_milli = 12000; style_flags = 1; color = rgba8(1, 2, 3, 255); text = "Frame note"; points = []; stroke_width_milli = 0; }}; delta_x = 0; delta_y = 0; }}]; }}; }}"#
        )
    };
    let source = complete_source(
        "",
        &bindings,
        &format!("{} {}", annotation_step("One"), annotation_step("Two")),
    );
    let program =
        compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
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
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut cancel).unwrap_err(),
        ScriptRunError::Cancelled
    );
    let mut polls = 0_u32;
    let mut cancel_after_staging = || {
        polls += 1;
        polls == 3
    };
    assert_eq!(
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut cancel_after_staging)
            .unwrap_err(),
        ScriptRunError::Cancelled
    );

    let invalid_source = complete_source(
        "",
        &bindings,
        &annotation_step("Invalid").replace("rgba8(1, 2, 3, 255)", "gray8(1)"),
    );
    let invalid = compile_inkscript(
        &invalid_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&invalid, base.clone(), None, &mut never_cancel).unwrap_err(),
        ScriptRunError::InvalidStep
    );

    let mut stale = base.clone();
    let fingerprint = capture_in_memory_fingerprint(&stale).unwrap();
    stale.add_guide(GuideAxis::Horizontal, 2).unwrap();
    let stale_digest = stale.document_state_digest().unwrap();
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
    assert_eq!(stale.document_state_digest().unwrap(), stale_digest);

    let allocation_fingerprint = capture_in_memory_fingerprint(&base).unwrap();
    let mut allocation_stale = base.clone();
    allocation_stale.next_id =
        crate::identity::StableIdCursor::from_next_raw(allocation_stale.next_id.next_raw() + 1);
    let allocation_before = allocation_stale.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_dry(
            &program,
            capture_in_memory_input_at(&allocation_stale, allocation_fingerprint),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::StaleInput
    );
    assert_eq!(
        allocation_stale.document_state_digest().unwrap(),
        allocation_before
    );

    let one_source = complete_source("", &bindings, &annotation_step("One"));
    let one = compile_inkscript(
        &one_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut id_overflow = base.clone();
    id_overflow.next_id = crate::identity::StableIdCursor::from_next_raw(MAX_PERSISTENT_NUMERIC_ID);
    let overflow_digest = id_overflow.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&one, id_overflow.clone(), None, &mut never_cancel)
            .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        id_overflow.document_state_digest().unwrap(),
        overflow_digest
    );

    let mut procedure_overflow = base.clone();
    procedure_overflow.next_procedure = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
    let procedure_digest = procedure_overflow.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&one, procedure_overflow.clone(), None, &mut never_cancel)
            .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        procedure_overflow.document_state_digest().unwrap(),
        procedure_digest
    );

    assert_eq!(
        compile_inkscript_with_limits(
            &source,
            InkScriptRunParameterDecision::Resolve(Vec::new()),
            ScriptCompileLimits::exact_current().with_invocations(1),
        ),
        Err(ScriptCompileError::ResourceLimit)
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

fn light_table_properties(
    opacity_milli: u32,
    display_mode: LightTableDisplayMode,
    display_color: [u8; 4],
    transform: (i32, i32, u32, u32, i32),
) -> LightTableItemProperties {
    let (
        translate_x_milli,
        translate_y_milli,
        scale_x_milli,
        scale_y_milli,
        rotation_milli_degrees,
    ) = transform;
    LightTableItemProperties {
        visible: true,
        opacity_milli,
        display_mode,
        display_color: PixelValue::Rgba(display_color),
        translate_x_milli,
        translate_y_milli,
        scale_x_milli,
        scale_y_milli,
        rotation_milli_degrees,
    }
}

fn light_table_item(
    name: &str,
    source_uuid: u128,
    source_revision: u64,
    pixels: [u8; 4],
    properties: LightTableItemProperties,
) -> LightTableItemInput {
    let source = LightTableSource::from_rgba_bytes(
        source_uuid,
        source_revision,
        RectI32 {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        RgbaRasterBytes {
            width: 1,
            height: 1,
            pixel_format: PixelFormat::StraightRgba8,
            dpi_x_milli: Some(DEFAULT_DPI_MILLI),
            dpi_y_milli: Some(DEFAULT_DPI_MILLI),
            pixels: pixels.to_vec(),
        },
    )
    .unwrap();
    LightTableItemInput {
        name: name.to_owned(),
        source,
        visible: properties.visible,
        opacity_milli: properties.opacity_milli,
        display_mode: properties.display_mode,
        display_color: properties.display_color,
        translate_x_milli: properties.translate_x_milli,
        translate_y_milli: properties.translate_y_milli,
        scale_x_milli: properties.scale_x_milli,
        scale_y_milli: properties.scale_y_milli,
        rotation_milli_degrees: properties.rotation_milli_degrees,
    }
}

fn light_table_script_item(
    name: &str,
    source_uuid: u128,
    source_revision: u64,
    asset_symbol: &str,
    properties: LightTableItemProperties,
) -> String {
    let mode = match properties.display_mode {
        LightTableDisplayMode::Color => "color",
        LightTableDisplayMode::Monotone => "monotone",
        LightTableDisplayMode::Halftone => "halftone",
    };
    let PixelValue::Rgba(color) = properties.display_color else {
        panic!("fixture light-table color must be rgba8");
    };
    format!(
        r#"{{ name = "{name}"; source = {{ document_uuid = uuid"{}"; source_revision = {source_revision}; reference_frame = rect(0, 0, 1, 1); dpi_x_milli = {DEFAULT_DPI_MILLI}; dpi_y_milli = {DEFAULT_DPI_MILLI}; raster = asset({asset_symbol}); }}; properties = {{ visible = {}; opacity_milli = {}; display_mode = {mode}; display_color = rgba8({}, {}, {}, {}); translate_x_milli = {}; translate_y_milli = {}; scale_x_milli = {}; scale_y_milli = {}; rotation_milli_degrees = {}; }}; }}"#,
        document_uuid(source_uuid),
        properties.visible,
        properties.opacity_milli,
        color[0],
        color[1],
        color[2],
        color[3],
        properties.translate_x_milli,
        properties.translate_y_milli,
        properties.scale_x_milli,
        properties.scale_y_milli,
        properties.rotation_milli_degrees,
    )
}

fn light_table_asset(symbol: &str, pixels: [u8; 4], base64: &str) -> String {
    let id = rgba8_asset_id(pixels.to_vec(), 1, 1);
    format!(
        r#"asset {symbol} {{ asset_id = blake3"{}"; kind = "canonical_raster"; descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 4; element_count = 1; }}; data = base64"""{base64}"""; }};"#,
        asset_digest_text(id)
    )
}

fn light_table_script_fixture() -> (
    Core,
    StaticScriptProgram,
    FrozenScriptAssets,
    Vec<LightTableItemInput>,
    u64,
    u64,
) {
    let mut base = Core::new();
    base.new_cell_with_uuid(
        8,
        8,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        0x4d32_324c_4947_4854,
    )
    .unwrap();
    let info = base.document_info().unwrap();
    let default_set_id = base.light_table_sets().unwrap()[0].id;
    let first = light_table_properties(
        900,
        LightTableDisplayMode::Color,
        [1, 2, 3, 255],
        (10, -20, 1_000, 1_100, 500),
    );
    let second = light_table_properties(
        800,
        LightTableDisplayMode::Monotone,
        [4, 5, 6, 255],
        (-30, 40, 900, 1_200, -1_000),
    );
    let third = light_table_properties(
        700,
        LightTableDisplayMode::Halftone,
        [7, 8, 9, 255],
        (50, 60, 1_300, 800, 2_000),
    );
    let (_, initial_item_id) = base
        .light_table_add_item(light_table_item("Initial", 0x91, 1, [5, 6, 7, 255], first))
        .unwrap();
    let input_a = light_table_script_item("Added A", 0xa1, 1, "lt_a", first);
    let input_b = light_table_script_item("Updated B", 0xb2, 2, "lt_b", third);
    let input_c = light_table_script_item("Bulk C", 0xc3, 3, "lt_c", first);
    let input_d = light_table_script_item("Bulk D", 0xd4, 4, "lt_d", second);
    let properties_second = r#"{ visible = true; opacity_milli = 800; display_mode = monotone; display_color = rgba8(4, 5, 6, 255); translate_x_milli = -30; translate_y_milli = 40; scale_x_milli = 900; scale_y_milli = 1200; rotation_milli_degrees = -1000; }"#;
    let program = format!(
        r#"
step "Initial item no-op" {{ enabled = true; invoke light_table_update_item_properties {{ item_id = $initial_item; properties = {{ visible = true; opacity_milli = 900; display_mode = color; display_color = rgba8(1, 2, 3, 255); translate_x_milli = 10; translate_y_milli = -20; scale_x_milli = 1000; scale_y_milli = 1100; rotation_milli_degrees = 500; }}; }}; }}
step "Initial active no-op" {{ enabled = true; invoke light_table_set_active {{ set_id = $default_set; }}; }}
step "Opacity" {{ enabled = true; invoke light_table_set_global_opacity {{ opacity_milli = 850; }}; }}
step "Opacity no-op" {{ enabled = true; invoke light_table_set_global_opacity {{ opacity_milli = 850; }}; }}
step "Create set" as created {{ enabled = true; invoke light_table_create_set {{ name = "Script Set"; }}; }}
step "Add item" as added {{ enabled = true; invoke light_table_add_item {{ input = {input_a}; }}; }}
step "Properties" {{ enabled = true; invoke light_table_update_item_properties {{ item_id = $added.item; properties = {properties_second}; }}; }}
step "Properties no-op" {{ enabled = true; invoke light_table_update_item_properties {{ item_id = $added.item; properties = {properties_second}; }}; }}
step "Update item" {{ enabled = true; invoke light_table_update_item {{ item_id = $added.item; input = {input_b}; }}; }}
step "Bulk" as bulk {{ enabled = true; invoke light_table_bulk_register {{ target_set_id = $created.set; inputs = [{input_c}, {input_d}]; }}; }}
step "Reorder item" {{ enabled = true; invoke light_table_reorder_item {{ item_id = $added.item; destination_index = 0; }}; }}
step "Duplicate set" as duplicate {{ enabled = true; invoke light_table_duplicate_set {{ set_id = $created.set; }}; }}
step "Rename duplicate" {{ enabled = true; invoke light_table_rename_set {{ set_id = $duplicate.set; name = "Script Copy"; }}; }}
step "Reorder duplicate" {{ enabled = true; invoke light_table_reorder_set {{ set_id = $duplicate.set; destination_index = 0; }}; }}
step "Activate created" {{ enabled = true; invoke light_table_set_active {{ set_id = $created.set; }}; }}
step "Remove added" {{ enabled = true; invoke light_table_remove_item {{ item_id = $added.item; }}; }}
step "Delete duplicate" {{ enabled = true; invoke light_table_delete_set {{ set_id = $duplicate.set; }}; }}
"#
    );
    let assets_source = [
        light_table_asset("lt_a", [10, 20, 30, 255], "ChQe/w=="),
        light_table_asset("lt_b", [40, 50, 60, 255], "KDI8/w=="),
        light_table_asset("lt_c", [70, 80, 90, 255], "RlBa/w=="),
        light_table_asset("lt_d", [100, 110, 120, 255], "ZG54/w=="),
    ]
    .join("\n");
    let source = complete_source_with_assets(
        &format!(
            r#"let default_set = select light_table_set {{ source_document_uuid = uuid"{}"; persistent_id = {default_set_id}; }}; let initial_item = select light_table_item {{ set = $default_set; source_document_uuid = uuid"{}"; persistent_id = {initial_item_id}; }};"#,
            document_uuid(info.document_uuid),
            document_uuid(info.document_uuid)
        ),
        &program,
        &assets_source,
    );
    let program =
        compile_inkscript(&source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    let mut never_cancel = || false;
    let assets = freeze_inkscript_assets(
        program.model.assets(),
        &mut [],
        ScriptAssetLimits::exact_current(),
        &mut never_cancel,
    )
    .unwrap();
    let direct_inputs = vec![
        light_table_item("Added A", 0xa1, 1, [10, 20, 30, 255], first),
        light_table_item("Updated B", 0xb2, 2, [40, 50, 60, 255], third),
        light_table_item("Bulk C", 0xc3, 3, [70, 80, 90, 255], first),
        light_table_item("Bulk D", 0xd4, 4, [100, 110, 120, 255], second),
    ];
    (
        base,
        program,
        assets,
        direct_inputs,
        default_set_id,
        initial_item_id,
    )
}

#[test]
fn light_table_catalog_results_assets_direct_replay_and_native_reopen_are_exact() {
    let (base, program, assets, inputs, default_set_id, initial_item_id) =
        light_table_script_fixture();
    let base_digest = base.document_state_digest().unwrap();
    let base_next_id = base.next_id.next_raw();
    assert_eq!(program.budget.max_invocations, 17);
    assert_eq!(program.budget.max_output_ids, 5);
    assert_eq!(program.budget.max_asset_bytes, 16);
    assert_eq!(assets.usage().logical_payload_bytes, 16);

    let mut never_cancel = || false;
    let mut scripted =
        run_inkscript_on_staged_core(&program, base.clone(), Some(&assets), &mut never_cancel)
            .unwrap();
    assert_export_round_trip(&base, &scripted.staged);
    assert_eq!(scripted.report.commit_count, 13);
    assert_eq!(scripted.report.statements.len(), 17);
    assert_eq!(
        scripted
            .report
            .statements
            .iter()
            .filter(|outcome| { **outcome == crate::script::report::ScriptStatementOutcome::NoOp })
            .count(),
        4
    );
    assert_eq!(scripted.report.results.len(), 5);
    assert_eq!(
        scripted
            .report
            .results
            .iter()
            .map(|result| (result.alias.as_str(), result.field.as_str()))
            .collect::<Vec<_>>(),
        [
            ("created", "set"),
            ("added", "item"),
            ("bulk", "items"),
            ("bulk", "items"),
            ("duplicate", "set"),
        ]
    );

    let mut direct = base.clone();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableUpdateItemProperties {
            item_id: initial_item_id,
            properties: light_table_properties(
                900,
                LightTableDisplayMode::Color,
                [1, 2, 3, 255],
                (10, -20, 1_000, 1_100, 500),
            ),
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableSetActive {
            set_id: default_set_id,
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableSetGlobalOpacity {
            opacity_milli: 850,
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableSetGlobalOpacity {
            opacity_milli: 850,
        })
        .unwrap();
    let created_set = direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableCreateSet {
            name: "Script Set".to_owned(),
        })
        .unwrap()
        .output_ids[0];
    let added_item = direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableAddItem {
            input: inputs[0].clone(),
        })
        .unwrap()
        .output_ids[0];
    let second = light_table_properties(
        800,
        LightTableDisplayMode::Monotone,
        [4, 5, 6, 255],
        (-30, 40, 900, 1_200, -1_000),
    );
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableUpdateItemProperties {
            item_id: added_item,
            properties: second,
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableUpdateItemProperties {
            item_id: added_item,
            properties: second,
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableUpdateItem {
            item_id: added_item,
            input: inputs[1].clone(),
        })
        .unwrap();
    let bulk_items = direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableBulkRegister {
            target_set_id: created_set,
            inputs: inputs[2..].to_vec(),
        })
        .unwrap()
        .output_ids;
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableReorderItem {
            item_id: added_item,
            destination_index: 0,
        })
        .unwrap();
    let duplicate_set = direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableDuplicateSet {
            set_id: created_set,
        })
        .unwrap()
        .output_ids[0];
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableRenameSet {
            set_id: duplicate_set,
            name: "Script Copy".to_owned(),
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableReorderSet {
            set_id: duplicate_set,
            destination_index: 0,
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableSetActive {
            set_id: created_set,
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableRemoveItem {
            item_id: added_item,
        })
        .unwrap();
    direct
        .execute_canonical_invocation(CanonicalInvocation::LightTableDeleteSet {
            set_id: duplicate_set,
        })
        .unwrap();
    assert_eq!(
        scripted
            .report
            .results
            .iter()
            .map(|result| result.persistent_id)
            .collect::<Vec<_>>(),
        [
            created_set,
            added_item,
            bulk_items[0],
            bulk_items[1],
            duplicate_set,
        ]
    );
    assert_eq!(scripted.staged.next_id.next_raw(), base_next_id + 14);
    assert_eq!(scripted.report.next_stable_id, base_next_id + 14);
    assert_same_document(&scripted.staged, &direct);
    assert_eq!(scripted.staged.light_table_items().unwrap().len(), 2);

    let final_digest = scripted.staged.document_state_digest().unwrap();
    for _ in 0..scripted.report.commit_count {
        scripted.staged.undo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        base_digest
    );
    for _ in 0..scripted.report.commit_count {
        scripted.staged.redo().unwrap();
    }
    assert_eq!(
        scripted.staged.document_state_digest().unwrap(),
        final_digest
    );
    scripted.staged.release_history_cache().unwrap();
    assert_eq!(
        scripted
            .staged
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        final_digest
    );

    let editor_digest = scripted.staged.editor_state().unwrap().digest;
    let native = scripted
        .staged
        .build_procedure_file(Some(scripted.staged.current_state), Some(editor_digest))
        .unwrap();
    let mut reopened = Core::from_procedure_file(
        decode_procedure_file(&encode_procedure_file(&native).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), final_digest);
    assert_eq!(reopened.next_id, scripted.staged.next_id);
    assert_eq!(reopened.next_procedure, scripted.staged.next_procedure);
    assert_eq!(reopened.next_state, scripted.staged.next_state);
    assert_eq!(reopened.savepoint, Some(reopened.current_state));
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert!(!reopened.document_info().unwrap().dirty);
    assert!(!reopened.editor_state().unwrap().dirty);
    for _ in 0..scripted.report.commit_count {
        reopened.undo().unwrap();
    }
    assert_eq!(reopened.document_state_digest().unwrap(), base_digest);
    for _ in 0..scripted.report.commit_count {
        reopened.redo().unwrap();
    }
    assert_eq!(reopened.document_state_digest().unwrap(), final_digest);
}

#[test]
fn light_table_invalid_cancel_stale_overflow_resource_and_asset_failures_are_atomic() {
    let (base, program, assets, _, _, _) = light_table_script_fixture();
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
    let mut polls = 0_u32;
    let mut cancel_after_staging = || {
        polls += 1;
        polls == 5
    };
    assert_eq!(
        run_inkscript_on_staged_core(
            &program,
            base.clone(),
            Some(&assets),
            &mut cancel_after_staging,
        )
        .unwrap_err(),
        ScriptRunError::Cancelled
    );
    let invalid = compile_inkscript(
        &complete_source(
            "",
            "",
            r#"step "Invalid opacity" { enabled = true; invoke light_table_set_global_opacity { opacity_milli = 1001; }; }"#,
        ),
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&invalid, base.clone(), None, &mut never_cancel).unwrap_err(),
        ScriptRunError::InvalidStep
    );
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&program, base.clone(), None, &mut never_cancel).unwrap_err(),
        ScriptRunError::InvalidStep
    );

    let mut stale = base.clone();
    let fingerprint = capture_in_memory_fingerprint(&stale).unwrap();
    stale.add_guide(GuideAxis::Vertical, 3).unwrap();
    let stale_digest = stale.document_state_digest().unwrap();
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
    assert_eq!(stale.document_state_digest().unwrap(), stale_digest);

    let create = compile_inkscript(
        &complete_source(
            "",
            "",
            r#"step "Create" { enabled = true; invoke light_table_create_set { name = "Overflow"; }; }"#,
        ),
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut id_overflow = base.clone();
    id_overflow.next_id = crate::identity::StableIdCursor::from_next_raw(MAX_PERSISTENT_NUMERIC_ID);
    let id_digest = id_overflow.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&create, id_overflow.clone(), None, &mut never_cancel)
            .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(id_overflow.document_state_digest().unwrap(), id_digest);
    let mut procedure_overflow = base.clone();
    procedure_overflow.next_procedure = ProcedureId::from_raw(MAX_PERSISTENT_NUMERIC_ID);
    let procedure_digest = procedure_overflow.document_state_digest().unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(&create, procedure_overflow.clone(), None, &mut never_cancel)
            .unwrap_err(),
        ScriptRunError::ResourceLimit
    );
    assert_eq!(
        procedure_overflow.document_state_digest().unwrap(),
        procedure_digest
    );
    assert_eq!(
        compile_inkscript_with_limits(
            &complete_source(
                "",
                "",
                r#"step "One" { enabled = true; invoke light_table_set_global_opacity { opacity_milli = 900; }; } step "Two" { enabled = true; invoke light_table_set_global_opacity { opacity_milli = 800; }; }"#,
            ),
            InkScriptRunParameterDecision::Resolve(Vec::new()),
            ScriptCompileLimits::exact_current().with_invocations(1),
        ),
        Err(ScriptCompileError::ResourceLimit)
    );

    let gray_id = gray8_asset_id(vec![1], 1, 1);
    let gray_source = complete_source_with_assets(
        "",
        &format!(
            r#"step "Gray" {{ enabled = true; invoke light_table_add_item {{ input = {{ name = "Gray"; source = {{ document_uuid = uuid"{}"; source_revision = 1; reference_frame = rect(0, 0, 1, 1); dpi_x_milli = {DEFAULT_DPI_MILLI}; dpi_y_milli = {DEFAULT_DPI_MILLI}; raster = asset(gray); }}; properties = {{ visible = true; opacity_milli = 1000; display_mode = color; display_color = rgba8(0, 0, 0, 255); translate_x_milli = 0; translate_y_milli = 0; scale_x_milli = 1000; scale_y_milli = 1000; rotation_milli_degrees = 0; }}; }}; }}; }}"#,
            document_uuid(0xee)
        ),
        &format!(
            r#"asset gray {{ asset_id = blake3"{}"; kind = "canonical_raster"; descriptor = {{ pixel_format = gray8; color_space = srgb; alpha = straight; width = 1; height = 1; stride = 1; element_count = 1; }}; data = base64"""AQ=="""; }};"#,
            asset_digest_text(gray_id)
        ),
    );
    let gray_program = compile_inkscript(
        &gray_source,
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let mut never_cancel = || false;
    let gray_assets = freeze_inkscript_assets(
        gray_program.model.assets(),
        &mut [],
        ScriptAssetLimits::exact_current(),
        &mut never_cancel,
    )
    .unwrap();
    let mut never_cancel = || false;
    assert_eq!(
        run_inkscript_on_staged_core(
            &gray_program,
            base.clone(),
            Some(&gray_assets),
            &mut never_cancel,
        )
        .unwrap_err(),
        ScriptRunError::InvalidStep
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
