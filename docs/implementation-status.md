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
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo, and replay. Batch v4 resolves every selected Color Replace layer selector to all matching stable plane IDs, deduplicates overlaps, and lowers the result together with the other enabled operations to one private `ApplyBatchOperations` procedure and one Undo unit. Missing/invalid/hidden/non-editable/cancelled targets publish nothing. The canonical payload and native v28/replay-25 contract remain unchanged because lowering emits the existing single-target operation records. Contact-sheet preview and sparse fill-protection semantics remain unchanged. |
| Persistence | Native Cell `.inkpod` remains exact-current v28/runtime replay epoch 25 and `.inkbatch` is exact-current v4. `.inkbatch` v4 stores a bounded nonempty target-selector array per operation and rejects v1-v3 without migration. Cell/Cut schemas, document/archive/editor digests, authoritative sections, checkpoint behavior, and atomic savepoint rules are unchanged. |
| InkScript | Exact-current registry schema/language/file v2, catalog/owner manifest v4, replay epoch 25, native v28 and ABI v22 are aligned. The public catalog remains exactly 75 commands; the private Batch v4 `ApplyBatchOperations` procedure is intentionally excluded. No `.inkscript` product route was added; `.inkbatch` v4 remains the independent product Batch contract. |
| Windows frontend | Right-side Splitter geometry updates now preserve existing Common Controls tab items and pane data and skip unchanged child placements. Color lays out every child without intermediate redraw, keeps the page tab behind its sibling controls, raises only the visible active-page controls, and then completes a bounded synchronous repaint; owner-drawn color surfaces repaint without background erase while standard controls erase before painting. The combined swatch, main-line and drawing-color labels, and HSV/opacity picker reproduce the corresponding live themed-tab pixels instead of filling with a fixed system-face brush; antialiased swatch and picker edges blend into those captured pixels, and the picker cache is invalidated after tab, theme, or system-color changes. Color and Layer child clipping prevents sibling overdraw during live resize. Width-dependent Layer/Plane owner-draw rows paint synchronously once after their list geometry changes, so right-aligned status cells cannot leave their old frame visible between pointer samples. Right-tab stack-boundary changes use a separate lightweight layout kind and synchronously erase and repaint only the strip spanning the old and new boundary, including intersecting pane headers and children, so vertically moved headers and frames cannot remain at their prior positions. Color now declares a 300-DIP minimum height, including its Dock header, and a 360-DIP preferred floating height, so its target row, Color/Palette/Chart tabs, main-line/drawing-color swatches, picker, opacity track, and numeric row remain present. Mouse and keyboard stack-boundary changes derive both adjacent limits from their pane descriptors and current pair extent, stop without changing the split weights at either minimum, and normalize legacy out-of-range weights toward the first valid movement. Structural dock changes retain the full refresh path. The Batch pane retains its editable fixed Input/Output graph and four-operation catalog. Color Replace exposes standard checkboxes for general-raster, binary-coloring, and grayscale-coloring targets; one or more may be selected together, all selectors round-trip through `.inkbatch` v4/ABI v22, and externally loaded fixed-ID selectors remain intact until semantic selection replaces them. Preview/run/save still builds one immutable graph, and the existing singleton tab, generated Japanese/English presentation, cancellable preview, issue-time targeting, and Rust-owned staged-result routes remain. |
| Preferences and shortcuts | `編集 > 環境設定` opens one resizable, owner-centered six-tab dialog for all persisted application/workspace preferences. Its 920-by-680-DIP initial bounds replace the former oversized expansion. The dialog creates one current-DPI 9-point Segoe UI font for its resource and dynamic controls, recreates it after DPI changes, and derives readable rows from the selected font's measured text height plus DPI-scaled padding. Every unframed page `STATIC` reproduces the corresponding coordinates of the live themed tab surface with transparent text, while `WS_EX_CLIENTEDGE` value fields retain their independent system-window surface. Page switches, resizes, theme changes, and system-color changes invalidate the tab surface and visible controls so old frames cannot remain. Shortcut labels for preset, primary/secondary, action, context, modifiers, and keyboard layout derive their widths from the localized text measured with the selected font. The modifier radios use dedicated localized `None`/`Ctrl`/`Shift`/`Alt`/`Win` entries and individually measured caption widths instead of unrelated catalog text or a fixed-width slot. Its Keyboard Shortcut tab reserves four physical-key rows separately from status text and projects the 329-command catalog into grouped/filterable command and physical-key views, supports two slots, action/context/matching metadata, editable copies of the complete built-in preset, conflict repair, layout selection, and current-only `.inkshortcuts` import/export. Exact conflicts remain editable but disable Apply/OK; prefix conflicts are rejected. Runtime input resolves Global/Canvas/Timeline/Pane plus logical/physical strokes, restores temporary Hold tools on key-up/deactivation, suppresses Toggle key-repeat, and installs no global hook. The former language and shortcut-reset menu commands are no longer separate product routes. |
| Subpalette | Each workspace owns the standalone read-only object introduced in ABI v20 and retained in exact-current ABI v22. Its asynchronous complete-cache, navigation, stable tile, sampling, registration, toolbar, tooltip, and cursor behavior is unchanged by the Batch-only ABI addition. |
| Tool options presentation | The left Tool rows retain the compact split layout and bounded owned popup with accessible pin/close actions, natural-height measurement, work-area clamping and overflow scrolling. Fill, selection, raster geometry, eyedropper, gradient, alpha gradient, airbrush, blur, stamp, dust and boundary-airbrush settings are embedded pages; only boundary airbrush exposes an explicit destructive Apply. Pin state is session-only. The retired TopContext ToolOptions pane is neither created nor persisted; Workspace V9 migrates V2–V8 records. |
| Color picker presentation | Ring, HSV-triangle, and alpha-track pointer drags update a pane-local preview and synchronously paint the current picker frame for every coalesced mouse sample. Drawing-color and main-line publication occurs once on button release, avoiding Core/editor-state round trips and full Color/Palette/Chart refreshes inside `WM_MOUSEMOVE`; capture cancellation restores the drag-origin color and hue. Keyboard and numeric-field changes retain their immediate commit route. |
| Windows icon presentation | The 14 permanent Tool commands map typed Windows-only `ToolIconId` values to a fixed Fluent UI System Icons subset. Raster Geometry remains a menu-owned six-command group. Layer/Plane visibility and editability cells plus its Add/Copy/Delete/Move Up/Move Down/Properties actions and the Color, Locator, Sequence, and Light Table pin/follow buttons use typed pane icon IDs; Subpalette uses open-files/open-folder/previous/next/fit/1:1 icons, a sampled-color registration tile, and a Fluent-derived eyedropper cursor, while Batch exposes no pin button. A checked-in 48-pixel A8 atlas is recolored from system text/highlight/disabled colors and DPI-scaled at draw time; theme, system-color, enable-state, and parent-DPI changes rebuild native button images. Full localized window text remains the Tooltip/MSAA/UIA name, and atlas/GDI failure returns to text presentation. Platform icon names do not cross Rust Core, C ABI, history, document, or workspace persistence boundaries. |
| Document-tab presentation | Initial and split editor groups apply a DPI-scaled 9-point Segoe UI ClearType font to each document-tab control. Each tab `HWND` recreates its owned font after a parent DPI transition and releases it during destruction. |
| Rendering and performance | Immutable snapshots carry raster/adjustment spans, shooting-frame and vanishing-point overlays, bounded viewport-clipped radial guides, snapshot-owned pools, view-local diagnostics and previews. The Windows renderer validates and draws radial overlays and handles and rebuilds them after device loss. Canonical composite schema 4 contains only raster passes and adjustment LUTs. Raster revision-max validation remains scalar-only and the audited source-call-graph lock plus payload counters remain green. |
| Product surface | Cut, structural sequence editing and endpoint Stop/Wrap selection, raster drawing/fill/effects, angled shooting-frame properties/handles/export, vanishing-point properties/handles/radial snap, selection, layer/plane, transform, Light Table, raster clipboard, common-raster import/export including all seven standard TGA 2.0 image types and old/new metadata containers, Batch, history visualization, recovery, and compaction-copy commands remain connected from the Windows UI to their owners. TGA product export retains the established top-left 32-bit uncompressed default; the typed Rust format API additionally writes every standard color-mapped/true-color/grayscale and uncompressed/RLE variant. Retired drawing-model commands and presentation are absent. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. The selected Fluent SVG subset, per-file hashes, upstream commit/release, MIT license, generated atlas hash, resource embedding, notices, and absence of generator/network use in normal builds are checked by `inkpod_windows_fluent_icons`; both packages carry the atlas inside the executable and the notice in `ThirdPartyNotices.txt`. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `WIN-001` | Native Windows shell with explicit Common Controls registration and a system-dark title-bar opt-in, Japanese/English UI with persisted System/Japanese/English selection and non-Japanese English fallback, offline Help/About/Acknowledgements with locked BLAKE3/PNG/Fluent icon dependency attribution, typed Fluent icons for all 14 Tool commands and the applicable Layer/Plane/pin states with localized text fallback, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks | Dark presentation is limited to the system title bar; physical high-contrast/200%-DPI, complete screen-reader, and Japanese IME validation/fixes remain; the embedded offline Help body is Japanese-only |
| `WORKSPACE-001` | Persistent and auxiliary panes retain DockHost and Tool Options remains an owned flyout. The right side uses dynamic stable-ID tabs with nonempty unique membership, deterministic add/remove/move/reorder, accessible descriptions and Workspace V9 persistence/migration from V2–V8. Live Splitter geometry updates do not rebuild right-side tabs or Color/Layer list contents, while structural dock changes still refresh their presentation. Color retains a 300-DIP minimum including its Dock header; its active page is placed without interim redraw and receives one bounded synchronous repaint after final geometry and z-order are established. Its owner-drawn swatch, labels, and picker reuse the current themed tab surface at their actual coordinates and repaint after tab/theme/system-color changes. Right-tab mouse/keyboard splitters stop at each adjacent descriptor minimum without continuing to change weights. The transient Job Progress pane and transient narrow-width suppression are not persisted. | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab |
| `SUBPALETTE-001` | The standalone Core/ABI object introduced in v20 is retained unchanged in exact-current ABI v22; prior all-preset evidence remains. | Complete the physical file/folder-dialog multi-image confirmation and rerun all v22 presets before `Verified` |
| `BATCH-001`–`BATCH-004` | Batch v4 Core/format/ABI/Windows implementation stores multiple Color Replace target selectors, resolves every matching layer, deduplicates planes, and commits one private canonical primitive/Undo unit atomically. Standard multi-select checkboxes, fixed-ID preservation, saved-set round-trip, and a selectable scrolling result surface with bounded per-input localized failure reasons are present. Full Rust gates, focused Windows/ABI checks, and x64 Debug English/Japanese product smokes cover the target-missing failure and recovery as well as prior Batch controls and preview routes. | Complete physical high-contrast/screen-reader checks and v22 all-preset reruns before `Verified` |
| `SCRIPT-001`, `SCRIPT-002`, `SCRIPT-005` | Exact-current registry schema/language/file v2 and catalog/owner manifest v4 with 75 public commands are aligned to native/replay/ABI v28/25/22; private Batch v4 orchestration is excluded | Product `.inkscript` acceptance and the M36 full gate remain pending |
| `SCRIPT-003` | M12 authority/plan/run/install and the M27A/M27B private CoreHost route remain; its current native output follows v28/epoch 25/ABI 22 | Product `.inkscript` file acceptance remains absent |
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
- Batch v4 folder output supports `.inkpod`, PNG, TIFF, TGA and BMP. A graph containing Masking
  rejects the four common-raster formats because they cannot retain fill-protection state.
