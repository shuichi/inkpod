use super::*;

fn binary(width: u32, height: u32) -> TileRaster {
    TileRaster::new(width, height, PixelFormat::BinaryMask8).unwrap()
}

fn color8(width: u32, height: u32) -> TileRaster {
    TileRaster::new(width, height, PixelFormat::StraightRgba8).unwrap()
}

fn select_all(width: u32, height: u32) -> TileRaster {
    let mut selection = binary(width, height);
    for y in 0..height {
        for x in 0..width {
            selection
                .set_pixel(x, y, PixelValue::Binary(255), 1)
                .unwrap();
        }
    }
    selection
}

fn rectangle_boundary(raster: &mut TileRaster, left: u32, top: u32, right: u32, bottom: u32) {
    for x in left..=right {
        raster
            .set_pixel(x, top, PixelValue::Binary(255), 1)
            .unwrap();
        raster
            .set_pixel(x, bottom, PixelValue::Binary(255), 1)
            .unwrap();
    }
    for y in top..=bottom {
        raster
            .set_pixel(left, y, PixelValue::Binary(255), 1)
            .unwrap();
        raster
            .set_pixel(right, y, PixelValue::Binary(255), 1)
            .unwrap();
    }
}

fn apply_plan(raster: &mut TileRaster, plan: &FillPlan) {
    for edit in &plan.edits {
        assert_eq!(raster.pixel(edit.x, edit.y).unwrap(), edit.before);
        raster.set_pixel(edit.x, edit.y, edit.after, 2).unwrap();
    }
}

#[test]
fn sparse_tiles_are_copy_on_write_and_edge_tiles_are_compact() {
    let mut raster = TileRaster::new(65, 65, PixelFormat::StraightRgba8).unwrap();
    assert_eq!(raster.allocated_tile_count(), 0);
    raster
        .set_pixel(64, 64, PixelValue::Rgba([1, 2, 3, 255]), 7)
        .unwrap();
    let mut copy = raster.clone();
    copy.set_pixel(64, 64, PixelValue::Rgba([4, 5, 6, 255]), 8)
        .unwrap();

    assert_eq!(
        raster.pixel(64, 64).unwrap(),
        PixelValue::Rgba([1, 2, 3, 255])
    );
    assert_eq!(
        copy.pixel(64, 64).unwrap(),
        PixelValue::Rgba([4, 5, 6, 255])
    );
    let data = raster.tile_data(TileCoord { x: 1, y: 1 }).unwrap();
    assert_eq!((data.width, data.height, data.bytes.len()), (1, 1, 4));
}

#[test]
fn borrowed_tile_view_exposes_full_stride_and_logical_edge_extent() {
    let mut raster = TileRaster::new(65, 66, PixelFormat::StraightRgba8).unwrap();
    let coord = TileCoord { x: 1, y: 1 };
    assert!(raster.tile_view(coord).is_none());
    raster
        .set_pixel(64, 64, PixelValue::Rgba([1, 2, 3, 4]), 7)
        .unwrap();
    raster
        .set_pixel(64, 65, PixelValue::Rgba([5, 6, 7, 8]), 8)
        .unwrap();

    let view = raster.tile_view(coord).unwrap();
    assert_eq!(view.coord(), coord);
    assert_eq!((view.width(), view.height()), (1, 2));
    assert_eq!(view.row_stride_bytes(), TILE_SIZE * 4);
    assert_eq!(view.bytes().len(), (TILE_SIZE * TILE_SIZE * 4) as usize);
    assert_eq!(&view.bytes()[0..4], &[1, 2, 3, 4]);
    let second_row = view.row_stride_bytes() as usize;
    assert_eq!(&view.bytes()[second_row..second_row + 4], &[5, 6, 7, 8]);
    assert_eq!(view.revision(), 8);

    let same_view = raster.tile_view(coord).unwrap();
    assert_eq!(view.bytes().as_ptr(), same_view.bytes().as_ptr());
}

