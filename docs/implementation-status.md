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
| Tab close icons | Document tabs, right-side group tabs and pane tabs including Sequence share `PaintTabCloseButton`. The borderless two-line glyph, DPI scaling, system colors and active/hover/pressed/disabled/focus presentation have one owner. Existing Common Controls buttons, accessible names, hit bounds, lifetime and stable-target close routes are unchanged. |
| Rust Core | All production document mutations enter one typed canonical primitive and use the same executor for live commit, Undo/Redo and replay. There is one layer model: exactly one MainLine and Color plane plus zero or more Raster planes. Current selection, ordered stable-ID saved masks and fill protection are document-owned outside that tree. Layer kinds, Selection planes, vanishing-point objects and adjustment layers are absent. Batch v5 targets Color/Raster roles and lowers one private `ApplyBatchOperations/canonical-v3` transaction. |
| Persistence | Native `.inkpod` is exact-current v32/replay epoch 27 with DocumentArchive schema 7, DOCM schema 8, document digest schema 12/domain 10 and snapshot-composite schema 5. Fully opaque common-raster imports retain exact editable RGBA MainLine pixels over a SolidWhite Genesis underlay; imports containing any non-opaque alpha retain a Transparent underlay. The Genesis asset preserves canonical decoded dimensions, RGBA8/16 depth, straight alpha and pixels, not the source container byte layout or optional metadata. DOCM is mandatory; no legacy two-plane synthesis or migration reader remains. Cut payload schema 3/replay epoch 25 is separate. `.inkbatch` is exact-current v5/operation v4. Pair paths and `Planned`/`Committed`/`None` are runtime-only, so native/replay versions are unchanged. Normal pair save/recovery and current-only rejection retain their existing atomicity rules. |
| Shared file I/O (`IO-003`) | One application-owned `inkpod-io` manager owns editable native/raster, sequence, reference, Light Table and Batch filesystem work. Windows submits typed paths through ABI v32; Rust owns bounded enumeration, identity, codecs, temporary files and cleanup. Raster-pair candidate proof collects native and raster candidates in one directory pass, while up to 32 notification-backed complete inventories make unchanged-folder revisits enumeration-free. Every attached Sequence retains all source tiles/thumbnails and up to 64 complete inactive editable targets in a COW resident bank while discarding the same pixels' dense decode-cache ownership. Exact-provenance sidecar-less targets reuse the resident tile map and precomputed asset ID with zero read/decode/hash/retile work. Replayed and companion-validated clean sidecar targets use a configurable application-wide 0–1024 MiB LRU (1 GiB default), fixed at 64 targets maximum; exact hits COW-share Core/assets/tiles. The live cell keeps prepared pair authority without duplicating its editable Core. Selected members still require complete stamps and final namespace/TOCTOU validation. |
| Raster pair resolution and save authority | File Open, Sequence and Revert use one same-stem resolver. A valid existing `.inkpod` wins only after staged replay and exact canonical decoded-raster comparison; an absent sidecar yields a disk-clean `Planned` pair; corrupt, non-current or mismatched sidecars fail closed. An eligible sidecar-less Sequence switch shares only the catalog's canonical tile backing and performs no dense payload copy, pixel re-hash or full tile reconstruction. A validated sidecar revisit clones the cached clean target's COW graph without native read/replay/raster decode/full comparison. These private construction decisions do not affect authority or pristine target re-registration. Authority remains exactly `None`/`Planned`/`Committed`; exact-path/UUID Revert retains runtime sequence, inactive recovery and all live view IDs/logical states. |
| C ABI | Exact-current ABI v32 retains the v30 Revert and v31 validated-target cache contracts, and adds fixed-size sequence resident authority, resident availability/switch, full-catalog render-preparation and immutable prepared-source snapshot APIs. The cache setter accepts 0–1 GiB and invalid input is atomic. Rust/C11/C++ numeric/layout checks reject ABI v31 and older callers. Existing manager/job, snapshot, editor, persistence, Batch v5 and InkScript handles keep their ownership/thread contracts. |
| Windows application data | Ordinary settings have one source of truth at `%LOCALAPPDATA%\inkpod\Settings\inkpod-settings.json`. Exact-current v5 includes bounded `animation.validatedSidecarCacheMiB` (0–1024, default 1024, 0 disables) beside Sequence thumbnail width and the existing settings. Apply/OK persists the candidate and changes the live application-wide limit; downstream failure restores both. `.inkshortcuts` remains v3; current-only old-version deletion and invalid/future-file preservation are unchanged. |
| InkScript | Exact-current registry schema/language/file v2, catalog/owner manifest v5, replay epoch 27, native v32 and ABI v32 are aligned. The public catalog has 73 commands; removed document features are tombstones and private Batch v5 orchestration remains excluded. No `.inkscript` product route was added. |
| Windows frontend | Layer/Plane presents one standard layer with MainLine, Color and optional Raster rows; add-plane creates Raster only and conversion changes storage format without changing role. Selection Mask commands manage a document-owned named-mask list. File Open adds a tab; Sequence resident hits exchange complete COW editing states without a file job or loading status and start dirty autosave separately, while misses retain the common resolver fallback. Revert reconciles pair identity/shell/binding. Batch targets Color/Raster roles through `.inkbatch` v5/ABI v32. |
| Right-pane tab presentation | Right-zone top-level tabs use their stable layout IDs for thresholded same-strip drag reordering and for bounded owner-drawn close buttons. Closing atomically hides all member panes, removes the addressed tab, and applies the model's deterministic replacement selection. Labels, tooltips, move-menu entries, and accessible descriptions resolve pane descriptor resource IDs through the active Japanese/English catalog at the DockHost boundary. Color/Palette/Chart retain semantic IDs while their visual order can be changed only within the Color tab control; the active page and all child `HWND`s remain loaded, and the order is session-local. Splitter focus gain/loss now synchronously redraws its system-color rule, clearing focus emphasis as soon as another component receives focus. |
| Right-pane resize transaction | Right-pane child layouts use fixed-capacity `PaneDialogLayoutPlan` objects: they register final geometry first, skip controls already at their platform-normalized bounds, publish changed bounds through one `BeginDeferWindowPos`/`DeferWindowPos`/`EndDeferWindowPos` batch with `SWP_NOREDRAW | SWP_NOCOPYBITS`, verify final/rollback geometry, and request one clipped-subtree completion only on success. DockHost defers affected same-parent pane roots while batching pane/tab/splitter geometry; Structure additionally dirties old/new right-zone and tool-tab bounds. Synchronous DockHost mutations restore the DockLayout/right-tab snapshot, attempt and verify physical rollback, and reject the candidate model when outer or inner placement fails. Structure may resynchronize DockHost chrome tab items under redraw suppression, but geometry and Structure relayout do not recreate surviving pane controls or reset pane-owned tab/list contents, selection, top index or a still-valid scroll position; resize/reflow may clamp an offset only when it falls outside the new range. Relayout does not independently reset focus; a successful hidden-pane toggle selects its destination tab and moves focus to the pane's natural first target. Floating/expanded AutoHide/reparenting complete under their destination parent. |
| Canvas viewport scrollbars | Every Main Canvas and workspace Subpalette Canvas now owns permanent native horizontal/vertical bars. Checked accepted-transform projection uses `q = -pan`, half-viewport base padding, per-view sticky extension, frozen thumb endpoints, a 32-DIP line step and a page step with one-line overlap. Bars never publish a candidate position before renderer-queue acceptance; Fit/1:1/rebind/document or source replacement carries a one-shot reset cause, while an interaction ending back inside base may discard only that axis's extension. Scroll state is nonpersistent and changes no document/editor/history/savepoint state. Subpalette keeps unmodified Arrow/Page image navigation and maps exact Shift+Arrow/Page to pan. Pure-model and CoreHost reset tests pass; focused real-`HWND` scrollbar assertions and the current local visible RendererHost runs pass. Full resize/frame, accessibility and cache-reuse verification remains. |
| Preferences and shortcuts | `編集 > 環境設定` keeps its owner-centered two-tab, scroll-free form. Animation includes localized numeric Sequence thumbnail width (32–96 DIP, default 64) and validated sidecar cache (0–1024 MiB, default 1024) fields. Apply/OK persists the v5 candidate and applies both live presentation and cache state transactionally; failure restores settings, cache limit, shortcuts and presentation, while Cancel changes nothing. Existing shortcut import/export remains v3. |
| Subpalette | Each workspace owns a standalone read-only catalog and view under ABI v32, with no hidden editable Core, Genesis or history. Its native scrollbar projection is independent from every Main Canvas, preserves unmodified Arrow/Page navigation, and carries source-scoped reset causes until an accepted publication. Catalog replacement keeps a stable auxiliary route and advances `presentation_epoch` so repeated tile IDs cannot reuse the old GPU image; visible rejection keeps the old catalog/view/snapshot, while hidden replacement clears the retained old snapshot before deferring publication. Strict active-image navigation rollback after renderer-queue rejection still needs a prepare/commit catalog ABI, so this route remains Experimental. |
| Tool options presentation | The left Tool rows retain the compact split layout and bounded owned popup with accessible pin/close actions, natural-height measurement, work-area clamping and overflow scrolling. Fill, selection, raster geometry, eyedropper, gradient, alpha gradient, airbrush, blur, stamp, dust and boundary-airbrush settings are embedded pages; only boundary airbrush exposes an explicit destructive Apply. Pin state is session-only. The retired TopContext ToolOptions pane is neither created nor persisted; Workspace state is stored as current-only readable JSON without registry or old-file migration. |
| Painting/color routing | Core-owned Pencil and Fill styles retain independent colors. After a successful Fill selects Color, an explicit MainLine choice from the menu or Layer/Plane palette returns to Pencil and restores its color; background tree refresh does not cancel an intentional Fill. Exact-color Pencil auto-erase remains the Core primitive for Binary, Grayscale and straight-RGBA MainLine pixels. |
| Color picker presentation | Ring, HSV-triangle, and alpha-track pointer drags update a pane-local preview and synchronously paint the current picker frame for every coalesced mouse sample. Drawing-color and main-line publication occurs once on button release, avoiding Core/editor-state round trips and full Color/Palette/Chart refreshes inside `WM_MOUSEMOVE`; capture cancellation restores the drag-origin color and hue. Keyboard and numeric-field changes retain their immediate commit route. |
| Windows icon presentation | The 14 permanent Tool commands map typed Windows-only `ToolIconId` values to a fixed Fluent UI System Icons subset. Raster Geometry remains a menu-owned six-command group. Layer/Plane visibility and editability cells plus its Add/Copy/Delete/Move Up/Move Down/Properties actions and the Color, Locator, Sequence, and Light Table pin/follow buttons use typed pane icon IDs; Subpalette uses open-files/open-folder/previous/next/fit/1:1 icons, a sampled-color registration tile, and a Fluent-derived eyedropper cursor, while Batch exposes no pin button. A checked-in 48-pixel A8 atlas is recolored from system text/highlight/disabled colors and DPI-scaled at draw time; theme, system-color, enable-state, and parent-DPI changes rebuild native button images. Full localized window text remains the Tooltip/MSAA/UIA name, and atlas/GDI failure returns to text presentation. Platform icon names do not cross Rust Core, C ABI, history, document, or workspace persistence boundaries. |
| Canvas tabs and sequence presentation | The Sequence pane remains one horizontal row with horizontal scrolling and Left/Right navigation. Cached thumbnails scale to the configured 32–96 DIP box with aspect ratio preserved; changing width retains item storage, selection, focus, top index, thumbnail generation and cache keys. A singleton Bottom Sequence measures current-DPI font/control/scrollbar/Cut metrics, rounds up to DIP with a 168-DIP floor, fixes the Bottom height and omits its splitter. Initial sequence discovery retains `連番読み込み`; a resident switch never shows `セルを読み込んでいます`, updates Canvas/selection immediately, and coalesces secondary pane/menu projection for 75 ms. Dirty recovery reports separate autosave status without resetting the periodic timer or blocking the visible switch. Document tab and sequence-switch identity/presentation fencing remains unchanged. |
| Rendering and performance | Immutable snapshots carry raster spans, the independent shooting-frame overlay, snapshot-owned pools, diagnostics, previews and up to 64 prepared sequence-source tile spans. CPU composition and GPU source caches each use a 64-source/1-GiB ceiling; renderer pre-upload prepares the catalog in background and rebuilds it after device loss. Frame-latency retry is 4 ms and transient visible occlusion receives a 250-ms retry window. Raster `revision-max`, payload gates and approved workloads/envelopes remain unchanged; the audited callgraph lock covers the new metadata-only prepared-source accessors. |
| Product surface | Cut/sequence, raster drawing/fill/destructive effects, angled shooting-frame, document-owned current/saved selection masks, standard layer/plane editing, transform, Light Table, raster clipboard, supported raster I/O, Batch, history, recovery and compaction remain connected. Vanishing-point and non-destructive adjustment-layer features are removed from the product and data format. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. The selected Fluent SVG subset, per-file hashes, upstream commit/release, MIT license, generated atlas hash, resource embedding, notices, and absence of generator/network use in normal builds are checked by `inkpod_windows_fluent_icons`; both packages carry the atlas inside the executable and the notice in `ThirdPartyNotices.txt`. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `ABI-001`, `ABI-002` | ABI v32 retains the v30 Revert and v31 bounded validated-target cache, and adds fixed-size sequence resident/preparation/snapshot-source records and functions. Rust layout/unit contracts and C11/C++ layout checks cover the 72/24/64-byte additions and explicit v31 rejection. | ABI v1-v31 callers must rebuild; unrun configurations/platform/manual checks remain. |
| `IO-001` | Native v32/epoch 27, required DOCM, exact decoded-pixel Genesis assets, full journal/inactive-branch/EditorState reopen and v32-only rejection are covered together with existing native replay/save/recovery. | Compression code 0, physical/manual coverage and unrun configurations remain. Source container bytes are intentionally outside the lossless pixel contract. |
| `IO-002` | PNG/TIFF retain exact RGBA8/16 in codec and pair contracts; TGA/BMP reject unsupported 16-bit companions without quantization. Save As is pair-only and flat raster is Export. | A current full product end-to-end normal-pair Save/reopen check for 16-bit PNG and TIFF remains. |
| `IO-003`, `SESSION-001`, `SEQ-001` | All Sequence source tiles/thumbnails and up to 64 complete editable Core states remain resident while dense decoder-cache ownership is discarded. Exact sidecar-less target provenance constructs Genesis/Asset from resident tiles and a precomputed ID; active/inactive states COW-share graph/asset/tile backing. Resident hits exchange Core states without a file job, read, decode, replay or full rebuild; dirty autosave publication follows separately. The validated-sidecar LRU is configurable 0–1024 MiB with a 1-GiB default. Live pair authority is retained without duplicating the active Core, with a focused explicit-sequence regression. | Unrun configurations/platform/manual/physical checks remain. First catalog attachment to existing sidecars still replays/compares before admission. Backends without a strong namespace observer retain uncached enumeration. Two-file filesystem atomicity is not promised. |
| `PERF-001` | Core workflow workloads and `revision-max` are unchanged. CPU/GPU sequence caches prepare up to 64 sources under 1-GiB bounds, renderer frame retry is 4 ms, and current ARM64/x64 Release A/B/A plus A/B/C/B/A tests complete 128 measured switches with zero reads/decodes/uploads/timeouts and one snapshot per step. Full Release CTest suites pass on the normal desktop; detailed timing/counter logs and environment distinctions are recorded below. | The separate 1-ms UI-handler p95 goal remains unmet. Passing semantic/Present checks is not full performance-goal acceptance. Formal physical-platform baselines, first sidecar attachment and edited/recovered latency remain separate. |
| `VIEW-001` | The zoom/box/fit/1:1/pan/flip baseline now includes permanent native bars, checked accepted-only `SCROLLINFO`, per-view sticky range state, frozen thumb tracking, Shift-modified line/page input, a latest-wins accepted-projection mailbox, and targeted/session reset causes retained across queue rejection. Pure-model and CoreHost regressions pass; the actual-`HWND` test reaches projection on two document-bound Canvas instances and its no-optimistic-line assertion. | Complete page/thumb, two-view, post-Core renderer-rejection reconciliation, resize/final-frame, disabled-state, accessible range/value and cache-hit performance evidence remains. The older locked-desktop Present failure is not reproduced in the current normal-desktop runs; this does not complete the remaining UI/platform checks. |
| `WIN-001`, `PREF-001`, `SHORT-001`, `HIST-002`, `FILTER-001`, `FILTER-002`, `FILTER-PREVIEW-001` | Native Windows shell with explicit Common Controls registration, native client colors and documented DWM title-bar-only dark opt-in, Japanese/English UI with persisted System/Japanese/English selection and non-Japanese English fallback, offline Help/About/Acknowledgements with locked BLAKE3/PNG/Fluent icon dependency attribution, typed Fluent icons for all 14 Tool commands and the applicable Layer/Plane/pin states with localized text fallback, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks; 316 localized menu IDs and 324 command-state entries, exact-current v5 settings/v3 shortcut presets and statusbar job selection/cancellation. The current shortcut/menu change defines a complete editable catalog, sparse Windows/VS Code defaults, preserved custom profiles, collision-free mnemonics, and Window submenu regrouping without changing direct pane toggles. ARM64 Debug settings/localization tests and both full English/Japanese product smokes pass with the new cache field. | A 9-or-more-document dynamic History Visualization paging regression remains before `SHORT-001` returns to Verified. Client UI intentionally retains native Win32 colors and visual styles without Windows app-color following. Canvas/Light Table/Subpalette content stays unchanged; physical high-contrast/200%-DPI, complete screen-reader, and Japanese IME validation/fixes remain; the embedded offline Help body is Japanese-only |
| `WORKSPACE-001`, `VIEW-004`, `SEQ-001` | Persistent and auxiliary panes retain DockHost and Tool Options remains an owned flyout. The right side uses dynamic stable-ID tabs with nonempty unique membership, deterministic add/remove/move/reorder, accessible descriptions and current-only human-readable settings-JSON persistence without registry migration. Every right-pane layout records final child geometry in a fixed-capacity `PaneDialogLayoutPlan`, skips platform-normalized unchanged bounds, batches the changed set without redraw, verifies final/rollback geometry and publishes inner failure to the pane root. DockHost defers affected same-parent completions until all outer geometry is installed, then synchronously repaints a bounded dirty union; Structure includes old/new right-zone and tool-tab bounds. Synchronous DockHost mutation failure restores the DockLayout/right-tab snapshot, attempts and verifies physical rollback, reprojects the old chrome and returns failure without committing the candidate model. Structure may rebuild DockHost chrome tab projection, but does not reset surviving pane-owned tab/list contents, selection, top index, a still-valid scroll position or control identity; resize/reflow may clamp an offset only when it falls outside the new range. Geometry notification itself does not reset focus; the successful hidden-pane toggle applies the required destination selection and new-pane focus. Color attempts and verifies restoration of its sibling tab/page z-order before reporting a shared-completion failure. Sequence commits geometry before guarded ListBox metric/top-index updates. Right-tab splitters stop at adjacent descriptor minimums. Canvas tabs keep stable view/session identity and the final tab closes to an empty workspace. The actual-`HWND`, DockHost boundary, workspace-layout and owner-model focused tests plus the x64 Debug MSVC `/W4 /WX` build pass. | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab. Floating/expanded AutoHide/reparenting use their destination-parent flow rather than the same-parent transaction. Non-x64-Debug configurations and the unrelated sequence-performance test remain pending for this change; cold/background and whole-sequence editable-document residency are outside this layout result |
| `SUBPALETTE-001` | ABI v32 retains the read-only catalog/view and parallel manager jobs. The independent permanent bars, sticky range/reset causes, exact Shift+Arrow/Page pan, unmodified Arrow/Page navigation, stable auxiliary route and catalog `presentation_epoch` are implemented. Visible catalog replacement rejection preserves the prior publication; hidden replacement clears the old retained snapshot before deferral. | Exact post-scroll sampling, durable hidden-publication retry, strict active-image renderer-rejection rollback, page/thumb/resize/accessibility product evidence, interactive file-dialog and unrun platform checks remain; see [correction record](subpalette-first-load-diagnosis.md). |
| `BATCH-001`–`BATCH-004` | Batch v5 resolves Color/Raster role or fixed-ID selectors, deduplicates overlaps and commits one private primitive/Undo unit through ABI v32. | Physical accessibility and unrun platform/configuration checks remain. |
| `SCRIPT-001`, `SCRIPT-002`, `SCRIPT-005` | Catalog/owner v5 with 73 public commands aligns to native32/replay27/ABI32; private Batch v5 orchestration is excluded. | Product `.inkscript` acceptance and the M36 full gate remain pending. |
| `SCRIPT-003` | The private CoreHost route emits current v32/epoch27/ABI32 output through the existing staged authority model. | Product `.inkscript` file acceptance remains absent. |
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

