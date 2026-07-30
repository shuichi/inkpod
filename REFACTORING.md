# Rust Core リファクタリング計画

## 文書情報

| 項目 | 内容 |
| --- | --- |
| 状態 | Proposed |
| 対象 | `rust/inkpod-core` |
| 基準日 | 2026-07-30 |
| 仕様の正本 | `AGENTS.md`、`PROMPT.md`、既存のテスト済み契約 |
| 実施方針 | 各マイルストーンを独立してレビュー、検証、ロールバック可能な変更単位にする |

この文書は Rust Core の内部品質を段階的に高めるための実施計画である。製品機能の追加計画ではなく、現在の利用者向け挙動、C ABI、native file format、履歴契約を維持しながら、安全性、決定性、型安全性、性能の観測可能性、保守性を改善することを目的とする。

## 1. 目的

最終的に、Rust Core を次の状態へ移行する。

- 同じ初期状態と入力列から同じ結果になることを、決定的な state-machine/property test で継続的に検証できる。
- success、no-op、invalid、cancel、stale revision、Undo/Redo の状態遷移を、共通の観測方法で比較できる。
- document 編集の clone、validation、revision、history commit、cache invalidation が一つの内部 transaction abstraction に集約される。
- layer、plane、view、history state、revision など、意味の異なる `u64` を内部で誤って混同できない。
- document、view logical、device の座標と寸法が型で区別され、変換の向きと単位が API から読み取れる。
- 大文書、snapshot、dirty tile、Undo/Redo、light table、Batch の代表的コストを再現可能な Core benchmark で測定できる。
- 公開 Rust API の単位、範囲、ownership、error、revision/history への影響が rustdoc から分かる。
- 大きな source file が責務単位に分割され、crate root と各 `mod.rs` は小さな module index に保たれる。

テスト数やファイル行数そのものは目標にしない。重複を増やさず、契約と責務が明確になることを優先する。

## 2. 非目標

以下は本計画に含めない。必要になった場合は別計画、別変更として扱う。

- Windows UI、renderer、C++ adapter の機能追加または再設計
- C ABI version の更新、公開 C header の型変更、関数の追加・削除
- `.inkpod` manifest、blob、migration、外部画像形式の仕様変更
- 既存の公開 Rust API を一括して破壊する型変更
- benchmark の根拠がない性能最適化
- production dependency の追加
- Core と無関係な crate の大規模整理
- proprietary format の推測実装
- 機能変更、名前変更、module 移動を同じ差分で大量に行うこと

## 3. 実施原則

### 3.1 契約を先に固定する

内部構造を変更する前に、公開 API から観測できる結果をテストで固定する。private field を直接比較するだけのテストをリファクタリングの主な安全網にはしない。private helper の局所的不変条件だけを implementation file 内の `#[cfg(test)] mod tests` で検証する。

### 3.2 一度に一種類のリスクだけを取る

transaction 導入時には ID 型の変更を混ぜず、ID 型の導入時には module 分割を混ぜない。機械的変更と意味上の変更を分け、失敗した場合に原因を一つのマイルストーンへ絞れるようにする。

### 3.3 境界は固定幅のまま維持する

C ABI、file DTO、既存の公開 Rust API は当面 `u32`、`u64`、`i32`、`f32`、`f64` 等の既存表現を維持する。newtype は Core 内部へ導入し、境界で明示的に変換する。公開 Rust API の typed facade が必要になった場合は、本計画完了後に別途設計する。

### 3.4 再現可能性を優先する

property test は固定 seed と失敗時の replay 情報を持たせる。benchmark は quick/full の入力規模を固定し、wall-clock time だけで pass/fail を決めない。

### 3.5 各変更を常に出荷可能に保つ

各マイルストーン、可能なら各移行 wave の終了時に format、clippy、workspace test、対象 platform の build/test を通す。途中状態の互換 shim は、次のマイルストーンへ持ち越さず、同じ wave 内で削除する。

## 4. 現状の基準線

2026-07-30 時点の構造を本計画の基準線とする。

