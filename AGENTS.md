# inkpod 開発ガイド

## 1. このファイルの役割

この指示はリポジトリ全体に適用する。inkpod は、サポートが終了した PaintMan のアニメーション彩色ワークフローを、保守可能なクロスプラットフォーム設計で再構築するプロジェクトである。

ここでいう「合理的な互換性」は、旧アプリの画面をピクセル単位で模写することではない。主線を保護しながら彩色するデータモデル、セル・レイヤー・プレーン・基準フレーム、色・選択・ライトテーブル・連番・バッチの操作意味、座標を維持するコピー＆ペースト等を再現することを意味する。OS 固有の外見は Windows 11 の標準 UI に合わせる。

旧ワークフローから正規化した機能仕様は、`PROMPT.md` の「内蔵機能仕様」を正本とする。通常の実装では外部 PDF や掲載画像を参照せず、同節に定義された操作意味、データ分離、座標、結果を実装・テストする。旧製品の画像、アイコン、文面、商標表示を複製してはならない。旧形式のバイナリ仕様など、`PROMPT.md` で未確定とした事柄を推測で「互換」と称してはならない。

## 2. 絶対に守る境界

### 2.1 責務分担

- Rust Core は計算、状態、画像データ、履歴、ファイル形式、入力の意味解釈、描画スナップショットを担当する。
- C++/Win32 は `WinMain`、メッセージループ、`HWND`、COM、ファイルダイアログ、クリップボード、スレッド連携、DPI、テーマ、アクセシビリティ、デバイスロスト等の OS 統合を担当する。
- Common Controls v6 はメニュー、ツールバー、ステータスバー、タブ、リスト、ツリー、フォーム、設定ダイアログ等の通常 UI を担当する。
- 独立した Canvas `HWND` は Direct3D 11、DXGI swap chain、Direct2D device context、DirectWrite、必要な WIC 連携を所有する。
- CMake をビルド全体の唯一の入口とし、Cargo の Rust `staticlib` ビルドも CMake のカスタムターゲットから起動する。
- Rust と C++ は Rust ABI や C++ ABI ではなく、バージョン付き C ABI だけで接続する。

### 2.2 Rust Core に入れてはいけないもの

Rust Core には次を持ち込まない。

- `HWND`、`HINSTANCE`、Windows メッセージ番号
- COM、WinRT、Direct2D、Direct3D、DXGI、WIC、DirectWrite の型
- Common Controls のハンドルや選択状態
- Windows 固有 DPI、テーマ、レジストリ、既知のフォルダー API
- UI スレッドや Windows メッセージループ
- C++ の所有権や STL 型

Rust Core が扱ってよい表示状態は、ズーム率、論理座標のパン、左右・上下反転、ガイド、グリッド、アクティブツールなど、OS に依存しない意味上の状態だけである。物理ピクセル、DPI、ポインターデバイス固有情報は C++ 側で正規化して渡す。

### 2.3 C++ 側を薄く保つ

C++ に画像処理アルゴリズム、レイヤー規則、履歴、選択演算、独自ファイル形式の判断を重複実装しない。C++ 側のコマンドハンドラーは入力を C ABI のイベントまたはコマンドへ変換し、結果を標準コントロールとレンダラーへ反映するところまでに留める。

## 3. 想定ディレクトリ構成

実際の進捗に合わせて小さく始めてよいが、責務は次の境界に収める。

```text
/
├── AGENTS.md
├── PROMPT.md
├── CMakeLists.txt
├── CMakePresets.json
├── rust-toolchain.toml
├── cmake/
│   └── RustCore.cmake
├── rust/
│   ├── Cargo.toml                 # workspace
│   ├── inkpod-core/               # 文書、状態遷移、履歴、スナップショット
│   ├── inkpod-image/              # ラスター、ベクター、選択、フィルタ
│   ├── inkpod-format/             # native 形式と import/export
│   └── inkpod-ffi/                # staticlib と C ABI のみ
├── include/inkpod/
│   └── core_ffi.h
├── apps/windows/
│   ├── CMakeLists.txt
│   ├── app/
│   ├── renderer/
│   ├── ui/
│   ├── resources/
│   └── app.manifest
├── tests/
│   ├── fixtures/
│   ├── golden/
│   ├── ffi/
│   └── integration/
└── docs/
    ├── architecture.md
    ├── compatibility.md
    ├── file-format.md
    ├── ffi.md
    └── implementation-status.md
```

