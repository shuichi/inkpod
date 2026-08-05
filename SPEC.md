# inkpod 機能・挙動仕様

この文書は、inkpod が維持する利用者向け機能、挙動契約、互換性の境界、要件 ID、すなわち「何を作るか」を定める恒久仕様である。技術境界、品質基準、作業規律、完了済み工程の進捗、過去の検証ログ、作業再開用プロンプトは含めない。

開発作業では、リポジトリ直下の `AGENTS.md` を作業規律と品質基準、本文書を機能と挙動の正本、`docs/architecture.md` を現在の構造、`docs/compatibility.md` を要件ごとの対応状況、`docs/implementation-status.md` を現在状態・既知差分・直近検証の要約として扱う。

## 目的

inkpod は次の構成を維持する。

- Rust Core: OS 非依存の文書モデル、画像処理、選択、履歴、連番、バッチ、永続化、描画スナップショット
- C++/Win32: Windows 11 のアプリケーション、OS 入出力、スレッド、Common Controls、Rust adapter
- Canvas: DXGI swap chain、Direct3D 11、Direct2D device context、DirectWrite、必要な WIC 連携
- ABI: `extern "C"` の versioned C ABI と Rust `staticlib`
- Build: CMake が唯一の入口であり、Cargo build も CMake から行う
- Portability: Rust Core は将来の macOS/Linux/iPad frontend から再利用可能

旧 PaintMan の外見やアイコンを複製することは目的ではありません。次の利用者価値を再現することが目的です。

1. 主線を保護したまま彩色できる
2. 2 値、階調、ベクターの彩色データをレイヤー/プレーンとして扱える
3. 小さな線切れや色トレースを考慮して、高速かつ安全にフィルできる
4. 基準フレームで異なる用紙サイズを整列し、前後セルをライトテーブル表示できる
5. 選択、レイヤー、履歴、変形、フィルタ、調整を非破壊または Undo 可能に扱える
6. 多量の連番セルへ同じ処理を安全に batch 適用できる
7. Windows 11 で自然に動き、Core は別プラットフォームへ持ち出せる

## 仕様の優先順位と自己完結性

通常の実装作業では外部の PaintMan マニュアルや掲載画像を参照しないでください。この `SPEC.md` に正規化した文章仕様を、旧ワークフロー互換性の正本として扱ってください。仕様が競合した場合は次の順に判断してください。

1. 今回のユーザー指示
2. リポジトリの `AGENTS.md`
3. この `SPEC.md` の内蔵機能仕様と requirement ID
4. 既存コードがテストで保証する契約
5. Windows 11 の標準的な操作慣習

対応するファイル形式は本文書に明記されたものだけです。未列挙の外部形式を追加せず、GUI、codec registry、file dialog に placeholder や disabled entry を残さないでください。

## 内蔵機能仕様

### 1. 用語と互換性の単位

- `カット`: 一つのショットに属するセル連番、背景、基準 frame、既定用紙、metadata をまとめた制作単位。
- `セル`: 一枚の作画・彩色文書。用紙、frame、複数 layer、選択、補助情報を持つ。
- `レイヤー`: セル画を重ねる単位。種類ごとの役割と許可 plane を持つ。
- `プレーン`: layer 内の最小編集単位。主線、彩色、色トレース、塗り、alpha 等を別々に保持する。
- `主線保護`: 彩色 mode では主線を合成表示するが、fill や brush が主線 plane を変更しない性質。
- `基準フレーム`: 紙のタップ穴に相当する位置合わせ基準。異寸法セルや light table を重ねるときは画像左上ではなくこの基準で揃える。
- `100フレーム`: 制作上の基準となる作画 frame サイズ。物理寸法、DPI、pixel 寸法を組にして保持し、50/200 frame 等はこの基準に対する比率で表す。
- `余白`: 作画 frame の外側だが文書内に存在する領域。camera work やはみ出し作画のため保持する。
- `安全フレーム`: 必ず画面内へ収めたい内容の目安。作画 frame とは別の overlay であり画像へ焼き込まない。
- `ライトテーブル`: 編集セルの背後へ参照セルを半透明で重ねる read-only 機能。編集画像と明示的に入れ替えた場合だけ編集対象が変わる。
- `合理的互換性`: 操作の意味、データ分離、座標、結果を再現すること。旧画面の配置、配色、アイコン、文言、制限を不必要に模写することではない。

### 2. Windows GUI の全体構成

Windows GUI は標準的な Windows 11 desktop application とし、古典的 MDI
や別 GUI framework へ移行せず、次の構造を持たせてください。

