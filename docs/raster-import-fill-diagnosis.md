# 解決（2026-08-29）

通常の PNG/TIFF/TGA/BMP 読み込みは、元の RGBA8/16 pixel を変換せず保護された主線
プレーンへ置き、同じ深度の空の彩色プレーンを上に作る。alpha が正の非白 pixel を
fill 境界とし、opaque white は保持したまま塗り領域として扱う。Genesis source asset は
表示用下地ではなく、初期主線の replay authority である。native v30／replay epoch 26。

提供された A0001.tga で5つの閉領域（53,033／2,039／414／17,717／21,097 pixel）の
fill、外周の overflow abort、主線 checksum 不変、Undo/Redo、save/reopen、source file 不変
を確認した。PNG/TIFF/TGA/BMP の12x12閉領域でも各36 pixelを確認した。以下は修正前の
原因調査記録である。

# ラスタ読込後の「塗りあふれ」調査

調査日: 2026-08-29。対象: `A0001.tga`。基準 commit:
`05396bd5bb17310d8a058abd73046a74233785e4` と既存の作業ツリー。
本調査では production code と元画像を変更していない。

## 結論

読み込んだ画像は immutable な Genesis の下地 asset にだけ入り、初期の主線・彩色
プレーンは空のままである。一方、通常 fill は編集プレーンだけを参照し、Genesis の
下地を参照しない。このため、画面には線が見えるのに、fill からは全面が一つの空領域に
見える。既定の「塗りあふれを中断」が有効なので、画像外周へ到達して失敗する。

対象ファイルのデコード異常、全領域の線切れ、許容誤差不足が原因ではない。
読み込みと編集処理の接続不備であり、PNG/TIFF/BMP でも同じ構造の画像で再現した。
画像中のすべての領域に線切れがないという主張ではなく、今回の全域での失敗は
元の線を参照する前に起きているという切り分けである。

## 対象画像の確認

- 1754 × 1240、計 2,174,960 pixel。24-bit true-color / RLE TGA、bottom-left origin。
- 490,338 bytes。SHA-256:
  `d5d57da45daba33ac2f5e4e606176d4c1beda9a1a9e09abb9d5c5acc8875223f`。
- RGBA decode は全 pixel が alpha 255。白 2,131,985、黒 30,865、青 5,110、
  赤 3,684、緑 `(0,140,75)` 2,407、ピンク `(255,128,255)` 909 pixel。
- inkpod の Rust decoder と Pillow の RGBA 結果は全 byte 一致した。
  RGBA SHA-256 は `9a2fdaed74ac63ad371ab85f1eef914df6713109b47d9966aa194e20ceb96e23`。
- Core 読込直後の PNG 合成出力も元の RGBA と完全一致した。
- 全 pixel を公開 `plane_pixel` API で検査すると、主線の非ゼロ pixel と
  彩色の非ゼロ RGBA pixel は、ともに **0** だった。

原画像やその PNG preview は追跡対象へ追加していない。元ファイルは読み取りだけで、
検証後の byte 一致も確認した。

## 実行経路

1. `rust/inkpod-core/src/file_io/prepare.rs` の `FileIoKind::OpenRaster` は
   `Core::import_decoded_common_raster` で staged Core を作る。
2. `rust/inkpod-core/src/animation/io.rs` の `import_owned_common_raster` →
   `new_cell_from_raster_asset` は元画像を asset にし、
   `document.base_surface = BaseSurface::Asset(record.id())` とする。
3. `rust/inkpod-core/src/document/model.rs` の `CellDocument::new` は
   空の `BinaryMask8` 主線と空の `StraightRgba8` 彩色プレーンを作る。
   上記読込は、この二つへ画像を取り込んだり線を分離したりしない。
4. 合成処理 `rust/inkpod-core/src/animation/raster.rs` は base asset と
   編集プレーンを合成するため、元の線は正常に表示・書き出しされる。
5. `rust/inkpod-core/src/paint.rs` の `apply_fill_internal` は
   `plane_for_paint_role` で主線と彩色の raster を取得する。
   base asset は `seed_fill_with_protection_and_cancel` に渡らない。
6. `rust/inkpod-image/src/fill.rs` の `hard_boundary` は二値主線、彩色の色差、
   selection、fill-protection を使う。両編集プレーンが空なので境界が存在しない。
   `overflow_abort` が有効な seed fill は画像の端で `FillError::Overflow` を返す。
