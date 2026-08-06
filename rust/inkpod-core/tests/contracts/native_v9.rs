use super::*;
use inkpod_format::{
    NativeRecord, NativeSection, OPAQUE_PRESERVE, read_procedure_file, save_procedure_file_atomic,
};

fn native_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "inkpod-{label}-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn commit_checkpoint_interval(core: &mut Core) {
    for index in 0..256 {
        let value = if index % 2 == 0 { 1 } else { 2 };
        core.set_main_line_color(PixelValue::Rgba([value, value, value, u8::MAX]))
            .unwrap();
    }
}

fn checkpoint_payload_mut(file: &mut inkpod_format::NativeFile) -> &mut Vec<u8> {
    &mut file
        .sections
        .iter_mut()
        .find(|section| section.fourcc == *b"CKPT")
        .expect("checkpoint section")
        .records[0]
        .payload
}

fn frame_field(payload: &[u8], wanted: u32) -> std::ops::Range<usize> {
    let mut cursor = 8;
    loop {
        let ordinal = u32::from_le_bytes(payload[cursor..cursor + 4].try_into().unwrap());
        let present = payload[cursor + 4];
        let length =
            u64::from_le_bytes(payload[cursor + 8..cursor + 16].try_into().unwrap()) as usize;
        let start = cursor + 16;
        let end = start + length;
        assert_eq!(present, 1);
        if ordinal == wanted {
            return start..end;
        }
        cursor = end;
    }
}

#[test]
fn io_001_save_reopen_restores_full_journal_editor_and_all_next_id_authorities() {
    let path = native_path("v9-full-session");
    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
    }]))
    .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 2.0,
        y: 2.0,
        pressure: 1.0,
    }]))
    .unwrap();
    core.undo().unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 3.0,
        y: 3.0,
        pressure: 1.0,
    }]))
    .unwrap();
    core.update_editor_state(
        core.editor_state().unwrap().revision,
        EditorStateUpdate::SetToolDiameter {
            tool: EditorTool::Brush,
            diameter_q16: 15_i64 << 16,
        },
    )
    .unwrap();
    core.save(&path).unwrap();

    let expected_digest = core.document_state_digest().unwrap();
    let expected_editor = core.editor_state_frame().unwrap();
    let expected_journal = core.journal_entries().to_vec();
    let expected_state = core.journal_state().unwrap();
    assert!(
        expected_journal
            .iter()
            .any(|entry| matches!(entry, JournalEntry::BranchCut(_)))
    );

    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), expected_digest);
    assert_eq!(reopened.editor_state_frame().unwrap(), expected_editor);
    assert_eq!(reopened.journal_entries(), expected_journal);
    assert_eq!(reopened.journal_state(), Some(expected_state));
    assert!(!reopened.document_info().unwrap().dirty);
    reopened.undo().unwrap();
    reopened.redo().unwrap();

    let mut expected = core.clone();
    expected.undo().unwrap();
    expected.redo().unwrap();
    expected.undo().unwrap();
    reopened.undo().unwrap();
    let expected_layer = expected
        .create_layer(LayerKind::Raster, "post-reopen authority")
        .unwrap()
        .1;
    let reopened_layer = reopened
        .create_layer(LayerKind::Raster, "post-reopen authority")
        .unwrap()
        .1;
    assert_eq!(reopened_layer, expected_layer);
    assert_eq!(reopened.journal_entries(), expected.journal_entries());
    assert_eq!(
        reopened.document_state_digest().unwrap(),
        expected.document_state_digest().unwrap()
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn io_001_v2_and_corrupt_open_are_current_only_and_atomic_for_the_live_core() {
    let path = native_path("v2-rejected");
    let mut legacy = vec![0_u8; 32];
    legacy[0..8].copy_from_slice(b"INKPOD\0\0");
    legacy[8..12].copy_from_slice(&2_u32.to_le_bytes());
    fs::write(&path, legacy).unwrap();

    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&line_stroke(vec![StrokeSample {
        x: 1.0,
        y: 1.0,
        pressure: 1.0,
    }]))
    .unwrap();
    let before_info = core.document_info().unwrap();
    let before_digest = core.document_state_digest().unwrap();
    let before_editor = core.editor_state_frame().unwrap();
    let before_journal = core.journal_entries().to_vec();

    assert!(matches!(core.open(&path), Err(CoreError::Format(_))));
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.document_state_digest().unwrap(), before_digest);
    assert_eq!(core.editor_state_frame().unwrap(), before_editor);
    assert_eq!(core.journal_entries(), before_journal);
    fs::remove_file(path).unwrap();
}