- `rust/inkpod-core/src` は約 14,500 行である。
- Core は `#![forbid(unsafe_code)]` を維持している。
- Core の production dependency は workspace 内の `inkpod-format` と `inkpod-image` だけである。
- private invariant test は対象 implementation file に colocate され、公開 workflow test は `rust/inkpod-core/tests/contracts` の integration target に分離されている。
- 直近の記録では Core は 83 tests、workspace 全体は 148 tests と doc-tests、platform-independent CTest は 5/5 が通過している。テスト数の増減ではなく、契約の維持を判定基準にする。
- Core 専用 benchmark はまだなく、`inkpod-image` に `harness = false` の `large_document` benchmark がある。
- CI は Rust format、zero-warning clippy、workspace test、image benchmark、および Windows の CMake build/CTest を実行している。
- document 編集では、`CellDocument` の before/after clone と `commit_document_edit` または `commit_document_edit_with_revision` の呼び出しが複数 module に分散している。
- `LayerNode`、`PlaneNode`、active IDs、revision、history state、view ID 等は、意味が異なっても内部で主に `u64` を共有している。
- `PointF32` や `CoordinateSpace` は存在するが、`ViewState` の pan、viewport、`device_to_document` の引数と戻り値等には raw scalar/tuple が残る。

特に大きい source file は次のとおりである。行数は分割の優先順位を考えるための観測値であり、上限ではない。

| File | おおよその行数 | 現在混在している主な責務 |
| --- | ---: | --- |
| `src/vector/operations.rs` | 1,071 | path/fill 編集、選択、rasterize、vectorize、thumbnail |
| `src/batch/codec.rs` | 788 | enum code、operation/filter payload、reader/writer |
| `src/transform.rs` | 746 | document transform、raster transform、frame/guide transform、数値 helper |
| `src/view.rs` | 732 | guide/grid、secondary view、locator、shortcut、view command、座標変換 |

## 5. 全体ロードマップ

```text
M0 基準線と安全網の定義
 └─ M1 決定性・state-machine/property test
     └─ M2 Core benchmark と性能基準線
         └─ M3 document transaction の集約
             └─ M4 stable ID・revision の newtype 化
                 └─ M5 座標・寸法の型付け
                     └─ M6 公開 API rustdoc の完成
                         └─ M7 責務単位の module 分割と最終整理
```

M1 と M2 が後続変更の安全網になるため、M3 以降を先行させない。M4 と M5 は対象型が異なるため別マイルストーンにし、レビュー差分を抑える。M7 の物理的な file 移動は最後に行い、先に意味上の境界を固める。

推奨する変更単位は次のとおりである。

| Milestone | 推奨変更数 | 主なリスク |
| --- | ---: | --- |
| M0 | 1 | 基準線の誤認 |
| M1 | 2–3 | flaky test、観測不足 |
| M2 | 1–2 | noisy benchmark、過大 fixture |
| M3 | 4–6 | history/revision/no-op semantics の変化 |
| M4 | 4–6 | ID 変換漏れ、境界 drift |
| M5 | 3–5 | 座標系、flip、rounding の変化 |
| M6 | 2–4 | documentation lint の一括導入による巨大差分 |
| M7 | 4 | module visibility、source tracking の漏れ |

## 6. Milestone 0: 基準線と安全網の定義

### 目的

後続差分を評価するための、再現可能な開始点と変更規律を確立する。

### 作業項目

- [ ] `cargo test --package inkpod-core --all-features -- --list` の結果を監査し、公開 workflow、architecture、resilience、private invariant の分類を確認する。
- [ ] Core の公開 mutation API を、document edit、view-only edit、preview transaction、long-running/cancellable operation、persistence、sequence state に分類する。
- [ ] document revision、view revision、history cursor、dirty/savepoint、render cache、stable ID allocator の現行挙動を表にする。
- [ ] `before`/`after` clone、direct document assignment、history commit、render cache clear の call site を inventory 化する。
- [ ] M1 以降で使う共通観測値 `CoreObservation` の項目を設計する。
- [ ] `docs/implementation-status.md` と `docs/compatibility.md` は、現在状態または代表的検証が実際に変わる場合だけ更新する。

### `CoreObservation` の候補

integration test から public API だけで組み立て、少なくとも次を含める。

- `DocumentInfo`
- `layers()` が返す layer/plane topology と stable IDs
- `history_entries()` と現在の Undo/Redo 可否に相当する情報
- `ViewState`
- `build_snapshot()` の revision、feature flags、tile 座標、tile revision、pixel checksum
- vector path/fill の公開情報
- guide、grid、light-table set/item の公開情報
- 対象 workflow に応じた selection bounds、palette、main-line color

一つの巨大な万能 snapshot にせず、共通部分と機能別 extension に分ける。snapshot の cache revision のように同値性と実装詳細の境界にある値は、「同一 Core 間の操作前後比較」と「二つの Core の決定性比較」のどちらで使うかを明記する。

### 完了条件

- 現行 public contract を変える production code の変更がない。
- 後続マイルストーンで比較する状態、操作分類、検証コマンドが文書化されている。
- inventory の各 call site が M3 の migration wave のいずれかに割り当てられている。
- 基準線の全検証が通過し、失敗がある場合は既存不具合として本計画と分離されている。

