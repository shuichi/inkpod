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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Batch v3 lowers its enabled ordered operations to one private `ApplyBatchOperations` procedure: exact native-depth Color Replace, atomic Move to Color Plane, sparse fill-protection Masking, and Erase. The mask is part of document state and acts as a hard boundary in every fill route; only wall-bearing tiles allocate. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. Existing filter, continuous-fill, visibility and resize primitives remain in Core but are absent from the public Batch catalog. |
| Persistence | Native Cell `.inkpod` is exact-current v28/runtime replay epoch 25 and `.inkbatch` is exact-current v3. Cell and Cut `.inkpod` files retain distinct magic values; Cut descriptor schema 2/replay epoch 24 remains current. `DocumentArchive` remains schema 6, document metadata is schema 7, `DocumentStateDigest` is schema 11/domain 8, EditorState is schema 7/domain 2, and canonical snapshot-composite is schema 4. `META/GENS/ASST/PROC/EDIT` remain authoritative and optional `CKPT` only accelerates open. Non-v28 Cell input, including exact v27, and `.inkbatch` v2 are rejected without migration. Atomic Cell/Cut savepoint rules are unchanged. |
| InkScript | Exact-current registry schema/language/file v2, catalog/owner manifest v4, replay epoch 25, native v28 and ABI v19 are aligned. The public catalog remains exactly 75 commands; the private Batch v3 `ApplyBatchOperations` procedure is intentionally excluded. The existing private Core-engine route still reuses the sole parser, canonical executor and native writer. No `.inkscript` file filter, clipboard payload, or Batch authoring UI was added; `.inkbatch` v3 is the independent product Batch contract. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. The Batch pane owns an editable draft view model with fixed Input and Output rows, a headerless stage list whose operation rows alone expose enabled checkboxes, exactly four popup-addable localized operations, compact font-matched inline parameter pages, full-width nonoverlapping alpha-aware color cells, bounded file/folder pickers, loaded-set restoration, preview validation, and no exposed follow/pin row. It always occupies a newly created singleton right-side tab; tab add/move/load paths reject mixing it with another pane. Its editable set selector enumerates `.inkbatch` files from `%APPDATA%\inkpod\batch-sets`; issue-time command context still fixes the target. Preview/run/save builds one validated immutable graph. Active-document application and pathless dirty new tabs consume generation-bound Rust-owned staged results on the owner thread; bare Rust pointers are not posted through window messages. Japanese/English generated presentation, dynamic right-side tabs, Workspace V9, and all existing document/renderer routes remain. |
| Subpalette | Each workspace owns a standalone read-only ABI v19 object and auxiliary renderer route rather than a document-follow target. Multiple selected files or one nonrecursive folder provide PNG/TIFF/TGA/BMP sources; Core orders terminal cell numbers before naturally ordered unnumbered names. File bytes are read asynchronously under a bound and completion is generation-checked. Decode/navigation failure preserves the last good private image/view. The pane has file/folder actions, icon previous/next/fit/1:1 actions, keyboard navigation, an always-eyedropper Canvas and cursor, exact sampled-color publication to the drawing color/Color pane, and a sample-gated palette-registration action. |
| Tool options presentation | The left Tool rows retain the compact split layout and bounded owned popup with accessible pin/close actions, natural-height measurement, work-area clamping and overflow scrolling. Fill, selection, raster geometry, eyedropper, gradient, alpha gradient, airbrush, blur, stamp, dust and boundary-airbrush settings are embedded pages; only boundary airbrush exposes an explicit destructive Apply. Pin state is session-only. The retired TopContext ToolOptions pane is neither created nor persisted; Workspace V9 migrates V2–V8 records. |
| Color picker presentation | Ring, HSV-triangle, and alpha-track pointer drags update a pane-local preview and synchronously paint the current picker frame for every coalesced mouse sample. Drawing-color and main-line publication occurs once on button release, avoiding Core/editor-state round trips and full Color/Palette/Chart refreshes inside `WM_MOUSEMOVE`; capture cancellation restores the drag-origin color and hue. Keyboard and numeric-field changes retain their immediate commit route. |
| Windows icon presentation | The 14 permanent Tool commands map typed Windows-only `ToolIconId` values to a fixed Fluent UI System Icons subset. Raster Geometry remains a menu-owned six-command group. Layer/Plane visibility and editability cells plus Color, Locator, Sequence, and Light Table pin/follow buttons use typed pane icon IDs; Subpalette uses previous/next/fit/1:1 icons and a Fluent-derived eyedropper cursor, while Batch exposes no pin button. A checked-in 48-pixel A8 atlas is recolored from system text/highlight/disabled colors and DPI-scaled at draw time; theme, system-color, enable-state, and parent-DPI changes rebuild native button images. Full localized window text remains the Tooltip/MSAA/UIA name, and atlas/GDI failure returns to text presentation. Platform icon names do not cross Rust Core, C ABI, history, document, or workspace persistence boundaries. |
| Document-tab presentation | Initial and split editor groups apply a DPI-scaled 9-point Segoe UI ClearType font to each document-tab control. Each tab `HWND` recreates its owned font after a parent DPI transition and releases it during destruction. |
| Rendering and performance | Immutable snapshots carry raster/adjustment spans, shooting-frame and vanishing-point overlays, bounded viewport-clipped radial guides, snapshot-owned pools, view-local diagnostics and previews. The Windows renderer validates and draws radial overlays and handles and rebuilds them after device loss. Canonical composite schema 4 contains only raster passes and adjustment LUTs. Raster revision-max validation remains scalar-only and the audited source-call-graph lock plus payload counters remain green. |
| Product surface | Cut, structural sequence editing and endpoint Stop/Wrap selection, raster drawing/fill/effects, angled shooting-frame properties/handles/export, vanishing-point properties/handles/radial snap, selection, layer/plane, transform, Light Table, raster clipboard, common-raster import/export, Batch, history visualization, recovery, and compaction-copy commands remain connected from the Windows UI to their owners. Retired drawing-model commands and presentation are absent. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. The selected Fluent SVG subset, per-file hashes, upstream commit/release, MIT license, generated atlas hash, resource embedding, notices, and absence of generator/network use in normal builds are checked by `inkpod_windows_fluent_icons`; both packages carry the atlas inside the executable and the notice in `ThirdPartyNotices.txt`. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `WIN-001` | Native Windows shell with explicit Common Controls registration and a system-dark title-bar opt-in, Japanese/English UI with persisted System/Japanese/English selection and non-Japanese English fallback, offline Help/About/Acknowledgements with locked BLAKE3/PNG/Fluent icon dependency attribution, typed Fluent icons for all 14 Tool commands and the applicable Layer/Plane/pin states with localized text fallback, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | Dark presentation is limited to the system title bar; physical high-contrast/200%-DPI, complete screen-reader, and Japanese IME validation/fixes remain; the embedded offline Help body is Japanese-only |
| `WORKSPACE-001` | Persistent and auxiliary panes retain DockHost and Tool Options remains an owned flyout. The right side uses dynamic stable-ID tabs with nonempty unique membership, deterministic add/remove/move/reorder, accessible descriptions and Workspace V9 persistence/migration from V2–V8. The transient Job Progress pane and transient narrow-width suppression are not persisted. | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |
| `SUBPALETTE-001` | Standalone Core/ABI v19 object, independent workspace UI, bounded asynchronous external-image loading, natural cell ordering, exact sampling/registration, icon/keyboard navigation, and failure-preserving decode are implemented and covered at Core/ABI and pane-surface boundaries. | Complete the physical file/folder-dialog multi-image confirmation and the remaining Windows preset matrix before `Verified` |
| `BATCH-001`–`BATCH-004` | Batch v3 Core/format/ABI/Windows implementation, approved quick/full benchmark replacement and focused contracts are present with exact four-operation popup authoring, clickable/Space operation-row checkboxes, compact bounded input pickers, full-width single-selection alpha-aware color cells, native-range old/new alpha editors, drawing-color destination selection, an exclusive singleton right-side tab, `%APPDATA%` set catalog, sparse mask persistence, three input and three output modes, dry-run validation and editable loaded sets | Physical high-contrast and screen-reader checks remain before `Verified` |
| `SCRIPT-001`, `SCRIPT-002`, `SCRIPT-005` | Exact-current registry schema/language/file v2 and catalog/owner manifest v4 with 75 public commands are aligned to native/replay/ABI v28/25/19; private Batch v3 orchestration is excluded | Product `.inkscript` acceptance and the M36 full gate remain pending |
| `SCRIPT-003` | M12 authority/plan/run/install and the M27A/M27B private CoreHost route remain; its current native output follows v28/epoch 25/ABI 19 | Product `.inkscript` file acceptance remains absent |
| `SCRIPT-004` | M05B provides private typed dependency closure. User-confirmed M24 exports one Commit or a same-branch linear chain with exact parent assertions, strict selectors, typed result links, deduplicated assets and cache-free replay parity. User-confirmed M25 exposes the existing exporter through a bounded event span and immutable fragment summary/text/release ABI; M26 changes no fragment semantics | Destination rebind/paste transaction, Batch/History clipboard and source-preserving structured edit remain pending |

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

