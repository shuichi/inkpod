# Implementation status

## Current milestone

- Milestone: M6
- Status: Verified
- Last verified worktree state: M6 completion fixes and the startup dirty-state
  fix remain uncommitted. Windows stable Rust formatting and the 104-test
  all-feature workspace suite passed on 2026-07-23; WSL stable Rust 1.97.1
  all-target/all-feature clippy passed with warnings denied. VS2026 x64 Debug
  built with `/W4 /WX /permissive-`, and Debug CTest passed assets, integrated
  C11/C++ M1-M6 ABI, and the real application/D2D smoke 3/3. The application
  smoke checks a clean `無題 - inkpod` title before the first edit and dirty
  state after the committed stroke. The prior unchanged Release build passed,
  but Release was not rerun for this fix; local Application Control previously
  blocked its newly linked unsigned EXE before ABI/application tests could run.

Before M6 edits, `cargo test --workspace m5_acceptance -- --nocapture`
reconfirmed all five M5 acceptance scenarios 5/5. The M6 acceptance boundary is
Verified: filter previews remain separate until one-unit apply, non-destructive
adjustments persist with validated native metadata, effect and alpha operations
cross the C ABI, and all M0-M5 Rust tests remain green. Full pressure-aware
airbrush/blur/stamp gestures, native filter/effect/adjustment editors, three-mode
dust removal with preview, and worker progress/cancel are connected and tested.
No M7 API or implementation was introduced.

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
  five-resolution ICO directly from `AppIcon.svg`. The ICO is embedded for the
  main window and title bars; About uses the exact generated 88 px PNG at the
  144-DPI reference scale and a 256 px source above that scale.
- This is `In progress` M8 packaging preparation. No signed MSIX was produced,
  and clean-machine install/uninstall remains unverified.

## Requirements

