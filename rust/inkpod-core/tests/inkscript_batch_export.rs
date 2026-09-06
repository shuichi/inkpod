use inkpod_core::inkscript::{
    InkScriptExportError, InkScriptExportLimits, InkScriptExportPortability,
    InkScriptRunParameterDecision, InkScriptSource, InkScriptSourceId, capture_in_memory_input,
    compile_inkscript, export_inkscript_fragment, export_inkscript_fragment_with_limits,
    run_inkscript_dry,
};
use inkpod_core::{
    AssetAlphaSemantics, AssetColorSpace, BATCH_OPERATION_VERSION, BatchColorPair,
    BatchMissingTargetPolicy, BatchOperation, BatchOperationKind, BatchTargetSelector, Core,
    DEFAULT_DPI_MILLI, GuideAxis, JournalEntry, JournalEventId, PixelFormat, PixelValue, PlaneType,
    PrimitiveRequest, RasterAssetInput,
};

fn events(core: &Core) -> Vec<JournalEventId> {
    core.journal_entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Commit(commit) => Some(commit.event_id()),
            _ => None,
        })
        .collect()
}

fn reopened(core: &Core) -> Core {
    let (native, _) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap();
    let bytes = inkpod_format::encode_procedure_file(&native).unwrap();
    Core::from_native_file(inkpod_format::decode_procedure_file(&bytes).unwrap(), false).unwrap()
}

fn fixture(uuid: u128) -> Core {
    fixture_with_prior_ids(uuid, 0)
}

fn fixture_with_prior_ids(uuid: u128, guide_count: usize) -> Core {
    let mut core = Core::new();
    let info = core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
        .unwrap();
    for position in 0..guide_count {
        core.add_guide(GuideAxis::Vertical, i32::try_from(position).unwrap())
            .unwrap();
    }
    let (_, raster_id) = core
        .create_plane(info.layer_id, PixelFormat::StraightRgba8, "Batch source")
        .unwrap();
    for plane_id in [info.color_plane_id, raster_id] {
        core.execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: core.document_info().unwrap().document_revision,
            target_plane_id: plane_id,
            raster: RasterAssetInput {
                width: 2,
                height: 1,
                pixel_format: PixelFormat::StraightRgba8,
                color_space: Some(AssetColorSpace::Srgb),
                alpha_semantics: AssetAlphaSemantics::Straight,
                canonical_stride: 8,
                pixels: vec![255, 0, 0, 255, 0, 0, 255, 255],
                expected_id: None,
            },
        })
        .unwrap();
    }
    core
}

fn target(kind: PlaneType) -> BatchTargetSelector {
    BatchTargetSelector {
        layer_id: None,
        plane_id: None,
        plane_kind: Some(kind),
        missing_policy: BatchMissingTargetPolicy::Error,
    }
}

fn operation(target: BatchTargetSelector, kind: BatchOperationKind) -> BatchOperation {
    BatchOperation {
        version: BATCH_OPERATION_VERSION,
        enabled: true,
        target,
        additional_targets: Vec::new(),
        kind,
    }
}

fn replace(target: BatchTargetSelector) -> BatchOperation {
    operation(
        target,
        BatchOperationKind::ColorReplace(vec![
            BatchColorPair {
                enabled: false,
                old: PixelValue::Rgba([255, 0, 0, 255]),
                new: PixelValue::Rgba([0, 0, 0, 255]),
            },
            BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([255, 0, 0, 255]),
                new: PixelValue::Rgba([0, 255, 0, 255]),
            },
        ]),
    )
}

fn file(fragment: &str) -> InkScriptSource {
    let text = fragment
        .replacen("inkscript_fragment 3;", "inkscript 3;", 1)
        .replacen("program {", "inputs { current_document; }\nprogram {", 1);
    InkScriptSource::new(
        InkScriptSourceId::new(3100),
        format!(
            "{text}output {{ policy = duplicate; format = inkpod; folder = \"out\"; cell_folder = false; basename = \"batch-export\"; start_number = 1; direction = ascending; }}\nexecution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}\n"
        )
        .as_bytes(),
    )
    .unwrap()
}

