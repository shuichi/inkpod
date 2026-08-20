use super::*;

fn raster_asset(
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    pixels: Vec<u8>,
) -> RasterAssetInput {
    let (color_space, alpha_semantics) = match pixel_format {
        PixelFormat::BinaryMask8 => (None, AssetAlphaSemantics::CoverageMask),
        PixelFormat::Grayscale8 | PixelFormat::Grayscale16 => (None, AssetAlphaSemantics::Opaque),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16 => {
            (Some(AssetColorSpace::Srgb), AssetAlphaSemantics::Straight)
        }
        PixelFormat::PremultipliedBgra8 => panic!("display-only format is not canonical"),
    };
    RasterAssetInput {
        width,
        height,
        pixel_format,
        color_space,
        alpha_semantics,
        canonical_stride: u64::from(width) * pixel_format.bytes_per_pixel() as u64,
        pixels,
        expected_id: None,
    }
}

fn rgba8_common(width: u32, height: u32, pixels: Vec<u8>) -> CommonRaster {
    CommonRaster::new(
        width,
        height,
        PixelFormat::StraightRgba8,
        Some(DEFAULT_DPI_MILLI),
        Some(DEFAULT_DPI_MILLI),
        pixels,
    )
    .unwrap()
}

fn decoded_png(core: &Core) -> CommonRaster {
    inkpod_format::decode_common_raster(
        CommonRasterFormat::Png,
        &core
            .export_common_raster(CommonRasterFormat::Png, false)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn solid_white_is_only_the_immutable_flattened_underlay() {
    let mut core = Core::new();
    let document = core
        .new_cell_with_uuid(2, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x4101)
        .unwrap();

    let genesis = core.genesis_info().unwrap();
    assert_eq!(genesis.state_id, StateId::GENESIS);
    assert_eq!(genesis.document_id, document.document_id);
    assert_eq!(genesis.cell_id, document.cell_id);
    assert_ne!(genesis.document_id, genesis.cell_id);
    assert_eq!(genesis.base_surface, BaseSurface::SolidWhite);
    assert_eq!(core.asset_store_usage(), AssetStoreUsage::default());
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    assert_eq!(core.selection_bounds().unwrap(), None);

    let layer = core.layer_thumbnail(document.layer_id, 2, 2).unwrap();
    assert!(layer.pixels.iter().all(|byte| *byte == 0));
    let snapshot = core.build_snapshot();
    assert_eq!(
        snapshot.feature_flags() & SNAPSHOT_FEATURE_SOLID_WHITE_BASE,
        SNAPSHOT_FEATURE_SOLID_WHITE_BASE
    );
    assert!(snapshot.tiles().is_empty());
    assert_eq!(decoded_png(&core).pixels, vec![u8::MAX; 2 * 2 * 4]);
    assert_eq!(
        core.verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        core.document_state_digest().unwrap()
    );
}

#[test]
fn every_canonical_raster_format_and_transparent_color_can_be_genesis() {
    let cases = [
        (
            raster_asset(1, 1, PixelFormat::BinaryMask8, vec![255]),
            [0, 0, 0, 255],
        ),
        (
            raster_asset(1, 1, PixelFormat::Grayscale8, vec![91]),
            [91, 91, 91, 255],
        ),
        (
            raster_asset(
                1,
                1,
                PixelFormat::Grayscale16,
                32_896_u16.to_le_bytes().to_vec(),
            ),
            [128, 128, 128, 255],
        ),
        (
            raster_asset(1, 1, PixelFormat::StraightRgba8, vec![1, 2, 3, 4]),
            [1, 2, 3, 4],
        ),
        (
            raster_asset(
                1,
                1,
                PixelFormat::StraightRgba16,
                [257_u16, 514, 771, 1_028]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            ),
            [1, 2, 3, 4],
        ),
        (
            raster_asset(1, 1, PixelFormat::StraightRgba8, vec![11, 22, 33, 0]),
            [0, 0, 0, 0],
        ),
    ];

    for (index, (input, expected_flat_rgba8)) in cases.into_iter().enumerate() {
        let expected_format = input.pixel_format;
        let mut core = Core::new();
        core.new_cell_from_raster_asset(
            input,
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            0x4200 + index as u128,
        )
        .unwrap();
        let BaseSurface::Asset(id) = core.genesis_info().unwrap().base_surface else {
            panic!("decoded image must be an immutable Genesis asset");
        };
        let info = core.asset_info(id).unwrap();
        assert_eq!(info.descriptor.kind, AssetKind::CanonicalRaster);
        assert_eq!(info.descriptor.pixel_format, Some(expected_format));
        assert_eq!(info.reference_count, 2);
        assert_eq!(core.asset_store_usage().asset_count, 1);
        assert_eq!(
            decoded_png(&core).pixels,
            expected_flat_rgba8,
            "flat Genesis conversion changed for {expected_format:?}"
        );
        assert_eq!(
            core.verify_journal_replay()
                .unwrap()
                .document_state_digest(),
            core.document_state_digest().unwrap()
        );
    }

    let mut hidden_rgb = Core::new();
    hidden_rgb
        .new_cell_from_raster_asset(
            raster_asset(1, 1, PixelFormat::StraightRgba8, vec![11, 22, 33, 0]),
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            0x42f1,
        )
        .unwrap();
    let mut transparent_black = Core::new();
    transparent_black
        .new_cell_from_raster_asset(
            raster_asset(1, 1, PixelFormat::StraightRgba8, vec![0, 0, 0, 0]),
            DEFAULT_DPI_MILLI,
            DEFAULT_DPI_MILLI,
            0x42f2,
        )
        .unwrap();
    assert_ne!(
        hidden_rgb.genesis_info().unwrap().base_surface,
        transparent_black.genesis_info().unwrap().base_surface,
        "transparent hidden RGB remains lossless in immutable asset identity"
    );
}

#[test]
fn codec_path_and_external_file_lifetime_do_not_change_asset_identity() {
    let pixels = vec![10, 20, 30, 255, 40, 50, 60, 255];
    let common = rgba8_common(2, 1, pixels);
    let png = encode_common_raster(CommonRasterFormat::Png, &common, false).unwrap();
    let bmp = encode_common_raster(CommonRasterFormat::Bmp, &common, false).unwrap();

    let mut png_core = Core::new();
    png_core
        .import_common_raster(CommonRasterFormat::Png, &png, 0x4301)
        .unwrap();
    let BaseSurface::Asset(png_id) = png_core.genesis_info().unwrap().base_surface else {
        panic!("PNG must open as an asset base");
    };
    let mut bmp_core = Core::new();
    bmp_core
        .import_common_raster(CommonRasterFormat::Bmp, &bmp, 0x4302)
        .unwrap();
    let BaseSurface::Asset(bmp_id) = bmp_core.genesis_info().unwrap().base_surface else {
        panic!("BMP must open as an asset base");
    };
    assert_eq!(png_id, bmp_id);
    assert_eq!(decoded_png(&png_core).pixels, common.pixels);
    assert_eq!(decoded_png(&bmp_core).pixels, common.pixels);

    let sequence = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "inkpod-asset-lifetime-{}-{sequence}.png",
        std::process::id()
    ));
    fs::write(&path, &png).unwrap();
    let mut caller_bytes = fs::read(&path).unwrap();
    let mut file_core = Core::new();
    file_core
        .import_common_raster(CommonRasterFormat::Png, &caller_bytes, 0x4303)
        .unwrap();
    let expected = file_core
        .export_common_raster(CommonRasterFormat::Png, false)
        .unwrap();
    caller_bytes.fill(0xa5);
    fs::write(&path, b"not the imported image").unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(
        file_core
            .export_common_raster(CommonRasterFormat::Png, false)
            .unwrap(),
        expected
    );
    assert_eq!(
        file_core.genesis_info().unwrap().base_surface,
        BaseSurface::Asset(png_id)
    );

    let normal_path = std::env::temp_dir().join(format!(
        "inkpod-asset-base-save-{}-{sequence}.inkpod",
        std::process::id()
    ));
    let recovery_path = std::env::temp_dir().join(format!(
        "inkpod-asset-base-recovery-{}-{sequence}.inkpod",
        std::process::id()
    ));
    let sentinel = b"existing file is atomically replaced by an asset-base save";
    fs::write(&normal_path, sentinel).unwrap();
    fs::write(&recovery_path, sentinel).unwrap();
    let before_info = file_core.document_info().unwrap();
    let before_usage = file_core.asset_store_usage();
    file_core.save(&normal_path).unwrap();
    file_core.autosave(&recovery_path).unwrap();
    assert_ne!(fs::read(&normal_path).unwrap(), sentinel);
    assert_ne!(fs::read(&recovery_path).unwrap(), sentinel);
    assert!(!file_core.document_info().unwrap().dirty);
    assert_eq!(
        file_core.document_info().unwrap().document_uuid,
        before_info.document_uuid
    );
    assert_eq!(file_core.asset_store_usage(), before_usage);
    assert_eq!(
        file_core
            .export_common_raster(CommonRasterFormat::Png, false)
            .unwrap(),
        expected
    );
    let mut reopened = Core::new();
    reopened.open(&normal_path).unwrap();
    assert_eq!(
        reopened.genesis_info().unwrap().base_surface,
        BaseSurface::Asset(png_id)
    );
    assert_eq!(
        reopened
            .export_common_raster(CommonRasterFormat::Png, false)
            .unwrap(),
        expected
    );
    fs::remove_file(normal_path).unwrap();
    fs::remove_file(recovery_path).unwrap();
}

