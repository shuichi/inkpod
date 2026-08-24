# FFI 利用ガイド

Inkpod の公開 C ABI は `include/inkpod/core_ffi.h` を仕様の正本とする。
各型・関数の引数、`struct_size`、NULL 可否、スレッド、所有権、出力、リビジョン、未保存状態、Undo、
排他状態、戻りステータスはヘッダーの Doxygen コメントに記載している。この文書は関数一覧を
複製せず、フロントエンドが ABI を安全に利用するための横断的な契約と典型的な呼び出し順序を説明する。

## 全体像

Windows フロントエンドは、UI/Input、Core エンジン、レンダラーの三つの長寿命スレッドに分かれる。

- UI/Input スレッドは Windows メッセージとポインター履歴を受け取り、入力を上限付き C++ キューへ渡す。
- Core エンジンスレッドは `InkpodCore` を作成し、すべての Core 操作とスナップショット構築を行う。
- レンダラースレッドは D3D11/DXGI/Direct2D リソースと `Present` を所有し、不変スナップショットだけを読む。

通常の文書 command／render 経路で Core は C++ コールバックを呼ばない。InkScript execution ABI だけは、
Core engine thread 上の明示的な `PlanTask`／`RunTask` advance 中に、OS authority／session／file install を
`InkpodInkScriptHostAdapter` callbackへ問い合わせる。この callback 中に Core lockを保持せず、同じ Coreへ
reenterしてはならない。UI スレッドは Core やレンダラーの完了を同期的に待たず、レンダラーは古い未描画
スナップショットだけを置き換えてよい。ストロークの開始・終了・キャンセルと入力サンプル自体は捨てない。

```text
UI/Input スレッド
  └─ 正規化した入力バッチ
       → Core エンジンスレッド
           ├─ Core コマンド／ストローク
           └─ 不変スナップショットを構築
                → スナップショットキュー（所有権を1回だけ移動）
                    → レンダラースレッド
                        ├─ 借用ビューを描画
                        └─ スナップショットを解放
```

## ABI と構造体

数値は固定幅 C 型を使う。Rust の `Vec`、`String`、スライス、列挙型のメモリ配置、トレイトオブジェクト、
参照と、C++ の STL、参照、例外は ABI を越えない。文字列は UTF-8 ポインターとバイト数、配列は
ポインター、要素数、バイト単位のストライドで表す。

拡張可能な構造体を渡すときは、少なくとも次を守る。

```cpp
InkpodSnapshotOptions options{};
options.struct_size = sizeof(options);
options.feature_flags = INKPOD_FEATURE_NONE;
```

- `struct_size` は必ず呼び出し側が設定する。出力構造体でも同じである。
- ABI v18 で既知の構造体末尾まで読み書きできるサイズが必要である。
- `reserved` は 0 とし、未知の必須機能フラグは指定しない。
- レコード列では、各レコードの `struct_size` と `*_stride_bytes` の両方を設定する。
- 要素数、ストライド、アラインメント、列全体のバイト範囲が有効でなければならない。
- 要素数が 0 の任意指定の列に限り、データポインターを NULL にできる。各 API の例外はヘッダーを参照する。
- 入力、出力、不透明オブジェクトの記憶域を重ねない。

ABI バージョンは Core 作成前に比較できる。`INKPOD_ABI_VERSION` とライブラリの戻り値が異なる場合は、
Core を作らず互換性エラーとして扱う。

現行ライブラリは ABI v18 だけを受理し、`InkpodCoreConfig::abi_version` が完全一致しなければ
`INKPOD_STATUS_INCOMPATIBLE_ABI` を返す。関数名や型名の `_v3` は、値／ID API 群が導入された世代を
示す接尾辞であり、ABI v3 の呼び出し側との実行時互換性を意味しない。ABI v1-v17 の呼び出し側は、
現行の v18 ヘッダーへ更新して再ビルドする。

ABI v18 は ABI v17 までの `_v3` 値／ID 専用プリミティブ制御 API、永続化 API、InkScript
source/compiler/fragment APIを保持し、
sequence source identity、Rust-owned二セルpair preview、その bounded candidate照会、
読込済みBatch graphのoperation照会とimmutable run-copy作成、角度付き撮影frameのedit／preview／snapshot、明示的な
指示raster export API、消失点のCRUD／preview／snapshot APIを保持する。ABI v14 で
`InkpodHistoryItem` から表示用UTF-8名を除き、固定幅の `InkpodHistoryEntryKind` を返す。
Coreは言語や `UiStringId` を保持せず、frontendだけがこの意味種別を表示catalogへ写像する。
ABI v2 で公開名から実装時のマイルストーン番号を除いたタスク API は、現行 v18 でも引き続き
`InkpodTask` / `InkpodTaskInfo` / `INKPOD_TASK_*` / `inkpod_task_*`、共有ラスタ入力は
`InkpodRasterSourceInput` を使用する。v1 のマイルストーン名は公開別名として残していない。

既存のラスタ文書オープン／インポート、クリップボード、ライトテーブル入力は、現行 v18 の操作別 API の
一回の上限付き呼び出し中に、同期的に検証、コピー、正規化、登録される。ストロークサンプルも同期的に
Rust 所有の正規化済みバイト列へコピーされ、4 MiB 以下ならプロシージャ内のペイロード、4 MiB 超なら
正規化済みサンプルアセットになる。シーケンスの入力元は従来どおり上限付きの Rust 所有ラスタコピーである。
`*_v3` API 群の呼び出し側は、世代付き実行時オブジェクト ID を明示的に解放する。操作別 API 群が使う
正規アセットの保持は Core 内部で行われ、`_v3` 付き実行時オブジェクト ID と、永続的で内容アドレス方式の
`AssetId` は別の名前空間に属する。

ABI v18 ではvectorおよびText／Annotationのenum値、record、edit、snapshot、clipboard、diagnostic exportを
公開境界から削除した。旧enum値、旧primitive ID、旧record size、ABI v17 configは明示的に拒否する。

## InkScript source／compiler／fragment（現行 ABI v18）

ABI v15 で追加されたsource parse、diagnostic copy、static compile、journal fragment exportは、exact-current
InkScript file v2／procedure catalog v4／replay epoch 25としてABI v18に保持される。`.inkscript` file filter、
Windows command／UI、実Windows path authorityはまだ接続しない。

`inkpod_inkscript_source_parse` は `InkpodInkScriptSourceInput` の UTF-8 span を呼出中だけ借用し、128 MiB
以下を Rust 所有へ一回コピーする。lex／parse error は API failure ではなく、`OK` と invalid source handle、
stable diagnostic 列として返る。`source_summary` は document kind、complete／valid／BOM flag、source／route
identity、diagnostic count だけを固定幅で返し、CST／AST pointer を公開しない。`source_text_copy` は元 byte列、
`source_diagnostics_copy` は strided diagnostic record列とcode／message／path／hintのpacked UTF-8を二段階で
一括copyする。容量不足では必要count／byte数だけを返し、record/textを部分copyしない。source handleは親を
持たず、外部同期すれば任意threadへ移送・照会・releaseでき、releaseはowner変数をNULLにする。同じNULL ownerの
再releaseはsuccess no-opである。

`inkpod_core_inkscript_compile` はCore owner threadだけで、sourceの`controller_id`／`session_generation`と
requestをexact matchしてから既存の単一Rust compilerへ委譲する。parameter spanは`ask = each_run`の各parameterに
stored default受理またはoverrideを名前で一件ずつ指定する。overrideのUTF-8 spanは呼出中だけ借用し、file v2の
bounded standalone value grammarで一つのclosed valueとしてRust所有へparseした後、source declarationの型へ
exact checkする。persisted sourceのstored defaultは変更しない。成功したprogramはsourceを親にせず、resolved value、
digest、budget、path intentを所有する一方、作成Core generationとowner threadへ束縛される。summary／releaseには同じlive Coreが
必要で、Core destroyより前にreleaseする。stale Core generation、別controller generation、Cancel、invalid、
resource failureはprogramを公開せず、Core文書状態を変更しない。

`inkpod_core_inkscript_fragment_export` はCore owner threadで、size/version付きjournal event recordのbounded spanを
stable IDへ解決し、既存のcanonical exporterだけを呼ぶ。fragmentはcanonical BOMなしLF text、base/final state、
commit count、portability、required-precondition countを所有し、表示summary／thumbnailをauthorityにしない。
`fragment_text_copy`は二段階caller bufferであり、program同様に同じCore generation／owner thread上で照会・releaseし、
Core destroyより先にreleaseする。NULL、short record、unknown flag/kind、noncurrent record、empty／nonlinear／
non-Commit、oversize、Cancel、stale generation、panicはowned handleを公開せず、document/editor revision、history、
dirty、savepoint、cache、asset、ID high-watermarkを変更しない。

全InkScript recordは`struct_size`と`version == INKPOD_INKSCRIPT_RECORD_VERSION`を持つ。ABI v18のRust unit contract、
C11/C++20 layout/include、header/export drift、ABI smokeは、NULL、alignment、short record、unknown flag/kind、
oversize、wrong thread、stale Core/controller generation、二段階copy、Cancel、double release、v16拒否を検証する。
error textは既存thread-local二段階APIを使い、共有global mutable bufferを設けない。

## InkScript execution／report（現行 ABI v18）

ABI v18は、M11／M12から維持している単一のRust planner／runnerへ、authority-free `PathIntent`、検証済み
authority grant、immutable plan、preview、one-shot confirmation、PlanTask／RunTask、task event、detached reportを
接続する。C++側へparser／catalog node、canonical procedure、別planner、別executorを公開しない。

`inkpod_core_inkscript_program_path_intents_copy`は、compile済みprogramのintentを固定幅recordとpacked UTF-8で
一括copyする。callerは各出力recordのsize／version／flagsを初期化する。容量不足では必要量だけを返し、全recordを
検証してからrecord／textをcopyするため、短いrecordや未知flagで部分出力しない。frontendはintentへauthorityを
付与し、`InkpodInkScriptPlanTaskRequest`へgrant列、controller／session／authority／open-session-set generation、
borrowed host adapterを渡す。grantとnested DTOはcreate中にRust所有へcopyされるが、callback contextはtask releaseまで
caller所有である。

host callbackのrequest／responseはsize-versioned fixed-width DTOだけである。callbackが返すpath、fingerprint、session、
record span、byte spanは同じcontextへの次のcallbackまで読み取り可能でなければならず、taskはcallbackをowner Core engine
threadからだけ呼ぶ。callback中にRust側Core lockを保持せず、callbackは同じCoreへreenterしてはならない。callback status、
NULL、短いresponse、未知flag、invalid UTF-8／stride／identityはfail closedである。

PlanTaskのadvanceはbounded planning stageを実行し、成功時だけimmutable planをtake可能にする。plan summaryはplan digestと
item数、preview copyは順序付きinput／output／destinationをpacked UTF-8とともに返す。confirmationはplan digest、authority、
open-session set、scopeへ束縛されたone-shot tokenである。RunTask createは成功時だけplan／confirmation ownerを消費し、
同じ既存runnerを一itemまたは一wait／terminal transitionずつ進める。各taskはlossless一event slotを持ち、未取得eventが
ある間のadvanceは`INKPOD_STATUS_QUEUE_FULL`を返す。query／cancelはreleaseと外部同期すれば任意threadからatomically呼べるが、
advance／event take／owner transfer／releaseは作成Coreのowner threadとgenerationへ固定される。

run itemはstaged Coreでdecode／cache-free replay／canonical execution／current-v28 encodeを完了し、temporary fileをwrite／
flush／closeしてidentityを再検証した後だけatomic installする。cancel、stale plan／confirmation／authority／session／input／
destination、resource、encode、save、install failureは進行中itemをinstallせず、入力Coreのdocument/editor revision、history、
dirty／savepoint、asset、ID high-watermarkを変更しない。既に完了した別itemはrun policyに従ってreportへ残る。

