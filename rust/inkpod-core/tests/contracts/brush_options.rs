use super::*;

fn brush(
    color: [u8; 4],
    shape: BrushShape,
    smoothing: u16,
    start_color: StartColorPredicate,
    samples: Vec<StrokeSample>,
) -> Stroke {
    Stroke {
        tool: PaintTool::Brush,
        plane: ActivePlane::Color,
        color,
        diameter: 5.0,
        shape,
        smoothing,
        start_color,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples,
    }
}

fn sample(x: f32, y: f32) -> StrokeSample {
    StrokeSample {
        x,
        y,
        pressure: 1.0,
    }
}

#[test]
fn paint_004_round_and_square_brushes_have_distinct_exact_footprints() {
    let mut round = Core::new();
    round
        .new_cell(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    round
        .apply_stroke(&brush(
            [10, 20, 30, 255],
            BrushShape::Round,
            0,
            StartColorPredicate::Any,
            vec![sample(8.0, 8.0)],
        ))
        .unwrap();

    let mut square = Core::new();
    square
        .new_cell(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    square
        .apply_stroke(&brush(
            [10, 20, 30, 255],
            BrushShape::Square,
            0,
            StartColorPredicate::Any,
            vec![sample(8.0, 8.0)],
        ))
        .unwrap();

    assert_eq!(
        round.plane_pixel(ActivePlane::Color, 10, 10).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    assert_eq!(
        square.plane_pixel(ActivePlane::Color, 10, 10).unwrap(),
        PixelValue::Rgba([10, 20, 30, 255])
    );

    let mut pressure = Core::new();
    pressure
        .new_cell(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let mut pressure_stroke = brush(
        [10, 20, 30, 255],
        BrushShape::Square,
        0,
        StartColorPredicate::Any,
        vec![StrokeSample {
            x: 8.0,
            y: 8.0,
            pressure: 0.25,
        }],
    );
    pressure_stroke.pressure_size = true;
    pressure.apply_stroke(&pressure_stroke).unwrap();
    assert_eq!(
        pressure.plane_pixel(ActivePlane::Color, 8, 8).unwrap(),
        PixelValue::Rgba([10, 20, 30, 255])
    );
    assert_eq!(
        pressure.plane_pixel(ActivePlane::Color, 10, 10).unwrap(),
        PixelValue::Rgba([0; 4])
    );
}

#[test]
fn paint_004_brush_options_share_the_existing_selection_clip() {
    let mut core = Core::new();
    core.new_cell(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 8,
            y: 8,
            width: 1,
            height: 1,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    core.apply_stroke(&brush(
        [90, 80, 70, 255],
        BrushShape::Square,
        500,
        StartColorPredicate::ExactNative,
        vec![sample(8.0, 8.0)],
    ))
    .unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 8, 8).unwrap(),
        PixelValue::Rgba([90, 80, 70, 255])
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 9, 8).unwrap(),
        PixelValue::Rgba([0; 4])
    );
}

#[test]
fn paint_004_start_color_is_exact_alpha_aware_nonconnected_and_immutable() {
    let mut core = Core::new();
    core.new_cell(16, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();

    for (x, color) in [
        (2.0, [30, 60, 90, 100]),
        (3.0, [30, 60, 90, 101]),
        (4.0, [30, 60, 90, 100]),
        (5.0, [200, 10, 10, 255]),
        (6.0, [30, 60, 90, 100]),
    ] {
        let mut seed = brush(
            color,
            BrushShape::Square,
            0,
            StartColorPredicate::Any,
            vec![sample(x, 3.0)],
        );
        seed.diameter = 1.0;
        core.apply_stroke(&seed).unwrap();
    }
    let before = core.document_info().unwrap();
    let mut restricted = brush(
        [5, 240, 20, 255],
        BrushShape::Square,
        0,
        StartColorPredicate::ExactNative,
        vec![sample(2.0, 3.0), sample(6.0, 3.0)],
    );
    restricted.diameter = 1.0;
    core.apply_stroke(&restricted).unwrap();

    for x in [2, 4, 6] {
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, x, 3).unwrap(),
            PixelValue::Rgba([5, 240, 20, 255])
        );
    }
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 3, 3).unwrap(),
        PixelValue::Rgba([30, 60, 90, 101])
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 5, 3).unwrap(),
        PixelValue::Rgba([200, 10, 10, 255])
    );
    assert_eq!(
        core.document_info().unwrap().document_revision,
        before.document_revision + 1
    );

    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 6, 3).unwrap(),
        PixelValue::Rgba([30, 60, 90, 100])
    );
    core.redo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 6, 3).unwrap(),
        PixelValue::Rgba([5, 240, 20, 255])
    );
}

