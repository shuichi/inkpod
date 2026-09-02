use inkpod_io::{IoConfig, IoError, IoManager, JobContext, PairRecovery};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(1);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "inkpod-pair-contract-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn native(&self) -> PathBuf {
        self.0.join("cell.inkpod")
    }
    fn raster(&self) -> PathBuf {
        self.0.join("cell.png")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manager() -> IoManager {
    IoManager::new(IoConfig {
        worker_count: 1,
        ..IoConfig::default()
    })
    .unwrap()
}

fn finish_cleanup(manager: &IoManager) {
    // A queued barrier observes the asynchronous Drop cleanup without assuming
    // the UI/owner thread blocks on filesystem work.
    let barrier = manager.submit(|_| Ok(())).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = barrier.try_take() {
            result.unwrap();
            return;
        }
        assert!(
            Instant::now() < deadline,
            "asynchronous pair cleanup did not finish"
        );
        std::thread::yield_now();
    }
}

#[test]
fn preparation_is_nonpublishing_and_install_updates_both_files() {
    let directory = Directory::new();
    fs::write(directory.native(), b"native-old").unwrap();
    fs::write(directory.raster(), b"raster-old").unwrap();
    let manager = manager();
    let context = JobContext::new();
    let prepared = manager
        .prepare_pair(
            &directory.native(),
            &directory.raster(),
            &context,
            |file| {
                file.write_all(b"native-new")?;
                Ok(())
            },
            b"raster-new",
            true,
        )
        .unwrap();
    assert_eq!(fs::read(directory.native()).unwrap(), b"native-old");
    assert_eq!(fs::read(directory.raster()).unwrap(), b"raster-old");
    assert!(matches!(
        manager.recover_pairs(&directory.native(), &context),
        Err(IoError::ResourceBusy(_))
    ));
    prepared.install(&context).unwrap();
    assert_eq!(fs::read(directory.native()).unwrap(), b"native-new");
    assert_eq!(fs::read(directory.raster()).unwrap(), b"raster-new");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    assert_eq!(
        manager
            .recover_pairs(&directory.native(), &context)
            .unwrap(),
        PairRecovery::NotNeeded
    );
}

