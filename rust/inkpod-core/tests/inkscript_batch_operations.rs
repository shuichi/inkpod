use inkpod_core::inkscript::{
    InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId, ScriptDryRunResult,
    ScriptRunError, ScriptStatementOutcome, capture_in_memory_fingerprint, capture_in_memory_input,
    capture_in_memory_input_at, compile_inkscript, run_inkscript_dry,
};
use inkpod_core::*;
use inkpod_format::{
    INKSCRIPT_FILE_VERSION, INKSCRIPT_PROCEDURE_CATALOG_VERSION, INKSCRIPT_REQUIRED_REPLAY_EPOCH,
    decode_procedure_file, encode_procedure_file,
};

fn source(bindings: &str, steps: &str) -> InkScriptSource {
    InkScriptSource::new(
        InkScriptSourceId::new(3000),
        format!(
            r#"inkscript {INKSCRIPT_FILE_VERSION};
requires {{ procedure_catalog = {INKSCRIPT_PROCEDURE_CATALOG_VERSION}; replay_epoch = {INKSCRIPT_REQUIRED_REPLAY_EPOCH}; }}
inputs {{ current_document; }}
{bindings}
program {{ {steps} }}
output {{ policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "batch"; start_number = 1; direction = ascending; }}
execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"#,
        )
        .as_bytes(),
    )
    .unwrap()
}

fn step(operations: &str) -> String {
    format!(
        "step \"Batch\" {{ enabled = true; invoke apply_batch_operations {{ operations = [{operations}]; }}; }}"
    )
}

fn role(kind: &str, missing: &str) -> String {
    format!("{{ kind = role; plane_kind = {kind}; missing = {missing}; }}")
}

fn strict(uuid: u128, layer: Option<u64>, plane: u64, missing: &str) -> String {
    let digits = format!("{uuid:032x}");
    let uuid = format!(
        "{}-{}-{}-{}-{}",
        &digits[..8],
        &digits[8..12],
        &digits[12..16],
        &digits[16..20],
        &digits[20..]
    );
    let layer = layer.map_or_else(|| "none".to_owned(), |id| id.to_string());
    format!(
        "{{ kind = strict; source_document_uuid = uuid\"{uuid}\"; persistent_layer_id = {layer}; persistent_plane_id = {plane}; plane_kind = none; missing = {missing}; }}"
    )
}

fn unary(kind: &str, target: &str, color: &str) -> String {
    format!("{{ kind = {kind}; enabled = true; target = {target}; colors = [{color}]; }}")
}

fn replace(targets: &[String], old: &str, new: &str, enabled: bool) -> String {
    format!(
        "{{ kind = color_replace; enabled = true; targets = [{}]; pairs = [{{ enabled = {enabled}; old = {old}; new = {new}; }}]; }}",
        targets.join(",")
    )
}

fn color(depth16: bool, rgba: [u8; 4]) -> PixelValue {
    if depth16 {
        PixelValue::Rgba16(rgba.map(|value| u16::from(value) * 257))
    } else {
        PixelValue::Rgba(rgba)
    }
}

fn literal(depth16: bool, rgba: [u8; 4]) -> String {
    let values = rgba.map(|value| u16::from(value) * if depth16 { 257 } else { 1 });
    format!(
        "rgba{}({},{},{},{})",
        if depth16 { 16 } else { 8 },
        values[0],
        values[1],
        values[2],
        values[3]
    )
}

fn fill(core: &mut Core, target: EditorTarget, x: u32, value: PixelValue) {
    core.apply_fill_for_editor_target(
        &FillRequest {
            operation: FillOperation::Seed,
            seed_x: x,
            seed_y: 0,
            color: value,
            selection: Some(RectI32 {
                x: x as i32,
                y: 0,
                width: 1,
                height: 1,
            }),
            use_document_selection: false,
            tolerance: 0,
            detached_regions: false,
            overflow_abort: false,
            gap_close: 0,
            transparent_only: false,
            inclusion_mode: InclusionMode::None,
            inclusion_colors: Vec::new(),
            extension_distance: 0,
        },
        target,
        false,
        false,
    )
    .unwrap();
}