循環依存を作らない。原則として `inkpod-core -> inkpod-image` と `inkpod-core -> inkpod-format` のような一方向の依存にし、`inkpod-ffi` は公開 API を薄く変換するだけにする。形式クレートからアプリ状態へ逆依存させる必要がある場合は、直列化用 DTO を境界に置く。

## 4. ビルドとツールチェーン

- Rust は stable toolchain、edition 2024 を基本とする。特定 nightly 機能へ依存しない。
- Windows コードは MSVC の C++20 と Unicode API を使う。ANSI 版 Win32 API を使わない。
- CMake は使用する機能を満たす最小バージョンを明示し、Visual Studio 2022 の x64 構成を提供する。
- Rust の `[lib] crate-type = ["staticlib"]` は `inkpod-ffi` だけに設定する。
- MSVC ランタイムは Rust の MSVC ターゲットと整合する `/MD` 系を全ターゲットで一貫して用いる。
- `app.manifest` に Common Controls v6 と Per-Monitor DPI Awareness v2 を宣言する。
- Windows アプリは少なくとも `user32`, `gdi32`, `comctl32`, `ole32`, `shell32`, `shlwapi`, `d2d1`, `dwrite`, `dxgi`, `d3d11`, `windowscodecs` を必要な範囲だけリンクする。
- 非 Windows 環境でも Rust のビルドとテストは実行できるようにする。トップレベル CMake は非 Windows で Win32 ターゲットを明確にスキップし、Rust Core の検証を壊さない。
- 依存ライブラリは必要最小限とし、GPL-3.0 と配布形態に適合するライセンスを確認して `THIRD_PARTY_NOTICES` または同等の記録を更新する。
- ルートの CMake/Cargo コマンドを README と CI の双方で同じにする。開発者だけが知る手動コピーやローカル絶対パスをビルドへ埋め込まない。

## 5. Rust のドメインモデル

### 5.1 座標と色

- ラスターの幅・高さ・インデックスには境界検査された固定幅整数を使う。
- ベクター形状、ガイド、基準フレーム、変形には文書論理座標を使う。画面座標や DIPs と混同せず、型または明示名で区別する。
- 変換は `document -> view logical -> device` の向きを明文化し、逆変換をテストする。
- 文書色は少なくとも sRGB の RGBA 8 bit/16 bit を損失なく表せる。Core 内の標準表現と、合成時の premultiplied/straight alpha の違いを型で区別する。
- 2 値、階調、RGBA、選択マスクを一つの無差別なバイト配列として扱わない。`PixelFormat` と stride を検証する。

### 5.2 文書、カット、セル

永続化可能なモデルは少なくとも次を表現する。

- Project/Cut: 作品、シーン、カット、セルフォルダー、連番規則、既定用紙、100 フレーム基準、メモ
- CellDocument: ID、名前、用紙サイズ、解像度、色深度、作画フレーム、安全フレーム、基準フレーム、余白、dirty/savepoint
- Layer: 安定 ID、名前、種類、表示、編集可否、不透明度、順序、出力対象
- Plane: 安定 ID、名前、種類、表示、編集可否、プレーン色、画像またはベクターデータ
- Workspace state: アクティブレイヤー・プレーン、選択、描画色、パレット、ツール設定、ガイド、グリッド、ライトテーブルセット、表示状態
- Sequence: セル番号、前後関係、欠番、表示範囲、再生 FPS

名前を配列添字や永続 ID の代用にしない。並べ替え、複製、削除を行っても参照が壊れない安定 ID を使う。

### 5.3 レイヤーとプレーンの型

最低限、次の意味を表現できる設計にする。

- 2 値彩色レイヤー: 主線プレーン、彩色プレーン、任意のラスタープレーン
- 階調彩色レイヤー: 階調主線プレーン、境界線を保持する彩色プレーン、任意のラスタープレーン
- ベクター彩色レイヤー: ベクター主線、色ごとの色トレース線、ベクター塗り、任意のラスタープレーン
- ラスター汎用レイヤーとアルファチャンネル
- フレーム、消失点、選択範囲、調整、テキスト、指示の各非画像または補助レイヤー

