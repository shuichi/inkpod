# Implementation status

This document contains only the current implementation summary, active gaps,
stable known differences, and latest representative verification. Product
requirements are defined in [`../SPEC.md`](../SPEC.md), requirement status and
evidence in [`compatibility.md`](compatibility.md), and ownership/data flow in
[`architecture.md`](architecture.md). Completed plans, superseded designs, and
past acceptance records are summarized in [`legacy.md`](legacy.md).

## Current implementation

| Area | Current state |
| --- | --- |
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. The independent document Color chart owns ordered exact-depth color/name entries and lock state; generation returns a bounded immutable document-UUID/revision-bound comparison from the committed composite, and Apply replaces the chart once without touching the palette. Light Table bulk registration, sequence autosave, and geometry/filter preview retain their stale-safe contracts. Existing brush, scoped replacement, explicit vector topology/diagnostics, Batch, grouped edit targets, typed clipboard, savepoint, Genesis, and asset contracts remain Core-owned. |
| Persistence | Native `.inkpod` is exact-current v19/runtime replay epoch 16 and `.inkbatch` is exact-current v2. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. `ReplaceColorChart/canonical-v1` stores complete named chart entries and lock, `DocumentStateDigest` is schema 6, and EDIT schema 5 stores the chart cursor. Non-v19 input, including v18, is rejected. Save streams to an adjacent temporary file and publishes path/savepoints only after replacement; staged open, recovery, and explicit compaction-copy rules are unchanged. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. Color chart generation uses the Core worker, shared Job Progress cancel, latest-token completion filtering, and a candidate/frequency/add-remove comparison before one targeted Apply; retry cancels older work and never retargets another document. The independent chart pane retains names, lock, page, and selection separately from the palette. Light Table, sequence autosave, geometry/filter preview, multi-window/session/view, workspace, diagnostics, brush, selection, scoped replacement, Batch, and DockHost routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry the mixed raster/vector render plan, view-local vector diagnostics, disconnected endpoints, and geometry preview. The Windows renderer distinguishes round and square vector cross-sections while preserving fills, centerline modes, AA state, and device-loss reconstruction. Raster-only changed-tile upload and canonical `revision-max` cache validation retain their scalar-only cache-hit path and unchanged source-call-graph lock. Fixed quick/full workloads, semantic counters, checksums, and approved ARM64 envelopes protect that boundary. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | Drawing, fill, independent Color chart generation/comparison, scoped color replacement, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `PAINT-002` | Raster/vector line, two-stage curve, rectangle, ellipse, regular polygon, click polyline, fill/outline, taper, constraints, cross-section, bounded ABI, Windows production gestures, current-only save/reopen, and M09 ARM64 user-visible confirmation | Guide/grid production snap remains M15/PM-GAP-013 |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto; Prompt/Autosave-before-switch setting; durable exact-native autosave and staged restore; dirty/history/editor preservation; async progress and failure-atomic production route; M11 ARM64 user-visible confirmation | Endpoint-loop preference control |
| `COLOR-CHART-PREVIEW-001` | Same-base noncumulative extraction, bounded candidates/frequencies/diff/overflow, immutable UUID/revision token, lock/stale/Cancel rejection, exact-name/cursor retention, one-step Undo/Redo and v19 save/reopen, owned ABI, asynchronous Windows Job Progress and comparison route | ARM64 user-visible generation/retry/Cancel/Apply/Undo/save-reopen confirmation |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with per-stack tabs, default lower-inspector grouping for Layer/Plane + Light Table + Subpalette/Reference, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

## Known differences

- Native `.inkpod` is current-only v19; non-v19 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V19 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete automatic verification is for M13 on 2026-08-11. Color
chart generation now compares a bounded immutable candidate against the
independent document chart before Apply. Repeated parameters always rescan the
same committed composite; exact existing colors retain their first name, new
colors receive deterministic `Color N` names, locked charts allow preview but
reject Apply, and a surviving selection retains its page/index. Apply is one
canonical Undo unit; Cancel, superseded work, a closed target, stale revision,
cross-document token misuse, overflow, and failure publish nothing. ARM64
user-visible confirmation remains before `COLOR-CHART-PREVIEW-001` can become
`Verified`. Endpoint looping remains deferred to M21 and guide/grid production
snap to M15.

| Boundary | Result |
| --- | --- |
| Rust workspace | 403 unit/integration tests plus one doctest, zero ignored; RGBA8/16 and alpha goldens, maximum/quantization boundaries, noncumulative preview, exact-name/cursor retention, success/no-op/invalid/Cancel/lock/stale/cross-document/overflow/failure atomicity, one-step Undo/Redo, replay, current save/reopen, v18 rejection, and FFI ownership/negative contracts passed. Public route inventory covers 250 Rust routes, 248 C ABI exports, and 349 Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V19/runtime replay epoch 16, ABI v9, EDIT schema 5, and `.inkbatch` v2 are current. `ReplaceColorChart/canonical-v1`, document digest schema 6, nested archive schema 2, catalog count/digest, exact top-level v18 rejection, current round-trip, replay, checkpoint, recovery, and compaction contracts passed |
| Windows ARM64 | 2026-08-11 Release configure/build passed with warnings denied and static CRT; all 31 CTests passed in 65.35 s. Coverage includes owned ABI negatives, asynchronous Job Progress, comparison/Apply, latest-wins cancellation, target-close race with no retarget/apply, palette separation, renderer/device reset, portable ZIP, and unsigned MSIX payload |
| Windows x64 | Not rerun for M12 because this invocation explicitly substituted the host ARM64 gate. The previous 2026-08-10 x64 Release M08 run passed all 30 then-current CTests |
| Performance | Required quick and additional full profiles passed all nine checksum/revision/history/reuse/output/failure gates. Protected snapshot checksums and counters remained unchanged, including quick/full `pan_zoom_snapshot` `517ed7ae78bf0487`/`439040e0244d5773` and `dirty_tile_rebuild` `9e13576def6f539b`/`a33f7534fcdd61e7`; canonical replay remained `f521d658a47051e9`. The intentional document-digest schema 6 commitment changed only `checkpoint_open` to `8847f8440d290c18` in both profiles while revision 3, history/output 256, reuse 1, success 1, and failure 0 remained fixed. M13 does not change benchmarked cache/payload algorithms, workload, harness, envelope, or `revision-max`, so a new five-process wall-clock comparison was not required |
| Fuzzing | `native_v19` and `native_core_v19` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

Semantic gates, active-envelope samples, and rebaseline rules live in
[`core-benchmark-baseline.md`](core-benchmark-baseline.md). Platform-specific
accessibility procedures live in
[`windows-release-checklist.md`](windows-release-checklist.md); superseded G13
observations are retained only in [`legacy.md`](legacy.md).

## Maintenance rule

Replace this snapshot when current state, active gaps, stable differences, or
representative verification changes. Do not append chronological logs. Update
[`compatibility.md`](compatibility.md) only when a requirement status, evidence,
or known difference changes.
