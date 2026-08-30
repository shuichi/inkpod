use super::*;

fn fixture() -> FileBatchGraph {
    FileBatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "daily-color".to_owned(),
        inputs: vec![FileBatchInput {
            kind: 1,
            path: "c001.inkpod".to_owned(),
            first_cell: 1,
            last_cell: 12,
        }],
        operations: vec![FileBatchOperation {
            version: BATCH_OPERATION_VERSION,
            kind: 2,
            flags: 1,
            targets: vec![
                FileBatchTarget {
                    layer_id: 10,
                    plane_id: 12,
                    plane_kind: 2,
                    missing_policy: 1,
                },
                FileBatchTarget {
                    layer_id: 20,
                    plane_id: 22,
                    plane_kind: 2,
                    missing_policy: 2,
                },
            ],
            payload: vec![1, 2, 3, 4],
        }],
        output: FileBatchOutput {
            destination: 3,
            folder: "out".to_owned(),
            format: 1,
            naming_template: "{stem}_{index:3}".to_owned(),
            failure_policy: 1,
            wait_milliseconds: 25,
            preview_before_save: true,
        },
    }
}

fn first_operation_offset(graph: &FileBatchGraph) -> usize {
    const HEADER_BYTES: usize = 28;
    let inputs_bytes = graph
        .inputs
        .iter()
        .map(|input| 16 + input.path.len())
        .sum::<usize>();
    HEADER_BYTES + 4 + graph.name.len() + 4 + inputs_bytes + 4
}

