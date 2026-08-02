use super::*;

fn rgba8(width: u32, height: u32, pixels: Vec<u8>) -> CommonRaster {
    CommonRaster::new(
        width,
        height,
        PixelFormat::StraightRgba8,
        Some(DEFAULT_DPI_MILLI),
        Some(DEFAULT_DPI_MILLI),
        pixels,
    )
    .unwrap()
}

fn rgba16(width: u32, height: u32, channels: Vec<u16>) -> CommonRaster {
    CommonRaster::new(
        width,
        height,
        PixelFormat::StraightRgba16,
        Some(DEFAULT_DPI_MILLI),
        Some(DEFAULT_DPI_MILLI),
        channels.into_iter().flat_map(u16::to_le_bytes).collect(),
    )
    .unwrap()
}

fn source(name: &str, uuid: u128, width: u32, height: u32, pixel: [u8; 4]) -> SequenceCellSource {
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    pixels[..4].copy_from_slice(&pixel);
    SequenceCellSource::from_common_raster(name, uuid, &rgba8(width, height, pixels)).unwrap()
}

#[test]
fn acceptance_reference_frame_aligns_different_cell_sizes_and_reopens() {
    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let mut pixels = vec![0_u8; 4 * 4 * 4];
    pixels[..4].copy_from_slice(&[10, 20, 30, 255]);
    let source_offset = (2 * 4 + 2) * 4;
    pixels[source_offset..source_offset + 4].copy_from_slice(&[200, 40, 20, 255]);
    let source_corner_offset = (3 * 4 + 3) * 4;
    pixels[source_corner_offset..source_corner_offset + 4].copy_from_slice(&[50, 60, 70, 255]);
    let source = LightTableSource::from_common_raster(
        0x1111,
        7,
        RectI32 {
            x: 2,
            y: 2,
            width: 4,
            height: 4,
        },
        &rgba8(4, 4, pixels),
    )
    .unwrap();
    core.light_table_add_item(LightTableItemInput::new("small reference", source))
        .unwrap();
    assert_eq!(
        core.light_table_sample(4, 4).unwrap(),
        PixelValue::Rgba([200, 40, 20, 255])
    );
    assert_eq!(
        core.light_table_sample(2, 2).unwrap(),
        PixelValue::Rgba([10, 20, 30, 255])
    );
    assert_eq!(
        core.light_table_sample(5, 5).unwrap(),
        PixelValue::Rgba([50, 60, 70, 255])
    );
    assert!(matches!(
        core.light_table_sample(0, 0),
        Err(CoreError::InvalidState(_))
    ));
    let snapshot = core.build_snapshot();
    let tile = &snapshot.tiles()[0];
    let mut golden = vec![0_u8; 8 * 8 * 4];
    golden[(2 * 8 + 2) * 4..(2 * 8 + 2) * 4 + 4].copy_from_slice(&[30, 20, 10, 255]);
    golden[(4 * 8 + 4) * 4..(4 * 8 + 4) * 4 + 4].copy_from_slice(&[20, 40, 200, 255]);
    golden[(5 * 8 + 5) * 4..(5 * 8 + 5) * 4 + 4].copy_from_slice(&[70, 60, 50, 255]);
    assert_eq!(tile.stride_bytes(), 8 * 4);
    assert_eq!(tile.pixels(), golden);

    let path = std::env::temp_dir().join(format!(
        "inkpod-test-reference-{}-{}.inkpod",
        std::process::id(),
        core.document_info().unwrap().document_revision
    ));
    let _ = std::fs::remove_file(&path);
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(
        reopened.light_table_sample(4, 4).unwrap(),
        PixelValue::Rgba([200, 40, 20, 255])
    );
    assert_eq!(
        reopened.light_table_sample(2, 2).unwrap(),
        PixelValue::Rgba([10, 20, 30, 255])
    );
    assert_eq!(
        reopened.light_table_sample(5, 5).unwrap(),
        PixelValue::Rgba([50, 60, 70, 255])
    );
    let before_swap = reopened.light_table_items().unwrap();
    assert_eq!(before_swap.len(), 1);
    let old_uuid = reopened.document_info().unwrap().document_uuid;
    let swapped = reopened
        .light_table_swap_with_active(before_swap[0].id)
        .unwrap();
    assert_eq!(swapped.document_uuid, 0x1111);
    assert_eq!((swapped.width, swapped.height), (4, 4));
    let after_swap = reopened.light_table_items().unwrap();
    assert_eq!(after_swap[0].id, before_swap[0].id);
    assert_eq!(after_swap[0].opacity_milli, before_swap[0].opacity_milli);
    assert_eq!(after_swap[0].source_document_uuid, old_uuid);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn acceptance_individual_and_global_opacity_multiply_to_twenty_five_percent() {
    let mut core = Core::new();
    core.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let source = LightTableSource::from_common_raster(
        0x2222,
        1,
        RectI32 {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        },
        &rgba8(2, 2, [100, 120, 140, 255].repeat(4)),
    )
    .unwrap();
    let mut input = LightTableItemInput::new("half", source);
    input.opacity_milli = 500;
    core.light_table_add_item(input).unwrap();
    core.light_table_set_global_opacity(500).unwrap();
    let items = core.light_table_items().unwrap();
    assert_eq!(items[0].effective_opacity_milli, 250);
    assert_eq!(
        core.light_table_sample(1, 1).unwrap(),
        PixelValue::Rgba([100, 120, 140, 64])
    );
}

#[test]
fn light_table_color_sampling_preserves_exact_rgba16() {
    let mut core = Core::new();
    core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let source = LightTableSource::from_common_raster(
        0x2424,
        1,
        RectI32 {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        &rgba16(1, 1, vec![1, 257, 32_769, 65_535]),
    )
    .unwrap();
    core.light_table_add_item(LightTableItemInput::new("RGBA16", source))
        .unwrap();
    assert_eq!(
        core.eyedropper(EyedropperSource::LightTableTopmost, 0, 0)
            .unwrap(),
        PixelValue::Rgba16([1, 257, 32_769, 65_535])
    );
    core.light_table_set_global_opacity(500).unwrap();
    assert_eq!(
        core.light_table_sample(0, 0).unwrap(),
        PixelValue::Rgba16([1, 257, 32_769, 32_768])
    );
    let before_fill = core.document_info().unwrap();
    assert!(matches!(
        core.apply_fill_with_light_table(&fill_request(0, 0, [10, 20, 30, 255]), false, true,),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(core.document_info().unwrap(), before_fill);
}

#[test]
fn light_table_set_item_management_is_transactional_and_stable_id_based() {
    let mut core = Core::new();
    core.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let default_set_id = core.light_table_sets().unwrap()[0].id;
    let (_, set_id) = core.light_table_create_set("References").unwrap();
    let path = std::env::temp_dir().join(format!(
        "inkpod-test-active-set-{}-{}.inkpod",
        std::process::id(),
        core.document_info().unwrap().document_revision
    ));
    let _ = std::fs::remove_file(&path);
    core.save(&path).unwrap();
    let before_active_switch = core.document_info().unwrap();
    let active_switch = core.light_table_set_active(default_set_id).unwrap();
    assert_eq!(
        active_switch.revision(),
        before_active_switch.document_revision + 1
    );
    assert!(core.document_info().unwrap().dirty);
    assert!(
        core.light_table_sets()
            .unwrap()
            .iter()
            .any(|set| set.id == default_set_id && set.active)
    );
    core.undo().unwrap();
    assert!(!core.document_info().unwrap().dirty);
    assert!(
        core.light_table_sets()
            .unwrap()
            .iter()
            .any(|set| set.id == set_id && set.active)
    );
    let source = LightTableSource::from_common_raster(
        0x2525,
        1,
        RectI32 {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        },
        &rgba8(2, 2, [20, 40, 60, 255].repeat(4)),
    )
    .unwrap();
    let mut invalid_source = source.clone();
    invalid_source.document_uuid = 0;
    let before_invalid_source = core.document_info().unwrap();
    assert!(matches!(
        core.light_table_add_item(LightTableItemInput::new("Invalid", invalid_source)),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), before_invalid_source);
    let mut invalid_rotation = LightTableItemInput::new("Invalid", source.clone());
    invalid_rotation.rotation_milli_degrees = i32::MIN;
    let before_invalid = core.document_info().unwrap();
    assert!(matches!(
        core.light_table_add_item(invalid_rotation),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), before_invalid);
    let (_, item_id) = core
        .light_table_add_item(LightTableItemInput::new("Item", source.clone()))
        .unwrap();
    let (_, duplicate_id) = core.light_table_duplicate_set(set_id).unwrap();
    assert_ne!(duplicate_id, set_id);
    let duplicate_item_id = core.light_table_items().unwrap()[0].id;
    assert_ne!(duplicate_item_id, item_id);
    core.light_table_rename_set(duplicate_id, "References")
        .unwrap();
    core.light_table_reorder_set(duplicate_id, 0).unwrap();
    core.light_table_set_active(set_id).unwrap();
    let mut update = LightTableItemInput::new("Moved", source);
    update.translate_x_milli = -1_000;
    core.light_table_update_item(item_id, update).unwrap();
    assert_eq!(
        core.light_table_sample(0, 1).unwrap(),
        PixelValue::Rgba([20, 40, 60, 255])
    );
    core.light_table_remove_item(item_id).unwrap();
    assert!(core.light_table_items().unwrap().is_empty());
    core.undo().unwrap();
    assert_eq!(core.light_table_items().unwrap()[0].id, item_id);
    core.redo().unwrap();
    assert!(core.light_table_items().unwrap().is_empty());
    core.light_table_delete_set(set_id).unwrap();
    core.light_table_delete_set(duplicate_id).unwrap();
    let sets = core.light_table_sets().unwrap();
    assert_eq!(sets.len(), 1);
    let final_set_id = sets[0].id;
    assert!(core.light_table_delete_set(final_set_id).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn acceptance_light_table_fill_boundary_is_read_only() {
    let mut core = Core::new();
    core.new_cell(5, 5, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let mut pixels = vec![0_u8; 5 * 5 * 4];
    for y in 0..5 {
        let offset = (y * 5 + 2) * 4;
        pixels[offset..offset + 4].copy_from_slice(&[10, 20, 30, 255]);
    }
    let source = LightTableSource::from_common_raster(
        0x3333,
        9,
        RectI32 {
            x: 2,
            y: 2,
            width: 5,
            height: 5,
        },
        &rgba8(5, 5, pixels),
    )
    .unwrap();
    core.light_table_add_item(LightTableItemInput::new("boundary", source))
        .unwrap();
    let before_item = core.light_table_items().unwrap()[0].clone();
    let before_sample = core.light_table_sample(2, 2).unwrap();
    let before_cancel = core.document_info().unwrap();
    let mut cancellation_polls = 0;
    assert_eq!(
        core.apply_fill_with_light_table_and_cancel(
            &fill_request(0, 2, [200, 0, 0, 255]),
            true,
            false,
            || {
                cancellation_polls += 1;
                cancellation_polls == 2
            },
        ),
        Err(CoreError::Cancelled)
    );
    assert_eq!(core.document_info().unwrap(), before_cancel);
    assert_eq!(core.light_table_items().unwrap()[0], before_item);
    assert_eq!(core.light_table_sample(2, 2).unwrap(), before_sample);
    let outcome = core
        .apply_fill_with_light_table(&fill_request(0, 2, [200, 0, 0, 255]), true, false)
        .unwrap();
    assert_eq!(outcome.changed_pixels, 10);
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 1, 2).unwrap(),
        PixelValue::Rgba([200, 0, 0, 255])
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 3, 2).unwrap(),
        PixelValue::Rgba([0, 0, 0, 0])
    );
    assert_eq!(core.light_table_items().unwrap()[0], before_item);
    assert_eq!(core.light_table_sample(2, 2).unwrap(), before_sample);
    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 1, 2).unwrap(),
        PixelValue::Rgba([0, 0, 0, 0])
    );
    assert_eq!(core.light_table_sample(2, 2).unwrap(), before_sample);
}

#[test]
fn acceptance_sequence_switch_rejects_unsaved_document_without_discarding_it() {
    let mut core = Core::new();
    let current = core
        .new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_sequence(vec![
        source("cell1.png", current.document_uuid, 2, 2, [1, 2, 3, 255]),
        source("cell2.png", 0x4444, 3, 2, [4, 5, 6, 255]),
    ])
    .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 0.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let before = core.document_info().unwrap();
    assert_eq!(
        core.sequence_step(SequenceDirection::Next, false),
        Err(CoreError::UnsavedChanges)
    );
    let after_rejection = core.document_info().unwrap();
    assert_eq!(after_rejection.document_uuid, before.document_uuid);
    assert_eq!(after_rejection.document_revision, before.document_revision);
    assert!(after_rejection.dirty);

    let path = std::env::temp_dir().join(format!(
        "inkpod-test-switch-{}-{}.inkpod",
        std::process::id(),
        before.document_revision
    ));
    let _ = std::fs::remove_file(&path);
    core.save(&path).unwrap();
    let switched = core.sequence_step(SequenceDirection::Next, false).unwrap();
    assert_eq!(switched.document_uuid, 0x4444);
    assert_eq!((switched.width, switched.height), (3, 2));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn acceptance_sequence_gaps_natural_order_thumbnails_subpalette_and_motion() {
    let mut core = Core::new();
    core.set_sequence(vec![
        source("cut10.png", 10, 2, 1, [10, 0, 0, 255]),
        source("cut1.png", 1, 1, 1, [1, 0, 0, 255]),
        source("cut3.png", 3, 3, 1, [3, 0, 0, 255]),
    ])
    .unwrap();
    let cells = core.sequence_cells().unwrap();
    assert_eq!(
        cells
            .iter()
            .map(|cell| cell.cell_number)
            .collect::<Vec<_>>(),
        vec![1, 3, 10]
    );
    assert!(cells.iter().all(|cell| cell.thumbnail.checksum != 0));
    core.set_subpalette_cell(1).unwrap();
    assert_eq!(
        core.subpalette_sample(0, 0).unwrap(),
        PixelValue::Rgba([3, 0, 0, 255])
    );
    let first = core
        .motion_check_start(MotionCheckConfig {
            fps: 24,
            loop_playback: true,
            include_selection: true,
            include_light_table: true,
        })
        .unwrap();
    assert_eq!(first.cell_number, 1);
    assert_eq!(first.fps, 24);
    assert!(first.include_selection && first.include_light_table);
    assert_eq!(
        core.motion_check_step(SequenceDirection::Next)
            .unwrap()
            .cell_number,
        3
    );
    assert_eq!(
        core.motion_check_step(SequenceDirection::Next)
            .unwrap()
            .cell_number,
        10
    );
    assert_eq!(
        core.motion_check_step(SequenceDirection::Next)
            .unwrap()
            .cell_number,
        1
    );
    assert!(core.motion_check_toggle_pause().unwrap().paused);

    let exported = core
        .export_sequence(CommonRasterFormat::Png, false)
        .unwrap();
    assert_eq!(exported.len(), 3);
    let mut imported = Core::new();
    imported
        .import_sequence(CommonRasterFormat::Png, exported)
        .unwrap();
    assert_eq!(
        imported
            .sequence_cells()
            .unwrap()
            .iter()
            .map(|cell| cell.cell_number)
            .collect::<Vec<_>>(),
        vec![1, 3, 10]
    );

    let mixed = vec![
        (
            "mixed2.bmp".to_owned(),
            CommonRasterFormat::Bmp,
            encode_common_raster(
                CommonRasterFormat::Bmp,
                &rgba8(1, 1, vec![20, 30, 40, 255]),
                false,
            )
            .unwrap(),
        ),
        (
            "mixed1.png".to_owned(),
            CommonRasterFormat::Png,
            encode_common_raster(
                CommonRasterFormat::Png,
                &rgba8(1, 1, vec![1, 2, 3, 255]),
                false,
            )
            .unwrap(),
        ),
    ];
    imported.import_mixed_sequence(mixed).unwrap();
    assert_eq!(imported.sequence_cell(0).unwrap().cell_number, 1);
    assert_eq!(imported.sequence_cell(1).unwrap().cell_number, 2);
    assert!(matches!(
        imported.sequence_cell(2),
        Err(CoreError::InvalidArgument(_))
    ));
}

#[test]
fn subpalette_reference_snapshot_has_independent_view_and_never_edits_document() {
    let mut core = Core::new();
    core.new_cell(8, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_sequence(vec![source(
        "reference12.png",
        12,
        3,
        2,
        [40, 80, 120, 200],
    )])
    .unwrap();
    core.set_subpalette_cell(0).unwrap();
    let view_id = core.create_view().unwrap();
    let before = core.document_info().unwrap();
    let primary_view = core.view_state();
    core.apply_subpalette_view_for(
        view_id,
        ViewCommand::Fit {
            viewport_width: 300.0,
            viewport_height: 100.0,
        },
    )
    .unwrap();
    core.apply_subpalette_view_for(
        view_id,
        ViewCommand::Flip {
            axis: MirrorAxis::Horizontal,
        },
    )
    .unwrap();
    let snapshot = core.build_subpalette_snapshot_for(view_id).unwrap();
    assert_eq!(
        (snapshot.document_width(), snapshot.document_height()),
        (3, 2)
    );
    assert_eq!(snapshot.tile_count(), 1);
    assert!(snapshot.view().flip_horizontal());
    assert!(snapshot.view().zoom() > 1.0);
    assert_eq!(core.view_state(), primary_view);
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(
        core.subpalette_sample(0, 0).unwrap(),
        PixelValue::Rgba([40, 80, 120, 200])
    );
    assert_eq!(
        core.subpalette_view_sample(view_id, 200.0, 25.0).unwrap(),
        PixelValue::Rgba([40, 80, 120, 200])
    );
    assert_eq!(
        core.subpalette_sample(3, 1),
        Err(CoreError::Raster(
            inkpod_image::RasterError::PixelOutOfBounds
        ))
    );
}

#[test]
fn rejects_a_mutated_common_raster_before_indexing_its_pixels() {
    let mut malformed = rgba8(1, 1, vec![1, 2, 3, 4]);
    malformed.pixels.clear();
    assert!(matches!(
        LightTableSource::from_common_raster(
            0x5151,
            1,
            RectI32 {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            &malformed,
        ),
        Err(CoreError::Format(_))
    ));

    let mut invalid_cell = source("cell1.png", 0x6161, 1, 1, [1, 2, 3, 255]);
    invalid_cell.frames.reference_frame.width = 0;
    let mut core = Core::new();
    assert!(matches!(
        core.set_sequence(vec![invalid_cell]),
        Err(CoreError::InvalidArgument(_))
    ));
    assert!(matches!(
        core.sequence_cells(),
        Err(CoreError::InvalidState(_))
    ));
}
