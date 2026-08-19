# inkpod 操作性・描画モデル改修 実装プロンプト

このリポジトリで、以下の改修を仕様、Rust Core、C ABI、Windows GUI、renderer、永続化、
InkScript catalog、テスト、文書まで一貫して実装してください。計画や調査だけで終了せず、変更範囲に
必要な実装と検証を完了してください。`AGENTS.md`を常時遵守し、着手前に`git status`、既存差分、
`SPEC.md`、`INKSCRIPT.md`、対象コード／テスト、関連する`docs/implementation-status.md`と
`docs/compatibility.md`を確認してください。ユーザーの既存変更を上書きせず、commit、push、PRは
行わないでください。

## 1. 目的と進行境界

アーティストによる試用結果を受け、InkScriptの製品UI実装へ進む前に、不要な描画モデルを削除し、
右側ペインの操作性を作り直します。InkScriptの実装済み範囲はM27Bまでとし、M27Bを利用者確認済みの
一時停止点として記録してください。`INKSCRIPT.md`には、ユーザーが明示的に再開を指示するまでM28Aへ
進まないfreeze gateを追加してください。

これはInkScript開発の一時停止であり、`.inkpod`のフォーマットフリーズ宣言ではありません。今回の
変更では現行native format、replay、ABI、InkScript catalogを新しいexact-current契約へ更新します。
M20、M21、M27B等の完了済みマイルストーン本文は当時の実装履歴として保持し、過去を未実装だったかの
ように書き換えないでください。M27B直後に再ベースライン節を置き、後続マイルストーンが新しい契約を
前提とすることを明記してください。現在残っている「M27B user confirmation pending」表記は、今回の
指示を確認完了として整合させてください。

この作業は少なくとも次の意味上のリスクへ分け、テストで各境界を固定しながら進めてください。

1. 現行仕様、版、削除境界の再ベースライン
2. vector機能の完全削除
3. Text／Annotation（指示レイヤー）機能の完全削除
4. native／replay／InkScript catalog／C ABIのexact-current更新
5. 右側ペインの動的タブ化とworkspace永続化
6. ペイン表示メニューの単純化
7. 全体検証とstatus／compatibility更新

途中状態で旧契約と新契約を同時にproductへ公開しないでください。placeholder、互換shim、常時成功stub、
無効化しただけの旧メニュー、未接続UIを残さないでください。

## 2. 削除する機能

### 2.1 vector機能

vector関連機能をGUI、renderer、C ABI、Rust Core、`inkpod-image`、`inkpod-format`、InkScript、
clipboard、snapshot、永続化から完全に削除してください。少なくとも次を含みます。

- `VectorColoring` layerとvector専用plane
- vector path、fill、segment、endpoint、connection、stable vector object IDとその高水位
- vector描画、vector fill、vector消去mode
- 線つなぎ、vector線幅修正
- vector選択とselectionのvector mode
- vectorをrasterize、rasterをvectorize、新規vector layerへの変換
- vector scoped color replacement、vector固有のgeometry／transform／clipboard分岐
- vector editor option、active tool、view-local centerline／endpoint／antialias診断
- immutable snapshotのvector span、rendererのvector描画とdiagnostics
- vector thumbnail／flatten／composite／import asset表現
- Windowsのvector controller、tool、flyout page、menu、shortcut、dialog、command state、resource、文字列
- C header／FFIのvector record、enum、buffer、snapshot view、公開関数、export
- InkScriptのvector専用8 commandと、残るcommandのvector引数、型、result、selector、binding分岐

`vector`という単語を無差別に削除せず、数学的なベクトル、コンテナ、第三者API等の無関係な用法は
保持してください。raster版も持つ混合機能は、rasterの公開契約とテストを維持した上でvector分岐だけを
除去してください。

### 2.2 Text／Annotation（指示レイヤー）機能

Text layerと指示／Annotation layerに属する機能をGUI、renderer、C ABI、Rust Core、InkScript、
snapshot、永続化から完全に削除してください。少なくとも次を含みます。

- `LayerKind::Text`、`LayerKind::Annotation`と対応する公開enum値
- `AnnotationObjectId`、Text／Stroke／Leader／Value object
- normal／instruction annotation output variant
- annotation create／update／move／delete、stroke begin／append／end
- `EditAnnotations` canonical invocation／primitiveのactive implementation
- portable glyph-cell raster、annotation thumbnail／flatten／snapshot／renderer経路
- annotation editor tool、menu、dialog、Canvas input、resource、localization、command state
- C header／FFIのannotation input、edit、stroke、snapshot recordと公開関数
- InkScriptのannotation command、型、selector、result、binding分岐
- `.inkpod`のannotation record、object count／ID high-watermark、digest／checkpoint／archive fields

