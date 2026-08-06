# Rust Core プリミティブ・Procedure Call History・`.inkpod` 再設計計画

更新日: 2026-08-07

状態: M0–M8 完了、M9 未完。全production document mutationのcanonical primitive化、ABI v3、typed CoreHost queue、cross-architecture replay gate、旧 revision-max 判定式の正本化、環境別性能ゲート、current-only `.inkpod` v8 cutoverは反映済み

## 1. 目的

Inkpod を、Win32 frontend から独立した決定的な Rust 編集エンジンとして再定義する。

- Inkpod の画像編集、文書操作、履歴、既定値、入力の意味解釈を Rust Core のプリミティブ機能として所有する。
- Win32 は Windows message、dialog、file authority、clipboard 接続、DPI、accessibility、renderer を担う shell に限定する。
- `.inkpod` の意味上の正本を、保存時点の可変 raster snapshot ではなく、immutable な初期状態と asset、正規化済み Procedure Call History とする。
- 同じ replay epoch、同じ genesis、同じ procedure 列から、対応 architecture 間で同じ Core 意味状態と bit-exact な canonical composite を得る。
- Undo/Redo、savepoint、redo branch の状態を Rust Core が所有し、通常保存後の reopen でも復元する。

本計画は実装順と完了条件を定める作業文書である。恒久的な製品仕様は実装前に `SPEC.md` へ反映し、所有権・ABI・ファイル形式の詳細はそれぞれ `docs/architecture.md`、`docs/ffi.md`、`docs/file-format.md` を正本として更新する。

## 2. 確定した設計判断

### 2.1 Rust のメモリではなく意味状態を永続化する

`.inkpod` は Rust の object layout や runtime cache の dump ではない。次だけを永続化の正本とする。

- `Genesis`: 文書の初期 topology、paper、DPI、color space、初期 stable ID、immutable base bitmap
- `Assets`: import、clipboard、Light Table、将来必要となる font 等の immutable canonical payload
- `Procedures`: Core が検証・正規化し、成功した transaction 単位の呼び出し記録
- `History state`: current cursor、active branch、redo availability、document savepoint、branch-cut event
- `Editor state`: active tool、tool ごとの current color、diameter、fill/selection/vector option 等、再開に有益な文書単位の編集状態
- `Editor savepoint`: 保存済み editor-state digest。session dirty は document state と document savepoint、または editor state と editor savepointのどちらかが異なれば成立する

次は永続化しない。

- COW node、inverse delta、materialized working document の一時 clone
- render/upload/thumbnail cache、GPU resource、immutable render snapshot
- active stroke、dialog preview、floating preview、task、progress、cancellation token
- window、dock、workspace、focus、monitor、DPI、file dialog、recent files
- shortcut、language、theme 等の application preference
- OS path、file handle、Windows file identity。必要な provenance path も既定では保存しない

### 2.2 Control plane と data plane を分ける

文書を変更する primitive の control plane は、固定幅値と Rust-owned object/asset ID だけを使う。

- primitive request は C++ memory、raw path、callback、STL object を保持しない。
- C record を ABI 上 `const T*` で一時的に渡すことは許すが、意味上は call-by-value とし、Rust は復帰後に pointer を保持しない。
- 可変長の sample、path、encoded image、clipboard payload は、bounded data-plane API で Rust-owned objectへ取り込み、primitive はその ID を参照する。
- data-plane の一時 object ID は invocation 中だけ有効とし、commit 時に inline canonical payload または immutable `AssetId` へ解決する。procedure journal へ一時 object ID を残さない。
- snapshot、thumbnail、export 等の大容量出力は、generation 付き Rust-owned ID と bounded bulk copy/borrow を使う。
- C++ は Rust allocation の所有権を持たず、ID release または明示 owner release API だけを使う。

### 2.3 生の FFI call ではなく Canonical Procedure を記録する

永続記録は frontend request ではなく、Core が確定した `CanonicalProcedure` とする。

```text
Frontend request
  -> validate target/revision/IDs
  -> normalize coordinates, colors, options, and variable payload IDs
  -> allocate explicit output IDs inside the transaction
  -> execute against working state
  -> detect no-op and validate limits
  -> atomically publish state/history/journal/revision/cache invalidation
```

`CanonicalProcedure` は少なくとも次を持つ。

- monotonic `ProcedureId`
- stable `PrimitiveId` と primitive schema version
- replay epoch
- base state ID と committed state ID
- canonical fixed-width arguments
- referenced immutable `AssetId`
- transaction 内で割り当てた output ID
- 必要な bounded inline canonical payload、その length と digest
- pre-state / post-state document-state digest

raw pointer、native enum layout、padding、frontend command ID、device-dependent path、external file pathを含めない。
request が参照した一時 object の内容は canonicalization 中に確定し、小さい payload は procedure に inline 化し、大きい payload は content-addressed asset へ昇格する。したがって journal 単体が解放済み runtime object に依存することはない。

### 2.4 Journal に残すもの

永続 journal に記録するのは次だけとする。

- 実変更を commit した document transaction
- Undo、Redo、history jump
- Undo 後の新規 commit による branch cut

journal の record は閉じた型 `JournalEntry::{Commit, HistoryMove, BranchCut}` とする。

- `Commit`: event ID、canonical procedure、parent/base `StateId`、committed `StateId`、所属 branch ID
- `HistoryMove`: event ID、Undo/Redo/Jump の種別、移動元/移動先 `StateId`、active branch ID
- `BranchCut`: event ID、分岐点 `StateId`、旧 active tail、新 branch ID、非 active 化した branch ID
- journal event ID は単調増加し、file order と一致する。Undo 後の新規編集では `BranchCut` と `Commit` をこの順序の一つの atomic publish batch として追加し、commit recordへbranch cutを重複記録しない

次は記録しない。

- query
- invalid、failure、cancel、stale、overflow
- 意味上の no-op
- stroke begin/append、preview begin/update 等の transient update
- editor state の途中操作。procedure が色や径を使う場合は、確定値を procedure 引数へ複製する

stroke は begin/append/end の runtime session を維持してよいが、成功した end 時に一つの canonical `ApplyStroke` として記録する。preview は apply 時だけ一件を記録する。

### 2.5 Undo/Redo と完全非破壊性

- immutable genesis、asset、committed procedure は暗黙に書き換えない。
- runtime Undo 高速化用の inverse delta や COW snapshot は派生 cache であり、永続正本ではない。
- Undo 後の新規編集では、現行仕様どおり以前の redo branch を通常 UI の対象から外す。
- 外れた branch の procedure は append-only journal 内に保持し、branch-cut event で非 active とする。
- reopen 後も、保存時点の history list、cursor、active redo tail、document/editor savepoint、非 active branch の監査情報を復元する。
- history を失う compaction は自動実行しない。将来提供する場合は、利用者が明示的に選ぶ「新しい genesis への書き出し」とする。

### 2.6 白紙と外部 asset

- 白紙の元画像は意味上 opaque white bitmap とする。
- 全面 tile allocation は行わず、immutable `SolidWhite` constant surface と sparse materialization で表現する。
- Genesis base surfaceはeditable layer/planeとは別のimmutable underlayとする。blank documentのflat canonical composite/exportにはopaque whiteとして参加するが、layer/plane単独exportやselection mask自体へ暗黙に混入させない。
- 「画像を新規文書として開く」はcanonical decoded bitmapをGenesis base assetとする。「既存文書へ読み込む」はasset ingestion後のImport primitiveとする。
- import、clipboard、Light Table 等は ingestion 時に Rust が canonical pixel/vector payloadへ変換し、content-addressed `AssetId` を発行する。
- procedure は外部 path や codec 呼び出しを再実行せず、`AssetId` を参照する。
- 元の PNG/TIFF/TGA/BMP bytes は必要なら optional provenance として保持できるが、replay の入力にはしない。

### 2.7 決定性と versioning

