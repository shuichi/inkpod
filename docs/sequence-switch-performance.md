# Sequence switching performance

This is the current reproduction procedure for `SEQ-001`, `PERF-001` and `IO-003`,
not a chronological optimization report. SPEC owns the response-time goals;
[architecture.md](architecture.md#bounded-sequence-source-render-caches) owns cache,
residency and publication details; [compatibility.md](compatibility.md) records
current evidence and remaining gaps. Existing [benchmark envelopes](core-benchmark-baseline.md)
and the canonical `revision-max` contract remain independent and unchanged.

## Observable contract

Measure already loaded, unedited 1754×1240 images through the real keyboard
dispatcher, Sequence pane, Core, Renderer and first successful Present of the
correct image. Warm navigation requires zero decode, full-image checksum scans,
full composition, new thumbnails and unchanged-content uploads, with one final
snapshot. Preserve directional intents and exact source/presentation identity.

Record p50/p95/p99/max and semantic counters. Targets are UI-handler p95 <= 1 ms,
snapshot submission p95 <= 4 ms and first correct successful Present p95 <= two
refresh intervals. These goals do not replace or widen an approved envelope.
Cold/unprepared/evicted/save-waiting paths have separate results. Edited/recovered
and first-sidecar admission are not inferred from warm pristine navigation.

## Reproduction

Build and run the normal application:


```text
cmake --preset windows-x64-release
cmake --build --preset windows-x64-release
ctest --preset windows-x64-release -R "^inkpod_windows_sequence_performance$" -V
cargo bench --package inkpod-core --bench core_workflows -- --quick
cargo test --release --package inkpod-core --lib script::tests::approved_quick_performance_contract -- --ignored --exact --nocapture --test-threads=1
```

The sequence test generates three original uncompressed 24-bit TGA fixtures
and uses the normal importer, keyboard dispatcher, Core, Renderer and
successful Present path. It records cold steps separately, then 64 A–B–A
and 64 A–B–C–B–A switches, followed by a 50-intent direction-reversal burst.
Warm assertions require one snapshot per step, no disk reads/decode/GPU
uploads, stable thumbnail/catalog storage, exact source/epoch identity,
bounded caches and no lost accepted navigation intents. The visible test
window is topmost. By default it does not request keyboard focus. Setting
`INKPOD_SEQUENCE_PERF_FOREGROUND=1` for the test process requests foreground
through the ordinary Windows API, records whether that request succeeded and
counts the measured steps that remained in the foreground. It restores the
previous foreground window on exit only if the test still owns foreground.
No focus-lock bypass is used.

The test logs DPI, advertised refresh rate, the DWM composition/refresh rates
and QPC frequency. For the first successful Present of each exact document
revision/presentation epoch it separates submission-to-frame-readiness,
Direct2D drawing, and the Present call. These timestamps are not overwritten
by a later repaint of the same document. No test-only GPU readback occurs in
the measured path. No user image, settings or source file is overwritten.

Present completion is not physical scanout. Hardware keyboard latency,
the user's actual TGA files, remote display and other GPU/driver environments
are not established by these measurements.
