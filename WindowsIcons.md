# Windows ツールアイコン改修計画

## 1. 目的

macOS 版は引き続き SF Symbols を使い、Windows 版は Fluent UI System Icons を使う。両 OS で図形を同一にするのではなく、同じ Inkpod command が同じ操作概念を伝えることを揃える。

本計画の初期対象は、Windows の Tool pane に常設されている 20 command とする。macOS の SF Symbols、SwiftUI の描画方法、Rust Core、C ABI、文書形式は変更しない。

関連要件は `WIN-001`、`WORKSPACE-001`、`PKG-001`、`PORT-001` である。

## 2. 採用方針

- SF Symbols の画像、export した SVG、path data、font glyph を Windows 配布物や Windows source tree へ入れない。
- command の操作意味は `SPEC.md` と既存 command contract を正本とし、OS 間の対応は `tests/macos/macos-command-parity.json` の既存 `semanticKey` で照合する。
- macOS は `Image(systemName:)` / `Label(systemImage:)` で SF Symbols を解決する。
- Windows は `semanticKey` に対応する型付き `ToolIconId` を経由して Fluent UI System Icons の選択済み subset を解決する。
- platform icon 名を Rust Core、C ABI、履歴、`.inkpod`、workspace layout record へ入れない。
- Windows の Tool pane はアイコン専用にしない。`SPEC.md` の 72 x 34 DIP、一列、7 pt の一語ラベルを維持し、アイコンと compact label を上下に配置する。
- button caption、Tooltip、MSAA/UI Automation name は従来どおり完全な localized label を使う。アイコンは補助表現であり、操作名の代替にしない。
- 通常 build、CI、配布 build は network access や icon generator の install を必要としない。選択済み SVG と生成済み mask atlas を repository に固定する。

## 3. 現状と変更境界

現在の Windows Tool pane は `apps/windows/ui/dialogs/tool_palette.cpp` にある 20 個の `BS_OWNERDRAW` button である。`ToolPaletteEntry` は command、完全ラベル、描画用 compact label を持ち、描画時には文字だけを表示している。DPI 変更時の font/layout 更新、checked/disabled/focus 描画、Tooltip、MSAA/UIA、command state 同期はすでに接続されている。

改修では、この command/state/accessibility 経路を残して `DrawToolButton` の presentation だけを拡張する。Canvas renderer、renderer thread、document mutation、command dispatch の変更は行わない。

## 4. 意味対応表

`macOS SF Symbol` は現在の SwiftUI surface で明示されている値を記す。`—` は macOS に command は存在するが、現在の Tool surface に専用 SF Symbol がないことを表す。Fluent の名称は upstream の Regular variant 名であり、実装時は manifest に正確な upstream path と hash を固定する。

| Windows command | semanticKey | macOS SF Symbol | Windows Fluent 候補 | 対応方針 |
|---|---|---|---|---|
| `IDM_TOOL_PENCIL` | `tool.pencil` | `pencil` | `Pen` | 直接対応 |
| `IDM_TOOL_BRUSH` | `tool.brush` | `paintbrush` | `Paint Brush` | 直接対応 |
| `IDM_TOOL_ERASER` | `tool.eraser` | `eraser` | `Eraser` | 直接対応 |
| `IDM_TOOL_FILL` | `tool.fill` | `paintbrush.pointed.fill` | `Paint Bucket` | 塗りつぶしという操作概念で対応 |
| `IDM_TOOL_CLOSED_FILL` | `tool.closed.fill` | `square.dashed` | `Paint Bucket` + `Square Hint` badge | 閉領域を小さい境界 badge で区別 |
| `IDM_TOOL_FILL_EXTENSION` | `tool.fill.extension` | `arrow.up.left.and.down.right.magnifyingglass` | `Paint Brush Arrow Down` | 塗りを外へ延ばす操作として暫定採用。実サイズで判別できなければ `Paint Bucket` + expand badge にする |
| `IDM_TOOL_EYEDROPPER` | `tool.eyedropper` | `eyedropper` | `Eyedropper` | 直接対応 |
| `IDM_VECTOR_LINE` | `vector.line` | `line.diagonal` | `Line` | 直接対応 |
| `IDM_VECTOR_CURVE` | `vector.curve` | `point.topleft.down.curvedto.point.bottomright.up` | `Bezier Curve Square` | Bezier curve の操作概念で対応 |
| `IDM_VECTOR_RECTANGLE` | `vector.rectangle` | — | `Rectangle Landscape` | shape の意味で対応 |
| `IDM_VECTOR_ELLIPSE` | `vector.ellipse` | — | `Oval` | shape の意味で対応 |
| `IDM_VECTOR_POLYLINE` | `vector.polyline` | — | `Line Flow Diagonal Up Right` | 折れ線として暫定採用。矢印に誤認される場合は Fluent の line/node 要素だけで compound icon を作る |
| `IDM_VECTOR_ERASER` | `vector.eraser` | `eraser` | `Eraser Segment` | 線分を消す意味で通常 Eraser と区別 |
| `IDM_EFFECT_GRADIENT` | `effect.gradient` | — | `Circle Half Fill` | 濃度遷移の意味で対応 |
| `IDM_EFFECT_AIRBRUSH` | `effect.airbrush` | — | `Spray Can` | 直接対応 |
| `IDM_EFFECT_BOUNDARY_AIRBRUSH` | `effect.boundary.airbrush` | — | `Spray Can` + `Border Outside` badge | 境界限定を badge で区別 |
| `IDM_EFFECT_BLUR` | `effect.blur` | — | `Blur` | 直接対応 |
| `IDM_EFFECT_STAMP` | `effect.stamp` | — | `Image Copy` | source image を複製して押す意味で対応 |
| `IDM_EFFECT_DUST` | `effect.dust` | — | `Broom Sparkle` | ゴミ取り／cleanup の意味で対応 |
| `IDM_EFFECT_ALPHA_GRADIENT` | `effect.alpha.gradient` | — | `Transparency Square` | alpha/transparency の意味で通常 Gradient と区別 |

