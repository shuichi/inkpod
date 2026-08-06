use super::*;

const JOURNAL_TEST_UUID: u128 = 0x0049_4e4b_504f_442d_4a4f_5552_4e41_4c02;

fn journal_core() -> Core {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        32,
        32,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        JOURNAL_TEST_UUID,
    )
    .unwrap();
    core
}

fn set_main_line(core: &mut Core, value: u8) {
    let revision = core.document_info().unwrap().document_revision;
    core.execute_primitive(PrimitiveRequest::SetMainLineColor {
        expected_revision: revision,
        color: PixelValue::Rgba([value, value.wrapping_add(1), value.wrapping_add(2), 255]),
    })
    .unwrap();
}

fn replace_palette(core: &mut Core, value: u16) {
    let revision = core.document_info().unwrap().document_revision;
    core.execute_primitive(PrimitiveRequest::ReplacePalette {
        expected_revision: revision,
        colors: vec![PixelValue::Rgba16([
            value,
            value.wrapping_add(1),
            value.wrapping_add(2),
            u16::MAX,
        ])],
    })
    .unwrap();
}

#[test]
fn hist_001_empty_core_has_no_document_journal_namespace() {
    assert!(Core::new().journal_state().is_none());
}

#[test]
fn hist_001_commit_and_history_move_records_are_ordered() {
    let mut core = journal_core();
    set_main_line(&mut core, 10);
    replace_palette(&mut core, 20);
    core.undo().unwrap();
    core.redo().unwrap();
    core.jump_history(0).unwrap();
    core.jump_history(2).unwrap();

    let journal = core.journal_entries();
    assert_eq!(journal.len(), 6);
    let JournalEntry::Commit(first) = &journal[0] else {
        panic!("event 1 must be a Commit");
    };
    assert_eq!(first.event_id().get(), 1);
    assert_eq!(first.procedure().procedure_id().get(), 1);
    assert_eq!(first.parent_state_id().get(), 1);
    assert_eq!(first.committed_state_id().get(), 2);
    assert_eq!(first.branch_id().get(), 1);

    let JournalEntry::Commit(second) = &journal[1] else {
        panic!("event 2 must be a Commit");
    };
    assert_eq!(second.event_id().get(), 2);
    assert_eq!(second.procedure().procedure_id().get(), 2);
    assert_eq!(second.parent_state_id().get(), 2);
    assert_eq!(second.committed_state_id().get(), 3);

    let expected_moves = [
        (HistoryMoveKind::Undo, 3, 2),
        (HistoryMoveKind::Redo, 2, 3),
        (HistoryMoveKind::Jump, 3, 1),
        (HistoryMoveKind::Jump, 1, 3),
    ];
    for (offset, (kind, source, destination)) in expected_moves.into_iter().enumerate() {
        let JournalEntry::HistoryMove(movement) = &journal[offset + 2] else {
            panic!("event {} must be a HistoryMove", offset + 3);
        };
        assert_eq!(movement.event_id().get(), (offset + 3) as u64);
        assert_eq!(movement.kind(), kind);
        assert_eq!(movement.source_state_id().get(), source);
        assert_eq!(movement.destination_state_id().get(), destination);
        assert_eq!(movement.active_branch_id().get(), 1);
    }
    assert_eq!(core.history_entries().len(), 2);
    assert_eq!(core.history_cursor(), 2);
}

#[test]
fn hist_001_branch_cut_and_commit_are_adjacent_and_retain_the_inactive_tail() {
    let mut core = journal_core();
    set_main_line(&mut core, 10);
    replace_palette(&mut core, 20);
    core.undo().unwrap();

    let before_noop = core.journal_state();
    let revision = core.document_info().unwrap().document_revision;
    let unchanged = core.main_line_color().unwrap();
    let outcome = core
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: revision,
            color: unchanged,
        })
        .unwrap();
    assert!(outcome.procedure().is_none());
    assert_eq!(core.journal_state(), before_noop);

    assert!(matches!(
        core.execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: revision,
            color: PixelValue::Binary(1),
        }),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.journal_state(), before_noop);
    core.begin_stroke(&line_stroke(vec![StrokeSample {
        x: 3.0,
        y: 4.0,
        pressure: 1.0,
    }]))
    .unwrap();
    core.cancel_stroke();
    assert_eq!(core.journal_state(), before_noop);

    set_main_line(&mut core, 30);

    let journal = core.journal_entries();
    assert_eq!(journal.len(), 5);
    let JournalEntry::BranchCut(cut) = &journal[3] else {
        panic!("event 4 must be the branch cut");
    };
    assert_eq!(cut.event_id().get(), 4);
    assert_eq!(cut.fork_state_id().get(), 2);
    assert_eq!(cut.old_active_tail_state_id().get(), 3);
    assert_eq!(cut.new_branch_id().get(), 2);
    assert_eq!(cut.deactivated_branch_id().get(), 1);

    let JournalEntry::Commit(commit) = &journal[4] else {
        panic!("event 5 must be the branch commit");
    };
    assert_eq!(commit.event_id().get(), 5);
    assert_eq!(commit.procedure().procedure_id().get(), 3);
    assert_eq!(commit.parent_state_id().get(), 2);
    assert_eq!(commit.committed_state_id().get(), 4);
    assert_eq!(commit.branch_id().get(), 2);

    let state = core.journal_state().unwrap();
    assert!(state.is_complete());
    assert_eq!(state.current_state_id().get(), 4);
    assert_eq!(state.active_branch_id().get(), 2);
    assert_eq!(state.active_branch_tail_state_id().get(), 4);
    assert_eq!(state.savepoint_state_id().unwrap().get(), 1);
    assert_eq!(state.history_cursor(), 2);
    assert_eq!(state.visible_history_count(), 2);
    assert_eq!(core.history_entries().len(), 2);
    assert_eq!(core.history_cursor(), 2);
    assert!(!core.document_info().unwrap().can_redo);

    let replay = core.verify_journal_replay().unwrap();
    assert_eq!(replay.current_state_id(), state.current_state_id());
    assert_eq!(replay.active_branch_id(), state.active_branch_id());
    assert_eq!(replay.history_cursor(), state.history_cursor());
    assert_eq!(
        replay.visible_history_count(),
        state.visible_history_count()
    );
    assert_eq!(
        replay.document_state_digest(),
        core.document_state_digest().unwrap()
    );
}