#[test]
fn paint_004_start_color_compares_rgba16_native_channels_without_quantization() {
    let plan = plan_cell_creation(&CellCreationOptions {
        sizing: CellSizing::ImagePixels {
            width: 8,
            height: 8,
        },
        dpi_x_milli: DEFAULT_DPI_MILLI,
        dpi_y_milli: DEFAULT_DPI_MILLI,
        margin_milli: 50,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: FrameAnchor::Center,
        initial_layer_kind: LayerKind::Raster,
        pixel_format: PixelFormat::StraightRgba16,
        count: 1,
    })
    .unwrap();
    let mut core = Core::new();
    core.new_cell_from_creation_plan(plan.item(0).unwrap(), 0x5041_494e_5404)
        .unwrap();
    let layers = core.layers().unwrap();
    let layer = layers
        .iter()
        .find(|layer| {
            layer
                .planes
                .iter()
                .any(|plane| plane.kind == PlaneType::Color)
        })
        .unwrap();
    let plane = layer
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneType::Color)
        .unwrap();
    let target = EditorTarget {
        layer_id: layer.id,
        plane_id: plane.id,
    };
    let mut state = core.editor_state().unwrap();
    for update in [
        EditorStateUpdate::SetActiveTarget(target),
        EditorStateUpdate::SetActiveTool(EditorTool::Brush),
        EditorStateUpdate::SetToolDiameter {
            tool: EditorTool::Brush,
            diameter_q16: 1_i64 << 16,
        },
        EditorStateUpdate::SetBrushOptions(EditorBrushOptions::default()),
    ] {
        state = core.update_editor_state(state.revision, update).unwrap();
    }

    for (x, color) in [
        (2.0, PixelValue::Rgba16([0x1201, 0x3402, 0x5603, 0x7804])),
        (3.0, PixelValue::Rgba16([0x1200, 0x3402, 0x5603, 0x7804])),
        (4.0, PixelValue::Rgba16([0x1201, 0x3402, 0x5603, 0x7804])),
        (5.0, PixelValue::Rgba16([0x1201, 0x3402, 0x5603, 0x7805])),
        (6.0, PixelValue::Rgba16([0x1201, 0x3402, 0x5603, 0x7804])),
    ] {
        let _ = core
            .update_editor_state(
                state.revision,
                EditorStateUpdate::SetToolColor {
                    tool: EditorTool::Brush,
                    color,
                },
            )
            .unwrap();
        core.begin_editor_stroke(&EditorStrokeInput {
            tool: None,
            coordinate_space: CoordinateSpace::Document,
            auto_erase: false,
            pressure_size: false,
            samples: vec![sample(x, 3.0)],
        })
        .unwrap();
        core.end_stroke().unwrap();
        state = core.editor_state().unwrap();
    }

    let replacement = PixelValue::Rgba16([1, 2, 3, 65_535]);
    state = core
        .update_editor_state(
            state.revision,
            EditorStateUpdate::SetToolColor {
                tool: EditorTool::Brush,
                color: replacement,
            },
        )
        .unwrap();
    core.update_editor_state(
        state.revision,
        EditorStateUpdate::SetBrushOptions(EditorBrushOptions {
            shape: BrushShape::Square,
            smoothing: 0,
            start_color: StartColorPredicate::ExactNative,
        }),
    )
    .unwrap();
    core.begin_editor_stroke(&EditorStrokeInput {
        tool: None,
        coordinate_space: CoordinateSpace::Document,
        auto_erase: false,
        pressure_size: false,
        samples: vec![sample(2.0, 3.0), sample(6.0, 3.0)],
    })
    .unwrap();
    core.end_stroke().unwrap();

    for x in [2, 4, 6] {
        assert_eq!(
            core.plane_pixel(ActivePlane::Color, x, 3).unwrap(),
            replacement
        );
    }
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 3, 3).unwrap(),
        PixelValue::Rgba16([0x1200, 0x3402, 0x5603, 0x7804])
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 5, 3).unwrap(),
        PixelValue::Rgba16([0x1201, 0x3402, 0x5603, 0x7805])
    );
}

