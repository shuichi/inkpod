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

#[test]
fn io_001_save_reopen_restores_full_journal_editor_and_all_next_id_authorities() {
    let path = native_path("v8-full-session");
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
