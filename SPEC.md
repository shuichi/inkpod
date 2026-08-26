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

#### 1.1 カットと個別セルの所有・保存

- カットは stable `CutId`、永続 UUID、作品名、話、scene、cut 名、尺、指示、Cell 作成既定値、ordered membership を持つ。Cell membership は各 Cell の stable `CellId`、document UUID、正の表示番号、参照 file 名を組にし、名前や配列添字を identity にしない。
- 保存 topology は個別セル参照方式とする。Cut descriptor と各 CellDocument は同じ `.inkpod` 拡張子を使うが、別 magic で識別する。descriptor は同じ directory にある個別 Cell `.inkpod` を一 path segment の相対 file 名だけで参照し、absolute path、`..`、subdirectory、symlink による directory 外参照、descriptor 自身への参照を許可しない。directory 一式を同じ構造のまま移動することはできるが、Cell file だけの rename／移動は参照切れとして open を拒否し、silent に探索・修復しない。
- `WorkspaceWindow` 配下の一つの `CutSession` が一つの Cut handle、Cut 自身の revision、history、dirty、通常 savepoint、recovery association を所有する。各 `DocumentSession` は引き続き一つの独立 Cell Core、file identity、history、dirty、savepoint、recovery を所有し、Cut transaction と Cell transaction を一つに偽装しない。
- 新規 Cut は既存の Cell creation plan へ Cut defaults を明示 copy して各 Cell を独立作成・保存し、その成功済み identity だけを descriptor membership に入れる。defaults の後続変更は既存 Cell を silent に変更せず、その後に明示作成する Cell の初期値にだけ使う。
- Cut Properties の metadata／defaults 変更は base revision を固定した一つの Cut procedure、一 revision、一 history item とする。no-op、invalid、Cancel、stale、overflow、failure は Cut state、history、ID、dirty、savepoint、descriptor を進めない。Cut Undo／Redo は Cut 専用 command だけが動かし、active Cell の Undo／Redo と混ぜない。
- 通常 Cut save は全 member file の存在、同一 directory、stable Cell identity を staged 検証してから descriptor 一 file を同一 directory の temporary file 経由で atomic replace し、成功後だけ Cut savepoint を進める。Cell file の保存は別の明示境界であり、複数 file の見かけ上の atomic commit を約束しない。Cut autosave は通常 savepoint／path authority を進めず、recovery open は dirty とする。
- Cut open は exact-current top-level version、replay epoch、checksum、bounds、UTF-8、ID／path 重複、history chainを検証し、さらに全 member Cell を current Cell reader で staged openして `CellId` と document UUID の一致を確認してから live Cut を置換する。missing、renamed、duplicate、identity mismatch、path traversal、非current version、corruption は現在の Cut／Cell session を変更せず拒否する。
- Cut の Cell 系列編集は、発行時 Cut revision と順序付き operation 列を一つの request に固定する。operation は既存 Cell file の追加、membership からの除外、stable `(CellId, document UUID)` を使う before／after 移動、連続範囲の正の表示番号への採番を持つ。全 operation 適用後の identity、path、表示番号、上限を検証してから Cut state、State ID、revision、history、dirty を一回だけ公開し、no-op、invalid、Cancel、stale、overflow、failure では一つも進めない。
- 表示番号と一覧順は別の値であり、どちらも Cell identity や file 名の代用にしない。表示番号 0 は予約値として拒否し、同じ Cut 内の表示番号は重複させない。除外は descriptor membership だけを変更し、Cell file を削除、rename、移動しない。開いている Cell、Light Table、Batch、subpalette、autosave 等が保持する stable identity は付け替えず、membership から外れた参照は明示的な missing／orphan 状態として扱う。
- Cut 系列の Undo／Redo は Sequence pane に focus がある Cut command として一 transaction だけを移動し、active Cell の document Undo／Redo と混ぜない。相対 file 名は descriptor の immutable member asset table に保持し、永続 canonical history は stable member identity、表示番号、順序だけを参照する。reorder／renumber は member file を書き換えず、通常 Cut save は更新済み descriptor 一 file だけを atomic replace する。

### 2. Windows GUI の全体構成

Windows GUI は標準的な Windows 11 desktop application とし、古典的 MDI
や別 GUI framework へ移行せず、次の構造を持たせてください。

- process には一つの `ApplicationHost` を置き、同一 UI/Input thread 上で複数の `WorkspaceWindow` を所有できるようにする。各 window は独立した menu bar、制約付き dock、editor area、status bar、focus history を持つ。`WM_QUIT` は最後の workspace window が閉じたときだけ発行する。
- 独立した常設 toolbar は置かない。利用者が実行できる全機能を menu bar の末端項目から呼び出せることを優先する。選択中 tool の option は、左 tool button の独立した展開領域から開く縦型の owned flyout に表示し、同じ command/state と既存の option callback を使う。flyout は標準 caption、window icon、resize frame を持たず、30 DIP の compact header に accessible な pin toggle と close button だけを置く。非 pin 時は owner workspace または別 application へ pointer/focus が移った後に、flyout 自身と combo 等の owned popup を除外して自動的に閉じる。pin は workspace session 内だけの状態とし、自動 close を抑止するが system-wide topmost にはしない。flyout の高さは可視 control の末尾と下余白から算出し、monitor work area を超える場合だけ内部を縦 scroll する。配置は button の右側を優先し、収まらない場合は左右を反転して work area 内へ clamp する。
- editor area は一つまたは二つの `EditorGroup` を持つ。二分割は左右または上下だけを許し、再帰分割しない。各 group は独立した tab strip、active `DocumentView`、一つの可視 Canvas slot、focus history を持つ。
- 一つの `DocumentSession` は一つの `InkpodCore` handle、file identity、dirty/savepoint、Undo/Redo、autosave/recovery を所有する。同じ document の全 `DocumentView` は session を共有し、zoom、pan、flip、表示補助、表示中 frame 等の view logical state だけを分離する。文書 raster、layer、history、保存先を view ごとに複製しない。
- tab label は active sequence cell 名、保存 file 名、`無題セル N`、`復元セル`の順で意味のある識別名を使い、dirty は `*`、同じ document の追加 view は `[ビュー N]` で示す。read-only、処理中、error も compact かつ accessible な状態として示す。各可視 tab の右端には DPI 対応の小さな close icon button を置き、label drag と同じ hit target にしない。button は発行時の stable view identity を対象として view を閉じ、最後の view の場合だけ document close と dirty 確認へ進む。Cancel、save failure、stale target では tab、document、active view を変更しない。
- `CanvasSurface` は非表示 tab ではなく可視 `EditorGroup` ごとに一つ持つ。active tab の切替時に同じ surface を別 `DocumentView` へ bind し直し、非表示 tab 数に比例して swap chain や renderer thread を増やさない。
- dock zone は `TopContext`、`Left`、`Right`、`Bottom`、`Floating`、`Hidden`、`AutoHide` に制限する。各 zone は一方向に並ぶ比率分割枠を持ち、各分割枠は一つ以上の pane からなる tab stack とする。pane の表示、非表示、tab 選択は他の分割枠とその比率を変更せず、任意に再帰する dock tree を作らない。docked tab の内容領域に pane 固有の常設 close button を重複配置せず、非表示化は共通 pane command、floating 時の system close、または keyboard route から行う。pane descriptor は stable type ID、default/allowed zone、scope、multiplicity、float/autohide 可否、最小寸法を宣言する。
- Color、Layer/Plane、Locator 等の inspector pane は、一つだけの split stack でも descriptor の localized title を dock header に表示する。Tool の専用 strip はこの単独 header の対象外とし、Tool Options は dock pane ではなく owned flyout とする。単独の Tool strip は固定幅で zone-extent splitter、float、AutoHide を持たず、表示／非表示だけを許す。それ以外の splitter は 4 DIP の操作領域を維持し、通常、hover、pointer capture、keyboard focus、high contrast の各状態で system color により境界と操作可能性を識別できるようにする。focus の取得／喪失では同期的に境界を再描画し、別 component へ focus が移った後に強調色を残さない。
- Right zone の top-level tab は固定カテゴリを持たない動的な非空 tab とする。一つの pane type は高々一つの tab に属し、tab 数、各 tab の pane 数、縦順序は既知 pane descriptor 数で bounded にする。tab identity は label や配列 index ではなく nonzero stable layout ID を使う。label は縦方向の先頭 pane の localized title、tooltip と accessible description は所属 pane の全 localized title を順序付きで示す。label drag は drag threshold を越えた同じ top-level tab strip 内だけで順序を変更し、strip 外 drop、`Esc`、capture cancellation は配置を変更しない。
- 非表示の right pane を表示するときは、選択 tab の content height から tab strip と splitter を除いた高さに、全 pane の 96-DPI 基準 minimum height を一か所で DPI 変換した合計が収まれば末尾へ追加する。収まらない場合または選択 tab がない場合は、その pane だけを持つ新しい tab を作る。追加先を選択して pane の自然な先頭 focus target へ移す。表示済み pane の toggle は dock／floating／AutoHide の別によらず非表示にし、古い hidden membership は保持しない。
- 各可視 top-level tab の右端には label drag と重ならない DPI 対応の小さな close icon button を置く。button は発行時の stable layout ID を対象として所属 pane を一括で非表示にし、tab を一回の構造変更で削除する。選択 tab の replacement は直前、次、先頭の順で決め、invalid、capacity failure、stale target は pane、tab、selection を変更しない。pane header／context menu／keyboard から `新しいタブへ移動`、既存 tab への移動、tab／pane の並べ替えへ到達できる。window が狭い場合は editor area を優先し、必要なら未選択 tab label を一時的に抑制するが、model、selection、保存 record は変更しない。
- pane の target scope は `Application`、`FollowActiveView`、`PinnedDocument`、`Job` を区別する。pin 先 document が閉じた場合は別文書へ silent に向けず、追従 mode へ戻して accessible notification を出す。pane action は発行時の target ID と generation を保持する。
- 現在相当の一 window、一 group 配置を初期 named workspace `彩色` として維持する。96 DPI の初期値は body 左端に splitter なしの固定幅 80 DIP の一列 tool pane、中央に document tabs と Canvas、右端に幅 320 DIP の Color と Layer/Plane を縦配置した一つの動的 tab、最下段に status bar とし、上端の Tool Options dock strip は配置しない。既存の 32:68、55:45 比率と 4 DIP splitter は inspector 側の復元可能な layout state とし、単独 Tool strip の幅は対象外とする。
- tool pane の既定 button row は 64 x 34 DIP、一列とし、tool 選択用の主領域と幅 20 DIP の展開領域に分ける。両領域は bezel/border のない owner-draw の flat 表示とし、通常時は pane 背景へ溶け込ませ、hover 時だけ system color で背景を弱く反転し、checked/pressed、disabled、keyboard focus、high contrast を system color で区別する。展開領域の chevron は通常時に system gray text color で主 icon より弱く表示し、hover 時は通常 text color、checked/pressed 時は highlight text color とする。展開領域は `詳細` tooltip/accessibility name を持つ。主領域の正規ラベルは `鉛筆`、`ブラシ`、`消しゴム`、`塗りつぶし`、`閉領域塗り`、`塗り延ばし`、`スポイト`、`直線`、`曲線`、`長方形`、`楕円`、`折れ線`、`線消しゴム`、`グラデーション`、`エアブラシ`、`境界ブラシ`、`ぼかし`、`スタンプ`、`ゴミ取り`、`アルファ階調` とし、詳細名は tooltip で補う。
- named workspace と per-window layout は Workspace V9 の versioned、bounded な application setting として保存し、`.inkpod` 文書へ混ぜない。V9 は動的 tab の stable ID／順序／選択、tab ごとの pane membership／縦順序、pane split weight／visibility／dock・floating・AutoHide placement、選択 preset と既存 window／editor split を保持する。V2–V8 は最終的に V9 へ正規化し、V8 の可視な旧固定3 tab membership は動的な非空 tab へ移す。monitor/DPI 構成が変わった場合は可視 work area へ clamp し、不正 record、重複 pane／tab ID、空 tab、範囲外 count、不正 selected tab、overflow、trailing garbage は拒否して初期配置へ戻す。temporary な narrow-window adaptation で保存済み logical layout を上書きしない。
- built-in named workspace は `彩色`、`線整理`、`参照・チェック`、`バッチ`、`集中` を提供する。全 preset は空 tab と重複 pane を持たない。layout record は開いている文書 path、Core state、active stroke、job owner を含めず、未知 pane は無視し、不足する既知 pane は preset 既定値で補う。
- floating pane は owner workspace を持つ通常の owned top-level window とし、閉じる操作では既定で非表示にする。`WS_EX_TOPMOST`、`WS_EX_PALETTEWINDOW`、`WS_EX_NOACTIVATE` は使わず、独立した `WM_DPICHANGED`、keyboard navigation、high contrast、screen reader を扱う。
- 下段の status bar は現在 tool/active plane、document 座標、zoom/view flip/grid、pixel RGBA/selection 寸法、文書寸法/DPI、処理進捗、dirty 状態、複数ストローク入力待ちを短く表示する。
- menu、shortcut、context menu、pane button は同じ command ID と enable/checked state を共有する。command 発行時に immutable な `CommandContext` として workspace、group、session、view、pane/job、generation を確定し、非同期実行時に active tab を再解決しない。stale target は明示 error または安全な no-op とし、現在 active な別文書へ fallback しない。
- 全ての実行可能な menu 末端項目に shortcut を割り当て、menu label に現在の割当を表示する。`Ctrl+S`、`Ctrl+O`、Undo/Redo、clipboard など標準操作は一般的な割当を維持する。描画・選択・塗りなど高頻度操作は、text 入力に focus がないときの single stroke を基本とする。その他は短い prefix-free な multi-stroke を使い、入力待ちを status bar に表示する。
- 色 palette の `1`–`0`、次 group の `Tab` は数値入力中でない場合の高速操作として保持する。shortcut は検索可能なタブ式の環境設定 dialog で編集し、組み込み preset は全 command に完全な既定割当を持つ。ユーザー preset は組み込み preset の複製から作成し、command ごとに主キーと任意の副キーを持ち、各割当を未設定または最大4 stroke の列として再割当てできる。
- shortcut 割当は `Global`、`Canvas`、`Timeline`、`Pane` の context、`Execute`、`Hold`、`Toggle` の action、論理キーまたは物理位置の照合方式を型付き値として持つ。`Global` は全 context と重なり、その他は同じ context 同士だけが重なる。重なる context の完全一致は解決待ちの競合として編集候補内に保持できるが、未解決競合がある候補は適用または永続化しない。解決操作は競合相手の解除または主／副割当の交換とし、prefix 衝突は候補作成時に拒否する。`Hold` は一時 tool 等の明示対応 command、`Toggle` は表示切替等の明示対応 command だけが選択できる。
- shortcut editor は command 名、stable command key、割当キーの文字検索、入力キー検索、context filter、category／競合／変更あり／未割当の件数と絞り込み、競合の前後移動、選択 command の詳細と既定値、修飾キー別の物理 keyboard 可視化を持つ。keyboard 表示は自動、JIS 109、US ANSI 104 を選択でき、キー選択で割当へ移動し、command の drag で割当を作成できる。Win 修飾キーは表示と通常の foreground 入力で扱うが、OS 予約 shortcut を奪う global hook は使わない。
- shortcut preset の import/export は current-version の `.inkshortcuts` だけを受理する versioned、length-bounded な application data format とする。unknown version、不正 UTF、duplicate command/slot、範囲外 count、trailing data、未対応 enum、未解決競合を拒否し、export は同一 volume の temporary file を完成、flush、close してから置換する。
- tab drag は同一 group 内の並べ替え、別 group/window への移動、window 外 drop による新規 window を扱う。active stroke、pointer capture、modal preview 中は開始せず、`Esc` で cancel した場合は元の位置を完全復元する。同じ操作は drag に依存せず menu と keyboard からも実行できる。
- `Ctrl+Tab`/`Ctrl+Shift+Tab` は tab、`Ctrl+F6`/`Ctrl+Shift+F6` は editor group/view、`F6`/`Shift+F6` は menu・dock pane・editor area・status の focus、`Ctrl+F4` は view close に使う。tab、splitter、pane header、AutoHide、target、dirty、job progress と command の disabled state は UI Automation から取得できるようにする。
- 数値入力と選択肢を共有する modal dialog は、選択肢ごとに標準 combo box を使い、owner window の中央かつ monitor work area 内へ配置する。Cancel は表示前の状態を変えない。
- 実行不能 command は disable する。例として選択なしの一部 command、対象 layer 未指定の batch を無言で成功させない。未接続 button、空 pane、常時成功する stub は生成しない。

