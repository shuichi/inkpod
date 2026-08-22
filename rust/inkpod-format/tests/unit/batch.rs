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
            version: BATCH_GRAPH_VERSION,
            kind: 2,
            flags: 1,
            target: FileBatchTarget {
                layer_id: 10,
                plane_id: 12,
                layer_kind: 1,
                plane_kind: 2,
                missing_policy: 1,
            },
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
fn batch_graph_rejects_unknown_container_version_and_cancel_cleans_temp() {
    let graph = fixture();
    let mut encoded = encode_batch_graph(&graph).unwrap();
    encoded[8..12].copy_from_slice(&(BATCH_GRAPH_VERSION + 1).to_le_bytes());
    assert!(matches!(
        decode_batch_graph(&encoded),
        Err(FormatError::Invalid("batch graph version is unsupported"))
    ));
    let mut old = encode_batch_graph(&graph).unwrap();
    old[8..12].copy_from_slice(&2_u32.to_le_bytes());
    assert!(matches!(
        decode_batch_graph(&old),
        Err(FormatError::Invalid("batch graph version is unsupported"))
    ));

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
