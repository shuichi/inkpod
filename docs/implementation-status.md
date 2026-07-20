# Implementation status

## Current milestone

- Milestone: M0
- Status: In progress
- Last verified commit/worktree state: `main` clean before M0; M0 changes are uncommitted

## Requirements

| ID | Status | Implementation | Tests | Notes |
|---|---|---|---|---|
| ARCH-001 | Verified | Root CMake custom output builds Cargo staticlib once per changed input and links MSVC targets | Debug/Release build and CTest; repeat build is a no-op | Local MSVC 19.51; VS2022 CI configured |
| ARCH-002 | Verified | `inkpod-core` has no frontend dependencies and forbids unsafe code | Architecture scan, clippy, Core tests | No Windows/frontend token or unsafe code |
| ABI-001 | Verified | ABI v1 opaque Core/snapshot, sized structures, checked batch, panic containment, per-thread errors | C11 header; C++ lifecycle/thread/negative smoke; Rust tests compile | Local signing policy blocks only the Rust FFI test executable |
| ABI-002 | Verified | Immutable revision plus borrowed batched tile span | Empty snapshot Core/C++ smoke | Raster tiles begin in M1 |
| IO-001 | Not started | — | — | M1 |
| IO-002 | Not started | — | — | M4 |
| DOC-001 | Not started | — | — | M1 |
| DOC-002 | Not started | — | — | M1 |
| HIST-001 | Not started | — | — | M1 |
| VIEW-001 | Not started | — | — | M1 |
| PAINT-001 | Not started | — | — | M1 |

All remaining requirement IDs from `PROMPT.md` are `Not started` and belong to
M2–M8. They are intentionally not represented by placeholder APIs or UI.

## M0 acceptance criteria

| Criterion | Status | Evidence |
|---|---|---|
| Non-Windows Rust format/lint/test | In progress | Ubuntu/macOS CI is configured but cannot run before a permitted push; local format/clippy and test compilation pass |
| Windows x64 application links and creates main window/Canvas | Verified | Debug/Release hidden hardware/WARP Direct2D smoke passes |
| CMake declares Rust inputs/output/dependency without unconditional rebuild | Verified | `add_custom_command(OUTPUT ...)`; repeat Release build reports `no work to do` |
| create → empty snapshot → release → destroy C/C++ smoke | Verified | Debug/Release `inkpod_abi_smoke` passes |
| Core/FFI failures contain panic and avoid leak/double release | Verified | Boundary containment plus C++ null/short/wrong-thread/two-stage-error/double-release tests |

## Verification

| Command | Platform | Result | Date |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11 x64 | Passed (direct installed rustfmt selected because the proxy is policy-blocked) | 2026-07-20 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64 | Passed | 2026-07-20 |
| `cargo test --workspace --all-features --no-run` | Windows 11 x64 | Passed; every Rust test executable compiled | 2026-07-20 |
| `cargo test --package inkpod-core --all-features` | Windows 11 x64 | Passed: 3 tests plus doc-tests | 2026-07-20 |
| `cargo test --workspace --all-features` | Windows 11 x64 | Core 3 tests passed; unsigned FFI test executable blocked before execution by Code Integrity events 3033/3077 | 2026-07-20 |
| `cmake --preset windows-x64-debug` / `cmake --build --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Passed with `/W4 /WX /permissive-` | 2026-07-20 |
| `ctest --preset windows-x64-debug` | Windows 11 x64 | Passed: 3/3 | 2026-07-20 |
| `cmake --preset windows-x64-release` / `cmake --build --preset windows-x64-release` | Windows 11 x64, MSVC 19.51 | Passed; immediate repeat had no work | 2026-07-20 |
| `ctest --preset windows-x64-release` | Windows 11 x64 | Passed: 3/3 | 2026-07-20 |

## Known gaps and unknowns

- `.inkpod` persistence, document state, raster tiles, edit history, and user
  drawing are M1 and are not stubbed in M0.
- M0 snapshot tile ABI is structurally batched but contains no tiles.
- DGA/CEL and legacy preset binary layouts are `Unknown`; no codec is enabled.
- Enterprise Code Integrity rejects the locally built unsigned Rust FFI unit
  test executable. The same static library passes Debug/Release C++ ABI tests,
  and all Rust tests compile; Ubuntu/macOS CI will execute the Rust test suite.
- The local compiler is MSVC 19.51 from Visual Studio Build Tools 2026. CI uses
  the Visual Studio 2022 environment required as the project baseline.
