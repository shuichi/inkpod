# inkpod 性能監査 — 2026-08-28

## 1. 結論と読み方

**実装・テスト・仕様・性能契約は変更していない。本監査が追加した監査成果物は、この報告書と[実測根拠 JSON](C:/Users/shuichi/GitHub/inkpod/audit/performance-audit-20260828-evidence.json)だけである。**

利用者の操作から UI → Core → Renderer を追い、14 件の測定候補を整理した。まず確認する価値が高いのは、view-only 操作からの同期 Core 呼び出しと全 pane 更新、Layer thumbnail の作り直し、選択 preview の同期 Renderer 呼び出し、二分割 focus 移動時の同一 route の GPU cache 破棄、局所 filter preview の全 tile 無効化である。**候補の優先度は次に測定する順序であり、実測した寄与率や確定したボトルネックの順位ではない。**

今回、新たに確認した事実は次のとおり。

- 既存 Rust テスト 61 件、CMake の静的境界テスト 3 件が通過した。
- 変更していない Core quick benchmark を、各系列「warm-up 1 process＋測定 5 process」の独立 2 系列で実行した。9 scenario の checksum・意味カウンターは維持された。
- `pan_zoom_snapshot` の中央値は **1.200958 ms / 1.072292 ms**。既存 ARM64 routine 上限 **1.05 ms** の超過を両系列で観測した。`dirty_tile_rebuild` は **2.124041 ms / 2.026042 ms** で既存範囲内だった。
- 上記 Core 計測は UI・Renderer を通らない。**14 候補の遅延量や、今回の pan/zoom 上限超過の原因を証明するものではない。**

根拠の区分は次のように統一する。

| 区分 | 意味 |
| --- | --- |
| 今回の実測 | 本監査中に実行し、終了コード・出力を保存した測定・テスト |
| 既存の実測記録 | repository に保存済みの測定。今回の再実測とは区別する |
| 静的事実 | 呼び出し元と処理本文から確認した経路・処理・条件 |
| 性能仮説 | 静的事実から疑う不要作業・待ち・影響。発生頻度、Release での実コスト、改善効果は未測定 |

A01〜A14 はいずれも「静的事実＋性能仮説」である。A01→A02/A03/A04、A08→A07 のように重なる経路があり、コストを足し合わせて見積もらない。

## 2. 対象・不変条件・調査方法

### 2.1 対象時点

- repository: `C:\Users\shuichi\GitHub\inkpod`
- 基準 HEAD: `ecb2d30b46b6584ae6ad9eb07bfec1c6b40da305`
- 開始時の `git status --short` は空。
- 調査中に外部の変更として `apps/windows/app/app_smoke.cpp`、`docs/compatibility.md`、`docs/implementation-status.md` の差分を検出した。本監査ではこれらを編集・復元していない。
- ソース参照は監査時の worktree。上記 smoke の行番号は差分後に照合した。Rust benchmark のソースは変更されておらず、Cargo の `fresh=true` と実行ファイル SHA-256 を根拠 JSON に記録した。
- 読み取り、ビルド・テストは PowerShell profile を読み込まない `login:false` で実行した。commit、push、設定変更、ユーザー画像の操作はしていない。

### 2.2 維持する契約

[SPEC の性能契約](C:/Users/shuichi/GitHub/inkpod/SPEC.md:609)、[連番と共有 I/O](C:/Users/shuichi/GitHub/inkpod/SPEC.md:193)、[既存 benchmark 基準](C:/Users/shuichi/GitHub/inkpod/docs/core-benchmark-baseline.md:315)を前提にした。

| 項目 | 本監査で緩和・置換しない条件 |
| --- | --- |
| Render cache | canonical `revision-max`、cache hit の payload 非走査。hash、clone generation、epoch、tombstone、negative cache の追加を前提にしない |
| 全無効化 | opacity、visibility、order、main-line color、color-check 等、式の外にある metadata の whole-cache invalidation を維持 |
| 連番 source cache | active・準備中・lease を含め CPU/GPU 各 8 source・128 MiB。decoded 8 GiB/GPU 512 MiB の内数。隣接準備は左右最大 2 件 |
| Shared I/O | 10,000 images、encoded 一 file 512 MiB、encoded 合計 8 GiB、decoded 合計 8 GiB、native/recovery bounded streaming 1 GiB。予約と最終 lease 解放までの計上を維持 |
| Thumbnail | application-wide budget を維持。現在の実装は既定 64 MiB、設定可能上限 256 MiB（[thumbnail_cache.h](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/thumbnail_cache.h:83)） |
| 正しさ | stable owner/ID/generation、単一 writer、順序、stale/cancel/failure atomicity、Undo/Redo、dirty/savepoint、保存 authority を維持 |
| 画質 | native depth、alpha、rounding、sampling、合成順、表示解像度、VSync を変えない |
| 性能評価 | 既存 workload/harness/envelope を変更しない。新しい補助測定を既存 envelope の合否に混ぜない |

`revision-max` の既知の衝突・masking、透明結果を保持しないことは明示された契約であり、今回「新たな性能欠陥」として挙げていない。

### 2.3 主要操作の経路と確認範囲

