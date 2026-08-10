# Legacy design and verification record

この文書は、現在の実装を理解する際に有用な歴史的背景を一か所へ集約したものである。
現行の仕様、設計、互換状態、形式、性能判定には使用しない。正本の一覧は
[`README.md`](README.md) を参照する。ここで要約した旧ベースラインの完全な内容は Git 履歴に残る。

## Core リファクタリング（M0–M9）

2026-07-30 の M0 ベースラインでは、Core の公開 mutation を文書置換、即時編集、履歴、
view-only、preview/session、長時間処理、永続化、sequence/transient workflow に分類した。
当時は Core test 83 件、C ABI v2、native `.inkpod` v2 が基準で、固定 seed の
state-machine test、共通の公開 `CoreObservation`、全 mutation に共通する transaction primitive は
まだ揃っていなかった。

後続作業では、公開 API だけから document/view revision、history、dirty/savepoint、stable ID、
snapshot、機能別状態を観測する test model を導入した。通常編集を stale/overflow 検査付きの共通
transaction に移し、成功時だけ文書、`StateId`、revision、history、journal、dirty、ID authority、
cache invalidation を同時公開する形へ統一した。preview、stroke、floating selection、cancellable work、
文書全体の置換は、それぞれ固有の長寿命 staging/lifecycle 境界を維持した。

その後、別個の Cell ID、不変 Genesis、内容アドレス方式 asset、型付き canonical procedure、
append-only journal、cache-free replay、cross-architecture digest、当時の v11 container と任意 CKPT が
順に接続された。M0 の関数一覧、移行 wave、当時の例外一覧、test 件数は完了時点で陳腐化したため、
現行 route は [`primitive-route-inventory.md`](primitive-route-inventory.md)、現行 transaction と
journal は [`architecture.md`](architecture.md) を正本とする。

## GUI モダナイゼーション（G0–G13）

2026-07-31 の G0 は、一つの `AppContext`、main window、document binding、Canvas/render thread と、
固定配置の主要 pane を持つ構成だった。基準値は `IDM_*` 282 個、production command 281 個、
C ABI v2 の 161 exports、Rust 177 tests + 1 doctest、Windows x64 Debug CTest 11/11 だった。
strong frontend ID、immutable `CommandContext`、複数 session/window、split editor group、汎用 docking、
single-instance activation は未実装だった。

G1–G13 で `ApplicationHost`、`WorkspaceWindow`、`DocumentSession`、`DocumentView`、`EditorGroup`、
session-keyed `CoreHost`、共有 `RendererHost`、bounded queue、target-aware pane、workspace persistence、
複数 top-level window、single-instance activation、resource/accessibility instrumentation を導入した。
旧 G0 の所有権表、command/ABI 件数、完了 gate、既知差分は比較資料であり、現在値は
[`architecture.md`](architecture.md)、[`windows-command-inventory.md`](windows-command-inventory.md)、
[`ffi.md`](ffi.md)、[`compatibility.md`](compatibility.md) に置く。

## 廃止された `.inkpod` v2

pre-M8 の v2 container は、32-byte header、binary manifest、checksummed tile blob area を分離し、
後から `DOCM`、`LTBL`、`VECT`、`ADJT` の optional section を加える構成だった。文書は main-line/color
plane を中心とする DTO で、active IDs、selection plane、guide/grid、Light Table raster、vector geometry、
adjustment metadata を保持した。一方で完全な canonical `EditorState`、不変 Genesis、asset registry、
procedure/control-event journal を表現できなかった。

この形式は互換 reader を持たず廃止された。現行 reader は v2 を payload decode 前に拒否し、v2 の
header flag、section、DTO、checksum は現行 bytes の一部ではない。現行の唯一の形式契約は
[`file-format.md`](file-format.md) の v15 である。

## `revision-max` 採用と性能校正

procedure-history 作業の初期段階で render-cache 検証へ semantic digest と tile 処理を持ち込んだ結果、
zoom/pan と dirty-tile workload が pre-M1 production より遅くなった。最終設計は document commitment と
render-cache identity を分離し、cache validation を source revision scalar の最大値へ戻し、変更 tile の
借用と一回の composition preparation を採用した。

2026-08-05 の同一 ARM64 Windows/Parallels host における旧 production と候補の交互 A/B では、
Core の quick/full `pan_zoom_snapshot` と `dirty_tile_rebuild` の候補 median が旧版より約 12.7–27.3%
低かった。native `drawing` は約 33.2% 低く、display-paced `wheel_zoom` は実質同等だった。
wheel の約 +0.0015% paired-ratio 差は CPU 回帰ではなく表示同期ノイズとして明示承認された。

M2、旧 G13、M4–M9 の各受入では checksum、revision、history、tile reuse/rebuild、payload access、
sample/Present、queue/resource counter を維持した。個別の dated sample 列は現行の判定材料ではないため
削除した。現行 workload、承認済み environment envelope、二段階 median 判定、例外的な旧版再構築手順は
[`core-benchmark-baseline.md`](core-benchmark-baseline.md) に残す。

## 2026-08-03 G13 native x64 観測

G13 の手動観測では、keyboard route の大半、UIA/MSAA 名、通常 DPI、device-loss/shutdown の自動経路を
確認した。日本語 resource と About の英語表記は当時の期待どおりだった。一方、Reference Check の
AutoHide edge button は keyboard focus 不能で Fail、高 contrast、200% display、実 screen reader の
全項目、日本語 IME composition、fault 後 shutdown の手動行は環境制約または未実施により Blocked だった。
これらを Pass に読み替えず、G13 完了後も該当 requirement の残作業として追跡した。

当時の native ARM64 interaction waiver、未完了行、作業 tree を使った observation record は過去の
リリース判断であり、現行候補には適用しない。現在の未完了項目は
[`compatibility.md`](compatibility.md)、再現可能な検証手順は
[`windows-release-checklist.md`](windows-release-checklist.md) を参照する。
