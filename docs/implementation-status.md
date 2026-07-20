# Implementation status

## Current milestone

- Milestone: M0
- Status: Verified
- Last verified commit/worktree state: M0 changes are uncommitted; WSL Ubuntu and VS2026 x64 validation passed

## Requirements

| ID | Status | Implementation | Tests | Notes |
|---|---|---|---|---|
| ARCH-001 | Verified | Root CMake uses explicit library inputs, a completion stamp, and Cargo staticlib/rlib byproducts before linking MSVC targets | Debug/Release build and CTest; immediate and post-Cargo-no-op repeat builds do no work | Local MSVC 19.51; VS2022 CI configured |
| ARCH-002 | Verified | `inkpod-core` has no frontend dependencies and forbids unsafe code | Source/manifest architecture scan, clippy, Core tests | No Windows/frontend token, target-specific dependency, or unsafe code |
| ABI-001 | Verified | ABI v1 opaque Core/snapshot, prefix-validated sized structures, explicit record strides, checked batch, panic containment, per-thread errors | C11 layout; C++ lifecycle/thread/negative smoke; Rust short-allocation/stride/panic/double-release tests | Stale copied aliases remain caller errors as documented |
| ABI-002 | Verified | Immutable revision plus borrowed, explicitly strided batched tile span | Empty snapshot Core/C++ smoke | Raster tiles begin in M1 |
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
| Non-Windows Rust format/lint/test | Verified | WSL Ubuntu with stable Rust 1.97.1 passes workspace format, lint, and all tests; Ubuntu/macOS hosted CI remains configured |
| Windows CI x64 application links and creates main window/Canvas | Verified | User accepts VS2026 as a Windows validation baseline; MSVC 19.51 Debug/Release hidden hardware/WARP Direct2D create/resize/DPI/device-recovery/render smoke passes |
| CMake declares Rust inputs/output/byproducts/dependency without unconditional rebuild | Verified | Explicit inputs, stamp output, and staticlib/rlib byproducts; immediate and mtime-only-input/Cargo-no-op repeats report `no work to do`; CI rejects an unchanged Rust rerun |
| create → empty snapshot → release → destroy C/C++ smoke | Verified | Debug/Release `inkpod_abi_smoke` passes |
| Core/FFI failures contain panic and avoid leak/double release | Verified | Panic injection plus C++/Rust null, physical-short-prefix, invalid-stride/enum, wrong-thread, two-stage-error, and repeated-release tests |

## Verification

| Command | Platform | Result | Date |
|---|---|---|---|
| `cargo fmt --all -- --check` | Windows 11 x64 | Passed using the installed stable toolchain directly because Cargo is absent from this process's `PATH` | 2026-07-20 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Windows 11 x64 | Passed | 2026-07-20 |
| `cargo test --workspace --all-features` | Windows 11 x64 | Passed: Core 3, FFI 4, doc-tests | 2026-07-20 |
| `cargo fmt --all -- --check` | WSL Ubuntu, Rust 1.97.1 | Passed | 2026-07-20 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | WSL Ubuntu, Rust 1.97.1 | Passed | 2026-07-20 |
| `cargo test --workspace --all-features` | WSL Ubuntu, Rust 1.97.1 | Passed: Core 3, FFI 4, doc-tests | 2026-07-20 |
| `cmake --preset windows-x64-debug` / `cmake --build --preset windows-x64-debug` | Windows 11 x64, MSVC 19.51 | Passed with `/W4 /WX /permissive-` | 2026-07-20 |
| `ctest --preset windows-x64-debug` | Windows 11 x64 | Passed: 3/3 | 2026-07-20 |
| `cmake --preset windows-x64-release` / `cmake --build --preset windows-x64-release` | Windows 11 x64, MSVC 19.51 | Passed; immediate repeat had no work | 2026-07-20 |
| `ctest --preset windows-x64-release` | Windows 11 x64 | Passed: 3/3 | 2026-07-20 |

## Known gaps and unknowns

- `.inkpod` persistence, document state, raster tiles, edit history, and user
  drawing are M1 and are not stubbed in M0.
- M0 snapshot tile ABI is structurally batched but contains no tiles.
- DGA/CEL and legacy preset binary layouts are `Unknown`; no codec is enabled.
- The local compiler is MSVC 19.51 from Visual Studio Build Tools 2026. Both
  VS2022 and VS2026 x64 are accepted Windows validation baselines.
