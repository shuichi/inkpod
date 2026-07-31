# GUI modernization baseline

This document freezes the pre-G1 regression baseline required by `GUI.md` G0.
It describes the implementation that must remain usable while G1 and later
milestones change ownership and presentation. It is not the target architecture
and it is not a chronological verification log.

## Scope and sources of truth

| Item | Baseline source |
|---|---|
| Product behavior | [`../PROMPT.md`](../PROMPT.md) |
| Milestone order and gates | [`../GUI.md`](../GUI.md) |
| Current ownership/thread structure | [`architecture.md`](architecture.md) |
| Requirement state | [`compatibility.md`](compatibility.md) |
| Current verification summary | [`implementation-status.md`](implementation-status.md) |

The baseline was frozen on 2026-07-31 before any G1 identity or
`CommandContext` implementation. If later work changes a count or behavior, the
new current value belongs in the relevant source/test/status document; this file
continues to record the G0 comparison point.

## Pre-G1 ownership baseline

| Area | G0 baseline | Target difference tracked by `GUI.md` |
|---|---|---|
| Application/window | One `Application` creates one main `HWND`; stack-owned `AppContext` is the private composition root | G2 introduces process-owned `ApplicationHost` and `WorkspaceWindow` ownership; G10 makes windows plural |
| Document | One `DocumentShellState` and one `CoreEngine` own one `InkpodCore` binding | G2 separates `DocumentSession`/`DocumentView`; G3 makes `CoreHost` session-keyed |
| Views/tabs | Same-document Core views can appear as tabs, but opening a document replaces the current document | G5 adds multiple document sessions/tabs; G6 adds two visible editor groups |
| Canvas/renderer | One Canvas `HWND`, one swap chain, and one renderer thread | G4 introduces one shared `RendererHost` and a `CanvasSurface` per visible editor group |
| Workspace | Four primary `WS_CHILD` panes use fixed geometry plus bounded widths/ratios, hide/reset/save/restore/mirror | G7-G9 introduce bounded dock zones, pane descriptors/scopes, named workspaces, and generalized persistence |
| Command target | Route/state ownership is unique, but handlers use the single `AppContext` and current active state | G1 captures strong frontend IDs, generation, and immutable `CommandContext` at issue time |

The retained one-window workflow is therefore a compatibility baseline, not a
claim that G1-G13 behavior is already implemented.

## Command surface baseline

The resources and structural tests enforce the following current surface:

- `apps/windows/app/resource.h` defines 282 unique `IDM_*` values.
- `apps/windows/app/app.rc` references 281 production command IDs in 288
  actionable menu/control occurrences.
- `IDM_BATCH_OPERATION_ADD` is the one reserved, defined but unreferenced ID.
- All 281 production commands have exactly one route owner, one command-state
  owner, and a configured one-to-four-stroke shortcut. Every user operation is
  reachable from a menu leaf; the main frame has no toolbar.

`tests/verify_windows_command_routes.cmake`,
`tests/verify_windows_command_states.cmake`, the native application smoke, and
[`windows-command-inventory.md`](windows-command-inventory.md) are the
authoritative evidence. Later command additions must update resources, routing,
state, shortcuts, tests, and the inventory together.

## C ABI baseline

- ABI version: 2.
- Public surface: 161 functions declared by
  `include/inkpod/core_ffi.h` and exported by `rust/inkpod-ffi`.
- `ffi_contract_public_surface_matches_header_and_every_function_has_a_test_reference`
  enforces exact header/export parity and a direct Rust or C++ contract-test
  reference for every function.
- C11/C++20 layout compilation, integrated `--abi-smoke-test`, validation,
  ownership, error-copy, task, snapshot, and double-release paths are retained.

G0 does not change the ABI. GUI-only milestones must prefer frontend ownership
changes; if a later milestone proves an ABI change necessary, the header, Rust
implementation, ownership/thread documentation, and ABI tests must land in the
same milestone.

## Native regression checklist

| Contract | Reproducible evidence at G0 |
|---|---|
| One main window and canonical panes | `RunDrawingPersistenceSmoke` resets the workspace, requires the four primary panes to be child windows of the main window, checks layout/mirroring/visibility, tabs, status, and menu/pane command-state agreement |
| Menu completeness and target state | `inkpod_windows_command_routes`, `inkpod_windows_command_state_ownership`, `inkpod_windows_command_state`, and `RunDrawingPersistenceSmoke` verify the 281-command route/state/shortcut surface |
| Open/edit/Undo/Redo/save/reopen | `RunDrawingPersistenceSmoke` draws on both planes, verifies preview versus commit, capture-loss cancel, Undo/Redo, atomic save, clean savepoint, reopen, metadata/checksum round-trip, partial revert, and history navigation |
| No-op/invalid/cancel | The native smoke rejects unavailable Undo and vector commands, reports missing sequence/vector state, verifies dialog validation/cancel, cancels a captured stroke without revision/checksum change, and preserves prior state on failed paths |
| Recovery | `RunPaintingRecoverySmoke` and the drawing smoke cover autosave deferral during a stroke, recovery open, dirty/pathless recovery state, and separation from the normal savepoint |
| Resize and DPI | `windows_workspace_layout` covers pure geometry at multiple DPI values; the native smoke resizes the main window and verifies that a Canvas `WM_DPICHANGED_AFTERPARENT` does not change document/device bounds |
| Device lost | `RunDrawingPersistenceSmoke` sends `kCanvasSimulateDeviceLoss`, requires successful GPU-resource recovery, and renders again without replacing the Core document |
| Thread/queue/shutdown boundary | The native smoke requires distinct UI, Core, and renderer thread IDs; `inkpod_windows_frontend_boundaries`, `inkpod_abi_smoke`, and application shutdown exercise the value/ownership boundaries and ordered teardown |

## G0 verification gate

The exact Windows preset is `windows-x64-debug`. G0 is complete only when these
commands succeed in the G0 worktree and their result is recorded below:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cmake --preset windows-x64-debug
cmake --build --preset windows-x64-debug
ctest --preset windows-x64-debug
git diff --check
```

| Verification | G0 result |
|---|---|
| Rust format/clippy/workspace tests | Passed: format clean, zero-warning all-target/all-feature clippy, 177 tests plus 1 doctest |
| Windows x64 Debug configure/build | Passed with MSVC 19.51 strict warnings; unsigned x64 MSIX assembly also passed |
| Windows x64 Debug CTest | Passed 11/11, including route/state/boundary, ABI, native application/DPI/device-lost, and MSIX payload smoke |
| Whitespace check | Passed with `git diff --check` |

ARM64 and Release are not G0 gates. They remain later milestone gates exactly as
listed in `GUI.md`.

## Known differences at the G0 boundary

- Strong frontend IDs and immutable `CommandContext` do not yet exist; G1 is the
  next milestone and no G2 ownership work may start first.
- `AppContext`, one `CoreEngine`/Core handle, and one Canvas/render thread still
  define the current single-window runtime.
- Multiple document sessions, two visible editor groups, constrained docking,
  named workspaces, multiple top-level windows, tab drag/tear-out, single logical
  instance activation, and all-session recovery are not baseline features.
- Current same-document tabs share document/history as intended, but opening a
  different file still replaces the current shell document.

These differences are tracked by `WIN-002`, `VIEW-004`, `WORKSPACE-001`,
`WORKSPACE-002`, and `SESSION-001` in `compatibility.md`; none may be marked
`Verified` without its milestone's production route and tests.
