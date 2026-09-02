use inkpod_format::{CommonRaster, CommonRasterFormat, decode_common_raster, encode_common_raster};
use inkpod_io::{IoConfig, IoError, IoManager, JobContext, JobState};
use std::fs::{self, File, FileTimes};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let number = DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "inkpod-io-contract-{}-{number}",
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

fn config() -> IoConfig {
    IoConfig {
        max_images: 8,
        max_file_bytes: 4096,
        max_encoded_bytes: 32_768,
        max_decoded_bytes: 256,
        worker_count: 3,
        queue_capacity: 16,
    }
}

fn encoded(path: &Path, color: u8, format: CommonRasterFormat) -> Vec<u8> {
    // Use the production BMP decoder to obtain the image pixel-format DTO without
    // making the I/O crate depend directly on inkpod-image.
    let mut bmp = vec![0_u8; 58];
    bmp[..2].copy_from_slice(b"BM");
    bmp[10..14].copy_from_slice(&54_u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40_u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&1_i32.to_le_bytes());
    bmp[22..26].copy_from_slice(&1_i32.to_le_bytes());
    bmp[26..28].copy_from_slice(&1_u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24_u16.to_le_bytes());
    bmp[54..57].copy_from_slice(&[color, color, color]);
    let raster: CommonRaster = decode_common_raster(CommonRasterFormat::Bmp, &bmp).unwrap();
    let bytes = encode_common_raster(format, &raster, false).unwrap();
    fs::write(path, &bytes).unwrap();
    bytes
}

fn png(path: &Path, color: u8) -> Vec<u8> {
    encoded(path, color, CommonRasterFormat::Png)
}

fn wait<T>(job: &inkpod_io::IoJob<T>) -> inkpod_io::IoResult<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = job.try_take() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "I/O job did not complete: {:?}",
            job.poll()
        );
        std::thread::yield_now();
    }
}

#[test]
fn same_size_timestamp_preserved_tga_rewrite_invalidates_cache() {
    let directory = Directory::new();
    let path = directory.path("rewritten.tga");
    let original = encoded(&path, 42, CommonRasterFormat::Tga);
    let manager = IoManager::new(config()).unwrap();
    let first = manager.read_image(&path, &JobContext::new()).unwrap();
    let first_stamp = first.source().stamp();
    let original_modified = fs::metadata(&path).unwrap().modified().unwrap();
    let replacement = encoded(&path, 84, CommonRasterFormat::Tga);
    assert_eq!(replacement.len(), original.len());
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(original_modified))
        .unwrap();
    let rewritten_stamp = manager.metadata(&path, &JobContext::new()).unwrap();
    assert_eq!(rewritten_stamp.identity, first_stamp.identity);
    assert_eq!(rewritten_stamp.length, first_stamp.length);
    assert_eq!(rewritten_stamp.modified, first_stamp.modified);
    assert_ne!(rewritten_stamp, first_stamp);
    let second = manager.read_image(&path, &JobContext::new()).unwrap();
    assert_ne!(first.generation(), second.generation());
    assert_eq!(second.raster().pixels, [84, 84, 84, 255]);
    assert_eq!(manager.cache_stats().physical_reads, 2);
    assert_eq!(manager.cache_stats().decodes, 2);
}

#[test]
fn same_size_timestamp_preserved_change_during_stream_read_is_rejected() {
    let directory = Directory::new();
    let path = directory.path("changing.tga");
    let original = encoded(&path, 42, CommonRasterFormat::Tga);
    let original_modified = fs::metadata(&path).unwrap().modified().unwrap();
    let original_permissions = fs::metadata(&path).unwrap().permissions();
    let changed_permissions = {
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(!permissions.readonly());
        permissions
    };
    let manager = IoManager::new(config()).unwrap();
    let result = manager.with_reader(&path, 4096, &JobContext::new(), |file| {
        let mut header = [0_u8; 18];
        file.read_exact(&mut header)?;
        let replacement = encoded(&path, 84, CommonRasterFormat::Tga);
        assert_eq!(replacement.len(), original.len());
        File::options()
            .write(true)
            .open(&path)?
            .set_times(FileTimes::new().set_modified(original_modified))?;
        fs::set_permissions(&path, changed_permissions.clone())?;
        Ok(())
    });
    fs::set_permissions(&path, original_permissions).unwrap();
    assert!(matches!(result, Err(IoError::ChangedDuringRead)));
}

