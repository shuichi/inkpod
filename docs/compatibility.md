# Compatibility status

Compatibility means operation semantics, data separation, coordinates, and
saved results—not replication of a legacy UI or assets.

| Requirement | Status | Implementation | Tests | Known difference / next work |
|---|---|---|---|---|
| ARCH-001 | Verified | CMake completion stamp lists every Rust source, including M7 batch/Core/settings/FFI modules, and connects Cargo `inkpod-ffi` staticlib/rlib byproducts to MSVC targets | VS2026 x64 Debug/Release build plus immediate no-op Debug rebuild | Cargo is not invoked when tracked inputs are unchanged; VS2022 CI remains configured |
| ARCH-002 | Verified | Safe, OS-independent Core/image/format crates with no frontend dependency | All three domain crates' source/manifest scan, clippy, workspace tests | No Rust Windows API dependency |
| ABI-001 | Verified | ABI v1 retains M0-M6 records and exposes copied, bounded M7 graph/operation records plus immutable graph/preview/report and atomic task handles | C11/C++20 layouts; executed M1-M7 ABI/application smoke; Rust short/oversized-stride/misaligned-nested-record, task, report, ownership, and transaction tests | Nested records are validated before reference creation; preview/report strings are borrowed from their owning handle; graph creation copies every caller span |
| ABI-002 | Verified | Immutable snapshot owns batched raster/overlay data plus flat cubic segment, fill, and boundary-ID spans; ownership moves unchanged through the renderer queue | Core zoom invariance; Rust FFI vector ownership; C++ validation and D2D smoke source | Vector records use document coordinates and are borrowed only for snapshot lifetime |
| IO-001 (native save) | Verified | `.inkpod` v1 adds bounded M3-M6 sections; `M6AD` persists stable adjustment-layer IDs plus brightness/contrast, curve, or levels parameters while retaining M1-M5 readers | Adjustment order/parameter/composite save-reopen; native missing/duplicate/wrong-layer/invalid-parameter rejection | Blob compression is optional and not enabled |
| IO-001 (M2 recovery) | Verified | Atomic autosave preserves the normal path/savepoint; recovery opens dirty and pathless; Windows assigns never-saved cells a private recovery path, queues timer/manual autosaves, defers them across an active stroke, and discovers private recovery at startup | Core/FFI recovery tests plus Windows active-stroke autosave, private-path discovery, and normal-file/recovery checksum smoke | Only the newest private recovery is prompted per launch; deferred files remain available for a later launch |
| IO-002 | Verified | PNG/TIFF/TGA/BMP preserve supported straight-alpha depth, dimensions, and DPI and are available from native Open/Import/Export dialogs with alpha/white export plus sequence import/export | Four-format round-trip/edge cases and Windows raster/sequence file-menu smoke | DGA/CEL remains `Unknown` pending rights-cleared fixtures |
| DOC-001 | Verified | CellDocument paper/DPI, 100/reference/drawing/safe frames, margins, image size, and resolution are editable through native creation/property dialogs | Metadata equality, mixed-paper alignment, native round-trip, and Windows dialog smoke | — |
| DOC-002 | Verified | Stable-ID typed raster/vector-coloring layer/plane tree is displayed and activated through separate native layer and plane panes | Core/format topology/save-reopen, ABI node queries, and Windows pane-selection smoke | — |
| DOC-003 | Verified | Native menus/panes expose create, duplicate, delete, drag/button reorder, show, edit, opacity, convert, merge, and hidden cleanup with transactional relationship preservation | Core raster/vector tree tests and Windows command plus real listbox-drag smoke | — |
| HIST-001 | Verified | Savepoint-aware transactions, Undo/Redo, whole revert, multi-step history selection, partial layer/selection revert, and exact preview cancel/apply are native workflows | Core history/preview regression and Windows history-dialog/partial-revert/title/dirty smoke | — |
| VIEW-001 | Verified | Anchored zoom, Canvas box zoom/pan, fit, 1:1, numeric/slider zoom, viewport resize, and both view flips use independent view revision | Core mode/box/resize/flip, FFI transform, and Windows gesture/dialog/DPI smoke | — |
| VIEW-002 | Verified | Ruler, guide add/move/delete, grid settings, snap, and transparent view are Core-owned native controls rendered from snapshot overlays | Core persistence/snap, C ABI overlay, and Windows menu/dialog/render smoke | — |
| VIEW-003 | Verified | Same-document tabs provide independent logical views; a locator pane reports document coordinates, selection bounds, and color | Core locator/multi-view, C ABI same-revision, and Windows tab/locator smoke | — |
| PAINT-001 | Verified | UI/Input, Core engine, and Renderer are distinct; pencil/brush/eraser/auto-erase/pressure samples stream through a bounded queue and preview at frame cadence | Core tool/resource tests, 256-record FFI test, Windows thread-ID/multi-sample/live-preview smoke | M2 UI retains a single brush diameter and RGBA8 color picker |
| FILL-001 | Verified | Connected seed fill exposes tolerance and selection clipping through its native option dialog and Canvas tool | Image goldens, Core/FFI transactions, and Windows dialog/Canvas-click smoke | — |
| FILL-002 | Verified | Inclusion/exclusion colors, overflow abort/reporting, gap-close axes/value, and detached regions are native fill options | Required inclusion/gap/overflow/cancel/no-op goldens and Windows option routing | Gap close is the documented native deterministic rule |
| FILL-003 | Verified | Closed-region range gestures, transparent-only/inclusion behavior, and fill extension are available from native Canvas tools | Closed/open/colored/transparent-only/cancel/extension tests plus Windows gesture smoke | — |
| SEL-001 | Verified | Rectangle, ellipse, lasso, polyline, trace, and wand are persistent-mask Canvas tools | Core authoring/property, C ABI validation, and Windows all-tool gesture smoke | — |
| SEL-002 | Verified | New/add/subtract/intersect, invert, configurable expand/shrink, and equal/different typed color selection are native operations | Exhaustive mask algebra plus Core/Windows mode/width/color smoke | — |
| SEL-003 | Verified | Raster selection-layer conversions and all eight vector selection modes are exposed by native menus/tools | Deterministic all-mode Core test, ABI count-query buffers, and Windows conversion/mode smoke | — |
| PAINT-002 (M5 slice) | Verified | Line, curve, rectangle/ellipse, and polyline Canvas tools render an immutable geometry preview and commit atomically | Core path/Undo, C ABI draw, renderer preview, and Windows gesture smoke | — |
| PAINT-003 | Verified | Gap connect, three-mode dust removal, and four line-width corrections have native command/dialog/gesture paths | Vector transactions, dust preview/cancel/atomicity, ABI, and Windows smoke | — |
| VECTOR-001 | Verified | Fixed-point cubic paths, variable widths, vector main-line/color-trace/fill topology, trace-before-main paint order, immutable snapshot, and Direct2D path/outline renderer with continuous seams and width-preserving bounded-miter joins | Image geometry test; Core zoom/order/golden/save tests; Rust FFI + compiled C11/C++20 renderer and closed-seam/corner geometry smoke | Arbitrary raster/vector layer interleaving still composites vector content after raster tiles |
| VECTOR-002 | Verified | Native tools cover draw, partial/intersection/full erase, connect, four width modes, all selection modes, vector-to-new-raster-layer rasterize, and raster-to-vector conversion; draw tools are enabled only for vector main-line/color-trace planes | Five M5 acceptance tests, bounds/all-mode/transactions, FFI conversion, Windows RunM5 command/Canvas smoke, and initial-cell command-state/diagnostic regression | Rasterize preserves the source vector layer and commits one Undo unit |
| FILTER-001 | Verified | Fixed sharpen/blur, bounded Gaussian/unsharp, channel invert, and alpha-independent auto contrast support RGBA8/16 with selection clipping; every catalog entry is exposed through a native editor/preview path | Exact 8/16-bit alpha/selection golden; catalog/image tests; Core/FFI/Windows menu smoke; task progress/cancel | Filter work runs on the Core engine worker; the UI polls a thread-safe task and never blocks the UI thread |
| FILTER-002 | Verified | Brightness/contrast, channel Bezier/B-spline curves, levels, HSV, and color balance use normalized 16-bit clamping and one final round; native editors expose parameters, channels, interpolation, and editable curve points | Preview/Undo acceptance, ABI validation, all-feature workspace regression, Windows editor smoke | Legacy preset byte layouts remain Unknown and are not fabricated |
| EFFECT-001 | Verified | Linear/radial 3–16-stop gradient with alpha/dither/45-degree constraint, pressure/fade/spacing/continuous airbrush gestures, boundary-only airbrush, screen-fixed pressure-sensitive blur regions, and round/square pressure-sensitive offset stamp gestures cross image/Core/C ABI/native Canvas | Five M6 acceptance tests; deterministic gesture/pressure, extreme-coordinate and boundary tests; Rust/C++ ABI; Windows editor/gesture smoke | Native semantics are documented project behavior rather than a claim about an undocumented legacy kernel |
| ADJUST-001 | Verified | Stable-ID brightness/contrast, curve, and levels layers compose in palette order with visibility/opacity without changing source pixels, persist in `M6AD`, and update through C ABI; Windows supports multiple create/select/re-edit/toggle/reorder; alpha edit/gradient and grayscale alpha view never alter RGB | Order/source/opacity/visibility/save-reopen acceptance; malformed metadata; alpha RGB-preservation; Rust/C++ ABI and multiple-adjustment Windows smoke | — |
| CLIP-001 | Verified | Private typed plus standard CF_DIBV5/CF_DIB clipboard paths preserve document coordinates; Cut and compatible/selected/converted Paste choices are native commands | Core cross-paper/failure, ABI ownership, and Windows private plus DIB-only external clipboard smoke | Standard import accepts validated 24/32-bit DIB layouts |
| XFORM-001 | Verified | View-only flips remain separate from destructive horizontal/vertical mirror, 90-degree rotate, image/paper size, and resolution dialogs | Core pixel/frame/guide/history and Windows command/dialog revision smoke | — |
| XFORM-002 | Verified | Floating selection move/scale/rotate has dialog and Canvas-handle preview plus exact commit/cancel | Core transform/retry/cancel, FFI lifecycle, and Windows dialog/handle gesture smoke | — |
| SHORT-001 | Verified | A categorized 24-command native editor supports rebind, conflict replacement, actual key resolution, and reset | Core/FFI resolve plus Windows editor/conflict/reset/WM_KEY smoke | — |
| COLOR-001 | Verified | Native RGB/HSV/alpha editor preserves RGBA8/16; menus choose selected/topmost/composite/light-table eyedropper source | Exact-depth Core/FFI plus Windows editor/source/Canvas smoke | BGRA8 display conversion never replaces stored exact-depth color |
| COLOR-002 | Verified | Native palette, chart page/search/edit/save/load, sequence subpalette, and color-check workflows include number and Tab shortcuts | Core palette/subpalette tests and Windows pane/menu/shortcut/file smoke | Legacy preset bytes remain `Unknown`; Inkpod files are documented separately |
| LT-001 | Verified | Native split set/item panes expose set administration, per-item transform/color/mode/opacity, and global opacity | Set/item transaction/opacity/native round-trip plus Windows panel/property smoke | — |
| LT-002 | Verified | Reference alignment, Canvas move/Shift-all, boundary/color sampling, reload, and dirty-safe edit-image swap are native controls | Mixed-size/reopen/swap goldens, read-only fill, ABI, and Windows movement/reload/sample/swap smoke | — |
| SEQ-001 | Verified | Native sequence pane/file commands provide natural-order import/export, numbered thumbnails, gaps, and first/previous/next/last/goto | Order/gap/thumbnail/dirty-switch plus Windows file/menu/pane smoke | — |
| SEQ-002 | Verified | Timed motion UI provides all six FPS values, loop, pause/step/first/last, selection/light-table options, and shortcuts | Core motion, C ABI state, and Windows timer/menu/key smoke | — |
| BATCH-001 | Verified | Checksummed versioned `.inkbatch` persists file/folder/current-sequence input selectors, ordered operations, conjunctive stable-ID/type selectors, and output/failure policy; a native modeless Batch palette edits and saves/loads the graph | Format bounds/checksum/atomic replacement tests, complete operation/filter graph round-trip, FFI copied-span ownership, Windows palette/menu smoke | This is the documented Inkpod graph, not a legacy preset compatibility claim |
| BATCH-002 | Verified | Ordered catalog covers replace/swap, continuous fill, separation, visibility, four line-width modes, all M6 filters, boundary airbrush, dust, mirror, rotate, resize/DPI, and plane conversion with skip/error target policy | Six M7 acceptance tests, complete catalog round-trip, Core operation tests, FFI nested-record validation, Windows add/edit/reorder/remove/swap/boundary dry-run smoke | Boundary airbrush starts with two colors; algorithms retain documented Inkpod semantics; undocumented legacy kernels are not inferred |
| BATCH-003 | Verified | Natural-order preview, seed-color warnings, current-file/UUID selection, current/all dry-run and execution, progress/cancel including cancellable waits, per-output atomicity, explicit continue/stop reports, and non-overwriting default policy run on the Core engine | Named M7 acceptance 6/6, current-file/wait regressions, FFI preview/report/cancel, final Release and reviewed Debug CTest 3/3 with real Batch palette dry-run/output smoke | Native `.inkpod` is the only batch output format currently enabled; overwrite requires explicit policy |
| M0 Windows shell (Help/About) | Verified | Japanese Help command opens a reference-DPI-normalized native modal About centered on the owner, with 15/9-point fonts, a 40 px reference-height name label, exact-size generated PNG icon, `Inkpod`, shorter description, CMake-derived version, and copyright | Final Debug smoke verifies target-DPI geometry/name-label height/font/icon/string/non-overlap; assets + ABI + application CTest passed 3/3 | The 574 x 544 reference is device pixels at 144 DPI and is converted once while retaining Windows theme, modal, and keyboard behavior |
| M8 legacy compatibility audit | Verified | Rights-cleared fixture/oracle gate plus per-format measured read/write/round-trip matrix | `m8_acceptance_unverified_legacy_codecs_remain_unknown` | The audit is Verified; each unavailable codec remains `Unknown` with zero measured variants |
| M8 malformed-input resilience | Verified | Six-file forged-length/dimension corpus with asserted bounded reject paths, deterministic truncation/bit-flip harness, and failed-open Core preservation cover native, batch, PNG, TIFF, TGA, and BMP public decoders/readers | `m8_acceptance_corrupted_file_corpus_is_bounded_and_non_destructive`; `m8_mutation_fuzz_all_file_decoders_never_panics`; `m8_corrupted_open_preserves_the_current_document_and_every_file` | Corpus rejection preserves Core state, input/existing output, and creates no temporary output |
| M8 large-document performance | Verified | Maximum-dimension sparse allocation/COW plus bounded dense filter benchmark | `cargo bench -p inkpod-image --bench large_document -- --quick` | Timing is reported, not used as a machine-dependent pass threshold |
| M8 Windows package | In progress | CMake assembles x64 MSIX with executable, assets, license, notices, and app-local MSVC runtime; non-elevated smoke unpacks and verifies the actual artifact | Debug/Release MakeAppx and payload CTest 4/4; the earlier runtime-dependent package passed elevated install/run/uninstall, but UAC cancellation prevented rerunning that acceptance on the corrected self-contained artifact | Current corrected artifact still needs one elevated Windows 11 install/installed ABI/uninstall pass; distribution also requires a protected production publisher credential |
| M8 Core portability | Verified | All Rust crate source targets, crate/workspace manifests, and resolved lockfile reject Windows imports/configuration/packages; next-frontend API gaps are documented | `m8_acceptance_rust_workspace_has_zero_windows_imports`; Linux/macOS CI workspace checks | Sandboxed frontends still need byte/stream I/O plus platform file-authority adapters |

