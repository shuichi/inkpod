# Rust Core refactoring baseline

This document is the historical M0 input for the completed Core refactoring. It
inventories the observable contract and mutation paths as of 2026-07-30. It is
not a current product status log. Current behavior lives in
[`PROMPT.md`](../PROMPT.md), development boundaries in
[`AGENTS.md`](../AGENTS.md), and the former milestone plan remains in Git
history.

The inventory is intentionally expressed in terms of public operations and
responsibility owners instead of line numbers, so later source movement does not
make it stale. M1 uses the observation design below. M3 must update the migration
assignment when it removes or adds an inventoried path.

## Specification and test baseline

The relevant product contracts are `DOC-002`, `DOC-003`, `VIEW-001` through
`VIEW-003`, `HIST-001`, the paint/fill/selection/transform requirements,
`LT-001`, `LT-002`, `SEQ-001`, `SEQ-002`, the filter/effect/Batch/vector
requirements, and `IO-001`. In particular:

- document and view revisions are separate;
- a successful document edit is one Undo unit, while a no-op or invalid edit is
  not a history unit;
- preview and long-running work commits only on explicit success;
- cancel, failure, and stale revision leave committed document, history, dirty
  state, and output unchanged;
- normal save advances the normal savepoint, while autosave, recovery save, and
  export do not;
- stable IDs, public Rust API, C ABI v2, and `.inkpod` v2 remain unchanged by
  this refactoring plan.

`cargo test --package inkpod-core --all-features -- --list` currently reports 83
tests and no Core doctests or benchmarks. Their boundary classification is:

| Target | Class | Count | Current contents |
| --- | --- | ---: | --- |
| `src/lib.rs` unit target | Private invariants colocated with implementation | 10 | Batch selector/limits, document active-plane validity, effect pressure, view-only color sampling, RGBA16 paste, selection algebra, vector bounds and ID-overflow behavior |
| `tests/architecture.rs` | Architecture guards | 6 | OS independence, unknown legacy codecs, test gating/naming, small roots, and recursive CMake tracking |
| `tests/contracts/main.rs` | Public workflow integration tests | 66 | animation 8, Batch 9, document/selection/view 13, effects 8, foundation/paint/fill/snapshot 13, history/stroke 7, vector 8 |
| `tests/resilience.rs` | Public resilience integration test | 1 | Corrupt open is non-destructive to the current document and files |

The placement policy is already satisfied: private-field/helper assertions live
in their implementation file, while behavior visible through public Core APIs
lives in the `contracts` or `resilience` integration targets. M1 extends the
public workflow target and adds only truly local helper invariants beside their
implementation.

The existing scenario tests cover representative success, no-op, invalid,
cancel, Undo/Redo, revision, dirty/savepoint, and non-destructive failure paths.
They do not yet provide one fixed-seed state-machine runner, a shared observation
record, or replayable operation sequences. Those are M1 work rather than M0
production changes.

## Public mutation classification

Only APIs on `Core` are classified here. Mutating value-object helpers such as
`BatchGraph` configuration do not own the open Core document and are outside the
transaction inventory.

| Class | Public operations | State contract |
| --- | --- | --- |
| Document replacement/lifecycle | `new_cell`, `new_cell_with_uuid`, `import_common_raster`, `open`, `open_recovery`, `revert`, `sequence_step`, `sequence_activate`, `light_table_swap_with_active` | Replaces the open document, advances document revision, resets history/view and path/recovery state according to the operation; not an ordinary Undo entry |
| Immediate document edit: tree and metadata | layer/plane create, duplicate, delete, reorder, properties, convert, merge; guide add/move/delete and `set_grid`; `update_paper_frames`; palette and main-line color; light-table set/item metadata; adjustment create/update | Success is one history unit; semantic no-op is stable; invalid input does not commit |
| Immediate document edit: raster/selection/vector | fill, selection authoring/invert/clear/resize/color/layer conversion, selected-content clear, destructive mirror/rotate/resize, immediate filters/effects, vector add/fill/erase/connect/width and raster/vector conversion | Stages or records changed content, advances document revision once on success, and supports Undo/Redo |
| History navigation | `undo`, `redo`, `jump_history`, `revert_active_plane_selection` | Moves or creates history state explicitly; invalid targets and empty/no-change partial revert do not move history |
| View/display-only | `set_active_plane`, `set_active_node`, `apply_view`, `create_view`, `close_view`, `apply_view_for`, shortcut mutation APIs, `set_color_check` | Does not advance document revision/history/dirty. Semantic view changes advance the applicable view revision; active-node and shortcut state currently have no public revision |
| Observation/cache-only mutation | `build_snapshot`, `build_snapshot_for`, `dispatch` | May update render cache/counters or return an outcome but does not edit the document or history. `dispatch` currently accepts the supplied no-op command count only |
| Preview/session transaction | stroke begin/append/end/cancel and one-shot stroke; filter/dust preview begin/update/apply/cancel; floating paste begin/transform/commit/cancel | Begin/update changes preview/session state only. Explicit end/apply/commit creates at most one history unit. Cancel restores the committed base observation |
| Long-running/cancellable | fill-with-cancel variants, filter/dust progress variants, `batch_execute`; bounded vector rasterize/vectorize and dense transforms are potentially long-running even where the current API has no cancel callback | Work is computed before commit or in a private working Core. Cancel, failure, and stale base must not publish partial document or output state |
| Persistence/export | `save`, `autosave`, `open`, `open_recovery`, `revert`, common-raster and sequence export, `batch_execute` output | Normal save changes savepoint/path only after atomic save succeeds. Autosave/export do not advance the normal savepoint. Open failure preserves the current document |
| Sequence/transient workflow | `set_sequence`, `import_sequence`, sequence activation/step, `set_subpalette_cell`, motion-check start/step/pause/stop | Sequence selection and motion/subpalette state are outside document history. Activating another clean cell replaces the document; dirty-state rejection leaves both active cell and document unchanged |

