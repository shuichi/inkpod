use inkpod_core::inkscript::{
    InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId, ScriptDryRunResult,
    ScriptRunError, capture_in_memory_input, compile_inkscript, run_inkscript_dry,
};
use inkpod_core::{
    BATCH_OPERATION_VERSION, BatchColorPair, BatchMissingTargetPolicy, BatchOperation,
    BatchOperationKind, BatchTargetSelector, Core, DEFAULT_DPI_MILLI, DocumentResize, PixelFormat,
    PixelValue, PlaneType, ResizeAnchor,
};
use inkpod_format::{
    INKSCRIPT_FILE_VERSION, INKSCRIPT_PROCEDURE_CATALOG_VERSION, INKSCRIPT_REQUIRED_REPLAY_EPOCH,
};

fn source(bindings: &str, steps: &str) -> InkScriptSource {
    InkScriptSource::new(InkScriptSourceId::new(3200), format!(
        "inkscript {INKSCRIPT_FILE_VERSION}; requires {{ procedure_catalog = {INKSCRIPT_PROCEDURE_CATALOG_VERSION}; replay_epoch = {INKSCRIPT_REQUIRED_REPLAY_EPOCH}; }} inputs {{ current_document; }} {bindings} program {{ {steps} }} output {{ policy = duplicate; format = inkpod; folder = \"out\"; cell_folder = false; basename = \"binding\"; start_number = 1; direction = ascending; }} execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"
    ).as_bytes()).unwrap()
}
fn fixture() -> (Core, u64, u64) {
    let mut core = Core::new();
    let info = core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x3200)
        .unwrap();
    let (_, plane) = core
        .create_plane(info.layer_id, PixelFormat::StraightRgba8, "Initial Raster")
        .unwrap();
    (core, info.layer_id, plane)
}
fn bindings() -> &'static str {
    "bindings { let owner = select layer { cardinality = first; }; let initial = select plane { plane_kind = raster; cardinality = first; }; }"
}
fn batch(target: &str) -> String {
    format!(
        "step \"Batch\" {{ enabled = true; invoke apply_batch_operations {{ operations = [{{ kind = color_replace; enabled = true; targets = [{target}]; pairs = [{{ enabled = true; old = rgba8(0,0,0,0); new = rgba8(0,255,0,255); }}]; }}]; }}; }}"
    )
}
fn run(core: &Core, source: &InkScriptSource) -> Result<ScriptDryRunResult, ScriptRunError> {
    let program =
        compile_inkscript(source, InkScriptRunParameterDecision::Resolve(Vec::new())).unwrap();
    run_inkscript_dry(
        &program,
        capture_in_memory_input(core).unwrap(),
        &mut || false,
    )
}
fn apply_expected(core: &mut Core, layer: u64, plane: u64) {
    core.apply_batch_operations(
        &[BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            target: BatchTargetSelector {
                layer_id: Some(layer),
                plane_id: Some(plane),
                plane_kind: Some(PlaneType::Raster),
                missing_policy: BatchMissingTargetPolicy::Error,
            },
            additional_targets: Vec::new(),
            kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([0; 4]),
                new: PixelValue::Rgba([0, 255, 0, 255]),
            }]),
        }],
        || false,
    )
    .unwrap();
}
fn equivalent(actual: &Core, expected: &Core) {
    assert_eq!(
        actual.document_state_digest().unwrap(),
        expected.document_state_digest().unwrap()
    );
    assert_eq!(
        actual.document_info().unwrap(),
        expected.document_info().unwrap()
    );
    assert_eq!(actual.journal_entries(), expected.journal_entries());
    let mut replay = actual.clone();
    replay.release_history_cache().unwrap();
    replay.verify_journal_replay().unwrap();
}

#[test]
fn producer_references_target_new_plane_and_enforce_owner() {
    let (core, layer, _) = fixture();
    let mut expected = core.clone();
    let (_, new_plane) = expected
        .create_plane(layer, PixelFormat::StraightRgba8, "Later")
        .unwrap();
    apply_expected(&mut expected, layer, new_plane);
    let steps = format!(
        "step \"Create\" as made {{ enabled = true; invoke create_plane {{ layer_id = $owner; format = rgba8; name = \"Later\"; }}; }} {}",
        batch("{ kind = references; layer = $owner; plane = $made.plane; }")
    );
    equivalent(
        run(&core, &source(bindings(), &steps)).unwrap().staged(),
        &expected,
    );

    let invalid = format!("step \"Other layer\" as other {{ enabled = true; invoke create_layer {{ name = \"Other\"; }}; }} {steps}").replace("layer = $owner; plane = $made.plane", "layer = $other.layer; plane = $made.plane");
    assert!(run(&core, &source(bindings(), &invalid)).is_err());
    assert_eq!(core.layers().unwrap().len(), 1);
}

#[test]
fn initial_references_resolve_bindings_and_deleted_role_does_not_retarget() {
    let (core, layer, plane) = fixture();
    let mut expected = core.clone();
    apply_expected(&mut expected, layer, plane);
    equivalent(
        run(
            &core,
            &source(
                bindings(),
                &batch("{ kind = references; layer = $owner; plane = $initial; }"),
            ),
        )
        .unwrap()
        .staged(),
        &expected,
    );
    let steps = format!(
        "step \"Create replacement\" {{ enabled = true; invoke create_plane {{ layer_id = $owner; format = rgba8; name = \"Replacement\"; }}; }} step \"Delete bound plane\" {{ enabled = true; invoke delete_plane {{ plane_id = $initial; }}; }} {}",
        batch("{ kind = role; plane_kind = raster; missing = skip; }")
    );
    assert!(run(&core, &source(bindings(), &steps)).is_err());
    assert_eq!(core.layers().unwrap()[0].planes.len(), 3);
}

#[test]
fn resized_dimensions_are_used_by_batch_with_fixed_initial_targets() {
    let (core, layer, plane) = fixture();
    let mut expected = core.clone();
    expected
        .resize_document(DocumentResize {
            width: 4,
            height: 3,
            dpi_x_milli: DEFAULT_DPI_MILLI,
            dpi_y_milli: DEFAULT_DPI_MILLI,
            anchor: ResizeAnchor::TopLeft,
            resample: false,
        })
        .unwrap();
    apply_expected(&mut expected, layer, plane);
    let steps = format!(
        "step \"Resize\" {{ enabled = true; invoke resize_document {{ resize = {{ width = 4; height = 3; dpi_x_milli = {DEFAULT_DPI_MILLI}; dpi_y_milli = {DEFAULT_DPI_MILLI}; anchor = top_left; resample = false; }}; }}; }} {}",
        batch("{ kind = role; plane_kind = raster; missing = error; }")
    );
    equivalent(
        run(&core, &source(bindings(), &steps)).unwrap().staged(),
        &expected,
    );
}