一般的なテキスト入力dialog、文書名、ファイル名、localization文字列処理まで削除してはいけません。

### 2.3 維持する隣接機能

次はText／Annotation layerとは別のownerなので維持してください。

- angled Shooting Frameとその編集、Canvas表示、指示入りラスター書き出し
- Vanishing Point layer／object、radial guide、snap、Canvas overlay
- Cut metadataの「指示」文字列
- guide、grid、frame metadata、selection layer、adjustment layer
- raster geometry、raster selection、raster transform、raster clipboard
- Light Table、subpalette/reference、Locator、Sequence、Batch

`EditAnnotations`に割り当てられていたnative `PrimitiveId`とvector familyの既存`PrimitiveId`は、active
catalog／executorから除去しても番号を再利用してはいけません。native file-format文書のtombstone／予約値
として記録し、現行writerは発行せず、現行readerはprocedureとして受理しないでください。

## 3. exact-current versionの更新

削除は受理可能なstate、procedure、ABI、catalogを変えるため、同じ変更で次へ更新してください。

- `.inkpod` top-level format: v26からv27
- runtime replay epoch: 23から24
- C ABI: v16からv17
- InkScript procedure catalog: v2からv3
- InkScript file format: grammar／serialized syntaxを変えない限りv2を維持
- InkScript registry schema／language schema: meta-schemaまたはlanguage-core schemaを変えない限りv2を維持

影響する`DocumentArchive`、document state digest/domain、EditorState、checkpoint、catalog digest等の下位schemaも
単調に更新し、番号を巻き戻したり再利用したりしないでください。CellとCutの両`.inkpod` header、native writer、
reader、recovery、autosave、fixture、文書をv27へ揃えてください。replay acceptanceとcatalog fingerprintはepoch 24へ
揃えてください。

InkScript catalog v3では、vector専用8 commandとannotation 1 commandをactive command集合から削除し、active
entry数を84から75へ更新してください。`apply_geometry`、selection、color replacement、transform等の残存commandに
vector型／分岐／resultが含まれる場合は、それらもcatalog v3の閉じたsignatureから除去してください。
`catalog-v2.json`をin-place変更せず、exact-current `catalog-v3.json`、owner manifest、registry、generated command
reference、fingerprint、runtime adapter、equivalence mappingを一括更新してください。catalog v2、native v26、
replay epoch 23、ABI v16は明示的に拒否するnegative testを持たせてください。

旧v26 `.inkpod`のmigration reader、one-shot importer、互換writer、互換shimは作らないでください。今回の改修後は、
vector／Text／Annotationを含むか否かにかかわらずv26を拒否します。試用データ救済のためにproduction契約へ例外を
追加しないでください。

## 4. テストと仕様の削除方針

削除対象のsuccess／round-trip／golden／smokeテストと専用fixtureは削除してください。ただし、ファイルやsuiteを
丸ごと消してraster等の残存契約まで失わせないでください。混合テストはraster-onlyへ再構成してください。

削除した機能の代わりに、少なくとも次のabsence／negative契約を追加してください。

- Layer／Plane作成APIが旧vector、Text、Annotation enum値を拒否する
- 旧primitive ID、旧canonical invocation、旧procedure catalog entryを現行reader／executorが受理しない
- v26／epoch 23 `.inkpod`をstaged openがlive state非変更のまま拒否する
- ABI v16、削除済みrecord／enum／symbolがexact-current header/export/catalogへ残らない
- Windows menu、shortcut、tool、flyout、dialog、command registry、localization catalogに削除機能が残らない
- snapshot／renderer／clipboard／EditorState／digest／checkpointにvector／annotation payloadが残らない
- raster描画、raster selection、raster transform、raster clipboard、Shooting Frame、Vanishing Pointが回帰しない
- native v27／epoch 24のsave／reopen、Undo／Redo、cache-free replay、savepoint、ID high-watermarkが一致する
- InkScript catalog v3の75 entryがowner manifest、runtime adapter、canonical executorと全単射である

`SPEC.md`からvector、Text、指示レイヤーの現行機能記述、メニュー、tool、option、file-format要件を削除し、混合要件は
raster／adjustment等の残存機能へ書き換えてください。`VECTOR-001`、`VECTOR-002`等の廃止された現行requirementを
互換表で`Verified`のまま残さないでください。Git履歴で確認できる過去ログを恒久仕様へ複製せず、現行の正本として
`docs/file-format.md`、`docs/ffi.md`、generated InkScript reference、`docs/compatibility.md`、
`docs/implementation-status.md`を整合させてください。

