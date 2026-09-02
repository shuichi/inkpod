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

#[derive(Clone, Copy, Debug)]
enum OriginalPairKind {
    Committed,
    Planned,
    RepairNeeded,
}

fn prepare_original_pair(
    kind: OriginalPairKind,
) -> (Directory, IoManager, PathBuf, PathBuf, PreparedPair) {
    let directory = Directory::new();
    let native = directory.0.join("cell.inkpod");
    let raster = directory.0.join("cell.png");
    let manager = IoManager::new(IoConfig::default()).unwrap();
    let context = JobContext::new();
    let prepared = match kind {
        OriginalPairKind::Committed => {
            std::fs::write(&native, b"old-native").unwrap();
            std::fs::write(&raster, b"old-raster").unwrap();
            prepare(&manager, &native, &raster)
        }
        OriginalPairKind::Planned => {
            std::fs::write(&raster, b"old-raster").unwrap();
            let (native_missing, native_physical) = manager.resolve_identity(&native).unwrap();
            assert!(!native_physical);
            let raster_stamp = optional_stamp(&raster).unwrap().unwrap();
            manager
                .prepare_planned_pair_checked(
                    &native,
                    &raster,
                    &context,
                    |file| {
                        file.write_all(b"new-native")?;
                        Ok(())
                    },
                    b"new-raster",
                    native_missing,
                    raster_stamp,
                )
                .unwrap()
        }
        OriginalPairKind::RepairNeeded => {
            std::fs::write(&native, b"old-native").unwrap();
            let native_stamp = optional_stamp(&native).unwrap().unwrap();
            manager
                .prepare_pair_checked(
                    &native,
                    &raster,
                    &context,
                    |file| {
                        file.write_all(b"new-native")?;
                        Ok(())
                    },
                    b"new-raster",
                    false,
                    Some((Some(native_stamp), None)),
                )
                .unwrap()
        }
    };
    (directory, manager, native, raster, prepared)
}

