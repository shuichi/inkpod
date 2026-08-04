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

次期 `.inkpod` の canonical/section digest には、公式 Rust 実装の
[`blake3`](https://github.com/BLAKE3-team/BLAKE3) crate を採用する。2026-08-04 時点の
公式 crate manifest（1.8.5）は
`CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` を宣言しており、inkpod は
GPL-3.0-only 配布と両立する Apache-2.0 option を選択する。採用指定は exact version
`=1.8.5`、`default-features = false`、feature `std` のみとし、`rayon`、`mmap`、`serde`、
`zeroize`、traits-preview、C/NEON opt-in は有効にしない。Apache-2.0 の条件に従い、
source/binary 配布には Apache-2.0 license text、upstream copyright/attribution、同梱される
NOTICE がある場合はその NOTICE を保持する。M0 では byte/digest 契約、version/features、
license/distribution 条件だけを固定し、crate はまだ production dependency や配布物へ
追加しない。最初の digest 実装を導入する変更でこの exact dependency と実際に解決された
transitive dependency を `Cargo.lock` に固定し、それらの license/notice をこの表と
`ThirdPartyNotices.txt`/配布 payload へ同時に追加する。
SIMD backend の選択は digest bytes を変えないが、採用 version の feature/dependency 構成は
x64、ARM64、非 Windows の build gate で再確認する。

Core の固定 seed property test と失敗列の縮小には、development-only dependency として
`proptest` 1.11.0（MIT OR Apache-2.0、MSRV 1.85）を使用する。default feature は無効にし、
`std` だけを有効にして fork、timeout、bit-set、macro の依存を除外する。手書き generator は
依存を増やさない一方で shrinking と標準 replay support を失うため採用しない。M0 の
route-inventory architecture test は `syn` 2.0.119 を direct development dependency として
使用する。以下はこの二つの direct dependency とその transitive dependency の locked set で、
test build にだけ入り、配布 package には含めない。

| dependency | locked version | license |
|---|---:|---|
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| bitflags | 2.13.1 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| libc | 0.2.189 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| rand | 0.9.5 | MIT OR Apache-2.0 |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 |
| rand_core | 0.9.5 | MIT OR Apache-2.0 |
| rand_xorshift | 0.4.0 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| unarray | 0.1.4 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| wasip2 | 1.0.1+wasi-0.2.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wit-bindgen | 0.46.0 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| zerocopy | 0.8.55 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.55 | BSD-2-Clause OR Apache-2.0 OR MIT |

FFI API HTML の生成には [doxygen-awesome-css](https://github.com/jothepro/doxygen-awesome-css)
v2.4.2（commit `d52eafe3e9303399fda15661f3d7bb8fe3d7eabc`）を使用する。license は MIT。
テーマ本体は文書生成時だけ取得し、生成した HTML へ upstream の `LICENSE` をコピーする。

Windows x64/ARM64 binary は、選択した MSVC toolchain の Microsoft Visual C/C++ runtime と Rust
MSVC target の C runtime support を executable へ静的リンクする。runtime code は適用される
Microsoft Visual Studio license terms に従って再配布し、inkpod の GPL license の対象には含めない。
MSIX と portable ZIP は app-local の MSVC CRT DLL を収録せず、この文書を package root の
`ThirdPartyNotices.txt` として収録する。
