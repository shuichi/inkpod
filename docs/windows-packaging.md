# Windows packaging assets

`AppIcon.svg` at the repository root is the source of truth for the Windows app
icon. The checked-in MSIX PNG assets and Win32 icon live under
`apps/windows/package/Assets`.

Regenerate them with Windows App Development CLI 0.4 or later from the
repository root:

```powershell
winapp manifest update-assets AppIcon.svg `
  --manifest apps/windows/package/Package.appxmanifest
```

The command currently emits 48 PNG files: five scale variants for each
manifest asset, plus plated and unplated target-size variants for the app-list
icon. It also emits `app.ico` with 16, 24, 32, 48, and 256 pixel images. The
Win32 resource script embeds that same ICO in `inkpod.exe`, so Explorer, the
main window, and the title bars use one generated asset. The About dialog embeds
the generated 88 px app-list PNG for an exact match at the 144-DPI reference
scale, plus the 256 px PNG for high-DPI downsampling; it does not ask Windows to
upscale the 48 px ICO entry to its 88 px display box.

The winapp CLI 0.5.0 SVG renderer omits a group when the group itself uses the
SVG `feDropShadow` primitive. Keep optional filter effects on separate shadow
elements rather than on `icon-body`; otherwise the background can disappear
from every generated Windows asset. The checked-in source keeps the squircle
and artwork geometry intact and omits only that unsupported outer shadow.

`inkpod_windows_assets` verifies the required file set, base and About PNG
dimensions, ICO directory entries, and that the four-part MSIX version matches
the CMake project version. When changing the application version, update both
`project(inkpod VERSION ...)` in `CMakeLists.txt` and `Identity/@Version` in
`Package.appxmanifest`; the last MSIX component remains zero.

The manifest and visual assets are packaging preparation only. A distributable
MSIX still requires release-layout assembly, publisher selection, signing, and
clean Windows 11 install/uninstall verification before M8 can be marked
Verified.
