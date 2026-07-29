use super::*;

#[test]
fn viewport_resize_refits_only_persistent_fit_or_one_to_one_modes() {
    let mut core = Core::new();
    core.new_cell(200, 100, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let fit = core
        .apply_view(ViewCommand::Fit {
            viewport_width: 400.0,
            viewport_height: 300.0,
        })
        .unwrap();
    assert_eq!(fit.mode(), ViewMode::Fit);
    let resized = core
        .apply_view(ViewCommand::ViewportResized {
            viewport_width: 800.0,
            viewport_height: 600.0,
        })
        .unwrap();
    assert_eq!(resized.mode(), ViewMode::Fit);
    assert!(resized.zoom() > fit.zoom());

    core.apply_view(ViewCommand::PanBy {
        device_dx: 10.0,
        device_dy: 5.0,
    })
    .unwrap();
    let manual = core
        .apply_view(ViewCommand::ViewportResized {
            viewport_width: 640.0,
            viewport_height: 480.0,
        })
        .unwrap();
    assert_eq!(manual.mode(), ViewMode::Manual);
    let repeated = core
        .apply_view(ViewCommand::ViewportResized {
            viewport_width: 320.0,
            viewport_height: 240.0,
        })
        .unwrap();
    assert_eq!(repeated.mode(), ViewMode::Manual);
    assert_eq!(repeated.zoom(), manual.zoom());
    assert_eq!(repeated.pan_x(), manual.pan_x());
    assert_eq!(repeated.pan_y(), manual.pan_y());
    assert_eq!(repeated.viewport_width(), 320.0);
    assert_eq!(repeated.viewport_height(), 240.0);
}

#[test]
fn acceptance_layer_tree_undo_redo_save_reopen_and_validation() {
    let mut core = Core::new();
    let created = core
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let base_layer = created.layer_id;
    let (_, duplicate) = core.duplicate_layer(base_layer).unwrap();
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap().len(), 1);
    core.redo().unwrap();
    assert_eq!(core.layers().unwrap().len(), 2);
    core.reorder_layer(duplicate, 0).unwrap();
    core.undo().unwrap();
    assert_eq!(core.layers().unwrap()[1].id, duplicate);
    core.redo().unwrap();
    let saved_order: Vec<_> = core
        .layers()
        .unwrap()
        .iter()
        .map(|layer| layer.id)
        .collect();
    assert_eq!(saved_order, vec![duplicate, base_layer]);

    let path = std::env::temp_dir().join(format!(
        "inkpod-test-tree-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    core.save(&path).unwrap();
    core.delete_layer(duplicate).unwrap();
    assert_eq!(core.layers().unwrap().len(), 1);
    core.undo().unwrap();
    assert_eq!(
        core.layers()
            .unwrap()
            .iter()
            .map(|layer| layer.id)
            .collect::<Vec<_>>(),
        saved_order
    );
    core.redo().unwrap();
    assert_eq!(core.layers().unwrap().len(), 1);
    core.save(&path).unwrap();

    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.layers().unwrap().len(), 1);
    reopened.undo().unwrap_err();

    let revision = reopened.document_info().unwrap().document_revision;
    assert!(matches!(
        reopened.create_plane(
            base_layer,
            PlaneType::Selection,
            PixelFormat::BinaryMask8,
            "Invalid Selection"
        ),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(
        reopened.document_info().unwrap().document_revision,
        revision
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn required_singleton_and_incompatible_plane_operations_do_not_mutate_document() {
    let mut core = Core::new();
    let created = core
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let before = core.document.clone();
    let revision = core.document_info().unwrap().document_revision;

    assert_eq!(
        core.duplicate_plane(created.main_plane_id),
        Err(CoreError::InvalidState(
            "required singleton planes cannot be duplicated"
        ))
    );
    assert_eq!(core.document, before);
    assert_eq!(core.document_info().unwrap().document_revision, revision);

    assert_eq!(
        core.merge_plane_into_below(created.main_plane_id),
        Err(CoreError::InvalidArgument(
            "only planes with compatible type and pixel format can merge"
        ))
    );
    assert_eq!(core.document, before);
    assert_eq!(core.document_info().unwrap().document_revision, revision);
}

#[test]
fn layer_thumbnail_preserves_aspect_content_and_hidden_layer_preview() {
    let mut core = Core::new();
    let created = core
        .new_cell(8, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.apply_stroke(&color_stroke(
        PaintTool::Pencil,
        1.0,
        StrokeSample {
            x: 7.0,
            y: 3.0,
            pressure: 1.0,
        },
    ))
    .unwrap();

    let before_visible = core.document_info().unwrap();
    let visible = core.layer_thumbnail(created.layer_id, 4, 4).unwrap();
    assert_eq!(
        (visible.width, visible.height, visible.stride_bytes),
        (4, 2, 16)
    );
    assert_eq!(visible.layer_id, created.layer_id);
    assert_eq!(
        visible.revision,
        core.document_info().unwrap().document_revision
    );
    assert_eq!(visible.pixels.len(), 32);
    assert!(visible.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
    assert_eq!(core.document_info().unwrap(), before_visible);

    core.set_layer_properties(created.layer_id, false, true, 1_000, "Coloring")
        .unwrap();
    let before_hidden = core.document_info().unwrap();
    let hidden = core.layer_thumbnail(created.layer_id, 4, 4).unwrap();
    assert_eq!(hidden.pixels, visible.pixels);
    assert!(matches!(
        core.layer_thumbnail(created.layer_id, 0, 4),
        Err(CoreError::InvalidArgument(_))
    ));
    assert!(matches!(
        core.layer_thumbnail(u64::MAX, 4, 4),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), before_hidden);
}

#[test]
fn acceptance_selection_boolean_property_and_authoring_tools() {
    fn mask(bits: u8) -> TileRaster {
        let mut mask = TileRaster::new(8, 1, PixelFormat::BinaryMask8).unwrap();
        for x in 0..8 {
            if bits & (1 << x) != 0 {
                mask.set_pixel(x, 0, PixelValue::Binary(255), 1).unwrap();
            }
        }
        mask
    }
    for left in 0_u8..=u8::MAX {
        for right in [0_u8, 0x55, 0xaa, u8::MAX] {
            let left_mask = mask(left);
            let right_mask = mask(right);
            for (operation, expected) in [
                (SelectionOperation::New, right),
                (SelectionOperation::Add, left | right),
                (SelectionOperation::Subtract, left & !right),
                (SelectionOperation::Intersect, left & right),
            ] {
                let combined =
                    combine_selection_masks(&left_mask, &right_mask, operation, 2).unwrap();
                for x in 0..8 {
                    assert_eq!(
                        matches!(combined.pixel(x, 0).unwrap(), PixelValue::Binary(255)),
                        expected & (1 << x) != 0
                    );
                }
            }
        }
    }

    let mut core = Core::new();
    core.new_cell(12, 12, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_selection(
        &SelectionShape::Ellipse(RectI32 {
            x: 2,
            y: 2,
            width: 8,
            height: 6,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    assert!(core.selection_bounds().unwrap().is_some());
    core.apply_selection(
        &SelectionShape::Polyline(vec![
            PointF32 { x: 1.0, y: 1.0 },
            PointF32 { x: 10.0, y: 1.0 },
            PointF32 { x: 5.0, y: 10.0 },
        ]),
        SelectionOperation::New,
    )
    .unwrap();
    assert!(core.selection_bounds().unwrap().is_some());
    core.apply_selection(
        &SelectionShape::Wand {
            x: 0,
            y: 0,
            tolerance: 0,
            gap_close: 0,
        },
        SelectionOperation::New,
    )
    .unwrap();
    assert_eq!(
        core.selection_bounds().unwrap(),
        Some(RectI32 {
            x: 0,
            y: 0,
            width: 12,
            height: 12,
        })
    );
    core.select_color(PixelValue::Binary(0), 0, false, SelectionOperation::New)
        .unwrap();
    assert_eq!(core.selection_bounds().unwrap().unwrap().width, 12);
    core.select_color(PixelValue::Binary(0), 0, true, SelectionOperation::New)
        .unwrap();
    assert_eq!(core.selection_bounds().unwrap(), None);
    core.apply_selection(
        &SelectionShape::Lasso(vec![
            PointF32 { x: 2.0, y: 2.0 },
            PointF32 { x: 9.0, y: 2.0 },
            PointF32 { x: 6.0, y: 9.0 },
        ]),
        SelectionOperation::New,
    )
    .unwrap();
    let lasso = core.selection_bounds().unwrap().unwrap();
    assert!(lasso.width >= 6 && lasso.height >= 6);
    core.apply_selection(
        &SelectionShape::Trace {
            points: vec![PointF32 { x: 0.0, y: 0.0 }, PointF32 { x: 11.0, y: 11.0 }],
            diameter: 1.5,
        },
        SelectionOperation::Add,
    )
    .unwrap();
    core.resize_selection(1).unwrap();
    core.resize_selection(-1).unwrap();
    let saved_bounds = core.selection_bounds().unwrap();
    let (_, selection_layer) = core.selection_to_layer("Saved Selection").unwrap();
    core.invert_selection().unwrap();
    core.selection_from_layer(selection_layer, SelectionLayerOperation::Replace)
        .unwrap();
    assert_eq!(core.selection_bounds().unwrap(), saved_bounds);
}

#[test]
fn acceptance_coordinate_preserving_typed_paste_and_floating_transform() {
    let mut source = Core::new();
    source
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    source.set_active_plane(ActivePlane::Color).unwrap();
    source
        .apply_stroke(&color_stroke(
            PaintTool::Pencil,
            1.0,
            StrokeSample {
                x: 6.0,
                y: 6.0,
                pressure: 1.0,
            },
        ))
        .unwrap();
    source
        .apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 6,
                y: 6,
                width: 1,
                height: 1,
            }),
            SelectionOperation::New,
        )
        .unwrap();
    let payload = source.copy_selection().unwrap();
    assert_eq!(payload.bounds.x, 6);
    assert_eq!(payload.planes[0].pixels[0].x, 6);

    let mut destination = Core::new();
    destination
        .new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    destination.begin_paste(&payload).unwrap();
    assert!(matches!(
        destination.commit_floating(),
        Err(CoreError::InvalidState(_))
    ));
    destination
        .set_floating_transform(FloatingTransform {
            translate_x: -4.0,
            translate_y: -4.0,
            ..FloatingTransform::default()
        })
        .unwrap();
    destination.commit_floating().unwrap();
    assert_eq!(
        destination.plane_pixel(ActivePlane::Color, 2, 2).unwrap(),
        PixelValue::Rgba([12, 34, 56, 255])
    );
    destination.undo().unwrap();
    assert!(
        destination
            .plane_pixel(ActivePlane::Color, 2, 2)
            .unwrap()
            .is_zero()
    );
    let transform_payload = ClipboardPayload {
        source_document_uuid: 1,
        bounds: RectI32 {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
        planes: vec![ClipboardPlane {
            kind: PlaneType::Color,
            pixel_format: PixelFormat::StraightRgba8,
            origin_x: 0,
            origin_y: 0,
            pixels: vec![
                ClipboardPixel {
                    x: 0,
                    y: 0,
                    value: PixelValue::Rgba([255, 0, 0, 255]),
                },
                ClipboardPixel {
                    x: 1,
                    y: 0,
                    value: PixelValue::Rgba([0, 0, 255, 255]),
                },
            ],
        }],
    };
    destination.begin_paste(&transform_payload).unwrap();
    destination
        .set_floating_transform(FloatingTransform {
            translate_x: 1.0,
            scale_x: 2.0,
            ..FloatingTransform::default()
        })
        .unwrap();
    destination.commit_floating().unwrap();
    assert_eq!(
        (0..4)
            .map(|x| destination.plane_pixel(ActivePlane::Color, x, 0).unwrap())
            .collect::<Vec<_>>(),
        vec![
            PixelValue::Rgba([255, 0, 0, 255]),
            PixelValue::Rgba([255, 0, 0, 255]),
            PixelValue::Rgba([0, 0, 255, 255]),
            PixelValue::Rgba([0, 0, 255, 255]),
        ]
    );
    destination.undo().unwrap();
    destination.begin_paste(&transform_payload).unwrap();
    destination
        .set_floating_transform(FloatingTransform {
            rotation_degrees: 180.0,
            ..FloatingTransform::default()
        })
        .unwrap();
    destination.commit_floating().unwrap();
    assert_eq!(
        destination.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([0, 0, 255, 255])
    );
    assert_eq!(
        destination.plane_pixel(ActivePlane::Color, 1, 0).unwrap(),
        PixelValue::Rgba([255, 0, 0, 255])
    );
    destination.undo().unwrap();
    let revision = destination.document_info().unwrap().document_revision;
    destination.begin_paste(&payload).unwrap();
    destination.cancel_floating();
    assert!(matches!(
        destination.commit_floating(),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(
        destination.document_info().unwrap().document_revision,
        revision
    );
}

#[test]
fn acceptance_view_flip_and_destructive_mirror_have_separate_revisions() {
    let mut core = Core::new();
    core.new_cell(8, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let before = core.document_info().unwrap();
    let view = core
        .apply_view(ViewCommand::Flip {
            axis: MirrorAxis::Horizontal,
        })
        .unwrap();
    let after_view = core.document_info().unwrap();
    assert_eq!(after_view.document_revision, before.document_revision);
    assert!(after_view.view_revision > before.view_revision);
    assert!(view.flip_horizontal());
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 1, 1).unwrap(),
        PixelValue::Binary(255)
    );

    core.mirror_document(MirrorAxis::Horizontal).unwrap();
    let after_mirror = core.document_info().unwrap();
    assert!(after_mirror.document_revision > after_view.document_revision);
    assert_eq!(after_mirror.view_revision, after_view.view_revision);
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 6, 1).unwrap(),
        PixelValue::Binary(255)
    );
    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::MainLine, 1, 1).unwrap(),
        PixelValue::Binary(255)
    );
}

#[test]
fn acceptance_multi_view_locator_guides_grid_and_shortcuts() {
    let mut core = Core::new();
    core.new_cell(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let secondary = core.create_view().unwrap();
    core.apply_view_for(
        secondary,
        ViewCommand::BoxZoom {
            document_rect: RectI32 {
                x: 4,
                y: 4,
                width: 8,
                height: 8,
            },
            viewport_width: 160.0,
            viewport_height: 160.0,
        },
    )
    .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 3.0,
        y: 3.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let primary = core.build_snapshot();
    let other = core.build_snapshot_for(secondary).unwrap();
    assert_eq!(primary.revision(), other.revision());
    assert_ne!(primary.view(), other.view());

    let (_, guide_id) = core.add_guide(GuideAxis::Vertical, 5).unwrap();
    core.set_grid(GridConfig {
        origin_x: 0,
        origin_y: 0,
        spacing_x: 8,
        spacing_y: 8,
        subdivisions: 2,
    })
    .unwrap();
    assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (5.2, 7.8));
    core.apply_view(ViewCommand::SetGuideSnapEnabled(true))
        .unwrap();
    assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (5.0, 7.8));
    core.apply_view(ViewCommand::SetGuideSnapEnabled(false))
        .unwrap();
    core.apply_view(ViewCommand::SetGridSnapEnabled(true))
        .unwrap();
    assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (4.0, 8.0));
    core.apply_view(ViewCommand::SetGuideSnapEnabled(true))
        .unwrap();
    assert_eq!(core.snap_document_point(5.2, 7.8).unwrap(), (5.0, 8.0));
    core.move_guide(guide_id, 6).unwrap();
    assert_eq!(core.guides().unwrap()[0].position, 6);
    let overlay_snapshot = core.build_snapshot();
    assert_eq!(overlay_snapshot.guides()[0].position, 6);
    assert_eq!(overlay_snapshot.grid().subdivisions, 2);

    let locator = core.locator_sample(None, 3.0, 3.0).unwrap();
    assert_eq!((locator.document_x, locator.document_y), (3, 3));
    core.rebind_shortcut(ShortcutBinding {
        command_id: 99,
        virtual_key: u32::from(b'Z'),
        modifiers: 1,
    })
    .unwrap();
    assert_eq!(
        core.resolve_shortcut(u32::from(b'Z'), SHORTCUT_MODIFIER_CONTROL)
            .unwrap(),
        Some(99)
    );
    assert!(
        !core
            .shortcut_bindings()
            .iter()
            .any(|binding| binding.command_id == 1)
    );
    assert!(
        core.shortcut_bindings()
            .iter()
            .any(|binding| binding.command_id == 99)
    );
    core.reset_shortcuts();
    assert_eq!(
        core.resolve_shortcut(u32::from(b'Z'), SHORTCUT_MODIFIER_CONTROL)
            .unwrap(),
        Some(1)
    );
    assert!(
        core.shortcut_bindings()
            .iter()
            .any(|binding| binding.command_id == 1)
    );
}

#[test]
fn shortcut_sequences_are_prefix_free_transactional_and_resettable() {
    let mut core = Core::new();
    let stroke = |key| ShortcutStroke {
        virtual_key: u32::from(key),
        modifiers: 0,
    };
    let defaults = vec![
        ShortcutSequenceBinding {
            command_id: 10,
            strokes: vec![stroke(b'Q'), stroke(b'F'), stroke(b'A')],
        },
        ShortcutSequenceBinding {
            command_id: 11,
            strokes: vec![stroke(b'Q'), stroke(b'F'), stroke(b'B')],
        },
        ShortcutSequenceBinding {
            command_id: 12,
            strokes: vec![ShortcutStroke {
                virtual_key: u32::from(b'S'),
                modifiers: SHORTCUT_MODIFIER_CONTROL,
            }],
        },
    ];
    core.set_shortcut_defaults(&defaults).unwrap();
    assert_eq!(
        core.resolve_shortcut_sequence(&[stroke(b'Q')]).unwrap(),
        ShortcutSequenceMatch::Prefix
    );
    assert_eq!(
        core.resolve_shortcut_sequence(&[stroke(b'Q'), stroke(b'F')])
            .unwrap(),
        ShortcutSequenceMatch::Prefix
    );
    assert_eq!(
        core.resolve_shortcut_sequence(&[stroke(b'Q'), stroke(b'F'), stroke(b'B')])
            .unwrap(),
        ShortcutSequenceMatch::Exact(11)
    );

    let conflicting = vec![
        ShortcutSequenceBinding {
            command_id: 20,
            strokes: vec![stroke(b'Q'), stroke(b'F')],
        },
        ShortcutSequenceBinding {
            command_id: 21,
            strokes: vec![stroke(b'Q'), stroke(b'F'), stroke(b'C')],
        },
    ];
    assert!(core.replace_shortcut_sequences(&conflicting).is_err());
    assert_eq!(core.shortcut_sequences(), defaults);

    core.rebind_shortcut_sequence(ShortcutSequenceBinding {
        command_id: 12,
        strokes: vec![stroke(b'Q'), stroke(b'F'), stroke(b'A')],
    })
    .unwrap();
    assert_eq!(
        core.resolve_shortcut_sequence(&[stroke(b'Q'), stroke(b'F'), stroke(b'A')])
            .unwrap(),
        ShortcutSequenceMatch::Exact(12)
    );
    assert!(
        !core
            .shortcut_sequences()
            .iter()
            .any(|binding| binding.command_id == 10)
    );

    core.reset_shortcuts();
    assert_eq!(core.shortcut_sequences(), defaults);
}

#[test]
fn tree_order_merge_names_and_active_ids_remain_consistent() {
    let mut core = Core::new();
    let created = core
        .new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.apply_stroke(&color_stroke(
        PaintTool::Pencil,
        1.0,
        StrokeSample {
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
        },
    ))
    .unwrap();
    let (_, top) = core.duplicate_layer(created.layer_id).unwrap();
    core.reorder_layer(top, 0).unwrap();
    {
        let document = core.document.as_mut().unwrap();
        let top_color = document.layers[0]
            .planes
            .iter_mut()
            .find(|plane| plane.kind == PlaneType::Color)
            .unwrap();
        top_color
            .raster
            .set_pixel(0, 0, PixelValue::Rgba([0, 0, 255, 128]), 99)
            .unwrap();
    }
    core.render_cache.clear();
    assert_eq!(core.build_snapshot().tiles()[0].pixels(), [156, 17, 6, 255]);
    core.merge_layer_into_below(top).unwrap();
    assert_eq!(core.layers().unwrap().len(), 1);
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([6, 17, 156, 255])
    );
    assert_eq!(
        paste_value(
            PixelValue::Rgba16([u16::MAX, 0, 0, u16::MAX]),
            PixelValue::Rgba16([0, 0, u16::MAX, 32_768]),
            PlaneType::Raster,
        )
        .unwrap(),
        PixelValue::Rgba16([32_767, 0, 32_768, u16::MAX])
    );

    let (_, raster_layer) = core.create_layer(LayerKind::Raster, "Raster").unwrap();
    let raster_plane = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == raster_layer)
        .unwrap()
        .planes[0]
        .id;
    core.duplicate_plane(raster_plane).unwrap();
    core.duplicate_plane(raster_plane).unwrap();
    let raster_names: BTreeSet<_> = core
        .layers()
        .unwrap()
        .iter()
        .find(|layer| layer.id == raster_layer)
        .unwrap()
        .planes
        .iter()
        .map(|plane| plane.name.clone())
        .collect();
    assert_eq!(raster_names.len(), 3);

    let (_, duplicate_coloring) = core.duplicate_layer(created.layer_id).unwrap();
    core.create_layer(LayerKind::Frame, "Frame").unwrap();
    core.delete_layer(duplicate_coloring).unwrap();
    assert!(core.document_info().is_ok());
    assert!(
        core.document
            .as_ref()
            .unwrap()
            .plane_by_id(core.document.as_ref().unwrap().active_plane_id)
            .is_some()
    );
}

#[test]
fn editable_layer_and_plane_flags_guard_pixel_commands() {
    let mut core = Core::new();
    let created = core
        .new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.set_plane_properties(created.color_plane_id, true, false, 1_000, "Color")
        .unwrap();
    let locked_revision = core.document_info().unwrap().document_revision;
    assert!(matches!(
        core.apply_stroke(&color_stroke(
            PaintTool::Pencil,
            1.0,
            StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            }
        )),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(
        core.document_info().unwrap().document_revision,
        locked_revision
    );

    core.set_plane_properties(created.color_plane_id, true, true, 1_000, "Color")
        .unwrap();
    core.set_layer_properties(created.layer_id, true, false, 1_000, "Coloring")
        .unwrap();
    let locked_revision = core.document_info().unwrap().document_revision;
    assert!(matches!(
        core.apply_fill(&fill_request(0, 0, [1, 2, 3, 255])),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(
        core.document_info().unwrap().document_revision,
        locked_revision
    );
}
