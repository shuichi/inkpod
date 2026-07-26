# Implementation status

## Current milestone

- Milestone: M8
- Status: In progress
- Last verified worktree state: M8 retains all M0-M7 vertical slices and adds a
  checked legacy-codec scope matrix, a six-format corrupted-file corpus plus
  deterministic mutation harness, a maximum-dimension sparse/COW and dense
  filter benchmark, whole-workspace Rust portability enforcement, documented
  next-frontend API gaps, and CMake-owned x64 MSIX assembly. Review corrected
  incomplete portability scanning, a corrupt PNG fixture that rejected before
  its intended dimension check, missing app-local MSVC runtime payload, and
  install-smoke failure cleanup. The corrected artifact passes non-elevated
  payload inspection, but its elevated install/run/uninstall rerun was cancelled
  at UAC; M8 therefore remains `In progress` only on acceptance scenario 4.

## Post-M8 refactoring regression baseline

- The public ABI now has a source-level coverage gate for all 155 functions in
  `include/inkpod/core_ffi.h`. The gate requires the header declarations and
  Rust `no_mangle` exports to remain identical and requires every public
  function to have a direct reference in a Rust or C++ contract test.
- Three focused FFI scenarios cover the 46 functions that previously had no
  direct test reference: document/frame/history/selection/clipboard/common
  raster round-trip; light-table/sequence/owned-buffer lifetimes; and
  vector/filter/task state machines. They exercise success, invalid nested
  `struct_size`, size query, `BUFFER_TOO_SMALL`, double release, savepoint
  restoration, natural ordering, cancellation-ready state, and Undo/revision
  transaction boundaries.
- These outer-contract tests are the refactoring safety net for ABI-visible
  behavior. Core/image/format unit, property, malformed-input, golden, and
  Windows application tests remain necessary for algorithmic edge cases and
  failure localization; FFI coverage does not replace them.

## Windows frontend refactoring R0-R3

- R0 is complete. The baseline records 274 defined `IDM_*` values, 273
  production resource commands, and exact ownership of all 273 in the main
  window procedure. `IDM_BATCH_OPERATION_ADD` remains an intentionally unused
  range marker; the 24 concrete Batch add commands are separately inventoried
  in `docs/windows-command-inventory.md`.
- R1.1-R1.3 source work is complete. COM apartment lifetime, About, shortcut,
  view, text-input, fill, history, M6 editor/progress, and Batch
  progress/palette code now lives in focused modules under `apps/windows`.
  Modal outputs are published only after `IDOK`; modeless dialogs receive typed
  query/cancel or dispatch/select callbacks and never receive the complete
  application state, Core handle, or FFI task ownership.
- `main.cpp` is 14,645 lines and 635,662 bytes after the extraction, down from
  the recorded 16,311 lines and 701,656 bytes. It contains no dialog procedure,
  `DialogBoxParamW`, or `CreateDialogParamW`; message and command routing remain
  in place for later R4 work.
- The public ABI, file format, renderer ownership, Core engine thread, renderer
  thread, snapshot queues, menus, resources, and user-visible dialog behavior
  are unchanged. R1 is `Complete`; the elevated installed-MSIX ABI smoke was
  cancelled at UAC after all non-elevated checks passed, and the user accepted
  that exact recorded external blocker for R1 completion.
- R2.1-R2.4 are complete. The former flat `AppState` is now the private
  `AppContext` composition root with lifetime, main-window handles,
  document-shell, tool, view, pane, animation, effects, and Batch owners plus
  `CoreEngine`. Initial values and the existing Core metadata cache remain
  unchanged; no C++ document model was introduced.
- Document replacement now calls explicit owner reset functions rather than one
  feature-named reset body silently clearing unrelated state. Effects and Batch
  progress callbacks receive only their owner state, and drawing color,
  motion-label, vector-preview, adjustment lookup, Batch palette refresh, and
  Batch derived-handle helpers use narrow state arguments.
- `main.cpp` is 14,501 lines and 642,344 bytes after R2, while the 235-line
  private `app_context.h` contains the state topology. Debug and Release strict
  builds and all four non-elevated CTest scenarios passed, including the real
  application `--smoke-test`, `--abi-smoke-test`, and package payload check.
  The explicitly optional elevated MSIX install/uninstall smoke was not run for
  R2. Public ABI, file format, menus, GUI behavior, thread assignment, shutdown,
  and snapshot ownership remain unchanged.
- R3.1-R3.4 are complete. `DocumentShellController` and the clipboard adapter
  own Windows document/recovery/common-raster path flow and native clipboard
  publication/import while Rust retains serialization, codecs, and typed
  document conversion. Document and color pane controllers load batched Core
  models; color-chart name/page synchronization is owned with the color pane
  model while Win32 list presentation remains in the main window for R4.
- View/guide/grid, fill, selection, floating paste, and vector commands now pass
  typed inputs to focused controllers. Effects owns task creation, progress,
  cancellation, preview apply, and completion posting. Batch owns graph
  creation/save/load, preview/run, report/task handles, progress, and derived
  state; asynchronous work captures the long-lived Batch state rather than a
  temporary controller.
- `main.cpp` is 12,857 lines and 577,974 bytes after R3. The command IDs, menu
  and dialog behavior, CoreEngine publish/refresh flags, Core and renderer
  threads, immutable snapshot ownership, public C ABI, and file formats are
  unchanged. Debug and Release strict builds and all four non-elevated CTest
  scenarios passed in both presets. The explicitly optional MSIX
  install/uninstall smoke was not run.

## User-requested Windows shell and package additions

- The Japanese Help menu now exposes `Inkpodについて`. Its native, owned modal
  dialog uses the reference image's 574 x 544 device-pixel layout at its 144 DPI
  capture scale, then converts once from that reference DPI to the target DPI. It
  displays the icon, `Inkpod`, the requested shorter English description,
  CMake-derived `Version 0.1.0`, and `© Shuichi Kurabayashi`. The dialog is
  centered on its owner and clamped only when the monitor work area requires it;
  its smoke test checks size, spacing, 15/9-point fonts, exact icon dimensions,
  the expanded 40 px reference name-label height, strings,
  copyright/separator separation, and origin.
- Windows App Development CLI 0.5.0 generated 48 MSIX PNG assets and one
  five-resolution ICO directly from `AppIcon.svg`. The ICO is embedded once for
  the main window, title bars, and About. About requests its target-DPI size via
  `LoadIconWithScaleDown`; the former WIC decoder and duplicate 88/256 px
  Win32 resources are removed while the MSIX PNG assets remain unchanged.
- CMake now assembles the x64 MSIX with executable, all generated assets,
  license, third-party notices, and the MSVC app-local runtime required by the
  `/MD` executable. A non-elevated CTest unpacks and verifies the actual package.
  The earlier runtime-dependent package passed elevated install/run/uninstall;
  the corrected self-contained package still needs that elevated rerun. The
  build artifact remains unsigned for the publisher's protected credential.

## Requirements