| ID | Status | Implementation | Tests | Notes |
|---|---|---|---|---|
| ARCH-001 | Verified | CMake explicitly tracks all image/format/core/FFI inputs, including M6 image-edit/Core/native-format sources, and Cargo byproducts behind a completion stamp | Debug/Release build plus an immediate no-op Debug rebuild | CMake remains the build entry; Cargo does not run on the unchanged rebuild |
| ARCH-002 | Verified | Core/image/format are safe and frontend-independent | All three domain crates' source/manifest scan, clippy, workspace tests | No Rust Windows dependency |
| ABI-001 | Verified | ABI v1 adds copied M6 strided curve/filter and gesture records, preview lifecycle, adjustment create/update, gradient/airbrush/boundary/pressure-blur/stamp, dust remove/preview, alpha edit/gradient, and a Rust-owned thread-safe task while retaining prior ownership rules | C11/C++20 layouts and executed M1-M6 smoke; Rust short/packed/strided-record, task pre-cancel/query/double-release, span, ownership, and transaction tests | Caller spans are borrowed only for the owner-thread Core call; task create/query/cancel/release may run on any thread; the filter record remains 72 bytes and accepts packed stride 0 |
| ABI-002 | Verified | Immutable snapshot owns flat M5 cubic/fill/boundary spans alongside raster/overlay data; ownership remains with the renderer queue | Core zoom invariance; Rust FFI lifetime/count tests; compiled C++ validator/D2D smoke | Vector records remain document-coordinate and snapshot-borrowed |
| IO-001 (native save) | Verified | `.inkpod` v1 adds bounded optional M3-M6 sections; `M6AD` stores stable adjustment-layer IDs and validated brightness/contrast, curve, or levels parameters while retaining M1-M5 reads | Adjustment order/parameters/composite save-reopen; native round-trip plus missing/duplicate/wrong-layer/invalid-parameter rejection | Blob compression remains optional and disabled |
| IO-001 (M2 recovery) | Verified | Atomic autosave leaves the normal savepoint/path untouched; recovery opens dirty, recovered, and pathless; Windows gives never-saved cells a private recovery path, queues timer autosave after an active stroke, and discovers private recovery at startup | Core/FFI recovery tests plus Windows active-stroke autosave, private-path discovery, and normal-vs-recovery smoke | Only the newest private recovery is prompted per launch; defer leaves it intact |
| IO-002 | In progress | Bounded PNG/TIFF/TGA/BMP DTO/codecs preserve supported depth, alpha, dimensions, and DPI; TIFF declares straight alpha and decode bounds are checked before allocation | Round-trip, TIFF alpha-tag, explicit-white, oversized-dimension, and Core PNG sequence tests | Codec tests pass, but Windows Open/Import/Export still has no common-raster or sequence path |
| DOC-001 | In progress | Cell paper/DPI plus transactional 100/reference/drawing/safe frames and independent margins | M1 metadata round-trip plus M4 mixed-size reference-frame golden | Windows paper/frame property and creation UI is not implemented |
| DOC-002 | In progress | Stable-ID typed tree adds vector-coloring with exactly one main-line, one-or-more color-trace, one fill, and optional raster planes | Core/format topology, invalid-combination, and save/reopen tests; Rust/C++ node queries | The production UI only switches the fixed main-line/color planes; the typed layer/plane palette is absent |
| DOC-003 | In progress | Existing tree operations preserve, duplicate, remove, and reassign vector paths/fills/boundaries transactionally | Core M3 tree tests plus M5 vector duplicate/delete/merge paths | Windows exposes duplicate/delete/move-to-top only; create, arbitrary reorder, show/edit, opacity, convert, merge, and the full panel are missing |
| HIST-001 | In progress | A new blank cell starts at a clean in-memory savepoint independently of its path; M6 filter and dust preview updates recompute from their immutable base; cancel/failure commits nothing, while apply/last-filter/gesture/effect/adjustment edits commit one document history unit | New-cell clean/edit/Undo regression; title/dirty Rust/C++ ABI and Windows smoke; named cancel/apply/worker and legacy M1-M5 history tests | Single-step Undo/Redo, whole-document revert, and M6 preview are connected; multi-step history UI and partial layer/selection revert are missing |
| VIEW-001 | In progress | device-pixel zoom/box zoom/pan/fit/1:1 plus horizontal/vertical view flip use an independent view revision | Core mode/box/flip tests; FFI transform flags; Windows exact-bounds DPI and flip/mirror smoke | Zoom/pan/fit/1:1/flip are connected, but box-zoom drag and the specified numeric/slider controls are absent |
| VIEW-002 | In progress | Ruler/guide/grid/snap/transparent-view state is Core-owned; snap obeys its enable flag, guides/grid persist, and snapshot overlay state drives renderer grid/guides/transparency | Core guide/grid/snap test; Rust/C++ overlay ABI and Windows grid/guide render smoke | Only the grid command is exposed; ruler/guide manipulation, snap, and transparent-view controls are absent |
| VIEW-003 | In progress | Locator reports document coordinates, selection bounds, and color per logical view; secondary views share one document and build independent immutable snapshots | Core locator/multi-view test; Rust/C++ ABI and Windows same-revision smoke | Logical view creation exists, but no separate document-tab/child viewport or color-locator UI is exposed |
| PAINT-001 | Verified | pencil/brush/eraser, auto erase, pressure-size, clipped/bounded incremental staging; UI/Input -> Core queue uses mouse fallback and `WM_POINTER` history | Core tool/resource tests; 256-record FFI test; Windows multi-sample live stroke smoke | PAINT-001 scope complete |
| FILL-001 | In progress | Connected seed fill uses normalized 16-bit per-channel tolerance and the persistent M3 binary selection mask | Image golden/property cases; Core/FFI selection/transaction tests; Windows Canvas click smoke | Canvas seed fill is connected with fixed/default options; tolerance and the specified option UI are absent |
| FILL-002 | In progress | Specified/except-specified inclusion (max six), overflow abort with candidate coordinate, bounded cancel/work, axis gap-close, detached matching regions | Required inclusion/gap/overflow golden cases plus cancel/no-op tests | Inclusion colors, overflow policy, gap-close preview/value, and detached-region options have no production UI |
| FILL-003 | In progress | Closed-region fill excludes escaping components, supports transparent or colored components plus transparent-only/inclusion, and fill extension spreads through a bounded mask/distance; all planners poll cancel | Image closed/open/colored/cancel golden and extension tests; Core/ABI operation validation | Closed-region range gestures, transparent-only options, and fill-extension tools are not exposed in Windows |
| SEL-001 | In progress | Persistent bounded binary mask supports rectangle, ellipse, lasso, polygon/polyline, trace, and connected color-wand authoring | Core M3 authoring/property test; C ABI validation; limited Windows selection smoke | The production menu exposes select-all, but rect/ellipse/lasso/polyline/trace/wand Canvas tools are absent |
| SEL-002 | In progress | New/add/subtract/intersect boolean algebra, invert, deterministic morphology expand/shrink, and typed color selection | Exhaustive 8-bit mask property table plus Core/Windows operation smoke | Invert and fixed 1px expand/shrink are exposed; authoring modes, configurable widths, and typed color-selection commands are missing |
| SEL-003 | In progress | Raster selection-layer conversion remains intact; vector cut/touch/contained/line/whole-line/intersection/fill-boundary/fill modes return deterministic ranges/IDs | `vector_002_all_selection_modes_have_deterministic_ranges_and_ids`; Rust/C++ caller-buffer ABI smoke | Selection-layer conversion commands and native vector-selection gesture/mode UI are absent |
| PAINT-002 (M5 slice) | In progress | Cubic path input represents line/curve/shape/polyline geometry and commits atomically | Core path/Undo plus Rust/C++ ABI draw tests | Interactive preview and authoring UI remain incomplete |
| PAINT-003 | In progress | Deterministic nearest-endpoint connect, add/subtract/scale/constant width correction, and bounded three-mode dust removal over full plane or pen/rectangle/polyline/lasso regions are implemented | Vector transaction plus dust mode/region/preview/cancel/atomicity image, Core, ABI, and Windows tests | Dust removal is exposed; gap-connect and line-width-correction commands/gestures are not |
| VECTOR-001 | Verified | `inkpod-image` fixed-point cubic/variable-width geometry, Core main-line/color-trace/fill topology, trace-before-protected-main paint order, immutable vector snapshot, and D2D fill/outline rendering | Image geometry; Core zoom/save/order/golden; Rust FFI; compiled C11/C++20 renderer smoke | Arbitrary raster/vector interleaving still renders vector after precomposited raster tiles |
| VECTOR-002 | In progress | Transactional draw, partial/intersection/full erase, nearest connect without duplicate zero-gap connectors, four width modes, all vector selection modes, deterministic rasterize, and bounded RGBA8 run vectorization | Five M5 acceptance tests plus object-limit, all-mode, transactional operation, and FFI buffer/conversion tests | Windows vector draw/erase/connect/width/select/convert authoring controls are not exposed |
| FILTER-001 | Verified | Deterministic fixed sharpen/blur presets, bounded Gaussian/unsharp, channel invert, and alpha-independent auto contrast support RGBA8/16 and selection clipping; all catalog entries use native editor/preview/last-filter paths | Exact 8/16-bit alpha/selection golden; catalog/image tests; Core/FFI/Windows editor smoke; task progress/cancel | Work runs on the Core engine worker while the UI polls/cancels a thread-safe task |
| FILTER-002 | Verified | Brightness/contrast, RGB/R/G/B Bezier/B-spline curves, levels, HSV, and color balance use documented normalized 16-bit clamp/rounding; native editors expose channel, interpolation, parameters, and curve points | Catalog validation, preview/Undo acceptance, C ABI validation, full workspace and Windows editor tests | Unknown legacy preset byte layouts are not fabricated |
| EFFECT-001 | Verified | Linear/radial 3–16-stop alpha gradients with dither/45-degree constraint, pressure/fade/spacing/continuous airbrush, boundary-only effect, pen/rectangle/polyline/lasso blur with screen-fixed pressure diameter, and round/square pressure-sensitive offset stamp gestures are connected through Core/C ABI/native Canvas | Boundary acceptance; deterministic gesture/pressure and extreme-coordinate tests; Rust/C++ ABI and Windows editor/gesture smoke | Native algorithms are documented Inkpod semantics, not inferred proprietary kernels |
| ADJUST-001 | Verified | Stable-ID non-destructive brightness/contrast, levels, and curve layers compose in palette order with visibility/opacity, persist in `M6AD`, and are exposed as multiple selectable/re-editable/reorderable native entries; alpha row edit/gradient and grayscale view preserve RGB | Order/source/opacity/visibility/save-reopen; malformed metadata; alpha RGB-preservation; Rust/C++ ABI and multiple-adjustment Windows smoke | — |
| CLIP-001 | In progress | Rust-owned typed clipboard retains absolute document coordinates, locates a compatible destination, and remains floating after a failed commit | Core different-paper/failure coordinate test; Rust/C++ ABI and Windows private-paste smoke | Private Copy/Paste is connected; cut, destination/converted paste choices, transform UI, and standard Windows clipboard interchange are incomplete |
| XFORM-001 | In progress | View flips change only view revision; destructive horizontal document mirror changes pixels/frames/guides and history | Core and Windows revision/history acceptance tests | Destructive vertical mirror, 90-degree rotate, image/paper size, and resolution UI are missing |
| XFORM-002 | In progress | Floating typed selection uses bounded inverse nearest-neighbor translate/scale/rotation, preview state, one-unit commit, retry after failure, and exact cancel | Core coordinate/scale/rotate/cancel test; FFI lifecycle and Windows translated-paste smoke | Windows paste uses an identity/default transform; dialog and handle-drag move/scale/rotate preview UI are absent |
| SHORT-001 | In progress | Bounded shortcut map supports rebind, conflict replacement, resolve, and reset; the native hotkey dialog edits bindings used by actual key events | Core/FFI resolve tests plus real Windows editor/reset/key-resolution smoke | The editor exposes only Undo/Redo/Copy/Paste, not the required categorized menu/tool/other command set |
| COLOR-001 | In progress | Straight RGBA8/16 and grayscale 8/16 values plus selected/topmost/composite/light-table eyedropper sources; Color-mode reference sampling remains exact-depth and display conversion is explicit | M2 exact-depth tests plus M4 transformed RGBA16 Core/FFI sampling | Windows exposes a basic color chooser and Canvas eyedropper; RGBA16, alpha, RGB/HSV, and eyedropper-source controls are absent |
| COLOR-002 | In progress | Document palette remains exact/undoable; sequence-backed subpalette registers a reference cell and samples its exact-depth pixels; color-check display is connected | Palette tests plus M4 subpalette/motion/sequence acceptance and Windows color-check smoke | Palette, chart, and subpalette authoring/view panels and related shortcuts are absent |
| LT-001 | In progress | Stable-ID persisted sets support create/duplicate/delete/rename/reorder/active selection; items support add/update/remove/reorder, transform/color/mode/opacity, and global opacity | Set/item transaction test; 50% × 50% acceptance; native round-trip/malformed tests | No production Windows light-table panel or set/item editing controls are exposed |
| LT-002 | In progress | Reference-frame alignment, transformed color sampling, read-only fill boundary/color, source reload by validated item replacement, and dirty-safe edit-image swap retaining item display state | Full-tile mixed-size/save-reopen/swap golden; cancellation/read-only fill; Rust/C++ boundary-fill and dirty rejection | Sampling/fill/swap are exercised by ABI smoke only; reload, movement, and edit-image-swap UI are absent |
| SEQ-001 | In progress | Bounded natural-order cut/cell sequence supports gaps, previous/next, clean-only cell switch, deterministic 64px thumbnails, and common-raster sequence import/export | Natural-order/gap/thumbnail acceptance; dirty-switch Core/FFI/C++ smoke | No production previous/next/number/file-preview/thumbnail or sequence import/export UI is exposed |
| SEQ-002 | In progress | Motion state validates 30/25/24/12/10/8 FPS, loop/step/pause and selection/light-table option flags, returning deterministic frame thumbnails | Core motion loop/pause test; Rust/C++ ABI start/step smoke | Motion calls are smoke-only; the timed playback window, controls, shortcuts, and options UI are absent |
| BATCH-001 | Not started | — | — | No batch palette or persisted Input -> Operations -> Output graph |
| BATCH-002 | Not started | — | — | No batch operation catalog or production UI |
| BATCH-003 | Not started | — | — | No dry-run, preview, progress/cancel, atomic output, or failure-report workflow |
| M0 Windows shell (Help/About) | Verified | Japanese Help command, reference-DPI-normalized owned modal About, reference-matched spacing, 15/9-point fonts with a 40 px reference-height name label, exact-size generated PNG icon, shorter description, copyright, and CMake-derived version | Debug `inkpod_windows_smoke` verifies exact target-DPI size/origin/spacing/name-label height/font/icon/string and non-overlap; final assets + ABI + application CTest passed 3/3 | The 574 x 544 reference is device pixels captured at 144 DPI and is scaled exactly once; native Win32 theme and keyboard/modal behavior are retained |
| M8 packaging assets | In progress | winapp CLI manifest, 48 scale/target-size PNGs, five-resolution ICO | `inkpod_windows_assets`; Release resource build | MSIX assembly, signing, and clean install/uninstall are not yet tested |

