//! Current native container v33 and replay epoch 28 public persistence contracts.

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
fn retired_cut_magic_is_rejected_without_replacing_the_cell() {
    let mut core = Core::new();
    core.new_cell(32, 24, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let path = native_path("retired-cut");
    core.save(&path).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    bytes[..8].copy_from_slice(b"INKCUT\0\0");
    fs::write(&path, &bytes).unwrap();
    let stable = core.document_info().unwrap();
    let pixels = core
        .export_common_raster(CommonRasterFormat::Png, false)
        .unwrap();
    assert!(core.open(&path).is_err());
    assert_eq!(core.document_info().unwrap(), stable);
    assert_eq!(
        core.export_common_raster(CommonRasterFormat::Png, false)
            .unwrap(),
        pixels
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_file(path).unwrap();
}

#[test]
fn io_001_imported_genesis_source_rejects_wrong_asset_plane_and_underlay() {
    let mut alpha_pixels = vec![255; 16];
    alpha_pixels[3] = 254;
    let raster =
        CommonRaster::new(2, 2, PixelFormat::StraightRgba8, None, None, alpha_pixels).unwrap();
    let mut core = Core::new();
    let info = core
        .import_decoded_common_raster(CommonRasterFormat::Tga, &raster, 0x3001)
        .unwrap();
    let (native, _) = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(false, || false)
        .unwrap();
    for mutation in 0..4 {
        let mut invalid = native.clone();
        let payload = &mut invalid
            .sections
            .iter_mut()
            .find(|section| section.fourcc == *b"GENS")
            .unwrap()
            .records[0]
            .payload;
        let source = frame_field(payload, 6);
        assert_eq!(source.len(), 40);
        match mutation {
            0 => payload[source.start..source.start + 8].copy_from_slice(&0_u64.to_le_bytes()),
            1 => payload[source.start..source.start + 8]
                .copy_from_slice(&info.color_plane_id.to_le_bytes()),
            2 => payload[source.start + 8..source.end].fill(0),
            3 => {
                let archive = frame_field(payload, 4);
                payload[archive.start] = 1; // Any non-opaque source alpha requires transparency.
            }
            _ => unreachable!(),
        }
        assert!(
            Core::from_native_file(invalid, false).is_err(),
            "mutation {mutation}"
        );
    }
    let mut without_source = native.clone();
    let payload = &mut without_source
        .sections
        .iter_mut()
        .find(|section| section.fourcc == *b"GENS")
        .unwrap()
        .records[0]
        .payload;
    let source = frame_field(payload, 6);
    payload[source.start - 12] = 0;
    payload[source.start - 8..source.start].fill(0);
    payload.drain(source);
    assert!(Core::from_native_file(without_source, false).is_err());
    assert_eq!(core.document_info().unwrap(), info);

    let opaque_raster =
        CommonRaster::new(2, 2, PixelFormat::StraightRgba8, None, None, vec![255; 16]).unwrap();
    let mut opaque_core = Core::new();
    opaque_core
        .import_decoded_common_raster(CommonRasterFormat::Tga, &opaque_raster, 0x3002)
        .unwrap();
    assert_eq!(
        opaque_core.genesis_info().unwrap().base_surface,
        BaseSurface::SolidWhite
    );
    let (mut wrong_opaque_underlay, _) = opaque_core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(false, || false)
        .unwrap();
    let payload = &mut wrong_opaque_underlay
        .sections
        .iter_mut()
        .find(|section| section.fourcc == *b"GENS")
        .unwrap()
        .records[0]
        .payload;
    let archive = frame_field(payload, 4);
    payload[archive.start] = 3; // Fully opaque source requires a solid-white underlay.
    assert!(Core::from_native_file(wrong_opaque_underlay, false).is_err());

    let encoded = inkpod_format::encode_procedure_file(&native).unwrap();
    assert_eq!(u32::from_le_bytes(encoded[8..12].try_into().unwrap()), 33);
    let mut previous = encoded;
    previous[8..12].copy_from_slice(&32_u32.to_le_bytes());
    assert!(inkpod_format::decode_procedure_file(&previous).is_err());
}

#[test]
fn io_001_native_raster_format_is_persisted_without_changing_pixel_asset_identity() {
    let mut core = Core::new();
    core.new_cell(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    assert_eq!(core.raster_file_format().unwrap(), CommonRasterFormat::Png);
    let before = core.document_info().unwrap();
    core.set_new_cell_raster_format(CommonRasterFormat::Tiff);
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.raster_file_format().unwrap(), CommonRasterFormat::Png);
    core.new_cell(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    assert_eq!(core.raster_file_format().unwrap(), CommonRasterFormat::Tiff);

    let raster = inkpod_format::CommonRaster::new(
        2,
        1,
        PixelFormat::StraightRgba8,
        Some(DEFAULT_DPI_MILLI),
        Some(DEFAULT_DPI_MILLI),
        vec![21, 42, 63, 255, 17, 29, 47, 127],
    )
    .unwrap();
    let mut first_asset = None;
    for format in [
        CommonRasterFormat::Png,
        CommonRasterFormat::Tiff,
        CommonRasterFormat::Tga,
        CommonRasterFormat::Bmp,
    ] {
        let encoded = inkpod_format::encode_common_raster(format, &raster, false).unwrap();
        core.import_common_raster(format, &encoded, 0x2901).unwrap();
        let asset = core.genesis_info().unwrap().base_surface;
        if let Some(expected) = first_asset {
            assert_eq!(asset, expected);
        } else {
            first_asset = Some(asset);
        }
        let state = core.document_info().unwrap();
        let prepared = core
            .capture_document_save()
            .unwrap()
            .prepare_normal_save(|| false)
            .unwrap();
        assert_eq!(core.document_info().unwrap(), state);
        let exported = core
            .capture_document_save()
            .unwrap()
            .prepare_raster_export(format, true, || false)
            .unwrap();
        assert_eq!(exported, core.export_common_raster(format, true).unwrap());
        assert_eq!(core.document_info().unwrap(), state);
        let (native, output_format, output, token) = prepared.into_parts();
        assert_eq!(output_format, format);
        assert_eq!(
            inkpod_format::decode_common_raster(format, &output)
                .unwrap()
                .pixels,
            raster.pixels
        );
        core.validate_document_save(&token).unwrap();
        let reopened = Core::from_native_file(native.clone(), false).unwrap();
        assert_eq!(reopened.raster_file_format().unwrap(), format);
        assert_eq!(reopened.genesis_info().unwrap().base_surface, asset);
        let recovered = Core::from_native_file(native.clone(), true).unwrap();
        assert_eq!(recovered.raster_file_format().unwrap(), format);
        assert!(recovered.document_info().unwrap().dirty);
        assert!(recovered.document_info().unwrap().recovered);

        let mut invalid = native;
        let payload = &mut invalid
            .sections
            .iter_mut()
            .find(|section| section.fourcc == *b"META")
            .unwrap()
            .records[0]
            .payload;
        let field = frame_field(payload, 21);
        payload[field].copy_from_slice(&99_u32.to_le_bytes());
        assert!(Core::from_native_file(invalid, false).is_err());
    }
}

#[test]
fn io_001_detached_file_tokens_reject_stale_cross_core_and_duplicate_publication() {
    let mut core = Core::new();
    core.new_cell_with_uuid(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x2902)
        .unwrap();
    let snapshot = core.capture_document_save().unwrap();
    let (native, token) = snapshot.prepare_native_save(false, || false).unwrap();
    let mut other = Core::new();
    other
        .new_cell_with_uuid(4, 4, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x2902)
        .unwrap();
    assert!(other.validate_document_save(&token).is_err());
    let unchanged = core.document_info().unwrap();
    let destination = native_path("prepared-save");
    core.commit_document_save(token.clone(), &destination)
        .unwrap();
    assert_eq!(
        core.document_info().unwrap().document_revision,
        unchanged.document_revision
    );
    assert!(!core.document_info().unwrap().dirty);
    assert!(core.commit_document_save(token, &destination).is_err());

    let open_token = core.capture_document_open().unwrap();
    let pending_save = core
        .capture_document_save()
        .unwrap()
        .prepare_native_save(false, || false)
        .unwrap()
        .1;
    core.adopt_opened_document(
        open_token,
        Core::from_native_file(native.clone(), false).unwrap(),
        Some(&destination),
    )
    .unwrap();
    assert!(core.validate_document_save(&pending_save).is_err());
    let stale_open = core.capture_document_open().unwrap();
    core.set_main_line_color(PixelValue::Rgba([8, 9, 10, 255]))
        .unwrap();
    let edited = core.document_info().unwrap();
    assert!(
        core.adopt_opened_document(
            stale_open,
            Core::from_native_file(native, false).unwrap(),
            Some(&destination)
        )
        .is_err()
    );
    assert_eq!(core.document_info().unwrap(), edited);
    assert!(matches!(
        core.capture_document_save()
            .unwrap()
            .prepare_normal_save(|| true),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(core.document_info().unwrap(), edited);
}

#[test]
fn io_001_normal_raster_save_rejects_precision_loss_and_preserves_creation_defaults_on_open() {
    let exact_pixels: Vec<_> = [1_u16, 1_001, 32_767, 65_535, 257, 258, 65_534, 50_000]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let exact_raster = inkpod_format::CommonRaster::new(
        2,
        1,
        PixelFormat::StraightRgba16,
        None,
        None,
        exact_pixels.clone(),
    )
    .unwrap();
    for format in [CommonRasterFormat::Png, CommonRasterFormat::Tiff] {
        let encoded = inkpod_format::encode_common_raster(format, &exact_raster, false).unwrap();
        let mut core = Core::new();
        core.import_common_raster(format, &encoded, 0x2904).unwrap();
        let (native, output_format, output, _) = core
            .capture_document_save()
            .unwrap()
            .prepare_normal_save(|| false)
            .unwrap()
            .into_parts();
        assert_eq!(output_format, format);
        let output = inkpod_format::decode_common_raster(format, &output).unwrap();
        assert_eq!(output.info.pixel_format, PixelFormat::StraightRgba16);
        assert_eq!(output.pixels, exact_pixels);
        let reopened = Core::from_native_file(native, false).unwrap();
        assert_eq!(reopened.raster_file_format().unwrap(), format);
    }
    for format in [CommonRasterFormat::Tga, CommonRasterFormat::Bmp] {
        let mut core = Core::new();
        core.set_new_cell_raster_format(format);
        core.new_cell_from_raster_asset(
            RasterAssetInput {
                width: 1,
                height: 1,
                pixel_format: PixelFormat::StraightRgba16,
                color_space: Some(AssetColorSpace::Srgb),
                alpha_semantics: AssetAlphaSemantics::Straight,
                canonical_stride: 8,
                pixels: [1001_u16, 2002, 3003, 65535]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
                expected_id: None,
            },
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            0x2903,
        )
        .unwrap();
        let before = core.document_info().unwrap();
        assert!(
            core.capture_document_save()
                .unwrap()
                .prepare_normal_save(|| false)
                .is_err()
        );
        assert_eq!(core.document_info().unwrap(), before);
        let native = core
            .capture_document_save()
            .unwrap()
            .prepare_native_save(false, || false)
            .unwrap()
            .0;
        let token = core.capture_document_open().unwrap();
        core.set_new_cell_raster_format(CommonRasterFormat::Tiff);
        core.adopt_opened_document(token, Core::from_native_file(native, true).unwrap(), None)
            .unwrap();
        assert_eq!(core.raster_file_format().unwrap(), format);
        core.new_cell(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        assert_eq!(core.raster_file_format().unwrap(), CommonRasterFormat::Tiff);
    }
    fn require_send<T: Send>() {}
    require_send::<inkpod_core::DocumentSaveSnapshot>();
    require_send::<inkpod_core::PreparedDocumentSave>();
}

#[test]
fn io_001_save_reopen_restores_full_journal_editor_and_all_next_id_authorities() {
    let path = native_path("v14-full-session");
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
    let expected_layer = expected.create_layer("post-reopen authority").unwrap().1;
    let reopened_layer = reopened.create_layer("post-reopen authority").unwrap().1;
    assert_eq!(reopened_layer, expected_layer);
    assert_eq!(reopened.journal_entries(), expected.journal_entries());
    assert_eq!(
        reopened.document_state_digest().unwrap(),
        expected.document_state_digest().unwrap()
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn io_001_v33_rejects_v32_and_corrupt_open_is_atomic_for_the_live_core() {
    let path = native_path("v32-rejected");
    let mut legacy = vec![0_u8; 128];
    legacy[0..8].copy_from_slice(b"INKPOD\0\0");
    legacy[8..12].copy_from_slice(&(inkpod_format::FORMAT_VERSION - 1).to_le_bytes());
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
fn io_001_clear_selected_content_journal_supports_save_autosave_and_reopen() {
    let normal_path = native_path("v14-clear-selected-normal");
    let recovery_path = native_path("v14-clear-selected-recovery");
    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.apply_stroke(&color_stroke(
        PaintTool::Pencil,
        1.0,
        StrokeSample {
            x: 2.0,
            y: 3.0,
            pressure: 1.0,
        },
    ))
    .unwrap();
    core.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 2,
            y: 3,
            width: 1,
            height: 1,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    core.clear_selected_content().unwrap();
    assert!(
        core.plane_pixel(ActivePlane::Color, 2, 3)
            .unwrap()
            .is_zero()
    );

    core.verify_journal_replay().unwrap();
    core.autosave(&recovery_path).unwrap();
    core.save(&normal_path).unwrap();

    let mut reopened = Core::new();
    reopened.open(&normal_path).unwrap();
    assert!(
        reopened
            .plane_pixel(ActivePlane::Color, 2, 3)
            .unwrap()
            .is_zero()
    );
    reopened.undo().unwrap();
    assert_eq!(
        reopened.plane_pixel(ActivePlane::Color, 2, 3).unwrap(),
        PixelValue::Rgba([12, 34, 56, 255])
    );
    reopened.redo().unwrap();
    reopened.verify_journal_replay().unwrap();

    let mut recovered = Core::new();
    recovered.open_recovery(&recovery_path).unwrap();
    assert!(recovered.document_info().unwrap().recovered);
    assert!(
        recovered
            .plane_pixel(ActivePlane::Color, 2, 3)
            .unwrap()
            .is_zero()
    );
    recovered.undo().unwrap();
    recovered.redo().unwrap();
    recovered.verify_journal_replay().unwrap();

    fs::remove_file(normal_path).unwrap();
    fs::remove_file(recovery_path).unwrap();
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
    let checkpoint_path = native_path("v14-checkpoint");
    let replay_path = native_path("v14-full-replay");
    let epoch_mismatch_path = native_path("v14-checkpoint-epoch-mismatch");
    let prefix_mismatch_path = native_path("v14-checkpoint-prefix-mismatch");
    let state_mismatch_path = native_path("v14-checkpoint-state-mismatch");
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
    let malformed_path = native_path("v14-malformed-checkpoint");
    let corrupt_path = native_path("v14-corrupt-checkpoint");
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
    let normal_path = native_path("v14-compaction-source");
    let stale_path = native_path("v14-compaction-stale");
    let compact_path = native_path("v14-compaction-output");
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
    assert!(matches!(
        core.capture_compacted_copy(plan)
            .unwrap()
            .prepare_compacted_copy(plan, || true),
        Err(CoreError::Cancelled)
    ));
    let captured = core.capture_compacted_copy(plan).unwrap();
    let (detached_file, token) =
        std::thread::spawn(move || captured.prepare_compacted_copy(plan, || false))
            .join()
            .unwrap()
            .unwrap();
    core.validate_document_save(&token).unwrap();
    core.write_compacted_copy(&compact_path, plan).unwrap();
    assert_eq!(core.document_info().unwrap(), expected_info);
    assert_eq!(core.document_state_digest().unwrap(), expected_digest);
    assert_eq!(core.editor_state_frame().unwrap(), expected_editor);
    assert_eq!(core.journal_entries(), expected_journal);

    let compact_file = read_procedure_file(&compact_path).unwrap();
    assert_eq!(
        inkpod_format::encode_procedure_file(&detached_file).unwrap(),
        inkpod_format::encode_procedure_file(&compact_file).unwrap()
    );
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

    let mut invalid_plan = plan;
    invalid_plan.journal_digest[0] ^= 1;
    assert!(matches!(
        core.capture_compacted_copy(invalid_plan)
            .unwrap()
            .prepare_compacted_copy(invalid_plan, || false),
        Err(CoreError::InvalidState("compaction plan is stale"))
    ));
    core.set_main_line_color(PixelValue::Rgba([4, 5, 6, u8::MAX]))
        .unwrap();
    assert!(core.validate_document_save(&token).is_err());
    assert!(matches!(
        core.capture_compacted_copy(plan),
        Err(CoreError::InvalidState("compaction plan is stale"))
    ));

    fs::remove_file(normal_path).unwrap();
    fs::remove_file(compact_path).unwrap();
}
