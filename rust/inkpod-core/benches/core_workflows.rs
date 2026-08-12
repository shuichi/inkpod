use inkpod_core::*;
use inkpod_image::TILE_SIZE;
use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BENCHMARK_UUID: u128 = 0x494e_4b50_4f44_2d4d_322d_4245_4e43_4801;
const EXPECTED_QUICK_CHECKSUMS: [u64; 10] = [
    0x517e_d7ae_78bf_0487,
    0x9e13_576d_ef6f_539b,
    0x517e_d7ae_78bf_0487,
    0x3f10_53b9_fde3_7d35,
    0x255a_b9ba_d114_dfdd,
    0x2813_c527_f273_11c8,
    0xf31d_31fe_1bb0_0fd7,
    0x264b_9802_8ac9_2ac6,
    0xc668_17dc_a534_5832,
    0x6503_00bd_ff90_44cb,
];
const EXPECTED_FULL_CHECKSUMS: [u64; 10] = [
    0x4390_40e0_244d_5773,
    0xa33f_7534_fcdd_61e7,
    0x4390_40e0_244d_5773,
    0xa2c1_a74e_7f97_81a3,
    0x77f6_3d83_e130_185f,
    0xb975_f3cf_db78_24fd,
    0x6732_b8b0_a656_5d03,
    0x264b_9802_8ac9_2ac6,
    0xc668_17dc_a534_5832,
    0x290f_6150_f771_8c2d,
];

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    sparse_tiles: u32,
    dirty_rebuild_steps: u32,
    pan_zoom_steps: u32,
    undo_edits: u32,
    light_table_side: u32,
    light_table_references: u32,
    vector_side: u32,
    vector_paths: u32,
    batch_cells: u32,
    batch_side: u32,
    checkpoint_samples: u32,
    output_color_guard_side: u32,
}

impl Profile {
    const fn from_quick(quick: bool) -> Self {
        if quick {
            Self {
                name: "quick",
                sparse_tiles: 8,
                dirty_rebuild_steps: 32,
                pan_zoom_steps: 2_048,
                undo_edits: 12,
                light_table_side: 128,
                light_table_references: 3,
                vector_side: 128,
                vector_paths: 8,
                batch_cells: 4,
                batch_side: 16,
                checkpoint_samples: 175_000,
                output_color_guard_side: 1_024,
            }
        } else {
            Self {
                name: "full",
                sparse_tiles: 32,
                dirty_rebuild_steps: 128,
                pan_zoom_steps: 8_192,
                undo_edits: 48,
                light_table_side: 256,
                light_table_references: 6,
                vector_side: 256,
                vector_paths: 32,
                batch_cells: 16,
                batch_side: 32,
                checkpoint_samples: 1_000_000,
                output_color_guard_side: 2_048,
            }
        }
    }
}

struct ScenarioResult {
    scenario: &'static str,
    elapsed: Duration,
    iterations: u64,
    input_items: u64,
    output_items: u64,
    reused_items: u64,
    document_revision: u64,
    history_entries: u64,
    successes: u64,
    failures: u64,
    checksum: u64,
}

impl ScenarioResult {
    fn print(&self, profile: Profile) {
        println!(
            "inkpod-core-workflows profile={} scenario={} iterations={} input_items={} output_items={} reused_items={} document_revision={} history_entries={} successes={} failures={} checksum={:016x} elapsed_ns={}",
            profile.name,
            self.scenario,
            self.iterations,
            self.input_items,
            self.output_items,
            self.reused_items,
            self.document_revision,
            self.history_entries,
            self.successes,
            self.failures,
            self.checksum,
            self.elapsed.as_nanos()
        );
    }
}

fn main() {
    let quick = std::env::args().any(|argument| argument == "--quick");
    let profile = Profile::from_quick(quick);
    let results = [
        sparse_snapshot(profile),
        dirty_tile_rebuild(profile),
        pan_zoom_snapshot(profile),
        undo_redo(profile),
        light_table_composite(profile),
        vector_snapshot(profile),
        batch_preview(profile),
        canonical_replay(profile),
        checkpoint_open(profile),
        output_color_guard(profile),
    ];

    for result in &results {
        result.print(profile);
    }
    let actual = results.map(|result| result.checksum);
    let expected = if quick {
        EXPECTED_QUICK_CHECKSUMS
    } else {
        EXPECTED_FULL_CHECKSUMS
    };
    assert_eq!(actual, expected, "benchmark semantic checksums changed");
}

