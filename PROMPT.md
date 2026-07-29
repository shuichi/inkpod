# inkpod 機能・実装仕様

この文書は、inkpod が維持する利用者向け機能、挙動契約、互換性の境界、プロジェクト固有の実装指針を定める恒久仕様である。完了済み工程の進捗、過去の検証ログ、作業再開用プロンプトは含めない。

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

通常の実装作業では外部の PaintMan マニュアルや掲載画像を参照しないでください。この `PROMPT.md` に正規化した文章仕様を、旧ワークフロー互換性の正本として扱ってください。仕様が競合した場合は次の順に判断してください。

1. 今回のユーザー指示
2. リポジトリの `AGENTS.md`
3. この `PROMPT.md` の内蔵機能仕様と requirement ID
4. 既存コードがテストで保証する契約
5. Windows 11 の標準的な操作慣習

この文書で決めていない旧 proprietary binary の byte layout や旧設定ファイル形式は、推測で実装しないでください。DGA/CEL の layer semantics をネイティブモデルで再現することと、旧 DGA/CEL ファイルを直接読み書きすることは別です。実 fixture と独立した検証方法がない codec は `Unknown` または `Experimental` と記録し、互換書き出しを有効にしないでください。

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

main frame は標準的な Windows 11 desktop application とし、次の領域を持たせてください。

- 最上段: menu bar。
- 独立した常設 toolbar は置かない。利用者が実行できる全機能を menu bar の末端項目から呼び出せることを優先する。選択中 tool の option strip は toolbar ではなく、同じ command/state を表示する context pane とする。
- この節で定める固定 dock workspace を Windows GUI の正規構成とする。既定値へ戻した 96 DPI の配置は、上端に全幅 40 DIP の選択中 tool options、下の body 左端に幅 80 DIP の一列 tool pane と 4 DIP splitter、中央に高さ 28 DIP の document tabs と Canvas、右端に幅 320 DIP の inspector と 4 DIP splitter、最下段に status bar とする。tool options は中央だけでなく main frame の client 幅全体を使う。
- 右 inspector は color/palette/chart tabs を上、layer/plane pane を下に固定し、既定の上下比を 32:68、間の splitter を 4 DIP とする。layer/plane pane 内は layer を上、plane を下に固定し、既定比を 55:45 とする。両方の上下比は splitter で変更できる。
- tool pane は 72 x 34 DIP の button を左右 4 DIP の余白と 3 DIP の縦間隔で一列に並べ、7 pt の読み取れる一語ラベルを表示する。正規ラベルは `鉛筆`、`ブラシ`、`消しゴム`、`塗りつぶし`、`閉領域塗り`、`塗り延ばし`、`スポイト`、`直線`、`曲線`、`長方形`、`楕円`、`折れ線`、`線消しゴム`、`グラデーション`、`エアブラシ`、`境界ブラシ`、`ぼかし`、`スタンプ`、`ゴミ取り`、`アルファ階調` とする。意味を推測させる一文字略号へ戻さず、詳細な正式名は tooltip で補う。選択中 tool は一つだけ明示する。
- 中央: 一つ以上の document tab と custom Canvas `HWND`。tab label は active sequence cell 名、保存 file 名、`無題セル N`、`復元セル`の順で意味のある識別名を使い、dirty は `*`、同じ document の追加 view は`[ビュー N]`で示す。同じ document の multi-view も別 tab/view として開ける。
- tool、tool options、color/palette/chart、layer/plane は main frame の固定 dock pane とし、主要 workspace の正規構成として floating frame へ変換しない。pane 幅、上下 split、表示状態、左右反転を DPI 非依存値で保存し、初期配置、明示保存配置、直前 session を区別する。左右反転時は inspector を左、tool pane を右へ入れ替え、全幅の tool options と status bar は維持する。各 pane を隠すと Canvas がその領域を直ちに回収し、狭い window では 320 DIP の最小 Canvas 幅を優先して inspector を一時的に退避する。
- color locator、light table、cell/sequence、subpalette、file preview、batch 等、正規 workspace に含めない補助 UI は必要に応じて独立した modeless floating palette として実装できる。未実装機能の placeholder child control は生成しない。
- 下段: status bar。現在 tool/active plane、document 座標、zoom/view flip/grid、pixel RGBA/selection 寸法、文書寸法/DPI、処理進捗、dirty 状態、複数ストローク入力待ちを短く表示する。
- 固定 dock pane は `WS_CHILD` とし、menu から表示を切り替えられる。補助 UI の floating frame は閉じる操作で破棄せず非表示にし、終了時に placement を保存し、起動時は現在の monitor work area を検証してから復元する。各 top-level palette は独立した `WM_DPICHANGED` 処理を持ち、keyboard navigation と high contrast を尊重する。top-level palette に `WS_EX_TOPMOST`、`WS_EX_PALETTEWINDOW`、`WS_EX_NOACTIVATE` は使わない。
- menu、shortcut、context menu は同じ command ID と enable/checked state を共有し、同じ処理を重複実装しない。
- 全の実行可能な menu 末端項目に shortcut を割り当て、menu label に現在の割当を表示する。`Ctrl+S`、`Ctrl+O`、Undo/Redo、clipboard など標準操作は一般的な割当を維持する。描画・選択・塗りなど高頻度操作は、text 入力に focus がないときの single stroke を基本とする。その他は短い prefix-free な multi-stroke を使い、入力待ちを status bar に表示する。
- 色 palette の `1`–`0`、次 group の `Tab` は数値入力中でない場合の高速操作として保持する。shortcut は検索可能な設定 dialog で最大4 strokeまで再割当てでき、完全一致の衝突は元の割当と交換し、prefix 衝突は拒否する。
- 実行不能 command は disable する。例として vector layer で pencil、選択なしの一部 command、対象 layer 未指定の batch を無言で成功させない。

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