#[test]
fn dropped_cancelled_and_failed_preparations_preserve_original_pair() {
    let directory = Directory::new();
    fs::write(directory.native(), b"native-old").unwrap();
    fs::write(directory.raster(), b"raster-old").unwrap();
    let manager = manager();
    for cancel in [false, true] {
        let context = JobContext::new();
        let prepared = manager
            .prepare_pair(
                &directory.native(),
                &directory.raster(),
                &context,
                |file| {
                    file.write_all(b"native-new")?;
                    Ok(())
                },
                b"raster-new",
                true,
            )
            .unwrap();
        if cancel {
            context.cancel();
            assert!(matches!(
                prepared.install(&context),
                Err(IoError::Cancelled)
            ));
        } else {
            drop(prepared);
        }
        finish_cleanup(&manager);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    }
    let failure = manager.prepare_pair(
        &directory.native(),
        &directory.raster(),
        &JobContext::new(),
        |file| {
            file.write_all(b"part")?;
            Err(IoError::InvalidInput("injected encode failure"))
        },
        b"raster-new",
        true,
    );
    assert!(failure.is_err());
    assert_eq!(fs::read(directory.native()).unwrap(), b"native-old");
    assert_eq!(fs::read(directory.raster()).unwrap(), b"raster-old");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn external_change_after_prepare_is_not_overwritten() {
    let directory = Directory::new();
    fs::write(directory.native(), b"native-old").unwrap();
    fs::write(directory.raster(), b"raster-old").unwrap();
    let manager = manager();
    let context = JobContext::new();
    let prepared = manager
        .prepare_pair(
            &directory.native(),
            &directory.raster(),
            &context,
            |file| {
                file.write_all(b"native-new")?;
                Ok(())
            },
            b"raster-new",
            true,
        )
        .unwrap();
    fs::write(directory.raster(), b"externally-changed").unwrap();
    assert!(matches!(
        prepared.install(&context),
        Err(IoError::ChangedDuringRead)
    ));
    finish_cleanup(&manager);
    assert_eq!(fs::read(directory.native()).unwrap(), b"native-old");
    assert_eq!(fs::read(directory.raster()).unwrap(), b"externally-changed");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn same_format_aliases_are_rejected_during_prepare_and_before_install() {
    let directory = Directory::new();
    let native = directory.native();
    let raster = directory.0.join("cell.tiff");
    let alias = directory.0.join("cell.tif");
    fs::write(&native, b"native-old").unwrap();
    fs::write(&raster, b"raster-old").unwrap();
    fs::write(&alias, b"unexpected-alias").unwrap();
    let manager = manager();
    let context = JobContext::new();

    assert!(matches!(
        manager.prepare_pair(
            &native,
            &raster,
            &context,
            |file| {
                file.write_all(b"native-new")?;
                Ok(())
            },
            b"raster-new",
            true,
        ),
        Err(IoError::ConfirmationRequired)
    ));

    fs::remove_file(&alias).unwrap();
    let prepared = manager
        .prepare_pair(
            &native,
            &raster,
            &context,
            |file| {
                file.write_all(b"native-new")?;
                Ok(())
            },
            b"raster-new",
            true,
        )
        .unwrap();
    fs::write(&alias, b"late-alias").unwrap();
    assert!(matches!(
        prepared.install(&context),
        Err(IoError::ConfirmationRequired)
    ));
    finish_cleanup(&manager);
    assert_eq!(fs::read(&native).unwrap(), b"native-old");
    assert_eq!(fs::read(&raster).unwrap(), b"raster-old");
    assert_eq!(fs::read(&alias).unwrap(), b"late-alias");
}

#[test]
fn captured_stamps_allow_normal_save_and_require_confirmation_after_external_change() {
    let directory = Directory::new();
    fs::write(directory.native(), b"native-old").unwrap();
    fs::write(directory.raster(), b"raster-old").unwrap();
    let manager = manager();
    let context = JobContext::new();
    let expected = Some((
        Some(manager.metadata(&directory.native(), &context).unwrap()),
        Some(manager.metadata(&directory.raster(), &context).unwrap()),
    ));
    assert!(matches!(
        manager.prepare_pair(
            &directory.native(),
            &directory.raster(),
            &context,
            |_| Ok(()),
            b"new",
            false
        ),
        Err(IoError::ConfirmationRequired)
    ));
    let installed = manager
        .prepare_pair_checked(
            &directory.native(),
            &directory.raster(),
            &context,
            |file| {
                file.write_all(b"native-new")?;
                Ok(())
            },
            b"raster-new",
            false,
            expected,
        )
        .unwrap()
        .install_with_stamps(&context)
        .unwrap();
    assert_eq!(
        installed.0,
        manager.metadata(&directory.native(), &context).unwrap()
    );
    assert_eq!(
        installed.1,
        manager.metadata(&directory.raster(), &context).unwrap()
    );
    fs::write(directory.raster(), b"external-change").unwrap();
    assert!(matches!(
        manager.prepare_pair_checked(
            &directory.native(),
            &directory.raster(),
            &context,
            |_| Ok(()),
            b"new",
            false,
            Some((Some(installed.0), Some(installed.1)))
        ),
        Err(IoError::ConfirmationRequired)
    ));
    assert_eq!(fs::read(directory.raster()).unwrap(), b"external-change");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn shutdown_drop_retains_journal_for_next_manager_recovery() {
    let directory = Directory::new();
    fs::write(directory.native(), b"native-old").unwrap();
    fs::write(directory.raster(), b"raster-old").unwrap();
    let manager = manager();
    let context = JobContext::new();
    let prepared = manager
        .prepare_pair(
            &directory.native(),
            &directory.raster(),
            &context,
            |file| {
                file.write_all(b"native-new")?;
                Ok(())
            },
            b"raster-new",
            true,
        )
        .unwrap();
    manager.shutdown();
    drop(prepared);
    assert!(fs::read_dir(&directory.0).unwrap().count() > 2);
    let reopened = IoManager::new(IoConfig::default()).unwrap();
    assert_eq!(
        reopened
            .recover_pairs(&directory.native(), &context)
            .unwrap(),
        PairRecovery::PreparedDiscarded
    );
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    assert_eq!(fs::read(directory.native()).unwrap(), b"native-old");
}

#[test]
fn missing_companion_can_be_rebuilt_but_missing_native_requires_confirmation() {
    let directory = Directory::new();
    fs::write(directory.native(), b"native-old").unwrap();
    let manager = manager();
    let context = JobContext::new();
    let expected = Some((
        Some(manager.metadata(&directory.native(), &context).unwrap()),
        None,
    ));
    let installed = manager
        .prepare_pair_checked(
            &directory.native(),
            &directory.raster(),
            &context,
            |file| {
                file.write_all(b"native-new")?;
                Ok(())
            },
            b"raster-new",
            false,
            expected,
        )
        .unwrap()
        .install_with_stamps(&context)
        .unwrap();
    assert_eq!(fs::read(directory.raster()).unwrap(), b"raster-new");
    fs::remove_file(directory.native()).unwrap();
    assert!(matches!(
        manager.prepare_pair_checked(
            &directory.native(),
            &directory.raster(),
            &context,
            |_| Ok(()),
            b"new",
            false,
            Some((Some(installed.0), Some(installed.1)))
        ),
        Err(IoError::ConfirmationRequired)
    ));

    let removed = Directory::new();
    fs::write(removed.native(), b"native-old").unwrap();
    fs::write(removed.raster(), b"raster-old").unwrap();
    let removed_expected = Some((
        Some(manager.metadata(&removed.native(), &context).unwrap()),
        Some(manager.metadata(&removed.raster(), &context).unwrap()),
    ));
    fs::remove_file(removed.raster()).unwrap();
    assert!(matches!(
        manager.prepare_pair_checked(
            &removed.native(),
            &removed.raster(),
            &context,
            |_| Ok(()),
            b"new",
            false,
            removed_expected,
        ),
        Err(IoError::ConfirmationRequired)
    ));

    let appeared = Directory::new();
    fs::write(appeared.native(), b"native-old").unwrap();
    let appeared_expected = Some((
        Some(manager.metadata(&appeared.native(), &context).unwrap()),
        None,
    ));
    fs::write(appeared.raster(), b"external-raster").unwrap();
    assert!(matches!(
        manager.prepare_pair_checked(
            &appeared.native(),
            &appeared.raster(),
            &context,
            |_| Ok(()),
            b"new",
            false,
            appeared_expected,
        ),
        Err(IoError::ConfirmationRequired)
    ));
}

#[test]
fn planned_pair_requires_the_exact_open_time_missing_native_and_raster_stamp() {
    let directory = Directory::new();
    fs::write(directory.raster(), b"raster-old").unwrap();
    let manager = manager();
    let context = JobContext::new();
    let (native_missing, physical) = manager.resolve_identity(&directory.native()).unwrap();
    assert!(!physical);
    let raster = manager.metadata(&directory.raster(), &context).unwrap();
    manager
        .prepare_planned_pair_checked(
            &directory.native(),
            &directory.raster(),
            &context,
            |file| {
                file.write_all(b"native-new")?;
                Ok(())
            },
            b"raster-new",
            native_missing,
            raster,
        )
        .unwrap()
        .install(&context)
        .unwrap();
    assert_eq!(fs::read(directory.native()).unwrap(), b"native-new");
    assert_eq!(fs::read(directory.raster()).unwrap(), b"raster-new");

    for change in ["native-created", "raster-changed", "raster-removed"] {
        let directory = Directory::new();
        fs::write(directory.raster(), b"raster-old").unwrap();
        let (native_missing, physical) = manager.resolve_identity(&directory.native()).unwrap();
        assert!(!physical);
        let raster = manager.metadata(&directory.raster(), &context).unwrap();
        match change {
            "native-created" => fs::write(directory.native(), b"unexpected-native").unwrap(),
            "raster-changed" => fs::write(directory.raster(), b"external-change").unwrap(),
            "raster-removed" => fs::remove_file(directory.raster()).unwrap(),
            _ => unreachable!(),
        }
        assert!(matches!(
            manager.prepare_planned_pair_checked(
                &directory.native(),
                &directory.raster(),
                &context,
                |_| Ok(()),
                b"new",
                native_missing,
                raster,
            ),
            Err(IoError::ConfirmationRequired)
        ));
    }
}

#[test]
fn new_pair_is_complete_and_bad_target_aliases_are_rejected() {
    let directory = Directory::new();
    let manager = manager();
    let context = JobContext::new();
    manager
        .prepare_pair(
            &directory.native(),
            &directory.raster(),
            &context,
            |file| {
                file.write_all(b"native-new")?;
                Ok(())
            },
            b"raster-new",
            false,
        )
        .unwrap()
        .install(&context)
        .unwrap();
    assert_eq!(fs::read(directory.native()).unwrap(), b"native-new");
    assert_eq!(fs::read(directory.raster()).unwrap(), b"raster-new");
    assert!(
        manager
            .prepare_pair(
                &directory.native(),
                &directory.raster(),
                &context,
                |_| Ok(()),
                b"",
                false
            )
            .is_err()
    );
    fs::remove_file(directory.raster()).unwrap();
    fs::hard_link(directory.native(), directory.raster()).unwrap();
    assert!(
        manager
            .prepare_pair(
                &directory.native(),
                &directory.raster(),
                &context,
                |_| Ok(()),
                b"",
                true
            )
            .is_err()
    );
}
