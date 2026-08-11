# inkpod 機能ギャップ解消マイルストーン

本書は docs/paintman-functional-gap-analysis.md の22件を、同文書の
1/22〜22/22の優先順位どおりに解消する実装計画である。調査基準は
repository commit 11809da3647319ef98b99d903fd9faa5d8932b3b
（2026-08-09）であり、各セッションの開始時には現行コード、テスト、
SPEC.md、docs/compatibility.md、docs/implementation-status.mdとの差分を
再確認する。

## 進め方

- 各マイルストーンの状態は、未着手、実装中、手動確認待ち、完了、
  判断待ちのいずれかとする。一つのCodexセッションで一つを実装し、
  自動検証とバイナリ生成まで進める粒度を前提とし、勝手に分割しない。
  外部判断や環境に阻まれた場合だけ同じマイルストーンを次のセッションで
  継続する。
- 一つのCodexセッションでは、上から最初の未着手または実装中の
  マイルストーンだけを実装する。完了しても次へ進まない。
- 自動検証に成功した時点では手動確認待ちとして停止する。利用者は提示された
  バイナリを確認し、成功した場合だけ本書末尾の汎用プロンプトを再送する。
  その再送を直前の手動確認成功とみなし、直前の一件だけを完了へ変更してから
  次の未着手を選ぶ。
- 手動確認で問題が見つかった場合は汎用プロンプトではなく不具合内容を送り、
  対象を実装中へ戻して同じマイルストーンだけを修正する。
- 後順位の機能が依存先として記載されていても、順位は入れ替えない。
  先行マイルストーンでは、後から接続できる最小の型・ID・adapter境界だけを
  用意し、後続機能そのものは先取りしない。
- 既に別変更で一部が実装されていた場合も、受入テスト、production経路、
  文書、バイナリ確認手順まで満たしたことを確認してから手動確認待ちにする。
- 仕様と既存テストから一意に決められない製品挙動は推測で実装しない。
  具体的な選択肢、影響、推奨案を利用者へ示す。turn内で回答を得られなければ
  判断待ちとして停止し、次のマイルストーンへ進まない。

## 優先順位一覧

| Milestone | 順位 | 優先度 | Gap | 利用者向け成果 |
|---|---:|---|---|---|
| M01 | 1/22 | P0 | PM-GAP-007 | raster／vector混在でも論理レイヤー順どおりに表示・出力する |
| M02 | 2/22 | P0 | PM-GAP-003 | 完全な制作条件を指定して同条件のセルを複数作る |
| M03 | 3/22 | P0 | PM-GAP-006 | 複数レイヤー／プレーンを一体の編集対象として扱う |
| M04 | 4/22 | P1 | PM-GAP-016 | raster内容を解釈した選択と詳細な作図optionを使う |
| M05 | 5/22 | P1 | PM-GAP-012 | 彩色修正向けbrush shape、補正、開始色限定を使う |
| M06 | 6/22 | P1 | PM-GAP-015 | 指定範囲だけを対話的に色置換する |
| M07 | 7/22 | P1 | PM-GAP-022 | 複数行・二セル比較・分離先を含むBatchを作成する |
| M08 | 8/22 | P1 | PM-GAP-018 | vector線のAA、中心線、未接続端点を診断表示する |
| M09 | 9/22 | P1 | PM-GAP-011 | raster／vectorで完全な図形作図semanticsを使う |
| M10 | 10/22 | P1 | PM-GAP-021 | parameter変更へ追従する非累積previewを使う |
| M11 | 11/22 | P2 | PM-GAP-004 | dirtyセルを安全に自動保存してから切り替える |
| M12 | 12/22 | P2 | PM-GAP-019 | 前後NセルをLight Tableへ一括登録する |
| M13 | 13/22 | P2 | PM-GAP-014 | Color chart生成結果を確定前に比較する |
| M14 | 14/22 | P2 | PM-GAP-020 | 現代的な出力色域外pixelをselectionにする |
| M15 | 15/22 | P2 | PM-GAP-013 | guide／grid snapを実際のproduction入力へ適用する |
| M16 | 16/22 | P2 | PM-GAP-017 | 五点基準でfloating transformする |
| M17 | 17/22 | P3 | PM-GAP-001 | Cutをstable ID付き制作単位として保持する |
| M18 | 18/22 | P3 | PM-GAP-002 | セル系列を構造として原子的に編集する |
| M19 | 19/22 | P3 | PM-GAP-009 | 再編集可能なtext／instruction annotationを保持する |
| M20 | 20/22 | P3 | PM-GAP-008 | 角度と位置を持つ撮影frameを編集する |
| M21 | 21/22 | P3 | PM-GAP-005 | セル切替の端点stop／loopを選ぶ |
| M22 | 22/22 | P3 | PM-GAP-010 | 複数消失点と放射補助線を編集する |

## 全マイルストーン共通の実装規律

1. セッション開始時に git status、既存差分、対象コードとテスト、
   SPEC.mdの関連節を確認する。現在状態または既知差分が関係する場合だけ
   status／compatibilityの該当箇所を確認し、利用者の変更を保護する。
2. 利用者が観測できる公開契約を先にテストで固定する。document mutationは
   typed requestから既存のcanonical primitive executorへ入り、success時だけ
   document、StateId、revision、history、journal、dirty、ID high-watermark、
   cache invalidationを一括公開する。
3. no-op、invalid、Cancel、stale、overflow、failureでrevision、history、
   journal、dirty、persistent ID、確定snapshot、通常出力を進めない。
   view-only変更はdocument historyとdirtyを変えない。
4. Rust Coreが意味、検証、座標、画像処理、選択、履歴、永続化、
   immutable snapshotを所有する。C++はCommandContextを固定する薄いOS／UI
   adapterとし、Coreの規則を再実装しない。
5. C ABIは固定幅値、structure size、bounded span、opaque ownership、
   NULL／alignment／短い構造体／未知enum／stale IDのnegative testを持つ。
   header、Rust宣言、export、docs/ffi.mdのdriftを残さない。
6. 座標はdocument、view logical、device pixelを分離し、CoreへOS DPIを
   入れない。長時間処理はprogress、cancel、base revision、target generationを
   固定し、UI threadで待たない。
7. serialized schema、canonical procedure、replay結果、.inkbatch semanticsを
   変更する場合は、フォーマットフリーズ前の規則に従い、同じ変更で該当する
   最上位file-format／replay versionを上げる。section versionだけで代用しない。
   各セッション冒頭でformat／replay／ABI impact表を作り、新canonical
   procedureまたはpersistent semanticsならversion更新を原則必須とする。
   既存C ABI recordのlayoutを暗黙変更せず、新export／size-versioned record
   または明示ABI version更新を選ぶ。
8. 新規production routeはdocs/primitive-route-inventory.mdへ分類し、
   ownership／thread／lifetime変更はdocs/ffi.md、永続化変更は
   docs/file-format.mdへ反映する。
9. 各PM-GAPの詳細能力を広い既存要件のVerifiedへ埋没させない。実装前に
   SPEC.mdへ独立追跡できるdomain requirement IDと受入条件を追加し、
   対象マイルストーンのdocs/compatibility.md行を必ず更新する。
   現在状態、active gap、既知差分、代表検証が変わる場合だけ
   docs/implementation-status.mdを置き換える。時系列ログは追加しない。
10. 対象マイルストーンの自動完了条件をすべて満たしたら状態を手動確認待ちへ
    変更し、変更内容、設計判断、ファイル、検証結果、未検証事項、手動確認を
    報告して停止する。手動確認前に完了へしない。

## 共通検証ゲート

変更範囲に応じた対象テストを追加したうえで、少なくとも次を実行する。

~~~text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo bench --package inkpod-core --bench core_workflows -- --quick
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --package inkpod-core --all-features --no-deps
cmake --preset windows-x64-release
cmake --build --preset windows-x64-release
ctest --preset windows-x64-release
~~~

MSVC x64を必須のWindows検証基準とし、ARM64はwindows-arm-releaseによる
追加検証として扱う。ARM64成功をx64未検証の代用にしない。性能に影響する
マイルストーンでは既存quick/full workloadの意味counterとchecksumを確認し、
wall-clockはwarm-up後5回以上の中央値を使う。上限超過は独立した5回以上の
再測定でも再現した場合だけ回帰と判定する。
docs/core-benchmark-baseline.mdのworkload、harness、envelope、revision-max式を
変更する必要がある場合は、変更前に利用者の明示承認を得る。検証不能な項目は
隠さず、理由と代替証拠を残す。

