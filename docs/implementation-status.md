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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. `EditorState` owns a tree-ordered bounded multi-edit-target set independently from the active paint plane; grouped layer/plane commands and typed raster/vector clipboard commit as one transaction. Exact-depth colors, tool options, savepoints, deterministic numeric rules, immutable Genesis, and content-addressed assets remain Core-owned. New Cell preview/commit shares one bounded immutable plan covering image/frame sizing, six frames, five anchors, layer topology, RGBA8/16, and 1..64 independent cells. |
| Persistence | Native `.inkpod` is exact-current v11/replay epoch 8. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. EDIT schema 2 persists active target and ordered edit targets with exact ID validation. Genesis preserves the distinct Cell ID and six frame rectangles. Save streams to an adjacent temporary file and publishes path/savepoints after replacement. Open validates and replays in a staged Core. Explicit compaction writes a separate new Genesis and never changes the live session. |
| Windows frontend | UI/Input, Core engine, and Renderer are separate owner threads connected by bounded value/ownership queues. Multiple windows, sessions, views, split groups, per-view Canvas input transforms, single-instance activation, recovery, workspace presets, target-aware panes, and device-loss reconstruction are connected without a process-global active-document pointer. The Layer/Plane pane keeps active rows separate from Ctrl/Shift/Space edit-target markers, projects marker state into accessibility names/status, and uses the Core capability matrix for grouped menu state. New Cell uses one standard-control dialog whose live summary copies the Core-owned plan; its bounded multi-session route prepares every Core before frontend registry/tab publication and rolls back the complete batch on failure. All eight modeless dialog surfaces are hosted as DockHost panes; effect and Batch work share one transient Job Progress pane with independent cancellation. |
| Rendering and performance | Immutable snapshots carry a bottom-to-top mixed raster/vector render plan with layer groups and adjustment LUTs. Canvas, layer thumbnails, and flat export share layer/plane index-0-on-top semantics; the Windows renderer executes the plan without rasterizing editable vector geometry. Raster-only changed-tile upload and canonical `revision-max` cache validation retain their scalar-only cache-hit path. Fixed Core/native workloads, semantic counters, and approved environment envelopes protect that boundary. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | Drawing, fill, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `DOC-002`, `DOC-003` | Stable-ID typed topology plus tree-ordered multi-edit-target Core/ABI/Windows route, grouped commands, typed raster/vector copy-paste, marker/accessibility/status/menu presentation, persistence, replay and smoke | x64 Release manual grouped main-line/color paste, one-step Undo, and active-plane-only brush verification |
| `LT-001` | Target-aware Light Table sets/items, ordering, transform, color/mode/opacity, reference alignment, sampling, reload, edit-image swap, navigation, and reference viewer | Previous/next-N bulk registration and automatic opacity-step controls |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto, dirty-switch confirmation | Automatic-save-on-switch and endpoint-loop preference controls |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, five presets, V5 persisted bounded layout with V4 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

## Known differences

- Native `.inkpod` is current-only v11; non-v11 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V11 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete M03 automatic verification is dated 2026-08-10.

| Boundary | Result |
| --- | --- |
| Rust workspace | 349 tests including one doctest, zero ignored; `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V11/replay epoch 8 round-trip, noncurrent rejection, EDIT schema-2 target persistence, bounded streaming, checkpoint match/fallback/rejection, inactive-branch asset retention, staged open, failed replacement, recovery, and exact-confirmation compaction contracts passed |
| Windows ARM64 | Not rerun for M03; ARM64 remains optional and does not substitute for the completed x64 Release gate |
| Windows x64 | 2026-08-10 Release configure/build completed with warnings denied and static CRT; `ctest --preset windows-x64-release` passed all 30 tests, including ABI, production application, portable ZIP, MSIX payload, renderer, and CoreHost smoke routes |
| Performance | All nine quick checksum, revision, history, reuse/rebuild, and payload-access gates passed with the unchanged workload, harness, envelopes, and `revision-max` formula. M03 does not alter the render-cache hot path, so full and five-run wall-clock measurements were not required |
| Fuzzing | `native_v11` and `native_core_v11` target declarations are current. Fuzz binary build and coverage-guided execution were not run for M03 because the optional `cargo fuzz` subcommand is outside the required gate |

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
