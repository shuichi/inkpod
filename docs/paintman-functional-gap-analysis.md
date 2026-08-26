# PaintMan に対する機能ギャップ分析

調査基準: repository commit `11809da3647319ef98b99d903fd9faa5d8932b3b`（2026-08-09）

## 1. 調査範囲と前提

### 1.1 上位互換の定義

本書でいう「PaintMan の機能的上位互換」とは、PaintMan の画面、メニュー名、操作順、旧データ形式を再現することではない。アニメーション彩色の利用者が、PaintMan User Guide に記載された対象内の作業目的を、inkpod で同等以上の結果品質、データ保持、安全性、反復効率で達成できることをいう。複数の PaintMan コマンドを inkpod の一つの汎用機能で合理的に置き換えられる場合は、個別の不足として重複計上しない。

### 1.2 対象と除外

対象は、カット／セル／フレーム管理、型付きレイヤーとプレーン、描画と線修正、色、フィル、選択、クリップボード、変形、表示、ライトテーブル、履歴、フィルター／特効、品質確認、連続作業、バッチである。

次は明示的に除外した。

- PaintMan 固有形式、旧形式、一般画像形式、データ交換形式の読み書き互換性
- タイムシート機能全般
- 旧 UI の配置、外観、アイコン、文言、Windows 固有の旧操作
- 商標、ブランド、問い合わせ情報
- OS、ファイルマネージャー、印刷機能、一般的な制作管理サービスで合理的に代替できるだけの機能

除外事項はギャップ件数に含めていない。タイムシートとセル順／尺、ファイル形式と保存・バッチ出力のように対象内能力と接する場合は、依存関係だけを注記した。

### 1.3 調査資料

- `AGENTS.md`
- `data/PaintMan_UserGuide.pdf`
- `SPEC.md`
- `docs/implementation-status.md`
- `docs/compatibility.md`
- `docs/primitive-route-inventory.md`
- `rust/inkpod-core`、`rust/inkpod-image`、`rust/inkpod-ffi`、`include/inkpod/core_ffi.h`
- `apps/windows/app`、`apps/windows/renderer`、`apps/windows/ui`
- Rust の unit／contract／FFI test と Windows smoke test のソース

PDF は全 191 表示ページを確認した。PDF p2～p189 は原則として見開きで、`印刷左ページ = 2 × PDF表示ページ - 2`、右ページはその次である。本書の主番号は **PDF ビューアーの表示ページ** とし、括弧内に **冊子の印刷ページ** を併記する。例: `PDF表示 p.89（印刷 pp.176–177）`。本文だけでは意味が曖昧な 36 ページをレンダリングし、レイヤー構造、セル作成、描画オプション、消失点、フィル、選択、ライトテーブル、バッチ、撮影フレーム、指示／テキストを視覚確認した。

全章の確認範囲は次のとおりである。

| PDF表示ページ | 章 | 本調査での扱い |
|---|---|---|
| pp.4–10 | 第1章 はじめに | 彩色工程の意味と上位互換の基準 |
| pp.11–20 | 第2章 クイックガイド | 基本彩色、セル切替、保存 |
| pp.21–55 | 第3章 RetasStudio の基本 | カット／セル管理を対象、タイムシート pp.27–45 は除外 |
| pp.56–61 | 第4章 画面表示 | zoom、guide、locator、multi-view |
| pp.62–70 | 第5章 用紙とフレーム | 新規セル、用紙、基準 frame |
| pp.71–84 | 第6章 レイヤー・プレーン | 型、構造、操作、合成順 |
| pp.85–112 | 第7章 彩色 | 描画、色、fill、QA、motion |
| pp.113–120 | 第8章 ライトテーブル | set/item、参照操作、前後 N 枚 |
| pp.121–135 | 第9章 選択範囲 | raster mask、clipboard |
| pp.136–152 | 第10章 画像の編集 | transform、history、filter、effect、adjustment |
| pp.153–161 | 第11章 バッチ処理 | graph、連続 fill、replace、separation |
| pp.162–167 | 第12章 指示・テキスト | 撮影 frame、annotation、text |
| pp.168–173 | 第13章 完成・出力 | 形式／出力は除外、QA 依存だけ確認 |
| pp.174–178 | 第14章 環境設定 | 意味を持つ policy と旧環境設定を分離 |
| pp.179–187 | 第15章 付録 | layer 型制約と全項目を再照合 |
| pp.188–191 | 索引／問い合わせ／奥付 | 索引で機能漏れを再確認、brand は除外 |

### 1.4 実装済み判定

`SPEC.md` の記載や `docs/compatibility.md` の `Verified` 表記だけでは実装済みとしなかった。判定規則は次のとおりである。

- `Implemented and verified`: production の Core／FFI／Windows 経路と、利用者が観測する結果を検査するテストを確認した。
- `Implemented but unverified`: 実処理は確認できるが、該当意味を直接検査するテストが不足する。
- `Specified only`: `SPEC.md` に能力はあるが、実データモデル、処理、FFI、または利用可能な frontend 経路を確認できない。
- `Partial`: 能力の一部、対象型、条件、または層だけを満たす。
- `Missing`: PaintMan にはあるが、`SPEC.md` にも実装にも意味上の能力がない。
- `Equivalent by another workflow`: UI／手順は異なるが、合理的な手順で同じ結果を得られる。
- `Out of scope`: 今回の明示的除外。
- `Unable to assess`: PDF または実装証拠が不足し、推測なしでは確定できない。

本調査では `cargo test --workspace --all-features` を実行し、全 workspace test と doctest が成功した。Windows CTest と実機ペンタブレット試験は再実行していない。既存テストが成功することと、テスト対象になっていない詳細能力が実装済みであることは分けて扱った。

## 2. エグゼクティブサマリー

### 2.1 総合判定

2026-08-12時点で、本調査から起票した対象内22群の機能ギャップは、CoreからWindows production routeまでの縦切り、自動検証、指定された利用者確認をすべて完了した。したがって、この22群を対象とする段階的解消計画は完了している。ただし、§7に分離した物理ペン等の追加観測事項と、明示的な対象外項目まで完了したことは意味しない。

個別Cell参照方式のカット作成、セル系列の構造編集、再編集可能な指示／text object、角度付き撮影frame、端点loop policy、複数消失点／放射補助線は、いずれも縦切り実装、自動検証、x64 Releaseの利用者確認まで完了した。現行要件の状態と残る既知差分は`docs/compatibility.md`を正本とする。

### 2.2 件数

- 対象内の意味上のギャップ: **22 群**
  - `Must`: 4 群
  - `Should`: 15 群
  - `Could`: 3 群
- 仕上げ工程での優先度: **P0 3 群、P1 7 群、P2 6 群、P3 6 群**。P0 が最優先である。
- 仕様上の不足: **0 群**。セル系列の構造編集と撮影フレームの意味契約はそれぞれ `SEQ-STRUCT-001`、`SHOOTING-FRAME-001` として確定した。
- 調査時点の実装上の不足: **22 群**。上の2群を含むため、仕様不足との件数は排他的ではなかった。
- 調査時点では`Specified only`: 9群、`Missing`: 1群、`Partial`: 11群、`Implemented but unverified`: 1群として整理した。段階的解消後は22群すべて実装・確認済みである。
- 対象外、`Equivalent by another workflow`、`Not required` は 22 群へ含めていない。

`Must`／`Should`／`Could` は PaintMan の機能的上位互換に必要かという評価であり、P0〜P3 はアニメの仕上げ工程で先に解消すべき順序である。たとえばカット管理は上位互換には `Must` だが、既存セルを日々彩色する際の直接的な結果品質を基準にすると P3 となる。

### 2.3 調査時点で重大だった不足と解消結果

1. rasterとadjustmentの論理レイヤー順合成は、ordered render planをCanvas、thumbnail、flattenへ共通適用し、x64利用者確認まで完了した。
2. frame/image size、DPI、8/16 bit、初期レイヤー、複数枚の新規セルと、それらを個別ファイル参照で束ねる新規カットは、x64利用者確認まで完了した。
3. 複数 edit target の production UI、Core／ABI、型付きコピー／編集は実装し、x64 Release の主線＋彩色手動確認まで完了した。
4. raster選択の閉領域内側、線沿い内側、筆跡形状、境界、作図optionは実装、自動検証、x64 Releaseの表示確認まで完了した。
5. 範囲限定の色置換と連番batch authoringは、実装、自動検証、x64 Releaseの利用者確認まで完了した。

`docs/compatibility.md`は`PAINT-*`、`COLOR-*`、`SEL-*`、`BATCH-*`等を広い単位で追跡する。現行scopeの個別能力はproduction routeの自動検証と利用者確認で追跡する。過去の優先順位と受入順は履歴であり、現在状態の判定には個別のコード、テスト、`docs/compatibility.md`を優先する。

### 2.4 結論保留領域

PaintMan の塗りあふれ中断が途中結果を残すか、フィルターの厳密な色空間／丸め、外部 clipboard 契約は PDF だけでは確定できない。二セルから色対を作るアルゴリズムは利用者判断により `SPEC.md` の exact native-depth 契約へ確定した。また、Light Table の回転、物理ペン入力、階調主線の比較（暗）は実装経路に対する直接検証が不足する。

## 3. 機能対応表