- 同一 replay epoch 内では、x64、ARM64、非 Windows Rust target 間で canonical Core state と canonical composite を bit-exact にする。
- Direct2D/D3D の画面上の antialiasing や monitor 表示までの pixel 完全一致は要求しない。
- 永続引数は fixed-point または明示的に bit pattern を固定した整数表現を用いる。
- rounding、alpha、color distance、filter kernel、transcendental function の実装を仕様化する。
- serialized schema だけでなく、replay 結果を変える primitive semantics の変更でも top-level format/replay version を上げる。
- ユーザーがフォーマットフリーズを宣言するまで、reader は exact current version だけを受理する。旧 reader、migration、compatibility shim は作らない。
- digestは用途ごとにdomain separationし、`DocumentStateDigest`、`EditorStateDigest`、`JournalPrefixDigest`、`AssetDigest`、`SectionDigest`、`FileRootDigest`を混用しない。procedureのpre/post digestはdocument stateだけを対象とする。
- 永続preconditionと履歴参照は`StateId`を使う。`DocumentRevision`はstale request検出用のsession-local counterであり、fileへ保存せず、open時に新しいCore generation内でrebaseする。

## 3. 現状レビューの基準点

### 3.1 維持する長所

- Rust Core/Image/Format は OS 非依存で、画像処理と document validation の大部分を所有している。
- `DocumentEdit` は stale base、overflow、no-op を検査し、document/revision/history/cache を一回で公開する。
- Undo/Redo payload は C++ ではなく Rust Core が所有している。
- stable ID、typed layer/plane、sparse 64 x 64 tile、Arc COW、selection、Light Table、vector、adjustment は Rust 側にある。
- `.inkpod` v2 decoder は寸法、count、length、checksum、ID/reference を bounded に検証する。
- save は same-directory temporary file を完成・flush・closeしてから置換する。
- current-version-only 方針はすでに `AGENTS.md` と `SPEC.md` にある。

### 3.2 解消する主要 gap

- Rust の統一 `Command` と C ABI batch command は `NoOp` しか持たず、実編集 API が分散している。
- `CoreHost` は serializable request ではなく任意の C++ closure を queue に保持する。
- history entry は pixel delta、palette/color before/after、または document before/afterで、呼び出した primitive と引数を復元できない。
- `.inkpod` v2 は `CellDocument::to_file()` による materialized state snapshotであり、open時にhistoryをresetする。
- stable-ID allocator の high-watermarkと削除済みIDが保存されず、open時はlive IDの最大値から再構成する。
- toolごとのcurrent color、diameter、fill/selection/vector optionsと多くの既定値がworkspace単位のC++ stateにある。
- C++がfill rounding、device/document変換、vector geometry生成等の結果を左右する意味処理を行う。
- C ABIはborrowed span、Rust-owned raw pointer、snapshot内Rust pointerを公開している。
- 白紙は透明zero planesとrendererの白背景であり、immutable white source bitmapではない。
- 現行formatはmanifest全体のstrong digest、content-addressed asset、opaque optional metadata round-tripを持たない。
- `exp`、`powf`、`sin/cos`、`f32/f64` 入力等、cross-architecture bit-exact replayの未監査箇所がある。

## 4. 目標状態の分類

| 状態 | owner | `.inkpod` | revision/history |
|---|---|---|---|
| Genesis、base bitmap、layer/plane topology | Rust Core | 必須 | 初期 state |
| Document mutation | Rust Core primitive | `PROC` | 1実変更 = 1 document revision = 1 history entry |
| Undo/Redo/jump/branch cut | Rust Core journal | `PROC`/`META` | cursor/stateを変更、history itemは増やさない |
| Asset payload | Rust asset store | `ASST` | immutable、content-addressed |
| Active tool、tool別色、径、fill等 | Rust `EditorState` | `EDIT` | editor revisionとeditor dirtyのみ。document historyには入れない |
| Active layer/plane、palette cursor等の編集target | Rust `EditorState` | `EDIT` | editor revisionとeditor dirtyのみ。document historyには入れない |
| Selection mask、palette content、main-line color等の文書内容 | Rust document | procedureから導出 | 意味変更時のみdocument revision/history |
| Zoom、pan、flip、viewport、表示補助 | Rust view state | 保存しない | view revisionのみ |
| Preview/stroke/floating staging | Rust transient session | 保存しない | commit前はdocument不変 |
| Renderer snapshot/cache/GPU | Rust snapshot + Win32 renderer | 保存しない | query/cacheのみ |
| Workspace/window/path/file identity | Win32 adapter | `.inkpod`外 | document semantics外 |

## 5. 目標となる `.inkpod` container

次期形式は現行 v2 から top-level version を上げる。最初の実装時点では v3 を想定するが、その後もschemaまたはreplay semanticsを変更するたびにversionを上げるため、フォーマットフリーズ前の最終番号は固定しない。

section directory、`META`、`GENS`をversioned manifest領域とし、容量の大きいasset/checkpoint payloadは`ASST`/`CKPT` blob領域へ分離する。

### 5.1 Header と section directory

- little-endian、固定幅整数、明示alignment、reserved=0
- magic、top-level format/replay version、required flags
- total file length、section directory offset/count
- `FileRootDigest`: domain tag、digest fieldをzero化した固定header、section directory bytesをこの順でBLAKE3-256へ入力する。directory entry内の全section digestを通じてfile全体をcoverする
- 各 section entryに type、section version、critical/optional flags、offset、stored length、logical length、compression code、stored-byte digest、logical-byte digest
- section rangeの重複、overflow、truncation、duplicate required sectionを拒否
- unknown critical sectionを拒否し、unknown optional sectionはbounded opaque bytesとしてround-trip

全digest入力はdomain tag、schema version、typed descriptor、各fieldのlittle-endian length prefix、payloadの順を厳密に定義する。stored-byte digestはfile上のexact bytes、logical-byte digestはdecompression後のexact canonical bytesを対象とする。`AssetId`はpayloadだけでなくpixel/alpha/color semantics、dimensions、strideを含むcanonical descriptorとpayloadから導出する。自己参照するdigest fieldはzeroとして計算する。exact byte contractはM0で`docs/file-format.md`へ固定する。

初期実装はcompression code `None` のみを受理する。圧縮を追加する場合はbenchmark、allocation bound、license確認後にtop-level versionを上げる。

### 5.2 Required sections

`META`

- document UUID
- replay epoch / primitive catalog digest
- current `StateId`、history cursor、active branch、document savepoint `StateId`、editor savepoint digest
- next stable-object ID、`ProcedureId`、`StateId`、`JournalEventId`、`BranchId` の各high-watermark
- procedure、journal event、asset、editor-state count
- expected current `DocumentStateDigest`、`EditorStateDigest`、`JournalPrefixDigest`

独立した`HIST` sectionは作らない。履歴の現在位置とsavepointは`META`、履歴操作とbranch構造は`PROC`のcontrol eventで表現する。

`GENS`

- initial document、paper、DPI、sRGB、frame、margin
- initial stable IDs と typed topology
- immutable base surface参照。白紙は`SolidWhite`
- implicitなbuild-defaultへ依存せず、replayに必要な初期値を完全記述

`ASST`

- BLAKE3-256 content address
- typed format、dimensions、stride、color/alpha semantics
- bounded chunk descriptorとpayload digest
- canonical decoded sourceを必須、元encoded bytes/provenanceはoptional
- procedure に inline 化しない大きな sample/path payload も immutable canonical asset として格納可能

`PROC`

- length-delimited、monotonicなprocedure/control-event列
- commit、history move、branch cutを区別
- primitive ID/schema、canonical arguments、stable input/output IDs、pre/post state digest。一時 object ID は禁止
- entry count、entry bytes、total replay workをboundedに検証

`EDIT`

- active tool、last color-consuming tool
- toolごとのexact-depth current color
- diameter、fill/selection/vector option等の意味上の現在値
- active layer/plane、palette cursor等の編集target
- editor-state schema version、persisted editor revision、`EditorStateDigest`

### 5.3 Optional sections

