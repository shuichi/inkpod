use super::*;

#[test]
fn empty_snapshot_remains_stable() {
    let mut core = Core::new();
    let first = core.build_snapshot();
    let second = core.build_snapshot();
    assert_eq!(first, second);
    assert_eq!(first.revision(), 0);
    assert_eq!(first.tile_count(), 0);
}

#[test]
fn acceptance_saved_drawing_vertical_slice() {
    let mut core = Core::new();
    let created = core
        .new_cell(1920, 1080, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    assert!(!created.dirty);

    let samples: Vec<_> = (0..128)
        .map(|index| StrokeSample {
            x: 100.0 + index as f32,
            y: 100.0 + (index / 4) as f32,
            pressure: 0.5,
        })
        .collect();
    core.apply_stroke(&line_stroke(samples)).unwrap();
    let after_line = core.document_info().unwrap();
    assert!(after_line.dirty);
    let line_checksum = after_line.main_plane_checksum;
    assert_ne!(line_checksum, created.main_plane_checksum);

    core.set_active_plane(ActivePlane::Color).unwrap();
    let mut color_stroke = line_stroke(vec![
        StrokeSample {
            x: 120.0,
            y: 140.0,
            pressure: 1.0,
        },
        StrokeSample {
            x: 220.0,
            y: 160.0,
            pressure: 1.0,
        },
    ]);
    color_stroke.plane = ActivePlane::Color;
    color_stroke.color = [220, 40, 30, 255];
    core.apply_stroke(&color_stroke).unwrap();
    let after_color = core.document_info().unwrap();
    assert_eq!(after_color.main_plane_checksum, line_checksum);
    assert_ne!(
        after_color.color_plane_checksum,
        created.color_plane_checksum
    );

    let colored_pixel = core.plane_pixel(ActivePlane::Color, 150, 146).unwrap();
    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 150, 146).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    core.redo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 150, 146).unwrap(),
        colored_pixel
    );

    let revision_before_view = core.document_info().unwrap().document_revision;
    core.apply_view(ViewCommand::PanBy {
        device_dx: 10.0,
        device_dy: -5.0,
    })
    .unwrap();
    core.apply_view(ViewCommand::ZoomAt {
        factor: 2.0,
        device_x: 320.0,
        device_y: 240.0,
    })
    .unwrap();
    let after_view = core.document_info().unwrap();
    assert_eq!(after_view.document_revision, revision_before_view);
    assert!(after_view.view_revision > after_color.view_revision);

    let path = std::env::temp_dir().join(format!(
        "inkpod-core-test-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let saved = core.save(&path).unwrap();
    assert!(
        !saved.dirty,
        "v11 normal save persists both document and editor savepoints"
    );
    let expected_snapshot = core.build_snapshot();
    drop(core);

    let mut reopened_core = Core::new();
    let reopened = reopened_core.open(&path).unwrap();
    assert_eq!(reopened.document_id, saved.document_id);
    assert_eq!(reopened.document_uuid, saved.document_uuid);
    assert_eq!(reopened.layer_id, saved.layer_id);
    assert_eq!(reopened.main_plane_id, saved.main_plane_id);
    assert_eq!(reopened.color_plane_id, saved.color_plane_id);
    assert_eq!(reopened.frames, saved.frames);
    assert_eq!(reopened.main_plane_checksum, saved.main_plane_checksum);
    assert_eq!(reopened.color_plane_checksum, saved.color_plane_checksum);
    assert_eq!(
        reopened_core.build_snapshot().tiles().len(),
        expected_snapshot.tiles().len()
    );
    assert!(!reopened.dirty);
    fs::remove_file(path).unwrap();
}

#[test]
fn fill_is_one_atomic_history_unit_and_never_changes_main_line() {
    let mut core = Core::new();
    core.new_cell(9, 9, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    for samples in [
        vec![
            StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            },
            StrokeSample {
                x: 7.0,
                y: 1.0,
                pressure: 1.0,
            },
        ],
        vec![
            StrokeSample {
                x: 7.0,
                y: 1.0,
                pressure: 1.0,
            },
            StrokeSample {
                x: 7.0,
                y: 7.0,
                pressure: 1.0,
            },
        ],
        vec![
            StrokeSample {
                x: 7.0,
                y: 7.0,
                pressure: 1.0,
            },
            StrokeSample {
                x: 1.0,
                y: 7.0,
                pressure: 1.0,
            },
        ],
        vec![
            StrokeSample {
                x: 1.0,
                y: 7.0,
                pressure: 1.0,
            },
            StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            },
        ],
    ] {
        core.apply_stroke(&line_stroke(samples)).unwrap();
    }
    let before = core.document_info().unwrap();
    let fill = PixelValue::Rgba([20, 90, 180, 255]);
    let outcome = core
        .apply_fill(&fill_request(4, 4, [20, 90, 180, 255]))
        .unwrap();
    let after = core.document_info().unwrap();
    assert_eq!(outcome.changed_pixels, 25);
    assert_eq!(after.document_revision, before.document_revision + 1);
    assert_eq!(after.main_plane_checksum, before.main_plane_checksum);
    assert_eq!(core.plane_pixel(ActivePlane::Color, 4, 4).unwrap(), fill);

    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 4, 4).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    core.redo().unwrap();
    assert_eq!(core.plane_pixel(ActivePlane::Color, 4, 4).unwrap(), fill);
    assert!(core.journal_state().unwrap().is_complete());
    core.verify_journal_replay().unwrap();
}