7. `apps/windows/ui/main_window_runtime.cpp` は ABI の `FILL_OVERFLOW` と
   候補座標を受け取って警告を表示する。警告自体は Core の結果に対応している。

`SPEC.md` 内蔵機能仕様 §5、§11 と `IO-002` / `IO-003`、
`FILL-001` / `FILL-002` / `FILL-003` の統合経路に関係する。
階調主線を display-only とする別仕様は今回の原因ではない。
この画像の初期 layer は `BinaryColoring` で、その主線が空になっている。

## 公開 Core API による再現と対照実験

実製品と同じ公開 raster import / fill API をローカル診断プログラムから実行した。
既定相当の tolerance 0、gap 0、含み塗りなし、離れた領域 off、selection なし、
overflow abort on を使用。座標は左上原点の document pixel。

対照実験では **同じ decode 済み pixel** を既存の `ImportRasterAsset` primitive で
彩色プレーンへ入れ、同じ fill executor を呼んだ。これは原因を切り分けるための
実験であり、完成した主線分離機能や Windows UI の回避手順ではない。

| seed | 通常の読込直後 | 同じ画像を編集プレーンに置いた対照実験 |
| --- | --- | --- |
| 顔 `(800,700)` | 外周 `(800,1239)` で overflow | 53,033 pixel を変更 |
| 耳 `(990,270)` | 外周 `(990,0)` で overflow | 2,039 pixel を変更 |
| 胸元 `(860,1010)` | 外周 `(860,1239)` で overflow | 414 pixel を変更 |
| 服 `(580,1100)` | 外周 `(580,1239)` で overflow | 17,717 pixel を変更 |
| 髪 `(700,500)` | 外周 `(700,0)` で overflow | 21,097 pixel を変更 |
| 画像外周 `(0,0)` | overflow | overflow（外周を塗るため期待どおり） |

すべての失敗で document digest、DocumentInfo、history が不変であることを検査した。
成功した対照実験5件はそれぞれ Undo / Redo の digest 一致を確認した。

さらに、第三者画像に依存しない 12 × 12 の完全に閉じた黒い矩形を生成した。
PNG/TGA/BMP/TIFF の4形式すべてで、通常読込後の seed `(5,5)` は overflow、
同じ pixel を編集プレーンへ置くと内部 6 × 6 = 36 pixel だけが正常に塗れた。

## 設定変更の影響と修正範囲

- gap を 4 pixel にしても対象画像の顔で同じ overflow を再現した。
  参照先に線がないため、隙間を閉じる設定では解決しない。
- overflow abort を off にすると、顔のクリックで **全 2,174,960 pixel** が塗られた。
  これは安全な回避策ではない。実験はメモリ内で行い、Undo の digest 復元も確認した。
- 読込画像を適切な編集・境界データとして利用できるように、Genesis の不変性と
  主線保護を保ったまま、読込時の working plane 構築または fill の参照入力を修正する必要がある。
  表示だけを直すこと、警告を抑止すること、主線閾値を変えることでは解決しない。
- 修正時には、閉領域画像の **open → fill → Undo/Redo → save/reopen** を公開 API と
  Windows UI で固定し、元の線・色トレース・alpha・native depth・replay を保護する。
  primitive semantics や serialized schema を変更する場合は最上位 version の規則に従う。

## 検証範囲とローカル証跡

診断用プログラム・出力は gitignored な `.inkpod-local/fill-diagnosis/` に置いた。
`probe/src/main.rs` は公開 API と synthetic control、`probe-output.txt` は実行結果、
`inspect_image.py` / `image-analysis.json` は独立 decoder 比較を記録する。
probe は入力画像と出力 directory を引数で受け取り、source の保存は行わない。
Rust 1.97.1 / x86_64-pc-windows-msvc / Release / offline、PowerShell no-profile で
実行し、終了 code 0 を確認した。

関連する既存検証も再実行し、すべて成功した。

- `cargo test --package inkpod-core --release --offline --test contracts fill`: 10 passed。
- `cargo test --package inkpod-core --release --offline --test contracts assets_genesis`: 12 passed。
- `cargo fmt --check`、`git diff --check`: 成功。

既存テストは編集プレーンの fill と Genesis の画像保持をそれぞれ検証しているが、
今回再現した「通常の画像 open の直後に見えている閉領域を塗る」という接続の
正常動作は保証していなかった。上記成功を本不具合の解消とは扱わない。

今回の調査では可視 Windows UI の再操作、全 workspace test、Clippy、CMake build、
CTest、benchmark は行っていない。production 修正・修正版の配布も行っていない。
