use inkpod_io::{
    IoConfig, IoError, IoManager, JobContext, RECOVERY_METADATA_VERSION, RecoveryIdentity,
    RecoveryIdentityKind, RecoveryMetadata, decode_recovery_metadata, encode_recovery_metadata,
    recovery_metadata_path,
};
use std::fs::{self, File, FileTimes};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let sequence = DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "inkpod-io-recovery-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manager() -> IoManager {
    IoManager::new(IoConfig {
        max_images: 2,
        max_file_bytes: 4096,
        max_encoded_bytes: 8192,
        max_decoded_bytes: 8192,
        worker_count: 2,
        queue_capacity: 8,
    })
    .unwrap()
}

fn metadata() -> RecoveryMetadata {
    RecoveryMetadata {
        session_id: 7,
        generation: 19,
        document_uuid: 0x102_03040506,
        original_identity: RecoveryIdentity {
            kind: RecoveryIdentityKind::Untitled,
            uuid: 0x102_03040506,
            ..RecoveryIdentity::default()
        },
        original_path: String::new(),
        source_path: "資料/原画0001.tif".into(),
        written_time_100ns: 116_444_736_000_000_001,
    }
}

fn checksum(bytes: &mut [u8]) {
    let length = bytes.len() - 32;
    let hash = blake3::hash(&bytes[..length]);
    bytes[length..].copy_from_slice(hash.as_bytes());
}

fn modified(path: &Path, seconds: u64) {
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
        .unwrap();
}

#[test]
fn metadata_round_trips_identity_variants_and_rejects_noncurrent_or_malformed_records() {
    let mut record = metadata();
    for identity in [
        RecoveryIdentity::default(),
        RecoveryIdentity {
            kind: RecoveryIdentityKind::PhysicalFile,
            volume_serial: 4,
            file_id: [9; 16],
            ..RecoveryIdentity::default()
        },
        RecoveryIdentity {
            kind: RecoveryIdentityKind::NormalizedPath,
            normalized_path: "C:/work/未保存.inkpod".into(),
            ..RecoveryIdentity::default()
        },
        record.original_identity.clone(),
    ] {
        record.original_identity = identity;
        let bytes = encode_recovery_metadata(&record).unwrap();
        assert_eq!(decode_recovery_metadata(&bytes).unwrap(), record);
        for cut in [0, 32, 147, bytes.len() - 1] {
            assert!(decode_recovery_metadata(&bytes[..cut]).is_err());
        }
        let mut corrupt = bytes.clone();
        corrupt[32] ^= 1;
        assert!(decode_recovery_metadata(&corrupt).is_err());
        let mut old = bytes.clone();
        old[8..12].copy_from_slice(&(RECOVERY_METADATA_VERSION - 1).to_le_bytes());
        checksum(&mut old);
        assert!(decode_recovery_metadata(&old).is_err());
        let mut reserved = bytes.clone();
        reserved[60..64].copy_from_slice(&1_u32.to_le_bytes());
        checksum(&mut reserved);
        assert!(decode_recovery_metadata(&reserved).is_err());
        let mut overflowing = bytes.clone();
        overflowing[104..108].copy_from_slice(&u32::MAX.to_le_bytes());
        checksum(&mut overflowing);
        assert!(decode_recovery_metadata(&overflowing).is_err());
    }
    record.source_path = "invalid\0path".into();
    assert!(encode_recovery_metadata(&record).is_err());
    record.source_path = "x".repeat(32_768);
    assert!(encode_recovery_metadata(&record).is_err());
    record.source_path.clear();
    record.original_identity = RecoveryIdentity {
        kind: RecoveryIdentityKind::PhysicalFile,
        ..RecoveryIdentity::default()
    };
    assert!(encode_recovery_metadata(&record).is_err());
}