#[test]
fn shared_cache_hits_keep_pinned_bytes_and_derived_pixels_charged() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    let bytes = png(&path, 20);
    let manager = IoManager::new(config()).unwrap();
    let context = JobContext::new();
    let first = manager.read_image(&path, &context).unwrap();
    let second = manager.read_image(&path, &context).unwrap();
    assert_eq!(first.generation(), second.generation());
    assert_eq!(first.raster().pixels, [20, 20, 20, 255]);
    assert_eq!(manager.cache_stats().physical_reads, 1);
    assert_eq!(manager.cache_stats().decodes, 1);
    assert_eq!(context.progress().read_completed, 1);
    assert_eq!(context.progress().loaded, 2);
    let lease = first.reserve_derived(16).unwrap();
    let retained_lease = lease.clone();
    manager.clear_cache();
    assert_eq!(manager.cache_stats().encoded_bytes, bytes.len() as u64);
    assert_eq!(manager.cache_stats().decoded_bytes, 20);
    drop((first, second));
    assert_eq!(manager.cache_stats().encoded_bytes, 0);
    assert_eq!(manager.cache_stats().decoded_bytes, 16);
    assert_eq!(manager.cache_stats().images, 1);
    drop(lease);
    assert_eq!(manager.cache_stats().decoded_bytes, 16);
    drop(retained_lease);
    assert_eq!(manager.cache_stats().decoded_bytes, 0);
    assert_eq!(manager.cache_stats().images, 0);
}

#[test]
fn retained_decoded_raster_requires_exact_manager_generation_and_allocation() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 20);
    let manager = IoManager::new(config()).unwrap();
    let first = manager.read_image(&path, &JobContext::new()).unwrap();
    let capability = first.reserve_derived(16).unwrap();
    let cached = manager.read_image(&path, &JobContext::new()).unwrap();
    let retained = manager
        .retain_decoded_raster(&capability, &cached)
        .expect("same decoded allocation must be reusable");
    assert_eq!(retained.info(), first.raster().info);
    assert_eq!(retained.pixels(), first.raster().pixels);
    assert_eq!(retained.pixels().as_ptr(), first.raster().pixels.as_ptr());

    let other = IoManager::new(config()).unwrap();
    let same_generation = other.read_image(&path, &JobContext::new()).unwrap();
    assert_eq!(same_generation.generation(), first.generation());
    assert!(
        other
            .retain_decoded_raster(&capability, &same_generation)
            .is_none()
    );
    assert!(
        manager
            .retain_decoded_raster(&capability, &same_generation)
            .is_none()
    );

    manager.clear_cache();
    let reloaded = manager.read_image(&path, &JobContext::new()).unwrap();
    assert_ne!(reloaded.generation(), first.generation());
    assert!(
        manager
            .retain_decoded_raster(&capability, &reloaded)
            .is_none()
    );
    let forced = manager
        .read_image_with_reload(&path, true, &JobContext::new())
        .unwrap();
    assert!(
        manager
            .retain_decoded_raster(&capability, &forced)
            .is_none()
    );
}

#[test]
fn retained_decoded_raster_keeps_one_existing_charge_until_last_owner_drops() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 20);
    let manager = IoManager::new(config()).unwrap();
    let image = manager.read_image(&path, &JobContext::new()).unwrap();
    let capability = image.reserve_derived(16).unwrap();
    let retained = manager.retain_decoded_raster(&capability, &image).unwrap();
    assert_eq!(manager.cache_stats().decoded_bytes, 20);

    manager.clear_cache();
    drop(image);
    drop(capability);
    let pinned = manager.cache_stats();
    assert_eq!(pinned.encoded_bytes, 0);
    assert_eq!(pinned.decoded_bytes, 4);
    assert_eq!(pinned.images, 1);
    assert_eq!(retained.pixels(), [20, 20, 20, 255]);
    manager.shutdown_and_wait();
    assert_eq!(manager.cache_stats(), pinned);

    let clone = retained.clone();
    drop(retained);
    assert_eq!(manager.cache_stats(), pinned);
    drop(clone);
    assert_eq!(manager.cache_stats().decoded_bytes, 0);
    assert_eq!(manager.cache_stats().images, 0);
}