### 3. メニュー構成

UI 表示文字列は、日本語と英語を言語非依存の型付き ID で参照する一つの catalog で管理する。単語単位の部分置換で表示文を組み立てず、各言語の完成した文または format string を catalog に置く。文書名、path、Light Table set 名等のユーザー所有文字列は翻訳せず、catalog 由来の prefix/suffix と明示的に合成する。`編集 > 環境設定 > 全般` で `システム設定`、`日本語`、`English` を選択でき、次回起動から process 内の全 workspace に適用する。`システム設定` は Windows の第1優先 UI 言語が `ja` の場合だけ日本語を選び、それ以外または判定不能時は英語を選ぶ。選択値は versioned、bounded な HKCU application setting として保存し、不正 record は `システム設定` へ戻す。言語は文書、履歴、native file、ユーザー入力の名前や path に混ぜない。実行可能な button、checkbox 等の catalog 由来 caption は、各 pane の最小幅と 96/120/144/192 DPI 相当の標準 UI font で全文を表示し、必要なら操作行を折り返す。省略表示を許すのは文書名や path 等の可変長ユーザー所有文字列であり、操作 caption の切り詰め、略称化、font 縮小で代用しない。以下は機能上必要な top-level menu と command です。Windows の標準慣習に合わせた mnemonic、ellipsis、並びの小調整は許可します。

#### ファイル

- `新規 > セル`: 用紙、pixel 寸法、DPI、100 frame、frame 配置、layer 種類、色深度、作成枚数を指定してセルを作る。
- `新規 > カット`: 作品/話/scene/cut 名、既定用紙、初期 layer、色深度、尺、作成枚数を設定し、descriptor と同じ directory に個別 Cell `.inkpod` を作る。
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
- `編集 > 環境設定`: application／workspace 単位の環境設定を category tab に集約する。少なくとも全般、保存・復元、workspace、animation、color 管理、keyboard shortcut を持ち、document、view、tool、batch operation 固有の設定を混ぜない。dialog は typed initial value と候補だけを所有し、`適用`／`OK` の検証と永続化が成功するまで live state を変更せず、`キャンセル` は最後の適用後の状態へ完全に戻す。shortcut page は上記の preset、検索、競合、一覧、詳細、物理 keyboard を提供し、command 数と各 category 件数を production command catalog から算出する。
- `設定 > グリッド`: 間隔、分割数、原点を指定する。

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
- overlay 表示は document pixel へ焼き込まない。

#### ウィンドウ

- 表示切替は `ツールパレット`、`ツールオプション` flyout、`カラー`、`レイヤー／プレーン` と、実装済みの補助 pane を列挙する。`ロケーター`、`シーケンス`、`ライトテーブル`、`サブパレット`、`参照`、`バッチ` は submenu ではなく、それぞれ一つの直接 checked toggle とする。menu、shortcut、pane control は同じ command ID と checked state を使い、checked state は dock／floating／AutoHide を含む実可視性、`ツールオプション` は flyout の可視性を表す。Color と Batch の文書固定／追従 command は Window menu に重複配置せず、各 pane の target control から操作する。
- `新しいビュー`: active document の別 `DocumentView` を active group に作る。
- `ビューを閉じる`、`文書を閉じる`: 前者は focused view だけを閉じ、後者は全 window/group の該当 view を列挙して document session を一度だけ閉じる。
- `右へ分割`、`下へ分割`、`別グループへ移動`、`別グループに新しいビュー`、`グループを閉じる`: 最大二 group の editor area を command/keyboard から操作する。
- `新しいウィンドウ`、`ビューを新しいウィンドウへ移動`、`新しいウィンドウに複製ビュー`: 同一 process、同一 UI/Input thread 上の workspace window を操作する。
- `ワークスペース`: named workspace の選択、保存、名前を付けて保存、復元、既定に戻す、および pane の dock/float/hide/auto-hide command を提供する。
- `フルスクリーン`。
- current `彩色` preset との移行互換として、従来の `初期位置`、`現在位置を保存`、`保存位置へ戻す`、`左右を反転` の意味を named workspace command から到達可能にする。

#### ヘルプ

- `inkpod ヘルプ`、`Inkpodファイルフォーマット`、`ショートカット一覧`、`診断情報`、`謝辞`、`inkpod について`。謝辞は使用する production 外部 library、その用途と license、および完全な第三者通知の参照先をオフラインで表示する。旧製品名や旧 asset を自社製品表示として使わない。

### 4. 起動、終了、読み込み、セル切替、保存

