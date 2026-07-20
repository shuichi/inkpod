# inkpod 開発ガイド

## 1. 適用範囲と仕様の正本

この指示はリポジトリ全体に適用する。inkpod は、旧 PaintMan のアニメーション彩色ワークフローを、長期保守可能なクロスプラットフォーム設計で再構築するプロジェクトである。

このファイルには、全タスクに常時適用する技術境界、品質基準、作業規律だけを置く。機能仕様、GUI メニュー、要件 ID、M0–M8 の内容と acceptance criteria は `PROMPT.md` を正本とし、ここへ複製しない。実装前に `PROMPT.md` の関連節と `docs/implementation-status.md` を読む。

指示が競合する場合の優先順位は、今回のユーザー指示、`AGENTS.md`、`PROMPT.md`、テスト済みの既存契約の順とする。外部の旧製品マニュアルや画像を通常の実装時に参照せず、未確定の proprietary binary 仕様を推測で互換と称しない。旧製品の画像、アイコン、文面、商標表示を複製しない。

「合理的な互換性」は旧 UI の模写ではなく、操作の意味、データ分離、座標、保存結果を再現することである。Windows 固有の外見と操作は Windows 11 の標準に合わせる。

## 2. アーキテクチャ境界

- Rust Core は OS 非依存の計算、文書状態、画像処理、選択、履歴、永続化、入力解釈、描画スナップショットを担当する。
- C++/Win32 は `wWinMain`、メッセージループ、`HWND`、COM、ファイルダイアログ、クリップボード、スレッド連携、DPI、テーマ、アクセシビリティ、Direct2D renderer を担当する。
- Common Controls v6 は menu、toolbar、status bar、tab、list、tree、form、dialog 等の通常 UI を担当する。
- Canvas `HWND` は Direct3D 11、DXGI swap chain、Direct2D device context、DirectWrite、必要な WIC 連携を所有する。
- CMake をビルド全体の入口とし、Cargo による Rust `staticlib` も CMake の target から構築できるようにする。
- Rust と C++ は versioned C ABI だけで接続する。Rust ABI や C++ ABI に依存しない。

Rust Core へ `HWND`、Windows message、COM/WinRT、Direct2D/Direct3D/DXGI/WIC/DirectWrite、Common Controls、registry、Windows 固有 DPI や UI thread の型を入れない。Core が保持してよい表示状態は、zoom、論理 pan、view flip、guide、grid、active tool など OS 非依存の意味上の状態だけである。

C++ に画像処理、レイヤー規則、選択演算、履歴、native file format の別実装を作らない。C++ command handler は入力を C ABI の command/event へ変換し、結果を UI と renderer へ反映する薄い adapter にする。

## 3. コード構成とビルド

責務は原則として次へ分離する。初期 milestone で空 crate や空 directory を大量生成しない。

- `rust/inkpod-core`: 文書、状態遷移、履歴、snapshot
- `rust/inkpod-image`: raster/vector、selection、fill、filter
- `rust/inkpod-format`: `.inkpod` と import/export
- `rust/inkpod-ffi`: C ABI と `staticlib` のみ
- `include/inkpod/core_ffi.h`: C/C++ 公開 header
- `apps/windows/app`: Win32 application と OS adapter
- `apps/windows/renderer`: Direct2D/D3D11/DXGI renderer
- `apps/windows/ui`: Common Controls UI
- `tests`: fixture、golden、FFI、integration
- `docs`: architecture、FFI、file format、compatibility、status

循環依存を避け、`inkpod-ffi` は公開 API の薄い変換だけにする。形式 crate から application state へ逆依存せず、必要なら serialization DTO を境界に置く。

- Rust は stable、edition 2024。nightly 固有機能へ依存しない。
- Windows は MSVC C++20 と Unicode API を使い、Visual Studio 2022 または 2026 x64 を検証基準とする。
- `staticlib` は `inkpod-ffi` だけに設定し、MSVC runtime は Rust MSVC target と整合する `/MD` 系で統一する。
- manifest で Common Controls v6 と Per-Monitor DPI Awareness v2 を有効にする。
- build にローカル絶対 path、手動 file copy、開発者個人だけの前提を埋め込まない。
- 非 Windows でも Rust の build/test を可能にし、Win32 target は明示的に skip する。
- 依存は必要最小限とし、配布ライセンスを確認して third-party notice を更新する。

## 4. Rust Core の不変条件