#[test]
fn sequence_render_leases_share_one_charge_and_keep_only_their_own_pixels() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 30);
    let manager = IoManager::new(config()).unwrap();
    let image = manager.read_image(&path, &JobContext::new()).unwrap();
    let source = image.reserve_derived(16).unwrap();
    let render = source.reserve_sequence_render(64).unwrap();
    assert_eq!(render.bytes(), 64);
    let charged = manager.cache_stats();
    assert_eq!(charged.decoded_bytes, 84);
    assert_eq!(charged.sequence_render_allocations, 1);
    assert_eq!(charged.sequence_render_bytes, 64);
    assert_eq!(charged.images, 1);
    let snapshot = render.clone();
    assert_eq!(manager.cache_stats(), charged);

    manager.clear_cache();
    drop((image, source));
    let retained = manager.cache_stats();
    assert_eq!(retained.encoded_bytes, 0);
    assert_eq!(retained.decoded_bytes, 64);
    assert_eq!(retained.images, 1);
    assert_eq!(retained.cached_images, 0);
    drop(render);
    assert_eq!(manager.cache_stats(), retained);

    // A reservation derived from a render lease is another allocation, not a
    // reference to its parent's pixel charge.
    let replacement = snapshot.reserve_sequence_render(32).unwrap();
    assert_eq!(manager.cache_stats().sequence_render_allocations, 2);
    drop(snapshot);
    assert_eq!(manager.cache_stats().decoded_bytes, 32);
    assert_eq!(manager.cache_stats().sequence_render_allocations, 1);
    drop(replacement);
    let released = manager.cache_stats();
    assert_eq!(released.decoded_bytes, 0);
    assert_eq!(released.sequence_render_allocations, 0);
    assert_eq!(released.sequence_render_bytes, 0);
    assert_eq!(released.images, 0);
}

#[test]
fn concurrent_sequence_render_reservations_share_the_manager_count_limit() {
    let directory = Directory::new();
    let first_path = directory.path("A001.png");
    let second_path = directory.path("B001.png");
    png(&first_path, 30);
    png(&second_path, 60);
    let manager = IoManager::new(config()).unwrap();
    let other_session = manager.clone();
    let first = manager.read_image(&first_path, &JobContext::new()).unwrap();
    let second = other_session
        .read_image(&second_path, &JobContext::new())
        .unwrap();
    let sources = [
        first.reserve_derived(1).unwrap(),
        second.reserve_derived(1).unwrap(),
    ];
    let attempts = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..16)
            .map(|index| {
                let source = &sources[index % sources.len()];
                scope.spawn(move || source.reserve_sequence_render(1))
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut renders: Vec<_> = attempts
        .into_iter()
        .filter_map(|result| match result {
            Ok(lease) => Some(lease),
            Err(IoError::ResourceBusy(_)) => None,
            Err(error) => panic!("unexpected reservation failure: {error}"),
        })
        .collect();
    assert_eq!(renders.len(), 8);
    let full = manager.cache_stats();
    assert_eq!(full.sequence_render_allocations, 8);
    assert_eq!(full.sequence_render_bytes, 8);
    assert_eq!(full.images, 2);
    assert_eq!(full, other_session.cache_stats());

    let render = renders.pop().unwrap();
    let snapshot = render.clone();
    drop(render);
    assert!(matches!(
        sources[0].reserve_sequence_render(1),
        Err(IoError::ResourceBusy(_))
    ));
    assert_eq!(manager.cache_stats(), full);
    drop(snapshot);
    renders.push(sources[1].reserve_sequence_render(1).unwrap());
    assert_eq!(manager.cache_stats(), full);
    drop(renders);
    assert_eq!(manager.cache_stats().sequence_render_allocations, 0);
    assert_eq!(manager.cache_stats().sequence_render_bytes, 0);
}

#[test]
fn sequence_render_byte_limit_rejects_invalid_and_aggregate_oversize_atomically() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 30);
    let limit = 128 * 1024 * 1024;
    let manager = IoManager::new(IoConfig {
        max_decoded_bytes: limit + 64,
        ..config()
    })
    .unwrap();
    let image = manager.read_image(&path, &JobContext::new()).unwrap();
    let source = image.reserve_derived(16).unwrap();
    // These are reservations only; the test does not allocate a large payload.
    let full = source.reserve_sequence_render(limit).unwrap();
    let charged = manager.cache_stats();
    assert_eq!(charged.sequence_render_allocations, 1);
    assert_eq!(charged.sequence_render_bytes, limit);
    assert_eq!(charged.decoded_bytes, limit + 20);
    assert!(matches!(
        source.reserve_sequence_render(0),
        Err(IoError::InvalidInput(_))
    ));
    for bytes in [limit + 1, u64::MAX] {
        assert!(matches!(
            source.reserve_sequence_render(bytes),
            Err(IoError::LimitExceeded(_))
        ));
    }
    assert!(matches!(
        source.reserve_sequence_render(1),
        Err(IoError::ResourceBusy(_))
    ));
    assert_eq!(manager.cache_stats(), charged);
    drop(full);
    let first = source.reserve_sequence_render(limit / 2).unwrap();
    let second = source.reserve_sequence_render(limit / 2).unwrap();
    let charged = manager.cache_stats();
    assert_eq!(charged.sequence_render_allocations, 2);
    assert_eq!(charged.sequence_render_bytes, limit);
    assert!(matches!(
        source.reserve_sequence_render(1),
        Err(IoError::ResourceBusy(_))
    ));
    assert_eq!(manager.cache_stats(), charged);
    drop((first, second));
    assert_eq!(manager.cache_stats().sequence_render_bytes, 0);
    assert_eq!(manager.cache_stats().decoded_bytes, 20);
}

