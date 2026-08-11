use super::*;

fn rgba_asset(format: PixelFormat, pixels: Vec<u8>, width: u32, uuid: u128) -> Core {
    let mut core = Core::new();
    core.new_cell_from_raster_asset(
        RasterAssetInput {
            width,
            height: 1,
            pixel_format: format,
            color_space: Some(AssetColorSpace::Srgb),
            alpha_semantics: AssetAlphaSemantics::Straight,
            canonical_stride: u64::from(width) * format.bytes_per_pixel() as u64,
            pixels,
            expected_id: None,
        },
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        uuid,
    )
    .unwrap();
    core
}

fn rgba16_bytes(pixels: &[[u16; 4]]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|pixel| pixel.iter().flat_map(|channel| channel.to_le_bytes()))
        .collect()
}

fn guard_fixture_8(uuid: u128) -> Core {
    rgba_asset(
        PixelFormat::StraightRgba8,
        vec![
            16, 16, 16, 255, 235, 235, 235, 255, 15, 15, 15, 255, 236, 236, 236, 255, 255, 0, 0,
            128, 0, 0, 255, 0,
        ],
        6,
        uuid,
    )
}

fn guard_fixture_16(uuid: u128) -> Core {
    rgba_asset(
        PixelFormat::StraightRgba16,
        rgba16_bytes(&[
            [16 * 257, 16 * 257, 16 * 257, 65_535],
            [235 * 257, 235 * 257, 235 * 257, 65_535],
            [15 * 257, 15 * 257, 15 * 257, 65_535],
            [236 * 257, 236 * 257, 236 * 257, 65_535],
            [65_535, 0, 0, 32_768],
            [0, 0, 65_535, 0],
        ]),
        6,
        uuid,
    )
}

#[test]
fn visible_composite_guard_is_native_depth_equivalent_and_one_replayable_undo_unit() {
    for mut core in [guard_fixture_8(0x5401), guard_fixture_16(0x5402)] {
        let base_revision = core.document_info().unwrap().document_revision;
        let before_pixels = core.document_state_digest().unwrap();
        let result = core
            .select_output_color_guard(
                OutputColorGuardProfile::Bt709ConservativeYCbCr,
                SelectionOperation::New,
                base_revision,
            )
            .unwrap();
        assert_eq!(result.summary.scanned_pixel_count, 5);
        assert_eq!(result.summary.selected_pixel_count, 3);
        assert_eq!(result.summary.transparent_pixel_count, 1);
        assert_eq!(result.dispatch.revision(), base_revision + 1);
        assert_eq!(
            core.selection_bounds().unwrap(),
            Some(RectI32 {
                x: 2,
                y: 0,
                width: 3,
                height: 1,
            })
        );
        assert_eq!(
            core.journal_entries()
                .iter()
                .rev()
                .find_map(|entry| match entry {
                    JournalEntry::Commit(commit) => Some(commit.procedure().primitive_id()),
                    JournalEntry::HistoryMove(_) | JournalEntry::BranchCut(_) => None,
                }),
            Some(PrimitiveId::SELECT_OUTPUT_COLOR_GUARD)
        );
        assert_ne!(core.document_state_digest().unwrap(), before_pixels);
        core.verify_journal_replay().unwrap();

        let before_noop = core.journal_state();
        let noop = core
            .select_output_color_guard(
                OutputColorGuardProfile::Bt709ConservativeYCbCr,
                SelectionOperation::New,
                core.document_info().unwrap().document_revision,
            )
            .unwrap();
        assert_eq!(noop.dispatch.revision(), result.dispatch.revision());
        assert_eq!(core.journal_state(), before_noop);

        core.undo().unwrap();
        assert_eq!(core.selection_bounds().unwrap(), None);
        core.redo().unwrap();
        assert_eq!(core.selection_bounds().unwrap().unwrap().width, 3);
        core.verify_journal_replay().unwrap();
    }
}