The 2026-07-24 GUI vertical-slice audit treats production menu/dialog/pane/
toolbar/shortcut/Canvas routing through the Windows adapter, C ABI, Core, and result display as
part of completion for user-invoked requirements. ABI calls made only by the
application smoke test do not satisfy that condition. A requirement remains
`In progress` when any user-visible operation grouped under its ID is not
exposed. All formerly `In progress` M0-M6 rows now satisfy this rule, including
real WM_COMMAND, dialog-control, listbox-drag, keyboard, timer, clipboard, and
Canvas-pointer application smoke. Internal build, ABI, and renderer contracts
do not need artificial GUI commands.

The reviewed 2026-07-25 M7 worktree passed named M6 acceptance 5/5, named M7
acceptance 6/6, Rust format, and the final WSL all-feature workspace suite (Core
64, architecture 1, FFI 15, format 20, image 22, plus doc-tests). WSL clippy
passed all targets/features with zero warnings; Windows Application Control
still blocked the exact `cargo-clippy.exe` frontend before startup (`os error
4551`). Strict MSVC Debug and Release builds passed. Final Release CTest passed
3/3 with M1-M7 ABI and real Batch palette/application/D2D smoke; Debug passed the
same reviewed GUI/ABI source before the last Rust-only validation/save-poll
refinements, then Application Control blocked the freshly linked Debug EXE before
test startup. The immediate unchanged Debug rebuild reported `ninja: no work to
do`, so Cargo was not reinvoked. The subsequent M8 pass re-ran the named M7
acceptance suite 6/6 before making M8 changes.

