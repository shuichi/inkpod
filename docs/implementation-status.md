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
| Persistence | Native `.inkpod` is exact-current v21/runtime replay epoch 18 and `.inkbatch` is exact-current v2. `META/GENS/ASST/PROC/EDIT` are authoritative; optional `CKPT` only accelerates open. `CommitFloating/canonical-v3` persists the five-anchor absolute-target transform; `SelectOutputColorGuard/canonical-v1` persists profile, selection operation, and base revision; `DocumentStateDigest` remains schema 6 and EDIT remains schema 5. Non-v21 input, including v20, is rejected. Save streams to an adjacent temporary file and publishes path/savepoints only after replacement; staged open, recovery, and explicit compaction-copy rules are unchanged. |
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
| `XFORM-003` | Half-open five-anchor absolute X/Y, anchor-pivot scale→clockwise-rotate→position Core/canonical-v3/ABI v10/Windows implementation; automatic Core, ABI, renderer and production smoke verification | x64 user-visible five-anchor dialog/handle, Cancel and Undo confirmation |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto; Prompt/Autosave-before-switch setting; durable exact-native autosave and staged restore; dirty/history/editor preservation; async progress and failure-atomic production route; M11 ARM64 user-visible confirmation | Endpoint-loop preference control |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with resource-titled singleton inspector headers, visible system-color 4-DIP splitters, an accessible keyboard-resizable Layer/Plane split, and an explicit Layer/Plane action target; default lower-inspector grouping, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

M16／`PM-GAP-017` is waiting for manual confirmation as `XFORM-003`. The
implemented contract uses absolute document X/Y for the selected
half-open-bounds anchor, that anchor as the scale/rotation pivot, local scale
then clockwise rotation then placement, and deterministic
destination-pixel-centre inverse mapping. Core raster/vector execution,
canonical-v3 replay, ABI v10, numeric dialog, Canvas handles and renderer use
the same noncumulative transform.

## Known differences

- Native `.inkpod` is current-only v21; non-v21 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V21 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete automatic verification is for M16 on 2026-08-12. The five
half-open anchors now drive absolute placement, nonuniform scale and positive or
negative rotation through the same Core raster/vector, canonical-v3, ABI v10,
Windows dialog/handle and renderer path. Preview retries always recompute from
the original floating content; invalid, overflow, Cancel and stale paths publish
nothing; commit is one Undo/Redo unit and replays after v21 save/reopen. The Help
menu also exposes `Inkpodウェブページ`, whose production handler opens
`https://shuichi.github.io/inkpod/` through the Windows shell adapter. M15's x64
user-visible snap confirmation remains successful. Endpoint looping remains
deferred to M21. M16 itself remains `In progress` until the x64 user-visible
five-anchor confirmation succeeds.

| Boundary | Result |
| --- | --- |
| Rust workspace | 423 unit/integration tests plus one doctest, zero ignored; all five anchors, uniform/nonuniform scale, positive/negative rotation, raster bounds/pixels, vector geometry, same-base retry, invalid/overflow/Cancel/stale atomicity, Undo/Redo, replay and v21 save/reopen passed. ABI coverage includes short structure and unknown anchor with no partial publication. Public route inventory covers 258 Rust routes, 255 C ABI exports, and 354 Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V21/runtime replay epoch 18, ABI v10, `CommitFloating/canonical-v3`, EDIT schema 5, and `.inkbatch` v2 are current. Exact top-level v20 rejection and current floating-transform save/reopen/replay are green |
| Windows x64 | 2026-08-12 Release configure/build passed with warnings denied and static CRT; all 31 CTests passed in 66.61 s. Native smoke covers five-anchor dialog values affecting the committed transform, same-base Canvas move/scale/rotation handles, Cancel and Undo/Redo, renderer/device reset, the Help web-page command without launching a browser in smoke mode, portable ZIP, and unsigned MSIX payload |
| Windows ARM64 | Not run for M16 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | Required quick and supplemental full profiles passed all ten fixed checksum/revision/history/reuse/rebuild/output/failure gates. `canonical_replay` produced `f521d658a47051e9` in both profiles at revision 6/history 5 with five successes and zero failures; quick cache-hit pan/zoom reused all eight items and dirty-tile rebuild reused 224 of 256 inputs. No workload, harness, envelope, render-cache formula, payload-access path or approved wall-clock scenario changed, so no envelope remeasurement was applicable |
| Fuzzing | `native_v21` and `native_core_v21` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

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
