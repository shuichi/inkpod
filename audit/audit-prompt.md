# Inkpod コードベース厳密監査プロンプト

> 実行前提: Codex 5.6 Sol / Ultra。モデルとreasoning profileは実行環境で指定し、
> このプロンプト内から変更しない。
>
> このファイル全体を監査依頼として使用する。後続のユーザー指示が監査scope、
> baseline、出力先を明示した場合は、その項目だけを上書きする。

あなたはInkpodの独立したコードベース監査者である。実装担当者として振る舞わず、
現在のworking treeを根拠に、アーキテクチャ、所有権、再生可能性、非破壊性、
データ構造、テスト保証を厳密に監査し、証拠付きの報告書を作成せよ。

## 1. 監査目的

次の命題を前提事実として受け入れず、production code、公開契約、実行可能なtestから
個別に立証または反証する。

1. Inkpodの文書状態、画像処理、選択、履歴、永続化、入力解釈、render snapshotは
   Rust Coreが所有する。
2. C++/Win32はUI、OS adapter、thread/queue、file dialog、clipboard、DPI、
   accessibility、Direct2D/D3D/DXGI rendererに限定されたshellである。
3. production document mutationは、型付きのRust primitiveへ正規化される。
4. primitive control planeは固定幅のcall-by-value値、stable ID、または
   generation付きRust-owned object ID/opaque handleだけで構成される。
5. 可変長入力はprimitiveとは別のbounded data-planeでRust所有へ取り込み、
   commit時にはcanonical inline payloadまたはimmutable AssetIdへ確定する。
6. 同じreplay epoch、Genesis、assets、journal event列から、同じauthoritative Core stateと
   canonical bitmap/compositeを得られる。
7. Undo、Redo、Jump、Undo後のbranchは同じ永続journalで表現され、履歴は論理的に
   非破壊である。

「文書replay」と「アプリケーションsession全体のreplay」、および
「canonical bitmap一致」と「OS font/GPU antialiasingまで含む画面pixel一致」を混同するな。

## 2. 権限と変更境界

これは監査タスクであり、修正タスクではない。

- 読み取り、検索、git metadata/diffの確認、既存の非破壊test/build、報告書の新規作成を
  許可する。
- source、設定、既存文書、test、fixture、generated header、lockfile、benchmark、
  baseline、git index/branch/commitを変更してはならない。
- formatterの書込み実行、code generatorによるtracked file更新、dependency更新、
  package install、`git add`、commit、push、checkout、reset、cleanを行わない。
- network access、外部公開、PR、issue作成、外部messageは、別途明示許可がない限り行わない。
- build/testが通常のignored artifactを生成することは許容するが、tracked fileや未知の
  untracked sourceを生成するcommandは実行しない。
- test failure、docs drift、設計違反を見つけても修正しない。報告書へ記録する。
- 既存のdirty working treeはユーザー所有である。開始時と終了時の
  `git status --short`を保存し、削除、退避、復元しない。
- 許可された報告書以外の差分が監査中に増えた場合、対象外なら作業を継続してよいが、
  moving targetとして報告する。監査対象と重なる場合は結論を条件付きにする。
- subagentを含む全agentに同じread-only境界を適用する。最終報告書はroot agentだけが書く。

既定の成果物はrepository rootの`audit-report-YYYYMMDD.html`とする。日付はJSTを使う。
同名fileが存在する場合は上書きせず、`audit-report-YYYYMMDD-HHMMSS.html`を使う。
ユーザーが出力先を指定した場合はそれを優先する。報告書以外のsourceを変更してはならない。

## 3. 仕様の正本と証拠順位

最初にrepository全体へ適用される`AGENTS.md`を読み、その後に`SPEC.md`の関連節を読む。
判断の優先順位は、今回のユーザー指示、`AGENTS.md`、`SPEC.md`、実行済みの公開契約test、
production implementation、補助文書の順とする。

最低限、次を相互照合する。

- `docs/architecture.md`
- `docs/ffi.md`
- `docs/file-format.md`
- `docs/determinism.md`
- `docs/primitive-route-inventory.md`
- `docs/compatibility.md`
- `docs/implementation-status.md`
- `docs/core-benchmark-baseline.md`
- Cargo/CMake manifest、公開C header、CI、test source

証拠の強さは原則として次の順とする。

1. 実行に成功したproduction-path test
2. 実行に成功したpublic contract/integration test
3. production call pathを端から端まで追跡したimplementation evidence
4. architecture/source scanとlayout検査
5. prose document、comment、test名だけの記述

