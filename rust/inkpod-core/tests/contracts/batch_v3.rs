use super::*;
use crate::{PaintTool, Stroke, StrokeSample};

fn target(kind: PlaneType) -> BatchTargetSelector {
    semantic_target(LayerKind::BinaryColoring, kind)
}

fn semantic_target(layer_kind: LayerKind, plane_kind: PlaneType) -> BatchTargetSelector {
    BatchTargetSelector {
        layer_id: None,
        plane_id: None,
        layer_kind: Some(layer_kind),
        plane_kind: Some(plane_kind),
        missing_policy: BatchMissingTargetPolicy::Error,
    }
}

fn operation(target_kind: PlaneType, kind: BatchOperationKind) -> BatchOperation {
    BatchOperation {
        version: BATCH_OPERATION_VERSION,
        enabled: true,
        target: target(target_kind),
        additional_targets: Vec::new(),
        kind,
    }
}

fn exact_operation(
    layer_id: u64,
    plane_id: u64,
    plane_kind: PlaneType,
    kind: BatchOperationKind,
) -> BatchOperation {
    BatchOperation {
        version: BATCH_OPERATION_VERSION,
        enabled: true,
        target: BatchTargetSelector {
            layer_id: Some(layer_id),
            plane_id: Some(plane_id),
            layer_kind: None,
            plane_kind: Some(plane_kind),
            missing_policy: BatchMissingTargetPolicy::Error,
        },
        additional_targets: Vec::new(),
        kind,
    }
}

fn native_plane(format: PixelFormat, uuid: u128) -> (Core, u64, u64, PlaneType, EditorTarget) {
    let mut core = Core::new();
    let document = core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
        .unwrap();
    let (plane_id, plane_kind) = match format {
        PixelFormat::BinaryMask8 => (document.main_plane_id, PlaneType::MainLine),
        PixelFormat::Grayscale8 | PixelFormat::Grayscale16 => {
            core.convert_layer(document.layer_id, LayerKind::GrayscaleColoring)
                .unwrap();
            if format == PixelFormat::Grayscale16 {
                core.convert_plane(document.main_plane_id, PlaneType::MainLine, format)
                    .unwrap();
            }
            (document.main_plane_id, PlaneType::MainLine)
        }
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16 => {
            let (_, plane_id) = core
                .create_plane(
                    document.layer_id,
                    PlaneType::Raster,
                    format,
                    "Batch native target",
                )
                .unwrap();
            (plane_id, PlaneType::Raster)
        }
        PixelFormat::PremultipliedBgra8 => panic!("display-only format is not canonical"),
    };
    (
        core,
        document.layer_id,
        plane_id,
        plane_kind,
        EditorTarget {
            layer_id: document.layer_id,
            plane_id,
        },
    )
}

