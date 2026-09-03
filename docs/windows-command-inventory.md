# Windows command inventory

This document summarizes the current production command surface and its enforced
ownership rules. `apps/windows/app/resource.h` and `apps/windows/app/app.rc` are
the resource sources of truth; this file is not a second registry.

## Current surface

- `resource.h` defines 314 unique `IDM_*` values: 312 state-owned commands and
  two history-visualization range sentinels.
- The generated application menu references 304 unique production command IDs
  through 311 actionable menu/control occurrences. `IDM_EFFECT_DUST`
  intentionally appears in two menus;
  six Layer commands appear in both the menu and dockable Layer pane. Every duplicate
  entry point shares one route and state.
- The eight remaining state-owned commands are pane-local actions. Batch exposes
  exactly four `IDM_BATCH_ADD_*` candidates: Color Replace, Move to Color Plane,
  Masking and Erase. `IDM_BATCH_EXTRACT_PAIRS` retains the independent exact-color
  pair extraction action and is not an add-operation candidate.
- The main frame has no toolbar. Every user operation remains reachable through
  a production menu leaf; palette controls are alternate entry points that
  forward the same command ID on the UI thread.

File > Open is the sole native/raster entry point. Cut creation, metadata,
independent history, membership editing and instruction export have no menu,
shortcut, command-state or route entries. New Cell is a direct File command.

## Routing and state ownership

`apps/windows/ui/main_window_runtime.cpp` routes commands through focused Batch,
document, edit, effects, document-pane, animation, selection/view, tool, color,
workspace, and application owners. Every production ID is handled exactly once.

`apps/windows/ui/command_state_catalog.inc` independently assigns all 312
state-owned commands exactly one enabled/checked-state owner. Pure state providers feed one
cached result used by menus, shortcuts, and palette entry points; querying state
does not mutate Core, tools, previews, or documents.

Windows structural tests compare `app.rc`, route owners, state owners, and the
shortcut catalog. Missing, duplicate, or extra production ownership fails the
test, so this prose inventory does not need manual handler lists.

## Menu and shortcut contract

- Layer and Plane commands are nested under Cell. Named saved-mask commands are
  nested under Selection > Convert. Edit > Preferences opens the unified application and
  workspace settings dialog, including language and shortcut controls.
- Every non-separator top-level menu, submenu caption, and static selectable menu
  item has an English-derived mnemonic that is unique among its siblings. Japanese
  labels expose the same Latin access key as `名称(&X)`, English labels use `&Name`,
  and the recent-file entries use their visible `1`-`8` ordinals. Top-level keys
  are File F, Edit E, View V, Cell L, Selection S, Filter I, Tools T, Color C,
  Production P, Window W, and Help H. Pressing Alt therefore exposes a keyboard
  path from the menu bar to every production menu leaf without depending on a
  command shortcut. The dynamic Inkpod File Visualization menu exposes up to 64
  open native files as at most eight numeric page submenus; page keys `1`-`8`
  and item keys `1`-`8` provide an access-key path to every candidate.
- Window keeps Tool Palette, Tool Options, Color, Layer / Plane, Locator,
  Sequence, Light Table, Subpalette / Reference View, and Batch as direct checked
  pane toggles. Tab operations, editor-group operations, and cross-window view
  operations are regrouped under Views and Tabs, Editor Groups, and Windows
  submenus; Workspace retains its existing submenu. This is presentation-only
  grouping: command IDs, route/state owners, and pane visibility semantics do not
  change.
- The shortcut editor still projects the complete 312-command production catalog.
  A missing built-in binding means `Unassigned`, not a missing command: every
  catalog entry remains searchable and can receive primary and secondary bindings
  with the existing context/action/matching model.
- The built-in profile is deliberately sparse. It assigns only conventional
  Windows file/edit/help/preferences keys and VS Code-derived tab, editor-group,
  split, and window keys for commands with the same meaning. All
  drawing, fill, selection, palette, motion, pane, workspace, and other
  inkpod-specific commands are unassigned. There are no unmodified `Q`, `K`, or
  `A` defaults, categorized `Q` prefixes, palette `1`-`0`/Tab fallback, or motion
  FPS `Ctrl+Alt+number` fallback.
