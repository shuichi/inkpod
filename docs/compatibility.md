# Compatibility status

Compatibility means operation semantics, data separation, coordinates, and
saved results—not replication of a legacy user interface or assets.

| Requirement | Status | Implementation | Tests | Known difference / next work |
|---|---|---|---|---|
| ARCH-001 | Verified | CMake custom output connects Cargo `inkpod-ffi` staticlib to MSVC targets | Windows Debug/Release build and CTest; repeat build reports no work | Local compiler is MSVC 19.51; VS2022 CI is configured but not run in this unpushed worktree |
| ARCH-002 | Verified | `inkpod-core` is safe, OS-independent Rust | `arch_002_core_sources_do_not_reference_windows_apis`; clippy | No Windows/frontend dependency |
| ABI-001 | Verified | ABI v1 opaque handles, sized structs, per-thread diagnostics, pointer-to-pointer release | C11 header test; C++20 lifecycle/thread/negative smoke; Rust tests compile | Local policy blocks the unsigned Rust FFI test executable; equivalent C++ ABI tests pass |
| ABI-002 | Verified | Immutable snapshot exposes one borrowed tile span | Core and C++ empty-snapshot smoke tests | M0 has no raster tiles |
| IO-001 | Not started | — | — | `.inkpod` v1 is M1 |
| IO-002 | Not started | — | — | Common raster codecs are M4 |
| DOC-001 through VECTOR-002 | Not started | — | — | Scheduled by M1–M8 in `PROMPT.md` |

## Unknown legacy formats

| Item | Status | Reason |
|---|---|---|
| DGA/CEL binary codec | Unknown | No rights-cleared fixture and independent oracle |
| Legacy palette/chart/filter preset binary layouts | Unknown | Byte layouts are not defined by the internal specification |

No legacy manual, PDF, image, icon, wording, or proprietary binary assumption
was used for M0.
