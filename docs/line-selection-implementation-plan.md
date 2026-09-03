# 線補正・選択支援の実装案（2026-09-03）

状態: **2026-09-03 ユーザー承認済み、実装・自動検証完了**。下記5項目をSPECへ反映した。
実装、成功／失敗の実行記録、実機・別構成の未検証範囲は[実装結果](line-selection-implementation.md)を参照。

ユーザーは監査報告の全条件を満たす実装を依頼した。開始commitは
`07c8dab252ec38218e3d09b7dcc272286c2ef17c`、branchは`main`。
前回監査の5ファイルの差分を保持した。この実装案の作成時点ではproduction変更は行っていなかった。

正本は[SPEC](../SPEC.md)、監査の根拠と当時の実行結果は
[監査報告](line-selection-audit.md)に残す。AGENTS §10.5に従い、
既存仕様とテストだけで一意に決められない利用者向け挙動を先に確認し、承認を得た。

## 承認された5項目

| 項目 | 推奨する契約 | 利用者への影響 |
| --- | --- | --- |
| 1. ゴミ取りの背景と連結性 | Binary/Grayscale主線はcoverage 0を背景とする。RGBA MainLineの初期値は透明＋白、Color/Rasterは透明のみ。RGBAの指定背景色を変更でき、色比較はnative-depth完全一致を初期値とする。除去跡は周囲の白/透明へ戻し、両方が接して一意に決められない成分は変更しない。前景の成分は8近傍、穴を判定する背景は4近傍とする | 不透明な白紙と透明画像を扱い分ける。斜めに接する線を一つの成分として保護する。従来の4近傍前景とは結果が変わる |
| 2. 局所範囲と成分の扱い | 成分サイズと画像端への接続は元plane全体で判定する。ゴミ取りは成分全体が操作帯と既存選択の共通部分に入った場合だけ変更する。線幅修正は元画像全体を参照して計算し、書込みだけを共通部分へ制限する。線つなぎは両端点と接続線全体が共通部分に収まる場合だけ適用する | 大線の帯内断片を小点と誤認しない。帯に一部分だけ触れた小点は残る。画像端につながる背景を穴として埋めない。帯外は一画素も変えない |
| 3. gapの単位・境界・候補 | 線つなぎとワンドで、gapを画素格子上の空白ステップ数とし「設定値以下」を対象にする。端点画素中心差が(dx,dy)なら空白ステップ数はmax(abs(dx),abs(dy))-1。水平・垂直・45度とも一画素欠けはgap=1。既接続は処理しない。線方向が向かい合う候補を優先し、一意に対応付けられない競合は変更しない | 現SPECの「未満」を「以下」へ改定する。端点中心のユークリッド距離とは別の単位。遠方、平行線、既存の別線を横切る接続を避ける。ワンドは探索前の仮想境界にだけ適用しsourceは不変 |
| 4. 太く／細くする量 | UIに「片側の増減量（document px）」を表示し、円形の近傍による膨張／収縮を使う。量1では軸平行断面が1/3/7→3/5/9、収縮は1/3/7→0/1/5となる | 全幅の増減は設定値の2倍。細線が消える場合もpreview、Cancel、Undoで確認・復元できる。新規ブラシ径やmaskの拡縮とは独立する |
| 5. 指定均一幅への変換 | 現SPECに独立したrasterモードを追加する。既存線の中心線を求め、指定した全幅で再構成する。帯外は不変。十分長い1/3/7幅の直線区間は指定幅へ揃える。分岐・閉路の接続関係を保つ | 輪郭やアンチエイリアスは変わり得る。交点・分岐の重なり部分まで一律の断面幅にはしない。8/16bitの深度と線色を保持し、単なる膨張／収縮では代用しない |

gapの向きの推定、同順位の扱い、偶数幅の配置、rounding等の細則も
実装時にSPECへ明記し、固定入力から一意になる契約テストを先に置く。
上表の操作意味を変更する判断が追加で必要になった場合は、その変更前に確認する。
指定背景色と既存の色許容差は混同せず、透明画素の未使用RGBを背景判定の差にしない。
異色小領域の単色周囲への置換はnative-depthで一致させる。

## 既存の依頼・仕様から確定できる内容

