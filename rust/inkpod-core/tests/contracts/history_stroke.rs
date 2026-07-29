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
