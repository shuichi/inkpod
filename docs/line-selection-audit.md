# 線補正・選択支援の実装監査（2026-09-03）

「既に実装されている」は一部だけ正しい。通常の連結同色ワンドと帯状のなぞり選択は Core の画素集合で確認できた。一方、現行コードには恒久的な線つなぎ、描画済み raster 線の太く／細く／均一化の経路がない。ゴミ取りには透明背景で動く処理があるが、白背景と範囲境界に問題があり、主線への通常 UI 入口も制限される。ワンドの gap close は切れ目からの漏れを防がない。

## 対象と証拠の扱い

- 開始 branch: `main`。commit: `07c8dab252ec38218e3d09b7dcc272286c2ef17c`。
- 開始時 `git status --short` と `git diff --stat` は空。既存ユーザー差分なし。
- 正本は [SPEC](../SPEC.md)、状態主張は [compatibility](compatibility.md) と [implementation-status](implementation-status.md)。旧製品の画像・マニュアル・第三者 fixture は使用していない。
- 要件: `PAINT-003`、`SEL-001`、`SEL-004`。関連する `SEL-002/003`、`HIST-001`、`DOC-002/003`、`IO-001/002`、`VIEW-001`、`CLIP-001`、仮想境界との区別に `FILL-002/004` を追跡した。関連要件全体の再認定ではない。
- production algorithm、UI機能、ABI、保存形式、SPEC は変更しない。追加は公開APIテスト、private product smoke、監査報告と状態訂正だけ。commit/push/PR は行わない。
- `contract_` は明記された仕様、`expectation_` は今回の利用者期待、`observation_` は仕様未確定の実装挙動。特に「成分の測定範囲」と「ワンドgap値の距離定義」は、今回のテストで新たな製品仕様にしない。
- 以下のソースリンクの `#L` は行番号。Codexの最終報告には直接開ける絶対パスリンクも付す。

## 比較表

