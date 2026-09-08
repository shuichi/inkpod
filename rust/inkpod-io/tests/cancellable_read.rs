use inkpod_io::{IoConfig, IoError, IoManager, JobContext};
use std::fs;

#[test]
fn callback_cancellation_during_bounded_read_releases_budget_and_keeps_source() {
    let path = std::env::temp_dir().join(format!(
        "inkpod-cancellable-read-{}.bin",
        std::process::id()
    ));
    let bytes = vec![53_u8; 3 * 64 * 1024];
    fs::write(&path, &bytes).unwrap();
    let manager = IoManager::new(IoConfig::default()).unwrap();
    let context = JobContext::new();
    let result = manager.read_bytes_cancellable(&path, bytes.len() as u64, &context, &mut || {
        context.progress().completed_bytes >= 64 * 1024
    });
    assert!(matches!(result, Err(IoError::Cancelled)));
    let stats = manager.cache_stats();
    assert_eq!(
        (stats.images, stats.encoded_bytes, stats.physical_reads),
        (0, 0, 0)
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let loaded = manager
        .read_bytes_cancellable(&path, bytes.len() as u64, &JobContext::new(), &mut || false)
        .unwrap();
    assert_eq!(loaded.bytes(), bytes);
    assert!(matches!(
        manager.read_bytes_cancellable(&path, bytes.len() as u64, &JobContext::new(), &mut || true),
        Err(IoError::Cancelled)
    ));
    assert_eq!(manager.cache_stats().physical_reads, 1);
    fs::remove_file(path).unwrap();
}

#[test]
fn callback_cancellation_reaches_waiting_shared_file_lock() {
    let path = std::env::temp_dir().join(format!(
        "inkpod-cancellable-lock-{}.bin",
        std::process::id()
    ));
    fs::write(&path, b"source").unwrap();
    let manager = IoManager::new(IoConfig::default()).unwrap();
    manager
        .with_file_locks(std::slice::from_ref(&path), &JobContext::new(), |_| {
            let mut polls = 0;
            assert!(matches!(
                manager.read_bytes_cancellable(&path, 6, &JobContext::new(), &mut || {
                    polls += 1;
                    polls == 3
                }),
                Err(IoError::Cancelled)
            ));
            Ok(())
        })
        .unwrap();
    assert_eq!(manager.cache_stats().physical_reads, 0);
    assert_eq!(fs::read(&path).unwrap(), b"source");
    fs::remove_file(path).unwrap();
}

#[test]
fn callback_read_keeps_change_detection_and_byte_caps() {
    let path = std::env::temp_dir().join(format!(
        "inkpod-cancellable-stale-{}.bin",
        std::process::id()
    ));
    let bytes = vec![7_u8; 3 * 64 * 1024];
    fs::write(&path, &bytes).unwrap();
    let manager = IoManager::new(IoConfig::default()).unwrap();
    assert!(matches!(
        manager.read_bytes_cancellable(
            &path,
            bytes.len() as u64 - 1,
            &JobContext::new(),
            &mut || false
        ),
        Err(IoError::LimitExceeded(_))
    ));
    let context = JobContext::new();
    let mut changed = false;
    let result = manager.read_bytes_cancellable(&path, bytes.len() as u64, &context, &mut || {
        if !changed && context.progress().completed_bytes >= 64 * 1024 {
            fs::write(&path, vec![9_u8; bytes.len()]).unwrap();
            changed = true;
        }
        false
    });
    assert!(changed);
    assert!(matches!(result, Err(IoError::ChangedDuringRead)));
    let stats = manager.cache_stats();
    assert_eq!(
        (stats.images, stats.encoded_bytes, stats.physical_reads),
        (0, 0, 0)
    );
    fs::remove_file(path).unwrap();
}
