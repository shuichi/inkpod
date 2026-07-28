use super::*;
use crate::{
    Channel, ColorBalance, CurveInterpolation, CurvePoint, DustMode, HsvAdjustment, Levels,
    PaintTool, Stroke, StrokeSample,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn temp_directory(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "inkpod-test-{label}-{}-{}",
        std::process::id(),
        TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn saved_cell(path: &Path, color: [u8; 4]) {
    let mut core = Core::new();
    core.new_cell(4, 4, 96_000, 96_000).unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.apply_stroke(&Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::Color,
        color,
        diameter: 1.0,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: crate::CoordinateSpace::Document,
        samples: vec![StrokeSample {
            x: 1.0,
            y: 1.0,
            pressure: 1.0,
        }],
    })
    .unwrap();
    core.save(path).unwrap();
}

fn replace_graph(input: &Path, output: &Path) -> BatchGraph {
    BatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "replace-set".to_owned(),
        inputs: vec![BatchInputSelector::file(input.to_string_lossy())],
        operations: vec![BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(BatchTargetSelector::color_plane()),
            kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([10, 20, 30, 255]),
                new: PixelValue::Rgba([30, 20, 10, 255]),
            }]),
        }],
        output: BatchOutputSettings {
            folder: output.to_string_lossy().into_owned(),
            ..BatchOutputSettings::default()
        },
    }
}

