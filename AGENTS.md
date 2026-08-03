# inkpod 開発ガイド

## 1. 適用範囲と仕様の正本

この指示はリポジトリ全体に適用する。inkpod は、PaintMan と合理的な互換性のあるアニメーション彩色ワークフローを、長期保守可能なクロスプラットフォーム設計で再構築するプロジェクトである。

このファイルには、全タスクに常時適用する技術境界、品質基準、作業規律、すなわち「どう開発するか」だけを置く。維持する機能、GUI メニュー、利用者向け挙動、要件 ID、すなわち「何を作るか」は `PROMPT.md` を正本とし、ここへ複製しない。実装前に `PROMPT.md` の関連節を読み、現在状態や既知差分に関係する場合だけ `docs/implementation-status.md` と `docs/compatibility.md` の該当箇所を確認する。

指示が競合する場合の優先順位は、今回のユーザー指示、`AGENTS.md`、`PROMPT.md`、テスト済みの既存契約の順とする。外部の旧製品マニュアルや画像を通常の実装時に参照しない。対応するファイル形式は `PROMPT.md` に明記されたものだけとし、未列挙の外部形式を追加しない。旧製品の画像、アイコン、文面、商標表示を複製しない。

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

Windows frontend の所有権は process 単位の `ApplicationHost`、top-level window 単位の `WorkspaceWindow`、論理文書単位の `DocumentSession`、表示単位の `DocumentView`、tab/Canvas slot 単位の `EditorGroup` に分ける。`CoreHost` と `RendererHost` は owner thread 上の registry を持ち、process-global な active document pointer や全機能を知る巨大 context を設けない。同じ文書の view は一つの session/Core handle を共有し、文書状態と view logical state を別 owner・別 revision にする。

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

循環依存を避け、`inkpod-ffi` は公開 API の薄い変換だけにする。形式 crate から application state へ逆依存せず、必要なら serialization DTO を境界に置く。Core の公開 Rust API は C ABI から独立させ、FFI の pointer validation や `#[repr(C)]` 型を domain model へ浸透させない。

- Rust は stable、edition 2024。nightly 固有機能へ依存しない。
- Windows は MSVC C++20 と Unicode API を使い、Visual Studio 2022 または 2026 x64 を検証基準とする。
- `staticlib` は `inkpod-ffi` だけに設定し、MSVC runtime は Rust MSVC target と整合する `/MD` 系で統一する。
- manifest で Common Controls v6 と Per-Monitor DPI Awareness v2 を有効にする。
- build にローカル絶対 path、手動 file copy、開発者個人だけの前提を埋め込まない。
- 非 Windows でも Rust の build/test を可能にし、Win32 target は明示的に skip する。
- 依存は必要最小限とし、配布ライセンスを確認して third-party notice を更新する。
- `lib.rs` と `mod.rs` は module declaration と意図した re-export を中心にし、production logic を置かない。責務で module を分け、便宜的な `helpers`、`common`、`utils` や循環依存を作らず、visibility を必要最小限に保つ。

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
- fill は再帰を使わず、scanline または上限付き明示 queue で selection、tile boundary、訪問数を検査する。color distance、alpha、rounding を固定し、gap close は仮想境界または別 transaction、overflow abort は all-or-nothing とする。

同期 document edit は共通 transaction 境界を通す。transaction は開始時の document と base revision、作業状態、commit revision を保持し、作業状態だけを変更する。commit 前に stale base と overflow を検査し、明示的な一回の commit だけが document、revision、history、dirty、cache invalidation を同時に公開する。`Drop` で commit しない。意味上の no-op は revision、history、dirty、render content を進めない。長寿命 preview/stroke/floating selection と一回の同期 edit を同じ transaction 型へ押し込まない。