| ID | Status | Implementation | Tests | Notes |
|---|---|---|---|---|
| ARCH-001 | Verified | CMake explicitly tracks all image/format/core/FFI inputs, including M7 batch/Core/settings/FFI sources, and Cargo byproducts behind a completion stamp | Debug/Release build plus an immediate no-op Debug rebuild | CMake remains the build entry; Cargo does not run on the unchanged rebuild |
| ARCH-002 | Verified | Core/image/format are safe and frontend-independent | All three domain crates' source/manifest scan, clippy, workspace tests | No Rust Windows dependency |
| ABI-001 | Verified | ABI v1 retains M0-M6 records and adds bounded M7 graph/input/operation/output records plus immutable graph, preview, report, and thread-safe task handles | All 155 public functions have a direct contract-test reference; header/export parity gate; C11/C++20 layouts and post-M8 integrated ABI/application smoke; Rust short/oversized-stride/misaligned-nested-record, buffer, ownership, cancellation, transaction, and round-trip tests | Caller graph spans are copied on create; nested records are validated before reference creation; borrowed preview/report strings live until their owning handle is released |
| ABI-002 | Verified | Immutable snapshot owns flat M5 cubic/fill/boundary spans alongside raster/overlay data; ownership remains with the renderer queue | Core zoom invariance; Rust FFI lifetime/count tests; compiled C++ validator/D2D smoke | Vector records remain document-coordinate and snapshot-borrowed |
| IO-001 (native save) | Verified | `.inkpod` v1 adds bounded optional M3-M6 sections; `M6AD` stores stable adjustment-layer IDs and validated brightness/contrast, curve, or levels parameters while retaining M1-M5 reads | Adjustment order/parameters/composite save-reopen; native round-trip plus missing/duplicate/wrong-layer/invalid-parameter rejection | Blob compression remains optional and disabled |
| IO-001 (M2 recovery) | Verified | Atomic autosave leaves the normal savepoint/path untouched; recovery opens dirty, recovered, and pathless; Windows gives never-saved cells a private recovery path, queues timer autosave after an active stroke, and discovers private recovery at startup | Core/FFI recovery tests plus Windows active-stroke autosave, private-path discovery, and normal-vs-recovery smoke | Only the newest private recovery is prompted per launch; defer leaves it intact |
| IO-002 | Verified | Bounded PNG/TIFF/TGA/BMP codecs are exposed by native Open/Import/Export dialogs; export selects preserved alpha or explicit white, and sequence import/export uses the same validated codec path | Four-format round-trip/alpha/DPI/bounds tests plus Windows PNG export/open/import and sequence import/export menu smoke | DGA/CEL remains `Unknown`; no proprietary layout is inferred |
| DOC-001 | Verified | Cell creation and native paper, image-size, resolution, 100/reference/drawing/safe-frame, and independent-margin dialogs dispatch transactional Core commands | Metadata/native round-trip, mixed-size frame golden, and Windows creation/frame/margin/size/DPI dialog smoke | — |
| DOC-002 | Verified | Stable-ID typed layer/plane tree, including vector-coloring topology, is displayed as separate native layer and plane panes and drives the active Core node | Topology/invalid-combination/save-reopen tests, ABI node queries, and Windows pane selection smoke | — |
| DOC-003 | Verified | Native menus and pane actions provide create, duplicate, delete, drag/button reorder, visibility, editability, opacity, conversion, compatible merge, and hidden-node cleanup | Core raster/vector tree transactions plus Windows menu and real listbox drag smoke | — |
| HIST-001 | Verified | Transactions, Undo/Redo, savepoints, whole revert, multi-step history selection, layer/selection partial revert, and preview apply/cancel are available from native UI | Core history/savepoint/preview tests plus title/dirty, multi-step dialog, partial-revert, and M6 preview Windows smoke | — |
| VIEW-001 | Verified | Canvas zoom/pan/box-zoom gestures, fit, 1:1, numeric/slider zoom, and horizontal/vertical view flips update independent view state | Core mode/resize/box/flip tests; ABI transform flags; Windows gesture/dialog/DPI smoke | Manual viewport resize preserves zoom/pan while recording the new viewport dimensions |
| VIEW-002 | Verified | Ruler, guide add/move/delete, grid settings, snap, and transparent view are Core-owned and exposed through native menus/dialogs and renderer overlays | Core persistence/snap tests, snapshot overlay ABI, and Windows guide/grid/ruler/snap/transparency smoke | — |
| VIEW-003 | Verified | Document tabs create same-document logical views with independent transforms; the locator pane reports document position, selection bounds, and color | Core locator/multi-view tests, ABI same-revision snapshots, and Windows tab/locator smoke | — |
| PAINT-001 | Verified | pencil/brush/eraser, auto erase, pressure-size, clipped/bounded incremental staging; UI/Input -> Core queue uses mouse fallback and `WM_POINTER` history | Core tool/resource tests; 256-record FFI test; Windows multi-sample live stroke smoke | PAINT-001 scope complete |
| FILL-001 | Verified | Canvas seed fill uses a native option dialog for tolerance and selection clipping, then dispatches the normalized typed request | Image goldens/properties, Core/FFI transactions, and Windows option-dialog plus Canvas-click smoke | — |
| FILL-002 | Verified | The fill dialog exposes up to six inclusion/exclusion colors, overflow abort/reporting, gap-close axes/value, and detached matching regions | Inclusion/gap/overflow/cancel/no-op goldens plus Windows option-to-Core smoke | Gap close uses the documented deterministic native axis-bridge rule |
| FILL-003 | Verified | Closed-region range gestures, transparent-only/inclusion options, and bounded fill extension are available as Canvas tools with preview-safe transactions | Closed/open/colored/transparent-only/cancel/extension tests and Windows gesture smoke | — |
| SEL-001 | Verified | Rectangle, ellipse, lasso, polyline, trace, and color-wand selection are native Canvas tools backed by the persistent binary Core mask | Core authoring/property tests, C ABI validation, and Windows gesture smoke for every tool | — |
| SEL-002 | Verified | New/add/subtract/intersect authoring, invert, configurable expand/shrink, and typed equal/different color selection are exposed through native controls | Exhaustive mask algebra plus Core/Windows mode/width/color-selection smoke | — |
| SEL-003 | Verified | Selection-layer conversion commands and all eight vector selection modes are exposed through the native selection UI | Deterministic vector range/ID test, Rust/C++ caller-buffer ABI, and Windows conversion/all-mode smoke | — |
| PAINT-002 (M5 slice) | Verified | Line, curve, rectangle/ellipse shape, and polyline Canvas tools publish immutable renderer previews and commit one cubic-path transaction on completion | Core path/Undo, Rust/C++ ABI draw, preview-revision, and Windows Canvas gesture smoke | — |
| PAINT-003 | Verified | Native commands expose nearest-endpoint gap connect, three-mode dust removal with region/preview, and four line-width corrections | Vector transactions, dust mode/region/cancel/atomicity tests, ABI, and Windows command/dialog smoke | — |
| VECTOR-001 | Verified | `inkpod-image` fixed-point cubic/variable-width geometry, Core main-line/color-trace/fill topology, trace-before-protected-main paint order, immutable vector snapshot, and continuous closed-path D2D fill/outline rendering with width-preserving bounded-miter joins | Image geometry; Core zoom/save/order/golden; Rust FFI; compiled C11/C++20 renderer plus closed-seam/corner `FillContainsPoint` smoke | Arbitrary raster/vector interleaving still renders vector after precomposited raster tiles |
| VECTOR-002 | Verified | Native Canvas/menu controls cover transactional draw, partial/intersection/full erase, connect, four width modes, all selection modes, vector-to-new-raster-layer rasterize, and raster-to-vector conversion; draw commands require an active vector main-line/color-trace plane | Five M5 acceptance tests, object-limit/all-mode/transactional tests, FFI buffers/conversion, Windows RunM5 command/Canvas smoke, and initial-cell disabled-command/local-diagnostic regression | Vector-to-raster creates a new RGBA8 layer and preserves the source vector layer as one Undo unit |
| FILTER-001 | Verified | Deterministic fixed sharpen/blur presets, bounded Gaussian/unsharp, channel invert, and alpha-independent auto contrast support RGBA8/16 and selection clipping; all catalog entries use native editor/preview/last-filter paths | Exact 8/16-bit alpha/selection golden; catalog/image tests; Core/FFI/Windows editor smoke; task progress/cancel | Work runs on the Core engine worker while the UI polls/cancels a thread-safe task |
| FILTER-002 | Verified | Brightness/contrast, RGB/R/G/B Bezier/B-spline curves, levels, HSV, and color balance use documented normalized 16-bit clamp/rounding; native editors expose channel, interpolation, parameters, and curve points | Catalog validation, preview/Undo acceptance, C ABI validation, full workspace and Windows editor tests | Unknown legacy preset byte layouts are not fabricated |
| EFFECT-001 | Verified | Linear/radial 3–16-stop alpha gradients with dither/45-degree constraint, pressure/fade/spacing/continuous airbrush, boundary-only effect, pen/rectangle/polyline/lasso blur with screen-fixed pressure diameter, and round/square pressure-sensitive offset stamp gestures are connected through Core/C ABI/native Canvas | Boundary acceptance; deterministic gesture/pressure and extreme-coordinate tests; Rust/C++ ABI and Windows editor/gesture smoke | Native algorithms are documented Inkpod semantics, not inferred proprietary kernels |
| ADJUST-001 | Verified | Stable-ID non-destructive brightness/contrast, levels, and curve layers compose in palette order with visibility/opacity, persist in `M6AD`, and are exposed as multiple selectable/re-editable/reorderable native entries; alpha row edit/gradient and grayscale view preserve RGB | Order/source/opacity/visibility/save-reopen; malformed metadata; alpha RGB-preservation; Rust/C++ ABI and multiple-adjustment Windows smoke | — |
| CLIP-001 | Verified | Cut/Copy and compatible, selected-plane, or converted-plane Paste preserve document coordinates; the adapter publishes/imports the private typed format and standard CF_DIBV5/CF_DIB | Core cross-paper/failure tests, FFI ownership, and Windows private plus DIB-only external-clipboard menu smoke | Standard interchange is bounded to supported 24/32-bit DIB layouts; other Windows formats are ignored safely |
| XFORM-001 | Verified | Native menus/dialogs separate view-only flips from destructive horizontal/vertical mirror, 90-degree rotation, image/paper size, and resolution changes | Core pixel/frame/guide/history tests and Windows command/dialog revision smoke | — |
| XFORM-002 | Verified | Floating paste offers dialog and Canvas-handle move/scale/rotate preview, Enter/OK commit, and Esc/Cancel restoration | Core coordinate/scale/rotate/retry/cancel, FFI lifecycle, and Windows dialog/handle gesture smoke | — |
| SHORT-001 | Verified | The categorized native editor covers 24 menu/tool/other commands; rebind conflict replacement, actual key resolution, and reset use the Core shortcut map | Core/FFI resolve tests plus Windows editor/conflict/reset and real WM_KEY command smoke | — |
| COLOR-001 | Verified | Native RGB/HSV plus alpha editor preserves straight RGBA8/16 values; eyedropper source menus select active/topmost/composite/light-table sampling | Exact-depth Core/FFI tests and Windows editor/source-selection/Canvas sampling smoke | Display conversion remains explicit BGRA8 and does not replace the stored exact-depth color |
| COLOR-002 | Verified | Palette, named chart pages/search/edit/save/load, sequence subpalette registration/sampling, and color-check panes are native workflows with 1–0/Tab shortcuts | Palette/chart/subpalette Core tests and Windows pane/menu/shortcut/save-load smoke | Legacy palette/chart byte layouts remain `Unknown`; Inkpod uses documented `.inkpalette`/`.inkchart` files |
| LT-001 | Verified | Split native set/item panes expose set create/duplicate/delete/rename/reorder, item add/delete/reorder/properties, per-item transform/color/mode/opacity, and global opacity | Set/item transaction/opacity/native round-trip tests plus Windows administration/property smoke | — |
| LT-002 | Verified | Native controls expose reference-frame alignment, Canvas movement (Shift moves all), boundary/color sampling, reload, and dirty-safe edit-image swap while preserving display state | Mixed-size/reopen/swap goldens, cancellation/read-only fill, ABI, and real Canvas move/reload/sample/swap smoke | — |
| SEQ-001 | Verified | Native sequence pane/file commands provide natural-order import/export, thumbnails, numbered first/previous/next/last/goto navigation, and gap-safe switching | Natural-order/gap/thumbnail/dirty-switch tests plus Windows file/menu/pane smoke | — |
| SEQ-002 | Verified | Timed motion UI supports 30/25/24/12/10/8 FPS, loop, pause/step/first/last, selection/light-table options, timer playback, and keyboard shortcuts | Core motion loop/pause tests, C ABI state tests, and Windows timer/menu/shortcut smoke | — |
| BATCH-001 | Verified | Versioned checksummed `.inkbatch` settings persist file/folder/current-sequence selectors, ordered enabled/configure-each-run operations, conjunctive stable-ID/type target selectors, and output/failure policies; a modeless native Batch palette supports add/edit/remove/reorder and save/load | Format checksum/version/bounds/atomic replacement tests, complete operation/filter graph round-trip, FFI copied-span ownership, and Windows `WM_COMMAND`/dialog smoke | Loaded graphs remain immutable handles; editing starts a new UI graph rather than mutating borrowed settings storage |
| BATCH-002 | Verified | Core and native Batch UI expose color replacement/swap, continuous fill seeds, separation, visibility, four vector line-width modes, all M6 filters, boundary airbrush, dust removal, mirror, 90-degree rotation, resize/DPI, and plane conversion with stable target/missing policy | Complete operation/filter versioned round-trip, Core operation/transaction tests, six named M7 acceptance tests, C ABI nested-record validation, and Windows add/edit/reorder/remove/swap/boundary dry-run smoke | Boundary airbrush starts with the required two colors; operations use documented Inkpod semantics; no proprietary legacy batch preset layout is inferred |
| BATCH-003 | Verified | Preview resolves natural-order inputs/output names and seed warnings; current scope matches the open file/UUID; dry-run, current/all execution, atomic per-file save, continue/stop failure reports, progress, cancellable waits, and thread-safe cancel run on the Core engine worker | Named M7 acceptance 6/6; current-file/wait-cancel regressions; FFI dry-run/report/cancel tests; final Release and reviewed Debug ABI/real Windows palette preview/dry-run/output smoke | Default Duplicate policy cannot overwrite input; overwrite requires the explicit policy |
| M0 Windows shell (Help/About) | Verified | Japanese Help command, reference-DPI-normalized owned modal About, reference-matched spacing, 15/9-point fonts with a 40 px reference-height name label, target-size native icon loaded from the shared application ICO, shorter description, copyright, and CMake-derived version | Debug `inkpod_windows_smoke` verifies exact target-DPI size/origin/spacing/name-label height/font/icon/string and non-overlap; final assets + ABI + application CTest passed 3/3 | The 574 x 544 reference is device pixels captured at 144 DPI and is scaled exactly once; native Win32 theme and keyboard/modal behavior are retained |
| M8 legacy compatibility audit | Verified | Per-codec fixture/read/write/round-trip matrix with a rights-cleared fixture/oracle gate | `acceptance_unverified_legacy_codecs_remain_unknown` | DGA, CEL, and three legacy preset codecs remain `Unknown` at measured zero scope |
| M8 malformed-input resilience | Verified | Forged native/batch/PNG/TIFF/TGA/BMP corpus with exact bounded reject-path assertions, deterministic valid-seed truncation/bit-flip mutation, and failed-open Core preservation | Named corrupted-corpus, mutation, and Core failed-open tests | Rejects without panic, current-document or file overwrite, or temporary output |
| M8 large-document performance | Verified | Maximum-dimension sparse raster/COW and bounded dense filter benchmark | `cargo bench -p inkpod-image --bench large_document -- --quick` | Reports timing and allocation-relevant counts without a hardware-specific threshold |
| M8 Windows package | In progress | MakeAppx x64 MSIX includes executable/assets/license/notices and app-local MSVC runtime; test signs a copy with an ephemeral matching certificate | Debug/Release package and unpacked-payload smoke 4/4; prior runtime-dependent artifact passed elevated install/installed ABI/uninstall | Corrected self-contained artifact needs one elevated install/run/uninstall pass; distribution signing remains an external protected-credential step |
| M8 Core portability | Verified | All four Rust crates' `src/tests/benches/examples/build.rs`, crate/workspace manifests, and lockfile reject Windows imports/configuration/packages; next-frontend gaps documented | `acceptance_rust_workspace_has_zero_windows_imports`; Linux/macOS CI | Byte/stream I/O and platform file-authority adapters remain explicit next-frontend work |

