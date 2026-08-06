# FFI 利用ガイド

Inkpod の公開 C ABI は `include/inkpod/core_ffi.h` を仕様の正本とする。
各型・関数の引数、`struct_size`、NULL 可否、スレッド、所有権、出力、revision、dirty、Undo、
排他状態、戻りステータスはヘッダーの Doxygen コメントに記載している。この文書は関数一覧を
複製せず、frontend が ABI を安全に利用するための横断的な契約と典型的な呼び出し順序を説明する。

## 全体像

Windows frontend は UI/Input、Core engine、Renderer の三つの長寿命 thread に分かれる。

- UI/Input thread は Windows message と pointer history を受け取り、入力を bounded C++ queue へ渡す。
- Core engine thread は `InkpodCore` を作成し、すべての Core 操作と snapshot 構築を行う。
- Renderer thread は D3D11/DXGI/Direct2D resource と `Present` を所有し、immutable snapshot だけを読む。

Core は C++ callback を呼ばない。UI thread は Core や Renderer の完了を同期的に待たず、Renderer は
古い未描画 snapshot だけを置き換えてよい。stroke の begin/end/cancel と入力 sample 自体は捨てない。

```text
UI/Input thread
  └─ 正規化した入力 batch
       → Core engine thread
           ├─ Core command / stroke
           └─ immutable snapshot 構築
                → snapshot queue（所有権を1回だけ移動）
                    → Renderer thread
                        ├─ borrowed view を描画
                        └─ snapshot release
```

## ABI と構造体

数値は固定幅 C 型を使う。Rust の `Vec`、`String`、slice、enum layout、trait object、reference と、
C++ の STL、reference、exception は ABI を越えない。文字列は UTF-8 pointer と byte count、配列は
pointer、count、byte stride で表す。

拡張可能な構造体を渡すときは、少なくとも次を守る。

```cpp
InkpodSnapshotOptions options{};
options.struct_size = sizeof(options);
options.feature_flags = INKPOD_FEATURE_NONE;
```

- `struct_size` は必ず呼び出し側が設定する。出力構造体でも同じである。
- ABI v3 で既知の構造体末尾まで読み書きできるサイズが必要である。
- `reserved` は 0、未知の必須 feature flag は指定しない。
- record span は各 record の `struct_size` と `*_stride_bytes` の両方を設定する。
- count、stride、alignment、全 span の byte 範囲が有効でなければならない。
- count が 0 の任意 span だけはデータ pointer を NULL にできる。各 API の例外はヘッダーを参照する。
- 入力、出力、opaque object の記憶域を重ねない。

ABI version は Core 作成前に比較できる。`INKPOD_ABI_VERSION` と library の戻り値が異なる場合は、
Core を作らず互換性エラーとして扱う。

ABI v3 は value/ID-only primitive control plane を追加した。ABI v2 で公開名から実装時の
マイルストーン番号を除いた task API は引き続き
`InkpodTask` / `InkpodTaskInfo` / `INKPOD_TASK_*` / `inkpod_task_*`、共有raster入力は
`InkpodRasterSourceInput` を使用する。v1のマイルストーン名は公開aliasとして残していないため、
旧headerを使うcallerはv2 headerへ更新して再ビルドする必要がある。構造体レイアウト、数値定数、
所有権、thread、statusの契約はこの名称変更では変えていない。

既存の raster-open/import、
clipboard、Light Table 入力は ABI v2 の bounded call の中で同期的に検証、copy、canonicalize、
intern される。stroke sample は同期的に Rust-owned canonical bytes へ copy され、4 MiB 以下は
procedure inline payload、4 MiB 超は canonical sample asset になる。sequence source は従来どおり
bounded な Rust-owned raster copy であり、vector primitive の asset 接続は M6 の範囲である。
ABI v3 caller は generation-tagged runtime object ID を明示 release する。ABI v2 caller が使う
canonical asset retention は従来どおり Core 内部であり、v3 runtime object ID と persistent
content-addressed `AssetId` は別 namespace である。

## ABI v3 value/ID control plane

`InkpodObjectId` は `object_type + Core generation + monotonic value` からなる固定幅 record である。
Core、snapshot、task、color array、sample stream、raster asset、thumbnail、export は異なる type を持つ。
ID は発行元 Core generation だけで有効であり、別 Core、destroy/recreate 後、release 後には使えない。
wrong type/zero value/unknown opcode は `INKPOD_STATUS_INVALID_ARGUMENT`、wrong generation/stale/double release は
`INKPOD_STATUS_INVALID_STATE`、未知 feature/schema は `INKPOD_STATUS_UNSUPPORTED` となる。
失敗時に document、history、journal、revision、dirty、object registry、出力 record の部分変更はない。

