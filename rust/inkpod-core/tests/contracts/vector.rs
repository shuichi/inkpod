use super::*;

fn vector_line(
    start: (f32, f32),
    end: (f32, f32),
    width_start: f32,
    width_end: f32,
    color: [u8; 4],
) -> VectorPathInput {
    let third_x = (end.0 - start.0) / 3.0;
    let third_y = (end.1 - start.1) / 3.0;
    VectorPathInput {
        segments: vec![VectorCubicSegment {
            p0: PointF32 {
                x: start.0,
                y: start.1,
            },
            p1: PointF32 {
                x: start.0 + third_x,
                y: start.1 + third_y,
            },
            p2: PointF32 {
                x: start.0 + third_x * 2.0,
                y: start.1 + third_y * 2.0,
            },
            p3: PointF32 { x: end.0, y: end.1 },
            width_start,
            width_end,
        }],
        color: PixelValue::Rgba(color),
        closed: false,
    }
}

fn vector_rectangle(x0: f32, y0: f32, x1: f32, y1: f32) -> VectorPathInput {
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
    let mut segments = Vec::new();
    for pair in corners.windows(2) {
        segments.extend(vector_line(pair[0], pair[1], 1.0, 1.0, [0, 0, 0, 255]).segments);
    }
    VectorPathInput {
        segments,
        color: PixelValue::Rgba([0, 0, 0, 255]),
        closed: true,
    }
}

