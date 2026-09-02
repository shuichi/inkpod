# Core COW 最適化計画

作成日: 2026-09-02

対象要件: `IO-003`, `SEQ-001`, `PERF-001`

状態: 設計・実装計画。本文書の作成時点では実装しない。

## 1. 結論

最初の実装対象は、Sequence pane から sidecar のない raster cell へ切り替えるときの
重複 materialization とする。

現状は、Sequence catalog が対象 raster の `TileRaster` を既に保持しているにもかかわらず、
cell switch の raster-pair resolver が同じ decoded raster を次の順で再構築している。

```text
LoadedImage / CommonRaster<Vec<u8>>
    ├─ 全 pixel Vec clone
    ├─ AssetId 算出のための全 payload scan
    ├─ AssetRecord の dense payload 所有
    └─ 全 pixel を別の TileRaster へ materialize
                                  ↓
                         CellDocument / Genesis
```

計画後の eligible な経路は次の形にする。

```text
final LoadedImage ── exact runtime provenance ──> retained decoded payload
                                                    │
SequenceCellSource.TileRaster ── COW clone ─────────┼─> AssetRecord
                                                    ├─> GenesisRasterSource
                                                    └─> CellDocument MainLine
```

ただし、`document_from_sequence_source` は利用しない。この helper は表示用の軽量文書を
作るだけで、`AssetStore`、`GenesisRasterSource`、ASST、canonical stable ID、EditorState の
初期基準、`Planned` pair authority を構築しないためである。

最適化は必ず opportunistic とする。正確な runtime provenance を証明できない場合は、
現在の `import_decoded_common_raster` 経路へ戻り、利用者向けの成功・失敗・保存結果を変えない。

## 2. 現状の基準経路と費用

### 2.1 Sequence catalog の作成

`rust/inkpod-core/src/file_io/prepare.rs::images` は各 `LoadedImage` を
`SequenceCellSource::from_loaded_image` へ渡す。ここで次を実行する。

- decoded `CommonRaster` を `common_to_tile_raster` で `TileRaster` に変換する。
- thumbnail を作る。
- tile と thumbnail の allocation を `DecodedLease` で I/O manager の budget に計上する。
- `source_generation` を catalog identity として保持する。

この処理によって、cell switch 前に編集用と同じ canonical pixel を持つ tiled source は既に存在する。

### 2.2 raster-pair cell switch

実製品経路は次のとおりである。

1. `FileIoJob::start_sequence_raster_pair_switch`
2. `IoManager::read_image`
3. `file_io::prepare::raster_pair`
4. `SequenceSwitchSnapshot::prepare_pair_target`
5. `Core::sequence_restore_prepared_pair_target`
6. owner thread の `commit_prepared_sequence_switch`

sidecar が存在する場合は `.inkpod` を staged Core へ完全 replay し、通常 composite と raster の
canonical decoded 値を比較する。この経路は履歴、Genesis、asset、EditorState、savepoint の正本であり、
最適化対象にしない。

sidecar が存在しない場合は `raster_pair` が `Core::new()` を作り、
`import_decoded_common_raster` を呼ぶ。この中で次の重複処理が起きる。

| 処理 | 現状の費用 |
|---|---|
| `CommonRaster::clone` | canonical dense pixel 全体の allocation と copy |
| `AssetStore::ingest_raster` | AssetId のための payload 全走査 |
| `materialize_raster` | 全 pixel decode、tile allocation、tile 書き込み |
| `TileRaster::clone` | tile pixel は共有するが `BTreeMap<TileCoord, Arc<Tile>>` は clone ごとに複製 |

`Tile` の pixel storage 自体は既に `Arc<Tile>` と `Arc::make_mut` による COW である。今回追加で
解消するのは、dense payload copy、二度目の tile materialization、および未変更 raster clone ごとの
tile map 複製である。

## 3. 目標

### 3.1 必須目標

