# Rust Core workflow benchmark baseline

This document defines the reproducible comparison contract for
`rust/inkpod-core/benches/core_workflows.rs`. It is a living baseline for the M3
and later refactoring milestones. Cross-host absolute wall-clock values are not
a CI gate. Routine validation uses an approved environment-specific reference
envelope for `pan_zoom_snapshot`, `dirty_tile_rebuild`, and the native
`wheel_zoom`/`drawing` performance-smoke scenarios, while semantic counters and
checksums remain hard gates on every host. Reconstructing the detached old
revision-max build is reserved for workload/harness changes, range calibration,
or an explicit boundary audit. Update a workload only when its input or
semantic output intentionally changes; changing it also requires recalibrating
the affected envelope.

## Commands and profiles

```text
cargo bench --package inkpod-core --bench core_workflows -- --quick
cargo bench --package inkpod-core --bench core_workflows
```

Both commands use the release benchmark profile and the same seven scenarios.
Quick is the bounded Linux CI profile; full increases inputs for local
before/after comparison. Neither profile reads or writes native `.inkpod`
files. Batch uses in-memory sequence cells, and its dry-run asserts that its
absent output directory remains absent.

| Parameter | Quick | Full |
| --- | ---: | ---: |
| sparse document dimensions | `MAX_RASTER_DIMENSION` square | same |
| sparse allocated tiles | 8 | 32 |
| dirty one-pixel edit + snapshot rebuild steps | 32 | 128 |
| pan/zoom view-only snapshot pairs | 2,048 | 8,192 |
| Undo/Redo edits | 12 | 48 |
| light-table document | 128 square, 3 references | 256 square, 6 references |
| vector document | 128 square, 8 closed paths/fills | 256 square, 32 closed paths/fills |
| Batch sequence | 4 cells at 16 square | 16 cells at 32 square |

## Output and assertions

Every scenario prints exactly one line with this stable schema:

```text
inkpod-core-workflows profile=<profile> scenario=<name> iterations=<n> input_items=<n> output_items=<n> reused_items=<n> document_revision=<n> history_entries=<n> successes=<n> failures=<n> checksum=<hex> elapsed_ns=<n>
```

`elapsed_ns` is observational when a run has no matching approved environment
envelope. The benchmark fails directly on semantic checksum or counter drift;
the routine procedure below additionally gates protected performance scenarios
on a matching environment. The scenario assertions cover these contracts:

| Scenario | Hard assertion |
| --- | --- |
| `sparse_snapshot` | only the deliberately allocated sparse tiles render |
| `dirty_tile_rebuild` | one edited tile rebuilds and all other tile revisions remain reusable |
| `pan_zoom_snapshot` | every alternating zoom/pan pair builds a snapshot; document revision, history, pixels, and all tile revisions remain unchanged |
| `undo_redo` | every edit is one history entry, Undo reaches the clean savepoint, and Redo restores the exact checksum |
| `light_table_composite` | all references contribute to the expected full tile grid and checksum |
| `vector_snapshot` | segment/fill counts, zero raster snapshot tiles, and rasterized pixels match |
| `batch_preview` | one invalid graph is rejected, all valid inputs dry-run successfully, and no output is generated |

The checksum is local FNV-1a over fixed-width public semantic data. It excludes
wall-clock time, allocator addresses, cache allocation order, and Batch output
paths. The expected values embedded in the benchmark are:

| Scenario | Quick checksum | Full checksum |
| --- | --- | --- |
| `sparse_snapshot` | `517ed7ae78bf0487` | `439040e0244d5773` |
| `dirty_tile_rebuild` | `9e13576def6f539b` | `a33f7534fcdd61e7` |
| `pan_zoom_snapshot` | `517ed7ae78bf0487` | `439040e0244d5773` |
| `undo_redo` | `3f1053b9fde37d35` | `a2c1a74e7f9781a3` |
| `light_table_composite` | `255ab9bad114dfdd` | `77f63d83e130185f` |
| `vector_snapshot` | `688dd42c93a71bec` | `27e6aa988b125683` |
| `batch_preview` | `f31d31fe1bb00fd7` | `6732b8b0a6565d03` |

The M4 vector checksums supersede the earlier values because the new distinct
Genesis Cell ID advances the shared stable-ID cursor before vector plane, path,
and fill IDs are allocated. The scenario inputs, iteration counts, rasterized
pixels, and routine timing envelopes are unchanged.

## Routine reference envelopes