- Alternate TGA storage controls and TGA 2.0 metadata authoring are available through the typed
  Rust format API; the Windows and Batch product surfaces intentionally continue to emit the
  existing top-left 32-bit uncompressed TGA default.
- InkScript remains reachable only by private ABI/application smoke hooks; no product command,
  `.inkscript` file filter, clipboard or pane reaches it. `.inkbatch` v4 is an independent closed
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

On 2026-08-26, the Color-page follow-up removed the remaining fixed
`COLOR_3DFACE` fills from the combined swatch and HSV/opacity picker. Both now
capture the live themed-tab pixels at their actual coordinates before composing
their color graphics; antialiased edges therefore blend into the same surface as
the two labels. The new product-smoke check failed with code `11140` before the
implementation and then passed after it while requiring exact corresponding
pixels for the swatch, both labels, and picker. The x64 Release build passed under
`/W4 /WX`, followed by `inkpod_windows_dock_host_boundaries`,
`inkpod_windows_workspace_layout`, and `inkpod_windows_owner_model`. English and
Japanese product smokes passed in 58.27 and 66.92 seconds. Computer-assisted
inspection also launched the rebuilt product, widened the right pane through its
horizontal splitter, and confirmed one continuous Color-tab background across
the swatch, labels, and picker. It then restored the prior pane width before
shutdown; the recovery candidate shown at launch was left intact.

