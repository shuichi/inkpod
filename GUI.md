# Inkpod GUI モダナイゼーション実装計画

## 文書情報

| 項目 | 内容 |
|---|---|
| 状態 | Active。G0、G1、G2、G3、G4、G5、G6、G7、G8、G9、G10、G11、G12 完了、G13 実装中 |
| 対象 | Windows frontend、C ABI adapter、renderer、UI に必要な Rust Core 接続 |
| 決定日 | 2026-07-31 |
| 採用方式 | Win32/Common Controls、タブ、分割ビュー、制約付きドック、複数トップレベルウィンドウ |
| 非採用方式 | 古典的 MDI、全面的な UI framework 移行、完全自由配置型ドッキング |
| 仕様の正本 | `PROMPT.md`。本書は合意済み GUI 方針を実装へ分解する計画であり、実装開始時に `PROMPT.md` へ要件を反映する |

本書は完成時の見た目を固定するデザイン仕様ではない。PaintMan が提供していた情報量と作業上の自由度を、Windows 11 に適合する操作体系、明示的な所有権、テスト可能な状態モデルへ置き換えるための実装計画である。

---

## 1. 決定事項

Inkpod の Windows GUI は、次の方式で発展させる。

1. Win32、Common Controls v6、Direct2D/D3D11/DXGI の既存技術境界を維持する。
2. 古典的 MDI は採用せず、文書はエディターグループ内のタブとして表示する。
3. 一つのトップレベルウィンドウ内に、最大二つのエディターグループを縦または横に分割できるようにする。
4. パネルは任意の入れ子を許す完全自由ドックではなく、上、左、右、下、フローティング、非表示、自動格納という制約付きドックにする。
5. 同一プロセス内に複数のトップレベルワークスペースウィンドウを作成できるようにする。
6. 同じ文書を複数のビュー、分割領域、トップレベルウィンドウで表示する場合、文書状態、Undo/Redo、dirty、保存先は共有し、zoom、pan、flip、表示補助、表示中 frame などのビュー状態は分離する。
7. 現在の固定配置は廃止するのではなく、初期ワークスペースプリセットとして維持する。
8. GUI の再構成を理由に、既存の command、shortcut、pane、編集機能、保存動作、アクセシビリティを欠損させない。

完成形は「一つの巨大な自由配置画面」ではなく、次の三段階の構造を持つ。

```text
ApplicationHost
├─ WorkspaceWindow 1
│  ├─ DockHost
│  │  ├─ Tool / Tool Options / Color / Layer / Auxiliary panes
│  │  └─ EditorArea
│  │     ├─ EditorGroup A ─ tabs + one visible Canvas slot
│  │     └─ EditorGroup B ─ tabs + one visible Canvas slot
│  └─ menu / status / command context
├─ WorkspaceWindow 2
│  └─ 同じ構造
├─ DocumentSession registry
├─ CoreHost
└─ RendererHost
```

---

## 2. 目的と非目的

### 2.1 目的

- 複数文書を同時に開き、タブ、分割、別ウィンドウで整理できること。
- 一つの文書を全体表示と拡大表示、別 frame、別表示補助などで同時に確認できること。
- PaintMan 相当のパレット群を、作業内容と画面寸法に応じて表示、格納、移動できること。
- どの文書、ビュー、pane、job を command が対象にするかを常に一意に決定できること。
- 既存の Rust Core、C ABI、Windows frontend、renderer の責務境界を崩さないこと。
- DPI、high contrast、keyboard navigation、screen reader、複数 monitor、device lost、異常終了からの回復を設計の一部として扱うこと。
- 各マイルストーンを単独で統合でき、途中段階でも既存 GUI を使用可能にすること。

### 2.2 非目的

- WinUI、WPF、Qt、MFC、Electron、Dear ImGui などへの全面移行。
- PaintMan のウィンドウ配置、画像、アイコン、文面の模写。
- 子ウィンドウをデスクトップのように重ねる古典的 MDI。
- pane を無制限に再帰分割できる IDE 級の完全自由ドック。
- C++ 側に第二の document model、Undo、保存規則、画像処理を作ること。
- workspace layout を `.inkpod` 文書形式へ保存すること。
- GUI 再編だけを理由に C ABI version を更新すること。
- 計測前に Core thread を文書数だけ増やすこと。
- 未接続のボタン、空 pane、常時成功する stub を先に大量配置すること。
- 現在対応していないファイル形式を、placeholder、disabled menu、拡張子だけの選択肢として表示すること。

---

## 3. 実装中も維持する不変条件

### 3.1 所有権と thread

- 全トップレベル `HWND`、子 `HWND`、Common Controls、menu、dialog は一つの UI/Input thread に所属する。
- 全 `InkpodCore` handle の create、dispatch、snapshot build、destroy は Core engine thread に固定する。
- `InkpodCore` は引き続き single-writer とし、renderer が読むのは release まで不変な snapshot だけとする。
- D3D11/DXGI/Direct2D device、swap chain、Present は Renderer thread に固定する。
- worker thread から `HWND` を直接操作しない。
- Rust 所有 pointer を `WPARAM` または `LPARAM` に裸で格納しない。
- queue へ渡す object は、enqueue の成功、失敗、破棄、window close の全経路で release 責務を一意にする。
- Core が lock を保持したまま C++ callback を呼ばない。

### 3.2 文書とビュー

- 一つの `DocumentSession` は一つの `InkpodCore` handle を所有する。
- 同じ文書を表示する全 `DocumentView` は、その `DocumentSession` を共有する。
- `DocumentView` ごとに Core の view logical state を持ち、文書 raster、layer、history を複製しない。
- `CanvasSurface` はタブではなく、画面上で可視な `EditorGroup` の Canvas slot に所属する。非表示タブごとに swap chain を保持しない。
- 文書内の ID や revision は Core ごとの局所値である。frontend の非同期 routing では必ず `DocumentSessionId` と組み合わせる。
- 同じ file identity を通常操作で二つの独立した `DocumentSession` として開かない。

### 3.3 command

- menu、shortcut、context menu、pane button は同じ command ID と enable/checked state を共有する。
- command 発行時に対象を `CommandContext` として確定し、非同期実行時に「その時点の active tab」を再解決しない。
- destructive command は成功時だけ commit し、失敗、cancel、stale revision で部分結果を残さない。
- menu bar を canonical な入口として維持し、toolbar は前提にしない。

### 3.4 移行規律

- 各マイルストーンで visible behavior、ownership、failure path、test、文書を同時に完成させる。
- 新旧二つの所有者を長期間併存させない。移行対象の互換 shim は同じマイルストーンで除去する。
- 巨大な `main_window_runtime.cpp` を単純に細分化するのではなく、所有権が移る単位で分割する。
- 現在の固定一画面構成を各中間段階の回帰基準にする。
- Windows GUI を伴うマイルストーンは native x64 smoke test を必須とし、thread、renderer、ABI 境界を変える節目では ARM64 も検証する。

---

## 4. 用語、識別子、scope

### 4.1 主要 object

| object | 責務 | 所有者 | 複数化の単位 |
|---|---|---|---|
| `ApplicationHost` | process lifetime、window/document registry、global service | `wWinMain` | process に一つ |
| `WorkspaceWindow` | 一つのトップレベル window、menu、dock、editor area、status | `ApplicationHost` | top-level `HWND` ごと |
| `DocumentSession` | file identity、dirty、save/recovery、Core handle binding | `ApplicationHost` | 開いている論理文書ごと |
| `DocumentView` | zoom、pan、flip、active frame、表示補助、Core view ID | `DocumentSession` | 同一文書の表示ごと |
| `EditorGroup` | tab collection、active view、Canvas slot、分割位置 | `WorkspaceWindow` | 初期版は一 window 最大二つ |
| `CanvasSurface` | 可視 view を描画する swap chain と per-surface resource | `RendererHost` | `EditorGroup` ごと |
| `PaneInstance` | tool、layer、color、locator 等の表示と target policy | `WorkspaceWindow` または floating owner | pane instance ごと |
| `JobSession` | batch、save、decode、filter 等の進捗と cancellation | `ApplicationHost` | 長時間処理ごと |

### 4.2 強い型を持つ識別子

少なくとも次を相互代入できない C++ strong type として導入する。

- `WorkspaceWindowId`
- `DocumentSessionId`
- `DocumentViewId`
- `EditorGroupId`
- `CanvasId`
- `PaneInstanceId`
- `JobSessionId`
- `Generation`

Rust Core が返す document 内 ID、view ID、revision と、frontend の ID は別 namespace とする。queue item、snapshot、command、drag token、timer には必要最小限の ID と generation を値として格納する。

### 4.3 pane の target scope

| scope | 例 | target の決め方 |
|---|---|---|
| Application | tool、color、shortcut、workspace selector | application または focused workspace |
| Follow active view | layer、tool options、locator | focused workspace の active editor group と active view |
| Pinned document | reference、layer 比較、subpalette | 明示的に固定した `DocumentSessionId` |
| Job | batch、progress、export queue | `JobSessionId` |

pane の title/header には、必要な場合だけ対象文書名または「アクティブに追従」を表示する。固定先文書を閉じたときは silent に別文書へ向けず、追従 mode へ戻して accessible notification を出す。

---

## 5. 完成時の操作契約

### 5.1 文書 tab