#[test]
fn fill_001_persistent_selection_mask_clips_seed_fill() {
    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_selection(
        &SelectionShape::Ellipse(RectI32 {
            x: 2,
            y: 2,
            width: 4,
            height: 4,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    let mut request = fill_request(4, 4, [20, 80, 200, 255]);
    request.use_document_selection = true;
    core.apply_fill(&request).unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 4, 4).unwrap(),
        PixelValue::Rgba([20, 80, 200, 255])
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([0; 4])
    );
}

#[test]
fn overflow_invalid_cancel_and_noop_do_not_commit_partial_fill() {
    let mut core = Core::new();
    let created = core
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let mut request = fill_request(4, 4, [1, 2, 3, 255]);
    request.overflow_abort = true;
    assert!(matches!(
        core.apply_fill(&request),
        Err(CoreError::FillOverflow { .. })
    ));
    assert_eq!(core.document_info().unwrap(), created);

    request.overflow_abort = false;
    assert!(matches!(
        core.apply_fill_with_cancel(&request, || true),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(core.document_info().unwrap(), created);

    request.selection = Some(RectI32 {
        x: 2,
        y: 2,
        width: 2,
        height: 2,
    });
    request.seed_x = 2;
    request.seed_y = 2;
    let first = core.apply_fill(&request).unwrap();
    assert_eq!(first.changed_pixels, 4);
    let before_noop = core.document_info().unwrap();
    let second = core.apply_fill(&request).unwrap();
    assert_eq!(second.changed_pixels, 0);
    assert_eq!(core.document_info().unwrap(), before_noop);
}

#[test]
fn autosave_recovery_never_inherits_or_overwrites_normal_path() {
    let suffix = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir();
    let normal = directory.join(format!(
        "inkpod-test-normal-{}-{suffix}.inkpod",
        std::process::id()
    ));
    let recovery = directory.join(format!(
        "inkpod-test-recovery-{}-{suffix}.inkpod",
        std::process::id()
    ));
    let restored = directory.join(format!(
        "inkpod-test-restored-{}-{suffix}.inkpod",
        std::process::id()
    ));

    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.save(&normal).unwrap();
    let normal_bytes = fs::read(&normal).unwrap();
    let mut request = fill_request(3, 3, [9, 8, 7, 255]);
    request.selection = Some(RectI32 {
        x: 2,
        y: 2,
        width: 2,
        height: 2,
    });
    core.apply_fill(&request).unwrap();
    let before_autosave = core.document_info().unwrap();
    let after_autosave = core.autosave(&recovery).unwrap();
    assert_eq!(after_autosave, before_autosave);
    assert!(after_autosave.dirty);
    assert_eq!(fs::read(&normal).unwrap(), normal_bytes);

    let mut recovered = Core::new();
    let recovered_info = recovered.open_recovery(&recovery).unwrap();
    assert!(recovered_info.recovered);
    assert!(recovered_info.dirty);
    assert!(matches!(
        recovered.revert(),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(fs::read(&normal).unwrap(), normal_bytes);
    recovered.save(&restored).unwrap();
    assert_eq!(fs::read(&normal).unwrap(), normal_bytes);
    assert_ne!(fs::read(&restored).unwrap(), normal_bytes);

    fs::remove_file(normal).unwrap();
    fs::remove_file(recovery).unwrap();
    fs::remove_file(restored).unwrap();
}

#[test]
fn fill_rejects_oversized_documents_before_materializing_selection() {
    let mut core = Core::new();
    core.new_cell(
        MAX_RASTER_DIMENSION,
        MAX_RASTER_DIMENSION,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
    )
    .unwrap();
    let mut request = fill_request(0, 0, [1, 2, 3, 255]);
    request.selection = Some(RectI32 {
        x: 0,
        y: 0,
        width: i32::try_from(MAX_RASTER_DIMENSION).unwrap(),
        height: i32::try_from(MAX_RASTER_DIMENSION).unwrap(),
    });
    assert!(matches!(
        core.apply_fill(&request),
        Err(CoreError::InvalidArgument(
            "fill document exceeds the bounded work limit"
        ))
    ));
}

#[test]
fn paint_001_brush_eraser_auto_erase_and_pressure_are_transactional() {
    let mut core = Core::new();
    core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();

    let center = StrokeSample {
        x: 20.0,
        y: 20.0,
        pressure: 1.0,
    };
    let mut brush = color_stroke(PaintTool::Brush, 9.0, center);
    brush.pressure_size = true;
    core.apply_stroke(&brush).unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 24, 20).unwrap(),
        PixelValue::Rgba([12, 34, 56, 255])
    );

    let eraser = color_stroke(PaintTool::Eraser, 9.0, center);
    core.apply_stroke(&eraser).unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 24, 20).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 24, 20).unwrap(),
        PixelValue::Rgba([12, 34, 56, 255])
    );

    core.undo().unwrap();
    brush.samples[0].pressure = 0.0;
    core.apply_stroke(&brush).unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 21, 20).unwrap(),
        PixelValue::Rgba([0; 4])
    );

    let point = StrokeSample {
        x: 5.0,
        y: 6.0,
        pressure: 1.0,
    };
    core.apply_stroke(&line_stroke(vec![point])).unwrap();
    let mut auto_erase = line_stroke(vec![point]);
    auto_erase.auto_erase = true;
    core.apply_stroke(&auto_erase).unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 5, 6).unwrap(),
        PixelValue::Binary(0)
    );
    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 5, 6).unwrap(),
        PixelValue::Binary(255)
    );
}