| 機能 | 利用者期待 | 現行仕様・要件 | 実装された UI 操作 | 観測結果 | 仕様判定 | 期待判定 | 根拠・テスト・実行結果 | 未検証・制限 |
|---|---|---|---|---|---|---|---|---|
| ゴミ取り | 帯内の小成分だけ除去。大線と帯外を保護。穴埋め・異色置換 | SPEC 352–357、PAINT-003: pen/rect/polyline/lasso、3 mode、局所/全体、preview | ゴミ取りの範囲・最大画素・mode・preview設定、Canvas gesture→task。通常入口はColor側のみ。範囲なしは選択/全体 | RGBA8/16の1・3画素を除去、4画素と帯外は保持。白背景小点と白穴は未処理。28画素線の帯内3画素を切断。masked open背景も穴扱い | 部分実装。背景mode、主線UI、欠落機能を含むPAINT-003のVerifiedは不適切 | 一部のみ | [Core](../rust/inkpod-core/src/effects/tools.rs#L328)、[dust](../rust/inkpod-image/src/edit/dust.rs#L5)、追加 `contract_dust_*`、`expectation_dust_*`: 通常成功、4再現失敗 | Color/Raster全UI設定の実ドラッグ網羅、主線入口追加、背景定義・全成分サイズの仕様判断 |
| 線つなぎ | 条件内の端点間へ恒久線。指定間隔以下 | SPEC 359–363、PAINT-003: **gap未満**、距離/角度/planeの決定的評価、一Undo | 対応command/gesture/ABIを発見できない | 対応executor/端点探索も現存しない。fill gap closeや自由描線は代替に数えない | 未実装 | 未達。加えて「以下」と「未満」に仕様差 | [tool enum](../rust/inkpod-core/src/api.rs#L608)、[primitive catalog](../rust/inkpod-core/src/primitive/catalog.rs#L1)、[resource IDs](../apps/windows/app/resource.h#L1)、全関連ソース横断検索 | d<g/d=g/d>g、距離単位、水平/垂直/斜め、競合、平行/交差、幅、plane保護、determinism、Undoの実測入口なし |
| 線幅を太くする | 既存線の帯内だけ指定量増幅 | SPEC 365–368、PAINT-003: morphologyで太く | 対応UI/ABI/Coreなし | 描画径と新規図形outline幅は存在するが既存線修正ではない | 未実装 | 未達 | [PaintTool](../rust/inkpod-core/src/api.rs#L608)、[図形width](../rust/inkpod-core/src/geometry.rs#L138)、catalog検索 | 幅1/3/7の前後断面、半径/直径/総増加量、端点/角/分岐、局所性、formatすべて入口なし |
| 線幅を細くする | 既存線の帯内だけ指定量減幅 | 同上: 太くと別mode | 対応UI/ABI/Coreなし | selection縮小はmask操作であり、線の収縮ではない | 未実装 | 未達 | [selection morphology](../rust/inkpod-core/src/selection/mask.rs#L66)、catalog検索 | 幅1/3/7、消失限界、端点/角/分岐、局所性、alpha等を実測できない |
| 線幅を均一にする | 幅1/3/7の連続線を指定一定幅へ | **現行仕様に含まれない**。PAINT-003は増減のみ | rasterにもvectorにも実動経路を発見できない | 「太く/細く/倍率/一定」という未参照localization文字列はあるが処理ではない | 要求なし。未実装自体を仕様違反に数えない | 未達 | [未参照文字列Text0360](../apps/windows/ui/localization_catalog.json#L3760)、[geometry](../rust/inkpod-core/src/geometry.rs#L138) | 連続線の遷移部から離れた断面比較を呼ぶAPIなし。新仕様の合意が必要 |
| ワンドの連結同色選択 | seedから連続する同色だけ選択 | SPEC 473、SEL-001、SEL-002: connected/tolerance、選択4演算 | 選択→ワンド、許容差/gap設定、クリック | 閉囲み内部196画素、外部除外。同色でも非連結は除外。RGBA8/16、alpha、16bit下位1差、4演算、Undo/replay/save成功 | 測定したCore契約は適合 | 測定範囲で達成 | [探索](../rust/inkpod-core/src/selection/geometry.rs#L101)、[比較](../rust/inkpod-core/src/selection/mask.rs#L225)、`contract_wand_closed_*`/`contract_wand_tolerance_*`等成功 | 階調主線の基本色/coverageの製品経路、今回の厳密fixtureでのWindowsワンドクリックは未確認 |
| ワンドの隙間漏れ防止 | 短い線切れを探索の仮想境界とし画像は不変 | SPEC 473のgap close。gap距離・等値・境界画素の厳密定義は未規定 | 同上、gapはCoreで0–64、UIは入力欄あり | 32×32の1画素切れ、gap=1で外部(4,12)も選択。900画素。画像は完全不変 | gap closeの意味が未規定で完全な適否は一意に判定できない。実装されたmask closingを漏れ防止の証拠にはできない | **未達** | [探索後の膨張/収縮](../rust/inkpod-core/src/selection/geometry.rs#L138)、`expectation_wand_gap_close_prevents_escape_through_one_pixel_break`失敗 | 現在のmask closingと期待する探索境界は別。仕様距離未確定、Windowsでの同fixture確認なし |
| なぞり選択 | 開いた軌跡の帯だけ。線/背景を含み画像不変 | SPEC 469–478、SEL-001/004: 丸/角、径、pressure、screen固定、Normal | 選択→なぞり、形状/径/pressure/screen設定、Canvas begin/append/end/cancel | Coreと英日Windows追加smokeで指定L字の全180画素が一致。丸/角、疎sample、クリック/反復/自己交差/はみ出し、圧力断面、4演算はCore成功。Windowsで反転、preview/Cancel、一Undo/Redo、source不変も成功 | 測定範囲で適合 | 基本の帯操作は達成 | [TraceBrush](../rust/inkpod-core/src/selection/geometry.rs#L88)、[FFI](../rust/inkpod-ffi/src/document_edit/selection_clipboard.rs#L229)、`contract_trace_open_l_exact_band_preserves_source_and_history`成功、[Canvas smoke](../apps/windows/app/app_smoke.cpp#L7751)の成功checkpoint | gesture途中zoom変更、screen固定時開始zoom、実pen/touch圧力、複数DPI/別view・入力通知batchの全組合せ |

## UIから処理までの追跡

**ゴミ取り:** `IDM_EFFECT_DUST` → [command_state](../apps/windows/ui/command_state.cpp#L468) の `color_plane_active` gate → [command handler](../apps/windows/ui/main_window_runtime.cpp#L20260) → `SelectCanvasEffect` → [PrepareCanvasEffectEditor](../apps/windows/ui/main_window_runtime.cpp#L6437) → Canvasで捕捉したtarget/options/samples → [QueueDustRemoval](../apps/windows/ui/main_window_runtime.cpp#L6755) → `inkpod_core_dust_preview_begin` / `inkpod_core_dust_remove` → [FFI](../rust/inkpod-ffi/src/effects/dust.rs#L9) → `apply_dust_removal_for_view` / `begin_dust_preview_for_view` → [Coreのshape∩selection](../rust/inkpod-core/src/effects/tools.rs#L384) → image dust。

全体/pen/rectangle/polyline/lassoは同じ設定の5候補。CoreのTraceは丸い帯で、Dust用recordには丸/角の選択fieldがない。document selectionが非空なら交差し、未指定かつselection空ならplane全体。既存の「filter全体」と「tool局所」の計算は同じDust処理へ入る。独立した線補正専用filter入口はない。UI初期範囲は全体であり、局所動作にはpen等を選ぶ必要がある。

最大サイズの実装単位は成分の**画素数**、許容範囲は1–65,536、上限を含む。連結性は4近傍。除去候補は色によらずalpha非ゼロの連結成分、穴候補はalphaゼロの連結成分、異色置換はnative RGBA完全一致の成分である。白を背景とみなす判定はなく、他の不透明色と同じ前景側に入る。サイズ測定は画像全体の成分ではなく、初めから操作mask内に制限される。この測定範囲と背景色の具体的指定方法はSPECにないため、観測と仕様判断を分ける。

CoreはeditableなMainLine/Color/Rasterの**straight RGBA8/16だけ**受理し、Binary/Grayscale8/16を拒否する ([editable_rgba_plane](../rust/inkpod-core/src/effects/helpers.rs#L69))。RGBA MainLineへの公開APIによる明示線編集は成功する。通常の彩色主線保護と、明示線編集の可否を混同していない。対して通常UIはMainLineでDustがdisabledとなり、主線に対する縦切りは成立していない。

**ワンド・trace:** [menu設定](../apps/windows/ui/main_window_runtime.cpp#L19243) → `EditorSelectionOptions` → captured targetとCanvas samples → [ApplySelectionGesture](../apps/windows/ui/main_window_runtime.cpp#L14604) → [SelectionController](../apps/windows/ui/tools/selection_controller.cpp#L1) → [selection FFI](../rust/inkpod-ffi/src/document_edit/selection_clipboard.rs#L1) → [Core canonical transaction](../rust/inkpod-core/src/selection/operations.rs#L68) → [mask generator](../rust/inkpod-core/src/selection/geometry.rs#L7)。C++はCanvas document boundsからdevice座標を逆変換し、panとflipを戻す。OS DPIを追加乗算するコードはない。CoreのNormal解釈はsource内容を読まず候補帯を採用する ([image interpretation](../rust/inkpod-image/src/selection.rs#L34))。

線つなぎ/線幅はresource IDs、UI tool enum/handler、public header、Core公開export、primitive catalog/executor、image crateとtestsを `gap connect / endpoint / line width / thicken / morphology / 線つなぎ / 線幅修正` 等で照合した。見つかったmorphologyはselection maskとfillの境界処理だけ。`PaintTool`はPencil/Brush/Eraserであり、過去の状態文書が言うcleanup/width primitiveを現存コードは裏付けない。架空API、stub、代替ブラシテストは追加していない。

## 小さな再現と修正方針（本体は未変更）

1. **仕様との差: 白背景の小点除去。** 32×32をopaque白、(16,16)だけ黒、最大1、全体RemoveForeground。期待変更集合 `{(16,16)}`、実際 `{}`。alpha>0の全1024画素を一成分として数えるため。白紙上の背景判定を透明判定から分ける方針が必要。任意の「背景色」をどう指定するかは仕様判断事項。
2. **仕様との差: 背景小穴。** 8×8黒に(4,4)だけopaque白、最大1、FillTransparentHoles。期待黒、実際白のまま。透明の1画素穴は周囲色へ埋まり、画像角の透明点は保持される。modeがalpha=0しか候補にしないため。
3. **利用者期待との差・仕様未規定: 成分サイズ。** y=16, x=2..29の28画素線に、矩形x=15..17,y=15..17、最大3。実際の変更集合 `{(15,16),(16,16),(17,16)}`。探索時からmask外を除外するため、全体の線ではなく帯内断片を3画素として除去する。測定を画像全体の成分にするか、境界横断成分を除外するかを決める必要がある。単に閾値を下げる修正ではない。
4. **利用者期待との差・仕様未規定: 切り取られた開背景。** 8×8黒にx=4,y=0..4の透明通路。操作範囲を(4,4)の1画素だけにすると、画像端につながる背景が黒へ埋まる。mask内探索では画像端への接続を観測できない。穴の閉鎖性はsource全体で判定し、変更可能範囲と分ける方針。
5. **利用者期待との差: ワンド漏れ。** 指定の外周x/y=8..23、seed(12,12)、上辺(15,8)を背景へ戻す。gap=0は白965画素を選択し、対照成立。gap=1は900画素で外部(4,12)も選択。探索後にmaskを四近傍で膨張→収縮しても、先に漏れた外部を内部と区別できない。source画素は不変。探索前の仮想境界構築とseed側探索に分離し、距離・等値・競合規則を仕様化する方針。gap=2に対して切れ目1/2/3画素、二箇所の切れ目も全て漏れた。これを端点中心間距離の「未満/以下」テストに読み替えてはいけない。

異色置換はnative-depthの完全同色成分を四近傍で集め、接する周辺画素のRGBA平均（整数half-up）へ置換する。単色周囲の赤い1画素は正確に `[20,40,60,255]` へ置換された。周囲に複数色がある場合の平均という選択は実装観測であり、「必ず既存の一色を選ぶ」仕様ではない。

線つなぎは空白画素数と端点中心間距離のいずれをgapにするかも実測不能。SPECの不等号は `d < g`、利用者期待は `d <= g`。実装時にはこの差を先に決め、水平/垂直/斜め、競合、平行線、交差付近、帯外、別planeと一Undoを独立に固定する必要がある。線幅増減はstructuring elementと量の単位から定義する必要があり、1/3/7画素の測定結果を捏造していない。均一化は独立の新規要件である。

## テストの評価と追加証拠

既存Rust workspaceは変更前に全件成功した。しかし、成功だけでは今回の操作意味を証明しない。

- [paint_003_dust_modes_preview_bounds_and_cancel_are_atomic](../rust/inkpod-image/tests/unit/edit.rs#L221) は5×5の透明背景の点/穴/異色を直接image APIへ渡す。各中心画素とcancelは見るが、白背景、3画素閾値、帯内断片、大線、帯外の全差分は見ない。
- [full_effect_gestures_dust_and_alpha_are_atomic](../rust/inkpod-core/tests/contracts/effects.rs#L277) は名前にdustがあるが、実際に呼ぶのはairbrush、blur、alpha gradient。Dust成功の証拠にはならない。同fileの `worker_cancel_and_dust_never_commit_partial_results` は本当にDustを呼ぶがcancel経路。
- [acceptance_selection_authoring_tools](../rust/inkpod-core/tests/contracts/document_selection.rs#L599) のwandは空の12×12 Binary planeをgap=0で全選択し、boundsを見るだけ。漏れ防止を検証していない。
- [sel_004_geometry_and_trace_options_share_one_mask_path](../rust/inkpod-core/tests/contracts/document_selection.rs#L879) は一地点で丸5画素、角9画素、pressure+screen1画素を確認する。開いたL字の帯と外接矩形の区別は従来未検証。
- [ABI smoke](../tests/abi_smoke.cpp#L1561) はDust preview/task成功とcancelを確認するが、白背景や変更集合を検査しない。Windowsの `RunImageEffectsSmoke` はInvert previewとgradientを呼び、Dust局所gestureを呼ばない。
- 既存Windows trace previewはactive/point_count/stroke_width、wandはeditor shape/locator boundsを主に見る。Core成功だけでWindowsの各条件をVerifiedとはしない。

追加 [line_selection_audit.rs](../rust/inkpod-core/tests/line_selection_audit.rs#L1) は23テスト。18成功、5失敗（前節の2つの仕様再現と3つの利用者期待再現）。ignoreなし。期待値は手で指定した画素集合、軸平行stripと半径2.5の円盤の整数条件、RGBA差から決めた。

選択マスク取得は、公開 `Core::clone` 上の不透明Color probeと `copy_selection` の座標を使い、live Coreへ変更しない。source色を選択色と誤認するsnapshot読取を避ける。白/透明/選択表示に似た色で読取自身を検証した。途中の試作はflattened snapshotを生maskと誤認しており、その時点の失敗は製品不具合に数えていない。fixture/reader修正後にも上記5件が残った。

成功した主な境界:

- Dust最大3で1/3を除去、4/大線/帯外不変、RGBA8/16、選択との交差、全体、4形状、no-op、0/65537拒否、cancel、preview Apply/Cancel、locked/不存在plane、1回Undo/Redo、replay、通常save/reopen。
- Wand閉囲み全196画素、開囲み全965画素、4連結の対角非接続、同色の非連結領域、幅1の通路、x=63/64 tile境界と最終x=65。許容差は全RGBA成分の最大差を16bit尺度で比較し `<= tolerance`。8bit一段差は257、16bitでは下位1差を保持。alpha差も同じ尺度。4演算、no-op、Undo/Redo/replay/save。
- Trace丸・径5のL字全集合。選択(8,16)/(16,24)、非選択(16,16)/(28,4)。線と透明背景双方を選び、sourceの両plane不変。クリック、同一点反復、疎sample、自己交差、画像外、丸/角、document固定でzoom option1/2/4同一、screen固定10÷zoom2=径5、pressure0.5×径10=径5。疎な可変pressureの断面はx=8/16/24で1/3/5画素。4演算、no-op、invalid径/空列/範囲外seed/gap65、stale target、Undo/Redo/replay/save。

未実行の条件を明確に残す。Core dustの一般的なallocation failure、revision/ID overflow、同期call中に外部からrevisionを進めるfault injection、階調MainLineワンドの基本色/coverageとの製品同値、全plane種別の全UI組合せは今回の新規fixtureでは未検証。既存のgeneric transaction/FFI/architecture testsの成功を個々の機能のfault証拠へ転用していない。selectionはdocument-ownedなので成功時revision/history/dirtyが進む。Undoでは画素/maskの復元と単調revisionを分けた。

## Windows product smoke

新規 [RunLineSelectionAuditSmoke](../apps/windows/app/app_smoke.cpp#L7751) を既存 `--smoke-test` 内へ追加。32×32の独立した一時sessionで、menuによるTrace選択、document径5、Normal、無pressure、実Canvasのdevice座標begin/append/endを使う。設定値は既存smoke用editor helperを通し、入力は既存 `SubmitCanvasStrokeEvent` へ投入する。実際の設定dialogやOSペンイベントの全操作を再現したものではない。期待maskは全1024画素を検査し、反転、source両checksum、一revision、一Undo/Redo、preview/Cancelを別々に判定する。sessionを閉じ元viewへ戻す。生snapshotのalphaを読むのは**source両planeが空であるこのfixtureだけ**である。

初回の追加smokeは英日とも23106、追加診断は23107だった。原因はfixtureが非表示Canvasの未発行snapshotから `bounds=0,0..0,0` を読み、全sampleを(0,0)へ潰していたこと。非表示Canvasのsnapshotを停止する現行契約に沿い、一時workspaceを可視化、既存のpresentation待機を通し、正のboundsを検査してから操作するよう**テストだけ**修正した。定規もfixture内で非表示にし、定規dragと選択入力を分離した。表示状態は終了時に復元する。修正後の英語・日本語経路で全180画素のL字、反転、source不変、一revision、一Undo/Redo、preview/Cancelが成功した。初期fixtureの失敗を製品不具合には数えない。専用の実ペン操作、物理DPIの切替は未実施。

変更前のfull CTestは47/49成功。英語product smokeはコード748で失敗。748は複数箇所に再利用されているため、ログだけから一箇所へ断定しない。日本語product smokeは長時間進まず、監査が開始したPID/親ctest/command lineを読み取りで再照合して、そのsmoke子だけ停止した。GUI観測で二つのinkpod workspaceは存在し、表示中の確認dialogは見えなかった。原因は未確定。途中停止を成功にしない。

確定版のproduct smoke全体は**英語237.28秒で311、日本語360.20秒でtimeout**、CTest exit 8だった。どちらも追加監査の成功checkpointを通過している。311は既存 [RunDocumentEditingSmokeのSaveToPath/OpenFromPath検査](../apps/windows/app/app_smoke.cpp#L8098) であり、どちらの呼出が失敗したかは未切分。日本語は後続で `editor split canvas failed canvas=1 sink=1 registered=0 view=0 workspace=1` を出した後にtimeoutとなった。これらをbaselineの748と同じ原因と扱わず、追加監査との因果も断定しない。全体成功とは報告せず、製品全体の再検証課題として残す。

## 実行記録

全shell呼出は `login:false`。MSVC x64環境は既存scriptと同じ `vswhere`→`VsDevCmd.bat -no_logo -arch=x64 -host_arch=x64` で初期化し、PowerShell profileを読まない。preset実在確認済み。Release/ARM64は今回未実行。

| コマンド/操作 | exit | 結果・ログ（repository内） |
|---|---:|---|
| `git status --short`, `git branch --show-current`, `git rev-parse HEAD`, `git diff --stat` | 0 | 開始状態は前記。探索時にPowerShellで展開されないpath globを渡した検索はexit1になり、directory指定で再検索した。未発見判定にその失敗を使っていない |
| `cargo test --workspace --all-features`（変更前） | 0 | `build/line-selection-audit/baseline-rust-tests.log` |
| `cmake --preset windows-x64-debug`（sandbox） | 1 | Ninja起動が `operation not permitted`。`configure.log` |
| 同configure（昇格再実行） | 0 | `configure-elevated.log`。自動承認拒否ではなくsandbox内の実行制約だった |
| `cmake --build --preset windows-x64-debug`（変更前） | 0 | `build.log`。MSVC/static CRT/packagingまで成功 |
| `ctest --preset windows-x64-debug`（変更前） | 8 | `baseline-ctest.log`。47/49成功、英語748、日本語停止。ABI、C11/C++20、static gates、renderer、CoreHost等は成功 |
| `cargo test --package inkpod-core --test line_selection_audit -- --nocapture`（fixture作成途中） | 101 | API引数/型のcompile修正、mask読取修正を実施。途中ログ `added-tests-first.log`, `added-tests.log`, `added-tests-corrected-reader.log`, `added-tests-mask-export.log` は製品判定に使わない |
| 同追加test（完成fixture） | 101 | `added-tests-final.log`、18成功/5失敗/0 ignored。前節の5最小再現 |
| `cargo test --workspace --all-features --no-fail-fast` | 101 | `final-workspace-tests.log`。全targetを継続実行してbaselineとの差を区別。失敗targetは追加auditのみ |
| `cargo fmt --all`（追加test整形） | 0 | production Rustへの差分なし |
| `cargo fmt --check` | 0 | `fmt-final.log` |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | `clippy-final.log` |
| `cargo bench --package inkpod-core --bench core_workflows -- --quick` | 0 | `bench.log`。workload/counter/envelope未変更 |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc --package inkpod-core --all-features --no-deps` | 0 | `rustdoc.log` |
| `cmake --build --preset windows-x64-debug`（追加smoke） | 0 | `build-with-audit-smoke.log`, `build-smoke-diagnostics.log` |
| `ctest --preset windows-x64-debug -R '^inkpod_windows_smoke' --timeout 240 -V`（追加smoke初回） | 8 | `audit-product-smokes.log`。英25.28秒/日28.36秒、両方23106 |
| `ctest --preset windows-x64-debug -R '^inkpod_windows_smoke$' --timeout 240 -V`（診断） | 8 | `audit-smoke-diagnostics.log`。前記active=0等の個別値 |
| `cmake --build --preset windows-x64-debug`（追加診断2回） | 0 | `build-smoke-final.log`, `build-smoke-ruler.log` |
| `ctest --preset windows-x64-debug -R '^inkpod_windows_smoke' --timeout 240 -V`（commit診断） | 8 | `audit-product-final.log`。英日とも23107、合計53.81秒 |
| `ctest --preset windows-x64-debug -R '^inkpod_windows_smoke$' --timeout 240 -V`（bounds診断） | 8 | `audit-smoke-ruler.log`。43.11秒、23107、boundsが全て0。fixture修正の根拠 |
| `cmake --build --preset windows-x64-debug`（可視fixture確定版） | 0 | `build-smoke-visible.log` |
| `ctest --preset windows-x64-debug -R '^inkpod_windows_smoke' --timeout 360 -V`（可視fixture確定版） | 8 | `audit-product-visible.log`。追加監査は英日成功、全体は英語311/日本語timeout、合計597.50秒。上記の通りbaseline失敗とは区別 |
| `ctest --preset windows-x64-debug -R 'inkpod_windows_(command_routes\|command_state_ownership\|frontend_boundaries\|command_context_boundaries\|filter_preview_boundaries\|application_host_boundaries\|localization_catalog)$'` | 0 | `final-static-ctest.log`。関連static gates 7/7成功、21.97秒 |
| `cargo fmt --check`, `git diff --check`（引渡し確認） | 0 / 0 | `fmt-delivery.log`, `diff-check.log`。LF→CRLFのGit注意のみ。変更は監査関連5ファイル |
| Computer Use `list_windows/get_window_state` | 成功 | 最初のlistはtimeout、reset後の再試行成功。日本語smokeの二workspaceを読取のみ。入力操作なし |

生ログは `build/line-selection-audit/`、shellごとの終了値は `rust-exits.txt` / `windows-exits.txt`。これらは生成物でありgit管理外。報告本文が永続的な要約。no-profile実行で一部のcargoは `could not canonicalize path C:\Users\shuichi` を出したが、各実終了値は表の通りであり、それを成功の代わりにはしていない。

## 状態文書の訂正

`PAINT-001/003` 一括VerifiedからPAINT-003を分離してIn progressへ変更。Pencil/Brush/Eraserの既存証拠は維持し、存在しないcleanup/width primitiveの主張を削除する。SEL-001もSEL-002/003から分離してIn progressへ変更し、connected selectionとgap closeを区別する。SEL-004の既存証拠は維持し、今回のCore全集合とWindows L字の証拠を追加するが、gesture途中のzoom変更や実pen/DPIは未検証と明記する。過去の広範なVerified文字列を訂正しても、過去の個々のテスト成功を否定するものではない。

## 不一致・未検証の分類

- **仕様との差:** PAINT-003の恒久線つなぎと既存raster線の太く/細くは未実装。背景と透明を別に挙げたDustの記述に対し、白背景小点/小穴の処理が欠ける。任意の背景色をどう決めるかは追加の仕様判断を要する。
- **利用者期待との差:** 帯内断片のサイズ測定で大線を切る、画像端へ開いた背景をmask内で穴扱いする、wandの探索後closingでは漏れを防げない。線つなぎの「以下」は現SPECの「未満」と異なり、線幅均一化は現SPEC外。
- **UI未接続:** DustのRGBA MainLine Core処理に通常menuから到達できない。線つなぎ/線幅修正はUIだけでなく公開処理自体がない。Color上のDustはgesture/ABI/taskまで静的に接続を追えたが、今回のWindows smokeでColor局所除去の画素集合までは検証していない。
- **テスト不足:** 過去のVerifiedは白背景、全成分対mask内断片、wand漏れ、L字全集合を裏付けていなかった。追加テストで上記を明らかにした。階調MainLineのwand意味、全UI mode/plane、gesture途中zoom変更、個別機能へのallocation/overflow/stale-revision fault injectionは残る。
- **環境による未検証:** Windows Release/ARM64、非Windows実行、物理pen/touch、複数物理DPIは未実施。baseline英語748と日本語停滞は原因未確定の既存実行失敗であり、環境だけが原因とは断定しない。今回のCanvas smoke成功をproduct smoke全体成功へ拡張しない。

修正を実装する場合、成分境界/背景色/gap距離など未規定部分の判断が先に必要。primitive semantics変更時のformat/replay version更新はAGENTSの規則に従うが、本監査ではいずれも変更していない。
