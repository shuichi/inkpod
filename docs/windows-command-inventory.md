# Windows command ownership

This is an index of current command owners, not a second command registry.
Product menu/shortcut behavior belongs to [SPEC](../SPEC.md); detailed owner and
thread boundaries belong to [architecture.md](architecture.md).

## Sources of truth

| Concern | Authoritative source |
| --- | --- |
| Stable numeric command IDs and reserved ranges | `apps/windows/app/resource.h` |
| Generated localized menu resources | `apps/windows/app/app.rc` and the localization generator |
| Enabled/checked-state ownership | `apps/windows/ui/command_state_catalog.inc` |
| Production dispatch | Feature owners reached from `apps/windows/ui/main_window_runtime.cpp` |
| User-editable bindings | Windows shortcut command catalog and Core shortcut contracts |

The structural tests compare resource IDs, route owners, state owners and the
shortcut catalog. Every production ID has exactly one execution owner and one
state owner; multiple menu/pane entry points share that state and route. Keep
counts in these executable comparisons instead of copying them into prose.

## Routing contract

- Commands capture immutable workspace/session/view/pane/job identity and generation.
  Query and execution resolve the same target; query is side-effect-free and stale
  commands never fall back to the newly active document.
- Batch, document, edit, effects, document-pane, animation, selection/view, tool,
  color, workspace and application owners translate inputs into Core commands.
  Controllers do not change another controller's private state.
- Menu, shortcut, context menu and pane actions share command ID and state. The
  main frame has no toolbar. Internal lifecycle/query/release/diagnostic ABI
  functions do not become artificial product commands.
- File > Open is the native/raster entry. Cut management and instruction export
  have no routes. Batch exposes four add-operation candidates; exact-color pair
  extraction is a separate action.
- The shortcut editor includes all production commands even when unassigned.
  SPEC defines sparse built-ins, preserved custom bindings, native reserved keys,
  mnemonic uniqueness and dynamic History Visualization pages.

When commands change, update resources and ownership catalogs together and run
structural/localization/shortcut and affected product tests. Do not add handler
lists or historical command counts here.
