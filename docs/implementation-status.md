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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Transactions publish document, `StateId`, revision, history, journal, dirty state, ID authorities, and cache invalidation atomically. History entries retain a language-neutral `HistoryEntryKind`; product labels are neither stored nor compared in Core. The read-only history-visualization query replays every retained branch, emits `Commit` rows in journal-event order, formats bounded typed arguments, and renders deterministic post-commit thumbnails without changing live state. Bounded stable-ID vanishing points support Canvas-exterior coordinates, exact RGBA8/16, opacity and visibility, arbitrary 1–180 degree intervals and phase, canonical CRUD, noncumulative preview/Cancel, deterministic viewport-clipped radial guides, guide/radial/grid snap precedence, and document transforms. Unequal-axis resampling is rejected atomically because it cannot preserve an equal-angle radial family. Sequence endpoint and shooting-frame owners retain their existing contracts. Text/Annotation and the separate Cut, geometry, output-color guard, Color chart, Light Table, brush, replacement, vector, Batch, clipboard, Genesis, and asset contracts retain their existing owners. |
| Persistence | Native `.inkpod` is exact-current v26/runtime replay epoch 23 and `.inkbatch` is exact-current v2. Cell and Cut `.inkpod` files use distinct magic values; Cut payload schema 2 remains current. Cell `DocumentArchive`/document metadata schema 5 persists the optional shooting frame, bounded annotation records, and bounded vanishing-point records; `DocumentStateDigest` schema 9/domain 8 covers the new semantic fields, while canonical snapshot-composite schema 3 is unchanged. `META/GENS/ASST/PROC/EDIT` remain authoritative and optional `CKPT` only accelerates open. Non-v26 input, including exact v25, is rejected without migration. Atomic Cell/Cut savepoint rules are unchanged. The application-wide sequence endpoint preference remains a separate bounded HKCU record at version 1. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. The UI has Japanese and English presentation; Edit > Settings > Language stores a versioned process-wide System/Japanese/English preference, with System resolving Japanese only from a first-preferred `ja` Windows UI language and otherwise falling back to English on the next launch. All Japanese/English product presentation is selected by typed `UiStringId` from one generated catalog; exact-language ja-JP/en-US STRINGTABLE, menu, and dialog resources use the same IDs and are loaded explicitly. Hook-based and partial replacement no longer exist. History menu/dialog text is resolved from fixed-width ABI v15 `InkpodHistoryEntryKind`, never from an English or Japanese string key. Tool owner draw, tooltip, MSAA and UIA share label IDs; Layer/Plane kind, format, visibility, editability, detail and accessible text are resolved before drawing; Plane badges use separate typed compact labels that fit the unchanged 42-by-42-DIP badge in at most two lines at the active 9-point font, while detail and accessibility retain the full kind label; owner-draw status cells use the widest localized state caption measured in the same font plus DPI-scaled padding, with one geometry shared by drawing and hit testing; Color/Palette/Chart tabs are ID-generated. DockHost inspector headers use the same DPI-scaled 9-point Segoe UI ClearType font as pane tabs and recreate it when the workspace DPI changes. Locator presentation caches its last text, option state, and neighborhood and skips idempotent `WM_SETTEXT`, check-state updates, and neighborhood invalidation during high-frequency pointer sampling; when replacement text is shorter, it completes one background-erasing redraw immediately so the old glyph tail cannot remain. Pane action buttons use the active control font and Common Controls ideal size, preserve full actionable captions, and wrap into additional rows when necessary; only variable user-owned target text may be ellipsized. The source gate rejects Japanese product literals outside the canonical catalog and generated Japanese artifacts and rejects the retired direct-language/history-label routes. User edit text, document-tab names, file paths, and Light Table set names stay outside localization and are composed only through an explicit user-text boundary. Tools > Inkpod file visualization dynamically lists open native-session paths and opens one modeless, cancellable, owner-data three-column history list per document without blocking the UI thread. Cell > Vanishing Point exposes typed create/update properties, handle-edit mode, and delete-all; the existing Layer pane owns VanishingPoint layers, while Canvas drags use begin/update/apply/Cancel preview calls. Production previous/next sequence commands retain the Core step-plan and Stop/Wrap preference route. Cell > Shooting Frame and File > Export Instruction Image retain their prior contracts. Cut properties, save, history, and structural sequence commands capture the issue-time workspace/session/generation and Cut revision, reject stale or mismatched document targets, and never fall back to a later active Cell. Existing annotation, geometry, pane, command-state, and accessibility routes remain connected without a process-global active-document pointer. |
| macOS frontend | M0–M10 are complete through Sandbox file lifecycle, document/layer/plane multi-view workspace, paint/selection/history, filter/effect/vector/annotation, Cut/Sequence/Light Table/Subpalette/Reference/motion, and Batch/job workflows. A value-only `CoreHost` keeps all live Core, Cut, view, snapshot, plan, clipboard, preview, stroke, history-builder, Batch graph/run-copy/pair-preview/task/report, and export-buffer pointers in private registries on one fixed `Foundation.Thread`; the MainActor coordinator and dedicated Metal renderer remain separate. M10's value-keyed Batch `WindowGroup` captures issue-time workspace/session/generation/revision, freezes a value-only graph before enqueue, retains balanced security-scoped input/output leases for the exact job lifetime, and cancels/awaits the job on window/workspace close or shutdown. Loaded `.inkbatch` operations are copied to values, explicitly resolved per run, and cloned to an immutable Rust-owned run graph; Swift never implements graph, image, history, selection, or format semantics. SwiftUI receives only value projections; stale work never falls back to another document. ABI v15, native v26, `.inkbatch` v2, and replay epoch 23 are unchanged. All 384 ledger rows are implemented: M10 contributes 50 `macEquivalent` rows and one native-surface `notApplicable` row. |
| Rendering and performance | Immutable snapshots carry mixed raster/vector/annotation/shooting-frame/vanishing-point spans plus bounded viewport-clipped radial guides, snapshot-owned pools, Core-ordered passes, adjustment LUTs, Light Table overlays, view-local diagnostics, selections and floating previews. The Windows renderer retains its device-loss coverage. The macOS product Canvas uses a custom SDR sRGB `CAMetalLayer`, premultiplied blending, alternate-rule stencil vector fill, offscreen group-opacity and adjustment ping-pong passes, CoreText font resolution, a process-wide session/tile/revision texture cache, retained latest snapshots, hidden-surface rejection, and GPU-only rebuild/purge on display/device change or memory pressure. Product-scene tests prove pan/zoom reuse without full tile re-upload, document-raster redraw after simulated device/display and memory events, and M9 Light Table snapshot presentation with exactly-once release; M8/M9 pass the same Metal API Validation profile. Raster revision-max validation remains scalar-only and the audited source-call-graph lock plus payload counters remain green. |
| Product surface | New Cut, structural sequence editing and endpoint Stop/Wrap selection, drawing/fill/vector/effects, Text/Instruction annotations, angled shooting-frame properties/handles/export, multiple vanishing-point properties/handles/radial snap, selection, layer/plane, transform, Light Table, clipboard, common-raster import/export, Batch, history visualization, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. macOS exposes the M2 Canvas, M3 commands/settings/localization, M4 file/Sandbox/clipboard workflow, M5 configurable New Cell/Cell Properties and multi-view workspace, M6 Tool/Options and raster paint/fill/color, M7 Selection/Transform/History, M8 Filter/Effect/Adjustment/Vector/Annotation/Shooting Frame/Vanishing Point/diagnostics, M9 Cut/Sequence/Light Table/Subpalette/Reference/motion, and M10 dedicated Batch graph/progress/report surfaces. |
| Build and distribution | CMake drives both the Rust static library/MSVC C++20 build and the macOS Cargo→Xcode sub-build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. macOS Debug and Release fix Cargo, CMake, and Xcode to arm64; the Release gate requires the Rust archive, XCTest executable, 64-session headless CoreHost, and `Inkpod.app` to contain exactly the arm64 architecture. The release CLI composes that arm64-only target with versioned app staging, bottom-up Hardened Runtime signing, compressed DMG creation, notary submission, ticket stapling, and Gatekeeper assessment; real Developer ID/notary execution remains unverified. Unsigned MSIX and four-file portable ZIP packaging paths are maintained for Windows. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `WIN-001` | Native Windows shell, Japanese/English UI with persisted System/Japanese/English selection and non-Japanese English fallback, offline Help/About/Acknowledgements with locked BLAKE3/PNG dependency attribution, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the embedded offline Help body is Japanese-only |
| `WORKSPACE-001` | Windows retains its eight DockHost surfaces, presets, V7 bounded layout, monitor/DPI recovery, AutoHide pointer access, and accessible names. macOS now has maximum-two nonrecursive EditorGroups, tabs, Layer/Plane plus M6 Color/Palette/Chart/Locator inspectors with follow/pin, five presets, and bounded named property-list layouts with malformed recovery and display clamp | Windows Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab; physical multiple-display macOS interaction is unverified |

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
- The macOS frontend currently stops at M10. M11 parity-freeze hardening,
  full accessibility, and distribution remain later work.
  Physical-tablet, multiple-display, sleep/wake, Instruments, and complete screen-reader/
  IME interaction remain unverified. Intel Mac is intentionally unsupported.