### GUI vertical-slice audit (2026-07-24)

For user-invoked requirements, `Verified` now requires a production menu,
dialog, toolbar, or Canvas gesture to reach the Windows adapter, C ABI, Core,
and visible/document result, with a Windows test covering that route. Direct C
ABI calls made only inside the application smoke test do not count as a GUI
vertical slice. If one user-visible operation grouped under a requirement ID is
still unavailable, the requirement remains `In progress`; completed Core/ABI
subsets stay recorded in the implementation and test columns. Internal build,
ABI, and renderer contracts do not require an artificial GUI entry point.

All M0-M7 rows satisfy that rule. The application
smoke enters through real `WM_COMMAND`, dialog control, listbox drag, keyboard,
timer, clipboard, and Canvas pointer paths; it does not count a direct smoke-only
C ABI call as completion. M7 adds real Batch palette and menu command paths;
M8 packaging is a build/install boundary and therefore uses package and
installed-binary smoke rather than an artificial in-app command.

## M0 re-verification before M1

| Criterion | Status | Evidence on 2026-07-20 before M1 edits |
|---|---|---|
| Rust format/lint/test baseline | Verified | `cargo fmt`, clippy `-D warnings`, and workspace tests passed (Core 2 + architecture 1 + FFI 4) |
| Windows x64 app creates main window/Canvas | Verified | Existing Debug `inkpod_windows_smoke` passed |
| CMake Rust target is incremental | Verified | Existing Debug build reported `ninja: no work to do` |
| create -> empty snapshot -> release -> destroy | Verified | Existing Debug C++ ABI smoke passed |
| panic/leak/double release error paths | Verified | Existing Rust negative/lifecycle tests and Debug ABI smoke passed |

## M2 re-verification before M3

| Criterion | Status | Evidence on 2026-07-20 before M3 edits |
|---|---|---|
| Rust formatting and warnings | Verified | `cargo fmt --all -- --check` and workspace/all-target/all-feature clippy with `-D warnings` passed |
| M2 Rust behavior | Verified | Workspace tests passed: Core 18, architecture 1, FFI 7, format 6, image 13, plus doc-tests |
| M2 Windows vertical slice | Verified | Existing `ctest --preset windows-x64-debug` passed assets, integrated C11/C++ ABI smoke, and M1/M2 app smoke (3/3) |
| M2 acceptance table | Verified | All eight named M2 golden/recovery cases below remained green before any M3 implementation edit |

## M3 re-verification before M4

| Criterion | Status | Evidence on 2026-07-20 before M4 edits |
|---|---|---|
| Named M3 Core acceptance tests | Verified | `cargo test -p inkpod-core m3_acceptance -- --nocapture`: 5 passed, covering all six scenarios |
| Existing Windows M1-M3 boundary | Verified | `ctest --preset windows-x64-debug --output-on-failure`: assets, integrated ABI, and application smoke passed 3/3 |
| Worktree baseline | Verified | `git status --short --branch` reported clean `main...origin/main` before implementation |

## M4 re-verification before M5

| Criterion | Status | Evidence on 2026-07-21 before M5 edits |
|---|---|---|
| Named M4 Core acceptance tests | Verified | The five `acceptance_*` library tests passed 5/5 and cover all six M4 scenarios; the subsequently enumerated, unrelated Windows architecture test executable was blocked before start by local Application Control |
| Existing Windows M1-M4 boundary | Verified | `ctest --preset windows-x64-debug --output-on-failure` passed assets, integrated C11/C++ ABI, and application smoke 3/3 before relinking M5 |
| Worktree baseline | Verified | `git status --short` was clean before M5 implementation |

## M5 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Zoom does not change Core vector geometry | Verified | `acceptance_zoom_never_changes_core_vector_geometry` compares every stored cubic point/width before and after view zoom and verifies only the view revision changes |
| 2 | Partial erase does not change another stroke | Verified | `acceptance_partial_erase_changes_only_the_touched_stroke` checks untouched path ID/geometry byte-for-byte and verifies one atomic Undo restores the erased path |
| 3 | Intersection erase cut points are deterministic | Verified | `acceptance_intersection_erase_cut_points_are_deterministic` runs equivalent documents twice, compares fixed-point pieces exactly, and fixes the crossing cut at the same parameter |
| 4 | Fill topology survives save/reopen | Verified | `acceptance_fill_topology_survives_save_and_reopen` persists stable path/fill/plane IDs and ordered boundary IDs through `.inkpod`; format malformed-topology tests reject cross-layer/open/missing relations |
| 5 | Rasterize antialias, pixel center, and scale are golden-fixed | Verified | `acceptance_rasterize_antialias_pixel_center_and_scale_golden` compares complete straight-RGBA buffers for center-sampled 1x, 4x4-AA 1x, and scaled 2x output |

The complementary `vector_002_connect_width_select_and_raster_vector_conversion_are_transactional`
test covers connect/repeated-connect no-op, all width modes, deterministic
rasterization/plane order, bounded RGBA8 run vectorization, invalid/no-op
behavior, and Undo/Redo. `vector_002_all_selection_modes_have_deterministic_ranges_and_ids`
executes all eight vector selection modes.
Format and FFI M5 tests cover bounded native topology, nested record rejection,
caller-owned count/size buffers, conversion, and snapshot lifetime. The real
Windows smoke creates a vector layer/path/fill on the Core engine, publishes its
snapshot to the Renderer, checks zoom invariance, and requests one D2D frame.
That code passed both Debug and Release Windows smoke presets.

### M5 review corrections