stable ID、document/view/render/preview revision、history state は意味ごとの newtype にし、異なる型同士の暗黙変換を許さない。raw 固定幅整数との変換は C ABI、公開互換 API、file DTO の境界へ集約し、C ABI layout を Rust newtype の表現へ依存させない。zero の意味、ID の所属 namespace と lifetime、increment/overflow を型ごとに定義する。

document、view logical、device 座標と point/size/rect/offset/zoom は意味ごとの型を使う。座標変換、flip、rounding、half-open pixel boundary は一か所に集約し、非有限値、極端な zoom/pan、最終 valid pixel、範囲外を検証する。Core の Canvas 変換へ OS DPI を適用しない。

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
- Windows frontend は UI/Input、Core engine、Renderer の三つの長寿命 thread に分ける。`InkpodCore` の create/全操作/destroy は Core engine thread、D3D11/DXGI/Direct2D/swap chain/Present は Renderer thread に固定する。
- UI/Input thread は pointer history を client device-pixel 座標で正規化し、bounded queue へ投入する。描画中に Core や Present を待たず、入力 sample と begin/end/cancel は破棄しない。Renderer は古い未描画 snapshot/frame だけを置換してよい。
- snapshot は所有権を明示した C++ queue で Core engine から Renderer へ渡す。Rust 所有 pointer を `PostMessage` の `WPARAM`/`LPARAM` に裸で積まず、受取側は enqueue 成否にかかわらず release 責務を一意に引き受ける。
- C header と Rust 宣言の drift を CI で検出する。

## 6. 保存、外部入力、フォーマット

native extension は `.inkpod` とし、versioned manifest と圧縮可能な blob を分離する。manifest には format version、UUID、寸法、DPI、色空間、frame、layer/plane tree、blob length/checksum を含め、形式を `docs/file-format.md` に記録する。

ユーザーがフォーマットフリーズを明示的に宣言するまで、native file format と application 固有の永続化ファイル形式に下位互換性を設けない。decoder は現在の version だけを受理し、旧 version 用 migration、互換 reader、互換 writer、互換 shim を追加しない。常に現在の要件に対して最も頑健で効率的な形式を選び、コードフリーズまでは serialized schema を変更するたびに最上位の file format version を必ずインクリメントする。section version だけの変更で最上位 version の更新を代用しない。フォーマットフリーズ後の互換性方針は、その宣言時のユーザー指示で定める。この規則は `.inkpod`、`.inkbatch`、native preset 等のファイルに適用し、HKCU の workspace layout record には適用しない。

- 保存は同一 volume の temporary file を完成・flush・close してから置換する。元 file を先に truncate しない。
- autosave/recovery/export と通常保存を区別し、autosave 成功だけで通常 savepoint を進めない。
- decoder は path traversal、zip bomb 相当、巨大寸法・個数、重複 ID、循環参照、checksum 不一致、不正 UTF を拒否する。
- 未知の必須 feature は拒否し、未知の任意 metadata は可能な範囲で round-trip する。
- 一般画像入出力は `PROMPT.md` に列挙された対応形式だけを実装し、未対応形式の placeholder、disabled entry、拡張子だけの偽装形式を作らない。
- Windows file dialog、shell drop、clipboard 接続は C++、decode/encode と document 変換は Rust を基本とする。
- app 内 clipboard は layer/plane type、document 座標、selection bounds を保持し、外部向け標準形式も提供する。

## 7. Win32 と renderer