- `ファイル > 開く` は focused workspace の active editor group に tab を追加する。
- 既に同じ file identity が開かれていれば、その `DocumentSession` の既存 view を選択する。別 view が必要な場合は `ウィンドウ > 新しいビュー` を明示的に使う。
- untitled document は frontend が発行する UUID で識別する。
- tab は文書名、dirty、read-only、処理中、エラーを compact な状態表示で示す。
- tab を閉じる操作は view を閉じる。最後の view を閉じる場合だけ文書 close となり、dirty 確認を行う。
- `文書を閉じる` は、その文書に属する全 window、全 group の view を列挙した上で一度だけ確認する。
- application exit では dirty な `DocumentSession` ごとに一度だけ保存判断を求める。同じ文書の view 数だけ確認 dialog を出さない。
- `名前を付けて保存` に成功した時点で file identity registry を atomically 更新する。競合する path が既に開かれていれば置換せず、明示的な解決を求める。

### 5.2 分割ビュー

- 初期版は一つの `WorkspaceWindow` に最大二つの `EditorGroup` を許可する。
- 分割方向は左右または上下とし、再帰分割はしない。
- 各 group は独立した tab strip、active view、Canvas slot を持つ。
- `右へ分割`、`下へ分割`、`別グループへ移動`、`別グループに新しいビュー`、`グループを閉じる` を menu と keyboard から実行できる。
- 同一文書を二つの group に表示する場合、編集結果、dirty、Undo/Redo は共有し、zoom、pan、flip、active frame、overlay は view ごとに独立する。
- splitter の移動中は layout preview を優先し、Canvas resize と Present を過剰に発行しない。
- 最小 Canvas 寸法を下回る場合は split ratio を clamp し、狭い画面で pane を失わない。

### 5.3 制約付き dock

使用可能な zone は次に限定する。

- `TopContext`: tool options 等の横長 pane。
- `Left`: tool、color 等。
- `Right`: layer、locator、light table、reference 等。
- `Bottom`: sequence、preview、job/progress 等。
- `Floating`: descriptor が許可する pane のみ。
- `Hidden`: menu から再表示可能。
- `AutoHide`: secondary pane のみ。edge strip から keyboard と pointer で展開可能。

各 zone は pane の tab stack と一方向の比率分割だけを持つ。zone の中へ任意の dock tree を作らない。pane descriptor は default zone、allowed zones、scope、multiplicity、float/autohide 可否、最小寸法を宣言する。

フローティング pane は通常の owned top-level window とし、常時最前面、非アクティブ window、独自 taskbar item にはしない。閉じる操作は instance の破棄ではなく既定では非表示とし、pane 固有状態を保持する。

### 5.4 複数トップレベルウィンドウ

- 各 `WorkspaceWindow` は完全な menu、dock、editor area、status を持つ。
- `新しいウィンドウ`、`ビューを新しいウィンドウへ移動`、`新しいウィンドウに複製ビュー` を提供する。
- 同じ `DocumentSession` の view を複数 window に配置できる。
- focus、menu state、status、pane target は window ごとに解決する。application-global な active document pointer は設けない。
- 全 window は同じ UI/Input thread 上に置き、`WM_QUIT` は最後の workspace window が閉じた時だけ発行する。
- window close は、その window から消える view のうち、他 window にも view が存在しない dirty session だけを確認対象にする。

### 5.5 drag and drop

- 最初に menu/keyboard command で移動、複製、新規 window の全機能を完成させ、その後 tab drag を追加する。
- tab drag は同一 group 内 reorder、別 group、別 window、window 外 drop による新規 window を扱う。
- drag 中は raw pointer ではなく、有効期限付き `DragToken` と ID/generation を使用する。
- active stroke、pointer capture、modal preview 中は移動を開始しない。操作を完了または cancel してから移動する。
- `Esc` で drag を cancel し、元の位置を完全復元する。

---

## 6. 目標アーキテクチャ

### 6.1 object graph

```text
ApplicationHost
├─ CommandRegistry / ShortcutService / ClipboardService
├─ DocumentRegistry
│  ├─ DocumentSession A ─ InkpodCore A ─ DocumentView A1, A2 ...
│  └─ DocumentSession B ─ InkpodCore B ─ DocumentView B1 ...
├─ WorkspaceWindowRegistry
│  ├─ WorkspaceWindow 1 ─ DockHost ─ EditorGroup A/B ─ Canvas slot A/B
│  └─ WorkspaceWindow 2 ─ DockHost ─ EditorGroup A/B ─ Canvas slot A/B
├─ CoreHost
│  └─ Core engine thread ─ session ID -> InkpodCore handle
├─ RendererHost
│  └─ Renderer thread ─ canvas ID -> CanvasSurface
└─ JobRegistry
```

`DocumentView` と `CanvasSurface` は一対一ではない。tab が非表示の間も `DocumentView` は存続するが、swap chain を持たない。group の active tab が変わると、その group の `CanvasSurface` が別の `DocumentView` に bind され、必要な snapshot を要求する。

### 6.2 UI/Input thread

UI/Input thread は次を行う。

- 全 window と Common Controls の作成、破棄、layout。
- focus から `CommandContext` を構築する。
- pointer history を Canvas client device-pixel 座標へ正規化する。
- `DocumentSessionId`、`DocumentViewId`、input sequence、generation を付けて bounded Core queue へ投入する。
- pane state、tab state、menu state、status の表示を更新する。
- Core や Present を同期的に待たない。

### 6.3 CoreHost

CoreHost は当初、一つの Core engine thread 上に複数の `InkpodCore` handle を保持する。

```text
CoreWorkItem {
    document_session_id,
    document_view_id?,
    generation,
    sequence,
    command/input payload
}
```

この方式では、複数文書の状態分離には支障がない一方、長時間処理は文書間でも直列化される。最初から Core thread を文書数だけ作らず、操作遅延を計測してから、必要な処理だけを pure worker job と revision 検査付き commit に分離する。

shortcut 設定のように application-global であるべき状態が現在 Core instance ごとに存在する場合は、`ApplicationHost` が正本を持ち、全 Core へ同期する。C ABI は複数 handle を扱える既存契約を優先し、GUI 再編だけでは変更しない。

### 6.4 RendererHost

現在の Canvas ごとの renderer thread から、次の構造へ移行する。

- `RendererHost`: 一つの長寿命 renderer thread、共有 D3D11 device、D2D device、factory、cache budget。
- `CanvasSurface`: Canvas `HWND` ごとの swap chain、D2D target/context、size、DPI、visibility、retained snapshot、overlay、generation。
- `SnapshotEnvelope`: `DocumentSessionId`、`DocumentViewId`、`CanvasId`、document revision、view revision、surface generation、snapshot ownership。

snapshot は routing key と generation が一致した場合だけ描画する。不一致、非表示化、surface 破棄、queue 置換の全経路で release する。device lost 時は全 `CanvasSurface` の GPU resource だけを再構築し、Core の文書状態と view state は失わない。

### 6.5 shutdown 順序

終了時は次の順序を固定する。

1. 新規 command、input、window 作成を停止する。
2. active stroke と modal preview を完了または cancel する。
3. dirty session の保存判断を完了する。
4. pane、tab、window の配置を保存する。
5. Canvas を unbind し、RendererHost の pending snapshot を drain/release する。
6. CoreHost の pending work を cancel/drain し、全 Core handle を owner thread で destroy する。
7. renderer device と COM resource を owner thread で破棄する。
8. 最後の workspace `HWND` を破棄し、`WM_QUIT` を発行する。

---

## 7. 現状からの主な差分

計画作成時点の Windows frontend は、概ね次の前提を持つ。

| 現状 | 目標 |
|---|---|
| stack 上の一つの `AppContext` | process 所有の `ApplicationHost` と複数の `WorkspaceWindow` |
| 一つの `DocumentShellState` | `DocumentRegistry` 内の複数 `DocumentSession` |
| 一つの `CoreEngine`、一つの Core handle | `CoreHost` が複数 Core handle を session ID で管理 |
| 一つの tab control と一つの Canvas | 最大二つの `EditorGroup`、group ごとに一つの可視 Canvas slot |
| Canvas ごとの `RenderThread` | 一つの `RendererHost` thread と複数 `CanvasSurface` |
| 固定 top/left/right/center layout | 制約付き `DockHost` と named workspace |
| 同一文書の view tab が中心 | 複数文書 tab と同一文書 view の両方 |
| 固定個数を前提にした palette message handling | `WorkspaceWindowRegistry` と pane instance 列挙 |
| active document が暗黙的 | 発行時に固定した `CommandContext` |

Rust Core には複数文書を格納する container を新設しない。複数の Core handle を frontend の `DocumentSession` で束ねる。この方が document failure、Undo、savepoint、recovery、close の境界が明確であり、現在の opaque handle ABI も維持できる。

---

## 8. マイルストーン一覧

```text
G0 仕様と基準固定
 └─ G1 ID・CommandContext
     └─ G2 ApplicationHost への所有権分解
         ├─ G3 複数 DocumentSession 対応 CoreHost
         └─ G4 複数 Canvas 対応 RendererHost
             └─ G5 複数文書 tab
                 └─ G6 分割 EditorGroup
                     ├─ G7 制約付き DockHost
                     │   ├─ G8 pane target と不足 pane
                     │   └─ G9 layout 永続化・workspace
                     └─ G10 複数トップレベル window
                         └─ G11 tab drag・tear-out
                             └─ G12 単一論理 instance・session recovery
                                 └─ G13 hardening・release gate
```