fn fill_native(
    core: &mut Core,
    target: EditorTarget,
    color: PixelValue,
    selection: Option<RectI32>,
) {
    core.apply_fill_for_editor_target(
        &FillRequest {
            operation: FillOperation::Seed,
            seed_x: selection.map_or(0, |rect| rect.x.max(0) as u32),
            seed_y: selection.map_or(0, |rect| rect.y.max(0) as u32),
            color,
            selection,
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

fn dot(core: &mut Core, color: [u8; 4], x: f32, y: f32) {
    core.apply_stroke(&Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::Color,
        color,
        diameter: 1.0,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![StrokeSample {
            x,
            y,
            pressure: 1.0,
        }],
    })
    .unwrap();
}

#[test]
fn batch_v4_catalog_is_closed_and_disabled_only_graph_is_invalid() {
    let graph = BatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "v4".to_owned(),
        inputs: vec![BatchInputSelector::active_document()],
        operations: vec![BatchOperation {
            enabled: false,
            ..operation(
                PlaneType::Color,
                BatchOperationKind::Erase(vec![PixelValue::Rgba([1, 2, 3, 4])]),
            )
        }],
        output: BatchOutputSettings {
            destination: BatchOutputDestination::ActiveDocument,
            ..BatchOutputSettings::default()
        },
    };
    assert_eq!(BATCH_GRAPH_VERSION, 4);
    assert_eq!(BATCH_OPERATION_VERSION, 3);
    assert!(graph.validate().is_err());

    let kinds = [
        BatchOperationKind::ColorReplace(vec![BatchColorPair {
            enabled: true,
            old: PixelValue::Rgba([1, 2, 3, 4]),
            new: PixelValue::Rgba([4, 3, 2, 1]),
        }]),
        BatchOperationKind::MoveToColorPlane(vec![PixelValue::Rgba([1, 2, 3, 4])]),
        BatchOperationKind::Masking(vec![PixelValue::Rgba([1, 2, 3, 4])]),
        BatchOperationKind::Erase(vec![PixelValue::Rgba([1, 2, 3, 4])]),
    ];
    assert_eq!(kinds.len(), 4);
}

#[test]
fn one_color_replace_operation_updates_all_selected_layers_as_one_undo_unit() {
    let mut core = Core::new();
    let document = core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xb304)
        .unwrap();
    let (_, second_binary) = core
        .create_layer(LayerKind::BinaryColoring, "Binary 2")
        .unwrap();
    let (_, grayscale) = core
        .create_layer(LayerKind::GrayscaleColoring, "Grayscale")
        .unwrap();
    let (_, raster) = core.create_layer(LayerKind::Raster, "Raster").unwrap();
    let plane = |core: &Core, layer_id: u64, kind: PlaneType| {
        core.layers()
            .unwrap()
            .into_iter()
            .find(|layer| layer.id == layer_id)
            .and_then(|layer| layer.planes.into_iter().find(|plane| plane.kind == kind))
            .map(|plane| plane.id)
            .unwrap()
    };
    let targets = [
        EditorTarget {
            layer_id: document.layer_id,
            plane_id: document.color_plane_id,
        },
        EditorTarget {
            layer_id: second_binary,
            plane_id: plane(&core, second_binary, PlaneType::Color),
        },
        EditorTarget {
            layer_id: grayscale,
            plane_id: plane(&core, grayscale, PlaneType::Color),
        },
        EditorTarget {
            layer_id: raster,
            plane_id: plane(&core, raster, PlaneType::Raster),
        },
    ];
    let old = PixelValue::Rgba([10, 20, 30, 40]);
    let new = PixelValue::Rgba([90, 80, 70, 60]);
    for target in targets {
        fill_native(&mut core, target, old, None);
    }
    let before = core.document_state_digest().unwrap();
    let history_before = core.history_entries().len();
    let procedures_before = core.persistence_info().unwrap().procedure_count;
    let operation = BatchOperation {
        version: BATCH_OPERATION_VERSION,
        enabled: true,
        target: semantic_target(LayerKind::Raster, PlaneType::Raster),
        additional_targets: vec![
            semantic_target(LayerKind::BinaryColoring, PlaneType::Color),
            semantic_target(LayerKind::GrayscaleColoring, PlaneType::Color),
        ],
        kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
            enabled: true,
            old,
            new,
        }]),
    };
    core.apply_batch_operations(std::slice::from_ref(&operation), || false)
        .unwrap();
    let after = core.document_state_digest().unwrap();
    assert_ne!(after, before);
    assert_eq!(core.history_entries().len(), history_before + 1);
    assert_eq!(
        core.persistence_info().unwrap().procedure_count,
        procedures_before + 1
    );
    assert_eq!(
        core.verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        after
    );
    core.undo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), before);
    core.redo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), after);

    let mut previous = after;
    for target in targets {
        core.apply_batch_operations(
            &[exact_operation(
                target.layer_id,
                target.plane_id,
                if target.layer_id == raster {
                    PlaneType::Raster
                } else {
                    PlaneType::Color
                },
                BatchOperationKind::ColorReplace(vec![BatchColorPair {
                    enabled: true,
                    old: new,
                    new: old,
                }]),
            )],
            || false,
        )
        .unwrap();
        let current = core.document_state_digest().unwrap();
        assert_ne!(current, previous, "selected target was not replaced");
        previous = current;
    }
    assert_eq!(previous, before);
}