The first approved routine range is identified as
`windows-arm64-apple-silicon-parallels-release-2026-08-05`. It applies only to
Windows build 26200 on the recorded Apple Silicon/Parallels ARM64 host, the
Release profiles and fixed workloads in this document, Rust/Cargo 1.97.1,
LLVM 22.1.6, MSVC 19.51 for the native route, and the recorded `Parallels`
power scheme. The native wheel range was captured with the Parallels Display
Adapter at 3456 x 2168 and 120 Hz, so its normalization denominator is
8,333,333.33 ns per refresh interval. A materially different host, target,
toolchain, power mode, or display mode needs its own approved range; it must not
silently reuse this one.

| Protected score | Accepted routine range | Recorded accepted median | Interpretation |
| --- | ---: | ---: | --- |
| quick `pan_zoom_snapshot`, 2,048 pairs | 0.70–1.05 ms total | 0.806 ms | Core CPU/view-cache gate |
| quick `dirty_tile_rebuild`, 32 edits | 1.8–2.4 ms total | 2.042 ms | Core incremental-drawing gate |
| full `pan_zoom_snapshot`, 8,192 pairs | 12–16 ms total | 13.575 ms | Core CPU/view-cache gate |
| full `dirty_tile_rebuild`, 128 edits | 8.5–11 ms total | 9.387 ms | Core incremental-drawing gate |
| native `drawing`, 16 strokes | 150–200 ms total | 163.196 ms | fixed burst through 16 final Presents |
| native `wheel_zoom`, 512 events | 0.95–1.10 display refresh intervals/event | approximately 1.00 | Present-paced routing gate, not a CPU microbenchmark |

Run at least one unmeasured warm-up followed by five measured runs and compare
their median with the matching row. The native command performs its fixture
warm-up internally, but the complete process is still run at least five times.
All semantic checksums, revisions, reuse/rebuild counts, payload-access counts,
samples, Presents, and queue/resource counters must pass before elapsed time is
considered. A result below the lower edge is diagnostic only: verify that work
was not skipped, then accept it when the semantic gate remains intact. If a
median exceeds the upper edge, run an independent second batch of at least five
measurements. Both batch medians must exceed the upper edge before the result is
classified as a performance regression.

For native `wheel_zoom`, divide elapsed time per event by the display refresh
interval recorded for that run. The retained 2026-08-05 measurements resolve to
approximately one refresh interval per event; this explains why their tiny
paired sign is not evidence of CPU cost. Core `pan_zoom_snapshot` remains the
CPU-sensitive zoom gate. Shared CI enforces semantic work rather than these
host-specific time ranges.

An envelope is never widened automatically. Changing one requires the range
ID, environment, complete samples, semantic counters, reason, and explicit user
approval to be recorded here. The historical A/B evidence below remains the
calibration provenance and an exceptional rebaseline tool, not a routine build
requirement.

## 2026-08-06 M5 routine acceptance

M5 retained Range ID
`windows-arm64-apple-silicon-parallels-release-2026-08-05` for the matching
Core Release workloads without changing a workload, semantic assertion,
reference envelope, or the revision-max formula. One quick run was used as the
unmeasured warm-up, followed by five measured quick and full runs. Acceptance
uses the prescribed median; isolated high samples did not move any Core median
outside its unchanged envelope.

| Protected score | Five measured samples (ns) | Median | Result |
| --- | --- | ---: | --- |
| quick `pan_zoom_snapshot` | 1,103,500; 825,167; 813,250; 809,958; 809,250 | 813,250 ns (0.813250 ms) | Within 0.70–1.05 ms |
| quick `dirty_tile_rebuild` | 1,860,291; 1,889,500; 2,132,250; 4,550,750; 2,029,417 | 2,029,417 ns (2.029417 ms) | Within 1.8–2.4 ms |
| full `pan_zoom_snapshot` | 12,747,208; 13,516,209; 12,826,959; 13,995,042; 14,786,542 | 13,516,209 ns (13.516209 ms) | Within 12–16 ms |
| full `dirty_tile_rebuild` | 10,693,167; 9,712,375; 8,982,291; 9,475,375; 13,600,708 | 9,712,375 ns (9.712375 ms) | Within 8.5–11 ms |

Every Core run retained the expected checksum, document revision, history,
input/output, reuse/rebuild, success, and failure counters. The ABI/queue change
does not alter these protected Core workloads.