## P0

### M01 — PM-GAP-007: raster／vector混在時の正しいレイヤー順

状態: 完了

関連要件: DOC-002、DOC-003、VECTOR-001、RENDER-001、ABI-002、PERF-001。

今回の契約影響:

| Contract | Current | M01 impact |
|---|---:|---|
| native `.inkpod` top-level format | 9 | 永続 schema/canonical procedure は不変。version 更新なし |
| replay epoch | 6 | primitive semantics は不変。epoch 更新なし |
| C ABI | 5 | 既存 layout を変えず ordered render-plan query を追加。version 更新なし |
| canonical snapshot-composite digest | schema 2 / context v2 | ordered pass と adjustment LUT を commitment へ追加 |

成果:

- layer index 0をpalette最上段とする既存規則のまま、R/V/R、V/R/Vを含む
  任意の論理layer／plane順が、表示、thumbnail、flatten exportで一致する。
- visibility、plane／layer opacity、alpha、adjustmentを順序どおりに適用する。
- editable vector geometryをdocument scaleのrasterへ潰さず、zoom時の品質を保つ。

実装:

1. layer index 0がpalette最上段であることをSPEC.mdの公開順序契約へ明記する。
2. rust/inkpod-core/src/snapshot.rsとvector render modelに、raster tile span、
   vector fill／stroke、layer groupを同じ順序で表すimmutableなordered render
   passを導入する。Coreが順序、opacity、adjustment semanticsとgroup境界を
   解決済みのrecordとして確定し、rendererにlayer規則や画像処理algorithmを
   再実装させない。
3. 現在の「全rasterを一枚へ先行合成してから全vectorを描く」経路を、
   ordered recordの反復へ置き換える。raster-onlyの連続区間はband／tile cacheで
   再利用してよいが、vectorを跨いで順序を失わない。
4. revision-maxのpayload非走査cache-hit契約を維持する。order、opacity、
   visibility、adjustment変更は既存契約どおり同じcommitで必要なcacheを
   invalidateする。
5. canonical_composite_digestもordered passの意味順をhashし、同じcontentでも
   reorderすれば正しい合成結果digestへ変わるようにする。
6. rust/inkpod-ffi/src/vector_snapshot、include/inkpod/core_ffi.h、
   apps/windows/renderer/canvas.cppをordered batched recordへ接続する。
   Coreが供給する解決済みrender passを順に実行するだけとする。中間surfaceが
   必要ならrenderer resourceとしてRAII管理し、device lostで再構築する。
7. thumbnailとflat exportを同じordered composite semanticsへ接続し、
   rasterizeによる回避を利用者へ要求しない。
8. PM-GAP-006のmulti-target UIは実装しない。既存の単一選択reorderだけで
   mixed order fixtureを作り、将来のstable target setと接続可能なIDを保つ。

受入・回帰:

- Core public contractでR/V/R、V/R/V、同一layer内plane順、非表示、
  0／50／100% opacity、alpha、adjustmentの小さなpixel goldenを固定する。
- reorderのsuccess／no-op／invalid、Undo／Redo、save／reopen後にsnapshot、
  thumbnail、export checksumが一致することを確認する。
- FFI record順、count／stride／ownership、短い構造体、NULL、未知record種別を
  検査する。
- Windows smokeでvectorをrasterの下と上へ移動し、実際のoffscreen pixel
  sample／goldenが変わることを確認する。record順だけのchecksumを描画結果の
  代用にしない。
- 既存pan_zoom_snapshotとdirty_tile_rebuildのpayload access、reuse／rebuild、
  checksum、revision gateを維持し、quickに加えてfull benchmarkを評価する。

手動確認:

- 不透明なraster、半透明vector、別rasterを三層に置き、順序を入れ替えて
  Canvas、保存後再open、PNG exportが同じ見た目になることを確認する。

### M02 — PM-GAP-003: 完全な制作条件による新規セル作成

状態: 完了

関連要件: DOC-001、SESSION-001、IO-001。

成果:

- frame sizeまたはimage size、DPI、初期layer type、8／16 bit、
  基準frame、最大寄りframe、五点anchor、作成枚数を一度に指定できる。
- 複数セルは全件をstaged生成できた場合だけsession／tabへ公開し、途中失敗で
  一部だけ残さない。

実装:

1. CoreにboundedなCellCreationOptionsを追加する。sizing mode、物理／pixel
   寸法、DPI、frame比率、anchor、initial layer kind、typed PixelFormat、
   countをenum／newtypeで表し、非有限値、ゼロ、上限、乗算overflow、
   topology不整合を一括検証する。
2. frame sizeとimage sizeの換算、rounding、margin、reference／drawing／safe／
   shooting／maximum-close frameの関係をSPEC.mdへ明記し、同じ関数だけを
   dialog previewとcommitで使う。
3. Rust Coreはbounded creation planと各Genesisを検証する。CoreHostが
   Core engine thread上で全Core handleを作成／破棄し、ApplicationHostが
   全件成功後だけDocumentSession／tabを公開する。UI threadやCore crateへ
   別handleのowner責務を持たせない。
4. frontendが供給してよいのは既存契約上のdocument UUIDだけとし、
   layer／plane等のCore-owned stable IDは全staging成功後のcommit境界で
   確定する。途中失敗でIDを消費しない。
5. 将来M17のCut defaultsをoptionsへ代入できるdefault-provider境界だけを置く。
   Cut model、membership、保存形式は先取りしない。
6. Windowsの新規セルdialogを番号入力ではない標準combo／radio／spin controlで
   拡張し、条件summaryと作成結果を表示する。CancelはCoreとUI stateを変えない。
   既存axis-aligned frame metadataだけを設定し、M20の角度付き撮影frame
   objectを先取りしない。
7. M17前の複数作成はfocused workspaceのactive EditorGroupへ独立untitled
   DocumentSessionとして全件公開する。表示名の連番はfrontend presentation
   だけとし、CellIdや将来のsequence numberへ流用しない。全件をtab registryへ
   入れられない場合は一件も公開しない。
8. Genesis／native schemaに新しい意味が入る場合はcurrent-only formatと
   replay versionを同じ変更で更新する。

受入・回帰:

- frame／image mode、8／16 bit、各初期layer、五点anchor、最大寄りframeの
  public Core table testとsave／reopen equalityを追加する。
- count 1と複数、最大境界、invalid、allocation failure、途中staging failureで
  session、revision、ID、tab、recent fileが部分公開されないことを検査する。
- FFIのowned result release、NULL、short struct、未知enum、巨大count、
  二重releaseをnegative testで固定する。
- Windows smokeで複数セルのtab数、各document info、pixel format、
  frame metadataが同一条件であることを観測する。

手動確認:

- image sizeとframe sizeの両方で3枚の16-bitセルを作り、各セルのDPI、
  frame、初期layerをpropertiesで比較し、Cancel時にセルが増えないことを
  確認する。

### M03 — PM-GAP-006: 複数レイヤー／プレーンの一体編集

状態: 完了

関連要件: DOC-002、DOC-003、CLIP-001。

成果:

- keyboard focus／描画先となるactive rowと、ordered multi-edit-targetを
  明確に分ける。
- 主線、彩色、任意raster／vector planeの組を、型、順序、document座標を
  保った一つのcopy／paste／duplicate／delete操作として扱える。
- 描画strokeは引き続き一つのactive planeだけを対象とし、複数対象へ
  暗黙に描かない。

実装:

1. EditorStateにdocument所属のstable layer／plane IDからなるbounded、
   重複なし、順序付きEditTargetSetを追加する。active targetは集合内外を
   明示し、deleted／stale IDを現在active文書へ再解決しない。集合順はclick順
   ではなくdocument tree順とし、reorder後も意味順へ正規化する。
2. commandごとのmulti-target capability matrixをCoreに置く。copy、
   typed paste destination resolution、duplicate、delete、visibility／
   editability、互換merge／convertを検証し、非互換集合は実行前に理由を返す。
3. 複数対象のdocument mutationは一つのcanonical invocation、一transaction、
   一revision、一history entryにする。途中failureで全対象を戻し、IDを
   消費しない。
4. typed clipboardを複数payloadのordered collectionとしてC ABIへ公開し、
   same-type destinationを優先する。通常pasteと明示conversion pasteを分け、
   lossがある場合はpreview／確認を維持する。