#[test]
fn existing_document_import_is_an_asset_only_canonical_procedure() {
    let mut core = Core::new();
    let document = core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x4401)
        .unwrap();
    let mut caller_pixels = vec![1, 2, 3, 255, 4, 5, 6, 128];
    let outcome = core
        .execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            raster: raster_asset(2, 1, PixelFormat::StraightRgba8, caller_pixels.clone()),
        })
        .unwrap();
    caller_pixels.fill(0);
    let procedure = outcome.procedure().unwrap();
    assert_eq!(procedure.primitive_id(), PrimitiveId::IMPORT_RASTER_ASSET);
    assert_eq!(procedure.primitive_schema_version(), 1);
    assert_eq!(procedure.input_ids(), &[document.color_plane_id]);
    assert_eq!(procedure.asset_ids().len(), 1);
    assert!(procedure.canonical_payload().is_empty());
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 1, 0).unwrap(),
        PixelValue::Rgba([4, 5, 6, 128])
    );
    assert_eq!(
        core.asset_info(procedure.asset_ids()[0])
            .unwrap()
            .reference_count,
        2
    );

    core.undo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 1, 0).unwrap(),
        PixelValue::Rgba([0; 4])
    );
    core.redo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 1, 0).unwrap(),
        PixelValue::Rgba([4, 5, 6, 128])
    );
    core.release_history_cache().unwrap();
    assert_eq!(
        core.verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        core.document_state_digest().unwrap()
    );
}