- exact provenance が一致する sidecar-less sequence target では dense pixel copy を行わない。
- Sequence catalog が持つ `TileRaster` の pixel allocation を Genesis asset と編集 document で共有する。
- 未変更 `TileRaster` の clone は tile count に依存しない O(1) にする。
- 最初の編集では map metadata と実際に変更した tile だけを detach する。
- 現行と同一の `AssetId`、`GenesisRasterSource`、stable ID 順序、document/editor savepoint、
  history/journal、`Planned` pair authority を生成する。
- Save、recovery、Revert、cache-free replay、Undo/Redo が I/O cache や外部 raster path なしで成立する。
- provenance 不一致、cache eviction、force reload、外部変更、budget failure では既存経路へ安全に戻る。
- cancel、stale、TOCTOU failure では live Core、ID high-watermark、pair authority、savepoint を進めない。

### 3.2 非目標

- existing sidecar の replay を raster preview や Sequence source で代用しない。
- target recovery、repair-needed reopen、standalone recovery を COW source から再構成しない。
- encoded raster bytes、path、filesystem identity を Genesis、ASST、journal、AssetId に永続化しない。
- 全 Sequence cell の decoded dense image または `AssetRecord` を無制限に pin しない。
- 通常保存時に必要な native/raster encoding の全 payload 読み取りを省略しない。
- 今回の第一段階では menu の File Open を tile reuse 対象にしない。File Open には既存の
  `SequenceCellSource` がないため、別の計測結果を得てから検討する。

## 4. 絶対に維持する不変条件

### 4.1 companion と publication

- File Open、Sequence、Revert は引き続き同じ companion resolver を使う。
- existing native candidate は decode、asset 検証、replay、canonical composite 比較を完了してから採用する。
- COW eligibility は pair recovery と final `LoadedImage` の確定後にだけ判定する。
- raster/native の complete `FileStamp`、missing-path identity、最終 candidate 再確認を省略しない。
- target は全て staged Core 上で構築し、owner thread の最終検証後に一回だけ公開する。

### 4.2 Genesis、ASST、履歴

- raster source は canonical dense payload を持つ content-addressed asset として ASST に保存できること。
- `GenesisRasterSource { plane_id, asset_id }` を従来どおり作り、GENS 内の初期 MainLine は archive 時に
  空にして ASST を正本とすること。
- materialized `TileRaster` と COW backing は派生 runtime state であり、replay の正本にしない。
- `AssetStore::detached_archive_round_trip` は runtime lease を捨てて payload を独立再取り込みする
  cache-free 検証境界として維持する。
- native save/reopen 後も unlimited Undo/Redo、redo branch、EditorState、savepoint が一致すること。

### 4.3 source identity

`source_generation` 単独を COW 再利用の証明にしてはならない。generation は `IoManager` ごとの
counter であり、別 manager では同じ数値が発生し得る。

再利用には、少なくとも次を一つの opaque runtime proof として照合する。

- 同じ `IoManager` owner (`Arc::ptr_eq` 相当)
- 同じ decoded allocation identity
- complete `FileStamp`
- cache generation
- `CommonRasterFormat`
- width、height、pixel format、DPI、canonical payload length

proof は path、encoded bytes、生 pointer、OS handle を Core の semantic stateへ露出しない。
また、`SequenceRenderSourceIdentity::owner_generation` は renderer/catalog の入れ替えを検出する
presentation provenance であり、`IoManager` が発行する decoded-allocation proof の代用にしない。

### 4.4 lifetime と memory budget

- decoded dense payload を共有する owner は、最後の `AssetRecord` または snapshot が破棄されるまで
  I/O manager の decoded charge を保持する。
- source tile を共有する owner は、最後の `AssetRecord` または snapshot が破棄されるまで既存の
  derived `DecodedLease` を保持する。
- `capture_file_snapshot(false)` は cloned Core から Sequence catalog を外すため、tile charge を
  `SequenceCellSource` だけに持たせてはならない。runtime lease は `AssetRecord` と一緒に clone/GC される。