## 7. Milestone 1: 決定性・state-machine/property test

### 目的

内部実装を変えても、同じ状態と入力が同じ結果を返し、失敗や cancel で部分状態が残らないことを自動検証する。

### 1A. deterministic state-machine harness

`rust/inkpod-core/tests/contracts` に public API 専用の state-machine test module を追加する。test-only の private bridge は作らない。

モデルは次の要素を持つ。

- 固定 UUID、固定 document size、固定 DPI の初期状態
- 現在存在する layer/plane/view/light-table IDs の pool
- 実行可能な操作を生成するための軽量な abstract state
- 実際の `Core` から得る `CoreObservation`
- seed、step index、操作列を失敗メッセージに出す replay 表現

生成対象は、最初から全 API に広げず次の順で増やす。

1. layer/plane create、duplicate、reorder、property、delete
2. guide/grid と view command
3. selection new/add/subtract/intersect、invert、clear、resize
4. paint/fill の小さな bounded raster 操作
5. vector path/fill の追加、編集、選択
6. light-table set/item 操作
7. Undo/Redo、history jump、Undo 後の新規編集

各生成操作は、valid、intentional no-op、intentional invalid のいずれかを明示する。任意の不正 byte 列や巨大 allocation を同じ generator に混ぜず、それらは resilience test に残す。

### 1B. 検証する property

- **Determinism:** 同じ fixed seed と操作列を二つの `Core` に適用すると、各 step の result class と `CoreObservation` が一致する。
- **Failure atomicity:** `Err` を返した操作の前後で、document、history、dirty、revision、snapshot 内容が変わらない。
- **Cancel atomicity:** stroke、filter preview、floating selection、cancellable operation の cancel 後に base observation が復元される。
- **No-op stability:** no-op は document revision、history、dirty、render content を不要に変更しない。
- **Undo/Redo round-trip:** 成功した一つの document edit は一回の Undo で直前状態へ戻り、一回の Redo で同じ状態へ進む。
- **Redo branch truncation:** Undo 後の新規 edit で以前の redo branch が消える。
- **Revision separation:** view-only edit は document revision/history/dirty を変えず、document edit は必要な cache invalidation を起こす。
- **Savepoint semantics:** 通常 save は savepoint を進め、autosave/recovery/export は通常 savepoint を進めない。
- **ID integrity:** 生成された stable ID は生存 object 間で重複せず、layer ID と plane ID の参照関係が壊れない。

### 1C. generator の実装方針

- shrinking と replay を利用できる成熟した property-test crate を `dev-dependencies` に限定して採用する。
- dependency 追加前に license、minimum supported stable Rust、default feature、transitive dependency を確認する。
- CI は固定 seed 群と bounded case count を使う。OS entropy や実行順へ結果を依存させない。
- 一つの case は小さい document と上限付き操作数を使い、通常の workspace test の時間を著しく増やさない。
- 重い case は ignored test に隠さず、後述の benchmark または明示的な full profile に分離する。

### 1D. private helper の colocated unit test

state-machine test と並行して、次の pure/private helper の境界値を implementation file 内で補強する。

- `view.rs`: device/document 正逆変換、flip、極端な zoom、非有限値、viewport 境界
- `transform.rs`: anchor offset、frame/guide の mirror/rotate/scale、checked conversion の境界
- `snapshot.rs`: dirty tile 再利用、tile revision、overlay と document revision の分離
- `document/validation.rs`: layer/plane topology、必須 plane、重複 ID、不正 active ID
- `animation`: light-table ordering、reference-frame alignment、sequence natural order の private invariant

既存の public workflow test と同じシナリオを private field で書き直さない。

### 完了条件

- 固定 seed を用いた test を連続して複数回実行しても結果が変わらない。
- 失敗時のログだけで seed と操作列を再実行できる。
- 上記 property が最低一つの success、no-op、invalid sequence を含む。
- test のために production public API または C ABI を拡張していない。
- `cargo test --package inkpod-core --all-features` の実行時間増加を記録し、通常 CI に不適切な増加があれば case size を分離する。

## 8. Milestone 2: Core benchmark と性能基準線

### 目的

transaction や newtype の導入前に、Core workflow の時間、処理量、cache 再利用を観測できるようにする。

### 2A. benchmark target

`rust/inkpod-core/Cargo.toml` に `harness = false` の Core benchmark target を追加し、`rust/inkpod-core/benches/core_workflows.rs` を作成する。既存 `inkpod-image/benches/large_document.rs` と同じく、追加の benchmark framework を必須にしない。