| ID | 分類 | PaintMan の機能的能力 | PDF 根拠 | inkpod の対応仕様 | 実装・テスト根拠 | 判定 | 差分 |
|---|---|---|---|---|---|---|---|
| PM-CAP-001 | カット管理 | カットに作品／話／シーン／カット情報、基準寸法、セル群、尺、指示をまとめる | 第3章「カットフォルダ」PDF表示 pp.22–26（印刷 pp.42–51） | `SPEC.md`「ファイル > 新規カット」、`CUT-001` | 個別Cell参照の`CutCore`／current-only記述子、ABI v14、Windows New Cut／Properties／Sequence production route、履歴・保存・negative・smoke | Implemented and verified | — |
| PM-CAP-002 | セル系列 | セルを追加・削除・並替え・採番し、系列を構造として編集する | 第3章「ファイルブラウザ」PDF表示 pp.46–55（印刷 pp.90–109） | `SEQ-STRUCT-001` | stable pair identity、bounded ordered insert/remove/move/renumber、一回Cut Undo/Redo、Cut schema 2、Windows drag/keyboard/dialog routeとx64 Release確認 | Implemented and verified | — |
| PM-CAP-003 | セル作成 | frame/image size、DPI、レイヤー型、8/16 bit、作成枚数を指定する | 第5章「新規セル」PDF表示 pp.63–65（印刷 pp.124–129） | `SPEC.md` §7、`DOC-001` | typed Cell creation plan、image/frame mode、DPI／frame、全initial layer、RGBA8/16、1..64枚、Core/ABI/Windows smokeとx64 Release利用者確認 | Implemented and verified | — |
| PM-CAP-004 | 用紙／基準フレーム | 用紙、作画／安全／基準 frame、余白、DPI、異寸法整列を保持する | 第5章 PDF表示 pp.65–70（印刷 pp.128–139） | `SPEC.md` §7、`DOC-001` | `FrameMetadata`; test `acceptance_reference_frame_aligns_different_cell_sizes_and_reopens`; Windows document smoke | Implemented and verified | — |
| PM-CAP-005 | 撮影フレーム | サイズ、角度、座標を持つ撮影 frame を文書内で編集する | 第12章 PDF表示 pp.163–164（印刷 pp.324–327） | `SHOOTING-FRAME-001` と「角度付き撮影 frame の確定 contract」 | stable-ID object、canonical edit/preview、ABI v17、Canvas handles、明示的な指示export、current-v27 save/reopen、x64 Release利用者確認 | Implemented and verified | — |
| PM-CAP-006 | 連続作業 | thumbnail／番号／前後で切替え、dirty 時に確認または自動保存し、端点を循環できる | 第2・3・14章 PDF表示 pp.19–20,55,175–176（印刷 pp.36–39,108–109,348–351） | `SEQ-001`, `SEQ-ENDPOINT-001` | natural-order/thumbnail、exact autosave/staged-restore、Stop/Wrap issue-time plan、ABI v14、human-readable settings-JSON field、Windows checked-state/production smoke、x64 Release利用者確認 | Implemented and verified | — |
| PM-CAP-007 | モーション確認 | FPS、範囲、loop、pause、step でセル系列を確認する | 第7章 PDF表示 pp.111–112（印刷 pp.220–223） | `SEQ-002` | animation contract、FFI、`RunProductionWorkflowSmoke` | Implemented and verified | タイムシート合成は対象外 |
| PM-CAP-008 | 彩色構造 | 2値／階調の主線、色トレース、彩色、汎用 raster plane を型付き分離する | 第6章 PDF表示 pp.71–84、第15章 pp.180–187 | `SPEC.md` §5、`DOC-002` | topology validation; test `acceptance_layer_tree_undo_redo_save_reopen_and_validation` | Implemented and verified | — |
| PM-CAP-009 | レイヤー／プレーン操作 | create、duplicate、delete、reorder、visibility、editability、opacity、convert、merge を扱う | 第6章 PDF表示 pp.76–82（印刷 pp.150–163） | `DOC-002`, `DOC-003`, `SPEC.md:223–234` | grouped capability/canonical/Undo contracts; FFI spans; Layer pane marker/menu/status and smoke; x64 Release manual check | Implemented and verified | — |
| PM-CAP-010 | 合成順 | rasterとadjustmentを論理 layer 順で合成する | 第6章 PDF表示 pp.76–82、第15章 pp.180–187 | `RENDER-001`, `DOC-002` | ordered render plan、ABI records、renderer pixel smoke、thumbnail／flatten／save-reopen、x64 Release確認 | Implemented and verified | — |
| PM-CAP-012 | 消失点 | 複数消失点、補助線角度、色、不透明度を編集する | 第7章 PDF表示 p.95（印刷 pp.188–189） | `VANISHING-POINT-001` | stable-ID object、canonical CRUD/preview、bounded radial snapshot、radial snap、ABI v17、Windows dialog／Canvas handle／renderer route、current-v27 persistence、x64 Release確認 | Implemented and verified | — |
| PM-CAP-013 | 自由描画／消去 | 鉛筆、brush、raster eraser、auto-erase、筆圧を扱う | 第7章 PDF表示 pp.88,107（印刷 pp.174–175,212–213） | `PAINT-001` | `paint_001_brush_eraser_auto_erase_and_pressure_are_transactional` | Implemented and verified | 物理 pen 経路の直接検証は §7 参照 |
| PM-CAP-013A | ストローク入力 | 実ペンタブレットの pressure を stroke 太さへ反映する | 第7章 PDF表示 pp.88,107（印刷 pp.174–175,212–213） | `PAINT-001` | `WM_POINTER` の PT_PEN／pressure route はあるが物理 device E2E なし | Implemented but unverified | 実機検証は §7 |
| PM-CAP-014 | 図形描画 | 直線、二段階曲線、矩形、楕円、N角形、polyline、制約、snap をrasterへ適用する | 第7章 PDF表示 pp.89–91（印刷 pp.176–181） | `PAINT-002`, `SNAP-001`, `SPEC.md` | raster typed geometry、resolved canonical procedure、preview state machine、全 primitive capability/golden、bounded ABI、Windows staged gesture/renderer/snap smoke | Implemented and verified | — |
| PM-CAP-015 | Brush 制御 | 丸／角、太さ、筆圧、補正、開始色と同色領域だけを塗る | 第7章 PDF表示 p.107（印刷 pp.212–213） | `PAINT-004`, `SPEC.md:293–298` | native-depth goldens、canonical replay、ABI v8 negatives、Windows Tool Options/Canvas smoke、x64 Release 手動確認 | Implemented and verified | PM-GAP-012 解消済み |
| PM-CAP-016 | 線修正 | ゴミ取り、線つなぎ、raster線幅修正を局所／選択へ適用する | 第7章 PDF表示 pp.92–94（印刷 pp.182–187） | `PAINT-003`, `EFFECT-001` | test `full_effect_gestures_dust_and_alpha_are_atomic`, raster width/connect contracts, Windows smoke | Implemented and verified | — |
| PM-CAP-017 | 入力 snap | guide／grid の表示設定を実際の直線等の入力点へ適用する | 第4章 PDF表示 pp.58–59（印刷 pp.114–117） | `SNAP-001`, `VIEW-002`, `SPEC.md:251–255` | view-targeted Core resolver、bounded ABI、全M09 Windows gesture共有route、checked-state／geometry／digest／off／Ctrl bypass／Undo/Redo smoke、x64 Release 31/31 CTestsと利用者確認 | Implemented and verified | — |
| PM-CAP-018 | 色作成／スポイト | RGB/HSV、alpha、8/16 bit、複数 source のスポイトを使う | 第7章 PDF表示 pp.96–98,102（印刷 pp.190–195,202–203） | `COLOR-001`, `COLOR-002` | exact-depth tests、eyedropper FFI、Color pane smoke | Implemented and verified | — |
| PM-CAP-019 | Palette／塗り見本 | palette group、shortcut、save/load、前セル subpalette、scroll連動、色採取を使う | 第7章 PDF表示 pp.97–101（印刷 pp.192–201） | `COLOR-002`, `SHORT-001` | palette/chart codecs、`acceptance_sequence_gaps_natural_order_thumbnails_subpalette_and_motion`, pane smoke | Implemented and verified | 保存形式自体は対象外 |
| PM-CAP-020 | Color chart | 色名、検索、lock、copy、セル色抽出、max／quantization／preview を扱う | 第7章 PDF表示 pp.99–100（印刷 pp.196–199） | `COLOR-002`, `COLOR-CHART-PREVIEW-001`, `SPEC.md:325–326` | bounded immutable comparison、noncumulative生成、頻度／差分、Cancel／stale／lock、Apply／Undo／save-reopen、owned ABI、Windows Job Progress／production smoke、ARM64利用者確認 | Implemented and verified | — |
| PM-CAP-021 | 基本フィル | connected／inclusion fill、tolerance、隙間閉じ、漏れ中断、selection、主線保護を扱う | 第7章 PDF表示 pp.103–104（印刷 pp.204–207） | `FILL-001`, `FILL-002` | fill goldens、`fill_is_one_atomic_history_unit_and_never_changes_main_line`, FFI／Windows smoke | Implemented and verified | inkpod の失敗原子性は PDF 記載より強い |
| PM-CAP-022 | 閉領域一括 | pen／rect／polyline／lasso 内の閉領域を一回で塗る | 第7章 PDF表示 p.105（印刷 pp.208–209） | `FILL-003` | `golden_only_completely_closed_regions_are_filled`; Windows closed-fill route | Implemented and verified | — |
| PM-CAP-023 | 塗りのばし | 既存色を drag 方向の狭い隙間へ広げる | 第7章 PDF表示 p.106（印刷 pp.210–211） | `FILL-003` | image test `tolerance_detached_closed_extension_and_color_check_semantics`; Core／UI／smoke route | Implemented and verified | — |
| PM-CAP-024 | 範囲色置換 | pen／rect／polyline／lasso の範囲でraster対象色を置換する | 第7章 PDF表示 p.106（印刷 pp.210–211） | `COLOR-REPLACE-001` | exact native-depth raster、4 region／selection intersection、canonical replay/save、ABI negatives、Windows production gesture smoke | Implemented and verified | — |
| PM-CAP-025 | 組み線／合成セル | 別セル主線を境界に使い、親セル内容を子セルへ複製する | 第7章 PDF表示 pp.107–109（印刷 pp.212–217） | `FILL-002`, `LT-002`, `CLIP-001` | LT boundary contract; ordered main-line/color raster clipboard contracts; x64 Release manual check | Implemented and verified | 親子という専用 UI は使わず、Light Table 境界＋複数 target copy/paste で合理的同等 |
| PM-CAP-026 | 基本選択 | rect／ellipse／wand／lasso／polyline／trace と New/Add/Subtract/Intersect を使う | 第9章 PDF表示 pp.122–124（印刷 pp.242–247） | `SEL-001` | `acceptance_selection_authoring_tools`; FFI／Windows gesture smoke | Implemented and verified | — |
| PM-CAP-027 | Raster 選択解釈 | shrink、閉領域内部、描線形状、境界、比率、中心、回転、trace brush option を使う | 第9章 PDF表示 pp.124–126（印刷 pp.246–251） | `SEL-004`, `SPEC.md:399–403` | typed image/Core interpreter; canonical selection tests; ABI v8 option negatives; Windows option/preview smoke and x64 Release manual check | Implemented and verified | — |
| PM-CAP-029 | Mask 管理 | 色選択、expand／shrink、selection layer 化、再読込、描画／消去を行う | 第9章 PDF表示 pp.128–130（印刷 pp.254–259） | `SEL-002`, `SEL-003` | selection layer／color operations contract、Windows smoke | Implemented and verified | — |
| PM-CAP-030 | 型付きコピー | 単一または複数raster planeを属性と座標を保ってcopyする | 第9章 PDF表示 pp.131–132（印刷 pp.260–263） | `CLIP-001` | grouped RGBA8/16 raster clipboard contracts; FFI ownership; Windows marker/copy/paste smoke; x64 Release manual check | Implemented and verified | — |
| PM-CAP-031 | Paste／floating | 互換 plane、選択 plane、新規変換先へ paste し、位置を保って preview／commit／cancel する | 第9章 PDF表示 pp.131–135（印刷 pp.260–269） | `CLIP-001`, `XFORM-002` | clipboard／floating contracts、FFI、`RunDocumentEditingSmoke` | Implemented and verified | Windows private clipboard の型保持は §5 参照 |
| PM-CAP-031A | Paste 合成 | 階調主線同士は重なりの暗い側を採用する | 第9章 PDF表示 pp.133–134（印刷 pp.264–267） | `SPEC.md:417`, `CLIP-001` | Core は Grayscale8/16 を coverage の濃い側へ合成するが明示 test なし | Implemented but unverified | §7 の比較（暗）検証待ち |
| PM-CAP-032 | 全体変形 | mirror、90度 rotate、image size、resolution／resample を frame／guide と整合させる | 第10章 PDF表示 pp.137–138（印刷 pp.272–275） | `XFORM-001` | destructive transform contracts、Windows dialog smoke | Implemented and verified | — |
| PM-CAP-033 | 部分変形 | X/Y、scale、aspect、五点基準、任意角 rotate を preview して確定する | 第10章 PDF表示 pp.139–140（印刷 pp.276–279） | `XFORM-002`, `XFORM-003` | half-open五点基準のCore raster、canonical procedure、ABI、Windows dialog/handle/renderer production smoke | Implemented and verified | — |
| PM-CAP-034 | 表示移動 | zoom、box zoom、fit、1:1、pan、上下左右 view flip を行う | 第4章 PDF表示 pp.57–58（印刷 pp.112–115） | `VIEW-001` | view/coordinate contracts、Windows gesture/render smoke | Implemented and verified | zoom slider 不在は UI 差 |
| PM-CAP-035 | 補助表示 | ruler、guide、grid、transparent view を表示・編集する | 第4章 PDF表示 pp.58–59（印刷 pp.114–117） | `VIEW-002` | guide/grid state、renderer、Windows smoke | Implemented and verified | 実入力 snap は PM-CAP-017 |
| PM-CAP-036 | Locator | 周辺拡大と座標／色を表示し、固定 mode で編集・edge scroll する | 第4章 PDF表示 p.60（印刷 pp.118–119） | `VIEW-003` | `SelectLocatorPixel`; `RunMagnifiedRasterHitSmoke` は checksum、Undo/Redo を検証 | Implemented and verified | 編集は 1px Pencil 固定 |
| PM-CAP-037 | Multi-view | 同じ文書／履歴を共有し viewport だけ異なる複数 view を使う | 第4章 PDF表示 p.60（印刷 pp.118–119） | `VIEW-004` | multi-view Core／FFI、split/group/window smoke | Implemented and verified | — |
| PM-CAP-039 | Fullscreen | 他 pane を隠して描画領域を最大化する | 第4章 PDF表示 p.60（印刷 pp.118–119） | `SPEC.md:161` | 専用 command なし | Equivalent by another workflow | OS maximize／workspace preset で合理的に代替。Not required |
| PM-CAP-040 | Light Table 管理 | 複数 set/item、順序、visibility、opacity、color mode、transform を保持する | 第8章 PDF表示 pp.114–120（印刷 pp.226–239） | `LT-001`, `LT-003` | LT Core／canonical／FFI／pane・production smoke、ARM64手動確認 | Implemented and verified | — |
| PM-CAP-041 | 参照利用 | 基準 frame 整列、移動、sample、reload、編集画像交換、前後移動を行う | 第8章 PDF表示 pp.114–120 | `LT-002` | animation contracts、FFI、Windows target-aware smoke | Implemented and verified | — |
| PM-CAP-041A | 参照変形 | 登録した Light Table 画像を個別に回転する | 第8章 PDF表示 p.114（印刷 pp.226–227） | `LT-001` | item `rotation_milli_degrees` と Core／FFI／UI route はあるが、回転結果を直接固定する E2E は薄い | Implemented but unverified | 同頁の「画面を回転できない」は Canvas/view の別能力 |
| PM-CAP-042 | 履歴／復帰 | Undo/Redo、複数段階移動、保存点復帰、部分復帰を行う | 第10章 PDF表示 p.141（印刷 pp.280–281） | `HIST-001` | `hist_001_history_jump_and_partial_selection_revert_are_transactional`; Windows history smoke | Implemented and verified | PDF は履歴一単位の詳細が曖昧 |
| PM-CAP-043 | Preview／Cancel | stroke、transform、filter 等を base から preview し、OK 一件／Cancel 無変更にする | 第10章 PDF表示 pp.140,142–146（印刷 pp.278–291） | `FILTER-PREVIEW-001`, `SPEC.md:430–444` | stroke／floating／Core filter preview contracts、Windows debounced update、bounded latest-wins queue、production smoke、ARM64手動確認 | Implemented and verified | M10完了。PM-GAP-021解消済み |
| PM-CAP-044 | Filter catalog | sharpen、blur、invert、auto contrast、色調補正を selection／plane へ適用し再実行する | 第10章 PDF表示 pp.142–146（印刷 pp.282–291） | `FILTER-001`, `FILTER-002` | `filter_catalog_executes_with_bounded_parameters`; `acceptance_apply_is_exactly_one_undo_unit_and_last_filter_reuses_it`; UI smoke | Implemented and verified | interactive preview loop は PM-CAP-043 |
| PM-CAP-045 | 特効／retouch | airbrush、gradient、boundary airbrush、local blur、stamp、dust を使う | 第10章 PDF表示 pp.147–149（印刷 pp.292–297） | `EFFECT-001`, `PAINT-003` | deterministic image/Core tests、FFI、代表 Windows smoke | Implemented and verified | 一部個別 WM_COMMAND の E2E は §7 |
| PM-CAP-046 | Adjustment／alpha | 非破壊 adjustment を再編集し、alpha だけを編集する | 第10章 PDF表示 pp.150–152（印刷 pp.298–303） | `ADJUST-001` | adjustment order/save-reopen、alpha RGB preservation、FFI／UI smoke | Implemented and verified | — |
| PM-CAP-047 | 彩色 QA | 完全白／透明候補を高 contrast で表示して塗り漏れを確認する | 第7章 PDF表示 p.110（印刷 p.218） | `COLOR-001` | `ColorCheckMode`、snapshot overlay、FFI／Windows route | Implemented and verified | — |
| PM-CAP-048 | 出力色域 QA | 規格外の色だけを selection にする | 第7章 PDF表示 p.110（印刷 p.219） | `COLOR-OUTPUT-QA-001`, `SPEC.md:376` | BT.709保守ガード、exact visible composite、selection algebra、ABI／Windows production smoke、x64利用者確認 | Implemented and verified | PM-GAP-020解消済み |
| PM-CAP-049 | Batch 基盤 | 対象、順序付き graph、preview、dry-run、progress、cancel、failure policy を扱う | 第11章 PDF表示 pp.154–155（印刷 pp.306–309） | `BATCH-001`, `BATCH-003` | `graph_preview_dry_run_and_owned_report_cross_ffi`; `RunBatchWorkflowSmoke` | Implemented and verified | 出力形式は対象外 |
| PM-CAP-050 | Batch 詳細 | 複数 seed、複数色対、二セルから色対生成、分離先、実行時再設定を扱う | 第11章 PDF表示 pp.157–160（印刷 pp.312–319） | `BATCH-002`, `BATCH-004`, `SPEC.md:461–466` | bounded multi-row authoring、exact二セル候補／ambiguity解決、typed destination、enqueue前immutable run copy、Core／ABI／Windows smoke、x64 Release手動確認 | Implemented and verified | — |
| PM-CAP-050A | Batch 分離 | 指定色を mask／置換へ分離する基本 operation | 第11章 PDF表示 p.160（印刷 pp.318–319） | `BATCH-002`, `BATCH-004` | replacement／selection mask／主線／彩色／native file destination、canonical replay、golden、x64 Release手動確認 | Implemented and verified | — |
| PM-CAP-051 | Shortcut | 彩色 command を競合なく割り当て、連続作業を高速化する | 第2・14章 PDF表示 pp.20,175–176 | `SHORT-001` | shortcut catalog／conflict／persistence tests、Windows key smoke | Implemented and verified | 旧キー配置の一致は不要 |
| PM-CAP-052 | タイムシート | タイムシート作成、編集、合成、camera 等 | 第3章 PDF表示 pp.27–45（印刷 pp.52–89） | — | — | Out of scope | セル順／尺との依存だけ注記 |
| PM-CAP-053 | 形式／交換 | PaintMan／Retas／一般形式、旧形式、export 互換 | 第13・15章 PDF表示 pp.168–173,179–187 | — | — | Out of scope | ギャップ件数外 |
| PM-CAP-054 | 旧 UI／環境 | palette 配置、旧メモリ設定、monitor 初期位置、brand 表示 | 第4・14章 PDF表示 pp.60–61,174–178 | — | workspace／OS 機能あり | Out of scope | Not required |