#[test]
fn masking_is_sparse_persistent_undoable_and_a_hard_fill_boundary() {
    let mut core = Core::new();
    core.new_cell(130, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    dot(&mut core, [7, 8, 9, 255], 64.0, 0.0);
    let before = core.document_info().unwrap();
    core.apply_batch_operations(
        &[operation(
            PlaneType::Color,
            BatchOperationKind::Masking(vec![PixelValue::Rgba([7, 8, 9, 255])]),
        )],
        || false,
    )
    .unwrap();
    let mask = core.fill_protection_mask_info().unwrap();
    assert_eq!(mask.allocated_tile_count, 1);
    assert_eq!(mask.wall_pixel_count, 1);
    assert_eq!(
        core.document_info().unwrap().document_revision,
        before.document_revision + 1
    );

    let fill = FillRequest {
        operation: FillOperation::Seed,
        seed_x: 0,
        seed_y: 0,
        color: PixelValue::Rgba([20, 40, 60, 255]),
        selection: None,
        use_document_selection: false,
        tolerance: 0,
        detached_regions: false,
        overflow_abort: false,
        gap_close: 0,
        transparent_only: false,
        inclusion_mode: InclusionMode::None,
        inclusion_colors: Vec::new(),
        extension_distance: 0,
    };
    core.apply_fill(&fill).unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 63, 0).unwrap(),
        PixelValue::Rgba([20, 40, 60, 255])
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 65, 0).unwrap(),
        PixelValue::Rgba([0; 4])
    );

    let path = temp_path("fill-protection-v28.inkpod");
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.fill_protection_mask_info().unwrap(), mask);
    reopened.undo().unwrap();
    reopened.undo().unwrap();
    assert_eq!(
        reopened
            .fill_protection_mask_info()
            .unwrap()
            .wall_pixel_count,
        0
    );
    reopened.redo().unwrap();
    assert_eq!(
        reopened
            .fill_protection_mask_info()
            .unwrap()
            .wall_pixel_count,
        1
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn erase_and_replace_are_exact_alpha_aware_and_semantic_noops_are_stable() {
    let mut core = Core::new();
    core.new_cell(3, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    dot(&mut core, [10, 20, 30, 40], 0.0, 0.0);
    dot(&mut core, [10, 20, 30, 41], 1.0, 0.0);
    core.apply_batch_operations(
        &[operation(
            PlaneType::Color,
            BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([10, 20, 30, 40]),
                new: PixelValue::Rgba([90, 80, 70, 60]),
            }]),
        )],
        || false,
    )
    .unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([90, 80, 70, 60])
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 1, 0).unwrap(),
        PixelValue::Rgba([10, 20, 30, 41])
    );

    let before_noop = core.document_info().unwrap();
    core.apply_batch_operations(
        &[operation(
            PlaneType::Color,
            BatchOperationKind::Erase(vec![PixelValue::Rgba([1, 1, 1, 1])]),
        )],
        || false,
    )
    .unwrap();
    assert_eq!(core.document_info().unwrap(), before_noop);
}