| milestone | 利用者向け成果 | 主な risk | 推奨変更単位 |
|---|---|---|---|
| G0 | 挙動契約が仕様化される | 要件の曖昧さ | 文書のみ |
| G1 | 見た目は不変 | 誤 routing | ID/command model + unit test |
| G2 | 見た目は不変 | ownership 回帰 | 一画面互換の縦切り |
| G3 | 複数 Core を安全に保持可能 | thread affinity、ID 衝突 | CoreHost + tests |
| G4 | 複数 Canvas を安全に描画可能 | snapshot lifetime、device lost | RendererHost + smoke |
| G5 | 一 window で複数文書 tab | save/close 誤対象 | session/tab 縦切り |
| G6 | 左右・上下の分割表示 | focus、resize、同期 | editor group 縦切り |
| G7 | primary pane の dock | layout corruption | DockHost model + UI |
| G8 | auxiliary pane と target 固定 | 別文書への誤操作 | pane ごとの縦切り |
| G9 | workspace 保存・復元 | DPI/monitor 差 | persistence + recovery |
| G10 | 複数 top-level window | shutdown、menu state | multi-window 縦切り |
| G11 | tab drag と tear-out | capture、UAF | command 完成後に drag |
| G12 | 二重起動連携と全 recovery | IPC、privacy | activation + recovery |
| G13 | 性能・アクセシビリティ・異常系 | release 品質 | 計測と gate |

---

## 9. 詳細マイルストーン

## G0. 仕様、用語、回帰基準の固定

### 目的

現在の `PROMPT.md` にある固定 dock、一つの Canvas、同一文書 view tab を前提とした記述を、合意した GUI 方針へ更新し、実装者ごとの解釈差をなくす。

### 実装項目

- `PROMPT.md` の Windows GUI、ウィンドウ menu、frontend、threading の各節を更新する。
- 少なくとも次の requirement を追加または既存 requirement から分離する。最終 ID は既存体系との整合を確認して決める。
  - `WIN-002`: 複数トップレベル window と application activation。
  - `VIEW-004`: 複数文書 tab、同一文書 view、分割、移動。
  - `WORKSPACE-001`: 制約付き dock、named workspace、layout persistence。
  - `WORKSPACE-002`: pane scope、follow/pin、command targeting。
  - `SESSION-001`: 複数文書の close、save、autosave、recovery lifecycle。
- 「view を閉じる」「文書を閉じる」「window を閉じる」「application を終了する」の差を仕様化する。
- 同じ file を二重に開いた場合、Save As の競合、外部変更、read-only の扱いを仕様化する。
- 現在の一画面 smoke、menu command 数、C ABI surface、native test を baseline として記録する。
- `docs/architecture.md`、`docs/compatibility.md`、`docs/implementation-status.md` の更新条件を決める。

### テスト

- 実装前の Rust workspace test と Windows x64 configure/build/test を保存する。
- 現在の主要 menu、pane 表示、open/edit/undo/save/reopen、DPI resize、device lost smoke の確認表を作る。

### 完了条件

- 本書と `PROMPT.md` に矛盾がない。
- 全 user-visible operation に command、target scope、dirty/close 契約がある。
- 実装前 baseline の成功数と既知差分が追跡可能である。

### 完了記録

2026-07-31 に G0 を完了した。`PROMPT.md` へ複数 workspace window、複数文書 tab、最大二つの editor group、制約付き dock、pane scope、session lifecycle、発行時 command target の契約を反映し、`WIN-002`、`VIEW-004`、`WORKSPACE-001`、`WORKSPACE-002`、`SESSION-001` を `docs/compatibility.md` へ追加した。

実装前 baseline は `docs/gui-modernization-baseline.md` に固定した。単一 window/group の native smoke、281 production command、ABI v2 の 161 functions、Rust 177 tests と 1 doctest、Windows x64 Debug configure/build、CTest 11/11、既知差分を追跡できる。G0 では製品 code と C ABI を変更しておらず、これを G1 着手前の回帰基準とする。

---

## G1. frontend ID と CommandContext の導入

### 目的

複数化の前に、暗黙の active document、raw pointer、control index に依存する routing を除去する。

### 実装項目

- `apps/windows/app/identity.h` を追加し、4.2 の strong ID と hash/comparison を定義する。
- `apps/windows/app/command_context.h/.cpp` を追加する。
- `CommandContext` に少なくとも workspace、editor group、document session、document view、pane、job、generation を optional な値として持たせる。
- menu、shortcut、context menu、pane button の入口を一つの command routing API へ集約する。
- async command は `CommandContext` の copy を queue へ渡す。
- registry lookup が失敗した stale ID は安全な no-op または明示 error とし、現在 active な別文書へ fallback しない。
- timer ID、drag token、posted notification に generation を付ける。

### テスト

- strong ID の取り違えが compile error になること。
- focus が command 発行直後に移動しても、元の文書だけが対象になること。
- window/view close 後の queued command が別 object に再利用されないこと。
- NULL、unknown command、stale generation、scope 不足が安全に拒否されること。

### 完了条件

- visible behavior は従来と同じである。
- document/view を操作する command handler に暗黙の global active pointer がない。
- command state と command execution が同じ target 解決規則を使う。

### 完了記録

2026-07-31 に G1 を完了した。frontend の workspace window、document session、document view、editor group、Canvas、pane、job、generation を非互換の strong ID にし、UI/Input thread 所有の target registry と pointer-free な `CommandContext` を導入した。現在の menu、shortcut、pane button と main-window `WM_COMMAND` の入口は `IssueCommand` で発行時 target を確定する。今後追加する context menu もこの入口を使う。G1 で未接続の context-menu UI は新設していない。command state と execution は同じ owner-to-scope 解決を使う。

filter/effect、Batch、autosave、Canvas effect gesture、locator は発行時 context の copy を queue と completion へ渡し、document/view の置換・close、job 終了、generation 不一致で別 target へ fallback しない。timer、drag、posted notification は generation 付き token に移行し、locator は value token と bounded result queue によって raw C++ pointer の `LPARAM` 渡しを廃止した。C ABI は変更していない。strong ID 誤用、focus 変更、view close、document 置換、NULL/unknown/missing/stale、token 世代、従来の native GUI 操作は unit、structural、application smoke で回帰保護する。次の実装対象は G2 である。

---

## G2. AppContext を ApplicationHost と WorkspaceWindow へ分解

### 目的

現在の一画面構成を保ったまま、process、window、document、view の所有権を分ける。

### 実装項目

- 次の責務 object を導入する。
  - `ApplicationHost`: service、registry、thread host、shutdown coordinator。
  - `WorkspaceWindow`: top-level `HWND` と window-local state。
  - `DocumentSession`: file/recovery/Core binding の shell。
  - `DocumentView`: Core view ID と presentation state。
- global service とするものを `ApplicationHost` へ移す。
  - shortcut、clipboard、document registry、CoreHost、RendererHost、job registry、application settings。
- window-local state とするものを `WorkspaceWindow` へ移す。
  - handles、focus、menu state、dock/layout、editor groups、status presentation、window placement。
- document-local state と view-local stateを明示的に分ける。
- `main_window_runtime.cpp` は、所有者の移動に合わせて少なくとも window procedure、command router、input router、document presenter、status presenter へ分割する。
- 既存 `AppContext` を facade として長期併存させず、移行が終わる同じ milestone で除去する。
- 最初は registry に各一件だけを格納し、従来の一 window、一 document、一 Canvas の動作を維持する。

### テスト

- application、window、document、view の構築と逆順破棄。
- init の各 failure point から leak なく unwind できること。
- 既存 command state ownership と frontend boundary の structural test。
- 従来の open/edit/undo/save/reopen と window close smoke。

### 完了条件

- `wWinMain` の stack-local object lifetime に Core、Canvas、document の一意性を依存しない。
- window procedure から process-global service へは `ApplicationHost` の明示 API 経由で到達する。
- 現行 layout と操作に利用者向け差分がない。

### 完了記録

2026-07-31 に G2 を完了した。heap 所有の `ApplicationHost` を process composition root とし、一件ずつを保持する `WorkspaceWindowRegistry` と `DocumentRegistry`、window-local `WorkspaceWindow`、Core binding と file/recovery shell を持つ `DocumentSession`、Core view ID と presentation state を持つ `DocumentView` を導入した。top-level `HWND` の `GWLP_USERDATA` は `WorkspaceWindow` だけを保持し、window procedure はその明示的な `application` link を通して process service へ到達する。旧 `AppContext` は同じ milestone で除去した。

`main_window_runtime.cpp` の所有者入口を window procedure、command router、input router、document presenter、status presenter に分割した。現段階の registry は一 window、一 document であり、一つの Core engine thread と Canvas ごとの renderer thread は従来どおりである。複数 Core handle の `CoreHost` は G3、共有 renderer thread と複数 `CanvasSurface` の `RendererHost` は G4 で行い、G2 ではその実装を先取りしていない。owner model test は workspace/document の各 allocation failure、invalid initialization、置換失敗時の旧 owner 保持、view 追加/切替/破棄、逆順 clear と live allocation の復帰を検証し、structural gate と native x64 smoke は owner boundary、従来の open/edit/Undo/Redo/save/reopen、window close、DPI、device lost を検証する。C ABI と利用者向け layout/操作は変更していない。

---

## G3. 複数 DocumentSession を扱う CoreHost

### 目的

一つの Core engine thread 上で複数の独立した Rust Core handle を安全に管理する。

### 実装項目

- `CoreEngine` を `CoreHost` へ発展させ、owner thread 上に次の registry を持つ。

```text
DocumentSessionId -> CoreEntry {
    InkpodCore*,
    active Core view IDs,
    generation,
    input sequence state,
    active stroke state,
    pending operations,
    copied diagnostic
}
```

- create、open、import、command、input、snapshot、save、close、destroy の全 `WorkItem` に `DocumentSessionId` を付与する。
- Core 内で同じ数値 ID/revision が発生しても、session namespace で衝突しないようにする。
- stroke の begin/append/end/cancel の順序と非破棄保証を session/view ごとに維持する。
- error text は ABI 呼び出し直後に owner thread 上で copy し、別 Core 呼び出しや thread-local error に依存しない。
- session close は新規 work を拒否し、pending work と snapshot ownership を解決した後、owner thread 上で Core handle を destroy する。
- shortcut 等の application-global 設定は `ApplicationHost` を正本にし、新規 Core 作成時と変更時に同期する。
- 最初は長時間処理も同じ CoreHost lane で直列実行し、この制約を既知差分として記録する。