| Finding | Correction and regression evidence |
|---|---|
| The persisted vector plane-to-layer map was built but unused, so a fill could reference a closed path from another vector layer | Native validation now requires every fill and boundary path to resolve to the same vector layer; malformed cross-layer and out-of-Core-coordinate fixtures are rejected |
| Connect, erase, vectorize, and tree duplication could exceed aggregate path/fill/segment/boundary limits and leave an unsavable Core state | Central checked collection limits now guard every growth path; raster vectorization derives a bounded run capacity and a focused test covers each persisted collection limit |
| M5 commands allocated stable IDs before the history commit could fail | Path/fill/connector/vectorize and vector-tree duplicate IDs now remain local until commit succeeds; a forced revision-overflow regression test verifies the allocator is unchanged |
| Repeated connect could select endpoints already joined by the previous connector and add degenerate/duplicate connectors | Coincident endpoints are treated as connected; a repeated command is a revision-preserving no-op |
| Positive widths below fixed-point precision rounded to zero, and transparent RGBA8 at threshold zero produced invisible vector objects | Sub-millipixel widths are rejected before commit; zero-alpha pixels are always skipped; Core and FFI negative/no-op tests cover both paths |
| Raster vectorization accepted vector placeholder planes and non-editable targets | Source kind is restricted to RGBA8 color/raster planes and both destination stroke/fill planes must be editable; Core/FFI tests exercise rejection |
| Main-line and color-trace paint order depended on object creation order, and only two of eight selection modes were executed | Snapshot/rasterization group traces before the protected main line; a golden pixel and a named all-mode selection test fix the order and normalized ranges/IDs |
| A fresh raster cell allowed vector draw-tool selection, then reported the unrelated cached `no sequence is configured` diagnostic | Vector draw commands now disable outside vector main-line/color-trace planes and reject direct command dispatch; local adapter failures replace cached Core diagnostics, an unconfigured sequence is a normal empty pane, and Windows smoke covers disabled/re-enabled commands plus the exact local diagnostic |
| Closed D2D strokes dropped the duplicate end sample but built one open outline polygon, leaving the final sampled interval disconnected from the start | Closed strokes now build independently closed inner and outer contours under alternate filling; the M5 renderer smoke probes the former rectangle seam midpoint and requires it to be inside the generated geometry |
| D2D stroke corners normalized the bisector but omitted the angle-dependent miter length, narrowing a 90-degree rectangle corner to about 70.7% of the requested half-width | Joins now intersect the incoming/outgoing offset lines, use a four-times-half-width miter limit, and emit two-point bevel contours beyond that limit; the renderer smoke probes between the old and corrected 90-degree corner boundaries |

## M5 re-verification before M6

| Criterion | Status | Evidence on 2026-07-22 before M6 edits |
|---|---|---|
| Named M5 acceptance tests | Verified | `cargo test --workspace m5_acceptance -- --nocapture` passed 5/5: zoom invariance, partial erase isolation, deterministic intersection cuts, fill-topology reopen, and rasterization golden |
| Existing recorded Windows M1-M5 boundary | Verified | The reviewed M5 status already recorded Debug and Release CTest 3/3 with integrated ABI and real application/D2D smoke; the M6 run subsequently retained all prior smoke paths |
| Worktree baseline | Verified | Windows `git status --short` was clean before the M6 implementation |

## M6 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Cancel restores the original tile checksum | Verified | `acceptance_cancel_restores_the_original_tile_checksum` proves preview snapshot divergence without committed revision/history change, then exact checksum restoration; Rust/C++ ABI and real Windows smoke repeat the check |
| 2 | Apply is one Undo unit | Verified | `acceptance_apply_is_exactly_one_undo_unit_and_last_filter_reuses_it` checks one history entry, exact Undo/Redo, and last-filter reuse; C++/Windows smoke applies then restores with one Undo |
| 3 | Adjustment layer leaves the source unchanged and reorder changes composite | Verified | `acceptance_adjustment_order_changes_composite_without_changing_source_plane` compares source checksums, changes layer order, observes a different BGRA composite, verifies opacity/visibility, and verifies the same order/parameters/composite after save/reopen |
| 4 | 8/16-bit, alpha edge, and selection edge are golden-fixed | Verified | `acceptance_eight_sixteen_bit_alpha_and_selection_edges_are_golden_fixed` fixes full-depth selected inversion results, unchanged outside-edge pixels, and exact alpha preservation for RGBA8/RGBA16 |
| 5 | Boundary airbrush does not blur uniform regions | Verified | `acceptance_boundary_airbrush_preserves_uniform_regions` checks exact far-field pixels and changed boundary-band pixels; a second image test covers the same rule on a 2D raster |

The filter catalog includes fixed sharpen/blur, Gaussian, unsharp, invert,
alpha-independent auto contrast, brightness/contrast, channel curves, levels,
HSV, and color balance. Selection masks clip every raster operation; blur uses
straight-alpha-safe premultiplied accumulation and deterministic rounding.
Gradient, pressure-aware airbrush/stamp gestures, screen-fixed pressure blur,
boundary effect, dust removal, alpha-only row edit/gradient, and alpha grayscale
view are typed Core/image operations connected through copied/strided C ABI
records. The versioned `M6AD` section rejects missing, duplicate, invalid, or
non-adjustment relationships. The C ABI validates sizes, strides, counts,
ownership, task state, and work bounds before mutation. Debug and Release
Windows smoke execute preview cancel/apply/Undo, every native filter/editor
route, multiple adjustment create/select/re-edit/visibility/reorder, gesture
records, snapshot publication, and a real D2D render.

## M6 review corrections

| Finding | Correction |
|---|---|
| Filter validation could panic on `i32::MIN`, and effect coordinate subtraction could overflow before conversion | Replaced absolute-value validation with bounded ranges and performs coordinate differences in floating point; extreme-value tests cover both cases |
| Stamp built an unnecessary sample vector and large images/radii lacked explicit work ceilings | Stamp now iterates only the clipped overlap without staging allocation; raster edit and radius-work limits reject unreasonable input before mutation |
| Adjustment opacity was ignored during composition | Composition interpolates original and adjusted pixels using `opacity_milli`; opacity 0, hidden state, reorder, source preservation, and save/reopen are tested |
| Effect, adjustment-update, and alpha-edit Core operations stopped before C ABI | Added copied records and synchronous owner-thread functions for every implemented M6 primitive, with Rust and C++ negative/ownership smoke |
| Curve pointer/count had no element stride, while growing the record would break ABI v1 layout | Reused the reserved 32-bit slot as `point_stride_bytes`, retained the 72-byte record and packed-stride-0 compatibility, and tests packed/strided callers |
| The exact 8/16-bit golden and no-op/invalid history behavior were under-tested | Golden tests now assert exact selected RGBA values; transactional tests cover no-op, invalid input, adjustment update, Undo, and Redo |
| M6 had no native command path | Added the complete filter catalog, parameter/curve editor, preview/apply/cancel/last-filter flow, and effect/adjustment commands through the Core engine and renderer snapshot path |
| Suspected CMake/Cargo rebuild on every invocation | Explicit input/stamp dependency graph was retained; an immediate unchanged Debug rebuild reports `ninja: no work to do` |
| Possible Windows-specific type leakage into Rust Core | Existing architecture scan plus full clippy/tests confirm Core/image/format contain no Windows API dependency |
| Full effect gesture/editor controls were absent | Added native editors plus Canvas-batched device-coordinate gradients, pressure/fade/spacing airbrush with 50 ms stationary continuous-spray samples, boundary effect, pressure/screen-fixed blur regions, Alt-source round/square stamp, and alpha gradient/view; Core converts the batch once per gesture and commits one Undo unit |
| Dust removal was absent | Added bounded deterministic foreground-speck, transparent-hole, and color-outlier modes over full plane or pen/rectangle/polyline/lasso, selection intersection, preview OK/Cancel, progress, cancellation, and atomic no-partial-commit tests |
| Filter work had no progress/cancel ownership model | Added Rust-owned atomic `InkpodTask`; Windows creates it on UI, runs the Core call on the Core engine thread, polls/cancels from UI, posts completion only after snapshot publication, and releases exactly once after the worker returns |
| Native adjustment controls tracked only one layer | Added multiple create with unique names, previous/next selection, selected-layer parameter reload/re-edit, visibility toggle, and reorder; integrated Windows smoke executes the complete sequence |

## M6 re-verification before M7

| Criterion | Status | Evidence on 2026-07-25 before M7 edits |
|---|---|---|
| Named M6 acceptance tests | Verified | `cargo test --workspace m6_acceptance -- --nocapture` passed 5/5: preview cancel checksum, one-unit apply/Undo, source-preserving adjustment reorder, RGBA8/16 alpha/selection golden, and boundary-only airbrush |
| Existing recorded Windows M1-M6 boundary | Verified | The reviewed M6 status recorded strict Debug CTest 3/3 with integrated ABI and real menu/dialog/Canvas smoke; final M7 Debug and Release smoke retained all preceding paths |
| Worktree baseline | Verified | `git status --short --branch` was clean before M7 implementation |

## M7 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | dry-run writes no files | Verified | `acceptance_dry_run_writes_no_files` checks the resolved output remains absent and reports `DryRun`; FFI and Windows `IDM_BATCH_DRY_RUN` smoke repeat it |
| 2 | Default output policy never overwrites input | Verified | `acceptance_default_output_never_overwrites_input` verifies unchanged input bytes and a distinct duplicate output; graph defaults and Windows graph-info smoke require Duplicate |
| 3 | Cancelled current file leaves no temporary output | Verified | `acceptance_cancelled_file_leaves_no_temporary_output` cancels during save, preserves the destination, and checks the same-directory `.inkpod.tmp.*` file is removed; format cancellation tests cover settings/output cleanup |
| 4 | One failure is recorded and policy continues or stops | Verified | `acceptance_failure_policy_records_and_continues_or_stops` compares full per-item reports and downstream execution for Continue and Stop |
| 5 | Color replacement old/new swap round-trips | Verified | `acceptance_color_replacement_swap_round_trips` swaps every pair, saves/loads `.inkbatch`, and compares the graph; Windows smoke invokes the production swap/save/load commands |
| 6 | Continuous-fill seed moved to another color warns in preview | Verified | `acceptance_continuous_fill_preview_warns_when_seed_moves_color` previews two frames and emits the warning only where the expected source color changed |

The graph format has independent container and graph/operation versions,
bounded UTF-8/count/payload fields, checksum validation, and same-directory
atomic replacement. Execution resolves file, naturally sorted folder, or the
Core-owned current sequence, creates a separate working Core per input, applies
enabled operations in order, and commits only a completed output. The native
Batch palette uses an immutable graph handle while a worker is active, polls a
thread-safe task in the progress dialog, owns preview/report handles until
replacement or shutdown, and posts only completion status to the UI thread.

### M7 review corrections

