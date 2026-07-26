# inkpod 実装用マスタープロンプト

以下を、このリポジトリで実装を担当する Codex へのプロンプトとして使用する。この文書は一度きりの雛形生成指示ではなく、複数回実行して最初の未完了 milestone から作業を再開できるようにしてある。

---

あなたは inkpod の principal engineer として、このリポジトリのコードを実際に設計・実装・検証してください。inkpod は、旧 PaintMan のアニメーション彩色ワークフローと合理的な互換性を持つ、長期保守可能な新規アプリケーションです。

最初にリポジトリ直下の `AGENTS.md` を全文読み、その規則をこのタスクの最上位の実装契約として扱ってください。計画や設計案だけで終了せず、コード、テスト、ビルド、必要な文書を変更し、現在の milestone を動く状態まで完成させてください。

## 目的

最終的に次の構成を実現してください。

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
- menu bar の下: 拡大/縮小、fit、1:1、表示反転、guide/grid、前後セル、保存等の高頻度 command を置く toolbar。常設の zoom slider は置かない。
- 左側の dock pane: tool palette。選択中 tool は一つだけ明示する。
- 中央: 一つ以上の document tab と custom Canvas `HWND`。同じ document の multi-view も別 tab/view として開ける。
- main frame に常設の右 dock pane は置かず、Canvas を利用可能な横幅全体へ広げる。tool options、layer/plane、color palette、color chart、color locator、light table、cell/sequence、subpalette、file preview は、必要な Core/C ABI の状態を保持したまま、後続作業で独立した modeless floating palette として実装する。再実装までは非表示の placeholder child control も生成しない。
- 下段: status bar。現在 tool、document 座標、zoom、pixel color、文書寸法、処理進捗、dirty 状態を表示する。
- modeless palette は表示/非表示と位置復元ができ、high-DPI、keyboard navigation、high contrast を尊重する。
- menu、toolbar、shortcut、context menu は同じ command ID と enable/checked state を共有し、同じ処理を重複実装しない。
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

- 各 modeless floating palette の実装後は、`ツールパレット`、`ツールオプション`、`レイヤー`、`カラー`、`カラーチャート`、`カラーロケーター`、`ライトテーブル`、`サブパレット`、`ファイルプレビュー`、`バッチ` の表示切替を提供する。
- `新規セルビュー`: 同じ document を別 viewport で開く。
- `フルスクリーン`。
- `パレットの整頓 > 初期位置/現在位置を保存/保存位置へ戻す`。

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
- 新規、複製、削除、menu、alpha edit を標準 button/toolbar から操作できる。
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
- toolbar と shortcut から拡大、縮小、数値倍率、slider、fit、1:1 を操作できる。1:1 は document pixel と表示 device pixel の関係を DPI policy とともに文書化する。
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

## 今回の開始手順

次を順に実行してください。

1. `git status --short`、tracked files、`AGENTS.md`、`docs/implementation-status.md`、既存の build/test 設定を確認する。
2. ユーザーの未 commit 変更を特定し、関係ない変更を保護する。
3. `AGENTS.md` の M0-M8 と status 文書を照合し、依存関係を満たす最初の未完了 milestone を今回の主対象にする。
4. status 文書がまだなければ M0 から開始し、このプロンプトにある status template を作る。
5. 変更前に短い実装計画を示すが、その後すぐ実装を開始する。
6. 現在の milestone の acceptance criteria をすべて満たすまで、モデル、ABI、UI adapter、テスト、文書を縦に接続する。
7. 現在の milestone が完了し、時間と安全な作業が残るなら次の milestone へ進む。後続を雑な stub で埋めるために品質を落とさない。

ユーザーへ設計選択を聞くのは、`AGENTS.md`、この文書の内蔵機能仕様、既存テストから安全に決められず、選択肢によって保存形式や互換性を不可逆に変える場合だけにしてください。ローカルで確認できることを質問しないでください。

## 必須アーキテクチャ

### Rust workspace

最低限、次の責務を分けてください。初期 milestone で空 crate を大量に作る必要はありませんが、循環依存や Win32 型の混入は許可しません。

- `inkpod-image`: raster tile、pixel/color、selection、fill、filter、vector geometry
- `inkpod-format`: `.inkpod` container、manifest、codec trait、common raster import/export
- `inkpod-core`: project/cut/cell、layer/plane、command、history、workspace、render snapshot
- `inkpod-ffi`: C ABI 変換と panic containment だけを持つ `staticlib`

Core の公開 Rust API は C ABI から独立させてください。FFI 用の pointer validation や `#[repr(C)]` 型を domain model に浸透させないでください。

### Windows frontend

- `wWinMain`、Unicode Win32 API、Common Controls v6、Per-Monitor DPI v2
- main frame、menu/toolbar/status bar、layer/plane panel、tool options、custom Canvas child window
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

Canvas の view transform は client の物理 device pixel を基準とし、`device = document * zoom + pan` とします。Direct2D Canvas は pixel unit/96-DPI target で同じ transform を使い、Per-Monitor DPI は menu、dialog、toolbar 等の UI scaling と実寸表示 policy にだけ反映してください。同一の client size と view state で DPI 変更だけにより Canvas が移動・縮小してはいけません。