- document、view logical、device 座標を型または明示名で区別し、正逆変換をテストする。
- raster 寸法、stride、index は固定幅整数で overflow と境界を検査する。
- 2 値、grayscale、RGBA 8/16 bit、selection mask を型付き `PixelFormat` で区別する。
- sRGB RGBA を損失なく保持し、straight alpha と premultiplied alpha を型または明示 API で区別する。
- project/cut、cell、layer、plane、frame、sequence は永続化可能な安定 ID を持つ。名前や配列添字を ID の代用にしない。
- layer type ごとに許可 plane、変換、統合条件を検証する。主線保護中の彩色 command は主線 plane を変更しない。
- 大画像は tile、遅延割当、copy-on-write を基本とし、Undo、snapshot、light table ごとの全画像複製を避ける。
- UI 操作は型付き command/input を通す。破壊的編集は transaction として成功時だけ commit する。
- stroke は begin/append/end を一つの履歴単位とし、preview の Cancel は元状態へ完全復元、OK は一つの Undo 単位にする。
- Undo 後の新規編集では redo branch を破棄する。保存成功時の savepoint を持ち、dirty 判定を file timestamp だけに依存させない。
- 長時間処理は進捗と cancellation を持ち、cancel、failure、stale revision で部分結果を commit しない。
- 同じ状態と入力から同じ結果を返す。tile 順、thread 数、hash iteration 順で画像結果を変えない。

Rust は Direct2D command ではなく immutable render snapshot を生成する。snapshot には raster tile/vector/text/overlay、revision、dirty rect と cache invalidation 情報を含め、font 解決と GPU resource は frontend に任せる。Core の可変参照を snapshot から露出しない。

## 5. C ABI、所有権、並行処理

公開 API は `include/inkpod/core_ffi.h` に置き、opaque Core handle、opaque snapshot、ABI version、create/dispatch/build snapshot/view/release/destroy を基本形とする。名称は変更できるが、ownership、lifetime、thread 規則を header と `docs/ffi.md` に明記する。

- 公開値は `#[repr(C)]` と固定幅整数を使う。structure size と feature flags を持たせる。
- Rust の `Vec`、`String`、slice、trait object、native enum layout、および C++ STL、reference、exception を ABI に出さない。
- 文字列は UTF-8 pointer + byte length、配列は pointer + count とし、必要な stride/capacity を明示する。
- 確保した側が解放する。Rust 所有 object には対応する Rust release 関数を用意する。
- NULL、alignment、length、enum、structure size、integer overflow を境界で検証する。
- exported Rust function は panic を捕捉して status/diagnostic に変換する。C++ exception も ABI 境界を越えさせない。
- error text は caller buffer の二段階 API 等で取得し、共有 global mutable string を使わない。
- 高頻度データは batch/span/snapshot 単位で渡し、sample、pixel、path element ごとに FFI 往復しない。
- `InkpodCore` は原則 single-writer。immutable snapshot だけを release まで renderer thread から読めるようにする。
- Core が lock 保持中に C++ callback を呼ばない。worker 結果は queue と revision 検査を経て commit する。
- C header と Rust 宣言の drift を CI で検出する。

## 6. 保存、外部入力、互換性

native extension は `.inkpod` とし、versioned manifest と圧縮可能な blob を分離する。manifest には format version、UUID、寸法、DPI、色空間、frame、layer/plane tree、blob length/checksum を含め、形式と migration を `docs/file-format.md` に記録する。

- 保存は同一 volume の temporary file を完成・flush・close してから置換する。元 file を先に truncate しない。
- autosave/recovery/export と通常保存を区別し、autosave 成功だけで通常 savepoint を進めない。
- decoder は path traversal、zip bomb 相当、巨大寸法・個数、重複 ID、循環参照、checksum 不一致、不正 UTF を拒否する。
- 未知の必須 feature は拒否し、未知の任意 metadata は可能な範囲で round-trip する。
- PNG/TIFF/TGA/BMP 等の既知形式を先に実装する。DGA/CEL は権利上利用可能な実 fixture と独立検証が揃うまで `Unknown`/`Experimental` とし、互換出力を有効にしない。
- Windows file dialog、shell drop、clipboard 接続は C++、decode/encode と document 変換は Rust を基本とする。
- app 内 clipboard は layer/plane type、document 座標、selection bounds を保持し、外部向け標準形式も提供する。

## 7. Win32 と renderer