- 起動時は Common Controls、COM、renderer、Rust Core の順に初期化し、途中失敗を status とユーザー向け説明へ変換して確実に unwind する。
- 同一 user/session では一つの論理 application instance とする。Explorer/file association 等からの secondary activation は command line を完全検証してから versioned、length-bounded、current-user 限定の IPC で primary process へ渡し、既定では last-focused workspace の active group を対象にする。明示的な `新しいウィンドウで開く` だけが新規 workspace を作り、primary timeout 時に同じ native file を別 process で無断編集しない。
- `ファイル > 開く` は focused workspace の active editor group に新しい document tab を追加する。同じ file identity が既に開いている場合は既存 `DocumentSession` の view を選択し、通常操作で二つの独立 session を作らない。別 view が必要な場合は `新しいビュー` を明示的に使う。
- file identity は Windows の volume/file ID を取得できる場合はそれを使い、取得できない場合は正規化した絶対 path を使う。表示名や tab index を identity に使わず、untitled document には frontend が UUID を発行する。
- 開いたセルと同じ sequence/folder にある画像は file preview に自然順で表示する。thumbnail click、前/次 command、番号指定で切り替える。
- active cell が dirty の状態で別セルへ移る場合は、versioned application setting で `Prompt` または `Autosave-before-switch` を選ぶ。Prompt の cancel、自動保存の失敗、発行後 stale、queue rejection では現在セルと未保存編集を保ち、durable な native recovery artifact と metadata の publication 成功後だけ対象セルへ切り替える。
- 自動保存済みセルは sequence entry の document UUID と source generation に関連付け、戻る際は exact native state を staged Core で検証・replayしてから active Core を交換する。flattened preview source から history、layer/plane tree、selection、editor state を再構成しない。
- `前のセル` と `次のセル` は、自然順に存在する entry だけを対象として欠番を飛ばす。closed な端点 policy は `Stop` と `Wrap` の二つとし、`Stop` は先頭から前／末尾から次を完全な no-op、`Wrap` は先頭と末尾を相互に切り替える。空 sequence と一件だけの sequence も別の明示 no-op result とする。
- 端点 policy は application-wide の versioned、length-bounded な HKCU setting とし、既定は `Stop`、missing／malformed／noncurrent record も `Stop` へ戻す。同一 process の全 workspace window は同じ値を使い、`連番・サブパレット > 端点で循環` の menu checked state、設定 command、configurable shortcut、status／accessibility 表示を一つの setting へ接続する。この値は document／EditorState／canonical procedure ではなく、document revision、history、journal、dirty、savepoint、`.inkpod` format を変えない。
- 通常の前後セル command は発行時の direction、端点 policy、sequence revision、source／target の document UUID、source generation、自然順 index、cell number を固定する。commit 前に Core が同じ target を再解決し、発行後の sequence／source／target 変更は stale として原子的に拒否する。通常 navigation の端点 policy と motion check 自身の loop setting は独立とする。
- 通常保存、自動保存、recovery、export は別 status とし、自動保存成功だけで通常 savepoint、document path authority、dirty 表示を進めない。
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

#### その他のレイヤー

- `ラスター汎用`: 背景・特効用 RGBA 8/16 bit。alpha channel を持てる。
- `フレーム`: 基準 frame、作画 frame、安全 frame、撮影 frame。
- `消失点`: 一つ以上の消失点と補助線設定。
- `選択範囲`: selection mask の保存と編集。
- `調整`: 明るさ/contrast、levels、tone curve を非破壊で保持する。

composite は layer/plane 順、visibility、opacity、alpha、adjustment を決定的に適用してください。プレーンは所属 layer を越えて並べ替えず、layer 同士と同一 layer 内 plane 同士を別に並べ替えます。
layer と同一 layer 内 plane はどちらも配列 index 0 を palette の最上段、すなわち合成結果の最上位とする。Canvas、layer thumbnail、flatten export は、raster と adjustment が混在しても同じ木順序を下から上へ合成し、adjustment は置かれた位置までの合成結果へ適用する。

#### 角度付き撮影 frame の確定 contract

- 文書は、通常の用紙・作画・安全・撮影範囲を持つ既存の axis-aligned `FrameMetadata` とは別に、角度付き撮影 frame object を 0 個または 1 個保持する。object は文書 namespace 内で再利用しない stable `ShootingFrameId`、milli-pixel document 座標の center、正の width/height、`u32::MAX + 1` を1回転とする時計回り binary turns、左上／右上／中央／左下／右下の操作 anchor、Canvas 表示 flag、指示 export 含有 flag を持つ。rotation はすべての `u32` を正規化済みの一回転範囲として受理し、座標と corner 計算は checked fixed-point で行う。frame は Canvas 外にまたがってよい。
- center と size/rotation が geometry authority であり、anchor は数値入力と handle 変形の固定点を選ぶための永続する意図である。五点は回転後の四 corner と center に対応し、Core の共通 geometry だけが corner、hit test、handle transform を決める。OS DPI、device pixel、renderer 固有の演算は canonical state と replay に含めない。
- create、complete-replacement update、delete は typed canonical executor と共通 transaction 境界を使う。preview は base document/revision と作業用 document を保持するlong-lived sessionとし、OK は一回の Undo 単位、Cancel は base への完全復元とする。no-op、invalid、Cancel、stale、overflow、failure は document/revision/history/journal/dirty/savepoint/ID high-watermark を進めない。
- 角度付きobjectは Canvas と利用者が明示的に選ぶ「指示入りラスター書き出し」にだけ含める。通常の完成画像 raster export、layer/document thumbnail、paper fit、crop bounds からは必ず除外する。通常 export と「撮影フレームを考慮して用紙サイズ変更」の唯一の authority は既存 `FrameMetadata::shooting_frame` であり、両者を暗黙に相互変換しない。指示 export では include flag が on の場合だけ、straight-alpha sRGB RGBA8 `#FF4040FF` の 1 document-pixel outline として決定的に合成する。
- document mirror/quarter-turn rotate は center、rotation、anchor を厳密に変換する。resample なしの canvas resize は選択 anchor の offset だけを center に加える。resample は等方 scale、またはobjectの辺が document 軸に平行な四分の一回転のときだけ同じ oriented rectangle へ厳密に写す。それ以外の非等方 resample は直交矩形で表現できないため、raster や metadata を部分変更せず文書 transform 全体を `InvalidArgument` で拒否する。

### 6. レイヤー・プレーンパレット

- 上段に layer、下段に active layer の plane を表示する split pane とする。
- layer/plane 間の splitter は pointer と keyboard の双方で高さを変更でき、可視かつ accessible にする。共通の下部操作 button には、現在の操作対象が layer と plane のどちらかを視覚表示と accessible name の双方で明示する。
- 各行に visibility、editable/target、name、種類に応じた color/thumbnail を表示する。
- visibility と editable の状態 button は、16 DIP の icon を維持した 32×32 DIP の正方形を 4 DIP 間隔で行中央に配置する。描画と hit test は同じ矩形を使い、完全な状態名は行の accessible text に保持する。
- active selection と複数 edit target を区別する。描画 command は active plane と明示 target 規則を検証する。
- drag and drop で同階層の順序を変える。
- opacity は数値と slider で変更する。
- 新規、複製、削除、property、alpha edit は必ず menu から操作でき、modeless palette を追加する場合も同じ command ID へ委譲する。
- 複製名は一意にする。削除は Undo 可能とし、必須 plane を最後の一枚まで削除できないよう validation する。
- 同種統合は同じ種類だけを対象にし、plane color 等の互換条件が異なるものを黙って統合しない。
- property dialog では name、type、opacity、plane color 等を編集し、type conversion は損失内容を事前表示する。
- 新規プレーンでは種類と形式を番号入力にせず、列挙値に対応する文字列を標準コンボボックスから選ぶ。全候補は選択可能とし、OK 時に選択中レイヤーの topology 制約を Core で再検証する。使用できない組み合わせはエラーを表示してダイアログを閉じない。

### 7. 用紙とフレーム

- 新規 Cell は一つの条件入力で `frame size` または `image size`、DPI、各辺余白、初期 layer 種類、8/16 bit 色深度、五点 anchor、作成枚数を指定する。作成枚数は 1 以上 64 以下とし、複数作成は全件成功時だけ focused workspace の active EditorGroup へ独立した untitled document として公開する。Cancel、invalid、overflow、UUID/割当/途中 staging failure では Core、session、tab、recent file、stable ID を一件も進めない。
- `image size` の入力幅・高さは最終 raster の正確な pixel 数である。各辺余白率を `m/1000` とすると、100% frame の幅・高さはそれぞれ `round_ties_even(image * 1000 / (1000 + 2m))` とし、残差は左/上へ切り捨て半分、右/下へ残りを置く。
- `frame size` の入力幅・高さは 100% frame の物理寸法 μm である。各軸の frame pixel 数は `round_ties_even(μm * dpi_milli / 25,400,000)`、各辺余白 pixel 数は `round_ties_even(frame * m / 1000)` とし、最終 raster は frame と両辺余白の和とする。全換算は整数の ties-to-even、checked arithmetic、現在の raster 寸法上限を用い、OS DPI を適用しない。
- 作画 frame と撮影 frame は新規作成時の 100% frame と一致する。安全 frame は 100% frame の指定比率を中央 anchor で縮尺し、最大寄り frame は指定比率を左上/右上/中央/左下/右下の選択 anchor で縮尺する。基準 frame の X/Y は同じ五点 anchor における 100% frame 上の基準座標とし、frame 寸法自体は 100% frame 寸法を保持する。preview と commit は同じ immutable な Core creation plan を使う。
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
- line、curve、rectangle、ellipse、polygon、polyline の Canvas 入力は、pointer-down 時に固定した view ID と view revision で device pixel から document 座標へ一度だけ変換し、同じ view の snap checked state を適用する。stale または閉じた view は active view へ置き換えず拒否する。OS DPI はこの変換へ重ねて適用しない。
- snap master が off の場合は bounded raw document point を使う。grid は原点と分割後の間隔ごとに各軸を最近点へ丸め、ちょうど半分は 0 から遠ざかる側を選ぶ。guide は元の各軸が guide から 4 document pixel 以内の場合に grid より優先し、複数候補は position、stable ID の昇順で最後の guide を選ぶ。解決前後は用紙の左上から右下の far edge までへ clamp する。
- `Ctrl` を pointer-down 時に押している間は図形入力の guide／grid snap だけを一時解除する。`Shift` の45度／縦横比制約、`Alt` の既存操作は変更しない。snap 解決は文書、view revision、history、journal、dirty、savepoint を変更せず、確定した geometry だけが既存の一つの Undo 単位になる。
- 透明表示は設定色または checkerboard で示し、pixel 値を変更しない。
- color locator は cursor 周辺を別倍率で表示し、X/Y、selection 幅 H、高さ V、対角長 L、RGBA を表示する。active raster stroke 中は、確定前の最新 preview と最新の処理済み pointer 座標へ bounded/coalesced に非同期追従し、query 自体は document revision、history、journal、dirty、savepoint を変更しない。End 後は確定結果、Cancel 後は元の文書へ再同期する。固定 mode では locator 上で編集でき、edge 付近は自動 scroll を選べる。
- multi-view は一つの document state と history を共有し、viewport transform だけを別に持つ。
### 9. 描画・線修正ツール

すべての tool option は tool button から開く Tool Options flyout の縦型 page に表示し、別の `詳細…` modal dialog へ転送しない。flyout は pencil/brush/eraser の共通値と、fill、selection、geometry、eyedropper、gradient/airbrush/blur/stamp/dust/alpha-gradient/boundary-airbrush の tool 固有値を既存 state/callback と双方向同期する。stroke/shape 確定前の preview と確定後の command を分離し、開いただけでは文書を変更しない。boundary airbrush のような即時破壊処理は page 内の明示的な `適用` でだけ実行する。

#### スポイト

- source は `最上位の非透明プレーン`、`選択中プレーン`、`合成表示色`、`ライトテーブル最上位色` から選ぶ。
- 描画 tool 中の `Alt` または右 click で一時 eyedropper を使える。light table source 用 modifier も command mapping として定義する。