- encoded cache entry と外部 path は retained asset の寿命に結び付けない。
- 同じ allocation の clone に二重 charge を付けない。

## 5. 提案する設計

### 5.1 `TileRaster` の上位 map も COW にする

`rust/inkpod-image/src/raster.rs` の内部表現を次のように変える。

```text
現在: tiles: BTreeMap<TileCoord, Arc<Tile>>
変更: tiles: Arc<BTreeMap<TileCoord, Arc<Tile>>>
```

読み取りと clone は map を共有する。`set_pixel`、`insert_tile`、`remove_tile_if_empty` などの全 mutation は
最初に `Arc::make_mut(&mut tiles)` を行い、その後、変更対象の `Arc<Tile>` だけを既存どおり
`Arc::make_mut` する。

これにより、未編集 document、Genesis、Sequence source、staging snapshot の clone は O(1) になる。
最初の実変更では tile map の metadata が一度だけ clone され、pixel copy は触った tile に限定される。

この変更は runtime representation だけであり、pixel 値、tile revision、checksum、serialization を変えない。

### 5.2 `inkpod-io` に opaque provenance と retained decoded lease を追加する

`DecodedLease` が作られた時点の source proof を内部に保持する。概念上は次の情報で構成する。

```text
DecodedSourceProof
    manager owner identity
    weak decoded-allocation identity
    source FileStamp
    source generation
    raster format/info/payload length
```

weak identity を用い、Sequence catalog を作っただけでは全 decoded image を pin しない。

`IoManager` に、catalog source の lease と pair resolver が得た final `LoadedImage` を一度に照合する
APIを追加する。全条件が一致した場合だけ、pathless な `RetainedDecodedRaster` を返す。

`RetainedDecodedRaster` は次だけを提供する。

- immutable `CommonRasterInfo`
- immutable canonical pixel slice
- allocation charge を保持する clone

path、encoded bytes、file handle、mutable pixel access は提供しない。照合対象は immutable `Arc` owner なので、
別の cache lookup を挟まず、検証済み `LoadedImage` 自身から lease を retain する。

### 5.3 `SequenceCellSource` に managed source capability を保持する

`SequenceCellSource::from_loaded_image` だけが managed capability を作れるようにする。

- 現在の derived `DecodedLease` に exact source proof を保持させる。
- source `TileRaster` と thumbnail は従来どおり immutable とする。
- runtime-only な `Arc<OnceLock<AssetId>>` を追加できる構造にする。
- `from_common_raster`、`from_rgba_bytes`、FFI の borrowed raster 経路は capability を持たず、常に fallback する。
- capability と cached `AssetId` は `PartialEq`、semantic digest、serialization、sequence identity に含めない。

第一段階は activation ごとに canonical payload を一度 hash し、copy/materialization の除去を先に検証する。
第二段階で、同じ decoded allocation proof が一致する warm revisit に限り `AssetId` を再利用し、hash scan も
省略する。inactive cell の `AssetRecord` 全体は cache せず、dense allocation の過剰 pin を避ける。

### 5.4 `AssetRecord` の payload backing を runtime 多相化する

`rust/inkpod-core/src/asset.rs` の `AssetRecord` は semantic payload と runtime backing を分ける。

```text
AssetPayload
    Owned(Arc<[u8]>)
    RetainedDecoded(RetainedDecodedRaster)

RuntimeRasterLease
    source tile allocation の DecodedLease
```

どちらの payload も `payload() -> &[u8]` から同じ canonical bytes を返す。
`AssetId`、descriptor、dedup collision 検査、persistent record、ASST writer は backing kind を観測しない。

private な `AssetStore::ingest_managed_raster` を追加し、次を一つの staged 操作として行う。