| 利用者の操作 | 追跡した経路 | 主な観察 |
| --- | --- | --- |
| 鉛筆・ブラシ・消しゴム | Canvas input → UI enqueue → CoreHost stroke → FFI/Core stroke → snapshot → Renderer upload/Present | Begin/Append 非同期・preview cadence を確認。End の状態通知と dirty tile allocation は A02/A03/A07 |
| pan・wheel zoom・FIT | Canvas gesture → ViewController → CoreHost Invoke → view apply → snapshot → Renderer | tile 再利用はある。UI 同期待ちと pane 更新が A01〜A04 |
| 選択矩形・範囲塗り・floating | pointer → geometry/floating preview → Core/Renderer → 確定編集 | Renderer への同期往復が A05。fill 演算の全条件は未網羅 |
| Filter preview・確定・Cancel | UI effect task → CoreHost → FFI → Core preview → image filter → snapshot → Renderer | A08/A09。正しい同一 base・取消・Undo 契約を照合 |
| 調整レイヤー・Light Table | UI property/edit → FFI → Core document edit → CPU tile compose → Renderer | A10/A11。現行通常経路は CPU 合成 |
| layer/plane 選択、色・tool 操作 | command → Core/published state → Tree/Color/menu/tab 更新 | A02/A03。geometry-only resize と data refresh を分離 |
| 通常連番の前後移動 | keyboard/Sequence → bounded intent queue → Core source activation → final snapshot → pristine GPU bank → Present | 最近の再利用対策を確認。既存 warm 実測を参照。cold/recovery は別条件 |
| 二分割・tab/group focus | activation → ApplicationHost → Canvas bind → Core view → snapshot | 内容・route 不変の group focus でも GPU 全破棄する経路が A06 |
| Cut の Sequence 表示 | Cut session cache → RefreshSequencePane → ThumbnailCache → list | warm thumbnail の再読込は防止済みだが、cache 削除・copy/reinsert が A04 |
| Save/Save As・autosave・export | UI FileIoController → Core owner capture → Rust I/O worker → encode/検証 → owner apply | capture/保存時の反復作業が A12/A13。UI が大きな encode を直接行うとは評価しない |
| Open・自動連番・Reference/Subpalette | path-only job → shared manager/locks/cache → decode/prepare → owner apply → snapshot | bounded workers、resident navigation を確認。予算圧迫時の LRU は A14 |
| resize・DPI・device loss | UI geometry → Renderer target resize/recovery | 通常 resize は tile cache を保つ。device loss に必要な GPU 破棄は候補にしない |

## 3. 実測・テストの結果

### 3.1 今回の Core quick 計測

環境は Windows 11 Pro build 26200、Parallels ARM VM、4 logical CPU、約 8 GiB RAM、電源 scheme `Parallels`。rustc/Cargo 1.97.1、LLVM 22.1.6、`aarch64-pc-windows-msvc`、Release/optimized、static CRT。OS/toolchain/power は既存 ARM64 reference と照合した。物理ホスト型、thermal 状態、OS の背景負荷は連続統制していない。

準備と実行は次のとおり。compile 時間は scenario の計測に含まない。

~~~text
cargo bench --locked --offline --package inkpod-core --bench core_workflows --no-run --message-format=json
& '.\target\release\deps\core_workflows-c9aeeeab1b9c0e36.exe' --quick
~~~

各系列で実行ファイルを 6 回、別 process で起動し、index 0 を warm-up とした。測定 5 回は全て採用。1 系列目の上限超過後、既存ルールどおり独立した 2 系列目を採った。監査側のビルド・テストは重ねず、subagent の shell 読み取りも停止した。採択結果に合わせた再試行・sample 除外はしていない。

| 保護対象 | 既存範囲 | 第1系列中央値 | 第2系列中央値 | 判断 |
| --- | ---: | ---: | ---: | --- |
| quick pan/zoom、2,048 pair | 0.70–1.05 ms | 1.200958 ms | 1.072292 ms | 両系列で上限超過。原因調査対象 |
| quick dirty tile、32 edits | 1.8–2.4 ms | 2.124041 ms | 2.026042 ms | 両系列で範囲内 |

pan/zoom の全測定値（ns、実行順）:

~~~text
第1系列: 1200958, 1588292, 1181541, 1824542, 981583
第2系列: 1668000, 1081291, 1072292, 1056667, 969250
~~~

既存ルールの「独立 2 系列で上限超過」という再現条件は満たした。ただし、**この比較だけで code regression の原因、特定 commit の責任、利用者の UI 遅延を特定しない**。第2系列の超過は約 2.1% であり、背景負荷や物理ホスト条件を固定した比較が原因切り分けに必要である。契約の上限は変更しない。

全 12 process は exit 0。9 scenario の checksum と、iterations/input/output/reuse/revision/history/success/failure の値は一致した。`pan_zoom_snapshot` は 4,096 操作、8/8 tile 再利用、document revision 9/history 8 を維持。`dirty_tile_rebuild` は rebuild 32/reuse 224 を維持。`batch_preview` の `failures=1` は既存の意図した negative case であり、実行失敗ではない。

warm-up、全 scenario の全 sample、意味カウンター、時刻、実行ファイル hash は[実測根拠 JSON](C:/Users/shuichi/GitHub/inkpod/audit/performance-audit-20260828-evidence.json)に保存した。残り 7 scenario は対応する基準環境・比較対象を揃えた性能判定をしていない。

**この計測の不足:** Core API だけの計測であり、UI の message pump、pane 更新、Renderer allocation/upload、GPU/Present は含まない。今回の上限超過と A01〜A14 の因果関係は未検証。最小の追加確認は、同じ binary/workload/環境で CPU sampling と実行順の背景負荷記録を行い、`pan_zoom_snapshot` 内の scalar metadata/候補 tile 列挙等とスケジューリング時間を分けること。payload 非走査を緩めて原因調査しない。

### 3.2 今回実行した既存テスト

| Command（全て profile 無読込） | 結果 | 証明する範囲 |
| --- | --- | --- |
| `cargo test --release --locked --offline --package inkpod-core --lib snapshot::tests -- --test-threads=1` | 11 passed | revision-max、payload 非走査、tile 再利用、source preparation、pixel 同値 |
| `cargo test --release --locked --offline --package inkpod-core --test contracts file_io:: -- --test-threads=1` | 25 passed | 非同期 I/O、save authority、fence、stale/cancel、resident reference、lease |
| `cargo test --release --locked --offline --package inkpod-io --test manager -- --test-threads=1` | 16 passed | shared cache、LRU、予約上限、並行・排他、取消 |
| `cargo test --release --locked --offline --package inkpod-core --test contracts effects:: -- --test-threads=1` | 9 passed | preview base/Cancel/Undo/Redo、adjustment、worker atomicity |
| `ctest --preset windows-x64-release -R '^inkpod_windows_(core_host\|renderer_host\|filter_preview)_boundaries$' --output-on-failure` | 3 passed | 現行ソースの静的境界チェック。Windows runtime 性能テストではない |

Rustup がユーザーディレクトリの canonicalize 警告を出したが、上記 Cargo command は全て exit 0 を得た。詳細は JSON に保存した。

コード変更のない監査に範囲を限定し、full workspace test、Clippy、rustdoc、full benchmark、CMake 全 configure/build/CTest、可視 native performance smoke は今回再実行していない。外部作業中の smoke ソースと既存 Windows binary の一致を性能結果の根拠にしていない。