#[test]
fn transparent_write_does_not_allocate_and_empty_tile_can_be_removed() {
    let mut raster = TileRaster::new(128, 128, PixelFormat::BinaryMask8).unwrap();
    raster.set_pixel(2, 3, PixelValue::Binary(0), 1).unwrap();
    assert_eq!(raster.allocated_tile_count(), 0);
    raster.set_pixel(2, 3, PixelValue::Binary(255), 2).unwrap();
    raster.set_pixel(2, 3, PixelValue::Binary(0), 3).unwrap();
    raster.remove_tile_if_empty(TileCoord { x: 0, y: 0 });
    assert_eq!(raster.allocated_tile_count(), 0);
}

#[test]
fn straight_alpha_preserves_rgb_when_alpha_is_zero() {
    let mut raster = TileRaster::new(64, 64, PixelFormat::StraightRgba8).unwrap();
    let value = PixelValue::Rgba([12, 34, 56, 0]);
    raster.set_pixel(1, 2, value, 1).unwrap();
    assert_eq!(raster.pixel(1, 2).unwrap(), value);
    assert_eq!(raster.allocated_tile_count(), 1);
}

#[test]
fn binary_mask_rejects_intermediate_values() {
    let mut raster = TileRaster::new(64, 64, PixelFormat::BinaryMask8).unwrap();
    assert_eq!(
        raster.set_pixel(0, 0, PixelValue::Binary(1), 1),
        Err(RasterError::PixelFormatMismatch)
    );
    assert_eq!(
        raster.insert_tile(TileData {
            coord: TileCoord { x: 0, y: 0 },
            width: 64,
            height: 64,
            bytes: vec![1; 64 * 64],
            revision: 1,
        }),
        Err(RasterError::PixelFormatMismatch)
    );
}

#[test]
fn golden_only_completely_closed_regions_are_filled() {
    let mut main = binary(9, 7);
    rectangle_boundary(&mut main, 1, 1, 4, 5);
    // The second outline is deliberately open at its top.
    for y in 1..=5 {
        main.set_pixel(6, y, PixelValue::Binary(255), 1).unwrap();
        main.set_pixel(8, y, PixelValue::Binary(255), 1).unwrap();
    }
    for x in 6..=8 {
        main.set_pixel(x, 5, PixelValue::Binary(255), 1).unwrap();
    }
    let mut color = color8(9, 7);
    let operation = select_all(9, 7);
    let fill = PixelValue::Rgba([30, 80, 200, 255]);
    let plan =
        closed_region_fill(&main, &color, &operation, fill, &FillOptions::default()).unwrap();
    apply_plan(&mut color, &plan);

    assert_eq!(color.pixel(2, 2).unwrap(), fill);
    assert_eq!(color.pixel(7, 2).unwrap(), PixelValue::Rgba([0; 4]));
    assert_eq!(color.pixel(0, 0).unwrap(), PixelValue::Rgba([0; 4]));
}

#[test]
fn golden_one_pixel_gap_leaks_at_zero_and_closes_at_one() {
    let mut main = binary(7, 7);
    rectangle_boundary(&mut main, 1, 1, 5, 5);
    main.set_pixel(3, 1, PixelValue::Binary(0), 2).unwrap();
    let color = color8(7, 7);
    let fill = PixelValue::Rgba([10, 20, 30, 255]);
    let mut options = FillOptions {
        overflow_abort: true,
        ..FillOptions::default()
    };
    assert!(matches!(
        seed_fill(&main, &color, None, (3, 3), fill, &options),
        Err(FillError::Overflow { .. })
    ));

    options.gap_close = 1;
    let plan = seed_fill(&main, &color, None, (3, 3), fill, &options).unwrap();
    assert!(plan.edits.iter().any(|edit| (edit.x, edit.y) == (3, 3)));
    assert!(plan.edits.iter().all(|edit| edit.y > 1 && edit.y < 5));
}