1. retained decoded metadata から canonical descriptor を検証する。
2. payload length、RGBA8/16、straight alpha、sRGB、stride を検証する。
3. source `TileRaster` の寸法、format、初期 revision を検証する。
4. canonical payload から `AssetId` を計算または exact-proof に結び付いた cached ID を採用する。
5. retained dense payload、COW `TileRaster`、tile allocation lease を一つの `AssetRecord` に収める。
6. 既存 ID がある場合は従来どおり descriptor と全 canonical bytes を比較し、collision を fail closed にする。

`detached_archive_round_trip` は `Owned` payload へ深い copy と再取り込みを行う現行動作を維持する。
これにより、manager、cache、source raster を破棄した後でも replay が成立することを検証できる。

### 5.5 canonical import primitive を共通化する

`new_cell_from_raster_asset_with_placement` を、asset の取り込みと document publication の二段階に分ける。

```text
Owned RasterAssetInput ─────────────┐
                                   ├─> one canonical imported-cell builder
Managed Sequence Raster Seed ──────┘
```

共通 builder は、現在の `MainLinePlane` import と同じ順序で次を行う。

- fresh Core の stable ID を同じ順番で取得する。
- `CellDocument::new` と同じ paper/frame 初期値を使う。
- retained `TileRaster` を MainLine に置き、同 depth の空 Color plane を作る。
- alpha に応じて `SolidWhite` / `Transparent` underlay を決める。
- asset GC roots と `GenesisRasterSource` を作る。
- MainLine を active editor target にする。
- document/editor の clean 初期 savepoint、Genesis state、journal、history を同じ値にする。
- runtime preview、selection、secondary state の reset を既存経路と同じ順序で行う。

pair path、missing-path proof、`Planned` authority は builder に渡さない。これらは従来どおり
`file_io::prepare::raster_pair` が final filesystem proof の検証後に設定する。

### 5.6 raster-pair resolver への接続

`SequenceSwitchSnapshot` から target の immutable `SequenceCellSource` または private な reuse candidate を
clone し、worker へ渡す。reuse 判定は recovery 後の final `LoadedImage` に対して行う。

| 状態 | 動作 |
|---|---|
| existing valid sidecar | 現行の完全 replay。COW source を使わない |
| target recovery | 現行の完全 replay と lineage 検証。COW source を使わない |
| repair-needed native | 現行 replay。COW source を使わない |
| sidecar なし、exact proof 一致 | managed asset + source tile の COW builder |
| proof absent/mismatch、別 manager、eviction、clear、reload | 現行 `import_decoded_common_raster` へ fallback |
| external change、candidate drift、TOCTOU failure | 現行どおり pair conflict。fallback で隠さない |
| cancel/stale | staged target と lease を破棄し、live Core を変更しない |
| digest collision、内部 descriptor 不整合 | invariant failure として fail closed |

通常の ineligibility はエラーではない。COW builder に入った後の内部不整合を silent fallback で隠してはならない。

### 5.7 pristine Sequence render cache の再登録

現行の pair target restore は `sequence_render_cache.invalidate_document()` を行うが、pair resolver で作った
target を pristine source として再登録しない。

準備結果に private な次の分類を持たせる。

```text
SequencePairTargetKind
    ReplayedSidecar
    ReplayedRecovery
    MaterializedRasterFallback
    CowManagedRaster
```

`CowManagedRaster` のときだけ、runtime sequence を staged target へ付けた後に
`register_pristine_sequence_source` を呼ぶ。sidecar、recovery、fallback では登録しない。
最初の preview/edit、document revision、state、view mode の既存 invalidation 条件は変更しない。

## 6. 実装フェーズ

各フェーズは独立した意味上の変更とし、前フェーズのテストが通ってから次へ進む。

### Phase 0: baseline と観測点を固定する

対象:

- `rust/inkpod-core/tests/contracts/file_io.rs`
- `rust/inkpod-core/tests/contracts/animation.rs`
- `rust/inkpod-io/tests/manager.rs`
- `apps/windows/app/sequence_performance_smoke.cpp`
- 必要なら専用の Rust benchmark

作業:

- 現行 sidecar-less switch の native DTO、AssetId、Genesis、digest、EditorState、pair proof を fixture 化する。
- test/benchmark 限定の forced-legacy selector を用意し、同じ入力を managed candidate と従来経路の両方へ
  通して結果を直接比較する。製品設定や C ABI には公開しない。
- copy bytes、asset hash bytes、full tile materialization pixels、COW hit/fallback、tile detach を数える
  test-support semantic counter を設計する。
- production C ABI は変更せず、Rust contract/benchmark から観測できるようにする。
- 既存 1754x1240 TGA の A-B-A / A-B-C-B-A を変更前に複数回測定し、全 sample と中央値を記録する。
- 承認済み `core_workflows` の workload、scenario、counter、checksum、timed interval、envelope は変更しない。
  COW 固有の測定が必要なら独立した contract test/benchmark を追加する。

完了条件:

- baseline の semantic checksum と native bytes が固定される。
- wall-clock envelope はこの時点で変更しない。

### Phase 1: `TileRaster` map-level COW

対象:

- `rust/inkpod-image/src/raster.rs`
- `rust/inkpod-image/benches/large_document.rs`
- Core の snapshot/Undo/render contract

作業:

- tile map を `Arc<BTreeMap<...>>` 化する。
- 全 mutation entry point を監査し、map と対象 tile の二段階 detach を実装する。
- clone、no-op write、remove、insert、checksum invalidation、edge tile をテストする。

完了条件:

- clone 時に map allocation を共有する。
- first write 後は source map と target map が分離する。
- touched tile だけ pixel allocation が分離し、untouched tile pointer、revision、checksum は一致する。
- Core semantic checksum と serialized output は変更前と一致する。

### Phase 2: I/O provenance と retained decoded payload

対象:

- `rust/inkpod-io/src/image.rs`
- `rust/inkpod-io/src/manager.rs`
- `rust/inkpod-io/src/cache.rs`
- `rust/inkpod-io/src/lib.rs`
- `rust/inkpod-io/tests/manager.rs`

作業:

- weak decoded identity を含む opaque source proof を `DecodedLease` に追加する。
- exact final `LoadedImage` からだけ `RetainedDecodedRaster` を得る manager API を追加する。
- retained dense bytes と derived tile bytes の charge/lifetime counter を検証する。

完了条件:

- same manager + exact allocation/stamp/generation/format/info だけが hit する。
- 別 manager の同じ generation、force reload、cache clear/eviction、stamp change は miss になる。
- miss は副作用なしで、通常 decode/import を妨げない。
- cache clear、manager shutdown、`LoadedImage` drop 後も retained owner がいる間だけ pixels と charge が残る。
- 最後の owner drop 後に全 charge が基準値へ戻り、ownership cycle がない。

### Phase 3: managed `AssetRecord`

対象:

- `rust/inkpod-core/src/asset.rs`
- `rust/inkpod-core/src/genesis.rs`
- `rust/inkpod-core/src/persistence.rs`
- `rust/inkpod-core/src/journal.rs`

作業:

- `AssetPayload` runtime backing を導入する。
- private managed-raster ingestion と tile lease ownership を実装する。
- owned/retained の dedup、GC、clone、detached replay、serialization を同一 contract に通す。

完了条件:

- owned と retained で descriptor、AssetId、payload、ASST bytes が一致する。
- managed record の構築で dense payload copy と full tile materialization が 0 になる。
- save snapshot が live Core/Sequence より長生きしても charge と pixels が有効である。
- detached replay は manager なしで成功し、意図的に独立 allocation を持つ。

### Phase 4: canonical imported-cell builder

対象:

- `rust/inkpod-core/src/animation/io.rs`
- `rust/inkpod-core/src/asset_operations.rs`
- `rust/inkpod-core/src/core.rs`

作業:

- owned input と managed seed が同じ builder を使うように refactor する。
- stable ID、Genesis、EditorState、history/savepoint の生成を一実装へ集約する。
- `document_from_sequence_source` は表示/prefetch 用のまま維持する。