fn replace_u32_and_checksum(encoded: &mut [u8], offset: usize, value: u32) {
    encoded[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    replace_checksum(encoded);
}

fn replace_first_target_and_checksum(
    encoded: &mut [u8],
    graph: &FileBatchGraph,
    target: FileBatchTarget,
) {
    const OPERATION_HEADER_BYTES: usize = 20;
    let offset = first_operation_offset(graph) + OPERATION_HEADER_BYTES;
    encoded[offset..offset + 8].copy_from_slice(&target.layer_id.to_le_bytes());
    encoded[offset + 8..offset + 16].copy_from_slice(&target.plane_id.to_le_bytes());
    encoded[offset + 16..offset + 20].copy_from_slice(&target.plane_kind.to_le_bytes());
    encoded[offset + 20..offset + 24].copy_from_slice(&target.missing_policy.to_le_bytes());
    replace_checksum(encoded);
}

fn replace_checksum(encoded: &mut [u8]) {
    const BODY_OFFSET: usize = 28;
    const CHECKSUM_OFFSET: usize = 20;
    let body_checksum = checksum(&encoded[BODY_OFFSET..]);
    encoded[CHECKSUM_OFFSET..BODY_OFFSET].copy_from_slice(&body_checksum.to_le_bytes());
}

fn assert_invalid(result: Result<FileBatchGraph, FormatError>, expected: &'static str) {
    assert!(
        matches!(result, Err(FormatError::Invalid(message)) if message == expected),
        "expected invalid format error: {expected}"
    );
}

#[test]
fn batch_graph_round_trip_and_checksum_validation() {
    let graph = fixture();
    let encoded = encode_batch_graph(&graph).unwrap();
    assert_eq!(decode_batch_graph(&encoded).unwrap(), graph);
    let mut corrupt = encoded;
    *corrupt.last_mut().unwrap() ^= 0x80;
    assert!(matches!(
        decode_batch_graph(&corrupt),
        Err(FormatError::Invalid("batch graph checksum does not match"))
    ));
}

#[test]
fn batch_operation_version_is_exact_current_on_encode_and_decode() {
    for version in [0, 3, 5] {
        let mut invalid = fixture();
        invalid.operations[0].version = version;
        assert!(matches!(
            encode_batch_graph(&invalid),
            Err(FormatError::Invalid(
                "batch operation version is unsupported"
            ))
        ));

        let graph = fixture();
        let mut encoded = encode_batch_graph(&graph).unwrap();
        replace_u32_and_checksum(&mut encoded, first_operation_offset(&graph), version);
        assert_invalid(
            decode_batch_graph(&encoded),
            "batch operation version is unsupported",
        );
    }
}

#[test]
fn batch_targets_are_closed_to_valid_plane_selectors_and_missing_policies() {
    let cases = [
        (
            FileBatchTarget {
                layer_id: 0,
                plane_id: 0,
                plane_kind: 0,
                missing_policy: 1,
            },
            "batch target plane selector is empty",
        ),
        (
            FileBatchTarget {
                layer_id: 10,
                plane_id: 0,
                plane_kind: 0,
                missing_policy: 1,
            },
            "batch target plane selector is empty",
        ),
        (
            FileBatchTarget {
                layer_id: 10,
                plane_id: 12,
                plane_kind: 1,
                missing_policy: 1,
            },
            "batch target plane kind must be Color or Raster",
        ),
        (
            FileBatchTarget {
                layer_id: 10,
                plane_id: 12,
                plane_kind: 4,
                missing_policy: 1,
            },
            "batch target plane kind must be Color or Raster",
        ),
        (
            FileBatchTarget {
                layer_id: 0,
                plane_id: 0,
                plane_kind: 2,
                missing_policy: 0,
            },
            "batch missing-target policy is unknown",
        ),
        (
            FileBatchTarget {
                layer_id: 0,
                plane_id: 0,
                plane_kind: 3,
                missing_policy: 3,
            },
            "batch missing-target policy is unknown",
        ),
    ];

    for (target, expected) in cases {
        let mut invalid = fixture();
        invalid.operations[0].targets[0] = target;
        assert!(
            matches!(encode_batch_graph(&invalid), Err(FormatError::Invalid(message)) if message == expected),
            "encode must reject: {expected}"
        );

        let graph = fixture();
        let mut encoded = encode_batch_graph(&graph).unwrap();
        replace_first_target_and_checksum(&mut encoded, &graph, target);
        assert_invalid(decode_batch_graph(&encoded), expected);
    }
}

#[test]
fn batch_target_without_plane_kind_requires_and_accepts_a_fixed_plane_id() {
    let mut graph = fixture();
    graph.operations[0].targets = vec![FileBatchTarget {
        layer_id: 10,
        plane_id: 12,
        plane_kind: 0,
        missing_policy: 2,
    }];
    let encoded = encode_batch_graph(&graph).unwrap();
    assert_eq!(decode_batch_graph(&encoded).unwrap(), graph);
}

#[test]
fn batch_graph_rejects_unknown_container_version_and_cancel_cleans_temp() {
    let graph = fixture();
    let mut encoded = encode_batch_graph(&graph).unwrap();
    encoded[8..12].copy_from_slice(&(BATCH_GRAPH_VERSION + 1).to_le_bytes());
    assert!(matches!(
        decode_batch_graph(&encoded),
        Err(FormatError::Invalid("batch graph version is unsupported"))
    ));
    for old_version in [1_u32, 2, 3, 4] {
        let mut old = encode_batch_graph(&graph).unwrap();
        old[8..12].copy_from_slice(&old_version.to_le_bytes());
        assert!(matches!(
            decode_batch_graph(&old),
            Err(FormatError::Invalid("batch graph version is unsupported"))
        ));
    }

    let directory = std::env::temp_dir().join(format!(
        "inkpod-batch-format-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let destination = directory.join("settings.inkbatch");
    let mut calls = 0_u32;
    let result = save_batch_graph_atomic_with_cancel(&destination, &graph, || {
        calls += 1;
        calls > 1
    });
    assert!(matches!(result, Err(FormatError::Cancelled)));
    assert!(!destination.exists());
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
    fs::remove_dir(&directory).unwrap();
}

#[test]
fn batch_graph_atomic_save_replaces_existing_settings_without_temp_files() {
    let directory = std::env::temp_dir().join(format!(
        "inkpod-batch-replace-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let destination = directory.join("settings.inkbatch");
    let first = fixture();
    save_batch_graph_atomic(&destination, &first).unwrap();
    let mut replacement = first;
    replacement.name = "replacement".to_owned();
    replacement.output.naming_template = "{stem}_replacement".to_owned();
    save_batch_graph_atomic(&destination, &replacement).unwrap();
    assert_eq!(read_batch_graph(&destination).unwrap(), replacement);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    fs::remove_dir_all(directory).unwrap();
}