すべての組み合わせを許すのではなく、レイヤー種類ごとに作成可能なプレーン、統合可能条件、変換可能条件を検証する。プレーンの色が異なる場合に同種統合できない等、互換上意味のある制約をテストする。

### 5.4 主線保護と白透過

- 彩色モードでは主線を表示しても、彩色操作が主線プレーンを書き換えてはならない。
- 階調主線は「coverage + 基本色」として扱える設計にし、スポイトは中間ピクセル色ではなく基本色を返せるようにする。
- 階調彩色の塗り境界は主線表示ではなく、彩色プレーンが持つ境界情報に基づく。
- 旧ワークフローの「純白 RGB(255,255,255) を透明として扱う」挙動は互換表示・検査・import/export オプションとして明示する。ネイティブ形式では白と透明を無条件に同一視せず、alpha を正しく保持する。

### 5.5 大画像とメモリ

- ラスターは既定 256 x 256 程度のタイルへ分割し、空タイルを割り当てず、変更タイルだけをコピーする copy-on-write を基本にする。
- タイルサイズは定数へ散在させず、形式とベンチマークにより変更可能にする。
- Undo、snapshot、light table で文書全体を毎回複製しない。`Arc` 等で不変タイルを共有し、変更前後の参照または可逆 command delta を履歴に保持する。
- 連続ストロークは pointer sample ごとではなく、stroke begin/append/end を一つの履歴トランザクションとして確定する。
- allocation failure、寸法オーバーフロー、stride オーバーフローを通常のエラーとして扱う。

## 6. コマンド、履歴、プレビュー

- UI 操作は型付き `Command` または `CoreInput` へ変換し、ドメイン状態を直接つまみ書きしない。
- すべての破壊的編集はトランザクションとして成功時だけ commit する。
- Undo/Redo は分岐を明確にし、Undo 後の新規編集で redo branch を破棄する。
- 保存成功時の savepoint を持ち、dirty 判定をファイル時刻だけへ依存させない。
- フィルタ、変形、調整値のダイアログは preview transaction を使う。Cancel はビット単位で元に戻し、OK/実行は一つの Undo 単位にする。
- 「復帰」は最後の保存状態へ戻し、「レイヤーを部分的に復帰」は選択範囲と対象プレーンだけを保存状態から復元する。
- 長時間処理は cancellation token と進捗を受け取り、途中キャンセルで半端な状態を commit しない。

## 7. 描画スナップショット

Rust は Direct2D 命令ではなく、次のような OS 非依存の immutable snapshot を生成する。

- 背景、用紙、フレーム、透明表示、グリッド、ガイド
- 可視レイヤー順に並んだ raster tile と世代番号
- ベクターパス、塗り、線幅、色、変換
- テキスト内容、フォント要求、位置、色。フォント解決はフロントエンドが行う
- 選択境界、ツールプレビュー、カーソル、消失点、ライトテーブル等の overlay
- dirty rectangle とキャッシュ無効化に使う revision

高頻度データを要素ごとに FFI 呼び出ししない。snapshot は一括取得し、配列は連続メモリまたは明示的な span view として渡す。snapshot の順序は同じ状態・同じ入力なら決定的でなければならない。

## 8. C ABI 契約

### 8.1 基本形

公開ヘッダーは `include/inkpod/core_ffi.h` に置く。少なくとも次の形を持つ。

```c
typedef struct InkpodCore InkpodCore;
typedef struct InkpodSnapshot InkpodSnapshot;

uint32_t inkpod_core_abi_version(void);
InkpodStatus inkpod_core_create(
    const InkpodCoreConfig* config,
    InkpodCore** out_core);
InkpodStatus inkpod_core_dispatch(
    InkpodCore* core,
    const InkpodInputBatch* input,
    InkpodUpdateResult* out_result);
InkpodStatus inkpod_core_build_snapshot(
    InkpodCore* core,
    const InkpodSnapshotRequest* request,
    InkpodSnapshot** out_snapshot);
InkpodStatus inkpod_snapshot_view(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotView* out_view);
void inkpod_snapshot_release(InkpodSnapshot* snapshot);
void inkpod_core_destroy(InkpodCore* core);
```