- Batch target selectors now require a layer selector and, for plane operations,
  a plane selector. Stable IDs and type constraints are matched conjunctively,
  so a converted or type-mismatched stable target follows its skip/error policy.
- `Current` file/folder execution now selects the Core's open path instead of
  silently taking the first naturally sorted input. A current cell outside the
  resolved input is a validation error.
- File-to-file wait intervals poll cancellation every 50 ms rather than sleeping
  for the entire configured interval. Atomic settings/output saves also poll
  after encoding and between bounded write chunks before replacement.
- C ABI strided records reject storage spans beyond `isize::MAX`, and a nested
  `InkpodFilterInput` is checked for alignment and `struct_size` before Rust
  creates a reference. Regression tests cover both invalid forms.
- The native boundary-airbrush batch command now supplies the required two-color
  input and is exercised through a real `WM_COMMAND` dry-run. Settings atomic
  replacement and every operation/filter serialization variant gained tests.

## M7 re-verification before M8

| Criterion | Status | Evidence on 2026-07-25 before M8 edits |
|---|---|---|
| Named M7 acceptance tests | Verified | Windows compiled the suite, then local Application Control blocked the fresh test EXE before startup (`os error 4551`); the same clean worktree ran under WSL with an isolated target and passed all six scenarios 6/6 |
| Worktree baseline | Verified | `git status --short --branch` reported clean `main...origin/main` before M8 implementation |

## M8 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Record measured read/write/round-trip scope for every legacy codec | Verified | The compatibility matrix separates DGA, CEL, palette, chart, and filter preset codecs and records 0 rights-cleared fixtures plus 0 read/write/round-trip variants for each |
| 2 | Never mark an unverified codec `Verified` | Verified | `acceptance_unverified_legacy_codecs_remain_unknown` parses every legacy row and requires `Unknown` plus the explicit zero-scope measurements |
| 3 | Corrupted corpus causes no panic, uncontrolled allocation, or overwrite | Verified | The named corpus test now asserts each intended bounded rejection path (including a valid-CRC oversized PNG), mutation passes every decoder without panic, and `corrupted_open_preserves_the_current_document_and_every_file` proves failed open leaves Core state and both files unchanged with no temp |
| 4 | Package installs and uninstalls on clean Windows 11 | In progress | Review found the prior installed artifact omitted the `/MD` MSVC runtime and was tested on a machine that already had it. The corrected MSIX contains app-local CRT DLLs and passes Debug/Release artifact-unpack checks; its elevated install/installed ABI/uninstall rerun was cancelled at UAC |
| 5 | Rust crates have zero Windows imports | Verified | `acceptance_rust_workspace_has_zero_windows_imports` scans all four crates' source/test/bench/example/build-script inputs, crate/workspace manifests, and resolved `Cargo.lock` for Windows imports, cfg/raw-DLL use, renamed packages, and Windows packages |

The large-document benchmark adds the M8 performance baseline without a brittle
wall-clock threshold. `--quick` exercised a 1,048,576-square sparse document
with 512 allocated tiles/COW isolation and a 1024-square 4 MiB dense invert in
8 ms and 49 ms respectively under the review WSL run. The next-frontend audit
records byte/stream I/O and platform file authority as explicit adapter/API gaps;
document/image/history/batch/snapshot state remains portable Rust.

## M4 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Different-size cells align by reference frame | Verified | Core `acceptance_reference_frame_aligns_different_cell_sizes_and_reopens` aligns 4x4 `(2,2)` to 8x8 `(4,4)`, compares the complete 8x8 BGRA tile golden, then checks `.inkpod` reopen and edit-image swap |
| 2 | Individual 50% × global 50% = effective 25% | Verified | Core `acceptance_individual_and_global_opacity_multiply_to_twenty_five_percent`; Rust/C++ ABI checks effective alpha 64 |
| 3 | Light-table fill boundary never changes the reference | Verified | Core `acceptance_light_table_fill_boundary_is_read_only` checks cancellation/fill/Undo exactness; Rust and C++ ABI invoke the new read-only boundary flag and re-sample the unchanged source |
| 4 | Previous/next does not silently discard an unsaved cell | Verified | Core `acceptance_sequence_switch_rejects_unsaved_document_without_discarding_it` checks explicit error plus unchanged UUID/revision/dirty, then clean switch; Rust/C++ ABI status 12 |
| 5 | Sequence gaps and natural ordering are correct | Verified | Core `acceptance_sequence_gaps_natural_order_thumbnails_subpalette_and_motion` sorts 1,3,10 without synthesizing missing cells and covers thumbnails/motion/subpalette/PNG sequence export-import |
| 6 | Common-format round-trip verifies depth/alpha/dimensions/DPI | Verified | Format round-trip covers all four formats and RGBA16 where representable; explicit-white, TIFF unassociated-alpha, indexed PNG, origin/alpha-aware TGA, and padded 24-bit BMP tests verify policy; oversized dimensions are rejected before allocation |

The M4 Windows ABI smoke is linked into the real `inkpod.exe` test binary. It
validates C11/C++20 record layouts, reference alignment, 25% opacity, read-only
reference-boundary fill, natural sequence order, motion step, explicit dirty
rejection, and all prior ABI paths.
The pre-M5 normal application smoke remained green, and the reviewed M5-linked
application smoke passes in both Debug and Release.

### M4 review corrections

| Finding | Correction and regression evidence |
|---|---|
| `i32::MIN` item rotation could overflow `abs()` and panic | Core and native validation use `unsigned_abs`; Rust FFI/native negative tests preserve revision/state |
| A light-table source-plane ID could collide with an M3 layer/plane/guide ID | Native validation now builds one occupied stable-ID set and rejects the malformed fixture |
| FFI raster dimensions/bytes were bounded only after copying | Shared dimension/1-GiB limits run before allocation; sequence bytes are cumulatively bounded; padded rows and caller-buffer reuse are tested |
| Persisted active-set selection bypassed revision, dirty, savepoint, and Undo | Active selection now commits through the document transaction path; savepoint/Undo behavior is tested |
| Light-table boundary fill was Core-only | ABI fill flags connect boundary/color sampling; Rust/C++ smoke verifies a 63-pixel fill without reference mutation |
| TIFF alpha was round-tripped internally without declaring its semantics | Writer emits unassociated `ExtraSamples=2`; reader rejects associated/undeclared fourth samples |
| Color-mode RGBA16 light-table sampling was quantized through display RGBA8 | Sampling now preserves exact source depth and applies opacity to 16-bit alpha; Core/FFI assert non-8-bit values and reject implicit 16-to-8 fill |
| Common-format readers mainly proved self-round-trip and missed common external variants | PNG expands indexed transparency, TGA honors origin/alpha descriptor bits, and BMP accepts padded 24-bit rows while validating 32-bit masks; focused fixtures cover each path |

## M3 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Layer reorder/duplicate/delete survives Undo/Redo/save/reopen | Verified | Core `acceptance_layer_tree_undo_redo_save_reopen_and_validation`; C++ ABI and real Layer-menu Windows smoke |
| 2 | Core rejects disallowed layer/plane combinations | Verified | Same Core test plus Rust/C++ ABI invalid selection-plane-in-coloring-layer checks; revision remains unchanged |
| 3 | Selection boolean passes a property test | Verified | Core `acceptance_selection_boolean_property_and_authoring_tools` exhaustively checks all 256 left masks against representative right masks and every bit for new/add/subtract/intersect |
| 4 | Paste between different paper sizes preserves document-origin coordinates | Verified | Core `acceptance_coordinate_preserving_typed_paste_and_floating_transform`; FFI and Windows 8x8 `(6,6)` to 4x4 translated `(2,2)` smoke |
| 5 | View flip and destructive mirror use separate history/revisions | Verified | Core `acceptance_view_flip_and_destructive_mirror_have_separate_revisions`; snapshot flip flag and Windows document/view revision plus Undo checks |
| 6 | Editing one view appears at the same revision in another view's next snapshot | Verified | Core `acceptance_multi_view_locator_guides_grid_and_shortcuts`; Rust/C++ ABI and Windows primary/secondary snapshot revision equality |

The M3 Windows smoke also exercises the actual Copy, Layer, Selection, Flip,
Mirror, Grid, New View, Shortcut Editor, and Shortcut Reset commands, typed clipboard ownership,
locator selection bounds, guide/grid persistence, and a final Direct2D render.

### M3 review corrections

The 2026-07-20 review found and corrected these M3-only defects:

- Layer index zero was treated as top by tree commands but composed as bottom;
  merge-below used the opposite direction. Rendering and merge now share the
  palette order, semitransparent RGBA8/16 merge uses source-over, duplicated
  plane names are unique, limits are checked, and deletion repairs a dangling
  active-plane ID.
- Paste now selects a compatible typed destination even when the active plane is
  incompatible, validates payload bounds, retains floating state after failure,
  and uses bounded inverse sampling for scale/rotation. Scale, rotation, retry,
  cancel, and cross-paper coordinates have explicit tests.
- Layer/plane editable flags now guard stroke, fill, paste, and main-line metadata
  edits. Snap calculations now obey `snap_enabled`.
- M3 file validation rejects out-of-paper guide positions, oversized grid
  spacing, and extra unreferenced plane payloads.
- Snapshot overlay ownership now carries guide/grid/view flags to the renderer;
  Grid and Guide state is visibly rendered and transparent view has a checker.
  `InkpodSnapshotTransform.flags` no longer overloads a reserved field.
- Shortcut editor bindings now resolve through Core in the actual Windows key
  path. Selection-point records cannot exceed their advertised stride, point-free
  requests require a zero span, and opaque clipboard/snapshot output-owner rules
  and repeat release tests are explicit.

## M2 acceptance scenarios

