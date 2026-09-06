use inkpod_core::inkscript::{
    InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId, compile_inkscript,
};

fn compile(inputs: &str, program: &str, output: &str) -> bool {
    let text = format!(
        "inkscript 3; requires {{ procedure_catalog = 8; replay_epoch = 29; }}\ninputs {{ {inputs} }}\nprogram {{ {program} }}\noutput {{ {output} }}\nexecution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}"
    );
    compile_inkscript(
        &InkScriptSource::new(InkScriptSourceId::new(4001), text.as_bytes()).unwrap(),
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .is_ok()
}

fn batch(kind: &str, enabled: bool, outer: bool) -> String {
    format!(
        "step \"batch\" {{ enabled = {outer}; invoke apply_batch_operations {{ operations = [{{ kind = {kind}; enabled = {enabled}; target = {{ kind = role; plane_kind = color; missing = error; }}; colors = [rgba8(1,2,3,4)]; }}]; }}; }}"
    )
}

#[test]
fn active_output_requires_exactly_one_current_input_and_one_batch_invocation() {
    let erase = batch("erase", true, true);
    let output = "policy = active_document;";
    assert!(compile(
        "profile = batch; current_document;",
        &erase,
        output
    ));
    assert!(!compile(
        "profile = batch; file \"a.inkpod\";",
        &erase,
        output
    ));
    assert!(!compile(
        "current_document; current_document;",
        &erase,
        output
    ));
    assert!(!compile("current_sequence;", &erase, output));
    let grid = "step \"grid\" { enabled = true; invoke set_grid { grid = { origin_x = 0; origin_y = 0; spacing_x = 8; spacing_y = 8; subdivisions = 1; }; }; }";
    assert!(!compile("current_document;", grid, output));
    assert!(!compile(
        "current_document;",
        &format!("{erase}\n{grid}"),
        output
    ));
}

#[test]
fn raster_rejects_enabled_masking_but_accepts_disabled_outer_masking() {
    let output =
        "policy = folder; format = png; folder = \"out\"; naming_template = \"{index:4}\";";
    assert!(!compile(
        "current_document;",
        &batch("masking", true, true),
        output
    ));
    assert!(compile(
        "current_document;",
        &batch("erase", true, true),
        output
    ));
    let program = format!(
        "{}\n{}",
        batch("masking", true, false),
        batch("erase", true, true).replace("\"batch\"", "\"erase\"")
    );
    assert!(compile("current_document;", &program, output));
    assert!(compile(
        "current_document;",
        &batch("masking", true, true),
        "policy = new_tabs;"
    ));
}

#[test]
fn raster_cannot_be_overwritten_with_native_bytes() {
    assert!(!compile(
        "file \"a.png\";",
        &batch("erase", true, true),
        "policy = explicit_overwrite; format = inkpod;"
    ));
}
