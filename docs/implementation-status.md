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
| Persistence | Native `.inkpod` is exact-current v26/runtime replay epoch 23 and `.inkbatch` is exact-current v2. Cell and Cut `.inkpod` files use distinct magic values; Cut payload schema 2 remains current. Cell `DocumentArchive`/document metadata schema 5 persists the optional shooting frame, bounded annotation records, and bounded vanishing-point records; `DocumentStateDigest` schema 9/domain 8 covers the new semantic fields, while canonical snapshot-composite schema 3 is unchanged. `META/GENS/ASST/PROC/EDIT` remain authoritative and optional `CKPT` only accelerates open. Normal save can now prepare a prospective-savepoint file without changing live state and commit path/savepoints only after frontend publication succeeds; legacy one-call save internally uses the same two phases. Non-v26 input, including exact v25, is rejected without migration. Atomic Cell/Cut savepoint rules are unchanged. The application-wide sequence endpoint preference remains a separate bounded HKCU record at version 1. |
| Windows frontend | UI/Input, Core engine, and Renderer remain separate owner threads connected by bounded value/ownership queues. The UI has Japanese and English presentation; Edit > Settings > Language stores a versioned process-wide System/Japanese/English preference, with System resolving Japanese only from a first-preferred `ja` Windows UI language and otherwise falling back to English on the next launch. All Japanese/English product presentation is selected by typed `UiStringId` from one generated catalog; exact-language ja-JP/en-US STRINGTABLE, menu, and dialog resources use the same IDs and are loaded explicitly. Hook-based and partial replacement no longer exist. History menu/dialog text is resolved from fixed-width ABI v15 `InkpodHistoryEntryKind`, never from an English or Japanese string key. Tool owner draw, tooltip, MSAA and UIA share label IDs; Layer/Plane kind, format, visibility, editability, detail and accessible text are resolved before drawing; Plane badges use separate typed compact labels that fit the unchanged 42-by-42-DIP badge in at most two lines at the active 9-point font, while detail and accessibility retain the full kind label; owner-draw status cells use the widest localized state caption measured in the same font plus DPI-scaled padding, with one geometry shared by drawing and hit testing; Color/Palette/Chart tabs are ID-generated. DockHost inspector headers use the same DPI-scaled 9-point Segoe UI ClearType font as pane tabs and recreate it when the workspace DPI changes. Locator presentation caches its last text, option state, and neighborhood and skips idempotent `WM_SETTEXT`, check-state updates, and neighborhood invalidation during high-frequency pointer sampling; when replacement text is shorter, it completes one background-erasing redraw immediately so the old glyph tail cannot remain. Pane action buttons use the active control font and Common Controls ideal size, preserve full actionable captions, and wrap into additional rows when necessary; only variable user-owned target text may be ellipsized. The source gate rejects Japanese product literals outside the canonical catalog and generated Japanese artifacts and rejects the retired direct-language/history-label routes. User edit text, document-tab names, file paths, and Light Table set names stay outside localization and are composed only through an explicit user-text boundary. Tools > Inkpod file visualization dynamically lists open native-session paths and opens one modeless, cancellable, owner-data three-column history list per document without blocking the UI thread. Cell > Vanishing Point exposes typed create/update properties, handle-edit mode, and delete-all; the existing Layer pane owns VanishingPoint layers, while Canvas drags use begin/update/apply/Cancel preview calls. Production previous/next sequence commands retain the Core step-plan and Stop/Wrap preference route. Cell > Shooting Frame and File > Export Instruction Image retain their prior contracts. Cut properties, save, history, and structural sequence commands capture the issue-time workspace/session/generation and Cut revision, reject stale or mismatched document targets, and never fall back to a later active Cell. Existing annotation, geometry, pane, command-state, and accessibility routes remain connected without a process-global active-document pointer. |
| macOS frontend | M0–M11 are complete through Sandbox file lifecycle, multi-view workspace, paint/selection/history, filter/effect/vector/annotation, Cut/animation, Batch/jobs, and collapsible Liquid Glass workspace chrome. File lifecycle commands are anchored after the `WindowGroup` new-item group, so Save and Save As are visible in the File menu and register Command-S and Shift-Command-S; launched-product tests cover both menu click and shortcut presentation of the Sandbox save panel. Normal save keeps the selected-URL security lease and file coordinator open while Core writes to Foundation's same-volume replacement directory, publishes with Foundation replace/move, and only then commits the Core path/savepoints; failure is presented as a localized alert. M12's automated hardening and distribution path is implemented without new product semantics: one source gate freezes all 384 implemented parity rows and required fault/a11y/soak evidence, the release CLI runs every automated profile before a real arm64 archive, and the protected workflow owns Developer ID/notary credentials. A value-only `CoreHost` keeps all live Core, Cut, view, snapshot, plan, clipboard, preview, stroke, prepared-save, history-builder, Batch graph/run-copy/pair-preview/task/report, and export-buffer pointers in private registries on one fixed `Foundation.Thread`; the MainActor coordinator and dedicated Metal renderer remain separate. SwiftUI receives only value projections and stale work never falls back to another document. ABI v15, native v26, `.inkbatch` v2, replay epoch 23, Metal shaders, and the parity ledger are unchanged. M12's automated, Developer ID signing, notarization, staple, and Gatekeeper paths have passed; overall completion still requires the recorded physical interaction and clean-Tahoe evidence. |
| Rendering and performance | Immutable snapshots carry mixed raster/vector/annotation/shooting-frame/vanishing-point spans plus bounded viewport-clipped radial guides, snapshot-owned pools, Core-ordered passes, adjustment LUTs, Light Table overlays, view-local diagnostics, selections and floating previews. The Windows renderer retains its device-loss coverage. The macOS product Canvas uses a custom SDR sRGB `CAMetalLayer`, premultiplied blending, Core-ordered mixed passes, a process-wide tile cache, retained latest snapshots, hidden-surface rejection, and GPU-only device recovery. It clears the complete backing viewport to a neutral Canvas color, draws paper/color-check/bounded transparency checker only inside the zoom/pan/flip-transformed half-open document rectangle, and limits adjustment LUTs to that rectangle, so a 1:1 Retina document no longer makes out-of-document right/bottom space look editable. M11 leaves glass composition outside Metal and passes the reduced SwiftUI safe region through existing Canvas resize ownership; product tests prove backing bounds/drawable agreement, half-open last-pixel input, 200 replacement/resize releases exactly once, tile reuse greater than upload, at most four uploads, and hidden draw count zero under Metal API Validation. Raster revision-max validation remains scalar-only and the audited source-call-graph lock plus payload counters remain green. |
| Product surface | New Cut, structural sequence editing and endpoint Stop/Wrap selection, drawing/fill/vector/effects, Text/Instruction annotations, angled shooting-frame properties/handles/export, multiple vanishing-point properties/handles/radial snap, selection, layer/plane, transform, Light Table, clipboard, common-raster import/export, Batch, history visualization, recovery, and compaction-copy commands are connected from the Windows UI to their owning Core or OS adapter. macOS additionally exposes the M2–M10 feature surfaces inside M11's single workspace chrome: a collapsible glass Tool/Options surface, maximum-size central Metal Canvas, one five-tab inspector with opaque color-judging wells, unchanged bottom Sequence timeline, and unchanged dedicated Batch window. |
| Build and distribution | CMake drives both the Rust static library/MSVC C++20 build and the macOS Cargo→Xcode sub-build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. macOS Debug and Release fix Cargo, CMake, and Xcode to arm64; the Release gate now produces a real `Inkpod.xcarchive` and requires the Rust archive, XCTest executable, 64-session headless CoreHost, and archived app executable to contain exactly arm64. `scripts/macOS.sh verify` runs Rust, Swift/ABI/parity, UI accessibility, Metal soak, Thread Sanitizer, and archive profiles; `release` cannot skip Developer ID signing/notarization. `publish` independently verifies the existing notarized DMG, requires a clean synchronized release commit, fixes the version tag to exact HEAD, adds a missing DMG to an existing public Release or creates a prerelease, and accepts tag/Release/upload races only for the same commit or byte-identical asset; it never clobbers different published bytes. The protected workflow imports ephemeral signing/notary credentials, and the checklist requires physical and clean-Tahoe evidence to be Pass/Fail/Blocked rather than inferred. Version 0.2.3/build 175 was signed with Developer ID team `ETD7LJJGQZ`; notarization submission `fd8ccacc-c1a2-4d30-9c5b-064ea38fc901` was Accepted with no issues, its ticket was stapled and validated, and Gatekeeper accepted the DMG as `Notarized Developer ID`. GitHub publication remains pending until the candidate source is committed on a clean synchronized release branch. Unsigned MSIX and four-file portable ZIP packaging paths are maintained for Windows. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. macOS shares the Canvas paper background with the Locator visualization and uses one bounded geometry for centered, inspector-width-responsive square-cell drawing and fixed-mode hit testing. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `WIN-001` | Native Windows shell, Japanese/English UI with persisted System/Japanese/English selection and non-Japanese English fallback, offline Help/About/Acknowledgements with locked BLAKE3/PNG dependency attribution, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | High contrast, 200% DPI, complete screen-reader, and Japanese IME validation/fixes; the embedded offline Help body is Japanese-only |
| `WORKSPACE-001` | Windows retains its eight DockHost surfaces, presets, V7 bounded layout, monitor/DPI recovery, AutoHide pointer access, and accessible names. macOS now has maximum-two nonrecursive EditorGroups, a fixed 54-point Tool strip with explicit Canvas-side transient/pinnable option popovers, one five-tab inspector with follow/pin, five presets, and bounded named layout-v3 records with deterministic v1/v2 migration, malformed recovery, and display clamp | Windows Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab; physical multiple-display macOS interaction is unverified |

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
- The M12 automated parity-freeze, hardening, archive, CI, and distribution
  orchestration is implemented and the current candidate passed Developer ID
  signing, notarization, staple validation, and Gatekeeper assessment.
  Clean-Tahoe launch, physical tablet, multiple-display, sleep/wake,
  Instruments, and complete VoiceOver/IME/appearance interaction remain
  unverified. Intel Mac is intentionally unsupported.

