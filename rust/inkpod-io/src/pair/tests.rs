use super::*;
use crate::IoConfig;
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let sequence = PAIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "inkpod-pair-fault-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> (Directory, IoManager, PathBuf, PathBuf) {
    let directory = Directory::new();
    let native = directory.0.join("cell.inkpod");
    let raster = directory.0.join("cell.png");
    std::fs::write(&native, b"old-native").unwrap();
    std::fs::write(&raster, b"old-raster").unwrap();
    let manager = IoManager::new(IoConfig {
        worker_count: 1,
        ..IoConfig::default()
    })
    .unwrap();
    (directory, manager, native, raster)
}

fn prepare(manager: &IoManager, native: &Path, raster: &Path) -> PreparedPair {
    manager
        .prepare_pair(
            native,
            raster,
            &JobContext::new(),
            |file| {
                file.write_all(b"new-native")?;
                Ok(())
            },
            b"new-raster",
            true,
        )
        .unwrap()
}

#[test]
fn known_failure_after_raster_install_rolls_back_before_reporting_failure() {
    let (directory, manager, native, raster) = fixture();
    let prepared = prepare(&manager, &native, &raster);
    assert!(prepared.install_inner(&JobContext::new(), true).is_err());
    assert_eq!(std::fs::read(native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(raster).unwrap(), b"old-raster");
    assert_eq!(std::fs::read_dir(directory.0.clone()).unwrap().count(), 2);
}

#[test]
fn interrupted_prepared_partial_and_complete_pairs_recover_with_identity_and_digest_checks() {
    for installed in 0..=2 {
        let (directory, manager, native, raster) = fixture();
        let mut prepared = prepare(&manager, &native, &raster);
        if installed >= 1 {
            backend::replace(
                &prepared.parent.join(&prepared.record.raster.stage),
                &raster,
                true,
            )
            .unwrap();
        }
        if installed == 2 {
            backend::replace(
                &prepared.parent.join(&prepared.record.native.stage),
                &native,
                true,
            )
            .unwrap();
        }
        // Emulate abrupt process termination between the durable journal and
        // cleanup without exposing a product API that can skip cleanup.
        prepared.discard_on_drop = false;
        drop(prepared);
        let expected = [
            PairRecovery::PreparedDiscarded,
            PairRecovery::RolledBack,
            PairRecovery::Completed,
        ][installed];
        assert_eq!(
            manager.recover_pairs(&native, &JobContext::new()).unwrap(),
            expected
        );
        assert_eq!(
            std::fs::read(&native).unwrap(),
            if installed == 2 {
                b"new-native"
            } else {
                b"old-native"
            }
        );
        assert_eq!(
            std::fs::read(&raster).unwrap(),
            if installed == 2 {
                b"new-raster"
            } else {
                b"old-raster"
            }
        );
        assert_eq!(std::fs::read_dir(directory.0.clone()).unwrap().count(), 2);
    }
}

#[test]
fn recovery_owns_pair_before_waiting_for_file_locks() {
    let (directory, manager, native, raster) = fixture();
    let mut prepared = prepare(&manager, &native, &raster);
    prepared.discard_on_drop = false;
    drop(prepared);
    let context = JobContext::new();
    std::thread::scope(|scope| {
        let mut recovery = None;
        let mut competing_prepare = None;
        let (sent, received) = mpsc::channel();
        let observed =
            manager.with_file_locks(std::slice::from_ref(&native), &JobContext::new(), |_| {
                recovery = Some(scope.spawn(|| manager.recover_pairs(&native, &context)));
                let deadline = Instant::now() + Duration::from_secs(5);
                while context.progress().phase != JobPhase::Reading {
                    if Instant::now() >= deadline {
                        return Err(IoError::ResourceBusy("recovery did not start reading"));
                    }
                    std::thread::yield_now();
                }
                let candidate_manager = manager.clone();
                let candidate_native = native.clone();
                let candidate_raster = raster.clone();
                competing_prepare = Some(scope.spawn(move || {
                    let result = candidate_manager.prepare_pair(
                        &candidate_native,
                        &candidate_raster,
                        &JobContext::new(),
                        |file| {
                            file.write_all(b"competing-native")?;
                            Ok(())
                        },
                        b"competing-raster",
                        true,
                    );
                    sent.send(result).unwrap();
                }));
                // A competing prepare must reject the live recovery owner
                // without waiting for this deliberately held filesystem lock.
                Ok(received.recv_timeout(Duration::from_secs(5)))
            });
        let recovered = recovery.unwrap().join().unwrap();
        if let Some(competing_prepare) = competing_prepare {
            competing_prepare.join().unwrap();
        }
        assert!(matches!(
            observed.unwrap().unwrap(),
            Err(IoError::ResourceBusy(_))
        ));
        assert_eq!(recovered.unwrap(), PairRecovery::PreparedDiscarded);
    });
    assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn paired_stems_follow_the_filesystem_backend_case_policy() {
    let directory = Directory::new();
    let native = directory.0.join("CELL.inkpod");
    let raster = directory.0.join("cell.png");
    std::fs::write(&native, b"old-native").unwrap();
    std::fs::write(&raster, b"old-raster").unwrap();
    let manager = IoManager::new(IoConfig::default()).unwrap();
    let result = manager.prepare_pair(
        &native,
        &raster,
        &JobContext::new(),
        |file| {
            file.write_all(b"new-native")?;
            Ok(())
        },
        b"new-raster",
        true,
    );
    if backend::normalized_leaf("CELL") == backend::normalized_leaf("cell") {
        result.unwrap().install(&JobContext::new()).unwrap();
        assert_eq!(std::fs::read(&native).unwrap(), b"new-native");
        assert_eq!(std::fs::read(&raster).unwrap(), b"new-raster");
    } else {
        assert!(matches!(result, Err(IoError::InvalidInput(_))));
        assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
        assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
    }
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn changed_recovery_evidence_is_retained_without_touching_final_files() {
    let (directory, manager, native, raster) = fixture();
    let mut prepared = prepare(&manager, &native, &raster);
    backend::replace(
        &prepared.parent.join(&prepared.record.raster.stage),
        &raster,
        true,
    )
    .unwrap();
    let backup = prepared.parent.join(&prepared.record.raster.backup);
    std::fs::write(&backup, b"tampered-backup").unwrap();
    let journal = prepared.journal.clone();
    prepared.discard_on_drop = false;
    drop(prepared);
    assert!(manager.recover_pairs(&native, &JobContext::new()).is_err());
    assert_eq!(std::fs::read(native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(raster).unwrap(), b"new-raster");
    assert!(journal.exists() && backup.exists());
    assert!(std::fs::read_dir(directory.0.clone()).unwrap().count() > 2);
}

#[test]
fn malformed_noncurrent_and_escaping_journals_are_rejected() {
    let (_directory, manager, native, raster) = fixture();
    let prepared = prepare(&manager, &native, &raster);
    let bytes = codec::encode(&prepared.record).unwrap();
    assert!(codec::decode(&bytes).is_ok());
    assert!(codec::decode(&bytes[..bytes.len() - 1]).is_err());
    let mut old = bytes.clone();
    old[8..12].copy_from_slice(&0_u32.to_le_bytes());
    let payload = old.len() - 32;
    let digest = *blake3::hash(&old[..payload]).as_bytes();
    old[payload..].copy_from_slice(&digest);
    assert!(codec::decode(&old).is_err());
    let mut unsafe_record = prepared.record.clone();
    unsafe_record.raster.stage = "../outside".to_owned();
    assert!(codec::encode(&unsafe_record).is_err());
}
