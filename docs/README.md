# Documentation map

[AGENTS.md](../AGENTS.md) は常時適用する短い開発指針、[SPEC.md](../SPEC.md) は機能・挙動・
不変条件・要件 ID の正本。以下は必要なときだけ読む専門資料であり、全件の通読を要求しない。

| 変更・確認する対象 | 読む文書と役割 |
| --- | --- |
| 現在の実装範囲・未完了項目 | [compatibility.md](compatibility.md): 要件別状態、既知差分、代表検証の唯一の記録 |
| 検証と完了条件 | [verification.md](verification.md): コマンド、構成、公開契約・Windows の受入条件 |
| 責務・所有権・thread・cache・Win32 layout | [architecture.md](architecture.md): 現行の実装境界と専門的な設計契約 |
| C ABI | [ffi.md](ffi.md): 所有権・寿命・失敗・呼出順。正確な宣言は公開 C header |
| 保存・復旧・codec | [file-format.md](file-format.md): 現行 bytes、上限、保存・復旧契約 |
| 再現性と数値 | [determinism.md](determinism.md): canonical 数値と cross-architecture replay の検証 |
| 性能 | [core-benchmark-baseline.md](core-benchmark-baseline.md): 固定 workload/counter、承認済み envelope と全基準 sample |
| Sequence の操作遅延 | [sequence-switch-performance.md](sequence-switch-performance.md): 現行の独立測定手順と観測限界 |
| Rust/C ABI/Windows route | [primitive-route-inventory.md](primitive-route-inventory.md): テストが直接検証する機械可読 inventory |
| Windows command ownership | [windows-command-inventory.md](windows-command-inventory.md): command/state の入口と担当 |
| Windows 設定保存 | [windows-application-data.md](windows-application-data.md): 設定・shortcut・workspace の現行格納契約 |
| InkScript の対応関係 | [inkscript-traceability.md](inkscript-traceability.md): 要件、registry、owner、証拠の対応 |
| InkScript command signature | [inkscript-command-reference.md](inkscript-command-reference.md): catalog からの生成物。手編集しない |
| Windows release の実機確認 | [windows-release-checklist.md](windows-release-checklist.md): 再現可能な実機検証手順 |
| 配布物の生成 | [windows-packaging.md](windows-packaging.md): package の生成・検証・公開手順 |
| 配布ライセンス | [third-party-notices.md](third-party-notices.md): 必須の第三者通知 |
| API HTML 生成 | [api/README.md](api/README.md): Doxygen の生成手順 |

## 更新規則

- 一つの情報は一つの正本へ置き、他文書からはリンクする。SPEC に実装ログを、AGENTS に詳細仕様を追加しない。
- 完了した計画・調査プロンプト・古い実行記録は削除し、必要な経緯は Git 履歴から確認する。
  現在も有効な制約・判断理由・未解決事項・承認済み性能基準は削除前に正本へ移す。
- 挙動の食い違いは現行 SPEC と実装・テストを照合し、整理だけを理由に仕様や合否基準を変えない。
- 文書の削除・移動時はリンク、code comment、テストと生成 script からの参照を確認する。
- `implementation-status.md` と `inkscript-performance-proposal.md` は既存リンク用の移転案内だけを残す。
  新しい情報は案内先へ置く。過去の監査成果物を現在の仕様・作業指示として扱わない。