- process には一つの `ApplicationHost` を置き、同一 UI/Input thread 上で複数の `WorkspaceWindow` を所有できるようにする。各 window は独立した menu bar、制約付き dock、editor area、status bar、focus history を持つ。`WM_QUIT` は最後の workspace window が閉じたときだけ発行する。
- 独立した常設 toolbar は置かない。利用者が実行できる全機能を menu bar の末端項目から呼び出せることを優先する。選択中 tool の option strip は toolbar ではなく、同じ command/state を表示する context pane とする。
- editor area は一つまたは二つの `EditorGroup` を持つ。二分割は左右または上下だけを許し、再帰分割しない。各 group は独立した tab strip、active `DocumentView`、一つの可視 Canvas slot、focus history を持つ。
- 一つの `DocumentSession` は一つの `InkpodCore` handle、file identity、dirty/savepoint、Undo/Redo、autosave/recovery を所有する。同じ document の全 `DocumentView` は session を共有し、zoom、pan、flip、表示補助、表示中 frame 等の view logical state だけを分離する。文書 raster、layer、history、保存先を view ごとに複製しない。
- tab label は active sequence cell 名、保存 file 名、`無題セル N`、`復元セル`の順で意味のある識別名を使い、dirty は `*`、同じ document の追加 view は `[ビュー N]` で示す。read-only、処理中、error も compact かつ accessible な状態として示す。tab を閉じる操作は view を閉じ、最後の view の場合だけ document close と dirty 確認へ進む。
- `CanvasSurface` は非表示 tab ではなく可視 `EditorGroup` ごとに一つ持つ。active tab の切替時に同じ surface を別 `DocumentView` へ bind し直し、非表示 tab 数に比例して swap chain や renderer thread を増やさない。
- dock zone は `TopContext`、`Left`、`Right`、`Bottom`、`Floating`、`Hidden`、`AutoHide` に制限する。各 zone は pane tab stack と一方向の比率分割だけを持ち、任意に再帰する dock tree を作らない。pane descriptor は stable type ID、default/allowed zone、scope、multiplicity、float/autohide 可否、最小寸法を宣言する。
- pane の target scope は `Application`、`FollowActiveView`、`PinnedDocument`、`Job` を区別する。pin 先 document が閉じた場合は別文書へ silent に向けず、追従 mode へ戻して accessible notification を出す。pane action は発行時の target ID と generation を保持する。
- 現在相当の一 window、一 group 配置を初期 named workspace `彩色` として維持する。96 DPI の初期値は上端に全幅 40 DIP の tool options、body 左端に幅 80 DIP の一列 tool pane、中央に document tabs と Canvas、右端に幅 320 DIP の color/palette/chart および layer/plane inspector、最下段に status bar とする。既存の 32:68、55:45 比率と 4 DIP splitter を初期値に使うが、これは固定所有権ではなく復元可能な layout state である。
- tool pane の既定 button は 72 x 34 DIP、一列、7 pt の読み取れる一語ラベルとする。正規ラベルは `鉛筆`、`ブラシ`、`消しゴム`、`塗りつぶし`、`閉領域塗り`、`塗り延ばし`、`スポイト`、`直線`、`曲線`、`長方形`、`楕円`、`折れ線`、`線消しゴム`、`グラデーション`、`エアブラシ`、`境界ブラシ`、`ぼかし`、`スタンプ`、`ゴミ取り`、`アルファ階調` とする。意味を推測させる一文字略号へ戻さず、詳細名は tooltip で補う。
- named workspace と per-window layout は versioned、bounded な application setting として保存し、`.inkpod` 文書へ混ぜない。monitor/DPI 構成が変わった場合は可視 work area へ clamp し、不正 record は拒否して初期配置へ戻す。temporary な narrow-window adaptation で保存済み logical layout を上書きしない。
- built-in named workspace は `彩色`、`線整理`、`参照・チェック`、`バッチ`、`集中` を提供する。layout record は window、split、dock、pane、floating placement と選択 preset だけを保持し、開いている文書 path や Core 所有状態を含めない。未知 pane は無視し、不足する既知 pane は既定値で補う。
- floating pane は owner workspace を持つ通常の owned top-level window とし、閉じる操作では既定で非表示にする。`WS_EX_TOPMOST`、`WS_EX_PALETTEWINDOW`、`WS_EX_NOACTIVATE` は使わず、独立した `WM_DPICHANGED`、keyboard navigation、high contrast、screen reader を扱う。
- 下段の status bar は現在 tool/active plane、document 座標、zoom/view flip/grid、pixel RGBA/selection 寸法、文書寸法/DPI、処理進捗、dirty 状態、複数ストローク入力待ちを短く表示する。
- menu、shortcut、context menu、pane button は同じ command ID と enable/checked state を共有する。command 発行時に immutable な `CommandContext` として workspace、group、session、view、pane/job、generation を確定し、非同期実行時に active tab を再解決しない。stale target は明示 error または安全な no-op とし、現在 active な別文書へ fallback しない。
- 全ての実行可能な menu 末端項目に shortcut を割り当て、menu label に現在の割当を表示する。`Ctrl+S`、`Ctrl+O`、Undo/Redo、clipboard など標準操作は一般的な割当を維持する。描画・選択・塗りなど高頻度操作は、text 入力に focus がないときの single stroke を基本とする。その他は短い prefix-free な multi-stroke を使い、入力待ちを status bar に表示する。
- 色 palette の `1`–`0`、次 group の `Tab` は数値入力中でない場合の高速操作として保持する。shortcut は検索可能な設定 dialog で最大4 strokeまで再割当てでき、完全一致の衝突は元の割当と交換し、prefix 衝突は拒否する。
- tab drag は同一 group 内の並べ替え、別 group/window への移動、window 外 drop による新規 window を扱う。active stroke、pointer capture、modal preview 中は開始せず、`Esc` で cancel した場合は元の位置を完全復元する。同じ操作は drag に依存せず menu と keyboard からも実行できる。
- `Ctrl+Tab`/`Ctrl+Shift+Tab` は tab、`Ctrl+F6`/`Ctrl+Shift+F6` は editor group/view、`F6`/`Shift+F6` は menu・dock pane・editor area・status の focus、`Ctrl+F4` は view close に使う。tab、splitter、pane header、AutoHide、target、dirty、job progress と command の disabled state は UI Automation から取得できるようにする。
- 数値入力と選択肢を共有する modal dialog は、選択肢ごとに標準 combo box を使い、owner window の中央かつ monitor work area 内へ配置する。Cancel は表示前の状態を変えない。
- 実行不能 command は disable する。例として vector layer で pencil、選択なしの一部 command、対象 layer 未指定の batch を無言で成功させない。未接続 button、空 pane、常時成功する stub は生成しない。

### 3. メニュー構成

表示名は日本語 resource を基本とし、英語 resource を追加できる構造にしてください。以下は機能上必要な top-level menu と command です。Windows の標準慣習に合わせた mnemonic、ellipsis、並びの小調整は許可します。

#### ファイル

- `新規 > セル`: 用紙、pixel 寸法、DPI、100 frame、frame 配置、layer 種類、色深度、作成枚数を指定してセルを作る。
- `新規 > カット`: 作品/scene/cut 名、セル folder、既定用紙、layer 数、尺を設定する。
- `開く`: `.inkpod` または対応 raster/sequence を開く。
- `最近使ったファイル`: 存在確認し、消失 path は履歴から整理できる。
- `保存`: 同一 path へ atomic save。変更がない場合は no-op。
- `名前を付けて保存`: 新しい `.inkpod` または明示した export 形式へ保存する。
- `復帰`: 最後に通常保存した savepoint へ文書全体を戻す。
- `レイヤーを部分的に復帰`: active layer/plane の selection 内だけを最後の保存状態から復元する。
- `読み込み`: 一般 raster、連番、palette/chart 等を document または参照データへ読み込む。
- `書き出し > ラスター`: 一枚またはカット連番を PNG/TIFF/TGA/BMP 等へ出力する。寸法、DPI、余白を含めるか、出力 layer、antialias、alpha/白背景合成を指定する。
- `自動保存設定`: 対象 document 種類、間隔、前後セル切替時保存、recovery 保存先を設定する。
- `終了`: dirty document ごとに保存/破棄/cancel を選び、worker を安全に停止する。

#### 編集

- `元に戻す`、`やり直し`: command 名を表示し、履歴がないとき disable する。
- `複数段階戻る`、`複数段階進む`: 履歴一覧から位置を選択する。
- `カット`、`コピー`、`ペースト`、`選択プレーンにペースト`、`変換してペースト`、`クリアー`。
- `変形 > 左右反転/上下反転/拡大・縮小/回転/移動`: selection 内の実データを変更する。
- `線修正 > 線つなぎ/線幅修正`: selection または tool で指定した範囲へ適用する。
- `スナップ > ガイド/グリッド`: checked state を表示する。
- `アルファ使用モード`: alpha 対応の描画、読み込み、保存を有効にする。
- `設定 > ショートカット`: menu/tool/other command を分類し、衝突する新割当は既存 command から解除する。`すべて戻す` で既定値へ戻す。
- `設定 > グリッド`: 間隔、分割数、原点を指定する。
- `設定 > 環境設定`: 色、透明表示、保存、cursor、performance、language 等を分類する。

#### セル

- `用紙設定`: frame 単位または pixel/物理寸法/DPI で用紙を変更し、元画像の anchor を左上/右上/中央/左下/右下から選ぶ。
- `撮影フレームを考慮して用紙サイズ変更`: 撮影 frame を収める用紙へ crop/expand する。
- `画像サイズ`: canvas 寸法を変更し、元画像の配置 anchor を選ぶ。
- `画像解像度`: 物理寸法、DPI、pixel 数、再 sample の有無を指定する。再 sample off は pixel 数を変えない。
- `鏡像 > 水平方向/垂直方向`: 全文書の実データを反転する。
- `回転 > 左90度/右90度`: 全文書の実データと frame 座標を回転する。
- `レイヤー > 新規/複製/削除/非表示を削除/変換/同種を統合/プロパティ`。
- `プレーン > 新規/複製/削除/変換/同種を統合/アルファ編集/プロパティ/設定`。
- `前のセル`、`次のセル`、`セル番号で移動`、`連続表示`。