#[test]
fn replace_mask_and_erase_use_each_native_pixel_format_without_conversion() {
    let cases = [
        (
            PixelFormat::BinaryMask8,
            PixelValue::Binary(255),
            PixelValue::Binary(0),
            PixelValue::Binary(0),
        ),
        (
            PixelFormat::Grayscale8,
            PixelValue::Grayscale8(91),
            PixelValue::Grayscale8(17),
            PixelValue::Grayscale8(0),
        ),
        (
            PixelFormat::Grayscale16,
            PixelValue::Grayscale16(32_896),
            PixelValue::Grayscale16(4_369),
            PixelValue::Grayscale16(0),
        ),
        (
            PixelFormat::StraightRgba8,
            PixelValue::Rgba([10, 20, 30, 40]),
            PixelValue::Rgba([40, 30, 20, 10]),
            PixelValue::Rgba([0; 4]),
        ),
        (
            PixelFormat::StraightRgba16,
            PixelValue::Rgba16([257, 514, 771, 1_028]),
            PixelValue::Rgba16([1_028, 771, 514, 257]),
            PixelValue::Rgba16([0; 4]),
        ),
    ];
    for (index, (format, old, new, empty)) in cases.into_iter().enumerate() {
        let (mut core, layer_id, plane_id, plane_kind, _target) =
            native_plane(format, 0xb300 + index as u128);
        core.apply_batch_operations(
            &[exact_operation(
                layer_id,
                plane_id,
                plane_kind,
                BatchOperationKind::ColorReplace(vec![BatchColorPair {
                    enabled: true,
                    old: empty,
                    new: old,
                }]),
            )],
            || false,
        )
        .unwrap();
        let baseline = core.document_state_digest().unwrap();

        core.apply_batch_operations(
            &[exact_operation(
                layer_id,
                plane_id,
                plane_kind,
                BatchOperationKind::ColorReplace(vec![BatchColorPair {
                    enabled: true,
                    old,
                    new,
                }]),
            )],
            || false,
        )
        .unwrap();
        core.apply_batch_operations(
            &[exact_operation(
                layer_id,
                plane_id,
                plane_kind,
                BatchOperationKind::ColorReplace(vec![BatchColorPair {
                    enabled: true,
                    old: new,
                    new: old,
                }]),
            )],
            || false,
        )
        .unwrap();
        assert_eq!(
            core.document_state_digest().unwrap(),
            baseline,
            "replace changed native semantics for {format:?}"
        );

        core.apply_batch_operations(
            &[exact_operation(
                layer_id,
                plane_id,
                plane_kind,
                BatchOperationKind::Masking(vec![old]),
            )],
            || false,
        )
        .unwrap();
        assert_eq!(
            core.fill_protection_mask_info().unwrap().wall_pixel_count,
            2
        );
        core.apply_batch_operations(
            &[exact_operation(
                layer_id,
                plane_id,
                plane_kind,
                BatchOperationKind::Masking(vec![new]),
            )],
            || false,
        )
        .unwrap();
        assert_eq!(
            core.fill_protection_mask_info().unwrap().wall_pixel_count,
            0
        );
        assert_eq!(
            core.document_state_digest().unwrap(),
            baseline,
            "masking changed the source raster for {format:?}"
        );

        core.apply_batch_operations(
            &[exact_operation(
                layer_id,
                plane_id,
                plane_kind,
                BatchOperationKind::Erase(vec![old]),
            )],
            || false,
        )
        .unwrap();
        core.apply_batch_operations(
            &[exact_operation(
                layer_id,
                plane_id,
                plane_kind,
                BatchOperationKind::ColorReplace(vec![BatchColorPair {
                    enabled: true,
                    old: empty,
                    new: old,
                }]),
            )],
            || false,
        )
        .unwrap();
        assert_eq!(
            core.document_state_digest().unwrap(),
            baseline,
            "erase did not use the exact empty value for {format:?}"
        );

        let before_invalid = core.document_info().unwrap();
        let wrong_depth = if format == PixelFormat::StraightRgba8 {
            PixelValue::Rgba16([1; 4])
        } else {
            PixelValue::Rgba([1; 4])
        };
        assert!(
            core.apply_batch_operations(
                &[exact_operation(
                    layer_id,
                    plane_id,
                    plane_kind,
                    BatchOperationKind::Erase(vec![wrong_depth]),
                )],
                || false,
            )
            .is_err()
        );
        assert_eq!(core.document_info().unwrap(), before_invalid);
    }
}

#[test]
fn ordered_operations_are_deterministic_and_commit_as_one_replayable_undo_unit() {
    let make_core = || {
        let mut core = Core::new();
        core.new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xb340)
            .unwrap();
        dot(&mut core, [1, 2, 3, 4], 0.0, 0.0);
        core
    };
    let operations = vec![
        operation(
            PlaneType::Color,
            BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([1, 2, 3, 4]),
                new: PixelValue::Rgba([4, 3, 2, 1]),
            }]),
        ),
        operation(
            PlaneType::Color,
            BatchOperationKind::Erase(vec![PixelValue::Rgba([4, 3, 2, 1])]),
        ),
    ];
    let mut first = make_core();
    let mut second = make_core();
    let before = first.document_state_digest().unwrap();
    let history_before = first.history_entries().len();
    let procedures_before = first.persistence_info().unwrap().procedure_count;
    first.apply_batch_operations(&operations, || false).unwrap();
    second
        .apply_batch_operations(&operations, || false)
        .unwrap();
    let after = first.document_state_digest().unwrap();
    assert_eq!(second.document_state_digest().unwrap(), after);
    assert_ne!(after, before);
    assert_eq!(first.history_entries().len(), history_before + 1);
    assert_eq!(
        first.persistence_info().unwrap().procedure_count,
        procedures_before + 1
    );
    assert_eq!(
        first
            .verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        after
    );
    first.undo().unwrap();
    assert_eq!(first.document_state_digest().unwrap(), before);
    first.redo().unwrap();
    assert_eq!(first.document_state_digest().unwrap(), after);

    let mut reversed = make_core();
    reversed
        .apply_batch_operations(&[operations[1].clone(), operations[0].clone()], || false)
        .unwrap();
    assert_ne!(reversed.document_state_digest().unwrap(), after);
}

