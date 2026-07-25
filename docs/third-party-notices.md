# サードパーティ通知

inkpod は PNG の encode/decode に次の Rust crate を使用する。version は `Cargo.lock` で固定し、
license expression は配布される crate manifest に基づく。

| crate | version | license |
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

TIFF、TGA、BMP は project の format crate 内で実装しており、別の codec dependency を追加しない。
配布 package には、上記 expression に基づいて選択した upstream license text を同梱する。

FFI API HTML の生成には [doxygen-awesome-css](https://github.com/jothepro/doxygen-awesome-css)
v2.4.2（commit `d52eafe3e9303399fda15661f3d7bb8fe3d7eabc`）を使用する。license は MIT。
テーマ本体は文書生成時だけ取得し、生成した HTML へ upstream の `LICENSE` をコピーする。

Windows MSIX は、選択した MSVC toolchain の documented redistributable directory から app-local の
Microsoft Visual C++ runtime DLL を収録する。これらは Microsoft component であり、Microsoft Visual
Studio license terms に従って再配布する。inkpod の GPL license の対象ではない。