### GUI vertical-slice audit (2026-07-23)

For user-invoked requirements, `Verified` now requires a production menu,
dialog, toolbar, or Canvas gesture to reach the Windows adapter, C ABI, Core,
and visible/document result, with a Windows test covering that route. Direct C
ABI calls made only inside the application smoke test do not count as a GUI
vertical slice. If one user-visible operation grouped under a requirement ID is
still unavailable, the requirement remains `In progress`; completed Core/ABI
subsets stay recorded in the implementation and test columns. Internal build,
ABI, and renderer contracts do not require an artificial GUI entry point.

The five M6 acceptance scenarios and the complete M6 native vertical slices are
Verified at image/Core/native-format/C ABI/Windows D2D boundaries. M7 remains
untouched.

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
| Named M4 Core acceptance tests | Verified | The five `m4_acceptance_*` library tests passed 5/5 and cover all six M4 scenarios; the subsequently enumerated, unrelated Windows architecture test executable was blocked before start by local Application Control |
| Existing Windows M1-M4 boundary | Verified | `ctest --preset windows-x64-debug --output-on-failure` passed assets, integrated C11/C++ ABI, and application smoke 3/3 before relinking M5 |
| Worktree baseline | Verified | `git status --short` was clean before M5 implementation |

## M5 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Zoom does not change Core vector geometry | Verified | `m5_acceptance_zoom_never_changes_core_vector_geometry` compares every stored cubic point/width before and after view zoom and verifies only the view revision changes |
| 2 | Partial erase does not change another stroke | Verified | `m5_acceptance_partial_erase_changes_only_the_touched_stroke` checks untouched path ID/geometry byte-for-byte and verifies one atomic Undo restores the erased path |
| 3 | Intersection erase cut points are deterministic | Verified | `m5_acceptance_intersection_erase_cut_points_are_deterministic` runs equivalent documents twice, compares fixed-point pieces exactly, and fixes the crossing cut at the same parameter |
| 4 | Fill topology survives save/reopen | Verified | `m5_acceptance_fill_topology_survives_save_and_reopen` persists stable path/fill/plane IDs and ordered boundary IDs through `.inkpod`; format malformed-topology tests reject cross-layer/open/missing relations |
| 5 | Rasterize antialias, pixel center, and scale are golden-fixed | Verified | `m5_acceptance_rasterize_antialias_pixel_center_and_scale_golden` compares complete straight-RGBA buffers for center-sampled 1x, 4x4-AA 1x, and scaled 2x output |

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