#[test]
fn acceptance_dry_run_writes_no_files() {
    let directory = temp_directory("dry-run");
    let input = directory.join("cell1.inkpod");
    let output = directory.join("new-output");
    saved_cell(&input, [10, 20, 30, 255]);
    let core = Core::new();
    let report = core
        .batch_execute(
            &replace_graph(&input, &output),
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: true,
                preview_confirmed: true,
            },
            |_, _| true,
        )
        .unwrap();
    assert_eq!(report.items[0].outcome, BatchItemOutcome::DryRun);
    assert!(!output.exists());
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acceptance_default_output_never_overwrites_input() {
    let directory = temp_directory("default-output");
    let input = directory.join("cell1.inkpod");
    saved_cell(&input, [10, 20, 30, 255]);
    let original = fs::read(&input).unwrap();
    let mut graph = replace_graph(&input, &directory);
    graph.output.folder.clear();
    let report = Core::new()
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
    let output = directory.join("cell1_batch.inkpod");
    assert_eq!(
        report.items[0].output_path.as_deref(),
        Some(output.as_path())
    );
    assert!(output.exists());
    assert_eq!(fs::read(&input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acceptance_cancelled_file_leaves_no_temporary_output() {
    let directory = temp_directory("cancel");
    let input = directory.join("cell1.inkpod");
    let output = directory.join("output");
    saved_cell(&input, [10, 20, 30, 255]);
    let mut save_polls = 0_u32;
    let report = Core::new()
        .batch_execute(
            &replace_graph(&input, &output),
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: false,
                preview_confirmed: true,
            },
            |completed, total| {
                if completed + 1 == total {
                    save_polls += 1;
                    return save_polls < 2;
                }
                true
            },
        )
        .unwrap();
    assert!(report.cancelled);
    assert!(!output.join("cell1_batch.inkpod").exists());
    if output.exists() {
        assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
    }
    assert!(fs::read_dir(&directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".inkpod.tmp.")
    }));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acceptance_failure_policy_records_and_continues_or_stops() {
    let directory = temp_directory("failure-policy");
    let good = directory.join("cell2.inkpod");
    let bad = directory.join("cell1.inkpod");
    saved_cell(&good, [10, 20, 30, 255]);
    fs::write(&bad, b"not an inkpod document").unwrap();
    let output = directory.join("out");
    let mut graph = replace_graph(&good, &output);
    graph.inputs = vec![BatchInputSelector {
        kind: BatchInputKind::Folder,
        path: directory.to_string_lossy().into_owned(),
        first_cell: 0,
        last_cell: 0,
    }];
    let continued = Core::new()
        .batch_execute(
            &graph,
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: true,
                preview_confirmed: true,
            },
            |_, _| true,
        )
        .unwrap();
    assert_eq!(continued.items.len(), 2);
    assert_eq!(continued.failure_count(), 1);
    assert_eq!(continued.items[0].outcome, BatchItemOutcome::Failed);
    assert_eq!(continued.items[1].outcome, BatchItemOutcome::DryRun);

    graph.output.failure_policy = BatchFailurePolicy::Stop;
    let stopped = Core::new()
        .batch_execute(
            &graph,
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: true,
                preview_confirmed: true,
            },
            |_, _| true,
        )
        .unwrap();
    assert_eq!(stopped.items.len(), 1);
    assert_eq!(stopped.failure_count(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acceptance_color_replacement_swap_round_trips() {
    let directory = temp_directory("replacement-roundtrip");
    let input = directory.join("cell1.inkpod");
    let settings = directory.join("replace.inkbatch");
    saved_cell(&input, [10, 20, 30, 255]);
    let mut graph = replace_graph(&input, &directory);
    graph.operations[0].swap_color_replacements().unwrap();
    graph.save(&settings).unwrap();
    let reopened = BatchGraph::load(&settings).unwrap();
    assert_eq!(reopened, graph);
    let BatchOperationKind::ColorReplace(pairs) = &reopened.operations[0].kind else {
        panic!("replacement operation disappeared");
    };
    assert_eq!(pairs[0].old, PixelValue::Rgba([30, 20, 10, 255]));
    assert_eq!(pairs[0].new, PixelValue::Rgba([10, 20, 30, 255]));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acceptance_continuous_fill_preview_warns_when_seed_moves_color() {
    let directory = temp_directory("seed-preview");
    let first = directory.join("cell1.inkpod");
    let second = directory.join("cell2.inkpod");
    saved_cell(&first, [10, 20, 30, 255]);
    saved_cell(&second, [50, 60, 70, 255]);
    let graph = BatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "fill-preview".to_owned(),
        inputs: vec![BatchInputSelector {
            kind: BatchInputKind::Folder,
            path: directory.to_string_lossy().into_owned(),
            first_cell: 0,
            last_cell: 0,
        }],
        operations: vec![BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(BatchTargetSelector::color_plane()),
            kind: BatchOperationKind::ContinuousFill(vec![BatchSeed {
                x: 1,
                y: 1,
                color: PixelValue::Rgba([255, 0, 0, 255]),
                tolerance: 0,
                gap_close: 0,
                expected_source: None,
            }]),
        }],
        output: BatchOutputSettings {
            folder: directory.join("out").to_string_lossy().into_owned(),
            ..BatchOutputSettings::default()
        },
    };
    let preview = Core::new()
        .batch_preview(&graph, BatchRunScope::All)
        .unwrap();
    assert_eq!(preview.items.len(), 2);
    assert!(preview.items[0].warnings.is_empty());
    assert!(
        preview.items[1]
            .warnings
            .iter()
            .any(|warning| warning.contains("moved to a different color"))
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn review_rejects_empty_or_type_mismatched_target_selectors() {
    let operation = BatchOperation {
        version: BATCH_OPERATION_VERSION,
        enabled: true,
        configure_each_run: false,
        target: Some(BatchTargetSelector {
            layer_id: None,
            plane_id: None,
            layer_kind: None,
            plane_kind: None,
            missing_policy: BatchMissingTargetPolicy::Skip,
        }),
        kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
            enabled: true,
            old: PixelValue::Rgba([0; 4]),
            new: PixelValue::Rgba([1, 2, 3, 4]),
        }]),
    };
    assert!(matches!(
        validate_operation(&operation),
        Err(CoreError::InvalidArgument(
            "batch target layer selector is empty"
        ))
    ));

    let mut core = Core::new();
    core.new_cell(2, 2, 96_000, 96_000).unwrap();
    let layers = core.layers().unwrap();
    let coloring = layers
        .iter()
        .find(|layer| layer.kind == LayerKind::BinaryColoring)
        .unwrap();
    let color_plane = coloring
        .planes
        .iter()
        .find(|plane| plane.kind == PlaneType::Color)
        .unwrap();
    let selector = BatchTargetSelector {
        layer_id: Some(coloring.id),
        plane_id: Some(color_plane.id),
        layer_kind: Some(LayerKind::VectorColoring),
        plane_kind: Some(PlaneType::Color),
        missing_policy: BatchMissingTargetPolicy::Skip,
    };
    assert_eq!(resolve_target(&core, &selector).unwrap(), None);
}