完了条件:

- 同じ UUID と raster input に対し、legacy と managed の public state、native bytes、raster output が一致する。
- success、no-op、invalid、allocation failure、cancel で ID/revision/savepoint atomicity が一致する。

### Phase 5: sidecar-less Sequence switch へ接続する

対象:

- `rust/inkpod-core/src/animation/sequence.rs`
- `rust/inkpod-core/src/file_io/session.rs`
- `rust/inkpod-core/src/file_io/prepare.rs`
- `rust/inkpod-core/src/sequence_io.rs`
- `rust/inkpod-core/src/animation/sequence_operations.rs`
- `rust/inkpod-core/src/animation/sequence_render_cache.rs`

作業:

- captured target source を raster-pair preparation へ渡す。
- missing-sidecar branch だけで proof を照合し、managed builder または legacy fallback を選ぶ。
- preparation kind を owner commit まで運び、exact COW hit のみ pristine source を再登録する。
- 第一段階の正しさ確定後、exact allocation に結び付いた `AssetId` cache で warm hash scan を省略する。

完了条件:

- COW hit/fallback の両方が同じ利用者向け結果を返す。
- sidecar、recovery、normal-save 後の stale catalog は COW branch を通らない。
- warm eligible switch は physical read 0、decode 0、dense copy 0、tile materialization 0 になる。
- first edit は touched tile のみ detach し、Sequence source と Genesis asset は不変である。

### Phase 6: 製品性能検証と文書更新

対象:

- `apps/windows/app/sequence_performance_smoke.cpp`
- `tests` の Windows performance registration
- `SPEC.md`
- `docs/architecture.md`
- `docs/compatibility.md`
- `docs/implementation-status.md`
- 必要な場合だけ `docs/ffi.md` / `docs/file-format.md`

作業:

- foreground desktop で変更前後を同じ build/profile/input で測る。
- Rust semantic benchmark を CI の正しさ gate、Windows smoke を製品経路の latency/resource gate とする。
- 現在状態、既知差分、検証結果だけを status/compatibility に反映する。

完了条件:

- 全 semantic gate が通る。
- warm median が変更前より悪化せず、copy/materialization の除去が counter で証明される。
- 新しい時間 envelope を採用する場合は、workload、環境、全 sample、中央値を提示してユーザー承認を得る。

## 7. テスト行列

### 7.1 source proof と fallback

- same manager、same stamp/generation/allocation/format/info は COW eligible。
- 別 manager で generation 数値だけ同じ場合は reject。
- `clear_cache`、force reload、eviction 後の新 allocation は reject して legacy fallback。
- 同じ file identity/size でも complete stamp が変われば reject。
- format、depth、DPI、dimension、payload length の不一致は reject。
- public/FFI constructed `SequenceCellSource` は capability がなく fallback。
- catalog replacement と owner generation replacement は古い capability を再利用しない。

### 7.2 Genesis と永続化

- legacy/managed の `AssetId`、`GenesisInfo`、Genesis raster `plane_id/asset_id` が一致。
- canonical document/editor digest、StateId、savepoint、journal、stable ID high-watermark が一致。
- RGBA8 PNG/TGA/BMP、RGBA8/16 PNG/TIFF、opaque/transparent source を検証し、alpha=0 の pixel に
  隠れている RGB 値も byte-for-byte に保持する。
- normal pair Save → cache clear → manager release → reopen が exact。
- release history cache → Undo/Redo → branch → Revert が exact。
- recovery lineage の UUID、Genesis document、raster asset identity、pair proof が一致。
- serialized native 全体と raster companion が同じ入力/UUIDに対して byte-for-byte 一致。

### 7.3 COW と lifetime

