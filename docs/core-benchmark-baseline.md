# Rust Core workflow benchmark baseline

This document defines the current reproducible performance and semantic
comparison contract for `rust/inkpod-core/benches/core_workflows.rs` and the
native Windows performance smoke. Cross-host wall-clock values are not a CI
gate. Semantic counters and checksums are hard gates on every host; elapsed time
is gated only when the environment matches an explicitly approved envelope.

Workload, harness, environment envelope, or the canonical cache formula may be
changed only with recorded reasoning, complete samples and semantic counters,
and explicit user approval. Replace current values instead of appending dated
acceptance logs. Historical calibration and milestone results are summarized in
[`legacy.md`](legacy.md).

## Commands and profiles

```text
cargo bench --package inkpod-core --bench core_workflows -- --quick
cargo bench --package inkpod-core --bench core_workflows
```

Both commands use the release benchmark profile and the same ten scenarios.
Quick is the bounded CI profile; full increases inputs for local before/after
comparison. The checkpoint fixture is written outside its timed open interval
and removed afterward. Batch uses in-memory sequence cells and asserts that its
absent output directory remains absent.

| Parameter | Quick | Full |
|---|---:|---:|
| sparse document dimensions | `MAX_RASTER_DIMENSION` square | same |
| sparse allocated tiles | 8 | 32 |
| dirty one-pixel edit + snapshot rebuild steps | 32 | 128 |
| pan/zoom view-only snapshot pairs | 2,048 | 8,192 |
| Undo/Redo edits | 12 | 48 |
| light-table document | 128 square, 3 references | 256 square, 6 references |
| vector document | 128 square, 8 closed paths/fills | 256 square, 32 closed paths/fills |
| Batch sequence | 4 cells at 16 square | 16 cells at 32 square |
| canonical replay fixture | 64 square, 5 canonical edits | same |
| checkpoint policy fixture | 256 procedures, 175,000 stroke samples | 256 procedures, 1,000,000 stroke samples |
| output-color guard fixture | 1,024-square straight RGBA16, 1,048,576 pixels | 2,048-square straight RGBA16, 4,194,304 pixels |

The output-color guard fixture repeats a fixed 16-pixel row-aligned pattern:
one transparent unsafe RGB pixel, seven opaque safe neutral pixels, and eight
opaque unsafe red pixels. Fixture construction is outside the timed interval.
The interval includes the exact visible-composite scan, sparse selection build,
and one canonical commit.

## Output and semantic gates

Every scenario prints exactly one line with this stable schema:

```text
inkpod-core-workflows profile=<profile> scenario=<name> iterations=<n> input_items=<n> output_items=<n> reused_items=<n> document_revision=<n> history_entries=<n> successes=<n> failures=<n> checksum=<hex> elapsed_ns=<n>
```

The benchmark fails directly on semantic checksum or counter drift. The
scenario assertions are:

| Scenario | Hard assertion |
|---|---|
| `sparse_snapshot` | only deliberately allocated sparse tiles render |
| `dirty_tile_rebuild` | one edited tile rebuilds and every other tile revision remains reusable |
| `pan_zoom_snapshot` | every zoom/pan pair builds a snapshot without changing document revision, history, pixels, or tile revisions |
| `undo_redo` | every edit is one history entry, Undo reaches the clean savepoint, and Redo restores the exact checksum |
| `light_table_composite` | every reference contributes to the expected tile grid and checksum |
| `vector_snapshot` | ordered pass, segment/fill counts, zero legacy raster snapshot tiles, and rasterized pixels match |
| `batch_preview` | one invalid graph is rejected, valid inputs dry-run successfully, and no output is generated |
| `canonical_replay` | six boundaries replay bit-exactly; final digest and runtime epoch 21 / native v24 / numeric v1 contract match |
| `checkpoint_open` | policy emits CKPT; verified open restores the journal/document digest and exact Undo/Redo; full crosses one million replay-work units |
| `output_color_guard` | exact scanned/selected/transparent counts, one canonical commit, revision 2/history 1, exact sparse selection bounds/tile bytes, zero CPU staging bytes, and result digest match |

The checksum is local FNV-1a over fixed-width public semantic data and excludes
wall-clock time, addresses, cache allocation order, and Batch output paths.