#[test]
fn empty_new_cancel_and_stale_guard_requests_are_atomic() {
    let mut safe = rgba_asset(
        PixelFormat::StraightRgba8,
        vec![128, 128, 128, 255],
        1,
        0x5403,
    );
    let base = safe.document_info().unwrap().document_revision;
    let empty = safe
        .select_output_color_guard(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            base,
        )
        .unwrap();
    assert_eq!(empty.summary.selected_pixel_count, 0);
    assert_eq!(empty.dispatch.revision(), base);

    safe.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    let selected_revision = safe.document_info().unwrap().document_revision;
    let cleared = safe
        .select_output_color_guard(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            selected_revision,
        )
        .unwrap();
    assert_eq!(cleared.summary.selected_pixel_count, 0);
    assert_eq!(cleared.dispatch.revision(), selected_revision + 1);
    assert_eq!(safe.selection_bounds().unwrap(), None);

    let mut cancelled = guard_fixture_8(0x5404);
    let before_cancel = cancelled.document_state_digest().unwrap();
    let base = cancelled.document_info().unwrap().document_revision;
    assert_eq!(
        cancelled.select_output_color_guard_with_cancel(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            base,
            |_completed, _total| false,
        ),
        Err(CoreError::Cancelled)
    );
    assert_eq!(cancelled.document_state_digest().unwrap(), before_cancel);

    let stale_base = cancelled.document_info().unwrap().document_revision;
    cancelled
        .apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
            SelectionOperation::New,
        )
        .unwrap();
    let before_stale = cancelled.document_state_digest().unwrap();
    assert!(matches!(
        cancelled.select_output_color_guard(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            stale_base,
        ),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(cancelled.document_state_digest().unwrap(), before_stale);
}

#[test]
fn guard_selection_round_trips_through_current_native_save_and_reopen() {
    let mut core = guard_fixture_8(0x5405);
    let base = core.document_info().unwrap().document_revision;
    core.select_output_color_guard(
        OutputColorGuardProfile::Bt709ConservativeYCbCr,
        SelectionOperation::New,
        base,
    )
    .unwrap();
    let path = std::env::temp_dir().join(format!(
        "inkpod-output-color-guard-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_file(&path);
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.selection_bounds(), core.selection_bounds());
    reopened.verify_journal_replay().unwrap();
    fs::remove_file(path).unwrap();
}

#[test]
fn guard_scans_visible_committed_layers_and_excludes_solid_paper_and_selection_overlay() {
    let mut core = Core::new();
    core.new_cell(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    core.apply_stroke(&Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::Color,
        color: [255, 0, 0, 255],
        diameter: 1.0,
        shape: BrushShape::Round,
        smoothing: 0,
        start_color: StartColorPredicate::Any,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![StrokeSample {
            x: 0.5,
            y: 0.5,
            pressure: 1.0,
        }],
    })
    .unwrap();
    let base = core.document_info().unwrap().document_revision;
    let selected = core
        .select_output_color_guard(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            base,
        )
        .unwrap();
    assert_eq!(selected.summary.scanned_pixel_count, 1);
    assert_eq!(selected.summary.selected_pixel_count, 1);
    assert_eq!(selected.summary.transparent_pixel_count, 1);
    assert_eq!(core.selection_bounds().unwrap().unwrap().width, 1);

    let layer = core.layers().unwrap().remove(0);
    core.set_layer_properties(layer.id, false, true, 1_000, &layer.name)
        .unwrap();
    let hidden_base = core.document_info().unwrap().document_revision;
    let hidden = core
        .select_output_color_guard(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            hidden_base,
        )
        .unwrap();
    assert_eq!(hidden.summary.scanned_pixel_count, 0);
    assert_eq!(hidden.summary.selected_pixel_count, 0);
    assert_eq!(hidden.summary.transparent_pixel_count, 2);
    assert_eq!(hidden.dispatch.revision(), hidden_base + 1);
    assert_eq!(core.selection_bounds().unwrap(), None);
}

#[test]
fn guard_preserves_rgba16_vector_color_before_classification() {
    let mut core = Core::new();
    core.new_cell(3, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let (_, layer_id) = core
        .create_layer(LayerKind::VectorColoring, "Vector")
        .unwrap();
    let (main_plane_id, _, _) = core.vector_layer_planes(layer_id).unwrap();
    core.vector_add_path(
        main_plane_id,
        VectorPathInput {
            segments: vec![VectorCubicSegment {
                p0: PointF32 { x: 0.0, y: 0.5 },
                p1: PointF32 { x: 1.0, y: 0.5 },
                p2: PointF32 { x: 2.0, y: 0.5 },
                p3: PointF32 { x: 3.0, y: 0.5 },
                width_start: 1.0,
                width_end: 1.0,
            }],
            // 4,111 is one native-depth code below the inclusive Y minimum.
            // RGBA8 down-conversion would round it up to 16 * 257 and miss it.
            color: PixelValue::Rgba16([4_111, 4_111, 4_111, u16::MAX]),
            closed: false,
        },
    )
    .unwrap();

    let base = core.document_info().unwrap().document_revision;
    let result = core
        .select_output_color_guard(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            base,
        )
        .unwrap();
    assert!(result.summary.scanned_pixel_count > 0);
    assert_eq!(
        result.summary.selected_pixel_count,
        result.summary.scanned_pixel_count
    );
}

#[test]
fn large_sparse_guard_allocates_only_the_changed_selection_tile() {
    let width = 257_u32;
    let height = 257_u32;
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    let selected_x = 129_u32;
    let selected_y = 129_u32;
    let offset = (selected_y as usize * width as usize + selected_x as usize) * 4;
    pixels[offset..offset + 4].copy_from_slice(&[255, 0, 0, 255]);
    let mut core = Core::new();
    core.new_cell_from_raster_asset(
        RasterAssetInput {
            width,
            height,
            pixel_format: PixelFormat::StraightRgba8,
            color_space: Some(AssetColorSpace::Srgb),
            alpha_semantics: AssetAlphaSemantics::Straight,
            canonical_stride: u64::from(width) * 4,
            pixels,
            expected_id: None,
        },
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        0x5406,
    )
    .unwrap();
    let before = core.resource_usage();

    let base = core.document_info().unwrap().document_revision;
    let result = core
        .select_output_color_guard(
            OutputColorGuardProfile::Bt709ConservativeYCbCr,
            SelectionOperation::New,
            base,
        )
        .unwrap();
    assert_eq!(result.summary.scanned_pixel_count, 1);
    assert_eq!(result.summary.selected_pixel_count, 1);
    assert_eq!(
        result.summary.transparent_pixel_count,
        u64::from(width) * u64::from(height) - 1
    );
    assert_eq!(
        core.selection_bounds().unwrap(),
        Some(RectI32 {
            x: selected_x as i32,
            y: selected_y as i32,
            width: 1,
            height: 1,
        })
    );
    let after = core.resource_usage();
    assert_eq!(after.document_tile_count, before.document_tile_count + 1);
    assert_eq!(after.cpu_staging_bytes, 0);
}
