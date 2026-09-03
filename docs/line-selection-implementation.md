# 線補正・選択支援の実装結果（2026-09-03）

監査で不足していた線補正とワンドの漏れ防止を実装した。利用者承認済みの5つの判断を
SPECへ反映し、以前の監査で失敗した画素assertionを維持したまま修正した。
監査当時の「未実装」「未達」は[当時の監査](line-selection-audit.md)に記録を残す。

対象8能力の実装と公開API・Windows実操作の検証は完了した。英語／日本語のproduct smokeは
それぞれ346.91秒／351.02秒で全シナリオが成功した。benchmarkも9シナリオすべて成功した。
全CTest再実行は48/49成功で、RendererHost読出し失敗1件を下記で別に扱う。

## 変更の前提

- branch `main`、開始commit `07c8dab252ec38218e3d09b7dcc272286c2ef17c`。commit/push/PRは実施しない。
- 監査の開始時はclean。その後に追加した監査・テスト・状態訂正と、承認済み実装案を保持して実装した。
- 判定の正本は[SPEC](../SPEC.md)。承認内容は[実装案](line-selection-implementation-plan.md)。
- 主対象PAINT-003、SEL-001、SEL-004に加え、SEL-002（選択演算）、HIST-001（transaction・Undo/Redo・preview Cancel）、IO-001（保存・再読込）、ABI-001/002（公開境界・所有権）を追跡した。通常彩色のPAINT-001の主線保護を、明示的な線編集の禁止へ読み替えていない。
- 第三者画像、追加の利用者画像、外部マニュアル、架空API、常時成功stubは使っていない。
- current-only規則に従いnative v34、replay epoch 29、C ABI 34、InkScript catalog/owner v7へ更新した。
  旧versionの互換readerは追加していない。新しいprimitive、背景設定、独立した線幅modeを保存・replayする。

## 比較と到達経路

以下の判定は承認後の現行SPECに対するもの。元のSPECから変更した点は次節で区別する。
CoreとWindowsを別々に実行したうえで判定する。物理入力・構成の残りは末節へ明記する。
WindowsではRGBAの線修正7modeを矩形gesture、Cancel、確定、Undo/Redoで実測し、
線つなぎの一括Apply、ワンドの閉／開／仮想閉の全mask、なぞりの4つのview条件も検証した。
Binary/Grayscale/RGBA8/16、全範囲形状・筆圧・閾値の詳しい境界値はCore/Image公開APIで検証する。
全mode×全形式×全範囲形状の組合せをWindows操作でも網羅する検証は今回未実行。

