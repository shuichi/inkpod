use super::*;

fn metadata(name: &str) -> FileCutMetadata {
    FileCutMetadata {
        work_title: "Work".to_owned(),
        episode: "12".to_owned(),
        scene: "3".to_owned(),
        cut_name: name.to_owned(),
        instruction: "Keep the main line".to_owned(),
        duration_frames: 24,
    }
}

fn defaults() -> FileCutDefaults {
    FileCutDefaults {
        sizing_mode: 1,
        size_a: 1920,
        size_b: 1080,
        dpi_x_milli: 144_000,
        dpi_y_milli: 144_000,
        margin_milli: 50,
        safe_frame_ratio_milli: 900,
        maximum_close_ratio_milli: 500,
        anchor: 3,
        initial_layer_kind: 1,
        pixel_format: 5,
    }
}

fn descriptor() -> FileCutDescriptor {
    FileCutDescriptor {
        cut_id: 1,
        cut_uuid: 0x1234_u128.to_le_bytes(),
        current_state_id: 2,
        savepoint_state_id: 1,
        next_state_id: 3,
        next_procedure_id: 2,
        history_cursor: 1,
        genesis_metadata: metadata("C000"),
        genesis_defaults: defaults(),
        metadata: metadata("C001"),
        defaults: defaults(),
        members: vec![FileCutMember {
            cell_id: 7,
            document_uuid: 0x9876_u128.to_le_bytes(),
            display_number: 1,
            relative_path: "C001-0001.inkpod".to_owned(),
        }],
        active_history: vec![FileCutHistoryEntry {
            procedure_id: 1,
            base_state_id: 1,
            committed_state_id: 2,
            before_metadata: metadata("C000"),
            before_defaults: defaults(),
            after_metadata: metadata("C001"),
            after_defaults: defaults(),
        }],
        inactive_history: Vec::new(),
    }
}

#[test]
fn cut_descriptor_round_trips_current_version_and_rejects_noncurrent() {
    let expected = descriptor();
    let bytes = encode_cut_descriptor(&expected).unwrap();
    assert_eq!(decode_cut_descriptor(&bytes).unwrap(), expected);

    let mut old = bytes.clone();
    old[8..12].copy_from_slice(&FORMAT_VERSION.saturating_sub(1).to_le_bytes());
    assert!(matches!(
        decode_cut_descriptor(&old),
        Err(FormatError::Unsupported(_))
    ));

    let mut future = bytes;
    future[12..16].copy_from_slice(&(CUT_DESCRIPTOR_REPLAY_EPOCH + 1).to_le_bytes());
    assert!(matches!(
        decode_cut_descriptor(&future),
        Err(FormatError::Unsupported(_))
    ));
}

#[test]
fn cut_descriptor_rejects_duplicates_corruption_and_oversized_text() {
    let mut duplicate = descriptor();
    duplicate.members.push(duplicate.members[0].clone());
    assert!(matches!(
        encode_cut_descriptor(&duplicate),
        Err(FormatError::Invalid(_))
    ));

    let mut oversized = descriptor();
    oversized.metadata.instruction = "x".repeat(MAX_TEXT_BYTES + 1);
    assert!(matches!(
        encode_cut_descriptor(&oversized),
        Err(FormatError::Invalid(_))
    ));

    let mut corrupt = encode_cut_descriptor(&descriptor()).unwrap();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0x80;
    assert!(matches!(
        decode_cut_descriptor(&corrupt),
        Err(FormatError::Invalid(_))
    ));
}
