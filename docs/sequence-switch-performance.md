# Sequence switching performance

This record covers the user-approved response-time work for `SEQ-001`,
`PERF-001` and `IO-003`: selecting already loaded, unedited 1754×1240
images with the sequence pane's Left/Right keys. The current implementation is
native v32, replay epoch 27 and exact-current ABI v32. The canonical
`revision-max` expression and existing benchmark workloads/envelopes are unchanged.

## Cause and implementation

The measured CPU cost was repeated image work. Fit arithmetic was negligible.
Switching discarded the useful display
tiles; document-info queries recomputed a whole-image checksum; repeated
sequence-pane queries generated the same thumbnails again. The frontend
also published an intermediate view and issued empty preview-clear renders.

The implementation caches checksums with the raster's existing mutation/COW
boundaries, retains immutable thumbnails and complete editable states, and keeps
bounded pristine-source CPU/GPU display caches. Both display caches are limited
to 64 images and 1 GiB, including the active source, within the application-wide
decoded/GPU budgets. Last-lease accounting includes snapshots and prepared
results. The full catalog can be prepared in the background; stale results are rejected.

Resident replacement exchanges the outgoing and target complete Core states on
the Core owner queue without file I/O, replay, decode or full reconstruction.
Dirty autosave publication follows as an independent job. The UI preserves directional
intents, publishes only the final view, preserves zoom/pan/flip for same-size
images, and does not rebuild an unchanged thumbnail list. Edited/recovered
documents continue through their existing transaction and persistence routes.
A presentation epoch prevents painting a newly committed document while the
Canvas still shows the previous image. This is Windows presentation metadata,
not a new Core cache key or serialized version.

The Renderer retains a pending latest frame when DXGI is not ready and waits
for ready surfaces or new work. Explicit accepted render requests retain their
bounded credits. Empty preview cancellation does not request another frame.
The first edit transfers the active pristine bitmap bank to the ordinary
incremental cache without duplicating it; only changed tiles need uploading.
Repeated-view activation checks the actual Canvas route, including newly
created views. Rejected stale editor commands refresh their captured session's
presentation without retrying the edit or adding work to successful navigation.
See [architecture](architecture.md) and [FFI ownership](ffi.md) for the exact
lifetime, publication, recovery and input-fence rules.

## CPU before/after

Same Parallels ARM64 VM (four virtual CPUs, 8 GiB), Windows build 26200,
Rust 1.97.1 / LLVM 22.1.6, x64 Release under emulation, static CRT.
Baseline commit: `159b44e27f63a024f8654ebb80cdc83599335dec`.

The original diagnostic fixture is decoded-memory RGBA8, two distinct COW
payloads and a 32-entry catalog at 1754×1240. It is **not the user's TGA**.
There is one discarded process warm-up, then five independent processes,
each with two internal warm-ups and nine measured switches. Values below
are medians of process medians.

| CPU stage | Before (ms) | After (ms) |
| --- | ---: | ---: |
| Activate, including one document-info query | 14.542500 | 0.112667 |
| Separate document-info query | 13.638542 | 0.000166 |
| First snapshot after switching | 89.182792 | 0.078708 |
| Fit arithmetic only | 0.001292 | 0.000167 |
| Repeated snapshot of the same cell | 0.145792 | 0.066750 |
| One sequence-cell/thumbnail query | 0.447333 | 0.000709 |
| Three full catalog query passes | 19.937959 | 0.048708 |

These independent intervals must **not** be summed into an end-to-end result.
The final row is a Core-call surrogate, not Win32 list creation time.
The fingerprint remains `21524bfbc76a2ba5`; a revisited image now reuses
560/560 tiles, instead of 0/560. No quality or resolution reduction is used.

## Final product-path measurements

After the final correctness fixes and rebuild, one process warm-up and five
independent measured processes passed on the same x64 Release VM. The window
was 1280×960 device pixels at 192 DPI. Both the
display mode and DWM reported approximately 120 Hz (8.333292 ms per refresh).
All 640 measured switches were confirmed in the foreground. Each process used
the normal importer and keyboard/Canvas path with three generated TGA files.

The table shows the median of the five process p95 values, in milliseconds.
The stages beginning at keyboard dispatch overlap and must not be summed.

| Stage | A–B–A | A–B–C–B–A |
| --- | ---: | ---: |
| Keyboard handler return | 0.440083 | 0.600542 |
| Final snapshot submitted | 1.180208 | 1.351917 |
| First successful Present returned | 2.722458 | 2.870625 |
| Submission to frame readiness | 0.182125 | 0.160583 |
| Direct2D drawing | 1.597000 | 1.652667 |
| Present API call | 0.126667 | 0.129875 |

