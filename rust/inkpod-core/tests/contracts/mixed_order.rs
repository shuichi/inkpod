use super::*;

fn rectangle_path(x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 4]) -> VectorPathInput {
    let points = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
    let segments = points
        .windows(2)
        .map(|pair| {
            let third_x = (pair[1].0 - pair[0].0) / 3.0;
            let third_y = (pair[1].1 - pair[0].1) / 3.0;
            VectorCubicSegment {
                p0: PointF32 {
                    x: pair[0].0,
                    y: pair[0].1,
                },
                p1: PointF32 {
                    x: pair[0].0 + third_x,
                    y: pair[0].1 + third_y,
                },
                p2: PointF32 {
                    x: pair[0].0 + third_x * 2.0,
                    y: pair[0].1 + third_y * 2.0,
                },
                p3: PointF32 {
                    x: pair[1].0,
                    y: pair[1].1,
                },
                width_start: 1.0,
                width_end: 1.0,
            }
        })
        .collect();
    VectorPathInput {
        segments,
        color: PixelValue::Rgba(color),
        closed: true,
    }
}

fn fill_active_plane(core: &mut Core, color: [u8; 4]) {
    core.apply_fill(&FillRequest {
        operation: FillOperation::Seed,
        seed_x: 1,
        seed_y: 1,
        color: PixelValue::Rgba(color),
        selection: None,
        use_document_selection: false,
        tolerance: 0,
        detached_regions: false,
        overflow_abort: false,
        gap_close: 0,
        transparent_only: false,
        inclusion_mode: InclusionMode::None,
        inclusion_colors: Vec::new(),
        extension_distance: 0,
    })
    .unwrap();
}

fn exported_pixel(core: &Core, x: u32, y: u32) -> [u8; 4] {
    let encoded = core
        .export_common_raster(inkpod_format::CommonRasterFormat::Png, false)
        .unwrap();
    let raster =
        inkpod_format::decode_common_raster(inkpod_format::CommonRasterFormat::Png, &encoded)
            .unwrap();
    let offset = (y as usize * raster.info.width as usize + x as usize) * 4;
    raster.pixels[offset..offset + 4].try_into().unwrap()
}

fn mixed_order_core() -> (Core, u64, u64, u64, u64) {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let top = core.layers().unwrap()[0].clone();
    let top_plane = top
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneType::Color)
        .unwrap()
        .id;
    core.set_active_node(top.id, top_plane).unwrap();
    fill_active_plane(&mut core, [0, 255, 0, 128]);

    let (_, vector_layer) = core
        .create_layer(LayerKind::VectorColoring, "Middle Vector")
        .unwrap();
    let (vector_main, vector_trace, vector_fill) = core.vector_layer_planes(vector_layer).unwrap();
    let (_, boundary) = core
        .vector_add_path(
            vector_trace,
            rectangle_path(0.0, 0.0, 4.0, 4.0, [255, 0, 0, 0]),
        )
        .unwrap();
    core.vector_add_fill(vector_fill, &[boundary], PixelValue::Rgba([255, 0, 0, 128]))
        .unwrap();
    core.set_plane_properties(vector_main, false, true, 1_000, "Main Line")
        .unwrap();
    core.set_plane_properties(vector_trace, false, true, 1_000, "Color Trace")
        .unwrap();

    let (_, bottom) = core
        .create_layer(LayerKind::Raster, "Bottom Raster")
        .unwrap();
    let bottom_plane = core.layers().unwrap()[2].planes[0].id;
    core.set_active_node(bottom, bottom_plane).unwrap();
    fill_active_plane(&mut core, [0, 0, 255, 255]);
    (core, top.id, vector_layer, bottom, bottom_plane)
}

fn layer_begin_order(snapshot: &RenderSnapshot) -> Vec<u64> {
    snapshot
        .render_passes()
        .iter()
        .filter(|pass| pass.kind() == RenderPassKind::LayerBegin)
        .map(RenderPass::layer_id)
        .collect()
}

