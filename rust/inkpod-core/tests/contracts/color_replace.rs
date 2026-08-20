use super::*;
use std::collections::BTreeMap;

fn tile_revisions(core: &mut Core) -> BTreeMap<(i32, i32), u64> {
    core.build_snapshot()
        .tiles()
        .iter()
        .map(|tile| ((tile.origin_x(), tile.origin_y()), tile.tile_revision()))
        .collect()
}

fn color_plane(core: &Core) -> u64 {
    core.layers()
        .unwrap()
        .into_iter()
        .flat_map(|layer| layer.planes)
        .find(|plane| plane.kind == PlaneType::Color)
        .unwrap()
        .id
}

fn filled_raster_core(
    width: u32,
    height: u32,
    format: PixelFormat,
    color: PixelValue,
) -> (Core, u64) {
    let mut core = Core::new();
    let options = CellCreationOptions {
        sizing: CellSizing::ImagePixels { width, height },
        dpi_x_milli: DEFAULT_DPI_MILLI,
        dpi_y_milli: DEFAULT_DPI_MILLI,
        margin_milli: 0,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: FrameAnchor::Center,
        initial_layer_kind: LayerKind::BinaryColoring,
        pixel_format: format,
        count: 1,
    };
    let plan = plan_cell_creation(&options).unwrap();
    core.new_cell_from_creation_plan(plan.item(0).unwrap(), 0x434f_4c4f_5252_4550)
        .unwrap();
    let plane_id = color_plane(&core);
    core.apply_fill(&FillRequest {
        color,
        ..fill_request(0, 0, [0, 0, 0, 0])
    })
    .unwrap();
    (core, plane_id)
}

fn request(
    core: &Core,
    plane_id: u64,
    mode: ScopedColorReplaceMode,
    target: PixelValue,
    replacement: PixelValue,
    region: Option<SelectionShape>,
) -> ScopedColorReplaceRequest {
    ScopedColorReplaceRequest {
        base_document_revision: core.document_info().unwrap().document_revision,
        plane_id,
        mode,
        target,
        replacement,
        region,
    }
}

#[test]
fn color_replace_001_raster_regions_selection_tile_boundary_and_cancel_are_exact() {
    let old = PixelValue::Rgba([11, 22, 33, 99]);
    let new = PixelValue::Rgba([44, 55, 66, 77]);
    let (mut core, plane_id) = filled_raster_core(70, 6, PixelFormat::StraightRgba8, old);
    core.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 64,
            y: 1,
            width: 1,
            height: 3,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    let scoped = request(
        &core,
        plane_id,
        ScopedColorReplaceMode::RasterColor,
        old,
        new,
        Some(SelectionShape::Rectangle(RectI32 {
            x: 63,
            y: 0,
            width: 3,
            height: 5,
        })),
    );
    let before = core.document_info().unwrap();
    let history = core.history_entries().len();
    let before_tiles = tile_revisions(&mut core);
    assert_eq!(before_tiles.len(), 2);
    let preview = core.preview_scoped_color_replace(&scoped).unwrap();
    assert_eq!(preview.base_document_revision, before.document_revision);
    assert_eq!(preview.matched_pixels, 3);
    assert_eq!(
        preview.affected_bounds,
        Some(RectI32 {
            x: 64,
            y: 1,
            width: 1,
            height: 3
        })
    );
    assert_eq!(
        core.document_info().unwrap(),
        before,
        "preview/Cancel is read-only"
    );
    assert_eq!(core.history_entries().len(), history);

    let outcome = core.apply_scoped_color_replace(scoped).unwrap();
    assert_eq!(outcome.revision(), before.document_revision + 1);
    let after_tiles = tile_revisions(&mut core);
    assert_eq!(after_tiles[&(0, 0)], before_tiles[&(0, 0)]);
    assert_ne!(after_tiles[&(64, 0)], before_tiles[&(64, 0)]);
    for y in 0..6 {
        for x in 0..70 {
            let expected = if x == 64 && (1..4).contains(&y) {
                new
            } else {
                old
            };
            assert_eq!(
                core.plane_pixel(ActivePlane::Color, x, y).unwrap(),
                expected
            );
        }
    }
    core.undo().unwrap();
    assert_eq!(core.plane_pixel(ActivePlane::Color, 64, 2).unwrap(), old);
    core.redo().unwrap();
    assert_eq!(core.plane_pixel(ActivePlane::Color, 64, 2).unwrap(), new);
}

