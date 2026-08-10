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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. `EditorState` owns a tree-ordered bounded multi-edit-target set and brush shape/smoothing/start-color options independently from the active paint plane. Raster brush execution uses round/square pressure footprints, causal fixed-point smoothing, the existing selection clip, and immutable exact native-depth start-color comparison including alpha. Scoped color replacement uses pen/rectangle/polyline/lasso regions intersected with selection, exact native-depth raster matching, and explicit whole stable vector path/fill units. Vector connections are explicit stable path/endpoint topology; view-local diagnostic flags expose only truly unconnected endpoints without mutating the document. Batch owns bounded enabled seed/pair rows, document UUID plus source-generation identity, exact two-cell extraction with explicit one-to-many resolution, typed separation destinations, and rejection of unresolved per-run configuration. Grouped layer/plane commands and typed raster/vector clipboard commit as one transaction; raster selection retains typed interpretation and construction options. Exact-depth colors, savepoints, deterministic numeric rules, immutable Genesis, and content-addressed assets remain Core-owned. |
| Persistence | Native `.inkpod` is exact-current v16/runtime replay epoch 13 and `.inkbatch` is exact-current v2. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. Genesis persists bounded explicit vector connections in `VECT` schema 2. Canonical procedures persist scoped replacement and typed separation semantics; Batch v2 retains row enablement, exact colors, destinations, source identity, and per-run configuration intent. EDIT schema 4 persists brush shape/smoothing/start-color predicate, active target, ordered edit targets, and selection interpretation/construction options with exact validation. Genesis preserves the distinct Cell ID and six frame rectangles. Save streams to an adjacent temporary file and publishes path/savepoints after replacement. Open validates and replays in a staged Core. Explicit compaction writes a separate new Genesis and never changes the live session. |
| Windows frontend | UI/Input, Core engine, and Renderer are separate owner threads connected by bounded value/ownership queues. Multiple windows, sessions, views, split groups, per-view Canvas input transforms, single-instance activation, recovery, workspace presets, target-aware panes, and device-loss reconstruction are connected without a process-global active-document pointer. Vector antialias, centerline overlay/only, and disconnected-endpoint commands share menu, shortcut, checked-state, and issue-time view ID/generation routing. Dock zones are one-direction splits of tab stacks, so revealing or selecting a pane cannot collapse sibling split stacks; the coloring workspace keeps Color above the Layer/Plane + Light Table + Subpalette/Reference tab stack. Tool Options projects Core-owned brush shape, 0–1000 smoothing, and exact start-color state and sends typed EditorState updates; Canvas stroke issue captures those values at begin. The Layer/Plane pane keeps active rows separate from edit-target markers, and Selection Options exposes typed range/geometry values with Cancel-safe preview. Scoped color replacement captures the current target color and routes all four region gestures. Batch provides multi-row seed/pair editing, two-cell selection and ambiguity prompts, typed separation destinations, and an enqueue-time immutable run graph; loaded presets retain their per-run prompts without mutating the saved graph. Closed-region and fill-extension drags publish a non-mutating rectangle preview. All eight modeless dialog surfaces are hosted as DockHost panes; effect and Batch work share one transient Job Progress pane with independent cancellation. |
| Rendering and performance | Immutable snapshots carry a bottom-to-top mixed raster/vector render plan with layer groups and adjustment LUTs plus view-local vector diagnostic flags and bounded disconnected endpoint records. Canvas, layer thumbnails, and flat export share layer/plane index-0-on-top semantics; the Windows renderer executes editable vector geometry with actual AA on/off, preserves fills in centerline-only mode, and draws centerlines/endpoints at device-pixel sizes. Device-loss reconstruction retains the same diagnostic snapshot. Raster-only changed-tile upload and canonical `revision-max` cache validation retain their scalar-only cache-hit path and unchanged source-call-graph lock. Fixed Core/native workloads, semantic counters, and approved environment envelopes protect that boundary. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | Drawing, fill, scoped color replacement, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `VIEW-005` | View-local vector AA, centerline overlay/only, explicit-topology disconnected endpoints, bounded ABI span, and Windows Direct2D/menu/shortcut routes | x64 Release user-visible behavior confirmation |
| `LT-001` | Target-aware Light Table sets/items, ordering, transform, color/mode/opacity, reference alignment, sampling, reload, edit-image swap, navigation, and reference viewer | Previous/next-N bulk registration and automatic opacity-step controls |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto, dirty-switch confirmation | Automatic-save-on-switch and endpoint-loop preference controls |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with per-stack tabs, default lower-inspector grouping for Layer/Plane + Light Table + Subpalette/Reference, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

## Known differences

- Native `.inkpod` is current-only v16; non-v16 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V16 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete M08 automatic verification is dated 2026-08-10. Typed
view-local vector diagnostics, explicit stable endpoint topology, and
device-pixel renderer presentation are fixed in `SPEC.md`; x64 Release manual
behavior confirmation is pending.

| Boundary | Result |
| --- | --- |
| Rust workspace | 383 tests including one doctest, zero ignored; `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V16/runtime replay epoch 13 and `.inkbatch` v2 current-only round-trip/noncurrent rejection; v15 and VECT schema-1 rejection; explicit vector connection save/reopen/Undo/Redo; scoped replacement and typed separation procedure/replay; EDIT schema-4 persistence; bounded streaming, checkpoint match/fallback/rejection, staged open, failed replacement, recovery, and compaction contracts passed |
| Windows ARM64 | Not rerun for M08; ARM64 remains optional and does not substitute for the completed x64 Release gate |
| Windows x64 | 2026-08-10 Release configure/build completed with warnings denied and static CRT; `ctest --preset windows-x64-release` passed all 30 tests, including view-local flags, AA on/off pixel difference, centerline/endpoint pixels, shortcut/checked state, device-loss reconstruction, ABI ownership/negative cases, portable ZIP, and MSIX payload routes; manual behavior confirmation remains pending |
| Performance | All nine mandatory quick checksum/revision/history/reuse/output/failure gates passed. The unchanged quick/full vector-snapshot scenario kept checksums `2813c527f27311c8`/`b975f3cfdb7824fd`, revisions 18/66, history 17/65, outputs 40/160, reuse 0, and failures 0 in every retained run. After discarded warm-ups, five-process wall-clock medians were 799.403 ms quick and 3219.557 ms full. The `revision-max` formula, source-call-graph lock, workload, harness, and envelopes are unchanged. This x64 host does not match the approved ARM64 Parallels envelope, so wall-clock is diagnostic |
| Fuzzing | `native_v16` and `native_core_v16` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

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