## Latest representative verification

The latest automatic verification is the macOS M12 hardening/distribution slice on
2026-08-16. Its arm64 CMake profile built the Rust archive and `Inkpod.app`,
compiled the canonical wrapper as C11 and C++20 with warnings denied, passed all
104 strict Swift 6 unit/integration tests plus the separate 64-session headless
executable, and validated the value-only CoreHost boundary. V-MacUI passed all
19 selected tests: eight product Canvas lifecycle tests and eleven launched-product
XCUITests. The new coverage exercises one five-tab inspector, a macOS-only
Workspace menu distinct from the system Window menu, a standard About panel with
the Windows product description and copyright, menu/shortcut/context-menu routes
without a duplicate primary-action toolbar, product
accessibility audit, 640/800/1200-point resize and reversible adaptation, stale
inspector reopening, unchanged document/history/dirty state, the explicit Tool
Options disclosure/pin/active-tool-retarget/close flow, layout-v3 v1/v2
migration, half-open backing-pixel bounds, and 200 resize/snapshot ownership
cycles. V-Metal repeated the product lifecycle with Metal API Validation and
reported no error; tile reuse exceeded uploads, uploads remained at most four,
hidden draws were zero, and every snapshot released exactly once.
The current Canvas regression coverage additionally preserves signed two-axis
AppKit scroll deltas during Retina point-to-backing conversion, fixes the
1920-by-1080 document paper boundary inside a larger 2706-by-1516 Retina backing
viewport, and checks pan, zoom, flip, half-open limits, and bounded transparent
checker work.
The current supplemental Sandbox-save verification also created a 6,192-byte
current-v26 document on iCloud Desktop with a freshly signed Debug app; no
Sandbox deny was logged. Prepared-publication success, missing candidate,
publication failure, stale token, and savepoint/path atomicity are automated.

