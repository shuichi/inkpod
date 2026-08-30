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
| Persistence | Native `.inkpod` is exact-current v32/replay epoch 27 with DocumentArchive schema 7, DOCM schema 8, document digest schema 12/domain 10 and snapshot-composite schema 5. Fully opaque common-raster imports retain exact editable RGBA MainLine pixels over a SolidWhite Genesis underlay; imports containing any non-opaque alpha retain a Transparent underlay. DOCM is mandatory; no legacy two-plane synthesis or migration reader remains. Cut payload schema 3/replay epoch 25 is separate. `.inkbatch` is exact-current v5/operation v4. Normal pair save/recovery and current-only rejection retain their existing atomicity rules. |
| Shared file I/O (`IO-003`) | One application-owned `inkpod-io` manager owns editable native/raster, sequence, reference, Light Table and Batch filesystem work. Windows submits typed paths through ABI v25; Rust owns bounded enumeration, identity, codecs, temporary files and cleanup. A complete-stamp match publishes the first buffered raster pass. A same-identity/same-length mismatch in timestamp or read-only fields is only a retry signal: the manager publishes nothing unless a fresh second pass is byte-for-byte identical and its complete stamp remains stable. Cache, streaming, save and overwrite authority continue to require complete stamps; external writers retain the documented permissive sharing policy. Existing cache caps, owner-thread staged publication and cancellation/stale atomicity are unchanged. |
| C ABI | Exact-current ABI v25 publishes MainLine/Color/Raster roles, role-preserving format conversion, Raster-only plane creation and document-owned saved-selection operations. Removed layer-kind, Selection-plane, vanishing-point and adjustment-layer layouts/routes are not retained. Existing manager/job, snapshot, editor, persistence, Batch v5 and InkScript handles keep their ownership/thread contracts. ABI v1-v24 callers must rebuild. |
| Windows application data | Ordinary settings now have one source of truth at `%LOCALAPPDATA%\inkpod\Settings\inkpod-settings.json`. The fixed-name current-v3 UTF-8 JSON stores language, save/recovery policy including `saveAndRecovery.defaultRasterFormat` (`png`/`tiff`/`tga`/`bmp`), animation policy, output guard, custom shortcut profiles, open workspace layouts, and saved layouts with readable enum/command/key strings and no binary/Base64 payload. Session-only previous-document paths remain a current-version bounded binary record at `Session\inkpod-session.bin`; recovery, Batch sets, Help cache, derived cache, and logs remain below the same `%LOCALAPPDATA%\inkpod` root. Recovery artifact/metadata I/O now belongs to Rust, with checksummed current-only metadata v2; previous-document paths remain outside the image I/O migration. Settings and `.inkshortcuts` use exact-current v3 and reject older versions (v1/v2) without migration; the retired job-progress pane/command is absent from their supported names. Settings and workspace persistence no longer read or write HKCU, `%APPDATA%`, or old formats. Decode is staged and bounded; unknown/duplicate/malformed/noncurrent input uses current defaults without automatically overwriting the invalid file, and saves use same-directory flush/close/replace. `ヘルプ > 設定ファイルを開く` creates a missing file through that atomic save path, preserves an existing invalid file, and passes the exact fixed path to its Windows shell association. See [`windows-application-data.md`](windows-application-data.md). |
| InkScript | Exact-current registry schema/language/file v2, catalog/owner manifest v5, replay epoch 27, native v32 and ABI v25 are aligned. The public catalog has 73 commands; removed document features are tombstones and private Batch v5 orchestration remains excluded. No `.inkscript` product route was added. |
| Windows frontend | Layer/Plane presents one standard layer with MainLine, Color and optional Raster rows; add-plane creates Raster only and conversion changes storage format without changing role. Selection Mask commands manage a document-owned named-mask list with save, replace/add/subtract, rename, delete and reopen. Vanishing-point and adjustment-layer commands, dialogs, resources and renderer routes are removed. Batch targets Color/Raster roles through `.inkbatch` v5/ABI v25. Existing localized job/status, pane layout, DPI/theme and staged-result behavior remains. |
| Right-pane tab presentation | Right-zone top-level tabs use their stable layout IDs for thresholded same-strip drag reordering and for bounded owner-drawn close buttons. Closing atomically hides all member panes, removes the addressed tab, and applies the model's deterministic replacement selection. Labels, tooltips, move-menu entries, and accessible descriptions resolve pane descriptor resource IDs through the active Japanese/English catalog at the DockHost boundary. Color/Palette/Chart retain semantic IDs while their visual order can be changed only within the Color tab control; the active page and all child `HWND`s remain loaded, and the order is session-local. Splitter focus gain/loss now synchronously redraws its system-color rule, clearing focus emphasis as soon as another component receives focus. |
| Right-pane resize transaction | Right-pane child layouts use fixed-capacity `PaneDialogLayoutPlan` objects: they register final geometry first, skip controls already at their platform-normalized bounds, publish changed bounds through one `BeginDeferWindowPos`/`DeferWindowPos`/`EndDeferWindowPos` batch with `SWP_NOREDRAW | SWP_NOCOPYBITS`, verify final/rollback geometry, and request one clipped-subtree completion only on success. DockHost defers affected same-parent pane roots while batching pane/tab/splitter geometry; Structure additionally dirties old/new right-zone and tool-tab bounds. Synchronous DockHost mutations restore the DockLayout/right-tab snapshot, attempt and verify physical rollback, and reject the candidate model when outer or inner placement fails. Structure may resynchronize DockHost chrome tab items under redraw suppression, but geometry and Structure relayout do not recreate surviving pane controls or reset pane-owned tab/list contents, selection, top index or a still-valid scroll position; resize/reflow may clamp an offset only when it falls outside the new range. Relayout does not independently reset focus; a successful hidden-pane toggle selects its destination tab and moves focus to the pane's natural first target. Floating/expanded AutoHide/reparenting complete under their destination parent. |
| Preferences and shortcuts | `編集 > 環境設定` opens one resizable, owner-centered two-tab dialog for all persisted application/workspace preferences. Save and Recovery includes the default raster companion format for subsequently created Cells; changing it does not change an open document's recorded format. General opens by default and presents General, Save and Recovery, Workspace, Animation, and Color Management as ordered sections in one scroll-free vertical form; Keyboard Shortcuts remains a dedicated page. Its 920-by-680-DIP initial bounds replace the former oversized expansion. The dialog creates one current-DPI 9-point Segoe UI font for its resource and dynamic controls, recreates it after DPI changes, and derives readable rows from the selected font's measured text height plus DPI-scaled padding. Every unframed page `STATIC` reproduces the corresponding coordinates of the live themed tab surface with transparent text, while `WS_EX_CLIENTEDGE` value fields retain their independent system-window surface. Page switches, resizes, theme changes, and system-color changes invalidate the tab surface and visible controls so old frames cannot remain. General and Shortcut labels derive their widths from localized text measured with the selected font. The modifier radios use dedicated localized `None`/`Ctrl`/`Shift`/`Alt`/`Win` entries and individually measured caption widths instead of unrelated catalog text or a fixed-width slot. Its Keyboard Shortcut tab reserves four physical-key rows separately from status text and projects the 321-command catalog into grouped/filterable command and physical-key views, supports two slots, action/context/matching metadata, editable copies of the complete built-in preset, conflict repair, layout selection, and exact-current v3 `.inkshortcuts` import/export with rejection of older versions (v1/v2). Exact conflicts remain editable but disable Apply/OK; prefix conflicts are rejected. Runtime input resolves Global/Canvas/Timeline/Pane plus logical/physical strokes, restores temporary Hold tools on key-up/deactivation, suppresses Toggle key-repeat, and installs no global hook. The former language and shortcut-reset menu commands are no longer separate product routes. |
| Subpalette | Each workspace owns a standalone read-only catalog and view under ABI v25, with no hidden editable Core, Genesis or history. Path-only asynchronous requests, staged replacement, cache leases, exact-depth sampling and compact presentation remain unchanged. |
| Tool options presentation | The left Tool rows retain the compact split layout and bounded owned popup with accessible pin/close actions, natural-height measurement, work-area clamping and overflow scrolling. Fill, selection, raster geometry, eyedropper, gradient, alpha gradient, airbrush, blur, stamp, dust and boundary-airbrush settings are embedded pages; only boundary airbrush exposes an explicit destructive Apply. Pin state is session-only. The retired TopContext ToolOptions pane is neither created nor persisted; Workspace state is stored as current-only readable JSON without registry or old-file migration. |
| Painting/color routing | Core-owned Pencil and Fill styles retain independent colors. After a successful Fill selects Color, an explicit MainLine choice from the menu or Layer/Plane palette returns to Pencil and restores its color; background tree refresh does not cancel an intentional Fill. Exact-color Pencil auto-erase remains the Core primitive for Binary, Grayscale and straight-RGBA MainLine pixels. |
| Color picker presentation | Ring, HSV-triangle, and alpha-track pointer drags update a pane-local preview and synchronously paint the current picker frame for every coalesced mouse sample. Drawing-color and main-line publication occurs once on button release, avoiding Core/editor-state round trips and full Color/Palette/Chart refreshes inside `WM_MOUSEMOVE`; capture cancellation restores the drag-origin color and hue. Keyboard and numeric-field changes retain their immediate commit route. |
| Windows icon presentation | The 14 permanent Tool commands map typed Windows-only `ToolIconId` values to a fixed Fluent UI System Icons subset. Raster Geometry remains a menu-owned six-command group. Layer/Plane visibility and editability cells plus its Add/Copy/Delete/Move Up/Move Down/Properties actions and the Color, Locator, Sequence, and Light Table pin/follow buttons use typed pane icon IDs; Subpalette uses open-files/open-folder/previous/next/fit/1:1 icons, a sampled-color registration tile, and a Fluent-derived eyedropper cursor, while Batch exposes no pin button. A checked-in 48-pixel A8 atlas is recolored from system text/highlight/disabled colors and DPI-scaled at draw time; theme, system-color, enable-state, and parent-DPI changes rebuild native button images. Full localized window text remains the Tooltip/MSAA/UIA name, and atlas/GDI failure returns to text presentation. Platform icon names do not cross Rust Core, C ABI, history, document, or workspace persistence boundaries. |
| Canvas tabs and sequence presentation | Document captions come from each stable view's own session, so selecting untitled and named files preserves the model's tab order and names. Closing the final canvas tab leaves the workspace open with no Canvas and keeps New/Open available. The sequence uses a single horizontal thumbnail row, horizontal scrolling and Left/Right navigation; its separate Previous/Next buttons are removed. Its dock minimum/preferred heights are 168/184 DIP, and its pane tab has a close button. See [SPEC.md](../SPEC.md) sections 2 and 4 (`VIEW-004`, `SEQ-001`). Initial and split editor groups apply a DPI-scaled 9-point Segoe UI ClearType font to each document-tab control. Each tab `HWND` recreates its owned font after a parent DPI transition and releases it during destruction. Every visible document tab owns a bounded, reused Common Controls close button with a system-color DPI-scaled icon and localized accessible name. The button routes the exact stable `DocumentViewId` through the existing `IDM_VIEW_CLOSE` command, preserves a still-valid previously active view when closing an inactive tab, and remains separate from label drag hit testing. Queued clean sequence replacement retains directional intents and same-size zoom/pan/flip; unchanged-catalog selection preserves the thumbnail list, focus and scroll. Issue-time session/view/generation and presentation epochs fence commands and direct pane callbacks until the selected document has been presented. The final independent foreground response series and both complete 46-test Windows configurations pass; see the verification record below. |
| Rendering and performance | Immutable snapshots carry raster spans, the independent shooting-frame overlay, snapshot-owned pools, diagnostics and previews. Canonical composite schema 5 has no adjustment LUT or vanishing/radial record. Raster `revision-max`, payload gates, cache limits and approved workloads/envelopes remain unchanged; the audited callgraph lock is updated for the removed snapshot inputs. |
| Product surface | Cut/sequence, raster drawing/fill/destructive effects, angled shooting-frame, document-owned current/saved selection masks, standard layer/plane editing, transform, Light Table, raster clipboard, supported raster I/O, Batch, history, recovery and compaction remain connected. Vanishing-point and non-destructive adjustment-layer features are removed from the product and data format. |
| Build and distribution | CMake drives the Rust static library and MSVC C++20 build. Rust domain crates remain OS-independent; Windows x64/ARM64 use static CRT. Unsigned MSIX and four-file portable ZIP packaging paths are maintained. The selected Fluent SVG subset, per-file hashes, upstream commit/release, MIT license, generated atlas hash, resource embedding, notices, and absence of generator/network use in normal builds are checked by `inkpod_windows_fluent_icons`; both packages carry the atlas inside the executable and the notice in `ThirdPartyNotices.txt`. |
| Locator responsiveness | Selection-bound queries scan sparse allocated mask tiles and reuse a document-identity/revision cache. Windows keeps one locator query in flight plus one latest request, presents processed intermediate generations during continuous input, and rejects reset or target-stale results before requeuing the newest pointer behind accepted stroke work. |