#[test]
fn rust_creates_recovery_parents_writes_metadata_time_and_keeps_cache_empty() {
    let directory = Directory::new();
    let manager = manager();
    let context = JobContext::new();
    let mut record = metadata();
    let path = manager
        .recovery_path(
            &directory.path("missing/nested"),
            record.document_uuid,
            Some(9),
            &context,
        )
        .unwrap();
    assert_eq!(
        path.file_name().unwrap(),
        "00000000000000000000010203040506-sequence-0000000000000009.inkpod"
    );
    record.written_time_100ns = 0;
    manager
        .write_recovery(&path, &record, &context, |file| {
            file.write_all(b"native-recovery-fixture")?;
            Ok(())
        })
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"native-recovery-fixture");
    let loaded = manager.read_recovery_metadata(&path, &context).unwrap();
    assert!(loaded.written_time_100ns > 116_444_736_000_000_000);
    record.written_time_100ns = loaded.written_time_100ns;
    assert_eq!(loaded, record);
    assert_eq!(manager.cache_stats().images, 0);
    assert_eq!(manager.cache_stats().encoded_bytes, 0);
    assert_eq!(manager.cache_stats().physical_reads, 1);
    assert_eq!(context.progress().read_completed, 1);
    assert!(
        manager
            .recovery_path(&directory.path("invalid"), 0, None, &context)
            .is_err()
    );
    assert!(!directory.path("invalid").exists());
    assert!(
        manager
            .recovery_path(&directory.0, 1, Some(0), &context)
            .is_err()
    );
    assert!(recovery_metadata_path(&directory.path("not-native.png")).is_err());
}

#[test]
fn candidates_preserve_native_files_when_sidecars_are_missing_invalid_or_obsolete() {
    let directory = Directory::new();
    let manager = manager();
    let context = JobContext::new();
    for (name, seconds) in [
        ("valid.inkpod", 3),
        ("missing.inkpod", 1),
        ("bad.inkpod", 2),
        ("old.INKPOD", 2),
    ] {
        fs::write(directory.path(name), b"native-fixture").unwrap();
        modified(&directory.path(name), seconds);
    }
    fs::write(directory.path("ignored.png"), b"png").unwrap();
    fs::create_dir(directory.path("ignored.inkpod")).unwrap();
    manager
        .write_recovery_metadata(&directory.path("valid.inkpod"), &metadata(), &context)
        .unwrap();
    fs::write(
        recovery_metadata_path(&directory.path("bad.inkpod")).unwrap(),
        b"bad metadata",
    )
    .unwrap();
    let mut old = encode_recovery_metadata(&metadata()).unwrap();
    old[8..12].copy_from_slice(&1_u32.to_le_bytes());
    checksum(&mut old);
    fs::write(
        recovery_metadata_path(&directory.path("old.INKPOD")).unwrap(),
        old,
    )
    .unwrap();
    let candidates = manager
        .list_recovery_candidates(&directory.0, &context)
        .unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate
                .recovery_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap())
            .collect::<Vec<_>>(),
        ["valid.inkpod", "bad.inkpod", "old.INKPOD", "missing.inkpod"]
    );
    assert!(candidates[0].metadata.is_some());
    assert!(candidates[0].metadata_error.is_none());
    for candidate in &candidates[1..] {
        assert!(candidate.metadata.is_none());
        assert!(candidate.metadata_error.is_some());
        assert!(candidate.recovery_path.exists());
    }
    assert!(
        manager
            .list_recovery_candidates(&directory.path("absent/nested"), &context)
            .unwrap()
            .is_empty()
    );
    assert_eq!(manager.cache_stats().images, 0);
}