#[test]
fn sequence_render_reservations_are_also_charged_to_the_decoded_limit() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 30);
    let manager = IoManager::new(IoConfig {
        max_decoded_bytes: 64,
        ..config()
    })
    .unwrap();
    let image = manager.read_image(&path, &JobContext::new()).unwrap();
    let source = image.reserve_derived(16).unwrap();
    let render = source.reserve_sequence_render(44).unwrap();
    let full = manager.cache_stats();
    assert_eq!(full.decoded_bytes, 64);
    assert_eq!(full.sequence_render_bytes, 44);
    assert!(matches!(
        source.reserve_sequence_render(1),
        Err(IoError::ResourceBusy(_))
    ));
    assert!(matches!(
        image.reserve_derived(1),
        Err(IoError::ResourceBusy(_))
    ));
    assert!(matches!(
        source.reserve_sequence_render(65),
        Err(IoError::LimitExceeded(_))
    ));
    assert_eq!(manager.cache_stats(), full);
    drop(source);
    let second = render.reserve_sequence_render(16).unwrap();
    assert_eq!(manager.cache_stats().decoded_bytes, 64);
    assert_eq!(manager.cache_stats().sequence_render_bytes, 60);
    drop((render, second));
    assert_eq!(manager.cache_stats().decoded_bytes, 4);
    assert_eq!(manager.cache_stats().sequence_render_bytes, 0);
}