Across all ten scenario/process pairs, p95 ranged from 2.625917 to 3.558583 ms;
the largest individual sample was 4.771167 ms. This meets the proposed
two-refresh-interval target (16.667 ms at 120 Hz) for this measured workload.
Every process recorded 128 successful Presents for 128 warm switches, exactly
one snapshot per switch, zero physical reads, decodes, uploads and frame-wait
timeouts, and all 50 accepted reversal-burst intents committed without loss.
The three retained sources occupied 26,099,520 bytes in each CPU/GPU cache.

These are cached, unedited revisits. Initial import, cache misses, recovery and
arbitrary files are not covered by the warm latency result. Cold preparation
steps are retained separately in the raw logs. First-time decode and upload
remain asynchronous work; the cache has a bounded working set.

Earlier intermediate diagnostics, including approximately 203 ms p95 and an
initial hidden-parent visibility failure, remain in the evidence. Foreground
state during the earlier 203 ms samples was not recorded, so its cause has not
been isolated to background scheduling or a driver. A later test that did not
request foreground nevertheless became foreground before its measured steps;
it is not a controlled background comparison. The final series above records
actual foreground state at every step. No VSync, timing envelope or image
quality setting was relaxed to obtain the result.

A later default full-suite run recorded zero foreground samples for all 128
switches. Its AB/ABC p95 values were 295.185417/205.742458 ms, with 44/37 real
frame-readiness timeouts, while the cache and input-delivery gates still passed.
That result is retained separately and does not meet the foreground latency
target. It establishes a slow background case, not a diagnosis of the earlier
unobserved run or proof of a particular OS/driver cause.

## 2026-09-02 COW construction follow-up

The current native v32/replay epoch 27 implementation adds an exact-provenance
construction optimization for sidecar-less Sequence targets. Sequence import
materializes each source once into its tiled COW representation, retains every
source tile set and thumbnail under the decoded budget, and immediately releases
the manager's dense decoded-cache owner. A warm switch clones the catalog tile
map only when manager, normalized path, complete file stamp, decode generation,
format and metadata all match. Every mismatch takes the ordinary owned import.
This classification is used only for the optimization choice and test-support
counters; pair authority and the existing resolver-proven pristine
re-registration do not depend on it.

The Core contract records zero dense-copy bytes and zero full tile
materialization on the managed path, exact 16-byte/four-pixel work on the forced
fallback, and zero warm AssetId hashing on A–B–A. Both paths have byte-identical
native output and identical document/Genesis/asset/editor/history semantics.
The managed path remains available after decoded-cache clear and manager worker
shutdown because the catalog owns the tiled source and its derived-budget lease;
dense canonical bytes are materialized only for persistence or another explicit
linear export.

The same update adds an application-wide LRU for fully replayed and pair-validated
sidecar targets. Its configurable byte budget is 0–1024 MiB (1 GiB default),
it retains at most 64 clean targets, and an exact hit shares immutable document,
asset and tile ownership through COW. Both normalized pair paths and both complete
file stamps are part of the key. Changed stamps invalidate the old pair entry;
recovered, dirty, pending, missing-companion and unproven targets are never
published. The byte figure is a conservative logical cache weight, not a promise
to count shared COW pages twice. A first sidecar visit still performs the required
read, replay and raster-pair validation; only subsequent exact visits skip them.

A single current x64 Release product run at 1280×960, 192 DPI and 60 Hz passes
both 64-switch scenarios in 39.02 seconds. Every measured switch is foreground;
warm reads, decodes and uploads are zero, snapshots remain one per step, and all
50 accepted reversal intents commit. A–B–A / A–B–C–B–A handler p50 is
12.303/12.519 ms and first-Present p95 is 260.862/249.704 ms. This is current
semantic and diagnostic evidence only: it neither replaces the approved
five-process series above nor changes its workload or envelope.

## 2026-09-03 resident-state and frame-latency follow-up

The current sequence attachment installs up to 64 complete inactive editable
Core states alongside the active Core. Graphs, assets, and tile maps share their
immutable backing through COW; the decoded dense owner is released after tile
adoption. A resident hit puts the outgoing state in the same bank and takes the
target state without starting a `SequenceSwitch` file job. A dirty outgoing
state starts durable autosave as a separate operation after the visible switch.
The status bar therefore does not publish `セルを読み込んでいます` for this path.

