use super::*;

fn core_with_raster_layer(uuid: u128) -> Core {
    let mut core = Core::new();
    core.new_cell_with_uuid(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
        .unwrap();
    core
}

fn plane_target(core: &Core, layer_index: usize, plane_index: usize) -> EditTarget {
    let layers = core.layers().unwrap();
    EditTarget::Plane(EditorTarget {
        layer_id: layers[layer_index].id,
        plane_id: layers[layer_index].planes[plane_index].id,
    })
}

#[test]
fn edit_target_set_is_bounded_tree_ordered_editor_state_and_reopens() {
    let mut core = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_0001);
    let (_, raster_layer) = core.create_layer(LayerKind::Raster, "Paint").unwrap();
    let raster_plane = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == raster_layer)
        .unwrap()
        .planes[0]
        .id;
    let main = plane_target(&core, 0, 0);
    let color = plane_target(&core, 0, 1);
    let raster = EditTarget::Plane(EditorTarget {
        layer_id: raster_layer,
        plane_id: raster_plane,
    });
    let active_before = core.editor_state().unwrap().state.target;
    let document_before = core.document_info().unwrap();
    let history_before = core.history_entries().len();
    let journal_before = core.journal_entries().len();

    let editor = core.editor_state().unwrap();
    let changed = core
        .update_editor_state(
            editor.revision,
            EditorStateUpdate::SetEditTargets(vec![raster, color, main]),
        )
        .unwrap();
    assert_eq!(changed.state.edit_targets, vec![main, color, raster]);
    assert_eq!(changed.state.target, active_before);
    assert_eq!(changed.revision.get(), editor.revision.get() + 1);
    assert!(changed.dirty);
    let document_after = core.document_info().unwrap();
    assert_eq!(
        document_after.document_revision,
        document_before.document_revision
    );
    assert_eq!(core.history_entries().len(), history_before);
    assert_eq!(core.journal_entries().len(), journal_before);

    let before_invalid = core.editor_state().unwrap();
    assert!(matches!(
        core.update_editor_state(
            before_invalid.revision,
            EditorStateUpdate::SetEditTargets(vec![EditTarget::Plane(EditorTarget {
                layer_id: raster_layer,
                plane_id: u64::MAX,
            })]),
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.editor_state().unwrap(), before_invalid);

    let sequence = TEST_PATH_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("inkpod-multi-target-{sequence}.inkpod"));
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(
        reopened.editor_state().unwrap().state.edit_targets,
        vec![main, color, raster]
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn main_and_color_copy_paste_is_one_coordinate_preserving_undo_unit() {
    let mut source = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_0002);
    source
        .apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 2,
                y: 3,
                width: 1,
                height: 1,
            }),
            SelectionOperation::New,
        )
        .unwrap();
    source.set_active_plane(ActivePlane::MainLine).unwrap();
    source
        .apply_stroke(&line_stroke(vec![StrokeSample {
            x: 2.0,
            y: 3.0,
            pressure: 1.0,
        }]))
        .unwrap();
    source.set_active_plane(ActivePlane::Color).unwrap();
    source
        .apply_stroke(&color_stroke(
            PaintTool::Pencil,
            1.0,
            StrokeSample {
                x: 2.0,
                y: 3.0,
                pressure: 1.0,
            },
        ))
        .unwrap();
    let main = plane_target(&source, 0, 0);
    let color = plane_target(&source, 0, 1);
    let editor = source.editor_state().unwrap();
    source
        .update_editor_state(
            editor.revision,
            EditorStateUpdate::SetEditTargets(vec![color, main]),
        )
        .unwrap();
    let payload = source.copy_selection().unwrap();
    assert_eq!(
        payload.bounds,
        RectI32 {
            x: 2,
            y: 3,
            width: 1,
            height: 1
        }
    );
    assert_eq!(
        payload
            .planes
            .iter()
            .map(|plane| plane.kind)
            .collect::<Vec<_>>(),
        vec![PlaneType::MainLine, PlaneType::Color]
    );

    let mut destination = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_0003);
    let history_before = destination.history_entries().len();
    let document_before_cancel = destination.document_info().unwrap();
    destination.begin_paste(&payload).unwrap();
    destination.cancel_floating();
    assert_eq!(destination.document_info().unwrap(), document_before_cancel);
    assert_eq!(destination.history_entries().len(), history_before);
    destination.begin_paste(&payload).unwrap();
    destination.commit_floating().unwrap();
    assert_eq!(destination.history_entries().len(), history_before + 1);
    assert_eq!(
        destination
            .plane_pixel(ActivePlane::MainLine, 2, 3)
            .unwrap(),
        PixelValue::Binary(255)
    );
    assert_eq!(
        destination.plane_pixel(ActivePlane::Color, 2, 3).unwrap(),
        PixelValue::Rgba([12, 34, 56, 255])
    );
    destination.undo().unwrap();
    assert!(
        destination
            .plane_pixel(ActivePlane::MainLine, 2, 3)
            .unwrap()
            .is_zero()
    );
    assert!(
        destination
            .plane_pixel(ActivePlane::Color, 2, 3)
            .unwrap()
            .is_zero()
    );
    destination.redo().unwrap();
    assert_eq!(
        destination.plane_pixel(ActivePlane::Color, 2, 3).unwrap(),
        PixelValue::Rgba([12, 34, 56, 255])
    );
    destination.verify_journal_replay().unwrap();
}

