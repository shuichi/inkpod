# Windows command inventory

This inventory is the R0 routing baseline for the Windows frontend. It records
production menu command ownership before feature controllers are introduced.
It is not a new command registry: `apps/windows/app/resource.h` and
`apps/windows/app/app.rc` remain the resource sources of truth.

## Baseline (2026-07-26)

- `resource.h` defines 274 unique `IDM_*` values.
- `app.rc` references 273 unique production `IDM_*` values.
- `MainWindowProcedure` has exactly 273 unique production `case IDM_*` labels,
  with no duplicate case and no production resource command missing.
- `IDM_BATCH_OPERATION_ADD` is the only defined-but-unreferenced value. It is a
  reserved aggregate ID; the UI uses the 24 concrete `IDM_BATCH_ADD_*` commands.
- Batch palette buttons forward to the same production IDs synchronously on the
  UI thread. They are alternate entry points, not additional command owners.

## Current menu and shortcut surface (2026-07-26)

- The production menu contains 274 actionable leaf occurrences representing all
  273 unique production commands; `IDM_EFFECT_DUST` intentionally appears in two
  relevant submenus and both occurrences use the same command owner/state.
- Layer and Plane commands are nested under Cell, while shortcut settings are
  under Edit > Settings. This reduces top-level scanning without changing any
  command ID or Core route.
- `command_state_catalog.inc` remains the one-owner catalog and now also seeds a
  complete default shortcut table. All 273 commands have one command-unique,
  prefix-free sequence of one to four strokes. Runtime menu labels display the
  active sequence on every leaf occurrence.
- Conventional file/edit commands retain standard Ctrl combinations. The main
  drawing, fill, eyedropper, selection, gradient, and airbrush tools use single
  strokes; motion FPS keeps its established Ctrl+Alt combinations; remaining
  commands use categorized `Q`-led sequences. Palette colors `1`–`0` remain a
  fallback when no configured sequence matches.
- The main frame creates no toolbar. All user-invocable operations must remain
  reachable through a production menu leaf; optional palette controls are only
  alternate entry points to the same ID. Internal lifecycle, immutable accessor,
  queue, snapshot release, and diagnostic ABI functions are not artificial user
  commands.
- Windows tests compare `app.rc`, command routes, state owners, and shortcut
  bindings; real application smoke recursively verifies every leaf has a visible
  assignment and rejects a toolbar child.

| Value range | Area | Defined | Production owner at R0 |
|---|---|---:|---|
| 40000-40099 | File | 11 | `MainWindowProcedure` |
| 40100-40199 | Edit/history/clipboard | 13 | `MainWindowProcedure` |
| 40200-40299 | View/guide/grid | 20 | `MainWindowProcedure` |
| 40300-40399 | Raster/fill tools | 8 | `MainWindowProcedure` |
| 40400-40499 | Main-line/color plane switch | 2 | `MainWindowProcedure` |
| 40500-40599 | Color/palette/chart | 26 | `MainWindowProcedure` |
| 40600-40699 | Help/About | 1 | `MainWindowProcedure` |
| 40700-40799 | Original layer quick commands | 3 | `MainWindowProcedure` |
| 40800-40899 | Selection | 22 | `MainWindowProcedure` |
| 40900-40999 | Shortcut settings | 2 | `MainWindowProcedure` |
| 41000-41099 | Filters | 14 | `MainWindowProcedure` |
| 41100-41199 | Effects | 8 | `MainWindowProcedure` |
| 41200-41299 | Adjustment layers | 6 | `MainWindowProcedure` |
| 41300-41399 | Cell/paper/frame | 12 | `MainWindowProcedure` |
| 41400-41499 | Layer tree | 10 | `MainWindowProcedure` |
| 41500-41599 | Plane tree | 11 | `MainWindowProcedure` |
| 41600-41699 | Light table | 16 | `MainWindowProcedure` |
| 41700-41799 | Sequence/subpalette/motion | 20 | `MainWindowProcedure` |
| 41800-41899 | Vector | 21 | `MainWindowProcedure` |
| 41900-41999 | Batch shell | 24 | `MainWindowProcedure` for 23 production IDs; one reserved ID as noted above |
| 42000-42099 | Concrete Batch operations | 24 | `MainWindowProcedure` |

Later routing steps must retain exactly one owner for every production ID. A
controller may receive a range, but the aggregate set must still equal the 273
IDs referenced by `app.rc`; `IDM_BATCH_OPERATION_ADD` remains excluded until a
real resource entry and handler are intentionally added by a separate feature
change.

## Structural size baseline

Before R1 source extraction, `main.cpp` had 16,311 physical lines and 701,656
bytes. `UpdateMenuState` occupied 608 lines. `MainWindowProcedure` occupied
3,934 lines, contained 304 total `case` labels, and contained the 273 production
command labels above. These measurements are comparison aids, not correctness
gates.