terminal reportはCoreから切り離されたimmutable handleで、外部同期下の任意threadでsummary、bounded item span copy、releaseが
できる。item reportはpreview ordinal、outcome／failure、commit数、final revision、next stable ID、state digest、input／destination
UTF-8を一括copyし、短いrecordやcapacity不足で部分copyしない。shutdownでは新規advanceを止め、taskをcancelし、event／reportを
drainしてtaskをreleaseした後、program、host context、session Core、owner Coreの順に破棄する。M26ではWindows authority／install
adapter、Core engine command route、UI、file filter、clipboard、product `.inkscript` acceptanceを追加していない。

## 角度付き撮影 frame（現行 ABI v18）

`inkpod_core_shooting_frame_get` はCore所有スレッドのread-only queryで、
完全サイズのcaller-owned `InkpodShootingFrameInfo`（136 bytes）と0/1 presenceを
返す。`InkpodShootingFrameInput`（64 bytes）はsigned milli-pixel center、正の
`u64` width/height、時計回りbinary turns、五点anchor、visible／instruction-export
flagを持つ。`inkpod_core_shooting_frame_edit` はcreate/update/deleteを一つの
canonical transactionとして実行し、stale、unknown enum/flag、短いrecord、zero
size、ID mismatch、overflow、no-opでは文書、履歴、revision、dirty、stable-ID
high-watermarkを部分更新しない。入力recordはcall中だけ借用し、Rustは保持しない。

Canvas操作は `inkpod_core_shooting_frame_preview_begin`、`_update`、`_apply`、
`_cancel` を使う。begin/updateはpreviewだけを差し替え、IDやhistoryを消費しない。
applyだけが実変更を一つのUndo単位にし、Cancel、capture loss、失敗は基底状態を
完全に保持する。previewは一つのCoreに高々一件で、競合するmutation、history
移動、save/openとは同時実行しない。

`inkpod_snapshot_get_shooting_frames` の `InkpodShootingFramePoint` spanは
snapshot-owned immutable borrowで、snapshot releaseまで有効である。record countは
0または1、point countはcenter、4 corners、rotation handleを含む6で、rendererは
count、stride、offset、enum、finite conversionを検証する。同じsnapshotの照会と
releaseは外部同期する。

`inkpod_core_export_instruction_common_raster` は通常exportとは別の明示的なdata-plane
queryで、include flag付き撮影frameを合成したRust-owned
`InkpodByteBuffer`を返す。成功handleは`inkpod_byte_buffer_release`で一回だけ解放し、
失敗時はownerをNULLのままにする。通常export、thumbnail、axis-aligned paper fitは
このobjectを含まず、ABI callerが両frame authorityを暗黙変換しない。

## 消失点と放射補助線（現行 ABI v18）

`inkpod_core_vanishing_points_copy` はCore所有スレッドのread-only queryである。
capacity 0／NULLで必要件数を取得し、十分なcaller-owned strided
`InkpodVanishingPointInfo`列へ二回目の呼出しで完全コピーする。Rustは出力bufferを
保持せず、point ID、owner layer ID、signed milli-pixel X/Y、1〜180000の間隔、
180000未満へ正規化済みの開始角、exact RGBA8/16、opacity 0〜1000、visibilityを
返す。

`inkpod_core_vanishing_point_edit` はcreate/update/delete/delete-allを一つの
canonical transactionとして実行する。create/updateの完全サイズ
`InkpodVanishingPointInput`はcall中だけ借用し、unknown flag、非zero reserved、
短い外側／color record、wrong layer、stale revision、invalid interval/color/
opacity/coordinate、存在しないID、上限／overflowをcommit前に拒否する。no-op、
失敗、Cancelではdocument/history/dirty/revisionとstable-ID high-watermarkを進めない。

Canvas handleとdialogは`inkpod_core_vanishing_point_preview_begin`、`_update`、
`_apply`、`_cancel`を使う。create previewのIDは0で、apply成功時だけstable IDを
割り当てる。snapshotの`inkpod_snapshot_get_vanishing_points`は、point列と
viewportでclip済みの`InkpodSnapshotRadialGuide`列をsnapshot-owned immutable
borrowとして返す。両列はsnapshot releaseまで有効で、個別releaseはない。同じ
snapshotの照会と解放は外部同期し、rendererはcount、stride、record size、flags、
色深度、opacity、angle、segment boundsを検証してから描画する。

## 決定的リプレイ契約と複合ダイジェスト

現行 ABI は、所有権を移さない読み取り専用の固定レイアウト照会を二つ提供する。

- `inkpod_core_get_replay_contract` は、呼び出し側が所有する `InkpodReplayContract` へ値を書き込む。
  リプレイエポック 23、現行のプロシージャ／コンテナバージョン 26、正規数値バージョン 1、閉じた
  プリミティブカタログの件数、BLAKE3-256 カタログダイジェストを返す。Core 所有スレッド専用であり、
  文書、リビジョン、履歴、未保存状態、レジストリ、スナップショットを変更しない。
- `inkpod_snapshot_get_canonical_digest` は、不変スナップショットの `InkpodCanonicalDigest` を、
  呼び出し側が所有するレコードへ書き込む。そのスナップショットを読める任意のスレッドから呼び出せる。
  呼び出し中はスナップショットを生存させる必要があり、同じスナップショットへのアクセスと解放を
  同時に行ってはならない。この照会はスナップショットの解放も参照保持も行わない。

どちらの出力も呼び出し側が所有し、`struct_size` には現行レコード全体を収められる値が必要である。
その他のフィールドは出力であり、呼び出し側が動作を選ぶための指定値ではない。公開契約上、呼び出し側は
`InkpodReplayContract::reserved` と `feature_flags` を 0 に初期化し、現行実装も成功時にそれぞれ 0 と
`INKPOD_FEATURE_NONE` を書き込む。`InkpodCanonicalDigest` にはこれらのフィールドがない。
`algorithm` は `INKPOD_DIGEST_BLAKE3_256` で、32 バイトのダイジェスト全体が呼び出し側のレコードへ
コピーされる。NULL、短いレコード、パニックでは通常の ABI ステータス契約に従い、出力を部分更新しない。
スレッド違反が成立するのは Core 所有スレッド専用のリプレイ契約照会だけであり、スナップショットの
ダイジェスト照会は、外部同期された任意の読み取りスレッドから呼び出せる。これらは検証値を公開するだけで、
製品の保存／オープン API は同じ v28 のリプレイ／カタログ契約を使い、現行でないネイティブ形式の
バージョンをすべて拒否する。

現行 ABI v18 は、Core 所有スレッド専用の永続化操作を三つ提供する。`inkpod_core_get_persistence_info` は、
形式バージョン、最後に成功したオープン方式、正本であるジャーナルの件数、決定的なリプレイ作業量と
未保存変更量（`dirty_bytes`）、アセット使用量、`INKPOD_PERSISTENCE_CHECKPOINT_DUE` フラグを、
リプレイや状態変更を行わずに返す。`open_strategy` は `INKPOD_NATIVE_OPEN_NOT_OPENED`、
`INKPOD_NATIVE_OPEN_FULL_REPLAY`、`INKPOD_NATIVE_OPEN_CHECKPOINT` のいずれかである。
`inkpod_core_compaction_plan` は、破棄されるイベント数とプロシージャ数に加え、文書、EditorState、
ジャーナルの正確なダイジェストを返す。UI は履歴件数を表示して確認を得た後、そのレコードを変更せずに
`inkpod_core_write_compacted_copy` へ渡す。書き込み時に確認トークンが古ければ `INVALID_STATE`、
トークンのフラグまたは予約領域が 0 でなければ `UNSUPPORTED` になる。成功時は、現在状態を新しい Genesis
とする別の v28 ファイルを書き出すが、作業中のパス、リビジョン、未保存状態、保存点、ID、履歴は変更しない。
`CoreHost` は三つの操作すべてを Core エンジンキュー経由で実行する。自動的な履歴圧縮は行わず、`CKPT` は
履歴やアセット保持の正本ではない。Windows では `ファイル > 履歴を破棄してコピー...` として公開し、
最初に失われるイベント数とプロシージャ数を表示する。出力先には開いているセッションが所有しないパスだけを
許可し、作成したコピーを現在の保存先として採用しない。

## 履歴可視化 snapshot（現行 ABI v18）

`inkpod_core_history_visualization_create` と
`inkpod_core_history_visualization_create_with_task` は Core 所有スレッド専用の一括読み取り操作である。
後者の `InkpodTask` は ready 状態で渡し、呼び出し中は任意スレッドから進捗照会と cooperative cancel が
できるが、呼び出し完了前に解放してはならない。

対話 UI は ABI v18 の分割 API を使う。`inkpod_core_history_visualization_builder_begin` は ready task と
Core を受け取り、呼び出し時点の journal、Genesis、asset、高水位を Rust-owned
`InkpodHistoryVisualizationBuilder` に固定する。`inkpod_history_visualization_builder_step` は同じ task と
1 以上の `maximum_events` を受け取り、Core 所有スレッドで最大その件数だけ replay する。`out_progress` は
完了／総 event 数と生成済み／総 row 数を返す。`complete == 0` の間は visualization を返さず、同じ builder を
Core queue の末尾へ再投入できる。`complete != 0` の成功時だけ builder を消費し、immutable snapshot を返す。
各 step 間に task を cancel でき、未完了 builder の release も task を canceled にする。task は builder より
長く生存させ、begin／step は同じ Core owner thread で呼び、builder と snapshot の同時照会・解放は外部同期する。

一括 API と分割 API の成功時は、作成時点の journal に含まれる全 `Commit` を `JournalEventId` 順に保持した
Rust-owned immutable `InkpodHistoryVisualization` を返す。非 active branch の commit も含み、
`HistoryMove`、`BranchCut`、Genesis は行にしない。thumbnail は最大 64×64 の出力へ直接合成し、full-canvas
intermediate を作らない。cancel、失敗、query 成功のいずれでも、live document、revision、history、journal、
dirty、savepoint、persistent ID は変更されない。

`inkpod_history_visualization_row_count`、`inkpod_history_visualization_row_get`、
`inkpod_history_visualization_release` は外部同期した任意スレッドから利用できる。`row_get` は primitive 名、
決定的な引数文字列、最大 64×64 の straight-alpha RGBA8 thumbnail の三つを caller-owned buffer へコピーする。
三 buffer をすべて NULL/capacity 0 とする最初の呼び出しで必要 byte 数を取得し、十分な容量を用意した二回目で
一括コピーする。部分的に短い buffer は `INKPOD_STATUS_BUFFER_TOO_SMALL` となり、Rust-owned の内部 pointer は
公開しない。同じ handle の照会と release を同時に行ってはならず、解放後に行 metadata を利用してはならない。

## 新規 Cell creation plan（現行 ABI v18）

`InkpodCellCreationOptions` は sizing mode、入力寸法、軸別 DPI、各辺余白率、
安全／最大寄り比率、五点 anchor、初期 layer kind、RGBA8/16 storage format、
1..64 の枚数を一つの 64-byte size-prefixed record で渡す。未知 enum、未知 flag、
短い record、0／上限外、換算 overflow、非対応 topology は、出力 plan を NULL の
まま拒否する。

`inkpod_cell_creation_plan_create` が返す `InkpodCellCreationPlan` は Rust 所有の
immutable opaque object で、Core や document stable ID を所有しない。
`inkpod_cell_creation_plan_count` と `inkpod_cell_creation_plan_copy` は同じ plan
から計算済み寸法、DPI、100%／基準／作画／安全／撮影／最大寄り frame、余白、
layer kind、format を caller-owned `InkpodCellCreationPlanItem` 列へコピーする。
copy は完全な 144-byte record、`struct_size`、capacity、stride、整列を検証し、
失敗時に `out_written` と要素を部分更新しない。preview と commit はこの同じ
immutable plan を使い、C++ 側で frame 換算を再実装しない。

plan は Core 所有スレッドへ移送できるが、照会と解放を同時に行ってはならない。
最後の所有者が `inkpod_cell_creation_plan_release` に owner pointer のアドレスを
渡すと Rust が解放して caller の変数を NULL にする。同じ NULL owner variable の
再解放は no-op success である。別名 pointer、解放後 pointer、同時利用は無効である。