## Active gaps

Only the following requirements are not yet `Verified` in
[`compatibility.md`](compatibility.md).

| Requirement | Available now | Remaining work |
| --- | --- | --- |
| `ABI-001`, `ABI-002` | ABI v25 closes the public model to standard image roles and document-owned saved-selection masks while retaining manager/job/snapshot ownership and owner-thread publication. | Current Rust/header/Windows validation is reported below; older ABI callers must rebuild and unrun platform/manual checks remain. |
| `IO-001`, `IO-003` | Native v32/epoch 27, required DOCM and v32-only rejection are covered together with shared Rust-manager routes, caches, pair save and recovery. | Physical file-management flows and unrun configurations remain; no cross-file atomicity promise. |
| `PERF-001` | Core workflow workloads and `revision-max` are unchanged. The current v32/catalog-v5 InkScript quick semantic gate passes with all non-byte counters unchanged, checksum `1e41e17e8bda22e3` and 88.9261 ms inside the existing 64–107 ms envelope. | A full current multi-process timing series, initial/cache-miss and background latency remain outside this run; prior approved measurements remain historical evidence. |
| `WIN-001`, `PREF-001`, `SHORT-001`, `HIST-002`, `FILTER-001`, `FILTER-002`, `FILTER-PREVIEW-001` | Native Windows shell with explicit Common Controls registration and a system-dark title-bar opt-in, Japanese/English UI with persisted System/Japanese/English selection and non-Japanese English fallback, offline Help/About/Acknowledgements with locked BLAKE3/PNG/Fluent icon dependency attribution, typed Fluent icons for all 14 Tool commands and the applicable Layer/Plane/pin states with localized text fallback, owner-centered work-area-clamped modal dialogs, DPI-aware layout, keyboard routes, MSAA/UIA names, theme and accessibility hooks; 313 localized menu IDs and 321 command-state entries, exact-current v3 settings/shortcut presets and statusbar job selection/cancellation | Dark presentation is limited to the system title bar; physical high-contrast/200%-DPI, complete screen-reader, and Japanese IME validation/fixes remain; the embedded offline Help body is Japanese-only |
| `WORKSPACE-001`, `VIEW-004`, `SEQ-001` | Persistent and auxiliary panes retain DockHost and Tool Options remains an owned flyout. The right side uses dynamic stable-ID tabs with nonempty unique membership, deterministic add/remove/move/reorder, accessible descriptions and current-only human-readable settings-JSON persistence without registry migration. Every right-pane layout records final child geometry in a fixed-capacity `PaneDialogLayoutPlan`, skips platform-normalized unchanged bounds, batches the changed set without redraw, verifies final/rollback geometry and publishes inner failure to the pane root. DockHost defers affected same-parent completions until all outer geometry is installed, then synchronously repaints a bounded dirty union; Structure includes old/new right-zone and tool-tab bounds. Synchronous DockHost mutation failure restores the DockLayout/right-tab snapshot, attempts and verifies physical rollback, reprojects the old chrome and returns failure without committing the candidate model. Structure may rebuild DockHost chrome tab projection, but does not reset surviving pane-owned tab/list contents, selection, top index, a still-valid scroll position or control identity; resize/reflow may clamp an offset only when it falls outside the new range. Geometry notification itself does not reset focus; the successful hidden-pane toggle applies the required destination selection and new-pane focus. Color attempts and verifies restoration of its sibling tab/page z-order before reporting a shared-completion failure. Sequence commits geometry before guarded ListBox metric/top-index updates. Right-tab splitters stop at adjacent descriptor minimums. Canvas tabs keep stable view/session identity and the final tab closes to an empty workspace. The actual-`HWND`, DockHost boundary, workspace-layout and owner-model focused tests plus the x64 Debug MSVC `/W4 /WX` build pass. | Reference Check AutoHide edge buttons are not reachable by F6/Tab/Shift+Tab. Floating/expanded AutoHide/reparenting use their destination-parent flow rather than the same-parent transaction. Non-x64-Debug configurations and the unrelated sequence-performance test remain pending for this change; cold/background and whole-sequence editable-document residency are outside this layout result |
| `SUBPALETTE-001` | ABI v25 retains the read-only catalog/view and parallel manager jobs; prior first-load correction evidence remains applicable. | Interactive file-dialog and unrun platform checks remain; see [correction record](subpalette-first-load-diagnosis.md). |
| `BATCH-001`–`BATCH-004` | Batch v5 resolves Color/Raster role or fixed-ID selectors, deduplicates overlaps and commits one private primitive/Undo unit through ABI v25. | Physical accessibility and unrun platform/configuration checks remain. |
| `SCRIPT-001`, `SCRIPT-002`, `SCRIPT-005` | Catalog/owner v5 with 73 public commands aligns to native32/replay27/ABI25; private Batch v5 orchestration is excluded. | Product `.inkscript` acceptance and the M36 full gate remain pending. |
| `SCRIPT-003` | The private CoreHost route emits current v32/epoch27/ABI25 output through the existing staged authority model. | Product `.inkscript` file acceptance remains absent. |
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
| Rust workspace | Current `cargo fmt --check`, workspace/all-target/all-feature Clippy with `-D warnings`, and `inkpod-core` rustdoc with `RUSTDOCFLAGS=-D warnings` pass. `cargo test --workspace --all-features` passes 664 tests, including one doctest, 246 public Core contracts, 60 ABI tests and 28 shared-I/O tests. The existing Release-only InkScript quick test is the sole routine-run ignore; its separate approved measurement is retained below. Raster sequence regressions cover clean load/navigation, actual-edit dirty state, Undo/Redo, save/reopen, no-op/bind, delayed discovery, discard/recovery and save-path authority separation. Clean-source recovery switches accept omitted source paths without creating recovery files; document-only/editor-only dirty sources still require them. Cancel, stale, malformed/wrong-identity recovery and exact restored editor revision/digest/dirty are covered. The Windows command route inventory matches the retired progress command removal. |
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