fn vector_core(width: u32, height: u32) -> (Core, u64, u64, u64, u64) {
    let mut core = Core::new();
    core.new_cell(width, height, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let (_, layer_id) = core
        .create_layer(LayerKind::VectorColoring, "Vector")
        .unwrap();
    let (main_id, trace_id, fill_id) = core.vector_layer_planes(layer_id).unwrap();
    (core, layer_id, main_id, trace_id, fill_id)
}

#[test]
fn acceptance_zoom_never_changes_core_vector_geometry() {
    let (mut core, _, main_id, _, _) = vector_core(32, 32);
    core.vector_add_path(
        main_id,
        VectorPathInput {
            segments: vec![VectorCubicSegment {
                p0: PointF32 { x: 2.0, y: 3.0 },
                p1: PointF32 { x: 8.0, y: 1.0 },
                p2: PointF32 { x: 16.0, y: 20.0 },
                p3: PointF32 { x: 28.0, y: 24.0 },
                width_start: 1.25,
                width_end: 4.75,
            }],
            color: PixelValue::Rgba16([257, 2_000, 40_000, 65_535]),
            closed: false,
        },
    )
    .unwrap();
    let revision = core.document_info().unwrap().document_revision;
    let paths_before = core.vector_paths().unwrap();
    let snapshot_before = core.build_snapshot();
    core.apply_view(ViewCommand::ZoomAt {
        factor: 8.0,
        device_x: 11.0,
        device_y: 13.0,
    })
    .unwrap();
    let snapshot_after = core.build_snapshot();
    assert_eq!(core.document_info().unwrap().document_revision, revision);
    assert_eq!(core.vector_paths().unwrap(), paths_before);
    assert_eq!(
        snapshot_before.vector_segments(),
        snapshot_after.vector_segments()
    );
    assert_eq!(snapshot_before.vector_segments().len(), 1);
    assert_ne!(snapshot_before.view().zoom(), snapshot_after.view().zoom());
}

#[test]
fn acceptance_partial_erase_changes_only_the_touched_stroke() {
    let (mut core, _, main_id, _, _) = vector_core(12, 10);
    let (_, touched_id) = core
        .vector_add_path(
            main_id,
            vector_line((1.0, 2.0), (11.0, 2.0), 1.0, 3.0, [10, 20, 30, 255]),
        )
        .unwrap();
    let (_, protected_id) = core
        .vector_add_path(
            main_id,
            vector_line((1.0, 7.0), (11.0, 7.0), 2.0, 2.0, [90, 80, 70, 255]),
        )
        .unwrap();
    let protected_before = core
        .vector_paths()
        .unwrap()
        .into_iter()
        .find(|path| path.id == protected_id)
        .unwrap();
    core.vector_erase(
        main_id,
        PointF32 { x: 6.0, y: 2.0 },
        1.0,
        VectorEraseMode::Partial,
    )
    .unwrap();
    let paths = core.vector_paths().unwrap();
    assert_eq!(
        paths.iter().find(|path| path.id == protected_id),
        Some(&protected_before)
    );
    assert_eq!(
        paths.iter().filter(|path| path.plane_id == main_id).count(),
        3
    );
    assert!(paths.iter().any(|path| path.id == touched_id));
    core.undo().unwrap();
    assert_eq!(core.vector_paths().unwrap().len(), 2);
    core.redo().unwrap();
    assert_eq!(core.vector_paths().unwrap().len(), 3);
    core.vector_erase(
        main_id,
        PointF32 { x: 6.0, y: 7.0 },
        1.0,
        VectorEraseMode::WholePath,
    )
    .unwrap();
    assert!(
        !core
            .vector_paths()
            .unwrap()
            .iter()
            .any(|path| path.id == protected_id)
    );
    core.undo().unwrap();
    assert!(
        core.vector_paths()
            .unwrap()
            .iter()
            .any(|path| path.id == protected_id)
    );
}

#[test]
fn acceptance_intersection_erase_cut_points_are_deterministic() {
    fn erased() -> Vec<VectorPathInfo> {
        let (mut core, _, main_id, _, _) = vector_core(10, 10);
        core.vector_add_path(
            main_id,
            vector_line((1.0, 5.0), (9.0, 5.0), 1.0, 1.0, [0, 0, 0, 255]),
        )
        .unwrap();
        core.vector_add_path(
            main_id,
            vector_line((3.0, 1.0), (3.0, 9.0), 1.0, 1.0, [1, 2, 3, 255]),
        )
        .unwrap();
        core.vector_add_path(
            main_id,
            vector_line((7.0, 1.0), (7.0, 9.0), 1.0, 1.0, [4, 5, 6, 255]),
        )
        .unwrap();
        core.vector_erase(
            main_id,
            PointF32 { x: 5.0, y: 5.0 },
            0.25,
            VectorEraseMode::ToIntersection,
        )
        .unwrap();
        core.vector_paths().unwrap()
    }
    let first = erased();
    let second = erased();
    assert_eq!(first, second);
    let horizontal: Vec<_> = first
        .iter()
        .filter(|path| path.color == PixelValue::Rgba([0, 0, 0, 255]))
        .collect();
    assert_eq!(horizontal.len(), 2);
    assert_eq!(horizontal[0].segments[0].p3, PointF32 { x: 3.0, y: 5.0 });
    assert_eq!(horizontal[1].segments[0].p0, PointF32 { x: 7.0, y: 5.0 });
    assert_eq!(
        first
            .iter()
            .filter(|path| path.color != PixelValue::Rgba([0, 0, 0, 255]))
            .count(),
        2
    );
}

#[test]
fn acceptance_fill_topology_survives_save_and_reopen() {
    let (mut core, layer_id, _, trace_id, fill_plane_id) = vector_core(8, 8);
    let (_, boundary_id) = core
        .vector_add_path(trace_id, vector_rectangle(1.0, 1.0, 7.0, 7.0))
        .unwrap();
    let (_, fill_id) = core
        .vector_add_fill(
            fill_plane_id,
            &[boundary_id],
            PixelValue::Rgba16([60_000, 1_000, 2_000, 50_000]),
        )
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "inkpod-core-test-topology-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let before_paths = core.vector_paths().unwrap();
    let before_fills = core.vector_fills().unwrap();
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.vector_paths().unwrap(), before_paths);
    assert_eq!(reopened.vector_fills().unwrap(), before_fills);
    assert_eq!(reopened.vector_fills().unwrap()[0].id, fill_id);
    let reopened_layer = reopened
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| layer.id == layer_id)
        .unwrap();
    assert_eq!(reopened_layer.kind, LayerKind::VectorColoring);
    let snapshot = reopened.build_snapshot();
    assert_eq!(snapshot.vector_fills().len(), 1);
    assert_eq!(snapshot.vector_segments().len(), 4);
    let _ = std::fs::remove_file(path);
}