The 384-row parity ledger remains fully implemented; M12 adds or changes no row.
All 618 required String Catalog keys have complete English and Japanese values,
and CTest passed 22/22, including the release-contract and 17-case release-CLI
tests. The publication regression accepts compact one-line `codesign`
entitlement output while continuing to reject any entitlement outside the exact
three-key allowlist. V-Rust passed formatting, all-target/all-feature Clippy
with warnings denied, all 465 tests, the quick ten-scenario semantic gates, and
strict rustdoc. The complete 95-test/headless suite also passed under Thread
Sanitizer. V-MacUI passed 19/19 and V-Metal reported zero validation errors.
Native format v26, replay epoch 23, canonical procedure format
26, `.inkbatch` v2, public ABI v15, and Metal shaders are unchanged. The preceding
arm64-only Release gate now creates a real `Inkpod.xcarchive`; the Rust archive,
XCTest bundle, CoreHost executable, and archived `Inkpod.app` reported exactly
`arm64`. A real local ad-hoc run produced version 0.2.3/build 0 with Hardened
Runtime and only the three approved Sandbox/file entitlements, then created a
compressed DMG whose checksum passed `hdiutil verify`. The release CLI then
rebuilt version 0.2.3/build 175, signed the arm64 app and DMG with Developer ID
team `ETD7LJJGQZ` plus Hardened Runtime and secure timestamps, and submitted it
as `fd8ccacc-c1a2-4d30-9c5b-064ea38fc901`. Apple returned `Accepted` with no
issues; stapler validation and Gatekeeper's `Notarized Developer ID` assessment
passed. The final stapled DMG SHA-256 is
`4b8bac5b70f21471c424cbd7acd62bfcae419ab179b1f83e4d62dc385248020f`.
Native Windows builds and
the hardware/manual macOS checks listed above were not available in this
environment; their last recorded evidence remains below.