### 3.3 既存の実測記録（今回の結果ではない）

[連番性能記録](C:/Users/shuichi/GitHub/inkpod/docs/sequence-switch-performance.md:43)には、同一の生成 fixture に対する最適化前後の CPU 区間、未編集 1754×1240 TGA の foreground warm 往復 640 回が保存されている。

- 前回の foreground 系列: process ごとの p95 の中央値は AB **2.722458 ms** / ABC **2.870625 ms**、最大 sample **4.771167 ms**。
- warm switch は snapshot 1 回、read/decode/upload 0。CPU/GPU 各 3 source、26,099,520 bytes を保持。
- 別の background run は AB/ABC p95 **295.185417/205.742458 ms**。background で遅い例は実測されているが、OS/driver の原因は確定していない。
- 当時の Core pan/zoom は独立中央値 **1.076875/1.024583 ms** で、第2系列では上限超過が再現していなかった。今回の 1.200958/1.072292 ms とは別系列である。

これらを cold open、cache 追放後、編集済み recovery、全 tab/group、全入力の保証に拡張しない。CPU 個別区間や keyboard 起点の重なる区間を加算しない。成功 `Present` の戻り時刻は物理 scanout 時刻ではない。

## 4. 測定候補一覧

| ID | 次の測定優先度 | 利用者の操作・候補 |
| --- | --- | --- |
| A01 | 高 | pan/wheel の同期 Core 呼出しと view-only からの全 pane 更新 |
| A02 | 高 | 無変更 layer を含む全 thumbnail の同期生成・copy |
| A03 | 中 | 無変更 menu/tool/Color lists/全 workspace tab の再設定 |
| A04 | 中 | Cut Sequence の warm thumbnail cache 全削除・再登録 |
| A05 | 高 | 選択・範囲塗り preview の Renderer 同期往復 |
| A06 | 高 | 二分割 focus 移動で同一 route の GPU cache 全破棄 |
| A07 | 中 | dirty tile の転送ごとの bitmap 新規確保 |
| A08 | 高 | 局所 filter preview の全 composite cache 無効化 |
| A09 | 中 | 小選択 Unsharp の未使用の選択外 blur 出力 |
| A10 | 中 | adjustment 合成 pixel ごとの定数検証・curve vector clone |
| A11 | 中 | Light Table 合成 pixel ごとの同一回転定数計算 |
| A12 | 高 | Save capture の Core 複製と直後の表示 cache 破棄 |
| A13 | 中・条件付き | native 保存ごとの独立 asset copy・全 journal 再検証 |
| A14 | 中・予算圧迫時 | shared I/O LRU の反復全走査・失敗時の cache 消失 |

## 5. 候補の詳細

### A01 — pan/wheel が Core 完了を同期で待ち、view-only でも全 pane を更新する

**経路・静的事実:** [Canvas SendGesture](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:3651) → `kCanvasViewGesture` → [ApplyView](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:7847) → [ViewController::Apply](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/tools/view_controller.cpp:14) → `CoreHost::Invoke(..., true, true)` → [future.get](C:/Users/shuichi/GitHub/inkpod/apps/windows/app/core_host.cpp:742)。[ProcessAdapter](C:/Users/shuichi/GitHub/inkpod/apps/windows/app/core_host.cpp:1797) は operation、document/editor metadata の再取得、snapshot 構築・提出の後で completion を返す。さらに [StateChanged 受信](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:21176)から editor/Tree/LightTable/Sequence/Subpalette/Color/menu が更新される。`RefreshEditorPresentation` の既定経路も editor state を同期再取得する。

**発生条件:** 通常 pan/wheel/FIT 等の view 操作。Begin/Append stroke 全般を同じ同期経路とは扱わない。stroke は End 成功時に状態通知を出す。viewport resize は refresh=false。未処理の同 session/generation の通知は coalesce されるため、「wheel 1 件＝pane 更新 1 回」とは断定しない。sequence activation pending 中には全更新を避ける早期 return がある。

**必要／不要の疑い:** view 順序、zoom anchor、正しい snapshot、zoom 表示更新は必要。UI が Core queue と snapshot 提出まで待つこと、document/editor が不変でも document 向け pane を総更新することは疑わしい。**この Invoke が Present 完了まで待つと確認したわけではない。**

**テストと不足:** [SnapshotPublicationKeepsInputResponsive](C:/Users/shuichi/GitHub/inkpod/tests/windows_core_host.cpp:249)は cached getter/enqueue を守るが、実 ViewController の UI 待ちを測らない。[native wheel loop](C:/Users/shuichi/GitHub/inkpod/apps/windows/app/app_smoke.cpp:16433)は SendMessage と Renderer idle 待ちを行い、loop 自身には post された StateChanged を pump する処理がない。他の再入経路で何件処理されたかは未測定であり、既存 wheel 結果を pane 更新込みの所要時間とみなせない。

**最小測定:** 同じ可視文書の通常 message loop で wheel/pan を行い、ETW CPU/Wait Analysis で UI future wait、Core operation/metadata/snapshot、通知後 pane 更新を分離する。Layer pane 表示/非表示を対照にし、既存 sample/snapshot/Present/queue counters を併記する。

**改善リスク:** 非同期化による gesture 順序、stale context、stroke 開始 transform、queue 上限の破壊。通知の絞り込みによる dirty/Undo/enable/pinned pane の更新漏れ。要件: `VIEW-001/002`、`PERF-001`。

### A02 — 全 layer thumbnail を作ってから cache に照合する

**経路・静的事実:** A01 の通知、または layer 選択 → [RefreshTreePane](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:1843) → [LoadTree](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/panes/document_panes.cpp:69)。同期 Invoke 内で全 layer を列挙し、Layer pane 可視時は各 layer の thumbnail を生成する。[FFI](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-ffi/src/document_edit/tree.rs:357) → [Core thumbnail](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/thumbnail.rs:48) は毎回 buffer を作り、各出力 pixel を 4×4 sample で計算。caller へ copy 後、[C++ で checker/BGRA 変換](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/panes/document_panes.cpp:18)。cache は計算後に [Put](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/thumbnail_cache.cpp:58)し、同 key でも全 pixel 比較を行う。

