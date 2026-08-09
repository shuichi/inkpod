# PaintMan 機能ギャップ解消セッション計画

## 1. 目的と使い方

この計画は `data/paintman-functional-gap-analysis.md` が特定した 18 件の機能ギャップを、`SPEC.md`、`AGENTS.md`、現行実装の契約に従って解消するための実装順序である。外見や旧製品固有形式の模写ではなく、カット準備から単一セル編集、複数プレーン作業、連続セル、検査、制作指示、バッチまでの意味上のワークフローを完成させる。

各開発セッションでは、依存済みのマイルストーンを一つだけ選び、そのマイルストーンのテスト、production 経路、必要な文書更新まで完了させる。次のマイルストーンへ着手しない。マイルストーンが大きく見える場合も、完了条件を削って部分実装を完成扱いにせず、このファイルを更新してより小さな縦切りへ分割してから実装する。

`data/paintman-functional-gap-analysis.md` は本計画作成時の調査資料である。M00 完了後の通常実装では、正規化済みの `SPEC.md` を機能仕様の正本とし、旧製品マニュアルや画像へ戻らない。

## 2. 計画時点の基準

- 計画作成時の worktree は clean。
- native `.inkpod` は current-only V9、replay epoch 6。
- C ABI は v5。
- `docs/implementation-status.md` の直近記録は Rust 333 tests、Windows ARM64 Debug CTest 30/30。ただしギャップ分析は一部の aggregate requirement をより厳密に分解し、18 件を未完と判定している。
- `docs/compatibility.md` では `PAINT-001/002/003`、`SEL-001/002`、`CLIP-001`、`ADJUST-001`、`BATCH-002` 等が aggregate として `Verified` になっている。M00 で検証済み部分を失わず、残作業を別 requirement として追跡可能にする。
- 現在の主要な実装場所は `rust/inkpod-core/src/document`、`animation`、`selection`、`paint.rs`、`stroke.rs`、`effects`、`batch`、`primitive`、`rust/inkpod-image/src/edit`、`rust/inkpod-format/src/native`、`rust/inkpod-ffi/src`、`include/inkpod/core_ffi.h`、`apps/windows/app`、`apps/windows/ui`、`apps/windows/renderer` である。

## 3. 全セッション共通の規律

### 開始条件

1. `git status --short` でユーザー差分を確認し、対象外の変更へ触れない。
2. このファイルで依存マイルストーンが完了済みか確認する。
3. `SPEC.md` の該当 requirement、`docs/compatibility.md`、対象コードと既存 test を読む。
4. 公開契約を production code より先に test で固定する。
5. 仕様だけでは安全に決められない選択が残る場合は、実装を推測せず、選択肢、影響、解除条件を提示して判断を得る。

### 実装境界

- production document mutation は既存の canonical primitive executor、transaction、Undo/Redo、replay 経路へ統合する。直接 field を変更する別経路を作らない。
- success、no-op、invalid、cancel、stale、overflow、失敗を区別し、失敗系で revision、`StateId`、history、journal、dirty、persistent ID、savepoint を進めない。
- schema を変更するマイルストーンは、その時点の top-level native format version を一つ上げ、旧 version を拒否し、`PROCEDURE_FORMAT_VERSION`、必要なら replay epoch、`docs/file-format.md`、fixture/test 名を同じセッションで更新する。V9 固定名を新しい current version に機械的に残さない。
- primitive semantics が既存 replay の結果を変える場合は、format version だけでなく replay epoch も同じセッションで更新する。
- C ABI の record、enum、export、ownership 契約を変更する場合は、その時点の ABI version、Rust/C header 宣言、C11/C++20 include、export catalog、`docs/ffi.md`、negative test を同じセッションで更新する。
- 新しい UI は menu から到達可能にし、command catalog、enable/checked state、shortcut、resource、accessibility name、発行時 `CommandContext`、stale target rejection を含める。未接続 button や常時成功 stub を残さない。
- `main.cpp`、`lib.rs`、`mod.rs` に production logic を置かず、責務名を持つ module/controller へ分ける。
- 完了時に requirement status/evidence が変わった場合だけ `docs/compatibility.md` を更新し、現在状態、既知差分、代表検証が変わった場合だけ `docs/implementation-status.md` を置き換える。

### マイルストーン共通の完了条件

- 指定された利用者向け縦切り、または明記された Core-only/contract-only scope が完了している。
- 対象に応じて success、no-op、invalid、cancel、stale、overflow、Undo/Redo、branch cut、save/reopen、malformed input を public API から検証している。
- Windows を含むマイルストーンは実製品の menu/pane/Canvas から FFI/Core までを GUI smoke または同等の native integration test で確認している。
- format/ABI の ownership、lifetime、thread、size/bounds、current-version-only 契約がコード、header、文書で一致している。
- 新しい warning、ignored test、過大 tolerance、無条件成功 placeholder がない。
- `git diff --check` が成功し、実行した検証と未実行理由をセッション最終報告へ記録している。