5. Layer／Plane paneはTreeViewのactive rowと別のtarget markerを表示し、
   Ctrl／ShiftまたはSpaceで集合を操作する。MSAA／UIA名、command enable state、
   status bar、menuを同じCommandContextへ同期する。
6. EditTargetSetはdocument semantic stateやview stateではなく、
   DocumentSessionが所有するpersisted EditorStateとする。EditorRevision／
   editor savepointだけを進め、document historyを進めない。EDIT schemaを
   変更するためtop-level format versionを更新し、open時に存在しないIDを
   boundedに拒否する。
7. marker集合の変更だけではdocument revision、StateId、history、journal、
   document dirtyを変えない。実際のmulti-target document commandだけを
   一transaction／一Undo単位にする。

受入・回帰:

- 主線＋彩色copy／pasteで属性、安定IDの意味、順序、origin、8／16 bitが
  保持されるpublic acceptanceを追加する。
- duplicate／delete／visibility、互換／非互換merge、no-op、invalid、
  stale、Cancel、Undo／Redo、save／reopenを対象集合全体で検査する。
- FFI spanのcount／stride／ownership、重複ID、別document ID、削除済みID、
  oversized countを検査する。
- Windows smokeでactive rowを変えずに複数markerを付け、Copy／Pasteの
  enable stateと結果tree／checksumを観測する。

手動確認:

- 主線と彩色planeを同時targetにして別セルへcopy／pasteし、一回のUndoで
  両方が戻ること、通常brushはactive planeだけへ描くことを確認する。

## P1

### M04 — PM-GAP-016: Raster選択の内容解釈と作図option

状態: 完了

関連要件: SEL-001、SEL-002、SEL-003。

成果:

- raster selectionで通常形状、描線へ密着するshrink、閉領域内部、
  描線形状、境界だけを区別できる。
- rectangle／ellipseはaspect、中心から作成、45度制約、確定前回転を持ち、
  trace brushは丸／角、pressure、screen-size固定を持つ。

実装:

1. inkpod-imageとCore selectionにtyped RangeInterpretation、
   SelectionConstructionOptions、TraceBrushOptionsを追加する。既存の
   half-open pixel境界と座標roundingを一か所で使う。
2. 描線／内部／境界の判定は再帰を使わず、bounded scanline／queueとtile
   boundary検査で決定的にmask化する。8／16 bit、alpha、階調主線の基本色／
   coverage規則を明示する。
3. option付きgeometryを既存selection algebraのnew／add／subtract／intersectへ
   正規化し、previewとcommitが同じmask生成関数を使う。
4. C ABIのselection optionsとWindows selection_controller／tool options paneを
   enumとtyped valueで拡張する。modifier、view ID、target generationを
   request発行時に固定する。
5. 新しいselection optionとcanonical selection procedureをpersistするため、
   top-level file-format／replay versionを更新し、非current version拒否testを
   同じ変更へ追加する。

受入・回帰:

- 各range interpretationを小さな線画fixtureのmask goldenで固定し、
  empty、外周接触、1 pixel線、穴、tile境界、最終valid pixelを含める。
- rectangle／ellipseのaspect、center、rotation、45度、trace shape／pressure／
  screen-sizeとselection algebraの組合せを検査する。
- success／semantic no-op／invalid／Cancel／stale、Undo／Redo、8／16 bit、
  FFI option validation、Windows preview gestureを検査する。
- New operationで既存selectionを空maskへ置換する場合はstate変更として
  revision／Undoを進め、元から同じ空maskの場合だけno-opとする。
- 既存selection／dirty tile benchmarkの意味counterを維持し、quickとfullを
  評価する。

手動確認:

- 同じ線画で「内部」「線」「境界」を切り替えてmask表示を比較し、回転ellipseと
  角trace brushを追加／削除modifier付きで確認する。

### M05 — PM-GAP-012: 彩色修正向けbrush option

状態: 完了

関連要件: PAINT-001、PAINT-004、HIST-001。

仕様決定ゲート:

- 開始色限定について、exact／tolerance、alphaを比較に含めるか、
  Grayscale／Binary／RGBA8／16の比較値、同色の非連結領域を許すかが
  SPEC.mdと公開テストで未確定なら、選択肢と誤塗り／性能への影響を示して
  利用者の決定を得る。決定したpredicateをSPEC.mdへ記録してから実装する。

成果:

- 通常brushで丸／角shape、stroke smoothing、開始pixelと同じ色の領域だけへ
  描くmodeを利用できる。
- 開始色限定はstroke開始時のimmutable baseを基準にし、描いた色によって
  predicateがstroke中に拡張しない。

実装:

1. inkpod-image/src/edit/brush.rs、Core stroke／paint modelへBrushShape、
   bounded smoothing parameter、StartColorPredicateを追加する。
2. 承認済みの開始色predicateをstored native-depthのtyped pixel semanticsで
   実装し、未指定のtoleranceやalpha特例を勝手に加えない。階調／binary対象も
   同じ決定内容をSPEC.mdへ明記する。
3. smoothingは固定幅／固定roundingのCore-owned algorithmとし、thread数、
   sample分割、OS pointer historyに依存しないcanonical sample列へ正規化する。
4. pressure、diameter、selection、主線保護、丸／角footprint、開始色predicateを
   一つのstroke executorで評価し、begin／append／endを一履歴単位に保つ。
5. FFI stroke option、EditorState codec、Windows tool options paneと
   input routerを接続する。UIはsample単位でCoreを待たず、begin／end／cancelを
   落とさない。
6. canonical stroke optionとreplay結果が変わるため、top-level
   format／replay version、非current rejection、procedure codecを更新する。

受入・回帰:

- 丸／角、smoothing off／on、pressure、開始色限定のdeterministic pixel goldenを
  追加し、隣接異色領域とalpha差が変わらないことを確認する。
- 同じsample列の分割方法を変えても同じcanonical結果になること、
  Cancel完全復元、一回Undo／Redo、no-op／stale／overflowを検査する。
- FFI option、未知enum、非有限sample、Windows stroke smokeを追加する。
- 既存stroke benchmarkの16 strokes／544 samples、queue、Present、
  payload access、checksumを維持し、quick／fullを評価する。

手動確認:

- 二色が接する狭い塗り残しで開始色限定brushを連続描画し、隣色へ
  はみ出さないこと、丸／角と補正量の違い、Cancel／Undoを確認する。

### M06 — PM-GAP-015: 対話的で範囲限定された色置換

状態: 完了

関連要件: COLOR-REPLACE-001、FILL-003、SEL-001、VECTOR-001。

今回の契約影響:

| Contract | Current | M06 impact |
|---|---:|---|
| native `.inkpod` top-level format | 13 | 新canonical procedureを追加するため14へ更新し、v13を拒否 |
| runtime replay epoch | 10 | scoped raster／vector置換semanticsを追加するため11へ更新 |
| C ABI | 8 | 既存layoutを変えずsize-versioned input recordと新exportを追加。version更新なし |
| `.inkbatch` top-level format | 1 | 既存Batch graph／operation semanticsは不変。version更新なし |

仕様決定ゲート:

- vectorの「対象線全体」がpath、stroke、segment、複合pathのどれを単位とし、
  fill objectを含むか、regionとの接触をgeometry／描画coverageのどちらで
  判定するかが未確定なら、実装前に候補と影響を示し、SPEC.mdへ決定を記録する。

成果:

- pen、rectangle、polyline、lassoの範囲内だけでtarget colorをdrawing colorへ
  置換し、範囲外の同色を保持する。
- vectorは承認済みの「対象線全体」単位で、範囲に触れた線の色を変える。

実装:

1. M04の共通region contractを使うScopedColorReplaceRequestをCoreへ追加し、
   region、selectionとのintersection、target color、replacement、
   target plane／modeを一つのcanonical procedureにする。
2. rasterはnative-depth exact color、vectorは承認済みのstable object単位で
   判定する。C++側hit testや別の置換規則を作らず、line／fill modeを明示する。
3. 主線modeと彩色modeのtopology／主線保護をCoreで検証する。SPEC.md
   §11に従いselection／geometryなしのdocument全体実行はWindowsで明示確認し、
   最終範囲をCore requestへ含める。
4. bounded region point ingestion、preview、commit／cancel、FFI recordと
   Windows gesture／tool options／command stateを接続する。