#[test]
fn cancellation_hidden_targets_and_protected_main_line_are_atomic() {
    let mut core = Core::new();
    let document = core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xb350)
        .unwrap();
    dot(&mut core, [3, 4, 5, 6], 0.0, 0.0);
    let erase = operation(
        PlaneType::Color,
        BatchOperationKind::Erase(vec![PixelValue::Rgba([3, 4, 5, 6])]),
    );
    let before_cancel = core.document_info().unwrap();
    let digest_before_cancel = core.document_state_digest().unwrap();
    let journal_before_cancel = core.journal_entries().to_vec();
    assert_eq!(
        core.apply_batch_operations(std::slice::from_ref(&erase), || true),
        Err(CoreError::Cancelled)
    );
    assert_eq!(core.document_info().unwrap(), before_cancel);
    assert_eq!(core.document_state_digest().unwrap(), digest_before_cancel);
    assert_eq!(core.journal_entries(), journal_before_cancel);

    core.set_plane_properties(document.color_plane_id, false, true, 1_000, "Color")
        .unwrap();
    let hidden = core.document_info().unwrap();
    let hidden_digest = core.document_state_digest().unwrap();
    assert!(core.apply_batch_operations(&[erase], || false).is_err());
    assert_eq!(core.document_info().unwrap(), hidden);
    assert_eq!(core.document_state_digest().unwrap(), hidden_digest);

    let protected = exact_operation(
        document.layer_id,
        document.main_plane_id,
        PlaneType::MainLine,
        BatchOperationKind::MoveToColorPlane(vec![PixelValue::Binary(0)]),
    );
    let protected_before = core.document_info().unwrap();
    assert!(core.apply_batch_operations(&[protected], || false).is_err());
    assert_eq!(core.document_info().unwrap(), protected_before);
}

#[test]
fn move_to_color_plane_moves_exact_pixels_and_preserves_other_destination_pixels() {
    for (index, (format, source_color, destination_color)) in [
        (
            PixelFormat::StraightRgba8,
            PixelValue::Rgba([12, 34, 56, 78]),
            PixelValue::Rgba([8, 7, 6, 5]),
        ),
        (
            PixelFormat::StraightRgba16,
            PixelValue::Rgba16([257, 514, 771, 1_028]),
            PixelValue::Rgba16([2_056, 1_799, 1_542, 1_285]),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let (mut core, layer_id, source_id, source_kind, source_target) =
            native_plane(format, 0xb360 + index as u128);
        assert_eq!(source_kind, PlaneType::Raster);
        let destination_id = core
            .layers()
            .unwrap()
            .iter()
            .find(|layer| layer.id == layer_id)
            .unwrap()
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Color)
            .unwrap()
            .id;
        if format == PixelFormat::StraightRgba16 {
            core.convert_plane(
                destination_id,
                PlaneType::Color,
                PixelFormat::StraightRgba16,
            )
            .unwrap();
        }
        let destination_target = EditorTarget {
            layer_id,
            plane_id: destination_id,
        };
        fill_native(&mut core, destination_target, destination_color, None);
        fill_native(
            &mut core,
            source_target,
            source_color,
            Some(RectI32 {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
        );
        let before = core.document_state_digest().unwrap();
        let history_before = core.history_entries().len();
        let move_operation = exact_operation(
            layer_id,
            source_id,
            PlaneType::Raster,
            BatchOperationKind::MoveToColorPlane(vec![source_color]),
        );
        core.apply_batch_operations(&[move_operation], || false)
            .unwrap();
        assert_eq!(core.history_entries().len(), history_before + 1);

        core.apply_batch_operations(
            &[exact_operation(
                layer_id,
                destination_id,
                PlaneType::Color,
                BatchOperationKind::Masking(vec![source_color]),
            )],
            || false,
        )
        .unwrap();
        assert_eq!(
            core.fill_protection_mask_info().unwrap().wall_pixel_count,
            1
        );
        core.undo().unwrap();
        core.apply_batch_operations(
            &[exact_operation(
                layer_id,
                source_id,
                PlaneType::Raster,
                BatchOperationKind::Masking(vec![source_color]),
            )],
            || false,
        )
        .unwrap();
        assert_eq!(
            core.fill_protection_mask_info().unwrap().wall_pixel_count,
            0
        );
        core.undo().unwrap();
        assert_eq!(core.document_state_digest().unwrap(), before);
    }
}

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("inkpod-{}-{name}", std::process::id()))
}