下位の証拠だけで`Verified`、`Pass`、または「すべて」と判定してはならない。
`docs/implementation-status.md`やroute inventoryの自己申告をimplementation evidenceの
代用にしない。

## 4. 実行手順

### 4.1 Baselineを固定する

最初に次を記録する。

- JST日時とtimezone
- repository absolute path
- branch、full HEAD SHA
- 開始時`git status --short`
- toolchain、target architecture、CMake preset一覧
- 調査対象directoryと明示的な除外
- 既存report、既存dirty path、監査中に変わり得る外部状態

監査対象は開始時のHEADとworking tree差分を合わせたsource snapshotである。dirty差分を
勝手に無視せず、同時に今回の監査が作った変更とも混同しない。

repository rootから、監査日より前の最新`audit-report-*.html`を探す。なければ最新の
`design-report-*.html`を探し、baseline reportとする。baselineは手掛かりであり正本ではない。
各既知findingを現行sourceで再検証する。

### 4.2 Ultraの並列性を監査レーンへ割り当てる

subagentが利用できる場合、available slotの範囲で少なくとも次の独立レーンに分割する。
slotが不足する場合は同じレーンを順番に実施する。

1. Rust Core: primitive、transaction、journal、Undo/Redo、replay、data model
2. C ABI: public header、Rust FFI、layout、validation、ownership、thread、panic/error
3. Win32: CoreHost、command route、C++ shadow state、queue、renderer、snapshot lifetime
4. Root: 要件抽出、persistence、test/build inclusion、反証、baseline比較、最終統合

各subagentはファイルを編集せず、nested agentを生成せず、次のschemaで結果だけを返す。

- claim
- production evidence
- contract/test evidenceと実行有無
- counterevidenceまたは許容例外
- production reachability
- confidence
- unresolved question

初回調査後、可能なら一つのagentをcriticとして再利用し、Critical/High候補と主要な
Pass判定の反証を試みる。root agentはCritical/High候補をproduction call siteから
terminal effectまで独立に再追跡し、subagentの結論をそのまま採用しない。

### 4.3 Source-derived inventoryを作る

前回reportの数値を流用せず、現在のsourceから次を再計数する。検索pattern、対象glob、
除外glob、算定方法をcommand ledgerへ残す。

- internal primitive catalog entry、PrimitiveId、schema、replayable/session-only
- 公開Rust Core mutation/query/lifecycle API
- C ABI mutation/query/data-plane/lifecycle export
- ABI-v3 opcodeとRust-owned object type
- production Win32 command ID、C ABI call site、CoreHost lane
- stable ID/revision newtypeとnamespace
- native format version、replay epoch、catalog digest、required section
- relevant contract/integration/smoke test

primitive inventoryは必ず次の二表に分ける。

1. Public boundary catalog: C ABI opcode/API、payload form、Win32 production call site
2. Internal canonical catalog: mutation、typed invocation、executor、procedure、journal/replay route

この二表を混ぜて、外部ABIの狭さをCore内部replay bypassと誤認したり、Core内部の
canonical化をboundary-call replay成立と誤認したりしてはならない。

### 4.4 End-to-end traceを行う

代表的な成功経路だけでなく、例外と反例候補を優先し、次を端から端まで追跡する。

`Windows command/input → CoreHost queue/side table → C ABI export → Rust FFI conversion →
PrimitiveRequest/CanonicalInvocation → transaction/apply → CanonicalProcedure → journal →
snapshot/persistence`

特に、queue record本体だけでなくtokenが指すside table/registryまで確認する。
wrapper、callback、lambda、path、pointerがtokenの先に隠れていないかを調べる。

### 4.5 検証を実行する

まずmanifest/presetを読み、実在するcommandだけを使う。依存downloadや環境変更が必要な
場合は実行せず、正確なblocking reasonを記録する。実行したcommandはcwd、profile/target、
exit code、passed/failed/skipped、要約をledgerへ残す。test sourceを読んだだけなら
`not executed`とする。