## 4. 不足機能一覧

この節は、アニメの仕上げ（色塗り）での **結果の正しさとデータ損失防止、日常的な使用頻度、連番全体への効率影響** を基準に、優先順位が高い順で掲載する。`PM-GAP-*` は安定 ID であるため番号自体は変更していない。

- **P0:** 仕上げ結果または文書の正しさを損なうため、彩色 production-ready の前提になる。
- **P1:** 日常の塗り・修正・連番処理の品質または速度へ大きく影響する。
- **P2:** 主要 workflow の安全性、確認品質、反復効率を大きく改善する。
- **P3:** 管理、指示、低頻度作業、または合理的な代替手段がある。

### PM-GAP-007 — raster／adjustment混在時にもlayer順を守る

- **不足していた能力:** visibility、opacity、alpha、adjustmentとともに、rasterを任意の論理layer順で決定的に合成する。段階的解消作業で実装・確認済み。
- **PaintMan で可能な作業:** raster線、彩色、特効、調整を一覧の順序どおりに合成する。
- **現状で困る状況:** 解消済み。Coreのordered render planをCanvas、thumbnail、flattenが共有し、raster／adjustment混在時も論理順を維持する。
- **不足層／カバレッジ:** R/V/R・V/R/V、visibility、opacity、alpha、adjustment、reorder、Undo／Redo、save／reopen、ABI ownership／negative、renderer pixel、device loss、x64 Release利用者確認まで完了した。`RENDER-001`は`Verified`。
- **推奨優先度（仕上げ）:** **1/22（P0）**。互換性評価は **Must**。見た目と論理データの不一致は判断、export、レビューの誤りにつながる。
- **代替手段:** 不要。
- **関連要件:** `DOC-002`, `RENDER-001`。
- **責務:** Core snapshot は全 content の ordered composite identity。FFI は順序を保つ snapshot records。renderer はその順序どおりに描画し、mixed-order golden／smoke を持つ。
- **依存ギャップ:** PM-GAP-006 で対象順を操作可能にする。
- **PDF 根拠:** 第6章「レイヤー／プレーン」、PDF表示 pp.76–82、第15章 pp.180–187。