`CKPT`

- procedure prefixのdigestと対応するstate digest
- materialized sparse document state
- open高速化専用で、genesis/assets/proceduresの代替にしない
- section hash、構造、resource boundが不正ならfile corruptionとして全fileを拒否する。構造は有効だがprefix/state digestまたはcheckpoint epochが一致しない場合だけcacheを無視してfull replayし、その最終digestがauthoritative digestと一致しなければfileを拒否する

`EXTM`

- provenance、annotation、将来のoptional metadata
- replay結果に影響しない
- 未知recordをopaqueに保持してround-tripできる
- Rust Core/format-owned document attachmentとして保持し、C++へownershipを渡さない。未知opaque recordはassetへの意味上の参照を宣言できない

### 5.4 Save/Open

- decode、全参照検証、asset検証、replayはstaged Core上で行い、成功時だけlive Coreと一回で交換する。
- cancel、failure、stale、resource-limitで既存Coreを変更しない。
- saveはstreaming encoderでsame-directory exclusive temporary fileへ書き、全section/digest/directoryを完成し、flush、`sync_all`、close後にdestinationを置換する。
- normal saveはcurrent `StateId`とcurrent `EditorStateDigest`をprospective savepointとしたstaged `META`を書き、destination置換成功後だけlive Coreへ同じdocument/editor savepointを公開する。encode/write/flush/replace失敗ではfileとlive savepointの双方を変更しない。
- session dirtyは`current StateId != document savepoint`または`current EditorStateDigest != editor savepoint digest`の論理和とする。
- autosave/recovery/exportは通常savepointを進めず、recovery metadataに元の通常保存path/authorityを継承させない。
- v2を含む非current top-level versionは明示的に拒否する。

## 6. マイルストーン概要

| Milestone | 目的 | 主な完了証拠 |
|---|---|---|
| M0 | 仕様と全mutation inventoryを固定 | `SPEC.md`、primitive分類表、境界guard |
| M1 | Canonical primitive kernelのCore-only縦切り | palette/main-line/stroke replay一致 |
| M2 | JournalとUndo/Redoを統合 | branch/cursor/savepoint state-machine test |
| M3 | EditorStateと既定値をCoreへ移管 | document切替・再作成時のCore-owned defaults |
| M4 | Immutable genesis/asset storeを導入 | white/import/clipboard/Light Table asset replay |
| M5 | ABI v3とtyped CoreHost queueへ移行 | value/ID-only control-plane contract test |
| M6 | 全primitive familyとWin32 thin-shellを完成 | 全production mutation routeのcatalog coverage |
| M7 | Cross-architecture決定性をgate化 | x64/ARM64/非Windows golden replay |
| M8 | 次期`.inkpod`へ切替 | save/reopen後のimage/history/editor state完全一致 |
| M9 | Checkpoint、性能、安全性、旧経路削除 | full replay同値、fuzz/bounds/bench、legacy不在 |

## 7. 詳細マイルストーン

### M0: 意味仕様・分類・追跡の固定

目的:

- 実装前に、何をprimitive、query、view、transient、I/O、application stateと呼ぶかを一意にする。

作業:

- `SPEC.md` に本計画の確定事項を製品仕様として反映する。
- `.inkpod = Genesis + Assets + Procedures/History controls（`PROC`）+ History state（`META`）+ EditorState + optional Checkpoints` を定義し、独立`HIST` sectionを作らない。
- 全公開Rust mutation、全C ABI export、全Windows production commandを棚卸しする。
- 各routeを次のいずれか一つへ分類する。
  - document primitive
  - history control event
  - editor-state command
  - view-only command
  - transient preview/stroke protocol
  - query/snapshot
  - asset/data-plane ingestion/export
  - OS/application adapter
- stable `PrimitiveId` namespace、primitive schema version、replay epochの更新規則を決定する。
- `JournalEntry::{Commit, HistoryMove, BranchCut}` のfield、ordering、atomic append規則を決定する。
- persistent `StateId`とsession-local `DocumentRevision`、persisted editor revisionの役割を分離する。
- canonical coordinate unit、fixed-point scale、rounding、各digestのdomain tag/対象/field order/length prefix/zero-field規則を決定する。
- procedure count、payload、asset、file、replay work、object countのexact limitsを決定し`docs/file-format.md`に表を作る。
- BLAKE3実装依存のlicenseと配布条件を確認し、採用時に`docs/third-party-notices.md`を更新する。
- 影響するrequirementを少なくとも `ARCH-002`、`ABI-001/002`、`IO-001`、`SAFE-001`、`PERF-001`、`DOC-001/002/003`、`HIST-001`、`PAINT-*`、`FILL-*`、`COLOR-*` として追跡する。

テスト/guard:

- mutation API/FFI/Windows commandの未分類・重複ownerを検出するarchitecture test。
- document primitiveがC++ feature implementationをownerにできないstructural guard。

完了条件:

- mutating routeの未分類が0。
- schema/semantics/epoch/limitの判断が文書化され、実装者が新たな製品判断をせずM1へ進める。

### M1: Canonical Primitive Kernel の縦切り

目的:

- Rust Core内に単一のprimitive実行・正規化・commit境界を作る。

作業:

- `PrimitiveId`、`PrimitiveRequest`、`CanonicalProcedure`、`PrimitiveOutcome`、`ProcedureId`、`StateId`、`ReplayEpoch` newtypeを追加する。
- request validation、canonicalization、working-state実行、no-op検出、explicit commitを一つのexecutorへ集約する。
- stable ID割当をtransaction内へ移し、失敗/no-opでhigh-watermarkを進めない。
- output IDをCanonicalProcedureへ明示的に埋め、replay時に一致を検査する。
- memory layout非依存のcanonical state serializerとBLAKE3-256 semantic digestを追加する。
- 最初のstrokeはbounded canonical sample payloadをprocedureへinline化し、callerやtransient objectのlifetimeから切り離す。
- 最初の縦切りとして次を移行する。
  - main-line color
  - palette content replace
  - 一つのraster stroke transaction
- 既存public Rust APIはexecutorを呼ぶ薄いwrapperとし、この段階では利用者向け挙動を変えない。

テスト:

- request実行とcanonical replayのdocument/state digest一致。
- success、no-op、invalid、cancel、stale、overflowのatomicity。
- generated ID、revision、history、dirty、cache invalidationの一回性。
- 同じrequestを異なるtile iteration順で実行した結果の一致。

完了条件:

- 縦切り対象にexecutor外のmutation実装がない。
- runtime結果とfresh Coreへのprocedure replay結果がbit-exact。

Revision-max render-cache invariant:

- render tile の cache source identity は、対象座標にある可視 plane の `tile_revision`、selection の `tile_revision`、Light Table の `source_revision` の数値最大値を取る旧 revision-max 方式を正本とする。
- cache hit 判定はこれらの固定幅 scalar revision だけを読み、source pixel のcopy/走査/hash、content digest、generation、tombstone、epoch、negative cacheを導入しない。zoom/pan等のview-only snapshotをsource raster総byte数へ比例させない。
- `RenderTile.source_revision` は同じCore内のcache検証専用で、公開snapshotの意味上の等価性、canonical document digest、procedure payload、C ABI、native formatへ含めない。別Core間は公開snapshot contentを比較し、cache revision/reuseは同じCore・同じbuild scheduleでのみ比較する。
- procedure replay用のschema-3 document-state digestは別のdomain-separated commitment treeとし、revision一致したraster-tile編集では変わったtile、所属raster root、document rootだけを更新する。metadata編集はmetadata commitmentを更新し、より広いdocument編集はcold rebuildする。このtreeはrevision-maxの入力、render tile identity、snapshot validationには使用しない。
- opacity、visibility、order、main-line color、color-check等のrevision-maxへ含まれないrender metadata editは、所有境界で既存のatomic whole-cache invalidationを必ず行う。
- 数値最大値だけでは、高いLight Table revisionによる低いraster revisionのmask、同値revision sourceの一方の削除、独立revision namespace間の衝突、primary/secondary alpha view間の共有cacheを区別できない。また透明な合成結果は保持されず再合成され得る。これらは性能を優先してrevision-maxを正本とする意図した既知制約として文書化し、暗黙に解消済みと扱わない。
- このruntime cache方針はM8の永続schema、format version、checkpoint authorityを変更せず、M8実装によって自動的に解消されるものでもない。

