# FFI API HTML の生成

この directory は、`include/inkpod/core_ffi.h`を正本として日本語 HTML API reference を生成する。
利用ガイド `docs/ffi.md`を top page にし、各型・関数の詳細は header の Doxygen comment から生成する。

theme は `doxygen-awesome-css` v2.4.2 の release commit を CMake `FetchContent` で固定取得する。
通常の application build では取得しない。

## 必要な tool

- CMake 3.25 以上
- Doxygen 1.17.0 以上
- theme の初回取得に使う Git と network connection

Graphviz、LaTeX、Node.js は不要である。

## 生成手順

repository root で次を実行する。

```powershell
cmake -S docs/api -B build/docs-api
cmake --build build/docs-api --target inkpod_ffi_docs
```

Doxygen が PATH にない場合は configure 時に指定する。

```powershell
cmake -S docs/api -B build/docs-api `
  -DDOXYGEN_EXECUTABLE=C:/tools/doxygen/bin/doxygen.exe
cmake --build build/docs-api --target inkpod_ffi_docs
```

生成物は `docs/api/html/index.html`。この directory は生成物なので Git 管理外とする。
Doxygen 1.17 は日本語 UI translation が古いという upstream warning を常に出すため、warning 全体は
error 扱いにしない。Doxygen comment の構文・参照 warning は build log で確認する。公開型・関数の
comment 欠落は header review と ABI test で検証する。

出力先を変える場合は `INKPOD_API_DOCS_OUTPUT_DIRECTORY` を指定する。

```powershell
cmake -S docs/api -B build/docs-api `
  -DINKPOD_API_DOCS_OUTPUT_DIRECTORY=C:/tmp/inkpod-api
```
