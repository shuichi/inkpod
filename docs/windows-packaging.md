# Windows packaging

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
main window, title bars, and About dialog use one generated asset. About calls
`LoadIconWithScaleDown` with its target-DPI size so Common Controls selects a
suitable ICO image and scales it down instead of maintaining a separate WIC
PNG-decoding path. The generated app-list PNGs remain package assets referenced
by the MSIX manifest; they are not duplicated as About-specific Win32 resources.

The winapp CLI 0.5.0 SVG renderer omits a group when the group itself uses the
SVG `feDropShadow` primitive. Keep optional filter effects on separate shadow
elements rather than on `icon-body`; otherwise the background can disappear
from every generated Windows asset. The checked-in source keeps the squircle
and artwork geometry intact and omits only that unsupported outer shadow.

`inkpod_windows_assets` verifies the required file set, representative package
PNG dimensions, ICO directory entries, the configured x64 or ARM64 package
identity, executable name, and that the four-part MSIX version matches the CMake
project version. CMake materializes an architecture-specific manifest in the
build directory from the checked-in asset-authoring manifest. When changing the
application version, update `project(inkpod VERSION ...)` in `CMakeLists.txt`;
the generated MSIX version's last component remains zero.

Every Windows CMake build now assembles an unsigned package through the Windows
SDK `MakeAppx` tool. The artifact is written to
`build/<preset>/package/inkpod-<version>-<architecture>.msix` and contains `inkpod.exe`, the
manifest, generated PNG assets, `LICENSE.txt`, `ThirdPartyNotices.txt`, and the
MSVC toolchain's matching x64 or ARM64 app-local CRT DLLs required by the `/MD`
executable. CRT discovery uses the developer environment's `VCToolsRedistDir`
instead of assuming that the compiler toolset and redistributable directory
have identical version names. If that variable is unavailable, CMake accepts
the compiler-version directory or a single unambiguous installed redist and
rejects multiple candidates. The runtime DLLs are explicit CMake dependencies,
so changing one rebuilds the package while an unchanged second build remains a
no-op. `inkpod_msix` can also be selected explicitly as a build target.

`inkpod_windows_msix_payload_smoke` unpacks the produced artifact with `MakeAppx`
without elevation and verifies the executable, identity/version/architecture, license,
notices, and required MSVC runtime payload. Hosted CI runs this test for Debug
and Release.

`inkpod_windows_msix_install_uninstall_smoke` is not registered in the default
CTest set. Configure with `-DINKPOD_ENABLE_ELEVATED_MSIX_TESTS=ON` to register it.
The test must run from an elevated Windows 11 workstation shell and rejects
Windows 10 and Windows Server. It refuses to
disturb an existing all-users `inkpod` package, copies the
unsigned build artifact to a private temporary directory, creates a one-day
ephemeral code-signing certificate whose subject matches `Identity/@Publisher`,
adds that certificate to LocalMachine Root only for the test, signs the copy,
installs it, verifies the installed version and payload, executes the installed
`inkpod.exe --abi-smoke-test`, uninstalls it, then removes the certificate,
private key, and temporary package in `finally`. Failure cleanup retains the
installed package identity until removal is confirmed, so a failed post-install
assertion can retry uninstall instead of orphaning the package. The generated artifact remains
unsigned; a release publisher must sign it with the organization's protected
production credential before distribution.

The default CTest set, including hosted Windows x64 CI, builds the MSIX and runs
manifest/assets, payload, ABI, and application smoke tests without registering
this administrator-only test. Run the full package test on an elevated clean
Windows 11 workstation with:

```powershell
cmake --fresh --preset windows-x64-release `
  -DINKPOD_ENABLE_ELEVATED_MSIX_TESTS=ON
cmake --build --preset windows-x64-release
ctest --preset windows-x64-release `
  -R inkpod_windows_msix_install_uninstall_smoke `
  --output-on-failure
```
