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

fn sequence_render_core(cells: Vec<SequenceCellSource>) -> Core {
    let mut core = Core::new();
    core.new_cell_with_uuid(4, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x7000)
        .unwrap();
    core.set_sequence(cells).unwrap();
    core.sequence_activate(0).unwrap();
    core
}

#[test]
fn sequence_render_revisit_reuses_exact_payload_and_tile_revision() {
    let mut core = sequence_render_core(vec![
        source("cell1.png", 0x7001, 4, 2, [12, 34, 56, 255]),
        source("cell2.png", 0x7002, 4, 2, [78, 90, 12, 255]),
    ]);
    let first = core.build_snapshot();
    let identity = first.sequence_render_source().unwrap();
    assert_eq!(
        (identity.document_uuid, identity.source_generation),
        (0x7001, 1)
    );
    assert_ne!(identity.owner_generation, 0);
    core.sequence_activate(1).unwrap();
    let second = core.build_snapshot();
    assert_ne!(second.sequence_render_source(), Some(identity));
    assert_ne!(first.tiles()[0].pixels(), second.tiles()[0].pixels());
    core.sequence_activate(0).unwrap();
    let before = core.document_info().unwrap();
    let revisited = core.build_snapshot();
    assert_eq!(revisited.sequence_render_source(), Some(identity));
    assert_eq!(
        revisited.tiles()[0].pixels().as_ptr(),
        first.tiles()[0].pixels().as_ptr()
    );
    assert_eq!(
        revisited.tiles()[0].tile_revision(),
        first.tiles()[0].tile_revision()
    );
    assert_eq!(core.document_info().unwrap(), before);
    let usage = core.resource_usage();
    assert_eq!(usage.sequence_render_cache_source_count, 2);
    assert_eq!(usage.sequence_render_cache_bytes, 4 * 2 * 4 * 2);
    assert_eq!(usage.sequence_render_cache_tile_count, 2);
    assert_eq!(usage.render_cache_bytes, usage.sequence_render_cache_bytes);
}

#[test]
fn sequence_render_first_edit_preserves_unchanged_tiles_and_pristine_bank() {
    let raster = rgba8(128, 1, [12, 34, 56, 255].repeat(128));
    for preview in [false, true] {
        let mut core = sequence_render_core(vec![
            SequenceCellSource::from_common_raster("cell1.png", 0x7081, &raster).unwrap(),
            source("cell2.png", 0x7082, 128, 1, [78, 90, 12, 255]),
        ]);
        let original = core.build_snapshot();
        assert_eq!(original.tiles().len(), 2);
        let stroke = line_stroke(vec![StrokeSample {
            x: 1.0,
            y: 0.0,
            pressure: 1.0,
        }]);
        if preview {
            core.begin_stroke(&stroke).unwrap();
            let previewed = core.build_snapshot();
            assert_eq!(previewed.sequence_render_source(), None);
            assert_eq!(
                previewed.tiles()[1].pixels().as_ptr(),
                original.tiles()[1].pixels().as_ptr(),
                "a first stroke preview must preserve the untouched tile"
            );
            assert_ne!(previewed.tiles()[0].pixels(), original.tiles()[0].pixels());
            core.cancel_stroke();
            let cancelled = core.build_snapshot();
            assert_eq!(
                cancelled.sequence_render_source(),
                original.sequence_render_source()
            );
            assert_eq!(
                cancelled.tiles()[0].pixels().as_ptr(),
                original.tiles()[0].pixels().as_ptr()
            );
            core.begin_stroke(&stroke).unwrap();
            let _ = core.build_snapshot();
            core.end_stroke().unwrap();
        } else {
            core.apply_stroke(&stroke).unwrap();
        }
        let edited = core.build_snapshot();
        assert_eq!(edited.sequence_render_source(), None);
        assert_ne!(edited.tiles()[0].pixels(), original.tiles()[0].pixels());
        assert_eq!(
            edited.tiles()[1].pixels().as_ptr(),
            original.tiles()[1].pixels().as_ptr(),
            "a first edit must preserve the untouched tile (preview={preview})"
        );
        assert_eq!(
            edited.tiles()[1].tile_revision(),
            original.tiles()[1].tile_revision()
        );
        core.undo().unwrap();
        core.sequence_activate(1).unwrap();
        core.sequence_activate(0).unwrap();
        let revisited = core.build_snapshot();
        for (tile, expected) in revisited.tiles().iter().zip(original.tiles()) {
            assert_eq!(tile.pixels().as_ptr(), expected.pixels().as_ptr());
            assert_eq!(tile.tile_revision(), expected.tile_revision());
        }
    }
}

#[test]
fn sequence_render_color_edit_after_preview_cancel_and_lru_revisit_updates_pixels() {
    let cells = (0..10)
        .map(|index| {
            let raster = rgba8(64, 32, [32 + index * 16, 64, 96, 255].repeat(64 * 32));
            let mut source = SequenceCellSource::from_common_raster(
                format!("source{}.tga", index + 1),
                (0x736f_7572_6365_6361_u128 << 64) | u128::from(index + 1),
                &raster,
            )
            .unwrap();
            source.frames.reference_frame = RectI32 {
                x: 0,
                y: 0,
                width: 64,
                height: 32,
            };
            source
        })
        .collect();
    let mut core = sequence_render_core(cells);
    let show = |core: &mut Core, index: usize| {
        core.sequence_activate(index).unwrap();
        let snapshot = core.build_snapshot();
        assert_eq!(
            &snapshot.tiles()[0].pixels()[..4],
            &[96, 64, 32 + index as u8 * 16, 255]
        );
        snapshot
    };
    for index in [0, 1, 0, 1] {
        let _ = show(&mut core, index);
    }
    let mut stroke = color_stroke(
        PaintTool::Pencil,
        256.0,
        StrokeSample {
            x: 32.0,
            y: 16.0,
            pressure: 1.0,
        },
    );
    stroke.color = [0, 0, 255, 255];
    core.begin_stroke(&stroke).unwrap();
    let preview = core.build_snapshot();
    let center = 16 * preview.tiles()[0].stride_bytes() as usize + 32 * 4;
    assert_eq!(
        &preview.tiles()[0].pixels()[center..center + 4],
        &[255, 0, 0, 255]
    );
    drop(preview);
    core.cancel_stroke();
    for index in [0, 1, 0].into_iter().chain(2..10).chain([0, 1, 0]) {
        let _ = show(&mut core, index);
    }
    core.apply_stroke(&stroke).unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 32, 16).unwrap(),
        PixelValue::Rgba([0, 0, 255, 255])
    );
    // Pencil is one pixel even with diameter 256; the old renderer fixture
    // incorrectly expected the off-center point to have been painted too.
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 16, 12).unwrap(),
        PixelValue::Rgba([0, 0, 0, 0])
    );
    let edited = core.build_snapshot();
    assert_eq!(edited.sequence_render_source(), None);
    assert_eq!(
        &edited.tiles()[0].pixels()[center..center + 4],
        &[255, 0, 0, 255]
    );
    let untouched = 12 * edited.tiles()[0].stride_bytes() as usize + 16 * 4;
    assert_eq!(
        &edited.tiles()[0].pixels()[untouched..untouched + 4],
        &[96, 64, 32, 255]
    );
    core.undo().unwrap();
    let undone = core.build_snapshot();
    assert_eq!(
        &undone.tiles()[0].pixels()[center..center + 4],
        &[96, 64, 32, 255]
    );
    let _ = show(&mut core, 1);
    let _ = show(&mut core, 0);
}

#[test]
fn sequence_render_pristine_identity_excludes_modes_previews_and_saved_edits() {
    let mut core = sequence_render_core(vec![
        source("cell1.png", 0x7101, 4, 2, [12, 34, 56, 255]),
        source("cell2.png", 0x7102, 4, 2, [78, 90, 12, 255]),
    ]);
    let normal = core.build_snapshot();
    let identity = normal.sequence_render_source().unwrap();
    core.apply_view(ViewCommand::SetAlphaView(true)).unwrap();
    assert_eq!(core.build_snapshot().sequence_render_source(), None);
    core.apply_view(ViewCommand::SetAlphaView(false)).unwrap();
    assert_eq!(
        core.build_snapshot().tiles()[0].pixels().as_ptr(),
        normal.tiles()[0].pixels().as_ptr()
    );
    let alpha_view = core.create_view().unwrap();
    core.apply_view_for(alpha_view, ViewCommand::SetAlphaView(true))
        .unwrap();
    let _ = core.build_snapshot();
    let alpha = core.build_snapshot_for(alpha_view).unwrap();
    assert_eq!(alpha.sequence_render_source(), None);
    assert_eq!(&alpha.tiles()[0].pixels()[..4], &[255, 255, 255, 255]);
    assert_eq!(
        core.build_snapshot().tiles()[0].pixels().as_ptr(),
        normal.tiles()[0].pixels().as_ptr()
    );
    core.set_color_check(Some(ColorCheckMode::NativeAlpha))
        .unwrap();
    let checked = core.build_snapshot();
    assert_eq!(checked.sequence_render_source(), None);
    assert_eq!(&checked.tiles()[0].pixels()[..4], &[0, 0, 0, 255]);
    core.set_color_check(None).unwrap();
    let main_plane = core.document_info().unwrap().main_plane_id;
    core.begin_filter_preview(
        main_plane,
        Filter::Invert {
            channel: Channel::Rgb,
        },
    )
    .unwrap();
    let preview = core.build_snapshot();
    assert_eq!(preview.sequence_render_source(), None);
    assert_eq!(&preview.tiles()[0].pixels()[..4], &[199, 221, 243, 255]);
    core.cancel_filter_preview().unwrap();
    assert_eq!(
        core.build_snapshot().sequence_render_source(),
        Some(identity)
    );
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    assert_eq!(core.build_snapshot().sequence_render_source(), None);
    let (_, token) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(false, || false)
        .unwrap();
    core.commit_document_save(token, std::path::Path::new("sequence-render-saved.inkpod"))
        .unwrap();
    assert!(!core.document_info().unwrap().dirty);
    assert_eq!(core.build_snapshot().sequence_render_source(), None);
    core.sequence_activate(1).unwrap();
    core.sequence_activate(0).unwrap();
    let fresh = core.build_snapshot();
    assert_eq!(fresh.sequence_render_source(), Some(identity));
    assert_eq!(
        fresh.tiles()[0].pixels().as_ptr(),
        normal.tiles()[0].pixels().as_ptr()
    );
    assert_eq!(
        fresh.canonical_composite_digest(),
        normal.canonical_composite_digest()
    );
}