### テスト

- 二つの Core で異なる文書を作成し、edit、Undo/Redo、dirty、savepoint が分離されること。
- Core-local ID と revision が同値でも snapshot/command が混線しないこと。
- 一方の malformed input、cancel、panic 変換、save failure が他方へ影響しないこと。
- session close と同時に到着する input、snapshot request、worker result が stale として破棄されること。
- 全 Core API が同じ owner thread から呼ばれること。

### 完了条件

- 同時に二つ以上の `DocumentSession` を作成、操作、保存、破棄できる。
- C ABI ownership と thread 規則に変更がない、または必要な変更が header、Rust、test、`docs/ffi.md` で同時に完成している。
- GUI はまだ一文書表示でもよいが、backend ownership は複数文書対応になっている。

### 完了記録

2026-08-01 に G3 を完了した。`CoreEngine` を複数 Core handle を所有する `CoreHost` へ置き換え、一つの長寿命 owner thread 上で `DocumentSessionId` と generation を key に create、command/input、snapshot、save、close、destroy を順序付ける bounded session registry を導入した。session ごとに Core-local view、stroke/sequence/pending state、document info、metric、ABI 呼び出し直後に copy した diagnostic を分離し、同じ Core-local ID/revision が異なる session で発生しても混線しない。close は新規 work を先に拒否し、受理済み work と live stroke を解決してから owner thread 上で handle を destroy する。UI 通知は session/generation/context を保持する pointer-free queue token とし、stale generation は再解決せず破棄する。

`DocumentRegistry` は複数 `DocumentSession` を保持でき、各 session は一つの非所有 `CoreHost` binding を持つ。application-global shortcut 設定は `ApplicationHost` を正本にし、既存 Core への一括同期と新規 Core initializer の両方を接続した。G3 の可視 UI は従来どおり一文書、一 Canvas であり、inactive session の snapshot はその Canvas へ publish しない。共有 renderer thread と session/view/canvas envelope は G4 とし、長時間処理が一つの CoreHost lane 上で直列になる制約は既知差分として残す。

C ABI は変更していない。二 Core の同値 local ID/revision、edit/dirty/savepoint/Undo/Redo/save-reopen の分離、invalid/cancel/save failure の隔離、close race、stale generation、owner-thread destroy、global initializer、pointer-free notification を native CoreHost test で検証した。2026-08-01 の Windows 11 x64 Debug strict build と CTest 17/17、Rust 177 tests と 1 doctest、format、clippy、whitespace check はすべて成功した。

---

## G4. 複数 Canvas を扱う RendererHost

### 目的

分割と複数 window の前提となる複数 Canvas を、一つの renderer thread と明示的な surface lifetime で描画する。

### 実装項目

- Canvas ごとの `RenderThread` を `RendererHost` と `CanvasSurface` へ分離する。
- D3D11/D2D device、factory、upload cache budget は host で共有し、swap chain、target、size、DPI、visibility は surface ごとに保持する。
- UI thread の Canvas `HWND` create/destroy と renderer thread の surface register/unregister を handshake する。
- `SnapshotEnvelope` に session、view、canvas、document/view revision、surface generation を持たせる。
- canvas bind が変わったときは旧 snapshot を release し、新 view の snapshot を要求する。
- hidden/minimized/occluded surface は Present と不要な snapshot build を停止する。
- queue は古い未描画 snapshot だけを置換し、stroke input や begin/end/cancel は対象にしない。
- device lost/reset は host が検出し、共有 device と全 surface の GPU resource を再構築する。
- 現在の per-Canvas renderer thread 実装は同じ milestone で除去する。

### テスト

- 二つの Canvas へ異なる session/view snapshot を送り、内容が混線しないこと。
- 同じ view を rebind した時の stale snapshot が描画されないこと。
- surface close、queue full、enqueue failure、device lost の全経路で snapshot が一度だけ release されること。
- 片方の resize、DPI change、occlusion が他方を壊さないこと。
- renderer API が一つの thread ID 上で実行されること。
- x64 と ARM64 で create/render/resize/device-lost native smoke を行う。

### 完了条件

- 一 process で複数 Canvas を同時描画できる。
- device resource は共有され、非表示 tab 数に比例して swap chain と renderer thread が増えない。
- document state は device lost から独立している。

### 完了記録

2026-08-01 に G4 を完了した。`ApplicationHost` 所有の一つの
`RendererHost` が Core より先に一つの renderer thread を開始し、共有
D3D11/DXGI/Direct2D device/factory、device generation、upload cache budget と
複数 `CanvasSurface` registry を owner thread 上で管理する。各 surface は
swap chain、D2D target/context、size、visibility/occlusion、retained snapshot、
surface generation だけを所有し、旧 Canvas ごとの `RenderThread` と device
所有は同じ milestone で除去した。

CoreHost からの snapshot は session、frontend view、Canvas、document/surface
generation、document/view revision を持つ `SnapshotEnvelope` で渡す。完全な
routing key と snapshot accessor の revision が一致する場合だけ bind 済み
surface が受理し、rebind、hidden/occluded、pending replacement、queue full、
surface close、RendererHost shutdown の全経路で Rust owner を一度だけ解放
する。device lost は全 surface の旧 GPU resource を先に破棄し、共有 device
を再生成して retained snapshot から全 surface を復元するため、Core document
state を破棄しない。C ABI は変更していない。

Canvas の stroke と view gesture も Canvas 所有の bounded queue へ payload を
copy し、workspace `HWND` へは token と surface generation の値だけを通知する。
document bounds と preview は pointer payload を持つ custom message ではなく
型付き Canvas API を使うため、`WPARAM`/`LPARAM` は C++ object を所有しない。

G4 native test は二つの Canvas と異なる session/view snapshot、同一 renderer
thread、rebind stale rejection、queue replacement/full failure、visibility、
resize/DPI isolation、surface close/shutdown、host-wide device lost recovery を
検証する。structural gate は ApplicationHost ownership、旧 RenderThread 不在、
共有 device/factory、envelope/revision routing、Canvas input の value-only
notification を固定する。Windows 11 の x64
Debug と ARM64 Debug は strict build と CTest 19/19 を完了した。可視 GUI は
意図どおり一 window、一 group、一 Canvas のままであり、次の対象は G5 である。

---

## G5. 一つの window 内の複数文書 tab

### 目的

`ファイル > 開く` が現在文書を置換する構造をやめ、一 window で複数文書を切り替えられるようにする。

### 実装項目

- `DocumentRegistry` に canonical file identity と untitled UUID の index を持たせる。
- Windows では可能な場合に volume/file ID を用い、取得できない場合は正規化した絶対 path を fallback とする。表示名を identity に使わない。
- tab item data は `DocumentViewId` への安全な token とし、Core view ID、配列 index、raw pointer を格納しない。
- open は新しい `DocumentSession` と初期 `DocumentView` を作り、active group に tab を追加する。
- duplicate open は既存 session を選択し、別 view の作成は明示 command に限定する。
- `ビューを閉じる`、`文書を閉じる`、`次/前の tab`、既存の `新しいビュー` を command registry へ追加または整理する。
- active tab 変更時に Canvas bind、pane target、menu state、status、title を一つの transaction として更新する。
- inactive tab は active processing indicator と dirty state だけを更新し、継続的な snapshot を要求しない。
- Save As 成功後に registry index、window title、recent files、recovery metadata を一括更新する。

### テスト

- 二文書の edit、Undo/Redo、selection、dirty、save/reopen が独立すること。
- 同じ file の相対 path、絶対 path、case 差、symlink/reparse 相当の duplicate open。
- Save As 先が既存 session と競合する場合に上書きや session 合流をしないこと。
- inactive tab の長時間 job 完了が active tab の pane/menu を誤更新しないこと。
- view close、last-view close、window close、application exit の dirty prompt 数。
- keyboard だけで tab の選択、並べ替え前の移動 command、close が可能であること。

### 完了条件

- 一 window で複数文書を開き、切り替え、独立編集、保存、close できる。
- 同じ file を通常操作で二つの独立 session として開けない。
- 一文書しか開かない従来 workflow に余分な modal 操作が増えない。

### 完了記録

2026-08-01 に G5 を完了した。`DocumentRegistry` は file identity と untitled
UUID の bounded index を持ち、Windows の既存 file は利用可能なら volume/file
ID、取得できない場合は正規化した絶対 path で識別する。New/Open/Import/
Recovery は一つの `DocumentSession` と初期 `DocumentView` を作って active group
へ追加し、duplicate open は既存 view を選択する。tab item data は frontend の
`DocumentViewId` 値だけを保持し、Core-local view ID、配列 index、raw pointer は
保持しない。

active tab の変更は live stroke/preview を cancel した後、session、Canvas route、
Core view、pane/status/menu/title/autosave presentation を同じ UI-thread transaction
で切り替える。Canvas は可視 editor group に一つのままで、inactive tab の Core
state notification は tab の dirty/processing 表示だけを更新し、snapshot や
active pane/menu target を切り替えない。発行済み非同期 work は capture 済みの
session/generation を検証し、後から active document を再解決しない。

`ビューを閉じる`、`文書を閉じる`、`次/前の tab` と既存の `新しいビュー` を
共通 command/state/shortcut catalog へ接続した。最後の view close は document
close へ昇格し、window close は dirty session ごとに一度だけ確認する。Save As
は別の open session と identity が競合する場合は書き込みも session 合流も行わず、
成功時だけ identity index、title、recent files、recovery metadata を更新する。