#[test]
fn color_replace_001_four_regions_and_native_rgba16_alpha_are_public_contracts() {
    let old = PixelValue::Rgba16([1_000, 2_000, 3_000, 4_000]);
    let different_alpha = PixelValue::Rgba16([1_000, 2_000, 3_000, 4_001]);
    let new = PixelValue::Rgba16([50_000, 40_000, 30_000, 20_000]);
    let regions = [
        SelectionShape::Trace {
            points: vec![PointF32 { x: 2.5, y: 2.5 }],
            diameter: 1.0,
        },
        SelectionShape::Rectangle(RectI32 {
            x: 2,
            y: 2,
            width: 1,
            height: 1,
        }),
        SelectionShape::Polyline(vec![
            PointF32 { x: 2.0, y: 2.0 },
            PointF32 { x: 3.0, y: 2.0 },
            PointF32 { x: 2.5, y: 3.0 },
        ]),
        SelectionShape::Lasso(vec![
            PointF32 { x: 2.0, y: 2.0 },
            PointF32 { x: 3.0, y: 2.0 },
            PointF32 { x: 2.5, y: 3.0 },
        ]),
    ];
    for region in regions {
        let (mut core, plane_id) = filled_raster_core(6, 6, PixelFormat::StraightRgba16, old);
        let no_match = request(
            &core,
            plane_id,
            ScopedColorReplaceMode::RasterColor,
            different_alpha,
            new,
            Some(region.clone()),
        );
        let revision = core.document_info().unwrap().document_revision;
        assert_eq!(
            core.apply_scoped_color_replace(no_match)
                .unwrap()
                .revision(),
            revision,
            "alpha is part of native-depth exact matching"
        );
        let exact = request(
            &core,
            plane_id,
            ScopedColorReplaceMode::RasterColor,
            old,
            new,
            Some(region),
        );
        assert!(
            core.preview_scoped_color_replace(&exact)
                .unwrap()
                .matched_pixels
                > 0
        );
        core.apply_scoped_color_replace(exact).unwrap();
        assert_eq!(core.plane_pixel(ActivePlane::Color, 2, 2).unwrap(), new);
        assert_eq!(core.plane_pixel(ActivePlane::Color, 5, 5).unwrap(), old);
    }
}

#[test]
fn color_replace_001_stale_invalid_hidden_locked_save_reopen_and_replay_are_atomic() {
    let old = PixelValue::Rgba([1, 2, 3, 4]);
    let new = PixelValue::Rgba([5, 6, 7, 8]);
    let (mut core, plane_id) = filled_raster_core(4, 4, PixelFormat::StraightRgba8, old);
    let stale = request(
        &core,
        plane_id,
        ScopedColorReplaceMode::RasterColor,
        old,
        new,
        None,
    );
    core.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    let revision = core.document_info().unwrap().document_revision;
    let history = core.history_entries().len();
    assert!(matches!(
        core.apply_scoped_color_replace(stale),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(core.document_info().unwrap().document_revision, revision);
    assert_eq!(core.history_entries().len(), history);

    let overflow = request(
        &core,
        plane_id,
        ScopedColorReplaceMode::RasterColor,
        old,
        new,
        Some(SelectionShape::Rectangle(RectI32 {
            x: i32::MAX,
            y: 0,
            width: 2,
            height: 1,
        })),
    );
    assert!(matches!(
        core.preview_scoped_color_replace(&overflow),
        Err(CoreError::InvalidArgument(_))
    ));

    let plane_name = core
        .layers()
        .unwrap()
        .into_iter()
        .flat_map(|layer| layer.planes)
        .find(|plane| plane.id == plane_id)
        .unwrap()
        .name;
    core.set_plane_properties(plane_id, false, true, 1_000, &plane_name)
        .unwrap();
    let hidden = request(
        &core,
        plane_id,
        ScopedColorReplaceMode::RasterColor,
        old,
        new,
        None,
    );
    assert!(matches!(
        core.apply_scoped_color_replace(hidden),
        Err(CoreError::InvalidState(_))
    ));
    core.set_plane_properties(plane_id, true, true, 1_000, &plane_name)
        .unwrap();
    core.set_plane_properties(plane_id, true, false, 1_000, &plane_name)
        .unwrap();
    let locked = request(
        &core,
        plane_id,
        ScopedColorReplaceMode::RasterColor,
        old,
        new,
        None,
    );
    assert!(matches!(
        core.apply_scoped_color_replace(locked),
        Err(CoreError::InvalidState(_))
    ));
    core.set_plane_properties(plane_id, true, true, 1_000, &plane_name)
        .unwrap();

    let exact = request(
        &core,
        plane_id,
        ScopedColorReplaceMode::RasterColor,
        old,
        new,
        None,
    );
    core.apply_scoped_color_replace(exact).unwrap();
    core.verify_journal_replay().unwrap();
    let path = std::env::temp_dir().join(format!(
        "inkpod-color-replace-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.plane_pixel(ActivePlane::Color, 0, 0).unwrap(), new);
    assert_eq!(reopened.plane_pixel(ActivePlane::Color, 3, 3).unwrap(), old);
    reopened.verify_journal_replay().unwrap();
    let _ = fs::remove_file(path);
}
