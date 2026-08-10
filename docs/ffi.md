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

Core は C++ コールバックを呼ばない。UI スレッドは Core やレンダラーの完了を同期的に待たず、
レンダラーは古い未描画スナップショットだけを置き換えてよい。ストロークの開始・終了・キャンセルと
入力サンプル自体は捨てない。

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
- ABI v9 で既知の構造体末尾まで読み書きできるサイズが必要である。
- `reserved` は 0 とし、未知の必須機能フラグは指定しない。
- レコード列では、各レコードの `struct_size` と `*_stride_bytes` の両方を設定する。
- 要素数、ストライド、アラインメント、列全体のバイト範囲が有効でなければならない。
- 要素数が 0 の任意指定の列に限り、データポインターを NULL にできる。各 API の例外はヘッダーを参照する。
- 入力、出力、不透明オブジェクトの記憶域を重ねない。

ABI バージョンは Core 作成前に比較できる。`INKPOD_ABI_VERSION` とライブラリの戻り値が異なる場合は、
Core を作らず互換性エラーとして扱う。

現行ライブラリは ABI v9 だけを受理し、`InkpodCoreConfig::abi_version` が完全一致しなければ
`INKPOD_STATUS_INCOMPATIBLE_ABI` を返す。関数名や型名の `_v3` は、値／ID API 群が導入された世代を
示す接尾辞であり、ABI v3 の呼び出し側との実行時互換性を意味しない。ABI v1-v8 の呼び出し側は、
現行の v9 ヘッダーへ更新して再ビルドする。

ABI v9 は `_v3` の値／ID 専用プリミティブ制御 API と永続化 API を保持し、
sequence source identity、Rust-owned二セルpair preview、その bounded candidate照会、
読込済みBatch graphのoperation照会とimmutable run-copy作成を追加した。
ABI v2 で公開名から実装時のマイルストーン番号を除いたタスク API は、現行 v9 でも引き続き
`InkpodTask` / `InkpodTaskInfo` / `INKPOD_TASK_*` / `inkpod_task_*`、共有ラスタ入力は
`InkpodRasterSourceInput` を使用する。v1 のマイルストーン名は公開別名として残していない。

既存のラスタ文書オープン／インポート、クリップボード、ライトテーブル入力は、現行 v9 の操作別 API の
一回の上限付き呼び出し中に、同期的に検証、コピー、正規化、登録される。ストロークサンプルも同期的に
Rust 所有の正規化済みバイト列へコピーされ、4 MiB 以下ならプロシージャ内のペイロード、4 MiB 超なら
正規化済みサンプルアセットになる。シーケンスの入力元は従来どおり上限付きの Rust 所有ラスタコピーであり、
ベクタープリミティブは型付きジオメトリと安定 ID をプロシージャに保持する。
`*_v3` API 群の呼び出し側は、世代付き実行時オブジェクト ID を明示的に解放する。操作別 API 群が使う
正規アセットの保持は Core 内部で行われ、`_v3` 付き実行時オブジェクト ID と、永続的で内容アドレス方式の
`AssetId` は別の名前空間に属する。

## 決定的リプレイ契約と複合ダイジェスト

現行 ABI は、所有権を移さない読み取り専用の固定レイアウト照会を二つ提供する。

- `inkpod_core_get_replay_contract` は、呼び出し側が所有する `InkpodReplayContract` へ値を書き込む。
  リプレイエポック 12、現行のプロシージャ／コンテナバージョン 15、正規数値バージョン 1、閉じた
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
製品の保存／オープン API は同じ v17 のリプレイ／カタログ契約を使い、現行でないネイティブ形式の
バージョンをすべて拒否する。