#[test]
fn hist_001_noop_stale_and_cancel_consume_no_journal_identity() {
    let mut core = journal_core();
    let initial = core.journal_state();
    let initial_revision = core.document_info().unwrap().document_revision;

    let no_op = core
        .execute_primitive(PrimitiveRequest::SetMainLineColor {
            expected_revision: initial_revision,
            color: core.main_line_color().unwrap(),
        })
        .unwrap();
    assert!(no_op.procedure().is_none());
    assert_eq!(core.journal_state(), initial);

    assert!(matches!(
        core.execute_primitive(PrimitiveRequest::ReplacePalette {
            expected_revision: initial_revision.saturating_sub(1),
            colors: vec![PixelValue::Rgba([1, 2, 3, 255])],
        }),
        Err(CoreError::InvalidState(
            "primitive request revision is stale"
        ))
    ));
    assert_eq!(core.journal_state(), initial);

    core.begin_stroke(&line_stroke(vec![StrokeSample {
        x: 4.0,
        y: 5.0,
        pressure: 1.0,
    }]))
    .unwrap();
    core.cancel_stroke();
    assert_eq!(core.journal_state(), initial);

    set_main_line(&mut core, 40);
    let JournalEntry::Commit(commit) = &core.journal_entries()[0] else {
        panic!("the first real change must be one Commit");
    };
    assert_eq!(commit.event_id().get(), 1);
    assert_eq!(commit.procedure().procedure_id().get(), 1);
    assert_eq!(commit.committed_state_id().get(), 2);
}

#[test]
fn hist_001_runtime_cache_can_be_released_and_rebuilt_from_genesis_and_journal() {
    let mut core = journal_core();
    set_main_line(&mut core, 10);
    replace_palette(&mut core, 20);
    core.undo().unwrap();
    set_main_line(&mut core, 30);
    core.jump_history(1).unwrap();
    let before = core.verify_journal_replay().unwrap();
    let before_revision = core.document_info().unwrap().document_revision;
    let before_journal = core.journal_entries().to_vec();
    let before_usage = core.resource_usage();
    assert!(before_usage.history_bytes > 0);

    core.release_history_cache().unwrap();

    let released_usage = core.resource_usage();
    assert_eq!(released_usage.history_bytes, 0);
    assert_eq!(
        released_usage.history_entry_count,
        before_usage.history_entry_count
    );
    assert_eq!(
        core.document_info().unwrap().document_revision,
        before_revision
    );
    assert_eq!(core.journal_entries(), before_journal);
    assert_eq!(core.verify_journal_replay().unwrap(), before);
    core.redo().unwrap();
    core.undo().unwrap();
    core.redo().unwrap();
    assert_eq!(
        core.verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        core.document_state_digest().unwrap()
    );
}