暫定または compound とした 7 項目は、16 DIP、compact label 併記、light/dark/high contrast の実画面で一度比較する。意味が曖昧な場合に SF Symbols の形へ寄せて描き直してはならない。代わりに別の Fluent symbol または複数の Fluent symbol を組み合わせた compound icon へ変更する。

## 5. 資産の取り込みと固定

### 5.1 配置

実装時は次の構成を追加する。

```text
apps/windows/ui/assets/fluent-tool-icons/
  LICENSE
  NOTICE
  upstream.json
  source/
    <選択した Regular SVG のみ>
  generated/
    tool-icons-16.png
    tool-icons-20.png
    tool-icons-24.png
    tool-icons-32.png
  manifest.json
```

- Fluent UI System Icons 全体は vendor せず、表に必要な最小 subset だけを保持する。
- `upstream.json` に repository URL、release/version、commit hash、取得日を記録し、floating `main` を参照しない。
- `manifest.json` に schema version、`ToolIconId`、`semanticKey`、Windows command symbol、upstream asset path、variant size、source SHA-256、atlas cell index、compound 構成を記録する。
- upstream の SVG は provenance と再生成用に保存する。runtime は SVG を解釈しない。
- PNG は透明背景の alpha-mask atlas とし、行順は `ToolIconId` の明示値と一致させる。
- nominal icon size は 16 DIP とし、96/120/144/192 DPI 用に 16/20/24/32 pixel atlas を用意する。upstream に一致する size variant がなければ一つ上の Regular variant から縮小し、その選択を manifest に記録する。
- compound icon は Fluent の構成要素だけから生成し、base と badge の source asset を両方 manifest に残す。

### 5.2 生成規則

- atlas generator と rasterizer は version を固定し、通常 build とは分離した明示的な asset-maintenance command にする。
- generator は source SVG の hash が manifest と一致しない場合に停止する。
- monochrome Regular variant を使い、asset 内の固定色を最終 PNG に焼き込まない。PNG は coverage/alpha だけを authority とする。
- generator は同じ入力、version、parameter から byte-identical PNG を生成し、`--check` で checked-in output の drift を検出する。
- upstream 更新は独立した変更として扱い、version/hash、全 atlas、notice、visual QA を同時に更新する。アプリ機能変更へ便乗して自動追従しない。

## 6. Windows 実装

### 6.1 型付き catalog

`apps/windows/ui/dialogs/tool_palette.h` または専用の `apps/windows/ui/tool_icon_catalog.h` に `enum class ToolIconId : std::uint8_t` を置く。`ToolPaletteEntry` は次を持つ形へ変更する。

```cpp
struct ToolPaletteEntry {
    UINT command;
    UiStringId label;
    UiStringId compact_label;
    ToolIconId icon;
};
```

- command と `ToolIconId` の対応は一つの compile-time table に限定する。
- `ToolIconId::Count` と `kToolPaletteEntryCount` を `static_assert` と test で照合する。
- `None` を production entry に許可しない。
- `semanticKey` との照合は manifest verifier で行い、Rust/C ABI に icon enum を追加しない。

### 6.2 resource と cache

