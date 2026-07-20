# Implementation status

## Current milestone

- Milestone: M2
- Status: Verified
- Last verified commit/worktree state: M2 changes are uncommitted; Windows 11
  VS2026 x64 Debug/Release plus Windows and WSL stable Rust validation passed on
  2026-07-20

`Verified` here means every required M2 golden case in `PROMPT.md` is covered by
an automated Rust test, with the seed-fill/eyedropper/color-check/recovery
vertical slice also covered through the C ABI and real Windows smoke test. M3
was not started.

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
| ABI-001 | Verified | ABI v1 adds sized/strided M2 fill/color records, exact 8/16-bit colors, leak coordinates, eyedropper/check modes, autosave/recovery, plus the M1 live-stroke and ownership/panic contracts | C11/C++20 integrated ABI smoke; Rust lifecycle/negative/live-preview/M1/M2 tests | C11 object is linked into `inkpod.exe --abi-smoke-test` |
| ABI-002 | Verified | Immutable raster snapshot exposes premultiplied-BGRA tiles plus view transform and temporary color-check output; Core reuses unchanged tile buffers and transfers one Rust owner to the renderer queue | Core/FFI preview/check snapshots and Windows upload/replacement/device-loss/color-check smoke | Snapshot sink releases on enqueue failure, replacement, and shutdown |
| IO-001 (native save) | Verified | `.inkpod` v1 UUID/manifest/blob, checksums, bounded decoder, temp/sync/replace, open/save/revert/savepoint; grayscale 8/16 and straight RGBA8/16 tile codes | Round-trip, 16-bit exact bytes, malformed, cancellation, replacement, save/discard/reopen | Blob compression remains optional and disabled |
| IO-001 (M2 recovery) | Verified | Atomic autosave leaves the normal savepoint/path untouched; recovery opens dirty, recovered, and pathless; Windows timer queues autosave without blocking UI | Core/FFI recovery tests and Windows normal-vs-recovery smoke | Large normal save/open progress UI remains incomplete IO-001 scope |
| IO-002 | Not started | — | — | M4 |
| DOC-001 (M1) | Verified | Cell paper/DPI plus 100/reference/drawing/safe frames and margins | 1920 x 1080 create and metadata save/reopen equality | M1 default values are fixed |
| DOC-002 (M2 coloring planes) | Verified | Stable typed binary/grayscale main-line and straight RGBA8/16 color planes; grayscale coverage and line base-color sampling are separate semantics | Exact format round-trip, grayscale display/eyedropper golden, protected main-line fill checks | General typed tree operations are M3 |
| HIST-001 (M2 transaction) | Verified | Stroke and fill both commit at most one history unit; overflow/cancel/invalid/no-op cannot partially change pixels/revision/history | Core fill Undo/Redo/atomicity tests; FFI and Windows fill smoke | Dialog preview remains later scope |
| VIEW-001 (M1) | Verified | device-pixel zoom/pan/fit/1:1 with independent view revision and persistent Fit/1:1 resize behavior | Core mode/transform tests and Windows exact-bounds DPI smoke | Box zoom and flips are M3 |
| PAINT-001 | Verified | pencil/brush/eraser, auto erase, pressure-size, clipped/bounded incremental staging; UI/Input -> Core queue uses mouse fallback and `WM_POINTER` history | Core tool/resource tests; 256-record FFI test; Windows multi-sample live stroke smoke | PAINT-001 scope complete |
| FILL-001 | Verified | Connected seed fill uses normalized 16-bit per-channel tolerance and an optional same-sized binary selection mask | Image golden/property cases; Core/FFI selection/transaction tests; Windows Canvas click smoke | M3 will add selection creation/editing UI |
| FILL-002 | Verified | Specified/except-specified inclusion (max six), overflow abort with candidate coordinate, bounded cancel/work, axis gap-close, detached matching regions | Required inclusion/gap/overflow golden cases plus cancel/no-op tests | Native deterministic gap rule is documented; no proprietary algorithm is claimed |
| FILL-003 | Verified | Closed-region fill excludes escaping components, supports transparent-only/inclusion, and fill extension spreads through a bounded mask/distance | Image closed/open golden and extension tests; Core/ABI operation validation | Windows M2 UI exposes seed fill; range gesture tools are later PAINT/selection UI scope |
| COLOR-001 (M2 scope) | Verified | Straight RGBA8/16 and grayscale 8/16 typed values, exact-depth eyedropper sources, explicit 16-to-8 display conversion only | Grayscale coverage/base-color and no-quantization goldens; format/FFI tests | Light-table source reports unavailable until M4 supplies items; full color-editor UI is later scope |
| COLOR-002 (M2 scope) | Verified | Bounded exact 8/16-bit palette entries and temporary legacy-white/native-alpha color-check categories | Palette no-quantization golden; Core/Windows color-check view-only tests | Chart and subpalette are M4 scope |
| M0 Windows shell (Help/About) | Verified | Japanese Help command, native modal About, shared generated icon, CMake-derived version and EXE version resource | Release `inkpod_windows_smoke` creates and closes the dialog through `WM_COMMAND`; EXE resource inspection | macOS-inspired information hierarchy implemented with native Win32 behavior |
| M8 packaging assets | In progress | winapp CLI manifest, 48 scale/target-size PNGs, five-resolution ICO | `inkpod_windows_assets`; Release resource build | MSIX assembly, signing, and clean install/uninstall are not yet tested |

All M3 requirements remain `Not started`; no layer-tree editing, selection
creation, clipboard, destructive transform, guide/grid, locator, multi-view, or
shortcut-editor implementation was added.