`InkpodPrimitiveRequestV3` は pointer、callback、path、native/STL object を含まない。stable opcode、
schema version、base document revision、target ID、generation-tagged payload ID、固定幅 tool/plane/color/
diameter/flags だけを値で持つ。現在の closed catalog は既存 canonical executor slice の
`SetMainLineColor` schema 1、`ReplacePalette` schema 1、`ApplyRasterStroke` schema 2、
`ImportRasterAsset` schema 1 である。可変 palette/sample/raster は先に
`inkpod_core_register_*_v3` の一回の bounded call で deep copy し、primitive request は返された ID
だけを参照する。caller は登録 call 復帰後に元 buffer を変更・解放できる。実行は
`Core::execute_primitive` に委譲され、旧 main-line/palette/stroke/import FFI wrapper と別の意味実装を
持たない。success は result を完全に書き、semantic no-op は committed flag、revision、history、
dirty、persistent ID を進めない。stale base、active stroke、invalid target/payload、overflow、allocation
failure は atomic error である。

snapshot、thumbnail、export、task は Rust-owned runtime ID である。snapshot metadata と tile/guide/
vector record は `first + capacity + stride` の batched copy、tile pixels と thumbnail/export bytes は
`InkpodBufferCopyV3 { offset, bytes, byte_capacity, written_bytes, total_bytes }` で取得する。capacity 0 の
byte query は `bytes == NULL`、record query は output NULL/stride 0 とし、実 storage はその一回の call
だけ borrowed である。Core は caller output pointer を保持しない。ID 自体は copy できるが、同じ live
object の query/release は Core owner thread 上で直列化し、最後に `inkpod_core_object_release_v3` を
正確に一回呼ぶ。Core ID は独立 release せず `inkpod_core_destroy` で終端する。

全 `inkpod_core_*_v3` call は、task query/cancel を含め Core owner thread 限定である。ID record は UI
queue へ値で渡せるが、その解決、copy、release は発行元 Core の owner thread で行う。旧 opaque
snapshot/task API の任意-thread例外は、それら固有の既存 contract であり v3 ID registry へは適用しない。

## スレッド契約

`InkpodCore` は single-writer かつ thread-affine である。作成、文書操作、view 操作、stroke、履歴、
保存／open、snapshot 構築、destroy は、すべて Core を作成した Core engine thread から呼ぶ。
違反は `INKPOD_STATUS_WRONG_THREAD` となり、handle や出力の所有権は移動しない。

`inkpod_core_get_resource_usage` も Core owner thread 限定の read-only query である。
caller-owned の完全な `InkpodResourceUsage` を一回の呼び出し中だけ借用し、成功時だけ値を
copy する。NULL、短い構造体、wrong thread、panic では出力を変更しない。tile/history、
render cache、CPU staging、light table/reference、sequence source、thumbnail cache は
logical payload の category 別推定値であり、allocator や GPU driver の private resident
size と COW clone の物理共有量は推測しない。query は snapshot を構築せず、document/view
revision、dirty、history、savepoint を変更しない。

Windows frontend の `CoreHost` は複数の `InkpodCore` owner 変数を一つの Core engine thread 上に
保持する。各 owner は `DocumentSessionId` と `Generation` の組で選択し、work item は投入時にその組を
値で確定する。同じ数値の Core-local document/view ID や revision は session をまたいだ routing key に
しない。session close は先に新規投入を拒否し、受理済み work と live stroke を解決してから、作成した
同じ thread 上で該当 owner だけを destroy する。この frontend registry は C ABI や Rust handle の
ownership 契約を変更しない。

canonical document mutation は closed `PrimitiveWork` variant で queue される。record は issue-time の
session/generation、`CommandContext` の target IDs、base revision、`InkpodPrimitiveRequestV3` の値、
snapshot/document-info publication flags、exactly-once sequence/completion stateだけを持ち、raw pointer、
closure、external path、STL containerを持たない。palette/sample/rasterの caller memory は enqueue 前の
register callでRust-owned IDへ変換する。queue飽和で未受理のsequence/pending countはatomicにrollbackし、
受理済み primitive はactive stroke終了後、close、shutdown drainでも高々一回だけ実行または明示解決する。
未移行の production route とquery/initializerが通る`LegacyInvokeWork`はM6互換bridgeとして別 variant に
隔離され、canonical primitive queueの意味ownerではない。M6を先取りしてそのrouteをprocedure化しない。

例外は immutable handle と atomic task である。