- `TileRaster::clone` が map と全 tile allocation を共有する。
- first one-pixel edit は map を一回、対象 tile を一枚だけ detach する。
- no-op write、preview cancel は detach/publish を残さない。
- untouched tile pointer、tile revision、source checksum が維持される。
- `Core::clone_for_staging`、`DocumentSaveSnapshot`、render snapshot が live Core より長生きしても charge が残る。
- Asset GC、snapshot drop、Core drop の最後の owner で retained decoded/tile charge が解放される。
- budget failure と allocation failure は旧 Core と cache stats を変更しない。

### 7.4 pair と sequence

- existing sidecar は必ず replay し、全 history/redo branch/editor savepoint を保持する。
- sidecar-less switch は `Planned` proof と直接 first Save を保持する。
- normal pair Save 後の再訪は sidecar replay を選ぶ。
- source recovery、target recovery、repair-needed、malformed/mismatched sidecar の結果を変更しない。
- stale request、cancel、queue rejection、final candidate drift、post-read stamp change は atomic failure。
- exact COW hit の snapshot だけが `INKPOD_SNAPSHOT_SOURCE_SEQUENCE_PRISTINE` を持つ。
- first edit/preview/display-mode change の既存 pristine invalidation を維持する。

### 7.5 既存 regression の強化対象

- `rust/inkpod-core/tests/contracts/file_io.rs`
  - `io_003_sequence_pair_switch_replays_existing_sidecar_history`
  - `io_003_sequence_pair_switch_reopens_a_cell_after_its_normal_pair_save`
  - `io_003_sequence_pair_switch_retains_missing_sidecar_first_save_proof`
- `rust/inkpod-core/tests/contracts/animation.rs`
  - `sequence_render_revisit_reuses_exact_payload_and_tile_revision`
  - `sequence_render_first_edit_preserves_unchanged_tiles_and_pristine_bank`
- `rust/inkpod-core/tests/contracts/native_v32.rs`
  - malformed Genesis/ASST と cache-free replay contract
- `rust/inkpod-io/tests/manager.rs`
  - manager ownership、cache generation、lease budget/lifetime contract

## 8. 性能計測と合格基準

時間だけを合否条件にせず、最初に意味上の counter を固定する。

| counter / observation | eligible cold switch | eligible warm revisit | fallback |
|---|---:|---:|---:|
| physical raster read | 現行 cache 契約以下 | 0 | 現行と同じ |
| raster decode | 現行 cache 契約以下 | 0 | 現行と同じ |
| dense payload copy bytes | 0 | 0 | 現行値 |
| full tile materialization pixels | 0 | 0 | 現行値 |
| canonical asset hash scans | 1以下 | Phase 5 完了後は0 | 現行値 |
| shared source tile count | source の allocated tile 数と一致 | 同左 | 0でも可 |
| first-edit detached pixel tiles | touched unique tile 数 | touched unique tile 数 | 現行と同じ |
| switch あたり snapshot publication | 1 | 1 | 1 |

workload は少なくとも次を固定する。

- 既存 Windows smoke: 1754x1240 TGA、A-B-A と A-B-C-B-A、各64 measured step。
- Rust quick: 小さい deterministic RGBA8/RGBA16 fixture で semantic counter を毎回検証。
- Rust full または専用 performance run: 4K RGBA8 と RGBA16、warm-up 後に複数回、中央値を採用。
- first edit: 64-pixel tile 境界内と境界跨ぎの二ケース。
- memory pressure: decoded budget 直下、eviction、snapshot pin、manager shutdown。

製品経路は `SPEC.md` の既存目標をそのまま合格基準とする。warm switch では UI handler p95 を
1 ms 以下、snapshot 提出 p95 を 4 ms 以下、正しい画像の最初の成功 Present までの p95 を
2 refresh interval 以内とし、p50/p95/p99/最大値と意味 counter を記録する。初回、未準備、cache 追放後、
保存待ちは warm 集計へ混ぜない。測定は一つの discarded process warm-up の後、少なくとも5つの独立した
Release process で行い、各 process の全 sample と中央値を残す。