native model/application smoke は二文書の edit、Undo/Redo、selection、dirty、
save/reopen、file-ID/path/case/hard-link duplicate、Save As conflict、inactive-session
completion、tab keyboard navigation、new/last-view/document close、dirty prompt 数、
recent-file cleanup を検証する。C ABI は変更していない。G5 の可視構成は一
window、一 editor group、一 Canvas、複数 document tab であり、分割表示は G6 の
対象である。2026-08-01 の Rust format、zero-warning clippy、workspace test は
177 tests と 1 doctest が成功した。Windows 11 x64 Debug/Release は MSVC 19.51
で fresh configure、strict build、unsigned MSIX assembly、CTest 19/19 を両構成
とも完了した。

---

## G6. 分割 EditorGroup

### 目的

同一文書または別文書を、同じ window 内で二画面同時に表示する。

### 実装項目

- `EditorArea`、`EditorGroup`、splitter model を導入する。
- 初期版は一 group または二 group のみとし、orientation と ratio を保持する。
- group ごとに tab control、active `DocumentViewId`、Canvas slot、focus history を持つ。
- 同一文書を別 group に複製表示する操作は、新しい Core view logical state を作る。文書データは複製しない。
- 別 group へ「移動」は既存 `DocumentView` を移し、「新しいビュー」は別 `DocumentView` を作る。
- active group は keyboard focus と最後の explicit activation から決め、mouse hover だけでは変更しない。
- pane target と menu command state は active group/view を参照する。
- splitter drag 中の resize を coalesce し、final position で高品質 redraw する。
- group close 時は view を他 group へ移すか閉じるかを規則化し、dirty session を誤って閉じない。

### テスト

- 同じ文書の二 view で zoom/pan/flip が独立し、編集と Undo/Redo が同期すること。
- 別文書の二 group で input、snapshot、pane、save target が混線しないこと。
- split right/down、ratio clamp、orientation change、group close、window resize、DPI change。
- focus を menu、pane、Canvas、tab strip 間で移しても command target が正しいこと。
- active stroke 中の tab/group 切り替えを安全に完了または拒否できること。

### 完了条件

- 左右と上下の二分割を menu/keyboard だけで作成、操作、解消できる。
- 同一文書の複数 view が history と dirty を共有し、表示状態を分離する。
- 一 group mode の描画性能と操作性に目立つ回帰がない。

### 完了記録

2026-08-01 に G6 を完了した。UI/Input thread 所有の bounded `EditorArea` は
一つまたは二つの `EditorGroup`、左右/上下 orientation、200〜800 milli に clamp
した split ratio を保持する。各 group は frontend の strong group/Canvas ID、tab
control、active `DocumentViewId`、focus history、可視 Canvas slot を所有し、非表示
tab ごとの Canvas は作らない。splitter は drag 中の layout を約 16 ms 単位に
coalesce し、release 時に最終 layout と Canvas redraw を確定する。

Window menu と共通 command/state/shortcut catalog に、右分割、下分割、別 group
へ移動、別 group に新しい view、次 group、group close を接続した。move は既存
`DocumentView` を移し、新しい view は同じ `DocumentSession` の Core-local logical
view を作る。group close は snapshot sink を解除してから Canvas を破棄し、全 view
を残る group へ移すため dirty document を閉じない。Canvas/tab focus または明示
command だけが active group を変更し、mouse hover は target を変更しない。

一つの `DocumentSession` は引き続き一つの `InkpodCore` handle を共有する。
`CoreHost` は frontend view ID を primary または secondary Core view ID へ写像し、
session/generation が一致する各可視 Canvas sink に view ごとの immutable snapshot
を fan-out する。同一文書の view は document/history/dirty/savepoint を共有し、
zoom/pan/flip と presentation だけを分離する。別文書は session namespace で分離
する。Core create/operation/destroy、GPU resource/Present、HWND/Common Controls の
thread 所有境界と C ABI v2 は変更していない。

pure/native tests は EditorArea の invalid/no-op/split/clamp/move/merge、二 group の
capture 済み `CommandContext` と stale rejection、primary/secondary view snapshot
fan-out、Canvas unbind、同一文書の独立 flip と共有 selection/Undo/Redo、別文書
分離、Canvas/tab/pane focus、active stroke cancel、split right/down、resize/DPI、
move/new-view/group close、および一 group の既存 workflow を検証する。

---

## G7. 制約付き DockHost と primary pane の移行

### 目的

現在の固定配置を、予測可能で復元可能な dock model へ置き換える。

### 実装項目

- HWND を操作しない pure な `DockLayoutModel` と、適用を担当する `DockHost` を分ける。
- `PaneDescriptor` に次を持たせる。
  - stable pane type ID、resource title、default zone。
  - allowed zones、scope、multiplicity。
  - float/autohide 可否、minimum/preferred size。
- layout model は `TopContext`、`Left`、`Right`、`Bottom` の `DockStack`、中央 `EditorArea`、floating placement を表現する。
- 各 stack は pane tab と一方向の比率分割だけを許す。
- drag 中は allowed zone だけを docking preview で示す。menu/context menu にも dock、float、hide、reset の代替操作を設ける。
- 最初の縦切りで tool、tool options、color、layer の primary pane を移行し、既定配置を現在の fixed layout と等価にする。
- narrow window では優先度の低い pane を temporary に tab 化または自動格納するが、保存済み layout を上書きしない。
- layout geometry は reference DPI と device pixel の由来を明示し、DPI scale を二重適用しない。

### テスト

- pure model の add/remove/move/tab/float/hide/reset と invalid operation。
- allowed zone 違反、重複不可 pane、minimum size、ratio rounding。
- 96/120/144/192 DPI、narrow window、monitor 境界、RTL を採用する場合の geometry。
- primary pane を全て非表示にしても menu から復元できること。
- high contrast と keyboard navigation で docking target と active pane が識別できること。

### 完了条件

- primary pane の全機能が dock、float、hide、restore 後も動作する。
- 完全自由 dock tree を作らず、layout model の状態数が bounded である。
- 固定 layout 専用の geometry code が除去される。

### 完了記録

2026-08-02 に G7 を完了した。HWND 非依存で固定長の `DockLayoutModel` は、
resource title、stable pane type ID、default/allowed zone、target scope、multiplicity、
float/autohide 可否、minimum/preferred size を持つ四つの `PaneDescriptor` を管理する。
表現可能な配置は `TopContext`、`Left`、`Right`、`Bottom` の四 stack、中央
`EditorArea`、floating、hidden に限定し、stack 内も tab または一方向の比率分割だけ
である。旧固定 geometry は削除し、既定値は従来の 40-DIP options、80-DIP tool、
320-DIP color/layer 構成を保つ。狭い window の一時格納は geometry の計算結果だけに
反映し、保存対象の model を変更しない。96-DPI reference value と device pixel の
変換は `DockHost` の HWND 境界で一度だけ行う。

UI/Input thread 上で `WorkspaceWindow` が所有する `DockHost` は、既存の tool、tool
options、color、layer の child HWND を再利用する。allowed zone だけを示す drag
preview、標準 tab control、mouse/keyboard splitter、Shift+F10 対応 context menu の
dock/float/hide/reset、Window menu からの hide/restore を接続した。floating frame は
main window 所有の通常 top-level window とし、閉じる操作は pane state を破棄せず
hide に変換する。操作対象の pane は入力時に確定し、document command routing、
CoreHost、RendererHost、C ABI v2 の所有権/thread 境界は変更していない。

workspace persistence は bounded な version 3 record へ更新し、旧 version 2 の固定
layout record を同じ既定配置へ移行する。pure tests は add/remove/move/tab/float/
hide/restore/reset、重複、allowed zone、minimum、ratio、破損 record、96/120/144/192
DPI、mirror、narrow 非破壊 adaptation を検証する。native smoke は全 primary pane の
既存機能に加え、float の owner/style/reparent、hide/restore、dock、tab/split/reset、
menu 復元、keyboard/high-contrast-compatible standard controls を検証する。

---

## G8. pane target と auxiliary pane の完成

### 目的

情報量を増やしつつ、pane が別文書を誤操作しない仕組みを完成させる。

### 実装項目

- `FollowActiveView`、`PinnedDocument`、`Application`、`Job` の target policy を実装する。
- pane header の target 表示、pin/unpin、失効通知、keyboard focus を共通化する。
- pane action は click 時点の `CommandContext` を capture し、完了通知にも session/view/job ID を付ける。
- 不足している pane/操作は、空 shell を先行させず、次の順に一つずつ UI から Core まで縦切りで完成させる。
  1. locator/navigation。
  2. sequence/file preview と thumbnail list。
  3. light table の登録、表示、opacity、offset、frame navigation。
  4. subpalette/reference viewer。
  5. palette register、clear、load、save と target affordance。
  6. batch pane の job target、progress、cancel、result。
- reference viewer は read-only の専用 Canvas binding とし、編集 input を Core へ送らない。
- light table、preview、layer の表示更新は document revision と view revision を区別する。
- multi-target layer 操作を提供する場合は、対象 session/layer ID の明示選択と transaction を必須にする。

### テスト

- active view 追従、document pin、pinned document close、別 window focus、job close。
- async completion が同名の別文書や新しい active tab を更新しないこと。
- 各 pane の success、no-op、invalid、cancel、Undo/Redo、必要な save/reopen。
- locator と reference viewer の座標変換、DPI、view flip。
- light table が document 全複製を保持せず、snapshot/tile cache budget に従うこと。

### 完了条件