- Native Cell `.inkpod` is current-only v28; non-v28 files, including v27, are rejected without a
  migration reader before format freeze.
- Normal user-initiated native save/open waits for the Core-engine work item;
  autosave and image-processing tasks use asynchronous paths.
- Sessions share one single-writer `CoreHost` execution lane. Queue latency is
  observable, and accepted work is retained without partial commit.
- Batch v3 folder output supports `.inkpod`, PNG, TIFF, TGA and BMP. A graph containing Masking
  rejects the four common-raster formats because they cannot retain fill-protection state.
- InkScript remains reachable only by private ABI/application smoke hooks; no product command,
  `.inkscript` file filter, clipboard or pane reaches it. `.inkbatch` v3 is an independent closed
  Batch product contract and does not expose the private Batch procedure through InkScript.
- V28 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs byte/stream I/O and platform
  file-authority adapters; Rust domain crates must remain platform-independent.

## Latest representative verification

On 2026-08-24, the `SUBPALETTE-001` implementation passed Rust formatting,
warnings-denied Clippy across the workspace/all targets/all features, the full
workspace/all-feature test suite, warning-denied `inkpod-core` rustdoc, and the
unchanged approved quick benchmark. ARM64 Debug then rebuilt under `/W4 /WX`
with static CRT, portable ZIP, and unsigned MSIX; its 40 non-product tests
passed in 49.40 s and its English/Japanese product smokes passed in
303.40/272.13 s. Native v28 and replay epoch 25 are unchanged; the standalone
read-only Subpalette contract increments only the C ABI to v19.

