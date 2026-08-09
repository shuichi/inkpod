# Documentation map

このディレクトリは、現在も実装判断に使う文書と、過去の判断を説明する文書を分離する。
製品の機能・挙動・要件 ID の正本は [`../SPEC.md`](../SPEC.md)、常時適用する開発境界と
品質基準は [`../AGENTS.md`](../AGENTS.md) であり、`docs/` はそれらを複製しない。

## 現行の正本

| 文書 | 維持する情報 |
|---|---|
| [`architecture.md`](architecture.md) | 現在のコンポーネント、所有権、スレッド、状態遷移、描画・キャッシュ境界 |
| [`compatibility.md`](compatibility.md) | `SPEC.md` の要件 ID ごとの現在の状態、代表証拠、既知差分 |
| [`implementation-status.md`](implementation-status.md) | 現在実装、未完了項目、安定した既知差分、直近の代表検証 |
| [`ffi.md`](ffi.md) | 現行 C ABI の所有権、有効期間、スレッド、失敗時の契約 |
| [`file-format.md`](file-format.md) | 現行 `.inkpod` とアプリケーション固有形式の正確なバイト契約 |
| [`determinism.md`](determinism.md) | 現行のクロスアーキテクチャ再現性契約 |
| [`core-benchmark-baseline.md`](core-benchmark-baseline.md) | 現行 workload、意味ゲート、承認済み環境 envelope、再測定手順 |
| [`primitive-route-inventory.md`](primitive-route-inventory.md) | テストが直接検証する、現行 Rust/C ABI/Windows route の機械可読 inventory |
| [`windows-command-inventory.md`](windows-command-inventory.md) | 現行 Windows command surface と ownership の要約 |
| [`windows-release-checklist.md`](windows-release-checklist.md) | 現行 Windows リリース候補の再現可能な native 検証手順 |
| [`windows-packaging.md`](windows-packaging.md) | 現行 Windows package の生成・検証・公開手順 |
| [`third-party-notices.md`](third-party-notices.md) | 配布に必要な第三者ライセンス通知 |
| [`api/README.md`](api/README.md) | C ABI API 文書の生成手順 |

## 歴史資料

[`legacy.md`](legacy.md) は、完了した Core リファクタリング／GUI モダナイゼーションの
開始時点、旧 `.inkpod` v2、過去の性能校正・受入記録、旧 G13 native 観測の要約だけを持つ。
これらは現行挙動や合否判定の根拠にしない。削除前の完全な表、サンプル列、旧計画は Git 履歴を参照する。

## 更新規則

- 現行文書は現在値を置き換え、日付順の進捗ログや完了済み計画を追記しない。
- 現在も再現すべき契約、上限、所有権、失敗時挙動、検証手順は現行文書に残す。
- 旧構造との比較、採用までの経緯、廃止形式、過去の個別計測は `legacy.md` に要約する。
- `legacy.md` は仕様の正本ではない。歴史記録を追加する必要がある場合も、現行文書の更新を先に行う。