The Release private native smoke also ran in five independent processes and
passed its executable semantic gate each time. Wheel samples were
8,533,981,250; 8,533,049,709; 8,551,619,916; 8,601,030,709; and 8,547,517,125 ns
(median 8,547,517,125 ns). Drawing samples were 266,622,500; 266,678,708;
266,585,375; 267,516,167; and 265,913,792 ns (median 266,622,500 ns). Each run
retained 256 tiles, 1,048,576 payload bytes, 512 wheel events/Presents, and 16
drawing strokes/544 samples/16 Presents. The current display was paced at about
60 Hz rather than the range record's 120 Hz: wheel was about one current refresh
interval per event and drawing about one current refresh interval per final
Present. Therefore the recorded 120-Hz native wall-clock envelope is not
applicable to this observation and was neither reported as a regression nor
widened. The Core portion of the environment range remains applicable and
passed above.

## 2026-08-06 M4 routine acceptance

M4 final validation reused Range ID
`windows-arm64-apple-silicon-parallels-release-2026-08-05` without changing a
workload, semantic assertion, reference envelope, or the revision-max formula.
Core used one unmeasured warm-up followed by five measured quick and full runs.
The native fixture performed its internal warm-up in each of five independent
measured Release processes. Individual samples are retained below; acceptance
uses the prescribed median, so the isolated high dirty-tile samples do not
change the result and did not require a second batch.

| Protected score | Five measured samples (ns) | Median | Result |
| --- | --- | ---: | --- |
| quick `pan_zoom_snapshot` | 889,750; 921,792; 916,791; 969,917; 992,250 | 921,792 ns (0.921792 ms) | Within 0.70–1.05 ms |
| quick `dirty_tile_rebuild` | 8,340,417; 1,994,584; 2,149,459; 1,874,625; 2,168,250 | 2,149,459 ns (2.149459 ms) | Within 1.8–2.4 ms |
| full `pan_zoom_snapshot` | 13,292,417; 13,285,625; 13,612,792; 12,934,125; 12,973,709 | 13,285,625 ns (13.285625 ms) | Within 12–16 ms |
| full `dirty_tile_rebuild` | 9,317,709; 15,880,250; 9,118,500; 9,338,166; 9,161,958 | 9,317,709 ns (9.317709 ms) | Within 8.5–11 ms |
| native `drawing` | 163,999,584; 162,555,250; 174,470,708; 168,510,875; 167,134,750 | 167,134,750 ns (167.134750 ms) | Within 150–200 ms |
| native `wheel_zoom` | 4,608,558,292; 4,266,759,958; 4,608,538,500; 4,266,352,125; 4,641,402,709 | 4,608,538,500 ns total; 1.080126211 intervals/event | Within 0.95–1.10 intervals/event |

Every Core run retained its expected checksum, document revision, history,
input/output, reuse/rebuild, success, and failure counters. Every native run
retained 256 tiles and 1,048,576 payload bytes; wheel retained 256 pairs, 512
events, 512 Presents, and 512 queue replacements; drawing retained 16 strokes,
544 samples, 16 Presents, and the expected cumulative 592 queue replacements.
No queue/resource-limit failure was published.

## 2026-08-05 canonical revision-max calibration provenance

The `pan_zoom_snapshot` workload performs 2,048 quick or 8,192 full alternating
zoom/pan pairs and builds a snapshot after every pair. It asserts that every
composed tile and renderer-facing tile revision is reused. The
`dirty_tile_rebuild` workload times 32 quick or 128 full successive pencil-pixel
document edits together with their snapshot rebuilds. Every step changes the
same allocated tile and asserts that exactly one tile is recomposed while all
other tile revisions remain reusable. The repeated workloads reduce timer and
scheduler noise without excluding canonical procedure/digest work. These are
the protected zoom and incremental-drawing scenarios. `sparse_snapshot` is
retained as a supplemental cold-composition observation.

The canonical implementation validates a cache entry using only
`max(visible plane tile revisions, selection tile revision, Light Table source
revision)`. It does not read, copy, or hash source pixels. A source guard rejects
`blake3`, `tile_data`, pixel access, checksum, digest, generation, tombstone,
epoch, and cache-state metadata in both the complete primary snapshot-validation
body and that helper. Their normalized combined source is locked so a delegated
helper cannot enter the validation call graph without an explicit audit. A
test-only composition payload counter first proves the fixture reads payload,
then remains exactly zero across 128 cache-hit wheel-style zoom snapshots while
tile revisions also remain unchanged.