On 2026-08-23, this reviewed Batch Pane UI follow-up passed Rust
`cargo fmt --check`, warnings-denied Clippy, the workspace/all-feature test,
warning-denied `inkpod-core` rustdoc, and the unchanged approved quick benchmark;
the follow-up itself changed only Win32 UI/test/document sources. x64
Debug, x64 Release and ARM64 Debug then rebuilt under `/W4 /WX`, including the
static-CRT portable ZIP and unsigned MSIX. The current 42 tests passed in each
configuration. x64 Debug was partitioned into 40 non-product-smoke tests
(124.16 s) plus final English/Japanese product-smoke reruns after the last
layout correction (336.37/337.64 s). x64 Release's initial full run passed
41/42 because the Japanese product smoke returned existing code 367
immediately after the English smoke; its isolated rerun passed, and the final
paired English/Japanese rerun passed in 105.83/100.96 s. ARM64 Debug passed
42/42 in 439.04 s before the layout-only correction; its final paired
English/Japanese rerun passed in 236.86/241.56 s. These smokes cover
standard operation-only checkboxes with click and Space routing, true single-row
color selection, native-range old/new alpha edits, drawing-color slot selection,
localized input count, compact bounded input pickers, full-width color columns,
and the exclusive singleton Batch right tab. The benchmark workload, semantic
counters and envelope were unchanged.

Earlier on 2026-08-23, the reviewed Batch Pane v3 implementation passed `cargo fmt --check`,
warnings-denied Clippy for all workspace targets and features, the complete
workspace/all-feature test suite, warning-denied `inkpod-core` rustdoc, and the
unchanged approved quick benchmark. The three required Windows configurations
built the executable, tests, static-CRT portable ZIP and unsigned MSIX under
`/W4 /WX`: x64 Debug, x64 Release and ARM64 Debug. Each configuration passed all
40 CTests. Totals were 684.53 s, 144.55 s and 459.54 s respectively; ABI smoke
took 46.63 s, 2.49 s and 26.73 s; English product smoke took 303.12 s, 57.79 s
and 203.85 s; Japanese product smoke took 315.78 s, 65.39 s and 212.90 s. These
product smokes cover the exact localized four-item Batch catalog, fixed
Input/Output rows, headerless stages, editable loaded v3 sets, inline parameter
page switching, run-state disable/cancel and owner-thread staged new-tab
adoption. A focused Windows test covers `%APPDATA%` set-name path validation,
extension mapping and enumeration without writing to the real user directory.