#### 選択範囲

- `すべてを選択`、`選択解除`、`選択反転`。
- `描画色を選択`、`描画色以外を選択`、`描画色を選択範囲に追加`。
- `拡張`、`縮小`: pixel 幅を指定する。
- `変換 > 選択範囲をレイヤーへ/レイヤーを選択範囲へ/レイヤーを追加/レイヤーを削除`。
- `色領域外を選択`: 選択した放送規格の安全範囲外の色を mask 化する。

#### フィルタ

- `シャープ > 強/弱/アンシャープマスク`。
- `ぼかし > 強/弱/ガウスぼかし`。
- `階調反転`、`自動コントラスト`。
- `明るさ・コントラスト`、`色調カーブ`、`レベル補正`、`色相・彩度・明度`、`カラーバランス`。
- `ゴミ取り`。
- menu 先頭に `直前のフィルタを再実行` を置き、前回確定した parameter を使う。

#### 表示

- `拡大`、`縮小`、`全体表示`、`ピクセル等倍`、数値 zoom。
- `左右反転`、`上下反転`: view だけを変え、document history/dirty を変更しない。
- `ルーラー`、`ガイド線`、`グリッド`、`透明部分`、`作画フレーム`、`安全フレーム` の checked toggle。
- `彩色チェック表示`: 完全な白と透明を検査する一時表示。
- vector 用に `アンチエイリアス表示`、`中心線表示`、`中心線チェック`、`端点表示`。
- overlay 表示は document pixel へ焼き込まない。

#### ウィンドウ

- pane 表示切替は `ツールパレット`、`ツールオプション`、`カラー`、`レイヤー／プレーン` と、実装済みの補助 pane を列挙する。menu、shortcut、pane control は同じ command ID と checked state を使う。
- `新しいビュー`: active document の別 `DocumentView` を active group に作る。
- `ビューを閉じる`、`文書を閉じる`: 前者は focused view だけを閉じ、後者は全 window/group の該当 view を列挙して document session を一度だけ閉じる。
- `右へ分割`、`下へ分割`、`別グループへ移動`、`別グループに新しいビュー`、`グループを閉じる`: 最大二 group の editor area を command/keyboard から操作する。
- `新しいウィンドウ`、`ビューを新しいウィンドウへ移動`、`新しいウィンドウに複製ビュー`: 同一 process、同一 UI/Input thread 上の workspace window を操作する。
- `ワークスペース`: named workspace の選択、保存、名前を付けて保存、復元、既定に戻す、および pane の dock/float/hide/auto-hide command を提供する。
- `フルスクリーン`。
- current `彩色` preset との移行互換として、従来の `初期位置`、`現在位置を保存`、`保存位置へ戻す`、`左右を反転` の意味を named workspace command から到達可能にする。

#### ヘルプ

- `inkpod ヘルプ`、`ショートカット一覧`、`診断情報`、`inkpod について`。旧製品名や旧 asset を自社製品表示として使わない。

### 4. 起動、終了、読み込み、セル切替、保存

- 起動時は Common Controls、COM、renderer、Rust Core の順に初期化し、途中失敗を status とユーザー向け説明へ変換して確実に unwind する。
- 同一 user/session では一つの論理 application instance とする。Explorer/file association 等からの secondary activation は command line を完全検証してから versioned、length-bounded、current-user 限定の IPC で primary process へ渡し、既定では last-focused workspace の active group を対象にする。明示的な `新しいウィンドウで開く` だけが新規 workspace を作り、primary timeout 時に同じ native file を別 process で無断編集しない。
- `ファイル > 開く` は focused workspace の active editor group に新しい document tab を追加する。同じ file identity が既に開いている場合は既存 `DocumentSession` の view を選択し、通常操作で二つの独立 session を作らない。別 view が必要な場合は `新しいビュー` を明示的に使う。
- file identity は Windows の volume/file ID を取得できる場合はそれを使い、取得できない場合は正規化した絶対 path を使う。表示名や tab index を identity に使わず、untitled document には frontend が UUID を発行する。
- 開いたセルと同じ sequence/folder にある画像は file preview に自然順で表示する。thumbnail click、前/次 command、番号指定で切り替える。
- active cell が dirty の状態で別セルへ移る場合は、設定に応じて保存確認または自動保存を行う。cancel ならセルを切り替えない。
- `前のセル` と `次のセル` は欠番を飛ばし、設定で末尾から先頭へ循環できる。
- 通常保存、自動保存、recovery、export は別 status とし、自動保存成功だけで通常 savepoint を進めない。
- 保存は temp file 完成後の置換とし、失敗しても元ファイルを残す。起動時は全 recovery 候補を列挙し、一件ずつ復元/破棄/保留を選べるようにして、silent に捨てない。通常の前回文書復元は layout と crash recovery から分離した既定 off の明示設定とする。
- `名前を付けて保存` は成功時に file identity registry、title、recent files、recovery metadata を一つの transaction として更新する。保存先 identity が別の open session と競合する場合は上書きや silent merge をせず、明示的な解決を求める。
- 外部変更または read-only は session ごとに検出し、保存前に利用者へ示す。read-only document を同じ path へ無言で書き換えず、reload は dirty/history を失うため明示確認と cancel を持つ。
- `view を閉じる` は一つの `DocumentView` だけを閉じる。最後の view でなければ document、history、dirty、job を保持し、dirty 確認を出さない。
- `document を閉じる` は全 workspace/group の該当 view を列挙し、dirty session について一度だけ保存/破棄/cancel を確認する。cancel または save failure では一つも閉じない。
- `window を閉じる` は、その window から消える view のうち他 window に view が残らない dirty session だけを確認する。他 window に残る session、Canvas、job を破棄しない。
- `application を終了する` は dirty な `DocumentSession` ごとに一度だけ保存判断を求める。同じ document の view 数だけ dialog を出さず、cancel または save failure では shutdown を開始しない。
- shutdown は新規 command/input を停止し、active stroke/modal preview と dirty 判断を解決し、layout を保存し、Canvas unbind と snapshot drain、Core work cancel/drain と owner-thread destroy、renderer resource の owner-thread 破棄、最後の `HWND` 破棄の順に行う。

### 5. 彩色文書の種類と合成

#### 2値彩色レイヤー

- `主線プレーン`: 二値の主線。彩色中は表示されるが保護される。
- `彩色プレーン`: 色トレース線と塗り色を保持する。
- 任意の `ラスタープレーン`: airbrush、gradient、retouch 等を分離して置ける。
- 軽量な legacy workflow を意図し、境界判定では主線と彩色 plane の線を利用できる。

#### 階調彩色レイヤー

