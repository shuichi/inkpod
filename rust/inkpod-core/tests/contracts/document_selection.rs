use super::*;

fn last_commit_primitive(core: &Core) -> PrimitiveId {
    core.journal_entries()
        .iter()
        .rev()
        .find_map(|entry| match entry {
            JournalEntry::Commit(commit) => Some(commit.procedure().primitive_id()),
            JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => None,
        })
        .expect("a committed document primitive must exist")
}

#[test]
fn bulk_layer_and_guide_deletes_are_single_replayable_primitives() {
    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let (_, hidden_one) = core.create_layer(LayerKind::Raster, "Hidden One").unwrap();
    let (_, hidden_two) = core.create_layer(LayerKind::Raster, "Hidden Two").unwrap();
    core.set_layer_properties(hidden_one, false, true, 1_000, "Hidden One")
        .unwrap();
    core.set_layer_properties(hidden_two, false, true, 1_000, "Hidden Two")
        .unwrap();
    let before_layers = core.layers().unwrap().len();
    let before_revision = core.document_info().unwrap().document_revision;
    let before_history = core.history_entries().len();

    let deleted = core.delete_hidden_layers().unwrap();
    assert_eq!(deleted.revision(), before_revision + 1);
    assert_eq!(core.layers().unwrap().len(), before_layers - 2);
    assert_eq!(core.history_entries().len(), before_history + 1);
    assert_eq!(
        last_commit_primitive(&core),
        PrimitiveId::DELETE_HIDDEN_LAYERS
    );
    let procedures = core
        .journal_entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Commit(commit) => Some(commit.procedure().clone()),
            JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => None,
        })
        .collect::<Vec<_>>();
    let mut direct_replay = Core::new();
    direct_replay
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    for procedure in &procedures {
        direct_replay
            .replay_procedure(procedure)
            .unwrap_or_else(|error| {
                panic!(
                    "primitive {:08x} failed direct replay: {error:?}",
                    procedure.primitive_id().get()
                )
            });
    }
    core.verify_journal_replay().unwrap();
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap().len(), before_layers);
    core.redo().unwrap();
    assert_eq!(core.layers().unwrap().len(), before_layers - 2);

    let (_, first_guide) = core.add_guide(GuideAxis::Horizontal, 2).unwrap();
    let (_, second_guide) = core.add_guide(GuideAxis::Vertical, 3).unwrap();
    assert_ne!(first_guide, second_guide);
    core.verify_journal_replay().unwrap();
    let before_delete_all = core.document_info().unwrap().document_revision;
    assert_eq!(
        core.delete_all_guides().unwrap().revision(),
        before_delete_all + 1
    );
    assert!(core.guides().unwrap().is_empty());
    assert_eq!(last_commit_primitive(&core), PrimitiveId::DELETE_ALL_GUIDES);
    core.verify_journal_replay().unwrap();
    core.undo().unwrap();
    assert_eq!(core.guides().unwrap().len(), 2);
    core.redo().unwrap();
    assert!(core.guides().unwrap().is_empty());
    let before_noop = core.journal_state();
    let before_noop_revision = core.document_info().unwrap().document_revision;
    assert_eq!(
        core.delete_all_guides().unwrap().revision(),
        before_noop_revision
    );
    assert_eq!(core.journal_state(), before_noop);
    assert!(core.journal_state().unwrap().is_complete());
    core.verify_journal_replay().unwrap();

    let mut invalid = Core::new();
    invalid
        .new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let only_layer = invalid.layers().unwrap()[0].id;
    invalid
        .set_layer_properties(only_layer, false, true, 1_000, "Hidden")
        .unwrap();
    let before_invalid = invalid.document_state_digest().unwrap();
    assert!(matches!(
        invalid.delete_hidden_layers(),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(invalid.document_state_digest().unwrap(), before_invalid);
    invalid.verify_journal_replay().unwrap();
}

#[test]
fn selected_raster_layer_receives_stroke_preview_commit_and_history() {
    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let original_color_checksum = core.document_info().unwrap().color_plane_checksum;
    let (_, raster_layer_id) = core.create_layer(LayerKind::Raster, "Raster").unwrap();
    let raster_plane_id = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == raster_layer_id)
        .unwrap()
        .planes[0]
        .id;
    core.set_active_node(raster_layer_id, raster_plane_id)
        .unwrap();

    let stroke = color_stroke(
        PaintTool::Brush,
        3.0,
        StrokeSample {
            x: 3.0,
            y: 4.0,
            pressure: 1.0,
        },
    );
    core.begin_stroke(&stroke).unwrap();
    let preview = core.build_snapshot();
    let tile = &preview.tiles()[0];
    let offset = 4 * tile.stride_bytes() as usize + 3 * 4;
    assert_eq!(&tile.pixels()[offset..offset + 4], &[56, 34, 12, 255]);
    assert_eq!(
        core.document_info().unwrap().color_plane_checksum,
        original_color_checksum
    );

    core.end_stroke().unwrap();
    assert_eq!(
        core.document_info().unwrap().color_plane_checksum,
        original_color_checksum
    );
    let committed = core.build_snapshot();
    let tile = &committed.tiles()[0];
    let offset = 4 * tile.stride_bytes() as usize + 3 * 4;
    assert_eq!(&tile.pixels()[offset..offset + 4], &[56, 34, 12, 255]);
    core.undo().unwrap();
    assert_eq!(core.build_snapshot().tile_count(), 0);
    core.redo().unwrap();
    let redone = core.build_snapshot();
    let tile = &redone.tiles()[0];
    let offset = 4 * tile.stride_bytes() as usize + 3 * 4;
    assert_eq!(&tile.pixels()[offset..offset + 4], &[56, 34, 12, 255]);
}