### M2: Procedure Journal と Undo/Redo の統合

目的:

- journalを意味上の履歴正本にし、Undo高速化用dataと分離する。

作業:

- append-only journal、active branch、history cursor、branch-cut eventをCoreへ追加する。
- journalを`Commit`、`HistoryMove`、`BranchCut`のclosed enumとして実装し、各recordの必須IDと遷移前後stateを検証する。
- `HistoryEntry` はcanonical procedure参照を持ち、inverse delta/COW snapshotはoptional runtime cacheにする。
- commit後にjournal追加だけ失敗する状態を作らず、document/history/journal/revisionを一つのpublish boundaryで更新する。
- `ProcedureId`、`StateId`、`JournalEventId`、`BranchId`はatomic publish時だけ消費し、failure/no-opでhigh-watermarkを進めず、同一document namespaceで再利用しない。
- Undo/Redo/jumpはcursorとdocument revisionを更新し、history itemを追加しない。
- Undo後の新規commitはactive redo tailをUIから外すが、旧procedureをjournalから削除しない。
- savepointを`StateId`で保持し、branch移動後もdirtyを正しく再計算する。
- history/cacheを破棄してgenesisから再生するslow pathを実装し、runtime cacheへの依存を検出できるようにする。

テスト:

- fixed-seed state machineでcommit/Undo/Redo/jump/branch cutを繰り返し、full replayとruntime状態を比較する。
- redo branch truncationのUI契約とjournal保持を同時に検証する。
- branch cutと後続commitが二重記録されず、途中failureで片方だけ残らないことを検証する。
- no-op/failure/cancelがjournal、ID、history、revisionを進めないことを検証する。
- runtime inverse cacheを全削除しても同じ状態へ再生できることを検証する。

完了条件:

- canonical procedure化済みの履歴では、Commit、Undo、Redo、jump、branch cut、savepointがjournal semanticsから導出される。
- canonical procedure化済みの履歴は、runtime inverse cacheを削除してもGenesisとcanonical procedureから同じ状態へ再生できる。
- 未移行mutationはsynthetic snapshot/diff procedureへ変換せずjournal incompleteを明示し、`HIST-001` の既存cached Undo/Redo契約を安全に維持する。
- runtime表現はserializationへ依存せず、M8でjournal semanticsを変更せずに保存・再オープンへ接続できる。

### M3: Core-owned EditorState と既定値

目的:

- 文書編集の意味や初期値をC++ workspace stateからRust Coreへ移す。

作業:

- document session単位のtyped `EditorState` と独立した`EditorRevision`を追加する。
- document作成前に使うRust-owned immutable `EditorDefaults`を`EditorState`から分離する。built-in defaultsはapplication preferenceとして保存せず、新規作成時に明示Genesis/EditorStateへcopyする。
- 少なくとも次をCore-ownedにする。
  - active tool、last color-consuming tool
  - pencilは黒、その他の彩色commandはCore定義の初期色
  - toolごとのRGBA8/16 current color
  - diameter
  - fill tolerance/gap/inclusion/overflow/Light Table options
  - selection shape/operation/tolerance/diameter
  - vector erase/select/tool options
  - new-document dialogへ提示するbuilt-in initial document spec
  - active layer/plane、palette cursor等の編集target
- EditorStateのquery/update APIを作り、C++はコピーしたpresentation cacheだけを持つ。
- document/view切替時は対象CoreのEditorStateを再取得し、workspaceの旧値で上書きしない。
- main-line color、palette、active layer/plane等の既存document stateとの重複ownerを解消する。
- EditorState変更はdocument revision/historyを進めず、編集procedureには使用した確定値を埋め込む。
- palette content、main-line color、selection maskはdocument primitive、palette cursor、active target、selection tool optionはEditorStateとして明示的に分離する。
- `EditorStateDigest`、editor savepoint、editor dirtyを実装し、session dirtyをdocument/editorの合成として公開する。

テスト:

- 複数document、同一document複数view、複数workspaceでcurrent color/optionsが正しいsessionに属する。
- pencil/brush/fill/vector各commandの初期値と切替復元。
- RGBA16とalphaをC++ packed RGBA8へ縮小しない。
- canonical EditorState frameをCoreへ復元した後、C++ presentation同期がactive plane/editor stateを上書きしない。production open経路でのend-to-end復元はM8で検証する。
- 色、径、active targetだけを変更した場合もeditor dirtyとなり、canonical EditorState frameのround-tripとnormal-save成功/失敗に対するsavepoint状態遷移が一致し、Undo historyは増えない。production `.inkpod`のsave/reopenでEditorStateを復元するend-to-end接続はM8のatomic format cutoverで検証する。

完了条件:

- C++にauthoritativeなtool/color/fill/selection/vector defaultが残らない。
- C++ stateはCore queryから再構築可能なpresentation cacheだけになる。
- production `.inkpod` v2を変更せず、M8がそのまま接続できるcanonical EditorState frame、digest、revision、savepoint契約が固定される。

実装実績（2026-08-05）:

- Rust Coreにimmutable `EditorDefaults`、document-session単位のtyped
  `EditorState`、document revisionと独立したnonzero `EditorRevision`、
  domain-separated `EditorStateDigest`を実装した。built-in new-document spec
  とeditor defaultはdocument作成時にcopyし、application preferenceへ保存しない。
- active/last color-consuming tool、tool別のstraight-alpha RGBA8/RGBA16 exact-
  depth color、Q16 diameter、fill/Light Table、selection、vector option、stable
  active layer/plane target、palette cursorを一つのEditorState ownerへ集約した。
  palette content、main-line color、selection maskは引き続きdocument primitiveが
  所有し、重複ownerを置いていない。
- typed query/updateはexact base EditorRevisionを検証し、semantic no-opでは
  revision/digest/dirtyを保持する。実変更はeditor revision/digest/dirtyだけを
  進め、document revision、StateId、journal、Undo history、document savepoint、
  render contentを変えない。invalid、stale、overflow、failureはatomicに拒否する。
- session dirtyを`document_dirty || editor_dirty`として公開した。normal save、
  failure、autosave、recovery、exportを別遷移として固定し、production v2 normal
  saveはdocument savepointだけを進める。v2が復元できないEditorStateをsavedと
  偽装せず、editor dirtyはM8まで残る。
- active targetはstable layer/plane IDで検証し、layer/plane topology変更、native
  open、sequence activate/step、Light Table swapを含むdocument replacementで、
  Coreが最初の有効planeへ決定的に再解決する。target/revision overflowを含む失敗は
  document、stable-ID cursor、EditorState、history、journal、cacheを部分公開しない。
- command/stroke/preview開始時のexact-depth color、diameter、option、stable targetを
  invocationへcopyする。M3で接続したstroke、fill、selection、color routeは、開始後の
  EditorState変更やworkspace切替を参照してretargetしない。
- `ApplyRasterStroke`はtarget Plane IDをinput roleとcanonical argumentの双方へ固定し、
  RGBA8/RGBA16をtagged exact depthのまま保持するschema v2へ進めた。accepted inputと
  bit-exact replay resultの変更なのでsemantics revision 4、`ReplayEpoch` 4、未実装の
  successor予約top-level v6を一緒に固定し、stroke v1互換decoderは残していない。
  他のM1 primitiveはschema v1のままで、production `.inkpod` v2も変更していない。
- C ABI v2へimmutable defaults query、side-effect-free EditorState query、typed
  update、exact-depth stroke begin、captured-target fill/selection/color-selectionの
  7 APIと7 fixed-layout recordをadditiveに追加した。NULL、短いtop-level/
  nested structure、未知enum、stale revision、invalid target、ownership、無副作用
  queryを公開契約で検証し、export/header/test-reference parityは175関数になった。
