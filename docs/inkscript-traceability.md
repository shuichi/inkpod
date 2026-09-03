# InkScript traceability

This document is the M00 traceability authority connecting the product requirements in
[`SPEC.md`](../SPEC.md), the normative language contract in
[`INKSCRIPT.md`](../INKSCRIPT.md), the machine-readable registries, milestones, and
planned evidence. It is not a command reference. The exact-current command signatures are in
[`catalog-v7.json`](../schemas/inkscript/catalog-v7.json); the derived presentation is
[`inkscript-command-reference.md`](inkscript-command-reference.md).

## Ratified contract

M00's starting contract was explicitly accepted before M01 implementation. M21's
selector correction, the M27 drawing-model rebaseline, the Batch v5 version
rebaseline, and the layer/plane contract rebaseline were separately approved before
implementation:

- InkScript registry schema/language/file version 2, procedure catalog and owner
  manifest version 7, replay epoch 29, `.inkpod` top-level version 34, and C ABI
  version 34 are the exact-current values. Catalog/owner v7 expose 74 public
  commands; the private `ApplyBatchOperations` canonical procedure is not an InkScript command.
  ABI v31 changes only application-owned validated-sidecar cache control/telemetry and does not
  change InkScript syntax, catalog entries, procedure semantics, replay, or native payloads.
- M07's approved exact-current registry schema v2 supersedes registry schema v1 solely to add
  closed catalog-owned enum, record, and constructor definitions; schema v1 is not accepted.
- M21's approved option A changed only the shooting-frame selector from a layer owner to the
  document-owned singleton required by SPEC/Core. Its file/catalog-v2 result is historical;
  M27 retains file/language/schema v2 while superseding catalog/owner v2 with v3.
- `schemas/inkscript/language-v2.json` freezes the command-independent grammar and
  section syntax. A grammar or section change requires an explicit InkScript file-version
  decision. Closed semantic entities and references may change only with an explicit
  procedure-catalog/replay rebaseline such as catalog v5; they are not silent fixes.
- Catalog v2 was the 84-command production contract after M23. The M27
  drawing-model rebaseline retires nine commands, reserves their assignments as
  tombstones, and replaced it with catalog/owner v3 and a 75-way bijection. The Batch v3
  rebaseline advanced the catalog/owner envelope to v4. The layer/plane rebaseline retires
  `convert_layer`, both adjustment-layer commands, both selection-layer commands, and
  `edit_vanishing_points`; those primitive IDs remain tombstones and cannot be reused.
  It adds the four document-owned saved-selection-mask commands, producing catalog/owner
  v5 and a 73-way bijection. Older catalog/owner resources are rejected; generated
  reference and fingerprints are regenerated from v6. The user-approved Cut removal also removes the shooting-frame instruction-export field, advancing its schema/semantics to 3/2 and catalog/owner to v6 without changing the 73-command count or language/file v2.
- Exact-source equivalence compares canonical state/pixel digests, ID high-watermarks,
  typed result roles and output ordinals, pre/post state digests, stable input/output/asset
  roles, and canonical invocations. Rebound execution guarantees deterministic execution
  only after every external strict selector is explicitly replaced; it does not claim
  source UUID, raw ID, or source-state equality.
- M24's journal exporter is a read-only orchestration query, not a catalog procedure. It maps
  the already frozen typed runtime invocation and retained asset authority back to an
  exact-current semantic fragment, emits exact parent state/ID-allocation assertions and
  strict selectors, and reuses the same canonical executor for equivalence. It changes no
  file grammar, command signature, primitive semantics, replay epoch, native schema, or ABI.
- M25 exposes source parse/diagnostic copy/static compile/journal fragment export through C ABI
  v15. The FFI only translates versioned fixed-width records, bounded spans, and opaque ownership
  into the public Rust APIs; private catalog models and executors remain unreachable, and no
  Windows command, file filter, clipboard, plan execution, report, or product UI route is added.
- Continuous Fill lowers one source seed to one step and therefore produces zero through
  N ordinary Commits. A contiguous, non-semantic `editor_group` is the only lossless 1:N
  link back to one legacy Batch operation; it cannot alter execution or Commit boundaries.
- M13 must present the complete InkScript quick/full workload, semantic counters,
  checksums, samples, environment, and proposed envelope and stop at `[~]`. M14 may change
  the benchmark harness or envelope only after that explicit approval. Existing workload,
  counter, `revision-max`, and envelope contracts are unchanged by M00.
- The Batch v5 product decision supersedes the earlier `.inkbatch` v2 shadow-parity
  cutover plan. `.inkbatch` v5 is the independent product authority and is editable in
  the Batch pane. It lowers its closed four-operation graph directly to a private typed
  canonical procedure. One Color Replace operation may own a bounded set of semantic
  layer selectors, which is resolved to exact stable IDs before that single procedure is
  committed. No `.inkscript` file filter, clipboard payload, or Batch
  InkScript authoring UI is added.

## Batch parity matrix

Entries named `INKS-TRACE-*` and `INKS-PARITY-*` are stable planned evidence IDs, not
passing tests in M00. M00's actual registry tests are listed in the next section.

