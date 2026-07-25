# Third-party notices

inkpod uses the following Rust crates for PNG encoding/decoding. Versions are
locked in `Cargo.lock`; license expressions come from the distributed crate
manifests.

| Crate | Version | License |
|---|---:|---|
| png | 0.17.16 | MIT OR Apache-2.0 |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| flate2 | 1.1.9 | MIT OR Apache-2.0 |
| crc32fast | 1.5.0 | MIT OR Apache-2.0 |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| simd-adler32 | 0.3.10 | MIT |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 |
| bitflags | 1.3.2 | MIT/Apache-2.0 |

TIFF, TGA, and BMP support is implemented in the project format crate and does
not add another codec dependency. Distribution packaging must include the
applicable upstream license texts selected under these expressions.

The Windows MSIX includes the app-local Microsoft Visual C++ runtime DLLs from
the selected MSVC toolchain's documented redistributable directory. Those files
remain Microsoft components and are redistributed under the Microsoft Visual
Studio license terms; they are not covered by inkpod's GPL license.