`inkpod_core_new_cell_from_plan` は Core 所有スレッド専用で、plan index と非ゼロの
document UUID だけを frontend authority として受け取る。layer／plane／Cell の
stable ID は Rust の staged Genesis が確定するまで発行せず、invalid index、UUID、
割当、overflow、topology failure は既存 document、revision、history、savepoint、
ID cursor、出力 `InkpodDocumentInfo` を変更しない。ABI v11 の
`InkpodDocumentInfo` と `InkpodPaperFramesInput` は撮影／最大寄り frame を含み、
前世代 layout を暗黙に受理しない。

## Cut handle（現行 ABI v18）

`InkpodCut` は Cut メタデータ、Cell 作成既定値、同一ディレクトリにある個別 Cell
ファイルへの順序付き参照、独立した履歴／保存点を所有する Rust-owned opaque handle である。
`InkpodCore` とは別の所有権単位であり、Windows の `CutSession` が handle を一つだけ所有する。
作成、すべての操作、破棄は同じ CoreHost owner thread で行い、別スレッドへの所有権移譲や
callback はない。入力レコード、UTF-8 span、member の strided array は呼び出し中だけ借用され、
Rust は復帰後に pointer を保持しない。

`inkpod_cut_create`、`inkpod_cut_open`、`inkpod_cut_open_recovery` は成功時だけ
`InkpodCut **out_cut` へ所有 pointer を返し、失敗時は NULL のままにする。
`inkpod_cut_info` は固定長の状態を返し、`inkpod_cut_metadata_copy` と
`inkpod_cut_member_get` は caller-owned buffer を使う二段階 API である。容量不足では必要 byte 数を
返して部分コピーしない。`InkpodDocumentInfo::cell_id` は ABI v11 で末尾に追加され、Cut member と
個別 Cell の永続 identity を結び付ける。

`inkpod_cut_update` は発行時の base revision に束縛され、success のときだけ metadata/defaults を
一つの履歴単位として commit する。同値更新、Cancel、履歴端の Undo/Redo は no-op で、stale、invalid、
overflow、failure は revision、state/procedure ID、履歴、dirty、savepoint を進めない。既定値の変更は
既存 Cell を変更せず、後で作る Cell にだけコピーされる。

`inkpod_cut_sequence_edit` は 104-byte の size-versioned
`InkpodCutSequenceEditOperation` を stride 付きで最大 256 件受け取る。operation は insert、remove、
move-before、move-after、range-renumber の closed kind で、Cell は常に `(cell_id,
document_uuid)`、移動 anchor も同じ stable identity で指定する。position、count、first number、step、
display number、insert 時だけの borrowed relative-path span は kind ごとに検証され、全レコードと path は
呼び出し中に Rust 所有値へコピーされる。request は 48 bytes、result は 40 bytes で、成功時の実変更だけ
`INKPOD_CUT_SEQUENCE_EDIT_APPLIED` を返し、一回だけ Cut revision/history を進める。同値の空 request と
`inkpod_cut_sequence_cancel` は安定した no-op である。

operation 内の失敗は `failed_operation_index` に 0-based index、request 全体または最終 membership 検証の
失敗は `INKPOD_CUT_SEQUENCE_REQUEST_ERROR_INDEX` を返す。NULL、短い request/result/operation、短い
stride、未知 kind/flag、stale base、欠落/重複 identity、番号衝突、zero step、上限超過、演算 overflow は、
membership、revision、ID、history、dirty、savepoint、Cell file を部分変更しない。Sequence pane に focus が
あるときだけ Cut-session generation を固定した command context から Cut Undo/Redo を呼び、active Cell の
document history へ暗黙に混ぜない。

通常保存と autosave は Cut 記述子と同じディレクトリの各 member `.inkpod` を staged validation し、
相対ファイル名、Cell ID、document UUID が一致するときだけ current v28 / Cut replay epoch 24 の
記述子を原子的に置換する。通常保存だけが Cut savepoint を進め、autosave と recovery open は通常の
path authority/savepoint を採用しない。非current version、欠落、重複、自己参照、directory escape、
symlink escape、identity mismatch は拒否する。

`inkpod_cut_destroy` は owner pointer のアドレスを受け取り、成功時に Rust object を解放して caller の
変数を NULL にする。NULL の owner variable は no-op success だが、コピーした alias、解放後 pointer、
wrong-thread 呼び出しは無効であり、所有権移譲にはならない。shutdown では全 `CutSession` を CoreHost
停止前に owner thread 上で破棄する。

## ABI v18 の `_v3` 付き値／ID 制御 API

この節の `V3` / `_v3` は API 群とレコード名の一部である。すべての呼び出しは、全体として
ABI v18 に一致するヘッダーとライブラリの組み合わせで使用する。

`InkpodObjectId` は、オブジェクト種別、Core 世代、単調増加値からなる固定幅レコードである。Core、
スナップショット、タスク、色配列、サンプル列、ラスタアセット、サムネイル、エクスポートは異なる種別を持つ。
ID は発行元の Core 世代でだけ有効であり、別の Core、破棄後の再作成、解放後には使えない。種別違い、
値 0、未知のオペコードは `INKPOD_STATUS_INVALID_ARGUMENT`、世代違い、期限切れ、二重解放は
`INKPOD_STATUS_INVALID_STATE`、未知の機能やスキーマは `INKPOD_STATUS_UNSUPPORTED` となる。
失敗時に、文書、履歴、ジャーナル、リビジョン、未保存状態、オブジェクトレジストリ、出力レコードを
部分的に変更しない。

`InkpodPrimitiveRequestV3` はポインター、コールバック、パス、ネイティブオブジェクト、STL オブジェクトを
含まない。安定したオペコード、スキーマバージョン、基準文書リビジョン、対象 ID、世代付きペイロード ID、
固定幅のツール、プレーン、色、直径、フラグだけを値で持つ。この API 群が受理する閉じたオペコード集合は、
既存の正規実行器のうち `SetMainLineColor` スキーマ 1、`ReplacePalette` スキーマ 1、
`ApplyRasterStroke` スキーマ 3、`ImportRasterAsset` スキーマ 1 である。可変長のパレット、サンプル、ラスタは、
先に `inkpod_core_register_*_v3` の一回の上限付き呼び出しで完全コピーし、プリミティブ要求は返された ID
だけを参照する。呼び出し側は登録から戻った後、元のバッファを変更または解放できる。実行は
`Core::execute_primitive` に委譲され、従来の主線色、パレット、ストローク、インポート用 FFI ラッパーと
別の意味実装を持たない。成功時は結果全体を書き込み、意味上の変更がない場合は確定済みフラグ、
リビジョン、履歴、未保存状態、永続 ID を進めない。基準リビジョンの不一致、実行中のストローク、
不正な対象やペイロード、オーバーフロー、割り当て失敗はいずれも原子的なエラーとなる。

その他の操作別 ABI も同じ正規 Core 境界へ委譲する。成功した文書変更は、安定した `PrimitiveId` と
プロシージャを一件だけ公開し、従来の FFI 内にピクセル、ジオメトリ、レイヤー、履歴規則の別実装を持たない。
プレビューの開始、更新、キャンセル、照会、エクスポート、デコード、取り込み自体は文書プロシージャではなく、
確定処理の終端だけがプロシージャを生成する。

スナップショット、サムネイル、エクスポート、タスクは Rust 所有の実行時 ID である。スナップショットの
メタデータとタイル、ガイド、ベクターレコードは `first + capacity + stride` の一括コピーで取得し、
タイルピクセルとサムネイル／エクスポートのバイト列は
`InkpodBufferCopyV3 { offset, bytes, byte_capacity, written_bytes, total_bytes }` で取得する。容量 0 の
バイト数照会では `bytes == NULL`、レコード件数照会では出力 NULL、ストライド 0 とする。実記憶域は
その一回の呼び出し中だけ借用され、Core は呼び出し側の出力ポインターを保持しない。ID 値自体はコピーできるが、
同じ有効オブジェクトの照会と解放は Core 所有スレッド上で直列化し、最後に
`inkpod_core_object_release_v3` を必ず一回だけ呼ぶ。Core ID は個別に解放せず、
`inkpod_core_destroy` で終端する。

`inkpod_core_*_v3` の全呼び出しは、タスクの照会とキャンセルを含め、Core 所有スレッドに限定される。
ID レコードは値として UI キューへ渡せるが、その解決、コピー、解放は発行元 Core の所有スレッドで行う。
従来の不透明スナップショット／タスク API にある任意スレッドの例外は、それら固有の契約であり、
v3 ID レジストリには適用しない。

## スレッド契約

`InkpodCore` は単一書き込みかつスレッド固定である。作成、文書操作、ビュー操作、ストローク、履歴、
保存／オープン、スナップショット構築、破棄は、すべて Core を作成した Core エンジンスレッドから呼ぶ。
違反時は `INKPOD_STATUS_WRONG_THREAD` となり、ハンドルや出力の所有権は移動しない。

`inkpod_core_get_resource_usage` も Core 所有スレッド専用の読み取り照会である。呼び出し側が所有する
完全な `InkpodResourceUsage` を一回の呼び出し中だけ借用し、成功時だけ値をコピーする。NULL、短い構造体、
スレッド違反、パニックでは出力を変更しない。タイル／履歴、描画キャッシュ、CPU ステージング、
ライトテーブル／参照画像、シーケンス入力元、サムネイルキャッシュの値は、論理ペイロードをカテゴリ別に
見積もったものである。アロケーターや GPU ドライバー内部の常駐量、COW 複製間の物理共有量は推測しない。
この照会はスナップショットを構築せず、文書／ビューのリビジョン、未保存状態、履歴、保存点を変更しない。

Windows フロントエンドの `CoreHost` は、複数の `InkpodCore` 所有変数を一つの Core エンジンスレッド上に
保持する。各所有変数は `DocumentSessionId` と `Generation` の組で選択し、作業項目は投入時にその組を
値として確定する。同じ数値の Core 内文書／ビュー ID やリビジョンを、セッションをまたぐ経路選択キーに
してはならない。セッションを閉じるときは、先に新規投入を拒否し、受理済み作業と実行中ストロークを
解決してから、作成時と同じスレッド上で該当する所有変数だけを破棄する。このフロントエンドレジストリは、
C ABI や Rust ハンドルの所有権契約を変更しない。

正規の文書変更は、閉じた `PrimitiveWork` バリアントとしてキューへ入れる。レコードが持つのは、発行時の
セッション／世代、`CommandContext` の対象 ID、基準リビジョン、`InkpodPrimitiveRequestV3` の値、
スナップショット／文書情報の公開フラグ、一度だけ処理するためのシーケンス／完了状態だけである。
生ポインター、クロージャ、外部パス、STL コンテナーは持たない。パレット、サンプル、ラスタの呼び出し側
メモリは、キュー投入前の登録呼び出しで Rust 所有 ID へ変換する。キュー飽和で受理されなかった場合は、
シーケンスと保留件数を原子的に巻き戻す。受理済みプリミティブは、実行中ストロークの終了後、クローズ時、
終了処理でのキュー排出時にも、高々一回だけ実行または明示的に解決する。

照会、初期化、操作別アダプターは固定 `AdapterWork` レコードを使う。キューレコードには、セッション／世代、
発行時の `CommandContext`、シーケンス、公開フラグ、上限付き入力トークンだけを置く。呼び出し可能オブジェクト、
ビュー更新と完了通知オブジェクトは `CoreHost` の上限付きレジストリが所有する。Core スレッドはトークンを
一度だけ削除してから実行し、キュー内のバリアント自体に生ポインター、クロージャ、パス、STL オブジェクトを
置かない。

例外は、不変ハンドル、Core を取らないコーデック、アトミックなタスクである。

- スナップショットの参照と解放は任意のスレッドから行える。同じスナップショットの参照と解放は外部同期する。
- タスクとバッチタスクの作成、照会、キャンセル、解放は任意のスレッドから行える。照会とキャンセルは、
  そのタスクを使う Core 操作の実行中に別スレッドから呼べるが、解放は Core 呼び出しが戻るまで待つ。
- 不変バッチグラフ、プレビュー、レポート、バイトバッファ、エンコード済みシーケンス、クリップボードの
  参照と解放には Core のスレッド制約がない。同じハンドルの利用と解放は呼び出し側で同期する。