CLI は少なくとも次を持つ。

- `--quick`: CI 用の bounded input
- default: 開発者が before/after を比較する full input
- 固定された human-readable 一行出力
- scenario ごとの elapsed time と意味上の counters

### 2B. scenario

| Scenario | 測定対象 | wall-clock 以外の検証値 |
| --- | --- | --- |
| sparse document snapshot | 大寸法、少数 tile の initial snapshot | allocated/rendered tile count、checksum |
| dirty tile rebuild | 一 tile 編集後の snapshot | 再生成 tile 数、未変更 tile revision の維持 |
| pan/zoom snapshot | view-only 変更時の cache 利用 | document revision 不変、render content checksum |
| Undo/Redo | 複数の小編集と履歴移動 | history entries、最終 checksum、dirty state |
| light table composite | 複数参照の整列と合成 | reference count、tile count、checksum |
| vector snapshot | path/fill の rasterization | segment/fill count、tile count、checksum |
| Batch dry-run/preview | graph validation と複数入力 preview | success/failure count、出力非生成 |

native save/open は filesystem と compression の影響が大きいため、Core workflow benchmark へ混在させない。必要なら format benchmark として別 target にする。

### 2C. 判定方法

- checksum、revision、tile count、history count 等の意味上の結果は hard assertion にする。
- shared CI runner の wall-clock absolute threshold は pass/fail に使わない。
- before/after は同じ machine、同じ build profile、同じ scenario parameter で比較する。
- regression 判定は複数回の中央値を基本とし、改善作業では raw output を変更記録へ添付する。
- allocation count を測れない段階では、allocated tile count と clone/COW semantics を proxy として使う。

### 2D. CI 接続

- Linux の Rust job へ `cargo bench -p inkpod-core --bench core_workflows -- --quick` を追加する。
- quick mode は correctness assertion と smoke performance の役割に限定する。
- full benchmark は release build で手動実行し、最適化を伴う変更の before/after に使用する。

### 完了条件

- quick/full が同じ scenario と出力 schema を使う。
- quick benchmark が CI 時間を過度に増やさない。
- 各 scenario が処理結果を `black_box` へ渡すだけでなく、意味上の assertion を持つ。
- M3 以降の変更で比較可能な基準出力が保存されている。
- benchmark 導入だけの段階では、計測に基づかない production optimization を行っていない。

## 9. Milestone 3: document transaction の集約

### 目的

分散している before/after clone、revision allocation、no-op 判定、history commit、render cache invalidation を、一つの内部 abstraction と共通 test に集約する。

### 3A. transaction contract の定義

内部型は仮に `DocumentEdit` とする。最終名称は実装時に既存語彙と合わせるが、少なくとも次を型として保持する。

```rust
pub(super) struct DocumentEdit {
    before: CellDocument,
    working: CellDocument,
    base_revision: u64,
    commit_revision: u64,
}
```

必要な操作は次に限定する。

- 現在 document から開始する。
- `working` だけを可変参照として編集する。
- 開始時の `base_revision` と現在値を commit 前に比較し、stale base を拒否する。
- raster/tile へ付与する revision と最終 document revision に同じ `commit_revision` を使う。
- consume して一度だけ commit する。
- before/working が同一なら既存 no-op semantics を返す。
- success 時だけ document、revision、history、cache を更新する。

`Drop` で暗黙 commit しない。明示的な `commit` だけを許可する。panic recovery を transaction の責務にせず、Core は引き続き safe Rust と FFI 境界の panic containment に依存する。

最初の abstraction は behavior-preserving にする。特に、失敗した操作が現在どの時点で ID を消費するかを M1 で観測し、transaction 導入と同時に黙って変更しない。ID allocator 自体の atomicity を変える場合は、明示した契約変更として別 wave にする。

### 3B. transaction 自体の unit test

`src/core.rs` または新しい `src/transaction.rs` に colocate し、次を検証する。

- unchanged working document は no-op で history/revision/cache を変えない。
- changed working document は一 revision、一 history entry だけ進める。
- stale revision は document/history/cache を変えない。
- revision/history-state overflow は部分 commit しない。
- Undo 後の commit は redo branch を切る。
- transaction の before/working を取り違えて commit できない。

### 3C. migration wave