- snapshot の accessor と release は任意 thread で呼べる。同じ snapshot の参照と release は外部同期する。
- task と batch task の query/cancel は、Core operation の実行中に別 thread から呼べる。
- task の release は任意 thread でよいが、その task を使う Core call が戻るまで待つ。
- immutable batch graph、preview、report、byte buffer、encoded sequence、clipboard の accessor/release は
  Core affinity を持たない。同じ handle の利用と release は呼び出し側で同期する。

任意 thread で呼べることは、同じ owner 変数を同時に解放してよいことを意味しない。

## Shortcut sequence 契約

Windows frontend は menu command と同じ `command_id` を持つ
`InkpodShortcutSequence` 表を Core engine thread で登録する。各列は1–4個の
`InkpodShortcutStroke` からなり、command ID の重複、完全一致、一方が他方の
prefix になる表は Core が transactional に拒否する。

- `inkpod_core_shortcut_defaults_set` は検証済み既定値と現在値を同時に置き換える。
- `inkpod_core_shortcut_sequences_set` は現在値だけを置き換え、`reset` は登録済み既定値へ戻す。
- `inkpod_core_shortcut_sequences_copy` は件数queryとcaller-owned strided buffer copyに対応する。これら3関数は Core owner thread 限定である。
- `inkpod_shortcut_sequence_resolve` は Core handle を取らない pure helper で、Core からcopyした immutable形状の表に対して任意 thread から `NONE` / `PREFIX` / `EXACT` を返す。UI keydown ごとに Core engine thread へ往復しないためのAPIである。

これらは document revision、dirty、Undo を変更しない。永続化形式や
text-focus guard、入力timeout、衝突時のUI上の交換policyはfrontendの責務である。

## 所有権と有効期間

### borrowed 入力

通常の `const T*` 入力、UTF-8 span、byte span、sample span は、その API 呼び出し中だけ borrowed
（借用）である。保持が必要な API は戻る前に意味値をコピーする。caller は API が戻った後に入力
buffer を再利用または解放できる。

canonical asset ingestion を行う API もこの規則の例外ではない。Core owner thread 上の call が
成功を返すまでに、raster-open/import、clipboard、Light Table の descriptor と encoded/decoded bytesを
検証して Rust-owned canonical bytes へ copy し、内容アドレス付き registry へ intern する。stroke
sample span も復帰前に copy し、4 MiB 以下はprocedure inline payload、4 MiB超は canonical sample
assetへ確定する。sequenceはboundedなRust-owned raster copyであり、vector primitive asset接続はM6である。
Core、transient session、journal、snapshot は復帰後に caller record、buffer、file name、path を参照しない。
同じ canonical descriptor と logical payload は同じ registry entry に deduplicate されるが、その
内部参照 count や allocation address は ABI の一部ではない。

### Rust-owned handle

opaque handle を生成する API は `T** out_*` を受け取る。owner 変数は呼び出し前に NULL でなければ
ならず、成功時だけ Rust-owned handle が入る。対応する release/destroy は pointer-to-owner を受け、
所有権を消費して同じ owner 変数を NULL にする。

```cpp
InkpodSnapshot* snapshot = nullptr;
InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
if (status == INKPOD_STATUS_OK) {
    // snapshot を所有している
}

inkpod_snapshot_release(&snapshot);
// snapshot == nullptr。同じ owner 変数での再 release は成功 no-op。
```

release 後は、handle から得た tile、pixel、guide、vector、文字列、byte span と、コピーしておいた
別名 pointer を一切使わない。Rust が確保した object を `free`、`delete`、`CoTaskMemFree` で解放しない。

主な owner と borrowed view の関係は次のとおりである。

| owner                | 生成                              | borrowed view の有効期間                                  | 解放                          |
| -------------------- | --------------------------------- | --------------------------------------------------------- | ----------------------------- |
| Core                 | create 成功から destroy まで      | Core pointer は owner thread の call 中だけ利用           | Core owner thread             |
| snapshot             | build 成功から release まで       | tile/pixel/transform/guide/vector view は release まで    | 外部同期した任意 thread       |
| clipboard            | copy/create 成功から release まで | raster export は caller buffer。内部 payload は公開しない | 外部同期した任意 thread       |
| byte buffer          | export 成功から release まで      | byte span は release まで                                 | 外部同期した任意 thread       |
| encoded sequence     | export 成功から release まで      | item name/byte span は release まで                       | 外部同期した任意 thread       |
| task / batch task    | create 成功から release まで      | query 値は caller へのコピー                              | Core call 終了後に任意 thread |
| batch graph          | create/load 成功から release まで | execute/preview 中は graph が生存する必要がある           | 外部同期した任意 thread       |
| batch preview/report | Core call の出力から release まで | item の UTF-8 span は親 handle の release まで            | 外部同期した任意 thread       |