- パレット／カラーチャートのファイル API にも Core のスレッド制約はない。不変の
  `InkpodColorChartFile` に対する件数取得、要素取得、解放では、同じハンドルの参照と解放を呼び出し側で同期する。

任意のスレッドから呼べることは、同じ所有変数を同時に解放してよいことを意味しない。

## パレット／カラーチャートのファイル契約

`.inkpalette` と `.inkchart` のバイトコーデックは `inkpod-format` が所有し、Windows 側は UTF-8 パス、
元の色深度を保つ色、表示名だけを渡す。両形式は現行のスキーマ 1 だけを受理する。ファイルは最大 16 MiB、
色は最大 4,096 個、チャート名は有効な UTF-8 の 1–1,024 バイトに制限する。保存時は同じディレクトリに
排他的な一時ファイルを作り、書き込み、フラッシュ、同期を終えた後だけ名前を変更する。失敗時に既存の
出力先を先に切り詰めない。

- `inkpod_palette_file_save/load` は呼び出し中だけパスと色バッファを借用する。読み込みは
  `InkpodColorBuffer` の必要量照会／短いバッファの契約に従い、成功時だけ呼び出し側のバッファへコピーする。
- `inkpod_color_chart_file_save` は、ストライド付き `InkpodColorChartEntry` と各 UTF-8 名を呼び出し中だけ
  借用し、復帰後は保持しない。
- `inkpod_color_chart_file_load` は不変の Rust 所有 `InkpodColorChartFile` を返す。件数取得と要素取得は
  読み取り専用であり、要素取得時の名前は必要量照会に対応した呼び出し側バッファへコピーする。最後に
  `inkpod_color_chart_file_release` を必ず一回だけ呼び、成功時は所有ポインターが NULL になる。
- これらは任意のスレッドから利用でき、Core ハンドルを取らず、文書、履歴、リビジョンを変更しない。
  同じチャートハンドルの参照と解放は外部同期する。読み込んだパレットを文書へ適用する場合は、別途行う
  正規の `ReplacePalette` 確定処理だけが文書変更となる。

## ショートカット列の契約

Windows フロントエンドは、メニューコマンドと同じ `command_id` を持つ `InkpodShortcutSequence` 表を
Core エンジンスレッドで登録する。各列は 1–4 個の `InkpodShortcutStroke` からなる。コマンド ID の重複、
列の完全一致、一方が他方の接頭列になる表は、Core がトランザクションとして拒否する。

- `inkpod_core_shortcut_defaults_set` は検証済み既定値と現在値を同時に置き換える。
- `inkpod_core_shortcut_sequences_set` は現在値だけを置き換え、リセット操作は登録済み既定値へ戻す。
- `inkpod_core_shortcut_sequences_copy` は、件数照会と呼び出し側所有のストライド付きバッファへのコピーに
  対応する。これら三関数は Core 所有スレッド専用である。
- `inkpod_shortcut_sequence_resolve` は Core ハンドルを取らない純粋な補助関数である。Core からコピーした
  不変形式の表に対し、任意のスレッドから `NONE` / `PREFIX` / `EXACT` を返す。UI のキー入力ごとに
  Core エンジンスレッドへ往復しないための API である。

これらは文書リビジョン、未保存状態、Undo を変更しない。永続化形式、テキスト入力フォーカスの保護、
入力タイムアウト、衝突時に UI 上で割り当てを交換する方針は、フロントエンドの責務である。

## Batch v3 graph／staged result／二セルpair preview（ABI v18）

ABI v18 の public authoring catalog は `COLOR_REPLACE`、
`MOVE_TO_COLOR_PLANE`、`MASKING`、`ERASE` の4種類だけである。Input と Output は
`InkpodBatchOperationKind` へ混在せず、`InkpodBatchGraphInput` の input span と output
fields に保持する。File／Folder の UTF-8 path、graph名、folder、naming template、
operation/color/pair spanは `inkpod_batch_graph_create` 中だけborrowedで、成功前にRust-owned
値へコピーされる。ActiveDocument inputにpathは指定できず、issue-time session IDとgenerationは
Windowsの `CommandContext` が別に固定する。

各 `InkpodBatchInput`、`InkpodBatchOperationInput`、`InkpodBatchColorPairInput`、
`InkpodColorValue` は完全な `struct_size`、既知flag、整列済みstride、上限付き件数を必要とする。
RGBAはstraight alphaを含む格納成分全部、Binary／Grayscaleはnative値を保持し、depth／formatを
暗黙変換しない。ColorReplaceの重複enabled old色、他3種の重複色、空payload、未知kind、短いrecord、
oversized spanはgraphを公開せず原子的に拒否する。`.inkbatch` graph versionは3で、v2はmigrationなしで
拒否する。

二セル比較は次の所有権順で行う。

1. Core所有スレッドで、自然順indexを `inkpod_core_sequence_source_identity` に渡し、
   caller-owned `InkpodSequenceSourceIdentity` へ非zero UUID＋source generationをコピーする。
2. 同じCore所有スレッドで二identityを `inkpod_core_batch_extract_color_pairs` に渡す。
   identityは呼び出し中だけborrowedで、stale／missing／同一identity、寸法・native format
   不一致、非NULLの出力ownerは拒否される。文書、history、revision、dirtyは変わらない。
3. 成功時の `InkpodBatchPairPreview*` はRust-owned immutable objectである。
   `get_info` と `get_candidate` は完全なcaller-owned出力へ値をコピーし、previewの
   releaseと外部同期すれば任意スレッドから読める。candidateは最大4,096件で、exact
   old/new、pixel count、half-open document bounds、ambiguity flagを持つ。
4. 最後のownerは `inkpod_batch_pair_preview_release(&preview)` を呼ぶ。成功時owner変数は
   NULLになり、同じNULL owner変数の再releaseはno-op successである。別名、解放済みの
   生pointer、照会と同時のreleaseは無効である。

previewは候補を自動決定しない。one-to-manyの解決（候補を一つ選ぶか旧色を除外するか）は
job enqueue前のfrontend責務である。`inkpod_batch_graph_get_info` の名前／folder／templateと
`inkpod_batch_graph_get_input` のpathはgraph-owned borrowed UTF-8で、graph解放まで有効である。
`inkpod_batch_graph_get_operation` と対応するcolor／pair queryはcaller-owned scalar recordへ
値をコピーする。NULL、短いrecord、範囲外index、kind不一致のrow queryは失敗し、graph、Core、
document stateを変更しない。この完全照会経路により、読込済みv3 setはeditable draftへ復元できる。

preview/run/save時だけfrontend draftから `inkpod_batch_graph_create` でRust-owned immutable graphを
一回構築する。`inkpod_batch_graph_clone_with_operations` は同数の完成済みoperation列へ差し替える
場合だけ別ownerを返し、元graphを変更しない。graphは `inkpod_batch_graph_release` で解放する。

NewTabs outputの `InkpodBatchReportInfo::staged_result_count` は、reportが所有するgeneration付き
staged Core slot数である。`inkpod_batch_report_take_staged_result` はCore owner threadで一slotを
一回だけ消費し、generationと新しい `InkpodCore*` の所有権を返す。NULL／非NULL出力owner、範囲外、
二回目take、wrong-threadはslotとownerを変えず失敗する。成功したCoreは呼出threadがownerとなり、
`inkpod_core_destroy` で解放する。WindowsはこのCoreをCore-engine thread上の新しい
`DocumentSession`へadoptし、裸pointerを `PostMessage` へ積まない。ActiveDocument outputも同じ
issue-time session/generationにだけ適用し、staleなら何もcommitしない。

## 所有権と有効期間

### 借用入力

通常の `const T*` 入力、UTF-8 列、バイト列、サンプル列は、その API 呼び出し中だけ借用される。
保持が必要な API は戻る前に意味値をコピーする。呼び出し側は API から戻った後に入力バッファを
再利用または解放できる。

正規アセットを取り込む API も、この規則の例外ではない。Core 所有スレッド上の呼び出しが成功を返すまでに、
ラスタ文書オープン／インポート、クリップボード、ライトテーブルの記述子とエンコード前後のバイト列を検証し、
Rust 所有の正規バイト列へコピーして、内容アドレス付きレジストリへ登録する。ストロークのサンプル列も
復帰前にコピーし、4 MiB 以下ならプロシージャ内のペイロード、4 MiB 超なら正規サンプルアセットとして確定する。
シーケンスは上限付きの Rust 所有ラスタコピーであり、ベクタープリミティブは型付きの正規ジオメトリと
安定 ID を保持する。

Core、一時セッション、ジャーナル、スナップショットは、復帰後に呼び出し側のレコード、バッファ、
ファイル名、パスを参照しない。同じ正規記述子と論理ペイロードは同じレジストリエントリへ重複排除されるが、
内部参照数や割り当てアドレスは ABI の一部ではない。

### Rust 所有ハンドル

不透明ハンドルを生成する API は `T** out_*` を受け取る。所有変数は呼び出し前に NULL でなければならず、
成功時だけ Rust 所有ハンドルが入る。対応する解放／破棄関数は所有変数へのポインターを受け取り、
所有権を消費して同じ変数を NULL にする。

```cpp
InkpodSnapshot* snapshot = nullptr;
InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
if (status == INKPOD_STATUS_OK) {
    // snapshot を所有している
}

inkpod_snapshot_release(&snapshot);
// snapshot == nullptr。同じ所有変数で再び解放しても、何もせず成功する。
```

### Ordered snapshot render plan

`inkpod_snapshot_get_render_plan` writes an exact-size caller-owned
`InkpodSnapshotRenderPlan`. Its pass and adjustment-LUT pointers are borrowed
from the immutable parent snapshot, may be read on an externally synchronized
renderer thread, and remain valid only until `inkpod_snapshot_release`.
`pass_stride_bytes` is `sizeof(InkpodSnapshotRenderPass)` and each adjustment
LUT is exactly 768 bytes: 256 red entries, then green, then blue.

Passes are emitted in bottom-to-top execution order. Layer begin/end records
form a non-nested group whose opacity applies once to the group. Raster records
index the corresponding snapshot tile span; adjustment
records index one LUT. The adapter rejects NULL, short/misaligned records,
unknown pass kinds, invalid group structure, out-of-range item spans, nonzero
reserved fields, and opacity above 1000 before the renderer retains a snapshot.
The records transfer no ownership. The render-plan export was additive in ABI
v5; the current library nevertheless requires ABI v18 because the selection
records now carry range, construction, pressure, and view-zoom values.

### Filter preview session

`inkpod_core_filter_preview_begin[_task]`,
`inkpod_core_filter_preview_update[_task]`,
`inkpod_core_filter_preview_apply`, and `inkpod_core_filter_preview_cancel`
operate on the Core owner thread and reuse the existing ABI v18 records. The
`InkpodFilterInput` and any curve-point span are borrowed only for the call;
`InkpodFilterPreviewInfo` and `InkpodDispatchResult` are caller-owned value
records and require no release. `InkpodTask*` remains caller-owned: the caller
may request cooperative cancellation, must keep it alive until the Core call
returns, and releases it with `inkpod_task_release` after completion.

Begin captures the stable Plane ID, committed base document, and base revision.
Every update recomputes from that same base; it never treats the previously
published preview as input. A failed or cancelled update retains the last
successfully published preview, and no begin/update changes committed document
revision, history, journal, dirty state, savepoint, or persistent IDs. Apply
commits the latest successful parameters through the canonical filter primitive
as at most one Undo unit. Cancel discards the session and restores the base.
Cross-plane update, missing/locked target, malformed or short records, invalid
parameters, stale base, cancellation, overflow, and task failure publish no
partial committed state.

The Windows adapter keeps at most one running task and one pending immutable
parameter set. A new dialog generation cooperatively cancels the running
begin/update and replaces only the pending value. Completion is resolved against
the issue-time session/view/generation; it never falls back to the newly active
document. The adapter releases every task after its completion message whether
the result is current, cancelled, stale, or failed.

### Geometry construction preview