The authoritative old-production baseline is detached commit `3f164db`, the
last pre-M1 revision-max implementation. Its production sources were left
unchanged; only the exact current benchmark workload and assertions were copied
into its worktree. The candidate retains that scalar formula, separates the M1
state digest from rendering, borrows changed source tiles without copying, and
prepares dirty-tile composition once per tile. This A/B therefore measures the
complete protected workloads, not a synthetic helper or an edit path with
procedure/digest work removed.

The following values were captured on Windows build 26200 ARM64, CPU reported
as Apple Silicon, Rust/Cargo 1.97.1 targeting `aarch64-pc-windows-msvc`, LLVM
22.1.6, the Release benchmark profile, and the `Parallels` power scheme. The old
and candidate binaries each received one discarded warm-up for quick and full.
Nine measured pairs then alternated old-first and candidate-first order. Samples
below are run-order `elapsed_ns`; each median is the fifth sorted sample. The
paired ratio is the median of the nine candidate/old ratios.

| Profile/scenario | Old revision-max samples | Old median | Candidate samples | Candidate median | Median change | Paired ratio median |
| --- | --- | ---: | --- | ---: | ---: | ---: |
| quick `pan_zoom_snapshot` | 1,179,334; 1,022,750; 983,292; 979,709; 1,003,709; 979,375; 935,417; 1,061,167; 938,542 | 983,292 (480.12 ns/pair) | 876,000; 927,541; 762,875; 806,125; 959,625; 763,583; 802,833; 860,250; 779,334 | 806,125 (393.62 ns/pair) | -18.018% | 0.822821 |
| quick `dirty_tile_rebuild` | 2,647,083; 2,901,167; 2,924,334; 2,831,958; 3,145,625; 2,790,375; 2,792,916; 2,628,458; 2,810,750 | 2,810,750 (87,835.94 ns/edit) | 2,024,042; 2,042,292; 2,003,833; 2,052,209; 1,894,417; 2,082,833; 2,047,291; 2,003,709; 2,078,375 | 2,042,292 (63,821.62 ns/edit) | -27.340% | 0.733030 |
| full `pan_zoom_snapshot` | 15,780,750; 18,756,375; 15,321,000; 20,530,458; 15,149,542; 15,212,375; 15,715,208; 15,543,917; 14,997,208 | 15,543,917 (1,897.45 ns/pair) | 12,914,583; 13,662,750; 22,707,583; 14,912,000; 12,638,667; 12,975,459; 13,575,417; 13,585,333; 12,932,542 | 13,575,417 (1,657.16 ns/pair) | -12.664% | 0.852954 |
| full `dirty_tile_rebuild` | 11,979,708; 13,977,250; 12,139,333; 12,720,791; 13,085,000; 12,054,417; 12,057,750; 12,089,750; 12,359,792 | 12,139,333 (94,838.54 ns/edit) | 9,335,625; 9,387,459; 14,266,459; 12,255,625; 9,173,458; 9,278,250; 9,558,500; 9,517,792; 9,199,709 | 9,387,459 (73,339.52 ns/edit) | -22.669% | 0.779287 |

Both protected candidate medians are below the old revision-max baseline in
both profiles. These results calibrate the approved routine envelope above.
The M1 flat document-hash observations and historical one-pair M2/G13 values use
a different workload and remain provenance only, not an acceptance threshold.
Reconstruct and compare the old build only when a workload or harness changes,
an environment envelope is created or revised, or the user requests an explicit
boundary audit. Such a comparison uses the same host, power mode, toolchain,
target, Release profile, exact input, warm-up, and order-interleaved samples.

## Windows native wheel/drawing fixture and calibration provenance

The private command below runs the complete Windows route independently of the
long application smoke:

```text
inkpod.exe --performance-smoke-test
```

The fixture creates a 1024-square document and uses 16 untimed horizontal
strokes to materialize exactly 256 document tiles (1,048,576 payload bytes).
After 32 untimed wheel pairs, `wheel_zoom` sends 256 alternating wheel pairs,
512 events total, through the Canvas token queue, UI command handling, CoreHost,
C ABI view update, snapshot construction, renderer queue, GPU update, and
Present. A smoke-only queue barrier holds the renderer during each enqueue and
then waits for idle, fixing the output at exactly 512 successful Presents.
For this gate, idle means that the renderer work queue is empty, its in-flight
work count is zero, and the `Present` path for the last dequeued item has
returned. A renderer-host regression test releases multiple queued renders and
requires the observed Present count to advance by exactly that queued count
before the idle wait returns. The document revision and checksum stay unchanged
while the view revision advances.

