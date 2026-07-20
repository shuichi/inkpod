# Implementation status

## Current milestone

- Milestone: M3
- Status: Verified
- Last verified commit/worktree state: M3 review fixes after `40d2603` are
  uncommitted. WSL stable Rust 1.97.1 acceptance/workspace tests and Windows 11
  stable Rust plus VS2026 x64 Debug/Release validation passed on 2026-07-20;
  both CTest presets passed 3/3 after local application-control evaluation.

`Verified` here means M2 was first re-run from its existing test evidence, then
all six M3 acceptance scenarios in `PROMPT.md` were covered by named Core tests,
the C ABI, and the real Windows application smoke path. M4 remains `Not started`.

## User-requested Windows shell and package additions

- The Japanese Help menu now exposes `Inkpodについて`. Its native, owned modal
  dialog uses a macOS-inspired centered hierarchy while preserving Windows DPI,
  keyboard, and modal-window behavior. It displays the generated app icon,
  product name, CMake-derived `0.1.0` version, and a short description.
- Windows App Development CLI 0.4.0 generated 48 MSIX PNG assets and one
  five-resolution ICO directly from `AppIcon.svg`. The ICO is also embedded in
  the EXE and used by the main window and About dialog.
- This is `In progress` M8 packaging preparation. No signed MSIX was produced,
  and clean-machine install/uninstall remains unverified.

## Requirements