| Scenario | Quick checksum | Full checksum |
|---|---|---|
| `sparse_snapshot` | `517ed7ae78bf0487` | `439040e0244d5773` |
| `dirty_tile_rebuild` | `9e13576def6f539b` | `a33f7534fcdd61e7` |
| `pan_zoom_snapshot` | `517ed7ae78bf0487` | `439040e0244d5773` |
| `undo_redo` | `3f1053b9fde37d35` | `a2c1a74e7f9781a3` |
| `light_table_composite` | `255ab9bad114dfdd` | `77f63d83e130185f` |
| `vector_snapshot` | `2813c527f27311c8` | `b975f3cfdb7824fd` |
| `batch_preview` | `f31d31fe1bb00fd7` | `6732b8b0a6565d03` |
| `canonical_replay` | `264b98028ac92ac6` | `264b98028ac92ac6` |
| `checkpoint_open` | `b63e39424fbad396` | `b63e39424fbad396` |
| `output_color_guard` | `8b2bd6bfbf8eada8` | `53bbe70c027a2864` |

The v19/schema-6 Color-chart commitment changed only the `checkpoint_open`
document-digest checksum from `eca2df7e74020108` to `8847f8440d290c18`.
Unchanged quick and full workloads independently produced the new value while
retaining procedure count 256, output/history 256, asset reuse 1, document
revision 3, one success, zero failures, and exact checkpoint Undo/Redo. The
workload, harness logic, timed interval, envelope, and `revision-max` formula
are unchanged.

The v20/epoch-17 output-color guard adds one closed canonical primitive. Its
dedicated tenth scenario changes the benchmark workload and harness, but does
not change the original nine scenarios, their checksums, pixel/cache paths, the
existing envelopes, or the `revision-max` formula. The new scenario's counter
mapping is deliberately explicit: `iterations` is row count, `input_items` is
total pixels, `output_items` is selected pixels, and `reused_items` is
transparent pixels skipped by the scan. Quick therefore fixes
1,048,576/524,288/65,536 pixels and 256 selection tiles; full fixes
4,194,304/2,097,152/262,144 pixels and 1,024 selection tiles. Both profiles
commit exactly once at revision 2/history 1 with zero failures.

The v21/epoch-18 floating-transform change updates the closed primitive catalog
and canonical contract identity but does not alter any benchmark workload,
harness, counter mapping, envelope, or `revision-max` expression. The recorded
`canonical_replay` checksum is updated only after quick/full profiles reproduce
the same new contract checksum.

The v22/epoch-19 individual-Cell Cut descriptor adds a separate bounded persistence
and history domain. It advances the closed current contract without changing the
ten benchmark workloads, their semantic counters/checksums, the harness, any
approved envelope, payload-access route, or the `revision-max` expression.

The v23/epoch-20 ordered Cut-membership transaction separates immutable member
assets from ordered membership and extends Cut history outside the document/render
hot path. It changes no benchmark workload, semantic counter/checksum, harness,
approved envelope, payload-access route, or `revision-max` expression. The required
quick run must therefore retain all ten recorded checksums and reuse/rebuild gates.

The v24/epoch-21 annotation primitive adds persisted annotation objects to the
canonical document frame and normal-output annotations to flattened snapshot output.
Unchanged quick and full workloads independently reproduced the new
`canonical_replay` checksum `264b98028ac92ac6` and `checkpoint_open` checksum
`b63e39424fbad396`. The output-color-guard result digest changed to
`8b2bd6bfbf8eada8` (quick) and `53bbe70c027a2864` (full), while retaining the exact
quick/full scanned, selected, transparent, revision, history, success, and failure
counters recorded above. No workload, harness logic, approved envelope,
payload-access route, or `revision-max` expression changed.

## Approved output-color-guard envelope

The active guard range ID is
`windows-x64-ryzen-9-9950x3d-release-2026-08-11-output-color-guard-v1`. It
applies only to Windows build 26200.8973 on the MSI MS-7E26 host with an AMD
Ryzen 9 9950X3D and 127.6 GiB memory, x86_64-pc-windows-msvc, Rust/Cargo 1.97.1,
LLVM 22.1.6, MSVC 19.51.36252.0, Release benchmark profile, and the Windows
Balanced power scheme. A materially different host, target, toolchain, or power
mode needs its own approved range.