On 2026-08-26, the Preferences page-redraw and localized-label-width correction
rebuilt the ARM64 Debug product under MSVC `/W4 /WX /permissive-`. An early
diagnostic invocation of the real Preferences command passed the dialog's full
six-page layout contract in both English and Japanese, including system-window
background brushes for every page static and measured-width checks for the seven
Shortcut label groups. The diagnostic invocation was removed before the final
build. The normal complete product smokes currently stop before the Preferences
command at pre-existing UI check `11003`; after the diagnostic moved Preferences
ahead of that check, execution continued through the dialog and stopped later at
unrelated DockHost check `11020`. Both stops remain separate worktree issues.

On 2026-08-26, the compact Preferences/font-height correction rebuilt the ARM64 Debug product
under MSVC `/W4 /WX /permissive-`. English and Japanese product smokes exercised
the real six-tab dialog and verified its 920-by-680-DIP initial bounds, exact
current-DPI 9-point Segoe UI font, measured readable control heights, page
containment, and non-overlap of the four-row keyboard/detail/status regions.
The same measured-height helper now protects Color and Layer/Plane rows shown in
the right tool tabs. The final consecutive Japanese and English runs passed in
207.96 and 204.22 seconds. All other CTest entries passed; the portable ZIP was
regenerated from the final executable and its payload smoke then passed.

