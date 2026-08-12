use super::*;

fn input(layer_id: u64, x_milli: i64, y_milli: i64) -> VanishingPointInput {
    VanishingPointInput {
        layer_id,
        x_milli,
        y_milli,
        interval_milli_degrees: 30_000,
        angle_milli_degrees: 0,
        color: PixelValue::Rgba([40, 120, 220, 255]),
        opacity_milli: 750,
        visible: true,
    }
}

fn core_with_layer() -> (Core, u64) {
    let mut core = Core::new();
    core.new_cell(64, 48, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let (_, layer_id) = core
        .create_layer(LayerKind::VanishingPoint, "Perspective")
        .unwrap();
    (core, layer_id)
}

#[test]
fn vanishing_point_001_crud_preview_snapshot_snap_and_history_contract() {
    let (mut core, layer_id) = core_with_layer();
    core.apply_view(ViewCommand::ViewportResized {
        viewport_width: 64.0,
        viewport_height: 48.0,
    })
    .unwrap();
    let base = core.document_info().unwrap();
    let ordinary = core
        .export_common_raster(CommonRasterFormat::Png, false)
        .unwrap();

    let preview = input(layer_id, -20_000, 24_000);
    core.begin_vanishing_point_preview(
        base.document_revision,
        VanishingPointPreviewTarget::Create,
        preview,
    )
    .unwrap();
    assert!(core.vanishing_points().unwrap().is_empty());
    let snapshot = core.build_snapshot();
    assert_eq!(snapshot.vanishing_points().len(), 1);
    assert!(!snapshot.radial_guides().is_empty());
    assert!(snapshot.radial_guides().len() <= MAX_SNAPSHOT_RADIAL_GUIDES);
    core.cancel_vanishing_point_preview().unwrap();
    assert!(core.vanishing_points().unwrap().is_empty());
    assert_eq!(core.document_info().unwrap(), base);

    let created = core
        .edit_vanishing_points(
            base.document_revision,
            &[VanishingPointEdit::Create(preview)],
        )
        .unwrap();
    let point_id = created.point_ids()[0];
    assert_ne!(point_id, 0);
    assert_eq!(core.vanishing_points().unwrap()[0].id, point_id);
    assert_eq!(
        core.export_common_raster(CommonRasterFormat::Png, false)
            .unwrap(),
        ordinary
    );

    core.apply_view(ViewCommand::SetSnapEnabled(true)).unwrap();
    core.apply_view(ViewCommand::SetGridSnapEnabled(true))
        .unwrap();
    core.apply_view(ViewCommand::SetGuideSnapEnabled(true))
        .unwrap();
    let radial = core.snap_document_point(20.0, 25.0).unwrap();
    assert_eq!(radial, (20.0, 24.0));
    let (_, guide_id) = core.add_guide(GuideAxis::Horizontal, 26).unwrap();
    assert_ne!(guide_id, point_id);
    assert_eq!(core.snap_document_point(20.0, 25.0).unwrap(), (20.0, 26.0));

    let revision = core.document_info().unwrap().document_revision;
    let mut updated = preview;
    updated.x_milli = 32_000;
    updated.y_milli = 20_000;
    updated.interval_milli_degrees = 15_000;
    updated.opacity_milli = 1_000;
    core.begin_vanishing_point_preview(
        revision,
        VanishingPointPreviewTarget::Update(point_id),
        updated,
    )
    .unwrap();
    assert_eq!(core.vanishing_points().unwrap()[0].input(), preview);
    assert_eq!(core.build_snapshot().vanishing_points()[0].input(), updated);
    core.apply_vanishing_point_preview().unwrap();
    assert_eq!(core.vanishing_points().unwrap()[0].input(), updated);
    core.undo().unwrap();
    assert_eq!(core.vanishing_points().unwrap()[0].input(), preview);
    core.redo().unwrap();
    assert_eq!(core.vanishing_points().unwrap()[0].input(), updated);
}

#[test]
fn vanishing_point_001_atomic_negative_delete_all_and_persistence_contract() {
    let (mut core, layer_id) = core_with_layer();
    let revision = core.document_info().unwrap().document_revision;
    let edits = [
        VanishingPointEdit::Create(input(layer_id, -100_000, 24_000)),
        VanishingPointEdit::Create(VanishingPointInput {
            interval_milli_degrees: 1_000,
            angle_milli_degrees: 195_000,
            color: PixelValue::Rgba16([257, 2_000, 40_000, 65_535]),
            opacity_milli: 0,
            ..input(layer_id, 96_000, 24_000)
        }),
    ];
    let created = core.edit_vanishing_points(revision, &edits).unwrap();
    assert_eq!(created.point_ids().len(), 2);
    let stable = core.vanishing_points().unwrap().to_vec();
    assert_eq!(stable[1].angle_milli_degrees, 15_000);
    let stable_info = core.document_info().unwrap();

    let no_op = core
        .edit_vanishing_points(
            stable_info.document_revision,
            &[VanishingPointEdit::Update {
                point_id: stable[0].id,
                input: stable[0].input(),
            }],
        )
        .unwrap();
    assert_eq!(no_op.revision(), stable_info.document_revision);
    assert!(
        core.edit_vanishing_points(
            stable_info.document_revision - 1,
            &[VanishingPointEdit::Delete {
                point_id: stable[0].id,
            }],
        )
        .is_err()
    );
    assert!(
        core.edit_vanishing_points(
            stable_info.document_revision,
            &[VanishingPointEdit::Update {
                point_id: stable[0].id,
                input: VanishingPointInput {
                    interval_milli_degrees: 0,
                    ..stable[0].input()
                },
            }],
        )
        .is_err()
    );
    assert!(
        core.edit_vanishing_points(
            stable_info.document_revision,
            &[VanishingPointEdit::Create(VanishingPointInput {
                x_milli: MAX_VANISHING_POINT_COORDINATE_MILLI + 1,
                ..input(layer_id, 0, 0)
            })],
        )
        .is_err()
    );
    assert_eq!(core.vanishing_points().unwrap(), stable);
    assert_eq!(core.document_info().unwrap(), stable_info);

    core.delete_all_vanishing_points(stable_info.document_revision)
        .unwrap();
    assert!(core.vanishing_points().unwrap().is_empty());
    core.undo().unwrap();
    assert_eq!(core.vanishing_points().unwrap(), stable);
    core.redo().unwrap();
    assert!(core.vanishing_points().unwrap().is_empty());
    core.undo().unwrap();
    core.verify_journal_replay().unwrap();

    let path = std::env::temp_dir().join(format!(
        "inkpod-vanishing-point-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.vanishing_points().unwrap(), stable);
    reopened.verify_journal_replay().unwrap();
    fs::remove_file(path).unwrap();
}

#[test]
fn vanishing_point_001_document_transform_and_angle_wrap_contract() {
    let (mut core, layer_id) = core_with_layer();
    let revision = core.document_info().unwrap().document_revision;
    core.edit_vanishing_points(
        revision,
        &[VanishingPointEdit::Create(VanishingPointInput {
            angle_milli_degrees: 195_000,
            ..input(layer_id, 96_000, 24_000)
        })],
    )
    .unwrap();
    let stable = core.vanishing_points().unwrap()[0];
    assert_eq!(stable.angle_milli_degrees, 15_000);

    core.rotate_document(RotateDirection::Right90).unwrap();
    let rotated = core.vanishing_points().unwrap()[0];
    assert_eq!((rotated.x_milli, rotated.y_milli), (24_000, 96_000));
    assert_eq!(rotated.angle_milli_degrees, 105_000);
    core.undo().unwrap();
    assert_eq!(core.vanishing_points().unwrap()[0], stable);

    core.mirror_document(MirrorAxis::Horizontal).unwrap();
    let mirrored = core.vanishing_points().unwrap()[0];
    assert_eq!((mirrored.x_milli, mirrored.y_milli), (-32_000, 24_000));
    assert_eq!(mirrored.angle_milli_degrees, 165_000);
    core.undo().unwrap();
    assert_eq!(core.vanishing_points().unwrap()[0], stable);

    let before_failure = core.document_info().unwrap();
    assert!(
        core.resize_document(DocumentResize {
            width: 128,
            height: 48,
            dpi_x_milli: DEFAULT_DPI_MILLI,
            dpi_y_milli: DEFAULT_DPI_MILLI,
            resample: true,
            anchor: ResizeAnchor::Center,
        })
        .is_err()
    );
    assert_eq!(core.document_info().unwrap(), before_failure);
    assert_eq!(core.vanishing_points().unwrap()[0], stable);

    core.resize_document(DocumentResize {
        width: 128,
        height: 96,
        dpi_x_milli: DEFAULT_DPI_MILLI,
        dpi_y_milli: DEFAULT_DPI_MILLI,
        resample: true,
        anchor: ResizeAnchor::Center,
    })
    .unwrap();
    let scaled = core.vanishing_points().unwrap()[0];
    assert_eq!((scaled.x_milli, scaled.y_milli), (192_000, 48_000));
    assert_eq!(scaled.angle_milli_degrees, 15_000);
    core.undo().unwrap();
    assert_eq!(core.vanishing_points().unwrap()[0], stable);
}