**発生条件:** Layer pane 可視中の delivered StateChanged、layer/plane 選択、明示 refresh。view-only は同 revision でも再生成する。thumbnail revision は document 全体の revision なので、一 layer の編集でも無変更 layer が新 key になる。旧 key は LRU eviction まで残り得る。非表示時に thumbnail 生成を skip する保護はある。

**必要／不要の疑い:** 変更 layer の画像と選択・編集可能 state の更新は必要。無変更 layer の sampling、allocation、FFI copy、色変換、旧 revision entry の重複は再利用可能な疑いが強い。80×60×4×64 layer＝1,228,800 bytes（約 1.17 MiB）の一巡 payload は**上限からの計算例で、実測 copy 量ではない**。Layer list 自体も data refresh で再構成されるが、geometry-only resize とは別経路である。

**テストと不足:** [layer thumbnail 契約](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/tests/contracts/document_selection.rs:569)と [ABI test](C:/Users/shuichi/GitHub/inkpod/tests/abi_smoke.cpp:835)は内容・寸法を守る。view-only/無変更 layer の生成回数、obsolete key の量、list reset 回数の性能ゲートはない。

**最小測定:** 1/16/64 layer で「zoom＋通知処理完了」「layer 選択だけ」「一 layer の 1 pixel edit」を分ける。FFI thumbnail、PopulateList の tracepoint count、ETW allocation/CPU、thumbnail entry/bytes/eviction の増分を採る。tracepoint の入った時間は合否判定に使わない。

**改善リスク:** hidden layer thumbnail、main-line 色、各 plane の順序/opacity/visibility、RGBA16、checker、寸法への依存を取りこぼせない。render の revision-max を別用途の thumbnail key へ安易に流用しない。既存 budget を維持する。要件: `DOC-002/003`、`RENDER-001`、`PERF-001`。

### A03 — menu 更新から Color list と全 workspace の tab まで再設定する

**経路・静的事実:** [UpdateMenuState](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:7676) → [全 command state 適用](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/command_state.cpp:729)、[tool button 更新](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/dialogs/tool_palette.cpp:569)、`RefreshDockPaneViews`、shortcut label、status、DrawMenuBar。[Color pane](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/panes/color_dock_pane.cpp:2841)は palette/chart/name 配列を copy し、PopulateLists（2589 行）で list を reset/repopulate。表示 item 数は bounded だが元配列 copy は全件。[UpdateDocumentTabLabels](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:7420)は全 workspace/group/view を列挙し、同 caption でも TabCtrl_SetItem 等を行う。

**発生条件:** A01 の通知、tool/色/選択・view toggle、activation、command 完了等。通常 pointer 移動全件に起きるとは評価しない。

**必要／不要の疑い:** command state を一度計算して共通適用する設計は適切。必要なのは変わった state/label/control の反映。zoom だけで Color 配列と list、他 workspace の同 tab caption、無変更 shortcut label、tool invalidation を再処理する点が疑わしい。GPU 転送とは別の UI CPU/GDI 作業である。

**テストと不足:** [command state tests](C:/Users/shuichi/GitHub/inkpod/tests/windows_command_state.cpp)は state 一致を確認する。geometry smoke は resize の reset 防止を確認する。無変更 data refresh の Win32 message 数、他 workspace への更新数は測らない。

**最小測定:** 通常 message loop の一回の view-only 操作について UpdateMenuState、LB_RESETCONTENT、TCM_SETITEMW、WM_SETTEXT、InvalidateRect の回数を stack とともに記録。workspace/tab 数、Color pane 表示有無を対照にする。

**改善リスク:** locale、theme/high contrast、DPI、menu 再生成、shortcut profile の切替で必要な再適用を飛ばす危険。Color 編集中値、selection/focus/scroll、UIA/MSAA text、複数 view の dirty caption を維持する。要件: `WIN-001`、`WORKSPACE-001`、`VIEW-004`。

### A04 — Cut Sequence の warm thumbnail を全削除・copy・再登録する

**経路・静的事実:** [RefreshSequencePane の Cut 分岐](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:2251)は最初に `Thumbnails().RemovePane`。全 member の `thumbnail_rgba` を [deep copy して Put](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:2293)する。[RemovePane](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/thumbnail_cache.cpp:173)は該当 entry を erase し、Sequence invalidation generation も進める。一方 [LoadCutMemberThumbnail](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:2153)は有効な member thumbnail を早期 return で再利用する。

**発生条件:** Cut を開いた状態の Sequence refresh、view-only 通知、metadata/membership 更新等。Cut 分岐の先頭には pane visibility guard がない。通常 sequence には別の同 catalog 軽量経路がある。

**必要／不要の疑い:** 変更 member の正しい画像、順序・選択・名前の更新は必要。warm 無変更 member の全 cache erase/copy/reinsert は不要の可能性が高い。**warm で native を毎回 reopen するわけではない。list も必ず reset されるわけではない。** cold thumbnail の Core create/open/thumbnail/destroy は別条件であり、別集計が必要。

**テストと不足:** [RunCutWorkflowSmoke](C:/Users/shuichi/GitHub/inkpod/apps/windows/app/app_smoke.cpp:15512)は thumbnail/identity/順序/保存の正しさを確認する。通常 sequence の cache 安定性 test はこの Cut 分岐を保証しない。erase/reinsert 後は entry/bytes が同じにもなるため、resident bytes だけでは churn を見逃す。

**最小測定:** 16/256 member の全 thumbnail を warm にして zoom を一回。RemovePane/Put/cold-open 分岐の count、invalidation generation、allocation stack を比較する。cold と warm を分離し、reopen/read の有無も確認する。

**改善リスク:** membership、同じ CellId の別 UUID、source 更新、pane/session generation、close、eviction 後の欠損を誤共有しない。全 Cut 画像の常駐や budget 拡張を前提にしない。要件: `CUT-001`、`SEQ-001`。

### A05 — 選択・範囲塗り outline 更新が Renderer と同期往復する

**経路・静的事実:** `kCanvasStrokeReady` → `UpdateSelectionGeometryPreview/UpdateFillGeometryPreview` → [GestureDocumentPoints](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:8371) → [GetDocumentBounds](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:4424) → [Invoke/future.get](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:2446)。続いて outline を [SetGeometryPreview](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:4469)で別の同期 Invoke。Renderer は [preview 更新後](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:3319)、ready なら D2D 描画と [Present(1,0)](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:528)を終えてから completion を返す。floating preview も類似経路。