#[test]
fn sequence_render_pristine_exit_keeps_metadata_and_light_table_invalidation() {
    let mut core = sequence_render_core(vec![
        source("cell1.png", 0x7181, 4, 2, [12, 34, 56, 255]),
        source("cell2.png", 0x7182, 4, 2, [78, 90, 12, 255]),
    ]);
    let original = core.build_snapshot();
    let layer = core.layers().unwrap().remove(0);
    core.set_layer_properties(layer.id, true, true, 500, &layer.name)
        .unwrap();
    let translucent = core.build_snapshot();
    assert_eq!(translucent.sequence_render_source(), None);
    assert_eq!(&translucent.tiles()[0].pixels()[..4], &[28, 17, 6, 128]);
    core.undo().unwrap();
    core.sequence_activate(1).unwrap();
    core.sequence_activate(0).unwrap();
    assert_eq!(
        core.build_snapshot().tiles()[0].pixels().as_ptr(),
        original.tiles()[0].pixels().as_ptr()
    );
    let light_table = LightTableSource::from_common_raster(
        0x7183,
        1,
        RectI32 {
            x: 2,
            y: 1,
            width: 4,
            height: 2,
        },
        &rgba8(4, 2, [200, 100, 50, 255].repeat(8)),
    )
    .unwrap();
    core.light_table_add_item(LightTableItemInput::new("reference", light_table))
        .unwrap();
    let with_reference = core.build_snapshot();
    assert_eq!(with_reference.sequence_render_source(), None);
    assert_eq!(
        &with_reference.tiles()[0].pixels()[4..8],
        &[50, 100, 200, 255]
    );
}

#[test]
fn sequence_render_catalog_replacement_and_independent_clone_use_new_owner() {
    let mut core = sequence_render_core(vec![
        source("cell1.png", 0x7201, 4, 2, [12, 34, 56, 255]),
        source("cell2.png", 0x7202, 4, 2, [78, 90, 12, 255]),
    ]);
    let original = core.build_snapshot();
    let mut independent = core.clone();
    let cloned = independent.build_snapshot();
    assert_ne!(
        cloned.sequence_render_source().unwrap().owner_generation,
        original.sequence_render_source().unwrap().owner_generation
    );
    assert_eq!(
        cloned, original,
        "runtime cache identity is not semantic snapshot equality"
    );
    core.sequence_activate(1).unwrap();
    core.set_sequence(vec![
        source("cell1.png", 0x7201, 4, 2, [200, 100, 50, 255]),
        source("cell2.png", 0x7202, 4, 2, [78, 90, 12, 255]),
    ])
    .unwrap();
    core.sequence_activate(0).unwrap();
    let replaced = core.build_snapshot();
    assert_ne!(
        replaced.sequence_render_source().unwrap().owner_generation,
        original.sequence_render_source().unwrap().owner_generation
    );
    assert_ne!(replaced.tiles()[0].pixels(), original.tiles()[0].pixels());
}

#[test]
fn sequence_render_live_snapshots_keep_the_eight_source_budget_charged() {
    let cells = (1..=10)
        .map(|index| {
            source(
                &format!("cell{index}.png"),
                0x7300 + index,
                4,
                2,
                [index as u8, 34, 56, 255],
            )
        })
        .collect();
    let mut core = sequence_render_core(cells);
    let mut snapshots = Vec::new();
    for index in 0..8 {
        core.sequence_activate(index).unwrap();
        snapshots.push(core.build_snapshot());
    }
    let charged = core.resource_usage();
    assert_eq!(charged.sequence_render_cache_source_count, 8);
    core.sequence_activate(8).unwrap();
    let fallback = core.build_snapshot();
    assert_eq!(&fallback.tiles()[0].pixels()[..4], &[56, 34, 9, 255]);
    assert_eq!(
        core.resource_usage().sequence_render_cache_bytes,
        charged.sequence_render_cache_bytes
    );
    assert_eq!(core.resource_usage().sequence_render_cache_source_count, 8);
    drop(snapshots.remove(0));
    core.sequence_activate(9).unwrap();
    let admitted = core.build_snapshot();
    assert_eq!(&admitted.tiles()[0].pixels()[..4], &[56, 34, 10, 255]);
    assert_eq!(core.resource_usage().sequence_render_cache_source_count, 8);
    core.sequence_activate(9).unwrap();
    assert_eq!(
        core.build_snapshot().tiles()[0].pixels().as_ptr(),
        admitted.tiles()[0].pixels().as_ptr()
    );
}

#[test]
fn sequence_switch_preserves_same_size_views_and_applies_existing_resize_modes() {
    for command in [
        ViewCommand::Fit {
            viewport_width: 40.0,
            viewport_height: 30.0,
        },
        ViewCommand::OneToOne {
            viewport_width: 40.0,
            viewport_height: 30.0,
        },
        ViewCommand::ZoomAt {
            factor: 2.0,
            device_x: 3.0,
            device_y: 4.0,
        },
    ] {
        let mut core = sequence_render_core(vec![
            source("cell1.png", 0x7401, 4, 2, [12, 34, 56, 255]),
            source("cell2.png", 0x7402, 4, 2, [78, 90, 12, 255]),
            source("cell3.png", 0x7403, 2, 4, [98, 76, 54, 255]),
        ]);
        core.apply_view(command).unwrap();
        core.apply_view(ViewCommand::Flip {
            axis: MirrorAxis::Horizontal,
        })
        .unwrap();
        let secondary = core.create_view().unwrap();
        let before = core.view_state();
        let secondary_before = core.build_snapshot_for(secondary).unwrap().view();
        core.sequence_activate(1).unwrap();
        assert_eq!(core.build_snapshot().view(), before);
        assert_eq!(
            core.build_snapshot_for(secondary).unwrap().view(),
            secondary_before
        );
        core.sequence_activate(2).unwrap();
        let resized = core.build_snapshot().view();
        assert_eq!(resized.mode(), before.mode());
        assert_eq!(resized.flip_horizontal(), before.flip_horizontal());
        match before.mode() {
            ViewMode::Fit => {
                assert_eq!(resized.zoom(), 7.125);
                assert_eq!((resized.pan_x(), resized.pan_y()), (12.875, 0.75));
            }
            ViewMode::OneToOne => {
                assert_eq!(resized.zoom(), 1.0);
                assert_eq!((resized.pan_x(), resized.pan_y()), (19.0, 13.0));
            }
            ViewMode::Manual => assert_eq!(resized, before),
        }
    }
}