| ID | Status | Implementation | Tests | Notes |
|---|---|---|---|---|
| ARCH-001 | Verified | CMake explicitly tracks image/format/core/FFI inputs and Cargo byproducts behind a completion stamp | Debug/Release build; immediate repeat has no work | CMake remains the build entry |
| ARCH-002 | Verified | Core/image/format are safe and frontend-independent | All three domain crates' source/manifest scan, clippy, workspace tests | No Rust Windows dependency |
| ABI-001 | Verified | ABI v1 exposes sized M3 typed-tree, selection, private clipboard/floating transform, guide/grid overlay, locator, multi-view, mirror, and shortcut resolve functions with Rust-owned opaque owner release | C11/C++20 integrated ABI smoke source; Rust lifecycle/negative/M1/M2/M3 FFI tests including short point records and repeat release | C11 object is linked into `inkpod.exe --abi-smoke-test`; caller spans remain borrowed |
| ABI-002 | Verified | Immutable raster snapshot owns premultiplied-BGRA tiles and guide spans, and copies grid/view/flip/color-check state; Core reuses unchanged buffers but assigns a new render revision after recomposition | Core/FFI primary/secondary/overlay snapshots and Windows upload/replacement/device-loss/flip/grid/guide/color-check smoke | Snapshot sink releases on enqueue failure, replacement, and shutdown |
| IO-001 (native save) | Verified | `.inkpod` v1 UUID/manifest/blob container adds a bounded optional M3 section for the typed layer/plane tree, properties, active IDs, selection mask, guides, and grid while retaining legacy M1/M2 reads | M3 layer history/save/reopen test; format legacy/exact-depth/malformed guide/grid/unreferenced-plane/cancel/replacement tests; Windows save/reopen tree smoke | Blob compression remains optional and disabled |
| IO-001 (M2 recovery) | Verified | Atomic autosave leaves the normal savepoint/path untouched; recovery opens dirty, recovered, and pathless; Windows gives never-saved cells a private recovery path, queues timer autosave, and discovers private recovery at startup | Core/FFI recovery tests plus Windows private-path discovery and normal-vs-recovery smoke | Only the newest private recovery is prompted per launch; defer leaves it intact |
| IO-002 | Not started | — | — | M4 |
| DOC-001 (M1) | Verified | Cell paper/DPI plus 100/reference/drawing/safe frames and margins | 1920 x 1080 create and metadata save/reopen equality | M1 default values are fixed |
| DOC-002 | Verified | Stable-ID typed layer/plane tree supports binary/grayscale coloring, raster, selection, frame, vanishing-point, adjustment, text, and annotation layers with validated plane/storage combinations | Core M3 save/reopen and invalid-combination tests; Rust/C++ ABI node queries | Non-raster semantic layer payloads are intentionally empty in M3 |
| DOC-003 | Verified | Transactional create/duplicate/delete/reorder, visibility/editability/opacity/name changes, binary/grayscale conversion, and compatible merge; index zero is top and editability guards pixel commands | Core M3 tree/order/lock tests plus ABI and real Layer-menu smoke | Full operations are Core/C ABI; Windows M3 menu exposes the acceptance vertical slice |
| HIST-001 | Verified | Pixel, metadata, typed-tree, selection, paste, guide/grid, and destructive mirror edits use atomic Undo/Redo transactions; failure retains floating state and cancel remains exact | Core M1-M3 history tests; Windows layer/paste/mirror Undo/Redo/save/reopen smoke | Dialog preview remains later scope |
| VIEW-001 | Verified | device-pixel zoom/box zoom/pan/fit/1:1 plus horizontal/vertical view flip use an independent view revision | Core mode/box/flip tests; FFI transform flags; Windows exact-bounds DPI and flip/mirror smoke | Windows exposes flip commands; box zoom is Core-connected for a later drag gesture |
| VIEW-002 | Verified | Ruler/guide/grid/snap/transparent-view state is Core-owned; snap obeys its enable flag, guides/grid persist, and snapshot overlay state drives renderer grid/guides/transparency | Core guide/grid/snap test; Rust/C++ overlay ABI and Windows grid/guide render smoke | Windows M3 exposes grid; remaining controls are ABI-ready |
| VIEW-003 | Verified | Locator reports document coordinates, selection bounds, and color per logical view; secondary views share one document and build independent immutable snapshots | Core locator/multi-view test; Rust/C++ ABI and Windows same-revision smoke | M3 creates logical secondary views; separate child-window layout is later UI work |
| PAINT-001 | Verified | pencil/brush/eraser, auto erase, pressure-size, clipped/bounded incremental staging; UI/Input -> Core queue uses mouse fallback and `WM_POINTER` history | Core tool/resource tests; 256-record FFI test; Windows multi-sample live stroke smoke | PAINT-001 scope complete |
| FILL-001 | Verified | Connected seed fill uses normalized 16-bit per-channel tolerance and the persistent M3 binary selection mask | Image golden/property cases; Core/FFI selection/transaction tests; Windows Canvas click smoke | Selection authoring is tracked under SEL-001/002 |
| FILL-002 | Verified | Specified/except-specified inclusion (max six), overflow abort with candidate coordinate, bounded cancel/work, axis gap-close, detached matching regions | Required inclusion/gap/overflow golden cases plus cancel/no-op tests | Native deterministic gap rule is documented; no proprietary algorithm is claimed |
| FILL-003 | Verified | Closed-region fill excludes escaping components, supports transparent or colored components plus transparent-only/inclusion, and fill extension spreads through a bounded mask/distance; all planners poll cancel | Image closed/open/colored/cancel golden and extension tests; Core/ABI operation validation | Windows M2 UI exposes seed fill; range gesture tools are later PAINT/selection UI scope |
| SEL-001 | Verified | Persistent bounded binary mask supports rectangle, ellipse, lasso, polygon/polyline, trace, and connected color-wand authoring | Core M3 authoring/property test; C ABI validation; Windows rectangle smoke | Full gesture tools are Core/C ABI in M3; Windows exposes select-all as the vertical slice |
| SEL-002 | Verified | New/add/subtract/intersect boolean algebra, invert, deterministic morphology expand/shrink, and typed color selection | Exhaustive 8-bit mask property table plus Core/Windows operation smoke | Selection changes are ordinary document history units |
| SEL-003 (selection layer) | Verified | Selection mask converts to/from a stable typed selection layer with replace/add/subtract | Core selection-layer round-trip and C ABI test | Vector-selection modes remain part of M5 vector work |
| CLIP-001 (M3 typed clipboard) | Verified | Rust-owned typed clipboard retains absolute document coordinates, locates a compatible destination, and remains floating after a failed commit | Core different-paper/failure coordinate test; Rust/C++ ABI and Windows paste smoke | OS standard clipboard interchange is still incomplete and is not claimed Verified |
| XFORM-001 (M3 flip/mirror) | Verified | View flips change only view revision; destructive document mirror changes pixels/frames/guides and history | Core and Windows revision/history acceptance tests | Destructive rotate/size/resolution remain future scope |
| XFORM-002 | Verified | Floating typed selection uses bounded inverse nearest-neighbor translate/scale/rotation, preview state, one-unit commit, retry after failure, and exact cancel | Core coordinate/scale/rotate/cancel test; FFI lifecycle and Windows translated-paste smoke | Windows Copy/Paste commits the identity transform; transform editor UI is later work |
| SHORT-001 | Verified | Bounded shortcut map supports rebind, conflict replacement, resolve, and reset; the native hotkey dialog edits the bindings used by actual Undo/Redo/Copy/Paste key events | Core/FFI resolve tests plus real Windows editor/reset/key-resolution smoke | M3 editor intentionally exposes the four connected commands |
| COLOR-001 (M2 scope) | Verified | Straight RGBA8/16 and grayscale 8/16 typed values, persisted exact-depth line base color and eyedropper sources, explicit 16-to-8 display conversion only | Grayscale coverage/base-color save-reopen and no-quantization goldens; format/FFI tests | Light-table source reports unavailable until M4 supplies items; full color-editor UI is later scope |
| COLOR-002 (M2 scope) | Verified | Bounded document palette is ABI-connected, undoable, and persisted with exact 8/16-bit entries; legacy-white/native-alpha checks carry background feature bits and force cache-safe tile revisions | Palette Core/ABI/format tests; Core/Windows view-only cache/feature/color-check tests | Windows M2 retains a single RGBA8 picker; palette editor, chart, and subpalette are later UI scope/M4 |
| M0 Windows shell (Help/About) | Verified | Japanese Help command, native modal About, shared generated icon, CMake-derived version and EXE version resource | Release `inkpod_windows_smoke` creates and closes the dialog through `WM_COMMAND`; EXE resource inspection | macOS-inspired information hierarchy implemented with native Win32 behavior |
| M8 packaging assets | In progress | winapp CLI manifest, 48 scale/target-size PNGs, five-resolution ICO | `inkpod_windows_assets`; Release resource build | MSIX assembly, signing, and clean install/uninstall are not yet tested |

