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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. Sequence autosave captures immutable source/target UUID, source generation, and document/editor revisions; commit rejects stale requests, while staged native restore replaces the live Core only after full validation/replay and retains dirty, history, journal, selection, and editor state. Bounded typed geometry and filter preview retain their existing non-mutating preview/one-commit contracts. Existing brush, scoped replacement, explicit vector topology/diagnostics, Batch, grouped edit targets, typed clipboard, exact-depth color, savepoint, Genesis, and asset contracts remain Core-owned. |
| Persistence | Native `.inkpod` is exact-current v17/runtime replay epoch 14 and `.inkbatch` is exact-current v2. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. Sequence-cell autosave reuses the exact current native recovery container and sidecar metadata without adopting its path or advancing the live normal savepoint. Resolved geometry is canonical procedure schema 2 and vector square cross-section is `VECT` schema 2; canonical procedures also retain scoped replacement and typed separation semantics, while EDIT schema 4 remains current. Non-v17 input is rejected. Save streams to an adjacent temporary file and publishes path/savepoints only after replacement; staged open, recovery, and explicit compaction-copy rules are unchanged. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. A versioned `Prompt`/`Autosave-before-switch` application setting routes sequence navigation through one immutable Core request. `DocumentSession` owns bounded exact UUID/generation recovery associations; the CoreHost lane writes recovery plus metadata asynchronously, disables navigation while pending, and publishes active-cell/UI/path state only on success. Returning to a cell stages its associated native artifact instead of reconstructing it from the flattened thumbnail source. Geometry and filter preview retain their shared command-state, issue-time target, immutable-snapshot, debounce/latest-wins, Job Progress, OK, and Cancel contracts. Existing multi-window/session/view, workspace, diagnostics, brush, selection, scoped replacement, Batch, DockHost, and job-progress routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry the mixed raster/vector render plan, view-local vector diagnostics, disconnected endpoints, and geometry preview. The Windows renderer distinguishes round and square vector cross-sections while preserving fills, centerline modes, AA state, and device-loss reconstruction. Raster-only changed-tile upload and canonical `revision-max` cache validation retain their scalar-only cache-hit path and unchanged source-call-graph lock. Fixed quick/full workloads, semantic counters, checksums, and approved ARM64 envelopes protect that boundary. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | Drawing, fill, scoped color replacement, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `PAINT-002` | Raster/vector line, two-stage curve, rectangle, ellipse, regular polygon, click polyline, fill/outline, taper, constraints, cross-section, bounded ABI, Windows production gestures, current-only save/reopen, and M09 ARM64 user-visible confirmation | Guide/grid production snap remains M15/PM-GAP-013 |
| `LT-001` | Target-aware Light Table sets/items, ordering, transform, color/mode/opacity, reference alignment, sampling, reload, edit-image swap, navigation, and reference viewer | Previous/next-N bulk registration and automatic opacity-step controls |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto; Prompt/Autosave-before-switch setting; durable exact-native autosave and staged restore; dirty/history/editor preservation; async progress and failure-atomic production route | M11 ARM64 user-visible confirmation and endpoint-loop preference control |
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

The latest complete automatic verification is for M11 on 2026-08-11. Sequence
navigation now offers Prompt or Autosave-before-switch; autosave publishes an
exact native recovery artifact before switching and restores that artifact when
the cell is revisited without adopting a normal path/savepoint. M10 user-visible
ARM64 confirmation is complete. M11 user-visible ARM64 confirmation is pending;
endpoint looping remains intentionally deferred to M22 and guide/grid production
snap to M15.

| Boundary | Result |
| --- | --- |
| Rust workspace | 394 tests including one doctest, zero ignored; new exact autosave/switch/staged-restore and no-op/invalid/stale/Undo/Redo contracts passed; public route inventory includes all three new Core/FFI routes; `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | M11 changes neither serialized schema nor canonical procedure/replay semantics, so V17/runtime replay epoch 14, ABI v9, EDIT schema 4, and `.inkbatch` v2 remain current. Exact native autosave restore preserves document digest, history, journal, selection/editor state, dirty/recovered status, and prior normal-file bytes; existing current-only round-trip and noncurrent rejection contracts passed |
| Windows ARM64 | 2026-08-11 Release fresh configure/build passed with warnings denied and static CRT; all 31 CTests passed, including versioned sequence-policy codec, UUID+source-generation artifact paths, bounded DocumentSession associations, pending command state, ABI negatives, exact production autosave/switch/back restore, CoreHost close/shutdown/queue routes, portable ZIP, and unsigned MSIX payload |
| Windows x64 | Not rerun for M11 because this invocation explicitly substituted the host ARM64 gate. The previous 2026-08-10 x64 Release M08 run passed all 30 then-current CTests |
| Performance | The required M11 quick benchmark retained all nine checksum/revision/history/reuse/output/failure gates; `dirty_tile_rebuild` remained `9e13576def6f539b` with revision 41, history 40, input/output/reuse 256/32/224, and `vector_snapshot` remained `2813c527f27311c8` with revision 18, history 17, output 40, reuse 0, failures 0. M11 does not change benchmarked render/edit paths, so no full or five-process timing gate was required. The preceding M09 five-process ARM64 quick/full medians remain the latest approved timing samples. The `revision-max` formula/source-call-graph lock, workload, harness, and envelopes are unchanged |
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