## M0 re-verification before M1

| Criterion | Status | Evidence on 2026-07-20 before M1 edits |
|---|---|---|
| Rust format/lint/test baseline | Verified | `cargo fmt`, clippy `-D warnings`, and workspace tests passed (Core 2 + architecture 1 + FFI 4) |
| Windows x64 app creates main window/Canvas | Verified | Existing Debug `inkpod_windows_smoke` passed |
| CMake Rust target is incremental | Verified | Existing Debug build reported `ninja: no work to do` |
| create -> empty snapshot -> release -> destroy | Verified | Existing Debug C++ ABI smoke passed |
| panic/leak/double release error paths | Verified | Existing Rust negative/lifecycle tests and Debug ABI smoke passed |

## M2 acceptance scenarios

| # | Required golden case | Status | Evidence |
|---|---|---|---|
| 1 | Only a completely closed region is filled | Verified | Image test `m2_golden_only_completely_closed_regions_are_filled` exercises the closed-region operation with adjacent closed and edge-connected regions |
| 2 | A one-pixel gap leaks at setting 0 and closes at setting 1 | Verified | Image test `m2_golden_one_pixel_gap_leaks_at_zero_and_closes_at_one` compares both settings on the same trace |
| 3 | Overflow abort reports the reached edge and commits zero pixels | Verified | Image test `m2_golden_overflow_abort_and_cancel_never_mutate_the_source`, Core atomicity test, and FFI leak-coordinate check verify plan-before-commit behavior |
| 4 | Included trace colors are replaced while non-target trace colors remain | Verified | Image test `m2_golden_inclusion_replaces_target_trace_but_preserves_other_trace` covers specified-color inclusion and preservation of the other trace |
| 5 | Grayscale-line display coverage agrees with base-color eyedropper sampling | Verified | Image test `m2_golden_grayscale_display_coverage_and_base_color_eyedropper_agree` plus Core view-only snapshot/eyedropper test |
| 6 | Pixels outside the selection remain unchanged | Verified | Image test `m2_golden_selection_clips_every_fill_edit`; Core accepts the typed selection rectangle without changing the protected main-line plane |
| 7 | A 16-bit value is never implicitly quantized to 8 bit | Verified | Image palette/fill golden, format byte-exact RGBA16 round-trip, and FFI rejection of a 16-bit fill into an RGBA8 plane |
| 8 | Opening an autosave never overwrites the normal saved file | Verified | Core test `m2_autosave_recovery_never_inherits_or_overwrites_normal_path`, FFI recovery test, and Windows smoke compare the reopened normal-file checksum after a pathless recovery session |

The Windows smoke additionally drives the real Fill and Eyedropper menu
commands through a Canvas click, checks that fill is one revision/Undo unit and
does not alter the main-line checksum, renders the temporary color check, queues
autosave without a UI wait, opens recovery as dirty/pathless, and then reopens
the unchanged normal file.

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
| `cargo fmt --all -- --check` | Windows 11 x64, stable Rust | Passed | 2026-07-20 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64, stable Rust | Passed | 2026-07-20 |
| `cargo test --workspace --all-features` | Windows 11 x64, stable Rust | Passed: Core 17, architecture 1, FFI 7, format 5, image 12, doc-tests | 2026-07-20 |
| `cargo fmt --all -- --check` | WSL Ubuntu, stable Rust 1.97.1 | Passed | 2026-07-20 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | WSL Ubuntu, stable Rust 1.97.1 | Passed | 2026-07-20 |
| `cargo test --workspace --all-features` | WSL Ubuntu, stable Rust 1.97.1 | Passed: Core 17, architecture 1, FFI 7, format 5, image 12, doc-tests | 2026-07-20 |
| `cmake --preset windows-x64-debug` / `cmake --build --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Passed with `/W4 /WX /permissive-` | 2026-07-20 |
| `ctest --preset windows-x64-debug` | Windows 11 x64 | Passed: assets + integrated C11/C++ ABI + M1/M2 Windows smoke, 3/3 | 2026-07-20 |
| `cmake --preset windows-x64-release` / `cmake --build --preset windows-x64-release` | Windows 11 x64, MSVC 19.51 | Passed optimized with `/W4 /WX /permissive-`; PDB emitted for diagnostics/application-control | 2026-07-20 |
| `ctest --preset windows-x64-release` | Windows 11 x64 | Passed: assets + integrated C11/C++ ABI + M1/M2 Windows smoke, 3/3 | 2026-07-20 |
| Immediate Debug and Release build repeats | Windows 11 x64 | Passed: both reported `ninja: no work to do`; Cargo was not invoked | 2026-07-20 |
| `winapp manifest update-assets AppIcon.svg --manifest apps/windows/package/Package.appxmanifest --verbose` | Windows 11 x64, Windows App Development CLI 0.4.0 | Passed: 48 PNG assets and 16/24/32/48/256 ICO generated | 2026-07-20 |

Earlier in the M1 review, freshly generated native test executables were
intermittently held by local application-control evaluation (`CTest` reported
`BAD_COMMAND`, and Cargo once reported OS error 4551, both before process start).
The final full Windows and WSL runs above passed; no test failed inside an
executable.

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
- Box zoom and view flip are later `VIEW-001` scope and remain unimplemented.
- `.inkpod` v1 separates blobs but does not compress them.
- DGA/CEL and legacy preset layouts remain `Unknown`; no codec is enabled.
- Local MSVC is 19.51 from Visual Studio Build Tools 2026. VS2022 and VS2026 x64
  remain accepted Windows validation baselines.