#[test]
fn import_raster_asset_no_op_and_stale_request_leave_every_public_state_unchanged() {
    let mut core = Core::new();
    let document = core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x4402)
        .unwrap();
    let before_info = core.document_info().unwrap();
    let before_digest = core.document_state_digest().unwrap();
    let before_journal_state = core.journal_state();
    let before_history = core.history_entries().to_vec();
    let before_journal = core.journal_entries().to_vec();
    let before_assets = core.asset_infos();
    let before_usage = core.asset_store_usage();
    let before_snapshot = core.build_snapshot();

    let no_op = core
        .execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            raster: raster_asset(2, 1, PixelFormat::StraightRgba8, vec![0; 2 * 4]),
        })
        .unwrap();
    assert!(no_op.procedure().is_none());
    assert_eq!(no_op.dispatch().revision(), document.document_revision);
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.document_state_digest().unwrap(), before_digest);
    assert_eq!(core.journal_state(), before_journal_state);
    assert_eq!(core.history_entries(), before_history);
    assert_eq!(core.journal_entries(), before_journal);
    assert_eq!(core.asset_infos(), before_assets);
    assert_eq!(core.asset_store_usage(), before_usage);
    assert_eq!(core.build_snapshot(), before_snapshot);

    assert!(matches!(
        core.execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: document.document_revision + 1,
            target_plane_id: document.color_plane_id,
            raster: raster_asset(2, 1, PixelFormat::StraightRgba8, vec![9; 2 * 4],),
        }),
        Err(CoreError::InvalidState(_))
    ));
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.document_state_digest().unwrap(), before_digest);
    assert_eq!(core.journal_state(), before_journal_state);
    assert_eq!(core.history_entries(), before_history);
    assert_eq!(core.journal_entries(), before_journal);
    assert_eq!(core.asset_infos(), before_assets);
    assert_eq!(core.asset_store_usage(), before_usage);
    assert_eq!(core.build_snapshot(), before_snapshot);
}

