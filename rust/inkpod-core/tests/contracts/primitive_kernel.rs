use super::*;
use std::collections::BTreeMap;

const PRIMITIVE_TEST_UUID: u128 = 0x0049_4e4b_504f_442d_5052_494d_4954_4901;
const REPLACEMENT_TEST_UUID: u128 = 0x0049_4e4b_504f_442d_5245_504c_4143_4502;

fn primitive_core() -> Core {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        128,
        64,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        PRIMITIVE_TEST_UUID,
    )
    .unwrap();
    core
}

fn document_digest(core: &Core) -> [u8; 32] {
    *core.document_state_digest().unwrap().as_bytes()
}

#[derive(Debug, PartialEq)]
struct PrimitiveCoreObservation {
    digest: [u8; 32],
    document: DocumentInfo,
    layers: Vec<LayerInfo>,
    palette: Vec<PixelValue>,
    main_line_color: PixelValue,
    history: Vec<HistoryEntryInfo>,
    history_cursor: usize,
    snapshot: RenderSnapshot,
    resources: ResourceUsage,
}

impl PrimitiveCoreObservation {
    fn capture(core: &mut Core) -> Self {
        let digest = *core.document_state_digest().unwrap().as_bytes();
        let document = core.document_info().unwrap();
        let layers = core.layers().unwrap();
        let palette = core.palette().unwrap().to_vec();
        let main_line_color = core.main_line_color().unwrap();
        let history = core.history_entries();
        let history_cursor = core.history_cursor();
        let snapshot = core.build_snapshot();
        let resources = core.resource_usage();
        Self {
            digest,
            document,
            layers,
            palette,
            main_line_color,
            history,
            history_cursor,
            snapshot,
            resources,
        }
    }
}

fn assert_semantically_equal(left: &mut Core, right: &mut Core) {
    assert_eq!(
        PrimitiveCoreObservation::capture(left),
        PrimitiveCoreObservation::capture(right)
    );
}

#[test]
fn canonical_execution_and_fresh_replay_are_bit_exact_at_each_primitive_boundary() {
    let mut runtime = primitive_core();
    let mut replay = primitive_core();
    let mut boundary_digests = vec![document_digest(&runtime)];

    let revision = runtime.document_info().unwrap().document_revision;
    let main_line = runtime
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: revision,
            color: PixelValue::Rgba16([0x1234, 0x4567, 0x89ab, 0xffff]),
        })
        .unwrap();
    let main_line_procedure = main_line
        .procedure()
        .expect("a committed primitive must return its canonical procedure")
        .clone();
    assert_eq!(main_line_procedure.primitive_id().get(), 0x0003_0001);
    assert_eq!(main_line_procedure.primitive_schema_version(), 1);
    assert_eq!(main_line_procedure.replay_epoch().get(), 7);
    assert_eq!(main_line_procedure.procedure_id().get(), 1);
    assert_eq!(main_line_procedure.base_state_id().get(), 1);
    assert_eq!(main_line_procedure.committed_state_id().get(), 2);
    assert!(main_line_procedure.output_ids().is_empty());
    assert!(main_line_procedure.canonical_payload().is_empty());
    replay.replay_procedure(&main_line_procedure).unwrap();
    assert_semantically_equal(&mut runtime, &mut replay);
    boundary_digests.push(document_digest(&runtime));

    let palette_colors = vec![
        PixelValue::Rgba([2, 4, 8, 255]),
        PixelValue::Rgba16([0x0102, 0x0304, 0x0506, 0x0708]),
    ];
    let revision = runtime.document_info().unwrap().document_revision;
    let palette = runtime
        .execute_primitive(PrimitiveRequest::ReplacePalette {
            expected_revision: revision,
            colors: palette_colors,
        })
        .unwrap();
    let palette_procedure = palette
        .procedure()
        .expect("a committed primitive must return its canonical procedure")
        .clone();
    assert_eq!(palette_procedure.primitive_id().get(), 0x0003_0002);
    assert_eq!(palette_procedure.primitive_schema_version(), 1);
    assert_eq!(palette_procedure.replay_epoch().get(), 7);
    assert_eq!(palette_procedure.procedure_id().get(), 2);
    assert_eq!(palette_procedure.base_state_id().get(), 2);
    assert_eq!(palette_procedure.committed_state_id().get(), 3);
    assert!(palette_procedure.output_ids().is_empty());
    assert!(palette_procedure.canonical_payload().is_empty());
    replay.replay_procedure(&palette_procedure).unwrap();
    assert_semantically_equal(&mut runtime, &mut replay);
    boundary_digests.push(document_digest(&runtime));

    let document = runtime.document_info().unwrap();
    let stroke = Stroke {
        tool: PaintTool::Brush,
        plane: ActivePlane::Color,
        color: [21, 34, 55, 233],
        diameter: 3.5,
        auto_erase: false,
        pressure_size: true,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![
            StrokeSample {
                x: 2.5,
                y: -0.0,
                pressure: 0.5,
            },
            StrokeSample {
                x: 66.25,
                y: 3.75,
                pressure: 1.0,
            },
        ],
    };
    let stroke_outcome = runtime
        .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            stroke,
        })
        .unwrap();
    let stroke_procedure = stroke_outcome
        .procedure()
        .expect("a committed primitive must return its canonical procedure")
        .clone();
    assert_eq!(stroke_procedure.primitive_id().get(), 0x0005_0001);
    assert_eq!(stroke_procedure.primitive_schema_version(), 2);
    assert_eq!(stroke_procedure.replay_epoch().get(), 7);
    assert_eq!(stroke_procedure.procedure_id().get(), 3);
    assert_eq!(stroke_procedure.base_state_id().get(), 3);
    assert_eq!(stroke_procedure.committed_state_id().get(), 4);
    assert!(stroke_procedure.output_ids().is_empty());
    assert_canonical_stroke_payload(stroke_procedure.canonical_payload());
    replay.replay_procedure(&stroke_procedure).unwrap();
    assert_semantically_equal(&mut runtime, &mut replay);
    boundary_digests.push(document_digest(&runtime));

    let contract = replay_contract();
    let composite = runtime
        .build_snapshot()
        .canonical_composite_digest()
        .unwrap()
        .as_bytes();
    assert_eq!(contract.replay_epoch().get(), 7);
    assert_eq!(contract.procedure_format_version(), 10);
    assert_eq!(contract.canonical_numeric_version(), 1);
    assert_eq!(contract.primitive_count(), 76);
    assert_eq!(
        *contract.primitive_catalog_digest(),
        [
            206, 12, 128, 124, 106, 176, 33, 145, 4, 172, 30, 236, 84, 240, 142, 25, 164, 149, 101,
            252, 183, 167, 178, 234, 173, 121, 76, 227, 69, 122, 249, 90
        ]
    );
    assert_eq!(
        boundary_digests,
        vec![
            [
                157, 39, 222, 250, 197, 81, 73, 180, 16, 206, 100, 55, 168, 228, 60, 16, 72, 252,
                226, 86, 212, 176, 116, 55, 184, 175, 221, 134, 75, 4, 68, 61
            ],
            [
                47, 152, 210, 188, 93, 64, 199, 40, 8, 85, 194, 168, 220, 245, 81, 196, 222, 31,
                103, 193, 109, 133, 10, 54, 228, 40, 11, 45, 183, 83, 232, 187
            ],
            [
                100, 156, 195, 98, 187, 93, 48, 218, 88, 140, 146, 169, 43, 4, 113, 240, 119, 52,
                35, 21, 33, 31, 76, 93, 251, 5, 238, 50, 63, 240, 202, 207
            ],
            [
                185, 84, 9, 16, 125, 188, 182, 250, 159, 45, 203, 93, 117, 111, 217, 32, 240, 107,
                109, 105, 96, 95, 19, 99, 104, 99, 170, 62, 225, 108, 194, 154
            ],
        ]
    );
    assert_eq!(
        composite,
        [
            51, 88, 148, 214, 238, 70, 18, 80, 131, 67, 44, 60, 133, 31, 255, 174, 184, 112, 85, 4,
            197, 186, 100, 255, 77, 211, 50, 174, 164, 197, 251, 183
        ]
    );
}

