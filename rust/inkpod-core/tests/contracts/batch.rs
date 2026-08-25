use super::*;

fn sequence_source(
    name: &str,
    document_uuid: u128,
    source_generation: u64,
    pixels: Vec<u8>,
) -> SequenceCellSource {
    let raster = CommonRaster::new(2, 1, PixelFormat::StraightRgba8, None, None, pixels).unwrap();
    SequenceCellSource::from_common_raster_with_generation(
        name,
        document_uuid,
        source_generation,
        &raster,
    )
    .unwrap()
}

#[test]
fn exact_pair_extraction_remains_alpha_aware() {
    let old = sequence_source("A001", 0x101, 7, vec![10, 20, 30, 40, 1, 2, 3, 4]);
    let new = sequence_source("A002", 0x202, 9, vec![10, 20, 30, 41, 1, 2, 3, 4]);
    let mut core = Core::new();
    core.set_sequence(vec![new, old]).unwrap();
    let extraction = core
        .extract_batch_color_pairs(
            SequenceSourceIdentity {
                document_uuid: 0x101,
                source_generation: 7,
            },
            SequenceSourceIdentity {
                document_uuid: 0x202,
                source_generation: 9,
            },
        )
        .unwrap();
    assert_eq!(extraction.unchanged_pixel_count, 1);
    assert_eq!(extraction.candidates.len(), 1);
    assert_eq!(
        extraction.candidates[0].old,
        PixelValue::Rgba([10, 20, 30, 40])
    );
    assert_eq!(
        extraction.candidates[0].new,
        PixelValue::Rgba([10, 20, 30, 41])
    );
}

#[test]
fn new_tab_output_is_pathless_dirty_and_has_a_new_uuid() {
    let mut core = Core::new();
    core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let source_uuid = core.document_info().unwrap().document_uuid;
    let graph = BatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "new-tab".to_owned(),
        inputs: vec![BatchInputSelector::active_document()],
        operations: vec![BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            target: BatchTargetSelector::color_plane(),
            additional_targets: Vec::new(),
            kind: BatchOperationKind::Erase(vec![PixelValue::Rgba([1, 2, 3, 4])]),
        }],
        output: BatchOutputSettings {
            destination: BatchOutputDestination::NewTabs,
            ..BatchOutputSettings::default()
        },
    };
    let report = core
        .batch_execute_with_new_tab_capacity(
            &graph,
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: false,
                preview_confirmed: true,
            },
            1,
            |_, _| true,
        )
        .unwrap();
    assert_eq!(report.staged_results.len(), 1);
    assert_ne!(
        report.staged_results[0].document_uuid().unwrap(),
        source_uuid
    );
    assert!(report.staged_results[0].is_pathless());
    let staged = report
        .staged_results
        .into_iter()
        .next()
        .unwrap()
        .into_core();
    assert!(staged.document_info().unwrap().dirty);
}

fn no_op_operation() -> BatchOperation {
    BatchOperation {
        version: BATCH_OPERATION_VERSION,
        enabled: true,
        target: BatchTargetSelector::color_plane(),
        additional_targets: Vec::new(),
        kind: BatchOperationKind::Erase(vec![PixelValue::Rgba([1, 2, 3, 4])]),
    }
}

fn graph_with(inputs: Vec<BatchInputSelector>, output: BatchOutputSettings) -> BatchGraph {
    BatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "batch-v4-contract".to_owned(),
        inputs,
        operations: vec![no_op_operation()],
        output,
    }
}

fn batch_temp_directory(label: &str) -> PathBuf {
    let sequence = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "inkpod-batch-v4-{label}-{}-{sequence}",
        std::process::id()
    ))
}