#[test]
fn pm_gap_007_mixed_layer_order_is_shared_by_snapshot_export_history_and_reopen() {
    let (mut core, top, vector, bottom, _) = mixed_order_core();
    let original_snapshot = core.build_snapshot();
    assert_eq!(
        layer_begin_order(&original_snapshot),
        vec![bottom, vector, top]
    );
    let original_digest = original_snapshot.canonical_composite_digest().unwrap();
    let original_pixel = exported_pixel(&core, 1, 1);
    assert_eq!(original_pixel, [64, 128, 63, 255]);

    let before = core.document_info().unwrap();
    let moved = core.reorder_layer(bottom, 0).unwrap();
    assert_eq!(moved.revision(), before.document_revision + 1);
    let moved_snapshot = core.build_snapshot();
    assert_eq!(
        layer_begin_order(&moved_snapshot),
        vec![vector, top, bottom]
    );
    assert_ne!(
        moved_snapshot.canonical_composite_digest().unwrap(),
        original_digest
    );
    let moved_pixel = exported_pixel(&core, 1, 1);
    assert_eq!(moved_pixel, [0, 0, 255, 255]);

    let moved_revision = core.document_info().unwrap().document_revision;
    assert_eq!(
        core.reorder_layer(bottom, 0).unwrap().revision(),
        moved_revision
    );
    assert!(matches!(
        core.reorder_layer(bottom, usize::MAX),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(
        core.document_info().unwrap().document_revision,
        moved_revision
    );
    assert_eq!(exported_pixel(&core, 1, 1), moved_pixel);

    core.undo().unwrap();
    assert_eq!(exported_pixel(&core, 1, 1), original_pixel);
    assert_eq!(
        core.build_snapshot().canonical_composite_digest().unwrap(),
        original_digest
    );
    core.redo().unwrap();
    assert_eq!(exported_pixel(&core, 1, 1), moved_pixel);

    let path = std::env::temp_dir().join(format!(
        "inkpod-pm-gap-007-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(exported_pixel(&reopened, 1, 1), moved_pixel);
    assert_eq!(
        reopened
            .build_snapshot()
            .canonical_composite_digest()
            .unwrap(),
        core.build_snapshot().canonical_composite_digest().unwrap()
    );
}

#[test]
fn pm_gap_007_plane_zero_is_top_and_mixed_plane_passes_follow_tree_order() {
    let (mut core, _, vector_layer, _, _) = mixed_order_core();
    let (_, raster_plane) = core
        .create_plane(
            vector_layer,
            PlaneType::Raster,
            PixelFormat::StraightRgba8,
            "Vector Layer Raster",
        )
        .unwrap();
    core.set_active_node(vector_layer, raster_plane).unwrap();
    fill_active_plane(&mut core, [255, 255, 0, 96]);

    let layer = core
        .layers()
        .unwrap()
        .into_iter()
        .find(|layer| layer.id == vector_layer)
        .unwrap();
    let snapshot = core.build_snapshot();
    let pass_planes: Vec<_> = snapshot
        .render_passes()
        .iter()
        .filter(|pass| {
            pass.layer_id() == vector_layer
                && matches!(
                    pass.kind(),
                    RenderPassKind::RasterTiles
                        | RenderPassKind::VectorFills
                        | RenderPassKind::VectorStrokes
                )
                && pass.item_count() != 0
        })
        .map(RenderPass::plane_id)
        .collect();
    let expected: Vec<_> = layer
        .planes
        .iter()
        .rev()
        .filter(|plane| plane.visible)
        .map(|plane| plane.id)
        .filter(|plane_id| pass_planes.contains(plane_id))
        .collect();
    assert_eq!(pass_planes, expected);

    let revision = core.document_info().unwrap().document_revision;
    let raster_index = layer
        .planes
        .iter()
        .position(|plane| plane.id == raster_plane)
        .unwrap();
    assert_eq!(raster_index, layer.planes.len() - 1);
    core.reorder_plane(raster_plane, 0).unwrap();
    assert_eq!(
        core.document_info().unwrap().document_revision,
        revision + 1
    );
    let reordered = core.build_snapshot();
    let reordered_planes: Vec<_> = reordered
        .render_passes()
        .iter()
        .filter(|pass| {
            pass.layer_id() == vector_layer
                && matches!(
                    pass.kind(),
                    RenderPassKind::RasterTiles
                        | RenderPassKind::VectorFills
                        | RenderPassKind::VectorStrokes
                )
                && pass.item_count() != 0
        })
        .map(RenderPass::plane_id)
        .collect();
    assert_eq!(reordered_planes.last(), Some(&raster_plane));
    assert_ne!(
        reordered.canonical_composite_digest().unwrap(),
        snapshot.canonical_composite_digest().unwrap()
    );
}

#[test]
fn pm_gap_007_group_opacity_adjustment_position_and_thumbnail_share_order() {
    let (mut core, _, vector_layer, _, _) = mixed_order_core();
    core.set_layer_properties(vector_layer, true, true, 500, "Middle Vector")
        .unwrap();
    let snapshot = core.build_snapshot();
    let vector_begin = snapshot
        .render_passes()
        .iter()
        .find(|pass| pass.kind() == RenderPassKind::LayerBegin && pass.layer_id() == vector_layer)
        .unwrap();
    assert_eq!(vector_begin.opacity_milli(), 500);
    let thumbnail = core.layer_thumbnail(vector_layer, 4, 4).unwrap();
    assert_eq!(&thumbnail.pixels[20..24], &[255, 0, 0, 64]);

    core.set_layer_properties(vector_layer, true, true, 0, "Middle Vector")
        .unwrap();
    assert_eq!(
        &core.layer_thumbnail(vector_layer, 4, 4).unwrap().pixels[20..24],
        &[0, 0, 0, 0]
    );
    assert_eq!(exported_pixel(&core, 1, 1), [0, 128, 127, 255]);
    core.set_layer_properties(vector_layer, true, true, 1_000, "Middle Vector")
        .unwrap();
    assert_eq!(
        &core.layer_thumbnail(vector_layer, 4, 4).unwrap().pixels[20..24],
        &[255, 0, 0, 128]
    );
    core.set_layer_properties(vector_layer, false, true, 500, "Middle Vector")
        .unwrap();
    assert_eq!(exported_pixel(&core, 1, 1), [0, 128, 127, 255]);
    core.set_layer_properties(vector_layer, true, true, 500, "Middle Vector")
        .unwrap();

    let before_adjustment = exported_pixel(&core, 1, 1);
    let (_, adjustment) = core
        .create_adjustment_layer(
            "Ordered Brightness",
            Adjustment::BrightnessContrast {
                brightness_milli: 100,
                contrast_milli: 0,
            },
        )
        .unwrap();
    let top_adjustment = core.build_snapshot();
    let adjustment_pass = top_adjustment
        .render_passes()
        .iter()
        .position(|pass| pass.kind() == RenderPassKind::Adjustment && pass.layer_id() == adjustment)
        .unwrap();
    let vector_pass = top_adjustment
        .render_passes()
        .iter()
        .position(|pass| {
            pass.kind() == RenderPassKind::LayerBegin && pass.layer_id() == vector_layer
        })
        .unwrap();
    assert!(adjustment_pass > vector_pass);
    let adjusted_on_top = exported_pixel(&core, 1, 1);
    assert_ne!(adjusted_on_top, before_adjustment);

    core.reorder_layer(adjustment, 2).unwrap();
    let reordered = core.build_snapshot();
    let adjustment_pass = reordered
        .render_passes()
        .iter()
        .position(|pass| pass.kind() == RenderPassKind::Adjustment && pass.layer_id() == adjustment)
        .unwrap();
    let vector_pass = reordered
        .render_passes()
        .iter()
        .position(|pass| {
            pass.kind() == RenderPassKind::LayerBegin && pass.layer_id() == vector_layer
        })
        .unwrap();
    assert!(adjustment_pass < vector_pass);
    assert_ne!(exported_pixel(&core, 1, 1), adjusted_on_top);
}