## Latest representative verification

The latest automatic verification is the macOS arm64-only build and distribution
change on 2026-08-16. Fresh Debug and Release presets fixed CMake, Cargo, and
Xcode to arm64; clean builds used `aarch64-apple-darwin` with deployment target
26.0. The Release architecture gate reported exactly `arm64` for the Rust
archive, XCTest bundle, CoreHost executable, and `Inkpod.app`; a deliberate
`CMAKE_OSX_ARCHITECTURES=x86_64` configure was rejected. The Debug Core/Xcode
check and CTest 20/20 passed, as did all four release-CLI contract tests, Rust
formatting, all-target/all-feature Clippy with warnings denied, and all 462 Rust
tests. The preceding product-feature verification remains the macOS
M10 Batch/job slice on 2026-08-16. Its arm64 CMake profile built the Rust archive and
`Inkpod.app`, compiled the canonical wrapper as C11 and C++20 with warnings
denied, passed all 88 strict Swift 6 unit/integration tests and the separate 64-session headless
executable, and validated the value-only CoreHost boundary. M10 did not change the
renderer; the prior M9 V-Metal evidence remains current, and this run's product
lifecycle used Metal validation with zero reported errors. V-MacUI passed all 14 selected tests: six product Canvas lifecycle
tests and eight launched-product XCUITests, including a dedicated Batch window
dry-run that retained its one-item report.
The 384-row parity ledger marks all rows implemented; M10's 51 rows are 50
`macEquivalent` plus one native-surface `notApplicable`. All 612 required
String Catalog keys have complete English and Japanese values. CTest
passed 20/20. V-Rust passed fmt, all-target/all-feature Clippy, all 462 tests,
the full ten-scenario benchmark semantic gates, and strict rustdoc. Native format
v26, replay epoch 23, canonical procedure format 26, and public ABI v15 are
unchanged.
The prior M2 Thread Sanitizer evidence remains current for that unchanged path.
The isolated macOS release-CLI regression passed all
four cases for the arm64-only repository/default-preset contract, dependency
ordering, packaged version metadata, temporary-file cleanup, the `notarize`
subcommand, and early ad-hoc-identity rejection; it used
fake signing/notary tools and
therefore is not Developer ID or Apple notary evidence. A prior real local ad-hoc
run covered signing, compressed-DMG creation, and `hdiutil verify`; the current
arm64-only distribution still requires equivalent real signing and DMG evidence.
Developer ID signing and Apple notarization were not attempted.
Native Windows builds and
the hardware/manual macOS checks listed above were not available in this
environment; their last recorded evidence remains below.