- 正規 workspace の表示切替は `ツールパレット`、`ツールオプション`、`カラー`、`レイヤー／プレーン` の四つとする。`カラー`は color/palette/chart tabs 全体、`レイヤー／プレーン`は layer/plane pane 全体を切り替える。補助 UI は実装されたものだけ個別の表示切替を追加し、menu、shortcut、control で同じ command ID と checked state を使う。
- `新規セルビュー`: 同じ document を別 viewport で開く。
- `フルスクリーン`。
- `パレットの整頓 > 初期位置/現在位置を保存/保存位置へ戻す/左右を反転`。

#### ヘルプ

- `inkpod ヘルプ`、`ショートカット一覧`、`診断情報`、`inkpod について`。旧製品名や旧 asset を自社製品表示として使わない。

### 4. 起動、終了、読み込み、セル切替、保存

- 起動時は Common Controls、COM、renderer、Rust Core の順に初期化し、途中失敗を status とユーザー向け説明へ変換して確実に unwind する。
- 開いたセルと同じ sequence/folder にある画像は file preview に自然順で表示する。thumbnail click、前/次 command、番号指定で切り替える。
- active cell が dirty の状態で別セルへ移る場合は、設定に応じて保存確認または自動保存を行う。cancel ならセルを切り替えない。
- `前のセル` と `次のセル` は欠番を飛ばし、設定で末尾から先頭へ循環できる。
- 通常保存、自動保存、recovery、export は別 status とし、自動保存成功だけで通常 savepoint を進めない。
- 保存は temp file 完成後の置換とし、失敗しても元ファイルを残す。起動時に新しい recovery があれば、開く/破棄/後で判断を選べる。

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
- 色を使う active command は、鉛筆、ブラシ、フィル、選択、エアブラシ、各ベクター描画 tool ごとに独立した現在色を持つ。command 切替時はその command の現在色を復元し、color editor、swatch、数値欄へ即時反映する。スポイト等の色を持たない一時 tool は直前の色付き command を変更先として維持する。
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

- native `.inkpod` は layer/plane、frame、history savepointに必要なmetadata、palette、light table、selection、color depthを保持するversioned container。
- 一般 raster import/export は少なくとも PNG、TIFF、TGA、BMP の対応可能な 8/16 bit、alpha、DPI を扱う。形式が表せない情報はflatten/export optionで明示する。
- legacy workflow の `白背景を合成` をexport optionとして持つ。onなら最下層へ白を合成してalphaを除き、offならformatが許すalphaを保持する。
- legacy white-transparency modeでは完全な白を透明候補としてcheckできるが、native documentでは白色pixelと透明alphaを同一視しない。
- 一枚 export とcut/sequence exportを分け、後者は対象layer、全体/作画frame、size/DPI、antialias、連番規則を設定する。
- 旧固有拡張子を、内容がnative JSON等であるだけの別形式へ流用しない。