## M5 re-verification before M6

| Criterion | Status | Evidence on 2026-07-22 before M6 edits |
|---|---|---|
| Named M5 acceptance tests | Verified | `cargo test --workspace m5_acceptance -- --nocapture` passed 5/5: zoom invariance, partial erase isolation, deterministic intersection cuts, fill-topology reopen, and rasterization golden |
| Existing recorded Windows M1-M5 boundary | Verified | The reviewed M5 status already recorded Debug and Release CTest 3/3 with integrated ABI and real application/D2D smoke; the M6 run subsequently retained all prior smoke paths |
| Worktree baseline | Verified | Windows `git status --short` was clean before the M6 implementation |

## M6 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Cancel restores the original tile checksum | Verified | `m6_acceptance_cancel_restores_the_original_tile_checksum` proves preview snapshot divergence without committed revision/history change, then exact checksum restoration; Rust/C++ ABI and real Windows smoke repeat the check |
| 2 | Apply is one Undo unit | Verified | `m6_acceptance_apply_is_exactly_one_undo_unit_and_last_filter_reuses_it` checks one history entry, exact Undo/Redo, and last-filter reuse; C++/Windows smoke applies then restores with one Undo |
| 3 | Adjustment layer leaves the source unchanged and reorder changes composite | Verified | `m6_acceptance_adjustment_order_changes_composite_without_changing_source_plane` compares source checksums, changes layer order, observes a different BGRA composite, verifies opacity/visibility, and verifies the same order/parameters/composite after save/reopen |
| 4 | 8/16-bit, alpha edge, and selection edge are golden-fixed | Verified | `m6_acceptance_eight_sixteen_bit_alpha_and_selection_edges_are_golden_fixed` fixes full-depth selected inversion results, unchanged outside-edge pixels, and exact alpha preservation for RGBA8/RGBA16 |
| 5 | Boundary airbrush does not blur uniform regions | Verified | `m6_acceptance_boundary_airbrush_preserves_uniform_regions` checks exact far-field pixels and changed boundary-band pixels; a second image test covers the same rule on a 2D raster |

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
| Filter work had no progress/cancel ownership model | Added Rust-owned atomic `InkpodM6Task`; Windows creates it on UI, runs the Core call on the Core engine thread, polls/cancels from UI, posts completion only after snapshot publication, and releases exactly once after the worker returns |
| Native adjustment controls tracked only one layer | Added multiple create with unique names, previous/next selection, selected-layer parameter reload/re-edit, visibility toggle, and reorder; integrated Windows smoke executes the complete sequence |

