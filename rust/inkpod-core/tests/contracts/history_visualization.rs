use super::*;

const HISTORY_VISUALIZATION_UUID: u128 = 0x0049_4e4b_504f_442d_4856_4953_5541_4c01;

fn visualization_core() -> Core {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        32,
        24,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        HISTORY_VISUALIZATION_UUID,
    )
    .unwrap();
    core
}

fn set_main_line(core: &mut Core, value: u8) {
    let expected_revision = core.document_info().unwrap().document_revision;
    core.execute_primitive(PrimitiveRequest::SetMainLineColor {
        expected_revision,
        color: PixelValue::Rgba([value, value + 1, value + 2, 255]),
    })
    .unwrap();
}

fn replace_palette(core: &mut Core, value: u16) {
    let expected_revision = core.document_info().unwrap().document_revision;
    core.execute_primitive(PrimitiveRequest::ReplacePalette {
        expected_revision,
        colors: vec![PixelValue::Rgba16([value, value + 1, value + 2, u16::MAX])],
    })
    .unwrap();
}

#[test]
fn hist_002_rows_are_commit_ordered_and_include_inactive_branches() {
    let mut core = visualization_core();
    set_main_line(&mut core, 10);
    replace_palette(&mut core, 20);
    core.undo().unwrap();
    set_main_line(&mut core, 30);

    let info_before = core.document_info().unwrap();
    let state_before = core.journal_state().unwrap();
    let journal_before = core.journal_entries().to_vec();
    let rows = core.history_visualization_rows().unwrap();

    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row.journal_event_id.get())
            .collect::<Vec<_>>(),
        vec![1, 2, 5]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.primitive_name.as_str())
            .collect::<Vec<_>>(),
        vec!["SetMainLineColor", "ReplacePalette", "SetMainLineColor"]
    );
    assert!(rows[0].arguments.contains("color=Rgba([10, 11, 12, 255])"));
    assert!(rows[1].arguments.contains("colors_count=1"));
    assert!(rows[1].arguments.contains("Rgba16([20, 21, 22, 65535])"));
    assert!(rows[2].arguments.contains("color=Rgba([30, 31, 32, 255])"));
    assert_eq!(rows[0].branch_id.get(), 1);
    assert_eq!(rows[1].branch_id.get(), 1);
    assert_eq!(rows[2].branch_id.get(), 2);
    for row in &rows {
        assert_eq!((row.thumbnail.width, row.thumbnail.height), (32, 24));
        assert_eq!(row.thumbnail.rgba8.len(), 32 * 24 * 4);
    }

    assert_eq!(core.document_info().unwrap(), info_before);
    assert_eq!(core.journal_state().unwrap(), state_before);
    assert_eq!(core.journal_entries(), journal_before);
}

#[test]
fn hist_002_empty_document_history_returns_an_empty_snapshot() {
    let core = visualization_core();
    assert!(core.history_visualization_rows().unwrap().is_empty());
    assert!(matches!(
        Core::new().history_visualization_rows(),
        Err(CoreError::NoDocument)
    ));
}

#[test]
fn hist_002_cancellation_is_an_observable_stable_noop() {
    let mut core = visualization_core();
    set_main_line(&mut core, 10);
    replace_palette(&mut core, 20);
    let info_before = core.document_info().unwrap();
    let state_before = core.journal_state().unwrap();
    let journal_before = core.journal_entries().to_vec();
    let mut progress_calls = 0_u64;

    let result = core.history_visualization_rows_with_progress(|completed, total| {
        progress_calls += 1;
        assert!(completed <= total);
        false
    });

    assert!(matches!(result, Err(CoreError::Cancelled)));
    assert_eq!(progress_calls, 1);
    assert_eq!(core.document_info().unwrap(), info_before);
    assert_eq!(core.journal_state().unwrap(), state_before);
    assert_eq!(core.journal_entries(), journal_before);
}

#[test]
fn hist_002_builder_processes_bounded_event_chunks_and_matches_the_snapshot_query() {
    let mut core = visualization_core();
    set_main_line(&mut core, 10);
    replace_palette(&mut core, 20);
    core.undo().unwrap();
    set_main_line(&mut core, 30);
    let expected = core.history_visualization_rows().unwrap();
    let mut builder = core.begin_history_visualization().unwrap();
    let mut previous = builder.progress();

    while !previous.is_complete() {
        let progress = builder.step(1).unwrap();
        assert!(progress.completed_events() >= previous.completed_events());
        assert!(progress.completed_events() <= previous.completed_events() + 1);
        assert!(progress.completed_rows() >= previous.completed_rows());
        assert!(progress.completed_rows() <= progress.total_rows());
        previous = progress;
    }

    assert_eq!(builder.finish().unwrap(), expected);
}

#[test]
fn hist_002_builder_owns_the_point_in_time_source_state() {
    let mut core = visualization_core();
    set_main_line(&mut core, 10);
    let mut builder = core.begin_history_visualization().unwrap();

    set_main_line(&mut core, 40);
    while !builder.progress().is_complete() {
        builder.step(1).unwrap();
    }
    let rows = builder.finish().unwrap();

    assert_eq!(rows.len(), 1);
    assert!(rows[0].arguments.contains("Rgba([10, 11, 12, 255])"));
    assert_eq!(core.history_visualization_rows().unwrap().len(), 2);
}

#[test]
fn hist_002_direct_bounded_thumbnail_matches_the_existing_visible_composite() {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        128,
        96,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        HISTORY_VISUALIZATION_UUID,
    )
    .unwrap();
    core.set_active_plane(ActivePlane::Color).unwrap();
    core.apply_stroke(&color_stroke(
        PaintTool::Pencil,
        3.0,
        StrokeSample {
            x: 63.0,
            y: 47.0,
            pressure: 1.0,
        },
    ))
    .unwrap();

    let expected = core.document_thumbnail().unwrap();
    let rows = core.history_visualization_rows().unwrap();

    assert_eq!(rows.last().unwrap().thumbnail, expected);
    assert_eq!((expected.width, expected.height), (64, 48));
}