**発生条件:** 矩形・楕円・lasso 選択、非 seed の範囲塗り、floating transform の preview 変更。別 Canvas の upload が queue 前方にあると bounds 照会も待つ。

**必要／不要の疑い:** 表示 transform に整合する座標、最新 outline、end/cancel 順序は必要。pointer ごとの bounds 照会と preview 提出で UI が二度 owner queue に同期する必要性は疑わしい。ただし **通常 preview の readiness は 0 ms probe** であり、「常に 100 ms 待つ」ではない。非 ready 時は pending frame を残して戻る保護がある。

**テストと不足:** [VerifyPreviewPublicationAndFramePermit](C:/Users/shuichi/GitHub/inkpod/tests/windows_renderer_host.cpp:573)は no-op、invalid、pixel 復元、permit 保持を検証するが、呼出しを [別 thread の async](C:/Users/shuichi/GitHub/inkpod/tests/windows_renderer_host.cpp:237)から行って HWND owner は pump する。実 UI handler の連続入力待ちを測らない。

**最小測定:** 固定件数・一定間隔の選択 drag で handler、bounds Invoke、preview Invoke、Renderer queue/D2D/Present を ETW/QPC で分離する。1 Canvas warm と、別 Canvas が upload 中の条件を比較。sample/end/cancel、outline、revision 不変、確定選択を同時記録する。

**改善リスク:** 表示中と新しい transform の混同、stale route、end/cancel 逆転、queue 飽和時の sample 欠落、sequence fence 破壊。単純な入力破棄を伴う非同期化は不可。要件: `SEL-001`、`FILL-001`、`VIEW-001`、`PERF-001`。

### A06 — 二分割の focus 移動だけで同じ route の GPU cache を破棄する

**経路・静的事実:** 別 group tab の `NM_SETFOCUS` または Canvas activation → [ActivateDocumentTab](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:1349) → [ActivateDocumentView](C:/Users/shuichi/GitHub/inkpod/apps/windows/app/application_host.cpp:898)。全体 active view が違うので fast path を外れる。[BindDocumentCanvas](C:/Users/shuichi/GitHub/inkpod/apps/windows/app/application_host.cpp:1053)は `changed=false` でも bind する。Renderer の [Bind 処理](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:3243)は無条件 [ClearSnapshot](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:723)し、tile cache、retained source bank、presentation 記録を破棄する。次の snapshot で upload が必要になる。

**発生条件:** 同じ文書の 2 view を 2 group に表示・warm 後、編集・resize なしで focus を往復。destination の session/view/generation/Canvas route は不変でも、全体 active view が変わる条件に絞った候補である。

**必要／不要の疑い:** active group、入力 target、Core view、pane projection の切替は必要。同じ route の有効 GPU payload 全破棄は再利用を失う疑いが強い。通常の同 group tab A↔B の異なる namespace まで「clear を除去すればよい」と一般化しない。

**テストと不足:** [二分割 focus smoke](C:/Users/shuichi/GitHub/inkpod/apps/windows/app/app_smoke.cpp:13036)は focus/flip/target を確認するが upload 増分を確認しない。[generation 変更 rebind test](C:/Users/shuichi/GitHub/inkpod/tests/windows_renderer_host.cpp:930)は全 resource clear を要求しており、こちらは維持が必要。同一 route と generation 変更の区別が不足する。

**最小測定:** 2 Canvas を warm 後に focus 32 往復。surface ごとの upload count/bytes、gpu bytes、source bank 数、snapshot 数、最初の成功 Present を採り、同じ group 再 focus を対照にする。pixel、flip、入力 target、close/rebind の安全性も確認。

**改善リスク:** same route と same contents は同義でない。recovery、文書交換、新 view 未 bind、preview cancel、stale snapshot、sequence fence を保つ。一般 tab の保持へ広げる場合も既存 GPU/sequence budget を守る。要件: `VIEW-004`、`WORKSPACE-001`、`PERF-001`。

### A07 — dirty tile の GPU 転送で bitmap を毎回作り直す

**経路・静的事実:** stroke → [CoreHost::ProcessStroke](C:/Users/shuichi/GitHub/inkpod/apps/windows/app/core_host.cpp:1683) → Core snapshot の変更 tile → [PrepareTileCache](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:2034)。ID/revision/dimensions 一致は再利用するが、不一致は [CreateBitmap](C:/Users/shuichi/GitHub/inkpod/apps/windows/renderer/canvas.cpp:2092)で新規 bitmap を作り、旧 entry を置換する。

**発生条件:** 同じ tile の繰り返し編集で複数の preview/commit snapshot が Renderer に届く場合。寸法・format が同じでも revision が変われば作り直す。

**必要／不要の疑い:** **変更 tile の upload は必要で、契約にも沿う。** 疑うのは backing allocation の反復だけである。in-place update より新規 bitmap の方が GPU 待ちを避けられる可能性もあり、速くなるとは断定できない。

**テストと不足:** [VerifyFirstSequenceEditTileReuse](C:/Users/shuichi/GitHub/inkpod/tests/windows_renderer_host.cpp:950)は 2 tile 中の変更 1 tile だけの upload と総 GPU bytes を検査する。現 counter は allocation/replacement と backing 再利用を区別しない。16 stroke/544 sample の既存 native drawing は burst→final Present で、実 pointer 周期の preview allocation を分離しない。

**最小測定:** 既存 drawing を graphics API/CPU allocation stack 付きで観測し、CreateBitmap 数・時間、upload bytes、resident/peak bytes、Present を分離。補助診断で同じ tile の固定 32 edit を用い、初回確保と置換を区別する。

**改善リスク:** 使用中 GPU resource の更新待ち、前 frame への影響、失敗後の部分表示、pristine bank の破壊、予算と実 peak の不整合。alpha/nearest sampling/画質は変えない。要件: `RENDER-001`、`PERF-001`。

### A08 — 小さな選択への filter preview でも合成済み tile を全破棄する

**経路・静的事実:** [QueueInteractiveFilterPreview](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:6066) → [EffectsController の非同期 task](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/effects_controller.cpp:79) → [FFI update](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-ffi/src/effects/task_filter.rs:305) → [Core preview](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/effects/preview.rs:22)。begin/update/Cancel がそれぞれ 59/128/147 行で render cache を clear。[snapshot](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/snapshot.rs:585)は cache miss の座標を再合成して新 tile revision を発行する。