#[test]
fn known_failure_after_native_install_rolls_back_before_reporting_failure() {
    let (directory, manager, native, raster) = fixture();
    let old_native = optional_stamp(&native).unwrap().unwrap();
    let old_raster = optional_stamp(&raster).unwrap().unwrap();
    let prepared = prepare(&manager, &native, &raster);
    let restored = match prepared
        .install_outcome_inner(&JobContext::new(), true, None, false, false, None)
        .unwrap()
    {
        PairInstallOutcome::RolledBack {
            error: IoError::InvalidInput("injected pair installation failure"),
            restored: Some(restored),
        } => restored,
        outcome => panic!("unexpected install outcome: {outcome:?}"),
    };
    assert_eq!(std::fs::read(native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(raster).unwrap(), b"old-raster");
    assert_ne!(restored.native.unwrap().identity, old_native.identity);
    assert_eq!(restored.raster, Some(old_raster));
    assert_eq!(std::fs::read_dir(directory.0.clone()).unwrap().count(), 2);
}

#[test]
fn external_replacement_wins_the_live_install_delete_publish_race() {
    let (directory, manager, native, raster) = fixture();
    let prepared = prepare(&manager, &native, &raster);
    let outcome = prepared
        .install_outcome_inner(
            &JobContext::new(),
            false,
            None,
            false,
            false,
            Some(b"external-native"),
        )
        .unwrap();

    assert!(matches!(
        outcome,
        PairInstallOutcome::FailedAfterPublication { .. }
    ));
    assert_eq!(std::fs::read(&native).unwrap(), b"external-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
    assert!(std::fs::read_dir(&directory.0).unwrap().count() > 2);
}

#[test]
fn external_replacement_of_second_raster_is_preserved_with_conflict_evidence() {
    let (directory, manager, native, raster) = fixture();
    let mut prepared = prepare(&manager, &native, &raster);
    publish_rollback_marker(
        &prepared.journal,
        &prepared.record,
        Some(&prepared.rollback_stage_proof),
    )
    .unwrap();
    install_member_exact(
        &prepared.parent,
        &prepared.record.native,
        &JobContext::new(),
        None,
    )
    .unwrap();

    assert!(
        install_member_exact(
            &prepared.parent,
            &prepared.record.raster,
            &JobContext::new(),
            Some(b"external-raster"),
        )
        .is_err()
    );
    let journal = prepared.journal.clone();
    prepared.discard_on_drop = false;
    drop(prepared);

    assert!(manager.recover_pairs(&native, &JobContext::new()).is_err());
    assert_eq!(std::fs::read(&native).unwrap(), b"new-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"external-raster");
    assert!(journal.exists());
    assert!(std::fs::read_dir(&directory.0).unwrap().count() > 2);
}

#[test]
fn late_alias_after_both_replacements_rolls_back_both_members() {
    let directory = Directory::new();
    let native = directory.0.join("cell.inkpod");
    let raster = directory.0.join("cell.tiff");
    let alias = directory.0.join("cell.tif");
    std::fs::write(&native, b"old-native").unwrap();
    std::fs::write(&raster, b"old-raster").unwrap();
    let manager = IoManager::new(IoConfig::default()).unwrap();
    let prepared = prepare(&manager, &native, &raster);

    let outcome = prepared
        .install_outcome_inner(&JobContext::new(), false, Some(&alias), false, false, None)
        .unwrap();
    assert!(matches!(
        outcome,
        PairInstallOutcome::RolledBack {
            error: IoError::ConfirmationRequired,
            restored: None
        }
    ));
    assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
    assert_eq!(
        std::fs::read(&alias).unwrap(),
        b"injected late companion alias"
    );
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 3);
}

#[test]
fn final_proof_failure_after_both_replacements_rolls_back_both_members() {
    let (directory, manager, native, raster) = fixture();
    let old_native = optional_stamp(&native).unwrap().unwrap();
    let old_raster = optional_stamp(&raster).unwrap().unwrap();
    let prepared = prepare(&manager, &native, &raster);
    let advertised = prepared.replacement_stamps();

    let restored = match prepared
        .install_outcome_inner(&JobContext::new(), false, None, true, false, None)
        .unwrap()
    {
        PairInstallOutcome::RolledBack {
            error: IoError::ChangedDuringRead,
            restored: Some(restored),
        } => restored,
        outcome => panic!("unexpected install outcome: {outcome:?}"),
    };
    assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
    let restored_native = optional_stamp(&native).unwrap().unwrap();
    let restored_raster = optional_stamp(&raster).unwrap().unwrap();
    assert_eq!(restored.native, Some(restored_native));
    assert_eq!(restored.raster, Some(restored_raster));
    assert_ne!(restored_native.identity, advertised.0.identity);
    assert_ne!(restored_raster.identity, advertised.1.identity);
    assert_eq!(restored_native.length, old_native.length);
    assert_eq!(restored_raster.length, old_raster.length);
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn overwrite_advertises_new_identities_that_become_both_final_members() {
    let (directory, manager, native, raster) = fixture();
    let old_native = optional_stamp(&native).unwrap().unwrap();
    let old_raster = optional_stamp(&raster).unwrap().unwrap();
    let prepared = prepare(&manager, &native, &raster);
    let advertised = prepared.replacement_stamps();
    assert_ne!(advertised.0.identity, old_native.identity);
    assert_ne!(advertised.1.identity, old_raster.identity);

    let installed = prepared
        .install_inner(&JobContext::new(), false, None, false, false)
        .unwrap();
    assert_eq!(installed.0.identity, advertised.0.identity);
    assert_eq!(installed.1.identity, advertised.1.identity);
    assert_eq!(optional_stamp(&native).unwrap(), Some(installed.0));
    assert_eq!(optional_stamp(&raster).unwrap(), Some(installed.1));
    assert_eq!(std::fs::read(&native).unwrap(), b"new-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"new-raster");
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn cleanup_failure_after_durable_commit_is_recovered_by_the_next_same_pair_save() {
    let (directory, manager, native, raster) = fixture();
    let prepared = prepare(&manager, &native, &raster);
    let advertised = prepared.replacement_stamps();

    let (native_stamp, raster_stamp) = prepared
        .install_inner(&JobContext::new(), false, None, false, true)
        .unwrap();
    assert_eq!(native_stamp.identity, advertised.0.identity);
    assert_eq!(raster_stamp.identity, advertised.1.identity);
    assert_eq!(native_stamp.length, advertised.0.length);
    assert_eq!(raster_stamp.length, advertised.1.length);
    assert_eq!(std::fs::read(&native).unwrap(), b"new-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"new-raster");
    assert_eq!(optional_stamp(&native).unwrap(), Some(native_stamp));
    assert_eq!(optional_stamp(&raster).unwrap(), Some(raster_stamp));
    assert!(std::fs::read_dir(&directory.0).unwrap().count() > 2);

    let next = manager
        .prepare_pair_checked(
            &native,
            &raster,
            &JobContext::new(),
            |file| {
                file.write_all(b"newer-native")?;
                Ok(())
            },
            b"newer-raster",
            false,
            Some((Some(native_stamp), Some(raster_stamp))),
        )
        .unwrap();
    let next_advertised = next.replacement_stamps();
    let (next_native, next_raster) = next
        .install_inner(&JobContext::new(), false, None, false, false)
        .unwrap();

    assert_eq!(next_native.identity, next_advertised.0.identity);
    assert_eq!(next_native.length, next_advertised.0.length);
    assert_eq!(next_raster.identity, next_advertised.1.identity);
    assert_eq!(next_raster.length, next_advertised.1.length);
    assert_eq!(std::fs::read(&native).unwrap(), b"newer-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"newer-raster");
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn scratch_drop_preserves_an_external_replacement_of_its_created_path() {
    let directory = Directory::new();
    let path = directory.0.join("scratch.tmp");
    let mut scratch = Scratch::default();
    let mut file = scratch.create(path.clone()).unwrap();
    file.write_all(b"owned").unwrap();
    file.flush().unwrap();
    let owned_identity = backend::stamp(&file).unwrap().identity;
    drop(file);

    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, b"external").unwrap();
    let external_identity = optional_stamp(&path).unwrap().unwrap().identity;
    assert_ne!(external_identity, owned_identity);
    drop(scratch);

    assert_eq!(std::fs::read(path).unwrap(), b"external");
}

#[test]
fn rollback_publication_retains_a_replaced_creation_stage() {
    let (_directory, _manager, _native, _raster, mut prepared) =
        prepare_original_pair(OriginalPairKind::Committed);
    let stage = rollback_stage_path(&prepared.journal);
    let marker = rollback_path(&prepared.journal);
    std::fs::remove_file(&stage).unwrap();
    std::fs::write(&stage, b"external rollback stage").unwrap();

    assert!(
        publish_rollback_marker(
            &prepared.journal,
            &prepared.record,
            Some(&prepared.rollback_stage_proof),
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&stage).unwrap(), b"external rollback stage");
    assert!(!marker.exists());
    assert!(prepared.journal.exists());
    prepared.discard_on_drop = false;
}

#[test]
fn rollback_publication_detects_replacement_between_verification_and_rename() {
    let (_directory, _manager, native, raster, mut prepared) =
        prepare_original_pair(OriginalPairKind::Committed);
    let marker = rollback_path(&prepared.journal);

    assert!(
        publish_rollback_marker_inner(
            &prepared.journal,
            &prepared.record,
            Some(&prepared.rollback_stage_proof),
            Some(b"external after verification"),
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(&marker).unwrap(),
        b"external after verification"
    );
    assert!(prepared.journal.exists());
    assert_eq!(std::fs::read(native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(raster).unwrap(), b"old-raster");
    prepared.discard_on_drop = false;
}

#[test]
fn live_cleanup_retains_a_replaced_creation_stage_and_all_authority() {
    let (_directory, _manager, native, raster, mut prepared) =
        prepare_original_pair(OriginalPairKind::Committed);
    let stage = commit_stage_path(&prepared.journal);
    std::fs::remove_file(&stage).unwrap();
    std::fs::write(&stage, b"external commit stage").unwrap();

    assert!(
        cleanup_with_stamps(
            &prepared.parent,
            &prepared.journal,
            &prepared.record,
            &prepared.journal_proof,
            &prepared.commit_stage_proof,
            &prepared.rollback_stage_proof,
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&stage).unwrap(), b"external commit stage");
    assert_eq!(std::fs::read(native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(raster).unwrap(), b"old-raster");
    assert!(prepared.journal.exists());
    prepared.discard_on_drop = false;
}

#[test]
fn interrupted_prepared_partial_and_complete_pairs_recover_with_identity_and_digest_checks() {
    for installed in 0..=2 {
        let (directory, manager, native, raster) = fixture();
        let mut prepared = prepare(&manager, &native, &raster);
        if installed >= 1 {
            publish_rollback_marker(
                &prepared.journal,
                &prepared.record,
                Some(&prepared.rollback_stage_proof),
            )
            .unwrap();
            install_member_exact(
                &prepared.parent,
                &prepared.record.native,
                &JobContext::new(),
                None,
            )
            .unwrap();
        }
        if installed == 2 {
            install_member_exact(
                &prepared.parent,
                &prepared.record.raster,
                &JobContext::new(),
                None,
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
            PairRecovery::RolledBack,
        ][installed];
        assert_eq!(
            manager.recover_pairs(&native, &JobContext::new()).unwrap(),
            expected
        );
        assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
        assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
        assert_eq!(std::fs::read_dir(directory.0.clone()).unwrap().count(), 2);
    }
}

#[test]
fn native_first_recovery_restores_committed_planned_and_repair_needed_at_every_cut() {
    for kind in [
        OriginalPairKind::Committed,
        OriginalPairKind::Planned,
        OriginalPairKind::RepairNeeded,
    ] {
        for installed in 0..=2 {
            let (directory, manager, native, raster, mut prepared) = prepare_original_pair(kind);
            publish_rollback_marker(
                &prepared.journal,
                &prepared.record,
                Some(&prepared.rollback_stage_proof),
            )
            .unwrap();
            if installed >= 1 {
                install_member_exact(
                    &prepared.parent,
                    &prepared.record.native,
                    &JobContext::new(),
                    None,
                )
                .unwrap();
            }
            if installed == 2 {
                install_member_exact(
                    &prepared.parent,
                    &prepared.record.raster,
                    &JobContext::new(),
                    None,
                )
                .unwrap();
            }
            prepared.discard_on_drop = false;
            drop(prepared);

            assert_eq!(
                manager.recover_pairs(&native, &JobContext::new()).unwrap(),
                PairRecovery::RolledBack,
                "{kind:?} failed at cut {installed}"
            );
            match kind {
                OriginalPairKind::Committed => {
                    assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
                    assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
                    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
                }
                OriginalPairKind::Planned => {
                    assert!(!native.exists());
                    assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
                    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
                }
                OriginalPairKind::RepairNeeded => {
                    assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
                    assert!(!raster.exists());
                    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
                }
            }
        }
    }
}

#[test]
fn prepared_journal_discards_missing_or_exact_stages_but_retains_torn_stages() {
    for commit_stage_state in 0..3 {
        let (directory, manager, native, raster) = fixture();
        let mut prepared = prepare(&manager, &native, &raster);
        assert!(prepared.journal.exists());
        let commit_stage = commit_stage_path(&prepared.journal);
        let rollback_stage = rollback_stage_path(&prepared.journal);
        assert!(commit_stage.exists());
        assert!(rollback_stage.exists());
        match commit_stage_state {
            0 => {
                std::fs::remove_file(&commit_stage).unwrap();
                std::fs::remove_file(&rollback_stage).unwrap();
            }
            1 => {}
            2 => {
                std::fs::write(&commit_stage, b"torn commit stage").unwrap();
                std::fs::write(&rollback_stage, b"torn rollback stage").unwrap();
            }
            _ => unreachable!(),
        }
        prepared.discard_on_drop = false;
        drop(prepared);

        let recovery = manager.recover_pairs(&native, &JobContext::new());
        assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
        assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
        if commit_stage_state == 2 {
            assert!(recovery.is_err());
            assert_eq!(std::fs::read(&commit_stage).unwrap(), b"torn commit stage");
            assert_eq!(
                std::fs::read(&rollback_stage).unwrap(),
                b"torn rollback stage"
            );
            assert!(std::fs::read_dir(&directory.0).unwrap().count() > 2);
        } else {
            assert_eq!(recovery.unwrap(), PairRecovery::PreparedDiscarded);
            assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
        }
    }
}

#[test]
fn durable_rollback_marker_resumes_after_exact_delete_before_backup_publication() {
    let (directory, manager, native, raster) = fixture();
    let mut prepared = prepare(&manager, &native, &raster);
    publish_rollback_marker(
        &prepared.journal,
        &prepared.record,
        Some(&prepared.rollback_stage_proof),
    )
    .unwrap();
    install_member_exact(
        &prepared.parent,
        &prepared.record.native,
        &JobContext::new(),
        None,
    )
    .unwrap();
    let current = installed_replacement_stamp(
        &native,
        &prepared.record.native.replacement,
        &JobContext::new(),
    )
    .unwrap();
    backend::remove_exact(&native, current).unwrap();
    assert!(!native.exists());
    prepared.discard_on_drop = false;
    drop(prepared);

    assert_eq!(
        manager.recover_pairs(&native, &JobContext::new()).unwrap(),
        PairRecovery::RolledBack
    );
    assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn rollback_revalidates_replacement_identity_before_exact_delete() {
    let (_directory, _manager, _native, raster) = fixture();
    let mut prepared = prepare(&_manager, &_native, &raster);
    backend::replace(
        &prepared.parent.join(&prepared.record.raster.stage),
        &raster,
        true,
    )
    .unwrap();
    publish_rollback_marker(
        &prepared.journal,
        &prepared.record,
        Some(&prepared.rollback_stage_proof),
    )
    .unwrap();
    let stale_state = classify(
        &prepared.parent,
        &prepared.record.raster,
        &JobContext::new(),
    )
    .unwrap();
    std::fs::remove_file(&raster).unwrap();
    std::fs::write(&raster, b"external replacement").unwrap();

    assert!(
        restore_member_exact(
            &prepared.parent,
            &prepared.record.raster,
            stale_state,
            &JobContext::new(),
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&raster).unwrap(), b"external replacement");
    prepared.discard_on_drop = false;
}

#[test]
fn recovery_completes_two_replacements_only_after_durable_commit_marker() {
    let (directory, manager, native, raster) = fixture();
    let mut prepared = prepare(&manager, &native, &raster);
    publish_rollback_marker(
        &prepared.journal,
        &prepared.record,
        Some(&prepared.rollback_stage_proof),
    )
    .unwrap();
    install_member_exact(
        &prepared.parent,
        &prepared.record.native,
        &JobContext::new(),
        None,
    )
    .unwrap();
    install_member_exact(
        &prepared.parent,
        &prepared.record.raster,
        &JobContext::new(),
        None,
    )
    .unwrap();
    publish_commit_marker(&prepared.journal, &prepared.commit_stage_proof).unwrap();
    prepared.discard_on_drop = false;
    drop(prepared);

    assert_eq!(
        manager.recover_pairs(&native, &JobContext::new()).unwrap(),
        PairRecovery::Completed
    );
    assert_eq!(std::fs::read(&native).unwrap(), b"new-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"new-raster");
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn orphan_commit_marker_finishes_cleanup_without_rolling_back() {
    let (directory, manager, native, raster) = fixture();
    let mut prepared = prepare(&manager, &native, &raster);
    publish_rollback_marker(
        &prepared.journal,
        &prepared.record,
        Some(&prepared.rollback_stage_proof),
    )
    .unwrap();
    install_member_exact(
        &prepared.parent,
        &prepared.record.native,
        &JobContext::new(),
        None,
    )
    .unwrap();
    install_member_exact(
        &prepared.parent,
        &prepared.record.raster,
        &JobContext::new(),
        None,
    )
    .unwrap();
    publish_commit_marker(&prepared.journal, &prepared.commit_stage_proof).unwrap();
    std::fs::remove_file(&prepared.journal).unwrap();
    sync_directory(&prepared.parent).unwrap();
    prepared.discard_on_drop = false;
    drop(prepared);

    assert_eq!(
        manager.recover_pairs(&native, &JobContext::new()).unwrap(),
        PairRecovery::Completed
    );
    assert_eq!(std::fs::read(&native).unwrap(), b"new-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"new-raster");
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn committed_marker_with_unpublished_members_is_retained_as_conflict() {
    let (directory, manager, native, raster) = fixture();
    let mut prepared = prepare(&manager, &native, &raster);
    publish_commit_marker(&prepared.journal, &prepared.commit_stage_proof).unwrap();
    let marker = commit_path(&prepared.journal);
    prepared.discard_on_drop = false;
    drop(prepared);

    assert!(matches!(
        manager.recover_pairs(&native, &JobContext::new()),
        Err(IoError::InvalidInput(_))
    ));
    assert_eq!(std::fs::read(&native).unwrap(), b"old-native");
    assert_eq!(std::fs::read(&raster).unwrap(), b"old-raster");
    assert!(marker.exists());
    assert!(std::fs::read_dir(&directory.0).unwrap().count() > 2);
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
    publish_rollback_marker(
        &prepared.journal,
        &prepared.record,
        Some(&prepared.rollback_stage_proof),
    )
    .unwrap();
    install_member_exact(
        &prepared.parent,
        &prepared.record.native,
        &JobContext::new(),
        None,
    )
    .unwrap();
    let backup = prepared.parent.join(&prepared.record.native.backup);
    std::fs::write(&backup, b"tampered-backup").unwrap();
    let journal = prepared.journal.clone();
    prepared.discard_on_drop = false;
    drop(prepared);
    assert!(manager.recover_pairs(&native, &JobContext::new()).is_err());
    assert_eq!(std::fs::read(native).unwrap(), b"new-native");
    assert_eq!(std::fs::read(raster).unwrap(), b"old-raster");
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
    let mut invalid_phase = bytes.clone();
    invalid_phase[12] = 2;
    let payload = invalid_phase.len() - 32;
    let digest = *blake3::hash(&invalid_phase[..payload]).as_bytes();
    invalid_phase[payload..].copy_from_slice(&digest);
    assert!(codec::decode(&invalid_phase).is_err());
    let mut unsafe_record = prepared.record.clone();
    unsafe_record.raster.stage = "../outside".to_owned();
    assert!(codec::encode(&unsafe_record).is_err());
}
