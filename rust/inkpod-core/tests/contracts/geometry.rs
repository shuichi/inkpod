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

fn vector_core() -> (Core, u64, u64) {
    let mut core = Core::new();
    core.new_cell(32, 32, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let (_, layer_id) = core
        .create_layer(LayerKind::VectorColoring, "Geometry")
        .unwrap();
    let (main, _, fill) = core.vector_layer_planes(layer_id).unwrap();
    (core, main, fill)
}

fn raster_core() -> (Core, u64) {
    let mut core = Core::new();
    core.new_cell(32, 32, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let plane = raster_color_plane(&core);
    (core, plane)
}

fn primitive_fixture(primitive: GeometryPrimitive) -> (Vec<PointF32>, bool, usize) {
    match primitive {
        GeometryPrimitive::Line => (vec![point(4.0, 5.0), point(20.0, 11.0)], false, 1),
        GeometryPrimitive::Curve => (
            vec![point(4.0, 5.0), point(20.0, 11.0), point(12.0, 20.0)],
            false,
            1,
        ),
        GeometryPrimitive::Rectangle => (vec![point(4.0, 5.0), point(20.0, 18.0)], true, 4),
        GeometryPrimitive::Ellipse => (vec![point(4.0, 5.0), point(20.0, 18.0)], true, 4),
        GeometryPrimitive::Polygon => (vec![point(12.0, 12.0), point(20.0, 12.0)], true, 5),
        GeometryPrimitive::Polyline => (
            vec![
                point(4.0, 4.0),
                point(20.0, 4.0),
                point(20.0, 20.0),
                point(4.0, 20.0),
            ],
            true,
            4,
        ),
    }
}

#[test]
fn paint_002_raster_vector_capability_table_and_goldens_cover_every_primitive() {
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
        let (points, closed, segment_count) = primitive_fixture(primitive);
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

        let (mut vector, vector_plane, _) = vector_core();
        let commit = vector
            .apply_geometry(&request(vector_plane, primitive, points.clone(), style))
            .unwrap();
        let path = vector
            .vector_paths()
            .unwrap()
            .into_iter()
            .find(|path| path.id == commit.path_id)
            .unwrap();
        assert_eq!(path.segments.len(), segment_count, "{primitive:?}");
        assert_eq!(path.closed, closed, "{primitive:?}");
        assert_eq!(commit.fill_id != 0, closed, "{primitive:?}");

        if closed {
            let mut fill_only = options(false, true);
            fill_only.close_path = primitive == GeometryPrimitive::Polyline;
            let (mut raster, raster_plane) = raster_core();
            raster
                .apply_geometry(&request(raster_plane, primitive, points.clone(), fill_only))
                .unwrap();
            let (mut vector, vector_plane, _) = vector_core();
            let fill_commit = vector
                .apply_geometry(&request(vector_plane, primitive, points, fill_only))
                .unwrap();
            assert_ne!(fill_commit.path_id, 0);
            assert_ne!(fill_commit.fill_id, 0);
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
            let (mut vector, vector_plane, _) = vector_core();
            assert!(matches!(
                vector.apply_geometry(&request(
                    vector_plane,
                    primitive,
                    points,
                    options(false, true),
                )),
                Err(CoreError::InvalidArgument(_))
            ));
        }
    }

    assert_eq!(
        raster_digests,
        vec![
            [
                225, 216, 143, 4, 249, 97, 51, 26, 82, 227, 71, 3, 19, 211, 20, 80, 71, 200, 141,
                42, 202, 75, 208, 236, 245, 58, 15, 26, 35, 181, 69, 114
            ],
            [
                119, 26, 229, 216, 229, 250, 227, 21, 18, 68, 32, 106, 120, 241, 226, 196, 252,
                249, 217, 109, 46, 11, 213, 107, 23, 226, 130, 155, 91, 238, 37, 70
            ],
            [
                186, 220, 186, 25, 37, 112, 75, 253, 152, 18, 159, 167, 132, 87, 108, 97, 240, 51,
                254, 81, 122, 57, 188, 174, 43, 104, 86, 202, 66, 109, 164, 221
            ],
            [
                181, 66, 208, 147, 178, 132, 221, 19, 25, 106, 194, 172, 165, 96, 142, 88, 41, 233,
                95, 123, 25, 10, 244, 14, 190, 147, 192, 30, 212, 213, 132, 195
            ],
            [
                158, 30, 49, 118, 120, 58, 216, 182, 175, 40, 11, 87, 17, 175, 122, 115, 101, 183,
                78, 183, 253, 235, 118, 173, 45, 100, 104, 185, 72, 247, 47, 193
            ],
            [
                117, 170, 62, 123, 159, 52, 145, 106, 248, 75, 90, 182, 254, 48, 135, 19, 117, 243,
                144, 143, 85, 123, 255, 114, 79, 179, 82, 67, 232, 117, 252, 92
            ],
        ]
    );
}

#[test]
fn paint_002_vector_rectangle_commits_outline_and_fill_as_one_procedure() {
    let (mut core, main_plane, fill_plane) = vector_core();
    let before = core.document_info().unwrap();
    let committed = core
        .apply_geometry(&request(
            main_plane,
            GeometryPrimitive::Rectangle,
            vec![point(4.0, 5.0), point(20.0, 18.0)],
            options(true, true),
        ))
        .unwrap();

    assert_eq!(committed.dispatch.revision(), before.document_revision + 1);
    assert_ne!(committed.path_id, 0);
    assert_ne!(committed.fill_id, 0);
    let path = core.vector_paths().unwrap().pop().unwrap();
    assert_eq!(path.id, committed.path_id);
    assert_eq!(path.plane_id, main_plane);
    assert!(path.closed);
    assert_eq!(path.segments.len(), 4);
    let fill = core.vector_fills().unwrap().pop().unwrap();
    assert_eq!(fill.id, committed.fill_id);
    assert_eq!(fill.plane_id, fill_plane);
    assert_eq!(fill.boundary_path_ids, vec![path.id]);
    core.undo().unwrap();
    assert!(core.vector_paths().unwrap().is_empty());
    assert!(core.vector_fills().unwrap().is_empty());
    core.redo().unwrap();
    assert_eq!(core.vector_paths().unwrap().len(), 1);
    assert_eq!(core.vector_fills().unwrap().len(), 1);
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
fn paint_002_curve_polyline_constraints_and_current_only_save_reopen_are_stable() {
    let (mut core, main_plane, _) = vector_core();
    let mut curve = request(
        main_plane,
        GeometryPrimitive::Curve,
        vec![point(2.0, 3.0), point(22.0, 3.0), point(12.0, 18.0)],
        options(true, false),
    );
    curve.options.taper_start = true;
    curve.options.taper_end = true;
    curve.options.cross_section = GeometryCrossSection::Square;
    let curve_id = core.apply_geometry(&curve).unwrap().path_id;
    let curve_info = core
        .vector_paths()
        .unwrap()
        .into_iter()
        .find(|path| path.id == curve_id)
        .unwrap();
    assert_eq!(curve_info.segments.len(), 2);
    assert!(curve_info.square_cross_section);
    assert!(curve_info.segments[0].width_start < curve_info.segments[0].width_end.max(0.01));

    let mut polyline = request(
        main_plane,
        GeometryPrimitive::Polyline,
        vec![
            point(3.0, 24.0),
            point(8.0, 16.0),
            point(16.0, 24.0),
            point(24.0, 16.0),
        ],
        options(true, true),
    );
    polyline.options.close_path = true;
    polyline.options.bezier_segments = true;
    core.apply_geometry(&polyline).unwrap();

    let path = std::env::temp_dir().join(format!(
        "inkpod-geometry-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    core.save(&path).unwrap();
    let expected_paths = core.vector_paths().unwrap();
    let expected_fills = core.vector_fills().unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.vector_paths().unwrap(), expected_paths);
    assert_eq!(reopened.vector_fills().unwrap(), expected_fills);
    fs::remove_file(path).unwrap();
}

#[test]
fn paint_002_constraints_rotation_cross_section_and_side_bounds_are_canonical() {
    let (mut core, main_plane, _) = vector_core();
    let mut line = request(
        main_plane,
        GeometryPrimitive::Line,
        vec![point(2.0, 2.0), point(10.0, 7.0)],
        options(true, false),
    );
    line.options.constrain_45_degrees = true;
    let line_id = core.apply_geometry(&line).unwrap().path_id;
    let line_path = core
        .vector_paths()
        .unwrap()
        .into_iter()
        .find(|path| path.id == line_id)
        .unwrap();
    assert_eq!(line_path.segments[0].p0, point(2.0, 2.0));
    assert_eq!(line_path.segments[0].p3, point(10.0, 10.0));

    let mut rectangle = request(
        main_plane,
        GeometryPrimitive::Rectangle,
        vec![point(16.0, 16.0), point(22.0, 19.0)],
        options(true, false),
    );
    rectangle.options.from_center = true;
    rectangle.options.aspect_ratio_q16 = 1 << 16;
    rectangle.options.rotation_turns = 1 << 30;
    rectangle.options.cross_section = GeometryCrossSection::Square;
    let rectangle_id = core.apply_geometry(&rectangle).unwrap().path_id;
    let rectangle_path = core
        .vector_paths()
        .unwrap()
        .into_iter()
        .find(|path| path.id == rectangle_id)
        .unwrap();
    assert_eq!(rectangle_path.segments[0].p0, point(22.0, 10.0));
    assert_eq!(rectangle_path.segments[0].p3, point(22.0, 22.0));
    assert!(rectangle_path.square_cross_section);

    for side_count in [3, 64] {
        let mut polygon = request(
            main_plane,
            GeometryPrimitive::Polygon,
            vec![point(16.0, 16.0), point(24.0, 16.0)],
            options(true, false),
        );
        polygon.options.polygon_sides = side_count;
        let id = core.apply_geometry(&polygon).unwrap().path_id;
        assert_eq!(
            core.vector_paths()
                .unwrap()
                .into_iter()
                .find(|path| path.id == id)
                .unwrap()
                .segments
                .len(),
            usize::from(side_count)
        );
    }
    for side_count in [2, 65] {
        let mut polygon = request(
            main_plane,
            GeometryPrimitive::Polygon,
            vec![point(16.0, 16.0), point(24.0, 16.0)],
            options(true, false),
        );
        polygon.options.polygon_sides = side_count;
        assert!(matches!(
            core.apply_geometry(&polygon),
            Err(CoreError::InvalidArgument(_))
        ));
    }
}

#[test]
fn paint_002_invalid_stale_no_content_and_point_bounds_are_atomic() {
    let (mut core, main_plane, _) = vector_core();
    let before = core.document_info().unwrap();
    let before_paths = core.vector_paths().unwrap();

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
    assert_eq!(outcome.path_id, 0);

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
    assert_eq!(core.vector_paths().unwrap(), before_paths);

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