**発生条件:** 多 tile の文書で、小選択の preview 開始・parameter 更新・Cancel。恒等 filter/同 parameter の成功処理も clear に到達する。

**必要／不要の疑い:** 同じ base からの再計算、影響 tile の更新、Cancel 完全復元は必要。影響しない tile の buffer 破棄、再合成、新 revision、後続 upload は不要の可能性。追加 upload の実量は未測定。

**テストと不足:** [effects 契約](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/tests/contracts/effects.rs:40)は Cancel、同一 base、一 Undo、failure atomicity を守り、今回 9 件が通過。小選択の前後で未変更 tile の payload/revision/rebuild/upload を追う test はない。

**最小測定:** 2〜4 個の非透明 tile で、一 tile 内の 1 pixel selection を確定後 warm。begin→update→Cancel の tile ID/revision/payload pointer/checksum を比較。恒等 preview も対照にする。時間は別の bounded 512² fixture で測る。

**改善リスク:** 単純な clear 削除は不可。高い preview revision から live への復元、削除 tile、selection/Light Table masking、複数 view を扱う必要がある。metadata の必須全無効化と canonical 式は変更しない。要件: `FILTER-PREVIEW-001`、`FILTER-001/002`、`PERF-001`。

### A09 — 小選択 Unsharp が使わない選択外 blur 出力を作る

**経路・静的事実:** A08 の filter task → [Core filter helper](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/effects/helpers.rs:99) → image filter → [unsharp_progress](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-image/src/edit/filter.rs:397)。selection を受け取るが、408 行で `blur_progress(source, None, ...)`。blur は全画素と各近傍を処理する。後段 410〜434 行は selection 外を skip し、soft を読むのは選択内だけ。

**発生条件:** 大 plane の小 selection に Sharpen Weak/Strong または Unsharp Mask。

**必要／不要の疑い:** 選択内の blur 値には選択外を含む近傍 source の読み取りが必要。不要と疑うのは**選択外の blur 出力そのもの**の計算と materialization である。source を selection bounds で切る案は境界を変えるため不可。

**テストと不足:** [image filter tests](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-image/tests/unit/edit.rs:8)は depth/alpha/edge、同 384 行以降は小 fixture の catalog を確認。ROI の soft 出力数、source read 数、tile allocation、halo を含む完全一致の性能 test はない。

**最小測定:** 固定 512² RGBA8/16、radius=2、同 amount/threshold で全選択、中央 16²、画像端 16² を比較。CPU sampling で blur と後段を分離。必要なら後続の診断計装で「soft 出力数」と「source 近傍 read 数」を別々に数える。

**改善リスク:** binomial kernel、premultiplied RGB/alpha、整数 rounding、端 clamp、selection、progress/cancel、既存 work bound を維持。処理領域に合わせた上限緩和を前提にしない。要件: `FILTER-001`、`FILTER-PREVIEW-001`。

### A10 — adjustment の定数検証と ToneCurve clone が pixel ごとに行われる

**経路・静的事実:** adjustment create/update または既存 adjustment のある文書の edit → [compose_tile](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/snapshot.rs:1512) → pixel×adjustment layer ごとに [apply_adjustment](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-image/src/edit/filter.rs:93)。毎回 validate し、ToneCurve は `points.clone()` を伴う一時 Filter を作る。curve validation は point の順序等も走査する。

**発生条件:** cold compose、dirty tile、metadata 変更後の再合成。cache-hit pan/zoom にこの計算を帰属させない。

**必要／不要の疑い:** pixel 変換・layer order/opacity は必要。同一合成中に不変の parameter 検証・curve vector clone は重複の疑いがある。**Release optimizer がどこまで除去・hoist するか、実 allocation 数は未確認。**

**テストと不足:** [adjustment order と非破壊性](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/tests/contracts/effects.rs:196)、同 341 行以降の no-op/invalid を照合。dense 文書＋複数 curve の allocation/validation 回数、point 数依存は未測定。

**最小測定:** 同じ source に adjustment なし、BrightnessContrast、3 point/最大許容 point の ToneCurve を付け、cold と 1 tile edit、warm hit を分ける。Release の allocation/CPU sampling で画素×layer に比例するか確認し、checksum/tile 数を併記。

**改善リスク:** display の 8bit 変換、16→8 rounding、alpha、curve interpolation を同値に保つ。display LUT を native16bit/export へ流用しない。metadata invalidation は維持。要件: `ADJUST-001`、`RENDER-001`、`PERF-001`。

### A11 — Light Table の同じ回転定数を合成 pixel ごとに算出する

**経路・静的事実:** Light Table property/edit → [snapshot の LT 合成](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/snapshot.rs:1528) → [LightTableState::composite](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/animation/light_table.rs:620) → [sample_item_source](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/animation/light_table.rs:732) → [rotate_q16](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-image/src/canonical.rs:416)。rotate は毎回 sin/cos を呼び、非直角は [CORDIC 反復](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-image/src/canonical.rs:375)。effective opacity/tint 等も item 内で不変。

**発生条件:** 非直角回転 reference を表示した cold/parameter 更新後の再合成。0/90/180/270°は即返す。cache hit の非透明 tile では再合成しない。透明結果の再合成は既存契約である。

**必要／不要の疑い:** 各 pixel の source 座標・sampling・合成は必要。同じ angle→turns、sin/cos、opacity/tint の都度計算は重複の可能性。実 cost と compiler hoisting は未確認。

**テストと不足:** [reference alignment](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/tests/contracts/animation.rs:595)、同 980/1009 行の opacity/RGBA16、canonical rotation test はある。[既存 LT benchmark](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/benches/core_workflows.rs:681)は rotation が既定 0 で、非直角 CORDIC を評価しない。

**最小測定:** source/opacity/translation 固定、0°/90°/13.7°、1/3 reference の cold snapshot を分離。sin_cos の CPU sample 比率を採り、warm hit を対照とする。既存 benchmark は変えない。

**改善リスク:** fixed-point、ties-to-even、checked overflow、frame、変換順を維持。浮動小数 affine への置換は同値ではない。parameter/source 更新後の古い定数保持を防ぎ、scalar cache 式・budget を守る。要件: `LT-001/002`、`PERF-001`。

