use super::*;

#[test]
fn shooting_frame_is_canvas_only_for_every_raster_export() {
    let mut core = Core::new();
    core.new_cell(32, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    for format in [
        CommonRasterFormat::Png,
        CommonRasterFormat::Tiff,
        CommonRasterFormat::Tga,
        CommonRasterFormat::Bmp,
    ] {
        for white in [false, true] {
            let before = core.export_common_raster(format, white).unwrap();
            let initial = core.document_info().unwrap();
            let frame = core
                .edit_shooting_frame(
                    initial.document_revision,
                    ShootingFrameEdit::Create(input(0x1800_0000)),
                )
                .unwrap();
            assert_eq!(core.build_snapshot().shooting_frames().len(), 1);
            let committed = core.document_info().unwrap();
            assert_eq!(core.export_common_raster(format, white).unwrap(), before);
            assert_eq!(core.document_info().unwrap(), committed);
            core.edit_shooting_frame(
                committed.document_revision,
                ShootingFrameEdit::Delete {
                    frame_id: frame.frame_id().unwrap(),
                },
            )
            .unwrap();
        }
    }
}

fn input(rotation_turns: u32) -> ShootingFrameInput {
    ShootingFrameInput {
        center_x_milli: 10_000,
        center_y_milli: 8_000,
        width_milli: 30_000,
        height_milli: 18_000,
        rotation_turns,
        anchor: ShootingFrameAnchor::Center,
        visible: true,
    }
}

#[test]
fn shooting_frame_001_edit_preview_export_and_history_contract() {
    let mut core = Core::new();
    core.new_cell(32, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let before = core.document_info().unwrap();
    let ordinary_before = core
        .export_common_raster(CommonRasterFormat::Png, false)
        .unwrap();

    let mut create_preview = input(0x0800_0000);
    core.begin_shooting_frame_preview(
        before.document_revision,
        ShootingFramePreviewTarget::Create,
        create_preview,
    )
    .unwrap();
    create_preview.center_x_milli = 12_000;
    core.update_shooting_frame_preview(create_preview).unwrap();
    assert_eq!(
        core.build_snapshot().shooting_frames()[0].center_x_milli,
        12_000
    );
    core.cancel_shooting_frame_preview().unwrap();
    assert!(core.shooting_frame().unwrap().is_none());
    assert_eq!(
        core.document_info().unwrap().document_revision,
        before.document_revision
    );

    let created = core
        .edit_shooting_frame(
            before.document_revision,
            ShootingFrameEdit::Create(input(0x1800_0000)),
        )
        .unwrap();
    let frame_id = created.frame_id().unwrap();
    let committed = core.shooting_frame().unwrap().unwrap();
    assert_eq!(committed.id, frame_id);
    assert_eq!(core.build_snapshot().shooting_frames(), &[committed]);
    assert_eq!(
        core.export_common_raster(CommonRasterFormat::Png, false)
            .unwrap(),
        ordinary_before
    );
    assert_eq!(core.document_thumbnail().unwrap().rgba8.len(), 32 * 24 * 4);
    assert_eq!(core.document_info().unwrap().frames, before.frames);

    let revision = core.document_info().unwrap().document_revision;
    let mut preview = committed.input();
    preview.center_x_milli = -4_000;
    preview.center_y_milli = 26_000;
    preview.anchor = ShootingFrameAnchor::TopLeft;
    core.begin_shooting_frame_preview(
        revision,
        ShootingFramePreviewTarget::Update(frame_id),
        preview,
    )
    .unwrap();
    assert_eq!(core.shooting_frame().unwrap().unwrap(), committed);
    assert_eq!(
        core.build_snapshot().shooting_frames()[0].center_x_milli,
        -4_000
    );
    core.cancel_shooting_frame_preview().unwrap();
    assert_eq!(core.shooting_frame().unwrap().unwrap(), committed);
    assert_eq!(core.document_info().unwrap().document_revision, revision);

    core.begin_shooting_frame_preview(
        revision,
        ShootingFramePreviewTarget::Update(frame_id),
        preview,
    )
    .unwrap();
    core.apply_shooting_frame_preview().unwrap();
    assert_eq!(
        core.document_info().unwrap().document_revision,
        revision + 1
    );
    assert_eq!(core.shooting_frame().unwrap().unwrap().input(), preview);
    core.undo().unwrap();
    assert_eq!(core.shooting_frame().unwrap().unwrap(), committed);
    core.redo().unwrap();
    assert_eq!(core.shooting_frame().unwrap().unwrap().input(), preview);
}

#[test]
fn shooting_frame_001_invalid_stale_noop_and_nonuniform_resample_are_atomic() {
    let mut core = Core::new();
    core.new_cell(32, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let revision = core.document_info().unwrap().document_revision;
    let created = core
        .edit_shooting_frame(revision, ShootingFrameEdit::Create(input(0x1800_0000)))
        .unwrap();
    let frame_id = created.frame_id().unwrap();
    let stable_frame = core.shooting_frame().unwrap();
    let stable_info = core.document_info().unwrap();

    let no_op = core
        .edit_shooting_frame(
            stable_info.document_revision,
            ShootingFrameEdit::Update {
                frame_id,
                input: stable_frame.unwrap().input(),
            },
        )
        .unwrap();
    assert_eq!(no_op.revision(), stable_info.document_revision);
    assert!(
        core.edit_shooting_frame(
            stable_info.document_revision - 1,
            ShootingFrameEdit::Delete { frame_id }
        )
        .is_err()
    );
    assert!(
        core.edit_shooting_frame(
            stable_info.document_revision,
            ShootingFrameEdit::Update {
                frame_id,
                input: ShootingFrameInput {
                    width_milli: 0,
                    ..input(0)
                },
            },
        )
        .is_err()
    );
    assert!(
        core.resize_document(DocumentResize {
            width: 48,
            height: 24,
            dpi_x_milli: DEFAULT_DPI_MILLI,
            dpi_y_milli: DEFAULT_DPI_MILLI,
            resample: true,
            anchor: ResizeAnchor::Center,
        })
        .is_err()
    );
    assert_eq!(core.shooting_frame().unwrap(), stable_frame);
    assert_eq!(core.document_info().unwrap(), stable_info);
}

#[test]
fn shooting_frame_001_anchor_transform_save_reopen_and_replay_contract() {
    let mut core = Core::new();
    core.new_cell(40, 30, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let revision = core.document_info().unwrap().document_revision;
    let mut source = input(0);
    source.center_x_milli = 20_000;
    source.center_y_milli = 15_000;
    source.width_milli = 20_000;
    source.height_milli = 10_000;
    let id = core
        .edit_shooting_frame(revision, ShootingFrameEdit::Create(source))
        .unwrap()
        .frame_id()
        .unwrap();
    let frame = core.shooting_frame().unwrap().unwrap();
    for (anchor, expected) in [
        (ShootingFrameAnchor::TopLeft, (10_000, 10_000)),
        (ShootingFrameAnchor::TopRight, (30_000, 10_000)),
        (ShootingFrameAnchor::Center, (20_000, 15_000)),
        (ShootingFrameAnchor::BottomLeft, (10_000, 20_000)),
        (ShootingFrameAnchor::BottomRight, (30_000, 20_000)),
    ] {
        assert_eq!(
            frame.anchor_point(anchor).unwrap(),
            ShootingFramePoint {
                x_milli: expected.0,
                y_milli: expected.1,
            }
        );
    }
    core.rotate_document(RotateDirection::Right90).unwrap();
    let rotated = core.shooting_frame().unwrap().unwrap();
    assert_eq!(rotated.id, id);
    assert_eq!(rotated.rotation_turns, 1 << 30);
    assert_eq!(
        (rotated.center_x_milli, rotated.center_y_milli),
        (15_000, 20_000)
    );
    core.mirror_document(MirrorAxis::Horizontal).unwrap();
    core.verify_journal_replay().unwrap();

    let path = std::env::temp_dir().join(format!(
        "inkpod-shooting-frame-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    core.save(&path).unwrap();
    let expected = core.shooting_frame().unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.shooting_frame().unwrap(), expected);
    reopened.verify_journal_replay().unwrap();
    fs::remove_file(path).unwrap();
}
