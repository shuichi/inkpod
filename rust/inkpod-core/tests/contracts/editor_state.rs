use super::*;

fn editor_core() -> Core {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        32,
        24,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        0x0049_4e4b_504f_442d_4544_4954_4f52_0001,
    )
    .unwrap();
    core
}

fn target_for(core: &Core, kind: PlaneType) -> EditorTarget {
    let layer = &core.layers().unwrap()[0];
    let plane = layer
        .planes
        .iter()
        .find(|plane| plane.kind == kind)
        .unwrap();
    EditorTarget {
        layer_id: layer.id,
        plane_id: plane.id,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DocumentObservation {
    info: DocumentInfo,
    digest: DocumentStateDigest,
    journal_state: JournalState,
    history_cursor: usize,
    history_len: usize,
    journal: Vec<JournalEntry>,
    tile_revisions: Vec<u64>,
    tile_payloads: Vec<Vec<u8>>,
}

fn observe_document(core: &mut Core) -> DocumentObservation {
    let snapshot = core.build_snapshot();
    DocumentObservation {
        info: core.document_info().unwrap(),
        digest: core.document_state_digest().unwrap(),
        journal_state: core.journal_state().unwrap(),
        history_cursor: core.history_cursor(),
        history_len: core.history_entries().len(),
        journal: core.journal_entries().to_vec(),
        tile_revisions: snapshot
            .tiles()
            .iter()
            .map(RenderTile::tile_revision)
            .collect(),
        tile_payloads: snapshot
            .tiles()
            .iter()
            .map(|tile| tile.pixels().to_vec())
            .collect(),
    }
}

fn assert_editor_only_change(core: &mut Core, update: EditorStateUpdate) -> EditorStateInfo {
    let before_editor = core.editor_state().unwrap();
    let before_document = observe_document(core);
    let changed = core
        .update_editor_state(before_editor.revision, update)
        .unwrap();
    assert_eq!(changed.revision.get(), before_editor.revision.get() + 1);
    assert_ne!(changed.digest, before_editor.digest);
    assert!(changed.dirty);

    let mut after_document = observe_document(core);
    assert_eq!(
        after_document.info.dirty,
        before_document.info.dirty || changed.dirty
    );
    after_document.info.dirty = before_document.info.dirty;
    after_document.info.active_plane = before_document.info.active_plane;
    assert_eq!(after_document, before_document);
    changed
}

#[test]
fn editor_defaults_are_deterministic_and_copied_into_each_new_document() {
    let empty_a = Core::new();
    let empty_b = Core::new();
    let defaults = empty_a.editor_defaults();
    assert_eq!(defaults, empty_b.editor_defaults());
    assert_eq!(
        defaults.initial_document,
        InitialDocumentSpec {
            width: 1_920,
            height: 1_080,
            dpi_x_milli: DEFAULT_DPI_MILLI,
            dpi_y_milli: DEFAULT_DPI_MILLI,
        }
    );
    assert_eq!(defaults.state.active_tool, EditorTool::Pencil);
    assert_eq!(
        defaults.state.last_color_consuming_tool,
        Some(EditorTool::Pencil)
    );
    assert_eq!(
        defaults.state.tool_style(EditorTool::Pencil).unwrap().color,
        Some(PixelValue::Rgba([0, 0, 0, 255]))
    );
    for tool in [
        EditorTool::Brush,
        EditorTool::Fill,
        EditorTool::Selection,
        EditorTool::VectorLine,
    ] {
        assert_eq!(
            defaults.state.tool_style(tool).unwrap().color,
            Some(PixelValue::Rgba([220, 40, 30, 255]))
        );
    }
    assert_eq!(
        defaults
            .state
            .tool_style(EditorTool::Pencil)
            .unwrap()
            .diameter_q16,
        1 << 16
    );
    assert_eq!(
        defaults
            .state
            .tool_style(EditorTool::Brush)
            .unwrap()
            .diameter_q16,
        8 << 16
    );
    assert_eq!(defaults.state.fill.tolerance, 0);
    assert_eq!(defaults.state.fill.gap_close, 0);
    assert_eq!(defaults.state.fill.extension_distance, 1);
    assert!(defaults.state.fill.overflow_abort);
    assert_eq!(
        defaults.state.selection.shape,
        EditorSelectionShape::Rectangle
    );
    assert_eq!(defaults.state.selection.operation, SelectionOperation::New);
    assert_eq!(defaults.state.vector.erase_mode, VectorEraseMode::Partial);
    assert_eq!(
        defaults.state.vector.selection_mode,
        VectorSelectionMode::Touching
    );

    let mut core = Core::new();
    core.new_cell(
        defaults.initial_document.width,
        defaults.initial_document.height,
        defaults.initial_document.dpi_x_milli,
        defaults.initial_document.dpi_y_milli,
    )
    .unwrap();
    let initial = core.editor_state().unwrap();
    assert_eq!(initial.revision.get(), 1);
    assert!(!initial.dirty);
    assert_eq!(
        initial.state.without_target(),
        defaults.state.without_target()
    );
    assert!(initial.state.target.is_some());

    core.update_editor_state(
        initial.revision,
        EditorStateUpdate::SetToolColor {
            tool: EditorTool::Brush,
            color: PixelValue::Rgba16([1, 2, 3, 4]),
        },
    )
    .unwrap();
    core.new_cell(10, 10, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    assert_eq!(
        core.editor_state()
            .unwrap()
            .state
            .tool_style(EditorTool::Brush)
            .unwrap()
            .color,
        Some(PixelValue::Rgba([220, 40, 30, 255]))
    );
}

#[test]
fn tool_switch_restores_exact_depth_color_diameter_and_last_color_tool() {
    let mut core = editor_core();
    let initial = core.editor_state().unwrap();
    let mut state = initial.clone();
    for tool in [
        EditorTool::Fill,
        EditorTool::Selection,
        EditorTool::VectorLine,
    ] {
        state = core
            .update_editor_state(state.revision, EditorStateUpdate::SetActiveTool(tool))
            .unwrap();
        assert_eq!(
            state.state.current_color(),
            Some(PixelValue::Rgba([220, 40, 30, 255]))
        );
        assert_eq!(state.state.current_diameter_q16(), 8 << 16);
        assert_eq!(state.state.last_color_consuming_tool, Some(tool));
    }

    let per_tool = [
        (
            EditorTool::Fill,
            PixelValue::Rgba([10, 20, 30, 40]),
            5 * 65_536 + 1,
        ),
        (
            EditorTool::Selection,
            PixelValue::Rgba16([0x1111, 0x2222, 0x3333, 0x4444]),
            6 * 65_536 + 2,
        ),
        (
            EditorTool::VectorLine,
            PixelValue::Rgba16([0xaaaa, 0xbbbb, 0xcccc, 0xdddd]),
            7 * 65_536 + 3,
        ),
    ];
    for (tool, color, diameter_q16) in per_tool {
        state = core
            .update_editor_state(
                state.revision,
                EditorStateUpdate::SetToolColor { tool, color },
            )
            .unwrap();
        state = core
            .update_editor_state(
                state.revision,
                EditorStateUpdate::SetToolDiameter { tool, diameter_q16 },
            )
            .unwrap();
    }
    for (tool, color, diameter_q16) in per_tool {
        state = core
            .update_editor_state(state.revision, EditorStateUpdate::SetActiveTool(tool))
            .unwrap();
        assert_eq!(state.state.current_color(), Some(color));
        assert_eq!(state.state.current_diameter_q16(), diameter_q16);
        assert_eq!(state.state.last_color_consuming_tool, Some(tool));
        state = core
            .update_editor_state(
                state.revision,
                EditorStateUpdate::SetActiveTool(EditorTool::Eyedropper),
            )
            .unwrap();
        assert_eq!(state.state.current_color(), Some(color));
        assert_eq!(state.state.last_color_consuming_tool, Some(tool));
    }

    let rgba16 = PixelValue::Rgba16([0x0123, 0x4567, 0x89ab, 0xcdef]);
    let brush = core
        .update_editor_state(
            state.revision,
            EditorStateUpdate::SetToolColor {
                tool: EditorTool::Brush,
                color: rgba16,
            },
        )
        .unwrap();
    let brush = core
        .update_editor_state(
            brush.revision,
            EditorStateUpdate::SetToolDiameter {
                tool: EditorTool::Brush,
                diameter_q16: 13 * 65_536 + 7,
            },
        )
        .unwrap();
    let brush = core
        .update_editor_state(
            brush.revision,
            EditorStateUpdate::SetActiveTool(EditorTool::Brush),
        )
        .unwrap();
    assert_eq!(brush.state.current_color(), Some(rgba16));
    assert_eq!(brush.state.current_diameter_q16(), 13 * 65_536 + 7);
    assert_eq!(
        brush.state.last_color_consuming_tool,
        Some(EditorTool::Brush)
    );

    let eyedropper = core
        .update_editor_state(
            brush.revision,
            EditorStateUpdate::SetActiveTool(EditorTool::Eyedropper),
        )
        .unwrap();
    assert_eq!(
        eyedropper.state.last_color_consuming_tool,
        Some(EditorTool::Brush)
    );
    assert_eq!(eyedropper.state.current_color(), Some(rgba16));

    let pencil = core
        .update_editor_state(
            eyedropper.revision,
            EditorStateUpdate::SetActiveTool(EditorTool::Pencil),
        )
        .unwrap();
    assert_eq!(
        pencil.state.current_color(),
        Some(PixelValue::Rgba([0, 0, 0, 255]))
    );
    let restored = core
        .update_editor_state(
            pencil.revision,
            EditorStateUpdate::SetActiveTool(EditorTool::Brush),
        )
        .unwrap();
    assert_eq!(restored.state.current_color(), Some(rgba16));
    assert_eq!(restored.state.current_diameter_q16(), 13 * 65_536 + 7);
}

#[test]
fn color_diameter_and_active_target_updates_each_leave_document_state_untouched() {
    let mut core = editor_core();
    let color = assert_editor_only_change(
        &mut core,
        EditorStateUpdate::SetToolColor {
            tool: EditorTool::Brush,
            color: PixelValue::Rgba16([1, 2, 3, 4]),
        },
    );
    assert_eq!(
        color.state.tool_style(EditorTool::Brush).unwrap().color,
        Some(PixelValue::Rgba16([1, 2, 3, 4]))
    );

    let diameter = assert_editor_only_change(
        &mut core,
        EditorStateUpdate::SetToolDiameter {
            tool: EditorTool::Brush,
            diameter_q16: 19 * 65_536 + 17,
        },
    );
    assert_eq!(
        diameter
            .state
            .tool_style(EditorTool::Brush)
            .unwrap()
            .diameter_q16,
        19 * 65_536 + 17
    );

    let target = target_for(&core, PlaneType::Color);
    assert_ne!(core.editor_state().unwrap().state.target, Some(target));
    let active_target =
        assert_editor_only_change(&mut core, EditorStateUpdate::SetActiveTarget(target));
    assert_eq!(active_target.state.target, Some(target));
}

#[test]
fn editor_updates_are_atomic_and_do_not_touch_document_history_or_render_content() {
    let mut core = editor_core();
    let before_editor = core.editor_state().unwrap();
    let before_document = observe_document(&mut core);

    let no_op = core
        .update_editor_state(
            before_editor.revision,
            EditorStateUpdate::SetActiveTool(before_editor.state.active_tool),
        )
        .unwrap();
    assert_eq!(no_op, before_editor);
    assert_eq!(observe_document(&mut core), before_document);

    let changed = core
        .update_editor_state(
            before_editor.revision,
            EditorStateUpdate::SetPaletteCursor(Some(PaletteCursor { group: 2, index: 7 })),
        )
        .unwrap();
    assert_eq!(changed.revision.get(), before_editor.revision.get() + 1);
    assert_ne!(changed.digest, before_editor.digest);
    assert!(changed.dirty);
    let changed_document = observe_document(&mut core);
    assert!(changed_document.info.dirty);
    let mut document_without_session_dirty = changed_document.clone();
    document_without_session_dirty.info.dirty = before_document.info.dirty;
    assert_eq!(document_without_session_dirty, before_document);

    let before_failure = core.editor_state().unwrap();
    let before_failure_document = changed_document;
    assert!(matches!(
        core.update_editor_state(
            before_editor.revision,
            EditorStateUpdate::SetActiveTool(EditorTool::Fill),
        ),
        Err(CoreError::InvalidState(
            "editor state base revision is stale"
        ))
    ));
    assert_eq!(core.editor_state().unwrap(), before_failure);
    assert!(matches!(
        core.update_editor_state(
            before_failure.revision,
            EditorStateUpdate::SetToolDiameter {
                tool: EditorTool::Brush,
                diameter_q16: 0,
            },
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.editor_state().unwrap(), before_failure);
    assert_eq!(observe_document(&mut core), before_failure_document);
}

#[test]
fn editor_state_is_session_owned_shared_by_views_and_independent_between_documents() {
    let mut first = editor_core();
    let mut second = editor_core();
    let view_id = first.create_view().unwrap();
    let start = first.editor_state().unwrap();
    let changed = first
        .update_editor_state(
            start.revision,
            EditorStateUpdate::SetSelectionOptions(EditorSelectionOptions {
                shape: EditorSelectionShape::Wand,
                operation: SelectionOperation::Add,
                tolerance: 321,
                gap_close: 9,
                diameter_q16: 17 << 16,
                ..EditorSelectionOptions::default()
            }),
        )
        .unwrap();
    first
        .apply_view_for(
            view_id,
            ViewCommand::PanBy {
                device_dx: 4.0,
                device_dy: -2.0,
            },
        )
        .unwrap();
    assert_eq!(first.editor_state().unwrap(), changed);
    assert_ne!(
        second.editor_state().unwrap().state.selection,
        changed.state.selection
    );

    second
        .update_editor_state(
            second.editor_state().unwrap().revision,
            EditorStateUpdate::SetActiveTool(EditorTool::VectorCurve),
        )
        .unwrap();
    assert_eq!(first.editor_state().unwrap(), changed);
}

#[test]
fn active_target_uses_stable_ids_and_reconciles_topology_deterministically() {
    fn exercise() -> Vec<EditorTarget> {
        let mut core = editor_core();
        let (_, layer_id) = core.create_layer(LayerKind::Raster, "Target").unwrap();
        let raster_plane_id = core
            .layers()
            .unwrap()
            .iter()
            .find(|layer| layer.id == layer_id)
            .unwrap()
            .planes[0]
            .id;
        let selected = core.editor_state().unwrap().state.target.unwrap();
        assert_eq!(
            selected,
            EditorTarget {
                layer_id,
                plane_id: raster_plane_id
            }
        );
        core.set_active_node(layer_id, raster_plane_id).unwrap();
        let explicit = core.editor_state().unwrap().state.target.unwrap();
        assert_eq!(explicit, selected);
        assert!(matches!(
            core.update_editor_state(
                core.editor_state().unwrap().revision,
                EditorStateUpdate::SetActiveTarget(EditorTarget {
                    layer_id,
                    plane_id: u64::MAX,
                }),
            ),
            Err(CoreError::InvalidArgument(_))
        ));
        core.delete_layer(layer_id).unwrap();
        let resolved = core.editor_state().unwrap().state.target.unwrap();
        let layers = core.layers().unwrap();
        assert!(layers.iter().any(|layer| {
            layer.id == resolved.layer_id
                && layer
                    .planes
                    .iter()
                    .any(|plane| plane.id == resolved.plane_id)
        }));
        vec![selected, explicit, resolved]
    }

    assert_eq!(exercise(), exercise());

    let mut overflow = editor_core();
    let (_, doomed_layer_id) = overflow
        .create_layer(LayerKind::Raster, "Overflow target")
        .unwrap();
    let mut maximum_revision = overflow.editor_state_frame().unwrap();
    maximum_revision[44..52].copy_from_slice(&u64::MAX.to_le_bytes());
    overflow
        .restore_editor_state_frame(&maximum_revision, EditorFrameDisposition::Saved)
        .unwrap();
    let before_document = observe_document(&mut overflow);
    let before_editor = overflow.editor_state().unwrap();
    assert!(matches!(
        overflow.delete_layer(doomed_layer_id),
        Err(CoreError::InvalidState("editor revision overflow"))
    ));
    assert_eq!(observe_document(&mut overflow), before_document);
    assert_eq!(overflow.editor_state().unwrap(), before_editor);
}

#[test]
fn target_changing_topology_editor_overflow_is_fully_atomic() {
    fn set_maximum_editor_revision(core: &mut Core) {
        let mut frame = core.editor_state_frame().unwrap();
        frame[44..52].copy_from_slice(&u64::MAX.to_le_bytes());
        core.restore_editor_state_frame(&frame, EditorFrameDisposition::Saved)
            .unwrap();
    }

    let mut layer_candidate = editor_core();
    let mut layer_control = editor_core();
    set_maximum_editor_revision(&mut layer_candidate);
    set_maximum_editor_revision(&mut layer_control);
    let layer_document_before = observe_document(&mut layer_candidate);
    let layer_editor_before = layer_candidate.editor_state().unwrap();
    let layer_topology_before = layer_candidate.layers().unwrap();
    let layer_resources_before = layer_candidate.resource_usage();
    assert!(matches!(
        layer_candidate.create_layer(LayerKind::Raster, "Rejected layer"),
        Err(CoreError::InvalidState("editor revision overflow"))
    ));
    assert_eq!(
        observe_document(&mut layer_candidate),
        layer_document_before
    );
    assert_eq!(layer_candidate.editor_state().unwrap(), layer_editor_before);
    assert_eq!(layer_candidate.layers().unwrap(), layer_topology_before);
    assert_eq!(layer_candidate.resource_usage(), layer_resources_before);
    let (_, candidate_guide_id) = layer_candidate.add_guide(GuideAxis::Vertical, 1).unwrap();
    let (_, control_guide_id) = layer_control.add_guide(GuideAxis::Vertical, 1).unwrap();
    assert_eq!(candidate_guide_id, control_guide_id);

    let mut plane_candidate = editor_core();
    let mut plane_control = editor_core();
    let (_, candidate_layer_id) = plane_candidate
        .create_layer(LayerKind::Raster, "Plane target")
        .unwrap();
    let (_, control_layer_id) = plane_control
        .create_layer(LayerKind::Raster, "Plane target")
        .unwrap();
    assert_eq!(candidate_layer_id, control_layer_id);
    set_maximum_editor_revision(&mut plane_candidate);
    set_maximum_editor_revision(&mut plane_control);
    let plane_document_before = observe_document(&mut plane_candidate);
    let plane_editor_before = plane_candidate.editor_state().unwrap();
    let plane_topology_before = plane_candidate.layers().unwrap();
    let plane_resources_before = plane_candidate.resource_usage();
    assert!(matches!(
        plane_candidate.create_plane(
            candidate_layer_id,
            PlaneType::Raster,
            PixelFormat::StraightRgba8,
            "Rejected plane",
        ),
        Err(CoreError::InvalidState("editor revision overflow"))
    ));
    assert_eq!(
        observe_document(&mut plane_candidate),
        plane_document_before
    );
    assert_eq!(plane_candidate.editor_state().unwrap(), plane_editor_before);
    assert_eq!(plane_candidate.layers().unwrap(), plane_topology_before);
    assert_eq!(plane_candidate.resource_usage(), plane_resources_before);
    let (_, candidate_guide_id) = plane_candidate.add_guide(GuideAxis::Horizontal, 1).unwrap();
    let (_, control_guide_id) = plane_control.add_guide(GuideAxis::Horizontal, 1).unwrap();
    assert_eq!(candidate_guide_id, control_guide_id);

    let mut selection_candidate = editor_core();
    let mut selection_control = editor_core();
    for core in [&mut selection_candidate, &mut selection_control] {
        core.apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 1,
                y: 1,
                width: 4,
                height: 3,
            }),
            SelectionOperation::New,
        )
        .unwrap();
        set_maximum_editor_revision(core);
    }
    let selection_document_before = observe_document(&mut selection_candidate);
    let selection_editor_before = selection_candidate.editor_state().unwrap();
    let selection_topology_before = selection_candidate.layers().unwrap();
    let selection_resources_before = selection_candidate.resource_usage();
    assert!(matches!(
        selection_candidate.selection_to_layer("Rejected selection"),
        Err(CoreError::InvalidState("editor revision overflow"))
    ));
    assert_eq!(
        observe_document(&mut selection_candidate),
        selection_document_before
    );
    assert_eq!(
        selection_candidate.editor_state().unwrap(),
        selection_editor_before
    );
    assert_eq!(
        selection_candidate.layers().unwrap(),
        selection_topology_before
    );
    assert_eq!(
        selection_candidate.resource_usage(),
        selection_resources_before
    );
    let (_, candidate_guide_id) = selection_candidate
        .add_guide(GuideAxis::Vertical, 2)
        .unwrap();
    let (_, control_guide_id) = selection_control.add_guide(GuideAxis::Vertical, 2).unwrap();
    assert_eq!(candidate_guide_id, control_guide_id);

    let mut vector_candidate = editor_core();
    let mut vector_control = editor_core();
    let (_, candidate_vector_layer_id) = vector_candidate
        .create_layer(LayerKind::VectorColoring, "Vector source")
        .unwrap();
    let (_, control_vector_layer_id) = vector_control
        .create_layer(LayerKind::VectorColoring, "Vector source")
        .unwrap();
    assert_eq!(candidate_vector_layer_id, control_vector_layer_id);
    set_maximum_editor_revision(&mut vector_candidate);
    set_maximum_editor_revision(&mut vector_control);
    let vector_document_before = observe_document(&mut vector_candidate);
    let vector_editor_before = vector_candidate.editor_state().unwrap();
    let vector_topology_before = vector_candidate.layers().unwrap();
    let vector_resources_before = vector_candidate.resource_usage();
    assert!(matches!(
        vector_candidate.rasterize_vector_layer_to_document(
            candidate_vector_layer_id,
            true,
            "Rejected raster",
        ),
        Err(CoreError::InvalidState("editor revision overflow"))
    ));
    assert_eq!(
        observe_document(&mut vector_candidate),
        vector_document_before
    );
    assert_eq!(
        vector_candidate.editor_state().unwrap(),
        vector_editor_before
    );
    assert_eq!(vector_candidate.layers().unwrap(), vector_topology_before);
    assert_eq!(vector_candidate.resource_usage(), vector_resources_before);
    let (_, candidate_guide_id) = vector_candidate
        .add_guide(GuideAxis::Horizontal, 2)
        .unwrap();
    let (_, control_guide_id) = vector_control.add_guide(GuideAxis::Horizontal, 2).unwrap();
    assert_eq!(candidate_guide_id, control_guide_id);
}

#[test]
fn canonical_editor_frame_round_trips_and_failure_or_overflow_is_atomic() {
    let mut source = editor_core();
    let current = source.editor_state().unwrap();
    let fill_state = source
        .update_editor_state(
            current.revision,
            EditorStateUpdate::SetFillOptions(EditorFillOptions {
                operation: FillOperation::ClosedRegion,
                tolerance: 0x1234,
                gap_close: 17,
                extension_distance: 9,
                inclusion_mode: InclusionMode::Specified,
                inclusion_colors: vec![PixelValue::Rgba16([1, 2, 3, 4])],
                overflow_abort: false,
                detached_regions: true,
                transparent_only: true,
                use_document_selection: true,
                light_table_boundary: true,
                light_table_color: true,
            }),
        )
        .unwrap();
    let mut maximum_selection = fill_state.state.selection.clone();
    maximum_selection.diameter_q16 = 4_096_i64 << 16;
    maximum_selection.interpretation = RangeInterpretation::Boundary;
    maximum_selection.aspect_ratio_q16 = 16 << 16;
    maximum_selection.from_center = true;
    maximum_selection.constrain_rotation_45 = true;
    maximum_selection.rotation_turns = 0x2000_0000;
    maximum_selection.trace_shape = TraceBrushShape::Square;
    maximum_selection.trace_pressure_size = true;
    maximum_selection.trace_screen_size = true;
    let maximum_selection_state = source
        .update_editor_state(
            fill_state.revision,
            EditorStateUpdate::SetSelectionOptions(maximum_selection.clone()),
        )
        .unwrap();
    maximum_selection.diameter_q16 = (4_096_i64 << 16) + 1;
    assert!(matches!(
        source.update_editor_state(
            maximum_selection_state.revision,
            EditorStateUpdate::SetSelectionOptions(maximum_selection),
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(source.editor_state().unwrap(), maximum_selection_state);
    let frame = source.editor_state_frame().unwrap();

    let mut destination = editor_core();
    let restored = destination
        .restore_editor_state_frame(&frame, EditorFrameDisposition::Unsaved)
        .unwrap();
    assert_eq!(restored.state, source.editor_state().unwrap().state);
    assert_eq!(restored.digest, source.editor_state().unwrap().digest);
    assert!(restored.dirty);
    assert_eq!(destination.editor_state_frame().unwrap(), frame);

    let before_failure = destination.editor_state().unwrap();
    let mut corrupt = frame.clone();
    *corrupt.last_mut().unwrap() ^= 0x80;
    assert!(
        destination
            .restore_editor_state_frame(&corrupt, EditorFrameDisposition::Saved)
            .is_err()
    );
    assert_eq!(destination.editor_state().unwrap(), before_failure);

    let mut maximum_revision = frame;
    maximum_revision[44..52].copy_from_slice(&u64::MAX.to_le_bytes());
    destination
        .restore_editor_state_frame(&maximum_revision, EditorFrameDisposition::Saved)
        .unwrap();
    let maximum = destination.editor_state().unwrap();
    assert_eq!(maximum.revision.get(), u64::MAX);
    assert!(matches!(
        destination.update_editor_state(
            maximum.revision,
            EditorStateUpdate::SetActiveTool(EditorTool::Brush),
        ),
        Err(CoreError::InvalidState("editor revision overflow"))
    ));
    assert_eq!(destination.editor_state().unwrap(), maximum);
}

#[test]
fn reopen_resolves_target_past_a_leading_layer_without_planes() {
    let sequence = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "inkpod-editor-empty-leading-layer-{sequence}.inkpod"
    ));
    let mut source = editor_core();
    source
        .create_adjustment_layer(
            "Leading adjustment",
            Adjustment::BrightnessContrast {
                brightness_milli: 0,
                contrast_milli: 0,
            },
        )
        .unwrap();
    assert!(source.layers().unwrap()[0].planes.is_empty());
    source.save(&path).unwrap();

    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    let target = reopened
        .editor_state()
        .unwrap()
        .state
        .target
        .expect("the first nonempty layer must supply the deterministic target");
    assert!(reopened.layers().unwrap().iter().any(|layer| {
        layer.id == target.layer_id && layer.planes.iter().any(|plane| plane.id == target.plane_id)
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn editor_savepoint_and_edit_frame_round_trip_with_current_native_format() {
    let sequence = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir();
    let normal = root.join(format!("inkpod-editor-contract-{sequence}.inkpod"));
    let recovery = root.join(format!("inkpod-editor-contract-{sequence}.recovery.inkpod"));
    let impossible = root
        .join(format!("inkpod-editor-contract-missing-{sequence}"))
        .join("document.inkpod");
    let mut core = editor_core();
    assert!(!core.document_info().unwrap().dirty);
    let changed = core
        .update_editor_state(
            core.editor_state().unwrap().revision,
            EditorStateUpdate::SetActiveTool(EditorTool::Brush),
        )
        .unwrap();
    let changed = core
        .update_editor_state(
            changed.revision,
            EditorStateUpdate::SetBrushOptions(EditorBrushOptions {
                shape: BrushShape::Square,
                smoothing: 725,
                start_color: StartColorPredicate::ExactNative,
            }),
        )
        .unwrap();
    assert!(changed.dirty);
    assert!(core.document_info().unwrap().dirty);

    assert!(core.save(&impossible).is_err());
    assert!(core.editor_state().unwrap().dirty);
    core.autosave(&recovery).unwrap();
    assert!(core.editor_state().unwrap().dirty);
    let encoded_export = core
        .export_common_raster(CommonRasterFormat::Png, false)
        .unwrap();
    assert!(!encoded_export.is_empty());
    assert!(core.editor_state().unwrap().dirty);
    core.save(&normal).unwrap();
    assert!(!core.editor_state().unwrap().dirty);
    assert!(!core.document_info().unwrap().dirty);

    let frame = core.editor_state_frame().unwrap();
    let token = core.editor_savepoint_token().unwrap();
    let clean = core.commit_editor_savepoint(token).unwrap();
    assert!(!clean.dirty);
    assert!(!core.document_info().unwrap().dirty);

    let mut reopened = Core::new();
    reopened.open(&normal).unwrap();
    assert_eq!(reopened.editor_state_frame().unwrap(), frame);
    assert_eq!(
        reopened.editor_state().unwrap().state.active_tool,
        EditorTool::Brush
    );
    assert_eq!(
        reopened.editor_state().unwrap().state.brush,
        EditorBrushOptions {
            shape: BrushShape::Square,
            smoothing: 725,
            start_color: StartColorPredicate::ExactNative,
        }
    );
    assert!(!reopened.document_info().unwrap().dirty);

    let mut recovered = Core::new();
    recovered.open_recovery(&recovery).unwrap();
    assert!(recovered.document_info().unwrap().dirty);
    assert!(recovered.editor_state().unwrap().dirty);

    let _ = fs::remove_file(normal);
    let _ = fs::remove_file(recovery);
}

#[test]
fn captured_fill_selection_and_color_targets_do_not_follow_later_editor_state() {
    let mut core = editor_core();
    let (_, first_layer_id) = core.create_layer(LayerKind::Raster, "Captured A").unwrap();
    let first_plane_id = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == first_layer_id)
        .unwrap()
        .planes[0]
        .id;
    let captured = EditorTarget {
        layer_id: first_layer_id,
        plane_id: first_plane_id,
    };
    let (_, second_layer_id) = core.create_layer(LayerKind::Raster, "Live B").unwrap();
    let second_plane_id = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == second_layer_id)
        .unwrap()
        .planes[0]
        .id;
    let live = EditorTarget {
        layer_id: second_layer_id,
        plane_id: second_plane_id,
    };
    assert_eq!(core.editor_state().unwrap().state.target, Some(live));

    let filled = core
        .apply_fill_for_editor_target(
            &fill_request(1, 1, [90, 80, 70, 255]),
            captured,
            false,
            false,
        )
        .unwrap();
    assert_eq!(filled.changed_pixels, 32 * 24);
    assert_eq!(
        core.editor_state().unwrap().state.target,
        Some(captured),
        "a changing fill must select its actual captured coloring target"
    );

    core.update_editor_state(
        core.editor_state().unwrap().revision,
        EditorStateUpdate::SetActiveTarget(live),
    )
    .unwrap();
    core.apply_selection_for_editor_target(
        &SelectionShape::Rectangle(RectI32 {
            x: 1,
            y: 1,
            width: 3,
            height: 2,
        }),
        SelectionOperation::New,
        captured,
    )
    .unwrap();
    assert_eq!(
        core.editor_state().unwrap().state.target,
        Some(live),
        "selection uses the captured source without replacing the live presentation target"
    );

    core.select_color_for_editor_target(
        PixelValue::Rgba([90, 80, 70, 255]),
        0,
        false,
        SelectionOperation::New,
        captured,
    )
    .unwrap();
    assert_eq!(
        core.selection_bounds().unwrap(),
        Some(RectI32 {
            x: 0,
            y: 0,
            width: 32,
            height: 24,
        })
    );
    assert_eq!(
        core.editor_state().unwrap().state.target,
        Some(live),
        "color selection uses the captured source without replacing the live presentation target"
    );

    core.delete_layer(first_layer_id).unwrap();
    let before_failure = observe_document(&mut core);
    let editor_before_failure = core.editor_state().unwrap();
    assert!(matches!(
        core.apply_selection_for_editor_target(
            &SelectionShape::Rectangle(RectI32 {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
            SelectionOperation::New,
            captured,
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(observe_document(&mut core), before_failure);
    assert_eq!(core.editor_state().unwrap(), editor_before_failure);
    assert!(matches!(
        core.select_color_for_editor_target(
            PixelValue::Rgba([90, 80, 70, 255]),
            0,
            false,
            SelectionOperation::New,
            captured,
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(observe_document(&mut core), before_failure);
    assert_eq!(core.editor_state().unwrap(), editor_before_failure);
}

#[test]
fn editor_stroke_captures_exact_values_and_stable_target_at_begin() {
    let mut core = editor_core();
    let color_target = target_for(&core, PlaneType::Color);
    let start = core.editor_state().unwrap();
    let start = core
        .update_editor_state(
            start.revision,
            EditorStateUpdate::SetActiveTarget(color_target),
        )
        .unwrap();
    let captured_color = PixelValue::Rgba16([0x0102, 0x3456, 0x789a, 0xbcde]);
    let start = core
        .update_editor_state(
            start.revision,
            EditorStateUpdate::SetToolColor {
                tool: EditorTool::Brush,
                color: captured_color,
            },
        )
        .unwrap();
    let start = core
        .update_editor_state(
            start.revision,
            EditorStateUpdate::SetToolDiameter {
                tool: EditorTool::Brush,
                diameter_q16: 9 * 65_536 + 123,
            },
        )
        .unwrap();
    core.update_editor_state(
        start.revision,
        EditorStateUpdate::SetActiveTool(EditorTool::Brush),
    )
    .unwrap();
    core.begin_editor_stroke(&EditorStrokeInput {
        tool: None,
        coordinate_space: CoordinateSpace::Document,
        auto_erase: false,
        pressure_size: true,
        samples: vec![StrokeSample {
            x: 4.0,
            y: 5.0,
            pressure: 1.0,
        }],
    })
    .unwrap();

    let live = core.editor_state().unwrap();
    core.update_editor_state(
        live.revision,
        EditorStateUpdate::SetToolColor {
            tool: EditorTool::Brush,
            color: PixelValue::Rgba([9, 8, 7, 6]),
        },
    )
    .unwrap();
    let live = core.editor_state().unwrap();
    core.update_editor_state(
        live.revision,
        EditorStateUpdate::SetToolDiameter {
            tool: EditorTool::Brush,
            diameter_q16: 3 << 16,
        },
    )
    .unwrap();
    let live = core.editor_state().unwrap();
    core.update_editor_state(
        live.revision,
        EditorStateUpdate::SetActiveTarget(target_for(&core, PlaneType::MainLine)),
    )
    .unwrap();
    core.append_stroke(&[StrokeSample {
        x: 6.0,
        y: 7.0,
        pressure: 0.75,
    }])
    .unwrap();
    core.end_stroke().unwrap();

    let JournalEntry::Commit(commit) = core.journal_entries().last().unwrap() else {
        panic!("stroke must commit one canonical procedure");
    };
    let procedure = commit.procedure();
    assert_eq!(procedure.input_ids(), &[color_target.plane_id]);
    let arguments = procedure.canonical_arguments();
    assert_eq!(&arguments[0..8], &color_target.plane_id.to_le_bytes());
    assert_eq!(u32::from_le_bytes(arguments[8..12].try_into().unwrap()), 2);
    assert_eq!(arguments[12], 2, "RGBA16 canonical color tag");
    assert_eq!(
        &arguments[13..21],
        &[2, 1, 0x56, 0x34, 0x9a, 0x78, 0xde, 0xbc]
    );
    assert_eq!(
        i64::from_le_bytes(arguments[21..29].try_into().unwrap()),
        9 * 65_536 + 123
    );
}

#[test]
fn editor_stroke_uses_the_captured_secondary_view_for_device_samples() {
    let mut core = editor_core();
    let main_line_target = target_for(&core, PlaneType::MainLine);
    let state = core.editor_state().unwrap();
    let state = core
        .update_editor_state(
            state.revision,
            EditorStateUpdate::SetActiveTarget(main_line_target),
        )
        .unwrap();
    core.update_editor_state(
        state.revision,
        EditorStateUpdate::SetActiveTool(EditorTool::Pencil),
    )
    .unwrap();

    let secondary = core.create_view().unwrap();
    let secondary_view = core
        .apply_view_for(
            secondary,
            ViewCommand::PanBy {
                device_dx: 10.0,
                device_dy: 5.0,
            },
        )
        .unwrap();
    let device_x =
        |document_x: f64| document_x.mul_add(secondary_view.zoom(), secondary_view.pan_x()) as f32;
    let device_y = 2.0_f64.mul_add(secondary_view.zoom(), secondary_view.pan_y()) as f32;
    let located = core
        .locator_sample(
            Some(secondary),
            f64::from(device_x(2.0)),
            f64::from(device_y),
        )
        .unwrap();
    assert_eq!((located.document_x, located.document_y), (2, 2));
    core.begin_editor_stroke_for_view(
        secondary,
        &EditorStrokeInput {
            tool: None,
            coordinate_space: CoordinateSpace::Device,
            auto_erase: false,
            pressure_size: false,
            samples: vec![StrokeSample {
                x: device_x(2.0),
                y: device_y,
                pressure: 1.0,
            }],
        },
    )
    .unwrap();

    // Append must retain the secondary transform selected at begin rather than
    // falling back to the primary view.
    core.append_stroke(&[StrokeSample {
        x: device_x(3.0),
        y: device_y,
        pressure: 1.0,
    }])
    .unwrap();
    core.end_stroke().unwrap();

    let painted = (0..24)
        .flat_map(|y| (0..32).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            core.plane_pixel(ActivePlane::MainLine, x, y).unwrap() == PixelValue::Binary(255)
        })
        .collect::<Vec<_>>();
    assert_eq!(painted, vec![(2, 2), (3, 2)]);
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 12, 7).unwrap(),
        PixelValue::Binary(0),
        "device samples must not use the primary view transform"
    );
}

#[test]
fn editor_stroke_tool_selector_uses_core_owned_style_without_switching_active_tool() {
    let mut core = editor_core();
    let color_target = target_for(&core, PlaneType::Color);
    let pencil_color = PixelValue::Rgba16([0x1234, 0x5678, 0x9abc, 0xdef0]);
    let mut state = core.editor_state().unwrap();
    state = core
        .update_editor_state(
            state.revision,
            EditorStateUpdate::SetActiveTarget(color_target),
        )
        .unwrap();
    state = core
        .update_editor_state(
            state.revision,
            EditorStateUpdate::SetToolColor {
                tool: EditorTool::Pencil,
                color: pencil_color,
            },
        )
        .unwrap();
    state = core
        .update_editor_state(
            state.revision,
            EditorStateUpdate::SetToolDiameter {
                tool: EditorTool::Pencil,
                diameter_q16: 65_536,
            },
        )
        .unwrap();
    let before = core
        .update_editor_state(
            state.revision,
            EditorStateUpdate::SetActiveTool(EditorTool::Brush),
        )
        .unwrap();

    core.begin_editor_stroke(&EditorStrokeInput {
        tool: Some(EditorTool::Pencil),
        coordinate_space: CoordinateSpace::Document,
        auto_erase: false,
        pressure_size: false,
        samples: vec![StrokeSample {
            x: 2.0,
            y: 2.0,
            pressure: 1.0,
        }],
    })
    .unwrap();
    core.end_stroke().unwrap();

    assert_eq!(core.editor_state().unwrap(), before);
    let JournalEntry::Commit(commit) = core.journal_entries().last().unwrap() else {
        panic!("stroke must commit one canonical procedure");
    };
    let arguments = commit.procedure().canonical_arguments();
    assert_eq!(u32::from_le_bytes(arguments[8..12].try_into().unwrap()), 1);
    assert_eq!(arguments[12], 2, "the selected pencil retains RGBA16");
    assert_eq!(
        &arguments[13..21],
        &[0x34, 0x12, 0x78, 0x56, 0xbc, 0x9a, 0xf0, 0xde]
    );
    assert_eq!(
        i64::from_le_bytes(arguments[21..29].try_into().unwrap()),
        65_536
    );
}