| Existing requirement | InkScript requirements | Owning milestones | Required planned evidence |
| --- | --- | --- | --- |
| `BATCH-001` | `SCRIPT-001`, `SCRIPT-003`, `SCRIPT-005` | Batch v5 | `.inkbatch` v5 owns fixed Input/Output, ordered enabled operations, bounded Color Replace target sets, immutable preview/run/save construction and staged output ownership without exposing InkScript |
| `BATCH-002` | `SCRIPT-002`, `SCRIPT-005` | Batch v5 | The public Batch catalog is exactly Color Replace, Move to Color Plane, Masking and Erase; retained legacy Core primitives are not authorable through Batch or InkScript |
| `BATCH-003` | `SCRIPT-003`, `SCRIPT-005` | Batch v5 | File/folder/issue-time active input and folder/issue-time active/new-tab output reuse native/common-raster codecs, bounded path planning and owner-thread staged results |
| `BATCH-004` | `SCRIPT-002`, `SCRIPT-003`, `SCRIPT-004`, `SCRIPT-005` | Batch v5 | Exact-depth colors, one-operation multi-layer Color Replace, atomic multi-plane movement and sparse fill-protection replacement use one private canonical procedure and one transaction |

Any future request to expose Batch primitives through InkScript must make a new explicit
catalog/file-version decision and prove input/output authority, canonical procedure,
state/composite digest, history, Undo/Redo, IDs, save/reopen, cache-free replay and failure
atomicity parity. Batch v5 itself does not imply that product route.

## Current machine-readable ownership

[`owner-manifest-v7.json`](../schemas/inkscript/owner-manifest-v7.json) assigns all 74
current command owners exactly once. The allocation is deliberately
reviewable before signatures are added:

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
command IDs remain outside the manifest. M24's journal-fragment query therefore remains
outside the 74-command catalog while exhaustively consuming its typed runtime variants.

## Registry and completeness evidence

The `inkscript_registry` integration target provides these current checks:

- `inkscript_registry_meta_schema_and_production_catalog_are_closed`
- `inkscript_registry_json_rejects_duplicate_malformed_and_overflowing_input`
- `inkscript_language_core_is_closed_and_references_resolve`
- `inkscript_owner_manifest_is_a_bijection_with_replayable_primitives`
- `inkscript_production_catalog_is_bijective_with_runtime_and_equivalence_evidence`
- `inkscript_generated_command_reference_has_no_drift`
- `inkscript_versions_and_traceability_match_repository_contracts`
- `inkscript_removed_draft_is_unreachable_from_production`
- `inkscript_private_typed_models_remain_unreachable_from_core_ffi_and_windows`

PowerShell `Test-Json -SchemaFile` additionally validates the language, production catalog, and owner
documents against the formal Draft 2020-12 meta-schema. The Rust tests validate the closed
meta-schema and JSON syntax, duplicate/malformed/overflow rejection, language type/nonterminal
references, unique field/order/type names, current version constants, the 74-entry
primitive-owner bijection, the two explicit exclusions, requirement/document
drift, immutable catalog fingerprint, generated-reference drift, public Rust ownership, absence of
the removed draft, and absence of private typed catalog models or Windows InkScript routes.

## M24 journal-fragment evidence

Six public exporter contracts plus one inline-asset unit contract cover active and inactive
one/linear Commit selection, Genesis, non-Commit/nonlinear rejection, exact parent assertions,
strict and strict-source-only binding, typed scalar/list result references, retained asset
deduplication, caller-lowered resource bounds, cancellation, source atomicity, and owned
`Send + Sync` output. Seven M17–M22 family fixtures additionally apply the emitted fragment to a
cache-free parent and compare final state/pixel digest, ID high-watermark, pre/post digest,
schema-ordered input/output/asset IDs, and the full canonical procedure sequence. The export
replay path does not materialize history visualization summaries or thumbnails.

## M25 C ABI evidence

Three `inkpod-ffi` contracts plus the C header/export drift, C11 include, C++20 ABI smoke, and
route-inventory gates cover the historical ABI-v15 source/compiler/export boundary. They prove
bounded source ownership, two-stage original-text and canonical-fragment copy, atomic strided
diagnostic records with packed UTF-8, stored-default/typed-override static compilation through the public Rust compiler, and
journal export through the M24 public exporter. Negative evidence covers v14 rejection, NULL,
misalignment, short records, unknown flags and enums, oversize spans, Cancel, resource limits,
stale controller/session tokens, stale Core generation, wrong-thread access, and idempotent NULL
release. No CST/AST/catalog node, per-token/per-node call, second model/executor, execution/report,
or Windows product route is exposed.

## M26 execution/report C ABI evidence

Three `inkpod-ffi` execution contracts plus the C header/export drift, C11 include, C++20 ABI
layout smoke, route inventory, and exact-old-version rejection cover the historical ABI-v16 PathIntent, copied
authority grants, fixed DTO host callbacks, immutable plan/preview, one-shot confirmation,
PlanTask/RunTask event flow, cancellation, atomic native install, and detached batched reports.
Success evidence reopens the current-v32 output, verifies cache-free full replay, Undo/Redo,
document/editor savepoints, history, state digest, and ID high-watermark while proving the input
Core is unchanged. Negative evidence covers v15 rejection, NULL, short/unknown records, queue
saturation, cancellation, stale authority/confirmation and save failure. The ABI delegates to the
existing M11/M12 planner/runner, invokes callbacks without a Core lock, and adds no Windows
authority adapter, command, UI, product route, parser/catalog node, or second executor.

## Raster line correction (approved 2026-09-03)

Catalog/owner v7 adds `apply_line_correction` (INKS-EQ-0089) and explicitly records native background,
gesture construction and independent width modes. The original v6 rebaseline above is historical.
`line_correction_codec_preserves_all_modes_background_and_brush_options` checks canonical encoding;
`line_corrections_export_execute_and_replay_the_same_canonical_edit` exercises live edits, fragment
export, compilation, execution and replay against exact canonical procedures and state.
The public line-selection suite covers native save/reopen and one-step Undo/Redo as well.