`inkpod_core_geometry_apply` and
`inkpod_core_geometry_preview_begin/update/commit/cancel` expose one Core-owned
raster geometry state machine on the Core owner thread.
`InkpodGeometryInput` carries a stable Plane ID, primitive, option flags,
native-depth outline/fill colors, width, aspect, polygon side count, rotation in
turns, and a caller-owned strided span of at most 256 `InkpodGeometryPoint`
records. Core copies every point during the call and retains no input record,
point pointer, Windows command ID, device coordinate, or DPI value. Each point
record requires its complete `struct_size`, zero reserved field, and finite
document coordinates.

One-shot apply accepts only `base_revision = 0`. Preview begin/update require
the exact current document revision, and update cannot change the target Plane
or primitive captured by begin. The Core handle owns at most one preview and
rebuilds every update from the same committed base. `InkpodGeometryPreviewInfo`
is a caller-owned value record with no release function. Cancel changes no
committed document/history/dirty/ID state. Commit executes the same
`ApplyGeometry` canonical primitive once and creates one Undo unit. The
caller-owned dispatch result reports revision/history outcome without allocating
any geometry object ID.

NULL, misalignment, short outer or nested records, zero/oversized counts,
invalid stride, unknown primitive/flag, nonzero reserved fields, stale base,
cross-target update, and point/work overflow are rejected without partial
preview or committed publication. Preview snapshots follow the normal
Rust-owned snapshot lifetime. These additive exports are retained in ABI v18 and do not
make an older ABI version acceptable.

解放後は、ハンドルから得たタイル、ピクセル、ガイド、文字列、バイト列と、コピーしておいた
別名ポインターを一切使わない。Rust が確保したオブジェクトを `free`、`delete`、`CoTaskMemFree` で解放しない。

主な所有者と借用データの関係は次のとおりである。

| 所有対象             | 所有期間                          | 借用データの有効期間                                      | 解放                          |
| -------------------- | --------------------------------- | --------------------------------------------------------- | ----------------------------- |
| Core                 | 作成成功から破棄まで              | Core ポインターは所有スレッドでの呼び出し中だけ利用       | Core 所有スレッド             |
| スナップショット     | 構築成功から解放まで              | タイル、ピクセル、変換、ガイドは解放まで                  | 外部同期した任意スレッド      |
| クリップボード       | コピー／作成成功から解放まで     | ラスタ出力は呼び出し側バッファ。内部ペイロードは非公開   | 外部同期した任意スレッド      |
| バイトバッファ       | 出力成功から解放まで              | バイト列は解放まで                                        | 外部同期した任意スレッド      |
| エンコード済み列     | 出力成功から解放まで              | 要素名とバイト列は解放まで                                | 外部同期した任意スレッド      |
| タスク／バッチタスク | 作成成功から解放まで              | 照会値は呼び出し側へのコピー                              | Core 呼び出し終了後の任意スレッド |
| 履歴可視化 builder   | begin 成功から完了または release まで | step 中は同じ task が生存し、point-in-time 入力は Rust 所有 | begin と step は Core 所有スレッド、release は外部同期 |
| 履歴可視化 snapshot  | Core query 成功から解放まで        | 行の文字列と thumbnail は呼び出し側バッファへのコピー     | 外部同期した任意スレッド      |
| バッチグラフ         | 作成／読込成功から解放まで        | 実行／プレビュー中はグラフが生存している必要がある       | 外部同期した任意スレッド      |
| バッチプレビュー／レポート | Core 出力から解放まで       | 要素の UTF-8 列は親ハンドルの解放まで                    | 外部同期した任意スレッド      |
| Cell creation plan   | plan 作成成功から解放まで          | copy/commit 呼出中は plan が生存し、解放と同時実行しない | 外部同期した任意スレッド      |
| カラーチャート       | 読込成功から解放まで              | 取得時に色と名前を呼び出し側バッファへコピー              | 外部同期した任意スレッド      |

スナップショットのラスタタイル記憶域はスナップショット側で独立して参照計数されるため、作成元 Core より
長く生存できる。ただし通常の終了処理では、レンダラーキューを空にしてスナップショットを先に解放すると、
所有権を追跡しやすい。

正規アセットレジストリは `InkpodCore` が所有し、操作別 API 群には独立した不透明ハンドルや解放 API を
公開しない。`_v3` 付きのラスタ／サンプル ID は、上限付き取り込み後の実行時オブジェクトを所有する
別のレジストリエントリであり、プリミティブの正規化／確定時に、プロシージャ内ペイロードまたは永続的な
`AssetId` へ解決される。実行時オブジェクト ID を解放しても、確定済みプロシージャによるアセット保持は
失われない。Genesis、保持対象のジャーナル分岐と無効な Redo 末尾、既知の永続参照、生存中の一時所有者が
保持ルートとなる。現在実体化されている文書やチェックポイントだけを見て解放してはならない。セッションを
閉じるときは、受理済み Core 作業と一時所有者を空にしてから、レジストリ全体を所有スレッド上で破棄する。
取り込みや確定の失敗時は、文書、履歴、ジャーナル、リビジョン、未保存状態、公開済み保持関係、
呼び出し側所有の出力を部分変更しない。

## 出力と失敗

値出力は呼び出し側が所有する。成功時だけ利用し、失敗時はヘッダーが部分出力を保証する場合を除いて
読み取らない。特に所有権を返す出力は呼び出し前に NULL にし、失敗時にも NULL のままか確認する。

部分出力を意図的に返す代表的なパターンは次のとおりである。

- `INKPOD_STATUS_BUFFER_TOO_SMALL` は必要な要素数またはバイト数を返す。
- `INKPOD_STATUS_FILL_OVERFLOW` は漏れ候補座標を返すが、文書を変更しない。
- キャンセルされたバッチ実行は、`INKPOD_STATUS_CANCELLED` と所有レポートを同時に返すことがある。
- エラーメッセージのコピーに失敗すると、書き込みバイト数を 0 にし、同じスレッドの診断を保持する。
- `inkpod_core_validate_plane_creation` は、UI で確定する前に種類と形式を検査する、所有スレッド専用の
  読み取り照会である。成功・失敗のどちらでも、文書、安定 ID、リビジョン、未保存状態、履歴を変更しない。
  実際の `inkpod_core_tree_edit` も同じ制約を再検証するため、照会後に状態が変わっても不正な作成は
  確定されない。

Rust のパニックは ABI 境界で捕捉され `INKPOD_STATUS_PANIC` になる。C++ の例外も ABI を越えさせない。

## 必要量照会と呼び出し側所有バッファ

`inkpod_core_locator_sample` と `inkpod_core_locator_neighborhood` は snapshot と同じく、active stroke
preview、filter preview、committed document の順で表示用 document を選ぶ。公開スポイトの committed
document 専用契約は変えない。いずれも読み取り照会であり、preview を参照しても document/view revision、
history、journal、dirty、savepoint を変更しない。

`inkpod_core_locator_neighborhood` は、モードレスなロケーターの拡大表示に必要な複数ピクセルを、
所有スレッド上の一回の ABI 呼び出しで返す。`radius` は 0..16 で、出力は常に
`(radius * 2 + 1)` の正方形に密配置した非乗算アルファの RGBA8 となり、文書外は透明になる。
`pixel_capacity == 0` かつ `pixels_rgba8 == NULL` でメタデータと `required_bytes` を問い合わせ、
十分な大きさの呼び出し側所有バッファを設定して再度呼ぶ。バッファは呼び出し中だけ借用され、Core は
保持しない。必要量照会とコピーのどちらも、文書、ビュー、リビジョン、未保存状態、Undo を変更しない。

`inkpod_core_geometry_points_resolve` は Core 所有スレッド専用の読み取り照会である。Windows は
`InkpodGeometryPointResolveInput` に bounded な `InkpodStrokeSample` span、発行時の `view_id`、
非zero の `expected_view_revision`、必要なら `INKPOD_GEOMETRY_RESOLVE_BYPASS_SNAP` を渡す。
入力 span は呼び出し中だけ借用され、Core はコピー後に保持しない。view ID 0 は primary view、
それ以外は live secondary view であり、stale revision や閉じた view を active view へ fallback しない。
出力 `InkpodGeometryPoint` span は呼び出し側所有で、各要素に `struct_size` を設定する。容量不足では
`InkpodGeometryPointResolveResult.point_count` だけを返し、point span を部分書き込みしない。成功しても
document/view revision、history、journal、dirty、savepoint は不変である。

可変長出力は、まず NULL／容量 0 で必要量を問い合わせ、呼び出し側が確保した後に再度呼ぶ。API ごとに
必要量を返すフィールド名が異なるため、各構造体の Doxygen 契約を確認する。

```cpp
InkpodClipboardRasterBuffer output{};
output.struct_size = sizeof(output);

InkpodStatus status = inkpod_clipboard_render_rgba8(clipboard, &output);
if (status != INKPOD_STATUS_BUFFER_TOO_SMALL && status != INKPOD_STATUS_OK) {
    // 診断を取得して中止
}

std::vector<std::uint8_t> pixels(static_cast<std::size_t>(output.required_bytes));
output.pixels_rgba8 = pixels.data();
output.pixel_capacity = pixels.size();
status = inkpod_clipboard_render_rgba8(clipboard, &output);
```

二回の呼び出しの間に対象オブジェクトを変更または解放しない。Core 文書を対象とする照会では、必要量取得と
本取得を同じ Core エンジン作業項目内で行うと、リビジョンのずれを避けられる。

`inkpod_core_layer_thumbnail` もこの呼び出し側所有バッファ方式を使う。最初の呼び出しでは
`InkpodLayerThumbnailBuffer::pixels_rgba8 = NULL`、`pixel_capacity = 0` とし、返された
`required_bytes` を確保してから、同じ安定レイヤー ID と最大寸法で再度呼ぶ。結果は上から下へ密配置した
非乗算アルファの RGBA8 で、`revision` は生成元の確定済み文書リビジョンである。バッファの確保と解放は
呼び出し側が担い、Core はポインターを保持しない。レイヤー自体が非表示でも内容を確認できる一方、
プレーンの表示状態とレイヤー／プレーン不透明度はサムネイルに反映される。

`inkpod_core_document_thumbnail_get` は visible document 全体を同じ上限付き 64×64 以下の
straight-alpha RGBA8 として返す。`InkpodDocumentThumbnailBuffer` の capacity 0／NULL による
size query と二回目の copy を同じ Core-owner-thread work item 内で行う。query は document
revision、history、dirty、savepoint を変更せず、Cut frontend は member file を staged Core で
開いて stable `(CellId, document UUID)` を再確認してからこの結果を presentation cache へ置く。

`inkpod_core_sequence_thumbnail_get` も同じ呼び出し側所有の照会／コピー契約を使う。
`pixels_rgba8 = NULL`、`pixel_capacity = 0` の照会で `required_bytes`、寸法、ストライド、チェックサムを
取得し、確保後の二回目で上限付きの非乗算アルファ RGBA8 をコピーする。ポインターは呼び出し中だけ
借用され、Core は保持しない。二回の呼び出しは同じ `DocumentSessionId + Generation` の
`CoreHost` 作業項目内で行い、別のアクティブ文書へ解決し直さない。

`inkpod_core_sequence_import_mixed_encoded` の `InkpodNamedRasterInput` 列は、呼び出し側所有の借用入力である。
レコード、UTF-8 名、エンコード済みバイト列は呼び出し終了まで有効とする。Core は全レコードの構造、形式、
長さを検証してから各画像をデコードする。全件成功時だけシーケンスを一括置換し、一件でも入力不正、
デコード失敗、割り当て失敗があれば、以前のシーケンス、現在の文書、未保存状態、Undo を保つ。

通常の前後セル切替は ABI v18 の additive な二段階 API を使う。
`inkpod_core_sequence_step_resolve` は Core 所有スレッド専用で、caller-owned の固定長 96-byte
`InkpodSequenceStepPlan` に direction、`STOP`／`WRAP`、`EMPTY`／`SINGLE_CELL`／`STOPPED`／
`ADVANCED`／`WRAPPED`、sequence revision、source／target の UUID・generation・自然順 index・
cell number をコピーする。空 sequence だけは revision 0、`INKPOD_SEQUENCE_INDEX_NONE`、zero identity
を返す。pointer は含まず、Core は出力を保持しない。