5. 新規primitiveとserialized procedureに対応するformat／replay version、
   route inventory、docs/ffi.mdを更新する。

受入・回帰:

- 4 geometry、tile境界、selection intersection、同色の離散領域、
  8／16 bit／alphaのraster goldenを追加する。
- vector pathの一部接触、非接触、複数path、hidden／locked／main-line protected
  targetを検査する。
- success／no-op／invalid／Cancel／stale／overflow、Undo／Redo、replay、
  save／reopen、FFI negative、Windows gesture smokeを追加する。
- quick／full benchmarkでregion mask作成と変更tileだけの再合成を評価し、
  既存workloadを無断変更しない。

手動確認:

- 同じ色を二か所に置き、一方だけをlasso置換して他方が不変であること、
  vector線の一部へ触れるとその線全体が変わることを確認する。

### M07 — PM-GAP-022: 連続彩色向けBatch authoringの詳細

状態: 完了

関連要件: BATCH-001、BATCH-002、BATCH-003、BATCH-004、CLIP-001。

仕様決定ゲート:

- 二セルpair抽出のsame-color、one-to-many、many-to-one、alpha差、
  候補ordering、曖昧候補を利用者が解決する方法はギャップ分析で判定不能と
  されている。推測でexact座標集計へ固定せず、具体的な候補algorithmと
  誤対応／操作量への影響を提示して利用者の決定を得る。決定後にSPEC.mdと
  小さなinput／expected-pair goldenを先に固定する。

成果:

- continuous fillの複数seed、color replaceの複数pair、行ごとのenable／反転、
  二セル比較によるpair候補、separation destination、実行時再設定を
  production UIから扱える。
- 二セルの対応が曖昧な場合は自動で一件へ決めず、候補と理由をpreviewし、
  利用者が解決するまで実行しない。

実装:

1. batch model／codec／validationをbounded multi-row recordへ統一し、
   現行のdocument UUID＋source generationからなるSequenceSourceIdentity、
   document座標、exact-depth color、enabled flag、destination enum、
   per-run configuration flagを保持する。M18でCellId providerへ差し替える。
2. 承認済みの二セルpair algorithmを決定的に実装する。曖昧さを
   ambiguity recordとして返し、silent winnerを選ばない。候補、件数、
   affected boundsをpreviewする。
3. separationはmask生成、単色置換、主線plane、彩色planeと、SPEC.md
   §19に既定済みの別file出力を
   typed destinationとして実装し、各target topologyと主線保護を検証する。
   基本mask／replacement／invert結果を先にgoldenで固定する。
   別fileは既存native .inkpod output routeだけを使い、新しい外部形式を
   追加しない。
4. 「実行ごとに設定」はjob enqueue前にimmutableな実行configを完成させる。
   実行中のgraphを変更せず、変更したい場合はcancelして新jobを作る。
5. C ABIにowned ambiguity／preview resultとbounded row spansを追加し、
   Windows batch dialog／controllerへrow editor、二セル選択、destination、
   preview、validation error位置を接続する。
6. M03のmulti-targetを使う。M18の構造化sequenceとM14のQAは後から接続できる
   adapterに留め、両機能を先取りしない。
7. .inkbatchの最上位versionを更新する。新しいBatch operationがdocument
   procedureとして保存／replayされる場合は.inkpod top-level format／replay
   versionも同じ変更で更新し、両方の非current rejectionを検査する。

受入・回帰:

- 複数seed／pairのcodec round-trip、上限、重複、行enable、全pair反転、
  preset save／loadを検査する。
- unambiguous／one-to-many／many-to-one／alpha差のpair extraction goldenと、
  未解決ambiguityの実行拒否を検査する。
- 全separation destinationのpixel／tree／output golden、dry-run非変更、
  cancel／failure／stale、per-output atomicity、既存file保持を検査する。
- FFI ownership／short record／oversized row、Windows row editor／preview／
  Job Progress smokeを追加する。
- 既存batch／replay／checkpoint benchmarkのchecksum、semantic counter、
  failure atomicityをquick／fullで評価する。

手動確認:

- 3 seedと3 pairを一つのBatchに設定し、曖昧な二セル比較が警告されること、
  mask／主線／彩色の各分離先をpreviewしてから少数sequenceへ実行できることを
  確認する。

### M08 — PM-GAP-018: Vector線の診断表示

状態: 完了

関連要件: VIEW-005、VECTOR-001、ABI-002。

成果:

- vector antialias on／off、中心線overlay、中心線のみ、未接続端点を
  viewごとに切り替えられる。
- 診断表示はdocument、history、journal、dirtyを変えず、同じdocumentの
  別viewへ漏れない。

実装:

1. ViewStateに互いの意味が矛盾しないtyped VectorDiagnosticFlagsを追加し、
   中心線のみの場合の通常stroke非表示、AA mode、endpoint markerをCoreで
   確定する。
2. vector modelの明示topological connectionを正本として未接続端点を
   stable path／endpoint ID付きのbounded diagnostic recordとしてsnapshotへ
   出す。近接距離だけで接続済みにしない。距離判定が必要なら別の仕様決定を
   経て、rendererで再判定しない。
3. M01のordered contentの後に非破壊diagnostic overlayを描くFFI recordと
   renderer routeを追加する。markerはzoomに対して読めるdevice-pixel sizeを
   view transformから得るが、document geometryを変更しない。
4. menu、shortcut、checked state、status／accessibilityを同じview IDと
   generationへ接続する。AA offは実際のvector render optionへ作用させる。
5. diagnostic flagsはDocumentView所有のview-local stateとし、document単位の
   persisted EditorStateや.inkpodへ混ぜない。再起動時の既定を保持する場合は
   versioned workspace／application settingとして分ける。

受入・回帰:

- 同一documentの二viewで全flagの独立性、view revisionだけの変化、
  dirty／history／document digest不変を検査する。
- 接続済み／未接続／閉path／近接するが未接続のendpoint、中心線、中心線のみ、
  AA on／offのsnapshot／renderer goldenを追加する。
- FFI flag、record bounds、未知bit、Windows toggle／shortcut／checked-state
  smoke、device lost後の再表示を検査する。

手動確認:

- 微小gapのあるvector線を高倍率／低倍率で開き、中心線、中心線のみ、
  未接続端点、AAを切り替え、別viewの設定が変わらないことを確認する。

### M09 — PM-GAP-011: PaintMan相当の図形作図semantics

状態: 完了

関連要件: PAINT-002、VECTOR-001、HIST-001。

成果:

- raster／vectorの両対象で、直線、二段階curve、rectangle、ellipse、
  N角形、filled shape、click式polylineをpreviewして確定できる。
- 入り／抜き、outline／fill、45度、aspect、center、断面形状、閉pathを
  target typeに応じて一貫して使える。

実装:

1. Coreにbounded pointsとtyped optionを持つGeometryRequest、
   GeometryPreviewSession、target capability matrixを置く。raster pixel生成と
   vector object生成は内部実装を分け、公開gesture semanticsを揃える。
2. curveはstart／endを確定後にcontrol pointを動かす二段階state machine、
   polylineはclick追加／double-click終了／close option、N角形は辺数上限と
   center／radius／rotationを持つ。
3. raster shapeはinkpod-imageの決定的なcoverage／fill kernelで描き、
   vector shapeはstable geometry IDをcommit時だけ消費する。previewはbaseから
   毎回作り、Cancelで完全復元する。
4. constraintをCore input interpreterへ集約し、M15がguide／grid sourceを
   差し込めるclosed interfaceだけを置く。このマイルストーンでは実際の
   production snap接続を先取りしない。
5. FFIにbounded point ingestionとpreview ownership、Windows vector_controller、
   input router、tool options pane、command enable stateを接続する。
6. 新規canonical geometry procedureとresult semanticsに合わせて
   format／replay version、route inventory、docs/ffi.mdを更新する。

受入・回帰:

- raster／vector × 各primitive × outline／fillのcapability tableとpixel／
  geometry goldenを追加する。
- 二段階curve、N角形の辺数境界、polyline終了、入り抜き、45度、aspect、
  center、rotation、断面を検査する。
- begin／update／commit／Cancel、no-content、invalid target、point上限、
  非有限／極端座標、stale、Undo／Redo、replay、save／reopenを検査する。
- FFI ownership／negativeとWindows段階gesture smokeを追加し、既存stroke／
  snapshot quick／full benchmarkの意味gateを維持する。