### C ABI

- opaque `InkpodCore*` と immutable `InkpodSnapshot*`
- ABI version と各 public structure の `struct_size`
- fixed-width integer、pointer + length、UTF-8
- snapshot を一括取得し、要素ごとの FFI call を避ける
- allocate 側が release する
- `catch_unwind`、NULL/length/alignment/enum validation、thread 契約
- C11 と C++20 の header compile test

最低限、create、dispatch batch、stroke begin/append/end/cancel、snapshot build/view/release、error copy、destroy を提供してください。API 名を変える場合は `docs/ffi.md` へ理由と所有権を記録してください。

## 要件 ID

`docs/compatibility.md` と test 名で次の ID を使い、実装状況を追跡してください。分割してもよいですが、意味を失わないでください。

### Foundation

- `ARCH-001`: CMake -> Cargo staticlib -> MSVC link の一方向 build
- `ARCH-002`: Rust Core に Windows 型が存在しない
- `ABI-001`: versioned C ABI、opaque handle、ownership test
- `ABI-002`: immutable batched render snapshot
- `IO-001`: versioned `.inkpod`、atomic save、round-trip、recovery
- `IO-002`: PNG/TIFF/TGA/BMP import/export と alpha/white background option

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
- `SHORT-001`: rebindable shortcut、conflict replacement、reset

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

## 実装 milestone と acceptance criteria

### M0: 基盤

実装内容:

- root `CMakeLists.txt` と `CMakePresets.json`
- Cargo workspace と最低限の Core/FFI crate
- `include/inkpod/core_ffi.h`
- Win32 app shell、manifest、resources、空の Canvas renderer
- architecture、FFI、file format、compatibility、implementation status 文書
- Linux/macOS の Rust test と Windows CMake build を含む CI

Acceptance:

- 非 Windows で Rust workspace の format/lint/test が通る
- Windows CI で x64 Debug または Release が link し、app が main window と Canvas を生成できる
- CMake の Rust target は入力/出力/byproduct/dependency を正しく宣言し、毎回無条件 rebuild しない
- create -> empty snapshot -> release -> destroy の C/C++ smoke test が通る
- Core/FFI error path に panic、leak、二重解放がない

### M1: 保存できる描画 vertical slice

実装内容:

- 2 値彩色 CellDocument、主線 plane、彩色 plane、tiled raster
- new/open/save/reopen、dirty/savepoint、Undo/Redo
- raster snapshot と D2D tile cache
- UI/Input、Core engine、Renderer の三スレッド構成と、描画中 preview snapshot
- zoom/pan/fit/1:1
- mouse/pen pencil、eraser、描画色、主線/彩色 mode switch
- `.inkpod` v1 manifest と blob、atomic save

Acceptance scenarios:

1. 新規 1920x1080 文書を作り、主線へ stroke を描く。
2. 彩色 mode へ切り替え、主線を表示したまま彩色 plane へ描く。
3. 彩色操作後も主線 tile の checksum が変わらない。
4. 1 stroke を 1 回 Undo/Redo できる。
5. 保存、破棄、再読込後に layer/plane ID、pixel、frame metadata が一致する。
6. pan/zoom は文書 revision を変更しない。
7. 連続描画では sample を順序どおり batch/span で Core engine へ渡し、FFI は sample ごとの snapshot call を要求しない。
8. pointer up 前に stroke preview が一回以上表示される。その間 document revision、dirty、Undo history は変化せず、end は一つの Undo 単位、cancel は開始前の状態へ完全復元する。
9. UI/Input、Core engine、Renderer の thread ID が異なり、同じ client size/view なら DPI 変更の前後で document の device-pixel bounds が一致する。

### M2: 中核彩色

実装内容:

- 2 値/階調彩色 plane semantics
- seed fill、tolerance、selection、含み塗り、overflow abort、gap close
- closed-region fill と fill extension
- eyedropper sources、8/16 bit palette
- 純白/透明 check、autosave/recovery

必須 golden cases:

- 完全な閉領域だけが塗られる
- 1 pixel gap は設定 0 で漏れ、設定 1 で閉じる
- overflow abort 有効時は外周到達を検出し、画像を 1 pixel も commit しない
- 含み塗り対象の色トレース線は fill 色へ置換され、対象外の線は残る
- 階調主線の表示 coverage と基本色スポイトが一致する
- selection 外を変更しない
- 16 bit 値が 8 bit へ暗黙量子化されない
- autosave から復元しても通常保存ファイルを上書きしない

### M3: 文書編集

実装内容:

- typed layer/plane 全操作
- selection mask、wand/lasso/trace、boolean、expand/shrink、selection layer
- typed clipboard、coordinate preserving paste、floating transform
- frame、ruler、guide、grid、snap、flip
- locator、multi-view、shortcut editor

Acceptance:

- layer reorder/duplicate/delete を Undo/Redo/save/reopen できる
- 許可されない layer/plane 組合せを Core が拒否する
- selection boolean は property test を通る
- 異なる用紙サイズ間 paste でも document origin に対する座標を維持する
- view flip と destructive mirror を別の履歴・revision として扱う
- multi-view の一方で編集すると他方の次 snapshot に同じ revision が現れる

