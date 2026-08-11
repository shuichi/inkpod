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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. Geometry pointer input now uses a read-only view-targeted resolver that performs the one device/document conversion, inclusive paper bounds, deterministic grid/guide precedence and Ctrl bypass before the existing preview/canonical executor. Stale or closed views fail without fallback or publication. Locator reads remain non-mutating and select active-stroke preview, filter preview, then committed document while the public eyedropper remains committed-only. Selection bounds scan only allocated mask tiles and are cached by document identity/revision. Existing output-color guard, Color chart, Light Table bulk registration, sequence autosave, brush, scoped replacement, explicit vector topology/diagnostics, Batch, grouped edit targets, typed clipboard, savepoint, Genesis, and asset contracts remain Core-owned. |
| Persistence | Native `.inkpod` is exact-current v20/runtime replay epoch 17 and `.inkbatch` is exact-current v2. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. `SelectOutputColorGuard/canonical-v1` persists profile, selection operation, and base revision; `DocumentStateDigest` remains schema 6 and EDIT remains schema 5. Non-v20 input, including v19, is rejected. Save streams to an adjacent temporary file and publishes path/savepoints only after replacement; staged open, recovery, and explicit compaction-copy rules are unchanged. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. Geometry gestures pass bounded device-pixel samples plus the procedure-captured Core view ID/revision and Ctrl bypass bit through the additive C ABI; C++ no longer performs geometry snap or device-to-document math. Menu checked state, all M09 line/curve/shape/polyline routes, preview, canonical commit and renderer preview share that path. Accepted mouse/pen raster packets still update Locator coordinates after enqueue and coalesce reads. Docked inspector singletons retain localized headers and the shared layer action row names its target visually and through MSAA. The output-color guard remains labelled as a conservative guard rather than standards conformance. Existing Color chart, Light Table, sequence autosave, filter preview, multi-window/session/view, workspace, diagnostics, brush, selection, scoped replacement, Batch, and DockHost routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry the mixed raster/vector render plan, view-local vector diagnostics, disconnected endpoints, and geometry preview. The Windows renderer distinguishes round and square vector cross-sections while preserving fills, centerline modes, AA state, and device-loss reconstruction. Raster-only changed-tile upload and canonical `revision-max` cache validation retain their scalar-only cache-hit path and unchanged source-call-graph lock. Fixed quick/full workloads, semantic counters, checksums, approved ARM64 revision-max envelopes, and the approved x64 output-color-guard envelope protect those boundaries. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | Drawing, fill, independent Color chart generation/comparison, output-color guard selection, scoped color replacement, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `SNAP-001` | View-targeted Core conversion/snap contract; bounded ABI; all M09 Windows geometry gestures; checked state, result geometry/digest, off/Ctrl bypass and Undo/Redo production smoke; x64 Release automatic gate | M15 x64 user-visible line/rectangle/polyline, bypass and zoom confirmation |
| `PAINT-002` | Raster/vector line, two-stage curve, rectangle, ellipse, regular polygon, click polyline, fill/outline, taper, constraints, cross-section, bounded ABI, Windows production gestures, current-only save/reopen, M09 ARM64 geometry confirmation, and M15 production snap automatic coverage | M15/SNAP-001 x64 user-visible confirmation |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto; Prompt/Autosave-before-switch setting; durable exact-native autosave and staged restore; dirty/history/editor preservation; async progress and failure-atomic production route; M11 ARM64 user-visible confirmation | Endpoint-loop preference control |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with resource-titled singleton inspector headers, visible system-color 4-DIP splitters, an accessible keyboard-resizable Layer/Plane split, and an explicit Layer/Plane action target; default lower-inspector grouping, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

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

The latest complete automatic verification is for M15 on 2026-08-11. The M14
x64 profile/overlay/pixel-invariance/Undo confirmation succeeded, so
`COLOR-OUTPUT-QA-001` is now `Verified`. M15 fixes the guide/grid snap contract
in `SPEC.md`: one view-targeted device/document conversion, grid then guide
precedence, inclusive four-document-pixel guide threshold, stable position/ID
tie-break, inclusive paper bounds, and pointer-down Ctrl bypass. Core resolves
the bounded span without mutation; the additive ABI retains caller ownership
and rejects short/unknown/stale/overflow/invalid cases without partial output.
All M09 Windows geometry gestures use the same resolver and production smoke
observes checked state, snapped/raw geometry, canonical digest, and Undo/Redo.
`SNAP-001` and `PAINT-002` remain `Experimental` only until the pending x64
user-visible line/rectangle/polyline, bypass, and zoom confirmation. Endpoint
looping remains deferred to M21.

| Boundary | Result |
| --- | --- |
| Rust workspace | 419 unit/integration tests plus one doctest, zero ignored; grid, horizontal/vertical guide, precedence, tie, off, Ctrl bypass, outside/far bounds, extreme zoom/pan, both flips, DPI equality, secondary/closed/stale view, non-mutation, applied geometry and Undo/Redo contracts passed. ABI coverage includes short/unknown enum/unknown flag/invalid value/overflow/stale view/short output/undersized buffer/NULL with no partial writes. Existing save/reopen, replay and geometry Cancel/failure contracts remain green. Public route inventory covers 258 Rust routes, 255 C ABI exports, and 353 Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V20/runtime replay epoch 17, ABI v9, EDIT schema 5, and `.inkbatch` v2 remain current. M15 changes no serialized schema, canonical procedure semantics, replay result, or application-specific persistence; exact top-level v19 rejection and current geometry save/reopen/replay remain green |
| Windows x64 | 2026-08-11 Release configure/build passed with warnings denied and static CRT; all 31 CTests passed in 69.70 s. Native smoke covers snap menu checked state, snapped/off/Ctrl-bypassed line geometry, canonical digest, Undo/Redo, all existing line/curve/rectangle/ellipse/polygon/polyline gestures, renderer/device reset, portable ZIP, and unsigned MSIX payload |
| Windows ARM64 | Not run for M15 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | Required quick passed all ten fixed checksum/revision/history/reuse/rebuild/output/failure gates. Representative protected checksums remain `517ed7ae78bf0487` (`pan_zoom_snapshot`), `9e13576def6f539b` (`dirty_tile_rebuild`), and `ed208415c7582547` (`output_color_guard`); cache-hit pan/zoom reused all eight items and existing payload/revision gates remained green. M15 adds only a bounded-at-256 input query and changes no benchmark workload, harness, envelope, render-cache formula, payload access path, or approved wall-clock scenario, so no envelope remeasurement or full-workload change was applicable |
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