APIs that appear in more than one row use the stricter contract. For example,
sequence activation is transient workflow state plus document replacement, and
Batch is cancellable persistence performed against private working Core values.

## Current state-transition semantics

| State | Current owner and transition | Public observation | Required M1 comparison |
| --- | --- | --- | --- |
| Document revision | `Core::document_revision`; changed document commits, pixel commits, history movement, new/open/import, sequence activation, and light-table swap advance it. No-op, invalid, cancelled work, normal save, autosave, export, and view-only edits do not | `DocumentInfo::document_revision`, `DispatchOutcome::revision`, snapshot revision outside previews | Exact before/after and cross-Core equality |
| View revision | Main `ViewState` and each secondary view; changed pan/zoom/fit/resize/flip/display toggles advance only the selected view. A semantic no-op does not. New/open resets the main view while advancing its revision | `view_state`, `ViewState::revision`, `DocumentInfo::view_revision`, snapshot view | Exact; document revision/history/dirty must remain fixed for view-only operations |
| History cursor/state | `history`, `history_cursor`, `current_state`, `next_state`; commit truncates a redo branch, appends one entry, and selects its after-state. Undo/Redo/jump apply stored values and move the cursor | `history_entries`, `history_cursor`, `DocumentInfo::{can_undo,can_redo}` | Entry label/order/applied flags, cursor, and Undo/Redo round-trip |
| Dirty/savepoint | `savepoint != Some(current_state)`. New/open/import/sequence activation and light-table swap establish a clean initial state; recovery open is dirty/pathless. Normal save records the current state; autosave/export do not. Undo back to a saved state becomes clean | `DocumentInfo::{dirty,recovered}` plus save/open results | Dirty transitions and save/autosave/export/revert behavior; the private numeric state token is not part of cross-Core observation |
| Render cache | `render_cache` plus `next_render_tile_revision`. Whole-document replacement, document-history replacement, main-line color, alpha/color-check display changes, and preview begin/update/cancel clear the cache. Pixel edits normally rely on tile source revisions and lazy rebuild | snapshot tile IDs/origins/dimensions/pixels/tile revisions | Semantic content across Cores; tile revision/reuse only in a same-Core cache extension with an identical snapshot schedule |
| Stable ID allocator | `next_id` supplies document/layer/plane/guide/light-table/vector IDs; opened files raise it above persisted IDs. Some operations reserve eagerly with `allocate_id`, while duplicate/vector/adjustment paths stage a local next value and publish it after commit | IDs returned by operations and exposed by `DocumentInfo`, topology, light-table, and vector queries | Uniqueness/reference integrity. M1 must explicitly observe whether a failed ID-creating operation consumes values by following it with a successful creation; M3 must not silently change the result |
| View ID allocator | `next_view_id` is checked and IDs are removed but not reused by `close_view` | returned view ID and secondary-view API results | Same sequence yields the same IDs; invalid close does not affect later views |
| Preview revision | `next_preview_revision` is separate from document revision and is consumed by stroke/filter/dust preview work, including some failed attempts | preview info and preview snapshot revision | Compare preview sessions with identical call schedules; cancel must restore the committed semantic observation even though the private counter can advance |