### PM-GAP-003 — 新規セルを完全な制作条件で作成する

- **不足していた能力:** frame-sizeまたはimage-size、DPI、initial layer type、8/16 bit、基準／最大寄りframe、anchor、作成枚数を指定する。段階的解消作業で実装・確認済み。
- **PaintMan で可能な作業:** カット既定値から同条件の複数セルを一度に作り、階調深度と画角を開始時点で固定する。
- **現状で困る状況:** 解消済み。typed creation planから1～64枚をall-or-noneで生成し、dialog previewとcommitが同じCore計算を使う。
- **不足層／カバレッジ:** image／frame mode、全initial layer、RGBA8/16、DPI／frame／anchor、invalid／overflow／allocation failure／Cancel、ABI ownership／negative、Windows production smoke、x64 Release利用者確認まで完了した。`DOC-001`は`Verified`。
- **推奨優先度（仕上げ）:** **2/22（P0）**。互換性評価は **Must**。bit depth や frame 条件の誤りは後工程で情報損失または全セル修正を招く。
- **代替手段:** 一枚ずつ作成し、用紙／frame／変換を後設定する。反復が多く、完全な同一条件を保証しにくい。
- **関連要件:** `DOC-001`, `SPEC.md` §7。
- **責務:** Core は typed creation options と複数生成の all-or-nothing 検証。FFI は bounded options／result span。Windows frontend は列挙型 dialog と一括結果表示。
- **依存ギャップ:** PM-GAP-001 の既定値を利用する。
- **PDF 根拠:** 第5章「新規セル」、PDF表示 pp.63–65（印刷 pp.124–129）。

### PM-GAP-006 — 複数レイヤー／プレーンを edit target として扱う

- **不足していた能力:** active selection と複数 edit target を分け、主線／彩色等の複数 plane を一つの copy／操作対象として明示する。M03 で実装し、x64 Release の手動確認まで完了した。
- **PaintMan で可能な作業:** 複数プレーンを属性付きでまとめて copy し、対応する構造へ一体として paste／編集する。
- **現状で困る状況:** 解消済み。主線＋彩色を別セルへ貼り付け、一回の Undo と active-plane-only brush を x64 Release で確認した。
- **不足層／カバレッジ:** Core／ABI／Windows production route、自動カバレッジ、利用者の手動確認まで完了。`DOC-002/003` は `Verified`。
- **推奨優先度（仕上げ）:** **3/22（P0）**。互換性評価は **Must**。主線と彩色の組を別々に扱うと位置、型、操作単位を失う可能性がある。
- **代替手段:** 一 plane ずつ操作する。原子的でなく、貼付先取り違えのリスクがある。
- **関連要件:** `DOC-002`, `DOC-003`, `CLIP-001`, `SPEC.md:227,410–418`。
- **責務:** Core は ordered multi-target と command validation。FFI は stable-ID span。Windows frontend は複数選択、target marker、enable state、結果表示。
- **依存ギャップ:** PM-GAP-007 の合成順、PM-GAP-002 の系列操作と直交する。
- **PDF 根拠:** 第6章 PDF表示 pp.76–82（印刷 pp.150–163）、第9章 pp.131–134（印刷 pp.260–267）。

### PM-GAP-016 — Raster 選択の内容解釈と作図 option

- **不足していた能力:** 描線へ密着する shrink、閉領域内部、描線形状、境界選択、矩形／楕円の aspect／中心／回転、trace brush の形状／筆圧／screen-size を区別する。
- **PaintMan で可能な作業:** 線画の内側、線そのもの、境界だけ等を目的に合わせて一回で mask 化する。
- **現状で困る状況:** 解消済み。range ごとの mask、回転 ellipse、角 trace brush、Add/Subtract は x64 Release で確認された。
- **不足層／カバレッジ:** image/Core、canonical procedure、ABI v8、Windows option／preview、v13／epoch-10 永続化、自動カバレッジ、利用者の手動確認まで完了。`SEL-004` は `Verified`。
- **推奨優先度（仕上げ）:** **4/22（P1）**。互換性評価は **Should**。選択は色修正、filter、copy、transform の共通基盤であり、手作業の mask 修正量が増える。
- **代替手段:** wand／trace／expand-shrink の組合せと mask 手編集。
- **関連要件:** `SEL-004`, `SPEC.md:399–403`。
- **責務:** Core/image は typed range interpretation。FFI は shape options。Windows frontend は option pane と preview gesture。
- **依存ギャップ:** PM-GAP-012, PM-GAP-015 の効率改善に使う。
- **PDF 根拠:** 第9章、PDF表示 pp.124–126（印刷 pp.246–251）。

### PM-GAP-012 — 彩色修正向け brush option