#[test]
fn file_and_folder_inputs_cover_supported_formats_natural_order_ranges_and_errors() {
    let directory = batch_temp_directory("inputs");
    fs::create_dir_all(&directory).unwrap();
    let raster = CommonRaster::new(
        1,
        1,
        PixelFormat::StraightRgba8,
        Some(DEFAULT_DPI_MILLI),
        Some(DEFAULT_DPI_MILLI),
        vec![9, 8, 7, 6],
    )
    .unwrap();
    for (name, format) in [
        ("cell1.png", CommonRasterFormat::Png),
        ("cell2.bmp", CommonRasterFormat::Bmp),
        ("cell3.tga", CommonRasterFormat::Tga),
        ("cell4.tiff", CommonRasterFormat::Tiff),
    ] {
        fs::write(
            directory.join(name),
            encode_common_raster(format, &raster, false).unwrap(),
        )
        .unwrap();
    }
    let native_path = directory.join("cell10.inkpod");
    let mut native = Core::new();
    native
        .new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xb410)
        .unwrap();
    native.save(&native_path).unwrap();
    fs::write(directory.join("cell5.jpg"), b"unsupported").unwrap();
    fs::create_dir_all(directory.join("cell0.png")).unwrap();

    let folder = BatchInputSelector {
        kind: BatchInputKind::Folder,
        path: directory.to_string_lossy().into_owned(),
        first_cell: 0,
        last_cell: 0,
    };
    let output = BatchOutputSettings {
        destination: BatchOutputDestination::NewTabs,
        ..BatchOutputSettings::default()
    };
    let preview = native
        .batch_preview(
            &graph_with(vec![folder.clone()], output.clone()),
            BatchRunScope::All,
        )
        .unwrap();
    assert_eq!(
        preview
            .items
            .iter()
            .map(|item| item.input_name.as_str())
            .collect::<Vec<_>>(),
        [
            "cell1.png",
            "cell2.bmp",
            "cell3.tga",
            "cell4.tiff",
            "cell10.inkpod"
        ]
    );
    assert!(preview.items.iter().all(|item| item.warnings.is_empty()));

    let ranged = BatchInputSelector {
        first_cell: 2,
        last_cell: 4,
        ..folder.clone()
    };
    let ranged_preview = native
        .batch_preview(
            &graph_with(vec![ranged], output.clone()),
            BatchRunScope::All,
        )
        .unwrap();
    assert_eq!(
        ranged_preview
            .items
            .iter()
            .map(|item| item.input_name.as_str())
            .collect::<Vec<_>>(),
        ["cell2.bmp", "cell3.tga", "cell4.tiff"]
    );

    let duplicate = graph_with(
        vec![
            BatchInputSelector::file(native_path.to_string_lossy()),
            folder,
        ],
        output.clone(),
    );
    assert!(
        native
            .batch_preview(&duplicate, BatchRunScope::All)
            .is_err()
    );
    let unsupported = graph_with(
        vec![BatchInputSelector::file(
            directory.join("cell5.jpg").to_string_lossy(),
        )],
        output.clone(),
    );
    assert!(
        native
            .batch_preview(&unsupported, BatchRunScope::All)
            .is_err()
    );
    let missing = graph_with(
        vec![BatchInputSelector::file(
            directory.join("missing.png").to_string_lossy(),
        )],
        output,
    );
    assert!(native.batch_preview(&missing, BatchRunScope::All).is_err());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn folder_outputs_cover_every_public_format_and_preflight_collisions() {
    let directory = batch_temp_directory("outputs");
    let mut core = Core::new();
    core.new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xb420)
        .unwrap();
    for (index, (format, extension)) in [
        (BatchOutputFormat::Inkpod, "inkpod"),
        (BatchOutputFormat::Png, "png"),
        (BatchOutputFormat::Tiff, "tiff"),
        (BatchOutputFormat::Tga, "tga"),
        (BatchOutputFormat::Bmp, "bmp"),
    ]
    .into_iter()
    .enumerate()
    {
        let output = BatchOutputSettings {
            destination: BatchOutputDestination::Folder,
            format,
            folder: directory.to_string_lossy().into_owned(),
            naming_template: format!("result_{index}_{{index:2}}"),
            failure_policy: BatchFailurePolicy::Stop,
            wait_milliseconds: 0,
            preview_before_save: true,
        };
        let graph = graph_with(vec![BatchInputSelector::active_document()], output);
        let preview = core.batch_preview(&graph, BatchRunScope::All).unwrap();
        assert_eq!(preview.items.len(), 1);
        assert!(preview.items[0].warnings.is_empty());
        let report = core
            .batch_execute(
                &graph,
                BatchRunOptions {
                    scope: BatchRunScope::All,
                    dry_run: false,
                    preview_confirmed: true,
                },
                |_, _| true,
            )
            .unwrap();
        assert_eq!(report.items[0].outcome, BatchItemOutcome::Succeeded);
        let path = directory.join(format!("result_{index}_01.{extension}"));
        assert!(path.is_file());
        if format == BatchOutputFormat::Inkpod {
            let mut reopened = Core::new();
            reopened.open(&path).unwrap();
        } else {
            let common = match format {
                BatchOutputFormat::Png => CommonRasterFormat::Png,
                BatchOutputFormat::Tiff => CommonRasterFormat::Tiff,
                BatchOutputFormat::Tga => CommonRasterFormat::Tga,
                BatchOutputFormat::Bmp => CommonRasterFormat::Bmp,
                BatchOutputFormat::Inkpod => unreachable!(),
            };
            inkpod_format::decode_common_raster(common, &fs::read(path).unwrap()).unwrap();
        }
    }

    let collision_output = BatchOutputSettings {
        destination: BatchOutputDestination::Folder,
        format: BatchOutputFormat::Inkpod,
        folder: directory.to_string_lossy().into_owned(),
        naming_template: "collision".to_owned(),
        failure_policy: BatchFailurePolicy::Stop,
        wait_milliseconds: 0,
        preview_before_save: false,
    };
    let collision = graph_with(
        vec![
            BatchInputSelector::active_document(),
            BatchInputSelector::active_document(),
        ],
        collision_output,
    );
    let collision_preview = core.batch_preview(&collision, BatchRunScope::All).unwrap();
    assert_eq!(collision_preview.items.len(), 2);
    assert!(
        collision_preview.items[1]
            .warnings
            .iter()
            .any(|warning| warning.contains("same output path"))
    );
    assert!(
        core.batch_execute(
            &collision,
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: false,
                preview_confirmed: true,
            },
            |_, _| true,
        )
        .is_err()
    );

    let existing_path = directory.join("existing.inkpod");
    fs::write(&existing_path, b"existing destination must be preserved").unwrap();
    let existing = graph_with(
        vec![BatchInputSelector::active_document()],
        BatchOutputSettings {
            naming_template: "existing".to_owned(),
            ..collision.output.clone()
        },
    );
    assert!(
        core.batch_preview(&existing, BatchRunScope::All)
            .unwrap()
            .items[0]
            .warnings
            .iter()
            .any(|warning| warning.contains("already exists"))
    );
    assert!(
        core.batch_execute(
            &existing,
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: true,
                preview_confirmed: true,
            },
            |_, _| true,
        )
        .is_err()
    );
    assert_eq!(
        fs::read(existing_path).unwrap(),
        b"existing destination must be preserved"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn contact_sheet_preview_uses_complete_temporary_copies_and_publishes_one_clean_document() {
    let directory = batch_temp_directory("contact-sheet");
    fs::create_dir_all(&directory).unwrap();
    let first_path = directory.join("A001.inkpod");
    let second_path = directory.join("A002.inkpod");
    for (path, uuid) in [(&first_path, 0xb421_u128), (&second_path, 0xb422_u128)] {
        let mut source = Core::new();
        source
            .new_cell_with_uuid(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, uuid)
            .unwrap();
        source.save(path).unwrap();
    }
    let real_output = directory.join("real-output");
    let graph = graph_with(
        vec![
            BatchInputSelector::file(first_path.to_string_lossy()),
            BatchInputSelector::file(second_path.to_string_lossy()),
        ],
        BatchOutputSettings {
            destination: BatchOutputDestination::Folder,
            format: BatchOutputFormat::Png,
            folder: real_output.to_string_lossy().into_owned(),
            naming_template: "{stem}_real".to_owned(),
            failure_policy: BatchFailurePolicy::Continue,
            wait_milliseconds: 0,
            preview_before_save: true,
        },
    );
    let mut core = Core::new();
    core.new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xb423)
        .unwrap();
    let before = core.document_info().unwrap();
    let mut originals_replaced = false;
    let report = core
        .batch_contact_sheet_preview(&graph, |completed, _| {
            if completed == 2 && !originals_replaced {
                fs::write(&first_path, b"replaced after both inputs were copied").unwrap();
                fs::write(&second_path, b"replaced after both inputs were copied").unwrap();
                originals_replaced = true;
            }
            true
        })
        .unwrap();

    assert!(originals_replaced);
    assert_eq!(core.document_info().unwrap(), before);
    assert!(!real_output.exists());
    assert_eq!(report.items.len(), 2);
    assert!(
        report
            .items
            .iter()
            .all(|item| item.outcome == BatchItemOutcome::Succeeded)
    );
    assert_eq!(report.staged_results.len(), 1);
    assert!(report.staged_results[0].is_pathless());
    let staged = report
        .staged_results
        .into_iter()
        .next()
        .unwrap()
        .into_core();
    let info = staged.document_info().unwrap();
    assert_eq!((info.width, info.height), (352, 176));
    assert!(!info.dirty);

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn contact_sheet_preview_cancellation_removes_temporary_storage_and_returns_no_result() {
    let directory = batch_temp_directory("contact-sheet-cancel");
    fs::create_dir_all(&directory).unwrap();
    let input_path = directory.join("A001.inkpod");
    let mut source = Core::new();
    source
        .new_cell_with_uuid(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xb424)
        .unwrap();
    source.save(&input_path).unwrap();
    let graph = graph_with(
        vec![BatchInputSelector::file(input_path.to_string_lossy())],
        BatchOutputSettings {
            destination: BatchOutputDestination::NewTabs,
            ..BatchOutputSettings::default()
        },
    );
    let core = Core::new();
    assert!(matches!(
        core.batch_contact_sheet_preview(&graph, |completed, total| completed < total),
        Err(CoreError::Cancelled)
    ));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn active_output_is_one_dirty_undo_unit_and_new_tab_capacity_is_preflighted() {
    let mut core = Core::new();
    core.new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0xb430)
        .unwrap();
    core.apply_stroke(&Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::Color,
        color: [20, 30, 40, 50],
        diameter: 1.0,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![StrokeSample {
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
        }],
    })
    .unwrap();
    let save_path = batch_temp_directory("active-savepoint").with_extension("inkpod");
    core.save(&save_path).unwrap();
    let before = core.document_state_digest().unwrap();
    let history_before = core.history_entries().len();
    let graph = BatchGraph {
        operations: vec![BatchOperation {
            kind: BatchOperationKind::Erase(vec![PixelValue::Rgba([20, 30, 40, 50])]),
            ..no_op_operation()
        }],
        output: BatchOutputSettings {
            destination: BatchOutputDestination::ActiveDocument,
            ..BatchOutputSettings::default()
        },
        ..graph_with(
            vec![BatchInputSelector::active_document()],
            BatchOutputSettings::default(),
        )
    };
    core.batch_execute(
        &graph,
        BatchRunOptions {
            scope: BatchRunScope::All,
            dry_run: false,
            preview_confirmed: true,
        },
        |_, _| true,
    )
    .unwrap();
    assert!(core.document_info().unwrap().dirty);
    assert_eq!(core.history_entries().len(), history_before + 1);
    core.undo().unwrap();
    assert_eq!(core.document_state_digest().unwrap(), before);

    let new_tabs = BatchGraph {
        output: BatchOutputSettings {
            destination: BatchOutputDestination::NewTabs,
            ..BatchOutputSettings::default()
        },
        ..graph
    };
    let info_before_capacity = core.document_info().unwrap();
    assert!(
        core.batch_execute_with_new_tab_capacity(
            &new_tabs,
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: false,
                preview_confirmed: true,
            },
            0,
            |_, _| true,
        )
        .is_err()
    );
    assert_eq!(core.document_info().unwrap(), info_before_capacity);
    fs::remove_file(save_path).unwrap();
}