- `主線プレーン`: grayscale coverage と基本線色を保持する。表示時に coverage と基本色を合成する。
- `彩色プレーン`: 色トレース、塗り色、および彩色用の細い境界情報を保持する。
- 主線の灰色 coverage 自体を fill 境界とみなさず、彩色 plane の境界が切れた場合に漏れる。
- eyedropper で階調主線を拾うと、中間表示色ではなく基本線色を返す。

#### ベクター彩色レイヤー

- `主線プレーン`: variable-width vector path。
- 一つ以上の `色トレース線プレーン`: 色別の vector path。
- `塗りプレーン`: 閉領域 topology と塗り色。
- 任意の raster plane。
- vector geometry は zoom で変化せず、表示時だけ rasterize する。

#### その他のレイヤー

- `ラスター汎用`: 背景・特効用 RGBA 8/16 bit。alpha channel を持てる。
- `フレーム`: 基準 frame、作画 frame、安全 frame、撮影 frame。
- `消失点`: 一つ以上の消失点と補助線設定。
- `選択範囲`: selection mask の保存と編集。
- `調整`: 明るさ/contrast、levels、tone curve を非破壊で保持する。
- `テキスト`: 再編集可能な文字。指示用 variant は通常 export から除外できる。
- `指示`: 手書き annotation。通常の完成画像 export から除外する。

composite は layer/plane 順、visibility、opacity、alpha、adjustment を決定的に適用してください。プレーンは所属 layer を越えて並べ替えず、layer 同士と同一 layer 内 plane 同士を別に並べ替えます。

### 6. レイヤー・プレーンパレット

- 上段に layer、下段に active layer の plane を表示する split pane とする。
- 各行に visibility、editable/target、name、種類に応じた color/thumbnail を表示する。
- active selection と複数 edit target を区別する。描画 command は active plane と明示 target 規則を検証する。
- drag and drop で同階層の順序を変える。
- opacity は数値と slider で変更する。
- 新規、複製、削除、property、alpha edit は必ず menu から操作でき、modeless palette を追加する場合も同じ command ID へ委譲する。
- 複製名は一意にする。削除は Undo 可能とし、必須 plane を最後の一枚まで削除できないよう validation する。
- 同種統合は同じ種類だけを対象にし、plane color 等の互換条件が異なるものを黙って統合しない。
- property dialog では name、type、opacity、plane color 等を編集し、type conversion は損失内容を事前表示する。
- 新規プレーンでは種類と形式を番号入力にせず、列挙値に対応する文字列を標準コンボボックスから選ぶ。全候補は選択可能とし、OK 時に選択中レイヤーの topology 制約を Core で再検証する。使用できない組み合わせはエラーを表示してダイアログを閉じない。

### 7. 用紙とフレーム

- 新規セルは `frame size` または `image size` で作る。frame size は100 frame基準に対する横/縦比、image size は width/height pixel と DPI を使う。
- 作画 frame は描くべき範囲、安全 frame は必ず見せたい範囲、余白は作画 frame 外の保存領域として別データで保持する。
- 基準 frame は用紙左上から中心までの X/Y と、左上/右上/中央/左下/右下の簡易配置を持つ。drag と数値のどちらでも移動できる。
- 最大寄り frame は zoom-in/camera frame が100 frame相当となる比率と anchor を保持する。
- 用紙変更時は元画像 anchor を選び、crop される pixel がある場合は確認と preview を出す。
- 物理寸法、DPI、pixel 数の関係は一つを変えたときの再 sample 設定で一意に決める。

### 8. 表示、移動、ルーラー、ロケーター

- zoom tool は click で拡大、`Alt`+click で縮小する。box zoom を有効にすると drag 矩形が viewport に収まる倍率へ移動する。
- menu と shortcut から拡大、縮小、数値倍率、fit、1:1 を操作できる。1:1 は document pixel と表示 device pixel の関係を DPI policy とともに文書化する。
- pan tool は Canvas drag。別 tool 使用中も一時 modifier で pan へ切替できる。
- 左右/上下 view flip は表示 transform だけを変更する。セル menu の mirror は実データを変更する。
- ruler から guide を drag して作成し、Canvas 外へ drag して削除する。move tool で位置変更する。
- grid は間隔、分割、原点を持ち、zoom が低い場合は表示線だけ間引く。snap 計算は表示間引きの影響を受けない。
- 透明表示は設定色または checkerboard で示し、pixel 値を変更しない。
- color locator は cursor 周辺を別倍率で表示し、X/Y、selection 幅 H、高さ V、対角長 L、RGBA を表示する。固定 mode では locator 上で編集でき、edge 付近は自動 scroll を選べる。
- multi-view は一つの document state と history を共有し、viewport transform だけを別に持つ。
- vector overlay は antialias on/off、中心線、中心線のみ、未接続端点を切り替えられる。

### 9. 描画・線修正ツール

すべての tool option は tool options pane に表示し、stroke/shape 確定前の preview と確定後の command を分離してください。

#### スポイト

- source は `最上位の非透明プレーン`、`選択中プレーン`、`合成表示色`、`ライトテーブル最上位色` から選ぶ。
- 描画 tool 中の `Alt` または右 click で一時 eyedropper を使える。light table source 用 modifier も command mapping として定義する。

#### 鉛筆

- click で一点、drag で1 document pixelの線を描く。
- 階調主線では1 pixel相当の antialias coverage を描く。
- ベクター彩色 layer では使用不可。
- stroke 開始 pixel が描画色と同色なら stroke 全体を erase mode にする auto erase を持つ。`Shift` で auto erase を一時無効にする。

#### 消しゴム

- tool options の先頭に `消去対象: 主線 / 彩色` を常時明示し、選択中の layer/plane、menu の主線/彩色 command、status bar と双方向に同期する。消しゴム選択だけでは対象を自動変更しない。
- shape、太さ、zoom に対して screen size を維持するか、pressure を太さへ反映するかを選ぶ。
- raster は cursor footprint 内を透明/背景へ消す。
- vector は `触れた部分だけ`、`触れた線の交点まで`、`触れた線全体` の三 mode を持つ。切断端は不必要に丸めない。

#### 直線・曲線・図形・折れ線

- 直線: start から end へ drag し、release で確定。
- 曲線: start/end を drag 後、control point を動かし click で確定する単純な curve workflow。
- 図形: 長方形、楕円、N角形。outline color/width、fill color、吸着、aspect ratio、中心から作成、作成後回転を持つ。
- 折れ線: click で頂点追加、double click で終了。始終点を結ぶ、区間を Bézier 化する option を持つ。
- line 系は入り、抜き、吸着、45度制約、断面形状を必要に応じて持つ。`Shift` は aspect/angle constraint として一貫させる。

#### ブラシ・エアブラシ

- brush は丸/角、太さ、pressure、stroke smoothing、開始 pixel と同色領域だけへ描く mode を持つ。
- airbrush は太さ、硬さ、dab 間隔、fade、pressure->size、pressure->opacity、停止中も時間で濃くなる continuous spray を持つ。

#### ゴミ取り

- 適用範囲は pen、rectangle、polyline、lasso。
- mode は `背景/透明以外の小点を除去`、`透明/背景の小穴を周囲色で埋める`、`周囲と異なる小領域を周囲色へ置換`。
- 最大サイズを指定し、必要な線を消す可能性を preview で確認する。
- tool は局所、filter は選択または plane 全体を一括処理する。vector では無効。