- **不足していた能力:** 丸／角 shape、stroke smoothing、開始 pixel と同色の領域だけを描く mode を通常 brush に持たせる。
- **PaintMan で可能な作業:** 境界近くの狭い塗り残しを、隣の色へはみ出さず連続 stroke で修正する。
- **現状で困る状況:** 解消済み。二色境界の狭い塗り残し、丸／角、補正量、Cancel／Undoをx64 Releaseで確認した。
- **不足層／カバレッジ:** Core/image、canonical procedure v3、ABI v8、Windows Tool Options／Canvas、v13／epoch-10 永続化、自動カバレッジ、利用者の手動確認まで完了。`PAINT-004` は `Verified`。
- **推奨優先度（仕上げ）:** **5/22（P1）**。互換性評価は **Should**。彩色修正の速度と誤塗り率へ直接影響する。
- **代替手段:** selection mask、細い brush、塗りのばし。準備操作が増える。
- **関連要件:** `PAINT-001`, `PAINT-004`, `SPEC.md:293–298`。
- **責務:** Core/image は shape、smoothing、start-color predicate を canonical stroke に含める。FFI は option。Windows frontend は pane と preview。
- **依存ギャップ:** PM-GAP-016 の選択詳細は代替 workflow を改善する。
- **PDF 根拠:** 第7章「ブラシ」、PDF表示 p.107（印刷 pp.212–213）。

### PM-GAP-015 — 対話的で範囲限定された色置換

- **不足している能力:** pen／rect／polyline／lasso 内のraster対象色だけを描画色へ置換する。
- **PaintMan で可能な作業:** 一部の領域だけ色指定を差し替え、同色を使う別領域は保つ。
- **現状で困る状況:** 解消済み。範囲外の同色保持、Cancel／Undo、raster表示結果を x64 Release で確認した。
- **不足層／カバレッジ:** Core／canonical procedure、ABI v8、Windows menu／Canvas／renderer、v14／epoch-11 永続化、自動カバレッジ、利用者の手動確認まで完了。`COLOR-REPLACE-001` は `Verified`。
- **推奨優先度（仕上げ）:** **6/22（P1）**。互換性評価は **Should**。selection と手動描画で一部代替できるが、離散領域の置換は操作が多い。
- **代替手段:** selection-by-color、mask、手動塗り、全体 batch 後の修正。
- **関連要件:** `COLOR-REPLACE-001`, `FILL-003`, `SPEC.md:375–378`。
- **責務:** Core は geometry mask と target-color replacement を一 transaction にする。FFI は region／color／target。Windows frontend は gesture／option／全体実行確認。
- **依存ギャップ:** PM-GAP-016 の range semantics を共用できる。
- **PDF 根拠:** 第7章「色置換ツール」、PDF表示 p.106（印刷 p.210）。

### PM-GAP-022 — 連続彩色向け Batch authoring の詳細

- **不足していた能力:** 複数 fill seed、複数 color pair、二セル比較からの pair 生成と曖昧さ preview、分離結果の mask／主線／彩色 destination、実行時再設定を production UI から扱う。
- **PaintMan で可能な作業:** 多数のセルへ複数箇所を連続 fill し、色指定変更や色分離を preset として反復する。
- **現状で困る状況:** 解消済み。exact native-depth二セル比較は件数／affected boundsを示し、one-to-manyを利用者が一候補または除外へ解決するまでsilent winnerを作らない。
- **不足層／カバレッジ:** Core／canonical procedure、ABI v17、Windows Batch production route、`.inkbatch` v2、現行 `.inkpod` v27／epoch-24、golden／smokeとx64 Release手動確認を完了した。
- **推奨優先度（仕上げ）:** **7/22（P1）**。互換性評価は **Should**。一件ずつ graph を分けると設定ミスと preview 回数が増え、長い sequence で影響が大きい。
- **代替手段:** 不要。
- **関連要件:** `BATCH-001`, `BATCH-002`, `BATCH-003`, `SPEC.md:461–466`。
- **責務:** Core は pair extraction の曖昧さを明示した result、separation destination、実行時 policy。FFI は bounded multi-row records。Windows pane は row editor、二セル選択、preview、destination 選択。
- **依存ギャップ:** PM-GAP-002 の cell identity、PM-GAP-006 の multi-target、PM-GAP-020 の QA と連携する。
- **PDF 根拠:** 第11章、PDF表示 pp.157–160（印刷 pp.312–319）。

### PM-GAP-011 — PaintMan 相当の図形作図 semantics

- **不足している能力:** rasterでの直線、二段階curve、N角形、click式polyline、45度／aspect／center／snapを一貫して扱う。
- **PaintMan で可能な作業:** 形の整った修正線や閉領域を少ない操作で作り、そのまま彩色境界として使う。
- **現状で困る状況:** 解消済み。Raster Geometryのproduction経路へguide／grid snapを接続した。
- **不足層／カバレッジ:** Core／canonical procedure、ABI、Windows production gesture、M09自動検証・ARM64手動確認、M15 x64 snap手動確認まで完了し、`PAINT-002`は`Verified`。
- **推奨優先度（仕上げ）:** **9/22（P1）**。互換性評価は **Should**。手描きと数値変形で一部代替できるが、追加操作が発生する。
- **代替手段:** snapが必要な場合は座標表示、45度／aspect／center制約を使って配置する。
- **関連要件:** `PAINT-002`, `SNAP-001`。
- **責務:** Core は typed geometry request と preview／commit。FFI は bounded points/options。Windows frontend は段階的 gesture、option pane、target validation。
- **依存ギャップ:** PM-GAP-013 の snap、PM-GAP-007 の ordered composition。
- **PDF 根拠:** 第7章 PDF表示 pp.89–91（印刷 pp.176–181）。

### PM-GAP-021 — parameter 変更に追従する非累積 preview

- **不足している能力:** filter／色調補正の parameter を変えるたびに同じ base から再計算し、結果を見て再調整してから一回だけ commit する。
- **PaintMan で可能な作業:** 強度や curve を完成画像で比較し、不要なら完全に Cancel する。
- **現状で困る状況:** M10でWindows dialogのparameter変更通知から既存Core／FFI preview sessionへ接続し、同じbaseからの非累積preview、OK一件、Cancel無変更をproduction routeへ実装した。ARM64の利用者向け表示確認も完了した。
- **不足層／カバレッジ:** Core／FFI／Windows production smokeとARM64手動確認が完了し、`FILTER-PREVIEW-001`は`Verified`。PM-GAP-021は解消済み。
- **推奨優先度（仕上げ）:** **10/22（P1）**。互換性評価は **Should**。結果品質へ直接影響し、Cancel→再度 dialog の反復が必要になる。
- **代替手段:** 手動確認完了までは、確定前に複数parameterを往復し、CancelとUndoで元画像へ戻ることを確認する。
- **関連要件:** `HIST-001`, `FILTER-001`, `FILTER-002`, `FILTER-PREVIEW-001`, `SPEC.md:432–444`。
- **責務:** Core は既存 preview session を維持。FFI ownership は既存契約を利用。Windows dialog/controller は120ms debounce、one-running／one-pending、cooperative cancel、stale rejection、OK／Cancelを接続する。
- **依存ギャップ:** なし。
- **PDF 根拠:** 第10章、PDF表示 pp.140,142–146（印刷 pp.278–291）。

### PM-GAP-004 — セル切替時に安全に自動保存する

- **不足している能力:** dirty cell から別セルへ移るとき、設定に従い通常保存とは別 status の自動保存を成功させてから切り替える。
- **PaintMan で可能な作業:** 前後セルを連続して彩色し、毎回の確認 dialog を省きながら変更を失わない。
- **現状で困る状況:** 解消済み。M11でversioned `Prompt`／`Autosave-before-switch` setting、発行時identity/revision固定、exact native recovery artifact、戻り時staged restore、非同期progressとcommand抑止をproduction routeへ接続し、ARM64で利用者向け表示と書込不可時の挙動を確認した。
- **不足層／カバレッジ:** Coreのsuccess/no-op/invalid/stale/Undo/Redo、ABI ownership/negative、Windows setting/association/production smokeとARM64手動確認を完了した。端点loop preferenceは別のPM-GAP-005で追跡する。
- **推奨優先度（仕上げ）:** **11/22（P2）**。互換性評価は **Should**。主に効率の問題だが、確認の見落としは変更損失につながる。
- **代替手段:** 毎回の保存確認、手動 save、一般 autosave。切替との順序は自動保証されない。
- **関連要件:** `SEQ-001`, `SESSION-001`, `SPEC.md:175,177`。
- **責務:** Core／session は保存対象 revision と切替 target を固定。FFI は save/switch result。Windows frontend は policy、progress、failure／Cancel 表示。
- **依存ギャップ:** PM-GAP-001/002 がある場合はその cell identity を使う。
- **PDF 根拠:** 第2・3・14章、PDF表示 pp.19–20,55,175–176（印刷 pp.36–39,108–109,348–351）。

### PM-GAP-019 — 前後 N セルの Light Table 一括登録と opacity step

- **不足している能力:** 現在セルから前後 N 枚を一括登録し、距離に応じて opacity を段階設定する。
- **PaintMan で可能な作業:** 前後動作を少数操作で onion-skin 表示し、どの時点の線か識別する。
- **現状で困る状況:** 解消済み。中間セルからの表示順、距離別opacity、一回Undo、save/reopenをARM64で確認した。
- **不足層／カバレッジ:** Core／canonical-v2、current-only `.inkpod` v27／epoch-24、ABI v17、Windows pane／preview／production route、golden／negative／smoke、quick／full benchmarkとARM64手動確認を完了した。
- **推奨優先度（仕上げ）:** **12/22（P2）**。互換性評価は **Should**。結果は手作業で得られるが、連続作業の頻出操作である。
- **代替手段:** 一枚ずつ追加し opacity を設定する。
- **関連要件:** `LT-001`, `LT-003`, `SEQ-001`, `SPEC.md:396–405`。
- **責務:** Core は sequence-relative bulk command と deterministic opacity policy。FFI は N／direction／step。Windows pane は preview／undoable command。
- **依存ギャップ:** PM-GAP-002 の安定した sequence membership が望ましい。
- **PDF 根拠:** 第8章、PDF表示 pp.119–120（印刷 pp.236–239）。