#[test]
fn sequence_render_prefetch_uses_two_neighbors_and_never_waits_for_a_busy_worker() {
    let manager = inkpod_io::IoManager::new(inkpod_io::IoConfig {
        worker_count: 1,
        queue_capacity: 8,
        ..inkpod_io::IoConfig::default()
    })
    .unwrap();
    let (release, receiver) = std::sync::mpsc::sync_channel::<()>(1);
    let blocker = manager
        .submit(move |_| {
            receiver
                .recv_timeout(std::time::Duration::from_secs(20))
                .map_err(|_| inkpod_io::IoError::ResourceBusy("test worker was not released"))
        })
        .unwrap();
    let mut core = sequence_render_core(vec![
        source("cell1.png", 0x7501, 4, 2, [12, 34, 56, 255]),
        source("cell2.png", 0x7502, 4, 2, [78, 90, 12, 255]),
        source("cell3.png", 0x7503, 4, 2, [98, 76, 54, 255]),
    ]);
    core.sequence_activate(1).unwrap();
    core.bind_file_io(manager.clone()).unwrap();
    let center = core.build_snapshot();
    assert_eq!(core.resource_usage().sequence_render_cache_source_count, 3);
    assert_eq!(core.resource_usage().sequence_render_cache_tile_count, 1);
    core.sequence_activate(2).unwrap();
    let foreground = core.build_snapshot();
    assert_eq!(&foreground.tiles()[0].pixels()[..4], &[54, 76, 98, 255]);
    assert_ne!(
        foreground.sequence_render_source(),
        center.sequence_render_source()
    );
    assert!(
        blocker.try_take().is_none(),
        "foreground waited for the blocked worker"
    );
    release.send(()).unwrap();

    // Poll while a new neighbor can finish, then use a FIFO barrier instead of
    // a timing threshold to prove that its result must be ready or adopted.
    core.sequence_activate(1).unwrap();
    for _ in 0..32 {
        let _ = core.build_snapshot();
    }
    let barrier = manager.submit(|_| Ok(())).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if let Some(result) = barrier.try_take() {
            result.unwrap();
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "sequence preparation stalled"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let _ = core.build_snapshot();
    let prepared = core.resource_usage();
    assert_eq!(prepared.sequence_render_cache_source_count, 3);
    assert_eq!(prepared.sequence_render_cache_tile_count, 3);
    core.sequence_activate(0).unwrap();
    let neighbor = core.build_snapshot();
    assert_eq!(&neighbor.tiles()[0].pixels()[..4], &[56, 34, 12, 255]);
    assert_eq!(
        core.resource_usage().sequence_render_cache_source_count,
        prepared.sequence_render_cache_source_count
    );
    manager.shutdown_and_wait();
    assert_eq!(core.resource_usage().sequence_render_cache_source_count, 3);
}

#[test]
fn sequence_render_prefetch_completed_for_old_catalog_cannot_replace_new_pixels() {
    let manager = inkpod_io::IoManager::new(inkpod_io::IoConfig {
        worker_count: 1,
        ..inkpod_io::IoConfig::default()
    })
    .unwrap();
    let mut core = sequence_render_core(vec![
        source("cell1.png", 0x7601, 4, 2, [12, 34, 56, 255]),
        source("cell2.png", 0x7602, 4, 2, [78, 90, 12, 255]),
    ]);
    core.bind_file_io(manager.clone()).unwrap();
    let original = core.build_snapshot();
    let barrier = manager.submit(|_| Ok(())).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if let Some(result) = barrier.try_take() {
            result.unwrap();
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "sequence preparation stalled"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    core.set_sequence(vec![
        source("cell1.png", 0x7601, 4, 2, [12, 34, 56, 255]),
        source("cell2.png", 0x7602, 4, 2, [200, 100, 50, 255]),
    ])
    .unwrap();
    core.sequence_activate(1).unwrap();
    let new_catalog = core.build_snapshot();
    assert_ne!(
        new_catalog
            .sequence_render_source()
            .unwrap()
            .owner_generation,
        original.sequence_render_source().unwrap().owner_generation
    );
    assert_eq!(&new_catalog.tiles()[0].pixels()[..4], &[50, 100, 200, 255]);
    manager.shutdown_and_wait();
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
    let editor_before_swap = reopened.editor_state().unwrap();
    let swapped = reopened
        .light_table_swap_with_active(before_swap[0].id)
        .unwrap();
    assert_eq!(swapped.document_uuid, 0x1111);
    assert_eq!((swapped.width, swapped.height), (4, 4));
    let editor_after_swap = reopened.editor_state().unwrap();
    assert_eq!(
        editor_after_swap.revision.get(),
        editor_before_swap.revision.get() + 1
    );
    assert_eq!(
        editor_after_swap.state.without_target(),
        editor_before_swap.state.without_target()
    );
    assert_eq!(
        editor_after_swap.state.target,
        Some(EditorTarget {
            layer_id: swapped.layer_id,
            plane_id: swapped.main_plane_id,
        })
    );
    assert!(editor_after_swap.dirty);
    assert!(swapped.dirty);
    let after_swap = reopened.light_table_items().unwrap();
    assert_eq!(after_swap[0].id, before_swap[0].id);
    assert_eq!(after_swap[0].opacity_milli, before_swap[0].opacity_milli);
    assert_eq!(after_swap[0].source_document_uuid, old_uuid);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn light_table_swap_editor_overflow_is_atomic_and_does_not_consume_stable_ids() {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let source = LightTableSource::from_common_raster(
        0x1212,
        1,
        RectI32 {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
        &rgba8(2, 2, [10, 20, 30, 255].repeat(4)),
    )
    .unwrap();
    let (_, item_id) = core
        .light_table_add_item(LightTableItemInput::new("swap overflow", source))
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "inkpod-test-light-table-editor-overflow-{}.inkpod",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    core.save(&path).unwrap();

    let mut maximum_revision = core.editor_state_frame().unwrap();
    maximum_revision[44..52].copy_from_slice(&u64::MAX.to_le_bytes());
    core.restore_editor_state_frame(&maximum_revision, EditorFrameDisposition::Saved)
        .unwrap();
    let document_before = core.document_info().unwrap();
    let digest_before = core.document_state_digest().unwrap();
    let editor_before = core.editor_state().unwrap();
    let layers_before = core.layers().unwrap();
    let items_before = core.light_table_items().unwrap();
    let history_before = core.history_entries().to_vec();
    let journal_before = core.journal_entries().to_vec();
    let snapshot_before = core.build_snapshot();
    let resources_before = core.resource_usage();

    assert!(matches!(
        core.light_table_swap_with_active(item_id),
        Err(CoreError::InvalidState("editor revision overflow"))
    ));
    assert_eq!(core.document_info().unwrap(), document_before);
    assert_eq!(core.document_state_digest().unwrap(), digest_before);
    assert_eq!(core.editor_state().unwrap(), editor_before);
    assert_eq!(core.layers().unwrap(), layers_before);
    assert_eq!(core.light_table_items().unwrap(), items_before);
    assert_eq!(core.history_entries(), history_before);
    assert_eq!(core.journal_entries(), journal_before);
    assert_eq!(core.build_snapshot(), snapshot_before);
    assert_eq!(core.resource_usage(), resources_before);

    let (_, guide_id) = core.add_guide(GuideAxis::Vertical, 1).unwrap();
    assert_eq!(guide_id, item_id + 2);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn light_table_swap_rejects_document_dirty_atomically() {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let source = LightTableSource::from_common_raster(
        0x1313,
        1,
        RectI32 {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
        &rgba8(2, 2, [20, 30, 40, 255].repeat(4)),
    )
    .unwrap();
    let (_, item_id) = core
        .light_table_add_item(LightTableItemInput::new("document dirty", source))
        .unwrap();
    assert!(core.document_info().unwrap().dirty);
    assert!(!core.editor_state().unwrap().dirty);

    let document_before = core.document_info().unwrap();
    let digest_before = core.document_state_digest().unwrap();
    let editor_before = core.editor_state().unwrap();
    let layers_before = core.layers().unwrap();
    let items_before = core.light_table_items().unwrap();
    let history_before = core.history_entries().to_vec();
    let journal_before = core.journal_entries().to_vec();
    let snapshot_before = core.build_snapshot();
    let resources_before = core.resource_usage();

    assert_eq!(
        core.light_table_swap_with_active(item_id),
        Err(CoreError::UnsavedChanges)
    );
    assert_eq!(core.document_info().unwrap(), document_before);
    assert_eq!(core.document_state_digest().unwrap(), digest_before);
    assert_eq!(core.editor_state().unwrap(), editor_before);
    assert_eq!(core.layers().unwrap(), layers_before);
    assert_eq!(core.light_table_items().unwrap(), items_before);
    assert_eq!(core.history_entries(), history_before);
    assert_eq!(core.journal_entries(), journal_before);
    assert_eq!(core.build_snapshot(), snapshot_before);
    assert_eq!(core.resource_usage(), resources_before);

    let (_, guide_id) = core.add_guide(GuideAxis::Vertical, 1).unwrap();
    assert_eq!(guide_id, item_id + 2);
}

#[test]
fn light_table_swap_accepts_saved_editor_state() {
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let source = LightTableSource::from_common_raster(
        0x1414,
        1,
        RectI32 {
            x: 0,
            y: 0,
            width: 3,
            height: 2,
        },
        &rgba8(3, 2, [50, 60, 70, 255].repeat(6)),
    )
    .unwrap();
    let (_, item_id) = core
        .light_table_add_item(LightTableItemInput::new("editor dirty", source))
        .unwrap();
    let initial_editor = core.editor_state().unwrap();
    let changed_editor = core
        .update_editor_state(
            initial_editor.revision,
            EditorStateUpdate::SetToolDiameter {
                tool: EditorTool::Brush,
                diameter_q16: 11_i64 << 16,
            },
        )
        .unwrap();
    assert!(changed_editor.dirty);

    let path = std::env::temp_dir().join(format!(
        "inkpod-test-light-table-editor-dirty-{}-{}.inkpod",
        std::process::id(),
        item_id
    ));
    let _ = std::fs::remove_file(&path);
    core.save(&path).unwrap();
    let before_swap = core.editor_state().unwrap();
    assert_eq!(before_swap.revision, changed_editor.revision);
    assert_eq!(before_swap.digest, changed_editor.digest);
    assert_eq!(before_swap.state, changed_editor.state);
    assert!(!before_swap.dirty);
    assert!(!core.document_info().unwrap().dirty);

    let swapped = core.light_table_swap_with_active(item_id).unwrap();
    assert_eq!(swapped.document_uuid, 0x1414);
    assert_eq!((swapped.width, swapped.height), (3, 2));
    let after_swap = core.editor_state().unwrap();
    assert_eq!(after_swap.revision.get(), before_swap.revision.get() + 1);
    assert_eq!(
        after_swap.state.without_target(),
        before_swap.state.without_target()
    );
    assert_eq!(
        after_swap.state.target,
        Some(EditorTarget {
            layer_id: swapped.layer_id,
            plane_id: swapped.main_plane_id,
        })
    );
    assert!(after_swap.dirty);
    assert!(swapped.dirty);

    let token = core.editor_savepoint_token().unwrap();
    let editor_clean = core.commit_editor_savepoint(token).unwrap();
    assert!(!editor_clean.dirty);
    assert!(
        !core.document_info().unwrap().dirty,
        "swap establishes a clean document savepoint; only editor dirty kept the session dirty"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn light_table_sources_are_canonical_assets_and_failed_updates_publish_nothing() {
    let mut core = Core::new();
    core.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let pixels = [10, 20, 30, 40].repeat(4);
    let first_raster = rgba8(2, 2, pixels.clone());
    let second_raster = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(300_000),
        Some(300_000),
        pixels,
    )
    .unwrap();
    let frame = RectI32 {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    };
    let first = LightTableSource::from_common_raster(0x1001, 1, frame, &first_raster).unwrap();
    let (_, item_id) = core
        .light_table_add_item(LightTableItemInput::new("first", first))
        .unwrap();
    let first_asset = core.asset_infos()[0].id;
    assert_eq!(core.asset_store_usage().asset_count, 1);
    assert_eq!(
        core.asset_info(first_asset)
            .unwrap()
            .descriptor
            .logical_payload_length,
        16
    );

    let second = LightTableSource::from_common_raster(
        0x2002,
        99,
        RectI32 {
            x: 11,
            y: -7,
            width: 4,
            height: 5,
        },
        &second_raster,
    )
    .unwrap();
    core.light_table_add_item(LightTableItemInput::new("second", second))
        .unwrap();
    assert_eq!(core.asset_store_usage().asset_count, 1);
    assert_eq!(core.asset_infos()[0].id, first_asset);

    let distinct = LightTableSource::from_common_raster(
        0x3003,
        1,
        frame,
        &rgba8(2, 2, [90, 80, 70, 60].repeat(4)),
    )
    .unwrap();
    let before_failure = core.asset_store_usage();
    assert!(matches!(
        core.light_table_update_item(
            u64::MAX,
            LightTableItemInput::new("invalid target", distinct),
        ),
        Err(CoreError::InvalidArgument(
            "light-table item ID does not exist"
        ))
    ));
    assert_eq!(core.asset_store_usage(), before_failure);

    let replacement = LightTableSource::from_common_raster(
        0x3003,
        1,
        frame,
        &rgba8(2, 2, [90, 80, 70, 60].repeat(4)),
    )
    .unwrap();
    core.light_table_update_item(
        item_id,
        LightTableItemInput::new("replacement", replacement),
    )
    .unwrap();
    assert_eq!(core.asset_store_usage().asset_count, 2);
    core.undo().unwrap();
    core.collect_unreferenced_assets().unwrap();
    assert_eq!(
        core.asset_store_usage().asset_count,
        2,
        "the redo tail remains an asset-retention root"
    );
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
    assert!(core.journal_state().unwrap().is_complete());
    core.verify_journal_replay().unwrap();
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
fn sequence_activate_and_step_reject_document_dirty_without_discarding_it() {
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
    let digest_before = core.document_state_digest().unwrap();
    let editor_before = core.editor_state().unwrap();
    let layers_before = core.layers().unwrap();
    let history_before = core.history_entries().to_vec();
    let journal_before = core.journal_entries().to_vec();
    let snapshot_before = core.build_snapshot();
    let resources_before = core.resource_usage();
    assert!(!editor_before.dirty);
    assert_eq!(core.sequence_activate(1), Err(CoreError::UnsavedChanges));
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.document_state_digest().unwrap(), digest_before);
    assert_eq!(core.editor_state().unwrap(), editor_before);
    assert_eq!(core.layers().unwrap(), layers_before);
    assert_eq!(core.history_entries(), history_before);
    assert_eq!(core.journal_entries(), journal_before);
    assert_eq!(core.build_snapshot(), snapshot_before);
    assert_eq!(core.resource_usage(), resources_before);
    assert_eq!(
        core.sequence_step(SequenceDirection::Next, false),
        Err(CoreError::UnsavedChanges)
    );
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.document_state_digest().unwrap(), digest_before);
    assert_eq!(core.editor_state().unwrap(), editor_before);
    assert_eq!(core.layers().unwrap(), layers_before);
    assert_eq!(core.history_entries(), history_before);
    assert_eq!(core.journal_entries(), journal_before);
    assert_eq!(core.build_snapshot(), snapshot_before);
    assert_eq!(core.resource_usage(), resources_before);

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
    assert!(!switched.dirty);
    assert!(!core.editor_state().unwrap().dirty);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn acceptance_autosave_sequence_switch_restores_exact_dirty_native_state() {
    let mut core = Core::new();
    let current = core
        .new_cell(3, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_sequence(vec![
        source("cell1.png", current.document_uuid, 3, 2, [1, 2, 3, 255]),
        source("cell2.png", 0x4545, 2, 3, [4, 5, 6, 255]),
    ])
    .unwrap();
    let normal_path = std::env::temp_dir().join(format!(
        "inkpod-test-sequence-normal-{}-{}.inkpod",
        std::process::id(),
        current.document_revision
    ));
    let source_recovery_path = std::env::temp_dir().join(format!(
        "inkpod-test-sequence-source-recovery-{}-{}.inkpod",
        std::process::id(),
        current.document_revision
    ));
    let target_recovery_path = std::env::temp_dir().join(format!(
        "inkpod-test-sequence-target-recovery-{}-{}.inkpod",
        std::process::id(),
        current.document_revision
    ));
    for path in [&normal_path, &source_recovery_path, &target_recovery_path] {
        let _ = std::fs::remove_file(path);
    }
    core.save(&normal_path).unwrap();
    let normal_bytes = std::fs::read(&normal_path).unwrap();
    core.update_editor_state(
        core.editor_state().unwrap().revision,
        EditorStateUpdate::SetToolDiameter {
            tool: EditorTool::Brush,
            diameter_q16: 13_i64 << 16,
        },
    )
    .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let source_before = core.document_info().unwrap();
    let source_digest = core.document_state_digest().unwrap();
    let source_history = core.history_entries().to_vec();
    let source_journal = core.journal_entries().to_vec();
    let source_editor = core.editor_state().unwrap();
    assert!(source_editor.dirty);
    let source_request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(source_request.requires_switch());
    assert_eq!(source_request.source_document_uuid, current.document_uuid);
    assert_eq!(source_request.target_document_uuid, 0x4545);

    core.autosave(&source_recovery_path).unwrap();
    assert_eq!(core.document_info().unwrap(), source_before);
    assert_eq!(std::fs::read(&normal_path).unwrap(), normal_bytes);
    let switched = core
        .sequence_commit_autosaved_switch(source_request)
        .unwrap();
    assert_eq!(switched.document_uuid, 0x4545);
    assert!(!switched.dirty);
    let target_editor = core.editor_state().unwrap();
    assert!(!target_editor.dirty);
    assert_eq!(
        target_editor.state.without_target(),
        source_editor.state.without_target()
    );

    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 0.0,
        y: 1.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let target_request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    core.autosave(&target_recovery_path).unwrap();
    let target_before_failed_restore = core.document_info().unwrap();
    let target_digest_before_failed_restore = core.document_state_digest().unwrap();
    let target_history_before_failed_restore = core.history_entries().to_vec();
    assert!(matches!(
        core.sequence_restore_autosaved_switch(target_request, &target_recovery_path),
        Err(CoreError::InvalidArgument(
            "recovery artifact does not match the sequence target"
        ))
    ));
    assert_eq!(core.document_info().unwrap(), target_before_failed_restore);
    assert_eq!(
        core.document_state_digest().unwrap(),
        target_digest_before_failed_restore
    );
    assert_eq!(core.history_entries(), target_history_before_failed_restore);
    let restored = core
        .sequence_restore_autosaved_switch(target_request, &source_recovery_path)
        .unwrap();
    assert_eq!(restored.document_uuid, current.document_uuid);
    assert!(restored.dirty);
    assert!(restored.recovered);
    assert_eq!(
        restored.main_plane_checksum,
        source_before.main_plane_checksum
    );
    assert_eq!(
        restored.color_plane_checksum,
        source_before.color_plane_checksum
    );
    assert_eq!(core.document_state_digest().unwrap(), source_digest);
    assert_eq!(core.history_entries(), source_history);
    assert_eq!(core.journal_entries(), source_journal);
    assert_eq!(core.editor_state().unwrap().state, source_editor.state);
    assert!(core.editor_state().unwrap().dirty);
    assert_eq!(core.sequence_cells().unwrap().len(), 2);
    let restored_digest = core.document_state_digest().unwrap();
    core.undo().unwrap();
    assert_ne!(core.document_state_digest().unwrap(), restored_digest);
    core.redo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), restored_digest);
    assert_eq!(std::fs::read(&normal_path).unwrap(), normal_bytes);

    for path in [normal_path, source_recovery_path, target_recovery_path] {
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn detached_sequence_switch_preserves_recovery_history_formats_and_view_owners() {
    let mut core = Core::new();
    core.set_new_cell_raster_format(CommonRasterFormat::Tiff);
    let current = core
        .new_cell(3, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let mut second = source("cell2.bmp", 0x7474, 2, 3, [4, 5, 6, 255]);
    second.raster_file_format = CommonRasterFormat::Bmp;
    core.set_sequence(vec![
        source("cell1.tiff", current.document_uuid, 3, 2, [1, 2, 3, 255]),
        second,
    ])
    .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let source_info = core.document_info().unwrap();
    let source_digest = core.document_state_digest().unwrap();
    let source_history = core.history_entries().to_vec();
    let source_journal = core.journal_entries().to_vec();
    let source_editor = core.editor_state().unwrap();
    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let captured = core.capture_sequence_switch(request).unwrap();
    let mut prepared = std::thread::spawn(move || captured.prepare(None, || false))
        .join()
        .unwrap()
        .unwrap();
    let recovery = prepared.take_source_recovery().unwrap();
    assert!(prepared.take_source_recovery().is_none());
    assert_eq!(core.document_info().unwrap(), source_info);
    let recovered_source = Core::from_native_file(recovery.clone(), true).unwrap();
    assert_eq!(
        recovered_source.document_state_digest().unwrap(),
        source_digest
    );
    assert_eq!(recovered_source.history_entries(), source_history);
    assert_eq!(
        recovered_source.raster_file_format().unwrap(),
        CommonRasterFormat::Tiff
    );

    // Independent view creation and reference selection after capture are kept.
    let view = core.create_view().unwrap();
    let view_state = core
        .apply_view_for(
            view,
            ViewCommand::ZoomAt {
                factor: 2.0,
                device_x: 0.0,
                device_y: 0.0,
            },
        )
        .unwrap();
    core.set_subpalette_cell(1).unwrap();
    let reference_pixel = core.subpalette_sample(0, 0).unwrap();
    core.validate_prepared_sequence_switch(&prepared).unwrap();
    let switched = core.commit_prepared_sequence_switch(prepared).unwrap();
    assert_eq!(switched.document_uuid, 0x7474);
    assert!(!switched.dirty);
    assert!(!core.editor_state().unwrap().dirty);
    assert_eq!(core.raster_file_format().unwrap(), CommonRasterFormat::Bmp);
    assert_eq!(core.build_snapshot_for(view).unwrap().view(), view_state);
    assert_eq!(core.subpalette_sample(0, 0).unwrap(), reference_pixel);

    let request = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let before_invalid = core.document_info().unwrap();
    let wrong_recovery = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(true, || false)
        .unwrap()
        .0;
    assert!(matches!(
        core.capture_sequence_switch(request)
            .unwrap()
            .prepare(Some(wrong_recovery), || false),
        Err(CoreError::InvalidArgument(
            "recovery artifact does not match the sequence target"
        ))
    ));
    assert_eq!(core.document_info().unwrap(), before_invalid);
    let mut prepared = core
        .capture_sequence_switch(request)
        .unwrap()
        .prepare(Some(recovery), || false)
        .unwrap();
    prepared.take_source_recovery().unwrap();
    let restored = core.commit_prepared_sequence_switch(prepared).unwrap();
    assert!(restored.recovered && restored.dirty);
    assert_eq!(restored.document_uuid, current.document_uuid);
    assert_eq!(core.raster_file_format().unwrap(), CommonRasterFormat::Tiff);
    assert_eq!(core.document_state_digest().unwrap(), source_digest);
    assert_eq!(core.history_entries(), source_history);
    assert_eq!(core.journal_entries(), source_journal);
    assert_eq!(core.editor_state().unwrap().state, source_editor.state);
    core.undo().unwrap();
    core.redo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), source_digest);
    assert_eq!(core.sequence_cells().unwrap().len(), 2);
    assert!(core.build_snapshot_for(view).is_ok());
}

#[test]
fn detached_sequence_switch_rejects_cancel_stale_owner_and_replaced_sequence() {
    let mut core = Core::new();
    let current = core
        .new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let cells = vec![
        source("cell1.png", current.document_uuid, 2, 2, [1, 2, 3, 255]),
        source("cell2.png", 0x7575, 2, 2, [4, 5, 6, 255]),
    ];
    core.set_sequence(cells.clone()).unwrap();
    let same = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    let save = core.capture_document_save().unwrap();
    let (_, authority) = save.prepare_native_save(true, || false).unwrap();
    let before = core.document_info().unwrap();
    let mut noop = core
        .capture_sequence_switch(same)
        .unwrap()
        .prepare(None, || false)
        .unwrap();
    assert!(noop.take_source_recovery().is_none());
    assert_eq!(core.commit_prepared_sequence_switch(noop).unwrap(), before);
    core.validate_document_save(&authority).unwrap();

    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    for cancel_at in [1, 2, 3] {
        let mut calls = 0;
        assert!(matches!(
            core.capture_sequence_switch(request)
                .unwrap()
                .prepare(None, || {
                    calls += 1;
                    calls == cancel_at
                }),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(core.document_info().unwrap(), before);
        core.validate_document_save(&authority).unwrap();
    }
    let prepared = core
        .capture_sequence_switch(request)
        .unwrap()
        .prepare(None, || false)
        .unwrap();
    let mut unrelated = Core::new();
    unrelated
        .new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    assert!(
        unrelated
            .validate_prepared_sequence_switch(&prepared)
            .is_err()
    );
    core.set_sequence(cells).unwrap();
    assert!(matches!(
        core.commit_prepared_sequence_switch(prepared),
        Err(CoreError::InvalidState("sequence switch request is stale"))
    ));
    assert_eq!(core.document_info().unwrap(), before);

    let prepared = core
        .capture_sequence_switch(request)
        .unwrap()
        .prepare(None, || false)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 0.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let edited = core.document_info().unwrap();
    let journal = core.journal_entries().to_vec();
    assert!(core.commit_prepared_sequence_switch(prepared).is_err());
    assert_eq!(core.document_info().unwrap(), edited);
    assert_eq!(core.journal_entries(), journal);
}

#[test]
fn autosave_sequence_switch_request_is_noop_or_stale_without_partial_change() {
    let mut core = Core::new();
    let current = core
        .new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_sequence(vec![
        source("cell1.png", current.document_uuid, 2, 2, [1, 2, 3, 255]),
        source("cell2.png", 0x4646, 2, 2, [4, 5, 6, 255]),
    ])
    .unwrap();
    let no_op = core
        .sequence_switch_request(0, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    assert!(!no_op.requires_switch());
    let before_no_op = core.document_info().unwrap();
    assert_eq!(
        core.sequence_commit_autosaved_switch(no_op).unwrap(),
        before_no_op
    );
    assert!(matches!(
        core.sequence_switch_request(usize::MAX, SequenceSwitchPolicy::AutosaveBeforeSwitch),
        Err(CoreError::InvalidArgument(_))
    ));

    let request = core
        .sequence_switch_request(1, SequenceSwitchPolicy::AutosaveBeforeSwitch)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 0.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let document_before = core.document_info().unwrap();
    let digest_before = core.document_state_digest().unwrap();
    let editor_before = core.editor_state().unwrap();
    let history_before = core.history_entries().to_vec();
    let journal_before = core.journal_entries().to_vec();
    let snapshot_before = core.build_snapshot();
    assert_eq!(
        core.sequence_commit_autosaved_switch(request),
        Err(CoreError::InvalidState("sequence switch request is stale"))
    );
    assert_eq!(core.document_info().unwrap(), document_before);
    assert_eq!(core.document_state_digest().unwrap(), digest_before);
    assert_eq!(core.editor_state().unwrap(), editor_before);
    assert_eq!(core.history_entries(), history_before);
    assert_eq!(core.journal_entries(), journal_before);
    assert_eq!(core.build_snapshot(), snapshot_before);
}

#[test]
fn sequence_activation_plan_revalidates_unbound_and_bound_source_identities() {
    let mut core = Core::new();
    let initial = core
        .new_cell_with_uuid(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x8171)
        .unwrap();
    core.set_sequence(vec![
        source("cell1.png", 0x8172, 2, 2, [1, 2, 3, 255]),
        source("cell2.png", 0x8173, 2, 2, [1, 2, 3, 255]),
    ])
    .unwrap();
    let plan = core.resolve_sequence_activation(0).unwrap();
    assert_eq!(plan.kind, SequenceActivationKind::Replace);
    assert_eq!(plan.source_index, None);
    assert_eq!(plan.source_generation, None);
    assert_eq!(plan.source_document_uuid, initial.document_uuid);
    assert_eq!(plan.target_document_uuid, 0x8172);
    let mut forged = plan;
    forged.kind = SequenceActivationKind::Bind;
    assert_eq!(
        core.commit_sequence_activation(forged),
        Err(CoreError::InvalidState("sequence activation plan is stale"))
    );
    assert_eq!(core.document_info().unwrap(), initial);
    let first = core.commit_sequence_activation(plan).unwrap();
    assert_eq!(first.document_uuid, 0x8172);
    assert!(first.document_revision > initial.document_revision);

    let (_, guide) = core.add_guide(GuideAxis::Vertical, 1).unwrap();
    let dirty = core.document_info().unwrap();
    let no_op = core.resolve_sequence_activation(0).unwrap();
    assert_eq!(no_op.kind, SequenceActivationKind::NoOp);
    assert_eq!(core.commit_sequence_activation(no_op).unwrap(), dirty);
    assert_eq!(core.sequence_activate(0).unwrap(), dirty);
    let replacement = core.resolve_sequence_activation(1).unwrap();
    assert_eq!(replacement.kind, SequenceActivationKind::Replace);
    assert_eq!(replacement.source_generation, Some(1));
    assert_eq!(
        core.commit_sequence_activation(replacement),
        Err(CoreError::UnsavedChanges)
    );
    for source_generation in [true, false] {
        let mut forged = replacement;
        if source_generation {
            forged.source_generation = Some(2);
        } else {
            forged.target_source_generation = 2;
        }
        assert_eq!(
            core.commit_sequence_activation(forged),
            Err(CoreError::InvalidState("sequence activation plan is stale"))
        );
    }
    assert!(matches!(
        core.resolve_sequence_activation(usize::MAX),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), dirty);
    core.delete_guide(guide).unwrap();
    assert_eq!(
        core.commit_sequence_activation(replacement),
        Err(CoreError::InvalidState("sequence activation plan is stale"))
    );
}

#[test]
fn sequence_activate_and_step_start_clean_with_inherited_editor_settings() {
    let mut core = Core::new();
    let current = core
        .new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_sequence(vec![
        source("cell1.png", current.document_uuid, 2, 2, [1, 2, 3, 255]),
        source("cell2.png", 0x5555, 3, 2, [4, 5, 6, 255]),
        source("cell3.png", 0x6666, 4, 2, [7, 8, 9, 255]),
    ])
    .unwrap();
    let initial_editor = core.editor_state().unwrap();
    let changed_editor = core
        .update_editor_state(
            initial_editor.revision,
            EditorStateUpdate::SetToolDiameter {
                tool: EditorTool::Brush,
                diameter_q16: 13_i64 << 16,
            },
        )
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "inkpod-test-sequence-editor-dirty-{}-{}.inkpod",
        std::process::id(),
        current.document_revision
    ));
    let _ = std::fs::remove_file(&path);
    core.save(&path).unwrap();
    let before_activate = core.editor_state().unwrap();
    assert_eq!(before_activate.revision, changed_editor.revision);
    assert_eq!(before_activate.digest, changed_editor.digest);
    assert_eq!(before_activate.state, changed_editor.state);
    assert!(!before_activate.dirty);
    assert!(!core.document_info().unwrap().dirty);

    let activated = core.sequence_activate(1).unwrap();
    assert_eq!(activated.document_uuid, 0x5555);
    assert_eq!((activated.width, activated.height), (3, 2));
    let after_activate = core.editor_state().unwrap();
    assert_eq!(
        after_activate.revision.get(),
        before_activate.revision.get() + 1
    );
    assert_eq!(
        after_activate.state.without_target(),
        before_activate.state.without_target()
    );
    assert_eq!(
        after_activate.state.target,
        Some(EditorTarget {
            layer_id: activated.layer_id,
            plane_id: activated.color_plane_id,
        })
    );
    assert!(!after_activate.dirty);
    assert!(!activated.dirty);

    let stepped = core.sequence_step(SequenceDirection::Next, false).unwrap();
    assert_eq!(stepped.document_uuid, 0x6666);
    assert_eq!((stepped.width, stepped.height), (4, 2));
    let after_step = core.editor_state().unwrap();
    assert_eq!(after_step.revision.get(), after_activate.revision.get() + 1);
    assert_eq!(
        after_step.state.without_target(),
        after_activate.state.without_target()
    );
    assert_eq!(
        after_step.state.target,
        Some(EditorTarget {
            layer_id: stepped.layer_id,
            plane_id: stepped.color_plane_id,
        })
    );
    assert!(!after_step.dirty);
    assert!(!stepped.dirty);
    assert!(!stepped.can_undo && !stepped.can_redo);
    assert!(core.history_entries().is_empty());
    assert!(core.journal_entries().is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn sequence_activation_failure_and_editor_overflow_are_atomic() {
    let mut core = Core::new();
    let current = core
        .new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_sequence(vec![
        source("cell1.png", current.document_uuid, 2, 2, [1, 2, 3, 255]),
        source("cell2.png", 0x7777, 3, 2, [4, 5, 6, 255]),
    ])
    .unwrap();
    let (_, prior_guide_id) = core.add_guide(GuideAxis::Vertical, 1).unwrap();
    let path = std::env::temp_dir().join(format!(
        "inkpod-test-sequence-editor-overflow-{}-{}.inkpod",
        std::process::id(),
        current.document_revision
    ));
    let _ = std::fs::remove_file(&path);
    core.save(&path).unwrap();

    let invalid_before = core.document_info().unwrap();
    let invalid_editor_before = core.editor_state().unwrap();
    assert!(matches!(
        core.sequence_activate(usize::MAX),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), invalid_before);
    assert_eq!(core.editor_state().unwrap(), invalid_editor_before);

    let normal_editor_frame = core.editor_state_frame().unwrap();
    let mut maximum_revision = normal_editor_frame.clone();
    maximum_revision[44..52].copy_from_slice(&u64::MAX.to_le_bytes());
    core.restore_editor_state_frame(&maximum_revision, EditorFrameDisposition::Saved)
        .unwrap();
    let document_before = core.document_info().unwrap();
    let digest_before = core.document_state_digest().unwrap();
    let editor_before = core.editor_state().unwrap();
    let layers_before = core.layers().unwrap();
    let history_before = core.history_entries().to_vec();
    let journal_before = core.journal_entries().to_vec();
    let snapshot_before = core.build_snapshot();
    let resources_before = core.resource_usage();

    assert!(matches!(
        core.sequence_activate(1),
        Err(CoreError::InvalidState("editor revision overflow"))
    ));
    assert_eq!(core.document_info().unwrap(), document_before);
    assert_eq!(core.document_state_digest().unwrap(), digest_before);
    assert_eq!(core.editor_state().unwrap(), editor_before);
    assert_eq!(core.layers().unwrap(), layers_before);
    assert_eq!(core.history_entries(), history_before);
    assert_eq!(core.journal_entries(), journal_before);
    assert_eq!(core.build_snapshot(), snapshot_before);
    assert_eq!(core.resource_usage(), resources_before);

    let (_, next_guide_id) = core.add_guide(GuideAxis::Horizontal, 1).unwrap();
    assert_eq!(next_guide_id, prior_guide_id + 1);
    core.save(&path).unwrap();
    core.restore_editor_state_frame(&normal_editor_frame, EditorFrameDisposition::Saved)
        .unwrap();
    let switched = core.sequence_step(SequenceDirection::Next, false).unwrap();
    assert_eq!(
        switched.document_uuid, 0x7777,
        "failed activation must not advance the sequence active index"
    );
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
fn seq_001_endpoint_policy_plans_empty_single_stop_wrap_and_gaps_without_document_mutation() {
    let empty = Core::new();
    let empty_plan = empty
        .resolve_sequence_step(SequenceDirection::Next, SequenceEndpointPolicy::Stop)
        .unwrap();
    assert_eq!(empty_plan.result, SequenceStepResult::Empty);
    assert!(!empty_plan.requires_switch());

    let mut core = Core::new();
    let first = core
        .new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_sequence(vec![source(
        "cell1.png",
        first.document_uuid,
        2,
        2,
        [1, 2, 3, 255],
    )])
    .unwrap();
    let single = core
        .resolve_sequence_step(SequenceDirection::Previous, SequenceEndpointPolicy::Wrap)
        .unwrap();
    assert_eq!(single.result, SequenceStepResult::SingleCell);
    assert_eq!(single.source_cell_number, Some(1));
    assert_eq!(single.target_cell_number, Some(1));
    assert!(!single.requires_switch());

    core.set_sequence(vec![
        source("cell1.png", first.document_uuid, 2, 2, [1, 2, 3, 255]),
        source("cell3.png", 0x3030, 2, 2, [3, 4, 5, 255]),
        source("cell10.png", 0x1010, 2, 2, [6, 7, 8, 255]),
    ])
    .unwrap();
    let document_before = core.document_info().unwrap();
    let editor_before = core.editor_state().unwrap();
    let history_before = core.history_entries().to_vec();
    let journal_before = core.journal_entries().to_vec();
    let snapshot_before = core.build_snapshot();

    let stopped = core
        .resolve_sequence_step(SequenceDirection::Previous, SequenceEndpointPolicy::Stop)
        .unwrap();
    assert_eq!(stopped.result, SequenceStepResult::Stopped);
    assert_eq!(stopped.source_index, Some(0));
    assert_eq!(stopped.target_index, Some(0));
    assert!(!stopped.requires_switch());
    assert_eq!(core.document_info().unwrap(), document_before);
    assert_eq!(core.editor_state().unwrap(), editor_before);
    assert_eq!(core.history_entries(), history_before);
    assert_eq!(core.journal_entries(), journal_before);
    assert_eq!(core.build_snapshot(), snapshot_before);

    let wrapped = core
        .resolve_sequence_step(SequenceDirection::Previous, SequenceEndpointPolicy::Wrap)
        .unwrap();
    assert_eq!(wrapped.result, SequenceStepResult::Wrapped);
    assert_eq!(wrapped.source_cell_number, Some(1));
    assert_eq!(wrapped.target_cell_number, Some(10));
    let wrapped_info = core.commit_sequence_step(wrapped).unwrap();
    assert_eq!(wrapped_info.document_uuid, 0x1010);

    let previous = core
        .resolve_sequence_step(SequenceDirection::Previous, SequenceEndpointPolicy::Stop)
        .unwrap();
    assert_eq!(previous.result, SequenceStepResult::Advanced);
    assert_eq!(previous.source_cell_number, Some(10));
    assert_eq!(previous.target_cell_number, Some(3));
    core.commit_sequence_step(previous).unwrap();
    let gap = core
        .resolve_sequence_step(SequenceDirection::Previous, SequenceEndpointPolicy::Stop)
        .unwrap();
    assert_eq!(gap.result, SequenceStepResult::Advanced);
    assert_eq!(gap.source_cell_number, Some(3));
    assert_eq!(gap.target_cell_number, Some(1));
}

#[test]
fn seq_001_endpoint_step_rejects_stale_and_unsaved_requests_atomically() {
    let mut core = Core::new();
    let first = core
        .new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_sequence(vec![
        source("cell1.png", first.document_uuid, 2, 2, [1, 2, 3, 255]),
        source("cell2.png", 0x2020, 2, 2, [4, 5, 6, 255]),
    ])
    .unwrap();
    let stale = core
        .resolve_sequence_step(SequenceDirection::Next, SequenceEndpointPolicy::Wrap)
        .unwrap();
    core.set_sequence(vec![
        source("cell1.png", first.document_uuid, 2, 2, [1, 2, 3, 255]),
        source("cell2.png", 0x2020, 2, 2, [4, 5, 6, 255]),
    ])
    .unwrap();
    let before_stale = core.document_info().unwrap();
    assert_eq!(
        core.commit_sequence_step(stale),
        Err(CoreError::InvalidState("sequence step request is stale"))
    );
    assert_eq!(core.document_info().unwrap(), before_stale);

    let request = core
        .resolve_sequence_step(SequenceDirection::Next, SequenceEndpointPolicy::Stop)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 0.0,
        y: 0.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let document_before = core.document_info().unwrap();
    let digest_before = core.document_state_digest().unwrap();
    let history_before = core.history_entries().to_vec();
    let journal_before = core.journal_entries().to_vec();
    assert_eq!(
        core.commit_sequence_step(request),
        Err(CoreError::UnsavedChanges)
    );
    assert_eq!(core.document_info().unwrap(), document_before);
    assert_eq!(core.document_state_digest().unwrap(), digest_before);
    assert_eq!(core.history_entries(), history_before);
    assert_eq!(core.journal_entries(), journal_before);
}

#[test]
fn document_thumbnail_is_bounded_deterministic_and_query_only() {
    let mut core = Core::new();
    core.new_cell(128, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let before = core.document_info().unwrap();
    let history_before = core.history_entries();

    let thumbnail = core.document_thumbnail().unwrap();
    assert_eq!((thumbnail.width, thumbnail.height), (64, 32));
    assert_eq!(thumbnail.rgba8.len(), 64 * 32 * 4);
    assert_ne!(thumbnail.checksum, 0);
    assert_eq!(core.document_thumbnail().unwrap(), thumbnail);
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.history_entries(), history_before);
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
fn light_table_bulk_registration_previews_skips_and_commits_one_natural_order_edit() {
    let mut core = Core::new();
    core.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let current_uuid = core.document_info().unwrap().document_uuid;
    let cells = vec![
        source("cell1.png", 0x7101, 2, 2, [1, 0, 0, 255]),
        source("cell2.png", 0x7102, 2, 2, [2, 0, 0, 255]),
        source("cell3.png", 0x7103, 2, 2, [3, 0, 0, 255]),
        source("cell4.png", current_uuid, 2, 2, [4, 0, 0, 255]),
        source("cell5.png", 0x7105, 2, 2, [5, 0, 0, 255]),
        source("cell6.png", 0x7106, 2, 2, [6, 0, 0, 255]),
        source("cell7.png", 0x7107, 2, 2, [7, 0, 0, 255]),
    ];
    core.set_sequence(cells).unwrap();

    let target_set_id = core.light_table_sets().unwrap()[0].id;
    let existing_source = LightTableSource::from_common_raster(
        0x7102,
        99,
        RectI32 {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        },
        &rgba8(
            2,
            2,
            vec![20, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
    )
    .unwrap();
    let mut existing_input = LightTableItemInput::new("preserved cell2", existing_source);
    existing_input.opacity_milli = 321;
    existing_input.translate_x_milli = 1_250;
    let (_, existing_id) = core.light_table_add_item(existing_input).unwrap();

    let request = core
        .light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Both,
            3,
            800,
            200,
        )
        .unwrap();
    let before_preview = core.document_info().unwrap();
    let preview = core
        .preview_light_table_bulk_registration(&request)
        .unwrap();
    assert_eq!(core.document_info().unwrap(), before_preview);
    assert_eq!(preview.add_count, 5);
    assert_eq!(preview.skip_count, 1);
    assert_eq!(
        preview
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.cell_number,
                    entry.distance,
                    entry.opacity_milli,
                    entry.action,
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (7, 3, 400, LightTableBulkRegistrationAction::Add),
            (6, 2, 600, LightTableBulkRegistrationAction::Add),
            (5, 1, 800, LightTableBulkRegistrationAction::Add),
            (3, 1, 800, LightTableBulkRegistrationAction::Add),
            (2, 2, 600, LightTableBulkRegistrationAction::SkipExisting),
            (1, 3, 400, LightTableBulkRegistrationAction::Add),
        ]
    );

    let revision_before = before_preview.document_revision;
    let history_before = core.history_entries().len();
    let journal_before = core.journal_entries().len();
    let (outcome, summary) = core.light_table_bulk_register(request).unwrap();
    assert_eq!(outcome.revision(), revision_before + 1);
    assert_eq!(summary.added_item_ids.len(), 5);
    assert_eq!(summary.add_count, 5);
    assert_eq!(summary.skip_count, 1);
    assert_eq!(core.history_entries().len(), history_before + 1);
    assert_eq!(core.journal_entries().len(), journal_before + 1);
    let JournalEntry::Commit(commit) = core.journal_entries().last().unwrap() else {
        panic!("bulk registration must append one canonical commit");
    };
    assert_eq!(
        commit.procedure().primitive_id(),
        PrimitiveId::LIGHT_TABLE_BULK_REGISTER
    );
    assert_eq!(
        core.light_table_items()
            .unwrap()
            .iter()
            .map(|item| (item.source_document_uuid, item.opacity_milli))
            .collect::<Vec<_>>(),
        vec![
            (0x7107, 400),
            (0x7106, 600),
            (0x7105, 800),
            (0x7103, 800),
            (0x7101, 400),
            (0x7102, 321),
        ]
    );
    let preserved = core
        .light_table_items()
        .unwrap()
        .into_iter()
        .find(|item| item.id == existing_id)
        .unwrap();
    assert_eq!(preserved.source_revision, 99);
    assert_eq!(preserved.translate_x_milli, 1_250);
    for added in &core.light_table_items().unwrap()[..5] {
        assert!(added.visible);
        assert_eq!(added.display_mode, LightTableDisplayMode::Color);
        assert_eq!(added.translate_x_milli, 0);
        assert_eq!(added.translate_y_milli, 0);
        assert_eq!(added.scale_x_milli, 1_000);
        assert_eq!(added.scale_y_milli, 1_000);
        assert_eq!(added.rotation_milli_degrees, 0);
    }

    core.undo().unwrap();
    assert_eq!(
        core.light_table_items().unwrap()[0].source_document_uuid,
        0x7102
    );
    assert_eq!(core.light_table_items().unwrap().len(), 1);
    core.redo().unwrap();
    assert_eq!(core.light_table_items().unwrap().len(), 6);
    core.verify_journal_replay().unwrap();

    let path = std::env::temp_dir().join(format!(
        "inkpod-test-light-table-bulk-{}-{}.inkpod",
        std::process::id(),
        core.document_info().unwrap().document_revision
    ));
    let _ = std::fs::remove_file(&path);
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(
        reopened
            .light_table_items()
            .unwrap()
            .iter()
            .map(|item| (item.source_document_uuid, item.opacity_milli))
            .collect::<Vec<_>>(),
        vec![
            (0x7107, 400),
            (0x7106, 600),
            (0x7105, 800),
            (0x7103, 800),
            (0x7101, 400),
            (0x7102, 321),
        ]
    );
    reopened.verify_journal_replay().unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn light_table_bulk_registration_noop_invalid_cancel_and_stale_are_atomic() {
    let mut core = Core::new();
    core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let current_uuid = core.document_info().unwrap().document_uuid;
    core.set_sequence(vec![
        source("cell1.png", 0x7201, 1, 1, [1, 0, 0, 255]),
        source("cell2.png", current_uuid, 1, 1, [2, 0, 0, 255]),
        source("cell3.png", 0x7203, 1, 1, [3, 0, 0, 255]),
    ])
    .unwrap();
    let target_set_id = core.light_table_sets().unwrap()[0].id;

    assert!(matches!(
        core.light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Both,
            1,
            1_001,
            0,
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert!(matches!(
        core.light_table_bulk_registration_request(
            u64::MAX,
            LightTableBulkDirection::Both,
            1,
            1_000,
            0,
        ),
        Err(CoreError::InvalidArgument(_))
    ));

    let zero = core
        .light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Both,
            0,
            1_000,
            100,
        )
        .unwrap();
    assert!(
        core.preview_light_table_bulk_registration(&zero)
            .unwrap()
            .entries
            .is_empty()
    );
    let before_zero = core.document_info().unwrap();
    let (zero_outcome, zero_summary) = core.light_table_bulk_register(zero).unwrap();
    assert_eq!(zero_outcome.revision(), before_zero.document_revision);
    assert_eq!(zero_summary.add_count, 0);
    assert_eq!(core.document_info().unwrap(), before_zero);

    let cancelled = core
        .light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Both,
            1,
            900,
            100,
        )
        .unwrap();
    let before_cancel = core.document_info().unwrap();
    core.preview_light_table_bulk_registration(&cancelled)
        .unwrap();
    assert_eq!(core.document_info().unwrap(), before_cancel);

    let stale = cancelled;
    core.light_table_set_global_opacity(999).unwrap();
    let before_stale = core.document_info().unwrap();
    let items_before_stale = core.light_table_items().unwrap();
    assert!(matches!(
        core.light_table_bulk_register(stale),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(core.document_info().unwrap(), before_stale);
    assert_eq!(core.light_table_items().unwrap(), items_before_stale);

    let initial = core
        .light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Both,
            1,
            900,
            100,
        )
        .unwrap();
    core.light_table_bulk_register(initial).unwrap();
    let duplicate = core
        .light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Both,
            1,
            900,
            100,
        )
        .unwrap();
    let preview = core
        .preview_light_table_bulk_registration(&duplicate)
        .unwrap();
    assert_eq!(preview.add_count, 0);
    assert_eq!(preview.skip_count, 2);
    let before_duplicate = core.document_info().unwrap();
    let history_before = core.history_entries().len();
    let journal_before = core.journal_entries().len();
    let (outcome, summary) = core.light_table_bulk_register(duplicate).unwrap();
    assert_eq!(outcome.revision(), before_duplicate.document_revision);
    assert_eq!(summary.add_count, 0);
    assert_eq!(summary.skip_count, 2);
    assert_eq!(core.document_info().unwrap(), before_duplicate);
    assert_eq!(core.history_entries().len(), history_before);
    assert_eq!(core.journal_entries().len(), journal_before);
}

#[test]
fn light_table_bulk_registration_directions_edges_gaps_upper_bound_and_sequence_stale() {
    let mut core = Core::new();
    core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let current_uuid = core.document_info().unwrap().document_uuid;
    let target_set_id = core.light_table_sets().unwrap()[0].id;

    core.set_sequence(vec![source(
        "cell5.png",
        current_uuid,
        1,
        1,
        [5, 0, 0, 255],
    )])
    .unwrap();
    for direction in [
        LightTableBulkDirection::Previous,
        LightTableBulkDirection::Next,
        LightTableBulkDirection::Both,
    ] {
        let request = core
            .light_table_bulk_registration_request(target_set_id, direction, 10_000, 0, 1_000)
            .unwrap();
        let preview = core
            .preview_light_table_bulk_registration(&request)
            .unwrap();
        assert!(preview.entries.is_empty());
        let before = core.document_info().unwrap();
        assert_eq!(
            core.light_table_bulk_register(request)
                .unwrap()
                .0
                .revision(),
            before.document_revision
        );
        assert_eq!(core.document_info().unwrap(), before);
    }
    assert!(matches!(
        core.light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Both,
            10_001,
            1_000,
            0,
        ),
        Err(CoreError::InvalidArgument(_))
    ));

    let cells = vec![
        source("cell1.png", 0x7301, 1, 1, [1, 0, 0, 255]),
        source("cell3.png", current_uuid, 1, 1, [3, 0, 0, 255]),
        source("cell10.png", 0x7310, 1, 1, [10, 0, 0, 255]),
    ];
    core.set_sequence(cells.clone()).unwrap();
    let previous = core
        .light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Previous,
            1,
            1_000,
            1_000,
        )
        .unwrap();
    assert_eq!(
        core.preview_light_table_bulk_registration(&previous)
            .unwrap()
            .entries
            .iter()
            .map(|entry| (entry.cell_number, entry.opacity_milli))
            .collect::<Vec<_>>(),
        vec![(1, 1_000)]
    );
    let next = core
        .light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Next,
            1,
            1_000,
            1_000,
        )
        .unwrap();
    assert_eq!(
        core.preview_light_table_bulk_registration(&next)
            .unwrap()
            .entries
            .iter()
            .map(|entry| (entry.cell_number, entry.opacity_milli))
            .collect::<Vec<_>>(),
        vec![(10, 1_000)]
    );
    let both = core
        .light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Both,
            10_000,
            0,
            1_000,
        )
        .unwrap();
    assert_eq!(
        core.preview_light_table_bulk_registration(&both)
            .unwrap()
            .entries
            .iter()
            .map(|entry| (entry.cell_number, entry.distance, entry.opacity_milli))
            .collect::<Vec<_>>(),
        vec![(10, 1, 0), (1, 1, 0)]
    );

    let stale = next;
    let before_stale = core.document_info().unwrap();
    core.set_sequence(cells).unwrap();
    assert!(matches!(
        core.preview_light_table_bulk_registration(&stale),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(core.document_info().unwrap(), before_stale);

    core.set_sequence(vec![
        source("cell3.png", current_uuid, 1, 1, [3, 0, 0, 255]),
        source("cell10.png", 0x7310, 1, 1, [10, 0, 0, 255]),
    ])
    .unwrap();
    let endpoint = core
        .light_table_bulk_registration_request(
            target_set_id,
            LightTableBulkDirection::Previous,
            10_000,
            1_000,
            0,
        )
        .unwrap();
    assert!(
        core.preview_light_table_bulk_registration(&endpoint)
            .unwrap()
            .entries
            .is_empty()
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

#[test]
fn sequence_metadata_and_thumbnail_borrow_the_prepared_immutable_source() {
    let mut core = Core::new();
    core.new_cell_with_uuid(4, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x9190)
        .unwrap();
    let original = core.document_info().unwrap();
    let missing = core.sequence_catalog_info();
    assert_eq!(
        (
            missing.revision,
            missing.cell_count,
            missing.owner_generation
        ),
        (0, 0, 0)
    );
    assert_eq!(missing.active_index, None);
    let first = source("cell1.tga", 0x9191, 4, 2, [1, 2, 3, 255]);
    let second = source("cell2.tga", 0x9192, 4, 2, [5, 6, 7, 255]);
    core.set_sequence(vec![second.clone(), first.clone()])
        .unwrap();
    let catalog = core.sequence_catalog_info();
    assert_eq!(catalog.cell_count, 2);
    assert_ne!(catalog.owner_generation, 0);
    let usage = core.resource_usage();
    assert_eq!(usage.thumbnail_cache_bytes, 2 * 4 * 2 * 4);
    let borrowed_pixels = core.sequence_thumbnail(0).unwrap().rgba8.as_ptr();
    for _ in 0..16 {
        let metadata = core.sequence_cell_metadata(0).unwrap();
        assert_eq!(
            (metadata.name, metadata.document_uuid),
            ("cell1.tga", 0x9191)
        );
        assert_eq!((metadata.width, metadata.height), (4, 2));
        let preview = core.sequence_thumbnail(0).unwrap();
        assert_eq!(preview.rgba8.as_ptr(), borrowed_pixels);
        assert_eq!(metadata.thumbnail_checksum, preview.checksum);
        assert_eq!(&preview.rgba8[..4], &[1, 2, 3, 255]);
    }
    let mut owned = core.sequence_cell(0).unwrap();
    owned.thumbnail.rgba8[0] = 99;
    assert_eq!(core.sequence_thumbnail(0).unwrap().rgba8[0], 1);
    assert_eq!(core.sequence_catalog_info(), catalog);
    assert_eq!(core.document_info().unwrap(), original);
    assert_eq!(
        core.resource_usage().thumbnail_cache_bytes,
        usage.thumbnail_cache_bytes
    );
    assert!(core.sequence_cell_metadata(2).is_err());
    assert!(core.sequence_thumbnail(2).is_err());
    assert!(core.set_sequence(vec![first.clone(), first]).is_err());
    assert_eq!(core.sequence_catalog_info(), catalog);
    core.set_sequence(vec![second]).unwrap();
    let replaced = core.sequence_catalog_info();
    assert!(replaced.revision > catalog.revision);
    assert_ne!(replaced.owner_generation, catalog.owner_generation);
    assert_eq!(core.sequence_thumbnail(0).unwrap().rgba8[0], 5);
}
