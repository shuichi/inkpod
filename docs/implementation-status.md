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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. Text/Annotation layers now own bounded stable-ID Text/Stroke/Leader/Value objects; `EditAnnotations/canonical-v2` covers atomic multi-object create/update/move/delete, while begin/append/end commits one instruction stroke and Cancel publishes nothing. The separate `CutCore`, geometry, output-color guard, Color chart, Light Table, sequence, brush, replacement, vector, Batch, clipboard, Genesis, and asset contracts retain their existing owners. |
| Persistence | Native `.inkpod` is exact-current v24/runtime replay epoch 21 and `.inkbatch` is exact-current v2. Cell and Cut `.inkpod` files use distinct magic values; Cut payload schema 2 remains current. Cell `DocumentArchive`/document metadata schema 3 persists bounded annotation records; `DocumentStateDigest` schema 7/domain 6 and canonical snapshot-composite schema 3 include their semantic fields. `META/GENS/ASST/PROC/EDIT` remain authoritative and optional `CKPT` only accelerates open. Non-v24 input, including exact v23, is rejected without migration. Atomic Cell/Cut savepoint rules are unchanged. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. Layer > Text/Instructions exposes Japanese text add/re-edit, instruction draw mode, previous/next selection, one-pixel keyboard move, and delete; selected objects have Canvas bounds/handles. Canvas input is converted to milli-pixel document points and queued as begin/append/end/cancel without UI waits. Generic layer properties, command-state/accessibility names, and all existing Cut/sequence/geometry/pane routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry mixed raster/vector/annotation render spans, snapshot-owned UTF-8/point pools, view-local diagnostics, and previews. The Windows renderer owns DirectWrite font resolution and a bounded text-format cache, falls back to Segoe UI with a visible warning, draws selection handles, and reconstructs the retained annotation snapshot after device loss. Core owns deterministic portable thumbnail/flat annotation rasterization; Normal objects participate in ordinary flat export while Instruction objects do not. Raster revision-max validation remains scalar-only and the audited source-call-graph lock plus payload counters remain green. |
| Product surface | New Cut, structural sequence editing, drawing/fill/vector/effects, Text/Instruction annotations, selection, layer/plane, transform, Light Table, clipboard, common-raster import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `ANNOTATION-001` | Stable-ID Text/Stroke/Leader/Value annotation content; canonical atomic multi-edit/stroke; ABI v12; DirectWrite Canvas/fallback; selection handles; keyboard route; save/reopen and output-policy tests | x64 Release Japanese text/instruction edit, save/reopen, and PNG exclusion confirmation |
| `IO-001` | Exact-current v24/epoch-21 annotation schema, replay, malformed rejection, staged save/reopen, and exact-v23 rejection | Same x64 user-visible annotation save/reopen confirmation |
| `RENDER-001` | Snapshot annotation spans; DirectWrite Canvas; Core deterministic thumbnail/flat output; Instruction filtering; device-loss smoke | Same x64 Canvas and PNG exclusion confirmation |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto; Prompt/Autosave-before-switch setting; durable exact-native autosave and staged restore; dirty/history/editor preservation; async progress and failure-atomic production route; M11 ARM64 user-visible confirmation | Endpoint-loop preference control |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with resource-titled singleton inspector headers, visible system-color 4-DIP splitters, an accessible keyboard-resizable Layer/Plane split, and an explicit Layer/Plane action target; default lower-inspector grouping, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

M17／`PM-GAP-001` and M18／`PM-GAP-002` completed their x64 user-visible confirmations. The selected
topology keeps one Cut descriptor and every member Cell as separate same-directory
`.inkpod` files. Sequence order and display number are mutable attributes, while
`(CellId, document UUID)` remains identity. One ordered request commits all
membership changes or none, and remove leaves the source file intact. M19 has
completed its automatic gates and awaits x64 user-visible confirmation; M20 and later remain untouched.

## Known differences

- Native `.inkpod` is current-only v24; non-v24 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V24 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete automatic verification is for M19 on 2026-08-12 and awaits
x64 user-visible confirmation. Bounded stable-ID Text/Stroke/Leader/Value objects
share one annotation-layer owner. Atomic multi-edit and instruction-stroke routes
use the canonical executor; Normal content participates in flat output and
Instruction content remains Canvas/thumbnail-only. Save/reopen, Undo/Redo, snapshot
ownership, DirectWrite fallback, device loss, and product command routing are covered.
M20 and later remain untouched.

| Boundary | Result |
| --- | --- |
| Rust workspace | 441 unit/integration tests plus one doctest, zero ignored. Annotation coverage includes success, no-op, invalid UTF-8/geometry, Cancel, stale, stable-ID and revision overflow, batch failure atomicity, one-step Undo/Redo, replay, save/reopen, output filtering, and ABI ownership/negative cases. Public route inventory covers 276 Rust routes, 279 C ABI exports, and 372 Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V24/runtime replay epoch 21, ABI v12, Cell document/archive metadata schema 3, document digest schema 7/domain 6, snapshot-composite schema 3, Cut descriptor schema 2, and `.inkbatch` v2 are current. Exact top-level v23, noncurrent archive/Cut versions, malformed annotation records, checksum failures, and corrupt corpus are rejected; Cell/Cut save/reopen is green |
| Windows x64 | 2026-08-12 Release configure/build passed with warnings denied and static CRT; all 31 CTests passed in 75.29 s. Native smoke covers Japanese text add/edit/move/delete, instruction begin/append/end/Cancel, selected handles, missing-font fallback warning, flat-output exclusion, renderer/device reset, ABI v12, portable ZIP, and unsigned MSIX payload. User-visible M19 confirmation is pending |
| Windows ARM64 | Not run for M19 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | Required quick and supplemental full profiles passed all ten checksum/revision/history/reuse/rebuild/output/failure gates. `canonical_replay` is `264b98028ac92ac6` at revision 6/history 5 and `checkpoint_open` is `b63e39424fbad396` in both profiles. Output-color-guard is `8b2bd6bfbf8eada8` quick and `53bbe70c027a2864` full with unchanged exact counters; independent five-run medians were 76.570 ms quick and 341.959 ms full, inside the approved 55–92 ms and 255–425 ms ranges. Workload, harness logic, envelope, payload-access route, and revision-max expression are unchanged |
| Fuzzing | `native_v24`, `native_core_v24`, and `cut_v24` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

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