#[test]
fn golden_overflow_abort_and_cancel_never_mutate_the_source() {
    let main = binary(8, 8);
    let color = color8(8, 8);
    let checksum = color.checksum();
    let options = FillOptions {
        overflow_abort: true,
        ..FillOptions::default()
    };
    assert!(matches!(
        seed_fill(
            &main,
            &color,
            None,
            (4, 4),
            PixelValue::Rgba([1, 2, 3, 255]),
            &options,
        ),
        Err(FillError::Overflow { .. })
    ));
    assert_eq!(color.checksum(), checksum);
    assert_eq!(
        seed_fill_with_cancel(
            &main,
            &color,
            None,
            (4, 4),
            PixelValue::Rgba([1, 2, 3, 255]),
            &FillOptions::default(),
            || true,
        ),
        Err(FillError::Cancelled)
    );
    assert_eq!(color.checksum(), checksum);
}

#[test]
fn golden_inclusion_replaces_target_trace_but_preserves_other_trace() {
    let mut main = binary(9, 7);
    rectangle_boundary(&mut main, 1, 1, 7, 5);
    let mut color = color8(9, 7);
    let included = PixelValue::Rgba([255, 0, 0, 255]);
    let preserved = PixelValue::Rgba([0, 0, 255, 255]);
    for y in 2..5 {
        color.set_pixel(3, y, included, 1).unwrap();
        color.set_pixel(6, y, preserved, 1).unwrap();
    }
    let fill = PixelValue::Rgba([0, 180, 80, 255]);
    let options = FillOptions {
        inclusion_mode: InclusionMode::Specified,
        inclusion_colors: vec![included],
        ..FillOptions::default()
    };
    let plan = seed_fill(&main, &color, None, (2, 3), fill, &options).unwrap();
    apply_plan(&mut color, &plan);

    assert_eq!(color.pixel(3, 3).unwrap(), fill);
    assert_eq!(color.pixel(6, 3).unwrap(), preserved);
}

#[test]
fn golden_grayscale_display_coverage_and_base_color_eyedropper_agree() {
    let mut coverage = TileRaster::new(3, 3, PixelFormat::Grayscale8).unwrap();
    coverage
        .set_pixel(1, 1, PixelValue::Grayscale8(128), 1)
        .unwrap();
    let base = PixelValue::Rgba16([1_000, 2_000, 3_000, u16::MAX]);
    let plane = PlaneSample {
        raster: &coverage,
        base_color: Some(base),
    };
    assert_eq!(
        eyedropper(EyedropperSource::SelectedPlane, 1, 1, plane, &[plane], &[],).unwrap(),
        Some(base)
    );
    let composite = eyedropper(EyedropperSource::Composite, 1, 1, plane, &[plane], &[])
        .unwrap()
        .unwrap();
    let PixelValue::Rgba16(composite) = composite else {
        panic!("grayscale 16-bit base must retain its depth");
    };
    assert_eq!(&composite[..3], &[1_000, 2_000, 3_000]);
    assert_eq!(composite[3], 128 * 257);
}

#[test]
fn golden_selection_clips_every_fill_edit() {
    let main = binary(6, 6);
    let mut color = color8(6, 6);
    let mut selection = binary(6, 6);
    for y in 2..=3 {
        for x in 1..=2 {
            selection
                .set_pixel(x, y, PixelValue::Binary(255), 1)
                .unwrap();
        }
    }
    let fill = PixelValue::Rgba([80, 90, 100, 255]);
    let plan = seed_fill(
        &main,
        &color,
        Some(&selection),
        (1, 2),
        fill,
        &FillOptions::default(),
    )
    .unwrap();
    apply_plan(&mut color, &plan);
    assert_eq!(plan.edits.len(), 4);
    assert_eq!(color.pixel(1, 2).unwrap(), fill);
    assert_eq!(color.pixel(0, 2).unwrap(), PixelValue::Rgba([0; 4]));
    assert_eq!(color.pixel(3, 3).unwrap(), PixelValue::Rgba([0; 4]));
}