#### 鉛筆

- click で一点、drag で1 document pixelの線を描く。
- 階調主線では1 pixel相当の antialias coverage を描く。
- stroke 開始 pixel が描画色と同色なら stroke 全体を erase mode にする auto erase を持つ。`Shift` で auto erase を一時無効にする。

#### 消しゴム

- tool options の先頭に `消去対象: 主線 / 彩色` を常時明示し、選択中の layer/plane、menu の主線/彩色 command、status bar と双方向に同期する。消しゴム選択だけでは対象を自動変更しない。
- shape、太さ、zoom に対して screen size を維持するか、pressure を太さへ反映するかを選ぶ。
- raster は cursor footprint 内を透明/背景へ消す。

#### 直線・曲線・図形・折れ線

- 直線: start から end へ drag し、release で確定。
- 曲線: start/end を drag 後、control point を動かし click で確定する単純な curve workflow。
- 図形: 長方形、楕円、N角形。outline color/width、fill color、吸着、aspect ratio、中心から作成、作成後回転を持つ。
- 折れ線: click で頂点追加、double click で終了。始終点を結ぶ、区間を Bézier 化する option を持つ。
- line 系は入り、抜き、吸着、45度制約、断面形状を必要に応じて持つ。`Shift` は aspect/angle constraint として一貫させる。

#### ブラシ・エアブラシ

- brush は丸/角、太さ、pressure、stroke smoothing、開始 pixel と同色領域だけへ描く mode を持つ。
- 開始色限定 brush は stroke 開始時の変更前 raster を immutable base とし、開始 pixel の native-depth 値と完全一致する pixel だけを各 brush footprint 内で描く。Binary／Grayscale 8/16 bit は格納 scalar、RGBA 8/16 bit は straight alpha を含む全 RGBA 成分を比較し、tolerance、表示変換、premultiply 後の値を比較へ使わない。4 近傍の連結性は要求せず、stroke が到達した footprint 内なら非連結の同値 pixel も対象にする。stroke 中に描いた色で predicate を拡張せず、開始 pixel が用紙外なら invalid とする。
- brush smoothing は off または 0〜1000 の整数強度 `s` とする。Core は document Q16.16 の各 x/y 座標について、最初の sample を変更せず、二点目以降を `round_ties_even((previous_normalized * s + raw * (1001 - s)) / 1001)` で因果的に正規化し、pressure は変更しない。中間積は符号付き固定幅の検査付き演算とし、同じ入力 sample 列は frontend の batch 分割、OS pointer history の通知単位、thread 数にかかわらず同じ canonical sample 列と pixel 結果を返す。
- airbrush は太さ、硬さ、dab 間隔、fade、pressure->size、pressure->opacity、停止中も時間で濃くなる continuous spray を持つ。

#### ゴミ取り

- 適用範囲は pen、rectangle、polyline、lasso。
- mode は `背景/透明以外の小点を除去`、`透明/背景の小穴を周囲色で埋める`、`周囲と異なる小領域を周囲色へ置換`。
- 最大サイズを指定し、必要な線を消す可能性を preview で確認する。
- tool は局所、filter は選択または plane 全体を一括処理する。

#### 線つなぎ

- 指定範囲内で設定 gap 未満の端点候補を結び、raster の接続線幅を指定できる。
- tool は drag した範囲、menu command は既存 selection を対象にする。
- 誤接続を避けるため候補の距離、角度、対象 plane を決定的に評価し、Undo 一回で戻す。

#### 線幅修正

- 適用範囲は pen/rectangle/polyline/lasso。
- `指定幅だけ太く`、`指定幅だけ細く` を別 mode にし、raster morphology として処理する。

#### 消失点

- `VANISHING-POINT-001`: `LayerKind::VanishingPoint` は stable ID を持つ一つ以上の消失点 object を所有し、各 object は Canvas 内外の signed document milli-pixel 座標、1/5/10/15/30度 preset または 1〜180度の fixed-point custom 間隔、180度周期で正規化する開始角、exact sRGB RGBA8/16、0〜1000 の不透明度、表示状態を保持する。一文書64個、snapshot内の導出放射線16384本を上限とする。
- 表示中の消失点だけから、現在 viewport と交差する有限な放射 segment を immutable snapshot へ導出する。通常 raster／thumbnail／instruction export には焼き込まず、Canvas overlay として描く。文書の回転・反転・Canvas移動・等方resampleでは幾何を追従させ、等角放射線を保存できない非等方resampleは全体を変更せず拒否する。
- dialog と Canvas handle から追加・移動・更新・削除・全削除できる。preview は同じ immutable base から再計算し、Cancel は無変更、OK は一 transaction／一 Undo 単位とし、新規 ID は commit 時だけ消費する。独立 native preset はこの要件に含めず、設定は native document にだけ保存する。
- snap master と guide snap が有効な場合、元入力から4 document pixel以内の最寄り放射線へ拘束する。explicit H/V guide は該当軸で radial guide より優先し、radial guide は grid より優先する。放射線候補は距離、`VanishingPointId`、正規化角の昇順で最後の候補を選び、snap master offまたはguide snap offでは放射線を入力へ適用しない。

### 10. 色、パレット、チャート、参照画像

- Color pane 内の `カラー`、`パレット`、`チャート` は semantic ID と表示順を分離した三つの tab とする。label drag は drag threshold を越えた同じ内部 tab control 内だけで順序を変更し、pane からの undock、Right zone の top-level tab への移動、個別 loading／unloading は行わない。active page と既存 child `HWND` を維持し、control 外 drop、`Esc`、capture cancellation は順序を変更しない。この内部順序は workspace record や `.inkpod` へ保存しない session-local presentation state とする。
- 描画色は sRGB RGBA 8/16 bit を保持し、RGB と HSV editor、alpha 数値/percent 表示を切り替える。
- 色を使う active command は、鉛筆、ブラシ、フィル、選択、エアブラシ等の raster command ごとに独立した現在色を持つ。鉛筆の既定色は黒、その他の彩色用 command の既定色は彩色用の初期色とする。command 切替時はその command の現在色を復元し、color editor、swatch、数値欄へ即時反映する。color pane は文書の主線色と active command の彩色用描画色を別のラベルと swatch で常時区別する。スポイト等の色を持たない一時 tool は直前の色付き command を変更先として維持する。
- color ring、HSV triangle、alpha track の pointer drag は pane-local preview を各入力 sample で即時描画し、button release 時だけ現在色を Core/editor state へ公開する。capture cancellation は drag 開始時の色と hue へ復元し、preview 中に palette/chart list や他 pane を全更新しない。
- color palette は複数 page/group を持ち、cell click で描画色取得、modifier+click で現在色登録、clear/save/load ができる。
- 高頻度の10色は `1`から`0`へ割り当て、`Tab`で次の10色 group へ切り替える。shortcut editor で変更可能にする。
- color chart は色と名前を表形式で管理し、複数 page、検索、次候補、lock、cut/copy/paste、save/load を持つ。旧版の5文字制限は native 形式へ課さない。
- `セルからカラーチャートを作成` は一意色を抽出するが、gradient/antialias 画像で色数が過大になるため、最大数、quantization、preview を用意する。
- chart生成previewは発行時のdocument revisionと同じbase compositeから毎回再抽出し、直前候補へ再量子化しない。previewは候補色、頻度、色数超過、元chartとの差分summaryをboundedに返し、chart、history、journal、dirtyを変更しない。Apply tokenがstaleなら別revisionや別chartへ適用しない。
- 生成結果のApplyはdocument paletteではなくdocument所有のColor chart全体を一transactionで置換する。native depthとstraight alphaを含む完全一致色が既存chartにある場合は最初の同色entryの名前を保持し、新規色だけ1始まりの最終順序に基づく`Color N`を既定名とする。消えた色の名前は残さない。chart lock中はpreviewを許可するがApplyを拒否し、lock状態自体を変更しない。
- chartの現在pageと選択位置はEditorStateでありdocument historyへ含めない。Apply後も選択色が完全一致で残ればそのentryへ追従し、残らない場合だけ先頭entry／page 0へ移す。空chartでは選択なし／page 0とする。Color chart entries、名前、lockとEditorState cursorは通常save/reopenで復元する。
- `SUBPALETTE-001`: subpalette は文書や追従先から独立した workspace 単位の参照 viewport とし、ユーザーが複数選択した PNG/TIFF/TGA/BMP または指定 folder 直下の同形式画像を表示する。folder は再帰走査しない。stem の末尾の十進数字列をセル番号として昇順に並べ、番号付き画像を先、番号なし画像を自然順で後に置く。同じ表示名でも別 source として保持する。
- subpalette viewport 上の pointer は active tool に関係なく常に eyedropper とし、cursor も通常の pointer icon と同程度の外形寸法に収めた eyedropper 表示にする。成功した sampling は表示変換を通した半開区間の device-pixel 座標から元画像の exact native-depth RGBA を取得し、pane 内で採取色を表示する登録 button、現在の描画色、および Color pane の選択対象へ反映する。pointer の押下、移動、離上に含まれる有効 sample は連続して採取し、cancel は採取しない。`取得色を登録` は最後に成功した subpalette sample だけを document palette へ追加し、sample 前は無効とする。file/folder open、前後移動、全体、等倍、採取色登録は標準 pane 幅では compact な一行 toolbar とし、幅不足時だけ折り返す。各 button は UI language に対応する操作名 tooltip を持つ。
- 画像は前へ／次へ toolbar button と Left／Up／PageUp、Right／Down／PageDown で順送り・逆送りできる。keyboard navigation は subpalette 内のどの操作 button または viewport に focus があっても機能する。file／folder を開く操作と、前へ、次へ、全体表示、等倍表示は localized accessible name を保つ icon button とする。source の置換または active image の変更時は、表示 snapshot と renderer cache を新しい画像へ切り替える。追従先、pin、アクティブ追従、現在セル、自動的に一つ前、Canvas scroll 連動は subpalette UI に置かない。
- file／folder を開いた時は対象画像を bounded background load で全件読み込み、全 decode が成功した一つの memory-resident cache として置換する。前へ／次へと keyboard navigation は再度の file I/O／decode を行わず、この cache の active image と snapshot だけを切り替える。別 source を開く時は新 cache の完成まで旧 cache と表示を保ち、成功時に旧 cache と旧 renderer source generation を一括破棄する。読込、decode、個数、aggregate memory 上限のいずれかが失敗した場合は旧 cache と選択を保つ。
- file read/decode は UI thread を塞がない bounded task とし、workspace と request generation を completion 時に再検証する。source 一覧の置換は全件検証後だけ公開し、移動先の read/decode failure、cancel、stale completion は直前に正常表示できた画像とその view state を保つ。外部 path、file bytes、decoded raster、sample は document history、journal、savepoint、native file へ永続化しない。
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
- drag で指定する範囲は button release まで文書を変更せず Canvas 上へ preview し、Cancel または tool／view 切替で消去する。
- `透明部分のみ` と含み塗りを組み合わせられる。
- 細い毛先や1 pixel領域も、通常 fill の seed click を何度も要求せず処理する。