#[test]
fn review_operation_item_counts_enforce_closed_bounds() {
    let validate_kind = |kind| {
        validate_operation(&BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(BatchTargetSelector::color_plane()),
            kind,
        })
    };
    let assert_invalid = |kind, message| {
        assert_eq!(
            validate_kind(kind),
            Err(CoreError::InvalidArgument(message))
        );
    };

    let pair = BatchColorPair {
        enabled: true,
        old: PixelValue::Rgba([0; 4]),
        new: PixelValue::Rgba([1, 2, 3, 4]),
    };
    for count in [1, MAX_BATCH_COLOR_PAIRS] {
        assert!(validate_kind(BatchOperationKind::ColorReplace(vec![pair.clone(); count])).is_ok());
    }
    for count in [0, MAX_BATCH_COLOR_PAIRS + 1] {
        assert_invalid(
            BatchOperationKind::ColorReplace(vec![pair.clone(); count]),
            "batch color-pair count is outside bounds",
        );
    }

    let seed = BatchSeed {
        x: 0,
        y: 0,
        color: PixelValue::Rgba([0; 4]),
        tolerance: 0,
        gap_close: 0,
        expected_source: None,
    };
    for count in [1, MAX_BATCH_SEEDS] {
        assert!(
            validate_kind(BatchOperationKind::ContinuousFill(vec![
                seed.clone();
                count
            ]))
            .is_ok()
        );
    }
    for count in [0, MAX_BATCH_SEEDS + 1] {
        assert_invalid(
            BatchOperationKind::ContinuousFill(vec![seed.clone(); count]),
            "batch fill-seed count is outside bounds",
        );
    }

    for count in [1, MAX_BATCH_COLORS] {
        assert!(
            validate_kind(BatchOperationKind::Separation(BatchSeparation {
                colors: vec![PixelValue::Rgba([0; 4]); count],
                replacement: PixelValue::Rgba([1, 2, 3, 4]),
                invert: false,
            }))
            .is_ok()
        );
    }
    for count in [0, MAX_BATCH_COLORS + 1] {
        assert_invalid(
            BatchOperationKind::Separation(BatchSeparation {
                colors: vec![PixelValue::Rgba([0; 4]); count],
                replacement: PixelValue::Rgba([1, 2, 3, 4]),
                invert: false,
            }),
            "batch separation color count is outside bounds",
        );
    }
}

