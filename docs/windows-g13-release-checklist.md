# G13 Windows release checklist

This checklist is the reproducible release procedure for the accessibility,
DPI, device-loss, and shutdown portion of GUI milestone G13. A release record
must identify the tested commit, preset, native CPU architecture, Windows build,
display language, input method, display scale, and assistive technology. A
cross-built executable is not evidence for a native interaction row.

## Automated evidence

Run the full CTest suite for x64 Debug and Release. For ARM64 Debug and Release,
retain the strict cross-build, MSIX, and host-independent CTest results. On
2026-08-03 the user explicitly waived the 11 native ARM64 executables for G13;
they remain `Not Run` and must not be described as passed. The following x64
evidence is mandatory:

| Scenario | Automated evidence |
| --- | --- |
| Resource ownership | `inkpod_windows_owner_model`, `inkpod_windows_core_host`, `inkpod_windows_renderer_host`, and `inkpod_windows_smoke`; the GUI smoke prints all four `inkpod-g13-resource` records and checks document/view/Canvas/pane counts, cache budgets, route namespaces, Undo/Redo, and device reset. |
| Keyboard-only routing | `inkpod_windows_smoke` covers forward/reverse tab, editor-group, and F6 focus cycles plus captured-view close, including focus in a standard edit control. |
| Screen-reader bridge | `inkpod_windows_smoke` reads MSAA/UI Automation names for dirty tabs, captionless splitters, panes, AutoHide controls, pane targets, and Batch job status. |
| 200% DPI and theme change | `inkpod_windows_smoke` sends 192-DPI, parent-DPI, theme, and setting changes while checking layout, tab-drag cancellation, Canvas routing, and unchanged document state. |
| Device lost and shutdown | `inkpod_windows_renderer_host`, `inkpod_windows_smoke`, and the G13 fault matrix cover host-wide device reset, stale/queue rejection, pending work, non-final/final window close, and owner-thread teardown. |

Automated success does not replace the real assistive-technology and IME rows
below. It does make routing, lifetime, and state-preservation regressions
repeatable on every supported build.

## Routine native revision-max performance gate

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

The command automatically rejects semantic drift, an incomplete 256-tile
fixture, wheel edits to document state, drawing revision/sample-count drift,
anything other than 512 wheel Presents and 16 drawing Presents, queue rejection,
or renderer resource-limit events. These semantic gates are mandatory on every
run. Before accepting measurements, verify that renderer idle means both an
empty queue and zero in-flight work after the last GPU update/Present path
returns; a wait that observes queue removal alone is invalid.

For routine wall-clock validation, select the matching approved environment
range in `core-benchmark-baseline.md`, retain the environment ID, toolchain,
target, Release profile, display/refresh mode, and power scheme, and run the
complete process at least five times after its internal fixture warm-up. Compare
the median with the range. For `wheel_zoom`, divide elapsed time per event by
the recorded display refresh interval; do not treat its display-paced absolute
nanoseconds as a CPU benchmark. A result below the range is diagnostic only when
all semantic work matches. If a median exceeds the upper edge, run an
independent second batch of at least five processes. Both medians must exceed
the upper edge before rejecting the candidate as a confirmed regression.

Reconstruct detached old revision-max commit `3f164db` only when the workload or
harness changes, a reference environment is created or revised, or an explicit
boundary audit is requested. In that case apply
`tests/revision_max_native_harness_3f164db.patch` (SHA-256
`2b434f0ab5827fc987f0cb583ff68f65c4af6b9aaf89531fa8735bee071044a0`) only
after `git apply --check` succeeds, and confirm that `git status --short` lists
exactly its eight allowlisted files. Do not reconstruct the harness by hand.
An envelope is not widened automatically; record the environment, complete
samples, semantic counters, reason, and explicit user approval. Exact fixture
semantics, active ranges, calibration samples, and rebaseline procedure are in
`core-benchmark-baseline.md`.

## Native interaction rows

Perform these rows once on native x64 for a release candidate. Native ARM64
interaction is outside the G13 gate under the 2026-08-03 user waiver. Record
Pass, Fail, or Blocked with a screenshot, short observation, or issue ID.
Restore the original Windows settings after each row.

1. **High contrast.** Open four documents in two visible groups, create a second
   window, enable a Windows high-contrast theme, and traverse every pane and
   editor group. Text, selection, focus, disabled state, owner-drawn thumbnails,
   splitters, and AutoHide controls must remain distinguishable. Disable high
   contrast and verify the application follows the theme change without restart.
2. **200% DPI.** With 100% and 200% displays if available, move both workspace
   windows between displays. Resize, split, dock/AutoHide, open a modeless pane,
   and perform a tab drag that is cancelled by the DPI transition. No control may
   be clipped or double-scaled, and the document revision and Canvas route must
   remain unchanged.