既存の承認済み workload、harness、reference environment、envelope、`revision-max` 式はこの計画で
変更しない。変更が必要になった場合は、理由、環境、全 sample、意味 counter を先に提示し、ユーザーの
明示承認を得る。実測値に合わせて envelope を緩和しない。

Windows smoke が foreground/visible Canvas を取得できず計測前に停止した場合は未計測として報告し、
成功と推測しない。semantic counter の CI gate は desktop visibility に依存させない。

## 9. version と公開境界の判定

次が全て成立する限り、この最適化は runtime representation の変更である。

- native `.inkpod` v32 の serialized bytes/schema が不変。
- runtime replay epoch 27 の primitive 結果が不変。
- `AssetId`、document/editor digest、Genesis、journal が不変。
- runtime proof/lease/cached identity を永続化しない。
- C ABI に新 record/field/function を追加しない。

この場合、native format、replay epoch、C ABI v30 は更新しない。

次のいずれかを行う場合は同じ変更内で version を更新する。

- ASST を省略する、または外部 raster/cache を replay authority にする。
- path、stamp、generation、provenance を native record に加える。
- Genesis、AssetId、procedure result、document/editor digest を変える。
- telemetry を C ABI の固定幅 record に公開する。

C ABI telemetry が本当に必要になるまでは、Rust test-support counter と既存の
`InkpodIoCacheInfo` / snapshot source identity で検証し、ABI変更を避ける。

## 10. rollback と障害時方針

- COW 判定は一つの private function に集約し、`Ineligible` と invariant error を区別する。
- `Ineligible` は必ず現行の owned import へ戻る。
- external conflict、TOCTOU、malformed native、digest collision は fallback で隠さず現行 error を返す。
- staged target が live Core へ公開される前なら、retained lease と COW clone を drop するだけで rollback できる。
- owner validation 後の publication は従来どおり一回の Core replacement とし、部分的に asset や
  pair authority だけを差し替えない。
- 性能または memory pressure が悪化した場合は managed eligibility を一時的に無効化して legacy path を
  使用できる構造にする。ただし恒久的な二重実装にせず、原因修正後に fallback は ineligibility 用だけへ戻す。

## 11. 明示的な禁止事項

- `source_generation` だけで source を一致判定しない。
- `document_from_sequence_source` を raster-pair open/switch に直接使わない。
- sidecar の存在確認、replay、canonical comparison を COW hit で短絡しない。
- final `LoadedImage` ではなく、recovery 前または catalog 作成時の file stampだけを信頼しない。
- Core semantic state、AssetId、native file に path、OS handle、manager pointer、cache generation を入れない。
- `SequenceCellSource` だけに tile allocation charge を置いたまま、document/snapshotへtileを共有しない。
- inactive Sequence 全体の decoded dense payloadを無制限に強参照しない。
- cache-free replay を共有Arcの存在だけで成功扱いにしない。
- benchmarkのwall-clock低下だけで、serialized/replay contractの差分を正当化しない。

## 12. 完了条件

本最適化は次の全項目が満たされたときだけ完了とする。

- eligible sidecar-less Sequence switch で dense copy と full tile materialization が counter 上 0。
- legacy fallback、existing sidecar、recovery の全 contract が回帰なし。
- native bytes、AssetId、Genesis、digest、history/editor/savepoint、pair authority が baseline と一致。
- first edit が touched tile だけを detachし、Undo/Redo と pristine cache invalidation が正しい。
- save/reopen/cache-free replay が manager/cache/path なしで成功。
- cancellation、stale、external change、allocation/budget failure が atomic。
- retained allocation が全 owner の寿命中だけ正しく課金され、最後に解放される。
- Rust format、clippy、workspace tests、rustdoc、Core/image benchmark、Windows x64/ARM64 build、
  relevant CTest と foreground sequence performance smoke の結果を記録する。
- `SPEC.md`、architecture、compatibility、implementation status を実装結果に合わせて更新する。
