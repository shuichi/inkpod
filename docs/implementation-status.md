# Implementation status

## Current milestone

- Milestone: M5
- Status: Verified
- Last verified worktree state: reviewed M5 implementation is uncommitted. WSL stable
  Rust 1.97.1 formatting, clippy, named M5 tests, and full workspace tests passed
  on 2026-07-21. VS2026 x64 Debug/Release configure/build passed with strict
  warnings; both post-link Windows CTest presets passed assets, integrated
  C11/C++ M5 ABI, and real application/D2D smoke 3/3. Immediate unchanged
  rebuilds were no-ops.

Before M5 edits, the five named M4 Core tests covering all six M4 scenarios
passed 5/5 and the existing Debug CTest passed 3/3. `Verified` here means all
five M5 acceptance scenarios pass named automation, vector state persists with
validated topology, the C ABI and immutable snapshot are connected, the D2D
 renderer and Windows smoke execute in both configurations, and legacy M0-M4
Rust tests remain green. No M6 API or implementation was introduced.

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
| ARCH-001 | Verified | CMake explicitly tracks all image/format/core/FFI inputs, including M5 geometry/state/format sources, and Cargo byproducts behind a completion stamp | Debug/Release build; immediate repeat has no work | CMake remains the build entry |
| ARCH-002 | Verified | Core/image/format are safe and frontend-independent | All three domain crates' source/manifest scan, clippy, workspace tests | No Rust Windows dependency |
| ABI-001 | Verified | ABI v1 adds copied M5 cubic/fill/edit inputs, caller-owned selection/raster buffers, raster-vector conversion, and strict nested-span/count validation | C11/C++20 layouts and executed M1-M5 smoke; Rust short-record/count-query/conversion tests | Caller storage is never retained |
| ABI-002 | Verified | Immutable snapshot owns flat M5 cubic/fill/boundary spans alongside raster/overlay data; ownership remains with the renderer queue | Core zoom invariance; Rust FFI lifetime/count tests; compiled C++ validator/D2D smoke | Vector records remain document-coordinate and snapshot-borrowed |
| IO-001 (native save) | Verified | `.inkpod` v1 adds bounded optional M3/M4/M5 sections; `M5VT` stores bounded fixed-point cubic paths, widths, colors, and same-layer fill boundaries while retaining M1-M4 reads | Fill-topology save/reopen; malformed cross-layer/open/missing/out-of-coordinate topology, count, relation, and cross-M3/M4/M5 ID tests | Blob compression remains optional and disabled |
| IO-001 (M2 recovery) | Verified | Atomic autosave leaves the normal savepoint/path untouched; recovery opens dirty, recovered, and pathless; Windows gives never-saved cells a private recovery path, queues timer autosave after an active stroke, and discovers private recovery at startup | Core/FFI recovery tests plus Windows active-stroke autosave, private-path discovery, and normal-vs-recovery smoke | Only the newest private recovery is prompted per launch; defer leaves it intact |
| IO-002 | Verified | Bounded PNG/TIFF/TGA/BMP DTO/codecs preserve supported depth, alpha, dimensions, and DPI; TIFF declares straight alpha and decode bounds are checked before allocation | Round-trip, TIFF alpha-tag, explicit-white, oversized-dimension, and Core PNG sequence tests | PNG/TIFF support RGBA8/16; TGA/BMP RGBA8; TGA has no standard DPI |
| DOC-001 | Verified | Cell paper/DPI plus transactional 100/reference/drawing/safe frames and independent margins | M1 metadata round-trip plus M4 mixed-size reference-frame golden | Windows paper property dialog remains a UI gap |
| DOC-002 | Verified | Stable-ID typed tree adds vector-coloring with exactly one main-line, one-or-more color-trace, one fill, and optional raster planes | Core/format topology, invalid-combination, and save/reopen tests; Rust/C++ node queries | Vector geometry is separate from placeholder raster payloads |
| DOC-003 | Verified | Existing tree operations preserve, duplicate, remove, and reassign vector paths/fills/boundaries transactionally | Core M3 tree tests plus M5 vector duplicate/delete/merge paths | Full Windows panel remains UI work |
| HIST-001 | Verified | M5 draw/fill/erase/connect/width/vectorize operations join prior edits as atomic Undo/Redo transactions; invalid/no-op paths, including repeated connect, do not commit | Core M1-M5 history and `vector_002...transactional` tests | Interactive vector preview UI remains later adapter work |
| VIEW-001 | Verified | device-pixel zoom/box zoom/pan/fit/1:1 plus horizontal/vertical view flip use an independent view revision | Core mode/box/flip tests; FFI transform flags; Windows exact-bounds DPI and flip/mirror smoke | Windows exposes flip commands; box zoom is Core-connected for a later drag gesture |
| VIEW-002 | Verified | Ruler/guide/grid/snap/transparent-view state is Core-owned; snap obeys its enable flag, guides/grid persist, and snapshot overlay state drives renderer grid/guides/transparency | Core guide/grid/snap test; Rust/C++ overlay ABI and Windows grid/guide render smoke | Windows M3 exposes grid; remaining controls are ABI-ready |
| VIEW-003 | Verified | Locator reports document coordinates, selection bounds, and color per logical view; secondary views share one document and build independent immutable snapshots | Core locator/multi-view test; Rust/C++ ABI and Windows same-revision smoke | M3 creates logical secondary views; separate child-window layout is later UI work |
| PAINT-001 | Verified | pencil/brush/eraser, auto erase, pressure-size, clipped/bounded incremental staging; UI/Input -> Core queue uses mouse fallback and `WM_POINTER` history | Core tool/resource tests; 256-record FFI test; Windows multi-sample live stroke smoke | PAINT-001 scope complete |
| FILL-001 | Verified | Connected seed fill uses normalized 16-bit per-channel tolerance and the persistent M3 binary selection mask | Image golden/property cases; Core/FFI selection/transaction tests; Windows Canvas click smoke | Selection authoring is tracked under SEL-001/002 |
| FILL-002 | Verified | Specified/except-specified inclusion (max six), overflow abort with candidate coordinate, bounded cancel/work, axis gap-close, detached matching regions | Required inclusion/gap/overflow golden cases plus cancel/no-op tests | Native deterministic gap rule is documented; no proprietary algorithm is claimed |
| FILL-003 | Verified | Closed-region fill excludes escaping components, supports transparent or colored components plus transparent-only/inclusion, and fill extension spreads through a bounded mask/distance; all planners poll cancel | Image closed/open/colored/cancel golden and extension tests; Core/ABI operation validation | Windows M2 UI exposes seed fill; range gesture tools are later PAINT/selection UI scope |
| SEL-001 | Verified | Persistent bounded binary mask supports rectangle, ellipse, lasso, polygon/polyline, trace, and connected color-wand authoring | Core M3 authoring/property test; C ABI validation; Windows rectangle smoke | Full gesture tools are Core/C ABI in M3; Windows exposes select-all as the vertical slice |
| SEL-002 | Verified | New/add/subtract/intersect boolean algebra, invert, deterministic morphology expand/shrink, and typed color selection | Exhaustive 8-bit mask property table plus Core/Windows operation smoke | Selection changes are ordinary document history units |
| SEL-003 | Verified | Raster selection-layer conversion remains intact; vector cut/touch/contained/line/whole-line/intersection/fill-boundary/fill modes return deterministic ranges/IDs | `vector_002_all_selection_modes_have_deterministic_ranges_and_ids`; Rust/C++ caller-buffer ABI smoke | Native vector gesture UI is not yet exposed |
| PAINT-002 (M5 slice) | In progress | Cubic path input represents line/curve/shape/polyline geometry and commits atomically | Core path/Undo plus Rust/C++ ABI draw tests | Interactive preview and authoring UI remain incomplete |
| PAINT-003 (M5 slice) | In progress | Deterministic nearest-endpoint connect and add/subtract/scale/constant width correction are implemented | `vector_002_connect_width_select_and_raster_vector_conversion_are_transactional` | Dust removal is not implemented |
| VECTOR-001 | Verified | `inkpod-image` fixed-point cubic/variable-width geometry, Core main-line/color-trace/fill topology, trace-before-protected-main paint order, immutable vector snapshot, and D2D fill/outline rendering | Image geometry; Core zoom/save/order/golden; Rust FFI; compiled C11/C++20 renderer smoke | Arbitrary raster/vector interleaving still renders vector after precomposited raster tiles |
| VECTOR-002 | Verified | Transactional draw, partial/intersection/full erase, nearest connect without duplicate zero-gap connectors, four width modes, all vector selection modes, deterministic rasterize, and bounded RGBA8 run vectorization | Five M5 acceptance tests plus object-limit, all-mode, transactional operation, and FFI buffer/conversion tests | Windows authoring controls are not yet exposed |
| CLIP-001 (M3 typed clipboard) | Verified | Rust-owned typed clipboard retains absolute document coordinates, locates a compatible destination, and remains floating after a failed commit | Core different-paper/failure coordinate test; Rust/C++ ABI and Windows paste smoke | OS standard clipboard interchange is still incomplete and is not claimed Verified |
| XFORM-001 (M3 flip/mirror) | Verified | View flips change only view revision; destructive document mirror changes pixels/frames/guides and history | Core and Windows revision/history acceptance tests | Destructive rotate/size/resolution remain future scope |
| XFORM-002 | Verified | Floating typed selection uses bounded inverse nearest-neighbor translate/scale/rotation, preview state, one-unit commit, retry after failure, and exact cancel | Core coordinate/scale/rotate/cancel test; FFI lifecycle and Windows translated-paste smoke | Windows Copy/Paste commits the identity transform; transform editor UI is later work |
| SHORT-001 | Verified | Bounded shortcut map supports rebind, conflict replacement, resolve, and reset; the native hotkey dialog edits the bindings used by actual Undo/Redo/Copy/Paste key events | Core/FFI resolve tests plus real Windows editor/reset/key-resolution smoke | M3 editor intentionally exposes the four connected commands |
| COLOR-001 | Verified | Straight RGBA8/16 and grayscale 8/16 values plus selected/topmost/composite/light-table eyedropper sources; Color-mode reference sampling remains exact-depth and display conversion is explicit | M2 exact-depth tests plus M4 transformed RGBA16 Core/FFI sampling | Full color-editor UI is later scope |
| COLOR-002 (M4 slice) | Verified | Document palette remains exact/undoable; sequence-backed subpalette registers a reference cell and samples its exact-depth pixels | Palette tests plus M4 subpalette/motion/sequence acceptance test | Color-chart authoring UI remains broader product work |
| LT-001 | Verified | Stable-ID persisted sets support create/duplicate/delete/rename/reorder/active selection; items support add/update/remove/reorder, transform/color/mode/opacity, and global opacity | Set/item transaction test; 50% × 50% acceptance; native round-trip/malformed tests | Native Windows panel UI is not yet exposed |
| LT-002 | Verified | Reference-frame alignment, transformed color sampling, read-only fill boundary/color, source reload by validated item replacement, and dirty-safe edit-image swap retaining item display state | Full-tile mixed-size/save-reopen/swap golden; cancellation/read-only fill; Rust/C++ boundary-fill and dirty rejection | Reload is caller-driven; failed validation leaves prior item unchanged |
| SEQ-001 | Verified | Bounded natural-order cut/cell sequence supports gaps, previous/next, clean-only cell switch, deterministic 64px thumbnails, and common-raster sequence import/export | Natural-order/gap/thumbnail acceptance; dirty-switch Core/FFI/C++ smoke | Sequence is workspace/session state rather than duplicated into every cell file |
| SEQ-002 | Verified | Motion state validates 30/25/24/12/10/8 FPS, loop/step/pause and selection/light-table option flags, returning deterministic frame thumbnails | Core motion loop/pause test; Rust/C++ ABI start/step smoke | Interactive playback window/timer remains a Windows UI adapter gap |
| M0 Windows shell (Help/About) | Verified | Japanese Help command, native modal About, shared generated icon, CMake-derived version and EXE version resource | Release `inkpod_windows_smoke` creates and closes the dialog through `WM_COMMAND`; EXE resource inspection | macOS-inspired information hierarchy implemented with native Win32 behavior |
| M8 packaging assets | In progress | winapp CLI manifest, 48 scale/target-size PNGs, five-resolution ICO | `inkpod_windows_assets`; Release resource build | MSIX assembly, signing, and clean install/uninstall are not yet tested |