#[test]
fn selected_raster_layer_receives_fill_without_changing_coloring_plane() {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let original_color_checksum = core.document_info().unwrap().color_plane_checksum;
    let (_, raster_layer_id) = core.create_layer(LayerKind::Raster, "Raster").unwrap();
    let raster_plane_id = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == raster_layer_id)
        .unwrap()
        .planes[0]
        .id;
    core.set_active_node(raster_layer_id, raster_plane_id)
        .unwrap();

    let outcome = core
        .apply_fill(&fill_request(1, 1, [90, 80, 70, 255]))
        .unwrap();
    assert_eq!(outcome.changed_pixels, 16);
    assert_eq!(
        core.document_info().unwrap().color_plane_checksum,
        original_color_checksum
    );
    let filled = core.build_snapshot();
    let tile = &filled.tiles()[0];
    let offset = tile.stride_bytes() as usize + 4;
    assert_eq!(&tile.pixels()[offset..offset + 4], &[70, 80, 90, 255]);
    core.undo().unwrap();
    assert_eq!(core.build_snapshot().tile_count(), 0);
}

#[test]
fn second_coloring_layer_fill_uses_its_own_main_line_boundary() {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(
        (0..4)
            .map(|y| StrokeSample {
                x: 1.0,
                y: y as f32,
                pressure: 1.0,
            })
            .collect(),
    ))
    .unwrap();

    let (_, second_layer_id) = core
        .create_layer(LayerKind::BinaryColoring, "Second")
        .unwrap();
    let second_color_id = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == second_layer_id)
        .unwrap()
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneType::Color)
        .unwrap()
        .id;
    core.set_active_node(second_layer_id, second_color_id)
        .unwrap();

    let outcome = core
        .apply_fill(&fill_request(0, 0, [90, 80, 70, 255]))
        .unwrap();
    assert_eq!(outcome.changed_pixels, 16);
}

#[test]
fn selected_plane_eyedropper_follows_second_coloring_target() {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    let sample = StrokeSample {
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
    };
    core.apply_stroke(&color_stroke(PaintTool::Pencil, 1.0, sample))
        .unwrap();

    let (_, second_layer_id) = core
        .create_layer(LayerKind::BinaryColoring, "Second")
        .unwrap();
    let second_color_id = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == second_layer_id)
        .unwrap()
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneType::Color)
        .unwrap()
        .id;
    core.set_active_node(second_layer_id, second_color_id)
        .unwrap();
    let mut second_stroke = color_stroke(PaintTool::Pencil, 1.0, sample);
    second_stroke.color = [200, 100, 50, 255];
    core.apply_stroke(&second_stroke).unwrap();

    assert_eq!(
        core.eyedropper(EyedropperSource::SelectedPlane, 1, 1)
            .unwrap(),
        PixelValue::Rgba([200, 100, 50, 255])
    );
}

