# 検証手順

変更対象の自動検証と受入条件を定める。現在の実装範囲・既知差分・代表結果は
[compatibility.md](compatibility.md)、性能の固定入力・counter・承認済み基準は
[core-benchmark-baseline.md](core-benchmark-baseline.md) を参照する。実行履歴をここへ追記しない。

## 自動検証と完了条件

変更範囲に応じて少なくとも次を実行する。

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo bench --package inkpod-core --bench core_workflows -- --quick
cargo doc --package inkpod-core --all-features --no-deps
cmake --preset windows-x64-debug
cmake --build --preset windows-x64-debug
ctest --preset windows-x64-debug --output-on-failure
```

実際の preset 名を使い、rustdoc は実行 shell に応じて `RUSTDOCFLAGS=-D warnings` 相当を設定する。非 Windows 環境でも Rust 検証を完了し、Win32 は Windows CI で検証する。実行できなかった検証を隠さない。

必要なテストは次を含む。

- Core: coordinate、tile indexing、selection algebra、Undo/Redo、serialization round-trip、fill/filter/transform/composite の unit/property/golden test
- format: `.inkpod` current-version round-trip、非現行 version 拒否、malformed/cancel test
- ABI: header の C11/C++20 include、ownership、NULL/短い structure/未知 enum/二重 release の negative test
- Windows: MSVC `/W4 /permissive-`、create/render、resize/DPI/device lost smoke test
- Windows multi-pane resize: 実 `HWND` の縦横 resize で anchor された child が最終差分だけ移動し、親子に deferred update region が残らず、geometry-only 経路が control／tab／list の再生成や内容 reset を起こさないことを確認する。非表示 smoke では `WM_PAINT` 件数を合否条件にせず、静的 contract と update-region／geometry 検査を組み合わせ、少なくとも一つの可視経路で旧 frame／背景 pixel が残らないことを検証する。
- Windows right-pane structure resize: 十分な高さの選択 tab へ pane を追加／削除して既存 pane が同時に縮小／拡大する可視経路を検証し、全 affected pane の最終 geometry、同期 paint、親子 update region の解消、既存 `HWND`、list の reset 不在／count／selection／top index、および有効な scroll position の保持または必要な範囲 clamp を確認する。focus は layout 通知で暗黙に reset せず、pane 表示 command が指定する新規 pane への移動だけを別に検証する。
- Windows hardening: queue saturation、close 中 input、active stroke、stale snapshot、save failure、allocation failure、shutdown race の fault injection、tab/window/layout/device reset の反復 soak、keyboard/UI Automation/high contrast/DPI/screen reader/IME の再現可能な確認
- CI: Rust と Windows の configure/build/test。新規 warning を放置しない

ピクセル完全一致でない処理も、色空間、rounding、境界条件を固定して小さな明示 tolerance を使う。第三者作品を golden fixture に使わない。

機能を完了扱いできるのは、次をすべて満たす場合だけである。

- UI から Core まで動く縦切り、または明示された Core-only scope になっている
- success、no-op、invalid、cancel、Undo/Redo、必要な save/reopen をテストしている
- ABI ownership、lifetime、thread 規則を文書化している
- `docs/compatibility.md` の requirement、状態、test、既知差分を更新している

互換状態は `Not started`、`In progress`、`Experimental`、`Verified`、`Blocked` のいずれかとする。test がない機能を `Verified` にしない。


## 実行環境と生成物

- Windows の検証基準は MSVC C++20 / Visual Studio 2022 または 2026 x64。
  対象 architecture の developer environment を明示し、configure/build/test で同じ preset を使う。
  ほかの実在 preset は `windows-x64-release`、`windows-arm-debug`、`windows-arm-release`。
- 全構成で C/C++ `/MT` と Rust `-C target-feature=+crt-static` を揃え、Debug でも `/MTd` を使わない。
- 非対話コマンドは shell profile に依存させない。POSIX shell では
  `RUSTDOCFLAGS='-D warnings' cargo doc --package inkpod-core --all-features --no-deps`、
  PowerShell では rustdoc の前に `$env:RUSTDOCFLAGS = '-D warnings'` を設定する。
- 文書だけの変更でも、変更した文書を入力にする生成物・契約テストと参照整合を確認する。
  対応する入力を変更した場合は以下の check を使う。

```text
python3 scripts/generate_inkscript_reference.py --check
python3 scripts/generate_windows_localization.py --check
cargo test --package inkpod-core --all-features --test route_inventory --test inkscript_registry
git diff --check
```

生成 reference と route inventory は実行可能な検証入力でもある。文書整理を理由に
生成結果との比較、ID の全単射、header/export drift の検査を削除しない。

## 公開契約と証拠

固定 seed、bounded case、失敗時の replay 情報を持つ state-machine/property test で、
determinism、failure/cancel atomicity、no-op stability、Undo/Redo round-trip、redo tail 切断と
非 active branch 保持、revision separation、savepoint、ID integrity を public API から観測する。
OS entropy、test 実行順、private field bridge に依存させない。局所的不変条件だけを実装 file に colocate する。

同じ executor 同士の比較だけで pixel の正しさを主張せず、小さい明示的な期待画像・mask を持つ。
Core の成功だけで Windows の到達性・画素結果・物理入力まで保証したと書かない。
Windows の実機検証と記録様式は [windows-release-checklist.md](windows-release-checklist.md) を使う。

同じ入力・構成の失敗と単独再実行の成功を区別し、原因未特定の間欠失敗を解決済みにしない。
最新の代表結果は対象 revision、環境・構成、検証範囲、未検証事項を伴って互換性表で置き換える。
過去の詳細 log は Git 履歴に任せ、承認済み性能基準の全 sample と意味 counter は基準文書に保持する。