### PM-GAP-014 — Color chart 生成結果を確定前に比較する

- **不足している能力:** 最大色数／quantization を変えながら生成結果を preview し、元 state へ戻して再調整する。
- **PaintMan で可能な作業:** gradient／AA 由来の過剰色を、意味のある palette へ安全に圧縮する。
- **現状で困る状況:** 解消済み。ARM64で最大色数／quantizationの再調整、比較、Cancel、Apply／Undo、保存再openまで利用者確認を完了した。
- **不足層／カバレッジ:** Core／canonical-v1、current-only `.inkpod` v27／epoch-24、EDIT schema、ABI v17 ownership／negative、Windows latest-wins Job Progress／比較／Apply production route、golden／smoke、ARM64利用者確認を完了した。
- **推奨優先度（仕上げ）:** **13/22（P2）**。互換性評価は **Could**。Undo と条件変更で反復できるが、比較が遅い。
- **代替手段:** 生成→確認→Undo→条件変更。
- **関連要件:** `COLOR-002`, `SPEC.md:326`。
- **責務:** Core は immutable preview result と apply token。FFI は preview ownership。Windows frontend は比較表示、parameter update、OK／Cancel。
- **依存ギャップ:** なし。
- **PDF 根拠:** 第7章「カラーチャート」、PDF表示 pp.99–100（印刷 pp.196–199）。

### PM-GAP-020 — 現代的な出力色域／放送安全域 QA

- **不足している能力:** 選択した出力規格と変換式に基づき、規格外 pixel だけを selection mask にする。
- **PaintMan で可能な作業:** 納品前に放送で問題になる色を一覧化し、該当 pixel だけ修正する。
- **現状で困る状況:** 解消済み。正式な規格適合表示ではないBT.709係数／nominal code相当の保守的ガードを、visible compositeからselectionへ生成し、x64でprofile／overlay／pixel不変／Undoまで利用者確認した。
- **不足層／カバレッジ:** exact RGBA8/16 raster kernel、transparent skip、sparse selection algebra、canonical-v1、current-only `.inkpod` v27／epoch-24、ABI ownership／negative、human-readable settings-JSON既定profile、Windows task／status production route、golden／smoke、quick／full semantic gate、承認済みx64 wall-clock envelope、x64利用者確認を完了した。
- **推奨優先度（仕上げ）:** **14/22（P2）**。互換性評価は **Should**。放送／配信向け納品では再修正を避ける品質ゲートになる。
- **代替手段:** 外部 video／color grading tool。文書内 selection へ戻す工程は手動。
- **関連要件:** `COLOR-001/002`, `SEL-002`, `SPEC.md:376`。
- **責務:** Core/image は選択可能な規格、変換、mask 生成を決定的に実装。FFI は profile／result。Windows frontend は設定と selection 生成。
- **依存ギャップ:** なし。2008年の固定 NTSC 式そのものは §6 の `Not required`。
- **PDF 根拠:** 第7章「色域外のチェック」、PDF表示 p.110（印刷 p.219）。

### PM-GAP-013 — guide／grid snap を production 入力へ適用する

- **不足している能力:** snap toggle と同じ view／document state を、直線や図形の入力点へ実際に適用する。
- **PaintMan で可能な作業:** guide または grid に沿った正確な直線を引く。
- **現状で困る状況:** 解消済み。line／shape／polyline等は同じview-targeted Core resolverを通り、checked state、off、Ctrl一時解除が確定geometryへ反映され、x64利用者確認まで完了した。
- **不足層／カバレッジ:** `SNAP-001`仕様、threshold／precedence／tie／bounds／extreme transform／flip／DPI／staleのCore契約、size-versioned ABI ownership／negative、全M09 Windows gesture共有route、geometry／digest／Undo/Redo production smoke、x64 Release 31/31 CTestsと利用者確認を完了した。
- **推奨優先度（仕上げ）:** **15/22（P2）**。互換性評価は **Should**。見た目だけ有効な toggle は操作誤認を生み、精度の再確認が必要になる。
- **代替手段:** 数値変形、目視、外部作図。
- **関連要件:** `SNAP-001`, `VIEW-002`, `PAINT-002`, `SPEC.md:251–255`。
- **責務:** Core/input 解釈で gesture 点を deterministic に snap。FFI は同じ view target を固定。Windows frontend は modifier／checked state を request へ渡す。
- **依存ギャップ:** PM-GAP-011、PM-GAP-010。
- **PDF 根拠:** 第4章、PDF表示 pp.58–59（印刷 pp.114–117）。

### PM-GAP-017 — 五点基準を使った floating transform

- **不足している能力:** 解消済み。五点基準dialog／handle、Cancel／Undoもx64 Releaseで利用者確認された。
- **PaintMan で可能な作業:** 選択画像の特定の角や中心を固定して、数値で正確に配置する。
- **現状で困る状況:** 解消済み。dialogとCanvas handleが同じanchor semanticsを使い、Cancelと一回Undoも確認された。
- **不足層／カバレッジ:** half-open五点anchor、absolute X/Y、anchor pivot、scale→clockwise rotate→positionをCore raster、canonical procedure、ABI、Windows dialog／Canvas handle／rendererへ接続した。invalid／overflow／Cancel／stale、Undo／Redo、replay、current-v27 save／reopen、unknown anchor／short structure、x64 Release利用者確認まで完了した。
- **推奨優先度（仕上げ）:** **16/22（P2）**。互換性評価は **Should**。視覚 handle で代替できるが、複数セルで同じ基準に揃えにくい。
- **代替手段:** 不要。
- **関連要件:** `XFORM-002`, `XFORM-003`, `SPEC.md:425`。
- **責務:** Core は anchor semantics と座標不変条件。FFI record に anchor。Windows dialog／handle は同じ値を使う。
- **依存ギャップ:** なし。
- **PDF 根拠:** 第10章「選択範囲の変形」、PDF表示 pp.139–140（印刷 pp.276–279）。

### PM-GAP-001 — カットを意味上の制作単位として保持する

- **不足している能力:** 解消済み。作品、話、シーン、カット、基準寸法、セル群、尺、指示、既定レイヤーを、一つの安定 ID を持つカットとして作成・保持できる。
- **PaintMan で可能な作業:** 新規カットで共通条件を一度決め、同じカットのセルを一貫した条件で準備する。
- **現状で困る状況:** 解消済み。Cut記述子が同一directoryの個別Cellを永続identityで束ね、metadata／尺／指示／既定値を独立所有し、x64 Releaseで利用者確認された。
- **不足層／カバレッジ:** `CUT-001` として仕様化し、`CutCore`、Cut canonical history、current-only v27／epoch-24記述子、ABI v17 ownership／negative、Windows New Cut／Properties／Sequence production route、no-op／invalid／Cancel／stale／overflow／failure／Undo／Redo／save／reopen／recoveryを実装・検証した。個別Cellの文書・履歴・dirty・savepointはCutから独立し、既定値は新規Cell作成時だけコピーする。x64 Release利用者確認も完了した。
- **推奨優先度（仕上げ）:** **17/22（P3）**。互換性評価は **Must**。カット単位の整合を外部命名へ委ねると、系列混同と既定値の不一致が起きる。
- **代替手段:** 不要。個別Cellファイル自体は通常の`.inkpod`として独立して扱える。
- **関連要件:** `CUT-001`, `DOC-001`, `SESSION-001`。
- **責務:** Core は `CutId`、metadata、個別Cell membership、既定値、独立履歴／保存点と不変条件。FFI は Rust-owned handle と bounded DTO。Windows frontend は `CutSession`、新規カット／properties、個別Cell作成・保存、sequence提示を所有する。
- **依存ギャップ:** PM-GAP-003 と PM-GAP-002 の所有単位になる。
- **PDF 根拠:** 第3章「カットフォルダ」、PDF表示 pp.22–26（印刷 pp.42–51）。

### PM-GAP-002 — セル系列を構造として編集する

- **不足していた能力:** セルを系列へ追加・削除・並替え・採番し、衝突なく一回の操作として確定する。自動検証とx64 Release利用者確認まで完了した。
- **PaintMan で可能な作業:** サムネイル一覧を見ながら欠番や順序を直し、作業対象の並びを整える。
- **現状で困る状況:** 自動検証の範囲では解消した。Sequence paneからadd／remove／dragまたはkeyboard reorder／range renumberでき、source fileをrename／deleteせずCut記述子だけを一回置換する。
- **不足層／カバレッジ:** `SEQ-STRUCT-001`として仕様化し、stable `(CellId, document UUID)` identity、bounded ordered transaction、最終state検証、一回Cut Undo／Redo、current-only v27／epoch-24 Cut schema 2、strided ABIと失敗operation index、Windows production routeを実装・確認した。remove後の参照は別Cellへ付け替えずorphan／missingとして表示する。
- **推奨優先度（仕上げ）:** **18/22（P3）**。互換性評価は **Should**。Explorer で代替できるが、連番の衝突、参照切れ、途中失敗のリスクがある。
- **代替手段:** 不要。物理file管理は引き続きExplorer／標準file dialogへ分離する。
- **関連要件:** `SEQ-001`, `SEQ-STRUCT-001`, `SESSION-001`。
- **責務:** Core は安定した sequence membership と編集 transaction。FFI は ordered edit request。Windows frontend は一覧からの insert/delete/reorder/renumber と確認。
- **依存ギャップ:** PM-GAP-001 のCut内部操作として実装済み。後続機能はstable Cell identity providerを利用できる。
- **PDF 根拠:** 第3章「ファイルブラウザ」、PDF表示 pp.46–55（印刷 pp.90–109）。