#### 色置換・塗りのばし・組み線

- 色置換 tool は pen、rectangle、polyline、lasso の document-space region と既存 selection の積集合だけで、対象色を描画色へ変更する。region がなく selection だけがある場合は selection、両方がない場合は文書全体を対象とし、文書全体の実行は Windows frontend が確定 request の発行前に明示確認する。region preview は button release まで文書を変更せず、Cancel または tool／view 切替で消去する。
- raster 色置換は対象 plane の native depth で alpha を含む全格納成分が対象色と完全一致する pixel だけを置換する。表示変換、premultiply 後の値、tolerance、連結性を判定へ使わず、selection／region 外の同色 pixel は変更しない。
- 彩色 mode は主線 plane を対象にできず、主線 mode の明示 command だけが editable な主線 pixel を変更できる。hidden または non-editable target は実行前に拒否する。
- 塗りのばしは既存色を drag 方向の狭い未着色領域へ広げ、効果範囲、強さ、drag で囲まれた範囲も処理する option を持つ。
- 組み線彩色では light table の線を境界として参照し、参照画像自体を変更しない。
- 合成動画では親セルの必要 layer/plane を typed clipboard で子セルへ座標維持 paste できる。

### 12. 彩色チェックとモーションチェック

- `彩色チェック表示` は legacy white-transparency mode で完全な白 RGB(255,255,255) を未彩色/透明候補として残し、それ以外を黒等の高 contrast で表示する。native alpha mode では透明 alpha も別 category で示す。
- `出力色安全ガード`は正式な放送規格適合判定ではなく、BT.709のY′CbCr係数とnominal code相当の閾値を使うinkpod独自の保守的QAとする。closed profileの初期値は`BT.709 conservative Y′CbCr guard`だけとし、旧NTSC固定式、EBU R103適合表示、自動legalizeを行わない。
- sourceは確定済みのvisible layer compositeとGenesis assetであり、solid-white paper、Light Table、guide、grid、selection overlay、color-check overlay、previewを含めない。raster／adjustment、visibility、opacity、layer／plane順を通常のdocument compositeと共有する。8 bit channelは`value * 257`で16 bitへ正確に昇格し、RGBA16 straight alphaで合成する。alphaが0のpixelは検査せず、alphaが正のpixelはpremultiplied表示値ではなく合成後のstraight RGBを検査する。
- 16 bit compositeの`R,G,B`を0以上65535以下とし、`Y_num = 2126*R + 7152*G + 722*B`、`Y′ = round_half_up(Y_num / 10000)`とする。`Cb = round_half_up((65535*18556 + 2*(10000*B - Y_num)) / (2*18556))`、`Cr = round_half_up((65535*15748 + 2*(10000*R - Y_num)) / (2*15748))`とし、検査付き整数演算だけを使う。Y′の安全域は8 bit code相当`16..=235`、Cb／Crは`16..=240`、16 bitでは各境界を257倍した値とする。境界値は安全で、一成分でも範囲外ならそのpixelを規格外候補にする。spatial filterと画像全体1% thresholdは適用せず、pixel単位の候補選択と件数／検査数／透明skip数を返す。
- ガード結果は元pixelを変更せず、`新規`、`追加`、`削除`、`交差`で既存selectionへ一transaction、一canonical procedure、一Undo単位として合成する。`新規`が非空selectionを空maskへ置換する場合は変更、既に同じ空maskの場合だけno-opとする。他operationの同一結果もno-opとし、Cancel、invalid profile、stale base revision、overflow、allocation／composition failureではselection、revision、history、journal、dirty、IDを進めない。
- 大画像scanはrow単位のprogressとcooperative cancellationを持ち、発行時document UUID／base revision／profile／selection operationへ固定する。profile semanticsはcanonical procedureへ保存するが、profileのUI既定値はapplication settingでありdocumentへ永続化しない。
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
- 前後N枚登録は現在セルを除き、自然順の前、後、または両方向から各方向最大N枚を対象にする。距離を現在セルから数えた1始まりの `d`、入力した不透明度を `base`、距離stepを `step` とすると、item opacityは `max(0, base - step * (d - 1))` milliとする。`base` と `step` は0以上1000以下で、検査付き整数演算だけを使う。N=0または対象がない場合はno-opとし、端点、欠番、不足枚数では存在するセルだけを登録する。
- 一括登録するitemは自然順で若いセルを下、後のセルを上に積み、既存itemの相対順を変えずに一括登録blockをsetの最上位へ置く。Light Tableのitem index 0は最上位であるため、一括登録block内は自然順の降順で格納する。
- 対象set内に同じsource document UUIDのitemが一つ以上存在する場合は、source generation/revisionにかかわらずそのセルをskipし、既存itemのstable ID、source、transform、opacity、color mode、visibility、名前、順序を変更しない。source更新は明示的な`更新`操作だけが行う。全対象がcurrent cell、範囲外、または既存sourceとしてskipされた場合はno-opとする。
- 前後N枚登録のpreviewは、発行時のsequence identity、active cell、対象set ID、document revisionに固定し、追加／skip件数、最終の上から下の順序、各source UUID／generation、距離、opacityを表示する。previewは文書を変更せず、Applyは全追加を一つのcanonical procedure、一revision、一history entryとして確定する。Cancel、invalid、stale、overflow、allocation/source failureではitem、ID、history、journal、dirty、snapshotを一つも進めない。
- `編集画像と入れ替え` または item double click は、現在編集 image と選択 item を入れ替える。dirty 保存確認を通し、参照側の transform/opacity 情報を壊さない。
- light table 全体で重なりを透けさせる option と、前後画像登録時の自動 opacity step を持てる。

### 14. 選択範囲

- selection は document 寸法の mask として保持し、処理効果をその mask 内へ限定する。
- tool は rectangle/ellipse、magic wand、lasso、polyline、trace brush。
- operation は `新規`、`追加`、`削除`、`交差`。modifier は Shift=追加、Alt=削除、Shift+Alt=交差を基本とする。
- selection 内を drag したとき、mask だけを移動するか、選択された active plane pixel も floating content として移動するかを option で分ける。
- rectangle/ellipse は aspect ratio、中心から作成、作成後回転、45度 constraint を持つ。
- magic wand は connected same-color、color tolerance、gap close を持つ。階調主線では基本色と coverage semantics を使う。
- trace brush は丸/角、太さ、pressure、screen-size固定を持つ。
- 範囲解釈は通常、描線に密着する shrink、閉じた内部、描線形状、必要に応じた境界選択を区別する。
- raster 内容の coverage は Binary／Grayscale 8/16 bit の非ゼロ値、RGBA 8/16 bit の非ゼロ alpha とする。探索は candidate 内の 4 近傍で行い、`描線に密着` は candidate 外周へ到達する未 coverage を除いた coverage と穴、`閉領域内部` は外周へ到達しない未 coverage、`描線形状` は coverage、`境界` は未 coverage または用紙外へ 4 近傍で接する coverage とする。通常は raster 内容を読まず candidate をそのまま使う。
- rectangle／ellipse の aspect は入力範囲を縮めず不足軸を拡張し、中心指定時は開始点を中心とする。回転値は一周を `u32` 全域で表し、45 度 constraint は最寄りの 1/8 周へ丸める。trace の screen-size 固定は gesture 開始時の view zoom で document 径へ正規化し、pressure は各 sample の径へ適用する。
- geometry preview と commit は同じ正規化済み option と mask generator を使う。Cancel、invalid、stale、overflow は mask、revision、履歴、journal を変えない。`新規` が非空 mask を空へ置換する場合は一変更、既に同じ空 mask なら no-op とする。
- 描画色と同じ/異なる領域の全選択、追加、mask expand/shrink を提供する。
- selection layer との相互変換、現在 mask への追加/削除、selection layer 自体を通常描画 tool で編集する操作を round-trip 可能にする。

### 15. カット、コピー、ペースト

- clipboard payload は source document ID、layer/plane type、document origin に対する bounds、pixel/selection、色深度を持つ。
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
- 全文書の左右/上下 mirror、90度回転はすべての画像 plane、selection、frame、guide の座標整合を保つ。
- 部分 transform は selection content を floating state にし、X/Y移動、幅/高さscale、aspect lock、五点基準、任意角回転を dialog と handle drag の双方で操作する。
- floating transform の元領域は document 座標の half-open 矩形 `[x, x + width) × [y, y + height)` とする。五点基準は左上 `(x, y)`、右上 `(x + width, y)`、中央 `(x + width/2, y + height/2)`、左下 `(x, y + height)`、右下 `(x + width, y + height)` であり、中央の半pixelは Q16.16 で正確に保持する。
- floating transform dialog の X/Y は移動量ではなく、選択した五点基準を配置する絶対 document 座標である。変換は元の同じ floating content に対し、選択anchorをpivotとしてlocal X/Y scale、時計回りrotation、anchorをX/Yへ配置する順で一回だけ評価する。preview update、anchor変更、retry、dialog、Canvas handleは前回preview結果へ累積適用せず、この同じ変換を使う。
- raster は変換後のhalf-open edge boundsをQ16.16で求め、外接destination pixel範囲を下端floor／上端ceilで決める。各destination pixel中心を逆写像し、source half-open cellをfloorで一意に選ぶ。用紙外はclipし、far edge、負座標、half-pixel、非一様scale、任意角回転でもOS DPI、renderer、thread数に依存しない。Canvas preview／handleも同じanchorと変換順序を使う。
- selection 内に描画内容がなければ明確な no-content error とし、履歴を増やさない。

### 17. 履歴、復帰、preview

- undo/redo は直前 command と複数段階移動を扱う。Undo または history jump 後の新規編集では旧 tail を通常 Redo の対象から外すが、procedure と asset は非 active journal branch として保持し、自動削除しない。
- `ツール > Inkpodファイルの可視化` は、process 内で現在開かれており native `.inkpod` の保存先を持つ document session を重複なく列挙する。項目を選ぶと document ごとに一つのモードレス dialog を開き、dialog を開いた時点の in-memory journal の `Commit` record を `JournalEventId` 順に、primitive 名、決定的な `field=value` 引数表現、その commit 後の可視 composite の最大 64×64 straight-alpha RGBA8 thumbnail の三列で表示する。`HistoryMove`、`BranchCut`、Genesis は行にせず、通常 Redo から外れた branch の commit も表示する。巨大な可変長引数は件数、byte 長、digest へ要約し、表示 query は document、revision、history、dirty、savepoint、persistent ID を変更しない。dialog は一つの scrollbar 付き list control だけを content とし、session が閉じられた場合は対応する dialog も閉じる。履歴の再構築は dialog を開いた時点の不変な入力を使い、進捗と cooperative cancel を備え、Core engine queue の末尾へ bounded step ごとに再投入する。UI/Input thread はその完了や Core query を同期的に待たず、共通 Job Progress と list 内の読み込み行を更新する。thumbnail は full-canvas composite を中間生成せず最大 64×64 の出力へ直接合成し、完成後の行データは owner-data list の可視範囲を小さな batch で caller-owned cache へコピーする。
- pointer down から up までの stroke、shape 確定、fill、filter apply、layer operation、paste commit をそれぞれ一 command とする。
- dialog preview は base state から毎回再計算し、parameter slider を動かすたびに結果へ累積適用しない。
- `実行`/`OK` は一回の commit、`キャンセル` は base state へ完全復元する。
- filter／色調補正 dialog は有効な parameter、channel、補間、curve point の変更を
  120ms以内の短いdebounce後にCore previewへ送り、同じ発行時document session、view、
  stable Plane ID、generation、base revisionを最後まで保持する。一つの実行中taskと
  一つのpending parameter setだけを持ち、新しい変更はpendingを置換して実行中taskへ
  cooperative cancelを要求する。古いcompletionを別targetへ適用しない。