#[test]
fn main_line_replay_does_not_depend_on_the_digest_excluded_active_target() {
    let mut runtime = primitive_core();
    let mut replay = primitive_core();
    let primary = runtime.document_info().unwrap();

    let mut alternate_main_plane_id = 0;
    for core in [&mut runtime, &mut replay] {
        let (_, alternate_layer_id) = core
            .create_layer(LayerKind::BinaryColoring, "Alternate Replay Target")
            .unwrap();
        let alternate_main = core
            .layers()
            .unwrap()
            .into_iter()
            .find(|layer| layer.id == alternate_layer_id)
            .and_then(|layer| {
                layer
                    .planes
                    .into_iter()
                    .find(|plane| plane.kind == PlaneType::MainLine)
            })
            .expect("the alternate coloring layer must own a main-line plane");
        alternate_main_plane_id = alternate_main.id;
        core.set_plane_properties(
            alternate_main.id,
            alternate_main.visible,
            false,
            alternate_main.opacity_milli,
            &alternate_main.name,
        )
        .unwrap();
    }

    runtime
        .set_active_node(primary.layer_id, primary.main_plane_id)
        .unwrap();
    let replay_alternate_layer_id = replay
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| {
            layer
                .planes
                .iter()
                .any(|plane| plane.id == alternate_main_plane_id)
        })
        .unwrap()
        .id;
    replay
        .set_active_node(replay_alternate_layer_id, alternate_main_plane_id)
        .unwrap();

    assert_eq!(document_digest(&runtime), document_digest(&replay));
    let replacement = PixelValue::Rgba16([0x1111, 0x3333, 0x7777, 0xffff]);
    let revision = runtime.document_info().unwrap().document_revision;
    let procedure = runtime
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: revision,
            color: replacement,
        })
        .unwrap()
        .procedure()
        .unwrap()
        .clone();

    let replayed = replay.replay_procedure(&procedure).unwrap();
    assert_eq!(replayed.procedure(), Some(&procedure));
    assert_eq!(runtime.main_line_color().unwrap(), replacement);
    assert_eq!(replay.main_line_color().unwrap(), replacement);
    assert_eq!(document_digest(&runtime), document_digest(&replay));
    assert_eq!(runtime.layers().unwrap(), replay.layers().unwrap());
    assert_eq!(runtime.build_snapshot(), replay.build_snapshot());

    let before_no_op = replay.document_info().unwrap();
    let no_op = replay.set_main_line_color(replacement).unwrap();
    assert_eq!(no_op.revision(), before_no_op.document_revision);
    assert_eq!(replay.document_info().unwrap(), before_no_op);
}

