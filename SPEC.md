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

- `セル`: 一枚の作画・彩色文書。用紙、frame、複数 layer、選択、補助情報を持つ。
- `レイヤー`: セル画を重ねる単一種類の画像単位。主線 plane 一枚と彩色 plane 一枚を必ず持ち、任意個の raster plane を追加できる。
- `プレーン`: layer 内の最小画像編集単位。意味上の役割は主線、彩色、raster の三種類とし、格納表現は独立した `PixelFormat` で表す。
- `選択状態`: layer/plane 木とは独立した文書所有の現在選択 mask、保存選択 mask 群、および fill protection mask。通常の画像合成へ参加しない。
- `主線保護`: 彩色 mode では主線を合成表示するが、fill や brush が主線 plane を変更しない性質。
- `基準フレーム`: 紙のタップ穴に相当する位置合わせ基準。異寸法セルや light table を重ねるときは画像左上ではなくこの基準で揃える。
- `100フレーム`: 制作上の基準となる作画 frame サイズ。物理寸法、DPI、pixel 寸法を組にして保持し、50/200 frame 等はこの基準に対する比率で表す。
- `余白`: 作画 frame の外側だが文書内に存在する領域。camera work やはみ出し作画のため保持する。
- `安全フレーム`: 必ず画面内へ収めたい内容の目安。作画 frame とは別の overlay であり画像へ焼き込まない。
- `ライトテーブル`: 編集セルの背後へ参照セルを半透明で重ねる read-only 機能。編集画像と明示的に入れ替えた場合だけ編集対象が変わる。
- `合理的互換性`: 操作の意味、データ分離、座標、結果を再現すること。旧画面の配置、配色、アイコン、文言、制限を不必要に模写することではない。

#### 1.1 単独セルと通常の連番

- 制作・保存の単位は独立した CellDocument とし、各セルが stable CellId、document UUID、文書履歴、dirty、savepoint、recovery を所有する。
- Sequence はファイルの自然順カタログとセル切り替えを扱う。通常のファイル追加、自動連番検出、thumbnail、前後移動、個別セルの保存／復元を維持する。
- Cut の metadata、Cell 作成既定値、永続 membership、独自の表示順／表示番号、専用 Undo／Redo、descriptor、CutSession／CutCore は設けない。Cut 専用の追加／除外／並べ替え／採番、drag reorder とその shortcut も提供しない。
- 旧 Cut descriptor は未対応入力として現在のセルを置換せず拒否する。既存ファイルの削除・変換・探索修復は行わない。

### 2. Windows GUI の全体構成

Windows GUI は標準的な Windows 11 desktop application とし、古典的 MDI
や別 GUI framework へ移行せず、次の構造を持たせてください。

- `WIN-001` の通常 UI は Windows のアプリ配色へ自動追従せず、タイトルバー以外は既定のライト配色（Win32 標準の描画色）を使用する。標準 control の visual style は Windows に任せ、owner-draw UI も `GetSysColor`／`GetSysColorBrush` の背景・文字・選択色の組を使う。ハイコントラストの system color を固定 RGB で上書きしない。
- タイトルバーだけは公開 DWM API による OS 標準のダーク表示を許可する。独自の client-area ダーク palette、アプリ配色監視、標準 control の visual-style opt-out は設けない。第三者のダークモード library、非公開 Win32 ordinal／API／theme class は使わない。
- Canvas の画像・余白・透明 checker・overlay、ライトテーブルの参照画像・opacity・合成、サブパレットの参照画像・透明 checker・採色結果、および色見本・thumbnail の画像値は UI テーマに依存させず、従来の表示と内容を維持する。テーマ変更で文書／view revision、履歴、dirty、色値、選択、focus、scroll position、pane contents を変更・再生成せず、テーマ設定を文書や workspace 保存へ追加しない。

