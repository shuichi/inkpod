# Implementation status

## Current milestone

- Milestone: M1
- Status: Verified
- Last verified commit/worktree state: reviewed M1 changes are uncommitted; Windows 11 VS2026 x64 Debug/Release plus Windows/WSL stable Rust validation passed on 2026-07-20

`Verified` here means every M1 acceptance scenario in `PROMPT.md` is covered by
an automated Rust/ABI test or the hidden Windows smoke test. M2 was not started.

## Requirements

| ID | Status | Implementation | Tests | Notes |
|---|---|---|---|---|
| ARCH-001 | Verified | CMake explicitly tracks image/format/core/FFI inputs and Cargo byproducts behind a completion stamp | Debug/Release build; immediate repeat has no work | CMake remains the build entry |
| ARCH-002 | Verified | Core/image/format are safe and frontend-independent | All three domain crates' source/manifest scan, clippy, workspace tests | No Rust Windows dependency |
| ABI-001 | Verified | ABI v1 adds sized document/view/stroke records, live begin/append/end/cancel with strided sample spans, checked UTF-8 paths/diagnostics, bounded cumulative stroke work, and existing ownership/panic rules | C11/C++20 integrated ABI smoke; Rust lifecycle/negative/live-preview/UTF-8/extended-stride/M1 tests | C11 object is linked into `inkpod.exe --abi-smoke-test` |
| ABI-002 | Verified | Immutable raster snapshot exposes premultiplied-BGRA tiles plus view transform; Core reuses unchanged tile buffers and transfers one Rust owner to the renderer queue | Core/FFI preview snapshots and Windows upload/replacement/device-loss smoke | Snapshot sink releases on enqueue failure, replacement, and shutdown |
| IO-001 (M1) | Verified | `.inkpod` v1 UUID/manifest/blob, checksums, bounded decoder, temp/sync/replace, open/save/revert/savepoint | Round-trip, malformed, cancellation, replacement, save/discard/reopen | Autosave/recovery/background progress remain M2/incomplete IO-001 scope |
| IO-002 | Not started | — | — | M4 |
| DOC-001 (M1) | Verified | Cell paper/DPI plus 100/reference/drawing/safe frames and margins | 1920 x 1080 create and metadata save/reopen equality | M1 default values are fixed |
| DOC-002 (M1) | Verified | Platform-generated document UUID, stable document/layer/main/color IDs, typed binary main-line and RGBA color planes | UUID/ID/pixel/checksum round-trip and main-line protection | General typed tree operations are M3 |
| HIST-001 (M1) | Verified | Core-owned live stroke preview, end-as-one transaction, exact cancel/failure rollback, Undo/Redo, redo truncation, savepoint, revert | Core live-preview/cancel/failure/history tests; FFI and Windows pre-pointer-up smoke | Dialog preview remains later scope; stroke preview is complete |
| VIEW-001 (M1) | Verified | device-pixel zoom/pan/fit/1:1 with independent view revision and persistent Fit/1:1 resize behavior | Core mode/transform tests and Windows exact-bounds DPI smoke | Box zoom and flips are M3 |
| PAINT-001 | Verified | pencil/brush/eraser, auto erase, pressure-size, clipped/bounded incremental staging; UI/Input -> Core queue uses mouse fallback and `WM_POINTER` history | Core tool/resource tests; 256-record FFI test; Windows multi-sample live stroke smoke | M1 tools only; no M2 fill |

All remaining requirement scope from M2–M8 is `Not started`. No M2 fill,
selection, autosave/recovery, or coloring extension was added.

## M0 re-verification before M1

| Criterion | Status | Evidence on 2026-07-20 before M1 edits |
|---|---|---|
| Rust format/lint/test baseline | Verified | `cargo fmt`, clippy `-D warnings`, and workspace tests passed (Core 2 + architecture 1 + FFI 4) |
| Windows x64 app creates main window/Canvas | Verified | Existing Debug `inkpod_windows_smoke` passed |
| CMake Rust target is incremental | Verified | Existing Debug build reported `ninja: no work to do` |
| create -> empty snapshot -> release -> destroy | Verified | Existing Debug C++ ABI smoke passed |
| panic/leak/double release error paths | Verified | Existing Rust negative/lifecycle tests and Debug ABI smoke passed |

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
| `cargo test --workspace --all-features` | Windows 11 x64, stable Rust | Passed: Core 13, architecture 1, FFI 6, format 4, image 4, doc-tests | 2026-07-20 |
| `cargo fmt --all -- --check` | WSL Ubuntu, stable Rust 1.97.1 | Passed | 2026-07-20 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | WSL Ubuntu, stable Rust 1.97.1 | Passed | 2026-07-20 |
| `cargo test --workspace --all-features` | WSL Ubuntu, stable Rust 1.97.1 | Passed: Core 13, architecture 1, FFI 6, format 4, image 4, doc-tests | 2026-07-20 |
| `cmake --preset windows-x64-debug` / `cmake --build --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Passed with `/W4 /WX /permissive-` | 2026-07-20 |
| `ctest --preset windows-x64-debug` | Windows 11 x64 | Passed: integrated C11/C++ ABI + M1 Windows smoke, 2/2 | 2026-07-20 |
| `cmake --preset windows-x64-release` / `cmake --build --preset windows-x64-release` | Windows 11 x64, MSVC 19.51 | Passed optimized with `/W4 /WX /permissive-`; PDB emitted for diagnostics/application-control | 2026-07-20 |
| `ctest --preset windows-x64-release` | Windows 11 x64 | Passed: integrated C11/C++ ABI + M1 Windows smoke, 2/2 | 2026-07-20 |
| Immediate Debug and Release build repeats | Windows 11 x64 | Passed: both reported `ninja: no work to do`; Cargo was not invoked | 2026-07-20 |

Earlier in the M1 review, freshly generated native test executables were
intermittently held by local application-control evaluation (`CTest` reported
`BAD_COMMAND`, and Cargo once reported OS error 4551, both before process start).
The final full Windows and WSL runs above passed; no test failed inside an
executable.

## Known gaps and unknowns

- User-facing save/open executes on the Core engine thread, but its current UI
  command handler synchronously waits for completion. The format layer has
  cancellation/no-partial-commit tests; non-blocking UI completion, progress,
  autosave, and recovery remain incomplete `IO-001` scope scheduled with M2 and
  are not labeled Verified.
- M1 stores only the required binary main-line and RGBA8 color planes. M2
  coloring/fill semantics were not implemented.
- Box zoom and view flip are later `VIEW-001` scope and remain unimplemented.
- `.inkpod` v1 separates blobs but does not compress them.
- DGA/CEL and legacy preset layouts remain `Unknown`; no codec is enabled.
- Local MSVC is 19.51 from Visual Studio Build Tools 2026. VS2022 and VS2026 x64
  remain accepted Windows validation baselines.