| Wave | 対象 | 含める操作 | 除外する操作 |
| --- | --- | --- | --- |
| 3.1 | `document/operations.rs` | layer/plane create、duplicate、delete、reorder、property | convert/merge |
| 3.2 | `document/operations.rs`、`animation/light_table_operations.rs` | convert/merge、単純な light-table metadata edit | swap、外部 raster reload |
| 3.3 | `view.rs`、`paint.rs` | guide/grid、palette、main-line color | view-only command、fill/stroke |
| 3.4 | `selection/operations.rs`、`transform.rs` | 即時 selection edit、document mirror/rotate/resize | floating paste、cancelable operation |
| 3.5 | `effects/operations.rs`、`vector/operations.rs` | 即時 effect/vector edit | filter preview、長時間 rasterize/vectorize |

stroke、filter preview、floating selection、Batch execution、progress/cancellation を持つ処理は、単純 transaction の移行完了後に別設計として評価する。base state を長期間保持する session transaction と、一回の同期 document edit を同じ型に押し込まない。

### 各 wave の検証

各 public operation について最低限、次を既存または追加 test で確認する。

- success が一つの history entry になる。
- no-op が revision/history/dirty を進めない。
- invalid input が observation を変えない。
- Undo/Redo が exact round-trip する。
- active preview/stroke 中は既存どおり拒否される。
- snapshot cache invalidation が過不足なく行われる。

### 完了条件

- 移行対象 module に hand-written の before/after pairing が残っていない。
- commit の実装箇所が transaction abstraction 内へ集約されている。
- public Rust API、C ABI、file DTO に変更がない。
- M1 の state-machine/property test と M2 の benchmark result が契約上同じである。
- preview/cancellation 系の未移行箇所が、意図した除外として一覧化されている。

## 10. Milestone 4: stable ID・revision の newtype 化

### 目的

意味の異なる ID、revision、history state の取り違えを compile time に防ぐ。境界表現は固定幅整数のまま維持する。

### 4A. 型の設計

最初から一つの汎用 `StableId(u64)` にまとめない。最低限、次の意味ごとに分ける。

```text
DocumentId       LayerId          PlaneId
LightTableSetId  LightTableItemId ViewId
HistoryStateId   DocumentRevision ViewRevision
RenderRevision   PreviewRevision
```

型は原則として次を満たす。

- `#[repr(transparent)]` を付けられる単一 fixed-width value
- `Copy + Clone + Eq + Ord + Hash + Debug`
- raw value の取得は意味が分かる境界 method に限定
- checked increment または専用 allocator を持つ
- 異なる ID 型同士の `From` を実装しない
- zero が sentinel か valid value かを型ごとに文書化

公開 API と file DTO では、既存の raw integer を受け取り、Core 入口で対応 newtype に変換する。戻り値は境界で raw integer に戻す。C ABI layout を newtype の representation に暗黙依存させない。

### 4B. migration slice

| Slice | 対象 | 主な file |
| --- | --- | --- |
| 4.1 | `LayerId`、`PlaneId` | `document/model.rs`、`document/operations.rs`、history、selection、paint |
| 4.2 | vector が保持する plane/path/fill 関連参照 | `vector/model.rs`、`vector/operations.rs` |
| 4.3 | light-table set/item、sequence 関連 ID | `animation/*` |
| 4.4 | `ViewId` と secondary view map | `view.rs`、`core.rs` |
| 4.5 | document/view/render/preview revision、history state | `core.rs`、`history.rs`、`snapshot.rs`、effects preview |
| 4.6 | format DTO と public API の conversion audit | `document/model.rs`、`persistence.rs`、`api.rs` |

path ID、fill ID、guide ID 等も inventory に含めるが、一つの slice が大きくなる場合は別 slice とする。

### 4C. 変換規則

- Core 内部の map key、field、helper parameter は newtype に統一する。
- public method の raw argument は method 冒頭で一度だけ変換する。
- file decode は validation 後に newtype 化し、encode は一か所で raw value 化する。
- `as u64` の散在を許さず、`get()` または named conversion を境界 module に集約する。
- error text では `Debug` の wrapper 表現ではなく、利用者が照合できる raw value を表示する。

### 完了条件

- Core 内部で layer ID を plane ID 引数へ渡すコードが compile しない。
- 対象 slice に raw `u64` ID field が残る場合は、境界または明示した例外として説明されている。
- native save/open round-trip で ID 値が変わらない。
- C header、FFI layout、file schema に差分がない。
- state-machine test が ID uniqueness、reference integrity、Undo/Redo 後の ID 安定性を検証する。

## 11. Milestone 5: 座標・寸法の型付け

### 目的

document、view logical、device pixel の座標を型で区別し、zoom、pan、flip、rounding の誤適用を防ぐ。

### 5A. 導入候補

```text
DocumentPointF64   DocumentPointI32   DocumentRectI32
DevicePointF64     DeviceSizeF64      ViewLogicalOffset
DocumentSizeU32    ZoomFactor
```