### PM-GAP-008 — 角度と位置を持つ撮影フレームを編集する

- **不足していた能力:** 撮影 frame の寸法、角度、座標を独立 object として保持・編集・表示する。
- **PaintMan で可能な作業:** camera framing の大きさと傾きをセル上で指示し、作画範囲と区別して後工程へ渡す。
- **現状で困る状況:** 推奨案 A＋N1 の縦切りとx64 Releaseでの利用者向けhandle、Cancel、save/reopen、出力分離確認を完了した。
- **不足層／カバレッジ:** `SHOOTING-FRAME-001`、Core／canonical-v2、current-only `.inkpod` v27／epoch-24、ABI v17、Windows properties／Canvas handle／renderer／明示的指示export production routeを実装した。独立overlay、既存axis-aligned frameのpaper-fit authority、通常export/thumbnail除外、非等方resampleの厳密表現不可時の原子的拒否を公開contractで固定し、必須自動gateとx64 Release利用者確認を完了した。
- **推奨優先度（仕上げ）:** **20/22（P3）**。互換性評価は **Should**。カメラ指示の取り違えは作画／彩色のやり直しにつながる。
- **代替手段:** 不要。PM-GAP-008は解消済み。
- **関連要件:** `SHOOTING-FRAME-001`, `DOC-001`, `DOC-002`, `SPEC.md` 「角度付き撮影 frame の確定 contract」。
- **責務:** Core は frame object geometry。FFI は typed query/edit。Windows frontend／renderer は handle／dialog／overlay と preview。
- **依存ギャップ:** Vanishing Pointと同じoverlay／handle基盤を共有できる。
- **PDF 根拠:** 第12章「撮影フレーム」、PDF表示 pp.163–164（印刷 pp.324–327）。

### PM-GAP-005 — 前後セル切替の端点 loop policy

- **不足していた能力:** 先頭／末尾で停止するか循環するかを利用者が選べる。
- **PaintMan で可能な作業:** 連続比較時に末尾から先頭へ戻り、keyboard 操作を途切れさせない。
- **現状で困る状況:** 自動検証の範囲では解消した。`端点で循環`をoffにすると完全no-opで停止し、onにすると先頭／末尾を相互に切り替える。
- **不足層／カバレッジ:** `SEQ-ENDPOINT-001`として、Coreのempty／one／Stop／Wrap／欠番／forward／backwardとissue-time stale原子性、96-byte caller-owned ABI v14 plan、human-readable settings-JSON codec、application-wide menu／configurable shortcut／checked state／status、motion loop分離、Windows production smokeを実装し、x64 Release利用者確認まで完了した。
- **推奨優先度（仕上げ）:** **21/22（P3）**。互換性評価は **Could**。作業結果を変えず、先頭／末尾 command で代替できる。
- **代替手段:** 実装経路は利用可能。手動確認までは先頭／末尾へ明示移動できる。
- **関連要件:** `SEQ-001`, `SEQ-ENDPOINT-001`, `SPEC.md:191–193`。
- **責務:** Core または frontend policy に loop preference、FFI の step result、Windows setting／checked state。
- **依存ギャップ:** なし。
- **PDF 根拠:** 第7章モーション確認と第14章設定、PDF表示 pp.111–112,175–176（印刷 pp.220–223,348–351）。

### PM-GAP-010 — 消失点と放射補助線

- **不足していた能力:** Canvas内外の複数消失点と、1/5/10/15/30度等の補助線間隔、色、不透明度を編集する。自動検証とx64 Release利用者確認まで完了した。
- **PaintMan で可能な作業:** パース線に沿った修正線や背景要素を描く。
- **現状で困る状況:** 解消済み。properties、Canvas handle、radial overlay／snapから同じpersistent objectを編集できる。
- **不足層／カバレッジ:** `VANISHING-POINT-001`のstable-ID Core object、canonical CRUD／preview、bounded radial snapshot、guide／gridと競合するradial snap、current-only `.inkpod` v27／epoch-24、ABI v17、Windows dialog／Canvas handle／renderer／device-loss production route、自動gate、x64 Release利用者確認まで完了した。`Verified`。
- **推奨優先度（仕上げ）:** **22/22（P3）**。互換性評価は **Could**。主なセル彩色では頻度が低く、他の作図 app や手動 guide で代替できる。
- **代替手段:** 外部作図、複数の手動線、参照画像。
- **関連要件:** `VANISHING-POINT-001`, `SNAP-001`, `VIEW-002`。
- **責務:** Core は point／radial guide state と hit/edit。FFI は CRUD。Windows frontend／renderer は dialog と overlay。
- **依存ギャップ:** PM-GAP-013 の snap と結合すれば入力拘束にも使える。
- **PDF 根拠:** 第7章「消失点」、PDF表示 p.95（印刷 pp.188–189）。

## 5. 仕様済み・未実装機能

この節は「PaintMan に対する仕様不足」ではなく、**既に `SPEC.md` に書かれているのに、実装または十分な検証を確認できなかったもの**を分離した一覧である。`docs/compatibility.md` が広い requirement ID を `Verified` とする場合でも、下表の詳細能力まで試験した証拠がないものを含む。

下表は仕様との traceability を優先した並びであり、仕上げ工程での優先順位は第4節を正とする。

| `SPEC.md` の能力 | 関連要件 | 現状 | 根拠／扱い |
|---|---|---|---|
| 新規カットとカット metadata／既定値 | `CUT-001` | 実装・手動確認済み | 個別Cell参照方式、独立Cut history/savepoint、current-only v27/epoch-24、ABI v17、Windows production smokeとx64 Release確認。PM-GAP-001解消済み |
| Cut内セル系列のadd／remove／reorder／renumber | `SEQ-STRUCT-001` | 実装・手動確認済み | stable pair identity、bounded ordered transaction、一回Cut Undo/Redo、Cut schema 2、ABI失敗index、Windows drag/keyboard/dialog smokeとx64 Release確認。PM-GAP-002解消済み |
| frame/image size、8/16 bit、複数枚の新規セル | `DOC-001` | 実装・手動確認済み | typed plan、image/frame mode、DPI／frame、全initial layer、RGBA8/16、1..64枚、Core/ABI/Windows smokeとx64 Release確認。PM-GAP-003解消済み |
| セル切替時自動保存 | `SEQ-001` | 実装・手動確認済み | exact native recovery association、staged restore、通常savepoint/path不変、Core／ABI／Windows production smokeとARM64確認。PM-GAP-004解消済み |
| sequence 端点 loop preference | `SEQ-ENDPOINT-001` | 実装・手動確認済み | Stop/Wrap、明示result、issue-time identity、ABI v14、human-readable settings JSON、menu／shortcut／checked state／status、Windows smoke／x64 Release確認。PM-GAP-005解消済み |
| 複数 edit target の presentation | `DOC-002`, `DOC-003` | 実装・手動確認済み | tree-ordered Core/ABI、Layer pane marker、capability menu、status、smoke、x64 Release 確認。PM-GAP-006 |
| 論理 layer 順の raster／adjustment合成 | `RENDER-001`, `DOC-002` | 実装・手動確認済み | ordered render plan、ABI、renderer pixel smoke、thumbnail／flatten、x64 Release確認。PM-GAP-007解消済み |
| 角度付き撮影frameの内容 | `SHOOTING-FRAME-001` | 実装・手動確認済み | stable ID、center/size/rotation/anchor、canonical preview、ABI v17、Canvas handles、通常/指示export分離、current-v27 save/reopen、x64 Release確認。PM-GAP-008解消済み |
| VanishingPoint の内容 | `VANISHING-POINT-001` | 実装・手動確認済み | stable ID、Canvas内外、間隔／開始角／exact color／opacity、canonical preview、radial snapshot／snap、ABI v17、Windows dialog／handle／renderer、v27 save/reopen、x64 Release確認。PM-GAP-010解消済み |
| 二段階 curve、N角形、line／polyline options、raster 図形 | `PAINT-002` | 実装・手動確認済み | Core/canonical、ABI v17、Windows staged gestures、現行v27/epoch-24、golden/smoke。snapはPM-GAP-013/M15 |
| 通常 brush の shape／smoothing／開始色限定 | `PAINT-004` | 実装・手動確認済み | Core/image、canonical v3、ABI v8、Windows pane/Canvas、v13/epoch-10、golden/smoke、x64 Release確認。PM-GAP-012 解消済み |
| guide／grid snap の実入力適用 | `SNAP-001`, `VIEW-002` | 実装・手動確認済み | view-targeted Core／ABI、全M09 Windows gesture共有route、checked state／geometry／digest／off／Ctrl bypass／Undo/Redo smoke、x64 Release利用者確認。PM-GAP-013解消済み |
| color-chart quantization preview | `COLOR-002`, `COLOR-CHART-PREVIEW-001` | 実装・手動確認完了 | noncumulative preview、頻度／差分、名前／cursor保持、lock／Cancel／stale、Undo／Redo、v19 save/reopen、owned ABI、Windows production smoke、ARM64利用者確認。PM-GAP-014 |
| 対話的 scoped color replace | `COLOR-REPLACE-001`, `FILL-003` | 実装・手動確認済み | Core／canonical procedure、ABI v8、Windows menu／Canvas、v14／epoch-11、golden/smoke、x64 Release確認。PM-GAP-015 解消済み |
| raster 選択の range interpretation／construction options | `SEL-004` | 実装・手動確認済み | typed range／geometry／trace、Core/ABI/Windows、v13/epoch-10、golden/smoke。PM-GAP-016 解消済み |
| floating transform の五点基準 | `XFORM-002`, `XFORM-003` | 実装・手動確認済み | Core raster／canonical／ABI／Windows dialog・handle・renderer、current-v27／epoch-24、golden／smoke、x64 Release利用者確認。PM-GAP-017解消済み |
| LT 前後 N 枚登録／自動 opacity step | `LT-001`, `LT-003` | 実装・手動確認済み | linear opacity、自然順z-order、既存source保持、一回Undo、現行v27/epoch-24 replay、ABI/Windows production smokeとARM64確認を完了した。PM-GAP-019解消済み |
| 設定可能な出力色域 check → selection | `COLOR-002`, `SEL-002`, `COLOR-OUTPUT-QA-001` | 実装・手動確認済み | 非適合表示のBT.709保守ガード、exact RGBA16 visible composite、transparent skip、selection algebra、canonical-v1、ABI/Windows production smoke、x64 Release確認。PM-GAP-020解消済み |
| dialog parameter 変更ごとの filter preview update | `FILTER-PREVIEW-001`, `HIST-001`, `FILTER-001` | 実装・手動確認済み | 同一base Core preview、ABI v9、120ms debounce、bounded latest-wins queue、issue-time target、Job Progress、Windows smoke、ARM64確認。PM-GAP-021解消済み |
| batch 複数 seed／pair UI、二セル pair 抽出、分離先、実行時再設定 | `BATCH-002`, `BATCH-004` | 実装・手動確認済み | Core／canonical、ABI、Windows row editor／二セルselector、`.inkbatch` v2、現行v27／epoch-24、golden／smoke、x64 Release確認。PM-GAP-022 解消済み |
| app private clipboard でraster layer/plane typeを保持 | `CLIP-001` | 実装済み | Rust所有のprivate handleがordered raster plane、RGBA8/16、originを保持し、Windowsは標準DIBも併記 |
| fullscreen command | Window specification | 未実装 | OS maximize／workspace preset で代替できるため `Not required`、ギャップ非計上 |