名称は進化させてよいが、所有権と lifetime はこの程度に明確でなければならない。

### 8.2 必須規則

- 全公開 Rust 構造体と enum 表現は `#[repr(C)]` または固定幅整数で定義する。
- opaque handle 以外に Rust の `Vec`, `String`, slice, trait object, enum layout を公開しない。
- C++ の STL、例外、参照型を C ABI に公開しない。
- 文字列は UTF-8 の pointer + byte length とし、NUL 終端の有無を関数ごとに揺らさない。
- 配列は pointer + element count。stride または capacity が必要なら明示する。
- Rust が確保したものは Rust の release 関数で解放する。C++ が確保した caller buffer は C++ が解放する。
- NULL、alignment、長さ、enum 範囲、整数オーバーフローを境界で検証する。
- Rust panic は全 exported function で `catch_unwind` し、status と診断へ変換する。panic を ABI 外へ出さない。
- C++ 例外を ABI 呼び出しの外へ出さない。Win32 層は status をユーザー向けエラーへ変換する。
- ABI version、structure size、feature flags を用意し、古いフロントエンドが新しい構造体を誤読しないようにする。
- 最終エラー文の取得は caller buffer の二段階 API 等で実装し、グローバルな可変文字列を共有しない。
- C ヘッダーと Rust 宣言の drift を CI で検出する。生成する場合も、レビュー可能なヘッダーをリポジトリへ含める。

### 8.3 スレッド規則

- `InkpodCore` は原則 single-writer とし、どのスレッドから呼べるかを API ごとに文書化する。
- immutable snapshot は release まで renderer thread から読めるようにしてよいが、Core の可変参照を snapshot へ露出しない。
- Core がロックを保持したまま C++ callback を呼ばない。可能なら callback 自体を ABI に設けず、pollable result queue を使う。
- ワーカーで decode/filter/save した結果の commit は revision を照合し、古い結果が新しい編集を上書きしないようにする。

## 9. ファイル管理と互換性

### 9.1 ネイティブ形式

ネイティブ拡張子は `.inkpod` とする。形式は versioned container とし、少なくとも次を満たす。

- 人間が確認できる versioned manifest と、圧縮可能なタイル・ベクター・サムネイル blob を分ける。
- manifest に format version、document UUID、寸法、DPI、色空間、frame、layer/plane tree、blob の checksum と長さを記録する。
- 未知の必須 feature は明確に拒否し、未知の任意 metadata は可能な限り round-trip する。
- 読み込み時に path traversal、zip bomb 相当、巨大寸法、重複 ID、循環参照、checksum 不一致を拒否する。
- 保存は一時ファイルへ完全に書き、flush/close 後に同一ボリューム上で置換する。既存ファイルへ直接書きかけない。
- 保存失敗時に元ファイルを失わず、autosave/recovery と通常保存を区別する。
- 形式仕様と migration 方針を `docs/file-format.md` に記録し、golden fixture を保持する。

### 9.2 import/export の優先度

1. PNG、TIFF、TGA、BMP の 8/16 bit、alpha、白背景合成、単一セル入出力
2. 連番と cut folder 規則、2 値トレース TGA を使う旧ワークフロー
3. 必要に応じて JPEG 等の参照画像入力。不可逆形式への上書き保存は警告する
4. DGA/CEL は実ファイル fixture と権利上利用可能な仕様が揃ってから codec として実装する

DGA/CEL の名称やレイヤー概念をモデル化することと、旧バイナリを完全に読み書きできることは別である。fixture による round-trip、破損系テスト、既知アプリとの比較がない段階で、互換書き出しを有効にしない。未対応形式は明確な status を返し、データを推測して破損させない。

### 9.3 ファイルとクリップボードの意味