#[test]
fn replacing_document_resets_procedure_state_without_reusing_stable_object_ids() {
    let mut replaced = primitive_core();
    let initial_document = replaced.document_info().unwrap();
    let initial_revision = initial_document.document_revision;
    let prior = replaced
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: initial_revision,
            color: PixelValue::Rgba([1, 2, 3, 255]),
        })
        .unwrap();
    assert_eq!(prior.procedure().unwrap().procedure_id().get(), 1);
    assert_eq!(prior.procedure().unwrap().committed_state_id().get(), 2);

    replaced
        .new_cell_with_uuid(
            96,
            48,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            REPLACEMENT_TEST_UUID,
        )
        .unwrap();
    assert_ne!(
        replaced.document_info().unwrap().document_id,
        initial_document.document_id
    );

    let mut parallel = primitive_core();
    parallel
        .new_cell_with_uuid(
            96,
            48,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            REPLACEMENT_TEST_UUID,
        )
        .unwrap();
    let mut replay = primitive_core();
    replay
        .new_cell_with_uuid(
            96,
            48,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            REPLACEMENT_TEST_UUID,
        )
        .unwrap();

    let replacement_revision = replaced.document_info().unwrap().document_revision;
    let replaced_outcome = replaced
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: replacement_revision,
            color: PixelValue::Rgba16([0x0123, 0x4567, 0x89ab, 0xffff]),
        })
        .unwrap();
    let replaced_procedure = replaced_outcome.procedure().unwrap().clone();
    assert_eq!(replaced_procedure.procedure_id().get(), 1);
    assert_eq!(replaced_procedure.base_state_id().get(), 1);
    assert_eq!(replaced_procedure.committed_state_id().get(), 2);

    let parallel_revision = parallel.document_info().unwrap().document_revision;
    let parallel_outcome = parallel
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: parallel_revision,
            color: PixelValue::Rgba16([0x0123, 0x4567, 0x89ab, 0xffff]),
        })
        .unwrap();
    assert_eq!(parallel_outcome.procedure(), Some(&replaced_procedure));
    assert_eq!(document_digest(&replaced), document_digest(&parallel));
    assert_eq!(replaced.layers().unwrap(), parallel.layers().unwrap());
    assert_eq!(
        replaced.main_line_color().unwrap(),
        parallel.main_line_color().unwrap()
    );

    replay.replay_procedure(&replaced_procedure).unwrap();
    assert_eq!(document_digest(&replaced), document_digest(&replay));
    assert_eq!(replaced.layers().unwrap(), replay.layers().unwrap());
    assert_eq!(
        replaced.main_line_color().unwrap(),
        replay.main_line_color().unwrap()
    );
}

#[test]
fn failed_document_replacement_preserves_live_state_and_all_id_cursors() {
    let mut failed = primitive_core();
    let mut control = primitive_core();
    for core in [&mut failed, &mut control] {
        let revision = core.document_info().unwrap().document_revision;
        let outcome = core
            .execute_primitive(PrimitiveRequest::SetMainLineColor {
                expected_revision: revision,
                color: PixelValue::Rgba([12, 34, 56, 255]),
            })
            .unwrap();
        let procedure = outcome.procedure().unwrap();
        assert_eq!(procedure.procedure_id().get(), 1);
        assert_eq!(procedure.base_state_id().get(), 1);
        assert_eq!(procedure.committed_state_id().get(), 2);
    }

    let before_digest = document_digest(&failed);
    let before_document = failed.document_info().unwrap();
    let before_layers = failed.layers().unwrap();
    let before_history = failed.history_entries();
    let before_history_cursor = failed.history_cursor();
    assert!(
        failed
            .new_cell_with_uuid(
                0,
                64,
                DEFAULT_DPI_MILLI,
                DEFAULT_DPI_MILLI,
                REPLACEMENT_TEST_UUID,
            )
            .is_err()
    );
    assert_eq!(document_digest(&failed), before_digest);
    assert_eq!(failed.document_info().unwrap(), before_document);
    assert_eq!(failed.layers().unwrap(), before_layers);
    assert_eq!(failed.history_entries(), before_history);
    assert_eq!(failed.history_cursor(), before_history_cursor);

    let failed_revision = failed.document_info().unwrap().document_revision;
    let failed_next = failed
        .execute_primitive(PrimitiveRequest::ReplacePalette {
            expected_revision: failed_revision,
            colors: vec![PixelValue::Rgba16([1, 2, 3, 4])],
        })
        .unwrap();
    let control_revision = control.document_info().unwrap().document_revision;
    let control_next = control
        .execute_primitive(PrimitiveRequest::ReplacePalette {
            expected_revision: control_revision,
            colors: vec![PixelValue::Rgba16([1, 2, 3, 4])],
        })
        .unwrap();
    assert_eq!(failed_next.procedure(), control_next.procedure());
    let next_procedure = failed_next.procedure().unwrap();
    assert_eq!(next_procedure.procedure_id().get(), 2);
    assert_eq!(next_procedure.base_state_id().get(), 2);
    assert_eq!(next_procedure.committed_state_id().get(), 3);

    let (failed_layer_outcome, failed_layer_id) = failed
        .create_layer(LayerKind::BinaryColoring, "Post-failure Layer")
        .unwrap();
    let (control_layer_outcome, control_layer_id) = control
        .create_layer(LayerKind::BinaryColoring, "Post-failure Layer")
        .unwrap();
    assert_eq!(failed_layer_outcome, control_layer_outcome);
    assert_eq!(failed_layer_id, control_layer_id);
    assert_eq!(failed.layers().unwrap(), control.layers().unwrap());
}