#[test]
fn sequence_attachment_recognizes_the_opened_genesis_without_replacing_it() {
    let mut core = Core::new();
    let expected = vec![31, 47, 63, 255, 71, 87, 103, 128];
    core.new_cell_from_raster_asset(
        raster_asset(2, 1, PixelFormat::StraightRgba8, expected.clone()),
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        0x4410,
    )
    .unwrap();
    let before = core.document_info().unwrap();
    let before_genesis = core.genesis_info().unwrap();
    let before_usage = core.asset_store_usage();
    let source = SequenceCellSource::from_common_raster(
        "cell2.png",
        0x4411,
        &rgba8_common(2, 1, expected.clone()),
    )
    .unwrap();
    core.set_sequence(vec![source]).unwrap();

    let activated = core.sequence_activate(0).unwrap();
    assert_eq!(activated, before);
    assert_eq!(core.genesis_info().unwrap(), before_genesis);
    assert_eq!(core.asset_store_usage(), before_usage);
    assert_eq!(core.sequence_cells().unwrap()[0].document_uuid, 0x4410);
    assert_eq!(decoded_png(&core).pixels, expected);
}

#[test]
fn forged_raster_descriptor_and_identity_are_atomic() {
    let mut core = Core::new();
    let document = core
        .new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x4501)
        .unwrap();
    let before_digest = core.document_state_digest().unwrap();
    let before_usage = core.asset_store_usage();
    let mut forged = raster_asset(
        2,
        1,
        PixelFormat::StraightRgba8,
        vec![1, 2, 3, 4, 5, 6, 7, 8],
    );
    forged.canonical_stride -= 1;
    assert!(matches!(
        core.execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            raster: forged,
        }),
        Err(CoreError::InvalidArgument(_))
    ));

    let mut forged_id = raster_asset(
        2,
        1,
        PixelFormat::StraightRgba8,
        vec![1, 2, 3, 4, 5, 6, 7, 8],
    );
    forged_id.expected_id = Some(AssetId::from_bytes([0x5a; 32]));
    assert!(matches!(
        core.execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            raster: forged_id,
        }),
        Err(CoreError::InvalidArgument(_))
    ));
    assert_eq!(core.document_info().unwrap(), document);
    assert_eq!(core.document_state_digest().unwrap(), before_digest);
    assert_eq!(core.asset_store_usage(), before_usage);
    assert!(core.history_entries().is_empty());
    assert!(core.journal_entries().is_empty());
}