#[test]
fn golden_rgba16_palette_and_fill_are_never_implicitly_quantized() {
    let mut palette = Palette::default();
    let exact = PixelValue::Rgba16([1, 257, 32_769, 65_534]);
    palette.push(exact).unwrap();
    assert_eq!(palette.colors(), &[exact]);

    let main = binary(4, 4);
    let color = TileRaster::new(4, 4, PixelFormat::StraightRgba16).unwrap();
    let plan = seed_fill(&main, &color, None, (1, 1), exact, &FillOptions::default()).unwrap();
    assert!(plan.edits.iter().all(|edit| edit.after == exact));
}

#[test]
fn tolerance_detached_closed_extension_and_color_check_semantics() {
    let main = binary(5, 3);
    let mut color = color8(5, 3);
    color
        .set_pixel(0, 1, PixelValue::Rgba([10, 10, 10, 255]), 1)
        .unwrap();
    color
        .set_pixel(4, 1, PixelValue::Rgba([11, 10, 10, 255]), 1)
        .unwrap();
    let options = FillOptions {
        tolerance: 257,
        detached_regions: true,
        ..FillOptions::default()
    };
    let plan = seed_fill(
        &main,
        &color,
        None,
        (0, 1),
        PixelValue::Rgba([50, 60, 70, 255]),
        &options,
    )
    .unwrap();
    assert!(plan.edits.iter().any(|edit| (edit.x, edit.y) == (4, 1)));

    let mut extension = color8(5, 3);
    let source = PixelValue::Rgba([100, 110, 120, 255]);
    extension.set_pixel(1, 1, source, 1).unwrap();
    let operation = select_all(5, 3);
    let extension_plan = extend_fill(&extension, &operation, (1, 1), 2).unwrap();
    assert!(
        extension_plan
            .edits
            .iter()
            .all(|edit| edit.after == source && edit.x.abs_diff(1) + edit.y.abs_diff(1) <= 2)
    );

    assert_eq!(
        color_check_category(
            PixelValue::Rgba([255, 255, 255, 255]),
            ColorCheckMode::LegacyWhiteTransparency,
        ),
        ColorCheckCategory::ExactWhite
    );
    assert_eq!(
        color_check_category(
            PixelValue::Rgba([255, 255, 255, 0]),
            ColorCheckMode::NativeAlpha,
        ),
        ColorCheckCategory::Transparent
    );
    assert_eq!(
        color_check_category(
            PixelValue::Rgba([255, 255, 255, 0]),
            ColorCheckMode::LegacyWhiteTransparency,
        ),
        ColorCheckCategory::ExactWhite
    );
}

#[test]
fn closed_region_handles_colored_components_and_all_fill_plans_cancel_atomically() {
    let mut main = binary(7, 7);
    rectangle_boundary(&mut main, 1, 1, 5, 5);
    let mut color = color8(7, 7);
    let source = PixelValue::Rgba([20, 30, 40, 255]);
    for y in 2..5 {
        for x in 2..5 {
            color.set_pixel(x, y, source, 1).unwrap();
        }
    }
    let operation = select_all(7, 7);
    let fill = PixelValue::Rgba([100, 110, 120, 255]);
    let plan =
        closed_region_fill(&main, &color, &operation, fill, &FillOptions::default()).unwrap();
    assert_eq!(plan.edits.len(), 9);
    assert!(plan.edits.iter().all(|edit| edit.before == source));

    let transparent_only = FillOptions {
        transparent_only: true,
        ..FillOptions::default()
    };
    assert!(
        closed_region_fill(&main, &color, &operation, fill, &transparent_only)
            .unwrap()
            .edits
            .is_empty()
    );

    let checksum = color.checksum();
    assert_eq!(
        closed_region_fill_with_cancel(
            &main,
            &color,
            &operation,
            fill,
            &FillOptions::default(),
            || true,
        ),
        Err(FillError::Cancelled)
    );
    assert_eq!(
        extend_fill_with_cancel(&color, &operation, (2, 2), 2, || true),
        Err(FillError::Cancelled)
    );
    assert_eq!(color.checksum(), checksum);
}