fn assert_canonical_stroke_payload(payload: &[u8]) {
    assert_eq!(payload.len(), 8 + 2 * 24);
    assert_eq!(u64::from_le_bytes(payload[0..8].try_into().unwrap()), 2);

    let first = &payload[8..32];
    assert_eq!(
        i64::from_le_bytes(first[0..8].try_into().unwrap()),
        2 * 65_536 + 32_768
    );
    assert_eq!(i64::from_le_bytes(first[8..16].try_into().unwrap()), 0);
    assert_eq!(
        u16::from_le_bytes(first[16..18].try_into().unwrap()),
        32_768
    );
    assert_eq!(&first[18..24], &[0; 6]);

    let second = &payload[32..56];
    assert_eq!(
        i64::from_le_bytes(second[0..8].try_into().unwrap()),
        66 * 65_536 + 16_384
    );
    assert_eq!(
        i64::from_le_bytes(second[8..16].try_into().unwrap()),
        3 * 65_536 + 49_152
    );
    assert_eq!(
        u16::from_le_bytes(second[16..18].try_into().unwrap()),
        65_535
    );
    assert_eq!(&second[18..24], &[0; 6]);
}

#[test]
fn no_op_invalid_and_stale_requests_are_atomic_and_consume_no_persistent_ids() {
    let mut core = primitive_core();
    let initial_revision = core.document_info().unwrap().document_revision;
    let initial = PrimitiveCoreObservation::capture(&mut core);

    let no_op = core
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: initial_revision,
            color: PixelValue::Rgba([0, 0, 0, 255]),
        })
        .unwrap();
    assert!(no_op.procedure().is_none());
    assert_eq!(no_op.dispatch().revision(), initial_revision);
    assert_eq!(PrimitiveCoreObservation::capture(&mut core), initial);

    assert!(matches!(
        core.execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: initial_revision,
            color: PixelValue::Binary(255),
        }),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(PrimitiveCoreObservation::capture(&mut core), initial);

    let first = core
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: initial_revision,
            color: PixelValue::Rgba([90, 80, 70, 255]),
        })
        .unwrap();
    let first_procedure = first.procedure().unwrap();
    assert_eq!(first.dispatch().revision(), initial_revision + 1);
    assert_eq!(first_procedure.procedure_id().get(), 1);
    assert_eq!(first_procedure.base_state_id().get(), 1);
    assert_eq!(first_procedure.committed_state_id().get(), 2);
    assert_eq!(core.history_entries().len(), 1);
    assert!(core.document_info().unwrap().dirty);

    let after_first = PrimitiveCoreObservation::capture(&mut core);
    assert!(matches!(
        core.execute_primitive(PrimitiveRequest::ReplacePalette {
            expected_revision: initial_revision,
            colors: vec![PixelValue::Rgba([1, 2, 3, 255])],
        }),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(PrimitiveCoreObservation::capture(&mut core), after_first);

    let current_revision = core.document_info().unwrap().document_revision;
    let second = core
        .execute_primitive(PrimitiveRequest::ReplacePalette {
            expected_revision: current_revision,
            colors: vec![PixelValue::Rgba([1, 2, 3, 255])],
        })
        .unwrap();
    let second_procedure = second.procedure().unwrap();
    assert_eq!(second.dispatch().revision(), current_revision + 1);
    assert_eq!(second_procedure.procedure_id().get(), 2);
    assert_eq!(second_procedure.base_state_id().get(), 2);
    assert_eq!(second_procedure.committed_state_id().get(), 3);
    assert_eq!(core.history_entries().len(), 2);
}

#[test]
fn cancelled_stroke_preview_restores_base_and_consumes_no_procedure_or_state_id() {
    let mut core = primitive_core();
    let before = PrimitiveCoreObservation::capture(&mut core);
    let mut stroke = line_stroke(vec![StrokeSample {
        x: 4.0,
        y: 4.0,
        pressure: 1.0,
    }]);
    core.begin_stroke(&stroke).unwrap();
    core.append_stroke(&[StrokeSample {
        x: 20.0,
        y: 4.0,
        pressure: 1.0,
    }])
    .unwrap();
    core.cancel_stroke();
    assert_eq!(PrimitiveCoreObservation::capture(&mut core), before);

    stroke.samples.push(StrokeSample {
        x: 20.0,
        y: 4.0,
        pressure: 1.0,
    });
    let document = core.document_info().unwrap();
    let committed = core
        .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision: document.document_revision,
            target_plane_id: document.main_plane_id,
            stroke,
        })
        .unwrap();
    let procedure = committed.procedure().unwrap();
    assert_eq!(procedure.procedure_id().get(), 1);
    assert_eq!(procedure.base_state_id().get(), 1);
    assert_eq!(procedure.committed_state_id().get(), 2);
}