#[test]
fn failure_and_cancellation_preserve_previous_recovery_and_discard_is_idempotent() {
    let directory = Directory::new();
    let manager = manager();
    let context = JobContext::new();
    let path = directory.path("recovery.inkpod");
    manager
        .write_recovery(&path, &metadata(), &context, |file| {
            file.write_all(b"old")?;
            Ok(())
        })
        .unwrap();
    let old_metadata = fs::read(recovery_metadata_path(&path).unwrap()).unwrap();
    let mut invalid = metadata();
    invalid.session_id = 0;
    assert!(
        manager
            .write_recovery(&path, &invalid, &context, |_| panic!(
                "invalid metadata must be checked before native writer"
            ))
            .is_err()
    );
    assert!(
        manager
            .write_recovery(&path, &metadata(), &context, |file| {
                file.write_all(b"partial")?;
                Err(IoError::InvalidInput("injected encoder failure"))
            })
            .is_err()
    );
    let cancelled = JobContext::new();
    cancelled.cancel();
    assert!(matches!(
        manager.write_recovery(&path, &metadata(), &cancelled, |_| panic!(
            "cancelled writer ran"
        )),
        Err(IoError::Cancelled)
    ));
    assert!(matches!(
        manager.discard_recovery(&path, &cancelled),
        Err(IoError::Cancelled)
    ));
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert_eq!(
        fs::read(recovery_metadata_path(&path).unwrap()).unwrap(),
        old_metadata
    );
    manager.discard_recovery(&path, &context).unwrap();
    manager.discard_recovery(&path, &context).unwrap();
    assert!(!path.exists());
    assert!(!recovery_metadata_path(&path).unwrap().exists());
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}

#[test]
fn recovery_probe_distinguishes_absence_from_older_or_newer_artifacts() {
    let directory = Directory::new();
    let manager = manager();
    let context = JobContext::new();
    let normal = directory.path("normal.inkpod");
    let recovery = directory.path("recovery.inkpod");
    assert!(
        !manager
            .recovery_is_newer(&normal, &recovery, &context)
            .unwrap()
    );
    fs::write(&recovery, b"recovery").unwrap();
    assert!(
        manager
            .recovery_is_newer(&normal, &recovery, &context)
            .unwrap()
    );
    fs::write(&normal, b"normal").unwrap();
    modified(&normal, 2);
    modified(&recovery, 1);
    assert!(
        !manager
            .recovery_is_newer(&normal, &recovery, &context)
            .unwrap()
    );
    modified(&recovery, 2);
    assert!(
        !manager
            .recovery_is_newer(&normal, &recovery, &context)
            .unwrap()
    );
    modified(&recovery, 3);
    assert!(
        manager
            .recovery_is_newer(&normal, &recovery, &context)
            .unwrap()
    );
}

#[test]
fn oversized_sidecars_and_nonregular_artifacts_never_remove_the_native_candidate() {
    let directory = Directory::new();
    let manager = manager();
    let context = JobContext::new();
    let path = directory.path("large.inkpod");
    fs::write(&path, b"native").unwrap();
    let sidecar = recovery_metadata_path(&path).unwrap();
    File::create(&sidecar)
        .unwrap()
        .set_len(512 * 1024 + 1)
        .unwrap();
    assert!(matches!(
        manager.read_recovery_metadata(&path, &context),
        Err(IoError::LimitExceeded(_))
    ));
    let candidates = manager
        .list_recovery_candidates(&directory.0, &context)
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].metadata_error.is_some());
    fs::remove_file(&sidecar).unwrap();
    fs::create_dir(&sidecar).unwrap();
    assert!(manager.discard_recovery(&path, &context).is_err());
    assert_eq!(fs::read(path).unwrap(), b"native");
}

#[cfg(unix)]
#[test]
fn sidecar_symlink_is_never_followed_for_write_or_discard() {
    let directory = Directory::new();
    let path = directory.path("recovery.inkpod");
    let unrelated = directory.path("unrelated.txt");
    fs::write(&path, b"native").unwrap();
    fs::write(&unrelated, b"unrelated").unwrap();
    std::os::unix::fs::symlink(&unrelated, recovery_metadata_path(&path).unwrap()).unwrap();
    let manager = manager();
    let context = JobContext::new();
    assert!(
        manager
            .write_recovery_metadata(&path, &metadata(), &context)
            .is_err()
    );
    assert!(manager.discard_recovery(&path, &context).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"native");
    assert_eq!(fs::read(&unrelated).unwrap(), b"unrelated");
}
