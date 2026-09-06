# Rust Core workflow benchmark baseline

This document defines the current reproducible performance and semantic
comparison contract for `rust/inkpod-core/benches/core_workflows.rs`, the
crate-private InkScript quick runner, and the native Windows performance smoke.
Cross-host wall-clock values are not a CI gate. Semantic counters and checksums
are hard gates on every host; elapsed time is gated only when the environment
matches an explicitly approved envelope.

Workload, harness, environment envelope, or the canonical cache formula may be
changed only with recorded reasoning, complete samples and semantic counters,
and explicit user approval. Historical calibration narratives belong in Git
history; complete samples defining an active envelope remain below.

Exact checksum assertions are executable in [core_workflows.rs](../rust/inkpod-core/benches/core_workflows.rs)
and [script/performance.rs](../rust/inkpod-core/src/script/performance.rs).
They must not be changed merely to make a measurement pass. SPEC owns the
current native/replay/catalog versions; version-dependent expectations must be
verified separately from workload, pixel, non-byte counter and timing contracts.

## Commands and profiles

```text
cargo bench --package inkpod-core --bench core_workflows -- --quick
cargo bench --package inkpod-core --bench core_workflows
cargo test --release --package inkpod-core --lib script::tests::approved_quick_performance_contract -- --ignored --exact --nocapture --test-threads=1
```

The two `cargo bench` commands use the release benchmark profile and the same nine scenarios.
Quick is the bounded CI profile; full increases inputs for local before/after
comparison. The checkpoint fixture is written outside its timed open interval
and removed afterward. Batch creates its native inputs outside the timed
interval, opens and replays them through the production path during preview and
dry-run, removes them afterward, and asserts that its absent output directory
remains absent.

The ignored InkScript command is a separate Release process-level quick gate.
It does not add a `core_workflows` scenario or change either existing
profile. Fixture construction is untimed; the interval begins immediately
before static compile and ends after plan, success/failure/cancel runs,
cache-free reopen, checksum construction, and exact counter assertions.

| Parameter | Quick | Full |
|---|---:|---:|
| sparse document dimensions | `MAX_RASTER_DIMENSION` square | same |
| sparse allocated tiles | 8 | 32 |
| dirty one-pixel edit + snapshot rebuild steps | 32 | 128 |
| pan/zoom view-only snapshot pairs | 2,048 | 8,192 |
| Undo/Redo edits | 12 | 48 |
| light-table document | 128 square, 3 references | 256 square, 6 references |
| Batch native inputs | 4 `.inkpod` files at 16 square | 16 `.inkpod` files at 32 square |
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
| `batch_preview` | one invalid graph is rejected; every unique native input opens, replays, and applies its exact native-depth Color Replace row in preview and dry-run; no output is generated |
| `canonical_replay` | six boundaries replay bit-exactly; final digest and runtime epoch 29 / native v34 / numeric v1 contract match |
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
| `batch_preview` | `9ae6835726a36053` | `d1be39275687aa9b` |
| `canonical_replay` | `34f65a7092a87cff` | `34f65a7092a87cff` |
| `checkpoint_open` | `5062efd7565e19e4` | `5062efd7565e19e4` |
| `output_color_guard` | `1b7549d882cd9743` | `fd514e08addb4698` |

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
and history domain. It advances that historical contract without changing the
then-current benchmark workloads, their semantic counters/checksums, the harness,
any approved envelope, payload-access route, or the `revision-max` expression.

The v23/epoch-20 ordered Cut-membership transaction separates immutable member
assets from ordered membership and extends Cut history outside the document/render
hot path. It changes no benchmark workload, semantic counter/checksum, harness,
approved envelope, payload-access route, or `revision-max` expression. The required
quick run had to retain the recorded checksums and reuse/rebuild gates.

The v25/epoch-22 shooting-frame primitive adds the optional angled-frame field
to the canonical document commitment without changing any benchmark workload,
harness logic, approved envelope, payload-access route, or `revision-max`
expression. Quick and full retain every scenario counter and the unchanged
`canonical_replay` checksum. The intentionally advanced document schema changes
`checkpoint_open` to `c66817dca5345832` in both profiles and changes the
output-color-guard result digest to `650300bdff9044cb` quick and
`290f6150f7718c2d` full; scanned/selected/transparent counts, revision 2,
history 1, success 1, and failure 0 are unchanged.

