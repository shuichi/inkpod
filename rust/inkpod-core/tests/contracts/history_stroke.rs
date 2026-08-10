use super::*;

#[test]
fn hist_001_redo_branch_is_discarded_and_savepoint_tracks_undo() {
    let mut core = Core::new();
    core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
    }]))
    .unwrap();
    core.undo().unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 2.0,
        y: 2.0,
        pressure: 1.0,
    }]))
    .unwrap();
    assert!(!core.document_info().unwrap().can_redo);
}

#[test]
fn hist_001_new_cell_starts_clean_and_tracks_initial_savepoint() {
    let mut core = Core::new();
    let created = core
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    assert!(!created.dirty);

    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
    }]))
    .unwrap();
    assert!(core.document_info().unwrap().dirty);

    core.undo().unwrap();
    let restored = core.document_info().unwrap();
    assert!(!restored.dirty);
    assert!(restored.can_redo);

    core.redo().unwrap();
    assert!(core.document_info().unwrap().dirty);
}

#[test]
fn hist_001_savepoint_undo_redo_and_revert_are_distinct() {
    let path = std::env::temp_dir().join(format!(
        "inkpod-core-savepoint-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut core = Core::new();
    core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
    }]))
    .unwrap();
    assert!(!core.save(&path).unwrap().dirty);
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 2.0,
        y: 2.0,
        pressure: 1.0,
    }]))
    .unwrap();
    assert!(core.document_info().unwrap().dirty);
    core.undo().unwrap();
    assert!(!core.document_info().unwrap().dirty);
    core.redo().unwrap();
    assert!(core.document_info().unwrap().dirty);
    core.revert().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 2, 2).unwrap(),
        PixelValue::Binary(0)
    );
    assert!(!core.document_info().unwrap().dirty);
    fs::remove_file(&path).unwrap();
    assert!(!path.exists());
    assert!(!core.save(&path).unwrap().dirty);
    assert!(path.exists());
    fs::remove_file(path).unwrap();
}

#[test]
fn hist_001_history_jump_and_partial_selection_revert_are_transactional() {
    let path = std::env::temp_dir().join(format!(
        "inkpod-core-history-list-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.save(&path).unwrap();
    for (x, y) in [(1.0, 1.0), (6.0, 6.0)] {
        core.apply_stroke(&color_stroke(
            PaintTool::Pencil,
            1.0,
            StrokeSample {
                x,
                y,
                pressure: 1.0,
            },
        ))
        .unwrap();
    }
    core.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    let before_revert = core.history_entries().len();
    core.revert_active_plane_selection().unwrap();
    assert_eq!(core.history_entries().len(), before_revert + 1);
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 1, 1).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    assert_ne!(
        core.plane_pixel(ActivePlane::Color, 6, 6).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    core.undo().unwrap();
    assert_ne!(
        core.plane_pixel(ActivePlane::Color, 1, 1).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    let full_cursor = core.history_entries().len();
    core.jump_history(0).unwrap();
    assert_eq!(core.history_cursor(), 0);
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 6, 6).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    core.jump_history(full_cursor).unwrap();
    assert_eq!(core.history_cursor(), full_cursor);
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 1, 1).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    assert!(core.history_entries().iter().all(|entry| entry.applied));
    fs::remove_file(path).unwrap();
}

#[test]
fn live_stroke_preview_is_visible_before_one_atomic_commit() {
    let mut core = Core::new();
    let created = core
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let first = line_stroke(vec![StrokeSample {
        x: 8.0,
        y: 8.0,
        pressure: 1.0,
    }]);
    core.begin_stroke(&first).unwrap();
    let during_begin = core.document_info().unwrap();
    assert_eq!(during_begin.document_revision, created.document_revision);
    assert_eq!(
        during_begin.main_plane_checksum,
        created.main_plane_checksum
    );
    assert_eq!(during_begin.dirty, created.dirty);
    assert!(!during_begin.can_undo);
    assert_eq!(core.build_snapshot().tile_count(), 1);
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 8, 8).unwrap(),
        PixelValue::Binary(0)
    );

    core.append_stroke(&[StrokeSample {
        x: 24.0,
        y: 8.0,
        pressure: 1.0,
    }])
    .unwrap();
    let preview = core.build_snapshot();
    assert!(preview.revision() >= 1_u64 << 63);
    assert_eq!(core.document_info().unwrap(), during_begin);

    core.end_stroke().unwrap();
    let committed = core.document_info().unwrap();
    assert_eq!(committed.document_revision, created.document_revision + 1);
    assert!(committed.dirty && committed.can_undo);
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 24, 8).unwrap(),
        PixelValue::Binary(255)
    );
    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 8, 8).unwrap(),
        PixelValue::Binary(0)
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 24, 8).unwrap(),
        PixelValue::Binary(0)
    );
}

#[test]
fn cancelling_live_stroke_restores_base_snapshot_without_revision_change() {
    let mut core = Core::new();
    let created = core
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.begin_stroke(&line_stroke(vec![StrokeSample {
        x: 12.0,
        y: 12.0,
        pressure: 1.0,
    }]))
    .unwrap();
    assert_eq!(core.build_snapshot().tile_count(), 1);
    core.cancel_stroke();
    assert_eq!(core.build_snapshot().tile_count(), 0);
    assert_eq!(core.document_info().unwrap(), created);
}