snapshot の raster tile storage は snapshot 側で独立して参照計数されるため、snapshot は作成元 Core より
長く生存できる。ただし通常の shutdown では Renderer queue を drain して snapshot を先に解放すると、
所有権の追跡が簡潔になる。

canonical asset registry は `InkpodCore` が所有し、ABI-v2 caller に独立した opaque handle や release
API を公開しない。ABI-v3 の raster/sample ID は bounded ingestion 後の runtime object を所有するための
別 registry entry であり、primitive canonicalization/commit 時に inline payload または persistent
`AssetId` へ解決される。runtime object ID を release しても committed procedure の asset retention は
失われない。Genesis、retained journal branch/redo tail、既知の persistent reference、live transient
owner が retention root となる。現在の materialized document や checkpoint だけを見て解放せず、
session close は受理済み Core work と transient owner を drain してから registry 全体を owner thread
上で破棄する。失敗した ingestion/commit は document、history、journal、revision、dirty、公開済み
retention edge、caller-owned output を部分変更しない。

## 出力と失敗

値出力は caller-owned である。成功時だけ利用し、失敗時はヘッダーが部分出力を保証する場合を除いて
読まない。特に owner 出力は呼び出し前に NULL にし、戻り値が失敗でも念のため NULL のままか確認する。

部分出力を意図的に返す代表的なパターンは次のとおりである。

- `INKPOD_STATUS_BUFFER_TOO_SMALL` は必要な count/byte 数を返す。
- `INKPOD_STATUS_FILL_OVERFLOW` は漏れ候補座標を返すが、文書を変更しない。
- cancelled batch execution は `INKPOD_STATUS_CANCELLED` と owned report を同時に返すことがある。
- error-message copy の失敗は written byte 数を 0 にし、同じ thread の diagnostic を保持する。
- `inkpod_core_validate_plane_creation` は UI の確定前に種類と形式を検査する owner-thread 限定の読み取り専用 query である。成功・失敗のどちらでも文書、stable ID、revision、dirty、history を変更しない。実際の `inkpod_core_tree_edit` も同じ制約を再検証するため、query 後に状態が変わっても不正な作成は commit されない。

Rust panic は ABI 境界で捕捉され `INKPOD_STATUS_PANIC` になる。C++ exception も ABI を越えさせない。

## size query と caller-owned buffer

`inkpod_core_locator_neighborhood` は modeless locator の magnified view に必要な
複数 pixel を一回の owner-thread ABI call で返す。`radius` は 0..16、出力は常に
`(radius * 2 + 1)` の正方形 packed straight RGBA8 で、文書外は透明になる。
`pixel_capacity == 0` かつ `pixels_rgba8 == NULL` で metadata と
`required_bytes` を問い合わせ、十分な caller-owned buffer を設定して再度呼ぶ。
buffer は call 中だけ借用され、Core は保持しない。query/copy とも document、view、
revision、dirty、Undo を変更しない。

可変長出力は、まず NULL/容量 0 で必要量を問い合わせ、caller が確保した後に再度呼ぶ。API により
必要量の field 名は異なるため、各構造体の Doxygen 契約を確認する。

```cpp
InkpodClipboardRasterBuffer output{};
output.struct_size = sizeof(output);

InkpodStatus status = inkpod_clipboard_render_rgba8(clipboard, &output);
if (status != INKPOD_STATUS_BUFFER_TOO_SMALL && status != INKPOD_STATUS_OK) {
    // diagnostic を取得して中止
}

std::vector<std::uint8_t> pixels(static_cast<std::size_t>(output.required_bytes));
output.pixels_rgba8 = pixels.data();
output.pixel_capacity = pixels.size();
status = inkpod_clipboard_render_rgba8(clipboard, &output);
```

2 回の call の間に対象 object を変更または解放しない。Core 文書を対象とする query では、必要量取得と
本取得を同じ Core engine work item 内で行うと revision drift を避けられる。

`inkpod_core_layer_thumbnail` もこの caller-owned buffer 方式を使う。最初の call は
`InkpodLayerThumbnailBuffer::pixels_rgba8 = NULL`、`pixel_capacity = 0` とし、返された
`required_bytes` を確保してから同じ stable layer ID と最大寸法で再度呼ぶ。結果は上から下へ packed
された straight-alpha RGBA8 で、`revision` は生成元の committed document revision である。
buffer の確保・解放は caller が担い、Core は pointer を保持しない。レイヤー自体が非表示でも内容を
確認できる一方、プレーンの表示状態とレイヤー／プレーン不透明度は thumbnail に反映される。