現行 ABI v9 は、Core 所有スレッド専用の永続化操作を三つ提供する。`inkpod_core_get_persistence_info` は、
形式バージョン、最後に成功したオープン方式、正本であるジャーナルの件数、決定的なリプレイ作業量と
未保存変更量（`dirty_bytes`）、アセット使用量、`INKPOD_PERSISTENCE_CHECKPOINT_DUE` フラグを、
リプレイや状態変更を行わずに返す。`open_strategy` は `INKPOD_NATIVE_OPEN_NOT_OPENED`、
`INKPOD_NATIVE_OPEN_FULL_REPLAY`、`INKPOD_NATIVE_OPEN_CHECKPOINT` のいずれかである。
`inkpod_core_compaction_plan` は、破棄されるイベント数とプロシージャ数に加え、文書、EditorState、
ジャーナルの正確なダイジェストを返す。UI は履歴件数を表示して確認を得た後、そのレコードを変更せずに
`inkpod_core_write_compacted_copy` へ渡す。書き込み時に確認トークンが古ければ `INVALID_STATE`、
トークンのフラグまたは予約領域が 0 でなければ `UNSUPPORTED` になる。成功時は、現在状態を新しい Genesis
とする別の v17 ファイルを書き出すが、作業中のパス、リビジョン、未保存状態、保存点、ID、履歴は変更しない。
`CoreHost` は三つの操作すべてを Core エンジンキュー経由で実行する。自動的な履歴圧縮は行わず、`CKPT` は
履歴やアセット保持の正本ではない。Windows では `ファイル > 履歴を破棄してコピー...` として公開し、
最初に失われるイベント数とプロシージャ数を表示する。出力先には開いているセッションが所有しないパスだけを
許可し、作成したコピーを現在の保存先として採用しない。

## 新規 Cell creation plan（現行 ABI v9）

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
ID cursor、出力 `InkpodDocumentInfo` を変更しない。ABI v9 の
`InkpodDocumentInfo` と `InkpodPaperFramesInput` は撮影／最大寄り frame を含み、
前世代 layout を暗黙に受理しない。

## ABI v9 の `_v3` 付き値／ID 制御 API

この節の `V3` / `_v3` は API 群とレコード名の一部である。すべての呼び出しは、全体として
ABI v9 に一致するヘッダーとライブラリの組み合わせで使用する。

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

## Batch行と二セルpair preview（ABI v9）

`InkpodBatchColorPairInput` と `InkpodBatchSeedInput` は graph 作成呼び出し中だけ
borrowed である。各行は完全な `struct_size`、整列済みstride、上限付き件数を必要とし、
Core graph は復帰前に enabled、exact-depth color、座標、tolerance、gap、expected
color を値としてコピーする。seed の `INKPOD_BATCH_SEED_ENABLED` とpairの `enabled`
が実行参加を決める。separation destination は 1 ReplaceSource、2 SelectionMask、
3 MainLinePlane、4 ColorPlane、5 NativeFile の閉じた列挙で、未知値、短いrecord、
oversized span、重複した有効行は原子的に拒否される。`.inkbatch` graph versionは2である。

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

previewは候補を自動決定しない。one-to-manyの解決（候補を一つ選ぶか旧色を除外するか）
と、`configure-each-run` のUI入力はjob enqueue前のfrontend責務である。
`inkpod_batch_graph_get_operation` は完全サイズのcaller-owned scalar/count recordへ
一操作をコピーし、対応するcolor／pair／seed／curve-point indexed queryがnested行を
caller-owned recordへコピーする。graphは照会中だけborrowedで、返したrecordはgraph
解放後も値として保持できる。NULL、短いrecord、範囲外index、kindと一致しないrow queryは
失敗し、graph、Core、document stateを変更しない。