#[test]
fn vector_layer_copy_paste_preserves_cross_plane_fill_topology_in_one_undo_unit() {
    let mut source = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_0006);
    let (_, layer_id) = source
        .create_layer(LayerKind::VectorColoring, "Vector source")
        .unwrap();
    let (_, trace_id, fill_id) = source.vector_layer_planes(layer_id).unwrap();
    let path = VectorPathInput {
        segments: vec![
            VectorCubicSegment {
                p0: PointF32 { x: 1.0, y: 1.0 },
                p1: PointF32 { x: 3.0, y: 1.0 },
                p2: PointF32 { x: 5.0, y: 1.0 },
                p3: PointF32 { x: 7.0, y: 1.0 },
                width_start: 1.0,
                width_end: 1.0,
            },
            VectorCubicSegment {
                p0: PointF32 { x: 7.0, y: 1.0 },
                p1: PointF32 { x: 7.0, y: 3.0 },
                p2: PointF32 { x: 7.0, y: 5.0 },
                p3: PointF32 { x: 7.0, y: 7.0 },
                width_start: 1.0,
                width_end: 1.0,
            },
            VectorCubicSegment {
                p0: PointF32 { x: 7.0, y: 7.0 },
                p1: PointF32 { x: 5.0, y: 7.0 },
                p2: PointF32 { x: 3.0, y: 7.0 },
                p3: PointF32 { x: 1.0, y: 7.0 },
                width_start: 1.0,
                width_end: 1.0,
            },
            VectorCubicSegment {
                p0: PointF32 { x: 1.0, y: 7.0 },
                p1: PointF32 { x: 1.0, y: 5.0 },
                p2: PointF32 { x: 1.0, y: 3.0 },
                p3: PointF32 { x: 1.0, y: 1.0 },
                width_start: 1.0,
                width_end: 1.0,
            },
        ],
        color: PixelValue::Rgba([10, 20, 30, 255]),
        closed: true,
    };
    let (_, path_id) = source.vector_add_path(trace_id, path).unwrap();
    source
        .vector_add_fill(fill_id, &[path_id], PixelValue::Rgba([40, 50, 60, 255]))
        .unwrap();
    source
        .apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            }),
            SelectionOperation::New,
        )
        .unwrap();
    let editor = source.editor_state().unwrap();
    source
        .update_editor_state(
            editor.revision,
            EditorStateUpdate::SetEditTargets(vec![EditTarget::Layer(layer_id)]),
        )
        .unwrap();
    let payload = source.copy_selection().unwrap();
    assert_eq!(
        payload
            .planes
            .iter()
            .map(|plane| plane.vector_paths.len())
            .sum::<usize>(),
        1
    );
    assert_eq!(
        payload
            .planes
            .iter()
            .map(|plane| plane.vector_fills.len())
            .sum::<usize>(),
        1
    );

    let mut destination = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_0007);
    destination
        .create_layer(LayerKind::VectorColoring, "Vector destination")
        .unwrap();
    let history_before = destination.history_entries().len();
    destination.begin_paste(&payload).unwrap();
    destination.commit_floating().unwrap();
    assert_eq!(destination.history_entries().len(), history_before + 1);
    let paths = destination.vector_paths().unwrap();
    let fills = destination.vector_fills().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].boundary_path_ids, vec![paths[0].id]);
    destination.undo().unwrap();
    assert!(destination.vector_paths().unwrap().is_empty());
    assert!(destination.vector_fills().unwrap().is_empty());
    destination.redo().unwrap();
    assert_eq!(destination.vector_fills().unwrap().len(), 1);
    destination.verify_journal_replay().unwrap();
}

