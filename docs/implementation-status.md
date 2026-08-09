# Implementation status

This document contains only the current implementation summary, active gaps,
stable known differences, and latest representative verification. Product
requirements are defined in [`../SPEC.md`](../SPEC.md), requirement status and
evidence in [`compatibility.md`](compatibility.md), and ownership/data flow in
[`architecture.md`](architecture.md). Completed plans and superseded results
belong to Git history.

## Current implementation

| Area | Current state |
| --- | --- |
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. `EditorState`, stable targets, exact-depth colors, tool options, savepoints, deterministic numeric rules, immutable Genesis, and content-addressed assets are Core-owned. |
| Persistence | Native `.inkpod` is exact-current v9/replay epoch 6. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. Save streams to an adjacent temporary file and publishes path/savepoints after replacement. Open validates and replays in a staged Core. Explicit compaction writes a separate new Genesis and never changes the live session. |
| Windows frontend | UI/Input, Core engine, and Renderer are separate owner threads connected by bounded value/ownership queues. Multiple windows, sessions, views, split groups, per-view Canvas input transforms, single-instance activation, recovery, workspace presets, target-aware panes, and device-loss reconstruction are connected without a process-global active-document pointer. All eight modeless dialog surfaces are hosted as DockHost panes; effect and Batch work share one transient Job Progress pane with independent cancellation. |
| Rendering and performance | Immutable snapshots and changed-tile upload are implemented. Canonical `revision-max` cache validation reads only source revision scalars; cache-hit zoom/pan performs no raster payload access. Fixed Core/native workloads, semantic counters, and approved environment envelopes protect the recovered performance boundary. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | Drawing, fill, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |

## Active gaps

Only the following requirements are `In progress`; all others in
[`compatibility.md`](compatibility.md) are `Verified`.

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `DOC-002`, `DOC-003` | Stable-ID typed layer/plane topology, transactional operations, selection, visibility/editability, metadata, thumbnails, and drag reorder | Multi-target editing presentation |
| `LT-001` | Target-aware Light Table sets/items, ordering, transform, color/mode/opacity, reference alignment, sampling, reload, edit-image swap, navigation, and reference viewer | Previous/next-N bulk registration and automatic opacity-step controls |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto, dirty-switch confirmation | Automatic-save-on-switch and endpoint-loop preference controls |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, five presets, V5 persisted bounded layout with V4 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

## Known differences

- Native `.inkpod` is current-only v9; non-v9 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Vector content currently composites after precomposited raster tiles rather
  than supporting arbitrary raster/vector interleaving.
- Batch output currently writes native `.inkpod` only.
- V9 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete procedure-history/persistence verification is dated
2026-08-07.

| Boundary | Result |
| --- | --- |
| Rust workspace | 333 tests including one doctest, zero ignored; `fmt`, all-target/all-feature Clippy with warnings denied, strict rustdoc, and all nine quick/full benchmark scenarios passed |
| Native format | V9 round-trip, current-only rejection, bounded streaming, checkpoint match/fallback/rejection, inactive-branch asset retention, staged open, failed replacement, recovery, and exact-confirmation compaction contracts passed |
| Windows ARM64 | Fresh Debug/Release builds completed 111 targets with static CRT, portable ZIP, and unsigned MSIX; final Debug CTest passed 28/28, including ABI and GUI smoke |
| Windows modeless panes | 2026-08-08 ARM64 Debug build completed with warnings denied, static CRT, portable ZIP, and unsigned MSIX; Debug CTest passed 30/30, including DockHost/layout persistence and 196-second GUI smoke |
| Performance | Quick pan/dirty/replay/checkpoint medians: `0.713125/1.797375/1.020958/28.065125 ms`; full: `12.289834/8.550750/1.218625/116.759334 ms`. Release native smoke retained 512 wheel events/512 Presents and 16 strokes/544 samples/16 Presents |
| Fuzzing | `native_v9` and `native_core_v9` fuzz binaries compile. Coverage-guided execution was not run because the host did not have the optional `cargo fuzz` subcommand installed |

Exact samples, envelopes, and rebaseline rules live in
[`core-benchmark-baseline.md`](core-benchmark-baseline.md). Platform-specific
accessibility evidence lives in
[`windows-g13-release-checklist.md`](windows-g13-release-checklist.md).

## Maintenance rule

Replace this snapshot when current state, active gaps, stable differences, or
representative verification changes. Do not append chronological logs. Update
[`compatibility.md`](compatibility.md) only when a requirement status, evidence,
or known difference changes.