- Windows `CoreHost`はCore owner thread上でsession/generationを固定してquery/update
  し、`DocumentSession`にはrevision/digest付きのdeep-copied presentation cacheだけを
  保持する。同一documentの複数viewは一つのEditorStateを共有し、別sessionは分離し、
  document/view/workspace切替時は対象Coreを再queryして旧workspace値を書き戻さない。
- schema-1 canonical EditorState/EDIT frame、4 MiB bound、round-trip、digest golden、
  revision/savepoint遷移をCore-only DTOとして実装した。production `.inkpod`はexact-
  current v2のままで、`META`/`GENS`/`ASST`/`PROC`/`EDIT`のsave/reopen接続はM8へ残した。
- 代表契約は`editor_state` public integration tests、canonical codec unit tests、
  FFI EditorDefaults/EditorState contract tests、sequence/Light Table target-reconciliation
  tests、Windows CoreHost/command-state/GUI ownership smokeで固定した。
- 最終検証はRust workspace 276 test＋1 doctest、zero-warning Clippy、strict
  rustdoc、ARM64 Debug/Release fresh configure/build、ABI/GUIを含むDebug CTest
  27/27を合格した。承認済みRange ID
  `windows-arm64-apple-silicon-parallels-release-2026-08-05`でwarm-up後5回の
  medianはCore quick pan/dirty `0.851833`/`1.903334 ms`、full pan/dirty
  `13.495000`/`9.158875 ms`、native drawing `174.223125 ms`、native wheel
  `0.996097 refresh intervals/event`で、全意味counterと承認rangeを満たした。
  workload/range、revision-max式、production v2は変更していない。

### M4: Immutable Genesis と Asset Store

目的:

- procedureが外部memory/pathに依存せず、元画像を不変に保持できるようにする。

作業:

- `SolidWhite` を含むtyped immutable base-surface modelを導入する。
- content-addressed `AssetId` とasset registryをCoreへ追加する。
- canonical asset descriptorにpixel format、color/alpha semantics、dimensions、stride、content digestを持たせる。
- import raster、clipboard、Light Table sourceをcanonical asset ingestionへ移す。
- 元encoded bytes、元file名/pathを意味入力から除外し、必要ならprivacy-aware optional provenanceへ分離する。
- sample stream、vector point stream等の大きなprimitive入力をRust-owned bounded objectへ登録し、成功したcommitではinline payloadまたはimmutable assetへ確定できるようにする。
- assetの重複排除、参照count、session close時解放、最大resource量を実装する。retention rootsはGenesis、全journal branch、redo tail、既知のEDIT/EXTM参照、active transient ownerとし、current materialized stateだけを基準にGCしない。checkpointだけを唯一のretention rootにはしない。

テスト:

- 白紙、RGBA8/16、binary/grayscale、transparent imageのgenesis replay。
- blank `SolidWhite`がflat compositeではopaque underlayとして働き、layer/plane単独exportとselection maskには混入しないことを検証する。
- 新規文書としてopenした画像がGenesis base asset、既存文書へimportした画像がprocedure参照assetになることを検証する。
- 同じ内容の別path/別codec由来assetが同じcanonical digestになる条件。
- 外部fileを変更・削除しても取り込み後のreplay結果が変わらない。
- forged dimensions、stride、length、digest、asset ID、decompression workをboundedに拒否する。
- active branchから外れたbranchまたはredo tailだけが参照するassetをsave/reopen/full replay後も保持する。

完了条件:

- committed procedureがexternal pathやC++ buffer lifetimeを参照しない。
- immutable base/assetを変更するmutation pathが存在しない。

実装実績（2026-08-06）:

- Rust Coreへdistinct stable Document/Cell IDとtyped immutable base surfaceを導入し、
  blankはallocation-free opaque `SolidWhite`、raster-open documentはcanonical
  `AssetId`をGenesis baseとして保持する。baseはflat composite/exportのunderlayにだけ
  参加し、editable layer/plane、layer-only export、selection contentには混入しない。
- BLAKE3 content-addressed asset registryをsession単位で実装した。kind、pixel format、
  color/alpha semantics、dimensions、canonical stride、logical element count、length、
  payloadをboundedに検証し、最大65,536 asset、1 asset 512 MiB、session合計768 MiB、
  stream 1,048,576 elementの閉じた上限で、path/codec/provenanceに依存せず同一内容を
  deduplicateする。callerのencoded bytes、file名/path、buffer lifetimeは保持しない。
- raster-open/import、private clipboard、Light Table sourceを同じcanonical ingestionへ
  接続した。既存documentのimportはasset-only `ImportRasterAsset/v1` procedureとなり、
  `ApplyRasterStroke/v2`のsample列は4 MiB以下をinline、それを超える入力をimmutable
  sample assetとして保持する。invalid、stale、no-op、forged descriptor/ID、allocation/
  work bound failureはregistry、document、history、journal、revision、dirtyを変更しない。
- Genesis、current state、全journal commit、inactive branch/redo tail、history cache、
  floating selection、Light Table、active strokeをretention rootとして数え、unrooted asset
  だけを回収する。fresh replayはrooted graphを空registryへdeep-copyしてexpected IDで
  再ingestし、`AssetRecord`、payload、raster ownershipをlive Coreと共有しない。
- canonical procedure sliceは`SetMainLineColor/v1`、`ReplacePalette/v1`、
  `ApplyRasterStroke/v2`、`ImportRasterAsset/v1`の4件となった。Genesis identity/baseを含む
  schema/domain 4、`ReplayEpoch` 5、未実装successor top-level v7を固定し、旧epochの
  compatibility decoderは追加していない。production `.inkpod`はexact-current v2のまま
  で、asset-backed Genesisのnormal/recovery/Batch出力はM8までI/O前に拒否する。
- public Core contractは全canonical raster format、transparent hidden RGB、SolidWhite、
  caller/file lifetime、dedup、branch retention、detached replay、large stroke、clipboard、
  Light Table、no-op/stale/failure atomicity、v2 pre-I/O拒否を固定した。C ABI v2のraster/
  clipboard/Light Table owned-buffer契約とWindows raster-open/export、自動sequence attach、
  thumbnail/pane cache lifetimeを接続し、caller buffer/file解放後も結果が不変であることを
  ABI/GUI smokeで検証した。inventoryはRust 219、C ABI 175、Windows command 331である。
- snapshotはasset baseをtile単位で一度prepareし、transparent RGBA/Binary sparse baseは
  allocated coordinateだけを列挙する。初回composeでpayload accessが増え、128回の
  view-only cache hitでは0、tile revision不変となる意味ゲートを追加し、tombstone、
  negative cache、revision-max式、既存workload/rangeは変更していない。
- 最終検証はRust workspace 299 test＋1 doctest、zero-warning Clippy、strict rustdoc、
  Core quick benchmark、ARM64 Debug/Release fresh configure/build、static CRT/package、
  ABI/GUIを含むDebug CTest 27/27を合格した。承認済みRange ID
  `windows-arm64-apple-silicon-parallels-release-2026-08-05`のwarm-up後5回medianは
  Core quick pan/dirty `0.921792`/`2.149459 ms`、full pan/dirty
  `13.285625`/`9.317709 ms`、native drawing `167.134750 ms`、native wheel
  `1.080126211 refresh intervals/event`で、全意味counterと既存rangeを満たした。
  `git diff --check`も合格した。ABI v3/typed queueはM5、残存primitive移行はM6、
  `GENS`/`ASST` persistenceはM8に残し、M5を先取りしていない。

### M5: ABI v3 と Typed CoreHost Queue

目的:

- primitive control planeをvalue/ID-onlyにし、serializableなrequestをCore owner threadへ送る。

作業:

- ABI versionを上げ、Core、snapshot、task、asset、sample stream等をtype/generation付きIDで表現する。
- 公開recordはfixed-width、`struct_size`、feature flags、明示strideを維持する。
- primitive invocationをstable opcodeとversioned value recordで公開する。
- 可変payloadは事前登録したRust-owned IDだけをprimitive recordに置く。
- snapshot/thumbnail/exportはIDとbounded batched view/copy APIを使い、owner lifetimeを明示する。
- `CoreHost` のmutation work itemを任意の`std::function`からclosed typed request variantへ置換する。
- queue itemにsession/generation/base revision/target IDsを値で固定する。
- UI custom messageへraw C++/Rust object pointerを積まない既存契約を維持する。
- 旧個別FFIは一時wrapperとして新executorを呼ばせ、同じprimitiveに二つの実装を作らない。
- `include/inkpod/core_ffi.h`と`docs/ffi.md`に各ID/recordのownership、owner thread、lifetime、success/no-op/error、release責務、bulk borrow/copy期限を記録し、対応するpublic Rust APIへrustdocを付ける。

テスト:

- C11/C++20 layout、header/export/catalog parity。
- invalid/stale/wrong-type/wrong-generation/double-release/short-structure/unknown-enum。
- requestをqueue後にcaller bufferを変更・解放しても意味が変わらない。
- queue saturation、close、shutdown、active stroke時の所有権とexactly-once処理。
- `cargo doc --package inkpod-core --all-features --no-deps`をwarning deny相当で通す。

完了条件:

- mutation queueとprimitive recordにraw pointer、closure、external pathがない。
- data-plane pointerを使用する箇所が文書化されたbounded call/lifetime内だけである。
- C header、Rust declaration、FFI文書、rustdocのownership/thread/error契約が一致する。

実装結果（2026-08-06）:

- ABI v3 は Core、snapshot、task、asset、sample streamをtype/generation付きruntime IDで公開し、fixed recordとstable opcodeだけをcontrol planeへ置く。
- 可変入力はbounded callでRust所有objectへdeep-copyし、snapshot、thumbnail、exportはIDからbounded batched copyで取得する。wrong type/generation、stale、double release、short structure、unknown schema/opcode、caller buffer mutationを公開契約テストで固定した。
- WindowsのM5対象routeはissue-time contextとrequest valueだけを持つclosed `PrimitiveWork` queueへ接続した。saturation、close、shutdown、active strokeで受理件数とcompletionがexactly onceになるnative testを追加した。
- 旧個別FFIは同じcanonical executorへ委譲する。未移行routeの`LegacyInvokeWork`はM6の明示的bridgeとして残し、M6のprimitive family移行は先取りしていない。
- Rust 303 tests + 1 doctest、ARM64 Debug/Release configure/build、Debug CTest 28/28、ABI/GUI/private performance smoke、Core quick/full各5回以上、strict rustdoc、format/Clippy、`git diff --check`を完了した。Core mediansは承認済みrange内で、native smokeは全意味counterを維持した。現在の約60 Hz表示は記録済み120 Hz native wall-clock rangeと一致しないため、そのrangeを変更せず観測値として記録した。
- `.inkpod`はexact-current v2のままであり、journal/Genesis/assets/EDIT persistenceはM8に残る。

### M6: 全 Primitive Family と Win32 Thin Shell

目的:

- 全document mutationをCanonical Primitive Kernelへ通し、C++から結果を左右する意味処理を除く。

一つの巨大変更にせず、次の縦切りを順に完了する。

1. document/paper/frame、layer/plane、guide/grid、palette/main-line color
2. selection、clipboard、floating selection、transform
3. raster stroke、fill、dust、filter、effect、alpha、adjustment
4. vector path/fill/erase/connect/width/rasterize/vectorize
5. Light Table、sequence、subpalette、common-raster Import primitive
6. Batch orchestration、export/query data plane

各縦切りで行うこと:

- public Rust methodと旧FFIを新primitive executorへ委譲する。
- transaction単位のcanonical procedureを定義する。
- C++にあるdevice/document変換、floor/ceil、shape/vector geometry生成、target fallbackをCoreへ移す。
- Windows dialogはtyped initial values/resultだけを返し、Core defaultsやapplication stateを所有しない。
- C++ command handlerはissue-time contextとinput tokenをCoreへ渡し、結果をUIへ反映するだけにする。
- frontend requestはclient device-pixel inputとview ID/revisionを渡し、Coreがdocument座標へcanonicalizeしてprocedureにはdevice非依存のfixed-point値を残す。
- ingestion/decode自体はdata-plane処理でjournalへ入れず、documentを変更したImport primitiveだけをcommitする。export/queryはstateを変更しないためjournalへ入れない。
- Batchはstaged Coreごとに同じcanonical document primitiveを適用するI/O orchestrationとし、Batch controller自体をdocument primitiveにしない。
- application-owned palette/chart/preset codecがM0で対応形式として維持された場合はRust formatへ移す。維持しない場合も明示SPEC変更なしに別形式を追加しない。
- 各primitiveにsuccess/no-op/invalid/cancel/Undo/Redo/replay testを追加する。

完了条件:

- 全production document mutationが一つのstable `PrimitiveId`を持つ。
- 全production `HistoryEntry`がcanonical procedureを参照し、CoreのCommit、Undo、Redo、jump、branch cut、savepointがjournal semanticsから導出される。
- production mutationからjournal incompleteへ遷移する経路が0。
- architecture inventoryでunclassified/direct mutationが0。
- C++にpixel処理、fill/vector geometry、layer規則、history、native document/preset codecの別実装がない。

実装結果（2026-08-06）:

- 全production document mutationをstable `PrimitiveId`付きtyped
  `CanonicalInvocation`へ移行した。canonical executorはlive Coreをstaged Coreへcloneし、
  同じpublic実装をinvocation guard下で適用してexactly one document commitを検証し、
  canonical procedure、`HistoryEntry`、StateId、document revision、journal event、cache
  invalidation、高水位IDを一度にpublishする。success以外のno-op、invalid、stale、cancel、
  overflow、failureは一切publishしない。
- 全production `HistoryEntry`がretained canonical procedureを参照し、Commit、Undo、Redo、
  jump、branch cut、savepoint、inverse/COW cache release/rebuildをGenesis＋journalから導出する。
  production journalは常にcompleteで、incompleteへ遷移する経路を削除した。Light Tableの
  whole-document swapはsession Genesis replacementとしてhistoryをresetし、通常の
  `HistoryEntry`を作らない区別を維持する。
- M0 inventoryで記録したC++ composite debtを閉じた。hidden layer/all guide delete、typed
  initial layer付きnew cell、target-zero vectorize、新planeへのconverted paste、chart palette
  replacementはそれぞれ一つのRust atomic operationになり、cancel/invalidではtopology、
  history、frontend presentation stateを変更しない。
- `.inkpalette`/`.inkchart` exact-current schema 1 codecを`inkpod-format`へ移し、RGBA8/16、
  UTF-8名、4,096色、1,024-byte名、16 MiB file上限、trailing/malformed拒否、同一directory
  temporary＋flush/sync＋replaceを実装した。205-function C ABIはpalette load/saveと
  exactly-once releaseのopaque chart handleを公開し、Windowsはpathと表示状態だけを扱う。
- Windows `CoreHost`から`LegacyInvokeWork`を削除した。work variantはfixed
  `AdapterWork`/`PrimitiveWork`/`StrokeWork`/`ControlWork`だけで、adapter callable、view
  update、promise/completionはbounded token registryが所有し、Core threadがexactly onceで
  removeして実行する。queued recordにraw pointer、closure、path、STL objectを置かない。
- source-derived inventoryはRust 228、C ABI 205、Windows command 331で、unclassified/direct
  mutationは0。public契約はfamily横断replay、success/no-op/invalid/cancel、Undo/Redo、bulk
  composite、floating-new-plane、vectorize-new-layer、typed Genesis、codec bounds/overwrite、ABI
  ownership、queue saturation/close/shutdownを固定した。