There is no public stale-revision argument on ordinary synchronous APIs. Current
stale checks occur inside long-running effect helpers by capturing the base
document revision and validating it before commit, but safe single-writer Core
usage cannot mutate that same Core through a progress callback while `&mut self`
is borrowed. M1 therefore exercises public cancellation/failure atomicity and
does not add a test-only public stale hook. M3 tests stale commit rejection as a
private transaction invariant beside the transaction implementation.

## `CoreObservation` design for M1

`CoreObservation` is test code built only from public Rust APIs. It is split so
tests request only the feature data relevant to their workflow:

```text
CoreObservation
  common: CommonObservation
  extensions: requested FeatureObservation values

CommonObservation
  document: DocumentInfo
  topology: Vec<LayerInfo>
  history: { entries: Vec<HistoryEntryInfo>, cursor: usize }
  main_view: ViewState
  snapshot: SemanticSnapshotObservation

FeatureObservation
  Selection { bounds }
  Color { palette, main_line_color }
  Guides { guides, grid }
  LightTable { sets, active_items }
  Vector { paths, fills, rendered_segments, rendered_fills }
  Sequence { cells, selected subpalette sample or motion frame when applicable }
  Pixels { explicitly requested `(plane, x, y, value)` samples }
  Cache { tile_id/origin/tile_revision before and after a same-Core rebuild }
```

`SemanticSnapshotObservation` records snapshot revision, feature flags, view,
document dimensions, guides/grid, and each tile's ID, origin, dimensions,
stride, and a fixed-algorithm checksum of `pixels()`. It also records all public
vector segment/fill data. M1 must use a locally specified checksum algorithm,
not `DefaultHasher`, so compiler/library changes cannot alter replay output.

Comparison modes are deliberately separate:

1. **Determinism comparison** applies the same operation and observation
   schedule to two Cores and compares common plus requested semantic extensions.
   Stable IDs and document/view revisions are included. Cache allocation
   counters are excluded unless both schedules explicitly request the cache
   extension.
2. **Atomicity comparison** captures one Core before and after an invalid,
   cancelled, stale, or no-op operation. It compares the complete common
   observation and requested feature extension. Expected session cleanup (for
   example, a cancelled preview disappearing) is asserted separately.
3. **Undo/Redo comparison** compares semantic document/feature content with the
   pre-edit and post-edit observations while asserting the expected newer
   document revision and changed history cursor separately. Revision numbers are
   therefore not erased from the record merely to make round-trip equality easy.
4. **Cache comparison** is same-Core only and asserts unchanged tile revisions
   for reused tiles and changed revisions/content only for invalidated tiles.

The common record intentionally does not expose private fields through a test
bridge. Dirty/savepoint is observed through `DocumentInfo::dirty`; active
layer/plane relationships through public topology and operation results; exact
selection content through semantic snapshot pixels plus requested bounds; and
preview state through public preview/stroke methods and snapshot revision.

The current public surface does not expose the exact generic active layer/plane
IDs, numeric savepoint/history-state token, next stable ID, filter-preview-active
flag, or floating-selection-active flag. M1 keeps expected active/session IDs in
its abstract model and verifies their effect through existing public operations;
it must not add accessors solely for tests. Allocator behavior is inferred by a
subsequent successful ID-returning operation, and session existence by the
documented success/error class of the corresponding public method.

Every generated operation in M1 carries an expected class (`Success`, `NoOp`,
`Invalid`, or `Cancel`) and a replay representation containing seed, case,
step, and public arguments. That keeps failure atomicity assertions independent
from error text and makes a failing sequence reproducible.

## M3 commit-path inventory and migration assignment

### Ordinary before/after document pairs

These are the current callers of `commit_document_edit` or
`commit_document_edit_with_revision`. Each is assigned to exactly one M3 wave or
to an explicit M3 exclusion below.