既存の `TileCoord`、`RectI32`、`PointF32`、`CoordinateSpace` と責務が重ならないかを先に整理する。既存型を拡張できる場合は新型を増やさない。pixel index、raster dimension、stride は `inkpod-image` の既存境界型と validation を尊重する。

### 5B. migration slice

1. `device_to_document` を typed input/output にし、raw public input の adapter を残す。
2. view pan、viewport、zoom の private helper を typed value にする。
3. guide/grid snap と locator sampling を document coordinate で統一する。
4. stroke/gesture の `CoordinateSpace` 分岐後を typed point にする。
5. transform の anchor、frame、guide helper を typed point/size/rect にする。
6. snapshot overlay と vector geometry の document coordinate 保持を audit する。

### 5C. 必須 test matrix

| 軸 | ケース |
| --- | --- |
| zoom | minimum、1:1、maximum、非有限、範囲外 |
| pan | 正、負、large bounded value、overflow 相当 |
| flip | none、horizontal、vertical、both |
| viewport | 1 pixel、通常、zero、非有限 |
| document edge | origin、最後の valid pixel、half-open right/bottom edge、outside |
| round-trip | document → device → document、device → document → device |
| DPI | Core の Canvas transform に OS DPI を二重適用しないこと |

round-trip は floating-point の性質に応じた小さい明示 tolerance を使う。pixel hit test は half-open cell の既存契約を exact assertion で維持する。

### 完了条件

- Core private API で document point を device point parameter へ直接渡せない。
- `device = document * zoom + pan` と flip の適用順が一つの変換実装に集約される。
- 6400% zoom、最終 document cell、flip view の既存 regression test が通る。
- C ABI の fixed-layout coordinate record は変更されていない。
- snapshot 内の vector/raster/overlay coordinate contract が rustdoc と test で一致する。

## 12. Milestone 6: 公開 API rustdoc の完成

### 目的

公開 Rust API を source archaeology なしで安全に利用できる状態にし、今後の設計 drift を lint で検出する。

### 6A. documentation template

公開 item には必要に応じて次を記載する。

- 何を表す型または操作か
- 座標系、単位、valid range、half-open/closed boundary
- stable ID の所属と lifetime
- success、no-op、error の違い
- document revision、view revision、history、dirty、savepoint への影響
- snapshot、slice、reference の ownership/lifetime
- cancellation と partial commit の禁止
- panic の有無。通常の Core API は invalid input を `CoreError` にすること
- 短い usage example が契約理解に有効な場合の doctest

### 6B. lint の段階導入

いきなり crate 全体へ `#![deny(missing_docs)]` を適用しない。次の順で module 単位に警告を解消する。

1. `Core`、`CoreError`、`DispatchOutcome`、`DocumentInfo`
2. document/layer/plane API
3. history、snapshot、view/coordinate API
4. selection、paint、stroke、transform
5. effects、vector、animation、Batch
6. crate root の再 export と module-level docs

全 public surface が解消された時点で crate root に `#![warn(missing_docs)]` を追加し、workspace lint policy と競合しないことを確認する。将来 `deny` へ上げるかは、本計画とは別に判断する。

### 6C. 検証

```text
RUSTDOCFLAGS="-D warnings" cargo doc --package inkpod-core --all-features --no-deps
cargo test --package inkpod-core --all-features --doc
```

example は temporary path、current time、OS 固有値へ依存させない。C ABI の ownership/lifetime は引き続き header と `docs/ffi.md` を正本とし、Rust Core rustdoc に C ABI の説明を複製しすぎない。

### 完了条件

- 全 public item が missing-doc warning なしで document build できる。
- ID、座標、revision/history side effect の記述が実装と test に一致する。
- doctest が通常の workspace test と同じ stable Rust で通る。
- rustdoc のためだけの public accessor を追加していない。

## 13. Milestone 7: 責務単位の module 分割と最終整理

### 目的

前段で確立した transaction/type/documentation 境界に沿って、大きな file を意味上の責務単位へ分割する。行数を減らすためだけの分割は行わない。

### 7A. `vector/operations.rs`

推奨構成:

```text
vector/
  mod.rs
  model.rs
  geometry.rs
  path_operations.rs
  selection.rs
  rasterization.rs
  vectorization.rs
  thumbnail.rs
```

- add/erase/connect/width correction は `path_operations.rs`。
- vector selection は `selection.rs`。
- raster layout と rasterize は `rasterization.rs`。
- raster plane からの vectorize は `vectorization.rs`。
- layer thumbnail sampling は `thumbnail.rs`。
- shared data invariant は `model.rs`、pure geometry は `geometry.rs` に残す。