- filter previewの失敗／cancelled updateは直前に成功したpreviewを保持するが、最新の
  parameterがinvalid／failureのまま`OK`された場合は古いpreviewを確定しない。tab／target
  delete、document close、stale generationでは安全にCancelする。dialogと共有Job Progressは
  計算中parameter、progress、failureを表示し、UI threadはCore workやPresentを待たない。
- `復帰` は最後の通常保存を staged Core で再構成して置換する。`部分復帰` は保存済み journal state から対象を再構成し、成功時だけ一件の新しい Undo 可能な canonical procedure として commit する。

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

- 一つの batch set は削除・無効化・並べ替え不能な一つの `入力`、一個以上の順序付き処理、削除・無効化・並べ替え不能な一つの `出力` から成る。入力と出力は `BatchOperationKind` に混在させず、全処理が無効な graph は実行不能とする。処理は enabled、複製、削除、上下移動を持つ。
- 公開 authoring catalog、`.inkbatch`、C ABI、Windows UI が扱う処理は `色置換`、`彩色プレーンへ送る`、`マスキング`、`消去` の四種類だけとする。既存 filter、continuous fill、visibility、resize 等の基礎 Core 機能は削除しないが、Batch v4 から到達可能にしない。
- 入力 node は、複数 file、非再帰 folder、job 発行時 active document の三種類の入力元を複数内包できる。file/folder は `.inkpod`、PNG、TIFF、TGA、BMP だけを受理し、folder は対応 file だけを自然順に列挙する。重複、missing、未対応形式、range、解決件数を preview する。active document は発行時の `DocumentSession` ID と generation に固定し、実行時に別の active document へ再解決しない。
- 出力 node は folder、発行時に固定した active document、新規 tab を選べる。folder format は `.inkpod`、PNG、TIFF、TGA、BMP とし、一件ごとに同一 volume の temporary file を完成してから atomic replace する。active document への適用は結果全体を一つの Undo 単位として dirty にし、path authority と savepoint を進めない。stale generation では何も適用しない。新規 tab は各結果に Rust が新しい document identity を割り当て、pathless/dirty な `DocumentSession` とする。session/tab 上限超過は job 開始前に拒否する。
- folder の命名は bounded template とし、初期 token は `{stem}` と `{index:N}` だけを許可する。拡張子は output format から決め、absolute path、separator、`..`、拡張子 token を拒否する。Core の dry-run は全出力 path、graph 内重複、既存 file 衝突を返し、一切書き込まないが、Windows product UI には独立した dry-run command を置かない。
- native 色一致は対象 plane の格納 depth で判定する。RGBA は straight alpha を含む全成分の完全一致、Binary/Grayscale は格納 scalar の完全一致とし、表示変換、premultiply 後の値、tolerance、連結性、暗黙 depth/format 変換を使わない。不一致は item 単位の preview validation error とする。
- color replace は `旧色 -> 新色` の bounded 複数行、行ごとの enable、追加、削除、全行反転を持つ。同じ旧色を持つ enabled 行の重複を拒否し、一致 pixel がなければ revision、history、journal、dirty を進めない。既存の exact color pair 抽出機能は維持する。一つの color replace operation は bounded かつ非空の target selector 集合を持ち、各semantic selectorに一致する全layerの対象planeをstable IDへ決定的に展開し、重複planeを一回だけ処理する。全targetは一つの `ApplyBatchOperations` canonical primitive、一回のtransaction、一つのUndo単位として適用し、missing/error、形式不一致、hidden、non-editable、cancel、overflowではどのtargetもcommitしない。
- 二枚の同位置セルから color pair を抽出する場合は、Core が所有する非zero document UUIDと非zero source generationの組で各immutable raster sourceを固定する。両sourceは同じ幅、高さ、native pixel formatを必要とし、異なる寸法、形式、stale／missing identityを変換、resample、現在activeな別cellへの再解決なしに拒否する。比較は同じdocument X/Yの格納値をnative depthで行い、RGBA 8/16 bitではstraight alphaを含む全成分、Binary／Grayscale 8/16 bitでは格納scalarの完全一致を使う。表示変換、premultiply後の値、toleranceを使わない。
- 同じ格納値のpixelは置換pairへ出さず、unchanged件数としてpreviewする。RGBが同じでもalphaが異なれば差分候補とする。各`旧色 -> 新色`候補はpixel件数と、その候補が現れたhalf-open document boundsを持つ。旧色groupは最初の差分pixelのscanline順、同じ旧色内の候補はpixel件数の降順、同数なら新色のnative値順で決定的に並べる。
- 一つの旧色が複数の新色へ対応するone-to-manyは未解決ambiguityとし、最多候補を自動採用しない。利用者がその旧色について一候補だけを選ぶか旧色group全体を除外するまで、graph作成と実行を拒否する。複数の異なる旧色が同じ新色へ対応するmany-to-oneは、それぞれがone-to-oneなら有効な複数pairとして許可する。previewは候補、件数、bounds、alphaを表示する。
- `彩色プレーンへ送る` は、指定色に一致した source pixel の native 値を同じ layer の彩色 plane の同一座標へ移す。destination の非対象座標は保持し、書いた座標の source は source format の empty 値へする。両 plane の format/dimensions は完全一致を必須とし、missing、hidden、non-editable、主線保護、stale revision は source/destination のどちらも変更しない。一つの Core transaction、一つの canonical procedure、一つの Undo 単位とする。
- `マスキング` は selection を流用せず、document 専用の sparse `fill protection mask` を置換する。指定色と一致する座標だけを `255 = 塗りの壁` として保持し、source raster は変更しない。壁 tile だけを割り当てる。mask は全 Core fill 経路の hard boundary、Undo/Redo、branch、replay、save/reopen、snapshot revision、cache invalidation の一部とする。追加/削除合成は初期 scope 外とする。
- `消去` は指定色と一致する source pixel だけを native empty 値へする。RGBA は transparent black、Binary/Grayscale は 0 とし、非対象 pixel は保持する。一致なしは no-op とする。
- マスキングを含む graph は情報を保持できない PNG/TIFF/TGA/BMP folder 出力を拒否し、`.inkpod`、active document、新規 tab だけを許可する。
- Batch pane は `%APPDATA%\inkpod\batch-sets` の `.inkbatch` file 名を列挙する編集可能な set 名 dropdown と保存/読込、Input/処理/Output を一列に置く headerless 工程 List-View、追加/複製/削除/上下移動、選択項目別の scrollable parameter host、validation、最下段に `プレビュー`、`全実行`、`中止` の三 button だけを持つ。入力行は `入力 (N件)` と表示し、処理の enable は工程 List-View 内の標準 checkbox で編集して固定 Input/Output には checkbox を表示しない。処理行の checkbox はその領域のclickまたは選択行のSpaceで切り替え、行選択だけでは切り替えない。読込時の decoder は current v4 だけを受理する。set 名は前後の空白を正規化し、path separator、Windows 予約名、末尾 dot を拒否し、dropdown には拡張子を除いた名前を表示する。Batch pane 自体には document follow/pin 表示や pin 操作を置かず、job 発行時 target 固定は command context で行う。`＋処理` は上記四候補だけの localizable popup とし、parameter は標準 Common Controls で inline 編集する。色置換parameterは汎用raster、2値彩色、階調彩色のtarget layer種別を標準checkboxで一つ以上複数選択でき、読込済みfixed-ID selectorは利用者がsemantic checkboxへ切り替えるまで保持する。入力 file/folder と folder 出力は parameter host 内の参照 button から Windows file/folder picker へ接続し、file picker は対応形式だけを列挙して複数選択を取り込む。parameter page は非表示 control の空間を残さない page 別の自然高さで配置し、動的 control も pane と同じ GUI font を使う。色 table は常に client 幅を使い切り、単一の選択行だけを選択表示し、alpha を反映した swatch と depth/RGBA 数値を重ならない領域へ表示する。選択行では旧色と新色のalphaをnative depthの範囲（8 bitは0–255、16 bitは0–65535）で数値編集できる。`描画色から取得` は色置換では旧色／新色の選択menuを開き、その他の処理ではその処理色へ適用し、適用後の値をpane内で常時確認可能にする。validation／実行結果欄は選択・コピー・縦scroll可能な read-only multiline control とし、実行失敗時は総件数だけでなく入力名と item 固有の理由を表示する。既知の対象欠落、hidden／non-editable、pixel format 不一致は日本語／英語へlocalizeし、未知のCore診断はboundedな技術詳細として表示する。一度に表示する失敗は先頭8件までとし、残件数を示す。
- `プレビュー` は Core engine thread 上の非同期 job とし、4 GiB 以下の専用 temporary job directory を作る。file/folder input は encoded file を chunk copy し、active document input は issue-time の immutable document/assets を temporary `.inkpod` へ materialize する。全 input の copy/materialize が完了するまで最初の処理を開始せず、copy 後の原 file の変更を処理入力へ反映しない。各 copy に全 enabled 処理を順に適用し、folder output 設定では同じ output format、それ以外では temporary `.inkpod` へ保存して再読込する。設定された実 output folder、active document、新規 tab へは書き込まない。
- preview 結果は input 順の決定的なほぼ正方格子へ並べた一枚の straight-alpha RGBA8 contact sheet とする。thumbnail の長辺上限は 160 pixel、padding は8 pixelを基準とし、全 contact sheet が16,777,216 pixel以下になるよう縮小する。透明部分には checkerboard、item failure には赤系 placeholder、Stop 後の未処理 item には灰色 placeholder を表示する。成功時は一つの clean/pathless Rust-owned staged Core として新しい Canvas tabへ `バッチプレビュー`／`Batch Preview` の名前で公開し、専用 temporary job directory は tab 公開前に削除する。この tab は表示専用の Batch preview とし、active document input の source として再解決しない。preview tab が active のまま後続の `プレビュー` または `全実行` を発行した場合は、その preview が保持する元の issue-time document/view context を target とし、元 target が stale なら active preview へ fallback せず拒否する。cancel、stale target、cleanup failure では tabを公開せず、元 document、実 output、revision、history、dirty、savepointを変更しない。
- Batch pane は右側 tool tab を開くたびに Batch 専用 tab へ配置する。この tab は Batch pane 一つだけを持ち、Batch を既存 tab へ追加することも、他 pane を Batch tab へ追加することも、drag／復元／workspace decode を含む全経路で許可しない。
- UI は選択変更で document/immutable graph を変更せず draft view model を編集し、preview/run/save 時だけ検証済み immutable graph を一回構築する。読込済み `.inkbatch` v4 は input/operation/outputと全target selectorを draft へ完全復元して編集可能にする。狭い pane は縦 scroll と responsive button wrap を使い、日本語/英語、96/120/144/192 DPI、high contrast、Tab/F6、screen reader name を扱う。
- 一件ごとに temporary output から atomic commit し、cancel/失敗した item に部分 output を残さない。dry-run は一切書かない。