#[test]
fn acceptance_rasterize_antialias_pixel_center_and_scale_golden() {
    let (mut core, layer_id, main_id, _, _) = vector_core(4, 4);
    core.vector_add_path(
        main_id,
        vector_line((0.0, 1.0), (4.0, 1.0), 1.0, 1.0, [255, 0, 0, 255]),
    )
    .unwrap();
    let no_aa = core.rasterize_vector_layer(layer_id, 1, false).unwrap();
    let red = [255_u8, 0, 0, 255];
    let clear = [0_u8; 4];
    assert_eq!(
        no_aa.pixels,
        [
            red, red, red, red, red, red, red, red, clear, clear, clear, clear, clear, clear,
            clear, clear
        ]
        .concat()
    );
    let aa = core.rasterize_vector_layer(layer_id, 1, true).unwrap();
    let half_red = [255_u8, 0, 0, 128];
    assert_eq!(
        aa.pixels,
        [
            half_red, half_red, half_red, half_red, half_red, half_red, half_red, half_red, clear,
            clear, clear, clear, clear, clear, clear, clear
        ]
        .concat()
    );
    let scaled = core.rasterize_vector_layer(layer_id, 2, false).unwrap();
    assert_eq!(
        (scaled.width, scaled.height, scaled.stride_bytes),
        (8, 8, 32)
    );
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 32 + x * 4;
            let expected = if y == 1 || y == 2 { red } else { clear };
            assert_eq!(&scaled.pixels[offset..offset + 4], &expected);
        }
    }
}

#[test]
fn vector_rasterize_to_document_creates_one_undoable_rgba_layer() {
    let (mut core, layer_id, main_id, _, _) = vector_core(4, 4);
    core.vector_add_path(
        main_id,
        vector_line((0.0, 1.0), (4.0, 1.0), 1.0, 1.0, [255, 0, 0, 255]),
    )
    .unwrap();
    let layer_count = core.layers().unwrap().len();
    let revision = core.document_info().unwrap().document_revision;
    let (outcome, raster_layer_id) = core
        .rasterize_vector_layer_to_document(layer_id, true, "Rasterized")
        .unwrap();
    assert_eq!(outcome.accepted_commands(), 1);
    assert!(outcome.revision() > revision);
    let layers = core.layers().unwrap();
    assert_eq!(layers.len(), layer_count + 1);
    let raster_layer = layers
        .iter()
        .find(|layer| layer.id == raster_layer_id)
        .unwrap();
    assert_eq!(raster_layer.kind, LayerKind::Raster);
    assert_eq!(raster_layer.planes.len(), 1);
    assert_eq!(
        raster_layer.planes[0].pixel_format,
        PixelFormat::StraightRgba8
    );
    assert!(
        core.build_snapshot()
            .tiles()
            .iter()
            .any(|tile| tile.pixels().chunks_exact(4).any(|pixel| pixel[3] != 0))
    );
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap().len(), layer_count);
    assert!(
        core.layers()
            .unwrap()
            .iter()
            .all(|layer| layer.id != raster_layer_id)
    );
}

