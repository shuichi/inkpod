# InkScript と現行 Batch の接続契約

本書は [INKSCRIPT M1](../INKSCRIPT.md#17-実装マイルストーン) で確定した後続実装契約である。
**D1–D4の推奨案は2026-09-06に利用者承認済み**。D1 は M3 の catalog v8／file v2 の実装契約。
D2／D3 の Core-only envelope・実行経路は M4 の file v3／catalog v8 契約とし、
[SPEC](../SPEC.md)、[言語仕様](../INKSCRIPT.md)、[language v3](../schemas/inkscript/language-v3.json)へ反映する。
D4 の製品接続と M5 以降の ABI／Windows 統合は別工程であり、cutover gate を維持する。
以後の版番号も予約せず、実装時の exact-current から決める。
実行結果・既知差分の記録先は [compatibility](compatibility.md) とする。

## 承認された判断

| ID | 推奨案 | 別案と影響 |
| --- | --- | --- |
| D1 処理列と target | `apply_batch_operations` 一 command 内の順序付き四 variant と、command 専用の型付き target selector。全 enabled 処理・展開 target を既存 executor へ一回渡す | 四 command に分割すると複数 Commit になり現行 parity を満たさない。新たな transaction block は一般言語・export の変更が大きく、既存 canonical primitive の再利用にも不要 |
| D2 入力の意味と上限 | `inputs` に明示 `profile = batch | canonical` を追加。省略は `canonical`。Batch pane が作る source は `batch`。既存の入力順・range・snapshot の違いを保持し、同じ I/O manager 上で policy を切り替える | 全入力を一律に現行 InkScript へ合わせると Batch の順序・番号・履歴・Stop 後の結果が変わる。一律 Batch 化は既存一般 script の契約を変える。output kind や command の内容から profile を推測しない |
| D3 出力と preview | 既存 naming policy を保ち、`folder`／`active_document`／`new_tabs` の closed output variant を追加。Batch の画像 preview と publication 契約を維持 | 一律に既存 `duplicate` を使う案は template・新 identity・active 出力を表せない。複数入力や file 入力を active 出力へ集約する案は新しい合成・Undo 契約が必要なため採らない |
| D4 編集・実行導線 | 同じ Batch 専用 tab・同じ source owner 内に「工程」「ソース」を置く。四処理で完全に表せる source だけ工程編集可能。一般 command／`current_sequence`／`each_run` はソース側で作成・編集する。実行は既存三 button を共用 | 一般 script を四処理へ丸める案は情報を失う。独立した editor／Run current／dry-run button の追加は導線・所有権が増える。一般 script を Core-only に残す案は製品公開範囲を狭める |

D1–D4 の承認は、下記 field、例、失敗条件、上限、変更先を後続実装の契約とする承認である。
**性能 workload／counter／harness／envelope の変更、M15 の production cutover、旧形式削除の承認は含まない。**
公開は M6–M14 の private 検証と M15 の別途明示承認に従う。

## 既存契約との照合

| 対象 | 根拠と維持する観測結果 |
| --- | --- |
| 全処理列 | [batch/operations.rs](../rust/inkpod-core/src/batch/operations.rs) の `Core::apply_batch_operations` → `ApplyBatchOperations`。各処理を別々に commit しない。[Batch v5 public tests](../rust/inkpod-core/tests/contracts/batch_v5.rs) が全処理列、native depth、Undo/Redo、replay を検査 |
| target | [batch/validation.rs](../rust/inkpod-core/src/batch/validation.rs)、[batch/operations.rs](../rust/inkpod-core/src/batch/operations.rs)。色置換だけが全 matching plane へ展開し、他三処理は最初の対象一つ。同一 selector の重複は invalid、異なる selector が同一 plane に重なる場合は一回だけ処理 |
| export | [script/export.rs](../rust/inkpod-core/src/script/export.rs) は展開済み `ApplyBatchOperations` を一 command へ export。[公開 export 契約](../rust/inkpod-core/tests/inkscript_batch_export.rs) と [registry tests](../rust/inkpod-core/tests/inkscript_registry.rs) が catalog v8 の75 commandを検証する |
| 入力・出力・preview | [batch/execute.rs](../rust/inkpod-core/src/batch/execute.rs)、[batch/model.rs](../rust/inkpod-core/src/batch/model.rs)、[Batch public tests](../rust/inkpod-core/tests/contracts/batch.rs)、[FFI Batch tests](../rust/inkpod-ffi/tests/unit/batch.rs)。active 一件制限、copy-before-processing、staged ownership を再利用 |
| 一般 script | [envelope.rs](../rust/inkpod-format/src/inkscript/envelope.rs)、[plan.rs](../rust/inkpod-core/src/script/plan.rs)、[run.rs](../rust/inkpod-core/src/script/run.rs)。現在は native 入出力、全体自然順、完全な session snapshot、authority-bound plan と item 単位 install |
| Windows | [batch_controller.cpp](../apps/windows/ui/batch_controller.cpp)、[batch_dialog.cpp](../apps/windows/ui/dialogs/batch_dialog.cpp)、[private engine](../apps/windows/app/inkscript_engine_route.cpp)、[authority](../apps/windows/app/inkscript_file_authority.cpp)。製品 owner と private owner の切替は M15 まで保留 |

確認できた食い違いを、仕様優先という理由だけで成功済み parity に含めない。

- **MainLine の fixed ID と `missing = skip`**：M1で候補からの除外が欠落扱いになる既存不具合を確認した。
  M3は公開反例を先に固定し、raw selector lowering前に拒否する。UUID／owner違反も新binderで
  欠落と区別する。既存canonical payload／replay結果は変えず、この是正はM9で旧結果とのparityに数えない。
- **既存 fill protection と raster 出力**：現在の graph 検査は enabled `Masking` の有無を判定し、
  入力に既にある mask を一律拒否する契約ではない。D3 はこの既存条件を保持する。
  「結果に mask があればすべて拒否」への強化を暗黙に加えない。
- **一般 script と Batch の入力**：以下の D2 の差はテスト期待値の誤りではなく異なる公開契約。
  同じ入力列を与えただけで parity とせず、比較時に `profile = batch` を固定する。
- **strict selector の強化**：旧 Batch の raw ID filter は、指定 layer と plane の owner が異なる場合も
  欠落＋skip になり得る。D1 の UUID 保護と owner 不一致拒否は、新 InkScript の明示的な強化であり、
  旧仕様違反の修正とは扱わない。M3/M9 はこの受理範囲の差を独立した negative case として記録する。
- **新規 tab の反復 identity**：既存 `stage_new_tab_result` は source UUID・item ordinal・revision から
  identity を導出し、public test は source と異なることを確認している。同一 job の反復時の一意性は
  検証されていない。D3 は既存 session／同時に公開する結果と衝突しない新 identity を要求し、
  M4 で反復・同一source重複を test してから保証する。現時点で成立済みとはしない。

## D1：一 command の表現と実行

command 名は `apply_batch_operations`、引数は必須の
`operations: list<batch_operation>` 一つ、result は空とする。文書 ID を生成する command ではない。
`editor_group` は表示 metadata のままであり transaction 境界に使わない。

次表の順を canonical field order とする。全 field は記載した variant 内で必須とし、
他 variant の field、未知 field、重複 field を拒否する。`enabled` は bool、
`pixel_value` は既存 literal を使用し、RGBA8/16 の depth を暗黙変換しない。

| `batch_operation.kind` | field（`kind` に続く順） | 条件 |
| --- | --- | --- |
| `color_replace` | `enabled`, `targets: list<batch_target>`, `pairs: list<batch_color_pair>` | targets 1–64、pairs 1–4,096。pair は `enabled`, `old`, `new`。enabled old 色の重複を拒否。全 pair 無効もtarget/editable/resource/cancel検査を通し、その成功後はno-op |
| `move_to_color_plane` | `enabled`, `target: batch_target`, `colors: list<pixel_value>` | colors 1–4,096、重複不可。同 layer の Color destination と format/dimensions が一致し、source/destination を一緒に変更 |
| `masking` | `enabled`, `target: batch_target`, `colors: list<pixel_value>` | colors 1–4,096、重複不可。selection と別の sparse fill protection を置換し、source pixel は不変 |
| `erase` | `enabled`, `target: batch_target`, `colors: list<pixel_value>` | colors 1–4,096、重複不可。該当する native 値のみ empty にする |

`batch_target` は command 専用の **型付き selector** であり、任意整数を既存 plane invocation の
ID 引数として渡す抜け道にしない。新 selector とその dependency/export rule を registry に記載する。
汎用 `select plane` の `missing = skip_dependents` を読み替えない。

| `batch_target.kind` | 必須 field | 解決規則 |
| --- | --- | --- |
| `role` | `plane_kind: color | raster`, `missing: error | skip` | 色置換では初期 document tree 順の全該当 plane。他三処理は同順の先頭一つ。layer-kind filter はない |
| `strict` | `source_document_uuid: uuid`, `persistent_layer_id: nullable<u64>`, `persistent_plane_id: nullable<u64>`, `plane_kind: nullable<color | raster>`, `missing: error | skip` | 少なくとも一つの persistent ID と、plane ID または role が必要。ID は非zero。UUID は必ず一致。layer ID と plane ID を両方指定した場合は owner も一致。layer ID＋role は当該 layer の role 解決を表す |
| `references` | `layer: nullable<layer_ref>`, `plane: plane_ref` | 同じ入力の先行 result または既存 binding を参照。layer があれば owner 一致を要求。欠落・UUID／owner／MainLine 違反は error。producer skip は既存 dependency 規則に従う |

role／strict は initial input へ束縛し、途中状態から target を探し直さない。
references の先行 result は producer 成功時に確定し、同じ型・owner 検査を行う。
strict の UUID 不一致、zero ID、owner 不一致、MainLine 指定／解決は `skip` でも invalid。
同一 source 内で対象が存在しない場合だけ `missing = skip` がその selector の対象を省く。
他 selector／他 operation は続行し、command 全体を skip しない。
hidden／non-editable／format 不一致は skip にしない。

`references` の runtime dependency skip は enabled operation の参照だけから伝播する。
disabled operation 内の参照だけが missing producer に依存しても、enabled な兄弟 operation を skip しない。
全 operation の名前・型・上限と fragment dependency closure は検査し、静的に disabled な producer を
参照する既存 compile error は維持する。この nested dependency 規則を registry/compiler/fragment の
公開 test に含める。enabled references の producer が skip された場合は既存規則どおり command 全体を skip する。

色置換は selector 記述順、各 selector の document tree 順に展開し、最初に出た plane ID を採用する。
同じ操作内の重なりは除去するが、異なる operation 間の同じ plane は除去しない。
他三処理を全 layer 処理へ拡張しない。disabled operation も構造・型・値の検査と source 保持の対象とし、
実行 target の解決・pixel work は行わない。

source の operations は 1–1,024。全 operation 無効は保存可能な source draft だが実行 plan を返さない。
展開後の canonical operations も 1,024 以下とし、超過は実行前に拒否する。
全対象 skip または pixel/mask の実変更なしは no-op とし Commit、ID、revision、dirty を進めない。
全 enabled 処理を通して成功した場合だけ、一つの `ApplyBatchOperations/canonical-v3` を commit する。
失敗・cancel・stale・overflow では先行 operation の途中 state も公開しない。

resource bound は [既存 Batch 上限](../rust/inkpod-core/src/batch/mod.rs) と
[canonical executor](../rust/inkpod-core/src/batch/operations.rs) を継承する。
展開済み各 operation の `width × height` の checked sum は **67,108,864 以下**。
static compile は source/list/色数、bind は展開数・dimensions・この pixel sum、runtime は残 budget と
cancel を検査する。bindでは既知initial targetと保守的上限を検査し、先行producer／resizeの実行後、
Batch invocation直前にも実際のresult ID・dimensions・展開数・総処理量を全列preflightする。
この再検査はBatch内の最初のmutationより前であり、role/strictの固定IDをselectorで探し直さない。
一 command に包んで上限を一 operation 相当へ減算しない。
script の aggregate work／ID capacity 制限も併用し、小さい上限を適用する。
既存 benchmark の work counter/formula は変更しない。新 entry の bound 式は既存の
checked sum／list length 表現で registry と compiler を一致させ、M3 の境界 test で固定する。

export は journal の **展開済み順序**をそのまま一 command にする。元 UI の selector grouping、
disabled operation、comment は journal にないため復元できると表示しない。
対象が選択 Commit 列の先行 producer に由来すれば typed result reference を優先し、
その他は UUID 付き strict selector と exact parent の state／ID allocation assertions を出す。
exact-source は canonical invocation 列、Commit 境界、state/pixel digest、ID、mask を比較する。
明示 rebind では全外部 strict selector を置換して再検証し、保証は rebound に変わる。
source UUID／raw ID／state digest 一致まで約束しない。非連続・非線形・非 Commit、
dependency closure 外、resource 超過、cancel は fragment を部分公開しない。

## D2：input profile と source 例

`inputs` は `profile` 一個まで（省略値 `canonical`）と既存の input declaration を持つ。
両 profile とも file／非再帰 folder／current_document を受け入れ、対応 codec は
`.inkpod`、PNG、TIFF、TGA、BMP。新拡張の codec は共有 decoder を使用する。
`current_sequence` は `canonical` だけで受け入れ、Batch 工程 UI へ追加しない。

| 観測点 | `batch` | `canonical`（既存 native 挙動を維持） |
| --- | --- | --- |
| 並び | input 宣言順。folder 内だけ既存 `natural_cmp` 順。同値は生UTF-8 filename、canonical path key順で確定 | 全 input 展開後、INKSCRIPT 7.3 の label／path／UUID による全体自然順 |
| 重複 | 同一 file identity／alias は error。active の重複宣言は保持。native UUID だけでは同一 file としない | UUID 重複または path alias は error |
| range | `cells = all` または `range(first,last)`。0 はその側の境界なし。両端が非zeroかつfirst>lastだけerror（`range(5,0)`は有効）。file stem の最後の数字 run で比較し、数字なし／overflow は対象に残す。activeに指定したrangeは無視する | 既存の非zero、閉じた inclusive range。表示番号の解決は既存 authority adapter 契約を保持 |
| pathless active の名前 | `active-document.inkpod`、stem は `active-document` | `current-cell.inkpod`、既存 naming policy 用 source stem はなし |
| open active snapshot | active→active は完全な staged current Core。その他の全出力（folder/new_tabs/duplicate/new_save）と画像preview用は既存Batchのdocument/assetsからのmaterialize規則（履歴/editorは再構成） | Genesis、journal/branch、ID high-watermark、document/editor、両 savepoint を含む完全な immutable snapshot |
| open文書が所有するfile/folder入力 | pathのdisk内容を読む。未保存のlive編集を取り込まない | open sessionのimmutable snapshotをcaptureし、dirty内容をbacking fileに置き換えない |

この profile は入力解決と snapshot policy だけを指定し、任意の shell／再帰／環境変数を追加しない。
型付き command の executor や一般74 command の集合を変更しない。
通常の四処理 pane は `batch` を明示して生成し、`canonical` へ暗黙に変換しない。
script 相対 path は保存先の親、未保存なら明示 base authority を必要とし、cwd から補完しない。

旧 Batch の `natural_cmp` 同値（大文字小文字だけ異なるfilename等）は列挙順に依存していた。
D2 はその未固定部分に上表のtie-breakを追加する承認済み契約で、通常の非同値順序は変えない。
M4では列挙順を反転したfixtureを追加し、この差を旧結果とのparity成功に数えない。

新しいraster入力はfile identity/fingerprintに結び付けたingestion snapshotを作る。
`batch`は既存`working_core`のencoded bytes由来UUIDを保持し、同じbytesの別fileもfile identityが異なれば
重複扱いしない。`canonical`だけはRustが発行する非zero ingestion identityをplan内で固定する。
両profileでrasterのfile aliasは拒否し、canonicalのnative UUID重複検査とは区別する。
open raster sessionは上表のprofile規則を使う。canonicalのraster display numberも既存Windows native adapterと
同じ末尾数字runから求め、数字なし／0／overflowは1とする。これをOS非依存の契約・testにする。
canonicalの別job間import identity一致は保証せず、同じimmutable plan内の再実行とcanonical pixel結果の
決定性を検証する。batchのnative出力は既存UUID/digestとのparityを検査し、new_tabs公開時の新identity発行と区別する。

次は **file v3／catalog v8 の source template**。四処理と envelope を表す完全fileであり、
Core-only compiler／plan／runnerで扱う。製品のfile filter／pane接続を意味しない。

```text
inkscript 3;
requires { procedure_catalog = 8; replay_epoch = 29; }
inputs {
    profile = batch;
    file "cells/A001.inkpod";
}
program {
    step "四処理を一度に適用" {
        enabled = true;
        invoke apply_batch_operations {
            operations = [
                { kind = color_replace; enabled = true;
                  targets = [
                      { kind = role; plane_kind = raster; missing = error; },
                      { kind = role; plane_kind = color; missing = error; }
                  ];
                  pairs = [{ enabled = true; old = rgba8(255,0,0,255); new = rgba8(0,255,0,255); }]; },
                { kind = move_to_color_plane; enabled = true;
                  target = { kind = role; plane_kind = raster; missing = error; };
                  colors = [rgba8(0,255,0,255)]; },
                { kind = masking; enabled = true;
                  target = { kind = role; plane_kind = color; missing = error; };
                  colors = [rgba8(0,255,0,255)]; },
                { kind = erase; enabled = true;
                  target = { kind = role; plane_kind = raster; missing = error; };
                  colors = [rgba8(0,0,255,255)]; }
            ];
        };
    }
}
output { policy = folder; format = inkpod; folder = "out"; naming_template = "{stem}_painted"; }
execution { failure = continue; wait_ms = 0; preview_before_save = false; }
```

M3 の独立期待値 fixture は一 layer・幅3×高さ1・RGBA8、Raster が赤／青／黄、Color が透明、
空の fill protection とする。この例の結果は Raster が透明／透明／黄、Color が緑／透明／透明、
mask が 255／0／0、MainLine 不変。一 Commit・一 Undo で全体が戻る。
同じ色の16-bit版と、誤った depth を混在させた invalid 例も固定する。

既存 InkScript の source 128 MiB、全 input 16,384、aggregate input bytes／output＋temporary 64 GiB、
aggregate wait 3,600,000 ms 等の上限を両 profile へ適用し、共有 manager／decoder の小さい上限も守る。
**aggregate wait／bytes は旧 Batch の per-item 制限だけでは許された job を拒否する場合がある。**
D2 の明示判断対象とし、既存 Batch 側の上限や性能 gate を緩和・変更して parity を装わない。
同上限内の workload で parity を比較し、上限超過は新 script の明示的な resource error として別検証する。

## D3：output、preview、失敗と所有権

既存 `duplicate`／`new_save`／`explicit_overwrite` の native naming・identity・authority 契約は維持する。
新 variant は次の closed field 集合とし、別 variant の field を許可しない。

`duplicate`／`new_save` は新raster入力も受け入れるが、出力は従来どおりnative `.inkpod`だけ。
`explicit_overwrite` はclosed native入力自身だけに限定する。`file "a.png"` と
`output { policy = explicit_overwrite; format = inkpod; }` は実行前に拒否し、rasterをnative bytesで置き換えない。

| `output.policy` | 必須 field | 観測結果 |
| --- | --- | --- |
| `folder` | `format: inkpod | png | tiff | tga | bmp`, `folder: string`, `naming_template: string` | 非空 folder、template は1–1,024 UTF-8 bytes。`{stem}`／`{index:N}`（N=1–12）だけ。index は既存 Batch 同様1始まり。extension は format が決める。既存 destination、input alias、item 間衝突を拒否し、自動 rename／上書きしない |
| `active_document` | 追加 field なし | `current_document` 一宣言・解決一件で、enabled mutation step が単一の `apply_batch_operations` である場合だけ（assertions は許可）。file／folder／sequence 入力や一般複数stepとの組合せは拒否。発行時 session/view/generation に結果全体を一 Undo として適用し、path authority と両 savepoint は保持 |
| `new_tabs` | 追加 field なし | 結果ごとに Rust が新 identity を割当て、既存 Batch の新規文書化と同様、結果document/assetsからhistory/editorを再構成し、pathless/dirty sessionを staged publication。sourceの履歴を持ち越すlogical forkではない。最大必要件数を job 開始前に capacity preflight |

folder path は32,768 UTF-8 bytes以下、template は absolute／separator／dot／`..`／拡張子 token を拒否する。
`canonical` の pathless input に新 `folder` を使う場合、`{stem}` は解決不能として error にし、
`{index:4}` 等の stem 不要 template を指定できる。既存 native naming の意味は変えない。
`format` は encoding の指定であり、Color/Raster の exact-depth 置換契約と同一ではない。
Batch の raster export は既存 `export_common_raster` の合成経路を再利用し、RGBA16 の格納精度保持を
新たに保証しない。native は現行 native 保存。enabled masking を含む処理列と raster folder の組合せは拒否する。
一般 script の複数 command 内も `apply_batch_operations` の outer step と masking operation が
ともに enabled の場合に同じ拒否を適用する。disabled outer stepだけを理由にraster出力を拒否しない。

例として前節の output を `format = png` にすると masking を失うため validation error。
masking を削除した source なら `folder` PNG が可能。
`inputs { profile = batch; current_document; }` と `output { policy = active_document; }` は一 Undo の適用。
`output { policy = new_tabs; }` は新 tab、同じ file 入力で `active_document` は実行前拒否となる。
一般 command の二つの enabled step と `active_document` も実行前に拒否する。
複数 canonical Commit を一 Undo へ合成する新しい history semantics は導入しない。
一般 script は folder／new_tabs／既存 native naming policy で実行できる。
new_tabs の前段では profile に応じた staged実行/replayを検証し、公開結果では新規Genesisのstate、
新identity、path/dirty/savepointsとsave/reopenを別に検証する。両者の履歴が等しいとは主張しない。

| 結果型／操作 | 副作用・publication |
| --- | --- |
| PathIntentPreview／ExecutionPreview | authority／入力順／衝突／必要確認の immutable 計画。画像ではない。planning 中は書かない |
| staged dry-run | 隔離 Core の結果と report。実 destination／live source を変更せず、UI の独立 command は追加しない |
| 画像 preview | planが選んだ全file inputのcopy／session snapshotのmaterializeが完了してから処理。canonicalのopen dirty fileもsnapshotをmaterializeし、diskへ読み替えない。folder指定なら同じcodec、他はtemporary nativeへ保存・再読込。実outputへは書かない |
| run | item 単位で staged execute → encode → authority 再検証 → atomic install または owner-thread publication。成功済み先行 item は後続失敗／cancel で rollback しない |

画像 preview は専用 temporary directory 4 GiB以下、thumbnail 長辺160、padding8を基準に
16,777,216 pixels以下の RGBA8 contact sheet を作る。input 順、failure 赤系、Stop 後の未処理灰色、
透明 checkerboard を維持する。cleanup 完了後に一つの clean/pathless 表示専用 staged Core を返す。
cancel／stale／cleanup failure では tab を公開しない。preview tab は元の issue-time context を保持し、
次 job が元 target を再利用する。stale 時に preview や別 active 文書へ fallback しない。
file input は copy 時に fingerprint を照合し、全copy完了後はその隔離bytesから実行する。
その後の元fileの変更を最新bytesへ読み替えない。origin session／authority はcleanup後の公開直前にも照合する。

| 失敗例 | 保持するもの／report |
| --- | --- |
| target 欠落(error)、hidden、non-editable、depth不一致、展開上限 | 当該 item の全処理を不公開。continue は後続へ、stop は残件を未処理として報告 |
| input alias、output collision、active 組合せ不正、capacity不足 | job preflightで拒否。source/default/live文書/既存destinationは不変 |
| each-run Cancel／無効値、authority拒否、source編集後の旧confirmation | jobを作らない、または失効したplanを拒否。stored defaultへ書き戻さない |
| copy／decode／encode／atomic install失敗、install前cancel | 失敗itemの部分outputなし。成功済みitemを保持。作成directory等の残り得る副作用は既存report契約で示す |
| preview cleanup失敗、publication前stale、close/shutdown | staged結果をreleaseし、別sessionへの通知・適用をしない。preview tab未公開 |

source controller は draft と source generation を所有し、Core engine が compile、snapshot、plan、task、
report、staged result を所有する。C ABI は opaque handle／bounded bulk／二段階 copy／take・releaseを使う。
UI callback／queue の結果は pointer-free DTO。Rust handle の parent lifetime、owner/release thread、
generation、cancel／close を [FFI](ffi.md) と header へ M5 で同時記載する。
renderer は immutable snapshot だけを所有する。

I/O は既存共有 Rust manager／private platform backend へ統合する。Windows は authority UI と
engine queue を担当し、codec・selector・画像処理・第二の I/O engine を持たない。
同volume atomic install、最終identity/fingerprint検証、application内lock、temporary guardを共有する。
overwriteではsourceの通常のwrite／truncateを排除するguardを最終fingerprint照合からreplaceまで保持する。
WindowsではREAD／DELETE共有を許可し、WRITE共有を許可しない。destination pathのidentityもreplace直前に
照合し、観測した変更はstaleとして拒否する。最終検査後の外部rename／delete／別objectへの置換は完全には
検出せず、厳密なno-lost-updateは保証しない。公開後の無条件rollbackや自動上書きretryは行わない。
詳細は[InkScript 7.9](../INKSCRIPT.md#79-output)と[上書き保存方式](inkscript-overwrite-design.md)に従う。
書込み排除またはatomic installを提供できないfilesystemは明示拒否し、TxFには依存しない。

## D4：source と四処理 pane

- source/CST を唯一の正本とし、「工程」「ソース」は同じ controller の表示。
  工程編集を許す条件は `profile=batch`、対応 Input/Output、単一の enabled outer step と
  `apply_batch_operations`、四処理内の literal parameter に完全に射影できること。
  assertions、一般 step、each-run／式参照等がある source は内容を削除せず、工程欄を read-only summary としてソースへ案内する。
- 工程欄は固定 Input/Output、四種類のみの追加、順序・enable・複製・削除、target role、
  loaded fixed ID、native-depth pair を保持。source 局所編集は comment、BOM/CRLF、無関係範囲を保持する。
  ソース表示への切替や項目選択で文書・source を変更しない。
- set dropdown は一 file 一 set。M15 で `.inkscript` 保存に切り替える。複数 set を一 source に
  詰める syntax、`.inkbatch` importer／migration は作らない。set名の安全なcomponent規則を再利用する。
- ソース保存は文法エラー／全無効を含む draft も正確に保存可能とする。診断と「実行不可」を表示し、
  run/preview planを返さない。これは現行 `.inkbatch` の検証済みgraph保存からの明示的な変更点。
  save失敗ではsourceを置換せずdirtyを解除しない。source Undo と document Undo は別に保持する。
- 一般74 command はソース側で存続。`current_sequence` は同側の `canonical` profile、
  `each_run` は既存 typed parameter で作成する。プレビュー／全実行時に該当値を transient copy で
  明示解決し、取消・無効値ではjobを作らない。defaultを暗黙採用／上書きしない。
- 最下段は「プレビュー」「全実行」「中止」の三つ。Run current／独立dry-run は製品へ追加しない。
  authority確認と `preview_before_save` の計画確認は画像 preview と区別する。
  共通status barと localized report（先頭8件＋残件数、copy/scroll）を維持する。
- Batch 専用 tab、他pane混在禁止、keyboard、狭幅scroll/wrap、geometry-only resize、日英、
  DPI/high contrast/accessibility は M7–M9 の private 可視経路、M18 の製品受入で検証する。

## 変更 owner と version impact

M3 の D1 実装は catalog／owner **8**、75 command を使用する。file **2**、replay epoch **29**、
native top-level **34**、C ABI **34**、registry schema **2**、`.inkbatch` **5**／operation **4** は維持する。
既存 record/list grammar と既存 registry の field constraint で表し、canonical payload/schema/semantics と
native replay fingerprint を変更しない。MainLine の是正は raw selector lowering 前に限定する。

M4 の D2／D3 は file **3** を使用し、旧file／fragment v2を拒否する。catalog／owner **8** は
file版metadataだけを3へ更新し、75 command／signature／owner assignment／binding意味を保持する。
registry schema **2**、epoch **29**、native **34**、ABI **34**、Batch **5**／operation **4** は維持する。
生成referenceとdrift fingerprintを同時更新する。性能checksumの基準変更は別の明示承認対象である。

| 工程／owner | 変更先・共有契約 | 版更新と受入 |
| --- | --- | --- |
| M3 format／registry | `inkpod-format/src/inkscript/{types,schema,fragment,emit}.rs`、現行 language/catalog/owner registry と生成reference。closed operation／selector／dependency・canonical order | 新 command/selector/binding は catalog 更新。selector literal/variant表現などserialized syntaxの拡張は file 更新も必要。registry schema を拡張する場合も同時更新。新 entry は既存74件と tombstone/owner ID を保つ |
| M3 Core | `batch/operations.rs` の既存 executor、`script/{catalog,compile,bind,execute,export}.rs` と typed primitive adapter | 単なるcatalog公開でcanonical payload/schema/結果が不変ならnative/replay更新不要。MainLine是正やselector loweringで同じcanonical invocationの結果が変わる場合はepoch＋nativeを同時更新し旧版拒否。新たな別executorは作らない |
| M4 format/Core/I/O | `inkscript/envelope.rs`、`script/{plan,run,report}.rs`、共通Batch codec/preview、`file_io` と `inkpod-io` | input profile/output fieldはfile更新。selector意味の変更があればcatalogも更新。runtime-only snapshot/authority/publicationでserialized native schemaやreplayを変えなければnative/epochは維持 |
| M5 ABI/Windows | `inkpod-ffi/src/inkscript*.rs`、ABI records、`include/inkpod/core_ffi.h`、private engine/authority、共有I/O adapter、`docs/ffi.md`／`docs/architecture.md` | symbol/record/呼出契約が変わるならABI更新、header/export、C11/C++20、非current caller拒否を同時実施。製品pane/filterは未接続 |
| M6–M8 UI | private ScriptController、source lifecycle、四処理projection、preview/run/cancel | 既存言語/実行境界を再利用。UIだけの変更で永続版を上げず、sourceのserialized変更なら該当版を判定 |
| 各工程の仕様・証拠 | `SPEC.md`、`INKSCRIPT.md`、`docs/{file-format,ffi,architecture,inkscript-traceability,compatibility}.md` の該当箇所 | 実装に合わせ正本・旧版拒否test・example・生成物を同時更新。本書の未実装例を現行command referenceへ混ぜない |

M3→M4で別々のserialized変更が入れば各変更で版を更新し、計画段階で「一つ上の版を共用」と決めない。
版依存bytes/checksumの変化はM2の既知quick gateと混同しない。基準変更は全sample/counterを用意し、別途承認を得る。

## 後続で固定する公開契約と判断手順

| 工程 | 追加・維持する公開 test と観測値 |
| --- | --- |
| M3 | format program/types/fragment と Core `inkscript_public`／Batch public contracts。四処理単独・組合せと先行CreatePlane/Resize→Batchを direct/script/export→再実行で比較。前記独立期待pixel/mask、canonical列・一Commit・Undo/Redo、全namespace ID、dirty/revision、save/reopen、cache-free replay。missing/error/skip、全無効、全pair無効、disabled参照とmissing producer、UUID/owner違反とskip、duplicate/overlap、MainLine/hidden/non-editable、depth、cancel/stale/overflow/resource/allocation failure |
| M4 | Batch/InkScript public I/O tests。両profileの順・range・重複・pathless・snapshot差、各codec、template/collision、active一件・複数step拒否、newtab容量、反復job/重複sourceでの新identity/path/savepoints、mask出力拒否。preview全copy後実行、失敗placeholder、cleanup-before-publication、元target固定、continue/stop/cancelと成功済みitem保持 |
| M5 | FFI source/Batch/execution tests、C11/C++20 header/export、private Windows authority/engine tests。bounded copy/take/release、wrong-thread、stale generation、close/shutdown/queue saturation、save failure、path race、最終fingerprint検証とsource書込み排除、検査後のname race境界 |
| M7–M9 | lossless source↔工程UI↔save/reopen、pair extractionのUUID/generation・scalar/RGBA8/16・ambiguity・Cancel、日英/keyboard/layout。INKSCRIPT 16.4 のcanonical/state/composite/history/ID/mask/savepoint/report/work counter全比較 |

上表は **実装時の検証要件**であり、本 M1 で新機能の成功を主張するものではない。
文書のみの検証は [verification](verification.md) に従い、既存生成reference、registry/route、
関連公開契約、リンク・要件ID・差分を確認する。Windows build／可視操作／実機、Release性能は
文書だけの変更の証拠へ流用せず、実装工程で行う。

次の判断はD1–D4の推奨案として承認済みであり、再確認を要しない。

1. D1：四処理を一 Undo にする source 表現、色置換以外の先頭一対象、strict/rebind、MainLine拒否でよいか。
2. D2：Batch用profileを明示し、既存の入力順・snapshot規則を維持するか。同値順の確定、新raster入力identity、aggregate上限による拒否も受け入れるか。
3. D3：active出力はactive入力一件・一つの四処理commandに限定し、template／履歴を新規化する新tabと反復identity／画像preview／mask出力制限を上記どおりにするか。
4. D4：同じtabのソース編集から一般script・sequence・each-runを扱い、四処理へ変換不能なら工程欄をread-onlyにするか。invalid draftのsource保存を許可するか。

M1は必要な利用者判断を含め`[x]`。M2以降を一工程ずつ進める。
性能gateの期待値・harness・環境別envelopeの変更とM15のproduction cutoverは別の明示承認を必要とする。