変更範囲と環境が許す限り、最低限次を実行する。

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test -p inkpod-core --test architecture
cargo test -p inkpod-core --test route_inventory
cargo test -p inkpod-core --test contracts
cargo bench --package inkpod-core --bench core_workflows -- --quick
cargo doc --package inkpod-core --all-features --no-deps
cmake --list-presets=all
cmake --preset <actual-windows-preset>
cmake --build --preset <actual-windows-build-preset>
ctest --preset <actual-windows-test-preset> --output-on-failure
```

rustdoc warning拒否は実行shellに合う`RUSTDOCFLAGS=-D warnings`相当を設定する。
Windows static boundary scripts、C11/C++20 header probe、ABI smoke、product smoke、
determinism/journal/native-format testsを実在するCTest/Cargo targetから確認する。

quick/full benchmarkは同じscenario、checksum、semantic counterを使う。wall-clockを
回帰根拠にする場合はbaseline文書のmachine/profile/warm-up/sample規則を満たすこと。
環境が一致しない測定を性能findingの決定的証拠にしない。

## 5. 必須監査マトリクス

各行を`Pass / Partial / Fail / Not evidenced`で判定し、代表的なproduction evidence、
test evidence、未確認事項を付ける。

| 領域 | 必須確認事項 |
|---|---|
| 仕様・version | SPEC/AGENTS、code constants、header、tests、file format、replay epoch、catalog count/digest、docsの一致。意味変更時のtop-level version/epoch更新。 |
| OS非依存 | `inkpod-core`、`inkpod-image`、`inkpod-format`へWin32、COM、D2D/D3D/DXGI/WIC、registry、DPI、UI-thread型が侵入していない。 |
| Rust domain ownership | document、project/cut/cell/frame/sequence、layer/plane、selection、palette、guide/grid、Light Table、vector、adjustment、history、savepoint、dirty、assetがRustにある。 |
| C++ shadow state | domainらしいC++ struct/vector/mapを列挙し、Core queryからの再構築、session/generation/revision binding、Undo/Redo/Open/Replace時のinvalidaton、mutation入力利用を確認。 |
| Primitive完全性 | 全production mutationがtyped canonical primitiveへ正規化され、直接document mutationの逃げ道がない。成功commitはexactly one procedure、no-op/invalid/cancel/stale/failureはzero procedureとなる。lifecycle/Genesis replacement、preview/stroke、history eventを別classにする。 |
| Control/data plane | primitive recordは固定幅値とRust-owned ID/handle。可変入力はbounded ingest後にRust-owned object/AssetIdとなり、pointer/path/callback/frontend ID/temp IDをprocedureへ残さない。resultのprocedure/state IDをboundary auditと相関できる。 |
| Transaction | private working state、base revision、checked ID reservation、明示一回publish。success/no-op/invalid/cancel/stale/overflow/allocation failureでrevision/history/journal/dirty/savepoint/ID/cacheが正しく進む、または全く進まない。 |
| Journal/history | Genesis + immutable Assets + CanonicalProcedure、Commit/HistoryMove/BranchCut、Undo/Redo/Jump、inactive branch保持、asset root、savepoint、cache release/rebuild、compactionを確認。 |
| Deterministic replay | live/replay semantics、pre/post digest、fixed numeric、ordered traversal、clock/locale/entropy/path/thread count/hash order/GPU非依存、全replayable primitiveのcoverage。 |
| Persistence | current-only format、META/GENS/ASST/PROC/EDITの権威、CKPTのcache性、staged open/replay、atomic save、recovery/autosave、malformed bounds/checksum/reference検査。 |
| Stable ID/revision | 意味別newtype、zero、namespace、lifetime、commit時消費、削除後非再利用、high-watermark、overflow、save/reopen/replay identity。name/index/generationによる代用も列挙。 |
| Pixel/tile/asset | typed PixelFormatとalpha、stride/overflow、sparse/lazy/COW tile、決定的iteration、content-addressed immutable asset、dedupe/retention、全面複製回避。 |
| Snapshot/render | immutable owned snapshot、document/view/render/preview revision分離、dirty/removed/invalidation、document座標、DPI非混入、GPU/font frontend解決、device-loss後のdocument保持。 |
| C ABI | versioned C ABI、repr(C)/fixed width、struct_size/flags/reserved、NULL/alignment/count/stride/capacity/enum/overflow、allocator symmetry、panic/exception containment、diagnostic lifetime。 |
| Ownership/thread | Core single-writer、Rust-owned handle generation/type、snapshot/task cross-thread規則、callback-under-lock禁止、queue saturation/close/stale/shutdown、全分岐のexact release。 |
| Win32 shell | command owner/CommandContext、dialog typed result、OS I/O、UI/DPI/accessibility、rendererだけを所有し、画像処理、selection、history、native codecを別実装しない。 |
| Performance/resource | FFI per-sample/pixel round-trip、全画面compose、Core/history clone、tile/cache reuse、bounded queue/cache、semantic counterとresource observability。 |
| Test assurance | public contract、property/state-machine、FFI negative、Windows production smoke、malformed/fuzz、determinism/golden、manual accessibilityの実行範囲。ignored/placeholderで失敗を隠さない。 |
| 文書の真実性 | `all`、`one executor`、`zero`、`complete`、`Verified`等の絶対表現がsource-derived全集合とtestで裏付けられる。 |

## 6. 誤検出を防ぐ反証規則

- raw pointer、`unsafe`、`std::function`の存在だけをfindingにしない。所有者、保持期間、
  thread、release、production reachabilityを追う。
- call中だけborrowし、完全検証後に復帰前deep-copyするbounded ingestionは、shared-lifetime
  violationではない。ただしprimitive control planeの非統一は別のarchitecture findingにできる。
- Rust-owned opaque immutable snapshot pointerは、lifetime、external synchronization、
  exactly-once releaseが成立すれば許容する。
- legacy C ABIを通ること自体はreplay bypassの証拠ではない。owned Rust requestへ変換され、
  canonical executor/journalへ入るかを確認する。
- C++のUI、path、clock、DPI、GPU stateはshell側に存在してよい。canonical procedureや
  authoritative resultへ混入してdeterminismを変える場合に限りfindingとする。
- test、example、smoke-only、dead code、未build targetをproduction findingに数えない。
  Cargo/CMake inclusionとproduction call reachabilityを確認する。
- testの欠如は実装bugと同一ではない。`Test gap`として分類する。
- 同じroot causeの多数のcall siteを別findingへ水増しせず、一findingとcoverage表へ集約する。
- search hitがないことを証拠にする場合、検索scope、pattern、除外glob、exit codeを記録する。
- architecture testは禁止依存の静的guard、route inventoryは集合と分類の一致であり、
  runtimeで「exactly one procedure」を生成する意味証明ではない。
- docsの将来方針、statusの自己申告、実装済み契約を区別する。
- 観測、推論、未確認を明示的に分ける。反証できない疑義はfindingではなく
  `Needs verification`へ置く。

## 7. Findingの証拠契約

各findingは次をすべて含む。

- stable finding ID
- severity
- confidence: `High / Medium / Low`
- reachability: `Confirmed / Conditional / Unverified`
- category: Safety / Correctness / Replayability / Architecture / Performance /
  Documentation / Test gap
- baseline比較status
- 破っている具体的なユーザー原則、AGENTS、SPEC requirement、または公開契約
- 現在の挙動と、期待される挙動
- production sourceのrepo-relative path、1-basedの狭い行範囲、symbol名
- upstream call siteからterminal effectまでの最短data/control-flow trace
- test file、exact test名、何をassertするか、実行済みか否か
- counterevidence、許容例外、発生条件
- user-visibleまたはarchitectural impact
- 最小の改善方向。実装はしない
- 完了を判定できるacceptance test

Critical/High findingは、少なくとも一つのproduction call siteとdownstream implementationを
端から端まで追跡し、許容されるownership/data-plane例外を反証した場合だけ採用する。
docs、comment、test名、grep hitだけではCritical/Highにしない。

### Severity

- **P0 / Critical**: production到達可能なUAF、double free、memory corruption、ABI UB、
  回復不能なdocument corruption、部分commit、replay検証を偽って誤状態をpublishする問題。
- **P1 / High**: Rust authorityの直接違反、production mutationのcanonical journal迂回、
  primitive control planeのclosed value/ID契約へのproduction-wideな違反、
  wrong-target/wrong-thread mutation、call後のcaller memory利用、同一入力でauthoritative stateが
  恒常的に変わる問題。
- **P2 / Medium**: boundary/executor二重化、重要invariantのenforcement不足、永続identity欠落、
  重要coverage gap、限定条件のrace、著しいscalability risk。
- **P3 / Low**: 局所hardening、observability、comment/version drift、低影響の保守性問題。
- **Strength**: 適合を支える設計と防御層。finding件数へ混ぜない。

severityはimpactとreachability、confidenceは証拠の確度であり、別軸として扱う。
「将来事故が起こり得る」だけでP0/P1にしない。

## 8. 前回監査との差分

baseline reportの各findingへ、次のいずれかのdispositionを必ず付ける。

- `Resolved`
- `Improved`
- `Unchanged`
- `Regressed`
- `New`
- `Not reverified`

同じroot causeには前回と同じfinding IDを使い、renumberしない。新規findingは
`ARCH-*`、`FFI-*`、`PRIM-*`、`HIST-*`、`DATA-*`、`PERSIST-*`、`RENDER-*`、
`WIN-*`、`TEST-*`、`DOC-*`のcategory prefixを使う。

`design-report-20260811.html`をbaselineに使える場合、同じroot causeには次のIDを維持する。

| ID | Baseline root cause |
|---|---|
| F-01 | pointer-free ABI-v3とgeneric closure/legacy mutation boundaryの分裂 |
| F-02 | adjustment layerのC++ cacheがCore revisionから再構築されない |
| F-03 | LightTableSwapWithActiveがnon-replayableでGenesis/historyをresetする |
| F-04 | Rust内部のcanonical transaction/publish実装が二系統 |
| F-05 | StableIdCursorが永続上限でchecked failureしない |
| F-06 | Project/Cut/Frame/Sequenceの永続typed IDが未実装 |
| F-07 | 全replayable primitiveのcatalog-driven conformance coverageがない |
| F-08 | snapshotに明示dirty/removed/invalidation情報がない |
| F-09 | code、header comment、docsのversion記述がdriftしている |

`Resolved`はimplementation evidenceと実行済みtestの両方がある場合だけ使う。baseline commitが
分かればgit diffを補助証拠にし、不明ならsource snapshot比較と明記する。

## 9. HTML報告書の契約

報告書は日本語のstandalone UTF-8 HTML5とし、CSSをinlineで含める。外部CSS、JavaScript、
CDN、font、画像へ依存しない。dark mode、print、high contrastを考慮し、statusを色だけで
表現しない。横長tableはscroll可能にし、長いpath/codeはwrapする。

最低限、次の順序で構成する。

1. Executive verdict。Rust authority、C ABI memory ownership、Win32 shell purity、
   Core semantic replay、boundary-call replay、whole-session replayを別々に判定する
2. 監査日時、HEAD、dirty state、environment、scope、除外
3. 前回監査との差分summaryと全finding disposition
4. Compliance matrix
5. architecture、thread、ownership、data-flow図
6. Rust domain data structureとstable-ID inventory
7. public boundary primitive inventory
8. internal canonical primitive/replay inventory
9. transaction、journal、Undo/Redo、non-destructive model
10. persistence、format、asset、save/open
11. snapshot、renderer、queue、resource model
12. C ABI ownership/data-flow matrix
13. C++ shadow-state候補の分類
14. Findingsをseverity、confidence、reachability順に表示
15. Strengths / positive controls
16. command ledgerとtest結果
17. Needs verification、limitations、moving-target差分
18. 優先度付きremediation roadmapとacceptance criteria
19. completion checklist

冒頭に現在のversion、epoch、catalog count/digest、ABI version、主要route countを置く。
各findingからproduction/test evidenceへ辿れるanchorを付ける。Pass判定にも代表的な
implementationとtest evidenceを付ける。

報告書内の各commandは、実際に実行したものだけを`passed`または`failed`と書く。
未実行commandは理由付きで`not executed`とする。cross-build、自動smoke、source scanを、
それぞれnative interaction、manual accessibility、runtime semanticsの代用にしない。

## 10. 完了条件

次をすべて満たすまで監査を完了扱いにしない。

- baselineの全findingにdispositionがある。
- 現在のcatalog/version/epoch/ABI/route countをsourceから再導出した。
- public Rust mutation、C ABI mutation、Win32 production routeを突合した。
- public boundary catalogとinternal canonical catalogを分離して評価した。
- 必須監査マトリクスの全行に証拠または明示的な未確認理由がある。
- Critical/High候補をrootがproduction pathで再検証した。
- 「すべて」「一つのexecutor」「完全非破壊」等の絶対表現に反例検索scopeがある。
- canonical Core/bitmap replayとGPU表示一致を区別した。
- findingごとにcounterevidence、confidence、reachability、acceptance testがある。
- test sourceの存在と実行成功を混同していない。
- 開始時と終了時のgit statusを比較した。
- 許可されたreport以外のsourceを変更していない。
- standalone HTMLのUTF-8、inline CSS、anchor、tag balance、外部依存なしを検証した。
- 未実行test、環境制約、manual-only項目、moving targetを隠していない。

終了時の会話報告は、reportへの絶対path link、verdict、severity別finding数、最重要finding、
実行した検証、未検証事項、変更ファイル一覧だけを簡潔に示す。commit、push、PRは行わない。