#[test]
fn grouped_rgba16_clipboard_keeps_exact_depth_order_and_origin() {
    let mut source = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_0009);
    let (_, layer_id) = source
        .create_layer(LayerKind::Raster, "16-bit source")
        .unwrap();
    let (_, first) = source
        .create_plane(
            layer_id,
            PlaneType::Raster,
            PixelFormat::StraightRgba16,
            "16 A",
        )
        .unwrap();
    let (_, second) = source
        .create_plane(
            layer_id,
            PlaneType::Raster,
            PixelFormat::StraightRgba16,
            "16 B",
        )
        .unwrap();
    source
        .apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 2,
                y: 3,
                width: 2,
                height: 2,
            }),
            SelectionOperation::New,
        )
        .unwrap();
    let mut alpha = TileRaster::new(8, 8, PixelFormat::Grayscale16).unwrap();
    alpha
        .set_pixel(2, 3, PixelValue::Grayscale16(0x1234), 1)
        .unwrap();
    source.edit_plane_alpha(first, &alpha).unwrap();
    alpha
        .set_pixel(2, 3, PixelValue::Grayscale16(0xabcd), 2)
        .unwrap();
    source.edit_plane_alpha(second, &alpha).unwrap();
    let editor = source.editor_state().unwrap();
    source
        .update_editor_state(
            editor.revision,
            EditorStateUpdate::SetEditTargets(vec![
                EditTarget::Plane(EditorTarget {
                    layer_id,
                    plane_id: second,
                }),
                EditTarget::Plane(EditorTarget {
                    layer_id,
                    plane_id: first,
                }),
            ]),
        )
        .unwrap();
    let payload = source.copy_selection().unwrap();
    assert_eq!(
        payload.bounds,
        RectI32 {
            x: 2,
            y: 3,
            width: 2,
            height: 2
        }
    );
    assert_eq!(payload.planes.len(), 2);
    assert!(
        payload
            .planes
            .iter()
            .all(|plane| plane.pixel_format == PixelFormat::StraightRgba16)
    );
    assert_eq!(
        payload.planes[0].pixels[0].value,
        PixelValue::Rgba16([0, 0, 0, 0x1234])
    );
    assert_eq!(
        payload.planes[1].pixels[0].value,
        PixelValue::Rgba16([0, 0, 0, 0xabcd])
    );

    let mut destination = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_000a);
    let (_, destination_layer) = destination
        .create_layer(LayerKind::Raster, "16-bit destination")
        .unwrap();
    let (_, destination_first) = destination
        .create_plane(
            destination_layer,
            PlaneType::Raster,
            PixelFormat::StraightRgba16,
            "16 A",
        )
        .unwrap();
    let (_, destination_second) = destination
        .create_plane(
            destination_layer,
            PlaneType::Raster,
            PixelFormat::StraightRgba16,
            "16 B",
        )
        .unwrap();
    destination
        .set_active_node(destination_layer, destination_first)
        .unwrap();
    destination
        .apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 2,
                y: 3,
                width: 2,
                height: 2,
            }),
            SelectionOperation::New,
        )
        .unwrap();
    let editor = destination.editor_state().unwrap();
    destination
        .update_editor_state(
            editor.revision,
            EditorStateUpdate::SetEditTargets(vec![
                EditTarget::Plane(EditorTarget {
                    layer_id: destination_layer,
                    plane_id: destination_first,
                }),
                EditTarget::Plane(EditorTarget {
                    layer_id: destination_layer,
                    plane_id: destination_second,
                }),
            ]),
        )
        .unwrap();
    destination.begin_paste(&payload).unwrap();
    destination.commit_floating().unwrap();
    let pasted = destination.copy_selection().unwrap();
    assert_eq!(pasted.bounds, payload.bounds);
    assert_eq!(pasted.planes, payload.planes);
    destination.undo().unwrap();
    assert!(
        destination
            .copy_selection()
            .unwrap()
            .planes
            .iter()
            .all(|plane| plane.pixels.is_empty())
    );
    destination.redo().unwrap();
    assert_eq!(destination.copy_selection().unwrap().planes, payload.planes);
    destination.verify_journal_replay().unwrap();
}