- `apps/windows/app/resource.h` に 16/20/24/32 pixel atlas の resource ID を追加する。
- `apps/windows/app/app_common.rc` へ各 atlas を `RCDATA` として埋め込む。
- `apps/windows/ui/tool_icon_atlas.{h,cpp}` を追加し、UI thread 上で WIC により該当 atlas を一度だけ decode する。
- `ToolIconAtlas` は dialog owner の RAII object とし、module-global mutable cache にしない。workspace window/tool pane ごとに DPI と system color に対応する cache を所有する。
- decode 後の alpha coverage に `COLOR_BTNTEXT`、`COLOR_HIGHLIGHTTEXT`、`COLOR_GRAYTEXT` を適用した premultiplied BGRA `HBITMAP` を生成する。
- `WM_THEMECHANGED`、`WM_SYSCOLORCHANGE`、関連する `WM_SETTINGCHANGE` で tinted cache を破棄して再生成し、全 button を invalidate する。
- `WM_DPICHANGED_AFTERPARENT` で atlas、font、layout を同じ更新単位で切り替える。
- HBITMAP、WIC object、memory DC の所有権を RAII にし、dialog destroy 後に GDI object を残さない。
- runtime SVG parser、icon font、Direct2D UI rendering は追加しない。Canvas/renderer thread の D2D 所有境界を維持する。

必要な link dependency は、実装で WIC と alpha blend を採用した場合に限り `windowscodecs` と `msimg32` を `inkpod` target へ追加する。

### 6.3 owner-draw layout

`DrawToolButton` は次の順で描画する。

1. 既存と同じ system-color background と border。
2. button 上部中央に 16 DIP の icon。
3. button 下部中央に既存の localized compact label。
4. 最後に既存と同じ keyboard focus rectangle。

icon と label を横並びにすると 72 DIP 内で英語 label が不足するため、上下配置を標準とする。label は `DT_END_ELLIPSIS`、font 縮小、一文字略号を使わない。96/120/144/192 DPI の各言語で実測し、収まらない場合は button 高さを勝手に変えず、catalog の意味を保つ一語 compact label と pane の既定寸法を仕様に照らして再検討する。

checked/pressed、disabled、focus、keyboard route、command dispatch は既存実装を維持する。画像 resource の decode に失敗した場合は現在の文字だけの owner-draw へ戻し、button を消したり command を無効化したりしない。ただし正式 build の smoke test では fallback 発生を失敗にする。

## 7. Accessibility、DPI、theme

- accessible name は `UiText(entry.label)` のままにし、Fluent asset 名を読み上げない。
- Tooltip も完全ラベルのままにする。
- selected/checked は既存の command state と accessibility state を共有し、icon variant だけで状態を表さない。
- disabled icon は `COLOR_GRAYTEXT`、selected/pressed icon は `COLOR_HIGHLIGHTTEXT`、通常 icon は `COLOR_BTNTEXT` を使う。
- high contrast では system color 以外の固定 RGB、opacity だけの状態差、Fluent 固有 accent colorを使わない。
- DPI は `MulDiv(reference_px, target_dpi, 96)` とし、16/20/24/32 atlas から exact size を選ぶ。OS DPI を Core/Canvas transform へ渡さない。
- RTL 対応が必要な将来の directional icon は Fluent metadata の direction policy を manifest に持たせる。今回の 20 icon は direction を意味に含める compound を除き、無条件 mirror しない。

## 8. 検証計画

### 8.1 repository/asset contract

`tests/windows/verify_tool_icons.py` を追加し、非 Windows CI でも次を検査する。

- manifest schema、upstream version/commit、license/notice の存在。
- 20 command、20 `semanticKey`、20 `ToolIconId` の一対一性と重複禁止。
- `tests/macos/macos-command-parity.json` に同じ Windows command と `semanticKey` が存在すること。
- source SVG と generated atlas の SHA-256。
- atlas の PNG signature、pixel format、16/20/24/32 cell 寸法、cell count、非空 alpha coverage。
- Windows icon tree に `SF Symbols`、Apple export metadata、macOS SF symbol asset が混入していないこと。
- generated file drift。通常の test は network を使わない。

### 8.2 Windows unit/smoke

- resource から全 atlas を load/decode できる。
- 全 `ToolPaletteEntry` が有効な cell を持つ。
- 96/120/144/192 DPI で icon bounds が button bounds 内にあり、compact label と重ならない。
- normal、checked、pressed、disabled、focus の owner-draw が成功し、system color policy と一致する。
- `WM_DPICHANGED_AFTERPARENT`、`WM_THEMECHANGED`、`WM_SYSCOLORCHANGE` 後に cache generation が進み、古い HBITMAP を再利用しない。
- resource decode failure fault injection で文字 fallback になり、command/caption/accessibility が維持される。
- ToolTip、window caption、MSAA name、UIA name が従来の完全ラベルと一致する。
- command enable/checked state と button state が従来どおり一致する。
- dialog create/destroy、workspace reset、DPI/theme 切替の反復後に GDI object/resource ownership leak がない。