### 21. この仕様で未確定として扱うもの

- DGA/CEL および旧 palette/chart/filter preset のbinary byte layout。
- 旧ソフト内部の厳密な色距離、filter kernel、放送色域判定式のうち文章で一意に決まらない部分。
- 旧UIのpixel位置、icon、配色、window dockingの細部。
- 未確定事項は新しい安全なnative仕様として定義し、`docs/compatibility.md`へ差分、式、rounding、fixture、testを記録する。
- 実データとの厳密互換が必要になった時点で、権利上利用可能なfixtureと期待出力を追加し、既存native semanticsを壊さない独立codecとして実装する。

## 文書の使い分け

- 新機能や挙動変更では、本文書の関連機能、要件 ID、禁止事項を先に確認する。
- 実装済み範囲と未対応範囲は `docs/compatibility.md`、現在の主要差分と直近検証は `docs/implementation-status.md` で確認する。
- 実ファイル、依存関係、所有権、thread 構成の詳細は `docs/architecture.md` を参照し、本文書へ一時的な構造や行数を複製しない。
- 完了済み工程の根拠が必要な場合は Git 履歴と該当テストを使い、本文書や status 文書へ時系列ログを再追加しない。
- 仕様と既存テストだけでは安全に決められず、選択によって保存形式や互換性を不可逆に変える場合は、実装前にユーザー判断を求める。

## 必須アーキテクチャ

### Rust workspace

次の責務を分離し、循環依存や Win32 型の混入を許可しない。

- `inkpod-image`: raster tile、pixel/color、selection、fill、filter、vector geometry
- `inkpod-format`: `.inkpod` container、manifest、codec trait、common raster import/export
- `inkpod-core`: project/cut/cell、layer/plane、command、history、workspace、render snapshot
- `inkpod-ffi`: C ABI 変換と panic containment だけを持つ `staticlib`

Core の公開 Rust API は C ABI から独立させてください。FFI 用の pointer validation や `#[repr(C)]` 型を domain model に浸透させないでください。

### Windows frontend

- `wWinMain`、Unicode Win32 API、Common Controls v6、Per-Monitor DPI v2
- main frame、menu/status bar、全幅の上 tool options、左の一列 tool dock、右上 color/palette/chart と右下 layer/plane dock、中央の document tabs と custom Canvas child window。主要四 pane は固定 dock とし、toolbar は作成しない
- Canvas ごとに swap chain を持ち、D3D11/DXGI surface から D2D device context を作る
- Rust snapshot の raster/vector/text/overlay を Direct2D primitive へ変換する
- resize、minimize、occlusion、DPI change、theme change、device lost を処理する
- file picker、drag and drop、clipboard、known folder、message loop は C++ に閉じ込める
- pen/mouse/touch を正規化して input batch として Rust へ渡す

Direct2D resource を Rust に渡したり、C++ で document state を別に持ったりしないでください。

Windows frontend は、少なくとも次の三つの長寿命 thread に責務を分離してください。

1. UI/Input thread: `HWND`、message loop、Common Controls、pointer history の取得、client device-pixel 座標への正規化、bounded input/command queue への投入を担当する。描画中に Core の完了や `Present` を待たない。
2. Core engine thread: `InkpodCore` をこの thread で生成・使用・破棄し、single-writer として command と `stroke begin/append/end/cancel` を順序どおり処理する。描画中の immutable preview snapshot を表示 cadence 以下で発行する。
3. Renderer thread: D3D11 device、DXGI swap chain、Direct2D device context、GPU tile cache と `Present` をこの thread で生成・使用・破棄し、最新の immutable snapshot を表示 cadence で描画する。

thread 間は所有権を明示した queue で接続してください。renderer は置き換えられた古い snapshot/frame を破棄してよい一方、pointer sample や stroke の begin/end/cancel を描画負荷軽減のために破棄してはいけません。Core thread から Canvas の window message queue へ Rust 所有 pointer を裸で積まず、snapshot の受取側が成功/失敗の両方で release 責務を引き受ける C++ queue を使ってください。