#[test]
fn paint_001_magnified_device_click_matches_the_locator_pixel_cell() {
    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_view(ViewCommand::ZoomAt {
        factor: 64.0,
        device_x: 0.0,
        device_y: 0.0,
    })
    .unwrap();

    let erase_at_device = |core: &mut Core, x: f32, y: f32| {
        let locator = core
            .locator_sample(None, f64::from(x), f64::from(y))
            .unwrap();
        let mut stroke = line_stroke(vec![StrokeSample {
            x,
            y,
            pressure: 1.0,
        }]);
        stroke.auto_erase = true;
        stroke.coordinate_space = CoordinateSpace::Device;
        core.apply_stroke(&stroke).unwrap();
        locator
    };

    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 3.0,
        y: 3.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let locator = erase_at_device(&mut core, 3.75 * 64.0, 3.75 * 64.0);
    assert_eq!((locator.document_x, locator.document_y), (3, 3));
    assert_eq!(locator.color, Some(PixelValue::Rgba([0, 0, 0, 255])));
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 3, 3).unwrap(),
        PixelValue::Binary(0)
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 4, 4).unwrap(),
        PixelValue::Binary(0)
    );

    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 7.0,
        y: 7.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let locator = erase_at_device(&mut core, 7.75 * 64.0, 7.75 * 64.0);
    assert_eq!((locator.document_x, locator.document_y), (7, 7));
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 7, 7).unwrap(),
        PixelValue::Binary(0)
    );

    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 2.0,
        y: 5.0,
        pressure: 1.0,
    }]))
    .unwrap();
    core.apply_view(ViewCommand::Flip {
        axis: MirrorAxis::Horizontal,
    })
    .unwrap();
    core.apply_view(ViewCommand::Flip {
        axis: MirrorAxis::Vertical,
    })
    .unwrap();
    let locator = erase_at_device(&mut core, (8.0 - 2.75) * 64.0, (8.0 - 5.75) * 64.0);
    assert_eq!((locator.document_x, locator.document_y), (2, 5));
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 2, 5).unwrap(),
        PixelValue::Binary(0)
    );
    assert_eq!(core.build_snapshot().tile_count(), 0);
}