- Windows のファイルダイアログと shell drop は C++、decode/encode と document 変換は Rust を基本とする。
- Windows UTF-16 path は C++ で検証して UTF-8 に変換する。不正 surrogate は黙って置換せずエラーにする。
- コピー内容には画像だけでなく元の layer/plane type、文書座標、選択 bounds を持たせる。
- アプリ内 clipboard format では元座標を維持し、外部向けには `CF_DIBV5` 等の標準形式も提供する。
- 通常 paste はコピー元と同じ属性の plane を優先し、「選択プレーンへ paste」は明示的に型変換して現在の plane へ入れる。
- 階調主線同士の paste 合成は互換モードで比較暗を再現する。

## 10. 機能別の実装規則

### 10.1 表示とキャンバス

- zoom、範囲 zoom、等倍、全体表示、pan、左右・上下反転を view transform として実装し、画像データを変更しない。
- ruler、guide、grid は文書論理座標で保持し、表示倍率による間引きと snap 計算を分離する。
- 透明表示色は設定可能にし、checkerboard または互換の単色表示を snapshot で表す。
- color locator はカーソル周辺の拡大表示、X/Y、選択の H/V/対角長、RGBA を表示し、固定時は locator 上の編集座標を正しく逆変換する。
- multi-view は一つの文書状態を複数 viewport から観察する。編集履歴を view ごとに複製しない。
- full screen、palette 整頓、パレット表示切替は Win32 workspace の責務だが、保存可能な layout setting として扱う。

### 10.2 線修正・描画

- 鉛筆はラスターで 1 document pixel、階調で antialias coverage を扱い、ベクター彩色では無効にする。
- auto erase は stroke 開始 pixel が描画色と一致した場合に stroke 全体を erase mode にし、Shift で抑止する。
- 消しゴムはラスターの局所消去と、ベクターの「触れた部分」「交点まで」「線全体」を別 command として実装する。
- 直線、曲線、長方形、楕円、多角形、折れ線は preview 中に文書を書き換えず、確定時に一度だけ commit する。
- 45 度制約、中心から作成、縦横比固定、吸着、入り・抜き、可変線幅をオプションとして表現する。
- 線つなぎは対象範囲内の gap 候補と閾値を決定的に処理し、元に戻せる追加線または vector edit とする。
- 線幅修正は加算、減算、倍率、一定幅を区別し、ラスター morphology と vector width edit を同じ関数で無理に処理しない。
- ゴミ取りは connected component の最大寸法/面積と周囲色ルールを明文化し、ベクターでは無効にする。

### 10.3 フィル

フィルは互換性の最重要機能である。少なくとも次を別々にテストする。

- seed pixel と同色の連結領域
- 8/16 bit color tolerance と alpha の扱い
- 指定色/指定色以外および最大 6 色相当の「含み塗り」
- 離れた同色領域も塗る global variant
- gap closing。元画像へ恒久線を加えず仮想境界として評価するか、追加線として commit するかを API で区別する
- overflow abort。領域が画像外周へ到達する等の漏れ条件を検出した場合、部分結果を commit しない
- light table の線を read-only 境界として使う
- light table の対応座標の色を描画色として使う
- selection 内の全閉領域を一括処理する閉領域フィル
- 透明部分だけを塗る設定
- 狭い先端へ既存色を伸ばす塗りのばし

アルゴリズムは入力タイル順やスレッド数で結果が変わらないようにする。巨大領域で再帰 flood fill を使わず、明示 queue/scanline と上限検査を使う。

### 10.4 色管理

- 描画色は RGBA 8/16 bit と RGB/HSV editor の双方から設定できる。
- eyedropper は「最上位の非透明 plane」「選択 plane」「合成表示色」「light table」を区別する。
- palette、color chart、sub palette は永続化でき、ロック、検索、前後セル参照、取得色の登録を表現する。
- color chart の旧ファイル形式は独立 codec とし、形式が不明な間はネイティブ JSON 等へ保存して旧拡張子を名乗らない。
- NTSC/放送色域チェックは用いた規格、変換式、閾値を文書化し、単なる RGB clamp としない。

### 10.5 レイヤー、選択、変形