#### 線つなぎ

- 指定範囲内で設定 gap 未満の端点候補を結び、raster の接続線幅を指定できる。
- tool は drag した範囲、menu command は既存 selection を対象にする。
- 誤接続を避けるため候補の距離、角度、対象 plane を決定的に評価し、Undo 一回で戻す。

#### 線幅修正

- 適用範囲は pen/rectangle/polyline/lasso。vector は触れた線全体 option を持つ。
- `指定幅だけ太く`、`指定幅だけ細く`、vector の `指定倍率で拡大/縮小`、`一定幅` を別 mode にする。
- raster は morphology、vector は path width edit とし、結果の意味を揃えても内部処理を混ぜない。

#### 消失点

- 一つ以上の消失点を Canvas 内外へ置ける。
- 補助線の角度間隔は少なくとも 1/5/10/15/30度、色、不透明度を設定する。
- dialog 表示中に追加・移動・削除・全削除でき、設定を native document または独立 native preset へ保存できる。

### 10. 色、パレット、チャート、参照画像

- 描画色は sRGB RGBA 8/16 bit を保持し、RGB と HSV editor、alpha 数値/percent 表示を切り替える。
- 色を使う active command は、鉛筆、ブラシ、フィル、選択、エアブラシ、各ベクター描画 tool ごとに独立した現在色を持つ。鉛筆の既定色は黒、その他の彩色用 command の既定色は彩色用の初期色とする。command 切替時はその command の現在色を復元し、color editor、swatch、数値欄へ即時反映する。color pane は文書の主線色と active command の彩色用描画色を別のラベルと swatch で常時区別する。スポイト等の色を持たない一時 tool は直前の色付き command を変更先として維持する。
- color palette は複数 page/group を持ち、cell click で描画色取得、modifier+click で現在色登録、clear/save/load ができる。
- 高頻度の10色は `1`から`0`へ割り当て、`Tab`で次の10色 group へ切り替える。shortcut editor で変更可能にする。
- color chart は色と名前を表形式で管理し、複数 page、検索、次候補、lock、cut/copy/paste、save/load を持つ。旧版の5文字制限は native 形式へ課さない。
- `セルからカラーチャートを作成` は一意色を抽出するが、gradient/antialias 画像で色数が過大になるため、最大数、quantization、preview を用意する。
- subpalette は彩色済み参照セルを独立 viewport で表示し、zoom/pan、前後セル、番号移動、現在セル登録、自動的に一つ前のセルを表示、Canvasとのscroll連動、取得色のpalette登録を持つ。
- light table から色を拾う場合は、item transform と基準 frame alignment を通した同一 document 座標を使う。

### 11. フィルと彩色

#### 通常フィル

- click した seed と許容誤差内で連結した領域を描画色へ置換する。
- `離れた領域も塗る` は同じ判定色を持つ非連結領域も処理する。
- selection があればその内側だけを処理する。
- `ライトテーブルの境界線を参照` は参照画像の線を仮想的な read-only 境界にする。
- `ライトテーブルの色を使用` は seed の document 座標に対応する最上位参照色を描画色として使う。
- pixel 変更を commit した fill は Core が確定した実際の対象 plane を active selection とし、plane pane、menu、status を同期する。主線から通常彩色 plane へ塗った場合は彩色を選択し、active な汎用 raster plane へ塗った場合はその安定 ID を維持する。0 pixel の no-op、cancel、failure では active selection を変更しない。

#### 含み塗り

- `なし`、`指定色`、`指定色以外` を持つ。
- 基本の赤/緑/青を含め最大6色相当を登録でき、対象色は境界とみなさず fill 領域と一緒に描画色へ置換する。
- 色トレース線を消して影色等へ含める用途を満たす。
- vector の `線全体を塗る` は一部に触れた path 全体の色を変更する。
- fill 実行直前の modifier で含み塗りを一時 off にできる。

#### 塗りあふれ中断

- fill 領域が外周へ到達する、設定した閉領域条件を外れる等の漏れを検出した場合は失敗 status と漏れ候補位置を返す。
- transaction は all-or-nothing とし、途中まで塗った pixel を commit しない。

#### 隙間を閉じる

- 指定 pixel 幅以下の線切れを仮想境界で閉じて fill する。
- gap 値が過大なときに狭い領域を誤って閉じたり別領域を結んだりし得るため、preview と上限を持つ。
- 通常は元の線 plane を変更しない。線を恒久接続する操作は `線つなぎ` command として分ける。

#### 閉領域フィル

- pen/rectangle/polyline/lasso で指定した範囲内に含まれる複数の閉領域を一回で塗る。
- `透明部分のみ` と含み塗りを組み合わせられる。
- 細い毛先や1 pixel領域も、通常 fill の seed click を何度も要求せず処理する。

#### 色置換・塗りのばし・組み線

- 色置換 tool は指定範囲の対象色を描画色へ変更し、主線 mode では主線、彩色 mode では塗り色へ作用する。selection なしの全体処理は実行前に明示する。
- 塗りのばしは既存色を drag 方向の狭い未着色領域へ広げ、効果範囲、強さ、drag で囲まれた範囲も処理する option を持つ。
- 組み線彩色では light table の線を境界として参照し、参照画像自体を変更しない。
- 合成動画では親セルの必要 layer/plane を typed clipboard で子セルへ座標維持 paste できる。

### 12. 彩色チェックとモーションチェック

- `彩色チェック表示` は legacy white-transparency mode で完全な白 RGB(255,255,255) を未彩色/透明候補として残し、それ以外を黒等の高 contrast で表示する。native alpha mode では透明 alpha も別 category で示す。
- 放送色域 check は選択した規格と変換式を設定に持ち、規格外 pixel だけを selection mask にする。
- motion check は同じ sequence の指定範囲を、倍率、背景色、余白色、開始時 pause、selection のみ、light table を含める設定で再生する。
- FPS shortcut は少なくとも 30/25/24/12/10/8、前後 frame、先頭/末尾、space pause/resume、Esc 終了を提供する。
- 簡易連続表示は追加設定なしで sequence を loop 表示し、Esc で終了する。

### 13. ライトテーブル

- set は複数作成、複製、削除、rename、並べ替えでき、document を閉じても native workspace/document 設定として保持する。
- document ごとに `デフォルト` set を持ち、必要なら `なし` を選べる。
- 複数 item を登録でき、若いセル番号を下層に置く等の順序を明示する。
- item ごとに visibility、個別 opacity、display color、color/monotone/halftone、position、scale、rotation、表示 layer、source revision を持つ。
- set は global opacity を持ち、実効 opacity は `item opacity * global opacity`。50%と50%は25%となる。
- move tool で単一 item を移動し、modifier で全 item をまとめて移動する。数値 X/Y、基準 frame に対する五点 anchor、position reset を持つ。
- source が変更された場合は `更新` で reload し、失敗時は以前の有効 snapshot を保持する。
- 前/次セル、一覧、番号移動、前後N枚登録、全表示/全非表示、主線のみ/彩色のみ、表示 layer 選択を提供する。
- `編集画像と入れ替え` または item double click は、現在編集 image と選択 item を入れ替える。dirty 保存確認を通し、参照側の transform/opacity 情報を壊さない。
- light table 全体で重なりを透けさせる option と、前後画像登録時の自動 opacity step を持てる。