fn output_color_guard(profile: Profile) -> ScenarioResult {
    let side = profile.output_color_guard_side;
    let pixel_count = u64::from(side) * u64::from(side);
    let transparent_count = pixel_count / 16;
    let selected_count = pixel_count / 2;
    let mut pixels = Vec::with_capacity(pixel_count as usize * 8);
    for index in 0..pixel_count {
        let pixel = match index & 15 {
            0 => [u16::MAX, 0, 0, 0],
            1..=7 => [128 * 257, 128 * 257, 128 * 257, u16::MAX],
            _ => [u16::MAX, 0, 0, u16::MAX],
        };
        for channel in pixel {
            pixels.extend_from_slice(&channel.to_le_bytes());
        }
    }
    let mut core = Core::new();
    core.new_cell_from_raster_asset(
        RasterAssetInput {
            width: side,
            height: side,
            pixel_format: PixelFormat::StraightRgba16,
            color_space: Some(AssetColorSpace::Srgb),
            alpha_semantics: AssetAlphaSemantics::Straight,
            canonical_stride: u64::from(side) * 8,
            pixels,
            expected_id: None,
        },
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        BENCHMARK_UUID + 5,
    )
    .expect("bounded output-color guard fixture must be valid");
    let before = core.resource_usage();
    assert_eq!(before.document_tile_count, 0);
    let base_revision = core
        .document_info()
        .expect("guard fixture must have a document")
        .document_revision;

    let started = Instant::now();
    let result = core
        .select_output_color_guard(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            base_revision,
        )
        .expect("bounded output-color guard must succeed");
    let elapsed = started.elapsed();

    assert_eq!(
        result.summary.scanned_pixel_count,
        pixel_count - transparent_count
    );
    assert_eq!(result.summary.selected_pixel_count, selected_count);
    assert_eq!(result.summary.transparent_pixel_count, transparent_count);
    assert_eq!(result.dispatch.accepted_commands(), 1);
    assert_eq!(result.dispatch.revision(), base_revision + 1);
    assert_eq!(core.history_entries().len(), 1);
    assert_eq!(
        core.selection_bounds()
            .expect("selection bounds must be readable"),
        Some(RectI32 {
            x: 8,
            y: 0,
            width: side as i32 - 8,
            height: side as i32,
        })
    );
    assert_eq!(
        core.journal_entries().iter().find_map(|entry| match entry {
            JournalEntry::Commit(commit) => Some(commit.procedure().primitive_id()),
            JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => None,
        }),
        Some(PrimitiveId::SELECT_OUTPUT_COLOR_GUARD)
    );
    let usage = core.resource_usage();
    let selection_tiles = u64::from(side.div_ceil(TILE_SIZE).pow(2));
    assert_eq!(usage.document_tile_count, selection_tiles);
    assert_eq!(usage.document_tile_bytes, pixel_count);
    assert_eq!(usage.cpu_staging_bytes, 0);
    let digest = core
        .document_state_digest()
        .expect("guard result digest must be available");
    let mut hash = Fnv1a64::new();
    hash.write(digest.as_bytes());
    hash.write(&result.summary.scanned_pixel_count.to_le_bytes());
    hash.write(&result.summary.selected_pixel_count.to_le_bytes());
    hash.write(&result.summary.transparent_pixel_count.to_le_bytes());
    hash.write(&usage.document_tile_count.to_le_bytes());
    hash.write(&usage.document_tile_bytes.to_le_bytes());
    hash.write(&PrimitiveId::SELECT_OUTPUT_COLOR_GUARD.get().to_le_bytes());
    let checksum = hash.finish();
    black_box(&core);

    ScenarioResult {
        scenario: "output_color_guard",
        elapsed,
        iterations: u64::from(side),
        input_items: pixel_count,
        output_items: selected_count,
        reused_items: transparent_count,
        document_revision: result.dispatch.revision(),
        history_entries: core.history_entries().len() as u64,
        successes: result.dispatch.accepted_commands(),
        failures: 0,
        checksum,
    }
}