- layer/plane の作成、複製、削除、表示、編集可否、不透明度、順序変更、変換、同種統合を command 化する。
- 選択は 8 bit mask または等価な精度を持ち、new/add/subtract/intersect/invert を正しく行う。
- 矩形、楕円、投げ縄、折れ線、なぞり、magic wand、描画色選択、selection expand/shrink を実装する。
- vector selection は部分切断、一部接触、完全包含、線全体、交点まで、塗り、塗りを囲む線を個別に表現する。
- selection layer との相互変換・追加・削除は mask の round-trip を保証する。
- 画像全体の鏡像/90 度回転/用紙サイズ/解像度変更と、view の反転/回転を別 API にする。
- 部分 transform は floating selection として移動・拡縮・回転を preview し、確定または cancel できる。
- resampling method、alpha edge、pixel center の定義をテストで固定する。

### 10.6 フィルタ、特効、調整レイヤー

- sharpen 強/弱、unsharp mask、blur 強/弱、Gaussian blur、invert、auto contrast、brightness/contrast、tone curve、levels、hue/saturation/value、color balance を pure function として実装する。
- filter は selection があればその内側、なければ active plane 全体に適用する。
- 8/16 bit の丸め、境界条件、alpha をフィルタごとに定義する。
- tone curve は RGB/R/G/B、B-spline と Bézier、level は input shadow/gamma/highlight と output range を保持する。
- adjustment layer は元画像を変更せず、順序に応じて composite へ作用し、設定を後から編集できる。
- gradient は 3 色以上、各 stop の alpha、線形/放射、合成/上書き、dither を扱う。
- airbrush、blur tool、stamp、境界だけを処理する airbrush effect は通常の全体 blur と混同しない。
- alpha edit mode では対象 raster plane の alpha channel だけを編集する。

### 10.7 ライトテーブルと連番

- 複数画像を set として保持し、各 item に path/document ID、表示、変換、個別 opacity、表示色、表示方法、対象 layer を持たせる。
- 実効 opacity は item opacity と global opacity の積である。
- 基準フレームを用いて異なる用紙サイズを自動整列する。単純な画像左上合わせにしない。
- 前後画像登録、画像入替、更新、位置リセット、数値位置、カラー/モノトーン/ハーフトーン、重なりの透過を表現する。
- light table item は編集文書へ明示的に入れ替えるまで read-only とする。
- motion check は範囲、倍率、背景/余白色、開始 pause、selection のみ、light table 表示、FPS、コマ送り、loop/cancel を扱う。

### 10.8 バッチ

- batch graph は Input -> ordered Operations -> Output として永続化する。
- input は単一ファイル、フォルダー、現在セルを含む連番を扱う。
- operation は少なくとも線幅、連続フィル、色置換、layer 表示、分離、airbrush effect、主要 filter、サイズ/解像度、鏡像、回転、layer 変換を表現する。
- operation ごとに対象 layer selector、設定 version、validation を持つ。
- 出力は新規、複製、明示上書きを区別する。既定で入力を破壊しない。
- 1 ファイルごとに atomic commit し、失敗、skip、cancel を結果一覧へ残す。途中の一件失敗で既に成功した別出力を破損させない。
- preview と dry-run を提供し、座標ベースの連続フィルは各フレームで seed が有効か事前検査できる。

## 11. Win32 実装規則

- `wWinMain` と wide-character Win32 API を使う。
- `InitCommonControlsEx` を起動時に一度呼び、manifest で Common Controls v6 を有効にする。
- 最低限 `WM_CREATE`, `WM_COMMAND`, `WM_NOTIFY`, `WM_SIZE`, `WM_DPICHANGED`, `WM_PAINT`, `WM_THEMECHANGED`, `WM_SETTINGCHANGE`, `WM_CLOSE`, `WM_DESTROY` を責務別に処理する。
- pen/mouse/touch は可能な範囲で `WM_POINTER` 系を使い、pressure/tilt の欠落する mouse fallback を持つ。
- COM は UI thread で適切に初期化し、RAII (`wil::com_ptr` または `Microsoft::WRL::ComPtr` 等) で管理する。
- renderer は BGRA 対応 D3D11 device、DXGI surface、D2D device context、swap chain を生成し、resize、occlusion、minimize、device removed/reset から復旧する。
- `BeginDraw/EndDraw`、swap chain `Present` の失敗を無視しない。device lost 時は GPU resource だけを捨て、Rust の文書状態を失わない。
- UI thread で decode、filter、巨大 save を同期実行しない。完了通知には private window message 等を使い、worker から `HWND` を直接操作しない。
- keyboard shortcut は command ID と設定を分離し、競合時は旧割当を解除する。reset で既定値へ戻せるようにする。
- UI 文字列は resource/string table 等に集約し、日本語と英語を追加できるようにする。
- 旧 PaintMan のアイコン、スクリーンショット、配色、文言をそのまま使用しない。Windows 11 の標準操作、high contrast、keyboard navigation を尊重する。

