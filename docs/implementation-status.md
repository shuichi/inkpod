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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. Bounded typed geometry resolves line, two-stage curve, rectangle, ellipse, regular polygon, and click polyline plus outline/fill, taper, 45-degree/aspect/center/rotation, round/square cross-section, and closure before canonical commit. Raster and vector backends share the gesture contract while keeping pixel and stable-object generation separate; preview begin/update is non-mutating, and commit alone consumes vector IDs. Existing brush, scoped replacement, explicit vector topology/diagnostics, Batch, grouped edit targets, typed clipboard, exact-depth color, savepoint, Genesis, and asset contracts remain Core-owned. |
| Persistence | Native `.inkpod` is exact-current v17/runtime replay epoch 14 and `.inkbatch` is exact-current v2. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. Resolved geometry is canonical procedure schema 2, including bounded Q16 segments, taper widths, fill boundary, target, and stable output IDs; vector square cross-section is persisted in `VECT` schema 2. Non-v17 input is rejected. Canonical procedures also retain scoped replacement and typed separation semantics; Batch v2 and EDIT schema 4 retain their existing exact validation. Save streams to an adjacent temporary file and publishes path/savepoints only after replacement; staged open, recovery, and explicit compaction-copy rules are unchanged. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. Raster and vector targets expose line, two-stage curve, rectangle, ellipse, polygon, and click/double-click polyline commands through shared command state, Core-owned active-tool state, Tool Options, and issue-time captured document/view/plane generations. Geometry preview is published through the normal immutable snapshot route; tool switches, Escape/Cancel, stale targets, and failed ingestion discard it without commit. Existing multi-window/session/view, workspace, diagnostics, brush, selection, scoped replacement, Batch, DockHost, and job-progress routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry the mixed raster/vector render plan, view-local vector diagnostics, disconnected endpoints, and geometry preview. The Windows renderer distinguishes round and square vector cross-sections while preserving fills, centerline modes, AA state, and device-loss reconstruction. Raster-only changed-tile upload and canonical `revision-max` cache validation retain their scalar-only cache-hit path and unchanged source-call-graph lock. Fixed quick/full workloads, semantic counters, checksums, and approved ARM64 envelopes protect that boundary. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | Drawing, fill, scoped color replacement, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `PAINT-002` | Raster/vector line, two-stage curve, rectangle, ellipse, regular polygon, click polyline, fill/outline, taper, constraints, cross-section, bounded ABI, Windows production gestures, and current-only save/reopen | M09 ARM64 user-visible confirmation; guide/grid production snap remains M15/PM-GAP-013 |
| `LT-001` | Target-aware Light Table sets/items, ordering, transform, color/mode/opacity, reference alignment, sampling, reload, edit-image swap, navigation, and reference viewer | Previous/next-N bulk registration and automatic opacity-step controls |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto, dirty-switch confirmation | Automatic-save-on-switch and endpoint-loop preference controls |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with per-stack tabs, default lower-inspector grouping for Layer/Plane + Light Table + Subpalette/Reference, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

## Known differences

- Native `.inkpod` is current-only v17; non-v17 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V17 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete M09 automatic verification is dated 2026-08-11. Raster and
vector geometry share one typed gesture/preview/commit contract through the
Windows production route. M09 user-visible ARM64 confirmation is pending, and
guide/grid production snap remains intentionally deferred to M15.

| Boundary | Result |
| --- | --- |
| Rust workspace | 391 tests including one doctest, zero ignored; `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V17/runtime replay epoch 14 and `.inkbatch` v2 current-only round-trip/noncurrent rejection; v16 and VECT schema-1 rejection; geometry preview/commit/replay/save-reopen plus stable vector output IDs and square cross-section; existing topology, scoped replacement, typed separation, EDIT schema-4, checkpoint, staged-open, failure/recovery, and compaction contracts passed |
| Windows ARM64 | 2026-08-11 Release configure/build passed with warnings denied and static CRT; all 30 CTests passed, including geometry command state/staged gestures, CoreHost/renderer/ABI negative routes, device-loss reconstruction, application smoke, portable ZIP, and unsigned MSIX payload |
| Windows x64 | Not rerun for M09 because this invocation explicitly substituted the host ARM64 gate. The previous 2026-08-10 x64 Release M08 run passed all 30 CTests |
| Performance | Every retained quick/full run kept all nine checksum/revision/history/reuse/output/failure gates. Vector snapshot remained `2813c527f27311c8`/`b975f3cfdb7824fd`, revisions 18/66, history 17/65, outputs 40/160, reuse 0, failures 0. After discarded warm-ups, five-process ARM64 medians were quick `pan_zoom_snapshot` 0.958 ms and `dirty_tile_rebuild` 1.908 ms, and full 14.789 ms and 8.874 ms, all inside the approved range; vector-snapshot medians were 826.730 ms quick and 3350.864 ms full. The `revision-max` formula/source-call-graph lock, workload, harness, and envelopes are unchanged |
| Fuzzing | `native_v17` and `native_core_v17` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

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