CPU composition and renderer GPU source caches now each admit 64 sources up to
1 GiB. Core preparation covers the full catalog on bounded workers, snapshot
accessors expose completed inactive sources without payload scans/copies, and
the renderer pre-uploads them in the background and rebuilds them after device
loss. The renderer's readiness wait was reduced from 100 ms to 4 ms; a visible
surface receives a 250-ms transient-occlusion retry window. Canvas/selection
publication remains immediate, while only secondary pane/menu projection is
coalesced for 75 ms.

One x64 Release run of the unchanged three-TGA product workload retained zero
warm reads, decodes, and uploads, one snapshot per step, three CPU/GPU sources
(26,099,520 bytes), zero frame-latency timeouts, and no lost accepted intents.
The measured switch CPU stages were:

| Stage | A–B–A p50 / p95 / max (ms) | A–B–C–B–A p50 / p95 / max (ms) |
| --- | ---: | ---: |
| Keyboard handler return | 1.303 / 2.011 / 3.108 | 1.285 / 2.483 / 3.886 |
| Final snapshot submitted | 1.101 / 1.492 / 2.543 | 1.134 / 2.095 / 3.472 |

Successful-Present p95 in that desktop was approximately 324/377 ms. It does
not satisfy the existing presentation target and is recorded as DWM/desktop
variability rather than hidden CPU, I/O, decode, upload, or frame-wait work:
those semantic counters were all zero. The test recorded zero foreground
samples, while the Present API itself remained at 0.220/0.192 ms p95. No
workload, envelope, VSync, image quality, or acceptance counter was relaxed.

## Verification

The final source passes `cargo fmt --check`, strict workspace/all-target/
all-feature Clippy, Core rustdoc with warnings denied, and 801 Rust tests plus
one intentional ignored Release-only gate. All nine unchanged quick Core
benchmark scenarios also pass.

The current x64 Release build completes its static-CRT check and portable
ZIP/unsigned MSIX generation. Its focused sequence-performance test passes.
The current x64 Debug full suite also passes, including both localized product
smokes:

| Configuration | Tests | English GUI | Japanese GUI |
| --- | --- | ---: | ---: |
| x64 Debug | 48/48, 1179.46 s | 498.16 s | 496.12 s |
| x64 Release | focused sequence 1/1, 54.21 s | not rerun | not rerun |

The strengthened x64 Debug Renderer test passes in 48.84 seconds. It prepares
all 35 synthetic catalog sources, observes 34 inactive sources after the active
source's first edit, and covers the shared 64-source/1-GiB ceiling,
current-source exclusion, one changed-tile upload, and device-loss reconstruction.

Earlier candidate failures remain recorded: the statusbar source gate needed
the owned completion-mailbox drain; a FIT fixture needed a visible Canvas;
new-view activation needed an actual-route check; stale editor rejection
needed explicit presentation refresh. The pinned-selection fixtures now
respect the command's existing zero success return. A later 45/46 run exposed
an immediate-Present assumption in the stroke-preview fixture. That fixture
now observes one accepted render's actual presentation within the existing
wait bound, without requesting another render or removing its checksum,
dirty-state, revision and frame-count assertions. The final full runs above
include all these regression checks.

An earlier ARM64 Debug full-suite performance run records AB/ABC p95
5.133292/5.212375 ms with 128 foreground samples and the same zero warm
read/decode/upload gates. It is a supplemental single process, not the
five-process Release acceptance series above. ARM64 and x64 Release full-suite,
physical keyboard/scanout timing and the user's actual TGA files were not
rerun for the resident-state follow-up.

## Existing performance contracts

All nine unchanged Core quick scenarios retain their checksums and semantic
counters. ARM64 quick pan/zoom measured 1.076875 ms in the first five-process
batch, above the existing 1.05 ms upper edge, then 1.024583 ms in the independent
five-process remeasurement. Thus the upper-bound breach did not reproduce
under the existing regression rule. Dirty-tile rebuild medians were
1.854959 and 1.884125 ms, within its 1.8–2.4 ms envelope.

The separate Release-only InkScript quick contract also passed one warm-up
and five measured processes with checksum `b65373bdba27b215`.
These ARM64 InkScript timings are diagnostic, not acceptance against its
Ryzen/x64 envelope.

## Reproduction and evidence

[The machine-readable record](sequence-switch-performance-20260828.json)
contains every Core timing sample, warm-up log, semantic fingerprint, original
before/after probe source, environment and unchanged benchmark output.
Probe source embedded in the record can be built against the corresponding
`inkpod-core` Release rlib with the recorded x64 target, optimization and
static-CRT options.

The independent product-path test is built with the normal application:

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
