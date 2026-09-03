# InkScript traceability

This reference connects [SPEC requirements](../SPEC.md), the separate normative
[language contract](../INKSCRIPT.md), machine-readable registries and executable
evidence. It is not a milestone prompt. Current signatures come from
[catalog-v7.json](../schemas/inkscript/catalog-v7.json); the generated presentation is
[inkscript-command-reference.md](inkscript-command-reference.md).

## Current contract

- Registry schema/language/file v2, catalog/owner v7, replay epoch 29, native v34
  and ABI v34 align. The public catalog contains 74 commands. Retired assignments
  remain tombstones; older catalog/owner resources are rejected.
- Grammar/section changes require an explicit file-version decision. Closed
  semantic entities and references require an explicit catalog/replay rebaseline.
- Exact-source equivalence compares canonical state/pixel digests, ID high-watermarks,
  typed result roles/output ordinals, pre/post digests, stable input/output/asset
  roles and canonical invocations. Rebound execution is deterministic only after
  explicit replacement of every external strict selector; source UUID/raw ID/state
  equality is not promised.
- Continuous Fill lowers one seed to one step and produces zero through N Commits.
  A contiguous non-semantic `editor_group` is the lossless 1:N source link; it
  cannot change execution or Commit boundaries.
- `.inkbatch` v5 remains an independent closed product format. No `.inkscript`
  product filter, clipboard, command or authoring UI is implied by private APIs.
  Future exposure requires a new explicit catalog/file-version decision and
  input/output authority, replay, state/pixel/history/ID/savepoint/failure parity.
- The quick performance contract is approved; full implementation remains reserved
  for M36. [The benchmark baseline](core-benchmark-baseline.md) owns its workload,
  counters, samples, environment and envelope. No milestone text here authorizes
  resuming paused work or changing those gates.

## Batch parity matrix

| Product requirement | Related script requirements | Contract to verify |
| --- | --- | --- |
| `BATCH-001` | `SCRIPT-001`, `SCRIPT-003`, `SCRIPT-005` | Fixed Input/Output and ordered enabled operations; immutable graph, preview/run/save and staged output ownership |
| `BATCH-002` | `SCRIPT-002`, `SCRIPT-005` | Four authorable operations: Color Replace, Move to Color Plane, Masking and Erase; private Batch primitive is not public InkScript |
| `BATCH-003` | `SCRIPT-003`, `SCRIPT-005` | Bounded file/folder/issue-time active input and folder/active/new-tab output, current codecs and owner-thread publication |
| `BATCH-004` | `SCRIPT-002`, `SCRIPT-003`, `SCRIPT-004`, `SCRIPT-005` | Exact-depth Color/Raster role or fixed-ID targets, deduplication, atomic movement and fill-protection replacement in one canonical transaction |

## Current machine-readable ownership

[`owner-manifest-v7.json`](../schemas/inkscript/owner-manifest-v7.json) assigns all 74
current command owners exactly once. The owner IDs are stable registry metadata:

| Owner | Replayable primitives | Scope |
| --- | ---: | --- |
| M07 | 6 | property, plane-format conversion and document transforms |
| M08 | 7 | legacy fill, replacement, separation, filter, boundary-airbrush, dust and raster line correction adapters |
| M15 | 13 | paper/frame and remaining document tree operations |
| M16 | 8 | color metadata and guide/grid operations |
| M17 | 3 | raster stroke, geometry and canonical raster import |
| M18A | 1 | gradient |
| M18B | 9 | gesture effects, alpha and scoped color operations |
| M19 | 13 | selection, saved masks, selected-source restoration and floating selection |
| M21 | 1 | shooting frame |
| M22 | 13 | replayable Light Table set/item operations |

`LIGHT_TABLE_SWAP_WITH_ACTIVE` is excluded because it is session-only, and the private
`APPLY_BATCH_OPERATIONS` procedure is excluded because `.inkbatch` v5 owns that route.
Query, view, preview, export, save/open, history-control, and frontend
command IDs remain outside the manifest. The journal-fragment query therefore remains
outside the 74-command catalog while exhaustively consuming its typed runtime variants.

## Executable evidence

- `inkscript_registry` checks registry/language/catalog versions and closed signatures,
  owner bijection, approved exclusions, generated reference equality, requirement
  coverage and absence of retired public routes.
- Public journal-fragment contracts cover active/inactive one-or-linear Commit
  selection, non-Commit/nonlinear rejection, exact parent assertions, strict binding,
  typed scalar/list results, deduplicated retained assets, bounded resources, Cancel,
  source atomicity and owned Send/Sync output. Family fixtures replay into a cache-free
  parent and compare complete canonical procedures, state/pixel digests and ID authority.
- FFI contracts and C11/C++20/header/export checks cover source/compiler/fragment
  ownership, two-stage UTF-8 copy, strided diagnostics, defaults/overrides, old-version,
  NULL/alignment/size/flag/enum/span negatives, stale tokens, wrong thread and release.
- Execution/report contracts cover authority-free PathIntent, copied grants, fixed DTO
  callbacks without Core locks, immutable plans, confirmation, PlanTask/RunTask flow,
  cancellation, atomic install and detached reports. Success reopens current output
  and verifies cache-free replay, Undo/Redo, both savepoints and source nonmutation.
- `line_correction_codec_preserves_all_modes_background_and_brush_options` and
  `line_corrections_export_execute_and_replay_the_same_canonical_edit` cover
  `apply_line_correction` (INKS-EQ-0089), exact canonical edits and exported replay.
  Independent Core/image pixel tests are also required, not just executor equivalence.

Current product availability and unverified configurations remain in
[compatibility.md](compatibility.md); private API evidence is not product acceptance.