### 検証セット

Rust/Core を変更するセッションでは原則として次を実行する。

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo bench --package inkpod-core --bench core_workflows -- --quick
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --package inkpod-core --all-features --no-deps
Remove-Item Env:RUSTDOCFLAGS
```

Windows/ABI を変更するセッションでは、上記に加えて現行 host で少なくとも次を実行する。

```powershell
cmake --preset windows-x64-debug
cmake --build --preset windows-x64-debug
ctest --preset windows-x64-debug
```

format/decoder を変更する場合は current-version round-trip、旧 version 拒否、corrupted corpus、staged open、save failure を追加検証する。hot path、tile、snapshot、stroke、selection、Light Table、batch を変更する場合は意味 counter と quick benchmark を確認し、性能契約や envelope を変更しない。full benchmark や ARM64/x64 Release、MSIX/ZIP は性能・配布境界を変えるマイルストーンと最終監査で実行する。実行環境がない検証は省略せず未検証として明記する。

## 4. 依存順序

主要な依存は次の通りである。

```text
M00 -> M01 -> M02 -> M03 -> M04 -> M05 -> M06
M00 -> M07 -> M08 -> M09
M00 -> M10 -> M15 -> M16
          \-> M17
M00 -> M11 -> M12
          \-> M13 -> M14
