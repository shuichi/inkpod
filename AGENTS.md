# inkpod 開発ガイド

この指示はリポジトリ全体に適用する。inkpod はアニメーション彩色ワークフローを、
長期保守可能なクロスプラットフォーム設計で実装する。

## 正本と読む範囲

- 優先順位は、今回のユーザー指示、`AGENTS.md`、`SPEC.md`、テスト済みの既存契約とする。
- 着手時に `git status`、既存差分、`SPEC.md` の関連要件、対象 code/test を確認する。
- 現在の実装範囲・既知差分・代表検証は `docs/compatibility.md` の該当行だけを読む。
- 詳細は `docs/README.md` から変更対象に関係する文書を選ぶ。全資料の通読を前提にしない。
- 製品の挙動・不変条件は `SPEC.md`、専門的な設計・形式・手順は対応する `docs/` 文書に置く。
  本書へ機能仕様、実装手順の詳細、版番号一覧、進捗ログを複製しない。
- 過去の計画・調査・完了記録は Git 履歴を参照し、現在の仕様や実装指示として扱わない。
- 通常の実装で外部の旧製品マニュアルや画像を参照しない。旧 UI、画像、アイコン、文面、
  商標表示を複製せず、操作の意味・データ・座標・保存結果の合理的な互換性を目指す。

## 維持する設計境界

- Rust Core が文書状態、画像処理、選択、履歴、永続化、入力解釈、不変 snapshot を所有する。
  C++/Win32 は UI、OS adapter、thread/queue、DPI、アクセシビリティ、GPU renderer を担当し、
  Core の処理を別実装しない。接続は versioned C ABI のみとする。
- Rust domain code と公開型へ OS 型を入れない。ファイル identity・排他・原子的置換の
  OS API は `inkpod-io` の非公開 platform backend にだけ隔離する。依存を循環させない。
- Windows は Common Controls v6 と Unicode API、Canvas は D3D11/DXGI/Direct2D を使う。
  UI/Input、Core engine、Renderer の owner thread と session/view の所有権を守る。
  詳細は `docs/architecture.md`、C ABI の所有権・寿命・失敗契約は `docs/ffi.md` に従う。
- 文書変更は一つの Rust canonical executor と明示 commit を通す。失敗・取消・stale・no-op
  で部分状態や ID を公開しない。データ、座標、revision、保存、性能の不変条件は `SPEC.md` に従う。
- `inkpod-core` は safe Rust とし、`unsafe` は FFI と証明可能な image hot path に局所化して
  各 block の safety invariant を記述する。外部入力・allocation・処理量には上限を設ける。
- `lib.rs`／`mod.rs` は module 宣言と意図した re-export を中心にする。責務ごとに分割し、
  便宜的な `helpers`／`common`／`utils`、巨大 context、空 crate の量産を避け、visibility を絞る。
  `main.cpp` は起動 mode と runner に限定する。private Windows 宣言を公開 C ABI へ出さない。
- Rust は stable / edition 2024、Windows は MSVC C++20、CMake を build の入口とする。
  Rust staticlib は `inkpod-ffi` だけ。全構成で C/C++ `/MT` と Rust `+crt-static` を揃える。
- 依存は最小限とし、追加時は配布ライセンスと `docs/third-party-notices.md` を更新する。
  build に個人の絶対 path、手動 copy、shell profile 依存を持ち込まない。

## 変更と判断

1. 短い計画を示し、ユーザー変更を保護して、依頼範囲の実装と検証まで進める。
2. 挙動を変える場合は公開契約をテストで先に固定する。一つの変更では一種類の意味上の
   risk を扱い、機械的な移動・rename、algorithm 変更、公開境界変更を分ける。
3. 仕様と既存テストだけで安全に決められない製品挙動は、選択肢・影響・解除条件を示して
   ユーザー判断を求める。対象外の refactor や formatting を混ぜない。
4. フォーマットフリーズ前は application 固有形式を current-only とし、migration や互換 shim
   を追加しない。schema／replay semantics の変更では `SPEC.md` の最上位 version 更新規則を守る。
5. 最適化は再現可能な before/after と意味 counter で評価する。workload、harness、環境別
   envelope、canonical `revision-max` 式の変更は、理由・全 sample・counter を示して明示承認を得る。
   測定値に合わせた基準緩和、暗黙の画質低下、重いテストを理由のない ignore へ移すことはしない。
6. commit、push、PR、外部公開はユーザーが明示的に依頼した場合だけ行う。

## 検証と完了

- 変更範囲に応じて `docs/verification.md` の format・lint・test・build を実行する。
  Win32 layout 変更は `docs/architecture.md` の multi-pane resize contract と可視経路を検証する。
  非 Windows でも Rust 検証を行い、実行できない Windows／実機項目を明示する。
- 非対話コマンドは `login: false`、PowerShell は `-NoProfile` を使う。profile が必要な診断は
  理由を記録する。wrapper だけ残った場合は child の有無を確認し、wrapper を終了して同じ
  コマンドを no-profile で再実行し、exit code 0 を取得する。成功を推測しない。
- 公開契約は public API から結果を観測する。private field bridge やテスト専用 public accessor
  を追加しない。局所的不変条件だけを実装 file に colocate する。
- rustdoc は座標・単位・範囲、ID の所属と寿命、成功／no-op／error、状態への影響、所有権、
  cancellation／panic を必要に応じて記す。invalid input は明示 error にする。
- placeholder、常時成功 stub、未接続 UI を完成扱いせず、test failure を削除・ignore・
  過大 tolerance で隠さない。第三者作品を golden に使わず、ユーザー画像や path を過剰に log しない。
- UI→Core の縦切り、または明示された Core-only scope を検証し、success／no-op／invalid／
  cancel／Undo・Redo／必要な save・reopen と ABI ownership を確認する。
- 状態・既知差分・代表検証が変わった場合だけ `docs/compatibility.md` を更新する。
  テストのない機能を `Verified` にしない。完了記録を別の進捗文書へ追記しない。
- 最終報告には利用者向け挙動、重要な設計判断、変更 file、検証結果、未検証事項・既知差分を簡潔に示す。