#[test]
fn sequence_render_admission_preflights_eviction_then_reclaims_the_lru() {
    let directory = Directory::new();
    let paths: Vec<_> = (0..3)
        .map(|index| directory.path(&format!("A{index}.bmp")))
        .collect();
    for (index, path) in paths.iter().enumerate() {
        encoded(path, index as u8, CommonRasterFormat::Bmp);
    }
    let manager = IoManager::new(IoConfig {
        max_decoded_bytes: 32,
        ..config()
    })
    .unwrap();
    let image = manager.read_image(&paths[0], &JobContext::new()).unwrap();
    let source = image.reserve_derived(16).unwrap();
    drop(manager.read_image(&paths[1], &JobContext::new()).unwrap());
    drop(manager.read_image(&paths[2], &JobContext::new()).unwrap());
    let before = manager.cache_stats();
    assert_eq!(before.decoded_bytes, 28);
    assert_eq!(before.cached_images, 3);
    // Only eight bytes can be evicted. A failed request must not evict either
    // image and then discover that the pinned source still prevents admission.
    assert!(matches!(
        source.reserve_sequence_render(13),
        Err(IoError::ResourceBusy(_))
    ));
    assert_eq!(manager.cache_stats(), before);
    let render = source.reserve_sequence_render(8).unwrap();
    let admitted = manager.cache_stats();
    assert_eq!(admitted.decoded_bytes, 32);
    assert_eq!(admitted.cached_images, 2);
    assert_eq!(admitted.evictions, before.evictions + 1);
    assert_eq!(admitted.sequence_render_allocations, 1);
    assert_eq!(admitted.sequence_render_bytes, 8);
    drop(render);
    drop(manager.read_image(&paths[2], &JobContext::new()).unwrap());
    assert_eq!(manager.cache_stats().physical_reads, 3);
    drop(manager.read_image(&paths[1], &JobContext::new()).unwrap());
    assert_eq!(manager.cache_stats().physical_reads, 4);
}

#[test]
fn sequence_render_budgets_belong_to_independent_managers() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 30);
    let first_manager = IoManager::new(config()).unwrap();
    let second_manager = IoManager::new(config()).unwrap();
    let first_image = first_manager.read_image(&path, &JobContext::new()).unwrap();
    let second_image = second_manager
        .read_image(&path, &JobContext::new())
        .unwrap();
    let first_source = first_image.reserve_derived(1).unwrap();
    let second_source = second_image.reserve_derived(1).unwrap();
    let first: Vec<_> = (0..8)
        .map(|_| first_source.reserve_sequence_render(1).unwrap())
        .collect();
    let second: Vec<_> = (0..8)
        .map(|_| second_source.reserve_sequence_render(1).unwrap())
        .collect();
    assert!(matches!(
        first_source.reserve_sequence_render(1),
        Err(IoError::ResourceBusy(_))
    ));
    assert!(matches!(
        second_source.reserve_sequence_render(1),
        Err(IoError::ResourceBusy(_))
    ));
    drop(first);
    assert_eq!(first_manager.cache_stats().sequence_render_allocations, 0);
    assert_eq!(second_manager.cache_stats().sequence_render_allocations, 8);
    drop(second);
    assert_eq!(second_manager.cache_stats().sequence_render_allocations, 0);
}

#[test]
fn sequence_render_leases_remain_memory_only_after_shutdown_and_owner_release() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 30);
    let manager = IoManager::new(config()).unwrap();
    let image = manager.read_image(&path, &JobContext::new()).unwrap();
    let source = image.reserve_derived(16).unwrap();
    manager.clear_cache();
    drop(image);
    manager.shutdown_and_wait();
    assert!(matches!(manager.submit(|_| Ok(())), Err(IoError::Shutdown)));
    let render = source.reserve_sequence_render(32).unwrap();
    assert_eq!(manager.cache_stats().decoded_bytes, 48);
    assert_eq!(manager.cache_stats().sequence_render_allocations, 1);
    drop(manager);
    let replacement = source.reserve_sequence_render(64).unwrap();
    assert_eq!(replacement.bytes(), 64);
    drop((source, render));
    assert_eq!(replacement.reserve_sequence_render(16).unwrap().bytes(), 16);
}

#[test]
fn lru_and_pinned_admission_obey_small_injected_budgets() {
    let directory = Directory::new();
    let paths: Vec<_> = (1..=3)
        .map(|index| directory.path(&format!("A{index}.bmp")))
        .collect();
    for (index, path) in paths.iter().enumerate() {
        encoded(path, index as u8, CommonRasterFormat::Bmp);
    }
    let manager = IoManager::new(IoConfig {
        max_images: 2,
        max_decoded_bytes: 8,
        ..config()
    })
    .unwrap();
    let context = JobContext::new();
    drop(manager.read_image(&paths[0], &context).unwrap());
    drop(manager.read_image(&paths[1], &context).unwrap());
    drop(manager.read_image(&paths[0], &context).unwrap());
    drop(manager.read_image(&paths[2], &context).unwrap());
    assert_eq!(manager.cache_stats().physical_reads, 3);
    drop(manager.read_image(&paths[1], &context).unwrap());
    assert_eq!(manager.cache_stats().physical_reads, 4);
    assert!(manager.cache_stats().images <= 2);
    let first = manager.read_image(&paths[0], &context).unwrap();
    let second = manager.read_image(&paths[1], &context).unwrap();
    assert!(matches!(
        manager.read_image(&paths[2], &context),
        Err(IoError::ResourceBusy(_))
    ));
    assert_eq!(manager.cache_stats().decoded_bytes, 8);
    drop((first, second));
    drop(manager.read_image(&paths[2], &context).unwrap());
}