| # | Required golden case | Status | Evidence |
|---|---|---|---|
| 1 | Only a completely closed region is filled | Verified | Image test `m2_golden_only_completely_closed_regions_are_filled` exercises the closed-region operation with adjacent closed and edge-connected regions |
| 2 | A one-pixel gap leaks at setting 0 and closes at setting 1 | Verified | Image test `m2_golden_one_pixel_gap_leaks_at_zero_and_closes_at_one` compares both settings on the same trace |
| 3 | Overflow abort reports the reached edge and commits zero pixels | Verified | Image test `m2_golden_overflow_abort_and_cancel_never_mutate_the_source`, Core atomicity test, and FFI leak-coordinate check verify plan-before-commit behavior |
| 4 | Included trace colors are replaced while non-target trace colors remain | Verified | Image test `m2_golden_inclusion_replaces_target_trace_but_preserves_other_trace` covers specified-color inclusion and preservation of the other trace |
| 5 | Grayscale-line display coverage agrees with base-color eyedropper sampling | Verified | Image test `m2_golden_grayscale_display_coverage_and_base_color_eyedropper_agree` plus Core exact-depth save/reopen and cache-safe view-only snapshot/eyedropper test |
| 6 | Pixels outside the selection remain unchanged | Verified | Image test `m2_golden_selection_clips_every_fill_edit`; Core accepts the typed selection rectangle without changing the protected main-line plane |
| 7 | A 16-bit value is never implicitly quantized to 8 bit | Verified | Image palette/fill golden, format byte-exact RGBA16/base-color/palette round-trip, ABI palette copy, and FFI rejection of a 16-bit fill into an RGBA8 plane |
| 8 | Opening an autosave never overwrites the normal saved file | Verified | Core test `m2_autosave_recovery_never_inherits_or_overwrites_normal_path`, FFI recovery test, and Windows smoke compare the reopened normal-file checksum after a pathless recovery session |

The Windows smoke additionally drives the real Fill and Eyedropper menu
commands through a Canvas click, checks that fill is one revision/Undo unit and
does not alter the main-line checksum, renders the temporary color check, queues
autosave without a UI wait (including between stroke begin and end), verifies
that it runs after stroke commit without an asynchronous error, opens recovery
as dirty/pathless, and then reopens the unchanged normal file.

## M2 review fixes

- The previously isolated image-crate palette is now a bounded Core document
  property with Undo/Redo, caller-owned strided C ABI set/get buffers, and exact
  `.inkpod` persistence. No Rust allocation ownership crosses the new ABI.
- Grayscale main-line base color is now persisted instead of being reset to
  black on reopen. Existing flag-0 M1 `.inkpod` v1 files remain readable with
  the historical black/empty-palette defaults.
- Color-check snapshots now carry explicit legacy/native background flags and
  receive a new render-tile revision after view-only recomposition. This fixes
  the D2D tile cache previously retaining the normal image and the sparse
  native-alpha background previously appearing as ordinary white paper.
- Closed-region fill now handles enclosed colored components when
  transparent-only is off. Closed-region and extension planners poll
  cancellation, and oversized documents are rejected before a selection
  rectangle can be expanded into a mask.
- Legacy-white classification uses exact white RGB independently of hidden
  alpha; native-alpha classification gives transparent pixels precedence.
- Never-saved cells now receive a private recovery path before editing. The
  Windows startup path discovers the newest orphan recovery and offers open,
  discard, or defer; a successful normal save removes the private file.
- Timer/manual autosave work that arrives between stroke begin and end remains
  queued while append/end/cancel packets overtake it, then runs after the live
  transaction closes. This preserves Core's save-during-stroke rejection while
  preventing the valid timer race from surfacing as an asynchronous error.
- The status worktree description was corrected from the stale claim that the
  already-committed initial M2 implementation was uncommitted.

## M1 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Create 1920 x 1080 cell and draw on main line | Verified | Core `acceptance_saved_drawing_vertical_slice`, FFI M1 test, Windows mouse smoke |
| 2 | Switch to color plane and draw while main line remains visible | Verified | Core verifies premultiplied BGRA color plus black main-line overlay in one snapshot; FFI checksum test and Windows plane-switch/D2D smoke |
| 3 | Color edit does not change main-line tile checksum | Verified | Core, Rust FFI, C++ ABI, and Windows smoke compare main checksum before/after |
| 4 | One stroke is one Undo/Redo unit | Verified | Core pixel test, FFI history test, Windows smoke |
| 5 | Save, discard, reopen preserves IDs, pixels, and frame metadata | Verified | Core/FFI round-trip plus Windows `SamePersistentMetadata` smoke |
| 6 | Pan/zoom does not change document revision | Verified | Core view test, Rust FFI test, Windows middle-pan/wheel smoke |
| 7 | Continuous drawing batches samples in order without per-sample snapshot calls | Verified | FFI accepts 256 extended records; adjacent append packets coalesce in the bounded Core queue without dropping samples; Windows records two multi-sample strokes while preview snapshots are frame-paced |
| 8 | Preview is visible before pointer-up without committed-state mutation; end is one Undo unit and cancel restores base | Verified | Core and FFI live-session tests; Windows presents a newer frame before button-up while revision/checksum/dirty stay fixed, then checks one-revision commit and capture-cancel equality |
| 9 | UI/Input, Core engine, and Renderer are distinct; DPI does not shift device-pixel bounds | Verified | Windows smoke compares three nonzero distinct thread IDs and exact Fit bounds `(16,69)-(624,411)` before/after simulated DPI change |

## M1 review fixes

- A newly created blank cell now starts clean even before it has a normal-save
  path. The first committed edit marks it dirty, Undo to the initial state clears
  dirty again, and the Windows title no longer shows `*` immediately at startup.
- Extreme or non-finite-result view transforms are rejected without changing
  document/view revision.
- Stroke coordinates, clipped segments, and rasterization work are bounded;
  a failed stage does not commit pixels, history, or a revision.
- A clean explicit Save rewrites/recreates its destination instead of returning
  early solely because the savepoint and path match.
- FFI diagnostic truncation preserves valid UTF-8, binary tile insertion
  enforces the binary invariant, and Canvas cancels a sample batch after
  allocation/count failure instead of forwarding a partial stroke.
- PAINT-001 tools and pressure, real nontrivial sample stride, failed-staging
  atomicity, missing-destination save, and all three Rust domain crate
  boundaries now have direct tests.
- Canvas no longer buffers a whole stroke until pointer-up. A dedicated Core
  engine owns the ABI handle, incremental preview snapshots are frame-paced,
  and a dedicated Renderer owns every D3D/DXGI/D2D/Present object.
- Snapshot ownership now moves directly into a renderer queue; stale pending
  frames are released while input samples remain ordered. D2D uses client
  device pixels at 96 DPI, eliminating the extra monitor-DPI scale that shifted
  and shrank the Canvas.

## Post-M8 semantic API refactoring

- The public C ABI is now version 2. Milestone-derived task and raster input names
  were replaced by `InkpodTask`, `InkpodTaskInfo`, `INKPOD_TASK_*`,
  `inkpod_task_*`, and `InkpodRasterSourceInput`; no compatibility aliases keep
  the old names exposed.
- Rust production modules now use `animation`, `vector`, `effects`, and `batch`;
  native format modules and public metadata types use document, light-table,
  vector, and adjustment terminology. `CellFile` fields follow the same naming.
  Core feature areas are further divided into model, operation, validation,
  geometry, codec, and I/O modules where those responsibilities differ.
- Existing `.inkpod` section magic bytes and flag bit values were preserved so
  the refactoring does not change on-disk compatibility. Only source identifiers,
  diagnostics, and API documentation changed.
- An architecture regression test rejects milestone-numbered production module
  files and public Rust/C FFI identifiers. The existing surface guard still
  requires exact header/export parity and a direct test reference for every C ABI
  function.
- `inkpod-core/src/lib.rs` was reduced from 9,042 lines to a 103-line module and
  public re-export index. Public `Core` methods and pure helpers now live in
  document, selection, transform, view, paint, stroke, history, persistence,
  snapshot, animation, vector, effects, and batch responsibility modules.
- `inkpod-image`, `inkpod-format`, and `inkpod-ffi` now follow the same pattern;
  their crate roots are 45, 49, and 56 lines. FFI ABI records, opaque handles,
  boundary converters, and feature exports are separate, while the header and
  all 155 exported symbol names/layouts remain unchanged.
- Shared internal state kept its prior effective crate scope; public Rust paths,
  C ABI values, and on-disk bytes did not change. Unit test bodies are stored
  under each crate's `tests/unit/`, with malformed corpus and resilience tests
  retained as integration targets under `tests/`.
- The architecture guard caps every crate root at 200 lines, rejects inline test
  bodies under `src/`, and verifies configure-aware recursive CMake tracking for
  all production Rust sources. The FFI parity test also scans `src/` recursively,
  so splitting an export cannot hide it from header drift detection.

## Verification