#[test]
fn bounded_stroke_work_overflow_is_atomic_and_consumes_no_persistent_ids() {
    let mut core = primitive_core();
    let before = PrimitiveCoreObservation::capture(&mut core);
    let document = core.document_info().unwrap();
    let sample = StrokeSample {
        x: 32.0,
        y: 32.0,
        pressure: 1.0,
    };
    let excessive = Stroke {
        tool: PaintTool::Brush,
        plane: ActivePlane::Color,
        color: [1, 2, 3, 255],
        diameter: 256.0,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![sample; 300],
    };
    assert!(matches!(
        core.execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            stroke: excessive,
        }),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(PrimitiveCoreObservation::capture(&mut core), before);

    let committed = core
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: document.document_revision,
            color: PixelValue::Rgba([1, 2, 3, 255]),
        })
        .unwrap();
    let procedure = committed.procedure().unwrap();
    assert_eq!(procedure.procedure_id().get(), 1);
    assert_eq!(procedure.base_state_id().get(), 1);
    assert_eq!(procedure.committed_state_id().get(), 2);
}

#[test]
fn legacy_public_wrappers_and_direct_executor_have_one_canonical_owner() {
    let mut wrapped = primitive_core();
    let mut direct = primitive_core();

    let color = PixelValue::Rgba16([0x1111, 0x2222, 0x3333, 0xeeee]);
    let wrapped_outcome = wrapped.set_main_line_color(color).unwrap();
    let revision = direct.document_info().unwrap().document_revision;
    let direct_outcome = direct
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: revision,
            color,
        })
        .unwrap();
    assert_eq!(wrapped_outcome, direct_outcome.dispatch());
    assert_semantically_equal(&mut wrapped, &mut direct);

    let colors = vec![
        PixelValue::Rgba([5, 10, 15, 255]),
        PixelValue::Rgba16([6, 11, 16, 65_535]),
    ];
    let wrapped_outcome = wrapped.replace_palette(&colors).unwrap();
    let revision = direct.document_info().unwrap().document_revision;
    let direct_outcome = direct
        .execute_primitive(PrimitiveRequest::ReplacePalette {
            expected_revision: revision,
            colors: colors.clone(),
        })
        .unwrap();
    assert_eq!(wrapped_outcome, direct_outcome.dispatch());
    assert_semantically_equal(&mut wrapped, &mut direct);

    let stroke = color_stroke(
        PaintTool::Pencil,
        1.0,
        StrokeSample {
            x: 70.0,
            y: 7.0,
            pressure: 1.0,
        },
    );
    let wrapped_outcome = wrapped.apply_stroke(&stroke).unwrap();
    let document = direct.document_info().unwrap();
    let direct_outcome = direct
        .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            stroke,
        })
        .unwrap();
    assert_eq!(wrapped_outcome, direct_outcome.dispatch());
    assert_semantically_equal(&mut wrapped, &mut direct);

    let wrapped_revision = wrapped.document_info().unwrap().document_revision;
    let wrapped_next = wrapped
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: wrapped_revision,
            color: PixelValue::Rgba([44, 55, 66, 255]),
        })
        .unwrap();
    let direct_revision = direct.document_info().unwrap().document_revision;
    let direct_next = direct
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: direct_revision,
            color: PixelValue::Rgba([44, 55, 66, 255]),
        })
        .unwrap();
    assert_eq!(wrapped_next.procedure().unwrap().procedure_id().get(), 4);
    assert_eq!(direct_next.procedure().unwrap().procedure_id().get(), 4);
    assert_eq!(wrapped_next.procedure(), direct_next.procedure());
    assert_semantically_equal(&mut wrapped, &mut direct);
}

#[test]
fn legacy_stroke_wrappers_validate_input_before_resolving_the_document_target() {
    let invalid = line_stroke(Vec::new());
    let mut no_document = Core::new();
    assert!(matches!(
        no_document.apply_stroke(&invalid),
        Err(CoreError::InvalidArgument(_))
    ));
    assert!(matches!(
        no_document.begin_stroke(&invalid),
        Err(CoreError::InvalidArgument(_))
    ));

    let mut noneditable = primitive_core();
    let document = noneditable.document_info().unwrap();
    let main_line = noneditable
        .layers()
        .unwrap()
        .into_iter()
        .flat_map(|layer| layer.planes)
        .find(|plane| plane.id == document.main_plane_id)
        .unwrap();
    noneditable
        .set_plane_properties(
            main_line.id,
            main_line.visible,
            false,
            main_line.opacity_milli,
            &main_line.name,
        )
        .unwrap();
    assert!(matches!(
        noneditable.apply_stroke(&invalid),
        Err(CoreError::InvalidArgument(_))
    ));
    assert!(matches!(
        noneditable.begin_stroke(&invalid),
        Err(CoreError::InvalidArgument(_))
    ));
}

