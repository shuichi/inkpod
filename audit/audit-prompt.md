# コード監査の依頼テンプレート

ユーザーが監査を依頼した場合にだけ使う補助テンプレート。通常の実装指示ではない。
対象、baseline、出力先、実行モデルは依頼時のユーザー指定を優先する。

## 境界

- AGENTS、SPEC の関連要件、対象 code/test を読み、必要な専門資料だけを docs/README から選ぶ。
- 監査だけの依頼では source、test、fixture、設定、既存文書、benchmark、Git index を変更しない。
  通常の ignored build/test artifact と依頼された報告書の作成だけを行う。
- 最初と最後に HEAD、working tree、toolchain、対象・除外・実行構成を記録する。
  既存差分は保護し、途中変更と重なる結論は観測対象の revision を限定する。
- 外部参照・公開、commit/push/PR、修正への移行は依頼の権限を越えて実行しない。

## 調べる境界

1. Rust の文書・画像・primitive・transaction・journal・Undo/Redo・cache-free replay。
2. C ABI の validation・layout・所有権・thread・panic/error と header/export の一致。
3. Windows の command target・queue・snapshot lifetime・Core/Renderer owner・shutdown。
4. 保存・復旧の staging、source authority、形式の拒否条件、テストが観測する利用者向け結果。

同じ executor 同士の一致、source scan、成功 status、文書や test 名だけでは pixel correctness や
UI の到達性を証明しない。文書 replay と application-session replay、canonical bitmap と
GPU/OS 表示を分ける。性能仮説は実測結果と分け、重なる経路の推定値を足し合わせない。

## 報告

各 finding は要件、対象 revision/file、具体的な trigger、期待値と観測、再現手順、影響、
反証・既存 guard の確認、未検証範囲を伴う。過去の finding は現行 code で再検証する。
実行 command と exit code、成功・失敗・未実行を分け、単独再実行の成功で full-run failure を消さない。
修正候補は実施済みと書かず、判断に必要な範囲に絞る。既存報告書を上書きしない。
