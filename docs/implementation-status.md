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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. Light Table bulk registration captures document/sequence/active-source identity, previews natural-order neighbors without mutation, skips an existing source UUID without updating it, resolves immutable source assets, and inserts the remaining block as one canonical transaction. Sequence autosave and existing geometry/filter preview retain their stale-safe staged contracts. Existing brush, scoped replacement, explicit vector topology/diagnostics, Batch, grouped edit targets, typed clipboard, exact-depth color, savepoint, Genesis, and asset contracts remain Core-owned. |
| Persistence | Native `.inkpod` is exact-current v18/runtime replay epoch 15 and `.inkbatch` is exact-current v2. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. `LightTableBulkRegister/canonical-v2` stores the target set and complete ordered resolved item/source-asset records, while sequence discovery and duplicate preview remain transient. Resolved geometry is canonical procedure schema 2, vector square cross-section is `VECT` schema 2, and EDIT schema 4 remains current. Non-v18 input, including v17, is rejected. Save streams to an adjacent temporary file and publishes path/savepoints only after replacement; staged open, recovery, and explicit compaction-copy rules are unchanged. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. The Light Table menu and pane expose previous/next/both-N registration with N/base/step input, bounded add/skip/order/opacity preview, explicit OK/Cancel, and one production Core apply. Issue-time session/generation and the Core request token prevent retargeting. Sequence autosave, geometry/filter preview, multi-window/session/view, workspace, diagnostics, brush, selection, scoped replacement, Batch, DockHost, and job-progress routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry the mixed raster/vector render plan, view-local vector diagnostics, disconnected endpoints, and geometry preview. The Windows renderer distinguishes round and square vector cross-sections while preserving fills, centerline modes, AA state, and device-loss reconstruction. Raster-only changed-tile upload and canonical `revision-max` cache validation retain their scalar-only cache-hit path and unchanged source-call-graph lock. Fixed quick/full workloads, semantic counters, checksums, and approved ARM64 envelopes protect that boundary. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | Drawing, fill, scoped color replacement, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `PAINT-002` | Raster/vector line, two-stage curve, rectangle, ellipse, regular polygon, click polyline, fill/outline, taper, constraints, cross-section, bounded ABI, Windows production gestures, current-only save/reopen, and M09 ARM64 user-visible confirmation | Guide/grid production snap remains M15/PM-GAP-013 |
| `LT-001`, `LT-003` | Target-aware Light Table sets/items plus previous/next/both-N registration with linear distance opacity, chronological z-order, UUID duplicate preservation, preview/Cancel, one-step Undo/Redo, replay/save-reopen, ABI and Windows production smoke | M12 is awaiting the user-visible ARM64 order/opacity/Undo/save-reopen check |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto; Prompt/Autosave-before-switch setting; durable exact-native autosave and staged restore; dirty/history/editor preservation; async progress and failure-atomic production route; M11 ARM64 user-visible confirmation | Endpoint-loop preference control |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with per-stack tabs, default lower-inspector grouping for Layer/Plane + Light Table + Subpalette/Reference, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

## Known differences

- Native `.inkpod` is current-only v18; non-v18 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V18 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete automatic verification is for M12 on 2026-08-11. Previous,
next, or both natural-sequence sides can be previewed and registered into the
active Light Table set with approved linear distance opacity, later-cell-above
z-order, and source-UUID duplicate preservation. The complete addition is one
canonical procedure and Undo unit. M12 is awaiting its user-visible ARM64 check;
M13 has not started. Endpoint looping remains intentionally deferred to M21 and
guide/grid production snap to M15.

| Boundary | Result |
| --- | --- |
| Rust workspace | 399 tests including one doctest, zero ignored; previous/next/both, N=0/1/upper, endpoints/gaps/one-cell, opacity 0/1000, duplicate source revision preservation, defaults/alignment, no-op/invalid/Cancel/stale/overflow/failure atomicity, one-step Undo/Redo, replay and save/reopen passed. Public route inventory covers 245 Rust routes, 240 C ABI exports, and 349 Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V18/runtime replay epoch 15, ABI v9, EDIT schema 4, and `.inkbatch` v2 are current. `LightTableBulkRegister/canonical-v2` resolves full immutable source assets and ordered item properties; catalog count/digest and exact v17 rejection passed together with replay, current-version round-trip, checkpoint, recovery, and compaction contracts |
| Windows ARM64 | 2026-08-11 Release configure/build passed with warnings denied and static CRT; all 31 CTests passed in 62.02 s, including caller-owned bulk ABI negatives, Light Table pane/menu preview Cancel/apply/duplicate no-op/Undo/Redo production route, renderer/device reset, portable ZIP, and unsigned MSIX payload |
| Windows x64 | Not rerun for M12 because this invocation explicitly substituted the host ARM64 gate. The previous 2026-08-10 x64 Release M08 run passed all 30 then-current CTests |
| Performance | Required quick and full profiles retained all nine checksum/revision/history/reuse/output/failure gates. Light Table composite remained quick `255ab9bad114dfdd` (revision 4, history 3, input/output 3/4) and full `77f63d83e130185f` (revision 7, history 6, input/output 6/16); canonical replay remained `f521d658a47051e9`. The bulk command does not change the benchmarked composite/snapshot algorithms, payload-access path, canonical `revision-max` formula, workload, harness, or envelopes, so the preceding approved M09 five-process ARM64 quick/full medians remain the applicable timing samples |
| Fuzzing | `native_v18` and `native_core_v18` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

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