## M4 acceptance scenarios

| # | Scenario | Status | Evidence |
|---|---|---|---|
| 1 | Different-size cells align by reference frame | Verified | Core `m4_acceptance_reference_frame_aligns_different_cell_sizes_and_reopens` aligns 4x4 `(2,2)` to 8x8 `(4,4)`, compares the complete 8x8 BGRA tile golden, then checks `.inkpod` reopen and edit-image swap |
| 2 | Individual 50% × global 50% = effective 25% | Verified | Core `m4_acceptance_individual_and_global_opacity_multiply_to_twenty_five_percent`; Rust/C++ ABI checks effective alpha 64 |
| 3 | Light-table fill boundary never changes the reference | Verified | Core `m4_acceptance_light_table_fill_boundary_is_read_only` checks cancellation/fill/Undo exactness; Rust and C++ ABI invoke the new read-only boundary flag and re-sample the unchanged source |
| 4 | Previous/next does not silently discard an unsaved cell | Verified | Core `m4_acceptance_sequence_switch_rejects_unsaved_document_without_discarding_it` checks explicit error plus unchanged UUID/revision/dirty, then clean switch; Rust/C++ ABI status 12 |
| 5 | Sequence gaps and natural ordering are correct | Verified | Core `m4_acceptance_sequence_gaps_natural_order_thumbnails_subpalette_and_motion` sorts 1,3,10 without synthesizing missing cells and covers thumbnails/motion/subpalette/PNG sequence export-import |
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