fn checkpoint_open(profile: Profile) -> ScenarioResult {
    let path = std::env::temp_dir().join(format!(
        "inkpod-core-benchmark-checkpoint-{}-{}.inkpod",
        std::process::id(),
        profile.name
    ));
    let mut core = Core::new();
    let created = core
        .new_cell_with_uuid(
            1,
            1,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            BENCHMARK_UUID + 4,
        )
        .expect("bounded checkpoint document must be valid");
    let samples = (0..profile.checkpoint_samples)
        .map(|index| StrokeSample {
            x: 0.25 + f32::from((index & 1) as u8) * 0.25,
            y: 0.25,
            pressure: 1.0,
        })
        .collect();
    core.execute_primitive(PrimitiveRequest::ApplyRasterStroke {
        expected_revision: created.document_revision,
        target_plane_id: created.color_plane_id,
        stroke: Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::Color,
            color: [17, 43, 91, 255],
            diameter: 1.0,
            shape: BrushShape::Round,
            smoothing: 0,
            start_color: StartColorPredicate::Any,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples,
        },
    })
    .expect("bounded long stroke must commit");
    for index in 0..255 {
        let value = if index % 2 == 0 { 1 } else { 2 };
        core.set_main_line_color(PixelValue::Rgba([value, value, value, 255]))
            .expect("checkpoint policy edit must commit");
    }
    let policy = core
        .persistence_info()
        .expect("persistence diagnostics must be available");
    assert_eq!(policy.procedure_count, 256);
    assert!(policy.checkpoint_due);
    if profile.checkpoint_samples == 1_000_000 {
        assert!(policy.replay_work >= 1_000_000);
    }
    let expected_digest = core
        .document_state_digest()
        .expect("checkpoint source digest must be available");
    core.save(&path)
        .expect("checkpoint benchmark fixture must save");

    let started = Instant::now();
    let mut opened = Core::new();
    opened
        .open(&path)
        .expect("checkpoint benchmark fixture must open");
    let elapsed = started.elapsed();
    assert_eq!(
        opened
            .persistence_info()
            .expect("opened persistence diagnostics must be available")
            .open_strategy,
        NativeOpenStrategy::Checkpoint
    );
    assert_eq!(
        opened
            .document_state_digest()
            .expect("opened checkpoint digest must be available"),
        expected_digest
    );
    assert_eq!(opened.journal_entries(), core.journal_entries());
    opened
        .undo()
        .expect("checkpoint history cache must rebuild");
    opened.redo().expect("checkpoint history cache must redo");
    let checksum = u64::from_le_bytes(expected_digest.as_bytes()[..8].try_into().unwrap());
    std::fs::remove_file(&path).expect("checkpoint benchmark file must be removable");
    black_box(&opened);

    ScenarioResult {
        scenario: "checkpoint_open",
        elapsed,
        iterations: 1,
        input_items: u64::from(profile.checkpoint_samples) + policy.procedure_count,
        output_items: opened.history_entries().len() as u64,
        reused_items: policy.asset_count,
        document_revision: opened.document_info().unwrap().document_revision,
        history_entries: opened.history_entries().len() as u64,
        successes: 1,
        failures: 0,
        checksum,
    }
}

fn canonical_replay(_profile: Profile) -> ScenarioResult {
    const UUID: u128 = 0x494e_4b50_4f44_2d4d_372d_474f_4c44_454e;
    let started = Instant::now();
    let mut core = Core::new();
    let created = core
        .new_cell_with_uuid(16, 16, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, UUID)
        .unwrap();
    core.apply_stroke(&Stroke {
        tool: PaintTool::Brush,
        plane: ActivePlane::Color,
        color: [17, 43, 91, 219],
        diameter: 3.375,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: true,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![
            StrokeSample {
                x: 1.125,
                y: 2.875,
                pressure: 0.375,
            },
            StrokeSample {
                x: 13.625,
                y: 5.25,
                pressure: 0.9375,
            },
        ],
    })
    .unwrap();
    core.apply_gradient_to_plane(
        created.color_plane_id,
        &Gradient {
            kind: GradientKind::Radial,
            mode: GradientMode::Composite,
            start_x_milli: 3_250,
            start_y_milli: 4_750,
            end_x_milli: 12_125,
            end_y_milli: 9_375,
            dither: true,
            stops: vec![
                GradientStop {
                    position_milli: 0,
                    color: [1_000, 2_000, 3_000, 50_000],
                },
                GradientStop {
                    position_milli: 425,
                    color: [12_345, 23_456, 34_567, 40_000],
                },
                GradientStop {
                    position_milli: 1_000,
                    color: [60_000, 40_000, 20_000, 30_000],
                },
            ],
        },
    )
    .unwrap();
    core.apply_blur_to_plane(created.color_plane_id, 2, 725)
        .unwrap();
    core.begin_filter_preview(
        created.color_plane_id,
        Filter::Levels(Levels {
            channel: Channel::Rgb,
            input_shadow: 321,
            input_gamma_milli: 1_375,
            input_highlight: 64_123,
            output_shadow: 777,
            output_highlight: 63_999,
        }),
    )
    .unwrap();
    core.apply_filter_preview().unwrap();
    core.apply_airbrush_to_plane(
        created.color_plane_id,
        AirbrushStroke {
            center_x_milli: 7_625,
            center_y_milli: 10_375,
            radius_milli: 3_125,
            hardness_milli: 375,
            opacity_milli: 625,
            color: [50_000, 5_000, 42_000, 55_000],
        },
    )
    .unwrap();
    let replay = core.verify_journal_replay().unwrap();
    assert_eq!(
        replay.document_state_digest(),
        core.document_state_digest().unwrap()
    );
    let snapshot = core.build_snapshot();
    let digest = snapshot.canonical_composite_digest().unwrap().as_bytes();
    let checksum = u64::from_le_bytes(digest[..8].try_into().unwrap());
    let elapsed = started.elapsed();
    black_box((&core, &snapshot));
    ScenarioResult {
        scenario: "canonical_replay",
        elapsed,
        iterations: 5,
        input_items: 6,
        output_items: snapshot.tile_count() as u64,
        reused_items: 0,
        document_revision: core.document_info().unwrap().document_revision,
        history_entries: core.history_entries().len() as u64,
        successes: 5,
        failures: 0,
        checksum,
    }
}

