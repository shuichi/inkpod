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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. The output-color guard scans only the committed visible straight-alpha composite at exact RGBA16 depth, skips transparent pixels, and combines a sparse binary mask with existing selection as one stale/cancel-safe transaction. The independent Color chart, Light Table bulk registration, sequence autosave, and geometry/filter preview retain their contracts. Existing brush, scoped replacement, explicit vector topology/diagnostics, Batch, grouped edit targets, typed clipboard, savepoint, Genesis, and asset contracts remain Core-owned. |
| Persistence | Native `.inkpod` is exact-current v20/runtime replay epoch 17 and `.inkbatch` is exact-current v2. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. `SelectOutputColorGuard/canonical-v1` persists profile, selection operation, and base revision; `DocumentStateDigest` remains schema 6 and EDIT remains schema 5. Non-v20 input, including v19, is rejected. Save streams to an adjacent temporary file and publishes path/savepoints only after replacement; staged open, recovery, and explicit compaction-copy rules are unchanged. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. `選択範囲 > 出力色安全ガード...` captures the issue-time document target/revision, runs the Core task with shared Job Progress cancel, and reports scanned/selected/transparent counts; it labels the sole profile as a conservative guard rather than standards conformance. The default profile uses a versioned closed HKCU application setting and is not document state. Existing Color chart, Light Table, sequence autosave, geometry/filter preview, multi-window/session/view, workspace, diagnostics, brush, selection, scoped replacement, Batch, and DockHost routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry the mixed raster/vector render plan, view-local vector diagnostics, disconnected endpoints, and geometry preview. The Windows renderer distinguishes round and square vector cross-sections while preserving fills, centerline modes, AA state, and device-loss reconstruction. Raster-only changed-tile upload and canonical `revision-max` cache validation retain their scalar-only cache-hit path and unchanged source-call-graph lock. Fixed quick/full workloads, semantic counters, checksums, and approved ARM64 envelopes protect that boundary. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | Drawing, fill, independent Color chart generation/comparison, output-color guard selection, scoped color replacement, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `PAINT-002` | Raster/vector line, two-stage curve, rectangle, ellipse, regular polygon, click polyline, fill/outline, taper, constraints, cross-section, bounded ABI, Windows production gestures, current-only save/reopen, and M09 ARM64 user-visible confirmation | Guide/grid production snap remains M15/PM-GAP-013 |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto; Prompt/Autosave-before-switch setting; durable exact-native autosave and staged restore; dirty/history/editor preservation; async progress and failure-atomic production route; M11 ARM64 user-visible confirmation | Endpoint-loop preference control |
| `COLOR-OUTPUT-QA-001` | Conservative BT.709 Y′CbCr guard kernel; native-depth visible composite scan; transparent skip; sparse selection algebra; progress/Cancel/stale/no-op/Undo/Redo/replay/save-reopen; caller-owned ABI; Windows production command and summary | x64 Release user-visible confirmation remains |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with per-stack tabs, default lower-inspector grouping for Layer/Plane + Light Table + Subpalette/Reference, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

## Known differences

- Native `.inkpod` is current-only v20; non-v20 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V20 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete automatic verification is for M14 on 2026-08-11. The sole
closed profile is explicitly labelled an inkpod conservative BT.709 Y′CbCr QA
guard, not standards conformance or automatic legalization. It scans the
committed visible straight-alpha composite, excludes solid paper and view
overlays, skips alpha zero, preserves native RGBA16 raster/vector values, and
uses exact rational half-up thresholds. New/Add/Subtract/Intersect commit only
the sparse selection as one canonical Undo unit. No-op, invalid, Cancel, stale,
overflow, and failure publish nothing; replay and v20 save/reopen retain the
profile and result. Core/FFI/Windows production tests, the persisted application
default, and the required x64 Release gate are complete. `COLOR-OUTPUT-QA-001`
remains `Experimental` until
the pending x64 user-visible overlay/pixel-invariance/Undo confirmation.
Endpoint looping remains deferred to M21 and guide/grid production snap to M15.

| Boundary | Result |
| --- | --- |
| Rust workspace | 412 unit/integration tests plus one doctest, zero ignored; BT.709 in/boundary/out and half-up goldens, RGBA8/16 raster/vector equivalence, alpha-zero skip, visible/hidden composite, sparse large-image allocation, selection algebra, success/no-op/invalid/Cancel/stale/overflow/failure atomicity, one-step Undo/Redo, replay, v20 save/reopen, v19 rejection, and FFI ownership/negative contracts passed. Public route inventory covers 252 Rust routes, 249 C ABI exports, and 350 Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V20/runtime replay epoch 17, ABI v9, EDIT schema 5, and `.inkbatch` v2 are current. `SelectOutputColorGuard/canonical-v1`, the 82-entry catalog count/digest, exact top-level v19 rejection, current round-trip/replay, document digest schema 6, checkpoint, recovery, and compaction contracts passed |
| Windows x64 | 2026-08-11 Release configure/build passed with warnings denied and static CRT; all 31 CTests passed in 84.42 s. Coverage includes output-color guard command/task/status production smoke, versioned default-profile setting codec, caller-owned ABI negatives, renderer/device reset, portable ZIP, and unsigned MSIX payload |
| Windows ARM64 | Not run for M14 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | Required quick and additional full profiles passed all nine checksum/revision/history/reuse/rebuild/output/failure gates. Every approved workload checksum remained unchanged, including quick/full `pan_zoom_snapshot` `517ed7ae78bf0487`/`439040e0244d5773`, `dirty_tile_rebuild` `9e13576def6f539b`/`a33f7534fcdd61e7`, `canonical_replay` `f521d658a47051e9`, and `checkpoint_open` `8847f8440d290c18`. M14 does not change those workload, harness, envelope, cache/payload algorithms, or `revision-max`; the new guard scan has no approved wall-clock envelope, so no timing approval or five-run rebaseline is claimed |
| Fuzzing | `native_v20` and `native_core_v20` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

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