読込済みpresetを実行するfrontendは、照会したstored valueから全operationのrun入力を作り、
設定対象のflagをすべてclearして `inkpod_batch_graph_clone_with_operations` に渡す。入力spanは
呼び出し中だけborrowedで、件数はsource graphと完全一致しなければならない。成功時だけ別の
Rust-owned immutable graphを返し、元preset graphは不変である。非NULL出力owner、短い／過大な
row、件数不一致、未解決flagは原子的に拒否する。run-copyも通常の
`inkpod_batch_graph_release` で解放する。Core executionは未解決flagを持つgraphをdry-runを含め
拒否するため、enqueueするgraphは必ず完成済みimmutable copyであり、実行中に行やdestinationを
変更しない。

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
form a non-nested group whose opacity applies once to the group. Raster, fill,
and stroke records index the corresponding snapshot view span; adjustment
records index one LUT. The adapter rejects NULL, short/misaligned records,
unknown pass kinds, invalid group structure, out-of-range item spans, nonzero
reserved fields, and opacity above 1000 before the renderer retains a snapshot.
The records transfer no ownership. The render-plan export was additive in ABI
v5; the current library nevertheless requires ABI v9 because the selection
records now carry range, construction, pressure, and view-zoom values.

### View-local vector diagnostics

`inkpod_snapshot_get_vector_diagnostics` copies an exact-size caller-owned
`InkpodSnapshotVectorDiagnostics`. Its flags are a mutually consistent snapshot
of the target view: antialias, centerline visible, centerline only, and endpoint
markers visible. Centerline-only always implies centerline-visible. The endpoint
pointer is a snapshot-owned borrowed span with exact
`sizeof(InkpodSnapshotVectorEndpoint)` stride and at most 131,072 records; a
nonempty span is returned only when endpoint markers are enabled. Each record
contains stable path/plane IDs, a closed start/end endpoint kind, and finite
document coordinates. Records are strictly ordered by path ID then endpoint.

Endpoint records come only from the Rust-owned explicit topology. The FFI and
renderer do not infer connection from equal or nearby coordinates. Centerline
width and endpoint marker radius are renderer-owned device-pixel presentation;
the record contains no DPI-scaled geometry. The span remains valid until
`inkpod_snapshot_release`, transfers no ownership, and may be read on an
externally synchronized renderer thread. NULL/misaligned/short output records,
unknown view-command flags or centerline values, invalid endpoint kinds,
excess counts/strides, and inconsistent flag combinations are rejected before
retention. The additive export and view commands retain ABI v9.

### Filter preview session

`inkpod_core_filter_preview_begin[_task]`,
`inkpod_core_filter_preview_update[_task]`,
`inkpod_core_filter_preview_apply`, and `inkpod_core_filter_preview_cancel`
operate on the Core owner thread and reuse the existing ABI v9 records. The
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
raster/vector geometry state machine on the Core owner thread.
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
caller-owned `out_path_id` and `out_fill_id` receive stable IDs only for the
corresponding vector objects; raster and absent fill return zero. Failure
zeroes both outputs.

NULL, misalignment, short outer or nested records, zero/oversized counts,
invalid stride, unknown primitive/flag, nonzero reserved fields, stale base,
cross-target update, and point/work overflow are rejected without partial
preview or committed publication. Preview snapshots follow the normal
Rust-owned snapshot lifetime. Geometry-created vector segments use flag bit 2,
`INKPOD_SNAPSHOT_VECTOR_SQUARE_CROSS_SECTION`, for square caps; other unknown
segment flag bits remain invalid. These are additive ABI v9 exports and do not
make an older ABI version acceptable.

解放後は、ハンドルから得たタイル、ピクセル、ガイド、ベクター、文字列、バイト列と、コピーしておいた
別名ポインターを一切使わない。Rust が確保したオブジェクトを `free`、`delete`、`CoTaskMemFree` で解放しない。

主な所有者と借用データの関係は次のとおりである。

| 所有対象             | 所有期間                          | 借用データの有効期間                                      | 解放                          |
| -------------------- | --------------------------------- | --------------------------------------------------------- | ----------------------------- |
| Core                 | 作成成功から破棄まで              | Core ポインターは所有スレッドでの呼び出し中だけ利用       | Core 所有スレッド             |
| スナップショット     | 構築成功から解放まで              | タイル、ピクセル、変換、ガイド、ベクターは解放まで        | 外部同期した任意スレッド      |
| クリップボード       | コピー／作成成功から解放まで     | ラスタ出力は呼び出し側バッファ。内部ペイロードは非公開   | 外部同期した任意スレッド      |
| バイトバッファ       | 出力成功から解放まで              | バイト列は解放まで                                        | 外部同期した任意スレッド      |
| エンコード済み列     | 出力成功から解放まで              | 要素名とバイト列は解放まで                                | 外部同期した任意スレッド      |
| タスク／バッチタスク | 作成成功から解放まで              | 照会値は呼び出し側へのコピー                              | Core 呼び出し終了後の任意スレッド |
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