`inkpod_core_sequence_thumbnail_get` も同じ caller-owned query/copy 契約を使う。
`pixels_rgba8 = NULL`、`pixel_capacity = 0` の query で `required_bytes`、寸法、stride、
checksum を取得し、確保後の二回目で bounded straight-alpha RGBA8 をコピーする。pointer は
呼び出し中だけ借用され、Core は保持しない。二回の call は同じ
`DocumentSessionId + generation` の CoreHost work item 内で行い、別 active document を再解決しない。

`inkpod_core_sequence_import_mixed_encoded` の `InkpodNamedRasterInput` span は caller-owned
borrowed 入力である。record、UTF-8 名、encoded bytes は呼び出し終了まで有効とし、Core は全 record
の構造、format、長さを検証してから各画像を decode する。全件成功時だけ sequence を一括置換し、
一件でも invalid/decode/allocation failure なら旧 sequence と current document、dirty、Undo を保つ。

## subpalette/reference snapshot 契約

read-only reference viewer は、対象 `DocumentSessionId + Generation` の Core owner thread で
`inkpod_core_subpalette_set` と `inkpod_core_view_create` を呼び、返った Core-local view ID を
その session namespace の外へ routing key として使わない。

- `inkpod_core_subpalette_view_apply` はその view の zoom、pan、flip、viewport だけを変更する。
- `inkpod_core_subpalette_view_sample` は同じ view transform を通した device 座標を half-open bounds で
  検証し、source の RGBA8/16 depth を caller-owned `InkpodColorValue` へコピーする。
- `inkpod_core_subpalette_build_snapshot` は NULL の owner 変数へ Rust-owned immutable snapshot を返す。
  通常 snapshot と同様に、成功後は sink または caller のどちらか一方だけが
  `inkpod_snapshot_release` の責任を持つ。

これら三関数と view close は Core owner thread 限定である。snapshot accessor/release だけが
外部同期した任意 thread で利用できる。reference raster は editable document へ install されず、
document revision、dirty、Undo/Redo、savepoint を変更しない。Windows Canvas は stroke を consume し、
編集 command を Core へ送らない。target rebind/close/shutdown では先に Canvas sink を unbind し、
捕捉済み session/generation の Core 上で view を close してから Canvas owner を破棄する。

## EditorDefaults / EditorState ABI v2

M3 は ABI version を変更せず、次の七つの Core-owner-thread API と固定幅 record を ABI v2 へ
additive に追加する。

- `inkpod_core_get_editor_defaults` は document 作成前にも有効な Rust-owned immutable
  `InkpodEditorDefaults` を caller-owned record へコピーする。built-in initial document spec と
  built-in editor values は application preference ではなく、新規 document 作成時に Core が
  session の Genesis/EditorState へ明示的にコピーする。
- `inkpod_core_get_editor_state` は現在の `InkpodEditorStateInfo` を副作用なくコピーする。
- `inkpod_core_update_editor_state` は `InkpodEditorStateUpdate` の kind と exact expected
  `EditorRevision` を検証し、成功時の完全な `InkpodEditorStateInfo` をコピーする。update kind は
  active tool、tool color、tool diameter、fill、selection、vector、active target、palette cursor の
  closed set である。
- `inkpod_core_editor_stroke_begin` は caller-owned `InkpodEditorStrokeInput` の sample span を call 中だけ
  borrow し、tool 0 なら active tool、非0なら指定 raster tool の Core-owned styleを選び、RGBA8/RGBA16
  exact-depth color、Q16 diameter、stable target を begin 時に一度だけ canonical stroke argument へ
  コピーする。selector は locator の固定鉛筆等に使うが、caller は color/diameter/target を渡さない。
  append/end は後続の EditorState を再参照しない。
- `inkpod_core_apply_fill_for_editor_target` と
  `inkpod_core_apply_selection_for_editor_target` は既存の bounded input/output record に、gesture beginで
  captureしたstable layer/plane ID pairを添えて実行する。pairは同じdocument namespace内で再検証し、
  gesture中のEditorState変更で別targetへ再解決しない。既存entrypointは一回の同期command開始時に
  current targetをCore内でcaptureする経路として維持する。
- `inkpod_core_select_color_for_editor_target` は色選択command開始時にcaptureしたstable layer/plane
  ID pairとexact-depth `InkpodColorValue`を使う。後続EditorState変更でsource planeをretargetせず、
  既存`inkpod_core_select_color`は同期command開始時のcurrent targetをCore内でcaptureして委譲する。