- PM-GAP-011のsnap部分はM15の完了まで既知差分として追跡し、この時点で
  PAINT-002全体をVerifiedへ更新しない。

手動確認:

- rasterとvectorで同じrectangle、filled N角形、二段階curve、closed polylineを
  作り、preview／Cancel／一回Undoと45度・center制約を確認する。

### M10 — PM-GAP-021: parameter変更に追従する非累積preview

状態: 完了

関連要件: HIST-001、FILTER-001、FILTER-002、FILTER-PREVIEW-001。

成果:

- filter／色調補正dialogのparameterを変えるたびに、同じbase stateから
  非累積でpreviewし、結果を見て再調整できる。
- OKは一回のcommit、Cancel／closeは完全復元となる。

実装:

1. 既存Core／FFI filter preview sessionを正本とし、Windows
   effects_controllerとeffects dialogのchange notificationからupdateを発行する。
   C++にfilter計算を追加しない。
2. UIからの連続updateはbounded latest-preview-wins queueにし、古いgenerationの
   worker結果を表示しない。begin、apply、cancel、releaseは欠落させない。
3. dialogは発行時CommandContext、base revision、target IDsを固定し、tab切替、
   target delete、document closeで別targetへfallbackせず安全にcancelする。
4. preview中のparameter／progress／errorをdialogとJob Progressに反映し、
   UI threadでCore workやPresentを待たない。
5. Core／FFIの既存契約が不足する場合だけ最小拡張し、所有権とstale resultの
   release責務をdocs/ffi.mdへ明記する。

受入・回帰:

- parameter A→Bのpreviewがbase→Bの一回計算と一致し、base→A→Bの累積結果とは
  区別されることをcontroller／Core acceptanceで固定する。
- rapid update、out-of-order completion、stale target、worker cancel、
  dialog close、Cancel、OK一回、Undo／Redoを検査する。
- Windows smokeは最低三回parameterを変え、途中preview checksumの変化、
  Cancel不変、OK後一history entryを観測する。

手動確認:

- Gaussian blurまたはtone curveの値を複数回往復し、表示が累積劣化せず、
  Cancelで元画像、OK後一回Undoで元画像へ戻ることを確認する。

## P2

### M11 — PM-GAP-004: セル切替時の安全な自動保存

状態: 完了

関連要件: SEQ-001、SESSION-001、IO-001。

成果:

- dirty cellから別cellへ移る際、PromptまたはAutosave-before-switch policyを
  選択できる。
- 自動保存のdurable publication成功後だけ切り替え、失敗／Cancel／staleでは
  現在cellと未保存編集を保持する。
- autosaveは通常savepointとpath authorityを進めない。

実装:

1. 発行時のsource session／cell identity、source revision、target identity、
   policyをimmutable switch requestへ固定する。pathやpane indexをcanonical
   identityとして保持しない。
2. authoritative document pathを置換せず、既存の別autosave／recovery artifact
   とmetadataへexact native stateをstaged保存する。untitled documentにも
   session UUIDでassociationを持たせ、flush／close／replace成功後だけ
   sequence activationを行う。
3. ApplicationHost／DocumentSession registryがsequence entryごとのexact
   native autosave sourceとgenerationを保持する。元cellへ戻る際はそのsourceを
   staged Coreでopen／validate／replayし、成功後だけactive Coreを交換する。
   flattened SequenceCellSourceだけからlayer／historyを復元しようとしない。
4. 切替後もautosave済みdirty stateとrecovery associationを保持し、
   通常保存前の編集状態を失わない。autosave成功を「通常保存済み」と表示せず、
   通常savepoint／path authorityを進めない。
5. save failure、Cancel、stale、close／shutdown競合ではtargetへ切り替えず、
   partial file、進んだsavepoint、別cellへのerror付替えを残さない。
6. versioned application setting、progress／failure UI、Sequence pane、
   DocumentSession／CoreHostの非同期連携を実装する。M17／M18のCut identityへ
   後から差し替えられるtarget adapterに留める。

受入・回帰:

- Prompt／Autosave policy、success後だけswitch、save failure、Cancel、
  stale revision、queue saturation、close／shutdown raceを検査する。
- 通常savepoint／path authority不変、dirty維持、recovery候補、元cellへ戻った
  ときのchecksum、元file非破壊を検査する。
- Windows session recovery testとproduction sequence smokeでprogress、
  active cell、error、command enable stateを観測する。

手動確認:

- dirty cellをAutosave modeで次へ切り替え、戻って編集が残ること、
  通常保存済み表示にならないこと、保存先を書込不可にした場合は切り替わらない
  ことを確認する。

### M12 — PM-GAP-019: 前後NセルのLight Table一括登録

状態: 完了

関連要件: LT-001、LT-003、SEQ-001。

仕様決定ゲート:

- 決定済み。distance別opacityは線形clamp、z-orderは後の自然順cellほど上、
  同一source document UUIDは既存itemを保持してskipする。明示的なsource更新だけが
  既存itemを変更する契約をSPEC.mdとgoldenへ固定した。

成果:

- 現在cellを基準に前／後／両方向のN枚を一操作で登録し、距離に応じた
  opacity stepを決定的に適用できる。

実装:

1. Coreにsequence-relative BulkLightTableRegistration requestを追加し、
   direction、N、base opacity、distance step、対象set IDをtyped値で持たせる。
2. 承認済みのopacity／z-order policyをmilli値とstable sequence orderから
   決定的に計算する。自然順、欠番、端点、不足枚数、clampをCoreで決定する。
3. current cellと既存同一source itemは承認済みduplicate policyで処理する。
   全件が同一結果ならno-op、途中不正なら全体failureにする。
4. 一括追加を一つのcanonical command、一Undo単位にし、個別source revision、
   alignment、color modeの既定を既存LT規則から得る。
5. C ABIにN／direction／opacity valuesとresult summaryを追加し、Light Table
   paneにpreview、件数、順序、opacity一覧を表示する。
6. M18までは現行sequence discoveryを使い、後からmembership providerを
   差し替えられるようにする。
7. 新しいcanonical LT commandとpersistent semanticsに合わせ、top-level
   format／replay versionと非current rejectionを更新する。

受入・回帰:

- 前／後／両方、N=0／1／上限、欠番、端点、不足、重複、1-cell sequence、
  opacity 0／1000とoverflowを検査する。
- 一回Undo／Redo、save／reopen、source更新、alignment、no-op stability、
  invalid／stale／failure atomicityを検査する。
- FFI negativeとWindows pane／preview smokeを追加し、既存LT compositeと
  snapshot quick／full benchmarkを評価する。

手動確認:

- 中間cellから前後3枚を登録し、距離ごとに薄くなること、一覧順、
  一回Undo、保存後再openを確認する。

### M13 — PM-GAP-014: Color chart生成結果の比較preview

状態: 完了

関連要件: COLOR-002、HIST-001。

仕様決定ゲート:

- 決定済み。生成結果でchart全体の色順を置換し、native depthを含む完全一致色の
  既存名を保持、新規色だけ決定的な既定名にする。lock中はpreviewだけを許可して
  Applyを拒否し、現在の選択色が残ればそのpage／選択を保持、消えた場合だけ
  先頭へ移す。独立要件ID、受入条件、永続化契約をSPEC.mdへ固定する。

成果:

- maximum colorsとquantizationを変えながら生成chartを元stateと比較し、
  確定または完全Cancelできる。

実装:

1. 確定対象をColor chart modelとし、immutable chart preview resultと
   revision-bound apply tokenへ分離する。既存palette生成kernelは色抽出／
   quantizationの内部処理としてだけ再利用し、document paletteを誤って
   確定対象にしない。previewはchart、history、dirtyを変更しない。
2. updateごとに同じbase compositeから再抽出し、前回preview結果へ再量子化
   しない。色数超過、代表色、頻度、差分summaryをbounded resultへ含める。
3. owned preview C ABI、size query／copy／release、stale apply validationを
   追加し、Windows dialogで元chartと候補、色数、parameterを比較する。
4. 重い抽出はworkerでprogress／cancelを持ち、latest resultだけを表示する。
   OKはColor chart entriesを一回だけ確定し、Cancelは無変更とする。既存name、
   lock、page metadataを保持／置換する規則をSPEC.mdへ明記する。
5. previewまたはchart codecのpersisted schemaが変わる場合だけversionを
   更新する。