`inkpod_core_sequence_step_commit` は同じ plan を呼出中だけ借用し、direction と endpoint policy を
Core で再解決して全 field が発行時と一致する場合だけ切り替える。empty／one-cell／stopped は dirty
文書でも document revision、history、journal、dirty、savepointを変えない success である。advanced／
wrapped は通常 savepoint が必要で、stale sequence/source/target、dirty source、未知 enum、nonzero
feature flag、NULL／misaligned／短い record は出力と Core を変更せず失敗する。固定長 caller-owned
record だけを使うため release API はなく、通常前後 navigation policy は motion-check の
`INKPOD_MOTION_FLAG_LOOP` から独立する。

セル切替前の自動保存は、同じCore所有スレッド上で次の三段階を使う。ABI versionはv9のままのadditive契約である。

1. `inkpod_core_sequence_switch_request` はcaller-ownedの固定長
   `InkpodSequenceSwitchRequest`へ、policy、source/target document UUID、sequence source
   generation、source document/editor revision、target indexを値としてコピーする。raw pointer、path、
   pane indexは保持しない。同じcellへの要求は`REQUIRED` flagなしのsuccessであり、範囲外index／policy、
   NULL／短い構造体はinvalidとなる。
2. frontendが別statusのnative recovery saveとmetadata publicationを完了した後だけ、
   `inkpod_core_sequence_commit_autosaved_switch`へ同じrequestを戻す。Coreはsource identityと両revisionを
   再検査し、staleなら現在文書、history、dirty、出力を変更しない。
3. target entryにexact recovery associationがある場合は、代わりに
   `inkpod_core_sequence_restore_autosaved_switch`へUTF-8 pathを渡す。path bytesは呼出中だけborrowedで、
   Coreは保持しない。current-version containerのdecode、asset検証、replay、target UUID照合をstaged Coreで
   完了してから一回で交換し、復元文書を`RECOVERED|DIRTY`かつnormal-path非採用として返す。失敗時は
   live Coreを交換しない。

これらはRust所有objectを新規に返さず、release関数も追加しない。Windows `DocumentSession`がartifact pathと
metadataをUUID+source generationへ関連付け、CoreHost queueの完了前に別cellへ再解決しない。

## Light Table 前後 N セル一括登録（現行 ABI v18）

三つの additive API は Core 所有スレッド専用で、すべて caller-owned の固定幅レコードだけを使う。
`inkpod_core_light_table_bulk_request` は対象 set ID、direction、N、base/step opacity を検証し、文書
revision、sequence revision、active source UUID/generation を固定した
`InkpodLightTableBulkRequest` をコピーする。入力レコードは呼び出し中だけ借用され、戻り後に Core は
ポインターを保持しない。N=0 は有効な no-op、opacity は 0..1000、direction は previous/next/both の
閉じた値である。

`inkpod_core_light_table_bulk_preview` は同じ request を再検証する非変更 query である。最初に
`entries=NULL, entry_capacity=0, entry_stride=0` で必要件数と add/skip 件数を取得し、二回目は
`sizeof(InkpodLightTableBulkPreviewEntry)` 以上かつ alignment の倍数である caller-owned strided span へ
top-to-bottom の候補をコピーする。各 entry は sequence index/cell number、source UUID/generation、距離、
計算済み opacity、Add/SkipExisting action と、skip 時の既存 source revision を持つ。名前や Rust 所有
バッファは公開しない。既存同一 UUID item は revision が違っても保持され、preview は既存 item の ID、
変換、opacity、表示 mode、可視性、名前、順序を変更しない。

`inkpod_core_light_table_bulk_register` は preview と同じ stale 条件を再検査し、add 件数分の caller-owned
`uint64_t` span へ新規 item ID を top-to-bottom 順で返す。容量不足、NULL、短い request/summary、未知 enum、
範囲外 opacity、stale document/sequence/active source、asset/ID/count overflow、panic は出力、文書、history、
journal、dirty、stable ID を部分更新しない。全候補 skip または N=0 は success/no-op で ID span は空、
一件以上の追加は一つの canonical procedure と一回 Undo 単位になる。Core が不変 source asset と全 item
property を canonical procedure へ解決して所有するため、sequence 入力 pointer や frontend command ID は
保存されない。Windows は issue-time の `DocumentSessionId + Generation` を固定し、preview の OK 後だけ
同じ request を apply する。

## 独立サブパレットの契約（ABI v19）

`InkpodSubpalette` は編集可能文書とは別の Rust 所有 opaque object であり、`inkpod_subpalette_create` を
呼んだ owner thread だけが照会、source 置換、decode、view 操作、sampling、snapshot 構築、release を行う。
Windows `CoreHost` は document session の有無に関係なく、この object の全操作を同じ Core engine thread
へ直列化する。ABI v19 caller は v18 header から再コンパイルが必要であるが、native file version と replay
epoch は変更しない。

- `inkpod_subpalette_replace_sources` は caller-owned の strided `InkpodSubpaletteSourceInput` 列を呼出し中だけ
  借用する。各 record は frontend 内だけで意味を持つ非ゼロ `source_token` と UTF-8 表示名を持ち、path や
  file bytes は Rust object へ渡さない。Core は上限、長さ、UTF-8、重複 token を全件検証し、stem 末尾の
  十進数字列によるセル順を確定してから一覧を一回で置換する。失敗時は一覧、active image、ID authority を保つ。
- `inkpod_subpalette_item_get`、`inkpod_subpalette_item_name_copy`、
  `inkpod_subpalette_adjacent_item` は caller-owned record/buffer へ bounded copy する read-only query である。
  item ID は一つの `InkpodSubpalette` object 内だけで有効で、source 置換後の stale ID は拒否する。
- `inkpod_subpalette_load_common_raster` は caller-owned bytes を呼出し中だけ借用し、PNG/TIFF/TGA/BMP を
  private Core へ staged decode する。成功時だけ active image と private view を置換し、decode failure では
  直前の active image、view、sample 結果、snapshot を保つ。
- `inkpod_subpalette_view_apply` は private view の zoom/pan/flip/viewport だけを変更する。
  `inkpod_subpalette_sample` は同じ変換を通した device-pixel 座標を半開区間で検証し、元画像の RGBA8/16
  native depth と straight alpha を caller-owned `InkpodColorValue` へコピーする。
- `inkpod_subpalette_build_snapshot` は NULL の owner slot へ Rust 所有の immutable `InkpodSnapshot` を返す。
  成功後は renderer sink または caller の一方だけが `inkpod_snapshot_release` を一度呼ぶ。
- `inkpod_subpalette_clear` と `inkpod_subpalette_release` は decoded raster と private Core/view を解放する。
  release は owner slot を NULL にする。NULL owner storage と wrong thread は negative status、既に NULL の
  owner slot を再度 release する操作は no-op とする。

この object は document revision、EditorRevision、history、journal、dirty、savepoint、persistent ID、native
serialization を変更しない。Windows は外部 path と bounded file read を platform adapter 内に保持し、completion
時に workspace ID と generation を再検証する。終了時は Canvas sink を解除し、pending load を stale にしてから、
owner thread 上で `InkpodSubpalette` を release する。

## Floating transform（現行 ABI v18）

ABI v10 の `InkpodFloatingTransform` は 48-byte の caller-owned borrowed
入力で、`struct_size`、closed anchor `u32`、absolute document target X/Y、
local scale X/Y、clockwise rotation degrees を順に持つ。anchor code は
1 TopLeft、2 TopRight、3 Center、4 BottomLeft、5 BottomRight である。source
bounds は半開区間で、選択 anchor を scale/rotation pivot とし、X/Y scale、
clockwise rotation、anchor の absolute target 配置の順に解釈する。C++ record
の pointer は呼び出し中だけ借用され、Core は復帰後に保持しないため release
関数はない。

`inkpod_core_floating_transform` は Core owner thread と active floating paste
を要求する。成功時は transient preview だけを同じ base に対して置換し、
document revision、history、dirty、savepoint、persistent ID を変更しない。
unknown anchor、非有限 target/scale/angle、範囲外・非正 scale は
`INVALID_ARGUMENT`、短い record は `INCOMPATIBLE_ABI`、floating 不在は
`INVALID_STATE` とし、どの失敗も直前 preview と committed state を保つ。
`inkpod_core_floating_commit` だけが raster payload を canonical procedure
で一括 commit し、Cancel は base を完全に保つ。ABI v10 への更新は record の
48-byte layout自体を変えず、旧 reserved/translate field の意味を closed
anchor/absolute target へ置き換えるため、ABI v9 caller は再コンパイルが必要である。

## EditorDefaults / EditorState（現行 ABI v18）

### 複数 edit target

- `InkpodEditTarget` は kind（Layer/Plane）と document 所属の stable Layer ID、
  必要な Plane ID だけを持つ caller-owned value record である。
  `inkpod_core_get_edit_targets` は二段階の count query と `count/capacity/stride`
  付き出力を使い、`inkpod_core_set_edit_targets` は完全な入力 span を呼び出し中に
  Core 所有の bounded collection へコピーする。呼び出し後に pointer は保持しない。
- count は 0..4,096、stride は `sizeof(InkpodEditTarget)` 以上かつ alignment の倍数、
  各 `struct_size` は完全長以上、reserved は 0 とする。空 span は NULL/count 0/
  stride 0 の組だけを受理する。Core は重複を除き document tree 順へ正規化し、
  foreign、deleted、cross-layer、oversized、malformed target を公開前に拒否する。
- `inkpod_core_set_edit_targets` は exact `EditorRevision` を precondition とする。
  成功して集合の意味が変わった場合だけ EditorRevision、EditorStateDigest、editor
  dirty を進め、document revision、StateId、history、journal、document dirty、stable
  ID は変えない。active row/active plane は集合とは独立する。
- `inkpod_core_get_edit_target_capabilities` は現在の effective target set に対する
  duplicate/delete/visibility/editability/merge/plane-convert/layer-convert の matrix を
  `InkpodEditTargetCapabilities` へコピーする read-only query である。
- `inkpod_core_apply_edit_target_command` は一つの command を一つの canonical
  invocation/transaction/Undo 単位として実行し、duplicate/merge の tree-ordered
  出力 target を caller-owned strided span へ返す。必要容量の query では
  `INKPOD_STATUS_BUFFER_TOO_SMALL` と必要 count だけを返し、document を変更しない。
  invalid、incompatible、stale、overflow、failure は結果 span、revision、history、ID
  を部分公開しない。
- これらは Core owner thread 限定で、Windows `CoreHost` は issue-time の
  `DocumentSessionId + Generation` を固定して query/update/command を実行する。
  private clipboard handle は ordered raster plane payload、document origin、型、
  8/16-bit 値を Rust 側で所有し、paste/cancel/release まで
  C++ が内部 pointer を参照しない。

次の八つの Core 所有スレッド用 API と固定幅レコードは ABI v2 以降に追加され、現行 ABI v18 に保持されている。
ABI v2 のライブラリや呼び出し側を受理するという意味ではない。

- `inkpod_core_get_editor_defaults` は文書作成前にも有効な Rust 所有の不変 `InkpodEditorDefaults` を、
  呼び出し側所有のレコードへコピーする。組み込みの初期文書仕様と EditorState 初期値はアプリケーション設定
  ではなく、新規文書の作成時に Core がセッションの Genesis／EditorState へ明示的にコピーする。
- `inkpod_core_get_editor_state` は現在の `InkpodEditorStateInfo` を副作用なくコピーする。
- `inkpod_core_update_editor_state` は `InkpodEditorStateUpdate` の種類と、期待する正確な
  `EditorRevision` を検証し、成功時の完全な `InkpodEditorStateInfo` をコピーする。更新種別は、
  アクティブツール、ツール色、ツール直径、塗り、選択、アクティブ対象、パレットカーソル、ブラシ設定の
  閉じた集合である。
- `inkpod_core_editor_stroke_begin` は、呼び出し側所有の `InkpodEditorStrokeInput` のサンプル列を
  呼び出し中だけ借用する。`tool` が 0 ならアクティブツール、0 でなければ指定ラスタツールについて、
  Core 所有のスタイルを選び、RGBA8/RGBA16 の色深度を保つ色、Q16 直径、安定した対象、形状、平滑化、
  始点色限定を、開始時に一度だけ
  正規ストローク引数へコピーする。ツール指定はロケーター用の固定鉛筆などに使うが、呼び出し側は色、直径、
  対象、ブラシ設定を渡さない。primary view を使う互換入口であり、追加／終了処理は、その後の EditorState を再参照しない。