| Assignment | Current call sites |
| --- | --- |
| Wave 3.1: layer/plane structure | `create_layer`, `duplicate_layer`, `delete_layer`, `reorder_layer`, `set_layer_properties`, `create_plane`, `duplicate_plane`, `delete_plane`, `reorder_plane`, `set_plane_properties` |
| Wave 3.2: conversion/merge | `convert_plane`, `merge_plane_into_below`, `convert_layer`, `merge_layer_into_below` |
| Wave 3.2: simple light-table edits | `light_table_set_global_opacity`, `light_table_create_set`, `light_table_duplicate_set`, `light_table_delete_set`, `light_table_rename_set`, `light_table_reorder_set`, `light_table_set_active`, `light_table_add_item`, `light_table_update_item_properties`, `light_table_update_item`, `light_table_remove_item`, `light_table_reorder_item`. `light_table_add_common_raster` delegates its commit to `light_table_add_item` |
| Wave 3.3: guide/grid and color metadata | `add_guide`, `move_guide`, `delete_guide`, `set_grid`, `replace_palette`, `set_main_line_color` |
| Wave 3.4: immediate selection/document transform | `apply_selection`, `invert_selection`, `clear_selection`, `resize_selection`, `select_color`, `selection_to_layer`, `selection_from_layer`, `clear_selected_content`, `mirror_document`, `rotate_document`, `resize_document`, `update_paper_frames` |
| Wave 3.5: immediate effect/adjustment/vector edits | `apply_masked_raster_operation`, `apply_raster_operation`, `apply_blur_tool_mask_to_plane` (and their immediate public effect wrappers), `create_adjustment_layer`, `update_adjustment_layer`, `vector_add_path`, `vector_add_fill`, `vector_erase`, `vector_connect`, `vector_correct_width` |
| Explicitly excluded: preview/session | `apply_filter_preview`; stroke `end_stroke`; floating `commit_floating`. Preview base/working ownership requires a later session-transaction design |
| Explicitly excluded: cancellable or potentially long-running | `apply_last_filter_with_progress`, `apply_dust_removal_to_plane`, Batch `apply_color_replacement` and `apply_separation`, `rasterize_vector_layer_to_document`, `vectorize_raster_plane`, and fill pixel commits |
| Explicitly excluded: external reload/replacement/history | `light_table_reload_common_raster`, `light_table_swap_with_active`, `revert_active_plane_selection`, and document lifecycle/open/import/sequence replacement paths |

The three helpers in `effects/operations.rs` are counted once even though many
public filter/effect entry points delegate to them. Conversely, specialized
pixel/history paths are listed below even when they do not clone a whole
`CellDocument`.

### Specialized history commits

| Current call site | M3 assignment |
| --- | --- |
| `commit_document_edit_with_revision` -> `commit_history_change(Document)` | Replaced by the M3 transaction commit implementation |
| fill in `paint.rs` -> `commit_pixel_history` | Excluded cancellable operation |
| `replace_palette` -> `commit_history_change(Palette)` | Wave 3.3 |
| `set_main_line_color` -> `commit_history_change(MainLineColor)` | Wave 3.3 |
| `clear_selected_content` -> `commit_pixel_history` | Wave 3.4 |
| `revert_active_plane_selection` -> `commit_pixel_history` | Excluded persistence/history hybrid |
| `end_stroke` -> `commit_pixel_history` | Excluded session transaction |
| `commit_floating` -> `commit_pixel_history` | Excluded session transaction |

### Direct document replacement and cache invalidation

| Current owner | Direct operation | M3 treatment |
| --- | --- | --- |
| `core.rs` | `new_cell_with_uuid` assigns a new document; `commit_document_edit_with_revision` publishes an ordinary edit; `apply_history_values` replaces a stored document during Undo/Redo | Lifecycle excluded; ordinary publish becomes the transaction boundary; history application remains history-owned |
| `persistence.rs` | `open` and `open_recovery` assign decoded documents | Excluded persistence lifecycle |
| `animation/io.rs` | `import_common_raster` assigns an imported document | Excluded import lifecycle |
| `animation/sequence_operations.rs` | `sequence_activate` assigns the selected cell document | Excluded sequence lifecycle |
| `animation/light_table_operations.rs` | `light_table_swap_with_active` assigns the selected reference as the new document | Excluded swap lifecycle |

Full `render_cache.clear()` calls belong to the same ordinary transaction or
lifecycle owner above, plus the following display/session owners:

- filter/dust preview begin, update, and cancel;
- main-line color and color-check display changes;
- alpha-view changes;
- main-line/document history application.

`build_snapshot` also drops the locally taken cache when no document exists;
that observation/cache-only branch is excluded from M3 document transactions.

M3 moves only the ordinary document-edit invalidation into its transaction
commit. It must not absorb preview, view/display, lifecycle, or history replay
invalidation. Pixel-history paths continue to use tile source revisions unless
their assigned wave deliberately migrates the whole operation and M1 cache
observations prove equivalent behavior.

## Verification gates for later milestones

Every milestone ends with:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --package inkpod-core --all-features
git diff --check
```

M2 and later also run the Core benchmark quick mode. M6 and later also build
Core rustdoc with `RUSTDOCFLAGS="-D warnings"`. A milestone that changes Rust
source composition or CMake tracked inputs additionally runs the available
CMake configure/build and platform-independent CTest; Windows x64 Debug/Release
build and CTest remain mandatory in Windows CI when the current host cannot run
them.

M0 adds no dependency and changes no production code, public API, C ABI, file
format, architecture, or requirement status.