On 2026-08-25, the right-side Splitter live-resize corrections rebuilt the x64
Release application, tests, static-CRT portable ZIP, and unsigned MSIX under
MSVC 19.51 `/W4 /WX /permissive-`. `inkpod_windows_dock_host_boundaries`,
`inkpod_windows_workspace_layout`, `inkpod_windows_owner_model`, and the
69.60-second English product smoke
passed. The product smoke moves the actual right-zone Splitter in both
directions and proves zero Tool-tab deletion, Color-list reset, and Layer-list
reset messages during geometry-only relayout; after both shrink and grow, the
Layer owner-draw list has no deferred paint left behind. It also adjusts the
actual right-tab stack boundary down and back up, proves that the parent update
region is fully painted after each visible step, accepts a repaint-free `NoOp`
when a minimum prevents movement, and verifies that the physical boundary returns
to its initial position. The workspace-layout test fixes Color at a 300-DIP
minimum at 96 and 192 DPI, proves that neither adjacent pane can cross its
descriptor minimum, and proves that repeated input at either limit leaves both
weights unchanged. Earlier
x64 Release GUI checks dragged the live outer Splitter horizontally and the
right-tab stack boundary vertically without stale status cells, headers, or
frames. The new minimum-stop GUI recheck was not run because two existing ARM
Debug inkpod windows owned the application single-instance route; they were left
untouched. Rust Core, C ABI, native/replay, workspace persistence, and the approved
benchmark contract are unchanged.