The historical v26/epoch-23 vanishing-point primitive added persistent point fields to the
canonical document commitment and bounded viewport-derived radial overlay
records. It does not change any benchmark workload, harness logic, approved
wall-clock envelope, payload-access route, or the `revision-max` expression.
Two independent quick and two independent full runs retained all then-current scenario
counters, `canonical_replay` checksum `264b98028ac92ac6`, and every audited
reuse/rebuild gate. The intentional schema-9 commitment changes
`checkpoint_open` to `07da1b4e6bc5d289` in both profiles and changes the
output-color-guard result digest to `cfb6b288963c78ba` quick and
`2b2196e06f7198b3` full. Workload, harness logic, approved envelope,
payload-access route, and `revision-max` remained unchanged.

The explicitly approved M27B rebaseline removed the retired drawing-model
scenario and retained the nine raster/current-product scenarios listed above.
Native v27/replay epoch 24, document digest schema 10, editor schema 7, and
composite schema 4 were that baseline's exact-current contract. Those versioned commitments
changed `canonical_replay` to `70d3465b6732887e`, `checkpoint_open` to
`bcf482082855c1f2`, and `output_color_guard` to `1005f8901846f431` quick /
`558cb3aacd55afd9` full. All retained workload parameters and semantic counters
remain unchanged; no wall-clock envelope or `revision-max` expression is widened.

The explicitly approved Batch v3 replacement advanced the then-current contract to
native v28/replay epoch 25/document-state schema 11. It replaces only the retired
`batch_preview` sequence/Filter fixture: construction now writes four quick or
sixteen full native files before timing, each with one patterned straight-RGBA8
color plane and one unique exact Color Replace row. The timed interval opens and
fully replays every file in both preview and dry-run, while `{stem}` plus a
four-digit index keeps all prospective output paths unique. Input dimensions
remain 16 square quick and 32 square full. Semantic counters remain exactly
`iterations=2`, `input_items=4/16`, `output_items=8/32`, `reused_items=0`,
`document_revision=0`, `history_entries=0`, `successes=4/16`, and `failures=1`
for the intentional invalid-version probe.

After one discarded warm-up per profile, the complete primary `batch_preview`
samples were 14,767,375; 6,771,084; 6,250,334; 6,072,125; 6,706,042 ns quick
(median 6,706,042 ns) and 31,496,500; 21,184,833; 23,868,166; 17,591,958;
20,480,292 ns full (median 21,184,833 ns). A separate diagnostic batch retained
the same counters and checksums with 14,216,292; 4,049,542; 3,598,625; 4,280,333;
3,788,792 ns quick (median 4,049,542 ns) and 22,633,209; 20,987,917; 19,087,125;
21,122,625; 18,912,500 ns full (median 20,987,917 ns). All processes produced
`9ae6835726a36053` quick and `d1be39275687aa9b` full. Measurements were taken on
an `aarch64-pc-windows-msvc` Parallels host with 8 GiB memory and therefore do
not establish or modify the approved x64 wall-clock envelopes. The new empty
fill-protection commitment changes only `checkpoint_open` and
`output_color_guard` digests in that historical baseline; their workloads, counters,
envelopes, and the `revision-max` expression are unchanged.

The user-approved v31/epoch-27 standard-layer rebaseline changes only the three
version-sensitive current checksums: `canonical_replay` is
`34f65a7092a87cff` in both profiles, `checkpoint_open` is
`5062efd7565e19e4` in both profiles, and `output_color_guard` is
`1b7549d882cd9743` quick / `fd514e08addb4698` full. Current quick and full runs
reproduced every semantic counter and checksum. Workloads, timed intervals,
approved envelopes, payload-access gates, and the `revision-max` expression are
unchanged.

