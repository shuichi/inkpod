# Compatibility status

Compatibility means operation semantics, data separation, coordinates, and
saved results—not replication of a legacy user interface or assets.

| Requirement | Status | Implementation | Tests | Known difference / next work |
|---|---|---|---|---|
| ARCH-001 | Verified | A CMake completion stamp connects Cargo `inkpod-ffi` staticlib/rlib byproducts to MSVC targets | VS2026 x64 Debug/Release build and CTest; unchanged repeat build rejects a Rust rerun in CI | MSVC 19.51 passed; VS2022 and VS2026 are accepted Windows validation baselines |
| ARCH-002 | Verified | `inkpod-core` is safe, OS-independent Rust | Source and manifest architecture scan; clippy; Core tests | No Windows/frontend dependency or target-specific Windows crate |
| ABI-001 | Verified | ABI v1 opaque handles, prefix-validated sized structs, strided records, per-thread diagnostics, pointer-to-pointer release | C11 layout test; C++20 lifecycle/thread/negative smoke; Rust short-allocation, stride, panic, and double-release tests | Arbitrary stale pointer aliases remain caller errors as documented |
| ABI-002 | Verified | Immutable snapshot exposes one borrowed, explicitly strided tile span | Core and C++ empty-snapshot smoke tests | M0 has no raster tiles |
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