#[test]
fn semantic_digest_and_execution_ignore_tile_materialization_order() {
    let mut forward = primitive_core();
    let mut reverse = primitive_core();
    for (core, points) in [
        (&mut forward, [(1.0, 1.0), (65.0, 1.0)]),
        (&mut reverse, [(65.0, 1.0), (1.0, 1.0)]),
    ] {
        for (index, (x, y)) in points.into_iter().enumerate() {
            let document = core.document_info().unwrap();
            core.execute_primitive(PrimitiveRequest::ApplyRasterStroke {
                expected_revision: document.document_revision,
                target_plane_id: document.color_plane_id,
                stroke: Stroke {
                    tool: PaintTool::Pencil,
                    plane: ActivePlane::Color,
                    color: if x < 64.0 {
                        [10, 20, 30, 255]
                    } else {
                        [40, 50, 60, 255]
                    },
                    diameter: 1.0,
                    auto_erase: false,
                    pressure_size: false,
                    coordinate_space: CoordinateSpace::Document,
                    samples: vec![StrokeSample {
                        x,
                        y,
                        pressure: 1.0,
                    }],
                },
            })
            .unwrap_or_else(|error| panic!("setup stroke {index} failed: {error}"));
        }
    }

    assert_eq!(
        forward.document_state_digest().unwrap().as_bytes(),
        reverse.document_state_digest().unwrap().as_bytes()
    );
    assert_eq!(forward.build_snapshot(), reverse.build_snapshot());

    let request_stroke = Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::Color,
        color: [70, 80, 90, 255],
        diameter: 1.0,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![StrokeSample {
            x: 10.0,
            y: 10.0,
            pressure: 1.0,
        }],
    };
    let forward_document = forward.document_info().unwrap();
    let forward_outcome = forward
        .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision: forward_document.document_revision,
            target_plane_id: forward_document.color_plane_id,
            stroke: request_stroke.clone(),
        })
        .unwrap();
    let reverse_document = reverse.document_info().unwrap();
    let reverse_outcome = reverse
        .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision: reverse_document.document_revision,
            target_plane_id: reverse_document.color_plane_id,
            stroke: request_stroke,
        })
        .unwrap();
    assert_eq!(forward_outcome.procedure(), reverse_outcome.procedure());
    assert_eq!(
        forward.document_state_digest().unwrap().as_bytes(),
        reverse.document_state_digest().unwrap().as_bytes()
    );
    assert_eq!(
        forward.document_info().unwrap(),
        reverse.document_info().unwrap()
    );
    assert_eq!(forward.build_snapshot(), reverse.build_snapshot());
}

#[test]
fn document_digest_excludes_target_view_revision_history_and_render_cache_state() {
    let mut core = primitive_core();
    execute_color_pencil(&mut core, 7.0, 9.0, [20, 40, 60, 255]);
    let primary = core.document_info().unwrap();
    let (_, alternate_layer_id) = core
        .create_layer(LayerKind::BinaryColoring, "Alternate Digest Target")
        .unwrap();
    let alternate_main_plane_id = core
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| layer.id == alternate_layer_id)
        .and_then(|layer| {
            layer
                .planes
                .into_iter()
                .find(|plane| plane.kind == PlaneType::MainLine)
        })
        .expect("the alternate coloring layer must own a main-line plane")
        .id;
    core.set_active_node(primary.layer_id, primary.color_plane_id)
        .unwrap();
    let semantic_color_checksum = core.document_info().unwrap().color_plane_checksum;
    core.set_active_node(alternate_layer_id, alternate_main_plane_id)
        .unwrap();

    let semantic_digest = document_digest(&core);
    let semantic_revision = core.document_info().unwrap().document_revision;
    let semantic_history_len = core.history_entries().len();

    core.set_active_node(primary.layer_id, primary.main_plane_id)
        .unwrap();
    assert_eq!(document_digest(&core), semantic_digest);
    core.set_active_node(primary.layer_id, primary.color_plane_id)
        .unwrap();
    assert_eq!(
        core.document_info().unwrap().active_plane,
        ActivePlane::Color
    );
    core.apply_view(ViewCommand::SetGridVisible(true)).unwrap();
    let secondary_view = core.create_view().unwrap();
    core.apply_view_for(
        secondary_view,
        ViewCommand::PanBy {
            device_dx: 13.0,
            device_dy: -8.0,
        },
    )
    .unwrap();
    let populated_snapshot = core.build_snapshot();
    assert!(populated_snapshot.tile_count() > 0);
    assert!(core.resource_usage().render_cache_tile_count > 0);
    assert_eq!(document_digest(&core), semantic_digest);

    core.set_main_line_color(PixelValue::Rgba([90, 70, 50, 255]))
        .unwrap();
    assert_ne!(document_digest(&core), semantic_digest);
    core.undo().unwrap();

    assert_eq!(
        core.document_info().unwrap().color_plane_checksum,
        semantic_color_checksum
    );
    assert!(core.document_info().unwrap().document_revision > semantic_revision);
    assert_eq!(core.history_entries().len(), semantic_history_len + 1);
    assert_eq!(core.history_cursor(), semantic_history_len);
    assert_eq!(
        core.document_info().unwrap().active_plane,
        ActivePlane::Color
    );

    let rebuilt_snapshot = core.build_snapshot();
    assert!(rebuilt_snapshot.tile_count() > 0);
    assert!(core.resource_usage().render_cache_tile_count > 0);
    assert_eq!(document_digest(&core), semantic_digest);

    core.close_view(secondary_view).unwrap();
    assert_eq!(document_digest(&core), semantic_digest);
}