#[test]
fn viewport_resize_refits_only_persistent_fit_or_one_to_one_modes() {
    let mut core = Core::new();
    core.new_cell(200, 100, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let fit = core
        .apply_view(ViewCommand::Fit {
            viewport_width: 400.0,
            viewport_height: 300.0,
        })
        .unwrap();
    assert_eq!(fit.mode(), ViewMode::Fit);
    let resized = core
        .apply_view(ViewCommand::ViewportResized {
            viewport_width: 800.0,
            viewport_height: 600.0,
        })
        .unwrap();
    assert_eq!(resized.mode(), ViewMode::Fit);
    assert!(resized.zoom() > fit.zoom());

    core.apply_view(ViewCommand::PanBy {
        device_dx: 10.0,
        device_dy: 5.0,
    })
    .unwrap();
    let manual = core
        .apply_view(ViewCommand::ViewportResized {
            viewport_width: 640.0,
            viewport_height: 480.0,
        })
        .unwrap();
    assert_eq!(manual.mode(), ViewMode::Manual);
    let repeated = core
        .apply_view(ViewCommand::ViewportResized {
            viewport_width: 320.0,
            viewport_height: 240.0,
        })
        .unwrap();
    assert_eq!(repeated.mode(), ViewMode::Manual);
    assert_eq!(repeated.zoom(), manual.zoom());
    assert_eq!(repeated.pan_x(), manual.pan_x());
    assert_eq!(repeated.pan_y(), manual.pan_y());
    assert_eq!(repeated.viewport_width(), 320.0);
    assert_eq!(repeated.viewport_height(), 240.0);
}

#[test]
fn view_coordinate_pan_viewport_and_invalid_results_preserve_document_state() {
    let mut core = Core::new();
    core.new_cell(8, 6, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let document_before = core.document_info().unwrap();

    let one_pixel = core
        .apply_view(ViewCommand::OneToOne {
            viewport_width: 1.0,
            viewport_height: 1.0,
        })
        .unwrap();
    assert_eq!(one_pixel.viewport_width(), 1.0);
    assert_eq!(one_pixel.viewport_height(), 1.0);

    core.apply_view(ViewCommand::PanBy {
        device_dx: 12.0,
        device_dy: 7.0,
    })
    .unwrap();
    let signed = core
        .apply_view(ViewCommand::PanBy {
            device_dx: -15.0,
            device_dy: -11.0,
        })
        .unwrap();
    assert_eq!((signed.pan_x(), signed.pan_y()), (-6.5, -6.5));

    let before_noop = core.view_state();
    let noop = core
        .apply_view(ViewCommand::PanBy {
            device_dx: 0.0,
            device_dy: 0.0,
        })
        .unwrap();
    assert_eq!(noop.revision(), before_noop.revision());

    core.apply_view(ViewCommand::PanBy {
        device_dx: 6.5,
        device_dy: 6.5,
    })
    .unwrap();
    let bounded = core
        .apply_view(ViewCommand::PanBy {
            device_dx: 16_777_216.0,
            device_dy: -16_777_216.0,
        })
        .unwrap();
    assert_eq!(bounded.pan_x(), 16_777_216.0);
    assert_eq!(bounded.pan_y(), -16_777_216.0);

    let before_invalid = core.view_state();
    assert!(
        core.apply_view(ViewCommand::PanBy {
            device_dx: 1.0,
            device_dy: 0.0,
        })
        .is_err()
    );
    assert!(
        core.apply_view(ViewCommand::Fit {
            viewport_width: f64::MAX,
            viewport_height: f64::MAX,
        })
        .is_err()
    );
    assert!(
        core.apply_view(ViewCommand::ViewportResized {
            viewport_width: 0.0,
            viewport_height: 10.0,
        })
        .is_err()
    );
    assert_eq!(core.view_state(), before_invalid);

    let document_after = core.document_info().unwrap();
    assert_eq!(
        document_after.document_revision,
        document_before.document_revision
    );
    assert_eq!(document_after.dirty, document_before.dirty);
    assert_eq!(document_after.can_undo, document_before.can_undo);
    assert_eq!(document_after.can_redo, document_before.can_redo);
}

#[test]
fn view_coordinate_hit_testing_is_half_open_and_independent_of_document_dpi() {
    let mut standard_dpi = Core::new();
    standard_dpi.new_cell(8, 6, 96_000, 96_000).unwrap();
    let mut high_dpi = Core::new();
    high_dpi.new_cell(8, 6, 300_000, 300_000).unwrap();

    for core in [&mut standard_dpi, &mut high_dpi] {
        core.apply_view(ViewCommand::Fit {
            viewport_width: 512.0,
            viewport_height: 384.0,
        })
        .unwrap();
    }
    assert_eq!(standard_dpi.view_state(), high_dpi.view_state());

    for core in [&mut standard_dpi, &mut high_dpi] {
        core.apply_view(ViewCommand::OneToOne {
            viewport_width: 8.0,
            viewport_height: 6.0,
        })
        .unwrap();
        core.apply_view(ViewCommand::PanBy {
            device_dx: 9.0,
            device_dy: -7.0,
        })
        .unwrap();
    }
    assert_eq!(standard_dpi.view_state(), high_dpi.view_state());

    let view = standard_dpi.view_state();
    let device = |document_x: f64, document_y: f64| {
        (
            document_x.mul_add(view.zoom(), view.pan_x()),
            document_y.mul_add(view.zoom(), view.pan_y()),
        )
    };
    for (document_x, document_y, expected) in [
        (0.0, 0.0, (0, 0)),
        (7.999_999, 5.999_999, (7, 5)),
        (8.0, 6.0, (8, 6)),
        (-0.000_001, -0.000_001, (-1, -1)),
    ] {
        let (device_x, device_y) = device(document_x, document_y);
        let standard = standard_dpi
            .locator_sample(None, device_x, device_y)
            .unwrap();
        let high = high_dpi.locator_sample(None, device_x, device_y).unwrap();
        assert_eq!((standard.document_x, standard.document_y), expected);
        assert_eq!((high.document_x, high.document_y), expected);
        assert_eq!(standard.color, high.color);
    }
}

#[test]
fn acceptance_layer_tree_undo_redo_save_reopen_and_validation() {
    let mut core = Core::new();
    let created = core
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let base_layer = created.layer_id;
    let (_, duplicate) = core.duplicate_layer(base_layer).unwrap();
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap().len(), 1);
    core.redo().unwrap();
    assert_eq!(core.layers().unwrap().len(), 2);
    core.reorder_layer(duplicate, 0).unwrap();
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap()[1].id, duplicate);
    core.redo().unwrap();
    let saved_order: Vec<_> = core
        .layers()
        .unwrap()
        .iter()
        .map(|layer| layer.id)
        .collect();
    assert_eq!(saved_order, vec![duplicate, base_layer]);

    let path = std::env::temp_dir().join(format!(
        "inkpod-test-tree-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    core.save(&path).unwrap();
    core.delete_layer(duplicate).unwrap();
    assert_eq!(core.layers().unwrap().len(), 1);
    core.undo().unwrap();
    assert_eq!(
        core.layers()
            .unwrap()
            .iter()
            .map(|layer| layer.id)
            .collect::<Vec<_>>(),
        saved_order
    );
    core.redo().unwrap();
    assert_eq!(core.layers().unwrap().len(), 1);
    core.save(&path).unwrap();

    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.layers().unwrap().len(), 1);
    reopened.undo().unwrap();
    assert_eq!(reopened.layers().unwrap().len(), 2);
    reopened.redo().unwrap();
    assert_eq!(reopened.layers().unwrap().len(), 1);

    let revision = reopened.document_info().unwrap().document_revision;
    assert!(matches!(
        reopened.create_plane(
            base_layer,
            PlaneType::Selection,
            PixelFormat::BinaryMask8,
            "Invalid Selection"
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(
        reopened.document_info().unwrap().document_revision,
        revision
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn required_singleton_and_incompatible_plane_operations_do_not_mutate_document() {
    let mut core = Core::new();
    let created = core
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let before_info = core.document_info().unwrap();
    let before_layers = core.layers().unwrap();
    let before_snapshot = core.build_snapshot();
    let revision = core.document_info().unwrap().document_revision;

    assert_eq!(
        core.duplicate_plane(created.main_plane_id),
        Err(CoreError::InvalidState(
            "required singleton planes cannot be duplicated"
        ))
    );
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.layers().unwrap(), before_layers);
    assert_eq!(core.build_snapshot(), before_snapshot);
    assert_eq!(core.document_info().unwrap().document_revision, revision);

    assert_eq!(
        core.merge_plane_into_below(created.main_plane_id),
        Err(CoreError::InvalidArgument(
            "only planes with compatible type and pixel format can merge"
        ))
    );
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.layers().unwrap(), before_layers);
    assert_eq!(core.build_snapshot(), before_snapshot);
    assert_eq!(core.document_info().unwrap().document_revision, revision);
}

#[test]
fn layer_thumbnail_preserves_aspect_content_and_hidden_layer_preview() {
    let mut core = Core::new();
    let created = core
        .new_cell(8, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.apply_stroke(&color_stroke(
        PaintTool::Pencil,
        1.0,
        StrokeSample {
            x: 7.0,
            y: 3.0,
            pressure: 1.0,
        },
    ))
    .unwrap();

    let before_visible = core.document_info().unwrap();
    let visible = core.layer_thumbnail(created.layer_id, 4, 4).unwrap();
    assert_eq!(
        (visible.width, visible.height, visible.stride_bytes),
        (4, 2, 16)
    );
    assert_eq!(visible.layer_id, created.layer_id);
    assert_eq!(
        visible.revision,
        core.document_info().unwrap().document_revision
    );
    assert_eq!(visible.pixels.len(), 32);
    assert!(visible.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
    assert_eq!(core.document_info().unwrap(), before_visible);

    core.set_layer_properties(created.layer_id, false, true, 1_000, "Coloring")
        .unwrap();
    let before_hidden = core.document_info().unwrap();
    let hidden = core.layer_thumbnail(created.layer_id, 4, 4).unwrap();
    assert_eq!(hidden.pixels, visible.pixels);
    assert!(matches!(
        core.layer_thumbnail(created.layer_id, 0, 4),
        Err(CoreError::InvalidArgument(_))
    ));
    assert!(matches!(
        core.layer_thumbnail(u64::MAX, 4, 4),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), before_hidden);
}

#[test]
fn acceptance_selection_authoring_tools() {
    let mut core = Core::new();
    core.new_cell(12, 12, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_selection(
        &SelectionShape::Ellipse(RectI32 {
            x: 2,
            y: 2,
            width: 8,
            height: 6,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    assert!(core.selection_bounds().unwrap().is_some());
    core.apply_selection(
        &SelectionShape::Polyline(vec![
            PointF32 { x: 1.0, y: 1.0 },
            PointF32 { x: 10.0, y: 1.0 },
            PointF32 { x: 5.0, y: 10.0 },
        ]),
        SelectionOperation::New,
    )
    .unwrap();
    assert!(core.selection_bounds().unwrap().is_some());
    core.apply_selection(
        &SelectionShape::Wand {
            x: 0,
            y: 0,
            tolerance: 0,
            gap_close: 0,
        },
        SelectionOperation::New,
    )
    .unwrap();
    assert_eq!(
        core.selection_bounds().unwrap(),
        Some(RectI32 {
            x: 0,
            y: 0,
            width: 12,
            height: 12,
        })
    );
    core.select_color(PixelValue::Binary(0), 0, false, SelectionOperation::New)
        .unwrap();
    assert_eq!(core.selection_bounds().unwrap().unwrap().width, 12);
    core.select_color(PixelValue::Binary(0), 0, true, SelectionOperation::New)
        .unwrap();
    assert_eq!(core.selection_bounds().unwrap(), None);
    core.apply_selection(
        &SelectionShape::Lasso(vec![
            PointF32 { x: 2.0, y: 2.0 },
            PointF32 { x: 9.0, y: 2.0 },
            PointF32 { x: 6.0, y: 9.0 },
        ]),
        SelectionOperation::New,
    )
    .unwrap();
    let lasso = core.selection_bounds().unwrap().unwrap();
    assert!(lasso.width >= 6 && lasso.height >= 6);
    core.apply_selection(
        &SelectionShape::Trace {
            points: vec![PointF32 { x: 0.0, y: 0.0 }, PointF32 { x: 11.0, y: 11.0 }],
            diameter: 1.5,
        },
        SelectionOperation::Add,
    )
    .unwrap();
    core.resize_selection(1).unwrap();
    core.resize_selection(-1).unwrap();
    let saved_bounds = core.selection_bounds().unwrap();
    let (_, selection_layer) = core.selection_to_layer("Saved Selection").unwrap();
    core.invert_selection().unwrap();
    core.selection_from_layer(selection_layer, SelectionLayerOperation::Replace)
        .unwrap();
    assert_eq!(core.selection_bounds().unwrap(), saved_bounds);
}

#[test]
fn locator_selection_bounds_cache_tracks_selection_history() {
    let mut core = Core::new();
    core.new_cell(4096, 4096, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    assert_eq!(
        core.locator_sample(None, 32.0, 32.0)
            .unwrap()
            .selection_bounds,
        None
    );

    let expected = RectI32 {
        x: 17,
        y: 23,
        width: 301,
        height: 205,
    };
    core.apply_selection(
        &SelectionShape::Rectangle(expected),
        SelectionOperation::New,
    )
    .unwrap();
    assert_eq!(
        core.locator_sample(None, 32.0, 32.0)
            .unwrap()
            .selection_bounds,
        Some(expected)
    );
    assert_eq!(core.selection_bounds().unwrap(), Some(expected));

    core.clear_selection().unwrap();
    assert_eq!(
        core.locator_sample(None, 32.0, 32.0)
            .unwrap()
            .selection_bounds,
        None
    );
    core.undo().unwrap();
    assert_eq!(
        core.locator_sample(None, 32.0, 32.0)
            .unwrap()
            .selection_bounds,
        Some(expected)
    );
    core.redo().unwrap();
    assert_eq!(
        core.locator_sample(None, 32.0, 32.0)
            .unwrap()
            .selection_bounds,
        None
    );
}

fn selected_pixels(core: &mut Core, width: u32, height: u32) -> Vec<(u32, u32)> {
    let snapshot = core.build_snapshot();
    let Some(pass) = snapshot
        .render_passes()
        .last()
        .filter(|pass| pass.kind() == RenderPassKind::RasterTiles && pass.layer_id() == 0)
    else {
        return Vec::new();
    };
    let first = usize::try_from(pass.first_item()).unwrap();
    let count = usize::try_from(pass.item_count()).unwrap();
    let mut result = Vec::new();
    for tile in &snapshot.tiles()[first..first + count] {
        for local_y in 0..tile.height() {
            for local_x in 0..tile.width() {
                let offset = local_y as usize * tile.stride_bytes() as usize + local_x as usize * 4;
                if tile.pixels()[offset] != 0 && tile.pixels()[offset + 1] != 0 {
                    let x = u32::try_from(tile.origin_x()).unwrap() + local_x;
                    let y = u32::try_from(tile.origin_y()).unwrap() + local_y;
                    if x < width && y < height {
                        result.push((x, y));
                    }
                }
            }
        }
    }
    result
}

fn raster_range_fixture_core() -> Core {
    let mut core = Core::new();
    core.new_cell(5, 5, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![
        StrokeSample {
            x: 1.0,
            y: 1.0,
            pressure: 1.0,
        },
        StrokeSample {
            x: 3.0,
            y: 1.0,
            pressure: 1.0,
        },
        StrokeSample {
            x: 3.0,
            y: 3.0,
            pressure: 1.0,
        },
        StrokeSample {
            x: 1.0,
            y: 3.0,
            pressure: 1.0,
        },
        StrokeSample {
            x: 1.0,
            y: 1.0,
            pressure: 1.0,
        },
    ]))
    .unwrap();
    core
}

#[test]
fn sel_004_raster_range_interpretations_are_atomic_replayable_and_exact() {
    let shape = SelectionShape::Rectangle(RectI32 {
        x: 0,
        y: 0,
        width: 5,
        height: 5,
    });
    let options = SelectionConstructionOptions::default();
    for (interpretation, expected) in [
        (RangeInterpretation::Normal, 25),
        (RangeInterpretation::Tight, 9),
        (RangeInterpretation::EnclosedInterior, 1),
        (RangeInterpretation::Drawing, 8),
        (RangeInterpretation::Boundary, 8),
    ] {
        let mut core = raster_range_fixture_core();
        let before = core.document_info().unwrap().document_revision;
        core.apply_selection_with_options(&shape, SelectionOperation::New, interpretation, options)
            .unwrap();
        if interpretation == RangeInterpretation::EnclosedInterior {
            assert_eq!(
                core.selection_bounds().unwrap(),
                Some(RectI32 {
                    x: 2,
                    y: 2,
                    width: 1,
                    height: 1,
                })
            );
        }
        assert_eq!(
            selected_pixels(&mut core, 5, 5).len(),
            expected,
            "{interpretation:?}"
        );
        assert_eq!(core.document_info().unwrap().document_revision, before + 1);
        core.verify_journal_replay()
            .unwrap_or_else(|error| panic!("{interpretation:?} replay failed: {error:?}"));
    }
    let mut core = raster_range_fixture_core();
    core.apply_selection_with_options(
        &shape,
        SelectionOperation::New,
        RangeInterpretation::Boundary,
        options,
    )
    .unwrap();
    let before_noop = core.document_info().unwrap();
    let history_before = core.history_entries().len();
    core.apply_selection_with_options(
        &shape,
        SelectionOperation::New,
        RangeInterpretation::Boundary,
        options,
    )
    .unwrap();
    assert_eq!(core.document_info().unwrap(), before_noop);
    assert_eq!(core.history_entries().len(), history_before);
    core.undo().unwrap();
    assert!(selected_pixels(&mut core, 5, 5).is_empty());
    core.redo().unwrap();
    assert_eq!(selected_pixels(&mut core, 5, 5).len(), 8);

    let before_invalid = core.document_state_digest().unwrap();
    let mut invalid = options;
    invalid.aspect_ratio_q16 = u32::MAX;
    assert!(matches!(
        core.apply_selection_with_options(
            &SelectionShape::RectangleGesture {
                anchor: PointF32 { x: 1.0, y: 1.0 },
                current: PointF32 { x: 2.0, y: 2.0 },
            },
            SelectionOperation::New,
            RangeInterpretation::Normal,
            invalid,
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_state_digest().unwrap(), before_invalid);
}

#[test]
fn sel_004_geometry_and_trace_options_share_one_mask_path() {
    let mut core = Core::new();
    core.new_cell(9, 9, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let geometry = SelectionConstructionOptions {
        aspect_ratio_q16: 1 << 16,
        from_center: true,
        constrain_rotation_45: true,
        rotation_turns: 0x1800_0000,
        ..SelectionConstructionOptions::default()
    };
    core.apply_selection_with_options(
        &SelectionShape::RectangleGesture {
            anchor: PointF32 { x: 4.5, y: 4.5 },
            current: PointF32 { x: 6.5, y: 5.5 },
        },
        SelectionOperation::New,
        RangeInterpretation::Normal,
        geometry,
    )
    .unwrap();
    let rotated = selected_pixels(&mut core, 9, 9);
    assert!(!rotated.is_empty());
    core.verify_journal_replay().unwrap();

    let sample = SelectionSample {
        x: 4.5,
        y: 4.5,
        pressure: 0.5,
    };
    let trace = |shape, pressure_size, screen_size, view_zoom_q16| SelectionConstructionOptions {
        trace: TraceBrushOptions {
            shape,
            pressure_size,
            screen_size,
            view_zoom_q16,
        },
        ..SelectionConstructionOptions::default()
    };
    core.apply_selection_with_options(
        &SelectionShape::TraceBrush {
            samples: vec![sample],
            diameter: 2.0,
        },
        SelectionOperation::New,
        RangeInterpretation::Normal,
        trace(TraceBrushShape::Round, false, false, 1 << 16),
    )
    .unwrap();
    assert_eq!(selected_pixels(&mut core, 9, 9).len(), 5);
    core.apply_selection_with_options(
        &SelectionShape::TraceBrush {
            samples: vec![sample],
            diameter: 2.0,
        },
        SelectionOperation::New,
        RangeInterpretation::Normal,
        trace(TraceBrushShape::Square, false, false, 1 << 16),
    )
    .unwrap();
    assert_eq!(selected_pixels(&mut core, 9, 9).len(), 9);
    core.apply_selection_with_options(
        &SelectionShape::TraceBrush {
            samples: vec![sample],
            diameter: 4.0,
        },
        SelectionOperation::New,
        RangeInterpretation::Normal,
        trace(TraceBrushShape::Round, true, true, 2 << 16),
    )
    .unwrap();
    assert_eq!(selected_pixels(&mut core, 9, 9).len(), 1);
}

#[test]
fn sel_004_new_empty_replaces_nonempty_once_and_then_is_a_no_op() {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let whole = SelectionShape::Rectangle(RectI32 {
        x: 0,
        y: 0,
        width: 4,
        height: 4,
    });
    core.apply_selection_with_options(
        &whole,
        SelectionOperation::New,
        RangeInterpretation::Normal,
        SelectionConstructionOptions::default(),
    )
    .unwrap();
    assert_eq!(selected_pixels(&mut core, 4, 4).len(), 16);
    let before_empty = core.document_info().unwrap().document_revision;
    core.apply_selection_with_options(
        &whole,
        SelectionOperation::New,
        RangeInterpretation::Drawing,
        SelectionConstructionOptions::default(),
    )
    .unwrap();
    assert!(selected_pixels(&mut core, 4, 4).is_empty());
    assert_eq!(
        core.document_info().unwrap().document_revision,
        before_empty + 1
    );
    let empty_info = core.document_info().unwrap();
    let empty_history = core.history_entries().len();
    core.apply_selection_with_options(
        &whole,
        SelectionOperation::New,
        RangeInterpretation::Drawing,
        SelectionConstructionOptions::default(),
    )
    .unwrap();
    assert_eq!(core.document_info().unwrap(), empty_info);
    assert_eq!(core.history_entries().len(), empty_history);
    core.undo().unwrap();
    assert_eq!(selected_pixels(&mut core, 4, 4).len(), 16);
    core.redo().unwrap();
    assert!(selected_pixels(&mut core, 4, 4).is_empty());
    core.verify_journal_replay().unwrap();
}

#[test]
fn acceptance_coordinate_preserving_typed_paste_and_floating_transform() {
    let mut source = Core::new();
    source
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    source.set_active_plane(ActivePlane::Color).unwrap();
    source
        .apply_stroke(&color_stroke(
            PaintTool::Pencil,
            1.0,
            StrokeSample {
                x: 6.0,
                y: 6.0,
                pressure: 1.0,
            },
        ))
        .unwrap();
    source
        .apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 6,
                y: 6,
                width: 1,
                height: 1,
            }),
            SelectionOperation::New,
        )
        .unwrap();
    let payload = source.copy_selection().unwrap();
    assert_eq!(payload.bounds.x, 6);
    assert_eq!(payload.planes[0].pixels[0].x, 6);

    let mut destination = Core::new();
    destination
        .new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    destination.begin_paste(&payload).unwrap();
    assert!(matches!(
        destination.commit_floating(),
        Err(CoreError::InvalidState(_))
    ));
    destination
        .set_floating_transform(FloatingTransform {
            translate_x: -4.0,
            translate_y: -4.0,
            ..FloatingTransform::default()
        })
        .unwrap();
    destination.commit_floating().unwrap();
    assert!(destination.journal_state().unwrap().is_complete());
    destination.verify_journal_replay().unwrap();
    assert_eq!(
        destination.plane_pixel(ActivePlane::Color, 2, 2).unwrap(),
        PixelValue::Rgba([12, 34, 56, 255])
    );
    destination.undo().unwrap();
    assert!(
        destination
            .plane_pixel(ActivePlane::Color, 2, 2)
            .unwrap()
            .is_zero()
    );
    let transform_payload = ClipboardPayload {
        source_document_uuid: 1,
        bounds: RectI32 {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
        planes: vec![ClipboardPlane {
            kind: PlaneType::Color,
            pixel_format: PixelFormat::StraightRgba8,
            origin_x: 0,
            origin_y: 0,
            pixels: vec![
                ClipboardPixel {
                    x: 0,
                    y: 0,
                    value: PixelValue::Rgba([255, 0, 0, 255]),
                },
                ClipboardPixel {
                    x: 1,
                    y: 0,
                    value: PixelValue::Rgba([0, 0, 255, 255]),
                },
            ],
            vector_paths: Vec::new(),
            vector_fills: Vec::new(),
        }],
    };
    destination.begin_paste(&transform_payload).unwrap();
    destination
        .set_floating_transform(FloatingTransform {
            translate_x: 1.0,
            scale_x: 2.0,
            ..FloatingTransform::default()
        })
        .unwrap();
    destination.commit_floating().unwrap();
    assert_eq!(
        (0..4)
            .map(|x| destination.plane_pixel(ActivePlane::Color, x, 0).unwrap())
            .collect::<Vec<_>>(),
        vec![
            PixelValue::Rgba([255, 0, 0, 255]),
            PixelValue::Rgba([255, 0, 0, 255]),
            PixelValue::Rgba([0, 0, 255, 255]),
            PixelValue::Rgba([0, 0, 255, 255]),
        ]
    );
    destination.undo().unwrap();
    destination.begin_paste(&transform_payload).unwrap();
    destination
        .set_floating_transform(FloatingTransform {
            rotation_degrees: 180.0,
            ..FloatingTransform::default()
        })
        .unwrap();
    destination.commit_floating().unwrap();
    assert_eq!(
        destination.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([0, 0, 255, 255])
    );
    assert_eq!(
        destination.plane_pixel(ActivePlane::Color, 1, 0).unwrap(),
        PixelValue::Rgba([255, 0, 0, 255])
    );
    destination.undo().unwrap();
    let revision = destination.document_info().unwrap().document_revision;
    destination.begin_paste(&payload).unwrap();
    destination.cancel_floating();
    assert!(matches!(
        destination.commit_floating(),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(
        destination.document_info().unwrap().document_revision,
        revision
    );
}

#[test]
fn converted_paste_to_new_plane_is_one_atomic_replayable_commit() {
    let payload = ClipboardPayload {
        source_document_uuid: 7,
        bounds: RectI32 {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        planes: vec![ClipboardPlane {
            kind: PlaneType::Color,
            pixel_format: PixelFormat::StraightRgba8,
            origin_x: 0,
            origin_y: 0,
            pixels: vec![ClipboardPixel {
                x: 0,
                y: 0,
                value: PixelValue::Rgba([21, 34, 55, 255]),
            }],
            vector_paths: Vec::new(),
            vector_fills: Vec::new(),
        }],
    };
    let mut core = Core::new();
    let created = core
        .new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let initial_layers = core.layers().unwrap();
    let initial_revision = core.document_info().unwrap().document_revision;
    let initial_history = core.history_entries().len();
    let initial_journal = core.journal_state();

    core.begin_paste_to_new_plane_converted(
        &payload,
        created.layer_id,
        PlaneType::Raster,
        PixelFormat::StraightRgba8,
        "Atomic Paste",
        875,
    )
    .unwrap();
    assert_eq!(core.layers().unwrap(), initial_layers);
    assert_eq!(
        core.document_info().unwrap().document_revision,
        initial_revision
    );
    assert_eq!(core.history_entries().len(), initial_history);
    assert_eq!(core.journal_state(), initial_journal);
    core.cancel_floating();
    assert_eq!(core.layers().unwrap(), initial_layers);
    assert_eq!(core.journal_state(), initial_journal);

    let invalid_revision = core.document_info().unwrap().document_revision;
    assert!(matches!(
        core.begin_paste_to_new_plane_converted(
            &payload,
            u64::MAX,
            PlaneType::Raster,
            PixelFormat::StraightRgba8,
            "Missing Layer",
            1_000,
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(
        core.document_info().unwrap().document_revision,
        invalid_revision
    );

    core.begin_paste_to_new_plane_converted(
        &payload,
        created.layer_id,
        PlaneType::Raster,
        PixelFormat::StraightRgba8,
        "Atomic Paste",
        875,
    )
    .unwrap();
    let committed = core.commit_floating().unwrap();
    assert_eq!(committed.revision(), initial_revision + 1);
    assert_eq!(core.history_entries().len(), initial_history + 1);
    assert_eq!(last_commit_primitive(&core), PrimitiveId::COMMIT_FLOATING);
    let layers = core.layers().unwrap();
    let pasted = layers[0]
        .planes
        .iter()
        .find(|plane| plane.name == "Atomic Paste")
        .expect("the pending plane is created at commit");
    assert_eq!(pasted.kind, PlaneType::Raster);
    assert_eq!(pasted.pixel_format, PixelFormat::StraightRgba8);
    assert_eq!(pasted.opacity_milli, 875);
    assert_eq!(
        core.build_snapshot().tiles()[0].pixels()[..4],
        [48, 30, 18, 223]
    );
    assert!(core.journal_state().unwrap().is_complete());
    core.verify_journal_replay().unwrap();

    core.undo().unwrap();
    assert_eq!(core.layers().unwrap(), initial_layers);
    core.redo().unwrap();
    assert!(
        core.layers().unwrap()[0]
            .planes
            .iter()
            .any(|plane| plane.name == "Atomic Paste")
    );
    core.verify_journal_replay().unwrap();
}

#[test]
fn acceptance_view_flip_and_destructive_mirror_have_separate_revisions() {
    let mut core = Core::new();
    core.new_cell(8, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let before = core.document_info().unwrap();
    let view = core
        .apply_view(ViewCommand::Flip {
            axis: MirrorAxis::Horizontal,
        })
        .unwrap();
    let after_view = core.document_info().unwrap();
    assert_eq!(after_view.document_revision, before.document_revision);
    assert!(after_view.view_revision > before.view_revision);
    assert!(view.flip_horizontal());
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 1, 1).unwrap(),
        PixelValue::Binary(255)
    );

    core.mirror_document(MirrorAxis::Horizontal).unwrap();
    let after_mirror = core.document_info().unwrap();
    assert!(after_mirror.document_revision > after_view.document_revision);
    assert_eq!(after_mirror.view_revision, after_view.view_revision);
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 6, 1).unwrap(),
        PixelValue::Binary(255)
    );
    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 1, 1).unwrap(),
        PixelValue::Binary(255)
    );
}

#[test]
fn acceptance_multi_view_locator_guides_grid_and_shortcuts() {
    let mut core = Core::new();
    core.new_cell(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let secondary = core.create_view().unwrap();
    core.apply_view_for(
        secondary,
        ViewCommand::BoxZoom {
            document_rect: RectI32 {
                x: 4,
                y: 4,
                width: 8,
                height: 8,
            },
            viewport_width: 160.0,
            viewport_height: 160.0,
        },
    )
    .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 3.0,
        y: 3.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let primary = core.build_snapshot();
    let other = core.build_snapshot_for(secondary).unwrap();
    assert_eq!(primary.revision(), other.revision());
    assert_ne!(primary.view(), other.view());

    let (_, guide_id) = core.add_guide(GuideAxis::Vertical, 5).unwrap();
    core.set_grid(GridConfig {
        origin_x: 0,
        origin_y: 0,
        spacing_x: 8,
        spacing_y: 8,
        subdivisions: 2,
    })
    .unwrap();
    assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (5.2, 7.8));
    core.apply_view(ViewCommand::SetGuideSnapEnabled(true))
        .unwrap();
    assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (5.0, 7.8));
    core.apply_view(ViewCommand::SetGuideSnapEnabled(false))
        .unwrap();
    core.apply_view(ViewCommand::SetGridSnapEnabled(true))
        .unwrap();
    assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (4.0, 8.0));
    core.apply_view(ViewCommand::SetGuideSnapEnabled(true))
        .unwrap();
    assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (5.0, 8.0));
    core.move_guide(guide_id, 6).unwrap();
    assert_eq!(core.guides().unwrap()[0].position, 6);
    let overlay_snapshot = core.build_snapshot();
    assert_eq!(overlay_snapshot.guides()[0].position, 6);
    assert_eq!(overlay_snapshot.grid().subdivisions, 2);

    let locator = core.locator_sample(None, 3.0, 3.0).unwrap();
    assert_eq!((locator.document_x, locator.document_y), (3, 3));
    core.rebind_shortcut(ShortcutBinding {
        command_id: 99,
        virtual_key: u32::from(b'Z'),
        modifiers: 1,
    })
    .unwrap();
    assert_eq!(
        core.resolve_shortcut(u32::from(b'Z'), SHORTCUT_MODIFIER_CONTROL)
            .unwrap(),
        Some(99)
    );
    assert!(
        !core
            .shortcut_bindings()
            .iter()
            .any(|binding| binding.command_id == 1)
    );
    assert!(
        core.shortcut_bindings()
            .iter()
            .any(|binding| binding.command_id == 99)
    );
    core.reset_shortcuts();
    assert_eq!(
        core.resolve_shortcut(u32::from(b'Z'), SHORTCUT_MODIFIER_CONTROL)
            .unwrap(),
        Some(1)
    );
    assert!(
        core.shortcut_bindings()
            .iter()
            .any(|binding| binding.command_id == 1)
    );
}

#[test]
fn shortcut_sequences_are_prefix_free_transactional_and_resettable() {
    let mut core = Core::new();
    let stroke = |key| ShortcutStroke {
        virtual_key: u32::from(key),
        modifiers: 0,
    };
    let defaults = vec![
        ShortcutSequenceBinding {
            command_id: 10,
            strokes: vec![stroke(b'Q'), stroke(b'F'), stroke(b'A')],
        },
        ShortcutSequenceBinding {
            command_id: 11,
            strokes: vec![stroke(b'Q'), stroke(b'F'), stroke(b'B')],
        },
        ShortcutSequenceBinding {
            command_id: 12,
            strokes: vec![ShortcutStroke {
                virtual_key: u32::from(b'S'),
                modifiers: SHORTCUT_MODIFIER_CONTROL,
            }],
        },
    ];
    core.set_shortcut_defaults(&defaults).unwrap();
    assert_eq!(
        core.resolve_shortcut_sequence(&[stroke(b'Q')]).unwrap(),
        ShortcutSequenceMatch::Prefix
    );
    assert_eq!(
        core.resolve_shortcut_sequence(&[stroke(b'Q'), stroke(b'F')])
            .unwrap(),
        ShortcutSequenceMatch::Prefix
    );
    assert_eq!(
        core.resolve_shortcut_sequence(&[stroke(b'Q'), stroke(b'F'), stroke(b'B')])
            .unwrap(),
        ShortcutSequenceMatch::Exact(11)
    );

    let conflicting = vec![
        ShortcutSequenceBinding {
            command_id: 20,
            strokes: vec![stroke(b'Q'), stroke(b'F')],
        },
        ShortcutSequenceBinding {
            command_id: 21,
            strokes: vec![stroke(b'Q'), stroke(b'F'), stroke(b'C')],
        },
    ];
    assert!(core.replace_shortcut_sequences(&conflicting).is_err());
    assert_eq!(core.shortcut_sequences(), defaults);

    core.rebind_shortcut_sequence(ShortcutSequenceBinding {
        command_id: 12,
        strokes: vec![stroke(b'Q'), stroke(b'F'), stroke(b'A')],
    })
    .unwrap();
    assert_eq!(
        core.resolve_shortcut_sequence(&[stroke(b'Q'), stroke(b'F'), stroke(b'A')])
            .unwrap(),
        ShortcutSequenceMatch::Exact(12)
    );
    assert!(
        !core
            .shortcut_sequences()
            .iter()
            .any(|binding| binding.command_id == 10)
    );

    core.reset_shortcuts();
    assert_eq!(core.shortcut_sequences(), defaults);
}

#[test]
fn tree_order_merge_names_and_active_ids_remain_consistent() {
    let mut core = Core::new();
    let created = core
        .new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.apply_stroke(&color_stroke(
        PaintTool::Pencil,
        1.0,
        StrokeSample {
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
        },
    ))
    .unwrap();
    let (_, top) = core.duplicate_layer(created.layer_id).unwrap();
    core.reorder_layer(top, 0).unwrap();
    let top_color = core
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| layer.id == top)
        .unwrap()
        .planes
        .into_iter()
        .find(|plane| plane.kind == PlaneType::Color)
        .unwrap()
        .id;
    core.set_active_node(top, top_color).unwrap();
    core.apply_stroke(&Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::Color,
        color: [0, 0, 255, 128],
        diameter: 1.0,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![StrokeSample {
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
        }],
    })
    .unwrap();
    assert_eq!(core.build_snapshot().tiles()[0].pixels(), [156, 17, 6, 255]);
    core.merge_layer_into_below(top).unwrap();
    assert_eq!(core.layers().unwrap().len(), 1);
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([6, 17, 156, 255])
    );
    let (_, raster_layer) = core.create_layer(LayerKind::Raster, "Raster").unwrap();
    let raster_plane = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == raster_layer)
        .unwrap()
        .planes[0]
        .id;
    core.duplicate_plane(raster_plane).unwrap();
    core.duplicate_plane(raster_plane).unwrap();
    let raster_names: BTreeSet<_> = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == raster_layer)
        .unwrap()
        .planes
        .iter()
        .map(|plane| plane.name.clone())
        .collect();
    assert_eq!(raster_names.len(), 3);

    let (_, duplicate_coloring) = core.duplicate_layer(created.layer_id).unwrap();
    core.create_layer(LayerKind::Frame, "Frame").unwrap();
    core.delete_layer(duplicate_coloring).unwrap();
    assert!(core.document_info().is_ok());
}

#[test]
fn editable_layer_and_plane_flags_guard_pixel_commands() {
    let mut core = Core::new();
    let created = core
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.set_plane_properties(created.color_plane_id, true, false, 1_000, "Color")
        .unwrap();
    let locked_revision = core.document_info().unwrap().document_revision;
    assert!(matches!(
        core.apply_stroke(&color_stroke(
            PaintTool::Pencil,
            1.0,
            StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            }
        )),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(
        core.document_info().unwrap().document_revision,
        locked_revision
    );

    core.set_plane_properties(created.color_plane_id, true, true, 1_000, "Color")
        .unwrap();
    core.set_layer_properties(created.layer_id, true, false, 1_000, "Coloring")
        .unwrap();
    let locked_revision = core.document_info().unwrap().document_revision;
    assert!(matches!(
        core.apply_fill(&fill_request(0, 0, [1, 2, 3, 255])),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(
        core.document_info().unwrap().document_revision,
        locked_revision
    );
}