M4 requirements remain `Not started`. No cut/cell sequence, light table, motion
check, common raster import/export, or M4 thumbnail workflow was added.

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

## M3 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Layer reorder/duplicate/delete survives Undo/Redo/save/reopen | Verified | Core `m3_acceptance_layer_tree_undo_redo_save_reopen_and_validation`; C++ ABI and real Layer-menu Windows smoke |
| 2 | Core rejects disallowed layer/plane combinations | Verified | Same Core test plus Rust/C++ ABI invalid selection-plane-in-coloring-layer checks; revision remains unchanged |
| 3 | Selection boolean passes a property test | Verified | Core `m3_acceptance_selection_boolean_property_and_authoring_tools` exhaustively checks all 256 left masks against representative right masks and every bit for new/add/subtract/intersect |
| 4 | Paste between different paper sizes preserves document-origin coordinates | Verified | Core `m3_acceptance_coordinate_preserving_typed_paste_and_floating_transform`; FFI and Windows 8x8 `(6,6)` to 4x4 translated `(2,2)` smoke |
| 5 | View flip and destructive mirror use separate history/revisions | Verified | Core `m3_acceptance_view_flip_and_destructive_mirror_have_separate_revisions`; snapshot flip flag and Windows document/view revision plus Undo checks |
| 6 | Editing one view appears at the same revision in another view's next snapshot | Verified | Core `m3_acceptance_multi_view_locator_guides_grid_and_shortcuts`; Rust/C++ ABI and Windows primary/secondary snapshot revision equality |

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
autosave without a UI wait, opens recovery as dirty/pathless, and then reopens
the unchanged normal file.

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
- The status worktree description was corrected from the stale claim that the
  already-committed initial M2 implementation was uncommitted.

## M1 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Create 1920 x 1080 cell and draw on main line | Verified | Core `m1_acceptance_saved_drawing_vertical_slice`, FFI M1 test, Windows mouse smoke |
| 2 | Switch to color plane and draw while main line remains visible | Verified | Core verifies premultiplied BGRA color plus black main-line overlay in one snapshot; FFI checksum test and Windows plane-switch/D2D smoke |
| 3 | Color edit does not change main-line tile checksum | Verified | Core, Rust FFI, C++ ABI, and Windows smoke compare main checksum before/after |
| 4 | One stroke is one Undo/Redo unit | Verified | Core pixel test, FFI history test, Windows smoke |
| 5 | Save, discard, reopen preserves IDs, pixels, and frame metadata | Verified | Core/FFI round-trip plus Windows `SamePersistentMetadata` smoke |
| 6 | Pan/zoom does not change document revision | Verified | Core view test, Rust FFI test, Windows middle-pan/wheel smoke |
| 7 | Continuous drawing batches samples in order without per-sample snapshot calls | Verified | FFI accepts 256 extended records; adjacent append packets coalesce in the bounded Core queue without dropping samples; Windows records two multi-sample strokes while preview snapshots are frame-paced |
| 8 | Preview is visible before pointer-up without committed-state mutation; end is one Undo unit and cancel restores base | Verified | Core and FFI live-session tests; Windows presents a newer frame before button-up while revision/checksum/dirty stay fixed, then checks one-revision commit and capture-cancel equality |
| 9 | UI/Input, Core engine, and Renderer are distinct; DPI does not shift device-pixel bounds | Verified | Windows smoke compares three nonzero distinct thread IDs and exact Fit bounds `(16,69)-(624,411)` before/after simulated DPI change |