- `wWinMain` と wide-character API を使い、`InitCommonControlsEx`、COM、renderer、Core の初期化失敗を安全に unwind する。
- `main.cpp` は起動 mode と application runner に限定し、feature command、dialog、pane、smoke scenario を置かない。private declaration は `apps/windows` 以下に閉じ、公開 C ABI header へ出さない。
- menu、toolbar、shortcut、context menu は同じ command ID と enable/checked state を共有する。
- `WM_COMMAND` 等の message handler を責務別に分け、worker thread から `HWND` を直接操作しない。完了通知が必要なら `PostMessage` で UI thread の queue へ値だけを渡し、window 状態の変更は UI thread が行う。
- command は発行時の immutable な `CommandContext` に workspace/session/view/pane/job の ID と generation を固定する。state query と execution は同じ target 解決を使い、query は副作用を持たず、stale target を現在 active な別文書へ再解決しない。controller は他 controller の private state を直接変更せず、各 command ID は一つの feature owner だけが処理する。
- dialog は typed initial value と typed result だけを受け取り、完全な application state、Core handle、FFI を所有しない。Cancel は caller state を変更しない。
- pen/mouse/touch は可能な範囲で `WM_POINTER` を用い、pressure/tilt のない mouse fallback を持つ。
- Canvas の座標は client device pixel に統一し、`device = document * zoom + pan` を Core snapshot と renderer で共有する。D2D Canvas は pixel unit/96-DPI target とし、Per-Monitor DPI を Canvas transform へ二重適用しない。
- UI の DPI 変換は `device_px = MulDiv(reference_px, target_dpi, reference_dpi)` とする。96 DPI 論理値の `reference_dpi` は 96、スクリーンショット等の実 device pixel は撮影時 DPI とし、同じ値へ DPI scale を二重適用しない。
- RAII で COM/GPU resource を管理する。resize、occlusion、minimize、DPI change、device removed/reset から復旧する。
- device lost 時は GPU resource だけを再構築し、Rust の document state を失わない。描画/Present の失敗を無視しない。
- UI thread で大きな decode、filter、save を同期実行しない。非表示・最小化時は不要な描画を止める。
- pan/zoom は GPU cache を再利用し、変更 tile だけ upload する。毎 frame の CPU 全面合成を避ける。
- UI string は resource に集約し、日本語・英語、high DPI、high contrast、keyboard navigation、アクセシビリティを考慮する。
- 非表示 tab の snapshot build と不要な Present を止め、GPU、thumbnail、reference、light-table cache に application-wide の上限と回収方針を持たせる。resource 使用量を document、view、Canvas、pane と cache category ごとに観測可能にする。
- `--smoke-test` と `--abi-smoke-test` は実製品の UI/Core/renderer/ABI 経路を検証する private entry point として維持する。

機能の正確な GUI、tool、fill、selection、filter、light table、batch の挙動は `PROMPT.md` の「内蔵機能仕様」を参照する。このファイルへ再掲しない。

## 8. 安全性と品質

- `inkpod-core` は safe Rust を維持する。`unsafe` は FFI と証明可能な image hot path に局所化し、各 block に safety invariant を記述する。
- allocation、寸法、stride、圧縮後 size、文字列長等に上限を設け、任意入力で panic、範囲外 access、制御不能な OOM を起こさない。
- container、decoder、FFI に malformed tests と fuzz target を用意する。
- user path、画像内容、未保存 document を log へ無制限に出さない。
- 最適化は再現可能な benchmark と before/after に基づく。画質低下は暗黙に行わず明示設定にする。
- placeholder、常時成功する stub、未接続 button、コンパイルしない巨大雛形を完成扱いしない。
- test failure を削除、ignore、過大 tolerance で隠さない。
- 既存のユーザー変更を上書きせず、対象外の refactor や formatting を混ぜない。

公開 Rust API の rustdoc には、必要に応じて座標系・単位・範囲・境界、ID の所属と lifetime、success/no-op/error、revision/history/dirty/savepoint への影響、ownership、cancellation、panic の有無を記す。invalid input は通常 `CoreError` 等の明示 error にし、test や文書化のためだけの public accessor を追加しない。

公開契約の regression test は public API から観測し、private helper の局所的不変条件だけを実装 file に colocate する。固定 seed、bounded case、失敗時の replay 情報を持つ state-machine/property test で determinism、failure/cancel atomicity、no-op stability、Undo/Redo round-trip、redo branch truncation、revision separation、savepoint、ID integrity を検証する。OS entropy、test 実行順、private field bridge に依存させない。

