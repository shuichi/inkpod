# Windows command inventory

This document summarizes the current production command surface and its enforced
ownership rules. `apps/windows/app/resource.h` and `apps/windows/app/app.rc` are
the resource sources of truth; this file is not a second registry.

## Current surface

- `resource.h` defines 385 unique `IDM_*` values.
- `app.rc` references 384 unique production command IDs through 391 actionable
  menu/control occurrences. `IDM_EFFECT_DUST` intentionally appears in two menus;
  six Layer commands appear in both the menu and dockable Layer pane. Every duplicate
  entry point shares one route and state.
- `IDM_BATCH_OPERATION_ADD` is the only defined but unreferenced value. It is a
  reserved aggregate ID; the UI uses 24 concrete `IDM_BATCH_ADD_*` commands.
- The main frame has no toolbar. Every user operation remains reachable through
  a production menu leaf; palette controls are alternate entry points that
  forward the same command ID on the UI thread.

## Routing and state ownership

`apps/windows/ui/main_window_runtime.cpp` routes commands through focused Batch,
document, edit, effects, document-pane, animation, selection/view, tool, color,
workspace, and application owners. Every production ID is handled exactly once.

`apps/windows/ui/command_state_catalog.inc` independently assigns the same 384
commands exactly one enabled/checked-state owner. Pure state providers feed one
cached result used by menus, shortcuts, and palette entry points; querying state
does not mutate Core, tools, previews, or documents.

Windows structural tests compare `app.rc`, route owners, state owners, and the
shortcut catalog. Missing, duplicate, or extra production ownership fails the
test, so this prose inventory does not need manual handler lists.

## Menu and shortcut contract

- Layer and Plane commands are nested under Cell; the Text/Instructions submenu
  shares the Layer owner; shortcut and System/Japanese/English language settings
  are under Edit > Settings.
- All 384 production commands have a command-unique, prefix-free sequence of one
  to four strokes, and every menu-leaf occurrence displays its active binding.
- Conventional file/edit commands retain standard Ctrl combinations. Frequent
  drawing, fill, eyedropper, selection, gradient, and airbrush tools use single
  strokes; motion FPS uses Ctrl+Alt combinations; remaining commands use
  categorized `Q`-led sequences.
- Palette colors `1`–`0` remain a fallback when no configured sequence matches.
- Internal lifecycle, queue, immutable accessor, snapshot-release, and diagnostic
  ABI functions are not exposed as artificial user commands.

## Numeric ranges

| Value range | Area | Defined |
|---|---|---:|
| 40000-40099 | File | 33 |
| 40100-40199 | Edit/history/clipboard | 13 |
| 40200-40299 | View/guide/grid | 38 |
| 40300-40399 | Raster/fill tools | 14 |
| 40400-40499 | Main-line/color plane switch | 2 |
| 40500-40599 | Color/palette/chart | 26 |
| 40600-40699 | Help/About | 5 |
| 40700-40799 | Original layer quick commands | 3 |
| 40800-40899 | Selection | 24 |
| 40900-40999 | Shortcut and language settings | 5 |
| 41000-41099 | Filters | 14 |
| 41100-41199 | Effects | 8 |
| 41200-41299 | Adjustment layers | 6 |
| 41300-41399 | Cell/paper/frame | 18 |
| 41400-41499 | Layer tree and Text/Instructions | 18 |
| 41500-41599 | Plane tree | 11 |
| 41600-41699 | Light table | 19 |
| 41700-41799 | Sequence/subpalette/motion | 21 |
| 41800-41899 | Vector | 23 |
| 41900-41999 | Window, pane-target, and Batch shell | 59 |
| 42000-42099 | Concrete Batch operations | 25 |

When commands change, update the resource definitions and ownership catalogs
first, keep the structural tests authoritative, and then update this summary if
the counts or user-visible grouping changed.