After one discarded warm-up per profile, the accepted M22
`output_color_guard` samples were 74,551,800; 74,892,300; 75,355,400;
73,662,700; 75,177,400 ns quick (median 74,892,300 ns) and 341,581,800;
339,117,200; 340,747,300; 348,131,800; 338,601,500 ns full (median
340,747,300 ns). Both medians remain inside the approved 55–92 ms and
255–425 ms envelopes, and every measured process retained the exact counters
and checksums above, so no independent regression-confirmation batch was needed.

## Approved InkScript quick envelope

The active range ID is
`windows-x64-ryzen-9-9950x3d-release-2026-09-06-inkscript-current`. The user
approved retaining the same **64–107 ms** bounds for Windows build 26200.9278
on the MSI MS-7E26 host with an AMD Ryzen 9 9950X3D, x86_64-pc-windows-msvc,
Rust/Cargo 1.98.1, LLVM 22.1.8, MSVC 19.51.36256, Release profile and Balanced
power. The original gate's current samples are in the M2 section below.
A materially different host, target, toolchain, or power mode needs a
separately approved range; a new measured median does not recalculate its bounds.

The earlier range ID
`windows-x64-ryzen-9-9950x3d-release-2026-08-15-inkscript-v1` and its reference
samples are retained below. They were approved for Windows 26200.9168,
MS-7E26/Ryzen 9 9950X3D, 127.6 GiB, Rust/Cargo 1.97.1 / LLVM 22.1.6 /
MSVC 19.51.36252.0, x64 Release, Balanced. They are not current-toolchain samples.

The fixed quick fixture uses InkScript source ID 913, exact-current file v2/catalog v7 and replay
epoch 29, 128 `set_plane_properties` steps, four successful 4-by-4 current-v34
inputs, one 256 KiB inline straight-sRGB RGBA8 asset, one Save failure and one
pre-linearization cancellation. Every successful output is reopened through
full Genesis/asset/procedure replay without a checkpoint cache. The runner is
`#[cfg(test)]`, crate-private, and ignored by routine debug tests; it adds no
public Rust API, feature, C ABI symbol, Windows route, or product file handling.

| Protected score | Accepted range | Reference median | Interpretation |
|---|---:|---:|---|
| quick InkScript pipeline | 64–107 ms total | 86.8725 ms (current environment) | compile + bind + asset freeze + six staged items + four cache-free reopens |

The semantic hard gates are source 371,176 bytes, 7,965 lexer tokens, 2,000 CST
nodes, zero parameters, one binding/assert/asset, 128 steps/dependency edges/
catalog invocations/work units, 262,144 logical/unique/inline-decoded/copied
asset bytes, zero authorized reads, 24,768 planned input bytes, 37,152 runner
native-read bytes, six attempted items/binding resolutions, 774 statements, 768
invocations, 384 Commit and 384 no-op outcomes, installed/failed/cancelled
4/1/1, 91,584 installed bytes, four cache-free reopens, 256 replayed Commits,
and checksum `3568e2ed6fb803d5`. The failure reason must be exactly Save; neither
negative probe may publish an output.