fn replay(fragment: &str, base: &Core) -> Core {
    let program = compile_inkscript(
        &file(fragment),
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    let result = run_inkscript_dry(
        &program,
        capture_in_memory_input(base).unwrap(),
        &mut || false,
    )
    .unwrap();
    reopened(result.staged())
}

#[test]
fn batch_export_preserves_expanded_order_exact_invocation_and_one_undo() {
    let mut core = fixture(0x3101);
    let base = reopened(&core);
    let mut replace = replace(target(PlaneType::Raster));
    let layer = &core.layers().unwrap()[0];
    let raster = layer
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneType::Raster)
        .unwrap();
    replace.additional_targets = vec![
        target(PlaneType::Color),
        BatchTargetSelector {
            layer_id: Some(layer.id),
            plane_id: Some(raster.id),
            ..target(PlaneType::Raster)
        },
    ];
    let mut disabled = operation(
        target(PlaneType::Color),
        BatchOperationKind::Erase(vec![PixelValue::Rgba([0, 255, 0, 255])]),
    );
    disabled.enabled = false;
    core.apply_batch_operations(
        &[
            replace,
            disabled,
            operation(
                target(PlaneType::Color),
                BatchOperationKind::Masking(vec![PixelValue::Rgba([0, 255, 0, 255])]),
            ),
            operation(
                target(PlaneType::Raster),
                BatchOperationKind::Erase(vec![PixelValue::Rgba([0, 0, 255, 255])]),
            ),
        ],
        || false,
    )
    .unwrap();
    let before_info = core.document_info().unwrap();
    let before_journal = core.journal_entries().to_vec();
    let event = *events(&core).last().unwrap();
    let exported = export_inkscript_fragment(&core, &[event], &mut || false).unwrap();
    assert_eq!(exported.commit_count(), 1);
    assert_eq!(
        exported.portability(),
        InkScriptExportPortability::RequiresBinding
    );
    assert_eq!(
        exported
            .text()
            .matches("invoke apply_batch_operations")
            .count(),
        1
    );
    assert_eq!(exported.text().matches("kind = color_replace").count(), 2);
    assert_eq!(exported.text().matches("kind = masking").count(), 1);
    assert_eq!(exported.text().matches("kind = erase").count(), 1);
    assert_eq!(exported.text().matches("kind = strict").count(), 4);
    assert!(exported.text().contains("state_digest = blake3"));
    assert!(exported.text().contains("id_allocation_digest = blake3"));
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.journal_entries(), before_journal);

    let mut replayed = replay(exported.text(), &base);
    assert_eq!(
        replayed.document_state_digest().unwrap(),
        core.document_state_digest().unwrap()
    );
    assert_eq!(replayed.journal_entries(), core.journal_entries());
    replayed.release_history_cache().unwrap();
    replayed.verify_journal_replay().unwrap();
    replayed.undo().unwrap();
    assert_eq!(
        replayed.document_state_digest().unwrap(),
        base.document_state_digest().unwrap()
    );
    replayed.redo().unwrap();
    assert_eq!(
        replayed.document_state_digest().unwrap(),
        core.document_state_digest().unwrap()
    );
}

#[test]
fn batch_export_prefers_preceding_plane_producer_and_retains_external_closure() {
    let base = fixture(0x3102);
    let mut core = reopened(&base);
    let info = core.document_info().unwrap();
    let raster = core.layers().unwrap()[0]
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneType::Raster)
        .unwrap()
        .id;
    let (_, plane) = core.duplicate_plane(raster).unwrap();
    core.apply_batch_operations(
        &[replace(BatchTargetSelector {
            layer_id: Some(info.layer_id),
            plane_id: Some(plane),
            ..target(PlaneType::Raster)
        })],
        || false,
    )
    .unwrap();
    let selected = events(&core)
        .into_iter()
        .skip(events(&base).len())
        .collect::<Vec<_>>();
    let exported = export_inkscript_fragment(&core, &selected, &mut || false).unwrap();
    assert!(exported.text().contains("kind = references"));
    assert!(exported.text().contains("plane = $step_1.plane"));
    assert!(
        !exported
            .text()
            .contains(&format!("persistent_plane_id = {plane}"))
    );
    assert_eq!(
        replay(exported.text(), &base).journal_entries(),
        core.journal_entries()
    );

    let single = export_inkscript_fragment(&core, &selected[1..], &mut || false).unwrap();
    assert!(
        single
            .text()
            .contains(&format!("persistent_plane_id = {plane}"))
    );
    assert!(!single.text().contains("$step_1.plane"));
}