| Command | Platform | Result | Date |
|---|---|---|---|
| `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features` | Windows 11 x64, stable Rust | R3 controller/shell/pane extraction passed formatting, zero-warning clippy, and 133 tests: Core 64, architecture 5, resilience 1, FFI 19, format 20 plus malformed corpus 2, and image 22 | 2026-07-26 |
| Visual Studio 2026 developer shell Debug and Release configure/build; `ctest --preset windows-x64-{debug,release} -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64, MSVC 19.51 | All R3 translation units passed `/W4 /WX /permissive-`; application, ABI, assets, and package-payload smoke passed 4/4 in Debug and Release | 2026-07-26 |
| `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features` | Windows 11 x64, stable Rust | R2 state-ownership refactor passed formatting, zero-warning clippy, and 133 tests: Core 64, architecture 5, resilience 1, FFI 19, format 20 plus malformed corpus 2, and image 22 | 2026-07-26 |
| Visual Studio 2026 Developer PowerShell Debug and Release configure/build; `ctest --preset windows-x64-{debug,release} -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64, MSVC 19.51 | Private `AppContext` and all migrated call sites passed `/W4 /WX /permissive-`; final application/ABI/assets/package-payload tests passed 4/4 in Debug (15.21 s) and Release (4.21 s) | 2026-07-26 |
| `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features` | Windows 11 x64, stable Rust | Shared-ICO About cleanup passed formatting, zero-warning clippy, and 133 tests: Core 64, architecture 5, resilience 1, FFI 19, format 20 plus malformed corpus 2, and image 22 | 2026-07-26 |
| Developer-shell Debug and Release configure/build; non-elevated CTest excluding install smoke | Windows 11 x64, MSVC 19.51 | WIC-free About resource/C++ link and both MSIX assemblies passed; assets, integrated ABI, target-DPI About/application, and package payload passed 4/4 in Debug (17.65 s) and Release (4.12 s) | 2026-07-26 |
| Non-elevated `ctest --preset windows-x64-release -R m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64 | Stopped at the required administrator guard before certificate/package mutation; the earlier UAC cancellation was not re-prompted, so installed execution remains unverified | 2026-07-26 |
| `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features` | Windows 11 x64, stable Rust | R0/R1 source passed formatting, zero-warning clippy, and 133 tests: Core 64, architecture 5, resilience 1, FFI 19, format 20 plus malformed corpus 2, and image 22 | 2026-07-26 |
| Developer-shell Debug configure/build; `ctest --preset windows-x64-debug -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64, MSVC 19.51 | All extracted C++20 translation units, resources, Rust staticlib, link, and MSIX assembly passed; assets, integrated ABI, application/D2D, and package payload passed 4/4 in 15.11 s | 2026-07-26 |
| Developer-shell Release configure/build; `ctest --preset windows-x64-release -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict optimized build and MSIX assembly passed; assets, integrated ABI, application/D2D, and package payload passed 4/4 in 4.21 s | 2026-07-26 |
| Non-elevated then UAC-launched `ctest --preset windows-x64-release -R m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64 | Normal shell stopped at the required administrator guard; the `RunAs` launch was cancelled at UAC before package/certificate mutation, so the changed packaged executable still needs its installed ABI smoke rerun | 2026-07-26 |
| `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features` | Windows 11 x64, stable Rust | All-crate responsibility/test-layout refactor passed formatting, zero-warning clippy, and 133 tests: Core 64, architecture 5, resilience 1, FFI 19, format 20 plus malformed corpus 2, and image 22 | 2026-07-25 |
| Visual Studio Developer PowerShell `cmake --preset windows-x64-debug`; `cmake --build --preset windows-x64-debug`; `ctest --preset windows-x64-debug -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64, MSVC 19.51 | Recursive Rust source tracking rebuilt the staticlib after the final module moves; strict C11/C++20 link and MSIX assembly passed; assets, integrated ABI, application/D2D, and package payload passed 4/4 in 15.37 s | 2026-07-25 |
| `cargo fmt --all -- --check`; `cargo test --workspace --all-features`; WSL-isolated `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64 + WSL Ubuntu, stable Rust | Core responsibility refactor passed formatting, zero-warning clippy, Core 64, architecture 5, Core resilience 1, FFI 19, format unit 20 plus corpus 2, image 22, and doc-tests | 2026-07-25 |
| Developer PowerShell `cmake --build --preset windows-x64-debug`; `ctest --preset windows-x64-debug -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64, MSVC 19.51 | Refactored Core staticlib and unchanged ABI v2 passed strict C11/C++20/Rust build, MSIX assembly, assets, ABI, application/D2D, and payload tests 4/4 in 19.76 s | 2026-07-25 |
| `cargo fmt --all -- --check`; `cargo test --workspace --all-features`; WSL-isolated `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64 + WSL Ubuntu, stable Rust | ABI v2 semantic-name refactor passed formatting, all workspace tests, and zero-warning clippy. Native Windows clippy was blocked before startup by Application Control (`os error 4551`) | 2026-07-25 |
| Developer PowerShell `cmake --build --preset windows-x64-debug`; `ctest --preset windows-x64-debug -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict C11/C++20/Rust build and MSIX assembly passed; assets, ABI v2, application/D2D, and package-payload tests passed 4/4 in 18.62 s | 2026-07-25 |
| `cargo fmt --all -- --check`; `cargo test --workspace --all-features`; WSL `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64 + WSL Ubuntu, stable Rust 1.97.1 | Post-M8 FFI baseline passed formatting, zero-warning WSL clippy, Core 64, architecture 3, Core M8 integration 1, FFI 19, format unit 20 plus corpus 2, image 22, and doc-tests. Native Windows clippy frontend was blocked before startup by Application Control (`os error 4551`) | 2026-07-25 |
| Developer-shell Debug build / `ctest --preset windows-x64-debug -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict C11/C++20/Rust build and MSIX assembly passed; assets + integrated ABI + application/D2D + package payload passed 4/4 in 18.49 s | 2026-07-25 |
| Developer-shell Release build / non-elevated CTest | Windows 11 x64, MSVC 19.51 | Strict optimized compile/link and MSIX assembly passed; assets and package payload passed, but Application Control blocked the newly linked ABI/application EXE before startup on both attempts (`BAD_COMMAND`) | 2026-07-25 |
| `cargo fmt --all -- --check`; WSL clippy/all-feature tests; `cargo bench -p inkpod-image --bench large_document -- --quick` | Windows + WSL Ubuntu, stable Rust 1.97.1, isolated target | Review source passed formatting, zero-warning clippy, Core 64, architecture 3, Core M8 integration 1, FFI 15, format unit 20 plus corpus 2, image 22 and doc-tests; benchmark reported sparse 8 ms and dense 49 ms | 2026-07-25 |
| Debug/Release configure/build plus immediate unchanged verbose rebuild | Windows 11 Pro x64 build 26200, MSVC 19.51 | Both packages assembled with 10 app-local CRT DLLs; immediate Debug and Release rebuilds reported `ninja: no work to do`, so neither Cargo nor MakeAppx was reinvoked | 2026-07-25 |
| `ctest --preset windows-x64-{debug,release} -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 Pro x64 build 26200 | Corrected source passed assets + integrated ABI ownership + application/D2D + unpacked MSIX payload 4/4; Debug 15.21 s, Release 4.28 s | 2026-07-25 |
| Non-elevated then UAC-launched corrected-package install smoke | Windows 11 Pro x64 build 26200 | Normal shell stopped at the required administrator guard; the UAC launch was cancelled by the user before package/certificate mutation. Corrected artifact install/run/uninstall therefore remains unverified | 2026-07-25 |
| `cargo test --workspace m7_acceptance -- --nocapture` | WSL Ubuntu, stable Rust 1.97.1, isolated target | Reconfirmed the clean pre-M8 M7 boundary: all six acceptance scenarios passed 6/6. Windows compiled the suite but local Application Control blocked the fresh test EXE before startup (`os error 4551`) | 2026-07-25 |
| `cargo test --workspace m8_acceptance -- --nocapture` | WSL Ubuntu, stable Rust 1.97.1, isolated target | Passed 3/3: legacy codec status/scope, corrupted corpus, and whole-workspace Rust portability | 2026-07-25 |
| `cargo test -p inkpod-format mutation_fuzz_all_file_decoders_never_panics -- --nocapture` | WSL Ubuntu, stable Rust 1.97.1, isolated target | Passed deterministic truncation and bit-flip mutations for native, batch, PNG, TIFF, TGA, and BMP public decoders | 2026-07-25 |
| `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` | WSL Ubuntu, stable Rust 1.97.1, isolated target | Final M8 source passed formatting and clippy with zero warnings | 2026-07-25 |
| `cargo test --workspace --all-features` | WSL Ubuntu, stable Rust 1.97.1, isolated target | Final M8 source passed: Core 64, architecture 3, FFI 15, format unit 20 plus integration 2, image 22, and doc-tests | 2026-07-25 |
| `cargo bench -p inkpod-image --bench large_document -- --quick` | WSL Ubuntu, stable Rust 1.97.1, isolated target | Passed: 1,048,576-square sparse raster, 512 distributed tiles and COW isolation in 10 ms; 1024-square 4 MiB dense invert in 50 ms | 2026-07-25 |
| Developer-shell Debug build / `ctest --preset windows-x64-debug -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 Pro x64 build 26200, MSVC 19.51 | Strict build and MakeAppx package target passed; assets + integrated ABI + application/D2D smoke passed 3/3 in 16.25 s | 2026-07-25 |
| Developer-shell Release configure/build / `ctest --preset windows-x64-release -E m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 Pro x64 build 26200, MSVC 19.51 | Strict optimized build and MakeAppx package target passed; assets + integrated ABI + application/D2D smoke passed 3/3 in 5.10 s | 2026-07-25 |
| Elevated `ctest --preset windows-x64-release -R m8_windows_msix_install_uninstall_smoke --output-on-failure` | Windows 11 Pro x64 build 26200 | Exit 0 after enforcing workstation/build and clean all-users state, signing a private copy, installing, checking version/payload, running installed ABI smoke, uninstalling, and deleting the ephemeral root certificate/private-key/temp state | 2026-07-25 |
| `cargo test --workspace m6_acceptance -- --nocapture` | Windows 11 x64, stable Rust 1.97.1 | Reconfirmed the pre-M7 M6 boundary 5/5; repeated on final M7 worktree with the same result | 2026-07-25 |
| `cargo test --workspace m7_acceptance -- --nocapture` | Windows 11 x64, stable Rust 1.97.1 | Passed all six named M7 acceptance scenarios 6/6 | 2026-07-25 |
| `cargo fmt --all -- --check` | Windows 11 x64, stable Rust 1.97.1 | Passed on final M7 source | 2026-07-25 |
| `cargo test --workspace --all-features` | WSL Ubuntu, stable Rust 1.97.1, isolated target | Final review source passed: Core 64, architecture 1, FFI 15, format 20, image 22, plus doc-tests | 2026-07-25 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64 + WSL Ubuntu, stable Rust 1.97.1 | Exact Windows frontend remained blocked before startup by Application Control (`os error 4551`); the identical command passed with zero warnings in WSL | 2026-07-25 |
| Developer-shell `cmake --preset windows-x64-debug` / `cmake --build --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Final review source passed with `/W4 /WX /permissive-`; includes C11 header, C++20 ABI, resource, Rust staticlib, and native Batch palette | 2026-07-25 |
| `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64 | Reviewed GUI/ABI source passed assets + integrated C11/C++ M1-M7 ABI + real application/D2D smoke, including boundary-airbrush Batch dry-run, 3/3 in 18.17 s. The final Rust-only validation/save-poll refinements rebuilt strictly; three later attempts were blocked before both EXE test bodies by Application Control (`BAD_COMMAND`) | 2026-07-25 |
| Developer-shell Release configure/build / `ctest --preset windows-x64-release --output-on-failure` | Windows 11 x64, MSVC 19.51 | Final strict optimized build passed; assets + M1-M7 ABI + real application/D2D Batch smoke passed 3/3 in 4.26 s | 2026-07-25 |
| Immediate unchanged developer-shell `cmake --build --preset windows-x64-debug` | Windows 11 x64, Ninja | Final review source reported `ninja: no work to do`; Cargo was not invoked | 2026-07-25 |
| `cargo test --workspace m5_acceptance -- --nocapture` | WSL Ubuntu, stable Rust 1.97.1 | Passed 5/5 before M6 edits | 2026-07-22 |
| `cargo fmt --all --check` | WSL Ubuntu, stable Rust 1.97.1 | Passed on final M6 source | 2026-07-23 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | WSL Ubuntu, stable Rust 1.97.1 | Passed with zero warnings | 2026-07-23 |
| `cargo test --workspace --all-features` | WSL Ubuntu, stable Rust 1.97.1, isolated target | Passed: Core 50, architecture 1, FFI 13, format 17, image 22, doc-tests (103 unit/integration tests) | 2026-07-23 |
| Developer-shell `cmake --build --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Passed with `/W4 /WX /permissive-` on final M6 source | 2026-07-23 |
| `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64 | Passed on final source: assets + integrated C11/C++ M1-M6 ABI + continuous-airbrush/multiple-adjustment application/D2D smoke, 3/3 in 13.19 s | 2026-07-23 |
| `cmake --preset windows-x64-release` / developer-shell `cmake --build --preset windows-x64-release` | Windows 11 x64, MSVC 19.51 | Passed optimized with `/W4 /WX /permissive-` | 2026-07-23 |
| `ctest --preset windows-x64-release --output-on-failure` (two attempts) | Windows 11 x64 | Assets passed; ABI/application processes were not started because local Application Control blocked the newly linked unsigned Release EXE (`BAD_COMMAND`) | 2026-07-23 |
| Immediate unchanged `cmake --build --preset windows-x64-debug` | Windows 11 x64, Ninja | `ninja: no work to do`; Cargo was not invoked | 2026-07-23 |
| `winapp manifest update-assets AppIcon.svg --manifest apps/windows/package/Package.appxmanifest --verbose` | Windows 11 x64, Windows App Development CLI 0.5.0 | Passed: updated 48 PNG assets and 16/24/32/48/256 ICO; 16/44/256/300 px representatives inspected | 2026-07-22 |
| Post-icon `cmake --preset windows-x64-debug` / build / `ctest --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Passed strict build and assets + ABI + 574 x 544 owner-centered About/application smoke, 3/3 | 2026-07-22 |
| Post-icon Release configure/build | Windows 11 x64, MSVC 19.51 | Strict optimized build passed; asset CTest passed, while the two fresh-EXE tests were blocked before process start by local application-control policy | 2026-07-22 |
| Post-About-DPI-fix developer-shell `cmake --build --preset windows-x64-debug` / `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64, MSVC 19.51 | Passed strict resource/C++ build and target-DPI About size/centering/description/copyright smoke, 3/3 in 12.39 s | 2026-07-23 |
| Post-About-icon/font-fix developer-shell `cmake --build --preset windows-x64-debug` / `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict WIC/resource/C++ build and 88/256 px asset checks passed; two CTest attempts and one direct smoke launch were blocked before process start by local Application Control (`BAD_COMMAND`) | 2026-07-23 |
| Post-name-descender-fix developer-shell `cmake --build --preset windows-x64-debug` / asset CTest | Windows 11 x64, MSVC 19.51 | Updated `main.cpp` passed `/W4 /WX` compilation and assets passed 1/1; final link could not replace the user-running `inkpod.exe` (`LNK1168`) | 2026-07-23 |
| Final post-name-descender rebuild / `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict link passed after the app was closed; assets + ABI + 40 px name-label/application smoke passed 3/3 in 12.31 s | 2026-07-23 |
| `cargo fmt --all` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64, stable Rust | Passed on the M0-M6 GUI-completion source | 2026-07-24 |
| `cargo test --workspace --all-features -q` | WSL Ubuntu, stable Rust, same worktree | Passed: Core 54, architecture 1, FFI 13, format 17, image 22 (107 tests total), plus doc-tests | 2026-07-24 |
| Developer-shell `cmake --build --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Passed with `/W4 /WX /permissive-` after the GUI-completion changes | 2026-07-24 |
| `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64 | Passed: assets + integrated C11/C++ M1-M6 ABI + real menu/dialog/pane/shortcut/clipboard/timer/Canvas application smoke, 3/3 in 15.08 s | 2026-07-24 |
| Post-vector-applicability `cargo fmt --all --check` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64, stable Rust | Passed with zero warnings | 2026-07-24 |
| Post-vector-applicability `cargo test --workspace --all-features` | WSL Ubuntu, stable Rust 1.97.1, isolated target | Passed: Core 54, architecture 1, FFI 13, format 17, image 22 (107 total), plus doc-tests; the prior Windows attempt was blocked before test-body start by Application Control (`os error 4551`) | 2026-07-24 |
| Post-vector-applicability developer-shell Debug build / `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict build passed; after one transient newly-linked `BAD_COMMAND`, retry executed assets + ABI + initial-cell/vector Canvas application smoke 3/3 in 16.52 s | 2026-07-24 |
| Post-vector-applicability developer-shell Release build / `ctest --preset windows-x64-release --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict optimized build passed; assets + ABI + initial-cell/vector Canvas application smoke passed 3/3 in 4.31 s | 2026-07-24 |
| Post-closed-vector-seam developer-shell Release build / `ctest --preset windows-x64-release --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict `/W4 /WX /permissive-` optimized build passed; assets + ABI + application smoke, including the D2D closed-stroke seam probe, passed 3/3 in 4.49 s | 2026-07-24 |
| Post-closed-vector-seam `cargo fmt --all --check` / clippy / WSL workspace tests | Windows 11 x64 + WSL Ubuntu, stable Rust 1.97.1 | Format and zero-warning clippy passed; Core 54, architecture 1, FFI 13, format 17, image 22 (107 total), plus doc-tests passed | 2026-07-24 |
| Post-closed-vector-seam developer-shell Debug build / `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict `/W4 /WX /permissive-` build passed; assets + ABI + application/D2D closed-seam smoke passed 3/3 in 18.96 s | 2026-07-24 |
| Post-vector-corner developer-shell Release build / `ctest --preset windows-x64-release --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict `/W4 /WX /permissive-` optimized build passed; assets + ABI + D2D seam/corrected-corner application smoke passed 3/3 in 4.57 s | 2026-07-24 |
| Post-vector-corner `cargo fmt --all --check` / clippy / WSL workspace tests | Windows 11 x64 + WSL Ubuntu, stable Rust 1.97.1 | Format and zero-warning clippy passed; Core 54, architecture 1, FFI 13, format 17, image 22 (107 total), plus doc-tests passed | 2026-07-24 |
| Post-vector-corner developer-shell Debug build / `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64, MSVC 19.51 | Strict `/W4 /WX /permissive-` build passed; assets + ABI + D2D seam/corrected-corner application smoke passed 3/3 in 18.56 s | 2026-07-24 |