### 14. 選択範囲

- selection は document 寸法の mask として保持し、処理効果をその mask 内へ限定する。
- tool は rectangle/ellipse、magic wand、lasso、polyline、trace brush。
- operation は `新規`、`追加`、`削除`、`交差`。modifier は Shift=追加、Alt=削除、Shift+Alt=交差を基本とする。
- selection 内を drag したとき、mask だけを移動するか、選択された active plane pixel/vector も floating content として移動するかを option で分ける。
- rectangle/ellipse は aspect ratio、中心から作成、作成後回転、45度 constraint を持つ。
- magic wand は connected same-color、color tolerance、gap close を持つ。階調主線では基本色と coverage semantics を使う。
- trace brush は丸/角、太さ、pressure、screen-size固定を持つ。
- 範囲解釈は通常、描線に密着する shrink、閉じた内部、描線形状、必要に応じた境界選択を区別する。
- vector selection は selection で切断、一部でも触れれば選択、完全包含のみ、線を選択、線全体、交点まで、塗りを囲む線、塗りを選択を区別する。
- 描画色と同じ/異なる領域の全選択、追加、mask expand/shrink を提供する。
- selection layer との相互変換、現在 mask への追加/削除、selection layer 自体を通常描画 tool で編集する操作を round-trip 可能にする。

### 15. カット、コピー、ペースト

- clipboard payload は source document ID、layer/plane type、document origin に対する bounds、pixel/vector/selection、色深度を持つ。
- `コピー` は対象として選択された layer/plane だけを格納する。主線と彩色の両方を target にした場合は両方の typed payload を保持する。
- 通常 `ペースト` は payload と同じ属性の destination plane を優先する。現在別種類の plane が選ばれていても、互換 destination が存在すれば元属性へ貼る。
- `選択プレーンにペースト` は明示的に現在 plane へ変換・合成する。損失がある型変換は preview/確認する。
- `変換してペースト` は新規 layer または plane の種類、色深度、名前を選んで貼る。
- アプリ内 paste は source の文書座標を維持する。destination 用紙が小さくても clip せず保持可能範囲と見えない範囲を正しく扱う。
- paste 直後は floating selection とし、drag 移動、transform、commit、cancel を可能にする。
- 階調主線同士の互換 paste は重なった pixel の暗い方を採用する `比較(暗)` semantics を持つ。
- 外部アプリ向けには標準 Windows image clipboard を併記するが、標準形式では失われる layer type/座標をアプリ内 private format で補う。

### 16. 画像全体と選択部分の変形

- view flip/zoom/pan と、document data の mirror/rotate/resize を別 command にする。
- image size は canvas width/height と元画像 anchor を変更する。resolution は物理寸法、DPI、pixel 数と再 sample algorithm を変更する。
- 全文書の左右/上下 mirror、90度回転はすべての画像 plane、vector、selection、frame、guide の座標整合を保つ。
- 部分 transform は selection content を floating state にし、X/Y移動、幅/高さscale、aspect lock、五点基準、任意角回転を dialog と handle drag の双方で操作する。
- selection 内に描画内容がなければ明確な no-content error とし、履歴を増やさない。

### 17. 履歴、復帰、preview

- undo/redo は直前 command と複数段階移動を扱う。Undo 後の新規編集で redo branch を破棄する。
- pointer down から up までの stroke、shape 確定、fill、filter apply、layer operation、paste commit をそれぞれ一 command とする。
- dialog preview は base state から毎回再計算し、parameter slider を動かすたびに結果へ累積適用しない。
- `実行`/`OK` は一回の commit、`キャンセル` は base state へ完全復元する。
- `復帰` は最後の通常保存、`部分復帰` は保存 snapshot の active plane/selection 部分を使う。

### 18. フィルタ、特効、レタッチ、調整レイヤー

- filter は selection があればその内側、なければ active plane 全体へ適用する。
- sharpen強/弱、blur強/弱は固定 preset。unsharp mask は radius、amount、threshold。Gaussian blur は radius/strength を持つ。
- 階調反転は対象 channel の値を反転する。auto contrast は histogram に基づき範囲を拡張し、alpha を色 histogram へ混ぜない。
- brightness/contrast は独立 parameter。
- tone curve は RGB/R/G/B channel、control point、B-spline/Bézier、reset、native preset save/load を持つ。
- levels は RGB/R/G/B、input shadow/gamma/highlight、output shadow/highlight、histogram を持つ。
- hue/saturation/value と color balance は preview 可能で、色空間と clamp/rounding を文書化する。
- gradient は線形/放射、3色以上のstop、各stop alpha、stop追加/移動/削除、45度 constraint、元画像へ合成/上書き、dither を持つ。selection なしでは全 plane を対象とする。
- airbrush effect は指定した二色以上の境界部分だけに設定幅のgradientを作る。通常 blur のように均一領域全体をぼかさない。
- blur tool は pen/rectangle/polyline/lasso 範囲、太さ、screen-size固定、blur size、pressure を持つ。
- stamp は Alt+click 等で source point を決め、destination drag と同じ offset で複製する。shape、size、hardness、spacing、pressure->size/opacity を持つ。
- adjustment layer は元 pixel を変更せず、brightness/contrast、levels、tone curve の parameter plane を保持する。複数作成、順序変更、再編集、visibility toggle ができる。
- alpha edit は raster plane の alpha channel だけを grayscale view で編集し、gradient 等も使える。通常 color plane を誤って変更しない。

### 19. バッチ処理

- batch palette は上から `入力`、順序付き `バッチ項目`、`出力` を表示する。set を複数保存、追加、削除、rename、並べ替えできる。
- 入力は folder、file、現在セルを含む sequence。対象 file の自然順と範囲を preview する。
- 各 operation は enabled、対象 layer selector、versioned parameters、必要なら `実行ごとに設定` を持つ。対象 layer が必要なのに空なら validation error。
- 出力は新規保存、複製保存、明示上書き、folder、cell folder、format、basename、開始番号、増減方向を持つ。
- `実行` は現在セルだけ、`全実行` は入力全体。file 間 wait、保存前 preview、progress、cancel、failure policy を持つ。
- operation 候補はゴミ取り、color replace、continuous fill、airbrush effect、全 filter、image size/resolution、mirror、90度回転、line width、2値彩色変換、raster汎用変換、layer visibility、separation。
- line width batch は加算/減算/倍率/一定幅を対象 type に応じて検証する。
- continuous fill は色と document X/Y seed の複数行を持ち、同じ座標が全 frame で意図した領域に残るか再生 preview する。各 frame で通常 fill semantics を使う。
- color replace は旧色/新色の複数 pair、行ごとのenable、全pair反転、native preset save/load を持つ。
- 二枚の同位置セルから color pair を抽出する場合は、差分色の曖昧さとalphaをpreviewする。
- layer visibility batch は名前だけでなく安定selector/typeを使い、存在しない対象のskip/error policyを明示する。
- separation は指定色をmask化し、単色置換、主線planeへ送る、彩色planeへ送る、別file出力を選べる。
- airbrush effect batch は境界を構成する複数色と幅を設定する。
- 一件ごとにtemp outputからatomic commitし、cancel/失敗したfileに部分出力を残さない。dry-runは一切書かない。