| 機能 | 利用者期待 | 現行仕様・要件 | UI操作 | 観測 | 仕様準拠判定 | 利用者期待に対する判定 | 根拠・検証 | 残る確認 |
|---|---|---|---|---|---|---|---|---|
| ゴミ取り | 帯内の小成分だけ除去、大線・帯外を保護。穴埋め・異色置換も別に成立 | PAINT-003。1〜65536画素、上限を含む。前景/異色8近傍、穴4近傍。元plane全体で測定 | ゴミ取りtoolと編集/Filterの一括適用。3mode、最大画素、4形状、ブラシ・背景・preview | 1/3画素を除去、4画素を保持。帯を横切る大線を切らず、画像端へ開く背景を埋めない。白/透明/指定色を区別。CoreとWindows実操作で一致 | 達成（承認後SPEC） | 達成（記載の制限内） | [閾値・帯・Undo](../rust/inkpod-core/tests/line_selection_audit.rs#L159)、[全成分・白背景・開背景](../rust/inkpod-core/tests/line_selection_audit.rs#L400)、[explicit_native_background_dust_holes_outliers_and_invalid_eight_bit_color](../rust/inkpod-image/tests/line_correction.rs#L338) | 実ペン／全DPI・別構成は末節参照 |
| 線つなぎ | 条件を満たす線端間へ恒久的な接続線を追加 | PAINT-003。空白格子ステップ数≤gap、両端方向差45度以内、相互に一意の最良候補 | 線つなぎtool／編集→線修正→線つなぎ。gap、接続全幅、範囲、背景、preview | 水平/垂直/斜め、閾値直前/等値/直後を検証。帯外・競合・平行・交差を保護。CoreとWindows実操作で一致 | 達成（承認後SPEC） | 達成（記載の制限内） | [connection_inclusive_gap_in_all_directions_and_native_formats](../rust/inkpod-image/tests/line_correction.rs#L32)、[connection_footprint_width_parallel_crossing_and_band_exclusion](../rust/inkpod-image/tests/line_correction.rs#L284)、[contract_line_connection_region_preview_native_depth_history_and_replay](../rust/inkpod-core/tests/line_selection_audit.rs#L1033) | 実ペン／全DPI・別構成は末節参照 |
| 線幅を太くする | 描画済みの線の局所幅を増やす | PAINT-003。円形近傍、片側半径1〜256 document px | 線幅修正→太くする | 半径1で断面1/3/7→3/5/9。帯外・別plane不変。CoreとWindows実操作で一致 | 達成（承認後SPEC） | 達成（記載の制限内） | [thicken_thin_and_uniform_are_independent_native_depth_operations](../rust/inkpod-image/tests/line_correction.rs#L90)、[contract_line_width_local_cross_sections_native_depth_and_one_undo](../rust/inkpod-core/tests/line_selection_audit.rs#L1117) | 実ペン／全DPI・別構成は末節参照 |
| 線幅を細くする | 描画済みの線の局所幅を減らす | PAINT-003。同じ半径単位の収縮 | 線幅修正→細くする | 半径1で1/3/7→0/1/5。細線消失もpreview/Undoで復元。CoreとWindows実操作で一致 | 達成（承認後SPEC） | 達成（記載の制限内） | 上記断面・履歴テスト | 実ペン／全DPI・別構成は末節参照 |
| 線幅を均一にする | 連続した1/3/7幅の区間を指定全幅へ揃える | PAINT-003へ承認済みの独立modeを追加。中心線から全幅1〜256で再構成 | 線幅修正→均一幅にする | 十分長い異幅区間が幅5へ一致。分岐・閉路・native色/深度を保持。CoreとWindows実操作で一致 | 達成（承認後SPEC） | 達成（記載の制限内） | [uniform_reconstructs_one_continuous_line_with_three_source_widths](../rust/inkpod-image/tests/line_correction.rs#L134)、[uniform_keeps_branch_endpoints_connected_and_morphology_has_circular_corners](../rust/inkpod-image/tests/line_correction.rs#L406)、閉路/2×2消失防止 | 交点の重なりは指定幅より太くなる。輪郭/AAは変わり得る |
| マジックワンドの連結同色選択 | seedから連続する同色だけを選ぶ | SEL-001/002。4近傍、native-depthのinclusive許容差 | 選択→ワンド、許容差、選択4演算 | 完全囲みの内部196画素、対角のみの同色を除外。色/alpha/16bit下位差の境界、4演算を確認。CoreとWindows実操作で一致 | 達成（承認後SPEC） | 達成（記載の制限内） | [contract_wand_closed_connected_set_source_undo_replay_save](../rust/inkpod-core/tests/line_selection_audit.rs#L509)、[contract_wand_tolerance_native_depth_alpha_and_four_connectivity](../rust/inkpod-core/tests/line_selection_audit.rs#L592)等 | 実ペン／全DPI・別構成は末節参照 |
| マジックワンドの隙間漏れ防止 | 囲み線の短い切れ目を仮想的に閉じて外へ漏らさない | SEL-001。探索前の仮想境界、gapは線つなぎと同じinclusive単位 | ワンドのgap設定 | 指定32×32 fixtureでgap無効は内外へ到達、有効は内部196画素のみ。元画素完全不変。tile境界/画像端、複数gap、scalar/8/16bitでも確認。CoreとWindows実操作で一致 | 達成（承認後SPEC） | 達成（記載の制限内） | [expectation_wand_gap_close_prevents_escape_through_one_pixel_break](../rust/inkpod-core/tests/line_selection_audit.rs#L551)、[contract_wand_inclusive_gap_threshold_and_multiple_breaks](../rust/inkpod-core/tests/line_selection_audit.rs#L568)、[contract_wand_virtual_boundary_at_tile_edge_and_image_edge_native_planes](../rust/inkpod-core/tests/line_selection_audit.rs#L633) | 仮想境界も方向・競合条件を使う。あらゆる形の隙間を無条件に閉じる契約ではない |
| なぞり選択 | 開いたL字の帯だけを選択し、線も背景も含める | SEL-001/004。丸/角、document径/開始zoomによるscreen径、pressure、Normal等 | 選択→なぞり、構築設定、Canvas begin/append/end/cancel | 指定L字の全180画素が一致。(16,16)/(28,4)を除外。source不変。疎sample、クリック、反復、自己交差、はみ出し、4演算、Undo/replay/saveを確認 | 達成（承認後SPEC） | 達成（記載の制限内） | [L字・履歴](../rust/inkpod-core/tests/line_selection_audit.rs#L721)、[形状・筆圧・zoom](../rust/inkpod-core/tests/line_selection_audit.rs#L792)、[RunLineSelectionAuditSmoke](../apps/windows/app/app_smoke.cpp#L7753) | 帯ハイライトは最大128点に間引いた表示。確定maskは全sampleを使う。物理pen/touch・全DPI組合せは別記 |

## 承認後に固定した仕様

1. ゴミ取りのサイズと穴の外部接続は、操作帯で切り取る前のsource plane全体で測る。
   成分全体が操作maskと既存selectionの共通部分に入る場合だけ変更する。
2. scalarの背景はcoverage 0。RGBA MainLineは白＋透明、Color/Rasterは透明を初期値とする。
   指定色はnative-depthの完全一致。alpha 0の未使用RGBは区別しない。
   白/透明が混在する除去跡は勝手に選ばずno-op。8bitへ表現できない指定色は拒否する。
3. gapは `max(abs(dx),abs(dy))-1`。空白画素数に対応する格子ステップ数であり、
   端点中心のユークリッド距離ではない。「未満」から承認済みの「以下」へ改定した。
   同順位の競合、相互最良でない候補、別線や別の提案接続を横切る候補を採用しない。
4. 太く/細くの指定値は片側半径。均一化の指定値は全幅。偶数全幅は中心を左上へ0.5 px寄せる。
   新規ブラシ径、選択maskの拡縮、図形線幅を既存線の修正として数えていない。
5. 均一化は元の監査時SPECに含まれていなかった。利用者承認後に独立modeとして追加した。
   分岐/交点の重なりやAA変化を、直線部の幅一致と混同しない。

## 実装の責務と失敗時の契約

- `rust/inkpod-image/src/line_correction/`: 背景、bounded grid/中心線、端点接続、円形morphologyと均一化。
  `edit/dust.rs`は成分全体・穴の画像端接続を判定する。
- `rust/inkpod-core/src/effects/line_correction.rs`: 型付きrequest、範囲∩selection、preview、canonical transaction。
  `selection/wand.rs`は仮想境界の構築後に4近傍探索する。
- `rust/inkpod-ffi/src/effects/line_correction.rs`: record/enum/span、READY task、期待revision、出力の検証。
  Windowsから渡したpointerを保持しない。header/docs/ffiに所有権と単位を記載した。
- `apps/windows/ui/tools/line_correction_options.cpp`とToolOptionsPane: 日本語/英語の設定。
  `main_window_runtime.cpp`: command/対象/開始frameを捕捉し、新C ABIへ渡す薄いadapter。
  画像処理やmask計算はC++へ複製していない。
- 成功だけが一transaction、一Undo単位になる。no-op/invalid/cancel/stale/処理上限の失敗では
  部分画素・余分なhistoryを公開しない。Undoで画素/maskを戻し、単調revisionの巻戻しは要求しない。
- native save/reopen、journal replay、InkScript export→compile→executeでも同じ結果を検証する。
  `apply_line_correction`はproduction catalogの74番目のcommandとして登録し、owner対応とfingerprintを更新した。

## 実操作で追加発見した境界不具合

device x=769.349976、pan x=179.4、zoom=49.1625の入力は、document x=11.999999になる。
旧effect用のrectangle作成はこれを先にfloorし、線幅修正で帯外の(11,23)/(11,25)を変更した。
[contract_line_rectangle_uses_canonical_selection_geometry_at_float_boundary](../rust/inkpod-core/tests/line_selection_audit.rs#L1303)は修正前に実際に失敗した。
新しい線補正rectangleを既存のQ16選択geometry・画素中心判定へ接続し、同じ期待集合で合格した。
Windows smokeの期待画素も変更していない。

全CTestと並行した最終のnative-depthレビューで、指定背景`RGBA16(1000,2000,3000,1)`と、
1階調だけ異なる線`RGBA16(1001,2000,3000,1)`の膨張がno-opになることも再現した。
比較用contrast×alphaが0へ丸められていたため、背景以外の比較値を最低1とした。
実際のRGBA値は変換しない。9×9 fixtureの全81画素を比較する
[native_low_alpha_contrast_against_custom_background_stays_foreground](../rust/inkpod-image/tests/line_correction.rs#L474)は修正前exit 101、
同じ期待値のまま修正後exit 0。細くする操作とsource不変も同時に検証した。
この変更は指定背景を使う微小contrastの形態演算だけに作用し、標準背景のUI fixtureの結果を変えない。

検証fixture側では二点を訂正した。RGBA8 eyedropper値を16bitとして割っていた読出しをdepthに従って
修正した。scalarへ変換するワンドfixtureは、alphaが同じ白/黒では両方とも同coverageになるため、
透明背景と不透明線へ変更し、変換後にも背景/線が異なることをassertした。期待する内部集合は不変。

## 実行記録

全コマンドはPowerShell profileに依存せず実行した。MSVCは`vswhere`で取得した
`VsDevCmd.bat -no_logo -arch=x64 -host_arch=x64`を使用した。実在presetは`windows-x64-debug`。
各ログは作業tree内の`build/line-selection-audit/implementation-*.log`に保存している。
全結果を確定した。全体CTestの失敗と、その後の単独成功は別々の実行として記録する。

| コマンド／検証対象 | exit code・結果 |
|---|---|---|---|---|---|---|---|---|
| `cargo fmt --check` | 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 |
| `cargo test --workspace --all-features` | 0、834件成功。既存Release専用テスト1件がignore、今回追加したignoreは0件 |
| `cargo bench --package inkpod-core --bench core_workflows -- --quick` | 0、9シナリオ成功。workload/counter/envelope変更なし |
| `RUSTDOCFLAGS=-D warnings; cargo doc --package inkpod-core --all-features --no-deps` | 0 |
| `cmake --preset windows-x64-debug` | 0 |
| `cmake --build --preset windows-x64-debug` | 0 |
| `ctest --preset windows-x64-debug --output-on-failure` | 8、48/49成功（873.79秒）。英346.34秒／日357.44秒は成功。RendererHost読出し失敗1件 |
| `ctest --preset windows-x64-debug -R '^inkpod_windows_smoke(_japanese)?$' --output-on-failure --timeout 360` | 0、英346.91秒／日351.02秒、2/2成功 |
| `ctest --preset windows-x64-debug -R '^(inkpod_windows_renderer_host|inkpod_abi_smoke)$' --output-on-failure --timeout 180` | 0、2/2成功。RendererHost 67.39秒、ABI 45.27秒。期待値・待ち条件の変更なし |
| Core `line_selection_audit` target（全workspace実行にも含む） | 0、29/29成功 |
| Image `line_correction` target（同上） | 0、9/9成功 |
| FFI `line_ffi_exact_pixels_preview_stale_invalid_cancel_and_ownership`（同上） | 0、1/1成功 |
| Coreのline-correction canonical codec／InkScript export（同上） | 0、2/2成功 |
| bundled Python `scripts/generate_windows_localization.py --check` | 0、生成resourceと一致 |
| bundled Python `scripts/generate_inkscript_reference.py --check` | 0、catalog v7と生成referenceが一致 |
| embedded format HTMLとCore catalogの照合 | 0、76 entryのname/schema/semanticsが全件一致 |
| `git diff --check` | 0。既存Git設定によるLF→CRLF通知あり |

最終ログは[workspace](../build/line-selection-audit/implementation-workspace-low-alpha-final.log)、
[Clippy](../build/line-selection-audit/implementation-clippy-low-alpha-final.log)、
[benchmark](../build/line-selection-audit/implementation-bench-low-alpha-final.log)、
[rustdoc](../build/line-selection-audit/implementation-rustdoc-low-alpha-final.log)、
[MSVC build](../build/line-selection-audit/implementation-windows-build-low-alpha-final.log)、
[英日product](../build/line-selection-audit/implementation-product-after-rectangle-fix.log)、
[全CTest再実行](../build/line-selection-audit/implementation-ctest-final.log)に残した。
`build/`はgit対象外の実行証跡で、結果はこの報告にも保持する。
[最終Rustのコマンド別exit記録](../build/line-selection-audit/implementation-rust-revalidation.log)と
[RendererHost／ABI単独再実行](../build/line-selection-audit/implementation-renderer-abi-recheck.log)も保存した。

全CTestと並行して見つけた最終の微小16bit contrast修正後には、fmt／Clippy／全workspace／
benchmark／rustdocをすべて再実行し、MSVC再buildとRendererHost／ABIも成功した。
その最終差分は指定背景付き16bit形態演算の比較値だけで、RGBA8のUI fixtureとUI adapterは不変。
英日product smokeの記録はその直前の実行であり、微小16bitケースの追加後にもう一巡した結果ではない。

834件に含まれる既存のignoreは`script::tests::approved_quick_performance_contract`
（Release専用InkScript性能gate）であり、今回のCore workflow benchmarkとは別対象。
新規機能の失敗をignoreへ移したものではない。Windows Release/ARM64と非Windowsは、
今回の代表構成をWindows x64 Debugとしたため未実行であり、成功とは記録しない。

途中の失敗は隠していない。最初のworkspace runは旧version/count/route inventoryの固定値で失敗し、
仕様変更との対応を確認して更新した。旧catalog/editor-default digestも、追加したtool/primitiveと
同じversion更新で改定し、画像goldenやreplay結果のassertionを緩和していない。
初期実装ではcallbackのgeneric再帰、FFI fixtureのmove、primitive schema登録漏れで
compile／replayテストがexit 101となり、それぞれ内部executorの分離、値tupleの保持、catalogとの整合を修正した。
`--all-features`を欠いた単独contracts実行は既存feature-gated API参照でcompile errorとなった。
初期CMake buildではC11 ABI versionとcommand件数assertが失敗し、現行契約へ更新した。
最初のCTestはexit 8（42/49）、次はexit 8（47/49）。後者の2失敗は上記rectangle境界を英日で再現したもの。
元の監査baselineには47/49とSave/Open・editor-splitの後段失敗があり、今回の境界再現と区別する。
最終全CTestではRendererHostの既存Sequence first-edit読出しが13.50秒で失敗した。
`S_FALSE`、`latency_timeouts=21`、期待RGB=(32,64,96)に対してreadback未取得だった。
当該期待値・待ち条件は変更せず、最終buildを他の重い処理なしで再実行し、
RendererHost 67.39秒／ABI 45.27秒で両方成功した。間欠的な読出し失敗の原因は未特定であり、
この再実行を原因修正の証明とは扱わない。全体48/49の記録を49/49へ書き換えていない。
環境の`could not canonicalize path C:\Users\shuichi`通知は複数のCargo実行で出るが、
上記成功コマンドでは実際のexit code 0を取得した。warningの無視やlint緩和はしていない。

## 未検証・制限

- 物理pen/touch、実モニターを切り替えた全DPI組合せ、ARM64/Releaseの今回変更後の実行は未確認。
  client device-pixelイベント、view flip、途中zoom/pan、開始screen径は英日Windows product smokeで検証済み。
- 処理資源には固定上限がある。大きな領域×大きな半径などでwork budgetを超える場合はatomicに失敗する。
  上限を性能測定に合わせて緩和したり、benchmark workload/envelopeを変更したりしていない。
- 選択のdirty/revision/historyはdocument-ownedの既存契約に従う。source画素が不変でも
  selection操作自身を無条件no-opとは扱わない。
- 状態文書の以前のVerifiedは、白背景・全成分測定・恒久線接続・既存線幅修正・探索前gap境界を
  裏付けていなかった。今回の証拠だけを根拠に、対象要件の状態を更新する。

## 実装・変更箇所の索引

- 仕様・履歴・形式: [承認後の線修正仕様](../SPEC.md#L352)、[ワンドの仮想境界](../SPEC.md#L483)、[primitive schema/semantics](../rust/inkpod-core/src/primitive/catalog.rs#L127)、[canonical引数と実行](../rust/inkpod-core/src/primitive/invocation.rs#L178)、[native version](../rust/inkpod-format/src/procedure.rs#L23)、[InkScript command](../schemas/inkscript/catalog-v7.json#L9201)、[ABI所有権・task契約](../docs/ffi.md#L12)。
- 画像・選択処理: [成分・穴・異色置換](../rust/inkpod-image/src/edit/dust.rs#L8)、[native背景定義](../rust/inkpod-image/src/line_correction/background.rs#L5)、[恒久線つなぎ](../rust/inkpod-image/src/line_correction/connect.rs#L162)、[仮想境界](../rust/inkpod-image/src/line_correction/connect.rs#L300)、[太く・細く・均一化](../rust/inkpod-image/src/line_correction/width.rs#L13)、[探索前境界と領域探索](../rust/inkpod-core/src/selection/wand.rs#L3)、[transaction・preview](../rust/inkpod-core/src/effects/line_correction.rs#L51)、[正規範囲・Q16矩形](../rust/inkpod-core/src/effects/line_correction.rs#L130)。
- 公開境界・UI: [128-byte C ABI](../include/inkpod/core_ffi.h#L2878)、[FFI validationとtask](../rust/inkpod-ffi/src/effects/line_correction.rs#L15)、[ツール設定・値域](../apps/windows/ui/tools/line_correction_options.cpp#L9)、[設定ペイン](../apps/windows/ui/panes/tool_options_pane.cpp#L419)、[gesture開始frame](../apps/windows/ui/main_window_runtime.cpp#L5145)、[UIからC ABIへ](../apps/windows/ui/main_window_runtime.cpp#L6793)、[非破壊の帯preview](../apps/windows/ui/main_window_runtime.cpp#L8265)、[選択入力adapter](../apps/windows/ui/main_window_runtime.cpp#L14699)、[組み込み操作ヘルプ](../html/manual.html#L1096)、[組み込み形式解説](../html/file_format.html#L916)。
- 具体的fixtureとassertion: [Core 29 tests](../rust/inkpod-core/tests/line_selection_audit.rs#L159)、[Image 9 tests](../rust/inkpod-image/tests/line_correction.rs#L32)、[FFI failure・所有権](../rust/inkpod-ffi/tests/unit/line_correction.rs#L18)、[実CanvasのL字全180画素](../apps/windows/app/app_smoke.cpp#L7753)、[実menu/Canvasのwand・線修正](../apps/windows/app/line_correction_smoke.cpp#L22)。
- export/replayの一致: [InkScript export→execute→journal replay](../rust/inkpod-core/src/script/tests.rs#L218)。pixel期待値の検証は上記の独立したCore/Image fixtureで行い、同じexecutor同士の比較だけで画素の正しさを判定していない。

## 差分の分類

- **仕様との差:** 元SPECにあった小点・小穴・異色修正と未接続の線補正を実装した。矩形の範囲外変更と微小16bit前景の誤判定は追加修正した。対象8能力の画素・mask条件について、承認後SPECに対する自動検証で再現中の不一致はない。RendererHostの間欠失敗は上記の別記録とする。
- **利用者期待との差:** gapの「未満」を「以下」へ、未規定だった全成分測定・背景を明文化した。均一化は当初SPEC外だったため、承認後に独立modeを追加した。交差部やAAの制限は承認案どおり。
- **UI未接続:** 対象8能力について解消した。線つなぎ・線幅はツールメニューと編集の線修正メニューから到達する。ツールパレットの新規アイコンは要件に含めていない。
- **テスト不足:** broadなVerifiedや鉛筆テストでは欠落していた線補正・漏れ防止を、小さい合成fixtureと完全画素集合で補った。単なる成功status、面積、外接矩形のみで合格にしていない。
- **環境による未検証:** 物理pen/touch、全実モニターDPI、ARM64/Windows Release、非Windowsは今回未実行。現行Windows x64 Debugの自動UI経路と区別する。
