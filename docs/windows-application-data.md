# Windows application data

## Storage root

inkpod が所有する Windows ユーザーデータは、すべて
`%LOCALAPPDATA%\inkpod` 以下へ保存する。通常設定を `%APPDATA%` または
HKCU registry へ複製しない。HKCU は将来の file association、shell integration、
managed policy 等、Windows との統合に registry が必要な場合だけ使用する。

```text
%LOCALAPPDATA%\inkpod\
  Settings\
    inkpod-settings.json
  Session\
    inkpod-session.bin
  Recovery\
  batch-sets\
  Help\
  Cache\
  Logs\
```

`Settings\inkpod-settings.json` は通常設定の唯一の正本である。ファイル名は
固定し、schema version は top-level の `formatVersion` に置く。前回文書の
path だけは session-only の bounded binary record `Session\inkpod-session.bin`
に分離する。

`ヘルプ > 設定ファイルを開く` はこの固定 path を Windows shell の `open`
動詞へ渡す。ファイルが未作成の場合だけ現在設定を通常の原子的保存経路で
materialize し、既存の不正な JSON は上書きせず編集対象としてそのまま開く。

## `inkpod-settings.json`

設定ファイルは UTF-8 JSON、2-space indent、末尾改行付きで書く。値には
意味の読める文字列、真偽値、数量値、配列、object を使う。Win32 command ID、
virtual-key、scan-code、modifier bit mask、Base64、binary blob を JSON へ保存しない。

代表例は次のとおり。

```json
{
  "format": "inkpod-settings",
  "formatVersion": 4,
  "general": {
    "uiLanguage": "ja-JP"
  },
  "saveAndRecovery": {
    "restorePreviousDocuments": true,
    "defaultRasterFormat": "png"
  },
  "animation": {
    "sequenceCellSwitch": "autosave-before-switch",
    "sequenceEndpoint": "wrap",
    "sequenceThumbnailWidthDip": 64
  },
  "colorManagement": {
    "outputGuardProfile": "bt709-conservative-ycbcr"
  },
  "keyboardShortcuts": {
    "keyboardLayout": "automatic",
    "activeProfile": "user-1",
    "customProfiles": [
      {
        "key": "user-1",
        "name": "My shortcuts",
        "bindings": [
          {
            "command": "file.save",
            "slot": "primary",
            "context": "global",
            "action": "execute",
            "match": "logical",
            "strokes": [
              {
                "logicalKey": "S",
                "physicalKey": "KeyS",
                "modifiers": [
                  "ctrl"
                ]
              }
            ]
          }
        ]
      }
    ]
  },
  "workspaces": {
    "windows": [],
    "savedLayouts": []
  }
}
```

組み込み shortcut profile は application の既定値なので複製保存せず、利用者が
作成または import した profile と active profile だけを JSON に保存する。組み込み
profile は Windows／VS Code 慣例だけを割り当てた sparse profile であり、bindings に
存在しない production command も shortcut editor の完全な command catalog には
`未割当`として現れる。組み込み既定表を application update で変更しても、保存済み
custom profile を新しい既定から再生成しない。利用者が明示した無修飾 `Q`、`K`、`A`
等を含む assignment、unassignment、primary／secondary slot、context、action、match は
そのまま保持する。Reset は利用者が選択した profile／command に対する明示操作だけとし、
settings decode や起動時の既定解決を custom profile の暗黙 reset に使わない。

workspace の pane、zone、tab、preset も `layer-plane`、`right`、`right-tab-1`、
`coloring` のような stable かつ人間が理解できる key で表す。

現行の設定 schema は `formatVersion: 4` だけを decode する。
`animation.sequenceThumbnailWidthDip` の追加に伴い通常設定を 4 へ更新した。
`.inkshortcuts` preset は引き続き version 3 である。旧 version は
移行または decode せず、下記の識別・削除規則に従う。文書の
`.inkpod` version、replay epoch、公開 C ABI はこの表示変更では変わらない。
`animation.sequenceThumbnailWidthDip` は 32～96 DIP の整数で、既定値は 64 DIP
である。Sequence pane の表示倍率と単独 Bottom dock の実測固定高だけに作用し、
文書、Core thumbnail、cache key、history、保存形式を変更しない。
`saveAndRecovery.defaultRasterFormat` は新規セルのラスタ保存形式で、
`png`（既定値）、`tiff`、`tga`、`bmp` を指定できる。環境設定の一般ページからも
同じ値を変更できる。この設定は既存文書の保存形式を変更しない。読み込んだ
ラスタ画像の形式、または新規作成時の既定形式は `.inkpod` の文書 metadata に
保持し、通常保存では同名の native とラスタ画像を Rust の保存ジョブで書き出す。

`Recovery` の画像、付随 metadata、候補列挙、通常保存との新旧比較、破棄は
application 共通の Rust I/O manager が処理する。Windows は path と typed metadata を
渡し、候補確認ダイアログや保存後の画面遷移をジョブ完了後に進める。
Recovery metadata の現行形式は checksummed binary version 2 で、旧形式を移行しない。

## Decode and save rules

- missing file または省略可能 section は current build の既定値を使う。
- valid JSON object の top-level に `format` と `formatVersion` が各一つだけあり、
  `format` が `inkpod-settings`、version が正の整数かつ current 未満なら旧版と識別する。
  旧版の内容は decode せず、同じ path を削除権付きで再度開き、検出時の byte 列と完全一致
  することを確認した同じ handle で削除する。削除成功後は missing file と同様に既定値を使う。
  検証前に file が変わった場合は一度だけ再読込・再判定し、削除または検証の I/O failure は
  起動 failure とする。
- duplicate／unknown field、不正 UTF-8、不正 enum、上限超過、trailing data、未来 version、
  format 不一致、旧版と一意に識別できない非現行 file は staged decode で設定全体を拒否して保持する。
- 不正な既存ファイルを通常終了時の自動保存で上書きしない。application は既定値で
  継続し、診断を出す。利用者が環境設定を明示適用した場合は current schema で置換できる。
- 保存は同じ directory に temporary file を完成し、flush、close 後に
  `MoveFileExW` の replace/write-through で置換する。destination を先に truncate しない。
- 開発中の形式なので旧 HKCU 値、旧 workspace record、
  `application-settings-v1.bi` からの migration は実装しない。上記の旧版 JSON 削除は
  migration／下位互換 reader ではなく、current-only policy の cleanup とする。

`.inkshortcuts` の import/export も同じ readable binding 表現を使い、top-level
`format` は `inkpod-shortcuts`、current `formatVersion` は `3` とする。v1/v2 は
migration せず拒否する。sparse な bindings は command catalog の縮小を意味せず、
省略した command は未割当として編集可能である。settings と `.inkshortcuts` の
decode／validation は、bare `Alt`、unmodified `F10`、`Alt+Space`、top-level menu
mnemonic の `Alt+英字` が既存 custom v3 profile にあってもそのまま保持し、
round-trip する。これらは shortcut editor の新規 record／rebind 時には拒否し、
runtime は native menu／system route を優先するため保持済み割当も発火しない。
`Alt+F4` は組み込み Exit binding の標準例外であり、OS が application へ配送しない
global shortcut のために hook を導入しない。
