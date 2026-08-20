use super::*;

fn point(x: f32, y: f32) -> PointF32 {
    PointF32 { x, y }
}

fn options(outline: bool, fill: bool) -> GeometryOptions {
    GeometryOptions {
        outline,
        fill,
        close_path: false,
        bezier_segments: false,
        constrain_45_degrees: false,
        from_center: false,
        taper_start: false,
        taper_end: false,
        cross_section: GeometryCrossSection::Round,
        aspect_ratio_q16: 0,
        polygon_sides: 5,
        rotation_turns: 0,
    }
}

fn request(
    plane_id: u64,
    primitive: GeometryPrimitive,
    points: Vec<PointF32>,
    options: GeometryOptions,
) -> GeometryRequest {
    GeometryRequest {
        plane_id,
        primitive,
        points,
        outline_color: PixelValue::Rgba([20, 40, 80, 255]),
        fill_color: PixelValue::Rgba([180, 60, 30, 255]),
        outline_width: 2.0,
        options,
    }
}

fn raster_color_plane(core: &Core) -> u64 {
    core.layers()
        .unwrap()
        .into_iter()
        .flat_map(|layer| layer.planes)
        .find(|plane| plane.kind == PlaneType::Color)
        .unwrap()
        .id
}

fn raster_core() -> (Core, u64) {
    let mut core = Core::new();
    core.new_cell(32, 32, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let plane = raster_color_plane(&core);
    (core, plane)
}

fn geometry_sample(x: f32, y: f32) -> StrokeSample {
    StrokeSample {
        x,
        y,
        pressure: 1.0,
    }
}

fn device_sample(
    view: ViewState,
    document_width: u32,
    document_height: u32,
    x: f64,
    y: f64,
) -> StrokeSample {
    let logical_x = if view.flip_horizontal() {
        f64::from(document_width) - x
    } else {
        x
    };
    let logical_y = if view.flip_vertical() {
        f64::from(document_height) - y
    } else {
        y
    };
    geometry_sample(
        logical_x.mul_add(view.zoom(), view.pan_x()) as f32,
        logical_y.mul_add(view.zoom(), view.pan_y()) as f32,
    )
}

#[test]
fn snap_001_geometry_input_contract_covers_precedence_ties_bypass_and_bounds() {
    let mut core = Core::new();
    core.new_cell(32, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_grid(GridConfig {
        origin_x: 0,
        origin_y: 0,
        spacing_x: 8,
        spacing_y: 8,
        subdivisions: 2,
    })
    .unwrap();
    core.add_guide(GuideAxis::Vertical, 5).unwrap();
    core.add_guide(GuideAxis::Horizontal, 13).unwrap();
    let before_document = core.document_info().unwrap();
    let before_history = core.history_entries().len();
    let before_journal = core.journal_state();

    let raw = core
        .resolve_geometry_points_for_view(
            0,
            0,
            CoordinateSpace::Document,
            &[geometry_sample(5.2, 7.8)],
            GeometrySnapMode::UseViewState,
        )
        .unwrap();
    assert_eq!(raw.points, vec![point(5.2, 7.8)]);

    core.apply_view(ViewCommand::SetGridSnapEnabled(true))
        .unwrap();
    let grid = core
        .resolve_geometry_points_for_view(
            0,
            0,
            CoordinateSpace::Document,
            &[geometry_sample(5.2, 7.8), geometry_sample(2.0, 2.0)],
            GeometrySnapMode::UseViewState,
        )
        .unwrap();
    assert_eq!(grid.points, vec![point(4.0, 8.0), point(4.0, 4.0)]);

    core.apply_view(ViewCommand::SetGuideSnapEnabled(true))
        .unwrap();
    let both = core
        .resolve_geometry_points_for_view(
            0,
            0,
            CoordinateSpace::Document,
            &[
                geometry_sample(5.2, 9.0),
                geometry_sample(9.0, 9.0),
                geometry_sample(9.001, 9.0),
            ],
            GeometrySnapMode::UseViewState,
        )
        .unwrap();
    assert_eq!(
        both.points,
        vec![point(5.0, 13.0), point(5.0, 13.0), point(8.0, 13.0)]
    );
    let bypass = core
        .resolve_geometry_points_for_view(
            0,
            both.view_revision,
            CoordinateSpace::Document,
            &[geometry_sample(5.2, 7.8), geometry_sample(-4.0, 40.0)],
            GeometrySnapMode::Bypass,
        )
        .unwrap();
    assert_eq!(bypass.points, vec![point(5.2, 7.8), point(0.0, 24.0)]);
    let far_edge = core
        .resolve_geometry_points_for_view(
            0,
            both.view_revision,
            CoordinateSpace::Document,
            &[geometry_sample(31.999, 23.999)],
            GeometrySnapMode::UseViewState,
        )
        .unwrap();
    assert_eq!(far_edge.points, vec![point(32.0, 24.0)]);

    let mut guide_tie = Core::new();
    guide_tie
        .new_cell(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    guide_tie.add_guide(GuideAxis::Vertical, 4).unwrap();
    guide_tie.add_guide(GuideAxis::Vertical, 6).unwrap();
    guide_tie
        .apply_view(ViewCommand::SetGuideSnapEnabled(true))
        .unwrap();
    assert_eq!(
        guide_tie
            .resolve_geometry_points_for_view(
                0,
                0,
                CoordinateSpace::Document,
                &[geometry_sample(5.0, 8.0)],
                GeometrySnapMode::UseViewState,
            )
            .unwrap()
            .points,
        vec![point(6.0, 8.0)]
    );

    let after_document = core.document_info().unwrap();
    assert_eq!(
        after_document.document_revision,
        before_document.document_revision
    );
    assert_eq!(after_document.dirty, before_document.dirty);
    assert_eq!(core.history_entries().len(), before_history);
    assert_eq!(core.journal_state(), before_journal);
}

#[test]
fn paint_002_geometry_input_is_view_targeted_dpi_independent_and_stale_safe() {
    let configured = |dpi| {
        let mut core = Core::new();
        core.new_cell(32, 24, dpi, dpi).unwrap();
        core.set_grid(GridConfig {
            origin_x: 0,
            origin_y: 0,
            spacing_x: 8,
            spacing_y: 8,
            subdivisions: 2,
        })
        .unwrap();
        core.add_guide(GuideAxis::Vertical, 5).unwrap();
        core.apply_view(ViewCommand::SetSnapEnabled(true)).unwrap();
        core.apply_view(ViewCommand::OneToOne {
            viewport_width: 32.0,
            viewport_height: 24.0,
        })
        .unwrap();
        core.apply_view(ViewCommand::PanBy {
            device_dx: 3.0,
            device_dy: -2.0,
        })
        .unwrap();
        core
    };
    let mut standard = configured(96_000);
    let high_dpi = configured(300_000);
    let standard_view = standard.view_state();
    let high_dpi_view = high_dpi.view_state();
    let standard_sample = device_sample(standard_view, 32, 24, 5.2, 7.8);
    let high_dpi_sample = device_sample(high_dpi_view, 32, 24, 5.2, 7.8);
    let resolved = standard
        .resolve_geometry_points_for_view(
            0,
            0,
            CoordinateSpace::Device,
            &[standard_sample],
            GeometrySnapMode::UseViewState,
        )
        .unwrap();
    let high_dpi_resolved = high_dpi
        .resolve_geometry_points_for_view(
            0,
            0,
            CoordinateSpace::Device,
            &[high_dpi_sample],
            GeometrySnapMode::UseViewState,
        )
        .unwrap();
    assert_eq!(resolved.points, vec![point(5.0, 8.0)]);
    assert_eq!(high_dpi_resolved.points, resolved.points);

    standard
        .apply_view(ViewCommand::PanBy {
            device_dx: 1.0,
            device_dy: 0.0,
        })
        .unwrap();
    assert!(matches!(
        standard.resolve_geometry_points_for_view(
            0,
            resolved.view_revision,
            CoordinateSpace::Device,
            &[standard_sample],
            GeometrySnapMode::UseViewState,
        ),
        Err(CoreError::InvalidState(_))
    ));

    let secondary = standard.create_view().unwrap();
    standard
        .apply_view_for(
            secondary,
            ViewCommand::Flip {
                axis: MirrorAxis::Horizontal,
            },
        )
        .unwrap();
    standard
        .apply_view_for(
            secondary,
            ViewCommand::Flip {
                axis: MirrorAxis::Vertical,
            },
        )
        .unwrap();
    standard
        .apply_view_for(
            secondary,
            ViewCommand::ZoomAt {
                factor: f64::MAX,
                device_x: 0.0,
                device_y: 0.0,
            },
        )
        .unwrap();
    let secondary_view = standard
        .apply_view_for(
            secondary,
            ViewCommand::PanBy {
                device_dx: 1_000_000.0,
                device_dy: -500_000.0,
            },
        )
        .unwrap();
    let secondary_result = standard
        .resolve_geometry_points_for_view(
            secondary,
            secondary_view.revision(),
            CoordinateSpace::Device,
            &[device_sample(secondary_view, 32, 24, 5.2, 7.8)],
            GeometrySnapMode::UseViewState,
        )
        .unwrap();
    assert_eq!(secondary_result.points, vec![point(5.0, 8.0)]);
    standard.close_view(secondary).unwrap();
    assert!(matches!(
        standard.resolve_geometry_points_for_view(
            secondary,
            secondary_result.view_revision,
            CoordinateSpace::Device,
            &[geometry_sample(0.0, 0.0)],
            GeometrySnapMode::UseViewState,
        ),
        Err(CoreError::InvalidArgument(_))
    ));

    let (mut raster, plane_id) = raster_core();
    raster
        .set_grid(GridConfig {
            origin_x: 0,
            origin_y: 0,
            spacing_x: 8,
            spacing_y: 8,
            subdivisions: 2,
        })
        .unwrap();
    raster
        .apply_view(ViewCommand::SetGridSnapEnabled(true))
        .unwrap();
    let geometry_points = raster
        .resolve_geometry_points_for_view(
            0,
            0,
            CoordinateSpace::Document,
            &[geometry_sample(3.8, 4.2), geometry_sample(19.7, 12.1)],
            GeometrySnapMode::UseViewState,
        )
        .unwrap()
        .points;
    let before = raster.document_info().unwrap();
    raster
        .apply_geometry(&request(
            plane_id,
            GeometryPrimitive::Line,
            geometry_points,
            options(true, false),
        ))
        .unwrap();
    let committed = raster
        .build_snapshot()
        .canonical_composite_digest()
        .unwrap();
    assert_eq!(
        raster.document_info().unwrap().document_revision,
        before.document_revision + 1
    );
    raster.undo().unwrap();
    assert_eq!(
        raster.document_info().unwrap().document_revision,
        before.document_revision + 2
    );
    raster.redo().unwrap();
    assert_eq!(
        raster
            .build_snapshot()
            .canonical_composite_digest()
            .unwrap(),
        committed
    );
}

fn primitive_fixture(primitive: GeometryPrimitive) -> (Vec<PointF32>, bool) {
    match primitive {
        GeometryPrimitive::Line => (vec![point(4.0, 5.0), point(20.0, 11.0)], false),
        GeometryPrimitive::Curve => (
            vec![point(4.0, 5.0), point(20.0, 11.0), point(12.0, 20.0)],
            false,
        ),
        GeometryPrimitive::Rectangle => (vec![point(4.0, 5.0), point(20.0, 18.0)], true),
        GeometryPrimitive::Ellipse => (vec![point(4.0, 5.0), point(20.0, 18.0)], true),
        GeometryPrimitive::Polygon => (vec![point(12.0, 12.0), point(20.0, 12.0)], true),
        GeometryPrimitive::Polyline => (
            vec![
                point(4.0, 4.0),
                point(20.0, 4.0),
                point(20.0, 20.0),
                point(4.0, 20.0),
            ],
            true,
        ),
    }
}

#[test]
fn paint_002_raster_goldens_cover_every_primitive() {
    let primitives = [
        GeometryPrimitive::Line,
        GeometryPrimitive::Curve,
        GeometryPrimitive::Rectangle,
        GeometryPrimitive::Ellipse,
        GeometryPrimitive::Polygon,
        GeometryPrimitive::Polyline,
    ];
    let mut raster_digests = Vec::new();
    for primitive in primitives {
        let (points, closed) = primitive_fixture(primitive);
        let mut style = options(true, closed);
        style.close_path = primitive == GeometryPrimitive::Polyline;

        let (mut raster, raster_plane) = raster_core();
        raster
            .apply_geometry(&request(raster_plane, primitive, points.clone(), style))
            .unwrap();
        raster_digests.push(
            raster
                .build_snapshot()
                .canonical_composite_digest()
                .unwrap()
                .as_bytes(),
        );

        if closed {
            let mut fill_only = options(false, true);
            fill_only.close_path = primitive == GeometryPrimitive::Polyline;
            let (mut raster, raster_plane) = raster_core();
            raster
                .apply_geometry(&request(raster_plane, primitive, points.clone(), fill_only))
                .unwrap();
        } else {
            let fill_only = request(
                raster_plane,
                primitive,
                points.clone(),
                options(false, true),
            );
            assert!(matches!(
                raster.apply_geometry(&fill_only),
                Err(CoreError::InvalidArgument(_))
            ));
        }
    }

    assert_eq!(
        raster_digests,
        vec![
            [
                61, 244, 206, 74, 71, 70, 209, 153, 202, 14, 14, 66, 0, 174, 178, 159, 224, 124,
                13, 198, 28, 114, 114, 117, 68, 106, 94, 195, 100, 42, 158, 31
            ],
            [
                100, 6, 64, 89, 42, 93, 0, 61, 81, 206, 147, 15, 175, 198, 115, 198, 183, 130, 54,
                158, 167, 254, 40, 134, 165, 9, 38, 245, 150, 206, 194, 44
            ],
            [
                220, 78, 13, 215, 62, 182, 103, 192, 210, 32, 202, 191, 206, 157, 231, 131, 213,
                146, 80, 224, 16, 148, 171, 126, 10, 7, 59, 31, 168, 74, 169, 158
            ],
            [
                75, 28, 222, 10, 220, 31, 219, 158, 135, 170, 216, 123, 109, 56, 183, 229, 164, 39,
                130, 149, 205, 238, 114, 164, 68, 54, 61, 162, 168, 228, 53, 182
            ],
            [
                102, 245, 15, 162, 122, 154, 218, 202, 238, 136, 222, 208, 49, 188, 124, 42, 254,
                19, 128, 161, 145, 130, 133, 52, 166, 194, 108, 252, 110, 93, 73, 209
            ],
            [
                23, 121, 24, 187, 18, 37, 109, 224, 237, 139, 163, 10, 235, 220, 127, 243, 10, 120,
                236, 186, 191, 173, 70, 69, 250, 151, 243, 251, 154, 94, 49, 91
            ],
        ]
    );
}

#[test]
fn paint_002_raster_preview_is_non_destructive_cancelable_and_commits_once() {
    let mut core = Core::new();
    core.new_cell(24, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let plane_id = raster_color_plane(&core);
    let before = core.document_info().unwrap();
    let before_snapshot = core.build_snapshot().canonical_composite_digest();
    let first = request(
        plane_id,
        GeometryPrimitive::Polygon,
        vec![point(12.0, 12.0), point(18.0, 12.0)],
        options(true, true),
    );
    let preview = core
        .begin_geometry_preview(before.document_revision, &first)
        .unwrap();
    assert_eq!(preview.base_revision, before.document_revision);
    assert!(preview.preview_revision >= 1_u64 << 63);
    assert_eq!(core.document_info().unwrap(), before);
    assert_ne!(
        core.build_snapshot().canonical_composite_digest(),
        before_snapshot
    );

    let mut updated = first.clone();
    updated.points[1] = point(20.0, 12.0);
    assert!(
        core.update_geometry_preview(before.document_revision, &updated)
            .unwrap()
            .preview_revision
            > preview.preview_revision
    );
    assert_eq!(core.document_info().unwrap(), before);
    core.cancel_geometry_preview().unwrap();
    assert_eq!(
        core.build_snapshot().canonical_composite_digest(),
        before_snapshot
    );

    core.begin_geometry_preview(before.document_revision, &updated)
        .unwrap();
    let committed = core.commit_geometry_preview().unwrap();
    assert_eq!(committed.dispatch.revision(), before.document_revision + 1);
    assert!(core.document_info().unwrap().dirty);
    core.undo().unwrap();
    assert_eq!(
        core.build_snapshot().canonical_composite_digest(),
        before_snapshot
    );
    core.redo().unwrap();
    assert_ne!(
        core.build_snapshot().canonical_composite_digest(),
        before_snapshot
    );
}

#[test]
fn paint_002_raster_target_matrix_and_selection_clip_are_exact() {
    let mut core = Core::new();
    core.new_cell(24, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let document = core.document_info().unwrap();
    core.apply_geometry(&request(
        document.main_plane_id,
        GeometryPrimitive::Line,
        vec![point(2.0, 2.0), point(20.0, 2.0)],
        options(true, false),
    ))
    .unwrap();
    assert_ne!(
        core.plane_pixel(ActivePlane::MainLine, 8, 2).unwrap(),
        PixelValue::Binary(0)
    );
    assert!(matches!(
        core.apply_geometry(&request(
            document.main_plane_id,
            GeometryPrimitive::Rectangle,
            vec![point(2.0, 2.0), point(20.0, 20.0)],
            options(false, true),
        )),
        Err(CoreError::InvalidArgument(
            "raster main-line geometry cannot be filled"
        ))
    ));

    core.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 0,
            y: 0,
            width: 8,
            height: 24,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    let color_plane = raster_color_plane(&core);
    core.apply_geometry(&request(
        color_plane,
        GeometryPrimitive::Rectangle,
        vec![point(2.0, 2.0), point(20.0, 20.0)],
        options(false, true),
    ))
    .unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 4, 4).unwrap(),
        PixelValue::Rgba([180, 60, 30, 255])
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 12, 4).unwrap(),
        PixelValue::Rgba([0, 0, 0, 0])
    );
}

#[test]
fn paint_002_invalid_stale_no_content_and_point_bounds_are_atomic() {
    let (mut core, main_plane) = raster_core();
    let before = core.document_info().unwrap();

    let stale = request(
        main_plane,
        GeometryPrimitive::Line,
        vec![point(1.0, 1.0), point(8.0, 1.0)],
        options(true, false),
    );
    assert_eq!(
        core.begin_geometry_preview(before.document_revision - 1, &stale),
        Err(CoreError::InvalidState(
            "geometry preview base revision is stale"
        ))
    );

    let no_content = request(
        main_plane,
        GeometryPrimitive::Line,
        vec![point(3.0, 3.0), point(3.0, 3.0)],
        options(true, false),
    );
    let outcome = core.apply_geometry(&no_content).unwrap();
    assert_eq!(outcome.dispatch.revision(), before.document_revision);

    let mut invalid = stale.clone();
    invalid.points[1].x = f32::NAN;
    assert!(matches!(
        core.apply_geometry(&invalid),
        Err(CoreError::InvalidArgument(_))
    ));
    let mut too_many = stale;
    too_many.points = vec![point(1.0, 1.0); MAX_GEOMETRY_POINTS + 1];
    assert!(matches!(
        core.apply_geometry(&too_many),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), before);

    core.begin_geometry_preview(before.document_revision, &no_content)
        .unwrap();
    assert!(matches!(
        core.update_geometry_preview(before.document_revision + 1, &no_content),
        Err(CoreError::InvalidState(_))
    ));
    core.cancel_geometry_preview().unwrap();

    let mut closed_taper = request(
        main_plane,
        GeometryPrimitive::Rectangle,
        vec![point(2.0, 2.0), point(9.0, 9.0)],
        options(true, false),
    );
    closed_taper.options.taper_start = true;
    assert!(matches!(
        core.apply_geometry(&closed_taper),
        Err(CoreError::InvalidArgument(_))
    ));
    let mut extreme = no_content;
    extreme.points[1] = point(f32::MAX, f32::MAX);
    assert!(matches!(
        core.apply_geometry(&extreme),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), before);
}