| Protected score | Accepted range | Reference median | Interpretation |
|---|---:|---:|---|
| quick `output_color_guard`, 1,024 square | 55–92 ms total | 72.876 ms | exact RGBA16 scan + sparse selection + canonical commit |
| full `output_color_guard`, 2,048 square | 255–425 ms total | 339.903 ms | scaled exact RGBA16 scan + sparse selection + canonical commit |

An unmeasured warm-up process was discarded for each profile before the
accepted sample batch. Checksum-discovery and output-extraction diagnostic runs
were also excluded. The accepted samples below are independent measured Release
processes in run order; every process retained the checksum and all
semantic/allocation counters above.

| Protected score | Complete accepted samples (ns) | Median |
|---|---|---:|
| quick `output_color_guard` | 73,712,100; 72,903,800; 72,875,900; 72,474,200; 72,660,400; 72,554,800; 73,089,600; 72,963,800; 72,812,200 | 72,875,900 |
| full `output_color_guard` | 350,192,400; 363,748,200; 339,902,800; 330,812,600; 328,050,600 | 339,902,800 |

The accepted bounds are the reference median's rounded 75–125% band. The lower
edge diagnoses accidentally skipped work while semantic hard gates remain
authoritative; the upper edge detects a material regression and retains the
independent-five-run confirmation rule. This new workload and envelope were
created under the user's explicit 2026-08-11 approval; they do not recalibrate
or widen any existing range.

## Approved routine envelope

The active range ID is
`windows-arm64-apple-silicon-parallels-release-2026-08-05`. It applies only to
Windows build 26200 on the recorded Apple Silicon/Parallels ARM64 host, Rust and
Cargo 1.97.1, LLVM 22.1.6, MSVC 19.51 for the native route, Release profiles,
the recorded `Parallels` power scheme, and the exact workloads above. The wheel
range was captured with the Parallels Display Adapter at 3456 x 2168 and 120 Hz;
its normalization denominator is 8,333,333.33 ns per refresh interval. A
materially different host, target, toolchain, power mode, or display mode needs
its own approved range.

| Protected score | Accepted range | Reference median | Interpretation |
|---|---:|---:|---|
| quick `pan_zoom_snapshot`, 2,048 pairs | 0.70–1.05 ms total | 0.806 ms | Core CPU/view-cache gate |
| quick `dirty_tile_rebuild`, 32 edits | 1.8–2.4 ms total | 2.042 ms | Core incremental-drawing gate |
| full `pan_zoom_snapshot`, 8,192 pairs | 12–16 ms total | 13.575 ms | Core CPU/view-cache gate |
| full `dirty_tile_rebuild`, 128 edits | 8.5–11 ms total | 9.387 ms | Core incremental-drawing gate |
| native `drawing`, 16 strokes | 150–200 ms total | 163.196 ms | burst through 16 final Presents |
| native `wheel_zoom`, 512 events | 0.95–1.10 refresh intervals/event | approximately 1.00 | Present-paced routing gate |

The approved-range evidence uses nanoseconds in run order. Core old/candidate
comparison used nine alternating-order pairs after discarded warm-ups. The
native comparison pooled eighteen alternating-order pairs after remeasurement
of the display-paced wheel scenario. The table retains the complete candidate
samples that define the active range; the superseded comparison samples and
adoption narrative are summarized in [`legacy.md`](legacy.md) and preserved in
Git history.

| Protected score | Complete accepted samples (ns) | Median |
|---|---|---:|
| quick `pan_zoom_snapshot` | 876,000; 927,541; 762,875; 806,125; 959,625; 763,583; 802,833; 860,250; 779,334 | 806,125 |
| quick `dirty_tile_rebuild` | 2,024,042; 2,042,292; 2,003,833; 2,052,209; 1,894,417; 2,082,833; 2,047,291; 2,003,709; 2,078,375 | 2,042,292 |
| full `pan_zoom_snapshot` | 12,914,583; 13,662,750; 22,707,583; 14,912,000; 12,638,667; 12,975,459; 13,575,417; 13,585,333; 12,932,542 | 13,575,417 |
| full `dirty_tile_rebuild` | 9,335,625; 9,387,459; 14,266,459; 12,255,625; 9,173,458; 9,278,250; 9,558,500; 9,517,792; 9,199,709 | 9,387,459 |
| native `drawing` | 168,649,542; 175,048,500; 172,506,125; 171,752,500; 172,462,042; 183,494,333; 288,510,833; 159,425,875; 170,125,125; 163,194,458; 162,927,625; 160,333,250; 161,339,875; 161,310,250; 159,860,250; 160,288,834; 163,196,666; 158,936,542 | 163,195,562 |
| native `wheel_zoom` | 4,300,356,375; 4,320,488,250; 4,275,040,667; 4,283,258,541; 4,807,472,250; 4,283,746,500; 4,283,928,083; 4,266,568,458; 4,266,470,125; 4,266,740,959; 4,266,852,041; 4,266,223,625; 4,266,664,208; 4,266,803,834; 4,266,816,292; 4,266,702,834; 4,267,033,625; 4,266,779,791 | 4,266,834,166.5 |