## M1 review fixes

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

## Verification

| Command | Platform | Result | Date |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11 x64, stable Rust | Passed on reviewed M3 source | 2026-07-20 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64, stable Rust | Passed on reviewed M3 source | 2026-07-20 |
| `cargo test --workspace --all-features` | Windows 11 x64, stable Rust | Passed on reviewed M3 source: Core 25, architecture 1, FFI 8, format 7, image 13, doc-tests | 2026-07-20 |
| `cargo fmt --all -- --check` | WSL Ubuntu, stable Rust 1.97.1 | Passed | 2026-07-20 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | WSL Ubuntu, stable Rust 1.97.1 | Passed | 2026-07-20 |
| `cargo test --workspace --all-features` | WSL Ubuntu, stable Rust 1.97.1 | Passed on reviewed M3 source: Core 25, architecture 1, FFI 8, format 7, image 13, doc-tests | 2026-07-20 |
| `cmake --preset windows-x64-debug` / `cmake --build --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Passed with `/W4 /WX /permissive-` | 2026-07-20 |
| `ctest --preset windows-x64-debug` | Windows 11 x64 | Passed: assets + integrated C11/C++ M3 ABI + M1/M2/M3 Windows smoke, 3/3 | 2026-07-20 |
| `cmake --preset windows-x64-release` / `cmake --build --preset windows-x64-release` | Windows 11 x64, MSVC 19.51 | Passed optimized with `/W4 /WX /permissive-` | 2026-07-20 |
| `ctest --preset windows-x64-release` | Windows 11 x64 | Passed: assets + integrated C11/C++ M3 ABI + M1/M2/M3 Windows smoke, 3/3 | 2026-07-20 |
| Immediate unchanged Debug and Release rebuild | Windows 11 x64, Ninja | Both reported `ninja: no work to do`; Cargo did not rerun | 2026-07-20 |
| `winapp manifest update-assets AppIcon.svg --manifest apps/windows/package/Package.appxmanifest --verbose` | Windows 11 x64, Windows App Development CLI 0.4.0 | Passed: 48 PNG assets and 16/24/32/48/256 ICO generated | 2026-07-20 |

Application control initially held newly linked test executables before process
start. After evaluation, the same final source passed native Windows and WSL
workspace suites plus all Debug/Release CTest cases; no in-process failure
occurred.

## Known gaps and unknowns

- User-facing normal save/open still synchronously waits for its Core-engine
  work item and does not expose progress/cancellation UI. M2 autosave is queued
  asynchronously and recovery is complete; the remaining large-operation UI is
  retained as incomplete `IO-001` scope.
- The Windows M2 tool exposes seed fill. Closed-region fill and fill extension
  are verified through image/Core/ABI automation but do not yet have separate
  range-gesture UI tools.
- Gap close uses the documented deterministic native axis-bridge rule. No
  proprietary legacy gap algorithm is inferred or claimed.
- The light-table eyedropper source returns unavailable until M4 provides light
  table items; selected, topmost, and composite sources are implemented.
- The M3 Windows menu provides the acceptance vertical slice. Box-zoom drag,
  ruler/guide/snap/transparent-view controls, the floating-transform editor,
  and full typed-tree panel remain UI work; their Core state/operations and C ABI
  are present and tested.
- The app-private typed clipboard is complete for M3 acceptance. Publishing and
  importing standard Windows clipboard formats remains incomplete `CLIP-001`
  scope and is not reported as Verified.
- Destructive rotate, paper size, and resolution changes remain incomplete
  `XFORM-001` scope; M3 verifies only view flip versus destructive mirror and
  floating move/scale/rotate.
- `.inkpod` v1 separates blobs but does not compress them.
- DGA/CEL and legacy preset layouts remain `Unknown`; no codec is enabled.
- Local MSVC is 19.51 from Visual Studio Build Tools 2026. VS2022 and VS2026 x64
  remain accepted Windows validation baselines.