#[test]
fn io_001_unknown_optional_section_round_trips_opaquely_through_core_save() {
    let first = native_path("opaque-input");
    let second = native_path("opaque-output");
    let mut core = Core::new();
    core.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.save(&first).unwrap();

    let mut file = read_procedure_file(&first).unwrap();
    let extension = NativeSection {
        fourcc: *b"VEND",
        schema_version: 7,
        flags: OPAQUE_PRESERVE,
        records: vec![NativeRecord {
            kind: 0x2222,
            schema_version: 9,
            flags: 0x0102_0304,
            payload: vec![0, 1, 2, 3, 0xfe, 0xff],
        }],
    };
    file.sections.push(extension.clone());
    save_procedure_file_atomic(&first, &file).unwrap();

    let mut reopened = Core::new();
    reopened.open(&first).unwrap();
    reopened.save(&second).unwrap();
    let round_trip = read_procedure_file(&second).unwrap();
    assert_eq!(
        round_trip
            .sections
            .iter()
            .find(|section| section.fourcc == *b"VEND"),
        Some(&extension)
    );
    fs::remove_file(first).unwrap();
    fs::remove_file(second).unwrap();
}

#[test]
fn io_001_failed_replace_does_not_publish_prospective_document_or_editor_savepoints() {
    let normal = native_path("save-before-replace-failure");
    let destination_directory = native_path("replace-failure-directory");
    fs::create_dir(&destination_directory).unwrap();

    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.save(&normal).unwrap();
    core.update_editor_state(
        core.editor_state().unwrap().revision,
        EditorStateUpdate::SetActiveTool(EditorTool::Eraser),
    )
    .unwrap();
    let before_info = core.document_info().unwrap();
    let before_editor = core.editor_state_frame().unwrap();
    let before_journal = core.journal_entries().to_vec();

    assert!(core.save(&destination_directory).is_err());
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.editor_state_frame().unwrap(), before_editor);
    assert_eq!(core.journal_entries(), before_journal);
    core.revert().unwrap();
    assert_eq!(
        core.editor_state().unwrap().state.active_tool,
        EditorTool::Pencil
    );

    fs::remove_file(normal).unwrap();
    fs::remove_dir(destination_directory).unwrap();
}