#[test]
fn hardlinks_share_identity_cache_and_per_file_lock_but_distinct_files_run_concurrently() {
    let directory = Directory::new();
    let first_path = directory.path("A001.png");
    let alias_path = directory.path("alias.png");
    png(&first_path, 50);
    fs::hard_link(&first_path, &alias_path).unwrap();
    let other_path = directory.path("B001.png");
    png(&other_path, 90);
    let manager = IoManager::new(config()).unwrap();
    let first = manager.read_image(&first_path, &JobContext::new()).unwrap();
    let alias = manager.read_image(&alias_path, &JobContext::new()).unwrap();
    assert_eq!(first.identity(), alias.identity());
    assert_eq!(manager.cache_stats().physical_reads, 1);
    drop((first, alias));

    let (entered, received) = mpsc::channel();
    let (release_first, wait_first) = mpsc::channel();
    let first_manager = manager.clone();
    let first_entered = entered.clone();
    let first_job = manager
        .submit(move |context| {
            first_manager.with_reader(&first_path, 4096, &context, |_| {
                first_entered.send(1).unwrap();
                wait_first.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            })
        })
        .unwrap();
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
    let alias_manager = manager.clone();
    let alias_entered = entered.clone();
    let alias_job = manager
        .submit(move |context| {
            alias_manager.with_reader(&alias_path, 4096, &context, |_| {
                alias_entered.send(2).unwrap();
                Ok(())
            })
        })
        .unwrap();
    let other_manager = manager.clone();
    let other_job = manager
        .submit(move |context| {
            other_manager.with_reader(&other_path, 4096, &context, |_| {
                entered.send(3).unwrap();
                Ok(())
            })
        })
        .unwrap();
    // The unrelated file can enter while the hardlink alias remains serialized.
    let first_completion = received.recv_timeout(Duration::from_secs(5));
    release_first.send(()).unwrap();
    assert_eq!(first_completion.unwrap(), 3);
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
    wait(&first_job).unwrap();
    wait(&alias_job).unwrap();
    wait(&other_job).unwrap();
}