- `docs/compatibility.md` で対象 requirement が test 付きの状態になっている。
- 表示だけの placeholder pane と未接続 button がない。
- どの pane action も対象 document/view/job を UI 上と code 上の両方で説明できる。

### 進捗記録

2026-08-02 に G8 の共通 target policy と最初の locator/navigation 縦切りを
実装した。UI-thread 所有の bounded `PaneTargetRegistry` は `Application`、
`FollowActiveView`、`PinnedDocument`、`Job` を strong ID と値の
`CommandContext` だけで保持する。pane action は入力時点で target を capture し、
固定先 close は別文書へ silent に転送せず追従へ戻り、header と MSAA alert で通知する。
job close 後の action は target 不在として拒否する。

modeless locator は対象文書名、追従/固定、X/Y、selection H/V/L、RGBA8/16、
9 x 9 neighborhood、固定編集、自動 scroll を表示する。neighborhood は caller-owned
bounded buffer を一度の C ABI call で取得し、結果 queue と window message には
session/view/generation と値 token だけを渡す。固定編集は click 時に capture した
session/view へ document 座標の一 pixel stroke を一 Undo 単位で発行する。
Window menu、configurable shortcut、Tab navigation、placement persistence を接続し、
表示だけの shell や未接続 control は残していない。

同日に二番目の sequence/file preview 縦切りも実装した。対象文書名と追従/固定を
表示する modeless owner-drawn list は、Core owner thread から caller-owned bounded
buffer へ取得した straight RGBA8 thumbnail を自然順で表示し、行選択、前/次、番号移動、
import を同じ exact session/generation へ発行する。dirty 時の Cancel、endpoint no-op、
invalid index、Undo 後の再選択で別文書へ retarget しない。番号付き raster を開くと、
最後の数字列の前後が一致する同一 folder の sibling だけを mixed-format で一括 decode し、
開いた cell を選択する。decode failure では旧 sequence を保持する。

三番目の light table 縦切りでは、modeless palette に対象文書と追従/固定を
表示し、set/item の登録・選択・複製・削除・並べ替え、全体 opacity、item の
表示・色・モード・opacity・offset、reload、Canvas 移動、編集画像との交換、
前/次 cell を既存 Core/C ABI command へ接続した。pane action は選択 ID を
session/generation namespace と組で保持し、Canvas 移動も開始時の
`CommandContext` から別文書へ retarget しない。同じ文書の複数 view は
一つの Core 所有 light-table/history を共有し、palette は画像複製を保持せず
set/item metadata だけを表示する。

最後に subpalette/reference viewer、palette target affordance、Batch job target を
順番どおり完成させた。reference viewer は専用 Canvas binding と Core-local view を持ち、
zoom/pan/flip、前後・現在セル、自動前セル、scroll 同期、色採取を提供するが、stroke は
必ず consume して編集 input を送らない。immutable reference snapshot は source tile を
共有境界で変換し、文書 revision、dirty、history、savepoint を変更しない。

Color pane は follow/pin 対象を header に表示し、登録・削除・clear・load・save の
button を既存 production command へ接続した。palette mutation は click 時の
session/generation だけへ発行する。Batch pane は follow/pin、実行中の Job target、進捗、
cancel、result を表示する。preview と smoke/async execution はどちらも確定済み
`CommandContext` を使い、active tab が変わっても結果を別文書へ適用しない。target close、
stale generation、queue failure、job close は結果を誤配せず、job 終了後は元の follow/pin
policy へ戻す。

2026-08-02 に G8 を完了した。全 auxiliary pane action は UI header と code の両方で
document/view/job target を説明でき、表示だけの placeholder pane と未接続 button はない。
次に着手する milestone は G9 である。

---

## G9. layout persistence、named workspace、responsive behavior

### 目的

用途別配置を再現可能にし、DPI、monitor、window 寸法の変化から安全に復元する。

### 実装項目

- versioned、bounded な `WorkspaceLayoutState` を定義する。
- 保存対象を window placement、split orientation/ratio、dock stacks、pane visibility/size、floating placement、selected preset に限定する。
- 開いている文書 path は layout record に混ぜない。
- layout record は既存方針に従い HKCU に保存し、size、count、string length、unknown ID を検証する。
- corrupt/unsupported record は拒否して既定 layout へ戻し、application 起動を失敗させない。
- 旧 fixed layout record があれば新形式へ一度だけ migration する。
- 初期 preset を用意する。
  - `彩色`: tool/color/layer を中心とする現在相当。
  - `線整理`: layer/selection/tool options を優先。
  - `参照・チェック`: reference/light table/locator/preview を優先。
  - `バッチ`: job、preview、result を優先。
  - `集中`: pane を最小化し Canvas を優先。
- `ワークスペースを保存`、`名前を付けて保存`、`復元`、`既定に戻す` を提供する。
- secondary pane の AutoHide edge strip を keyboard、pointer、screen reader から操作可能にする。
- missing monitor、DPI change、taskbar work area change では window と floating pane を可視領域へ clamp する。
- temporary responsive adaptation は保存済み logical layout を変更しない。

### テスト

- serialization round-trip、version migration、truncated/corrupt/oversized record。
- unknown pane ID は無視し、必須 pane 不足は default を補うこと。
- monitor の追加、削除、primary 変更、DPI 変更後の復元。
- 日本語/英語、高 contrast、標準/compact density での最小寸法。
- preset 切替後も document/view、active stroke、Undo/Redo が変化しないこと。

### 完了条件

- application restart 後に layout が安全に復元される。
- 不正 record と存在しない monitor で window を見失わない。
- 現在相当の配置へ一操作で戻せる。

### 完了記録

2026-08-02 に G9 を完了した。`WorkspaceLayoutState` は 8 KiB 以下の bounded
version 4 record とし、window placement、editor split orientation/ratio、primary
dock stack、pane visibility/size、secondary pane の floating placement/AutoHide、
selected preset と任意名だけを HKCU に保存する。文書 path、文書 ID、Core 所有状態は
record に含めない。version 2 fixed record と version 3 dock record は検証後に現在形式へ
一度だけ移行し、truncated、oversized、unknown enum、重複 ID、不正文字列は既定配置へ
安全に戻す。unknown pane ID は無視し、不足する既知 pane は現在の default で補う。

`彩色`、`線整理`、`参照・チェック`、`バッチ`、`集中` の五 preset と、保存、名前を
付けて保存、復元、既定に戻す command を Window menu と configurable shortcut へ接続した。
secondary pane は resource 名を持つ標準 `BUTTON` の edge strip から pointer、Tab/Space、
screen reader で開け、main workspace へ戻ると格納される。標準/compact density と狭幅時の
一時 geometry は保存 model を変更しない。monitor 削除、primary 変更、DPI/work-area 変更時は
main/floating placement を device-pixel capture DPI から一度だけ換算し、可視 work area へ
clamp する。

preset 発行時に Canvas が stroke を capture 中なら、文書/Core command には触れず UI 配置の
適用だけを保留する。Canvas は `CanvasId` と surface generation の値だけで終了を通知し、同じ
surface と検証できた場合に配置を適用するため、active stroke、Undo/Redo、document/view revision
は変わらない。pure serialization/migration/malformed/monitor/DPI/density tests、構造 gate、実 HWND
AutoHide/accessibility と active-stroke smoke、x64 Debug/Release の全 CTest を完了した。次に
着手する milestone は G10 である。

---

## G10. 複数トップレベル WorkspaceWindow

### 目的

同一 process 内で複数の作業 window を開き、同じ文書または別文書を monitor 間で扱えるようにする。

### 実装項目

- `ApplicationHost` に `WorkspaceWindowRegistry` と last-focused window を持たせる。
- `WorkspaceWindow` ごとに top-level `HWND`、menu、DockHost、EditorArea、status、focus history、layout preset を所有させる。
- window procedure は `WorkspaceWindow` instance を安全に取得し、process-global singleton state に依存しない。
- message loop の modeless dialog、accelerator、floating pane 処理を固定配列から registry 列挙へ変更する。
- `新しいウィンドウ`、`ビューを新しいウィンドウへ移動`、`新しいウィンドウに複製ビュー` を command-first で実装する。
- document/session registry、CoreHost、RendererHost、clipboard、shortcut、job registry は application で共有する。
- menu enable/checked、title、status、pane target は各 window の focused context から更新する。
- window close と application exit の session 参照数、dirty prompt、job cancellation を分離する。
- per-window placement と preset selection を保存する。floating pane は所属 workspace window を owner とする。

### テスト

- 二つの top-level `HWND` の作成、focus、menu、DPI、close、再作成。
- 同一文書の view を二 window に置き、編集、Undo/Redo、dirty が同期し、zoom/pan が独立すること。
- 別文書を別 window に置き、input、save、pane、snapshot が分離されること。
- 一方の window close で他方の session、Canvas、job が破棄されないこと。
- 最後の window だけが application shutdown を開始すること。
- renderer/core の shutdown 順序と pending notification の stale rejection。
- x64 と ARM64 の native multi-window smoke。

### 完了条件

- 二つ以上の workspace window を同時利用できる。
- 同一文書を別 monitor の二 window で安全に表示できる。
- 一 window 利用時の操作と性能に目立つ回帰がない。

### 完了記録

2026-08-02 に bounded な `WorkspaceWindowRegistry`、window ごとの `HWND`、menu、
`DockHost`、`EditorArea`、status、focus、layout 所有、registry 列挙型 message loop、
新規 window と view の window 間 move/duplicate command、window 単位の close と最終
window shutdown を実装した。同一 session の複数 window view は一つの Core binding、
文書、履歴、dirty、savepoint を共有し、view state と可視 group ごとの Canvas だけを
分離する。command は発行時の `CommandContext` を保持し、window procedure と input は
対象 `HWND` の `WorkspaceWindowId` へ routing する。x64 Debug/Release の build と 21/21
CTest は通過し、multi-window smoke は二つの実 `HWND`、focus/menu/DPI、同一文書の
編集/Undo/独立 flip、別文書の edit/save/reopen/pane 分離、dirty close の Cancel/Discard、
一方の close 後の継続、最後の window だけの `WM_QUIT` を確認した。