#[test]
fn document_digest_changes_for_representative_semantic_document_edits() {
    let mut core = primitive_core();
    let mut previous = document_digest(&core);

    core.set_main_line_color(PixelValue::Rgba16([0x1111, 0x2222, 0x3333, 0xffff]))
        .unwrap();
    previous = assert_document_digest_changed(&core, previous, "main-line color");

    core.replace_palette(&[
        PixelValue::Rgba([3, 5, 8, 255]),
        PixelValue::Rgba16([13, 21, 34, 55]),
    ])
    .unwrap();
    previous = assert_document_digest_changed(&core, previous, "palette");

    execute_color_pencil(&mut core, 11.0, 13.0, [89, 144, 233, 255]);
    previous = assert_document_digest_changed(&core, previous, "plane raster");

    core.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 2,
            y: 3,
            width: 17,
            height: 19,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    previous = assert_document_digest_changed(&core, previous, "selection");

    core.add_guide(GuideAxis::Vertical, 23).unwrap();
    previous = assert_document_digest_changed(&core, previous, "guide");

    core.set_grid(GridConfig {
        origin_x: 3,
        origin_y: 4,
        spacing_x: 8,
        spacing_y: 12,
        subdivisions: 2,
    })
    .unwrap();
    previous = assert_document_digest_changed(&core, previous, "grid");

    core.light_table_create_set("Digest Reference").unwrap();
    previous = assert_document_digest_changed(&core, previous, "light-table topology");

    core.create_layer(LayerKind::Raster, "Digest Overlay")
        .unwrap();
    assert_document_digest_changed(&core, previous, "layer/plane topology");
}

#[test]
fn document_digest_ignores_different_edit_counts_for_the_same_final_raster() {
    let mut direct = primitive_core();
    let mut overwritten = primitive_core();
    let final_color = [101, 151, 201, 255];

    execute_color_pencil(&mut direct, 31.0, 17.0, final_color);
    execute_color_pencil(&mut overwritten, 31.0, 17.0, [7, 11, 13, 255]);
    execute_color_pencil(&mut overwritten, 31.0, 17.0, final_color);

    assert_eq!(
        direct.plane_pixel(ActivePlane::Color, 31, 17).unwrap(),
        overwritten.plane_pixel(ActivePlane::Color, 31, 17).unwrap()
    );
    assert_eq!(
        direct.document_info().unwrap().color_plane_checksum,
        overwritten.document_info().unwrap().color_plane_checksum
    );
    assert_ne!(
        direct.document_info().unwrap().document_revision,
        overwritten.document_info().unwrap().document_revision
    );
    assert_ne!(
        direct.history_entries().len(),
        overwritten.history_entries().len()
    );
    assert_eq!(document_digest(&direct), document_digest(&overwritten));
}

#[test]
fn document_digest_excludes_light_table_source_provenance() {
    let mut first = primitive_core();
    let mut second = primitive_core();
    let pixels = vec![
        10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
    ];
    let reference_frame = RectI32 {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    };
    let make_source = |document_uuid, source_revision, dpi_x_milli, dpi_y_milli| {
        LightTableSource::from_rgba_bytes(
            document_uuid,
            source_revision,
            reference_frame,
            RgbaRasterBytes {
                width: 2,
                height: 2,
                pixel_format: PixelFormat::StraightRgba8,
                dpi_x_milli: Some(dpi_x_milli),
                dpi_y_milli: Some(dpi_y_milli),
                pixels: pixels.clone(),
            },
        )
        .unwrap()
    };

    let (_, first_item_id) = first
        .light_table_add_item(LightTableItemInput::new(
            "Digest Source",
            make_source(0x1111, 7, 72_000, 96_000),
        ))
        .unwrap();
    let (_, second_item_id) = second
        .light_table_add_item(LightTableItemInput::new(
            "Digest Source",
            make_source(0x2222, 19, 300_000, 144_000),
        ))
        .unwrap();

    assert_eq!(first_item_id, second_item_id);
    assert_eq!(
        first.light_table_sample(64, 32).unwrap(),
        second.light_table_sample(64, 32).unwrap()
    );
    assert_eq!(document_digest(&first), document_digest(&second));
}

