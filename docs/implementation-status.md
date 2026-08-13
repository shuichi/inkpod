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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. The read-only history-visualization query replays every retained branch, emits `Commit` rows in journal-event order, formats bounded typed arguments, and renders deterministic post-commit thumbnails without changing live state. Bounded stable-ID vanishing points support Canvas-exterior coordinates, exact RGBA8/16, opacity and visibility, arbitrary 1–180 degree intervals and phase, canonical CRUD, noncumulative preview/Cancel, deterministic viewport-clipped radial guides, guide/radial/grid snap precedence, and document transforms. Unequal-axis resampling is rejected atomically because it cannot preserve an equal-angle radial family. Sequence endpoint and shooting-frame owners retain their existing contracts. Text/Annotation and the separate Cut, geometry, output-color guard, Color chart, Light Table, brush, replacement, vector, Batch, clipboard, Genesis, and asset contracts retain their existing owners. |
| Persistence | Native `.inkpod` is exact-current v26/runtime replay epoch 23 and `.inkbatch` is exact-current v2. Cell and Cut `.inkpod` files use distinct magic values; Cut payload schema 2 remains current. Cell `DocumentArchive`/document metadata schema 5 persists the optional shooting frame, bounded annotation records, and bounded vanishing-point records; `DocumentStateDigest` schema 9/domain 8 covers the new semantic fields, while canonical snapshot-composite schema 3 is unchanged. `META/GENS/ASST/PROC/EDIT` remain authoritative and optional `CKPT` only accelerates open. Non-v26 input, including exact v25, is rejected without migration. Atomic Cell/Cut savepoint rules are unchanged. The application-wide sequence endpoint preference remains a separate bounded HKCU record at version 1. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. Tools > Inkpod file visualization dynamically lists open native-session paths and opens one modeless, cancellable, owner-data three-column history list per document without blocking the UI thread. Cell > Vanishing Point exposes typed create/update properties, handle-edit mode, and delete-all; the existing Layer pane owns VanishingPoint layers, while Canvas drags use begin/update/apply/Cancel preview calls. Production previous/next sequence commands retain the Core step-plan and Stop/Wrap preference route. Cell > Shooting Frame and File > Export Instruction Image retain their prior contracts. Cut properties, save, history, and structural sequence commands capture the issue-time workspace/session/generation and Cut revision, reject stale or mismatched document targets, and never fall back to a later active Cell. Existing annotation, geometry, pane, command-state, and accessibility routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry mixed raster/vector/annotation/shooting-frame/vanishing-point spans plus bounded viewport-clipped radial guides, snapshot-owned pools, view-local diagnostics, and previews. The Windows renderer validates and draws exact-color radial overlays and handles and rebuilds them after device loss. Core-owned normal raster, thumbnail, and instruction export exclude vanishing-point overlays. Raster revision-max validation remains scalar-only and the audited source-call-graph lock plus payload counters remain green. |
| Product surface | New Cut, structural sequence editing and endpoint Stop/Wrap selection, drawing/fill/vector/effects, Text/Instruction annotations, angled shooting-frame properties/handles/export, multiple vanishing-point properties/handles/radial snap, selection, layer/plane, transform, Light Table, clipboard, common-raster import/export, Batch, history visualization, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. Static production commands remain menu-accessible with configurable shortcuts; dynamic history-document entries are ephemeral session targets. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `WIN-001` | Native Windows shell, offline Help/About/Acknowledgements with locked BLAKE3/PNG dependency attribution, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with resource-titled singleton inspector headers, visible system-color 4-DIP splitters, an accessible keyboard-resizable Layer/Plane split, and an explicit Layer/Plane action target; default lower-inspector grouping, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

M17／`PM-GAP-001` and M18／`PM-GAP-002` completed their x64 user-visible confirmations. The selected
topology keeps one Cut descriptor and every member Cell as separate same-directory
`.inkpod` files. Sequence order and display number are mutable attributes, while
`(CellId, document UUID)` remains identity. One ordered request commits all
membership changes or none, and remove leaves the source file intact. M19 has
completed its automatic gates and x64 user-visible confirmation. The user selected
the recommended independent-overlay and exact-transform-rejection contract for M20.
Its vertical slice has passed the required automatic gates and x64 user-visible
confirmation. M21 and M22 have also completed their required automatic gates and
x64 user-visible confirmations. The 22-item PaintMan gap-closure program is complete;
its compact historical record is retained in [`legacy.md`](legacy.md).

## Known differences

- Native `.inkpod` is current-only v26; non-v26 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V26 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest automatic verification is for HIST-002 on 2026-08-13. The complete
retained procedure graph now flows through one read-only Core replay/format/thumbnail
query, an immutable ABI-v12 snapshot, and the dynamic Windows Tools submenu plus
modeless owner-data list. Commit ordering including inactive branches, typed string
formatting, post-commit thumbnails, empty/cancel/no-mutation behavior, bounded ABI
copies, issue-time session targeting, dialog reuse/close, and the production route
are covered. Native v26/runtime epoch 23 is unchanged because no serialized or
replay semantics changed.

| Boundary | Result |
| --- | --- |
| Rust workspace | 458 unit/integration tests including one doctest, zero ignored. HIST-002 coverage includes retained-branch event order, primitive names and bounded typed arguments, deterministic thumbnails, empty/cancel/no-mutation behavior, ABI ownership, size queries and negative cases. Public route inventory covers 294 Rust routes, 301 C ABI exports, and 381 static Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V26/runtime replay epoch 23, ABI v12, Cell document/archive metadata schema 5, document digest schema 9/domain 8, snapshot-composite schema 3, Cut descriptor schema 2, and `.inkbatch` v2 are current. Exact top-level v25, noncurrent archive/Cut versions, malformed vanishing-point/shooting-frame records, checksum failures, and corrupt corpus are rejected; Cell/Cut save/reopen is green |
| Windows x64 | 2026-08-13 Debug configure/build passed with warnings denied and static CRT; all 33 CTests passed in 237.56 s. Native smoke covers the dynamic open-`.inkpod` submenu, fixed session/generation targeting, one modeless dialog per document, its sole three-column owner-data list, lazy row text, thumbnail path, reuse and close behavior. ABI v12, renderer/device-loss, portable ZIP, and unsigned MSIX payload tests also passed. The latest Release run remains the 2026-08-13 M22 run with all 33 CTests passed in 75.57 s |
| Windows ARM64 | Not run for M22 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | The HIST-002 quick profile preserved every checksum/revision/history/reuse/rebuild/output/failure gate. `canonical_replay` remains `264b98028ac92ac6` at revision 6/history 5; `checkpoint_open` remains `07da1b4e6bc5d289` with 175256 input, 256 output, and one reused item; output-color-guard remains `cfb6b288963c78ba` with 1048576 input, 524288 output, and 65536 reused items. The M22 supplemental full results and approved envelopes remain current; workload, harness, payload-access route, and revision-max expression are unchanged |
| Fuzzing | `native_v26`, `native_core_v26`, and `cut_v26` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

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