After one untimed vertical stroke, `drawing` sends 16 vertical strokes through
the same native input route. Each stroke has begin/end plus 32 move samples,
crosses all 16 tile rows, commits one document revision, and is synchronized to
exactly one final Present. The measured total is 544 samples and 16 Presents.
This is a burst-to-final-frame throughput/latency gate; it deliberately does not
claim to measure physical pointer sampling cadence or how many optional preview
frames can be displayed between those samples. CoreHost and renderer queue
rejection/resource-limit counters must not advance in either scenario.

The old `3f164db` and candidate binaries used the same private harness. The
launch/performance-smoke adapter and smoke-only queue pause/idle instrumentation
were copied into the detached old worktree. The old Core, cache identity,
composition, GPU update, and Present algorithms remained unchanged. Both were
ARM64 Release builds on the same Windows build 26200 Apple Silicon/Parallels
host, MSVC 19.51 toolchain, and power scheme recorded above. Every process
performs the fixture and internal warm-ups before its timer. The initial nine
order-alternating pairs put the wheel candidate median 0.008817% above the old
median, so the required remeasurement used nine more pairs with the starting
order reversed. All 18 pairs are retained and pooled below; none were discarded.

For rebaseline or an explicit audit, the exact old-worktree backport is
versioned as
`tests/revision_max_native_harness_3f164db.patch` (SHA-256
`2b434f0ab5827fc987f0cb583ff68f65c4af6b9aaf89531fa8735bee071044a0`).
It contains full old/new Git blob IDs and changes only the seven launch/smoke
adapter files plus `apps/windows/renderer/canvas.cpp`. A structural test fixes
that eight-file allowlist, artifact hash, and the Canvas hunk's restriction to
pause/idle/in-flight instrumentation; GPU update and Present algorithms are
forbidden from its changed lines. Recreate the baseline from a fresh detached
worktree before measuring. This is not part of routine range validation:

```powershell
$baseline = 'C:\path\to\inkpod-revision-max-baseline'
$artifact = 'C:\path\to\inkpod\tests\revision_max_native_harness_3f164db.patch'
git worktree add --detach $baseline 3f164db
git -C $baseline apply --check $artifact
git -C $baseline apply $artifact
git -C $baseline diff --check
git -C $baseline status --short
```

The final status must list exactly the eight allowlisted paths recorded in the
artifact. Applying it to a fresh `3f164db` was reverified, and all eight
line-ending-normalized Git blob hashes matched the candidate-side blob IDs in
the artifact before the recorded build.

| Native scenario | Old revision-max samples | Old median | Candidate samples | Candidate median | Median change | Paired ratio median |
| --- | --- | ---: | --- | ---: | ---: | ---: |
| `wheel_zoom` | 4,333,094,542; 4,300,313,875; 4,266,779,666; 4,374,209,833; 4,308,311,041; 4,283,368,833; 4,266,868,292; 4,267,208,958; 4,266,503,084; 4,266,692,750; 4,266,565,000; 4,266,683,083; 4,266,846,917; 4,266,459,708; 4,267,009,167; 4,266,605,000; 4,283,311,416; 4,266,696,833 | 4,266,857,604.5 (8,333,706.26 ns/event) | 4,300,356,375; 4,320,488,250; 4,275,040,667; 4,283,258,541; 4,807,472,250; 4,283,746,500; 4,283,928,083; 4,266,568,458; 4,266,470,125; 4,266,740,959; 4,266,852,041; 4,266,223,625; 4,266,664,208; 4,266,803,834; 4,266,816,292; 4,266,702,834; 4,267,033,625; 4,266,779,791 | 4,266,834,166.5 (8,333,660.48 ns/event) | -0.000549% | 1.000015 |
| `drawing` | 263,484,584; 257,813,209; 254,334,666; 258,986,167; 261,507,958; 257,344,042; 244,670,500; 248,282,625; 241,934,875; 240,791,208; 241,478,667; 237,061,084; 243,021,166; 235,438,416; 241,834,541; 244,370,625; 244,096,000; 238,072,000 | 244,233,312.5 (15,264,582.03 ns/stroke) | 168,649,542; 175,048,500; 172,506,125; 171,752,500; 172,462,042; 183,494,333; 288,510,833; 159,425,875; 170,125,125; 163,194,458; 162,927,625; 160,333,250; 161,339,875; 161,310,250; 159,860,250; 160,288,834; 163,196,666; 158,936,542 | 163,195,562 (10,199,722.63 ns/stroke) | -33.180466% | 0.671642 |