#[test]
fn masking_rejects_non_native_folder_outputs_and_unsafe_names() {
    let directory = batch_temp_directory("mask-output");
    for format in [
        BatchOutputFormat::Png,
        BatchOutputFormat::Tiff,
        BatchOutputFormat::Tga,
        BatchOutputFormat::Bmp,
    ] {
        let graph = BatchGraph {
            operations: vec![BatchOperation {
                kind: BatchOperationKind::Masking(vec![PixelValue::Rgba([1, 2, 3, 4])]),
                ..no_op_operation()
            }],
            ..graph_with(
                vec![BatchInputSelector::active_document()],
                BatchOutputSettings {
                    destination: BatchOutputDestination::Folder,
                    format,
                    folder: directory.to_string_lossy().into_owned(),
                    ..BatchOutputSettings::default()
                },
            )
        };
        assert!(graph.validate().is_err());
    }
    for naming_template in ["../escape", "fake.png", "{unknown}", "{index:0}"] {
        let graph = graph_with(
            vec![BatchInputSelector::active_document()],
            BatchOutputSettings {
                destination: BatchOutputDestination::Folder,
                folder: directory.to_string_lossy().into_owned(),
                naming_template: naming_template.to_owned(),
                ..BatchOutputSettings::default()
            },
        );
        assert!(graph.validate().is_err(), "accepted {naming_template}");
    }
}