#[test]
fn redo_and_inactive_branch_assets_survive_cache_release_full_replay_and_checkpoint() {
    let mut core = Core::new();
    let document = core
        .new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x4601)
        .unwrap();
    let first = core
        .execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            raster: raster_asset(1, 1, PixelFormat::StraightRgba8, vec![1, 2, 3, 255]),
        })
        .unwrap()
        .procedure()
        .unwrap()
        .asset_ids()[0];
    core.undo().unwrap();
    assert_eq!(core.collect_unreferenced_assets().unwrap(), 0);
    assert!(
        core.asset_info(first).is_some(),
        "redo-only asset was collected"
    );

    let revision = core.document_info().unwrap().document_revision;
    let second = core
        .execute_primitive(PrimitiveRequest::ImportRasterAsset {
            expected_revision: revision,
            target_plane_id: document.color_plane_id,
            raster: raster_asset(1, 1, PixelFormat::StraightRgba8, vec![9, 8, 7, 255]),
        })
        .unwrap()
        .procedure()
        .unwrap()
        .asset_ids()[0];
    assert_ne!(first, second);
    assert_eq!(core.collect_unreferenced_assets().unwrap(), 0);
    assert_eq!(core.asset_store_usage().asset_count, 2);
    assert!(
        core.asset_infos()
            .iter()
            .all(|asset| asset.reference_count > 0)
    );

    core.release_history_cache().unwrap();
    assert_eq!(
        core.verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        core.document_state_digest().unwrap()
    );
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([9, 8, 7, 255])
    );

    for index in 0..254 {
        let value = if index % 2 == 0 { 11 } else { 12 };
        core.set_main_line_color(PixelValue::Rgba([value, value, value, 255]))
            .unwrap();
    }
    assert!(core.persistence_info().unwrap().checkpoint_due);
    let path = std::env::temp_dir().join(format!(
        "inkpod-checkpoint-assets-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    core.save(&path).unwrap();
    let expected_digest = core.document_state_digest().unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(
        reopened.persistence_info().unwrap().open_strategy,
        NativeOpenStrategy::Checkpoint
    );
    assert_eq!(reopened.document_state_digest().unwrap(), expected_digest);
    assert_eq!(reopened.asset_store_usage().asset_count, 2);
    assert!(reopened.asset_info(first).is_some());
    assert!(reopened.asset_info(second).is_some());
    reopened.undo().unwrap();
    reopened.redo().unwrap();
    assert_eq!(reopened.document_state_digest().unwrap(), expected_digest);
    fs::remove_file(path).unwrap();
}

#[test]
fn large_stroke_payload_promotes_to_bounded_sample_asset() {
    const SAMPLE_COUNT: usize = 175_000;
    let mut core = Core::new();
    let document = core
        .new_cell_with_uuid(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x4701)
        .unwrap();
    let outcome = core
        .execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision: document.document_revision,
            target_plane_id: document.color_plane_id,
            stroke: Stroke {
                tool: PaintTool::Pencil,
                plane: ActivePlane::Color,
                color: [20, 30, 40, 255],
                diameter: 1.0,
                shape: BrushShape::Round,
                smoothing: 0,
                start_color: StartColorPredicate::Any,
                auto_erase: false,
                pressure_size: false,
                coordinate_space: CoordinateSpace::Document,
                samples: vec![
                    StrokeSample {
                        x: 0.0,
                        y: 0.0,
                        pressure: 1.0,
                    };
                    SAMPLE_COUNT
                ],
            },
        })
        .unwrap();
    let procedure = outcome.procedure().unwrap();
    assert!(procedure.canonical_payload().is_empty());
    assert_eq!(procedure.asset_ids().len(), 1);
    let asset = core.asset_info(procedure.asset_ids()[0]).unwrap();
    assert_eq!(asset.descriptor.kind, AssetKind::CanonicalSampleStream);
    assert_eq!(asset.descriptor.logical_element_count, SAMPLE_COUNT as u64);
    assert_eq!(asset.reference_count, 2);
    core.release_history_cache().unwrap();
    assert_eq!(
        core.verify_journal_replay()
            .unwrap()
            .document_state_digest(),
        core.document_state_digest().unwrap()
    );
}