3. **Keyboard only.** Without a pointer, traverse tabs in both directions,
   traverse editor groups, run the F6 focus cycle, activate panes and AutoHide,
   invoke a menu command, and close one captured view with Ctrl+F4. Focus must be
   visible and must never jump to a different window or document.
4. **Narrator or equivalent UIA client.** Read the active/dirty document tab,
   splitter, each primary and secondary pane, follow/pinned target, disabled
   reason, and running/completed Batch status. Names must match the visible
   target and state, with no pointer-derived or stale document identity.
5. **Japanese IME.** In a standard editable name field, compose, convert,
   confirm, and cancel Japanese text. Repeat after switching documents and while
   a modeless pane is open. Composition must stay in the edit control; workspace
   shortcuts must not consume composition keystrokes or retarget the result.
6. **Language resources.** On Japanese Windows, inspect menus, common dialogs,
   pane titles, command errors, accessibility names, and About. On English
   Windows, repeat the same route and record fallback behavior. The current
   product has Japanese resources only, so Japanese text on an English display
   language is an explicit known difference rather than a passed English
   localization row.
7. **Shutdown after faults.** With two windows open, trigger the documented
   device-loss test hook, close a non-final window, then close the final window
   with a dirty document and with a cancelled dirty prompt. No background owner
   thread, HWND, snapshot, or partial Core transaction may remain.

## Release record template

```text
Commit/build:
Preset and native architecture:
Windows build:
Display language / IME:
Display scale and monitor arrangement:
Assistive technology and version:

High contrast: Pass | Fail | Blocked — evidence
200% DPI: Pass | Fail | Blocked — evidence
Keyboard only: Pass | Fail | Blocked — evidence
Screen reader: Pass | Fail | Blocked — evidence
Japanese IME: Pass | Fail | Blocked — evidence
Japanese resources: Pass | Fail | Blocked — evidence
English resources: Known difference — Japanese-only fallback
Faulted shutdown: Pass | Fail | Blocked — evidence
```

As of 2026-08-03, the automated x64 rows had executable coverage and the native
ARM64 executables were `Not Run` under the then-current waiver. Native ARM64
Debug/Release ABI, owner/Core/renderer, GUI, static-CRT, MSIX, and portable-ZIP
coverage subsequently passed on 2026-08-04; the current record is in
[`implementation-status.md`](implementation-status.md).

## Native x64 observation — 2026-08-03

```text
Commit/build: uncommitted G13 working tree; build/windows-x64-release/inkpod.exe
Preset and native architecture: windows-x64-release, native AMD64
Windows build: 10.0.26200
Display language / IME: ja-JP / Microsoft Japanese IME installed
Display scale and monitor arrangement: apparent 100% single-display run;
  no native 200% display was available to Computer Use
Assistive technology and version: Computer Use UIA/MSAA client 26.727.51351

High contrast: Blocked — Alt+Shift+Print did not change the theme and launching
  Windows Settings timed out waiting for app approval. The original normal theme
  remained active.
200% DPI: Blocked — no 200% display was available and Windows Settings could not
  be opened by the automation session. No display setting was changed.
Keyboard only: Fail — Ctrl+Tab/Ctrl+Shift+Tab, Ctrl+F6, F6/Shift+F6, and the
  keyboard Window-menu route opened the expected document/group/pane targets.
  The three Reference Check AutoHide edge buttons had UIA names but could not be
  reached from the workspace with F6, Tab, or Shift+Tab.
Screen reader: Blocked — the equivalent UIA client read active and dirty tabs,
  dock splitters, panes, disabled controls, follow targets, and AutoHide names.
  A real running/completed Batch announcement and all disabled reasons were not
  observed, so this row is not reported as passed.
Japanese IME: Blocked — the Microsoft Japanese IME is installed, but Computer
  Use could not inject Kanji/Hiragana/Zenkaku mode keys. Alt+grave and the Windows
  on-screen keyboard remained alphanumeric, so composition/conversion/cancel was
  not exercised. The layer-name dialog was cancelled without changing the model.
Japanese resources: Pass — menus, pane titles, errors, and accessibility names
  were Japanese. About displayed `About Inkpod` and an English product
  description; the user confirmed this as expected behavior on 2026-08-03.
English resources: Known difference — no English display-language run was made;
  About is intentionally English in both language environments.
Faulted shutdown: Blocked — not repeated in this accessibility-only observation;
  retain the x64 Debug/Release automated fault evidence.
```

The test created four documents, two visible editor groups, and a second
workspace window, then returned the workspace to the Coloring preset. The dirty
label was observed as `無題セル 1 *`; Undo restored `保存済み`. No display or
theme setting was changed. The protected Windows on-screen-keyboard process
started for the blocked IME attempt could not be closed through Computer Use or
an elevated process-stop request and requires manual closure on this host.

Completion disposition (2026-08-03): the user accepted G13 as complete. The
failed or blocked rows above remain accurate evidence and are deferred to their
existing `In progress` requirements for separate resolution; they are not
reclassified as passed.