公開 record は `InkpodEditorFillOptions`、`InkpodEditorSelectionOptions`、
`InkpodEditorVectorOptions`、`InkpodEditorStateInfo`、`InkpodEditorDefaults`、
`InkpodEditorStateUpdate`、`InkpodEditorStrokeInput` である。caller は入力の
top-level recordと、その入力がadvertiseする各nested recordの`struct_size`をABI v2 headerの
完全な`sizeof(record)`以上に設定し、reservedと未知flagを0にする。query/updateの出力は
callerがtop-level outputの`struct_size`だけを提示し、Coreが成功時に完全なcaller-owned copyと
各nested outputの`struct_size`を書き込む。短いtop-level record、使用する短いnested input、
NULL、未知enum/update kind、非有限・範囲外の値、0または存在しないstable target IDは拒否する。
RGBA8/RGBA16 は既存の `InkpodColorValue` tag と channel 幅を保持し、
alpha を含め packed RGBA8 へ縮小しない。diameter と option scalar は ABI record で定義した exact
integer/Q16 表現を使う。

七 API の入力は call 中だけ borrowed、出力 record は caller-owned copy であり、release 関数を
必要としない。query は editor/document revision、digest、dirty、history、journal、render content を
変更しない。update は expected revision が一致したときだけ一括適用する。semantic no-op は
`EditorRevision`、`EditorStateDigest`、dirty を保持し、semantic change は editor revision/digest と
editor dirty だけを更新する。stale、invalid、overflow、allocation failure、panic では Core と出力
record のどちらも変更しない。active target は layer/plane の stable-ID pair で、document topology
変更後の検証と決定的な再解決も Core が行う。

Windows の `CoreHost` は issue-time の `DocumentSessionId + Generation` を owner thread で解決して
query/update し、結果を同じ key の presentation cache へ deep copy する。document/view/workspace
切替は対象 Core を再 query する。同一 document の複数 view は一つの EditorState を共有し、別
session は分離される。workspace の以前の表示値を Core へ戻してはならない。

## M4 canonical Genesis / asset ingestion under ABI v2

M4 の Core は Genesis の stable Document ID と distinct Cell ID、および immutable base surface を
所有する。blank document の base は allocation-free な `SolidWhite`、raster-open-as-document の base
は canonical raster asset である。base は editable layer/plane、selection mask、borrowed snapshot
buffer ではない。既存文書への raster import、private clipboard、Light Table source は同じ
canonical registry を使う。`ImportRasterAsset` と 4 MiB 超の `ApplyRasterStroke` は外部 path や
caller buffer ではなく immutable asset identity を procedure に固定し、小さい stroke は owned
inline payload に固定する。

ABI v2 の raster descriptor は、対応 API の既存 `struct_size`、format、dimension、stride、length、
count 上限を満たす必要がある。canonical raster は pixel format、sRGB/alpha semantics、width、height、
padding のない canonical stride、logical payload length を含めて識別される。別 codec/path 由来でも
これらと logical pixel bytes が同じなら deduplicate できる。encoded bytes、file name、path、timestamp、
optional provenance は asset identity や replay input に含めない。forged dimension、stride、length、
digest/identity、work bound は commit 前に拒否する。

この接続は既存の同期 ABI-v2 ingestion entrypoint を Core owner thread で実行する。UI thread が
queue へ投入する前に lifetime を失う pointer を work item へ保持してはならず、`CoreHost` adapter は
issue-time session/generation と入力値を所有してから呼び出す。ABI v3 では variable payload を
同期 bounded call で generation-tagged asset/sample ID に変換し、closed typed queue には
`CommandContext`、base revision、target、opcode/schema、固定値、ID だけを格納する。caller buffer は
queue item に入らない。また M8 までは
production `.inkpod` v2 が `GENS`/`ASST` を保存・復元しないため、この runtime ingestion 契約を
successor container の end-to-end persistence とみなさない。特に raster-open 由来の asset-backed
Genesis は M8 まで normal save、autosave/recovery、Batch `.inkpod` output ができない。Core は
`INVALID_STATE` を file/directory 作成より前に返し、既存出力、document/revision/dirty、normal path、
savepoint を変更しない。Windows adapter も current path と recent-file list を変更しない。一般画像への
flat export はこの制約を受けない。

## 編集状態と排他

Core が持つ transient editing state は、committed document と分離される。