### A12 — Save capture が Core 全体を複製してから表示データを捨てる

**経路・静的事実:** [QueueNativeSave](C:/Users/shuichi/GitHub/inkpod/apps/windows/ui/main_window_runtime.cpp:11944) → [FileIoController::Queue](C:/Users/shuichi/GitHub/inkpod/apps/windows/app/file_io_controller.cpp:465) → CoreHost FileIoWork → [FFI submit](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-ffi/src/file_io/exports.rs:95) → [FileIoJob::start](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/file_io/job.rs:185)。worker submit の前、Core owner 上で [capture_file_snapshot](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/persistence_task.rs:381)が `clone_for_staging` し、render_cache、secondary_views、通常は sequence/source cache、motion 等を clear/drop する。[clone 本体](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/core.rs:418)はこれらを複製している。

**発生条件:** Save/Save As、autosave、raster export 等の capture。大きな history、tile metadata、sequence catalog、複数 view があるほど対象が増える。UI queue 投入は非同期でも、その capture 中は同じ Core owner lane の後続 input/edit を処理できない。

**必要／不要の疑い:** immutable な保存入力、exact document/editor/history/authority の固定は必要。直後に捨てる cache map・view・sequence metadata の clone は不要の可能性。さらに [HistoryChange::Pixels](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/history.rs:321)は Vec、history は Vec<HistoryEntry> なので、その inverse payload の copy も発生し得る。**TileRaster/asset の clone 自体は COW/Arc 共有であり、すべての画像 bytes がコピーされるとは言わない。** snapshot での不要複製と、live cache の破棄を混同しない。clear されるのは frozen 側である。

**テストと不足:** 今回通過した [file_io tests](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/tests/contracts/file_io.rs:1641)は fence、save authority、failure、lease を守る。[native save tests](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/tests/contracts/native_v28.rs:146)も exact snapshot を確認する。capture の時間・allocation と history/catalog/cache サイズの関係、capture 中の入力待ちは測らない。

**最小測定:** 同じ document pixels を固定して history 件数、sequence entry 数、warm render cache 量だけ変える。公開 capture_document_save の前後を別診断で測り、drop 時間を除いた capture と worker prepare を分離。allocation stack と Core queue delay を確認する。既存 binary の Save 操作の ETW stack からも clone/clear の占有を確認できる。

**改善リスク:** 必要 field の省略による hidden branch、opaque metadata、ID high-watermark、savepoint、editor/owner token、検証入力の欠落。最小 capture DTO 化は保存契約の精査を要する。history を削除・compaction する案ではない。要件: `IO-001/003`、`SESSION-001`、`PERF-001`。

### A13 — native 保存の全 journal 再検証・独立 asset copy が毎回走る

**経路・静的事実:** A12 の detached snapshot → I/O worker の [prepare::save](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/file_io/prepare.rs:330) → [prepare_normal_save](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/persistence_task.rs:212)で companion raster を合成/encode → [prepare_native_save](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/persistence_task.rs:239) → [build_procedure_file](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/persistence.rs:317)。冒頭に [verify_journal_replay](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/journal.rs:972)を実行し、全 journal graph を再構築する。[detached_archive_round_trip](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/asset.rs:490)は保持 asset payload を独立 copy・再 ingest する。続いて journal/asset/Genesis 等を serialize する。save wrapper の cancellation は大きな build の前後で確認し、rebuild_runtime_from_journal の continue callback は常に true（journal.rs:1155）。

**発生条件:** 長い journal/大きい asset を持つ通常保存と autosave/recovery。直前と同じ immutable 入力部分でも同じ検証を通る。UI thread の encode ではなく worker の占有・保存待ち・取消応答の問題候補である。

**必要／不要の疑い:** journal が正本として再構築できることの検証、完全な current-version 出力、通常 pair 保存の両出力、durable publication は必要。繰り返し保存の不変部分を毎回同じ規模で再実行する cost を測る余地がある。ただし **独立 asset copy は明示された replay 検証境界であり、それ自体を不要と断定しない**。同等の検証独立性を保てない再利用案は採用候補から外す。Cancel 後も残る計算時間は別に測る。

**テストと不足:** [独立 copy test](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/asset.rs:1260)、[checkpoint/full replay 同値](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/tests/contracts/native_v28.rs:534)、save/reopen/failure のテストはある。既存 `checkpoint_open` の open 時間は save の capture/flatten/replay/serialization を分離しない。同入力の連続保存、history 長別の allocation、途中 cancel 応答の時間が不足する。

**最小測定:** 同じ captured state を用いた二回の prepare と、小 edit 後の prepare を別診断で比較。CPU/allocation stack で companion flatten、detached assets、journal replay、serialize を分離し、disk flush/install は別区間にする。100/1,000 commit 程度の bounded fixture と cancel 要求から worker 解放までを測る。両 output/再open/Undo/Redo の同値と非成功時の非 publication を併記。

**改善リスク:** prefix/検証結果の誤再利用で branch、asset、epoch、opaque section、savepoint の破損を見逃す危険。checkpoint を journal の代替にしない。検証削除、履歴削除、quality/depth 低下、保存の flush/replace 省略は提案しない。要件: `IO-001/003`、`PERF-001`。本候補は cost 調査優先で、最適化の妥当性は条件付き。

### A14 — shared I/O の予算予約が mutex 内で LRU 全走査を反復する

**経路・静的事実:** Open/sequence/Reference/Batch の path-only job → [IoManager::read_image_with_reload](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-io/src/manager.rs:275) → encoded/image/decoded 予約 → [ImageCache::reserve](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-io/src/cache.rs:170)。global cache mutex を保持し、予算不足の間、全 entries から unpinned LRU を min_by_key で探して一件ずつ remove する。N entry、E eviction なら概ね O(N×E) の metadata 探索となる。最後に pinned 分しか残らず admission 不能なら、それまでの cache eviction は戻さず ResourceBusy を返す。

**発生条件:** budget 近傍で複数 worker が読み込み、大きい一件のために多数の小 entry を追い出す場合。通常 hit/予算に余裕がある場合はこの反復をしない。これは一般 reserve の話で、[sequence display reserve](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-io/src/cache.rs:239)には回収可能量を事前確認する別の保護がある。

