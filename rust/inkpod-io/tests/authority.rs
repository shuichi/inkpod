use inkpod_io::{GuardedPublishOutcome, PublishSource};
use inkpod_io::{IoConfig, IoManager, JobContext};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);
fn directory() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "inkpod-authority-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    path
}

fn guarded_available(manager: &IoManager) -> bool {
    if manager.supports_guarded_publication() {
        return true;
    }
    let root = directory();
    let path = root.join("unsupported.inkpod");
    let context = JobContext::new();
    let authority = manager.observe_path_authority(&path, &context).unwrap();
    assert!(matches!(
        manager.publish_guarded(&authority, None, b"never", &context, &mut || false),
        Err(inkpod_io::IoError::InvalidInput(_))
    ));
    assert!(!path.exists());
    fs::remove_dir_all(root).unwrap();
    false
}

#[test]
fn explicit_absolute_path_authority_rejects_cwd_and_preserves_physical_aliases() {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    let context = JobContext::new();
    assert!(
        manager
            .observe_path_authority(std::path::Path::new("relative.inkpod"), &context)
            .is_err()
    );
    let root = directory();
    let a = root.join("a.inkpod");
    let b = root.join("b.inkpod");
    fs::write(&a, b"original").unwrap();
    fs::hard_link(&a, &b).unwrap();
    let original = manager.observe_path_authority(&a, &context).unwrap();
    let alias = manager.observe_path_authority(&b, &context).unwrap();
    assert_eq!(original.object, alias.object);
    assert_eq!(original.parent, alias.parent);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn guarded_publication_installs_once_and_rejects_collision_stale_digest_and_parent() {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    if !guarded_available(&manager) {
        return;
    }
    let context = JobContext::new();
    let root = directory();
    let path = root.join("a.inkpod");
    let absent = manager.observe_path_authority(&path, &context).unwrap();
    assert_eq!(
        manager
            .publish_guarded(&absent, None, b"first", &context, &mut || false)
            .unwrap(),
        GuardedPublishOutcome::Installed
    );
    assert!(
        manager
            .publish_guarded(&absent, None, b"lost", &context, &mut || false)
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), b"first");
    let authority = manager.observe_path_authority(&path, &context).unwrap();
    let loaded = manager.read_bytes(&path, 100, &context).unwrap();
    let proof = PublishSource {
        path: path.clone(),
        stamp: loaded.stamp(),
        digest: *blake3::hash(loaded.bytes()).as_bytes(),
    };
    let bad = PublishSource {
        digest: [1; 32],
        ..proof.clone()
    };
    assert!(
        manager
            .publish_guarded(&authority, Some(bad), b"lost", &context, &mut || false)
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), b"first");
    assert_eq!(
        manager
            .publish_guarded(&authority, Some(proof), b"second", &context, &mut || false)
            .unwrap(),
        GuardedPublishOutcome::Installed
    );
    assert_eq!(fs::read(&path).unwrap(), b"second");
    let later = root.join("b.inkpod");
    let later_authority = manager.observe_path_authority(&later, &context).unwrap();
    assert_eq!(
        manager
            .publish_guarded(&later_authority, None, b"cancelled", &context, &mut || true)
            .unwrap(),
        GuardedPublishOutcome::CancelledBeforeInstall
    );
    assert!(!later.exists());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    let moved = root.with_extension("moved");
    fs::rename(&root, &moved).unwrap();
    fs::create_dir(&root).unwrap();
    assert!(
        manager
            .publish_guarded(&later_authority, None, b"lost", &context, &mut || false)
            .is_err()
    );
    assert!(!later.exists());
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(moved).unwrap();
}