| Boundary | Result |
| --- | --- |
| Rust workspace | All 465 tests, including one doctest and zero ignored, passed; `fmt`, all-target/all-feature Clippy with warnings denied, strict rustdoc, architecture gates, and the route inventory passed. Prepared save adds side-effect-free candidate creation, stale-token rejection, post-publication savepoint commit, and FFI ownership coverage. Shortcut V2 covers Unicode/named keys, logical modifiers, prefix-free transactionality, invalid records, reset and pure resolution without changing document/replay state |
| Native format | V26/runtime replay epoch 23, ABI v15, Cell document/archive metadata schema 5, document digest schema 9/domain 8, snapshot-composite schema 3, Cut descriptor schema 2, and `.inkbatch` v2 are current. Exact top-level v25, noncurrent archive/Cut versions, malformed vanishing-point/shooting-frame records, checksum failures, and corrupt corpus are rejected; Cell/Cut save/reopen is green |
| macOS arm64 | The current CMake→Cargo→Xcode automatic gate passed on macOS 26.6.1 with Xcode 26.6/Tahoe SDK. All 104 strict Swift tests, the 64-session headless executable, eight product Canvas lifecycle tests, eleven launched-product XCUITests including the accessibility audit, distinct Workspace/system Window menus, localized About description/copyright, and Tool Options disclosure/pin/retarget/close flow, 384-command parity with all rows implemented, 618-key en/ja localization, CoreHost and release source boundaries, layout-v3 migration, signed Retina scroll-delta conversion, Retina document-paper/backing-viewport separation, prepared-save publication/failure atomicity, fault/no-op/cancel/stale/failure coverage, 200-cycle Metal ownership/reuse metrics, prior M2–M11 regressions, and CTest 22/22 passed. A freshly signed Sandbox Debug app created a 6,192-byte current-v26 file on iCloud Desktop and produced no Sandbox deny log. V-MacUI passed 19/19 and V-Metal reported zero validation errors |
| macOS Thread Sanitizer | The complete current 95-test Swift suite plus headless executable passed under Xcode Thread Sanitizer with zero reported races, failures, or skips |
| macOS arm64 Release | CMake, Cargo, Xcode, the release CLI, and repository gates select arm64 only. Release produced a real `Inkpod.xcarchive`; `lipo -archs` reported exact arm64 for the Rust static archive, linked XCTest executable, CoreHost headless executable, and archived app. Version 0.2.3/build 175 passed exact entitlement inspection, Developer ID signing, Apple notarization (`fd8ccacc-c1a2-4d30-9c5b-064ea38fc901`, Accepted/no issues), staple validation, Gatekeeper `Notarized Developer ID`, and final DMG checksum verification. Clean-machine evidence remains outstanding; Intel Mac is not supported |
| Windows x64 | M11 made no Rust ABI/native-format or Windows production-code change. Canonical-header C11/C++20, Windows route/source-boundary static regressions, CTest's Windows static gates, and all shared Rust tests passed in the macOS profile, but native Debug/Release configure presets are disabled on this non-Windows host. The current 2026-08-14 Debug `inkpod` and localization-test targets remain the latest MSVC `/W4 /WX` static-CRT evidence. Their full-path English and Japanese product smoke runs passed in 186.20 s and 185.05 s; the immediately preceding full Debug run passed all 36 CTests in 438.87 s, and the latest Release run remains the 2026-08-13 M22 run with all 33 then-current CTests passed in 75.57 s |
| Windows ARM64 | Not run for M22 and not used as a substitute for the required x64 gate. The latest M13 ARM64 Release run passed all 31 CTests in 65.35 s |
| Performance | M11's required quick profile passed all ten checksum/revision/history/reuse/rebuild/output/failure gates; `canonical_replay` remains `264b98028ac92ac6` at revision 6/history 5 and quick `checkpoint_open` remains `07da1b4e6bc5d289`. The product Metal test completed 200 resize/snapshot cycles with exactly-once release, zero hidden draws, reuse greater than upload, and no more than four uploads. The preceding full M10 profile and approved envelopes remain current because workload, harness, Core, ABI, payload-access route, and revision-max expression are unchanged |
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