fn sparse_snapshot(profile: Profile) -> ScenarioResult {
    let (mut core, coordinates) = sparse_core(profile.sparse_tiles);
    let started = Instant::now();
    let snapshot = core.build_snapshot();
    let elapsed = started.elapsed();
    let checksum = snapshot_checksum(&snapshot);
    assert_eq!(snapshot.tile_count(), coordinates.len());
    assert_eq!(core.history_entries().len(), coordinates.len());
    assert_eq!(snapshot.revision(), u64::from(profile.sparse_tiles) + 1);
    assert_eq!(
        snapshot.revision(),
        core.document_info()
            .expect("sparse document must remain available")
            .document_revision
    );
    black_box(&snapshot);

    ScenarioResult {
        scenario: "sparse_snapshot",
        elapsed,
        iterations: 1,
        input_items: profile.sparse_tiles.into(),
        output_items: snapshot.tile_count() as u64,
        reused_items: 0,
        document_revision: snapshot.revision(),
        history_entries: core.history_entries().len() as u64,
        successes: 1,
        failures: 0,
        checksum,
    }
}

fn dirty_tile_rebuild(profile: Profile) -> ScenarioResult {
    let (mut core, coordinates) = sparse_core(profile.sparse_tiles);
    let before = core.build_snapshot();
    let before_checksum = snapshot_checksum(&before);
    let mut previous_revisions = tile_revisions(&before);
    let (x, y) = coordinates[0];

    let mut after = before.clone();
    let mut rebuilt_total = 0_u64;
    let mut reused_total = 0_u64;
    let started = Instant::now();
    for step in 0..profile.dirty_rebuild_steps {
        let color = if step + 1 == profile.dirty_rebuild_steps {
            [220, 30, 10, 255]
        } else if step % 2 == 0 {
            [221, 31, 11, 255]
        } else {
            [219, 29, 9, 255]
        };
        paint_pixel(&mut core, x, y, color);
        after = core.build_snapshot();
        let after_revisions = tile_revisions(&after);
        let rebuilt = after_revisions
            .iter()
            .filter(|(tile_id, revision)| previous_revisions.get(tile_id) != Some(revision))
            .count();
        let reused = after_revisions.len() - rebuilt;
        assert_eq!(after.tile_count(), before.tile_count());
        assert_eq!(rebuilt, 1);
        assert_eq!(reused + rebuilt, profile.sparse_tiles as usize);
        rebuilt_total += rebuilt as u64;
        reused_total += reused as u64;
        previous_revisions = after_revisions;
        black_box(&after);
    }
    let elapsed = started.elapsed();
    assert_eq!(
        core.history_entries().len(),
        (profile.sparse_tiles + profile.dirty_rebuild_steps) as usize
    );
    assert_eq!(
        after.revision(),
        u64::from(profile.sparse_tiles + profile.dirty_rebuild_steps) + 1
    );
    assert_ne!(before_checksum, snapshot_checksum(&after));
    let checksum = snapshot_checksum(&after);
    black_box(&after);

    ScenarioResult {
        scenario: "dirty_tile_rebuild",
        elapsed,
        iterations: u64::from(profile.dirty_rebuild_steps),
        input_items: before.tile_count() as u64 * u64::from(profile.dirty_rebuild_steps),
        output_items: rebuilt_total,
        reused_items: reused_total,
        document_revision: after.revision(),
        history_entries: core.history_entries().len() as u64,
        successes: u64::from(profile.dirty_rebuild_steps),
        failures: 0,
        checksum,
    }
}