## 6. 不要と判断した差異

| 項目 | PDF 根拠 | 判定 | 理由／依存注記 |
|---|---|---|---|
| タイムシートの作成、編集、camera、合成、印刷 | 第3章 PDF表示 pp.27–45（印刷 pp.52–89） | Out of scope | 明示的対象外。セル順、尺、モーション範囲との依存だけ残る |
| PaintMan／Retas 固有形式、旧形式、一般画像形式、交換／export 互換 | 第13・15章 PDF表示 pp.168–173,179–187 | Out of scope | 明示的対象外。保存／batch の「意味」だけ対象 |
| 旧版混在、Ver.5 互換、旧 palette／batch preset 形式 | PDF表示 pp.10,99,131–132,158,161,176 | Out of scope | 形式互換と旧環境由来 |
| dummy file、folder 命名規約そのもの | 第3章 PDF表示 pp.25–26 | Not required | stable ID、個別Cell参照、descriptor-onlyの系列構造編集で置き換えた |
| 通常の file 一覧、rename、copy、folder 作成 | 第3章 PDF表示 pp.46–54 | Equivalent by another workflow | Explorer／標準 file dialog で代替し、系列の原子的再構成だけをCut構造編集として実装した |
| 進行表、伝言板、一般メモ | 第3章 PDF表示 pp.50–53 | Equivalent by another workflow | 制作管理／レビュー app で代替 |
| 印刷 | 第13章 PDF表示 p.173（印刷 pp.344–345） | Not required | OS／標準画像 app で代替 |
| fullscreen、palette 整頓、zoom slider、旧 pane 配置 | 第4章 PDF表示 pp.57–61 | Not required | maximize、workspace preset、menu／shortcut で同じ目的を達成可能 |
| 旧仮想メモリ保存先、手動メモリ割当、起動 monitor 指定 | 第14章 PDF表示 p.178 | Not required | 現代 OS の memory／window 管理へ委ねる |
| plugin folder の場所指定 | 第14章 PDF表示 p.177 | Unable to assess / Not required | PDF は拡張可能な処理契約を説明せず、folder UI の一致だけでは機能差にならない |
| 2008年固定の NTSC video-level 式そのもの | 第7章 PDF表示 p.110（印刷 p.219） | Not required | 固定式の再現は不要。現代的で設定可能な出力色域 QA は PM-GAP-020 |
| brand、商標、問い合わせ、画面文言、icon | PDF表示 pp.190–191 等 | Out of scope | 明示的対象外 |

## 7. 判定不能事項

| 事項 | 現在分かること | 判定に必要な追加情報 |
|---|---|---|
| Light Table item の回転結果 | PDF表示 p.114 は登録画像の個別回転を明記し、inkpod に item rotation の Core／FFI／UI route がある。ただし回転後の座標／render 結果を直接固定する E2E が薄い | rotation 角、基準 frame、sample 座標、save/reopen を観測する acceptance／Windows smoke |
| 彩色 layer の単一性 | PDF表示 p.185 の脚注は「ラスター彩色」と書くが、表の列名は「階調彩色」で不整合 | 修正版 manual または対象 layer type ごとの実動作 |
| 「塗りあふれたら中断」の commit semantics | PDF表示 pp.103–104 は漏れ検出と早期停止まで。inkpod は all-or-nothing をテスト済み | PaintMan が途中結果を残すかを示す実動作。inkpod の強い原子性を維持する限り上位互換上の阻害はない |
| PaintMan filter の厳密な pixel semantics | filter 種別と parameter は読めるが、色空間、edge、rounding、16-bit 精度がない | vendor の algorithm 仕様または再配布可能な入力／出力 golden。現在は機能的能力だけを比較 |
| 外部 clipboard の contract | PDF は他 app 画像の paste を述べるが、標準形式、alpha、DPI、座標、type の契約がない | OS clipboard format と実入力例。inkpod の内部 typed clipboard 能力とは分離する |
| 実機 pen pressure／eraser | Windows は `WM_POINTER` の PT_PEN と pressure を読むが、物理 device の E2E test を確認できない。tilt／touch は PaintMan PDF の比較対象ではない | 対応 pen device を使う再現可能な smoke、pressure curve と eraser-end の期待結果 |
| 階調主線 paste の比較（暗） | Core は Grayscale8/16 を coverage の濃い側へ `max` 合成するが、その意味を直接固定する test 名がない | 8/16-bit の明示 golden と Windows paste E2E |
| Color chart の管理操作 | search／lock／rename／copy 等の実処理はあるが、Windows smoke の state assertion が薄い | 操作前後の chart state、Undo、save/reopen を観測する UI test |
| 個別 effect command の frontend coverage | Core/image/FFI test は広いが、gradient／stamp／dust 等の全 WM_COMMAND を個別に結果まで検査する E2E は一部不足 | command ごとの最小 checksum／Undo／Cancel smoke |

## 8. 推奨される次の仕様化順序

これは実装計画ではなく、`SPEC.md` で意味、境界、受入条件を明確にする順序である。依存関係を先に確定するための順序であり、第4節の仕上げ優先順位とは別軸である。

1. **文書階層と identity** — PM-GAP-001、002、003のCut／Cell所有関係、stable ID、共通既定値、系列構造編集と既存CutへのCell追加時の原子性を個別Cell参照方式で定義・実装した。
2. **合成と edit target の不変条件** — PM-GAP-006、007を定義する。複数targetとraster／adjustment ordered compositeは、copy、merge、render、export、Undoのデータ損失境界になる。
3. **撮影補助 object** — PM-GAP-008、010を型、座標、通常render／指示exportへの含有規則として定義する。UI文言ではなく永続objectの意味を先に決める。
4. **入力 primitive と拘束** — PM-GAP-011、012、013を一つの gesture／preview／commit 契約として整理する。shape、curve、snap、brush predicate を target 型ごとに列挙する。
5. **Selection を共通 region contract にする** — PM-GAP-015、016、017を、raster／floatingで再利用できるregion interpretationとanchor semanticsにまとめる。
6. **連続セル参照と保存 policy** — PM-GAP-004、005、019を、sequence-relative target、dirty/save failure、bulk LT 操作として定義する。
7. **診断と preview** — PM-GAP-014、020、021を、documentを変更しないpreview／overlay／selection resultとして定義する。旧NTSC固定値ではなく出力規格を選べる契約にする。
8. **Batch authoring** — 最後に PM-GAP-022を定義する。上記 Cut/Cell identity、multi-target、selection、preview を再利用し、二セル pair の曖昧さと separation destination を明文化する。

仕様化時には、各能力へ独立 requirement ID と少なくとも success、no-op、invalid、Cancel、Undo/Redo、必要な save/reopen、Windows production route の受入条件を与えるべきである。現在のように広い requirement 行へ多くの詳細能力を束ねると、一部の代表テストだけで `Verified` と見えてしまう。