#[test]
fn failed_live_append_discards_preview_without_partial_commit() {
    let mut core = Core::new();
    let created = core
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let first = StrokeSample {
        x: 32.0,
        y: 32.0,
        pressure: 1.0,
    };
    core.begin_stroke(&color_stroke(PaintTool::Brush, 256.0, first))
        .unwrap();
    let excessive = vec![first; 300];
    assert!(matches!(
        core.append_stroke(&excessive),
        Err(CoreError::InvalidArgument(_))
    ));
    assert!(!core.stroke_is_active());
    assert_eq!(core.build_snapshot().tile_count(), 0);
    assert_eq!(core.document_info().unwrap(), created);
}

#[test]
fn incremental_preview_batching_matches_one_shot_digest_and_history() {
    let mut samples = vec![
        StrokeSample {
            x: -100.0,
            y: -20.0,
            pressure: 0.01,
        },
        StrokeSample {
            x: 100.0,
            y: 70.0,
            pressure: 0.01,
        },
    ];
    for index in 0..94 {
        samples.push(StrokeSample {
            x: 4.0 + index as f32 * 0.5,
            y: 12.0 + ((index % 7) as f32 * 0.25),
            pressure: (index as f32 + 1.0) / 94.0,
        });
    }
    let stroke = Stroke {
        tool: PaintTool::Brush,
        plane: ActivePlane::Color,
        color: [17, 91, 203, 211],
        diameter: 19.0,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: true,
        coordinate_space: CoordinateSpace::Document,
        samples: samples.clone(),
    };

    let mut one_shot = Core::new();
    one_shot
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    one_shot.apply_stroke(&stroke).unwrap();

    let mut incremental = Core::new();
    incremental
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let mut first = stroke.clone();
    first.samples = samples[..3].to_vec();
    incremental.begin_stroke(&first).unwrap();
    for batch in samples[3..].chunks(11) {
        incremental.append_stroke(batch).unwrap();
    }
    let one_shot_snapshot = one_shot.build_snapshot();
    let incremental_snapshot = incremental.build_snapshot();
    assert_eq!(
        incremental_snapshot.tile_count(),
        one_shot_snapshot.tile_count()
    );
    for (actual, expected) in incremental_snapshot
        .tiles()
        .iter()
        .zip(one_shot_snapshot.tiles())
    {
        assert_eq!(actual.tile_id(), expected.tile_id());
        assert_eq!(actual.pixels(), expected.pixels());
    }
    incremental.end_stroke().unwrap();

    assert_eq!(
        incremental.document_state_digest().unwrap(),
        one_shot.document_state_digest().unwrap()
    );
    assert_eq!(incremental.history_entries(), one_shot.history_entries());
    assert_eq!(
        incremental.document_info().unwrap(),
        one_shot.document_info().unwrap()
    );
}

#[test]
fn pressure_radius_growth_rechecks_aggregate_work_and_discards_failed_preview() {
    let mut core = Core::new();
    let created = core
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let stable_tree = core.layers().unwrap();
    let stroke = Stroke {
        tool: PaintTool::Brush,
        plane: ActivePlane::Color,
        color: [40, 80, 120, 255],
        diameter: 256.0,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: true,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![StrokeSample {
            x: 32.0,
            y: 32.0,
            pressure: 0.0,
        }],
    };
    core.begin_stroke(&stroke).unwrap();

    let high_pressure = vec![
        StrokeSample {
            x: 32.0,
            y: 32.0,
            pressure: 1.0,
        };
        260
    ];
    assert!(matches!(
        core.append_stroke(&high_pressure),
        Err(CoreError::InvalidArgument(
            "stroke rasterization work exceeds the bounded limit"
        ))
    ));
    assert!(!core.stroke_is_active());
    assert_eq!(core.document_info().unwrap(), created);
    assert_eq!(core.layers().unwrap(), stable_tree);
    assert_eq!(core.build_snapshot().tile_count(), 0);

    let outcome = core
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: created.document_revision,
            color: PixelValue::Rgba([9, 8, 7, 255]),
        })
        .unwrap();
    let procedure = outcome.procedure().unwrap();
    assert_eq!(procedure.procedure_id().get(), 1);
    assert_eq!(procedure.base_state_id().get(), 1);
    assert_eq!(procedure.committed_state_id().get(), 2);
}

#[test]
fn many_single_sample_appends_do_not_rebuild_the_complete_preview() {
    const SAMPLE_COUNT: usize = 16_384;
    let sample = StrokeSample {
        x: 31.25,
        y: 32.75,
        pressure: 1.0,
    };
    let stroke = line_stroke(vec![sample; SAMPLE_COUNT]);

    let mut one_shot = Core::new();
    one_shot
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    one_shot.apply_stroke(&stroke).unwrap();

    let mut incremental = Core::new();
    incremental
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let mut first = stroke.clone();
    first.samples.truncate(1);
    incremental.begin_stroke(&first).unwrap();
    for sample in &stroke.samples[1..] {
        incremental
            .append_stroke(std::slice::from_ref(sample))
            .unwrap();
    }
    incremental.end_stroke().unwrap();

    assert_eq!(
        incremental.document_state_digest().unwrap(),
        one_shot.document_state_digest().unwrap()
    );
    assert_eq!(incremental.history_entries(), one_shot.history_entries());
}