#[test]
fn writes_cancel_before_publication_invalidate_cache_and_leave_no_partial_file() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    let before = png(&path, 20);
    let manager = IoManager::new(config()).unwrap();
    let old = manager.read_image(&path, &JobContext::new()).unwrap();
    let cancel = JobContext::new();
    assert!(matches!(
        manager.write_atomic(&path, &cancel, |file| {
            file.write_all(b"incomplete")?;
            cancel.cancel();
            Ok(())
        }),
        Err(IoError::Cancelled)
    ));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    let panic_manager = manager.clone();
    let panic_path = path.clone();
    let panicked = manager
        .submit(move |context| {
            panic_manager.write_atomic(&panic_path, &context, |file| {
                file.write_all(b"incomplete before panic")?;
                panic!("injected writer panic");
            })
        })
        .unwrap();
    assert!(matches!(wait(&panicked), Err(IoError::WorkerPanicked)));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    let replacement_path = directory.path("replacement.png");
    let replacement = png(&replacement_path, 100);
    manager
        .write_bytes_atomic(&path, &replacement, &JobContext::new())
        .unwrap();
    let new = manager.read_image(&path, &JobContext::new()).unwrap();
    assert_eq!(old.raster().pixels, [20, 20, 20, 255]);
    assert_eq!(new.raster().pixels, [100, 100, 100, 255]);
    assert_ne!(old.identity(), new.identity());
    assert!(matches!(
        manager.write_new_atomic(&path, &JobContext::new(), |file| {
            file.write_all(b"wrong")?;
            Ok(())
        }),
        Err(IoError::Io(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), replacement);
}

#[test]
fn sequence_discovery_ignores_digit_width_and_retains_seed_with_bounded_selection() {
    let directory = Directory::new();
    for name in [
        "A0002x.png",
        "A10x.png",
        "A1x.png",
        "A3x.bmp",
        "a05X.tga",
        "A4y.png",
        "nodigit.png",
    ] {
        fs::write(directory.path(name), b"fixture").unwrap();
    }
    let manager = IoManager::new(config()).unwrap();
    let discovered = manager
        .discover_sequence(&directory.path("A10x.png"), &JobContext::new())
        .unwrap();
    let names: Vec<_> = discovered
        .paths
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["A1x.png", "A0002x.png", "A3x.bmp", "a05X.tga", "A10x.png"]
    );
    assert_eq!(discovered.seed_index, 4);
    assert!(!discovered.truncated);
    for index in 0..1_010 {
        fs::write(directory.path(&format!("B{index}.png")), b"fixture").unwrap();
    }
    let discovered = manager
        .discover_sequence(&directory.path("B1009.png"), &JobContext::new())
        .unwrap();
    assert_eq!(discovered.paths.len(), 1_000);
    assert!(discovered.truncated);
    assert_eq!(
        discovered.paths[discovered.seed_index].file_name().unwrap(),
        "B1009.png"
    );
    assert_eq!(discovered.paths[0].file_name().unwrap(), "B10.png");
}

#[test]
fn missing_nested_path_identity_and_owned_temporary_copy_are_bounded() {
    let directory = Directory::new();
    let manager = IoManager::new(config()).unwrap();
    let missing = directory.path("not-created/subfolder/image.png");
    assert!(!manager.exists(&missing, &JobContext::new()).unwrap());
    let identity = manager.resolve_identity(&missing).unwrap();
    assert!(!identity.1);
    let equivalent = directory.path("not-created/other/../subfolder/image.png");
    assert_eq!(identity, manager.resolve_identity(&equivalent).unwrap());
    #[cfg(windows)]
    assert_eq!(
        identity,
        manager
            .resolve_identity(&directory.path("NOT-CREATED/SUBFOLDER/IMAGE.PNG"))
            .unwrap()
    );
    let source = directory.path("A001.png");
    let encoded = png(&source, 40);
    let source_image = manager.read_image(&source, &JobContext::new()).unwrap();
    drop(source_image);
    let temporary = manager
        .create_temporary_directory("batch-preview", &JobContext::new())
        .unwrap();
    let copied = temporary.path().join("input.png");
    assert_eq!(
        manager
            .copy_file(&source, &copied, 4096, &JobContext::new())
            .unwrap(),
        encoded.len() as u64
    );
    drop(manager.read_image(&copied, &JobContext::new()).unwrap());
    assert_eq!(manager.cache_stats().cached_images, 2);
    let path = temporary.path().to_path_buf();
    temporary.cleanup().unwrap();
    assert!(!path.exists());
    assert_eq!(manager.cache_stats().cached_images, 1);
    let cancelled_copy = directory.path("cancelled.png");
    assert!(matches!(
        manager.copy_file_with_cancel(&source, &cancelled_copy, 4096, &JobContext::new(), || true),
        Err(IoError::Cancelled)
    ));
    assert!(!cancelled_copy.exists());
}