将来のBatch authoring／execution形式であるInkScriptのlanguage core、schema registry、exact-source／rebound
等価性、実装gateは[`INKSCRIPT.md`](INKSCRIPT.md)を規範とする。exact-current `.inkbatch` v4 と Batch v4
UI／ABIを production contract とする。M23で批准済みcatalogを使うRust compile／bind／staged-run APIはproductから独立して
公開してよいが、`.inkscript` file filter、clipboard、C ABI、Windows command／UI、Batch production executorからは
各owner milestoneとM34 cutoverまで到達可能にしない。

### 20. 形式、白透過、一般画像入出力

- exact-current 契約は `.inkpod` top-level format v28、runtime replay epoch 25、C ABI v22、`.inkbatch` v4、InkScript registry schema／language／file v2、production catalog／owner manifest v4 とする。native v27／epoch 24、ABI v21以前、`.inkbatch` v3以前、catalog／owner manifest v3、および削除済み Batch authoring operation は migration や shim を設けず拒否する。今回の更新は native format freeze 宣言ではない。
- native `.inkpod` は、保存時点の可変 raster snapshot を意味上の正本にしない。正本は immutable な `Genesis`、content-addressed な `Assets`、Core が検証・正規化して実変更を確定した `Procedures` と history control event、history の現在位置と high-watermark を持つ `META`、文書単位の `EditorState` とする。materialized document、inverse delta、COW snapshot、render/checkpoint cache は派生物であり、これらだけで文書を成立させない。
- frontend request は target/revision/ID と上限を検証し、座標、色、option、可変長入力、transaction 内の output ID を正規化してから一つの `CanonicalProcedure` として確定する。procedure は monotonic ID、primitive ID/schema、replay epoch、base/committed `StateId`、固定幅引数、stable input/output ID、immutable `AssetId` または bounded inline payload、pre/post document-state digest を持ち、raw pointer、外部 path、native enum layout、frontend command ID、一時 object ID を含めない。
- `Genesis` は document UUID、paper、DPI、sRGB、frame、margin、初期 stable-ID topology、immutable base surface を完全記述する。白紙の base surface は全面 tile を割り当てない opaque white の `SolidWhite` underlay とし、flat canonical composite/export には参加するが、個別 layer/plane export や selection mask へ暗黙に混入させない。
- import、clipboard、Light Table 等の外部入力は ingestion 時に Rust が canonical pixel payload へ変換し、immutable `AssetId` を発行する。procedure は外部 path、codec の再実行、caller buffer の lifetime を参照しない。元 encoded bytes や provenance は replay に影響しない任意 metadata としてのみ保持できる。
- 永続 journal は閉じた型 `Commit`、`HistoryMove`、`BranchCut` だけを持つ。実変更を確定した document transaction、実際に移動した Undo/Redo/history jump、history cursor が active branch の tail 以外にある状態からの新規 commit による branch cut だけを順序どおり記録し、query、invalid、failure、cancel、stale、overflow、no-op、stroke/preview の途中更新は記録しない。stroke end、preview apply、floating commit は成功時にそれぞれ一つの canonical procedure とする。
- `.inkpod` section は history procedure/control event を `PROC`、history cursor、active branch、document/editor savepoint と各 persistent ID の high-watermark を `META` に置く。独立した `HIST` section は作らない。`EDIT` は active tool、最後の色付き command、tool ごとの exact-depth color、diameter、fill/selection option、active layer/plane、palette cursor 等の再開に必要な文書単位 editor state を保持する。`CKPT` は任意の open 高速化 cache、`EXTM` は replay に影響しない任意 metadata とする。checkpoint の hash、構造、resource bound 違反は file corruption として拒否し、構造上有効な epoch/prefix/state 不一致だけは checkpoint を無視して full replay する。checkpoint を全て除いても同じ state、pixel、history、次 ID を再構成できなければならない。
- 通常保存後の reopen は画像だけでなく、history list/cursor、Undo/Redo availability、active/non-active branch、document/editor savepoint、persistent ID high-watermark、EditorState を復元する。通常 UI から外れた redo branch も監査可能な append-only journal と asset retention root に残し、自動 squash しない。
- open は decode、全参照・asset 検証、replay を staged Core で完了し、成功時だけ live Core を一回で置換する。通常保存は current `StateId` と `EditorStateDigest` を prospective savepoint として一時 file へ書き、flush、close、destination 置換の成功後だけ live path と両 savepoint を公開する。autosave、recovery、export は通常 savepoint を進めず、recovery open は以前の通常保存先への authority を継承しない pathless かつ dirty な session とする。
- 履歴を失う compaction は自動実行しない。利用者へ失われる event/procedure 数を事前表示し、revision と digest で対象を再確認したうえで、open session の path とは別の file へ新しい Genesis として書き出す。成功しても live path、history、dirty、savepoint を変更しない。
- persistent `StateId` は Genesis と commit 済み意味状態を参照し、procedure の precondition、history、savepoint に使う。`DocumentRevision` は stale request 検出用の session-local counter であり file へ保存せず、open 時に新しい Core generation 内で rebase する。EditorState は document history と別の persisted editor revision/digest/savepoint を持ち、session dirty は document state または editor state のいずれかが各 savepoint と異なれば成立する。
- 同じ replay epoch、Genesis、Assets、canonical procedure/control-event 列から、x64、ARM64、非 Windows Rust target で同じ canonical Core state と bit-exact な canonical composite を得る。Direct2D/D3D の画面 antialiasing や monitor 表示の一致はこの契約に含めない。primitive semantics が replay 結果を変える場合は replay epoch と top-level format version を更新する。
- ユーザーがフォーマットフリーズを宣言するまで、`.inkpod`、`.inkbatch`、native preset等のapplication固有の永続化ファイル形式は現在versionだけを読み書きし、下位互換reader/writer、migration、互換shimを持たない。現在の要件に対して最も頑健で効率的なschemaを選ぶ。この規則はHKCUのworkspace layout recordには適用しない。
- コードフリーズまでは、serialized schemaを変更するたびに対象形式の最上位format versionを必ずインクリメントする。section/record versionだけの変更で代用せず、旧versionは明示的に拒否する。
- 一般 raster import/export は少なくとも PNG、TIFF、TGA、BMP の対応可能な 8/16 bit、alpha、DPI を扱う。形式が表せない情報はflatten/export optionで明示する。
- TGA は Truevision TGA 2.0 の標準 image type 0、1、2、3、9、10、11 を対象とし、color-mapped／true-color／black-and-white、非圧縮／RLE、4方向の画像原点、Image ID、Color Map、Footer、Extension Area、Developer Area を境界検査付きで読み書きする。true-color は16／24／32 bit、color-map index は8／16 bit、color-map entry は15／16／24／32 bit、black-and-white は8 bitを標準対応範囲とする。image type 128–255 のdeveloper-defined data、予約済みimage type／bit depth、`.vda`／`.icb`／`.vst`別名は対応形式に含めない。RLE writerはpacketをscanline境界で分割し、readerは既存資産互換のため境界越えpacketも画像全体の上限内に限って受理する。
- TGA import はcanonical straight-alpha RGBA8へ決定的に変換する。5-bit channelはbit replication、premultiplied alphaは整数roundingでstraightへ戻し、alpha attributeが未定義または無効ならopaqueとして扱う。Extension Areaのalpha attribute type、color-correction table、postage stamp、scan-line tableと未知developer fieldは型付きTGA metadataとして境界内で保持し、通常の画像importではpixel結果に必要な情報だけを適用する。pixel aspect ratioはDPIへ読み替えない。
- TGA exportの既定値は既存互換のtop-left／32-bit BGRA／非圧縮／旧形式footerなしを維持する。TGA固有APIではimage kind、depth、RLE、origin、X/Y origin、Image ID、TGA 2.0 footer／extension／developer metadataを明示できる。alphaまたは色精度を失う形式への変換、grayscale化、palette化は明示optionなしに暗黙実行しない。自動palette化は入力走査順を固定し、表現可能色数を超えた場合は失敗する。日時等をwall clockから自動挿入しない。
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
- no-op は document revision、`StateId`、history、journal、persistent ID、dirty、render content を変えない。invalid、cancel、stale revision、overflow、失敗は document、history、journal、ID、dirty、revision、確定 snapshot、通常出力 file に部分変更を残さない。
- 一つの document edit は一回の Undo で直前の意味状態へ戻り、一回の Redo で同じ結果へ進む。Undo 後の新規 edit は旧 redo tail を通常 UI の対象から外すが、非 active journal branch は保持する。
- view-only edit は document revision、`StateId`、history、journal、dirty を変えず、意味上の変更がある対象 view の revision だけを進める。document edit は必要な render cache invalidation を起こす。
- 通常 save の成功だけが通常 savepoint を進める。autosave、recovery save、export は通常 savepoint を進めない。
- stable ID は所属 document/session 内で重複せず、保存、Undo/Redo、snapshot を通して参照関係を維持する。生成 ID は commit 時だけ消費し、削除後も再利用せず、layer、plane、view 等の別 namespace を混同しない。
- 長時間処理は base revision、cancel、target generation を確定し、全計算と検証が成功した場合だけ結果を公開する。

## 横断的な性能契約