ARM64 toolset 導入後、`Hostx64\arm64\cl.exe` と Rust
`aarch64-pc-windows-msvc` target による Debug/Release の fresh configure、strict
cross-build、ARM64 MSIX assembly は完了した。両構成の CTest は構造検証と package smoke
12/21 に成功したが、AMD64 実行ホストでは ARM64 executable を起動できず、native unit、
ABI、renderer、multi-window smoke の9件が `BAD_COMMAND` で未実行となった。2026-08-02
にユーザー判断で、x64 Debug/Release の全 native CTest と ARM64 Debug/Release の strict
cross-build、構造検証、package smoke を G10 の検証 gate として受け入れ、G10 を完了とした。
ARM64 native tests を実行済みとは扱わない。次に着手する milestone は G11 である。

---

## G11. tab drag、window 間 drop、tear-out

### 目的

G6 と G10 で完成した command を pointer drag からも利用できるようにする。

### 実装項目

- Common Controls tab と協調する `TabDragCoordinator` を導入する。
- `DragToken` に source window/group、view、operation、generation を持たせる。
- reorder、group 間 move、window 間 move、copy/new-view modifier、window 外 drop を support する。
- window 外 drop は新しい `WorkspaceWindow` を作成して view を移動する。
- drop commit 前に target の存在、allowed operation、last-view/dirty rule、active stroke、modal state を再検証する。
- drag visual は Windows theme、DPI、high contrast に追従する。
- `Esc`、capture loss、source/target close、DPI/monitor change で元配置へ rollback する。
- pointer drag が困難な利用者のため、同じ操作を menu、context menu、keyboard から実行可能なまま維持する。

### テスト

- 同一 group reorder、別 group/window move、new window、cancel、capture loss。
- source/target window close と drop の race。
- active stroke、modal preview、save/job 中の許可/拒否規則。
- drag 中に view ID が stale になっても別 tab を移動しないこと。
- 100% から 200% DPI の monitor 間 drag。

### 完了条件

- command と drag の結果が同じ model operation を通る。
- cancel と失敗で view、tab order、dirty、history が完全復元される。
- drag 実装に文書所有権や Core handle の移動が含まれない。

### 完了記録

2026-08-02 に、`ApplicationHost` が UI thread 上で所有する値型だけの
`TabDragCoordinator` と Common Controls tab subclass を実装した。`DragToken` は発行時の
workspace、editor group、document session/view、generation と move/copy operation を保持し、
button release までは配置を変更しない。同一 group reorder、group/window 間 move、Ctrl による
新規 view、window 外 drop の tear-out は、menu/context menu/keyboard と同じ
`ApplicationHost::MoveDocumentView`、`CreateDocumentViewInGroup`、
`MoveOrDuplicateViewToNewWorkspace` を通る。

commit 直前に token、source index、target workspace/group、capacity、active Canvas stroke、
floating/modal preview、effect task、capture を再検証する。同期 save は UI thread 上で drag と
同時進行せず、session/generation で隔離された Batch job 中の view 移動は許可する。失敗時は
source/target `EditorArea`、routing、Canvas binding、発行前 active context を復元する。
`Esc`、capture loss、source/target close、DPI/monitor/theme change は commit 前に cancel する。
drag image は現在の native tab item を capture するため Windows theme、DPI、high contrast の
描画を継承する。document/session/Core handle、revision、dirty、history は移動しない。

value-token/model unit、command-state/catalog/構造 gate と native smoke で、reorder、別 group/window、
tear-out、Ctrl-copy、menu parity、cancel/capture loss、200% DPI、target close race、stale source、
active stroke、modal/effect 拒否、Batch job 許可、revision/checksum/dirty/history 不変を検証した。
x64 Debug/Release の strict build と全 22/22 CTest を完了した。次に着手する milestone は
G12 である。

---

## G12. 単一論理 instance、application activation、recovery

### 目的

OS から複数 file を開く操作と複数 workspace window を整合させ、異常終了時に全 session を回復可能にする。

### 実装項目

- 同一 user/session 内の primary process 検出に named mutex を使用する。
- secondary process は command line を既存 parser で完全検証した後、current-user に制限した named pipe へ versioned、length-prefixed activation request を送る。
- request は open path、open mode、target preference、request ID を持ち、path/string/count に上限を設ける。
- primary process は IPC thread で受信した値を copy し、UI queue へ渡す。`HWND` や Rust pointer を IPC payload に含めない。
- default は last-focused workspace の active group に open する。明示 flag または `新しいウィンドウで開く` の要求だけが新規 window を作る。
- primary が応答しない場合は bounded timeout と診断を返し、同じ native file を独立 process で無断編集しない。
- autosave/recovery metadata を `DocumentSessionId` と original file identity ごとに管理する。
- 起動時 recovery は「最新一件」ではなく全候補を列挙し、元 file、autosave 時刻、session、状態を比較して選択できるようにする。
- 通常 layout 復元と document session 復元を分離する。前回開いていた path の自動復元は privacy を考慮し、明示設定を既定 off で提供する。crash recovery は別契約として維持する。

### テスト

- Unicode、space、長い path、複数 path、duplicate request、同時起動。
- IPC version 不一致、truncated/oversized message、偽 request、primary timeout。
- 同じ file が既に開いている場合の activation と focus。
- 複数 dirty session の autosave、crash、全候補列挙、個別復元/破棄。
- recovery 成功だけでは通常 savepoint を進めないこと。
- secondary process が Core、window、renderer を作成せず終了できること。

### 完了条件

- Explorer/file association から複数 file を開いても、一つの論理 application 内の tab/window として扱われる。
- 二重 process による同じ file の無警告上書きを防ぐ。
- 複数文書の recovery を一件も silent に捨てない。

### 完了記録

2026-08-02 に G12 を完了した。同一 user/session の SID 付き named mutex と
current-user/SYSTEM だけを許可する local named pipe を導入し、secondary は既存 parser で
Unicode/space を含む最大 64 path と --new-window を検証してから、version 1、長さ付き、
最大 1 MiB の request を送る。primary の IPC thread は request ID の重複、queue 上限、
shutdown、malformed message を検査し、値 token だけを UI thread message queue へ渡す。
secondary は Common Controls、CoreHost、RendererHost、workspace HWND を作る前に終了し、
5 秒以内に primary が応答しなければ別 process で編集を開始しない。UI thread は受信時に
last-focused workspace/active group または明示された新規 workspace を確定し、複数 path を
既存の open/identity route へ順に渡す。同じ file は既存 session/view を activate する。

autosave ごとに versioned/bounded metadata sidecar へ DocumentSessionId、generation、
document UUID、original file identity/path、source、時刻を記録する。起動時は最大 4096 件の
全 recovery を新しい順に列挙し、metadata が壊れていても候補本体を保持したまま、一件ずつ
復元、破棄、保留を選べる。通常 save は対応する recovery と sidecar を除去するが、
autosave/recovery は通常 savepoint を進めない。前回開いていた通常 path の復元は workspace
layout と crash recovery から分離し、「ファイル > 起動時に前回の文書を復元」の明示設定を
既定 off で追加した。C ABI と Rust Core の所有権契約は変更していない。

protocol/transport unit は Unicode、space、long/multiple path、duplicate、4 concurrent
clients、version/truncation/oversize/偽 request、timeout、UI queue failure を確認した。
recovery unit と application smoke は metadata/path record の current-version
round-trip/malformed rejection、全候補列挙、個別破棄、二つの dirty session の autosave と
列挙、crash 相当の close/recovery、savepoint/Undo/Redo/save/reopen、同一 file focus と明示
new-window routing を確認した。Rust 178 tests と 1 doctest、x64 Debug/Release の strict build、
unsigned MSIX assembly、全 25/25 CTest を完了した。次に着手する milestone は G13 である。

---

## G13. 性能、resource budget、アクセシビリティ、release gate

### 目的

複数化による resource 増大と複雑な focus/routing を計測し、production quality の失敗耐性を完成させる。

### 実装項目

- 計測 scenario を固定する。
  - 一 window、一文書、一 view。
  - 一 window、四文書、二 visible group。
  - 二 window、四文書、四 view。
  - 大画像、light table、reference、Undo history、device lost。
- document tile/history、immutable snapshot、CPU staging、GPU texture、thumbnail、reference/light-table cache の使用量を category 別に観測可能にする。
- inactive tab の snapshot build を停止し、GPU/thumbnail cache に LRU と application-wide budget を設ける。
- 一 window の visible Canvas を最大二つに制限し、非表示 tab に swap chain を持たせない。
- CoreHost の長時間処理直列化による入力遅延を計測する。許容できない処理だけを次の形へ分離する。
  1. Core owner thread で immutable input と base revision を確定。
  2. worker で OS 非依存かつ deterministic な計算。
  3. Core owner thread の queue へ結果を戻す。
  4. revision/cancel/generation を検査し、成功時だけ transaction commit。
- 計測なしに Core thread affinity を緩和したり、一文書一 thread に変更したりしない。
- keyboard navigation を完成させる。
  - `Ctrl+Tab` / `Ctrl+Shift+Tab`: tab。
  - `Ctrl+F6` / `Ctrl+Shift+F6`: editor group または view。
  - `F6` / `Shift+F6`: menu、dock pane、editor area、status の focus cycle。
  - `Ctrl+F4`: view close。
