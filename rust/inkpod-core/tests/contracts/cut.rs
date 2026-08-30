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

#[test]
fn seq_struct_001_ordered_edit_is_one_cut_transaction_and_round_trips() {
    let directory = unique_test_directory("cut-sequence-edit");
    let mut members = Vec::new();
    for index in 0..5_u32 {
        let mut member = write_member(
            &directory,
            &format!("C005-{index:04}.inkpod"),
            0x6000 + u128::from(index),
        );
        member.display_number = index * 2 + 1;
        members.push(member);
    }
    let inserted = write_member(&directory, "C005-extra.inkpod", 0x7000);
    let removed_identity = SequenceMemberId::of(&members[1]);
    let moved_identity = SequenceMemberId::of(&members[4]);
    let first_identity = SequenceMemberId::of(&members[0]);
    let mut cut = CutCore::new(CutCreateRequest {
        cut_uuid: 0x90abcd,
        metadata: cut_metadata("C005"),
        defaults: cut_defaults(),
        members: members.clone(),
    })
    .unwrap();
    let before = cut.info();

    let outcome = cut
        .edit_sequence(SequenceEditRequest {
            base_revision: before.revision,
            operations: vec![
                SequenceEditOperation::Insert {
                    position: 2,
                    member: inserted.clone(),
                },
                SequenceEditOperation::MoveBefore {
                    member: moved_identity,
                    anchor: first_identity,
                },
                SequenceEditOperation::RenumberRange {
                    start: 0,
                    count: 6,
                    first_number: 10,
                    step: 10,
                },
                SequenceEditOperation::Remove {
                    member: removed_identity,
                },
            ],
        })
        .unwrap();
    assert_eq!(outcome, CutMutationOutcome::Applied);
    assert_eq!(cut.info().revision, before.revision + 1);
    assert_eq!(cut.info().state_id, before.state_id + 1);
    assert_eq!(cut.info().member_count, 5);
    assert_eq!(
        cut.members()
            .iter()
            .map(|member| (member.document_uuid, member.display_number))
            .collect::<Vec<_>>(),
        vec![
            (0x6004, 10),
            (0x6000, 20),
            (0x7000, 40),
            (0x6002, 50),
            (0x6003, 60)
        ]
    );
    assert!(directory.join(&members[1].relative_path).is_file());

    assert_eq!(cut.undo().unwrap(), CutMutationOutcome::Applied);
    assert_eq!(cut.members(), members.as_slice());
    assert_eq!(cut.redo().unwrap(), CutMutationOutcome::Applied);
    let edited = cut.members().to_vec();
    let path = directory.join("C005.inkpod");
    cut.save(&path).unwrap();
    let reopened = CutCore::open(&path).unwrap();
    assert_eq!(reopened.members(), edited.as_slice());
    assert!(reopened.info().can_undo);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn seq_struct_001_noop_cancel_stale_invalid_and_overflow_are_atomic() {
    let directory = unique_test_directory("cut-sequence-atomic");
    let first = write_member(&directory, "C006-0001.inkpod", 0x8001);
    let mut second = write_member(&directory, "C006-0002.inkpod", 0x8002);
    second.display_number = 2;
    let mut cut = CutCore::new(CutCreateRequest {
        cut_uuid: 0x91abcd,
        metadata: cut_metadata("C006"),
        defaults: cut_defaults(),
        members: vec![first.clone(), second.clone()],
    })
    .unwrap();
    let before = cut.clone();
    assert_eq!(
        cut.edit_sequence(SequenceEditRequest {
            base_revision: before.info().revision,
            operations: vec![SequenceEditOperation::RenumberRange {
                start: 0,
                count: 0,
                first_number: 1,
                step: 1,
            }],
        })
        .unwrap(),
        CutMutationOutcome::NoOp
    );
    assert_eq!(cut.info(), before.info());
    assert_eq!(cut.cancel_sequence_edit(), CutMutationOutcome::NoOp);

    let invalid_cases = [
        SequenceEditRequest {
            base_revision: before.info().revision + 1,
            operations: Vec::new(),
        },
        SequenceEditRequest {
            base_revision: before.info().revision,
            operations: vec![SequenceEditOperation::MoveAfter {
                member: SequenceMemberId::of(&first),
                anchor: SequenceMemberId::new(99, 99).unwrap(),
            }],
        },
        SequenceEditRequest {
            base_revision: before.info().revision,
            operations: vec![SequenceEditOperation::RenumberRange {
                start: 0,
                count: 2,
                first_number: u32::MAX,
                step: 1,
            }],
        },
        SequenceEditRequest {
            base_revision: before.info().revision,
            operations: vec![SequenceEditOperation::RenumberRange {
                start: 0,
                count: 2,
                first_number: 1,
                step: 0,
            }],
        },
    ];
    for request in invalid_cases {
        assert!(cut.edit_sequence(request).is_err());
        assert_eq!(cut.info(), before.info());
        assert_eq!(cut.members(), before.members());
    }

    let duplicate_number = CutMember {
        cell_id: 7,
        document_uuid: 0x9000,
        display_number: 2,
        relative_path: "C006-extra.inkpod".to_owned(),
    };
    assert!(
        cut.edit_sequence(SequenceEditRequest {
            base_revision: before.info().revision,
            operations: vec![SequenceEditOperation::Insert {
                position: 2,
                member: duplicate_number,
            }],
        })
        .is_err()
    );
    assert_eq!(cut.info(), before.info());
    assert_eq!(cut.members(), before.members());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn seq_struct_001_retained_member_asset_overflow_is_atomic() {
    let members = (0..MAX_CUT_MEMBERS)
        .map(|index| CutMember {
            cell_id: index as u64 + 1,
            document_uuid: index as u128 + 1,
            display_number: index as u32 + 1,
            relative_path: format!("C007-{index:04}.inkpod"),
        })
        .collect::<Vec<_>>();
    let removed = SequenceMemberId::of(&members[0]);
    let mut cut = CutCore::new(CutCreateRequest {
        cut_uuid: 0x92abcd,
        metadata: cut_metadata("C007"),
        defaults: cut_defaults(),
        members,
    })
    .unwrap();
    cut.edit_sequence(SequenceEditRequest {
        base_revision: cut.info().revision,
        operations: vec![SequenceEditOperation::Remove { member: removed }],
    })
    .unwrap();
    let before = cut.clone();
    let error = cut
        .edit_sequence(SequenceEditRequest {
            base_revision: cut.info().revision,
            operations: vec![SequenceEditOperation::Insert {
                position: 0,
                member: CutMember {
                    cell_id: 65,
                    document_uuid: 65,
                    display_number: 65,
                    relative_path: "C007-0064.inkpod".to_owned(),
                },
            }],
        })
        .unwrap_err();
    assert_eq!(error.operation_index(), SEQUENCE_EDIT_REQUEST_ERROR_INDEX);
    assert_eq!(cut.info(), before.info());
    assert_eq!(cut.members(), before.members());
}

fn unique_test_directory(label: &str) -> PathBuf {
    let sequence = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("inkpod-{label}-{}-{sequence}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}