削除によりInkScript quick fixture／checksumの更新が不可避な場合は、削除済みcommandのscenarioだけを除き、残存scenario、
意味counter、測定方法を維持してください。この指示は削除対象scenarioとcatalog／version由来checksumの更新を承認しますが、
wall-clock envelopeの緩和、意味counterの削減、別workloadへの置換は承認しません。必要ならbefore／afterと全sampleを記録し、
envelope変更は行わず別途報告してください。

## 5. 右側ペインを動的タブへ変更

固定の右側タブ`彩色`、`参照`、`進行管理`を廃止し、右zoneに置けるペインから構成される動的な非空タブ集合へ
置き換えてください。built-in workspace presetの名称としての`彩色`、`線整理`、`参照・チェック`、`バッチ`、`集中`は
別概念なので維持し、各presetが動的タブの初期配置を設定するようにしてください。

### 5.1 ペインdescriptorと対象範囲

- 右zoneへ置ける全ペインは、`PaneDescriptor`にstable type ID、静的なminimum width／height DIP、preferred size、
  scope、allowed zoneを持つこと
- minimumはdevice pixelではなく96-DPI基準DIPで宣言し、DPI変換は既存の一か所だけで行うこと
- 日本語／英語、標準UI font、96／120／144／192 DPIで操作captionと必須controlが欠けない値にすること
- Tool paletteはLeft、Tool Optionsはowned flyoutのままとし、右動的タブへ入れないこと
- 一つのpane typeは原則一instance、一つの右タブにだけ所属すること
- tab数とpane数は既知descriptor数でboundedにし、allocationや再帰dock treeへ依存させないこと

### 5.2 表示トグルからの追加

ユーザーがメニューバーで非表示ペインを選んだ場合は、次の順で処理してください。

1. 現在選択中の右タブがなければ、新しいタブを作る。
2. 選択中タブのcontent heightからtab stripとsplitterを除いたavailable heightを求める。
3. 既存全ペインと新規ペインについて、DPI変換後のminimum heightの合計と必要splitterを求める。
4. 合計がavailable以下なら、新規ペインを選択中タブの末尾へ追加する。新規ペインへ最低minimum heightを確保し、
   既存ペインは各minimumを下回らない範囲のcompressible surplusに比例して縮小する。残余は既存split weightと新規paneの
   preferred heightから決定的に配分する。
5. 合計がavailableを超えるなら、新しいタブを一つ作り、そのタブへ新規ペインだけを追加する。
6. 追加先タブを選択し、ペインの自然な最初のfocus targetへfocusを移す。

表示済みペインのmenu itemを選んだ場合は、その配置がdocked、floating、auto-hideのいずれでも非表示にしてください。
最後のペインが外れたタブは即座に削除し、直前、次、先頭の順でselection replacementを決定してください。再表示時は
古いhidden membershipへ戻さず、その時点のactive tabに対して上記algorithmを実行してください。

空タブは作成、表示、永続化しないでください。ユーザーが明示的にタブを増やせる操作として、pane header／context menu／
keyboardから到達可能な「新しいタブへ移動」を提供し、選択paneを新しい非空タブへ移してください。既存タブ間のpane移動、
pane順序、tab順序の変更もpointerに依存せずkeyboardから到達可能にしてください。移動がno-op、invalid、capacity超過の場合は
layoutを部分変更しないでください。

### 5.3 minimum widthと狭幅適応

右zoneの必要minimum widthは、可視／選択タブ内ペインのminimum widthの最大値としてください。必要ならCanvas／editorの
既存minimum widthを侵害しない範囲で右zoneを広げてください。window全体が狭すぎる場合もpane内容をclip、caption省略、font
縮小してminimumを破らず、既存のtemporary narrow-window auto-hide／suppressionを使ってください。これは保存済みlogical
layoutを変更せず、windowが広がれば自動復元し、status／accessibilityから一時非表示を通知できることとします。

### 5.4 タブ名とaccessibility

- tab labelは、そのタブで縦方向の先頭にあるpane descriptorのlocalized titleを常に使うこと
- paneの追加、削除、並べ替え、言語変更でlabelを更新すること
- tab identityをlabelや配列indexに依存させず、boundedなstable layout IDを使うこと
- 同名labelが存在してもidentityを混同しないこと
- tooltipとaccessible descriptionにはタブ内の全pane名を順序付きで含めること
- tab、pane header、splitter、一時auto-hide、checked／selected stateをUI Automationから取得できること

### 5.5 workspace永続化

現在のworkspace layout record V8をV9へ更新し、次をboundedに保存してください。

- 動的tabのstable ID、順序、selected tab
- tabごとのpane membershipと縦順序
- paneのsplit weight、visibility、dock／float／auto-hide placement
- selected workspace presetと既存window／editor split情報

