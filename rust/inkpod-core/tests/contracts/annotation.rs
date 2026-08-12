use super::*;

fn new_annotation_core() -> (Core, u64, u64) {
    let mut core = Core::new();
    core.new_cell(64, 48, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let (_, text_layer) = core.create_layer(LayerKind::Text, "Text").unwrap();
    let (_, note_layer) = core
        .create_layer(LayerKind::Annotation, "Instructions")
        .unwrap();
    (core, text_layer, note_layer)
}

fn text_input(layer_id: u64, text: &str, output: AnnotationOutput) -> AnnotationObjectInput {
    AnnotationObjectInput {
        layer_id,
        kind: AnnotationKind::Text,
        output,
        bounds: RectI32 {
            x: 4,
            y: 5,
            width: 36,
            height: 12,
        },
        font_family_hint: "Yu Gothic UI".to_owned(),
        font_size_milli: 12_000,
        style_flags: ANNOTATION_STYLE_BOLD,
        color: PixelValue::Rgba([24, 48, 96, 255]),
        text: text.to_owned(),
        points: Vec::new(),
        stroke_width_milli: 0,
    }
}

fn stroke_input(layer_id: u64, output: AnnotationOutput) -> AnnotationObjectInput {
    AnnotationObjectInput {
        layer_id,
        kind: AnnotationKind::Stroke,
        output,
        bounds: RectI32 {
            x: 3,
            y: 20,
            width: 24,
            height: 8,
        },
        font_family_hint: String::new(),
        font_size_milli: 0,
        style_flags: 0,
        color: PixelValue::Rgba16([65_535, 2_000, 2_000, 65_535]),
        text: String::new(),
        points: vec![
            AnnotationPoint {
                x_milli: 3_000,
                y_milli: 22_000,
            },
            AnnotationPoint {
                x_milli: 12_000,
                y_milli: 28_000,
            },
            AnnotationPoint {
                x_milli: 27_000,
                y_milli: 20_000,
            },
        ],
        stroke_width_milli: 1_500,
    }
}

#[test]
fn annotation_001_batch_edit_is_one_history_item_and_round_trips() {
    let (mut core, text_layer, note_layer) = new_annotation_core();
    let before = core.document_info().unwrap();
    let expected = before.document_revision;
    let outcome = core
        .edit_annotations(
            expected,
            &[
                AnnotationEdit::Create(text_input(
                    text_layer,
                    "仕上げ色: 青 — e\u{301}",
                    AnnotationOutput::Normal,
                )),
                AnnotationEdit::Create(stroke_input(note_layer, AnnotationOutput::Instruction)),
                AnnotationEdit::Create(AnnotationObjectInput {
                    layer_id: note_layer,
                    kind: AnnotationKind::Leader,
                    output: AnnotationOutput::Instruction,
                    bounds: RectI32 {
                        x: 30,
                        y: 18,
                        width: 20,
                        height: 10,
                    },
                    font_family_hint: String::new(),
                    font_size_milli: 0,
                    style_flags: 0,
                    color: PixelValue::Rgba([180, 20, 20, 255]),
                    text: String::new(),
                    points: vec![
                        AnnotationPoint {
                            x_milli: 30_000,
                            y_milli: 22_000,
                        },
                        AnnotationPoint {
                            x_milli: 48_000,
                            y_milli: 28_000,
                        },
                    ],
                    stroke_width_milli: 1_000,
                }),
                AnnotationEdit::Create(AnnotationObjectInput {
                    layer_id: text_layer,
                    kind: AnnotationKind::Value,
                    output: AnnotationOutput::Normal,
                    bounds: RectI32 {
                        x: 42,
                        y: 4,
                        width: 18,
                        height: 10,
                    },
                    font_family_hint: "Definitely Missing Font".to_owned(),
                    font_size_milli: 10_000,
                    style_flags: ANNOTATION_STYLE_ITALIC,
                    color: PixelValue::Rgba([0, 128, 0, 255]),
                    text: "R=24 G=48 B=96".to_owned(),
                    points: vec![
                        AnnotationPoint {
                            x_milli: 42_000,
                            y_milli: 14_000,
                        },
                        AnnotationPoint {
                            x_milli: 36_000,
                            y_milli: 20_000,
                        },
                    ],
                    stroke_width_milli: 1_000,
                }),
            ],
        )
        .unwrap();
    assert_eq!(outcome.revision(), expected + 1);
    assert_eq!(outcome.created_object_ids().len(), 4);
    let objects = core.annotation_objects().unwrap();
    assert_eq!(objects.len(), 4);
    assert_eq!(core.build_snapshot().annotations().len(), 4);
    let expected_digest = core.document_state_digest().unwrap();

    core.undo().unwrap();
    assert!(core.annotation_objects().unwrap().is_empty());
    core.redo().unwrap();
    assert_eq!(core.annotation_objects().unwrap(), objects);
    assert_eq!(core.document_state_digest().unwrap(), expected_digest);
    core.verify_journal_replay().unwrap();

    let path = std::env::temp_dir().join(format!(
        "inkpod-annotation-roundtrip-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.annotation_objects().unwrap(), objects);
    assert_eq!(reopened.document_state_digest().unwrap(), expected_digest);
    reopened.verify_journal_replay().unwrap();
    fs::remove_file(path).unwrap();
}

#[test]
fn annotation_001_utf8_limits_invalid_stale_noop_and_overflow_are_atomic() {
    let (mut core, text_layer, _) = new_annotation_core();
    let revision = core.document_info().unwrap().document_revision;
    let max_text = "x".repeat(MAX_ANNOTATION_TEXT_BYTES);
    let created = core
        .edit_annotations(
            revision,
            &[AnnotationEdit::Create(text_input(
                text_layer,
                &max_text,
                AnnotationOutput::Normal,
            ))],
        )
        .unwrap();
    let object_id = created.created_object_ids()[0];
    let stable = core.annotation_objects().unwrap();
    let stable_info = core.document_info().unwrap();

    let invalid_inputs = [
        text_input(text_layer, "", AnnotationOutput::Normal),
        text_input(
            text_layer,
            &"x".repeat(MAX_ANNOTATION_TEXT_BYTES + 1),
            AnnotationOutput::Normal,
        ),
        AnnotationObjectInput {
            bounds: RectI32 {
                x: 0,
                y: 0,
                width: 0,
                height: 10,
            },
            ..text_input(text_layer, "bad bounds", AnnotationOutput::Normal)
        },
        AnnotationObjectInput {
            kind: AnnotationKind::Stroke,
            text: String::new(),
            points: vec![AnnotationPoint {
                x_milli: 0,
                y_milli: 0,
            }],
            stroke_width_milli: 1_000,
            ..text_input(text_layer, "", AnnotationOutput::Normal)
        },
    ];
    for input in invalid_inputs {
        assert!(
            core.edit_annotations(
                stable_info.document_revision,
                &[AnnotationEdit::Create(input)]
            )
            .is_err()
        );
        assert_eq!(core.annotation_objects().unwrap(), stable);
        assert_eq!(core.document_info().unwrap(), stable_info);
    }
    assert!(
        core.edit_annotations(
            stable_info.document_revision - 1,
            &[AnnotationEdit::Delete { object_id }]
        )
        .is_err()
    );
    let noop = core
        .edit_annotations(
            stable_info.document_revision,
            &[AnnotationEdit::Move {
                object_id,
                delta_x: 0,
                delta_y: 0,
            }],
        )
        .unwrap();
    assert_eq!(noop.revision(), stable_info.document_revision);
    assert!(
        core.edit_annotations(
            stable_info.document_revision,
            &[AnnotationEdit::Move {
                object_id,
                delta_x: i32::MAX,
                delta_y: i32::MAX,
            }],
        )
        .is_err()
    );
    assert_eq!(core.annotation_objects().unwrap(), stable);
    assert_eq!(core.document_info().unwrap(), stable_info);

    assert!(
        core.edit_annotations(
            stable_info.document_revision,
            &[
                AnnotationEdit::Create(text_input(
                    text_layer,
                    "must not commit",
                    AnnotationOutput::Normal,
                )),
                AnnotationEdit::Delete {
                    object_id: u64::MAX,
                },
            ],
        )
        .is_err()
    );
    assert_eq!(core.annotation_objects().unwrap(), stable);
    assert_eq!(core.document_info().unwrap(), stable_info);

    let second = core
        .edit_annotations(
            stable_info.document_revision,
            &[AnnotationEdit::Create(text_input(
                text_layer,
                "second",
                AnnotationOutput::Normal,
            ))],
        )
        .unwrap();
    assert_eq!(second.created_object_ids(), &[object_id + 1]);
    let mut revision = second.revision();
    let mut replacement = text_input(text_layer, "updated", AnnotationOutput::Instruction);
    replacement.bounds.x = 8;
    revision = core
        .edit_annotations(
            revision,
            &[AnnotationEdit::Update {
                object_id,
                input: replacement,
            }],
        )
        .unwrap()
        .revision();
    revision = core
        .edit_annotations(
            revision,
            &[AnnotationEdit::Move {
                object_id,
                delta_x: 1,
                delta_y: 2,
            }],
        )
        .unwrap()
        .revision();
    let moved = core
        .annotation_objects()
        .unwrap()
        .into_iter()
        .find(|object| object.id == object_id)
        .unwrap();
    assert_eq!(moved.text, "updated");
    assert_eq!((moved.bounds.x, moved.bounds.y), (9, 7));
    core.edit_annotations(
        revision,
        &[
            AnnotationEdit::Delete { object_id },
            AnnotationEdit::Delete {
                object_id: second.created_object_ids()[0],
            },
        ],
    )
    .unwrap();
    assert!(core.annotation_objects().unwrap().is_empty());
    core.undo().unwrap();
    assert_eq!(core.annotation_objects().unwrap().len(), 2);
    core.redo().unwrap();
    assert!(core.annotation_objects().unwrap().is_empty());
}

#[test]
fn annotation_001_stroke_cancel_and_end_are_transient_and_atomic() {
    let (mut core, _, note_layer) = new_annotation_core();
    let revision = core.document_info().unwrap().document_revision;
    let start = AnnotationPoint {
        x_milli: 2_000,
        y_milli: 3_000,
    };
    core.begin_annotation_stroke(
        revision,
        note_layer,
        AnnotationOutput::Instruction,
        PixelValue::Rgba([200, 30, 20, 255]),
        1_250,
        start,
    )
    .unwrap();
    core.append_annotation_stroke(&[AnnotationPoint {
        x_milli: 8_000,
        y_milli: 9_000,
    }])
    .unwrap();
    assert_eq!(core.document_info().unwrap().document_revision, revision);
    assert_eq!(core.build_snapshot().annotations().len(), 1);
    core.cancel_annotation_stroke().unwrap();
    assert!(core.annotation_objects().unwrap().is_empty());
    assert_eq!(core.document_info().unwrap().document_revision, revision);

    core.begin_annotation_stroke(
        revision,
        note_layer,
        AnnotationOutput::Instruction,
        PixelValue::Rgba([200, 30, 20, 255]),
        1_250,
        start,
    )
    .unwrap();
    core.append_annotation_stroke(&[
        AnnotationPoint {
            x_milli: 8_000,
            y_milli: 9_000,
        },
        AnnotationPoint {
            x_milli: 12_000,
            y_milli: 4_000,
        },
    ])
    .unwrap();
    let committed = core.end_annotation_stroke().unwrap();
    assert_eq!(committed.revision(), revision + 1);
    assert_eq!(core.annotation_objects().unwrap().len(), 1);
    core.undo().unwrap();
    assert!(core.annotation_objects().unwrap().is_empty());
    core.redo().unwrap();
    assert_eq!(core.annotation_objects().unwrap().len(), 1);
}

#[test]
fn annotation_001_instruction_is_excluded_but_normal_text_is_flattened() {
    let (mut core, text_layer, note_layer) = new_annotation_core();
    let baseline = core
        .export_common_raster(CommonRasterFormat::Png, false)
        .unwrap();
    let revision = core.document_info().unwrap().document_revision;
    core.edit_annotations(
        revision,
        &[AnnotationEdit::Create(stroke_input(
            note_layer,
            AnnotationOutput::Instruction,
        ))],
    )
    .unwrap();
    let instruction_only = core
        .export_common_raster(CommonRasterFormat::Png, false)
        .unwrap();
    assert_eq!(instruction_only, baseline);

    let revision = core.document_info().unwrap().document_revision;
    core.edit_annotations(
        revision,
        &[AnnotationEdit::Create(text_input(
            text_layer,
            "Normal output",
            AnnotationOutput::Normal,
        ))],
    )
    .unwrap();
    let with_text = core
        .export_common_raster(CommonRasterFormat::Png, false)
        .unwrap();
    assert_ne!(with_text, baseline);
    assert!(
        core.layer_thumbnail(text_layer, 32, 24)
            .unwrap()
            .pixels
            .iter()
            .any(|byte| *byte != 0)
    );
}