The reviewed M8 Rust suite passed in WSL, including the expanded architecture,
exact-path corpus, mutation, and failed-open preservation tests. Local
Application Control still blocks newly generated Windows Rust test/clippy
frontends intermittently, so the isolated WSL results are recorded. Corrected
Debug and Release CTest executed and passed 4/4, including the unpacked MSIX
payload. Windows Server 2022 hosted CI excludes only the workstation/elevation
install test. The pre-review package completed an elevated Windows 11
install/run/uninstall pass, but the corrected app-local-runtime artifact has not:
its UAC rerun was cancelled before mutation.

## Known gaps and unknowns

- All user operations grouped under M0-M7 are connected from production GUI
  entry points. There are no remaining M0-M7 `In progress` requirements.
- User-facing normal save/open still synchronously waits for its Core-engine
  work item. Autosave and M6 image processing use the existing asynchronous
  queue/task paths; adding progress UI to ordinary native save/open is outside
  the completed requirement IDs.
- Gap close uses the documented deterministic native axis-bridge rule. No
  proprietary legacy gap algorithm is inferred or claimed.
- Vector layers preserve their mutual z-order, but the current snapshot renderer
  draws vector content after the precomposited raster tiles rather than allowing
  arbitrary raster/vector interleaving. This known renderer limitation is
  recorded under `VECTOR-001`, which was already Verified and was not rebuilt.
- M6 filter/effect/dust work executes on the long-lived Core engine thread. The
  native progress dialog polls/cancels the thread-safe task; cancellation,
  failure, or stale revision never commits a partial raster.
- M7 batch output currently writes native `.inkpod` cells. The graph is
  versioned and extensible, but no undocumented legacy batch/preset format is
  accepted or emitted.
- M8's CMake-produced MSIX artifact is intentionally unsigned. The automated smoke
  signs only a private copy with an ephemeral test certificate; public release
  still requires the publisher's protected production credential and timestamp
  policy, which must not be committed to the repository. The corrected package
  now carries its app-local MSVC runtime and passes artifact inspection, but one
  elevated install/installed-ABI/uninstall rerun is still required.
- `.inkpod` v1 separates blobs but does not compress them.
- DGA/CEL and legacy preset layouts remain `Unknown` at the explicitly recorded
  zero-fixture/read/write/round-trip scope; no codec is enabled.
- Local MSVC is 19.51 from Visual Studio Build Tools 2026. VS2022 and VS2026 x64
  remain accepted Windows validation baselines.
