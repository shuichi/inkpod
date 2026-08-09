use super::*;

fn image_options(kind: LayerKind, pixel_format: PixelFormat) -> CellCreationOptions {
    CellCreationOptions {
        sizing: CellSizing::ImagePixels {
            width: 1_920,
            height: 1_080,
        },
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
        margin_milli: 50,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: FrameAnchor::Center,
        initial_layer_kind: kind,
        pixel_format,
        count: 3,
    }
}

#[test]
fn cell_creation_image_and_frame_modes_have_one_canonical_geometry() {
    let image = plan_cell_creation(&image_options(
        LayerKind::GrayscaleColoring,
        PixelFormat::StraightRgba16,
    ))
    .unwrap();
    assert_eq!(image.len(), 3);
    let item = image.item(0).unwrap();
    assert_eq!((item.width(), item.height()), (1_920, 1_080));
    assert_eq!(item.pixel_format(), PixelFormat::StraightRgba16);
    assert_eq!(
        item.frames(),
        FrameMetadata {
            hundred_frame: RectI32 {
                x: 87,
                y: 49,
                width: 1_745,
                height: 982,
            },
            reference_frame: RectI32 {
                x: 959,
                y: 540,
                width: 1_745,
                height: 982,
            },
            drawing_frame: RectI32 {
                x: 87,
                y: 49,
                width: 1_745,
                height: 982,
            },
            safe_frame: RectI32 {
                x: 174,
                y: 98,
                width: 1_570,
                height: 884,
            },
            shooting_frame: RectI32 {
                x: 87,
                y: 49,
                width: 1_745,
                height: 982,
            },
            maximum_close_frame: RectI32 {
                x: 523,
                y: 294,
                width: 872,
                height: 491,
            },
            margins: Margins {
                left: 87,
                top: 49,
                right: 88,
                bottom: 49,
            },
        }
    );

    let frame = plan_cell_creation(&CellCreationOptions {
        sizing: CellSizing::FrameMicrometres {
            width: 254_000,
            height: 127_000,
        },
        dpi_x_milli: 100_000,
        dpi_y_milli: 100_000,
        margin_milli: 50,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: FrameAnchor::BottomRight,
        initial_layer_kind: LayerKind::BinaryColoring,
        pixel_format: PixelFormat::StraightRgba8,
        count: 1,
    })
    .unwrap();
    let item = frame.item(0).unwrap();
    assert_eq!((item.width(), item.height()), (1_100, 550));
    assert_eq!(
        item.frames().reference_frame,
        RectI32 {
            x: 1_050,
            y: 525,
            width: 1_000,
            height: 500,
        }
    );
    assert_eq!(
        item.frames().maximum_close_frame,
        RectI32 {
            x: 550,
            y: 275,
            width: 500,
            height: 250,
        }
    );

    let anchors = [
        (FrameAnchor::TopLeft, (50, 25), (50, 25)),
        (FrameAnchor::TopRight, (1_050, 25), (550, 25)),
        (FrameAnchor::Center, (550, 275), (300, 150)),
        (FrameAnchor::BottomLeft, (50, 525), (50, 275)),
        (FrameAnchor::BottomRight, (1_050, 525), (550, 275)),
    ];
    for (anchor_index, (anchor, reference_origin, maximum_close_origin)) in
        anchors.into_iter().enumerate()
    {
        let anchored = plan_cell_creation(&CellCreationOptions {
            sizing: CellSizing::FrameMicrometres {
                width: 254_000,
                height: 127_000,
            },
            dpi_x_milli: 100_000,
            dpi_y_milli: 100_000,
            margin_milli: 50,
            safe_frame_ratio_milli: 900,
            maximum_close_ratio_milli: 500,
            anchor,
            initial_layer_kind: LayerKind::BinaryColoring,
            pixel_format: PixelFormat::StraightRgba8,
            count: 1,
        })
        .unwrap();
        let frames = anchored.item(0).unwrap().frames();
        assert_eq!(
            (frames.reference_frame.x, frames.reference_frame.y),
            reference_origin
        );
        assert_eq!(
            (frames.maximum_close_frame.x, frames.maximum_close_frame.y),
            maximum_close_origin
        );
        let mut core = Core::new();
        core.new_cell_from_creation_plan(
            anchored.item(0).unwrap(),
            0x1_000_u128 + anchor_index as u128,
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!(
            "inkpod-cell-anchor-{}-{anchor_index}.inkpod",
            std::process::id()
        ));
        core.save(&path).unwrap();
        let mut reopened = Core::new();
        assert_eq!(reopened.open(&path).unwrap().frames, frames);
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn every_initial_layer_and_depth_is_created_and_reopens_without_loss() {
    let kinds = [
        LayerKind::BinaryColoring,
        LayerKind::GrayscaleColoring,
        LayerKind::Raster,
        LayerKind::Selection,
        LayerKind::Frame,
        LayerKind::VanishingPoint,
        LayerKind::Adjustment,
        LayerKind::Text,
        LayerKind::Annotation,
        LayerKind::VectorColoring,
    ];
    let formats = [PixelFormat::StraightRgba8, PixelFormat::StraightRgba16];
    for (kind_index, kind) in kinds.into_iter().enumerate() {
        for (format_index, format) in formats.into_iter().enumerate() {
            let mut options = image_options(kind, format);
            options.sizing = CellSizing::ImagePixels {
                width: 16,
                height: 8,
            };
            options.count = 1;
            let plan = plan_cell_creation(&options).unwrap();
            let item = plan.item(0).unwrap();
            let mut core = Core::new();
            let uuid = 0x2000_u128 + (kind_index * 2 + format_index) as u128;
            let created = core.new_cell_from_creation_plan(item, uuid).unwrap();
            assert!(!created.dirty);
            assert!(!created.can_undo);
            assert_eq!(created.frames, item.frames());
            let layers = core.layers().unwrap();
            assert_eq!(layers[0].kind, kind);
            let primary = layers
                .iter()
                .find(|layer| {
                    layer
                        .planes
                        .iter()
                        .any(|plane| plane.kind == PlaneType::Color)
                })
                .unwrap();
            assert_eq!(
                primary
                    .planes
                    .iter()
                    .find(|plane| plane.kind == PlaneType::Color)
                    .unwrap()
                    .pixel_format,
                format
            );
            if kind == LayerKind::GrayscaleColoring {
                assert_eq!(
                    layers[0]
                        .planes
                        .iter()
                        .find(|plane| plane.kind == PlaneType::MainLine)
                        .unwrap()
                        .pixel_format,
                    if format == PixelFormat::StraightRgba16 {
                        PixelFormat::Grayscale16
                    } else {
                        PixelFormat::Grayscale8
                    }
                );
            }

            let path = std::env::temp_dir().join(format!(
                "inkpod-cell-creation-{}-{kind_index}-{format_index}.inkpod",
                std::process::id()
            ));
            core.save(&path)
                .unwrap_or_else(|error| panic!("save failed for {kind:?}/{format:?}: {error}"));
            let mut reopened = Core::new();
            let info = reopened
                .open(&path)
                .unwrap_or_else(|error| panic!("reopen failed for {kind:?}/{format:?}: {error}"));
            assert_eq!(info.frames, created.frames);
            assert_eq!(reopened.layers().unwrap(), layers);
            std::fs::remove_file(path).unwrap();
        }
    }
}

#[test]
fn invalid_overflow_and_failed_uuid_do_not_publish_or_consume_ids() {
    let invalid = [
        CellCreationOptions {
            count: 0,
            ..image_options(LayerKind::BinaryColoring, PixelFormat::StraightRgba8)
        },
        CellCreationOptions {
            count: MAX_CELL_CREATION_COUNT + 1,
            ..image_options(LayerKind::BinaryColoring, PixelFormat::StraightRgba8)
        },
        CellCreationOptions {
            sizing: CellSizing::FrameMicrometres {
                width: u32::MAX,
                height: u32::MAX,
            },
            dpi_x_milli: MAX_CELL_CREATION_DPI_MILLI,
            dpi_y_milli: MAX_CELL_CREATION_DPI_MILLI,
            margin_milli: 1_000,
            ..image_options(LayerKind::BinaryColoring, PixelFormat::StraightRgba8)
        },
        CellCreationOptions {
            pixel_format: PixelFormat::BinaryMask8,
            ..image_options(LayerKind::BinaryColoring, PixelFormat::StraightRgba8)
        },
    ];
    for options in invalid {
        assert!(plan_cell_creation(&options).is_err());
    }
    let maximum = plan_cell_creation(&CellCreationOptions {
        count: MAX_CELL_CREATION_COUNT,
        ..image_options(LayerKind::BinaryColoring, PixelFormat::StraightRgba8)
    })
    .unwrap();
    assert_eq!(maximum.len(), MAX_CELL_CREATION_COUNT as usize);

    let options = image_options(LayerKind::Raster, PixelFormat::StraightRgba16);
    let plan = plan_cell_creation(&CellCreationOptions {
        count: 1,
        ..options
    })
    .unwrap();
    let item = plan.item(0).unwrap();
    let mut core = Core::new();
    core.new_cell_with_uuid(4, 4, 96_000, 96_000, 0x1111)
        .unwrap();
    let before = core.document_info().unwrap();
    assert!(core.new_cell_from_creation_plan(item, 0).is_err());
    assert_eq!(core.document_info().unwrap(), before);

    let created_after_failure = core.new_cell_from_creation_plan(item, 0x2222).unwrap();
    let mut reference = Core::new();
    reference
        .new_cell_with_uuid(4, 4, 96_000, 96_000, 0x1111)
        .unwrap();
    let expected = reference.new_cell_from_creation_plan(item, 0x2222).unwrap();
    assert_eq!(created_after_failure.document_id, expected.document_id);
    assert_eq!(created_after_failure.cell_id, expected.cell_id);
    assert_eq!(created_after_failure.layer_id, expected.layer_id);
}