| Boundary | Result |
| --- | --- |
| Rust workspace | All 462 tests, including one doctest and zero ignored, passed; `fmt`, all-target/all-feature Clippy with warnings denied, strict rustdoc, architecture gates, and the route inventory passed. Shortcut V2 covers Unicode/named keys, logical modifiers, prefix-free transactionality, invalid records, reset and pure resolution without changing document/replay state |
| Native format | V26/runtime replay epoch 23, ABI v15, Cell document/archive metadata schema 5, document digest schema 9/domain 8, snapshot-composite schema 3, Cut descriptor schema 2, and `.inkbatch` v2 are current. Exact top-level v25, noncurrent archive/Cut versions, malformed vanishing-point/shooting-frame records, checksum failures, and corrupt corpus are rejected; Cell/Cut save/reopen is green |
| macOS arm64 | CMake→Cargo→Xcode M10 check passed on macOS 26.6.1 with Xcode 26.6/Tahoe SDK. All 88 strict Swift tests, the 64-session headless executable, six product Canvas lifecycle tests, eight launched-product XCUITests, 384-command parity with all rows implemented, 612-key en/ja localization, CoreHost source boundary, M10 all-operation/natural-order/dry-run/cancel/collision/bookmark/close/shutdown success and negative coverage, Batch graph/task/report/pair/run-copy exactly-once ownership, prior Sandbox/file/clipboard/Cut/animation regressions, and CTest 20/20 passed. The required M10 V-MacUI target passed all 14 selected tests with Metal validation error count zero |
| macOS Thread Sanitizer | The prior M2 19-XCTest and headless-executable run passed under Xcode Thread Sanitizer with zero failures or skips; M3–M8 did not require or rerun this extended profile |
| macOS arm64 Release | CMake, Cargo, Xcode, the release CLI, and the repository contract test select arm64 only. The clean Release validation reported exact `arm64` output from `lipo -archs` for the Rust static archive, linked XCTest executable, CoreHost headless executable, and `Inkpod.app`; Intel Mac is not supported |
| Windows x64 | M10 made no Rust ABI/native-format or Windows production-code change. Canonical-header C11/C++20, Windows route/source-boundary static regressions, and the existing shared Rust tests passed in the macOS profile, but native Debug/Release configure presets are disabled on this non-Windows host. The current 2026-08-14 Debug `inkpod` and localization-test targets otherwise remain the latest MSVC `/W4 /WX` static-CRT evidence. Their full-path English and Japanese product smoke runs passed in 186.20 s and 185.05 s; the immediately preceding full Debug run passed all 36 CTests in 438.87 s, and the latest Release run remains the 2026-08-13 M22 run with all 33 then-current CTests passed in 75.57 s |
| Windows ARM64 | Not run for M22 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | The required full profile passed every checksum/revision/history/reuse/rebuild/output/failure gate. M10 `batch_preview` completed 16 successes plus one intentional failure with checksum `6732b8b0a6565d03`; `canonical_replay` remains `264b98028ac92ac6` at revision 6/history 5; full `checkpoint_open` remains `07da1b4e6bc5d289` with 1000256 input, 256 output, and one reused item; full output-color-guard is `2b2196e06f7198b3` with 4194304 input, 2097152 output, and 262144 reused items. Workload, harness, payload-access route, and revision-max expression are unchanged |
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