The user-approved `batch_preview` replacement uses four quick or sixteen full
native v28 files and exact native-depth Color Replace rows. It preserves the
2-iteration, 4/16-input, 8/32-output, zero-reuse/revision/history, 4/16-success
and one intentional-invalid-probe counters. Quick/full checksums are
`9ae6835726a36053` and `d1be39275687aa9b`; the five primary samples and five
diagnostic samples per profile are recorded in `core-benchmark-baseline.md`.
Every process passed all semantic gates. Measurements ran on an ARM64 Parallels
host, so no x64 wall-clock envelope was changed or inferred.

The 2026-08-18 Color-picker drag-follow repair rebuilt ARM64 Debug `inkpod`,
the portable ZIP, and the unsigned MSIX under MSVC `/W4 /WX` with static CRT.
Ring, HSV-triangle, and alpha-track drags now synchronously paint pane-local
preview state, publish to Core/editor state once on release, and restore the
drag origin on capture cancellation. Per the user's request, no test
executable or smoke mode was run for this refinement.

The 2026-08-18 Layer/Plane status-button refinement rebuilt ARM64 Debug
`inkpod`, the portable ZIP, and the unsigned MSIX under MSVC `/W4 /WX` with
static CRT. Visibility and editability now use centered 32-by-32-DIP buttons,
unchanged 16-DIP icons, a 4-DIP gap, and shared draw/hit-test rectangles. Per
the user's request, no test executable or smoke mode was run for this
refinement.

The 2026-08-18 Tool Options popup refinement rebuilt ARM64 Debug `inkpod`, the
portable ZIP, and the unsigned MSIX under MSVC `/W4 /WX` with static CRT. The
build covers the captionless natural-height popup, deferred outside-activation
dismissal, session-only pin/close header actions, work-area clamping, overflow
scrolling, and the two generated Japanese/English pin labels. Per the user's
request, no test executable or smoke mode was run for this refinement.

The latest Windows product-smoke hardening on 2026-08-18 replaces the
multi-workspace isolated-document stroke's fixed client-pixel coordinates with
document-relative points derived from the post-Fit snapshot transform. ARM64
Debug rebuilt the executable, portable ZIP, and unsigned MSIX under MSVC
`/W4 /WX` with static CRT; the English product smoke passed in 335.32 s and the
Japanese product smoke passed in 343.86 s. Product behavior, Rust Core, C ABI,
native/replay versions, and compatibility status are unchanged.

The latest automatic work is the Windows Tool Options flyout slice on 2026-08-18. x64 Debug rebuilt the executable, four-file portable ZIP,
and unsigned MSIX under MSVC `/W4 /WX` with static CRT. The complete 38-test split passed: the 35 non-product tests other than command-route
inventory passed together in 186.29 s with ABI smoke at 169.71 s, the corrected command-route inventory passed independently, English product
smoke passed in 553.74 s, and Japanese product smoke passed in 575.25 s. The product smokes cover flat split Tool rows, expansion-region
accessibility, owned flyout lifetime and keyboard routing, direct fill/brush/plane edits, embedded selection/geometry/effect pages, explicit
boundary-airbrush Apply, Window-menu checked state, no default TopContext geometry, and the existing document/render/workspace lifecycle.
Workspace V8 round-trip and V2–V7 migration tests verify that legacy docked ToolOptions records become hidden and are no longer persisted.
Rust Core, C ABI, native/replay versions, and document semantics are unchanged.

The latest automatic work is the Windows right-side top-level tool-tab slice on 2026-08-17. ARM64 Debug rebuilt the executable, four-file
portable ZIP, and unsigned MSIX under MSVC `/W4 /WX` with static CRT. The complete 37-test split passed: the 34 tests excluding ABI and the
two product smokes passed in 15.07 s, ABI smoke passed in 117.26 s, English product smoke passed in 365.52 s, and Japanese product smoke
passed in 372.46 s. The model/layout contracts cover initial membership, unique repeated moves, selection fallback, all-hidden, re-show,
empty tabs, extreme-ratio positive pane heights, ordered reordering, and editor expansion; both product smokes cover menu check state,
top-level control creation, real tab switching, drag-reorder round-trip, all-hidden collapse, and restoration. The implementation retains the existing pane HWNDs, individual pane
headers, owned floating windows, DockLayout split weights, and 4-DIP splitter input while adding an ordered runtime `ToolTab` source of truth
and a Common Controls tab projection. Rust Core, C ABI, native/replay versions, document behavior, and the V7 HKCU workspace record are
unchanged; top-level tab state intentionally initializes per workspace in this slice. The tab-drag smoke harness now positions its temporary
document-tab control so both the drag source and the unchanged second-tab insertion target remain visible with DPI-scaled captions.