- `inkpod_core_editor_stroke_begin_for_view` は ABI v5 で追加した view-aware 入口である。`view_id == 0` は
  primary view、それ以外は同じ Core が所有する live secondary view を表す。device-pixel サンプルは開始時に
  指定 view の変換を捕捉して文書座標へ正規化し、後続の append/end でも同じ変換を使う。存在しない view ID は
  副作用なく拒否する。
- `inkpod_core_apply_fill_for_editor_target` と
  `inkpod_core_apply_selection_for_editor_target` は、既存の上限付き入出力レコードに、操作開始時に捕捉した
  安定レイヤー／プレーン ID の組を添えて実行する。この組は同じ文書名前空間内で再検証し、操作中に
  EditorState が変わっても別の対象へ解決し直さない。既存の入口関数は、一回の同期コマンド開始時に
  現在の対象を Core 内で捕捉する経路として維持する。
- `inkpod_core_select_color_for_editor_target` は、色選択コマンドの開始時に捕捉した安定レイヤー／プレーン ID
  の組と、色深度を保つ `InkpodColorValue` を使う。その後に EditorState が変わっても入力元プレーンを
  切り替えない。既存の `inkpod_core_select_color` は、同期コマンド開始時に現在の対象を Core 内で捕捉して
  委譲する。

ABI v18 の `InkpodSelectionInput` と `InkpodEditorSelectionOptions` は、range interpretation、
Q16.16 aspect、from-center／45度制約、`u32` turns、round／square trace、pressure-size、
screen-size を固定幅値として保持する。gesture の rectangle／ellipse はちょうど二つの
`InkpodSelectionPoint` を渡し、trace point は座標に加えて 0..1 の pressure を持つ。
`view_zoom_q16` は screen-size trace の発行時 view を固定し、Core は call 後に pointer span を
保持しない。未知 enum／flag、非ゼロ reserved、短い record、不正 count／stride、非有限点、
範囲外 pressure／zoom は文書、EditorState、履歴を変えずに拒否する。

`InkpodEditorBrushOptions` は caller-owned の入れ子入力であり、Core は値だけをコピーしてポインターを保持しない。
`shape` は Round または Square、`smoothing` は 0..1000、`start_color` は Any または ExactNative とする。
ExactNative は変更前の始点 pixel を native depth と alpha を含めて完全一致比較し、接続性を要求せず、到達した
footprint 内の一致 pixel だけを変更する。平滑化は仕様の固定 Q16.16 式を使い、開始後の設定変更は実行中 stroke に
影響しない。未知 enum、範囲外平滑化、非ゼロ reserved、短い入れ子 record は原子的に拒否される。

公開レコードは `InkpodEditorFillOptions`、`InkpodEditorSelectionOptions`、
`InkpodEditorBrushOptions`、`InkpodEditorStateInfo`、`InkpodEditorDefaults`、
`InkpodEditorStateUpdate`、`InkpodEditorStrokeInput` である。呼び出し側は、最上位の入力レコードと、
その入力が使用する各入れ子レコードの `struct_size` を、現行 ABI v18 ヘッダーにある完全な
`sizeof(record)` 以上に設定し、予約領域と未知フラグを 0 にする。照会／更新の出力では、呼び出し側は
最上位出力の `struct_size` だけを提示する。Core は成功時に、呼び出し側所有の完全なコピーと、各入れ子出力の
`struct_size` を書き込む。短い最上位レコード、使用対象の短い入れ子入力、NULL、未知の列挙値／更新種別、
非有限値、範囲外の値、0 または存在しない安定対象 ID は拒否する。RGBA8/RGBA16 は既存の
`InkpodColorValue` タグとチャンネル幅を保持し、アルファを含む密配置 RGBA8 へ縮小しない。直径とオプションの
スカラー値には、ABI レコードで定義した正確な整数／Q16 表現を使う。

八つの API の入力は呼び出し中だけ借用され、出力レコードは呼び出し側所有のコピーなので解放関数を
必要としない。照会は EditorState／文書のリビジョン、ダイジェスト、未保存状態、履歴、ジャーナル、描画内容を
変更しない。更新は期待リビジョンが一致したときだけ一括適用する。意味上の変更がない場合は `EditorRevision`、
`EditorStateDigest`、未保存状態を保ち、意味上の変更は EditorState のリビジョン、ダイジェスト、未保存状態だけを
更新する。期限切れ、不正入力、オーバーフロー、割り当て失敗、パニックでは Core と出力レコードのどちらも
変更しない。アクティブ対象はレイヤー／プレーンの安定 ID の組であり、文書構造変更後の検証と決定的な
再解決も Core が行う。

Windows の `CoreHost` は、発行時の `DocumentSessionId + Generation` を所有スレッドで解決して照会／更新し、
結果を同じキーの表示キャッシュへ完全コピーする。文書、ビュー、ワークスペースを切り替えたときは対象 Core を
再照会する。同一文書の複数ビューは一つの EditorState を共有し、別セッションは分離される。ワークスペースに
残った以前の表示値を Core へ書き戻してはならない。

## 正規 Genesis とアセット取り込み（現行 ABI v18）

Core は、Genesis の安定した文書 ID、別個の Cell ID、不変の基底面を所有する。空の文書では
割り当て不要の `SolidWhite`、ラスタを文書として開く場合は正規ラスタアセットが基底面となる。基底面は、
編集可能なレイヤー／プレーン、選択マスク、借用スナップショットバッファではない。既存文書へのラスタ
インポート、アプリ内クリップボード、ライトテーブル入力元は同じ正規レジストリを使う。
`ImportRasterAsset` と 4 MiB 超の `ApplyRasterStroke` は、外部パスや呼び出し側バッファではなく、
不変のアセット識別子をプロシージャへ固定する。小さいストロークは所有済みのプロシージャ内ペイロードにする。

操作別ラスタ記述子は、対応 API の現行 `struct_size`、形式、寸法、ストライド、長さ、件数の上限を満たす
必要がある。正規ラスタは、ピクセル形式、sRGB／アルファの意味、幅、高さ、パディングのない正規ストライド、
論理ペイロード長を含めて識別する。別のコーデックやパスに由来していても、これらと論理ピクセル列が同じなら
重複排除できる。エンコード済みバイト列、ファイル名、パス、時刻、任意の出自情報は、アセット識別子や
リプレイ入力に含めない。偽装された寸法、ストライド、長さ、ダイジェスト／識別子、作業量上限は
確定前に拒否する。

この接続では、既存の同期式操作別取り込み関数を Core 所有スレッドで実行する。UI スレッド側で寿命が切れる
ポインターを作業項目へ保持してはならない。`CoreHost` アダプターは、発行時のセッション／世代と入力値を
所有してから呼び出す。`_v3` 付き API 群では、可変長ペイロードを一回の同期式上限付き呼び出しで世代付きの
アセット／サンプル ID に変換する。閉じた型付きキューに格納するのは、`CommandContext`、基準リビジョン、
対象、オペコード／スキーマ、固定値、ID だけであり、呼び出し側バッファは作業項目に入れない。

V14 は `GENS` / `ASST` に、アセットを基底とする Genesis と、保持対象の全分岐のアセットグラフを保存する。
通常保存、自動保存／復旧、バッチによる `.inkpod` 出力、再オープンは、同じ Core 所有の対応付けを使う。
Windows アダプターは成功後だけ、現在のパス、最近使ったファイル一覧、未保存表示を更新する。一般画像への
平坦化エクスポートは別の出力経路である。

## 編集状態と排他

Core が持つ一時編集状態は、確定済み文書と分離される。

| 状態                    | 開始／更新中の確定済みリビジョン・未保存状態・Undo | スナップショット                 | 完了                                                    |
| ------------------- | ---------------------------------------------- | --------------------------------------- | ------------------------------------------------------------- |
| 実行中ストローク        | 開始／追加では不変                                  | ストロークプレビューを観測できる | 終了時に実変更を高々 1 Undo 単位で確定。キャンセルは完全復元 |
| フィルター／ごみ取りプレビュー | 開始／更新では不変                           | 一時プレビューリビジョンを観測できる | 適用時に 1 Undo 単位で確定。キャンセルは元の基底を保持 |
| 浮動貼り付け            | 開始／変換では不変                                  | 浮動プレビューを観測できる       | 確定は高々 1 Undo 単位。キャンセルは基底を保持       |
| 撮影frame preview       | 開始／更新では不変                                  | snapshot-owned handle geometryを観測 | 適用時に高々1 Undo単位。Cancelは基底とID cursorを保持 |
| 消失点 preview           | 開始／更新では不変                                  | snapshot-owned point／radial geometryを観測 | 適用時に高々1 Undo単位。Cancelは基底とID cursorを保持 |

一つの Core に各状態は高々一つであり、実行中ストロークとフィルター／ごみ取りプレビューは同時に存在できない。
競合する文書編集、履歴移動、保存、オープン、レイヤー／プレーン操作、別プレビューの開始は
`INKPOD_STATUS_INVALID_STATE` になる。不変スナップショットの構築は一時状態中も許される。

実行中ストロークへのサンプル追加が失敗した場合、部分的なプレビューを後から終了処理で確定してはならない。
Core はセッションを無効化するため、フロントエンドはストロークを打ち切り、必要ならキャンセルしてから
次の開始へ進む。

## リビジョン、未保存状態、Undo の読み方

`document_revision` は確定済み文書の識別に使う。ビューだけの状態は `view_revision`、
フィルター／ストロークプレビューの描画更新は、スナップショット側の一時リビジョンで区別する。

| 操作の種類                                                    | 文書リビジョン       | 未保存状態                        | Undo                              |
| ------------------------------------------------------------- | -------------------- | --------------------------------- | --------------------------------- |
| 照会、スナップショット参照、タスク、ショートカット、ビューだけの操作 | 不変           | 不変                              | 不変                              |
| EditorState の照会または意味上の変更なし                      | 不変                 | 不変                              | 不変                              |
| EditorState の意味上の更新                                    | 不変                 | EditorState 保存点との差だけ変化 | 不変                              |
| ストローク開始／追加、プレビュー開始／更新、浮動変換          | 不変                 | 不変                              | 不変                              |
| ストローク終了、プレビュー適用、浮動状態の確定                 | 実変更時に 1 回進む  | 未保存                            | 高々 1 単位                       |
| 直接の文書編集                                                | 実変更時に 1 回進む  | 未保存                            | 原則 1 単位                       |
| Undo／Redo／履歴位置の移動                                    | 結果状態へ進む       | 保存点との位置で再計算            | カーソルを移動し項目は増やさない  |
| 現行 v28 の通常保存                                           | 不変                 | 置換成功時に文書／EditorState とも保存済み | 不変                    |
| 自動保存                                                      | 不変                 | 不変                              | 不変                              |
| 新規作成／インポート                                          | 新しい文書情報が正本 | 戻り情報が正本                    | 新しい Genesis／履歴              |
| v28 のオープン／復旧                                          | 実行時リビジョンを付け直す | 戻り情報が正本               | ファイルの全ジャーナル／履歴を復元 |

意味上の変更がない場合の厳密な出力やリビジョンは、各関数の Doxygen 契約に従う。フロントエンドはファイル時刻ではなく、
Core が返す文書フラグと保存点に基づいて未保存状態を表示する。

## 典型的な起動から終了まで

### 1. Core を作る

Core エンジンスレッドを開始し、そのスレッド上で ABI バージョンを確認して Core を作る。

```cpp
InkpodCoreConfig config{};
config.struct_size = sizeof(config);
config.abi_version = INKPOD_ABI_VERSION;
config.feature_flags = INKPOD_FEATURE_NONE;

InkpodCore* core = nullptr;
Check(inkpod_core_create(&config, &core));
```