### 7B. `batch/codec.rs`

推奨構成:

```text
batch/codec/
  mod.rs
  codes.rs
  operation.rs
  filter.rs
  payload.rs
```

- public-to-file enum code は `codes.rs`。
- operation kind の encode/decode は `operation.rs`。
- filter payload は `filter.rs`。
- bounded reader/writer と primitive decode は `payload.rs`。
- malformed length、trailing byte、invalid enum の test は最も近い implementation file に colocate する。

### 7C. `transform.rs`

推奨構成:

```text
transform/
  mod.rs
  document.rs
  raster.rs
  frame.rs
  numeric.rs
```

- public document mirror/rotate/resize orchestration は `document.rs`。
- raster convert/mirror/rotate/place/resample は `raster.rs`。
- frame metadata、margins、guides は `frame.rs`。
- checked scale/dimension helper は `numeric.rs`。

### 7D. `view.rs`

推奨構成:

```text
view/
  mod.rs
  commands.rs
  coordinates.rs
  guides.rs
  secondary.rs
  shortcuts.rs
```

- main/secondary view command は `commands.rs` と `secondary.rs`。
- device/document transform と range validation は `coordinates.rs`。
- guide/grid/snap は `guides.rs`。
- shortcut validation/resolution は `shortcuts.rs`。

### 分割規則

- 一 file ずつ移動し、同じ差分で algorithm を書き換えない。
- visibility は必要最小限にし、安易に `pub(crate)` へ広げない。
- `lib.rs` と各 `mod.rs` は module declaration と意図した re-export を中心にする。
- private test は対象 helper と一緒に移動し、public contract test の module 構造は維持する。
- recursive CMake source tracking と architecture test が新規 file を認識することを確認する。
- 分割後に循環依存が生じる場合は shared helper の所属を再検討し、便宜的な global utility module を作らない。

### 完了条件

- 対象四 file の責務が上記 module に分かれ、旧 compatibility shim が残っていない。
- crate root と module index が production logic を持たない。
- `pub(crate)` item 数が分割前より理由なく増えていない。
- test 名、requirement ID、公開 API、C ABI、file format が維持されている。
- M1 state-machine test と M2 benchmark の意味上の出力が分割前と一致する。

## 14. 各マイルストーン共通の検証ゲート

開発中は対象 test を先に実行し、マイルストーン完了時に少なくとも次を通す。

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --package inkpod-core --all-features
git diff --check
```

M2 以降:

```text
cargo bench --package inkpod-core --bench core_workflows -- --quick
```

M6 以降:

```text
RUSTDOCFLAGS="-D warnings" cargo doc --package inkpod-core --all-features --no-deps
```

source 構成または CMake tracked input に関わるマイルストーンでは、利用可能な platform で CMake configure/build と CTest も実行する。非 Windows では Rust と platform-independent CTest を完了し、Windows x64 Debug/Release の CMake build/CTest は Windows CI で確認する。実行できなかった検証は完了報告に明記する。

## 15. レビュー時のチェックリスト

各変更で次を確認する。

- [ ] 利用者向け挙動または仕様 status を意図せず変更していない。
- [ ] public Rust API、C ABI、file format の差分有無が説明されている。
- [ ] success、no-op、invalid、cancel、Undo/Redo の該当ケースがある。
- [ ] document revision と view revision のどちらを進めるかが明確である。
- [ ] dirty/savepoint と redo branch の挙動が維持されている。
- [ ] failure、cancel、stale revision で部分結果を commit しない。
- [ ] test-only public API または private bridge を追加していない。
- [ ] new dependency がある場合、用途、license、feature、代替案が記録されている。
- [ ] benchmark を変更した場合、scenario parameter と output schema の互換性が説明されている。
- [ ] source file を追加・移動した場合、module index、architecture guard、CMake tracking を確認した。
- [ ] unrelated formatting、rename、機能追加を混ぜていない。
- [ ] `docs/compatibility.md` と `docs/implementation-status.md` は、実際に状態が変わった場合だけ更新した。

## 16. リスクと対策

| リスク | 兆候 | 対策 |
| --- | --- | --- |
| property test の flaky 化 | 同じ commit で seed 未指定の失敗が変わる | fixed seed、bounded generator、replay 表示、OS entropy 非依存 |
| test が private 実装へ密結合 | module 移動だけで大量に壊れる | public observation と private invariant を分離 |
| benchmark noise を regression と誤認 | CI machine ごとに時間が大きく変動 | absolute time gate を避け、同一環境の中央値と意味上の counters を使う |
| transaction 導入で no-op/history が変化 | revision や history entry が一つ余分に進む | M1 property、operation ごとの success/no-op/Undo test、wave 移行 |
| transaction が preview/cancel を複雑化 | base state の ownership が曖昧になる | 同期 edit と session transaction を分け、M3 では preview 系を除外 |
| newtype が ABI/file layout を変える | header parity または round-trip failure | internal-first、境界 conversion、`#[repr(transparent)]` に ABI を依存させない |
| ID 変換のため visibility が拡大 | `pub(crate)` helper が増える | constructor/conversion を所有 module に置き、slice ごとに visibility audit |
| 座標型導入で rounding が変化 | edge pixel、flip、高 zoom test failure | 既存式を先に型で包み、algorithm 変更は別差分 |
| module 分割で循環依存 | shared utility への逃避が増える | domain ownership を見直し、データ方向を一方向にする |
| documentation lint が巨大差分化 | 全 module の警告を一度に修正 | module 単位で導入し、crate-level warn は最後に有効化 |