### 20. 形式、白透過、一般画像入出力

- native `.inkpod` は、保存時点の可変 raster snapshot を意味上の正本にしない。正本は immutable な `Genesis`、content-addressed な `Assets`、Core が検証・正規化して実変更を確定した `Procedures` と history control event、history の現在位置と high-watermark を持つ `META`、文書単位の `EditorState` とする。materialized document、inverse delta、COW snapshot、render/checkpoint cache は派生物であり、これらだけで文書を成立させない。
- `Genesis` は document UUID、paper、DPI、sRGB、frame、margin、初期 stable-ID topology、immutable base surface を完全記述する。白紙の base surface は全面 tile を割り当てない opaque white の `SolidWhite` underlay とし、flat canonical composite/export には参加するが、個別 layer/plane export や selection mask へ暗黙に混入させない。
- import、clipboard、Light Table 等の外部入力は ingestion 時に Rust が canonical pixel/vector payload へ変換し、immutable `AssetId` を発行する。procedure は外部 path、codec の再実行、caller buffer の lifetime を参照しない。元 encoded bytes や provenance は replay に影響しない任意 metadata としてのみ保持できる。
- 永続 journal は閉じた型 `Commit`、`HistoryMove`、`BranchCut` だけを持つ。実変更を確定した document transaction、実際に移動した Undo/Redo/history jump、history cursor が active branch の tail 以外にある状態からの新規 commit による branch cut だけを順序どおり記録し、query、invalid、failure、cancel、stale、overflow、no-op、stroke/preview の途中更新は記録しない。stroke end、preview apply、floating commit は成功時にそれぞれ一つの canonical procedure とする。
- `.inkpod` section は history procedure/control event を `PROC`、history cursor、active branch、document/editor savepoint と各 persistent ID の high-watermark を `META` に置く。独立した `HIST` section は作らない。`EDIT` は active tool、最後の色付き command、tool ごとの exact-depth color、diameter、fill/selection/vector option、active layer/plane、palette cursor 等の再開に必要な文書単位 editor state を保持する。`CKPT` は任意の open 高速化 cache、`EXTM` は replay に影響しない任意 metadata とする。
- 通常保存後の reopen は画像だけでなく、history list/cursor、Undo/Redo availability、active/non-active branch、document/editor savepoint、persistent ID high-watermark、EditorState を復元する。通常 UI から外れた redo branch も監査可能な append-only journal と asset retention root に残し、自動 squash しない。
- persistent `StateId` は Genesis と commit 済み意味状態を参照し、procedure の precondition、history、savepoint に使う。`DocumentRevision` は stale request 検出用の session-local counter であり file へ保存せず、open 時に新しい Core generation 内で rebase する。EditorState は document history と別の persisted editor revision/digest/savepoint を持ち、session dirty は document state または editor state のいずれかが各 savepoint と異なれば成立する。
- 同じ replay epoch、Genesis、Assets、canonical procedure/control-event 列から、x64、ARM64、非 Windows Rust target で同じ canonical Core state と bit-exact な canonical composite を得る。Direct2D/D3D の画面 antialiasing や monitor 表示の一致はこの契約に含めない。primitive semantics が replay 結果を変える場合は replay epoch と top-level format version を更新する。
- ユーザーがフォーマットフリーズを宣言するまで、`.inkpod`、`.inkbatch`、native preset等のapplication固有の永続化ファイル形式は現在versionだけを読み書きし、下位互換reader/writer、migration、互換shimを持たない。現在の要件に対して最も頑健で効率的なschemaを選ぶ。この規則はHKCUのworkspace layout recordには適用しない。
- コードフリーズまでは、serialized schemaを変更するたびに対象形式の最上位format versionを必ずインクリメントする。section/record versionだけの変更で代用せず、旧versionは明示的に拒否する。
- 一般 raster import/export は少なくとも PNG、TIFF、TGA、BMP の対応可能な 8/16 bit、alpha、DPI を扱う。形式が表せない情報はflatten/export optionで明示する。
- legacy workflow の `白背景を合成` をexport optionとして持つ。onなら最下層へ白を合成してalphaを除き、offならformatが許すalphaを保持する。
- legacy white-transparency modeでは完全な白を透明候補としてcheckできるが、native documentでは白色pixelと透明alphaを同一視しない。
- 一枚 export とcut/sequence exportを分け、後者は対象layer、全体/作画frame、size/DPI、antialias、連番規則を設定する。
- 旧固有拡張子を、内容がnative JSON等であるだけの別形式へ流用しない。

### 21. 未指定事項の決定

- 旧UIのpixel位置、icon、配色、window dockingの細部は再現対象にしない。
- 本文書が数値や内部表現を一意に定めない場合は、安全性、頑健性、効率、決定性を優先したnative仕様を定義し、式、rounding、fixture、testを記録する。
- 永続化schemaに影響する決定は、前節のフォーマットフリーズ前version規則に従う。

## 横断的な状態遷移契約

- 同じ初期状態と入力列は、thread 数、tile 順、hash iteration 順にかかわらず同じ result class と意味上の結果を返す。
- 操作結果は success、no-op、invalid、cancel、stale revision を区別する。Undo 対象となる一つの確定 document edit は一つの document revision と一つの history entry だけを進める。
- no-op は document revision、history、dirty、render content を変えない。invalid、cancel、stale revision、overflow、失敗は document、history、dirty、revision、確定 snapshot、通常出力 file に部分変更を残さない。
- 一つの document edit は一回の Undo で直前の意味状態へ戻り、一回の Redo で同じ結果へ進む。Undo 後の新規 edit は以前の redo branch を破棄する。
- view-only edit は document revision、history、dirty を変えず、意味上の変更がある対象 view の revision だけを進める。document edit は必要な render cache invalidation を起こす。render cache の source identity は永続化しない派生状態とし、対象座標にある可視 plane の `tile_revision`、selection の `tile_revision`、Light Table の `source_revision` の数値最大値を使う旧 revision-max 方式を正本とする。cache hit 判定はこれらの scalar revision だけを読み、source pixel のcopy、走査、hash、digest、generation、tombstone、epoch、negative cacheを使ってはならない。zoom、pan、flip、viewport 等の view-only snapshot は、変更されていない source tile の pixel payload を再コピー・再hashしてはならない。revision-maxへ含まれないrender metadata変更は同じcommit境界でwhole-cache invalidationを行う。数値最大値の衝突、高いLight Table revisionによるmask、同値revision sourceの削除、表示mode間の共有cache、および透明合成結果の非保持は、性能を優先した正本方式の既知制約とする。このruntime cache方針はM8 native schemaへ保存せず、M8によって自動変更しない。
- 通常 save の成功だけが通常 savepoint を進める。autosave、recovery save、export は通常 savepoint を進めない。
- stable ID は所属 document/session 内の生存 object 間で重複せず、保存、Undo/Redo、snapshot を通して参照関係を維持する。layer、plane、view 等の別 namespace を混同しない。
- 長時間処理は base revision、cancel、target generation を確定し、全計算と検証が成功した場合だけ結果を公開する。