#[test]
fn multi_target_property_and_duplicate_commands_are_atomic() {
    let mut core = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_0004);
    let layer_id = core.layers().unwrap()[0].id;
    let (_, first) = core
        .create_plane(layer_id, PlaneType::Raster, PixelFormat::StraightRgba8, "A")
        .unwrap();
    let (_, second) = core
        .create_plane(layer_id, PlaneType::Raster, PixelFormat::StraightRgba8, "B")
        .unwrap();
    let targets = vec![
        EditTarget::Plane(EditorTarget {
            layer_id,
            plane_id: second,
        }),
        EditTarget::Plane(EditorTarget {
            layer_id,
            plane_id: first,
        }),
    ];
    let editor = core.editor_state().unwrap();
    core.update_editor_state(editor.revision, EditorStateUpdate::SetEditTargets(targets))
        .unwrap();

    let history_before = core.history_entries().len();
    let visibility = core
        .apply_edit_target_command(EditTargetCommand::SetVisibility(false))
        .unwrap();
    assert_eq!(visibility.dispatch.accepted_commands(), 1);
    assert_eq!(core.history_entries().len(), history_before + 1);
    let layers = core.layers().unwrap();
    assert!(
        layers[0]
            .planes
            .iter()
            .filter(|plane| plane.id == first || plane.id == second)
            .all(|plane| !plane.visible)
    );
    core.undo().unwrap();
    assert!(
        core.layers().unwrap()[0]
            .planes
            .iter()
            .filter(|plane| plane.id == first || plane.id == second)
            .all(|plane| plane.visible)
    );

    let duplicate = core
        .apply_edit_target_command(EditTargetCommand::Duplicate)
        .unwrap();
    assert_eq!(duplicate.dispatch.accepted_commands(), 1);
    assert_eq!(duplicate.output_targets.len(), 2);
    assert_eq!(core.history_entries().len(), history_before + 1);
    assert_eq!(
        core.editor_state().unwrap().state.edit_targets,
        duplicate.output_targets
    );
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap()[0].planes.len(), 4);
    core.redo().unwrap();
    assert_eq!(core.layers().unwrap()[0].planes.len(), 6);
}

