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
    saved_cell_on_plane(path, ActivePlane::Color, color);
}

fn saved_cell_on_plane(path: &Path, plane: ActivePlane, color: [u8; 4]) {
    let mut core = Core::new();
    core.new_cell(4, 4, 96_000, 96_000).unwrap();
    core.set_active_plane(plane).unwrap();
    core.apply_stroke(&Stroke {
        tool: PaintTool::Pencil,
        plane,
        color,
        diameter: 1.0,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
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

fn separation_graph(
    input: &Path,
    output: &Path,
    destination: BatchSeparationDestination,
) -> BatchGraph {
    BatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "separate-set".to_owned(),
        inputs: vec![BatchInputSelector::file(input.to_string_lossy())],
        operations: vec![BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(BatchTargetSelector::color_plane()),
            kind: BatchOperationKind::Separation(BatchSeparation {
                colors: vec![PixelValue::Rgba([10, 20, 30, 255])],
                replacement: PixelValue::Rgba([80, 70, 60, 255]),
                invert: false,
                destination,
            }),
        }],
        output: BatchOutputSettings {
            folder: output.to_string_lossy().into_owned(),
            ..BatchOutputSettings::default()
        },
    }
}

fn sequence_source(
    name: &str,
    document_uuid: u128,
    source_generation: u64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> SequenceCellSource {
    let raster = CommonRaster::new(
        width,
        height,
        PixelFormat::StraightRgba8,
        None,
        None,
        pixels,
    )
    .unwrap();
    SequenceCellSource::from_common_raster_with_generation(
        name,
        document_uuid,
        source_generation,
        &raster,
    )
    .unwrap()
}

#[test]
fn acceptance_two_cell_pair_extraction_is_exact_alpha_aware_and_deterministic() {
    let old_uuid = 0x101_u128;
    let new_uuid = 0x202_u128;
    let old = sequence_source(
        "A001",
        old_uuid,
        7,
        4,
        1,
        vec![
            1, 2, 3, 4, // unchanged
            10, 20, 30, 40, // alpha-only change
            50, 60, 70, 80, // many-to-one first old
            90, 100, 110, 120, // many-to-one second old
        ],
    );
    let new = sequence_source(
        "A002",
        new_uuid,
        9,
        4,
        1,
        vec![
            1, 2, 3, 4, // unchanged
            10, 20, 30, 41, // alpha-only change remains a candidate
            200, 201, 202, 203, // shared destination
            200, 201, 202, 203, // shared destination
        ],
    );
    let mut core = Core::new();
    core.set_sequence(vec![new, old]).unwrap();

    let extraction = core
        .extract_batch_color_pairs(
            SequenceSourceIdentity {
                document_uuid: old_uuid,
                source_generation: 7,
            },
            SequenceSourceIdentity {
                document_uuid: new_uuid,
                source_generation: 9,
            },
        )
        .unwrap();

    assert_eq!(extraction.pixel_format, PixelFormat::StraightRgba8);
    assert_eq!(extraction.unchanged_pixel_count, 1);
    assert_eq!(extraction.ambiguity_count, 0);
    assert_eq!(extraction.candidates.len(), 3);
    assert_eq!(
        extraction.candidates[0],
        BatchPairCandidate {
            old: PixelValue::Rgba([10, 20, 30, 40]),
            new: PixelValue::Rgba([10, 20, 30, 41]),
            pixel_count: 1,
            affected_bounds: RectI32 {
                x: 1,
                y: 0,
                width: 1,
                height: 1,
            },
            ambiguous: false,
        }
    );
    assert_eq!(
        extraction.resolved_pairs(&[]).unwrap(),
        extraction
            .candidates
            .iter()
            .map(|candidate| BatchColorPair {
                enabled: true,
                old: candidate.old,
                new: candidate.new,
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn acceptance_two_cell_one_to_many_requires_an_explicit_choice_or_exclusion() {
    let old_uuid = 0x303_u128;
    let new_uuid = 0x404_u128;
    let old = sequence_source(
        "B001",
        old_uuid,
        1,
        3,
        1,
        vec![9, 8, 7, 6, 9, 8, 7, 6, 1, 1, 1, 255],
    );
    let new = sequence_source(
        "B002",
        new_uuid,
        1,
        3,
        1,
        vec![2, 2, 2, 255, 3, 3, 3, 255, 1, 1, 1, 255],
    );
    let mut core = Core::new();
    core.set_sequence(vec![old, new]).unwrap();
    let extraction = core
        .extract_batch_color_pairs(
            SequenceSourceIdentity {
                document_uuid: old_uuid,
                source_generation: 1,
            },
            SequenceSourceIdentity {
                document_uuid: new_uuid,
                source_generation: 1,
            },
        )
        .unwrap();

    assert_eq!(extraction.ambiguity_count, 1);
    assert_eq!(extraction.candidates.len(), 2);
    assert!(
        extraction
            .candidates
            .iter()
            .all(|candidate| candidate.ambiguous)
    );
    assert!(extraction.resolved_pairs(&[]).is_err());
    assert_eq!(
        extraction
            .resolved_pairs(&[BatchPairResolution {
                old: PixelValue::Rgba([9, 8, 7, 6]),
                selected_new: Some(PixelValue::Rgba([3, 3, 3, 255])),
            }])
            .unwrap(),
        vec![BatchColorPair {
            enabled: true,
            old: PixelValue::Rgba([9, 8, 7, 6]),
            new: PixelValue::Rgba([3, 3, 3, 255]),
        }]
    );
    assert!(
        extraction
            .resolved_pairs(&[BatchPairResolution {
                old: PixelValue::Rgba([9, 8, 7, 6]),
                selected_new: Some(PixelValue::Rgba([4, 4, 4, 255])),
            }])
            .is_err()
    );
    assert!(
        extraction
            .resolved_pairs(&[BatchPairResolution {
                old: PixelValue::Rgba([9, 8, 7, 6]),
                selected_new: None,
            }])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn acceptance_two_cell_pair_extraction_rejects_stale_identity_and_geometry_mismatch() {
    let old_uuid = 0x505_u128;
    let new_uuid = 0x606_u128;
    let old = sequence_source("C001", old_uuid, 4, 1, 1, vec![1, 2, 3, 4]);
    let new = sequence_source("C002", new_uuid, 5, 1, 1, vec![4, 3, 2, 1]);
    let mut core = Core::new();
    core.set_sequence(vec![old, new]).unwrap();
    assert!(
        core.extract_batch_color_pairs(
            SequenceSourceIdentity {
                document_uuid: old_uuid,
                source_generation: 3,
            },
            SequenceSourceIdentity {
                document_uuid: new_uuid,
                source_generation: 5,
            },
        )
        .is_err()
    );

    let mismatched = sequence_source("C002", new_uuid, 6, 2, 1, vec![4, 3, 2, 1, 0, 0, 0, 0]);
    core.set_sequence(vec![
        sequence_source("C001", old_uuid, 4, 1, 1, vec![1, 2, 3, 4]),
        mismatched,
    ])
    .unwrap();
    assert!(
        core.extract_batch_color_pairs(
            SequenceSourceIdentity {
                document_uuid: old_uuid,
                source_generation: 4,
            },
            SequenceSourceIdentity {
                document_uuid: new_uuid,
                source_generation: 6,
            },
        )
        .is_err()
    );
}

#[test]
fn acceptance_separation_selection_destination_is_atomic_and_undoable_after_reopen() {
    let directory = temp_directory("separation-selection");
    let input = directory.join("cell1.inkpod");
    let output = directory.join("output");
    saved_cell(&input, [10, 20, 30, 255]);
    let report = Core::new()
        .batch_execute(
            &separation_graph(&input, &output, BatchSeparationDestination::SelectionMask),
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: false,
                preview_confirmed: true,
            },
            |_, _| true,
        )
        .unwrap();
    assert_eq!(report.items[0].outcome, BatchItemOutcome::Succeeded);
    let mut reopened = Core::new();
    reopened.open(&output.join("cell1_batch.inkpod")).unwrap();
    assert_eq!(
        reopened.selection_bounds().unwrap(),
        Some(RectI32 {
            x: 1,
            y: 1,
            width: 1,
            height: 1,
        })
    );
    reopened.undo().unwrap();
    assert_eq!(reopened.selection_bounds().unwrap(), None);
    reopened.redo().unwrap();
    assert_eq!(
        reopened.selection_bounds().unwrap(),
        Some(RectI32 {
            x: 1,
            y: 1,
            width: 1,
            height: 1,
        })
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acceptance_separation_plane_and_native_destinations_have_exact_pixels_and_stable_tree() {
    let cases = [
        (
            "replace-source",
            BatchSeparationDestination::ReplaceSource,
            ActivePlane::Color,
            BatchTargetSelector::color_plane(),
            PixelValue::Rgba([10, 20, 30, 255]),
            PixelValue::Rgba([80, 70, 60, 255]),
            PixelValue::Binary(0),
            PixelValue::Rgba([80, 70, 60, 255]),
        ),
        (
            "main-line",
            BatchSeparationDestination::MainLinePlane,
            ActivePlane::Color,
            BatchTargetSelector::color_plane(),
            PixelValue::Rgba([10, 20, 30, 255]),
            PixelValue::Binary(u8::MAX),
            PixelValue::Binary(u8::MAX),
            PixelValue::Rgba([10, 20, 30, 255]),
        ),
        (
            "color-plane",
            BatchSeparationDestination::ColorPlane,
            ActivePlane::MainLine,
            BatchTargetSelector {
                layer_id: None,
                plane_id: None,
                layer_kind: Some(LayerKind::BinaryColoring),
                plane_kind: Some(PlaneType::MainLine),
                missing_policy: BatchMissingTargetPolicy::Error,
            },
            PixelValue::Binary(u8::MAX),
            PixelValue::Rgba([80, 70, 60, 255]),
            PixelValue::Binary(u8::MAX),
            PixelValue::Rgba([80, 70, 60, 255]),
        ),
        (
            "native-file",
            BatchSeparationDestination::NativeFile,
            ActivePlane::Color,
            BatchTargetSelector::color_plane(),
            PixelValue::Rgba([10, 20, 30, 255]),
            PixelValue::Rgba([80, 70, 60, 255]),
            PixelValue::Binary(0),
            PixelValue::Rgba([80, 70, 60, 255]),
        ),
    ];
    for (
        label,
        destination,
        source_plane,
        target,
        source_color,
        replacement,
        expected_main,
        expected_color,
    ) in cases
    {
        let directory = temp_directory(label);
        let input = directory.join("cell1.inkpod");
        let output = directory.join("output");
        saved_cell_on_plane(&input, source_plane, [10, 20, 30, 255]);
        let mut before = Core::new();
        before.open(&input).unwrap();
        let tree = before.layers().unwrap();
        let mut graph = separation_graph(&input, &output, destination);
        graph.operations[0].target = Some(target);
        let BatchOperationKind::Separation(options) = &mut graph.operations[0].kind else {
            unreachable!();
        };
        options.colors = vec![source_color];
        options.replacement = replacement;
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
        assert_eq!(report.items[0].outcome, BatchItemOutcome::Succeeded);
        let output_file = output.join("cell1_batch.inkpod");
        assert!(output_file.is_file());
        let mut reopened = Core::new();
        reopened.open(&output_file).unwrap();
        assert_eq!(reopened.layers().unwrap(), tree);
        assert_eq!(
            reopened.plane_pixel(ActivePlane::MainLine, 1, 1).unwrap(),
            expected_main
        );
        assert_eq!(
            reopened.plane_pixel(ActivePlane::Color, 1, 1).unwrap(),
            expected_color
        );
        assert_eq!(
            reopened.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
            PixelValue::Rgba([0, 0, 0, 0])
        );
        fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn acceptance_batch_execute_rejects_unresolved_per_run_configuration() {
    let directory = temp_directory("unresolved-run-configuration");
    let input = directory.join("cell1.inkpod");
    let output = directory.join("output");
    saved_cell(&input, [10, 20, 30, 255]);
    let mut graph = replace_graph(&input, &output);
    graph.operations[0].configure_each_run = true;
    assert_eq!(
        Core::new().batch_execute(
            &graph,
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: true,
                preview_confirmed: true,
            },
            |_, _| true,
        ),
        Err(CoreError::InvalidState(
            "batch run contains unresolved per-run configuration"
        ))
    );
    assert!(!output.exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acceptance_native_file_separation_rejects_explicit_input_overwrite() {
    let directory = temp_directory("separation-native-overwrite");
    let input = directory.join("cell1.inkpod");
    saved_cell(&input, [10, 20, 30, 255]);
    let mut graph = separation_graph(&input, &directory, BatchSeparationDestination::NativeFile);
    graph.output.policy = BatchOutputPolicy::ExplicitOverwrite;
    assert!(graph.validate().is_err());
    fs::remove_dir_all(directory).unwrap();
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
                enabled: true,
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
                enabled: false,
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
                destination: BatchSeparationDestination::ReplaceSource,
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