On 2026-08-25, the Truevision TGA 2.0 codec expansion passed `cargo fmt --check`,
warnings-denied workspace Clippy, the full workspace/all-feature test suite,
warnings-denied workspace rustdoc, and the unchanged approved Core quick benchmark. The
format matrix covers standard image types 0/1/2/3/9/10/11, true-color 16/24/32-bit,
8/16-bit indices, 15/16/24/32-bit color-map entries, grayscale 8-bit, uncompressed/RLE,
all four origins, old/new containers, Extension/Developer Areas, straight/premultiplied/
undefined alpha, color correction, postage stamps, scan-line tables, malformed mutations,
and the buildable `tga_v2` fuzz target. Focused Core and ABI contracts prove palette-RLE
and grayscale-RLE input use the existing shared import path. The reported `A0001.tga`
decoded through that format implementation as 1754x1240. Windows x64 Debug then rebuilt
the static Rust library, application, tests, portable ZIP and unsigned MSIX under the
explicit no-profile Visual Studio environment; all 42 CTests passed in 675.01 s, including
ABI smoke and English/Japanese product smokes in 295.69/310.25 s. Native v28, replay epoch
25, `.inkbatch` v3, and C ABI v21 are unchanged.

On 2026-08-25, the Batch preview-lineage follow-up rebuilt x64 and ARM64 Debug
under MSVC 19.51 `/W4 /WX`, including static-CRT/package checks. Focused command
context boundary/state/runtime tests passed 3/3 on both architectures, and the
remaining x64 non-product Windows matrix passed 40/40 in 105.68 s. English and
Japanese x64 product smokes passed in 387.44 s and 354.70 s with the preview
Canvas left active while Run All executed; the resulting new tab retained the
original source dimensions rather than processing the contact sheet. The
preview document now stores an exact pointer-free source context, Batch query
and execution share the same source resolver, and a Job can bind that validated
inactive document without focus retargeting. Native v28, replay epoch 25,
`.inkbatch` v3, and C ABI v21 are unchanged.

On 2026-08-25, the Batch contact-sheet preview and three-button product surface
passed `cargo fmt --check`, warnings-denied Clippy across the workspace/all
targets/all features, the full workspace/all-feature test suite,
warnings-denied `inkpod-core` rustdoc, and the unchanged approved quick
benchmark. x64/ARM64 Debug/Release rebuilt under MSVC 19.51 `/W4 /WX` with
static CRT, portable ZIP, and unsigned MSIX. All four 42-test matrices passed:
x64 Debug 677.61 s, x64 Release 223.30 s, ARM64 Debug 521.74 s, and ARM64
Release 144.13 s. Their English/Japanese product smokes passed in
302.45/308.15 s, 89.83/106.81 s, 258.97/219.45 s, and 59.34/66.12 s.
Focused Core/FFI contracts prove complete source copying before processing,
temporary-root cleanup, configured-output isolation, one clean/pathless staged
contact sheet, cancellation without publication, and owner-thread staged-result
transfer. Product smoke covers Preview as one localized Canvas tab, unchanged
Run All and Cancel behavior, and the absence of former Dry Run/Current Cell
product routes. Native v28, replay epoch 25, and `.inkbatch` v3 are unchanged;
the C ABI is exact-current v21.

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