V8からV9へのmigrationでは、旧固定3タブのvisible pane membershipを動的な非空タブへ変換し、固定ラベルではなく先頭paneの
localized titleを使ってください。V2～V7の既存migrationを壊さず、最終的にV9へ正規化してください。未知paneは無視し、
不足する既知paneはpreset既定へ補い、重複pane、空tab、重複tab ID、範囲外count、invalid selected tab、overflow、trailing
garbageを拒否して既定layoutへ安全に戻してください。temporary narrow-window adaptationはV9へ保存しないでください。

## 6. メニューの単純化

メニューバーにある次のpane submenuを廃止し、それぞれ一つのchecked toggle menu itemにしてください。

- ロケーター
- シーケンス
- ライトテーブル
- サブパレット／参照ビュー

旧submenu内の`表示`、`固定`、Locatorのfixed mode／auto-scroll等の細かな操作はmenuから削除し、各paneのheaderまたは内容領域へ
移してください。pane内操作は標準controlを使い、keyboard、tooltip、accessible name、checked state、UI Automationを備え、
既存のtarget scope／pin generation契約を維持してください。menu toggle、pane close／hide、context commandは同じcommand IDと
visibility stateを共有してください。

固定右タブ`彩色`、`参照`、`進行管理`の表示menu item、command ID、resource、shortcut、localization、handlerも削除してください。
Color、Layer／Plane、Tool palette、Tool Options、Batch、Job Progressの既存表示commandは、今回明示した変更または動的タブとの
接続に必要な範囲以外で機能を拡張しないでください。特にBatch submenuの直接toggle化は今回の必須scopeに含めません。

## 7. UI／layoutの必須テスト

少なくとも次をmodel/static/native smokeで検証してください。

- 初期状態と全workspace presetが空tab／重複paneを持たない
- active tabへ追加できる場合と、minimum height不足で新tabになる場合
- 既存paneがminimumを下回らず、splitterを含む合計がavailableと一致する
- 96／120／144／192 DPI、日英、極端なwindow sizeで決定的なlayoutになる
- minimum width不足でclipせずtemporary suppressionし、拡幅で復元する
- hideで空になったtabの削除とselection replacement
- paneを新tabへ移動、既存tabへ移動、tab／pane reorder、Cancel／invalid／capacityの原状復帰
- tab labelが先頭paneに追従し、tooltip／accessibilityが全paneを列挙する
- menu checked stateがdocked／floating／auto-hideを含む実visibilityと一致する
- V9 round-trip、V8 migration、V2～V7 migration経由、malformed／unknown／duplicate／overflow拒否
- layout restore後もdocument path、Core state、active stroke、job ownerをlayout recordへ混入させない
- F6／Shift+F6、Tab、keyboard splitter、pane action、screen reader／UIAの到達性

## 8. 検証と完了条件

変更範囲に応じたunit／contract／property／golden／FFI／Windows smokeを追加または更新し、少なくとも次をno-profile shellで
実行してください。実際のCMake preset名をリポジトリから確認して使用してください。

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo bench --package inkpod-core --bench core_workflows -- --quick
cargo doc --package inkpod-core --all-features --no-deps
cmake --preset <windows-preset>
cmake --build --preset <windows-build-preset>
ctest --preset <windows-test-preset>
```

`cargo doc`は実行shellに応じて`RUSTDOCFLAGS=-D warnings`相当を設定してください。C11／C++20 header include、
header/export/catalog drift、ABI smoke、`--smoke-test`、`--abi-smoke-test`、英語／日本語resource parityも
検証してください。source gate、generated files、InkScript generated referenceは正規generatorから更新し、手編集した派生物を
残さないでください。実行できない検証、環境依存failure、未検証のmanual UI項目を隠さず報告してください。

完了時には以下をすべて満たしてください。

- product source、公開Rust API、C ABI header/export、Windows command/resource、renderer、native schema、InkScript catalogから
  vector／Text／Annotation機能が除去されている
- v27／epoch 24／ABI v17／InkScript catalog v3のexact-current契約が一致している
- raster機能、Shooting Frame、Vanishing Point、Cut instruction metadataが維持されている
- 固定3タブがなく、動的非空タブ、minimum size packing、V9 persistence、直接pane toggleが実製品UIへ接続されている
- success、no-op、invalid、capacity、narrow-window、migration、malformed、Undo／Redo、save／reopen、replay、ABI ownershipを
  public経路から検証している
- `SPEC.md`、`INKSCRIPT.md`、file format、FFI、compatibility、implementation statusが現行コードと一致している
- compatibility statusは許可された値だけを使い、テストのない機能を`Verified`にしていない

最終報告では、利用者向け変更、維持した隣接機能、重要な設計判断、version impact、主要変更ファイル、削除／追加したテスト、
全検証結果、未検証事項、既知差分を簡潔に示してください。