The previous M27 rebaseline established the user-confirmed M27B Core-engine route. Batch v3 retains that route while advancing the
exact-current tuple to native v28, replay epoch 25, ABI v18 and the 75-command catalog/owner manifest v4. The immutable `CommandContext`, copied authority inputs, pointer-free progress, nonblocking wait,
cancel/close/shutdown ownership and atomic native install contracts remain unchanged. Catalog v2 and the nine retired commands are rejection
fixtures only; the production parser, compiler, binder, canonical executor, exporter and native writer use the exact-current tuple. Raster
Geometry is connected through the public Core and ABI preview/commit route and the Windows command/Canvas path; invalid target, cancellation,
document switch, Undo/Redo, save/reopen and snap behavior remain atomic. Rust, static Windows, and current x64 native boundary results for this
continuation are recorded in the table below. The artifact-synchronized x64 Debug build and all 40 CTests now pass. The full InkScript fixture
remains reserved for M36;
the strengthened WIN-001
Japanese/English localization slice remains unchanged. A versioned HKCU preference records System, Japanese,
or English; System selects Japanese only for a first-preferred `ja` Windows UI language
and otherwise selects English on the next launch. The canonical typed catalog generates
the C++ IDs/table plus same-ID ja-JP/en-US STRINGTABLE, menu,
and dialog resources. Product resource lookup names the selected LANGID explicitly.
Complete format strings are selected before arguments are inserted; no hook, Japanese-key,
partial, hybrid, English-history-key, or direct-language branch remains. History entries
cross ABI v18 as one of five fixed-width semantic kinds and are mapped exhaustively to
catalog IDs only at the Windows presentation boundary. User names and paths cross an explicit opaque-text
boundary and are never treated as translation input. Tool labels share IDs across owner
draw, tooltip and accessibility; both full product smoke modes retrieve each real ToolTip
control value and compare it with the same typed label used by the button, MSAA, and UIA.
Layer/Plane presentation is pre-resolved before drawing, Plane badges use typed compact
two-line labels without ellipsis while the adjacent detail and accessibility text retains
the full kind, and owner-draw visibility/editability cells use centered 32-by-32-DIP
buttons around unchanged 16-DIP icons, a 4-DIP gap, and shared draw/hit-test geometry;
Color tabs are ID-generated. The static gate rejects raw, escaped, or UTF-8-byte-array
Japanese literals in all other Windows product sources, rejects direct wide-string bypasses
in product dialog/effect presentation fields and fallback resource APIs, compares the exact
Japanese/English resource-identifier sets, covers all 346 localized menu command IDs and all
354 state-owned command IDs including the eight pane-local actions, and checks generated
artifact hashes. Dedicated tests exercise both resource languages and embedded-NUL file
filters. Full English and Japanese product smoke runs cover the same UI/Core/renderer,
owner-draw, tooltip, MSAA/UIA, state, device-loss and workspace-lifecycle paths. The native
format and replay epoch are exact-current v28/25; C ABI is exact-current v18.