- 最終検証はRust workspace 311 test＋1 doctest、ignore 0、zero-warning Clippy、strict
  rustdoc、Core quick benchmark、ARM64 Debug/Release configure/build、static CRT/package、
  ABI/GUIを含むDebug CTest 28/28、Release private performance smoke、`git diff --check`を合格した。
  warm-up後5回medianはCore quick pan/dirty `0.853208`/`2.156208 ms`、full pan/dirty
  `14.913708`/`9.583542 ms`、native drawing `192.690000 ms`、native wheel
  `1.007763 refresh intervals/event`で、全意味counterと変更していない承認済みrangeを満たした。
  一回だけdrawingのreplaceable publicationが592でなく593だったが、8 ms preview schedulerの
  置換可能snapshot観測であり、revision/checksum/sample/final Presentは不変だった。
- production `.inkpod`はexact-current v2のままである。cross-architecture bit-exact hardeningは
  M7、canonical invocationのpersistent codecと`GENS`/`ASST`/`PROC`/`EDIT` cutoverはM8に残し、
  いずれも先取りしていない。

### M7: 決定性 Hardening と Replay Gate

目的:

- Procedure Call Historyをformatの正本にできるだけの決定性を証明する。

作業:

- document coordinate、diameter、transform、curve、angle、pressure等のcanonical fixed-point表現を適用する。
- half-open bounds、pixel-center、nearest rounding、alpha、premultiplication、color distanceを一か所へ集約する。
- `exp`、`powf`、`sin/cos` 等の画像結果へ影響するplatform mathを監査し、決定的lookup/table/integer algorithmまたはbit-exact実装へ置換する。
- hash iteration、thread count、tile scheduling、SIMD、x64/ARM64で結果が変わらないようにする。
- primitive catalog digestとreplay epochをbuild/format contractへ接続する。
- Core canonical compositeのbit-exact checksumを公開test helperではなく既存public snapshot/outputから観測する。

テスト:

- 同一fixtureをx64/ARM64/Linux/macOSでreplayし、全procedure境界のstate digestと最終pixel checksumを比較する。
- quick/full benchmarkで同じscenario/counter/checksumを使用する。
- fixed-seed property/state-machine testに失敗時replay情報を残す。

完了条件:

- 対応architectureのgolden replayがbit-exact。
- 未監査の画像結果依存floating/transcendental pathが0。
- primitive semantics変更時にepoch/versionを上げない変更をCI guardが拒否する。

完了記録 (2026-08-06):

- `inkpod-image::canonical`をfixed numeric authorityとし、IEEE-754からQ16/u16/turnsへの
  exact変換、ties-to-even、integer sqrt、CORDIC、alpha/premultiplication、color distance、
  fixed log2/exp2を集約した。Gaussian、Levels、gradient、airbrush/stamp、Light Table、
  floating selection、ABI angle constraintの画像結果依存transcendental pathをinteger/fixed
  algorithmへ置換し、Core/image/FFI production sourceの再導入guardを追加した。
- 全production typed invocationをcanonical schemaへ正規化し、76-entry closed primitive
  catalog、catalog digest、canonical-numeric version 1、replay epoch 6、reserved successor
  version 8を一つのbuild contractとして公開した。production `.inkpod`はexact-current v2の
  ままで、v8 reader/writerとpersistent invocation codecは実装していない。
- public contract fixtureはGenesisと5 procedure後の全6 state digest、各境界のfresh replay、
  最終immutable snapshot composite digestを固定した。同じgoldenはnative Windows ARM64と
  `x86_64-pc-windows-msvc`でbit-exactに通過し、Linux/macOS CI matrixも同一testを実行する。
- C ABIは207 exportとなり、caller-owned replay contractとsnapshot digest queryを追加した。
  `CoreHost`、ABI smoke、GUI smoke、renderer sinkで実製品のCore/Windows経路へ接続した。
- Rustは321 testsと1 doctest、zero ignored、fmt、all-target/all-feature Clippy `-D warnings`、
  strict rustdoc、8-scenario quick benchmarkを通過した。Core quick/fullはwarm-up後各5回で、
  pan/dirty中央値が`0.877375/1.930792 ms`と`13.382959/9.206375 ms`、canonical replayが
  `1.091042/1.391250 ms`となり、既存rangeと全semantic gateを変更せず満たした。
- Windows ARM64 Debug/Releaseのfresh configure/buildと最終relink、static CRT、portable ZIP、
  unsigned MSIXを通過した。最終Debug CTestは28/28（ABI 27.21秒、GUI 177.70秒、全体
  212.90秒）、Release private performance smokeはexit 0、`git diff --check`も通過した。

### M8: 次期 `.inkpod` Format Cutover

目的:

- `META/GENS/ASST/PROC/EDIT` を正本とするcurrent-only native formatへ切り替える。

作業:

- exact byte layout、section order-independent directory、BLAKE3 digest、limitsを`docs/file-format.md`へ先に固定する。
- Rust format DTOをCore runtime typeから分離する。
- staged decode/validate/replayとstreaming atomic saveを実装する。
- normal save、autosave、recovery、revert、partial revertの新journal/savepoint semanticsを接続する。
- normal saveはprospective document/editor savepointをfileへencodeし、replace成功後だけlive Coreへ公開する。
- recovery saveは通常savepoint/pathを進めない。recovery openはpathlessかつdirtyなsessionとして復元し、以前の通常保存先へのauthorityを暗黙に継承しない。
- partial revertは保存済みjournal stateをstaged Coreで再構成し、選択対象を一件の新しいundoable canonical procedureとしてcommitする。
- open後にhistory list、cursor、redo availability、non-active branch、EditorState、ID high-watermarkを復元する。
- 現行v2とそれ以前/以後を明示的に拒否し、migration/compatibility readerを作らない。
- schema変更のたびにtop-level versionを上げる。
- current format decoder、section directory、journal/replay parserのcoverage-guided fuzz targetをcutoverと同時に追加する。

テスト:

- genesisからのfull replayとsave直前live Coreの全意味状態/pixel checksum一致。
- save/reopen後のUndo/Redo/jump/branch cut/document/editor savepoint/dirty/EditorState/次stable-object・Procedure・State・JournalEvent・Branch ID一致。
- runtime `DocumentRevision`は新Core generation内へrebaseされ、persistent procedure preconditionが`StateId`だけで再現される。
- missing/duplicate/reordered/overlapping section、unknown critical、opaque optional round-trip。
- bad digest、truncation、oversize、operation bomb、asset bomb、ID collision/cycle/reference error。
- cancel/allocation/write/replace failureでCoreと既存fileを変更しない。
- normal save失敗でstaged/live savepointが進まず、成功fileはreopen直後cleanになる。EditorStateだけがdirtyな場合も同じ契約を満たす。
- recovery save/open、partial revertのsuccess/cancel/failureでpath authority、dirty、history、journal、savepointが上記規約どおりになる。
- v2拒否をcurrent-version policyとして明示testする。

完了条件:

- `.inkpod` にmaterialized final rasterだけでは文書を成立させられず、Genesis/Assets/Proceduresから再構成できる。
- reopen直後のUndo/Redoを含む利用者向け縦切りがWindows UIから通る。
- `IO-001`、`HIST-001`、`ABI-001`の新契約がtestと文書で追跡される。

完了記録 (2026-08-07):

- production `.inkpod`をexact-current version 8へ切り替え、128-byte header、order-independent
  section directory、required `META/GENS/ASST/PROC/EDIT`、section/file BLAKE3-256、8-byte
  alignment、zero padding、opaque optional section round-trip、bounded current-only decodeを
  `inkpod-format`へ実装した。v2を含む全non-current versionとreserved `CKPT`を拒否し、
  migration、checkpoint、compressionは追加していない。
- CoreはGenesis、rooted asset graph、全Commit/HistoryMove/BranchCut、76種のtyped canonical
  invocation、cursor/inactive branch、document/editor savepoint、EditorState、stable-object・
  Procedure・State・JournalEvent・Branch high-watermarkを保存し、open時にstaged fresh replayと
  final digest照合後だけlive sessionを置換する。runtime `DocumentRevision`は新generationへ
  rebaseし、Undo/Redo/jump/branch continuationとID非再利用をpublic contractで固定した。
