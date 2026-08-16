# macOS release checklist

This is the reproducible M12 release-candidate procedure for the native arm64
Tahoe app. A release is not complete when only the build or notarization script
passes: retain one record covering the automated profiles, native interaction
rows, signing identity, notarization log, and clean-machine launch. Record every
row as `Pass`, `Fail`, or `Blocked`; an empty or `Blocked` row is not release
evidence.

## Automated release gate

On an arm64 Mac running macOS 26 with Xcode 26, run:

```text
INKPOD_BUILD_NUMBER=<decimal-build-number> ./scripts/macOS.sh verify
```

The command runs the portable Rust format/lint/test/quick-benchmark/doc profile,
the full Swift unit and integration suite, CTest source/ABI/parity gates, the
launched-product XCUITest including `performAccessibilityAudit`, Metal API
Validation and the 200-cycle snapshot/chrome soak, Thread Sanitizer, and a real
arm64 `.xcarchive`. It stops on the first failure. The source gate separately
requires all 384 parity rows to be `implemented` with no placeholder evidence.

Windows x64 and ARM64 Debug/Release jobs are release inputs rather than results
that can be inferred from a Mac build. Retain the four CI job links and any
native interaction evidence required by
[`windows-release-checklist.md`](windows-release-checklist.md).

## Signed and notarized artifact

The protected release workflow imports a temporary Developer ID Application
certificate and App Store Connect notary key, creates a temporary keychain, and
invokes:

```text
INKPOD_VERSION=<major.minor.patch> \
INKPOD_BUILD_NUMBER=<decimal-build-number> \
INKPOD_CODESIGN_IDENTITY="Developer ID Application: …" \
INKPOD_NOTARY_PROFILE=inkpod-ci-notary \
./scripts/macOS.sh release
```

`release` cannot skip notarization. It reruns the automated gate, signs nested
Mach-O code before the enclosing app with Hardened Runtime and a secure
timestamp, creates and signs the DMG, verifies the DMG, waits for an accepted
notary result, retrieves the notary log, staples and validates the ticket, and
runs Gatekeeper assessment. Use `dmg` with an ad-hoc identity only for local
packaging diagnostics; such output is never a release artifact.

For the candidate, retain:

- `codesign --verify --deep --strict --verbose=2` output for `Inkpod.app`;
- displayed entitlements proving App Sandbox, app-scoped bookmarks, and
  user-selected read/write access, with no `get-task-allow`, network,
  automation, or library-validation exception;
- `lipo -archs` and `file` output proving every inspected executable and Rust
  archive is arm64 only;
- the accepted submission identifier and complete notary log;
- `stapler validate` and `spctl --assess` output;
- the final DMG SHA-256.

## GitHub Release publication

Publish only an already notarized candidate from a clean `main` checkout that
is synchronized with `origin/main`:

```text
gh auth status
INKPOD_VERSION=<major.minor.patch> ./scripts/macOS.sh publish
```

`publish` does not rebuild, sign, notarize, commit, or push a branch. It verifies
the local DMG signature, stapled ticket, Gatekeeper result, image checksum, exact
filename, and SHA-256, then mounts it read-only and checks the enclosed app's
signature, bundle ID, version/build, arm64-only executable, and exact entitlement
allowlist before touching release state. The default tag is
`v<INKPOD_VERSION>`. If that tag is absent, the command creates an annotated tag
at exact synchronized `HEAD`, pushes it, and creates a GitHub prerelease. If the
Release already exists, its public metadata is preserved and only the missing
DMG is uploaded.

Tag creation, Release creation, and asset upload are each followed by a remote
read. A concurrent publisher is accepted only when it fixed the same tag to the
same commit or uploaded byte-identical content. An existing identical asset is
a no-op. An asset with the same name and different bytes is rejected; the
command never uses `--clobber`, because deleting the old asset before a failed
upload would lose the published artifact. Set `INKPOD_GITHUB_PRERELEASE=0` only
when intentionally creating a new stable Release; it never changes an existing
Release between prerelease and stable.

## Native interaction matrix

Perform these rows on the exact notarized candidate. Restore changed System
Settings after each row.

1. **VoiceOver.** Read the active/dirty document, leading tool surface,
   inspector tabs and semantic rows, Canvas document/tool/zoom/selection value,
   Sequence, and Batch progress. Invoke every Canvas-only action through an
   accessibility action and confirm focus never moves to a stale document.
2. **Full Keyboard Access.** Traverse toolbar, tab/group, tool surface,
   inspector, timeline, sheet, and status regions in both directions. Invoke
   menu and shortcut duplicates and confirm their enabled/checked state and
   issue-time target agree.
3. **Japanese IME.** Compose, convert, confirm, and cancel text in document,
   layer, annotation, and Settings fields. Character and multi-stroke shortcuts
   must remain suspended while marked text exists.
4. **Appearance.** In Light and Dark appearances, test default and non-default
   accent colors, Reduce Transparency, Increase Contrast, and Reduce Motion.
   Glass fallback must preserve hierarchy, focus, and labels; palette, chart,
   checker, thumbnail, and color-judgement wells must remain opaque and
   color-neutral.
5. **Retina and multiple display.** Move two windows between 1x/2x displays,
   resize to 640/800/1200 points, mirror pane edges, collapse/expand chrome, and
   verify Fit, last-pixel hit testing, drawable size, and unchanged document
   state. Record a single-display setup as `Blocked`, not `Pass`.
6. **sleep/wake and display switch.** Sleep with visible and hidden tabs, wake,
   disconnect/reconnect a display, and confirm only renderer resources rebuild;
   document/session/view identities, history, dirty state, and savepoints stay
   intact.