#[test]
fn view_003_locator_neighborhood_is_bounded_clipped_and_read_only() {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 0.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let before = core.document_info().unwrap();
    let neighborhood = core.locator_neighborhood(None, 0.25, 0.25, 1).unwrap();
    assert_eq!((neighborhood.origin_x, neighborhood.origin_y), (-1, -1));
    assert_eq!((neighborhood.width, neighborhood.height), (3, 3));
    assert_eq!(neighborhood.pixels_rgba8.len(), 36);
    assert_eq!(&neighborhood.pixels_rgba8[0..4], &[0, 0, 0, 0]);
    assert_eq!(&neighborhood.pixels_rgba8[16..20], &[0, 0, 0, 255]);
    assert!(core.locator_neighborhood(None, 0.0, 0.0, 17).is_err());
    assert_eq!(core.document_info().unwrap(), before);
}

#[test]
fn abi_002_snapshot_composites_visible_main_line_over_color() {
    let mut core = Core::new();
    core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 10.0,
        y: 10.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let main_checksum = core.document_info().unwrap().main_plane_checksum;

    let mut color = color_stroke(
        PaintTool::Pencil,
        1.0,
        StrokeSample {
            x: 10.0,
            y: 10.0,
            pressure: 1.0,
        },
    );
    color.samples.push(StrokeSample {
        x: 20.0,
        y: 10.0,
        pressure: 1.0,
    });
    core.apply_stroke(&color).unwrap();
    assert_eq!(
        core.document_info().unwrap().main_plane_checksum,
        main_checksum
    );

    let snapshot = core.build_snapshot();
    assert_eq!(snapshot.tile_count(), 1);
    let tile = &snapshot.tiles()[0];
    let pixel = |x: usize, y: usize| {
        let offset = y * tile.stride_bytes() as usize + x * 4;
        &tile.pixels()[offset..offset + 4]
    };
    assert_eq!(pixel(10, 10), [0, 0, 0, 255]);
    assert_eq!(pixel(20, 10), [56, 34, 12, 255]);
}

#[test]
fn invalid_view_and_excessive_stroke_work_do_not_commit_partial_state() {
    let mut core = Core::new();
    let created = core
        .new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    assert!(matches!(
        core.apply_view(ViewCommand::PanBy {
            device_dx: f64::MAX,
            device_dy: 0.0,
        }),
        Err(CoreError::InvalidArgument(_))
    ));
    let after_view = core.document_info().unwrap();
    assert_eq!(after_view.document_revision, created.document_revision);
    assert_eq!(after_view.view_revision, created.view_revision);

    let extreme = line_stroke(vec![StrokeSample {
        x: f32::MAX,
        y: 0.0,
        pressure: 1.0,
    }]);
    assert!(matches!(
        core.apply_stroke(&extreme),
        Err(CoreError::InvalidArgument(_))
    ));

    let mut excessive = color_stroke(
        PaintTool::Brush,
        256.0,
        StrokeSample {
            x: 32.0,
            y: 32.0,
            pressure: 1.0,
        },
    );
    excessive.samples = vec![excessive.samples[0]; 300];
    assert!(matches!(
        core.apply_stroke(&excessive),
        Err(CoreError::InvalidArgument(_))
    ));
    let after_strokes = core.document_info().unwrap();
    assert_eq!(after_strokes.document_revision, created.document_revision);
    assert_eq!(
        after_strokes.color_plane_checksum,
        created.color_plane_checksum
    );
    assert!(!after_strokes.can_undo);
}

#[test]
fn off_canvas_segment_is_clipped_before_rasterization_work_is_counted() {
    let mut core = Core::new();
    core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![
        StrokeSample {
            x: -10_000_000.0,
            y: 32.0,
            pressure: 1.0,
        },
        StrokeSample {
            x: 10_000_000.0,
            y: 32.0,
            pressure: 1.0,
        },
    ]))
    .unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 0, 32).unwrap(),
        PixelValue::Binary(255)
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 63, 32).unwrap(),
        PixelValue::Binary(255)
    );
}