- 対象はゴミ取り3mode、恒久線つなぎ、既存線の太く/細く/均一化、ワンド、なぞり選択。
- ゴミ取りの最大サイズは画素数1–65,536、上限を含む。無効値を明示的に拒否する。
- 線編集は編集可能なactive MainLine/Color/Rasterを明示的に対象とし、通常彩色の主線保護と分ける。別planeは不変。
- pen/rectangle/polyline/lasso、既存selectionとの交差、全体適用、preview/Cancelを実製品経路へ接続する。
- ワンドの通常探索は4近傍のconnected same-color、既存のnative-depth許容差を維持する。探索後のmask closingを漏れ防止に流用しない。
- なぞり選択は帯そのものとし、Normalではsource内容を読まない。screen固定径はgesture開始zoomで正規化する。
- 成功だけを一transaction、一Undo単位として公開する。no-op、invalid、cancel、stale、overflow、failureで部分結果を公開しない。
- source画素、mask、revision/history/dirty/savepointを区別して検証する。Undo時に単調revisionの巻戻しは要求しない。
- 本体変更に伴うABI/schema/replay変更はAGENTSのcurrent-only規則に従い、最上位versionを更新する。互換readerやshimは追加しない。
- workload、counter、性能envelopeは変更しない。commit/push/PRは行わない。

## 実装の順序と責務

1. 上記判断をSPECへ反映し、監査の最小fixtureを新しい公開契約として固定する。
2. Rust imageに成分・背景判定と仮想境界の処理を責務別に実装する。計測範囲と変更可能範囲を分離し、boundedな資源とcancelを持たせる。
3. ゴミ取り、線つなぎ、太く/細く/均一化を型付きrequestとcanonical primitiveへ接続する。live、preview確定、Undo/Redo、replayで同じexecutorを使う。
4. 所有権と上限が明確なversioned C ABI、headerと文書を更新する。C++に画像アルゴリズムを置かない。
5. Win32のmenu、設定、captured target、gesture、task/previewを接続する。既存のcontroller/command ownershipを守り、未接続UIや常時成功stubは作らない。
6. なぞりの開始時座標変換・zoom固定、ワンドの階調主線意味、全selection演算をCore/ABI/実Canvasで確認する。
7. 保存・再読込、失敗注入、Windows product smokeの残存失敗を切り分け、必要な修正と検証を完了して状態文書を更新する。

## 受入れ証拠

| 対象 | 必須の具体的検証 |
| --- | --- |
| ゴミ取り | 1/3/4画素閾値、小点＋大線＋帯外、帯を横切る大線、白/透明、閉穴/画像端へ開く背景、異色小領域、全4形状、全体とselection、各pixel format/plane |
| 線つなぎ | 水平/垂直/斜め、gap直前/等値/直後、帯内外、既接続、候補なし、競合、平行/交差、設定幅、別plane、同入力同結果 |
| 線幅 | 1/3/7画素の増減断面、消失限界、端点/曲がり/分岐、帯内外、近傍別線、Binary/Grayscale/RGBA8/16、連続した異幅区間の均一化 |
| ワンド | 指定32×32囲みと(15,8)の切れ目、seed(12,12)と外部(4,12)、閉囲み全196画素、gap無効対照、複数gap/対角/細通路/画像端/tile、色/alpha/native-depth境界、4演算、source不変 |
| なぞり | 指定径5のL字全180画素と帯外2点、丸/角/pressure、疎sample、click/反復/自己交差/はみ出し、4演算、document固定/screen固定、途中zoom/pan/flip、DPI |
| 共通 | preview前後/Cancel、no-op/invalid/stale/overflow/failure、一Undo/Redo、journal replay、通常save/reopen、UI入口から実処理までの証拠 |

監査時の`observation_`は当時の挙動の記録であり、新契約の正解にはしない。
承認された意味変更に該当するものは、その変更理由を記録して新契約テストへ更新する。
既知不具合の再現assertionを弱めて成功扱いにしない。

実行はno-profile PowerShell。Rustのfmt/clippy/workspace test/quick bench/rustdoc、
実在するCMake presetによるconfigure/build/CTestを実行し、exit codeを記録する。
Coreの成功をWindows/物理入力の成功へ読み替えず、実機・構成ごとの証拠を分ける。