受入・回帰:

- maximum／quantizationの境界、RGBA8／16、alpha、gradient／AA fixtureの
  deterministic chart goldenを追加する。
- A→B updateの非累積、preview中不変、stale token、Cancel、Apply後の
  chart entries／name／lock、Undo／Redo、save／reopenを検査する。
- FFI ownership／buffer size／double release、Windows比較dialog smoke、
  cancel／close raceを検査する。

手動確認:

- gradientを含むcellで最大色数とquantizationを往復し、候補の色数／色を
  比較してCancelとApply／Undoを確認する。

### M14 — PM-GAP-020: 現代的な出力色域／放送安全域QA

状態: 完了

関連要件: COLOR-001、COLOR-002、COLOR-OUTPUT-QA-001、SEL-002。

仕様決定ゲート:

- 決定済み。正式な放送規格適合判定ではなく、BT.709のY′CbCr係数と
  nominal code相当の閾値を使うinkpod独自の保守的な出力色安全ガードとする。
  committed visible composite、straight alpha、透明pixel、fixed half-up、inclusive
  boundary、対象外overlay、selection演算の正確な契約をSPEC.mdへ固定した。

成果:

- 利用者が選んだ明記済みoutput profileについて、規格外pixelだけを
  selection maskにでき、元pixelは変更しない。

実装:

1. profileごとの変換と判定をinkpod-imageのtyped、fixed-point、
   architecture-independent kernelとして実装する。8／16 bitを途中で
   不要に8 bitへ落とさない。
2. alpha 0のRGBを検査対象にするか、premultiplied表示値ではなくstored straight
   colorを使うかを仕様どおりに固定する。
3. out-of-gamut scanをtile／COW selection resultとして生成し、
   existing selectionとのnew／add／subtract／intersectを一つのCore
   canonical procedureにする。
4. 大画像処理はprogress、cancel、base revision、target generationを持ち、
   all-or-nothingでselectionだけをcommitする。
5. C ABIにclosed profile enum／parameters／result countsを追加し、
   Windows settings dialog、selection生成command、status summaryを接続する。
6. profile semanticsをpersistする場合はformat／replay versionと
   docs/file-format.mdを更新する。新しいselection procedureは必ず
   top-level format／replay versionへ反映する。

受入・回帰:

- 各承認profileについて既知のin／boundary／out sample、閾値の直前／直後、
  8／16 bit同値性、alpha規則、roundingのgoldenを追加する。
- selection algebra、semantic no-op、Cancel、stale、overflow、Undo／Redo、
  replay、save／reopen、FFI unknown profileを検査する。
- New operationが既存selectionを空maskへ置換する場合は変更として扱い、
  元から同じ空maskの場合だけno-opとする。
- 大画像でbounded allocation、変更tileだけのcommit、quick／full benchmarkの
  既存semantic gateを評価する。

手動確認:

- profileを選び、意図的な規格外色だけがselection overlayになること、
  元画像のpixel値が不変で、Undoでselectionだけが戻ることを確認する。

### M15 — PM-GAP-013: guide／grid snapのproduction入力接続

状態: 完了

関連要件: SNAP-001、VIEW-002、PAINT-002。

今回の契約影響:

| Contract | Current | M15 impact |
|---|---:|---|
| native `.inkpod` top-level format | 20 | 永続schema／canonical procedureは不変。version更新なし |
| runtime replay epoch | 17 | 解決済みgeometry procedure semanticsは不変。epoch更新なし |
| C ABI | 9 | 既存layoutを変えずsize-versionedな入力点解釈exportを追加。version更新なし |
| `.inkbatch` top-level format | 2 | Batch graph／operation semanticsは不変。version更新なし |

成果:

- menu／paneのsnap checked stateが、実際のline、shape、polyline等の入力点へ
  反映される。
- zoom、pan、flip、OS DPIによって同じdocument入力の意味が変わらない。

実装:

1. 既存Core::snap_document_pointのthreshold、guide／grid precedence、
   tie-break、modifier規則を公開契約としてテストで固定する。意味を変える
   必要がある場合は先にSPEC.mdを更新する。
2. Windowsはdevice-pixel sample、view ID／generation、modifierをbounded
   inputへ渡すだけとし、Core input interpreterがdocument座標変換とsnapを
   行う。C++に別のsnap計算を作らない。
3. M09の全geometry routeと既存line／shape／polyline gestureを同じ
   interpreterへ接続する。snap offと一時modifier解除はraw document pointを
   そのまま使う。
4. multi-viewのview stateを発行時に固定し、stale viewをactive viewへ
   fallbackしない。Canvasの96-DPI pixel unit規則を維持する。
5. M22のradial guideは後で新しいconstraint sourceとして追加できるclosed
   interfaceにするが、この時点ではguide／gridだけを完了する。

受入・回帰:

- grid、horizontal／vertical guide、双方有効、等距離tie、off、modifier、
  outside bounds、極端zoom／pan、flip、最終valid pixelを検査する。
- 同じdocument gestureを異なるOS DPIで入力して同じcanonical geometryに
  なること、view ID違い／staleを検査する。
- FFI view targetとWindows Canvas gestureのresult geometry／checksum、
  checked state、Undo／Redoをsmokeで観測する。

手動確認:

- gridとguideを表示し、直線／rectangle／polylineの点が吸着すること、
  off／一時解除で吸着しないこと、zoomを変えても同じdocument位置になることを
  確認する。

### M16 — PM-GAP-017: 五点基準によるfloating transform

状態: 手動確認待ち

関連要件: XFORM-002、XFORM-003、HIST-001。

仕様決定ゲート:

- X／Yがanchorのtarget座標かtranslation量か、scale／rotation pivot、
  変換順序、half-open boundsのroundingはSPEC.mdから一意でない。候補と
  dialog／handle互換への影響を示し、利用者決定後にSPECと5-anchor goldenを
  固定する。

成果:

- 左上、右上、中央、左下、右下のanchorをscale／rotate／position計算へ
  実際に反映し、dialogとCanvas handleで同じ結果を得る。

実装:

1. Core transform modelにclosed TransformAnchor enumを追加し、floating boundsの
   half-open geometryから各anchor pointを一つの関数で求める。
2. 承認済みのX／Y、pivot、変換順序、rounding契約をSPEC.mdへ明記し、
   既存numeric transformの一か所へ集約する。
3. preview update、retry、commit、Cancelは同じbase floating contentを使い、
   anchor変更を累積変形にしない。
4. InkpodFloatingTransformのC ABI record、unknown enum validation、
   Windows numeric dialogとhandle controllerを同じanchor値へ接続する。
5. canonical procedure layout／semanticsが変わるため、top-level
   file-format／replay version、非current rejection、route inventory、
   docs/ffi.mdを同じ変更で更新する。

受入・回帰:

- 5 anchor × translate／uniform／nonuniform scale／positive／negative rotationの
  boundsとpixel／vector goldenを追加する。
- aspect lock、負座標、極端値、empty selection、retry、Cancel、stale、
  Undo／Redo、replay、save／reopenを検査する。
- FFI short struct／unknown anchorとWindows dialog値が実結果へ作用する
  smokeを追加する。

手動確認:

- 非対称なselectionを各anchorで同じ数値scale／rotationし、指定点が固定される
  こと、dialogとhandle、Cancel／Undoの結果を確認する。

## P3

### M17 — PM-GAP-001: Cutを意味上の制作単位として保持

状態: 未着手

関連要件: SEQ-001、DOC-001、SESSION-001。専用要件IDを新設する。

仕様決定ゲート:

- Cutと個別CellDocumentの保存所有関係が現行SPECでは一意でない。実装前に、
  一つのcut-root .inkpodへcellを内包する方式と、cut descriptor .inkpodから
  同一directoryの個別cell .inkpodを相対参照する方式について、atomicity、
  大容量cell、単独cell編集、移動／rename、recoveryの影響を示して利用者の
  決定を得る。
- 新しい拡張子、absolute path、canonical procedure内の外部path、silentな
  file移動を導入しない。選択したtopologyをSPEC.mdとdocs/file-format.mdへ
  先に記載する。
- Cutが独立Core handleか複数CellDocumentのcoordinatorか、Cut自身のdirty、
  history、savepoint、Undo owner、Core engine thread、multi-cell publish、
  recoveryを同時に決め、現行DocumentSession owner graphへの影響を
  docs/architecture.mdへ記載する。