#[test]
fn vector_002_connect_width_select_and_raster_vector_conversion_are_transactional() {
    let (mut core, layer_id, main_id, trace_id, _) = vector_core(4, 4);
    let revision = core.document_info().unwrap().document_revision;
    let mut too_thin = vector_line((0.0, 0.0), (1.0, 0.0), 0.0001, 1.0, [0, 0, 0, 255]);
    too_thin.segments[0].width_end = 0.0001;
    assert!(matches!(
        core.vector_add_path(main_id, too_thin),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap().document_revision, revision);

    let (_, left_id) = core
        .vector_add_path(
            main_id,
            vector_line((0.0, 1.0), (1.0, 1.0), 1.0, 1.0, [0, 0, 0, 255]),
        )
        .unwrap();
    let (_, right_id) = core
        .vector_add_path(
            main_id,
            vector_line((2.0, 1.0), (3.0, 1.0), 1.0, 1.0, [0, 0, 0, 255]),
        )
        .unwrap();
    let (_, connector_id) = core.vector_connect(main_id, 1.5).unwrap();
    let connector_id = connector_id.unwrap();
    let revision_after_connect = core.document_info().unwrap().document_revision;
    let (outcome, repeated_connector) = core.vector_connect(main_id, 1.5).unwrap();
    assert!(repeated_connector.is_none());
    assert_eq!(outcome.revision(), revision_after_connect);
    core.vector_correct_width(
        &[left_id, right_id, connector_id],
        VectorWidthMode::Add(1.0),
    )
    .unwrap();
    core.vector_correct_width(
        &[left_id, right_id, connector_id],
        VectorWidthMode::Subtract(0.5),
    )
    .unwrap();
    core.vector_correct_width(
        &[left_id, right_id, connector_id],
        VectorWidthMode::Scale(2.0),
    )
    .unwrap();
    core.vector_correct_width(
        &[left_id, right_id, connector_id],
        VectorWidthMode::Constant(2.0),
    )
    .unwrap();
    assert!(core.vector_paths().unwrap().iter().all(|path| {
        path.segments
            .iter()
            .all(|segment| segment.width_start == 2.0)
    }));
    let revision_after_width = core.document_info().unwrap().document_revision;
    let outcome = core
        .vector_correct_width(
            &[left_id, right_id, connector_id],
            VectorWidthMode::Constant(2.0),
        )
        .unwrap();
    assert_eq!(outcome.revision(), revision_after_width);
    let selected = core
        .vector_select(
            RectI32 {
                x: 0,
                y: 0,
                width: 4,
                height: 3,
            },
            VectorSelectionMode::Touching,
        )
        .unwrap();
    assert_eq!(selected.path_ranges.len(), 3);

    // Path creation order must not put a later color trace over the
    // protected main-line plane.
    core.vector_add_path(
        trace_id,
        vector_line((0.0, 1.0), (3.0, 1.0), 1.0, 1.0, [255, 0, 0, 255]),
    )
    .unwrap();
    let snapshot = core.build_snapshot();
    assert_eq!(snapshot.vector_segments()[0].plane_id, trace_id);
    assert!(
        snapshot.vector_segments()[1..]
            .iter()
            .all(|segment| segment.plane_id == main_id)
    );
    let rasterized = core.rasterize_vector_layer(layer_id, 1, false).unwrap();
    assert_eq!(&rasterized.pixels[0..4], &[0, 0, 0, 255]);

    let (_, raster_layer_id) = core.create_layer(LayerKind::Raster, "Source").unwrap();
    let raster_plane_id = core
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| layer.id == raster_layer_id)
        .unwrap()
        .planes[0]
        .id;
    let before_empty_conversion = core.document_info().unwrap().document_revision;
    let (outcome, fill_ids) = core
        .vectorize_raster_plane(raster_plane_id, layer_id, 0)
        .unwrap();
    assert!(fill_ids.is_empty());
    assert_eq!(outcome.revision(), before_empty_conversion);
    assert_eq!(
        core.document_info().unwrap().document_revision,
        before_empty_conversion
    );
    core.set_layer_properties(layer_id, true, false, 1_000, "Vector")
        .unwrap();
    assert!(matches!(
        core.vectorize_raster_plane(raster_plane_id, layer_id, 1),
        Err(CoreError::InvalidState(_))
    ));
    core.set_layer_properties(layer_id, true, true, 1_000, "Vector")
        .unwrap();
    core.set_active_node(raster_layer_id, raster_plane_id)
        .unwrap();
    core.apply_stroke(&Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::Color,
        color: [7, 8, 9, 255],
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
    let before_revision = core.document_info().unwrap().document_revision;
    let (outcome, fill_ids) = core
        .vectorize_raster_plane(raster_plane_id, layer_id, 1)
        .unwrap();
    assert_eq!(outcome.revision(), before_revision + 1);
    assert_eq!(fill_ids.len(), 1);
    core.undo().unwrap();
    assert_eq!(core.vector_fills().unwrap().len(), 0);
    core.redo().unwrap();
    assert_eq!(core.vector_fills().unwrap().len(), 1);
    assert!(core.journal_state().unwrap().is_complete());
    core.verify_journal_replay().unwrap();
}

#[test]
fn vector_002_all_selection_modes_have_deterministic_ranges_and_ids() {
    let (mut core, _, main_id, trace_id, fill_plane_id) = vector_core(8, 8);
    let (_, horizontal_id) = core
        .vector_add_path(
            main_id,
            vector_line((0.0, 4.0), (6.0, 4.0), 1.0, 1.0, [0, 0, 0, 255]),
        )
        .unwrap();
    for x in [1.0, 5.0] {
        core.vector_add_path(
            main_id,
            vector_line((x, 0.0), (x, 8.0), 1.0, 1.0, [0, 0, 0, 255]),
        )
        .unwrap();
    }
    let (_, boundary_id) = core
        .vector_add_path(trace_id, vector_rectangle(1.0, 1.0, 7.0, 7.0))
        .unwrap();
    let (_, fill_id) = core
        .vector_add_fill(
            fill_plane_id,
            &[boundary_id],
            PixelValue::Rgba([20, 40, 60, 255]),
        )
        .unwrap();
    let center = RectI32 {
        x: 2,
        y: 3,
        width: 2,
        height: 2,
    };

    let cut = core
        .vector_select(center, VectorSelectionMode::CutBySelection)
        .unwrap();
    assert_eq!(
        cut.path_ranges,
        vec![VectorSelectionRange {
            path_id: horizontal_id,
            start_million: 333_333,
            end_million: 666_667,
        }]
    );
    for mode in [
        VectorSelectionMode::Touching,
        VectorSelectionMode::Line,
        VectorSelectionMode::WholeLine,
    ] {
        assert_eq!(
            core.vector_select(center, mode).unwrap().path_ranges,
            vec![VectorSelectionRange {
                path_id: horizontal_id,
                start_million: 0,
                end_million: 1_000_000,
            }]
        );
    }
    assert_eq!(
        core.vector_select(center, VectorSelectionMode::ToIntersection)
            .unwrap()
            .path_ranges,
        vec![VectorSelectionRange {
            path_id: horizontal_id,
            start_million: 166_667,
            end_million: 833_333,
        }]
    );
    assert_eq!(
        core.vector_select(center, VectorSelectionMode::FillBoundary)
            .unwrap()
            .path_ranges,
        vec![VectorSelectionRange {
            path_id: boundary_id,
            start_million: 0,
            end_million: 1_000_000,
        }]
    );
    assert_eq!(
        core.vector_select(center, VectorSelectionMode::Fill)
            .unwrap()
            .fill_ids,
        vec![fill_id]
    );
    let contained = core
        .vector_select(
            RectI32 {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
            VectorSelectionMode::FullyContained,
        )
        .unwrap();
    assert_eq!(contained.path_ranges.len(), 4);
    assert!(
        contained
            .path_ranges
            .iter()
            .all(|range| range.start_million == 0 && range.end_million == 1_000_000)
    );
}

#[test]
fn vectorize_into_new_layer_is_one_atomic_replayable_primitive() {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let (_, source_layer_id) = core.create_layer(LayerKind::Raster, "Source").unwrap();
    let source_plane_id = core
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| layer.id == source_layer_id)
        .unwrap()
        .planes[0]
        .id;

    let before_empty = core.journal_state();
    let (empty, layer_id, fill_ids) = core
        .vectorize_raster_plane_into_new_layer(source_plane_id, 1, "Vectorized")
        .unwrap();
    assert_eq!(layer_id, 0);
    assert!(fill_ids.is_empty());
    assert_eq!(core.journal_state(), before_empty);
    assert_eq!(
        empty.revision(),
        core.document_info().unwrap().document_revision
    );

    core.set_active_node(source_layer_id, source_plane_id)
        .unwrap();
    core.apply_stroke(&Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::Color,
        color: [10, 20, 30, 255],
        diameter: 1.0,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![StrokeSample {
            x: 1.0,
            y: 1.0,
            pressure: 1.0,
        }],
    })
    .unwrap();
    let before_layers = core.layers().unwrap().len();
    let before_history = core.history_entries().len();
    let before_revision = core.document_info().unwrap().document_revision;
    let (outcome, vector_layer_id, fill_ids) = core
        .vectorize_raster_plane_into_new_layer(source_plane_id, 1, "Vectorized")
        .unwrap();
    assert_eq!(outcome.revision(), before_revision + 1);
    assert_ne!(vector_layer_id, 0);
    assert_eq!(fill_ids.len(), 1);
    assert_eq!(core.layers().unwrap().len(), before_layers + 1);
    assert_eq!(core.history_entries().len(), before_history + 1);
    let primitive = core
        .journal_entries()
        .iter()
        .rev()
        .find_map(|entry| match entry {
            JournalEntry::Commit(commit) => Some(commit.procedure().primitive_id()),
            JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => None,
        })
        .unwrap();
    assert_eq!(
        primitive,
        PrimitiveId::VECTORIZE_RASTER_PLANE_INTO_NEW_LAYER
    );
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap().len(), before_layers);
    assert!(core.vector_fills().unwrap().is_empty());
    core.redo().unwrap();
    assert_eq!(core.layers().unwrap().len(), before_layers + 1);
    assert_eq!(core.vector_fills().unwrap().len(), 1);
    assert!(core.journal_state().unwrap().is_complete());
    core.verify_journal_replay().unwrap();

    let before_invalid = core.document_state_digest().unwrap();
    assert!(matches!(
        core.vectorize_raster_plane_into_new_layer(source_plane_id, 1, ""),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_state_digest().unwrap(), before_invalid);
}