- 大画像は sparse tile、遅延割当、copy-on-write を基本とし、Undo、snapshot、Light Table、checkpoint のために画像全体を無条件に複製しない。snapshot は変更 tile だけを再合成し、pan、zoom、flip、viewport 変更では既存の合成済み tile と renderer resource を再利用する。
- `revision-max` を採用する理由は、procedure journal や semantic digest の強度を render-cache 検証 cost へ転嫁せず、view-only 操作を source raster byte 数から独立させるためである。完全な source fingerprint より、固定幅 scalar による高速で予測可能な cache hit 判定を優先する。採用経緯、代替案、測定根拠は `docs/architecture.md` と `docs/core-benchmark-baseline.md` を正本とする。
- render tile cache の canonical source identity は `revision-max` 方式とする。各 document tile 座標について、可視 layer 内の可視 plane の `tile_revision`、selection の `tile_revision`、Light Table の `source_revision` の数値最大値を一つの scalar として求める。cache 内の `source_revision` と一致すれば合成済み pixel buffer と renderer-facing tile revision を再利用し、不一致ならその座標だけを再合成して新しい tile revision を公開する。透明な合成結果は cache に保持せず、必要なら再合成してよい。
- cache hit 判定は上記の固定幅 revision scalar だけを読み、source pixel の取得・copy・走査、payload hash/digest、clone generation、削除 tombstone、epoch、negative cache を追加しない。検証 cost は source byte 数ではなく可視 source 数に比例させ、cache hit の zoom/pan snapshot が raster payload size に比例する work を行ってはならない。cache の `source_revision` は非公開の派生 bookkeeping であり、semantic equality、C ABI、document/procedure digest、永続化へ含めない。
- opacity、visibility、layer/plane order、main-line color、color-check mode 等、`revision-max` 式に含まれない render metadata の変更は、同じ commit 境界で whole-cache invalidation を行う。
- 数値最大値は衝突のない source 記述ではない。高い Light Table revision が後続の低い raster revision を mask する場合、同じ最大値を持つ source の一方を削除しても値が変わらない場合、独立 revision namespace が同値になる場合、表示 mode の異なる view が cache を共有する場合がある。また透明結果は negative cache を持たない。これらは `revision-max` を性能上の正本とする際の既知制約であり、暗黙に別方式へ変更しない。
- 性能回帰は wall-clock だけで判定しない。`pan_zoom_snapshot` は quick/full で 2,048/8,192 pair、`dirty_tile_rebuild` は同一 allocated tile への 1 pixel edit と snapshot rebuild を 32/128 回実行し、checksum、revision、tile reuse/rebuild、payload access を固定する。初回 compose では payload access が正に増え、同じ fixture の cache-hit zoom snapshot 128 回では増分 0 を必須とする。private native smoke は 1024 平方・256 allocated tile の 512 wheel event を各一回の Present まで、16 stroke/544 sample の multi-tile drawing を各一回の Present まで測り、sample、Present、queue、resource counter を固定する。
- wall-clock は同じ workload、profile、入力と一致する `docs/core-benchmark-baseline.md` の承認済み環境別 envelope を使い、warm-up 後 5 回以上の中央値で比較する。下限未満は処理省略を疑う診断値にだけ使い、意味ゲートが正常な高速化を拒否しない。上限超過は独立した 5 回以上の再測定でも中央値が上限を超えた場合だけ回帰とする。workload、harness、reference 環境、envelope、`revision-max` 式を変更する場合は、理由、環境、全 sample、意味 counter を記録し、ユーザーの明示承認を得る。envelope を測定結果に合わせて自動緩和しない。

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
- `PERF-001`: 「横断的な性能契約」に定める sparse/COW、変更 tile だけの再合成、canonical `revision-max` cache、payload 非走査、意味 counter、固定 workload、承認済み環境別 envelope を維持する
- `PKG-001`: Rust/Win32 の静的 CRT、x64/ARM64 self-contained MSIX、ならびに ZIP 直下へ `inkpod.exe`、`README.txt`、`LICENSE.txt`、`ThirdPartyNotices.txt` だけを収録する x64/ARM64 portable payload と package/dependency 検証
- `PORT-001`: Rust workspace の OS 非依存性と次 frontend の adapter gap

### Document and view

- `DOC-001`: CellDocument、用紙、DPI、100 frame、基準/作画/安全 frame、余白
- `CELL-001`: image/frame size、DPI、六種 frame、五点 anchor、初期 layer、8/16 bit、bounded 複数枚を同一 plan から all-or-none で作る新規 Cell workflow
- `DOC-002`: stable ID を持つ typed layer/plane tree
- `DOC-003`: create/duplicate/delete/reorder/show/edit/opacity/convert/merge
- `RENDER-001`: raster／adjustment の layer/plane 木順序、visibility、opacity、alpha を共有する Canvas/thumbnail/flatten 合成
- `SHOOTING-FRAME-001`: stable ID、center／size／binary-turn rotation／五点 anchor／表示・指示 export policy を持つ独立した角度付き撮影 frame object、preview/transaction、document transform、Canvas／指示 export／save／reopen contract
- `VANISHING-POINT-001`: Canvas内外の複数stable-ID消失点、bounded fixed-point放射線、exact color／opacity／visibility、preview／transaction、radial snap、document transform、Canvas overlay、save／reopen contract
- `VIEW-001`: zoom、box zoom、fit、1:1、pan、horizontal/vertical flip
- `VIEW-002`: ruler、guide/grid、snap、transparent view
- `SNAP-001`: view-targeted device/document座標変換、guide/grid優先順位、Ctrl一時解除を共有するproduction図形入力snap
- `VIEW-003`: color locator の座標/RGBA/selection sampling と magnified neighborhood 表示・編集
- `VIEW-004`: 複数文書 tab、同一文書 view、二分割 group、group/window 間の移動と複製
- `HIST-001`: transaction、Undo/Redo、savepoint、revert、preview cancel
- `HIST-002`: open native document の canonical procedure journal、typed 引数、commit 後 thumbnail のモードレス可視化

### Paint and color

- `PAINT-001`: pencil、brush、eraser、auto erase、pressure
- `PAINT-004`: brush の丸／角 footprint、決定的 smoothing、immutable native-depth 開始色限定
- `PAINT-002`: line/curve/shape/polyline と preview commit
- `PAINT-003`: gap connect、dust removal、line width correction
- `FILL-001`: connected seed fill、tolerance、selection
- `FILL-002`: 含み塗り、overflow abort、gap close、detached regions
- `FILL-003`: closed-region fill、transparent-only、fill extension
- `FILL-004`: sparse fill protection mask を全 fill 経路の hard boundary とし、Undo/Redo、replay、save/reopen、snapshot/cache revisionへ含める
- `COLOR-REPLACE-001`: pen／rectangle／polyline／lassoとselectionで限定したnative-depth raster置換
- `COLOR-001`: RGBA 8/16、RGB/HSV、eyedropper source
- `COLOR-002`: palette、chart、subpalette、color check
- `SUBPALETTE-001`: 外部 file/folder source、自然セル順、独立 viewport、常時 eyedropper、exact sample/register、icon/keyboard navigation、非同期 failure atomicity
- `COLOR-CHART-PREVIEW-001`: 同一base compositeからの非累積Color chart生成preview、頻度／差分summary、revision-bound Apply、exact-color名前継承、lock拒否、cursor継承、Cancel無変更、一回Undo／Redoとsave/reopen
- `COLOR-OUTPUT-QA-001`: BT.709係数とnominal code相当閾値を使う非適合表示の保守的Y′CbCr guard、visible straight-alpha composite、透明skip、fixed half-up、selection algebra、progress／Cancel／stale、Undo／Redo、replay、save/reopen

### Selection and editing

- `SEL-001`: rect/ellipse/lasso/polyline/trace/wand selection
- `SEL-002`: new/add/subtract/intersect/invert/expand/shrink/color selection
- `SEL-003`: selection layer conversion
- `SEL-004`: raster range interpretation と rectangle／ellipse／trace construction options
- `CLIP-001`: typed clipboard、standard clipboard、document coordinate preservation
- `XFORM-001`: destructive mirror/rotate/size/resolution と非破壊 view transform の分離
- `XFORM-002`: floating selection move/scale/rotate、preview/commit/cancel
- `XFORM-003`: half-open boundsの五点anchorをpivotとするscale→時計回りrotate→絶対document X/Y配置、非累積preview、dialog／Canvas handle／rasterの同一結果

### Animation workflow

- `LT-001`: light table set、per-item transform/color/opacity、global opacity
- `LT-002`: reference-frame alignment、boundary/color sampling、edit image swap
- `LT-003`: 自然順の前／後／両方向Nセルを線形opacity stepと時系列z-orderでpreviewし、同一source UUIDの既存itemを保持したまま一つのUndo単位で一括登録する
- `CUT-001`: stable CutId、metadata、Cell作成既定値、ordered Cell membershipを、同一directoryの個別Cell `.inkpod`へのbounded相対参照として保持し、独立history／savepoint／recovery、default明示copy、staged identity検証、Cut Properties／Undo／Redo／save／reopenを提供する
- `SEQ-001`: cut/cell sequence、自然順の前後セル、欠番、thumbnail preview
- `SEQ-ENDPOINT-001`: application-wideの`Stop`／`Wrap`端点policy、empty／one／stopped／advanced／wrapped result、issue-time cell identity、motion loopとの分離、versioned HKCU persistence
- `SEQ-STRUCT-001`: Cut membership の add／remove／move-before／move-after／range renumber を stable Cell identity の一 transaction として行い、表示順／番号を file 名から分離し、Cut 専用 Undo／Redo、save／reopen、orphan 状態を提供する
- `SEQ-002`: motion check、FPS、loop、step、selection/light table option
- `PREF-001`: application／workspace 環境設定を集約するタブ式 dialog、候補 state、Apply／OK／Cancel 原子性、scope と再起動要否の表示
- `SHORT-001`: 完全な組み込み preset と未割当可能なユーザー preset、主／副の最大4-stroke shortcut、Global／Canvas／Timeline／Pane context、Execute／Hold／Toggle action、論理／物理照合、text-focus guard、context-aware prefix-free resolve、競合解決、検索／分類／keyboard 可視化、`.inkshortcuts` current-version import/export、永続化、reset

### Image processing and batch

- `FILTER-001`: sharpen/blur/Gaussian/invert/auto contrast
- `FILTER-002`: brightness/contrast、curve、levels、HSV、color balance
- `FILTER-PREVIEW-001`: filter／色調補正dialogの非累積live preview、bounded latest-wins更新、発行時target固定、OK一commit／Cancel完全復元
- `EFFECT-001`: gradient、airbrush、airbrush boundary effect、blur tool、stamp
- `ADJUST-001`: non-destructive adjustment layer と alpha edit
- `BATCH-001`: fixed Input -> one-or-more ordered Operations -> fixed Output graph、draft編集、複数target color replace、`.inkbatch` v4
- `BATCH-002`: 公開四処理（exact color replace、move-to-color-plane、fill-protection masking、erase）とnative-depth一致
- `BATCH-003`: file/folder/issue-time-active input、folder/issue-time-active/new-tab output、bounded naming dry-run、progress/cancel、per-output atomicity
- `BATCH-004`: exact native-depth二セルpair抽出、複数行色置換、mask-aware output validation、staged result ownership、inline parameter hostとloaded-set編集
- `SCRIPT-001`: exact-current UTF-8 `.inkscript`／fragmentのclosed grammar、lossless CST、typed semantic AST、canonical emitter、schema registry、bounded diagnostic／resource contract
- `SCRIPT-002`: 全現行journal-replayable primitiveのclosed typed catalog、同一canonical executor、exact-source／rebound等価性、selector／assert／result／asset／portability／work formula、Continuous Fillの一seed一stepと1:N `editor_group`
- `SCRIPT-003`: authority-bound immutable plan、dry-run／progress／cancel／failure report、inputごとのstaged executionとexact-current `.inkpod` atomic install、save/reopen／Undo/Redo／cache-free replay／ID／savepoint保持
- `SCRIPT-004`: journalからのexact fragment export、dependency closure、strict bindingの明示rebind、Batch／History間のtransactional clipboard、source-preserving structured edit
- `SCRIPT-005`: `.inkbatch`現行productionを維持したprivate実装、M29C shadow parity、M34明示cutover、M35旧形式削除、承認済みperformance gateと最終hardening
