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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. The optional stable-ID angled shooting frame is independent of axis-aligned paper-fit metadata; `EditShootingFrame/canonical-v2`, one transient preview state machine, fixed-point geometry/hit testing, document-transform hooks, and exact nonuniform-transform rejection share one owner. Text/Annotation and the separate Cut, geometry, output-color guard, Color chart, Light Table, sequence, brush, replacement, vector, Batch, clipboard, Genesis, and asset contracts retain their existing owners. |
| Persistence | Native `.inkpod` is exact-current v25/runtime replay epoch 22 and `.inkbatch` is exact-current v2. Cell and Cut `.inkpod` files use distinct magic values; Cut payload schema 2 remains current. Cell `DocumentArchive`/document metadata schema 4 persists the optional shooting frame and bounded annotation records; `DocumentStateDigest` schema 8/domain 7 includes its semantic fields, while canonical snapshot-composite schema 3 is unchanged. `META/GENS/ASST/PROC/EDIT` remain authoritative and optional `CKPT` only accelerates open. Non-v25 input, including exact v24, is rejected without migration. Atomic Cell/Cut savepoint rules are unchanged. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. Cell > Shooting Frame exposes typed properties, handle-edit mode, and delete; Canvas center/corner/rotation drags use begin/update/apply/Cancel preview calls. File > Export Instruction Image uses the explicit instruction-output route. Existing annotation, Cut, sequence, geometry, pane, command-state, and accessibility routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry mixed raster/vector/annotation/shooting-frame render spans, snapshot-owned UTF-8/point pools, view-local diagnostics, and previews. The Windows renderer validates and draws the fixed red angled frame plus center/corner/rotation handles and reconstructs it after device loss. Core owns deterministic normal/thumbnail/instruction raster policy: only the explicit instruction export includes an enabled shooting frame. Raster revision-max validation remains scalar-only and the audited source-call-graph lock plus payload counters remain green. |
| Product surface | New Cut, structural sequence editing, drawing/fill/vector/effects, Text/Instruction annotations, angled shooting-frame properties/handles/export, selection, layer/plane, transform, Light Table, clipboard, common-raster import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto; Prompt/Autosave-before-switch setting; durable exact-native autosave and staged restore; dirty/history/editor preservation; async progress and failure-atomic production route; M11 ARM64 user-visible confirmation | Endpoint-loop preference control |
| `SHOOTING-FRAME-001` | Independent stable-ID angled object, canonical edit/preview, fixed-point geometry, document transforms, normal/instruction output split, current native persistence, ABI v12, Windows properties/handles/renderer/export route and production smoke | x64 Release user-visible verification |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with resource-titled singleton inspector headers, visible system-color 4-DIP splitters, an accessible keyboard-resizable Layer/Plane split, and an explicit Layer/Plane action target; default lower-inspector grouping, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

M17／`PM-GAP-001` and M18／`PM-GAP-002` completed their x64 user-visible confirmations. The selected
topology keeps one Cut descriptor and every member Cell as separate same-directory
`.inkpod` files. Sequence order and display number are mutable attributes, while
`(CellId, document UUID)` remains identity. One ordered request commits all
membership changes or none, and remove leaves the source file intact. M19 has
completed its automatic gates and x64 user-visible confirmation. The user selected
the recommended independent-overlay and exact-transform-rejection contract for M20.
Its vertical slice has passed the required automatic gates and now awaits the
x64 user-visible confirmation; later milestones remain untouched.

## Known differences

- Native `.inkpod` is current-only v25; non-v25 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V25 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete automatic verification is for M20 on 2026-08-12. Its optional
stable-ID angled frame remains independent of axis-aligned paper-fit metadata and
uses one canonical edit/preview owner. Normal and thumbnail output exclude the
object; only the explicit instruction export includes it. Save/reopen, Undo/Redo,
stale and overflow atomicity, snapshot ownership, device loss, and production
properties/handle/export routing are covered. M20 now awaits only its x64
user-visible confirmation; later milestones remain untouched.

| Boundary | Result |
| --- | --- |
| Rust workspace | 446 unit/integration tests plus one doctest, zero ignored. Shooting-frame coverage includes create/update/delete, preview/Cancel, no-op, invalid, stale, stable-ID and revision overflow, nonuniform-resample failure atomicity, five-anchor transforms, one-step Undo/Redo, replay, save/reopen, normal/thumbnail/instruction output policy, and ABI ownership/negative cases. Public route inventory covers 283 Rust routes, 287 C ABI exports, and 376 Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V25/runtime replay epoch 22, ABI v12, Cell document/archive metadata schema 4, document digest schema 8/domain 7, snapshot-composite schema 3, Cut descriptor schema 2, and `.inkbatch` v2 are current. Exact top-level v24, noncurrent archive/Cut versions, malformed shooting-frame records, checksum failures, and corrupt corpus are rejected; Cell/Cut save/reopen is green |
| Windows x64 | 2026-08-12 Release configure/build passed with warnings denied and static CRT; all 31 CTests passed in 77.70 s. Native smoke covers shooting-frame property creation, explicit instruction-image export, center-handle drag, Undo/Redo, capture Cancel, fixed-red rendering/device reset, ABI v12, portable ZIP, and unsigned MSIX payload. The user-visible M20 properties/handles, Cancel, save/reopen, and output split confirmation remains pending |
| Windows ARM64 | Not run for M20 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | Required quick and supplemental full profiles passed all ten checksum/revision/history/reuse/rebuild/output/failure gates. `canonical_replay` remains `264b98028ac92ac6` at revision 6/history 5 and `checkpoint_open` is `c66817dca5345832` in both profiles. Output-color-guard is `650300bdff9044cb` quick and `290f6150f7718c2d` full with unchanged exact counters. The checksum changes are the expected current schema/digest result; workload, harness logic, envelope, payload-access route, and revision-max expression are unchanged |
| Fuzzing | `native_v25`, `native_core_v25`, and `cut_v25` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

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