既存の `apps/windows/app/app_smoke.cpp` の Tool palette 検証は削除せず、icon resource、表示 bounds、fallback 非発生を追加する。既存の Japanese/English caption、Tooltip、MSAA/UIA、owner-draw、command-state assertions はそのまま残す。

### 8.3 visual QA

Windows 11 の Japanese/English、light/dark/high contrast、100/125/150/200% DPI で screenshot と操作確認を行う。特に次を比較する。

- Pencil/Brush/Eraser/Fill の誤認がない。
- Fill/Closed Fill/Fill Extension の三つを label を読む前でも区別できる。
- Line/Curve/Polyline、Eraser/Vector Eraser、Gradient/Alpha Gradient、Airbrush/Boundary Brush が相互に区別できる。
- checked、disabled、keyboard focus が icon の追加前と同等以上に明確である。
- 72 x 34 DIP button、80 DIP Tool pane、scroll、keyboard navigation が崩れていない。

## 9. ライセンスと配布

- Fluent UI System Icons の upstream `LICENSE` と必要な `NOTICE` を選択資産と一緒に保持する。
- `docs/third-party-notices.md` に Microsoft Corporation の copyright、MIT license text、使用した repository/version/commit、選択 SVG から生成した atlas を配布することを記録する。
- portable ZIP と MSIX に既存経路で入る `ThirdPartyNotices.txt` が新しい notice を含むことを packaging test で確認する。
- About/Help の謝辞が full notice authority を参照する既存経路を維持し、dependency/notice drift test を更新する。
- SF Symbols や Apple Design Resources の notice を Windows 配布物へ追加して利用を正当化する方式は採らない。Windows 側へ Apple asset 自体を含めない。

参照元:

- [Apple SF Symbols](https://developer.apple.com/sf-symbols/)
- [Apple Design Resources License](https://developer.apple.com/support/downloads/terms/apple-design-resources/Apple-Design-Resources-License-20230621-English.pdf)
- [Microsoft Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons)
- [Fluent UI System Icons MIT License](https://github.com/microsoft/fluentui-system-icons/blob/main/LICENSE)
- [Fluent UI System Icons SVG package](https://github.com/microsoft/fluentui-system-icons/blob/main/packages/svg-icons/README.md)

## 10. 実装の分割順

### Slice 1: 資産と contract

1. Fluent release/commit と mapping table を固定する。
2. 最小 SVG subset、LICENSE、NOTICE、manifest、4 atlas を追加する。
3. repository/asset verifier と third-party notice を追加する。
4. この時点では製品 UI を変更しない。

### Slice 2: 一つの縦切り

1. `ToolIconId`、resource ID、`ToolIconAtlas` を追加する。
2. Pencil 一項目だけを icon + label で描画し、DPI/theme/failure ownership test を通す。
3. Canvas renderer や command routing に変更がないことを確認する。

### Slice 3: 直接対応 icon

Pencil、Brush、Eraser、Fill、Eyedropper、Line、Curve、Rectangle、Ellipse、Vector Eraser、Airbrush、Blur、Dust を接続し、Japanese/English と 4 DPI の bounds test を通す。

### Slice 4: 曖昧／compound icon

Closed Fill、Fill Extension、Polyline、Gradient、Boundary Brush、Stamp、Alpha Gradient を接続する。visual QA で相互識別を確認し、必要な mapping 調整はこの slice 内だけで行う。

### Slice 5: 完了処理

1. full Windows build、ABI smoke、product smoke、MSIX/portable packaging test を実行する。
2. Windows 11 の light/dark/high contrast と 100/125/150/200% DPI を確認する。
3. 実装状態が変わった時点で `docs/compatibility.md` の `WIN-001` / `WORKSPACE-001` / `PKG-001` / `PORT-001` と、必要な `docs/implementation-status.md` を更新する。
4. 未実施の physical DPI、screen reader、high contrast 確認を実施済みとして記載しない。

## 11. 完了条件

- Windows Tool pane の全 20 command に Fluent 由来 icon があり、SF Symbols asset を一つも含まない。
- macOS の SF Symbols と Windows の Fluent icon が command の `semanticKey` で対応している。
- icon の有無にかかわらず完全ラベル、Tooltip、MSAA/UIA、keyboard focus、enable/checked state が維持される。
- Japanese/English、96/120/144/192 DPI、light/dark/high contrast で読め、ラベルの省略や一文字略号がない。
- resource failure は文字表示へ安全に fallback し、正式 build/smoke では resource 欠落を検出する。
- selected subset の provenance、hash、MIT license、ThirdPartyNotices、packaging が検証される。
- Rust Core、C ABI、file/replay version、document behavior に変更がない。