fn pan_zoom_snapshot(profile: Profile) -> ScenarioResult {
    let (mut core, _) = sparse_core(profile.sparse_tiles);
    let before = core.build_snapshot();
    let before_revisions = tile_revisions(&before);
    let document_revision = core
        .document_info()
        .expect("pan/zoom document must be available")
        .document_revision;
    let history_entries = core.history_entries().len();

    let mut after = before.clone();
    let started = Instant::now();
    for step in 0..profile.pan_zoom_steps {
        core.apply_view(ViewCommand::ZoomAt {
            factor: if step % 2 == 0 { 1.01 } else { 1.0 / 1.01 },
            device_x: 0.5,
            device_y: 0.5,
        })
        .expect("bounded zoom must succeed");
        core.apply_view(ViewCommand::PanBy {
            device_dx: if step % 2 == 0 { 1.0 } else { -1.0 },
            device_dy: if step % 2 == 0 { -0.5 } else { 0.5 },
        })
        .expect("bounded pan must succeed");
        after = core.build_snapshot();
        black_box(&after);
    }
    let elapsed = started.elapsed();

    assert_eq!(after.revision(), document_revision);
    assert_eq!(document_revision, u64::from(profile.sparse_tiles) + 1);
    assert_eq!(core.history_entries().len(), history_entries);
    assert_eq!(history_entries, profile.sparse_tiles as usize);
    assert_eq!(tile_revisions(&after), before_revisions);
    assert_eq!(snapshot_checksum(&after), snapshot_checksum(&before));
    assert_eq!(
        after.view().revision(),
        before.view().revision() + u64::from(profile.pan_zoom_steps) * 2
    );
    let checksum = snapshot_checksum(&after);
    black_box(&after);

    ScenarioResult {
        scenario: "pan_zoom_snapshot",
        elapsed,
        iterations: u64::from(profile.pan_zoom_steps) * 2,
        input_items: before.tile_count() as u64,
        output_items: after.tile_count() as u64,
        reused_items: after.tile_count() as u64,
        document_revision,
        history_entries: history_entries as u64,
        successes: u64::from(profile.pan_zoom_steps) * 2,
        failures: 0,
        checksum,
    }
}

fn undo_redo(profile: Profile) -> ScenarioResult {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        4_096,
        4_096,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        BENCHMARK_UUID + 1,
    )
    .expect("bounded Undo/Redo document must be valid");
    core.set_active_plane(ActivePlane::Color)
        .expect("color plane must exist");
    // The fixture's active target is pre-measurement state. Establish its
    // canonical editor savepoint so the existing Undo assertion continues to
    // measure the document savepoint rather than independent editor-state dirty.
    // Production v2 save never uses this token-only benchmark setup.
    let editor_savepoint = core
        .editor_savepoint_token()
        .expect("benchmark editor state must exist");
    core.commit_editor_savepoint(editor_savepoint)
        .expect("benchmark editor savepoint must be current");

    let started = Instant::now();
    for index in 0..profile.undo_edits {
        let x = 1 + (index * 131 * TILE_SIZE) % (4_096 - TILE_SIZE);
        let y = 1 + (index * 197 * TILE_SIZE) % (4_096 - TILE_SIZE);
        paint_pixel(&mut core, x, y, [index as u8, 80, 160, 255]);
    }
    let edited = core.build_snapshot();
    let edited_checksum = snapshot_checksum(&edited);
    assert_eq!(core.history_entries().len(), profile.undo_edits as usize);
    for _ in 0..profile.undo_edits {
        core.undo().expect("each edit must be undoable");
    }
    let undone = core.build_snapshot();
    assert_eq!(undone.tile_count(), 0);
    assert!(!core.document_info().expect("document must exist").dirty);
    for _ in 0..profile.undo_edits {
        core.redo().expect("each edit must be redoable");
    }
    let redone = core.build_snapshot();
    let elapsed = started.elapsed();

    assert_eq!(snapshot_checksum(&redone), edited_checksum);
    assert_eq!(core.history_cursor(), profile.undo_edits as usize);
    assert!(core.document_info().expect("document must exist").dirty);
    black_box(&redone);

    ScenarioResult {
        scenario: "undo_redo",
        elapsed,
        iterations: u64::from(profile.undo_edits) * 3,
        input_items: profile.undo_edits.into(),
        output_items: redone.tile_count() as u64,
        reused_items: 0,
        document_revision: redone.revision(),
        history_entries: core.history_entries().len() as u64,
        successes: u64::from(profile.undo_edits) * 3,
        failures: 0,
        checksum: edited_checksum,
    }
}