The final M8 pass completed the three named acceptance tests, the six-decoder
mutation harness, zero-warning clippy, all-feature workspace tests, and the
large-document quick benchmark under WSL. Strict Debug and Release Windows
builds assembled the MSIX and passed their non-package native smoke 3/3. An
elevated Windows 11 Pro build 26200 Release-package smoke then enforced a clean
all-users state and passed install, installed ABI execution, uninstall, and
ephemeral certificate/private-key/temp cleanup. Hosted Windows Server CI builds
the package but excludes that explicitly workstation-only install test.

The 2026-07-25 M8 review found that the installed smoke had run on a developer
machine with the MSVC runtime already present, while the MSIX did not carry its
`/MD` runtime. The package now includes the toolchain's app-local CRT and a
non-elevated artifact-unpack smoke. Debug and Release CTest pass 4/4 and repeated
builds are no-ops. The corrected artifact's elevated install rerun was cancelled
at UAC, so only the Windows-package row is conservatively `In progress`.

## Legacy codec measured scope

| Item | Status | Fixture / oracle | Read | Write | Round-trip | Reason |
|---|---|---:|---:|---:|---:|---|
| DGA binary codec | Unknown | 0 fixtures | 0 variants | 0 variants | 0 variants | No rights-cleared fixture and independent oracle |
| CEL binary codec | Unknown | 0 fixtures | 0 variants | 0 variants | 0 variants | No rights-cleared fixture and independent oracle |
| Legacy palette preset | Unknown | 0 fixtures | 0 variants | 0 variants | 0 variants | Byte layout is not defined by the internal specification |
| Legacy chart preset | Unknown | 0 fixtures | 0 variants | 0 variants | 0 variants | Byte layout is not defined by the internal specification |
| Legacy filter preset | Unknown | 0 fixtures | 0 variants | 0 variants | 0 variants | Byte layout and proprietary kernel semantics are not independently defined |

No legacy manual, image, icon, wording, proprietary binary assumption, or
third-party artwork was used. The matrix records the measured zero scope rather
than presenting absent codecs as compatible. PNG dependency licenses are
recorded in `docs/third-party-notices.md`.