| 状態                | 開始／更新中の committed revision・dirty・Undo | snapshot                                | 完了                                                          |
| ------------------- | ---------------------------------------------- | --------------------------------------- | ------------------------------------------------------------- |
| live stroke         | begin/append では不変                          | stroke preview を観測できる             | end が実変更を高々 1 Undo 単位で commit。cancel は完全復元    |
| filter/dust preview | begin/update では不変                          | transient preview revision を観測できる | apply が 1 Undo 単位で commit。cancel は original base を保持 |
| floating paste      | begin/transform では不変                       | floating preview を観測できる           | commit が高々 1 Undo 単位。cancel は base を保持              |

1 Core に各 state は高々 1 個であり、live stroke と filter/dust preview は同時に存在できない。
競合する文書編集、履歴移動、保存、open、layer/plane 操作、別 preview 開始は
`INKPOD_STATUS_INVALID_STATE` になる。immutable snapshot 構築は transient state 中も許される。

live stroke の append が失敗した場合、部分的な preview を後から end で commit してはならない。
Core は session を無効化するため、frontend は stroke を打ち切り、必要なら cancel を行って次の begin へ進む。

## revision、dirty、Undo の読み方

`document_revision` は committed document の識別に使う。view-only 状態は `view_revision`、filter/stroke
preview の描画更新は snapshot 側の transient revision で区別する。

| 操作の種類                                                    | document revision    | dirty                             | Undo                              |
| ------------------------------------------------------------- | -------------------- | --------------------------------- | --------------------------------- |
| query、snapshot accessor、task、shortcut、view-only 操作      | 不変                 | 不変                              | 不変                              |
| EditorState query または semantic no-op                      | 不変                 | 不変                              | 不変                              |
| EditorState の semantic update                               | 不変                 | editor savepoint との差だけ変化   | 不変                              |
| stroke begin/append、preview begin/update、floating transform | 不変                 | 不変                              | 不変                              |
| stroke end、preview apply、floating commit                    | 実変更時に 1 回進む  | dirty                             | 高々 1 単位                       |
| 直接の文書編集                                                | 実変更時に 1 回進む  | dirty                             | 原則 1 単位                       |
| Undo/Redo/history jump                                        | 結果状態へ進む       | savepoint との位置で再計算        | cursor を移動し item は増やさない |
| 現行 v2 の通常保存                                            | 不変                 | document だけ clean。editor dirty は保持 | 不変                         |
| autosave                                                      | 不変                 | 不変                              | 不変                              |
| new/open/import/recovery                                      | 新しい文書情報が正本 | 戻り情報が正本                    | 旧 history を引き継がない         |

no-op の厳密な出力や revision は各関数の Doxygen 契約に従う。frontend は file timestamp ではなく、
Core が返す document flags と savepoint に基づいて未保存状態を表示する。

## 典型的な起動から終了まで

### 1. Core を作る

Core engine thread を開始し、その thread 上で ABI version を確認して Core を作る。

```cpp
InkpodCoreConfig config{};
config.struct_size = sizeof(config);
config.abi_version = INKPOD_ABI_VERSION;
config.feature_flags = INKPOD_FEATURE_NONE;

InkpodCore* core = nullptr;
Check(inkpod_core_create(&config, &core));
```

各 `core` owner 変数は CoreHost の session entry が一意に保持する。raw Core pointer を UI message の
`WPARAM`/`LPARAM` に積まず、UI 通知は session ID/generation を含む bounded queue の value token で
取り出す。

### 2. new または open

新規作成では platform adapter が非 0 の 128-bit UUID、寸法、DPI を用意する。open では Windows file
dialog が得た path を UTF-8 byte span に変換し、Core engine thread へコピーして渡す。いずれも成功時の
`InkpodDocumentInfo` を UI state の初期値とする。

open/decode が失敗した場合、現在の文書は保持される。frontend は失敗前に tab や Renderer state を
破棄せず、成功通知を受けてから切り替える。

### 3. stroke を streaming する

UI/Input thread は pointer history を client device pixel 座標で正規化し、bounded queue へ batch で入れる。
Core engine thread は style と最初の sample で begin し、後続 sample を append する。sample ごとに FFI を
往復したり snapshot を作ったりしない。

```text
pointer down
  → stroke begin（style + initial samples）
  → stroke append（0回以上の sample batch）
  → stroke end
pointer cancel / capture lost
  → stroke cancel
```

描画中の見た目が必要なら、frame cadence に合わせて begin/append 後に snapshot を作る。end は pointer up
と同じ順序で必ず Core queue に入り、成功した stroke 全体を 1 Undo 単位にする。

### 4. snapshot の所有権を Renderer へ移す

Core engine thread は owner 変数を NULL から構築し、snapshot sink へ raw owner をちょうど 1 回渡す。
sink は enqueue 成否にかかわらず release 責任を引き受ける。

