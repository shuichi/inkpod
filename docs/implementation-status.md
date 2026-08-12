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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. Sequence previous/next resolves an immutable identity- and revision-bound step plan with explicit empty/single/stopped/advanced/wrapped results; commit re-resolves that exact plan, making endpoint stop a dirty-safe no-op and rejecting stale switches atomically. The optional stable-ID angled shooting frame is independent of axis-aligned paper-fit metadata; `EditShootingFrame/canonical-v2`, one transient preview state machine, fixed-point geometry/hit testing, document-transform hooks, and exact nonuniform-transform rejection share one owner. Text/Annotation and the separate Cut, geometry, output-color guard, Color chart, Light Table, brush, replacement, vector, Batch, clipboard, Genesis, and asset contracts retain their existing owners. |
| Persistence | Native `.inkpod` is exact-current v25/runtime replay epoch 22 and `.inkbatch` is exact-current v2. Cell and Cut `.inkpod` files use distinct magic values; Cut payload schema 2 remains current. Cell `DocumentArchive`/document metadata schema 4 persists the optional shooting frame and bounded annotation records; `DocumentStateDigest` schema 8/domain 7 includes its semantic fields, while canonical snapshot-composite schema 3 is unchanged. `META/GENS/ASST/PROC/EDIT` remain authoritative and optional `CKPT` only accelerates open. Non-v25 input, including exact v24, is rejected without migration. Atomic Cell/Cut savepoint rules are unchanged. The application-wide sequence endpoint preference is a separate bounded HKCU record at version 1; missing or malformed data selects Stop and does not affect document dirty/history/savepoints or any native format version. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. Production previous/next sequence commands resolve the Core step plan before dirty prompting, apply the app-wide Stop/Wrap preference, and revalidate the same plan after autosave; the checked menu command persists the preference and participates in the shared shortcut/state/accessibility catalog. Cell > Shooting Frame exposes typed properties, handle-edit mode, and delete; Canvas center/corner/rotation drags use begin/update/apply/Cancel preview calls. File > Export Instruction Image uses the explicit instruction-output route. Existing annotation, Cut, geometry, pane, command-state, and accessibility routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry mixed raster/vector/annotation/shooting-frame render spans, snapshot-owned UTF-8/point pools, view-local diagnostics, and previews. The Windows renderer validates and draws the fixed red angled frame plus center/corner/rotation handles and reconstructs it after device loss. Core owns deterministic normal/thumbnail/instruction raster policy: only the explicit instruction export includes an enabled shooting frame. Raster revision-max validation remains scalar-only and the audited source-call-graph lock plus payload counters remain green. |
| Product surface | New Cut, structural sequence editing and endpoint Stop/Wrap selection, drawing/fill/vector/effects, Text/Instruction annotations, angled shooting-frame properties/handles/export, selection, layer/plane, transform, Light Table, clipboard, common-raster import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `SEQ-ENDPOINT-001` | App-wide persisted Stop/Wrap preference, Core-owned revision/identity-bound step plan and exact commit, explicit empty/single/stopped/advanced/wrapped results, dirty-safe endpoint no-op, stale rejection, additive ABI v12 route, checked/configurable Windows command, and production smoke | x64 Release user-visible verification |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with resource-titled singleton inspector headers, visible system-color 4-DIP splitters, an accessible keyboard-resizable Layer/Plane split, and an explicit Layer/Plane action target; default lower-inspector grouping, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

M17／`PM-GAP-001` and M18／`PM-GAP-002` completed their x64 user-visible confirmations. The selected
topology keeps one Cut descriptor and every member Cell as separate same-directory
`.inkpod` files. Sequence order and display number are mutable attributes, while
`(CellId, document UUID)` remains identity. One ordered request commits all
membership changes or none, and remove leaves the source file intact. M19 has
completed its automatic gates and x64 user-visible confirmation. The user selected
the recommended independent-overlay and exact-transform-rejection contract for M20.
Its vertical slice has passed the required automatic gates and x64 user-visible
confirmation. M21 has passed its automatic gates and now awaits the x64
user-visible confirmation; M22 remains untouched.

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

The latest complete automatic verification is for M21 on 2026-08-12. Sequence
previous/next now resolves a Core-owned immutable step plan using the app-wide
Stop/Wrap preference and commits only the same revision/identity-bound result.
Empty, single-cell, stopped, wrapped, stale, dirty prompt/autosave, settings
round-trip, ABI negative, command-state, and production-route behavior are covered.
M20 has completed its x64 user-visible confirmation; M21 now awaits only that
confirmation, and M22 remains untouched.

| Boundary | Result |
| --- | --- |
| Rust workspace | 448 unit/integration tests plus one doctest, zero ignored. Sequence endpoint coverage includes empty, single, natural-order gaps, Stop/Wrap in both directions, dirty-safe no-op, stale and unsaved atomicity, settings round-trip, additive ABI ownership/structure-size/enum/flag negative cases, command checked state, and production prompt/autosave routes. Public route inventory covers 285 Rust routes, 289 C ABI exports, and 377 Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V25/runtime replay epoch 22, ABI v12, Cell document/archive metadata schema 4, document digest schema 8/domain 7, snapshot-composite schema 3, Cut descriptor schema 2, and `.inkbatch` v2 remain current and unchanged for M21. The separate sequence endpoint application-setting record is v1 and rejects malformed/version-mismatched values by retaining the Stop default. Exact top-level v24, noncurrent archive/Cut versions, malformed shooting-frame records, checksum failures, and corrupt corpus remain rejected; Cell/Cut save/reopen is green |
| Windows x64 | 2026-08-12 Release configure/build passed with warnings denied and static CRT; all 31 CTests passed in 81.02 s. Native smoke covers endpoint Stop without dirty prompting or document/history/checksum changes, previous/next Wrap, checked command state, settings persistence, dirty Prompt and Autosave revalidation, ABI v12, portable ZIP, and unsigned MSIX payload. The user-visible M21 Stop/Wrap, persistence, and dirty-cell confirmation remains pending |
| Windows ARM64 | Not run for M21 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | The required M21 quick profile passed all ten checksum/revision/history/reuse/rebuild/output/failure gates. `canonical_replay` remains `264b98028ac92ac6` at revision 6/history 5, `checkpoint_open` remains `c66817dca5345832` with 175256 input items, 256 output items, and one reused item, and output-color-guard remains `650300bdff9044cb` with 1048576 input, 524288 output, and 65536 reused items. The latest supplemental full profile remains the passing M20 run. M21 does not change a performance route; workload, harness, approved envelope, payload-access route, and revision-max expression are unchanged |
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
