# Subpalette first-load failure diagnosis

Date: 2026-08-28. Requirements: `SUBPALETTE-001`, `IO-003`.
Scope: original diagnosis and subsequent Windows publication fix.

## Cause before the fix

The first reference load has a circular dependency between source publication
and Canvas visibility:

1. `PresentSubpalettePane` derives `source_available` from the currently
   published `workspace.subpalette_info`, which is empty on first load.
2. `UpdateSubpalettePaneDialog` hides the Canvas when `source_available` is
   false. Starting a load preserves this empty published state.
3. After successful I/O, decode, candidate installation, Fit and snapshot
   preparation, `InstallSubpaletteSources` submits the candidate snapshot before
   publishing the candidate's information to the workspace.
4. `RendererHost::Impl::Submit` rejects snapshots for a surface whose published
   state is hidden or occluded. The first-load Canvas is still hidden.
5. `InstallSubpaletteSources` treats this rejection as `INVALID_STATE`, releases
   the successfully loaded candidate and displays `SubpaletteReadFailed`.
   The later source publication and `PresentSubpalettePane` call are never reached.

Relevant implementation:

- `apps/windows/ui/main_window_runtime.cpp`: `PresentSubpalettePane` and
  `InstallSubpaletteSources` (source availability, submission and publication order).
- `apps/windows/ui/panes/subpalette_pane.cpp`: `UpdateSubpalettePaneDialog`
  (`ShowWindow(canvas, source ? SW_SHOW : SW_HIDE)`).
- `apps/windows/renderer/canvas.cpp`: `RendererHost::Impl::Submit`
  (`!status->visible || status->occluded` rejection).

History identifies commit `ad6b424` (2026-08-27) as introducing the
submission-success requirement before source publication. Its parent published
the decoded catalog first and did not roll back the load when snapshot submission
was rejected. The diagnosis does not propose reverting the shared I/O migration.

## Reproduction and controls before the fix

The four user-specified PNGs were read in place, without conversion or modification.
All are 3508 by 2480, RGB8 (PNG color type 2). Total encoded size is 10,650,007
bytes; the four decoded RGBA8 payloads total 139,197,440 bytes, within the existing
budget. No user image is added to repository fixtures.

| Path exercised | Result |
| --- | --- |
| Rust production metadata/read/decode, all four originals | Success |
| Rust `SubpaletteCatalog::replace_loaded_images` and snapshot | Success, four images |
| Windows production `FileIoController` and `CoreHost`, each file, all four files, and folder | Six successful jobs; expected item/loaded counts, no failures; initial view/snapshot preparation succeeds |
| Real Subpalette dialog with an empty catalog, calling production `InstallSubpaletteSources` for the same six cases | Six failures; zero published images, Canvas and renderer visibility false, read-failure presentation |
| Control: first seed a real decoded catalog through `FileIoController`, present its successful info, then call unchanged production `InstallSubpaletteSources` for all four files | Success; four published images, Canvas visible, no error |

The Windows diagnostic links the existing x64 Release product objects and Rust
library and uses the real dialog resources, renderer, Core owner thread, I/O
controller and load-completion function. Diagnostic code supplies setup, unused
input callbacks and control seeding; the implementations are unchanged. Two
complete native runs reproduce the result and exit 0. The final run verifies
unchanged SHA-256 hashes for all four originals. File dialogs are bypassed by supplying
the exact paths to the same production load function; interactive file-dialog
operation and ARM64 are not claimed as verified in this investigation.

Local diagnostic sources and logs are under the ignored
`.inkpod-local/diagnostics/subpalette-png/` directory. The existing GUI-control
tool could not start because its sandbox initialization failed; the native
diagnostic therefore owns its separate test windows and closes them on completion.

## Impact and correction

This failure is about the empty reference viewport, not these PNG contents,
Japanese filenames or Dropbox metadata. The submission condition is format
independent; TIFF/TGA/BMP use the same failing first-load path, although the
provided reproduction set contains PNG only. A hidden/occluded pane at completion
can reach the same rejection after an existing image has loaded.

A correction must separate successful candidate preparation/publication from
whether a frame can be presented immediately. Preserve bounded preparation,
old/new lease accounting, failure/cancel/stale-target atomicity and the rule
that hidden surfaces do not perform unnecessary rendering. Do not simply remove
the renderer visibility guard or turn every failed submission into success.

The fix retains candidate decode and first-snapshot preparation before replacement.
After binding the candidate route, the Windows adapter synchronizes an actually
hidden or minimized Canvas with renderer visibility. Hiding a parent alone does
not guarantee a child `WM_SHOWWINDOW`. A matching surface that is hidden or
occluded defers snapshot submission without discarding the prepared Core catalog.
Other submission failures still restore the previous route and fail the load.
The staged snapshot is released on every path; no additional retained-snapshot
owner or cache limit is introduced.

Successful publication updates the pane and invalidates only its Canvas. The
existing visibility/viewport notification publishes the current snapshot on
first show or restoration. The renderer's hidden/occluded rejection remains.
Core, C ABI, persisted formats and decode/budget rules are unchanged.

`RunSubpalettePaneSmoke` now includes `RunSubpaletteFileLoadSmoke`, using PNGs
exported from the smoke document through the product path. It checks first file
and folder loads, replacement completed while hidden/minimized, successful
Present after show/restore, no additional Present during the hidden completion,
malformed/cancelled replacement preserving the old cache and selection, and
unchanged document metadata. The first-file assertion fails on the pre-fix
implementation and passes with the fix. User images are not permanent fixtures.

## Post-fix validation

- No-profile `cmake --preset windows-x64-release` and the matching full build
  pass without compiler warnings; all 46 matching CTests pass, including the
  English/Japanese application smokes, renderer/ABI checks, sequence performance
  and package payload checks.
- The native diagnostic is relinked against the corrected x64 Release product
  objects. Each original PNG, all four together, and the folder now succeed from
  an empty pane: six expected catalogs, six successful Present acknowledgements,
  no load-error presentation. Existing-source replacement also succeeds.
- The diagnostic forwards the real pane's viewport notifications to the product
  `ApplySubpaletteViewInput` handler because its isolated scaffold has no editor
  tabs to activate. Full application activation is covered by the separate
  English/Japanese product smokes, not claimed from this scaffold.
- The native run exits 0 and all four original SHA-256 hashes are unchanged.
  Its log is `.inkpod-local/diagnostics/subpalette-png/windows-fixed-result.log`.
- `cargo fmt --check` and `git diff --check` pass. Rust production code, C ABI and
  formats are unchanged, so Rust clippy/test/bench/doc were not rerun. Other
  Windows configurations and interactive file-dialog selection remain untested
  for this correction.