Canvas の view transform は client の物理 device pixel を基準とし、`device = document * zoom + pan` とします。Direct2D Canvas は pixel unit/96-DPI target で同じ transform を使い、Per-Monitor DPI は menu、dialog、status bar、modeless palette 等の UI scaling と実寸表示 policy にだけ反映してください。同一の client size と view state で DPI 変更だけにより Canvas が移動・縮小してはいけません。

### C ABI

- opaque `InkpodCore*` と immutable `InkpodSnapshot*`
- ABI version と各 public structure の `struct_size`
- fixed-width integer、pointer + length、UTF-8
- snapshot を一括取得し、要素ごとの FFI call を避ける
- allocate 側が release する
- `catch_unwind`、NULL/length/alignment/enum validation、thread 契約
- C11 と C++20 の header compile test

最低限、create、dispatch batch、stroke begin/append/end/cancel、snapshot build/view/release、error copy、destroy を提供してください。API 名を変える場合は `docs/ffi.md` へ理由と所有権を記録してください。

### Windows frontend の内部境界

- `main.cpp` は起動 mode の解釈と application runner の呼び出しに限定し、feature command、dialog、pane、smoke scenario を置かない。
- `Application` は初期化、起動時 recovery、message loop、shutdown 順序を所有する。domain operation は実装しない。
- main window procedure は `WM_*` の正規化と委譲に限定する。各 command ID は一つの feature owner だけが処理する。
- command の enabled/checked state は feature ごとの副作用のない query から構築し、menu、shortcut、context menu、存在する palette control が同じ結果を使う。query 中に Core、tool、preview、document を変更しない。
- controller は担当 state と typed input だけを受け取り、別 controller の private state を直接変更しない。Core 操作は `CoreEngine` を介して Core engine thread へ送る。
- dialog entry point は dialog 固有の typed initial value と result を使う。dialog module は完全な application state、`CoreEngine`、Rust FFI を受け取らず、Cancel では caller state を変更しない。
- top-level frontend state は lifetime、window、document shell、tool、view、pane、animation、effects、batch 等の owner state を合成する。完全な context を全関数へ渡したり、C++ 側に第二の document model を作ったりしない。
- private declaration は `apps/windows` 以下に閉じ、公開 `include/inkpod` API へ出さない。汎用の `helpers.*`、`common.*`、`utils.*` や、全機能を知る巨大 controller を作らない。
- `--smoke-test` と `--abi-smoke-test` は実製品の UI/Core/renderer/ABI 経路を検証する private entry point として維持する。

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
- `COMPAT-001`: rights-cleared fixture/oracle に基づく legacy codec 実測範囲
- `SAFE-001`: malformed/corrupted input の bounded rejection と非破壊性
- `PERF-001`: large sparse/COW document と bounded dense workload の benchmark
- `PKG-001`: x64/ARM64 self-contained MSIX payload と release signing 境界
- `PORT-001`: Rust workspace の OS 非依存性と次 frontend の adapter gap

### Document and view

- `DOC-001`: CellDocument、用紙、DPI、100 frame、基準/作画/安全 frame、余白
- `DOC-002`: stable ID を持つ typed layer/plane tree
- `DOC-003`: create/duplicate/delete/reorder/show/edit/opacity/convert/merge
- `VIEW-001`: zoom、box zoom、fit、1:1、pan、horizontal/vertical flip
- `VIEW-002`: ruler、guide/grid、snap、transparent view
- `VIEW-003`: color locator と同一文書 multi-view
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

## 実装の詳細規則

### 文書とメモリ

- raster は sparse tiled storage と copy-on-write を使う。
- width x height x bytes を無検査で確保しない。
- document ID、layer ID、plane ID は安定 ID にする。
- view state と document state を別 revision にする。
- savepoint は history 上の位置または同等の確実な方法で管理する。

### フィル

- 再帰 flood fill は使用しない。
- scanline または明示 queue を使い、訪問上限、selection、tile boundary を検査する。
- color distance と alpha の式を明文化する。
- gap close は入力を先に破壊せず、仮想境界または別 transaction で扱う。
- overflow abort は all-or-nothing transaction とする。
- 同じ seed/config/state なら thread 数に関係なく同じ結果にする。

### 履歴

- pointer down から up までを一 stroke とする。
- stroke は begin/append 中の preview state と確定 document state を分け、end だけを一 history entry として commit し、cancel/failure は開始前へ完全復元する。
- filter/transform dialog は preview state を commit state と分ける。
- Undo 後に新規編集した場合、redo を無効にする。
- file save、autosave、export を同じ dirty semantics にしない。