## Verification

| Command | Platform | Result | Date |
|---|---|---|---|
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

The complete Rust suite is recorded under WSL because Windows Application
Control blocked the freshly linked Rust test/clippy executables before their
bodies could start (`os error 4551`). The same source passed format, clippy, and
all 103 tests under WSL using an isolated target directory. This policy event is
not a code-test failure. Final Debug CTest executed all three tests successfully.
The final Release binary compiled and linked strictly, but its two EXE tests
remained externally blocked before process start; no Release test body failed.

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
- The M4 acceptance slice is connected through Core/C ABI and automated
  Windows smoke: reference add/sample/opacity/fill/swap, sequence set/step, and
  motion start/step. Full set/item administration, paper properties,
  folder-preview/common-codec file flows, dedicated thumbnail/subpalette ABI,
  and playback timing/window controls remain Windows adapter work; their
  absence is not reported as M5 work.
- M5 vector authoring/editing is complete in Core and C ABI, and immutable
  vector snapshot rendering is implemented in the D2D Renderer. Native toolbar/
  Canvas gesture controls are not yet exposed. Arbitrary interleaving of raster
  and vector layers currently draws vector content after the precomposited
  raster tiles, while ordering among vector layers is preserved.
- M6 filter/effect/dust work executes on the long-lived Core engine thread, not
  the UI thread. The native progress dialog polls/cancels the thread-safe task;
  cancellation, failure, or stale revision never commits a partial raster.
- M7 batch graph, execution, dry-run, progress, and output policy remain
  `Not started`; no M7 code or status was introduced in this milestone.
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
