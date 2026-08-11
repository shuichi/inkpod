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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. The separate `CutCore` owns stable Cut identity, metadata, Cell-creation defaults, ordered `(CellId, document UUID, relative filename)` membership, Cut-only history/savepoint/recovery, and staged member validation without entering any Cell history. Geometry pointer input uses the existing read-only view-targeted resolver, and output-color guard, Color chart, Light Table bulk registration, sequence autosave, brush, scoped replacement, explicit vector topology/diagnostics, Batch, grouped edit targets, typed clipboard, Genesis, and asset contracts remain Core-owned. |
| Persistence | Native `.inkpod` is exact-current v22/runtime replay epoch 19 and `.inkbatch` is exact-current v2. Cell and Cut `.inkpod` files use distinct magic values; a Cut descriptor stores bounded same-directory individual Cell references and rejects noncurrent versions, traversal, escape, missing files, duplicate pair/path identities, and identity mismatch before publication. `META/GENS/ASST/PROC/EDIT` remain authoritative for Cell documents; optional `CKPT` only accelerates open. `CommitFloating/canonical-v3`, `SelectOutputColorGuard/canonical-v1`, `DocumentStateDigest` schema 6, and EDIT schema 5 remain current. Non-v22 input, including v21, is rejected. Cell and Cut saves each stream to an adjacent temporary file and advance only their own savepoint after replacement. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. One `CutSession` per workspace owns the Rust Cut handle and cache while each individual Cell remains an independent `DocumentSession`. File > New Cut creates and saves separate Cell files before the descriptor; Cut Properties changes metadata/defaults without silently mutating existing Cells; Sequence opens referenced Cells; Cut Save/Undo/Redo and descriptor open use the production Core-owner-thread route. Geometry, Locator, Color chart, Light Table, sequence autosave, filter preview, multi-window/session/view, workspace, diagnostics, brush, selection, scoped replacement, Batch, and DockHost routes remain connected without a process-global active-document pointer. |
| Rendering and performance | Immutable snapshots carry the mixed raster/vector render plan, view-local vector diagnostics, disconnected endpoints, and geometry preview. The Windows renderer distinguishes round and square vector cross-sections while preserving fills, centerline modes, AA state, and device-loss reconstruction. Raster-only changed-tile upload and canonical `revision-max` cache validation retain their scalar-only cache-hit path and unchanged source-call-graph lock. Fixed quick/full workloads, semantic counters, checksums, approved ARM64 revision-max envelopes, and the approved x64 output-color-guard envelope protect those boundaries. See [`architecture.md`](architecture.md#canonical-revision-max-render-cache-identity) and [`core-benchmark-baseline.md`](core-benchmark-baseline.md). |
| Product surface | New Cut, Cut Properties/Save/Undo/Redo, individual member Cell opening, drawing, fill, independent Color chart generation/comparison, output-color guard selection, scoped color replacement, selection, layer/plane, transform, Light Table, sequence, vector, filter/effect, adjustment, clipboard, PNG/TIFF/TGA/BMP import/export, Batch, history, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. All production commands remain menu-accessible with configurable shortcuts. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `CUT-001` | Same-directory individual-Cell Cut descriptor; stable Cut/member identity; independent metadata/defaults/history/savepoint/recovery; Windows New Cut, Properties, Save, Undo/Redo, Sequence and descriptor-open production routes; automatic success/no-op/invalid/Cancel/stale/overflow/failure/save/reopen/ownership coverage | x64 Release user-visible creation, existing-Cell independence, Undo/Redo, save/reopen confirmation |
| `SEQ-001` | Natural-order sequence discovery, thumbnails, direct selection, first/previous/next/last/goto; Prompt/Autosave-before-switch setting; durable exact-native autosave and staged restore; dirty/history/editor preservation; async progress and failure-atomic production route; M11 ARM64 user-visible confirmation | Endpoint-loop preference control |
| `WIN-001` | Native Windows shell, offline Help/About, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the current display resources are Japanese-only |
| `WORKSPACE-001` | All eight modeless surfaces use DockHost; applicable panes support dock/tab/float/hide/AutoHide, one-direction split stacks with resource-titled singleton inspector headers, visible system-color 4-DIP splitters, an accessible keyboard-resizable Layer/Plane split, and an explicit Layer/Plane action target; default lower-inspector grouping, five presets, V7 persisted bounded layout with targeted V4/V5/V6 migration, monitor/DPI recovery, AutoHide pointer access, and accessible names. The transient Job Progress pane is intentionally excluded from persistence | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |

M17／`PM-GAP-001` is automatically complete and awaiting x64 user-visible
confirmation. The selected individual-reference topology keeps one Cut descriptor
and every member Cell as separate same-directory `.inkpod` files. Cut metadata,
defaults, revision, history, dirty state and savepoint are independent from every
Cell; defaults are copied only when a Cell is explicitly created. Persistent
membership uses the `(CellId, document UUID)` pair because the numeric Cell ID is
owned by each Cell-document namespace. M18 and later milestones have not started.

## Known differences

- Native `.inkpod` is current-only v22; non-v22 files are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch output currently writes native `.inkpod` only.
- V22 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

The latest complete automatic verification is for M17 on 2026-08-12 and awaits
x64 user-visible confirmation. New Cut creates independently saved Cell files and
one descriptor, Cut Properties keeps existing Cells unchanged, Sequence opens the
referenced files, and Cut Save/Undo/Redo/reopen use the Core-owner-thread production
route. Failure cleanup preserves the originating FFI diagnostic before releasing
the Rust-owned Cut handle. Exact pair identity permits the same numeric Cell ID in
different Cell-document namespaces while duplicate pairs and duplicate paths remain
invalid. M18 and later milestones remain untouched.

| Boundary | Result |
| --- | --- |
| Rust workspace | 431 unit/integration tests plus one doctest, zero ignored. Cut coverage includes metadata/default independence, same numeric Cell ID in distinct document namespaces, success/no-op/invalid/Cancel/stale/overflow/failure, Undo/Redo, save/reopen/recovery, noncurrent/corrupt/path/identity rejection, and FFI ownership/negative cases. Public route inventory covers 268 Rust routes, 268 C ABI exports, and 359 Windows commands. `fmt`, all-target/all-feature Clippy with warnings denied, and strict rustdoc passed |
| Native format | V22/runtime replay epoch 19, ABI v11, Cut descriptor schema 1/epoch 19, `CommitFloating/canonical-v3`, EDIT schema 5, and `.inkbatch` v2 are current. Exact top-level v21 rejection, Cut noncurrent rejection, Cell/Cut save/reopen, checksum and corrupt-corpus coverage are green |
| Windows x64 | 2026-08-12 Release configure/build passed with warnings denied and static CRT; all 31 CTests passed in 70.68 s. Native smoke covers New Cut with two individually saved Cells, Properties/default independence, Sequence member opening, Cut Undo/Redo/save/reopen, candidate replacement failure, renderer/device reset, ABI, portable ZIP, and unsigned MSIX payload. User-visible M17 confirmation is pending |
| Windows ARM64 | Not run for M17 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | The required M17 quick profile passed all ten fixed checksum/revision/history/reuse/rebuild/output/failure gates. `canonical_replay` produced `f521d658a47051e9` at revision 6/history 5 with five successes and zero failures; cache-hit pan/zoom reused all eight items and dirty-tile rebuild reused 224 of 256 inputs. The supplemental full profile was not rerun for M17 because no workload, harness, envelope, render-cache formula, payload-access path or approved wall-clock scenario changed; its prior approved baseline remains unchanged |
| Fuzzing | `native_v22`, `native_core_v22`, and `cut_v22` target declarations are current. Fuzz binary build and coverage-guided execution were not run because the optional `cargo fuzz` subcommand is outside the required gate |

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