#[test]
fn document_digest_excludes_light_table_reference_frame_extent() {
    let mut first = primitive_core();
    let mut second = primitive_core();
    let pixels = vec![
        10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
    ];
    let make_source = |width, height| {
        LightTableSource::from_rgba_bytes(
            0x1111,
            7,
            RectI32 {
                x: 0,
                y: 0,
                width,
                height,
            },
            RgbaRasterBytes {
                width: 2,
                height: 2,
                pixel_format: PixelFormat::StraightRgba8,
                dpi_x_milli: Some(DEFAULT_DPI_MILLI),
                dpi_y_milli: Some(DEFAULT_DPI_MILLI),
                pixels: pixels.clone(),
            },
        )
        .unwrap()
    };

    let (_, first_item_id) = first
        .light_table_add_item(LightTableItemInput::new(
            "Extent-independent Source",
            make_source(2, 2),
        ))
        .unwrap();
    let (_, second_item_id) = second
        .light_table_add_item(LightTableItemInput::new(
            "Extent-independent Source",
            make_source(200, 300),
        ))
        .unwrap();

    assert_eq!(first_item_id, second_item_id);
    assert_eq!(
        first.light_table_sample(64, 32).unwrap(),
        second.light_table_sample(64, 32).unwrap(),
        "reference-frame extent does not affect Light Table sampling"
    );
    assert_eq!(document_digest(&first), document_digest(&second));
}

#[test]
fn document_digest_includes_light_table_reference_frame_alignment() {
    let mut first = primitive_core();
    let mut second = primitive_core();
    let pixels = vec![
        10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
    ];
    let make_source = |reference_frame| {
        LightTableSource::from_rgba_bytes(
            0x1111,
            7,
            reference_frame,
            RgbaRasterBytes {
                width: 2,
                height: 2,
                pixel_format: PixelFormat::StraightRgba8,
                dpi_x_milli: Some(DEFAULT_DPI_MILLI),
                dpi_y_milli: Some(DEFAULT_DPI_MILLI),
                pixels: pixels.clone(),
            },
        )
        .unwrap()
    };

    first
        .light_table_add_item(LightTableItemInput::new(
            "Aligned Source",
            make_source(RectI32 {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            }),
        ))
        .unwrap();
    second
        .light_table_add_item(LightTableItemInput::new(
            "Aligned Source",
            make_source(RectI32 {
                x: 1,
                y: 0,
                width: 2,
                height: 2,
            }),
        ))
        .unwrap();

    let destination_reference = first.document_info().unwrap().frames.reference_frame;
    let x = u32::try_from(destination_reference.x).unwrap();
    let y = u32::try_from(destination_reference.y).unwrap();
    assert_ne!(
        first.light_table_sample(x, y).unwrap(),
        second.light_table_sample(x, y).unwrap(),
        "source reference-frame origin changes Light Table sampling"
    );
    assert_ne!(document_digest(&first), document_digest(&second));
}

#[test]
fn primitive_cache_invalidation_is_scoped_and_published_once() {
    let mut core = primitive_core();
    for (x, color) in [(1.0, [10, 20, 30, 255]), (65.0, [40, 50, 60, 255])] {
        execute_color_pencil(&mut core, x, 1.0, color);
    }
    let before = tile_revisions(&core.build_snapshot());
    assert_eq!(before.len(), 2);

    let revision = core.document_info().unwrap().document_revision;
    core.execute_primitive(PrimitiveRequest::ReplacePalette {
        expected_revision: revision,
        colors: vec![PixelValue::Rgba([1, 2, 3, 255])],
    })
    .unwrap();
    assert_eq!(tile_revisions(&core.build_snapshot()), before);

    execute_color_pencil(&mut core, 1.0, 1.0, [90, 80, 70, 255]);
    let after_stroke = tile_revisions(&core.build_snapshot());
    assert_ne!(after_stroke[&(0, 0)], before[&(0, 0)]);
    assert_eq!(after_stroke[&(64, 0)], before[&(64, 0)]);
    assert_eq!(tile_revisions(&core.build_snapshot()), after_stroke);

    let revision = core.document_info().unwrap().document_revision;
    core.execute_primitive(PrimitiveRequest::SetMainLineColor {
        expected_revision: revision,
        color: PixelValue::Rgba([90, 40, 20, 255]),
    })
    .unwrap();
    let after_main_line = tile_revisions(&core.build_snapshot());
    assert_ne!(after_main_line[&(0, 0)], after_stroke[&(0, 0)]);
    assert_ne!(after_main_line[&(64, 0)], after_stroke[&(64, 0)]);
    assert_eq!(tile_revisions(&core.build_snapshot()), after_main_line);
}

fn execute_color_pencil(core: &mut Core, x: f32, y: f32, color: [u8; 4]) {
    let document = core.document_info().unwrap();
    core.execute_primitive(PrimitiveRequest::ApplyRasterStroke {
        expected_revision: document.document_revision,
        target_plane_id: document.color_plane_id,
        stroke: Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::Color,
            color,
            diameter: 1.0,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples: vec![StrokeSample {
                x,
                y,
                pressure: 1.0,
            }],
        },
    })
    .unwrap();
}

fn assert_document_digest_changed(core: &Core, before: [u8; 32], field: &str) -> [u8; 32] {
    let after = document_digest(core);
    assert_ne!(after, before, "{field} must contribute to document digest");
    after
}

fn tile_revisions(snapshot: &RenderSnapshot) -> BTreeMap<(i32, i32), u64> {
    snapshot
        .tiles()
        .iter()
        .map(|tile| ((tile.origin_x(), tile.origin_y()), tile.tile_revision()))
        .collect()
}