### M4: 制作ワークフロー

実装内容:

- cut/cell sequence と thumbnail preview
- reference frame と margin を含む用紙管理
- light table、subpalette、前後セル、item swap
- motion check と連番 import/export
- PNG/TIFF/TGA/BMP 8/16 bit と alpha

Acceptance:

- 異寸法セルを reference frame で重ねる golden test
- individual 50% x global 50% = effective 25% opacity
- light table を fill boundary に使っても参照画像を変更しない
- 前後セルを入れ替えても unsaved 文書を黙って破棄しない
- sequence の欠番と自然順を正しく扱う
- common format round-trip の bit depth/alpha/寸法/DPI を検証する

### M5: ベクター

実装内容:

- cubic path、可変線幅、主線、色トレース、塗り topology
- vector snapshot と D2D rendering
- draw、erase partial/intersection/full、connect、width correction
- vector selection modes と raster/vector conversion

Acceptance:

- zoom しても Core の vector geometry は変化しない
- partial erase が他 stroke を変更しない
- intersection erase の切断点が決定的
- fill topology が save/reopen で維持される
- rasterize 時の antialias、pixel center、scale を golden test で固定する

### M6: 画像編集

実装内容:

- filter 一式、preview transaction、last filter
- gradient、airbrush、boundary airbrush effect、blur、stamp
- adjustment layer と alpha channel edit

Acceptance:

- Cancel で元 tile checksum に戻る
- Apply は一つの Undo 単位
- adjustment layer は元 plane を変更せず、順序変更で composite が変わる
- 8/16 bit、alpha edge、selection edge を golden test する
- boundary airbrush effect が一様領域を通常 blur のように崩さない

### M7: バッチ

実装内容:

- versioned batch graph と設定保存
- input selectors、ordered operations、output policy
- continuous fill、color replacement、separation、visibility、line width、filters/effects
- preview/dry-run、progress、cancel、failure report

Acceptance:

- dry-run はファイルを書かない
- 既定 output policy は入力を上書きしない
- cancel された現在ファイルに一時出力を残さない
- 一件の失敗を記録し、policy に従って後続を継続または停止する
- color replacement の old/new 入替を round-trip する
- continuous fill の seed が別色へ移動した frame を preview で警告する

### M8: 互換性拡張と仕上げ

実装内容:

- 利用可能な実 fixture に基づく legacy codec
- fuzz、large document benchmark、memory/performance tuning
- Windows installer/MSIX の選定と packaging
- Core portability audit と次 frontend 向け API gap

Acceptance:

- legacy codec ごとに read/write/round-trip の実測範囲を compatibility 表へ記載
- 未検証 codec を `Verified` にしない
- corrupted file corpus で panic/OOM/overwrite を起こさない
- CMake が自己完結した Windows package を生成し、非管理者 payload smoke で
  executable、assets、license、notices、app-local runtime を検証できる
- 管理者権限を使う package install／installed ABI smoke／uninstall は任意の
  release-validation とし、M8 の完了条件には含めない
- Rust crates の Windows import がゼロであることを自動検査する

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

## status 文書の形式

`docs/implementation-status.md` には少なくとも次を含めてください。

```markdown
# Implementation status

## Current milestone

- Milestone: M0
- Status: In progress
- Last verified commit/worktree state: ...

## Requirements

| ID | Status | Implementation | Tests | Notes |
|---|---|---|---|---|
| ARCH-001 | In progress | ... | ... | ... |

## Verification

| Command | Platform | Result | Date |
|---|---|---|---|

## Known gaps and unknowns

- ...
```

状態は `Not started`, `In progress`, `Experimental`, `Verified`, `Blocked` に限定してください。`Blocked` は具体的な不足 fixture、toolchain、外部判断がある場合だけに使ってください。

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

## 今回の終了条件

最終目標は M0-M8 の全完了ですが、各実行では少なくとも「開始時点で最初に未完了だった milestone」を acceptance criteria まで完成させてください。大きすぎる場合も、その milestone 内でユーザーが実行できる縦切りを完成させ、残項目を具体的な requirement ID とテスト不足として status に残してください。計画だけで終了しないでください。

作業終了前に次を行ってください。

1. diff を見直し、ユーザーの既存変更を壊していないことを確認する。
2. format/lint/test/build を実行する。
3. `docs/implementation-status.md` と `docs/compatibility.md` を実態どおり更新する。
4. 未実行の Windows 専用検証と理由を明記する。
5. 新規 codec/dependency/unsafe/ABI change があれば、設計と所有権を文書化する。

最終報告は次の順で簡潔に記述してください。

- 利用者が実際にできるようになったこと
- 主要な設計・ファイル
- 実行した検証と結果
- 未対応・Experimental・Blocked の互換性項目
- 次に着手すべき requirement ID

---

このプロンプトを再実行したときは、既に `Verified` の milestone を最初から作り直さず、status とテストを根拠に最初の未完了項目から続行すること。