- normal saveはprospective document/editor savepointをencodeし、同一directoryのchunked
  temporary write/flush/close/replace成功後だけpathと両savepointを公開する。autosaveは通常
  authorityを進めず、recovery openはpathlessかつdirty、partial revertとBatch outputも同じ
  v8 reader/writerへ接続した。cancel、corruption、non-current、replace failureは既存fileと
  live Coreを変更しない。
- Windowsの既存CoreHost/ABI save/open経路をv8へ接続し、通常save直後の両dirty解除、
  EditorState復元、asset-backed Genesis、Light Table、sequenceのsave/reopenをGUI smokeで
  通した。`native_v8`と`native_core_v8`のcoverage-guided fuzz targetを追加し、両targetの
  buildを確認した。
- Rustは330 tests、zero ignored、fmt、all-target/all-feature Clippy `-D warnings`、strict
  rustdoc、8-scenario quick benchmarkを通過した。Core quick/fullはwarm-up後各5回で、
  pan/dirty中央値が`0.918291/1.781125 ms`と`12.343584/8.582375 ms`、canonical replayが
  `1.082333/1.204709 ms`となった。quick dirtyの下限未満は32 edit/32 rebuild/224 reuseと
  checksumが一致する診断上の高速化であり、range、tolerance、workloadは変更していない。
- Windows ARM64 Debug/Releaseをfresh configure/buildして各111 target、static CRT、portable
  ZIP、unsigned MSIXを通過した。最終Debug CTestは28/28（ABI 24.80秒、GUI 176.88秒、全体
  208.27秒）、Release private performance smokeはwheel 512 event/512 Presentとdrawing
  16 stroke/544 sample/16 Presentを保持してexit 0、`git diff --check`も通過した。

### M9: Checkpoint・性能・安全性・旧経路削除

目的:

- 大規模journalを実用速度とbounded resourceで扱い、移行用実装を残さない。

作業:

- `CKPT` を追加し、prefix digest/state digestでfull replayとの同値を検証する。
- checkpoint作成間隔をprocedure countだけでなくreplay work/dirty bytesで決める。
- file/section/asset/procedure payloadをstreaming処理し、最大file全体の二重memory保持を避ける。
- content-addressed assetのdedup、chunking、必要ならversioned compressionをbenchmark後に追加する。
- explicit compactionは新しいGenesisへ書き出す別操作とし、history lossを事前表示する。自動squashはしない。
- M8で導入したcoverage-guided fuzz corpusを拡張し、operation-cost、長大branch、asset retention、checkpointを対象にしたfuzz/property testを追加する。
- 旧`CellFile` final-state正本、旧history-only表現、旧mutating FFI implementation、C++ tool semantics/native codecを削除する。
- `docs/architecture.md`、`docs/ffi.md`、`docs/file-format.md`、`docs/compatibility.md`、`docs/implementation-status.md`を実装事実に合わせて更新する。

テスト/検証:

- checkpoint openとfull replayの全state/pixel/history digest一致。
- checkpoint sectionのhash/構造/bound違反はfile reject、構造上有効なprefix/state digestまたはepoch不一致はfull replay fallbackとなることを検証する。
- large sparse document、長いstroke、100万件級を含む設定済み上限近傍のbounded benchmark。
- 連続zoom/pan snapshotは変更のないtile payload/revisionを再利用し、source raster byte数に比例するcopy/hashを行わないことをquick/full benchmarkで継続検証する。snapshot validation本体とrevision-max helperのcall graphをsource-lockし、初回composeでは正に増えるpayload access counterが128回のcache-hit zoom snapshotでは0のままになることを、wall-clockとは独立した意味ゲートとして常時要求する。
- 保護対象はquick/fullで2,048/8,192 pairを測る`pan_zoom_snapshot`、同一allocated tileへの1 pixel edit＋snapshot rebuildを32/128回測る`dirty_tile_rebuild`、および1024平方・256 allocated tileの実Canvasで512 wheel eventを各1 Presentまでと16本/544 sampleのmulti-tile strokeを各1 Presentまで測るprivate native performance smokeとする。
- 日常のwall-clock検証は`docs/core-benchmark-baseline.md`の承認済み環境別reference envelopeを使う。一回以上warm-upして5回以上の中央値を比較し、下限未満は処理省略を疑う診断値にだけ使い、意味ゲートが正常な高速化は拒否しない。上限を超えた場合は独立した5回以上を再測定し、両方の中央値が上限を超えたときだけ回帰として拒否する。native wheelは固定nanosecond値ではなく、記録したdisplay refresh interval当たりの時間へ正規化する。
- detached旧revision-max buildとのA/Bは、workload/harness変更、reference環境追加・変更、envelope再設定、または境界結果の明示監査時に限る。envelopeは自動的に緩和せず、変更理由、環境、全sample、意味counterを記録してユーザーの明示承認を得る。
- cargo fmt/clippy/test/bench/doc、Windows configure/build/CTest、ABI/UI/renderer smoke。
- x64/ARM64 replay、save/open、recovery、queue saturation、shutdown raceの反復test。

完了条件:

- checkpointを全削除しても同じ文書を再構成できる。
- journal/asset/checkpointのmemory、disk、CPU workが定義済み上限内。
- 旧mutation/file/history正本へのproduction参照が0。
- 現在状態、既知差分、代表検証がstatus/compatibility文書へ反映される。

## 8. 実装中の不変条件

- 一つのprimitiveに旧実装と新実装の二つの意味ownerを置かない。移行中の旧APIは新executorへ委譲する。
- format、ABI、Core、Windowsを一度に全面置換せず、各milestoneの小さい縦切りを常にbuild/test可能に保つ。
- no-op、invalid、cancel、stale、overflow、allocation failureはdocument、journal、history、ID、dirty、revisionへ部分結果を残さない。
- generated IDはtransaction commit時だけ消費し、削除後も同一document namespaceで再利用しない。
- procedure replayはexternal path、clock、locale、OS entropy、thread count、hash order、GPU stateへ依存しない。
- file decode/replayはlive Coreを直接変更せず、全成功後に一回だけ公開する。
- checkpoint、inverse delta、COW snapshot、render snapshotは最適化であり、意味上の正本へ昇格させない。
- schemaまたはreplay semanticsを変えた変更は、フォーマットフリーズ前のtop-level version更新を同じ変更に含める。
- 既存のユーザー変更、対象外refactor、無関係formattingを混ぜない。

## 9. 全体の完了条件

本再設計を完了扱いできるのは、次をすべて満たす場合だけである。

- Win32の全document mutationがtyped requestとして一つのRust primitiveへ到達する。
- C++にauthoritativeなdocument/tool default、画像処理、geometry、history、native format実装がない。
- primitive control planeは固定値またはRust-owned IDだけで構成される。
- `.inkpod` のauthoritative dataはGenesis、Assets、`PROC`内のProcedures/History control events、`META`内のHistory state、EditorStateであり、独立`HIST` sectionを持たない。
- save/reopen後に同じ画像、全persistent ID high-watermark、history cursor、Undo/Redo、redo branch、document/editor savepoint、dirty、EditorStateを復元する。
- checkpointなしのfull replayと通常openがbit-exactに一致する。
- 同一epochのx64/ARM64/非Windows replayがcanonical state/pixel checksumで一致する。
- malformed、resource bomb、cancel、failure、stale、shutdown raceで部分commitや既存file破壊がない。
- current-version-only policyがtestされ、v2 compatibility reader/migration/shimが存在しない。
- `SPEC.md`、architecture、FFI、file-format、compatibility、implementation-statusが実装と一致する。
- AGENTS.mdの該当するRust、CMake、MSVC、CTest、benchmark、rustdoc検証が成功し、未実行事項が明記される。