M00 -> M18 -> M19
M00 -> M20
M00 -> M21
M00 -> M22 -> M23 -> M24
M04 -> M25 -> M26
M04 -> M27 -> M28
M03 + M09 + M14 -> M29 -> M30
M06 + M09 + M12 + M14 + M16 + M17 + M19 + M20 + M21 + M24 + M26 + M28 + M30 -> M31 -> M32
```

独立枝は順序を入れ替えられるが、一つのセッションで並行実装しない。schema/ABI version の番号はこの順序に固定せず、各セッション開始時の current version から増分する。

## 5. マイルストーン

### Phase A — 仕様追跡、カット、セルバンク

#### M00 — ギャップ別 requirement と status の正規化

- **対象:** 全 gap、contract-only。
- **実装:** `SPEC.md` の aggregate requirement を、検証済み部分を保ったまま gap 単位へ分割する。少なくとも cut、cell bank、新規セル、multi-target/clipboard、raster stroke option、raster geometry、raster morphology、advanced raster selection、QC、alpha tool target、capture frame、vanishing guide、text/annotation、Light Table source/automation、switch policy、production metadata、batch topology variant を個別追跡できる ID にする。
- **文書:** `docs/compatibility.md` を新 ID と一対一にし、`data/paintman-functional-gap-analysis.md` の根拠に従って `Not started` または `In progress` へ設定する。既存の実装済み部分を `Not started` に戻さない。
- **完了条件:** 18 gap のすべてが一つ以上の requirement ID と後続マイルストーンへ対応し、以後の実装が `SPEC.md` だけで判断できる。production code、format、ABI は変更しない。

#### M01 — cut/cell 所有権と永続化境界の確定

- **対象:** `PM-GAP-001/002/003/016/017` の前提、contract-only。
- **決定事項:** cut と cell の `DocumentSession`/`InkpodCore` 所有関係、`.inkpod` 内の document kind、cell file identity、cut 内参照の可搬性、atomic save 単位、非タイムシートの開始/終了/expected count/欠番、背景・参照、cut default の範囲を固定する。
- **制約:** 新しい外部拡張子を暗黙に作らない。canonical procedure へ raw path を保存しない。child cell が別 file の場合は ID と frontend file authority を分離し、path traversal と partial save を扱う。
- **文書:** `SPEC.md`、`docs/architecture.md`、`docs/file-format.md`、必要なら `docs/ffi.md` に所有権、lifetime、thread、save/recovery の設計を記す。
- **完了条件:** M02/M03 が追加判断なしで test を書ける。未決定事項があれば M02 を開始しない。

#### M02 — Cut Core model と current-only persistence

- **対象:** `PM-GAP-001`、明示的 Core-only。
- **実装:** 所属 namespace と lifetime を持つ `CutId`、cut identity/metadata、既定用紙・layer 構成、背景・参照、非タイムシート sequence metadata を責務別 module に追加する。作成、query、metadata edit を canonical transaction へ通し、ID は commit 時だけ消費する。
- **永続化:** M01 で決めた DTO 境界から current native schema へ保存し、top-level format version と必要な replay epoch を更新する。未知必須 feature、重複 ID、循環、巨大個数、壊れた UTF-8 を拒否する。
- **テスト:** create success/no-op/invalid、metadata edit、Undo/Redo、branch cut、ID high-watermark、save/reopen、旧 version 拒否、malformed/cancel/staged-open atomicity。
- **完了条件:** public Rust API だけで一つの cut を作成・保存・再開できる。Windows/FFI route は M04 まで対象外。

#### M03 — Cell bank の atomic 編集

- **対象:** `PM-GAP-002`、明示的 Core-only。
- **実装:** cut に属する stable `CellId`、表示番号、自然順とは別の明示順、欠番、thumbnail/source identity を保持し、追加、複製、挿入、削除、改番、並べ替えを typed request として実装する。開いている cell と bank entry の identity を名前や index で結ばない。
- **規則:** 最後の cell、番号衝突、同一位置 reorder、dirty/open child、複数操作の partial failure、削除後 ID 再利用禁止を固定する。bulk operation は一回の transaction と一回の Undo にする。
- **テスト:** success/no-op/invalid/cancel/stale/overflow、renumber collision、Undo/Redo、branch cut、deterministic order、save/reopen、削除済み ID 非再利用、property/state-machine test。
- **完了条件:** Core API でセルバンクを編集し、保存後も同じ ID、番号、順序、欠番を復元できる。

#### M04 — Cut/Cell Bank の C ABI と Windows 縦切り

- **対象:** `PM-GAP-001/002`。
- **実装:** bounded two-call query、stable-ID command、cut create/open、cell bank selection/edit の C ABI を追加する。Windows に `新規 > カット`、cut context、thumbnail bank、追加/複製/削除/挿入/改番/並べ替えを menu と pane から接続する。
- **安全性:** command 発行時の cut/session/cell ID と generation を固定し、stale response を別 cut へ適用しない。削除や改番の確認は typed dialog result だけを Core へ渡す。
- **テスト:** header/export parity、NULL/short/count overflow/unknown enum/wrong-thread、ABI ownership、CoreHost queue、dirty/open cell の cancel、Windows command state、keyboard/accessibility、GUI smoke。
- **完了条件:** GUI から cut を作り、cell bank を編集し、保存・再開後も同じ構成を確認できる。ABI/format 文書と version が現行コードに一致する。

#### M05 — 新規セル制作条件と複数枚作成

- **対象:** `PM-GAP-003`。
- **実装:** frame size/image size、paper preset、100 frame、基準位置/anchor、DPI、initial layer kind、8/16-bit、作成枚数、cut default override を一つの validated creation request にする。複数枚作成と cell bank 登録を all-or-nothing にする。
- **Windows:** `新規 > セル` dialog を列挙値と数値 validation を持つ form に更新し、単独 cell と cut 内 bulk create の両方から同じ request を使う。Cancel は ID、untitled number、bank を変えない。
- **テスト:** preset/custom、各 layer/depth、frame/reference alignment、count 上限、重複番号、allocation failure、Undo/Redo、save/reopen、Windows dialog/smoke。
- **完了条件:** 同一条件の複数セルを一回で作成でき、個別作成との差異や部分登録がない。

#### M06 — セル切替の保存方針と端点循環

- **対象:** `PM-GAP-016`。
- **実装:** `確認`/`通常保存`/`自動保存` の切替方針と endpoint `停止`/`循環` を versioned、bounded な application setting として保持する。Core の既存 loop semantics と Windows save orchestration を接続する。
- **規則:** 通常保存だけが通常 savepoint を進め、自動保存は autosave/recovery authority のままにする。save failure/cancel/stale target では active cell を切り替えない。
- **テスト:** clean/dirty、untitled、read-only、save failure、cancel、最初/最後/単一 cell、欠番、loop、shutdown race、setting round-trip、Windows sequence smoke。
- **完了条件:** 利用者が方針を設定し、連続切替で確認回数を減らしても保存権限と savepoint を壊さない。

### Phase B — 複数 edit target と clipboard

#### M07 — 複数 edit target の Core/EditorState/FFI 契約

- **対象:** `PM-GAP-005` の target 部分、明示的 Core/FFI scope。
- **実装:** active layer/plane と、順序を正規化した複数 edit target set を分離する。layer target の展開、非表示/非 editable、異なる document、重複 ID、必須 plane、target 消滅時の扱いを Core で検証する。
- **永続化:** 再開に必要な target set を `EditorState` と current schema に保存し、history 対象の document mutation と editor-only revision を混同しない。
- **FFI:** bounded ID span、query、replace/add/remove target を追加し、caller pointer を保持しない。
- **テスト:** active selection 不変、重複正規化、no-op、stale/missing ID、delete/Undo/Redo 後の整合、save/reopen、ABI negative test。
- **完了条件:** Core/FFI から active plane を変えず複数 target を設定・復元できる。Windows presentation は M09 まで対象外。

#### M08 — Multi-plane clipboard と atomic paste/transform

- **対象:** `PM-GAP-005` の clipboard 部分、明示的 Core/FFI scope。
- **実装:** target set の全 plane を layer/plane type、source ID、document bounds、pixel/vector/selection、色深度付き payload に格納する。通常 paste の compatible destination mapping、選択プレーンへの変換、新規 topology への paste、floating transform、cancel を一つの payload/transaction で扱う。
- **規則:** 主線と彩色の片方だけ成功する partial commit を禁止する。異なる paper、欠落 destination、型/深度変換、重なる target、見えない範囲、比較(暗)を決定的に処理する。
- **テスト:** 複数 plane round-trip、cross-document coordinate preservation、Undo/Redo/replay/save/reopen、conversion preview/cancel、allocation failure、malformed private clipboard、standard DIB fallback。
- **完了条件:** public Core/FFI 契約で主線+彩色を一回で copy/paste/transform できる。

#### M09 — Layer pane の target 表示と複数プレーン編集 UI

- **対象:** `PM-GAP-005` の Windows 完成。
- **実装:** layer/plane 行で active selection と target checked state を視覚・アクセシビリティ上区別し、keyboard でも複数 target を設定できるようにする。copy/cut/paste/transform/clear/merge/convert の command state と確認を target set に対応させる。
- **安全性:** pane controller は private state を直接変更せず M07/M08 の ABI を使う。異種変換の損失と destination topology を確定前に表示する。
- **テスト:** command catalog/state、single/multi target、主線+彩色 copy/paste、cancel、stale pane target、Undo 一回、save/reopen、screen-reader name、GUI smoke。
- **完了条件:** Windows GUI だけで複数 target workflow を安全に完遂でき、`DOC-002/003` と clipboard の gap status を更新できる。

### Phase C — Raster 描画、線修正、選択、alpha

#### M10 — Brush/Eraser の deterministic option 縦切り

- **対象:** `PM-GAP-008`。
- **実装:** 丸/角 footprint、stroke smoothing、開始 pixel と同色領域だけへ作用する mode、screen-size 固定、pressure enable を typed stroke style に追加する。screen-size は発行時 view transform から document 単位へ正規化し、replay は view/DPI に依存させない。
- **接続:** `inkpod-image` kernel、Core canonical stroke、EditorState、FFI record、Tool Options pane、status/cursor preview を一貫させる。
- **テスト:** 固定 sample 列、zoom/DPI/flip、binary/grayscale/RGBA8/16、selection、主線保護、same-color 境界、cancel/no-op、Undo/Redo/replay/save/reopen、Canvas smoke。
- **完了条件:** brush/eraser の全 option が Windows から到達し、同じ canonical input から bit-exact な結果になる。

#### M11 — Raster line/curve/shape/polyline kernel と Core/FFI

- **対象:** `PM-GAP-006`、明示的 Core/FFI scope。
- **実装:** raster 直線、単純曲線、矩形、楕円、N角形、折れ線を deterministic document geometry から rasterize する。outline/fill、width、45度/aspect/center constraint、回転、closed/polyline option を bounded typed input にする。
- **履歴:** preview state と一回の commit を分離し、active raster target、selection、主線保護、binary/grayscale/RGBA8/16 を検証する。
- **テスト:** geometry/rounding/half-open boundary golden、extreme coordinates、empty/off-canvas、cancel/stale/overflow、Undo/Redo/replay/save/reopen、FFI span/enum negative test。
- **完了条件:** Core/FFI から全 raster shape を preview/commit できる。Windows interaction は M12 まで対象外。

#### M12 — Raster shape の Windows tool interaction

- **対象:** `PM-GAP-006` の Windows 完成。
- **実装:** active plane type に応じて既存 vector controller と新しい raster controller を明示分岐し、line/curve/rectangle/ellipse/polyline/N角形、control point、double-click 終了、Esc cancel、preview overlay を接続する。
- **規則:** tool 名が同じでも raster/vector の canonical primitive を混同しない。pointer capture 中の tab/window 変更と stale view を安全に cancel する。
- **テスト:** command enable、modifier、zoom/flip/DPI hit、preview cancel、commit Undo 一回、active target、tab switch/close race、GUI drawing smoke。
- **完了条件:** raster plane 上へ正確な線・図形を GUI から直接描ける。

#### M13 — Raster gap-connect/line-width Core/FFI

- **対象:** `PM-GAP-007`、明示的 Core/FFI scope。
- **実装:** binary/grayscale raster の選択/gesture 範囲へ、距離・角度・端点評価が決定的な gap connect と、太らせる/細らせる morphology を追加する。一定幅は入力 raster から skeleton/距離規則を固定し、曖昧な場合を明示 error または preview diagnostic にする。
- **安全性:** bounded queue/work count、selection/tile boundary、cancel、overflow、all-or-nothing、主線 target validation を持つ。
- **テスト:** 1px gap、複数候補、同距離 tie-break、edge/tile boundary、binary/grayscale、selection clip、over-thin no-op、cancel/failure atomicity、Undo/Redo/replay、golden、FFI negative test。
- **完了条件:** Core/FFI に reusable な raster line correction primitive があり、vector primitive と区別される。

#### M14 — Raster 線修正の Windows UI と batch 基礎接続

- **対象:** `PM-GAP-007` の Windows 完成と `PM-GAP-018` の raster line-width 前提。
- **実装:** `線つなぎ`/`線幅修正` の局所 tool、selection menu command、parameter/preview/cancel を M13 へ接続する。既存 Batch line-width selector が raster target を選んだ場合も同じ primitive を使う。
- **テスト:** raster/vector command dispatch、pen/rect/polyline/lasso scope、preview apply/cancel、selectionなし invalid、Undo 一回、batch per-file atomicity/cancel、Windows production/batch smoke。
- **完了条件:** raster 主線の補修を GUI と既存 batch graph から実行でき、`PAINT-003` の raster 差分を閉じられる。

#### M15 — Selection geometry/trace option

- **対象:** `PM-GAP-009` の geometry 部分。
- **実装:** rectangle/ellipse の aspect、中心から作成、回転、45度 constraint と、trace brush の丸/角、pressure、screen-size 固定を typed selection request にする。M10 の footprint/numeric 規則を共有し、別実装を作らない。
- **接続:** Core/image、FFI、Selection controller、Tool Options pane、preview overlay。
- **テスト:** modifier combinations、rotation/rounding、zoom/DPI/flip、pressure、boolean New/Add/Subtract/Intersect、cancel/no-op/Undo、save/reopen、Windows gesture smoke。
- **完了条件:** PaintMan 相当の selection geometry option を GUI から決定的に利用できる。

#### M16 — 彩色向け selection 解釈と mask-only move

- **対象:** `PM-GAP-009` の range interpretation 部分。
- **実装:** 通常、描線密着 shrink、閉じた内部、描線形状、境界選択を別 mode にし、mask だけの translate を content floating transform から分離する。binary/grayscale/color plane の境界判定、selection algebra との合成順を固定する。
- **接続:** Core/image、canonical primitive、FFI、Windows mode selector/drag interaction。
- **テスト:** 小領域/穴/開線/tile edge、mask-only move で pixel 不変、content move との区別、selection layer round-trip、cancel/stale/Undo/Redo/replay、golden、GUI smoke。
- **完了条件:** 同じ geometry から用途別 mask を作り、mask だけを安全に移動できる。

#### M17 — Alpha channel を通常 tool target にする

- **対象:** `PM-GAP-011`。
- **実装:** raster plane の channel target を `Color`/`Alpha` として型付けし、pencil/brush/eraser/fill/gradient を alpha-only executor へ正規化する。Alpha edit 中は RGB を bit-exact に保持し、alpha 非対応 format/plane を拒否する。
- **Windows:** Alpha mode を plane pane、Tool Options、menu checked state、status、Canvas grayscale view で常時明示し、target 切替を undoable document edit と混同しない。
- **テスト:** 全通常 tool、RGBA8/16、selection、pressure/smoothing、fill gap、gradient、RGB preservation、cancel/no-op/Undo/Redo/replay/save/reopen、誤 target 防止、GUI smoke。
- **完了条件:** Windows の通常描画 workflow だけで alpha の局所補修ができる。

### Phase D — 品質検査と制作指示データ

#### M18 — QC 判定契約と Core/FFI mask

- **対象:** `PM-GAP-010`、明示的 contract/Core/FFI scope。
- **先行決定:** 対応する放送規格、sRGB からの変換式、matrix/range/clamp/rounding、alpha の扱い、赤/緑/青を未処理色トレース候補とする exact/tolerance 規則を `SPEC.md` に固定する。
- **実装:** 規格外色と色トレース候補を非破壊 selection mask/diagnostic category として抽出する bounded kernel、canonical command/query、FFI request/result を追加する。
- **テスト:** 規格境界の内外、RGBA8/16、alpha 0、rounding、selection clip、tile edge、no match no-op、cancel/overflow、golden、cross-target determinism、FFI negative test。
- **完了条件:** public Core/FFI から exact mask を再現できる。Windows 表示は M19 まで対象外。

#### M19 — QC 設定、selection、overlay の Windows UI

- **対象:** `PM-GAP-010` の Windows 完成。
- **実装:** 規格と候補色設定、`色領域外を選択`、色トレース候補 check、category を区別する非破壊 overlay を menu/pane/Canvas へ接続する。overlay toggle は document pixel/history/dirty を変更しない。
- **テスト:** command state、規格切替、selection 作成、overlay toggle、Undo 対象の有無、document/view revision 分離、high contrast、screen-reader name、GUI color-check smoke。
- **完了条件:** 納品前 QC を GUI で実行し、違反 pixel と未処理候補を区別して確認できる。

#### M20 — 撮影 frame と最大寄り frame

- **対象:** `PM-GAP-004`。
- **実装:** stable ID を持つ一つ以上の撮影 frame（位置、寸法、角度）と最大寄り frame/anchor を通常 frame から分離して document model、transform、snapshot、persistence、FFI に追加する。mirror/rotate/resize/paper fit で座標整合を保つ。
- **Windows:** frame 表示 toggle、Canvas handle、数値 dialog、追加/削除/並べ替え、`撮影フレームを考慮して用紙サイズ変更` を実データへ接続する。
- **テスト:** invalid/non-finite/extreme、複数 frame、transform round-trip、crop preview/cancel、Undo/Redo/replay/save/reopen、renderer overlay、GUI smoke。
- **完了条件:** 撮影範囲を保存・再編集し、用紙合わせと下流確認に利用できる。

#### M21 — 消失点/perspective guide

- **対象:** `PM-GAP-012`。
- **実装:** stable vanishing point ID、document 座標、方向、1/5/10/15/30度 interval、色、opacity を typed payload にし、derived guide を immutable snapshot へ生成する。通常 guide と payload を重複保存しない。
- **接続:** canonical create/edit/delete/clear、persistence、FFI、Canvas add/move/delete、settings dialog、表示 toggle。snapping を提供する場合は距離と tie-break を先に仕様化する。
- **テスト:** Canvas 外点、複数点、transform、derived line bounds、cancel/no-op/Undo/Redo/replay/save/reopen、snapshot determinism、GUI smoke。
- **完了条件:** 空の `LayerKind::VanishingPoint` ではなく、内容付きで再編集可能な guide になる。

#### M22 — Editable text の OS 非依存 model と persistence

- **対象:** `PM-GAP-013` の text 部分、明示的 Core/FFI scope。
- **先行決定:** UTF-8 上限、paragraph/newline、document bounds、rotation、alignment、font family/style/size の要求値、font 未発見時の frontend fallback、通常 export からの除外、rasterize の意味を `SPEC.md` に固定する。
- **実装:** stable text object ID と typed text layer payload、canonical create/edit/move/delete/rasterize request、snapshot record、persistence、bounded UTF-8 FFI を追加する。Core は font 解決や DirectWrite object を保持しない。
- **テスト:** malformed/oversized UTF-8、empty/no-op、bounds/transform、Undo/Redo/replay/save/reopen、export exclusion metadata、FFI buffer/ownership。
- **完了条件:** Core/FFI で text を内容付き・再編集可能に保持できる。Windows 入力/描画は M23 まで対象外。

#### M23 — Text editing と DirectWrite rendering

- **対象:** `PM-GAP-013` の editable text Windows 縦切り。
- **実装:** text tool、標準 edit control/IME、font selector、Canvas placement/move/rotate、DirectWrite layout cache、missing-font fallback、text snapshot rendering、明示 rasterize command を M22 へ接続する。
- **安全性:** frontend font metrics を canonical document state に逆流させない。IME composition 中の tab close、Esc、stale document を安全に処理する。
- **テスト:** Japanese IME、multiline、DPI/zoom/flip、font fallback、edit cancel、Undo 一回、save/reopen、normal export exclusion、rasterize、device-loss rebuild、GUI smoke。
- **完了条件:** Windows で text を作成・再編集・移動・表示・rasterize できる。

#### M24 — 指示 annotation、leader、RGB 注記

- **対象:** `PM-GAP-013` の instruction/freehand 部分。
- **先行決定:** annotation stroke の footprint、leader endpoint、RGB sample の固定値/参照更新時期、通常 export 除外、transform/rasterize、cell revision attachment を `SPEC.md` に固定する。
- **実装:** stable annotation/leader ID、freehand stroke、arrow/leader、text label、exact RGBA8/16 sample reference を Core/snapshot/persistence/FFI と Windows pen/Canvas interaction に追加する。
- **テスト:** pressure/coalesced input、sample edge/alpha、source revision change、transform、cancel/no-op/Undo/Redo/replay/save/reopen、normal export exclusion、explicit instruction export、GUI smoke。
- **完了条件:** pixel 本体を汚さず、座標付きの修正・色・撮影指示を保存し下流へ渡せる。

### Phase E — Light Table と連続作業

#### M25 — Light Table source の layer/plane filter

- **対象:** `PM-GAP-014`。
- **実装:** source document identity/revision と、主線のみ、彩色のみ、stable layer/plane ID 集合の filter descriptor を item に保持する。mutable Core 参照は保持せず、登録/更新時に immutable filtered asset/composite を決定的に生成する。
- **接続:** Core source topology query、filter validation、asset retention、persistence、FFI、Windows item property UI、reload failure fallback。
- **テスト:** main/color/arbitrary filter、missing/deleted source ID、order/opacity、RGBA16、source update、reload failure、save/reopen、asset branch retention、fill boundary/sample read-only、GUI smoke。
- **完了条件:** 登録後も item ごとに参照 layer/plane を選び直せる。

#### M26 — Light Table の前後 N 枚と一括 control

- **対象:** `PM-GAP-015`。
- **実装:** active cell に対する前後 N 枚 bulk registration、旧自動登録の置換、距離ベース opacity step、set 全 item の同時 move/scale/rotation、重複部透過 mode を bounded transaction として実装する。
- **接続:** M03 の stable cell order、M25 の filter、FFI span、Sequence/Light Table pane の設定と一括 command。全 item 操作は既存個別 transform を再利用する。
- **テスト:** 欠番/端点/N上限、置換規則、opacity rounding、group transform、mixed paper reference alignment、cancel/stale/allocation failure atomicity、Undo 一回、save/reopen、GUI smoke。
- **完了条件:** セル切替後に一操作で再現可能な onion-skin 状態を構築できる。

### Phase F — 制作進行情報と Batch 完成

#### M27 — Production metadata の最小仕様

- **対象:** `PM-GAP-017`、contract-only。
- **決定事項:** 担当、期限、枚数、工程状態、OK/NG、comment、伝言、linked internal reference、複数ページ memo の型、上限、stable ID、revision attachment、編集権限、削除/履歴、通常 export との分離を固定する。
- **境界:** 初回 scope は inkpod 内の保持とする。外部 tracker 同期、network identity、通知は別 requirement とし、この gap 完了条件へ混ぜない。外部 path/URL を許可する場合は replay 非依存 metadata とし、信頼境界を明記する。
- **文書:** `SPEC.md` に独立 requirement を追加し、`docs/architecture.md` と `docs/file-format.md` に ownership/persistence を記す。
- **完了条件:** M28 が UI や保存形式を推測せず実装できる。

#### M28 — Production metadata の Core/FFI/Windows 縦切り

- **対象:** `PM-GAP-017` の実装。
- **実装:** M27 の bounded stable-ID records、canonical create/edit/delete/status transition、memo page order、cell/revision link を Core/persistence/FFI に追加し、cut summary、comment/message、memo surface を target-aware Windows pane として接続する。
- **規則:** locale/clock 依存 replay を避け、期限は正規化値として受け取る。削除後 ID を再利用せず、partial save や stale cell link を拒否する。
- **テスト:** status transition、OK/NG、UTF-8/bounds、comment order、memo page reorder、missing cell/revision、Undo/Redo/replay/save/reopen、malformed metadata、pane follow/pin/stale、GUI smoke。
- **完了条件:** cut/cell と一体の制作進行・伝達情報を保存・再開・引継ぎできる。

#### M29 — Batch topology variant の Core graph/codec

- **対象:** `PM-GAP-018`、明示的 Core/format/FFI scope。
- **実装:** M14 の raster line width、2値彩色 layer/raster layer 変換、source を保持した別 plane への separation、二セル比較からの color mapping 生成を typed graph operation として追加する。target/result topology と source preservation を operation ごとに明記する。
- **永続化:** `.inkbatch` の top-level current version を上げ、旧 version を拒否する。document primitive を再利用し、batch 専用の別画像処理を作らない。
- **安全性:** per-file staged Core、dry-run、preview digest、cancel/failure、output atomicity、複数 target mapping を維持する。
- **テスト:** graph codec round-trip/old rejection/malformed、全 variant、missing target、source preservation、mapping ambiguity/alpha、dry-run、cancel/failure、per-file atomicity、FFI negative test。
- **完了条件:** Core/FFI で不足 variant を preview/実行できる。Windows editor は M30 まで対象外。

#### M30 — Batch topology preview と Windows editor

- **対象:** `PM-GAP-018` の Windows 完成。
- **実装:** source/target layer selector、result topology、source preservation、二セル比較、ambiguous mapping、raster/vector width mode を Batch pane の typed editor/preview へ追加する。
- **規則:** operation の target が欠落する場合は command を disable または validation error にし、無言 skip しない。実行中に pane target が変わっても issue-time context を維持する。
- **テスト:** recipe edit/save/load、各 variant preview、current/all、cancel、failure report、stale pane/job、output確認、Job Progress、Windows batch smoke。
- **完了条件:** 利用者が GUI から全不足 variant を安全に設定・preview・batch 実行できる。

### Phase G — 統合監査

#### M31 — PaintMan 相当 workflow の end-to-end acceptance

- **対象:** 18 gap 全体、統合のみ。新機能追加はしない。
- **シナリオ:** cut 作成、複数セル作成/改番、主線+彩色 target copy、raster 線補修/図形/brush option、advanced selection、alpha edit、前後 Light Table、QC、撮影 frame、text/annotation、switch autosave/loop、production metadata、batch、save/reopen/recovery を一つの制作フローとして検証する。
- **故障注入:** queue saturation、active stroke 中切替、save failure、stale snapshot、allocation failure、cancel、device reset、tab/window close を含め、partial commit と target drift がないことを確認する。
- **検証:** Rust full suite、quick/full benchmark、x64 Debug/Release CMake+CTest、可能なら ARM64 Debug、ABI smoke、GUI smoke、portable ZIP/MSIX payload、strict rustdoc、format corrupted corpus。
- **完了条件:** gap 18 件の acceptance evidence が public API/Windows route から得られ、未検証事項だけが明示される。

#### M32 — 追跡文書と既知差分の最終整合

- **対象:** 文書/監査のみ。
- **実装:** M31 の証拠を基に `docs/compatibility.md` を更新し、test のある requirement だけを `Verified` にする。`docs/implementation-status.md` の current state、active gaps、known differences、representative verification を置き換える。`docs/ffi.md`、`docs/file-format.md`、`docs/architecture.md`、command inventory の version/件数/名称 drift を検査する。
- **完了条件:** `PM-GAP-001` から `018` までが test 付きの `Verified`、またはユーザーが明示承認した scope 変更として説明される。`Blocked` や未検証の残差を全体完了として扱わない。`SESSION.md` の完了済み計画を恒久仕様へ転記せず、必要なら Git 履歴へ残してこの作業用ファイルを終了扱いにする。

## 6. Gap-to-milestone 対応表

| Gap | 主マイルストーン | 完了を判定する最終マイルストーン |
|---|---|---|
| `PM-GAP-001` cut | M01–M04 | M04 |
| `PM-GAP-002` cell bank | M01、M03、M04 | M04 |
| `PM-GAP-003` new cell | M01、M05 | M05 |
| `PM-GAP-004` capture frame | M20 | M20 |
| `PM-GAP-005` multi-target/clipboard | M07–M09 | M09 |
| `PM-GAP-006` raster geometry | M11–M12 | M12 |
| `PM-GAP-007` raster connect/width | M13–M14 | M14 |
| `PM-GAP-008` brush/eraser option | M10 | M10 |
| `PM-GAP-009` raster selection semantics | M15–M16 | M16 |
| `PM-GAP-010` QC mask | M18–M19 | M19 |
| `PM-GAP-011` alpha tool routing | M17 | M17 |
| `PM-GAP-012` vanishing guide | M21 | M21 |
| `PM-GAP-013` text/annotation | M22–M24 | M24 |
| `PM-GAP-014` Light Table source filter | M25 | M25 |
| `PM-GAP-015` Light Table automation | M26 | M26 |
| `PM-GAP-016` switch/save/loop policy | M06 | M06 |
| `PM-GAP-017` production metadata | M27–M28 | M28 |
| `PM-GAP-018` batch variants | M14、M29–M30 | M30 |

M31/M32 は全 gap の統合検証と追跡完了に必須である。

## 7. セッション終了時の更新形式

`Status: Completed` のないマイルストーンは未着手として扱う。各セッションは該当見出し直下へ `Status: Completed` と検証要約を一行だけ追記し、次のマイルストーンへ必要な既知差分があればその依存欄へ反映する。時系列ログを増やさず、詳細は Git diff/history と正式な status 文書へ置く。commit、push、PR は明示依頼がある場合だけ行う。
