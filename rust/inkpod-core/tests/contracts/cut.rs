use super::*;

fn cut_metadata(name: &str) -> CutMetadata {
    CutMetadata {
        work_title: "Inkpod".to_owned(),
        episode: "01".to_owned(),
        scene: "A".to_owned(),
        cut_name: name.to_owned(),
        instruction: "Protect the main line".to_owned(),
        duration_frames: 24,
    }
}

fn cut_defaults() -> CutDefaults {
    CutDefaults {
        sizing: CellSizing::ImagePixels {
            width: 32,
            height: 24,
        },
        dpi_x_milli: DEFAULT_DPI_MILLI,
        dpi_y_milli: DEFAULT_DPI_MILLI,
        margin_milli: 50,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: FrameAnchor::Center,
        initial_layer_kind: LayerKind::BinaryColoring,
        pixel_format: PixelFormat::StraightRgba8,
    }
}

fn write_member(directory: &Path, name: &str, uuid: u128) -> CutMember {
    let mut cell = Core::new();
    let info = cell
        .new_cell_with_uuid(32, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
        .unwrap();
    cell.save(&directory.join(name)).unwrap();
    CutMember {
        cell_id: info.cell_id,
        document_uuid: uuid,
        display_number: 1,
        relative_path: name.to_owned(),
    }
}

#[test]
fn cut_metadata_defaults_history_and_save_reopen_are_independent_from_cells() {
    let directory = unique_test_directory("cut-roundtrip");
    let member = write_member(&directory, "C001-0001.inkpod", 0x1234);
    let mut cut = CutCore::new(CutCreateRequest {
        cut_uuid: 0xabcdef,
        metadata: cut_metadata("C001"),
        defaults: cut_defaults(),
        members: vec![member.clone()],
    })
    .unwrap();
    let original = cut.info();
    assert!(original.dirty);
    assert_eq!(original.cut_id, 1);
    assert_eq!(cut.members(), std::slice::from_ref(&member));

    let mut updated_metadata = cut_metadata("C001A");
    updated_metadata.duration_frames = 36;
    let outcome = cut
        .update(CutUpdateRequest {
            base_revision: original.revision,
            metadata: updated_metadata.clone(),
            defaults: cut_defaults(),
        })
        .unwrap();
    assert_eq!(outcome, CutMutationOutcome::Applied);
    assert_eq!(cut.info().metadata, updated_metadata);
    assert!(cut.info().can_undo);
    cut.undo().unwrap();
    assert_eq!(cut.info().metadata.cut_name, "C001");
    cut.redo().unwrap();
    assert_eq!(cut.info().metadata.cut_name, "C001A");

    let path = directory.join("C001.inkpod");
    cut.save(&path).unwrap();
    assert!(!cut.info().dirty);
    cut.save(&path).unwrap();
    assert!(!cut.info().dirty);
    let mut reopened = CutCore::open(&path).unwrap();
    assert_eq!(reopened.info().metadata, updated_metadata);
    assert_eq!(reopened.members(), &[member]);
    assert!(reopened.info().can_undo);
    reopened.undo().unwrap();
    assert_eq!(reopened.info().metadata.cut_name, "C001");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cut_noop_invalid_cancel_stale_and_failure_publish_nothing() {
    let directory = unique_test_directory("cut-atomic");
    let member = write_member(&directory, "C002-0001.inkpod", 0x2345);
    let request = CutCreateRequest {
        cut_uuid: 0x123456,
        metadata: cut_metadata("C002"),
        defaults: cut_defaults(),
        members: vec![member],
    };
    let mut cut = CutCore::new(request).unwrap();
    let before = cut.info();
    assert_eq!(
        cut.update(CutUpdateRequest {
            base_revision: before.revision,
            metadata: before.metadata.clone(),
            defaults: before.defaults,
        })
        .unwrap(),
        CutMutationOutcome::NoOp
    );
    assert_eq!(cut.info(), before);

    let mut changed = before.metadata.clone();
    changed.scene = "B".to_owned();
    assert!(matches!(
        cut.update(CutUpdateRequest {
            base_revision: before.revision + 1,
            metadata: changed.clone(),
            defaults: before.defaults,
        }),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(cut.info(), before);
    assert_eq!(cut.cancel_update(), CutMutationOutcome::NoOp);
    assert_eq!(cut.info(), before);

    changed.cut_name.clear();
    assert!(matches!(
        cut.update(CutUpdateRequest {
            base_revision: before.revision,
            metadata: changed,
            defaults: before.defaults,
        }),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(cut.info(), before);
    assert!(
        cut.save(&directory.join("missing").join("C002.inkpod"))
            .is_err()
    );
    assert_eq!(cut.info(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cut_rejects_duplicate_missing_traversal_and_wrong_cell_references() {
    let directory = unique_test_directory("cut-members");
    let member = write_member(&directory, "C003-0001.inkpod", 0x3456);
    let base = CutCreateRequest {
        cut_uuid: 0x789abc,
        metadata: cut_metadata("C003"),
        defaults: cut_defaults(),
        members: vec![member.clone()],
    };
    let mut duplicate = base.clone();
    duplicate.members.push(member.clone());
    assert!(matches!(
        CutCore::new(duplicate),
        Err(CoreError::InvalidArgument(_))
    ));

    let mut traversal = base.clone();
    traversal.members[0].relative_path = "../other.inkpod".to_owned();
    assert!(matches!(
        CutCore::new(traversal),
        Err(CoreError::InvalidArgument(_))
    ));

    let mut cut = CutCore::new(base).unwrap();
    let path = directory.join("C003.inkpod");
    cut.save(&path).unwrap();
    fs::remove_file(directory.join("C003-0001.inkpod")).unwrap();
    assert!(CutCore::open(&path).is_err());
    let _replacement = write_member(&directory, "C003-0001.inkpod", 0x9999);
    assert!(CutCore::open(&path).is_err());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cut_member_identity_combines_document_uuid_and_cell_id() {
    let directory = unique_test_directory("cut-member-identity");
    let first = write_member(&directory, "C004-0001.inkpod", 0x4567);
    let mut second = write_member(&directory, "C004-0002.inkpod", 0x5678);
    second.display_number = 2;
    assert_eq!(first.cell_id, second.cell_id);
    assert_ne!(first.document_uuid, second.document_uuid);

    let mut cut = CutCore::new(CutCreateRequest {
        cut_uuid: 0x89abcd,
        metadata: cut_metadata("C004"),
        defaults: cut_defaults(),
        members: vec![first.clone(), second.clone()],
    })
    .unwrap();
    let path = directory.join("C004.inkpod");
    cut.save(&path).unwrap();
    assert_eq!(CutCore::open(&path).unwrap().members(), &[first, second]);
    fs::remove_dir_all(directory).unwrap();
}

fn unique_test_directory(label: &str) -> PathBuf {
    let sequence = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("inkpod-{label}-{}-{sequence}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}