fn light_table_composite(profile: Profile) -> ScenarioResult {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        profile.light_table_side,
        profile.light_table_side,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        BENCHMARK_UUID + 2,
    )
    .expect("bounded light-table document must be valid");
    let frame = centered_frame(profile.light_table_side, profile.light_table_side);
    for index in 0..profile.light_table_references {
        let source = LightTableSource::from_rgba_bytes(
            BENCHMARK_UUID + 100 + u128::from(index),
            u64::from(index) + 1,
            frame,
            patterned_rgba(
                profile.light_table_side,
                profile.light_table_side,
                index as u8,
            ),
        )
        .expect("bounded light-table source must be valid");
        let mut input = LightTableItemInput::new(format!("Reference {index:02}"), source);
        input.opacity_milli = 350 + index * 80;
        input.translate_x_milli = (index as i32 % 3 - 1) * 1_000;
        input.translate_y_milli = (index as i32 % 2) * 1_000;
        core.light_table_add_item(input)
            .expect("light-table item must be accepted");
    }

    let started = Instant::now();
    let snapshot = core.build_snapshot();
    let elapsed = started.elapsed();
    let expected_tiles = profile.light_table_side.div_ceil(TILE_SIZE).pow(2) as usize;
    assert_eq!(
        core.light_table_items()
            .expect("light-table items must be readable")
            .len(),
        profile.light_table_references as usize
    );
    assert_eq!(snapshot.tile_count(), expected_tiles);
    assert_eq!(
        snapshot.revision(),
        u64::from(profile.light_table_references) + 1
    );
    assert_eq!(
        core.history_entries().len(),
        profile.light_table_references as usize
    );
    let checksum = snapshot_checksum(&snapshot);
    assert_ne!(checksum, fnv1a64(&[]));
    black_box(&snapshot);

    ScenarioResult {
        scenario: "light_table_composite",
        elapsed,
        iterations: 1,
        input_items: profile.light_table_references.into(),
        output_items: snapshot.tile_count() as u64,
        reused_items: 0,
        document_revision: snapshot.revision(),
        history_entries: core.history_entries().len() as u64,
        successes: 1,
        failures: 0,
        checksum,
    }
}

fn vector_snapshot(profile: Profile) -> ScenarioResult {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        profile.vector_side,
        profile.vector_side,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        BENCHMARK_UUID + 3,
    )
    .expect("bounded vector document must be valid");
    let (_, layer_id) = core
        .create_layer(LayerKind::VectorColoring, "Benchmark vectors")
        .expect("vector layer creation must succeed");
    let (_, trace_plane, fill_plane) = core
        .vector_layer_planes(layer_id)
        .expect("required vector planes must exist");
    let columns = if profile.vector_paths <= 8 { 4 } else { 8 };
    let cell = profile.vector_side / columns;
    for index in 0..profile.vector_paths {
        let column = index % columns;
        let row = index / columns;
        let x0 = (column * cell + 2) as f32;
        let y0 = (row * cell + 2) as f32;
        let x1 = (column * cell + cell - 2) as f32;
        let y1 = (row * cell + cell - 2) as f32;
        let (_, path_id) = core
            .vector_add_path(trace_plane, vector_rectangle(x0, y0, x1, y1, index as u8))
            .expect("bounded vector path must be accepted");
        core.vector_add_fill(
            fill_plane,
            &[path_id],
            PixelValue::Rgba([40, index as u8, 180, 220]),
        )
        .expect("bounded vector fill must be accepted");
    }

    let started = Instant::now();
    let snapshot = core.build_snapshot();
    let rasterized = core
        .rasterize_vector_layer(layer_id, 1, true)
        .expect("bounded vector rasterization must succeed");
    let elapsed = started.elapsed();

    assert_eq!(
        snapshot.vector_segments().len(),
        profile.vector_paths as usize * 4
    );
    assert_eq!(snapshot.vector_fills().len(), profile.vector_paths as usize);
    assert_eq!(snapshot.tile_count(), 0);
    assert_eq!(snapshot.revision(), u64::from(profile.vector_paths) * 2 + 2);
    assert_eq!(
        core.history_entries().len(),
        profile.vector_paths as usize * 2 + 1
    );
    assert_eq!(rasterized.width, profile.vector_side);
    assert_eq!(rasterized.height, profile.vector_side);
    let checksum = hash_combine(
        vector_snapshot_checksum(&snapshot),
        fnv1a64(&rasterized.pixels),
    );
    black_box((&snapshot, &rasterized));

    ScenarioResult {
        scenario: "vector_snapshot",
        elapsed,
        iterations: 2,
        input_items: profile.vector_paths.into(),
        output_items: (snapshot.vector_segments().len() + snapshot.vector_fills().len()) as u64,
        reused_items: 0,
        document_revision: snapshot.revision(),
        history_entries: core.history_entries().len() as u64,
        successes: 2,
        failures: 0,
        checksum,
    }
}