成果:

- stable CutIdを持つ作品、話、scene、cut metadata、基準寸法、尺、指示、
  cell membership、M02のcreation defaultsを一つの制作単位として保持する。
- Cut defaultsから作ったcellが同じ制作条件を受け取り、個別上書きとの差を
  明示できる。

実装:

1. CoreにCutId、CutMetadata、CutDefaults、ordered CellMembershipを追加し、
   文字列長、寸法、尺、重複CellId、所有namespace、参照cycle、件数上限を
   型付きで検証する。IDはcommit時だけ消費し再利用しない。
2. 選択したowner modelに従い、WorkspaceWindow配下のCutSessionがCut handleを
   所有し、create／query／edit／destroyをCore engine threadへ固定する。
   DocumentSessionは引き続き各cell Coreだけを所有する。
3. Cut create／edit／membership queryをCutSessionのtyped transactionへ
   正規化する。Cut descriptorのatomic replaceと各CellDocumentへのdefaults
   copyを別の明示境界にし、別Core／別fileを一つのdocument transactionとして
   偽装しない。
4. M02のCellCreationOptionsへCutDefaultsを明示copyするrouteを接続し、
   default変更で既存cellをsilentに書き換えない。
5. 選択したpersistence topologyをcurrent-only top-level formatへ追加し、
   staged decode／validation／replay後にだけlive Cutを置換する。duplicate ID、
   path traversal、missing member、巨大count、checksum mismatchを拒否する。
6. C ABIにopaque Cut handle、bounded UTF-8 DTO、CRUD／query／releaseを追加し、
   thread ownershipをdocs/ffi.mdへ記載する。
7. Windowsに新規Cut、Cut Properties、Cut名／尺／defaults／membershipの
   Sequence pane presentationを追加し、複数workspace／sessionのtargetを
   CommandContextで固定する。

受入・回帰:

- Cut／CellのID namespace、metadata bounds、default copy、重複／削除後ID、
  success／no-op／invalid／Cancel／stale／failure、Undo／Redoを検査する。
- create／edit／save／reopen、atomic replace failure、malformed manifest、
  missing／duplicate member、recovery、current-version only rejectionを検査する。
- FFI UTF-8／span／ownership negative、Windows New Cut／Properties／Sequence
  smokeで実データを観測する。
- format／replay version、fuzz target、corrupt corpus、file-format文書を
  同じ変更で更新する。

手動確認:

- 新規Cutに作品／話／scene／尺／defaultsを設定し、そのdefaultsで複数cellを
  作成して保存／再openし、membershipと条件が保たれることを確認する。

### M18 — PM-GAP-002: セル系列の構造編集

状態: 未着手

関連要件: SEQ-001、SESSION-001。構造編集用の独立要件IDを新設する。

成果:

- Cut内のcellをstable CellIdのままadd、remove、reorder、renumberし、
  衝突のない一transactionとして確定できる。
- display order／numberをfilenameやarray indexのidentityにしない。

実装:

1. M17のCellMembershipへtyped SequenceEditRequestを追加する。insert、
   remove、move-before／after、renumber rangeをordered operationとして
   boundedに受け取り、最終state全体をvalidateしてから一回commitする。
2. number重複、reserved value、CellId重複、別Cut ID、参照中item、overflowを
   検証する。失敗時はmembership、references、ID、history、fileを不変にする。
3. removeは既定でmembershipから外す意味とし、source fileの物理削除を
   silentに行わない。物理削除が必要なら別の明示OS commandと確認、recovery
   policyとして分ける。
4. Light Table、Batch、subpalette、autosaveが保持するCellId参照を、
   reorder／renumberでは維持し、remove時は明示orphan／missing statusへ
   遷移させる。active targetを別cellへsilentに付け替えない。
5. C ABIにordered edit spanとper-operation result indexを追加し、
   Sequence paneのinsert／remove、drag reorder、renumber dialog、
   thumbnail selection、keyboard／accessibilityを接続する。
6. Cut membership historyはDocumentSession historyから分離する。Sequence
   paneにfocusがありCutSessionをtargetにした明示Undo／RedoだけがCut historyを
   動かし、active documentのCtrl+Zへsilentに混ぜない。menu表示名と
   CommandContextにowner／generationを示す。
7. 選択したM17 topologyに応じてmanifest／containerを一回atomic saveする。
   複数外部file renameをtransaction成功の根拠にしない。
8. 新しいCut／sequence persistent semanticsとhistory recordに合わせ、
   top-level format／replay versionと非current rejectionを更新する。

受入・回帰:

- insert／remove／reorder／renumber、空／一件／欠番、番号衝突、duplicate ID、
  oversized edit、途中invalid operationを検査する。
- 一回Undo／Redo、save／reopen、failure rollback、active cell、thumbnail、
  LT／Batch参照、ID非再利用を検査する。
- FFI ordered span／result／negative、Windows drag／keyboard／dialog smokeを
  追加する。

手動確認:

- 5 cellの順序をdragし、欠番をrenumberし、一件をmembershipから外して
  一回Undoする。保存／再open後の順序、thumbnail、LT参照を確認する。

### M19 — PM-GAP-009: 再編集可能なtext／instruction annotation

状態: 未着手

関連要件: DOC-002、IO-001。

成果:

- Text／Annotation layerが空のkindではなく、再編集可能text、手描き指示、
  指示線／値、通常／指示variantをdocument座標で保持する。
- 通常Textはthumbnail／flat exportへ含め、指示variantは通常の完成画像export
  から除外する。両方を編集Canvasでは表示する。

実装:

1. stable AnnotationObjectIdを持つCore content modelを追加する。bounded UTF-8
   text、document bounds、logical font family hint／size／style、color、
   text／stroke／leader／value kind、export inclusionをtypedに保持する。
2. font解決、glyph cache、DirectWrite resourceはWindows rendererが所有し、
   Core snapshotはimmutable text／geometry recordだけを返す。font不在時の
   fallback、warning、layout再現性とflat exportの描画ownerをSPEC.mdへ定め、
   silentな別layoutを確定結果として扱わない。
3. create／edit／move／delete／stroke begin-update-endをcanonical procedureへ
   追加し、複数object editも一transactionへまとめる。non-output metadataを
   flat export compositorで確実に除外する。
4. C ABIにbounded UTF-8二段階API、geometry span、snapshot recordsを追加し、
   ownership／invalid UTF-8／length上限を検証する。
5. Windowsにtext editor、annotation draw route、selection handle、
   layer property、keyboard navigation、screen-reader nameを接続する。
6. native schema、replay version、file-format、route inventoryを更新し、
   M20が別object typeを載せられる最小overlay interfaceを設ける。

受入・回帰:

- UTF-8、空／最大長、複合文字、bounds、stroke、leader、value、
  edit／move／delete、invalid geometryを検査する。
- Undo／Redo、replay、save／reopen、normal Canvas表示、通常Textのthumbnail／
  flat export包含、instructionのfinal export除外、font fallback、
  malformed dataを検査する。
- FFI ownership／buffer sizing／UTF-8 negative、renderer device lost、
  Windows text／draw／accessibility smokeを追加する。

手動確認:

- 日本語textと手描き指示線を作成・再編集し、保存／再open後に位置と内容が
  保たれ、通常PNG exportに指示が焼き込まれないことを確認する。

### M20 — PM-GAP-008: 角度と位置を持つ撮影frame

状態: 未着手

関連要件: DOC-001、DOC-002。撮影frame object用の独立要件IDを新設する。

仕様決定ゲート:

- 角度付き撮影frameのcenter／corner／anchor表現、finished export boundsへの
  作用、通常Canvasと指示exportへの含有規則がSPEC.mdで未決定なら、
  独立overlayとして保持する案とcrop／export targetとして使う案の影響を示し、
  利用者の決定後に受入条件を固定する。
- 既存axis-aligned FrameMetadataのshooting frameと新objectのどちらを正本と
  するか、併存／変換、paper fit、document transform、export時の優先規則も
  同じ決定へ含める。

成果:

- axis-alignedな既存FrameMetadataと混同せず、寸法、角度、document位置を
  持つ撮影frameを独立objectとして作成・移動・回転・編集できる。

実装:

1. stable ShootingFrameId、document-space center、positive size、
   fixed-point rotation、anchor、表示／export policyをCore modelへ追加する。