#[test]
fn review_current_scope_selects_the_open_file_instead_of_the_first_file() {
    let directory = temp_directory("current-file-scope");
    let first = directory.join("cell1.inkpod");
    let current = directory.join("cell2.inkpod");
    saved_cell(&first, [10, 20, 30, 255]);
    saved_cell(&current, [40, 50, 60, 255]);
    let mut core = Core::new();
    core.open(&current).unwrap();
    let mut graph = replace_graph(&first, &directory.join("out"));
    graph.inputs = vec![BatchInputSelector {
        kind: BatchInputKind::Folder,
        path: directory.to_string_lossy().into_owned(),
        first_cell: 0,
        last_cell: 0,
    }];
    let report = core
        .batch_execute(
            &graph,
            BatchRunOptions {
                scope: BatchRunScope::Current,
                dry_run: true,
                preview_confirmed: true,
            },
            |_, _| true,
        )
        .unwrap();
    assert_eq!(report.items.len(), 1);
    assert_eq!(report.items[0].input_name, "cell2.inkpod");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn review_file_wait_polls_cancellation_without_sleeping_the_full_interval() {
    let directory = temp_directory("wait-cancel");
    let first = directory.join("cell1.inkpod");
    let second = directory.join("cell2.inkpod");
    saved_cell(&first, [10, 20, 30, 255]);
    saved_cell(&second, [10, 20, 30, 255]);
    let mut graph = replace_graph(&first, &directory.join("out"));
    graph.inputs = vec![BatchInputSelector {
        kind: BatchInputKind::Folder,
        path: directory.to_string_lossy().into_owned(),
        first_cell: 0,
        last_cell: 0,
    }];
    graph.output.wait_milliseconds = 1_000;
    let started = std::time::Instant::now();
    let mut first_item_completion_polls = 0_u32;
    let report = Core::new()
        .batch_execute(
            &graph,
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: true,
                preview_confirmed: true,
            },
            |completed, total| {
                if completed == 3 && total == 6 {
                    first_item_completion_polls += 1;
                    return first_item_completion_polls == 1;
                }
                true
            },
        )
        .unwrap();
    assert!(report.cancelled);
    assert_eq!(report.items.len(), 1);
    assert!(started.elapsed() < Duration::from_millis(750));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn review_every_operation_and_filter_variant_round_trips() {
    let directory = temp_directory("catalog-roundtrip");
    let settings = directory.join("catalog.inkbatch");
    let target = || Some(BatchTargetSelector::color_plane());
    let mut operations: Vec<_> = vec![
        Filter::SharpenWeak,
        Filter::SharpenStrong,
        Filter::BlurWeak,
        Filter::BlurStrong,
        Filter::GaussianBlur {
            radius: 2,
            strength_milli: 500,
        },
        Filter::UnsharpMask {
            radius: 2,
            amount_milli: 750,
            threshold: 12,
        },
        Filter::Invert {
            channel: Channel::Green,
        },
        Filter::AutoContrast,
        Filter::BrightnessContrast {
            brightness_milli: -100,
            contrast_milli: 200,
        },
        Filter::ToneCurve {
            channel: Channel::Blue,
            interpolation: CurveInterpolation::BSpline,
            points: vec![
                CurvePoint {
                    input: 0,
                    output: 1,
                },
                CurvePoint {
                    input: u16::MAX,
                    output: u16::MAX - 1,
                },
            ],
        },
        Filter::Levels(Levels {
            channel: Channel::Red,
            input_shadow: 1,
            input_gamma_milli: 1_100,
            input_highlight: u16::MAX - 1,
            output_shadow: 2,
            output_highlight: u16::MAX - 2,
        }),
        Filter::Hsv(HsvAdjustment {
            hue_degrees_milli: 45_000,
            saturation_milli: 100,
            value_milli: -100,
        }),
        Filter::ColorBalance(ColorBalance {
            red_milli: 100,
            green_milli: -100,
            blue_milli: 50,
        }),
    ]
    .into_iter()
    .map(|filter| BatchOperation {
        version: BATCH_OPERATION_VERSION,
        enabled: true,
        configure_each_run: false,
        target: target(),
        kind: BatchOperationKind::Filter(filter),
    })
    .collect();
    let operation = |target, kind| BatchOperation {
        version: BATCH_OPERATION_VERSION,
        enabled: true,
        configure_each_run: false,
        target,
        kind,
    };
    operations.extend([
        operation(
            target(),
            BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([1, 2, 3, 4]),
                new: PixelValue::Rgba([4, 3, 2, 1]),
            }]),
        ),
        operation(
            target(),
            BatchOperationKind::ContinuousFill(vec![BatchSeed {
                x: 1,
                y: 2,
                color: PixelValue::Rgba([10, 20, 30, 255]),
                tolerance: 5,
                gap_close: 1,
                expected_source: Some(PixelValue::Rgba([1, 1, 1, 255])),
            }]),
        ),
        operation(
            target(),
            BatchOperationKind::Separation(BatchSeparation {
                colors: vec![PixelValue::Rgba([1, 2, 3, 255])],
                replacement: PixelValue::Rgba([9, 8, 7, 255]),
                invert: true,
            }),
        ),
        operation(
            Some(BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                layer_kind: Some(LayerKind::BinaryColoring),
                plane_kind: None,
                missing_policy: BatchMissingTargetPolicy::Skip,
            }),
            BatchOperationKind::Visibility { visible: false },
        ),
        operation(
            Some(BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                layer_kind: Some(LayerKind::VectorColoring),
                plane_kind: Some(PlaneType::VectorMainLine),
                missing_policy: BatchMissingTargetPolicy::Skip,
            }),
            BatchOperationKind::LineWidth(VectorWidthMode::Add(0.5)),
        ),
        operation(
            Some(BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                layer_kind: Some(LayerKind::VectorColoring),
                plane_kind: Some(PlaneType::VectorMainLine),
                missing_policy: BatchMissingTargetPolicy::Skip,
            }),
            BatchOperationKind::LineWidth(VectorWidthMode::Subtract(0.25)),
        ),
        operation(
            Some(BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                layer_kind: Some(LayerKind::VectorColoring),
                plane_kind: Some(PlaneType::VectorMainLine),
                missing_policy: BatchMissingTargetPolicy::Skip,
            }),
            BatchOperationKind::LineWidth(VectorWidthMode::Scale(1.5)),
        ),
        operation(
            Some(BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                layer_kind: Some(LayerKind::VectorColoring),
                plane_kind: Some(PlaneType::VectorMainLine),
                missing_policy: BatchMissingTargetPolicy::Skip,
            }),
            BatchOperationKind::LineWidth(VectorWidthMode::Constant(2.0)),
        ),
        operation(
            target(),
            BatchOperationKind::BoundaryAirbrush(BoundaryAirbrush {
                colors: vec![[0, 0, 0, u16::MAX], [u16::MAX; 4]],
                width: 3,
                strength_milli: 750,
            }),
        ),
        operation(
            target(),
            BatchOperationKind::DustRemoval(DustRemoval {
                mode: DustMode::RemoveForeground,
                maximum_pixels: 4,
            }),
        ),
        operation(None, BatchOperationKind::Mirror(MirrorAxis::Horizontal)),
        operation(None, BatchOperationKind::Rotate90(RotateDirection::Right90)),
        operation(
            None,
            BatchOperationKind::Resize(DocumentResize {
                width: 16,
                height: 12,
                dpi_x_milli: 96_000,
                dpi_y_milli: 120_000,
                resample: true,
                anchor: ResizeAnchor::BottomRight,
            }),
        ),
        operation(
            target(),
            BatchOperationKind::ConvertPlane {
                destination_kind: PlaneType::Raster,
                destination_format: PixelFormat::StraightRgba8,
            },
        ),
    ]);
    let graph = BatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "catalog".to_owned(),
        inputs: vec![BatchInputSelector::current_sequence()],
        operations,
        output: BatchOutputSettings::default(),
    };
    graph.save(&settings).unwrap();
    assert_eq!(BatchGraph::load(&settings).unwrap(), graph);
    fs::remove_dir_all(directory).unwrap();
}