#[test]
fn guarded_overwrite_excludes_writers_and_cancels_without_publication() {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    if !guarded_available(&manager) {
        return;
    }
    for cancel_after_guard in [true, false] {
        let context = JobContext::new();
        let root = directory();
        let path = root.join("original.inkpod");
        let moved = root.join("foreign.inkpod");
        fs::write(&path, b"original").unwrap();
        let authority = manager.observe_path_authority(&path, &context).unwrap();
        let loaded = manager.read_bytes(&path, 100, &context).unwrap();
        let proof = PublishSource {
            path: path.clone(),
            stamp: loaded.stamp(),
            digest: *blake3::hash(loaded.bytes()).as_bytes(),
        };
        let mut saw_guard = false;
        let result = manager.publish_guarded(
            &authority,
            Some(proof),
            b"replacement",
            &context,
            &mut || {
                // A separate open is harmless before the guard is acquired:
                // no truncate/write is requested. Once denied, the source
                // content guard must stay held through cancellation or install.
                if fs::OpenOptions::new().write(true).open(&path).is_err() {
                    saw_guard = true;
                    assert_eq!(fs::read(&path).unwrap(), b"original");
                    cancel_after_guard
                } else {
                    false
                }
            },
        );
        assert!(saw_guard, "the final source validation must hold its guard");
        assert_eq!(
            result.unwrap(),
            if cancel_after_guard {
                GuardedPublishOutcome::CancelledBeforeInstall
            } else {
                GuardedPublishOutcome::Installed
            }
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            if cancel_after_guard {
                &b"original"[..]
            } else {
                &b"replacement"[..]
            }
        );
        let final_authority = manager.observe_path_authority(&path, &context).unwrap();
        if cancel_after_guard {
            assert_eq!(final_authority, authority);
        } else {
            assert_ne!(final_authority.object, authority.object);
        }
        assert!(!moved.exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        // All guards must be released on both cancellation and success.
        fs::write(&path, b"after publication").unwrap();
        fs::rename(&path, &moved).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn guarded_overwrite_rejects_name_changes_observed_before_install() {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    if !guarded_available(&manager) {
        return;
    }
    for replace_name in [false, true] {
        let context = JobContext::new();
        let root = directory();
        let path = root.join("original.inkpod");
        let moved = root.join("foreign.inkpod");
        fs::write(&path, b"original").unwrap();
        let authority = manager.observe_path_authority(&path, &context).unwrap();
        let loaded = manager.read_bytes(&path, 100, &context).unwrap();
        let proof = PublishSource {
            path: path.clone(),
            stamp: loaded.stamp(),
            digest: *blake3::hash(loaded.bytes()).as_bytes(),
        };
        let mut changed = false;
        let result = manager.publish_guarded(
            &authority,
            Some(proof),
            b"must not install",
            &context,
            &mut || {
                if !changed && fs::OpenOptions::new().write(true).open(&path).is_err() {
                    // Overwrite permits DELETE sharing so its own atomic rename
                    // can succeed. A foreign name change before the final path
                    // check must be rejected, even though the open source's
                    // identity and digest still match the planned bytes.
                    fs::rename(&path, &moved).unwrap();
                    if replace_name {
                        fs::write(&path, b"external").unwrap();
                    }
                    changed = true;
                }
                false
            },
        );
        assert!(changed);
        assert!(matches!(
            result,
            Err(inkpod_io::IoError::ConfirmationRequired)
        ));
        assert_eq!(fs::read(&moved).unwrap(), b"original");
        if replace_name {
            assert_eq!(fs::read(&path).unwrap(), b"external");
        } else {
            assert!(!path.exists());
        }
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            1 + usize::from(replace_name)
        );
        fs::write(&moved, b"guard released").unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn guarded_overwrite_install_failure_preserves_source_and_removes_temporary() {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    if !guarded_available(&manager) {
        return;
    }
    let context = JobContext::new();
    let root = directory();
    let path = root.join("readonly.inkpod");
    fs::write(&path, b"original").unwrap();
    let original_permissions = fs::metadata(&path).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let authority = manager.observe_path_authority(&path, &context).unwrap();
    let loaded = manager.read_bytes(&path, 100, &context).unwrap();
    let result = manager.publish_guarded(
        &authority,
        Some(PublishSource {
            path: path.clone(),
            stamp: loaded.stamp(),
            digest: *blake3::hash(loaded.bytes()).as_bytes(),
        }),
        b"must not install",
        &context,
        &mut || false,
    );
    let final_authority = manager.observe_path_authority(&path, &context).unwrap();
    fs::set_permissions(&path, original_permissions).unwrap();
    assert!(matches!(
        result,
        Err(inkpod_io::IoError::UnsupportedAtomicPublication)
    ));
    assert_eq!(final_authority, authority);
    assert_eq!(fs::read(&path).unwrap(), b"original");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    fs::write(&path, b"guard released").unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn folder_publication_revalidates_source_after_temporary_write() {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    if !guarded_available(&manager) {
        return;
    }
    let context = JobContext::new();
    let root = directory();
    let source = root.join("input.inkpod");
    let output = root.join("output.inkpod");
    fs::write(&source, b"original").unwrap();
    let bytes = manager.read_bytes(&source, 100, &context).unwrap();
    let proof = PublishSource {
        path: source.clone(),
        stamp: bytes.stamp(),
        digest: *blake3::hash(bytes.bytes()).as_bytes(),
    };
    let destination = manager.observe_path_authority(&output, &context).unwrap();
    let mut changed = false;
    let result =
        manager.publish_guarded(&destination, Some(proof), b"result", &context, &mut || {
            if !changed {
                fs::write(&source, b"external").unwrap();
                changed = true;
            }
            false
        });
    assert!(result.is_err());
    assert!(!output.exists());
    assert_eq!(fs::read(&source).unwrap(), b"external");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn temporary_substitution_does_not_delete_foreign_replacement() {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    if !guarded_available(&manager) {
        return;
    }
    let context = JobContext::new();
    let root = directory();
    let output = root.join("output.inkpod");
    let moved = root.join("moved.tmp");
    let destination = manager.observe_path_authority(&output, &context).unwrap();
    let mut replacement = None;
    let result = manager.publish_guarded(&destination, None, b"result", &context, &mut || {
        if replacement.is_none() {
            let temporary = fs::read_dir(&root)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".inkpod-script-")
                })
                .unwrap();
            fs::rename(&temporary, &moved).unwrap();
            fs::write(&temporary, b"foreign").unwrap();
            replacement = Some(temporary);
        }
        false
    });
    assert!(result.is_err());
    assert!(!output.exists());
    assert_eq!(fs::read(replacement.unwrap()).unwrap(), b"foreign");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn temporary_writer_exclusion_prevents_encoded_byte_corruption() {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    if !guarded_available(&manager) {
        return;
    }
    let root = directory();
    let output = root.join("output.inkpod");
    let context = JobContext::new();
    let destination = manager.observe_path_authority(&output, &context).unwrap();
    let encoded = vec![0x73; 131_072];
    let mut polls = 0;
    let result = manager
        .publish_guarded(&destination, None, &encoded, &context, &mut || {
            polls += 1;
            if polls == 2 {
                let path = fs::read_dir(&root)
                    .unwrap()
                    .map(|value| value.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(".inkpod-script-")
                    })
                    .unwrap();
                assert!(
                    fs::OpenOptions::new().write(true).open(path).is_err(),
                    "external writer must be denied while encoding is written"
                );
            }
            false
        })
        .unwrap();
    assert_eq!(result, GuardedPublishOutcome::Installed);
    assert_eq!(fs::read(output).unwrap(), encoded);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn parent_replacement_is_denied_through_directory_creation_and_enumeration() {
    let manager = IoManager::new(IoConfig::default()).unwrap();
    if !guarded_available(&manager) {
        return;
    }
    let root = directory();
    let moved = root.with_extension("replaced");
    let context = JobContext::new();
    let output = root.join("out/nested/result.inkpod");
    let authority = manager.observe_path_authority(&output, &context).unwrap();
    let mut attempted = false;
    let mut created = Vec::new();
    let prepared = manager
        .prepare_directory_chain(
            &authority,
            &context,
            &mut || {
                if !attempted {
                    attempted = true;
                    assert!(
                        fs::rename(&root, &moved).is_err(),
                        "approved anchor must remain pinned through relative creates"
                    );
                }
                false
            },
            &mut created,
        )
        .unwrap();
    assert_eq!(created.len(), 2);
    assert!(!prepared.is_directory);
    assert!(root.join("out/nested").exists());
    assert!(!moved.exists());
    let directory_authority = manager.observe_path_authority(&root, &context).unwrap();
    manager
        .with_directory_authority(&root, &directory_authority, &context, || {
            assert!(
                fs::rename(&root, &moved).is_err(),
                "enumerated directory must remain pinned"
            );
            Ok(())
        })
        .unwrap();
    fs::remove_dir_all(root).unwrap();
}