## 12. 性能、堅牢性、セキュリティ

- pan/zoom 中は既存 GPU tile を再利用し、文書全体を CPU で毎 frame 合成しない。
- snapshot には tile revision と dirty rect を含め、変更 tile だけ GPU upload する。
- brush sample、tile、vector command を batch で FFI 渡しする。
- debug build での正しさを優先し、release だけで成立する unsafe 最適化を先に入れない。
- `unsafe` は FFI と証明可能な hot path に局所化し、各 block に safety invariant を記述する。
- decoder、container、FFI には fuzz target を用意し、任意入力で panic、OOM 誘発、範囲外アクセスを起こさない。
- 外部ファイルを信用せず、寸法・個数・圧縮後サイズ・パス・ID・文字列長へ上限を設ける。
- ログに画像内容、ユーザーパス、未保存文書を無制限に出さない。
- 性能改善には再現可能な benchmark と before/after を添える。画質を下げて速くした場合は明示的な品質設定にする。

## 13. テストと検証

### 13.1 Rust

- `cargo fmt --check`
- workspace 全体の `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- property test: selection boolean、座標変換、tile indexing、Undo/Redo、serialization round-trip
- golden test: fill、gap close、line correction、transform、filter、composite
- malformed input と cancellation のテスト

ピクセル完全一致が仕様でない処理は、勝手な大きい許容差を設けず、色空間と丸めを固定した上で小さな明示 tolerance を使う。

### 13.2 ABI と C++

- `core_ffi.h` を C11 と C++20 の両方で include できる smoke test
- create/dispatch/snapshot/release/destroy の ownership test
- 不正 NULL、短い structure、未知 enum、二重 release を安全に処理する negative test
- MSVC `/W4 /permissive-`。新規警告を放置しない
- Windows CI で CMake configure/build/test を Debug と Release の少なくとも一方ずつ検証する
- renderer の device lost/resize/DPI は可能な限り seam を作って unit test し、実ウィンドウ smoke test も持つ

### 13.3 互換性と完了条件

`docs/compatibility.md` に機能 ID、`PROMPT.md` の仕様節または requirement ID、期待挙動、実装状態、テスト、既知差分を記録する。状態は `Not started`, `In progress`, `Experimental`, `Verified` のいずれかとし、テストがない機能を `Verified` にしない。

変更した機能の完了条件は次の全てである。

- UI から Rust Core までの縦の経路が動く、または Core-only milestone と明記されている
- success、no-op、invalid input、cancel、Undo/Redo、save/reopen を必要な範囲でテストしている
- メモリ所有権と thread 規則を文書化している
- placeholder、常時成功する stub、未接続ボタンを完成扱いしていない
- 対応表と実装状態を更新した
- 実行した検証コマンドと、実行できなかった Windows 固有検証を報告した

## 14. 自己完結仕様の参照マップ

実装・テスト設計では、外部資料ではなく `PROMPT.md` の「内蔵機能仕様」を参照する。各領域の基準節は次のとおりである。

| 領域 | `PROMPT.md` の節 | 特に固定する挙動 |
|---|---:|---|
| 用語と基本データ | 1, 4, 5 | 主線保護、カット/セル、2 値/階調/ベクター、保存、自動保存 |
| Windows UI とメニュー | 2, 3 | GUI 領域、command ID、menu/toolbar/shortcut の同一 command 化 |
| 用紙・frame・表示 | 7, 8 | 100 frame、基準 frame、zoom/pan/flip、guide/grid snap、multi-view |
| layer/plane | 5, 6 | 型、作成/複製/削除/変換/統合、表示/編集可否、alpha |
| 線修正 | 9 | erase mode、line connect、line width、vanishing point |
| 色 | 10 | eyedropper source、palette/chart/subpalette、色深度 |
| fill と彩色 | 11 | 含み塗り、overflow abort、gap close、closed-region fill、replace/extend |
| check と再生 | 12 | 未彩色検査、色域検査、motion check、連続表示 |
| light table | 13 | set、基準 frame alignment、opacity、表示方式、画像入替 |
| selection/clipboard | 14, 15 | boolean selection、selection layer、座標維持 paste |
| transform/history | 16, 17 | view と実データの区別、floating transform、Undo/Redo/revert |
| filter/effect | 18 | preview、各 filter、adjustment layer、gradient、retouch、alpha |
| batch | 19 | input-operation-output、continuous fill、replace、separate、airbrush |
| 保存/書き出し | 4, 20 | atomic save、native format、PNG/TIFF/TGA/BMP、連番 export |
| 未確定事項 | 21 | proprietary binary、厳密式、旧 UI 細部を推測しない |

## 15. 段階実装

順序は依存関係を守る。後段の UI だけを先に大量に作らない。

### M0: 基盤

- workspace、CMake/Cargo 統合、Windows shell、manifest、CI
- status/error、opaque handle、ABI version、空 snapshot
- architecture/file-format/FFI/compatibility/status 文書

### M1: 保存できる縦切り

- 2 値彩色 CellDocument、主線/彩色 plane、tile store
- 新規、開く、atomic save、reopen、dirty、Undo/Redo
- D3D11/DXGI/D2D canvas に raster snapshot を表示
- mouse/pen stroke、eraser、描画色、zoom/pan/fit/1:1

### M2: PaintMan の中核彩色

- 階調彩色、fill、含み塗り、overflow abort、gap close、closed-region fill
- eyedropper、palette、純白/透明 check、autosave/recovery
- golden fixture とメモリ・性能 benchmark

### M3: 文書編集

- layer/plane 操作、selection 一式、clipboard、transform、frame/guide/grid
- multi-view、color locator、shortcut editor

### M4: 制作ワークフロー

- cut/cell sequence、file preview、light table/subpalette、基準 frame alignment
- motion check、連番 import/export、common raster formats

### M5: ベクター

- path/fill model、vector rasterization snapshot、vector draw/erase/connect/width/select
- vector coloring layer と raster/vector conversion

### M6: 画像編集

- filter、gradient、airbrush、blur、stamp、adjustment layer、alpha edit

### M7: バッチ

- persisted batch graph、preview/dry-run、進捗/cancel、atomic outputs
- continuous fill、color replacement、separation、filter/effect operations

### M8: 互換性の拡張と別 frontend の準備

- 実 fixture に基づく legacy codec
- Core API の platform-neutrality audit
- Windows packaging と、将来の iPad/macOS frontend が必要とする API gap の文書化

各 milestone は動く縦切りで閉じる。後の milestone の型を仮置きしてよいが、未実装機能を成功 status で返さない。

## 16. エージェントの作業手順

1. 変更前に `AGENTS.md`、`PROMPT.md`、既存 status、git diff、対象テストを読む。
2. `docs/implementation-status.md` で最初の未完了 milestone と依存関係を確認する。
3. 大きな変更は、モデル/ABI、Core 実装、Windows adapter、テスト、文書の順に小さな縦切りへ分ける。
4. 仕様が `PROMPT.md` の内蔵機能仕様と既存テストから決められない場合は、実データ fixture やユーザー判断が必要な点を `Unknown` と記録する。互換挙動を捏造しない。
5. 既存のユーザー変更を上書きせず、関係ないリファクタリングを混ぜない。
6. 実装後に format、lint、unit/integration/build を実行する。Windows を実行できない環境では Rust 検証を完了し、Windows CI または未検証事項を明示する。
7. 最終報告は、達成した利用者向け挙動、重要な設計判断、テスト結果、残る既知差分を簡潔に示す。

計画だけ、UI の空ボタンだけ、コンパイルしない巨大な雛形だけで終えてはならない。現在の milestone について安全に実装・検証できる範囲を実際に完成させる。