fn batch_preview(profile: Profile) -> ScenarioResult {
    let mut core = Core::new();
    let cells = (0..profile.batch_cells)
        .map(|index| {
            SequenceCellSource::from_rgba_bytes(
                format!("A{:04}", index + 1),
                BENCHMARK_UUID + 1_000 + u128::from(index),
                patterned_rgba(profile.batch_side, profile.batch_side, index as u8),
            )
            .expect("bounded sequence source must be valid")
        })
        .collect();
    core.set_sequence(cells)
        .expect("bounded benchmark sequence must be valid");

    let output_folder = benchmark_non_output_path(profile);
    assert!(
        !output_folder.exists(),
        "benchmark output path must start absent"
    );
    let graph = BatchGraph {
        version: inkpod_format::BATCH_GRAPH_VERSION,
        name: "Core benchmark".to_owned(),
        inputs: vec![BatchInputSelector::current_sequence()],
        operations: vec![BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(BatchTargetSelector::color_plane()),
            kind: BatchOperationKind::Filter(Filter::Invert {
                channel: Channel::Rgb,
            }),
        }],
        output: BatchOutputSettings {
            folder: output_folder.to_string_lossy().into_owned(),
            ..BatchOutputSettings::default()
        },
    };
    let mut invalid_graph = graph.clone();
    invalid_graph.version = 0;
    assert!(invalid_graph.validate().is_err());

    let started = Instant::now();
    let preview = core
        .batch_preview(&graph, BatchRunScope::All)
        .expect("valid in-memory Batch preview must succeed");
    let report = core
        .batch_execute(
            &graph,
            BatchRunOptions {
                scope: BatchRunScope::All,
                dry_run: true,
                preview_confirmed: true,
            },
            |_, _| true,
        )
        .expect("valid in-memory Batch dry-run must succeed");
    let elapsed = started.elapsed();

    let successes = report
        .items
        .iter()
        .filter(|item| item.outcome == BatchItemOutcome::DryRun)
        .count();
    assert_eq!(preview.items.len(), profile.batch_cells as usize);
    assert_eq!(successes, profile.batch_cells as usize);
    assert_eq!(report.failure_count(), 0);
    assert!(!report.cancelled);
    assert!(
        !output_folder.exists(),
        "Batch dry-run must not create output"
    );
    let checksum = batch_checksum(&preview, &report);
    black_box((&preview, &report));

    ScenarioResult {
        scenario: "batch_preview",
        elapsed,
        iterations: 2,
        input_items: profile.batch_cells.into(),
        output_items: (preview.items.len() + report.items.len()) as u64,
        reused_items: 0,
        document_revision: 0,
        history_entries: 0,
        successes: successes as u64,
        failures: 1,
        checksum,
    }
}

fn sparse_core(tile_count: u32) -> (Core, Vec<(u32, u32)>) {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        MAX_RASTER_DIMENSION,
        MAX_RASTER_DIMENSION,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        BENCHMARK_UUID,
    )
    .expect("maximum-dimension sparse Core document must be valid");
    core.set_active_plane(ActivePlane::Color)
        .expect("color plane must exist");
    let mut coordinates = Vec::with_capacity(tile_count as usize);
    for index in 0..tile_count {
        let x = 1 + (index * 257 * TILE_SIZE) % (MAX_RASTER_DIMENSION - TILE_SIZE);
        let y = 1 + (index * 509 * TILE_SIZE) % (MAX_RASTER_DIMENSION - TILE_SIZE);
        paint_pixel(&mut core, x, y, [10 + index as u8, 40, 90, 255]);
        coordinates.push((x, y));
    }
    (core, coordinates)
}

fn paint_pixel(core: &mut Core, x: u32, y: u32, color: [u8; 4]) {
    core.apply_stroke(&Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::Color,
        color,
        diameter: 1.0,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![StrokeSample {
            x: x as f32,
            y: y as f32,
            pressure: 1.0,
        }],
    })
    .expect("bounded benchmark pixel edit must succeed");
}

fn patterned_rgba(width: u32, height: u32, salt: u8) -> RgbaRasterBytes {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[
                (x as u8).wrapping_mul(3).wrapping_add(salt),
                (y as u8).wrapping_mul(5).wrapping_add(salt.wrapping_mul(7)),
                (x as u8)
                    .wrapping_add(y as u8)
                    .wrapping_add(salt.wrapping_mul(11)),
                255,
            ]);
        }
    }
    RgbaRasterBytes {
        width,
        height,
        pixel_format: PixelFormat::StraightRgba8,
        dpi_x_milli: Some(DEFAULT_DPI_MILLI),
        dpi_y_milli: Some(DEFAULT_DPI_MILLI),
        pixels,
    }
}

fn centered_frame(width: u32, height: u32) -> RectI32 {
    RectI32 {
        x: (width / 2) as i32,
        y: (height / 2) as i32,
        width: width as i32,
        height: height as i32,
    }
}