- tab、splitter、pane header、auto-hide、target、dirty、job progress を UI Automation から取得できるようにする。
- high contrast、200% DPI、keyboard-only、screen reader、IME、日本語/英語 resource を確認する。
- queue saturation、close 中 input、active stroke、stale snapshot、device lost、save failure、OOM 相当の allocation failure、shutdown race を fault injection する。
- security/privacy review と third-party notice review を行う。

### テストと release gate

- Rust: format、clippy、workspace/all-features tests、必要な property/golden/fuzz regression。
- Windows: MSVC `/W4 /permissive-`、x64 Debug/Release、ARM64 Debug/Release の configure/build/test。
- native smoke: multi-window、multi-document、split、dock、DPI、device lost、recovery、accessibility。
- soak: tab/window の反復作成破棄、layout 切替、連続 open/close、renderer reset。
- performance: baseline との差分を記録し、閾値は初回計測後に文書化する。根拠のない数値目標を先に固定しない。

### 完了条件

- resource 使用量が document、view、Canvas、pane ごとに説明可能である。
- 長時間処理中も別文書の input を失わず、cancel または stale な結果が部分 commit を残さない。
- accessibility、DPI、device lost、shutdown の release scenario が自動または再現可能な手順で検証されている。
- 対象 requirement が `docs/compatibility.md` で `Verified` または根拠付きの既知差分になっている。

### 実装状況

2026-08-03 に G13 の resource/latency hardening を開始した。Core は document tile/history、
render cache、CPU staging、light table/reference、sequence source、thumbnail cache の logical
payload を read-only C ABI で返す。CoreHost は session/generation ごとの queue 受理・拒否、
pending high-water mark、queue wait を owner-thread dispatch 時に計測する。別 session の長時間
work 中に受理済み input が失われず順序どおり完了することを native test で固定したが、worker
分離の必要性を決める scenario baseline は未完了である。

RendererHost は retained/pending snapshot、GPU tile、swap chain、surface、queue replacement/
rejection、stale、resource-limit、device-reset を value copy で観測できる。active tile の合計を
512 MiB の application-wide budget へ事前 admission し、inactive tile は全 CanvasSurface を
横断した LRU で再利用・破棄する。非表示/occluded/閉じた surface の snapshot は従来どおり
受理しない。aggregate に加えて document/view/Canvas/generation route を保持する per-surface
value copy を取得できる。UI thread では layer/sequence thumbnail と color-picker CPU cache を
pane instance/workspace 別に計測する。`Ctrl+Tab` / `Ctrl+Shift+Tab`、`Ctrl+F6` /
`Ctrl+Shift+F6`、`F6` / `Shift+F6`、`Ctrl+F4` は text edit の誤作動を避けつつ
workspace navigation として処理し、
native smoke で forward/reverse の tab/group/focus 巡回と view close を確認する。Common Controls
の MSAA/UI Automation bridge から tab の dirty label、captionless splitter、pane、AutoHide、
target、job status の accessible name を取得する回帰も追加した。

fault matrix は renderer queue saturation、close 中の Core input、active stroke、stale snapshot、
device lost、置換不能な save、owner graph の allocation failure、pending queue を残した shutdown を
native test で再現する。save failure は dirty、revision、path、recent list を変更しない。security/
privacy と third-party notice review では新規依存がなく、telemetry が集計量と強い ID/generation
だけを保持し、path、文書名、画像内容を保持・記録しないことを確認した。

Windows x64 の quick benchmark は warmed 5-run median と回帰 review 閾値を
`docs/core-benchmark-baseline.md` に固定した。owner/CoreHost/RendererHost/production GUI lifecycle
を各 5 回反復する bounded soak を完走した。x64 Debug/Release の strict build、unsigned MSIX、
25/25 CTest と ARM64 Debug/Release の strict cross-build、MSIX、host-independent 14/25 CTest は
成功した。thumbnail の application-wide cache/budget/LRU、四つの固定 multi-window resource
scenario、high contrast/200% DPI/screen reader/IME/日本語・英語 resource の release checklist、
ARM64 上でしか実行できない 11 native test は未完了のため、G13 は `In progress` である。

---

## 10. マイルストーン共通の検証

変更範囲に応じて、少なくとも次を実行する。preset は repository に存在する正確な名称を使う。

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features

cmake --preset windows-x64-debug
cmake --build --preset windows-x64-debug
ctest --preset windows-x64-debug
```

追加 gate は次の通りとする。

| gate | 必須検証 |
|---|---|
| G0-G3 | Rust 全検証、Windows x64 Debug build/test、対象 unit/structural test |
| G4 | x64/ARM64 Debug、renderer native smoke、device lost |
| G5-G7 | x64 Debug/Release、document/split/dock native smoke |
| G8-G9 | pane ごとの Core-to-UI 縦切り、persistence malformed test、DPI/accessibility |
| G10 | x64/ARM64 Debug/Release、multi-window shutdown smoke |
| G11-G12 | drag race、IPC malformed/security、recovery integration |
| G13 | 全 Rust/Windows matrix、soak、performance、accessibility release checklist |

各 milestone で次も確認する。

- `git diff --check` が成功する。
- C header と Rust 宣言の drift test が成功する。
- 新規 warning がない。
- ownership、thread、failure/cancel path の test がある。
- user-visible behavior、現在状態、既知差分が変わった場合だけ `docs/implementation-status.md` を更新する。
- requirement 状態、test、既知差分が変わった場合は `docs/compatibility.md` を更新する。

---

## 11. 代表的な受け入れシナリオ

最終的に少なくとも次を一連の操作として検証する。

### A. 複数文書

1. A と B を同じ window の tab で開く。
2. A を編集し、B を編集する。
3. A だけ Undo し、B の内容と history が変わらないことを確認する。
4. B を保存し、A だけ dirty のままであることを確認する。
5. application exit で A について一度だけ確認される。

### B. 同一文書の分割

1. A を右へ分割し、同一文書の新しい view を作る。
2. 左を全体表示、右を拡大表示にする。
3. 右で編集し、左へ同じ document revision が表示される。
4. Undo は両方へ反映されるが、zoom/pan は変わらない。

### C. 別 window

1. A の view を新しい window に複製する。
2. 二つの monitor で異なる DPI に配置する。
3. 一方で編集、他方で locator/light table を確認する。
4. 一方の window を閉じても A は閉じず、他方の view が継続する。

### D. dock と workspace

1. layer、color、light table、reference を許可された zone に配置する。
2. reference を document A に pin し、active tab を B に切り替える。
3. reference は A のまま、layer は B に追従する。
4. workspace を保存して再起動し、monitor 構成を変えても全 pane が可視範囲へ復元される。

### E. 異常系

1. A で長時間処理、B で stroke、別 window で resize を同時に行う。
2. B の input begin/end が失われず、UI thread が Core/Present を待たない。
3. device lost を発生させ、GPU resource だけが再構築される。
4. A の job を cancel し、partial commit がない。
5. 異常終了後、A と B の recovery 候補が両方列挙される。

---

## 12. 主要 risk と抑制策

| risk | 影響 | 抑制策 |
|---|---|---|
| snapshot が別 Canvas に描画される | 誤表示、情報混線 | session/view/canvas/generation を持つ envelope、stale rejection |
| command が focus 移動後の別文書へ届く | 破壊的な誤編集 | 発行時に immutable `CommandContext` を capture |
| 同じ file を二 session で保存する | lost update | canonical file identity registry、Save As の atomic conflict check |
| CoreHost の長時間処理が全 document を待たせる | 入力遅延 | まず直列契約を維持して計測し、pure worker + revision commit を限定導入 |
| Canvas 数に比例して thread/GPU memory が増える | resource 枯渇 | shared RendererHost、可視 group だけ Canvas、application-wide LRU budget |
| dock layout record が壊れる | 起動不能、pane 消失 | version、size/count 上限、fallback、monitor clamp、reset command |
| tab drag 中の close/capture loss | UAF、view 消失 | ID/generation token、transactional move、command-first、rollback |
| AppContext 分解で既存機能が欠損する | 大規模回帰 | G2 は一 instance 互換に限定、所有者単位で移行、baseline smoke |
| shutdown 中の pending callback | UAF、double release | 明示 shutdown state、queue close、ordered drain、single release test |
| pane が pinned target の失効後に別文書へ向く | 意図しない操作 | silent fallback 禁止、FollowActive へ戻して通知 |
| 自由度が高すぎて操作不能になる | 学習負荷、support 増大 | zone 制約、最大二 group、named presets、常時使える reset |

---

## 13. 実装順序に関する判断

この計画では、dock や複数 window の見た目を先に作らない。先に ID、所有権、CoreHost、RendererHost を複数化し、その後に visible UI を接続する。理由は、tab や window を先に増やすと、現在の単一 `AppContext`、単一 Core、単一 Canvas への暗黙参照が残り、見た目は動いても command、save、snapshot、close の対象を保証できないためである。

また、複数文書と同一文書の複数 view を同時に扱うが、次の二つは明確に分ける。

- 文書を増やす: `DocumentSession` と Core handle を増やす。
- 表示を増やす: 同じ session に `DocumentView` を増やす。

Canvas は表示 tab 数ではなく可視 editor group 数だけ作る。これにより、文書を多数開いても renderer thread、swap chain、GPU target が tab 数に比例して増えず、複数文書の利便性と resource 制御を両立できる。

G0 から G6 までを最初の機能的な導入単位、G7 から G9 を workspace 導入単位、G10 から G12 を multi-window/session 導入単位、G13 を release gate とする。各単位の途中でも、現在相当の一 window、一 group、固定 preset を使用可能な状態に保つ。
