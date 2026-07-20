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
main window, and the About dialog all use one generated asset.

`inkpod_windows_assets` verifies the required file set, base PNG dimensions,
ICO directory entries, and that the four-part MSIX version matches the CMake
project version. When changing the application version, update both
`project(inkpod VERSION ...)` in `CMakeLists.txt` and `Identity/@Version` in
`Package.appxmanifest`; the last MSIX component remains zero.

The manifest and visual assets are packaging preparation only. A distributable
MSIX still requires release-layout assembly, publisher selection, signing, and
clean Windows 11 install/uninstall verification before M8 can be marked
Verified.