fn fixture(depth16: bool) -> (Core, EditorTarget) {
    let mut core = Core::new();
    let info = core
        .new_cell_with_uuid(3, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xba730)
        .unwrap();
    let format = if depth16 {
        PixelFormat::StraightRgba16
    } else {
        PixelFormat::StraightRgba8
    };
    core.convert_plane(info.color_plane_id, format).unwrap();
    let (_, plane_id) = core.create_plane(info.layer_id, format, "Raster").unwrap();
    let target = EditorTarget {
        layer_id: info.layer_id,
        plane_id,
    };
    for (x, rgba) in [[255, 0, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]]
        .into_iter()
        .enumerate()
    {
        fill(&mut core, target, x as u32, color(depth16, rgba));
    }
    (core, target)
}

fn pixels(core: &Core, target: EditorTarget, depth16: bool) -> Vec<PixelValue> {
    // Clipboard is the production native-depth read path for an arbitrary plane.
    let mut copy = core.clone();
    copy.set_active_node(target.layer_id, target.plane_id)
        .unwrap();
    let width = copy.document_info().unwrap().width;
    copy.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 0,
            y: 0,
            width: width as i32,
            height: 1,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    let payload = copy.copy_selection().unwrap();
    assert_eq!(payload.planes.len(), 1);
    let mut values = vec![color(depth16, [0; 4]); width as usize];
    for pixel in &payload.planes[0].pixels {
        values[pixel.x as usize] = pixel.value;
    }
    values
}

fn direct_operation(kind: PlaneType, operation: BatchOperationKind) -> BatchOperation {
    BatchOperation {
        version: BATCH_OPERATION_VERSION,
        enabled: true,
        target: BatchTargetSelector {
            layer_id: None,
            plane_id: None,
            plane_kind: Some(kind),
            missing_policy: BatchMissingTargetPolicy::Error,
        },
        additional_targets: Vec::new(),
        kind: operation,
    }
}

fn four_operations(depth16: bool) -> (Vec<BatchOperation>, String) {
    let red = [255, 0, 0, 255];
    let green = [0, 255, 0, 255];
    let blue = [0, 0, 255, 255];
    let mut replace_direct = direct_operation(
        PlaneType::Raster,
        BatchOperationKind::ColorReplace(vec![BatchColorPair {
            enabled: true,
            old: color(depth16, red),
            new: color(depth16, green),
        }]),
    );
    replace_direct
        .additional_targets
        .push(BatchTargetSelector::color_plane());
    let direct = vec![
        replace_direct,
        direct_operation(
            PlaneType::Raster,
            BatchOperationKind::MoveToColorPlane(vec![color(depth16, green)]),
        ),
        direct_operation(
            PlaneType::Color,
            BatchOperationKind::Masking(vec![color(depth16, green)]),
        ),
        direct_operation(
            PlaneType::Raster,
            BatchOperationKind::Erase(vec![color(depth16, blue)]),
        ),
    ];
    let text = [
        replace(
            &[role("raster", "error"), role("color", "error")],
            &literal(depth16, red),
            &literal(depth16, green),
            true,
        ),
        unary(
            "move_to_color_plane",
            &role("raster", "error"),
            &literal(depth16, green),
        ),
        unary("masking", &role("color", "error"), &literal(depth16, green)),
        unary("erase", &role("raster", "error"), &literal(depth16, blue)),
    ]
    .join(",");
    (direct, text)
}