## 17. 停止・切り戻し条件

次のいずれかが起きた場合、その wave を先へ進めず、直前の green state へ戻して原因を分離する。

- 既存 requirement の利用者向け結果が、承認された仕様変更なしに変わる。
- native save/open round-trip または C ABI layout/header parity に差分が出る。
- invalid/cancel/stale revision で document、history、dirty、output file に部分変更が残る。
- state-machine test が再現不能な失敗を起こす。
- quick benchmark が通常 CI を不安定にする。
- newtype または module 分割のために public visibility を広範囲へ拡大する必要が生じる。
- 一つの変更で複数マイルストーンの意味上の変更が不可避になる。

切り戻しは destructive な repository 操作ではなく、対象変更を通常の差分として取り消せる小ささを維持することで可能にする。

## 18. 全計画の Definition of Done

本計画は次をすべて満たした時点で完了とする。

- M1 の deterministic state-machine/property test が通常 CI で安定して通る。
- M2 の Core quick benchmark が CI に接続され、full benchmark を再現できる。
- 同期 document edit の transaction lifecycle が一か所に集約されている。
- Core 内部の主要 stable ID、revision、history state が意味別 newtype になっている。
- document/view/device の主要座標変換が typed API に集約されている。
- Core の全公開 Rust API が warning なしで rustdoc を生成できる。
- `vector/operations.rs`、`batch/codec.rs`、`transform.rs`、`view.rs` の混在責務が意味単位に分割されている。
- public Rust API、C ABI v2、`.inkpod` file format、既存 requirement status に意図しない破壊的変更がない。
- Rust format、zero-warning clippy、workspace test、Core test、quick benchmark、rustdoc、platform-independent CTest が通る。
- Windows x64 Debug/Release の CMake build と CTest が CI で通る。
- 最終的な architecture、current status、代表的検証が `docs/architecture.md`、`docs/implementation-status.md`、`docs/compatibility.md` の必要箇所へ反映されている。

## 19. 進捗記録

マイルストーンを開始したら、次の表だけを更新して全体状態を追跡する。個々の test 結果や詳細は各変更の記録と既存 status document に置き、この文書を日誌にしない。

| Milestone | Status | 完了日 | 備考 |
| --- | --- | --- | --- |
| M0 基準線と安全網 | Completed | 2026-07-30 | 公開 mutation・状態遷移・CoreObservation・M3 wave inventory を文書化 |
| M1 決定性・property test | Completed | 2026-07-30 | 固定 seed CoreObservation state machine、atomicity、境界 invariant を通常 test に追加 |
| M2 Core benchmark | Completed | 2026-07-30 | 7 Core workflow の固定 quick/full benchmark、semantic assertion、Linux CI、基準出力を追加 |
| M3 document transaction | Completed | 2026-07-30 | 同期 document edit を owning transaction に集約し、除外 workflow も atomic publish 境界を共有 |
| M4 ID・revision newtype | Completed | 2026-07-30 | Core 内部の stable ID・revision を意味別 newtype 化し、公開・C ABI・file 境界の整数表現を維持 |
| M5 座標・寸法型 | Completed | 2026-07-30 | document/device 型と単一 view transform を導入し、公開・C ABI・file 境界を維持 |
| M6 rustdoc | Completed | 2026-07-30 | 全公開 API の契約 rustdoc、missing-doc warning、crate-level doctest を追加 |
| M7 module 分割 | Completed | 2026-07-30 | vector・Batch codec・transform・view を責務別 module に分割し、architecture guard を追加 |