M5 is complete at the Core/image/format/C ABI and compiled Windows D2D-smoke
boundary. All acceptance behavior is covered by named Rust automation. Remaining
UI differences are listed below; they do not represent M6 progress.

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
| `cargo fmt --all -- --check` | WSL Ubuntu, stable Rust 1.97.1 | Passed on final M5 source | 2026-07-21 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | WSL Ubuntu, stable Rust 1.97.1 | Passed with zero warnings | 2026-07-21 |
| `cargo test --workspace --all-features` | WSL Ubuntu, stable Rust 1.97.1 | Passed: Core 41, architecture 1, FFI 11, format 16, image 14, doc-tests (83 total unit/integration tests) | 2026-07-21 |
| `cmake --preset windows-x64-debug` / developer-shell `cmake --build --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Passed with `/W4 /WX /permissive-` | 2026-07-21 |
| `ctest --preset windows-x64-debug --output-on-failure` | Windows 11 x64 | Passed after M5 review: assets + integrated C11/C++ M1-M5 ABI + application/D2D smoke including active-stroke autosave deferral, 3/3 in 8.41 s | 2026-07-21 |
| `cmake --preset windows-x64-release` / developer-shell `cmake --build --preset windows-x64-release` | Windows 11 x64, MSVC 19.51 | Passed optimized with `/W4 /WX /permissive-` | 2026-07-21 |
| `ctest --preset windows-x64-release --output-on-failure` | Windows 11 x64 | Passed after M5 review: assets + integrated C11/C++ M1-M5 ABI + application/D2D smoke including active-stroke autosave deferral, 3/3 in 1.83 s | 2026-07-21 |
| Immediate unchanged Debug and Release rebuild | Windows 11 x64, Ninja | Both reported `ninja: no work to do`; Cargo did not rerun | 2026-07-21 |
| `winapp manifest update-assets AppIcon.svg --manifest apps/windows/package/Package.appxmanifest --verbose` | Windows 11 x64, Windows App Development CLI 0.4.0 | Passed: 48 PNG assets and 16/24/32/48/256 ICO generated | 2026-07-20 |

The first Windows Cargo attempt was blocked before process start by local
application control (OS error 4551), so the complete Rust suite was recorded
under WSL. A first Debug CTest attempt exposed repeated flattening in 1920 x 1080
vector rasterization and exceeded 120 seconds; precomputed geometry/bounds and a
non-rendering size-query path corrected it. The final newly linked Debug/Release
Windows application/ABI tests above passed. A plain-PowerShell MSVC attempt that
lacked the Visual Studio SDK environment is not counted.

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