各 `core` 所有変数は `CoreHost` のセッションエントリが一意に保持する。生の Core ポインターを UI
メッセージの `WPARAM` / `LPARAM` に積まず、UI 通知はセッション ID／世代を含む上限付きキューの
値トークンで取り出す。

### 2. 新規作成またはオープン

新規作成ではプラットフォームアダプターが 0 でない 128 ビット UUID、寸法、DPI を用意する。オープンでは、
Windows ファイルダイアログが得たパスを UTF-8 バイト列へ変換し、Core エンジンスレッドへコピーして渡す。
いずれも成功時の `InkpodDocumentInfo` を UI 状態の初期値とする。

オープン／デコードに失敗した場合、現在の文書は保持される。フロントエンドは失敗前にタブやレンダラー状態を
破棄せず、成功通知を受けてから切り替える。

### 3. ストロークを逐次入力する

UI/Input スレッドはポインター履歴を入力元 Canvas のクライアントデバイスピクセル座標で正規化し、
その Canvas に結び付く view ID とともに上限付きキューへバッチで入れる。Core エンジンスレッドは指定 view の
変換、スタイル、最初のサンプルでストロークを開始し、後続サンプルを
追加する。サンプルごとに FFI を往復したり、スナップショットを作ったりしない。

```text
ポインター押下
  → ストローク開始（スタイル + 初期サンプル）
  → ストローク追加（0回以上のサンプルバッチ）
  → ストローク終了
ポインターのキャンセル／キャプチャ喪失
  → ストロークをキャンセル
```

描画中の見た目が必要なら、フレーム周期に合わせて開始／追加後にスナップショットを作る。終了処理は
ポインター解放と同じ順序で必ず Core キューに入り、成功したストローク全体を 1 Undo 単位にする。

### 4. スナップショットの所有権をレンダラーへ移す

Core エンジンスレッドは所有変数を NULL から構築し、スナップショットの受け取り先へ生の所有ポインターを
ちょうど一回渡す。受け取り先は、キュー投入の成否にかかわらず解放責任を引き受ける。

```cpp
InkpodSnapshot* next = nullptr;
Check(inkpod_core_build_snapshot(core, &options, &next));

snapshot_sink.Submit(next); // この呼び出しで所有権を移動する
next = nullptr;             // Core エンジン側は以後参照・解放しない
```

`Submit` がキュー飽和でスナップショットを採用しない場合も、受け取り先の内部で直ちに解放する。
呼び出し元へ所有権を戻す設計と混在させない。スナップショットポインターを `PostMessage` の値引数として
送らず、所有権を明示した C++ キューを使う。

### 5. レンダラーが読み取り、解放する

レンダラースレッドはスナップショットからラスタビュー、変換、オーバーレイ、ベクタービューを取得し、
親スナップショットの生存中だけ借用する。古い保留中スナップショットを最新のものへ置き換える場合、
置換された所有者をその場で解放する。描画完了、デバイスリセット、ウィンドウ終了の各経路でも、
所有者を一度だけ解放する。

### 6. 終了する

推奨順序は次のとおりである。

```text
UI/Input の新規投入を停止
  → 実行中のストローク／プレビュー／浮動状態を終了／適用またはキャンセル
  → Core 作業キューを空にする
  → スナップショットの受け取り先を閉じる
  → レンダラーの保留中／現在のスナップショットをすべて解放
  → タスク／レポート／プレビュー／グラフ／クリップボード／バイトバッファを解放
  → Core エンジンスレッド上で Core を破棄
  → Core エンジン／レンダラースレッドを終了待ち
```

`inkpod_core_destroy` は生存中の一時状態を確定せず破棄し、所有変数を NULL にする。スナップショットは
Core より長く生存できるが、通常の終了処理では先にレンダラー側の所有者を解放すると、漏れを検出しやすい。

## タスク、進捗、キャンセル

長時間処理では、任意のスレッドでタスクハンドルを先に作り、そのハンドルを Core 操作が戻るまで所有する。
UI スレッドは照会で進捗を読み、ユーザー操作でキャンセルを要求できる。

```text
任意スレッド: タスク作成
Core エンジン:      Core 操作（タスク）─────────→ 復帰
UI スレッド:                 照会／照会／キャンセル
任意スレッド:                                           タスク解放
```

キャンセルは要求であり、即時終了の保証ではない。Core がキャンセル状態を確認すると、段階的に構築した結果を
破棄して `INKPOD_STATUS_CANCELLED` を返す。ファイル出力を伴う処理は、完成した一時ファイルだけを
置換対象とし、キャンセルや失敗で部分出力を確定しない。

Color chart 生成では `inkpod_core_color_chart_preview_create_task` が、呼出し中だけ
borrow する生存中の `InkpodTask` を使って source row 単位の進捗と cooperative cancel
を公開する。成功時の `InkpodColorChartPreview*` は Rust 所有の immutable object であり、
候補色、名前、頻度、差分 summary と、発行元 document UUID／revision を内部に所有する。
Core や caller buffer への借用は保持しない。`inkpod_color_chart_preview_get` は名前を
NULL/0 で size query した後、caller buffer へ exact byte length をコピーする。
`inkpod_core_color_chart_preview_apply` は同じ document identity、current revision、非overflow、
chart unlocked のときだけ Core owner thread 上で一回の canonical transaction を commit する。
別 document、stale revision、lock、overflow、invalid record、cancel／failure は chart、history、
journal、dirty、ID を変更しない。所有者は Apply／Cancel／stale／failure のいずれでも、worker が
task と preview を借用し終えた後に `inkpod_color_chart_preview_release` を exactly once 呼び、
owner slot を NULL にする。NULL owner、短い record、短い name buffer、二重 release は negative
status となる。Windows adapter は発行時 `CommandContext` と世代付き request token を固定し、
旧 task を cancel して最新 token の completion だけを比較 dialog に渡す。

出力色安全ガードでは `inkpod_core_select_output_color_guard` が、caller-owned の
`InkpodOutputColorGuardRequest` と `InkpodOutputColorGuardResult` を呼出し中だけ
借用する。唯一の closed profile は
`INKPOD_OUTPUT_COLOR_GUARD_BT709_CONSERVATIVE_YCBCR` であり、正式な規格適合判定や
自動 legalize を意味しない。Core 所有スレッドで、生存中の caller-owned
`InkpodTask` と発行時の base document revision を渡す。Core は committed visible
straight-alpha composite を source row 単位で走査し、task の進捗と cooperative cancel
を更新する。成功時だけ `New`／`Add`／`Subtract`／`Intersect` を一つの canonical
transaction として選択へ反映し、result に確定 revision、走査 pixel、選択 pixel、
透明 skip pixel 数をコピーする。入力／結果／task を保持または解放せず、Rust 所有
allocation も返さない。未知 profile／operation、短い record、予約値／flag、NULL、
Cancel、stale、overflow、failure は selection、revision、history、journal、dirty、ID
を変更しない。意味上の no-op も revision と履歴を進めない。Windows adapter は
発行時 `CommandContext` と文書世代を固定し、完了時に同じ target へ summary を表示する。
profile の UI 既定値は versioned HKCU application setting へ保存し、次回通常起動時に
closed enum として復元する。smoke mode は利用者の registry を変更せず、この設定は
document、EditorState、canonical procedure、`.inkpod` section のいずれにも含めない。

バッチ実行だけは、キャンセル／失敗時にもレポートの所有権を返すことがある。戻りステータスを確認した後も、
`out_report != NULL` なら内容を読み、必ず解放する。

## 保存、自動保存、復旧

通常保存では、v28 の必須セクション `META` / `GENS` / `ASST` / `PROC` / `EDIT`、保持対象の不透明な任意
セクション、チェックポイントの作成条件を満たす場合だけ任意の `CKPT` を構築する。保存後に設定予定の
文書／EditorState 保存点を含むコンテナは、同じディレクトリの一時ファイルへ複数回に分けて書き込む。
フラッシュ、同期、クローズを終えてから置換する。成功後だけ通常保存パスと両保存点を Core へ公開するため、
EditorState だけが
未保存の場合も、再オープン直後は保存済みになる。失敗時に元ファイルを切り詰めず、文書／EditorState の
どちらの保存点も変更しない。

自動保存とエクスポートは、出力を原子的に書いても通常保存パス、文書／EditorState 保存点、未保存状態を
変えない。通常の v28 オープンでは、Genesis、アセット、プロシージャジャーナル、カーソル／分岐、すべての
ID 発行状態、EditorState、両保存点を、段階的に構築した Core で検証・復元してから、現在の Core 状態を
一回だけ置換する。`InkpodCore` の `_v3` 付きオブジェクトレジストリの世代自体は、オープンで更新されない。

`CKPT` は、リプレイエポック、ジャーナル接頭部、状態ダイジェスト、次に発行する安定 ID、履歴カーソル、
作業量などがすべて一致するときだけ採用する。不一致なら正本である `PROC` から全リプレイする。不正形式の
チェックポイント、
またはコンテナハッシュが壊れたチェックポイントはオープン全体を拒否し、既存 Core を変更しない。実際に
採用した経路は `inkpod_core_get_persistence_info` の `open_strategy` で取得できる。復旧オープンも同じ内容を
復元するが、通常保存先としてのパスを引き継がず、両保存点を解除して、未保存、復旧済み、パスなしの状態にする。
以前の通常ファイルを上書きするには、ユーザーが明示したパスで改めて通常保存する必要がある。

ストローク、プレビュー、浮動状態の実行中は保存やオープンを実行せず、Core キュー上で完了または
キャンセルした後に行う。

## 診断メッセージの取得

エラーメッセージはスレッドローカルな UTF-8 文字列である。失敗した API と同じスレッドで、まず末尾の NUL を
含む必要バイト数を取得し、次に呼び出し側のバッファへコピーする。別スレッドへステータスとメッセージを
通知する場合は、Core エンジンスレッドで `std::string` へコピーしてからキューへ渡す。

```cpp
std::string CopyInkpodError() {
    std::uint64_t required = 0;
    if (inkpod_error_message_size(&required) != INKPOD_STATUS_OK || required == 0) {
        return {};
    }

    std::string text(static_cast<std::size_t>(required), '\0');
    std::uint64_t written = 0;
    if (inkpod_error_message_copy(
            reinterpret_cast<std::uint8_t*>(text.data()), required, &written)
        != INKPOD_STATUS_OK) {
        return {};
    }
    text.resize(static_cast<std::size_t>(written));
    return text;
}
```

診断文にはユーザーのパスや画像内容を無制限に記録せず、UI 表示やログ側でも同じ方針を守る。

## 実装時の確認項目

- Core の作成、全操作、破棄が同じ Core エンジンスレッドに固定されている。
- すべての構造体で `struct_size`、予約領域、機能フラグ、レコードストライドを初期化している。
- 所有権を返す出力変数を NULL で開始し、所有権移動後に元変数を NULL にしている。
- 解放後に借用列やコピー済みの別名ポインターを使っていない。
- ストローク、プレビュー、浮動状態の状態機械と、失敗時のキャンセル経路がある。
- スナップショットの受け取り先が、キュー投入の成否にかかわらず解放責任を一意に持つ。
- Core 呼び出しが終わる前にタスクを解放していない。
- 戻りステータスが失敗でも返り得るバッチレポートを解放している。
- 未保存状態をファイル時刻から推測せず、Core の文書フラグを使っている。
- C11 と C++20 の両方でヘッダーをインクルードし、Rust 宣言とのずれを検査するテストを通している。
- `inkpod_core_preview_scoped_color_replace` と
  `inkpod_core_apply_scoped_color_replace` は完全サイズの
  `InkpodScopedColorReplaceInput` を caller-owned borrowed record として受け取る。point span は call 中だけ
  借用し、Rust は保持・解放しない。preview output と apply result も caller-owned で、Rust 所有 allocation は
  返さない。mode は raster color/main-line を明示し、region は pen、rectangle、
  polyline、lasso のいずれかだけを受理する。base document revision、stable Plane ID、native-depth target/replacement、
  size/flags/reserved/alignment/count/stride を境界で検証し、stale、invalid、overflow、hidden/locked target、failure は
  文書・履歴・ID・dirty を進めない。preview は常に非変更で、apply の実変更だけが一つの canonical Undo 単位になる。