#[test]
fn io_001_checkpoint_is_optional_verified_and_exactly_equivalent_to_full_replay() {
    let checkpoint_path = native_path("v9-checkpoint");
    let replay_path = native_path("v9-full-replay");
    let epoch_mismatch_path = native_path("v9-checkpoint-epoch-mismatch");
    let prefix_mismatch_path = native_path("v9-checkpoint-prefix-mismatch");
    let state_mismatch_path = native_path("v9-checkpoint-state-mismatch");
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    commit_checkpoint_interval(&mut core);
    for _ in 0..64 {
        core.undo().unwrap();
    }
    core.set_main_line_color(PixelValue::Rgba([33, 44, 55, u8::MAX]))
        .unwrap();
    assert_eq!(core.persistence_info().unwrap().procedure_count, 257);
    assert!(core.persistence_info().unwrap().checkpoint_due);
    core.save(&checkpoint_path).unwrap();

    let expected_digest = core.document_state_digest().unwrap();
    let expected_editor = core.editor_state_frame().unwrap();
    let expected_journal = core.journal_entries().to_vec();
    let expected_state = core.journal_state().unwrap();
    let file = read_procedure_file(&checkpoint_path).unwrap();
    assert!(
        file.sections
            .iter()
            .any(|section| section.fourcc == *b"CKPT")
    );

    let mut checkpoint = Core::new();
    checkpoint.open(&checkpoint_path).unwrap();
    assert_eq!(
        checkpoint.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::Checkpoint
    );
    assert_eq!(checkpoint.document_state_digest().unwrap(), expected_digest);
    assert_eq!(checkpoint.editor_state_frame().unwrap(), expected_editor);
    assert_eq!(checkpoint.journal_entries(), expected_journal);
    assert_eq!(checkpoint.journal_state(), Some(expected_state));
    checkpoint.undo().unwrap();
    checkpoint.redo().unwrap();
    assert_eq!(checkpoint.document_state_digest().unwrap(), expected_digest);

    let mut replay_file = file.clone();
    replay_file
        .sections
        .retain(|section| section.fourcc != *b"CKPT");
    save_procedure_file_atomic(&replay_path, &replay_file).unwrap();
    let mut replay = Core::new();
    replay.open(&replay_path).unwrap();
    assert_eq!(
        replay.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert_eq!(replay.document_state_digest().unwrap(), expected_digest);
    assert_eq!(replay.editor_state_frame().unwrap(), expected_editor);
    assert_eq!(replay.journal_entries(), expected_journal);
    assert_eq!(replay.journal_state(), Some(expected_state));

    for (path, field) in [
        (&epoch_mismatch_path, 1_u32),
        (&prefix_mismatch_path, 4_u32),
        (&state_mismatch_path, 6_u32),
    ] {
        let mut mismatch_file = file.clone();
        let payload = checkpoint_payload_mut(&mut mismatch_file);
        let range = frame_field(payload, field);
        if field == 1 {
            payload[range].copy_from_slice(&(ReplayEpoch::CURRENT.get() + 1).to_le_bytes());
        } else {
            payload[range.start] ^= 0x80;
        }
        save_procedure_file_atomic(path, &mismatch_file).unwrap();
        let mut mismatch = Core::new();
        mismatch.open(path).unwrap();
        assert_eq!(
            mismatch.persistence_info().unwrap().open_strategy,
            NativeOpenStrategy::FullReplay
        );
        assert_eq!(mismatch.document_state_digest().unwrap(), expected_digest);
        assert_eq!(mismatch.journal_entries(), expected_journal);
    }

    fs::remove_file(checkpoint_path).unwrap();
    fs::remove_file(replay_path).unwrap();
    fs::remove_file(epoch_mismatch_path).unwrap();
    fs::remove_file(prefix_mismatch_path).unwrap();
    fs::remove_file(state_mismatch_path).unwrap();
}

#[test]
fn safe_001_malformed_or_hash_corrupt_checkpoint_rejects_without_live_publication() {
    let malformed_path = native_path("v9-malformed-checkpoint");
    let corrupt_path = native_path("v9-corrupt-checkpoint");
    let mut source = Core::new();
    source
        .new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    commit_checkpoint_interval(&mut source);
    source.save(&malformed_path).unwrap();

    let mut malformed = read_procedure_file(&malformed_path).unwrap();
    checkpoint_payload_mut(&mut malformed).clear();
    save_procedure_file_atomic(&malformed_path, &malformed).unwrap();

    let mut live = Core::new();
    live.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    live.set_main_line_color(PixelValue::Rgba([9, 8, 7, u8::MAX]))
        .unwrap();
    let before_info = live.document_info().unwrap();
    let before_digest = live.document_state_digest().unwrap();
    let before_journal = live.journal_entries().to_vec();
    assert!(matches!(
        live.open(&malformed_path),
        Err(CoreError::Format(_))
    ));
    assert_eq!(live.document_info().unwrap(), before_info);
    assert_eq!(live.document_state_digest().unwrap(), before_digest);
    assert_eq!(live.journal_entries(), before_journal);

    source.save(&corrupt_path).unwrap();
    let valid = read_procedure_file(&corrupt_path).unwrap();
    let needle = valid
        .sections
        .iter()
        .find(|section| section.fourcc == *b"CKPT")
        .unwrap()
        .records[0]
        .payload
        .clone();
    let mut bytes = fs::read(&corrupt_path).unwrap();
    let offset = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("checkpoint bytes in encoded file");
    bytes[offset + needle.len() - 1] ^= 0x80;
    fs::write(&corrupt_path, bytes).unwrap();
    assert!(matches!(
        live.open(&corrupt_path),
        Err(CoreError::Format(_))
    ));
    assert_eq!(live.document_info().unwrap(), before_info);
    assert_eq!(live.document_state_digest().unwrap(), before_digest);
    assert_eq!(live.journal_entries(), before_journal);

    fs::remove_file(malformed_path).unwrap();
    fs::remove_file(corrupt_path).unwrap();
}

#[test]
fn io_001_compaction_requires_an_exact_confirmation_token_and_never_mutates_live_history() {
    let normal_path = native_path("v9-compaction-source");
    let stale_path = native_path("v9-compaction-stale");
    let compact_path = native_path("v9-compaction-output");
    let mut core = Core::new();
    core.new_cell(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_main_line_color(PixelValue::Rgba([1, 2, 3, u8::MAX]))
        .unwrap();
    core.save(&normal_path).unwrap();
    let stale = core.compaction_plan().unwrap();
    core.update_editor_state(
        core.editor_state().unwrap().revision,
        EditorStateUpdate::SetActiveTool(EditorTool::Eraser),
    )
    .unwrap();
    assert!(matches!(
        core.write_compacted_copy(&stale_path, stale),
        Err(CoreError::InvalidState("compaction plan is stale"))
    ));
    assert!(!stale_path.exists());

    let expected_info = core.document_info().unwrap();
    let expected_digest = core.document_state_digest().unwrap();
    let expected_editor = core.editor_state_frame().unwrap();
    let expected_journal = core.journal_entries().to_vec();
    let plan = core.compaction_plan().unwrap();
    assert_eq!(plan.history_procedure_count, 1);
    core.write_compacted_copy(&compact_path, plan).unwrap();
    assert_eq!(core.document_info().unwrap(), expected_info);
    assert_eq!(core.document_state_digest().unwrap(), expected_digest);
    assert_eq!(core.editor_state_frame().unwrap(), expected_editor);
    assert_eq!(core.journal_entries(), expected_journal);

    let compact_file = read_procedure_file(&compact_path).unwrap();
    assert!(
        !compact_file
            .sections
            .iter()
            .any(|section| section.fourcc == *b"CKPT")
    );
    let mut compacted = Core::new();
    compacted.open(&compact_path).unwrap();
    let compacted_persistence = compacted.persistence_info().unwrap();
    assert_eq!(
        compacted_persistence.open_strategy,
        NativeOpenStrategy::FullReplay
    );
    assert_eq!(compacted_persistence.procedure_count, 0);
    assert_eq!(compacted_persistence.journal_event_count, 0);
    assert_eq!(compacted.document_state_digest().unwrap(), expected_digest);
    assert_eq!(compacted.editor_state_frame().unwrap(), expected_editor);
    assert!(compacted.undo().is_err());
    assert!(!compacted.document_info().unwrap().dirty);

    fs::remove_file(normal_path).unwrap();
    fs::remove_file(compact_path).unwrap();
}
