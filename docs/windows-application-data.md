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
  "formatVersion": 1,
  "general": {
    "uiLanguage": "ja-JP"
  },
  "saveAndRecovery": {
    "restorePreviousDocuments": true
  },
  "animation": {
    "sequenceCellSwitch": "autosave-before-switch",
    "sequenceEndpoint": "wrap"
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
作成した profile と active profile だけを JSON に保存する。workspace の pane、
zone、tab、preset も `layer-plane`、`right`、`right-tab-1`、`coloring` のような
stable かつ人間が理解できる key で表す。

## Decode and save rules

- missing file または省略可能 section は current build の既定値を使う。
- duplicate／unknown field、不正 UTF-8、不正 enum、上限超過、trailing data、
  非現行 `formatVersion` は staged decode で設定全体を拒否する。
- 不正な既存ファイルを通常終了時の自動保存で上書きしない。application は既定値で
  継続し、診断を出す。利用者が環境設定を明示適用した場合は current schema で置換できる。
- 保存は同じ directory に temporary file を完成し、flush、close 後に
  `MoveFileExW` の replace/write-through で置換する。destination を先に truncate しない。
- 開発中の形式なので旧 HKCU 値、旧 workspace record、
  `application-settings-v1.bi` からの migration は実装しない。

`.inkshortcuts` の import/export も同じ readable binding 表現を使い、top-level
`format` は `inkpod-shortcuts`、current `formatVersion` は `2` とする。