The counters and checksum above are the fixed assertions in the checked-in
Release runner. Passing Debug workspace tests does not execute this ignored
gate. Current failures are recorded in [compatibility.md](compatibility.md#representative-verification).
A current-version run must pass independently; encoded-byte or digest
drift is not authority to update expectations, the workload or its envelope.

The approved M13 reference batch discarded one warm-up process and retained the
following nine independent Release processes. These samples define the range;
the bounds are the rounded 75–125% band. The lower edge is diagnostic when all
semantic gates remain exact. An upper-edge breach requires another independent
batch of at least five processes, and the envelope is never widened
automatically.

| Complete accepted samples (ns) | Median |
|---|---:|
| 85,230,900; 87,304,700; 84,489,500; 86,339,300; 85,838,200; 85,097,100; 85,237,900; 85,372,200; 86,873,700 | 85,372,200 |

The full fixture is reserved and is not an executable acceptance gate; see
[Reserved InkScript full fixture](#reserved-inkscript-full-fixture).

### M2 approved current-version correction

M1's D1–D4 implementation contract was approved on 2026-09-06. That approval
does not change this performance contract. The original Release quick test at
source `1c65f9db029de56034e2a501c55ce472f708e28b` was rerun without harness or
expectation changes. It exited **101**, with **6,192 actual / 5,968 expected**
at `script/performance.rs:604`, before `Instant::now()`. There is no elapsed
pipeline sample from this failed process; its test duration is not a sample.

The current host is the same MS-7E26 / Ryzen 9 9950X3D family, using Balanced
power, but Windows build **26200.9278**, Rust/Cargo **1.98.1**, LLVM **22.1.8**
and MSVC **19.51.36256** differ from the original reference environment.
Diagnostic observations were not counted as a pass against that environment.
The user subsequently approved this current environment with the same
64–107 ms bounds; no envelope was widened.

The 224-byte increase comes from two persisted tools, `EffectLineConnect` and
`EffectLineWidth`, in [`EditorTool::ALL`](../rust/inkpod-core/src/editor/model.rs).
Each contributes **52 bytes** to the colors sequence and **60 bytes** to the
diameters sequence, including sequence-element lengths and frame/field headers
([editor codec](../rust/inkpod-core/src/editor/codec.rs)). Thus
`2 × (52 + 60) = 224`. Each native file contains one EDIT record, and 224 is
divisible by the container's 8-byte alignment. This is format-dependent state
size, not an increase in the quick script's operations, images or asset payload.

| Fixed assertion | Previous value | Approved current value | Derivation / evidence |
| --- | ---: | ---: | --- |
| Per-input native length | 5,968 | 6,192 | All four fixed UUID inputs encoded independently through the public save API |
| Planned input bytes | 23,872 | 24,768 | Four × 6,192 |
| Runner native-read bytes | 35,808 | 37,152 | Six attempts × 6,192; attempt count remains a separate hard assertion |
| Installed output bytes | 90,688 | 91,584 | Four × 22,896, confirmed by public property calls and native save; one EDIT record per output |
| Full pipeline checksum | `1e41e17e8bda22e3` | `3568e2ed6fb803d5` | All ten authorized diagnostic processes used the unchanged hash walk and agreed |

Public fixture inspection used `new_cell_with_uuid(4, 4, DEFAULT_DPI_MILLI,
DEFAULT_DPI_MILLI, 0x1001..0x1004)` followed by
`capture_document_save().prepare_native_save(false, ...)` and the current encoder.
This uses the same current-state/editor savepoint pair as `build_inputs`, without
changing the benchmark. Each input is header 128 + directory 640 + META 648 +
GENS 848 + EDIT 3,922 + alignment 6 = **6,192 bytes**. The plain BLAKE3 values
below use the same hash as `NativeInputFingerprint` in the fixture.

| Fixture UUID | Native bytes | BLAKE3 |
| --- | ---: | --- |
| `0x1001` | 6,192 | `45edce860e6270ea0d6b2e63f8690f9890edd65c7ecaf4e6e18a2c161cf7666e` |
| `0x1002` | 6,192 | `f6c6312bfce7b24d9961e9e65b61446090b4beebacb627c256ee52dd66a726d3` |
| `0x1003` | 6,192 | `61fbb27cb546819f50b261f54a35596f5019f158958fd972ecccc1acaa12a144` |
| `0x1004` | 6,192 | `c97e147a3ac7ea4b78a404280db562d8317a9440d33570ef7837572e751ce476` |

An independent public-API diagnostic then applied the same 128 property calls
and `probe_name` sequence to each input. Every output had 64 changes, 64 no-ops,
64 history entries/cursor, full cache-free replay, and clean document/editor
savepoints after reopen. Each output adds 16,702 PROC bytes (64 records) and
two alignment bytes to the input: **22,896 bytes**. This Debug fixture inspection
is not Release timing evidence and does not cover the runner's Save/cancel probes.

| Fixture UUID | Output bytes | Output BLAKE3 |
| --- | ---: | --- |
| `0x1001` | 22,896 | `2e962f1483909ed1d2faf39d1c52804c64e704e74e5c619f28384b4b0deccc04` |
| `0x1002` | 22,896 | `4cf8a5abac15b208944d0ca584d05f7a66b0d6f24c68c0eccb2b5d1ba2396f90` |
| `0x1003` | 22,896 | `3072ef891a980c500f8631fa7060b9cd22d7c170393cb9aca115386f5d81a7e5` |
| `0x1004` | 22,896 | `dd6c8ba2ed5ad47b3192156ae6b507bde5abf770616a03b17427f2bbfa72ec69` |

The checksum also includes the current static compilation digest, native bytes,
document/editor digests, reports, history/IDs, savepoints and counters. Catalog,
epoch, native format and editor/digest changes therefore affect more than byte
lengths. The old checksum must not be retained through a hash adjustment, nor
replaced with a value obtained after dropping any of those observations.

**Diagnostic collection was explicitly authorized on 2026-09-06.** The existing
fail-fast gate and all its expected values remain in place. The private
`current_quick_performance_diagnostic` test reuses the existing execution and
hash walk, records only the four version-dependent byte fields and checksum,
and fails after reporting every mismatch. Its output is marked
`inkpod-inkscript-diagnostic acceptance=false`; it is not a passing gate.
Every non-byte assertion, Save/cancel classification, nonpublication check and
cache-free reopen remains fail-fast. The observer reserves all nine mismatch
slots before timing and prints after the original interval ends. No public
API, feature, ABI, workload, source padding, hash algorithm, timed interval or
acceptance envelope changed. Both ignored test entries remain Release-only
execution procedures; Debug workspace success does not execute either one.

One warm-up process measured **87,032,500 ns** and was discarded. All retained
samples below are independent processes of the same Release test binary on
the current configuration. Every process, including warm-up, exited **101**
after all nine expected drift observations: four per-input lengths, summed
input bytes, plan input bytes, runner read bytes, installed bytes and checksum.
No non-byte assertion failed. The ordinary gate also independently retained
its original 6,192/5,968 failure after the observer refactor.

| Complete diagnostic samples in run order (ns) | Median (ns) |
| --- | ---: |
| 87,118,100; 86,392,200; 86,169,600; 86,796,100; 87,667,100; 86,374,300; 87,248,300; 86,735,000; 86,882,800 | 86,796,100 |

Every sample had exactly the same non-time fields: source/token/CST
371,176/7,965/2,000; parameters/bindings/asserts 0/1/1; steps/dependency edges/
catalog invocations/catalog work units 128 each; asset declarations/unique
assets 1/1; logical/unique/decoded/copied bytes 262,144 each; authorized asset
read bytes 0; planned input/read bytes 24,768/37,152; attempted items/binding
resolutions 6/6; statements/invocations 774/768; Commit/no-op 384/384;
installed/failed/cancelled 4/1/1; installed bytes 91,584; cache-free reopens/
replayed Commits 4/256; checksum `3568e2ed6fb803d5`.

**The user approved these five literal changes in `performance.rs` on
2026-09-06, after reviewing the diagnostic samples and counters.** They are
applied; all remaining assertions and the diagnostic observer are unchanged:

```diff
-const EXPECTED_INPUT_NATIVE_BYTES: u64 = 23_872;
+const EXPECTED_INPUT_NATIVE_BYTES: u64 = 24_768;
-const EXPECTED_RUNNER_NATIVE_READ_BYTES: u64 = 35_808;
+const EXPECTED_RUNNER_NATIVE_READ_BYTES: u64 = 37_152;
-const EXPECTED_INSTALLED_OUTPUT_BYTES: u64 = 90_688;
+const EXPECTED_INSTALLED_OUTPUT_BYTES: u64 = 91_584;
-const EXPECTED_CHECKSUM: u64 = 0x1e41_e17e_8bda_22e3;
+const EXPECTED_CHECKSUM: u64 = 0x3568_e2ed_6fb8_03d5;
-            checks.check("per_input_native_bytes", bytes.len() as u64, 5_968);
+            checks.check("per_input_native_bytes", bytes.len() as u64, 6_192);
```

The user separately approved the same **64–107 ms** numeric envelope on this
current environment. After applying the five literals and rebuilding Release,
the original `approved_quick_performance_contract` was run in ten independent
processes: one discarded warm-up (**87,978,900 ns**) and the nine samples below.
All ten exited **0**, with the exact current counters/checksum and no diagnostic
observer. The retained median **86,872,500 ns** satisfies the approved envelope;
there was no upper-bound breach requiring a confirmation batch.

| Complete accepted current-environment samples in run order (ns) | Median (ns) |
| --- | ---: |
| 87,509,300; 86,171,500; 86,318,800; 86,199,500; 86,182,500; 86,872,500; 87,284,400; 88,070,600; 87,034,000 | 86,872,500 |

The old reference samples remain unchanged. Diagnostic samples and their failed
exits remain distinct from these successful original-gate samples. Future
format changes follow the same order: preserve failure → derive version-only
bytes → obtain any necessary diagnostic authorization → retain all counters
and samples → approve exact expectations/environment → independently rerun.
The full fixture remains reserved for M17; this correction cannot complete it.

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
samples that define the active range. Superseded comparisons and adoption
narratives remain in Git history.

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

## Reserved InkScript full fixture

This preserves the unimplemented M36 proposal from the former performance plan.
It is not a current executable gate or permission to resume implementation.
The full fixture and proposed envelope remain fixed; its old native/catalog-dependent
bytes and checksum describe the original v27/epoch-24/file-v1/catalog-v1 measurement,
not current accepted input. Any rebaseline requires an explicit decision with
reasoning, samples and semantic counters. Do not silently update these values.

The same private pipeline includes static compile, binding, inline asset freeze,
success, Save failure, cancellation immediately before install linearization, and
cache-free reopen. Input construction and process startup are untimed. Source ID
is 913; 4-by-4 input UUIDs begin at `0x1001`, with natural `cell1..cellN` ordering.
One unique Color binding and one empty-selection assertion precede paired
`set_plane_properties` names, giving half commits and half no-ops. The inline
straight-sRGB RGBA8 asset uses xorshift64 seed `0x494e4b5343524950`, shifts 13, 7, 17,
and little-endian state bytes. Failure and cancellation execute/encode the first
item, but neither installs output. FNV-1a covers compile/output/state/report/counter
results; failure reason and remaining counters also have independent assertions.

For steps S, successful items N, asset side A and attempted items R=N+2:
asset bytes = 4*A*A; binding resolutions = R; statements = (S+1)*R;
invocations = S*R; commits = no-ops = (S/2)*R; replayed commits = (S/2)*N.
The catalog has S invocation/work units and dependency edges, with zero output
IDs, catalog asset bytes or output growth.

| Reserved full field | Original proposed value |
| --- | --- |
| Steps / successful items / asset side | 1,024 / 8 / 2,048 |
| Source bytes / tokens / CST nodes | 22,537,324 / 61,725 / 15,440 |
| Parameters / bindings / asserts / steps | 0 / 1 / 1 / 1,024 |
| Dependency edges / catalog invocations / work units | 1,024 / 1,024 / 1,024 |
| Asset declarations / unique assets | 1 / 1 |
| Logical / unique / inline-decoded / copied asset bytes | 16,777,216 each |
| Authorized asset reads | 0 bytes |
| Input native bytes / runner reads | 49,536 / 61,920 |
| Attempts / binding resolutions | 10 / 10 |
| Statements / invocations | 10,250 / 10,240 |
| Commits / no-ops | 5,120 / 5,120 |
| Installed / failed / cancelled | 8 / 1 (Save) / 1 (before install) |
| Installed bytes | 1,127,488 |
| Cache-free reopens / replayed commits | 8 / 4,096 |
| Original checksum | `17c636b92b1aebf1` |
| Proposed full envelope / reference median | 15.3–25.6 s / 20.4550998 s |

The reference environment is the Ryzen/x64 environment of the approved quick
envelope above. After one unmeasured warm-up, the complete full samples were
21,118,546,900; 20,455,099,800; 21,988,093,300; 20,432,645,500; 20,310,961,100 ns.
The proposed 75–125% band, diagnostic lower edge and independent five-process
upper-edge remeasurement policy remain unchanged. The original compound candidate
measured 19,976.5163 ms; exploratory one-axis measurements remain in Git history.