**必要／不要の疑い:** allocation 前予約、pin/lease 計上、LRU 回収、atomic な counter 更新は必要。毎 victim の全走査、mutex 保持中の payload 解放、結局失敗する request による再利用可能 cache の喪失は不要 work/競合の疑い。live owner の画像が消えるという主張ではない。

**テストと不足:** 今回 16 件通過した [manager tests](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-io/tests/manager.rs:405)は小 budget の LRU/pinned rejection を検査。[sequence reserve test](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-io/tests/manager.rs:307)は失敗時 non-eviction も検証。一般 reserve の「一部回収できるが不足」の cache 変化、多数 eviction の scan 数、mutex 待ちは未検証。

**最小測定:** まず既存の小 budget 注入方式で 3〜数件の生成画像を使い、一部を pin した不足 request 前後の cache_stats を比較。次に bounded な 100/1,000 小画像で、1 件の大 request の eviction 数、wait stack、read/decode 再発を測る。製品の上限を変えず、fixture 側の注入値として扱う。

**改善リスク:** LRU 順序、reservation の排他性、lease が外れた時点、旧/新同時生存、失敗時表示保持を崩さない。追加 index 自体のメモリ、last-lease の実解放、lock 順序を考慮する。cache 上限を上げて隠さない。要件: `IO-003`、`PERF-001`。

## 6. 不要処理と認定しなかったもの

- **通常 pan/zoom の tile 再利用はある。** Core は scalar revision で hit を判断し、Renderer も ID/revision/dimensions 一致を upload しない。A01/A02 はこの正常な GPU 再利用と併存する UI 作業である。
- **未編集連番再訪の最近の修正は存在する。** checksum memoization、immutable thumbnail、CPU/GPU pristine source bank、first edit への bank 移譲、final transform 一回の publication、bounded neighbor preparation を確認した。「連番では毎回全面 checksum/再合成/再 upload」という過去の問題を再報告していない。
- **clone は全画像 copy と同義でない。** [TileRaster](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-image/src/raster.rs:108)は tile payload と checksum の COW/Arc を使う。metadata/tree clone、inverse Vec copy、変更 tile の COW、独立 replay copy は別々に評価した。
- **選択変更の全無効化には既存 test 契約がある。** [snapshot test](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/snapshot.rs:1906)は選択外 tile の revision 変化も明示的に期待する。これを勝手に局所化する前提にしない。
- **現行通常 snapshot は raster pass。** [snapshot.rs](C:/Users/shuichi/GitHub/inkpod/rust/inkpod-core/src/snapshot.rs:656)は adjustment LUT を出さない。`build_ordered_content` は retained validation helper で実呼出しがなく、Renderer の D2D adjustment graph 再構築コードだけを見て通常操作の負荷とは認定しなかった。A10 の CPU compose が現経路である。
- **Locator、progress、filter task に非同期保護がある。** Locator は一件実行＋最新一件、filter は debounce/cooperative cancel、file progress は cached state を使う。通常 stroke の sample/begin/end/cancel を捨てる最適化は考えない。
- **不可視/occluded、resize/device loss の扱いを区別した。** 不可視 sink の不要 publication/Present は抑制される。通常 resize は target resource の変更で、tile cache 全破棄ではない。device loss に伴う GPU 破棄・再構築は必要。
- **全面 Draw/Present だけで不要とは言わない。** swap-chain の契約、最終画面、accepted render credit、VSync を保つ必要がある。empty/同一 geometry preview の no-op で余分な Present を避ける処理もある。

## 7. 次の最小確認の進め方

1. **追加コードなしの trace から始める。** A01/A02/A03/A05/A06 を同じ通常 message loop で観測し、UI wait、Core、pane/GDI、Renderer queue、D2D/Present を別区間にする。
2. **最少 fixture で意味上の再利用を確認する。** A08 の 2〜4 tile、A06 の 2 Canvas、A04 の warm Cut、A14 の小 budget を先に使う。pixel、revision、owner、end/cancel、cache bytes を同時記録する。
3. **疑う計算だけを補助診断で分離する。** A09/A10/A11/A12/A13 は文書全体の経過時間だけで判断せず、soft 出力、curve allocation、CORDIC、capture、独立 replay を分ける。
4. **既存 workload と承認済み envelope はそのまま使う。** 新 counter/private harness が必要な場合は後続の実装・計測作業として扱い、この監査では追加していない。時間測定は profiler/tracepoint の影響を分け、同環境・同入力の warm-up＋5 回以上と意味 gate を維持する。

今すぐ変更する案の承認を求める報告ではない。とくに A06/A08/A13 は、不要に見える clear/検証を除去すると正しさを壊し得るため、先に測定と依存関係の確認が必要である。

## 8. 未調査・未検証の範囲

- 実ユーザー画像、最大 layer/catalog/history、8 GiB/512 MiB 近傍の実資源圧迫、巨大 Cut、極端な sparse/dense 比での定量測定。
- fill の gap close/closed region/overflow abort/fill-protection の全組合せ、selection morphology、広域 geometry、color replace/chart extraction の algorithm profile。
- Batch/InkScript の全 primitive/owner、Motion Check の継続 playback、巨大 history visualization、clipboard、Cut Cell の cold validation の全 call graph。
- native16bit companion/export の全 codec・全合成条件、disk/ネットワーク share、anti-virus、flush/replace/recovery の I/O latency、worker 公平性と shutdown の長時間 soak。
- 全 window/tab/view mode/cache/pin の組合せ、DPI/theme/high contrast/IME/UIA/screen reader、physical pen cadence、hardware keyboard、remote display。
- driver 内部 allocation、GPU 実行時間、physical scanout、frame-readiness の background 遅延原因、thermal/物理ホスト条件の統制。
- A01〜A14 の発火数、allocation/GPU transfer 総量、ユーザー応答への寄与率、改善後の before/after。今回の benchmark 上限超過と各候補の因果関係。
- full Windows configure/build/runtime CTest と全 Rust 品質ゲートの再実行。既存文書の Verified 状態を、今回の限定的監査だけで更新・格上げしていない。

この報告は、全アプリケーションの性能保証や網羅的な欠陥一覧ではない。静的に確認した処理、既存の保護、今回の限定実測を分け、次の測定で確かめられる形にした監査結果である。
