use super::*;

fn diagnostic_native_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "inkpod-{label}-{}-{}.inkpod",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn vector_line(start: (f32, f32), end: (f32, f32)) -> VectorPathInput {
    let third_x = (end.0 - start.0) / 3.0;
    let third_y = (end.1 - start.1) / 3.0;
    VectorPathInput {
        segments: vec![VectorCubicSegment {
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
            width_start: 2.0,
            width_end: 2.0,
        }],
        color: PixelValue::Rgba([20, 30, 40, 255]),
        closed: false,
    }
}

fn closed_rectangle(x0: f32, y0: f32, x1: f32, y1: f32) -> VectorPathInput {
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
    let segments = corners
        .windows(2)
        .flat_map(|pair| vector_line(pair[0], pair[1]).segments)
        .collect();
    VectorPathInput {
        segments,
        color: PixelValue::Rgba([20, 30, 40, 255]),
        closed: true,
    }
}

fn diagnostic_core() -> (Core, u64) {
    let mut core = Core::new();
    core.new_cell(32, 32, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .unwrap();
    let (_, layer_id) = core
        .create_layer(LayerKind::VectorColoring, "Diagnostics")
        .unwrap();
    let (main_plane_id, _, _) = core.vector_layer_planes(layer_id).unwrap();
    (core, main_plane_id)
}

#[test]
fn pm_gap_018_vector_diagnostics_are_view_local_and_non_mutating() {
    let (mut core, main_plane_id) = diagnostic_core();
    core.vector_add_path(main_plane_id, vector_line((2.0, 4.0), (28.0, 4.0)))
        .unwrap();

    let before = core.document_info().unwrap();
    let history_before = core.history_entries();
    let digest_before = core.build_snapshot().canonical_composite_digest().unwrap();
    let primary_before = core.view_state();
    assert!(primary_before.vector_antialias());
    assert_eq!(
        primary_before.vector_centerline_mode(),
        VectorCenterlineMode::Hidden
    );
    assert!(!primary_before.vector_endpoints_visible());

    let secondary_id = core.create_view().unwrap();
    core.apply_view_for(secondary_id, ViewCommand::SetVectorAntialias(false))
        .unwrap();
    core.apply_view_for(
        secondary_id,
        ViewCommand::SetVectorCenterlineMode(VectorCenterlineMode::Only),
    )
    .unwrap();
    let secondary = core
        .apply_view_for(secondary_id, ViewCommand::SetVectorEndpointsVisible(true))
        .unwrap();
    assert!(!secondary.vector_antialias());
    assert_eq!(
        secondary.vector_centerline_mode(),
        VectorCenterlineMode::Only
    );
    assert!(secondary.vector_endpoints_visible());
    assert!(secondary.revision() > primary_before.revision());

    let no_op = core
        .apply_view_for(secondary_id, ViewCommand::SetVectorEndpointsVisible(true))
        .unwrap();
    assert_eq!(no_op.revision(), secondary.revision());
    assert_eq!(core.view_state(), primary_before);
    assert!(
        core.apply_view_for(u64::MAX, ViewCommand::SetVectorAntialias(false))
            .is_err()
    );

    let primary_snapshot = core.build_snapshot();
    let secondary_snapshot = core.build_snapshot_for(secondary_id).unwrap();
    assert!(primary_snapshot.vector_endpoints().is_empty());
    assert_eq!(secondary_snapshot.vector_endpoints().len(), 2);
    assert_eq!(
        secondary_snapshot.view().vector_centerline_mode(),
        VectorCenterlineMode::Only
    );
    assert_eq!(
        secondary_snapshot.canonical_composite_digest().unwrap(),
        digest_before
    );

    let after = core.document_info().unwrap();
    assert_eq!(after.document_revision, before.document_revision);
    assert_eq!(after.view_revision, before.view_revision);
    assert_eq!(after.dirty, before.dirty);
    assert_eq!(core.history_entries(), history_before);
}

#[test]
fn pm_gap_018_endpoint_records_use_exact_topology_and_stable_identity() {
    let (mut core, main_plane_id) = diagnostic_core();
    let (_, first) = core
        .vector_add_path(main_plane_id, vector_line((1.0, 2.0), (8.0, 2.0)))
        .unwrap();
    let (_, second) = core
        .vector_add_path(main_plane_id, vector_line((8.0, 2.0), (16.0, 2.0)))
        .unwrap();
    let (_, near) = core
        .vector_add_path(main_plane_id, vector_line((16.000_5, 2.0), (24.0, 2.0)))
        .unwrap();
    core.vector_add_path(main_plane_id, closed_rectangle(4.0, 8.0, 12.0, 16.0))
        .unwrap();
    core.apply_view(ViewCommand::SetVectorEndpointsVisible(true))
        .unwrap();

    let endpoints = core.build_snapshot().vector_endpoints();
    assert_eq!(
        endpoints
            .iter()
            .map(|endpoint| (endpoint.path_id, endpoint.endpoint))
            .collect::<Vec<_>>(),
        vec![
            (first, VectorEndpoint::Start),
            (first, VectorEndpoint::End),
            (second, VectorEndpoint::Start),
            (second, VectorEndpoint::End),
            (near, VectorEndpoint::Start),
            (near, VectorEndpoint::End),
        ]
    );
    assert_eq!(endpoints[1].point, PointF32 { x: 8.0, y: 2.0 });
    assert_eq!(endpoints[2].point, PointF32 { x: 8.0, y: 2.0 });
    assert_eq!(endpoints[3].point, PointF32 { x: 16.0, y: 2.0 });
    assert_eq!(endpoints[4].point, PointF32 { x: 16.001, y: 2.0 });
    assert!(
        endpoints
            .iter()
            .all(|endpoint| endpoint.plane_id == main_plane_id)
    );

    let history_before = core.history_entries().len();
    let (_, connector) = core.vector_connect(main_plane_id, 0.001).unwrap();
    let connector = connector.expect("coincident endpoints connect explicitly");
    assert_eq!(core.history_entries().len(), history_before + 1);
    assert_eq!(
        core.build_snapshot()
            .vector_endpoints()
            .iter()
            .map(|endpoint| (endpoint.path_id, endpoint.endpoint))
            .collect::<Vec<_>>(),
        vec![
            (first, VectorEndpoint::Start),
            (second, VectorEndpoint::End),
            (near, VectorEndpoint::Start),
            (near, VectorEndpoint::End),
        ]
    );

    core.undo().unwrap();
    assert_eq!(core.build_snapshot().vector_endpoints().len(), 6);
    core.redo().unwrap();
    assert!(
        core.build_snapshot()
            .vector_endpoints()
            .iter()
            .all(|endpoint| endpoint.path_id != connector)
    );

    let path = diagnostic_native_path("pm-gap-018-vector-topology");
    core.save(&path).unwrap();
    let mut reopened = Core::new();
    reopened.open(&path).unwrap();
    reopened
        .apply_view(ViewCommand::SetVectorEndpointsVisible(true))
        .unwrap();
    assert_eq!(
        reopened.build_snapshot().vector_endpoints(),
        core.build_snapshot().vector_endpoints()
    );
    let _ = std::fs::remove_file(path);
}