`inkpod_core_locator_neighborhood` は、モードレスなロケーターの拡大表示に必要な複数ピクセルを、
所有スレッド上の一回の ABI 呼び出しで返す。`radius` は 0..16 で、出力は常に
`(radius * 2 + 1)` の正方形に密配置した非乗算アルファの RGBA8 となり、文書外は透明になる。
`pixel_capacity == 0` かつ `pixels_rgba8 == NULL` でメタデータと `required_bytes` を問い合わせ、
十分な大きさの呼び出し側所有バッファを設定して再度呼ぶ。バッファは呼び出し中だけ借用され、Core は
保持しない。必要量照会とコピーのどちらも、文書、ビュー、リビジョン、未保存状態、Undo を変更しない。

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

`inkpod_core_sequence_thumbnail_get` も同じ呼び出し側所有の照会／コピー契約を使う。
`pixels_rgba8 = NULL`、`pixel_capacity = 0` の照会で `required_bytes`、寸法、ストライド、チェックサムを
取得し、確保後の二回目で上限付きの非乗算アルファ RGBA8 をコピーする。ポインターは呼び出し中だけ
借用され、Core は保持しない。二回の呼び出しは同じ `DocumentSessionId + Generation` の
`CoreHost` 作業項目内で行い、別のアクティブ文書へ解決し直さない。

`inkpod_core_sequence_import_mixed_encoded` の `InkpodNamedRasterInput` 列は、呼び出し側所有の借用入力である。
レコード、UTF-8 名、エンコード済みバイト列は呼び出し終了まで有効とする。Core は全レコードの構造、形式、
長さを検証してから各画像をデコードする。全件成功時だけシーケンスを一括置換し、一件でも入力不正、
デコード失敗、割り当て失敗があれば、以前のシーケンス、現在の文書、未保存状態、Undo を保つ。

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

## サブパレット参照スナップショットの契約

読み取り専用の参照ビューアーは、対象 `DocumentSessionId + Generation` の Core 所有スレッドで
`inkpod_core_subpalette_set` と `inkpod_core_view_create` を呼び、返された Core 内のビュー ID を
そのセッション名前空間の外で経路選択キーとして使わない。

- `inkpod_core_subpalette_view_apply` は、そのビューのズーム、パン、反転、ビューポートだけを変更する。
- `inkpod_core_subpalette_view_sample` は、同じビュー変換を通したデバイス座標を半開区間の境界で検証し、
  入力元の RGBA8/16 色深度を保って、呼び出し側所有の `InkpodColorValue` へコピーする。
- `inkpod_core_subpalette_build_snapshot` は、NULL の所有変数へ Rust 所有の不変スナップショットを返す。
  通常のスナップショットと同様に、成功後は受け取り先または呼び出し側のどちらか一方だけが
  `inkpod_snapshot_release` の責任を持つ。

これら三関数とビューのクローズは Core 所有スレッド専用である。スナップショットの参照と解放だけが、
外部同期した任意のスレッドから利用できる。参照ラスタは編集可能文書へ組み込まれず、文書リビジョン、
未保存状態、Undo/Redo、保存点を変更しない。Windows Canvas はストローク入力を消費し、編集コマンドを
Core へ送らない。対象の再割り当て、クローズ、終了処理では、先に Canvas の受け取り先を解除し、
捕捉済みセッション／世代の Core 上でビューを閉じてから Canvas 所有者を破棄する。

