#![no_main]

use libfuzzer_sys::fuzz_target;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(1);

fuzz_target!(|bytes: &[u8]| {
    let path = std::env::temp_dir().join(format!(
        "inkpod-native-core-fuzz-{}-{}.inkpod",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    if std::fs::write(&path, bytes).is_ok() {
        let mut core = inkpod_core::Core::new();
        if core.open(&path).is_ok() {
            let _ = core.persistence_info();
            let _ = core.compaction_plan();
            let _ = core.verify_journal_replay();
            let _ = core.collect_unreferenced_assets();
        }
        let _ = std::fs::remove_file(path);
    }
});
