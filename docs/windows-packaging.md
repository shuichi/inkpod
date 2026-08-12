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
The dialog also displays the semantic application version and the configured
numeric build number as `Version <version> (Build <number>)`.

The winapp CLI 0.5.0 SVG renderer omits a group when the group itself uses the
SVG `feDropShadow` primitive. Keep optional filter effects on separate shadow
elements rather than on `icon-body`; otherwise the background can disappear
from every generated Windows asset. The checked-in source keeps the squircle
and artwork geometry intact and omits only that unsupported outer shadow.

`inkpod_windows_assets` verifies the required file set, representative package
PNG dimensions, ICO directory entries, the configured x64 or ARM64 package
identity, executable name, and that the four-part MSIX version matches the CMake
project and build numbers. CMake materializes an architecture-specific manifest
in the build directory from the checked-in asset-authoring manifest. When
changing the application version, update `project(inkpod VERSION ...)` in
`CMakeLists.txt`. `INKPOD_BUILD_NUMBER` supplies the fourth component, defaults
to zero for local builds, and accepts decimal values from 0 through 65535.
Hosted CI passes the GitHub Actions run number explicitly. The same value is
embedded in the executable version resource and shown by the About dialog.

Local x64 and ARM64 release production uses `scripts/build-windows-x64.ps1` and
`scripts/build-windows-arm64.ps1`, respectively. One wrapper invocation takes
an exclusive `.inkpod-local/build-number.lock`, reads the Git-ignored shared
counter and any higher existing CMake-cache value for its architecture,
reserves the next number before starting work, and passes that exact value to
every selected Debug/Release configure preset. A failed or interrupted build
keeps its reserved number, so a later artifact cannot reuse the same identity.
Both wrappers reject malformed state and refuse to wrap past 65535. `-DryRun`
does not reserve a number. Direct CMake preset use and hosted CI do not mutate
the local counter.

The manifest registers `.inkpod` as the `inkpod` file type. The executable uses
`CommandLineToArgvW` so a shell launch can pass one quoted Unicode path, then
opens it through the same native/raster and Recovery-aware path as the File
menu. An unpackaged build does not write per-user registry associations: it can
be selected through Windows **Open with**, while an installed MSIX supplies the
file-type registration and Windows retains control of the default-app choice.

Every Windows CMake build now assembles an unsigned package through the Windows
SDK `MakeAppx` tool. The artifact is written to
`build/<preset>/package/inkpod-<four-part-version>-<architecture>.msix` and contains `inkpod.exe`, the
manifest, generated PNG assets, `LICENSE.txt`, and `ThirdPartyNotices.txt`.
C/C++ uses MSVC `/MT` in every configuration and the x64/ARM64 Rust MSVC targets
use `-C target-feature=+crt-static`. The final executable therefore contains the
required Visual C/C++ runtime code and neither package ships app-local `MSVCP`,
`VCRUNTIME`, or UCRT DLLs. Windows system DLLs used by Win32, Direct2D, and
Direct3D remain operating-system dependencies. `inkpod_msix` can also be
selected explicitly as a build target.

The same build creates an install-free GitHub Release artifact through
`scripts/package-windows-portable.ps1` and the `inkpod_portable_zip` target. ZIP
names use the three-part CMake project version while the EXE and MSIX retain the
four-part version containing `INKPOD_BUILD_NUMBER`:

```text
build/windows-x64-release/package/Inkpod-0.1.0-windows-x64.zip
build/windows-arm-release/package/Inkpod-0.1.0-windows-arm.zip
```

`arm` in the archive name denotes the ARM64 target. Each ZIP has no enclosing
directory and contains exactly these case-sensitive root entries:

```text
inkpod.exe
README.txt
LICENSE.txt
ThirdPartyNotices.txt
```

The script writes a unique temporary archive beside the destination, verifies
the exact entry set and each source-file SHA-256, and only then atomically
replaces an older artifact. Creation or validation failure before replacement
leaves an existing valid ZIP unchanged. The portable build registers no `.inkpod`
association and still uses the current
Windows user profile and HKCU for workspace, recent-file, autosave, recovery,
and Help-cache state.

`inkpod_windows_msix_payload_smoke` unpacks the produced artifact with `MakeAppx`
without elevation and verifies the executable, identity/version/architecture, license,
notices, `.inkpod` association, and absence of app-local dynamic CRT DLLs.
`inkpod-ffi` has a Windows MSVC compile-time guard for the `crt-static` target
feature, and `inkpod_windows_static_crt` inspects the final PE dependencies.
Packaging depends on the same PE check, so a dynamic-CRT executable cannot
produce an MSIX or portable ZIP.
`inkpod_windows_portable_zip_payload_smoke` verifies the exact ZIP name, entry
count/casing/root placement, source hashes, PE architecture, embedded version,
and an extracted x64 `--portable-smoke-test`. The lightweight portable smoke
proves that the packaged executable loads outside the build tree and can query
the Rust ABI version and create/destroy a Core. The full `--abi-smoke-test`
remains a separate CTest and is not repeated by the ZIP check. Hosted CI runs
these checks for Debug and Release.

`scripts/publish-windows-release.ps1` is the complete local prerelease
orchestrator. `-Version <major.minor.patch> -DryRun` validates the version and
download-page replacement contracts and prints the proposed work without
editing files, building, or contacting GitHub. `-Publish` requires a clean
release branch synchronized with `origin`, updates and pushes the version,
clean-builds both Release presets, validates the portable ZIP and MSIX payloads,
creates the annotated three-part `v<version>` tag, and uploads the x64 and ARM
portable ZIPs to a GitHub prerelease. It then reads the actual uploaded asset
URLs, updates `html/index.html`, and commits/pushes that page separately. The tag
therefore names the exact source used for the binaries rather than the later
download-link commit.

CTest runs automatically for the build matching the native host architecture.
The other architecture is still clean-built and receives architecture,
embedded-version, payload, checksum, and static-runtime validation, but its
native CTest and interaction rows must be recorded on matching Windows hardware
as described in `docs/windows-release-checklist.md`.

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
unsigned.

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