The previous M27 rebaseline established the user-confirmed M27B Core-engine route. Batch v4 retains that route with the
exact-current tuple at native v28, replay epoch 25, ABI v22 and the 75-command catalog/owner manifest v4. The immutable `CommandContext`, copied authority inputs, pointer-free progress, nonblocking wait,
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
introduced through ABI v18 remain available in v22 as one of five fixed-width semantic kinds and are mapped exhaustively to
catalog IDs only at the Windows presentation boundary. User names and paths cross an explicit opaque-text
boundary and are never treated as translation input. Tool labels share IDs across owner
draw, tooltip and accessibility; both full product smoke modes retrieve each real ToolTip
control value and compare it with the same typed label used by the button, MSAA, and UIA.
Layer/Plane presentation is pre-resolved before drawing. Layer rows are 56 DIP with
48-by-36-DIP thumbnails; Plane rows are 48 DIP with 32-by-32-DIP typed badges rendered in
an 8-point font, while adjacent detail and accessibility text retains the full kind.
Owner-draw visibility/editability cells use centered 32-by-32-DIP buttons around unchanged
16-DIP icons, a 4-DIP gap, and shared draw/hit-test geometry. The common footer keeps its
target label and six 24-by-24-DIP Fluent icon buttons on one row at the standard pane width,
uses compact right-aligned wrapping only when narrower, and switches complete localized
Tooltip/MSAA/UIA action names with the active Layer/Plane target;
Color tabs are ID-generated. The static gate rejects raw, escaped, or UTF-8-byte-array
Japanese literals in all other Windows product sources, rejects direct wide-string bypasses
in product dialog/effect presentation fields and fallback resource APIs, compares the exact
Japanese/English resource-identifier sets, covers all 344 localized menu command IDs and all
352 state-owned command IDs including the eight pane-local actions, and checks generated
artifact hashes. Dedicated tests exercise both resource languages and embedded-NUL file
filters. Full English and Japanese product smoke runs cover the same UI/Core/renderer,
owner-draw, tooltip, MSAA/UIA, state, device-loss and workspace-lifecycle paths. The native
format and replay epoch are exact-current v28/25; C ABI is exact-current v22.

