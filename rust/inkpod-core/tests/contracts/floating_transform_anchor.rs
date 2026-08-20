use super::*;

const TARGET: f64 = 12.0;

fn raster_payload() -> ClipboardPayload {
    ClipboardPayload {
        source_document_uuid: 0x4d31_3600_0000_0000_0000_0000_0000_0001,
        bounds: RectI32 {
            x: 2,
            y: 3,
            width: 2,
            height: 2,
        },
        planes: vec![ClipboardPlane {
            kind: PlaneType::Color,
            pixel_format: PixelFormat::StraightRgba8,
            origin_x: 2,
            origin_y: 3,
            pixels: vec![
                ClipboardPixel {
                    x: 2,
                    y: 3,
                    value: PixelValue::Rgba([255, 0, 0, 255]),
                },
                ClipboardPixel {
                    x: 3,
                    y: 3,
                    value: PixelValue::Rgba([0, 255, 0, 255]),
                },
                ClipboardPixel {
                    x: 2,
                    y: 4,
                    value: PixelValue::Rgba([0, 0, 255, 255]),
                },
                ClipboardPixel {
                    x: 3,
                    y: 4,
                    value: PixelValue::Rgba([255, 255, 0, 255]),
                },
            ],
        }],
    }
}

fn transformed_raster_bounds(anchor: FloatingTransformAnchor) -> RectI32 {
    let mut core = Core::new();
    core.new_cell(24, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.begin_paste(&raster_payload()).unwrap();
    core.set_floating_transform(FloatingTransform {
        anchor,
        target_x: TARGET,
        target_y: TARGET,
        scale_x: 2.0,
        scale_y: 2.0,
        rotation_degrees: 0.0,
    })
    .unwrap();
    core.commit_floating().unwrap();

    let mut minimum_x = u32::MAX;
    let mut minimum_y = u32::MAX;
    let mut maximum_x = 0;
    let mut maximum_y = 0;
    for y in 0..24 {
        for x in 0..24 {
            if !core
                .plane_pixel(ActivePlane::Color, x, y)
                .unwrap()
                .is_zero()
            {
                minimum_x = minimum_x.min(x);
                minimum_y = minimum_y.min(y);
                maximum_x = maximum_x.max(x);
                maximum_y = maximum_y.max(y);
            }
        }
    }
    RectI32 {
        x: minimum_x as i32,
        y: minimum_y as i32,
        width: (maximum_x - minimum_x + 1) as i32,
        height: (maximum_y - minimum_y + 1) as i32,
    }
}

#[test]
fn xform_003_five_half_open_anchors_place_uniform_scale_at_absolute_target() {
    let cases = [
        (
            FloatingTransformAnchor::TopLeft,
            RectI32 {
                x: 12,
                y: 12,
                width: 4,
                height: 4,
            },
        ),
        (
            FloatingTransformAnchor::TopRight,
            RectI32 {
                x: 8,
                y: 12,
                width: 4,
                height: 4,
            },
        ),
        (
            FloatingTransformAnchor::Center,
            RectI32 {
                x: 10,
                y: 10,
                width: 4,
                height: 4,
            },
        ),
        (
            FloatingTransformAnchor::BottomLeft,
            RectI32 {
                x: 12,
                y: 8,
                width: 4,
                height: 4,
            },
        ),
        (
            FloatingTransformAnchor::BottomRight,
            RectI32 {
                x: 8,
                y: 8,
                width: 4,
                height: 4,
            },
        ),
    ];
    for (anchor, expected) in cases {
        assert_eq!(transformed_raster_bounds(anchor), expected, "{anchor:?}");
    }
}

#[test]
fn xform_003_preview_retry_cancel_and_invalid_values_do_not_publish_document_state() {
    let mut core = Core::new();
    core.new_cell(24, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let before = core.document_info().unwrap();
    let history_before = core.history_entries().len();
    core.begin_paste(&raster_payload()).unwrap();
    for anchor in [
        FloatingTransformAnchor::TopLeft,
        FloatingTransformAnchor::BottomRight,
    ] {
        core.set_floating_transform(FloatingTransform {
            anchor,
            target_x: TARGET,
            target_y: TARGET,
            scale_x: 1.25,
            scale_y: 0.75,
            rotation_degrees: 15.0,
        })
        .unwrap();
    }
    assert!(
        core.set_floating_transform(FloatingTransform {
            target_x: f64::INFINITY,
            ..FloatingTransform::default()
        })
        .is_err()
    );
    assert!(
        core.set_floating_transform(FloatingTransform {
            target_x: 16_777_217.0,
            scale_x: -1.0,
            ..FloatingTransform::default()
        })
        .is_err()
    );
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.history_entries().len(), history_before);
    core.cancel_floating();
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.history_entries().len(), history_before);
}

#[test]
fn xform_003_anchor_transform_replays_and_round_trips_through_current_native_format() {
    let path = std::env::temp_dir().join(format!(
        "inkpod-xform-003-v21-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut core = Core::new();
    core.new_cell(24, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.begin_paste(&raster_payload()).unwrap();
    core.set_floating_transform(FloatingTransform {
        anchor: FloatingTransformAnchor::BottomRight,
        target_x: TARGET,
        target_y: TARGET,
        scale_x: 2.0,
        scale_y: 2.0,
        rotation_degrees: 0.0,
    })
    .unwrap();
    core.commit_floating().unwrap();
    core.verify_journal_replay().unwrap();
    let expected = core.document_state_digest().unwrap();
    core.save(&path).unwrap();

    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), expected);
    reopened.verify_journal_replay().unwrap();
    reopened.undo().unwrap();
    reopened.redo().unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), expected);
    fs::remove_file(path).unwrap();
}