Every sample retained the expected checksum, revision/history,
reuse/rebuild/payload-access, sample/Present, and queue/resource counts. The
wheel median is approximately one 120-Hz refresh interval per event and is not
a CPU speedup claim.

For unprotected scenarios, a same-machine median enters release review when it
is both more than 25% above its accepted reference and more than 100 microseconds
slower. The image benchmark reports integer milliseconds, so its noise floor is
one millisecond. This general review rule never weakens a semantic, allocation,
or resource gate.

## Routine measurement procedure

1. Confirm the environment exactly matches the selected range ID.
2. Run at least one unmeasured warm-up, then at least five measured processes.
   The native command warms its fixture internally, but the complete process is
   still repeated at least five times.
3. Verify all checksum, revision, history, reuse/rebuild, payload-access,
   sample, Present, queue, and resource counters before considering time.
4. Compare the median with the matching range. A value below the lower edge is
   diagnostic only: verify that no work was skipped, then accept it when every
   semantic gate remains intact.
5. If the median exceeds the upper edge, run a second independent batch of at
   least five processes. Confirm a regression only when both medians exceed it.
6. For `wheel_zoom`, divide elapsed time per event by that run's display refresh
   interval. Do not interpret its absolute nanoseconds as a CPU benchmark.

An envelope is never widened automatically. Creating or changing one records a
new range ID, environment, all samples, semantic counters, reason, and explicit
approval in this section.

## Native Windows fixture

Run the Release executable with:

```text
inkpod.exe --performance-smoke-test
```

The fixture creates a 1024-square document and uses 16 untimed strokes to
materialize exactly 256 tiles and 1,048,576 payload bytes. After 32 untimed wheel
pairs, `wheel_zoom` sends 256 alternating wheel pairs through Canvas input, UI,
`CoreHost`, C ABI view update, snapshot construction, renderer queue, GPU update,
and Present. A smoke-only barrier fixes the result at 512 successful Presents.
Idle requires an empty queue, zero in-flight work, and return from the last GPU
update/Present path.

After an untimed vertical stroke, `drawing` sends 16 vertical strokes. Each has
begin/end plus 32 move samples, crosses all 16 tile rows, commits one revision,
and produces one final Present: 544 samples and 16 Presents total. CoreHost and
renderer rejection/resource-limit counters must remain unchanged. This measures
burst-to-final-frame behavior, not physical pointer sampling cadence or optional
preview-frame count.

## Exceptional recalibration or boundary audit

Reconstruct the detached old production build only when a workload or harness
changes, an environment envelope is created or revised, or the user requests an
explicit boundary audit. The baseline is commit `3f164db`. The exact native
harness backport is `tests/revision_max_native_harness_3f164db.patch` with
SHA-256 `2b434f0ab5827fc987f0cb583ff68f65c4af6b9aaf89531fa8735bee071044a0`.
Structural tests enforce its eight-file allowlist and prohibit production GPU
update/Present changes.

```powershell
$baseline = 'C:\path\to\inkpod-revision-max-baseline'
$artifact = 'C:\path\to\inkpod\tests\revision_max_native_harness_3f164db.patch'
git worktree add --detach $baseline 3f164db
git -C $baseline apply --check $artifact
git -C $baseline apply $artifact
git -C $baseline diff --check
git -C $baseline status --short
```

The status must contain exactly the eight allowlisted paths. Compare old and
candidate on the same host, target, toolchain, Release profile, power/display
mode, inputs, warm-up, and semantic work. Use order-interleaved samples and
retain every sample. Any formula, workload, harness, or envelope change needs
explicit approval; the raw calibration history is not copied back into this
living contract.