- `wWinMain` と wide-character API を使い、`InitCommonControlsEx`、COM、renderer、Core の初期化失敗を安全に unwind する。
- menu、toolbar、shortcut、context menu は同じ command ID と enable/checked state を共有する。
- `WM_COMMAND` 等の message handler を責務別に分け、worker thread から `HWND` を直接操作しない。
- pen/mouse/touch は可能な範囲で `WM_POINTER` を用い、pressure/tilt のない mouse fallback を持つ。
- RAII で COM/GPU resource を管理する。resize、occlusion、minimize、DPI change、device removed/reset から復旧する。
- device lost 時は GPU resource だけを再構築し、Rust の document state を失わない。描画/Present の失敗を無視しない。
- UI thread で大きな decode、filter、save を同期実行しない。非表示・最小化時は不要な描画を止める。
- pan/zoom は GPU cache を再利用し、変更 tile だけ upload する。毎 frame の CPU 全面合成を避ける。
- UI string は resource に集約し、日本語・英語、high DPI、high contrast、keyboard navigation、アクセシビリティを考慮する。

機能の正確な GUI、tool、fill、selection、filter、light table、batch の挙動は `PROMPT.md` の「内蔵機能仕様」を参照する。このファイルへ再掲しない。

## 8. 安全性と品質

- `unsafe` は FFI と証明可能な hot path に局所化し、各 block に safety invariant を記述する。
- allocation、寸法、stride、圧縮後 size、文字列長等に上限を設け、任意入力で panic、範囲外 access、制御不能な OOM を起こさない。
- container、decoder、FFI に malformed tests と fuzz target を用意する。
- user path、画像内容、未保存 document を log へ無制限に出さない。
- 最適化は再現可能な benchmark と before/after に基づく。画質低下は暗黙に行わず明示設定にする。
- placeholder、常時成功する stub、未接続 button、コンパイルしない巨大雛形を完成扱いしない。
- 既存のユーザー変更を上書きせず、対象外の refactor や formatting を混ぜない。

## 9. 検証と完了条件

変更範囲に応じて少なくとも次を実行する。

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cmake --preset <windows-preset>
cmake --build --preset <windows-build-preset>
ctest --preset <windows-test-preset>
```

実際の preset 名を使う。非 Windows 環境でも Rust 検証を完了し、Win32 は Windows CI で検証する。実行できなかった検証を隠さない。

必要なテストは次を含む。

- Core: coordinate、tile indexing、selection algebra、Undo/Redo、serialization round-trip、fill/filter/transform/composite の unit/property/golden test
- format: `.inkpod` round-trip、migration、malformed/cancel test
- ABI: header の C11/C++20 include、ownership、NULL/短い structure/未知 enum/二重 release の negative test
- Windows: MSVC `/W4 /permissive-`、create/render、resize/DPI/device lost smoke test
- CI: Rust と Windows の configure/build/test。新規 warning を放置しない

ピクセル完全一致でない処理も、色空間、rounding、境界条件を固定して小さな明示 tolerance を使う。第三者作品を golden fixture に使わない。

機能を完了扱いできるのは、次をすべて満たす場合だけである。

- UI から Core まで動く縦切り、または明示された Core-only milestone になっている
- success、no-op、invalid、cancel、Undo/Redo、必要な save/reopen をテストしている
- ABI ownership、lifetime、thread 規則を文書化している
- `docs/compatibility.md` の requirement、状態、test、既知差分を更新している
- `docs/implementation-status.md` の acceptance criteria と検証記録を更新している

互換状態は `Not started`、`In progress`、`Experimental`、`Verified` のいずれかとし、test がない機能を `Verified` にしない。

## 10. エージェントの作業手順

1. `git status`、既存差分、`PROMPT.md`、status 文書、対象 code/test を確認する。
2. ユーザー変更を保護し、`PROMPT.md` と status から依存関係を満たす最初の未完了 milestone を選ぶ。
3. 大きな変更を model/ABI、Core、Windows adapter、test、document の小さな縦切りへ分ける。
4. 短い計画を示した後、計画だけで止まらず現在 milestone を実装・検証する。
5. 仕様が決められない場合は `Unknown` として fixture、期待出力、ユーザー判断の必要性を記録し、互換挙動を捏造しない。
6. format、lint、test、build を実行し、status/compatibility 文書を更新する。
7. 最終報告では、利用者向け挙動、重要な設計判断、変更 file、検証結果、未検証事項、既知差分を簡潔に示す。

commit、push、PR、外部公開はユーザーが明示的に依頼した場合だけ行う。
