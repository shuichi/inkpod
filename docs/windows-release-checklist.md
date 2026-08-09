# Windows release checklist

This is the current reproducible release-candidate procedure for accessibility,
DPI, device loss, performance, and shutdown behavior. A release record identifies
the tested commit, preset, native CPU architecture, Windows build, display
language, input method, display scale, and assistive technology. A cross-built
executable is not evidence for a native interaction row.

## Automated evidence

Run the complete CTest suite for every x64 and ARM64 Debug/Release artifact in
the release scope. Retain build and host-independent results separately from
native execution results. The following x64 evidence is mandatory:

| Scenario | Automated evidence |
|---|---|
| Resource ownership | `inkpod_windows_owner_model`, `inkpod_windows_core_host`, `inkpod_windows_renderer_host`, and `inkpod_windows_smoke`; the GUI smoke checks document/view/Canvas/pane counts, cache budgets, route namespaces, Undo/Redo, and device reset. |
| Keyboard-only routing | `inkpod_windows_smoke` covers forward/reverse tab, editor-group, and F6 focus cycles plus captured-view close, including focus in a standard edit control. |
| Screen-reader bridge | `inkpod_windows_smoke` reads MSAA/UI Automation names for dirty tabs, captionless splitters, panes, AutoHide controls, pane targets, and Batch job status. |
| DPI and theme change | `inkpod_windows_smoke` sends 192-DPI, parent-DPI, theme, and setting changes while checking layout, tab-drag cancellation, Canvas routing, and unchanged document state. |
| Device lost and shutdown | `inkpod_windows_renderer_host`, `inkpod_windows_smoke`, and the fault matrix cover host-wide device reset, stale/queue rejection, pending work, non-final/final window close, and owner-thread teardown. |

Automated success does not replace real assistive-technology, display, and IME
observations. It makes routing, lifetime, and state-preservation regressions
repeatable on every supported build.

## Native performance gate

Run the private performance smoke from a native Release build. Because `inkpod`
is a Windows GUI executable, capture its standard-error records explicitly:

```powershell
$perfLog = Join-Path $env:TEMP 'inkpod-native-performance.log'
$process = Start-Process -FilePath '.\inkpod.exe' `
    -ArgumentList '--performance-smoke-test' -Wait -PassThru `
    -WindowStyle Hidden -RedirectStandardError $perfLog
Get-Content -LiteralPath $perfLog
if ($process.ExitCode -ne 0) { throw "performance smoke failed: $($process.ExitCode)" }
```

The command rejects semantic drift, an incomplete 256-tile fixture, wheel edits
to document state, drawing revision/sample-count drift, anything other than 512
wheel Presents and 16 drawing Presents, queue rejection, or renderer resource-
limit events. Renderer idle means both an empty queue and zero in-flight work
after the last GPU update/Present path returns.

For wall-clock validation, use the matching approved environment range and
procedure in [`core-benchmark-baseline.md`](core-benchmark-baseline.md). Retain
the environment ID, toolchain, target, Release profile, display/refresh mode,
power scheme, all measured samples, and semantic counters. Do not widen an
envelope to accept a result.

## Native interaction rows

Perform these rows on native x64 for every release candidate. Record additional
architectures separately when suitable native hardware is available; do not
infer an interaction result from a cross-build. Record Pass, Fail, or Blocked
with a screenshot, short observation, or issue ID, and restore Windows settings
after each row.

1. **High contrast.** Open four documents in two visible groups, create a second
   window, enable a Windows high-contrast theme, and traverse every pane and
   editor group. Text, selection, focus, disabled state, owner-drawn thumbnails,
   splitters, and AutoHide controls must remain distinguishable. Disable high
   contrast and verify the application follows the theme change without restart.
2. **200% DPI.** With 100% and 200% displays, move both workspace windows between
   displays. Resize, split, dock/AutoHide, open a modeless pane, and cancel a tab
   drag through the DPI transition. No control may be clipped or double-scaled;
   document revision and Canvas route must remain unchanged.
3. **Keyboard only.** Without a pointer, traverse tabs in both directions,
   editor groups, the F6 focus cycle, panes, and AutoHide; invoke a menu command
   and close one captured view with Ctrl+F4. Focus must remain visible and scoped
   to the intended window and document.
4. **Narrator or equivalent UIA client.** Read the active/dirty document tab,
   splitter, every primary and secondary pane, follow/pinned target, disabled
   reason, and running/completed Batch status. Names must match visible target
   and state without pointer-derived or stale document identity.
5. **Japanese IME.** In a standard editable name field, compose, convert,
   confirm, and cancel Japanese text. Repeat after switching documents and with
   a modeless pane open. Composition must remain in the edit control; workspace
   shortcuts must not consume it or retarget the result.
6. **Language resources.** On Japanese Windows, inspect menus, common dialogs,
   pane titles, command errors, accessibility names, and About. Repeat the route
   on English Windows and record fallback behavior. Japanese-only display
   resources remain a known difference until `WIN-001` records otherwise.
7. **Shutdown after faults.** With two windows open, trigger the documented
   device-loss hook, close a non-final window, then close the final window with
   a dirty document and with a cancelled dirty prompt. No background owner
   thread, HWND, snapshot, or partial Core transaction may remain.

## Release record

```text
Commit/build:
Preset and native architecture:
Windows build:
Display language / IME:
Display scale and monitor arrangement:
Assistive technology and version:

Automated CTest: Pass | Fail | Blocked — evidence
Native performance: Pass | Fail | Blocked — environment ID and samples
High contrast: Pass | Fail | Blocked — evidence
200% DPI: Pass | Fail | Blocked — evidence
Keyboard only: Pass | Fail | Blocked — evidence
Screen reader: Pass | Fail | Blocked — evidence
Japanese IME: Pass | Fail | Blocked — evidence
Japanese resources: Pass | Fail | Blocked — evidence
English resources: Pass | Known difference | Fail | Blocked — evidence
Faulted shutdown: Pass | Fail | Blocked — evidence
```

Update [`implementation-status.md`](implementation-status.md) with only the
latest representative result and [`compatibility.md`](compatibility.md) only
when a requirement status, representative evidence, or known difference
changes. Do not append completed release records to this checklist; superseded
milestone observations belong in [`legacy.md`](legacy.md) or Git history.