| Boundary | Result |
| --- | --- |
| InkScript registry | Exact-current registry schema/language/file v2 accepts production catalog/owner manifest v4 and rejects retired resources. `inkscript_registry` covers all 75 public command/owner/runtime-adapter/equivalence identities, required metadata, session-only exclusion, normalized fingerprint, generated-reference drift, duplicate/malformed/overflow rejection, version drift, public ownership, private-model isolation and Windows non-reachability. File v2/catalog v4/epoch 25/native v28/ABI v22 are aligned |
| InkScript M01 | Public `inkpod-format` source/line-map/diagnostic/lexer API passes all 12 contract tests; malformed/truncation property corpus is deterministic; the exact-current fuzz target is `inkscript_lexer_v2`; file v2/catalog v4/epoch 25/native v28/ABI v22 are current |
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
| Current Batch v4 rebaseline | Native v28/runtime epoch 25/ABI v22, InkScript schema/language/file v2 plus catalog/owner v4, and `.inkbatch` v4 are exact-current. The public InkScript registry remains 75 commands and excludes private `ApplyBatchOperations`; ABI v1–v21 and `.inkbatch` v1–v3 are rejected. This is not a native format-freeze declaration |
| Rust workspace | On 2026-08-25, `cargo fmt --check`, the full workspace/all-feature test suite, warnings-denied Clippy across all targets/features, warning-denied `inkpod-core` rustdoc, and the unchanged quick benchmark all passed for v28/Batch v4 and ABI v22. The new contract covers multiple matching same-kind layers, one Undo/Redo unit, replay parity, target bounds/duplicates, v4 round-trip and noncurrent rejection |
| Native format | V28/runtime replay epoch 25, ABI v22, Cell document archive schema 6, metadata schema 7, document digest schema 11/domain 8, EditorState schema 7/domain 2, snapshot-composite schema 4, Cut descriptor schema 2/epoch 24, and `.inkbatch` v4 are current. Exact top-level v27, `.inkbatch` v1–v3, noncurrent archive/Cut versions, retired codes, ABI v1–v21, checksum failures and corrupt inputs are rejected; Batch lowering leaves the native/replay payload unchanged |
| Windows Subpalette | Prior ABI v21 complete x64/ARM64 Debug/Release 42-test matrices, packaging and product-smoke evidence remains applicable to the unchanged Subpalette implementation. Exact-current ABI v22 focused ABI smoke passes; the complete v22 preset rerun remains pending |
| Windows Batch v4 | On 2026-08-25, x64 Debug rebuilt under MSVC 19.51 `/W4 /WX`. The Batch set-store/input-picker/color-editor and localization tests pass. The result surface now reads bounded per-item report spans, shows each input name and a localized missing-target/hidden-or-non-editable/format-mismatch reason, preserves unknown Core diagnostics as bounded technical details, limits the visible list to eight failures plus a remaining count, and uses a selectable read-only multiline Edit with vertical scrolling. English/Japanese product smokes passed in 382.01/416.34 s and cover a real missing stable target, localized detail text, control style, recovery to a valid graph, three simultaneous semantic target checkboxes, `.inkbatch` v4 preservation, Preview/Run All/Cancel and document lifecycle routes. Other v22 presets remain pending |
| Windows Color Pin resize | On 2026-08-26, x64 Debug rebuilt under MSVC `/W4 /WX` with the bounded old/new target-row erase and its sub-gap Pin-move smoke contract. The probe runs after the full workflow with a real DockHost layout large enough to show Color, preserving the specified transient narrow-window suppression instead of forcing an initially hidden pane visible. `inkpod_windows_dock_host_boundaries`, `inkpod_windows_owner_model`, and the complete English/Japanese product smokes pass; the localized smokes reached the focused Pin contract in 325.22/340.36 seconds. The prior visible 96-DPI run also covered large width changes followed by a 3-pixel round trip and showed no stale Pin frame. |
| Windows Layer/Plane compact pane | On 2026-08-26, the final x64 Debug executable and pinned 32-icon Fluent atlas rebuilt under MSVC `/W4 /WX`. The complete 41-test non-product-smoke matrix passes, including Fluent-icon, localization, catalog, DockHost/owner/workspace layout, ABI, static-CRT and package-payload checks. Unmodified English/Japanese product smokes pass in 325.22/340.36 seconds and cover actual 56/48-DIP list rows, one-line 24-DIP action icons at standard width, target-dependent localized Tooltip/window/MSAA/UIA names, owner draw, splitter input, Color Pin resize, and normal document/workspace routes. |
| Windows Preferences | On 2026-08-26, x64 Release rebuilt under MSVC `/W4 /WX`. The 920-by-680-DIP dialog uses current-DPI 9-point Segoe UI plus measured text-height padding on all six pages; its four keyboard rows, instruction, status, and detail controls are smoke-checked for containment and non-overlap. Every unframed page `STATIC` paints the corresponding live themed tab pixels with transparent text, while client-edge values retain the independent system-window surface. Page changes, resizes, theme changes, and system-color changes invalidate the tab and visible children. Seven localized Shortcut label groups retain measured-width checks. Complete English/Japanese product smokes passed in 58.27/66.92 seconds and traverse every page while requiring exact representative tab/label background-pixel equality. The prior complete ARM64 Debug baseline remains applicable to unchanged behavior outside this x64 presentation correction. Color and Layer/Plane pane rows retain the same measured-height rule. |
| Windows x64 | Exact-current v28/ABI v22 x64 Debug executable, four-file portable ZIP, and unsigned MSIX rebuilt under `/W4 /WX`. The complete 41-test non-product-smoke matrix, static CRT and both final localized product smokes pass. Release build/tests remain to be rerun for v22; prior v21 evidence is historical |
| Windows ARM64 | Exact-current v28/ABI v22 ARM64 Debug retains the prior complete 43-test baseline. The current Preferences redraw/label-width binary rebuilds cleanly and passes its real-dialog English/Japanese contract through an early diagnostic invocation. Normal complete smokes stop before Preferences at unrelated check `11003`; the diagnostic ordering continued through Preferences and stopped later at unrelated DockHost check `11020`. ARM64 Release remains at the prior v21 evidence and still requires a v22 rerun |
| Performance | The unchanged approved quick profile passed with `batch_preview=9ae6835726a36053`, `canonical_replay=70d3465b6732887e`, `checkpoint_open=a90e56558c9eaaab`, and `output_color_guard=f169350a6a43e727`; the other scenario checksums and all semantic counters also passed. No workload, harness, payload-access route, revision-max expression or approved envelope changed. The InkScript full contract remains deferred to M36 |
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