7. **Memory pressure and GPU recovery.** Exercise the diagnostic injection while
   painting, during snapshot replacement, and with Light Table visible. Cache
   eviction must remain bounded and every accepted/rejected/replaced snapshot
   must be released exactly once.
8. **Faulted close and shutdown.** Saturate normal Core work, keep an active
   stroke, inject save failure and stale snapshots, close a view/window, then
   quit. Cancel and failure must preserve the live document; successful shutdown
   must leave no Core, renderer, task, lease, or snapshot owner.
9. **Clean Tahoe launch.** On a clean arm64 Tahoe account or machine, download
   the final DMG through a quarantine-producing path, mount it, drag the app to
   Applications, launch through Finder, and open/save/reopen a Unicode-named
   `.inkpod` file. Confirm Gatekeeper identifies the notarized Developer ID app
   without a bypass or privacy exception.

## Release record

```text
Commit:
Version / build:
macOS / Xcode / SDK:
Machine and native architecture:
Display arrangement / scale:
Input devices:
Assistive technology:
Appearance / accessibility settings:

Rust profile: Pass | Fail | Blocked — evidence
macOS unit/integration/CTest: Pass | Fail | Blocked — evidence
XCUITest accessibility audit: Pass | Fail | Blocked — evidence
Metal validation / 200-cycle soak: Pass | Fail | Blocked — evidence
Thread Sanitizer: Pass | Fail | Blocked — evidence
arm64 xcarchive: Pass | Fail | Blocked — evidence
Windows x64 Debug/Release: Pass | Fail | Blocked — CI evidence
Windows ARM64 Debug/Release: Pass | Fail | Blocked — CI/native evidence
VoiceOver: Pass | Fail | Blocked — evidence
Full Keyboard Access: Pass | Fail | Blocked — evidence
Japanese IME: Pass | Fail | Blocked — evidence
Appearance matrix: Pass | Fail | Blocked — evidence
Retina / multiple display: Pass | Fail | Blocked — evidence
sleep/wake / display switch: Pass | Fail | Blocked — evidence
Memory pressure / GPU recovery: Pass | Fail | Blocked — evidence
Faulted close / shutdown: Pass | Fail | Blocked — evidence
Developer ID signing: Pass | Fail | Blocked — identity and codesign evidence
Apple notarization: Pass | Fail | Blocked — submission ID and notary log
Staple / Gatekeeper: Pass | Fail | Blocked — evidence
GitHub Release publication: Pass | Fail | Blocked — tag, URL, and asset SHA-256
clean Tahoe launch/open/save/reopen: Pass | Fail | Blocked — evidence
Final DMG SHA-256:
Known differences / issue IDs:
Release decision: Ship | Do not ship
```

### 2026-08-16 signed candidate

```text
Commit: 0adac7c97b708961edfc93b2281678270d4a241a + current working-tree changes
Version / build: 0.2.3 / 175
macOS / Xcode / SDK: 26.6.1 / 26.6 (17F113) / 26.5
Machine and native architecture: local Tahoe host / arm64
Display arrangement / scale: Blocked — physical matrix not run
Input devices: Blocked — physical tablet matrix not run
Assistive technology: XCUITest audit only; manual VoiceOver blocked
Appearance / accessibility settings: automated audit only; manual matrix blocked

Rust profile: Pass — fmt, Clippy, 462 tests plus doctest, ten quick gates, rustdoc
macOS unit/integration/CTest: Pass — 95 Swift tests, headless host, CTest 22/22
XCUITest accessibility audit: Pass — 17 selected product/UI tests
Metal validation / 200-cycle soak: Pass — validation enabled, zero reported errors
Thread Sanitizer: Pass — complete current suite and headless host
arm64 xcarchive: Pass — Rust archive, XCTest, headless host, and app are arm64 only
Windows x64 Debug/Release: Blocked — workflow defined; native jobs not run
Windows ARM64 Debug/Release: Blocked — workflow defined; native jobs not run
VoiceOver: Blocked — physical interaction not run
Full Keyboard Access: Blocked — physical interaction not run
Japanese IME: Blocked — physical interaction not run
Appearance matrix: Blocked — complete physical matrix not run
Retina / multiple display: Blocked — multiple-display matrix not run
sleep/wake / display switch: Blocked — physical cycle not run
Memory pressure / GPU recovery: Blocked — complete physical matrix not run
Faulted close / shutdown: Pass — automated saturation/failure/stale/shutdown coverage
Developer ID signing: Pass — team ETD7LJJGQZ; app and DMG strict verification
Apple notarization: Pass — fd8ccacc-c1a2-4d30-9c5b-064ea38fc901, Accepted, no issues
Staple / Gatekeeper: Pass — stapler validate; source=Notarized Developer ID
GitHub Release publication: Blocked — candidate was built from working-tree changes; clean synchronized commit required
clean Tahoe launch/open/save/reopen: Blocked — clean account/machine not run
Final DMG SHA-256: 4b8bac5b70f21471c424cbd7acd62bfcae419ab179b1f83e4d62dc385248020f
Known differences / issue IDs: Intel unsupported; physical/manual and native Windows evidence outstanding
Release decision: Do not ship — remaining Blocked rows are not inferred from automation
```

Update [`implementation-status.md`](implementation-status.md) only with the
latest representative result and [`compatibility.md`](compatibility.md) only
when a requirement status, evidence, or known difference changes. Do not turn a
manual `Blocked` row into `Verified` based on source review or a cross-build.