#[test]
fn hist_001_state_id_savepoint_survives_history_moves_and_branching() {
    let path = std::env::temp_dir().join(format!(
        "inkpod-core-journal-savepoint-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut core = journal_core();
    set_main_line(&mut core, 10);
    replace_palette(&mut core, 20);
    core.save(&path).unwrap();
    let savepoint = core.journal_state().unwrap().savepoint_state_id().unwrap();
    assert_eq!(savepoint.get(), 3);
    assert!(!core.document_info().unwrap().dirty);

    core.undo().unwrap();
    assert!(core.document_info().unwrap().dirty);
    set_main_line(&mut core, 30);
    assert_eq!(
        core.journal_state().unwrap().savepoint_state_id(),
        Some(savepoint)
    );
    assert_eq!(
        core.journal_state()
            .unwrap()
            .active_branch_tail_state_id()
            .get(),
        4
    );
    assert!(core.document_info().unwrap().dirty);
    core.undo().unwrap();
    assert_eq!(core.journal_state().unwrap().current_state_id().get(), 2);
    assert!(core.document_info().unwrap().dirty);
    core.redo().unwrap();
    assert_eq!(core.journal_state().unwrap().current_state_id().get(), 4);
    assert!(core.document_info().unwrap().dirty);
    core.verify_journal_replay().unwrap();

    fs::remove_file(path).unwrap();
}

#[test]
fn hist_001_document_families_keep_the_journal_complete_and_replayable() {
    let mut core = journal_core();
    set_main_line(&mut core, 10);
    core.verify_journal_replay().unwrap();
    replace_palette(&mut core, 20);
    core.verify_journal_replay().unwrap();
    core.add_guide(GuideAxis::Vertical, 7).unwrap();
    core.verify_journal_replay().unwrap();
    core.create_layer(LayerKind::Raster, "Replay raster")
        .unwrap();
    core.verify_journal_replay().unwrap();
    core.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 1,
            y: 1,
            width: 3,
            height: 3,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    core.verify_journal_replay().unwrap();
    core.mirror_document(MirrorAxis::Horizontal).unwrap();
    core.verify_journal_replay().unwrap();
    core.light_table_create_set("Replay light table").unwrap();

    let state = core.journal_state().unwrap();
    assert!(state.is_complete());
    assert_eq!(state.visible_history_count(), core.history_entries().len());
    let expected_digest = core.document_state_digest().unwrap();
    core.verify_journal_replay().unwrap();
    core.release_history_cache().unwrap();
    assert_eq!(
        core.verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        expected_digest
    );
    assert!(core.document_info().unwrap().can_undo);
    core.undo().unwrap();
    core.redo().unwrap();
    core.verify_journal_replay().unwrap();
}

#[test]
fn hist_001_fixed_seed_journal_state_machine_matches_full_replay() {
    const SEED: u64 = 0xd8e8_5f2a_7b19_c403;
    let mut random = SEED;
    let mut core = journal_core();
    set_main_line(&mut core, 1);
    replace_palette(&mut core, 2);
    core.undo().unwrap();
    core.redo().unwrap();
    core.jump_history(0).unwrap();
    core.jump_history(2).unwrap();
    core.undo().unwrap();
    set_main_line(&mut core, 3);

    for step in 0..192_u64 {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        match random % 8 {
            0..=2 => replace_palette(&mut core, (step as u16).wrapping_add(1)),
            3 if core.document_info().unwrap().can_undo => {
                core.undo().unwrap();
            }
            4 if core.document_info().unwrap().can_redo => {
                core.redo().unwrap();
            }
            5 => {
                let count = core.history_entries().len();
                let target = usize::try_from(random >> 32).unwrap() % (count + 1);
                core.jump_history(target).unwrap();
            }
            6 => {
                core.apply_stroke(&line_stroke(vec![StrokeSample {
                    x: ((step * 7) % 32) as f32,
                    y: ((step * 11) % 32) as f32,
                    pressure: 1.0,
                }]))
                .unwrap();
            }
            _ => set_main_line(&mut core, (step as u8).wrapping_add(1)),
        }

        if step % 37 == 0 {
            core.release_history_cache().unwrap();
        }

        let replay = core.verify_journal_replay().unwrap_or_else(|error| {
            panic!("journal replay seed={SEED:#018x} step={step}: {error}")
        });
        let state = core.journal_state().unwrap();
        assert_eq!(replay.current_state_id(), state.current_state_id());
        assert_eq!(replay.active_branch_id(), state.active_branch_id());
        assert_eq!(replay.history_cursor(), core.history_cursor());
        assert_eq!(replay.visible_history_count(), core.history_entries().len());
        assert_eq!(
            replay.document_state_digest(),
            core.document_state_digest().unwrap(),
            "journal replay seed={SEED:#018x} step={step}"
        );
    }

    let mut commits = 0;
    let mut cuts = 0;
    let mut undos = 0;
    let mut redos = 0;
    let mut jumps = 0;
    for entry in core.journal_entries() {
        match entry {
            JournalEntry::Commit(_) => commits += 1,
            JournalEntry::BranchCut(_) => cuts += 1,
            JournalEntry::HistoryMove(movement) => match movement.kind() {
                HistoryMoveKind::Undo => undos += 1,
                HistoryMoveKind::Redo => redos += 1,
                HistoryMoveKind::Jump => jumps += 1,
            },
        }
    }
    assert!(commits > 0, "seed must exercise Commit");
    assert!(cuts > 0, "seed must exercise BranchCut");
    assert!(undos > 0, "seed must exercise Undo");
    assert!(redos > 0, "seed must exercise Redo");
    assert!(jumps > 0, "seed must exercise Jump");
}