- Native Cell/Cut `.inkpod` is current-only v32; non-v32 files, including v31, are rejected without a
  migration reader before format freeze.
- Sessions retain one single-writer `CoreHost` lane for document mutation and
  result publication; file read/decode/encode/install runs asynchronously in the
  separate bounded parallel Rust I/O service. Queue latency is observable, and
  stale/cancel/failure does not publish partial document state.
- Ordinary pair save cannot atomically replace two filesystem objects. Both
  outputs are prepared first; a mutation fence, overwrite revalidation and
  bounded backup/journal recovery protect publication and preserve unresolved evidence.
- A durable pair journal or cleanup marker intentionally keeps the affected
  destination busy until `recover_pairs` can prove completion or rollback; an
  uncertain marker is not silently deleted to make a later Save appear usable.
- Application-owned identity reservations close in-process duplicate-open/write
  races. A nonparticipating external process can still create a case/TIFF alias
  after the final bounded scan, and portable non-Windows path replacement retains
  the documented narrow ABA window; the next resolver pass reports conflict.
- Companion inventory reuse is currently enabled only on Windows, where a retained
  namespace-change handle provides a fail-closed proof. Other backends enumerate
  for every candidate proof until they gain an equally strong observer; their
  correctness is unchanged, but they do not receive this warm-path optimization.
- Codec and pair contracts cover exact 16-bit PNG/TIFF and reject TGA/BMP
  precision loss, but the current full product 16-bit PNG/TIFF normal-pair
  Save/reopen route has not yet been recorded.
- GUI icons, palette/chart, shortcut/settings, clipboard memory images, test
  fixtures and Cut Cell validation remain outside the shared image I/O migration.
- Batch v5 folder output supports `.inkpod`, PNG, TIFF, TGA and BMP. A graph containing Masking
  rejects the four common-raster formats because they cannot retain fill-protection state.
- Alternate TGA storage controls and TGA 2.0 metadata authoring are available through the typed
  Rust format API; the Windows and Batch product surfaces intentionally continue to emit the
  existing top-left 32-bit uncompressed TGA default.
- Common-raster open places exact decoded RGBA8/16 pixels in the protected main-line plane,
  creates an empty same-depth color plane above it, and retains an immutable Genesis source
  asset. Positive-alpha nonwhite source pixels are fill boundaries; opaque white remains exact
  source data but is fillable. The supplied 1754x1240 TGA passes five closed fills, edge overflow
  abort, unchanged main-line checksum, Undo/Redo and save/reopen. Synthetic PNG/TIFF/TGA/BMP
  fixtures pass the same route. `FILL-001`/`FILL-002`/`FILL-003` are Verified. See
  [the raster-open/fill diagnosis](raster-import-fill-diagnosis.md).
