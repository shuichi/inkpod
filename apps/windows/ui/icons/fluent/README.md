# Fluent UI System Icons subset

Windows の Tool pane と pane action に使う Microsoft Fluent UI System Icons の固定 subset です。

- upstream: `microsoft/fluentui-system-icons`
- upstream release: `1.1.337`
- upstream commit: `84e8a2ae0e55b3cbe176b5cc33154fe82ef363cc`
- license: MIT (`LICENSE.txt`)
- selected source and SHA-256: `selected-icons.tsv`
- generated runtime asset: `fluent_icon_masks.bin` (48 x 48, A8, atlas order is the manifest order)

`svg/` の選択済み SVG と生成済み atlas は repository に固定し、通常 build、CI、配布 build は network access、npm package、font、または generator の install を要求しません。atlas は `app_common.rc` から `RCDATA` として executable に埋め込みます。MSIX と portable ZIP は同じ executable を収録するため、別の package file は増えません。

SVG や選択を更新する場合だけ、Windows の repository root で次を実行します。

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/generate-windows-fluent-icons.ps1
```

generator は Windows に標準搭載される WPF path renderer を使い、外部 package を install しません。生成後は `ctest -R inkpod_windows_fluent_icons` で provenance、個別 SVG hash、atlas header/hash、resource 埋め込み、license/notice、通常 build から generator が分離されていることを検証します。

SF Symbols の image、SVG、path data、font glyph はこの directory と Windows source tree に含めません。意味上の識別子は C++ の `ToolIconId` / `PaneIconId` に閉じ、Rust Core、C ABI、文書・履歴・workspace layout には保存しません。