#[test]
fn grouped_capabilities_noop_merge_delete_and_incompatible_failure_are_atomic() {
    let mut core = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_0008);
    let layer_id = core.layers().unwrap()[0].id;
    let (_, first) = core
        .create_plane(
            layer_id,
            PlaneType::Raster,
            PixelFormat::StraightRgba8,
            "Merge A",
        )
        .unwrap();
    let (_, second) = core
        .create_plane(
            layer_id,
            PlaneType::Raster,
            PixelFormat::StraightRgba8,
            "Merge B",
        )
        .unwrap();
    let set_targets = |core: &mut Core, targets: Vec<EditTarget>| {
        let editor = core.editor_state().unwrap();
        core.update_editor_state(editor.revision, EditorStateUpdate::SetEditTargets(targets))
            .unwrap();
    };
    let pair = vec![
        EditTarget::Plane(EditorTarget {
            layer_id,
            plane_id: first,
        }),
        EditTarget::Plane(EditorTarget {
            layer_id,
            plane_id: second,
        }),
    ];
    set_targets(&mut core, pair.clone());
    let capabilities = core.edit_target_capabilities().unwrap();
    assert!(capabilities.duplicate);
    assert!(capabilities.delete);
    assert!(capabilities.visibility);
    assert!(capabilities.editability);
    assert!(capabilities.merge);
    assert!(capabilities.convert_planes);
    assert!(!capabilities.convert_layers);

    let before_noop = core.document_info().unwrap();
    let history_before_noop = core.history_entries().len();
    let noop = core
        .apply_edit_target_command(EditTargetCommand::SetVisibility(true))
        .unwrap();
    assert_eq!(noop.dispatch.accepted_commands(), 1);
    assert_eq!(core.document_info().unwrap(), before_noop);
    assert_eq!(core.history_entries().len(), history_before_noop);

    let before_merge_count = core.layers().unwrap()[0].planes.len();
    let merge = core
        .apply_edit_target_command(EditTargetCommand::Merge)
        .unwrap();
    assert_eq!(merge.dispatch.accepted_commands(), 1);
    assert_eq!(merge.output_targets.len(), 1);
    assert_eq!(
        core.layers().unwrap()[0].planes.len(),
        before_merge_count - 1
    );
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap()[0].planes.len(), before_merge_count);

    let main = plane_target(&core, 0, 0);
    let color = plane_target(&core, 0, 1);
    set_targets(&mut core, vec![main, color]);
    assert!(!core.edit_target_capabilities().unwrap().merge);
    let before_invalid = core.document_info().unwrap();
    let history_before_invalid = core.history_entries().len();
    assert!(matches!(
        core.apply_edit_target_command(EditTargetCommand::Merge),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), before_invalid);
    assert_eq!(core.history_entries().len(), history_before_invalid);

    set_targets(&mut core, pair);
    let revision_before_delete = core.document_info().unwrap().document_revision;
    let delete = core
        .apply_edit_target_command(EditTargetCommand::Delete)
        .unwrap();
    assert_eq!(delete.dispatch.accepted_commands(), 1);
    assert!(delete.output_targets.is_empty());
    assert_eq!(
        core.layers().unwrap()[0].planes.len(),
        before_merge_count - 2
    );
    assert_eq!(
        core.document_info().unwrap().document_revision,
        revision_before_delete + 1
    );
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap()[0].planes.len(), before_merge_count);
    core.verify_journal_replay().unwrap();
}

#[test]
fn brush_remains_single_active_plane_when_edit_targets_are_multiple() {
    let mut core = core_with_raster_layer(0x4d30_3300_0000_0000_0000_0000_0000_0005);
    let main = plane_target(&core, 0, 0);
    let color = plane_target(&core, 0, 1);
    let editor = core.editor_state().unwrap();
    core.update_editor_state(
        editor.revision,
        EditorStateUpdate::SetEditTargets(vec![main, color]),
    )
    .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.apply_stroke(&color_stroke(
        PaintTool::Brush,
        1.0,
        StrokeSample {
            x: 1.0,
            y: 1.0,
            pressure: 1.0,
        },
    ))
    .unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 1, 1).unwrap(),
        PixelValue::Rgba([12, 34, 56, 255])
    );
    assert!(
        core.plane_pixel(ActivePlane::MainLine, 1, 1)
            .unwrap()
            .is_zero()
    );
}