| Boundary | Result |
| --- | --- |
| InkScript registry | Exact-current registry schema/language/file v2 accepts production catalog/owner manifest v4 and rejects retired resources. `inkscript_registry` covers all 75 public command/owner/runtime-adapter/equivalence identities, required metadata, session-only exclusion, normalized fingerprint, generated-reference drift, duplicate/malformed/overflow rejection, version drift, public ownership, private-model isolation and Windows non-reachability. File v2/catalog v4/epoch 25/native v28/ABI v19 are aligned |
| InkScript M01 | Public `inkpod-format` source/line-map/diagnostic/lexer API passes all 12 contract tests; malformed/truncation property corpus is deterministic; the exact-current fuzz target is `inkscript_lexer_v2`; file v2/catalog v4/epoch 25/native v28/ABI v19 are current |
| InkScript M02 | Public `inkpod-format` lossless CST/bounded parser API passes all 13 contract tests; complete file/fragment grammar, noncurrent rejection, byte-perfect writer, recovery/error nodes, duplicate/missing/group rules and caller-lowered resource stops are covered; the exact-current fuzz target is `inkscript_parser_v2` |
| InkScript M03 | Public `inkpod-format` semantic AST, generated language-schema projection, bounded private `SchemaView`, canonical file/fragment emitter and deterministic name allocator pass all 6 contract tests; invalid CST, unknown command/field/type/order and noncanonical input fail closed under exact-current file v2/catalog v4 |
| InkScript M04 | Public immutable typed orchestration envelope and authority-free path-intent preview pass all 4 contract tests; exact-current file v2/catalog v4 requirements, metadata, bounds, closed output/execution, atomicity and ownership fail closed |
| InkScript M05A | Public immutable declaration and per-run models pass all 4 contract tests; generated type/enum/constructor/record projection, exact Q16 ties-to-even, diagnostics, namespace rules, duplicate/undefined/forward/cycle rejection, no-op, Cancel/invalid atomicity and ownership fail closed under exact-current file v2/catalog v4 |
| InkScript M05B | Public immutable step/group/result/dependency and closed-fragment APIs pass all 5 contract tests; result typing, references, unknown schema/command, resource stops, strict binding, asset dedup, canonical reparse, atomicity and ownership fail closed under exact-current file v2/catalog v4 |
| InkScript M06 | Generated typed selector/assert/ID-namespace metadata, initial-snapshot evaluator and closed catalog interface pass 7 contracts. Initial order, cardinality, missing/ambiguous/owner mismatch, strict preconditions, result dependencies, skip rules, bounds, atomicity and ownership fail closed under exact-current file v2/catalog v4 |
| InkScript M07 | Five Core-private adapter tests and two schema-composition tests cover all 7 owner primitives, bidirectional typed lowering, direct-vs-scripted canonical state equivalence, no-op/invalid/stale/resource/format/target/unknown-value atomicity, ownership, and absence of a second executor or product route. The user-confirmed private slice remains green |
| InkScript M08 | Ten Core-private grouped-adapter/image-codec tests cover all 6 owner primitives plus raster line-width projection, typed payload round-trip, grouping, exact-depth pairs, native separation, target policy, canonical equivalence, atomicity, ownership/thread suitability and absence of a second executor. The active slice is 13/75 |
| InkScript M09 | Four compiler/runner integration contracts plus the retained M06–M08 tests cover immutable parameter freeze/digests, aggregate and persistent-resource budgets, initial selector/assert/skip binding, single in-memory/native-byte staged execution, result availability/ordinal, success/no-op/invalid/missing/ambiguous/cancel/stale/overflow atomicity, source nonpublication, Undo/Redo and direct canonical Commit/revision/history/journal/next-ID/savepoint/final-digest parity. The private route is user-confirmed |
| InkScript M10 | Four Core-private asset contracts and one format public-model contract cover exact `data`/`data_file`, inline/authorized canonical identity and bytes, no-op, descriptor/digest/length/duplicate validation, truncation/extra data, individual and aggregate limits, cancellation, stale identity, reader failures, exact read/copy counters, typed role policy, immutable Rust ownership and product non-reachability. The private route is user-confirmed |
| InkScript M11 | Four Core-private planning contracts cover stable path intents, exact authority/object identity, dirty/pathless and sequence snapshots, open-session routing, natural order/range/dedup, Batch-compatible naming, collision/overwrite policy, external asset stale detection, checked number/resource bounds, cancel/failure atomicity, plan/confirmation digest order, one-shot scope and Core-engine-owned `Send` plan lifetime. No filesystem mutation, output install, public API, ABI or product route exists |
| InkScript M12 | Four Core-private integration contracts plus one ownership contract cover preview-ordered multi-item execution, nonblocking wait continuation, continue/stop, deterministic terminal reports, dry-run nonmutation, current-v27 prospective-savepoint encode/reopen, same-volume temporary and atomic install linearization, authority revalidation, cancel races, save/resource failures, direct canonical/Undo/Redo/cache-free replay/ID/savepoint parity, and `Send` task/adapter ownership. The OS adapter remains injected and private |
| InkScript M13 | The proposal fixes deterministic quick/full source, seed, inputs, work formulas, parser/compile/bind/runner/asset counters, checksums, timed interval, independent-process sample policy and environment-specific bounds. Every declared candidate-axis point and the selected full compound was measured with a temporary Release probe that was removed afterward. The quick contract and 64–107 ms x64 envelope are user-approved; the full contract remains deferred to M36, and no existing harness/envelope changed |
| InkScript M14 | The user-approved quick Release runner retains its 128-step/six-item semantic contract, independent-process policy and 64–107 ms x64 envelope without changing the existing benchmark harness. Catalog v3 changes only the included static compile digest; checksum and counters remain locked by the current quick gate |
| InkScript M15 | Seven contracts cover the exact 13-entry owner slice, bidirectional codecs, canonical equivalence, typed scalar and mixed layer/plane results, strict targets, atomicity, Undo/Redo, ID high-watermark, current-v27 full replay, and both savepoints. The active slice is 26/75 |
| InkScript M16 | Eight contracts cover the exact 8-entry metadata/color/guide owner slice, exact RGBA8/16, embedded-NUL chart names, guide order and chaining, strict/rebound selectors, atomicity, current-v27 full replay and both savepoints. The active slice is 34/75 |
| InkScript M17 | Seven contracts cover the exact 3-entry stroke/raster-geometry/import owner slice, Q16 and native-depth preservation, source-ordered samples, frozen raster assets, canonical equivalence, atomicity, Undo/Redo, cache-free replay, current-v27 full replay and both savepoints. The active slice is 37/75 |
| InkScript M18A | Four contracts cover fill-owner reuse and `apply_gradient`, Q16 conversion, RGBA16 ordered stops, checked work/resource formulas, selection clipping, atomicity, current-v27 full replay and both savepoints. The active slice is 38/75 |
| InkScript M18B | Four contracts cover the 11-entry raster gesture/effect, alpha, adjustment and scoped-color slice, source-ordered Q16 samples, selection clipping, typed adjustment chaining, atomicity, current-v27 full replay and both savepoints. The active slice is 49/75 |
| InkScript M19 | Five contracts cover the 11-entry raster selection/floating slice, pixel restore, layer result chaining, output-color guard, frozen raster assets, floating destinations, Q16 transforms, atomicity, current-v27 full replay and both savepoints. The active slice is 60/75 |
| InkScript M20 | The former eight-entry owner slice is retired. Catalog v4, current runtime, native v28 and ABI v18 contain no corresponding command, type, payload or adapter; immutable older catalogs remain only as rejection fixtures |
| InkScript M21 | The active two-entry slice owns document shooting-frame and vanishing-point commands. Closed typed CRUD, result chaining, binding, canonical parity, atomicity, current-v27 full replay and both savepoints are covered; the retired third entry is rejected. The active slice is 62/75 |
| InkScript M22 | Three contracts cover all 13 replayable Light Table entries, result chaining, frozen RGBA assets, all set/item edits, session-only/query/preview exclusion, atomicity, current-v27 full replay and both savepoints. The active catalog reaches 75/75 |
| InkScript M23 | The production catalog build fingerprint and registry suite prove the exact 75-way catalog/owner/replayable-primitive/Rust runtime/typed-adapter/equivalence mapping, prohibited-command exclusion, old-catalog rejection and generated-reference drift. Public integration covers compile, no-op, invalid source, catalog-v2 rejection, limits, cancellation, stale capture, atomicity, Undo/Redo/cache-free replay and single-writer ownership |
| InkScript M24 | Six public exporter contracts, one inline-asset unit contract, and seven strengthened M17–M22 family fixtures cover one/linear active/inactive selection, Genesis, exact parent assertions, external strict binding, typed references, retained assets, failure atomicity, cache-free replay and exact canonical parity. Visualization summaries/thumbnails are not materialized. No grammar, catalog, replay, native, ABI or Windows route changed; user confirmation is complete |
| InkScript M25 | Three FFI contracts plus one public standalone-value grammar contract, C header/export drift and ABI smoke cover source parse/summary/original-text copy, batched diagnostics, typed stored-default/override static compile, program summary, one-Commit fragment export/summary/text, NULL/alignment/short record/unknown flag and enum/oversize/cancel/resource/stale controller/stale Core generation/wrong-thread/double-release contracts, and nonmutation of document/editor revision, history, savepoints and persistent IDs. ABI v15 rejects v14; parser/catalog nodes and per-node calls remain absent. User confirmation is complete |
| InkScript M26 | Three FFI execution contracts, C header/export drift and ABI smoke cover copied authority, fixed DTO callbacks, immutable plan/preview, one-shot confirmation, PlanTask/RunTask lifecycle, atomic current-v28 install and detached reports. NULL/short/unknown/queue-full/cancel/stale/save-failure/double-release, cross-thread query/cancel, input nonmutation, output replay, Undo/Redo, ID high-watermark and savepoints are covered. ABI v18 rejects v17 |
| InkScript M27A | The private owner-thread Windows adapter implements ABI v18 authority/file callbacks with handle-relative no-follow traversal, file/alias/parent identity, native fingerprinting, open-session exclusion, verified-parent temporary files, overwrite guards and atomic install. Real-filesystem replacement/reparse/race/stale/resource/ownership/thread contracts are retained |
| InkScript M27B | The private bounded `CoreHost` route owns parse/compile/export/authority/plan/confirmation/run/report operations on the engine thread and emits pointer-free values. Focused tests cover success/no-op/invalid/cancel/stale/overflow/resource/save-failure atomicity, queue saturation, close/shutdown races, nonblocking `wait_ms`, native save/reopen, Undo/Redo, cache-free replay, ID high-watermark and both savepoints. Private ABI/application smoke reaches the production parser, catalog, executor and native writer; user confirmation is complete |
| Current Batch v3 rebaseline | Native v28/runtime epoch 25/ABI v19, InkScript schema/language/file v2 plus catalog/owner v4, and `.inkbatch` v3 are exact-current. The public InkScript registry remains 75 commands and excludes private `ApplyBatchOperations`; retired Batch enum/catalog/file assignments are rejected. This is not a native format-freeze declaration |
| Rust workspace | On 2026-08-24, format, the full workspace/all-feature test suite, warnings-denied Clippy across all targets/features including the approved benchmark, warning-denied `inkpod-core` rustdoc, and the unchanged quick benchmark all passed for v28/Batch v3 and the ABI v19 standalone Subpalette contract |
| Native format | V28/runtime replay epoch 25, ABI v19, Cell document archive schema 6, metadata schema 7, document digest schema 11/domain 8, EditorState schema 7/domain 2, snapshot-composite schema 4, Cut descriptor schema 2/epoch 24, and `.inkbatch` v3 are current. Exact top-level v27, `.inkbatch` v2, noncurrent archive/Cut versions, retired codes, checksum failures and corrupt inputs are rejected; the Subpalette ABI increment does not change native or replay data |
| Windows Subpalette | On 2026-08-24, ARM64 Debug rebuilt under MSVC 19.51 with `/W4 /WX`, static CRT, portable ZIP, and unsigned MSIX. The 40 non-product tests passed in 49.40 s; the final English/Japanese product smokes passed in 303.40/272.13 s and cover the independent pane policy, removed follow/current controls, icon actions, eyedropper cursor, sample-gated registration, and localized presentation. Core/ABI tests cover catalog ordering, exact sampling, staged decode, failure preservation, ownership, and wrong-thread rejection |
| Windows Batch v3 | On 2026-08-23, x64 Debug, x64 Release and ARM64 Debug built under `/W4 /WX` with static CRT, portable ZIP and unsigned MSIX. The current 42-test matrix passes in every configuration, including focused set-store, input-picker and color-editor contracts plus ABI v18 and English/Japanese product smokes. Product smoke covers the localized four-item popup catalog, fixed endpoints, standard operation-only checkboxes with click/Space routing, single-row color selection, native old/new alpha edits, drawing-color destination selection, editable loaded sets, cancel/disable state and staged new-tab ownership |
| Windows x64 | On 2026-08-23, current v28/ABI v18 x64 Debug and Release rebuilt under MSVC 19.51 with `/W4 /WX`; C11/C++20 header checks, executable, host tests, portable ZIP and unsigned MSIX built, and static CRT verification passed. Debug's 40 non-product-smoke tests passed in 124.16 s and its final English/Japanese product smokes passed in 336.37/337.64 s. Release's initial full run had one transient Japanese product-smoke code 367; the isolated rerun and final paired English/Japanese rerun passed, with the latter taking 105.83/100.96 s. Package payload gates used the final executable |
| Windows ARM64 | On 2026-08-23, current v28/ABI v18 ARM64 Debug rebuilt under MSVC 19.51 with `/W4 /WX`; the portable ZIP and unsigned MSIX built and static CRT verification passed. The full 42-test matrix passed in 439.04 s before the final layout-only correction, then the rebuilt final English/Japanese product smokes passed in 236.86/241.56 s; package payload gates used the final executable. The latest full ARM64 Release run remains the M13 run with all 31 then-current CTests passed in 65.35 s |
| Performance | The fixed nine-scenario quick and full profiles each passed twice with identical per-profile counters and checksums. The approved current-version rebaseline is `canonical_replay=70d3465b6732887e`, `checkpoint_open=bcf482082855c1f2`, quick output-color-guard `1005f8901846f431`, and full output-color-guard `558cb3aacd55afd9`; workload, harness, payload-access route, revision-max expression, counters and approved envelopes are unchanged. The previously recorded approved InkScript quick gate retained checksum `4401131d804c8eb7` and all counters at 88.9569 ms inside the unchanged 64–107 ms x64 envelope; its full contract remains deferred to M36 |
| Fuzzing | `native_v28`, `native_core_v28`, `cut_v28`, `inkscript_lexer_v2`, and `inkscript_parser_v2` are the exact-current target declarations. Coverage-guided execution has not been run for this change |

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