2. geometry、hit test、handle transform、Canvas外座標、angle normalizationを
   document座標の一か所へ集約し、OS DPIやrenderer mathをcanonical stateへ
   入れない。
3. create／update／deleteとlong-lived previewを分け、OKは一transaction、
   Cancelはbaseへ完全復元する。document mirror／rotate／resize時のframe
   transformを既存XFORM contractへ追加する。
4. M19のgeneric overlay transportを再利用しつつ、annotationとは別のtyped
   record／ID namespaceを持つFFI、snapshot、rendererを追加する。
5. Windowsにhandle drag、numeric properties dialog、visibility／command state、
   accessible labelを接続する。
6. SPEC、native schema、replay version、file-format、route inventoryを更新する。

受入・回帰:

- 0／非90度／90度相当、五点anchor、Canvas内外、極端値、angle wrap、
  resize／mirror／rotateとの座標goldenを追加する。
- preview／Cancel、no-op、invalid、stale、Undo／Redo、replay、save／reopen、
  export policyを検査する。
- FFI unknown enum／non-finite、renderer、Windows handle／dialog smokeを
  追加する。

手動確認:

- 角度付きframeをCanvas外へ跨がせて数値／handleで編集し、Cancel、
  一回Undo、保存／再open、決定したexport policyを確認する。

### M21 — PM-GAP-005: 前後セル切替の端点loop policy

状態: 未着手

関連要件: SEQ-001。

成果:

- 前／次cell commandが先頭／末尾でStopするかWrapするかを利用者が選べ、
  設定とchecked stateが再起動後も保たれる。

実装:

1. Stop／Wrapのclosed EndpointPolicyをversioned bounded application settingへ
   追加する。scopeをapplication-wide HKCU settingとしてSPEC.mdへ固定し、
   document／editor stateへ混ぜず、.inkpod format version、dirty、historyを
   変えない。
2. 既存sequence stepへpolicyとissue-time cell identityを渡し、empty／one-cell、
   missing cells、前／後方向のresult classをCore／adapterで明示する。
3. motion check自身のloop settingと通常前後navigation preferenceを分離する。
4. Settings UI、menu checked state、shortcut、status／accessibilityを同じ
   preferenceへ接続し、window／application scopeをSPEC.mdへ記載する。

受入・回帰:

- empty、one、many、first-prev、last-next、欠番、Stop／Wrap、forward／backward、
  preference load／malformed fallbackを検査する。
- document revision、history、dirty不変、stale target、複数windowのscope、
  Windows keyboard／checked-state smokeを追加する。

手動確認:

- 3 cellの先頭と末尾で前／次を押し、Stop／Wrapを切り替え、再起動後も
  設定どおりでdirtyにならないことを確認する。

### M22 — PM-GAP-010: 消失点と放射補助線

状態: 未着手

関連要件: SPEC.mdの消失点節。専用要件IDを新設する。

成果:

- Canvas内外へ複数消失点を置き、1／5／10／15／30度等の間隔、色、
  opacityを設定して放射補助線を表示・編集できる。
- 必要な場合はM15のsnap sourceとして放射線へ入力を拘束できる。

実装:

1. stable VanishingPointId、DocumentPoint、1／5／10／15／30度presetと
   bounded fixed-point custom interval、exact color、opacity milli、
   visibilityをpersistent document objectとしてCoreへ追加する。
   LayerKind::VanishingPointの空contentを解消する。
2. viewportと交差する有限なradial segmentだけをsnapshot時に導出し、
   Canvas外のpointや極端viewportでも本数／allocation上限を超えない。
3. CRUD、hit／move、add／delete allとdialog previewを分け、OK一transaction、
   Cancel無変更、ID commit時消費を守る。
4. C ABIのCRUD／snapshot records、Windows pane／dialog／Canvas handle、
   renderer overlay、menu／command stateを接続する。
5. M15のconstraint source interfaceへradial guideを追加し、guide／gridとの
   distance／tie-breakをCoreで決定する。snap offでは入力を変えない。
6. document persistenceだけを実装し、未指定の独立preset形式を勝手に
   追加しない。native schema、replay version、file-formatを更新する。

受入・回帰:

- multiple point、Canvas外、各interval、color、opacity 0／1000、angle wrap、
  viewport境界、上限をsnapshot／renderer goldenで検査する。
- create／move／delete／all-delete、no-op／invalid／Cancel／stale、
  Undo／Redo、replay、save／reopen、export exclusionを検査する。
- radial snap、guide／grid競合、FFI negative、Windows handle／dialog／
  checked-state smoke、device lostを検査する。

手動確認:

- Canvas外を含む二つの消失点を置き、間隔／色／opacityを変え、放射線への
  snap、Cancel／Undo、保存／再openを確認する。

## プロンプト例

以下を初回の実装セッションと、各バイナリ確認後の次セッションへ、
そのまま渡す。

~~~text
このリポジトリのAGENTS.md、SPEC.md、MILESTONES.md、
docs/paintman-functional-gap-analysis.mdと、対象に関係する既存コード、
テスト、追跡文書を読んでください。git statusで既存差分と未追跡ファイルを
確認し、それらを利用者の変更として保護してください。

初回実行では手動確認待ちがないため、状態の完了更新を行わないでください。
二回目以降、このプロンプトは直前に手動確認待ちとなったバイナリの動作確認に
成功した後だけ送っています。手動確認待ちがちょうど一件なら、その一件だけを
完了へ更新してください。複数ある場合や、直前の確認結果を一意に特定できない
場合は状態を変更せず、実装も始めずに矛盾を報告してください。判断待ちがある
場合はそれを飛ばさず、示された利用者判断を反映して同じマイルストーンを
再開するか、まだ判断が足りなければ質問して停止してください。

次にMILESTONES.mdを先頭から確認し、未完了の最初のマイルストーン、すなわち
最初の実装中、なければ最初の未着手だけを実装してください。今回選んだ
マイルストーン以外へ勝手に進まず、自動検証を終えても次のマイルストーンを
開始しないでください。後続機能への依存があっても順位を入れ替えず、今回
必要な最小の接続境界だけを設け、後続機能を先取りしないでください。対象外の
refactor、rename、formattingを混ぜないでください。対象の状態は作業開始時に
実装中へ更新してください。

公開契約を観測するテストを先に追加し、Core、canonical procedure、
C ABIとheader、Windows adapter／UI／renderer、必要な永続化と文書まで、
マイルストーンに記載された縦切りを完成させてください。仕様と既存テスト
だけで安全に決められない製品挙動がある場合は推測で実装せず、具体的な
選択肢、影響、推奨案、解除条件を示して私の判断を待ってください。

success、no-op、invalid、Cancel、stale、overflow、failure、Undo／Redo、
必要なsave／reopen、ABI ownership／negative case、Windows production routeを
変更範囲に応じて検証してください。serialized schema、canonical procedure、
replay結果、application固有の永続形式を変更する場合は、フォーマットフリーズ
前の規則に従い、実行時点のcurrent top-level file-format／replay version、
非current version拒否test、file-format文書を同じ変更で更新してください。

少なくとも次の検証を実行してください。

cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo bench --package inkpod-core --bench core_workflows -- --quick
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --package inkpod-core --all-features --no-deps
cmake --preset windows-x64-release
cmake --build --preset windows-x64-release
ctest --preset windows-x64-release

ARM64検証は追加で実行して構いませんが、必須のx64 Release検証の代用にしないで
ください。性能に関係する変更では該当する既存quick／full benchmarkのchecksum、
revision、reuse／rebuild、payload access等の意味counterと承認済みenvelopeを
評価してください。wall-clockはwarm-up後5回以上の中央値を取り、上限超過は
独立した5回以上の再測定で確認してください。benchmark workload、harness、
envelope、revision-max式を勝手に変更しないでください。実行できない検証や
失敗を隠さないでください。

対象の自動完了条件をすべて満たした場合だけ、その状態を手動確認待ちへ更新して
ください。ここでは完了へ変更しないでください。利用者向け挙動、重要な設計判断、
変更ファイル、テスト／benchmark／build結果、未検証事項、既知差分、生成した
x64 Releaseバイナリの絶対パス、私が行う短い手動確認手順を報告し、そこで停止して
ください。自動完了条件を満たせない場合は実装中のまま、仕様判断が必要なら
判断待ちへ変更し、具体的なblockerと次に必要な判断を報告してください。
commit、push、PRは行わないでください。
~~~