benchmark は quick/full で同じ scenario と意味上の counter/checksum を使う。共有 CI の wall-clock 絶対値だけで失敗判定せず、同じ machine・profile・入力の複数回中央値で before/after を比較する。重い検証を理由なく ignored test に隠さない。

## 9. 検証と完了条件

変更範囲に応じて少なくとも次を実行する。

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo bench --package inkpod-core --bench core_workflows -- --quick
cargo doc --package inkpod-core --all-features --no-deps
cmake --preset <windows-preset>
cmake --build --preset <windows-build-preset>
ctest --preset <windows-test-preset>
```

実際の preset 名を使い、rustdoc は実行 shell に応じて `RUSTDOCFLAGS=-D warnings` 相当を設定する。非 Windows 環境でも Rust 検証を完了し、Win32 は Windows CI で検証する。実行できなかった検証を隠さない。

必要なテストは次を含む。

- Core: coordinate、tile indexing、selection algebra、Undo/Redo、serialization round-trip、fill/filter/transform/composite の unit/property/golden test
- format: `.inkpod` current-version round-trip、非現行 version 拒否、malformed/cancel test
- ABI: header の C11/C++20 include、ownership、NULL/短い structure/未知 enum/二重 release の negative test
- Windows: MSVC `/W4 /permissive-`、create/render、resize/DPI/device lost smoke test
- Windows hardening: queue saturation、close 中 input、active stroke、stale snapshot、save failure、allocation failure、shutdown race の fault injection、tab/window/layout/device reset の反復 soak、keyboard/UI Automation/high contrast/DPI/screen reader/IME の再現可能な確認
- CI: Rust と Windows の configure/build/test。新規 warning を放置しない

ピクセル完全一致でない処理も、色空間、rounding、境界条件を固定して小さな明示 tolerance を使う。第三者作品を golden fixture に使わない。

機能を完了扱いできるのは、次をすべて満たす場合だけである。

- UI から Core まで動く縦切り、または明示された Core-only scope になっている
- success、no-op、invalid、cancel、Undo/Redo、必要な save/reopen をテストしている
- ABI ownership、lifetime、thread 規則を文書化している
- `docs/compatibility.md` の requirement、状態、test、既知差分を更新している
- 現在状態、既知差分、または代表的な直近検証が変わった場合は `docs/implementation-status.md` を更新している

互換状態は `Not started`、`In progress`、`Experimental`、`Verified`、`Blocked` のいずれかとする。test がない機能を `Verified` にしない。

## 10. エージェントの作業手順

1. `git status`、既存差分、`PROMPT.md` の関連節、対象 code/test を確認し、現在状態や既知差分が関係する場合だけ status/compatibility の該当箇所を読む。
2. ユーザー変更を保護し、今回の依頼に対応する `PROMPT.md` の機能・要件 ID と、status/compatibility に記録された現在状態・既知差分を確認する。
3. 公開契約を test で先に固定し、一つの変更では一種類の意味上の risk だけを扱う。機械的な rename/module 移動、algorithm 変更、公開境界変更を分け、大きな変更を model/ABI、Core、Windows adapter、test、document の小さな縦切りへ分ける。
4. 短い計画を示した後、計画だけで止まらず今回の scope を実装・検証する。
5. 仕様と既存テストだけで安全に決められない場合は推測で実装せず、具体的な選択肢、影響、解除条件を示してユーザー判断を求める。
6. format、lint、test、build を実行し、現在状態、要件 status、既知差分、代表的検証が変わった場合だけ status/compatibility 文書を更新する。
7. 最終報告では、利用者向け挙動、重要な設計判断、変更 file、検証結果、未検証事項、既知差分を簡潔に示す。

commit、push、PR、外部公開はユーザーが明示的に依頼した場合だけ行う。