The pooled candidate median is below the old revision-max median in both native
scenarios, so the fixed-work native gate passes. The wheel margin is only
-0.000549%, while its paired-ratio median is 1.000015371 (+0.001537%, with the
candidate slower in 10 of 18 pairs). The user explicitly accepted this observed
microdifference as display-paced measurement noise on 2026-08-05. It is not
claimed as a speedup, counted as a performance regression, or converted into a
general tolerance for later changes. The drawing improvement is 33.180466%. An earlier
observational loop that let the renderer freely coalesce snapshots was not
adopted as the baseline because old and candidate runs presented different
frame counts. Performance evidence must compare equal semantic work, wait for
in-flight Present completion, and require equal successful Present counts.

The Core benchmark and native performance smoke are complementary protected
scenarios. Routine acceptance follows the matching environment envelope above,
including the independent second five-run batch after an upper-edge result.
The explicitly accepted +0.001537% wheel diagnostic remains calibration
provenance for this run and is not a CPU speedup claim. Rebuild the old binary
only for recalibration or an explicit audit. Stored wall-clock values remain
host-specific; shared CI continues to enforce semantic counters and the
source-payload guard rather than comparing nanoseconds across machines.

## M2 reference run

The initial baseline was captured on 2026-07-30 on macOS ARM64 with Rust 1.95.0.
The table reports the median of three warmed release runs. These numbers are a
same-machine comparison reference only.

| Scenario | Quick median ns | Full median ns | Quick semantic counters | Full semantic counters |
| --- | ---: | ---: | --- | --- |
| `sparse_snapshot` | 735,166 | 3,143,417 | 8 tiles, revision/history 9/8 | 32 tiles, revision/history 33/32 |
| `dirty_tile_rebuild` | 82,250 | 90,250 | 1 rebuilt, 7 reused | 1 rebuilt, 31 reused |
| `pan_zoom_snapshot` | 1,417 | 9,625 | 8 reused | 32 reused |
| `undo_redo` | 2,529,584 | 11,250,375 | 12 entries, 36 successful steps | 48 entries, 144 successful steps |
| `light_table_composite` | 1,769,125 | 13,713,916 | 3 references, 4 tiles | 6 references, 16 tiles |
| `vector_snapshot` | 59,910,334 | 301,657,959 | 32 segments, 8 fills | 128 segments, 32 fills |
| `batch_preview` | 224,541 | 1,076,667 | 4 dry-run successes, 1 rejected invalid graph | 16 dry-run successes, 1 rejected invalid graph |

Compare performance changes on the same machine and profile using repeated-run
medians. Correctness is determined by the assertions and fixed counters, never
by an absolute elapsed-time threshold.

## G13 Windows x64 reference and review threshold

The first G13 Windows reference was captured on 2026-08-03 with Windows build
26200, an AMD64 Family 26 Model 68 processor, and Rust 1.97.1. The values below
are medians of five warmed quick-profile runs. They are not portable speed
claims; comparison requires the same machine, power mode, toolchain, and
profile.

| Benchmark/scenario | Median |
| --- | ---: |
| image sparse 1,048,576 square / 512 samples | 8 ms |
| image dense 1,024 square / 4,194,304 bytes | 45 ms |
| `sparse_snapshot` | 878,800 ns |
| `dirty_tile_rebuild` | 112,500 ns |
| `pan_zoom_snapshot` | 5,300 ns |
| `undo_redo` | 3,057,500 ns |
| `light_table_composite` | 1,868,600 ns |
| `vector_snapshot` | 45,555,100 ns |
| `batch_preview` | 210,100 ns |

For scenarios other than protected zoom and incremental drawing, a same-machine
median is a release-review regression when it is both more than 25% above this
reference and more than 100 microseconds slower. The image benchmark reports
integer milliseconds, so its absolute-noise floor is one millisecond. Such a
confirmed general regression blocks release until it is explained, accepted, or
corrected; a single run never fails the gate. `pan_zoom_snapshot` and
`dirty_tile_rebuild` instead use the matching approved routine envelope above.
An upper-edge median triggers a second independent five-run batch, and both
medians must remain above the edge before the change is rejected. Semantic
checksum, counter, allocation-bound, and resource-budget failures remain
unconditional.