#[test]
fn bounded_jobs_cancel_without_running_and_batches_publish_monotonic_counts() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 10);
    let manager = IoManager::new(IoConfig {
        worker_count: 1,
        queue_capacity: 1,
        ..config()
    })
    .unwrap();
    let (entered, wait_entered) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    let first = manager
        .submit(move |_| {
            entered.send(()).unwrap();
            wait_release.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        })
        .unwrap();
    wait_entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let ran = Arc::new(AtomicU64::new(0));
    let task_ran = Arc::clone(&ran);
    let second = manager
        .submit(move |_| {
            task_ran.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
        .unwrap();
    second.cancel();
    assert!(matches!(
        manager.submit(|_| Ok(())),
        Err(IoError::ResourceBusy(_))
    ));
    release.send(()).unwrap();
    wait(&first).unwrap();
    assert!(matches!(wait(&second), Err(IoError::Cancelled)));
    assert_eq!(ran.load(Ordering::Relaxed), 0);

    let batch = manager
        .submit_images(vec![path.clone(), path], false)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let progress = batch.poll();
        assert!(progress.loaded <= 2 && progress.completed <= 2);
        if progress.state == JobState::Completed {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "batch did not complete: {progress:?}"
        );
        std::thread::yield_now();
    }
    assert_eq!(batch.poll().loaded, 2);
    assert_eq!(batch.poll().read_completed, 1);
    assert_eq!(manager.cache_stats().decodes, 1);
    let results = batch.take_completed(2);
    assert_eq!(
        results.iter().map(|item| item.index).collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(results.iter().all(|item| item.result.is_ok()));
    assert!(batch.take_completed(2).is_empty());
    let context = JobContext::new();
    let child = context.child();
    child.record_loaded();
    child.record_read_completed();
    assert_eq!(context.progress().loaded, 0);
    assert_eq!(context.progress().read_completed, 0);
    assert_eq!(child.progress().loaded, 1);
    child.cancel();
    assert!(context.is_cancelled());
    manager.shutdown();
    assert!(matches!(manager.submit(|_| Ok(())), Err(IoError::Shutdown)));
}

#[test]
fn image_batch_yields_between_files_when_executor_queue_is_full() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 10);
    let manager = IoManager::new(IoConfig {
        worker_count: 1,
        queue_capacity: 1,
        ..config()
    })
    .unwrap();
    let locked_manager = manager.clone();
    let locked_path = path.clone();
    let (entered, wait_entered) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    let file_owner = std::thread::spawn(move || {
        locked_manager.with_file_locks(&[locked_path], &JobContext::new(), |_| {
            entered.send(()).unwrap();
            wait_release.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        })
    });
    wait_entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let batch = manager
        .submit_images(vec![path.clone(), path.clone(), path], false)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while batch.poll().reading == 0 {
        assert!(Instant::now() < deadline, "image lane did not start");
        std::thread::yield_now();
    }
    let batch_context = batch.context();
    let queued = manager
        .submit(move |_| Ok(batch_context.progress().completed))
        .unwrap();
    assert!(matches!(
        manager.submit(|_| Ok(())),
        Err(IoError::ResourceBusy(_))
    ));
    release.send(()).unwrap();
    file_owner.join().unwrap().unwrap();
    assert_eq!(wait(&queued).unwrap(), 1);
    while batch.poll().state != JobState::Completed {
        assert!(Instant::now() < deadline, "image lane did not resume");
        std::thread::yield_now();
    }
    assert_eq!(batch.take_completed(3).len(), 3);
    assert_eq!(batch.poll().loaded, 3);
}

#[test]
fn byte_and_pixel_limits_are_checked_before_allocation() {
    let directory = Directory::new();
    let path = directory.path("A001.png");
    png(&path, 10);
    let manager = IoManager::new(IoConfig {
        // Four resident RGBA bytes fit, but PNG's second four-byte source
        // allocation must also be reserved before decoding begins.
        max_decoded_bytes: 4,
        ..config()
    })
    .unwrap();
    assert!(matches!(
        manager.read_image(&path, &JobContext::new()),
        Err(IoError::LimitExceeded(_))
    ));
    assert_eq!(manager.cache_stats().decoded_bytes, 0);
    assert_eq!(manager.cache_stats().decodes, 0);
    manager.clear_cache();
    assert!(matches!(
        manager.read_bytes(&path, 1, &JobContext::new()),
        Err(IoError::LimitExceeded(_))
    ));
    assert_eq!(manager.cache_stats().encoded_bytes, 0);
    let streamed = manager
        .with_reader(&path, 4096, &JobContext::new(), |file| {
            let mut magic = [0_u8; 8];
            file.read_exact(&mut magic)?;
            Ok(magic)
        })
        .unwrap();
    assert_eq!(streamed, *b"\x89PNG\r\n\x1a\n");
}