fn vector_rectangle(x0: f32, y0: f32, x1: f32, y1: f32, salt: u8) -> VectorPathInput {
    let points = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
    let segments = points
        .windows(2)
        .map(|pair| {
            let start = pair[0];
            let end = pair[1];
            let third_x = (end.0 - start.0) / 3.0;
            let third_y = (end.1 - start.1) / 3.0;
            VectorCubicSegment {
                p0: PointF32 {
                    x: start.0,
                    y: start.1,
                },
                p1: PointF32 {
                    x: start.0 + third_x,
                    y: start.1 + third_y,
                },
                p2: PointF32 {
                    x: start.0 + third_x * 2.0,
                    y: start.1 + third_y * 2.0,
                },
                p3: PointF32 { x: end.0, y: end.1 },
                width_start: 1.0 + f32::from(salt % 3),
                width_end: 1.0 + f32::from((salt + 1) % 3),
            }
        })
        .collect();
    VectorPathInput {
        segments,
        color: PixelValue::Rgba([20, 40 + salt, 80, 255]),
        closed: true,
    }
}

fn tile_revisions(snapshot: &RenderSnapshot) -> BTreeMap<u64, u64> {
    snapshot
        .tiles()
        .iter()
        .map(|tile| (tile.tile_id(), tile.tile_revision()))
        .collect()
}

fn snapshot_checksum(snapshot: &RenderSnapshot) -> u64 {
    let mut hash = Fnv1a64::new();
    for tile in snapshot.tiles() {
        hash.write(&tile.tile_id().to_le_bytes());
        hash.write(&tile.origin_x().to_le_bytes());
        hash.write(&tile.origin_y().to_le_bytes());
        hash.write(&tile.width().to_le_bytes());
        hash.write(&tile.height().to_le_bytes());
        hash.write(&tile.stride_bytes().to_le_bytes());
        hash.write(tile.pixels());
    }
    hash.finish()
}

fn vector_snapshot_checksum(snapshot: &RenderSnapshot) -> u64 {
    let mut hash = Fnv1a64::new();
    for segment in snapshot.vector_segments() {
        hash.write(&segment.path_id.to_le_bytes());
        hash.write(&segment.plane_id.to_le_bytes());
        hash.write(&segment.z_order.to_le_bytes());
        hash.write(&segment.segment_index.to_le_bytes());
        hash.write(&segment.segment_count.to_le_bytes());
        hash.write(&segment.color_rgba);
        hash.write(&[segment.closed as u8, segment.stroke_visible as u8]);
        for value in [
            segment.cubic.p0.x,
            segment.cubic.p0.y,
            segment.cubic.p1.x,
            segment.cubic.p1.y,
            segment.cubic.p2.x,
            segment.cubic.p2.y,
            segment.cubic.p3.x,
            segment.cubic.p3.y,
            segment.cubic.width_start,
            segment.cubic.width_end,
        ] {
            hash.write(&value.to_bits().to_le_bytes());
        }
    }
    for fill in snapshot.vector_fills() {
        hash.write(&fill.fill_id.to_le_bytes());
        hash.write(&fill.plane_id.to_le_bytes());
        hash.write(&fill.z_order.to_le_bytes());
        hash.write(&fill.color_rgba);
        for boundary in &fill.boundary_path_ids {
            hash.write(&boundary.to_le_bytes());
        }
    }
    hash.finish()
}

fn batch_checksum(preview: &BatchPreview, report: &BatchRunReport) -> u64 {
    let mut hash = Fnv1a64::new();
    for item in &preview.items {
        hash.write(item.input_name.as_bytes());
        hash.write(&(item.warnings.len() as u64).to_le_bytes());
    }
    for item in &report.items {
        hash.write(item.input_name.as_bytes());
        let outcome = match item.outcome {
            BatchItemOutcome::Succeeded => 1_u8,
            BatchItemOutcome::Skipped => 2,
            BatchItemOutcome::Failed => 3,
            BatchItemOutcome::Cancelled => 4,
            BatchItemOutcome::DryRun => 5,
        };
        hash.write(&[outcome]);
    }
    hash.write(&[report.cancelled as u8]);
    hash.finish()
}

fn benchmark_non_output_path(profile: Profile) -> PathBuf {
    std::env::temp_dir().join(format!(
        "inkpod-core-benchmark-no-output-{}-{}",
        std::process::id(),
        profile.name
    ))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = Fnv1a64::new();
    hash.write(bytes);
    hash.finish()
}

fn hash_combine(left: u64, right: u64) -> u64 {
    let mut hash = Fnv1a64::new();
    hash.write(&left.to_le_bytes());
    hash.write(&right.to_le_bytes());
    hash.finish()
}

struct Fnv1a64(u64);

impl Fnv1a64 {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    const fn finish(self) -> u64 {
        self.0
    }
}
