use super::*;

fn raster_core(format: PixelFormat, pixels: Vec<u8>, width: u32, uuid: u128) -> Core {
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

fn entry(color: PixelValue, name: &str) -> ColorChartEntry {
    ColorChartEntry {
        color,
        name: name.to_owned(),
    }
}

#[test]
fn comparison_preview_is_bounded_named_and_non_cumulative() {
    let mut core = raster_core(
        PixelFormat::StraightRgba8,
        vec![255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0],
        4,
        0x5301,
    );
    core.replace_color_chart(
        &[
            entry(PixelValue::Rgba([255, 0, 0, 255]), "Primary"),
            entry(PixelValue::Rgba16([1, 2, 3, 4]), "Sixteen"),
        ],
        false,
    )
    .unwrap();
    let before = core.document_info().unwrap();
    let digest_before = core.document_state_digest().unwrap();
    let history_before = core.history_entries().len();

    let limited = core.preview_color_chart_generation(1, 0).unwrap();
    assert!(limited.summary().exceeds_maximum);
    assert_eq!(limited.summary().source_unique_colors, 2);
    assert_eq!(limited.entries().len(), 2);
    assert!(matches!(
        core.apply_color_chart_preview(&limited),
        Err(CoreError::InvalidState(
            "color chart preview exceeds the configured maximum"
        ))
    ));

    let _first = core.preview_color_chart_generation(4, 5).unwrap();
    let second = core.preview_color_chart_generation(4, 0).unwrap();
    let fresh = core.preview_color_chart_generation(4, 0).unwrap();
    assert_eq!(second, fresh);
    assert_eq!(
        second.entries(),
        [
            ColorChartPreviewEntry {
                color: PixelValue::Rgba([0, 0, 255, 255]),
                name: "Color 1".to_owned(),
                frequency: 1,
            },
            ColorChartPreviewEntry {
                color: PixelValue::Rgba([255, 0, 0, 255]),
                name: "Primary".to_owned(),
                frequency: 2,
            },
        ]
    );
    assert_eq!(second.summary().retained_colors, 1);
    assert_eq!(second.summary().added_colors, 1);
    assert_eq!(second.summary().removed_colors, 1);
    assert_eq!(core.document_info().unwrap(), before);
    assert_eq!(core.document_state_digest().unwrap(), digest_before);
    assert_eq!(core.history_entries().len(), history_before);
}

#[test]
fn apply_is_one_document_edit_preserves_cursor_and_round_trips() {
    let mut core = raster_core(
        PixelFormat::StraightRgba8,
        vec![255, 0, 0, 255, 0, 0, 255, 255],
        2,
        0x5302,
    );
    core.replace_palette(&[PixelValue::Rgba16([9, 8, 7, 6])])
        .unwrap();
    core.replace_color_chart(
        &[
            entry(PixelValue::Rgba([255, 0, 0, 255]), "Primary"),
            entry(PixelValue::Rgba([5, 6, 7, 255]), "Removed"),
        ],
        false,
    )
    .unwrap();
    let editor = core.editor_state().unwrap();
    core.update_editor_state(
        editor.revision,
        EditorStateUpdate::SetColorChartCursor(Some(ColorChartCursor { page: 0, index: 0 })),
    )
    .unwrap();
    let preview = core.preview_color_chart_generation(4, 0).unwrap();
    let revision_before = core.document_info().unwrap().document_revision;
    let history_before = core.history_entries().len();
    let palette_before = core.palette().unwrap().to_vec();

    let applied = core.apply_color_chart_preview(&preview).unwrap();
    assert_eq!(applied.accepted_commands(), 1);
    assert_eq!(applied.revision(), revision_before + 1);
    assert_eq!(core.history_entries().len(), history_before + 1);
    assert_eq!(core.palette().unwrap(), palette_before);
    assert_eq!(
        core.color_chart().unwrap().entries(),
        preview.chart_entries()
    );
    assert_eq!(
        core.editor_state().unwrap().state.color_chart_cursor,
        Some(ColorChartCursor { page: 0, index: 1 })
    );

    core.undo().unwrap();
    assert_eq!(core.color_chart().unwrap().entries()[1].name, "Removed");
    core.redo().unwrap();
    assert_eq!(
        core.color_chart().unwrap().entries(),
        preview.chart_entries()
    );

    let path = std::env::temp_dir().join(format!(
        "inkpod-color-chart-preview-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_file(&path);
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    assert_eq!(reopened.color_chart().unwrap(), core.color_chart().unwrap());
    assert_eq!(
        reopened.editor_state().unwrap().state.color_chart_cursor,
        Some(ColorChartCursor { page: 0, index: 1 })
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn cancel_lock_stale_and_no_op_are_atomic() {
    let mut core = raster_core(PixelFormat::StraightRgba8, vec![10, 20, 30, 255], 1, 0x5303);
    let preview = core.preview_color_chart_generation(4, 0).unwrap();
    let before_cancel = core.document_info().unwrap();
    drop(preview);
    assert_eq!(core.document_info().unwrap(), before_cancel);
    assert_eq!(
        core.preview_color_chart_generation_with_cancel(4, 0, |_, _| false),
        Err(CoreError::Cancelled)
    );
    assert_eq!(core.document_info().unwrap(), before_cancel);

    let cross_document_preview = core.preview_color_chart_generation(4, 0).unwrap();
    let mut other = raster_core(PixelFormat::StraightRgba8, vec![10, 20, 30, 255], 1, 0x5304);
    let other_before = other.document_info().unwrap();
    assert_eq!(
        other.apply_color_chart_preview(&cross_document_preview),
        Err(CoreError::InvalidState(
            "color chart preview belongs to another document"
        ))
    );
    assert_eq!(other.document_info().unwrap(), other_before);

    core.replace_color_chart(
        &[entry(PixelValue::Rgba([10, 20, 30, 255]), "Color 1")],
        true,
    )
    .unwrap();
    let locked = core.preview_color_chart_generation(4, 0).unwrap();
    let locked_before = core.document_info().unwrap();
    assert_eq!(
        core.apply_color_chart_preview(&locked),
        Err(CoreError::InvalidState("color chart is locked"))
    );
    assert_eq!(core.document_info().unwrap(), locked_before);

    let unlocked_entries = core.color_chart().unwrap().entries().to_vec();
    core.replace_color_chart(&unlocked_entries, false).unwrap();
    let no_op = core.preview_color_chart_generation(4, 0).unwrap();
    assert_eq!(core.color_chart().unwrap().entries(), no_op.chart_entries());
    assert!(!core.color_chart().unwrap().locked());
    let history_before = core.history_entries().len();
    let revision_before = core.document_info().unwrap().document_revision;
    let outcome = core.apply_color_chart_preview(&no_op).unwrap();
    assert_eq!(outcome.accepted_commands(), 1);
    assert_eq!(outcome.revision(), revision_before);
    assert_eq!(core.history_entries().len(), history_before);

    let stale = core.preview_color_chart_generation(4, 0).unwrap();
    core.set_main_line_color(PixelValue::Rgba([1, 2, 3, 255]))
        .unwrap();
    let stale_before = core.document_info().unwrap();
    assert_eq!(
        core.apply_color_chart_preview(&stale),
        Err(CoreError::InvalidState(
            "color chart preview revision is stale"
        ))
    );
    assert_eq!(core.document_info().unwrap(), stale_before);
}

#[test]
fn rgba16_alpha_and_gradient_source_have_deterministic_rgba8_golden() {
    let channels = [
        [0x1212_u16, 0x5656, 0x9a9a, 0x8080],
        [0x1212_u16, 0x5656, 0x9a9a, 0x8080],
        [0xffff_u16, 0xffff, 0xffff, 0],
    ];
    let pixels = channels
        .into_iter()
        .flatten()
        .flat_map(u16::to_le_bytes)
        .collect();
    let core = raster_core(PixelFormat::StraightRgba16, pixels, 3, 0x5304);
    let preview = core.preview_color_chart_generation(8, 0).unwrap();
    assert_eq!(
        preview.entries(),
        [ColorChartPreviewEntry {
            color: PixelValue::Rgba([18, 86, 154, 128]),
            name: "Color 1".to_owned(),
            frequency: 2,
        }]
    );
}