#[test]
fn batch_export_resource_and_cancel_failures_preserve_source_and_publish_nothing() {
    let mut core = fixture(0x3103);
    core.apply_batch_operations(&[replace(target(PlaneType::Color))], || false)
        .unwrap();
    let selected = [*events(&core).last().unwrap()];
    let before = core.document_info().unwrap();
    let journal = core.journal_entries().to_vec();
    assert_eq!(
        export_inkscript_fragment_with_limits(
            &core,
            &selected,
            InkScriptExportLimits::exact_current().with_source_bytes(32),
            &mut || false,
        )
        .unwrap_err(),
        InkScriptExportError::ResourceLimit,
    );
    let mut polls = 0;
    assert_eq!(
        export_inkscript_fragment(&core, &selected, &mut || {
            polls += 1;
            polls >= 3
        })
        .unwrap_err(),
        InkScriptExportError::Cancelled,
    );
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.journal_entries(), journal);
}

#[test]
fn batch_export_requires_explicit_rebinding_before_execution_on_another_document() {
    let mut source = fixture(0x3104);
    source
        .apply_batch_operations(&[replace(target(PlaneType::Raster))], || false)
        .unwrap();
    let exported =
        export_inkscript_fragment(&source, &[*events(&source).last().unwrap()], &mut || false)
            .unwrap();
    let destination = fixture_with_prior_ids(0x3105, 3);
    let before = destination.document_state_digest().unwrap();
    let strict = compile_inkscript(
        &file(exported.text()),
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    assert!(
        run_inkscript_dry(
            &strict,
            capture_in_memory_input(&destination).unwrap(),
            &mut || false
        )
        .is_err()
    );

    // Model an explicit text edit: remove the exact-parent assertions, then replace every
    // external strict target with a semantic role. This is not an automatic export fallback.
    let mut rebound = exported.text().to_owned();
    let assertion_start = rebound.find("assert document").unwrap();
    let assertion_end = assertion_start + rebound[assertion_start..].find("};").unwrap() + 2;
    rebound.replace_range(assertion_start..assertion_end, "");
    let without_assertions = compile_inkscript(
        &file(&rebound),
        InkScriptRunParameterDecision::Resolve(Vec::new()),
    )
    .unwrap();
    assert!(
        run_inkscript_dry(
            &without_assertions,
            capture_in_memory_input(&destination).unwrap(),
            &mut || false
        )
        .is_err()
    );
    let strict_kind = rebound.find("kind = strict").unwrap();
    let target_start = rebound[..strict_kind].rfind('{').unwrap();
    let target_end = strict_kind + rebound[strict_kind..].find('}').unwrap() + 1;
    rebound.replace_range(
        target_start..target_end,
        "{ kind = role; plane_kind = raster; missing = error; }",
    );
    assert!(!rebound.contains("kind = strict"));
    assert!(!rebound.contains("source_document_uuid"));
    let actual = replay(&rebound, &destination);
    let mut expected = reopened(&destination);
    expected
        .apply_batch_operations(&[replace(target(PlaneType::Raster))], || false)
        .unwrap();
    assert_eq!(
        actual.document_state_digest().unwrap(),
        expected.document_state_digest().unwrap()
    );
    assert_eq!(actual.journal_entries(), expected.journal_entries());
    assert_eq!(destination.document_state_digest().unwrap(), before);
    assert_ne!(
        actual.document_state_digest().unwrap(),
        source.document_state_digest().unwrap()
    );
}