```cpp
InkpodSnapshot* next = nullptr;
Check(inkpod_core_build_snapshot(core, &options, &next));

snapshot_sink.Submit(next); // この呼び出しで所有権を移動する
next = nullptr;             // Core engine 側は以後参照・release しない
```

`Submit` が queue full で snapshot を採用しない場合も、sink 内で直ちに release する。呼び出し元へ所有権を
戻す設計と混在させない。snapshot pointer を `PostMessage` の値引数として送らず、所有権を明示した C++ queue
を使う。

### 5. Renderer が読み、release する

Renderer thread は snapshot から raster view、transform、overlay、vector view を取得し、親 snapshot の
生存中だけ借用する。古い pending snapshot を最新のものへ置き換える場合、置換された owner をその場で
release する。描画完了、device reset、window shutdown の各経路でも owner を一度だけ release する。

### 6. shutdown する

推奨順序は次のとおりである。

```text
UI/Input の新規投入を停止
  → active stroke/preview/floating を end/apply または cancel
  → Core work queue を drain
  → snapshot sink を閉じる
  → Renderer の pending/current snapshot をすべて release
  → task/report/preview/graph/clipboard/byte buffer を release
  → Core engine thread 上で core destroy
  → Core engine / Renderer thread を join
```

`inkpod_core_destroy` は live transient state を commit せず破棄し、owner 変数を NULL にする。snapshot は Core
より長く生存できるが、通常 shutdown では先に Renderer owner を解放して leak 検出を単純にする。

## task、進捗、cancel

長時間処理では、task handle を先に作り、その handle を Core operation が戻るまで所有する。UI thread は
query で進捗を読み、ユーザー操作で cancel を要求できる。

```text
Core engine: task create → Core operation(task) ─────────→ return → task release
UI thread:                         query / query / cancel
```

cancel は要求であり、即時終了の保証ではない。Core が cancellation を poll すると staged result を破棄し、
`INKPOD_STATUS_CANCELLED` を返す。file 出力を伴う処理は完成済み temporary file だけを置換対象とし、cancel や
failure で部分出力を commit しない。

batch execution だけは cancel/失敗時にも report owner を返し得る。戻り status を確認した後も
`out_report != NULL` なら内容を読み、必ず release する。

## 保存、autosave、recovery

通常保存は同一 directory の temporary file を完成・flush・close してから置換する。成功時だけ normal path と
document savepoint が進む。現行 production `.inkpod` v2 は canonical EDIT frame を保存・復元しないため、
M3 の editor savepoint は進めず、editor dirty があれば session dirty は解消しない。失敗時に元 file を
truncate せず、document/editor のどちらの savepoint も変更しない。M8 の atomic format cutover までは
`session_dirty = document_dirty || editor_dirty` がこの差分を利用者へ正しく公開する。

autosave、export は出力を atomic に書いても normal path、document/editor savepoint、dirty を変えない。
recovery open は文書と built-in defaults からコピーした EditorState を dirty・recovered・pathless として
開く。通常 v2 open は built-in defaults の clean EditorState を作るが、保存前の EditorState を復元した
ものとはみなさない。以前の通常 file を上書きするには、ユーザーが明示した path で改めて通常保存する
必要がある。

active stroke/preview/floating 中は保存や open を実行せず、Core queue 上で完了または cancel 後に行う。

## diagnostic の取得

error text は thread-local UTF-8 である。失敗した API と同じ thread で、まず trailing NUL を含む必要 byte 数を
取得し、次に caller buffer へコピーする。別 thread へ status と text を通知する場合は、Core engine thread で
text を `std::string` へコピーしてから queue へ渡す。

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

診断文にはユーザー path や画像内容を無制限に記録せず、UI 表示や log 側でも同じ方針を守る。

## 実装時の確認項目

- Core の create/全操作/destroy が同じ Core engine thread に固定されている。
- すべての構造体で `struct_size`、reserved、feature flags、record stride を初期化している。
- owned 出力変数を NULL で開始し、所有権移動後に元変数を NULL にしている。
- release 後に borrowed span や copied alias を使っていない。
- stroke/preview/floating の state machine と失敗時 cancel 経路がある。
- snapshot sink が enqueue 成否の両方で release 責任を一意に持つ。
- task を Core call 終了前に release していない。
- 戻り status が失敗でも返り得る batch report を解放している。
- dirty を file timestamp から推測せず、Core の document flags を使っている。
- C11 と C++20 の両方でヘッダーを include し、Rust 宣言との drift test を通している。