- process には一つの `ApplicationHost` を置き、同一 UI/Input thread 上で複数の `WorkspaceWindow` を所有できるようにする。各 window は独立した menu bar、制約付き dock、editor area、status bar、focus history を持つ。`WM_QUIT` は最後の workspace window が閉じたときだけ発行する。
- 独立した常設 toolbar は置かない。利用者が実行できる全機能を menu bar の末端項目から呼び出せることを優先する。選択中 tool の option は、左 tool button の独立した展開領域から開く縦型の owned flyout に表示し、同じ command/state と既存の option callback を使う。flyout は標準 caption、window icon、resize frame を持たず、30 DIP の compact header に accessible な pin toggle と close button だけを置く。非 pin 時は owner workspace または別 application へ pointer/focus が移った後に、flyout 自身と combo 等の owned popup を除外して自動的に閉じる。pin は workspace session 内だけの状態とし、自動 close を抑止するが system-wide topmost にはしない。flyout の高さは可視 control の末尾と下余白から算出し、monitor work area を超える場合だけ内部を縦 scroll する。配置は button の右側を優先し、収まらない場合は左右を反転して work area 内へ clamp する。
- editor area は一つまたは二つの `EditorGroup` を持つ。二分割は左右または上下だけを許し、再帰分割しない。各 group は独立した tab strip、active `DocumentView`、一つの可視 Canvas slot、focus history を持つ。
- 一つの `DocumentSession` は一つの `InkpodCore` handle、file identity、dirty/savepoint、Undo/Redo、autosave/recovery を所有する。同じ document の全 `DocumentView` は session を共有し、zoom、pan、flip、表示補助、表示中 frame 等の view logical state だけを分離する。文書 raster、layer、history、保存先を view ごとに複製しない。
- tab label は active sequence cell 名、保存 file 名、`無題セル N`、`復元セル`の順で意味のある識別名を使い、dirty は `*`、同じ document の追加 view は `[ビュー N]` で示す。read-only、処理中、error も compact かつ accessible な状態として示す。各可視 tab の右端には DPI 対応の小さな close icon button を置き、label drag と同じ hit target にしない。button は発行時の stable view identity を対象として view を閉じ、最後の view の場合だけ document close と dirty 確認へ進む。Cancel、save failure、stale target では tab、document、active view を変更しない。
- tab の順序と label は各 `DocumentView` の所属 session／generation に結び付ける。選択だけで順序や別文書の label を変更せず、Sequence pane の可視性、pin 先、workspace 共通の表示 cache を tab 名の情報源にしない。
- 文書 tab、Right zone の top-level tab、Sequence 等の pane tab の close は、既存の標準 button control と操作対象を維持し、枠なしの二本の斜線による共通の icon 描画を使う。線幅、余白、DPI scaling、active／hover／pressed／disabled／keyboard focus の描画規則を共通化し、system color を使う。
- 最後の document tab を閉じても workspace window は存続し、文書・Canvas のない editor area を表示する。既定セルを自動作成せず、旧画像・入力 target・snapshot を解除する。空 workspace から `新規作成`、`開く`、最近使った file を利用でき、Cancel／失敗は空状態を維持する。他 window の文書へ暗黙に切り替えず、window close と document close を分離する。
- `CanvasSurface` は非表示 tab ではなく可視 `EditorGroup` ごとに一つ持つ。active tab の切替時に同じ surface を別 `DocumentView` へ bind し直し、非表示 tab 数に比例して swap chain や renderer thread を増やさない。
- dock zone は `TopContext`、`Left`、`Right`、`Bottom`、`Floating`、`Hidden`、`AutoHide` に制限する。各 zone は一方向に並ぶ比率分割枠を持ち、各分割枠は一つ以上の pane からなる tab stack とする。pane の表示、非表示、tab 選択は他の分割枠とその比率を変更せず、任意に再帰する dock tree を作らない。docked tab の内容領域に pane 固有の常設 close button を重複配置せず、非表示化は共通 pane command、floating 時の system close、または keyboard route から行う。pane descriptor は stable type ID、default/allowed zone、scope、multiplicity、float/autohide 可否、最小寸法を宣言する。
- Color、Layer/Plane、Locator 等の inspector pane は、一つだけの split stack でも descriptor の localized title を dock header に表示する。Tool の専用 strip はこの単独 header の対象外とし、Tool Options は dock pane ではなく owned flyout とする。単独の Tool strip は固定幅で zone-extent splitter、float、AutoHide を持たず、表示／非表示だけを許す。Bottom zone の所属 pane が Sequence 一つだけの場合も、Sequence の一段表示に必要な高さへ固定して zone-extent splitter とその 4 DIP 操作領域を生成しない。Bottom に別 pane が所属した場合、または Sequence を別 zone へ移した場合は通常の可変 splitter を復元する。それ以外の splitter は 4 DIP の操作領域を維持し、通常、hover、pointer capture、keyboard focus、high contrast の各状態で system color により境界と操作可能性を識別できるようにする。focus の取得／喪失では同期的に境界を再描画し、別 component へ focus が移った後に強調色を残さない。
- dock、floating pane、editor group の splitter を pointer または keyboard で変更する間は、各更新で確定した最終配置だけを提示する。移動前の frame、caption、control、owner-draw 内容を残さず、未描画領域や中間配置を見せない。resize の完了、cancel、capture 喪失後は親子 window に未処理の再描画領域を残さず、geometry-only の変更で既存 control、内容、選択、focus、scroll position を再生成または reset しない。
- Right zone の top-level tab は固定カテゴリを持たない動的な非空 tab とする。一つの pane type は高々一つの tab に属し、tab 数、各 tab の pane 数、縦順序は既知 pane descriptor 数で bounded にする。tab identity は label や配列 index ではなく nonzero stable layout ID を使う。label は縦方向の先頭 pane の localized title、tooltip と accessible description は所属 pane の全 localized title を順序付きで示す。label drag は drag threshold を越えた同じ top-level tab strip 内だけで順序を変更し、strip 外 drop、`Esc`、capture cancellation は配置を変更しない。
- 非表示の right pane を表示するときは、選択 tab の content height から tab strip と splitter を除いた高さに、全 pane の 96-DPI 基準 minimum height を一か所で DPI 変換した合計が収まれば末尾へ追加する。収まらない場合または選択 tab がない場合は、その pane だけを持つ新しい tab を作る。追加先を選択して pane の自然な先頭 focus target へ移す。表示済み pane の toggle は dock／floating／AutoHide の別によらず非表示にし、古い hidden membership は保持しない。
- 各可視 top-level tab の右端には label drag と重ならない DPI 対応の小さな close icon button を置く。button は発行時の stable layout ID を対象として所属 pane を一括で非表示にし、tab を一回の構造変更で削除する。選択 tab の replacement は直前、次、先頭の順で決め、invalid、capacity failure、stale target は pane、tab、selection を変更しない。pane header／context menu／keyboard から `新しいタブへ移動`、既存 tab への移動、tab／pane の並べ替えへ到達できる。window が狭い場合は editor area を優先し、必要なら未選択 tab label を一時的に抑制するが、model、selection、保存 record は変更しない。
- pane の target scope は `Application`、`FollowActiveView`、`PinnedDocument`、`Job` を区別する。pin 先 document が閉じた場合は別文書へ silent に向けず、追従 mode へ戻して accessible notification を出す。pane action は発行時の target ID と generation を保持する。
- 現在相当の一 window、一 group 配置を初期 named workspace `彩色` として維持する。96 DPI の初期値は body 左端に splitter なしの固定幅 80 DIP の一列 tool pane、中央に document tabs と Canvas、右端に幅 320 DIP の Color と Layer/Plane を縦配置した一つの動的 tab、最下段に status bar とし、上端の Tool Options dock strip は配置しない。既存の 32:68、55:45 比率と 4 DIP splitter は inspector 側の復元可能な layout state とし、単独 Tool strip の幅は対象外とする。
- tool pane の既定 button row は 64 x 34 DIP、一列とし、tool 選択用の主領域と幅 20 DIP の展開領域に分ける。両領域は bezel/border のない owner-draw の flat 表示とし、通常時は pane 背景へ溶け込ませ、hover 時だけ system color で背景を弱く反転し、checked/pressed、disabled、keyboard focus、high contrast を system color で区別する。展開領域の chevron は通常時に system gray text color で主 icon より弱く表示し、hover 時は通常 text color、checked/pressed 時は highlight text color とする。展開領域は `詳細` tooltip/accessibility name を持つ。主領域の正規ラベルは `鉛筆`、`ブラシ`、`消しゴム`、`塗りつぶし`、`閉領域塗り`、`塗り延ばし`、`スポイト`、`直線`、`曲線`、`長方形`、`楕円`、`折れ線`、`線消しゴム`、`グラデーション`、`エアブラシ`、`境界ブラシ`、`ぼかし`、`スタンプ`、`ゴミ取り`、`アルファ階調` とし、詳細名は tooltip で補う。
- named workspace と per-window layout は `%LOCALAPPDATA%\inkpod\Settings\inkpod-settings.json` の `workspaces` に versioned、bounded な human-readable JSON として保存し、`.inkpod` 文書へ混ぜない。動的 tab の key／順序／選択、tab ごとの pane membership／縦順序、pane split weight／visibility／dock・floating・AutoHide placement、選択 preset と既存 window／editor split は、`right-tab-1`、`layer-plane`、`auto-hide` 等の意味が読める文字列と真の数量値で表す。旧 registry record や旧 application-settings file の migration は行わず、旧 application-settings file の検出時削除は通常設定の current-only policy に従う。monitor/DPI 構成が変わった場合は可視 work area へ clamp し、不正 record、重複 pane／tab key、空 tab、範囲外 count、不正 selected tab、overflow、trailing garbage は拒否して初期配置へ戻す。temporary な narrow-window adaptation で保存済み logical layout を上書きしない。
- built-in named workspace は `彩色`、`線整理`、`参照・チェック`、`バッチ`、`集中` を提供する。全 preset は空 tab と重複 pane を持たない。layout record は開いている文書 path、Core state、active stroke、job owner を含めず、未知 pane は無視し、不足する既知 pane は preset 既定値で補う。
- floating pane は owner workspace を持つ通常の owned top-level window とし、閉じる操作では既定で非表示にする。`WS_EX_TOPMOST`、`WS_EX_PALETTEWINDOW`、`WS_EX_NOACTIVATE` は使わず、独立した `WM_DPICHANGED`、keyboard navigation、high contrast、screen reader を扱う。
- 下段の status bar は現在 tool/active plane、document 座標、zoom/view flip/grid、pixel RGBA/selection 寸法、文書寸法/DPI、処理進捗、dirty 状態、複数ストローク入力待ちを短く表示する。
- バックグラウンド処理の共通表示は status bar のジョブ名と標準 progress bar とし、独立した `処理進捗` pane とその表示 command は設けない。file I/O の既存 polling ABI と各 task の進捗照会を再利用し、UI は Core の処理完了を同期的に待たない。総量不明・反映／保存確定中は不定表示とし、read 件数、loaded 件数、処理量を同じ単位として扱わない。複数実行時は選択 job と残件数を表示し、status bar から対象を選択・cancel できる。対象は発行時 workspace と job identity／generation に固定し、tab 切替や遅い完了通知で別 job へ再解決しない。終了後は通常の status 表示へ戻す。
- menu、shortcut、context menu、pane button は同じ command ID と enable/checked state を共有する。command 発行時に immutable な `CommandContext` として workspace、group、session、view、pane/job、generation を確定し、非同期実行時に active tab を再解決しない。stale target は明示 error または安全な no-op とし、現在 active な別文書へ fallback しない。
- separator を除く全ての top-level menu、submenu、静的な実行可能 menu item は、英語 command 名を基準に同じ sibling scope 内で一意な mnemonic を持つ。top-level は File `F`、Edit `E`、View `V`、Cell `L`、Selection `S`、Filter `I`、Tools `T`、Color `C`、Production `P`、Window `W`、Help `H` とする。日本語 resource は `ファイル(&F)`、`開く(&O)...`、英語 resource は `&File`、`&Open...` のように同じ Latin mnemonic を表示し、`Alt` で access-key underline を表示して menu bar から全 menu 末端へ到達できるようにする。mnemonic は localized menu resource の一部であり、command shortcut profile へ混ぜない。動的な最近使った file は表示順の `1`–`8` を access key とする。動的な `Inkpod File Visualization` は最大64件を8件ずつの page に分け、page と各 page 内 item のそれぞれに `1`–`8` の mnemonic を与えて全件へ access-key で到達可能にする。
- 組み込み shortcut preset は、Windows と Visual Studio Code で意味が確立しており inkpod に同じ意味の command が存在する操作だけに既定割当を持つ sparse preset とする。New Cell `Ctrl+N`、Open `Ctrl+O`、Save `Ctrl+S`、Save As `Ctrl+Shift+S`、Undo `Ctrl+Z`、Redo primary／secondary `Ctrl+Y`／`Ctrl+Shift+Z`、Cut／Copy／Paste `Ctrl+X`／`Ctrl+C`／`Ctrl+V`、Select All `Ctrl+A`、Zoom In／Out `Ctrl+=`／`Ctrl+-`、Preferences `Ctrl+,`、Keyboard Shortcuts `Ctrl+K, Ctrl+S`、Help `F1` を維持する。tab は previous primary／secondary `Ctrl+PageUp`／`Ctrl+Shift+Tab`、next primary／secondary `Ctrl+PageDown`／`Ctrl+Tab`、move left／right `Ctrl+Shift+PageUp`／`Ctrl+Shift+PageDown`、Close View primary／secondary `Ctrl+W`／`Ctrl+F4`、Split Right `Ctrl+\`、Move to Other Group `Ctrl+Alt+Right`、Focus Group 1／2 `Ctrl+1`／`Ctrl+2`、Next Editor Group `Ctrl+K, Ctrl+Right`、Close Group `Ctrl+K, W`、New Window `Ctrl+Shift+N`、Duplicate View in New Window `Ctrl+K, O` とする。comma は同時押しではなく次の stroke を表す。
- 描画、fill、selection、palette、motion、pane、workspace 等の inkpod 固有 command は、組み込み preset では全て未割当とする。無修飾の `Q`、`K`、`A` 等、`Q` prefix、palette の `1`–`0`／`Tab` fallback、motion FPS の `Ctrl+Alt+数字` を暗黙の別経路として残さない。組み込み preset の割当有無とは独立して、production command catalog の全 command を shortcut editor に表示し、利用者が主／副の各割当を未設定または最大4 stroke の列として設定できる。ユーザー preset は sparse な組み込み preset の複製から作成できるが、既存の custom profile、import した profile、利用者が明示した `Q`／`K`／`A` 等の割当は、組み込み既定の変更、application update、decode で暗黙に削除または上書きしない。Reset は利用者が選択した command または profile に対する明示操作だけとする。廃止commandを参照するpreset／設定は既存のunknown-command検証で拒否し、移行や他commandへの付け替えは行わない。不正な設定ファイルに対する既定値fallbackと自動上書き防止は維持する。
- shortcut 割当は `Global`、`Canvas`、`Timeline`、`Pane` の context、`Execute`、`Hold`、`Toggle` の action、論理キーまたは物理位置の照合方式を型付き値として持つ。`Global` は全 context と重なり、その他は同じ context 同士だけが重なる。重なる context の完全一致は解決待ちの競合として編集候補内に保持できるが、未解決競合がある候補は適用または永続化しない。解決操作は競合相手の解除または主／副割当の交換とし、prefix 衝突は候補作成時に拒否する。`Hold` は一時 tool 等の明示対応 command、`Toggle` は表示切替等の明示対応 command だけが選択できる。
- shortcut editor は command 名、stable command key、割当キーの文字検索、入力キー検索、context filter、category／競合／変更あり／未割当の件数と絞り込み、競合の前後移動、選択 command の詳細と既定値、修飾キー別の物理 keyboard 可視化を持つ。keyboard 表示は自動、JIS 109、US ANSI 104 を選択でき、キー選択で割当へ移動し、command の drag で割当を作成できる。Win 修飾キーは表示と通常の foreground 入力で扱うが、bare `Alt`、`F10`、`Alt+Space`、top-level menu mnemonic の `Alt+英字` は native menu／system 操作を優先する。shortcut editor の新規 record／rebind でこれらの割当を拒否するが、既存 custom v3 profile の decode／validation では割当自体を削除または無効な profile として拒否せず、そのまま round-trip する。保持した予約割当は runtime では発火せず native 操作が勝つことを editor で説明する。`Alt+F4` は標準の Exit built-in だけを例外とし、`Alt+Tab` 等 OS が foreground application へ配送しない system combination を奪う global hook は使わない。OS が foreground application へ配送しない組合せを設定しても application-wide に動作するとは表示しない。
- shortcut preset の import/export は top-level `format: "inkpod-shortcuts"`、current `formatVersion: 3` の `.inkshortcuts` JSON だけを受理する versioned、length-bounded な application data format とする。command は `file.save`、key は `S`／`KeyS`、modifier は `ctrl` 等の意味が読めて手編集できる文字列で表し、Win32 command ID、virtual-key／scan-code 数値、modifier bit mask、Base64／binary payload は保存しない。unknown version、不正 UTF、duplicate key／command／slot、範囲外 count、trailing data、未対応 enum、未解決競合を拒否し、export は同一 volume の temporary file を完成、flush、close してから置換する。
- Windows frontend が所有する application data はすべて `%LOCALAPPDATA%\inkpod` 以下へ集約する。通常設定の正本は固定名 `Settings\inkpod-settings.json` とし、top-level の `format: "inkpod-settings"` と整数 `formatVersion` で schema を識別する。現行は `formatVersion: 5` とする。UI 言語、保存・復元、animation、color management、shortcut profile、workspace／window layout と named workspace snapshot は UTF-8、2-space indent、末尾改行付きの JSON へ一度だけ保存し、同じ値を registry や別 file へ二重保存しない。missing file／missing optional section は既定値とする。top-level に `format` と `formatVersion` が各一つだけあり、format が完全一致し、version が正の整数かつ current 未満である valid JSON は旧版設定と識別し、内容を移行または decode せず、同じ path を削除権付きで再度開いて byte 列が検出時と一致することを確認してからその handle で削除する。削除成功後は missing file と同様に既定値を使い、削除不能または検証中の I/O failure は起動失敗とする。削除前に内容が変わった場合は一度だけ再判定する。duplicate／unknown field、不正 UTF、不正 enum、現行より新しい version、format 不一致、旧版と一意に識別できない非現行 file は設定全体を staged decode で拒否して保持する。不正 file は自動終了時に上書きせず、既定値を使って診断を出す。保存は同一 directory の temporary file を flush、close 後に原子的に置換する。HKCU は将来の OS 統合または管理 policy だけに限定し、開発中の旧 HKCU／旧 file migration は実装しない。
- session-only の前回文書 path は `Session\inkpod-session.bin` の current-version、bounded binary record とし、通常設定 JSON へ混ぜない。recovery は `Recovery`、Batch set は `batch-sets`、埋め込み help cache は `Help`、派生 cache は `Cache`、log は `Logs` に置き、`%APPDATA%` には inkpod 所有 data を保存しない。
- tab drag は同一 group 内の並べ替え、別 group/window への移動、window 外 drop による新規 window を扱う。active stroke、pointer capture、modal preview 中は開始せず、`Esc` で cancel した場合は元の位置を完全復元する。同じ操作は drag に依存せず menu と keyboard からも実行できる。
- `Ctrl+PageDown`／`Ctrl+PageUp` は次／前の tab の primary、`Ctrl+Tab`／`Ctrl+Shift+Tab` は同じ linear next／previous command の secondary とする。inkpod はこの変更で VS Code の MRU tab picker 自体は導入しない。`Ctrl+1`／`Ctrl+2` は第1／第2 editor group、`Ctrl+K, Ctrl+Right` は次の editor group、`F6`／`Shift+F6` は menu・dock pane・editor area・status の focus、`Ctrl+W` と secondary `Ctrl+F4` は view close に使う。tab、splitter、pane header、AutoHide、target、dirty、job progress と command の disabled state は UI Automation から取得できるようにする。
- 数値入力と選択肢を共有する modal dialog は、選択肢ごとに標準 combo box を使い、owner window の中央かつ monitor work area 内へ配置する。Cancel は表示前の状態を変えない。
- 実行不能 command は disable する。例として選択なしの一部 command、対象 layer 未指定の batch を無言で成功させない。未接続 button、空 pane、常時成功する stub は生成しない。

### 3. メニュー構成

UI 表示文字列は、日本語と英語を言語非依存の型付き ID で参照する一つの catalog で管理する。単語単位の部分置換で表示文を組み立てず、各言語の完成した文または format string を catalog に置く。文書名、path、Light Table set 名等のユーザー所有文字列は翻訳せず、catalog 由来の prefix/suffix と明示的に合成する。`編集 > 環境設定 > 全般` で `システム設定`、`日本語`、`English` を選択でき、次回起動から process 内の全 workspace に適用する。`システム設定` は Windows の第1優先 UI 言語が `ja` の場合だけ日本語を選び、それ以外または判定不能時は英語を選ぶ。選択値は `inkpod-settings.json` の `general.uiLanguage` に `system`、`ja-JP`、`en-US` のいずれかで保存し、不正な設定 file は全体を拒否して `システム設定` を含む既定値へ戻す。言語は文書、履歴、native file、ユーザー入力の名前や path に混ぜない。実行可能な button、checkbox 等の catalog 由来 caption は、各 pane の最小幅と 96/120/144/192 DPI 相当の標準 UI font で全文を表示し、必要なら操作行を折り返す。省略表示を許すのは文書名や path 等の可変長ユーザー所有文字列であり、操作 caption の切り詰め、略称化、font 縮小で代用しない。以下は機能上必要な top-level menu と command です。ellipsis と並びは Windows の標準慣習に合わせ、全 menu caption の mnemonic は本節の collision-free access-key 契約に従う。

#### ファイル

- `新規セル`: 用紙、pixel 寸法、DPI、100 frame、frame 配置、色深度、作成枚数を指定し、標準画像 layer を持つセルを作る。
- `開く`: `.inkpod` または対応 raster/sequence を共通の入口で開く。独立した「ラスターを読み込み」command は設けない。
- `最近使ったファイル`: 存在確認し、消失 path は履歴から整理できる。
- `保存`: `Committed` な通常 raster pair、または raster open から得た `Planned` pair へ通常ペア保存する。どちらもない `None` の場合だけ通常保存先を求める。完全な clean `Committed` pair は、両 member の identity／complete stamp と外部競合を再検査した後に限り、実装が物理的な再書き出しを論理的に省略してよい。省略は必須ではなく、通常の pair transaction を検証して両 member を再書き出してもよい。
- `名前を付けて保存`: 新しい `.inkpod` と同一 stem の raster companion の組へ保存する。この command は既存文書からも明示実行でき、`保存` が自動的に保存先を求める条件とは分けて扱う。単体 raster や別形式への平坦化出力は `書き出し > ラスター` の責務であり、`名前を付けて保存` の保存先にはしない。
- 同じ `DocumentSession` に未完了の file I/O が一つでもある間、利用者向けの `保存` と `名前を付けて保存` は disabled とし、command 実行時にも保存先 dialog の表示前と復帰後の両方で同じ条件を再検査する。競合を検出した明示保存を silent に破棄、別文書へ再解決、または暗黙の後続保存へ変換せず、busy failure として現在文書、保存 authority、選択済み保存先を一切公開しない。Sequence／recovery 等の内部 continuation は、発行時 session／generation と予約 token を保持する型付き continuation としてだけ同一 session の file I/O 後へ直列化できる。
- `復帰`: 現在の `Committed` pair の native path を、通常の native-open とは区別した current-document Revert として強制再読込し、最後に通常保存した savepoint へ文書全体を戻す。共有 resolver が同じ native path と同じ document UUID を再現した場合だけ置換し、runtime sequence catalog／active index／inactive cell recovery に加えて、同じ session を表示している全 `DocumentView` の stable view ID と論理 view state を維持する。render cache は破棄して新しい文書 revision から再構築し、active `SequenceFileBinding` を再読込後の pair path／identity と owner generation へ張り直す。別 path、別 UUID、pair conflict、取消、stale は現在文書と frontend authority を変更せず失敗する。
- `レイヤーを部分的に復帰`: active layer/plane の selection 内だけを最後の保存状態から復元する。
- `読み込み`: 一般 raster、連番、palette/chart 等を document または参照データへ読み込む。
- `書き出し > ラスター`: 一枚または通常の連番を PNG/TIFF/TGA/BMP 等へ出力する。寸法、DPI、余白を含めるか、出力 layer、antialias、alpha/白背景合成を指定する。
- `自動保存設定`: 対象 document 種類、間隔、前後セル切替時保存、recovery 保存先を設定する。
- `終了`: dirty document ごとに保存/破棄/cancel を選び、worker を安全に停止する。

#### 編集

- `元に戻す`、`やり直し`: command 名を表示し、履歴がないとき disable する。
- `複数段階戻る`、`複数段階進む`: 履歴一覧から位置を選択する。
- `カット`、`コピー`、`ペースト`、`選択範囲にペースト`、`変換してペースト`、`クリアー`。
- `変形 > 左右反転/上下反転/拡大・縮小/回転/移動`: selection 内の実データを変更する。
- `線修正 > 線つなぎ/線幅修正`: selection または tool で指定した範囲へ適用する。
- `スナップ > ガイド/グリッド`: checked state を表示する。
- `アルファ使用モード`: alpha 対応の描画、読み込み、保存を有効にする。
- `編集 > 環境設定`: application／workspace 単位の環境設定を `一般` と `キーボードショートカット` の二つの category tab に集約する。`一般` page は一般、保存と復元、workspace、animation、color 管理の区分見出しを持ち、各区分の少数設定を一つの縦型 form で表示する。animation では Sequence thumbnail の表示幅を 32～96 DIP、既定 64 DIP の整数で指定でき、全 workspace へ即時反映する。同じ区分で検証済み sidecar target cache の上限を 0～1024 MiB、既定 1024 MiB の整数で指定でき、0 は cache を無効化して即時解放する。document、view、tool、batch operation 固有の設定を混ぜない。dialog は typed initial value と候補だけを所有し、`適用`／`OK` の検証と永続化が成功するまで live state を変更せず、`キャンセル` は最後の適用後の状態へ完全に戻す。shortcut page は上記の preset、検索、競合、一覧、詳細、物理 keyboard を提供し、command 数と各 category 件数を production command catalog から算出する。
- `編集 > キーボードショートカット`: 同じ環境設定 dialog を `キーボードショートカット` page を選択した状態で直接開く。別の editor／設定 store を作らず、`Ctrl+K, Ctrl+S` と同じ command route を使う。
- `設定 > グリッド`: 間隔、分割数、原点を指定する。

#### セル

- `用紙設定`: frame 単位または pixel/物理寸法/DPI で用紙を変更し、元画像の anchor を左上/右上/中央/左下/右下から選ぶ。
- `撮影フレームを考慮して用紙サイズ変更`: 撮影 frame を収める用紙へ crop/expand する。
- `画像サイズ`: canvas 寸法を変更し、元画像の配置 anchor を選ぶ。
- `画像解像度`: 物理寸法、DPI、pixel 数、再 sample の有無を指定する。再 sample off は pixel 数を変えない。
- `鏡像 > 水平方向/垂直方向`: 全文書の実データを反転する。
- `回転 > 左90度/右90度`: 全文書の実データと frame 座標を回転する。
- `レイヤー > 新規/複製/削除/非表示を削除/統合/プロパティ`。layer 種類の選択・変換 command は設けない。
- `プレーン > 新規ラスタープレーン/複製/削除/形式変換/統合/アルファ編集/プロパティ/設定`。必須の主線／彩色 plane は複製・削除できない。
- `前のセル`、`次のセル`、`セル番号で移動`、`連続表示`。

#### 選択範囲

- `すべてを選択`、`選択解除`、`選択反転`。
- `描画色を選択`、`描画色以外を選択`、`描画色を選択範囲に追加`。
- `拡張`、`縮小`: pixel 幅を指定する。
- `保存選択 > 現在選択を保存/保存選択を選択/保存選択で置換/保存選択を追加/保存選択を減算/保存選択名を変更/保存選択を削除`。保存選択は layer ではなく stable ID と名前を持つ zero-or-more の文書 mask とし、save/reopen 後も一覧から対象を選べる。
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

- 表示切替は `ツールパレット`、`ツールオプション` flyout、`カラー`、`レイヤー／プレーン`、`ロケーター`、`シーケンス`、`ライトテーブル`、`サブパレット／参照ビュー`、`バッチ` の九つを Window menu 直下の直接 checked toggle とする。menu、shortcut、pane control は同じ command ID と checked state を使い、checked state は dock／floating／AutoHide を含む実可視性、`ツールオプション` は flyout の可視性を表す。Color と Batch の文書固定／追従 command は Window menu に重複配置せず、各 pane の target control から操作する。
- pane toggle 群の直接性を保ったまま、document-tab 操作を `ビューとタブ`、二分割操作を `エディターグループ`、workspace-window 間操作を `ウィンドウ` submenu にまとめ、その後に既存の `ワークスペース` submenu を置く。これは menu の発見性だけを変える regroup であり、command ID、route owner、checked／enabled state、shortcut、`CommandContext` は変更しない。
- `ビューとタブ > 新しいビュー`: active document の別 `DocumentView` を active group に作る。`ビューとタブ > ビューを閉じる`、`文書を閉じる` は、前者は focused view だけを閉じ、後者は全 window/group の該当 view を列挙して document session を一度だけ閉じる。次／前の tab と tab の左／右移動も同じ submenu に置く。
- `エディターグループ > 右へ分割`、`下へ分割`、`別グループへ移動`、`別グループに新しいビュー`、`次のエディターグループ`、`グループを閉じる`: 最大二 group の editor area を command/keyboard から操作する。
- `ウィンドウ > 新しいウィンドウ`、`ビューを次／新しいウィンドウへ移動`、`次／新しいウィンドウに複製ビュー`: 同一 process、同一 UI/Input thread 上の workspace window を操作する。
- `ワークスペース`: named workspace の選択、保存、名前を付けて保存、復元、既定に戻す、および pane の dock/float/hide/auto-hide command を提供する。
- `フルスクリーン`。
- current `彩色` preset との移行互換として、従来の `初期位置`、`現在位置を保存`、`保存位置へ戻す`、`左右を反転` の意味を named workspace command から到達可能にする。

#### ヘルプ

- `inkpod ヘルプ`、`Inkpodファイルフォーマット`、`ショートカット一覧`、`診断情報`、`謝辞`、`設定ファイルを開く`、`inkpod について`。`設定ファイルを開く` は `%LOCALAPPDATA%\inkpod\Settings\inkpod-settings.json` が未作成なら現在設定を原子的に保存してから、JSON に関連付けられたアプリへ渡す。既存の不正な JSON は上書きしない。謝辞は使用する production 外部 library、その用途と license、および完全な第三者通知の参照先をオフラインで表示する。旧製品名や旧 asset を自社製品表示として使わない。

### 4. 起動、終了、読み込み、セル切替、保存

- 起動時は Common Controls、COM、renderer、Rust Core の順に初期化し、途中失敗を status とユーザー向け説明へ変換して確実に unwind する。
- 同一 user/session では一つの論理 application instance とする。Explorer/file association 等からの secondary activation は command line を完全検証してから versioned、length-bounded、current-user 限定の IPC で primary process へ渡し、既定では last-focused workspace の active group を対象にする。明示的な `新しいウィンドウで開く` だけが新規 workspace を作り、primary timeout 時に同じ native file を別 process で無断編集しない。
- `ファイル > 開く` は focused workspace の active editor group に新しい document tab を追加する。同じ logical file identity が既に開いている場合は既存 `DocumentSession` の view を選択し、通常操作で二つの独立 session を作らない。別 view が必要な場合は `新しいビュー` を明示的に使う。
- file identity は Windows の volume/file ID を取得できる場合はそれを使い、取得できない場合は正規化した絶対 path を使う。表示名や tab index を identity に使わず、untitled document には frontend が UUID を発行する。通常 raster pair は native と raster の二つの物理 identity または正規化 missing path を一つの logical identity として registry に予約し、どちらの member から開いても同じ `DocumentSession` を解決する。
- 編集対象の対応 raster path を `R` とすると、companion native candidate `N` は `R` と同じ directory にある同一 stem の `.inkpod` とする。`ファイル > 開く` で `R` を選ぶ場合と、Sequence pane で raster cell `R` を選ぶ場合は同じ companion resolver を使い、前者の新規 tab と後者の cell-switch transaction という publication の違いだけを frontend で扱う。codec、sidecar 優先順位、conflict、logical identity、保存 authority の意味を経路ごとに変えない。resolver の directory inventory は application-owned な上限付き cache とし、OS の directory namespace 変更通知または同等の complete change proof が未発火の間だけ再利用する。entry の追加、削除、rename、observer failure、明示 cache 消去では再列挙し、選択した native／raster member 自体の complete stamp 検証と最終 TOCTOU 検証を inventory cache で省略しない。
- 通常 pair の runtime 保存 authority は `None`、`Planned`、`Committed` の三状態だけとする。companion raster が欠落した repair-needed は `Committed` の下位状態であり、第四の authority ではない。recovery metadata の `REPAIR_NEEDED` を含む四 kind は、この三状態と欠落 member を固定幅 proof へ符号化する表現であって authority の状態数を増やさない。
- `N` が存在する場合は raster だけから新しい Genesis を silent に作らず、`N` の staged decode、全 asset 検証、replay と `R` の staged decode を先に行う。`N` の companion format と `R` の対応形式が一致し、`N` から得る通常 raster composite と `R` の canonical decoded raster が寸法、native depth、straight alpha、全 pixel 値、および形式が保持する DPI で一致した場合だけ、既存 sidecar `N` を優先して開き、`Committed` authority を公開する。この一致は decoded raster の意味上の同一性であり、圧縮方法、packet/chunk 配置、palette 表現、任意 metadata、timestamp、元 encoded byte 列の一致を要求しない。
- `N` が存在しない場合は、`R` を exact-depth の immutable Genesis source asset として取り込んだ clean な native 編集文書をメモリ上に作り、`N`、raster target、format、open 時に観測した identity／complete stamp を session-only の `Planned` pair として保持する。この時点では `.inkpod` を disk に作らず、通常 savepoint または `Committed` authority を公開しない。ただし app 内の重複 open/write を防ぐため、planned pair の両 path は同じ logical identity に予約する。
- `N` が存在するが malformed、非 current version、asset/replay failure、別 companion format、または canonical decoded raster 不一致の場合は、sidecar を無視して raster を silent に開かず、pair conflict として open 全体を失敗させる。選択前の文書を維持し、file の削除、rename、上書き、savepoint／authority の公開を行わない。利用者が別 command の明示的な raster import を選ぶ場合だけ authority `None` の新規文書として扱い、既存 `N` を planned destination にしない。
- `.inkpod` を直接開いた場合も同じ pair 検証を使う。companion が欠落している場合は native を staged replay 後に repair-needed な `Committed` authority として開け、次の通常 `保存` は文書が clean でも companion を再生成する。companion が存在して不一致の場合は外部変更 conflict とし、無言でどちらかを上書きしない。
- 開いたセルと同じ sequence/folder にある画像は file preview に自然順で表示する。thumbnail click、前/次 command、番号指定で切り替える。
- Sequence pane は thumbnail と番号／名前を左から右へ一段に並べ、縦に折り返さず横 scroll する。thumbnail は環境設定の 32～96 DIP の表示幅を上限辺とし、横長、縦長、正方形のいずれも元画像の aspect ratio を維持する。表示倍率だけを変更し、Core thumbnail、cache key、一覧内容を再生成しない。選択 frame が見切れる場合だけ必要量 scroll し、選択・thumbnail 更新・表示幅変更・geometry-only resize で一覧、focus、scroll を reset しない。一覧に focus があるときの `←`／`→` で前後へ移動し、pane 内の前／次 button は置かない。pane の最小高は 168 DIP とし、Bottom に単独所属するときの固定高は current DPI の thumbnail、三つの画像／文字間 padding、一行 text、水平 scrollbar と border、上下余白、header/import row、28 DIP の dock header を device pixel で合計して DIP へ切り上げる。Cut 専用の操作行は作成しない。pane tab の close は Sequence pane だけを非表示にし、document／sequence／history は破棄せず、Window menu から再表示できる。
- PNG/TIFF は decoded RGBA 8/16 bit、TGA/BMP は各 codec が lossless に保持できる対応深度の色と alpha を変換せず初期の主線プレーンへ置き、同じ深度の空の彩色プレーンをその上に作る。16-bit の通常 companion は PNG/TIFF だけが対象であり、TGA/BMP を保存先とする 16-bit pair は事前に precision error として拒否し、8-bit へ silent quantization しない。opaque な完全白は主線に保持するが fill 境界にはせず、alpha が正の非白 pixel を主線境界とする。全 pixel が opaque の source は SolidWhite の用紙、1 pixel でも非 opaque の source は Transparent の用紙を持ち、source pixel 自体はどちらも変更しない。自動的な二値化、主線色への置換、主線／彩色の色分離は行わない。読み込み直後は主線プレーンを選択する。未編集セルを sequence から新たに読み込むときは、document と EditorState の双方に読み込み直後の clean な初期基準を置く。新 layer／plane target への自動 reconciliation は利用者の編集として数えず、移動だけでは dirty、保存確認、不要な autosave を発生させない。これは通常保存の成功や path authority を意味しない。既存セルへの `NOOP`／初回 `BIND`、自動連番の遅い追加は既存の編集・savepoint を保持する。初回 `BIND` の成功時は sequence source を現在文書 UUID と新しい owner generation へ rekey し、active `SequenceFileBinding` を現在文書が既に持つ pair path／identity へ rebase するが、文書、history、dirty、savepoint、通常保存 authority 自体は置換しない。standalone recovery または pair proof `None` の復元は pathless／dirty として exact history と EditorState を保持する一方、sequence 内部の exact pair recovery は resolver baseline と proof が一致するときだけ target 固有 authority と encoded savepoint を再採用し、clean navigation は clean／non-recovered、未保存差分は dirty／recovered とする。Sequence 登録に成功した全セルの immutable source tile、thumbnail、および最大 64 セルの完全な editable Core state は同時に memory resident とする。active／inactive Core は immutable graph／asset／tile backing を COW 共有し、decode に使った同一画素の dense cache ownership は tile 採用後に解放する。セル切替では outgoing state を resident bank へ移し、target state を取り出して交換するだけとし、同一画素の二重 dense 常駐、全面 graph 再構築、native replay、raster decode を行わない。
- 同寸法の通常 sequence セル切替は各 view の zoom、pan、反転を保持し、初期変換と FIT 後の中間 frame を別々に公開しない。異寸法では現在の表示 mode に従う最終変換を一度だけ決定する。初回 open と明示 FIT は引き続き全体表示を行う。
- 未編集 source の再訪では、その初期状態の合成済み tile と GPU bitmap を再利用する。`dirty == false` だけでは初期状態と判定せず、保存済み編集、recovery、preview、Light Table、alpha／color-check 表示と混同しない。source の document UUID／generation と runtime owner を分離し、独立 Core、更新 catalog、別表示 mode 間で cache を誤共有しない。
- source cache は active を含め CPU／GPU 各最大 64 枚・1 GiB とし、application-wide な decoded／GPU 予算の内数とする。連番登録後は全 catalog source を bounded worker／renderer で background 準備し、foreground の切替は未完了準備、upload、Present を同期的に待たない。確保前に予約し、snapshot／tile 等の最後の lease が解放されるまで計上する。予約できない場合は LRU 回収後に通常表示へ戻し、画質を落とさない。
- 同一 catalog の選択変更は項目・thumbnail を再生成せず、選択、必要量の scroll、表示文言だけを更新する。不変 raster の既存 checksum 値を共有し、metadata／サイズ照会のために画像 payload を走査し直さない。
- active cell が dirty の状態で別セルへ移る場合は、versioned application setting で `Prompt` または `Autosave-before-switch` を選ぶ。`Autosave-before-switch` は outgoing の dirty editable state を同期的に resident bank へ退避して target resident へ即時切替し、durable recovery artifact／metadata の publication は切替後に別 job として進める。切替前の Prompt cancel、resident 不在、queue rejection、stale では現在セルと未保存編集を保ち、切替後の autosave 失敗は resident の未保存編集を保持して status へ報告するが、表示セルを巻き戻さない。
- source recovery の要否は dirty flag だけで判定しない。Core は normal save、通常 open、immutable sequence source、または exact sequence recovery を採用した時点の runtime-only document／EditorState revision baseline を記録する。以後の edit、Undo／Redo、branch cut、EditorState 更新で current revision が baseline と異なる実セル切替は、現在位置が savepoint と一致して clean でも、issue-time sequence request の `SOURCE_RECOVERY_REQUIRED` を立てて fresh append-only recovery generation を必須にする。これにより edit→Undo の Redo tail と、旧 tail を切った新 branch を保持する。normal pair 保存成功は current を再 baseline とし、exact recovery の無編集再訪、未編集 navigation、同一セル `NOOP`／初回 `BIND` は不要な recovery を作らない。authority revoke は baseline を失効し、次の Save As 成功まで fail closed とする。
- 自動保存済みセルは sequence entry の document UUID と source generation に関連付け、戻る際は exact native state を staged Core で検証・replayしてから active Core を交換する。flattened preview source から history、layer/plane tree、selection、editor state を再構成しない。通常 pair authority を持つ source の recovery metadata は capture 時の両 member の complete stamp または normalized missing-path identity を exact proof として保持する。戻る時は同じ共通 companion resolver が現在の pair を再解決し、UUID、canonical Genesis、raster source identity、capture 時 proof がすべて一致するときだけ、その target 固有の `Committed`（companion 欠落の repair-needed を含む）または `Planned` runtime authority を recovery の exact history／EditorStateへ再採用する。coherent な外部 pair 保存を含む差替え、欠落 member の出現、stamp／missing identity 不一致は原子的な conflict とする。authority `None` の untitled／明示 raster import／standalone recovery は proof `None` のまま pathless に復元し、`名前を付けて保存`を要求する。
- 別の `(document UUID, source generation)` へ実際に切り替えたときは、旧セルの通常保存先、source path、recovery association、file identity を新セルへ流用しない。同じ UUID でも source generation が異なれば実切替とする。target は共通 companion resolver が返した target 固有の `Committed`、`Planned`、または明示 conflict result を採用する。valid sidecar があれば exact history／EditorState を staged replayし、sidecar がなければ clean Genesis と planned pair を採用する。Core は no-op、現在の画像への初回の連番関連付け、実際の文書置換を判別し、`NOOP`／`BIND` だけなら文書と旧保存 authority を維持する。target 解決、identity 予約、source の保存／recovery 判断、Core replacement の全てが成功するまで旧セルとその authority を保持し、取消、失敗、stale、queue rejection では target state を公開しない。
- application は replay と canonical companion 比較を完了した clean／non-recovered な sidecar target を application-wide LRU に保持できる。key は正規化した native／raster 両 path と両方の complete stamp とし、最大 64 target、上記 0～1024 MiB の保守的 logical weight 上限を同時に適用する。hit は Core、asset、tile の immutable backing を COW 共有し、native read、replay、raster decode、全面比較を繰り返さない。path または stamp の変化、sidecar 消失、上限縮小、LRU pressure は entry を失効し、dirty、recovery、planned／pending target は cache しない。directory proof、選択 member の stamp、最終 namespace／TOCTOU 検証は cache hit でも省略しない。active job／Core が既に共有している backing の lifetime は cache eviction と分離し、cache 上限へ live document の所有量を混ぜない。
- `前のセル` と `次のセル` は、自然順に存在する entry だけを対象として欠番を飛ばす。closed な端点 policy は `Stop` と `Wrap` の二つとし、`Stop` は先頭から前／末尾から次を完全な no-op、`Wrap` は先頭と末尾を相互に切り替える。空 sequence と一件だけの sequence も別の明示 no-op result とする。
- 端点 policy は application-wide な `inkpod-settings.json` の `animation.sequenceEndpoint` とし、`stop` または `wrap` の読みやすい文字列で保存する。既定は `Stop`、missing setting は `Stop`、不正または非現行の設定 file は全体を拒否して `Stop` を含む既定値へ戻す。同一 process の全 workspace window は同じ値を使い、`連番・サブパレット > 端点で循環` の menu checked state、設定 command、configurable shortcut、status／accessibility 表示を一つの setting へ接続する。この値は document／EditorState／canonical procedure ではなく、document revision、history、journal、dirty、savepoint、`.inkpod` format を変えない。
- 通常の前後セル command は受付時の workspace／session／view／generation、direction、端点 policy、catalog owner／revision を固定する。切替中の方向入力は上限付き queue に順序どおり保持し、先行する切替の確定状態に対して次の target を解決する。実行準備時に source／target の document UUID、source generation、自然順 index、cell number を固定し、commit 前に Core が同じ target を再解決する。catalog／source／target の変更や対象 close は stale として原子的に拒否する。UI は重い合成完了を同期的に待たず、新しい source の実 document revision と切替固有の一時 token が正しく Present されるまでは新しい描画開始を受け付けない。復元では revision が同値または低下しても古い画像の Present を完了扱いにしない。この token は Windows の入力／描画連携だけに使い、Core の cache key、C ABI、永続化へ入れない。受理済み stroke の sample／end／cancel は失わない。通常 navigation の端点 policy と motion check 自身の loop setting は独立とする。
- 通常保存、自動保存、recovery、export は別 status とし、初回の連番探索と個別セルの pair 解決／読み込みも別 status とする。resident fast-path の cell switch では `セルを読み込んでいます` を表示せず、Canvas／選択を即時更新し、pane・menu 等の二次投影だけを短時間 coalesce する。自動保存成功だけで通常 savepoint、document path authority、dirty 表示を進めず、autosave timer を cell switch ごとに再起動しない。
- 通常 raster-pair destination state は `Planned`、`Committed`、`None` の三つだけとする。`Committed` を持つ文書の `保存` は同じ native/raster pair を使う。`Planned` を持つ文書の `保存` は destination dialog を開かず、open 時に固定した両 target の identity／missing-path proof を再検証して最初の pair を materialize する。`None` の文書の `保存` だけが通常保存先を得るため destination dialog へ進む。
- `Planned` は通常 path/savepoint authority または既存 `.inkpod` の overwrite authority ではない。open 後に native candidate が作成された場合、または raster identity／complete stamp が変わった場合は first save を conflict として止め、silent overwrite しない。明示 `保存` による planned pair の初回 materialize と、committed pair の companion 修復は、文書が clean でも no-op にしない。
- standalone の recovery open は例外として authority `None` の pathless／dirty session とする。通常の起動時 recovery open では metadata の original identity/path、source path、pair proof を重複 open の防止、conflict 表示、利用者への候補提示にだけ使い、`Planned` または `Committed` を自動採用しない。前項の sequence-associated target recovery だけが、同じ raster-pair resolver と exact proof の再検証後に target 固有 authority を再採用できる。standalone recovery は利用者が明示した destination への通常 pair 保存が成功した後だけ `Committed` authority と document/editor savepoint を取得する。
- 保存は temp file 完成後の置換とし、失敗しても元ファイルを残す。起動時は全 recovery 候補を列挙し、一件ずつ復元/破棄/保留を選べるようにして、silent に捨てない。通常の前回文書復元は layout と crash recovery から分離した既定 off の明示設定とする。document session を閉じる時も、まだ通常保存で退役していない inactive sequence cell の append-only recovery を自動削除しない。live sequence association は終了するが、artifact と metadata は起動時の standalone recovery 候補として残し、authority `None` の pathless 文書として exact history／EditorState を復元できるようにする。対象 cell の通常保存成功または利用者がその候補を明示破棄した場合だけ exact proof 付きで削除する。
- `名前を付けて保存` は任意の編集文書から明示実行でき、成功時に pair logical identity registry、title、recent files、recovery metadata を一つの transaction として更新する。保存先 pair のいずれかの identity または missing path が別の open/planned session と競合する場合は上書きや silent merge をせず、明示的な解決を求める。
- `復帰` は `INKPOD_IO_OPEN_NATIVE` に force reload と current-document Revert の両 flag を付ける専用要求とし、通常の強制 open を Revert と推測しない。Core は発行時の current path と document UUID の完全一致を apply 時にも再検証し、成功時は native に保存された document/history/editor/savepoint を採用する一方、runtime-only の sequence catalog、active index、全 live view の stable ID／論理 state と次の view ID を保持する。旧 document revision に属する render cache は破棄して再構築する。frontend は Core apply 後に pair logical identity、shell path、active `SequenceFileBinding` と sequence pane projection を新しい owner generationへ一回で再公開し、inactive recovery association を破棄しない。Core apply 後の snapshot／presentation failure は適用済み Revert を再実行せず、authority と UI を適用済み状態へ reconciliation してから error を報告する。
- 外部変更または read-only は session ごとに検出し、保存前に利用者へ示す。read-only document を同じ path へ無言で書き換えず、reload は dirty/history を失うため明示確認と cancel を持つ。
- `view を閉じる` は一つの `DocumentView` だけを閉じる。最後の view でなければ document、history、dirty、job を保持し、dirty 確認を出さない。
- `document を閉じる` は全 workspace/group の該当 view を列挙し、dirty session について一度だけ保存/破棄/cancel を確認する。cancel または save failure では一つも閉じない。
- `window を閉じる` は、その window から消える view のうち他 window に view が残らない dirty session だけを確認する。他 window に残る session、Canvas、job を破棄しない。
- `application を終了する` は dirty な `DocumentSession` ごとに一度だけ保存判断を求める。同じ document の view 数だけ dialog を出さず、cancel または save failure では shutdown を開始しない。
- shutdown は新規 command/input を停止し、active stroke/modal preview と dirty 判断を解決し、layout を保存し、Canvas unbind と snapshot drain、Core work cancel/drain と owner-thread destroy、renderer resource の owner-thread 破棄、最後の `HWND` 破棄の順に行う。

#### 4.1 共有ファイル I/O と編集・参照の分離

- `IO-003`: application 所有の一つの Rust `inkpod-io` manager が、編集 native/recovery、編集 PNG/TIFF/TGA/BMP、自動・明示連番、サブパレット/Reference、Light Table 追加/更新、Batch file/folder/preview の filesystem 操作を共有する。Windows はパスと型付き操作・対象だけを渡し、画像 byte read/write、列挙、file identity、temporary/replace/cleanup は Rust が行う。GUI icon、palette/chart、shortcut/settings、clipboard の memory image、test fixture は対象外とする。
- ラスタ編集 open は File Open と Sequence で共通の companion resolver を通す。valid な同一 stem sidecar があればその Genesis／Assets／Procedures／EditorState を staged replayして既存 native document と `Committed` authority を開き、sidecar がなければ codec を閉じた PNG/TIFF/TGA/BMP 値と exact decoded pixel asset から新しい native document と `Planned` pair をメモリ上に作る。新規 Cell の既定は PNG とし、環境設定の `saveAndRecovery.defaultRasterFormat` (`png`/`tiff`/`tga`/`bmp`) は以後の新規 Cell にだけ適用する。参照 open は読み取り専用の immutable decoded image と view を作り、companion 探索、編集用 Genesis/history、planned/committed pair を作らない。
- 通常の編集 Save/Save As は、一つの immutable document/editor state から `.inkpod` と同一 stem のラスタを同一 directory に作る一つの logical pair transaction とする。新しく導出する TIFF 出力 suffix は `.tif`、入力は `.tif`/`.tiff` を受理する。既存の `Planned`／`Committed` raster authority が `.tiff` の場合、通常 `保存` は同じ file identity を維持するためその path を使い、明示 `名前を付けて保存` の新しい pair は `.tif` を使う。通常 raster composite は Light Table、guide、selection overlay を含まない。TGA/BMP の RGBA16 等、出力形式が保持できない precision は事前エラーとし、silent quantization をしない。
- 二つの独立 file の filesystem-level atomic replace を約束しない。両 temporary の完成/flush/close、両 destination の identity/overwrite 再検査、ordered lock と bounded recovery record/backup を用い、native `.inkpod`、raster companion の順で install する。ただし両 output の install 成功後だけ `Planned` を破棄して `Committed`、通常 path と document/editor savepoint を一回で公開し、中間の native-only 状態または片方だけの完了を保存成功として公開しない。部分失敗は復元を試み、未解決の crash/external conflict を成功表示せず復旧対象として保持する。native が clean でも companion 欠落は再生成し、外部変更は確認する。autosave/recovery、明示 export、Batch 指定形式出力、preview temporary は native 一 file 等の独立した既存目的を維持し、通常ペア保存を暗黙適用しない。
- IO は bounded worker/queue で非同期・並列に実行し、同じ物理 file と replacement destination の read/write は排他する。file alias と置換後の file identity 変更を扱う。live Core は owner thread に保持し、staged decode/replay/encode 結果だけを受け取る。stale/cancel/failure は別 active target へ fallback せず、文書と既存表示を保持する。
- shared LRU の上限は application 全体で 10,000 image、encoded 一 file 512 MiB、encoded 合計 8 GiB、decoded pixel 合計 8 GiB とする。native/recovery は画像 cache に入れず、既存の 1 GiB bounded streaming 上限を維持する。read/allocation 前に容量を予約し、利用中 lease、旧表示と置換候補、結果待ちを計上する。cache map から外しただけで live payload を解放済みと数えない。subpalette の全件常駐と I/O/decode-free navigation を保持し、旧/新が同時に収まらない場合は新 candidate を失敗させ旧表示を保つ。文書/history の Asset は eviction 対象にしない。
- 自動連番は単体 open 成功後の独立 job とする。Rust worker 内で directory 直下を同期列挙し、stem の最後の ASCII 数字列とその前後が一致する PNG/TIFF/TGA/BMP を自然順で集める。数字幅は一致不要、拡張子は pattern の一部にしない。開いた source を必ず含む自然順近傍最大 1,000 件を選び、超過は truncated と表示する。この上限を一般 sequence/subpalette/Batch 件数へ流用しない。late completion は既存 Genesis を再 open/activate せず、その後の編集を保持する。
- polling ABI は状態、列挙数、対象数、read 完了数、`loaded_count`、失敗/取消数を別々に返す。`loaded_count` は decode/検証が完了し使用可能な画像数で、cache hit を含む。poll/cancel は live Core を借用せず任意 thread で行え、反映は発行時 target/generation を owner thread で再検証する。job の結果と handle は Rust-owned とし、release と参照は明示同期する。

### 5. 標準画像レイヤーと合成

Inkpod の画像 layer は一種類だけとし、`LayerKind` や初期 layer 種類を永続化・公開 API・UI に持たせません。各 layer は stable ID、名前、visibility、editability、opacity と、次の plane topology を持ちます。

- `主線プレーン`: exactly one。`BinaryMask8`、`Grayscale8/16`、または straight-alpha `RGBA8/16` を保持する。格納形式は layer 種類ではなく `PixelFormat` で表し、彩色 command から保護する一方、明示的な線修正 command はこの plane を対象にできる。
- `彩色プレーン`: exactly one。straight-alpha `RGBA8/16` で色トレース線と塗り色を保持する。新規作成・画像読み込み時は主線に対応する depth で初期化し、その後の明示的な形式変換は plane ごとに扱う。
- `ラスタープレーン`: zero or more。背景、特効、airbrush、gradient、retouch 等を任意に分離する straight-alpha `RGBA8/16` plane とする。

必須の主線／彩色 plane は削除・複製できず、失敗時に自動再作成するのではなく topology validation で原子的に拒否します。追加 layer の主線／彩色形式は文書の primary layer から継承します。二値／階調／RGBA の変換は layer 変換ではなく対象 plane の role を変えない明示的な形式変換とし、損失内容を事前表示します。

現在選択、保存選択、fill protection は layer/plane 木へ入れません。現在選択と fill protection はそれぞれ一つの文書 mask、保存選択は stable ID と名前を持つ zero or more の文書 mask とし、いずれも通常 composite、thumbnail、raster export へ参加しません。

Frame は `FrameMetadata` と独立した角度付き撮影 frame object だけで表し、画像 layer として表しません。消失点と adjustment layer は文書状態、native format、replay、公開 API、UI のいずれにも含めません。

composite は layer/plane 順、visibility、opacity、alpha を決定的に適用します。プレーンは所属 layer を越えて並べ替えず、layer 同士と同一 layer 内 plane 同士を別に並べ替えます。layer と同一 layer 内 plane はどちらも配列 index 0 を palette の最上段、すなわち合成結果の最上位とします。主線／彩色の役割から合成順を暗黙固定せず、opaque white を保持する読み込み主線の上へ彩色結果を表示できる明示順序を保持します。

#### 角度付き撮影 frame の確定 contract

- 文書は、通常の用紙・作画・安全・撮影範囲を持つ既存の axis-aligned `FrameMetadata` とは別に、角度付き撮影 frame object を 0 個または 1 個保持する。object は文書 namespace 内で再利用しない stable `ShootingFrameId`、milli-pixel document 座標の center、正の width/height、`u32::MAX + 1` を1回転とする時計回り binary turns、左上／右上／中央／左下／右下の操作 anchor、Canvas 表示 flag を持つ。rotation はすべての `u32` を正規化済みの一回転範囲として受理し、座標と corner 計算は checked fixed-point で行う。frame は Canvas 外にまたがってよい。
- center と size/rotation が geometry authority であり、anchor は数値入力と handle 変形の固定点を選ぶための永続する意図である。五点は回転後の四 corner と center に対応し、Core の共通 geometry だけが corner、hit test、handle transform を決める。OS DPI、device pixel、renderer 固有の演算は canonical state と replay に含めない。
- create、complete-replacement update、delete は typed canonical executor と共通 transaction 境界を使う。preview は base document/revision と作業用 document を保持するlong-lived sessionとし、OK は一回の Undo 単位、Cancel は base への完全復元とする。no-op、invalid、Cancel、stale、overflow、failure は document/revision/history/journal/dirty/savepoint/ID high-watermark を進めない。
- 角度付き object は Canvas にだけ表示し、通常の raster export、layer/document thumbnail、paper fit、crop bounds からは必ず除外する。指示画像の書き出し、frame の焼き込み設定と永続 flag は設けない。「撮影フレームを考慮して用紙サイズ変更」の authority は既存 `FrameMetadata::shooting_frame` のままとし、角度付き object と暗黙に相互変換しない。通常の raster export の白背景合成は維持する。
- document mirror/quarter-turn rotate は center、rotation、anchor を厳密に変換する。resample なしの canvas resize は選択 anchor の offset だけを center に加える。resample は等方 scale、またはobjectの辺が document 軸に平行な四分の一回転のときだけ同じ oriented rectangle へ厳密に写す。それ以外の非等方 resample は直交矩形で表現できないため、raster や metadata を部分変更せず文書 transform 全体を `InvalidArgument` で拒否する。

### 6. レイヤー・プレーンパレット

- 上段に layer、下段に active layer の plane を表示する split pane とする。
- layer/plane 間の splitter は pointer と keyboard の双方で高さを変更でき、可視かつ accessible にする。共通の下部操作 button には、現在の操作対象が layer と plane のどちらかを視覚表示と accessible name の双方で明示する。
- 各行に visibility、editable/target、name、plane 役割と形式に応じた color/thumbnail を表示する。layer 種類は表示・選択しない。
- visibility と editable の状態 button は、16 DIP の icon を維持した 32×32 DIP の正方形を 4 DIP 間隔で行中央に配置する。描画と hit test は同じ矩形を使い、完全な状態名は行の accessible text に保持する。
- active selection と複数 edit target を区別する。描画 command は active plane と明示 target 規則を検証する。
- drag and drop で同階層の順序を変える。
- opacity は数値と slider で変更する。
- 新規、複製、削除、property、alpha edit は必ず menu から操作でき、modeless palette を追加する場合も同じ command ID へ委譲する。
- 複製名は一意にする。削除は Undo 可能とし、必須 plane を最後の一枚まで削除できないよう validation する。
- 統合は topology、plane role、format、順序の互換条件を Core で検証し、異なるものを黙って変換しない。
- layer property dialog では name と opacity、plane property dialog では name、opacity、形式固有属性を編集する。layer type field は設けず、形式変換は損失内容を事前表示する。
- 新規 plane command は raster plane だけを作成する。形式は番号入力にせず `RGBA8`／`RGBA16` の標準コンボボックスから選び、OK 時に選択中 layer の topology 制約を Core で再検証する。

### 7. 用紙とフレーム

- 新規 Cell は一つの条件入力で `frame size` または `image size`、DPI、各辺余白、8/16 bit 色深度、五点 anchor、作成枚数を指定する。各 Cell は標準画像 layer 一枚で開始する。作成枚数は 1 以上 64 以下とし、複数作成は全件成功時だけ focused workspace の active EditorGroup へ独立した untitled document として公開する。Cancel、invalid、overflow、UUID/割当/途中 staging failure では Core、session、tab、recent file、stable ID を一件も進めない。
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
- 各可視 EditorGroup の Main Canvas は、Canvas `HWND` の native non-client horizontal／vertical scrollbar を常設する。custom draw／overlay scrollbar や別の document state を作らず、各 bar は移動可能範囲がない場合も非表示にせず標準の disabled state とする。scrollbar を除いた最終 client device-pixel 領域だけを viewport とし、fit、box zoom、sampling、入力座標、snapshot、Present は同じ領域を使う。
- scrollbar は accepted な view transform からだけ作る view-only projection とし、document、EditorState、history、journal、dirty、savepoint、native file、workspace layout を変更しない。各 device axis の scroll position は `q = -pan` とする。accepted zoom／flip を適用し pan を除いた画像の半開 device bounds を `[b0,b1)`、scrollbar を除いた viewport extent を `V` とした base content range は `[b0 - V/2, b1 + V/2)`、page は `V`、base position range は `[b0 - V/2, b1 - V/2]` とする。非有限値、空範囲、固定幅変換の overflow は candidate projection を公開せず、直前の accepted projection を保つ。
- scrollbar の dynamic range は stable `DocumentViewId` ごとに持つ非永続の sticky presentation state とする。accepted `q` が base の外へ達した場合は、越えた側へ `q` を含めたうえでさらに一つの現在 viewport extent を guard として拡張する。thumb tracking 中は gesture 開始時の両端を固定し、line／page scroll、Canvas pan、zoom gesture、geometry-only resize の処理中は暗黙に縮小しない。scroll／pan interaction の完了時に accepted `q` が base 内へ戻っている軸だけは base range へ戻してよい。base 外にある軸は sticky range を保つ。成功した Fit、1:1、明示 view reset、Canvas bind／rebind、document／source 置換では、対応する accepted transform の base range へ再初期化する。非 active tab、別 EditorGroup、同じ document の別 view、Subpalette と range／position を共有しない。
- native line／page／thumb 操作は現在の sticky range 内の絶対 target `q` を求め、直前の accepted `q` との差を相対 `PAN_BY` の型付き view input として既存の view command 経路へ渡す。line step は 32 DIP、page step は scrollbar を除いた accepted viewport extent から一つの line step を引いた値（最小 1 device pixel）とし、native `SCROLLINFO.nPage` 自体は viewport extent とする。UI は candidate thumb を Core acceptance より先に authoritative state としない。invalid、stale、cancel は直前の accepted state を保ち、renderer queue failure は表示中の bar と snapshot を保つ。Core がすでに view input を受理していた場合は viewport refresh からその transform を再発行し、対応する accepted projection が届くまで次の相対 scroll command を許可しない。`Shift`+矢印は対応軸の line scroll、`Shift`+`PageUp`／`PageDown` は vertical page scroll とし、標準 scrollbar と同じ方向へ pan する。
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
- MainLine を対象にするときは文書の主線色、Color/Raster を対象にするときは鉛筆 command が保持する彩色用描画色を stroke 開始時に固定する。一方を変更しても他方を変更しない。
- stroke 開始 pixel が対象に応じた上記描画色と同色なら stroke 全体を erase mode にする auto erase を持つ。`Shift` で auto erase を一時無効にする。

#### 消しゴム

- tool options の先頭に `消去対象: 主線 / 彩色` を常時明示し、選択中の layer/plane、menu の主線/彩色 command、status bar と双方向に同期する。消しゴム選択だけでは対象を自動変更しない。
- shape、太さ、zoom に対して screen size を維持するか、pressure を太さへ反映するかを選ぶ。
- raster は cursor footprint 内を透明へ消し、表示上は文書の用紙を露出する。全 pixel の alpha が最大値の common-raster source は SolidWhite、1 pixel でも alpha が最大値未満の source は Transparent の用紙なので、前者の消去跡は白、後者は透過表示になる。Color/Raster plane の消去値は source の用紙にかかわらず透明とする。

#### 直線・曲線・図形・折れ線

- 直線: start から end へ drag し、release で確定。
- 曲線: start/end を drag 後、control point を動かし click で確定する単純な curve workflow。
- 図形: 長方形、楕円、N角形。outline color/width、fill color、吸着、aspect ratio、中心から作成、作成後回転を持つ。
- 折れ線: click で頂点追加、double click で終了。始終点を結ぶ、区間を Bézier 化する option を持つ。
- line 系は入り、抜き、吸着、45度制約、断面形状を必要に応じて持つ。`Shift` は aspect/angle constraint として一貫させる。

#### ブラシ・エアブラシ

- brush は丸/角、太さ、pressure、stroke smoothing、開始 pixel と同色領域だけへ描く mode を持つ。
- brush の描画色も鉛筆と同じ対象別契約に従い、MainLine では文書の主線色、Color/Raster では brush command が保持する彩色用描画色を stroke 開始時に固定する。
- 開始色限定 brush は stroke 開始時の変更前 raster を immutable base とし、開始 pixel の native-depth 値と完全一致する pixel だけを各 brush footprint 内で描く。Binary／Grayscale 8/16 bit は格納 scalar、RGBA 8/16 bit は straight alpha を含む全 RGBA 成分を比較し、tolerance、表示変換、premultiply 後の値を比較へ使わない。4 近傍の連結性は要求せず、stroke が到達した footprint 内なら非連結の同値 pixel も対象にする。stroke 中に描いた色で predicate を拡張せず、開始 pixel が用紙外なら invalid とする。
- brush smoothing は off または 0〜1000 の整数強度 `s` とする。Core は document Q16.16 の各 x/y 座標について、最初の sample を変更せず、二点目以降を `round_ties_even((previous_normalized * s + raw * (1001 - s)) / 1001)` で因果的に正規化し、pressure は変更しない。中間積は符号付き固定幅の検査付き演算とし、同じ入力 sample 列は frontend の batch 分割、OS pointer history の通知単位、thread 数にかかわらず同じ canonical sample 列と pixel 結果を返す。
- airbrush は太さ、硬さ、dab 間隔、fade、pressure->size、pressure->opacity、停止中も時間で濃くなる continuous spray を持つ。

#### ゴミ取り

- 適用範囲は pen、rectangle、polyline、lasso。
- mode は `背景/透明以外の小点を除去`、`透明/背景の小穴を周囲色で埋める`、`周囲と異なる小領域を周囲色へ置換`。
- 最大サイズを指定し、必要な線を消す可能性を preview で確認する。
- tool は局所、filter は選択または plane 全体を一括処理する。
- 最大サイズは画素数1〜65,536で上限を含む。前景成分は8近傍、穴の背景は4近傍とする。成分サイズと画像端への接続は元plane全体で判定し、成分全体が操作範囲と既存selectionの共通部分に含まれる場合だけ変更する。範囲をまたぐ小点も変更しない。
- Binary/Grayscale主線の背景はcoverage 0。RGBA MainLineの初期設定は透明＋白、Color/Rasterは透明のみ。RGBAの背景色は指定でき、native-depth完全一致で比較する。alpha 0の未使用RGBは区別しない。小点除去は周囲の背景へ戻し、透明と指定色が混在して一意でない場合は変更しない。

#### 線つなぎ

- 指定範囲内で設定 gap 以下の端点候補を結び、raster の接続線幅を指定できる。gapは画素格子上の空白ステップ数で、端点画素中心差(dx,dy)に対するmax(abs(dx),abs(dy))-1とする。水平・垂直・45度とも一画素欠けはgap=1であり、ユークリッド中心間距離とは区別する。
- tool は drag した範囲、menu command は既存 selection を対象にする。
- 誤接続を避けるため候補の距離、角度、対象 plane を決定的に評価し、Undo 一回で戻す。
- 両端点と接続線全体が操作範囲と既存selectionの共通部分に収まる場合だけ接続する。既接続、別線を横切る接続、向かい合わない線端、一意に対応付けられない競合は変更しない。
- gapは0〜64（0は接続なし）、接続全幅は1〜256 document px。8近傍中心線の端点から終端方向を求め、相手への方向との差が両端とも45度以内の候補だけを採用する。空白ステップ数、中心間距離の二乗の順に比較し、互いに一意の最良候補である組だけを結ぶ。同順位、提案接続同士の交差は両方を採用しない。

#### 線幅修正

- 適用範囲は pen/rectangle/polyline/lasso。
- 線補正toolのrectangleは通常選択と同じdocument Q16.16正規化と画素中心判定を使う。device入力のfloat変換誤差を先にfloorして外側の一列を追加しない。penの全sampleはCoreへ渡し、表示用の帯previewはboundedな間引き表示でよい。
- `指定幅だけ太く`、`指定幅だけ細く` を別 mode にし、raster morphology として処理する。
- 増減量は円形近傍の片側半径（document px）と表示する。量1の軸平行断面は1/3/7→3/5/9、収縮は1/3/7→0/1/5。元plane全体を参照し、書込みだけを操作範囲と既存selectionの共通部分へ制限する。
- `指定均一幅`を独立modeとし、既存線の中心線から指定した全幅（document px）で再構成する。十分長い1/3/7幅の区間を同じ指定幅へ揃え、分岐・閉路の接続関係と8/16bit深度・線色を保持する。輪郭とアンチエイリアスは変わり得る。交点・分岐の重なり部分は指定幅より太くなってよい。
- 増減半径と均一化全幅は1〜256。接続／均一化の偶数全幅は中心を左上へ0.5 px寄せた円で描く。均一化は最近傍中心線画素のnative色／coverageを使い、等距離ならy、x順で決める。削除部分は4近傍で最近の元背景へ戻す。RGBA8指定背景色の各16bit成分は257の倍数だけを受理し、8bitへ暗黙に量子化しない。
- 指定背景と完全一致しない非透明画素は、16bitの色差やalphaが最小の非ゼロ値でも前景として扱う。形態演算の比較値を丸めて背景と同じ0にせず、採用する画素のnative RGBA値はそのまま保持する。

### 10. 色、パレット、チャート、参照画像

- Color pane 内の `カラー`、`パレット`、`チャート` は semantic ID と表示順を分離した三つの tab とする。label drag は drag threshold を越えた同じ内部 tab control 内だけで順序を変更し、pane からの undock、Right zone の top-level tab への移動、個別 loading／unloading は行わない。active page と既存 child `HWND` を維持し、control 外 drop、`Esc`、capture cancellation は順序を変更しない。この内部順序は workspace record や `.inkpod` へ保存しない session-local presentation state とする。
- 描画色は sRGB RGBA 8/16 bit を保持し、RGB と HSV editor、alpha 数値/percent 表示を切り替える。
- 色を使う active command は、鉛筆、ブラシ、フィル、選択、エアブラシ等の raster command ごとに独立した彩色用の現在色を持つ。鉛筆の既定色は黒、その他の彩色用 command の既定色は彩色用の初期色とする。command 切替時はその command の現在色を復元し、color editor、swatch、数値欄へ即時反映する。color pane は文書の主線色と active command の彩色用描画色を別のラベルと swatch で常時区別し、MainLine への鉛筆/ブラシ stroke は前者、Color/Raster への stroke は後者を使う。RGBA MainLine の主線色変更は既存 pixel を再着色せず、以後の MainLine 描画へだけ適用する。スポイト等の色を持たない一時 tool は直前の色付き command を変更先として維持する。
- color ring、HSV triangle、alpha track の pointer drag は pane-local preview を各入力 sample で即時描画し、button release 時だけ現在色を Core/editor state へ公開する。capture cancellation は drag 開始時の色と hue へ復元し、preview 中に palette/chart list や他 pane を全更新しない。
- color palette は複数 page/group を持ち、cell click で描画色取得、modifier+click で現在色登録、clear/save/load ができる。
- 現在色と subpalette 採取色の登録は、対象文書の全 page/group から native depth と straight RGBA が完全一致する最初の項目を再利用する。同色を再登録しても項目数、document revision、history、dirty は変えず、その項目の選択位置と group へ移る。登録上限に達していても既存色の再利用は可能とする。透明度または native depth／成分値が異なる色は別色として保持し、既存項目や読込 file の重複を自動削除しない。
- 高頻度の10色を選ぶ command と次の10色 group へ切り替える command を shortcut catalog に含める。組み込み preset では `1`–`0` と `Tab` を含めて未割当とし、利用者が shortcut editor で必要なキーを設定できるようにする。
- color chart は色と名前を表形式で管理し、複数 page、検索、次候補、lock、cut/copy/paste、save/load を持つ。旧版の5文字制限は native 形式へ課さない。
- `セルからカラーチャートを作成` は一意色を抽出するが、gradient/antialias 画像で色数が過大になるため、最大数、quantization、preview を用意する。
- chart生成previewは発行時のdocument revisionと同じbase compositeから毎回再抽出し、直前候補へ再量子化しない。previewは候補色、頻度、色数超過、元chartとの差分summaryをboundedに返し、chart、history、journal、dirtyを変更しない。Apply tokenがstaleなら別revisionや別chartへ適用しない。
- 生成結果のApplyはdocument paletteではなくdocument所有のColor chart全体を一transactionで置換する。native depthとstraight alphaを含む完全一致色が既存chartにある場合は最初の同色entryの名前を保持し、新規色だけ1始まりの最終順序に基づく`Color N`を既定名とする。消えた色の名前は残さない。chart lock中はpreviewを許可するがApplyを拒否し、lock状態自体を変更しない。
- chartの現在pageと選択位置はEditorStateでありdocument historyへ含めない。Apply後も選択色が完全一致で残ればそのentryへ追従し、残らない場合だけ先頭entry／page 0へ移す。空chartでは選択なし／page 0とする。Color chart entries、名前、lockとEditorState cursorは通常save/reopenで復元する。
- `SUBPALETTE-001`: subpalette は文書や追従先から独立した workspace 単位の参照 viewport とし、ユーザーが複数選択した PNG/TIFF/TGA/BMP または指定 folder 直下の同形式画像を表示する。folder は再帰走査しない。stem の末尾の十進数字列をセル番号として昇順に並べ、番号付き画像を先、番号なし画像を自然順で後に置く。同じ表示名でも別 source として保持する。
- subpalette viewport 上の pointer は active tool に関係なく常に eyedropper とし、cursor も通常の pointer icon と同程度の外形寸法に収めた eyedropper 表示にする。成功した sampling は表示変換を通した半開区間の device-pixel 座標から元画像の exact native-depth RGBA を取得し、pane 内で採取色を表示する登録 button、現在の描画色、および Color pane の選択対象へ反映する。pointer の押下、移動、離上に含まれる有効 sample は連続して採取し、cancel は採取しない。`取得色を登録` は最後に成功した subpalette sample だけを document palette へ追加し、sample 前は無効とする。file/folder open、前後移動、全体、等倍、採取色登録は標準 pane 幅では compact な一行 toolbar とし、幅不足時だけ折り返す。各 button は UI language に対応する操作名 tooltip を持つ。
- subpalette Canvas も Main Canvas と同じ native non-client horizontal／vertical scrollbar を常設し、移動可能範囲がない bar は表示したまま disabled にする。独立した Subpalette view の accepted zoom／flip／pan、画像 bounds、scrollbar を除いた client viewport から同じ `q = -pan`、半 viewport padding、per-view sticky dynamic range を作り、document view、Main Canvas、別 workspace と range／position を共有しない。成功した全体表示、等倍表示、catalog bind／rebind、source 一覧の置換、active image の変更で candidate の accepted transform に対応する base range へ reset し、failure、cancel、stale completion は旧画像、view、range、position を保つ。
- 画像は前へ／次へ toolbar button と無修飾の Left／Up／PageUp、Right／Down／PageDown で順送り・逆送りできる。これらの無修飾 key は subpalette 内のどの操作 button または viewport に focus があっても画像 navigation を維持し、scrollbar pan へ再割当しない。`Shift`+Left／Right／Up／Down は対応軸の line scroll、`Shift`+PageUp／PageDown は vertical page scroll とし、pane 内の同じ focus 範囲から利用できる。file／folder を開く操作と、前へ、次へ、全体表示、等倍表示、および native scrollbar は localized accessible name／role／range／value を保つ。source の置換または active image の変更時は、表示 snapshot、renderer cache、scroll projection を新しい画像へ切り替える。追従先、pin、アクティブ追従、現在セル、自動的に一つ前、Canvas scroll 連動は subpalette UI に置かない。
- file／folder を開いた時は対象画像を bounded background load で全件読み込み、全 decode が成功した一つの memory-resident cache として置換する。前へ／次へと keyboard navigation は再度の file I/O／decode を行わず、この cache の active image と snapshot だけを切り替える。別 source を開く時は新 cache の完成まで旧 cache と表示を保つ。成功時も workspace-local auxiliary route は維持し、新しい `presentation_epoch` で catalog incarnation を区別して旧 tile ID の再利用を防いだ後、旧 cache を解放する。読込、decode、個数、aggregate memory 上限のいずれかが失敗した場合は旧 cache と選択を保つ。
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
- sourceは確定済みのvisible layer compositeとGenesis assetであり、solid-white paper、Light Table、guide、grid、selection overlay、color-check overlay、previewを含めない。raster、visibility、opacity、layer／plane順を通常のdocument compositeと共有する。8 bit channelは`value * 257`で16 bitへ正確に昇格し、RGBA16 straight alphaで合成する。alphaが0のpixelは検査せず、alphaが正のpixelはpremultiplied表示値ではなく合成後のstraight RGBを検査する。
- 16 bit compositeの`R,G,B`を0以上65535以下とし、`Y_num = 2126*R + 7152*G + 722*B`、`Y′ = round_half_up(Y_num / 10000)`とする。`Cb = round_half_up((65535*18556 + 2*(10000*B - Y_num)) / (2*18556))`、`Cr = round_half_up((65535*15748 + 2*(10000*R - Y_num)) / (2*15748))`とし、検査付き整数演算だけを使う。Y′の安全域は8 bit code相当`16..=235`、Cb／Crは`16..=240`、16 bitでは各境界を257倍した値とする。境界値は安全で、一成分でも範囲外ならそのpixelを規格外候補にする。spatial filterと画像全体1% thresholdは適用せず、pixel単位の候補選択と件数／検査数／透明skip数を返す。
- ガード結果は元pixelを変更せず、`新規`、`追加`、`削除`、`交差`で既存selectionへ一transaction、一canonical procedure、一Undo単位として合成する。`新規`が非空selectionを空maskへ置換する場合は変更、既に同じ空maskの場合だけno-opとする。他operationの同一結果もno-opとし、Cancel、invalid profile、stale base revision、overflow、allocation／composition failureではselection、revision、history、journal、dirty、IDを進めない。
- 大画像scanはrow単位のprogressとcooperative cancellationを持ち、発行時document UUID／base revision／profile／selection operationへ固定する。profile semanticsはcanonical procedureへ保存するが、profileのUI既定値はapplication settingでありdocumentへ永続化しない。
- motion check は同じ sequence の指定範囲を、倍率、背景色、余白色、開始時 pause、selection のみ、light table を含める設定で再生する。
- shortcut catalog は少なくとも 30/25/24/12/10/8 FPS、前後 frame、先頭／末尾、pause／resume、終了の各 command を提供する。これらの inkpod 固有操作は組み込み preset では未割当とし、motion check 中の `Esc` だけは shortcut profile ではなく標準の cancel／終了操作として扱う。
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
- `編集画像と入れ替え` または item double click は、現在編集 image と選択 item を入れ替える。dirty 保存確認を通し、参照側の transform/opacity 情報を壊さない。成功時は編集対象 document の置換として旧 sequence catalog、active-cell association、pair/file binding と保留中の自動 catalog publication を同じ owner generation で失効させる。失敗、cancel、stale の場合はこれらを保持する。
- light table 全体で重なりを透けさせる option と、前後画像登録時の自動 opacity step を持てる。

### 14. 選択範囲

- selection は document 寸法の mask として保持し、処理効果をその mask 内へ限定する。
- tool は rectangle/ellipse、magic wand、lasso、polyline、trace brush。
- operation は `新規`、`追加`、`削除`、`交差`。modifier は Shift=追加、Alt=削除、Shift+Alt=交差を基本とする。
- selection 内を drag したとき、mask だけを移動するか、選択された active plane pixel も floating content として移動するかを option で分ける。
- rectangle/ellipse は aspect ratio、中心から作成、作成後回転、45度 constraint を持つ。
- magic wand は connected same-color、color tolerance、gap close を持つ。階調主線では基本色と coverage semantics を使う。
- wandのgap closeは線つなぎと同じ空白ステップ数の「設定値以下」とし、領域探索前に仮想境界を構築してseedから探索する。sourceの線・背景は変更しない。探索後の選択maskの膨張・収縮で代用しない。
- trace brush は丸/角、太さ、pressure、screen-size固定を持つ。
- 範囲解釈は通常、描線に密着する shrink、閉じた内部、描線形状、必要に応じた境界選択を区別する。
- raster 内容の coverage は Binary／Grayscale 8/16 bit の非ゼロ値、RGBA 8/16 bit の非ゼロ alpha とする。探索は candidate 内の 4 近傍で行い、`描線に密着` は candidate 外周へ到達する未 coverage を除いた coverage と穴、`閉領域内部` は外周へ到達しない未 coverage、`描線形状` は coverage、`境界` は未 coverage または用紙外へ 4 近傍で接する coverage とする。通常は raster 内容を読まず candidate をそのまま使う。
- rectangle／ellipse の aspect は入力範囲を縮めず不足軸を拡張し、中心指定時は開始点を中心とする。回転値は一周を `u32` 全域で表し、45 度 constraint は最寄りの 1/8 周へ丸める。trace の screen-size 固定は gesture 開始時の view zoom で document 径へ正規化し、pressure は各 sample の径へ適用する。
- geometry preview と commit は同じ正規化済み option と mask generator を使う。Cancel、invalid、stale、overflow は mask、revision、履歴、journal を変えない。`新規` が非空 mask を空へ置換する場合は一変更、既に同じ空 mask なら no-op とする。
- 描画色と同じ/異なる領域の全選択、追加、mask expand/shrink を提供する。
- 現在 mask を stable-ID保存選択として複製し、保存選択から現在 mask への置換／追加／削除、rename、delete を Undo／Redo、replay、save／reopen で round-trip 可能にする。保存選択を通常画像 tool の active plane にしない。

### 15. カット、コピー、ペースト

- clipboard payload は source document ID、source scope（layer または plane）、plane role、document origin に対する bounds、pixel/selection、色深度を持つ。
- `コピー` は対象として選択された layer/plane だけを格納する。主線と彩色の両方を target にした場合は両方の typed payload を保持する。
- 通常 `ペースト` は payload と同じ属性の destination plane を優先する。現在別種類の plane が選ばれていても、互換 destination が存在すれば元属性へ貼る。
- `選択範囲にペースト` は clipboard selection を明示的に現在選択 mask へ変換・合成する。損失がある型変換は preview/確認する。
- `変換してペースト` は新規標準 layer または raster plane、色深度、名前を選んで貼る。
- アプリ内 paste は source の文書座標を維持する。destination 用紙が小さくても clip せず保持可能範囲と見えない範囲を正しく扱う。
- paste 直後は floating selection とし、drag 移動、transform、commit、cancel を可能にする。
- 階調主線同士の互換 paste は重なった pixel の暗い方を採用する `比較(暗)` semantics を持つ。
- 外部アプリ向けには標準 Windows image clipboard を併記するが、標準形式では失われる source scope／plane role／座標をアプリ内 private format で補う。

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
- `ツール > Inkpodファイルの可視化` は、process 内で現在開かれており native `.inkpod` の保存先を持つ document session を重複なく列挙する。項目を選ぶと document ごとに一つのモードレス dialog を開き、dialog を開いた時点の in-memory journal の `Commit` record を `JournalEventId` 順に、primitive 名、決定的な `field=value` 引数表現、その commit 後の可視 composite の最大 64×64 straight-alpha RGBA8 thumbnail の三列で表示する。`HistoryMove`、`BranchCut`、Genesis は行にせず、通常 Redo から外れた branch の commit も表示する。巨大な可変長引数は件数、byte 長、digest へ要約し、表示 query は document、revision、history、dirty、savepoint、persistent ID を変更しない。dialog は一つの scrollbar 付き list control だけを content とし、session が閉じられた場合は対応する dialog も閉じる。履歴の再構築は dialog を開いた時点の不変な入力を使い、進捗と cooperative cancel を備え、Core engine queue の末尾へ bounded step ごとに再投入する。UI/Input thread はその完了や Core query を同期的に待たず、status bar の共通ジョブ進捗と list 内の読み込み行を更新する。thumbnail は full-canvas composite を中間生成せず最大 64×64 の出力へ直接合成し、完成後の行データは owner-data list の可視範囲を小さな batch で caller-owned cache へコピーする。
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
  delete、document close、stale generationでは安全にCancelする。dialogとstatus barの共通ジョブ進捗は
  計算中parameter、progress、failureを表示し、UI threadはCore workやPresentを待たない。
- `復帰` は最後の通常保存を staged Core で再構成して置換する。`部分復帰` は保存済み journal state から対象を再構成し、成功時だけ一件の新しい Undo 可能な canonical procedure として commit する。

### 18. フィルタ、特効、レタッチ

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
- alpha edit は raster plane の alpha channel だけを grayscale view で編集し、gradient 等も使える。通常 color plane を誤って変更しない。

### 19. バッチ処理

- 一つの batch set は削除・無効化・並べ替え不能な一つの `入力`、一個以上の順序付き処理、削除・無効化・並べ替え不能な一つの `出力` から成る。入力と出力は `BatchOperationKind` に混在させず、全処理が無効な graph は実行不能とする。処理は enabled、複製、削除、上下移動を持つ。
- 公開 authoring catalog、`.inkbatch`、C ABI、Windows UI が扱う処理は `色置換`、`彩色プレーンへ送る`、`マスキング`、`消去` の四種類だけとする。既存 filter、continuous fill、visibility、resize 等の基礎 Core 機能は削除しないが、Batch v5 から到達可能にしない。
- 入力 node は、複数 file、非再帰 folder、job 発行時 active document の三種類の入力元を複数内包できる。file/folder は `.inkpod`、PNG、TIFF、TGA、BMP だけを受理し、folder は対応 file だけを自然順に列挙する。重複、missing、未対応形式、range、解決件数を preview する。active document は発行時の `DocumentSession` ID と generation に固定し、実行時に別の active document へ再解決しない。
- 出力 node は folder、発行時に固定した active document、新規 tab を選べる。folder format は `.inkpod`、PNG、TIFF、TGA、BMP とし、一件ごとに同一 volume の temporary file を完成してから atomic replace する。active document への適用は結果全体を一つの Undo 単位として dirty にし、path authority と savepoint を進めない。stale generation では何も適用しない。新規 tab は各結果に Rust が新しい document identity を割り当て、pathless/dirty な `DocumentSession` とする。session/tab 上限超過は job 開始前に拒否する。
- folder の命名は bounded template とし、初期 token は `{stem}` と `{index:N}` だけを許可する。拡張子は output format から決め、absolute path、separator、`..`、拡張子 token を拒否する。Core の dry-run は全出力 path、graph 内重複、既存 file 衝突を返し、一切書き込まないが、Windows product UI には独立した dry-run command を置かない。
- native 色一致は対象 Color/Raster plane の格納 depth で判定する。RGBA は straight alpha を含む全成分の完全一致とし、表示変換、premultiply 後の値、tolerance、連結性、暗黙 depth/format 変換を使わない。不一致は item 単位の preview validation error とする。
- Batch v5 の semantic target role は `Color` と `Raster` だけに閉じ、`MainLine` は role code でも fixed-ID 解決後でも拒否する。これにより Batch 経路から主線保護を迂回できない。旧 MainLine role code は tombstone とし、reader、C ABI、Windows UI のいずれも受理しない。
- color replace は `旧色 -> 新色` の bounded 複数行、行ごとの enable、追加、削除、全行反転を持つ。同じ旧色を持つ enabled 行の重複を拒否し、一致 pixel がなければ revision、history、journal、dirty を進めない。既存の exact color pair 抽出機能は維持する。一つの color replace operation は bounded かつ非空の target selector 集合を持ち、各semantic selectorに一致する全layerの対象planeをstable IDへ決定的に展開し、重複planeを一回だけ処理する。全targetは一つの `ApplyBatchOperations` canonical primitive、一回のtransaction、一つのUndo単位として適用し、missing/error、形式不一致、hidden、non-editable、cancel、overflowではどのtargetもcommitしない。
- 二枚の同位置セルから color pair を抽出する場合は、Core が所有する非zero document UUIDと非zero source generationの組で各immutable raster sourceを固定する。両sourceは同じ幅、高さ、native pixel formatを必要とし、異なる寸法、形式、stale／missing identityを変換、resample、現在activeな別cellへの再解決なしに拒否する。比較は同じdocument X/Yの格納値をnative depthで行い、RGBA 8/16 bitではstraight alphaを含む全成分、Binary／Grayscale 8/16 bitでは格納scalarの完全一致を使う。表示変換、premultiply後の値、toleranceを使わない。
- 同じ格納値のpixelは置換pairへ出さず、unchanged件数としてpreviewする。RGBが同じでもalphaが異なれば差分候補とする。各`旧色 -> 新色`候補はpixel件数と、その候補が現れたhalf-open document boundsを持つ。旧色groupは最初の差分pixelのscanline順、同じ旧色内の候補はpixel件数の降順、同数なら新色のnative値順で決定的に並べる。
- 一つの旧色が複数の新色へ対応するone-to-manyは未解決ambiguityとし、最多候補を自動採用しない。利用者がその旧色について一候補だけを選ぶか旧色group全体を除外するまで、graph作成と実行を拒否する。複数の異なる旧色が同じ新色へ対応するmany-to-oneは、それぞれがone-to-oneなら有効な複数pairとして許可する。previewは候補、件数、bounds、alphaを表示する。
- `彩色プレーンへ送る` は、指定色に一致した source pixel の native 値を同じ layer の彩色 plane の同一座標へ移す。destination の非対象座標は保持し、書いた座標の source は source format の empty 値へする。両 plane の format/dimensions は完全一致を必須とし、missing、hidden、non-editable、主線保護、stale revision は source/destination のどちらも変更しない。一つの Core transaction、一つの canonical procedure、一つの Undo 単位とする。
- `マスキング` は selection を流用せず、document 専用の sparse `fill protection mask` を置換する。指定色と一致する座標だけを `255 = 塗りの壁` として保持し、source raster は変更しない。壁 tile だけを割り当てる。mask は全 Core fill 経路の hard boundary、Undo/Redo、branch、replay、save/reopen、snapshot revision、cache invalidation の一部とする。追加/削除合成は初期 scope 外とする。
- `消去` は指定色と一致する source pixel だけを native empty 値へする。RGBA は transparent black、Binary/Grayscale は 0 とし、非対象 pixel は保持する。一致なしは no-op とする。
- マスキングを含む graph は情報を保持できない PNG/TIFF/TGA/BMP folder 出力を拒否し、`.inkpod`、active document、新規 tab だけを許可する。
- Batch pane は `%LOCALAPPDATA%\inkpod\batch-sets` の `.inkbatch` file 名を列挙する編集可能な set 名 dropdown と保存/読込、Input/処理/Output を一列に置く headerless 工程 List-View、追加/複製/削除/上下移動、選択項目別の scrollable parameter host、validation、最下段に `プレビュー`、`全実行`、`中止` の三 button だけを持つ。入力行は `入力 (N件)` と表示し、処理の enable は工程 List-View 内の標準 checkbox で編集して固定 Input/Output には checkbox を表示しない。処理行の checkbox はその領域のclickまたは選択行のSpaceで切り替え、行選択だけでは切り替えない。読込時の decoder は current v5 だけを受理する。set 名は前後の空白を正規化し、path separator、Windows 予約名、末尾 dot を拒否し、dropdown には拡張子を除いた名前を表示する。Batch pane 自体には document follow/pin 表示や pin 操作を置かず、job 発行時 target 固定は command context で行う。`＋処理` は上記四候補だけの localizable popup とし、parameter は標準 Common Controls で inline 編集する。色置換parameterは `Raster` と `Color` の target plane role を標準checkboxで一つ以上複数選択でき、読込済みfixed-ID selectorは利用者がsemantic checkboxへ切り替えるまで保持する。layer kind selector は設けない。入力 file/folder と folder 出力は parameter host 内の参照 button から Windows file/folder picker へ接続し、file picker は対応形式だけを列挙して複数選択を取り込む。parameter page は非表示 control の空間を残さない page 別の自然高さで配置し、動的 control も pane と同じ GUI font を使う。色 table は常に client 幅を使い切り、単一の選択行だけを選択表示し、alpha を反映した swatch と depth/RGBA 数値を重ならない領域へ表示する。選択行では旧色と新色のalphaをnative depthの範囲（8 bitは0–255、16 bitは0–65535）で数値編集できる。`描画色から取得` は色置換では旧色／新色の選択menuを開き、その他の処理ではその処理色へ適用し、適用後の値をpane内で常時確認可能にする。validation／実行結果欄は選択・コピー・縦scroll可能な read-only multiline control とし、実行失敗時は総件数だけでなく入力名と item 固有の理由を表示する。既知の対象欠落、hidden／non-editable、pixel format 不一致は日本語／英語へlocalizeし、未知のCore診断はboundedな技術詳細として表示する。一度に表示する失敗は先頭8件までとし、残件数を示す。
- `プレビュー` は Core engine thread 上の非同期 job とし、4 GiB 以下の専用 temporary job directory を作る。file/folder input は encoded file を chunk copy し、active document input は issue-time の immutable document/assets を temporary `.inkpod` へ materialize する。全 input の copy/materialize が完了するまで最初の処理を開始せず、copy 後の原 file の変更を処理入力へ反映しない。各 copy に全 enabled 処理を順に適用し、folder output 設定では同じ output format、それ以外では temporary `.inkpod` へ保存して再読込する。設定された実 output folder、active document、新規 tab へは書き込まない。
- preview 結果は input 順の決定的なほぼ正方格子へ並べた一枚の straight-alpha RGBA8 contact sheet とする。thumbnail の長辺上限は 160 pixel、padding は8 pixelを基準とし、全 contact sheet が16,777,216 pixel以下になるよう縮小する。透明部分には checkerboard、item failure には赤系 placeholder、Stop 後の未処理 item には灰色 placeholder を表示する。成功時は一つの clean/pathless Rust-owned staged Core として新しい Canvas tabへ `バッチプレビュー`／`Batch Preview` の名前で公開し、専用 temporary job directory は tab 公開前に削除する。この tab は表示専用の Batch preview とし、active document input の source として再解決しない。preview tab が active のまま後続の `プレビュー` または `全実行` を発行した場合は、その preview が保持する元の issue-time document/view context を target とし、元 target が stale なら active preview へ fallback せず拒否する。cancel、stale target、cleanup failure では tabを公開せず、元 document、実 output、revision、history、dirty、savepointを変更しない。
- Batch pane は右側 tool tab を開くたびに Batch 専用 tab へ配置する。この tab は Batch pane 一つだけを持ち、Batch を既存 tab へ追加することも、他 pane を Batch tab へ追加することも、drag／復元／workspace decode を含む全経路で許可しない。
- UI は選択変更で document/immutable graph を変更せず draft view model を編集し、preview/run/save 時だけ検証済み immutable graph を一回構築する。読込済み `.inkbatch` v5 は input/operation/outputと全target selectorを draft へ完全復元して編集可能にする。狭い pane は縦 scroll と responsive button wrap を使い、日本語/英語、96/120/144/192 DPI、high contrast、Tab/F6、screen reader name を扱う。
- 一件ごとに temporary output から atomic commit し、cancel/失敗した item に部分 output を残さない。dry-run は一切書かない。

将来のBatch authoring／execution形式であるInkScriptのlanguage core、schema registry、exact-source／rebound
等価性、実装gateは[`INKSCRIPT.md`](INKSCRIPT.md)を規範とする。exact-current `.inkbatch` v5 と Batch v5
UI／ABIを production contract とする。M23で批准済みcatalogを使うRust compile／bind／staged-run APIはproductから独立して
公開してよいが、`.inkscript` file filter、clipboard、C ABI、Windows command／UI、Batch production executorからは
各owner milestoneとM34 cutoverまで到達可能にしない。

### 20. 形式、白透過、一般画像入出力

- exact-current 契約は `.inkpod` top-level format v34、runtime replay epoch 29、C ABI v34、`DocumentArchive` schema 7、必須 `DOCM` schema 9、`DocumentStateDigest` schema 13/domain 11、snapshot-composite schema 5、`.inkbatch` v5／operation schema 4、InkScript registry schema／language／file v2、74-command production catalog／owner manifest v7 とする。native v33以前、epoch 28以前、`.inkbatch` v4以前、catalog／owner manifest v6以前、および廃止した Cut descriptor・指示画像書き出しの契約は migration や shim を設けず拒否する。旧ABIの呼び出し元は現行headerでの再buildを必須とする。ABI v33 は Cut handle／records／functions、Cut 判別と指示画像の I/O flags、撮影 frame の指示 export flag を削除する。frame の入力・永続化・canonical procedure から指示 export flag を除去するため、`EditShootingFrame` schema は3、semantics revisionは2へ進める。通常Sequence、pair authority、resident bank、render preparationは引き続きruntime-onlyとする。今回の更新は native format freeze 宣言ではない。 ABI v34は線補正request・task APIと2種類のraster line toolを追加する。`ApplyLineCorrection` schema 2／semantics 1、`ApplyDustRemoval` schema 3／semantics 3、`ApplySelection` semantics 4とし、背景判定・成分全体判定・inclusive gap・探索前仮想境界・線幅modeをcanonical replayへ固定する。
- native `.inkpod` は、保存時点の可変 raster snapshot を意味上の正本にしない。正本は immutable な `Genesis`、content-addressed な `Assets`、Core が検証・正規化して実変更を確定した `Procedures` と history control event、history の現在位置と high-watermark を持つ `META`、文書単位の `EditorState` とする。materialized document、inverse delta、COW snapshot、render/checkpoint cache は派生物であり、これらだけで文書を成立させない。
- frontend request は target/revision/ID と上限を検証し、座標、色、option、可変長入力、transaction 内の output ID を正規化してから一つの `CanonicalProcedure` として確定する。procedure は monotonic ID、primitive ID/schema、replay epoch、base/committed `StateId`、固定幅引数、stable input/output ID、immutable `AssetId` または bounded inline payload、pre/post document-state digest を持ち、raw pointer、外部 path、native enum layout、frontend command ID、一時 object ID を含めない。
- `Genesis` は document UUID、paper、DPI、sRGB、frame、margin、初期 stable-ID topology、immutable base surface を完全記述する。白紙の base surface は全面 tile を割り当てない opaque white の `SolidWhite` underlay とし、flat canonical composite/export には参加するが、個別 layer/plane export や selection mask へ暗黙に混入させない。
- import、clipboard、Light Table 等の外部入力は ingestion 時に Rust が canonical pixel payload へ変換し、immutable `AssetId` を発行する。procedure は外部 path、codec の再実行、caller buffer の lifetime を参照しない。元 encoded bytes や provenance は replay に影響しない任意 metadata としてのみ保持できる。
- 編集用 raster open の「無損失」は、supported codec が decode した canonical pixel payload の寸法、native 8/16-bit depth、straight alpha と全 channel 値を変更せず immutable Genesis source asset として保持し、`.inkpod` の `GENS`／`ASST` から cache-free replayできることを意味する。元 container の圧縮、packet/chunk、palette 表現、任意 metadata、provenance、file 名/path、encoded byte 列の byte-for-byte 保存または再生成は意味しない。これらを保持する場合も replay に影響しない任意 metadata とする。
- 永続 journal は閉じた型 `Commit`、`HistoryMove`、`BranchCut` だけを持つ。実変更を確定した document transaction、実際に移動した Undo/Redo/history jump、history cursor が active branch の tail 以外にある状態からの新規 commit による branch cut だけを順序どおり記録し、query、invalid、failure、cancel、stale、overflow、no-op、stroke/preview の途中更新は記録しない。stroke end、preview apply、floating commit は成功時にそれぞれ一つの canonical procedure とする。
- `.inkpod` section は history procedure/control event を `PROC`、history cursor、active branch、document/editor savepoint と各 persistent ID の high-watermark を `META` に置く。独立した `HIST` section は作らない。`EDIT` は active tool、最後の色付き command、tool ごとの exact-depth color、diameter、fill/selection option、active layer/plane、palette cursor 等の再開に必要な文書単位 editor state を保持する。`CKPT` は任意の open 高速化 cache、`EXTM` は replay に影響しない任意 metadata とする。checkpoint の hash、構造、resource bound 違反は file corruption として拒否し、構造上有効な epoch/prefix/state 不一致だけは checkpoint を無視して full replay する。checkpoint を全て除いても同じ state、pixel、history、次 ID を再構成できなければならない。
- 通常保存後の reopen は画像だけでなく、history list/cursor、Undo/Redo availability、active/non-active branch、document/editor savepoint、persistent ID high-watermark、EditorState を復元する。通常 UI から外れた redo branch も監査可能な append-only journal と asset retention root に残し、自動 squash しない。
- open は decode、全参照・asset 検証、replay を staged Core で完了し、成功時だけ live Core を一回で置換する。通常保存は current `StateId` と `EditorStateDigest` を prospective savepoint として一時 file へ書き、flush、close、pair installation の成功後だけ live path、`Committed` authority と両 savepoint を公開する。autosave、recovery、export は通常 savepoint を進めない。通常の standalone recovery open と pair proof `None` の sequence recovery は metadata の original/source path から authority を復元せず、`None` の pathless かつ dirty な session とする。sequence 内部の exact pair recovery だけは、metadata v4 の capture 時 pair proof、現在の共通 resolver 結果、document UUID、canonical Genesis、raster identity、journal prefix と encoded savepoint baseline がすべて一致するときに限り、target 固有の `Committed`／`Planned`（repair-needed を含む）と encoded savepoint を再採用する。
- 履歴を失う compaction は自動実行しない。利用者へ失われる event/procedure 数を事前表示し、revision と digest で対象を再確認したうえで、open session の path とは別の file へ新しい Genesis として書き出す。成功しても live path、history、dirty、savepoint を変更しない。
- persistent `StateId` は Genesis と commit 済み意味状態を参照し、procedure の precondition、history、savepoint に使う。`DocumentRevision` は stale request 検出用の session-local counter であり file へ保存せず、open 時に新しい Core generation 内で rebase する。EditorState は document history と別の persisted editor revision/digest/savepoint を持ち、session dirty は document state または editor state のいずれかが各 savepoint と異なれば成立する。
- 同じ replay epoch、Genesis、Assets、canonical procedure/control-event 列から、x64、ARM64、非 Windows Rust target で同じ canonical Core state と bit-exact な canonical composite を得る。Direct2D/D3D の画面 antialiasing や monitor 表示の一致はこの契約に含めない。primitive semantics が replay 結果を変える場合は replay epoch と top-level format version を更新する。
- ユーザーがフォーマットフリーズを宣言するまで、`.inkpod`、`.inkbatch`、`.inkshortcuts`、`inkpod-settings.json`、session record、native preset 等の application 固有の永続化形式は現在 version だけを読み書きし、下位互換 reader/writer、migration、互換 shim を持たない。`inkpod-settings.json` は top-level marker だけで一意に識別できる旧版を移行せず削除する通常設定 policy に従う。現在の要件に対して最も頑健で効率的な schema を選ぶ。
- コードフリーズまでは、serialized schemaを変更するたびに対象形式の最上位format versionを必ずインクリメントする。section/record versionだけの変更で代用せず、旧versionは明示的に拒否する。
- raster pair の `Planned`／`Committed`／`None`、logical path identity、companion resolution は session/runtime の filesystem authority であり `.inkpod` へ path を永続化しない。この authority 変更だけでは serialized schema、replay semantics、top-level format version を変更しない。将来 pair path、filesystem identity、source digest を永続 record に加える場合は本節の version 更新規則に従う。
- 一般 raster import/export は少なくとも PNG、TIFF、TGA、BMP の対応可能な 8/16 bit、alpha、DPI を扱う。形式が表せない情報はflatten/export optionで明示する。
- TGA は Truevision TGA 2.0 の標準 image type 0、1、2、3、9、10、11 を対象とし、color-mapped／true-color／black-and-white、非圧縮／RLE、4方向の画像原点、Image ID、Color Map、Footer、Extension Area、Developer Area を境界検査付きで読み書きする。true-color は16／24／32 bit、color-map index は8／16 bit、color-map entry は15／16／24／32 bit、black-and-white は8 bitを標準対応範囲とする。image type 128–255 のdeveloper-defined data、予約済みimage type／bit depth、`.vda`／`.icb`／`.vst`別名は対応形式に含めない。RLE writerはpacketをscanline境界で分割し、readerは既存資産互換のため境界越えpacketも画像全体の上限内に限って受理する。
- TGA import はcanonical straight-alpha RGBA8へ決定的に変換する。5-bit channelはbit replication、premultiplied alphaは整数roundingでstraightへ戻し、alpha attributeが未定義または無効ならopaqueとして扱う。Extension Areaのalpha attribute type、color-correction table、postage stamp、scan-line tableと未知developer fieldは型付きTGA metadataとして境界内で保持し、通常の画像importではpixel結果に必要な情報だけを適用する。pixel aspect ratioはDPIへ読み替えない。
- TGA exportの既定値は既存互換のtop-left／32-bit BGRA／非圧縮／旧形式footerなしを維持する。TGA固有APIではimage kind、depth、RLE、origin、X/Y origin、Image ID、TGA 2.0 footer／extension／developer metadataを明示できる。alphaまたは色精度を失う形式への変換、grayscale化、palette化は明示optionなしに暗黙実行しない。自動palette化は入力走査順を固定し、表現可能色数を超えた場合は失敗する。日時等をwall clockから自動挿入しない。
- legacy workflow の `白背景を合成` をexport optionとして持つ。onなら最下層へ白を合成してalphaを除き、offならformatが許すalphaを保持する。
- legacy white-transparency modeでは完全な白を透明候補としてcheckできるが、native documentでは白色pixelと透明alphaを同一視しない。
- 一枚 export とsequence exportを分け、後者は対象layer、全体/作画frame、size/DPI、antialias、連番規則を設定する。
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

- 大画像は sparse tile、遅延割当、copy-on-write を基本とし、Undo、snapshot、Light Table、checkpoint のために画像全体を無条件に複製しない。snapshot は変更 tile だけを再合成し、pan、zoom、flip、viewport 変更では既存の合成済み tile と renderer resource を再利用する。sidecar のない Sequence pair target は、catalog source と最終 member proof が同じ manager、正規化 path、complete stamp、generation、format、metadata であると runtime に証明できる場合だけ、catalog が既に所有する immutable tile backing と事前計算済み canonical `AssetId` を COW 共有して Genesis/Asset を構築する。dense payload copy、再 hash、全面 tile materialization は行わず、native save／script export 等が canonical dense byte stream を実際に要求する間だけ tile から一時 materialize する。証明できない場合は同じ意味結果を返す owned import へ戻す。この最適化分類は pair authority と resolver-proven target の pristine 再登録を変更しない。
- `revision-max` を採用する理由は、procedure journal や semantic digest の強度を render-cache 検証 cost へ転嫁せず、view-only 操作を source raster byte 数から独立させるためである。完全な source fingerprint より、固定幅 scalar による高速で予測可能な cache hit 判定を優先する。採用経緯、代替案、測定根拠は `docs/architecture.md` と `docs/core-benchmark-baseline.md` を正本とする。
- render tile cache の canonical source identity は `revision-max` 方式とする。各 document tile 座標について、可視 layer 内の可視 plane の `tile_revision`、selection の `tile_revision`、Light Table の `source_revision` の数値最大値を一つの scalar として求める。cache 内の `source_revision` と一致すれば合成済み pixel buffer と renderer-facing tile revision を再利用し、不一致ならその座標だけを再合成して新しい tile revision を公開する。透明な合成結果は cache に保持せず、必要なら再合成してよい。
- cache hit 判定は上記の固定幅 revision scalar だけを読み、source pixel の取得・copy・走査、payload hash/digest、clone generation、削除 tombstone、epoch、negative cache を追加しない。検証 cost は source byte 数ではなく可視 source 数に比例させ、cache hit の zoom/pan snapshot が raster payload size に比例する work を行ってはならない。cache の `source_revision` は非公開の派生 bookkeeping であり、semantic equality、C ABI、document/procedure digest、永続化へ含めない。
- opacity、visibility、layer/plane order、main-line color、color-check mode 等、`revision-max` 式に含まれない render metadata の変更は、同じ commit 境界で whole-cache invalidation を行う。
- 数値最大値は衝突のない source 記述ではない。高い Light Table revision が後続の低い raster revision を mask する場合、同じ最大値を持つ source の一方を削除しても値が変わらない場合、独立 revision namespace が同値になる場合、表示 mode の異なる view が cache を共有する場合がある。また透明結果は negative cache を持たない。これらは `revision-max` を性能上の正本とする際の既知制約であり、暗黙に別方式へ変更しない。
- 性能回帰は wall-clock だけで判定しない。`pan_zoom_snapshot` は quick/full で 2,048/8,192 pair、`dirty_tile_rebuild` は同一 allocated tile への 1 pixel edit と snapshot rebuild を 32/128 回実行し、checksum、revision、tile reuse/rebuild、payload access を固定する。初回 compose では payload access が正に増え、同じ fixture の cache-hit zoom snapshot 128 回では増分 0 を必須とする。private native smoke は 1024 平方・256 allocated tile の 512 wheel event を各一回の Present まで、16 stroke/544 sample の multi-tile drawing を各一回の Present まで測り、sample、Present、queue、resource counter を固定する。
- wall-clock は同じ workload、profile、入力と一致する `docs/core-benchmark-baseline.md` の承認済み環境別 envelope を使い、warm-up 後 5 回以上の中央値で比較する。下限未満は処理省略を疑う診断値にだけ使い、意味ゲートが正常な高速化を拒否しない。上限超過は独立した 5 回以上の再測定でも中央値が上限を超えた場合だけ回帰とする。workload、harness、reference 環境、envelope、`revision-max` 式を変更する場合は、理由、環境、全 sample、意味 counter を記録し、ユーザーの明示承認を得る。envelope を測定結果に合わせて自動緩和しない。
- sequence の独立計測は1754×1240の読み込み済み未編集sourceを対象とし、実 keyboard dispatcher→Sequence pane→Core→Renderer→成功 Present を通す。warm往復では再decode、全画像checksum走査、全面再合成、新規thumbnail生成、同内容のGPU uploadを0、必要な最終snapshotを1回とする。UI handler p95 1 ms以下、snapshot提出p95 4 ms以下、正しい画像の最初の成功Presentまでp95 2 refresh interval以内を目標とし、p50／p95／p99／最大値と意味counterを記録する。初回／未準備／cache追放後／保存待ちは別集計とし、Present APIの戻りを物理画面への到達時刻と同一視しない。これらの目標は既存benchmarkの承認済みenvelopeを置換・緩和しない。

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
- `ABI-001`: exact-current versioned C ABI、opaque handle、ownership test、current-document Revert の型付き flag／組合せ検証
- `ABI-002`: immutable batched render snapshot、Core apply 後の presentation failure と document replacement publication の分離
- `IO-001`: versioned `.inkpod`、exact decoded-pixel Genesis asset、全 history／inactive branch／EditorState／assets replay、atomic native save、round-trip、recovery
- `IO-002`: PNG/TIFF/TGA/BMP import/export、PNG/TIFF の RGBA8/16 pair、TGA/BMP の非対応 precision 拒否、alpha/white background option
- `IO-003`: path-only Rust filesystem boundary、File Open／Sequence／Revert 共通 companion resolver、existing same-stem native priority、三状態 `Planned`／`Committed`／`None` authority、repair-needed `Committed`、pair logical identity、shared bounded parallel I/O、file identity/locks、encoded/decoded LRU、polling、通常 native/raster logical pair 保存
- `WIN-001`: Windows shell、Help/About、DPI/theme、全 menu mnemonic、native menu／system 予約キーを保つ keyboard behavior
- `WIN-002`: 同一 process/UI thread 上の複数 `WorkspaceWindow`、window-local focus/menu/status、application activation、最後の window による shutdown
- `WORKSPACE-001`: 制約付き dock、最大二つの `EditorGroup`、named workspace、versioned layout persistence と monitor/DPI recovery
- `WORKSPACE-002`: pane scope、follow/pin/job target、発行時 `CommandContext`、ID/generation による stale routing rejection
- `SESSION-001`: 複数 `DocumentSession` の paired logical file identity、planned/committed/none authority、active Sequence binding の rebase、current-document Revert、view/document/window/application close、Save fallback／pair-only Save As、autosave/recovery exception lifecycle
- `SAFE-001`: malformed/corrupted input の bounded rejection と非破壊性
- `PERF-001`: 「横断的な性能契約」に定める sparse/COW、変更 tile だけの再合成、canonical `revision-max` cache、payload 非走査、意味 counter、固定 workload、承認済み環境別 envelope を維持する
- `PKG-001`: Rust/Win32 の静的 CRT、x64/ARM64 self-contained MSIX、ならびに ZIP 直下へ `inkpod.exe`、`README.txt`、`LICENSE.txt`、`ThirdPartyNotices.txt` だけを収録する x64/ARM64 portable payload と package/dependency 検証
- `PORT-001`: Rust workspace の OS 非依存性と次 frontend の adapter gap

### Document and view

- `DOC-001`: CellDocument、用紙、DPI、100 frame、基準/作画/安全 frame、余白
- `CELL-001`: image/frame size、DPI、六種 frame、五点 anchor、標準画像 layer、8/16 bit、bounded 複数枚を同一 plan から all-or-none で作る新規 Cell workflow
- `DOC-002`: 種類 field を持たない stable-ID画像 layer と、MainLine exactly one／Color exactly one／Raster zero-or-more の typed plane tree
- `DOC-003`: layer/plane の create/duplicate/delete/reorder/show/edit/opacity/format-convert/compatible-merge と必須 topology 保護
- `RENDER-001`: 画像 layer/plane 木順序、visibility、opacity、alpha を共有する Canvas/thumbnail/flatten 合成
- `SHOOTING-FRAME-001`: stable ID、center／size／binary-turn rotation／五点 anchor／Canvas 表示 policy を持つ独立した角度付き撮影 frame object、preview/transaction、document transform、Canvas-only overlay／通常 export からの除外／save／reopen contract
- `VIEW-001`: zoom、box zoom、fit、1:1、pan、horizontal/vertical flip、accepted view ごとに独立した常設 native scrollbar と sticky dynamic range
- `VIEW-002`: ruler、guide/grid、snap、transparent view
- `SNAP-001`: view-targeted device/document座標変換、guide/grid優先順位、Ctrl一時解除を共有するproduction図形入力snap
- `VIEW-003`: color locator の座標/RGBA/selection sampling と magnified neighborhood 表示・編集
- `VIEW-004`: 複数文書 tab、同一文書 view、二分割 group、group 1／2 focus command、group/window 間の移動と複製
- `HIST-001`: exact Genesis source、transaction、Undo/Redo／inactive branch、document/editor savepoint、exact-path／UUID Revert、preview cancel
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
- `SUBPALETTE-001`: 外部 file/folder source、自然セル順、独立 viewport と常設 native scrollbar、常時 eyedropper、exact sample/register、icon/keyboard navigation、非同期 failure atomicity
- `COLOR-CHART-PREVIEW-001`: 同一base compositeからの非累積Color chart生成preview、頻度／差分summary、revision-bound Apply、exact-color名前継承、lock拒否、cursor継承、Cancel無変更、一回Undo／Redoとsave/reopen
- `COLOR-OUTPUT-QA-001`: BT.709係数とnominal code相当閾値を使う非適合表示の保守的Y′CbCr guard、visible straight-alpha composite、透明skip、fixed half-up、selection algebra、progress／Cancel／stale、Undo／Redo、replay、save/reopen

### Selection and editing

- `SEL-001`: rect/ellipse/lasso/polyline/trace/wand selection
- `SEL-002`: new/add/subtract/intersect/invert/expand/shrink/color selection
- `SEL-003`: 文書所有のstable-ID保存選択 mask、現在選択とのreplace/add/subtract、rename/delete、layer compositeからの分離
- `SEL-004`: raster range interpretation と rectangle／ellipse／trace construction options
- `CLIP-001`: typed clipboard、standard clipboard、document coordinate preservation
- `XFORM-001`: destructive mirror/rotate/size/resolution と非破壊 view transform の分離
- `XFORM-002`: floating selection move/scale/rotate、preview/commit/cancel
- `XFORM-003`: half-open boundsの五点anchorをpivotとするscale→時計回りrotate→絶対document X/Y配置、非累積preview、dialog／Canvas handle／rasterの同一結果

### Animation workflow

- `LT-001`: light table set、per-item transform/color/opacity、global opacity
- `LT-002`: reference-frame alignment、boundary/color sampling、edit image swap
- `LT-003`: 自然順の前／後／両方向Nセルを線形opacity stepと時系列z-orderでpreviewし、同一source UUIDの既存itemを保持したまま一つのUndo単位で一括登録する
- `SEQ-001`: cell sequence、共通 companion resolution、`BIND` rekey／binding rebase、target 固有 pair authority、Revert 中の runtime catalog／inactive recovery 保持、自然順の前後セル、欠番、thumbnail preview
- `SEQ-ENDPOINT-001`: application-wideの`Stop`／`Wrap`端点policy、empty／one／stopped／advanced／wrapped result、issue-time cell identity、motion loopとの分離、human-readable settings JSON persistence
- `SEQ-002`: motion check、FPS、loop、step、selection/light table option
- `PREF-001`: application／workspace 環境設定を一般と keyboard shortcut の二タブへ集約する dialog、keyboard shortcut page への直接 command、一般 page 内の意味別区分、候補 state、Apply／OK／Cancel 原子性、scope と再起動要否の表示
- `SHORT-001`: 全 production command を編集できる catalog と Windows／VS Code 慣例だけを割り当てる sparse な組み込み preset、inkpod 固有 command の既定未割当、保持されるユーザー preset、主／副の最大4-stroke shortcut、Global／Canvas／Timeline／Pane context、Execute／Hold／Toggle action、論理／物理照合、text-focus guard、context-aware prefix-free resolve、競合解決、検索／分類／keyboard 可視化、全 menu の collision-free mnemonic、`.inkshortcuts` current v3 import/export、永続化、reset

### Image processing and batch

- `FILTER-001`: sharpen/blur/Gaussian/invert/auto contrast
- `FILTER-002`: brightness/contrast、curve、levels、HSV、color balance
- `FILTER-PREVIEW-001`: filter／色調補正dialogの非累積live preview、bounded latest-wins更新、発行時target固定、OK一commit／Cancel完全復元
- `EFFECT-001`: gradient、airbrush、airbrush boundary effect、blur tool、stamp
- `ALPHA-001`: raster plane のalpha-only編集と通常color channelからの分離
- `BATCH-001`: fixed Input -> one-or-more ordered Operations -> fixed Output graph、draft編集、複数target color replace、`.inkbatch` v5
- `BATCH-002`: 公開四処理（exact color replace、move-to-color-plane、fill-protection masking、erase）とnative-depth一致
- `BATCH-003`: file/folder/issue-time-active input、folder/issue-time-active/new-tab output、bounded naming dry-run、progress/cancel、per-output atomicity
- `BATCH-004`: exact native-depth二セルpair抽出、複数行色置換、mask-aware output validation、staged result ownership、inline parameter hostとloaded-set編集
- `SCRIPT-001`: exact-current UTF-8 `.inkscript`／fragmentのclosed grammar、lossless CST、typed semantic AST、canonical emitter、schema registry、bounded diagnostic／resource contract
- `SCRIPT-002`: 全現行journal-replayable primitiveのclosed typed catalog、同一canonical executor、exact-source／rebound等価性、selector／assert／result／asset／portability／work formula、Continuous Fillの一seed一stepと1:N `editor_group`
- `SCRIPT-003`: authority-bound immutable plan、dry-run／progress／cancel／failure report、inputごとのstaged executionとexact-current `.inkpod` atomic install、save/reopen／Undo/Redo／cache-free replay／ID／savepoint保持
- `SCRIPT-004`: journalからのexact fragment export、dependency closure、strict bindingの明示rebind、Batch／History間のtransactional clipboard、source-preserving structured edit
- `SCRIPT-005`: `.inkbatch`現行productionを維持したprivate実装、M29C shadow parity、M34明示cutover、M35旧形式削除、承認済みperformance gateと最終hardening