fn run(base: &Core, source: &InkScriptSource) -> Result<ScriptDryRunResult, ScriptRunError> {
    let program =
        compile_inkscript(source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    run_inkscript_dry(
        &program,
        capture_in_memory_input(base).unwrap(),
        &mut || false,
    )
}

fn assert_unchanged(actual: &Core, before: &Core) {
    assert_eq!(
        capture_in_memory_fingerprint(actual).unwrap(),
        capture_in_memory_fingerprint(before).unwrap()
    );
    assert_eq!(
        actual.document_info().unwrap(),
        before.document_info().unwrap()
    );
    assert_eq!(
        actual.document_state_digest().unwrap(),
        before.document_state_digest().unwrap()
    );
    assert_eq!(actual.journal_entries(), before.journal_entries());
    assert_eq!(
        actual.journal_state().unwrap(),
        before.journal_state().unwrap()
    );
    assert_eq!(
        actual.editor_state().unwrap(),
        before.editor_state().unwrap()
    );
    assert_eq!(
        actual.persistence_info().unwrap(),
        before.persistence_info().unwrap()
    );
    assert_eq!(actual.resource_usage(), before.resource_usage());
}

#[test]
fn four_operations_match_independent_rgba8_and_rgba16_golden_as_one_undo() {
    for depth16 in [false, true] {
        let (base, raster) = fixture(depth16);
        let before = base.clone();
        let (operations, text) = four_operations(depth16);
        let mut direct = base.clone();
        direct
            .apply_batch_operations(&operations, || false)
            .unwrap();
        let mut result = run(&base, &source("", &step(&text))).unwrap();
        assert_unchanged(&base, &before);
        assert_eq!(result.report().commit_count(), 1);
        assert_eq!(
            result.report().statements(),
            &[ScriptStatementOutcome::Committed]
        );
        assert_eq!(
            result.staged().history_entries().len(),
            base.history_entries().len() + 1
        );
        assert_eq!(
            result.staged().document_info().unwrap().document_revision,
            base.document_info().unwrap().document_revision + 1
        );
        assert_eq!(
            result.staged().document_state_digest().unwrap(),
            direct.document_state_digest().unwrap()
        );
        assert_eq!(result.staged().journal_entries(), direct.journal_entries());
        assert_eq!(
            pixels(result.staged(), raster, depth16),
            vec![
                color(depth16, [0; 4]),
                color(depth16, [0; 4]),
                color(depth16, [255, 255, 0, 255])
            ]
        );
        let info = base.document_info().unwrap();
        assert_eq!(
            (0..3)
                .map(|x| result
                    .staged()
                    .plane_pixel(ActivePlane::Color, x, 0)
                    .unwrap())
                .collect::<Vec<_>>(),
            vec![
                color(depth16, [0, 255, 0, 255]),
                color(depth16, [0; 4]),
                color(depth16, [0; 4])
            ]
        );
        assert_eq!(
            result.staged().document_info().unwrap().main_plane_checksum,
            info.main_plane_checksum
        );
        assert_eq!(
            result
                .staged()
                .fill_protection_mask_info()
                .unwrap()
                .wall_pixel_count,
            1
        );
        // Independent mask golden [255,0,0]: exact point fills must be blocked
        // only at x=0. Selection and source pixels are separate from protection.
        for x in 0..3 {
            let mut probe = result.staged().clone();
            let target = EditorTarget {
                layer_id: info.layer_id,
                plane_id: info.color_plane_id,
            };
            fill(&mut probe, target, x, color(depth16, [99, 88, 77, 255]));
            assert_eq!(
                probe.plane_pixel(ActivePlane::Color, x, 0).unwrap(),
                if x == 0 {
                    color(depth16, [0, 255, 0, 255])
                } else {
                    color(depth16, [99, 88, 77, 255])
                }
            );
        }
        let after = result.staged().document_state_digest().unwrap();
        result.staged_mut().undo().unwrap();
        assert_eq!(
            result.staged().document_state_digest().unwrap(),
            base.document_state_digest().unwrap()
        );
        result.staged_mut().redo().unwrap();
        assert_eq!(result.staged().document_state_digest().unwrap(), after);
        result.staged_mut().release_history_cache().unwrap();
        assert_eq!(
            result
                .staged()
                .verify_journal_replay()
                .unwrap()
                .document_state_digest(),
            after
        );
        let (native, _) = result
            .staged()
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap();
        let mut reopened = Core::from_native_file(
            decode_procedure_file(&encode_procedure_file(&native).unwrap()).unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(reopened.document_state_digest().unwrap(), after);
        reopened.release_history_cache().unwrap();
        assert_eq!(
            reopened
                .verify_journal_replay()
                .unwrap()
                .document_state_digest(),
            after
        );
        reopened.undo().unwrap();
        assert_eq!(
            reopened.document_state_digest().unwrap(),
            base.document_state_digest().unwrap()
        );
        reopened.redo().unwrap();
        assert_eq!(reopened.document_state_digest().unwrap(), after);
    }
}

#[test]
fn each_operation_matches_direct_execution_and_preserves_unrelated_state() {
    for depth16 in [false, true] {
        let (base, _) = fixture(depth16);
        let (operations, _) = four_operations(depth16);
        let targets = ["raster", "raster", "raster", "raster"];
        let kinds = ["color_replace", "move_to_color_plane", "masking", "erase"];
        for index in 0..4 {
            let mut operation = operations[index].clone();
            let red = literal(depth16, [255, 0, 0, 255]);
            let text = if index == 0 {
                replace(
                    &[role("raster", "error"), role("color", "error")],
                    &red,
                    &literal(depth16, [0, 255, 0, 255]),
                    true,
                )
            } else {
                operation = direct_operation(
                    PlaneType::Raster,
                    match index {
                        1 => BatchOperationKind::MoveToColorPlane(vec![color(
                            depth16,
                            [255, 0, 0, 255],
                        )]),
                        2 => BatchOperationKind::Masking(vec![color(depth16, [255, 0, 0, 255])]),
                        _ => BatchOperationKind::Erase(vec![color(depth16, [255, 0, 0, 255])]),
                    },
                );
                unary(kinds[index], &role(targets[index], "error"), &red)
            };
            let mut direct = base.clone();
            direct
                .apply_batch_operations(&[operation], || false)
                .unwrap();
            let result = run(&base, &source("", &step(&text))).unwrap();
            assert_eq!(
                result.staged().document_state_digest().unwrap(),
                direct.document_state_digest().unwrap()
            );
            assert_eq!(result.staged().journal_entries(), direct.journal_entries());
        }
    }
}

#[test]
fn replacement_expands_all_layers_but_other_operations_use_only_the_first_plane() {
    let (mut base, first) = fixture(false);
    let (_, second_layer) = base.create_layer("Second").unwrap();
    let (_, second_plane) = base
        .create_plane(second_layer, PixelFormat::StraightRgba8, "Second raster")
        .unwrap();
    let second = EditorTarget {
        layer_id: second_layer,
        plane_id: second_plane,
    };
    for x in 0..3 {
        fill(&mut base, second, x, PixelValue::Rgba([255, 0, 0, 255]));
    }
    let replace = replace(
        &[role("raster", "error")],
        "rgba8(255,0,0,255)",
        "rgba8(0,255,0,255)",
        true,
    );
    let result = run(&base, &source("", &step(&replace))).unwrap();
    assert_eq!(
        pixels(result.staged(), first, false)[0],
        PixelValue::Rgba([0, 255, 0, 255])
    );
    assert_eq!(
        pixels(result.staged(), second, false),
        vec![PixelValue::Rgba([0, 255, 0, 255]); 3]
    );
    for kind in ["move_to_color_plane", "masking", "erase"] {
        let operation = unary(kind, &role("raster", "error"), "rgba8(255,0,0,255)");
        let result = run(&base, &source("", &step(&operation))).unwrap();
        assert_eq!(
            pixels(result.staged(), second, false),
            vec![PixelValue::Rgba([255, 0, 0, 255]); 3]
        );
        assert_eq!(
            pixels(result.staged(), first, false)[0],
            PixelValue::Rgba(if kind == "masking" {
                [255, 0, 0, 255]
            } else {
                [0; 4]
            }),
        );
        if kind == "masking" {
            assert_eq!(
                result
                    .staged()
                    .fill_protection_mask_info()
                    .unwrap()
                    .wall_pixel_count,
                1
            );
        }
    }
}

#[test]
fn noop_disabled_pairs_and_all_skipped_targets_do_not_publish_ids_or_commits() {
    let (base, _) = fixture(false);
    let info = base.document_info().unwrap();
    let texts = [
        unary("erase", &role("raster", "error"), "rgba8(1,2,3,255)"),
        replace(
            &[role("raster", "error")],
            "rgba8(255,0,0,255)",
            "rgba8(1,2,3,255)",
            false,
        ),
        unary(
            "erase",
            &strict(info.document_uuid, None, u64::MAX, "skip"),
            "rgba8(255,0,0,255)",
        ),
    ];
    for text in texts {
        let result = run(&base, &source("", &step(&text))).unwrap();
        assert_eq!(result.report().commit_count(), 0);
        assert_eq!(
            result.report().statements(),
            &[ScriptStatementOutcome::NoOp]
        );
        assert_unchanged(result.staged(), &base);
    }
    let disabled = unary("erase", &role("raster", "error"), "rgba8(255,0,0,255)").replacen(
        "enabled = true",
        "enabled = false",
        1,
    );
    assert!(
        compile_inkscript(
            &source("", &step(&disabled)),
            InkScriptRunParameterDecision::Resolve(Vec::new())
        )
        .is_err()
    );
}

#[test]
fn strict_missing_skip_omits_only_that_target_but_never_bypasses_uuid_owner_or_mainline() {
    let (base, raster) = fixture(false);
    let info = base.document_info().unwrap();
    let erase = unary("erase", &role("raster", "error"), "rgba8(255,0,0,255)");
    let missing = unary(
        "erase",
        &strict(info.document_uuid, None, u64::MAX, "skip"),
        "rgba8(255,0,0,255)",
    );
    let result = run(&base, &source("", &step(&format!("{missing},{erase}")))).unwrap();
    assert_eq!(result.report().commit_count(), 1);
    for invalid in [
        strict(
            info.document_uuid + 1,
            Some(info.layer_id),
            raster.plane_id,
            "skip",
        ),
        strict(info.document_uuid, Some(u64::MAX), raster.plane_id, "skip"),
        strict(
            info.document_uuid,
            Some(info.layer_id),
            info.main_plane_id,
            "skip",
        ),
        strict(info.document_uuid, None, u64::MAX, "error"),
    ] {
        let text = format!("{erase},{}", unary("erase", &invalid, "rgba8(255,0,0,255)"));
        assert!(run(&base, &source("", &step(&text))).is_err(), "{invalid}");
    }
}

#[test]
fn hidden_noneditable_depth_and_move_destination_errors_are_atomic() {
    let (base, raster) = fixture(false);
    let (_, text) = four_operations(false);
    for (plane_id, visible, editable) in [
        (raster.plane_id, false, true),
        (raster.plane_id, true, false),
        (base.document_info().unwrap().color_plane_id, false, true),
        (base.document_info().unwrap().color_plane_id, true, false),
    ] {
        let mut invalid = base.clone();
        invalid
            .set_plane_properties(plane_id, visible, editable, 1000, "Target")
            .unwrap();
        let before = invalid.clone();
        assert!(run(&invalid, &source("", &step(&text))).is_err());
        assert_unchanged(&invalid, &before);
        let disabled_pairs = replace(
            &[role("raster", "error")],
            "rgba8(255,0,0,255)",
            "rgba8(1,2,3,255)",
            false,
        );
        if plane_id == raster.plane_id {
            assert!(run(&invalid, &source("", &step(&disabled_pairs))).is_err());
        }
    }
    let wrong_depth = format!(
        "{},{}",
        unary("erase", &role("raster", "error"), "rgba8(255,0,0,255)"),
        unary("erase", &role("raster", "error"), "rgba16(0,0,65535,65535)")
    );
    assert!(run(&base, &source("", &step(&wrong_depth))).is_err());
    let mut mismatch = base.clone();
    mismatch
        .convert_plane(
            mismatch.document_info().unwrap().color_plane_id,
            PixelFormat::StraightRgba16,
        )
        .unwrap();
    assert!(
        run(
            &mismatch,
            &source(
                "",
                &step(&unary(
                    "move_to_color_plane",
                    &role("raster", "error"),
                    "rgba8(255,0,0,255)"
                ))
            )
        )
        .is_err()
    );
    assert!(
        run(
            &base,
            &source(
                "",
                &step(&unary(
                    "move_to_color_plane",
                    &role("color", "error"),
                    "rgba8(0,0,0,0)"
                ))
            )
        )
        .is_err()
    );
}

#[test]
fn overlapping_targets_are_deduplicated_and_duplicate_fields_colors_and_depth_are_rejected() {
    let (base, raster) = fixture(false);
    let info = base.document_info().unwrap();
    let text = replace(
        &[
            role("raster", "error"),
            strict(
                info.document_uuid,
                Some(raster.layer_id),
                raster.plane_id,
                "error",
            ),
        ],
        "rgba8(255,0,0,255)",
        "rgba8(0,255,0,255)",
        true,
    );
    let reference = replace(
        &[role("raster", "error")],
        "rgba8(255,0,0,255)",
        "rgba8(0,255,0,255)",
        true,
    );
    assert_eq!(
        run(&base, &source("", &step(&text)))
            .unwrap()
            .staged()
            .journal_entries(),
        run(&base, &source("", &step(&reference)))
            .unwrap()
            .staged()
            .journal_entries()
    );
    let valid = unary("erase", &role("raster", "error"), "rgba8(255,0,0,255)");
    for invalid in [
        valid.replace(
            "colors = [rgba8(255,0,0,255)]",
            "colors = [rgba8(255,0,0,255),rgba8(255,0,0,255)]",
        ),
        valid.replace("enabled = true;", "enabled = true; enabled = false;"),
        valid.replace("enabled = true;", "enabled = true; extra = 0;"),
        valid.replace("rgba8(255,0,0,255)", "rgba8(256,0,0,255)"),
        valid.replace("plane_kind = raster", "plane_kind = main_line"),
        valid.replace("kind = erase", "kind = resize_document"),
        unary(
            "erase",
            &strict(info.document_uuid, None, 0, "skip"),
            "rgba8(255,0,0,255)",
        ),
    ] {
        assert!(
            compile_inkscript(
                &source("", &step(&invalid)),
                InkScriptRunParameterDecision::Resolve(Vec::new())
            )
            .is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn cancellation_after_progress_and_stale_capture_leave_live_state_untouched() {
    let (mut base, _) = fixture(false);
    let (_, text) = four_operations(false);
    let program = compile_inkscript(
        &source("", &step(&text)),
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    for threshold in [0, 3, 8, 16] {
        let before = base.clone();
        let mut polls = 0;
        let result = run_inkscript_dry(
            &program,
            capture_in_memory_input(&base).unwrap(),
            &mut || {
                polls += 1;
                polls > threshold
            },
        );
        assert_eq!(result.unwrap_err(), ScriptRunError::Cancelled);
        assert_unchanged(&base, &before);
    }
    let fingerprint = capture_in_memory_fingerprint(&base).unwrap();
    base.add_guide(GuideAxis::Vertical, 1).unwrap();
    let before = base.clone();
    assert_eq!(
        run_inkscript_dry(
            &program,
            capture_in_memory_input_at(&base, fingerprint),
            &mut || false
        )
        .unwrap_err(),
        ScriptRunError::StaleInput
    );
    assert_unchanged(&base, &before);
}

#[test]
fn initial_roles_stay_fixed_while_producer_references_use_the_new_plane() {
    let (base, _) = fixture(false);
    let bindings = "bindings { let owner = select layer { cardinality = one; missing = error; }; }";
    let create = "step \"Create\" as made { enabled = true; invoke create_plane { layer_id = $owner; format = rgba8; name = \"Made\"; }; }";
    let replace_initial = replace(
        &[role("raster", "error")],
        "rgba8(0,0,0,0)",
        "rgba8(0,255,0,255)",
        true,
    );
    let fixed = run(
        &base,
        &source(bindings, &format!("{create} {}", step(&replace_initial))),
    )
    .unwrap();
    assert_eq!(
        fixed.report().statements(),
        &[
            ScriptStatementOutcome::Committed,
            ScriptStatementOutcome::NoOp
        ]
    );
    let mut direct = base.clone();
    let (_, new_plane) = direct
        .create_plane(
            base.document_info().unwrap().layer_id,
            PixelFormat::StraightRgba8,
            "Made",
        )
        .unwrap();
    assert_eq!(fixed.staged().journal_entries(), direct.journal_entries());
    let reference = "{ kind = references; layer = $owner; plane = $made.plane; }";
    let replace_reference = replace(
        &[reference.to_owned()],
        "rgba8(0,0,0,0)",
        "rgba8(0,255,0,255)",
        true,
    );
    let mut from_result = run(
        &base,
        &source(bindings, &format!("{create} {}", step(&replace_reference))),
    )
    .unwrap();
    assert_eq!(
        from_result.report().statements(),
        &[
            ScriptStatementOutcome::Committed,
            ScriptStatementOutcome::Committed
        ]
    );
    let operation = BatchOperation {
        target: BatchTargetSelector {
            layer_id: Some(base.document_info().unwrap().layer_id),
            plane_id: Some(new_plane),
            plane_kind: Some(PlaneType::Raster),
            missing_policy: BatchMissingTargetPolicy::Error,
        },
        ..direct_operation(
            PlaneType::Raster,
            BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([0; 4]),
                new: PixelValue::Rgba([0, 255, 0, 255]),
            }]),
        )
    };
    direct
        .apply_batch_operations(&[operation], || false)
        .unwrap();
    assert_eq!(
        from_result.staged().journal_entries(),
        direct.journal_entries()
    );
    let target = EditorTarget {
        layer_id: base.document_info().unwrap().layer_id,
        plane_id: new_plane,
    };
    assert_eq!(
        pixels(from_result.staged(), target, false),
        vec![PixelValue::Rgba([0, 255, 0, 255]); 3]
    );
    from_result.staged_mut().undo().unwrap();
    assert_eq!(
        pixels(from_result.staged(), target, false),
        vec![PixelValue::Rgba([0; 4]); 3]
    );
    from_result.staged_mut().redo().unwrap();
    from_result.staged_mut().release_history_cache().unwrap();
    assert_eq!(
        from_result
            .staged()
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        direct.document_state_digest().unwrap()
    );
}

#[test]
fn only_enabled_nested_references_propagate_missing_producer_skip() {
    let (base, _) = fixture(false);
    let bindings =
        "bindings { let absent = select plane { name = \"Absent\"; missing = skip_dependents; }; }";
    let producer = "step \"Producer\" as made { enabled = true; invoke duplicate_plane { plane_id = $absent; }; }";
    let reference = unary(
        "erase",
        "{ kind = references; layer = none; plane = $made.plane; }",
        "rgba8(255,0,0,255)",
    );
    let sibling = unary("erase", &role("raster", "error"), "rgba8(255,0,0,255)");
    let disabled_reference = reference.replacen("enabled = true", "enabled = false", 1);
    let result = run(
        &base,
        &source(
            bindings,
            &format!(
                "{producer} {}",
                step(&format!("{disabled_reference},{sibling}"))
            ),
        ),
    )
    .unwrap();
    assert_eq!(
        result.report().statements(),
        &[
            ScriptStatementOutcome::Skipped,
            ScriptStatementOutcome::Committed
        ]
    );
    let result = run(
        &base,
        &source(
            bindings,
            &format!("{producer} {}", step(&format!("{reference},{sibling}"))),
        ),
    )
    .unwrap();
    assert_eq!(
        result.report().statements(),
        &[
            ScriptStatementOutcome::Skipped,
            ScriptStatementOutcome::Skipped
        ]
    );
    assert_unchanged(result.staged(), &base);
    let disabled_producer = producer.replacen("enabled = true", "enabled = false", 1);
    assert!(
        compile_inkscript(
            &source(
                bindings,
                &format!(
                    "{disabled_producer} {}",
                    step(&format!("{disabled_reference},{sibling}"))
                )
            ),
            InkScriptRunParameterDecision::Resolve(Vec::new())
        )
        .is_err()
    );
}

#[test]
fn source_item_bounds_and_expanded_operation_bounds_are_closed() {
    let (mut base, _) = fixture(false);
    let info = base.document_info().unwrap();
    let erase = unary("erase", &role("raster", "error"), "rgba8(1,2,3,255)");
    let compile = |text: &str| {
        compile_inkscript(
            &source("", &step(text)),
            InkScriptRunParameterDecision::Resolve(Vec::new()),
        )
    };
    assert!(compile(&vec![erase.clone(); 1024].join(",")).is_ok());
    assert!(compile(&vec![erase; 1025].join(",")).is_err());
    for count in [64, 65] {
        let targets = (0..count)
            .map(|index| strict(info.document_uuid, None, u64::MAX - index, "skip"))
            .collect::<Vec<_>>();
        let text = replace(&targets, "rgba8(0,0,0,0)", "rgba8(1,2,3,255)", true);
        assert_eq!(compile(&text).is_ok(), count == 64);
        if count == 64 {
            assert_eq!(
                run(&base, &source("", &step(&text)))
                    .unwrap()
                    .report()
                    .commit_count(),
                0
            );
        }
    }
    for count in [4096, 4097] {
        let pair = "{ enabled = false; old = rgba8(1,2,3,255); new = rgba8(4,5,6,255); }";
        let pairs = format!(
            "{{ kind = color_replace; enabled = true; targets = [{}]; pairs = [{}]; }}",
            role("raster", "error"),
            vec![pair; count].join(",")
        );
        assert_eq!(compile(&pairs).is_ok(), count == 4096);
        let colors = (0..count)
            .map(|index| format!("rgba8({},{},9,255)", index / 256, index % 256))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!(
            "{{ kind = erase; enabled = true; target = {}; colors = [{colors}]; }}",
            role("raster", "error")
        );
        assert_eq!(compile(&text).is_ok(), count == 4096);
    }
    base.create_plane(info.layer_id, PixelFormat::StraightRgba8, "Second raster")
        .unwrap();
    let replace = replace(
        &[role("raster", "error")],
        "rgba8(1,2,3,255)",
        "rgba8(4,5,6,255)",
        true,
    );
    let within = vec![replace.clone(); 512].join(",");
    assert_eq!(
        run(&base, &source("", &step(&within)))
            .unwrap()
            .report()
            .commit_count(),
        0
    );
    let overflow = vec![replace; 513].join(",");
    let before = base.clone();
    assert!(run(&base, &source("", &step(&overflow))).is_err());
    assert_unchanged(&base, &before);
}

#[test]
fn summed_pixel_work_is_checked_at_bind_and_again_after_resize_before_batch() {
    let mut large = Core::new();
    large
        .new_cell_with_uuid(8192, 8192, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xba731)
        .unwrap();
    let erase = unary("erase", &role("color", "error"), "rgba8(1,2,3,255)");
    let operations = format!("{erase},{erase}");
    let before = large.clone();
    assert!(run(&large, &source("", &step(&operations))).is_err());
    assert_unchanged(&large, &before);
    let (base, _) = fixture(false);
    let resize = "step \"Resize\" { enabled = true; invoke resize_document { resize = { width = 8192; height = 8192; dpi_x_milli = 72000; dpi_y_milli = 72000; resample = false; anchor = top_left; }; }; }";
    let before = base.clone();
    assert!(
        run(
            &base,
            &source("", &format!("{resize} {}", step(&operations)))
        )
        .is_err()
    );
    assert_unchanged(&base, &before);
}