#[test]
fn batch_copies_asset_backed_sources_and_writes_current_native_output() {
    let mut core = Core::new();
    core.import_common_raster(
        CommonRasterFormat::Png,
        &encode_common_raster(
            CommonRasterFormat::Png,
            &rgba8_common(1, 1, vec![12, 34, 56, 255]),
            false,
        )
        .unwrap(),
        0x4751,
    )
    .unwrap();
    let before_info = core.document_info().unwrap();
    let before_digest = core.document_state_digest().unwrap();
    let before_usage = core.asset_store_usage();
    let sequence = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "inkpod-asset-batch-{}-{sequence}",
        std::process::id()
    ));
    let output = root.join("output");
    fs::create_dir_all(&root).unwrap();
    let graph = BatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "asset-base".to_owned(),
        inputs: vec![BatchInputSelector::current_sequence()],
        operations: vec![BatchOperation {
            version: BATCH_OPERATION_VERSION,
            enabled: true,
            configure_each_run: false,
            target: Some(BatchTargetSelector::color_plane()),
            kind: BatchOperationKind::ColorReplace(vec![BatchColorPair {
                enabled: true,
                old: PixelValue::Rgba([1, 2, 3, 255]),
                new: PixelValue::Rgba([3, 2, 1, 255]),
            }]),
        }],
        output: BatchOutputSettings {
            folder: output.to_string_lossy().into_owned(),
            ..BatchOutputSettings::default()
        },
    };

    let dry_run = core
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
    assert_eq!(dry_run.items[0].outcome, BatchItemOutcome::DryRun);
    assert!(!output.exists());

    let report = core
        .batch_execute(
            &graph,
            BatchRunOptions {
                scope: BatchRunScope::Current,
                dry_run: false,
                preview_confirmed: true,
            },
            |_, _| true,
        )
        .unwrap();
    assert_eq!(report.failure_count(), 0);
    assert_eq!(report.items[0].outcome, BatchItemOutcome::Succeeded);
    let output_path = report.items[0]
        .output_path
        .as_ref()
        .expect("successful native batch output has a path");
    assert!(output_path.exists());
    let mut reopened = Core::new();
    reopened.open(output_path).unwrap();
    assert!(matches!(
        reopened.genesis_info().unwrap().base_surface,
        BaseSurface::Asset(_)
    ));
    assert_eq!(core.document_info().unwrap(), before_info);
    assert_eq!(core.document_state_digest().unwrap(), before_digest);
    assert_eq!(core.asset_store_usage(), before_usage);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn clipboard_pixels_are_interned_for_preview_and_released_after_commit() {
    let mut core = Core::new();
    core.new_cell_with_uuid(2, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI, 0x4801)
        .unwrap();
    let mut payload = ClipboardPayload {
        source_document_uuid: 0x4802,
        bounds: RectI32 {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        planes: vec![ClipboardPlane {
            kind: PlaneType::Color,
            pixel_format: PixelFormat::StraightRgba8,
            origin_x: 0,
            origin_y: 0,
            pixels: vec![ClipboardPixel {
                x: 0,
                y: 0,
                value: PixelValue::Rgba([7, 8, 9, 255]),
            }],
        }],
    };
    core.begin_paste(&payload).unwrap();
    assert_eq!(core.asset_store_usage().asset_count, 1);
    assert_eq!(core.asset_store_usage().referenced_asset_count, 1);
    payload.planes[0].pixels[0].value = PixelValue::Rgba([100, 101, 102, 255]);
    drop(payload);
    core.commit_floating().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([7, 8, 9, 255])
    );
    assert_eq!(core.asset_store_usage().asset_count, 0);
    core.undo().unwrap();
    core.redo().unwrap();
    assert_eq!(
        core.plane_pixel(ActivePlane::Color, 0, 0).unwrap(),
        PixelValue::Rgba([7, 8, 9, 255])
    );
}