- The conventional set is New Cell `Ctrl+N`, Open `Ctrl+O`, Save `Ctrl+S`, Save
  As `Ctrl+Shift+S`, Undo `Ctrl+Z`, Redo primary/secondary
  `Ctrl+Y`/`Ctrl+Shift+Z`, Cut/Copy/Paste `Ctrl+X`/`Ctrl+C`/`Ctrl+V`, Select All
  `Ctrl+A`, Zoom In/Out
  `Ctrl+=`/`Ctrl+-`, Preferences `Ctrl+,`, Keyboard Shortcuts
  `Ctrl+K, Ctrl+S`, Help `F1`, and Exit `Alt+F4`.
- The VS Code-derived view set is previous tab primary/secondary
  `Ctrl+PageUp`/`Ctrl+Shift+Tab`, next tab primary/secondary
  `Ctrl+PageDown`/`Ctrl+Tab`, move tab left/right
  `Ctrl+Shift+PageUp`/`Ctrl+Shift+PageDown`, Close View primary/secondary
  `Ctrl+W`/`Ctrl+F4`, Split Right `Ctrl+\`, Move to Other Group
  `Ctrl+Alt+Right`, Focus Group 1/2 `Ctrl+1`/`Ctrl+2`, Next Editor Group
  `Ctrl+K, Ctrl+Right`, Close Group `Ctrl+K, W`, and New Window
  `Ctrl+Shift+N`. Duplicate View in New Window uses `Ctrl+K, O`. A comma
  separates successive strokes; it does not mean a simultaneous key press.
  Ctrl+Tab and Ctrl+Shift+Tab remain linear next/previous aliases in inkpod;
  this change does not add VS Code's MRU-tab picker.
- Assigned menu leaves display the active primary binding; unassigned leaves do
  not show a synthetic key sequence. Access keys remain visible independently of
  assignment. Existing custom/imported profiles are never rebuilt from the
  current built-in profile, so an explicit user binding (including `Q`, `K`, or
  `A`) to a surviving command survives default-table changes. Profiles naming
  deleted commands fail the existing unknown-command validation; no migration
  or silent reassignment is added. Invalid settings keep the default fallback
  and existing automatic-write protection.
- The Alt-letter key space belongs to menu mnemonics in the built-in profile.
  Native menu/system handling wins for bare Alt, unmodified F10, Alt+Space, and
  the top-level Alt-letter mnemonics. New recording/rebinding rejects those
  reserved combinations, but decode and validation preserve an existing custom
  v3 binding so it round-trips unchanged; it cannot fire while the native route
  owns the key. Alt+F4 is the one standard built-in exception and retains Exit
  semantics. Alt+Tab and other OS-global combinations are not intercepted by a
  shortcut hook; user bindings remain limited to key messages Windows actually
  delivers to the foreground application.
- Internal lifecycle, queue, immutable accessor, snapshot-release, and diagnostic
  ABI functions are not exposed as artificial user commands.

## Numeric ranges

| Value range | Area | Defined |
|---|---|---:|
| 40000-40099 | File | 21 |
| 40100-40199 | Edit/history/clipboard | 13 |
| 40200-40299 | View/guide/grid | 36 |
| 40300-40399 | Raster/fill tools | 16 |
| 40400-40499 | Main-line/color plane switch | 2 |
| 40500-40599 | Color/palette/chart | 26 |
| 40600-40699 | Help/About | 6 |
| 40700-40799 | Original layer quick commands | 3 |
| 40800-40899 | Selection | 26 |
| 40900-40999 | Preferences | 2 |
| 41000-41099 | Filters | 14 |
| 41100-41199 | Effects | 8 |
| 41200-41299 | Reserved | 0 |
| 41300-41399 | Cell/paper/frame | 15 |
| 41400-41499 | Layer tree | 9 |
| 41500-41599 | Plane tree | 11 |
| 41600-41699 | Light table | 19 |
| 41700-41799 | Sequence/subpalette/motion | 21 |
| 41800-41899 | Reserved surviving commands | 6 |
| 41900-41999 | Window, pane-target, and Batch shell | 55 |
| 42000-42099 | Four Batch candidates plus color-pair extraction | 5 |

When commands change, update the resource definitions and ownership catalogs
first, keep the structural tests authoritative, and then update this summary if
the counts or user-visible grouping changed.