## 仕様と追跡

- 本文書を機能、利用者向け挙動、要件 ID の正本とし、`AGENTS.md` を作業規律、技術境界、品質基準の正本とする。
- 実装済み範囲、test、既知差分は `docs/compatibility.md`、現在状態と直近の代表的検証は `docs/implementation-status.md`、現在の所有権・thread・data flow は `docs/architecture.md` で管理する。
- 完了工程や過去の検証ログは Git 履歴を参照し、恒久仕様へ時系列記録を追加しない。
- 仕様と既存 test だけでは安全に決められない製品挙動は、実装前に選択肢と影響を示してユーザー判断を求める。

## 要件 ID

`docs/compatibility.md` と test 名で次の ID を使い、実装状況を追跡してください。分割してもよいですが、意味を失わないでください。

### Foundation

- `ARCH-001`: CMake -> Cargo staticlib -> MSVC link の一方向 build
- `ARCH-002`: Rust Core に Windows 型が存在しない
- `ABI-001`: versioned C ABI、opaque handle、ownership test
- `ABI-002`: immutable batched render snapshot
- `IO-001`: versioned `.inkpod`、atomic save、round-trip、recovery
- `IO-002`: PNG/TIFF/TGA/BMP import/export と alpha/white background option
- `WIN-001`: Windows shell、Help/About、DPI/theme/keyboard behavior
- `WIN-002`: 同一 process/UI thread 上の複数 `WorkspaceWindow`、window-local focus/menu/status、application activation、最後の window による shutdown
- `WORKSPACE-001`: 制約付き dock、最大二つの `EditorGroup`、named workspace、versioned layout persistence と monitor/DPI recovery
- `WORKSPACE-002`: pane scope、follow/pin/job target、発行時 `CommandContext`、ID/generation による stale routing rejection
- `SESSION-001`: 複数 `DocumentSession` の file identity、view/document/window/application close、save/Save As、autosave/recovery lifecycle
- `SAFE-001`: malformed/corrupted input の bounded rejection と非破壊性
- `PERF-001`: large sparse/COW document、bounded dense workload、変更 tile だけの再合成、およびsource raster payloadを走査しない連続zoom/pan snapshotを保護する。Core quick/fullの`pan_zoom_snapshot`は2,048/8,192 pair、`dirty_tile_rebuild`は同一allocated tileへの1 pixel stroke＋snapshot rebuildを32/128回測り、private native performance smokeは1024平方・256 allocated tileに対する512 wheel eventを各1 Presentまで、16本のmulti-sample/multi-tile strokeを各1 Presentまで測る。checksum、revision、再利用/rebuild tile数、payload access、sample数、Present数、queue/resource counterはwall-clockと独立した意味ゲートとして常時一致させ、初回composeでpayload accessを観測した同じfixtureについて128回のcache-hit zoom snapshot後のpayload access増分を0とする。通常のwall-clock判定は、同一workload/profileと一致する`docs/core-benchmark-baseline.md`の承認済み環境別reference envelopeを使い、一回以上のwarm-up後5回以上の中央値を比較する。下限未満は処理省略の診断値であり、意味ゲートが正常な高速化は許可する。上限を超えた場合は独立した5回以上を再測定し、両方の中央値が上限を超えた場合に回帰として拒否する。native wheelは記録したdisplay refresh intervalで正規化し、CPU側zoom回帰はCore scenarioで判定する。detached旧revision-max buildとのA/Bはworkload/harness変更、reference環境追加・変更、envelope再設定、または境界結果の明示監査時だけ行う。envelopeは自動緩和せず、変更には環境、全sample、意味counter、理由の記録とユーザーの明示承認を必要とする
- `PKG-001`: Rust/Win32 の静的 CRT、x64/ARM64 self-contained MSIX、ならびに ZIP 直下へ `inkpod.exe`、`README.txt`、`LICENSE.txt`、`ThirdPartyNotices.txt` だけを収録する x64/ARM64 portable payload と package/dependency 検証
- `PORT-001`: Rust workspace の OS 非依存性と次 frontend の adapter gap

### Document and view

- `DOC-001`: CellDocument、用紙、DPI、100 frame、基準/作画/安全 frame、余白
- `DOC-002`: stable ID を持つ typed layer/plane tree
- `DOC-003`: create/duplicate/delete/reorder/show/edit/opacity/convert/merge
- `VIEW-001`: zoom、box zoom、fit、1:1、pan、horizontal/vertical flip
- `VIEW-002`: ruler、guide/grid、snap、transparent view
- `VIEW-003`: color locator の座標/RGBA/selection sampling と magnified neighborhood 表示・編集
- `VIEW-004`: 複数文書 tab、同一文書 view、二分割 group、group/window 間の移動と複製
- `HIST-001`: transaction、Undo/Redo、savepoint、revert、preview cancel

### Paint and color

- `PAINT-001`: pencil、brush、eraser、auto erase、pressure
- `PAINT-002`: line/curve/shape/polyline と preview commit
- `PAINT-003`: gap connect、dust removal、line width correction
- `FILL-001`: connected seed fill、tolerance、selection
- `FILL-002`: 含み塗り、overflow abort、gap close、detached regions
- `FILL-003`: closed-region fill、transparent-only、fill extension
- `COLOR-001`: RGBA 8/16、RGB/HSV、eyedropper source
- `COLOR-002`: palette、chart、subpalette、color check

### Selection and editing

- `SEL-001`: rect/ellipse/lasso/polyline/trace/wand selection
- `SEL-002`: new/add/subtract/intersect/invert/expand/shrink/color selection
- `SEL-003`: selection layer conversion と vector selection modes
- `CLIP-001`: typed clipboard、standard clipboard、document coordinate preservation
- `XFORM-001`: destructive mirror/rotate/size/resolution と非破壊 view transform の分離
- `XFORM-002`: floating selection move/scale/rotate、preview/commit/cancel

### Animation workflow

- `LT-001`: light table set、per-item transform/color/opacity、global opacity
- `LT-002`: reference-frame alignment、boundary/color sampling、edit image swap
- `SEQ-001`: cut/cell sequence、前後セル、欠番、thumbnail preview
- `SEQ-002`: motion check、FPS、loop、step、selection/light table option
- `SHORT-001`: 全 menu command への single/multi-stroke shortcut、text-focus guard、prefix-free resolve、conflict replacement、永続化、reset

### Image processing and batch

- `FILTER-001`: sharpen/blur/Gaussian/invert/auto contrast
- `FILTER-002`: brightness/contrast、curve、levels、HSV、color balance
- `EFFECT-001`: gradient、airbrush、airbrush boundary effect、blur tool、stamp
- `ADJUST-001`: non-destructive adjustment layer と alpha edit
- `BATCH-001`: persisted Input -> Operations -> Output graph
- `BATCH-002`: line width、continuous fill、replace、visibility、separate、filter/effect
- `BATCH-003`: dry-run、preview、progress、cancel、per-output atomicity、failure report
- `VECTOR-001`: path/variable width/fill/color-trace model と rendering
- `VECTOR-002`: vector draw/erase/connect/width/select/convert