#[test]
fn paint_004_smoothing_is_batching_independent_and_cancel_is_atomic() {
    let samples = (0..24)
        .map(|index| sample(4.0 + index as f32, 8.0 + ((index % 3) as f32 - 1.0) * 3.0))
        .collect::<Vec<_>>();
    let stroke = brush(
        [80, 120, 200, 255],
        BrushShape::Round,
        700,
        StartColorPredicate::Any,
        samples.clone(),
    );

    let mut one_shot = Core::new();
    one_shot
        .new_cell(40, 20, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    one_shot.apply_stroke(&stroke).unwrap();

    let mut unsmoothed = Core::new();
    unsmoothed
        .new_cell(40, 20, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let mut off = stroke.clone();
    off.smoothing = 0;
    unsmoothed.apply_stroke(&off).unwrap();
    assert_ne!(
        unsmoothed.document_state_digest().unwrap(),
        one_shot.document_state_digest().unwrap()
    );

    let mut incremental = Core::new();
    let initial = incremental
        .new_cell(40, 20, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let mut first = stroke.clone();
    first.samples = samples[..2].to_vec();
    incremental.begin_stroke(&first).unwrap();
    for batch in samples[2..].chunks(5) {
        incremental.append_stroke(batch).unwrap();
    }
    incremental.end_stroke().unwrap();
    assert_eq!(
        incremental.document_state_digest().unwrap(),
        one_shot.document_state_digest().unwrap()
    );

    incremental.begin_stroke(&stroke).unwrap();
    incremental.cancel_stroke();
    assert_eq!(
        incremental.document_state_digest().unwrap(),
        one_shot.document_state_digest().unwrap()
    );
    assert_eq!(
        incremental.document_info().unwrap().document_revision,
        initial.document_revision + 1
    );
}

#[test]
fn paint_004_rejects_non_brush_options_and_out_of_bounds_start_without_state_change() {
    let mut core = Core::new();
    let initial = core
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let no_op = brush(
        [0; 4],
        BrushShape::Square,
        0,
        StartColorPredicate::ExactNative,
        vec![sample(2.0, 2.0)],
    );
    let no_op_outcome = core.apply_stroke(&no_op).unwrap();
    assert_eq!(no_op_outcome.revision(), initial.document_revision);
    assert_eq!(core.document_info().unwrap(), initial);

    let mut invalid = brush(
        [1, 2, 3, 255],
        BrushShape::Round,
        1_001,
        StartColorPredicate::Any,
        vec![sample(2.0, 2.0)],
    );
    assert!(matches!(
        core.apply_stroke(&invalid),
        Err(CoreError::InvalidArgument(_))
    ));

    invalid.smoothing = 0;
    invalid.start_color = StartColorPredicate::ExactNative;
    invalid.samples = vec![sample(-1.0, 2.0)];
    assert!(matches!(
        core.apply_stroke(&invalid),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), initial);
    assert!(core.history_entries().is_empty());
}