### renderer

- Core snapshot の tile revision で GPU bitmap を cache する。
- dirty tile だけ upload する。
- immutable snapshot は Core engine thread から所有権付き queue で Renderer thread へ渡し、古い未描画 snapshot は release して最新を優先する。
- pointer sample は UI/Input から Core engine へ順序どおり渡し、renderer の遅延を理由に破棄しない。
- Canvas の document/view/device 変換は client device pixel に統一し、D2D の暗黙 DIP 変換と二重適用しない。
- minimize/occlusion 時に無駄な rendering loop を止める。
- device lost で Core document を破棄しない。
- transparent color、white compatibility check、selection overlay を元画像へ焼き込まない。

### background work

- decode、encode、filter、batch は worker で実行可能にする。
- worker は `HWND` や Common Controls を操作しない。
- completion 時に document revision を照合する。
- cancel と app shutdown で worker lifetime を回収する。

## 追跡文書の責務

- `docs/compatibility.md` は要件 ID ごとの状態、実装、test、既知差分の正本とする。状態は `Not started`、`In progress`、`Experimental`、`Verified`、`Blocked`、外部形式の実測範囲には `Unknown` を使う。
- `Verified` は対応する user-facing または明示的な Core-only 契約と再現可能な test がある場合だけに使う。source が移動しただけでは状態を変更しない。
- `Blocked` は不足 fixture、利用不能な toolchain、外部判断など、具体的な解除条件がある場合だけに使う。
- `docs/implementation-status.md` は現在の実装状態、既知差分、直近の代表的な検証だけを記録する。完了工程、時系列の作業ログ、古い test count を蓄積しない。
- `docs/architecture.md` は現在の構造と ownership を説明する。移行前の行数、段階名、進捗表を設計規則として使わない。
- 過去の詳細は Git 履歴に残し、常時参照する文書へ複製しない。

## テストと CI

変更範囲に応じて、最低限次を実行してください。

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cmake --preset <windows-preset>
cmake --build --preset <windows-build-preset>
ctest --preset <windows-test-preset>
```

実際の preset 名はリポジトリに定義したものを使ってください。非 Windows 環境では Rust の全検証を実行し、Win32 は Windows CI で検証してください。「この環境では Windows build を実行できない」ことを、コード未検証の言い訳にせず CI を追加してください。

さらに次を用意してください。

- pure Rust unit/property/golden tests
- `.inkpod` round-trip と malformed fixture tests
- C/C++ ABI smoke and negative tests
- Windows app creation/render smoke test
- codec/FFI fuzz targets
- large tile/fill/filter benchmarks

golden image は自作の単純な幾何 fixture を使い、旧製品由来の画像や第三者作品を含めないでください。

## 禁止事項

- Rust Core から Win32/COM/Direct2D を呼ぶ
- C++ に画像処理や history の別実装を作る
- 1 pointer sample、1 pixel、1 path element ごとに FFI を往復する
- Rust の `Vec`/`String`、C++ STL、例外、panic を ABI 越しに出す
- unbounded allocation、再帰 flood fill、無検査 path extraction
- 保存先ファイルを直接 truncate してから書く
- fixture のない DGA/CEL writer を「互換」として有効にする
- 旧 PaintMan の UI asset、スクリーンショット、長い説明文をコピーする
- button と menu だけを並べ、Core command に未接続のまま完成扱いする
- `TODO`、常時 `OK` の stub、空 callback を status `Verified` にする
- 現在の milestone と無関係な全面 rewrite
- test failure を削除、ignore、過大 tolerance で隠す

## 変更の完了条件

変更範囲に対応する success、no-op、invalid、cancel、Undo/Redo、必要な save/reopen を検証し、UI を持つ機能は UI から Core までの実経路へ接続する。diff と既存テスト契約を見直し、未実行の platform 固有検証と理由を明記する。

要件の状態や既知差分が変わった場合だけ `docs/compatibility.md` を更新し、現在状態または代表的な最新検証が変わった場合だけ `docs/implementation-status.md` を更新する。新規 codec、dependency、`unsafe`、ABI、file-format、ownership の変更は対応する設計文書へ記録する。