## EditorDefaults / EditorState（現行 ABI v9）

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
  private clipboard handle は ordered raster/vector plane payload、document origin、型、
  8/16-bit 値、vector path/fill topology を Rust 側で所有し、paste/cancel/release まで
  C++ が内部 pointer を参照しない。

次の八つの Core 所有スレッド用 API と固定幅レコードは ABI v2 以降に追加され、現行 ABI v9 に保持されている。
ABI v2 のライブラリや呼び出し側を受理するという意味ではない。

- `inkpod_core_get_editor_defaults` は文書作成前にも有効な Rust 所有の不変 `InkpodEditorDefaults` を、
  呼び出し側所有のレコードへコピーする。組み込みの初期文書仕様と EditorState 初期値はアプリケーション設定
  ではなく、新規文書の作成時に Core がセッションの Genesis／EditorState へ明示的にコピーする。
- `inkpod_core_get_editor_state` は現在の `InkpodEditorStateInfo` を副作用なくコピーする。
- `inkpod_core_update_editor_state` は `InkpodEditorStateUpdate` の種類と、期待する正確な
  `EditorRevision` を検証し、成功時の完全な `InkpodEditorStateInfo` をコピーする。更新種別は、
  アクティブツール、ツール色、ツール直径、塗り、選択、ベクター、アクティブ対象、パレットカーソル、ブラシ設定の
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

ABI v9 の `InkpodSelectionInput` と `InkpodEditorSelectionOptions` は、range interpretation、
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
`InkpodEditorVectorOptions`、`InkpodEditorBrushOptions`、`InkpodEditorStateInfo`、`InkpodEditorDefaults`、
`InkpodEditorStateUpdate`、`InkpodEditorStrokeInput` である。呼び出し側は、最上位の入力レコードと、
その入力が使用する各入れ子レコードの `struct_size` を、現行 ABI v9 ヘッダーにある完全な
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

## 正規 Genesis とアセット取り込み（現行 ABI v9）

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
| 現行 v17 の通常保存                                           | 不変                 | 置換成功時に文書／EditorState とも保存済み | 不変                    |
| 自動保存                                                      | 不変                 | 不変                              | 不変                              |
| 新規作成／インポート                                          | 新しい文書情報が正本 | 戻り情報が正本                    | 新しい Genesis／履歴              |
| v17 のオープン／復旧                                          | 実行時リビジョンを付け直す | 戻り情報が正本               | ファイルの全ジャーナル／履歴を復元 |

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

バッチ実行だけは、キャンセル／失敗時にもレポートの所有権を返すことがある。戻りステータスを確認した後も、
`out_report != NULL` なら内容を読み、必ず解放する。

## 保存、自動保存、復旧

通常保存では、v17 の必須セクション `META` / `GENS` / `ASST` / `PROC` / `EDIT`、保持対象の不透明な任意
セクション、チェックポイントの作成条件を満たす場合だけ任意の `CKPT` を構築する。保存後に設定予定の
文書／EditorState 保存点を含むコンテナは、同じディレクトリの一時ファイルへ複数回に分けて書き込む。
フラッシュ、同期、クローズを終えてから置換する。成功後だけ通常保存パスと両保存点を Core へ公開するため、
EditorState だけが
未保存の場合も、再オープン直後は保存済みになる。失敗時に元ファイルを切り詰めず、文書／EditorState の
どちらの保存点も変更しない。

自動保存とエクスポートは、出力を原子的に書いても通常保存パス、文書／EditorState 保存点、未保存状態を
変えない。通常の v17 オープンでは、Genesis、アセット、プロシージャジャーナル、カーソル／分岐、すべての
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
  返さない。mode は raster color/main-line、vector color-line/main-line/fill を明示し、region は pen、rectangle、
  polyline、lasso のいずれかだけを受理する。base document revision、stable Plane ID、native-depth target/replacement、
  size/flags/reserved/alignment/count/stride を境界で検証し、stale、invalid、overflow、hidden/locked target、failure は
  文書・履歴・ID・dirty を進めない。preview は常に非変更で、apply の実変更だけが一つの canonical Undo 単位になる。