- InkScript remains reachable only by private ABI/application smoke hooks; no product command,
  `.inkscript` file filter, clipboard or pane reaches it. `.inkbatch` v5 is an independent closed
  Batch product contract and does not expose the private Batch procedure through InkScript.
- V32 accepts compression code 0 only; measured checkpoint behavior has not
  justified decompression complexity.
- `revision-max` intentionally accepts scalar aliasing and transparent-result
  recomposition and relies on whole-cache invalidation for metadata outside its
  formula; see the performance contract in [`../SPEC.md`](../SPEC.md#横断的な性能契約).
- Portable ZIPs do not register file associations. Administrator MSIX
  install/installed-ABI/uninstall remains optional validation.
- A future sandboxed frontend still needs platform file-authority adapters for
  the shared I/O service; Rust domain crates remain platform-independent, with
  OS file APIs confined to the private `inkpod-io` backend.

## Latest representative verification

### Current release: 0.3.1 (`PKG-001`, 2026-09-03)

GitHub prerelease `v0.3.1` points to `41c0367be2b5bad34e4371e537041a5fcdad6abd`.
The published portable ZIPs were clean-built with `windows-x64-release`
(application version `0.3.1.141`) and `windows-arm-release` (`0.3.1.142`). Both
architectures pass static-CRT/dependency, embedded-version, four-file ZIP and
unsigned-MSIX payload validation; the extracted x64 portable startup also passes.
The complete native x64 CTest suite passes 49/49 in 109.67 seconds, including
RendererHost (7.46 seconds), sequence performance (6.07 seconds), and English/
Japanese product smoke (39.86/42.24 seconds).

Before the application-version-only commit, the same production sources at
`9177334` passed `cargo fmt --check`, workspace/all-target/all-feature Clippy with
warnings denied, all 801 workspace tests (one existing Release-only test ignored),
the unchanged Core quick benchmark, and Core rustdoc with warnings denied.

The release-commit [push CI](https://github.com/shuichi/inkpod/actions/runs/33704461297)
passes Rust on Linux/macOS and the Windows x64 Release job. Windows Debug fails
before CMake in `same_size_timestamp_preserved_tga_rewrite_invalidates_cache`
(`rust/inkpod-io/tests/manager.rs:107`), both initially and on one unchanged-job
retry: the rewrite preserves identity, length, modified time and change time,
so the required unequal `FileStamp` assertion fails. The same test passes locally
and in the tag-triggered CI's I/O step, but the two failures remain unresolved;
no test or acceptance gate was relaxed for publication.

Local logs are under `build/release-validation-0.3.1/`. ARM64 native execution,
the optional administrator MSIX install/uninstall test, and the physical
accessibility/DPI/IME interaction rows were not repeated for this release.
The existing feature gaps and performance acceptance limits below remain.

### Native client colors and visible-test revalidation (`WIN-001`, `PERF-001`, 2026-09-03)

The experimental partial client-area dark mode was withdrawn on user request.
Client controls and owner-draw UI again use their original Win32 colors and
visual styles. App-color detection/subscriptions, custom palette/brush ownership,
and native-control visual-style opt-outs are removed. Only the original documented
DWM workspace-title-bar opt-in remains. Canvas, Light Table, Subpalette images,
alpha/checkers, sampling, thumbnails, Core/renderer logic and persistence are
unchanged. The two experiment-only theme tests and their theme-transition smoke
probes were removed with that feature; the original product/renderer/performance
checks remain. One new native-color source gate gives 49 CTests (48 original + 1).

The earlier failed runs are not counted as successes:

- Product smoke stopped with error 930 at `first folder on empty hidden Canvas`.
  Two Subpalette images had loaded and the Canvas was visible, but the compound
  route/retained-snapshot/first-Present gate did not succeed within five seconds.
- Sequence performance stopped with error 18007 during warm-up and subsequently
  hit the 180-second CTest timeout (ARM64 Release: 180.27 seconds). Reported
  document revision and active index matched, but the complete identity/epoch/
  presentation condition was not established. A prior unchanged ARM64 Release
  executable both reproduced error 930 and passed one performance run, so these
  results did not establish either a theme-caused or a baseline-only failure.
- The independent RendererHost test failed its first Sequence-edit readback;
  `read=0` combined HRESULT and expected-pixel checks, with 30 frame-latency
  timeouts. That target did not link the experimental theme code.

Failure-only diagnostics now distinguish route/source, identity/generation,
submitted/presented epoch and QPC, visibility/occlusion, queue/timeouts and
readback HRESULT/actual-versus-expected pixels. Existing pass criteria, deadlines,
workloads, envelopes and renderer production code are unchanged. Passing reruns
show the failures are no longer reproduced in the checked runs; they do not
prove a common root cause or a renderer bug fix.

An additional x64 Release attempt under the restricted runner failed 7/49 in
58.77 seconds. InkScript engine-route/file-authority, ABI and both product smokes
failed with I/O status 7 (product error 828); RendererHost and Sequence failed
before their first Present with `DXGI_STATUS_OCCLUDED` (`0x087a0001`, Sequence
error 18006). The same executables pass these early gates under the normal
desktop runner without a rebuild. This distinguishes that restricted-runner
failure from the earlier errors 930/18007; neither failed attempt is silently
discarded. Real-window/OS-file-authority tests require their normal desktop and
filesystem environment, not a restricted substitute.

Current no-profile validation on the local Windows ARM64 desktop:

- ARM64 Debug builds with `/W4 /WX`; native-color, RendererHost and complete
  English product smoke pass (3/3, 399.17 seconds; product smoke 390.71 seconds).
  The separate Sequence performance smoke passes in 44.14 seconds.
- ARM64 Release configure/build, static-CRT and package checks pass. The full
  CTest suite passes 49/49 in 291.71 seconds: RendererHost 6.10 seconds, Sequence
  performance 9.78 seconds, English product smoke 146.55 seconds, Japanese
  product smoke 110.99 seconds.
- x64 Release configure/build, static-CRT and package checks also pass. On the
  normal desktop, all 49 CTests pass in 197.27 seconds: RendererHost 6.35 seconds,
  Sequence performance 11.97 seconds, English product smoke 77.02 seconds and
  Japanese product smoke 80.95 seconds. Both complete product smokes reach the
  Subpalette folder-first-load, later Light Table, save/recovery and shutdown
  phases instead of stopping at the earlier failed gates.
- RendererHost and Sequence performance each pass five consecutive additional
  runs per Release architecture (20 executions, no failures), using
  `--repeat until-fail:5` with the unchanged tests. ARM64 takes 88.23 seconds
  and x64 90.33 seconds; GPU tests run serially, without concurrent builds.
- Rust format, all-target/all-feature warnings-denied Clippy, all 801 workspace
  tests (one pre-existing ignored test), all nine unchanged quick benchmark
  scenarios and warnings-denied Core rustdoc pass.

Every completed Release Sequence run retains 128 measured switches, zero
additional reads/decodes/uploads, one snapshot per step, zero frame-latency
timeouts and 50/50 burst completions. Each records 256 warm Presents, not one
Present per step. The display is 120 Hz at 192 DPI. In the full suites, ARM64
has 128 foreground measured samples and x64 has zero; the five subsequent runs
per architecture each have all 128 measured samples foreground.

The table gives the median of the five per-run p95 values, in milliseconds;
each pair is A/B/A then A/B/C/B/A. Full samples and counters remain in the logs.

| Release target | UI handler | Snapshot submission | First successful Present |
| --- | --- | --- | --- |
| ARM64 native | 2.481 / 2.034 | 1.461 / 1.345 | 7.687 / 8.133 |
| x64 under ARM64 emulation | 2.219 / 2.259 | 1.766 / 1.684 | 8.139 / 7.869 |

The separate 1-ms handler goal is still not met. Passing CTest therefore does
not make `PERF-001` Verified, and successful Present is not physical-display
arrival. These measurements are not an approved physical-x64 performance
baseline or a before/after speedup claim.
Physical high-contrast/DPI transitions, complete screen-reader/IME, physical x64
hardware, the final x64 Debug configuration, edited/recovered latency and other
unrun platform checks remain outside this result. The ARM64 Debug checks above
are focused runs, not another complete Debug CTest matrix.

Detailed local logs: `build/native-ctest-arm-release.log`,
`build/native-ctest-x64-release.log` (restricted-runner failures),
`build/native-ctest-x64-release-desktop.log` (normal desktop),
`build/native-render-repeat-arm-release.log`,
`build/native-render-repeat-x64-release.log`,
`build/native-colors-initial-tests.log`, `build/native-sequence-arm-debug-result.log`
and `build/native-rust-validation.log`.

### Complete resident Sequence switching (`IO-003`, `SEQ-001`, `PERF-001`, 2026-09-03)

Sequence attachment now retains up to 64 complete editable Core states in
addition to all source tiles and thumbnails. The active state is not duplicated
in the inactive bank. Resident switching validates the selected member's bounded
identity metadata, exchanges active/inactive COW states on the Core owner lane,
and publishes Canvas/selection immediately without a file job or
`セルを読み込んでいます`. Any identity mismatch takes the existing resolver path.
Dirty outgoing state is captured and durably autosaved by a separate job whose
pair proof is fixed at issue time; its completion does not gate the visible
switch. Non-active document completion cannot activate another workspace, and
queued pointer hover alone cannot steal the active tab.

CPU and GPU preparation cover the full catalog under independent 64-source and
1-GiB limits. Prepared snapshots exclude the current active source, so the first
edit transfers its pristine GPU backing into the incremental cache and uploads
only changed tiles instead of retaining a duplicate current bitmap. Device-loss
reconstruction, the 4-ms renderer readiness retry, 250-ms transient visible
occlusion recovery, and 75-ms secondary-pane coalescing remain bounded.

No-profile Rust format, all-target/all-feature warnings-denied Clippy, the full
workspace suite (801 passing tests and one intentional ignored Release-only
gate), warnings-denied Core rustdoc, and all nine unchanged quick benchmark
scenarios pass. The latest x64 Release build completes static-CRT verification,
portable ZIP and unsigned MSIX packaging. Its focused three-synthetic-TGA
performance scenario passes with 128 warm switches, zero reads/decodes/uploads,
one snapshot per switch and no lost accepted intents. Handler p95 is 2.01/2.48 ms
and snapshot-submission p95 is 1.49/2.09 ms for A/B/A and A/B/C/B/A. The current
x64 Debug suite passes all 48 CTests in 1179.46 seconds, including English GUI
smoke in 498.16 seconds and Japanese GUI smoke in 496.12 seconds. The subsequently
strengthened Renderer host prepares all 35 synthetic sources and passes in 48.84
seconds. Current DWM first-Present p95 remains outside the existing target
on the non-foreground measurement desktop despite zero frame-latency timeouts;
physical scanout, ARM64, a full x64 Release suite, and the user's actual TGA were
not rerun.

### Core COW Sequence construction optimization (`IO-003`, `SEQ-001`, `PERF-001`, 2026-09-02)

`TileRaster` now shares its ordered tile map on clone; the first effective write
detaches the map and only the touched tile, while no-op writes preserve all
sharing. Sequence attachment retains every source tile and thumbnail but drops
each corresponding dense decoder-cache owner immediately after successful tile
adoption. The manager's decoded charge therefore equals the resident source-tile
plus thumbnail charge instead of source tiles plus the same dense pixels.

For a sidecar-less target, exact same-manager, normalized-path, complete-stamp,
generation, format and metadata proof now constructs Genesis/Asset directly from
the catalog tiles and precomputed `AssetId`. Decoded-cache clear no longer forces
a fallback. The target performs zero physical read, decode, dense copy, pixel
hash and full tile materialization; persistence materializes canonical dense bytes
only for the duration of a save/export that actually requests them.

Existing sidecars undergo replay and canonical companion comparison on first
visit, then enter one application-wide validated-target LRU only if clean,
non-recovered and committed. It uses exact normalized pair paths plus both
complete stamps, a configurable 0–1024 MiB conservative weight (1 GiB default),
and a hard 64-target cap. Exact revisits COW-share the validated Core/assets/tiles
and perform zero native/raster read, decode, replay or full comparison. Stamp
change, missing sidecar, disable, limit shrink and LRU pressure invalidate entries;
member and final TOCTOU checks remain unchanged. ABI v31 publishes the setter and
six counters in the 120-byte cache-info record. Settings schema v5 publishes the
same 0–1024 MiB value in the localized Preferences form.

Current all-feature `inkpod-core` tests pass (187 substantive unit tests plus one intentional ignore,
7 architecture, 296 contracts, 7 public InkScript, 19 registry, and resilience/
route-inventory suites). `inkpod-ffi` passes all 70 tests, including exact 0,
1 GiB and over-limit atomicity. The changed product performance smoke additionally
rejects dense-plus-tiled initial residency before entering its existing warm
zero-read/decode/upload and one-snapshot-per-step timing gates. Strict workspace
Clippy, rustdoc with warnings denied, all-feature workspace tests and all nine
unchanged quick benchmark scenarios pass. ARM64 Debug configure/build, static-CRT,
portable ZIP/unsigned MSIX, ABI, sequence performance and both full English/Japanese
product smokes pass. The first unfiltered 48-test run passed 47 tests and exposed
only a stale settings-test expectation for newly obsolete V4; after correcting
that test-only input, the exact settings test passes independently. The sequence
performance test passed in that unfiltered run in 68.54 s. Two later verbose
standalone attempts again passed the new pre-Present residency check but could not
reach their timing phase because the desktop continuously returned
`DXGI_STATUS_OCCLUDED`, including after the test's ordinary foreground request;
this environment-sensitive Present gate is not counted as a cache-semantic failure.

### Hosted Windows CI/local visible-window split (`PERF-001`, `VIEW-003`, `WIN-001`, 2026-09-02)

`inkpod_windows_sequence_performance`, `inkpod_windows_smoke`, and
`inkpod_windows_smoke_japanese` remain registered CTest tests and remain part of
an unfiltered local `ctest --preset <windows-preset>` run. They now carry the
`local-only` label because they require real visible-window Present cadence,
foreground scheduling, and long UI message-loop scenarios that are not stable
on the hosted Windows runner. GitHub Actions excludes only that label with
`ctest --preset <windows-preset> -LE local-only`; the other Windows architecture,
CoreHost, renderer, ABI, localization, packaging, and static-CRT tests continue
to gate both Debug and Release jobs. A focused local run is available with
`ctest --preset <windows-preset> -L local-only`.

### Windows renderer/Locator CI synchronization correction (`PERF-001`, `VIEW-003`, `WIN-001`, 2026-09-02)

The continuous-snapshot renderer smoke retains its exact semantic gates: an
accepted epoch must be presented while submission is active, host work remains
bounded, and the final queue reaches idle. Its background producer now yields
briefly after every submission so a low-core-count CI worker can schedule the UI
and renderer owner threads before the producer's own deadline. The final idle
fence is evaluated even when an earlier assertion fails, preventing a
short-circuit from leaving the last accepted item in the failure diagnostic.

The magnified Locator smoke now records the generation advanced by its first
pointer sample and waits for that exact-or-newer generation to be copied from
the asynchronous UI mailbox before checking the 9-by-9 RGBA neighborhood. A
Core owner-lane idle fence alone did not guarantee that the posted UI result had
already been presented. Production coalescing, stale-generation rejection and
the five-second bounded smoke deadline are unchanged.

The x64 Release build, static-CRT check and package generation pass. The
standalone renderer host passes five consecutive runs (7.49, 7.34, 7.48, 7.09
and 7.15 seconds); the English product smoke passes two consecutive runs (70.03
and 66.55 seconds), and the previously reproducible Japanese Locator failure
passes standalone in 100.50 seconds. The complete 48-test x64 Release CTest run
passes in 230.17 seconds, including renderer host 6.87 seconds, sequence
performance 67.06 seconds, and English/Japanese product smokes 64.60/71.46
seconds. The sequence pair conflict and snapshot-sink capacity messages emitted
inside the product smoke are expected conflict/rollback fault-injection probes;
the later TGA paired-open assertion passes in the complete run.

### Windows CI sequence and right-pane smoke correction (`SEQ-001`, `PERF-001`, `WORKSPACE-001`, 2026-09-02)

Pair-backed sequence replacement may install a staged clean document whose
document revision is equal to or lower than the outgoing document. The Windows
sequence-performance smoke therefore waits for the application-owned switch
completion count and still requires the exact active cell UUID, presentation
epoch, presented document revision, route and render-source owner. The pair
restore path now re-registers the resolver-proven target as the current pristine
sequence source after invalidating the outgoing document. A public Core
regression covers both directions of a normal-pair reopen and requires each
resulting snapshot to expose the selected UUID/source-generation identity.

The right-pane product probe previously assumed that Color, Layer and Reference
could share one selected tab on every CI desktop. Their 96-DPI minimums plus two
splitters require 748 physical pixels. The reported CI geometry leaves only
about 633 pixels in the selected tab even when Bottom Sequence is already
hidden, so the production layout correctly creates another tab. The visible
Structure probe now stages Layer as the selected tab's sole
survivor before adding Reference; the corresponding minimum is 444 pixels and
fits the small CI work area. It restores the complete DockLayout/right-tab
snapshot afterward and still exercises the real Reference command, both pane
HWNDs, survivor shrink/grow, Layer list state and child repaint, focus and
rejected-transaction rollback. Production placement policy and pane minimums
are unchanged.

Workspace format, warning-denied clippy and all Rust tests pass. No-profile x64
Debug and Release builds pass with static CRT and package generation. After the
small-desktop fixture correction, the x64 Debug English/Japanese product smokes
pass in 433.69/458.39 seconds, and the x64 Release counterparts pass in
73.64/106.49 seconds; none fail at Structure add. The local sequence-performance
run at that point advanced through the former document-revision and
first-warm-pristine failures but timed out before a valid foreground timing
sample. The later current COW verification above supersedes that desktop-state
result without relaxing its semantic gates.

### Sequence companion inventory and progress optimization (`IO-003`, `SEQ-001`, `PERF-001`, `WIN-001`, 2026-09-02)

Raster-pair resolution now obtains native and raster candidate sets from one
bounded directory snapshot at each of its initial, post-recovery and final proof
points. This halves a cold resolution from six directory enumerations to three.
On Windows, the application manager retains a bounded LRU inventory with a
nonrecursive namespace-change observer. An unchanged folder therefore serves
later cell switches without another `read_dir`; add, delete, rename, observer
failure, explicit cache clear and eviction all fail closed to enumeration. The
selected raster/native stamps, recovery revalidation and final TOCTOU proof are
unchanged.

Cross-platform pair contracts classify case-only path variants by physical file
identity rather than treating every non-Windows filesystem as case-sensitive.
On a case-sensitive volume, distinct `source.PNG`/`source.png` identities still
exercise and require ambiguity rejection. On a case-insensitive volume such as
the default macOS CI filesystem, the variant is the same selected file and the
fixture does not copy that file onto itself or require its alias path to be
absent.

The status bar now reserves `連番読み込み` for initial automatic/explicit
sequence discovery. Ordinary cell replacement completed before the 50 ms poll
shows no transient loading text; slower replacement reports
`セルを読み込んでいます`, while required source autosave remains immediate.
This change does not alter the native schema, replay epoch or C ABI.

Focused verification covers one-pass paired discovery, zero-enumeration warm
revisit, add/delete/rename invalidation, explicit clear and 32-directory LRU,
plus the Windows status/polling source contract. Workspace format, clippy, all
Rust tests, rustdoc and the nine-scenario Core quick benchmark pass. The x64
Debug product build passes, as do 47 of 48 CTests including English and Japanese
product smokes (537.45/514.71 seconds), ABI, localization, static CRT and package
checks. In that run, the remaining sequence-performance test stopped at its
desktop gate before measurement: both the ordinary run and an explicit
foreground-request retry reported `foreground=0` and Canvas `visible=0`. The
later current COW verification above supplies the superseding product-path
result. ARM64 Debug builds successfully; its DockHost/status/localization/ABI/
static-CRT focused set passes 5/5.

### Raster-pair lifecycle and ABI v30 validation (historical 2026-09-02 record)

At this verification point the source and public contract identified ABI v30 as exact-current and
rejected v1-v29 callers. ABI v30 added `INKPOD_IO_REVERT_CURRENT`; it is valid only
for forced `OPEN_NATIVE`, requires the live current native path and document UUID,
and retains runtime sequence state plus all live stable view IDs/logical states.
The historical ABI v29 record remains the issue-time source-recovery preservation
flag, and explicit Rust/C11/C++ contracts reject it as a current caller.

The implementation/document audit covers the shared File Open/Sequence/Revert
pair resolver, three-state `None`/`Planned`/`Committed` authority with
repair-needed as a `Committed` substate, pair-only Save As versus flat Export,
initial `BIND` rekey/binding rebase, exact-pair duplicate reuse/conflict, both
Committed and Planned explicit Save As, post-publication pair-failure recovery,
and Revert retention/republication of the runtime sequence, active binding,
inactive recovery associations and primary/secondary views. PNG/TIFF retain the
16-bit normal-companion contract; TGA/BMP reject unsupported precision.

The final Rust workspace test passes after the last ABI-hardening change, including
184 Core unit tests plus one intentional ignore, 7 architecture tests, the real
SavePair/reopen/Revert/Redo contract and native-first/raster-failure retry. Workspace
format, warning-denied clippy, warning-denied `inkpod-core` rustdoc and all nine
quick benchmark scenarios pass; the changed FFI crate was rechecked separately
after adding the ABI drift assertions and passes all 69 tests.

The x64 Debug `/W4 /WX` build and static-CRT/package checks pass. Six focused
session/authority/CoreHost routes and the ABI smoke pass, and final English and
Japanese product smokes pass in 483.18 s and 504.71 s. The portable ZIP passes
after package regeneration. In that run, sequence performance could not enter
its presentation gate because Windows reported `foreground=0` and the Canvas
remained `visible=0`; it was recorded under `PERF-001`, not as a raster-pair
protocol failure. The later current COW verification above supersedes that
desktop-state result.

x64 Release builds the Rust static library, product/tests, static-CRT check,
portable ZIP and unsigned MSIX successfully. Its save authority, recovery,
owner-model, ABI, InkScript file-authority and package-payload tests pass. A
transient CoreHost suite failure passes on immediate isolated rerun; renderer-host
continuous presentation remains unverified in the current non-foreground desktop
(`foreground=0`, frame-latency timeouts) after two isolated retries. ARM64 Debug
and Release cross-build all product/test/package targets with static CRT, portable
ZIP and unsigned MSIX; ARM64 execution is unavailable on this x64 host.

The full 16-bit PNG/TIFF product pair route, unrun configurations/platforms and
physical/manual checks remain. Accordingly `ABI-001`/`ABI-002`,
`IO-001`/`IO-002`/`IO-003`, `SESSION-001` and `SEQ-001` keep their calibrated
non-Verified status; the previously pending current Revert evidence is complete,
so `HIST-001` returns to `Verified`.

### Legacy application-settings cleanup (`PREF-001`, `WIN-001`, 2026-09-01)

The Windows settings loader now deletes an unambiguously identified older
`inkpod-settings.json` before defaults are published. It reopens the same path
with read/delete access, compares the complete byte sequence with the initially
classified input, and marks that same handle for deletion. A changed file is
reclassified once; open, read, verification, or deletion failure returns an I/O
error. Malformed current JSON, foreign format markers, duplicate markers, zero or
future versions are retained and continue through the invalid-file path.

The incremental x64 Debug MSVC `/W4 /WX` build, static-CRT verification, portable
ZIP and unsigned MSIX generation passed. The focused
`inkpod_windows_application_settings` test passed and covers deletion of V1–V3,
default restoration, retained non-old inputs, and a sharing-lock deletion failure.
The real user settings path was not modified by this test.

### Sequence thumbnail sizing and singleton Bottom dock (`SEQ-001`, `PREF-001`, `WORKSPACE-001`, 2026-09-01)

The Windows Sequence pane now reads an application-wide thumbnail-width setting
from exact-current application-settings V5. Preferences accepts 32 through 96
DIP, defaults to 64 DIP, persists before publication, and restores both the
setting and visible pane widths if publication or relayout fails. Owner-draw
thumbnail placement scales the long edge to that width while preserving the
source aspect ratio. Width-only relayout keeps the existing list items,
selection, top index, focus, labels, thumbnail generation, and decoded cache.

When Sequence is the only pane assigned to Bottom, the runtime measures the pane
content at the current DPI and available width, adds the dock-tab height, rounds
up to DIP, fixes Bottom to that extent, and omits the zone-extent splitter. The
measurement includes the thumbnail row, measured text, list borders and native
horizontal scrollbar, margins and gaps, the header/import row, and editable Cut
button rows when present. Adding another Bottom pane or moving Sequence to a
different zone restores the persisted, user-resizable Bottom extent and normal
splitter.

The x64 Debug clean-first build completed with `/W4 /WX`, static-CRT verification,
portable ZIP, and unsigned MSIX generation. The complete 48-test Windows CTest
suite passed before the final layout-failure rollback hardening (English product
smoke 421.87 s, Japanese product smoke 404.74 s, total 977.21 s). A clean-first
rebuild followed by final-source `inkpod_windows_application_settings`,
`inkpod_windows_workspace_layout`, and English `inkpod_windows_smoke` also passed
(smoke 384.61 s). Product-smoke coverage exercises 32/64/96 DIP column geometry,
landscape/portrait/square aspect ratios, list-state and cache retention, and the
absence of the singleton Bottom splitter through real `HWND` paths. No Rust,
Core, file format, or C ABI source changed in this slice; ARM64 and physical
accessibility/high-contrast validation were not rerun.

### Main/Subpalette native scrollbar slice (`VIEW-001`, `SUBPALETTE-001`, `WORKSPACE-001`, `WIN-001`, `PERF-001`, 2026-09-01)

Every visible editor-group Canvas and the workspace Subpalette Canvas now keep
native non-client horizontal and vertical bars present, using the standard
disabled state when no accepted movement is available. Scroll state is derived
only from accepted transforms: `q = -pan`, image bounds receive half a viewport
of base padding, and each stable document or auxiliary view owns its sticky
dynamic range. Crossing an endpoint extends that side past `q` by one viewport;
thumb tracking freezes its starting endpoints, active input/resize does not
shrink them, and scroll/pan completion may return an axis to base only when its
accepted `q` is back inside base. Fit, 1:1, explicit reset, Canvas bind/rebind,
document/source replacement and Subpalette image changes reset only the addressed
view after an accepted publication.

The Canvas calculates both checked `SCROLLINFO` candidates before applying them,
uses `SIF_TRACKPOS`, verifies native readback and restores the old pair on a
partial mismatch. Its bounded gesture record carries a relative `PAN_BY` delta
to the existing Main/Subpalette adapter. The adapter may synchronously execute
the owner-lane Core operation and snapshot build, but does not wait for Present;
the bar remains unchanged until the renderer accepts an envelope and the
latest-wins projection mailbox returns that transform. A second relative command
is blocked meanwhile. Line step is 32 DIP and page step is the accepted viewport
minus one line, at least one device pixel. Layout keeps both styles permanent and
the bounded completion repaint includes `RDW_FRAME`. Subpalette retains
unmodified Arrow/Page source navigation and exact Shift-modified pan.

`inkpod_windows_canvas_scroll_model` and `inkpod_windows_core_host` pass. Their
coverage includes overflow/rounding, base/sticky/frozen/reset projection,
line/page/thumb target resolution, targeted and session reset, queue-rejection
retention and one-shot consumption. `inkpod_windows_renderer_host` reaches and
passes style/exact-range assertions on two document-bound Canvas instances and
one no-optimistic-line assertion. Its queue-paused actual-`HWND` path also sends
`SB_ENDSCROLL` before pumping the final accepted projection and verifies that the
base-return shrink is folded into that projection's single native commit, but the
current post-resume desktop returns permanent `DXGI_STATUS_OCCLUDED` at its
visible-Present gate even after explicit diagnostic retries; the bounded product
one-shot retry remains unchanged. Remaining evidence includes disabled-state,
page/thumb, two-view independence, Main post-Core renderer-rejection
reconciliation, both-axis resize/final update regions, exact
post-scroll Subpalette sampling, hidden deferred retry and active-image
renderer-rejection fault injection, Japanese/English DPI and MSAA/UIA checks,
physical high-contrast/screen-reader confirmation, and zero-payload cache-hit
performance. The final x64 Debug build and all 44 CTests that do not require a
visible Present pass; renderer-host and sequence-performance stop at occlusion,
and the English/Japanese product smokes were not rerun on the locked desktop.

### Right-pane two-layer resize transaction (`WORKSPACE-001`, 2026-08-31)

The previous right-pane resize routes mixed direct `SetWindowPos`, Color-only
target-row erasure, per-control repaint, and pane-specific full redraws. They now
share one inner transaction. Each pane first records every final child bound in a
fixed-capacity `PaneDialogLayoutPlan`. Commit measures all registered `HWND`s,
omits unchanged placements, applies the changed set with
`BeginDeferWindowPos`/`DeferWindowPos`/`EndDeferWindowPos` and
`SWP_NOREDRAW | SWP_NOCOPYBITS`, and re-applies the complete registered set with
no redraw if deferred placement fails. Final geometry and rollback are verified;
failure is sticky at the outer pane root. Platform-normalized ComboBox height is
used consistently for unchanged detection and final validation. Sequence commits
all geometry before guarded item-height/column/top-index mutation. A pane requests
one synchronous clipped-subtree completion only after success. Color then places
its Common Controls tab at `HWND_BOTTOM`, raises visible sibling pages in stable
keyboard order, verifies that z-order, and attempts and verifies restoration of
the old order on failure; restoration verification failure remains a transaction
failure.

DockHost is the outer transaction for affected pane roots that stay under the
same DockHost parent, including a root becoming Hidden. It begins inner sticky-
failure tracking, defers pane completion, and batches tab, pane and splitter
placement. Structure changes include the old/new right-zone and tool-tab bounds.
A synchronous DockHost mutation snapshots DockLayout, right-tab membership and
pane host flags; outer or inner failure restores the pre-mutation models,
attempts and verifies registered physical rollback, and reapplies the old chrome
projection before returning failure without committing the candidate model.
Floating/expanded AutoHide and other reparenting transitions complete under their
destination parent rather than joining this same-parent transaction. Structure
may resynchronize DockHost chrome tab items under redraw suppression, but it does
not rebuild or reset surviving pane-owned tab/list contents or issue Color/Layer
`LB_RESETCONTENT`. Selection, top index, a still-valid scroll position and `HWND`
identity remain stable; resize/reflow may clamp an offset only when it falls
outside the new range. Focus remains stable unless an explicit successful show command applies
the SPEC 88 destination selection and natural-first-target focus rule.

The dedicated actual-`HWND` helper, DockHost-boundary, workspace-layout and
owner-model tests pass. The helper coverage includes batched placement, unchanged
skip, constructor clipping, invalid/overflow atomicity, destroyed-peer rollback,
ancestor defer, tab/page sibling z-order, ComboBox height normalization, one final
completion and empty parent/child update regions. The x64 Debug Windows
application builds under MSVC `/W4 /WX`; all 44 current tests other than the two
product smokes and the unrelated sequence-performance test pass in 28.00 seconds.
Final English and Japanese product smokes pass in 110.55 and 113.32 seconds. Their
same-selected-tab Reference add/remove probe clears injected old-frame sentinels
after both size changes; checks every affected pane geometry and current
pane/control identity; leaves pane, list, owner and tool-tab update regions empty;
observes zero Color/Layer `LB_RESETCONTENT`; and preserves surviving list counts,
selections, top indexes, stable tab identity and the explicit new-pane focus rule.
Successful removal also preserves the survivor focus. The same probe rejects a
synchronous layout callback through `WM_COMMAND` and confirms Reference
placement, right-tab model, focus and failure-result rollback. The owner-model
test separately covers full DockLayout/right-tab rollback when a compound move
adds tab membership before its DockLayout primitive rejects the pane. Outer
DockHost OS-level placement/rollback failure injection, non-x64-Debug
configurations and the sequence-performance test were not run for this change.

### Startup TGA file-stamp correction (`IO-002`, `IO-003`, `VIEW-004`, 2026-08-30)

The reported error originates in the shared Rust reader before TGA decode,
Core publication or Canvas binding, so Canvas initialization does not generate
this failure. The reader rejected any complete pre/post Windows file-stamp
mismatch. A `ChangeTime`/read-only transition can describe metadata, and Windows
may finalize `LastWriteTime` only after a writer handle closes. Either can change
the stamp without changing the data stream, so the report is consistent with a
cold-file provider transition producing a false `ChangedDuringRead`. The
reporter's exact provider transition was not available to observe directly.

When physical identity and byte length match but timestamp or read-only fields
differ, the manager treats that mismatch only as a retry signal. It opens the
path again and publishes nothing unless the
second pass is byte-for-byte identical to the first and its complete stamp stays
unchanged throughout that pass. Cache hits, streaming/native reads, save,
overwrite, pair-install and recovery evidence retain complete-stamp comparison,
and external writers keep the documented permissive sharing policy. Thus a
metadata-only transition can recover without treating timestamps as proof of
byte equality or blocking an already-open cloud/provider writer.

The regressions change permissions or modification time after the first buffered
pass and verify the stable retry stamp and exact bytes. Another overwrites a TGA
with a same-length valid TGA, restores its modification time, and proves that
cached pixels/generation are invalidated; the streaming control still rejects
the same class of change.

`cargo fmt --check`, warnings-denied workspace/all-target/all-feature Clippy,
workspace/all-feature rustdoc, and all
708 Rust tests across 22 nonempty suites pass. All nine quick Core benchmark
checksum/counter gates pass unchanged. Final x64 Debug build 134 verifies static
CRT and package generation, and all 44 non-GUI-smoke CTests pass against that
build in 52.07 seconds, including ABI smoke and sequence performance. At that
checkpoint, the English product smoke passed in 127.87 seconds and reached its
synthetic empty-workspace TGA scenario. The immediately preceding run stopped at the
existing pre-scenario magnified-preview presentation check `867` despite equal
document revision/checksum; the rerun did not reproduce it. At that checkpoint,
Japanese had not been rerun after the final byte-for-byte retry refinement. The
later exact-current x64 Debug English/Japanese full smokes recorded in the
right-pane section supersede that localized checkpoint gap; the reporter's
original file/provider, Release, ARM64 and physical accessibility were not
directly rerun for this correction.

### Main-line color routing and opaque-import erasure (`PAINT-001`, `FILL-001`, `COLOR-001`, `IO-001`, 2026-08-30)

The previous Windows correction made an explicit MainLine choice leave Fill, but
did not correct the Core stroke color. Stroke begin captured the selected tool's
paint color before resolving its stable layer/plane target. An RGBA MainLine
therefore received the green coloring paint, and Pencil auto-erase compared its
first black source pixel with that same green value instead of the document's
black MainLine color.

Stroke begin now resolves the target first. MainLine Pencil/Brush strokes capture
the document-owned MainLine color, while Color/Raster strokes capture the active
tool's paint color; the resolved value remains immutable for that stroke.
Changing the MainLine drawing color is valid for RGBA planes and affects future
strokes without recoloring existing pixels. Explicit MainLine selection still
returns Fill to Pencil, while an automatic pane refresh preserves an intentional
Fill interaction.

Eraser continues to clear plane pixels to transparent. The checkerboard appeared
because common-raster imports previously always had a Transparent Genesis
underlay, so clearing an opaque-white or black source pixel exposed transparency.
Fully opaque RGBA8/16 imports now retain their exact editable MainLine pixels over
a SolidWhite underlay; a source containing even one non-opaque pixel retains a
Transparent underlay. Thus an opaque TGA erases to white paper, while a source
that genuinely contains transparency still reveals transparency. This Genesis
semantic change advances exact-current native/procedure format to v32; replay
epoch 27 and ABI v25 are unchanged, and native v31 is rejected.

The public Core regression keeps Pencil paint green while setting MainLine black,
then proves MainLine black drawing, same-point auto-erase, Color-plane green
drawing, no cross-plane change, and exact Undo/Redo. RGBA8/16 import contracts
cover opaque-white and non-opaque underlays, composition, native round-trip and
replay, while a sensitive malformed-Genesis unit test locks the alpha/underlay
validator. No-profile format, warnings-denied Clippy, all 708 Rust tests, Core
rustdoc, all nine quick workflow gates and the Release InkScript semantic gate
pass. The x64 Debug CoreHost, renderer and ABI tests pass in 0.42, 6.65 and
10.35 seconds. At that checkpoint, its English product smoke passed in 127.87 seconds with
renderer-pixel checks for white to black to white, the explicit Eraser route,
Undo/Redo, and erased-state native save/reopen. At that checkpoint, the Japanese
product smoke, Windows Release/ARM64 builds and physical accessibility checks had
not been rerun. The later exact-current x64 Debug Japanese full smoke recorded in
the right-pane section supersedes the localized checkpoint gap; Release/ARM64 and
physical accessibility remain unrerun for this correction.

### Standard layer/plane model (`DOC-002`, `DOC-003`, `IO-001`, `ABI-001`, `BATCH-004`, 2026-08-30)

The current-only model has no layer-kind enum. Every image layer owns exactly
one MainLine plane, exactly one Color plane, and zero or more Raster planes.
Selection state, named saved-selection masks, and fill protection are
document-owned outside that image tree. Vanishing-point objects and adjustment
layers are absent from the current product, ABI, replay catalog, and native
format. At that rebaseline, native v31/replay epoch 27, ABI v25,
Batch v5/operation schema 4, and
the 73-command InkScript catalog/owner manifest v5 are aligned; older exact
versions are rejected without migration.

`cargo fmt --check`, full workspace Clippy with warnings denied, and the full
Rust workspace pass with 698 tests across 22 nonempty suites plus one existing
Release-only ignored performance gate. The generated InkScript reference check,
`inkpod-core` rustdoc with warnings denied, the format fuzz workspace compile,
all nine quick Core benchmark checksum gates, and the explicit Release InkScript
quick performance contract also pass. The latter retains checksum
`ae0d04681b2f5a63` and completed in 90.7331 ms inside the approved 64–107 ms
envelope.

The latest x64 Debug build succeeds under the Visual Studio developer
environment, including static-CRT, unsigned MSIX, and portable ZIP targets. All
46 CTests pass in 313.47 seconds. The complete English and Japanese product
smokes pass in 133.63 and 130.00 seconds; the sequence-performance and ABI
smokes pass in 18.96 and 11.31 seconds. ARM64 and the physical accessibility
checks listed below were not rerun for this model change.

### Subpalette first-load correction (`SUBPALETTE-001`, `IO-003`, 2026-08-28)

The added first-file regression fails before the fix. Candidate preparation
remains staged; a matching hidden/occluded renderer surface now defers submission
while the complete Core catalog is adopted. Actual parent hiding/minimization is
synchronized before submission, and the normal viewport notification publishes
the snapshot when visible. Other submission failures still restore the old route.

No-profile x64 Release CMake configure/build and all 46 CTests pass, including
English/Japanese production smokes, renderer/ABI contracts, sequence performance
and package payload checks. The new smoke covers empty-pane file/folder loads,
hidden/minimized completion without additional Present, successful show/restore,
malformed/cancelled replacement preserving the old cache/selection, and unchanged
document metadata. `cargo fmt --check` and `git diff --check` pass.

The updated native diagnostic links the rebuilt product objects and loads the
four original RGB8 PNGs individually, together and by folder, resetting to an
empty pane each time. All six cases publish the expected catalog and reach a
successful Present. Existing-source replacement also succeeds. The run exits 0
and all four original SHA-256 hashes remain unchanged. No user images are added
to fixtures. Rust logic/ABI/formats are unchanged; Rust clippy/test/bench/doc,
other Windows configurations and interactive file-dialog selection were not
rerun for this correction. See [the record](subpalette-first-load-diagnosis.md).

### Canvas stroke smoke coordinates (`PAINT-001`, `PAINT-003`, 2026-08-28)

The follow-up CI failure `42` checks color-plane mutation with main-line
protection. The color and cancellation samples still used fixed client pixels,
unlike the preceding main-line stroke. A regression fixture now uses the real
Canvas/Core view commands to fit and shrink the paper, then place it wholly
inside a 640-by-480 device-pixel Canvas beyond x=270. It waits for the actual
active-view revision to be presented before querying the paper bounds.

With that fixture and the old input intact, x64 Release reproduced `42` in
6.63 s at 192 DPI: paper bounds were `(320,120)-(624,291)`, pointer-down returned
1, and revision stayed 2 while both plane checksums stayed unchanged. The old
color samples at x=100..270 were entirely in the margin. This failed regression
is retained in `build/windows-x64-release/ci-42-before-ctest.log`.

The fixed samples are chosen inside the presented paper bounds in device pixels,
without applying UI DPI again. Cancellation must first capture input and produce
a preview, then leave both plane checksums, revision and flags unchanged. Color
must commit one revision and change only the color plane. The exact two completed
strokes, Undo/Redo, save/reopen and the existing `11174` tab checks remain in place.
Failure diagnostics include query/input results, tool/plane, input and paper
coordinates, client dimensions, DPI, revision and both checksums. Production
input, Rust, ABI, persistence and replay semantics are unchanged.

No-profile x64 Release configure/build, static CRT and ZIP/MSIX generation pass.
The same regression fixture remains in both complete English/Japanese smokes,
which pass in 62.65/73.49 s (136.48 s together); the other 44 CTests pass in
49.81 s against the same executable. Logs are `ci-42-fixed-smokes.log` and
`ci-42-other-ctest.log` under `build/windows-x64-release/`. `cargo fmt --check`
and `git diff --check` pass.

x64 Debug configure/build, static CRT and ZIP/MSIX generation also pass. Its
first full CTest run passes 45/46 in 753.08 s: Japanese smoke passes in 417.85 s,
but English passes the changed stroke checks and fails later at multi-workspace
reopen check `816` in 196.08 s. This failure is retained in
`build/windows-x64-debug/ci-42-ctest.log`; the original composite check does not
identify whether opening or active-document validation failed. An isolated
English rerun against the unchanged binary passes in 389.71 s, retained in
`build/windows-x64-debug/ci-42-english-rerun.log`. No assertion or timeout was
relaxed, but the first `816` failure remains unexplained and is not considered
resolved by the passing rerun. The original CI runner and ARM64, Rust
clippy/test/benchmark/rustdoc have not been rerun for this smoke-only change.

### Shared tab-close painting (`WORKSPACE-001`, `VIEW-004`, `SEQ-001`, 2026-08-28)

The existing Sequence and document-tab product smokes now compare their real
`WM_DRAWITEM` output with the right-group close button at identical device-pixel
bounds. The comparison requires exact pixel equality, the expected system-color
background and a nonempty foreground glyph for normal, pressed, disabled, focused
and pressed/focused states, each with and without hover. At 192 DPI, the regression
first failed on the old Sequence caption-frame drawing, then passed with the
shared painter. The static tab-boundary contract also requires all three callers
to use that painter; existing close/drag/cancel/stale-target checks remain intact.

Both x64 Release and ARM64 Debug configure/build, static-CRT verification,
portable ZIP and unsigned MSIX generation pass. All 46 x64 Release CTests pass
in 159.27 seconds, including complete English/Japanese product smokes in
58.42/66.55 seconds. ARM64 Debug's four targeted tab/dock-boundary, owner-model
and Japanese product-smoke tests pass in 266.60 seconds; the complete Japanese
product smoke takes 266.20 seconds. The full ARM64 CTest suite was not repeated.
A normal Japanese ARM64 Debug window was also inspected at
200% display scaling: all three tab-close glyphs are borderless, their accessible
button names remain present, and clicking Sequence close hides only that pane
while leaving the clean document open. The verification window was restored to
its original size with Sequence hidden and closed without editing a document.

`cargo fmt --check` and `git diff --check` pass. Rust sources, ABI, persistence and
replay are unchanged; Rust clippy/test/benchmark/rustdoc were not rerun for this
Windows-only drawing change. Physical 100%/150% scaling, high contrast and full
screen-reader checks were not repeated.

### Right-pane smoke setup (`WORKSPACE-001`, 2026-08-28)

The Locator setup behind smoke failure `11174` no longer requires a new-tab
mutation when height-aware restoration has already created a singleton tab.
Only `MovePaneToNewTab` accepts `Ok` or `NoOp`; restoration and relayout still
must succeed, and both paths require one additional tab, Locator-only membership,
matching model/control selection and visible dock state. The existing real-tab
drag, localized label and close assertions are unchanged. Failure diagnostics
identify the stage/result, DPI, initial right-zone height and bounded tab contents.
No product placement rule, Rust primitive, file/replay version or ABI changes.

The existing workspace-layout test now exercises 16 deterministic combinations:
96/120/144/192 DPI, combined/split initial Color/Layer tabs, and exact-fit versus
one-pixel-short heights. It checks both explicit-move and already-singleton paths,
then verifies that another singleton move preserves IDs, the ID high-water mark,
selection, tab order and contents.

Both no-profile x64 Release and Debug configure/builds pass under MSVC
`/W4 /WX /permissive-`, including static CRT and regenerated ZIP/MSIX packages.
Release passes all 46 CTests in 163.25 s, with English/Japanese product smokes
in 60.53/66.21 s. Debug passes all 46 in 892.39 s, with English/Japanese product
smokes in 382.02/373.73 s. Full logs are retained as
`build/windows-x64-release/ci-11174-ctest.log` and
`build/windows-x64-debug/ci-11174-ctest.log`. `cargo fmt --check` and
`git diff --check` pass. ARM64, Rust Clippy/tests/rustdoc and the Core benchmark
were not rerun for this smoke-only change. The original CI runner's individual
failing return value was not logged, and that runner has not been rerun here;
the new diagnostics retain that distinction if another setup failure occurs.

### Sequence-switch response work (`SEQ-001`, `PERF-001`, `IO-003`, 2026-08-28)

This user-approved slice advances the exact-current C ABI to v24. Native v29,
replay epoch 25, the canonical `revision-max` expression, benchmark workloads
and approved envelopes are unchanged. Response measurements and complete-suite
validation are recorded separately below.

| Boundary | Latest completed result / remaining validation |
| --- | --- |
| Rust | No-profile `cargo fmt --check`, workspace/all-target/all-feature Clippy with `-D warnings`, and Core rustdoc with `RUSTDOCFLAGS=-D warnings` pass. `cargo test --workspace --all-features` passes 693 tests across 26 suites, with one existing ignored Release-only test. This includes 171 Core unit tests plus that ignore, 258 public Core contracts and 61 ABI tests. |
| CPU performance | All nine unchanged Core quick scenarios retain exact checksums and semantic counters. Native ARM64 pan/zoom medians are 1.076875 ms in the primary five-process batch and 1.024583 ms in its independent remeasurement. The first exceeds the existing 1.05 ms upper bound; the second does not, so the excess is not reproduced under the existing rule. Dirty-tile medians are 1.854959/1.884125 ms, within the unchanged envelope. The separate Release-only InkScript quick contract passes one warm-up and five measured processes with checksum `b65373bdba27b215`; its ARM64 timings are diagnostic, not acceptance against the Ryzen/x64 envelope. |
| Windows response | After the final correctness fixes and rebuild, one foreground warm-up and five independent measured processes pass: 640 measured switches, all in the foreground, scenario/process p95 2.625917–3.558583 ms and maximum 4.771167 ms. Median process p95 is 2.722458/2.870625 ms for AB/ABC, within the 120 Hz two-refresh target of 16.67 ms. Every process has zero warm reads, decodes, uploads and frame-wait timeouts, 128 Presents for 128 warm steps, one snapshot per step and 50/50 reversal-burst intents committed. |
| Windows final validation | Both complete configurations build, verify static CRT and produce ZIP/MSIX artifacts. x64 Release passes all 46 CTests in 157.03 s, including English/Japanese GUI smoke in 58.57/64.26 s, CoreHost 0.24 s, Renderer 4.24 s and ABI 2.28 s. ARM64 Debug passes all 46 in 570.19 s, including English/Japanese GUI smoke in 242.69/247.54 s, CoreHost 0.43 s, Renderer 4.67 s and ABI 26.39 s. No coordinate tolerance, accepted-render gate or performance envelope was relaxed. |
| Renderer repetitions | Five consecutive independent x64 Release runs pass in 4.31/4.52/4.61/4.66/4.60 s; five ARM64 Debug runs pass in 4.96/4.78/4.42/4.65/4.28 s. Each exact 248-render drain produces 248 successful Presents with zero readiness timeouts. Phase diagnostics record before/after foreground and visibility; these are boundary observations, not continuous monitoring. |

Earlier candidates are retained as investigation history, not final acceptance:
the first x64 full invocation passed 43/46 in 63.69 s (owned-mailbox source gate
and hidden-Canvas FIT fixture), and a later invocation passed 45/46 in 195.49 s
(stroke preview checked the frame count immediately after asynchronous render
acceptance). New-view route binding and stale-editor failure refresh were also
corrected. The stroke fixture now waits for the captured route's actual Present
without issuing another render; its original mutation/checksum/dirty/frame
assertions remain. The final complete runs above pass those checks.

CPU before/after, methodology and complete samples are retained in
[the sequence performance record](sequence-switch-performance.md). Its
decoded-memory RGBA8 CPU probe measures separate intervals, not an actual TGA
open or end-to-end Present latency. An earlier approximately 203 ms UI
diagnostic did not record foreground state during its samples; it does not
isolate a background-window or driver cause. The earlier complete 45-test
successes and standalone renderer failures below remain historical baselines
and do not describe acceptance of the latest implementation.

### Palette registration (`COLOR-002`, 2026-08-28)

The Windows registration adapter reads the issuing session's authoritative
palette and reuses the first exact native-depth straight-RGBA match across all
groups. Current-color and Subpalette registration share this path; duplicate
registration updates the selected group/row without a document edit. New colors
continue through the existing Rust replacement primitive. A full palette still
accepts an existing color, while a new color fails without changing the palette
or selection. Existing and file-loaded duplicates are not automatically removed.
Native v29, replay 25 and ABI v23 are unchanged; this slice changes no Rust
primitive, serialized schema or ownership/thread contract.

Validation used no-profile PowerShell and the x64 Release preset. Configure,
the complete MSVC build, static-CRT validation and regenerated ZIP/MSIX packages
pass. `cargo fmt --check` and `git diff --check` pass.

The 45-test CTest run passed 42 tests before the final smoke-fixture corrections.
The fixture now sets drawing color through the real editor update route and
checks Undo/Redo palette contents rather than assuming a `WM_COMMAND` return of
one. Final focused CoreHost/English/Japanese product smokes pass 3/3 in 132.47 s
(0.30/64.12/67.97 s), including repeated registration through both buttons,
cross-group row selection, unchanged revision/history/dirty, alpha/depth/low-bit
distinctions, Undo/Redo and full-palette failure atomicity. Final regenerated
package/static-CRT checks pass 3/3 in 3.13 s. These are combined results across
runs, not one final 45/45 invocation.

The unchanged CoreHost test failed once without diagnostics in the complete
suite and passed on its focused rerun; the cause is unconfirmed. An earlier
portable-payload cleanup failed with an executable access-denied error, then
passed in both later runs. No assertion, timeout or workload was relaxed.
ARM64, x64 Debug and physical UI checks were not rerun. Rust Clippy, unit tests,
benchmarks and rustdoc were not rerun because Rust code is unchanged.

### Historical baseline: canvas, sequence and job-progress checkpoints (2026-08-28)

The following record predates the sequence-response optimization and ABI v24.
Its then-current 45-test successes, subsequent failures and timing investigations
are retained as recorded; the latest completed and pending gates are above.

The current Rust gates and all 45 CTests on both ARM64 Debug and x64 Release
pass, including real-timer and native-keyboard READY/INSTALLING cancellation.
The current canvas, sequence and statusbar routes have product-smoke coverage;
earlier I/O results below remain separate baseline evidence. Subsequent standalone
renderer repeats fail on both selected configurations. Their exact accepted-work/
Present contract, ARM64 pan/zoom timing acceptance and the broader physical/
platform checks remain open under `PERF-001`, `IO-003` and `WIN-001`.

| Boundary | Completed checkpoint |
| --- | --- |
| Rust workspace | Current `cargo fmt --check`, workspace/all-target/all-feature Clippy with `-D warnings`, and `inkpod-core` rustdoc with `RUSTDOCFLAGS=-D warnings` pass. `cargo test --workspace --all-features` passes 664 tests, including one doctest, 246 public Core contracts, 60 ABI tests and 28 shared-I/O tests. The existing Release-only InkScript quick test is the sole routine-run ignore; its separate approved measurement is retained below. Raster sequence regressions cover clean load/navigation, actual-edit dirty state, Undo/Redo, save/reopen, no-op/bind, delayed discovery, discard/recovery and save-path authority separation. A source whose current document/editor revisions still equal its runtime preservation baseline may omit recovery; edit→Undo clean and branch-cut→Undo clean must publish a fresh append-only generation so Redo history is neither lost nor resurrected. Document-only/editor-only dirty, recovered and repair-needed sources also require recovery. Cancel, stale, malformed/wrong-identity recovery and exact restored editor revision/digest/dirty are covered. The Windows command route inventory matches the retired progress command removal. |
| Visible Windows UI | A normal Japanese ARM64 Debug window preserves untitled/PNG tab names and order across selection, shows one horizontal sequence row with accessible frame names, scrolls horizontally and navigates with Left/Right, and closes/reopens its Sequence pane without closing the document. Closing the final tab leaves the same empty workspace; Open then loads a TGA without a dummy document. A real TGA stroke becomes dirty and Undo restores clean state. All 32 generated source-image hashes and the existing settings-file hash are unchanged, and navigation created no native companions. Physical full screen-reader/high-contrast/200%-DPI/IME coverage is not claimed. |
| Windows ARM64 Debug | Final configure and complete build, static-CRT verification and portable-ZIP/unsigned-MSIX packaging pass. All 45 CTests pass in 856.56 s, including complete English/Japanese product smokes in 441.50/348.95 s, renderer (2.63 s), CoreHost (0.53 s), ABI (41.64 s), and the new job-progress control/model test (0.26 s). The product smokes traverse the same final-tab/reopen, frame clean/dirty/recovery, horizontal Sequence, pinned/following-caption, cached progress/native cancellation, and split-failure restoration contracts as x64. A subsequent standalone renderer repeat fails as recorded below; five successful repetitions are not claimed. |
| Windows x64 Release | Final configure and complete build, static-CRT verification and portable-ZIP/unsigned-MSIX packaging pass. All 45 CTests pass in 245.54 s, including complete English/Japanese product smokes in 99.35/105.21 s, renderer (3.10 s), CoreHost (0.97 s), ABI (4.10 s), and the new job-progress control/model test (0.34 s). Product coverage includes stable tab names/order, an empty final workspace and reopen, clean raster navigation and exact recovery, horizontal Sequence/close behavior, pinned/following captions, native cached-timer/keyboard cancellation while Core work is held, and split-failure cleanup/restoration at the real snapshot-sink capacity limit. Package payload, portable startup/cleanup, localization and current-only settings/shortcuts also pass. |
| Current format and fuzz declarations | Native v29/replay 25/ABI v23, META section/record v2 with required field 21, settings JSON v3, `.inkshortcuts` v3 and `.inkbatch` v4 are current; settings/shortcut versions 1 and 2 are rejected without migration. Native v29/replay 25/ABI v23 did not change in this slice. The declared native fuzz targets are `native_v29`, `native_core_v29` and `cut_v29`; `inkscript_lexer_v2`, `inkscript_parser_v2` and `tga_v2` remain. Coverage-guided fuzz execution has not been run for this change. |

After the current complete suites, `inkpod_windows_renderer_host` was run with
`--repeat until-fail:5` separately on each configuration. Both stop on the first
run: ARM64 Debug fails in 24.60 s with `before=11 after=132 queued=248`; x64
Release fails in 23.42 s with `before=10 after=131 queued=248`. Each failure
reports an empty queue, `stale=0`, `rejected=1`, a visible/exposed surface and
`occluded=0`. These are failed exact accepted-work/Present checks, not five
successful repetitions. Current logs are in
`build/ui-refinement-verification/{arm,x64}-renderer-repeat-final.log`.
The failure precedes the newly added Unbind/empty-canvas assertions; queue
processing and Present counting are unchanged, and the earlier ARM64 baseline
failed the same assertion. The cause remains unconfirmed, including the newly
observed x64 repetition failure. The harness, workload, assertions and thresholds
were not relaxed.

At the earlier I/O baseline, both complete builds regenerated
their ZIP/MSIX artifacts. Static-CRT, MSIX-payload and portable-ZIP startup/
cleanup checks then passed again for each configuration: x64 3/3 in 5.24 s;
ARM64 3/3 in 4.28 s. These are reruns of existing entries, not additional tests.

The retained renderer fixture shows nonoverlapping parent windows before Canvas
creation, services owner-thread messages during synchronous renderer waits and
retries only `S_FALSE` while awaiting initial readback. RGB limits, the 256
enqueue attempts, queue saturation and exact accepted-work/Present equality
were unchanged at that I/O baseline, including production renderer code. Broadening the measured
message pump from sent messages to all non-paint categories produced successful
single runs but did not eliminate the ARM64 failure. The exact non-presented
HRESULTs have not been measured, so the remaining cause is not established.
Final renderer logs are retained in `build/io-migration-verification/`.
The preliminary x64 English/Japanese product runs failed at fixture code 1514:
they incorrectly expected initial `BIND` immediately after Light Table swap,
whose white base plus editable color plane correctly requires `REPLACE`.
The fixture now closes that session and uses the production raster-open job
before testing `BIND`; all bind/no-op/replace and save-authority assertions
remain, and no Core predicate was relaxed. Both failed and successful logs
are retained.
Windows x64 Debug and ARM64 Release, physical screen-reader/high-contrast/IME
flows, and non-Windows platform execution were not rerun for this slice.

#### Core quick timing checks at that historical baseline

The canvas/sequence/statusbar changes were measured with the unchanged quick
command on the same ARM64 environment, with no concurrent build, test or inkpod
GUI process. Each independent batch discarded one warm-up and measured five
separate processes. All 12 processes exited zero, and all nine scenarios kept
their exact checksums and semantic counters. The 90 measured scenario records
and complete logs are in `build/ui-refinement-verification/`.

| Current check | Complete measured samples (ns) | Median (ns) |
| --- | --- | ---: |
| Pan/zoom, first batch | 1,174,083; 1,379,583; 1,381,583; 2,933,375; 1,998,541 | 1,381,583 |
| Pan/zoom, independent recheck | 1,130,666; 1,103,834; 1,065,625; 1,099,458; 1,159,291 | 1,103,834 |
| Dirty tile, first batch | 2,785,125; 1,902,709; 2,016,375; 1,913,375; 2,033,125 | 2,016,375 |
| Dirty tile, independent recheck | 2,227,542; 1,850,500; 1,881,625; 1,992,833; 2,624,333 | 1,992,833 |

Both dirty-tile medians meet the existing 2,400,000 ns upper bound. Both
pan/zoom medians exceed the unchanged 1,050,000 ns upper bound, so timing
acceptance remains unresolved. The earlier isolated pre-change comparison
below also exceeded that bound; these results do not establish a new
regression caused by the current UI changes. No workload, harness, checksum,
envelope or canonical `revision-max` formula changed.

#### Retained I/O baseline timing investigation

On the Parallels ARM64 host (Windows build 26200, Rust/Cargo 1.97.1,
LLVM 22.1.6, MSVC 19.51, static CRT, Release, Parallels power scheme),
`cargo bench --package inkpod-core --bench core_workflows -- --quick`
exited successfully in every process and retained all nine checksums and
semantic counters. After one discarded warm-up, the two independent five-run
batches below exceeded the approved 1,050,000 ns pan/zoom upper bound. The
unchanged dirty-tile medians were 2,348,791 and 1,921,375 ns, within its
1,800,000–2,400,000 ns band.

| Pan/zoom comparison | Complete measured samples (ns) | Median (ns) |
| --- | --- | ---: |
| Current, first batch | 1,542,209; 1,515,458; 1,570,542; 2,026,959; 1,182,959 | 1,542,209 |
| Current, independent second batch | 1,213,000; 1,154,666; 1,161,916; 1,254,792; 1,528,959 | 1,213,000 |
| Alternating isolated HEAD `2aae884`, same sandbox | 1,242,625; 1,108,334; 1,554,500; 1,068,417; 1,096,125 | 1,108,334 |
| Alternating current, same sandbox | 1,160,917; 1,201,542; 1,138,542; 1,143,000; 1,171,209 | 1,160,917 |
| Alternating isolated HEAD, ordinary host execution | 2,407,875; 1,739,125; 2,133,917; 6,421,042; 2,573,000 | 2,407,875 |
| Alternating current, ordinary host execution | 1,985,750; 4,398,792; 1,271,708; 4,330,042; 1,367,958 | 1,985,750 |
| Final authority-audited code, five-run batch | 1,432,417; 1,151,625; 1,132,833; 1,225,500; 1,290,959 | 1,225,500 |
| Final code, independent five-run recheck | 1,466,833; 1,095,958; 1,169,917; 1,279,292; 1,144,416 | 1,169,917 |

Each alternating comparison also discarded one warm-up per variant. The
pre-change code exceeds the same timing bound, and the host comparison has
substantial variation; these measurements do not establish that this I/O
change caused the upper-bound breach. Timing acceptance is nevertheless not
claimed. No workload, timing interval, checksum, envelope or `revision-max`
formula was relaxed. Complete local stdout/stderr and all 423 scenario
observations (315 original plus 108 final-code observations) are retained under
`build/io-migration-verification/`;
the temporary baseline worktree was removed after measurement.

The final-code runs also discarded one warm-up per batch and retained every
semantic record exactly. Their dirty-tile measurements were:

| Dirty-tile batch | Complete measured samples (ns) | Median (ns) |
| --- | --- | ---: |
| Final code | 2,820,958; 2,655,958; 1,974,375; 1,921,708; 6,766,500 | 2,655,958 |
| Independent final-code recheck | 1,848,792; 1,799,250; 1,921,000; 2,294,625; 1,960,917 | 1,921,000 |

The first final dirty-tile median exceeds 2,400,000 ns; that breach does not
recur in the independent recheck. Both final pan/zoom medians still exceed
1,050,000 ns. All benchmark processes exit zero, but elapsed-time acceptance
is not claimed. The final samples and their unchanged counters are also saved
in `core-quick-final-observations.json` in the verification directory.

The separately ignored Release-only InkScript quick contract was initially invoked
explicitly without changing its harness. It failed at the input native-size
assertion (`script/performance.rs:604`): actual 6,072 bytes versus the retained
6,192-byte expectation. An isolated, unchanged HEAD `2aae884` run also failed
there, producing 6,056 bytes. The old expectation predates existing GENS/EDIT
schema changes; the current v29 META addition accounts for the measured
16-byte increase from HEAD. These initial failures remain recorded in
`build/io-migration-verification/`; they are not passed timing samples.

The user subsequently explicitly approved updating only the native-byte and
checksum expectations to v29. The only executable changes are five numeric
expectations in `script/performance.rs`: 6,072 bytes per input; 24,288 planned
input bytes; 36,432 runner-read bytes; 91,104 installed-output bytes; checksum
`b65373bdba27b215`. The intermediate discovery run passed all other assertions
and stopped at the old checksum, which is retained in
`inkscript-v29-expectation-discovery.log`. Workload, execution, timing interval,
checksum calculation, all non-byte counters and performance limits are unchanged.

The explicit Release-only command now passes in six independent processes:
one discarded warm-up (360,199,042 ns), then five measured samples
325,186,084; 331,263,375; 326,240,250; 237,307,875; 557,279,625 ns
(median 326,240,250 ns). Every run preserves the expected source, asset,
execution, failure/cancel, replay and byte counters and the v29 checksum.
The environment is Windows 11 build 26200, a 4-vCPU/8-GiB Parallels ARM64 VM,
Rust/Cargo 1.97.1, LLVM 22.1.6, static CRT, Release and the Parallels power scheme.
These timings are diagnostic only because the approved InkScript 64–107 ms
envelope applies to the Ryzen/x64 reference host. The current ARM64 Core
pan/zoom and renderer failures above remain unresolved. Full logs are retained
as `inkscript-v29-approved-{0..5}.log` (0 is warm-up), and current hard gates
and all samples are recorded in `docs/core-benchmark-baseline.md`.

For that earlier numeric-expectation-only follow-up, `cargo fmt --check` and
`cargo clippy --workspace --all-targets --all-features -- -D warnings` also
pass. Production code, native schema, ABI and Windows adapters are unchanged;
the full workspace test suite and Windows build/test matrix were not rerun.

### Earlier verification context

The records below retain the versions, dates and counts of their original runs.
They provide historical evidence and do not certify the current canvas,
sequence, statusbar or settings-v3 implementation, or replace the outstanding gates above.

On 2026-08-26, the Windows application-data consolidation rebuilt the complete
x64 Debug preset under MSVC `/W4 /WX /permissive-`. All 44 CTest entries passed:
the 42 non-product-smoke tests completed in 92.49 seconds, including the new
application-settings JSON/file round-trip, malformed/current-version rejection,
shortcut JSON, session binary, workspace, static persistence, static CRT, MSIX,
and portable-ZIP checks; the final English and Japanese product smokes completed
in 295.13 and 333.44 seconds after the smoke-only in-memory settings path was
enabled. The unsigned MSIX and portable ZIP were regenerated
and verified. Metadata captured before and after both product smokes proves they
neither created nor modified the real user's `inkpod-settings.json`.

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
adoption. A focused Windows test covers `%LOCALAPPDATA%\\inkpod\\batch-sets` set-name path validation,
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
and a Common Controls tab projection. Rust Core, C ABI, native/replay versions, document behavior, and the then-current V7 workspace record were
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
Japanese/English localization slice remains unchanged. The settings JSON `general.uiLanguage` field records System, Japanese,
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
| Windows right-pane resize transaction | On 2026-08-31, the former Color-only target-row/per-control correction was replaced by the shared fixed-capacity pane transaction and the DockHost outer deferral/bounded-union transaction. The actual-`HWND` helper, DockHost-boundary, workspace-layout and owner-model tests pass, including unchanged skip, batch placement, invalid/overflow and destroyed-peer rollback, compound model/tab rollback, tab/page z-order, ComboBox normalization, final/deferred completion and empty update regions. The x64 Debug application builds under MSVC `/W4 /WX`; all 44 current tests other than the two product smokes and unrelated sequence-performance test pass in 28.00 seconds. Final English/Japanese product smokes pass in 110.55/113.32 seconds. Their same-tab Reference add/remove probe verifies affected geometry and identity, sentinel clearance, empty pane/list/owner/tool-tab update regions, zero Color/Layer `LB_RESETCONTENT`, retained list count/selection/top index, new-pane focus and successful-remove survivor focus; rejected-`WM_COMMAND` coverage verifies Reference placement/right-tab model/focus/failure-result rollback. Outer DockHost OS-level placement/rollback fault injection, physical high-DPI repaint, non-x64-Debug configurations and sequence performance remain pending for this change. |
| Windows Layer/Plane compact pane | On 2026-08-26, the final x64 Debug executable and pinned 32-icon Fluent atlas rebuilt under MSVC `/W4 /WX`. The complete 41-test non-product-smoke matrix passes, including Fluent-icon, localization, catalog, DockHost/owner/workspace layout, ABI, static-CRT and package-payload checks. Unmodified English/Japanese product smokes pass in 325.22/340.36 seconds and cover actual 56/48-DIP list rows, one-line 24-DIP action icons at standard width, target-dependent localized Tooltip/window/MSAA/UIA names, owner draw, splitter input and normal document/workspace routes. Those runs predate and do not validate the current shared resize transaction. |
| Windows Preferences | On 2026-08-26, x64 Release rebuilt under MSVC `/W4 /WX` with the two-tab Preferences layout. The 920-by-680-DIP dialog opens General, keeps the former General, Save and Recovery, Workspace, Animation, and Color Management categories as ordered sections in one scroll-free page, and retains Keyboard Shortcuts as the dedicated second page. Its real-window smoke contract requires exactly two tabs, General as the initial selection, localized label widths, General section containment and non-overlap, four separate keyboard rows, and non-overlapping shortcut instruction/status/detail regions. Every unframed page `STATIC` continues to paint the corresponding live themed-tab pixels with transparent text, while client-edge values retain the independent system-window surface. The complete 43-test x64 Release matrix passed in 130.29 seconds; its final English/Japanese product smokes passed in 47.58/50.52 seconds and traverse both pages. ARM64 has not been rerun for the new two-tab layout. |
| Windows shortcut and menu mnemonics | On 2026-08-31, the generated Japanese/English menu catalogs match the template with collision-free sibling mnemonics, the x64 Debug application rebuilds under MSVC `/W4 /WX`, and seven focused command-route/state/settings/shortcut/localization CTests pass. The Rust public-route inventory passes. The final English/Japanese product smokes pass in 439.35/439.94 seconds and cover sparse-default resolution, unassigned-command conflict reassignment, native-menu reservation during Hold, menu shortcut suffixes, direct Keyboard Shortcuts and editor-group routes, and the retained Sequence endpoint keyboard contract. A 9-or-more-document dynamic History Visualization paging regression was not run. |
| Windows right-pane tabs | On 2026-08-26, ARM64 Debug was freshly configured and rebuilt under MSVC `/W4 /WX`; the executable, portable ZIP, unsigned MSIX, and static-CRT check completed. All 43 tests pass in 499.02 seconds. The complete English/Japanese product smokes completed in 224.75/224.21 seconds and exercise top-level stable-ID drag reorder, localized labels, the close icon and pane hiding, Color/Palette/Chart reorder-only drag with active-page/child-window preservation and outside-drop cancellation, plus splitter focus-gain/focus-loss pixel repaint with no deferred update region. The application-settings JSON and native/replay/ABI versions are unchanged. |
| Windows x64 | Exact-current v28/ABI v22 x64 Release executable, portable ZIP, and unsigned MSIX rebuilt under `/W4 /WX` for the two-tab Preferences layout. The complete 43-test matrix, static CRT, package payloads, and final English/Japanese product smokes pass. The prior x64 Debug executable, packaging, non-product-smoke matrix, and localized product-smoke evidence remains applicable outside the changed Preferences presentation. |
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
